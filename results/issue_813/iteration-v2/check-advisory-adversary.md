# Adversarial review — issue #813 (663.1: staged scrub + keep)

Verdict: the red→green evidence holds up. I re-ran it and it still stands. I found no input
that makes scrub or reconstruction lose a staged obligation. Two findings are about what the
patch sets up for #814 and for the window after publication, and one is about a gate result
that looks too fast to trust.

## Evidence I re-ran (in a scratch copy of `$PDCA_TARGET`)

- Green: `cargo test -p wyrd-custodian --test staged_protection --test staged_scrub` → 32 + 11 passed.
- Red for legs D–F, which C4-verify never proves (brief.md:64-72). I put back base `gc.rs`,
  `reconstruction.rs` and `scrub.rs`, kept the new tests, and removed the two new field
  initialisers at `crates/custodian/tests/staged_protection.rs:518-519` →
  **6 failed / 26 passed**, every one by assertion (`:2349`, `:2400`, `:2504`, `:2559`,
  `:2620`, and rewritten leg F at `:2265`). None failed to compile, and none failed for an
  unrelated reason.
- Read-order mutant (the round-1 defect class). In the patched code I moved the committed read
  ahead of the staged read in `crates/custodian/src/reconstruction.rs:210/221` and
  `crates/custodian/src/scrub.rs:123/152`. Caught by: `staged_protection.rs:2506` (leg H),
  `staged_protection.rs:2622` (name-before-fault leg), `staged_scrub.rs:922` (flip+drain
  between reads), and the DST sweep leg at gaps `[0,0,0]` (`crates/dst/tests/custodian.rs:3046`).
  The seeded DST leg misses it on one random seed but catches it under `MADSIM_TEST_NUM=50`
  (gaps `[17,10,1]`), which is the sweep the CI gate runs. The eight `REGRESSION_SEEDS` do not
  catch it. That is fine because the sweep does, but no committed seed pins this bug yet.
- Round-1 carry-forward. All four items are fixed, and each is guarded by a test that goes red
  on base: part-before-inode order in scrub, exact-length staged placement
  (`crates/custodian/src/gc.rs:1446-1464`, empty placement → malformed, no fake repair),
  staged names emitted before `read_committed` can fail (`reconstruction.rs:218-221`), and
  seeded DST coverage for reconstruction.

## Findings

- NEEDS-HUMAN [impl] — `crates/server/src/custodian.rs:501-505` hard-codes
  `wyrd_testkit::SystemClock` for `ReconstructionContext::clock`. Its comment says this is
  "the same clock this loop's own `clock` closure already advances". That is only true when the
  caller passes wall time. `crates/server/tests/custodian_day_one.rs:1174`, `:1234` and `:1344`
  drive this same loop with `|| 500`. In those runs the pass's `now_millis` is 500 while
  `ctx.clock.now_millis()` is about 1.79e12. The brief asked the deployed loop to pass "the
  clock it already advances" (brief.md:104-105). Nothing reads the field yet. But #814 will
  read `ctx.clock` to check a staged re-place's write window against a pre-mark stamped from
  the pass's `now_millis`. That mixes a manual clock and the wall clock in one lifecycle, which
  is the #557/#565 class the rubric's first MUST forbids. Fix: keep one
  `wyrd_testkit::ManualClock` in the loop, `set(clock())` once per reconstruction pass, and pass
  that same reading as the pass's `now_millis`, so the field and the argument come from one
  source. The test and DST sites that pair `SystemClock` with a fixed `NOW`/`HANDOFF_NOW` (for
  example `crates/custodian/tests/staged_protection.rs:518` with `now: u64` at `:508`) have the
  same mismatch and should follow the same pattern before #814 builds on them.

- NEEDS-HUMAN [human] — after publication, scrub still checks the `part:` record's placement
  even when it no longer matches the committed one, and scrub and reconstruction then keep
  undoing each other. `crates/custodian/src/scrub.rs:199-200` only de-duplicates identical
  `(dserver, fragment)` keys. Concrete case, following the path this patch creates:
  1. Chunk C (RS k=2, m=1) loses fragment 0 on server 3 while staged. Scrub enqueues it, and
     reconstruction keeps the obligation (`reconstruction.rs:727`).
  2. The upload publishes, and the inode copies placement `[3,1,2]`.
  3. Reconstruction now takes the committed path, rebuilds fragment 0 on server 0, and moves
     the inode to `[0,1,2]`.
  4. The `part:` record still says `[3,1,2]` until the retirement drain deletes it.
  5. Every later scrub fetches `(3,(C,0))`, gets `Ok(None)`, enqueues C, reports `emit_missing`
     and answers `Changed`.
  6. Every reconstruction pass finds the committed chunk whole and drains it
     (`reconstruction.rs:827-829`).
  So scrub never answers `Satisfied` and keeps raising false "missing fragment" signals for a
  location nothing reads. `crates/custodian/src` has no `retire:records` drain yet, so this
  window has no bound in the current tree (though no multipart writer exists yet either). One
  option: skip a staged placement for any chunk the committed reading also names. Once
  published, the committed placement is the one reads use, and the staged-first read order
  still sees each chunk in at least one class. This departs from the brief's literal "every
  fragment a committed `part:` record places" (brief.md:95), so it is a scope call.

- NEEDS-HUMAN [human] — `check-gates.json` row T4-batch-review
  (`gate-logs/T4-batch-review.log:10`) reports "0 blocking" after 30.48 s for three codex
  passes over a 175 KB patch. The same gate found 9 blocking items in the previous round
  (brief.md:159). The log is one summary line with no evidence from the individual passes.
  Before counting this gating row as green, open `results/issue_813/review-batch.md` and
  confirm all three passes actually ran to completion rather than erroring out or returning an
  empty or cached result.

## Tried to refute, could not

- The drain gate while a staged record is unreadable (`reconstruction.rs:553` in the patch)
  withholds every drain and certifies nothing. Red on base, green on the fix.
- A kept staged obligation stays off the repair-backlog gauge: `Assessment::Staged` never
  increments `under_replicated`.
- `W_write` has one definition: `crates/custodian/src/gc.rs:201` is reused at
  `crates/server/src/custodian.rs:132`.
- Scrub never reads `sidx:`. Rewritten leg F checks the read log across every damaged and
  faulted fixture.
- The `StagedSet::place` refactor (`gc.rs:1410-1432`) behaves the same as before, and GC and
  restore legs A–E pass unedited.
- The C4-diff-cov misses (`reconstruction.rs:219-220`, `:1229-1238`, `gc.rs:1425-1428`) come
  from a tool artifact: that gate only measured `--test staged_scrub`
  (`gate-logs/C4-diff-cov.log:14`), and the `staged_protection` legs I ran do execute those
  lines. The one path really left untested is session-listing pagination in the new
  `staged_committed_parts` (`gc.rs:1611-1613`), which is line-for-line the same as the loop in
  `staged_fragments`. That is not a refutation.
