# Build notes — issue 1006 (743.1b): the `required` rows of the capability matrix, attempt 2

## Base

- Built on `635e0bd` (`pdca-integrate: issue_955`), the supplied `pdca-integration/main-rdfeb36496b2d`
  stack base. `origin/pdca-integration/main-rdfeb36496b2d` is at the same commit. It holds child-1
  (`5d2c679`, issue_1005).
- The brief's first step passed: `git merge-base --is-ancestor 9b1a3e2 HEAD` returned 0, and
  `crates/validate/src/matrix.rs` exists.
- `Cargo.lock`, `deny*.toml`, `crates/validate/src/lib.rs`, every doc and child-1's test file are
  untouched. No public type, variant, field or function was added.

## What this attempt changed, against each carry-forward item

Line numbers are on the patched tree.

1. **C5: a clean 404 no longer erases a lost or cut-off follow-up.**
   - `judge_follow_ups` (`crates/validate/src/matrix.rs:1391-1406`) keeps everything each follow-up
     found. `[a → no answer, a → 404]`, `[a → 404, a → no answer]` and `[a → cut-off, a → 404]` are
     now unsettled on both delete rows, and INCONCLUSIVE in a full run. That is what brief (C) says
     (`brief.md:73`) and what (B) step 5 requires.
   - The test that endorsed the old rule is gone. `a_clean_404_never_erases_a_lost_or_cut_off_follow_up`
     replaces it and also asserts the full-run verdict. `a_second_follow_up_never_undoes_a_fatal_one`
     keeps the fatal half and adds lost-probe and cut-before-fatal legs.
   - Running the new tests against the old rule: 41 passed, and that one test failed.
2. **Status checks that no leg pinned.** New test
   `a_defined_answer_under_only_a_wrong_status_is_protocol_on_every_row` sends each row's defined
   answer with only the status changed, and asserts the exact "answered X; the row defines Y"
   message. It includes the three legs the adversary asked for: a ranged 200 with the right
   `Content-Range` and bytes, `DeleteObjects` 204 with a correct document, and a listing 201 with
   the right keys.
3. **The four other changes no test noticed.**
   - `is_strong_etag(sent)` on the matching form: new test
     `a_matching_tag_that_is_not_strong_cannot_test_the_matching_row` uses `sent = etag`, not strong.
   - The two sorts: new tests `the_written_keys_may_be_recorded_in_any_order` and
     `the_requested_keys_may_be_recorded_in_any_order` (unsorted `expected`, `keys = [b, a]`,
     `keys = [a, b, a]`).
   - An unfit conditional GET answered 206: new legs in
     `a_conditional_get_with_an_unfit_tag_never_passes_and_is_held_to_what_it_got`.
4. **Stale citation.** `delete_result`'s doc (`matrix.rs:1357-1358`) now cites
   `crates/gateway-s3/src/lib.rs:2396-2397` and `:2548`. I opened both and they say what it claims.

Two simplifications came out of my own trial code changes, each removing a check that could not
change any result:

- Once every follow-up's findings stay on the record, a requested key with a non-proving follow-up
  already has a finding. So the `proved` list did nothing beyond "was this key asked about at all?",
  and `judge_follow_ups` now asks that directly (28 lines down to 16).
- `!s.is_empty()` in `parse_content_range`: `"".parse::<u64>()` already fails.

The `599` legs were added because the `..=599` edge of the follow-up's 5xx check had no test.

## The failing gate: C4 diff coverage — not fixable from Do, please decide

The row failed because `run-verify.sh --print-base` resolved to `origin/main`, and `origin/main`
(`af99ea5`) has no `crates/validate/src/matrix.rs`: child-1 (#1005, PR #1031) has not merged. So the
message "the bundle is stale; rebase Do" is wrong for this bundle. The brief forbids Do from rebasing
or merging `main` to get past a base problem, and there is nothing to rebase onto that has child-1
except the stack base this patch already matches.

I ran `--print-base` by hand with no `PDCA_VERIFY_BASE` set and got `origin/main`. Whether the driver
sets that variable for the diff-coverage gate I did not establish, so this may be a harness gap for
stacked bundles (the bundle's `stack-base` file names the right base). If so it belongs upstream in
`eduralph/pdca-harness`. I measured coverage myself on the stack base instead (see "Commands run"), which is not the gate's own figure, so the row may still read `fail` at Check.

## Why this shape

The two iteration-9 defects in #937 came from checks that returned early with "unsettled" before a
fatal check could run. Child-1 had already built the fix for that: stages plus `settle`. So no check
here returns a result. Each one only adds a finding, and a cut or an unfit ask is just one more
`Unproved` finding, which cannot stop a later check.

Status findings do not gate the later checks. For example `object_bytes` also runs on a 404. This
cannot produce a wrong class, because the status stage comes first and its fatal finding wins. The
only fatal stage earlier than status is `PutObject`'s request-side fact. Leaving out the gates means
there is no branch for a future edit to get wrong.

The one place that switches on the status is `conditional`, because (G) makes a 200 and a 304 read
different checks. Its `_ => {}` arm cannot hide anything: any other status is already fatal.

Taken from the peer (`results/issue_937/iteration-v9/patch.diff:1240-1266`): `parse_content_range`,
and the difference-offset report. `is_strong_etag` was already in child-1.

## Decisions for the human at sign-off

1. **Retries: this attempt takes the strict reading, and that settles a deferred item one way.**
   The adversary filed the retry rule as a human decision (`deferred-findings.json`, first item),
   while the reviewer filed it as an implementation defect and the carry-forward routed it here. I
   followed the brief's words: a lost or cut-off follow-up is unsettled, with no exception.
   - The cost falls on #939: a runner that retries a timed-out `HEAD` gets INCONCLUSIVE even when the
     retry answers 404, unless it records only the last attempt. Child-1's type doc still says "a key
     may be probed more than once (a retry)" (`matrix.rs:704-706`). That remains true, because two
     clean answers pass.
   - To restore forgiveness instead: about 6 lines in `judge_follow_ups`, plus flipping one test. It
     needs an explicit change to the brief.

2. **A `DeleteObjects` of no keys still judges its follow-ups** (`deferred-findings.json`, second
   item; unchanged). (G) says "only the two facts" and I read that as covering the exercise's own
   answer. Every follow-up of a no-keys delete is of a key it did not request, so (C)'s rule applies:
   a 5xx or a byte on it is `protocol`, anything else stays unsettled. The other reading would let an
   unfit ask hide a 5xx. Shown in `a_delete_of_no_keys_reads_only_the_two_facts_of_its_own_answer`.

3. **The 404 fact applies only on `required` rows.** It sits in `required`, not `shared_facts`. The
   brief keeps the `unsupported` and `supported, not checked` checks unchanged, and no `pre-0.1` read
   row is in those states. A future profile that makes a read `unsupported` would judge its 404 by the
   declared refusal.

4. **`Exercise::Other` is never a typed read,** so a 404 to `Other { GetObject }` stays unsettled. The
   brief defines a typed read by the exercise's form, and child-1's test expects this at
   `capability_matrix.rs:587-595`.

5. **`Content-Range` is read by value:** `BYTES 2-5/16` and `bytes 02-05/016` pass. The brief says
   "exactly", but RFC 9110 §14.1 makes the unit case-insensitive and §14.4 allows `1*DIGIT`.
   `AGENTS.md`'s grammar rule asks for case-insensitive units, and the brief reports no defect in this
   grammar. Signs, spaces, `*` and overflow are all refused.

6. **A cut after every expected byte is unsettled,** not a pass, because the end of the body was never
   seen. This is the iteration-9 behaviour, and (C) lists a cut "after a correct prefix".

The other two deferred items (the T4 review evidence, and confirming the base before publishing) are
not build work and I did not touch them.

## Ruled out

- **Patching iteration 9's `check_required` in place.** The brief rules this out: its
  `Result`-with-early-return shape caused the defects. Its 270 lines (`patch.diff:907-1266`) would have
  needed every `?` and `return Err` removed anyway.
- **A shared helper for `refusal`'s no-body arms and `no_body`.** They overlap by about 10 lines, but
  `refusal` is child-1's and the brief says not to change it. Worth a later clean-up.
- **A new parser against an existing one.** I looked for a shared `Content-Range` parser, as the rubric
  asks. Neither `crates/validate/src/s3/` nor any other crate has one: `verbs.rs:61` only mentions the
  header. The brief also names `parse_content_range` as in scope.

## Finding against child-1 (not fixed here, per the brief)

`PRE_0_1_ROWS`'s `cites` strings (`matrix.rs:284-337`) carry gateway line numbers from `9b1a3e2` that no
longer match this target. For example `put_object_response :3401-3404` is now at
`crates/gateway-s3/src/lib.rs:3654`, and `errors :2293-2300` at `:2396-2397` / `:2548`. The gateway file
changed by 853 lines between those commits. The profile is child-1's and out of scope, so I left it.

No fact was missing from child-1's type.

## Refuting my own test

**(a) Genuine red? Yes.** I reverted `crates/validate/src/matrix.rs` to `HEAD` and ran
`cargo test -p wyrd-validate --test capability_matrix_required --test capability_matrix`. It compiled.
The required file gave `3 passed; 39 failed`, including the first leg. Child-1's file gave `30 passed`.
With the fix restored: `42 passed; 0 failed` and `30 passed`.

The three that pass on the base assert behaviour child-1 already has, and pin it here, so they have no
red→green of their own: `a_document_cut_before_it_could_be_read_is_unsettled`,
`an_exercise_with_no_check_of_its_own_is_unsettled_on_a_required_row`,
`no_response_head_is_no_verdict_and_its_follow_ups_are_not_judged`.

I also made 88 distinct trial code changes by hand, covering what `cargo mutants` does not:

- 60 were caught in the first run of 79 and 16 failed to compile (the crate denies warnings). I rewrote
  those 16 so they compile; all 16 were then caught.
- 3 got through the first run. One showed the `proved` filter did nothing, and one that `!s.is_empty()`
  did nothing: both are removed (above). The third was the `..=599` edge, now tested and caught.
- 9 more against the final code: 7 caught, and 2 that got through do not change any class:
  - Moving an out-of-range ask's `Unproved` finding from the `request` stage to `open` changes only
    which reason an unsettled result reports.
  - Skipping the `<Deleted>` match when an `<Error>` row is present changes only the message, because
    both are `protocol` and the `<Error>` row's finding comes first.

**(b) Production path? Yes.** Every leg calls `wyrd_validate::matrix::classify` and/or `verdict` directly.
There is no copy, mock or stand-in. `parse_content_range` is private and is reached through `classify`.

**(c) Fixture includes the fault? Yes.** Each fixture carries the wrong answer itself: wrong bytes, wrong
header, 200 to a follow-up of a deleted key, `<Error>` rows, a lost probe beside a clean 404. The
precedence fixtures put the unproving fact first and the fatal one later, and cover every pair the
brief's Design names. The full-run tests replace one exercise at a time inside the real `PRE_0_1` profile.

The limit: this module is pure and #939 does the mapping from real responses, so nothing here has touched
a live gateway. That is the brief's stated scope, and (H)'s PASS is shown on hand-built exercises only.

## Commands run

- `cargo clippy --locked -p wyrd-validate --all-targets -- -D warnings`: clean.
- `cargo fmt --check -p wyrd-validate`: clean.
- `typos crates/validate`: clean (`typos` is the brief's named external dependency, and it was present).
- `git diff --check`: clean. `crates/validate/tests/capability_matrix.rs` is unedited (`git diff` empty).
- The target has no pre-commit config and no `core.hooksPath`, so `cargo xtask ci` is its formatter and
  lint gate.
- `cargo xtask ci` on the final tree: exit 0, "xtask ci: all checks passed". No file changed during or
  after the run.
- `cargo llvm-cov --locked -p wyrd-validate --test capability_matrix_required --test capability_matrix`,
  with the changed lines of `matrix.rs` scored by hand against the lcov report: 479 changed, 333
  instrumentable, 333 covered, none missed. The other 146 have no coverage record (comments, blank lines,
  closing braces, the doc table). I wrote this scoring inline, so it is not the C4-diff-cov gate's result.
- `cargo mutants --in-diff patch.diff -p wyrd-validate -- --test capability_matrix_required`: 78 mutants,
  45 caught, 33 unviable, 0 missed.
- Not run: `engine/scripts/run-verify.sh`, because it writes to a worktree outside the roots given to this
  leaf. I did the same red and green legs by hand with `cargo test` under `timeout`; Check's C4-verify runs
  the wrapper.
