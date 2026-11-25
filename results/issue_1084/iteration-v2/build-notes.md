# Build notes — #1084: committed repair and evacuation pre-mark their destinations (0016 X47)

Iteration 2. All `path:line` citations are on the cycle worktree (`$PDCA_WORKTREE`), base
`6e44de9` (`pdca-integration/main-r9dd123c9ed70`, which carries #1013 and #1015) plus this
patch, unless marked *base*.

## 0. What this iteration changed, and why (the carry-forward)

Iteration 1's Check never ran: it halted on a `root + device-mapper` dependency the builder
declared for the nightly `tier1_disk_faults` scenario. The sign-off said: keep the approach
(`premark.rs` shared by both movers, D1–D7 legs); do not declare `root + device-mapper` or
docker as a blocking dependency; keep the nightly tiers compiling and list them under manual
validation.

So:

- **The patch is byte-identical to iteration 1's** (`cmp` of `git diff` against
  `iteration-v1/patch.diff`: identical). The rejection was about the declaration, not the
  design, and my own review (§3.5) found nothing that needed changing.
- **No external dependency is declared.** The nightly tier scenarios are listed under manual
  validation in §8, and they compile in this tree (`cargo xtask ci` builds every target).
- **All evidence was re-run fresh on this tree:** green, the base red, the five mutation reds,
  and the full `cargo xtask ci` (§5, §7).

## 1. Base check (the brief's stop condition)

Both prerequisites are on the base this bundle was given, so I built:

- `reconcile_step` runs the retirement drain: `crates/custodian/src/reconciliation.rs:187`
  (`retirement::reconcile(ctx)`).
- `wyrd-metadata-redb` is a custodian dev-dependency: `crates/custodian/Cargo.toml:65`.

No dependency or drain was added. `Cargo.lock` and `deny*.toml` are untouched.

## 2. The defect, in one paragraph

On the base, `repair_chunk` (*base* `crates/custodian/src/reconstruction.rs:1103`) and
`evacuate_chunk` (*base* `crates/custodian/src/rebalance.rs:516`) picked a destination, wrote
the fragment with `put_fragment(.., None)` (*base* `reconstruction.rs:1229`, `rebalance.rs:638`),
and committed the repoint with a blind legacy mark on each **vacated** position (*base*
`reconstruction.rs:1232-1240`, `rebalance.rs:642-649`). They never read or touched the mark at
the **destination**. So a stale mark survived on a referenced position (D1/D2); a later
unreference that writes no mark (a Complete's supersede) let GC reclaim on the old stamp before
the drain marked it (D3); a `reclaiming` or unreadable destination was written over (D4); GC could
delete the destination mid-move and the commit then published over missing bytes (D5); a lost
commit left an unreferenced, unmarked fragment GC keeps for ever (*base* `gc.rs:740-742`, D6);
and the write had no deadline at all (D7).

## 3. The change

### 3.1 One shared rule: `crates/custodian/src/premark.rs` (new, private module, `lib.rs:27`)

What both committed movers use, mirroring the staged re-place
(`crates/custodian/src/reconstruction/staged.rs:557-576`, `:634-656`, `:680-708`, `:721-751`):

- `destination_mark` (`premark.rs:117`) — reads the destination's mark: absent, re-stampable
  (`Stamped`), or unusable (`Reclaiming`, `Unreadable`). Same classification as staged
  `position` (`staged.rs:557-576`).
- `choose` (`premark.rs:140`) — the selector's pick; a server whose position is unusable is
  named through a callback and dropped from the pool, and the selector picks again. Every round
  drops a server, so it ends. `None` when no free domain is left, and the move aborts.
- `premark` (`premark.rs:191`) — ONE batch: the repoint's own preconditions (root generation,
  and the `seg:` record for a segmented map, copied from `metadata::repoint_chunk`'s prepared
  batch) + each destination's mark as read (`require_absent` / `require(bytes)`) + a `put` of a
  fresh structured mark stamped from the context clock as the batch is built. Event identity
  `repoint:<chunk hex>:<stamp>` (`premark.rs:301`), 0016's per-move nonce, the same shape as
  staged's `replace:` (`staged.rs:805`).
- `PreMarked::authorize` (`premark.rs:232`) — the write gate: `Some(stamp + window)` only while
  `now < stamp + W_REPOINT_MILLIS` (`gc.rs:218`). One gate per move, as staged (`staged.rs:680-687`).
- `refusal` (`premark.rs:278`) — classifies the writes' answers exactly as staged does
  (`staged.rs:691-708`): `write-unverified` outranks `write-deadline-expired`; any other fault
  propagates.
- `PreMarked::adopt` (`premark.rs:252`) — the repoint batch + `require(orphan:<P_new> ==
  pre-mark)` + `delete` per destination + a legacy mark on each vacated position stamped from the
  context clock (skipped for a rebuild in place). Returns whether it landed.

### 3.2 The movers

- `repair_chunk` (`reconstruction.rs:1112`): `premark::choose` (`:1134`) replaces the bare
  selector; the prepare is unchanged; then pre-mark (`:1254`), gate (`:1264`), writes with
  `Some(deadline)` sent together through `join_all` (`:1270-1271`), adopt (`:1279`). A lost
  pre-mark or adoption is `RepairOutcome::Conflict`; a gate or D-server refusal is `Aborted`
  with a `move-aborted` audit line (`:1558`). Destinations passed over are named
  (`emit_destination_passed_over`, `:1523`) with the staged path's action strings and its
  `reconstruction_unreadable_destination_marks` counter. The `now_millis` parameter is gone:
  the move reads only `ctx.clock`.
- `evacuate_chunk` (`rebalance.rs:541`): the same sequence (`:556`, `:683`, `:693`,
  `:699-700`, `:707`), audit lines at `:865` and `:899`, and a `deferred: #682` marker for the
  seeded DST at `:676` (the repair path already carries one, `reconstruction.rs:1199`).
- `RebalanceContext` gains `clock` (`rebalance.rs:112`) and `write_window_millis`
  (`rebalance.rs:121`), documented as `ReconstructionContext::clock` /
  `staged_write_window_millis` are (`reconstruction.rs:135`, `:144`; their docs now say the
  committed repair reads them too). `rebalance::reconcile` no longer takes `now_millis`
  (`rebalance.rs:170`; caller `reconciliation.rs:213`): nothing in it read that value once the
  vacated mark moved to the context clock.

### 3.3 The clock (ADR-0009)

Every time read in a committed move — the pre-mark's stamp, the `W_repoint` gate, the vacated
mark — comes from the context's clock. In the deployed loop that clock and the pass's
`now_millis` are one `LoopClock` (`crates/server/src/custodian.rs:582-587`), fed the wall clock
by the CLI (`crates/server/src/cli.rs:1109`), which is also the clock the D server judges the
deadline on. The vacated mark's bytes are unchanged: `OrphanMark::legacy(t)` encodes as the
bare decimal the base wrote (`crates/core/src/metadata.rs:285-288`).

### 3.4 Docs

`docs/design/architecture/06-runtime-view.md` steps 2 and 3 (lines 40-41) only, per the brief:
the move pre-marks its destination, writes only while the pre-mark is young and with a
D-server-enforced deadline, and clears the pre-mark in the adopting mutation.

### 3.5 Self-review this iteration (nothing changed, one thing ruled out)

I re-read the production diff against the staged peer and the repo rubric. One candidate
finding, checked and dropped:

- **"No free domain" changed from a pass error to an abort in the committed repair?** On the
  base, `repair_chunk` ran the selector with `?` (*base* `reconstruction.rs:1122-1126`), so a
  refusal failed the pass; `premark::choose` returns `None` instead, and the repair answers
  `Aborted`. I wrote a fix (a `Choice` enum keeping the error for a first-round refusal, ~25
  lines across `premark.rs`, `reconstruction.rs`, `rebalance.rs`) and a test leg for it. The leg
  showed the case cannot happen: planning already runs the same selector, with the same
  topology, count and survivor domains, and diverts such a chunk to `Assessment::Blocked`
  before any repair is dispatched (`reconstruction.rs:871-879`). The test got `Ok(Satisfied)`
  on the fixed tree as on the base. The difference is unreachable, so I reverted both and kept
  iteration 1's code. Evacuation's selector refusal was already `Aborted` on the base
  (*base* `rebalance.rs:528-535`), unchanged.

## 4. Choices, and what I ruled out (with costs)

- **Name of the new window field: `write_window_millis`, not `staged_write_window_millis`.**
  The brief says "the same two fields". Rebalance has nothing staged, so reusing the name would
  put a wrong word on a public field. Its doc says it is the same quantity
  (`rebalance.rs:113-121`). If the reviewer wants the identical name it is a one-token rename
  across 18 struct literals.
- **The pre-mark pins the repoint's preconditions.** A move whose record already moved then
  writes nothing at all (as staged pins the session and part, `staged.rs:641-652`). Cost: four
  test interceptors that keyed on "the first commit with a positive precondition" or "a
  precondition on `inode:`" now key on "the batch that puts the placement record" (§6). Those
  predicates were fragile anyway: without the pin, a destination carrying a stale mark would
  make the pre-mark carry a positive precondition too.
- **Rejected: one generic `commit_move(.., send: impl FnOnce(u64) -> W)`.** Iteration 1 wrote it
  first. It made the reconstruction future not `Send` for all lifetimes: `cargo clippy -p
  wyrd-custodian --all-targets` failed at `crates/custodian/tests/multipart_session_terminal.rs:403`
  ("implementation of `Send` is not general enough … for `&ReconstructionContext<'_>`"), because
  that test awaits a pass inside an `async_trait` method. The split API (pre-mark / authorize /
  send in the caller / adopt) keeps the fan-out a local `map` consumed by `join_all`, exactly the
  staged shape. Cost of the split: about 10 lines of flow repeated in each mover.
- **Rejected: `bytes` as a normal custodian dependency**, so a struct could hold the fragment
  bytes. Production custodian code never names `Bytes` (staged holds marks as `Vec<u8>`,
  `staged.rs:185`); the split API needs no such field. 0 `Cargo.toml` lines instead of 1 plus a
  `Cargo.lock` change the brief rules out.
- **Rejected: routing the staged re-place through `premark`.** Out of scope per the brief, and
  staged pins more (session, part, drain keys) and chooses by matching (`staged.rs:406-523`);
  folding it in would rewrite `staged.rs:634-759` (~125 lines) of a path the brief says already
  keeps the rule.
- **`choose` drops a whole server when one of its positions is unusable.** Staged instead
  matches fragments to servers (Kuhn matching, `staged.rs:468-523`). For a committed move this
  can abort where a matching would have found a placement. It is transient (retried next pass)
  except behind an unreadable mark, which is NEEDS-HUMAN anyway. The brief allows "picks another
  server or aborts".
- **Not added: the drain fence (`require_absent(desired:dserver:<S_new>)`) on the committed
  repair.** 0016's segment-repoint row lists it (`0016:670`); the brief's Scope does not.
  Evacuation already excludes draining servers from its pool. A candidate follow-up issue.

## 5. Tests

### 5.1 Where they live

- Repair legs: `crates/custodian/tests/move_destination_marks.rs` (new). It builds no
  `RebalanceContext` and uses nothing this patch adds, so it compiles on the base.
- Evacuation legs: `#[cfg(test)] mod tests` at `crates/custodian/src/rebalance.rs:959-960`,
  inline (not a sibling `rebalance/tests.rs`) so no classifier takes it for an added test file
  that must compile on the base.

Both drive `reconcile_step` over `RedbMetadataStore::in_memory()` behind a wrapper that can hold
a commit before or after it applies, or land another writer's puts first
(`move_destination_marks.rs:348-426`), and a per-server chunk-store double that records every
write sent and judges the deadline with `WriteDeadlineExpired::if_elapsed` on the test's
`ManualClock` just before storing (`move_destination_marks.rs:506-532`, `rebalance.rs:1405-1431`),
the same call `FsChunkStore` makes. One `ManualClock` backs every context and every
`now_millis`. Repair contexts are built as the deployed loop builds them
(`move_destination_marks.rs:738-761`, after `crates/server/src/custodian.rs:611-630`: the clock,
`W_WRITE_MILLIS`, `operator_fleet_size`, `Tombstone::Retain`); evacuation uses a library-level
`Some(RebalanceContext)`. Topology: servers 2–5 in domains `d2`–`d5`; the RS(2,1) chunk on 3, 4,
5; server 2 free. The object is published by a production `complete_upload` (flat: one part;
segmented: 2 × 100 chunks, past the 158-chunk flat limit, the extra 199 chunks single-copy on
server 3) and superseded in D3 by a second production Complete. Interleavings use
`tokio::select!`/`join!` (`held`, `move_destination_marks.rs:292`) so a move that skips the held
step finishes and fails its leg instead of hanging.

Each leg runs on a flat and a segmented object (`on_both_shapes!`). Repair: 22 tests;
evacuation: 20 (repair has an extra rebuilt-in-place D1 leg).

| Leg | Repair (`move_destination_marks.rs`) | Evacuation (`rebalance.rs`) |
|---|---|---|
| D1 | `:948`, `:964`; in place `:983` | `:1868`, `:1885` |
| D2 | `:1008`, `:1034` | `:1906`, `:1933` |
| D3 (after D1, after D2) | `:1062`, `:1105`, `:1115` | `:1961`, `:2004`, `:2014` |
| D4 reclaiming / unreadable | `:1130`, `:1158`, `:1175` | `:2029`, `:2056`, `:2074` |
| D5 | `:1190` | `:2089` |
| D6 | `:1230` | `:2129` |
| D7a / D7b | `:1305`, `:1331` (setup `:1263`) | `:2204`, `:2227` (setup `:2162`) |

One reading choice: for the repair D7 legs, "no placement names a missing fragment" cannot hold
before the later pass, because the placement still names server 5's lost fragment, which is the
obligation's own cause. The legs assert the placement is unchanged (it names nothing the move
wrote or GC reclaimed), then `assert_complete` after the later pass repairs it.

### 5.2 Red → green (re-run this iteration)

Each run used `cargo test -p wyrd-custodian --test move_destination_marks` (the target
C4-verify runs) or `cargo test -p wyrd-custodian --lib rebalance::tests`, under `timeout`.

- **Green, with the fix:** repair 22/22 pass; evacuation 20/20 pass.
- **Red, base** (`git apply -R` of the whole patch, then only the new test file put back: what
  C4-verify's red leg does): **22/22 fail**, each at its own claim:
  D1 `FragmentId {…} on 2 is named by the placement and marked Some(b"1000")` (in place: `… on 5 …`);
  D2 `the first move's mark outlived it on a position the placement names` (`Some(b"10000000")`);
  D3 `GC reclaimed FragmentId {…} on 2|5 before the drain marked it`;
  D4 `the repair wrote to a destination it may not write`;
  D5 `a placement names the position GC reclaimed` (left 2);
  D6 `the lost move's fragment is kept for ever`;
  D7a `no pre-mark was committed before the write`;
  D7b `the D server stored a write past its pre-mark's evidence`.
  The fix was then re-applied and `git diff` compared equal to the saved patch.
- **Demonstrated reds: the brief's mutations**, each applied to the fix alone and run against
  both movers (script in scratch, sources restored after each; tree verified equal to the patch
  afterwards). The crate builds with `-D warnings`, so each mutation keeps every binding read.

| Mutation (where) | Repair legs red | Evacuation legs red |
|---|---|---|
| M1 destination mark left in place: adopt drops `.delete(key)` (`premark.rs:262`) | D1 ×2, D1-in-place ×2, D2 ×2 (+ the later-pass checks in D4-reclaiming, D5, D7a, D7b): 14 | D1 ×2, D2 ×2 (+ the same later-pass checks): 12 |
| M2 destination mark not required at commit: adopt drops `.require(key, pre-mark)` (`premark.rs:262`) | D5 ×2 | D5 ×2 |
| M3 `reclaiming` destination accepted (`premark.rs:125`) | D4-reclaiming ×2 | D4-reclaiming ×2 |
| M4 writes authorized with no `W_REPOINT` check (`premark.rs:237`) | D7a ×2 | D7a ×2 |
| M5 writes sent with no deadline (`None`, `reconstruction.rs:1270`, `rebalance.rs:699`) | D7b ×2 | D7b ×2 |

### 5.3 Refuting my own test

- **(a) Genuine red?** Yes. With the production change reverted the new file is 22/22 red
  (above), each failing at its own leg's claim. The inline evacuation legs cannot compile on the
  base (they build the new `RebalanceContext`), so their red is the five mutations, each turning
  exactly its legs red on both shapes.
- **(b) Production path?** Yes. Every leg goes through `wyrd_custodian::reconcile_step` into the
  real `reconstruction::reconcile` / `rebalance::reconcile`, the real `premark` module, the real
  GC (`gc::reconcile`) and the real retirement drain; the object is published and superseded by
  production `complete_upload`. Only the stores are doubles: real redb behind a pass-through
  wrapper, and an in-memory D server that applies the seam's own `WriteDeadlineExpired::if_elapsed`
  rule.
- **(c) Fixture includes the fault?** Yes. The stale mark, the `reclaiming` and unreadable
  destination marks, GC running mid-move with the clock past grace, the root rewritten before the
  commit, and the aged leftover copy are all planted or injected in the real store, and each leg
  asserts the fault actually happened (`reached` / hold assertions, "GC did not reclaim the
  destination in flight", "the racer landed", "GC kept the old copy").

## 6. Existing tests changed, and why

Each change follows from the move now committing a pre-mark first and sending a deadline:

- `RebalanceContext` literals gain `clock: &ManualClock::new(<the now the test passes>)` and
  `write_window_millis: W_WRITE_MILLIS`:
  `crates/custodian/tests/{placement_ceiling,rebalance,segmented_map_evacuate,segmented_map_rebalance,staged_drain_status}.rs`
  and `crates/dst/tests/custodian.rs` (the brief's list).
- `tests/placement_ceiling.rs`: its D servers are real `FsChunkStore`s on the wall clock while
  its passes run at 500, so a deadline fixed at 500 + window is long past. The stores now judge
  deadlines on the fleet's own `ManualClock` (`:154`), which both contexts read, and the repair
  context uses the deployed window (window 0 makes the deadline equal the stamp, refused at once
  by the inclusive rule).
- `tests/segmented_map_evacuate.rs` and `tests/segmented_map_rebalance.rs`: their store doubles
  asserted "no leg deletes metadata"; the adoption deletes the pre-mark, so the doubles now apply
  deletes. Leg 4 asserted a lost move publishes no `orphan:` mark, which is the very defect (3);
  it now asserts the only mark is the move's own pre-mark over the copy and the vacated position
  is unmarked.
- `tests/rebalance.rs` criterion 6: the racer keyed on the first commit with an `inode:`
  precondition, which is now the pre-mark; it now keys on the commit that puts `inode:`, and the
  orphan assertion expects exactly the copy's pre-mark.
- `crates/dst/tests/custodian.rs`: `CrashMeta` and `RaceAtDrainMove` keyed on "first positive
  precondition"; both now use `rewrites_placement` (`:226`). The drain-race property's lost-move
  branch now expects exactly one mark, the copy's pre-mark, never on the vacated position.
- `crates/chunkstore-grpc/tests/{tier1_jepsen_consistency,tier2_kill_reconstruct}.rs`: the same
  `CrashMeta` predicate change, and their contexts move from `ManualClock::new(0)` + window 0 to
  `SystemClock` + `W_WRITE_MILLIS`, with `now_millis` read from the same clock, because they
  write to real gRPC D servers on the wall clock. Neither runs a GC pass on another clock.
- `crates/custodian/tests/tier1_disk_faults.rs`: the same clock and window change (`:576-599`).
- `crates/server/tests/closed_write_path.rs`: it drove the deployed loop with a fixed `|| 500` /
  `|| 700` over real `FsChunkStore` D servers, so the repair's deadline was long past on their
  wall clock. It now drives the loop with the wall clock (`:197`), as the deployed CLI does
  (`cli.rs:1109`).

## 7. Gates run (this iteration, on the final tree)

`PDCA_WORKTREE=<this worktree> ./engine/xtask.sh ci` (the project's gate, the C4-ci row), on the
final tree, which is byte-identical to `patch.diff`: **exit 0, "xtask ci: all checks passed"**.
Steps, in order: `typos`; docs lint; docs render (`render_site.py --check`, 101 pages, link
audit OK); gitlink, unsafe and blackbox guards; `cargo fmt --all -- --check`; `cargo clippy
--workspace --exclude wyrd-dst --all-targets`; `cargo build … --all-targets` (this compiles the
three nightly-tier test files in §8); `cargo test --workspace --exclude wyrd-dst`; `cargo-machete`;
`cargo fetch --locked`; the three `cargo deny` runs; the statics gate (ADR-0035); the
deploy-guard (ADR-0010); `cargo clippy -p wyrd-dst` and `cargo test -p wyrd-dst` under
`--cfg madsim` (custodian DST: 42 passed, 1 ignored, the existing #1015 ignore).

Totals over the log: 235 test binaries, **2647 passed, 0 failed**, 16 ignored (the existing
`#[ignore]`d tier scenarios and the #1015 DST ignore). `move_destination_marks` ran inside it:
22 passed.

Commit hooks: the repo's formatter is `cargo fmt` and its spell-checker is `typos`; both ran
inside the gate above and are clean.

An earlier run of the gate was stopped part-way (I had started editing the tree under it for the
change ruled out in §3.5); its result is discarded, and the run above started after the tree was
back to the final state.

C4-verify's classifier, dry-run on the final `patch.diff`
(`engine/scripts/run-verify.sh --classify`): `ADDED_TEST crates/custodian/tests/move_destination_marks.rs`,
crates `chunkstore-grpc`, `custodian`, `dst`, `server`, as the brief expects.

## 8. Manual validation (nightly tiers, not run here)

Per the carry-forward, these are nightly-only tiers: they compile in this tree (`cargo xtask
ci` builds every test target) and their run is left to the nightly tier jobs or the job VM
(pdca-harness #969). I updated them (§6) because the pre-mark commit and the new deadline
change what they see:

- `crates/custodian/tests/tier1_disk_faults.rs` — `cargo xtask disk-faults` (nightly
  `tier1-disk-faults`; needs root and device-mapper).
- `crates/chunkstore-grpc/tests/tier2_kill_reconstruct.rs` — `cargo xtask kill-reconstruct`
  (nightly `tier2-kill-reconstruct`; docker-compose D servers).
- `crates/chunkstore-grpc/tests/tier1_jepsen_consistency.rs` — `cargo xtask jepsen` (nightly
  `tier1-jepsen`; docker-compose D servers).

What to check on the next nightly (or a hand run on a host that has them): each scenario's
real-repair phase still lands (`Reconciled::Changed`), the kill-reconstruct crash phase still
finds its orphan on the spare server with the victim still in the committed placement, and
nothing reports a write refused past its deadline.

## 9. Residuals

- **A rebuilt-in-place pre-mark after a lost adoption.** When the destination is the position
  the placement already names (repair, `vacated == dserver`), a lost adoption leaves the fresh
  pre-mark on a still-referenced position. GC will not reclaim it while referenced, and the next
  pass's retry re-stamps and consumes it. If the obligation is instead drained without a move
  (the fragment came back) and the object is later superseded, that pre-mark could age past grace
  before the drain marks the position. The staged re-place has the same shape
  (`staged.rs:737-741`). This is "settling an aborted move's pre-marks", #825's.
- **#825's two named residuals** apply here as on the staged path (`deferred: #825` in the
  module doc, `premark.rs:62-67`).
- **Seeded DST for these interleavings** is #682's (`deferred: #682` at
  `reconstruction.rs:1199`, `rebalance.rs:676`).
- **ADR-0011's counter table** describes `reconstruction_aborted` as "could not place"
  (`docs/design/adr/0011-durability-telemetry-and-declarative-management.md:34`). The committed
  repair now also aborts on a stale pre-mark or a refused write, as the staged re-place already
  did. The brief rules ADR edits out; a one-line follow-up there would keep the table exact.
