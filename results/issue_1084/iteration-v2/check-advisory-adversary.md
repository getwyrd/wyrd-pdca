# Adversarial review — #1084 (committed repair and evacuation pre-mark their destinations)

**Verdict: refuted in one place, with three test gaps behind it.** The fix itself leaves a
stale mark on a position the placement still names when an in-place repair dies (or loses its
adoption) after its write lands. That is the D3 failure the brief set out to remove, and it is a
regression: the base passes the same test. Separately, the patch's 42 legs (22 repair, 20 inline
evacuation) all still pass with two of its safety checks weakened, so the red→green evidence for
the write gate and the deadline-refusal path is weaker than the brief's posture says.

How checked: copied `$PDCA_TARGET` into a scratch dir, added tests to
`crates/custodian/tests/move_destination_marks.rs` that use only the existing rig, and ran
`cargo test -p wyrd-custodian` (integration file and `--lib rebalance::tests`) on the fix, on the
fix with mutants applied, and with the production files reverted to the base.

## Findings

- NEEDS-HUMAN [impl] — **An in-place repair that crashes after its write leaves its pre-mark on a referenced position, and GC later reclaims early (regression, reproduced).** `crates/custodian/src/premark.rs:207-214` writes a structured pre-mark at the destination even when it is rebuilt in place (`dest.dserver == dest.vacated`, `premark.rs:263-266`), so the mark sits on a position the current placement names. If the pass dies after the write lands and before the adoption (`crates/custodian/src/reconstruction.rs:1279`), or the adoption loses, or a landed write answers `write-unverified`, the next pass finds the fragment intact and drains the obligation as healthy (`reconstruction.rs:916-918`) without touching the mark. GC skips the position while it is referenced (`gc.rs:666-673`), and the sweep keeps structured marks, so the pre-mark stays indefinitely. When a multipart Complete later supersedes the object, GC reclaims the fragment at once on the pre-mark's old stamp (`gc.rs:686-703`), before the retirement drain marks it. **Test results:** with server 2 down (`WITHOUT_FREE`, so the fragment is rebuilt on 5), the pass is dropped at its `repoints` commit and the next pass is run. On the fix, the flat test fails `assert_no_mark_on_placement` with `FragmentId { .. index: 2 } on 5 is named by the placement and marked {"orphaned_at_millis":10000000,"event":"repoint:…c0ffee00:10000000"}`. The flat and segmented tests fail `supersede_then_reclaim` with `GC reclaimed … on 5 before the drain marked it`. **All three pass on the base** (production files reverted), because the base writes no mark in place. This breaks the brief's C-1 clause ("a mark never outlives, on a position a committed reference names, the event it evidenced") and the premise child-1 (#1083) depends on. The docs claim at `docs/design/architecture/06-runtime-view.md:40` ("A mark therefore never outlives its event on a position a committed placement names") is false for this case. The `deferred: #825` note (`premark.rs:62-67`) is about aborted pre-marks over *unreferenced* bytes, not a mark left on a named position, so I do not read it as covering this. One possible fix: for an in-place destination, delete the stale mark (under a precondition on its exact bytes plus the repoint pins) instead of writing a fresh pre-mark. GC never reclaims a referenced position, and a supersede mid-move leaves the position for the drain to mark. Another: have the healthy-drain path remove a `repoint:` pre-mark from a named position. The test, using the existing rig only:
  ```rust
  let rig = Rig::new(shape).await;
  rig.lose(SOURCE).await;
  let at = rig.clock.now_millis();
  let hold = Hold::default();
  rig.store.arm(repoints, Act::Before(hold.clone()));
  {
      let pass = rig.repair(&WITHOUT_FREE); // rebuilt in place on 5
      tokio::pin!(pass);
      tokio::select! {
          _ = &mut pass => panic!("never reached its adoption"),
          () = hold.reached.notified() => {} // dropped here: the crash
      }
  }
  rig.clock.advance(1_000);
  rig.repair(&WITHOUT_FREE).await;          // finds the chunk whole, drains it
  assert!(!rig.owed().await);
  rig.assert_no_mark_on_placement().await;  // red on the fix, green on base
  supersede_then_reclaim(&rig, at).await;   // red on the fix, green on base
  ```

- NEEDS-HUMAN [impl] — **The deadline-refusal path is untested: neutralizing it passes every leg (reproduced).** `premark.rs:278-294` (`refusal`), called at `reconstruction.rs:1271` and `rebalance.rs:700`. C5 lists `refusal -> Ok(None)` as MISSED. I confirmed it: with `refusal` returning `Ok(None)`, all 22 repair legs and all 20 inline evacuation legs pass. D7b passes for a different reason than intended. By the time the held write is released, GC has already deleted the pre-mark, so the adoption's precondition fails and the refusal check is never needed. **Concrete failing case:** hold the destination write, advance the clock by `W_WRITE_MILLIS + 1` (still inside grace, pre-mark intact), and release it. The D server refuses the write. Under the mutant the move adopts anyway, and the placement names server 2, which holds nothing (0016 outcome (c)). My test `adv_flat_refused_inside_grace` (assert `placement()[INDEX] == SOURCE` after the refusal) is green on the fix and red under the mutant. The `write-unverified` arm (`premark.rs:287-288`) has no test at all. Both `emit_move_aborted` functions (`reconstruction.rs:1559`, `rebalance.rs:900`) survive as no-ops (C5 MISSED), so no test checks the audit line either.

- NEEDS-HUMAN [impl] — **The `W_repoint` gate's bound is not pinned by any leg (reproduced).** D7a and D7b advance the clock by `GRACE + 1` = 61 s (`move_destination_marks.rs:1277`, `rebalance.rs:2176`). That is past `W_REPOINT` (10 s), the write window (30 s) and the grace, so any gate up to 61 s passes. With `premark.rs:237` gating at `W_REPOINT_MILLIS * 5` (50 s, beyond the write window) plus the `refusal` mutant above, all 42 legs pass. C5 also lists `<` → `<=` at `premark.rs:237` as MISSED. A leg that holds right after the pre-mark commit and advances exactly `W_REPOINT` (my `adv_flat_gate_bound`: assert `sent_to(FREE).is_empty()`) catches both mutants. The brief's posture ("writes authorized with no `W_REPOINT_MILLIS` check → (D7a)") only covers deleting the check outright, which is the weakest mutation of the gate.

- NEEDS-HUMAN [impl] — **`choose` drops a whole server when only one of its positions is unusable.** At `premark.rs:160-163`, one bad (server, index) position rules out the server for every index. The staged peer the brief said to mirror tracks unusable positions per (server, index) and re-assigns by bipartite matching (`crates/custodian/src/reconstruction/staged.rs:406-460`; "the server may still take another fragment", `staged.rs:556`). **Concrete case (from reading the code; the rig's RS(2,1) chunk cannot lose two fragments):** RS(2,2) with indices 2 and 3 missing, and two free servers C and D in distinct domains, picked in the order [C, D]. If `orphan:C:<chunk>:2` is unreadable, C is dropped. D alone cannot host two fragments, so `choose` returns `None` and the repair aborts on every pass, even though 2→D, 3→C is legal. An unreadable mark never clears on its own, so the repair never finishes. `assess` still classifies the chunk `Repairable`, because it checks only the bare selector (`reconstruction.rs:871-879`), so the chunk keeps the repairable-backlog gauge above zero. The comment at `reconstruction.rs:869` ("This mirrors the selector `repair_chunk` runs") is no longer true. Lower severity than the three findings above.

- NEEDS-HUMAN [impl] — **The DST losing-branch assertion is loose.** `crates/dst/tests/custodian.rs:6267` accepts any single structured mark whose key differs from the vacated key. A build that pre-marks the wrong key (for example the wrong fragment index) still passes, and the copy's real destination is left unmarked. The fixture knows the destination, so it should assert that exact key. Minor.

- NEEDS-HUMAN [human] — **The Tier-1 Jepsen and Tier-2 kill-reconstruct edits are unverified this round; toolchain unavailable, so this verdict is provisional.** Their crash trigger moved from "any positive precondition" to "puts an `inode:` key" (`crates/chunkstore-grpc/tests/tier1_jepsen_consistency.rs:246`, `tier2_kill_reconstruct.rs:207`). Their clock moved to `SystemClock` and their write window to `W_WRITE_MILLIS`. C4-ci shows both as `ignored` (`gate-logs/C4-ci.log:737-738, 767`: they need containerized D servers). The trigger ignores `seg:`, which is fine only while these tests use flat objects. Per AGENTS.md they need a nightly or manual run, and a Tier-1 follow-up is warranted. This is not a refutation.

## Attempted to refute, could not

- **Same-millisecond pre-mark identity** (`premark.rs:301`, raised by T4). Moves run one after another under one leader. Every pre-mark is written under a precondition on the exact bytes read, and the adoption consumes only an exact match. I found no interleaving where two byte-identical marks let an adoption publish over missing bytes.
- **GC racing a non-in-place destination.** I tried GC's swap landing before `choose`, between `choose` and the pre-mark, and between the pre-mark and the adoption. Each one makes the move or GC lose on an exact-value precondition, and D5 covers the last case. D2's round trip re-stamps the vacated legacy mark under a precondition, as intended.
- **Clock.** Both movers read only `ctx.clock`. The deployed `LoopClock` backs both `now_millis` and `ReconstructionContext::clock` (`crates/server/src/custodian.rs:582-587, 618-619`). Evacuation stays library-only (`rebalance: None`), as scoped.
- **Gate evidence.** C4-verify's red is real for the repair file: 22 tests fail with production reverted, and I confirmed one of them (D1) on my own base copy. C4-diff-cov shows "not measured" only because the patch sits on main + #1013, not main. That is an environment issue, not a refutation.

T4-batch-review is already a failing gate with 12 blocking items. Finding 1 overlaps its `premark.rs:213` and `reconstruction.rs:1279` items, now reproduced rather than argued. Findings 2 and 3 overlap its `move_destination_marks.rs:1277` item.

<!-- pdca:leaf-complete -->
