# Adversarial review — issue 1018 (`wyrd mpu abort`, FU-6 part 1)

**Bottom line: I could not refute the fix.** I re-ran the proof, ran the one negation that leg B
cannot catch (fence split over two batches), tried the race and doubt paths by reading the code,
and threw hostile command lines at the built binary. The one-batch fence, the exact-epoch segment
obligation (F18) and the X57 closure all held. What is left is two small operator-hint gaps
(both already among T4's items) and some notes on how far each piece of evidence reaches.

## The evidence

- **The red→green proof is real, but on its own it only shows the verb exists.** GREEN reproduced
  here: `cargo test -p wyrd-server --test multipart_operator_abort`, 6 passed. RED
  (`gate-logs/C4-verify.log`): 6 ran, 6 failed, so this is not a zero-test run. But all six RED
  failures are the same one: exit 2 `unknown command mpu` at each test's first exit-code
  assertion (`crates/server/tests/multipart_operator_abort.rs:564`, `:637`, `:716`, `:795`, `:902`,
  `:960`). The checks that actually test behaviour are elsewhere: the exact changed-key sets
  (`:575-579`, `:641-645`), the decoy check (`:665-672`, `:683-687`), C5 (49 mutants, 0 missed)
  and the DST sweep. This is a note on what the C4 row proves, not a refutation.
- **Negation 2 (records obligation in a second batch) is caught. I ran it myself.** In a scratch
  copy I patched `fence_rounds` (`crates/core/src/multipart/abort.rs:526-528`) so that the
  `Completing` door commits the session plus the bytes obligation first, then the records
  obligation in a separate batch. Two tests then failed at `crates/dst/tests/custodian.rs:8699-8701`
  ("a fenced session carries every obligation of its door"):
  `operator_abort_completing_door_cut_reaches_both_sides` and
  `operator_abort_outcome_unknown_claims_no_more_than_it_saw`. With the file restored, all 7 DST
  rows pass. Two limits on what this shows:
  - The seeded single-cut row (`custodian.rs:8848-8852`) still **passed** under the negation with
    `MADSIM_TEST_NUM=1`, because one random cut rarely lands in the ~1 ms gap. The deterministic
    sweep (`:8859-8882`) is what catches the split.
  - Both live-Complete race rows (`custodian.rs:9229-9432`) **passed** under the negation, because
    they judge only end states. They are no evidence for atomicity, and build notes or the PR text
    should not cite them for it.

## The fix: what I tried and could not break

- **Wrong segment epoch (F18).** The decoder refuses a `Completing` record whose publish-target
  epoch differs from its own epoch (`crates/core/src/multipart.rs:2963-2966`), and
  `completing_teardown` builds the group from that epoch (`:2703-2708`, `:2767-2778`). The records
  obligation can only name `seg:<g>:<E>:`.
- **Racing the root flip.** The flip moves the session in the same batch that writes
  `retire:bytes:s:<id>:<E>`. `lost` reads the session again after it finds the key
  (`abort.rs:602-613`), so the abort answers from the new state (`Completed`) rather than
  reporting a false `ObligationKeyTaken`.
- **Racing a Complete-fence release** (`Completing@E → Open@E+1` plus `retire:records:s:<id>:<E>`).
  The abort's fence loses, the session reads as moved, and it fences `Open@E+1` under a different
  key. Nothing is stranded.
- **A false "nothing written" after an unknown outcome.** The drain never rewrites an `Aborting`
  record; it only deletes it (`crates/custodian/src/retirement/sessions.rs:947`, `:974-976`). So the
  byte-for-byte match in `Doubt::landed` (`abort.rs:643-649`) cannot miss our own landed fence and
  answer `AlreadyAborting`. Absence while in doubt answers `AbsentAfterDoubt` (`abort.rs:505-508`).
- **Two operator aborts at once.** The loser reads `Aborting` and answers `AlreadyAborting`, with
  no second obligation.
- **Regression in the client Abort.** `fence_rounds` with one obligation does the same steps as
  the old loop, and `lost_any` over one key is the same as `lost`. C4-ci passed the existing
  `multipart_abort.rs` suite.
- **Hostile command lines** against the built binary: `--upload-id=ID`, `-- extra`, a trailing
  `--log-level`, an uppercase id, `--data-dir -x`, a directory with no store, bare `mpu`, and
  `mpu ABORT`. Each was refused (exit 1, or exit 2 from the usage / pre-parse path) and no store
  file was created. `parse_exact` (`crates/server/src/cli.rs:3473-3493`) and `parse` (`:3443-3465`)
  split the arguments the same way, because every flag the verb accepts takes a value and no value
  may start with `--`.

## Findings

- NEEDS-HUMAN [impl] — `crates/server/src/cli.rs:3273-3276`: when the absolute cluster-file path
  is not valid UTF-8, the FDB spelling quietly falls back to the **relative** cluster file. A
  concrete case: `WYRD_FDB_CLUSTER_FILE=fdb.cluster` set for the one command, run from a non-UTF-8
  working directory. The printed follow-up commands then carry
  `WYRD_FDB_CLUSTER_FILE='fdb.cluster'`. Replayed from another directory (exactly the case the
  v2 sign-off asked to close), they read a different cluster file, or none. The redb path refuses
  the same situation (`cli.rs:3166-3178`). Fix: refuse it the same way. This one bug is 4 of T4's
  12 blocking items (`cli.rs:3276`, reported four times). It needs the fdb feature, a relative
  cluster file and a non-UTF-8 working directory all at once.
- NEEDS-HUMAN [impl] — `crates/server/src/cli.rs:3369-3374`: the stop-condition hint gives no way
  out for an upload the record-only one-shot can never finish. If an `Aborting` session has a
  staging entry that fails the placement rule or will not decode, every pass that does not see the
  whole fleet holds it (`crates/custodian/src/retirement/sessions.rs:731-745`), and
  `--reap-abandoned-uploads` is metadata-only. An operator following the hint loops for ever, each
  pass `BLOCKED` and naming this upload. The hint only says that a pass blocked on *other* uploads
  is not a stall. It should also say what to do when the blocked line names *this* upload (for
  example, run the custodian with `--endpoints`). Same as T4's `cli.rs:3371` item. This only
  happens after damage, so it is low severity.
- `crates/server/src/cli.rs:3268` (T4): the printed FDB command leaves out
  `WYRD_FDB_EXTERNAL_CLIENT_DIR`. A follow-up run without it fails at connect (the version check),
  so it fails safe and never acts on the wrong store. It is reasonable to fold this into the
  `:3276` fix, or to record it as rejected with that reason.
- My read on T4's DST and test-gap items: none of them hides a defect in the fix.
  - `custodian.rs:8839`: the `late` count can include a commit that applied before the cut. True,
    but the "lands after the operator gave up" property is shown separately by the `InFlight` row
    (`:8989-9007`), which lands fences only after `operator_abort` has returned.
  - `:9308`: accepting `RoundsSpent` is dead tolerance here (`ABORT_ROUNDS` is 4, and the Complete
    moves the session at most twice after the abort starts). A "never re-reads" mutant would fail
    the sweep's "fenced after losing a round" check (`:9422-9424`), and leg C catches a wrong
    answer on `Completed`.
  - `:9320`: segments cannot be deleted without an obligation naming them, and the row checks
    there is none.
  - `:8959`: `operator_abort` works on any store and FDB does emit 1021, so this is a fidelity nit
    on the TiKV model.
  - `multipart_operator_abort.rs:267`: a surviving grandchild process only matters on a timeout,
    when the test has already failed.

  Recording these as rejected, with these reasons, is defensible.
- The C4 diff-coverage failure ("patch.diff does not apply on origin/main",
  `gate-logs/C4-diff-cov.log`) is a harness fault: the gate used `origin/main` rather than the
  stacked base. It is no evidence against the fix; coverage is simply unmeasured.
- The `check-gates.json` claims I checked hold: C4-verify (6 ran red, all 6 green here), C5 (0
  missed), C4-ci green. The overall `fail` comes from T4 alone (gating), and the items above are
  my triage of it.

<!-- pdca:leaf-complete -->
