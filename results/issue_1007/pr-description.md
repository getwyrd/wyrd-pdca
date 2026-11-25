## Summary
**User impact:** A client finishing a multipart upload can get the list of parts wrong, and it should be able to fix the list and try again. An earlier attempt at this feature (#636, discontinued) did not allow that: three bad requests used up the upload's allowed attempts, so the fourth, correct request was refused even though every part was safely stored. The only way out was to abort and upload everything again. This PR builds the first step of finishing an upload so that this cannot happen. Users see no change yet, because the S3 request that will use this code has not landed.

This PR adds the step that locks an upload for publishing, checks the part list, and unlocks the upload again if the list is rejected, with no attempt spent.

This is the first of five changes that split up #658, and it comes from the design proposal, not from a user report.

## What to look at
The main thing to check is the promise this code makes to clients: a mistake by the client never costs it an attempt and never leaves the upload stuck. If the code cannot unlock an upload it locked, or cannot tell whether a write landed, it returns an error that says so. It does not return an answer that could be wrong.

To try it:

```
cargo test -p wyrd-core --test multipart_complete
```

The tests run against a real in-memory store through a wrapper that can refuse or fail specific reads and writes. No production code calls the new function yet, so there is nothing to try through S3.

## Root cause
Nothing yet takes a multipart session from `Open` to `Completing`. The discontinued #636 patch did, but it charged every fence against `attempts`, including fences for requests that were then rejected, so client errors used up a budget meant only to limit how many `seg:` epochs a session can create.

## Fix
- **New module `crates/core/src/multipart/complete.rs`:** `fence_complete` reads the session and answers every state but `Open` from the existing `complete_answer` table, with no commit.
- **The fence:** on `Open@E` it commits `Completing@E+1` with `attempts + 1`, requiring the exact bytes it read. A list that is already invalid is answered before this, with no commit.
- **Validation:** it checks the named parts against the session's `psum:` range and the bucket marker, read again behind the fence, so a part replaced between the two reads is still caught.
- **A valid request** returns a `HeldFence`, which the publication changes will consume. Nothing publishes here.
- **A rejected request** is released to `Open@E+2` with `attempts` as read before the fence, in at most `RELEASE_ROUNDS` commits.
- **Two error arms:** `InDoubt` means a commit's outcome is unknown. `Stopped` means nothing is in doubt, and it reports what became of the fence (`FenceState`).
- **A store fault behind the fence** is never answered as a rejection. It tries one release and reports the result. A fence left standing is cleared by the `W_completing` rollback (#625), the reaper that has not landed yet.
- **Session transitions:** adds crate-private `SessionRecord::complete_fence` and `fence_released` in `multipart.rs`.
- **Docs:** corrects the `ABORT_ROUNDS` comment in `abort.rs` and adds the fence to the architecture doc.

Line references below are to this branch, which is based on `main` at `376b106`. `complete.rs` and the test file are new here. Design references are to `docs/design/proposals/draft/0016-multipart-commit-protocol.md`.

## Verification
- **Claim:** a rejected request never spends the budget, and a correct request can always fence afterwards (`0016:994-1000`).
  - **Checked:** `crates/core/src/multipart/complete.rs:596-601` creates the release record together with the fence. `:663-670` releases on a rejection.
  - **Test:** `crates/core/tests/multipart_complete.rs:1003` (typos, and a part replaced before the fence, each repeated `MAX_COMPLETE_ATTEMPTS` times, then a valid request fences). `:1663` and `:1733` cover nonzero budgets and a retried fence.
- **Claim:** no answer leaves this request's own fence standing; only an error can.
  - **Checked:** `complete.rs:875-922` (`release`) and `:929-953` (`moved`).
  - **Test:** `multipart_complete.rs:1219` covers a release that loses one conflict and then lands, with the bound written in the test as 4 (`:91`). `:1518` and `:1571` cover a session that moved during the release.
- **Claim:** another publisher's fence is refused and left byte-for-byte unchanged, and a completed upload answers an identical retry with the recorded publication, with zero commits (`0016:976`, `:953-958`).
  - **Checked:** `complete.rs:689-709`.
  - **Test:** `multipart_complete.rs:837`, `:867`, `:922`.
- **Claim:** a fence that cannot be released, and an unknown commit outcome, end in an error that says so.
  - **Checked:** `complete.rs:893-918`.
  - **Test:** `multipart_complete.rs:1303`, `:1342`, `:1386`, `:1636`, and `:2214-2437` for store faults behind the fence.
- **Claim:** a deleted bucket answers `NoSuchBucket` and the session is `Open` again (`0016:1035`). The bucket is checked first, and the first bad part is the one reported.
  - **Checked:** `complete.rs:743-765`.
  - **Test:** `multipart_complete.rs:1267`, `:1805`, `:1855`.
- **Claim:** the part set is read in full across pages, and a page that breaks the paging contract is a store fault.
  - **Checked:** `complete.rs:773-812`.
  - **Test:** `multipart_complete.rs:2036`, `:2086`.

**This test has no ordinary pre-fix failure.** The module is new, so with the patch reverted the test file does not compile (E0432), which proves little. The evidence is instead 52 temporary edits to the production code, each putting one defect back, such as charging a rejected request, leaving the fence up, or setting `RELEASE_ROUNDS` to 1. All 52 made a test fail at an assertion, not at a build error.

**Overall checks:** `cargo xtask ci` passes. All 43 tests in the new file pass. `cargo mutants --in-diff` reports 72 mutants: 47 caught, 25 unviable, 0 missed. 99.0% of the changed lines that can be measured were executed (388 of 392).

## Known limits
These came up in review and were accepted for this change:

- **Validation reads only `psum:`.** A missing `part:` record with its `psum:` still present returns `Held`. Both are written in one batch, so this needs store damage. #1008 must treat a missing `part:` record as an error.
- **The pre-fence check can answer from an out-of-date session.** If the upload is completed and its records drained between the first read and that check (`complete.rs:607-609`), a retry could be told `InvalidPart` when it should get `AlreadyCompleted`. This cannot happen until publication and the drain exist. The re-read that fixes it is carried to #1008.
- **Two test gaps are carried to #1008.** No test can tell the scan fault from the re-read fault at `multipart_complete.rs:2437`, and none shows the held bucket marker comes from the read behind the fence.
- **`fenced_at_millis` is fixed before the reads and retries,** so a slow request's fence looks older than it is and could be rolled back early. The caller should stamp it as late as possible (#625, #670).
- **A bucket deleted and recreated under the same name** is accepted, because a session does not record which bucket it started in.
- **`fence_complete`, `HeldFence` and the error types are `pub`** so the test can call them before the S3 request exists. Their visibility may narrow when the callers arrive.

Fixes #1007
