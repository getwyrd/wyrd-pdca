# Re-plan note for #1030 (657.2d), 2026-11-09

#1030 is being re-planned because the slice it builds on, #1029 (657.2c), changed its protocol at
re-plan on 2026-11-09. The previous brief is archived at `results/issue_1030/iteration-v1/brief.md`
(with its plan review). Re-use what still holds; re-check everything that assumed the old #1029.

## What changed in #1029

Read `results/issue_1029/brief.md`, sections *Re-plan decision* and *Design*. In short:

- **After a commit round is answered unknown, #1029 never compensates.** It answers `Committed` on the
  own-commit proof (slot gone, plus a later session read still at `Open@E`). It sends the same batch again
  while that batch may land, and otherwise returns the in-doubt error with the slot still held.
- **Why:** a slot found at the bytes the commit required does not prove the commit did not land. Every
  reserve round puts the same bytes (`crates/core/src/multipart/stage.rs:1296-1298`), and retry rounds at
  one clock reading stamp the same value (`stage.rs:1567-1570`). After the commit deletes the slot
  (`commit.rs:791`), a late reserve, intent and renewal of the same attempt can rebuild exactly those bytes.
  Iteration 2 of #1029 compensated a landed part this way, and was rejected for it.

## What in the old #1030 brief this touches

- Lines 15 and 70 of the archived brief assume #1029 can settle a commit as "did not land" and then send a
  compensation in doubt-free form. Under the new #1029, no compensation follows an unknown commit outcome.
  So #1030's compensations come only from #1028's definite-loss classification and #1027's pre-commit stops.
- The compensation also deletes the slot (`commit.rs:814`). So a slot found at the bytes a compensation
  required is just as weak a proof that the compensation did not land. The same late staging batches can
  rebuild it. The obligation key is a stronger witness for a non-empty part, because no staging batch writes
  `retire:`. For an empty part there is no obligation, so the settle needs other evidence or must keep the
  doubt. Re-derive the "Not landed" rows of the old criterion table (archived brief lines 31-35) with this in
  mind.
- The leg "A held commit beside an unknown empty compensation" (archived lines 43-46) tested that #1029 sends
  no compensation while a commit is unsettled. Under the new #1029 that holds by design; decide whether the
  leg is still needed here.
- Add a cross-phase leg like #1029's (X), aimed at a compensation answered unknown: late reserve, intent and
  renewal batches land after the compensation, before its re-read.

## Intake

This is a re-plan of an already-planned bundle, not new intake. The alpha track is at 18 of 6, and #1030
is two splits deep, so no split can be proposed.
