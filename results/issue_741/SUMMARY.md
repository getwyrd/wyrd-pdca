# Result — issue 741 / validate-s3-client-layer

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: decomposed instead of built as one cycle: the slice was judged to be more
  than one shippable outcome. The seams are set out in `split-proposal.md`; the
  original defect and scope are in `iteration-v3/brief.md`.
- Success criterion: the slice is decomposed, not built here — the child bundles
  issue_852, issue_853, issue_854 each carry their own brief, and together they cover
  the goal of `iteration-v3/brief.md`. No patch lands in this bundle; each child is verified
  by its own cycle.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: decomposition only: no patch, test or gate run belongs to this bundle. / out
  of scope: building any part of the original slice here — the child bundles
  carry that work.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: split
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — N/A — close disposition (no patch to verify)
- C3 Change: none — patch.diff
- C4 Verification (red→green): none — N/A — close disposition (no patch to verify)
- C5 Causal adequacy: none — reviewer + human sign-off

## 4. Conformance (Check — stack)
- T1 Structure: none — N/A — close disposition (no patch to verify)
- T2 Shape: none — N/A — close disposition (no patch to verify)
- T3 Runtime: none — N/A — close disposition (no patch to verify)
- T4 Contribution: none — N/A — close disposition (no patch to verify)
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

# Advisory review — SKIPPED (close disposition)

The reviewer leaf was skipped: this bundle's Plan concluded a close / no-fix disposition (split), so there is no patch to review.

- NEEDS-HUMAN — Confirm the close disposition 'split' (no patch was built). Override to a fix path (iterate-to-Do) if the close is wrong.


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] Confirm the close disposition 'split' (no patch was built). Override to a fix path (iterate-to-Do) if the close is wrong.
- [x] `deny.toml` / `deny-all-features.toml` (the old RUSTSEC-2026-0253 entry, base `deny.toml:86`): the brief said to keep the waiver, rewrite its rationale, and record that the maintainer accepted an unsound `lru` in a shipped binary. The patch deletes the waiver instead. Checked: the deletion is correct. The base lockfile already resolves `aws-sdk-s3` 1.148.0 (`Cargo.lock:210-211`) and `lru` 0.18.4 (`Cargo.lock:2139-2140`). `aws-sdk-s3` 1.144.0+ requires `lru ^0.18.2`, while 1.142.0 still required `^0.16.3`. So the advisory matched nothing before this patch, and the waiver's own removal trigger had already fired. (I could not check 1.143.0, so the "first release" claim at root `Cargo.toml:94` is unverified. The 1.144.0 floor is safe either way.) What a human must decide: the brief's header asks sign-off §9 to confirm and post to #741 that the RUSTSEC-2026-0253 exposure was *accepted*. That sentence would now be false. No unsound `lru` ships. The tracker note should say the exposure is moot, and the departure from the brief should be recorded as deliberate.
- [x] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [x] The brief assumes a base that does not exist yet. It says the crate “DOES exist pre-patch” and must build on “#740's accepted result” (`brief.md:49-52`, `brief.md:67-69`), but the declared prerequisite is only `PLANNED` (`dependency-state.json:2-5`), and the resolved `origin/main` target has no `crates/validate` member at all (`Cargo.toml:9-32`). On this target, `cargo test -p wyrd-validate ...` cannot run. Revise the target/ordering to require a materialized #740 result rather than treating planned work as the pre-patch tree.
- [x] The brief suppresses the tracker’s load-bearing dependency decision without tracker evidence. The only recorded maintainer comment says the shipped `aws-sdk-s3` move “needs the audit **before** the crate lands” and “Confirm before starting” (`notes.json:1`); the brief instead declares the exposure “SETTLED,” says its audit is merely a slice deliverable, and orders Do not to reopen it (`brief.md:10-14`, `brief.md:243-270`). Add the missing recorded acceptance or retain the human dependency/license decision as unresolved.
- [x] Criterion 3 does not yet falsify the claimed *bidirectional client* memory invariant. Its permitted observables are the largest buffer requested from the PUT source or live generated PUT bytes (`brief.md:35-39`, `brief.md:221-226`); reading GET output incrementally can still pass if the client first collects the whole response and then yields small chunks. The target source guarantees only that the **gateway** streams (`crates/gateway-s3/src/lib.rs:12-17`), not that this new client does. Require a concrete aggregation-sensitive oracle for PUT and GET, and require the deliberate-buffering mutation to fail on each direction.
- [x] The scope contains an optional drive-by change that breaks its own parallel-work claim. It says this slice touches `crates/validate/**`, root `Cargo.toml`, and `deny.toml`, disjoint from #738’s `crates/server/**` (`brief.md:70-76`), but later recommends deduplicating the server’s SDK dependency even though that is “not required by the criterion” (`brief.md:323-325`). The target’s pins are inline in `crates/server/Cargo.toml:126-130`, so deduplication necessarily adds the very `crates/server/**` edit the brief said was excluded. Remove that cleanup or declare the overlap and scope explicitly.
- [x] `crates/validate/src/client.rs:212-219`: **a complete chunked GET is refused** as `S3Error::Body` ("declared no Content-Length"). Reproduced: `200 OK, Transfer-Encoding: chunked`, body `3\r\nhel\r\n0\r\n\r\n` → `Err(Body{…})`. The refusal fixes the previous round's close-delimited torn-body finding, but it is wider than that finding needed. Chunked framing marks its own end, hyper already enforces the terminal chunk, and the rubric names "the chunked terminal CRLF" as acceptable framing. Real S3 and the Wyrd gateway always send `Content-Length`, so this only bites behind a proxy that re-frames responses. A human should decide whether the validator calls such a deployment "untrustworthy" or narrows the refusal to bodies framed only by connection close.
- [x] `deny.toml:76` / `deny-all-features.toml:103` / root `Cargo.toml:94-98`: **the patch deletes the RUSTSEC-2026-0253 waiver instead of rewriting its rationale as the brief ordered** (`brief.md:356-374`: "replace" the rationale, record the 2026-08-17 acceptance, "leave the existing REMOVAL TRIGGER intact"). It also pins `aws-sdk-s3` at a 1.144.0 floor, not the 1.137.0 the brief named (`brief.md:352-354`). I checked the facts and the deviation looks correct. The base `Cargo.lock` already resolves only `lru 0.18.4` under `aws-sdk-s3 1.148.0`, so the waiver had already met its own removal trigger before this patch. In the local registry, `aws-sdk-s3` 1.142.0 depends on `lru 0.16.3` and 1.144.0 on `0.18.2`; 1.143.0 was not available locally to check. C4-ci shows `cargo deny` advisories ok on both configs. Still, the brief's premise ("aws-sdk-s3 pins `lru ^0.16.3`") was stale, so the sign-off item "confirm Do … replaced the waiver rationale" now has nothing to confirm. The maintainer's acceptance survives only as a Cargo.toml comment (`Cargo.toml:86`), and `deny-all-features.toml` sits outside the brief's stated file set. Sign-off should accept the deviation explicitly.

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
- By / date: Eduard Ralph / 2026-09-30

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
