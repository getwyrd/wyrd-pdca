# Build notes — #1084: committed repair and evacuation pre-mark their destinations (0016 X47)

Iteration 3. All `path:line` citations are on the cycle worktree (`$PDCA_WORKTREE`), base
`6e44de9` (`pdca-integration/main-r9dd123c9ed70`, which carries #1013 and #1015) plus this patch,
unless marked *base* or *v2* (`iteration-v2/patch.diff`).

## 0. What this iteration changed (the carry-forward, item by item)

The sign-off kept the design (shared `premark.rs`, pre-mark, gated write with deadline, adoption
consuming the pre-mark, the D1–D7 legs) and asked for five fixes. Against v2 the patch changes 7
files (+1397/−453, `git diff 62bf348 HEAD --stat`).

| Carry-forward item | What I did | Where |
|---|---|---|
| **Regression: in-place pre-mark** on a still-named position | A fragment rebuilt in place gets **no pre-mark**. The pre-mark batch instead deletes whatever stale mark the position holds, under its exact bytes and the repoint's pins, and writes nothing there. The adoption has nothing to consume for it. The batch still commits before any write, so the write gate is anchored at an instant the generation was still current. The adversary's crash-before-adoption leg and a lost-adoption leg, both shapes. Docs corrected. | `premark.rs:32-46` (rule), `:395-431` (pre-mark), `:463-484` (adopt), `:367` (`in_place`); tests `move_destination_marks.rs:1584-1658`; `06-runtime-view.md:40` |
| **`choose` per position**, reuse the staged peer | The staged matcher (`choose_destinations`, `assign`, `augment`, Kuhn matching) moved into `premark.rs` as the one shared `choose`, generic over an `Admission` trait (which servers, which positions). Staged keeps its own admission (fleet + desired-state fence) through `StagedAdmission`; both committed movers use `premark::Committed`. Staged's `position` now reuses `premark::destination_mark`, so the mark classification exists once. No third copy. | `premark.rs:150-310`; `reconstruction/staged.rs:317-327`, `:369-390`, `:432`; `reconstruction.rs:886-901`; `rebalance.rs:556-566` |
| **`assess` "mirrors the selector" must stay true** | The committed repair now chooses its destinations **at the assessment**, as the staged assessment already did, and `repair_chunk` writes to exactly those. A chunk with no usable assignment is `Blocked` (off the repairable-backlog gauge), not `Repairable` then `Aborted` on every pass. The comment now says what it mirrors. | `reconstruction.rs:873-915` (assess), `:203`, `:211-216` (`CommittedTarget`), `:1136-1144` (repair_chunk) |
| Multi-fragment **swap** case | New swap legs through `reconcile_step` on both movers, both shapes, with a `reclaiming` and an unreadable mark each: repair on an RS(2,2) chunk over servers 2,3,4,7; evacuation on the RS(2,1) chunk with servers 4 and 5 draining over five servers. Staged's own swap test (`tests/staged_repair.rs:1208`) now runs the shared code too. | `move_destination_marks.rs:1686`; `rebalance.rs:2467` |
| **Tests (1)**: refusal inside grace; `write-unverified`; `emit_move_aborted` | (D7c) the write held until exactly stamp + `W_write`, inside grace, no GC: refused, not adopted, obligation kept, pre-mark still standing, `move-aborted`/`write-deadline-expired` on the audit seam; one millisecond earlier it is adopted. (D7d) the D-server double now runs the post-publication check (`if_publication_unverified`), and a write held after publishing is answered `write-unverified` and never adopted. D7a/D7b now assert their audit reasons too. | repair `:1516`, `:1535`, `:1399`, `:1429`; evacuation `rebalance.rs:2383`, `:2402`, `:2272`, `:2299`; doubles `move_destination_marks.rs:551-612`, `rebalance.rs:1428-1491` |
| **Tests (2)**: pin `W_repoint` at its boundary | (D7e) a move held after its pre-mark until exactly stamp + `W_repoint` sends no write and aborts `pre-mark-stale`; one millisecond earlier the write is sent and adopted. Kills `<`→`<=`, a widened and a narrowed gate (§4.3). | `move_destination_marks.rs:1567`; `rebalance.rs:2434` |
| **Tests (3)**: DST oracle at `custodian.rs:6267`, and the `Rewrite` rival | The lost-move branch now asserts the exact key `orphan_key(3, frag(drained))`, the move's event `repoint:<chunk>:500` and stamp 500 for the supersede racer. The `Rewrite` rival is now a mover that keeps the rule (`RivalMove`): it pre-marks the destination over whatever it reads there, pinned to the `seg:` record, writes under a deadline, and adopts by consuming its pre-mark and marking the vacated position. A new check runs in every arm: no position the live generation names carries a mark. | `crates/dst/tests/custodian.rs:6093-6177`, `:6369`, `:6404-6433`, `:6447-6470` |
| **Tests (4)**: DST committed-repair fixtures | `MemDServer` judges every deadline-bearing write (before and after storing) on the clock the pass reads, given through `servers_on(&clock)`. A server built with no clock (`servers()`) panics on a deadline-bearing write, so no fixture can run a mover against it unnoticed: that caught three reaper properties that drive production `UploadPart`, which I wired to their own `ManualClock`. Every `staged_write_window_millis: 0` is now `W_WRITE_MILLIS`. | `custodian.rs:285-349`, `:658-671`; the fixtures at `:682`, `:749`, `:851`, `:1100`, `:1179`, `:1270`, `:1399`, the handoff `:2953`, the drain race `:6252`, the drain ceiling `:6583`, reapers `:7707`, `:7777`, `:8029` |
| Tier-1/Tier-2 container scenarios | Not declared as dependencies. They compile (`cargo xtask ci` builds every target); listed under manual validation (§8). | — |

Other T4 batch-review findings: the one I did not fix (same-millisecond pre-mark identity,
v2 `premark.rs:301`) is recorded-rejected with its reason in `review-rejected.md`, matching the
Check reviewer's and the adversary's own conclusion last round. §9 maps every v2 finding.

## 1. Base check (the brief's stop condition)

Both prerequisites are on the base, so I built:

- `reconcile_step` runs the retirement drain: `crates/custodian/src/reconciliation.rs:187`.
- `wyrd-metadata-redb` is a custodian dev-dependency: `crates/custodian/Cargo.toml:65`.

No dependency or drain was added. `Cargo.lock` and `deny*.toml` are untouched.

## 2. The change

### 2.1 The rule, shared (`crates/custodian/src/premark.rs`, new, `lib.rs:27`)

- `destination_mark` (`:129`): the one classification of a destination's mark — absent,
  re-stampable, or unusable (`reclaiming`, unreadable). Used by both committed movers and by the
  staged re-place.
- `Admission` trait (`:150-170`) and `choose` (`:196`, with `assign` `:255` and `augment` `:283`): the
  staged peer's per-position matcher, moved verbatim apart from the two admission calls. A
  position ruled out never rules out its server.
- `Committed` (`:312-351`): the committed movers' admission — every server of the pool, each
  ruled-out position named through the mover's own audit callback. A chosen server outside the
  fleet view still aborts where its store is resolved, as on the base
  (`reconstruction.rs:1169-1173`), so that behaviour did not change.
- `premark` (`:395`): one batch — the repoint's own pins + each destination's mark as read + a
  fresh structured pre-mark on every destination the placement does not already name; a
  destination rebuilt in place gets its stale mark deleted instead (`:419-422`).
- `PreMarked::authorize` (`:442`): `Some(stamp + window)` only while `now < stamp + W_REPOINT`.
- `PreMarked::adopt` (`:463`): the repoint + `require(pre-mark)` + delete per moved destination +
  a legacy mark on each vacated position; nothing for an in-place destination (`:473-477`).
- `refusal` (`:492`) and `move_event` (`:515`): unchanged from v2.

### 2.2 Why the in-place fix is safe (the reasoning behind `premark.rs:32-46`)

A position the placement names is never reclaimed by GC (`gc.rs:666-673`), so a pre-mark there
evidences nothing — and it outlives the move whenever the move stops after its write (crash, lost
adoption, refused write): the next pass finds the chunk whole and drains it on the healthy path
without reading marks, and a later supersede lets GC reclaim on that stamp. With no pre-mark:

- **Crash or lost adoption after the write:** the position is still named, carries no mark, and
  the drain marks it fresh once something unreferences it. Shown by the four
  `in_place_interrupted` legs, each followed by (D3).
- **A supersede after the pre-mark batch:** the adoption loses on the root pin. The drain marks the
  position at some `d` after the batch's stamp `t`, and GC reclaims no earlier than `d + G`. The
  write was authorized only before `t + W_repoint` and the D server refuses it from `t + W_write`,
  so it lands before `t + W_write < t + G < d + G`: inside the drain mark's grace. This is why the
  batch still commits for an all-in-place move (then it carries only preconditions).
- **A supersede before the batch:** the pins fail, nothing is deleted or written.

I first kept a `require_absent` on the in-place position in the adoption. I removed it: nothing
marks a still-named position without also moving the root or `seg:` record the adoption already
pins (a supersede rewrites the root, a move its record), so no test can make it matter, and an
untested precondition invites a review finding for nothing.

### 2.3 The movers

- `repair_chunk` (`reconstruction.rs:1136`): takes the destinations the assessment chose
  (`CommittedTarget`), then pre-mark (`:1265`), gate (`:1275`), writes with `Some(deadline)` sent
  together (`:1279-1282`), adopt (`:1291`). Aborts are named (`emit_move_aborted`, `:1571`).
- `assess` (`reconstruction.rs:873-915`): the choice, `Blocked` on none. `RepairPlan` lost its
  `survivor_domains` field (`:179-194`): both targets now choose at assessment, so nothing read it.
- `evacuate_chunk` (`rebalance.rs:541`): `premark::choose` over the non-draining pool (`:556-566`),
  then the same sequence (`:682`, `:692`, `:698-699`, `:706`).
- `RebalanceContext` keeps v2's `clock` (`rebalance.rs:112`) and `write_window_millis` (`:121`).

### 2.4 The clock (ADR-0009)

Unchanged from v2: every time read in a committed move — the pre-mark's stamp, the gate, the
vacated mark — comes from the context's clock, which the deployed loop backs with the same
`LoopClock` the pass's `now_millis` is read from (`crates/server/src/custodian.rs:582-587`). The
test doubles judge deadlines on the same `ManualClock` the contexts read; the DST `MemDServer` now
does too.

### 2.5 Docs

`docs/design/architecture/06-runtime-view.md:40` (Reconstruct): the per-position choice and the
in-place rule ("gets no pre-mark … clears whatever stale mark it holds … before any write"); `:41`
(Rebalance) as in v2.

## 3. Choices, and what I ruled out (with costs)

- **In-place: rejected "have the healthy-drain path remove a `repoint:` pre-mark from a named
  position"** (the carry-forward's second suggestion). It does not cover the in-flight window the
  code reviewer reproduced (adoption paused, supersede, GC reclaims at once on the pre-mark's
  stamp), so it fails the invariant whatever its size; it would also cost one mark read per placed
  position of every drained chunk on the drain path. The chosen fix is 14 lines in `premark.rs`
  (`:367-369`, `:417-423`, `:472-477`).
- **Rejected: keep the choice in `repair_chunk` and only reword `assess`'s comment.** About 30
  lines fewer, but a chunk whose only free positions carry unreadable marks would stay
  `Repairable` and `Aborted` on every pass, holding the repairable-backlog gauge above zero for
  good — the iteration-7 "returns to zero" rule this module documents. Moving the choice to
  assessment is the staged peer's shape (`staged.rs:317-327`).
- **`Admission` as a trait, not an enum of rules inside `premark.rs`.** An enum would pull the
  staged fence (`desired:dserver:` read, fleet membership) into the shared module. The trait keeps
  each mover's admission in its own module: `StagedAdmission` 22 lines, `Committed` 38 lines. The
  trait's methods return `impl Future + Send`, which is what keeps the reconstruction future
  `Send` (the v1 closure design failed `cargo clippy --all-targets` on that,
  `tests/multipart_session_terminal.rs:403`); clippy over all custodian targets is clean.
- **DST: a clock-less `MemDServer` panics on a deadline-bearing write** rather than accepting it.
  Accepting would keep the old silent lax judge for any fixture I missed. Cost: three reaper
  fixtures outside this change's movers also had to be wired to their clock (one line each); they
  pass.
- **Not fixed: the same-millisecond identity** (`premark.rs:510-517`). Reason in
  `review-rejected.md`; a fresh random nonce would need an RNG dependency (`Cargo.lock` is out of
  scope) or process-global state (ADR-0035).
- **Not touched: the staged re-place's own in-place pre-mark** (`staged.rs:518-526`, `:605-615`).
  It has the same shape as v2's committed defect, but the brief puts the staged re-place out of
  scope. §10.

## 4. Tests

### 4.1 Where they live, and the rig

Repair legs: `crates/custodian/tests/move_destination_marks.rs` (new; 34 tests). It uses nothing
this patch adds, so it compiles on the base. Evacuation legs: the inline `#[cfg(test)] mod tests`
of `crates/custodian/src/rebalance.rs:965-2525` (28 tests), because they build a
`RebalanceContext`.

Both drive `reconcile_step` over `RedbMetadataStore::in_memory()` behind a wrapper that can hold a
commit before or after it applies or land another writer's puts first, and a per-server D-server
double that records every write and judges the deadline on the test's one `ManualClock` before
storing (`if_elapsed`) and after (`if_publication_unverified`), with optional holds at either
point. Repair contexts are built as the deployed loop builds them (`move_destination_marks.rs:809-832`);
evacuation uses a library-level `Some(RebalanceContext)`. Topology as the brief: servers 2–5 in
domains d2–d5, the RS(2,1) chunk on 3, 4, 5, server 2 free. The swap legs widen it (§0).

| Leg | Repair (`move_destination_marks.rs`) | Evacuation (`rebalance.rs`) |
|---|---|---|
| D1 / D1 in place | `:1058` / `:1077` | `:1953` |
| D2 | `:1128` | `:2001` |
| D3 after D1 / D2 | `:1199` / `:1209` (`supersede_then_reclaim` `:1156`) | `:2072` / `:2082` |
| D4 reclaiming / unreadable | `:1252` / `:1269` | `:2124` / `:2142` |
| D5 | `:1284` | `:2157` |
| D6 | `:1324` | `:2197` |
| D7a / D7b | `:1399` / `:1429` | `:2272` / `:2299` |
| D7c deadline at / before | `:1516` | `:2383` |
| D7d write-unverified | `:1535` | `:2402` |
| D7e gate at / before | `:1567` | `:2434` |
| In place, crash / lost adoption | `:1601` (enum `:1588`) | — (an evacuation never moves in place) |
| Swap | `:1686` | `:2467` |

Every leg runs on a flat and on a segmented object.

### 4.2 Red → green

- **Green, with the fix:** repair 34/34, evacuation 28/28, custodian lib 68/68, staged repair
  30/30; full `cargo xtask ci` green (§7).
- **Red on the base**, done as C4-verify does it (every modified file back to `6e44de9`,
  `premark.rs` moved aside, the new test file kept): **32/34 fail**, each at its own claim — D1
  `…is named by the placement and marked Some(b"1000")` (`:953`); D2 the first move's mark outlived
  it (`:1131`); D3 GC reclaimed before the drain marked; D4 the repair wrote a destination it may
  not write (`:1231`); D5 a placement names the reclaimed position; D6 kept for ever; D7a/D7e no
  pre-mark was committed (`:1411`, `:1560`); D7b the D server stored a write past its pre-mark's
  evidence (`:1441`); D7c/D7d no pre-mark (`:921`); lost-adoption in place: the stale mark stays
  (`:953`); swap: the ruled-out position was written (`:1710`). The two that pass on the base are
  the in-place **crash** legs: the base never marks an in-place position, so they guard v2's
  regression, not the base defect. They are red on v2 and under M9 below.
- **Red on v2's production code** (v2's six production files checked out, the new test file
  kept): 6/34 fail — the four in-place legs and the two swap legs. The other 28 pass on v2: the new
  D7c–D7e legs were test gaps there, not code defects, so their red is the mutations below.
- **Demonstrated reds: mutations of the fixed tree**, each applied alone and run against the repair
  file, the inline evacuation legs and `tests/staged_repair.rs` (built with `--cap-lints=warn` in a
  separate target dir so a mutation's unused binding does not stop the build; script in scratch,
  every file restored after each, tree verified clean at the end):

| Mutation | Repair red | Evac red | Staged red | The legs it turns red |
|---|---|---|---|---|
| M1 adopt keeps the destination mark (brief: "left in place") | 20/34 | 20/28 | 0 | D1, D2 and every later-pass check |
| M2 adopt does not require the pre-mark (brief) | 2 | 2 | 0 | D5 |
| M3 a `reclaiming` destination accepted (brief) | 4 | 4 | 1 | D4 reclaiming, swap |
| M4 no `W_repoint` check (brief) | 4 | 4 | 0 | D7a, D7e |
| M4b gate `<` → `<=` (v2 survivor) | 2 | 2 | 0 | D7e |
| M4c gate widened ×5 | 2 | 2 | 0 | D7e |
| M4d gate narrowed by 1 ms | 2 | 2 | 0 | D7e |
| M5r repair writes with `None` (brief) | 6 | 0 | 0 | D7b, D7c, D7d |
| M5e evacuation copies with `None` (brief) | 0 | 6 | 0 | D7b, D7c, D7d |
| M5f deadline 1 ms late | 4 | 4 | 0 | D7c, D7d |
| M5g deadline 1 ms early | 2 | 2 | 0 | D7c |
| M6 `refusal` → `Ok(None)` (v2 survivor) | 6 | 6 | 0 | D7b, D7c, D7d |
| M6b `write-unverified` reported as expired | 2 | 2 | 0 | D7d |
| M7r repair `emit_move_aborted` silent (v2 survivor) | 10 | 0 | 0 | D7a–D7e |
| M7e evacuation `emit_move_aborted` silent (v2 survivor) | 0 | 10 | 0 | D7a–D7e |
| M8 `move_event` → `"xyzzy"` (v2 survivor) | 6 | 6 | 0 | D7c, D7d, D7e |
| M9 in place pre-marked and consumed, as v2 | 4 | 0 | 0 | in place crash ×2, lost adoption ×2 |
| M10 in-place stale mark not deleted | 4 | 0 | 0 | D1 in place, lost adoption |
| M11 a ruled-out position rules out its whole server, as v2 | 2 | 2 | 1 | swap |

All five v2 C5 survivors (`<`→`<=`, `refusal`→`Ok(None)`, `move_event`→`"xyzzy"`, both
`emit_move_aborted`→`()`) are now killed. The v2 C5 timeout (`premark.rs:166` `==`→`!=` in
`choose`) is in code that is now the shared matcher's completeness check (`premark.rs:243`);
flipping it loops, which cargo-mutants counts as a timeout, not a survivor.

- **DST oracle binds:** the drain-race property over 40 seeds is green; with the `Rewrite` rival
  reverted to v2's semantics (publish the record with no pre-mark), it fails at the
  Rewrite arm's assertion (`custodian.rs:6428-6432`): `the rival's adoption left more than its vacated mark (drained 0, racer
  Rewrite, move first: false)`.

### 4.3 Refuting my own test

- **(a) Genuine red?** Yes. Production reverted: 32/34 repair legs red, each at its own claim; the
  two green ones are v2-regression guards and are red on v2 and under M9. The inline evacuation
  legs cannot compile on the base, so their red is the 19 mutations above, each turning exactly
  its legs red on both shapes.
- **(b) Production path?** Yes. Every leg goes through `wyrd_custodian::reconcile_step` into the
  real `reconstruction::reconcile` / `rebalance::reconcile`, the real `premark` module, the real
  GC and retirement drain, with objects published and superseded by production `complete_upload`.
  Only the stores are doubles: real redb behind a pass-through wrapper, and an in-memory D server
  applying the seam's own `if_elapsed` / `if_publication_unverified`, the calls `FsChunkStore`
  makes.
- **(c) Fixture includes the fault?** Yes. The stale, `reclaiming` and unreadable marks, the
  mid-move GC past grace, the root rewritten before the adoption, the dropped (crashed) pass, the
  aged leftover copy, the write held to its exact deadline or after publishing, and the gate held to
  its exact boundary are all planted or injected in the real store, and each leg asserts the fault
  happened (`reached`, "the racer landed", "the in-place write landed", "GC kept the old copy").

## 5. Existing tests changed, and why

As v2 (unchanged this round): the `RebalanceContext` literals; `placement_ceiling.rs`,
`segmented_map_evacuate.rs`, `segmented_map_rebalance.rs`, `rebalance.rs` criterion 6,
`staged_drain_status.rs`; the Tier-1/Tier-2 clock, window and crash-predicate changes; and
`server/tests/closed_write_path.rs` on the wall clock. New this round: `crates/dst/tests/custodian.rs`
as in §0 (D-server clock, real windows, the drain-race rival and oracle).

## 6. Commit hooks

The repo's formatter is `cargo fmt` and its spell checker `typos`; both ran inside the gate (§7)
and are clean.

## 7. Gates run (on the final tree, `patch.diff` byte-identical to it)

- `PDCA_WORKTREE=<worktree> ./engine/xtask.sh ci`: **exit 0, "xtask ci: all checks passed"** —
  typos; docs lint and render; gitlink, unsafe and blackbox guards; `cargo fmt --check`; workspace
  clippy, build and test; cargo-machete; `cargo fetch --locked`; the three `cargo deny` runs; the
  statics gate (ADR-0035); the deploy-guard (ADR-0010); `cargo clippy -p wyrd-dst` and `cargo test
  -p wyrd-dst` under `--cfg madsim` (custodian DST: 42 passed, 1 ignored, the existing #1015
  ignore). Totals: 235 test binaries, **2667 passed, 0 failed, 16 ignored**.
- `./engine/xtask.sh dst` alone, before that, also green (it first caught the three unwired reaper
  fixtures, §0).
- `engine/scripts/run-verify.sh --classify patch.diff`: `ADDED_TEST
  crates/custodian/tests/move_destination_marks.rs`; crates `chunkstore-grpc`, `custodian`, `dst`,
  `server`. I did not run its full mode: it creates and resets a `../wyrd-verify` worktree off the
  primary checkout, outside the roots I may write; I reproduced its red leg in the cycle worktree
  instead (§4.2). Check runs it as C4-verify.
- **C4 diff coverage could not be measured last round** because `run-diff-cov.sh` applies the patch
  to `origin/main`, while this bundle is stacked on #1013/#1015 (`stack-base`). Nothing in the
  patch can fix that; it is the gate's base choice for a stacked bundle.

## 8. Manual validation (nightly tiers, not run here)

Per the carry-forward these stay nightly or manual. They compile in this tree. Unchanged since v2:

- `crates/custodian/tests/tier1_disk_faults.rs` — `cargo xtask disk-faults` (nightly
  `tier1-disk-faults`; root and device-mapper, the job VM, pdca-harness #969).
- `crates/chunkstore-grpc/tests/tier2_kill_reconstruct.rs` — `cargo xtask kill-reconstruct`
  (docker-compose D servers).
- `crates/chunkstore-grpc/tests/tier1_jepsen_consistency.rs` — `cargo xtask jepsen`
  (docker-compose D servers).

What to check: each scenario's real repair still lands (`Reconciled::Changed`); the
kill-reconstruct crash phase still finds its orphan on the spare server with the victim still in
the committed placement; nothing reports a write refused past its deadline. Their crash trigger
matches a batch that puts an `inode:` key; fine while they use flat objects (adversary, v2).

## 9. Every v2 finding, and its answer

| v2 finding | Answer |
|---|---|
| T4 `premark.rs:162` / `:170` (×3), reviewer F2, adversary #4, code review #2: server excluded for one bad position | Fixed: shared per-position matcher (§0); swap legs both movers |
| T4 `premark.rs:213`, `reconstruction.rs:1279`, reviewer F1, adversary #1, code review #1: in-place pre-mark | Fixed: no in-place pre-mark (§2.2); four interrupted legs + D3 |
| T4 `move_destination_marks.rs:1277`, reviewer F3, adversary #2/#3, code review #3: refusal and gate bounds untested | Fixed: D7c, D7d, D7e on both movers; M4b, M4c, M4d, M5f, M5g, M6, M6b red |
| T4 `dst/custodian.rs:6267`/`:6268` (×3), reviewer F4, adversary #5, code review #4: loose DST oracle | Fixed: exact key, event, stamp; plus "no mark on a live position" in every arm |
| T4 `dst/custodian.rs:6074`, reviewer F4: rival publishes without a pre-mark | Fixed: `RivalMove` keeps the rule; reverting it fails the property |
| T4 `reconstruction.rs:1264`, reviewer F4, code review #5: zero windows, lax `MemDServer` | Fixed: real windows; `MemDServer` judges on the pass clock |
| T4 `premark.rs:301`: same-millisecond identity | Recorded-rejected (`review-rejected.md`) |
| Adversary #6: Tier-1/Tier-2 unverified this round | Manual validation (§8), per the carry-forward |
| C5 survivors (5) | All killed (§4.2) |

## 10. Residuals

- **The staged re-place pre-marks an in-place destination** (`staged.rs:518-526`, and its
  adoption skips only the vacated mark for it, `:611-615`): the same shape as v2's committed defect.
  Out of scope by the brief ("the staged re-place … stays"); `premark::choose` and
  `destination_mark` are already shared, so the follow-up is the staged batch alone. Candidate
  follow-up issue.
- **ADR-0011's counter table** still describes `reconstruction_aborted` as "could not place"
  (`docs/design/adr/0011-durability-telemetry-and-declarative-management.md:34`); the committed
  repair also aborts on a stale pre-mark or a refused write (as the staged re-place already did),
  and a chunk with no usable destination position is now `Blocked`. The brief rules ADR edits out.
- **#825's residuals** (settling an aborted move's pre-marks; a `WriteEffect::Unknown` write
  landing after its pre-mark was reclaimed) apply here as on the staged path (`deferred: #825`,
  `premark.rs:83-88`).
- **Seeded DST for these interleavings** is #682's (`deferred: #682` at `reconstruction.rs:1208`,
  `rebalance.rs:675`).
- **Scratch:** this run left `pdca-builder-1084-scripts/` (mutation and DST target dirs, logs)
  under `$PDCA_SCRATCH`; per the harness rules the harness reclaims it.
