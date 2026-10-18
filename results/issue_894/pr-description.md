## Summary
**User impact:** Wyrd can't start a multipart upload yet. Large uploads from S3 clients
such as aws-cli, which switch to multipart above a size threshold, have nothing on the
server side to begin them. Earlier attempts at this piece also turned away ordinary
parallel uploads: 2 or 3 out of 8, 10 or 16 concurrent starts were refused as "server
full" when there was plenty of room. 10 is aws-cli's default concurrency.

This PR adds the core step that starts a multipart upload. It reserves a slot in the
cluster's session count and records the new upload in one atomic write. Parallel starts
are all admitted. Real limits (cluster full, bucket gone, upload too large to ever
finish) are refused cleanly and write nothing. When the store can't say whether the
write landed, the caller gets a clear "in doubt, re-read" answer instead of a wrong one.

The S3 front end is not wired up here. That is #668. Abort is #895.

## What to look at
- **The new function** is `create_session` in `crates/core/src/multipart/create.rs`.
  The core of it is one loop: read the current state, build one write, commit, and decide
  what the result means.
- **The tests** in `crates/core/tests/multipart_create.rs` are named for the rule each
  one pins, grouped A–F. A test-local store wrapper can hold a commit, report it as
  conflicted, or report it as "outcome unknown", so each race is reproduced on purpose,
  not by luck.
- **To try it:** `cargo test -p wyrd-core --test multipart_create`. For the seeded
  simulation: `cargo xtask dst` (it runs `crates/dst/tests/multipart_admission.rs` under
  `--cfg madsim`).
- **Size:** the diff is about 240 KB. Most of it is tests. The production code is
  `create.rs` (~700 lines) plus three constructors in `multipart.rs`.

## Root cause
`crates/core/src/multipart.rs` was pure by design. It defined the session record, the
segment-group marker and the `mpuctl` admission ledger, but had no `MetadataStore` call
and no `WriteBatch`, so nothing created a session. An earlier design spent one shared
retry budget on two different failures. "Lost the ledger compare-and-swap to a peer" and
"picked an upload id that was already taken" counted against the same small limit, so
ordinary concurrency used it up and got refused.

## Fix
- `create_session(store, budget, request, runtime)` builds one batch per attempt
  (proposal 0016, *Create session* row). The batch requires the ledger to be unchanged
  (or absent, to bootstrap it), the upload id and nonce to be absent, and the bucket
  marker to be the bytes first read. It puts the session record, the marker and the
  ledger at `count + 1`.
- **Two budgets, never shared.** A taken id or nonce mints a new identity, up to
  `MAX_UPLOAD_ID_ATTEMPTS`. A lost compare-and-swap, or an unknown outcome, spends one
  contention unit, waits `admission_backoff_millis(k, jitter)`, and retries the same
  identity with every precondition rebuilt from a fresh read. The contention budget is
  the profile's `max_sessions`, as stored in the ledger, not a fixed constant.
- **Order of checks on every read:** identity keys, then bucket, then ledger, then
  contention. So a missing bucket is reported as `NoSuchBucket` even when the ledger is
  also full.
- **Unknown commit outcomes.** After `CommitUnknownResult`, the create adopts a record
  only if it is byte-for-byte the record it wrote. Any other record under its id is
  another client's. If a batch may still land (`may_still_commit: true`), it never mints a
  new identity. It ends `CreateError::InDoubt` naming its own id. Before any ending other
  than `Created`, a final read settles whether its batch landed.
- **Error class.** `CreateError::InDoubt`'s `source()` is the unknown outcome, never the
  fault that stopped the create. So `wyrd_traits::classify` returns `Indeterminate` ("do
  not retry, re-read") even when a transient fault stopped it.
- **Size headroom.** A session whose widest reachable state (`Aborting`, `Completing`,
  `Completed`) would exceed `MAX_VALUE_BYTES` is refused up front with the new
  `Refusal::SessionTooLarge { bytes, limit }`. Otherwise it could be admitted and then
  never fenced.
- **Constructors.** One route each for `SessionRecord::open`,
  `AdmissionRecord::bootstrap` and `AdmissionRecord::admit_one`. Each round-trips through
  its decoder, and no decoder was loosened.
- **Docs.** The multipart module header, the `MAX_ADMISSION_CAS_ATTEMPTS` /
  `ADMISSION_BACKOFF_CAP_MILLIS` comments, the two `KnobSet` fields (Create does not read
  them yet; #508 reconciles them), and the multipart paragraph in
  `docs/design/architecture/05-building-block-view.md` now say that Create writes. They
  also say what a contention refusal means. It is bounded, not ruled out: on TiKV, lock
  contention with nothing moved can spend the budget.
- **Deferred, on purpose:** the bucket deletion-fence precondition (ADR-0046 §4) is
  #511's. The bucket precondition lives in one place, `require_bucket`, so #511 adds the
  fence there. Two live creates handed the same 128-bit id, nonce and request are
  indistinguishable. That bound is stated in the rustdoc, not tested as a guarantee.

## Verification
Line numbers are on `main` @ `71e9b9f` with this patch applied.

- **Claim:** parallel starts are all admitted, and every session is counted in the same
  write.
  - **Checked:** `crates/core/src/multipart/create.rs:336-345` (entry, size check first),
    `:407-444` (bucket, then ledger), `:700` (`require_bucket`).
  - **Test:** `a_free_running_create_storm_on_an_empty_store_admits_every_create`
    (`crates/core/tests/multipart_create.rs:448`; 8, 10 and 16 creates, exact `mpu:` /
    `seggrp:` / `mpuctl` counts).
    `a_prime_a_create_that_loses_the_cas_many_times_is_still_admitted` (`:655`; a forced
    schedule where one create loses more than `MAX_UPLOAD_ID_ATTEMPTS` times and is still
    admitted).
    `a_prime_the_contention_budget_follows_the_profile_in_effect` (`:686`).
- **Claim:** real limits refuse and write nothing, in the stated order.
  - **Test:** `b_a_full_ledger_answers_session_cap_and_writes_nothing` (`:1155`),
    `b_a_profile_skew_is_refused_and_writes_nothing` (`:1183`),
    `b_a_missing_bucket_answers_no_such_bucket_and_writes_nothing` (`:1209`),
    `b_a_missing_bucket_is_judged_before_a_full_ledger` (`:1230`),
    `b_a_bucket_marker_moved_under_the_commit_answers_no_such_bucket` (`:1274`),
    `b_a_torn_ledger_ends_in_a_ledger_error_and_writes_nothing` (`:1349`),
    `b_contention_spent_with_nothing_moved_refuses_after_the_whole_budget` (`:1380`;
    TiKV-style `Conflict` with nothing moved), and the size-headroom legs (`:1630`,
    `:1646`).
- **Claim:** a found record is adopted only if it is exactly this create's own.
  - **Checked:** `Identity::judge`, `create.rs:509-518`. `SessionRecord::same_session`,
    `crates/core/src/multipart.rs:2439-2457`.
  - **Test:** C5 (`:1891`, `:1954`; a byte-identical record already there is not
    adopted) and C6 (i)–(iv) (`:2019`, `:2062`, `:2110`, `:2157`; another session under
    the same id, differing by object or by nonce alone, is a collision or `InDoubt`, never
    `Created`).
- **Claim:** unknown outcomes are re-driven under the same identity, and a create that
  may still land ends `InDoubt` and classifies `Indeterminate`.
  - **Checked:** `CreateError::source`, `create.rs:201-210`.
  - **Test:** F1–F10 (`:2260`–`:3072`), including
    `f6_a_batch_that_may_still_land_keeps_the_create_in_doubt` (`:2547`),
    `f7_the_recovery_rebuilds_the_ledger_precondition_from_a_fresh_read` (`:2597`), the
    F8 legs (`:2658`–`:2808`; the doubt survives later faults) and the F9 classification
    legs (`:2847`, `:2892`).
- **Claim:** under seeded simulation with real ambiguous commits, every session is
  counted and is either answered `Created` or named in an `InDoubt`. No session is
  stranded.
  - **Test:** `multipart_admission_invariants_hold_under_the_dst_seed_sweep`
    (`crates/dst/tests/multipart_admission.rs:461`) and the 1021 / 1031 / mixed / witness
    legs (`:483`–`:549`). Two negative controls prove the checks can fail (`:579`,
    `:591`).
- **Red/green:** `create_session` does not exist on `main`, so the new tests cannot
  compile before this change. A plain revert-and-rerun has nothing to show. Instead,
  each rule was checked by mutation: one edit to the production code per row, about 40
  in total. Examples: share the two budgets, drop a precondition, adopt any record found,
  return the stop cause as `source()`, swap the bucket and ledger checks, drop the
  nonce from `same_session`. Every mutant turned its named test red. One known survivor,
  which held outcome `InDoubt.outcome` reports when several are held, has no effect on
  the answer. `cargo mutants --in-diff`: 75 mutants, 59 caught, 16 unviable, none
  missed.
- **Gate:** `cargo xtask ci` passes, including the DST tier under `--cfg madsim`:
  `multipart_create` 43/43, `multipart_state_machine` 22/22, `multipart_admission` 7/7.
  Diff coverage is 99.7% (343 of 344 instrumentable changed lines).

Fixes #894
