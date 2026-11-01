# Build notes, #927 attempt 3 (Iteration 2 carry-forward)

## What this attempt is

I started from `iteration-v2/patch.diff`, not `iteration-v1/patch.diff`. The brief's Citations section
names v1, but the carry-forward's line numbers are all on v2's tree and it says "make the fixes below"
and "keep the settle order and the own-commit proof as they are". I opened only that patch from
`iteration-v2/`, plus `gate-logs/C4-ci.log` and `gate-logs/C5-mutants.log` to look at the stall and the
earlier mutant count. I did not read v2's reviews or notes.

Line numbers below are on the patched tree, base `376b106`. The size is 249,644 bytes against the
100 KB signal: the human's override says do not split.

## The changes against the carry-forward

**Fix 1, a damaged `part:` record is not a rival.** `crates/core/src/multipart/commit.rs:861-873`: when the
part key differs from what the commit requires, it is decoded. A decode failure goes in `damaged`, and
does **not** call `doubt.cannot_land()`, so it settles nothing. `commit.rs:936-938`: in the slot-held arm,
`damaged` returns through `torn`, so it is `Stop::Record` with nothing in doubt, or `InDoubt` with cause
`Record` and the flag kept. No compensation either way. Module doc: `commit.rs:78-80`.

The check sits in the `SlotSeen::Held` arm, not at the decode. What I ruled out: returning straight at
the decode, which is the same line count. It answers `InDoubt` for a commit that landed before the
damage, where the own-commit proof can answer `Committed`. The carry-forward says to keep that proof as
it is and the brief says it comes first "whatever the part key now holds". The second half of
`b_a_damaged_part_record_under_a_held_commit_keeps_the_doubt` pins this. The `Gone` and `Rewritten` arms
need no check, because neither compensates and neither reads the part key.

One judgement call for you: damage on the part key with a held commit reports
`may_still_commit: true`. The commit's `require` fails while the damage stands, but something could
repair the key, and the test shows the held batch landing after a repair. Claiming it cannot land would
be a guess.

**Fix 2, the five tests.** Each fails under the edit the carry-forward names (table below):
- `crates/core/tests/multipart_upload_part_commit.rs:3692` for `commit.rs:792` (`can_recur`, was `:788`)
- `:3735` for `commit.rs:624` (the ceiling's exclusion, was `:620`)
- `:3769` for `commit.rs:650-652` (`require_absent`, was `:646-648`)
- `:3797` for `commit.rs:527-531` (the doubt search, was `:523-527`)
- `:3826` for `commit.rs:1095-1098` (the epoch chase's list, was `:1081`)

**Fix 3, the never-run arms.** `:3867` runs the slot-gone arms at `commit.rs:918-923` (fenced, and newer
`Open`). `:3901` runs `Compensated::Left` in `refuse` (`commit.rs:1041`) and the slot-gone arms at
`commit.rs:1151-1158`, through a rival loss and through an over-limit part. Each arm has its own red.

**Fix 4, full CI, and why `custodian_gc` stalled.** See the two sections below.

**One leg the carry-forward did not ask for.** `:1738`,
`b_a_rollback_before_the_quiesce_renewal_commits_under_the_live_epoch`, pins the sign-off's decision that
a rollback before the quiesce renewal is adopted and may answer `Committed`. No code changes for it.

**Unchanged from attempt 2:** the batch builders, the classification, the compensation, `PartRecord::commit`
(`crates/core/src/multipart.rs:3029`), `PartRecord::summary` (`:3058`), `RetireObligation::part_chunks`
(`:4102`), `RecordError::PartChunkCountOverflow` (`:375`), the widened `Refusal::OperationAborted` doc
(`:4666`), the `StageOutcome::Staged` doc (`crates/core/src/multipart/stage.rs:363`) and the docs paragraph
(`docs/design/architecture/05-building-block-view.md:248`).

The sign-off's other decisions are kept as they were: slots left held after a Complete's rollback
(to be raised against 0016, not edited here), and the dispatch-only lease promise.

## Why `custodian_gc` stalled: a deadlock in third-party code, on the base too

It is not caused by this patch, and it is not "unrelated" in the sense of being harmless: any `cargo xtask
ci` can hit it, here and on `main`. A stalled run hangs until something kills it, which is how the last
C4-ci used up its 7200 s.

**Evidence.** I ran the binary in a loop with a 60 s limit and caught stalled runs under gdb.

| Tree | Plain loop | Under gdb |
|---|---|---|
| Patched | 3 stalls in 633 runs | first stall at run 83 |
| Unpatched base `376b106` (built from `git archive` in scratch) | 2 stalls in 65 runs | first stall at run 23 |

Don't compare the rates: other lanes and my own jobs were loading the machine differently during each
loop. Both gdb captures show the same stack. On one thread:

```
tracing_core::callsite::dispatchers::Dispatchers::rebuilder        tracing-core 0.1.36 callsite.rs:548  read-locks LOCKED_DISPATCHERS
tracing_core::callsite::DefaultCallsite::register / ::interest
opentelemetry_sdk::metrics::meter_provider::{impl#3}::drop        opentelemetry_sdk 0.33.1 meter_provider.rs:166  logs from Drop
  ... SdkMeterProvider ← MetricsLayer ← the subscriber's Arc dropped ← Dispatch dropped ...
tracing_core::callsite::dispatchers::Rebuilder::for_each           callsite.rs:562-573  drops an upgraded Dispatch
tracing_core::callsite::register_dispatch                          callsite.rs:485-487  holds the write lock (callsite.rs:552)
wyrd_server::logging::dispatch                                     crates/server/src/logging.rs:287
CustodianService::with_logging_to                                  crates/server/src/custodian.rs:361
wyrd_server::cli::cmd_custodian                                    crates/server/src/cli.rs:1025
```

**The cause.** A thread creating a new `Dispatch` holds the `LOCKED_DISPATCHERS` write lock while it walks
the registered dispatchers, upgrading each weak reference. If another thread drops its last strong
reference during that walk, the walker holds the only one left, and dropping it runs the subscriber's
destructor **on the thread holding the write lock**. `SdkMeterProviderInner::drop` logs through `tracing`.
The first event at that callsite registers it, which read-locks the same `RwLock`, so the thread waits on
itself. Every other test then queues on that lock, which is why the log showed them all "running for over
60 seconds".

**Why this binary.** It runs ten tests in one process, and they build and drop `Dispatch`es concurrently,
each with its own meter provider. A deployed process builds one, so I would not expect this there, but I
have not proved that.

**Not fixed here.** It needs an edit outside this slice's scope (`crates/telemetry`, or the tests), and
`Cargo.lock` is out of scope too. The rubric's rule for a real finding outside scope is a reference to an
issue, and the brief's intake says this slice adds no tracker issue, so **I have not filed one. Please
decide whether to.** Options, none tested:
- Serialize the logging tests in this binary, or run them one process per test.
- Turn off the `internal-logs` feature, which `opentelemetry_sdk` 0.33.1 enables by default
  (its `Cargo.toml:42`, `:69`), so that `Drop` does not log through `tracing`.
- Keep each test's `Dispatch` alive until the process ends.
- Report it upstream to `tokio-rs/tracing`: dropping a `Dispatch` while the lock is held is the root of it.

**What this means for Check.** C4-ci can hit this again on this bundle by chance. If it does, the stall will
be in `custodian_gc` with the stack above, and a re-run is a fair response. A stall with any other stack
is not this bug.

## (E) the whole gate

`./engine/xtask.sh ci` on the final tree: **exit 0, "xtask ci: all checks passed", 4m34s**. The log shows
each step ran rather than skipped: `typos`, `lint_docs: OK`, `render_site: wrote 101 page(s)` and `link
audit OK`, the gitlink, unsafe and blackbox guards, fmt, clippy, build, the workspace tests (`custodian_gc`:
10 passed in 0.17 s), cargo-machete, `cargo deny` (`advisories ok, bans ok, licenses ok, sources ok`, and
the all-features advisories), conformance (`5 valid + 6 invalid vectors pass`), statics, the deploy guard,
and the DST (deterministic simulation) tests. I edited no source after that run started. The temporary-edit
runs afterwards restored the files, and `git diff` shows only the intended changes.

`git apply --check` of `patch.diff` passes on a clean copy of `376b106`. `cargo fmt --all -- --check` and
`typos` pass. The repo configures no pre-commit hook in the worktree, so `cargo xtask ci`'s fmt and prose
steps are the commit-readiness check.

## Self-refutation

**(a) Genuine red? Yes.** The module does not exist on the base, so the file cannot compile there, and
C4-verify reports UNVERIFIABLE (exit 77), the pre-declared sign-off item. The evidence that binds is the
table below: 49 runs of a temporary edit against a named leg, all red by assertion or panic, none a
compile error. Two first attempts did not compile (a struct variant, and a method left unused under `-D
warnings`). I corrected those edits and re-ran them, and the table holds the corrected runs.

**(b) Production path? Yes.** Every leg calls `commit_part` or `upload_part` in `crates/core/src/multipart/
commit.rs`, over `RedbMetadataStore::in_memory()`. Sessions come from the real Create and parts from the
real `stage_part`, and each Abort is the real one. The wrapper changes answers and timing only and passes
every applied batch to the real store. Two things stand in for code that does not exist yet: `roll_back`
for a Complete (#658), and the test's teardown and idle-fence batches for #659 and #625. The brief asks for
those.

**(c) Fixture includes the fault? Yes.** The damage is real bytes in the store, planted by hook at the moment
it matters. The held commits are real batches kept unapplied and landed later against the real store. The
rollback, Abort, teardown and rival all run on the inner store between the attempt's reads.

## Demonstrated reds

Each row: the temporary edit, the leg, and the first failure message. Each edit was made in the tree, the leg
was run with `cargo test -p wyrd-core --test multipart_upload_part_commit <leg> -- --exact`, and the file was
restored. Rows are in the brief's order, then the carry-forward's.

| Edit | Leg | First failure |
|---|---|---|
| compensating on CommitUnknownResult without re-reading | `b_a_landed_commit_answered_unknown_is_committed` | assertion `left == right` failed: Err(Stopped { upload_id: UploadId("00000000000000000000000000000001"), cause |
| after an unknown outcome, a part being there taken as Committed | `b_an_unlanded_reupload_is_not_committed_on_the_prior_part` | assertion `left == right` failed: it committed again |
| part bytes alone taken as proof of the attempt's own commit | `c1_an_empty_part_is_never_committed_on_its_bytes` | assertion `left == right` failed: a Committed attempt holds no slot |
| the session's state taken from a read before the slot read, never re-checked | `c2_a_torn_read_never_answers_committed` | assertion `left == right` failed: Ok(Committed { part_number: PartNumber(1), digest: Digest([14, 217, 101, 146 |
| the possibly-landed commit dropped from its doubt before classification | `c3_a_landed_commit_then_a_rival_is_committed` | assertion `left == right` failed: Err(Stopped { upload_id: UploadId("00000000000000000000000000000001"), cause |
| the rolled-back Open@E' sent to the reaper instead of compensated | `b_a_rolled_back_session_is_compensated_under_its_newer_epoch` | assertion `left == right` failed: Ok(Refused(NoSuchUpload)) |
| an unknown compensation outcome treated as not landed with may_still_commit: true | `c_an_unknown_compensation_outcome_is_settled_by_a_re_read` | expected InDoubt, got Err(Stopped { upload_id: UploadId("00000000000000000000000000000001"), cause: Compensati |
| a commit-loop counter that never reaches its bound | `c_every_loop_ends_after_exactly_its_bound` | the commit never stopped committing (201 batches) |
| a compensation-loop counter that never reaches its bound | `c_every_loop_ends_after_exactly_its_bound` | the commit never stopped committing (201 batches) |
| my slot is gone read as the slot key is absent (ignoring attempt_id) | `c3_a_landed_commit_then_a_rival_is_committed` | assertion `left == right` failed: Err(InDoubt(InDoubt { upload_id: UploadId("00000000000000000000000000000001" |
| a compensation sent while the attempt's commit is held and unsettled | `h_a_commit_held_while_a_rollback_lands_is_in_doubt` | expected InDoubt, got Err(Stopped { upload_id: UploadId("00000000000000000000000000000001"), cause: SlotLost { |
| the rival-record branch checked before the own-commit proof | `c3_a_landed_commit_then_a_rival_is_committed` | assertion `left == right` failed: Err(Stopped { upload_id: UploadId("00000000000000000000000000000001"), cause |
| an empty compensation or re-upload that still writes a {chunks} obligation | `c4_empty_parts_never_install_an_empty_obligation` | every obligation decodes against its own key: RetireObligationOwesNothing { component: "payload" } |
| the same-part loser answering a refusal other than OperationAborted | `b_a_same_part_loser_compensates_its_own_chunks` | assertion `left == right` failed: Ok(Refused(NoSuchUpload)) |
| the rollback loser answering a refusal other than OperationAborted | `b_a_rolled_back_session_is_compensated_under_its_newer_epoch` | assertion `left == right` failed: Ok(Refused(NoSuchUpload)) |
| the lease coverage check removed | `f_a_lease_used_up_by_retries_stops_the_rounds_and_compensates` | a part commit was sent with 3002 ms left on its lease |
| the lease coverage check removed | `f_a_commit_round_is_sent_with_exactly_the_margin_and_not_with_a_millisecond_less` | a part commit was sent with 4999 ms left on its lease |
| the lease coverage check off by one, too strict | `f_a_commit_round_is_sent_with_exactly_the_margin_and_not_with_a_millisecond_less` | assertion `left == right` failed: Err(Stopped { upload_id: UploadId("00000000000000000000000000000001"), cause |
| the lease coverage check off by one, too loose | `f_a_commit_round_is_sent_with_exactly_the_margin_and_not_with_a_millisecond_less` | a part commit was sent with 4999 ms left on its lease |
| a commit round allowed while a commit is in doubt and the lease is short | `f_a_short_lease_with_a_commit_in_doubt_keeps_the_doubt` | a part commit was sent with 4999 ms left on its lease |
| a compensation sent while a commit is in doubt and the lease is short | `f_a_short_lease_with_a_commit_in_doubt_keeps_the_doubt` | expected InDoubt, got Err(Stopped { upload_id: UploadId("00000000000000000000000000000001"), cause: LeaseShort |
| a lease past expiry taken as proof that a held commit did not land | `f_a_commit_delayed_past_its_lease_is_settled_by_reads_never_by_the_clock` | assertion `left == right` failed: run ii |
| a moved session seen after the last slot read counted as settling the commit | `b_read_gap_a_precondition_that_dies_between_the_reads_settles_only_at_a_later_slot_read` | assertion `left == right` failed: LandedThenRollback at gap 3 |
| a rival seen after the last slot read counted as settling the commit | `b_read_gap_a_precondition_that_dies_between_the_reads_settles_only_at_a_later_slot_read` | assertion `left == right` failed: LandedThenRival at gap 3 |
| settling never allowed after a precondition dies | `b_read_gap_a_precondition_that_dies_between_the_reads_settles_only_at_a_later_slot_read` | assertion `left == right` failed: Err(InDoubt(InDoubt { upload_id: UploadId("00000000000000000000000000000001" |
| a settled commit stop that returns without compensating | `g_settled_stops_never_fill_the_slot_cap` | assertion `left == right` failed: multipart part commit: upload 00000000000000000000000000000001: all 4 part c |
| a settled renewal stop that returns without compensating | `g_a_settled_renewal_stop_compensates_and_keeps_its_error` | assertion `left == right` failed: multipart part commit: the quiesce renewal did not cover the commit, so none |
| a settled renewal stop that returns without compensating | `g_settled_stops_never_fill_the_slot_cap` | assertion `left == right` failed: multipart part commit: the quiesce renewal did not cover the commit, so none |
| a compensation after SlotLost | `g_a_stop_that_cannot_show_its_records_are_its_own_does_not_compensate` | expected a renewal error, got Err(Stopped { upload_id: UploadId("00000000000000000000000000000001"), cause: Sl |
| a compensation after a renewal InDoubt | `g_a_stop_that_cannot_show_its_records_are_its_own_does_not_compensate` | assertion `left == right` failed: multipart part commit: the quiesce renewal did not cover the commit, so none |
| the over-limit check in the commit removed | `d_a_part_past_the_per_part_bound_is_refused_and_compensated` | assertion `left == right` failed: Ok(Committed { part_number: PartNumber(1), digest: Digest([150, 52, 238, 16, |
| the session check after an unknown compensation made always true | `h_a_held_compensation_under_a_teardown_is_in_doubt` | expected InDoubt, got Ok(Refused(OperationAborted)) |
| the obligation re-read skipped | `h_the_obligation_re_read_settles_a_landed_compensation_and_catches_other_bytes` | assertion `left == right` failed: Err(InDoubt(InDoubt { upload_id: UploadId("00000000000000000000000000000001" |
| the obligation byte comparison made always true | `h_the_obligation_re_read_settles_a_landed_compensation_and_catches_other_bytes` | expected Stopped, got Ok(Refused(OperationAborted)) |
| COMPENSATION_ROUNDS lowered by one | `h_the_epoch_chase_lands_within_its_bound_after_every_rollback_a_session_can_see` | assertion `left == right` failed: Err(Stopped { upload_id: UploadId("00000000000000000000000000000001"), cause |
| UploadPartError::source returning None | `h_a_composed_error_keeps_its_class_and_its_message` | assertion `left == right` failed: multipart part commit: upload 00000000000000000000000000000001 is in doubt:  |
| carry-forward 1: a damaged part record read as a rival (the fix reverted) | `g_a_damaged_part_record_at_the_re_read_is_a_record_stop_not_a_rival` | expected Stopped, got Ok(Refused(OperationAborted)) |
| carry-forward 1: a damaged part record read as a rival (the fix reverted) | `b_a_damaged_part_record_under_a_held_commit_keeps_the_doubt` | expected InDoubt, got Ok(Refused(OperationAborted)) |
| carry-forward 1: damage counted as settling a held commit | `b_a_damaged_part_record_under_a_held_commit_keeps_the_doubt` | expected InDoubt, got Err(Stopped { upload_id: UploadId("00000000000000000000000000000001"), cause: Record(Mal |
| carry-forward 2: can_recur forced to false | `b_a_rival_over_an_empty_prior_settles_nothing_because_those_bytes_can_come_back` | expected InDoubt, got Ok(Refused(OperationAborted)) |
| carry-forward 2: the ceiling's exclusion of the part's own prior disabled | `d_the_cumulative_ceiling_leaves_out_the_parts_own_prior` | assertion `left == right` failed: Ok(Refused(EntityTooLarge { chunks: 80897, limit: 80896 })) |
| carry-forward 2: require_absent on the re-upload's obligation key removed | `d_a_taken_obligation_key_keeps_those_bytes_and_the_prior_part` | expected Stopped, got Ok(Committed { part_number: PartNumber(1), digest: Digest([159, 16, 142, 203, 132, 37, 1 |
| carry-forward 2: the doubt search replaced with .last() | `b_the_kept_doubt_is_the_round_that_may_still_land` | round 1 may still land |
| carry-forward 2: the epoch chase keeping only the latest obligation | `h_the_epoch_chase_re_reads_every_obligation_it_sent` | assertion `left == right` failed: Err(InDoubt(InDoubt { upload_id: UploadId("00000000000000000000000000000001" |
| carry-forward 3: the commit's slot-gone, session-fenced arm answering SlotLost | `g_a_slot_gone_under_a_moved_session_at_the_commits_re_read` | assertion `left == right` failed: Err(Stopped { upload_id: UploadId("00000000000000000000000000000001"), cause |
| carry-forward 3: the commit's slot-gone, newer-Open arm answering a refusal | `g_a_slot_gone_under_a_moved_session_at_the_commits_re_read` | expected Stopped, got Ok(Refused(OperationAborted)) |
| carry-forward 3: refuse() answering its own refusal under a fenced session | `g_a_slot_gone_under_a_moved_session_at_the_compensations_re_read` | assertion `left == right` failed: Ok(Refused(OperationAborted)) |
| carry-forward 3: the compensation's slot-gone, session-fenced arm answering SlotLost | `g_a_slot_gone_under_a_moved_session_at_the_compensations_re_read` | assertion `left == right` failed: Err(Stopped { upload_id: UploadId("00000000000000000000000000000001"), cause |
| the session the commit requires read before the quiesce renewal adopted a rollback | `b_a_rollback_before_the_quiesce_renewal_commits_under_the_live_epoch` | assertion `left == right` failed: Ok(Refused(OperationAborted)) |

Notes on specific rows:
- "A lease past expiry" clears the doubt only at a held-slot read, so run (i) still passes and the leg fails
  in the runs where a compensation appears, as the brief describes.
- "A compensation sent while the commit is held" is run against (H)'s leg, as the brief directs.
- "A rival seen after the last slot read" adds a part re-read after the slot read to make that history
  reachable, since the code reads the part first.
- The brief's "`EntryLost`" row cannot come out of `StagedAttempt::renew`; the unit test at `commit.rs:1241`
  (`a_stopped_renewal_compensates_only_on_the_rows_that_say_so`) holds it.

## Slot bytes, by invariant rather than by red

Both builders take the slot bytes from the attempt's own state: `commit.rs:649` and `:678`, through
`StagedAttempt::slot_bytes`. The wrapper checks every batch of every leg at
`crates/core/tests/multipart_upload_part_commit.rs:563`, and the dispatch margin on every commit batch at `:592`.

## C5 mutants

`scripts/mutants-in-diff` (cargo-mutants 27.1.0): **78 tested, 63 caught, 15 unviable, 0 missed, 0 timeouts.**
It ran on the tree before the adopted-rollback leg was added, and that leg is test-only.

No survivors, so attempt 1's six answer-changing survivors are gone and its two `lossy` and one `Display`
survivors are killed by the message assertions (`in_doubt`, `stopped`, `assert_slot_lost`).

The one mutant the brief lists as a survivor, a builder that re-reads the slot bytes from the key, is **not**
generated by cargo-mutants. By hand it is unreachable under the classification, for the brief's reason: a
conforming attempt recognises its own commit in C3′ and returns before any later batch is built, and no
conforming path sends a batch after another attempt holds the index.

The 15 unviable mutants are all `Default::default()` replacements for return types with no `Default`
(`crates/core/src/multipart.rs:3035`, `:3059`, `:4109`; `commit.rs:420`, `:461`, `:527`, `:547`, `:572`, `:597`,
`:724`, `:996`, `:1020`, `:1038`, `:1065`, `:1213`). They do not compile, so they say nothing about the tests.

## Rubric self-review

- **One clock:** the lease check and the part's stamp read the caller's `StageClock` (`commit.rs:817`, `:775`);
  the module doc states the owner. No new read in this attempt.
- **Metadata validation:** fix 1 is this rule applied at a re-read. Damage is an error, never a value.
- **Serialization identity:** `PartRecord::commit` and `summary` are tested by the wrapper's
  `assert_values_decode` on every batch.
- **Absent or unsupported entries:** every stop is typed, and a slot found gone is `SlotLost`, a refusal or the
  own-commit proof.
- **Transactions:** an unknown outcome is never read as a conflict.
- **Await discipline:** every loop is bounded and counted against a number written in the test. Each store
  call is bounded by the store's contract, which the module doc says. No helper tasks are spawned.
- **Test fidelity:** seeded Tier-0 DST coverage is deferred to #660 with the in-code `// deferred: #660` marker
  (`commit.rs`, after the module doc), as the brief puts it out of scope.
- **Docs currency:** the architecture paragraph is in the diff, unchanged from attempt 2. Fix 1 adds no port,
  operation, flag or persisted field.
- **`#![forbid(unsafe_code)]`:** no new crate root; the test file carries it.

## For sign-off

1. C4-verify will report UNVERIFIABLE (exit 77), pre-declared by the brief.
2. The `custodian_gc` deadlock above needs a decision on an issue. It is on `main` and I have not filed anything.
3. The two ⚑ items the Plan flagged are already ratified in the carry-forward, and the 0016 wording on the lease
   and the held slots after a rollback are still yours to raise there.
