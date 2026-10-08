# Adversarial review — issue #721 (advisory, never gating)

Method: re-read the frozen gate logs, then rebuilt the target in a scratch copy and ran four
probe tests against the **patched** production code (`reconcile_step`, the real control point,
with the new test file's own fixture helpers). Probe results quoted below are runs, not
readings.

## Refutations

- **NEEDS-HUMAN [impl] — `crates/core/src/metadata.rs:2900-2908`: the freshly-read `seg:`
  record is never re-checked against the root `SegmentRef` the move selected at `:2882-2888`,
  so a racing rewrite whose bounds contradict the root is repointed and the obligation
  DRAINED.** The resolver enforces exactly this invariant (`record.byte_offset()/byte_len()`
  vs. the root's entry, `crates/core/src/metadata.rs:2607`); the write side drops it and
  derives `within` from the record's *own* claim (`:2903`). Concrete failing case, run:
  leg 3's fixture and window (`Race::AfterSegmentPage`), racer writes
  `SegmentRecord::new([SIBLING, CHUNK], 0)` at `seg:…:1` — structurally valid, but it claims
  object bytes `0..16` while the root's table says that segment starts at 8, so it overlaps
  segment 0. `within = 8 − 0 = 8` lands exactly on `prior`, the equality pin matches, and the
  move commits: **outcome `Changed`, `queued_repairs` empty (the obligation deleted), one
  orphan mark published, and `resolve_chunk_map` on the object afterwards returns `Err`.** The
  repair certifies over — and discharges "the last record saying live data is
  under-replicated" for — a map no reader can resolve. Fix is ~3 lines (bounds mismatch →
  `Repoint::Conflict`, which keeps the obligation queued). Reachability today is a foreign /
  future `seg:` writer only (#653 owns the publisher), but that is the same window legs 2–3
  exist to pin. This is also the *correct* form of the three blocking T4 findings
  (`gate-logs/T4-batch-review.log`), whose stated consequence is wrong: the move cannot
  *create* an extent mismatch — `SegmentRecord::new(chunks, record.byte_offset())` (`:2914`)
  preserves both offset and derived length — it silently *acts on* one.

- **NEEDS-HUMAN [human] — `crates/custodian/tests/segmented_map_repoint.rs:577` and `:637`
  assert a property the pass does not have, under a message claiming it does.** Both legs
  assert only `assert_ne!(outcome, Reconciled::Changed)` with the message "the pass must not
  certify a repair it did not make". Run: leg 3's scenario answers **`Reconciled::Satisfied`**
  (probe A) and leg 4's answers **`Satisfied`** with the obligation still queued and the
  rebuilt fragment left on the free server (probe D). `Satisfied` *is* this module's
  certification — its own words at `crates/custodian/src/reconstruction.rs:310-312` and
  `:350-357` ("an operator reading `Satisfied` is being told redundancy is restored"), which is
  why a ceiling refusal is routed to `Blocked` at `:348`. So brief leg 3's criterion ("the pass
  does not certify") is **not met post-fix**, and the assertion is shaped so it cannot notice.
  Tagged `[human]` because the honest alternatives are (a) accept it — a lost repoint race is
  transient and the flat path has always answered `Satisfied` — and correct the leg's message
  and the brief, or (b) make a lost placement CAS non-certifying, which changes the flat arm
  too and is a scope decision, not an iteration.

## Attempted and could not refute

- **The red→green is genuine and on the production path.** `gate-logs/C4-verify.log` shows
  legs 1 and 2 failing on assertions (`Blocked` vs `Changed`) with production reverted, not on
  compilation; the test file names none of the symbols this patch introduces (no `repoint_chunk`
  / `Repoint` / `segment_value_ceiling_crossed` anywhere in
  `crates/custodian/tests/segmented_map_repoint.rs`), everything is driven through
  `reconcile_step` and read back out of the store, and leg 2's `meta.raced()` self-check
  (`crates/custodian/tests/segmented_map_repoint.rs:505`) forecloses a vacuous pass where the race never fired.
- **Two obligations inside ONE `seg:` record do not serialise or corrupt each other** (probe C):
  both land in the same pass with placements `[[0,2],[0,2]]` and an empty queue — the second
  move re-reads the record the first one committed, and the sibling entry it holds still equals
  its own `prior`.
- **The new root CAS on a *segmented* root does not depend on an unproven encode identity** —
  `require(root_key, encode(generation))` (`crates/core/src/metadata.rs:2924`) rests on
  decode→encode byte identity, which is already pinned for the segmented root shape by
  `crates/core/tests/segmented_map_record.rs:142`.
- **Neither `?` in the segmented arm can end a fleet-wide pass for one object's data.**
  `seg_key(...)?` (`:2891`) is unreachable because `SegmentedMap::new` rejects an unaddressable
  index at decode (`:913`), and `SegmentRecord::new(...)?` (`:2914`) cannot newly fail because
  only a `placement` changed, so `checked_chunk_bytes` and the emptiness/span checks see the
  same values the decode already accepted.
- **Offset addressing does not mis-address a zero-length neighbour or a duplicate id.**
  `chunk_at` (`:2947-2959`) breaks only on `at > byte_offset`, so a zero-length chunk sharing an
  offset is walked past rather than hiding its successor; duplicate committed ids are the
  standing #700 deferral and behave exactly as on the base (first reference in key order wins,
  `crates/custodian/src/reconstruction.rs:534`).
- **The rustdoc link from the public `flat_value_ceiling_crossed` to the new private
  `segment_value_ceiling_crossed` (`crates/core/src/metadata.rs:375`)** does raise `rustdoc::private_intra_doc_links`, but it matches five
  pre-existing instances in the same crate (`InodeRecordWire`, `live_lease_guards`, …) and no
  CI job builds rustdoc — house style, not a finding.
- **Budget conformance**: 4 files, ~157 added semantic non-test production lines (the
  `#[cfg(test)]` hunk excluded), `patch.diff` 92.7 KB — all inside the brief's caps.

## Verdict annotations (no action, so the human is not re-litigating them)

- `C5-mutants` "4 missed" is a mutation **scope** artifact, not a test gap: all four are in
  `repoint_chunk`'s flat arm (`crates/core/src/metadata.rs:2858-2860`) and `cargo mutants`
  tested them with `wyrd-core`'s package tests only. I re-applied the `delete field chunk_map`
  mutation by hand and `cargo test -p wyrd-custodian --test reconstruction` fails 5 tests
  (including `kills_a_d_server_and_reconstructs_to_full_redundancy_through_reconcile_step`).
  Do not route C5 back to Do as a coverage defect.
- T4's fourth blocking finding (Tier-0 DST coverage for the new concurrent path,
  `gate-logs/T4-batch-review.log`) is answered by the brief's carve-out to **#722**, which
  forbids touching `crates/dst/tests/custodian.rs` here; per the rubric's "deferrals are
  settled" it wants a decline-with-reference, not a fix in this slice.
- `check-gates.json`'s C4-verify line ("5 test(s) ran red") counts tests *run*, not failed —
  the log shows 2 discriminating failures (legs 1, 2), exactly the posture the brief declared.
