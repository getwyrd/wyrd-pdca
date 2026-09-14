# PR description

One logical fix per PR.

## Summary
**User impact:** after a large enough delete (roughly 1.8 million fragments
retired at once from one maximum-size object, well within what the system is
supposed to support), garbage collection stops running entirely and never
recovers on its own — every future GC pass fails before it can free anything.
Because the failure is self-reinforcing (the pass that would shrink the
backlog back down is the one that can no longer run), the only way out today
is manual intervention. The same problem blocks the post-restore repair pass
from ever completing after a large restore.

This PR makes garbage collection and the post-restore repair pass read the
list of orphaned fragments in bounded pages instead of in one unbounded read,
so both keep working no matter how large that list gets, and caps how much
each cleanup commits in one transaction so a large reclaim can't overrun the
storage backend's transaction size limit either.

## What to look at
- `crates/custodian/src/gc.rs` — the reclaim loop (`reconcile`) and the new
  `OrphanWindow` type that reads one bounded page-set of the orphan ledger per
  pass, remembering where it left off so the next pass continues from there.
- `crates/custodian/src/restore.rs` — the post-restore pass now checks which
  fragments are already marked by paging through the ledger the same way,
  instead of reading it all at once.
- To exercise it: `cargo test -p wyrd-custodian --test gc_ledger_walk` runs a
  new test file that seeds an orphan list bigger than the backend's read
  limit and drives real GC and restore passes over it — every test fails on
  the old code and passes on the new code.

## Root cause
Both passes read the full set of orphaned-fragment markers with a single
store `scan`, and that call is defined to fail completely — no partial
result — once the result would exceed the backend's cap. A single large
delete can install far more markers than that cap allows, so once the
backlog crosses it, the read that GC depends on starts failing on every
pass, and the backlog can never shrink. GC's cleanup writes had the same
problem in miniature: each pass committed all of its deletes as one
transaction sized by the pass, so a pass reclaiming many fragments could
hand the backend a transaction bigger than it's allowed to accept.

## Fix
GC and restore now read the orphan ledger a bounded page at a time
(`OrphanWindow`), persisting a cursor so each pass resumes where the last one
stopped and wraps back to the start once it reaches the end. Because a pass
now sees only part of the ledger at a time, the rules for what it's allowed
to conclude from a partial read had to tighten to stay safe:
- A fragment whose own marker falls outside the page(s) a pass actually read
  is treated as "unknown", not "unmarked" — so an expired write lease can no
  longer be used to reclaim a fragment whose marker is sitting just outside
  the window, while that marker is still within its grace period.
- A marker whose stored value can't be parsed still counts as a marker (kept
  and logged for an operator to fix), rather than being silently treated as
  no marker at all and reclaimed.
- Only a marker spelled exactly the way the writer spells it can authorize a
  reclaim; a differently-spelled key for the same position is left alone.
- Restore's "is this fragment already marked?" check now walks the entire
  ledger in the same bounded pages instead of one scan, so it never misses an
  existing marker and overwrites it — overwriting would silently reset that
  marker's grace period.

GC's own cleanup deletes now commit in fixed-size batches instead of one
batch sized by the whole pass, bounding transaction size independent of how
much a pass reclaims. That caps the number of writes; it does not by itself
prove those writes complete within the backend's separate time budget for a
transaction — that would need calibration against a real backend, which is
out of scope here and called out in the code comments.

## Verification
- **Claim:** GC and restore complete successfully no matter how large the
  orphan ledger grows, and never conclude "no marker" from a marker that
  simply fell outside the page a pass happened to read.
- **Checked:** `crates/custodian/src/gc.rs:242-260` (the bounded per-pass
  read and its resume cursor) and `crates/custodian/src/restore.rs:412-430`
  (the paged "already marked" check) on the branch this PR targets.
- **Test:** `crates/custodian/tests/gc_ledger_walk.rs` (new) — ten cases
  covering surviving an oversized ledger, exact read/write budgets, no
  starvation across passes, the partial-read safety rules above, restore
  over the same oversized ledger, and refusing a backend page that breaks
  its own read contract. Every case fails on the old code (the ledger read
  fails outright once past the backend's cap) and passes on the new one.
  A companion simulated-storage test also exercises the same walk under
  concurrent fragment deletes.

Fixes #661
