# Check review — issue #255 / m4.4-server-backend-selection

**Task under review:** the M4 composition change in `crates/server` — let the metadata backend be
chosen by config (redb dev / TiKV prod) instead of `server` hard-coding `RedbMetadataStore`, by
parameterizing the `cli.rs` helpers over `M: MetadataStore`, adding a `redb|tikv` selector, moving
the local-disk paths onto the shared tokio runtime, and bounding `alloc_inode`'s retry spin.

**Grounding note.** Target `$PDCA_TARGET` = `/home/eddie/wyrd/wyrd.pdca-wt`, patch already applied
and consistent with `patch.diff` (constants `cli.rs:57-59`, generic `alloc_inode` `cli.rs:515`) — no
staleness caveat. Sandbox blocked direct `cargo`/`git` invocation, so C4 is grounded on the gate
re-runs recorded in `check-gates.json` plus source-level verification of the red/green mechanism; all
other rows are re-derived from the target source read-only.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Spec is proposal 0015 slice 4's three couplings (config selector, tokio runtime, bounded `alloc_inode`); all three are addressed. No spec ambiguity for the redb (Check) path — the tikv deferral is explicitly scoped in brief.md:11. |
| C2 Reproduction (red pre-fix) | PASS | Pre-fix `alloc_inode` was an unbounded `loop` (target retains the shape only through the applied fix); the new test `alloc_inode_is_bounded_against_a_perpetual_conflict_store` (`tests/backend_selection.rs:77-81`) hangs→times-out pre-fix and the generic-mock assertions don't compile without the seam. `check-gates.json` C4-verify re-ran `run-verify.sh` → "red without the fix". |
| C3 Change | PASS | Diff confined to 4 files — `Cargo.lock`, `crates/server/Cargo.toml`, `crates/server/src/cli.rs`, new `tests/backend_selection.rs`. `core`/`custodian`/`traits` byte-for-byte untouched; `deny.toml` untouched (scope binding brief.md:15 held). Implements all three couplings. |
| C4 Verification (red→green) | PASS | `check-gates.json`: C4-ci `cargo xtask ci` (fmt/clippy/build/test/deny/conformance) = pass (gating); C4-verify red→green = pass. Mechanism verified in source: bounded loop `cli.rs:515-542`, tokio runtime with `enable_all()` (`tikv`/redb share `tokio_runtime()` `cli.rs:676`). tikv arm is `#[cfg(feature="tikv")]`-gated OUT of the default build by design (brief.md:11) — its compile belongs to `--features tikv`/tikv-conformance, not a Check gap. |
| C5 Causal adequacy | PASS | Fixes root causes, not symptoms: hard-coded concrete → config selector (`cli.rs:67-108`); pollster-bound local path → shared tokio runtime; unbounded `Conflict` spin → bounded backoff that *removes* the cause (`cli.rs:534-539`). Symptom-guard smell-test does NOT fire: `#[cfg(feature="tikv")]` is designed compile-time composition and `from_config`'s tikv-off `Err` (`cli.rs:88-91`) is config validation, not a runtime capability probe papering over a load-time side effect. |
| T1 Structure | PASS | New integration test at the conventional path `crates/server/tests/backend_selection.rs`; two `#[tokio::test]` cases plus a local `AlwaysConflict` mock (`tests/backend_selection.rs:64-79`). |
| T2 Shape | PASS | Assertions are specific: config default/explicit/unknown-name (`:38-49`), monotonic persisted inodes `(1,2)` (`:44-53`), and bounded-error-not-value on perpetual conflict (`:80-85`). No tautologies. |
| T3 Runtime | PASS | Deterministic and self-bounding: in-memory redb (`RedbMetadataStore::in_memory` exists, `metadata-redb/src/lib.rs:36`), `tokio::time::timeout(5s)` safety net (`:79`) turns a regression to unbounded spin into a loud failure not a suite hang; no pollster caller of `alloc_inode` remains, so no reactor-panic path. |
| T4 Contribution | PASS | The mock is constructible ONLY because `alloc_inode`/helpers are generic over `M` — the test load-bears the parameterization seam (revert → won't compile) and pins the bounded-retry contract; it fails pre-fix for two independent reasons. |
| T5 Judgment | PASS | Test judgment is sound for the Check-verifiable (redb) surface. Note for human sign-off: the tikv selection arm (`from_config(Some("tikv"))→Tikv`, `open_tikv_meta` `cli.rs:113-120`) has NO automated Check coverage — it is compiled out of default CI and proven only by maintainer-run `cargo xtask tikv-conformance` (brief.md:11); this deferral is by design but is not test-guarded here. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decision owed: does redb-path CI evidence + a DEFERRED, maintainer-run `cargo xtask tikv-conformance` satisfy the slice DoD "`server` runs identically on redb (dev) and TiKV (prod) chosen by config"? The `tikv` arm never runs in default CI, so the "runs on TiKV" half is unverified at Check. Human must (a) run `cargo xtask tikv-conformance` (needs `deploy/tikv-single-node` docker + `WYRD_TIKV_PD_ENDPOINTS`, host `pkg-config`+`libssl-dev`) and confirm green before merge-readiness, and (b) confirm the prior-art check on `crates/server/src/cli.rs` (merged history + closed/rejected work) — could not be mechanically settled here (git/gh blocked in this sandbox). |
