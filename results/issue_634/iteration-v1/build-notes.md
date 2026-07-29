# Build notes — issue 634 / scan-page-seam

*(Withheld from the reviewer; written for the human at sign-off.)*

Target: `getwyrd/wyrd @ main`, built in `$PDCA_WORKTREE`
(`/home/eddie/development/wyrd/wyrd.pdca-wt`) off `origin/main` @ `22d71b4`.
Every `path:line` below is against that base unless it names a new file.

---

## 1. What was built, and why in that shape

The plan artifact (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:2646-2672`,
read in full before writing code) fixes the signature *and* the four semantic clauses. The
brief settled the two implementer decisions (required method; how 36 in-test doubles absorb
it) and legs A–F. The patch is the union:

| Piece | Where | Note |
|---|---|---|
| `scan_page` (required, no default) + its normative doc contract | `crates/traits/src/lib.rs:776` (after `scan`) | the one narrow-seam widening 0016 authorises |
| `ScanPage`, `ZeroPageLimit`, `page_limit`, `test_double_scan_page` | `crates/traits/src/lib.rs:324` (beside `ScanCapExceeded`, `:288-324`) | seam-crate types so every backend raises/classifies the same ones (#516's rule) |
| native cursored bodies | redb `crates/metadata-redb/src/lib.rs:110` · TiKV `crates/metadata-tikv/src/lib.rs:980` · FDB `crates/metadata-fdb/src/lib.rs:1771` | each over its own range primitive; **none** over `scan()` |
| sim-store bodies + cap knob | `crates/dst/tests/support/mod.rs:291`, `:739` | `BTreeMap` range slice — the faithful model shape |
| 6 `contract_scan_page_*` clauses + `run_all` / `run_all_cap_lowered` | `crates/metadata-conformance/src/lib.rs:428-441` | one runner, no per-driver clause list |
| 34 delegating one-liners | 26 test files | `wyrd_traits::test_double_scan_page(self, …)` |
| **new** `crates/metadata-redb/tests/scan_page.rs` | legs B + C | cap escape + page bound on a real backend |
| **new** `crates/metadata-conformance/tests/scan_page_demonstrated_red.rs` | legs A + D | four violating doubles + one conforming one |

Three design points worth the human's eye:

**a. `next` is derived from page fullness, never from an `items.last()` on an empty page.**
Every backend computes `next = if items.len() == limit { items.last() } else { None }`. That
makes "an empty page is always terminal" *structural* rather than asserted — the shape that
makes a drain loop forever cannot be produced even by a degenerate cap.

**b. The cursor is the range primitive's own exclusive bound, not successor arithmetic**,
wherever the backend offers one: redb `Bound::Excluded` (`metadata-redb/src/lib.rs`), FDB
`KeySelector::first_greater_than`, the sim stores `Bound::Excluded`. TiKV has no exclusive
bound, so it reuses the successor helper the peer callsite already agreed on
(`paging::next_page_start`, `crates/metadata-tikv/src/lib.rs:435-440`) — the brief's
"re-exposure of this machinery, not a new one".

**c. Every backend clamps the cursor up to the prefix start** (`max(prefix, after)`), so an
`after` below the range cannot walk a caller out of the namespace it asked for.

---

## 2. The forced refutation (the three questions, answered with evidence)

### (a) Genuine red? — **yes**, four independent ways

1. **The whole fix reverted** (`git stash` of the 36 tracked files, the two new test files
   left in place — exactly what the C4-verify RED leg materializes on the base). Result:
   **build error, 0 tests ran, 0 failed** — `E0432` (unresolved `wyrd_traits::page_limit`,
   `wyrd_traits::ScanPage`), `E0407 × 5` ("method `scan_page` is not a member of trait
   `MetadataStore`"), `E0425 × 13` (no `contract_scan_page_*`, no `LOWERED_SCAN_CAP`).
   **This is a compile-shaped red and I am flagging it as such**, per the brief's
   falsifiability obligation: `engine/scripts/run-verify.sh:416-427` puts the `TESTS_RAN == 0`
   guard inside the cargo-*succeeded* branch, so a build failure falls through to the
   unconditional `PASS — red without the fix` at `:433`. A run reporting zero tests is a
   non-result. The three experiments below are the real evidence.

2. **The rejected design put back** — redb's `scan_page` replaced by a `scan()`-backed
   shim (the #508-v4 shape), everything else unchanged:
   `4 of 7` tests in `crates/metadata-redb/tests/scan_page.rs` **FAILED**, each with
   `ScanCapExceeded { cap: 8 }` — including leg B
   (`a_population_scan_refuses_whole_is_still_walkable_page_by_page`) and the shared
   cap-escape clause driven through `redb_honours_the_shared_scan_page_clauses`. This is the
   brief's "the leg that fails against any `scan()`-backed shim", measured.

3. **The cursor made inclusive** (`Bound::Excluded` → `Bound::Included` in redb):
   `4 of 7` **FAILED**, the shared-clause one at
   `crates/metadata-conformance/src/lib.rs:348` ("a page must start strictly after the
   cursor").

4. **The zero-limit refusal removed** from `page_limit`:
   `2 of 7` redb tests **FAILED** plus `1 of 10` in the demonstrated-red file
   (`a_faithful_paged_store_passes_every_new_clause`).

Restored after each; `git diff --stat` back to the shipped 38 files / 2406 insertions, and
both targets green again (7 + 10 = **17 passing**).

**And the semantic red the brief calls the binding demonstration (leg D):** the four
`#[should_panic]` tests in `scan_page_demonstrated_red.rs` pass, i.e. each clause *catches*
its violating store **by assertion, inside the same `cargo xtask ci` run** — string ordering
→ clause (a); inclusive cursor → (b); `next: None` on a full page → (c); LIMIT/OFFSET paging
→ (d). Each violating store still passes all four pre-existing sequential clauses, so the new
clauses add discriminating power rather than restating the old suite. The OFFSET double
additionally passes the *static-population* clauses (a) and (c), which is what shows (d) is
not a restatement of them: only a mid-walk mutation can see that bug.

### (b) Production path? — **yes**

`crates/metadata-redb/tests/scan_page.rs` drives `RedbMetadataStore::in_memory()` — the real
backend, the real `scan`/`scan_page` bodies — with only the *ceiling* moved by the
established `with_scan_cap` idiom (`crates/metadata-redb/tests/scan.rs:75-89` licenses
exactly this). The conformance clauses run through `run_all` against redb
(`crates/metadata-redb/tests/conformance.rs:28`), against redb + `SimTikvMetadataStore` +
`SimFdbMetadataStore` in-simulator (`crates/dst/tests/conformance.rs`, gating row `run_dst`),
and against real FDB/TiKV clusters in the off-Check jobs. The doubles in
`scan_page_demonstrated_red.rs` are *deliberately wrong* stores whose only job is to prove
the clauses bite; the clause functions they drive are the **same** functions the backends
drive — not copies.

### (c) Fixture includes the fault? — **yes**

The cap-escape fixture seeds `cap × 3 + 1` keys (25 at `LOWERED_SCAN_CAP = 8`) — a population
deliberately **past** the cap and deliberately **not** a multiple of it, and the clause
asserts `scan` fails loud on it *before* walking, so a cap-lowering hook that silently
lowered nothing would fail the clause rather than pass it vacuously. The no-skip fixture
mutates *during* the walk (insert behind the cursor, delete ahead of it, insert ahead of it)
and asserts over the control set that includes every key that was present throughout —
the deleted key is excluded because the contract does not cover it, and that exclusion is
stated in the clause rather than quietly convenient. The order fixture asserts up front that
byte order and lossy-string order actually *differ* for its keys
(`assert_ne!(expected, lossy_order, "the fixture is broken…")`), so the clause cannot pass
because the fixture stopped discriminating.

---

## 3. Verification actually run (all in `$PDCA_WORKTREE`)

| Command | Result |
|---|---|
| `./engine/xtask.sh ci` (= `cargo xtask ci`) | **exit 0, "all checks passed"** — run twice, before and after the final doc-comment reword. Prose gates ran for real (`typos` clean, `lint_docs: OK`, `render_site: link audit OK`), so this is CI parity, not a warn-skip. |
| `cargo test -p wyrd-metadata-redb --test scan_page` | 7 passed |
| `cargo test -p wyrd-metadata-conformance --test scan_page_demonstrated_red` | 10 passed |
| `RUSTFLAGS=--cfg madsim cargo test -p wyrd-dst --test conformance` | 3 passed (redb + both sim stores, incl. the cap-lowered clause) |
| `cargo clippy -p wyrd-metadata-fdb --features fdb --tests` | **clean** |
| `cargo clippy -p wyrd-server --features fdb,etcd --tests` | **clean** |
| `cargo clippy -p wyrd-metadata-tikv --features tikv --tests` | **clean** |
| `cargo clippy -p wyrd-server --features tikv,etcd --tests` | **clean** |
| `cargo fmt --all -- --check` | clean (the target's commit hook formatter) |
| `cargo doc --no-deps` (not a gate row) | clean for `metadata-conformance` / `metadata-redb`. `wyrd-traits` fails on a **pre-existing** ambiguity at `crates/traits/src/lib.rs:12` (`[`async_trait`]` is both a crate and an attribute macro) — present on the base, untouched by this patch, and `cargo doc` is not in `run_ci`. Left alone as out of scope. |
| `./engine/xtask.sh fdb-conformance` (live Docker cluster) | **exit 0** — `trait_contract_against_fdb ... ok`, plus `--lib` (40), `contention` (3), `scan` (2), `timeout` (4). Not a clean skip: the log shows the compose stack coming up and `Database created` before the run. |
| `./engine/xtask.sh tikv-conformance` (live Docker cluster) | **exit 1 — pre-existing, proven** (see below). |

No external dependency was missing: all four feature-gated rows compiled (this host has
`libfdb_c`, the FDB headers and openssl dev, as the brief said), and Docker was up for both
cluster legs. **No `NEEDS-HUMAN external dependency` marker is warranted.**

### The FDB leg is real behavioural evidence, not just compile evidence

The brief classed `metadata-fdb` as *deferred* (compile-only at Check, behavioural at
sign-off). Docker was available, so I ran the deferred leg: **FoundationDB's `scan_page`
passed all five new `run_all` clauses and the cap-escape clause via `run_all_cap_lowered`,
against a live `fdbserver`** — including the `with_scan_cap`-lowered walk over a population
that backend's own `scan` refuses whole. That closes the FDB half of the deferral. The
sign-off item that remains for FDB is only "is this the posture you want", not "does it work".

### The TiKV leg is red — and it is red on the **pristine base** too (measured, not assumed)

`xtask tikv-conformance` fails at the **first** clause, `contract_commit_and_get`, before any
`scan_page` code runs:

```
called `Result::unwrap()` on an `Err` value: PessimisticLockError { … abort:
"Error(InvalidKeyMode { cmd: acquire_pessimistic_lock, storage_api_version: V2, key: … })" }
```

I did not assume that was pre-existing. I reset the worktree to the pristine base
(`git checkout -- .` with the two new files removed — `git status` clean), re-ran the same
job, and got the **identical** failure at `crates/metadata-conformance/src/lib.rs:32` — the
same assertion my patch shifts to `:33` by adding exactly one `use` line. Then I restored the
tree with `git apply patch.diff` and re-verified (`fmt --check` clean, 7 + 10 tests green,
diffstat back to 38 files / 2406 insertions).

Diagnosis: the throwaway `deploy/` TiKV runs with `storage.api-version = 2` while
tikv-client 0.4 writes V1-mode keys — a deployment/client mismatch in the `deploy/` stack,
unrelated to #634 and unrelated to `scan_page`. It sits with the crate's already-recorded
state: retained fallback on a **build-only** CI bar (#443), Tier-1 battery independently red
(#537). Worth a tracker issue of its own; **not** this slice's to fix, and not something to
"work around" by weakening the shared suite (which would violate the invariant the suite
exists to enforce). TiKV's `scan_page` therefore has compile evidence + the shared clause
wired and ready, but **no behavioural green** — the one genuinely unproven backend.

---

## 4. Alternatives considered, with their costs

**A default `scan_page` body over `scan()`** — rejected, and this is the one rejection with a
measured number on both sides. It would have saved the **272 lines** of mechanical churn in
the test doubles (34 impls × 8 lines) and left `crates/dst/tests/support/mod.rs` alone. What
it costs is detectability: measured in refutation experiment 2 above, a `scan()`-backed body
fails 4 of 7 redb tests *only because redb exposes `with_scan_cap`* — the conformance suite
cannot lower a cap through the trait seam, so on a backend that inherited the default
silently there is no clause that could fire at all. 272 lines of identical one-liners in
`tests/` is a cheap price for "no production backend can inherit the cap it exists to
escape", and it is the recorded reason #508's 4th attempt was rejected.

**A second closure parameter on `run_all`** (rather than the separate `run_all_cap_lowered`)
— rejected on call-site cost: `run_all` has 6 call sites across 4 driver files, and every one
would have to supply a cap-lowering factory, including any future backend that has no cap
knob (TiKV had none before this patch). The separate runner leaves all 6 `run_all` call sites
byte-identical and adds one line per driver; a new cap-scoped clause added inside it is still
picked up everywhere with no per-driver list, which is the property the brief cares about.

**A `ScanCapExceeded`-style error for `limit == 0` per backend** — rejected for the same
reason #516 consolidated `ScanCapExceeded`: a per-crate look-alike makes the caller's
downcast depend on which store it holds. `ZeroPageLimit` lives in `crates/traits` beside it.

**Opaque continuation token instead of `Some(last_key_returned)`** — not considered; 0016
settles it (`0016:2657-2658`) and the brief records it only so Do does not "improve" it.

**Editing the architecture docs** — checked and not needed: `grep -i scan` over
`docs/design/architecture/` finds only a deployment-view table row and one unrelated
sentence; no paragraph states the store offers only a whole-namespace scan (the brief's
conditional). No ADR/spec/proposal file was touched.

---

## 5. Three judgement calls the human should confirm at sign-off

**(i) I added `with_scan_cap` to `TikvMetadataStore` and to both DST sim stores, and gave the
sim stores' `scan` the production fail-loud behaviour.** The brief's leg E table *assumes*
the hook exists on TiKV ("where `with_scan_cap` lowers the cap the same way") — it did not;
only redb (`crates/metadata-redb/src/lib.rs:77`) and FDB
(`crates/metadata-fdb/src/lib.rs:1331`) had one. The brief also requires the lowered-cap
clause to run on both sim stores at Check via `run_dst`. So rather than declare three of six
implementations unproven, I added the knob:

* TiKV: +19 lines (field, builder, and `scan` now reads `self.scan_cap` instead of the
  constant — default is `paging::SCAN_CAP`, so behaviour at the default is byte-identical).
* Sim stores: +12 lines each (field, builder, and `scan` routed through a `scan_capped`
  helper that raises the seam's `ScanCapExceeded`, which the models previously never raised).

This brushes the brief's "any change to `scan` itself … out of scope". My reading: that line
forbids relaxing `scan`'s *semantics* or the `SCAN_CAP` value, neither of which happens here
(the default cap and the fail-loud rule are unchanged; the sim models moved *toward* the
production error semantics the rubric's *test fidelity* row asks for). If you disagree, the
minimal alternative is to drop the sim-store knob and the TiKV knob and accept that the
cap-escape clause runs on redb only — say so and I will cut it.

**(ii) TiKV's `scan_page` has no behavioural green, and cannot get one on this host.** All
four feature-gated clippy rows are clean, and I went past the brief's requirement and ran
both live-cluster legs: **FDB passed** (§3), **TiKV fails on the base as well as on the
patch**, at the first clause, on a `storage_api_version: V2` key-mode mismatch in the
`deploy/` stack. So TiKV's implementation is compile-verified and its conformance driver is
wired (`crates/metadata-tikv/tests/conformance.rs`, both runners) — but nothing has executed
it. That is the one honest gap in this slice; it is pre-existing environment rot rather than
anything #634 introduced, and the alternative (weakening or forking the suite so TiKV passes)
is precisely what the brief forbids. Please decide at sign-off whether it needs its own
tracker issue.

**(iii) Scope of X90.** Not claimed here (the brief assigns the `orphan:` pagination *scale*
case to #637). No consumer switches to `scan_page` in this slice — by design; the seam lands
ahead of #636/#637.

---

## 6. A defect I caught in my own clause during self-review (recorded, because it is the
## interesting one)

The first draft of clause (d) ended with a loop asserting that even the *unconstrained* keys
(inserted behind the cursor, inserted ahead of it, deleted mid-walk) were "returned at most
once". That was wrong: `0016:2659-2661` says such keys "may be missed **or duplicated**", so
the assertion tightened the contract past its source and would have rejected a conforming
backend that reads a fresh snapshot per page — the exact failure mode the brief warns about
for the inserted-ahead case. It passed on all six implementations here, which is precisely
why it was dangerous: a latent over-constraint in a *conformance* suite is a future backend's
false red. Removed, and replaced with a comment saying the absence of an assertion **is** the
clause, so a later reader does not re-add it. `cargo xtask ci` re-run green afterwards
(third full run, exit 0).

## 7. Things I deliberately did **not** do

* No consumer rewiring (`crates/custodian/src/gc.rs:322` still uses `scan`) — #637's slice.
* No change to `scan`'s signature, its cap value, or its fail-loud rule.
* No ADR / spec / proposal edit (`0016` included) — automatic NEEDS-HUMAN territory.
* No PR opened, nothing pushed, nothing marked ready.
