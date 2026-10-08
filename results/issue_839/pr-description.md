## Summary
**User impact:** after restoring the metadata store, an operator runs
`wyrd custodian --reconcile-after-restore` to learn what state the cluster is in. Today
that output leaves out two things. It keeps the pieces of in-progress multipart uploads on
disk without counting them. When one of those uploads has a damaged record, the command
can still report the run as clean, and the operator only finds the damage later when a
drain stalls on it. The damage is logged, but only in the audit log.

This PR makes the restore report and its CLI output count the kept upload pieces and name
any damaged upload record, so the run is no longer reported clean while one exists. The
exit status does not change: a damaged upload record does not need a human (reasons
below).

## What to look at
- **The report:** two new fields on the post-restore report. One counts the upload pieces
  kept. The other lists the upload records the pass could not trust. Only the second one
  stops a run from being reported clean.
- **The CLI output:** the summary line gains both counts. A new informational line names
  the untrusted records. It is deliberately **not** a NEEDS-HUMAN paragraph and does not
  set the exit status.
- **Try it:** seed an open upload session with one part record whose chunks are on disk and
  one part record with a wrong-length placement, then run the post-restore pass. On `main`
  the report has neither field and says the run is clean. With this PR it counts the kept
  pieces, names the bad record, and is not clean. The new test file does exactly this.

## Root cause
The post-restore pass's mark gate put the staged (multipart upload) protection in the same
`continue` as the committed-map checks, so fragments kept on staged grounds were skipped
without being counted. A comment left that counter for later. Untrusted staged records
were passed to the audit log and nowhere else (`attribute_staged`, marked `deferred: #664`),
so `RestoreReport` had nothing to name them with and `is_clean()` did not consider them.

## Fix
- `crates/custodian/src/restore.rs`
  - `RestoreReport` gains `staged_skipped: usize` and `staged_untrusted: Vec<String>`.
  - The mark gate is split in two. The committed checks still skip without counting. The
    staged check follows and counts. Resulting order: committed → staged → displaced →
    pending lease. Each kept fragment lands in at most one counter, and the one it lands in
    is the first protection that keeps it.
  - `attribute_staged` now also returns the untrusted record names, once per record (one
    part record can hold several chunks), in key order. The audit line is unchanged.
  - `is_clean()` also requires `staged_untrusted` to be empty. `needs_human()` behaves the
    same as before. Its doc comment records why untrusted staged records are left out on
    purpose: once the session is fenced (#841, #842), its staged bytes are garbage whatever
    the record says, so what is left is cleanup, not a judgement. Keep-on-doubt protects user
    data, not leftover system records (#811). Until the fence lands the session is still
    open, but no production client can create a session before #508, which lands after the
    fence.
  - The pass's summary log event gains both values.
- `crates/server/src/cli.rs`: `restore_verdict` prints both counts on the summary line.
  When a staged record is untrusted, it adds an informational line naming those records
  (through the same `named_records` helper the unreadable-records paragraph uses). The line
  says the pass marked none of their fragments and did not check that the staged bytes
  survived the restore. It makes no cleanup promise, because whether the retire drain
  removes such a record is still undecided (#659).
- `docs/design/architecture/m4-first-deployment-blueprint.md`: step 7 of the restore
  runbook describes the new output.

**Note for reviewers:** `staged_untrusted` counts **records**. The existing
`restore_untrusted_staged_records` counter metric counts **(record, chunk) pairs**. A record
that holds two chunks is 1 in the report and 2 in the metric, so do not compare them
directly on a dashboard.

## Verification
- **Claim:** every fragment kept on staged grounds is counted, exactly once, under the first
  protection that keeps it.
  - **Checked:** on `main`, the staged check shares the uncounted `continue` with the
    committed checks, `crates/custodian/src/restore.rs:431-441` (comment at `:433-434`). The
    counter to copy is `pending_skipped`, at `:489-491`.
  - **Test:** `crates/custodian/tests/restore_staged_report.rs`,
    `staged_skips_are_counted_once_by_the_first_protection_that_keeps_them`. The fixture has
    staged-only, staged + pending lease, staged + displaced, and committed + staged fragments,
    plus one real stray. It expects `staged_skipped: 4`, `pending_skipped: 0`,
    `displaced_kept: 0`, and only the stray marked.
- **Claim:** an untrusted staged record is named in the report, does not need a human, and
  stops the run from being reported clean. None of its chunk's fragments gets marked, and
  the audit line still fires.
  - **Checked:** on `main`, untrusted records only reach the audit log,
    `crates/custodian/src/restore.rs:813-827` (the `deferred: #664` marker is at `:819-821`).
    `is_clean()` at `:197-199` does not consider them.
  - **Test:** same file, the untrusted-record test. Session 1 has a wrong-length placement
    and session 2 has only valid records. It checks that the list contains session 1's record
    exactly once and no key from session 2, that `needs_human()` is false and `is_clean()` is
    false, that no held fragment has an `orphan:` mark, and that the audit event names the
    record.
- **Claim:** the CLI counts both, puts untrusted records on an informational line (not
  NEEDS-HUMAN), and keeps `needs_human()` as the exit status.
  - **Checked:** `restore_verdict` at `crates/server/src/cli.rs:1256`, `named_records` at
    `:1389`, and the unreadable-records paragraph it copies at `:1346-1358`.
  - **Tests:** `restore_needs_human_agrees_with_every_paragraph_it_prints` gains a case with
    an untrusted record, expecting no NEEDS-HUMAN and no status. The new
    `restore_verdict_counts_staged_skips_and_names_untrusted_staged_records_as_information`
    checks the required wording, that the record names appear, and that neither
    "NEEDS-HUMAN" nor any cleanup wording appears.
- **Before and after:** with only the new test file applied on `main` (36f006d), both tests
  in `restore_staged_report.rs` compile and **fail by assertion**. With the fix, both pass
  and the full `cargo xtask ci` passes (typos, docs, fmt, clippy `-D warnings`, workspace
  tests, deny, conformance, DST). The existing `restore_reconcile.rs` (17 tests) and
  `staged_protection.rs` (36 tests) pass unchanged.

Fixes #839
