# Result — issue 655 / multipart-knob-constants-and-derivations

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: the multipart seam's **numbers**, each a named constant carrying its derivation in its
  own doc comment, plus one function — `knob_clamps_hold` — that asserts the inequalities *between*
  them. 0016 settles each knob's valid **range** and its **bounding invariant** and leaves the
  **value** to the implementing slice (`0016:1463-1480`, `:3072-3080`); the caps are enforced in
  this seam's code, so this seam picks them. After this slice the whole later protocol (#656–#660)
  and the reaper (#625) **consume** a single, self-consistent value set instead of each re-deriving
  one — which is the split-budget-authority failure `docs/principles.md:138-143` records as the
  reason the §6 invariant row exists at all.
- Success criterion: the NEW file `crates/core/tests/multipart_knobs.rs` passes. Five legs,
  all pure — no store, no runtime, no fixture beyond literals:
  1. **The shipped value set satisfies every clamp.** `knob_clamps_hold` over the deployment's own
     `Budget` returns success.
  2. **`knob_clamps_hold` is not vacuous — it REJECTS each violation, one leg per clamp.** A table
     of budgets, each violating exactly **one** inequality, each asserted to be rejected *and* to
     name the clamp it broke. At minimum: `max_chunkref_bytes × MAX_MAP_CHUNKS > V/2`; the same for
     `MAX_SEG_CHUNKS` and for `MAX_PART_CHUNKS`; `MAX_PART_CHUNKS > B_ops`; `MAX_STAGED_CHUNKS`
     below `MAX_PART_CHUNKS`; `MAX_STAGED_CHUNKS` above `MAX_ROOT_SEGMENTS × MAX_SEG_CHUNKS`;
     `MAX_INFLIGHT_PARTS > MAX_PARTS_PER_SESSION`; `MAX_INFLIGHT_PARTS > ⌊SCAN_CAP / (2 ×
     MAX_PART_CHUNKS)⌋`; `MAX_INFLIGHT_PARTS` whose whole-range fence/terminal-delete batch exceeds
     the mutation-byte budget; `MAX_INFLIGHT_PARTS > B_ops`; `MAX_SESSIONS × U_ref > W_ref`;
     `MAX_SESSIONS > SCAN_CAP/2`; `MAX_OWNED_FLEET > W_ref / 2`; a retry bound of 0. **This leg is
     the binding one** — a `knob_clamps_hold` that returns success unconditionally passes leg (1)
     and proves nothing.
  3. **The derivations are recomputed independently — the "no /2-vs-/4 drift" check, made
     mechanical.** The test recomputes, from first principles and *not* by calling the production
     helper, each derived quantity and asserts equality with the shipped constant:
     `VALUE_CHUNK_CAPACITY = ⌊(MAX_VALUE_BYTES / 2) / max_chunkref_bytes⌋`;
     `MAX_STAGED_CHUNKS = MAX_ROOT_SEGMENTS × MAX_SEG_CHUNKS`;
     `U_ref = min( (MAX_PARTS_PER_SESSION + MAX_INFLIGHT_PARTS) × MAX_PART_CHUNKS ,
     MAX_STAGED_CHUNKS + 2 × MAX_INFLIGHT_PARTS × MAX_PART_CHUNKS )`;
     `MAX_SESSIONS = min( ⌊W_ref / U_ref⌋ , SCAN_CAP/2 )`;
     `MAX_OWNED_FLEET = MAX_SESSIONS × MAX_INFLIGHT_PARTS × MAX_PART_CHUNKS`;
     `max_part_bytes = MAX_PART_CHUNKS × chunk_size`. Assert the **halving** each budget uses is
     the one 0016 states (`V/2`, `E_tx/2`, `W_ref/2`) — a `/4` or a whole-`V` sizing must fail this
     leg. Assert the *stated range* too: `MAX_MAP_CHUNKS`, `MAX_SEG_CHUNKS` and `MAX_PART_CHUNKS`
     each land in **165–381** at the `b_ref` extremes 0016 computes (`0016:1050-1075`), so a
     `max_chunkref_bytes` that drifted from the encoded reality is caught.
  4. **Every capacity knob fits the key space #691 gave it** (`PART_NUMBER_WIDTH = 6` /
     `SLOT_INDEX_WIDTH = 6`, pinned at the split Plan). `MAX_INFLIGHT_PARTS` is addressable by
     the slot-index width and `MAX_PARTS_PER_SESSION` by the part-number width, with
     byte-lexicographic order still equal to numeric order **at the cap** — the property that makes
     the `slot:` key space *be* the in-flight bound (`0016:349`) rather than an integer someone must
     CAS correctly. A knob that overflows its width is rejected by `knob_clamps_hold`.
  5. **The admission backoff is bounded, and its two retry budgets are separate.**
     `admission_backoff_millis` never exceeds the cap, is never below the base, grows with the
     attempt, and includes jitter within the stated envelope for every jitter input in a swept
     range. Assert **separately** that the upload-id-collision budget and the CAS-contention budget
     are **distinct constants**: a single shared budget is the carried-forward `503 SlowDown` defect
     (below), and this is the slice that fixes its *numbers*.
- Repo + branch target: getwyrd/wyrd @ main   (INTEGRATION §2: single slice; no live milestone
  integration branch. Verified `git -C ../wyrd rev-parse origin/main` → `339da46`.)
- Scope: the knob **values** and the clamps between them, appended to
  `crates/core/src/multipart.rs` (the module #691 creates and #692/#693 extend — the workspace has no directory modules;
  verified), with each constant carrying its **derivation in its own doc comment** and the whole set
  cross-checked by one `knob_clamps_hold`:
  - **Consumed, never redefined** — import from `crates/core/src/metadata.rs` and `wyrd_traits`:
    `MAX_VALUE_BYTES` (`metadata.rs:327`), `MAX_ROOT_VALUE_BYTES` (`:352`), `MAX_ROOT_SEGMENTS`
    (`:322`), `SCAN_CAP` (`crates/traits/src/lib.rs:286`). A second spelling of any of these is a
    defect.
  - **The value-ceiling family:** `max_chunkref_bytes` (with its per-scheme companion — the encoded
    worst case, **measured**, not asserted in prose), the shared `VALUE_CHUNK_CAPACITY =
    ⌊(V/2) / max_chunkref_bytes⌋`, and `MAX_MAP_CHUNKS` / `MAX_SEG_CHUNKS` / `MAX_PART_CHUNKS` over
    it — the *identical* rule for all three, because each is one JSON value (`0016:1058-1075`) —
    plus `max_part_bytes = MAX_PART_CHUNKS × chunk_size`, the number that becomes the `UploadPart`
    refusal.
  - **The session family:** `MAX_PARTS_PER_SESSION`, `MAX_INFLIGHT_PARTS` (under all **four** clamps
    at `0016:1471`), `MAX_STAGED_CHUNKS` (fixed at the publishable ceiling), and the fleet-wide
    `MAX_OWNED_FLEET`.
  - **The derived pair, spelled as derivations:** `U_ref` (both branches, `min` of them) and
    `MAX_SESSIONS = min(⌊W_ref/U_ref⌋, SCAN_CAP/2)`.
  - **The retry/backoff bounds:** `R_publish`, `MAX_COMPLETE_ATTEMPTS`, and the **two separate**
    budgets — upload-id collision vs. admission-CAS contention — with the jittered backoff base and
    cap, and `admission_backoff_millis`.
  - **`Budget`** — the value type the clamps are checked over, so `knob_clamps_hold` can be run
    against a hypothetical set (leg 2) and not only the shipped one; and `knob_clamps_hold` itself,
    returning **which** clamp failed, not a bare bool.
  - **The two inputs 0016 assigns elsewhere but this seam's clamps depend on:** `W_ref` (the
    reconcile RAM budget) and the `B` / `B_ops` operation-and-byte budget are **#625's** by
    `0016:3072-3080`, yet `MAX_SESSIONS` and the `MAX_PART_CHUNKS ≤ B_ops` clamp cannot be written
    without them, and #625 builds **after** this slice. So: define them **here** as named constants
    with their derivation, record the chosen values as an explicit **value set** in
    `build-notes.md`, and state in each doc comment that **#625 consumes these and must not
    re-derive them**. (Carried verbatim from #636's brief; it is the split-budget-authority rule
    above.) If a value genuinely cannot be chosen without #625, that is a Check §6 item — **not** a
    placeholder constant.

  **Out of scope:**
  - **`crates/core/src/metadata.rs` — DO NOT TOUCH.** #682 is in this same wave and owns that file;
    editing it here turns two independent bundles into a conflict. Everything this slice needs from
    it is available by import. This is a hard rule, not a preference. (The discontinued #636 patch
    put `MAX_BATCH_BYTES` / `MAX_BATCH_OPS` in `metadata.rs`; put them in `multipart.rs` instead.)
  - **The reaper's windows** — `W_open`, `W_session`, `W_completing`, `W_tombstone`, the cursor-keyed
    out-of-band drain and the clock guard are **#625's** (0016's own assignment, and #636's stated
    out-of-scope). This slice defines no time window except the memory budget `W_ref`. Likewise
    `W_write` / `G_orphan` (the write path and #625) and `W_repoint` (#653) are not this slice's.
  - **Enforcing any cap.** No admission check, no `EntityTooLarge` refusal, no batch splitting —
    those are #656/#657/#658/#659. This slice ships the numbers and the assertion that they are
    mutually consistent; it changes no behaviour because there is none yet.
  - **Any store round trip**, any `async fn`, any `WriteBatch`.
  - `crates/core/src/lib.rs` — untouched (the `pub mod multipart;` line is #691's).
  - The S3 verbs and their status codes (**#508**); the custodian protection class (**#637**).
  - Any file under `docs/design/adr/` or `docs/design/specs/`, any edit to `0016` itself, any
    conformance-vector change, any new dependency.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): unverifiable — gate exceeded its 7200s timeout
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: unverifiable —                why this slice has no isolable red (the cargo output is above).
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 99.5% — 183 of 184 instrumentable changed lines executed (floor 80%); 184 of 749 changed lines were instru
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 125 mutants tested in 4m: 2 missed, 94 caught, 29 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_655/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 12.98s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Issue #655 implements the multipart knob constants and derivations; it needs a rebuild for an incomplete byte-budget clamp and excess review size, plus human sign-off on verification and sizing assumptions.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The normative knob table fixes explicit ranges, derivations, ownership, and failure impacts, making the requested seam independently judgeable (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:1462`). |
| C2 Reproduction (red pre-fix) | NEEDS-HUMAN | Human must accept criterion-absence as the reproduction — without the patch, the test loses its imported API and fails compilation before any assertion executes (`crates/core/tests/multipart_knobs.rs:19`; `gate-logs/C4-verify.log:50`). |
| C3 Change | PASS | The change remains additive and confined to the authorized module plus its required integration test, with no new dependency or enforcement surface (`crates/core/src/multipart.rs:4356`; `crates/core/tests/multipart_knobs.rs:1`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Human must require a completed required gate or accept partial evidence — post-fix fmt/test/clippy, typos, machete, and non-advisory deny checks pass, but red never executes and the full gate times out in `custodian_gc` before cargo-deny advisories (`gate-logs/C4-ci.log:2049`; `gate-logs/C4-verify.log:50`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Rebuild must require `B_bytes` to hold at least one maximal segment mutation and test its rejection — the validator currently checks only slot-pin bytes, so an accepted set can derive a zero-sized segment batch and never publish (`crates/core/src/multipart.rs:4980`; `crates/core/tests/multipart_knobs.rs:178`). |
| T1 Structure | PASS | The one-authority structure is preserved because `KnobSet` projects into the existing `Budget` and reuses its derivations instead of re-spelling them (`crates/core/src/multipart.rs:4787`). |
| T2 Shape | FAIL | The reviewability cap is exceeded: a conservative diff count leaves 587 content-bearing additions after comments, blanks, attributes, punctuation-only, and chain-continuation lines are excluded, versus the brief's 400-line ceiling (`crates/core/src/multipart.rs:5045`; `crates/core/tests/multipart_knobs.rs:636`). |
| T3 Runtime | NEEDS-HUMAN | Human must approve the assumed 5 ms per backend operation or require slowest-backend calibration — that unmeasured value determines whether `MAX_BATCH_OPS` actually stays within the five-second transaction envelope (`crates/core/src/multipart.rs:4544`). |
| T4 Contribution | N/A | `pr-description.md` is absent by design at Check; the substantive contribution audit is mandatory and reruns at publish (`gate-logs/T4-contribution.log:10`). |
| T5 Judgment | NEEDS-HUMAN | Human must decide whether `W_ref` may ship as a compile-time 4,000,000-reference budget rather than a deployment input sized from reconcile-host RAM, because that choice fixes admitted concurrency (`crates/core/src/multipart.rs:4585`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Human must decide whether this inert value set is fit for the later enforcing slices despite no current production consumer exercising the composition (`crates/core/src/multipart.rs:4371`). |

Prior-art check: an affected-path scan across 300 GitHub PRs found only merged prerequisite edits (#703, #724, #725, #792, #793, #799) to `crates/core/src/multipart.rs`, no closed/unmerged path match, and no prior PR touching the new test; the brief separately records the discontinued #508/#636 issue work.

Mutation note: the two surviving `<` to `<=` mutants select equal-valued branches at the tie and are equivalent, so they add no defect beyond the missing clamp above (`gate-logs/C5-mutants.log:13`).


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C2 Reproduction (red pre-fix) — Human must accept criterion-absence as the reproduction — without the patch, the test loses its imported API and fails compilation before any assertion executes (`crates/core/tests/multipart_knobs.rs:19`; `gate-logs/C4-verify.log:50`).
- [ ] C4 Verification (red→green) — Human must require a completed required gate or accept partial evidence — post-fix fmt/test/clippy, typos, machete, and non-advisory deny checks pass, but red never executes and the full gate times out in `custodian_gc` before cargo-deny advisories (`gate-logs/C4-ci.log:2049`; `gate-logs/C4-verify.log:50`).
- [ ] C5 Causal adequacy — Rebuild must require `B_bytes` to hold at least one maximal segment mutation and test its rejection — the validator currently checks only slot-pin bytes, so an accepted set can derive a zero-sized segment batch and never publish (`crates/core/src/multipart.rs:4980`; `crates/core/tests/multipart_knobs.rs:178`).
- [ ] T3 Runtime — Human must approve the assumed 5 ms per backend operation or require slowest-backend calibration — that unmeasured value determines whether `MAX_BATCH_OPS` actually stays within the five-second transaction envelope (`crates/core/src/multipart.rs:4544`).
- [ ] T5 Judgment — Human must decide whether `W_ref` may ship as a compile-time 4,000,000-reference budget rather than a deployment input sized from reconcile-host RAM, because that choice fixes admitted concurrency (`crates/core/src/multipart.rs:4585`).
- [ ] Validation — fitness-to-purpose — Human must decide whether this inert value set is fit for the later enforcing slices despite no current production consumer exercising the composition (`crates/core/src/multipart.rs:4371`).
- [ ] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) unverifiable — gate exceeded its 7200s timeout
- [ ] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_655/review-b
- [ ] C1 Spec — Human must reconcile the settled 165–381 capacity range with the measured 315-byte reference, which yields 158 at the 50 KB ceiling and changes the accepted limits (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:1466`; `crates/core/src/multipart.rs:4364`).

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
- Iteration delta (if iterating): Auto-iterate (round 2): rebuilding for the implementation-level findings — C2 Reproduction (red pre-fix) — Human must accept criterion-absence as the reproduction — without the patch, the test loses its imported API and fails compilation before any assertion executes (`crates/core/tests/multipart_knobs.rs:19`; `gate-logs/C4-verify.log:50`).; C4 Verification (red→green) — Human must require a completed required gate or accept partial evidence — post-fix fmt/test/clippy, typos, machete, and non-advisory deny checks pass, but red never executes and the full gate times out in `custodian_gc` before cargo-deny advisories (`gate-logs/C4-ci.log:2049`; `gate-logs/C4-verify.log:50`).; C5 Causal adequacy — Rebuild must require `B_bytes` to hold at least one maximal segment mutation and test its rejection — the validator currently checks only slot-pin bytes, so an accepted set can derive a zero-sized segment batch and never publish (`crates/core/src/multipart.rs:4980`; `crates/core/tests/multipart_knobs.rs:178`).; T3 Runtime — Human must approve the assumed 5 ms per backend operation or require slowest-backend calibration — that unmeasured value determines whether `MAX_BATCH_OPS` actually stays within the five-second transaction envelope (`crates/core/src/multipart.rs:4544`).; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_655/review-b. 4 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-09-12

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
