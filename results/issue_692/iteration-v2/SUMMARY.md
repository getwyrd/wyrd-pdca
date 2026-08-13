# Result — issue 692 / multipart-record-family

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: 
- Success criterion: the NEW file `crates/core/tests/multipart_records.rs` passes.
  Every leg pure — no store, no async:
  1. **Every record decode is validating** (ADR-0045): each record type round-trips
     `encode`/`decode`, and a structurally invalid value is rejected at decode with a
     typed error. **Five relational identity checks are BINDING, each proven with a
     hand-authored torn value** (all five are carried-forward MUST-FIXes from the v2
     sign-off and its batch review):
     (a) an `AdmissionRecord` whose `max_sessions` disagrees with what its own stored
     `profile` tuple derives is rejected (`0016:1469-1470`: `U_ref` and
     `MAX_SESSIONS = ⌊W_ref/U_ref⌋` are functions of the tuple, implemented HERE);
     (b) `decode_owned_entry` **takes the `sidx:` key** and rejects a payload whose
     `owner` differs from the key's upload id (v2 review, `multipart.rs:1555`);
     (c) a `SessionRecord` whose `publish_target.parent`/`name` disagrees with the
     session's own `parent`/`object` is rejected (v2 review, `multipart.rs:1258`);
     (d) `decode_retire_obligation` **takes the `retire:` key** and rejects a payload
     whose mode or generation identity disagrees with the key's token — a
     generation-scoped payload under a session token, and vice versa, are both errors
     (v2 review, `multipart.rs:1789/1800`; the archived test at
     `multipart_records.rs:1108` *affirmed* this case — it must now reject);
     (e) a `PendingEntry` with exactly one of `owner`/`staged` is **torn** and rejected at
     decode, under both a `pending:` and a `sidx:` reading (v2 review,
     `metadata.rs:1537/1541`) — both-absent (legacy) and both-present (owned) are the only
     valid shapes.
  2. **Decode→encode is the identity on a legacy value:** a legacy `pending:` value with
     neither new field re-encodes **byte-identically** — the `skip_serializing_if`
     identity every `require(key, encode(prior))` CAS in `metadata.rs` depends on
     (`metadata.rs:1368-1391`, ADR-0047:38-50).
  3. **The identity/occupancy boundary holds:** a decoded `AdmissionRecord` whose `count`
     exceeds its `max_sessions` still **decodes** — occupancy above a lowered cap is a
     legitimate live state that admission (a later slice) refuses to grow, not a decode
     error; rejecting it would make a durable record unreadable the day the profile is
     lowered (the `MAX_ROOT_SEGMENTS` boundary, `metadata.rs:312-321`). Assert it decodes.
     Identity relations (two stored spellings of one quantity — legs 1a–1e) are binding;
     occupancy relations are not. This settles the v2 reviewer's "count above the derived
     admission cap" finding the other way, deliberately, at Plan.
- Repo + branch target: getwyrd/wyrd @ main   (INTEGRATION §2; base verified `339da46`)
- Scope (one logical fix) / out of scope: extend `crates/core/src/multipart.rs` with the record family and validating
  decoders listed under Goal, including `encode_record`/`decode_record` and the five
  relational checks; **key-taking decode APIs** for the two records whose identity lives
  partly in the key (`decode_owned_entry(key, bytes)`, `decode_retire_obligation(key,
  bytes)`) — a decode that cannot see the key cannot validate against it, which is exactly
  how v2's shape failed its review. In `crates/core/src/metadata.rs`, **ONE allowance**:
  `PendingEntry` (`metadata.rs:1528`) gains `owner: Option<multipart::UploadId>` and
  `staged: Option<multipart::StagedPlacement>` (`#[serde(default, skip_serializing_if =
  "Option::is_none")]`, doc comments per the v2 hunk), drops `Copy` (forced — `UploadId`
  is a `String` newtype), and gains the torn-shape rejection (leg 1e) in a manual
  `Deserialize`; nothing else in that file changes. **The mechanical ripple is explicitly
  ALLOWED (this Plan's answer to the v2 C1 finding that the 5-file ceiling was
  unbuildable):** the 8 files that construct or copy `PendingEntry` —
  `crates/core/src/write.rs`, `crates/core/tests/mutation_regressions.rs`,
  `crates/custodian/tests/{gc,restore_reconcile,segmented_map_consumers}.rs`,
  `crates/dst/tests/custodian.rs`, `crates/metadata-redb/tests/conformance.rs`,
  `crates/server/tests/custodian_gc.rs` — may each gain the mechanical `owner: None,
  staged: None` initializer lines / clone-instead-of-copy fixes, **≤ 8 changed lines per
  file, no logic change, no new function**. A ninth ripple file or a non-mechanical hunk
  in any of them means the seam is wrong: STOP and hand back.
  / out of scope: the outcome enums, answer table, `MultipartEtag`, digests and `sha2`
  (child-3's — no `Cargo.toml`/`Cargo.lock` change here); the knob **values** (#655);
  every store round trip (#656–#659); reaper/windows (#625); custodian **source** code —
  `crates/custodian/src/` untouched (only its *tests'* initializers); `docs/design/`
  untouched.

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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 114 mutants tested in 3m: 83 caught, 31 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_692/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — scripts/pdca contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Reviewing #692: add the pure multipart record family, validating decoders, and legacy-safe `PendingEntry` ownership fields.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | NEEDS-HUMAN | Decide whether a no-writer schema slice is exempt from the living-architecture rule—the brief forbids `docs/design/` edits while two persisted fields are added, and the target calls same-PR documentation a merge requirement (`AGENTS.md:154`, `crates/core/src/metadata.rs:1560`). |
| C2 Reproduction (red pre-fix) | PASS | Criterion absence is independently reproducible: adding only the required test to base `9dbcd72` fails Cargo with exit 101 because the imported record/decoder API does not exist (`crates/core/tests/multipart_records.rs:29`). |
| C3 Change | PASS | The patch stays on the specified 11 paths, keeps the eight ripple files mechanical and at or below eight added lines each, and introduces the substantive codec at the planned seam (`crates/core/src/multipart.rs:878`, `crates/core/src/metadata.rs:1553`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Decide whether born-at-tier compile-red plus six independently failing negations is an acceptable substitute for behavioral red→green—the restored patch passes all 18 focused tests, every `xtask ci` component, and all 114 in-diff mutants, but no pre-fix behavioral test can execute (`crates/core/tests/multipart_records.rs:389`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Rebuild must extend the decode boundary to nested EC geometry and zero-lifetime slots—the adversarial probe accepted `ReedSolomon { k: 0, m: 1 }` and `lease_expiry == reserved_at`, so structurally invalid durable records still become values (`crates/core/src/multipart.rs:1503`, `crates/core/src/multipart.rs:1657`). |
| T1 Structure | FAIL | The review footprint exceeds the brief's cap: 1,389 added nonblank/noncomment lines versus at most 1,250, despite using exactly the prescribed paths (`crates/core/src/multipart.rs:875`, `crates/core/tests/multipart_records.rs:1`). |
| T2 Shape | FAIL | The persisted shape is not fully validating: `PartRecordWire` accepts raw `ChunkRef` values and `StagedPlacement` derives unchecked `EcScheme` deserialization, contrary to the target's structural-decode rule (`crates/core/src/multipart.rs:1538`, `crates/core/src/multipart.rs:1657`, `AGENTS.md:146`). |
| T3 Runtime | PASS | The change remains pure—no store call, async path, or external service—and focused plus workspace execution completed locally (`crates/core/src/multipart.rs:62`, `crates/core/tests/multipart_records.rs:3`). |
| T4 Contribution | NEEDS-HUMAN | Human must settle contribution readiness after inspecting the four unavailable batch-review blockers and affected-path rejected-attempt artifacts—the merged path history and closed PR #647 were checked, but `scripts/review-branch` and the archived attempts are not present, so the required recorded-resolution audit cannot be reproduced (`AGENTS.md:206`). |
| T5 Judgment | NEEDS-HUMAN [impl] | Rebuild must replace the test that endorses an already-lapsed equality slot and add invalid-geometry cases—the current suite asserts `reserved_at == lease_expiry` decodes and probes only valid EC geometry (`crates/core/tests/multipart_records.rs:630`, `crates/core/tests/multipart_records.rs:673`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether this record contract is fit for the later store/admission consumers given the accepted malformed EC values, zero-lifetime slot, documentation-scope conflict, and non-behavioral RED posture—those choices determine whether durable state remains safely actionable. |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C1 Spec — Decide whether a no-writer schema slice is exempt from the living-architecture rule—the brief forbids `docs/design/` edits while two persisted fields are added, and the target calls same-PR documentation a merge requirement (`AGENTS.md:154`, `crates/core/src/metadata.rs:1560`).
- [ ] C4 Verification (red→green) — Decide whether born-at-tier compile-red plus six independently failing negations is an acceptable substitute for behavioral red→green—the restored patch passes all 18 focused tests, every `xtask ci` component, and all 114 in-diff mutants, but no pre-fix behavioral test can execute (`crates/core/tests/multipart_records.rs:389`).
- [ ] C5 Causal adequacy — Rebuild must extend the decode boundary to nested EC geometry and zero-lifetime slots—the adversarial probe accepted `ReedSolomon { k: 0, m: 1 }` and `lease_expiry == reserved_at`, so structurally invalid durable records still become values (`crates/core/src/multipart.rs:1503`, `crates/core/src/multipart.rs:1657`).
- [ ] T4 Contribution — Human must settle contribution readiness after inspecting the four unavailable batch-review blockers and affected-path rejected-attempt artifacts—the merged path history and closed PR #647 were checked, but `scripts/review-branch` and the archived attempts are not present, so the required recorded-resolution audit cannot be reproduced (`AGENTS.md:206`).
- [ ] T5 Judgment — Rebuild must replace the test that endorses an already-lapsed equality slot and add invalid-geometry cases—the current suite asserts `reserved_at == lease_expiry` decodes and probes only valid EC geometry (`crates/core/tests/multipart_records.rs:630`, `crates/core/tests/multipart_records.rs:673`).
- [ ] Validation — fitness-to-purpose — Decide whether this record contract is fit for the later store/admission consumers given the accepted malformed EC values, zero-lifetime slot, documentation-scope conflict, and non-behavioral RED posture—those choices determine whether durable state remains safely actionable.
- [ ] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_692/review-b
- [ ] size backstop — this slice is behaving oversized: patch is 104 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Plan
- Iteration delta (if iterating): Slice is oversized (106,723 bytes vs. 100KB threshold, 11 files) and it shows: T4 batched review gate fails with 4 blocking findings (budget/session-token validation gaps), plus substantive NEEDS-HUMAN findings on causal adequacy (decode still accepts degenerate ReedSolomon{k:0,m:1} and zero-lifetime lease_expiry==reserved_at) and judgment (test suite endorses the same lapsed-lease case it should reject). These are implementation-shaped findings consistent with the size backstop's own recommendation. Re-plan and split via `pdca split 692` rather than attempting another iterate-do on this cut.
- By / date: Eduard Ralph / 2026-08-08

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
