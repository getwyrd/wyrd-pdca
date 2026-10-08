## Summary
**User impact:** when a disk or server holding part of an object uploaded with
multipart upload is lost, the cluster never rebuilds the missing piece. Every
maintenance pass skips the repair and leaves it queued, so the object keeps
running with less redundancy than it was stored with. If enough further pieces
are lost, the object's data is gone. Objects uploaded in a single request are
not affected.

This PR makes the repair pass rebuild those pieces and record their new
location, the same way it already does for single-request objects.

## What to look at
The change is in the custodian's reconstruction pass: the step that records a
rebuilt piece's new location now calls the shared "move one chunk" helper added
in #776, instead of writing the object's main record itself. That helper knows
how to update the smaller per-segment record that multipart objects keep their
chunk list in. The old "refuse and skip" path for these objects is removed.

To try it: `cargo test -p wyrd-custodian --test segmented_map_repoint`. The
first test in that file seeds a multipart-style object with one lost fragment,
queues its repair and runs one pass. On `main` the repair is refused and stays
queued. With this PR the fragment is rebuilt on a healthy server and the queue
is empty.

## Root cause
#697 stopped a segmented object from aborting the whole reconstruction pass,
but on purpose it wrote nothing for it: the obligation was routed to a refused
site and answered `Assessment::Refused` on every pass, because there was no safe
way to change a placement held in a `seg:` record. #776 added that write
(`metadata::repoint_chunk`), but nothing called it.

## Fix
- `repair_chunk` builds its commit from `metadata::repoint_chunk` for flat and
  segmented objects alike, then adds the obligation's delete and one orphan
  mark per displaced fragment to the batch the move hands back. The placement
  change, the drain and the orphan evidence stay one version-conditional write
  (ADR-0015). The segmented root is never rewritten.
- Each answer of the move maps to an existing outcome, and every answer except
  `Prepared` writes nothing: `Conflict` → conflict (retried next pass),
  `Refused` → ceiling refusal (pass reports `Blocked`), `VersionExhausted` or a
  typed `ChunkMapError` → contained for that object. Any other store error under
  the move's own read ends the pass, as a store fault does elsewhere.
- The plan pins the root the resolver actually answered from, so a resolve that
  restarted onto a newer generation is not repaired against the retired one.
- The refusal path (`Site::Refused`, `Assessment::Refused`, `emit_refused`) is
  removed. `emit_aborted` now carries a `reason`.
- Docs: the runtime view (section 6.3) and crosscutting concepts (section 8.7)
  describe the write side. The `deferred: #777` note on `repoint_chunk` is
  replaced by a pointer to its caller.
- The #697 test `an_obligation_inside_a_segmented_object_is_refused_never_discarded`
  asserted the refusal this PR removes. It is renamed `…_is_repaired_never_discarded`
  and now asserts that the repair lands.

Known and left as is: on a lost race the rebuilt fragment already written to
the destination server is left behind (tracked in #723). A seeded simulation
test for this move is tracked in #682, with a marker at
`crates/custodian/src/reconstruction.rs:1165`. Until then the interleavings are
scripted in the new test file.

## Verification
Line numbers are on this branch (`main` with the patch applied). The patch
applies cleanly to `main` at 36f006d.

- **Claim:** a chunk held in a `seg:` record is repaired in its own record: the
  fragment is rebuilt on a healthy server in a distinct failure domain, the
  `seg:` record names it, the obligation is drained, the pass answers `Changed`,
  and the root bytes are unchanged.
  - **Checked:** `crates/custodian/src/reconstruction.rs:1167-1206`: the move is
    prepared, mapped to an outcome, and the drain is added to its batch.
  - **Test:** `crates/custodian/tests/segmented_map_repoint.rs:504`
    (`a_segmented_objects_under_replicated_chunk_is_repaired_in_its_own_record`).
    It fails on `main` (refused, still queued) and passes here.
- **Claim:** a racing move of a sibling chunk in the same `seg:` record is
  merged, not lost.
  - **Test:** `segmented_map_repoint.rs:562`. Fails on `main`, passes here.
- **Claim:** if the planned chunk itself or the root generation changes under
  the plan, the repair loses cleanly: no repair-owned metadata written,
  obligation still queued, exactly one conflict counted and no abort, and the
  pass answers `Satisfied` (a lost race is a retry, not a gap).
  - **Checked:** `reconstruction.rs:1185` (conflict inside the move) and
    `reconstruction.rs:1218` (conflict at the commit).
  - **Test:** `segmented_map_repoint.rs:586` and `:605`, with the counter check
    in `assert_lost` (`:469`). Relabelling either line as an abort turns exactly
    its own test red.
- **Claim:** a segment record at the full `MAX_VALUE_BYTES` ceiling is refused,
  left byte-identical, and the pass answers `Blocked`.
  - **Test:** `segmented_map_repoint.rs:669`. It passes on `main` too, by
    design: it guards the ceiling check, which a mutation run covers.
- **Claim:** records the move cannot rewrite (torn, unparsable or noncanonical
  key, version that cannot advance, length overflow) are contained once per
  object and never retried forever. A store fault under the move's own read
  ends the pass.
  - **Test:** `segmented_map_repoint.rs:769`, `:774`, `:818`, `:826`, `:863`,
    `:870`, `:892`.
- **Claim:** when the resolver restarts onto a newer root, the repair lands on
  the generation it answered from, and the retired generation's records are
  untouched.
  - **Checked:** `reconstruction.rs:702` (`prior: resolved.record…`).
  - **Test:** `segmented_map_repoint.rs:929`. Pinning the scanned root instead
    turns only this test red.
- **Whole file on `main` without the fix:** 13 tests ran, 12 failed, 1 passed
  (the ceiling test above). With the fix: 13 passed.
- **Mutation check:** each pin in the move and the caller was broken one line
  at a time. Each break turned exactly the test(s) that claim it red.
- **Full gate:** `cargo xtask ci` passes. Diff coverage is 98.6%.

Fixes #777
