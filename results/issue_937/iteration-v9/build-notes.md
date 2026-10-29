# Build notes — #937 validate: the capability matrix (iteration 9)

Withheld from the reviewer; for the human at sign-off.

## Base

Built on the worktree the driver handed me: `$PDCA_WORKTREE` at **`93a6f37`**
(`pdca-integration/main-r42c5348f203d`, the bundle's `stack-base`), not `main @ 9b1a3e2`. The
patch adds two files, one `lib.rs` line and the six 0017 hunks iteration 8 carried; it depends on
nothing that differs between the two bases. `Cargo.lock` and `deny*.toml` are untouched. The
patch also applies cleanly on `main @ 9b1a3e2` (checked with `git apply --cached --check`
against a temporary index read from that commit; `9b1a3e2` is on `origin/main` in this clone).

## What changed from iteration 8 (the carry-forward)

Started from `iteration-v8/patch.diff`, as the carry-forward directs ("everything else
stands"); applied it to the worktree with `git apply` (clean), then made one grading fix and the
tests for it, and trimmed for the size budget. No fixture or check the brief names was dropped.

**The bug.** Iteration 8's `classify_unsupported` judged a `DeclaredCode::NoBody` row
(HeadBucket's declared `400` with no body) like a `Code` row: a `Body::Cut` after one or more
bytes fell into the generic "cut off before its code" arm (unsettled), and a whole non-empty
`Body::Bytes` fell through to the code match and passed when the client had read
`ErrorCode::NoBody`. Both are bodies on an answer that declares none, so by (B) both are
`protocol`.

**The fix** (`crates/validate/src/matrix.rs:827-829`): on the declared status, before the
generic body arms, a row declaring no body runs the existing `expect_no_body` check
(`:938-957`), the same check every bodiless `required` answer takes (a PUT's 200, a HEAD, a 304,
a 204, the follow-up 404):

- `Body::Bytes(non-empty)` → `protocol` ("answered 400 with a body of N bytes");
- `Body::Cut { Arrived::Count(n > 0) | Arrived::Bytes(non-empty) }` → `protocol`;
- `Body::Unreadable`, `Listing`, `DeleteResult` → `protocol` ("answered 400 with a body");
- `Body::Empty` or `Body::Bytes(b"")` → passes on to the code match (then `Pass` for
  `ErrorCode::NoBody`, `protocol` for any other code, as before);
- `Body::Cut` after zero bytes → unsettled ("cut off after the head").

Its rustdoc states the order (`:811-814`).

**Shape of the fix.** To use `?` on `expect_no_body` I turned `classify_unsupported` into
`check_unsupported(..) -> Result<(), RowResult>` (`:815-858`), the shape `check_required`
already had, with `head(answer)?` (`:860-865`) in place of its own `NoAnswer` match. `classify`
(`:794-809`) now folds either check with `checked.err().unwrap_or(RowResult::Pass)`, which let
me drop the `classify_required` wrapper. The alternative was a nested
`if code == NoBody { if let Err(r) = expect_no_body(response) { return r; } }` inside the
`RowResult`-returning function: 5 lines rather than 3, and a second early-return idiom in a
function whose siblings all use `?`. Net: −8 lines in `matrix.rs`. Edition is 2021
(`Cargo.toml:39`), so no let-chain lint applies either way; clippy is clean.

**Tests for it** (carry-forward: "row and run tests for these cases in both arrival forms;
extend `head_bucket_passes_on_a_400_with_no_body_only` and include HeadBucket in the bodiless-
response test"), `crates/validate/tests/capability_matrix.rs`:

- `hb400(b)` (`:131-134`), shared by both tests: HeadBucket answered its declared 400, code read
  as `ErrorCode::NoBody`, body `b`.
- `head_bucket_passes_on_a_400_with_no_body_only` (`:467-500`): `Body::Empty` and `bytes(b"")`
  pass; `InvalidRequest` code and a 404 stay `protocol` (as before); then for each of
  `bytes(b"x")`, `cutb(b"x")`, `cutn(1)`, `bad()`: the row classifies `protocol` **and** the run
  FAILs naming HeadBucket/`protocol` (`fails`); `bytes(b"xy")` reports "answered 400 with a body
  of 2 bytes"; `cutb(b"")` and `cutn(0)` are unsettled at the row and INCONCLUSIVE in the run;
  and with no code read at all, `cutn(3)` is `protocol`, `cutn(0)` unsettled.
- `a_bodiless_answer_is_protocol_on_any_byte` (`:913-954`) now lists HeadBucket's 400 beside the
  five bodiless required answers: `hb400(cutn(0))` unproved; `hb400(cutn(1))`,
  `hb400(cutb(b"x"))`, `hb400(bytes(b"x"))` FAIL `protocol`; `hb400(bytes(b""))` passes.

## Size budget (100,000 bytes)

Iteration 8 was 99,724; with the fix and its tests the first green build was **101,269**. Cuts,
by yield, none of them a fixture or a check:

- Test helpers shared (≈ 1,100 bytes): `unproved_as` (`:341-349`) parametrised by the
  `Unproved` kind, so `unproved` and the new `no_answer` (`:351`, `:355`) replace five
  hand-written `assert!(matches!(v.unproved[..], [(Op, Unproved::X { .. })]))` blocks
  (`:525-531`, `:1213-1234`, `:991-1003`); `hb400` in place of a six-line closure; a `put_b`
  closure and an `h(len, etag)` closure (`:796-802`) where rustfmt had split `fails(put(..))`
  / `fails(head(..))` over four lines each; the "above `0.1.0`" versions as one split string
  (`:414-423`); two `matches!` blocks as `let` bindings (`:1101-1110`).
- `matrix.rs` rustdoc that restated the brief (≈ 400 bytes): module doc `:3-16` (the rule as
  built is still stated, 5xx included); `PRE_0_1_ROWS` doc `:251-254`; `Body::Unreadable` doc;
  the `Exercise` doc; the `check_delete_objects` comment `:1170-1172`.
- Code (≈ 300 bytes): `FailureClass::name()` folded into its `Display` (`:737-746`; nothing
  called `name()`, the test uses the enum values); `parse_version`'s build-metadata and
  pre-release identifier checks share one `ident` closure (`:396-399`, `:415`); the
  `classify_required` wrapper dropped (above).
- Test comments restating the brief (≈ 200 bytes): `:636`, `:677`, `:915-916`.

Final: **99,636 bytes** (`wc -c results/issue_937/patch.diff`). 39 legs, as iteration 8.

## Where each carry-forward and brief item lives in the test

Unchanged from iteration 8's mapping except the two tests above; the (A)–(E) mapping in
`iteration-v8/build-notes.md` §"Where each brief item lives" still holds leg for leg (39 legs,
same names). The HeadBucket cases are in `head_bucket_passes_on_a_400_with_no_body_only` and
`a_bodiless_answer_is_protocol_on_any_byte`.

## Demonstrated reds (each: temporary edit to `matrix.rs`, run, restore; sha256 verified)

Driver: a Python script (`$PDCA_SCRATCH/pdca-builder-937-it9-reds/reds.py`, removed at the
end of this beat) that applies one exact-string substitution, runs the test, restores the
original, and checks the file's sha256 before and after (`b3d7a206…cd7e1d`, identical). The
test ran as `cargo test -p wyrd-validate --test capability_matrix` from the worktree — the
exact target `engine/scripts/run-verify.sh:408` assembles for an added test and the one the
brief's *Falsifiability* names — under the tool's own timeout; the configured gate
`./engine/xtask.sh ci` ran once afterwards on the restored tree (*Gate*, below). All 39 legs pass
unmutated; every mutation turned at least one leg red (`test result: FAILED`). One slip in my
driver's labelling, for honesty: its "compile error" regex also matched cargo's standard
`error: test failed, to rerun pass …` trailer, so the raw log says "COMPILE ERROR" on every row;
each row's `N passed; M failed` shows the mutant compiled and ran. Line numbers are the worktree's.

| # | temporary edit | failing legs |
|---|---|---|
| 1 | 2xx on an unsupported row returns `Ok` (`:820`) | `any_2xx_to_an_unsupported_row_is_unexpected_support`, `any_fatal_fails_the_run_naming_every_fatal_row`, `each_version_selects_its_own_profile_and_expectations` |
| 2 | 2xx head with a cut body returns `NoAnswer` (`:817-818`) | `any_2xx_to_an_unsupported_row_is_unexpected_support` |
| 3 | PUT ETag check `!etag.is_empty()` (`:995`) | `a_bodiless_answer_is_protocol_on_any_byte`, `any_fatal_fails_the_run_naming_every_fatal_row`, `put_passes_only_on_200_with_a_strong_etag_after_the_body_ended` |
| 4 | non-matching conditional GET returns `Ok` on 304 (`:1060`) | `conditional_get_with_the_wrong_status_or_a_body_on_its_304_is_protocol` |
| 5 | a key with no probe is not unsettled (`:1179`) | `delete_objects_document_must_name_every_key_once_with_no_error_row`, `every_follow_up_is_judged_against_the_key_it_asked_about` |
| 6 | `select` filters `\|_\| true` (`:481`) | `a_version_no_profile_covers_selects_none_and_says_why`, `each_version_selects_its_own_profile_and_expectations`, `pre_0_1_covers_every_version_below_0_1_0` |
| 7 | `+git.unknown` check never matches (`:430`) | `unknown_is_never_read_as_release_0_0_0` |
| 8 | a `supported, not checked` row noted `NotExercised` (`:1325-1328`) | `a_not_checked_row_is_listed_not_covered_and_never_passes`, `an_unexercised_undeclared_or_unanswered_row_is_inconclusive` |
| 9 | `request_body_ended` ignored (`:991`) | `a_bodiless_answer_is_protocol_on_any_byte`, `put_passes_only_on_200_with_a_strong_etag_after_the_body_ended` |
| 10 | follow-up with no answer → `oracle` (`:1121`) | `a_delete_follow_up_is_judged_on_its_status`, `a_lost_or_unacknowledged_delete_result_never_hides_the_probes`, `an_unexercised_undeclared_or_unanswered_row_is_inconclusive`, `any_fatal_fails_the_run_naming_every_fatal_row`, `every_follow_up_is_judged_against_the_key_it_asked_about` |
| 11 | `head()` turns a 5xx into unsettled (`:863`, iteration 7's branch restored) | 20 legs (19 passed): `a_5xx_to_a_required_row_is_protocol`, `a_lost_or_unacknowledged_delete_result_never_hides_the_probes`, `unsupported_row_passes_only_on_the_declared_status_and_code`, … — `head()` now also serves `check_unsupported`, so the unsupported rows' 5xx fixtures go red too |
| 12 | non-matching check ignores `sent` (`:1055`) | `conditional_get_settles_nothing_without_the_tag_its_form_needs` |
| 13 | 404 to a written key → `protocol` (`:880`) | `a_404_to_a_read_of_a_written_key_is_oracle` |
| 14a | object-body cut unsettled without reading the prefix (`:918`) | `a_cut_off_object_body_is_judged_by_the_bytes_that_arrived` |
| 14b | bodiless-answer cut unsettled whatever arrived (`:943`) | `a_bodiless_answer_is_protocol_on_any_byte`, `conditional_get_with_the_wrong_status_or_a_body_on_its_304_is_protocol`, `head_bucket_passes_on_a_400_with_no_body_only` |
| 15 | unreadable error body treated like a cut (`:837`) | `unsupported_row_with_an_unreadable_error_body_is_protocol` |
| **16** | **the carry-forward bug: the `NoBody` arm removed (`:827-829`), i.e. iteration 8's grading restored** | `a_bodiless_answer_is_protocol_on_any_byte`, `head_bucket_passes_on_a_400_with_no_body_only` |

## Refuting my own test

- **(a) Genuine red?** Yes. For this iteration's change, mutation 16 *is* the fix reverted:
  with the `NoBody` arm removed the two extended legs fail (2 failed, 37 passed); restored, 39
  pass. For the module as a whole the posture is the net-new one the brief records: `matrix` does
  not exist on the base, so with production reverted the test target does not compile and
  C4-verify reports UNVERIFIABLE (accepted at iteration 7's sign-off). The binding evidence is
  the 16 mutation runs above, each turning a named behaviour red.
- **(b) Production path?** Yes. Every leg calls `wyrd_validate::matrix::{classify, verdict,
  parse_version, select, select_reported, is_strong_etag}` and the shipped `PRE_0_1` /
  `PROFILES` constants; `hb400`, `unproved_as` and the other helpers only build fixtures and
  assert on what production returns — no copy of any check.
- **(c) Fixture includes the fault?** Yes. Each FAIL fixture replaces exactly one exercise of an
  otherwise passing run (`verdict_with`) and asserts the run FAILs naming that row and class;
  each INCONCLUSIVE fixture asserts no fatal, exactly one unproved row, and that it is the
  exercised one with the expected `Unproved` kind. The HeadBucket fixtures feed the fault the
  reviewers found — a body (whole or cut) on the answer the client read as "no body" — and the
  run FAILs on it.

## Commit-readiness

- `cargo fmt -p wyrd-validate` applied; both files are as rustfmt wrote them.
- `cargo clippy -p wyrd-validate --all-targets -- -D warnings`: clean.
- `typos` over the three touched files: clean. Docs renderer deps (`markdown_it`, `yaml`)
  present, so `cargo xtask ci`'s prose gates run rather than warn-skip.
- No `Cargo.toml`, `Cargo.lock`, `deny*.toml`, gateway or `05-building-block-view.md` change
  (out of scope per the brief).

## Gate

`./engine/xtask.sh ci` (the configured C4-ci runner, `pdca.toml` `[gates] runner`, subcmd `ci`)
ran on the worktree after the reds, with `matrix.rs` restored (sha256-verified): **every step
green except the two `cargo deny` advisory steps**. xtask's summary: "2 checks failed: cargo
deny check; cargo deny --all-features --config deny-all-features.toml check advisories". Inside
it, `tests/capability_matrix.rs` ran its 39 legs green (`ci.log:2645-2647`). Both deny steps
name the same two advisories, RUSTSEC-2026-0385 (non-constant-time mask generation) and
RUSTSEC-2026-0386 (`AsyncFd::new` I/O safety), against dependencies this patch does not touch
(`git diff --stat`: `lib.rs`, `matrix.rs`, `capability_matrix.rs`, the 0017 draft; no
`Cargo.toml`, `Cargo.lock` or `deny*.toml`). This is the base's red the brief predicts for
`93a6f37` and the human accepted as inherited at iteration 7's sign-off; the brief says it is not
this slice's to fix. Recorded with the standing token so the driver matches the registered
doctor row (no new row proposed):

NEEDS-HUMAN external dependency: cargo deny all-features advisories — the unpatched base `93a6f37` fails both cargo-deny advisory steps of C4-ci on RUSTSEC-2026-0385 / RUSTSEC-2026-0386; the gate's fmt, clippy, build, test (incl. this test's 39 legs), prose and conformance steps are green, so C4-ci cannot read green on this base until the advisory is recorded or cleared on `main`.

## Citations (target branch, this worktree at `93a6f37`)

- The fix: `crates/validate/src/matrix.rs:794-809` (`classify`), `:811-858`
  (`check_unsupported`; the `NoBody` arm `:827-829`), `:860-865` (`head`), `:938-957`
  (`expect_no_body`).
- Gateway lines every `pre-0.1` row cites are unchanged from iteration 8 and were read on this
  base: `crates/gateway-s3/src/lib.rs:121`, `:410-422`, `:1694-1699`, `:1766-1788`,
  `:1797-1804`, `:1809-1815` (HeadBucket's 400 with no body), `:1819-1826`, `:1842-1849`,
  `:1865-1871`, `:2008`/`:3401`, `:2017`/`:2469`, `:2025`/`:2570`, `:2026-2028`, `:2293-2300`,
  `:2713-2719`, `:2785-2803`, `:2995`.
- Version shapes: `crates/server/src/version/derivation.rs:33-68`, `:65`, `:118-124`.
- The one `s3` dependency: `ErrorCode`, `crates/validate/src/s3/error.rs:68-76`, re-exported at
  `crates/validate/src/s3.rs:60` and `crates/validate/src/lib.rs:34-36`.
- Proposal 0017 hunks: carried from iteration 8 unchanged (§3 states, version keying, the
  `pre-0.1` paragraph; §7 table rows and the 5xx sentence; *Dependencies*).
- Rubric self-review (`AGENTS.md:122-199`): no clock read; no shared mutable state (consts
  only); grammar strictness kept (`is_strong_etag` `:1222`, `parse_content_range` `:1233`,
  `parse_numeric` `:446`: every token checked, digits-only before `parse`); absent or wrong
  entries produce explicit results, never silent success; the test drives production; docs
  currency: 0017 amended in-PR, `05-building-block-view.md` is 743.3's by the brief.

## For the human

- **Dropped renderings (carried from iteration 8):** `Display for RunVerdict` and
  `Display for Operation` are still out for size; `FailureClass`'s `name()` is now its `Display`
  only (`format!("{class}")` gives the §7 spelling). 743.3 prints a verdict and will add a
  rendering then.
- **NEEDS-HUMAN carried from the brief (not this slice's):** a PUT or DELETE 2xx the base client
  reports as `S3Error::Unreadable` cannot be told apart as "a byte where the success has none"
  vs "framed then cut before its first byte"; flagged for 743.3's mapping in the brief's
  *Design*. The matrix holds either (`Body::Unreadable` → `protocol`, `Body::Cut` after no byte
  → unsettled).
- No external dependency beyond the brief's (`typos`, `docs-renderer`) was needed.
- **Orphan scratch dirs, not mine:** `$PDCA_SCRATCH` holds `pdca-builder-937-it4-*` …
  `-it7` and `-redlegs` directories from earlier iterations; I left them and removed only the
  one this session created (`pdca-builder-937-it9-reds`).
