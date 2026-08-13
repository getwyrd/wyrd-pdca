# Result — issue 710 / ceiling-refused-placement-writes-do-not-certify

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: 
- Success criterion: the NEW file `crates/custodian/tests/placement_ceiling.rs` passes,
  driven **only** through symbols visible on the base — `wyrd_custodian::{reconcile_step,
  Custodian, FencedZone, ReconstructionContext, RebalanceContext, Reconciled}`,
  `wyrd_custodian::desired_state::{set_lifecycle, DServerLifecycle, reconciliation_status,
  ReconciliationStatus}`, `wyrd_core::repair::{enqueue_repair, queued_repairs, repair_key}`,
  `wyrd_core::metadata::{inode_key, encode, decode, MAX_VALUE_BYTES, ChunkMap, InodeRecord,
  ChunkRef, EcScheme}` — over in-memory `MetadataStore` / `ChunkStore` doubles. Three legs:
  1. **A repoint that would cross the value ceiling is refused, not persisted.** Hand-seed a
     committed **flat** root whose encoded length is just under `MAX_VALUE_BYTES`, holding a chunk
     placed on small-id D servers, and arrange a repair whose new placement uses large `u64` ids
     so the re-encoded record crosses the ceiling. Assert: the record is **byte-identical**
     afterwards, the obligation **stays queued**, the pass does **not** answer `Satisfied`, and the
     refusal is named on the audit seam. Base behaviour: the oversized record is committed (the CAS
     has no ceiling check), so `get(inode_key)` returns bytes whose length **exceeds
     `MAX_VALUE_BYTES`** → **red**.
     **Assert the stored byte length, not a downstream un-repairability**: an in-memory
     `MetadataStore` double has no value ceiling and will happily hold the oversized value, so "the
     object is now un-repairable" is *not* observable through it. Do **not** copy the parent
     brief's optional two-phase demo ("commit once, then a second repair fails"): with a
     ceiling-enforcing double the *first* crossing write is already refused by the store, so that
     sequence demonstrates nothing — the two store models cannot share one narrative. The coherent
     supplementary leg, if wanted: over a ceiling-enforcing double, the base's crossing repair
     surfaces as a raw backend `Err` out of `reconcile_step` (unclassified, indistinguishable from
     a transient fault), while post-fix the pass returns cleanly with the obligation queued and
     the refusal named — assert that contrast. The binding assertion either way is the stored
     length over the unlimited double.
  2. **An evacuation that did not persist does not certify.** With `set_lifecycle(.., Draining)`
     on a server holding a fragment, arrange an evacuation that cannot proceed (refused by the
     ceiling, or aborted for want of a free distinct failure domain). Assert the fragment is still
     on the draining server and the pass MUST NOT answer `Satisfied` while `reconciliation_status`
     for that server is not converged. Base: the silent `Aborted => {}` arm lets the loop answer
     `Satisfied` → **red**. This leg **flips a deliberately pinned base assertion**:
     `crates/custodian/tests/rebalance.rs:940-975` seeds exactly this shape (no free distinct
     domain) and asserts `Satisfied` ("no move: collapsing the chunk's spread would violate
     durability"). Rewriting that pin is sanctioned — #696's brief defers the certification
     question to #682 — and is why `tests/rebalance.rs` is this child's **named** fifth file, not
     an option. No new public variant is needed: `Reconciled::Blocked` already exists on base
     (`crates/custodian/src/reconciliation.rs:44`, and it outranks `Changed`), so the
     discriminator asserts `!matches!(.., Satisfied)` with base symbols only.
  3. **A refused move is subtracted, never counted as a success.** Assert the documented
     `repaired − conflict − aborted` identity holds over a pass mixing one repaired, one refused
     and one aborted chunk — a refused repair must not inflate reported successes. The counters
     are observable through in-memory doubles: existing tests already read the
     `monotonic_counter`s back via the telemetry subscriber and compute exactly this identity
     (`crates/custodian/tests/reconstruction.rs:1939-1945`). (This leg is **not** independently
     discriminating: on the base the would-be-refused repair simply commits its oversized record
     and *correctly* counts as a success, so the identity holds there too — it is red only as a
     derivative of leg (1). It ships because it pins the accounting rule for the new outcome —
     the same discount this proposal applies to child-2's leg (3).)
  Legs (1) and (2) are the discriminating reds — both red against the current `origin/main`
  (`339da46`) without #696/#697, and both surviving their merge (#696 defers the `Aborted` arm to
  #682; neither PR adds a ceiling check). Leg (3) is binding but derivative.
- Repo + branch target: getwyrd/wyrd @ main   (INTEGRATION §2: single slice; no live milestone
  integration branch — M4's is merged and deleted, and every #635 slice so far landed on `main`
  directly.)
- Scope (one logical fix) / out of scope: 

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: Fixed
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (5 test(s) ran red).
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 20 mutants tested in 43s: 11 caught, 9 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_710/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — scripts/pdca contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review task: Refuse oversized flat repair/evacuation placement writes without persisting anything, and ensure non-persisted moves neither certify convergence nor inflate success accounting.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief fixes a concrete operator-safety failure with bounded flat-path scope, observable refusal/accounting outcomes, and the backend ceiling already defined at `crates/core/src/metadata.rs:324`. |
| C2 Reproduction (red pre-fix) | PASS | The isolated base compiled the base-visible test unchanged and failed 4 of 5 tests on oversized persistence and false certification, while the exact-ceiling control at `crates/custodian/tests/placement_ceiling.rs:430` passed. |
| C3 Change | PASS | The patch enforces the existing full-value ceiling and the no-certification rule at both binding flat write paths without touching segmented/read-side behavior (`crates/core/src/metadata.rs:380`, `crates/custodian/src/reconstruction.rs:915`, `crates/custodian/src/rebalance.rs:513`). |
| C4 Verification (red→green) | PASS | Independently compiled evidence is red on the base and 5/5 green with the patch; typos, docs render, fmt, clippy, build/tests, machete, all deny graphs, conformance, statics, orchestrator scan, and DST also pass, with the read-only global deny lock rerun from scratch (`crates/custodian/tests/placement_ceiling.rs:308`). |
| C5 Causal adequacy | PASS | Tests pin the legal boundary, transient-over-permanent compound-failure precedence, byte-identical no-write behavior, and metric offsets, and an independent 20-mutant run caught 11 with 9 unviable and none surviving (`crates/custodian/tests/placement_ceiling.rs:430`, `crates/custodian/tests/placement_ceiling.rs:453`, `crates/custodian/tests/placement_ceiling.rs:477`). |
| T1 Structure | PASS | One core ceiling helper feeds crate-private repair/evacuation outcomes, keeping policy at the metadata boundary and certification/accounting in the custodian loops (`crates/core/src/metadata.rs:356`, `crates/custodian/src/reconstruction.rs:794`, `crates/custodian/src/rebalance.rs:386`). |
| T2 Shape | PASS | The diff is confined to the five named files with no dependency or out-of-scope surface, and the prior human explicitly waived the semantic-line size backstop, so no remaining shape decision is owed. |
| T3 Runtime | PASS | Both paths resolve transient prerequisites first, weigh the exact encoded record, return before any fragment/metadata write on refusal, then preserve the existing CAS and prior object metadata on legal writes (`crates/custodian/src/reconstruction.rs:853`, `crates/custodian/src/reconstruction.rs:923`, `crates/custodian/src/rebalance.rs:457`, `crates/custodian/src/rebalance.rs:522`). |
| T4 Contribution | NEEDS-HUMAN | Confirm closed/rejected-work coverage and contribution linkage — merged history was checked by each affected path, but `scripts/review-branch`, `scripts/pdca`, and a mechanically searchable closed/rejected-work index are absent from the target, so the asserted T4 passes remain provisional. |
| T5 Judgment | PASS | Treating aborts, refusals, and lost-CAS evacuations as non-certifying matches the operator fact that the draining server remains referenced, while the tests preserve distinct audit outcomes (`crates/custodian/src/rebalance.rs:150`, `crates/custodian/tests/rebalance.rs:1339`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the in-memory red→green and audit-seam evidence is sufficient operational proof for the tightest production backend — no live ceiling-enforcing backend was exercised, so deployment fitness remains a sign-off judgment (`crates/custodian/tests/placement_ceiling.rs:322`). |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] T4 Contribution — Confirm closed/rejected-work coverage and contribution linkage — merged history was checked by each affected path, but `scripts/review-branch`, `scripts/pdca`, and a mechanically searchable closed/rejected-work index are absent from the target, so the asserted T4 passes remain provisional.
- [x] Validation — fitness-to-purpose — Decide whether the in-memory red→green and audit-seam evidence is sufficient operational proof for the tightest production backend — no live ceiling-enforcing backend was exercised, so deployment fitness remains a sign-off judgment (`crates/custodian/tests/placement_ceiling.rs:322`).
- [x] size backstop — this slice is behaving oversized: 3 round(s) already spent (threshold 2). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.
- [x] T5 Judgment — Decide whether lost-CAS conflicts are intentionally excluded from “a move that did not persist” — the existing test still requires `Satisfied` while the drain remains `Pending`, which changes the operator-certification meaning at `crates/custodian/tests/rebalance.rs:1343` and `crates/custodian/tests/rebalance.rs:1376`.

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
- By / date: Eduard Ralph / 2026-08-09

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
