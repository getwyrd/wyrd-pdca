# Result — issue 716 / multipart-session-lifecycle-records

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: with the admission ledger and its profile arithmetic merged (child-1), the
  in-flight lifecycle records still do not exist. This child lands `SessionRecord` with
  `SessionState`/`PublishTarget`/`Completion` (`mpu:` — the *states* of `0016:528-602`;
  the answer table is #693's), `SlotRecord` (`slot:`), and `PartRecord`/`PartSummary`
  (`part:`/`psum:`), each validating **inside its own `Deserialize`** and encoded/decoded
  through the base's `metadata::encode` / `metadata::decode` (`metadata.rs:1536-1543`).
  **CORRECTED 2026-08-09, at #715's re-plan:** this field previously said each type gets
  "an arm in child-1's `encode_record`/`decode_record`". **There is no such envelope and
  none is coming** — #715's third attempt built one and it was the v3 T2 flat FAIL: a value
  carries no type tag (`0016:348` and the whole §1 record table), so a dispatching arm has
  nothing to dispatch on, and the base already provides the generic codec. The pattern to
  mirror is the target's own, cited under **Citations expected**. Salvage the corresponding
  types from `results/issue_692/iteration-v2/patch.diff`, fixing the recorded defects.
- Success criterion: every type this child lands round-trips `encode`/`decode`, and
  each of the following hand-authored torn values is rejected with a typed error
  (ADR-0045) — one named negation per leg demonstrated in `build-notes.md`:
  **(1c)** a `SessionRecord` whose `publish_target.parent`/`name` disagrees with the
  session's own `parent`/`object` (v2 review, `multipart.rs:1258`);
  **(1c-epoch, NEW 2026-08-09 — plan-review finding, and it is a second identity relation the
  brief had simply omitted)** a `SessionRecord` whose `publish_target`'s **fence epoch**
  disagrees with the session's own `epoch` is rejected. `publish_target` carries the
  `Completing` fence epoch `E` precisely so segments are written under a deterministic
  segment-group nonce for **that attempt** (`0016:350`, `:560-563`); a record whose two
  epochs disagree addresses another attempt's segment-group — the F18 class. The archived
  decoder already enforced it (`results/issue_692/iteration-v2/patch.diff:719`), so this is
  restoring a check the salvage would otherwise silently lose;
  **(1i, PartRecord half)** `PartRecord` validates each `ChunkRef` structurally at decode
  rather than accepting a raw one (v2 reviewer C5 + T2 FAIL, `multipart.rs:1538`) — nested
  types validate at decode, not just the outer record. **The structural checks are exactly
  these, enumerated so an over-strict decoder is as wrong as a permissive one**
  (`ADR-0045:69-74`, which tabulates them): the `EcScheme` is rejected unless
  `erasure::supported(k, m)` (`crates/core/src/erasure.rs:120` — the #285 precedent).
  **The "logical `len` must be non-zero" rule that stood here is WITHDRAWN (plan-review
  finding, 2026-08-09): it was an invention.** ADR-0045's row says "logical length
  **consistent with the scheme**", not non-zero, and the target's encoder handles a
  zero-length chunk without complaint (`crates/core/src/erasure.rs:79-83`, `shard_size`
  applies `.max(1)`). Do MUST NOT reinvent it — inventing a bound whose source does not
  demand it is exactly what cost #715 three rounds. The record-level length rule lives in
  leg 1k below, where the target genuinely does demand one.
  **PLUS a binding POSITIVE case:** a `ChunkRef`
  whose `placement` length does NOT match its scheme's fragment count **still decodes** —
  placement length is the standing *contextual* check, liberal on read
  (`ADR-0045:69-74`, `AGENTS.md:146-149`, `0016:416-429`). Geometry is validated; length is
  not. Without that positive case a decoder that rejects both passes this brief and breaks
  the target contract;
  **(1i, SlotRecord half)** a `SlotRecord` with `lease_expiry_millis <=
  reserved_at_millis` — a slot born already lapsed — is rejected (`multipart.rs:1657`);
  **(1j, NEW — state-forbidden fields, plan-advisory finding 2026-08-09)** an `mpu:` value
  that carries a field its own `state` forbids is a **decode error**, not a value: the
  normative example is a `Completing`-only `fenced_at_millis` on an `Open` session
  (`0016:403-415`). Without this leg a permissive state decoder passes every round trip and
  every other leg here while accepting exactly the record the target calls invalid. Prove it
  with a hand-authored `Open` session carrying `fenced_at_millis`, and the mirror: a
  `Completing` session MISSING a field that state requires.
  Decode validates against **format** maxima, never live knobs (`0016:390-402`).
  **(1k, NEW 2026-08-09 — plan-review finding)** a `PartRecord` whose own `len` does not equal
  the checked sum of its `chunks`' lengths is rejected at decode, and the sum is **overflow-
  checked** so an absurd chunk list is a typed error rather than a wrap. The record stores
  both fields (`0016:351`), and this is not a new invention: the target's analogous
  `SegmentRecord::from_wire` does exactly this — `checked_chunk_bytes(&chunks)` compared to
  `byte_len`, returning `ChunkMapError::SegmentLengthMismatch`
  (`crates/core/src/metadata.rs:1142-1156`). **Mirror that function.** The archived
  implementation had the check (`results/issue_692/iteration-v2/patch.diff:895`); without it
  as a criterion Do can drop it and still satisfy every other leg.
  **(1m, NEW 2026-08-09 — plan-review finding: the wire policy does NOT arrive from #715.)**
  Each landed record type is `#[serde(deny_unknown_fields)]`, and an unknown field in a
  stored value is a typed decode rejection. #715 settles this for its OWN admission type
  only — with the codec envelope withdrawn there is no shared wrapper through which a policy
  could propagate, so this child must decide it for the lifecycle types explicitly. The
  reasoning is #715's and it is verified in the target: `metadata.rs` has **two** CAS shapes
  — `require(key, encode(prior))` for `inode:` (`:1766`, `:1891`) and `require(key, current)`
  on the **raw** read bytes for `pending:` (`:1984`, `:2011-2020`) — so a permissive decoder
  either wedges every later CAS with a permanent `Conflict` or silently drops the field on
  the next `put`. A session transition CASes the session record whole (`0016:555-558`), and
  which shape it uses is #656–#659's to choose, so both hazards are live here too.
  **(leg D — docs currency, SETTLED YES, added 2026-08-09 at #715's re-plan.)** This child
  declares four more persisted record shapes, so `AGENTS.md:154-157` bites here exactly as it
  does on #715 and #717 — all three now carry the paragraph rather than leaving it to a
  fourth sign-off round. **EXTEND, do not duplicate:** #715 lands beneath this child and
  already adds a multipart paragraph to
  `docs/design/architecture/05-building-block-view.md` § "The metadata model"; this child
  extends that same paragraph with the session/slot/part records in one or two sentences.
  Read what #715 actually landed on the merged base before writing — do not assume this
  brief's wording of it. Verified by presence + the `C4-ci` prose/render gate, **not** by a
  negation; it is NOT one of the nine demonstrations below.
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope: extend `crates/core/src/multipart.rs` with
  the lifecycle types named in **Defect** above and their validating decoders, plus the
  typed error variants those rejections need as new variants of the module's existing
  `RecordError` (which #715 has already widened from keys to record values beneath this
  child). **The landed public type list is exactly these
  SEVEN** — corrected 2026-08-09 from "five", which contradicted the Defect field and left
  the budget and the "every type round-trips" criterion without a fixed review surface:
  `SessionRecord`, `SessionState`, `PublishTarget`, `Completion`, `SlotRecord`,
  `PartRecord`, `PartSummary`. Private wire/`*Wire` mirror structs and the error variants
  their manual `Deserialize` impls need are **implementation detail, not additional landed
  types** — they do not count against the seven and need no separate round-trip leg, but
  they DO count against the line budget. One new test file. **Plus the ONE docs paragraph
  extension of leg D** (added 2026-08-09). Budget ≈ 770 added lines /
  3 files. Cite `path:line`; sources Do MUST open: `0016:333-527`, `0016:528-602`,
  ADR-0045. / out of scope: `Budget`/`AdmissionRecord` (child-1, merged beneath this);
  `OwnedEntry`/`StagedPlacement`, retirement types, `PendingEntry`, and every file
  outside `multipart.rs` + the new test (child-3); the outcome enums and answer table
  (#693); knob values (#655); store round trips (#656–#659); **every `docs/` file except
  the one `05-building-block-view.md` paragraph** — ADRs, proposals and specs untouched
  (INTEGRATION §2 immutability).

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — PASS on confirm — first run failed transiently: xtask: `cargo test --workspace --exclude wyrd-dst` failed with exit stat
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: unverifiable —                why this slice has no isolable red (the cargo output is above).
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 56 mutants tested in 2m: 34 caught, 22 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 1 blocking, 2 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_716/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — scripts/pdca contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of issue #716: add validated multipart session, slot, part, and part-summary lifecycle records and document their persisted metadata shapes.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The exact seven public shapes, nine falsifiability legs, and format-vs-live validation boundary are sufficiently determinate against the normative record table (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:348`). |
| C2 Reproduction (red pre-fix) | NEEDS-HUMAN | Accept a compile-only absence witness for this new functionality — base production plus the retained test exits 101 before behavior can run because the seven APIs do not exist (`crates/core/tests/multipart_session_records.rs:35`). |
| C3 Change | NEEDS-HUMAN | Decide whether to grant a Plan budget exception — the allowed three-file scope is respected, but the diff adds 1,436 raw lines (837 even after excluding blanks and line comments) against the stated 770-line cap, expanding the review surface (`crates/core/src/multipart.rs:1459`, `crates/core/tests/multipart_session_records.rs:1`, `docs/design/architecture/05-building-block-view.md:202`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Accept criterion-absence as the red oracle — the base-plus-test leg exits 101, while the patch passes 27/27 focused tests and every CI component; `cargo deny` needed scratch-local lock state because the default advisory lock was read-only (`crates/core/tests/multipart_session_records.rs:35`). |
| C5 Causal adequacy | PASS | Direct decode constructors enforce the challenged relations, no capability-probe/runtime-guard smell is introduced, and mutation rerun caught every viable mutation (34 caught, 22 unviable, zero survivors) (`crates/core/src/multipart.rs:1705`, `crates/core/src/multipart.rs:1811`, `crates/core/src/multipart.rs:2044`). |
| T1 Structure | PASS | The change stays in exactly the three authorized locations: the existing multipart module, one new focused test, and the existing metadata-model paragraph (`crates/core/src/multipart.rs:1459`, `crates/core/tests/multipart_session_records.rs:1`, `docs/design/architecture/05-building-block-view.md:202`). |
| T2 Shape | PASS | The seven public data shapes use per-type decode validation and closed wire forms while leaving placement length contextual and liberal on read (`crates/core/src/multipart.rs:1472`, `crates/core/src/multipart.rs:1912`, `crates/core/src/multipart.rs:2033`). |
| T3 Runtime | N/A | No runtime behavior applies in this cycle: these are pure record codecs with no live writer until downstream store wiring (`crates/core/src/multipart.rs:1634`). |
| T4 Contribution | NEEDS-HUMAN | Decide whether provenance is sufficient — affected-path merged history contains no prior lifecycle definitions, but closed/rejected work and the asserted batch-review result cannot be mechanically settled because `scripts/review-branch`, `scripts/pdca`, and their artifacts are absent. |
| T5 Judgment | PASS | The evidence is adversarial enough for implementation judgment: all seven round trips, eight isolating rejection cases, the liberal-placement positive, serialization-identity witnesses, and zero surviving mutants (`crates/core/tests/multipart_session_records.rs:205`, `crates/core/tests/multipart_session_records.rs:394`, `crates/core/tests/multipart_session_records.rs:646`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether pure-code evidence is fit for the future multipart store path — no writer or live compare-and-set path consumes these records yet, so workflow compatibility remains outside this cycle (`crates/core/src/multipart.rs:1634`, `docs/design/architecture/05-building-block-view.md:202`). |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C2 Reproduction (red pre-fix) — Accept a compile-only absence witness for this new functionality — base production plus the retained test exits 101 before behavior can run because the seven APIs do not exist (`crates/core/tests/multipart_session_records.rs:35`).
- [x] C3 Change — Decide whether to grant a Plan budget exception — the allowed three-file scope is respected, but the diff adds 1,436 raw lines (837 even after excluding blanks and line comments) against the stated 770-line cap, expanding the review surface (`crates/core/src/multipart.rs:1459`, `crates/core/tests/multipart_session_records.rs:1`, `docs/design/architecture/05-building-block-view.md:202`).
- [x] C4 Verification (red→green) — Accept criterion-absence as the red oracle — the base-plus-test leg exits 101, while the patch passes 27/27 focused tests and every CI component; `cargo deny` needed scratch-local lock state because the default advisory lock was read-only (`crates/core/tests/multipart_session_records.rs:35`).
- [x] T4 Contribution — Decide whether provenance is sufficient — affected-path merged history contains no prior lifecycle definitions, but closed/rejected work and the asserted batch-review result cannot be mechanically settled because `scripts/review-branch`, `scripts/pdca`, and their artifacts are absent.
- [x] Validation — fitness-to-purpose — Decide whether pure-code evidence is fit for the future multipart store path — no writer or live compare-and-set path consumes these records yet, so workflow compatibility remains outside this cycle (`crates/core/src/multipart.rs:1634`, `docs/design/architecture/05-building-block-view.md:202`).
- [x] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [x] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 1 blocking, 2 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_716/review-b
- [x] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) flaked at Check — failed, then passed its once-only confirm re-run (full output: gate-logs/C4-ci.log) — confirm the pass is trustworthy and note what interfered
- [x] The problem statement cannot be checked against the tracker record because the supplied advisory bundle has no `notes.json` or `sources/`; the brief instead relies on unquoted “v2 review” claims (`brief.md:12-18`) and tells Do to salvage `results/issue_692/iteration-v2/patch.diff` / read `check-review.md` (`brief.md:62-77`), but neither path exists in `$PDCA_TARGET`. Supply the tracker/archive evidence or quote the binding thread findings in the brief; otherwise the asserted 1c/1i causes and the failed-prior-attempt context are unauditable.
- [x] The claimed restored invariant is cited to sources that do not support it: `$PDCA_TARGET` has no `docs/principles.md`, while `brief.md:28-30` cites `0016:2802-2813` for C-1 even though `docs/design/proposals/draft/0016-multipart-commit-protocol.md:2802-2813` discusses acceptable cost classes and graduation-disposition rules, not field agreement or recursive decode validation. The format-max citation is stale too: `brief.md:19` cites `0016:499-511`, but the actual format-vs-live-knob rule is at `docs/design/proposals/draft/0016-multipart-commit-protocol.md:390-402`. Replace these with resolving evidence before treating the root-cause framing as established.
- [x] The success criterion does not falsify the promised “validating” session decoder: its only session negation is a mismatched `publish_target` (`brief.md:9-18`), while the target contract also requires a decode error when an `mpu:` value carries a field forbidden by its state—explicitly, `fenced_at_millis` on `Open` (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:403-407`). A permissive state-field decoder can pass the named test and every round trip; add an observable negative matrix for state-specific required/forbidden fields.
- [x] The `ChunkRef` leg leaves a load-bearing acceptance rule untested. “Validates each `ChunkRef` structurally” (`brief.md:14-16`) does not enumerate the required scheme/logical-length checks, and it does not pin that placement length is deliberately *not* a decode-time invariant (`docs/design/adr/0045-metadata-validation-boundaries.md:69-74`; `docs/design/proposals/draft/0016-multipart-commit-protocol.md:416-429`). Add exact negative cases for the structural fields and a positive case proving a wrong-length placement still decodes, or an over-strict decoder can satisfy the brief while violating the target contract.
- [x] Scope is not enumerable: the Defect names seven Rust types—`SessionRecord`, `SessionState`, `PublishTarget`, `Completion`, `SlotRecord`, `PartRecord`, and `PartSummary` (`brief.md:2-8`)—but Scope and the 700-line budget call them “the five lifecycle types” (`brief.md:41-50`). State the exact landed type list (and whether helper wire/error types count); otherwise the two-file budget and “every type” round-trip criterion have no fixed review surface.
- [x] The declared prerequisite exists but does not match the brief’s base-state assertion: `dependency-state.json:2-5` records #715 as `PLANNED`, while the brief repeatedly says its result is already “merged” (`brief.md:2`, `brief.md:39-45`, `brief.md:86-89`). The supplied `origin/main` target reinforces the mismatch: `crates/core/src/multipart.rs:9-10` says `Budget` and `encode_record`/`decode_record` are still the next child, and those symbols are absent. Make the plan explicitly contingent on materializing #715’s merged result and require the citations/scope estimate to be refreshed on that base; the current target cannot support the planned dispatch-arm work.
- [x] size backstop — this slice is behaving oversized: 2 round(s) already spent (threshold 2). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: merged-wider
- Iteration delta (if iterating):
- By / date: Eduard Ralph / 2026-08-11

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 6 finding(s); brief revised: yes (plan-advisory-*.md)
- `review-rejected.md` is written before the patch's final edit pass, so a dispositioned finding's line number can go stale by the time the gate re-reviews the final diff (this round: `multipart.rs:1578`, same argued-down whitespace/reordering class as the six already-recorded lines, just never re-synced) — Do should re-sync/write `review-rejected.md` line numbers AFTER the patch is finalized, not before.
- File a bug: `wyrd-gateway-s3::tests::a_bodyless_response_is_recorded_complete_not_aborted` flaked in this bundle's C4-ci run (panicked, then passed clean on confirm) — unrelated to this patch (only touches `crates/core/src/multipart.rs` + new test + one doc paragraph); investigate the 204/no-body vs Drop-arm-`aborted` logging race in `crates/gateway-s3/src/lib.rs:4259`.
- `deferred-findings.json`'s carry-forward ledger (auto-iterate, #332) replays a HUMAN §6 finding verbatim into every later round's §6 so it isn't silently lost across `iterate-do` rebuilds — but it has no check for whether a later Plan revision to `brief.md` already answered that finding, so an already-fixed finding (this bundle: items 9 and 10, both plan-advisory findings the brief was subsequently revised to address) keeps reappearing at sign-off until a human manually re-verifies and clears it each time. Consider having the ledger (or assemble.py) diff the finding's cited brief text/lines against the current brief before replaying it.
- (empty is the common case)
