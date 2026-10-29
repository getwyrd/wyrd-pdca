# Build notes — #937 (743.1): the capability matrix

## Base

Built on the worktree `$PDCA_WORKTREE` = `/home/eddie/wyrd/wyrd.pdca-wt-l1`, the integration line
`pdca-integration/main-r42c5348f203d` at tip `93a6f37` (`results/issue_937/stack-base-tip`):
`main` plus #744, #853 and #932. Nothing from those is used. The matrix imports only
`crate::s3::ErrorCode` (`crates/validate/src/s3/error.rs:69-76`, re-exported at
`crates/validate/src/s3.rs:59`), which the brief allows.

**#508 is not on this base.** The denylist still lists `uploads`, `uploadId` and `partNumber`
(`crates/gateway-s3/src/lib.rs:410-413`), and the object-path guard still refuses them
(`:1842-1849`). #932 (`6a85eaa`, staging a multipart part's bytes) is core-only. So the six
multipart rows are `unsupported(#508) → 501 NotImplemented`, per (A).

Every gateway citation in the brief was re-checked on this base and holds at the brief's line
numbers. CopyObject `:1865-1871`, denylist `:410-422`, object guard `:1842-1849`, bulk delete
`:1766-1788`, bucket denylist `:1797-1804`, bucket other method `:1809-1815`, root path
`:1819-1826`, 206 `:2713-2719`, 304 `:2785-2803`, DeleteObjects 200 with errors `:2293-2300`,
`UNKNOWN_SERVER_VERSION` `:121`, `Server` header `:1699`. I also cited lines for the required
rows the brief did not list: PUT 200 `:2008` (`put_object_response` `:3401`), GET `:2017`
(`serve_get` `:2469`), HEAD `:2025` (`serve_head` `:2570`), DELETE 204 `:2026-2028`, listing
`:1806-1808` (`list_objects` `:823`, v2 `:868`, v1 shim `:855`), If-None-Match `:2995`.

## What changed

| File | Change |
|---|---|
| `crates/validate/src/matrix.rs` (new) | The whole module: `Operation`, `Expected` (three states), `Row`, `Profile`, `PRE_0_1`, `PROFILES`; `parse_version`, `select`, `select_reported`; the observation type (`Answer`, `Response`, `Body`, `Exercise`); `classify`; `verdict`. |
| `crates/validate/src/lib.rs:24` | `pub mod matrix;`, that one line. |
| `crates/validate/tests/capability_matrix.rs` (new) | 41 tests, legs (A) through (E); `#![forbid(unsafe_code)]` at `:5`. |
| `docs/design/proposals/draft/0017-blackbox-validation-tool.md` | The §3 amendment: the third state added to the state list (after `:195`); `:259-261` corrected (the `Server` header since #886; `--server-version` is now an override; unknown never reads as 0.0.0); a new paragraph after `:276` on `pre-0.1` being edited in place until the first tag. |

## Design decisions (and what I ruled out)

- **The observation type is per-row (`Exercise`), not one generic struct.** Each variant carries
  what the oracle expects (object bytes, the range, the PUT's ETag, the written keys) next to
  the answers. A mismatched exercise/row pairing then can't happen silently. `Exercise::answer()`
  gives the primary answer, so an `unsupported` row is judged the same way whatever variant fed
  it. That is what makes leg (C)'s "same answer flips the verdict" work: one `PutObject`
  exercise is a pass under a `required` row and `unexpected-support` under an `unsupported` one.
- **Judgment calls the brief didn't settle.** I picked the more cautious reading each time:
  - A 5xx other than 501 to a **required** row → `Unsettled` (§7 calls 5xx `availability`,
    which is budgeted and not fatal). A 501 to a required row is a refusal, so it is
    `protocol`. For **unsupported** rows any wrong status, a 5xx included, is `protocol`, as (B)
    says.
  - A 2xx head whose body then failed, on a **required** row → `Unsettled` (the bytes never
    arrived, so neither `integrity` nor a pass is proven). On an **unsupported** row it is
    `unexpected-support`, as (B) says.
  - An **unsupported** row whose status matches but whose error body failed before the code
    could be read → `Unsettled`. (B) only says "no verdict when no head arrived". This case
    can't prove the code is right, and calling it `protocol` would claim a wrong code nobody
    saw. If the status is wrong, it is `protocol` whatever the body did.
  - `DeleteObjects` with an `<Error>` row, or `<Deleted>` not naming exactly the requested
    keys → `protocol` (the brief names no class for the `<Error>` fixture; it is a wrong
    per-key error code / document content). A requested key with no follow-up `HEAD` →
    `Unsettled`. A fatal probe outranks an unsettled one. An empty key list → `Unsettled`.
  - `HeadObject` / GET with a wrong status (e.g. 404) → `protocol`; a 200 with the wrong size or
    ETag → `oracle`, as the brief says.
  - An exercise of an operation the profile doesn't declare → `Unproved::Undeclared` →
    INCONCLUSIVE. A `required` row fed only `Exercise::Other` (no check exists) → `Unsettled`.
  - A row with several exercises is proved only if every one passed.
  - **Overlapping profiles** → `NoProfile::Ambiguous`, not first-match. Picking one would be a
    guess.
- **Version grammar** is SemVer 2.0.0, every token checked: no leading zeros, no empty
  identifiers, identifier bytes `[0-9A-Za-z-]`, no `v` prefix, no whitespace, no `wyrd/`
  prefix (stripping the `Server` product token is 743.3's job). `+git.unknown` and
  `+git.unknown.dirty` are matched as the **whole** build metadata, so `+git.unknownx` (a real
  sha-like value) is still a release. Profile edges are `Bound` triples (const-constructible).
  A pre-release sorts below its release, so `0.1.0-rc.1` falls in `pre-0.1`.
- **ETag** is checked byte by byte against RFC 9110 §8.8.3 (`"` etagc* `"`, etagc =
  `%x21 / %x23-7E / %x80-FF`). `""` is grammatically a strong tag and is accepted. An empty
  *value* (no quotes) is refused.
- **Content-Range** is parsed strictly (unit case-insensitive, digits only), then compared
  numerically. That refuses `*/size`, missing size, signs and extra spaces.
- No clock reads anywhere in the module. No `static`, only `const`, so the statics gate has
  nothing to flag.

## Test location and runner

`crates/validate/tests/capability_matrix.rs`, the crate's existing integration-test directory.
A copy is in the bundle as `capability_matrix.rs`. The quick red→green runs used
`timeout 600 cargo test -p wyrd-validate --test capability_matrix` in the worktree. That is the
exact target the brief's Falsifiability names and that `cargo xtask ci` runs. xtask has no
single-test subcommand (`xtask/src/main.rs:119-150`), so I used `timeout` to bound it. The full
gate ran through the project wrapper `./engine/xtask.sh ci` (result below).

## Demonstrated reds (required by the brief's Verification posture)

Each was a temporary edit to `crates/validate/src/matrix.rs`, run, and restored; `cmp` against
the saved original confirmed the restore. The script lives at
`$PDCA_SCRATCH/pdca-builder-937-reds/reds.py`.

| # | Temporary edit | Result | Failing legs |
|---|---|---|---|
| 1 | 2xx to an unsupported row → `Pass` | 4 failed | `any_2xx_to_an_unsupported_row_is_unexpected_support`, `an_unexpected_success_fails_the_run`, `each_version_selects_its_own_profile_and_expectations`, `any_fatal_fails_the_run_naming_every_fatal_row` (B) |
| 2 | 2xx head whose body failed → `NoAnswer` | 1 failed | `any_2xx_to_an_unsupported_row_is_unexpected_support` (B) |
| 3 | PUT ETag check accepts any non-empty value | 2 failed | `put_etag_that_is_not_a_strong_tag_is_protocol`, `any_fatal_fails_the_run_naming_every_fatal_row` (B′) |
| 4 | Non-matching conditional GET accepts 304 | 1 failed | `non_matching_conditional_get_answered_304_is_protocol` (B′) |
| 5 | DeleteObjects passes on its document alone (probe loop skipped) | 3 failed | `delete_objects_document_alone_never_proves_the_delete`, `delete_follow_up_with_no_answer_or_a_5xx_is_inconclusive_not_oracle`, `an_unresolved_delete_names_the_delete_row` (B′) |
| 6 | Selector ignores the version (takes the first profile) | 4 failed | `each_version_selects_its_own_profile_and_expectations`, `a_version_no_profile_covers_selects_none`, `overlapping_profiles_select_none`, `pre_0_1_covers_every_version_below_0_1_0` (C) |
| 7 | `+git.unknown` read as release 0.0.0 | 1 failed | `unknown_is_never_read_as_release_0_0_0` (C) |
| 8 | `supported, not checked` row counted as unexercised | 1 failed | `a_not_checked_row_need_not_be_exercised_and_is_listed_as_not_covered` (D) |
| 9 | PUT check ignores when its head arrived | 1 failed | `put_answered_before_its_body_ended_is_protocol_whatever_its_etag` (B′) |
| 10 | Follow-up HEAD with no answer → `oracle` | 3 failed | `delete_follow_up_with_no_answer_or_a_5xx_is_inconclusive_not_oracle`, `an_unresolved_delete_names_the_delete_row`, `any_fatal_fails_the_run_naming_every_fatal_row` (B′)/(E) |

Restored tree: `test result: ok. 41 passed; 0 failed`.

## Self-refutation (the three forced questions)

- **(a) Genuine red?** Yes. With the module reverted the test cannot compile: `matrix` does not
  exist on the base. That is the brief's pre-declared posture (a), net-new, and C4-verify will
  report UNVERIFIABLE (exit 77), which is expected. Since a compile failure is a weak red, the
  ten behaviour mutations above each turned a named leg red, and every one went green again
  after the restore.
- **(b) Production path?** Yes. Every test calls `wyrd_validate::matrix::{classify, verdict,
  parse_version, select, select_reported, is_strong_etag}` and the real `PRE_0_1` /
  `PROFILES` constants through the crate's public API. No copy or stand-in. The test-local
  profiles in (C)/(D) are built with the production `Profile::new` / `Row::*` constructors and
  judged by the production `select`/`classify`/`verdict`.
- **(c) Fixture includes the fault?** Yes. Each negative fixture is the faulty answer itself,
  fed into an otherwise passing full `pre-0.1` run (`verdict_with` replaces one exercise of
  `passing_run()`, which covers all 22 rows). So the FAIL / INCONCLUSIVE comes from that fault
  and not from a missing row. The test asserts exactly one fatal or unproved row, naming that
  row. The INCONCLUSIVE fixtures include the HEAD that timed out, the 503 HEAD, and the bulk
  delete with one unanswered HEAD among 404s.

## Rubric self-review

- Grammar strictness: ETag, Content-Range and version parsers validate every token (above).
- Absent entries: a missing follow-up, an empty key list, an undeclared or unexercised row each
  give an explicit unproved result. None is a silent pass. Listings are compared for exact
  equality, not by count.
- Serialization identity / transactions / await discipline / probes: no surface touched (pure
  module, no I/O).
- One clock: no clock reads. No shared mutable globals (consts only). Not a new crate, so no
  new `forbid(unsafe_code)` root; the crate's `lib.rs:1` already forbids it.
- Docs currency: no port, API op, RPC, CLI flag or persisted field is added. The 0017 amendment
  is in this patch. `05-building-block-view.md` and the `lib.rs` module-doc "#743" sentence are
  out of scope per the brief (743.3).

## Commit-readiness

`cargo fmt -p wyrd-validate` applied. `cargo clippy -p wyrd-validate --all-targets -- -D
warnings` is clean. Full gate: see below.

## Full gate

`./engine/xtask.sh ci` (the project's `cargo xtask ci`) on the worktree: **all checks passed**
(exit 0), on the second run.

The first run failed three checks, all caused by this patch and all fixed before the second
run:
- `typos` flagged the spelling `Unparseable`. The variant is now `NoProfile::Unparsable`.
- The UnsafeForbid guard (#616) treats every `tests/*.rs` as a crate root. The test file now
  carries `#![forbid(unsafe_code)]` after its crate docs (`capability_matrix.rs:5`).
- `cargo test --workspace` failed only on `xtask/tests/repo_hygiene_guards.rs:398`, the same
  guard, for the same file.

After the fix, the test target gave 41 passed and `typos` was clean on every touched file. The
docs lint and the site render (`lint_docs: OK`, `render_site: link audit OK`) passed on both
runs, so the declared external dependencies `typos` and `docs-renderer` were present. No
NEEDS-HUMAN external dependency.
