# Adversarial review — issue #839 (809.1, restore-staged-report)

**Verdict: could not refute.** I re-ran the proof myself, probed the edge cases the new test
file skips, and hand-mutated the ordering. The fix held every time. No NEEDS-HUMAN items.

## Attempts that failed to refute the fix

- **Red→green re-run, independently.** In a scratch clone of the base (`ffc75ff`), with only the
  new test file added, both tests fail **by assertion**, not by compile error:
  `crates/custodian/tests/restore_staged_report.rs:471` (`staged_skipped`: `None` vs `Some("4")`)
  and `:561` (no `staged_untrusted` in the report). With the full patch both pass, and the 5
  `restore_` tests in `crates/server/src/cli.rs` pass. This matches `gate-logs/C4-verify.log`.
  The tests call the production `reconcile_after_restore`. Only the store and the disks are
  doubles, and nothing in the path is mocked away.
- **Is the "first protection" order really pinned?** cargo-mutants never generates a reordering,
  so I tested two by hand at `crates/custodian/src/restore.rs:480-483`. Moving the staged check
  after the displaced check makes E fail with `staged_skipped` 3 vs 4. Moving it ahead of the
  committed readings makes E fail with 7 vs 4. The count-based assertion is discriminating: it
  cannot pass while the ordering property fails.
- **Untrusted `sidx:` entry** (a value that will not decode, under a key that names its chunk),
  which the new file does not cover. It is named in `staged_untrusted`, its 2 fragments count as
  `staged_skipped`, nothing is marked, `needs_human()` is false and `is_clean()` is false. This is
  correct: `crates/custodian/src/gc.rs:1391-1392` holds it, and `restore.rs:856-871` names it.
- **Untrusted record beside an unreadable one.** The untrusted record is still named, the
  unreadable one goes to `unresolvable`, `staged_skipped` is 0 (the incomplete gate at
  `restore.rs:471` runs first), and nothing is marked. No double listing: a key lands in `held`
  or in `unresolvable`, never both (`gc.rs:1405-1417`, `:1376-1397`).
- **Untrusted record over a chunk a committed object also places.** The committed copies are
  skipped uncounted, and a stale extra copy on another server is held and counted as staged, not
  marked. The record is named.
- **Second run.** The same `staged_skipped` and `staged_untrusted` come back, and the stray moves
  to `already_marked`. The new counters are stable across re-runs.
- **CLI claims.** "blocks every drain in the cluster" (`cli.rs:1379-1380`, and the blueprint)
  matches `crates/custodian/src/desired_state.rs:322-340`: a held staged chunk returns
  `PendingMalformed` for every server. The `needs_human()` doc says no production client can
  create a session yet, and that holds: the only `mpu_key` in non-test code is its definition at
  `crates/core/src/multipart.rs:1212`. The note line contains no "NEEDS-HUMAN", "automatic" or
  "cleanup". The agreement test (`cli.rs:2930-3033`) still pairs each paragraph with the exit
  status.
- **Reviewer rationalization.** I found no unwarranted claim in `check-gates.json`. One caveat
  on reading the evidence: E's `pending_skipped: 0` and `displaced_kept: 0` assertions were
  already true on the base (the staged check sat in the same gate there), so they were **not**
  part of the red. They guard the ordering, and the hand mutations above show they do that job.
  Only `staged_skipped` and `staged_untrusted` produced the red, which is what the brief predicted.

## Minor observations (non-blocking; no rebuild needed)

- `crates/custodian/src/restore.rs:132-135`: the `staged_skipped` doc lists the order as
  "committed readings (uncounted), the staged class, …" and leaves out the incomplete-set gate
  that runs before all of them. So on any INCOMPLETE run (one unreadable record anywhere)
  `staged_skipped` is 0 even though every staged fragment was kept. The CLI's INCOMPLETE
  paragraph then says "the counts above cover the REST of the store only"
  (`crates/server/src/cli.rs:1361-1362`), which is not quite true of this count. The older
  `pending_skipped` and `displaced_kept` counts already had the same wording problem; this
  patch adds one more count under it. A one-clause doc fix if anyone cares. It is not a defect.
- `restore.rs:1050` vs `:1075`: the existing counter `restore_untrusted_staged_records`
  increments once per (record, chunk) pair, but the new summary field `staged_untrusted` counts
  records. For the H-iii fixture (one record, two chunks) the counter says 2 and the summary
  says 1. The unit difference is documented on the field ("once per record"), so this is not a
  defect. It is worth knowing before anyone builds a dashboard that compares the two.
