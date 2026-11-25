# Adversarial review — issue 1062, iteration 3

**Verdict:** I could not refute the fix. The three test items the human asked for in iteration 2 are met in substance, and every leg I attacked fails for the right reason through `reconcile_step`. Two findings remain. The two `:886` mutants the human asked to have killed still survive, and they are killable (one small test, `[impl]`). Item 4, the unbounded and unaudited arm-(0) wait, is still a pending human decision, and a probe shows it also pins live bytes.

## What I re-ran

- I built the patch in a scratch copy and ran `multipart_retirement_marks` (26 tests) and `gc_mark_sweep` (21 tests). All pass. C4-verify's red is a blanket red: the child-1 base leaves every `retire:bytes:` obligation untouched, so all 26 fail. That shows the tests reach the new mode, but not that each leg is sharp, so I mutated production code one leg at a time. Each mutation turned the named leg red:
  - the `list` `Err` arm made a silent skip (`crates/custodian/src/retirement/marks.rs:774-777`) → `e_an_untrusted_chunk_in_a_chunks_list_is_held`, `e_an_untrusted_chunk_in_a_flat_generation_is_held`;
  - the `seg:` `Err` arm made a silent skip (`marks.rs:857-861`) → `e_an_untrusted_chunk_in_a_segment_record_is_held`;
  - an unreadable mark sent to arm (b) (`marks.rs:259`) → `c_a_mark_of_none_of_the_three_shapes_is_held_and_never_restamped`;
  - `retained: false` for `part:` entries (`marks.rs:699`) → `h_…`;
  - segments-before-parts ignored (`marks.rs:187`) → `g_…`;
  - `evidenced` ignored (`marks.rs:433`) → `b_…`, `d_an_aborted_sessions…`, `s_a_part_larger…`, and three `paging_…` tests;
  - owned-residue hold off (`marks.rs:152`) → `r_…`;
  - arm (0) re-stamped as arm (b) (`marks.rs:261`) → both `c_…reclaiming…`/`c_…no_fragment…` tests and `h_…`;
  - the GC sweep arm dropped, or made to sweep while the obligation is present (`crates/custodian/src/gc.rs:918-924`) → `b9_a_retirement_mark_is_kept_while_its_obligation_is_present_and_swept_after` and `c_a_drain_mark_over_no_fragment_is_swept_only_once_its_obligation_is_gone`;
  - the `:886` guard set to false (`Tail::Ended` after one page) → `paging_a_group_of_small_segments_past_one_page`;
  - the `:603` guard set to false → `paging_an_aborted_sessions_parts_past_one_page`.

## Findings

- NEEDS-HUMAN [impl] — The two `:886` mutants the C5 log still lists as MISSED (`replace match guard index < MAX_SEGMENT_INDEX with true`, and `< with <=`) are **killable, not equivalent**. The code at `crates/custodian/src/retirement/marks.rs:886-887` is correct; no test reaches it. Failing case: a segmented `{generation}` whose last page of 16 `seg:` records ends at index 999,999 (`MAX_SEGMENT_INDEX`), with every position already carrying this obligation's mark. The patch finishes and deletes the obligation in one step. Both mutants instead build cursor entry 1,000,000, which `checked_cursor` refuses (`crates/core/src/multipart.rs:4296`). The step then becomes a `CursorRefused` hold, nothing commits, and the obligation is held for good. I wrote that probe in scratch: 16 planted records at 999,984–999,999, all pre-marked, one drain, assert the obligation is gone and nothing was audited. It passes on the patch and fails on both mutants. Iteration 2's item 3 asked for the `:886` mutants to be killed. The `false` mutant is now killed; these two need this one boundary test. Low severity.
- NEEDS-HUMAN [human] — **Item 4 is still open, and it pins live bytes as well as a ledger row.** Arm (0) over a position no retained record names returns `Planned::Wait(AWAITING_GC)` (`marks.rs:396-399`, `:494`). That wait only increments a counter and logs at debug level (`crates/custodian/src/retirement.rs:438-441`, `:803-812`). It has no time bound and emits no audit event. GC keeps a `reclaiming` mark whose server has left the fleet (`gc.rs:905`, `server-not-in-fleet`), so it never deletes the key. Probe: a `{chunks}` obligation over one RS(2,1) chunk on servers 0–2, a `reclaiming` mark of another event on server 2's position, then server 2 removed from the fleet. Over 20 drain + GC rounds, each past the grace and the late-write deadline, the outcomes were `[Changed, Partial ×19]`, with 19 `awaiting-gc` debug waits and **zero audit events**. The obligation was still present. The two live fragments on servers 0 and 1 were kept, because the drain's own marks protect them while the obligation exists (`gc.rs:695`, `draining-retirement`). The patch rightly left production code alone, per iteration 2's instructions, so the policy choice is still owed. Options: after N waits, emit an audit event or turn the wait into a typed hold (as (H) does), or accept it with a tracking issue.

## Attacks that did not land

- **(T) stamp.** The clock is read for each step after the obligation's bytes are in hand (`retirement.rs:428`). A lost precondition re-reads the obligation and loops back to that read (`:470`). The (T) test would catch the cached pass-time shape. It cannot tell a per-step read from one read at the start of the retirement leg, because the install fires during reconstruction. The code reads per step anyway, so there is no defect.
- **A `part:` placement changing between two steps**, which would let the cursor's "positions before me are marked" shortcut skip a re-placed position. Not reachable: the staged re-place requires `mpu == Open@E` (`crates/custodian/src/reconstruction/staged.rs:16`, `:23`), and every token that carries parts is installed by a transition out of `Open`. Placements in a superseded `seg:` record are stable once the supersede's inode compare-and-swap commits (`0016:2432-2440`).
- **A second component silently dropped.** `plan` drains only one component (`marks.rs:156-164`). However, `checked_against_key` (`multipart.rs:4401`) leaves `{session, parts}` as the only legal pair, and the drain handles both halves.
- **The GC sweep change** (`gc.rs:918-924`) only ever deletes marks, never fragments. Its worst case is an unmarked fragment that GC keeps (a leak, not a loss), as its doc says. Both directions of it are tested (mutations above).
- **The budget mutants** (`marks.rs:366-367`, `:422-423`, `:439-440`) are equivalent. Operations cannot bind before `reads_left` does: 233×2 + 16×2 + 2 = 500 = `MAX_BATCH_OPS`, checked by the const asserts at `retirement.rs:187-191`. Bytes are about 200 KB worst case against `MAX_BATCH_BYTES` = `E_TX_BYTES / 2`. The human already accepted these.

## Gate notes (not refutations)

- C4-diff-cov measured nothing: the bundle stacks on child-1 and does not apply on `origin/main`. This is the same as earlier rounds. The targeted mutations above partly cover what it would have shown.
- T4 reports 0 blocking in 38 s for three review passes over a 277 KB diff, where iteration 2 had 3 blocking. I cannot see `review-batch.md`, so I cannot tell whether this was a fresh deep review. I'm noting it, not raising it.
<!-- pdca:leaf-complete -->
