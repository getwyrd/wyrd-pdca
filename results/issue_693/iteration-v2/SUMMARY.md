# Result — issue 693 / multipart-state-machine-digests

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: the records exist (previous children) but nothing answers a verb and
  nothing computes the object's identity. This child lands the **typed outcome
  vocabulary** (`InvalidPart`, `Backpressure`, `Refusal`, `CreateOutcome`,
  `ReserveOutcome`, `UploadPartOutcome`, `CompleteOutcome`, `AbortOutcome`,
  `Publication`), **decision 3's verb × state answer table as pure, total functions**, and
  the two digests — `multipart_etag` and `complete_fingerprint` — with `sha2` added to
  `crates/core`. After this child, every later slice answers in this vocabulary and no
  slice invents its own.
- Success criterion: the NEW file `crates/core/tests/multipart_state_machine.rs`
  passes. Every leg pure:
  1. **Every reachable decision-3 cell is answered by a pure function with a typed
     outcome.** Table-test the matrix at `0016:969-978` — `{UploadPart,
     CompleteMultipartUpload, AbortMultipartUpload, ListParts, ListMultipartUploads}` ×
     `{Open, Completing, Aborting, Completed(tombstone), absent}` = **25 cells**, each
     asserted as its typed outcome, never "an error" and never an HTTP status (the
     status/XML mapping is #508's). The two conditional cells get both branches: a
     `Completed` tombstone answers success-with-recorded-ETag **only** on a
     `complete_fingerprint` match, and not-found otherwise (`0016:898-908`). A helper
     enumerates the full product and fails on any unanswered cell, so a later verb or
     state cannot be added silently.
  2. **`multipart_etag` is the settled composition, proved against an independent
     oracle:** `lowercase_hex(SHA-256(d₁ ‖ … ‖ d_N)) + "-" + N` over the **raw 32-byte
     digests** in part-number order, `N` the named count — never MD5 (ADR-0047:73-89
     closed the basis; `:112` and `0016:3064-3070` deferred only the composition to
     here). The test computes the expectation itself from the digest bytes. Discriminating
     cases: N=1; a strict subset differs from the full set; hex-text concatenation differs
     from raw bytes; the `-N` suffix is the named count; **a non-ascending or duplicate
     part-number list is a typed error — never silently sorted** (the carried-forward v2
     finding at `multipart.rs:1903`: sorting erased request order; 0016 makes ascending
     part numbers a Complete *validation*, `0016:707`, `0016:994`, so the pure functions
     receive an already-ascending list or refuse).
  3. **`complete_fingerprint` distinguishes an identical retry from a different
     assembly** (`0016:898-908`): identical ascending lists agree; one changed digest
     disagrees; same digests under different part numbers disagree; a strict subset
     disagrees; a non-ascending or duplicate list is the same typed error as leg 2 —
     canonical order **is** the request order, and the request must be ascending (pinned).
  4. **`MultipartEtag` decode is validating:** parsing rejects a count suffix of 0, a
     count above `MAX_PART_NUMBER`, and a malformed hex or suffix — the count-vs-keyspace
     relational check the v2 review found missing (`multipart.rs:1844`).
  5. **The outcome enums are exhaustive:** no `#[non_exhaustive]` on any public outcome
     enum — assert by matching each without a wildcard arm in the test. (Pinned at Plan:
     every consumer is in-workspace (`Cargo.toml:40` `publish = false`); a new outcome
     variant MUST break every gateway wire-mapping table at compile time rather than fall
     into a `_ =>` arm that maps it to a silently wrong status — reliability over compile
     convenience, the human's explicit call, doubly so with multiple protocol gateways
     planned.)
- Repo + branch target: getwyrd/wyrd @ main   (INTEGRATION §2; base verified `339da46`)
- Scope: extend `crates/core/src/multipart.rs` with the outcome enums, `Verb`, the
  `*Answer` types, the per-verb answer functions + the total `answer` dispatcher,
  `canonical_named_parts` (validates ascending/duplicate-free, **refuses** otherwise),
  `MultipartEtag` (validating parse/serde per leg 4), `multipart_etag`,
  `complete_fingerprint`; add `sha2.workspace = true` to `crates/core/Cargo.toml` with a
  doc comment recording it is not a new dependency decision (`sha2 = "0.11"` is already a
  workspace dependency at `Cargo.toml:147`, used by `gateway-s3`/`server`, inside the
  `deny.toml` allowlist — ADR-0003's audit is not re-opened; `Cargo.lock` updates
  mechanically). **Plan decision pinned:** all public outcome enums exhaustive — no
  `#[non_exhaustive]` (rationale in leg 5).
  / out of scope: any store round trip (#656–#659); the S3 status/XML mapping (#508 — this
  child names no HTTP status); the knob values (#655); reaper/windows (#625);
  `metadata.rs`, `lib.rs`, `write.rs`, `custodian/` untouched; `docs/design/` untouched.

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
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 94.6% — 139 of 147 instrumentable changed lines executed (floor 80%); 147 of 695 changed lines were instru
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 35 mutants tested in 48s: 11 caught, 24 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_693/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.02s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Issue #693 adds the pure multipart verb-by-state outcome table and SHA-256 ETag/fingerprint identities; the implementation is sound, but its persisted-ETag correction conflicts with the brief's exact scope and the repository's same-PR docs rule.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | NEEDS-HUMAN | Decide whether to widen Plan to the existing record fixture and living architecture: the required whole recorded ETag changes persisted `Completion.etag`, but the brief fixes exactly four files and says `docs/design/` is untouched (`brief.md:102`, `brief.md:112`, `brief.md:115`; `crates/core/src/multipart.rs:1953`; `crates/core/tests/multipart_session_records.rs:293`; `AGENTS.md:154`). |
| C2 Reproduction (red pre-fix) | N/A | This is new functionality with no pre-existing behavioral repro; retaining the new test while stashing production fails on absent symbols before any test executes, exactly the declared born-at-tier limitation (`brief.md:57`, `brief.md:118`; `gate-logs/C4-verify.log:15`). |
| C3 Change | PASS | The patch implements all 25 typed state cells, the conditional tombstone retry, distinct object/request identities, strict named-part validation, and canonical ETag decoding without adding store or wire behavior (`crates/core/src/multipart.rs:3745`, `crates/core/src/multipart.rs:3791`, `crates/core/src/multipart.rs:3862`, `crates/core/src/multipart.rs:3893`, `crates/core/src/multipart.rs:4299`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Accept compile-only pre-fix evidence as sufficient for this born-at-tier API: no behavioral discriminator executes red, while the restored 22-test suite, 94.6% diff coverage, mutation run, and frozen full CI are green (`gate-logs/C4-verify.log:10`, `gate-logs/C4-verify.log:15`; `gate-logs/C4-diff-cov.log:69`; `gate-logs/C5-mutants.log:13`; `gate-logs/C4-ci.log:3497`). |
| C5 Causal adequacy | PASS | The tests directly discriminate every declared wrong cause—wrong `Completing` cell, hex-text hashing, omitted part numbers, and silently sorted input—and no capability probe or runtime guard masks a load-time cause (`crates/core/tests/multipart_state_machine.rs:339`, `crates/core/tests/multipart_state_machine.rs:504`, `crates/core/tests/multipart_state_machine.rs:586`, `crates/core/tests/multipart_state_machine.rs:532`). |
| T1 Structure | PASS | The dependency remains workspace-managed, production logic stays in the established multipart module, and the required always-on external test is isolated with `forbid(unsafe_code)` (`crates/core/Cargo.toml:23`, `crates/core/src/multipart.rs:3731`, `crates/core/tests/multipart_state_machine.rs:33`). |
| T2 Shape | PASS | Private `MultipartEtag` fields preserve validated construction, both digests share one strict order check, and every malformed list/grammar boundary surfaces as a typed error (`crates/core/src/multipart.rs:3745`, `crates/core/src/multipart.rs:3775`, `crates/core/src/multipart.rs:3791`). |
| T3 Runtime | PASS | The digest paths stream linearly over at most `MAX_PART_NUMBER` entries and the answer paths are constant-time pure functions, with no I/O, async work, clock, global state, or production call site (`crates/core/src/multipart.rs:3862`, `crates/core/src/multipart.rs:3893`, `crates/core/src/multipart.rs:4209`; `brief.md:135`). |
| T4 Contribution | FAIL | The persisted `Completion.etag` representation changed without the living-architecture update the hard docs-currency rule requires; the publish-artifact check is N/A until publish, and an independent affected-path check found only prerequisite merged work with no overlapping open/rejected implementation (`crates/core/src/multipart.rs:1953`; `docs/design/architecture/05-building-block-view.md:202`; `AGENTS.md:154`; `gate-logs/T4-contribution.log:10`). |
| T5 Judgment | PASS | No further implementation defect remains: the stored retry answer preserves the complete ETag, changed part numbers alter the fingerprint, `MAX_PART_NUMBER` is accepted, and malformed/out-of-range spellings are rejected (`crates/core/tests/multipart_state_machine.rs:422`, `crates/core/tests/multipart_state_machine.rs:586`, `crates/core/tests/multipart_state_machine.rs:649`, `crates/core/tests/multipart_state_machine.rs:670`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether this typed vocabulary and digest composition are fit as the durable contract for downstream wire/store slices—there are intentionally no production consumers yet, so automated evidence cannot validate integration ergonomics (`brief.md:99`, `brief.md:135`; `crates/core/src/multipart.rs:3903`). |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C1 Spec — Decide whether to widen Plan to the existing record fixture and living architecture: the required whole recorded ETag changes persisted `Completion.etag`, but the brief fixes exactly four files and says `docs/design/` is untouched (`brief.md:102`, `brief.md:112`, `brief.md:115`; `crates/core/src/multipart.rs:1953`; `crates/core/tests/multipart_session_records.rs:293`; `AGENTS.md:154`).
- [ ] C4 Verification (red→green) — Accept compile-only pre-fix evidence as sufficient for this born-at-tier API: no behavioral discriminator executes red, while the restored 22-test suite, 94.6% diff coverage, mutation run, and frozen full CI are green (`gate-logs/C4-verify.log:10`, `gate-logs/C4-verify.log:15`; `gate-logs/C4-diff-cov.log:69`; `gate-logs/C5-mutants.log:13`; `gate-logs/C4-ci.log:3497`).
- [ ] Validation — fitness-to-purpose — Decide whether this typed vocabulary and digest composition are fit as the durable contract for downstream wire/store slices—there are intentionally no production consumers yet, so automated evidence cannot validate integration ergonomics (`brief.md:99`, `brief.md:135`; `crates/core/src/multipart.rs:3903`).
- [ ] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_693/review-b
- [ ] T5 Judgment — Confirm affected-path prior art across merged and closed/rejected work — the permitted target contains only one commit, one local ref, and no PR metadata, so avoiding duplicate or previously rejected work cannot be mechanically settled here.

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
- Iteration delta (if iterating): Auto-iterate (round 2): rebuilding for the implementation-level findings — C4 Verification (red→green) — Accept compile-only pre-fix evidence as sufficient for this born-at-tier API: no behavioral discriminator executes red, while the restored 22-test suite, 94.6% diff coverage, mutation run, and frozen full CI are green (`gate-logs/C4-verify.log:10`, `gate-logs/C4-verify.log:15`; `gate-logs/C4-diff-cov.log:69`; `gate-logs/C5-mutants.log:13`; `gate-logs/C4-ci.log:3497`).; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_693/review-b. 3 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-09-12

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
