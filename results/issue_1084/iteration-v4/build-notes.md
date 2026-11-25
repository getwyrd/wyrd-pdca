# Build notes — #1084: committed repair and evacuation pre-mark their destinations (0016 X47)

Attempt 4 (re-plan 2026-11-23). All `path:line` citations are on the cycle worktree
(`$PDCA_WORKTREE`, base `6e44de9` = `pdca-integration/main-r9dd123c9ed70`, which carries #1013 and
#1015) with this patch applied, unless marked *base* or *v3* (`iteration-v3/patch.diff`).

## 0. What this attempt changed, against the re-plan

I started from v3's patch (the brief: "reuse what it got right") and changed what the re-plan
asks. Every item:

| Re-plan item | What I did | Where |
|---|---|---|
| 1. In-place repair, option A, #1100 deferred | Kept v3's handling (no mark at a named position, a stale one deleted under its bytes and the pins, the write gated). Added the `deferred: #1100` marker on the code that writes in place, and on the in-place arm of the pre-mark batch; the module doc states the race and why it is deferred. Recorded the iteration-3 T4 finding (`premark.rs:470` then) as deferred in `review-rejected.md`, naming #1100. | `premark.rs:50-56`, `:477-479`; `reconstruction.rs:1317-1319`; `review-rejected.md` |
| 2. Per-move nonce | `move_event` now mints `repoint:<chunk>:<nonce>`, the nonce a fresh `std::hash::RandomState`'s hash (§2.2). The two "same millisecond" rejections are deleted from `review-rejected.md`. D8 on both movers, both shapes; the DST replay test decides the source. | `premark.rs:628-631`; `dst/tests/custodian.rs:6742`, `:6848` |
| 3. Refusals never certify | A repair refused by destination marks is a new `Assessment::Refused`; one stopped after its pre-mark (stale pre-mark, write refused, landing unverified) is a new `RepairOutcome::Stopped(reason)`. Both set `refused`, which joins the verdict's hole: the pass answers `Blocked`. Each is named by its own reason (`repair-refused`/`no-usable-destination`, or `aborted` with the stop's reason), never `unplaced`. | `reconstruction.rs:353`, `:378-382`, `:484-487`, `:536`, `:794`, `:916-930`, `:1136`, `:1315`, `:1324`, `:1612` |
| 4. Test fidelity | Zero-window fixtures moved to `W_WRITE_MILLIS` with doubles that refuse at or after the deadline on the context's clock (five files, §6). The D5 legs release the adoption while the key still holds `reclaiming` (two arms). `segmented_map_repoint.rs`'s `Race::IntoCommit` now fires at the adoption, and its leg asserts the adoption lost after the write. The DST rival pins the root and the `seg:` record. | §6 |
| 5. GC's comments | `gc.rs:255-269` (which writers mark before they write; the one exception, #1100) and `gc.rs:869-879` (the drain-mark sweep's premise, with the in-place write folded in). Doc comments only. | `gc.rs` |
| 6. Vacated mark under a precondition | The adoption reads each vacated position's mark just before it commits and writes under a precondition on it: absent ⇒ `require_absent` + put; a legacy or structured mark of another event ⇒ `require(bytes)` + put; `reclaiming` or unreadable ⇒ `require(bytes)`, nothing written. | `premark.rs:525-563`, `:565-588` |

## 1. Base check (the brief's stop condition)

Both prerequisites are on the base, so I built: `reconcile_step` calls `retirement::reconcile`
(`crates/custodian/src/reconciliation.rs:187`), and `wyrd-metadata-redb` is a custodian
dev-dependency (`crates/custodian/Cargo.toml:65`). No dependency or drain added. `Cargo.lock` and
`deny*.toml` untouched.

## 2. The change

### 2.1 The rule, shared (`crates/custodian/src/premark.rs`, new; `lib.rs:27`)

Unchanged from v3 except as listed in §0: `destination_mark` (`:155`), the shared per-position
choice `choose` (`:222`) used by the staged re-place and both committed movers, `premark`
(`:452`), `PreMarked::authorize` (`:500`, `now < stamp + W_REPOINT`), `PreMarked::adopt`
(`:525`), `refusal` (`:594`). New this attempt: `Committed::new` and `Committed::ruled_out`
(`:354`, `:373`; the flag set where a position is ruled out, `:401`), the vacated-mark read and
`Vacated` (`:532-563`, `:565`), and the nonce (`:628`).

### 2.2 The nonce

`move_event(chunk)` = `repoint:<chunk hex>:<16 hex>`, the hex being
`RandomState::new().build_hasher().finish()` (`premark.rs:628-631`), minted once per move in
`premark()`. Why it meets the brief's three constraints and its uniqueness requirement:

- **No new crate, `Cargo.lock` untouched**: std only.
- **No new field on `ReconstructionContext`** (nor on `RebalanceContext`): called inline.
- **Unique across moves**: std seeds each thread's SipHash keys from the OS once and advances
  `k0` on every `RandomState::new()`, so two moves on one thread get different keys, two on
  different threads independent random keys; a 64-bit hash under distinct keys repeats with
  negligible chance. Not derived from the chunk, the stamp, the root or the `seg:` record.
- **A madsim seed replays**: madsim seeds std's per-thread keys from the run's seed
  (`madsim-0.2.34/src/sim/rand.rs:173-180`, `GlobalRng::new_with_seed` → `init_std_random_state`;
  its `getrandom` override feeds the seed into the first `RandomState` of a fresh thread). I checked
  this empirically first with a throwaway probe (same seed on two fresh threads ⇒ equal; different
  seed ⇒ different), then wrote the brief's replay test (`dst/tests/custodian.rs:6848`), which is
  the binding check (§4.4).
- The event stays inside the mark grammar: 57 visible-ASCII bytes, under `MAX_ORPHAN_EVENT_LEN`
  (256, `crates/core/src/metadata.rs:126`), and `OrphanMark::structured` checks it.

The statics gate (ADR-0035) is unaffected: no `static`/`thread_local!` in this code; std's own
per-thread keys are what madsim seeds, and the replay test shows they replay.

### 2.3 The verdict (reconstruction)

- **Refused at assessment** (`reconstruction.rs:916-930`): `choose` found no assignment *and*
  the admission ruled out a position by its mark ⇒ `Assessment::Refused`; with no position ruled
  out it is the base's capacity `Blocked`, unchanged. `Refused` is counted on the same blocked
  gauge (off the repairable backlog, as `Blocked` is), sets `refused`, and emits
  `repair-refused`/`no-usable-destination` (`:378-382`, `:1612`).
- **Stopped after the pre-mark** (`reconstruction.rs:1315`, `:1324`): `RepairOutcome::Stopped`
  carries `pre-mark-stale`, `write-deadline-expired` or `write-unverified`. The loop sets
  `refused` and calls `emit_aborted(chunk, reason)` (`:484-487`), which keeps the
  `reconstruction_aborted` offset (the success identity) and names the reason. v3's separate
  `move-aborted` line for repair is gone (it said "every fragment it wrote stays under its
  pre-mark", false for an in-place write — the adversary's note).
- `hole` includes `refused` (`:536`) ⇒ `Reconciled::Blocked`, as evacuation already answers.

### 2.4 Docs

`docs/design/architecture/06-runtime-view.md:40` (Reconstruct): adds the vacated mark's
precondition, the pre-mark's per-move identity, and that a refused repair keeps its obligation,
is named, and does not certify. `:41` (Rebalance) as v3. No doc or audit text says an in-place
write sits under a pre-mark: I fixed the two places that implied it (`reconstruction.rs:1534-1538`,
`emit_conflict`; `premark.rs:590-595`, `refusal`).

## 3. Choices, and what I ruled out

- **Nonce through a seam, as `CreateRuntime::mint` takes it** (`crates/core/src/multipart/create.rs:72-76`):
  needs a new field on both contexts; the brief forbids one on `ReconstructionContext` (the new
  test file must compile on the base). Ruled out by constraint.
- **Nonce from `rand`/`getrandom`**: a new direct dependency of `wyrd-custodian` edits its
  `dependencies` list in `Cargo.lock`. Ruled out by constraint.
- **Refusal as a flag on `Blocked` (`Blocked { refused: bool }`) instead of a new
  `Assessment::Refused`**: the flag also has to be set at the staged re-place's `Blocked`
  (`reconstruction/staged.rs:326`), which the brief keeps as is, and it turns the capacity arm
  into a branch. The separate variant (`reconstruction.rs:787-794`, its arm `:378-382`, its
  branch `:925-929`) leaves the capacity arm and the staged code byte-identical to the base.
  Making *every* `Blocked` (capacity) non-certifying is outside the brief's list of refusals; I
  did not change it (§11).
- **`choose` returning a richer result** (which positions were ruled out) instead of the
  admission's flag: that changes the staged caller too (`reconstruction/staged.rs:317-327`), which
  the brief keeps as is. The flag is instance state on the admission (`premark.rs:342-376`).
- **Vacated mark read at assessment, as the staged peer reads it** (`staged.rs:302-318`): would
  pin the mark across the whole write window and make every GC move of that key in between cost
  the move a lost adoption; read just before the adoption it is pinned only across one read and
  one commit. Same precondition rule either way.
- **Kept `reclaiming`/unreadable vacated marks pinned** (`require(bytes)`, not left unpinned):
  if GC finishes and deletes the key between the read and the commit, the adoption conflicts
  and the next pass writes a fresh mark, rather than committing with the position unmarked.

## 4. Tests

### 4.1 Where, and the rig

Repair legs: `crates/custodian/tests/move_destination_marks.rs` (new, 42 tests). It uses nothing
this patch adds, so it compiles on the base (`ADDED_TEST`). Evacuation legs: the inline test
module of `crates/custodian/src/rebalance.rs:964` (34 tests), because they build a
`RebalanceContext`, whose literal this change edits. The nonce's replay test:
`crates/dst/tests/custodian.rs:6848` (`--cfg madsim` only).

Both rigs drive `reconcile_step` over `RedbMetadataStore::in_memory()` behind a wrapper that can
hold a commit before or after it applies, or land another writer's puts first — now with several
hooks armed at once, each consumed by the first commit it matches — and a per-server D-server
double that records every write, judges the deadline on the test's one `ManualClock` before
storing (`if_elapsed`) and after (`if_publication_unverified`), and can hold a write in flight,
after publishing, or a delete after its bytes are gone. Repair contexts as the deployed loop builds
them (`W_WRITE_MILLIS`, the clock the pass's `now_millis` is read from); evacuation with a
library-level `Some(RebalanceContext)`. Topology as the brief: servers 2–5 in d2–d5, the RS(2,1)
chunk on 3, 4, 5; each repair pass whose destination matters gets a topology with one free domain
({2,3,4} → server 2, {3,4,5} → server 5), and the placement is asserted after each such pass
before any mark.

| Leg | Repair (`move_destination_marks.rs`) | Evacuation (`rebalance.rs`) |
|---|---|---|
| D1 / D1 in place | `:1190` / `:1209` | `:2045` |
| D2 | `:1260` | `:2093` |
| D3 after D1 / D2 | `:1319` / `:1329` | `:2164` / `:2174` |
| D4 reclaiming / unreadable / vacated | `:1375` / `:1388` / `:1400` | `:2216` / `:2230` / `:2242` |
| D5 (GC held after intent / after delete) | `:1452` | `:2294` |
| D6 | `:1524` | `:2366` |
| D7a / D7b / D7c / D7d | `:1593` / `:1619` / `:1680` / `:1718` | `:2438` / `:2460` / `:2514` / `:2552` |
| D8 | `:1743` | `:2595` |
| D9 (verdict asserted in D4, D7a–d; plus landing unverified) | `:1345` `refuses`, `:1801` | — (brief: repair only) / `:2573` landing unverified |
| D10a (crash, lost adoption) / D10b | `:1832` / `:1900` | — (repair only) |
| Swap | `:1983` | `:2668` |

Every leg runs on a flat and a segmented object.

### 4.2 Red on the base (C4-verify's red, done as it does it)

Every modified file back to `6e44de9`, the new test file kept (`premark.rs` then not compiled:
the base `lib.rs` has no `mod premark`): **40/42 fail**, each at its own claim — D1/D2 the stale
mark on a named position (`Some(b"1000")`, `Some(b"10000000")`); D3 "GC reclaimed … before the
drain marked it"; D4 "the repair wrote to a destination it may not write"; D4-vacated
`left: b"10000000"` (blind) vs the planted `reclaiming` bytes; D5 "the move adopted a destination
it may not publish" (placement 2); D6 kept for ever; D7a/D7d "no pre-mark was committed before the
write"; D7b "the D server stored a write past its pre-mark's evidence"; D7c/D9 "the move's
pre-mark"; D8 "B never reached its pre-mark commit"; D10a lost-adoption and D10b; swap
`[3,4,2,7]` vs `[3,4,7,2]`. The two that pass on the base are the D10a **crash** legs: the base
never writes a mark at an in-place position, so they guard against that mutation (M11 below), not
the base defect.

### 4.3 Demonstrated reds — mutations of the fixed tree

Each applied alone, both test sets run with `--no-fail-fast` in a separate target dir
(`--cap-lints=warn`), the file restored after each and the tree diffed clean at the end. Script and
JSON results: `$PDCA_SCRATCH/pdca-builder-1084-scripts/mutate_v4.py`, `mutations-v4.json`.

| Mutation (brief's name first) | Repair red | Evac red | Legs |
|---|---|---|---|
| M1 destination mark left in place (adopt keeps it) | 26 | 26 | D1, D2, D4, D5, D7, D8, D9, swap |
| M2 destination's mark not required at the commit | 6 | 6 | D5 (both arms), D8 |
| M3 a `reclaiming` destination accepted | 4 | 4 | D4 reclaiming, swap |
| M4 a vacated mark written blind | 2 | 2 | D4 vacated |
| M5 no `W_REPOINT_MILLIS` check | 4 | 4 | D7a, D7d |
| M6 refusal one ms late (`<` → `<=`) | 2 | 2 | D7d |
| M7r / M7e writes sent with `None` | 8 / 0 | 0 / 6 | D7b, D7c, D9-unverified, D10b / evac D7b, D7c, unverified |
| M8 a refused write treated as landed | 6 | 4 | D7b, D7c (+ D10b repair) |
| M9 identity from chunk and stamp alone (constant nonce) | 2 | 2 | D8 |
| M10a a refusal after the pre-mark answered `Satisfied` | 14 | — | D7a–d, D8, D9, D10b |
| M10b a destination refusal answered `Satisfied` | 4 | — | D4 |
| M11 a pre-mark written at an in-place position (v2's shape) | 4 | — | D10a (crash ×2, lost adoption ×2) |
| M12 an in-place stale mark not deleted | 4 | — | D1 in place, D10a lost adoption |

### 4.4 The nonce's replay test

`a_committed_moves_pre_mark_nonce_replays_under_its_seed` (`dst/tests/custodian.rs:6848`): a plain
`#[test]` running seed `0x1084` twice, each on a fresh `std::thread` with its own
`Runtime::with_seed_and_config`. Each run makes two committed repairs onto fresh destinations
through `reconcile_step` at one clock instant (500), and returns the pre-marks a tap captured from
the committed batches (`PremarkTap`, `:6742`, instance state only), the verdicts and the final
placement. It asserts both runs equal, and the two moves' identities different within a run.
`install_dst_barrier()` is called first, as the macros do. Results (madsim build, scratch target):

- `RandomState` (the fix): **pass**.
- Process-global counter nonce: **red** — "a madsim seed no longer replays" (`…:0000` vs `…:0001`).
- Wall-clock nonce (`SystemTime::now()` nanos): madsim virtualises `clock_gettime`
  (`madsim-0.2.34/src/sim/time/system_time.rs:40-44`), so the wall clock **replays**; my first
  one-move version of the test passed it. I extended the run to two moves in one instant, and it is
  now **red** — "two moves pre-marked in one instant carry the same identity". (It is also rejected
  by `clippy.toml`'s `disallowed-methods` on `SystemTime::now`, at deny.)

### 4.5 Refuting my own test

- **(a) Genuine red?** Yes. Base production with the new file kept: 40/42 red at their own
  claims (§4.2); the 2 green ones are red under M11. The inline evacuation legs cannot compile on
  the base; every one of the brief's named mutations turns its legs red on both shapes (§4.3). The
  replay test is red on the counter and wall-clock sources (§4.4).
- **(b) Production path?** Yes. Every leg goes through `wyrd_custodian::reconcile_step` into the
  real `reconstruction::reconcile` / `rebalance::reconcile`, the real `premark` module, real GC
  and the real retirement drain; objects are published and superseded by the production
  `complete_upload`. Only the stores are doubles: real redb behind a pass-through wrapper, and an
  in-memory D server applying the trait's own `if_elapsed` / `if_publication_unverified`, the calls
  `FsChunkStore` makes (`crates/chunkstore-fs/src/lib.rs:286-288`).
- **(c) Fixture includes the fault?** Yes. The stale, `reclaiming` and unreadable marks (at the
  destination and at the vacated position), GC's intent and delete mid-move (held there), the root
  rewritten before the adoption, the dropped pass, the aged leftover copy, the second move
  re-creating the key, the supersede between pre-mark and write, and the exact deadline/gate
  instants are planted or injected in the real store, and each leg asserts the fault happened
  (`reached`, "the racer landed", "the key does not hold `reclaiming`…", "the two pre-marks were
  stamped in one millisecond", "the in-place write landed").

## 5. One thing the D10b leg taught me

D10b's "late" arm holds the in-place write past the drain mark's grace and lets GC sweep the
fragment-less drain mark first; the D server then refuses the write. On the base (no deadline) the
write lands unreferenced and unmarked, so that arm is the leg's base red; the "before its deadline"
arm is the brief's property (reclaimed only after the drain mark's grace).

## 6. Existing tests changed, and why

- `crates/custodian/tests/reconstruction.rs`: `MemDServer` judges deadlines on a `ManualClock`
  (`MemDServer::on`, `:126`); each test makes one `pass_clock` that its servers and its contexts
  share (second passes `set(600)` on it); every window `W_WRITE_MILLIS` (15 literals). Check: with
  the window back at 0 and the enforcing double, 11/15 go red (the adversary's count), all pass at
  `W_WRITE_MILLIS`.
- `segmented_map_repoint.rs`: the double judges deadlines on its own clock, which the context
  reads (`clock: &free.1.clock`, `:485`); window `W_WRITE_MILLIS`. `Race::IntoCommit` fires only on
  a commit that rewrites a placement record — the adoption (`:146-152`) — and leg 4 now asserts the
  fragment was written, sits under its pre-mark alone, and the adoption lost.
- `segmented_map_reconstruction.rs` (`clock: &d2.clock`, `:452`) and `staged_protection.rs` (four
  disks share one clock, `disks()` `:457`; the pass reads `d[0].clock`, `:569`): same treatment.
- `crates/server/tests/custodian_day_one.rs`: `MemDServer::on(&pass_clock)` (`:149`), 7 windows
  `W_WRITE_MILLIS`, and the run-loop clocks — the three fixed `|| 500` and the two advancing ones —
  read the same `pass_clock` (`loop_clock`, `:585`).
- `crates/dst/tests/custodian.rs`: the rival pins the root as well as the `seg:` record
  (`:6158`, `:6176`, built `:6323`); the lost-move branch checks the move's own `repoint:<chunk>:`
  identity instead of v3's `repoint:<chunk>:<stamp>`.
- Carried from v3 unchanged: `RebalanceContext` literals in `placement_ceiling.rs`,
  `rebalance.rs`, `segmented_map_evacuate.rs`, `segmented_map_rebalance.rs`,
  `staged_drain_status.rs`; the Tier-1/Tier-2 files; `server/tests/closed_write_path.rs`; the DST
  fixtures' clocked `MemDServer`.

## 7. Commit hooks

The repo's formatter is `cargo fmt` and its spell checker `typos`; both ran inside the gate (§8).
I also ran `cargo fmt --all` after every edit.

## 8. Gates run (on the final tree; `patch.diff` is `git diff 6e44de9` of it)

- `PDCA_WORKTREE=<worktree> ./engine/xtask.sh ci` (the project's own gate): **exit 0, "xtask
  ci: all checks passed"** — `typos` (clean), docs lint and render with link audit, gitlink /
  unsafe / blackbox guards, `cargo fmt --check`, workspace clippy, build and test, cargo-machete,
  `cargo fetch --locked`, the three `cargo deny` runs, the statics gate (ADR-0035: "no
  DST-reachable shared mutable global state"), the deploy-guard, and `cargo clippy` / `cargo test
  -p wyrd-dst` under `--cfg madsim` with the 50-seed sweep (custodian campaign 43 passed, 1
  ignored — #1015's known gap; the new replay test and the drain-race property among the passes).
  Totals over the run: 235 test binaries, **2682 passed, 0 failed, 16 ignored**. Log:
  `$PDCA_SCRATCH/pdca-builder-1084-scripts/ci-v4.log`.
- After the gate I made one doc-only edit: three public doc comments linked the private
  `crate::premark` module (`rebalance.rs:99`, `reconstruction.rs:30`, `:123`), which `cargo doc`
  reports as errors; they are plain code text now. (`cargo doc -p wyrd-custodian` already fails on
  the base for other private links, e.g. `gc.rs:23`, so it is not a gate; the patch now adds no
  error of its own.) Re-checked after it: `cargo fmt --check`, `typos` on the touched files,
  `cargo clippy -p wyrd-custodian --all-targets`, and the repair + inline evacuation legs (42 + 74
  lib tests) — all clean. `patch.diff` is `git diff 6e44de9` of that final tree (24 files; `git
  apply --check -R` against the tree passes).
- `engine/scripts/run-verify.sh --classify patch.diff`: `ADDED_TEST
  crates/custodian/tests/move_destination_marks.rs`; crates `chunkstore-grpc`, `custodian`,
  `dst`, `server` — as the brief's dry run. I did not run its full mode: it builds a
  `../wyrd-verify` worktree off the primary checkout, outside the roots I may write; §4.2 is the
  same red leg done in the cycle worktree.
- C4 diff coverage applies the patch to `origin/main`, while this bundle is stacked on #1013/#1015
  (`stack-base`); last round it could not measure for that reason, and nothing in the patch can
  change it.

## 9. Manual validation (nightly tiers, not run here)

Not declared as dependencies (pdca-harness #969). They compile in this tree (the gate builds every
target):

- `crates/custodian/tests/tier1_disk_faults.rs` — `cargo xtask disk-faults` (root + device-mapper).
- `crates/chunkstore-grpc/tests/tier2_kill_reconstruct.rs` — `cargo xtask kill-reconstruct`.
- `crates/chunkstore-grpc/tests/tier1_jepsen_consistency.rs` — `cargo xtask jepsen`.

What to check: each scenario's repair still lands (`Reconciled::Changed`); kill-reconstruct still
finds its orphan on the spare with the victim in the committed placement; nothing reports a write
refused past its deadline, and no `repair-refused` line appears in a healthy run.

## 10. Iteration-3 findings and their answers

| Finding | Answer |
|---|---|
| Reviewer F1, T4 `premark.rs:515`/`:516`, code review #1: reused identity | Fixed: per-move nonce; D8 both movers/shapes; replay test |
| Reviewer F2, T4 `premark.rs:470`: delayed vacate vs in-place write | Deferred to #1100 (re-plan option A); recorded in `review-rejected.md`; `deferred: #1100` in code; GC doc names it |
| Reviewer F3, T4 `reconstruction.rs:900`/`:1284`, adversary #2, code review #2: refusals certify | Fixed: `Refused`/`Stopped` ⇒ `Blocked`, own reasons; D4/D7/D9 assert the verdict |
| T4 `premark.rs:481`: vacated mark blind | Fixed: precondition per the read; D4-vacated both movers/shapes |
| T4 `premark.rs:405`, `reconstruction.rs:1266`: race hook at the pre-mark | Fixed: `IntoCommit` at the adoption; leg 4 asserts after-write loss |
| T4 `dst/custodian.rs:6153`: rival pins the segment only | Fixed: root + `seg:` |
| T4 `move_destination_marks.rs:1284`, code review #3: D5 released after cleanup | Fixed: two arms, released while the key holds `reclaiming` |
| Adversary #1: zero-window fixtures | Fixed (§6) |
| Adversary #3: GC comments and `reconstruction.rs:1577` audit text | Fixed: `gc.rs:255-269`, `:869-879`; repair's `move-aborted` line removed |

## 11. Residuals

- **#1100** (deferred, above).
- **#825**: settling an aborted move's pre-marks; a write answered `WriteEffect::Unknown`. B's
  pre-mark in D8 is such a leftover (a structured mark with nothing beneath, kept by the sweep's
  `event-may-await-write`); the next pass re-stamps it.
- **A capacity-blocked repair (`Blocked`, no free domain) and an `unplaced` abort still answer
  `Satisfied`**, as on the base. They are outside the brief's list of refusals; the invariant's
  last clause would cover them too. Candidate follow-up issue.
- **An unreadable vacated mark is not named by the mover.** The adoption leaves it byte for
  byte and pinned (`premark.rs:556`); GC's ledger walk names every unreadable `orphan:` mark it
  reads as NEEDS-HUMAN (`unreadable-orphan-mark`, `gc.rs:2389-2393`), so it reaches a human
  without a second emitter. The staged peer instead withholds its repair on one
  (`staged.rs:302-318`); the brief asks the committed movers to leave it and move on.
- **The staged re-place pre-marks an in-place destination** (`staged.rs` batch), the shape v2 had
  on the committed path; out of scope ("stays as it is").
- **ADR-0011's counter table** still reads `reconstruction_aborted` as "could not place"; ADR
  edits are out of scope.
- **Seeded DST for these interleavings** is #682's (`deferred: #682` markers kept).

## 12. Scratch

Under `$PDCA_SCRATCH`: `pdca-builder-1084-scripts/` (v3's leftovers plus this attempt's
`mutate_v4.py`, `mutations-v4.json`, `mutate-v4.log`, `ci-v4.log`, the `target-mut` and
`target-dstmut` build dirs). The harness reclaims it.
