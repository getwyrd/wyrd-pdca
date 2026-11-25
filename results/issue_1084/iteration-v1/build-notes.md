# Build notes — #1084: committed repair and evacuation pre-mark their destinations (0016 X47)

All `path:line` citations are on the cycle worktree (`$PDCA_WORKTREE`), base
`6e44de9` (`pdca-integration/main-r9dd123c9ed70`) plus this patch, unless marked *base*.

## 0. Base check (the brief's stop condition)

Both prerequisites are present on the base this bundle was given, so I built:

- `reconcile_step` runs the retirement drain on the reconstruction input:
  `crates/custodian/src/reconciliation.rs:187` (`retirement::reconcile(ctx)`).
- `wyrd-metadata-redb` is a custodian dev-dependency: `crates/custodian/Cargo.toml:65`.

No dependency or drain was added. `Cargo.lock` and `deny*.toml` are untouched.

## 1. What the defect was, in one paragraph

On the base, `repair_chunk` (*base* `crates/custodian/src/reconstruction.rs:1103`) and
`evacuate_chunk` (*base* `crates/custodian/src/rebalance.rs:516`) picked a destination,
wrote the fragment with `put_fragment(.., None)` (*base* `reconstruction.rs:1229`,
`rebalance.rs:638`), and committed the repoint with a blind legacy mark on each **vacated**
position (*base* `reconstruction.rs:1232-1240`, `rebalance.rs:642-649`). They never read or
touched the mark at the **destination**. So a stale mark survived on a referenced position
(D1/D2), a later unreference that writes no mark (a Complete's supersede) let GC reclaim on
the old stamp before the drain marked it (D3), a `reclaiming` or unreadable destination was
written over (D4), GC could delete the destination mid-move and the commit then published
over missing bytes (D5), a lost commit left an unreferenced, unmarked fragment GC keeps for
ever (*base* `gc.rs:740-742`, D6), and the write had no deadline at all (D7).

## 2. The change

### 2.1 One shared rule: `crates/custodian/src/premark.rs` (new, private module, `lib.rs:27`)

The pieces both committed movers use, mirroring the staged re-place
(`crates/custodian/src/reconstruction/staged.rs:557-576`, `:634-656`, `:680-687`, `:721-751`):

- `destination_mark` (`premark.rs:117`) — reads the destination's mark: absent, re-stampable
  (`Stamped`), or unusable (`Reclaiming`, `Unreadable`). Same classification as staged
  `position` (`staged.rs:557-576`).
- `choose` (`premark.rs:140`) — the selector's pick; a server whose position is unusable is
  named through a callback and dropped from the pool, and the selector picks again. Ends:
  every round drops a server. `None` when no free domain is left → the move aborts.
- `premark` (`premark.rs:191`) — ONE batch: the repoint's own preconditions (root generation,
  and the `seg:` record for a segmented map, copied from `metadata::repoint_chunk`'s prepared
  batch) + each destination's mark as read (`require_absent` / `require(bytes)`) + `put` of a
  fresh structured mark, stamped from the context clock as the batch is built. Event
  identity `repoint:<chunk hex>:<stamp>` (`premark.rs:301`), 0016's per-move nonce; it is not
  a `retire:` token spelling (`crates/core/src/multipart.rs` `parse_retire_key` accepts only
  `s:`/`g:`), so GC's `retirement_draining` never holds it.
- `PreMarked::authorize` (`premark.rs:232`) — the write gate: `Some(stamp + window)` only
  while `now < stamp + W_REPOINT_MILLIS` (`gc.rs:218`). One gate per move.
- `refusal` (`premark.rs:278`) — classifies the writes' answers exactly as staged does
  (`staged.rs:691-708`): `write-unverified` outranks `write-deadline-expired`; any other
  fault propagates.
- `PreMarked::adopt` (`premark.rs:252`) — the repoint batch + `require(orphan:<P_new> ==
  pre-mark)` + `delete` per destination + a legacy mark on each vacated position stamped
  from the context clock (skipped for a rebuild in place). Returns whether it landed.

### 2.2 The movers

- `repair_chunk` (`reconstruction.rs:1112`): `premark::choose` (`:1134`) replaces the bare
  selector; the prepare is unchanged; then pre-mark (`:1254`), gate (`:1264`), writes with
  `Some(deadline)` sent together with `join_all` (`:1270-1271`), adopt (`:1279`). Lost
  pre-mark or adoption → `RepairOutcome::Conflict`; gate or D-server refusal → `Aborted`
  with a `move-aborted` audit line (`:1558`). Destinations passed over are named
  (`emit_destination_passed_over`, `:1523`), reusing the staged path's action strings and
  its `reconstruction_unreadable_destination_marks` counter. The `now_millis` parameter is
  gone: the move reads only `ctx.clock`.
- `evacuate_chunk` (`rebalance.rs:541`): same sequence (`:556`, `:683`, `:693`,
  `:699-700`, `:707`), audit lines at `:865` and `:899`, a `deferred: #682` marker for the
  seeded DST at `:676` (the repair path already carries one, `reconstruction.rs:1199`).
- `RebalanceContext` gains `clock` (`rebalance.rs:112`) and `write_window_millis`
  (`rebalance.rs:121`), documented as `ReconstructionContext::clock` /
  `staged_write_window_millis` are (`reconstruction.rs:135`, `:144`, whose docs now say the
  committed repair reads them too). `rebalance::reconcile` no longer takes `now_millis`
  (`rebalance.rs:170`; caller `reconciliation.rs:213`): nothing in the loop read it once the
  vacated mark moved to the context clock.

### 2.3 Docs

`docs/design/architecture/06-runtime-view.md` steps 2 and 3 (lines 40-41) only, per the
brief: the move pre-marks its destination, writes only while the pre-mark is young and with
a D-server-enforced deadline, and clears the pre-mark in the adopting mutation.

## 3. Choices, and what I ruled out (with costs)

- **Name of the new window field: `write_window_millis`, not `staged_write_window_millis`.**
  The brief says "the same two fields". Rebalance has nothing staged; reusing the name would
  document a falsehood on a public field. The doc on it says it is the same quantity
  (`rebalance.rs:113-121`). If the reviewer wants the identical name it is a one-token
  rename in 18 literals.
- **The pre-mark pins the repoint's preconditions.** A move whose record already moved then
  writes nothing at all (mirrors staged pinning the session and part, `staged.rs:641-652`).
  Cost: four test interceptors that keyed on "the first commit with a positive
  precondition" or "a precondition on `inode:`" now key on "the batch that puts the
  placement record" (§5). Those predicates were already fragile: without the pin, a
  destination carrying a stale mark would make the pre-mark carry a positive precondition
  too.
- **Rejected: one generic `commit_move(.., send: impl FnOnce(u64) -> W)`.** I wrote it first.
  It made the reconstruction future not `Send` for all lifetimes — `cargo clippy -p
  wyrd-custodian --all-targets` failed at `crates/custodian/tests/multipart_session_terminal.rs:403`
  ("implementation of `Send` is not general enough … for `&ReconstructionContext<'_>`"),
  because that test awaits a pass inside an `async_trait` method. The split API
  (pre-mark / authorize / send in the caller / adopt) keeps the fan-out a local
  `map` consumed by `join_all`, exactly the staged shape, and compiles everywhere. Cost of
  the split: ~10 lines of flow repeated in each mover.
- **Rejected: adding `bytes` as a normal custodian dependency** so a struct could hold the
  fragment bytes. Production custodian code never names `Bytes` (staged holds marks as
  `Vec<u8>`, `staged.rs:185`); the split API needs no such field. 0 Cargo.toml lines.
- **Rejected: routing the staged re-place through `premark`.** Out of scope per the brief,
  and staged pins more (session, part, drain keys) and chooses by matching
  (`staged.rs:406-523`); folding it in would rewrite `staged.rs:634-759` (~125 lines) of a
  path the brief says already keeps the rule.
- **`choose` drops a whole server when one position is unusable.** Staged instead swaps
  fragments between servers (Kuhn matching, `staged.rs:468-523`). For a committed move
  that can abort where a swap would have found a placement; it is transient (retried next
  pass) except behind an unreadable mark, which is NEEDS-HUMAN anyway. The brief allows
  "picks another server or aborts".
- **Not added: the drain fence (`require_absent(desired:dserver:<S_new>)`) on the committed
  repair.** 0016's segment-repoint row lists it (`0016:670`), the brief's Scope does not.
  Evacuation already excludes draining servers from its pool. Worth a follow-up issue.

## 4. Tests

### 4.1 Where they live

- Repair legs: `crates/custodian/tests/move_destination_marks.rs` (new). Builds no
  `RebalanceContext`, uses nothing this patch adds: it compiles on the base.
- Evacuation legs: `#[cfg(test)] mod tests` at `crates/custodian/src/rebalance.rs:960`,
  inline (not a sibling `rebalance/tests.rs`) so no classifier mistakes it for an added test
  file that must compile on the base.

Both drive `reconcile_step` over `RedbMetadataStore::in_memory()` behind a wrapper that can
hold a commit before/after it applies or land another writer's puts first
(`move_destination_marks.rs:333-405`), and a per-server chunk-store double that records every
write sent and judges the deadline with `WriteDeadlineExpired::if_elapsed` on the test's
`ManualClock` just before storing (`move_destination_marks.rs:507-531`,
`rebalance.rs:1406-1433`) — the call `FsChunkStore` makes. One `ManualClock` backs every
context and every `now_millis`. Repair contexts are built as `run_reconstruction_until`
builds them (`crates/server/src/custodian.rs:564-580`: clock, `W_WRITE_MILLIS`,
`operator_fleet_size`, `Tombstone::Retain`); evacuation uses a library-level
`Some(RebalanceContext)`. Topology: servers 2–5 in domains `d2`–`d5`; RS(2,1) chunk on
3, 4, 5; server 2 free. The object is published by a production `complete_upload`
(flat: one part; segmented: 2 × 100 chunks, past the 158-chunk flat limit, the extra
199 chunks single-copy on server 3), and superseded in D3 by a second production
Complete. Interleavings use `tokio::select!`/`join!` (`held`, `move_destination_marks.rs:292`)
so a move that skips the held step finishes and fails its leg instead of hanging.

Each leg runs on a flat and a segmented object (`on_both_shapes!`). Repair: 22 tests;
evacuation: 20 (repair has an extra rebuilt-in-place D1 leg).

| Leg | Repair (`move_destination_marks.rs`) | Evacuation (`rebalance.rs`) |
|---|---|---|
| D1 | `:948`, `:964`; in place `:983` | `:1868` |
| D2 | `:1008`, `:1034` | `:1906` |
| D3 (after D1, after D2) | `:1062`, `:1105`, `:1115` | `:1961` |
| D4 reclaiming / unreadable | `:1130`, `:1158`, `:1175` | `:2029` |
| D5 | `:1190` | `:2089` |
| D6 | `:1230` | `:2129` |
| D7a / D7b | `:1263`, `:1305`, `:1331` | `:2162`, `:2204`, `:2227` |

One reading choice: for the repair D7 legs, "no placement names a missing fragment" cannot
hold before the later pass — the placement names server 5's lost fragment, which is the
obligation's own cause. The legs assert the placement is unchanged (it names nothing the move
wrote or GC reclaimed), and then `assert_complete` after the later pass repairs it.

### 4.2 Red → green

Run with `cargo test -p wyrd-custodian --test move_destination_marks` (the exact target
C4-verify runs) and `cargo test -p wyrd-custodian --lib rebalance::tests`, each under
`timeout`.

- **Green, with the fix:** repair 22/22 pass; evacuation 20/20 pass.
- **Red, base** (every modified file `git checkout`-ed to `6e44de9`, `premark.rs` removed, only
  the new test file kept — what C4-verify's red leg does): **22/22 fail**, each at its own
  claim:
  D1 `… on 2 is named by the placement and marked Some(b"1000")` (in place: `… on 5 …`);
  D2 `the first move's mark outlived it …` (`Some(b"10000000")`);
  D3 `GC reclaimed FragmentId {…} on 2|5 before the drain marked it`;
  D4 `the repair wrote to a destination it may not write`;
  D5 `a placement names the position GC reclaimed` (left 2, right 5);
  D6 `the lost move's fragment is kept for ever`;
  D7a `no pre-mark was committed before the write`;
  D7b `the D server stored a write past its pre-mark's evidence`.
  (A first cut of the D3 legs failed inside the D1/D2 setup on the base; I split the moves
  from the D1/D2 assertions so D3's own assertion is the one that goes red.)
- **Demonstrated reds — the brief's mutations**, each applied to the fix alone and run against
  both movers (script kept in scratch; source restored after each, diff stat unchanged):

| Mutation (where) | Repair legs red | Evacuation legs red |
|---|---|---|
| M1 destination mark left in place — adopt drops `.delete(key)` (`premark.rs` `adopt`) | D1 ×2, D1-in-place ×2, D2 ×2 (+ the later-pass checks in D4-reclaiming, D5, D7a, D7b) | D1 ×2, D2 ×2 (+ same later-pass checks) |
| M2 destination mark not required at commit — adopt drops `.require(key, pre-mark)` | D5 ×2 | D5 ×2 |
| M3 `reclaiming` destination accepted (`destination_mark`) | D4-reclaiming ×2 | D4-reclaiming ×2 |
| M4 writes authorized with no `W_REPOINT` check (`authorize`) | D7a ×2 | D7a ×2 |
| M5 writes sent with no deadline (`None` in each mover) | D7b ×2 | D7b ×2 |

### 4.3 Refuting my own test

- **(a) Genuine red?** Yes. With production reverted to the base the new file is 22/22 red
  (above). The inline evacuation legs cannot compile on the base (they build the new
  `RebalanceContext`), so their red is the five mutations, each turning exactly its legs red
  on both shapes.
- **(b) Production path?** Yes. Every leg goes through `wyrd_custodian::reconcile_step` into
  the real `reconstruction::reconcile` / `rebalance::reconcile`, the real `premark` module,
  the real GC (`gc::reconcile`) and the real retirement drain; the object is published and
  superseded by production `complete_upload`. Only the stores are doubles: real redb behind a
  pass-through wrapper, and an in-memory D server that applies the seam's own
  `WriteDeadlineExpired::if_elapsed` rule.
- **(c) Fixture includes the fault?** Yes. The stale mark, the `reclaiming` / unreadable
  destination mark, GC running mid-move with the clock past grace, the root rewritten before
  the commit, and the aged leftover copy are all planted or injected in the real store, and
  each leg asserts the fault actually happened (`reached`/hold assertions, "GC did not reclaim
  the destination in flight", "the racer landed", "GC kept the old copy").

## 5. Existing tests changed, and why

Each change follows from the move now writing a pre-mark first and sending a deadline:

- `RebalanceContext` literals — `clock: &ManualClock::new(<the now the test passes>)`,
  `write_window_millis: W_WRITE_MILLIS`: `tests/{placement_ceiling,rebalance,segmented_map_evacuate,segmented_map_rebalance,staged_drain_status}.rs`,
  `crates/dst/tests/custodian.rs` (the brief's list).
- `tests/placement_ceiling.rs`: its D servers are real `FsChunkStore`s on the wall clock while
  its passes run at 500; a deadline fixed at 500 + window is long past on the wall clock. The
  stores now judge deadlines on the fleet's own `ManualClock` (`:156`), which both contexts read
  (`NOW`, `:65`), and the repair context uses the deployed window (`:300`; window 0 makes the
  deadline equal the stamp, refused at once, inclusive rule).
- `tests/segmented_map_evacuate.rs` and `tests/segmented_map_rebalance.rs`: their store doubles
  asserted "no leg deletes metadata"; the adoption deletes the pre-mark, so the doubles now
  apply deletes (`:170`, `:87`). Leg 4 asserted a lost move publishes no `orphan:` mark — the
  very defect (3); it now asserts the only mark is the move's own pre-mark over the copy and
  the vacated position is unmarked (`segmented_map_evacuate.rs:626`).
- `tests/rebalance.rs` criterion 6: the racer keyed on the first commit with an `inode:`
  precondition, which is now the pre-mark; it now keys on the commit that puts `inode:`
  (`:1266`), and the orphan assertion expects exactly the copy's pre-mark.
- `crates/dst/tests/custodian.rs`: `CrashMeta` and `RaceAtDrainMove` keyed on "first positive
  precondition"; both now use `rewrites_placement` (`:226`, used `:215`, `:6074`). The
  drain-race property's lost-move branch now expects exactly one mark, the copy's pre-mark,
  never the vacated position (`:6267`).
- `crates/chunkstore-grpc/tests/{tier1_jepsen_consistency,tier2_kill_reconstruct}.rs`: same
  `CrashMeta` predicate change (`:246`, `:207`), and their contexts move from
  `ManualClock::new(0)` + window 0 to `SystemClock` + `W_WRITE_MILLIS`, with `now_millis` read
  from the same clock — they write to real gRPC D servers on the wall clock.
- `crates/custodian/tests/tier1_disk_faults.rs`: same clock/window change (`:579`).
- `crates/server/tests/closed_write_path.rs`: found by `cargo xtask ci` — it drove the deployed
  loop with a fixed `|| 500` / `|| 700` over real `FsChunkStore` D servers, so the repair's
  write was refused. It now drives the loop with the wall clock (`:197`), as the deployed role
  does.

## 6. Gates run

- `./engine/xtask.sh ci` (the project's gate, on this worktree): typos, docs lint, docs render
  (`render_site.py --check`, link audit OK), fmt, clippy, build, machete, deny (all three),
  conformance, statics, deploy-guard, DST clippy and DST tests under `--cfg madsim` (custodian
  DST 42 passed, 1 ignored — the pre-existing `#1015` ignore) all **green**. The only failure
  was `closed_write_path`, fixed afterwards (§5) and re-run green.
- After the last edits (the `closed_write_path` fix and doc-only edits in `rebalance.rs`):
  `cargo fmt --all -- --check`, `cargo clippy -p wyrd-custodian -p wyrd-server --all-targets`,
  and `cargo test --workspace --exclude wyrd-dst` — results in §6.1.
- Commit hooks: the repo's formatter is `cargo fmt` (run, clean) and `typos` (run on every
  touched file, clean).

### 6.1 Final workspace test run

`cargo test --workspace --exclude wyrd-dst --no-fail-fast` on the final tree: exit 0, 222 test
targets, **2537 passed, 0 failed**, 14 ignored (the pre-existing `#[ignore]`d tier scenarios).
`cargo fmt --all -- --check` clean; `cargo clippy -p wyrd-custodian -p wyrd-server --all-targets`
clean; `cargo test -p wyrd-server --test closed_write_path` 1/1 green.

## 7. Residuals, honestly

- **A rebuilt-in-place pre-mark after a lost adoption.** When the destination is the
  position the placement already names (repair, `vacated == dserver`), a lost adoption leaves
  the fresh pre-mark on a still-referenced position. GC will not reclaim it while referenced,
  and the next pass's retry re-stamps and consumes it. If the obligation is instead drained
  without a move (the fragment came back) and the object is later superseded, that pre-mark
  could age past grace before the drain marks the position. The staged re-place has the same
  shape (`staged.rs:737-741`). This is "settling an aborted move's pre-marks", #825's.
- **#825's two named residuals** apply here as on the staged path (noted in the module doc,
  `premark.rs` `deferred: #825`).
- **Seeded DST for these interleavings** is #682's (`deferred: #682` at
  `reconstruction.rs:1199`, `rebalance.rs:676`).

## 8. NEEDS-HUMAN

The nightly tier jobs exercise the committed repair against real D servers. I updated four of
their test files (§5) because the new deadline and the pre-mark commit change what they
see, but I could not run them here.

NEEDS-HUMAN external dependency: docker — the updated `crates/chunkstore-grpc/tests/tier2_kill_reconstruct.rs` and `tier1_jepsen_consistency.rs` scenarios (`cargo xtask kill-reconstruct`, `cargo xtask jepsen`, nightly `tier2-kill-reconstruct` / `tier1-jepsen`) need docker-compose D servers; only their compile-time checks and non-ignored helper unit tests ran here.

```toml
[[doctor.checks]]
id    = "docker"
cmd   = "docker info"
hint  = "install Docker and add the PDCA user to the docker group so `cargo xtask kill-reconstruct` / `cargo xtask jepsen` can start their compose fleets"
level = "WARN"
```

NEEDS-HUMAN external dependency: root + device-mapper — the updated `crates/custodian/tests/tier1_disk_faults.rs` (`cargo xtask disk-faults`, nightly `tier1-disk-faults`) needs root and `dmsetup`; it compiled but did not run here.

```toml
[[doctor.checks]]
id    = "root + device-mapper"
cmd   = "sudo -n dmsetup version"
hint  = "run on a host with passwordless sudo and device-mapper (dmsetup) for `cargo xtask disk-faults`"
level = "WARN"
```

Manual validation: run `cargo xtask kill-reconstruct`, `cargo xtask jepsen` and
`cargo xtask disk-faults` on a host that has them (or let the next nightly run), and check
each scenario's repair still lands (`Reconciled::Changed` on the real-repair phase).
