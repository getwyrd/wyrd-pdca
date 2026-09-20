# Adversarial review — issue 637 (staged-byte protection)

What I checked: patch.diff against the target at `$PDCA_TARGET` (the patch is applied there). I re-read
the red/green log, ran three of the surviving C5 mutants against `cargo test -p wyrd-custodian` in a
scratch copy (all three survived), and wrote one probe test, in scratch only. All scratch work has been
deleted.

## Findings

- NEEDS-HUMAN [human] — **The stale-mark cleanup can leave a deleted object's bytes with no way to be
  reclaimed.** `crates/custodian/src/ledger.rs:363-381` drops any lapsed legacy mark on a fragment that
  `definitely_references` (`crates/custodian/src/gc.rs:356-358`) says is named. That check counts a
  `part:` record from a session in *any* state (`crates/custodian/src/staged.rs:19-24`). The failing case,
  reproduced with a scratch test on the patched tree:
  1. An upload publishes. Its `part:` record still exists, waiting for the records-mode
     `retire:records:{parts}` drain, which by design writes no marks.
  2. The object is deleted by `metadata::unlink`, which writes legacy marks
     (`crates/core/src/metadata.rs:1881-1899`).
  3. A GC pass after the grace window drops all three marks as "stale" (0 left).
  4. The part record then retires.

  After four more GC passes, all 3 fragments are still on disk with no mark and no reference. The test
  file's own classification sweep (`Rig::unclassified`, `crates/custodian/tests/staged_protection.rs:448`)
  flags all 3 as gaps. That breaks the brief's invariant ("garbage-with-a-sound-reclamation-path"). The
  base tree does reclaim these bytes. This can't happen until #508 publishes uploads, but the rule that
  causes it lands in this diff. A human needs to pick the fix. One option is to re-stamp instead of
  dropping when only a staged record names the fragment (0016:1254-1255 allows "re-stamps or drops").
  Another is to drop only against committed references.

- NEEDS-HUMAN [impl] — **No test covers the path where GC's reclaim-intent CAS loses.** At
  `crates/custodian/src/ledger.rs:483-497`, when the batched intent CAS conflicts, the per-mark
  fallback is the only code that decides *not* to delete a fragment whose mark changed under the pass.
  Changing `== CommitOutcome::Committed` to `!=` at `:492` deletes exactly those fragments. That mutant
  passes the whole custodian suite (I confirmed it; C5 lists it too). Leg I's store double fails commits
  with `Err`, never `Conflict` (`staged_protection.rs:180-184`), so the loop never runs, and no DST case
  rewrites a mark GC has already read. Add a case where one mark in the batch changes between the walk
  and the intent commit, and assert its fragment survives.

- NEEDS-HUMAN [impl] — **Two safety checks in the staged re-place have no tests.**
  1. `crates/custodian/src/reconstruction.rs:1148-1162` must re-stamp a destination mark that has a
     different identity. If the check at `:1152` is inverted, the old mark is reused as the pre-mark with
     its old stamp. That mutant passes every custodian test (I confirmed it). The failing case it hides:
     P_new has an old, empty mark. The mutant reuses it, and the custodian dies after issuing the put.
     The next leader's GC sweeps the empty mark at once, because it is already past the 45 s deadline. The
     delayed write (deadline now+40 s) then lands with no reference and no evidence. No test seeds any
     mark at the destination.
  2. The write deadline at `:1179-1188` is never checked, and it is what makes the 45 s sweep safe. Both
     D-server doubles ignore it (`staged_protection.rs:242-248`, `crates/dst/tests/custodian.rs:282-290`),
     so the `+`→`-`/`*` mutants at `:1179` survive and the `is_write_deadline_expired` → `Aborted` branch
     never runs.

- NEEDS-HUMAN [impl] — **No test checks that a drain can still reach `Satisfied` while uploads live on
  other servers.** At `crates/custodian/src/staged.rs:95-100`, changing the check to `*server != dserver`
  makes every drain `Pending` whenever any staged byte exists anywhere. All 20 legs still pass (I
  confirmed it). Legs B and F only ever assert `Pending` (`desired_state.rs:201-205`). Add a case with
  staged fragments on servers 0–2 and a drained server 3 that holds none, and assert `Satisfied`.

- NEEDS-HUMAN [impl] — **Legs G and H2(c) don't pin the numbers they claim to check.**
  - G asserts `read < ledger` plus a bound derived from the pass's own measured read
    (`staged_protection.rs:1379-1396`). Any per-pass budget under 3,020 marks passes, and the `<`→`<=`
    mutant at `ledger.rs:233` survives.
  - H2(c) only checks at 10 s and 50 s (`staged_protection.rs:1704-1724`), so any late-write deadline in
    (10 s, 50 s] passes. Dropping `δ_clock` (`ledger.rs:106-107`, 45 s → 35 s) survives.
  - The unit test at `ledger.rs:667-671` compares against a hard-coded 60 000, not the deployed
    `GC_GRACE_WINDOW_MILLIS` (`crates/server/src/custodian.rs:114`).

- NEEDS-HUMAN [human] — **19 tests went red, not 20, and the second half of leg F was replaced by a test
  that already passes on the base.** The C4-verify row in `check-gates.json` says "20 test(s) ran red",
  but `gate-logs/C4-verify.log` shows `1 passed; 19 failed`. The test that passes on the base is
  `f_a_committed_segmented_fragment_on_a_draining_server_is_held_and_never_dropped`
  (`staged_protection.rs:1263`). The brief asked for a committed segmented object's fragments to be
  **evacuated**. Rebalance still refuses them (`crates/custodian/src/rebalance.rs:642`, an existing
  `deferred: #682`), and the test asserts that refusal. If the human accepts the #682 deferral, this is
  settled, but the gate text still overstates the evidence.

- NEEDS-HUMAN [human] — **X57 isn't implemented: a fenced `Completing` session's `seg:` records get no
  deleter.** `crates/custodian/src/restore.rs:716-722` and `:798-801` install only
  `retire:bytes:{session, parts}`. The session is listed in `segments_unretired` because its record
  doesn't carry the segment-group nonce. The brief (leg C and Open question 2) requires
  `retire:records:{seg:<g>:<E>}` in the same batch. Leg C asserts the report line instead
  (`staged_protection.rs:889-900`). This is the §6 item the brief predicted.

- NEEDS-HUMAN [human] — **Three legs prove less than their names say.**
  - (a) "A retried Complete is refused" (`staged_protection.rs:953-965`) is a hand-built CAS on the
    session bytes from before the fence. Any write to that record would make it conflict, and no Complete
    path exists until #508. The real check is the `Aborting` state asserted above it.
  - (b) In J(i), the drain fence comes from the test's own batch (`crates/dst/tests/custodian.rs:2531-2560`,
    "emulated since the production writer is #657's"). Only `reconciliation_status` there is production
    code, so X59 isn't shown for any production writer.
  - (c) The I2 classification sweep is written twice, inside the tests (`staged_protection.rs:438-512`,
    `crates/dst/tests/custodian.rs:2424`), not as a shared helper. The brief calls that a §6 item.

## What I tried and could not break

- **Read order.** `staged.rs:134-201` reads `sidx:` before `part:` for each session. `gc.rs:414-415`
  builds the staged set before scanning `inode:`. Reconstruction builds the staged set before its
  committed reading (`reconstruction.rs:190-191`). I found no interleaving that hides a chunk from both
  classes.
- **Intent before delete, and resume.** The intent commits before `delete_fragment`
  (`ledger.rs:475-505`). A `reclaiming` mark is re-checked against the reference set before it resumes
  (`ledger.rs:352-360`). A destination holding a `reclaiming` mark aborts the re-place
  (`reconstruction.rs:1150`). The deployed passes run one after another (`crates/server/src/custodian.rs`,
  "the passes run sequentially"), so GC and a re-place in the same leader never overlap.
- **Restore marks the fence generation complete before its mark pass** (`restore.rs:370-381`, which runs
  before the reference build at `:397` and the fleet listing at `:455`). If a gateway starts accepting
  uploads the moment it reads "complete", this same pass could mark a new upload's fragments. GC's
  staged check (`gc.rs:215`, `ledger.rs:363`) still protects them, so I found no loss path, only a stale
  mark and an inflated `stranded_marked`.
- **Shape of the red leg.** All 19 reds are assertion panics, not build errors, and `reconcile_step` is
  still called with 7 arguments (`staged_protection.rs:358`). The DST cases ran in C4-ci
  (`gate-logs/C4-ci.log:3435-3450`).
- **Surviving C5 mutants in `crates/core`** (`orphan.rs:222`, `multipart.rs:3772`) look like a tool
  artifact: cargo-mutants runs only the owning crate's tests, and the callers of those methods are in
  `wyrd-custodian`. The same goes for `restore.rs:261` (`needs_human`), which the restore-verdict test in
  `crates/server/src/cli.rs:2988` covers.
