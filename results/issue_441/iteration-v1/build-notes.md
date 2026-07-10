# Build notes — issue 441 / fdb-packaging-and-version-coupling

Withheld from the reviewer by the driver; for the human at sign-off.

## What shipped

Against `getwyrd/wyrd @ main` (`b1ccca3`, `$PDCA_WORKTREE` = `/home/eddie/wyrd/wyrd.pdca-wt-l1`):

1. **`pub mod preflight`** (`crates/metadata-fdb/src/lib.rs:835-1015` in the final tree,
   inserted before `mod store`) — a third pure, non-feature-gated sibling to `classify`
   and `config`. `ClientStatus`, `Verdict`, `verdict()`, `message()`, exactly the shapes
   the brief's Design 1 names. Own `#[cfg(test)] mod tests` (4 cases) mirroring
   `classify`/`config`'s shape, per the Citations field.
2. **`crates/metadata-fdb/tests/preflight.rs`** (new file, 179 lines) — the brief's named
   test file. 6 tests: the three real fixture-table rows (skew/unreachable/healthy) → the
   three `Verdict` arms (assertion 3), the `VersionSkew`/`Unreachable` message-content
   checks (assertion 2), and one fail-honest ambiguous-status case.
3. **`config::EXTERNAL_CLIENT_DIR_ENV` + `external_client_dir()`** (`:392-399`,
   `:444-450`) — the `WYRD_FDB_EXTERNAL_CLIENT_DIR` env var, peer-shaped to
   `CLUSTER_FILE_ENV`/`cluster_file()`.
4. **`ensure_network()` rewritten** (`:1109-1148`) onto the `FdbApiBuilder` →
   `NetworkBuilder::set_option(NetworkOption::ExternalClientDirectory)` → `.boot()` path,
   preserving the `#[allow(unsafe_code)]` + SAFETY comment shape at the one call the crate
   permits (`:126-130` doc, unchanged).
5. **`FdbMetadataStore::preflight()` + `client_status()`** (`:1277-1315`, `:1158-1225`) —
   the bounded readiness probe `connect()` now runs before returning `Ok`, wired into
   `crates/server/src/cli.rs:168`'s `open_fdb_meta` (unmodified — the seam was already
   there per the brief's Production reach field).
6. **`docs/design/architecture/07-deployment-view.md` §7.6** (new section, 58 lines) — the
   packaging contract: container path, bare-metal path, version coupling, the
   multi-version upgrade dance, a manual repro (verified live — see below), and the
   single-binary trade. No ADR touched.
7. `serde_json` added to `metadata-fdb`'s already-optional `fdb` feature (not a new
   dependency to the workspace — pinned in root `Cargo.toml:126`, already used by
   `wyrd-core`/`wyrd-server`).

## Why this shape, what I tried, what I ruled out

**Single-shot probe → bounded poll loop (the one real design change from my first
draft).** My first implementation called `Database::get_client_status()` **once** and fed
the result straight to `preflight::verdict`. Unit tests (which construct `ClientStatus`
fixtures directly) all passed immediately, and `cargo xtask fdb-conformance`'s existing
`--lib`/`conformance`/`contention`/`scan`/`timeout` legs all passed too — none of them
calls `connect()` (they all use `open()`, which has no probe). So the whole "the code
compiles and the existing feature-gated suite stays green" signal was clean, and it would
have been easy to call that "done" and ship a probe that fails on every real connect.

Per the refutation discipline below, I did not stop at green-unit-tests — I called
`FdbMetadataStore::connect()` for real against the live `deploy/fdb-single-node` cluster
(a throwaway example binary, not shipped). Result: `connect()` failed with `Unreachable`
against a cluster that `fdbcli` on the same host confirmed was healthy. Root cause,
confirmed by dumping the raw JSON in a loop: `Database::new()` returns before the client
has dialed anything, so `Connections[0].Status` reads `"connecting"` for the first
~0.2–2s (observed; matches the design doc's own "both settle within ~2s" note, which I'd
read as being about the *skew/unreachable* distinction, not about needing to wait at
all). A single call almost always lands in that window, so a literal reading of Design
item 3 ("performs a bounded probe") would have shipped a `connect()` that fails on nearly
every real, healthy connection — the opposite of the brief's Goal (a).

Fix: `preflight()` now polls `get_client_status()` at 100ms intervals (`STATUS_POLL_INTERVAL`,
justified against the observed ~0.2–2s settle time) until a *settled* status is parsed
(`client_status()` now treats `"connecting"` as `None`, same as unparsable — see
`:1163-1176`) or the caller's deadline elapses. This is still a single bounded probe from
`connect()`'s perspective — never slower than `self.timeout_ms` — just not a single
*call*. I considered instead lengthening a fixed initial sleep before the one call, but
that either wastes time on a cluster that resolves fast (skew/healthy both did, ~100ms in
my repro) or is still wrong on a slow network — polling with a deadline dominates that on
both axes at the cost of ~15 extra lines.

**A second real bug caught the same way:** `tokio::time::timeout(...)` constructed as an
eager argument to `runtime.block_on(...)` panics ("no reactor running") because
`Timeout`/`Sleep` register with the runtime's timer driver at construction time, before
`block_on` has entered the runtime. Fixed by moving construction inside the `async move`
block passed to `block_on` (`:1291-1310`). Also only caught by actually running it.

**Cost of the alternative I rejected (bake the guard into `store`, feature-gated).** The
brief's own Alternatives section already makes this call on a checkable basis
(`xtask/src/main.rs:1255-1264` lists only `tikv` in `feature_gated_checks()`), so I didn't
re-litigate it — I note it here only because the poll-loop fix above stayed entirely
inside the already-feature-gated `store` module and did not touch the pure `preflight`
module's public shape, so the checkable-RED property the brief establishes for
`preflight` is unaffected by the fix.

**`ClientStatus.coordinators_reachable` naming.** The brief's design doc explicitly warns
that "zero reachable coordinators" (the `Coordinators` list) is the *wrong* signal — it
stays populated under skew. I read `coordinators_reachable` as the *connection* signal
(`Status == "connected"`), not the coordinator-list signal, and verified this against the
three live JSON shapes I captured myself (see below) — it discriminates skew/healthy
(`"connected"`) from unreachable (`"failed"`) exactly as the design doc's table shows.

## Manual verification beyond what the brief required

The brief scopes the live, whole-system red (a mismatched cluster) as **deferred**, named
to #470. This host has Docker + a pullable `foundationdb/foundationdb:7.1.61` image (the
brief's own "Supplementary legs" field said so), so I ran it anyway, against the actual
`FdbMetadataStore::connect()` production path (not a copy):

| Cluster | `connect()` result | Time |
|---|---|---|
| `foundationdb:7.3.77` (matches client) | `Ok(_)` | ~103ms |
| `foundationdb:7.1.61` (skew) | `Err("...protocol version mismatch ... fdb00b071010000 ... multi-version ...")` | ~103ms |
| nothing listening | `Err("...unreachable after waiting 3.001592404s...")` | 3.0s (the configured deadline) |

All three match the brief's Design 1 fixture table and the shipped test's expectations.
This is strictly more than the brief asked for at Check (`xtask fdb-conformance`'s `--lib`
leg only *compiles* the call site); I did it because it directly falsified my first
(buggy) implementation and I wanted the same live check to confirm the fix. The doc's
manual-repro section (§7.6) was rewritten to the exact command I ran (`--network host`,
matching `deploy/fdb-single-node/docker-compose.yml`'s own note that a bridge port-mapping
does not give a host client the address FDB advertises) and marked "verified live on this
host".

I did **not** ship this live run as an automated test — it needs Docker + a second FDB
image, outside the brief's binding criterion, and #470 is where it becomes a real gate.

## `cargo xtask fdb-conformance` (supplementary leg, run twice — before and after the poll fix)

Both runs: `--lib`, `conformance`, `contention`, `scan`, `timeout` all pass against the
throwaway `deploy/fdb-single-node` cluster. `xtask fdb-conformance: FoundationDB passed
the shared MetadataStore conformance suite and the contention properties`. (The `--lib`
leg only compiles the new call site under `--features fdb`, since none of these four test
binaries call `connect()` — they all use `open()`, which is unchanged. The live table
above is what actually exercises `connect()`/`preflight()`.)

## `cargo xtask ci` (C4-ci, gating)

Green, full run: `cargo fmt --all -- --check`, `cargo clippy --workspace --exclude
wyrd-dst --all-targets`, `cargo build`, `cargo test --workspace --exclude wyrd-dst`
(includes the default-build `wyrd-metadata-fdb` unit tests, `tests/preflight.rs`, and the
existing conformance/contention/scan/timeout binaries' clean-skip legs), `cargo-machete`
(clean — `serde_json` correctly listed under `[package.metadata.cargo-machete] ignored`),
`cargo deny check`, `xtask statics`, `xtask deploy-guard`, `cargo test -p wyrd-dst
--cfg madsim`. No FDB feature enabled anywhere in this run — confirms the brief's "stays
green on a machine with no FDB" claim structurally, not just by inspection.

Also ran `typos` (a separate, always-on CI job per `typos.toml`, not part of `xtask ci`)
over every touched file — caught and fixed `unparseable` → `unparsable` (5 occurrences in
`lib.rs`, 1 in `preflight.rs`) before it could fail commit/CI.

Formatter: `cargo fmt --all` run over the whole workspace before finalizing; `cargo fmt
--all -- --check` is clean.

## Refutation (forced, per the Do beat's own checklist)

**(a) Genuine red?** Yes — proven twice, once by hand and once by the project's own
`engine/scripts/run-verify.sh` (the C4-verify gate), run against this bundle's actual
`patch.diff` in an isolated `../wyrd-verify-l1` worktree off `origin/main`:

```
run-verify.sh: GREEN — cargo test -p wyrd-metadata-fdb --test preflight (fix applied)
... 6 passed ...
run-verify.sh: RED — cargo test -p wyrd-metadata-fdb --test preflight (production reverted, test kept)
error[E0432]: unresolved import `wyrd_metadata_fdb::preflight`
   ...
error: could not compile `wyrd-metadata-fdb` (test "preflight") due to 1 previous error
run-verify.sh: PASS — red without the fix, green with it.
```

This matches the brief's Falsifiability field exactly: reverting `src/lib.rs` removes
`preflight`, and the kept test fails to *compile* (not just fails an assertion) — the
strongest form of red the brief predicted.

I additionally did the brief's named **demonstrated-red negation**, by hand, not shipped:
flipped `status.cluster_protocol.is_some()` to `.is_none()` in `verdict`'s `VersionSkew`
arm (`crates/metadata-fdb/src/lib.rs:911-912` in the final tree) and re-ran `cargo test -p
wyrd-metadata-fdb --test preflight`. Result: `skew_fixture_is_version_skew` and
`an_ambiguous_status_degrades_to_unreachable_never_a_guessed_skew` both fail (2 of 6 red,
the other 4 — which don't exercise that arm — stay green, as expected). Reverted the
negation; all 6 green again. This is assertion 3's own falsifiability, separate from (and
in addition to) the module-existence red above.

**(b) Production path?** Yes. `tests/preflight.rs` calls `wyrd_metadata_fdb::preflight::{verdict,
message}` directly — the real functions, not a copy. And beyond the shipped test:
`FdbMetadataStore::connect()` — the exact function `crates/server/src/cli.rs:168`'s
`open_fdb_meta` calls in production — was driven live against three real clusters (table
above), through the real `preflight()`/`client_status()`/`ensure_network()` I wrote, with
no stand-in.

**(c) Fixture includes the fault?** Yes. `tests/preflight.rs`'s three fixtures are
hand-transcribed from the brief's own Design 1 table, which the brief states are captured
JSON from a *live* `libfdb_c` 7.3.77 against three real clusters (skew, unreachable,
healthy) — not invented inputs. Assertion 3 requires exactly this. Beyond the shipped
test, my live verification table above used the actual faulty topology (a real 7.1.61
cluster, a real absent listener) against the real production `connect()`, not a
healthy-only fixture that excludes the fault.

## External dependencies

None beyond the base Rust toolchain for the binding criterion (as the brief states).
Docker + `libfdb_c` 7.3.77 + `fdbcli` 7.3.77 + a pullable `foundationdb/foundationdb:7.1.61`
image were all present on this host and used for the supplementary/manual verification
above; none of that is required for the shipped `patch.diff` + `tests/preflight.rs` to go
red→green, and I did not use any dependency the brief didn't already name as present.
`serde_json` is a workspace-pinned dependency already used elsewhere (root `Cargo.toml:126`,
`crates/core`, `crates/server`) — new to `metadata-fdb`'s own graph, not to the tree, and
gated by the already-optional `fdb` feature; this is not a NEEDS-HUMAN new-dependency item
under INTEGRATION §4's framing (that framing — which I did not open, only the brief's own
citation of it — is about a dependency new to the *workspace*, e.g. `tikv-client`/`etcd-client`,
both of which the root `Cargo.toml` flags explicitly as such; `serde_json` carries no such
flag).

## Scope discipline

Did not touch: ADR-0014, ADR-0042 (frozen, brief says MUST NOT), `docs/design/README.md`
(brief says MUST NOT), the doctor rows (#439), deploy profiles (#469), the fault battery
(#442), or the `wyrd:fdb` image (#470). Read only `brief.md` plus the five cited peer
callsites (`crates/metadata-fdb/src/lib.rs:141` `classify`, `:382` `config`,
`:853-869` `ensure_network`/`NETWORK`, `:898`/`:911` `connect`/`open`, `:416`
`DEFAULT_TRANSACTION_TIMEOUT_MS`, `crates/server/src/cli.rs:168` `open_fdb_meta`) plus the
vendored `foundationdb-0.10.0` crate source (to verify `FdbApiBuilder`/`NetworkBuilder`/
`NetworkOption::ExternalClientDirectory`/`Database::get_client_status` signatures cited in
the brief's Design section actually match what's in this workspace's `Cargo.lock` —
verification, not new scope) and the generated `foundationdb`'s `OUT_DIR/options.rs` (to
confirm the exact `NetworkOption::ExternalClientDirectory(String)` shape and option code
63, matching `/usr/include/foundationdb/fdb.options:114`).

## Open questions (unchanged from the brief, not mine to resolve)

Both of the brief's Open Questions (superseding-ADR-or-not; doc page placement) are
answered by the brief's own defaults (no ADR change; extend §7 as §7.6), which I followed.
Not re-litigated here — flagged only so sign-off knows they were pre-decided by Plan, not
silently dropped by Do.
