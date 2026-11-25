# Recorded rejections for #1084.
#
# The two "same millisecond" entries recorded in iteration 3 (premark.rs:515 and :510) are deleted
# by the 2026-11-23 re-plan: the iteration-3 reviewer's probe refuted them (a delayed pre-mark
# committed under `require_absent` after GC deleted the first move's mark recreates its bytes), and
# the pre-mark now carries a fresh per-move nonce (premark.rs `move_event`).
#
# Line numbers are on the iteration-7 tree. A decision re-anchors to the nearest finding in its
# file that carries its class and MATCH phrase, so a shifted line still binds.
#
# ── #1100: a fragment rebuilt in place, left unreferenced and unmarked ──
#
# Decided with the human twice. Re-plan 2026-11-23, item 1, option A: the in-place repair's
# late-adoption race is deferred to #1100. Iteration-5 sign-off (2026-11-24): the same class is
# triggered by ANY unreference whose mark is stamped before the in-place batch and commits after
# it (a gateway unlink, a flat overwrite, a competing move's adoption); handle it as docs, not
# code, and widen #1100 to match. The code that writes in place carries `deferred: #1100`
# (premark.rs:481, reconstruction.rs:1351), and gc.rs:263-271, premark.rs:51-58 and
# 06-runtime-view.md:40 name the widened exception.
#
# Two entries, because the class is reported at two sites: the vacated mark's stamp (the competing
# move's side), and the in-place batch (the in-place repair's side, where the iteration-5 batch
# reported the unlink / DELETE / overwrite trigger twice).
crates/custodian/src/premark.rs:546 | BUG | concurrent in-place repair | Deferred — tracked in #1100. A competing move's adoption built before an in-place repair's pre-mark batch and committed after it carries a vacated-mark stamp older than that batch, so GC may retire the mark before the in-place write lands, and the fragment is then left unreferenced and unmarked: bytes GC keeps for ever, never bytes lost. The brief (re-plan 2026-11-23, item 1, option A) defers it: stamping the vacated mark at commit time is impossible (`MetadataStore` has no commit stamp, and 0016 rules out caller-side timing for when a commit applies, `0016:1304-1312`), and making the competing adoption conflict needs a key an in-place move would change, which none exists for on a segmented object. The race predates this change (on `main` an in-place write has no deadline at all); the code that writes in place carries `deferred: #1100`, and `gc.rs`'s writer doc names it as the sweep's one exception.
crates/custodian/src/premark.rs:484 | BUG | in-place repair | Deferred — tracked in #1100, widened by the iteration-5 sign-off (2026-11-24). An in-place repair writes with no mark ahead of it, so any unreference whose mark is stamped before the in-place batch and commits after it — a gateway unlink (DELETE), a flat overwrite, a competing move's adoption — can have that early mark swept while the position is still empty and the repair's write deadline is still open; the write then lands unreferenced and unmarked: bytes GC keeps for ever, never bytes lost. The sign-off's decision: docs, not code. A pre-mark at a named position is ruled out (it outlives a crash on a referenced position, #1084 round 2), no commit carries a stamp of its own, and 0016 defines no fence an in-place move could take. The exception is named where it is created (`premark.rs:51-58`, `:481`), in GC's sweep premise (`gc.rs:263-271`) and in `06-runtime-view.md:40`. The iteration-5 reviewer did not reopen it ("In-place late writes (two reports): not reopened").
#
# ── Decisions from the iteration-4 T4 batch review (2026-11-23), re-pinned in iteration 6 ──
#
# The iteration-4 sign-off asked for four fixes only and "no new features"; the iteration-4 and
# iteration-5 reviewers both adjudicated these as not defects of this change, and the iteration-5
# sign-off again left them off its fix list. Recorded by the builder on that basis; the human can
# overturn any of them at sign-off by deleting its line.
#
# 1. A `reclaiming` mark on a position the placement already names keeps an in-place repair
#    waiting. The refusal is the brief's D4; what clears such a mark is GC's, which is #1083's.
#    Decided with the human at the iteration-6 sign-off (2026-11-25): "Doc fix only ... Correct
#    the comment to say so and point at #1083; do not change the refusal behaviour." The comment
#    is corrected (`Assessment::Refused`, reconstruction.rs:810-821). The iteration-6 batch
#    reworded the finding ("... repair waits for GC to clear a mark that GC deliberately
#    preserves on referenced positions, leaving the repair blocked indefinitely"), which the
#    iteration-4 phrase did not match, so it blocked again; the MATCH is now the one word every
#    wording of it carries.
crates/custodian/src/premark.rs:167 | BUG | reclaiming | Out of scope — GC's side is #1083 (child-1 of #1014's split: "GC's behaviour, the cleanup and the marker"). The mover's refusal is the rule this change was asked to keep: the brief's D4 says a destination whose mark is `reclaiming` is never written, and no writer replaces a `reclaiming` mark (`crates/core/src/metadata.rs:154-164`); a fragment rebuilt in place is written to a destination like any other. Writing there would put a fragment on a position GC has recorded its decision to reclaim (0016 outcome (c), `0016:2530-2535`). The wait is not silent: every pass names the position (`destination-passed-over`), names the refusal (`repair-refused`), counts the chunk on the repair-blocked level and answers `Blocked`; with any other free failure domain the repair goes there instead. The wait does not end by itself — GC reclaims no referenced position, so it never finishes that reclaim — and `Assessment::Refused`'s doc now says so and names #1083 (iteration-6 sign-off: doc fix only, the refusal behaviour unchanged). Whether GC should clear a `reclaiming` mark left on a referenced position whose fragment is missing is a change to GC's behaviour, which this bundle may not make (brief, out of scope) and #1083 owns.
#
# 2. and 3. One finding seen at two sites and in two rounds: the choice of destinations may offer
#    a server that is in the topology but not in the pass's fleet, and the move then aborts. The
#    iteration-5 batch worded it "servers absent from the fleet" and "a topology entry absent from
#    the live fleet", which the iteration-5 phrases did not match; the second entry's MATCH is now
#    the one word every wording shares.
crates/custodian/src/reconstruction.rs:944 | BUG | servers outside the current fleet | Rejected — the abort is the base's contract, kept on purpose, and the deployed loop cannot reach it. On the base a selector pick outside the fleet view aborts the repair and leaves the obligation for the next pass (`reconstruction.rs:1143` on `6e44de9`, `:1244` with this change), and the brief keeps that ("a destination outside it aborts the repair"). The deployed loop builds the fleet and the topology from the same reachable servers in one loop (`live_reconstruction_view`, `crates/server/src/custodian.rs:293-306`), so the two never differ there; only a library caller that hands in a topology wider than its fleet sees the abort, as it did before this change. Teaching the choice to skip off-fleet servers is a placement-policy change the iteration-4 sign-off ruled out ("no new features").
crates/custodian/src/premark.rs:390 | BUG | fleet | Rejected — the same finding as `reconstruction.rs:927` above, seen from the shared choice. The committed movers' admission offers every server of the pool it is given and leaves the fleet check where it has always been, at the point the store is resolved (`premark.rs:342-345`); a server outside the fleet aborts the move there, as on the base (`crates/custodian/tests/reconstruction.rs`, the off-fleet abort test from #238). Not reachable in the deployed loop, where fleet and topology are built together (`crates/server/src/custodian.rs:293-306`). The iteration-5 reviewer reproduced it and kept the abort: "Do not silently expand this child into a fleet-selection redesign."
#
# ── Decisions from the iteration-5 T4 batch review (2026-11-24) ──
#
# Nine rows. Two are fixed in iteration 6 (the evacuation doubles that dropped the write deadline,
# `tests/segmented_map_evacuate.rs:181` and `tests/rebalance.rs:863`). Two are the #1100 class
# (above). Two are the fleet finding (above). The other three follow. The iteration-5 sign-off
# listed four fixes and said "fix only these, and don't grow the patch beyond them"; the
# iteration-5 reviewer adjudicated each of these three (`iteration-v5/check-review.md`, "The batch
# review's other concerns"). Recorded by the iteration-6 builder on that basis, for the human to
# confirm or delete.
#
# 4. The adoption does not pin the destination server's desired-state record (0016's drain fence).
crates/custodian/src/premark.rs:550 | BUG | drain fence | Out of scope — a real gap against proposal 0016 (`0016:670`, `require_absent(desired:dserver:<S_new>)`), older than this change and outside this child's Scope, which is the destination pre-mark, the write gate and the adoption that consumes the pre-mark (`0016:1287-1312`, `:1341-1360`). The committed movers have never carried the fence: #722 shipped the segmented evacuation without it and deferred "drain fence, rollback and resume" to #653. The iteration-5 reviewer reproduced it and ruled it a tracked scope exclusion, not an in-PR fix. NOTE for the human: #653 is now CLOSED and the committed movers still lack the fence, so the deferral has no open issue behind it; build-notes §6 drafts one. Until it is filed this entry has no live issue reference.
#
# 5. A vacated position whose mark is already `reclaiming` is left as it is by the adoption.
crates/custodian/src/premark.rs:560 | BUG | readers | Rejected — this is the brief's own rule (Scope, *The vacated position's mark*; D4): a `reclaiming` or undecodable mark at the vacated position is "left byte for byte" (`crates/core/src/metadata.rs:154-164`, ADR-0045 decision 3), because no writer replaces a mark GC has recorded its decision on. GC had decided to delete that fragment before the move ran and deletes it whether or not the move adopts; the move does not shorten any reader's grace, and an evacuation copies the bytes out first. Overwriting the mark to restart its grace would break the reclaim-intent boundary D5 depends on. What GC does with a `reclaiming` mark on a position a placement names is #1083's. The iteration-5 reviewer: "not a defect against this brief".
#
# 6. No seeded Tier-0 case for a partial fan-out (one destination write lands, another is refused).
crates/custodian/src/reconstruction.rs:1357 | TEST-GAP | Tier-0 | Deferred — tracked in #682. The brief puts seeded DST for these interleavings out of scope ("seeded DST for these interleavings (#682; the nonce's same-seed replay test is not an interleaving property and stays in scope)"), and the move carries the marker (`reconstruction.rs:1280`, `// deferred: #682`). The all-or-nothing rule itself is one fold over every answer (`premark::refusal`, `premark.rs:600`), exercised with a refused and an uncertified write by D7b, D7c and D9, and since iteration 7 by a scripted mixed-answer leg per mover and shape, one write landed and one refused (`one_refused_write_refuses_a_whole_*_repair`, `one_refused_copy_refuses_a_whole_*_evacuation`); only the seeded form is deferred. The iteration-5 reviewer: "settled under #682".
#
# ── The iteration-6 T4 batch review (2026-11-25) ──
#
# Three blocking rows, none needing a new entry. `reconstruction.rs:1376` (a root rewrite that
# leaves the chunk unrepaired answered `Satisfied`) is fixed in iteration 7: a planned repair that
# lost its race never certifies (`reconstruction.rs:499-502`). `premark.rs:600` (no mixed-answer
# test) is fixed by the legs named in entry 6. `premark.rs:167` is entry 1, re-pinned above.
