# Build notes — #937 (743.1) validate: the capability matrix — iteration 3

Withheld from the reviewer. For the human at sign-off. Line numbers are on the worktree base
below, with the patch applied.

## Base

Same as iteration 2: the worktree the driver gave this Do, `pdca-integration/main-r42c5348f203d`
at `93a6f37` (on `main` `4bac538`: #744 `61e5341`, #853 `582da99`, #932 `93a6f37`). The brief's
ordering note allows either base. The matrix uses nothing from #744/#853/#932 except
`s3::ErrorCode` (`crates/validate/src/s3/error.rs:69-76`), the same type on both bases.
`crates/gateway-s3/src/lib.rs` and `crates/server/src/version/derivation.rs` are byte-identical
to `main` `4bac538` here (`git diff --quiet 4bac538 93a6f37 -- …` exit 0), so every row citation
holds on both.

#508 is **not** on this base: the denylist still lists `uploads`, `uploadId`, `partNumber`
(`crates/gateway-s3/src/lib.rs:410-413`). So the six multipart rows stay `unsupported(#508)`.

**The `lib.rs` line applies on this line only** (`crates/validate/src/lib.rs:24`, between
`pub mod args;` and `pub mod payload;`; on `main` the next line is `pub mod s3;`, and rustfmt's
`reorder_modules` keeps `mod` lines sorted, so no placement has context that matches both). The
other three files apply on both. **Expect C4-diff-cov to say "does not apply on origin/main"
again**: that gate takes `origin/main` as its base while Do, C4-ci and C4-verify use the
integration line. A harness base mismatch, not a patch defect; worth raising upstream.

## What changed since iteration 2 (the carry-forward)

Started from iteration 2's patch (it applied cleanly to `93a6f37`) and changed what the
carry-forward names, plus two small gaps of the same kind found in self-review.

### Production — `crates/validate/src/matrix.rs`

1. **A cut-off bulk-delete document no longer hides the probes** (the gating T4 finding; C5
   causal adequacy; T5). `check_delete_objects` (`matrix.rs:1209-1259`) now judges the
   `<DeleteResult>` as one check, in the new `check_delete_result` (`:1261-1297`), and every
   follow-up `HEAD` as its own check, then folds them with `worst` (`:1013`), as `DeleteObject`
   already did. Before, `Body::Failed` returned `Unsettled` before any probe was looked at.
   Now: torn document + `HEAD 200` → `oracle`; + `HEAD 403` → `protocol`; + all `404` →
   INCONCLUSIVE (never PASS).
   - Why `oracle` is sound when the document was lost: after a `200` no correct server leaves a
     requested key readable. The lost document either had an `<Error>` row for it (fatal
     `protocol`, brief B′), named it under `<Deleted>` falsely (`oracle`), or left it out
     (`protocol`). Every reading is FAIL. The class follows the brief's per-probe rule ("a 2xx →
     `oracle`"). The reviewer's counter-argument (0017 §7 outcome resolution) applies to a
     mutation with no acknowledgement, which is the next point.
   - Without a `200` (no answer, a 5xx, any other status) the early return stays: no delete was
     acknowledged, so a readable key disagrees with nothing. Pinned by a new test.
   - The "key requested twice" check moved into `check_delete_result` (`:1264-1270`), so it now
     spoils only the document check. A duplicated key's probes are still judged:
     `[a, a]` + `HEAD a → 200` was `Unsettled`, now `oracle` (same defect class as the torn
     document).
2. **The one C5 survivor** (`||` → `&&` in `parse_numeric`, iteration 2's `matrix.rs:572`). It
   was an equivalent mutant: the `s.is_empty() ||` test duplicated what `u64::from_str` already
   refuses (`""` is `IntErrorKind::Empty`), so merging it into the digit check changed nothing. I
   removed that redundant test (`matrix.rs:570-578`). The digit check stays (it refuses the `+`
   `from_str` takes, the AGENTS.md grammar rule). With the new shape, every mutant there is
   caught: `||` → `&&` now lets `01` through (leg R12 below). Pinned by new inputs: `0..0`,
   `.0.0`, `0.0.`, a core number and a pre-release number one past `u64::MAX`; and value checks
   `10.20.30` and `18446744073709551615.0.0`.
3. **A listing of a prefix where nothing was written is unsettled, not PASS**
   (`matrix.rs:1165-1176`). Self-review found it: an empty `expected` and an empty listing
   passed, the same vacuous pass the module already refuses for a `DeleteObjects` of no keys
   (rubric: *absent entries*, no silent success). A key listed where nothing was written is
   still `oracle`. Not in the carry-forward; it costs 4 lines of code and 9 of test. Drop it at
   sign-off if you think it is beyond the slice.
4. **`verdict`'s rustdoc said two things** (`matrix.rs:1393-1396`; a deferred finding named the
   contradiction). It now describes what the code does: a row is proved only when every
   exercise of it passed. **The rule itself is unchanged and still yours to pick** (see below).

### Test — `crates/validate/tests/capability_matrix.rs` (46 → 48 tests)

- New `a_cut_off_delete_result_never_hides_a_probe_that_reads_a_key` (`:878`): the reviewer's
  case (`a → 200, b → 404`) and its mirror → `oracle`; `b → 403` → `protocol`; all `404`, and
  `404` + no answer → INCONCLUSIVE.
- New `probes_after_an_unacknowledged_bulk_delete_are_not_judged` (`:899`): no answer, and 503,
  each followed by readable keys → INCONCLUSIVE, with `NoAnswer` named for the first.
- New `a_listing_of_a_prefix_with_nothing_written_proves_nothing` (`:912`).
- `every_follow_up_is_judged_against_the_key_it_asked_about` (`:844`): adds `[a, a]` +
  `HEAD a → 200` → `oracle` (`:874`).
- `parse_reads_releases_and_ignores_build_metadata` (`:962`) and
  `parse_refuses_malformed_versions` (`:1002`): the inputs in point 2.
- Removed `untagged_build_selects_pre_0_1`: its one assertion (`0.0.0+git.abc1234` selects
  `pre-0.1`) is the first input of `pre_0_1_covers_every_version_below_0_1_0` (`:285-299`).
  Removed for size (below).

### Not changed, on purpose (the deferred findings, held for you)

- **Strict (E).** A row exercised twice, once passing and once with no answer, is INCONCLUSIVE
  (`a_row_passes_only_when_every_exercise_of_it_passed`). The brief's (E) says "exercised only
  with no answer". The strict rule never over-claims, and a PUT/DELETE with no answer is an
  unresolved mutation (0017 §7). But it means one timed-out *read* retry ends a run INCONCLUSIVE,
  which sits badly with §7's budgeted `availability`. 743.3 is the first caller; decide before
  it builds retries on this. Changing it is ~5 lines in `verdict` plus flipping that test.
- **`0.1.0-rc.1` selects `pre-0.1`.** The brief says `pre-0.1` covers everything below `0.1.0`;
  the amendment says "Retention starts at the first tag". If the first tag is `v0.1.0-rc.1`, it
  lands in the edited-in-place profile. Either end the range at the first tag or say "the first
  `0.1.0` release" in the amendment. Left as the brief states.
- A 5xx on a required row other than 501 is unsettled (`availability`); a 501 is `protocol`. On
  an `unsupported` row any wrong status, 5xx included, is `protocol` (brief B).

## Size

`patch.diff` is **99,419 bytes**, under the 100 KB backstop (decimal or binary). By file:
`matrix.rs` 53,274 (iteration 2: 52,232), the test 42,610 (40,474), the 0017 amendment 3,249
(unchanged), `lib.rs` 286 (unchanged). The first measure was 100,422 bytes; I removed the one
duplicate test, shortened two new tests and one doc comment, with no assertion lost.

## How the test was run

The project wrapper `./engine/xtask.sh` exposes whole gates only (`cargo xtask ci`, …), not one
test target. For red→green and the red legs I ran the exact target the brief names
(`cargo test -p wyrd-validate --test capability_matrix`) under `timeout 900`. The test is pure
(no I/O, no network, no clock), so it cannot hang; the timeout is a backstop. The full gate ran
once on the final tree (below).

## Refuting my own test

**(a) Genuine red? Yes.** Two forms:

- *This iteration's fix reverted.* With iteration 2's `matrix.rs` swapped back in (leg R0), the
  shipped test goes **red, 3 failed**: `a_cut_off_delete_result_never_hides_a_probe_that_reads_a_key`,
  `a_listing_of_a_prefix_with_nothing_written_proves_nothing`,
  `every_follow_up_is_judged_against_the_key_it_asked_about`. That is a behavioural pre-fix red
  for the change this iteration makes, which the C4 finding asked for.
- *The whole module reverted* (`pub mod matrix;` removed) does not compile:
  `unresolved import wyrd_validate::matrix`. That is the declared posture (a); C4-verify will
  say UNVERIFIABLE (exit 77).

Then 14 temporary edits to production `matrix.rs`, one at a time, each run and restored (a
script in scratch; it asserts each edit site is unique and restores byte for byte). Final run on
the shipped files: `sha256sum -c` OK for both before and after. Every leg went red:

| # | Temporary edit to `matrix.rs` | Red test(s) |
|---|---|---|
| R0 | iteration 2's `matrix.rs` (this iteration's fix reverted) | the three above |
| R1 | (B) a 2xx to an unsupported row → pass | `any_2xx_to_an_unsupported_row_is_unexpected_support`, `an_unexpected_success_fails_the_run`, `each_version_selects_its_own_profile_and_expectations`, `any_fatal_fails_the_run_naming_every_fatal_row` |
| R2 | (B) a 2xx head whose body failed → no answer | `any_2xx_to_an_unsupported_row_is_unexpected_support` |
| R3 | (B′) PUT ETag check accepts any non-empty value | `put_etag_that_is_not_a_strong_tag_is_protocol`, `a_bodiless_answer_cut_off_after_its_head_is_not_proved`, `any_fatal_fails_the_run_naming_every_fatal_row` |
| R4 | (B′) non-matching conditional GET accepts a 304 | `non_matching_conditional_get_answered_304_is_protocol` |
| R5 | (B′) `DeleteObjects` passes on its document alone | `delete_objects_document_alone_never_proves_the_delete`, `delete_follow_up_with_no_answer_or_a_5xx_is_inconclusive_not_oracle`, `an_unresolved_delete_names_the_delete_row`, `every_follow_up_is_judged_against_the_key_it_asked_about`, `a_cut_off_delete_result_never_hides_a_probe_that_reads_a_key` |
| R6 | (C) selector ignores the version (`take(1)`) | `each_version_selects_its_own_profile_and_expectations`, `pre_0_1_covers_every_version_below_0_1_0`, `a_version_no_profile_covers_selects_none`, `overlapping_profiles_select_none`, `a_selection_that_finds_no_profile_says_why` |
| R7 | (C) `+git.unknown` read as release `0.0.0` | `unknown_is_never_read_as_release_0_0_0` |
| R8 | (D) a `supported, not checked` row counted as unexercised | `a_not_checked_row_need_not_be_exercised_and_is_listed_as_not_covered` |
| R9 | (B′) PUT check ignores when its head arrived | `put_answered_before_its_body_ended_is_protocol_whatever_its_etag`, `a_bodiless_answer_cut_off_after_its_head_is_not_proved` |
| R10 | (B′)/(E) a delete whose follow-up HEAD had no answer → `oracle` | `delete_follow_up_with_no_answer_or_a_5xx_is_inconclusive_not_oracle`, `an_unresolved_delete_names_the_delete_row`, `any_fatal_fails_the_run_naming_every_fatal_row`, `a_cut_off_delete_result_never_hides_a_probe_that_reads_a_key` |
| R11 | only the bulk fix undone (return on a torn document before the probes) | `a_cut_off_delete_result_never_hides_a_probe_that_reads_a_key` |
| R12 | `\|\|` → `&&` in `parse_numeric` | `parse_refuses_malformed_versions` |
| R13 | empty listing of an empty prefix passes | `a_listing_of_a_prefix_with_nothing_written_proves_nothing` |
| R14 | a duplicated key's probes skipped | `every_follow_up_is_judged_against_the_key_it_asked_about` |

R1–R10 are the brief's ten required demonstrated reds; R0, R11–R14 cover this iteration's
changes.

**(b) Production path? Yes.** Every test calls `wyrd_validate::matrix::{classify, verdict,
select, select_reported, parse_version, is_strong_etag}` and the `PRE_0_1` constant directly.
No copy, no mock. The legs edit those functions and the tests notice.

**(c) Fixture includes the fault? Yes.** The new fixtures carry the failing element itself: a
torn `<DeleteResult>` (`Body::Failed`) together with a probe that reads a requested key, a
probe that answers 403, a duplicated key with a readable probe, readable keys after a bulk
delete with no answer or a 503, and a listing with nothing written. Full runs are still built
from `PRE_0_1.rows()` (`passing_run`, `capability_matrix.rs:127-181`), so no declared row is
left out.

## Mutation gate (C5) run locally

The gate's own command (`cargo mutants --in-diff <bundle>/patch.diff --no-shuffle
--cap-lints=true`, from `scripts/mutants-in-diff`), on the final patch, with `--output` in
scratch so nothing lands in the worktree: **158 mutants tested in 8m: 128 caught, 30 unviable,
0 missed** (iteration 2: 1 missed).

## Full gate and commit-readiness

- `./engine/xtask.sh ci` (C4-ci, `cargo xtask ci`) on the final tree, `PDCA_WORKTREE` set to
  this worktree: **exit 1, with three checks failed, none caused by this patch.** What passed:
  `typos`, `lint_docs.py`, `render_site.py --check` ("link audit OK"), the gitlink, unsafe and
  blackbox guards, `cargo fmt --check`, workspace clippy and build, `cargo deny … licenses bans
  sources`, and the DST tier. What failed:
  1. **`cargo deny check` and `cargo deny --all-features --config deny-all-features.toml check
     advisories`**: RUSTSEC-2026-0385 against `ctutils 0.4.2` (`Cargo.lock:74`), published after
     iteration 2's green C4-ci. This patch touches no `Cargo.toml`, `Cargo.lock` or `deny*.toml`
     (`git diff --name-only` lists only the four files above), so the unpatched base fails the
     same way. Declared below.
  2. **`cargo test --workspace`**: four `deployed_role_*` tests in
     `crates/server/tests/custodian_gc.rs` hung (0% CPU, every thread in a futex wait, 29 minutes)
     while the C5 mutation run loaded the host. I stopped that test binary (my own gate run's)
     with SIGTERM. Re-run alone, `cargo test -p wyrd-server --test custodian_gc` passed: **10
     passed in 0.17 s**. No crate outside `crates/validate` depends on `wyrd-validate`, so the
     patch cannot reach it: a load-dependent hang in the custodian run loop (its 60 ms shutdown
     timer never fires), likely a pre-existing flake. Worth an issue if it recurs in Check.
     Because cargo stopped there, the test binaries after it did not run inside the gate. I ran
     the whole of `cargo test -p wyrd-validate` separately: all 8 binaries pass (this file: 48
     passed).
- Also seen on the host, not mine: a `target/debug/wyrd-validate --scenario smoke …` process
  from this lane's worktree (PID 2996178), running for 2 days 14 hours at 99% CPU. It predates
  this session; I did not stop it. You may want to.
- `cargo fmt --all -- --check` clean (rustfmt wrapped one new line; applied).
  `cargo clippy -p wyrd-validate --all-targets --all-features -- -D warnings` clean. `typos` on
  the three changed text files clean.
- `patch.diff` is byte-identical to `git diff` in the worktree and applies with
  `git apply --check` on `93a6f37`.
- `typos` and the docs renderer (the brief's two external dependencies) are present and ran
  inside the gate.

NEEDS-HUMAN external dependency: cargo deny all-features advisories — RUSTSEC-2026-0385 (`ctutils 0.4.2`, `Cargo.lock:74`) fails `cargo deny check` and the all-features advisories check on the unpatched base; this patch changes no manifest, lockfile or deny config, so C4-ci cannot go green on this base until the advisory is resolved (bump `ctutils` to ≥ 0.4.4 in a separate change, or a recorded ignore)
