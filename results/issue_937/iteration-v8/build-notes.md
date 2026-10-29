# Build notes — #937 validate: the capability matrix (iteration 8)

Withheld from the reviewer; for the human at sign-off.

## Base

Built on the worktree the driver handed me: `$PDCA_WORKTREE` at **`93a6f37`**
(`pdca-integration/main-r42c5348f203d`, the bundle's `stack-base`), not `main @ 9b1a3e2`. The
patch adds two files, one `lib.rs` line and six hunks in a draft proposal; it depends on nothing
that differs between the two bases (every gateway line the rows cite was checked on this base,
see *Citations*). `Cargo.lock` and `deny*.toml` are untouched. If `cargo xtask ci`'s cargo-deny
advisory steps are red on this base (RUSTSEC-2026-0385/-0386, the inherited red the human
accepted at iteration 7's sign-off), that is the base's, not this slice's — see *Gate* below.

## What was built (deltas from iteration 7's module, per the brief)

Started from `iteration-v7/patch.diff` as the brief directs; iteration 7's `matrix.rs` was the
peer and the test was rebuilt from its test. Every delta the brief lists:

- **(B) a 5xx is an answer.** Iteration 7's `required_response` ("5xx is availability →
  unsettled") is gone; `head()` (`crates/validate/src/matrix.rs:867-872`) only turns *no head*
  into `NoAnswer`, and `expect_status` (`:874-881`) makes any other status, a 5xx included,
  `protocol`. The module doc states the rule (`:9-17`); `RowResult::Unsettled`'s doc says
  "never a 5xx" (`:773-774`). Amended 0017 §7 to match (table rows + one sentence).
- **(B) cut vs. unreadable.** `Body::Failed { why }` became two states: `Body::Cut { arrived,
  why }` carrying what arrived (`Arrived::Bytes` for a streamed object body, `Arrived::Count`
  for a buffered one; `:579-602`) and `Body::Unreadable { why }` for a body that arrived whole
  and is not the document its status calls for (`:573-578`). `expect_bytes` (`:894-941`)
  judges the bytes before a cut: not a prefix of the expected bytes (a wrong byte, or more
  bytes than the object; for a count-only cut, more than the object) → `integrity`; a correct
  prefix, or no byte → unsettled. `expect_no_body` (`:943-964`): any byte before a cut →
  `protocol`; a cut after no byte → unsettled. `whole_document` (`:966-973`) and the
  unsupported row (`:839-854`): cut → unsettled, unreadable → `protocol`.
- **(B1) unsupported rows:** 2xx → `unexpected-support` whatever the body (`:832-835`); any
  other status → `protocol` (`:836-838`); declared status + unreadable body or another document
  → `protocol`; declared status + cut-off body → unsettled.
- **(B2) conditional GETs carry `sent` and `etag`** (`:644-659`). Hit: unsettled unless `sent`
  is a strong tag equal to `etag`, then 304 + no body (`:1053-1062`). Miss: unsettled unless
  `sent` is a strong tag that differs, then 200 + whole body (`:1063-1077`). Guard first, as
  the ranged exercise's impossible-range guard the brief points at (`:1031-1036`).
- **(B2) 404 to a written key is `oracle`:** `expect_read_status` (`:883-892`) on every
  `GetObject` form and `HeadObject`; every other wrong status stays `protocol`.
- **(B2) the follow-up `HEAD`'s 5xx is `protocol`:** `probe_deleted` (`:1130-1151`), the
  `500..=599 → unsettled` arm iteration 7 had is gone (`_ => protocol`).
- **Listing / `<DeleteResult>` fixtures** the brief adds (a key listed twice → `oracle`; a
  `<Deleted>` naming a key twice → `protocol`) were already classified by iteration 7's sort
  and compare (`:1109-1115`, `:1225-1232`); they are now fixtures in the test.
- **(C), (D), (E)** unchanged in substance from iteration 7 (reviewed clean). `PRE_0_1`'s
  profile doc says retention starts at the first `0.1.0` release (`:252-256`); the 0017 §3
  paragraph says the same, corrected from "the first tag" as the brief asks.

## Design choices and what was ruled out

- **`Arrived { Bytes, Count }` rather than `Cut { bytes: Vec<u8> }` alone.** A bytes-only cut
  would force 743.3 to fake a prefix for buffered bodies (documents, error bodies) and would
  lose the one fact a count carries for an object body: *more* bytes than the object arrived,
  which is `integrity` even without the bytes. Cost of the two-variant enum: 16 lines
  (`:587-602`) plus one match arm in `expect_bytes` (`:905-916`).
- **Guard-first on the conditional exercises** (an exercise that did not send the right tag
  settles nothing, whatever the answer). The alternative, judge the answer and then the tag,
  would let a 500 to a mis-mapped exercise read as `protocol` and so FAIL a run whose real
  defect is the mapping; the brief wants INCONCLUSIVE, never PASS and not a false FAIL either.
  Same shape as the ranged guard the brief cites.
- **`expect_read_status` as a separate helper** rather than a flag on `expect_status`: five
  call sites (the four GET forms and HEAD) take it, four (PUT, DELETE, bulk, listings) take the
  plain one; a flag would put a boolean at nine sites for the same 10 lines.
- **Size budget (100,000 bytes).** The first green build was 119,045 bytes; iteration 7's was
  101,896. What I ruled out: `#[rustfmt::skip]` on the row table and the test tables (would
  keep one-line rows but is a reviewer smell and sidesteps the fmt gate), and `git diff -U1`
  (smaller context, more fragile on a base other than the one it was cut from). What I did,
  in order of yield: dropped `impl Display for RunVerdict` (≈1.4 KB) and `impl Display for
  Operation` (≈0.45 KB) — both renderings 743.3 can add when it prints a verdict; the structured
  `RunVerdict` carries everything the brief's (D)/(E) name (`:1296-1307`); tightened rustdoc
  that restated the brief (16.7 KB → 11.5 KB of `//` bytes in `matrix.rs`) and test comments
  (9.4 KB → 6.4 KB); a `gw!` citation macro and `nimpl`/`invalid` const helpers so each profile
  row is one line (`:221-298`); inline format captures and `oracle()`/`integrity()` helpers so
  calls stay under rustfmt's 60-char argument width; shorter test helper names (`fails`,
  `unproved`, `body`, `hs`, `cutb`, `cutn`); merged overlapping tests (41 → 39 legs). **No
  fixture or check the brief names was dropped**; the mapping below shows where each lives.
  Final: **99,724 bytes**.
- **`use Operation as Op;` sits right above the row table** (`:250`) rather than in the top
  `use` block, so the alias is defined where it is read. Legal (uniform paths); clippy is clean.

## Where each brief item lives in the test (`crates/validate/tests/capability_matrix.rs`)

- (A) `pre_0_1_declares_exactly_todays_rows` (every row cites `crates/gateway-s3/src/lib.rs:`),
  `pre_0_1_covers_every_version_below_0_1_0`, `a_full_passing_run_against_pre_0_1_passes`.
- (B1) `unsupported_row_passes_only_on_the_declared_status_and_code` (other code / status /
  5xx / no code / no body / no `<Code>`), `head_bucket_passes_on_a_400_with_no_body_only`,
  `any_2xx_to_an_unsupported_row_is_unexpected_support` (incl. a 2xx whose body then failed or
  was unreadable), `an_unsupported_row_with_no_response_head_has_no_verdict`,
  `unsupported_row_with_its_error_body_cut_off_is_not_proved` (INCONCLUSIVE fixture),
  `unsupported_row_with_an_unreadable_error_body_is_protocol` (its FAIL pair).
- (B2) `put_passes_only_on_200_with_a_strong_etag_after_the_body_ended` (`*`, unquoted, weak,
  empty, no ETag, early success); `strong_entity_tag_grammar`;
  `a_5xx_to_a_required_row_is_protocol` (PUT 503, whole GET 500, and every other row);
  `a_404_to_a_read_of_a_written_key_is_oracle` (HEAD and whole GET 404 → `oracle`; PUT/DELETE
  404 stay `protocol`); `whole_get_with_the_wrong_bytes_is_integrity` (one byte off, short,
  long, **empty body for a non-empty object**); `a_cut_off_object_body_is_judged_by_the_bytes_
  that_arrived` (cut after differing bytes → `integrity`; after a correct prefix →
  INCONCLUSIVE; count-only cuts); `ranged_get_passes_only_on_206_with_the_right_content_range_
  and_bytes` (200 whole body; wrong `Content-Range` incl. signs/unit; edges; impossible range);
  `conditional_get_with_the_wrong_status_or_a_body_on_its_304_is_protocol` (200 to matching,
  304 to non-matching, **304 cut after one byte**); `conditional_get_settles_nothing_without_
  the_tag_its_form_needs` (sent == object's tag on the miss, `*`, differing tag on the hit → all
  INCONCLUSIVE naming the row); `head_with_the_wrong_size_or_etag_is_oracle`;
  `listing_that_is_not_exactly_the_written_keys_is_oracle` (**a key twice**; empty prefix);
  `a_200_without_the_document_the_check_reads_is_protocol` (**ListObjectsV2 unreadable body**,
  documents where bytes were expected); `a_delete_follow_up_is_judged_on_its_status` (204 +
  HEAD 200 → `oracle`; **204 + HEAD 503 → `protocol`**; no answer → INCONCLUSIVE);
  `delete_objects_document_must_name_every_key_once_with_no_error_row` (one `<Error>` row;
  **`<Deleted>` naming a key twice**; all deleted + HEAD 200 → `oracle`; no HEADs →
  INCONCLUSIVE); `required_row_with_a_cut_off_document_is_not_proved`;
  `a_bodiless_answer_is_protocol_on_any_byte`; `every_follow_up_is_judged_against_the_key_it_
  asked_about`; `a_lost_or_unacknowledged_delete_result_never_hides_the_probes`;
  `an_exercise_without_the_rows_check_settles_nothing`.
- (C) `parse_reads_releases_and_ignores_build_metadata`, `unknown_is_never_read_as_release_
  0_0_0` (`unknown`, `+git.unknown`, `+git.unknown.dirty`), `parse_refuses_malformed_versions`,
  `each_version_selects_its_own_profile_and_expectations` (the same PUT receipt flips between
  the two test-local profiles; `1.0.0-rc.1` selects the lower one),
  `a_version_no_profile_covers_selects_none_and_says_why` (no match, unknown, unparsable,
  overlapping profiles).
- (D) `a_not_checked_row_is_listed_not_covered_and_never_passes`,
  `a_profile_cannot_declare_an_operation_twice`, `a_run_that_proves_no_row_is_inconclusive`.
- (E) `any_fatal_fails_the_run_naming_every_fatal_row`, `an_unexercised_undeclared_or_
  unanswered_row_is_inconclusive`, `a_row_passes_only_when_every_exercise_of_it_passed`.

## Demonstrated reds (each: temporary edit to `matrix.rs`, run, restore; file hash verified)

Runner: `cargo test -p wyrd-validate --test capability_matrix`, the form
`engine/scripts/run-verify.sh` uses for an added test. All 39 legs pass unmutated.

| # | temporary edit | failing legs |
|---|---|---|
| 1 | 2xx on an unsupported row returns `Pass` (`:834`) | `any_2xx_to_an_unsupported_row_is_unexpected_support`, `any_fatal_fails_the_run_naming_every_fatal_row`, `each_version_selects_its_own_profile_and_expectations` |
| 2 | 2xx head with a cut body returns `NoAnswer` (`:831`) | `any_2xx_to_an_unsupported_row_is_unexpected_support` |
| 3 | PUT ETag check `!etag.is_empty()` (`:1009`) | `put_passes_only_on_200_with_a_strong_etag_after_the_body_ended`, `a_bodiless_answer_is_protocol_on_any_byte`, `any_fatal_fails_the_run_naming_every_fatal_row` |
| 4 | non-matching conditional GET returns `Ok` on 304 (`:1074`) | `conditional_get_with_the_wrong_status_or_a_body_on_its_304_is_protocol` |
| 5 | a key with no probe is not unsettled (`:1194`) | `delete_objects_document_must_name_every_key_once_with_no_error_row`, `every_follow_up_is_judged_against_the_key_it_asked_about` |
| 6 | `select` filters `|_| true` (`:487`) | `a_version_no_profile_covers_selects_none_and_says_why`, `each_version_selects_its_own_profile_and_expectations`, `pre_0_1_covers_every_version_below_0_1_0` |
| 7 | `+git.unknown` check never matches (`:436`) | `unknown_is_never_read_as_release_0_0_0` |
| 8 | a `supported, not checked` row noted `NotExercised` (`:1340`) | `a_not_checked_row_is_listed_not_covered_and_never_passes`, `an_unexercised_undeclared_or_unanswered_row_is_inconclusive` |
| 9 | `request_body_ended` ignored (`:1005`) | `put_passes_only_on_200_with_a_strong_etag_after_the_body_ended`, `a_bodiless_answer_is_protocol_on_any_byte` |
| 10 | follow-up with no answer → `oracle` (`:1135`) | `a_delete_follow_up_is_judged_on_its_status`, `a_lost_or_unacknowledged_delete_result_never_hides_the_probes`, `an_unexercised_undeclared_or_unanswered_row_is_inconclusive`, `any_fatal_fails_the_run_naming_every_fatal_row`, `every_follow_up_is_judged_against_the_key_it_asked_about` |
| 11 | `head()` turns a 5xx into unsettled (`:870`, iteration 7's branch restored) | `a_5xx_to_a_required_row_is_protocol`, `a_lost_or_unacknowledged_delete_result_never_hides_the_probes` |
| 12 | non-matching check ignores `sent` (`:1069`) | `conditional_get_settles_nothing_without_the_tag_its_form_needs` |
| 13 | 404 to a written key not `oracle` (`:886`) | `a_404_to_a_read_of_a_written_key_is_oracle` |
| 14a | object-body cut unsettled without reading the prefix (`:925`) | `a_cut_off_object_body_is_judged_by_the_bytes_that_arrived` |
| 14b | bodiless-answer cut unsettled whatever arrived (`:950`) | `a_bodiless_answer_is_protocol_on_any_byte`, `conditional_get_with_the_wrong_status_or_a_body_on_its_304_is_protocol` |
| 15 | unreadable error body treated like a cut (`:846`) | `unsupported_row_with_an_unreadable_error_body_is_protocol` |

Two first attempts at 7 and 11 were defective mutations (one left a constant unused, which the
crate's denied warnings refuse to compile; one anchored on a doc line I had since trimmed) and
were re-run with corrected edits; the table shows the corrected runs. Driver script and output:
`$PDCA_SCRATCH/pdca-builder-937-reds/` (removed at the end of this beat).

## Refuting my own test

- **(a) Genuine red?** Yes. With production reverted — `matrix.rs` moved out and
  `pub mod matrix;` dropped from `lib.rs` — the test target does not compile (85 errors,
  `cannot find value CopyObject` and the like); restored, 39 pass. That is the net-new posture
  the brief records (C4-verify reports UNVERIFIABLE on a module that does not exist on the
  base). The binding evidence is the 16 mutation runs above: each named behaviour, when broken,
  turns at least one leg red.
- **(b) Production path?** Yes. Every leg calls `wyrd_validate::matrix::{classify, verdict,
  parse_version, select, select_reported, is_strong_etag}` and the shipped `PRE_0_1` /
  `PROFILES` constants; the test defines fixtures and two test-local profiles only, no copy of
  any check.
- **(c) Fixture includes the fault?** Yes. Each FAIL fixture replaces exactly one exercise of an
  otherwise passing run (`verdict_with`) and asserts the run FAILs naming that row and class;
  each INCONCLUSIVE fixture asserts no fatal, exactly one unproved row, and that it is the
  exercised one. The conditional-GET control fixtures send the wrong tag on purpose and assert
  INCONCLUSIVE (not PASS, not FAIL) whatever the answer, including a 200 with the whole body.

## Commit-readiness

- `cargo fmt --check -p wyrd-validate`: clean (both files written by `cargo fmt`).
- `cargo clippy -p wyrd-validate --all-targets -- -D warnings`: clean.
- `typos` over the three touched files: clean. Docs renderer deps (`markdown_it`, `yaml`)
  present, so `cargo xtask ci`'s prose gates run locally rather than warn-skip.
- No `Cargo.toml`, `Cargo.lock`, `deny*.toml`, gateway or `05-building-block-view.md` change
  (out of scope per the brief).

## Gate

`./engine/xtask.sh ci` (the configured C4-ci runner) ran on the worktree after the reds, with
the module restored: **every step green except the two `cargo deny` advisory steps** — xtask's
summary is "2 checks failed: cargo deny check; cargo deny --all-features --config
deny-all-features.toml check advisories". Run directly, both name the same two advisories,
RUSTSEC-2026-0385 (`AsyncFd::new` I/O safety) and RUSTSEC-2026-0386 (non-constant-time mask
generation), against dependencies this patch does not touch (`git diff --stat` shows no
`Cargo.toml`, `Cargo.lock` or `deny*.toml` change). This is the base's red the brief predicts
for `93a6f37` and the human accepted as inherited at iteration 7's sign-off; the brief says it
is not this slice's to fix. Recorded with the standing token so the driver matches the
registered doctor row (no new row proposed):

NEEDS-HUMAN external dependency: cargo deny all-features advisories — the unpatched base `93a6f37` fails both cargo-deny advisory steps of C4-ci on RUSTSEC-2026-0385 / RUSTSEC-2026-0386; the gate's fmt, clippy, build, test (incl. this test), prose and conformance steps are green, so C4-ci cannot read green on this base until the advisory is recorded or cleared on `main`.

## Citations (target branch, this worktree at `93a6f37`)

- Gateway lines every `pre-0.1` row cites were read on this base: `crates/gateway-s3/src/lib.rs:121`
  (`UNKNOWN_SERVER_VERSION`), `:410-422` (denylist), `:1694-1699` (`Server` header),
  `:1766-1788` (bulk delete), `:1797-1804` (bucket denylist), `:1809-1815` (bucket path, other
  methods), `:1819-1826` (root path), `:1842-1849` (object subresource denylist), `:1865-1871`
  (`CopyObject`), `:2008`/`:3401` (PUT), `:2017`/`:2469` (GET), `:2025`/`:2570` (HEAD),
  `:2026-2028` (DELETE 204), `:2293-2300` (200 with errors), `:2713-2719` (206),
  `:2785-2803` (304), `:2995` (`If-None-Match`).
- Version shapes: `crates/server/src/version/derivation.rs:33-68` (`normalize_describe`), `:65`
  (`+git.unknown`), `:118-124` (`Fallback` rung).
- The one `s3` dependency: `ErrorCode`, `crates/validate/src/s3/error.rs:68-76`, re-exported at
  `crates/validate/src/s3.rs:60` and `crates/validate/src/lib.rs:34-36`.
- Proposal 0017: §3 `docs/design/proposals/draft/0017-blackbox-validation-tool.md:193-199`
  (states), `:261-268` (version keying), `:284-291` (`pre-0.1` paragraph, new), §7 `:508-509`
  (the two amended table rows), `:518-520` (the 5xx sentence, new), Dependencies `:878-883`.
- The patch also applies cleanly onto `main @ 9b1a3e2` (checked with `git apply --cached
  --check` against a temporary index read from that commit).

## For the human

- **Dropped renderings.** `Display for RunVerdict` and `Display for Operation` from iteration 7
  are gone for size; the structured verdict is unchanged. 743.3 prints a verdict and will add a
  rendering then (or print `Debug`). If you would rather keep them, the budget needs ~1.9 KB
  elsewhere; the test comments and the module's remaining rustdoc are the only places left.
- **NEEDS-HUMAN carried from the brief (not this slice's):** a PUT or DELETE 2xx the base
  client reports as `S3Error::Unreadable` cannot be told apart as "a byte where the success has
  none" vs "framed then cut before its first byte" — flagged for 743.3's mapping in the brief's
  *Design*; the matrix holds either (`Body::Unreadable` → `protocol`, `Body::Cut` after no byte
  → unsettled).
- No external dependency beyond the brief's (`typos`, `docs-renderer`) was needed.
- **Orphan scratch dirs, not mine:** `$PDCA_SCRATCH` holds eleven `pdca-builder-937-it4-*` …
  `-it7`/`-redlegs` directories dated 2026-10-23 from earlier iterations of this bundle. I
  removed every directory this session created (`-fmt`, `-reds`, `-basecheck`) and left those,
  since I did not create them; they are safe to delete once this bundle is signed off.
