# Build notes — issue 1015 (625.1): the multipart reaper, `Open`-session half

Withheld from the reviewer. For the human at sign-off.

## Base and prerequisites

- **Base commit:** `3c600b6` (`pdca-integrate: issue_1013`), the stack base the driver recorded
  (`results/issue_1015/stack-base-tip`, branch `pdca-integration/main-r9dd123c9ed70`). Every
  `path:line` below is on the patched tree over that base.
- **Prerequisites present on the base** (`git log --oneline 3c600b6`): `pdca-integrate: issue_1027`
  (49463ec), `issue_1007` (c9e236e), `issue_1008` (5a28b33), `issue_1009` (6eaf793), `issue_1010`
  (0de399f), `issue_1013` (3c600b6). #1012 is not a commit of its own on the stack; the drain it
  stood for arrives as `issue_1061` (21467e6, "drain multipart part and segment records") and
  `issue_1062` (b452551, "free the storage of aborted and replaced multipart uploads"), under
  #1013. **#659's drain is dispatched from `reconcile_step` on the base**:
  `git show 3c600b6:crates/custodian/src/reconciliation.rs` line 168, `retirement::reconcile(ctx)`.
- **#806 has not landed**: its deferral marker is still at `crates/custodian/src/gc.rs:1566`
  (`staged_fragments_observing`). So the profile comparison is one new `wyrd-custodian` function,
  `check_admission_profile` (`crates/custodian/src/reaper.rs:225`), whose doc names #806 as the
  caller to add at that deferral.

## What the patch does, with citations

Proposal 0016 decision 6 (`0016:1796-2277`) read in full, including the pass pseudocode
(`0016:2165-2230`) and the failure-mode table (`0016:2254-2278`). This slice implements the
pseudocode's `Open` branch only, in the order given.

1. **New module `crates/custodian/src/reaper.rs`.**
   - `ReaperPolicy` (`reaper.rs:142`), `#[non_exhaustive]` with `Default`: `w_open_millis`,
     `w_session_millis`, `profile`, `tombstone`. Siblings add windows as fields; callers outside
     the crate start from `default()`. `ReaperPolicy::tombstone()` is the route for #659's
     tombstone input (still `Retain`; 625.3 flips it there).
   - `ReaperContext` (`reaper.rs:177`): `meta`, `clock`, `clock_source`, `policy`. Plain pub fields,
     the `GcContext` shape.
   - Windows: `W_OPEN_MILLIS = 7 days` (`reaper.rs:106`), `W_SESSION_MILLIS = 30 days`
     (`reaper.rs:112`), each with a compile-time bound check (`≤ 7 d`, `≥ 21 d`, and
     `W_open < W_session`) right below. Leg A's fixture (created 14 d ago, progress 10 d ago)
     leaves 3 d margin below `W_open` and 16 d below `W_session`.
   - The pass, `reconcile` (`reaper.rs:282`): profile check first (`:287` refuses on mismatch,
     returns `Blocked` before reading a session); `mpu:` listing in pages capped at the ledger's
     `max_sessions` (`:313`); per session (`visit`, `:345`): clock guard first (`:363`), then the
     `Completing`-and-later deferral to #1017 (`:368`), one clock read per session before its ranges
     (`:376`), the ceiling arm with no slot pins (`:379`), then the idle arm reading `slot:` first
     (`read_slots`, `:434`), `psum:` second (`latest_psum`, `:485`), and the owned `sidx:` leases
     last and only when otherwise idle (`owned_lease_live`, `:528`).
   - The fence (`fence`, `:610`) reuses #895's one builder, `fence_batch`
     (`crates/core/src/multipart/abort.rs:81`), and adds the idle arm's pins over the whole key
     space `[0, MAX_INFLIGHT_PARTS)` (`pin_slots`, `:568`). Each batch is measured against
     `MAX_BATCH_OPS`/`MAX_BATCH_BYTES` (`fence_cost`, `:588`; check at `:653`) and never sent past
     them. No splitter. A `Conflict` is counted and re-judged next pass (`:674`).
   - Telemetry on the existing seam: `monotonic_counter.reaper_*` events (`emit_pass`, `:754`),
     audit target `wyrd.custodian.reaper.audit` for holds, the foreign-clock signal and the profile
     refusal (action `reaper-profile-mismatch`).
   - Unit tests: `largest_fence_fits_both_batch_budgets` (`:819`) builds the idle fence over a
     session record with a 1,024-byte object name and 16 pins of the widest slot record and checks
     both budgets; `idle_pins_cover_the_whole_slot_key_space` (`:853`).
2. **`reconcile_step` gains `reaper: Option<&ReaperContext<'_>>`**
   (`crates/custodian/src/reconciliation.rs:142`). With a reconstruction input, the order is
   reconstruction → reaper (`:176`) → retirement leg. With no reconstruction input but a reaper,
   it runs the reaper and then the retirement leg **record-only** (`:188`, `:197`).
3. **Retirement leg plumbing (#659's layer, touched for wiring only).** The leg now reads through a
   small `Drain` view (`crates/custodian/src/retirement.rs:579`) instead of the whole
   `ReconstructionContext`, and `ByChunk` carries `Option<fleet>` (`:421`, `:429`) so a no-fleet
   pass holds every untrusted record as `PartialFleet` instead of treating `0 == 0` as "whole
   fleet". `reconcile_record_only` (`:564`) is the no-fleet entry. `sessions.rs` signatures
   switched from `&ReconstructionContext` to `&Drain` (`retirement/sessions.rs:258`, `:325`, `:591`,
   `:914`). No drain behavior changed. This matters: without the `Option`, an empty fleet with
   `operator_fleet_size = 0` would pass the whole-fleet gate and retire an untrusted record by chunk
   id over an empty listing, deleting the record and marking nothing.
4. **`Budget::shipped()`** (`crates/core/src/multipart.rs:1754`): returns the private
   `SHIPPED_PROFILE`, so the custodian has a typed local profile to compare. Not a format change
   and not a constructor of caller-chosen values.
5. **Server.** `CustodianService::reconcile_pass` threads `reaper`
   (`crates/server/src/custodian.rs:395`). The deployed loop builds `ReaperPolicy::default()` once
   (`:589`), routes the tombstone through it (`:627`), and passes the reaper on its existing
   reconstruction pass (`:633`, `:679`). New `reap_abandoned_uploads_once` (`:462`) is one
   `reconcile_pass` with only the reaper. CLI: `--reap-abandoned-uploads` joins `VALUELESS_FLAGS`
   (`crates/server/src/cli.rs:2882`), is parsed at `:1005`, refused together with
   `--reconcile-after-restore`, handled before the `--endpoints` check (`:1102`), dispatched per
   backend by `run_reaper_once_over_backend` (`:1657`, the `run_restore_reconcile_over_backend`
   shape). It exits `FAILURE` on `Blocked`.
6. **Ripple.** 143 call sites gained a `None` reaper argument, by a script that inserts it before
   the last argument of every `reconcile_step(`/`reconcile_pass(` call, then `cargo fmt`. Files:
   every `crates/custodian/tests/*.rs` that calls it, `crates/dst/tests/custodian.rs`,
   `crates/chunkstore-grpc/tests/tier{1,2}_*.rs`, `crates/server/tests/custodian_day_one.rs`.
7. **Docs.** `docs/design/architecture/06-runtime-view.md:94` (new paragraph in 6.7, after the
   terminal-delete paragraph, written so 625.2 can append) and
   `docs/design/architecture/08-crosscutting-concepts.md:113` (a "Multipart sessions" bullet in
   8.9, so 625.3 can append the `Completing` and tombstone windows).

## Production reach

Deployed loop: `cmd_custodian` → `run_reconstruction_over_backend` → `CustodianService::
run_reconstruction_until` → per interval, the reconstruction `reconcile_pass(..., Some(&ctx), None,
Some(&reaper), ...)` (`custodian.rs:679`) → `reconcile_step` → reconstruction, then
`reaper::reconcile`, then `retirement::reconcile`. A healthy fleet therefore reaps every interval.
One-shot: `cmd_custodian` with `--reap-abandoned-uploads` → `run_reaper_once_over_backend` →
`reap_abandoned_uploads_once` → the **same** `reconcile_pass` → `reconcile_step` → `reaper::reconcile`,
then `retirement::reconcile_record_only`. No test-only entry point exists. Not covered here (625.2):
a custodian with no `--endpoints` runs no loop at all, so outside the one-shot it never reaps.

## Operation capacity (the brief's leg-H note)

Equal stored profiles do not prove equal operation budgets: `B_ops` is `KnobSet::batch_ops`
(`crates/core/src/multipart.rs:6152`), and `KnobSet::profile` (`:6190`) leaves it out. This slice
adds no format field. Operation capacity rests on three things instead:

1. Every reaper batch is sized from `MAX_BATCH_OPS` (`multipart.rs:5936`) and `MAX_BATCH_BYTES`
   (`:5897`), and measured against them before it is sent (`reaper.rs:653`); no other budget is
   defined in the reaper (`:5895` says #625 must not re-derive it, and it doesn't).
2. `knob_clamps_hold` (`multipart.rs:6279`; its fence clamps at `:6309-6310`) and
   `crates/core/tests/multipart_knobs.rs` (leg 1, the shipped set passes every clamp, around
   `:167-171`) are unmodified by this patch and green in the gate.
3. The `B_ops` calibration (`multipart.rs:5932`, "Uncalibrated") is 625.3's.

## Tests and evidence

### Binding red→green: `crates/server/tests/multipart_reaper.rs` (new, not cfg-gated)

Five plain `#[test]`s; each seeds a redb store with canonical raw records, drops it, runs
`wyrd custodian --data-dir <dir> --reap-abandoned-uploads` (flag last) as a subprocess with a
120 s bound (`:49`), reopens the store and asserts on it. Convergence is bounded by
`CONVERGE_RUNS = 6` (`:54`).

- A `idle_abandoned_session_is_fenced_and_drained_with_every_byte_evidenced` (`:425`)
- B `idle_arm_never_reaps_a_progressing_upload` (`:488`)
- C `over_age_session_is_fenced_and_drained_however_live` (`:550`)
- E-open `foreign_clocked_open_sessions_survive_and_a_foreign_aborting_one_drains` (`:584`)
- H `profile_mismatch_signals_and_mutates_nothing` (`:663`)

**RED leg (base production code + only this test file):** I checked out `3c600b6`'s `crates/`,
removed `reaper.rs`, restored only the new test file, and ran it. **5 tests ran, 5 failed.** Every
one-shot exited with `wyrd: flag --reap-abandoned-uploads needs a value` (the parser at
`cli.rs` `ParsedArgs::parse` demands a value for any flag outside `VALUELESS_FLAGS`). On that first
red run A, C and E-open failed on the exit-status check inside the convergence helper; I then
reordered the helper so the store-convergence assertion comes first
(`Store::reap_until`), so on the base they now fail on durable state ("the store did not converge"),
as the brief asks. H fails on its signal assertion (`stderr` lacks `reaper-profile-mismatch`).
**B is red on the base too, but only through its final exit-status assertion**; its store
assertions are vacuous on the base (nothing reaps). I kept that status check on purpose: a B that
passes when the binary never ran its pass would be inert. B's real teeth are negation (1) below.

**GREEN leg:** 5/5 pass on the patched tree, inside the full gate too.

I did not run `engine/scripts/run-verify.sh` myself: it builds in the shared `../wyrd-verify`
checkout, which is Check's. The manual red above is the same experiment (base production code,
test file kept).

### `crates/custodian/tests/gc.rs`, appended case — no same-test red

`reaper_marks_are_reclaimed_by_gc_at_the_grace_window` (`gc.rs:1275`). Leg A's fixture over
in-memory stores, converged with `reconcile_step(…, Some(&reaper), T0)` (no fleet), then the
fragments of both byte sets put on six in-memory D servers; three GC passes at `T0 + GRACE − 1`
reclaim nothing, three at `T0 + GRACE` reclaim all six. It asserts each mark is stamped `T0` by the
one clock. **Not a C4 red→green**: it does not compile against the base `reconcile_step`. Its red
is negation (15).

### DST rows, appended to `crates/dst/tests/custodian.rs` (no new file under `crates/dst/tests/`)

Module `reaper_rows` (`:7040`), registered as four `dst_campaign_test!`s:
`reaper_x60_progress_reads_source_first` (`:7286`), `reaper_x62_idle_fence_pins_the_slot_evidence`
(`:7321`, with a seeded coin for the ceiling half), `reaper_x19_mid_pass_session_is_never_condemned`
(`:7382`), `reaper_pre_first_chunk_window` (`:7441`, live and crashed halves). A `Hooked` store
commits one batch at the exact reaper step each row names (before the second progress read; before
the fence commit; at the first `slot:` read). Ages and ids are drawn from the run seed. Green over
20 seeds locally and inside the gate's `cargo test -p wyrd-dst (--cfg madsim)` stage.

### Negations (each run, then reverted)

| # | Broken variant | Leg that catches it | Result |
|---|---|---|---|
| 1 | progress from `created_at` + `psum:` only, slot ignored (reserved stamp and slot lease both dropped) | B(ii) in `multipart_reaper.rs`; DST `reaper_pre_first_chunk_window` | both **red** (B at its snapshot assertion; DST at the live-half `Open` assertion) |
| 2 | read `psum:` before `slot:` | DST `reaper_x60_…` | **red** at the `Open` assertion after the hook fired |
| 3 | idle fence without slot pins | DST `reaper_x62_…` (non-ceiling half) | **red** at the `Open` assertion |
| 6 | skip foreign-clocked sessions wholesale (patched into the session leg, since the reaper itself never touches non-`Open` sessions) | E-open | **red** (the `Aborting` session never converges) |
| 10 | idle arm disabled, ceiling kept | A | **red** (store does not converge) |
| 14 | no profile comparison | H | **red** at the signal assertion |
| 15 | no reaper dispatched from `reconcile_step` | `gc.rs` reaper case | **red**, but at its convergence assertion ("torn down within four passes"), before it reaches the GC passes. Same root cause; it never gets as far as "no mark, nothing deleted" |

X60 and X62 cannot be built at record level in the integration test; they are only claimed for the
DST rows, as the brief says. X19 has no single-line negation I could build (a stale-snapshot
reaper is a different design), so that row is not negation-tested; it does fail if the drain walks
an `Open` session's `sidx:` range, which is the shape the row guards.

## Refuting my own test

- **(a) Genuine red?** Yes. With the fix reverted (base `crates/` + the new test file), 5 ran and 5
  failed. Each leg also has its own negation that turns only that leg red (table above).
- **(b) Production path?** Yes. The test runs the built `wyrd` binary
  (`env!("CARGO_BIN_EXE_wyrd")`) against a real redb store, through `cmd_custodian` →
  `reconcile_pass` → `reconcile_step` → the production reaper and the production retirement drain.
  No mock, no copy.
- **(c) Fixture includes the fault?** Yes. Leg A seeds the actual abandoned session with both byte
  sets (a committed part and crashed in-flight residue with lapsed leases) and asserts marks on
  every position of both; H seeds the actual mismatched profile beside two sessions that would
  otherwise be fenced; E-open seeds real foreign-clocked sessions in both states. Leg A's timestamps
  are chosen so only the idle arm can fire (negation 10 confirms: with only the ceiling, A stays red).

## Gate

`./engine/xtask.sh ci` (the project runner, `cargo xtask ci`, with `PDCA_WORKTREE` set to the cycle
worktree) on the final tree: **exit 0**, no failing check. That covers typos, `lint_docs.py`,
`render_site.py --check`, the unsafe-forbid and statics guards, fmt, clippy `-D warnings`, the
workspace tests, cargo-deny, and the `--cfg madsim` DST clippy and tests. The first run failed one
check, the `#![forbid(unsafe_code)]` crate-root guard on the new test file; fixed, then rerun green.
After the final edits the red leg was rerun with the final test file: 5 ran, 5 failed, A/C/E-open
at "the store did not converge within 6 one-shots", H at its signal assertion, B at its exit-status
assertion.

## Decisions and what I ruled out

- **Reaper before the drain in the same step**, not after. After the drain, a session fenced this
  pass would wait a whole interval (30 s) before draining, and the one-shot would need one more run.
  Cost of my choice: a metadata store fault inside the reaper's reads fails the step before the
  drain runs that interval. I judged that acceptable because the same fault would almost certainly
  hit the drain's reads too. Flagging it so a reviewer who prefers strict fault isolation can ask
  for the swap; it is a two-block move in `reconciliation.rs:176-187`.
- **The parameter, not a field on `ReconstructionContext`.** A field would have forced the one-shot
  to build a `ReconstructionContext` with an empty fleet and a fake topology, and `reconcile_step`
  would then run reconstruction over that empty fleet, which reads a non-empty repair queue as
  "every fragment confirmed gone" (`reconstruction.rs:106-118`, the `unreachable` doc). The
  parameter costs the 143-site mechanical ripple instead.
- **A reaper hold makes the step `Blocked`.** An undecodable session is also held by the drain, so
  such a session is reported twice per pass. I kept both: they are different legs and each names
  what it could not do.
- **Absent `mpuctl`** reads as "nothing admitted": the pass goes ahead, bounded by the local
  profile's `max_sessions`. Refusing instead would make a damaged store with sessions but no ledger
  an absorbing state for the reaper. The terminal delete already holds on a missing ledger.
- **Clock-source spelling.** `WALL_CLOCK_SOURCE = "wall"` (`reaper.rs:96`) lives in the reaper, the
  spelling every in-tree wall-clocked writer and test uses. No production gateway creates a
  session yet, so there was no core constant to reuse.
- **Pass cadence** is the deployed loop's interval (30 s default). A slower cadence would need a
  stored last-run instant; not added.

## Items for the human

- **Touches #659's layer for plumbing** (point 3 above). Behavior unchanged, but the `ByChunk`
  change is a real safety fix for any no-fleet caller. If the reviewer considers any of this #659's
  to own, it is a §6 item against that layer, per the brief.
- **One added `pub` in core** (`Budget::shipped`). Not a record or state-machine change.
- **B's base red is through exit status only** (explained above).
- No external dependency was missing: `typos` and the docs renderer (`lint_docs.py`,
  `render_site.py --check`) both ran inside `cargo xtask ci` and passed.

## Scratch

Created under `$PDCA_SCRATCH` (`/var/tmp/pdca/wyrd-pdca-9c587031/issue_1015`):
`pdca-builder-1015-scripts/` (the ripple and negation scripts), `pdca-builder-1015-logs/` (gate
logs), `pdca-builder-1015-madsim-target/` (a separate cargo target dir for `--cfg madsim` builds,
several GB). Left for the harness to reclaim, per its filesystem rules. The worktree is left with
the change uncommitted on top of `3c600b6`.
