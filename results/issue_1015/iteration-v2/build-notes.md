# Build notes — issue 1015 (625.1), iteration 2: the multipart reaper, `Open`-session half

Withheld from the reviewer. For the human at sign-off.

## Summary

This iteration starts from iteration 1's patch (it still applies to the same base) and fixes
what sign-off sent back:

| Carry-forward item | Status |
|---|---|
| 1. Slot ABA race (reserve → part commit → release between the reads and the idle fence) | **Not closed. It can't be closed without a 0016 change.** Stopped as instructed. The false doc claims are fixed. The race is reproduced by an `#[ignore]`d DST row. **NEEDS-HUMAN**, see below. |
| 2. Profile refusal must mutate nothing | Fixed. A refusal is now its own result and suppresses the retirement leg in both `reconcile_step` branches. Per-session holds still let the drain run. Leg H is extended with an already-`Aborting` upload, its obligation, a part and residue. |
| 3. A per-session read fault must not skip #659's drain | Fixed. Any store fault reading one session's ranges, or the listing, is a hold. New seeded DST row. |
| Rebase onto current main | The patch applies cleanly on the stack base merged with current `origin/main` (checked). It can't apply on bare `origin/main`, because #1061/#1062/#1013 aren't merged there. See *Base*. |
| Leg B: slot reserved 1 h ago, lease lapsed | Added, B(iv). |
| Fence cost against an independent count | Done. The unit test adds up the batch from its parts and compares with `fence_cost`. |
| Non-zero exit on a held session | New subprocess leg. |
| Move `WALL_CLOCK_SOURCE` to `wyrd_core::multipart`, fix its doc | Done. |

## Base and prerequisites

- **Base commit:** `3c600b6` (`pdca-integrate: issue_1013`), branch
  `pdca-integration/main-r9dd123c9ed70`, the stack base the driver recorded
  (`results/issue_1015/stack-base-tip`). Every `path:line` below is on the patched tree over it.
- **Prerequisites on the base** (`git log --first-parent 3c600b6`): `issue_1008` (5a28b33),
  `issue_1009` (6eaf793), `issue_1010` (0de399f), `issue_1013` (3c600b6), plus #1007 and #1027,
  which are merged to main under it (8932adb, e62a746). #1012's drain arrives as `issue_1061`
  (21467e6) and `issue_1062` (b452551) under #1013. **#659's drain is dispatched from
  `reconcile_step` on the base**: `git show 3c600b6:crates/custodian/src/reconciliation.rs`
  line 168, `retirement::reconcile(ctx)`.
- **#806 has not landed.** Its deferral is still at `crates/custodian/src/gc.rs:1566`. So the
  profile comparison stays one `wyrd-custodian` function, `check_admission_profile`
  (`crates/custodian/src/reaper.rs:247`), and its doc names #806 as the second caller.
- **"Rebase onto current main".** `origin/main` is now `57aefe0`. Compared with the stack base
  it adds only CI workflows, the secret guard (`.pre-commit-config.yaml`,
  `.secret-guard/activate.sh`) and `Cargo.lock` bumps (`git diff --stat 3c600b6...origin/main`:
  9 files, none under `crates/` or `docs/`). It already contains #1008–#1011 and #1028–#1030. It
  does **not** contain #1061, #1062 or #1013. I checked application without touching the
  worktree:
  - `git merge-tree --write-tree 3c600b6 origin/main` merges cleanly, and `git apply --cached
    --check` of this patch against that tree **succeeds**;
  - against bare `origin/main` it fails, because `crates/custodian/src/retirement.rs` does not
    exist there.

  So the C4 diff-coverage gate that applies the patch to bare `origin/main` will keep reporting
  "does not apply" until #1061/#1062/#1013 merge. That comes from how the gate picks its base,
  not from this patch. I can't move the harness's stack base myself. If you want diff coverage
  measured this round, the gate needs the stack base (or a refreshed one: stack base + main
  merges cleanly).

## Item 1 — the slot ABA race: not closable in this slice (NEEDS-HUMAN)

**The defect, reproduced.** `crates/dst/tests/custodian.rs:7589`
(`prop_slot_cycle_between_the_reads_and_the_fence`) seeds a session the idle arm judges
abandoned, with its one slot index read free. Just before the fence commits, the hooked store
commits a real reserve batch (`require(mpu)`, `require_absent(slot)`, `put slot`, the shape of
`crates/core/src/multipart/stage.rs:1330-1333`), then a real part-commit batch (`require(mpu)`,
`require_absent(part)`, `require(slot == bytes)`, `put part`, `put psum`, `delete slot`, the
shape of `crates/core/src/multipart/commit.rs:1199-1215`). The fence then commits, and the
session goes `Aborting` with the part it has just committed. Run with `--ignored`:
`assertion left == right failed: Changed / left: Some(Aborting) / right: Some(Open)`. It is
registered `#[ignore = "known gap: …"]` at `crates/dst/tests/custodian.rs:7871` as the
regression for whoever closes it.

**Why it can't be closed here.**

- The store offers only point preconditions: `require(key == bytes)` and `require_absent(key)`
  (`crates/traits/src/lib.rs:1491-1543`). There's no range or version precondition, and the
  trait has only `get`/`scan`/`scan_page`/`commit`.
- What a completed reserve→commit cycle leaves changed:
  - the new part number's `part:` and `psum:` keys;
  - a `retire:` key, but only on a re-upload;
  - nothing else. The slot is absent again (the commit deletes it), the sidx entries are
    created and deleted, and the session record is required by the commit but not rewritten
    (`commit.rs:1199`).
- The part number is the client's choice, anywhere in `[1, MAX_PART_NUMBER]` (9,999). Pinning
  every candidate's absence costs 9,999+ preconditions against `MAX_BATCH_OPS` = 500
  (`crates/core/src/multipart.rs:5949`).
- No per-session key exists that every part commit rewrites. So the fence has nothing to pin
  that tells "cycle happened" from "nothing happened".
- `0016:1882-1883` says "a renewal, a release, or a *new* reservation between the reads and the
  fence therefore turns the fence into a Conflict". That holds only while the reservation is
  still standing when the fence lands. A finished cycle is not covered: 0016's X62 reasoning
  has a gap.

**Options that would close it. Each is a 0016 amendment, so I built none of them.**

- **(a) A per-session witness every part commit rewrites.** For example, a progress counter or
  a "last commit" field on a per-session key, which the fence pins.
  - It is a record and format change to #1027's commit batch, plus a new key or field.
  - It makes every part commit of a session contend on one key.
  - Estimate: +1 put and +1 precondition in `commit_batch` (`commit.rs:1190-1222`) and in
    `compensation_batch` (`:1226-1246`); a new record type with codec and conformance vectors;
    +1 pin in the reaper fence. Roughly 150–250 lines across core and the custodian, plus
    vectors.
- **(b) The reaper claims every free slot index before its last read.**
  - The reaper puts a lapsed placeholder slot record at each free index under
    `require_absent`, re-reads `psum:`, then fences, pinning the placeholders.
  - No format change: a lapsed slot is already legal residue (F11a). But it makes the reaper a
    writer of `slot:` keys, which 0016 says it never is (`0016:1853`).
  - A live client would see `503 SlowDown` during the judgment.
  - A reaper crash between the claim and the release leaves the session unable to reserve until
    a later pass reaps or releases it. That needs a recovery rule in 0016.
  - Estimate: ~120 lines in `reaper.rs` (claim batch, release batch, second `psum:` read),
    plus the recovery rule.
- **(c) A range or "unchanged since" precondition on `MetadataStore`.** A trait change across
  four backends (redb, TiKV, FDB, and the test doubles) and their conformance suite.

**What I did instead**, per the carry-forward ("stop and say so"):

- Fixed the two doc claims the sign-off named:
  - the module doc now says "a reservation **still standing** when the fence lands", and has a
    *Known gap* section (`crates/custodian/src/reaper.rs:56-72`);
  - `pin_slots`' doc says the same (`reaper.rs:629-632`);
  - the runtime view states the gap in plain words
    (`docs/design/architecture/06-runtime-view.md:94`, "One gap is stated rather than
    hidden …").
- Shipped the ignored repro row.
- I did **not** add a "re-read right before the fence" narrowing. It shrinks the window but
  doesn't close it (the hooked DST row still fails at the fence commit), and code that looks
  like a fix but isn't would mislead review.
- **Impact if shipped as-is.** A client that has been silent past `W_open` (7 days) and comes
  back exactly inside the reaper's read→fence window has its upload aborted together with the
  part it just committed. It sees `NoSuchUpload` on its next request and must restart. No
  published object is affected.

**For the human:**

- Decide whether this ships with the gap stated, or waits for a 0016 amendment.
- Proposed tracking issue: "0016 X62: the idle fence misses a reserve→commit slot cycle between
  the reaper's reads and its fence".
- The T4 batch-review finding at the old `reaper.rs:579` will likely recur. It is a real
  finding; if you accept shipping with the gap, record it in `review-rejected.md` as a
  deferral, not as noise. Suggested line:
  `crates/custodian/src/reaper.rs:644 | BUG | require_absent passes again | deferred: needs a 0016 change (build-notes item 1); repro row reaper_slot_cycle_between_the_reads_and_the_fence`.
  I didn't write that file myself; it's a triage decision.

## Item 2 — a profile refusal now writes nothing

- `reaper::reconcile` returns `Reaped` (`crates/custodian/src/reaper.rs:267`): `Refused` when
  the profile differs or the ledger doesn't decode, else `Ran(Reconciled)`.
- `reconcile_step` goes through `reap()` (`crates/custodian/src/reconciliation.rs:220`). On
  `Refused` it records `Blocked` and returns `false`, and both call sites (`:177` with a
  reconstruction input, `:187` record-only) then skip the retirement leg.
- A per-session hold is `Ran(Blocked)`, so the drain still runs. That keeps the foreign-clock
  teardown (E-open's `Aborting` half) and holds of undecodable sessions working.
- With no reaper supplied, the retirement leg runs exactly as on the base.
- **Consequence for the deployed loop.** While the profile mismatches, the deployed custodian
  drains no `retire:` obligation at all, including ordinary overwrite obligations. The brief's
  "mutates nothing" and 0016's fail-closed reading (`0016:2063-2072`) both point this way. The
  operator signal (`reaper-profile-mismatch`) and the `Blocked` outcome make it visible.
  Reconstruction (before the reaper) and rebalance (after) still run that step.
- **A #1013 test fixture had to change.** `crates/server/tests/custodian_gc.rs`
  (`deployed_retirement_leg_waits_for_the_whole_operator_fleet_and_keeps_completed_tombstones`)
  created its two sessions through the production `create_session` under a small test profile
  (`w_ref 128`, every other number 1), so its `mpuctl` stored a profile the deployed custodian
  is not sized for. In iteration 1 that was harmless: the reaper refused but the drain ran
  anyway, which was exactly the bug. With the fix, the deployed loop refuses the whole leg on
  that store, and the test (which checks the drain's partial-fleet hold) failed in my first
  full gate run (`custodian_gc.rs:1490`, "the hold reaches the log sink …"). The fixture now
  creates under `Budget::shipped()` (`custodian_gc.rs:1229-1234`). Nothing in the test
  depended on the small profile's numbers (it creates two sessions; parts are seeded raw). That
  is a change to #1013's test, forced by item 2's semantics. If you'd rather #1013's owner make
  it, it's a §6 item.
- **Leg H** (`crates/server/tests/multipart_reaper.rs:694`) now also seeds an already-`Aborting`
  upload with its `retire:bytes:` obligation, a committed part and owned residue (`:711`, via
  the new `aborting_session` helper at `:348`, which E-open now shares). Under negation (16)
  below, H goes red at "the store is untouched". Iteration 1's Open-only H stayed green under
  that defect.

## Item 3 — a read fault in the reaper never skips the drain

- In `pass()` (`reaper.rs:341`), an `Err` out of one session's `visit` is held as
  `store-fault` (`:395`), and the listing moves to the next session.
- A listing page that can't be read is held as `listing-unreadable` (`:376`) and ends the
  listing only.
- Either way the pass answers `Ran(Blocked)` and the retirement leg runs.
- Only a fault reading `mpuctl` itself is still an error that fails the step. With the profile
  unknown, nothing may act (module doc, `reaper.rs:50-54`).
- **DST row:** `prop_read_fault_holds_the_session_and_the_drain_runs`
  (`crates/dst/tests/custodian.rs:7535`), registered at `:7865`.
  - The seed draws which of the faulted session's three ranges (`slot:`, `psum:`, owned
    `sidx:`) the store refuses, pass after pass.
  - Every pass must answer `Blocked`, not an error.
  - The `Aborting` upload beside it must be deleted, its `part:` and `sidx:` ranges empty.
  - The faulted session's records must be byte-identical.

## Other carry-forward items

- **`WALL_CLOCK_SOURCE`** now lives at `crates/core/src/multipart.rs:2442`. Its doc says what is
  actually true: it names the wall clock; the Create caller passes it as
  `CreateRequest::clock_source`; no production caller creates a session yet (#668 is the first),
  so only tests write it today; and it names a clock rather than proving one. The custodian's
  re-export was removed (`crates/custodian/src/lib.rs:42`). The server
  (`crates/server/src/custodian.rs:78`), `gc.rs` and the DST file import it from core.
- **Leg B(iv)** (`multipart_reaper.rs:540`): a slot reserved an hour ago whose lease lapsed half
  an hour ago. It survives because the reservation stamp is progress.
- **Fence cost against an independent count:** `largest_fence_fits_both_batch_budgets`
  (`crates/custodian/src/reaper/tests.rs:646`).
  - It counts 20 ops: session require + obligation `require_absent` + 2 puts + 16 pins.
  - It sums the bytes of every key and value from the teardown's own parts, and asserts
    `fence_cost(&batch) == (20, bytes)`.
  - It checks that `within_budgets` accepts the result.
- **Non-zero exit on a held session:** `a_held_session_fails_the_one_shot_and_the_pass_goes_on`
  (`multipart_reaper.rs:738`).
  - An undecodable session record sits beside leg A's idle session.
  - Every one-shot names the hold and exits non-zero.
  - The idle session is still torn down, its bytes licensed, and the held record is left byte
    for byte.

## Unit tests in the custodian crate (the C5 survivors)

Iteration 1's mutation run was 85 missed out of 142. Most of them were reaper logic, because the
mutants gate runs only the mutated crate's tests, and nothing in `wyrd-custodian` drove the
reaper except one `gc.rs` case. New file `crates/custodian/src/reaper/tests.rs` (13 tests):

- **The store.** An in-memory store whose pages hold at most 3 entries. Short pages are legal
  under the `scan_page` contract, so every paged walk turns pages.
- **The profile.** A 5-number test profile (2 slot indices, 3 parts, 3 chunks per part), so
  every bound is a handful of records.
- **Boundaries.** One pass over 12 sessions puts each arm's boundary at exactly `W` and `W + 1`.
  This covers the ceiling, the idle window, slot lease == now, owned lease == now, fresh `psum`,
  fresh reservation, foreign, and non-`Open`. The test asserts the exact tallies.
- **Damage beside its undamaged twin.** 5 vs 6 summaries, 6 vs 7 owned entries, and so on,
  which kills the `+`/`*`/`-` bound mutants.
- **Outcomes.** Listing past the admitted count (with and without a ledger), unreadable
  listing, lost fence → `Partial`, failed commit → hold, empty → `Satisfied`.
- **Operator signals.** Each pass runs under its own capturing `tracing` subscriber, so the
  signals are asserted from what was emitted. No global subscriber or static is used, so the
  ADR-0035 statics gate isn't touched.
- **Budget check.** `within_budgets` is factored out (`reaper.rs:672`) so its inclusive
  boundary can be tested; the real fence can never reach 500 ops.
- **Tallies.** `Pass` is returned by the inner `pass()` (`reaper.rs:341`), so the tests read
  the tallies directly.
- **One small simplification.** `read_slots` no longer re-checks the owner of a key read under
  the session's own fixed-width prefix (iteration 1's `&owner == upload`). It can't differ, and
  the mutant on it could never be killed.

I did not rerun `cargo mutants` (Check's gate runs it). I expect most of iteration 1's reaper
survivors to be caught now. The `emit_*` → `()` mutants are covered by the log assertions.

## Production reach (unchanged from iteration 1)

- **Deployed loop:** `cmd_custodian` → `run_reconstruction_over_backend` →
  `CustodianService::run_reconstruction_until`. Each interval runs the reconstruction pass
  `reconcile_pass(…, Some(&ctx), None, Some(&reaper), …)` (`crates/server/src/custodian.rs:680`)
  → `reconcile_step` → reconstruction → `reap()` → `retirement::reconcile`. The policy is
  `ReaperPolicy::default()` (`custodian.rs:590`), and the tombstone is routed through it
  (`:628`).
- **One-shot:** `wyrd custodian --reap-abandoned-uploads` (`crates/server/src/cli.rs:1005`,
  valueless at `:2882`) → `run_reaper_once_over_backend` (`cli.rs:1657`) →
  `reap_abandoned_uploads_once` (`custodian.rs:463`) → the same `reconcile_pass` →
  `reconcile_step` → `reap()` → `retirement::reconcile_record_only`. It exits non-zero on
  `Blocked` (`cli.rs:1121`).
- No test-only entry point.

## Operation capacity (the brief's leg-H note)

Equal stored profiles don't prove equal operation budgets. `B_ops` is `KnobSet::batch_ops`
(`crates/core/src/multipart.rs:6165`, set at `:6194`), and `KnobSet::profile` (`:6203`) leaves
it out. No format change was made. Operation capacity rests on three things:

1. Every reaper batch is sized from `MAX_BATCH_OPS` (`multipart.rs:5949`) and `MAX_BATCH_BYTES`
   (`:5910`), and measured against them before it is sent (`reaper.rs:672`, used in `fence`).
   No other budget is defined in the reaper; `:5908` says #625 must not re-derive it.
2. `knob_clamps_hold` (`multipart.rs:6292`) and `crates/core/tests/multipart_knobs.rs` leg 1
   (`:172`) are unmodified (`git diff --stat 3c600b6 -- crates/core/tests/multipart_knobs.rs` is
   empty) and green in the gate.
3. The `B_ops` calibration (`multipart.rs:5945`, "Uncalibrated") is 625.3's (#1017).

## Tests and evidence

### Binding red→green: `crates/server/tests/multipart_reaper.rs` (new, not cfg-gated)

Six plain `#[test]`s. Each one:

- seeds a redb store with canonical raw records and drops it;
- runs `wyrd custodian --data-dir <dir> --reap-abandoned-uploads` (flag last) as a subprocess,
  bounded at 120 s (`:52`);
- reopens the store and asserts on what it holds.

Convergence is bounded at `CONVERGE_RUNS = 6` (`:57`).

**RED leg.** Base production code plus only this file. I snapshotted the work in a temporary
commit, ran `git checkout 3c600b6 -- crates docs` (the new file stays), and ran
`cargo test -p wyrd-server --test multipart_reaper`. **6 ran, 6 failed.** Every one-shot printed
`wyrd: flag --reap-abandoned-uploads needs a value`.

- A, C and E-open failed on durable state: "the store did not converge within 6 one-shots"
  (`:199`).
- H failed on its signal assertion (`:723`).
- The held-session leg failed on durable state: the idle session wasn't torn down (`:767`).
- B failed on its exit-status assertion (`:578`). **B's store assertions are vacuous on the
  base**, because nothing reaps. Its real teeth are negations (1) and (1b).

**GREEN leg.** 6/6 pass on the patched tree.

### `crates/custodian/tests/gc.rs`: appended case, no same-test red

`reaper_marks_are_reclaimed_by_gc_at_the_grace_window` (`gc.rs:1275`), unchanged from
iteration 1 except for the import. Leg A's fixture is converged by the reaper; the fragments
of both byte sets are then put on six in-memory D servers. GC at `T0 + GRACE − 1` reclaims
nothing, and GC at `T0 + GRACE` reclaims all six. It is not a C4 red→green: it doesn't compile
against the base `reconcile_step`. Its red is negation (15).

### Negations (each applied alone to the snapshot, run, then restored with `git checkout <snapshot> -- <file>`)

| # | Broken variant | Leg run | Result |
|---|---|---|---|
| 1 | progress from `created_at` + `psum:` only; slot stamp and slot lease both ignored | B | **red**, "no record of a progressing upload moved" (`:569`) |
| 1b | slot reservation stamp ignored, slot lease kept | B (iv) | **red**, same assertion |
| 6 | foreign-clocked sessions skipped wholesale (patched into the retirement session leg, since the reaper never touches non-`Open` sessions) | E-open | **red**, "store did not converge" |
| 10 | idle arm disabled, ceiling kept | A | **red**, "store did not converge" |
| 14 | no profile comparison (`== local \|\| true`) | H | **red** at the signal assertion |
| 15 | no reaper dispatched from `reconcile_step` | `gc.rs` reaper case | **red**, but at its convergence assertion ("torn down within four passes", `gc.rs:1353`), before it reaches the GC passes. Same cause; it never gets as far as "no mark, nothing deleted" |
| 16 | **iteration 1's defect:** a refused profile check still runs the retirement leg | H | **red**, "the store is untouched" |
| 17 | **iteration 1's defect:** a per-session read fault ends the step (`visit(…).await?`) | DST read-fault row; unit `damage_holds…` | both **red**: the DST row at `unwrap()` on `Err(Store("an injected read fault"))`, the unit test at its `expect` |

Interleaving negations (2) (`psum:` before `slot:`, X60) and (3) (no slot pins, X62) are DST-only
by construction, as the brief says. They were run in iteration 1 against the same rows, which
this iteration didn't change apart from the `Hooked::new` batch list. I did not rerun them.

### DST rows (all in the existing `crates/dst/tests/custodian.rs`; no new file under `crates/dst/tests/`)

- X60, X62, X19 and pre-first-chunk are unchanged in substance.
- New: the read-fault row, and the ignored slot-cycle row.
- `MADSIM_TEST_NUM=20 cargo test -p wyrd-dst --test custodian reaper_` (`--cfg madsim`, separate
  target dir under `$PDCA_SCRATCH`): **5 passed, 1 ignored**. The ignored one, run with
  `--ignored`, fails as described under item 1.

## Refuting my own test

- **(a) Genuine red?** Yes. With the fix reverted (base `crates/` and `docs/`, only the new test
  file kept), 6 ran and 6 failed, for the reasons listed above. Each leg also has a negation
  that turns it red alone (table above), including both of iteration 1's defects (16, 17).
- **(b) Production path?** Yes. The test runs the built `wyrd` binary
  (`env!("CARGO_BIN_EXE_wyrd")`) against a real redb store, through `cmd_custodian` →
  `reconcile_pass` → `reconcile_step` → the production reaper and the production retirement
  drain. No mock, no copy. The unit tests call the production `pass()`/`reconcile()` over an
  in-memory store.
- **(c) Fixture includes the fault?** Yes.
  - A seeds the abandoned session with both byte sets (committed part plus crashed residue,
    leases lapsed) and asserts marks on every position.
  - H now seeds the exact shape the reviewer reproduced: an `Aborting` upload with obligation,
    part and residue under a mismatched profile.
  - The DST read-fault row injects a real store `Err` on the reaper's reads.
  - The held-session leg seeds a real undecodable record.
  - Leg A's timestamps make only the idle arm able to fire; negation 10 confirms it.

## Gate

`./engine/xtask.sh ci` (the project runner, `PDCA_WORKTREE` set to the cycle worktree) on the
final tree: **exit 0, "xtask ci: all checks passed".**

- That run covers typos, `lint_docs.py`, `render_site.py --check` (101 pages), the repo guards
  (unsafe-forbid, gitlinks, blackbox), fmt, clippy `-D warnings`, the workspace tests,
  cargo-machete, cargo-deny, conformance vectors, the statics guard, the orchestrator guard,
  and the `--cfg madsim` DST clippy and tests at 50 seeds.
- The new tests all ran inside it: the 13 `reaper::tests`, the `gc.rs` reaper case, all 6
  `multipart_reaper` legs, and 5 reaper DST rows green with the slot-cycle row ignored.
- **The first full run failed one test**, `custodian_gc.rs`'s
  `deployed_retirement_leg_waits_for_the_whole_operator_fleet_and_keeps_completed_tombstones`
  (see item 2: its fixture stored a non-shipped profile). After fixing that fixture, the second
  full run passed.
- `cargo fmt --all -- --check` is clean (run from the worktree, under its pinned toolchain).
- `gitleaks stdin` over the whole patch: "no leaks found". `origin/main` now has a gitleaks
  pre-commit hook; the stack base doesn't yet, but a publish commit made on top of main would
  run it.
- `git apply --check --whitespace=error-all` of `patch.diff` against `3c600b6` passes, and so
  does a plain `--check` against `3c600b6` merged with `origin/main` (`57aefe0`).
- `patch.diff` is 42 files, ~255 KB. That is iteration 1's 40, plus `reaper/tests.rs` and
  `custodian_gc.rs`. The bulk is tests and the mechanical `None` ripple of the widened
  `reconcile_step`.
- The bundle copy of the test is at `results/issue_1015/crates/server/tests/multipart_reaper.rs`,
  byte-identical to the patched file.

## Decisions and what I ruled out

- **Refusal as a separate result rather than a flag on `Blocked`.** Iteration 1's bug was
  exactly that a refusal and a hold were the same `Blocked`. A separate enum variant makes the
  dispatcher's choice explicit and checkable. The cost is one 2-variant enum and one 20-line
  helper.
- **Holding faults rather than running the drain first.** The carry-forward allowed either.
  Running the drain first would delay every fenced session's drain by a whole pass, and the
  one-shot would need one more run. Holding keeps the order (fence, then drain in the same step)
  and isolates faults per session, the way decode faults already were.
- **The unit tests live in `reaper/tests.rs`**, not inline, to keep `reaper.rs` readable.
  cargo-mutants skips the `#[cfg(test)] mod tests;` file.
- **Docs.** The brief scopes docs to `06` and `08`; both are updated. `05-building-block-view.md:246`
  still says the custodian's multipart consumption is the drain (#1061/#1062/#1013) and that
  "nothing else writes or consumes them in production yet". With the reaper the custodian also
  writes the `Open → Aborting` fence and reads `mpuctl`/`slot:`/`psum:`. I left `05` alone
  because the brief assigns doc files per slice. If you read "docs currency" as covering it, it
  is a one-sentence addition.
- **Everything else is as in iteration 1:** the parameter rather than a context field, the
  windows (7 d / 30 d), an absent `mpuctl` read as "nothing admitted", the pass cadence, and
  #659's layer touched for plumbing only (`Drain` view, `ByChunk` as `Option<fleet>`).

## Items for the human (NEEDS-HUMAN)

1. **The slot-cycle gap (item 1).** Ship with the gap stated, or hold for a 0016 amendment.
   The options and costs are above.
2. **Diff coverage can't be measured against bare `origin/main`** until #1061/#1062/#1013 merge.
   The patch applies on stack base + main.
3. **Fitness of the windows** (7 d idle, 30 d ceiling): unchanged from iteration 1, still owed.

## Scratch

Created under `$PDCA_SCRATCH` (`/var/tmp/pdca/wyrd-pdca-9c587031/issue_1015`):

- `pdca-builder-1015-scripts/`: the negation helper, patch copies, a temporary git index;
- `pdca-builder-1015-logs/`: gate logs. `ci-v2.log` is the first full run (the
  `custodian_gc` failure); `ci-v3.log` is the final, passing run. `ci.log` and `ci2.log` are
  iteration 1's;
- `pdca-builder-1015-madsim-target/`: the `--cfg madsim` cargo target, several GB.

They're left for the harness to reclaim, per its filesystem rules (no `rm` from the builder).
The worktree is left with the change uncommitted on top of `3c600b6`. The temporary snapshot
commit (`07eca58`) used for the red leg and the negations has been reset away; it survives
only as an unreferenced object.
