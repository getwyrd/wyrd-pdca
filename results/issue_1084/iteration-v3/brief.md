# custodian: committed repair and evacuation pre-mark their destinations (0016 X47) (636.6c-2) — issue 1014 child-2

> Child 2 of 3 of #1014's split. Do reads ONLY this file. Keep the `- **Label:** value` lines.
> The rule is normative in proposal 0016, `docs/design/proposals/draft/0016-multipart-commit-protocol.md`:
> read the pre-mark rule (`0016:1287-1312`), X47 (`0016:2587`) and X29 (`0016:2568`) first.
> Citations are verified on `f54db83` (#1013's PR head on `main` @ `cdc4047`); re-locate them on
> the base this bundle is given.
>
> **Base.** Do builds on `main` plus #1013's published head (`f54db83`, getwyrd/wyrd PR #1082).
> `main` alone (`57aefe0`) lacks what this child's tests use: the retirement drain and its leg
> in `reconcile_step` (D3 needs the drain), and the custodian's `wyrd-metadata-redb`
> dev-dependency (`crates/custodian/Cargo.toml:65` and its `Cargo.lock` entry on f54db83, added
> by #1061). #1013 reaches the base only if PR #1082 has merged into `main`, or if `1013` is
> named in the same `pdca flow` command (the run then carries it onto its integration line
> before wave 0); otherwise the driver holds this bundle rather than build it on `main`
> (`src/pdca_harness/flow.py`, `_runnable`, #646/#647). **Check the base before writing code:**
> `reconcile_step` calls `retirement::reconcile` (`crates/custodian/src/reconciliation.rs:159-172`
> on f54db83) and `crates/custodian/Cargo.toml` lists `wyrd-metadata-redb` as a dev-dependency.
> If either is missing, stop and say so in `build-notes.md`; do not add the dependency or the
> drain yourself.

- **Slug:** committed-move-destination-premark
- **Track:** alpha
- **Kind:** enhancement (design proposal 0016, X47) / defect
- **Defect:** The committed-path moves — reconstruction's `repair_chunk`
  (`crates/custodian/src/reconstruction.rs:1103`) and rebalance's `evacuate_chunk`
  (`crates/custodian/src/rebalance.rs:516`) — write the destination fragment, then commit the
  repoint with a fresh legacy mark on each **vacated** position only
  (`reconstruction.rs:1226-1240`, `rebalance.rs:634-649`). They never read or touch the mark at
  the **destination** position. 0016 X47 requires the committed repoint to pre-mark its
  destination before writing the fragment and to adopt it in the repoint's commit under a
  precondition on that pre-mark (`0016:1287-1294`, `:2587`); the staged path does exactly that
  (`crates/custodian/src/reconstruction/staged.rs:557-576`, `:634-656`, `:721-751`), the
  committed paths do not. Three failures follow on the base: (1) a fragment moved back onto a
  server within grace (evacuate 5→2 marks `orphan:5:c:i`; a later move 2→5) stays referenced
  under that old mark, and once an unreference that writes no fresh mark comes (multipart
  Complete's supersede today; #1014's routed `unlink` next), GC reclaims it on the old stamp
  before the drain marks it; (2) a destination already carrying an expired mark can have the
  move's freshly written fragment reclaimed by GC before the move commits, after which the commit
  publishes a placement over missing bytes (0016 refutation outcome (c)); (3) a move that loses
  its commit leaves its written fragment unreferenced and unmarked, which GC keeps for ever
  (`gc.rs:740-742`); (4) both movers send their destination writes with no deadline
  (`put_fragment(.., None)`, `reconstruction.rs:1229`, `rebalance.rs:638`), so a pre-mark and an
  adoption precondition alone would still let a write land after GC had reclaimed its evidence:
  0016 also gates the write (`0016:1342-1360`), and the staged path does both
  (`staged.rs:680-687`).
- **Success criterion:** both movers, each on a flat and on a segmented committed object, driven
  **through `reconcile_step`** over `RedbMetadataStore::in_memory()` and a placement-aware
  chunk-store double that enforces `put_fragment`'s deadline on the test's clock, as a D server
  does (`wyrd_traits::WriteDeadlineExpired`). Repair runs with a reconstruction context built as
  the deployed loop builds it (`crates/server/src/custodian.rs:564-580`). Evacuation runs with a
  **library-level `Some(RebalanceContext)`**: the deployed loop passes `rebalance: None`
  (`custodian.rs:591-599`, `:613-621`), and wiring it is out of scope. **Topology:** four D
  servers in four distinct failure domains, the RS(2,1) object's fragments on servers 3, 4 and
  5, server 2 free. Evacuation excludes the draining server and the survivors' domains
  (`rebalance.rs:528-535`) and refuses when no domain is free (`crates/core/src/placement.rs:282-287`), so
  server 2 is the only legal destination for server 5's fragment; repair's destination is
  forced the same way. One `wyrd_testkit::ManualClock` backs every context's clock and every
  pass's `now_millis`. **Where the legs live:** the repair legs in the NEW file
  `crates/custodian/tests/move_destination_marks.rs`; the evacuation legs in the `#[cfg(test)]`
  module of `crates/custodian/src/rebalance.rs`, still through `crate::reconcile_step`. They
  must build a `RebalanceContext`, whose literal this child changes (Scope), and C4-verify
  compiles every added test file in one red-leg run, so no added file could hold them and still
  compile on the base. It shows all of, for each mover:
  (D1) A stale legacy mark at the destination position before the move: after the move commits,
  no mark exists at any position the new placement names (rebuilt-in-place positions included),
  and the vacated positions carry their mark as today. (D2) The adversary's round trip: a
  fragment moved from server 5 to server 2 and, within grace, back to 5 (evacuation: drain 5,
  then drain 2 and un-drain 5; repair: lose it on 5, then on 2): after the second commit no mark
  exists at server 5's position. (D3) The consequence: after (D1) or (D2), a
  production multipart Complete supersedes the object; GC-only passes with the clock past the old
  stamp's grace reclaim none of its fragments before the drain marks them, and all are gone after
  those marks' grace. (D4) A destination whose mark is `reclaiming`, or undecodable, is never
  written: the move picks another server or aborts, and names it on the audit seam. (D5) GC
  records its reclaim intent on the destination's mark while the move is in flight (a wrapper
  runs a GC pass between the destination write and the move's commit): the move's commit fails,
  no placement names the reclaimed position, and a later pass repairs the chunk. (D6) A move that
  loses its commit (a wrapper changes the root first): its written destination fragment is
  reclaimed by GC after grace, not kept for ever. (D7) **The write is gated, not only the
  adoption.** Server 2 already holds an unreferenced copy of the fragment under an aged mark (an
  earlier move that lost its commit). (D7a) The store wrapper holds the move right after its
  pre-mark commit lands; the clock moves past `W_REPOINT_MILLIS` (`gc.rs:218`) and past the
  pre-mark's grace; GC-only passes reclaim the old copy and delete the pre-mark's key; the move
  is released and sends **no** write to server 2 (the double records none) and aborts. (D7b) The
  double holds the destination `put_fragment` in flight; GC-only passes past the pre-mark's grace
  reclaim the old copy and the pre-mark; the double then refuses the write, its deadline (the
  pre-mark's stamp plus the write window) having passed, and the move aborts. After either, no
  fragment at server 2 lacks both a reference and a mark, no placement names a missing
  fragment, and a later pass moves the chunk.
- **Falsifiability:** RED is reachable in-process on `cargo test -p wyrd-custodian`. **On the
  base**, with existing APIs: (D1)/(D2) the destination mark survives, (D3) GC reclaims early,
  (D4) the move writes onto a `reclaiming` destination, (D5) the commit lands over a reclaimed
  position, (D6) the fragment is held for ever; the repair legs in the new file earn
  C4-verify's red. (D7) is red on the two mutations named under Verification posture: a build
  that pre-marks and adopts under the precondition but leaves the write ungated strands
  unmarked bytes there, which is the attack the plan review raised.
- **Invariant to restore:** **C-1** (`docs/principles.md` §5 C-1, §6 row *storage lifecycle /
  reclamation*): every durable fragment is, at every instant, named by a record or evidenced for
  reclamation by a mark whose grace a reader can rely on; a mark never outlives, on a position a
  committed reference names, the event it evidenced; no commit publishes a placement over a
  position GC has decided to reclaim (0016 outcome (c), `0016:2530-2535`); and no fragment is
  written after its evidence may have been reclaimed (`0016:1342-1360`). **Sources:** C-1
  (maintainer's rule 2026-07-25); 0016 X47/X29, the pre-mark rule and the write gate; `metadata.rs:154-169`
  ("once every move that re-places a chunk adopts under its mark's precondition, nothing can
  reference that position again before its key is gone"). SELF-TEST: it binds both movers and
  GC's reclaim intent; guarding one module cannot satisfy it.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** 1013
- **Ordering note:** wave 1 of #1014's split, **first**: in this split child-1 = #1083, child-2 =
  #1084, child-3 = #1085, and both other children now depend on this one. Child-1 does because
  its cleanup marker stays valid only while no move can recreate a stale mark on a referenced
  position (plan review 2026-11-18); child-3 because routing removes the inline re-stamp that
  today hides a stale destination mark. This child's docs edit is confined to
  `06-runtime-view.md:40-41`, child-1's to the GC paragraphs. `1013` is in `Depends on` for the
  drain (D3), the redb dev-dependency, and the `ReconstructionContext` fields of #1013's stack
  (`crates/custodian/src/reconstruction.rs:119-156` on f54db83) this child's tests construct;
  see *Base* at the top. **Intake-cap override (wyrd-pdca-P1):** the human's 2026-11-18
  override for #1014's split.
- **Scope:** the committed-path repair and evacuation moves keep 0016 X47's destination rule and
  its write gate (`0016:1287-1312`, `:1342-1360`), as the staged re-place does: a pre-mark
  committed before any write; writes authorized only while the pre-mark is younger than
  `W_REPOINT_MILLIS` and sent with a deadline, the pre-mark's stamp plus the write window, that
  the D server enforces; an adoption that requires and consumes the pre-mark. So no mark older
  than the move survives on a position the move's new placement names, the move never publishes
  over a position GC has decided to reclaim, no write lands after its evidence may be gone, and
  a move that does not commit leaves what it wrote reclaimable. **The clock:** both movers read
  a move's time from one clock, the context's `wyrd_testkit::Clock` (ADR-0009, ADR-0024;
  `ReconstructionContext::clock`, `reconstruction.rs:119-133`), and take the window from the
  context (`staged_write_window_millis`, `:134-141`, deployed as `gc::W_WRITE_MILLIS`,
  `crates/server/src/custodian.rs:132`). `RebalanceContext` (`rebalance.rs:91-99`) has neither,
  so it gains the same two fields, documented the same way, and every struct literal of it is
  updated: `crates/custodian/tests/{placement_ceiling,rebalance,segmented_map_evacuate,segmented_map_rebalance,staged_drain_status}.rs`
  and `crates/dst/tests/custodian.rs` (built only under `--cfg madsim`, so check it with
  `cargo xtask ci`). The docs. / out of scope:
  - the staged re-place (already keeps the rule) and #825's residuals (settling an aborted
    move's pre-marks; a write the D server answered `WriteEffect::Unknown`), which apply here
    equally and stay #825's;
  - production wiring of rebalance: the deployed loop passes `rebalance: None`
    (`crates/server/src/custodian.rs:591-599`, `:613-621`), and the evacuation legs use a
    library-level context;
  - backfill and every other caller of `commit_chunk_map` that does not move a fragment;
  - GC, the cleanup and the marker (child-1); routing (child-3);
  - seeded DST for these interleavings (#682); `Cargo.lock`, `deny*.toml`, 0016, any ADR.
- **Repro instruction:** on `f54db83`: a committed RS(2,1) object over **four** D servers in four
  distinct failure domains, its fragments on servers 3, 4 and 5, server 2 free; put the legacy
  value `1000` at `orphan_key` for that fragment's position on server 2; mark server 5 draining
  and run `reconcile_step` with a library-level `Some(RebalanceContext)`; after the pass the
  inode's placement names server 2 and `orphan:2:<chunk>:<index>` still holds `1000`. (With
  only three servers the evacuation aborts: no free domain is left, `rebalance.rs:528-535`.)
- **Difficulty:** high
- **Do model:** opus
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/custodian/tests/move_destination_marks.rs`, a **NEW** file holding the
  repair legs (dry-run of `run-verify.sh --classify`: `ADDED_TEST`). The evacuation legs are
  inline in `crates/custodian/src/rebalance.rs` (Success criterion says why). Crates touched:
  `crates/custodian` (source and tests) and `crates/dst` (one test file's struct literals).
- **Verification posture:** default. The new file builds no `RebalanceContext` and uses nothing
  this child adds, so it compiles on the base and earns C4-verify's red there; the inline
  evacuation legs get their red from the mutations below. **Demonstrated reds (recorded in
  `build-notes.md`, each mover):** destination mark left in place → (D1)/(D2); the destination's
  mark not required at the commit → (D5); a `reclaiming` destination accepted → (D4); writes
  authorized with no `W_REPOINT_MILLIS` check → (D7a); writes sent with no deadline (`None`)
  → (D7b).
- **Citations expected:** Do must cite `path:line` on the target branch for every change.
  - **The peer to mirror:** the staged re-place — reading the destination's mark and refusing a
    `reclaiming` or undecodable one (`position`, `crates/custodian/src/reconstruction/staged.rs:557-576`),
    the pre-mark commit before any fragment write (`:634-656`, its event `move_event` `:805`), the
    write gate and store-enforced deadline (`:680-687`; `W_REPOINT_MILLIS` `gc.rs:218`,
    `W_WRITE_MILLIS` `gc.rs:202`), and the adoption that requires and deletes the pre-mark and
    marks the vacated position (`:721-751`).
  - **The two committed movers:** `repair_chunk` (`crates/custodian/src/reconstruction.rs:1103`,
    writes `:1226-1230`, vacated marks `:1232-1237`, commit `:1240`); `evacuate_chunk`
    (`crates/custodian/src/rebalance.rs:516`, copies `:634-640`, vacated marks `:642-647`,
    commit `:649`); the repoint they share (`metadata::repoint_chunk`).
  - **GC's side:** reclaim intent before deletion (`crates/custodian/src/gc.rs:768-790`); the
    no-mark arm that keeps an unmarked fragment (`:740-742`).
  - **Docs:** `docs/design/architecture/06-runtime-view.md:40` (Reconstruct) and `:41`
    (Rebalance) only.
- **Prior-art check (triage cycles):** by path, on `main`: `reconstruction.rs` (659ed31,
  9599aad, 19cec1a, 5377850) and `rebalance.rs` (f481cd3, 0538720, c40948a): none touches a
  destination mark on the committed path; the staged path got the rule in 5377850 (#826). Open:
  #825 (staged pre-mark settling, related, not overlapping). Tracker search for "destination
  mark" / "stale mark repair rebalance": nothing. Raised by #1014's iteration-2 adversary and
  reproduced through `metadata::repoint_chunk` by its iteration-3 reviewer.
- **Disposition hint:** likely-fix

## Docs currency

`AGENTS.md:169-172`: `06-runtime-view.md` steps 2 and 3 (`:40-41`) say the move marks its
destination before writing it, sends the write only while that mark is young and with a
deadline the D server enforces, and clears the mark in the commit that adopts the fragment, so
a mark never outlives its event on a referenced position.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR MAY
happen during the cycle (useful for CI feedback). The PR MUST NOT be marked ready before
sign-off accepts.

Plan-review response (2026-11-18, revision pass): checked on f54db83. (1) The repro and the topology now use four servers in four domains; three leave no free domain. (2) The write gate and store-enforced deadline are in scope for both movers, with D7a/D7b and a named clock source; `RebalanceContext` gains the clock and window, so the evacuation legs move inline (no added test file can compile on both sides). (3) The redb dev-dependency is on the real base (`crates/custodian/Cargo.toml:65` on f54db83); it is missing only on `main`. (4) Evacuation runs through a library-level `Some(RebalanceContext)`, and production wiring is out of scope. (5) A *Base* paragraph names f54db83 (PR #1082), says how the driver supplies it or holds the bundle, and tells Do to stop if it is missing. Difficulty is raised to high and Do is pinned to opus, because of the added write gate and the struct change.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Check was skipped (dependency_halt) on the builder's own proposed `root + device-mapper` row, which is not registered in [[doctor.checks]]; no gate, reviewer or adversary has seen this patch. Keep the approach (premark.rs shared by both movers, D1–D7 legs) — the rejection is about verification, not design. Next build: do NOT declare `root + device-mapper` (or docker) as a blocking external dependency. `crates/custodian/tests/tier1_disk_faults.rs` (and the docker tier scenarios) are nightly-only tiers: keep them compiling, list them under manual validation in build-notes.md, and leave the run to the nightly tier jobs / the job VM (pdca-harness #969). Passwordless sudo on this host is not available by decision.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Check was skipped (dependency_halt) on the builder's own proposed `root + device-mapper` row, which is not registered in [[doctor.checks]]; no gate, reviewer or adversary has seen this patch. Keep the approach (premark.rs shared by both movers, D1–D7 legs) — the rejection is about verification, not design.
  Next build: do NOT declare `root + device-mapper` (or docker) as a blocking external dependency. `crates/custodian/tests/tier1_disk_faults.rs` (and the docker tier scenarios) are nightly-only tiers: keep them compiling, list them under manual validation in build-notes.md, and leave the run to the nightly tier jobs / the job VM (pdca-harness #969). Passwordless sudo on this host is not available by decision.
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Keep the design (shared premark.rs for both movers: pre-mark, gated write with deadline, adoption consuming the pre-mark; the D1–D7 legs). Fix: - REGRESSION — in-place destinations: an in-place repair (`dest.dserver == dest.vacated`) now writes a structured pre-mark on a position the current placement names (`premark.rs:207-214`); if the pass crashes or loses adoption after the write lands, the next healthy pass drains the obligation without removing it, and a later supersede lets GC reclaim the fragment immediately on the old stamp (reproduced by all three reviewers, flat and segmented; base passes). Handle already-referenced destinations safely — e.g. delete the stale mark under an exact-bytes precondition plus the repoint pins instead of writing a fresh pre-mark, or have the healthy-drain path remove a `repoint:` pre-mark from a named position. Add the adversary's crash-before-adoption regression on both map shapes, and correct `06-runtime-view.md:40` if needed. - `choose` (`premark.rs:157-163`): keep exclusions per (server, fragment) position, not per server, and re-assign the way the staged peer does (`reconstruction/staged.rs:406-460`, reuse it, don't write a third copy). Cover a multi-fragment swap case; keep `assess`'s "mirrors the selector" comment true. - Tests that must become load-bearing: (1) a deadline refusal while the pre-mark still stands and the destination holds nothing (advance past `W_WRITE` but inside grace) — assert no adoption, the obligation kept, and the refusal audit event; this must kill the `premark::refusal -> Ok(None)` survivor; also cover the `write-unverified` arm and the `emit_move_aborted` audit lines. (2) Pin the `W_repoint` gate at its exact boundary (`premark.rs:237`, kills `<`→`<=` and a widened gate). (3) DST `crates/dst/tests/custodian.rs:6267`: assert the exact destination key `orphan_key(3, frag(drained))`, and make the `Rewrite` rival obey adoption semantics. (4) Existing committed-repair DST fixtures: give them a real write window and make `MemDServer` enforce deadlines on the shared clock. - Not this round: Tier-1/Tier-2 container scenarios stay nightly/manual (list them under manual validation in build-notes; do not declare them as blocking dependencies). The plan-advisory §6 items about the brief are already addressed by the revised brief.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Keep the design (shared premark.rs for both movers: pre-mark, gated write with deadline, adoption consuming the pre-mark; the D1–D7 legs). Fix:
  - REGRESSION — in-place destinations: an in-place repair (`dest.dserver == dest.vacated`) now writes a structured pre-mark on a position the current placement names (`premark.rs:207-214`); if the pass crashes or loses adoption after the write lands, the next healthy pass drains the obligation without removing it, and a later supersede lets GC reclaim the fragment immediately on the old stamp (reproduced by all three reviewers, flat and segmented; base passes). Handle already-referenced destinations safely — e.g. delete the stale mark under an exact-bytes precondition plus the repoint pins instead of writing a fresh pre-mark, or have the healthy-drain path remove a `repoint:` pre-mark from a named position. Add the adversary's crash-before-adoption regression on both map shapes, and correct `06-runtime-view.md:40` if needed.
  - `choose` (`premark.rs:157-163`): keep exclusions per (server, fragment) position, not per server, and re-assign the way the staged peer does (`reconstruction/staged.rs:406-460`, reuse it, don't write a third copy). Cover a multi-fragment swap case; keep `assess`'s "mirrors the selector" comment true.
  - Tests that must become load-bearing: (1) a deadline refusal while the pre-mark still stands and the destination holds nothing (advance past `W_WRITE` but inside grace) — assert no adoption, the obligation kept, and the refusal audit event; this must kill the `premark::refusal -> Ok(None)` survivor; also cover the `write-unverified` arm and the `emit_move_aborted` audit lines. (2) Pin the `W_repoint` gate at its exact boundary (`premark.rs:237`, kills `<`→`<=` and a widened gate). (3) DST `crates/dst/tests/custodian.rs:6267`: assert the exact destination key `orphan_key(3, frag(drained))`, and make the `Rewrite` rival obey adoption semantics. (4) Existing committed-repair DST fixtures: give them a real write window and make `MemDServer` enforce deadlines on the shared clock.
  - Not this round: Tier-1/Tier-2 container scenarios stay nightly/manual (list them under manual validation in build-notes; do not declare them as blocking dependencies). The plan-advisory §6 items about the brief are already addressed by the revised brief.
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 34 mutants tested in 5m: 5 missed, 20 caught, 8 unviable, 1 timeouts
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 12 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1084/review
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: Re-plan to answer one open design question; the shared pre-mark design for fresh destinations (pre-mark, deadline-gated write, adoption consuming the pre-mark; D1–D7) is kept. OPEN DESIGN QUESTION (Plan must decide and write into the brief): how an IN-PLACE repair's written fragment stays covered by reclamation evidence against a delayed move's vacate. Two approaches have failed review: round 2 — a structured pre-mark on a position the placement names (crash/lost adoption left it, later supersede let GC reclaim on the old stamp); round 3 — no pre-mark, relying on later unreferences carrying later stamps (a competing move samples its vacated-mark stamp before commit, premark.rs:470; a delayed adoption installs an old T0 mark, GC sweeps it, the in-place write then lands unmarked — reviewer probe, both map shapes). Candidate directions: stamp the vacated mark at commit time; make the move's commit conflict with an in-flight in-place repair at that position; or take in-place repair out of this slice / route it through the normal move path. Decide with the GC sweep rules in view (gc.rs:255-263, :862-865 become false under this patch and must be corrected) and keep it distinct from #825's residuals. Already-answered items the brief must state explicitly: - Per-move identity: pre-mark values must carry a fresh per-move nonce (0016:1179). chunk id + millisecond stamp (premark.rs:402/:516) lets a delayed pre-mark recreate another move's exact bytes after GC, and adoption publishes over missing bytes. Add the delayed-pre-mark/adoption regression, flat and segmented. - Refusals must not certify: destination admission Blocked (reconstruction.rs:900) and RepairOutcome::Aborted from pre-mark-stale / write-deadline-expired / write-unverified (:1284) must yield a non-Satisfied outcome with the obligation kept, as evacuation already returns Blocked; give them their own reason instead of "unplaced". D4/D7 repair tests must assert the outcome. Test fidelity to require: committed-repair fixtures (reconstruction.rs tests, segmented_map_repoint, segmented_map_reconstruction, staged_protection, server custodian_day_one) use a real write window (W_WRITE_MILLIS) and doubles that enforce the deadline on the context clock (11 of 15 go red otherwise); D5 must release adoption while the key still holds `reclaiming`, both movers and shapes. Consider whether repair and evacuation should be separate slices over a shared pre-mark slice (logic ~980 lines; patch 210 -> 296 KB).
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Re-plan to answer one open design question; the shared pre-mark design for fresh destinations (pre-mark, deadline-gated write, adoption consuming the pre-mark; D1–D7) is kept.
  OPEN DESIGN QUESTION (Plan must decide and write into the brief): how an IN-PLACE repair's written fragment stays covered by reclamation evidence against a delayed move's vacate. Two approaches have failed review: round 2 — a structured pre-mark on a position the placement names (crash/lost adoption left it, later supersede let GC reclaim on the old stamp); round 3 — no pre-mark, relying on later unreferences carrying later stamps (a competing move samples its vacated-mark stamp before commit, premark.rs:470; a delayed adoption installs an old T0 mark, GC sweeps it, the in-place write then lands unmarked — reviewer probe, both map shapes). Candidate directions: stamp the vacated mark at commit time; make the move's commit conflict with an in-flight in-place repair at that position; or take in-place repair out of this slice / route it through the normal move path. Decide with the GC sweep rules in view (gc.rs:255-263, :862-865 become false under this patch and must be corrected) and keep it distinct from #825's residuals.
  Already-answered items the brief must state explicitly:
  - Per-move identity: pre-mark values must carry a fresh per-move nonce (0016:1179). chunk id + millisecond stamp (premark.rs:402/:516) lets a delayed pre-mark recreate another move's exact bytes after GC, and adoption publishes over missing bytes. Add the delayed-pre-mark/adoption regression, flat and segmented.
  - Refusals must not certify: destination admission Blocked (reconstruction.rs:900) and RepairOutcome::Aborted from pre-mark-stale / write-deadline-expired / write-unverified (:1284) must yield a non-Satisfied outcome with the obligation kept, as evacuation already returns Blocked; give them their own reason instead of "unplaced". D4/D7 repair tests must assert the outcome.
  Test fidelity to require: committed-repair fixtures (reconstruction.rs tests, segmented_map_repoint, segmented_map_reconstruction, staged_protection, server custodian_day_one) use a real write window (W_WRITE_MILLIS) and doubles that enforce the deadline on the context clock (11 of 15 go red otherwise); D5 must release adoption while the key still holds `reclaiming`, both movers and shapes.
  Consider whether repair and evacuation should be separate slices over a shared pre-mark slice (logic ~980 lines; patch 210 -> 296 KB).
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 55 mutants tested in 9m: 39 caught, 13 unviable, 3 timeouts
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 11 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1084/review
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
