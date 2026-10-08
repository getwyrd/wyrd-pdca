# Adversarial review — #843 (restore-fence DST coverage)

**Verdict: I could not break the main claim.** The three new properties run the production
fence (`reconcile_after_restore` → `fence_session`, `crates/custodian/src/restore.rs:801-866`)
over `SimTikvMetadataStore`, which has a hop inside each commit. They fail when the fence is
broken. I tried to break the evidence in four ways and only found a weak check on the
*reported cause* (first bullet). What I ran, in a scratch copy of `$PDCA_TARGET` with
`RUSTFLAGS=--cfg madsim MADSIM_TEST_NUM=50`:

- Green leg: `restore_fence_never_shares_the_epoch_with_a_session_writer`,
  `restore_fence_settles_an_ambiguous_commit_on_the_next_pass` and
  `restore_fence_reaches_the_contested_window` all pass. This matches `gate-logs/C4-ci.log:3853-3865`.
- D4(a), with `.require(key, read)` removed at `crates/custodian/src/restore.rs:824`: D1 and D3
  go **red** at `crates/dst/tests/custodian.rs:5576`. Open arm, writer at 5500 µs: the fence
  and the writer were both `Committed`, so the left side was 2 and the right side 1.
- D4(b), session in one commit and obligations in a second (`restore.rs:828-832`): D2 goes
  **red** at `custodian.rs:5446`. Open arm, `Landed`: the session moved to `Aborting` and
  `retire:` was empty. The reverse order (obligations first, then session) turns all three
  tests red.
- I checked the `stale: true` label (`custodian.rs:5589`) against the sim's commit model
  (`crates/dst/tests/support/mod.rs:300-357`, two 1 ms hops with locks taken at prewrite). A
  writer apply logged between the fence's read and the fence's answer always happens *before*
  the fence's prewrite. So the `Conflict` in that window is a failed precondition, not a lost
  lock race. A lock-race conflict logs the writer *after* the fence's answer, so it is labeled
  `stale: false`. D3's reachability claim is therefore sound.

## Findings

- NEEDS-HUMAN [impl] — `crates/dst/tests/custodian.rs:5660-5666`: when the writer wins, the
  test accepts `ChangedUnderPass | LostConflict | ObligationKeyTaken { .. }` as the reason the
  pass gives. That lets a wrong diagnosis through. **Concrete failing case:** change
  `restore.rs:844-856` to check the obligation keys *before* re-reading the session, so
  `ChangedUnderPass` is never reported. In the Completing arm the pass then reports
  `ObligationKeyTaken { retire:records:s:<id>:3 }`. That key is the root flip's own `{parts}`
  obligation, and the operator summary turns it into "whose retirement key was already taken
  by another obligation" (`crates/server/src/cli.rs:1484`, `:1494`) for an upload that simply
  published. The Open arm reports `LostConflict` instead. With this change all three new tests
  still pass, and so does the existing `crates/custodian/tests/restore_completing_fence.rs`.
  Only `restore_open_fence.rs:592` catches it, and only for the Open shape. This patch is the
  only test that models the flip sharing the fence's `retire:records:s:<id>:<E>` key, so it is
  the place to pin this. In this model, every writer-won schedule re-reads the session
  (`restore.rs:844`) after the writer has applied, so `ChangedUnderPass` is the only correct
  cause. I narrowed the match to `ChangedUnderPass` alone and re-ran on the **unmodified**
  fence: all three tests pass across 50 seeds. So the stricter check costs nothing and adds a
  real check.
- `crates/dst/tests/custodian.rs:5863` (D3, Completing arm) — a limit, not a defect. In this
  arm the fence's `Conflict` would happen even without the session-bytes precondition. The
  flip writes `retire:records:s:<id>:3` (`custodian.rs:5346-5347`, put at `:5370`), the same key the
  fence `require_absent`s (`restore.rs:825-827`), so the fence loses on that key alone.
  Mutation (a) above was caught **only** by the Open arm. That meets the brief ("at least one
  seed of D1"), but the Completing arm's "stale-preimage conflict" does not show that the
  session precondition is what did the work. Nobody should read the Completing arm as guarding
  that precondition.
- NEEDS-HUMAN [human] — `crates/custodian/src/restore.rs:785` still says
  `// deferred: #843 — seeded Tier-0 DST coverage of this fence (809.5).`, and the doc comment
  at `restore.rs:478-482` names only `restore_two_readings_never_license_a_mark` as the DST pin.
  When this bundle closes #843, the marker will point at a closed issue and still say the
  coverage is missing. The brief's scope ("nothing outside that file") correctly kept the
  builder out of `restore.rs`. A human should decide whether to allow a one-line comment edit
  in this PR or file a follow-up.
- `gate-logs/C4-verify.log:62-64` — the `C4-verify` row in `check-gates.json` says "red->green:
  pass", but the gate ran green-only ("the per-fix RED can't be isolated"). `C4-diff-cov` and
  `C5-mutants` measured nothing on this test-only diff ("n/a", "No mutants to filter"). The
  brief said this in advance, so it is not a refutation. But no gate showed a red in this
  bundle. The red exists only in the builder's D4 notes and in my re-run above.

## Tried and could not break

- **Whether the test reaches production code:** the tap (`custodian.rs` `FenceMeta`) passes
  every call through to `SimTikvMetadataStore`. It intercepts only to log, and to strike in
  D2. It is not bare `MemMeta`. The writer commits through the same store but bypasses the
  strike.
- **Log attribution:** if the fence's listing ever stopped going through
  `scan_page(MPU_PREFIX)`, the test would classify against the staged read and fail loudly
  (fence and writer both counted as winners). It would not pass silently.
- **Existing properties unchanged (L):** the two new calls are added at the end of the
  regression-seed chain (`custodian.rs:6103-6104`), so earlier properties' RNG draws do not
  move. No existing assertion was edited.
