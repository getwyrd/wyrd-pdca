# Build notes — #937 (743.1) validate: the capability matrix — iteration 2

Withheld from the reviewer. For the human at sign-off. Line numbers are on the worktree base
below, with the patch applied.

## Base

Built on the worktree the driver gave this Do: `pdca-integration/main-r42c5348f203d` at
`93a6f37` (on `main` `4bac538`: #744 `61e5341`, #853 `582da99`, #932 `93a6f37`). The brief's
ordering note allows either base. The matrix uses nothing from #744/#853/#932 except
`s3::ErrorCode`, the same type with the same `Display` on both bases
(`crates/validate/src/s3/error.rs:69`, `:191-199` here; `:168-176` on `4bac538`).

#508 is **not** on this base: the subresource denylist still lists `uploads`, `uploadId`,
`partNumber` (`crates/gateway-s3/src/lib.rs:410-424`). #932 only stages part bytes in core. So
the six multipart rows stay `unsupported(#508)`, not `supported, not checked (#750)`.

**The one `lib.rs` hunk applies on this line only.** `pub mod matrix;` goes at
`crates/validate/src/lib.rs:24`, between `pub mod args;` and `pub mod payload;`. On `main` the
next line is `pub mod s3;` (no `payload`/`sizes` there), and rustfmt's `reorder_modules` keeps
`mod` lines sorted, so no placement has context that matches both bases. On `main` it is the
same one line, between `pub mod args;` and `pub mod s3;`; the other three files apply on both.

This is why iteration 1's C4-diff-cov said "patch.diff does not apply on origin/main": that gate
takes its base from the brief's `@ main` (`engine/scripts/run-verify.sh:263-269`, rung
`PDCA_BRIEF_BASE`), while Do's worktree, C4-ci and C4-verify used the integration line
(C4-verify applied the same patch and went GREEN, `iteration-v1/gate-logs/C4-verify.log:10`).
That is a harness base mismatch between gates, not a patch defect. **Expect it to recur on this
iteration's C4-diff-cov for the same reason.** Worth raising upstream (pdca-harness): a bundle
with a `stack-base` should give C4-diff-cov the same base as C4-verify.

## What changed since iteration 1 (the carry-forward)

Started from iteration 1's patch (it applied cleanly to `93a6f37`) and changed what the
carry-forward names, plus report-text fixes found by running the C5 gate locally.

### Production — `crates/validate/src/matrix.rs`

1. **Bodiless answers now look at their body** (C5 causal adequacy; T4 ×3; adversary).
   New `expect_no_body` (`matrix.rs:998-1008`): `Empty` or zero bytes passes; `Body::Failed` is
   unsettled (whether bytes arrived is unknown); any bytes or document is `protocol`. Used by:
   - PUT 200, after the early-receipt and ETag checks (`matrix.rs:1075-1076`). A torn PUT was
     `Pass`; now INCONCLUSIVE. An unexpected body on a PUT was `Pass`; now `protocol`.
   - the matching conditional GET's 304 (`matrix.rs:1111-1115`). `304 + Body::Failed` was fatal
     `protocol` ("answered 304 with a body", which the observation does not show); now
     unsettled. Real bytes stay `protocol`.
   - HEAD 200, after the size and ETag checks (`matrix.rs:1139`). A torn HEAD was `Pass`.
   - DELETE 204 (`matrix.rs:1141-1145`). A torn 204 was `Pass`.
   - the delete probe's 404 (`matrix.rs:1188`): a 404 that did not end whole proves nothing.
   Head facts are judged first, so a cut never hides a fatal fact the head already showed (an
   early receipt, a bad ETag, a key still readable).
2. **`worst` fold** (`matrix.rs:1012-1024`): fatal outranks unsettled outranks pass, and every
   check is judged. Used by DELETE (body check and probe) and by the bulk delete.
3. **Bulk-delete follow-ups are each judged against their own key**
   (`check_delete_objects`, `matrix.rs:1204-1279`):
   - every follow-up for a key is judged, not just the first (`:1261-1277`). Any fatal probe of
     a key fails the row: `[(a,404),(a,200)]` → `oracle` (it was `Pass`). One 404 proves the key
     even if another probe of it could not settle: `[(a,503),(a,404)]` → proved (it was
     unsettled).
   - a follow-up naming a key the delete did not request is unsettled, not skipped
     (`:1252-1260`).
   - a key requested twice is unsettled before anything else (`:1212-1220`). Before, `[a,a]`
     requested from a server that names `a` once under `<Deleted>` would have been a false
     `protocol`.
4. **Removed unused code** the reviewer named or C5 found: `Ord`/`PartialOrd` for `Release` and
   `PreId`'s derived ordering (`cmp_bound`, `matrix.rs:439-451`, is the only comparison
   selection needs, since bounds have no pre-release), and `Display for Bound` (used nowhere).
5. **Report text:** `NoProfile` printed `Release { major: 2, … }` through `{:?}`. Added
   `Display for Release` (`matrix.rs:453-465`) so it reads `no profile covers release
   2.1.0-rc.1.x`.
6. The module doc and the `RowResult::Unsettled` doc are reworded to match: a check that
   cannot settle neither passes nor fails; a row with such a check and no fatal one is unproved.

### Test — `crates/validate/tests/capability_matrix.rs` (41 → 46 tests)

- `a_bodiless_answer_cut_off_after_its_head_is_not_proved` (`:772`): torn PUT, HEAD, 304, 204
  and probe-404 → INCONCLUSIVE; an early receipt, a `*` ETag, or a readable key with a torn body
  still FAIL.
- `a_bodiless_answer_that_carries_a_body_is_protocol` (`:810`): bytes or a document on PUT 200,
  HEAD, 304, 204 and probe-404 → `protocol`; zero bytes on PUT/HEAD/304 → pass (the reviewer's
  untested `Bytes(empty)` guard).
- `every_follow_up_is_judged_against_the_key_it_asked_about` (`:844`): the reviewer's three-key
  fixture `a:404, b:404, c:200` → `oracle`. Also `a:404, b:404` with `c` unprobed →
  INCONCLUSIVE. That second fixture is what catches `== → !=` against the new all-probes code;
  the three-key fixture alone does not, because under `!=` key `a` collects `c`'s 200 and still
  reads `oracle`. Plus duplicate probes in both orders, probes of unrequested keys alone and
  beside the requested ones, and a key requested twice.
- `a_range_is_checked_at_its_edges_and_an_impossible_one_settles_nothing` (`:876`): ranges
  `3-3`, `15-15` and `0-15` pass; `5-4` and `2-16` are unsettled.
- `required_row_with_a_cut_off_body_or_an_availability_5xx_is_not_proved` (`:730`) extended:
  torn listing v2 and v1, bulk-delete document, non-matching GET and ranged GET → INCONCLUSIVE
  (the two surviving `Body::Failed` arm deletions).
- `ranged_get_with_the_wrong_content_range_is_protocol` (`:546`): adds `bytes +2-+5/+16` and
  each single signed field (the surviving `&& → ||` mutant; the AGENTS.md grammar rule).
- `parse_refuses_malformed_versions` (`:950`): adds `0.0.0-r_c` (pre-release character check).
- Report text pinned: each `NoProfile` (`a_selection_that_finds_no_profile_says_why`, `:1090`;
  `overlapping_profiles_select_none`, `:1067`), an unsupported row's `protocol` detail
  (`:320`, `:356`), and the `integrity` offset (`whole_get_one_byte_off_is_integrity`, `:499`).

### Not changed, on purpose

- **A row exercised twice, once passing and once with no answer, is INCONCLUSIVE**
  (`a_row_passes_only_when_every_exercise_of_it_passed`). The brief's (E) says "exercised only
  with no answer"; I kept iteration 1's stricter rule (every exercise must pass) because a PUT or
  DELETE with no answer is an unresolved mutation (0017 §7, `0017:513-524`), and the invariant
  is "never claim more than the answers prove". Smoke exercises each row once, so the two
  readings differ only for later scenarios. Flagging it in case you want the literal reading.
- **A 5xx on a required row** other than 501 is unsettled (`availability`, `0017:493`); a 501 is
  `protocol` (a refusal). On an `unsupported` row any wrong status, 5xx included, is `protocol`,
  per brief (B).
- The findings in `deferred-findings.json` are held for you, not build work; not touched.

## Size

`patch.diff` is 96,265 bytes (iteration 1: 85,859), under the 100 KB backstop. The growth is the
new tests (~8 KB) and the fold, no-body and probe code (~2 KB), less the removed `Ord`/`Display`.

## How the test was run

The project wrapper `./engine/xtask.sh` exposes whole gates only (`cargo xtask ci`, …), not a
single test target. For the red legs I ran the exact target the brief names
(`cargo test -p wyrd-validate --test capability_matrix`) under `timeout 600`. The test is pure
(no I/O, no network, no clock), so it cannot hang; the timeout is a backstop. The full gate
(`./engine/xtask.sh ci`) was run once on the final tree (result at the end).

## Refuting my own test

**(a) Genuine red? Yes, in the only form a net-new module allows.** With the fix reverted
(`pub mod matrix;` removed from `lib.rs`), the test does not compile: `error[E0432]: unresolved
import wyrd_validate::matrix`. That is the declared verification posture (a): C4-verify will say
UNVERIFIABLE (exit 77). To show each leg binds its property, I made 27 temporary edits to the
production `matrix.rs`, one at a time, ran the target, and restored the file. Final run on the
shipped `matrix.rs` (sha256 `ba8153c0…1439`, restored byte for byte, `sha256sum -c` OK). The
test file then had one comment reworded and nothing else (`:868`; now `528fbac1…7e9a`), re-run
green. Every leg went RED on the named test:

| # | Temporary edit to `matrix.rs` | Red test(s) |
|---|---|---|
| 1 | (B) a 2xx to an unsupported row → pass | `any_2xx_to_an_unsupported_row_is_unexpected_support`, `an_unexpected_success_fails_the_run`, `each_version_selects_its_own_profile_and_expectations`, `any_fatal_fails_the_run_naming_every_fatal_row` |
| 2 | (B) a 2xx head whose body failed → no answer | `any_2xx_to_an_unsupported_row_is_unexpected_support` |
| 3 | (B′) PUT ETag check accepts any non-empty value | `put_etag_that_is_not_a_strong_tag_is_protocol`, `a_bodiless_answer_cut_off_after_its_head_is_not_proved`, `any_fatal_fails_the_run_naming_every_fatal_row` |
| 4 | (B′) non-matching conditional GET accepts a 304 | `non_matching_conditional_get_answered_304_is_protocol` |
| 5 | (B′) `DeleteObjects` passes on its document alone | `delete_objects_document_alone_never_proves_the_delete`, `delete_follow_up_with_no_answer_or_a_5xx_is_inconclusive_not_oracle`, `an_unresolved_delete_names_the_delete_row`, `every_follow_up_is_judged_against_the_key_it_asked_about` |
| 6 | (C) selector ignores the version (`take(1)`) | `each_version_selects_its_own_profile_and_expectations`, `pre_0_1_covers_every_version_below_0_1_0`, `a_version_no_profile_covers_selects_none`, `overlapping_profiles_select_none`, `a_selection_that_finds_no_profile_says_why` |
| 7 | (C) `+git.unknown` read as release `0.0.0` | `unknown_is_never_read_as_release_0_0_0` |
| 8 | (D) a `supported, not checked` row counted as unexercised | `a_not_checked_row_need_not_be_exercised_and_is_listed_as_not_covered` |
| 9 | (B′) PUT check ignores when its head arrived | `put_answered_before_its_body_ended_is_protocol_whatever_its_etag`, `a_bodiless_answer_cut_off_after_its_head_is_not_proved` |
| 10 | (B′)/(E) a delete whose follow-up HEAD had no answer → `oracle` | `delete_follow_up_with_no_answer_or_a_5xx_is_inconclusive_not_oracle`, `an_unresolved_delete_names_the_delete_row`, `any_fatal_fails_the_run_naming_every_fatal_row` |
| 11 | PUT ignores its response body (iteration-1 defect) | `a_bodiless_answer_cut_off_after_its_head_is_not_proved`, `a_bodiless_answer_that_carries_a_body_is_protocol` |
| 12 | 304 arm put back to iteration 1's (torn 304 → `protocol`) | `a_bodiless_answer_cut_off_after_its_head_is_not_proved` |
| 13 | HEAD ignores its body | both bodiless tests |
| 14 | DELETE ignores its body | both bodiless tests |
| 15 | bulk-delete probe `probed == key` → `!=` | `every_follow_up_is_judged_against_the_key_it_asked_about` |
| 16 | bulk delete judges only the first probe per key | same |
| 17 | probes of unrequested keys ignored | same |
| 18 | Content-Range digit check `&&` → `\|\|` (lets `+` through) | `ranged_get_with_the_wrong_content_range_is_protocol` |
| 19 | pre-release character check `\|\|` → `&&` | `parse_refuses_malformed_versions` |
| 20 | zero bytes read as a body | `a_bodiless_answer_that_carries_a_body_is_protocol` |
| 21 | range guard `first > last` → `>=` | `a_range_is_checked_at_its_edges_and_an_impossible_one_settles_nothing` |
| 22 | listing's `Body::Failed` arm deleted | `required_row_with_a_cut_off_body_or_an_availability_5xx_is_not_proved` |
| 23 | bulk delete's `Body::Failed` arm deleted | same |
| 24 | a key requested twice not caught | `every_follow_up_is_judged_against_the_key_it_asked_about` |
| 25 | fatal no longer outranks unsettled in `worst` | `a_bodiless_answer_cut_off_after_its_head_is_not_proved`, `delete_follow_up_with_no_answer_or_a_5xx_is_inconclusive_not_oracle` |
| 26 | `DeclaredCode`'s report text emptied (a C5 survivor) | `unsupported_row_passes_only_on_the_declared_status_and_code`, `head_bucket_passes_on_a_400_with_no_body_only` |
| 27 | integrity offset finds the first *equal* byte (a C5 survivor) | `whole_get_one_byte_off_is_integrity` |

Legs 1–10 are the brief's required demonstrated reds; 11–25 are the carry-forward findings;
26–27 are survivors from my local C5 run. Leg 8's first spelling did not compile (an unused
`mut` under `-D warnings`), so it was rewritten to keep the not-covered list and also mark the
row unexercised; that version went red as above.

**(b) Production path? Yes.** Every test calls `wyrd_validate::matrix::{classify, verdict,
select, select_reported, parse_version, is_strong_etag}` and the `PRE_0_1` constant directly:
the production functions, no copy. The legs edit those functions and the tests notice.

**(c) Fixture includes the fault? Yes.** The fixtures carry the failing element itself: torn
bodies (`Body::Failed`) on every bodiless answer, a probe that reads a deleted key, probes for
the wrong key, duplicate probes, signed `Content-Range` fields, and full runs built from
`PRE_0_1.rows()` so no declared row is left out (`passing_run`, `capability_matrix.rs:127-181`).

## Mutation gate (C5) run locally

Ran the gate's own script (`scripts/mutants-in-diff`, `PDCA_BUNDLE` = this bundle,
`PDCA_WORKTREE` = this worktree) on the patch before the last round of edits:
**159 mutants: 4 missed, 125 caught, 30 unviable** (iteration 1: 16 missed). The four:

- `Display for DeclaredCode` → now caught (leg 26).
- `Display for Bound` → removed; it was used nowhere.
- `!=` → `==` in `expect_bytes`'s offset → now caught (leg 27).
- `||` → `&&` in `parse_numeric`'s digit check (`matrix.rs:571`): **an equivalent mutant**. The
  only non-digit `u64::from_str` accepts is a leading `+`, and `parse_version` splits on `+` (as
  build metadata) before any number reaches `parse_numeric`; pre-release numbers are checked
  all-digits before the call. So no input can tell the two apart. I kept the explicit check
  because the AGENTS.md grammar rule ("no `+`/`-` signs via `from_str`") asks for it rather than
  relying on the split above it. Expect Check's C5 to report this one as missed.

I did not re-run the whole mutants pass after the last edits (it takes ~10 minutes and the
gate re-runs it); legs 26 and 27 show the two fixed survivors are now caught.

## Full gate and commit-readiness

- `./engine/xtask.sh ci` (the C4-ci gate, `cargo xtask ci`) on the final tree, with
  `PDCA_WORKTREE` set to this worktree: **`xtask ci: all checks passed`, exit 0.** It ran `typos`
  and `render_site.py --check` ("link audit OK"; the brief's two external dependencies, both
  present, neither warn-skipped), fmt, clippy, the workspace tests (this file: 46 passed), deny
  and the DST tier. No `test result: FAILED` anywhere in the log. It ran before the one-comment
  rewording in the test (`:868`); after it, `cargo fmt --check`, the test and `typos` re-ran
  clean.
- `cargo fmt --all -- --check` clean; `cargo clippy -p wyrd-validate --all-targets
  --all-features -- -D warnings` clean.
- `patch.diff` is byte-identical to `git diff` in the worktree, applies with
  `git apply --check` on the base `93a6f37`, and on `origin/main` (`ce4388b`) every file but the
  one `lib.rs` line applies (see *Base*).
- No NEEDS-HUMAN external dependency: everything the brief named was present and ran.
