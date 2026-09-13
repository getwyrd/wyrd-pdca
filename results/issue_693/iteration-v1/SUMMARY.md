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
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 91.2% — 135 of 148 instrumentable changed lines executed (floor 80%); 148 of 631 changed lines were instru
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 32 mutants tested in 50s: 2 missed, 9 caught, 21 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_693/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.00s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of issue #693's pure multipart verb-by-state outcomes and identity digests: rebuild before acceptance because retry publication loses the multipart ETag suffix, large canonical counts are misclassified, and the tests miss both contract boundaries.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The normative contract pins all 25 lifecycle cells, exact digest composition, validating decode, and exhaustive public outcomes, including recorded-ETag retry semantics (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:894`). |
| C2 Reproduction (red pre-fix) | N/A | This is born-at-tier functionality with no pre-existing API to exercise; stashing production left the new test unable to compile at its added imports rather than producing a behavioral red (`crates/core/tests/multipart_state_machine.rs:28`; `gate-logs/C4-verify.log:15`). |
| C3 Change | FAIL | An identical completed retry must return the exact recorded `<hex>-N` token, but `Publication` carries only `Digest` and therefore cannot represent the named-count suffix (`crates/core/src/multipart.rs:3942`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Accept the pre-declared born-at-tier limitation — both the frozen gate and an independent stash/pop rerun produced compile-only red (101) and 21-test green, so no behavioral pre-fix discriminator executed (`gate-logs/C4-verify.log:10`; `gate-logs/C4-verify.log:82`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Rebuild the tests to require the full composed retry ETag and accept exactly `MAX_PART_NUMBER` — the current oracle repeats the digest-only shape and mutation testing preserves the maximum-boundary regression (`crates/core/tests/multipart_state_machine.rs:126`; `gate-logs/C5-mutants.log:14`). |
| T1 Structure | PASS | The four prescribed files isolate pure digest and lifecycle logic in the existing multipart module with an always-on integration test, within the 950-semantic-line budget (`crates/core/src/multipart.rs:3705`; `crates/core/tests/multipart_state_machine.rs:1`). |
| T2 Shape | FAIL | Canonical counts above `u32::MAX` should reach the typed `EtagPartCountOutOfRange { parts: u64 }` path, but parsing as `u32` first collapses them into malformed syntax and defeats the public error taxonomy (`crates/core/src/multipart.rs:533`; `crates/core/src/multipart.rs:3763`). |
| T3 Runtime | N/A | Production reach is intentionally deferred: these are pure synchronous functions with no store, async, or current runtime call sites (`crates/core/src/multipart.rs:4147`). |
| T4 Contribution | FAIL | The multi-pass contribution review's two unique blockers both ground in the public contract; the absent PR-description audit is correctly deferred and remains N/A until its mandatory publish rerun (`gate-logs/T4-batch-review.log:10`; `gate-logs/T4-contribution.log:10`). |
| T5 Judgment | NEEDS-HUMAN | Confirm affected-path prior art across merged and closed/rejected work — the permitted target contains only one commit, one local ref, and no PR metadata, so avoiding duplicate or previously rejected work cannot be mechanically settled here. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the corrected record/outcome model and born-at-tier evidence are sufficient for downstream gateway/store consumers — this vocabulary is their durable contract, and a wrong retry token becomes a client-visible silent success. |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Accept the pre-declared born-at-tier limitation — both the frozen gate and an independent stash/pop rerun produced compile-only red (101) and 21-test green, so no behavioral pre-fix discriminator executed (`gate-logs/C4-verify.log:10`; `gate-logs/C4-verify.log:82`).
- [ ] C5 Causal adequacy — Rebuild the tests to require the full composed retry ETag and accept exactly `MAX_PART_NUMBER` — the current oracle repeats the digest-only shape and mutation testing preserves the maximum-boundary regression (`crates/core/tests/multipart_state_machine.rs:126`; `gate-logs/C5-mutants.log:14`).
- [ ] T5 Judgment — Confirm affected-path prior art across merged and closed/rejected work — the permitted target contains only one commit, one local ref, and no PR metadata, so avoiding duplicate or previously rejected work cannot be mechanically settled here.
- [ ] Validation — fitness-to-purpose — Decide whether the corrected record/outcome model and born-at-tier evidence are sufficient for downstream gateway/store consumers — this vocabulary is their durable contract, and a wrong retry token becomes a client-visible silent success.
- [ ] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_693/review-b

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
- Iteration delta (if iterating): Auto-iterate (round 1): rebuilding for the implementation-level findings — C4 Verification (red→green) — Accept the pre-declared born-at-tier limitation — both the frozen gate and an independent stash/pop rerun produced compile-only red (101) and 21-test green, so no behavioral pre-fix discriminator executed (`gate-logs/C4-verify.log:10`; `gate-logs/C4-verify.log:82`).; C5 Causal adequacy — Rebuild the tests to require the full composed retry ETag and accept exactly `MAX_PART_NUMBER` — the current oracle repeats the digest-only shape and mutation testing preserves the maximum-boundary regression (`crates/core/tests/multipart_state_machine.rs:126`; `gate-logs/C5-mutants.log:14`).; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_693/review-b. 2 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-09-12

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
