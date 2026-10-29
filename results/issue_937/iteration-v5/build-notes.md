# Build notes — #937 (743.1) validate: the capability matrix — iteration 5

Withheld from the reviewer. For the human at sign-off. Line numbers are on the worktree base
below with the patch applied, unless a line says otherwise.

## What this round is

Iteration 4's sign-off asked for two things at the implementation level: fix the 0017
*Dependencies* paragraph that now contradicts the amended §3, and get full-CI evidence on a base
whose advisories are clear. Iteration 3's sign-off (still standing) asked that the matrix be
carried forward unchanged in substance.

- **The matrix, the test and the `lib.rs` line are byte-identical to iterations 3 and 4.**
  sha256 `matrix.rs` `f1431971…eca12`, the test `05779312…6790` — the same values iteration 4
  recorded. `diff iteration-v4/patch.diff patch.diff` shows one new hunk (the *Dependencies*
  paragraph) and the doc's blob id, nothing else.
- **One doc change: the *Dependencies* paragraph of 0017** (`docs/design/proposals/draft/0017-blackbox-validation-tool.md:873-880`).
  Details under *What changed*.
- **The advisory wall is still there, for the same reason as iteration 4.** The worktree the
  driver gave this Do is again `93a6f37`. Details and the NEEDS-HUMAN marker under *Base*.

## Base: read this first (it decides whether C4-ci can go green)

- **The worktree is `93a6f37`**, the tip of `pdca-integration/main-r42c5348f203d` (`main`
  `4bac538` + #744 `61e5341`, #853 `582da99`, #932 `93a6f37`). The bundle's `stack-base-tip`
  still says `93a6f37`. `b552ca2` (the base iteration 3's sign-off named) is **not** an
  ancestor: `git merge-base --is-ancestor b552ca2 HEAD` exits 1. Local `main` is at `b552ca2`.
- **Why I built here.** Check rebuilds base + `patch.diff` from the stack base. A patch built
  for `b552ca2` does not apply on `93a6f37` (the `lib.rs` module list differs; iteration 4's
  notes show the hunk). The harness owns the worktree base; I do not.
- **What it costs, confirmed on this round's unpatched base.** `Cargo.lock` here pins
  `ctutils 0.4.2` (`Cargo.lock:1000-1001`) and `tokio 1.52.3` (`:3953-3954`). Before applying
  the patch I ran `cargo deny --all-features --config deny-all-features.toml check advisories`
  in the worktree: **exit 1**, `error[vulnerability] RUSTSEC-2026-0385` (ctutils) and
  `error[unsound] RUSTSEC-2026-0386` (tokio). #950 and #954 fixed both on `main`; the
  integration line does not have them. The patch touches no `Cargo.toml`, `Cargo.lock` or
  `deny*.toml` (the brief puts manifests and deny config out of scope, and bumping the lockfile
  here would duplicate #950/#954 and conflict on merge).
- **The standing doctor row will read green.** Its detect command runs in `../wyrd`
  (`pdca.toml:1411`), which is at `b552ca2` where both checks pass. C4-ci runs on `93a6f37`,
  where they fail. The row cannot see the mismatch; only a refresh of the integration line
  (or a rebase of this bundle onto `main`, with the one-line `lib.rs` hunk from iteration 4's
  notes) clears it.
- #508 is not on this base: the denylist still lists `uploads`, `uploadId`, `partNumber`
  (`crates/gateway-s3/src/lib.rs:410-413`), so the six multipart rows stay
  `unsupported(#508)` (`crates/validate/src/matrix.rs:326-367`).
- I re-read every gateway line the rows cite on this base (`:1842-1849`, `:410-422`,
  `:1766-1788`, `:1797-1804`, `:1809-1815`, `:1819-1826`, `:1865-1871`, `:2008`, `:2017`,
  `:2025-2028`, `:2293-2300`, `:2469`, `:2570`, `:2713-2719`, `:2785-2803`, `:2995`, `:3401`)
  and `crates/server/src/version/derivation.rs:30-41`, `:65`, `:121`. Each still gives the
  answer its row or comment claims.
- **Expect C4-diff-cov to say "does not apply on origin/main" again** (the `lib.rs` hunk).

NEEDS-HUMAN external dependency: cargo deny all-features advisories — the run's stack base `93a6f37` (`pdca-integration/main-r42c5348f203d`) still pins `ctutils 0.4.2` and `tokio 1.52.3` (`Cargo.lock:1000-1001`, `:3953-3954`), so `cargo deny --all-features --config deny-all-features.toml check advisories` exits 1 on the unpatched base (RUSTSEC-2026-0385, RUSTSEC-2026-0386; confirmed this round before the patch was applied) and C4-ci cannot go green on it; #950/#954 fixed both on `main` `b552ca2` but not on the integration line, and the standing doctor row runs in `../wyrd` (`b552ca2`, green), so it will not flag this. This patch touches no manifest, lockfile or deny config; refresh the integration line onto `b552ca2` (or rebase this bundle on `main` with the one-line `lib.rs` hunk in iteration 4's notes) and re-run Check

(The case is already registered — the standing row at `pdca.toml:1410-1411` — so no new
`[[doctor.checks]]` row is proposed. What that row cannot do is run against the stack base
instead of `../wyrd`; that is a harness question, not build work.)

## What changed since iteration 4

**The 0017 *Dependencies* paragraph**, `0017:873-880` (base `:858-863`). Iteration 4's review
found that amended §3 (`0017:263-268`) says every handler response carries
`Server: wyrd/<version>` and points the reader to *Dependencies*, which still said "**The
gateway advertises no version.** No `Server:` header, nothing on the wire … Interim:
`--server-version`." The two passages contradicted each other after the patch. The review
offered two fixes: record that #736 landed through #886, or drop the cross-reference. I took
the first: the paragraph now reads "**The gateway advertises its version — landed.** [#736]
landed through #886 …", keeps the `normalize_describe` provenance, and says `--server-version`
is an override. Eight lines replace six.

Why in place rather than "decline with an issue reference": the brief scoped "the 0017 §3
amendment", and this paragraph is in *Dependencies*. But it is the same draft proposal the
patch already edits, the finding is a contradiction the patch itself introduced, and an issue
would cost a tracker entry for a one-paragraph doc fix. The reviewer named this exact option.
If you would rather keep the patch to §3, drop the hunk at `patch.diff:2857-2877`; nothing else
depends on it.

I verified the "landed" claim end to end on this base before writing it: `crates/server/build.rs:62-63`
includes `src/version/derivation.rs` (where `normalize_describe` is, `:41`), `:117` bakes
`WYRD_BUILD_IDENTITY`; `crates/server/src/cli.rs:2681` sets `config.server_version` from
`version::BUILD_IDENTITY`; the gateway inserts the `Server` header on every response its
handler returns, `crates/gateway-s3/src/lib.rs:1693-1699`. The old paragraph's citation
`xtask/src/dist.rs:119,356` was stale (`:119` is a struct field, `:356` is
`INSTALL_TIME_TOKENS`); the use is at `xtask/src/dist.rs:546`, which the new text cites.

Nothing else changed. Iteration 4's one-sentence §3 edit ("every response the gateway's handler
returns", `0017:263-264`) is kept.

## Open design points: still yours to decide

Unchanged from iteration 4, as asked: current behaviour kept, rustdoc consistent with the code.

1. **Strict (E).** A row is proved only when it was exercised and *every* exercise of it passed
   (`crates/validate/src/matrix.rs:1393-1396`, enforced at `:1397-1421`; pinned by
   `a_row_passes_only_when_every_exercise_of_it_passed`, `capability_matrix.rs:1301`). Brief
   (E) says "exercised only with no answer". The strict rule never claims more than the answers
   prove; a PUT or DELETE with no answer is an unresolved mutation (0017 §7). But one timed-out
   *read* retry then ends the run INCONCLUSIVE, which fits poorly with §7's budgeted
   `availability`. 743.3 (#939) is the first caller; decide before it builds retries on this.
   Relaxing it to the brief's wording is about 5 lines in `verdict` plus flipping that test.
2. **`0.1.0-rc.1` selects `pre-0.1`.** The brief says `pre-0.1` covers everything below `0.1.0`
   (`matrix.rs:406-413`; pinned at `capability_matrix.rs:285`). The amendment says "Retention
   starts at the first tag" (`0017:287`). If the first tag is `v0.1.0-rc.1`, that tag lands in
   the profile that is edited in place. Either end the range at the first tag or say "the first
   `0.1.0` release" in the amendment. I left it as the brief states.
3. **Listing a prefix with nothing written is unsettled, not PASS** (`matrix.rs:1170-1173`;
   pinned by `a_listing_of_a_prefix_with_nothing_written_proves_nothing`,
   `capability_matrix.rs:912`). A rule I added in iteration 3; the brief does not ask for it. It
   is the same vacuous pass the module refuses for a `DeleteObjects` of no keys
   (`matrix.rs:1218-1220`). A key listed where nothing was written is still `oracle`. To drop
   it: 4 lines of code, 9 of test.

For the record (unchanged since iteration 2): a 5xx other than 501 on a required row is
unsettled (`availability`, `matrix.rs:962-970`); a 501 is `protocol`. On an `unsupported` row
any wrong status, a 5xx included, is `protocol` (brief B; `matrix.rs:936-941`).

## Where each part of the brief lives (on `93a6f37` + patch)

- `crates/validate/src/lib.rs:24`: `pub mod matrix;`, the brief's one line.
- `crates/validate/src/matrix.rs` (new, 1,479 lines):
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
- `crates/validate/tests/capability_matrix.rs` (new, 48 tests): (A) `:222-313`, (B) `:319-444`,
  (B′) `:450-946`, (C) `:952-1158`, (D) `:1164-1215`, (E) `:1221-1307`.
- 0017: the third state `0017:196-199`; the version on the wire `:263-268`; editing `pre-0.1`
  in place until the first tag `:284-290`; *Dependencies* `:873-880`.
- Docs currency: the patch adds a library module, not a port, API operation, RPC, CLI flag or
  persisted field. The brief leaves `05-building-block-view.md` to 743.3, which adds the
  feature.

## How the test was run

The project wrapper `./engine/xtask.sh` runs whole gates only (`cargo xtask ci`; `xtask` has
no single-target test subcommand, `xtask/src/main.rs:119-150`). For red→green and the red legs
I ran the exact target the brief names, `cargo test -p wyrd-validate --test capability_matrix`,
under `timeout 600`/`900` from the worktree. The test is pure (no I/O, no network, no clock),
so it cannot hang; the timeout is a backstop. The whole gate ran once on the final tree (below).

## Refuting my own test

**(a) Genuine red? Yes.** Two forms, both on this round's tree:

- *The whole module reverted* (`pub mod matrix;` removed from `lib.rs`): the test target does
  not compile — `error[E0432]: unresolved import wyrd_validate::matrix`, exit 101. The
  declared posture (a); C4-verify will say UNVERIFIABLE (exit 77). `lib.rs` was restored from a
  copy and `git diff --stat` shows the one-line change again.
- *Each behaviour reverted, one at a time.* The red-leg script from iteration 4 (copied into
  `$PDCA_SCRATCH/pdca-builder-937-it5-redlegs/`, run with `python3 -I`) made 18 temporary
  edits to the production `matrix.rs`. For each it checked the anchor occurs exactly once, ran
  the target, and restored the file byte for byte (sha256 checked after every leg).
  `sha256sum -c` on both shipped files passed afterwards; the final run on them was **48
  passed**. Every leg went red (`redlegs.log` there has the per-leg failing test names):

| # | Temporary edit to `matrix.rs` | Result | Red test(s) |
|---|---|---|---|
| R0 | iteration 2's `matrix.rs` (iteration 3's fix reverted) | 45 passed, 3 failed | `a_cut_off_delete_result_never_hides_a_probe_that_reads_a_key`, `a_listing_of_a_prefix_with_nothing_written_proves_nothing`, `every_follow_up_is_judged_against_the_key_it_asked_about` |
| R1 | (B) a 2xx to an unsupported row → pass | 44/4 | `any_2xx_to_an_unsupported_row_is_unexpected_support`, `an_unexpected_success_fails_the_run`, `each_version_selects_its_own_profile_and_expectations`, `any_fatal_fails_the_run_naming_every_fatal_row` |
| R2 | (B) a 2xx head whose body failed → no answer | 47/1 | `any_2xx_to_an_unsupported_row_is_unexpected_support` |
| R3 | (B′) the PUT ETag check accepts any non-empty value | 45/3 | `put_etag_that_is_not_a_strong_tag_is_protocol`, `a_bodiless_answer_cut_off_after_its_head_is_not_proved`, `any_fatal_fails_the_run_naming_every_fatal_row` |
| R4 | (B′) the non-matching conditional GET accepts a 304 | 47/1 | `non_matching_conditional_get_answered_304_is_protocol` |
| R5 | (B′) `DeleteObjects` passes on its document alone | 43/5 | `delete_objects_document_alone_never_proves_the_delete`, `delete_follow_up_with_no_answer_or_a_5xx_is_inconclusive_not_oracle`, `an_unresolved_delete_names_the_delete_row`, `every_follow_up_is_judged_against_the_key_it_asked_about`, `a_cut_off_delete_result_never_hides_a_probe_that_reads_a_key` |
| R6 | (C) the selector ignores the version (`take(1)`) | 43/5 | `each_version_selects_its_own_profile_and_expectations`, `pre_0_1_covers_every_version_below_0_1_0`, `a_version_no_profile_covers_selects_none`, `overlapping_profiles_select_none`, `a_selection_that_finds_no_profile_says_why` |
| R7 | (C) `+git.unknown` read as release `0.0.0` | 47/1 | `unknown_is_never_read_as_release_0_0_0` |
| R8 | (D) a `supported, not checked` row also counted as unexercised | 47/1 | `a_not_checked_row_need_not_be_exercised_and_is_listed_as_not_covered` |
| R9 | (B′) the PUT check ignores when its head arrived | 46/2 | `put_answered_before_its_body_ended_is_protocol_whatever_its_etag`, `a_bodiless_answer_cut_off_after_its_head_is_not_proved` |
| R10 | (B′)/(E) a delete whose follow-up `HEAD` had no answer → `oracle` | 44/4 | `delete_follow_up_with_no_answer_or_a_5xx_is_inconclusive_not_oracle`, `an_unresolved_delete_names_the_delete_row`, `any_fatal_fails_the_run_naming_every_fatal_row`, `a_cut_off_delete_result_never_hides_a_probe_that_reads_a_key` |
| R11 | only iteration 3's bulk fix undone (return on a torn document before the probes) | 47/1 | `a_cut_off_delete_result_never_hides_a_probe_that_reads_a_key` |
| R12 | `\|\|` → `&&` in `parse_numeric` | 47/1 | `parse_refuses_malformed_versions` |
| R13 | an empty listing of an empty prefix passes | 47/1 | `a_listing_of_a_prefix_with_nothing_written_proves_nothing` |
| R14 | a duplicated key's probes skipped (early return) | 47/1 | `every_follow_up_is_judged_against_the_key_it_asked_about` |
| R15 | a probe paired with the wrong key (`==` → `!=`, iteration 1's surviving mutant) | 47/1 | `every_follow_up_is_judged_against_the_key_it_asked_about` |
| R16 | only the first probe of a key judged (a later readable `HEAD` dropped) | 47/1 | `every_follow_up_is_judged_against_the_key_it_asked_about` |
| R17 | a bodiless answer cut off after its head read as "answered with a body" (`protocol`) | 47/1 | `a_bodiless_answer_cut_off_after_its_head_is_not_proved` |

R1–R10 are the brief's ten required demonstrated reds. R0 and R11–R17 cover the review findings
of iterations 1–3.

**(b) Production path? Yes.** Every test calls `wyrd_validate::matrix::{classify, verdict,
select, select_reported, parse_version, is_strong_etag}` and the `PRE_0_1` constant directly
(`capability_matrix.rs:7-13`). No copy, no mock. The legs edit those functions and the tests
notice.

**(c) Fixture includes the fault? Yes.** Each failing element is in the fixture itself: a 2xx
to an unsupported row, a cut-off body, a weak, unquoted or `*` ETag, an early PUT
acknowledgement, a 304 to a non-matching tag, a `<DeleteResult>` with an `<Error>` row, a
follow-up `HEAD` that reads a deleted key, gets no answer or a 503, a torn `<DeleteResult>`
next to a readable probe, `+git.unknown`, and an overlapping table. Full runs are built from
`PRE_0_1.rows()` (`passing_run`, `capability_matrix.rs:127-181`), so no declared row is left
out.

## Gates and commit-readiness

- `cargo fmt --all -- --check`: clean.
- `cargo clippy -p wyrd-validate --all-targets --all-features -- -D warnings`: clean.
- `typos` on the four changed files: clean. `lint_docs.py`: OK. `render_site.py --check`:
  "link audit OK", 99 pages. Both of the brief's external dependencies (`typos`,
  `docs-renderer`) are present on this host (`typos-cli 1.48.0`; `markdown_it`/`yaml` import).
- The target repo configures no commit hooks (no `core.hooksPath`, no pre-commit config), so
  the formatter and the prose gates above are what a commit has to pass.
- `patch.diff` is `git diff` of the worktree (the two new files intent-to-add); it reverses
  cleanly on the worktree (`git apply --check -R`), so it applies on `93a6f37`.
- Size: **101,078 bytes = 98.7 KB.** The backstop fires at `patch_bytes / 1024 >= 100`
  (`src/pdca_harness/size_signal.py:490-491`; `patch_kb = 100`), i.e. at 102,400 bytes, so the
  patch is under it with 1.3 KB to spare. The growth over iteration 4 (99,441) is the one
  *Dependencies* hunk (1,637 bytes). I did not trim the matrix or the test to buy headroom:
  that would break the byte-identity the carry-forward asked for.
- `./engine/xtask.sh ci` (C4-ci) on the final tree: **see the next section.**

## The whole gate on the final tree

`/home/eddie/wyrd/wyrd-pdca/engine/xtask.sh ci` with `PDCA_WORKTREE` set to this worktree,
on the final tree (`$PDCA_SCRATCH/pdca-builder-937-it5-ci/ci.log`, 4,206 lines):

**Exit 1. Exactly two checks failed, both the advisory wall described under *Base*:**

```
xtask: 2 checks failed:
  - cargo deny check: exited with exit status: 1
  - cargo deny --all-features --config deny-all-features.toml check advisories: exited with exit status: 1
```

(`ci.log:3642` `advisories FAILED, bans ok, licenses ok, sources ok`; `:4002` `advisories
FAILED`; the IDs at `:3337`, `:3419`, `:3653`, `:3735` are RUSTSEC-2026-0385 and
RUSTSEC-2026-0386, the same two as on the unpatched base.)

Every other step passed: `typos`, `lint_docs.py` (OK), `render_site.py --check` (link audit
OK), the gitlink, unsafe and blackbox guards (`wyrd-validate` links no `wyrd-*` crate),
`cargo fmt --all -- --check`, workspace clippy, build and test — 199 test binaries, 222
`test result: ok` lines, 0 `FAILED`; `capability_matrix` **48 passed** at `ci.log:2697` —
`cargo-machete`, `cargo deny --all-features check licenses bans sources`, the statics and
deploy guards, and the DST tier (`--cfg madsim` clippy and test, all ok). No hang this time.

## Scratch

Everything this round made outside the worktree is under `$PDCA_SCRATCH`
(`/var/tmp/pdca/wyrd-pdca-9c587031/issue_937/`): `pdca-builder-937-it5-redlegs` (the red-leg
script, `matrix-iter2.rs` for R0, `lib.rs.orig`, `redlegs.log`, `shipped.sha256`),
`pdca-builder-937-it5-docs` (the rendered site), `pdca-builder-937-it5-ci` (the whole-gate
log). The `-it4-*` and older dirs there are previous rounds'. I did not delete any of them: the
harness owns that root and reclaims it.
