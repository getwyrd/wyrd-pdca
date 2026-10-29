# Build notes — #937 (743.1) validate: the capability matrix — iteration 7

Withheld from the reviewer. For the human at sign-off. Line numbers are on the worktree base
below with the patch applied, unless a line says otherwise.

## What this round is

Iteration 6's sign-off asked for three implementation-level things:

1. **Two guards deletable with the suite green** (review, "pure test additions"): the
   exercise-matches-row guard at the top of `classify`, and the "a document where object bytes
   were expected" guard in `body_bytes`. Both now have fixtures (`RED-26`, `RED-27` below).
2. **A vacuous PASS at the profile level**: `verdict` returned PASS for a profile with no
   `required` or `unsupported` row (an empty run against it), and `Profile::new` accepted a
   profile declaring one operation twice, with `Profile::row` silently reading the first. The
   review offered "make zero proved rows INCONCLUSIVE, or add a test over `PROFILES`". I did
   the first, in production, and closed the duplicate at construction (details under *What
   changed*). Both are pinned (`RED-28`, `RED-29`, `RED-30`).
3. **The advisory wall** (C4-ci: `cargo deny … check advisories` fails on the base). Still
   there, same cause, declared again under *Base* with the standing marker.

Iteration 3's sign-off (still standing) asked that the matrix be carried forward unchanged in
substance and the three open design points flagged again; they are, under *Open design points*.

The brief's 100 KB backstop (`brief.md:166-167`) forced a size pass on the test and two
refactors in `matrix.rs` with identical behaviour; see *Size*.

## Base: read this first (it decides whether C4-ci can go green)

- **The worktree is `93a6f37`**, the tip of `pdca-integration/main-r42c5348f203d` (`main`
  `4bac538` + #744 `61e5341`, #853 `582da99`, #932 `93a6f37`); the bundle's `stack-base-tip`
  says the same. The brief's ordering note (`brief.md:112-122`) says either base works and that
  Do records which one it used. Check rebuilds base + `patch.diff` from the stack base, and the
  harness owns the worktree base, so I built here.
- **What it costs, confirmed on this round's base before anything was edited.** `Cargo.lock`
  pins `ctutils 0.4.2` (`Cargo.lock:1000-1001`) and `tokio 1.52.3` (`:3953-3954`). `cargo deny
  --all-features --config deny-all-features.toml check advisories` in the worktree: **exit 1**,
  `advisories FAILED`, RUSTSEC-2026-0385 and RUSTSEC-2026-0386
  (`$PDCA_SCRATCH/pdca-builder-937-it7/deny.log`). The patch touches no `Cargo.toml`,
  `Cargo.lock` or `deny*.toml` (out of scope, `brief.md:134`; bumping the lockfile here would
  duplicate #950/#954 and conflict on merge).
- #508 is not on this base: the denylist still lists `uploads`, `uploadId`, `partNumber`
  (`crates/gateway-s3/src/lib.rs:410-413`), so the six multipart rows stay `unsupported(#508)`
  per `brief.md:29-31`.
- Expect C4-diff-cov to say "does not apply on origin/main" again (the `lib.rs` hunk differs
  between the integration line and `main`).

NEEDS-HUMAN external dependency: cargo deny all-features advisories — the run's stack base `93a6f37` (`pdca-integration/main-r42c5348f203d`) still pins `ctutils 0.4.2` and `tokio 1.52.3` (`Cargo.lock:1000-1001`, `:3953-3954`), so `cargo deny --all-features --config deny-all-features.toml check advisories` exits 1 on the unpatched base (RUSTSEC-2026-0385, RUSTSEC-2026-0386; confirmed this round before the patch was applied, `$PDCA_SCRATCH/pdca-builder-937-it7/deny.log`) and C4-ci cannot go green on it; #950/#954 fixed both on `main` but not on the integration line, and the standing doctor row runs in `../wyrd` (green), so it will not flag this. This patch touches no manifest, lockfile or deny config; refresh the integration line onto current `main` (or rebase this bundle on `main`) and re-run Check

(The case is already registered — the standing row in `pdca.toml` — so no new `[[doctor.checks]]`
row is proposed. What that row cannot do is run against the stack base instead of `../wyrd`;
that is a harness question, not build work.)

## What changed since iteration 6

### Production, `crates/validate/src/matrix.rs` (three behavioural changes, two refactors)

1. **A profile cannot declare an operation twice** — `Profile::new`, `matrix.rs:225-241`.
   The `const fn` now walks the rows pairwise and panics on a repeated operation. For the
   shipped `const` profiles that panic is a **compile error** (a duplicate can never ship); for
   a profile built at run time it panics there, which the test pins with `#[should_panic]`
   (`capability_matrix.rs:1081`). Why construction and not `verdict`: `Profile::row` is public
   and 743.3 may call `classify(profile.row(op), …)` directly, so a check only in `verdict`
   would still let the first row hide the second. Cost of the alternative (a `Result` from
   `new`): `Result::unwrap` is not `const`, so every `const` profile would need a
   `match … { Ok(p) => p, Err(_) => panic!() }` block — the same panic, 4 more lines per profile.
2. **A run that proves no row is INCONCLUSIVE** — `verdict`, `matrix.rs:1423-1440`.
   `RunVerdict` gains `proved: Vec<Operation>` (every `required`/`unsupported` row that was
   exercised and whose every exercise passed, in profile order), and the outcome is PASS only
   when something was proved. With the row rules unchanged, `proved` is empty with `fatal` and
   `unproved` both empty exactly when the profile declares no checked row, so that is the one
   new INCONCLUSIVE case; the `Display` says so ("nothing proved: no required or unsupported
   row declared", `matrix.rs:1452-1460`). When rows were proved the report lists them
   ("proved: PutObject, GetObject, …"), which 743.3's verdict artifact wants anyway. Cost of the
   alternative (reject such a profile in `Profile::new`): 4 lines less, but a transitional
   profile whose rows all moved to `supported, not checked` would then be unconstructible
   rather than honestly INCONCLUSIVE, and `verdict` is where the claim "PASS" is made.
3. **`Display for Operation`** (refactor, same output): the 22-arm `name()` table is gone; the
   three `GetObject` forms are spelt out and every other row prints its variant identifier,
   which *is* the S3 operation name (`matrix.rs:72-85`). `RunVerdict`'s `Display` uses
   `to_string`. Output pinned by the existing `contains("PutObject: protocol")`,
   `contains("UploadPart: supported, not checked (#750)")` and the new `proved:` assertions.
4. Section banners are the crate's one-line style (`// --- title ---…`, as
   `crates/validate/tests/s3_client_upload_peers.rs:223`), in both files.
5. Module doc and `verdict` doc updated for the zero-proved rule (`matrix.rs:23-29`,
   `:1378-1383`).

Everything else in `matrix.rs` is iteration 6's text, which is iteration 3's text.

### The test, `crates/validate/tests/capability_matrix.rs` — 57 tests (was 53)

| Finding (iteration 6 review) | Test | Pins | Leg |
|---|---|---|---|
| exercise-matches-row guard (`classify`) deletable | `an_exercise_of_another_operation_settles_nothing_about_a_row` `:533` | a GET answer against the PutObject row is `Unsettled` with the exact detail; a PUT receipt against CopyObject is not `unexpected-support`; against `not_checked(UploadPart)` is not `NotChecked` | RED-26 |
| "document where bytes expected" guard deletable | `a_document_where_object_bytes_were_expected_is_protocol` `:659` | a GET of an **empty** object answered 200 + a listing is `protocol` with the exact detail (no byte mismatch could catch it); a listing to a GET, a `<DeleteResult>` to the non-matching conditional GET, a listing to a 206 are `protocol`, never `integrity` | RED-27 |
| duplicate operation in a profile | `a_profile_cannot_declare_an_operation_twice` `:1081` | `Profile::new` panics with "declares an operation twice" | RED-28 |
| vacuous PASS at profile level | `a_run_that_proves_no_row_is_inconclusive` `:1089` | a profile holding only `not_checked(UploadPart)` (exercised) and a profile with no rows both give INCONCLUSIVE with `proved`, `fatal`, `unproved` empty and "nothing proved" in the report; every shipped profile has a checked row | RED-29 |
| (new) `proved` must exclude fatal rows | `any_fatal_fails_the_run_naming_every_fatal_row` `:1113` | two fatal rows and one unproved ⇒ `proved.len() == rows − 3` | RED-30 |

`proved` is also asserted in `a_full_passing_run_against_pre_0_1_passes` `:371` (every row, in
profile order, and the report line), `a_not_checked_row_need_not_be_exercised…` `:1056`
(`[PutObject]`), `an_empty_run_proves_nothing` `:1176` (empty) and
`a_row_passes_only_when_every_exercise_of_it_passed` `:1184` (PutObject excluded).

## Size, and what the size pass did

The brief caps the patch at the 100 KB backstop (`brief.md:166-167`; the threshold is
102,400 bytes). Iteration 6 was 101,953; the four new tests and the production changes took the
first draft to **108,115**. Nothing was dropped to get back under; every assertion of iteration 6
is still in the file. What changed, in order of bytes saved:

- `use FailureClass::{Integrity, Oracle, Protocol, UnexpectedSupport}; use Operation::*;` at the
  top of the test, so rustfmt (`fn_call_width = 60`) folds most `assert_fails_with(…)` calls back
  onto one line (−1.7 KB).
- One-line section banners in both files, the crate's own style (−1.5 KB).
- `Display for Operation` via the variant identifier (−0.8 KB; item 3 above).
- Test helpers for literals rustfmt spreads over 3–7 lines (`struct_lit_width = 18`): `just(status)`,
  `fatal(class, detail)`, `unsettled(why)`, `head(answer)`, `listed(key, size)`, `rel(a, b, c)`,
  `selected(table, v)`, `ranged_at(first, last, answer)`, consts `ANY` and `NIMPL` (−1.4 KB).
- The malformed-version, bad-entity-tag, wrong-`Content-Range` and below-`0.1.0` lists are one
  split string each instead of one literal per line; the three malformed strings with whitespace
  or nothing are chained separately (−0.9 KB).
- Two-line comments that said one thing made one line; the 0017 *Dependencies* paragraph and the
  §3 version sentence tightened without losing a fact (−0.5 KB).

Result: **101,896 bytes = 99.51 KiB**, 504 bytes under the backstop. If you want more headroom, the one hunk nothing
else depends on is the 0017 *Dependencies* paragraph iteration 5 added at iteration 4's review's
request (now 6 lines); dropping it re-opens that review finding.

## Open design points: still yours to decide

Unchanged from iterations 4–6, as asked: current behaviour kept, rustdoc consistent with the
code, flagged again here.

1. **Strict (E).** A row is proved only when it was exercised and *every* exercise of it passed
   (`matrix.rs:1378-1383`, enforced at `:1384-1447`; pinned by `a_row_passes_only_when_every_exercise_of_it_passed`,
   `capability_matrix.rs:1184`). Brief (E) says "exercised only with no answer". The strict rule
   never claims more than the answers prove; a PUT or DELETE with no answer is an unresolved
   mutation (0017 §7, `0017:528-539`). But one timed-out *read* retry then ends the run
   INCONCLUSIVE, which fits poorly with §7's budgeted `availability` (`0017:508`). 743.3 (#939) is
   the first caller; decide before it builds retries on this. Relaxing it to the brief's wording
   is about 5 lines in `verdict` plus flipping that test.
2. **`0.1.0-rc.1` selects `pre-0.1`.** The brief says `pre-0.1` covers everything below `0.1.0`
   (`matrix.rs:396-403`; pinned at `capability_matrix.rs:357`). The amendment says
   "Retention starts at the first tag" (`0017:287`). If the first tag is `v0.1.0-rc.1`, that tag
   lands in the profile that is edited in place. Either end the range at the first tag or say
   "the first `0.1.0` release" in the amendment. I left it as the brief states.
3. **Listing a prefix with nothing written is unsettled, not PASS** (`matrix.rs:1154-1158`;
   pinned by `a_listing_of_a_prefix_with_nothing_written_proves_nothing`,
   `capability_matrix.rs:861`). A rule added in iteration 3; the brief does not ask for it. It is
   the same vacuous pass the module refuses for a `DeleteObjects` of no keys and, since this
   round, for a profile with nothing to prove. To drop it: 4 lines of code, 9 of test.

New this round, for the record: **the zero-proved rule** (item 2 under *What changed*) is my
reading of 0017's vacuity gate (`0017:570-575`) applied to the matrix; the brief's (E) does not
spell it out. If you would rather reject such a profile at construction, it is a 6-line swap.

For the record (unchanged since iteration 2): a 5xx other than 501 on a required row is
unsettled (`availability`, `matrix.rs:946-954`); a 501 is `protocol`. On an
`unsupported` row any wrong status, a 5xx included, is `protocol` (brief B).

## Where each part of the brief lives (on `93a6f37` + patch)

- `crates/validate/src/lib.rs:24`: `pub mod matrix;`, the brief's one line.
- `crates/validate/src/matrix.rs` (new, 1,486 lines):
  - rows in three states, `Expected` `:117-130`; `Row` `:134-139`;
  - `Profile::new` with the duplicate check `:225-241`;
  - (A) `PRE_0_1_ROWS` `:268-393`, each row citing its gateway line; `PRE_0_1` `:396-403`;
    `PROFILES` `:406`;
  - (C) `parse_version` `:493-552` (unknown and `+git.unknown[.dirty]` never a release; every
    token checked; `parse_numeric` `:561-566`); `select` `:593-607`; `select_reported`
    `:610-613`; `VersionRange::contains` with SemVer precedence `:206-214`;
  - the observation type `Answer`/`Response`/`Body`/`Exercise` `:619-818`, with
    `request_body_ended` and a probe per follow-up;
  - (B) `classify_unsupported` `:908-942` (any 2xx is `unexpected-support`);
  - (B′) `check_required` `:1038-1166`; `probe_deleted` `:1171-1191`; `check_delete_objects`
    `:1197-1243`; `check_delete_result` `:1247-1281`; `is_strong_etag` (RFC 9110 §8.8.3)
    `:1286-1294`; `parse_content_range` `:1298-1313`;
  - (D) `RowResult::NotChecked`, `classify` `:891-906`, `not_covered` in `verdict`;
  - (E) `RunVerdict` `:1365-1376` (with `proved` `:1369`); `verdict` `:1384-1447`; its
    `Display` `:1449-1486`.
- `crates/validate/tests/capability_matrix.rs` (new, 57 tests): fixtures `:17-298`; (A)
  `:300-382`; (B) `:384-489`; (B′) `:490-886`; (C) `:887-1046`; (D) `:1047-1109`; (E)
  `:1110-1192`.
- 0017: the third state `0017:196-199`; the version on the wire `:263-268`; editing `pre-0.1`
  in place until the first tag `:284-290`; *Dependencies* `:873-878`.
- Docs currency: the patch adds a library module, not a port, API operation, RPC, CLI flag or
  persisted field. The brief leaves `05-building-block-view.md` to 743.3, which adds the
  feature (`brief.md:130-131`).

## How the test was run

The project's per-fix runner composes `cargo test -p <pkg> --test <name>` from the added test
file (`engine/scripts/run-verify.sh:404-413`), and the whole gate is `./engine/xtask.sh ci`
(`docs/INTEGRATION.md:441`). `xtask` has no single-target test subcommand. For red→green and the
red legs I ran the runner's own target, `cargo test -p wyrd-validate --test capability_matrix`,
under `timeout 600`/`900` from the worktree; the test is pure (no I/O, no network, no clock), so
the timeout is a backstop. The whole gate ran once on the final tree (below).

Baseline before any edit, iteration 6's test on this base: 53 passed. After the edits: **57
passed** (every run in this round; last at `redlegs.log`, final line).

## Refuting my own test

**(a) Genuine red? Yes.** Two forms, both on this round's tree:

- *The whole module reverted* (`pub mod matrix;` removed from `lib.rs`): the test target does
  not compile (`error[E0432]: unresolved import wyrd_validate::matrix`). The declared posture
  (`brief.md:138-141`); C4-verify will say UNVERIFIABLE (exit 77).
- *Each behaviour reverted, one at a time.* `$PDCA_SCRATCH/pdca-builder-937-it7/redlegs.py`
  (run with `python3 -I`) made 30 temporary edits to the production `matrix.rs`. For each it
  checked the anchor occurs exactly once, ran the target, and restored the file byte for byte
  (sha256 `cd4940eb…f72c` checked after every leg; `redlegs.log`). Every leg went red:

| # | Temporary edit to `matrix.rs` | Result | Red test(s) |
|---|---|---|---|
| RED-1 | (B) a 2xx to an unsupported row → `Pass` | 53/4 | `any_2xx_to_an_unsupported_row_is_unexpected_support`, `an_unexpected_success_fails_the_run`, `any_fatal_fails_the_run_naming_every_fatal_row`, `each_version_selects_its_own_profile_and_expectations` |
| RED-2 | (B) a 2xx head whose body failed → `NoAnswer` | 56/1 | `any_2xx_to_an_unsupported_row_is_unexpected_support` |
| RED-3 | (B′) the PUT ETag check accepts any non-empty value | 54/3 | `put_etag_that_is_not_a_strong_tag_is_protocol`, `a_bodiless_answer_cut_off_after_its_head_is_not_proved`, `any_fatal_fails_the_run_naming_every_fatal_row` |
| RED-4 | (B′) the non-matching conditional GET accepts a 304 | 56/1 | `non_matching_conditional_get_answered_304_is_protocol` |
| RED-5 | (B′) `DeleteObjects` folds the document check only (`worst(checks.into_iter().take(1))`) | 52/5 | `delete_objects_document_alone_never_proves_the_delete`, `delete_follow_up_with_no_answer_or_a_5xx_is_inconclusive_not_oracle`, `an_unresolved_delete_names_the_delete_row`, `every_follow_up_is_judged_against_the_key_it_asked_about`, `a_cut_off_delete_result_never_hides_a_probe_that_reads_a_key` |
| RED-6 | (C) the selector ignores the version (`contains(release) \|\| true`) | 53/4 | `each_version_selects_its_own_profile_and_expectations`, `pre_0_1_covers_every_version_below_0_1_0`, `a_version_no_profile_covers_selects_none`, `a_selection_that_finds_no_profile_says_why` |
| RED-7 | (C) the `+git.unknown` identity check never fires (`&& false`) | 56/1 | `unknown_is_never_read_as_release_0_0_0` |
| RED-8 | (D) a `supported, not checked` row also counted as unexercised | 55/2 | `a_not_checked_row_need_not_be_exercised_and_is_listed_as_not_covered`, `an_exercise_the_profile_declares_no_row_for_is_not_proved` |
| RED-9 | (B′) the PUT check ignores when its head arrived | 55/2 | `put_answered_before_its_body_ended_is_protocol_whatever_its_etag`, `a_bodiless_answer_cut_off_after_its_head_is_not_proved` |
| RED-10 | (B′)/(E) a delete whose follow-up `HEAD` had no answer → `oracle` | 53/4 | `delete_follow_up_with_no_answer_or_a_5xx_is_inconclusive_not_oracle`, `an_unresolved_delete_names_the_delete_row`, `any_fatal_fails_the_run_naming_every_fatal_row`, `a_cut_off_delete_result_never_hides_a_probe_that_reads_a_key` |
| RED-11 | it5: `Exercise::Other { .. } => Ok(())` on a required row | 56/1 | `a_required_row_is_never_passed_by_an_exercise_that_has_no_check` |
| RED-12 | it5: the empty-keys guard removed | 56/1 | `a_bulk_delete_of_no_keys_or_with_a_key_missing_from_deleted_is_not_a_pass` |
| RED-13 | it5: `<Deleted>` no longer has to name every requested key | 56/1 | `a_bulk_delete_of_no_keys_or_with_a_key_missing_from_deleted_is_not_a_pass` |
| RED-14 | it5: an unsupported row's torn error body made fatal `protocol` | 56/1 | `an_unsupported_row_refused_as_declared_but_cut_off_is_not_proved` |
| RED-15 | it5: the `Content-Range` unit no longer has to be `bytes` | 56/1 | `ranged_get_with_the_wrong_content_range_is_protocol` |
| RED-16 | it5: a 206 with no `Content-Range` passes (`None => {}`) | 56/1 | `ranged_get_with_the_wrong_content_range_is_protocol` |
| RED-17 | it5: a listing 200 without a listing document → `Ok` | 56/1 | `a_200_without_the_document_the_check_reads_is_protocol` |
| RED-18 | it5: a bulk-delete 200 without a `<DeleteResult>` → `Ok` | 56/1 | `a_200_without_the_document_the_check_reads_is_protocol` |
| RED-19 | it5: an exercise with no declared row silently skipped | 56/1 | `an_exercise_the_profile_declares_no_row_for_is_not_proved` |
| RED-20 | it1: a probe paired with the wrong key (`==` → `!=`) | 56/1 | `every_follow_up_is_judged_against_the_key_it_asked_about` |
| RED-21 | it1: `\|\|` → `&&` in `parse_numeric` | 56/1 | `parse_refuses_malformed_versions` |
| RED-22 | it1: a bodiless answer cut off after its head read as `protocol` | 56/1 | `a_bodiless_answer_cut_off_after_its_head_is_not_proved` |
| RED-23 | it2: a torn bulk-delete document returns before the probes | 56/1 | `a_cut_off_delete_result_never_hides_a_probe_that_reads_a_key` |
| RED-24 | it3: an empty listing of an empty prefix passes | 56/1 | `a_listing_of_a_prefix_with_nothing_written_proves_nothing` |
| RED-25 | it1: a signed `Content-Range` number accepted (`&&` → `\|\|`) | 56/1 | `ranged_get_with_the_wrong_content_range_is_protocol` |
| RED-26 | it6: the exercise-matches-row guard in `classify` removed (`&& false`) | 56/1 | `an_exercise_of_another_operation_settles_nothing_about_a_row` |
| RED-27 | it6: a document where object bytes were expected → `Ok(&[])` | 56/1 | `a_document_where_object_bytes_were_expected_is_protocol` |
| RED-28 | it6: the duplicate-operation check in `Profile::new` removed (`&& false`) | 56/1 | `a_profile_cannot_declare_an_operation_twice` (a `should_panic` test: "test did not panic as expected"; the log's name column is blank for that line shape) |
| RED-29 | it6: the zero-proved rule removed (`\|\| proved.is_empty()` dropped) | 56/1 | `a_run_that_proves_no_row_is_inconclusive` |
| RED-30 | it7: a fatal row listed as proved (the `fatal` filter dropped) | 56/1 | `any_fatal_fails_the_run_naming_every_fatal_row` |

(`redlegs.log`: every line `RED`; `restored: OK; final run: 57 passed / 0 failed`.)

RED-1–10 are the brief's ten required demonstrated reds (`brief.md:142-150`). RED-11–19 are
iteration 5's findings, RED-20–25 iterations 1–3's, RED-26–29 iteration 6's, RED-30 this
round's own addition.

**(b) Production path? Yes.** Every test calls `wyrd_validate::matrix::{classify, verdict,
select, select_reported, parse_version, is_strong_etag}`, `Profile::new` and the
`PRE_0_1`/`PROFILES` constants directly (`capability_matrix.rs:7-15`). No copy, no mock. The
legs edit those functions and the tests notice.

**(c) Fixture includes the fault? Yes.** Each failing element is in the fixture itself: a 2xx
to an unsupported row, a cut-off body (on required and unsupported rows), a weak, unquoted or
`*` ETag, an early PUT acknowledgement, a 304 to a non-matching tag, a `<DeleteResult>` with an
`<Error>` row or a key missing from `<Deleted>`, a bulk delete of no keys, a follow-up `HEAD`
that reads a deleted key, gets no answer or a 503, a torn `<DeleteResult>` next to a readable
probe, a 200 with no document or the wrong one (now also to a GET of an empty object), a
`Content-Range` with the wrong unit or none, a generic exercise against a required row, an
exercise paired with the wrong row, an exercise with no declared row, a profile with a repeated
operation, a profile with nothing to prove, `+git.unknown`, and an overlapping table. Full runs
are built from `PRE_0_1.rows()` (`passing_run`, `capability_matrix.rs:239`), so no declared
row is left out.

## Gates and commit-readiness

- `cargo fmt --all`: ran over the tree after every edit; both files are fmt-clean.
- `cargo clippy -p wyrd-validate --all-targets --all-features -- -D warnings`: clean (the
  workspace already denies warnings and `clippy::all`, `Cargo.toml:255-269`).
- `typos` on the three changed text files: clean. Both of the brief's external dependencies
  (`typos`, `docs-renderer`) are present on this host.
- The target repo configures no commit hooks (no `core.hooksPath`, no `.pre-commit-config.yaml`,
  no `.githooks`), so the formatter and the prose gates are what a commit has to pass.
- `patch.diff` is `git diff` of the worktree (the two new files intent-to-add); it reverses
  cleanly on the worktree (`git apply --check -R`), so it applies on `93a6f37`.
- Size: **101,896 bytes**, under the 102,400-byte backstop (see *Size*).

## Self-review against the rubric (`AGENTS.md` § Review rubric & protocol)

- One clock: the module reads no clock. Narrow seams: the module depends on `crate::s3::ErrorCode`
  only (`matrix.rs:33`); nothing from a `wyrd-*` crate (the blackbox guard checks this in `ci`).
- No shared mutable global state: `PRE_0_1`, `PROFILES` and the rows are `const`; the duplicate
  check runs at compile time for them.
- Grammar strictness: the entity-tag, `Content-Range` and version parsers validate every token;
  signs, wrong units, empty identifiers, leading zeros are refused and pinned.
- Absent entries: a missing document, a key missing from `<Deleted>`, an unprobed key, a listing
  with nothing written, a row with no exercise, an exercise with no row, an exercise paired with
  the wrong row, and a profile with nothing to prove each give an explicit result (`protocol` or
  unsettled/INCONCLUSIVE), never silent success. A duplicate row is refused at construction, not
  silently shadowed.
- Protocol input: a torn body is never silently accepted (unsettled on required and unsupported
  rows, pinned both ways); a 2xx that then tears is still `unexpected-support`.
- Docs currency: no port, API operation, RPC, CLI flag or persisted field; 0017 amended as the
  brief asks, and its *Dependencies* section no longer contradicts §3.

## The whole gate on the final tree

`/home/eddie/wyrd/wyrd-pdca/engine/xtask.sh ci` with `PDCA_WORKTREE` set to this worktree,
on the final tree (`$PDCA_SCRATCH/pdca-builder-937-it7/ci.log`).

**Exactly two checks fail, both the advisory wall described under *Base*:** `cargo deny check`
(`ci.log:3653`) and `cargo deny --all-features --config deny-all-features.toml check advisories`
(`ci.log:4013`), each on RUSTSEC-2026-0385/0386 against the unchanged `ctutils`/`tokio` pins —
the same two as on the unpatched base (`deny.log`).

Every other step passed: `typos` (`ci.log:2`), `lint_docs` OK (`:5`), `render_site` 99 pages
and link audit OK (`:7-9`), the unsafe guard (`:15`), the blackbox guard (`wyrd-validate` links
no `wyrd-*` crate, `:18`), `cargo fmt --all -- --check` (`:20`), workspace clippy (`:22`), build
and test — `capability_matrix` **57 passed** at `ci.log:2645-2646`, 0 `FAILED` result lines in
the whole log — `cargo-machete` (`:3330`), `cargo deny --all-features check licenses bans
sources` (`:4015`), and the DST tier (`--cfg madsim` clippy and test, `:4025` on).

Final summary (`ci.log:4212-4215`, 4,215 lines, 222 `test result: ok` lines, 0 `FAILED`):

```
xtask: 2 checks failed:
  - cargo deny check: exited with exit status: 1
  - cargo deny --all-features --config deny-all-features.toml check advisories: exited with exit status: 1
```

Exit 1 for those two alone; nothing in this patch is in either.

## Scratch

Everything this round made outside the worktree is under `$PDCA_SCRATCH`
(`/var/tmp/pdca/wyrd-pdca-9c587031/issue_937/pdca-builder-937-it7/`): `deny.log`,
`redlegs.py`, `redlegs.log`, `banners.py`, `ci.log`. The `-it6-*` and older dirs there are
previous rounds'. I did not delete any of them: the harness owns that root.
