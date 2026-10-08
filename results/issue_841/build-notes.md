# Build notes — #841 (809.3) restore fence for `Open` upload sessions — iteration 3

Worktree: `/home/eddie/wyrd/wyrd.pdca-wt-l0`, base `4bda59c` (origin/main + #839 + #840, the
`pdca-integration/…/main` stack base). Every `path:line` below is on that tree with the patch
applied.

## What this iteration changed (the carry-forward)

I started from iteration 2's patch (`iteration-v2/patch.diff`, applied cleanly on `4bda59c`) and
fixed what the carry-forward named. Nothing else in the design changed.

1. **A failed fence no longer reads `clean=true` in the summary.** (C5 finding; the T4 batch's
   three `restore.rs:713` / `:1315` BUGs.)
   - `crates/custodian/src/restore.rs:1320`: `clean = fence_finished && report.is_clean()`.
   - `crates/custodian/src/restore.rs:1321`: `needs_human = !fence_finished || report.needs_human()`.
   - Why `needs_human` too: one of the batch findings named it. When the fence did not finish,
     an `Open` session may still be live, and no loop fences it; only the operator's re-run does.
     The Check reviewer wrote that this part "need not" change. I changed it anyway, so the
     summary cannot read `needs_human=false` for a run that ends in `Err` with work left. This
     only affects the summary line. `RestoreReport::needs_human()` is unchanged, and there is no
     report on the `Err` path.
   - New leg: `a_fence_fault_on_an_otherwise_clean_store_is_never_certified_clean`
     (`crates/custodian/tests/restore_open_fence.rs:816`). The store holds one `Open@3` session
     and nothing else; its fence commit fails. The test asserts the pass is `Err` and the one
     summary has `clean == "false"` and `needs_human == "true"`.
   - Docs now say "never clean" for this case: `restore.rs:370-371` (the pass doc),
     `06-runtime-view.md:65`, and `m4-first-deployment-blueprint.md:637` (`clean=false`).

2. **`SessionRecord::open_teardown`'s contract now has a `wyrd-core` test.** (C5 finding; the 5
   surviving mutants.)
   - New inline module `open_teardown` (`crates/core/src/multipart.rs:5067-5121`). Its test
     `only_an_open_session_has_a_teardown_and_its_obligation_matches_its_key` (`:5084`) checks:
     - `Open@3` gives a teardown whose session equals a decoded `Aborting@4` record (every other
       field the same);
     - the key equals `retire_key(Bytes, Session{id, 3, None})`;
     - the encoded payload decodes through `decode_retire_obligation` against that key, to the
       same mode, token and payload, which is `{session, all}`;
     - `Open@u64::MAX`, `Aborting`, `Completing` and `Completed` all give `None`.
   - I applied each surviving mutant by hand and ran this one test. All four distinct ones fail
     it (`vec![0]` is the same case as `vec![1]`):
     - `delete !` at `:2265` fails at `:5096` (the `expect`);
     - `delete field state` at `:2272` fails at `:5097`;
     - `key -> vec![]` / `vec![1]` at `:3612` fails at `:5104`.
   - **Why inline, not in `crates/core/tests/`:** a file there would be a 10th file, and the
     brief caps the patch at 9. Inline `#[cfg(test)]` modules are already the pattern in
     `crates/core/src/metadata.rs:3382`, `:4449`, `read.rs:717` and others. cargo-mutants runs
     `wyrd-core`'s lib tests, so this test is in its reach.
   - I removed the `debug_assert!` in `RetireObligation::session_teardown` (iteration 2 had it
     at `multipart.rs:3609-3616`). It checked the same key/payload agreement that the new test
     now checks by a full decode, and its doc now points at the test (`:3591-3592`). This also
     saved bytes (see Size).

3. **DST coverage: deferred to #843, with a marker in the code.** (The T4 batch's three
   `TEST-GAP` findings, and the adversary's NEEDS-HUMAN [human].)
   - #843 is child-5, open: "dst: seeded Tier-0 coverage for the restore session fence (809.5)"
     (checked with `gh issue view 843`).
   - Markers: `crates/custodian/src/restore.rs:722` and
     `crates/custodian/tests/restore_open_fence.rs:15`, both `// deferred: #843`.
   - I also recorded the three findings in the bundle's `review-rejected.md` as
     "Deferred — tracked in #843", following the rubric's deferral rule and the format other
     bundles use (e.g. `results/issue_635/review-rejected.md`).
   - **This decision is the human's to confirm.** `deferred-findings.json` lists it as needing
     human judgment. The brief does put DST out of scope ("DST coverage (child-5; …)"), but if
     you want this fence to wait for DST coverage, delete those rows and say so at sign-off.

4. **Not changed: C4 diff coverage "patch.diff does not apply on origin/main".** That gate
   applies the patch to bare `origin/main`, but this bundle's base is `origin/main` + #839 +
   #840 (brief, first paragraph). The patch can't apply there. This is the harness's stacked-base
   limit, not a defect in the patch: `git apply --check -R` of `patch.diff` against the worktree
   is clean. Both Check reviewers read it the same way.

## What the patch does (unchanged from iteration 2, re-cited)

The post-restore pass (`reconcile_after_restore`) now fences every upload session the restored
image holds `Open`, as its last step, after Pass 3 (`restore.rs:714-716`):

- `fence_open_sessions` (`restore.rs:723-736`) re-lists `mpu:` in bounded pages through
  `gc::staged_page` (`restore.rs:726`; `gc.rs:1734`, widened to `pub(crate)`).
- `open_teardown` (`restore.rs:793-812`) sorts each session:
  - `Open` → fence;
  - `Aborting` / `Completed` → leave alone, not counted;
  - anything else → a named cause.
- The fence is ONE `WriteBatch` (`restore.rs:754-761`):
  - `require(mpu:<id>, bytes as read)`;
  - `require_absent(retire:bytes:s:<id>:<E>)`;
  - put the session as `Aborting@E+1`;
  - put the obligation `{"session":true,"parts":"all"}`.
- What happens on each commit outcome:
  - `Committed`: counted in `sessions_fenced` and audited `action=session-fenced`
    (`restore.rs:763-767`).
  - `Conflict`: the pass re-reads the session, then the obligation key, once. It names the
    session `ChangedUnderPass`, `ObligationKeyTaken{key}` or `LostConflict`, and never retries
    (`restore.rs:768-781`).
  - `Err` (including `CommitUnknownResult`): audited `action=session-fence-failed`, then passed
    up unchanged. It is never read as a conflict (`restore.rs:783-786`).
- The summary goes out before the error is passed up (`restore.rs:715`). When the fence did not
  finish, it reads INCOMPLETE, `clean=false` and `needs_human=true` (`emit_summary`,
  `restore.rs:1293-1330`).
- Report fields: `sessions_fenced: usize` and `sessions_unsettled: Vec<UnsettledSession>`
  (`restore.rs:215`, `:218`; types at `:223-266`).
  - `is_clean()` is false when any session was fenced (`restore.rs:286`, doc `:278`).
  - `needs_human()` counts unsettled sessions (`restore.rs:323`, doc `:304-305`).
- `Completing` sessions are named, with a `// deferred: #842` marker (`restore.rs:807`).

Writer-side API in `crates/core/src/multipart.rs`:

- `SessionRecord::open_teardown(&self, &UploadId) -> Option<OpenTeardown>` (`:2264`). It returns
  `None` unless the session is `Open` below `u64::MAX`. It clones the decoded record and changes
  only `epoch` (+1) and `state` (`Aborting {}`).
- `OpenTeardown { session, obligation }` (`:2282`).
- `RetireObligation { mode, token, payload }` (`:3584`). Its fields are private, and its only
  constructor is the private `session_teardown` (`:3593`), which builds the key and the payload
  together. Accessors: `key()` (`:3612`) and `payload()` (`:3617`). So the key and the payload
  can't disagree.
- Doc notes updated to match:
  - module header (`:91-95`);
  - `SessionRecord`'s constructor note (`:2163-2166`);
  - the retire rows table's `{session, all}` row (`:3250`);
  - `RetirePayload`'s note (`:3288-3291`);
  - the `all` wildcard note (`:3172-3173`);
  - the `RetiredMap` sentence (`:2989`).
  - In iteration 2 I also edited the `RecordError` wildcard doc (`:381`). I reverted that this
    time: "the reaper's `Open` arm commits" is still true, so the edit wasn't needed.

Other files:

- `crates/server/src/cli.rs`:
  - The summary line counts fenced and unfenced sessions (`:1273-1274`).
  - A NEEDS-HUMAN paragraph names unfenced sessions through `named_records`, which is unchanged
    (`:1374-1392`), and counts them by cause (`unsettled_causes`, `:1453`).
  - Comment fix for the "never decodes a session value" claim (`:1352-1357`).
  - Tests:
    - the agreement test is extended (`:3028`, `:3113`);
    - `restore_verdict_counts_fenced_sessions_and_names_the_ones_it_could_not_fence` (`:3123`);
    - the 21-session bound test (`:3172`).
  - I dropped the "fenced sessions alone" block at the end of the `:3123` test. It repeated the
    agreement test's `fenced` case, which already asserts `needs_human` false, `is_clean()`
    false, and no "could NOT be fenced" paragraph.
- `crates/custodian/tests/staged_protection.rs:1725-1800`: the one existing leg the brief says
  changes by design. Its protection assertions are kept. Its naming assertion now requires the
  session to be named exactly once on the restore audit seam, as `session-unsettled`, and never
  as `unresolvable-staged-record` (`:1784`).
- `crates/custodian/tests/restore_staged_report.rs:228-238`: `segment_nonce` added to child-1's
  `open_session` builder. **Flag:** I also rewrote that builder's 4-line doc comment, which said
  "No segment nonce …". A strict reading of "nothing else in it" excludes that edit. Both of the
  file's legs pass unchanged.
- Docs: `06-runtime-view.md:65` (new §6.5 paragraph) and `m4-first-deployment-blueprint.md:599-640`
  (step 7: four bills, the corrected UNREADABLE claim, the NOT FENCED bill, the fence paragraph).

## Decisions and what I ruled out

- **Fence before Pass 3: ruled out** (leg P3; #809 iteration 1's defect).
- **A fresh `mpu:` listing for the fence, not the staged reader's.** `staged_fragments` keeps no
  session values, and its observer (`gc.rs:1537`) reports only part reads. Reusing it would mean
  widening `StagedSet` or the observer in `gc.rs`, and the brief limits `gc.rs` to a visibility
  change.
- **Sort out a `Conflict` by re-reading, rather than reading the obligation key first.** A
  pre-read would still need the after-conflict check for F-race, and would add one `get` per
  `Open` session. The `require_absent` in the batch is what enforces the rule.
- **Pass up an `Err` from the fence commit; don't try to settle it with a re-read.** Copying
  GC's settle path (`gc.rs:1022-1060`) would be about 25 lines plus a test leg. The fence is
  idempotent, so the re-run settles it. The cost: a fence that landed under an unknown result is
  never counted in `sessions_fenced` by either run. It is named on the audit seam
  (`session-fence-failed`).
- **`sessions_unsettled` holds `{session, cause}`, not a `Vec<String>`.** F-collision needs the
  cause on the report and the audit line.
- **An undecodable session value stays out of `unresolvable`.** If it went in, every mark would
  be withheld, which changes the base's protection and mark behaviour (brief H(i)). It is named
  only as unsettled.
- **This iteration: `needs_human` on a cut fence** (above). **Inline core test** (above, cost: a
  10th file). **Kept the CLI's count-by-cause table** (`cli.rs:1453`), although it duplicates
  `SessionUnsettled`'s `Display` in wording. Printing `"<key> (<cause>)"` through `named_records`
  instead would have saved about 1.5 KB of diff. It would also drop the cause of every session
  past the 20th from the CLI, and change a part that passed review.

## Tests: red → green

New file: `crates/custodian/tests/restore_open_fence.rs` (copy in the bundle). It has 9
`#[tokio::test]` functions and uses only symbols that exist on the base; the new report fields
are read through `Debug`.

| Leg | Test (`restore_open_fence.rs`) |
|---|---|
| F | `an_open_session_is_fenced_whole` (:489) |
| F-atomic (both runs) | `a_failed_fence_commit_leaves_neither_write` (:532) |
| F-race | `a_session_that_changes_under_the_pass_is_not_fenced_blind` (:566) |
| F-collision | `the_fence_never_overwrites_an_obligation` (:611) |
| H (i)(ii)(iii) | `what_the_pass_cannot_fence_is_named_never_passed_off_as_done` (:687) |
| K | `a_second_pass_is_idempotent` (:730) |
| P3 | `a_fence_fault_never_hides_the_pass_verdicts` (:772) |
| P3, otherwise clean (new) | `a_fence_fault_on_an_otherwise_clean_store_is_never_certified_clean` (:816) |
| Paging | `sessions_listed_across_pages_are_all_fenced` (:840) |
| CLI | the `cli.rs` tests above (green-only, inside `cli.rs`, as the brief says) |

**9 of 9 red on the base.** I reverted `crates/{core,custodian,server}/src` to `4bda59c` and kept
every test file. The file compiled, and all 9 failed by assertion. The gate's UNVERIFIABLE
compile-failure path is not reached. Where each first failed:

- F, K, Paging: `assert_fenced` (`:371`) finds the session still `Open@3`.
- F-atomic: `expect_err` (`:550`) gets `Ok`.
- F-race: "the race was not exercised" (`:582`).
- F-collision: `needs_human()` false (`:646`).
- H: the undecodable session is not named (`:699`).
- P3: `Ok` (`:785`).
- P3, otherwise clean: `Ok` (`:825`).

**The new leg is red against iteration 2's fix.** I put back only iteration 2's two summary lines
(`clean = report.is_clean()`, `needs_human = report.needs_human()`). The new leg then failed and
the other 8 passed. The failure printed the reviewer's exact counterexample: `"clean": "true"`,
`"needs_human": "false"`, `"fence_finished": "false"`, message `INCOMPLETE — the session fence
did not finish …`. With the fix: 9 of 9 pass.

**Green on the final tree:**
- `restore_open_fence` 9/9, `staged_protection` 36/36, `restore_staged_report` 2/2.
- `wyrd-server` `restore_` tests 7/7, and the `wyrd-core` `open_teardown` test 1/1.
- The full gate `./engine/xtask.sh ci` (= `cargo xtask ci` in this worktree) printed
  `xtask ci: all checks passed`, exit 0. It covers typos, docs lint/render, fmt, clippy, build,
  the whole-workspace tests, cargo-machete, cargo-deny, statics, deploy-guard, DST clippy and
  DST tests. `crates/dst/tests/custodian.rs` is untouched and its restore properties pass.

**How I ran the single file.** I used `cargo test -p wyrd-custodian --test restore_open_fence`
under `timeout`. That is the call `engine/scripts/run-verify.sh` makes (`:441`). I did not run
`run-verify.sh` itself: it creates its own verify worktree beside the primary Wyrd checkout,
which is outside the roots I may write to. Check's C4-verify runs it.

**Self-test from the brief's Invariant.** I ran this in iteration 2, and the fence code it
covers has not changed since:
- fence moved before Pass 3: only P3 failed;
- fence split into two commits: F-atomic's obligation-side run failed (and so did F, which is
  stricter than the brief's note);
- blind put of the obligation: only F-collision failed.

## Refute-your-own-test

- **(a) Genuine red? Yes.** With the sources reverted to the base and the tests kept, all 9 fail
  by assertion (above). With only iteration 2's summary lines put back, the new leg fails and
  shows the reviewer's counterexample. Each core mutant, applied by hand, fails the new core
  test. All of these were real runs.
- **(b) Production path? Yes.** Every leg calls the production
  `wyrd_custodian::reconcile_after_restore` through a `GcContext`. The fence code under test is
  the production `fence_open_sessions` / `fence_session` / `SessionRecord::open_teardown`. The
  doubles are only the `MetadataStore` / `ChunkStore` seams. Obligations are checked through the
  production `decode_retire_obligation`, sessions through `decode_session_record`, and the
  summary through the production `emit_summary`'s real audit event. The core test calls the
  production `open_teardown` directly.
- **(c) Fixture includes the fault? Yes.**
  - The new P3 leg puts a real `CommitUnknownResult` on the fence's own commit, over a store with
    no other finding. That is the case the earlier P3 leg masked with its dangling chunk.
  - F-atomic injects the same on each half.
  - F-race lands a real concurrent `Open@3 → Completing@4` batch, and asserts that it committed.
  - F-collision seeds a decodable foreign `{session}` obligation under the fence's key.
  - H seeds an undecodable value, `u64::MAX`, and a `Completing` session.
  - Paging lowers the scan cap to 2 and asserts that a one-`scan` listing fails.

## Scope and budget

- **Files: 9**, the brief's maximum. The same nine as iteration 2; the core test is inline in
  `multipart.rs`.
- **Size: 89,646 bytes** (`wc -c patch.diff`; budget under 90 KB). The fixes added about
  4.4 KB to iteration 2's 89,236. I won back about 4 KB without dropping any leg or binding
  assertion:
  - put back the base wording of the `is_clean` / `needs_human` docs and added one short line to
    each, instead of reflowing them (about 2.1 KB);
  - reverted the unneeded `RecordError` doc edit (about 0.6 KB);
  - removed the `debug_assert!` that the core test replaces (about 0.45 KB);
  - removed the redundant CLI sub-case (about 0.45 KB);
  - made the new test file's asserts shorter (about 0.4 KB).
- `crates/dst/tests/custodian.rs`: untouched.

## For the human at sign-off

- **DST deferral to #843.** I recorded it in `review-rejected.md` and with in-code markers.
  Confirm it, or overrule it (point 3 above).
- **Stale sentence outside the file budget.** `docs/design/architecture/05-building-block-view.md:202`
  says the multipart record types have no writers yet. After this patch the restore fence writes
  a session transition and a `retire:bytes:` obligation. The 9-file budget leaves `05` out.
  Either allow a 10th file, or open a follow-up issue.
- **`LostConflict` has no test.** It needs a store that reports a conflict that a re-read can't
  explain, and the brief names no leg for it. It is the fall-through arm (`restore.rs:778`).
- **C4 diff coverage** can't be measured on a stacked base (point 4 above).
- **External dependencies** (`typos`, `docs-renderer`): both are present locally, and their
  steps passed inside `cargo xtask ci`. I needed no dependency the brief didn't list.
