# Result — issue 1084 / committed-move-destination-premark

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: The committed-path moves — reconstruction's `repair_chunk`
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
- Success criterion: both movers, each on a flat and on a segmented committed object, driven
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
- Repo + branch target: getwyrd/wyrd @ main
- Scope: the committed-path repair and evacuation moves keep 0016 X47's destination rule and
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

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — N/A — close disposition (no patch to verify)
- C3 Change: none — patch.diff
- C4 Verification (red→green): none — N/A — close disposition (no patch to verify)
- C5 Causal adequacy: none — reviewer + human sign-off

## 4. Conformance (Check — stack)
- T1 Structure: none — N/A — close disposition (no patch to verify)
- T2 Shape: none — N/A — close disposition (no patch to verify)
- T3 Runtime: none — N/A — close disposition (no patch to verify)
- T4 Contribution: none — N/A — close disposition (no patch to verify)
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

# Advisory review — SKIPPED (builder-declared external dependency confirmed, #341)

The builder declared an unmet external dependency in build-notes.md and the claim was CONFIRMED deterministically (the named [[doctor.checks]] row's detect cmd exited non-zero), so the Check beat was not spent adjudicating a patch already stated to be unverifiable. Gates are recorded N/A; no reviewer or adversary ran. The bundle is resumable.

- NEEDS-HUMAN — External dependency `root + device-mapper` confirmed absent (detect cmd `sudo -n dmsetup version` exited 1). Install hint: run on a host with passwordless sudo and device-mapper (dmsetup) for `cargo xtask disk-faults`. Provide it and answer iterate-do to resume the full Do+Check band, or discontinue at sign-off.


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] External dependency `root + device-mapper` confirmed absent (detect cmd `sudo -n dmsetup version` exited 1). Install hint: run on a host with passwordless sudo and device-mapper (dmsetup) for `cargo xtask disk-faults`. Provide it and answer iterate-do to resume the full Do+Check band, or discontinue at sign-off.
- [ ] external dependency: docker — the updated `crates/chunkstore-grpc/tests/tier2_kill_reconstruct.rs` and `tier1_jepsen_consistency.rs` scenarios (`cargo xtask kill-reconstruct`, `cargo xtask jepsen`, nightly `tier2-kill-reconstruct` / `tier1-jepsen`) need docker-compose D servers; only their compile-time checks and non-ignored helper unit tests ran here.
- [ ] external dependency: root + device-mapper — the updated `crates/custodian/tests/tier1_disk_faults.rs` (`cargo xtask disk-faults`, nightly `tier1-disk-faults`) needs root and `dmsetup`; it compiled but did not run here.
- [ ] builder-declared external dependency `docker` was REFUTED deterministically — its detect cmd (`docker info`) exited 0, so the dependency is present and full Check ran (#341); weigh the builder's claim against the review before accept
- [ ] **The stated repro cannot evacuate.** `brief.md:80-83` specifies an RS(2,1) object and only three servers in distinct domains, then expects evacuation from 5 to 2. The mover removes draining servers and excludes both survivors' domains (`crates/custodian/src/rebalance.rs:524-535`); the selector rejects insufficient free domains (`crates/core/src/placement.rs:273-286`). A valid three-fragment placement already occupies all three domains, so this pass aborts before reaching the defect. Specify a fourth server/domain, an explicit initial placement, and destination 2 outside the survivors' domains.
- [ ] **The acceptance cases omit the write-side deadline required by the claimed invariant.** D5/D6 inject races after the destination write (`brief.md:44-48`); the cited recipe skips the staged peer's write-window enforcement (`brief.md:93-97`). Both current movers send `put_fragment(..., None)` (`crates/custodian/src/reconstruction.rs:1215`, `crates/custodian/src/rebalance.rs:638`). Merely adding pre-mark/adoption CAS can pass these cases yet strand bytes: pause after pre-marking a destination already holding an unreferenced copy, let GC reclaim that copy and its aged mark, then resume the unrestricted write. The adoption fails but the new bytes have no mark. Proposal `docs/design/proposals/draft/0016-multipart-commit-protocol.md:1341-1360` requires the write gate; the peer implements both that gate and a store-enforced deadline at `crates/custodian/src/reconstruction/staged.rs:680-687`. Add deterministic delayed-authorization/deadline cases for both movers and identify their shared clock source. This concerns ordinary unrestricted writes, not the deferred #825 `WriteEffect::Unknown` residual.
- [ ] **The specified test cannot compile on the supplied base within the stated constraints.** `brief.md:30-31` requires `RedbMetadataStore::in_memory()`, while `brief.md:49-51,86-91` promises a new-file, behaviorally red test through existing APIs and `brief.md:79` excludes `Cargo.lock`. The custodian manifest has no `wyrd-metadata-redb` dependency (`crates/custodian/Cargo.toml:11-60`), and its lockfile dependency list likewise omits it (`Cargo.lock:4975-4992`). A workspace dependency declaration alone does not expose the crate to this test. Specify permitted manifest/lockfile setup for both sides of the red→green comparison, or choose a store already available to this test target. Recheck this requirement after correcting the base below; a compile failure is not the promised behavioral red.
- [ ] **The production-context requirement hides an evacuation wiring change.** `brief.md:31-34` requires both movers with the contexts the deployed loop passes. The cited `crates/server/src/custodian.rs:397` is a generic forwarding method; the actual loop passes `rebalance: None` (`crates/server/src/custodian.rs:583-590,605-612`). Consequently a fixture matching that wiring never evacuates. Revise the criterion to explicitly supply a library-level `Some(RebalanceContext)` for evacuation, or declare production wiring as additional scope; do not leave it implicit under the “only crate touched is custodian” claim (`brief.md:87`).
- [ ] **The resolved main base does not contain the declared prerequisite.** `brief.md:61-67` targets `getwyrd/wyrd @ main`, depends on #1013, and requires its context changes. The repository and main ref resolve, and `dependency-state.json:2-5` confirms #1013 exists and is COMPLETE, but target HEAD equals `origin/main` at `57aefe0`; `git merge-base --is-ancestor f54db83 HEAD` returns 1. The expected stack is also absent from the API: `ReconstructionContext` ends at `crates/custodian/src/reconstruction.rs:141-142`, and `reconcile_step` runs only GC, scrub, reconstruction and rebalance (`crates/custodian/src/reconciliation.rs:138-165`), without the retirement drain D3 needs. Declare a base containing the prerequisite stack, or make landing it on main explicit before claiming the tests run on this base. COMPLETE alone does not establish integration.
- [ ] size backstop — this slice is behaving oversized: patch is 206 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Check was skipped (dependency_halt) on the builder's own proposed `root + device-mapper` row, which is not registered in [[doctor.checks]]; no gate, reviewer or adversary has seen this patch. Keep the approach (premark.rs shared by both movers, D1–D7 legs) — the rejection is about verification, not design. Next build: do NOT declare `root + device-mapper` (or docker) as a blocking external dependency. `crates/custodian/tests/tier1_disk_faults.rs` (and the docker tier scenarios) are nightly-only tiers: keep them compiling, list them under manual validation in build-notes.md, and leave the run to the nightly tier jobs / the job VM (pdca-harness #969). Passwordless sudo on this host is not available by decision.
- By / date: Eduard Ralph / 2026-11-10

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 5 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
- `dependency_halt` skipped all of Check based only on the builder's proposed `root + device-mapper` row (not registered in `[[doctor.checks]]`) for a nightly-only test; human installs dmsetup and registers the row in pdca.toml — consider an upstream issue: an unregistered, builder-proposed dependency shouldn't be enough to skip review.
- Run `WYRD_TIER1=1 cargo xtask disk-faults` against 1084's patch once the disposable job VM lands (depends on eduralph/pdca-harness #969); only register the `root + device-mapper` doctor row then — no passwordless sudo on the host.
