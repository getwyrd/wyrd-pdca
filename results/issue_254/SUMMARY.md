# Result — issue 254 / native-prefix-scan-read-consistency

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: 
- Success criterion: a prefix that spans **more than one internal page** returns the **complete** set of matching `(key, value)` pairs — never a silently truncated subset — observed as a single consistent cut; when the interim cap is exceeded the call returns **`Err`** (with an operator-visible ADR-0011 audit signal), **never a partial `Vec`**; the shared `MetadataStore` conformance suite (#252) still passes for both backends and no consumer's order-independence regresses; and the module carries a read-consistency contract doc stating the fresh-TSO-snapshot-per-call / consistent-cut-across-pages semantics and why `rename`'s read-then-commit is safe (the commit precondition re-check under the locking rule, not read freshness). `cargo xtask ci` stays green on a machine with **no** TiKV (the tikv module + the endpoint-gated at-scale test skip cleanly); the behavioural at-scale proof runs under `cargo xtask tikv-conformance` against the throwaway `deploy/` single-node TiKV. **BINDING:** completeness-or-fail-loud + one-consistent-snapshot-per-`scan` + the documented contract. **ILLUSTRATIVE:** the exact paging mechanism, page size, and cap value are Do's call within the invariant.
- Repo + branch target: getwyrd/wyrd @ `feat/m4-production-metadata-backend`  (INTEGRATION §2 — M4 slices target the integration branch, NOT `main`; feature branch `feat/m4.3-native-prefix-scan`; commit subject `feat(metadata-tikv): … (M4.3, #254)`)
- Scope (one logical fix) / out of scope: the one logical change is proposal 0015 §"Suggested PR sequence" item 3 — a native, internally-paged prefix scan with documented read consistency:

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix (implement — accepted-plan feature slice behind Accepted ADR-0008/ADR-0015; no new ADR is minted).
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: fail — run-verify.sh: FAIL — the test PASSES without the fix, so it does not catch the bug (no red).
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

# Check review — issue 254 / native-prefix-scan-read-consistency

**Task under review:** replace M4.1's single unpaged `txn.scan(range, u32::MAX)` in `crates/metadata-tikv/src/lib.rs` with an *internally-paged* prefix scan that (a) holds **one** `begin_pessimistic` snapshot across all pages (the #261 consistent cut), (b) materializes the complete owned `Vec`, (c) **fails loud** (`Err`, never a partial `Vec`) when an interim cap is breached (#262 / ADR-0011), and (d) documents the `get`/`scan` read-consistency contract — trait unchanged.

Grounded against target source `/home/eddie/wyrd/wyrd.pdca-wt` (patch applies cleanly; target file matches `patch.diff`). `check-gates.json`: `C4-ci` = pass, `C4-verify` = fail (non-gating), overall = pass.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Brief is a plan-pointer to accepted proposal 0015 §"Native prefix scan" + #261/#262; binding invariant (complete-set-or-fail-loud + one-snapshot-per-scan + contract doc) is concrete and testable. Nothing to re-derive. |
| C2 Reproduction (red pre-fix) | NEEDS-HUMAN | At-scale red is **deferred/endpoint-gated** — `tests/scan.rs:394` skips with no `WYRD_TIKV_PD_ENDPOINTS`, so the completeness/truncation repro is not observable at Check. Decision owed: confirm the `cargo xtask tikv-conformance` red against `deploy/` single-node TiKV (pre-declared #146 sign-off item); build-notes withheld from reviewer, so the recorded demonstrated-red cannot be re-checked here. |
| C3 Change | PASS | Paged loop under **one** `begin_pessimistic` txn (`lib.rs:493-536`): cursor advances via `paging::after_page`/`next_page_start`; cap-breach returns `Err(ScanCapExceeded)` with a `txn.rollback()` first (drop-safety, `lib.rs:521-531`). Matches the binding invariant; order stays unspecified. |
| C4 Verification (red→green) | NEEDS-HUMAN | `C4-ci` PASS = whole-tree compile + non-tikv lib tests green (the `paging` unit tests `lib.rs:226-296` ran green). `C4-verify` FAIL is the **pre-declared** endpoint-gated artifact (the skipping `scan.rs` "passes" without the fix) — NOT a real regression and non-gating; do not read it as a blocking verification failure. Decision owed: confirm the deferred at-scale red→green via `xtask tikv-conformance` (set sizes / page count / cap-breach), since `tikv-client 0.4 txn.scan` sort-order + limit semantics that the cursor-advance relies on are exercised only there. |
| C5 Causal adequacy | PASS | Root cause (one unbounded read, no paging, no completeness guard, undocumented snapshot) is **removed/transformed**, not guarded — the single-shot read is replaced by a paged loop. Symptom-guard smell-test does **not** trip: the fail-loud cap is a data-loss correctness guard, and the `#[cfg(feature="tikv")]` / endpoint gates predate this slice (M4.1 posture), not a capability-probe papering over a load-time side effect. |
| T1 Structure | PASS | Paging/cap **decision** logic factored into a new dependency-free `pub mod paging` (`lib.rs:126`) mirroring `keyspace` — pure, unit-tested on every machine; store `scan` merely drives it. Clean separation. |
| T2 Shape | PASS | `MetadataStore` trait byte-for-byte untouched (patch touches only `metadata-tikv/src/lib.rs`, `tests/scan.rs`, `xtask/src/main.rs`; `crates/traits` unmodified) — no paginated/streaming method, materialized `Vec` preserved; cap breach surfaces as a downcast-able typed `ScanCapExceeded` → `BoxError` (`lib.rs:154-175,527`). |
| T3 Runtime | PASS | The Check-observable unit exercised: `paging` unit tests (cursor-advance, short-page termination, cap-checked-before-termination, operator-visible error) passed under `C4-ci`. At-scale runtime is deferred (folds into the C4 NEEDS-HUMAN). |
| T4 Contribution | NEEDS-HUMAN | Own tests genuinely contribute: `tests/scan.rs` at-scale completeness proof + `xtask` wired to run the `scan` binary (`xtask/src/main.rs:502`). **But scope item (d)** — wiring #419's three shared properties (read-after-commit / rename-race / scan-is-consistent-cut) into `metadata-tikv/tests/conformance.rs` — is **absent**, and I confirmed those property fns do **not** exist in `crates/metadata-conformance/src/lib.rs`: **#419 has not landed in this base.** Correct for the builder not to call non-existent fns. Decision owed (`Depends on: 419`): ship #254 ahead of #419 — accepting no shared executable pin for the consistent-cut invariant at Check (rests on the endpoint-gated `scan.rs`) — or hold for #419. |
| T5 Judgment | NEEDS-HUMAN | Three Do-choices need human confirmation per the brief's enumerated items: (1) `SCAN_CAP = 1<<20` (`lib.rs:145`) may be a **product-facing** "max dirents per listing", not pure implementation (#262 "a correctness constraint, not a tuning knob"); (2) the ADR-0011 operator-visible audit signal is left **caller-side** (store returns descriptive `Err`, GC/custodian emits) — confirm this satisfies #262 without a new tracing dep (ADR-0003); (3) the read-consistency contract doc (`lib.rs:301-329`) restates an accepted decision (#261, ADR-0015 Option C) — doc-of-a-decision is human-confirmable (INTEGRATION §4). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Whether the paged scan, as shipped, actually protects GC's never-reclaim safety set at production scale is only provable against real TiKV (`xtask tikv-conformance`), which is deferred at Check. Human confirms fitness-to-purpose at sign-off, together with the STOP-discipline draft-only posture (brief §"STOP discipline"). |

## Notes
- Target state: `#419`'s conformance properties are not in this base (verified: no matching fns in `crates/metadata-conformance/src/lib.rs`). This is the expected trailing-prerequisite state for a `Depends on`/verification-level dependency, **not** a patch defect — the paged-scan code compiles and `C4-ci` is green without #419.
- No blocking (gating) failure re-derived: the only FAIL row in `check-gates.json` (`C4-verify`) is explicitly non-gating and pre-declared as the endpoint-gated deferral; presenting it as a blocking C4 failure would fabricate a blocker for a patch that in fact compiles and passes CI.

### Advisory — codex

- NEEDS-HUMAN — `crates/metadata-tikv/src/lib.rs:145` hard-codes `SCAN_CAP` to `1 << 20`; this is potentially a product-visible maximum listing size, so sign-off should confirm the cap value and where it is documented.
- NEEDS-HUMAN — `crates/metadata-tikv/src/lib.rs:147` makes the cap breach a descriptive typed `Err`, with the audit signal delegated to callers; sign-off should confirm that caller-side surfacing satisfies the ADR-0011 / #262 “operator-visible audit signal” requirement without adding store-level telemetry.
- NEEDS-HUMAN — `crates/metadata-tikv/tests/scan.rs:75` scans only after all writes have committed, so the new live test proves multi-page completeness but not the one-snapshot-across-pages invariant under concurrent mutation; this aligns with the deferred #419 posture, but should be explicitly accepted alongside the `C4-verify` no-red result.
- `crates/metadata-tikv/tests/scan.rs:57` uses only the process id in the live-test namespace. That is fine for the throwaway `xtask tikv-conformance` volume, but repeated runs against a long-lived external TiKV can collide with stale keys after PID reuse; reuse the timestamp-based fresh namespace pattern from `tests/contention.rs` to make the test robust.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C2 Reproduction (red pre-fix) — At-scale red is **deferred/endpoint-gated** — `tests/scan.rs:394` skips with no `WYRD_TIKV_PD_ENDPOINTS`, so the completeness/truncation repro is not observable at Check. Decision owed: confirm the `cargo xtask tikv-conformance` red against `deploy/` single-node TiKV (pre-declared #146 sign-off item); build-notes withheld from reviewer, so the recorded demonstrated-red cannot be re-checked here.
- [x] C4 Verification (red→green) — `C4-ci` PASS = whole-tree compile + non-tikv lib tests green (the `paging` unit tests `lib.rs:226-296` ran green). `C4-verify` FAIL is the **pre-declared** endpoint-gated artifact (the skipping `scan.rs` "passes" without the fix) — NOT a real regression and non-gating; do not read it as a blocking verification failure. Decision owed: confirm the deferred at-scale red→green via `xtask tikv-conformance` (set sizes / page count / cap-breach), since `tikv-client 0.4 txn.scan` sort-order + limit semantics that the cursor-advance relies on are exercised only there.
- [x] T3 Runtime
- [x] T4 Contribution — Own tests genuinely contribute: `tests/scan.rs` at-scale completeness proof + `xtask` wired to run the `scan` binary (`xtask/src/main.rs:502`). **But scope item (d)** — wiring #419's three shared properties (read-after-commit / rename-race / scan-is-consistent-cut) into `metadata-tikv/tests/conformance.rs` — is **absent**, and I confirmed those property fns do **not** exist in `crates/metadata-conformance/src/lib.rs`: **#419 has not landed in this base.** Correct for the builder not to call non-existent fns. Decision owed (`Depends on: 419`): ship #254 ahead of #419 — accepting no shared executable pin for the consistent-cut invariant at Check (rests on the endpoint-gated `scan.rs`) — or hold for #419.
- [x] T5 Judgment — Three Do-choices need human confirmation per the brief's enumerated items: (1) `SCAN_CAP = 1<<20` (`lib.rs:145`) may be a **product-facing** "max dirents per listing", not pure implementation (#262 "a correctness constraint, not a tuning knob"); (2) the ADR-0011 operator-visible audit signal is left **caller-side** (store returns descriptive `Err`, GC/custodian emits) — confirm this satisfies #262 without a new tracing dep (ADR-0003); (3) the read-consistency contract doc (`lib.rs:301-329`) restates an accepted decision (#261, ADR-0015 Option C) — doc-of-a-decision is human-confirmable (INTEGRATION §4).
- [x] Validation — fitness-to-purpose — Whether the paged scan, as shipped, actually protects GC's never-reclaim safety set at production scale is only provable against real TiKV (`xtask tikv-conformance`), which is deferred at Check. Human confirms fitness-to-purpose at sign-off, together with the STOP-discipline draft-only posture (brief §"STOP discipline").
- [x] `crates/metadata-tikv/src/lib.rs:145` hard-codes `SCAN_CAP` to `1 << 20`; this is potentially a product-visible maximum listing size, so sign-off should confirm the cap value and where it is documented.
- [x] `crates/metadata-tikv/src/lib.rs:147` makes the cap breach a descriptive typed `Err`, with the audit signal delegated to callers; sign-off should confirm that caller-side surfacing satisfies the ADR-0011 / #262 “operator-visible audit signal” requirement without adding store-level telemetry.
- [x] `crates/metadata-tikv/tests/scan.rs:75` scans only after all writes have committed, so the new live test proves multi-page completeness but not the one-snapshot-across-pages invariant under concurrent mutation; this aligns with the deferred #419 posture, but should be explicitly accepted alongside the `C4-verify` no-red result.

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
- Consistent-cut-under-concurrent-mutation pin is gated on #419 (runnable on today's single-node TiKV); region-boundary paging and true-distribution scenarios wait for M4.5 (#256) / M4.7 DST (#258). Revisit #254's scan at those tiers.
