# Build notes — #1005 (743.1a): capability matrix core

## Base

- Worktree `HEAD` = `376b106248d2ac2e816c5d258609e04de09a425f` (merge of PR #990 on `main`).
- `git merge-base --is-ancestor 9b1a3e2 HEAD` → exit 0, so the base carries #744, #853 and the
  two cargo-deny fixes. Built on it as-is.
- `git diff --stat 9b1a3e2 HEAD -- crates/gateway-s3/src/lib.rs crates/server/src/version/derivation.rs crates/validate docs/design/proposals/draft/0017-blackbox-validation-tool.md`
  is empty, so every `9b1a3e2` line number the brief cites is still right on this base. I
  re-read each cited range before using it.
- `Cargo.lock` and `deny*.toml` are untouched (`git status` shows only the four files below).

## What changed

| File | Change |
|---|---|
| `crates/validate/src/lib.rs:24` | `pub mod matrix;` (the one line the brief allows) |
| `crates/validate/src/matrix.rs` (new, 1140 lines) | the module |
| `crates/validate/tests/capability_matrix.rs` (new, 1196 lines, 30 tests) | the test the brief names |
| `docs/design/proposals/draft/0017-blackbox-validation-tool.md` | §3, §7 and *Dependencies* amendments |

### `matrix.rs`, by part

- **Carried from #937 iteration 9 unchanged** (review found no defect there): `Operation`
  (`:32`), `Row` and its constructors (`:123-164`), `Bound` / `VersionRange` / `Profile` with the
  const duplicate-operation panic (`:175-247`), `PRE_0_1` / `PROFILES` (`:340-350`), versions
  and selection (`parse_version :426-481`, `select :515-529`, `select_reported :532-535`,
  `NoProfile`), the observation types other than the two changes below, `FailureClass` and its
  Display (`:775-797`), `RowResult` (`:800-820`), the verdict types and the fold's structure
  (`:1021-1140`), `is_strong_etag` (`:1008-1016`, kept public with its grammar test).
- **Not carried**: iteration 9's `check_required` and helpers (`expect_status`,
  `expect_read_status`, `expect_bytes`, `expect_no_body`, `whole_document`, `worst`,
  `probe_deleted`, `check_delete_objects`, `check_delete_result`, `parse_content_range`): lines
  885-1266 of iteration 9's `patch.diff`, about 380 lines. They are child-2's.
- **New: one ladder** (brief (B)).
  - `classify` (`:897-920`) is steps 1 and 2, then builds an `Evidence` and calls `settle`.
  - `Evidence` (`:843-857`) holds the findings in one `Vec` per message stage: `request`,
    `status`, `headers`, `body`, `follow_ups`, plus `open` for what leaves a row unproved
    whatever arrived.
  - `Evidence::settle` (`:863-885`) is steps 3–5. It reads the stages in message order, and the
    first `Fatal` wins. Only when there is none does the first `Unproved` / `NotChecked`
    decide. Otherwise the result is Pass.
  - A check never returns early. Each check only *adds* to the stage its evidence comes from,
    and `settle` runs after all of them. So no check can skip a later one: the shape (B) step 3
    forbids is not expressible in this design.
- **New: `shared_facts`** (`:926-935`): the 5xx fact, run on every row before its state's
  checks. A 5xx is `protocol` unless the row is `unsupported` and declares exactly that status.
  The 404 fact is child-2's. A comment says so; it is not stubbed.
- **New: `refusal`** (`:941-996`): the `unsupported` / `HeadBucket` checks, rewritten into the
  stage form. Status stage: any 2xx → `unexpected-support`, any other status but the declared
  one → `protocol`. Body stage: the shape check (an `<Error>` document, or no byte for
  `HeadBucket`), then the code check. The code check still runs after a cut whenever a code was
  read. It reads no header and no `Content-Length` (brief (D)).
- **New: `required`** (`:1001-1005`): the seam. It pushes one `Unproved("required-row checks
  not yet defined")` into `open`. Child-2 replaces its body with pushes into
  `request`/`status`/`headers`/`body`/`follow_ups`. It does not have to touch `classify`,
  `settle`, the stage order, or any public type. Child-2 will probably widen this private fn's
  signature (to take the exercise and response); that is private and allowed.
- **`supported, not checked` rows** (`:914-916`) go through steps 1 and 2 and the 5xx fact, then
  get `NotChecked { issue }`: "a 5xx is `protocol`; anything else is not checked" (decided
  2026-10-25). Iteration 9 returned `NotChecked` before any of that, so a 5xx there was never a
  pass but was never fatal either.
- **The fold** (`verdict`, `:1067-1140`): the fatal arm comes first. Next, a guard arm
  (`:1088`) ignores every non-fatal result on a row that need not be proved (`must_prove`,
  `:166-168`). That keeps (H)'s "a run can PASS without it" true now that a not-checked row can
  return `NoAnswer` (step 2). The `NotChecked` arm (`:1093`) cannot be reached on a checked row.
  It fails closed (noted unproved) rather than panicking.

### Choices I made that the brief left open, or where I left iteration 9's code

1. **`DeclaredCode::matches` (`:73-80`): `NoBody` now also matches `error_code: None`.**
   Iteration 9 required `Some(ErrorCode::NoBody)`. Brief (D) says `HeadBucket` passes on "the
   declared status with no body". Before this change, a 400 with `Body::Empty` and no code read
   was `protocol`, a FAIL on an answer that is exactly the declared one. A code that *was* read
   (`Code` / `MissingInXml`) still fails, because a code means an `<Error>` document arrived.
   Code rows are unchanged: no code read is `protocol` (brief (C): "a declared error code other
   than the row's, or none").
2. **Observation type: two additions beyond iteration 9.**
   - `GetObjectIfNoneMatchHit` gains `object: Vec<u8>` (`:683-688`). The brief names this one.
   - `DeleteObject` is now `{ key, answer, follow_ups: Vec<(String, Answer)> }` (`:707-711`).
     Iteration 9 had `{ answer, follow_up: Answer }`.
   - Why the `DeleteObject` change: *Design* says "A delete carries each follow-up `HEAD` as
     its own answer-or-no-answer, with the key it asked about". Iteration 9's single
     `follow_up` had no key, and had no room for a retry. Child-2 changes no public type, so it
     has to be right here.
   - **This is the one place I went past the letter of the brief's "new against iteration 9"
     list. Flag it for the human.** If child-2's planner expects iteration 9's shape, child-2
     must use this one instead. The tuple shape matches `DeleteObjects`, which review already
     accepted.
3. **`impl Display for Expected`** (`:107-120`). The 5xx detail names the row's state:
   `"answered 500, a 5xx the row does not declare; the row is unsupported(#766), refused 501
   NotImplemented"`.
4. **Citations.** `PRE_0_1_ROWS` (`:284-337`) uses the brief's lines:
   - ranged: `2704-2725`;
   - `PutObject`: `:3401-3404`;
   - `HeadBucket` adds `:1548-1553`, where `finish_response` treats every HEAD as bodiless (that
     is why its 400 carries no body);
   - every other row as in iteration 9.

   I re-read each cited range on `376b106`.
5. **Fixtures.** No test sends a typed `required` exercise through the verdict and depends on
   its result, except at a 5xx or with no head. Child-2 changes what those exercises return, so
   asserting on them here would force child-2 to edit this file. The success criterion forbids
   that.
   - Where a test needs an unproved `required` row, it uses `Exercise::Other` (carries no check
     of its own) or `Answer::NoAnswer`. That is brief (H)'s rule.
   - Typed `required` exercises are used only where child-2 cannot change the answer: at a 5xx
     (`a_5xx_is_protocol_on_every_row_unless_the_row_declares_that_status`, asserting the class
     only, not the detail) and in the observation-type round trip.

### Alternatives I ruled out for the ladder's representation (costs, concretely)

- **Stage-tagged findings, sorted** (`enum Stage { Request, Status, Header(u8), Body,
  FollowUp(usize) }`, `findings.sort_by_key(stage)`):
  - In this child nothing builds `Request`, `Header` or `FollowUp`. The workspace has
    `warnings = "deny"` (`Cargo.toml` `[workspace.lints.rust]`), so `dead_code` fails the build.
  - The fix would be an `#[allow(dead_code)]` that child-2 must remove. That is a one-line edit
    child-2 has to remember, plus the sort.
  - The per-stage `Vec` fields have no such cost: rustc reads them in `settle`, so empty is
    fine.
- **Data tables of `fn` pointers per row state** (`struct Table { request: &'static [Check],
  … }`): by my estimate about 40 more lines (a `Check` type alias, a `Table` struct with an
  `EMPTY` const, a `table(row)` dispatcher). Each check would also have to dig its declared status and code back
  out of `row.expected` with a `let … else` that silently skips if mis-wired. It gains nothing
  over pushing into named stages.
- **Per-arm ordered calls** (each row state writes its own sequence of checks): this puts the
  order in every arm, which is exactly the "re-deciding the order" the brief rules out.

## How the test maps to the success criterion

The test is `crates/validate/tests/capability_matrix.rs` (30 tests).

- **(A)**
  - `pre_0_1_declares_exactly_todays_rows` (`:278`) checks the exact rows and states, and the
    gateway path and refusal lines each row cites.
  - `pre_0_1_covers_every_version_below_0_1_0` (`:350`) covers `0.0.0+git.abc1234`,
    `0.1.0-rc.1` and `0.1.0-rc.1+git.3.abc1234[.dirty]`.
  - `a_profile_cannot_declare_an_operation_twice` (`:365`).
- **(B)**
  - `an_exercise_of_another_row_is_unsettled_on_every_row_state` (`:376`)
  - `no_response_head_is_no_verdict_on_every_row_state` (`:398`, including follow-ups not
    judged)
  - `the_first_failing_check_in_message_order_names_the_class` (`:428`)
  - `nothing_that_leaves_an_exercise_unproved_stops_a_later_check` (`:475`)
- **(C)**
  - `the_fatal_classes_print_as_0017_names_them` (`:518`)
  - `a_5xx_is_protocol_on_every_row_unless_the_row_declares_that_status` (`:530`: a `required`
    row with every typed form plus `Other`; an `unsupported` row; a `supported, not checked`
    row; and a test-local `unsupported` row declaring `503` that passes on it)
  - `a_required_rows_exercise_with_a_head_and_no_check_of_its_own_is_unsettled` (`:587`)
- **(D)**
  - `unsupported_row_passes_only_on_the_declared_status_and_code` (`:600`)
  - `head_bucket_passes_on_a_400_with_no_body_only` (`:653`)
  - `any_2xx_to_an_unsupported_row_is_unexpected_support` (`:694`)
  - `unsupported_row_with_its_error_body_cut_off_is_unsettled` (`:732`)
  - `unsupported_row_with_an_unreadable_error_body_is_protocol` (`:750`)
  - `an_unsupported_row_reads_no_header` (`:773`)
  - `a_not_checked_row_is_never_a_pass` (`:792`)
- **(E)**
  - `parse_reads_releases_and_ignores_build_metadata` (`:825`)
  - `unknown_is_never_read_as_release_0_0_0` (`:847`)
  - `parse_refuses_malformed_versions` (`:862`)
  - `each_version_selects_its_own_profile_and_the_rows_flip_with_it` (`:885`: a declared `501
    NotImplemented` passes the `unsupported` row and is `protocol` on the `required` one; PASS
    vs FAIL at the verdict too; no 2xx used)
  - `a_version_no_profile_covers_selects_none_and_says_why` (`:910`)
- **Observation type**: `the_observation_type_records_every_answer_and_ask` (`:949`) builds every
  typed form with every field and every body form.
- **(H)**
  - `a_run_proving_every_checked_row_passes_and_lists_the_not_checked_row` (`:1024`): the PASS
    leg, against a test-local profile of `unsupported` rows plus one `supported, not checked`
    row.
  - `any_fatal_fails_the_run_naming_every_fatal_row` (`:1057`)
  - `an_unproved_required_row_keeps_the_run_inconclusive` (`:1091`)
  - `an_unexercised_undeclared_or_unsettled_row_is_inconclusive` (`:1125`)
  - `a_row_is_proved_only_when_every_exercise_of_it_passed` (`:1163`)
  - `a_run_that_proves_no_row_is_inconclusive` (`:1179`)

The file does not assert that a full `pre-0.1` run is INCONCLUSIVE (brief (H)). Every assertion
against `PRE_0_1` is one child-2 cannot flip: a FAIL from a fatal answer, or an INCONCLUSIVE
caused by an unexercised, no-head or `Other` row.

## Refutation (forced)

**(a) Genuine red?** Yes.

- With the production module removed, the test cannot compile: net-new, so C4-verify will report
  UNVERIFIABLE (exit 77), as the brief's *Verification posture* expects.
- In its place, I broke each rule one at a time in `matrix.rs` and reran
  `cargo test -p wyrd-validate --test capability_matrix`.
- Each mutation was one scripted text replacement (each one is given in the table). I restored
  the file from a backup afterwards: SHA-256 `0c2b3a2b…b8c2` before and after. The scratch
  copy and the logs were deleted once the failing assertions below were copied out.

| Rule removed | Result | The assertion that fails |
|---|---|---|
| **unexpected-support**: `refusal` ignores a 2xx (`if false {…} else if got != status && !response.is_success()`) | 4 failed | `any_2xx_to_an_unsupported_row_is_unexpected_support` at `capability_matrix.rs:710`: `CopyObject: … status: 200 … body: Empty` → `left: Some(Protocol)`, `right: Some(UnexpectedSupport)`. A 2xx with the declared `<Code>` read (`refusal(200, "NotImplemented")`) would then pass outright. |
| **wrong code passes**: `(Self::Code(_), Some(ErrorCode::Code(_))) => true` | 2 failed | `unsupported_row_passes_only_on_the_declared_status_and_code` at `:614`: `status: 501 … error_code: Some(Code("InvalidRequest"))` → `left: None` (passed), `right: Some(Protocol)` |
| **`+git.unknown` read as `0.0.0`**: drop the `UNKNOWN_BUILDS` check in `parse_version` | 2 failed | `unknown_is_never_read_as_release_0_0_0` at `:854`: `0.0.0+git.unknown` → `left: Ok(Release(Release { major: 0, minor: 0, patch: 0, pre: [] }))`, `right: Ok(Unknown)` |
| **unsettled `required` folded into PASS** (the fold drops `Unsettled`) | 7 failed | `an_unproved_required_row_keeps_the_run_inconclusive` at `:1115`: `RunVerdict { outcome: Pass, proved: [CopyObject, HeadBucket, PutObject], … }` → `left: Pass`, `right: Inconclusive` |
| **required row passed by the ladder** (`required()` pushes nothing) | 3 failed | same assertion at `:1115`, `outcome: Pass` |
| **cut after a 2xx hides the 2xx** (iteration 9's early-return shape: a `Body::Cut` returns before the status is judged) | 5 failed | `any_2xx_to_an_unsupported_row_is_unexpected_support` at `:710`: `status: 200 … body: Cut { Count(0) }` → `left: None`, `right: Some(UnexpectedSupport)`; and `nothing_that_leaves_an_exercise_unproved_stops_a_later_check` at `:485` |

**(b) Production path?** Yes. Every leg calls the public `wyrd_validate::matrix` functions
(`classify`, `verdict`, `parse_version`, `select`, `select_reported`, `is_strong_etag`,
`Profile::new`) and the shipped `PRE_0_1` / `PROFILES` constants. The test file has no copy or
mock of any of them. The test-local profiles are built with the production `Profile::new` /
`Row::*` constructors.

**(c) Fixture includes the fault?** Yes. Each fatal leg feeds the actual wrong answer:
- a 2xx (bare, cut, unreadable, with a clean `<Error>`);
- a wrong or missing code;
- a 5xx;
- a body byte on `HeadBucket`;
- an unreadable or other-document error body;
- `+git.unknown[.dirty]`;
- a `required` row left unproved by an `Other` exercise or no head.

None of these is filtered out. The PASS legs are the only ones without a fault, and each has a
red twin in the same test.

## Gates run locally (all from the worktree)

| Step | Result |
|---|---|
| `cargo fmt -p wyrd-validate -- --check` | clean |
| `cargo clippy -p wyrd-validate --all-targets` (workspace lints, `warnings = deny`) | clean, after one fix (`clippy::cloned_ref_to_slice_refs` in the test) |
| `cargo test -p wyrd-validate --test capability_matrix` | 30 passed |
| `cargo test -p wyrd-validate` (all 7 targets, including the in-process gateway round trips) | all passed |
| `typos` (repo root) | clean |
| `python3 docs/publishing/tools/lint_docs.py` | OK |
| `python3 docs/publishing/tools/render_site.py --check` | link audit OK |

- **Runner caveat.** The project runner `./engine/xtask.sh ci` only runs the whole gate: there
  is no single-test entry point, and the full run includes DST and cargo-deny. So for the
  red→green check I ran the one target the brief's *Falsifiability* names
  (`cargo test -p wyrd-validate --test capability_matrix`, the same target `cargo xtask ci`'s
  test step builds), with a `timeout` wrapper. I did not run the whole `cargo xtask ci`; Check's
  C4-ci gate does.
- **Existing failure, outside this change.** `cargo doc -p wyrd-validate --no-deps` fails, but
  only on `crates/validate/src/sizes.rs:340`, a public doc linking to the private
  `Tuning::check`. `cargo xtask ci` does not run `cargo doc`, and the new module adds no rustdoc
  warning.
- **External dependencies.** The two the brief names are present: `typos-cli 1.48.0`, and
  `python3 -c "import markdown_it, yaml"` succeeds. I found no undeclared dependency, so there
  is no NEEDS-HUMAN item.

## Rubric self-review (`AGENTS.md` *Review rubric & protocol*)

- **One clock**: no clock is read.
- **Seams / dependency direction**: the only crate type used is `crate::s3::ErrorCode`, and no
  `wyrd-*` dependency was added.
- **Validation boundaries**: a duplicate operation is refused at construction, and a malformed
  version is an error, never a value.
- **No shared mutable global state**: the profiles are `const`.
- **`#![forbid(unsafe_code)]`**: on the new test crate root; `lib.rs` already has it.
- **Docs currency**: no port, API operation, RPC, CLI flag or persisted field is added. The
  brief puts `05-building-block-view.md` out of scope, and 0017 is amended as specified.
- **Protocol input**: a cut is unsettled, never accepted.
- **Grammar strictness**: the SemVer tokens are checked, with no sign accepted via `from_str`;
  carried from iteration 9.
- **Absent entries**: an undeclared operation and an unexercised row are both INCONCLUSIVE; the
  unreachable fold arm fails closed.
- Transactions, await discipline and probes: not touched.

## For the human at sign-off

- The `DeleteObject` shape change (item 2 above) is the one call that binds child-2 beyond what
  the brief listed. Accept it, or say so and child-2's brief can name it.
- The `HeadBucket` `None`-code leniency (item 1) is a deliberate change from iteration 9's
  behaviour, made to follow (D)'s wording.
