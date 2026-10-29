# Build notes — #937 (743.1) validate: the capability matrix — iteration 4

Withheld from the reviewer. For the human at sign-off. Line numbers are on the worktree base
below, with the patch applied, unless a line says otherwise.

## What this round is

The carry-forward asked for three things: rebuild on `main` `b552ca2`, carry iteration 3's patch
forward with no change in substance, and get a full Do+Check with gates and review. I carried
the patch forward. The matrix, the test and the `lib.rs` line are byte-identical to iteration 3.
The one change is a single sentence in the 0017 amendment (below). I re-ran every piece of
evidence on this round's tree.

**I could not build on `b552ca2` in the harness worktree.** The next section explains why and
what it costs. Read it before you weigh C4-ci.

## Base: read this first (it decides whether C4-ci can go green)

- **The worktree the driver gave this Do is `93a6f37`**, the tip of
  `pdca-integration/main-r42c5348f203d`. That is `main` `4bac538` plus #744 (`61e5341`), #853
  (`582da99`) and #932 (`93a6f37`). The branch has not moved since 2026-10-23 02:10, and the
  bundle's `stack-base-tip` still says `93a6f37`. `b552ca2` is **not** an ancestor
  (`git merge-base --is-ancestor b552ca2 HEAD` exits 1).
- **Why I built there anyway.** Check rebuilds base + `patch.diff` from that stack base. A
  patch built for `b552ca2` does not apply on `93a6f37`: the `lib.rs` hunk fails, as shown
  below. Check would then have nothing to test, which defeats the goal of this round.
- **What it costs: C4-ci's cargo-deny steps fail again, for a reason that predates this
  patch.** `93a6f37` still pins `ctutils 0.4.2` and `tokio 1.52.3` (`Cargo.lock`). In this
  worktree, `cargo deny --all-features --config deny-all-features.toml check advisories` and
  `cargo deny check advisories` both fail on RUSTSEC-2026-0385 and RUSTSEC-2026-0386. #950 and
  #954 fixed these on `main`. They are not on the integration line. The patch touches no
  `Cargo.toml`, `Cargo.lock` or `deny*.toml`.
- **The standing doctor row will not catch this.** Its command runs in `../wyrd`, which is now
  at `b552ca2` and green. C4-ci runs on `93a6f37`. So the row reads green while the gate fails
  on the same advisories. That is a mismatch in the harness, not in this patch.
- **What would fix it** (your call or the driver's, not build work): refresh the integration
  line onto `b552ca2`, or fold #950 and #954 into it, then re-run Check. Or rebase this bundle
  on `main` with the one-line `lib.rs` change below.
- **I checked `main` too.** I exported `b552ca2` into scratch (`git archive`, so no worktree
  was registered in the repo) and applied `patch.diff`. Three of the four files apply as they
  are. `lib.rs` does not, because the module list differs: here the line after `pub mod args;`
  is `pub mod payload;`, on `main` it is `pub mod s3;`. rustfmt keeps `mod` lines sorted, so
  no placement has context that matches both bases. With the main-side line added by hand:
  `cargo fmt --check` clean, `cargo clippy -p wyrd-validate --all-targets --all-features -- -D
  warnings` clean, `capability_matrix` **48 passed**, and every `wyrd-validate` test binary
  passes.
  The `lib.rs` hunk for `main`, if you rebase:

  ```diff
  @@ -20,6 +20,7 @@
   pub mod access_keys;
   pub mod args;
  +pub mod matrix;
   pub mod s3;
  ```

- **`ErrorCode` is the same on both bases**, `MissingInXml` included (the test uses it at
  `crates/validate/tests/capability_matrix.rs:337`): `crates/validate/src/s3/error.rs:69-76`
  here, `:65-72` on `b552ca2`, re-exported at the crate root on both. This answers the open
  question in deferred finding 7.
- `crates/gateway-s3/src/lib.rs` and `crates/server/src/version/derivation.rs` are identical
  on both bases (`git diff 93a6f37 b552ca2 --stat` lists neither). I re-read every line the
  rows cite on this base. Each one still gives the answer its row expects.
- #508 is on neither base: the denylist still lists `uploads`, `uploadId`, `partNumber`
  (`crates/gateway-s3/src/lib.rs:410-413`). So the six multipart rows stay `unsupported(#508)`
  (`crates/validate/src/matrix.rs:326-367`).
- **Expect C4-diff-cov to say "does not apply on origin/main" again.** It is the same `lib.rs`
  hunk.

## What changed since iteration 3

- **One sentence in the 0017 amendment** (`docs/design/proposals/draft/0017-blackbox-validation-tool.md:263-264`).
  Iteration 3 said "every gateway response carries `Server: wyrd/<version>`" and cited
  `crates/gateway-s3/src/lib.rs:1697-1699`. The gateway's own comment, three lines up
  (`:1693-1698`), says responses that hyper produces before the service is entered (a malformed
  request line, an oversized header section) carry no `Server` header. The sentence now says
  "every response the gateway's handler returns", citing `:1693-1699`. This changes no
  behaviour and adds 22 bytes. I made it so the reviewer does not flag the overstatement.
- Nothing else. `diff iteration-v3/patch.diff patch.diff` shows only those two lines and the
  doc's blob id. sha256 of the shipped files: `matrix.rs` `f1431971…eca12`, the test
  `05779312…6790`.

## Open design points: still yours to decide

As you asked, I kept current behaviour and checked that the rustdoc says what the code does.

1. **Strict (E).** A row is proved only when it was exercised and *every* exercise of it passed
   (`crates/validate/src/matrix.rs:1393-1396`, enforced at `:1400-1421`; pinned by
   `a_row_passes_only_when_every_exercise_of_it_passed`, `capability_matrix.rs:1301`). The
   brief's (E) says "exercised only with no answer". The strict rule never claims more than the
   answers prove. A PUT or DELETE with no answer is an unresolved mutation (0017 §7). But one
   timed-out *read* retry then ends the run INCONCLUSIVE, which fits poorly with §7's budgeted
   `availability`. 743.3 (#939) is the first caller, so decide before it builds retries on this.
   Relaxing it to the brief's wording is about 5 lines in `verdict` plus flipping that test.
2. **`0.1.0-rc.1` selects `pre-0.1`.** The brief says `pre-0.1` covers everything below
   `0.1.0` (`matrix.rs:405-413`; pinned at `capability_matrix.rs:292`). The amendment says
   "Retention starts at the first tag" (`0017:287`). If the first tag is `v0.1.0-rc.1`, that
   tag lands in the profile that is edited in place. To fix it, either end the range at the
   first tag or say "the first `0.1.0` release" in the amendment. I left it as the brief states.
3. **Listing a prefix with nothing written is unsettled, not PASS** (`matrix.rs:1170-1173`;
   pinned by `a_listing_of_a_prefix_with_nothing_written_proves_nothing`,
   `capability_matrix.rs:912`). I added this rule myself in iteration 3; the brief does not
   ask for it. It is the same "vacuous pass" the module already refuses for a `DeleteObjects`
   of no keys (`matrix.rs:1218-1220`). A key listed where nothing was written is still
   `oracle`. To drop it: 4 lines of code, 9 of test.

Also, for the record (unchanged since iteration 2): a 5xx other than 501 on a required row is
unsettled (`availability`, `matrix.rs:962-970`), and a 501 is `protocol`. On an `unsupported`
row, any wrong status, a 5xx included, is `protocol` (brief B; `matrix.rs:936-941`).

## Where each part of the brief lives (on `93a6f37` + patch)

- `crates/validate/src/lib.rs:24`: `pub mod matrix;`, the brief's one line.
- `crates/validate/src/matrix.rs` (new, 1,479 lines):
  - rows in three states, `Expected` `:137-152`; `Row` `:154-197`;
  - (A) `PRE_0_1_ROWS` `:278-403`, each row citing its gateway line; `PRE_0_1` `:406-413`;
    `PROFILES` `:416`;
  - (C) `parse_version` `:505-564` (unknown `:510-512`, `+git.unknown[.dirty]` `:555-557`,
    tokens checked `:517-554`, `parse_numeric` `:573-578`); `select` `:605-619`;
    `select_reported` `:622-625`; `VersionRange::contains` with SemVer precedence `:230-238`,
    `:442-450`;
  - the observation type `Answer`/`Response`/`Body`/`Exercise` `:631-830`, with
    `request_body_ended` at `:742`, a probe per follow-up at `:771` and `:778`;
  - (B) `classify_unsupported` `:924-958`;
  - (B′) `check_required` `:1054-1182`; `probe_deleted` `:1187-1207`; `check_delete_objects`
    `:1213-1259`; `check_delete_result` `:1263-1297`; `is_strong_etag` (RFC 9110 §8.8.3)
    `:1302-1310`; `parse_content_range` `:1314-1329`;
  - (D) `RowResult::NotChecked` `:884-887`, `classify` `:916`, `not_covered` `:1422-1435`;
  - (E) `verdict` `:1397-1449`; its `Display` `:1451-1479`.
- `crates/validate/tests/capability_matrix.rs` (new, 48 tests): (A) `:222-313`, (B) `:319-444`,
  (B′) `:450-946`, (C) `:952-1158`, (D) `:1164-1215`, (E) `:1221-1307`.
- 0017 §3 amendment: the third state `0017:196-199`; the version on the wire `0017:263-268`;
  editing `pre-0.1` in place until the first tag `0017:284-290`.
- Docs currency: the patch adds a library module, not a port, API operation, RPC, CLI flag or
  persisted field. The brief leaves `05-building-block-view.md` to 743.3, which adds the
  feature.

## How the test was run

The project wrapper `./engine/xtask.sh` runs whole gates only (`cargo xtask ci`). It does not
run a single test target. `run-verify.sh` is Check's C4-verify gate, and it reports
UNVERIFIABLE (exit 77) for a new module, as the brief's posture (a) says. For red→green and
the red legs I ran the exact target the brief names, `cargo test -p wyrd-validate --test
capability_matrix`, under `timeout 600`/`900`. The test is pure (no I/O, no network, no
clock), so it cannot hang. The timeout is a backstop. The full gate ran once on the final tree
(below).

## Refuting my own test

**(a) Genuine red? Yes.** Two forms:

- *The whole module reverted* (`pub mod matrix;` removed): the test cannot compile
  (`unresolved import wyrd_validate::matrix`). This is the declared posture (a). C4-verify will
  say UNVERIFIABLE.
- *Each behaviour reverted, one at a time.* A script in scratch made 18 temporary edits to the
  production `matrix.rs`. For each one it checked that the edit's anchor occurs exactly once,
  ran the test, and restored the file byte for byte. It checked the sha256 after every leg.
  `sha256sum -c` on both shipped files passed afterwards, and the final run on them was **48
  passed**. Every leg went red:

| # | Temporary edit to `matrix.rs` | Red test(s) |
|---|---|---|
| R0 | iteration 2's `matrix.rs` (iteration 3's fix reverted) | `a_cut_off_delete_result_never_hides_a_probe_that_reads_a_key`, `a_listing_of_a_prefix_with_nothing_written_proves_nothing`, `every_follow_up_is_judged_against_the_key_it_asked_about` |
| R1 | (B) a 2xx to an unsupported row → pass | `any_2xx_to_an_unsupported_row_is_unexpected_support`, `an_unexpected_success_fails_the_run`, `each_version_selects_its_own_profile_and_expectations`, `any_fatal_fails_the_run_naming_every_fatal_row` |
| R2 | (B) a 2xx head whose body failed → no answer | `any_2xx_to_an_unsupported_row_is_unexpected_support` |
| R3 | (B′) the PUT ETag check accepts any non-empty value | `put_etag_that_is_not_a_strong_tag_is_protocol`, `a_bodiless_answer_cut_off_after_its_head_is_not_proved`, `any_fatal_fails_the_run_naming_every_fatal_row` |
| R4 | (B′) the non-matching conditional GET accepts a 304 | `non_matching_conditional_get_answered_304_is_protocol` |
| R5 | (B′) `DeleteObjects` passes on its document alone | `delete_objects_document_alone_never_proves_the_delete`, `delete_follow_up_with_no_answer_or_a_5xx_is_inconclusive_not_oracle`, `an_unresolved_delete_names_the_delete_row`, `every_follow_up_is_judged_against_the_key_it_asked_about`, `a_cut_off_delete_result_never_hides_a_probe_that_reads_a_key` |
| R6 | (C) the selector ignores the version (`take(1)`) | `each_version_selects_its_own_profile_and_expectations`, `pre_0_1_covers_every_version_below_0_1_0`, `a_version_no_profile_covers_selects_none`, `overlapping_profiles_select_none`, `a_selection_that_finds_no_profile_says_why` |
| R7 | (C) `+git.unknown` read as release `0.0.0` | `unknown_is_never_read_as_release_0_0_0` |
| R8 | (D) a `supported, not checked` row also counted as unexercised | `a_not_checked_row_need_not_be_exercised_and_is_listed_as_not_covered` |
| R9 | (B′) the PUT check ignores when its head arrived | `put_answered_before_its_body_ended_is_protocol_whatever_its_etag`, `a_bodiless_answer_cut_off_after_its_head_is_not_proved` |
| R10 | (B′)/(E) a delete whose follow-up `HEAD` had no answer → `oracle` | `delete_follow_up_with_no_answer_or_a_5xx_is_inconclusive_not_oracle`, `an_unresolved_delete_names_the_delete_row`, `any_fatal_fails_the_run_naming_every_fatal_row`, `a_cut_off_delete_result_never_hides_a_probe_that_reads_a_key` |
| R11 | only iteration 3's bulk fix undone (return on a torn document before the probes) | `a_cut_off_delete_result_never_hides_a_probe_that_reads_a_key` |
| R12 | `\|\|` → `&&` in `parse_numeric` | `parse_refuses_malformed_versions` |
| R13 | an empty listing of an empty prefix passes | `a_listing_of_a_prefix_with_nothing_written_proves_nothing` |
| R14 | a duplicated key's probes skipped (early return) | `every_follow_up_is_judged_against_the_key_it_asked_about` |
| R15 | a probe paired with the wrong key (`==` → `!=`, iteration 1's surviving mutant) | `every_follow_up_is_judged_against_the_key_it_asked_about` |
| R16 | only the first probe of a key judged (a later readable `HEAD` dropped) | `every_follow_up_is_judged_against_the_key_it_asked_about` |
| R17 | a bodiless answer cut off after its head read as "answered with a body" (`protocol`) | `a_bodiless_answer_cut_off_after_its_head_is_not_proved` |

R1–R10 are the brief's ten required demonstrated reds. R0 and R11–R17 cover the review
findings of iterations 1–3. My first R8 edit did not compile: the build runs with `-D
warnings`, and an unused `mut` was an error. I rewrote it to keep the `not_covered` list and
also mark the row unexercised, then re-ran that leg alone. It went red on the named test. The
other 17 legs ran in one pass.

**(b) Production path? Yes.** Every test calls `wyrd_validate::matrix::{classify, verdict,
select, select_reported, parse_version, is_strong_etag}` and the `PRE_0_1` constant directly.
No copy, no mock. The legs edit those functions, and the tests notice.

**(c) Fixture includes the fault? Yes.** Each failing element is in the fixture itself: a 2xx
to an unsupported row, a cut-off body, a weak, unquoted or `*` ETag, an early PUT
acknowledgement, a 304 to a non-matching tag, a `<DeleteResult>` with an `<Error>` row, a
follow-up `HEAD` that reads a deleted key, gets no answer or a 503, a torn `<DeleteResult>`
next to a readable probe, `+git.unknown`, and an overlapping table. Full runs are built from
`PRE_0_1.rows()` (`passing_run`, `capability_matrix.rs:127-181`), so no declared row is left
out.

## Gates and commit-readiness

- `cargo fmt --all -- --check`: clean (worktree, and the `b552ca2` tree).
- `cargo clippy -p wyrd-validate --all-targets --all-features -- -D warnings`: clean (both).
- `typos` on the four changed files: clean. `lint_docs.py`: OK. `render_site.py --check`:
  "link audit OK", 99 pages rendered. These two are the brief's external dependencies, and both
  are present on this host.
- The target repo configures no commit hooks (no `core.hooksPath`, no pre-commit config), so
  the formatter and the prose gates above are what a commit has to pass.
- `patch.diff` is byte-identical to `git diff` in the worktree (the two new files are marked
  intent-to-add), and it applies with `git apply --check` on `93a6f37`.
- Size: `patch.diff` is **99,441 bytes**, under the 100 KB backstop.
- `./engine/xtask.sh ci` (C4-ci) on the final tree, with `PDCA_WORKTREE` set to this worktree:
  **exit 1. Only two checks failed, both the advisory wall described under *Base*:**
  `cargo deny check` (advisories FAILED; bans, licenses and sources ok) and `cargo deny
  --all-features --config deny-all-features.toml check advisories`, both on RUSTSEC-2026-0385
  and RUSTSEC-2026-0386. Every other step passed: `typos`, `lint_docs.py`,
  `render_site.py --check`, the gitlink, unsafe and blackbox guards, `cargo fmt --check`,
  workspace clippy, build and test (214 test binaries ok, none failed; `capability_matrix` 48
  passed), `cargo-machete`, `cargo deny --all-features check licenses bans sources`, the
  statics and deploy guards, and the DST tier (`--cfg madsim` clippy and test). Iteration 3's
  `custodian_gc` hang did not recur: the host was not loaded this time.

NEEDS-HUMAN external dependency: cargo deny all-features advisories — the run's stack base `93a6f37` (`pdca-integration/main-r42c5348f203d`) still pins `ctutils 0.4.2` and `tokio 1.52.3` (`Cargo.lock`), so `cargo deny check` and the all-features advisories check fail on the unpatched base (RUSTSEC-2026-0385, RUSTSEC-2026-0386) and C4-ci cannot go green on it; #950/#954 fixed both on `main` `b552ca2` but not on the integration line, and the standing doctor row checks `../wyrd` (`b552ca2`, green), so it will not flag this. This patch touches no manifest, lockfile or deny config; refresh the integration line onto `b552ca2` (or rebase this bundle on `main` with the one-line `lib.rs` change above) and re-run Check

## Scratch

Everything this round made outside the worktree is under `$PDCA_SCRATCH`
(`/var/tmp/pdca/wyrd-pdca-9c587031/issue_937/`): `pdca-builder-937-it4-redlegs` (the red-leg
script, its logs and the sha256 list), `pdca-builder-937-it4-main` (the `b552ca2` export and
its `target/`, the large one), `pdca-builder-937-it4-docs` (the rendered site),
`pdca-builder-937-it4-ci` (the full-gate log), `pdca-builder-937-it4-base` (a fresh `93a6f37`
export, used only for `git apply --check`). I did not delete them: the harness owns that
root and reclaims it. `pdca-builder-937-redlegs` there is iteration 2's and was already
present.
