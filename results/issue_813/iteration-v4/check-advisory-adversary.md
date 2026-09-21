# Adversarial review — issue 813 (staged-scrub-and-keep), round 4

**Verdict: I could not refute the production fix.** I found two test gaps the builder can close,
and one gate result a human should confirm. None of them is a production correctness bug, so the
brief's stop rule (a new production bug means `iterate-plan`) is not triggered.

What I re-ran myself, on a scratch copy of the patched tree (since deleted):

- Patched tree: `staged_scrub` 16/16 and `staged_protection` 36/36 green.
- By-hand red for G, G-held, H and I, done the way the brief describes: base production code, the
  patched `staged_protection.rs` with the two new `ReconstructionContext` field lines removed. 8 of
  36 failed by assertion: G ×2, G-held ×2, H, I ×2 and F. J, the empty-queue leg and the no-class
  control stayed green, which is what the brief says should happen.
- Every mutation the brief names turns its leg red: dropping `.chain(staged.held…)`
  (`crates/custodian/src/reconstruction.rs:244`) fails both G-held legs; reading the staged class
  in the empty-queue branch (`:214`) fails the empty-queue leg; dropping staged chunks from
  `drain_only` (`:308`) fails J at `crates/custodian/tests/staged_protection.rs:2906`; dropping the
  `referenced.malformed` half (`crates/custodian/src/scrub.rs:237`) fails A′-malformed.
- Two extra mutations were also caught: taking `staged_kept` out of `hole` fails 5 legs, and taking
  `!staged_incomplete` out of the drain gate fails leg I.

## Findings

- NEEDS-HUMAN [impl] — **No test covers scrub's staged reader past the first page of upload
  sessions.** `staged_committed_parts` has its own copy of the `mpu:` session paging loop
  (`crates/custodian/src/gc.rs:1600-1618`). Only the inner `walk_staged_range` is shared with GC.
  `gc.rs:1616` (`after = Some(last)`) is a MISS in `gate-logs/C4-diff-cov.log`. I changed that arm
  to return after page 1, and the **whole `wyrd-custodian` test suite still passed** (0 failures).
  Concrete case: with 513 or more sessions (`STAGED_PAGE = 512`, `gc.rs:300`), a rotten fragment in
  the 513th session's part would be skipped and scrub would answer `Satisfied` if this loop ever
  regressed. Today's code is correct; only the guard is missing. The comment at
  `crates/custodian/tests/staged_scrub.rs:111-112` ("the paged-range legs are GC's, over the same
  shared helpers") is only half true, because the session loop is not shared. Fix: add a scrub leg
  with more sessions than one page, or a lowered scan cap, like GC's (D) leg at
  `staged_protection.rs:1534`.
- NEEDS-HUMAN [impl] — **No test pins scrub's promise to name staged damage before the committed
  read.** `crates/custodian/src/scrub.rs:147-163` says the malformed and unreadable part records are
  named "before the committed read's own `?` can carry the names away". I moved both emit loops
  (`:152`, `:161`) to after `referenced_fragments` (`:174`). All five suites that touch this code
  still passed (`staged_scrub`, `staged_protection`, `segmented_map_reconstruction`,
  `reconstruction`, `scrub`). Reconstruction has a test for its version of this promise
  (`an_unreadable_staged_record_is_named_even_when_the_committed_read_then_faults`,
  `staged_protection.rs:2715`); scrub has none. Concrete case: an undecodable `part:` record plus a
  store fault on `inode:`. The pass returns `Err`, and with the emits moved the record is never
  named. Fix: add the scrub version of that test in `staged_scrub.rs`.
- NEEDS-HUMAN [human] — **The T4 batched-review pass may be stale or short-circuited.**
  `check-gates.json` reports `T4-batch-review` as "0 blocking, 0 recorded-rejected, 0
  noise-dropped" after 33.15 s, for three review passes over a 218 KB diff
  (`gate-logs/T4-batch-review.log`). That is quick for three fresh passes. The brief also says
  finding (a) (fleet-absent D server; the fleet walk is at `scrub.rs:255`) must be recorded in
  `review-rejected.md`, and the gate shows no recorded rejection. Either this round's review did not
  raise (a) again, or the rejection was not recorded. From the inputs I have, I can't tell which. A
  human should check that `results/issue_813/review-batch.md` was produced from this patch.
- **C4-verify overstates its own count** (this is about the gate's report, not the fix). The
  `path_line` says "16 test(s) ran red", but `gate-logs/C4-verify.log` shows 11 failed and 5 passed
  with production reverted. The 5 that pass on base are B, the intact control, both A′ legs and
  A′-malformed. The brief expects exactly those to pass on base, so the red→green proof holds. The
  summary line is just wrong.
- **The diff-coverage figure mostly measures scrub, not reconstruction.** C4-diff-cov ran only
  `--test staged_scrub` for `wyrd-custodian`, so `reconstruction.rs:226-227` and `:1238-1247` show
  as MISS even though leg I runs them in CI. The 94.5% says little about the reconstruction half.
  For that half, the evidence is the by-hand red above.

## Refutation attempts that failed

- **Publication race, both hook timings (scrub and reconstruction):** the source-first read order
  holds, and the DST property 13 reconstruction driver calls the production `reconcile_step`, with
  a control obligation that must drain. The test is not a tautology.
- **A committed map and a part record name the same chunk (valid, malformed or unreadable
  committed map):** the committed rule decides every time (`scrub.rs:236-243`,
  `reconstruction.rs` `assess`). An unreadable committed object makes the pass `Blocked` whatever
  the part record says.
- **J-discharge over-reach:** a whole committed chunk's duplicate obligation drains with the pass
  `Satisfied`, and the mutation named in the brief is caught.
- **Aborting sessions:** their part fragments are still checked and their obligations still kept
  until retirement. That is bounded, and it matches GC's "whatever state the upload is in".
- **Clock seam (ADR-0009):** `LoopClock` (`crates/server/src/custodian.rs:146-169`, struct at `:146`, `Clock` impl at `:165`) is the only
  time source for both a pass's `now_millis` and `ReconstructionContext::clock`. No lock is held
  across an `.await`, and the context field is not read yet.
- **A trade-off, not a bug:** a store fault on the staged ranges now fails the whole reconstruction
  pass (`reconstruction.rs:217`, `?`). Before this patch, committed repairs were not affected by
  such a fault. GC already behaves this way, the brief requires it, and the fault would usually hit
  the `inode:` read in the same pass too. I don't count it as a refutation.
