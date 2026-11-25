## Summary
**User impact:** Nobody using Wyrd can complete a multipart upload through this code yet, because an uploaded part's data can be stored but never becomes a finished part. Clients see no change from this PR alone: the S3 `UploadPart` request is connected later, in #669.

This PR adds the step that finishes a part: it commits the part, replaces an earlier upload of the same part number, or refuses the part and cleans up after it. It is the first of four changes to this step. #1028, #1029 and #1030 follow.

## What to look at
The main question is what happens to a part's data on each way out. The data must always stay tracked, either by the finished part or by a record that schedules it for deletion. An attempt must also never give up an upload slot it does not own.

To try it, run `cargo test -p wyrd-core --test multipart_upload_part_commit`. The 29 tests use a real in-memory metadata store. A test wrapper forces conflicts, unknown results, delayed writes and clock moves. There is no S3 path to exercise yet.

Two behaviours are deliberately temporary. If the store answers a sent commit with a conflict or an unknown result, the attempt stops and sends nothing more. The follow-up changes replace those stops, and no client can reach them before then.

## Root cause
`stage_part` writes a part's chunks and holds an in-flight slot, but on `main` no code writes the `part:` or `psum:` records, so `PartRecord` and `PartSummary` have decoders and no writer. An attempt that stopped under a live session also had no way to release its slot, and the reaper does not walk `Open` sessions, so repeated stops could fill `MAX_INFLIGHT_PARTS` (`0016:1420-1431`).

## Fix
- **Commit:** `commit_part` (`crates/core/src/multipart/commit.rs:636`) refuses an over-limit attempt first, then runs one last lease renewal and reads the `psum:` range and the part key. It then sends one batch holding `part:`, `psum:`, the slot release and the `sidx:` deletes. It sends no renewal after those reads, because a renewal would rewrite the slot bytes the commit requires.
- **Re-upload:** the batch requires the prior part's exact bytes and puts the prior part's chunks in one `retire:bytes:` obligation. An empty prior writes no obligation.
- **Ceilings:** the commit enforces `MAX_PART_CHUNKS` itself, because `StageRequest.max_part_chunks` is set by the caller and can be higher (`crates/core/src/multipart/stage.rs:325-329`). The `MAX_STAGED_CHUNKS` check is an unfenced read. The overshoot it can leave is the one proposal 0016 allows (`0016:1441-1451`).
- **Stops before a commit is sent:** a settled stop compensates in one batch: an obligation for the attempt's own chunks, the slot release and the `sidx:` deletes. It does not compensate when the renewal is in doubt, the slot is lost, a record is damaged or the store fails.
- **Lease margin:** a commit is sent only with `E_TX_MILLIS` left on the lease. This covers dispatch only. A batch that applies after the lease ends is made safe by its `require`s (`0016:1851-1852`).
- **Supporting changes:** `PartRecord::commit`, `PartRecord::summary`, `RetireObligation::part_chunks`, `RecordError::SessionEpochNotRisen`, `upload_part` (reserve, stage, commit), a corrected doc on `StageOutcome::Staged`, and one paragraph in the architecture docs. `stage.rs` changes in that doc comment only.

## Verification
Line numbers are for this PR's branch. `commit.rs` and the test file are new.

| Claim | Checked in code | Test (`crates/core/tests/multipart_upload_part_commit.rs`) |
|---|---|---|
| Parts committed out of order have agreeing `part:` and `psum:` records and leave no staging records | `commit.rs:477` | `:1453` `a_parts_staged_out_of_order_each_commit_with_records_that_agree` |
| A re-upload retires the prior chunks in the same batch | `commit.rs:477` | `:1571` `d_a_reupload_retires_the_prior_chunks_in_the_same_batch` |
| The per-part ceiling holds for truncated and whole bodies, and a body exactly at the ceiling commits | `commit.rs:386` | `:1705`, `:1740`, `:1767` |
| The cumulative ceiling refuses, and leaves out the part's own prior | `commit.rs:436`, `:679` | `:1801`, `:1849` |
| A commit is sent with exactly the margin left, and not with 1 ms less | `commit.rs:584` | `:1872` `f_a_commit_is_sent_with_exactly_the_margin_and_not_with_a_millisecond_less` |
| Settled stops compensate and cannot fill the slot cap | `commit.rs:420`, `:538` | `:1973`, `:2008` |
| No batch is sent under a damaged session the renewal adopted | `commit.rs:404` | `:2309` `h2_a_damaged_session_the_renewal_adopted_is_never_compensated_under` |
| A batch naming a slot requires the bytes its own submitter last wrote | `commit.rs:477`, `:513` | `:2369` `h3_the_slot_guard_rejects_a_batch_carrying_another_attempts_bytes` |
| A sent commit answered conflict or unknown sends nothing more | `commit.rs:571` | `:2442`, `:2465` |

- **Pre-fix and post-fix:** this is new code, so with the fix removed the test file does not compile (E0432, E0599) and no test fails on behaviour. Instead, each guard was temporarily broken to confirm a named test fails, including the lease check, both ceilings, compensation, the epoch check and the slot guard. All 29 tests pass with the fix.
- **Mutation testing:** `cargo mutants --in-diff` tested 49 mutants: 34 caught, 15 did not compile, none survived.
- **Coverage:** a direct `cargo llvm-cov` run executed every instrumented line in `commit.rs` (381 of 381) and every added line in `multipart.rs` (57 of 57). That run used the earlier base described below.
- **Full gate:** `cargo xtask ci` passed on an earlier base, `635e0bd`. The patch applies cleanly to the current `main` (`fd64faf`), but the full gate has not been run on that tree, so this PR's CI is the first full run there.

**Known gaps**
- The epoch check in `renewed_session` compares the session only before and after the last renewal, not at each renewal round.
- A non-`Open` record at an epoch that has not risen is answered `NoSuchUpload` inside the renewal (#1026).
- No test pins the over-limit check running before the renewal, or the scheme checks in `PartRecord::commit` and `RetireObligation::part_chunks`. All 29 tests still pass if the over-limit check moves after the renewal, or if either scheme check is deleted. Deferred — tracked in #1028.
- A compensation for a part staged above `MAX_PART_CHUNKS` could in theory exceed a backend's value limit. That would surface as a store error with nothing lost. It is not exercised, because the test fixture's chunk references are small and redb has no such limit.
- When a compensation itself fails, the returned error reports that failure and not the stop that triggered it.

Fixes #1027
