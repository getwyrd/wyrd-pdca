# Adversarial review — issue 1013, iteration 5 (owned walk, by-chunk retirement, terminal delete)

All claims below come from a scratch copy of `$PDCA_TARGET` (the patch applied), with
`cargo test -p wyrd-custodian --test multipart_session_terminal` re-run there. The green
baseline reproduces: 40 passed.

- **This round's red→green holds (not a refutation).** I removed each of this round's two fixes
  by hand and re-ran the file. (1) Scan position off (`after` forced to `None` at
  `crates/custodian/src/retirement/sessions.rs:581`, the move at `:816` disabled): 8 tests fail.
  Among them are the behavioural ones: the three-held test's "retires within 8 passes"
  (`crates/custodian/tests/multipart_session_terminal.rs:2890`) and both page-of-held-entries
  legs (`:2659`, Aborting on a short fleet and Completed with `Tombstone::Elapsed`). (2) The old
  `covered > 0` rule put back on the bookmark choice (`sessions.rs:769`):
  `d_an_undecodable_entry_listed_to_the_brim_behind_two_held_ones_retires` fails (`:2820`). The
  tests run the production `reconcile_step`, with the context built the way the deployed caller
  builds it (`multipart_session_terminal.rs:697-736`, `operator_fleet_size = configured.len()`),
  not a copy of the logic. C4-verify's UNVERIFIABLE (the test calls API the patch adds) is the
  pre-declared posture; these hand reds stand in for it.

- NEEDS-HUMAN [impl] — **Held wide entries move the bookmark on every pass, for ever, and each
  move costs a session step. Sixteen such sessions starve every session listed after them.**
  How it happens:
  - When a step runs out of reads re-reading an entry from position 0, that entry becomes the
    `advanced` candidate (`sessions.rs:769-770`) even if it marked nothing.
  - `advanced` beats a held entry's saved count (`:779`), so the bookmark moves (`:787`).
  - A moved bookmark skips the free idle return (`:831`), so the commit is charged a step
    (`:840`), and the visit then ends (`:857`).
  - The session listing restarts at the head every pass (`:241`), so the same sessions pay
    first, every pass.

  **Repro:** take the fixture of `d_a_trusted_entry_behind_three_held_part_way_retires`
  (`multipart_session_terminal.rs:2863`) without its trusted entry: three RS(128,128) entries,
  each with a mark of none of the three shapes at position 150. In steady state that session
  commits 2 session-leg batches every pass. The bookmark cycles (entry 3 at 150) → (entry 2 at
  81) → (entry 2 at 150) → (entry 3 at 81) → … and never settles. Now list 16 such `Aborting`
  sessions ahead of a 17th aborted session that owns nothing. After 150 deployed passes the
  17th still has its `mpu:` record and its `mpuctl` slot. Every pass answers `Blocked` (for the
  churners' holds), and no audit event names the starved session. With 15 churners it ends
  normally.

  This breaks the module's own stated rule. `sessions.rs:71-73` and
  `crates/custodian/src/retirement.rs:166-167` say a held session must not spend a step every
  pass, because "enough of them would starve every session listed after them". That is the
  reason the scan position was put outside the step budget, but the bookmark commit is still
  inside it. It also breaks brief C-1 ("every session state the drain owns has an exit") for
  healthy sessions.

  **Test gap:** the existing three-held test only asserts that the trusted entry retires. It
  never asserts that the held steady state goes quiet. The page-of-held-entries test does
  (`:2692-2700`).

  **Trigger:** wide placements, where re-reading the held entries' already-marked positions
  costs more than the 233 reads a step has, plus holds that cost reads (unreadable or
  `reclaiming` marks, or a failed listing behind wide named positions). It is rare, but it is
  the same stall family as the iteration-3 and iteration-4 sign-off items.

  **Fix direction (the builder's call):** don't charge a step for a commit that only moves the
  bookmark while the step holds something, or don't let an entry that was re-read from 0 and
  marked nothing take the bookmark from a held entry's count. Add a steady-state-quiet assertion
  to the three-held test and a 17-session starvation regression.

- NEEDS-HUMAN [human] — **The gating T4 batch review is red only on DST scope.** Its two
  blocking items (`gate-logs/T4-batch-review.log`: `sessions.rs:586`, `:841`) ask for seeded
  Tier-0 DST (the repo's deterministic-simulation tests) coverage of the destructive owned-entry
  and terminal-delete paths. The brief says "add no DST scenario (#660 owns it)", and every
  sign-off repeated that. But the in-code marker at `sessions.rs:223` defers only "two drainers
  of one session". The rubric's test-fidelity MUST covers any "new destructive or concurrent
  path", so the reviewer's reading is not baseless. A human has to choose: record the rejection
  with the brief's reason, widen the `// deferred: #660` marker (if #660 really owns the
  single-drainer path), or open a tracking issue. Builder iteration can't settle this.

- **Tried to refute these; could not:**
  - *Whole-fleet gate.* `ByChunk::whole` (`retirement.rs`, `fleet.len() ==
    operator_fleet_size`) uses the same `fleet` and `operator_fleet_size` the deployed GC gate
    uses (`crates/server/src/custodian.rs:577`, and the `if fleet.len() == operator_fleet_size`
    GC branch). A failed `list_fragments` is a `PartialFleet` hold, not a short retirement.
  - *Scan-position hold counting* (`sessions.rs:814-815`). Every hold pushed in `walk_step`
    names its own entry key (`:403`, `:415`, `:429`, `:507`, `:518`, `:748`, `:759`, and
    `partial_fleet`), so a held entry is never miscounted as not held, or the reverse.
  - *Terminal delete on a non-empty range.* `Walked::Empty` comes only from the head page
    (`:586-597`), so entries before a scan position can't be skipped into an early delete.
  - *Exactly-once decrement.* The batch carries preconditions on the exact session-record bytes
    and the exact `mpuctl` bytes (`:899`, `:927`).
  - *Surviving C5 mutants.* The batch-budget arithmetic (`:483-484`, `:544-549`, `:659-668`)
    can't bind: the batch limit is 5 MB against at most ~2 MB of marks and deletes, and the op
    count is pinned by the `const` assert at `:163-164`. The `read_entry` owner guard (`:410`)
    is always true inside `sidx_range`. I found no survivor that hides a safety defect.

Scratch used: `$PDCA_SCRATCH/pdca-adversary-1013-redleg`, removed after this report.

<!-- pdca:leaf-complete -->
