# Build notes — #1008 (636.5b): Complete's flat publication, lost flips and the bucket pin

Base: the cycle worktree's HEAD, `60e5d2c` (`pdca-integrate: issue_956`), the bundle's
`stack-base-tip`. `patch.diff` applies to it. Line numbers below are on the **patched** tree.

## The base check the brief asked for first

The base holds both prerequisites, so I built rather than stopped:

- **#1007**: merged to `main` as PR #1032 (`8932adb`), so `crates/core/src/multipart/complete.rs`
  (the fence phase) and `crates/core/tests/multipart_complete.rs` (its 43 legs and the logging
  wrapper) exist on the base.
- **#1027**: folded in as `49463ec` (`pdca-integrate: issue_1027`), so `commit::upload_part`
  exists (`crates/core/src/multipart/commit.rs:712`).

The patch **modifies** the test file and adds none, so C4-verify's green-only reading applies,
as the brief says. The per-fix reds are the demonstrated ones below.

## What changed, and where

**`crates/core/src/multipart/complete.rs`** (the slice's module):

| What | Where |
|---|---|
| Module doc: the new *Publication* section (the flip's contribution table, the lost-flip table, what a refused or unknown flip does) and the clock paragraph | `:85-132`, `:143-150` |
| `PublishRequest { fresh_inode, completed_at_millis }`, the caller's inputs | `:1127` |
| `complete_upload`, the public verb | `:1165` |
| `publish_flat`: assemble, read the target, the flip loop with the re-reads after a lost flip | `:1178` (session check `:1265`, marker check `:1279`, `R_PUBLISH` exhaustion `:1282`, unknown outcome `:1245`) |
| `give_up` (one release, as a fault behind the fence) | `:1295` |
| `FlipPlan::read` (each named `part:` record must equal its frozen summary; unnamed parts) and `FlipPlan::flip` (version from the re-read prior, the `Completed` record, the contribution, the bucket pin at `:1431`) | `:1336`, `:1383` |
| `read_target`, `superseded` (the empty-prior rule) | `:1457`, `:1485` |
| `Admit { Any, Flat }`: the flat-only check before and behind the fence | `:688`, used at `:777` (before, no commit) and `:825-839` (behind, one release) |
| `fence_complete` now delegates to `fence(…, Admit::Any)`; behaviour unchanged | `:678`, `:723` |
| `HeldFence.released` (the release bytes minted with the fence, so publication can release) | `:274` |
| `DoubtfulCommit::Flip`; `Stop::{SegmentedUnsupported, PartDisagrees, Target, Publication}` with `Display`/`source` | `:384`, `:470-489` |
| `release`, `moved` and `ReleaseFor::end` return `CompleteOutcome` instead of `FenceOutcome`, so the publication path reuses them; `fence` wraps with `FenceOutcome::Answered` | `:1044`, `:1098`, `:992` |
| The `deferred: #660` DST marker now names the flip's races too | `:613` |

**`crates/core/src/multipart.rs`** (pure records; still no store call):

| What | Where |
|---|---|
| `SessionRecord::completed`: `Completing@E → Completed@E+1`, `attempts` kept | `:2702` |
| `RetireObligation::published_parts` (records mode), `unnamed_parts` (bytes mode), both `{parts}` under `s:<id>:<E>`; `superseded_generation` (`g:<inode>:<version>`, refuses an empty flat list as decode does) | `:4224`, `:4232`, `:4237`, `:4260` |
| Unit tests: each new obligation decodes against its own key; `completed` only from `Completing` below `u64::MAX` | `mod flip_records`, `:5946` |
| Doc updates: module header, `SessionRecord`, `RetiredMap`, `RetirePayload`, `RetireObligation` writer lists | `:10-13`, `:104-125`, others |

**`docs/design/architecture/05-building-block-view.md:254`**: a new paragraph on the verb (flat
only, the flip moves the session to `Completed` in the same batch as the root and its
obligations, the lost-flip rules, nothing calls it in production until #670). The fence-phase
paragraph's "publishes nothing yet: publication is #1008–#1011" now says the verb below publishes.

**The test file**, appended (`crates/core/tests/multipart_complete.rs:2675` on). Shared changes to
#1007's scaffolding, kept small: `OnCommit::Run(Interfere, …)` runs a real operation on the inner
store before a commit (`:337`, trait `:343`); `upload_with` lets a session declare a content type
(`:548`); `stopped_error` knows the two new source-carrying stops (`:701`); `in_doubt_error` knows
`DoubtfulCommit::Flip` (`:771`). The rest is new: the placement-aware chunk store `Fleet`
(`:2747`), the `Stager` that drives #1027's `upload_part` and ordinary `PutObject`s through
`write.rs` (`:2881`), and these legs:

| Brief item | Leg |
|---|---|
| (A) | `a_parts_staged_out_of_order_publish_one_flat_object_that_reads_back` `:3330` |
| (D2-tail) | `d2_tail_after_typos_and_replaced_parts_a_valid_list_publishes` `:3425` (D2's replacement is a real `UploadPart` landing during the fence commit) |
| (C) | `c_a_subset_complete_retires_the_published_records_and_the_unnamed_bytes` `:3490` |
| (F) | `f_a_flip_that_loses_to_an_overwrite_publishes_the_next_version_and_retires_it` `:3562`; exhaustion `f_when_every_flip_loses_the_fence_is_released_and_the_session_stays_usable` `:3650` |
| (F2) | `f2_superseding_a_zero_byte_prior_installs_no_obligation_for_it` `:3723` |
| (H2) | `h2_a_bucket_removed_just_before_the_flip_answers_no_such_bucket` `:3763` (removed, and recreated with other bytes) |
| (J) | `j_a_flip_the_store_refuses_for_good_releases_the_fence_and_stops` `:4026` (and the case where that release fails too) |
| (G0-flip) | `g0_an_unknown_flip_outcome_returns_the_in_doubt_error_and_nothing_more` `:4093` |
| (K) | `k_a_list_past_one_flat_map_stops_before_the_fence` `:4140`, `k_a_list_only_the_frozen_set_shows_past_one_flat_map_releases_the_fence` `:4179` |
| beyond the brief | another writer takes the fence (`:3811`), damaged part or target (`:3858`), failing reads after a lost flip (`:3968`), a segmented prior (`:4267`), the fence phase alone still holds a list past one flat map (`:4315`) |

## Decisions, and what I ruled out

**Publish through #653's committer, unchanged.** `staged_publication::publish`
(`crates/core/src/staged_publication.rs:445`) already pins the dirent and inode on both branches
(`:917-927`) and applies `overwrite_modified` (`:885`), which is what the brief cites at
`05-building-block-view.md:219`. The flip's contribution pins `mpu:<id>` at the fence's bytes, which
satisfies its fence rules (`:837-857`: exactly one pin, and the put moves it). I pass a
`no_segments` contribution (`complete.rs:1309`) that the committer only calls for a map past
`MAX_MAP_CHUNKS`, which the `Admit::Flat` check rules out.

**`Admit` inside the fence phase, not a separate check in the verb.** K needs zero commits when the
pre-fence read shows the excess, and one release when only the frozen set does. A pre-check in
`complete_upload` before calling `fence_complete` would need its own session read and `psum:` walk
(roughly 25 lines duplicating `validate`), and it would answer K before decision 3's state table,
so an `Aborting` or `Completing` session would get "segmented unsupported" instead of
`NoSuchUpload`/`OperationAborted`. Behind the fence it would also need the verb to release a fence
`fence_complete` handed back. The `Admit` enum costs 35 lines (`:688-720`) and two call sites, and
the state answers, clock, budget and validation all still come first. #1009 deletes the `Flat` arm.

**The session is re-read after a lost flip** (`:1258-1272`). The brief's design re-reads the dirent
and the marker. I also read the session first: if another writer moved it (the reaper's rollback, a
teardown fence), the fence is not this Complete's any more, so the request is answered from the
new state with no release. Without it the answer comes out the same, but only after the
remaining `R_PUBLISH - 1 = 2` flips and one release commit all conflict, plus four more target
reads. Cost: 13 lines. Leg `:3811` pins it (the mutation X5 below goes red on the commit count).

**Which release each stop gets.** `NoSuchBucket` and `R_PUBLISH` exhaustion are answers, so they
get a rejection's release (`RELEASE_ROUNDS`, a lock conflict retried). A refused flip (J), a
fault reading parts or target, a damaged record, a committer refusal, and K behind the fence are
errors, so they get the fault-behind-the-fence release: one commit, and the error says whether the
fence came down (`ReleaseFor::Fault`, doc broadened at `:992-1003`). That matches the brief's J
text ("If that release cannot land, child-1's stopped case with the fence standing applies").

**One stamp.** `PublishRequest::completed_at_millis` stamps `Completion.completed_at_millis` and the
root's `modified`. The first is a session-scoped stamp, so it must come from the session's clock
(`0016:1966-1976`); the fence already checked the request's clock is the session's. That clock is
the gateway's wall clock (`0016:1955-1958`), the same clock any other publication's `modified` is
read on. Two parameters would let a caller pass two clocks. Written up in the module doc,
`complete.rs:143-150`, per the rubric's "a new clock read states which source owns its lifecycle".

**The fresh inode id is the caller's** (`PublishRequest::fresh_inode`), as `write::commit_create`
takes one (`crates/core/src/write.rs:271`). A lost flip wrote nothing, so every round may reuse it.

**A segmented prior is retired by its group, not refused.** `superseded` (`:1485`) has to match
`ChunkMap::Segmented` anyway. 0016 specifies `{generation: {segments}}` for it (`0016:2417`), the
record shape exists (`RetiredMap::Segmented`), and refusing would make a later flat Complete fail
over any object #1009 publishes. Leg `:4267` publishes a real segmented prior through the
committer and checks the obligation.

**Two redundant checks removed after review.** `FlipPlan::read` first compared the record's digest
to the list's and re-counted the assembled chunks against `MAX_MAP_CHUNKS`. Both are implied: the
summary comparison covers the digest (and validation already matched summary to list), and the
summary counts are what `Admit::Flat` bounded. No test could tell either check from its absence,
so I removed them and wrote the reasoning in the doc (`:1331-1335`, `:1304-1308`).

**Not used: `unwrap`/`expect`/`unreachable!` in production.** The digests over a validated list,
the `PartNumberSet` of a non-empty list and `completed()` on a fresh fence cannot fail, but each
failure still maps to a typed stop with one release.

## Demonstrated reds

Each mutation is one temporary edit to `complete.rs` or `multipart.rs`, applied by script, run
through the whole test file (`--no-fail-fast`), then restored and checked with `cmp`. The script and
its log are in the bundle's scratch dir. **25 mutations, 25 red, none by a compile failure** (the
repo denies warnings, so each edit was written to compile clean).

The six the brief requires:

| # | Mutation | Leg that goes red, and how |
|---|---|---|
| R1 | `retire:records:` names every frozen part | (C): `left: [(1, 3)]`, `right: [(1, 1), (3, 3)]`, "only the published parts" |
| R2 | the prior frozen at fence time (no target re-read after a lost flip) | (F): `expected Published, got Refused(OperationAborted)`; F-exhausted too |
| R3 | a generation obligation for any existing prior, empty or not | (F2): `Stopped { fence: Released, cause: Record(RetireObligationOwesNothing { "generation.chunks" }) }`. With the constructor's refusal removed as well, the obligation is installed and F2 fails on `retire:bytes:g:900:1 does not decode` |
| R4 | the bucket pin dropped from the flip | (H2): `expected a refusal, got Published`; (A)'s `assert_flip` too |
| R5 | a definite flip error answered `OperationAborted` without a release | (J): the error class assertion, `left: None, right: Some(Transient)` |
| R6 | an unknown flip outcome treated as `Conflict` | (G0-flip): `expected the in-doubt error, never an answer: got AlreadyCompleted(…)` |

The other nineteen, each aimed at one rule of the new code:

| # | Mutation | Red leg(s) |
|---|---|---|
| X1 | no flat check before the fence | K before the fence (commits) |
| X2 | no flat check behind the fence | K behind the fence (the cause becomes the committer's refusal) |
| X3 | `>` → `>=` on the flat ceiling | both K legs (158 must publish) |
| X4 | `fence_complete` alone refuses past one flat map | `k_the_fence_phase_on_its_own_holds_…` |
| X5 | a session moved off the fence not checked | another-writer leg (5 commits, not 2) |
| X6 | marker checked for presence only | H2 "recreated" |
| X7 | one flip more than `R_PUBLISH` | F-exhausted |
| X8 | a part record never checked against its summary | damaged part "replaced" |
| X9 | unnamed parts inverted | C, F2 |
| X10 | obligations not under `require_absent` | A, H2 (`assert_flip`) |
| X11 | obligation token one epoch late | C |
| X12 | content type dropped | A |
| X13 | no publication stamp on the root | A, F, F-exhausted, F2 |
| X14 | a dangling dirent read as a free name | damaged target |
| X15 | the completion records version 1 on a supersede | F, F-exhausted, F2, F3 |
| X16 | a fresh flip records version 2 | A |
| X17 | `give_up` sends no release | J, damaged part/target, failing reads |
| X18 | `Completed` keeps the `Completing` epoch | A |
| X19 | a segmented prior gets no obligation | F3 |

## How I ran the checks

- **The project's runner.** `./engine/xtask.sh ci` (the `[gates] runner` in `pdca.toml`, which
  runs `cargo xtask ci` in `$PDCA_WORKTREE`), wrapped in `timeout 5400`. Its result is at the end
  of this file.
- **Iteration and the reds.** The runner has no single-test subcommand, so for the per-leg runs I
  used `cargo test -p wyrd-core --test multipart_complete`, always under `timeout 900`, so no run
  could hang the beat. The legs carry their own 120 s bound (`within`, `:139`).
- `cargo clippy -p wyrd-core --all-targets -- -D warnings`: clean. `cargo fmt -p wyrd-core`: applied.
- `typos` on the four changed files: clean. `python3 docs/publishing/tools/lint_docs.py`: OK.
  `python3 docs/publishing/tools/render_site.py --check`: 101 pages, link audit OK (output in
  scratch). These are the brief's `typos` and `docs-renderer` dependencies; both were present.

**One mistake to report.** My first `xtask ci` run went red on the DST crate's clippy and test
steps. That was my doing: I edited `complete.rs` while the run was compiling the `--cfg madsim`
build, and it compiled a half-edited file (`cannot find value digest`). Every other step of that
run passed. I re-ran the whole gate afterwards with no file touched during it.

## Refuting my own test

- **(a) Genuine red?** Reverting the whole fix removes `complete_upload`, `PublishRequest` and the
  new variants, so the test file stops compiling: that is a build failure, not a red. That is why
  the brief puts this bundle at green-only. The binding evidence is the 25 single-rule mutations
  above: each leaves the test file unchanged, compiles, and turns at least one leg red.
- **(b) Production path?** Yes. Every leg calls the production `complete_upload` against a real
  `RedbMetadataStore`. Sessions come from the real Create, parts from #1027's real `upload_part`
  (stage and commit) onto the chunk-store double, priors and interlopers from the real `write.rs`
  PUT path, and the flip from #653's real committer. Read-back is the real `read::read_object`.
  The wrapper only scripts commit outcomes and runs those real operations at a commit point.
- **(c) Fixture includes the fault?** Yes. Each fault is injected at the flip's own commit: a real
  overwrite lands first (F), the marker is deleted or rewritten (H2), the store answers a
  definite error (J) or an unknown outcome, applied and not (G0), the session is moved (another
  writer), or a read is armed to fail. F2's prior is a real empty PUT. K's frozen-only case is a
  real re-upload of the same body in smaller chunks, so the digest stays and the count grows.

## For the human at sign-off

- **The brief's two questions.** J (a definite flip error releases once, then stops with that error
  as its cause) is implemented as the brief describes. K is a stop, not an answer, with its
  cause text naming segmented publication; `Stop::SegmentedUnsupported` and `Admit::Flat` are both
  marked `deferred: #1009` for removal there.
- **Beyond the brief's design:** the session re-read after a lost flip; installing a segmented
  prior's `{generation: {segments}}` rather than refusing it; one publication stamp for both
  records. Each is argued above.
- **New public surface** for #670 to call: `complete_upload(store, upload_id, &CompleteRequest,
  &PublishRequest)`. `CompleteRequest` is unchanged, so #1007's callers and tests are untouched.
- **Branches no test reaches**, each mapped to a typed stop and one release rather than a panic:
  a committer refusal other than `VersionOverflow` (the `PublicationError` downcast at `:1251`;
  for a flat map the only ones left are value or batch ceilings, which are #1011's), a prior whose
  chunk scheme the coder does not support (`superseded` → `Stop::Record`), the epoch ceiling on
  `completed()`, and the two digests refusing a validated list. `VersionOverflow` itself is
  reached, by a prior at `u64::MAX` (`:3858`).
- **DST**: none added; the existing `deferred: #660` marker now names the flip's races
  (`complete.rs:613`).

## Not done here (by the brief's scope)

Segmented publication (#1009), recovery after an unknown outcome (#1010), the value ceiling
(#1011), the drain and `unlink`/`PutObject` routing (#659), the reaper (#625), the S3 verb (#670),
DST (#660), and any change to 0016, ADRs, `Cargo.lock` or `deny*.toml`.

## Gate result

`./engine/xtask.sh ci` (that is, `cargo xtask ci`: fmt, clippy, build, the workspace tests, the
`--cfg madsim` DST build and tests, the feature-matrix rows, deny and conformance), run on the
final tree with nothing edited during it: **`xtask ci: all checks passed`, exit 0.** Its
`multipart_complete` binary ran 59 tests, all passing: the 43 from #1007 and 16 new.

Patch size (`git diff --stat`): `multipart/complete.rs` +663, `multipart.rs` +280 (about 130 of
them the unit-test module), the test file +1763, the architecture doc 4 lines changed;
2611 insertions, 99 deletions in all.
