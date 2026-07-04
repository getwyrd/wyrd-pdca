# Result — issue 255 / m4.4-server-backend-selection

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: 
- Success criterion: `server` runs identically on redb (dev) and TiKV (prod) chosen **by config**, achieved by a **composition change confined to `crates/server`** (BINDING: this slice's diff outside the `metadata-tikv` crate is confined to `server` — `core`/`custodian`/`traits` byte-for-byte untouched by THIS change. Note: `core` already carries `wyrd-metadata-redb` under `[dev-dependencies]` for one restart-regression test — that is pre-existing and dev-only, NOT production redb dependence, per proposal 0015:346-347; the binding claim is "untouched by this diff", not "no redb dep exists anywhere"). Three couplings are removed: the ~7 `cli.rs` helpers are parameterized over `M: MetadataStore` behind a **redb | tikv** config selector; the local paths run on the `tokio` runtime the cluster paths already use; and `alloc_inode` gains **bounded retry-with-backoff** instead of its unbounded `Conflict`-spin. Mechanism names (config enum, selector shape, backoff schedule) are ILLUSTRATIVE — Do's call. **Check-verifiable (binding, at C4-verify):** the parameterized `server` compiles and its redb path passes a red→green regression (roundtrip via the generic helpers on the redb backend; `alloc_inode` returns a bounded error against a perpetual-`Conflict` store rather than spinning). **Deferred (off-Check):** the TiKV-backend green is **NOT** part of `cargo xtask ci` — the `tikv` feature is off by default, so `metadata-tikv` compiles as an empty skeleton and `ci` never touches the `tikv-client` tree (`metadata-tikv/Cargo.toml:11-20`; `xtask/src/main.rs:157` "Not part of run_ci"). It is proven **on-demand** by `cargo xtask tikv-conformance` (brings up `deploy/tikv-single-node` docker, sets `WYRD_TIKV_PD_ENDPOINTS`, rebuilds `--features tikv`, runs the shared conformance suite; host needs `pkg-config`+`libssl-dev`). There is **no automated CI exercising TiKV yet** — that is independent open follow-up #420, NOT a prerequisite of this slice. So at Check the binding evidence is the **redb** path via `cargo xtask ci`; the TiKV proof is a maintainer-run `cargo xtask tikv-conformance`. The tikv selection arm in `server` is itself `#[cfg(feature="tikv")]`-gated, so it is compiled OUT of the default Check build — its compile+run belongs to the `--features tikv` / tikv-conformance job.
- Repo + branch target: getwyrd/wyrd @ `feat/m4-production-metadata-backend`   (the M4 integration branch, INTEGRATION §2; the publisher opens the slice PR against it — do NOT target `main`. Do cuts the slice branch `feat/m4.4-server-backend-selection` off it.)
- Scope (one logical fix) / out of scope: the backend-selection composition change in `server` — parameterize the `cli.rs` metadata helpers over `M: MetadataStore`, add the redb|tikv config selector, move the local paths onto `tokio`, bound `alloc_inode`'s retries with backoff. redb stays the dev default (ADR-0014). The tikv arm sits behind an **OFF-by-default `server` `tikv` feature** that forwards to `metadata-tikv`'s `tikv` feature (mirror `metadata-tikv/Cargo.toml:11-20`), so the default build and `cargo deny` graph are UNCHANGED and `cargo xtask ci` stays green with no TiKV. Bounding `alloc_inode` introduces a new exhaustion error path — thread it through its callers. Preserve existing DST-test determinism (the server `dst_*` tests drive the store via their own `pollster::block_on` and do not route through `cli.rs`'s runtime; reuse the existing `cli.rs:290/518` tokio-runtime pattern). / **out of scope:** the `metadata-tikv` crate's own `commit`/`scan`/prefix-scan implementation (M4.2 #253, M4.3 #254); **both items of #420** — the automated nightly TiKV CI workflow (item 1) AND the `deny.toml` allowlist / tikv-tree deny-audit (item 2). Item 2 is **DEFERRED to #420 by decision** (human, 2026-07-04), even though #420 names M4.4 as its "natural trigger": keeping `tikv` off by default means `cargo deny` in `cargo xtask ci` excludes the feature-off tikv tree, so `ci` stays green without it, and the tree was already adjudicated/approved at #252's sign-off (ADR-0003). This slice therefore does NOT touch `deny.toml`; the `deploy/` TiKV/PD + etcd production stack (M4.5 #256); Jepsen/Tier-1/Tier-2 campaigns (M4.6 #257); DST second-implementation pin (M4.7 #258); any change to the `MetadataStore` trait or its `core`/`custodian` consumers; NamespaceStore-on-TiKV (#265, M10).

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it.
- C5 Causal adequacy: none — reviewer + human sign-off

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 Contribution: none — (no gate configured)
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

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

### Advisory — codex

- No advisory findings.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] Validation — fitness-to-purpose — Decision owed: does redb-path CI evidence + a DEFERRED, maintainer-run `cargo xtask tikv-conformance` satisfy the slice DoD "`server` runs identically on redb (dev) and TiKV (prod) chosen by config"? The `tikv` arm never runs in default CI, so the "runs on TiKV" half is unverified at Check. Human must (a) run `cargo xtask tikv-conformance` (needs `deploy/tikv-single-node` docker + `WYRD_TIKV_PD_ENDPOINTS`, host `pkg-config`+`libssl-dev`) and confirm green before merge-readiness, and (b) confirm the prior-art check on `crates/server/src/cli.rs` (merged history + closed/rejected work) — could not be mechanically settled here (git/gh blocked in this sandbox).

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: merged-wider
- Iteration delta (if iterating):
- By / date: Eduard Ralph / 2026-07-04

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
