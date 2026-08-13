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
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: unverifiable —                why this slice has no isolable red (the cargo output is above).
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 54 mutants tested in 2m: 34 caught, 20 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_716/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — scripts/pdca contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of issue #716’s multipart session, slot, part, and summary lifecycle records with decode-time structural validation and architecture documentation.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance boundary is determinate: seven record types and the required record relations map directly to the persisted-value table at `docs/design/proposals/draft/0016-multipart-commit-protocol.md:346`. |
| C2 Reproduction (red pre-fix) | NEEDS-HUMAN | Accept criterion absence as the born-at-tier red oracle — with production reverted but the new imports retained at `crates/core/tests/multipart_session_records.rs:35`, the test exits 101 at compile time, so no pre-fix behavior can execute. |
| C3 Change | PASS | No Plan re-entry is needed: 766 nonblank/noncomment additions fit the 770-line budget and are confined to the lifecycle module/test plus the required architecture paragraph at `docs/design/architecture/05-building-block-view.md:202`. |
| C4 Verification (red→green) | NEEDS-HUMAN | Accept compile-red→green as sufficient verification — the reverted slice cannot compile at `crates/core/tests/multipart_session_records.rs:35`, while the patched focused suite passes 24/24 and a second full `cargo xtask ci` run passes, leaving no executable old behavior to compare. |
| C5 Causal adequacy | PASS | The required relations are enforced at the value-construction boundaries (`crates/core/src/multipart.rs:1705`, `crates/core/src/multipart.rs:1811`, `crates/core/src/multipart.rs:1955`), all 54 in-diff mutants were caught or unviable, and the patch adds no capability probe or symptom guard. |
| T1 Structure | PASS | The change remains cohesive: production records begin in the existing multipart module at `crates/core/src/multipart.rs:1459`, their criterion suite is isolated at `crates/core/tests/multipart_session_records.rs:1`, and docs currency is handled in the living architecture. |
| T2 Shape | PASS | The key-selected record model needs no type envelope (`crates/core/src/multipart.rs:15`), and the public record shapes route generic codec reads through validating `Deserialize` boundaries such as `crates/core/src/multipart.rs:1639` and `crates/core/src/multipart.rs:1899`. |
| T3 Runtime | N/A | No runtime behavior is in scope: the module explicitly has no writer, store call, or production consumer yet (`crates/core/src/multipart.rs:63`), and the new logic is pure decode/encode validation. |
| T4 Contribution | NEEDS-HUMAN | Decide whether review and contribution provenance is sufficient — affected-path merged history contains no prior lifecycle definitions, but closed/rejected work and the asserted four-finding batch-review failure cannot be mechanically settled because the `review-branch`/contribution scripts and artifacts are absent. |
| T5 Judgment | PASS | No remaining implementation defect is evidenced: all enumerated torn-value and liberal-read witnesses at `crates/core/tests/multipart_session_records.rs:396` pass, the mutation rerun has no survivors, and the complete repository gate is green. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether record-only evidence is sufficient for the upcoming store wiring — this slice intentionally has no live writer or consumer (`crates/core/src/multipart.rs:63`), so end-to-end operational fitness cannot yet be observed. |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C2 Reproduction (red pre-fix) — Accept criterion absence as the born-at-tier red oracle — with production reverted but the new imports retained at `crates/core/tests/multipart_session_records.rs:35`, the test exits 101 at compile time, so no pre-fix behavior can execute.
- [ ] C4 Verification (red→green) — Accept compile-red→green as sufficient verification — the reverted slice cannot compile at `crates/core/tests/multipart_session_records.rs:35`, while the patched focused suite passes 24/24 and a second full `cargo xtask ci` run passes, leaving no executable old behavior to compare.
- [ ] T4 Contribution — Decide whether review and contribution provenance is sufficient — affected-path merged history contains no prior lifecycle definitions, but closed/rejected work and the asserted four-finding batch-review failure cannot be mechanically settled because the `review-branch`/contribution scripts and artifacts are absent.
- [ ] Validation — fitness-to-purpose — Decide whether record-only evidence is sufficient for the upcoming store wiring — this slice intentionally has no live writer or consumer (`crates/core/src/multipart.rs:63`), so end-to-end operational fitness cannot yet be observed.
- [ ] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_716/review-b
- [ ] The problem statement cannot be checked against the tracker record because the supplied advisory bundle has no `notes.json` or `sources/`; the brief instead relies on unquoted “v2 review” claims (`brief.md:12-18`) and tells Do to salvage `results/issue_692/iteration-v2/patch.diff` / read `check-review.md` (`brief.md:62-77`), but neither path exists in `$PDCA_TARGET`. Supply the tracker/archive evidence or quote the binding thread findings in the brief; otherwise the asserted 1c/1i causes and the failed-prior-attempt context are unauditable.
- [ ] The claimed restored invariant is cited to sources that do not support it: `$PDCA_TARGET` has no `docs/principles.md`, while `brief.md:28-30` cites `0016:2802-2813` for C-1 even though `docs/design/proposals/draft/0016-multipart-commit-protocol.md:2802-2813` discusses acceptable cost classes and graduation-disposition rules, not field agreement or recursive decode validation. The format-max citation is stale too: `brief.md:19` cites `0016:499-511`, but the actual format-vs-live-knob rule is at `docs/design/proposals/draft/0016-multipart-commit-protocol.md:390-402`. Replace these with resolving evidence before treating the root-cause framing as established.
- [ ] The success criterion does not falsify the promised “validating” session decoder: its only session negation is a mismatched `publish_target` (`brief.md:9-18`), while the target contract also requires a decode error when an `mpu:` value carries a field forbidden by its state—explicitly, `fenced_at_millis` on `Open` (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:403-407`). A permissive state-field decoder can pass the named test and every round trip; add an observable negative matrix for state-specific required/forbidden fields.
- [ ] The `ChunkRef` leg leaves a load-bearing acceptance rule untested. “Validates each `ChunkRef` structurally” (`brief.md:14-16`) does not enumerate the required scheme/logical-length checks, and it does not pin that placement length is deliberately *not* a decode-time invariant (`docs/design/adr/0045-metadata-validation-boundaries.md:69-74`; `docs/design/proposals/draft/0016-multipart-commit-protocol.md:416-429`). Add exact negative cases for the structural fields and a positive case proving a wrong-length placement still decodes, or an over-strict decoder can satisfy the brief while violating the target contract.
- [ ] Scope is not enumerable: the Defect names seven Rust types—`SessionRecord`, `SessionState`, `PublishTarget`, `Completion`, `SlotRecord`, `PartRecord`, and `PartSummary` (`brief.md:2-8`)—but Scope and the 700-line budget call them “the five lifecycle types” (`brief.md:41-50`). State the exact landed type list (and whether helper wire/error types count); otherwise the two-file budget and “every type” round-trip criterion have no fixed review surface.
- [ ] The declared prerequisite exists but does not match the brief’s base-state assertion: `dependency-state.json:2-5` records #715 as `PLANNED`, while the brief repeatedly says its result is already “merged” (`brief.md:2`, `brief.md:39-45`, `brief.md:86-89`). The supplied `origin/main` target reinforces the mismatch: `crates/core/src/multipart.rs:9-10` says `Budget` and `encode_record`/`decode_record` are still the next child, and those symbols are absent. Make the plan explicitly contingent on materializing #715’s merged result and require the citations/scope estimate to be refreshed on that base; the current target cannot support the planned dispatch-arm work.

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
- Iteration delta (if iterating): Auto-iterate (round 2): rebuilding for the implementation-level findings — C2 Reproduction (red pre-fix) — Accept criterion absence as the born-at-tier red oracle — with production reverted but the new imports retained at `crates/core/tests/multipart_session_records.rs:35`, the test exits 101 at compile time, so no pre-fix behavior can execute.; C4 Verification (red→green) — Accept compile-red→green as sufficient verification — the reverted slice cannot compile at `crates/core/tests/multipart_session_records.rs:35`, while the patched focused suite passes 24/24 and a second full `cargo xtask ci` run passes, leaving no executable old behavior to compare.; T4 Contribution — Decide whether review and contribution provenance is sufficient — affected-path merged history contains no prior lifecycle definitions, but closed/rejected work and the asserted four-finding batch-review failure cannot be mechanically settled because the `review-branch`/contribution scripts and artifacts are absent.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_716/review-b. 7 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-08-11

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 6 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
