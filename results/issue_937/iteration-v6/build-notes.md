# Build notes — #937 (743.1) validate: the capability matrix — iteration 6

Withheld from the reviewer. For the human at sign-off. Line numbers are on the worktree base
below with the patch applied, unless a line says otherwise.

## What this round is

Iteration 5's sign-off asked for three implementation-level things, all test coverage (the
review wrote the cases as a probe file and found production already passes every one: "pure
additions with no code change"):

1. a fixture that stops a generic `Exercise::Other` from ever passing a `required` row;
2. fixtures for the two untested bulk-delete pass conditions (no keys requested; `<Deleted>`
   naming every requested key);
3. one fixture per guard that could be removed with the suite still green (an `unsupported`
   row's torn error body, the `Content-Range` unit, a 206 without `Content-Range`, a listing or
   bulk-delete 200 without its document, an exercise the profile declares no row for).

Iteration 3's sign-off (still standing) asked that the matrix be carried forward unchanged in
substance, with the three open design points flagged again.

- **`matrix.rs`, the `lib.rs` line and both 0017 hunks are byte-identical to iteration 5.**
  sha256 `matrix.rs` `f1431971…eca12` (the value iterations 3, 4 and 5 recorded; `cargo fmt
  --all` left it untouched). `diff iteration-v5/patch.diff patch.diff` touches only the
  `crates/validate/tests/capability_matrix.rs` hunk.
- **The test grew from 48 to 53 cases** and pins every case the review named; details under
  *What changed*.
- **The advisory wall is still there, for the same reason as iterations 4 and 5.** The worktree
  the driver gave this Do is again `93a6f37`. Details and the NEEDS-HUMAN marker under *Base*.

## Base: read this first (it decides whether C4-ci can go green)

- **The worktree is `93a6f37`**, the tip of `pdca-integration/main-r42c5348f203d` (`main`
  `4bac538` + #744 `61e5341`, #853 `582da99`, #932 `93a6f37`); the bundle's `stack-base-tip`
  says the same. `b552ca2` (the base iteration 3's sign-off named) is **not** an ancestor:
  `git merge-base --is-ancestor b552ca2 HEAD` exits 1. `origin/main` is now `9b1a3e2`; the
  merge-base with it is `6a85eaa`, before #950/#954.
- **Why I built here.** Check rebuilds base + `patch.diff` from the stack base, and the harness
  owns the worktree base; the brief's ordering note (`brief.md:112-122`) says either base works
  and that Do records which one it used.
- **What it costs, confirmed on this round's base before anything was edited.** `Cargo.lock`
  pins `ctutils 0.4.2` (`Cargo.lock:1000-1001`) and `tokio 1.52.3` (`:3953-3954`). `cargo deny
  --all-features --config deny-all-features.toml check advisories` in the worktree: **exit 1**,
  `advisories FAILED`, RUSTSEC-2026-0385 and RUSTSEC-2026-0386 (`$PDCA_SCRATCH/pdca-builder-937-it6/deny.log`).
  The patch touches no `Cargo.toml`, `Cargo.lock` or `deny*.toml` (the brief puts manifests and
  deny config out of scope, `brief.md:134`; bumping the lockfile here would duplicate #950/#954
  and conflict on merge).
- **The standing doctor row will read green.** Its detect command runs in `../wyrd`
  (`pdca.toml:1411`, `WYRD_REPO:-../wyrd`), which is at `b552ca2` where both checks pass. C4-ci
  runs on `93a6f37`, where they fail. Only a refresh of the integration line (or a rebase of
  this bundle onto `main`) clears it.
- #508 is not on this base: the denylist still lists `uploads`, `uploadId`, `partNumber`
  (`crates/gateway-s3/src/lib.rs:410-413`), so the six multipart rows stay
  `unsupported(#508)` (`crates/validate/src/matrix.rs:326-367`), per `brief.md:29-31`.
- Every gateway and derivation line the rows and comments cite was re-read on this base in
  iteration 5 (`iteration-v5/build-notes.md:47-51`); the crate did not change between that
  worktree and this one (same commit).
- **Expect C4-diff-cov to say "does not apply on origin/main" again** (the `lib.rs` hunk
  differs between the integration line and `main`).

NEEDS-HUMAN external dependency: cargo deny all-features advisories — the run's stack base `93a6f37` (`pdca-integration/main-r42c5348f203d`) still pins `ctutils 0.4.2` and `tokio 1.52.3` (`Cargo.lock:1000-1001`, `:3953-3954`), so `cargo deny --all-features --config deny-all-features.toml check advisories` exits 1 on the unpatched base (RUSTSEC-2026-0385, RUSTSEC-2026-0386; confirmed this round before the patch was applied) and C4-ci cannot go green on it; #950/#954 fixed both on `main` (`b552ca2` and later) but not on the integration line, and the standing doctor row runs in `../wyrd` (green), so it will not flag this. This patch touches no manifest, lockfile or deny config; refresh the integration line onto current `main` (or rebase this bundle on `main`) and re-run Check

(The case is already registered — the standing row at `pdca.toml:1408-1413` — so no new
`[[doctor.checks]]` row is proposed. What that row cannot do is run against the stack base
instead of `../wyrd`; that is a harness question, not build work.)

## What changed since iteration 5: the test only

`crates/validate/tests/capability_matrix.rs`, 1,288 lines (was 1,307), 53 tests (was 48). Each
review finding, the test that now pins it, and the red leg (table under *Refuting my own test*)
that shows the test binds production:

| Finding (iteration 5 review) | Test | Pins | Leg |
|---|---|---|---|
| `Exercise::Other` on a `required` row could be made `Ok(())` with the suite green | `a_required_row_is_never_passed_by_an_exercise_that_has_no_check` `:539` | `classify` gives `Unsettled`; the run is INCONCLUSIVE naming `PutObject` | R11 |
| empty-keys guard (`matrix.rs:1218-1220`) untested | `a_bulk_delete_of_no_keys_or_with_a_key_missing_from_deleted_is_not_a_pass` `:722-731` | `keys: []` + empty `<DeleteResult>` is `Unsettled`, never `Pass` | R12 |
| `<Deleted>` must name every key (`matrix.rs:1291-1295`) untested | same test, `:732-740` | `<Deleted>` naming `p/a` of `[p/a, p/b]`, no `<Error>`, both `HEAD`s 404 → `protocol` | R13 |
| `unsupported` row, declared status, torn body (`matrix.rs:942-946`) | `an_unsupported_row_refused_as_declared_but_cut_off_is_not_proved` `:465` | `CopyObject` 501 + `Body::Failed` → `Unsettled`/INCONCLUSIVE; a 400 + `Body::Failed` is still `protocol` | R14 |
| the unit must be `bytes` (`matrix.rs:1316-1318`) | `ranged_get_with_the_wrong_content_range_is_protocol` `:584-613`, two new entries | `items 2-5/16` and `2-5/16` → `protocol` | R15 |
| 206 with no `Content-Range` (`matrix.rs:1106`) | same test, `:604-608` | right bytes, no header → `protocol` | R16 |
| listing 200 without a listing document (`matrix.rs:1159`) | `a_200_without_the_document_the_check_reads_is_protocol` `:678` | v2 and v1 with no body, raw bytes, or a `<DeleteResult>` → `protocol` | R17 |
| bulk-delete 200 without a `<DeleteResult>` (`matrix.rs:1274`) | same test | no body, raw bytes, or a listing → `protocol` | R18 |
| an exercise with no declared row (`matrix.rs:1408`) | `an_exercise_the_profile_declares_no_row_for_is_not_proved` `:1237` | a good `HeadObject` against a profile with no `HeadObject` row → INCONCLUSIVE, `Unproved::Undeclared`, "no row declares it" | R19 |

**Size, and why the existing fixtures were refactored.** The brief caps the patch at the 100 KB
backstop (`brief.md:166-167`). Iteration 5's patch was 101,078 bytes; the new tests alone add
about 4.5 KB, which would have put it near 106 KB, over the 102,400-byte threshold
(`patch_bytes / 1024 >= 100`). I did not touch `matrix.rs` or the 0017 hunks to buy room (the
carry-forward asked for them unchanged). Instead the test's own `Exercise` literals were folded
into helpers (`capability_matrix.rs:81-162`: `other`, `get`, `hit`, `miss`, `del`, `list_v2`,
`list_v1`, `ranged_206`, `whole`, `part`, `both_404`; `head_of` and `ranged` moved up to the
fixtures block). Every existing assertion is kept — same statuses, bodies, headers, classes and
expected detail strings; `passing_run` builds the same 22 exercises. Result: **101,953 bytes =
99.56 KiB, 447 bytes under the threshold.** If you want more headroom, the one hunk nothing else
depends on is the 0017 *Dependencies* paragraph iteration 5 added at iteration 4's review's
request (`patch.diff`, the hunk at `@@ -855,12 +870,14 @@`, 1,637 bytes); dropping it
re-opens that review finding.

## Open design points: still yours to decide

Unchanged from iterations 4 and 5, as asked: current behaviour kept, rustdoc consistent with the
code, flagged again here.

1. **Strict (E).** A row is proved only when it was exercised and *every* exercise of it passed
   (`crates/validate/src/matrix.rs:1393-1396`, enforced at `:1397-1421`; pinned by
   `a_row_passes_only_when_every_exercise_of_it_passed`, `capability_matrix.rs:1282`). Brief
   (E) says "exercised only with no answer". The strict rule never claims more than the answers
   prove; a PUT or DELETE with no answer is an unresolved mutation (0017 §7, `0017:528-539`). But
   one timed-out *read* retry then ends the run INCONCLUSIVE, which fits poorly with §7's
   budgeted `availability` (`0017:508`). 743.3 (#939) is the first caller; decide before it
   builds retries on this. Relaxing it to the brief's wording is about 5 lines in `verdict` plus
   flipping that test.
2. **`0.1.0-rc.1` selects `pre-0.1`.** The brief says `pre-0.1` covers everything below `0.1.0`
   (`matrix.rs:406-413`; pinned at `capability_matrix.rs:334`). The amendment says "Retention
   starts at the first tag" (`0017:287`). If the first tag is `v0.1.0-rc.1`, that tag lands in
   the profile that is edited in place. Either end the range at the first tag or say "the first
   `0.1.0` release" in the amendment. I left it as the brief states.
3. **Listing a prefix with nothing written is unsettled, not PASS** (`matrix.rs:1170-1173`;
   pinned by `a_listing_of_a_prefix_with_nothing_written_proves_nothing`,
   `capability_matrix.rs:907`). A rule added in iteration 3; the brief does not ask for it. It is
   the same vacuous pass the module refuses for a `DeleteObjects` of no keys
   (`matrix.rs:1218-1220`, now pinned too). A key listed where nothing was written is still
   `oracle`. To drop it: 4 lines of code, 9 of test.

For the record (unchanged since iteration 2): a 5xx other than 501 on a required row is
unsettled (`availability`, `matrix.rs:962-970`); a 501 is `protocol`. On an `unsupported` row
any wrong status, a 5xx included, is `protocol` (brief B; `matrix.rs:936-941`).

## Where each part of the brief lives (on `93a6f37` + patch)

- `crates/validate/src/lib.rs:24`: `pub mod matrix;`, the brief's one line.
- `crates/validate/src/matrix.rs` (new, 1,479 lines, unchanged since iteration 3):
  - rows in three states, `Expected` `:139-152`; `Row` `:156-197`;
  - (A) `PRE_0_1_ROWS` `:278-403`, each row citing its gateway line; `PRE_0_1` `:406-413`;
    `PROFILES` `:416`;
  - (C) `parse_version` `:505-564` (unknown `:494`, `:510-512`; `+git.unknown[.dirty]` `:498`,
    `:555-557`; every token checked `:517-554`; `parse_numeric` `:573-578`); `select`
    `:605-619`; `select_reported` `:622-625`; `VersionRange::contains` with SemVer precedence
    `:230-238`, `:442-450`;
  - the observation type `Answer`/`Response`/`Body`/`Exercise` `:633-830`, with
    `request_body_ended` at `:742`, a probe per follow-up at `:771` and `:778`;
  - (B) `classify_unsupported` `:924-958` (any 2xx is `unexpected-support`, `:930-935`);
  - (B′) `check_required` `:1054-1182`; `probe_deleted` `:1187-1207`; `check_delete_objects`
    `:1213-1259`; `check_delete_result` `:1263-1297`; `is_strong_etag` (RFC 9110 §8.8.3)
    `:1302-1310`; `parse_content_range` `:1314-1329`;
  - (D) `RowResult::NotChecked` `:884-887`, `classify` `:907-922`, `not_covered` `:1422-1435`;
  - (E) `verdict` `:1397-1449`; its `Display` `:1451-1479`.
- `crates/validate/tests/capability_matrix.rs` (new, 53 tests): fixtures `:15-265`; (A)
  `:268-362`; (B) `:365-485`; (B′) `:487-942`; (C) `:944-1150`; (D) `:1152-1195`; (E)
  `:1197-1288`.
- 0017: the third state `0017:196-199`; the version on the wire `:263-268`; editing `pre-0.1`
  in place until the first tag `:284-290`; *Dependencies* `:873-880`.
- Docs currency: the patch adds a library module, not a port, API operation, RPC, CLI flag or
  persisted field. The brief leaves `05-building-block-view.md` to 743.3, which adds the
  feature (`brief.md:130-131`).

## How the test was run

The project wrapper `./engine/xtask.sh` runs whole gates only (it execs `cargo xtask "$@"`,
`engine/xtask.sh:36`; `xtask` has no single-target test subcommand, `xtask/src/main.rs:119-150`).
For red→green and the red legs I ran the exact target the brief names (`brief.md:98-99`),
`cargo test -p wyrd-validate --test capability_matrix`, under `timeout 600`/`900`/`1500` from the
worktree. The test is pure (no I/O, no network, no clock), so it cannot hang; the timeout is a
backstop. The whole gate ran once on the final tree (below).

Baseline before any edit, iteration 5's test on this base: 48 passed
(`$PDCA_SCRATCH/pdca-builder-937-it6/baseline-test.log`). After the edit: **53 passed**
(`new-test.log`).

## Refuting my own test

**(a) Genuine red? Yes.** Two forms, both on this round's tree:

- *The whole module reverted* (`pub mod matrix;` removed from `lib.rs`, leg R0): the test
  target does not compile — `error[E0432]: unresolved import wyrd_validate::matrix`, exit 101.
  The declared posture (`brief.md:138-141`); C4-verify will say UNVERIFIABLE (exit 77).
- *Each behaviour reverted, one at a time.* `$PDCA_SCRATCH/pdca-builder-937-it6/redlegs.py`
  (run with `python3 -I`) made 25 temporary edits to the production `matrix.rs`. For each it
  checked the anchor occurs exactly once, ran the target, and restored the file byte for byte
  (sha256 checked after every leg; `redlegs.log:117` `restored: OK`). The final run on the
  restored tree was **53 passed** (`redlegs.log:118`). Two legs (R5, R7) first hit a compile
  error from the unused-code lint rather than a behavioural red; both were rewritten to compile
  and re-run (`redlegs-R5-R7.log`). Every leg went red:

| # | Temporary edit to `matrix.rs` | Result | Red test(s) |
|---|---|---|---|
| R1 | (B) a 2xx to an unsupported row → `Pass` | 49/4 | `any_2xx_to_an_unsupported_row_is_unexpected_support`, `an_unexpected_success_fails_the_run`, `any_fatal_fails_the_run_naming_every_fatal_row`, `each_version_selects_its_own_profile_and_expectations` |
| R2 | (B) a 2xx head whose body failed → `NoAnswer` | 51/2 | `any_2xx_to_an_unsupported_row_is_unexpected_support`, `an_unsupported_row_refused_as_declared_but_cut_off_is_not_proved` |
| R3 | (B′) the PUT ETag check accepts any non-empty value | 50/3 | `put_etag_that_is_not_a_strong_tag_is_protocol`, `a_bodiless_answer_cut_off_after_its_head_is_not_proved`, `any_fatal_fails_the_run_naming_every_fatal_row` |
| R4 | (B′) the non-matching conditional GET accepts a 304 | 52/1 | `non_matching_conditional_get_answered_304_is_protocol` |
| R5 | (B′) `DeleteObjects` folds the document check only (`worst(checks.into_iter().take(1))`) | 48/5 | `delete_objects_document_alone_never_proves_the_delete`, `delete_follow_up_with_no_answer_or_a_5xx_is_inconclusive_not_oracle`, `an_unresolved_delete_names_the_delete_row`, `every_follow_up_is_judged_against_the_key_it_asked_about`, `a_cut_off_delete_result_never_hides_a_probe_that_reads_a_key` |
| R6 | (C) the selector ignores the version (`take(1)`) | 48/5 | `each_version_selects_its_own_profile_and_expectations`, `pre_0_1_covers_every_version_below_0_1_0`, `a_version_no_profile_covers_selects_none`, `overlapping_profiles_select_none`, `a_selection_that_finds_no_profile_says_why` |
| R7 | (C) the `+git.unknown` identity check never fires (`&& false`) | 52/1 | `unknown_is_never_read_as_release_0_0_0` |
| R8 | (D) a `supported, not checked` row also counted as unexercised | 51/2 | `a_not_checked_row_need_not_be_exercised_and_is_listed_as_not_covered`, `an_exercise_the_profile_declares_no_row_for_is_not_proved` |
| R9 | (B′) the PUT check ignores when its head arrived | 51/2 | `put_answered_before_its_body_ended_is_protocol_whatever_its_etag`, `a_bodiless_answer_cut_off_after_its_head_is_not_proved` |
| R10 | (B′)/(E) a delete whose follow-up `HEAD` had no answer → `oracle` | 49/4 | `delete_follow_up_with_no_answer_or_a_5xx_is_inconclusive_not_oracle`, `an_unresolved_delete_names_the_delete_row`, `any_fatal_fails_the_run_naming_every_fatal_row`, `a_cut_off_delete_result_never_hides_a_probe_that_reads_a_key` |
| R11 | it5: `Exercise::Other { .. } => Ok(())` on a required row | 52/1 | `a_required_row_is_never_passed_by_an_exercise_that_has_no_check` |
| R12 | it5: the empty-keys guard removed | 52/1 | `a_bulk_delete_of_no_keys_or_with_a_key_missing_from_deleted_is_not_a_pass` |
| R13 | it5: `<Deleted>` no longer has to name every requested key | 52/1 | `a_bulk_delete_of_no_keys_or_with_a_key_missing_from_deleted_is_not_a_pass` |
| R14 | it5: an unsupported row's torn error body made fatal `protocol` | 52/1 | `an_unsupported_row_refused_as_declared_but_cut_off_is_not_proved` |
| R15 | it5: the `Content-Range` unit no longer has to be `bytes` | 52/1 | `ranged_get_with_the_wrong_content_range_is_protocol` |
| R16 | it5: a 206 with no `Content-Range` passes (`None => {}`) | 52/1 | `ranged_get_with_the_wrong_content_range_is_protocol` |
| R17 | it5: a listing 200 without a listing document → `Ok` | 52/1 | `a_200_without_the_document_the_check_reads_is_protocol` |
| R18 | it5: a bulk-delete 200 without a `<DeleteResult>` → `Ok` | 52/1 | `a_200_without_the_document_the_check_reads_is_protocol` |
| R19 | it5: an exercise with no declared row silently skipped | 52/1 | `an_exercise_the_profile_declares_no_row_for_is_not_proved` |
| R20 | it1: a probe paired with the wrong key (`==` → `!=`) | 52/1 | `every_follow_up_is_judged_against_the_key_it_asked_about` |
| R21 | it1: `\|\|` → `&&` in `parse_numeric` | 52/1 | `parse_refuses_malformed_versions` |
| R22 | it1: a bodiless answer cut off after its head read as `protocol` | 52/1 | `a_bodiless_answer_cut_off_after_its_head_is_not_proved` |
| R23 | it2: a torn bulk-delete document returns before the probes | 51/2 | `a_cut_off_delete_result_never_hides_a_probe_that_reads_a_key`, `every_follow_up_is_judged_against_the_key_it_asked_about` |
| R24 | it3: an empty listing of an empty prefix passes | 52/1 | `a_listing_of_a_prefix_with_nothing_written_proves_nothing` |
| R25 | it1: a signed `Content-Range` number accepted (`&&` → `\|\|`) | 52/1 | `ranged_get_with_the_wrong_content_range_is_protocol` |

R1–R10 are the brief's ten required demonstrated reds (`brief.md:142-150`). R11–R19 are
iteration 5's review findings (the fixtures added this round). R20–R25 re-pin the findings of
iterations 1–3.

**(b) Production path? Yes.** Every test calls `wyrd_validate::matrix::{classify, verdict,
select, select_reported, parse_version, is_strong_etag}` and the `PRE_0_1`/`PROFILES` constants
directly (`capability_matrix.rs:7-13`). No copy, no mock. The legs edit those functions and the
tests notice.

**(c) Fixture includes the fault? Yes.** Each failing element is in the fixture itself: a 2xx
to an unsupported row, a cut-off body (on required and unsupported rows), a weak, unquoted or
`*` ETag, an early PUT acknowledgement, a 304 to a non-matching tag, a `<DeleteResult>` with an
`<Error>` row or a key missing from `<Deleted>`, a bulk delete of no keys, a follow-up `HEAD`
that reads a deleted key, gets no answer or a 503, a torn `<DeleteResult>` next to a readable
probe, a 200 with no document or the wrong one, a `Content-Range` with the wrong unit or none,
a generic exercise against a required row, an exercise with no declared row, `+git.unknown`,
and an overlapping table. Full runs are built from `PRE_0_1.rows()` (`passing_run`,
`capability_matrix.rs:206-230`), so no declared row is left out.

## Gates and commit-readiness

- `cargo fmt --all`: ran over the tree; `matrix.rs` unchanged by it (same sha), the test
  formatted.
- `cargo clippy -p wyrd-validate --all-targets --all-features -- -D warnings`: clean
  (`$PDCA_SCRATCH/pdca-builder-937-it6/clippy.log`).
- `typos` on the four changed files: clean (`typos-cli 1.48.0`). Both of the brief's external
  dependencies (`typos`, `docs-renderer`) are present on this host (`markdown_it`/`yaml`
  import). `lint_docs.py` and `render_site.py --check` run inside the whole gate below.
- The target repo configures no commit hooks (no `core.hooksPath`, no `.pre-commit-config.yaml`,
  no `.githooks`), so the formatter and the prose gates are what a commit has to pass.
- `patch.diff` is `git diff` of the worktree (the two new files intent-to-add); it reverses
  cleanly on the worktree (`git apply --check -R`), so it applies on `93a6f37`.
- Size: **101,953 bytes = 99.56 KiB**, under the 102,400-byte backstop (see *What changed*).

## Self-review against the rubric (`AGENTS.md` § Review rubric & protocol)

- One clock: the module reads no clock. Narrow seams: the module depends on `crate::s3::ErrorCode`
  only (`matrix.rs:33`); nothing from a `wyrd-*` crate (the blackbox guard checks this in `ci`).
- No shared mutable global state: `PRE_0_1`, `PROFILES` and the rows are `const`.
- Grammar strictness: the entity-tag (`:1302-1310`), `Content-Range` (`:1314-1329`) and version
  (`:505-578`) parsers validate every token; signs, wrong units, empty identifiers, leading
  zeros are refused and pinned.
- Absent entries: a missing document, a key missing from `<Deleted>`, an unprobed key, a
  listing with nothing written, a row with no exercise and an exercise with no row each give an
  explicit result (`protocol` or unsettled), never silent success; now pinned.
- Protocol input: a torn body is never silently accepted (unsettled on required and unsupported
  rows, pinned both ways); a 2xx that then tears is still `unexpected-support`.
- Docs currency: no port, API operation, RPC, CLI flag or persisted field; 0017 amended as the
  brief asks.

## The whole gate on the final tree

`/home/eddie/wyrd/wyrd-pdca/engine/xtask.sh ci` with `PDCA_WORKTREE` set to this worktree,
on the final tree (`$PDCA_SCRATCH/pdca-builder-937-it6/ci.log`, 4,211 lines):

**Exit 1. Exactly two checks failed, both the advisory wall described under *Base*:**

```
xtask: 2 checks failed:
  - cargo deny check: exited with exit status: 1
  - cargo deny --all-features --config deny-all-features.toml check advisories: exited with exit status: 1
```

(`ci.log:3647` `advisories FAILED, bans ok, licenses ok, sources ok`; `:4007` `advisories
FAILED`; the IDs at `:3336`, `:3418`, `:3652`, `:3734` are RUSTSEC-2026-0385 and
RUSTSEC-2026-0386, the same two as on the unpatched base, `deny.log`.)

Every other step passed: `typos` (`ci.log:2`), `lint_docs` OK (`:5`), `render_site` 99 pages,
link audit OK (`:9`), the unsafe guard (every crate root forbids unsafe code, `:15`), the
blackbox guard (`wyrd-validate` links no `wyrd-*` crate, `:18`), `cargo fmt --all -- --check`,
workspace clippy, build and test — 199 test binaries, 222 `test result: ok` lines, 0 `FAILED`;
`capability_matrix` **53 passed** at `ci.log:2702` — `cargo-machete`, `cargo deny
--all-features check licenses bans sources`, the statics and deploy guards, and the DST tier
(`--cfg madsim` clippy and test, all ok).

## Scratch

Everything this round made outside the worktree is under `$PDCA_SCRATCH`
(`/var/tmp/pdca/wyrd-pdca-9c587031/issue_937/pdca-builder-937-it6/`): `deny.log`,
`baseline-test.log`, `new-test.log`, `clippy.log`, `redlegs.py`, `redlegs.log`,
`redlegs-R5-R7.log`, `ci.log`. The `-it5-*` and older dirs there are previous rounds'. I did
not delete any of them: the harness owns that root (bundle-scoped, reclaimed at the
publish/freeze boundary, `pdca.toml:1435-1440`).
