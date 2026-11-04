# Build notes — issue 1006 (743.1b): the `required` rows of the capability matrix

## Base

- Built on `635e0bd` (`pdca-integrate: issue_955`), the supplied `pdca-integration/main-rdfeb36496b2d`
  stack base. It holds child-1 (`5d2c679`, issue_1005).
- The brief's first step passed: `git merge-base --is-ancestor 9b1a3e2 HEAD` returned 0, and
  `crates/validate/src/matrix.rs` exists.
- `Cargo.lock`, `deny*.toml`, `crates/validate/src/lib.rs`, every doc and child-1's test file are
  untouched. No public type, variant, field or function was added.

## What changed (paths and lines on the patched tree)

`crates/validate/src/matrix.rs`:

- `:20-23` the module doc no longer says the `required` checks are still to come.
- `:855-857` the `Evidence::open` doc now names what still lands there.
- `:918` `classify` hands the exercise to `required`. The ladder and `settle` are unchanged.
- `:923-926` the `shared_facts` doc now points at `required` for the 404 fact.
- `:1020-1133` `required`: one arm per typed exercise. Each arm pushes findings into the stage
  its evidence comes from, and `Evidence::settle` (child-1) picks the first fatal one in message
  order. Nothing returns early except where the brief says "only the two facts".
- `:1138` `is_typed_read`, `:1149` `defined_status`, `:1161` `conditional`, `:1190` `strong_etag`,
  `:1203` `declares_no_body`, `:1212` `content_range`, `:1227` `head_matches_the_object`,
  `:1246` `object_bytes`, `:1307` `no_body`, `:1332` `not_the_document`, `:1342` `listing`,
  `:1361` `delete_result`, `:1391` `judge_follow_ups`, `:1425` `follow_up`,
  `:1474` `parse_content_range`. All private.

`crates/validate/tests/capability_matrix_required.rs` is new, 37 tests. Its first leg is
`a_full_defined_run_against_pre_0_1_passes`.

## Why this shape

The two iteration-9 defects in #937 came from checks that returned early with "unsettled" before a
fatal check could run. Child-1 had already built the fix for that: stages plus `settle`. So no
check here returns a result. Each one only adds a finding, and a cut or an unfit ask is just one
more `Unproved` finding, which cannot stop a later check.

Status findings do not gate the later checks. For example `object_bytes` also runs on a 404. This
cannot produce a wrong class, because the status stage comes first and its fatal finding wins. The
only fatal stage earlier than status is `PutObject`'s request-side fact. Leaving out the gates
means there is no branch for a future edit to get wrong.

The one place that switches on the status is `conditional`, because (G) makes a 200 and a 304 read
different checks. Its `_ => {}` arm cannot hide anything: any other status is already fatal.

Taken from the peer (`results/issue_937/iteration-v9/patch.diff:1240-1266`):
`parse_content_range` unchanged, and the difference-offset report. `is_strong_etag` was already in
child-1.

## Decisions the brief leaves open — please check these at sign-off

1. **A retry clears a lost probe of the same key.** `[a → no answer, a → 404]` passes.
   - For: child-1's type says "a key may be probed more than once (a retry)"
     (`matrix.rs:704-706`), iteration 9 tested the same behaviour
     (`iteration-v9/patch.diff:2359-2365`), and "never less" argues that a key proved gone is
     proved.
   - Against: (C) lists "a follow-up with no head or a cut-off body" as unsettled without
     mentioning retries, so a strict reading makes this exercise unsettled.
   - A fatal probe is never cleared: `[a → 404, a → 200]` is `oracle`.
   - To take the strict reading, remove the `retried` filter in `judge_follow_ups` (about 3 lines)
     and flip the `a_retry_proves_…` test.

2. **A `DeleteObjects` of no keys still judges its follow-ups.** (G) says "only the two facts" and
   I read that as covering the exercise's own answer.
   - Every follow-up of a no-keys delete is a follow-up of a key it did not request, so (C)'s rule
     applies: a 5xx or a byte on it is `protocol`, and anything else stays unsettled.
   - The other reading would let an unfit ask hide a 5xx, which is the defect class this brief was
     written to remove, and it contradicts (B) step 3.
   - Shown in `a_delete_of_no_keys_reads_only_the_two_facts_of_its_own_answer`.

3. **The 404 fact applies only on `required` rows.** It sits in `required`, not `shared_facts`.
   The brief keeps the `unsupported` and `supported, not checked` checks unchanged, and no
   `pre-0.1` read row is in those states. A future profile that makes a read `unsupported` would
   judge its 404 by the declared refusal.

4. **`Exercise::Other` is never a typed read,** so a 404 to `Other { GetObject }` stays unsettled.
   The brief defines a typed read by the exercise's form, and child-1's test expects this at
   `capability_matrix.rs:587-595`.

5. **`Content-Range` is read by value:** `BYTES 2-5/16` and `bytes 02-05/016` pass. The brief says
   "exactly", but RFC 9110 §14.1 makes the unit case-insensitive and §14.4 allows `1*DIGIT`.
   `AGENTS.md`'s grammar rule asks for case-insensitive units, and the brief reports no defect in
   this grammar. Signs, spaces, `*` and overflow are all refused.

6. **A cut after every expected byte is unsettled,** not a pass, because the end of the body was
   never seen. This is the iteration-9 behaviour, and (C) lists a cut "after a correct prefix".

## Ruled out

- **Patching iteration 9's `check_required` in place.** The brief rules this out: its
  `Result`-with-early-return shape caused the defects. Its 270 lines (`patch.diff:907-1266`) would
  have needed every `?` and `return Err` removed anyway.
- **A shared helper for `refusal`'s no-body arms and `no_body`.** They overlap by about 10 lines,
  but `refusal` is child-1's and the brief says not to change it. Worth a later clean-up.
- **A new parser against an existing one.** I looked for a shared `Content-Range` parser, as the
  rubric asks. Neither `crates/validate/src/s3/` nor any other crate has one: `verbs.rs:61` only
  mentions the header. The brief also names `parse_content_range` as in scope.

## Refuting my own test

**(a) Genuine red? Yes.** I reverted `crates/validate/src/matrix.rs` to `HEAD` and ran
`cargo test -p wyrd-validate --test capability_matrix_required`. It compiled and gave
`3 passed; 34 failed`, including the first leg. With the fix restored: `37 passed; 0 failed`.

The three that pass on the base assert behaviour child-1 already has, and they pin it here:
`a_document_cut_before_it_could_be_read_is_unsettled`,
`an_exercise_with_no_check_of_its_own_is_unsettled_on_a_required_row`,
`no_response_head_is_no_verdict_and_its_follow_ups_are_not_judged`.

I also ran `cargo mutants` (it deliberately breaks the code to see if tests notice) over the new
functions: 98 mutants, 61 caught, 36 unviable (did not compile), 1 missed.
- The missed one was `a != b` → `a == b` in `object_bytes`' offset report (only the detail text
  changed). I added `the_report_of_wrong_bytes_says_where_they_differ`, then reapplied that mutant
  by hand: it now goes red.
- I did not re-run the full mutant set after that one test addition, or after the test-only clippy
  fix and a doc-only module comment edit.

**(b) Production path? Yes.** Every leg calls `wyrd_validate::matrix::classify` and/or `verdict`
directly. There is no copy, mock or stand-in. `parse_content_range` is private and is reached
through `classify`.

**(c) Fixture includes the fault? Yes.** Each fixture carries the wrong answer itself: wrong bytes,
wrong header, 200 to a follow-up of a deleted key, `<Error>` rows. The precedence fixtures put the
unproving fact first and the fatal one later, and cover every pair the brief's Design names. The
full-run tests replace one exercise at a time inside the real `PRE_0_1` profile, for every
`required` row and every `unsupported` row.

The limit: this module is pure and #939 does the mapping from real responses, so nothing here has
touched a live gateway. That is the brief's stated scope.

## Commands run

- `cargo xtask ci`: exit 0, "all checks passed". The new test's last two additions (the offset test
  and the unsupported-row loops) landed while this run was in progress, so it may not have compiled
  them.
- On the final tree, re-run separately:
  - `cargo test -p wyrd-validate --test capability_matrix_required --test capability_matrix`:
    37 and 30 passed.
  - `cargo clippy -p wyrd-validate --all-targets -- -D warnings`: clean.
  - `cargo fmt --check -p wyrd-validate`: clean.
  - `typos crates/validate`: clean.
- `crates/validate/tests/capability_matrix.rs` passes unedited (`git diff` empty).

## Existing problem, not from this patch

`RUSTDOCFLAGS='-D warnings' cargo doc -p wyrd-validate --no-deps` fails on
`crates/validate/src/sizes.rs:340`, a public doc linking to the private `Tuning::check`. This patch
does not touch that file. `cargo xtask ci` still passes. Not fixed here (out of scope).

## Findings against child-1

None. Every fact the table reads was already a field of child-1's type, so no public item was added.
