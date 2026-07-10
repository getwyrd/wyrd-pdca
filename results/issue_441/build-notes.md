# Build notes — issue 441 / fdb-packaging-and-version-coupling (iteration 2)

Target branch: `getwyrd/wyrd @ main`, base `b1ccca3` (== `origin/main` at build time).
All edits made in `$PDCA_WORKTREE` = `/home/eddie/wyrd/wyrd.pdca-wt-l1`.
Line citations below are **post-patch** unless marked `origin/main`.

---

## 1. What iteration 1 got wrong, and what changed

The carry-forward is precise and correct, so I did not re-plan. The v1 patch built
`FdbMetadataStore::preflight()` as a **synchronous** method that constructed its own
current-thread Tokio runtime and called `Runtime::block_on`. Every production caller of
`open_fdb_meta()` is already inside a runtime, so every `wyrd … --metadata-backend fdb`
invocation panicked with *"Cannot start a runtime from within a runtime"* (exit 101).

**The fix is the seam, not a guard.** `connect()` and `preflight()` are now `async fn`s that
await on the **caller's** runtime — the shape `open_tikv_meta` already uses
(`crates/server/src/cli.rs:147`). No runtime is constructed anywhere in the driver.

| | iteration 1 | this patch |
|---|---|---|
| `FdbMetadataStore::connect` | `pub fn` | `pub async fn` (`crates/metadata-fdb/src/lib.rs:1250`) |
| `FdbMetadataStore::preflight` | `fn`, owns a `Runtime`, `block_on` | `pub async fn`, no runtime, no `block_on` (`:1291`) |
| `open_fdb_meta` | `fn` | `async fn` (`crates/server/src/cli.rs:175`) |
| its 7 call sites | `open_fdb_meta()?` | `open_fdb_meta().await?` (`cli.rs:383, 469, 850, 1490, 1496, 1561, 1609`) |
| `cli.rs:165` doc comment | *"`connect()` is synchronous (unlike `open_tikv_meta`'s `.await`)"* | rewritten (`cli.rs:167-173`) — that sentence was the trap |

The polling loop's semantics are unchanged (re-poll until settled or deadline); only runtime
ownership moved. `open()` (`:1326`) stays probe-free, as the brief requires, so
`tests/timeout.rs`, `scan.rs`, `contention.rs`, `conformance.rs` keep their semantics.

**Coverage that would have caught it** (the carry-forward's explicit ask). The pure
`preflight` unit tests cannot see a runtime bug, and `cargo xtask ci` never compiles the
`fdb` feature — that is exactly why v1's green gates meant nothing. Two new feature-gated
cases, both run by `cargo xtask fdb-conformance`, drive the probe **from inside a Tokio
runtime**:

- `crates/metadata-fdb/tests/timeout.rs::preflight_against_an_unreachable_cluster_is_err_not_a_panic`
  — drives production `preflight()` inside `guarded()` (a real multi-thread runtime) against
  the file's existing unreachable-coordinator fixture. Needs no cluster, no Docker, no env
  mutation.
- `crates/metadata-fdb/tests/conformance.rs::connect_probes_the_real_cluster_from_inside_a_runtime`
  — drives the **whole production entry point** `connect()` (env resolution → `Database::new`
  → `preflight().await` → `Ok`) inside `runtime.block_on`, against the live 7.3.77 cluster
  `xtask fdb-conformance` brings up.

Neither mutates the environment: `connect()` reads `WYRD_FDB_CLUSTER_FILE`, which the harness
already exports. I deliberately avoided `std::env::set_var` — there is not one call of it in
`crates/` today, and mutating env while sibling test threads run is a race I did not want to
introduce to buy a test.

## 2. Refuting my own test (the three forced questions)

**(a) Genuine red? — YES, demonstrated three ways, each actually run.**

1. *The brief's named test, fix reverted.* `git stash push -- crates/metadata-fdb/src/lib.rs`
   (production file back to `origin/main`), test kept:
   ```
   error[E0432]: unresolved import `wyrd_metadata_fdb::preflight`
     --> crates/metadata-fdb/tests/preflight.rs:25:24
   error: could not compile `wyrd-metadata-fdb` (test "preflight")
   EXIT=101
   ```
   This is assertions 1–2 failing by non-existence, exactly as the brief's Falsifiability
   field predicts.

2. *The brief's requested demonstrated red — negate the skew arm.* Changed
   `status.cluster_protocol.is_some()` → `.is_none()` in `preflight::verdict` (`:909`):
   ```
   test skew_fixture_is_version_skew ... FAILED
   test an_ambiguous_status_degrades_to_unreachable_never_a_guessed_skew ... FAILED
   test result: FAILED. 4 passed; 2 failed
   ```
   Assertion 3 is load-bearing, not decorative. Restored afterwards.

3. *The carry-forward's defect, reproduced as a test failure.* I re-introduced v1's exact
   shape (sync `preflight()` + `Runtime::block_on`) and ran the new timeout case:
   ```
   test preflight_against_an_unreachable_cluster_is_err_not_a_panic ... FAILED
   thread '…' panicked at crates/metadata-fdb/src/lib.rs:1296:34:
   Cannot start a runtime from within a runtime. This happens because a function
   (like `block_on`) attempted to block the current thread while the thread is being
   used to drive asynchronous tasks.
   ```
   The bug that shipped past every gate in iteration 1 is now a red test. Restored afterwards.

**(b) Production path? — YES.** No mock, no stand-in, no re-implementation anywhere.

- `tests/preflight.rs` imports `wyrd_metadata_fdb::preflight` — the production module the
  patch adds — and calls the production `verdict` / `message`.
- `tests/timeout.rs` calls production `FdbMetadataStore::preflight()` on a real `Database`.
- `tests/conformance.rs` calls production `FdbMetadataStore::connect()`.
- Beyond the tests, I ran the **actual `wyrd` binary** built `--features fdb` against real
  clusters (§3). `crates/server/src/cli.rs:175` → `connect()` → `preflight()` is the path
  every `wyrd … --metadata-backend fdb` invocation takes.

The one seam worth naming honestly: `tests/preflight.rs`'s three fixtures are *reduced*
`ClientStatus` values, not raw JSON, because the JSON reducer (`store::client_status`,
`:1165`) is feature-gated and cannot compile in the default build — which is the whole reason
the brief demanded a non-feature-gated pure module. That reduction is **not** left unverified:
§3's live skew run proves `client_status` produces exactly the `skew_fixture()` shape from
real 7.1.61 JSON, and negating its `Compatible` arm changes the real output (§3, run C).

**(c) Fixture includes the fault? — YES, and I checked that it does rather than assuming.**

The `skew_fixture` is the *failing* element: `Compatible: false` on a `"connected"`
connection, carrying the **cluster's** `fdb00b071010000`. It is not curated to be easy —
`healthy: false` there too, and `unreachable_fixture` also has `healthy: false`, so a
classifier that keyed on `Healthy` alone would pass one and fail the other. The
`an_ambiguous_status_degrades_to_unreachable_never_a_guessed_skew` case is the deliberately
adversarial row (connected + unhealthy + no protocol version) that a naive "unhealthy ⇒ skew"
rule would misclassify.

I also verified the fault is present in the *live* path, not just the fixtures: I stood up an
actually-mismatched `foundationdb:7.1.61` cluster and ran the real binary against it (§3).
The nemesis is injected, not described.

One negative result I want on record, because it looked like evidence and is not: flipping
`client_status`'s `Compatible` arm does **not** turn
`connect_probes_the_real_cluster_from_inside_a_runtime` red. A healthy cluster satisfies
`verdict`'s first arm (`healthy && coordinators_reachable ⇒ Ready`) before `cluster_protocol`
is ever consulted, so that test does not bind the `Compatible` arm. The live skew run does
(§3, run C). I checked this rather than claiming the healthy test covered it.

## 3. Live validation actually performed (not described — run)

Host: `libfdb_c.so` 7.3.77, `fdbcli` 7.3.77, Docker + compose available.

**A. `cargo xtask fdb-conformance`** (the brief's "Do SHOULD run it and record the output"):
```
running 2 tests
test trait_contract_against_fdb ... ok
test connect_probes_the_real_cluster_from_inside_a_runtime ... ok
…
running 4 tests
test preflight_against_an_unreachable_cluster_is_err_not_a_panic ... ok
test get_against_an_unreachable_cluster_fails_rather_than_hanging ... ok
test a_blind_commit_that_times_out_is_an_unknown_result ... ok
test a_conditional_commit_that_times_out_is_never_a_conflict ... ok
…
xtask fdb-conformance: FoundationDB passed the shared MetadataStore conformance suite and
the contention properties
```

**B. The live version-skew red — the run the brief deferred to #470, which turned out to be
runnable here.** `foundationdb/foundationdb:7.1.61` pulled fine, so I ran it rather than
merely documenting it. Real `wyrd` binary, `--features fdb`, real mismatched cluster:
```
$ WYRD_FDB_CLUSTER_FILE=/tmp/skew.cluster ./target/debug/wyrd put … --metadata-backend fdb
wyrd: fdb backend: …: FoundationDB metadata store: client/cluster protocol version mismatch
— this client is api 730 (fdb-7_3 pin), the cluster reports protocol version
fdb00b071010000. A FoundationDB client cannot talk to a cluster running a different
protocol version, ever: load the cluster's `libfdb_c` into a multi-version external-client
directory and point WYRD_FDB_EXTERNAL_CLIENT_DIR at it, … see
docs/design/architecture/07-deployment-view.md.
exit_code=1  elapsed_ms=203   (panic count in stderr: 0)
```
Exit **1**, not 101; **203 ms**, not a 10 s anonymous timeout; names the **cluster's**
protocol version. This is iteration 1's exact smoke run, now passing.

**C. The `Compatible` arm is load-bearing on real JSON.** Same 7.1.61 cluster, with
`client_status`'s arm negated (`if !compatible { None }`), rebuilt:
```
FoundationDB metadata store: cluster unreachable after waiting 101.884426ms …
reported as unreachable rather than a guessed version skew …
```
The skew diagnosis collapses to `Unreachable`. So the arm the design doc's fixture table
identifies as the discriminator really is the discriminator, on live 7.3.77-vs-7.1.61 JSON.
Restored afterwards.

**D. Fail-honest on a genuinely unreachable cluster** (`x:x@192.0.2.1:4500`, RFC 5737):
reports `cluster unreachable … rather than a guessed version skew`, bounded at ~8 s (inside
the 10 s `DEFAULT_TRANSACTION_TIMEOUT_MS`). Never claims a mismatch. This is the
misdiagnosis-prevention property in the live path.

**E. The probe does not reject healthy production traffic** (the regression risk of putting a
probe in front of every connect). Real binary, real 7.3.77 compose cluster, full round trip:
```
$ wyrd put /tmp/payload --key smoke441 --metadata-backend fdb   → exit 0, 304 ms
  put ok: key=smoke441 inode=1 chunks=1 bytes=6 durability=rs(6,3) version=1
$ wyrd get smoke441 --metadata-backend fdb --out /tmp/h.roundtrip → "hello"
```
The probe costs ~200 ms (one round trip), as §7.6 documents.

All containers torn down (`docker rm -f fdb71`, `compose down -v`).

**F. Gates.** `cargo xtask ci` (the gating `C4-ci`) passes on this machine with the `fdb`
feature off. `cargo fmt --all --check` clean. `cargo test -p wyrd-metadata-fdb --test
preflight` → 6 passed on the default build (no `fdb`, no `libfdb_c`, no Docker) — the brief's
Success criterion command, verbatim.

## 4. Design decisions and what I ruled out

**`serde_json` as an optional dep of `metadata-fdb`, `fdb`-feature-only.**
`get_client_status()` returns JSON bytes; something must parse them. It is already pinned in
the root `Cargo.toml:126` and used by `wyrd-core`/`wyrd-server`, so it is new to this crate's
graph only, and only under an already-optional feature. The default build's dependency set is
byte-identical. `cargo deny` (inside `xtask ci`) passes. This is not the "human-only new
dependency" case INTEGRATION §4 guards — no new crate enters the workspace tree.

**Ruled out: hand-rolling the JSON parse to avoid the dep.** The fields needed are
`Healthy`, `Connections[0].{Status,Compatible,ProtocolVersion}` — nested object + array
indexing. A hand-rolled scanner is ~60–80 lines of string-slicing that must fail *honest* on
every malformed shape, and every one of its bugs converts to a false `VersionSkew` or a
missed one. Cost of the dep: **3 lines** in `Cargo.toml` (feature entry, dep line, machete
ignore) + 1 in `Cargo.lock`, all under `--features fdb`. Not close.

**Ruled out (again, per the brief): calling `fdb_get_client_version()`** for the exact client
version string. It needs a direct `foundationdb-sys` dep and a **second `unsafe` block**,
contradicting `crates/metadata-fdb/src/lib.rs:126-128` ("exactly one `unsafe` block exists in
this crate"). The message names the API version from the safe `get_max_api_version()` plus the
`fdb-7_3` pin, and — the field that actually identifies the mismatch — the **cluster's**
`ProtocolVersion`. Live run B confirms that is enough to diagnose.

**`ensure_network()`'s `unsafe` contract preserved, not deleted.** `foundationdb::boot()` is
replaced by its own inlined body (`FdbApiBuilder::default().build()?` →
`set_option(ExternalClientDirectory)` → `.boot()`) so the network option can be set before
boot. `NetworkBuilder::boot` is `unsafe` too (`foundationdb-0.10.0/src/api.rs:207`), so the
`#[allow(unsafe_code)]` + SAFETY comment carried over verbatim and extended; the `OnceLock`
"exactly one network per process, guard never dropped" contract (`:1094-1102`) is untouched.
`#![deny(unsafe_code)]` (not `forbid`) at `:130` still holds with exactly one opt-in site.

**Ruled out: a new `tests/connect_probe.rs` binary** for the connect() regression. It would
become a second `ADDED_TEST` in `run-verify.sh` (`_is_test_file`, `:69`), and its default-build
form is a clean *skip* — green pre-fix and post-fix — which muddies the red→green signal the
brief spent a whole field securing. Folding the case into the existing `conformance.rs`
(already in `xtask`'s leg list, `xtask/src/main.rs:410-417`) costs 0 new files and 0 xtask
changes.

**Doc: extended `07-deployment-view.md` §7.6** (`status: living`), created no new page, touched
neither `docs/design/README.md` nor ADR-0014/ADR-0042 — per the brief and the sign-off's
"RESOLVED — do not re-litigate" on Open question 1.

## 5. Deferred, declared

The brief pre-declares that the live wrong-version-cluster run is **#470's** to automate, and
that this bundle ships the guard + a documented manual repro. That split stands. What changed
is that the repro is no longer only *documented*: run B above executed it end-to-end on this
host against a real `foundationdb:7.1.61`, and §7.6 records the observed output rather than a
predicted one. #470 still owns automating it against the image it builds.

No external dependency was missing. Nothing was substituted for a compile, a tool, or an
environment.

## 6. Worktree hygiene note (no action needed, but recorded)

While verifying the patch applied to a clean base I ran `git stash push -u` on a tree that had
an intent-to-add entry (`git add -N`), which git refuses; the following `git stash pop` then
hit a conflict, restored `stash@{0}`'s *untracked* files to disk, and correctly **kept the
stash**. I confirmed (i) the stash list is intact at 5 entries, (ii) the two restored files
(`docs/design/proposals/draft/0011-*.md`, `0012-*.md`) were byte-identical to their copies
still inside `stash@{0}^3`, and (iii) `patch.diff` never contained them. I then removed the two
strays to restore the worktree. No stash and no uncommitted work was destroyed; `patch.diff`
is unaffected and reverse-applies exactly against the working tree.
