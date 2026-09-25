# Adversarial review — issue 813, round 5 (staged-scrub-and-keep)

**Verdict: I tried to refute this and could not.** Round 5 adds two tests and changes no
production code. Both tests catch the defect they claim to pin. Every round-4 mutation the brief
names is still caught. None of the production inputs I built against scrub or reconstruction broke
the fix. Toolchain was present: I rebuilt the patched tree in a scratch copy, ran
`staged_scrub` (18/18) and `staged_protection` (36/36) green, then ran each mutation below
against it and put the file back each time.

## The evidence (red→green)

- **Weak spot in the gate evidence, then closed by hand.** Both round-5 tests go red on base
  only because base scrub reads no `part:` record at all (`gate-logs/C4-verify.log`). Any test
  that needs scrub to read a part record fails there, so C4-verify alone does not show that
  these two tests pin *paging* or *emit-before-the-committed-read*. I ran the mutations that
  would:
  - **A-paged.** Make scrub's own session loop check only the first page's sessions while
    still listing every page (`crates/custodian/src/gc.rs:1602`, add
    `if after.is_some() { break; }`). Result: `scrub_checks_committed_parts_of_sessions_past_the_first_page`
    goes red at `crates/custodian/tests/staged_scrub.rs:1027`. The lost chunks sit at session 0,
    512 and 1024 (the first session of each page, and the only one on page 3), so skipping the
    first or last session of a later page would also be caught. The page-count assertion
    (`staged_scrub.rs:1035-1041`) fails loudly if `STAGED_PAGE` (`gc.rs:300`) changes in either
    direction.
  - **C-order.** Move both staged emit loops (`crates/custodian/src/scrub.rs:152-163`) to after
    `referenced_fragments` (`scrub.rs:174`). Result: red at `staged_scrub.rs:969`. Moving only
    the unreadable-record loop turns it red at `staged_scrub.rs:978`, so each record is pinned
    on its own. I found no route by which the test passes for the wrong reason: if the fault hit
    the staged read instead, nothing would be emitted and the test would fail.
- **The round-4 mutations the brief names, re-run on this tree. All were caught:**
  - dropping `.chain(staged.held…)` (`crates/custodian/src/reconstruction.rs:244`) turns both
    G-held legs red;
  - dropping the `referenced.malformed` half of the supersede check (`scrub.rs:237`) turns
    A′-malformed red;
  - calling `staged_fragments` in the empty-queue branch (`reconstruction.rs:215`) turns the
    empty-queue leg red;
  - keeping every `Drain` whose chunk a staged record names (`reconstruction.rs:308`) turns
    J-discharge red at `crates/custodian/tests/staged_protection.rs:2900`.
- **Extra mutations, also caught:**
  - removing `!staged_incomplete` from the drain gate (`reconstruction.rs:436`) turns
    `an_unreadable_staged_record_holds_back_every_drain` red;
  - neutering `staged_kept` in the certification check (`reconstruction.rs:451`) turns 5 G/H
    legs red;
  - removing `!staged.unresolvable.is_empty()` from scrub's answer (`scrub.rs:326`) turns C and
    C‴ red.
- **A misstated gate claim, verdict unaffected.** The C4-verify row in `check-gates.json` says
  "18 test(s) ran red". The log shows 13 failed and 5 passed on the red leg. The 5 are B, the
  intact control, the two A′ legs and A′-malformed, all green on base by design as the brief
  says. The PASS stands, but the count in the summary text is wrong. This is the harness's
  wording, not something the patch can fix.
- **Diff-coverage misses are a gap in what the gate measures.** The gate reports misses at
  `reconstruction.rs:226-227` and `:1238-1247`. It measures the custodian crate only through
  `--test staged_scrub` (`gate-logs/C4-diff-cov.log:14`). Leg I
  (`staged_protection.rs:2715`) runs those lines, and the drain-gate mutation above shows the
  path is pinned.

## The fix: inputs I tried that did not break it

- **Zero-fragment scheme.** A part record with `ReedSolomon{k:0,m:0}` and `placement: []` would
  pass `staged_placement`'s exact-length check (0 == 0). It would put no fragments in the set and
  mark nothing malformed, so scrub would certify a chunk it never checked. Blocked earlier: the
  decoder rejects it (`checked_chunk_scheme`, `crates/core/src/multipart.rs:2359-2369`), so the
  record becomes unresolvable and the pass answers `Blocked`.
- **A multipart upload published as a segmented object.** `referenced_fragments` resolves
  `seg:` chunks into `schemes` (`gc.rs:1234-1273`), so scrub's "committed map wins" check
  (`scrub.rs:237`) covers them. Reconstruction returns `Refused` for them before it looks at the
  staged set.
- **A part re-commit, an abort, or a retirement landing mid-pass.** Each one yields at worst a
  spurious enqueue, which the next reconstruction pass drains once no record names the chunk.
  Base already behaves this way when a committed object is deleted mid-scrub. Nothing new.
- **The same chunk under two part records with different schemes.** `placed` keeps one scheme
  per `(dserver, fragment)`. That is the same one-scheme-per-chunk shape as
  `ReferenceSet::schemes` on base, and a mismatch fails safe by enqueueing a repair.
- **Settled, not re-raised:** fragments on servers outside `ctx.fleet` (review finding (a),
  rejected by the human); the malformed-committed-placement asymmetry; the re-queue bound after
  delete-before-retire; one unreadable staged record stopping every drain. The brief states all
  four as accepted decisions.

## Where the reviewer might have rationalized

- The T4 batch review took 34 s and returned 0 findings over a patch of about 225 KB
  (`gate-logs/T4-batch-review.log`). That is thin for three multi-pass reviews. The human left
  the T4 staleness question open on purpose (brief, carry-forward), so I note it here and do not
  raise it again as a NEEDS-HUMAN item. My own mutations back up the test-strength claims that a
  review would have had to take on trust.

No NEEDS-HUMAN items from this pass.
