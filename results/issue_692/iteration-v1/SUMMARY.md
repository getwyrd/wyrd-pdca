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
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): fail — xtask: `typos` found misspellings (exit exit status: 2). Fix them, or record a deliberate exception in typos.toml (keep 
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: unverifiable —                why this slice has no isolable red (the cargo output is above).
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 96 mutants tested in 3m: 2 missed, 64 caught, 30 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_692/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — scripts/pdca contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Reviewing issue #692’s pure multipart record family and validating decoders, including legacy `PendingEntry` compatibility and key/value identity checks.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The normative contract separates structural decode validity from live-cap policy and fixes the derived admission formula, so acceptance is testable (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:390`, `docs/design/proposals/draft/0016-multipart-commit-protocol.md:1470`). |
| C2 Reproduction (red pre-fix) | PASS | Retaining the new test while reverting production fails to compile on its missing record APIs, while five relational mutations are caught and removing legacy omission makes the byte-identity test fail (`crates/core/tests/multipart_records.rs:29`, `crates/core/tests/multipart_records.rs:603`). |
| C3 Change | PASS | The scoped change implements all five binding identity boundaries directly at decode, including key-aware ownership/retirement checks and torn `PendingEntry` rejection (`crates/core/src/multipart.rs:1105`, `crates/core/src/multipart.rs:1350`, `crates/core/src/multipart.rs:1651`, `crates/core/src/multipart.rs:1889`, `crates/core/src/metadata.rs:1583`). |
| C4 Verification (red→green) | FAIL | The functional red→green leg is real and the restored test passes 13/13, but merge verification remains red because `cargo xtask ci` independently reports `entrys` as a spelling error (`crates/core/tests/multipart_records.rs:533`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Rebuild must pin both safety branches—the `SCAN_CAP/2` clamp and generation-source mutual exclusion—because `cargo mutants --in-diff` leaves those two mutations alive (`crates/core/src/multipart.rs:1020`, `crates/core/src/multipart.rs:1844`). |
| T1 Structure | PASS | The patch stays within the exact 11-file seam, keeps each of the eight named ripple files mechanical and within eight changed lines, and adds 1,243 code-bearing lines against the 1,250-line semantic budget (`crates/core/src/metadata.rs:1526`, `crates/core/src/multipart.rs:878`). |
| T2 Shape | PASS | Key-derived identity is visible to the two decoders that require it, while absent optional fields preserve the shared persisted wire shape (`crates/core/src/multipart.rs:1648`, `crates/core/src/multipart.rs:1882`, `crates/core/src/metadata.rs:1560`). |
| T3 Runtime | N/A | This slice deliberately has no store call, writer, production consumer, or async path, so runtime/topology judgment belongs to the later persistence slices (`crates/core/src/multipart.rs:7`, `crates/core/src/multipart.rs:60`). |
| T4 Contribution | NEEDS-HUMAN | Human must settle contribution readiness after inspecting the three recorded batch-review blockers and closed/rejected affected-path history—the `scripts/review-branch` runner, its log, and archived attempts were not supplied, although merged history shows only the prerequisite key grammar. |
| T5 Judgment | NEEDS-HUMAN [impl] | Rebuild must reject a generation obligation with neither chunks nor segments—the adversarial probe is currently accepted, allowing an obligation that owes nothing despite the explicit-error convention (`crates/core/src/multipart.rs:1841`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Human must decide whether pure, dormant record vocabulary is a fit handoff for the downstream persistence slices, because this cycle intentionally has no production writer or consumer to demonstrate operational usefulness (`crates/core/src/multipart.rs:60`). |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Rebuild must pin both safety branches—the `SCAN_CAP/2` clamp and generation-source mutual exclusion—because `cargo mutants --in-diff` leaves those two mutations alive (`crates/core/src/multipart.rs:1020`, `crates/core/src/multipart.rs:1844`).
- [ ] T4 Contribution — Human must settle contribution readiness after inspecting the three recorded batch-review blockers and closed/rejected affected-path history—the `scripts/review-branch` runner, its log, and archived attempts were not supplied, although merged history shows only the prerequisite key grammar.
- [ ] T5 Judgment — Rebuild must reject a generation obligation with neither chunks nor segments—the adversarial probe is currently accepted, allowing an obligation that owes nothing despite the explicit-error convention (`crates/core/src/multipart.rs:1841`).
- [ ] Validation — fitness-to-purpose — Human must decide whether pure, dormant record vocabulary is a fit handoff for the downstream persistence slices, because this cycle intentionally has no production writer or consumer to demonstrate operational usefulness (`crates/core/src/multipart.rs:60`).
- [ ] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [ ] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `typos` found misspellings (exit exit status: 2). Fix them, or record a deliberate exception in typos.toml (keep 
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_692/review-b

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
- Iteration delta (if iterating): Auto-iterate (round 1): rebuilding for the implementation-level findings — C5 Causal adequacy — Rebuild must pin both safety branches—the `SCAN_CAP/2` clamp and generation-source mutual exclusion—because `cargo mutants --in-diff` leaves those two mutations alive (`crates/core/src/multipart.rs:1020`, `crates/core/src/multipart.rs:1844`).; T4 Contribution — Human must settle contribution readiness after inspecting the three recorded batch-review blockers and closed/rejected affected-path history—the `scripts/review-branch` runner, its log, and archived attempts were not supplied, although merged history shows only the prerequisite key grammar.; T5 Judgment — Rebuild must reject a generation obligation with neither chunks nor segments—the adversarial probe is currently accepted, allowing an obligation that owes nothing despite the explicit-error convention (`crates/core/src/multipart.rs:1841`).; C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `typos` found misspellings (exit exit status: 2). Fix them, or record a deliberate exception in typos.toml (keep ; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_692/review-b. 1 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-08-08

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
