## Summary
**User impact:** when the custodian repaired a lost piece of an object, or moved one off a
server being drained, it could leave that piece open to early deletion. A piece moved back
onto a server it had left recently could later be deleted while the object still needed it,
cutting the object's redundancy without anyone noticing. A move could also publish a
location for a piece that had already been deleted, or leave a copy on disk that nothing
would ever clean up. Separately, a repair that did not happen could still be reported as
done.

This PR makes both kinds of move mark their destination before writing to it, send the
write with a deadline the storage server enforces, and clear that mark in the same commit
that switches the object over to the new copy. A repair that is refused or loses a race is
no longer reported as finished.

**Stacking:** this builds on #1082 (#1013) and #1097 (#1015) and should merge after them.

## What to look at
- **The shared move rule** in a new module, `crates/custodian/src/premark.rs`. Its header
  comment lays out the five steps (choose, pre-mark, gate, write, adopt) and the one case
  left for later (#1100). Repair and evacuation both call it, so the rule lives in one
  place, as the staged re-place already does.
- **The two callers:** `repair_chunk` in `reconstruction.rs` and `evacuate_chunk` in
  `rebalance.rs`. Each now does pre-mark → check the window → send writes with a
  deadline → adopt.
- **Behaviour change in the pass result:** a pass that still owes a repair now answers
  `Blocked` instead of `Satisfied` or `Changed`. That covers a repair blocked for lack of
  a free failure domain, a refused repair, and a repair that lost its race. One case shows
  up in normal runs: a flat object with several damaged chunks reads `Blocked` on every pass
  but the last, because only one re-point per flat object lands per pass. It takes the same
  number of passes as before. In the deployed server only the one-shot reaper CLI reads this
  result.
- **API change:** `RebalanceContext` gains `clock` and `write_window_millis`, matching
  `ReconstructionContext`.
- **To try it:** `cargo test -p wyrd-custodian --test move_destination_marks` (55 repair
  tests, both flat and segmented objects), `cargo test -p wyrd-custodian --lib rebalance`
  (the evacuation tests). `cargo xtask ci` runs everything, including the simulator replay
  test for the per-move nonce.

## Root cause
The committed repair and evacuation paths wrote the destination fragment and then committed
the re-point with a fresh mark on the **vacated** position only. They never read or replaced
the mark at the **destination**, and they sent destination writes with no deadline. Proposal
0016 X47 and its write gate (`0016:1287-1312`, `:1341-1360`, `:2587`) require a pre-mark at
the destination, a gated write, and an adoption that consumes the pre-mark; the staged
re-place already did all three, the committed paths did none.

## Fix
- **New `premark` module** (`crates/custodian/src/premark.rs`):
  - `choose` (`:226`): picks a destination per fragment. A mark rules out one position,
    never the whole server. A `reclaiming` or unreadable destination mark is never written.
  - `premark` (`:456`): commits a structured mark at each new position under the
    re-point's own pins (the root, and the `seg:` record for a segmented map). For a
    position the placement already names, it deletes any stale mark there instead.
  - `PreMarked::authorize` (`:504-511`): allows the writes once, only while the clock is
    before `stamp + W_REPOINT_MILLIS`. It returns the write deadline `stamp + W_write`.
  - `PreMarked::adopt` (`:529-564`): the re-point batch, plus a requirement that each
    pre-mark is unchanged and a delete of it. The vacated position's mark is written under
    a precondition on what was read there: absent ⇒ require absent; another event's mark ⇒
    re-stamp under its exact bytes; `reclaiming` or unreadable ⇒ left byte for byte.
  - `refusal` (`:600`): one refused write refuses the whole move.
  - `move_event` (`:632-635`): each pre-mark carries a per-move nonce from std's
    `RandomState`. Two moves of one chunk in the same millisecond therefore never share
    an identity. Under madsim a seed still replays to the same run.
- **Callers:** `reconstruction.rs:1336-1369` and `rebalance.rs:667-706` follow
  pre-mark → authorize → write with `Some(deadline)` → adopt.
- **Pass result:** `Assessment::Blocked`, refusals and `RepairOutcome::Conflict` set
  `refused`, so the pass answers `Blocked` (`reconstruction.rs:494-502`). The loss is still
  named `conflict` / `aborted` on the audit seam.
- **Fixtures:** every committed-repair and evacuation test double now refuses a write at or
  after its deadline on the test's clock. Contexts use `W_WRITE_MILLIS` instead of a zero
  window. Race hooks that fired on the move's first commit now fire at the adoption, so they
  still show an adoption losing to a supersede after the write.
- **Docs:** `docs/design/architecture/06-runtime-view.md:40-41`, and GC's doc comments on
  which writers mark before they write (`gc.rs`, comments only).

**Known limits, not changed here:**
- A fragment rebuilt in place can be left unreferenced and unmarked when an unreference
  stamped before the in-place batch commits after it. The bytes are kept, never lost.
  Deferred to #1100.
- Settling an aborted move's pre-marks is #825.
- Seeded simulator coverage of these interleavings is #682.
- Two older gaps, not caused by this change, will get their own issues:
  - The committed movers lack 0016's drain fence on the destination.
  - In a library context whose topology is wider than its fleet, the fallback choice can
    pick a server outside the fleet and abort.

## Verification
- **Claim:** after a move commits, no mark exists at any position the new placement names,
  including after a round trip back to a server within grace (D1, D2).
  **Checked:** `premark.rs:456-494` (pre-mark replaces the destination mark),
  `:548-551` (adoption deletes it).
  **Tests:** `move_destination_marks.rs` `d1` (`:1237`), `d1_in_place` (`:1256`), `d2`
  (`:1307`); evacuation `rebalance.rs` `d1` (`:2069`), `d2` (`:2117`).
- **Claim:** after such a move and a multipart Complete that supersedes the object, GC
  reclaims none of its fragments before the drain marks them (D3).
  **Tests:** `d3_after_d1` / `d3_after_d2` (`move_destination_marks.rs:1366`, `:1376`;
  `rebalance.rs:2188`, `:2198`).
- **Claim:** a `reclaiming` or unreadable destination is never written, and a vacated
  mark is never overwritten blind (D4).
  **Checked:** `premark.rs:386-397` (candidate and position read), `:552-561` (vacated
  precondition).
  **Tests:** `d4_reclaiming`, `d4_unreadable`, `d4_vacated`, `d4_vacated_raced`,
  `d4_destination_raced` (`move_destination_marks.rs:1461-1578`; `rebalance.rs:2240-2307`).
- **Claim:** if GC records reclaim intent on the destination mid-move, the adoption
  fails and no placement names the reclaimed position (D5).
  A move that loses its commit leaves its copy reclaimable, and the pass does not
  certify (D6).
  **Tests:** `d5` (`:1632`), `d6` (`:1723`), `d6_superseded_before_the_premark`
  (`:1807`), `d6_a_segmented_repairs_pre_mark_loses_to_a_seg_record_rewrite` (`:1858`),
  plus the evacuation twins (`rebalance.rs:2369`, `:2441`, `:2477`).
- **Claim:** no write is sent after the pre-mark is `W_REPOINT_MILLIS` old. The refusal
  holds at exactly that instant, and a write 1 ms earlier goes through. A write past its
  deadline is refused by the server double (D7a–d).
  **Checked:** `premark.rs:504-511`; `reconstruction.rs:1348-1356`.
  **Tests:** `d7a`–`d7d` (`move_destination_marks.rs:1937-2062`; `rebalance.rs:2558-2672`).
- **Claim:** each move's identity is its own, and the nonce replays under a fixed seed
  (D8).
  **Tests:** `d8` (`move_destination_marks.rs:2087`, `rebalance.rs:2715`);
  `a_committed_moves_pre_mark_nonce_replays_under_its_seed`
  (`crates/dst/tests/custodian.rs:6869`, runs in `cargo xtask ci`'s simulator leg).
- **Claim:** a refused, blocked or lost repair never reports `Satisfied` (D9, D6).
  **Checked:** `reconstruction.rs:494-502`.
  **Tests:** `d4_no_free_domain` (`:1430`), `d9_unverified` (`:2150`), `mixed` (`:2406`).
- **Claim:** a fragment rebuilt in place is never left under a mark (D10).
  **Tests:** the four `d10_a_*` legs (`move_destination_marks.rs:2230-2245`), `d10b`
  (`:2258`).
- **Red before / green after:** with the production changes reverted and the new test
  file kept, 53 of the 55 tests in `move_destination_marks.rs` fail, each on its own
  assertion. All 55 pass with the change. The other two guard against a pre-mark at an
  in-place position, and go red under that mutation. The evacuation tests and the
  timing/identity legs were each shown red by breaking the line they guard. Examples:
  - the window check moved from `<` to `<=`;
  - writes sent with no deadline;
  - the nonce made constant;
  - `refused = true` removed from the conflict arm.
- **Whole gate:** `cargo xtask ci` passes on the branch: 2700 tests passed, 0 failed.
  That includes typos, docs render, fmt, clippy, deny, and the madsim simulator leg.
- **Not run here:** the nightly Tier-1 disk-fault test and the Tier-2 kill/reconstruct
  docker test. They compile, and their expected verdicts were checked by reading them:
  each seeds one object with one chunk, so none can hit the new `Blocked`-instead-of-
  `Changed` case.

All line numbers refer to this PR's branch.

Fixes #1084
