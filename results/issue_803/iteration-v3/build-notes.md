# Build notes — #803 staged protection class in the shared reference set (662.1), iteration 3

Target: `getwyrd/wyrd` @ `main` = `78f9859` (re-checked with `git ls-remote`: `main` has not moved).
Every `path:line` below is on the patched tree (`main` + `patch.diff`) unless marked "base".

## What this iteration changes, and why

The sign-off sent iteration 2 back for two things.

**Item 2 (the required fix): nothing pinned "scrub and drain status keep today's answers"**
(brief.md:96-99). The adversarial reviewer showed that adding
`unresolvable.extend(staged.unresolvable.clone());` just before `Ok(ReferenceSet {`
(`crates/custodian/src/gc.rs:641`) passed all 11 tests, while it turned scrub's answer into
`Blocked` fleet-wide over one torn `sidx:` key. That gap is now closed by a scrub check in
`e1_an_sidx_key_naming_no_chunk_blocks_gc_and_restore_but_never_scrub`
(`crates/custodian/tests/staged_protection.rs:1071`, the test the carry-forward named, renamed so
its name says what it now checks; the old name is still a prefix of the new one).

**Item 1: fail-closed retention stays as it is.** One unreadable staged record still blocks GC
and restore fleet-wide. No production line changed: `gc.rs`, `restore.rs`, `cli.rs`, the DST file
and both docs are byte-identical to iteration 2 (checked per file by hash against
`iteration-v2/patch.diff`). What the tests now pin about that tradeoff, end to end:

- the stall itself: all three E(i) tests assert GC answers `Blocked` and reclaims nothing, and
  restore marks nothing and names the record (`staged_protection.rs:1018-1127`);
- its limit per chunk: the three E(ii) tests assert an untrusted-but-named record holds only its
  own chunk while unrelated fragments are still judged (`:1129-1234`);
- its limit per pass (new): the torn-`sidx:` record does not reach scrub — scrub still does its
  work and answers `Changed`, not `Blocked`, and names no staged record on its audit trail
  (`:1085-1116`).

The drain-status half is deliberately **not** pinned (see "Choices" below); a
`// deferred: #664` marker sits at the site (`:1096`).

### The new check, line by line

- `scrub_pass` (`staged_protection.rs:457`): one scrub pass through the production
  `reconcile_step` with only a `ScrubContext`, so its answer is not merged with GC's through
  `least_certified` (`crates/custodian/src/reconciliation.rs:142-147`).
- In the `sidx:` test, after the GC and restore assertions: seed one committed object whose only
  fragment is missing from its D server (`LOST`, `LOST_INODE`, `:1011-1013`, seeded at `:1099`),
  run scrub (`:1100`), then assert:
  - the repair for `LOST` was enqueued (`queued_repairs`, `:1101`) — scrub actually walked;
  - the answer is exactly `Reconciled::Changed` (`:1106`) — not `Blocked`;
  - scrub's audit seam does not name the torn `sidx:` key (`:1112`).
- `assert_blocks_both_passes` now borrows the store (`&Meta`, `:1018`) so the `sidx:` test can
  run scrub on the same store afterwards; its three callers pass `&meta`.
- Module doc updated (`:13-14`, `:41-49`), plus a `SCRUB_AUDIT` constant (`:101`) and two imports
  (`ScrubContext`, `wyrd_core::repair::queued_repairs` — both on `main` already; the test still
  names no symbol this slice adds).

## Choices and alternatives rejected

- **Where the check lives: the `sidx:` test only, not the shared `assert_blocks_both_passes`.**
  0016 says scrub never acts on an owned entry, because an in-flight chunk has no committed
  scheme to verify against (`0016:775-780`). So "a torn `sidx:` key never blocks scrub" stays
  true after #663. An unreadable `part:` value or `mpu:` key is different: once scrub verifies
  committed parts (#663, `0016:824`), whether those block scrub is #663's decision. Putting the
  check in the shared helper would cost about the same lines but would pin an answer #663 may
  have to flip. The comment at `:1093-1095` says so.
- **Drain status: left out, with `// deferred: #664` at the site.** The carry-forward allowed
  either. I left it out because the answer it would pin is wrong by 0016's own standard: the
  drain must count owned entries as held (`0016:827`), and "count only committed parts, not
  in-flight owned entries" is a listed failure mode whose observable is exactly a `Satisfied`
  drain (`0016:883`). Today's answer over this store is `Satisfied`, so asserting it would put a
  known C-1 gap into a test. The scrub check alone already catches the named mutant (below), so
  leaving the drain out loses no coverage of it.
- **Scrub over the unchanged E(i) store, asserting `Satisfied`.** Rejected. With no committed
  object, scrub has nothing to do and answers `Satisfied`, which is also what `reconcile_step`
  returns when given no scrub context at all (`reconciliation.rs:134`). That assertion could not
  tell "scrub ran and was not blocked" from "scrub did nothing". Cost of the chosen version over
  that one: 5 more lines (two constants, one seed, one `queued_repairs` assertion).
- **`assert_ne!(outcome, Blocked)` instead of `assert_eq!(outcome, Changed)`.** Rejected: the
  exact answer is known, and the file's convention is exact answers.
- **The negative audit assertion (`:1112`) is not vacuous**: the same capture carries scrub's own
  `"action":"missing"` line for `LOST` (visible in the mutant logs quoted below), so the capture
  was live when it found no line naming the `sidx:` key.

## Evidence

### Red → green (the project's C4-verify runner)

`PDCA_BUNDLE=results/issue_803 PDCA_BRIEF_BASE=origin/main ./engine/scripts/run-verify.sh`, run on
the final `patch.diff` (its own clean `origin/main` = `78f9859` worktree):

- `GREEN — cargo test -p wyrd-custodian --test staged_protection (fix applied)`: `11 passed`.
- `RED — … (production reverted, test kept)`: `0 passed; 11 failed`.
- `run-verify.sh: PASS — red without the fix, green with it (11 test(s) ran red).`

**11 tests ran red, all by assertion.** The failing lines on the red leg are all property
assertions: A `:764` (fragment reclaimed), B `:792` (fragment marked), C1 and C2 `:852` (moved
fragment reclaimed), D `:981` (fragment reclaimed), E(i) ×3 `:1025` (a stray marked while the
record was unreadable), E(ii) ×3 `:1160` (a held fragment marked). None is an unwrap, a fixture
panic or a compile error. On base the `sidx:` test fails at `:1025`, before it reaches the scrub
check (base restore marks the stray), which is fine: base is red for the brief's reason.

### The carry-forward's mutant, before and after

Each row: the final `patch.diff` plus one mutant, run through `run-verify.sh` (GREEN leg = "with
the fix" = with the mutant). The worktree was restored after each build and compared byte-for-byte
against `patch.diff`; the final probes were spliced into patch copies so the running CI was never
touched.

| Mutant | Iteration 2's test | This test |
|---|---|---|
| `unresolvable.extend(staged.unresolvable.clone());` before `Ok(ReferenceSet {` (`gc.rs:641`) — the carry-forward's | **11/11 green** (the gap) | `sidx:` test red at `:1106`, `left: Blocked, right: Changed`; other 10 green |
| scrub answers on the whole set: `Ok(if referenced.is_incomplete() {` (`crates/custodian/src/scrub.rs:205`) | not run | red at `:1106` (`Blocked`) |
| scrub names staged records: a loop over `referenced.staged.unresolvable` calling `emit_unscrubbable` after `scrub.rs:116` | not run | red at `:1112` ("scrub named the owned entry sidx:e2…:000001:not-a-chunk …") |

Under the first mutant the captured scrub audit shows both the leak and the live capture:
`"action":"unresolvable-chunk-map","inode":"sidx:e2e2…:000001:not-a-chunk"` and
`"action":"missing","dserver":0,"chunk":"…e2"` — the repair was still enqueued (the
`queued_repairs` assertion passed), only the answer changed.

The last two rows are outside this patch (it does not touch `scrub.rs`); I ran them because they
are the other two ways the stall could leak into scrub, and each is caught by its own assertion.
The drain side of the same leak (`desired_state.rs:225` reading the staged half) is not caught, by
the choice above; the gc.rs fold also changes drain status, but the scrub check catches it first.

### Full gate

`./engine/xtask.sh ci` (= `cargo xtask ci` in the worktree), on the final state, identical to
`patch.diff`: **`xtask ci: all checks passed`**, exit 0. Steps: typos, docs lint and render, the
gitlink and unsafe guards, `cargo fmt --check`, clippy `--workspace --all-targets`, build,
`cargo test --workspace` (`staged_protection`: 11 passed, including
`e1_an_sidx_key_naming_no_chunk_blocks_gc_and_restore_but_never_scrub`;
`cli::tests::restore_verdict_names_the_blocking_records_and_counts_the_ones_it_cannot_fit` ok),
machete, deny, statics, deploy-guard, madsim DST clippy + test (custodian suite 18 passed,
including `gc_staged_build_under_concurrent_handoffs` and `gc_staged_build_reaches_every_landing`).

## The fix itself (unchanged since iteration 1, restated for sign-off)

- `gc.rs:662` `StagedSet { placed, held, unresolvable }`, the staged member; `gc.rs:451`
  `ReferenceSet.staged`, kept apart from `placed`, `malformed` and `unresolvable`, because scrub
  and drain status read exactly those three (`scrub.rs:88`, `:114`, `:205`;
  `desired_state.rs:188`, `:225`) and keep their answers. The new test now pins the scrub half
  of that.
- `gc.rs:468` `protection()`: reasons `staged` and `staged-malformed`, before
  `incomplete-reference-set`; `gc.rs:494` `is_incomplete()` covers both halves and drives GC's
  `Blocked` (`gc.rs:378`) and restore's mark gate (`crates/custodian/src/restore.rs:408`).
- `gc.rs:552`: the staged class is read before the `inode:` scan; `gc.rs:789`
  `staged_fragments`: one `scan("mpu:")`, then per session `sidx:<id>:` then `part:<id>:` —
  `sidx:` → `part:` → `inode:`, bounded per-session ranges, no global scan (`0016:782-800`,
  `:890`).
- Restore: unreadable staged records join the `unresolvable` union and the audit seam
  (`restore.rs:323`); held ones are named on the seam with `// deferred: #664` (`restore.rs:326`).
- Docs: `docs/design/architecture/06-runtime-view.md` §6.7 step 2. Two edits outside the brief's
  named files, kept from iteration 1: `crates/server/src/cli.rs` (the restore verdict no longer
  calls every `unresolvable` entry a "committed object") and
  `docs/design/architecture/m4-first-deployment-blueprint.md:610-611`. If judged out of scope, drop
  both; nothing else depends on them.

## The three refutation questions

- **(a) Genuine red?** Yes. Whole file: `run-verify.sh` reverted production, kept the test, and
  got `0 passed; 11 failed`, each on a property assertion (lines above). The carry-forward's gap
  specifically: with iteration 2's test the mutant was 11/11 green; with this test the same
  mutant fails the `sidx:` test at `:1106` (`Blocked` vs `Changed`), and the two scrub-side
  mutants fail at `:1106` and `:1112`.
- **(b) Production path?** Yes. Scrub runs through the public `wyrd_custodian::reconcile_step`
  with a `ScrubContext`, which calls the real `scrub::reconcile` → the real
  `referenced_fragments` → `staged_fragments`. The repair queue is read back with the production
  `wyrd_core::repair::queued_repairs`. Only the stores are doubles.
- **(c) Fixture includes the fault?** Yes. The torn `sidx:` key is in the store when scrub runs —
  the same record that blocks GC and restore earlier in the same test — and scrub is given real
  work (a committed fragment it must find missing), so a pass that did nothing, or was never run,
  cannot satisfy the check.

## Commit-readiness

- `cargo fmt --all -- --check` clean; `typos` clean on the test; `cargo clippy -p wyrd-custodian
  --all-targets -- -D warnings` clean; the full `cargo xtask ci` above ran fmt, clippy and typos
  over the workspace.
- The target repo has no commit hooks configured (no `core.hooksPath`, no hook files, no
  pre-commit config), so CI's fmt/clippy/typos steps are the commit bar. DCO sign-off is the
  publish step's.
- `patch.diff` was produced with `git diff` (the new test added with intent-to-add) in the
  worktree whose HEAD is `78f9859`, and `run-verify.sh` applied it cleanly to a fresh
  `origin/main` checkout.

## Size

`patch.diff` is 122,388 bytes (iteration 2: 119,119). The +3,269 bytes are all in
`staged_protection.rs`. The human waived the size threshold for this bundle.

## What I read beyond `brief.md`

The carry-forward cites line numbers that only exist in the previous patch, so I read
`iteration-v2/patch.diff`, its `build-notes.md`, `check-advisory-adversary.md` ("the adversary's
note" the carry-forward refers to), `check-review.md`, `check-gates.md` and
`session-carry-forward`. I also read the brief's cited background (0016 decision 2,
`0016:765-893`), the cited `scrub.rs` and `desired_state.rs`, `reconcile_step`'s signature
(`reconciliation.rs:122-164`) and `enqueue_repair` / `queued_repairs`
(`crates/core/src/repair.rs:138-159`) to write the scrub check against the real API.

## External dependencies

`typos` and `docs-renderer` (the brief's list) were present and exercised by `cargo xtask ci`.
Nothing outside that list was needed, so there is no NEEDS-HUMAN external-dependency item.
