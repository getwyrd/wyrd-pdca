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
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 94.6% — 139 of 147 instrumentable changed lines executed (floor 80%); 147 of 703 changed lines were instru
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 35 mutants tested in 48s: 11 caught, 24 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_693/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 12.94s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Reviewing issue #693's pure multipart state-answer vocabulary and digest identities; the implementation evidence is technically green, with human decisions still owed on the Plan scope expansion, prior art, and downstream fitness.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The normative 25-cell table, exact ETag formula, fingerprint discriminators, validating grammar, and exhaustive-enum rule make the requested contract decision-complete and falsifiable (`brief.md:19`, `brief.md:31`, `brief.md:42`, `brief.md:47`). |
| C2 Reproduction (red pre-fix) | N/A | This is born-at-tier functionality with no pre-existing callable behavior, so the brief explicitly declares ordinary behavioral reproduction unavailable (`brief.md:118`, `brief.md:131`). |
| C3 Change | NEEDS-HUMAN | Decide whether to approve the six-file expansion beyond the pinned four-file scope and its explicit `docs/design/` exclusion so the whole-ETag persisted-field change can update its existing round-trip test and mandatory living documentation — otherwise the accepted Plan and repository policy remain inconsistent (`brief.md:114`, `brief.md:115`, `crates/core/src/multipart.rs:1954`, `crates/core/tests/multipart_session_records.rs:293`, `docs/design/architecture/05-building-block-view.md:204`, `AGENTS.md:154`). |
| C4 Verification (red→green) | PASS | The already recorded born-at-tier exception remains satisfied: the frozen and independent reruns both fail compilation before any pre-fix test executes and pass all 22 tests after restoration, while full CI is green (`brief.md:166`, `gate-logs/C4-verify.log:10`, `gate-logs/C4-verify.log:162`, `gate-logs/C4-ci.log:3497`). |
| C5 Causal adequacy | PASS | The change implements the causes directly through strict request-order validation, raw-digest ETag composition, numbered-pair fingerprinting, and exhaustive state dispatch; all 11 viable diff mutants were caught (`crates/core/src/multipart.rs:3746`, `crates/core/src/multipart.rs:3864`, `crates/core/src/multipart.rs:3895`, `crates/core/src/multipart.rs:4303`, `gate-logs/C5-mutants.log:13`). |
| T1 Structure | PASS | The pure protocol logic stays in the existing core multipart module, with only the already-workspace-scoped hash dependency and no store, async, global-state, or concrete-backend coupling (`crates/core/Cargo.toml:23`, `crates/core/src/multipart.rs:3732`). |
| T2 Shape | PASS | Private-field validated identities, typed refusal/outcome enums, and wildcard-free answer types preserve invalid-state exclusion and force downstream mappings to handle future variants deliberately (`crates/core/src/multipart.rs:3777`, `crates/core/src/multipart.rs:3953`, `crates/core/src/multipart.rs:4193`). |
| T3 Runtime | PASS | Both digests are single-pass O(N) computations over borrowed validated input with incremental hashing and no store or network work, so runtime risk is bounded to the named-part count (`crates/core/src/multipart.rs:3864`, `crates/core/src/multipart.rs:3895`). |
| T4 Contribution | N/A | Contribution artifacts are absent by design at Check and the mandatory publish gate will audit them later; the separate batch review found zero blockers (`gate-logs/T4-contribution.log:10`, `gate-logs/T4-batch-review.log:10`). |
| T5 Judgment | NEEDS-HUMAN | Confirm that the prior-art search covered all six affected paths in merged history and closed/rejected work — the brief records a symbol/open-PR check, but the disposable target has one synthetic commit and no refs or remote, so this reviewer cannot mechanically settle duplication risk (`brief.md:146`, `brief.md:149`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Approve the durable public vocabulary and changed `Completed.etag` grammar as the right contract for #508 and #656–#659 — those consumers do not exist yet, so green pure-function evidence cannot validate their eventual wire/store integration (`crates/core/src/multipart.rs:1936`, `crates/core/src/multipart.rs:4031`). |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C3 Change — Decide whether to approve the six-file expansion beyond the pinned four-file scope and its explicit `docs/design/` exclusion so the whole-ETag persisted-field change can update its existing round-trip test and mandatory living documentation — otherwise the accepted Plan and repository policy remain inconsistent (`brief.md:114`, `brief.md:115`, `crates/core/src/multipart.rs:1954`, `crates/core/tests/multipart_session_records.rs:293`, `docs/design/architecture/05-building-block-view.md:204`, `AGENTS.md:154`).
- [x] T5 Judgment — Confirm that the prior-art search covered all six affected paths in merged history and closed/rejected work — the brief records a symbol/open-PR check, but the disposable target has one synthetic commit and no refs or remote, so this reviewer cannot mechanically settle duplication risk (`brief.md:146`, `brief.md:149`).
- [x] Validation — fitness-to-purpose — Approve the durable public vocabulary and changed `Completed.etag` grammar as the right contract for #508 and #656–#659 — those consumers do not exist yet, so green pure-function evidence cannot validate their eventual wire/store integration (`crates/core/src/multipart.rs:1936`, `crates/core/src/multipart.rs:4031`).
- [x] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [x] size backstop — this slice is behaving oversized: 2 round(s) already spent (threshold 2). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat. Human decision: accepting the size/scope overrun rather than iterate-plan.
- [x] T5 Judgment — Confirm affected-path prior art across merged and closed/rejected work — the permitted target contains only one commit, one local ref, and no PR metadata, so avoiding duplicate or previously rejected work cannot be mechanically settled here.
- [x] C1 Spec — Decide whether to widen Plan to the existing record fixture and living architecture: the required whole recorded ETag changes persisted `Completion.etag`, but the brief fixes exactly four files and says `docs/design/` is untouched (`brief.md:102`, `brief.md:112`, `brief.md:115`; `crates/core/src/multipart.rs:1953`; `crates/core/tests/multipart_session_records.rs:293`; `AGENTS.md:154`).

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
- By / date: Eduard Ralph / 2026-09-12

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
