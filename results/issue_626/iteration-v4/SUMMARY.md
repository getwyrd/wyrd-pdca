# Result — issue 626 / multipart-commit-protocol

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: The multipart **commit protocol** — what happens *underneath* a
- Success criterion: two legs, both evaluated at Check on the patched tree.
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope: ONE logical change: REWORK draft proposal 0016 (starting from

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: patch touches no Wyrd crate (docs/CI only) — nothing to verify per-fix; the C4-ci gate covers it.
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — INFO Diff changes no Rust source files

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 6 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of issue #626’s draft multipart commit protocol, including bounded staging/reclamation, reaping, restore safety, and segmented chunk-map publication.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance decision is explicit and inspectable: exactly two draft-doc paths plus seven settled protocol decisions, F1–F18 dispositions, computed bounds, and normative #625-before/#508 sequencing; the proposal identifies that scope at `docs/design/proposals/draft/0016-multipart-commit-protocol.md:17`. |
| C2 Reproduction (red pre-fix) | PASS | The target base has neither proposal 0016 nor its index row (`git cat-file HEAD:docs/design/proposals/draft/0016-multipart-commit-protocol.md` and `git show HEAD:docs/design/proposals/README.md` both showed absence), while the patched artifact is present at `docs/design/proposals/draft/0016-multipart-commit-protocol.md:1`; this independently reproduces the artifact-level red, though the rejected iteration-1 judgment artifacts were not supplied for replay. |
| C3 Change | PASS | The scope decision is satisfied: `patch.diff` changes only the index and the new draft (1 and 1,598 added lines), with required draft frontmatter at `docs/design/proposals/draft/0016-multipart-commit-protocol.md:1` and the settlement row at `docs/design/proposals/README.md:32`. |
| C4 Verification (red→green) | NEEDS-HUMAN | Decide whether to accept the rerun’s unrelated flaky whole-tree failure — the docs-specific `typos`, lint, and link audit passed, but `cargo xtask ci` failed once in pre-existing `gateway-s3` test `a_bodyless_response_is_recorded_complete_not_aborted`, which passed immediately when rerun alone, so this run did not independently reproduce a fully green gate. |
| C5 Causal adequacy | FAIL | The admission design must enforce its worst-case aggregate reference-work bound, but the single uniform session counter permits a setting derived from small parts (~104 sessions) while later large-part sessions require ~11 or ~1; distribution-dependent capacity is not enforced by `mpuctl:count`, so the claimed by-construction bound can be exceeded (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:1419`). |
| T1 Structure | PASS | The document follows the proposal section set—Motivation, Design, Alternatives, Graduation criteria, Backward compatibility, and Open questions—and the index exposes it as a draft (`docs/design/proposals/README.md:32`). |
| T2 Shape | FAIL | The human needs a mechanically enforceable capacity shape: either set `MAX_SESSIONS` from the worst legal `MAX_PART_CHUNKS`, reserve weighted reference-work units, or introduce an equivalent aggregate budget; a narrative range based on workload mix does not preserve the maintenance bound (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:1419`). |
| T3 Runtime | N/A | This cycle deliberately delivers design prose and no runtime implementation; executable interleavings and conformance tests are obligations for #625/#508 (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:1441`). |
| T4 Contribution | FAIL | The proposal is not ready to contribute as #508’s settled protocol while its core admission formula can admit aggregate staged-reference work beyond `W_ref`; that risks the custodian-plane halt the design is intended to exclude (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:1419`). |
| T5 Judgment | NEEDS-HUMAN | Decide whether the per-session start CAS and per-chunk session precondition are an acceptable cost for a guaranteed residue bound, because accepting a cheaper best-effort cap can reopen the unbounded-staging safety class (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:1543`); merged history was checked by affected path, but closed/rejected remote work could not be independently settled from the supplied artifacts. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether this draft is fit to govern #625/#508 only after the aggregate-admission defect is resolved and the architecture authority accepts the operational/capacity trade-offs; draft-to-accepted ratification is explicitly a separate governance act (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:39`). |

### Advisory — adversary

# Adversarial review — issue 626, iteration 4 (0016 multipart commit protocol)

Lens: refute the reworked design against the brief's Refutation standard (outcomes a–d absent
from the execution register) and the D-F honest-arithmetic bar. All citations grounded on
`$PDCA_TARGET` (wyrd.pdca-wt @ cd82a29 + this patch); `0016:N` = new-file line N in
`docs/design/proposals/draft/0016-multipart-commit-protocol.md`. Note the deterministic T4 gate
is already red (6 blocking, `check-gates.json`); the findings below are independent of it —
I have not seen `review-batch.md`.

## Refutations that landed

- NEEDS-HUMAN [impl] — **`MAX_PART_CHUNKS` contradicts the doc's own knob rule — the capacity
  arithmetic is computed at a forbidden value, and the max-part-size ceiling is never stated
  (leg B(iv)).** The knob table (`0016:695`) binds `max_chunkref_bytes × MAX_PART_CHUNKS ≤ V/2`,
  i.e. `MAX_PART_CHUNKS ≤ 165–381` (same `b_ref` = 131–302 B as `0016:640-641`) — a `part:`
  record is one JSON value under the 100 KB ceiling. Yet the `W_ref`/`MAX_SESSIONS` arithmetic
  (`0016:943-945`, register `0016:1419`) is computed at `MAX_PART_CHUNKS = 5,120` ("a 5 GiB
  part") — 13–31× outside the knob's own valid range — and the narrative repeatedly treats a
  5 GiB part at 1 MiB chunks as admissible (`0016:633`, `0016:839`). Consequence if the knob
  rule is real: at the default 1 MiB chunk (`crates/server/src/lib.rs:51`) the maximum part is
  **165–381 MiB**; an S3-legal 5 GiB part cannot commit its part record (over-`V` value = a
  permanent `UploadPart` failure — the same class as iteration 1's hidden 165–390 MiB map
  ceiling, one record class down). The computable number `max_part_bytes = MAX_PART_CHUNKS ×
  chunk_size` appears nowhere; the S3-conformance consequence (parts above ~165–381 MiB refused
  at default chunks, vs S3's 5 GiB part maximum) is neither registered as an accepted cost nor
  flagged. Reconcile: state the part-size ceiling per chunk-size as a real number, recompute the
  `W_ref` scenarios at in-range values, or design part-record segmentation.

- NEEDS-HUMAN [impl] — **The `sidx:` record as specified cannot deliver the two load-bearing
  iteration-3 fixes (findings 2/4): its value carries no placement.** §1 fixes the value to
  `PendingEntry { owner, lease_expiry_millis }` (`0016:221`, `0016:242-264`; `PendingEntry` today
  is `lease_expiry_millis` only, `crates/core/src/metadata.rs:344-350`; `write::intent` writes
  exactly that, `crates/core/src/write.rs:198-214`). But (a) the reaper must "orphan-mark +
  delete owned `sidx:` entries" (`0016:365`, `0016:977-978`) and orphan records are
  **placement-keyed** — `orphan:<dserver>:<chunk>:<index>`
  (`crates/core/src/metadata.rs:68-70`); a record-only reaper (D-A, `0016:864`) holding only a
  chunk-id-in-key cannot compute those keys, and deleting the entry without evidence strands the
  fragments unreferenced-and-unevidenced — kept forever under `Defer` (outcome (a), the exact F3
  class the decision exists to close). (b) `genuinely_holds` must count in-flight owned
  fragments as held **on a specific draining server** (`0016:481`, X14 `0016:1232`), but it is
  computed from `(DServerId, FragmentId)` pairs (`crates/custodian/src/gc.rs:228-247`,
  `crates/custodian/src/desired_state.rs:157-164`) — unresolvable from a chunk id. The fix is
  additive (write the `WritePlan`'s placement into the `sidx:` value at intent time) but it is a
  record-shape change ADR-0046 requires stated, and the serialization-identity section
  (`0016:250-264`) currently covers only `owner`.

- NEEDS-HUMAN [impl] — **The cursor-keyed `retire:` walk rests on a store primitive that does
  not exist, and the seam change is nowhere acknowledged.** `MetadataStore::scan` is
  prefix-only, complete-or-fail-loud, no cursor/limit/range (`crates/traits/src/lib.rs:772-776`,
  `SCAN_CAP` at `:286`). The doc lets `retire:` grow past `SCAN_CAP` (X39 `0016:1258`,
  `0016:931`) and disposes of it by "cursor-keyed bounded key ranges" (`0016:223-224`,
  `0016:983-984`) — not expressible with `scan(prefix)`: any prefix covering the namespace fails
  `ScanCapExceeded` with no partial result, and fixed sharding only divides an *unbounded*
  (alarmed-not-bounded, `0016:931`) population by a constant. Enumerating obligations therefore
  needs a new ranged/limited scan on the `MetadataStore` seam — a change every backend, the DST
  sim store, and the conformance suite must implement (ADR-0010/0016 narrow-seam rule) — which
  "What the implementing slices change" (`0016:1266-1303`) does not name. X39's disposal is
  unimplementable as written.

- NEEDS-HUMAN [impl] — **F13's disposal has an unregistered window: restore-then-serve-before-
  fence.** X17 (`0016:1235`) disposes the sharpest carried trace by "the restore fences every
  resurrected session"; the fence lives in the restore pass / restore tool (`0016:477`,
  `0016:1515-1517`) — but nothing orders that fence against gateways resuming service on the
  restored image. Concrete execution: image goes live → the client's retried Complete arrives
  **before** `reconcile_after_restore` runs → the session is `Open@E` in the image, the fence
  CAS succeeds, the records-only proof passes over resurrected `part:` records → publication
  over GC-reclaimed bytes — outcome (c), the very F13 trace, absent from the register in this
  interleaving. Needs one normative line: the fence completes before the store serves multipart
  verbs (or gateways refuse multipart until the restore fence generation completes).

## Secondary findings (consistency / register quality)

- NEEDS-HUMAN [impl] — **Stale "≈ 52 × SCAN_CAP" arithmetic contradicts the doc's own bound.**
  `MAX_OWNED_FLEET = MAX_SESSIONS × MAX_INFLIGHT_PARTS × MAX_PART_CHUNKS ≤ W_ref ≤ SCAN_CAP` by
  the knob table's own ranges (`0016:699-700`), yet three passages still claim the fleet owned
  population "can reach ≈ 52 × SCAN_CAP" (`0016:477`, `0016:752`, X25 `0016:1243`, echoed
  `0016:1334`) — an iteration-3-era figure now unconstructible under the doc's own enforced
  admission (X25's test premise cannot be set up), and the `MAX_OWNED_FLEET` row's "NOT bounded
  by `SCAN_CAP`" is false given `W_ref ≤ SCAN_CAP`. The `sidx:`-disjointness argument survives
  without the inflated number; the numbers should agree with the knobs (D-F).

- NEEDS-HUMAN [impl] — **`seg:` maintenance writers are missing from the ADR-0046 contract, and
  the repoint-vs-drain race is unregistered.** §1's `seg:` row (`0016:222`) names only the
  Completing segment-write phase as writer, but decision 2 commits reconstruction to repairing
  committed objects (placement rewrite, `0016:479`, `0016:514`) and rebalance to evacuating
  committed objects (`0016:480` excludes only *staged*) — for a committed **segmented** object
  both must rewrite `seg:<id>:<E>:<i>` placement. No precondition serializes such a rewrite
  against the retirement drain deleting/enumerating the same records on supersede/delete
  (`0016:1142-1148`; the drain step preconditions only the obligation, `0016:363`): a drain that
  enumerates a segment's old placement, races a re-place repoint, then deletes the record leaves
  the moved fragment unreferenced and unevidenced (outcome (a) for that fragment). Add the
  writer rows and the exact-bytes CAS rule + a register row.

- NEEDS-HUMAN [impl] — **Reference-build work at the segmented ceilings is uncharged — the same
  class as iteration 3's rejected ~10^10-reads claim, reintroduced for committed objects.**
  `W_ref` charges sessions' `part:`/`sidx:` reads (`0016:933-951`) but decision 7(e)
  (`0016:1132-1140`) claims only scan-safety for segment resolution: committed segmented objects
  add up to `MAX_ROOT_SEGMENTS` (312–520) record reads per inode per reconcile pass with no
  budget, knob, or alarm, and one max segmented object alone contributes ~198K chunks → ~1.78 M
  `(server, fragment)` pairs to the in-memory `ReferenceSet` (`gc.rs:228-238`). The segmented
  population is bounded by nothing but the `inode:` scan cap, so per-pass custodian work/memory
  grows orders of magnitude past today's ceiling with no register row (D-F).

- NEEDS-HUMAN [impl] — **Grace-start contradiction, delete path.** Decision 4 claims "the
  reader-safe grace still starts when the object becomes unreferenced" (`0016:667-669`) while
  the accepted-costs row states "grace starts at drain, not at the supersede/delete commit"
  (`0016:1423`). The latter is what the mechanism does (`orphaned_at` is stamped by the drain,
  `metadata.rs:470-486`); the former sentence is false as written. Direction is safe; the text
  should say one thing.

## Attempted and could not refute

Attempted, against the register and by fresh construction, and **could not** refute: the
fence/epoch machine and the O(1) session-precondition publication proof (every part/intent/slot
batch carries `require(mpu == Open@E)`, checked at commit — the post-fence residue race of
iteration 3 finding 1 is genuinely closed at creation); the per-attempt epoch-scoped `seg:` keys
(re-ran the X40 rollback→re-Complete-while-obligation-pending trace — the disjoint epochs hold);
the exactly-once terminal decrement (exact-bytes precondition on `mpu:` serializes gateway vs
reaper); the Completed-path `sidx:` walk (iteration 3 finding 2 closed); the `409`-vs-resume
contradiction (resolved cleanly in favour of `409`, cost registered); the byte-budgeted batch
inventory (checked every row against `E_tx/2`); and the D-C/D-D tension, surfaced as the ⚑
NEEDS-HUMAN question exactly as iteration 3 directed. Leg-A mechanics verified: frontmatter,
template section set, index row, link targets resolve (`docs/design/{adr,architecture}`), and
`typos` is clean on the new file.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Decide whether to accept the rerun’s unrelated flaky whole-tree failure — the docs-specific `typos`, lint, and link audit passed, but `cargo xtask ci` failed once in pre-existing `gateway-s3` test `a_bodyless_response_is_recorded_complete_not_aborted`, which passed immediately when rerun alone, so this run did not independently reproduce a fully green gate.
- [ ] T5 Judgment — Decide whether the per-session start CAS and per-chunk session precondition are an acceptable cost for a guaranteed residue bound, because accepting a cheaper best-effort cap can reopen the unbounded-staging safety class (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:1543`); merged history was checked by affected path, but closed/rejected remote work could not be independently settled from the supplied artifacts.
- [ ] Validation — fitness-to-purpose — Decide whether this draft is fit to govern #625/#508 only after the aggregate-admission defect is resolved and the architecture authority accepts the operational/capacity trade-offs; draft-to-accepted ratification is explicitly a separate governance act (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:39`).
- [ ] **`MAX_PART_CHUNKS` contradicts the doc's own knob rule — the capacity
- [ ] **The `sidx:` record as specified cannot deliver the two load-bearing
- [ ] **The cursor-keyed `retire:` walk rests on a store primitive that does
- [ ] **F13's disposal has an unregistered window: restore-then-serve-before-
- [ ] **Stale "≈ 52 × SCAN_CAP" arithmetic contradicts the doc's own bound.**
- [ ] **`seg:` maintenance writers are missing from the ADR-0046 contract, and
- [ ] **Reference-build work at the segmented ceilings is uncharged — the same
- [ ] **Grace-start contradiction, delete path.** Decision 4 claims "the
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 6 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Rejected on the foundation of the advisory review and the adversary review; the brief's directions stand — rework the document, do not re-plan. Carry forward: 1. Triage the red T4 gate: all 6 blocking findings in review-batch.md must be fixed or recorded-rejected in review-rejected.md — the gate blocks while any is unchecked. 2. Core defect (reviewer C5/T2/T4, 0016:1419): admission must MECHANICALLY enforce the aggregate W_ref bound. A flat per-session counter is distribution-dependent (~104 small-part sessions vs ~11 or ~1 large-part). Either derive MAX_SESSIONS from the worst legal MAX_PART_CHUNKS, reserve weighted reference-work units, or an equivalent aggregate budget — a narrative range does not preserve the maintenance bound. 3. Adversary refutations to close: - Knob-rule self-contradiction: W_ref/MAX_SESSIONS arithmetic is computed at MAX_PART_CHUNKS = 5,120 while the doc's own knob rule caps it at 165–381. State max_part_bytes per chunk size as a real number, recompute scenarios in-range or design part-record segmentation, and register the S3 5 GiB part-size consequence. - sidx: value carries no placement, so the reaper cannot compute orphan keys and genuinely_holds cannot count in-flight fragments. Write the WritePlan placement into the sidx: value at intent time; state the record shape per ADR-0046 and extend the serialization-identity section beyond `owner`. - The cursor-keyed retire: walk needs a ranged/limited scan the MetadataStore seam does not have (scan is prefix-only, complete-or-fail). Name the seam change in "What the implementing slices change" or redesign the walk within scan(prefix). - F13 window: add a normative line that the restore fence completes before the store serves multipart verbs (restore-then-serve-before-fence). - Secondary: fix the stale "≈ 52 × SCAN_CAP" passages to agree with the knobs; add seg: maintenance-writer rows + exact-bytes CAS rule + a register row for the repoint-vs-drain race; charge segmented reference-build work (budget/knob/alarm + register row); resolve the grace-start contradiction on the delete path. 4. PRESERVE what the adversary could not refute: the fence/epoch machine, the O(1) session-precondition publication proof, per-attempt epoch-scoped seg: keys, the exactly-once terminal decrement, and the byte-budgeted batch inventory.
- By / date: Eduard Ralph / 2026-07-23

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
