# custodian: every non-Open multipart session has an exit — Completing rollback, tombstone expiry, G_orphan, B_ops calibration (625.3)

> Third of three slices #625 (multipart reaper) was split into on 2026-10-30. It builds on
> 625.1 (child-1), which lands the reaper pass, its clock guard, its profile check, the
> operator one-shot and `ReaperPolicy` (with #659's tombstone input routed through it, still
> "retain"). Do reads ONLY this file. Design: 0016 decision 6 (`0016:1794-2277`); the
> `Completing` exits (`0016:1918-1930`); the three-arm mark rule (`0016:1127-1224`); the
> clock-lifecycle table (`0016:2497-2509`); the per-backend budget (`0016:621-648`,
> `:2907-2910`). Teardown mechanics are **#659's** (the drain, the three-arm guard, the
> terminal delete). This slice supplies the judgments that feed them.

- **Slug:** multipart-reaper-terminal-exits
- **Track:** alpha
- **Kind:** enhancement (design proposal)
- **Defect:** After 625.1, two states still have no exit when no client verb arrives.
  - **`Completing`:** a gateway that died holding the fence leaves the session `Completing`
    forever. It is never rolled back at `W_completing`, and when over-age it is never moved to
    `Aborting`.
  - **`Completed`:** #659's drain retains the tombstone, because its deployed tombstone input
    says "retain" until #625 supplies the window. The record and its `mpuctl` slot are
    therefore held forever.

  Two more gaps fall to this slice:
  - The grace window `G_orphan` is not yet bound to the strict inequality 0016 assigns to #625.
  - `B_ops` rests on a per-operation latency "assumed, not measured" (`SLOWEST_OP_MILLIS`,
    `crates/core/src/multipart.rs:5082-5105`), whose replacement the source assigns to #625.
- **Success criterion:** a NEW test file `crates/server/tests/multipart_reaper_exits.rs`
  drives the reaper through 625.1's one-shot as a subprocess (the same mechanics; the seeding
  helpers are **copied** into this file, not shared, per `run-verify.sh:141-144`). Every leg
  asserts on durable store state.
  **(D) `Completing` is exited, both ways.**
  - A `Completing` session whose `fenced_at_millis` is older than `W_completing`, and which is
    **not** over-age, rolls back to `Open@E+1` and installs `retire:records:{seg:<g>:<E>}` for
    that attempt's segments.
  - A `Completing` session that **is** over-age (`now - created_at > W_session`) goes
    **straight** to `Aborting@E+1`, in one batch that also installs that obligation. It is
    never rolled back first (`0016:1918-1930`).
  - Assert that the named epoch's `seg:` range drains empty and that a **later** epoch's
    segments are untouched.

  **(E-terminal) The clock guard on the terminal states.**
  - A foreign-clocked `Completing` session whose `fenced_at_millis` is far past `W_completing`
    is **not** rolled back: its state, epoch, `seg:` range and `retire:` records are all
    unchanged.
  - A foreign-clocked `Completed` session keeps its record, because its tombstone window
    cannot be judged. Assert that the record **survives**. Its only exit is #633's
    terminal-expiry verb.

  **(G) Tombstone expiry.** A local-clock `Completed` session whose `completed_at_millis` is
  older than `W_tombstone`, with no session `retire:` obligation left and an empty `sidx:`
  range, has its record deleted and `mpuctl.count` decremented by exactly one. A `Completed`
  session inside `W_tombstone` keeps its record, and an identical Complete retry still answers
  `AlreadyCompleted` (`multipart.rs:4789-4795`).

  **(H-terminal)** With a mismatched `mpuctl.profile`, a stale `Completing` session is not
  rolled back either (625.1's check guards every arm).

  **(F) Teardown over the composed reaper path is idempotent and waits for GC's decision.**
  The `orphan:` mark guard (arms (0), (a), (b), (c), and fail-closed on undecodable values) is
  **#659 child-1's** code and tested there. This slice does not implement, copy or re-test it
  arm by arm. It asserts only what the reaper's path adds on top:
  - Run the one-shot twice over a partially drained obligation of a session the reaper
    fenced. It converges to the same terminal state with **no double-decrement**.
  - **The reaper's teardown waits for the obligation.** In an `Aborting` session's obligation,
    one position carries a `reclaiming` mark. After the one-shot, the `mpu:` record is
    present, the count is unchanged, the mark is byte-identical, and the obligation is still
    there. The test then deletes that key, as GC's blind delete would (`gc.rs:804-824`), and
    runs the one-shot again. The position gets a fresh mark under `require_absent`, the
    obligation and the session are gone, and the count drops by exactly one.

  **`G_orphan` boundary** (`cargo xtask ci` evidence, appended to the existing
  `crates/custodian/tests/gc.rs`): pin the tick at which a mark written by a fence becomes
  reclaimable, and show that `G_orphan > W_write + δ_clock` holds **strictly**. GC's grace check
  is inclusive (`gc.rs:172-176`).
- **Falsifiability:** on 625.1's folded base, the `Completing` session is untouched, so D fails.
  The `Completed` record survives past `W_tombstone`, so G fails. H-terminal is vacuous. The
  E-terminal fixtures survive on the base too, so they are vacuous; evidence them through the
  negations below. **F is green on the base if #659's drain is correct.** It is regression
  coverage of #659's mechanism through the reaper path: say so, and evidence it through
  negations (8) and (9). If the obligation-wait fixture reds on the base, #659 child-1's arm (0)
  is missing or broken. That is a §6 item against #659, never a fix here. Do records the base commit, confirms 625.1 and #659's drain are on it, and
  records the RED leg's executed and failed test counts.
- **Invariant to restore:** for every state in 0016's exit table (`0016:569-579`) there is an
  actor that drives it out, and no durable byte is left with nothing that will reclaim it.
  After this slice that holds for `Completing` and `Completed` as well as `Open`. C-1 applies.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** 1015
- **Conflicts with:**
- **Ordering note:** wave 2 of #625's split, in parallel with child-2 by file partition. This
  slice owns:
  - the reaper module and `gc.rs` under `crates/custodian`;
  - `crates/core/src/multipart.rs`, for the `pub` budget constants and `SLOWEST_OP_MILLIS`'s
    doc comment only;
  - `crates/metadata-conformance`, `crates/metadata-fdb/tests/conformance.rs` and
    `crates/metadata-tikv/tests/conformance.rs`, the dev-dependencies, and the mechanical
    `Cargo.lock` change;
  - `crates/dst/tests/custodian.rs`, `crates/custodian/tests/gc.rs` and its new test file;
  - `docs/design/architecture/08-crosscutting-concepts.md`.

  It MUST NOT edit `crates/server/src/*`, `crates/server/tests/custodian_gc.rs` or
  `06-runtime-view.md`, which child-2 owns. Add the windows to `ReaperPolicy::default()` and
  flip the tombstone input inside `crates/custodian`. If a server file must change, raise a §6
  item. #633 must be re-pointed to this slice and to child-2.
  **Needs #659 child-1 (`multipart-retirement-drain-step`) on its base**, for arm (0) of the
  mark guard that F's obligation-wait fixture exercises. Usually child-1's re-pointed `659`
  edge carries it transitively, because #659's child-2 depends on #659's child-1. If it does
  not, add #659 child-1's id to this slice's `Depends on` after acceptance; the ordering fields
  here can name siblings only.
- **Surfaces:** data
- **Difficulty:** high
- **External dependencies:** `typos`, `docs-renderer`, `docker`, `libfdb_c loadable`, `fdb headers (bindgen)`, `fdb cluster healthy`, `openssl dev (pkg-config + libssl)`, `cmake (tikv-client build dep)`
  The binding criterion needs only the base toolchain: every leg runs in-process over redb.
  The six backend tokens are for the **off-Check calibration leg** only. The first four are the
  full FoundationDB conformance set, and the last two, with `docker`, build the `tikv`
  feature.
- **Test file:** `crates/server/tests/multipart_reaper_exits.rs`, a **NEW** file, not
  `#![cfg]`-gated. DST rows go into the EXISTING `crates/dst/tests/custodian.rs` (no new file
  under `crates/dst/tests/`). The calibration is appended to the existing backend
  `conformance.rs` files, not added as a new file, because an added `tests/*.rs` would join
  C4-verify's invocation.
- **Verification posture:** red→green at Check for D and G. **Negations** recorded in
  `build-notes.md`:
  - (4) walk `sidx:` for `Aborting` only, not `Completed` ⇒ G's residue fixture is stranded;
  - (5) fire the terminal delete before `sidx:` is observed empty ⇒ G;
  - (6′) skip the clock guard for `Completing` ⇒ E-terminal;
  - (8) against #659's code, on a scratch tree only: treat `reclaiming` as a different-event
    mark ⇒ F's obligation-wait fixture;
  - (9) delete and decrement while a `retire:` obligation is present ⇒ F;
  - (15) roll back an over-age `Completing` session before aborting it ⇒ D.

  **Seeded DST, appended to `crates/dst/tests/custodian.rs`:**
  - the **Complete/reap race in both interleavings**;
  - a **crash at each step of the reaper's own transitions** (the `Completing` rollback and the
    over-age edge, each with its `retire:records:` install), converging with the count at its
    true value. This row is kept only if #659 child-1's crash row does not already cover these
    transitions; if it does, cite it.

  **Not here (decided 2026-10-30):**
  - the drain-vs-GC `reclaiming` race and the crash at each drain step are #659 child-1's;
  - X56 (two concurrent drainers) is #660's;
  - the reaper writes no `orphan:` mark itself, since its fences only install obligations that
    #659's drain pays out, so it owes no reaper-vs-GC race row.

  If Do finds the reaper does write a mark, that write gets the race row here.

  **Off-Check: the `B_ops` calibration.**
  - **Fixture.** Append one `#[test]` per networked backend to the existing
    `crates/metadata-fdb/tests/conformance.rs` and `crates/metadata-tikv/tests/conformance.rs`,
    gated exactly as the trait-contract test beside it. The batch builder and the timing loop
    are one `pub` function in `crates/metadata-conformance/src/lib.rs`, outside `run_all`.
    The budget values come from `wyrd-core` (`MAX_BATCH_OPS`, `MAX_BATCH_BYTES` and
    `SLOWEST_OP_MILLIS` made `pub`, with values unchanged), added as a dev-dependency of both
    crates and never copied as literals.
  - **Two worst-case batches.**
    - (i) Operation-bound: `MAX_BATCH_OPS` `require_absent` puts on distinct keys, with small
      values.
    - (ii) Byte-bound: the same count with `MAX_BATCH_BYTES` of puts in total, with the
      preconditions split between `require_absent` and exact-value guards on pre-seeded keys.
  - **Assertion.** Each batch commits, and its slowest wall time over at least five runs is
    at most `MAX_BATCH_OPS × SLOWEST_OP_MILLIS` (2,500 ms at `4bac538`). Every measurement is
    printed.
  - **Running it.** The wiring already exists (`conformance` is in both xtask leg lists,
    `xtask/src/main.rs:574`, `:297`). Record the **first** attempt's figures, because TiKV's
    runner retries (`:304-344`).
  - **If the slowest batch is within the bound,** replace only the "assumed, not measured"
    wording with the measured figures (backend, version, host, date).
  - **If it is over the bound,** change no constant and raise a §6 item. The re-valuing is
    #655's value set and the human's call.
  - **Who runs it.** Do runs `cargo xtask fdb-conformance` and `cargo xtask tikv-conformance`
    where the host allows. Otherwise Eduard Ralph runs them before merge and records the
    figures in SUMMARY §9. Check only compiles the TiKV test (`host-tikv`).
- **Production reach:** the new arms live inside 625.1's pass, which child-2 runs on every
  deployed shape, and inside the same `reconcile_step` dispatch as the one-shot that the test
  drives. No new entry point. Do records the call path.
- **Scope:**
  - the `Completing` rollback at `W_completing` and the over-age `Completing → Aborting` edge,
    both installing `retire:records:{seg:<g>:<E>}`, through the existing fenced transitions;
  - the clock guard applied to `W_completing` and `W_tombstone`;
  - the `W_tombstone` judgment, which replaces "retain" as #659's tombstone input via
    `ReaperPolicy`;
  - the knob values `W_completing`, `W_tombstone` and **`G_orphan`**: each inside 0016's range
    and cited at its definition, with `G_orphan > W_write + δ_clock` and, where a repoint can
    intervene, `G_orphan > W_repoint + W_write + δ_clock` (`0016:1478-1479`, `:2497-2509`);
  - the calibration experiment;
  - docs in `08-crosscutting-concepts.md`.

  **Not this slice's to choose:** `W_ref`, `B_bytes`, `B_ops`, the profile tuple and
  `MAX_SESSIONS` are #655's single value set, consumed unchanged.

  **Out of scope:**
  - the `Open` arms (625.1) and the deployed wiring (625.2);
  - the operator abort and terminal-expiry verbs and the foreign-clock alarm (#633);
  - the `orphan:` mark guard, including arm (0) (`reclaiming`), and its DST rows (#659
    child-1), and X56 (#660);
  - #806, #630, FU-2 and FU-4;
  - ADRs, specs, and any edit to 0016;
  - any change to the records or state machine of the slices below.
- **Repro instruction:** on 625.1's base:
  1. Seed a local-clock `Completing@E` session with `fenced_at_millis` past `W_completing`
     and `created_at` inside `W_session`, its `seg:<g>:<E>:` records, and `mpuctl`.
  2. Seed a `Completed` session with `completed_at_millis` past `W_tombstone`, no obligation
     and empty `sidx:`.
  3. Drop the store, run 625.1's one-shot as a bounded subprocess, and reopen the store.

  Both sessions are exactly as seeded.
- **Citations expected:** `path:line` for every change. Mirror:
  - the fenced transitions on the base (#693's state machine, #895's fence batch builder);
  - #659's drain context and tombstone input;
  - GC's grace check (`gc.rs:172-176`) and blind delete (`:804-824`);
  - the `reclaiming` rule (`metadata.rs:109-140`);
  - the restore pass's same-event skip (`restore.rs:278-286`, for arm (a) only;
    `mark_orphaned`, `gc.rs:110-122`, is a precedent for neither arm);
  - the budget constants (`multipart.rs:5053`, `:5067`, `:5082-5106`).
- **Prior-art check (triage cycles):** no reaper exists before 625.1, and #659 lands the drain
  and the tombstone input this slice drives. Re-run by path (`crates/custodian/src/`,
  `crates/metadata-conformance/src/lib.rs`) on your base.
- **Disposition hint:** likely-fix

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Keep the slice whole: production logic is ~9 KB (the 148 KB is tests, comments and diff context), so this is not a split. C4-ci is red because tombstone expiry ages out the fixture at `crates/server/tests/custodian_gc.rs:1470`/`:1531` (passes on base, fails with the patch). NARROW SCOPE EXCEPTION (granted at sign-off): this slice MAY edit, in child-2's (#1016) files, ONLY (1) the tombstone timestamp/age in `custodian_gc.rs`'s deployed-retirement test so the completed upload is inside the window when asserted — keep every partial-fleet safety assertion, and keep the change base-neutral — and (2) the tombstone wording in `06-runtime-view.md` (and the matching stale "retained tombstone" text in `05-building-block-view.md`) so the living docs agree with `08-crosscutting-concepts.md`. Nothing else in `crates/server/*`. Expect a small merge conflict with #1016; whichever lands second resolves it. Also justify the `core/multipart.rs` transition API beyond constants/doc in build-notes.md, or move it into the custodian. Fix in place: - Clock identity: bind the tombstone judgment to `ReaperContext::clock_source`, not the hard-coded `wall` in `ReaperPolicy::default()` (`reaper.rs:392`, `retirement.rs:430`); test both directions (a logical context holds a wall tombstone as foreign and expires its own). - DST: `CrashAt` must stop all I/O at the crash point (a real crash, not one transient error) and assert the boundary state; the flip-first race must witness the flip landing between the rollback read and its commit, with no fallback publish. - Boundary tests: exact-tick and neighbour-tick tests for `W_session` (Completing), `W_completing` and `W_tombstone` (`reaper.rs:1084`, `:1101`, `retirement.rs:431`); a test that a fresh `Completing` session is kept. - `G_orphan`: have production read the bound the test checks (no unread copy at `gc.rs:304` vs `server/src/custodian.rs:115`); since server is child-2's, if wiring needs a server edit, raise it as a §6 item rather than editing. - Calibration: per-run identity in the TiKV namespace (as FDB has); record the ops actually sent; keep the raw first-attempt FDB/TiKV output in build-notes, or state plainly that no live cluster was available — do not quote figures without it. Decided at sign-off: set `W_TOMBSTONE_MILLIS` to 2 minutes (2 * MINUTE_MILLIS), not 15 — covers normal SDK Complete retries and lifts the ceiling from ~3 to ~23 completed multipart uploads/min with MAX_SESSIONS = 46. Keep `W_TOMBSTONE < W_OPEN`; update the doc comment's throughput figure, the unit check (`assert_eq!(W_TOMBSTONE_MILLIS, 120_000)`), the tests and `08-crosscutting-concepts.md` to match, and size the `custodian_gc.rs` fixture fix against the 2-minute window.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Keep the slice whole: production logic is ~9 KB (the 148 KB is tests, comments and diff context), so this is not a split. C4-ci is red because tombstone expiry ages out the fixture at `crates/server/tests/custodian_gc.rs:1470`/`:1531` (passes on base, fails with the patch).
  NARROW SCOPE EXCEPTION (granted at sign-off): this slice MAY edit, in child-2's (#1016) files, ONLY (1) the tombstone timestamp/age in `custodian_gc.rs`'s deployed-retirement test so the completed upload is inside the window when asserted — keep every partial-fleet safety assertion, and keep the change base-neutral — and (2) the tombstone wording in `06-runtime-view.md` (and the matching stale "retained tombstone" text in `05-building-block-view.md`) so the living docs agree with `08-crosscutting-concepts.md`. Nothing else in `crates/server/*`. Expect a small merge conflict with #1016; whichever lands second resolves it. Also justify the `core/multipart.rs` transition API beyond constants/doc in build-notes.md, or move it into the custodian.
  Fix in place:
  - Clock identity: bind the tombstone judgment to `ReaperContext::clock_source`, not the hard-coded `wall` in `ReaperPolicy::default()` (`reaper.rs:392`, `retirement.rs:430`); test both directions (a logical context holds a wall tombstone as foreign and expires its own).
  - DST: `CrashAt` must stop all I/O at the crash point (a real crash, not one transient error) and assert the boundary state; the flip-first race must witness the flip landing between the rollback read and its commit, with no fallback publish.
  - Boundary tests: exact-tick and neighbour-tick tests for `W_session` (Completing), `W_completing` and `W_tombstone` (`reaper.rs:1084`, `:1101`, `retirement.rs:431`); a test that a fresh `Completing` session is kept.
  - `G_orphan`: have production read the bound the test checks (no unread copy at `gc.rs:304` vs `server/src/custodian.rs:115`); since server is child-2's, if wiring needs a server edit, raise it as a §6 item rather than editing.
  - Calibration: per-run identity in the TiKV namespace (as FDB has); record the ops actually sent; keep the raw first-attempt FDB/TiKV output in build-notes, or state plainly that no live cluster was available — do not quote figures without it.
  Decided at sign-off: set `W_TOMBSTONE_MILLIS` to 2 minutes (2 * MINUTE_MILLIS), not 15 — covers normal SDK Complete retries and lifts the ceiling from ~3 to ~23 completed multipart uploads/min with MAX_SESSIONS = 46. Keep `W_TOMBSTONE < W_OPEN`; update the doc comment's throughput figure, the unit check (`assert_eq!(W_TOMBSTONE_MILLIS, 120_000)`), the tests and `08-crosscutting-concepts.md` to match, and size the `custodian_gc.rs` fixture fix against the 2-minute window.
- Failing gate: C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) — xtask: cargo test --workspace --exclude wyrd-dst: `cargo test --workspace --exclude wyrd-dst` failed with exit status: 1
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 84 mutants tested in 11m: 20 missed, 54 caught, 10 unviable
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 22 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1017/review
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Keep the slice whole and keep the design: the Completing exits, clock guard, crash/flip-first DST rows and calibration evidence hold. Fix: - REAL DEFECT — tombstone retry window: `W_tombstone` is measured from `PublishRequest.completed_at_millis`, stamped before `complete_upload` fences and writes segments (`crates/core/src/multipart/complete.rs:1474/:1522`, persisted unchanged at `:1834`), so a slow Complete can publish an already-expired tombstone and an immediate identical retry gets `NoSuchUpload` (reviewer probe, `reviewer-probe.log:14`). Take the stamp at publication through the same clock seam, and add a delayed-Complete regression proving the full 2-minute window survives publication. Do not change the 2-minute value. - Update the `W_tombstone` doc comment (`crates/custodian/src/reaper.rs:268-277`) to say when the window starts, and correct the "about 23 a minute" throughput figure for the fleet loop's interval and the drain before delete. - E-terminal server leg (`crates/server/tests/multipart_reaper_exits.rs:837-841`): assert the foreign `Completed` hold on one line carrying `reaper-session-held`, the `foreign-clock` reason and `mpu:{id}` together (or assert the `reaper_sessions_held` tally is 2), so making the `Completed` foreign arm a no-op turns the leg red. - WIDENED SCOPE EXCEPTION (granted at sign-off), in addition to last round's: this slice MAY edit `crates/server/src/custodian.rs` for exactly two things — (a) wire the deployed grace to the tested constant (`GC_GRACE_WINDOW_MILLIS = wyrd_custodian::gc::G_ORPHAN_MILLIS`, keeping the strict late-write compile-time check), so the G_orphan boundary test pins the deployed value; and (b) correct the stale comment at `:625-627` that says every `Completed` session keeps its record and slot. Nothing else in `crates/server/src`. Update the `gc.rs:303-308` doc to say production now reads it.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Keep the slice whole and keep the design: the Completing exits, clock guard, crash/flip-first DST rows and calibration evidence hold. Fix:
  - REAL DEFECT — tombstone retry window: `W_tombstone` is measured from `PublishRequest.completed_at_millis`, stamped before `complete_upload` fences and writes segments (`crates/core/src/multipart/complete.rs:1474/:1522`, persisted unchanged at `:1834`), so a slow Complete can publish an already-expired tombstone and an immediate identical retry gets `NoSuchUpload` (reviewer probe, `reviewer-probe.log:14`). Take the stamp at publication through the same clock seam, and add a delayed-Complete regression proving the full 2-minute window survives publication. Do not change the 2-minute value.
  - Update the `W_tombstone` doc comment (`crates/custodian/src/reaper.rs:268-277`) to say when the window starts, and correct the "about 23 a minute" throughput figure for the fleet loop's interval and the drain before delete.
  - E-terminal server leg (`crates/server/tests/multipart_reaper_exits.rs:837-841`): assert the foreign `Completed` hold on one line carrying `reaper-session-held`, the `foreign-clock` reason and `mpu:{id}` together (or assert the `reaper_sessions_held` tally is 2), so making the `Completed` foreign arm a no-op turns the leg red.
  - WIDENED SCOPE EXCEPTION (granted at sign-off), in addition to last round's: this slice MAY edit `crates/server/src/custodian.rs` for exactly two things — (a) wire the deployed grace to the tested constant (`GC_GRACE_WINDOW_MILLIS = wyrd_custodian::gc::G_ORPHAN_MILLIS`, keeping the strict late-write compile-time check), so the G_orphan boundary test pins the deployed value; and (b) correct the stale comment at `:625-627` that says every `Completed` session keeps its record and slot. Nothing else in `crates/server/src`. Update the `gc.rs:303-308` doc to say production now reads it.
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 95 mutants tested in 6m: 1 missed, 79 caught, 15 unviable
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 12 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1017/review
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: Split the slice; it is two concerns, and the second keeps generating findings. Patch grew 151 -> 267 -> 349 KB / 30 files over three rounds. Production logic is ~550 code lines (+976 incl. comments) in 10 files; the rest is tests. Keep (sound across three rounds, adversary could not refute): the Completing exits (W_completing rollback to Open@E+1, over-age straight to Aborting@E+1, both with retire:records:{seg:<g>:<E>}), the clock guard on terminal states, the G_orphan strict boundary, and the calibration evidence. Suggested split for the re-plan (pdca split): - Slice A: Completing exits + clock guard + G_orphan boundary + calibration (reaper.rs / retirement.rs side). - Slice B: tombstone expiry (W_tombstone) together with the publication-stamp fix in the Complete path (core/src/multipart/complete.rs, staged_publication.rs — ~300 logic lines). This is where every round-3 finding lives. Findings Slice B must address: - ReadyFlip::flip (crates/core/src/staged_publication.rs:510) accepts a different store than the one publish_segments verified; bind the store into the token or keep both phases behind one store-owning call; add a cross-store regression. - DST race fixture (crates/dst/tests/custodian.rs:8242/:8713) stamps publication with a frozen fence-time clock; with the stamp fix reverted all 9 reaper_* rows stay green. Drive the stamp from the reaper's ManualClock and assert the winning flip's tombstone survives the reaps and an identical retry answers AlreadyCompleted. - Stamp is read before the final flip commit (complete.rs:1655); a slow commit eats the 2-minute window. Add a delayed-successful-flip regression. Open questions for Plan (not decided at sign-off): - Whether "stamp immediately before the final commit" is acceptable, or the window must start after the commit lands. - Who owns the deployed-loop tombstone-expiry test (crates/server/src/custodian.rs:638 is untested; swapping it to Retain passes all tests): this split or sibling #1016 (custodian_gc.rs). - Live FDB/TiKV first-attempt calibration output was not available to the reviewer; Plan should require it be retained (brief.md:169).
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Split the slice; it is two concerns, and the second keeps generating findings. Patch grew 151 -> 267 -> 349 KB / 30 files over three rounds. Production logic is ~550 code lines (+976 incl. comments) in 10 files; the rest is tests.
  Keep (sound across three rounds, adversary could not refute): the Completing exits (W_completing rollback to Open@E+1, over-age straight to Aborting@E+1, both with retire:records:{seg:<g>:<E>}), the clock guard on terminal states, the G_orphan strict boundary, and the calibration evidence.
  Suggested split for the re-plan (pdca split):
  - Slice A: Completing exits + clock guard + G_orphan boundary + calibration (reaper.rs / retirement.rs side).
  - Slice B: tombstone expiry (W_tombstone) together with the publication-stamp fix in the Complete path (core/src/multipart/complete.rs, staged_publication.rs — ~300 logic lines). This is where every round-3 finding lives.
  Findings Slice B must address:
  - ReadyFlip::flip (crates/core/src/staged_publication.rs:510) accepts a different store than the one publish_segments verified; bind the store into the token or keep both phases behind one store-owning call; add a cross-store regression.
  - DST race fixture (crates/dst/tests/custodian.rs:8242/:8713) stamps publication with a frozen fence-time clock; with the stamp fix reverted all 9 reaper_* rows stay green. Drive the stamp from the reaper's ManualClock and assert the winning flip's tombstone survives the reaps and an identical retry answers AlreadyCompleted.
  - Stamp is read before the final flip commit (complete.rs:1655); a slow commit eats the 2-minute window. Add a delayed-successful-flip regression.
  Open questions for Plan (not decided at sign-off):
  - Whether "stamp immediately before the final commit" is acceptable, or the window must start after the commit lands.
  - Who owns the deployed-loop tombstone-expiry test (crates/server/src/custodian.rs:638 is untested; swapping it to Retain passes all tests): this split or sibling #1016 (custodian_gc.rs).
  - Live FDB/TiKV first-attempt calibration output was not available to the reviewer; Plan should require it be retained (brief.md:169).
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 115 mutants tested in 8m: 1 missed, 90 caught, 24 unviable
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 13 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1017/review
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
