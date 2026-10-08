# Build notes — #810 restore-fence-generation (iteration 3)

Base: the per-cycle worktree at `0b48ab7` (`origin/main` + #839–#842 folded, the integration
branch `pdca-integration/r-a834e98f67b952c61db7a03084c626e8/main`). Every `path:line` below is on
that tree with this patch applied.

## What this iteration changed, against the carry-forward

The production code is **byte-identical to iteration 2's** (`crates/core/src/multipart.rs`,
`crates/custodian/src/restore.rs`, and the non-test part of `crates/server/src/cli.rs`). Every
finding in the carry-forward was a missing test, not a wrong behaviour, and each hand mutant the
reviewers built against the production code was a real survivor. So this iteration adds the tests
that kill them. It does not re-submit iteration 2's test set unchanged.

| Carry-forward item | What I did | Where |
|---|---|---|
| No test checks `RestoreReport::fence_generation` from a real pass (C5 survivor at `restore.rs:662`; hand mutant at `restore.rs:962`) | New leg G in `restore_open_fence.rs`: one pass that completes (an `Open` session fenced cleanly) and one that does not (leg H's unfenceable sessions). Each asserts `report.fence_generation == Some(decode_fence_generation(stored bytes))`, plus the generation number and `complete` flag, plus the audit summary's two generation fields. | `crates/custodian/tests/restore_open_fence.rs:855` (`stored_generation`), `:867` |
| Same, at the operator's end: the only CLI test built the report by hand | New CLI test that runs the one-shot's own backend dispatch `run_restore_reconcile_over_backend` (`cli.rs:1551`) over a **real redb store** twice: generation 1 complete, then generation 2 NOT complete (an `mpu:` key naming no upload). It reads `mpufence` back from redb, checks the report carries the same record, and checks the printed verdict says that generation and not its opposite. | `crates/server/src/cli.rs:3461` |
| No test checks the opening write's compare-and-set (hand mutants A: drop `require`/`require_absent`, C: treat the opening `Conflict` as acknowledged) | New `Fault::Race` in the base-compatible test file: the double lands another pass's record under `mpufence` just before this pass's commit, then judges the commit as usual. New test, two opening arms (over a restored `{3,true}` with the racer writing `{4,false}`; over no record with the racer writing `{1,false}`, i.e. the same bytes this pass would write) and one completing arm (a newer pass opens 2 just before this pass's completion lands). Opening arms assert `Err`, exactly one commit offered, the session still `Open`, no `retire:` and no `orphan:` key, and the record is the racer's. The completing arm asserts `Err` and the record reads `{2,false}`. | `crates/custodian/tests/restore_fence_generation.rs:75` (fault), `:223` (double), `:922` (test) |
| (minor) the K check was weakened to `sessions_unsettled` only | Now compares `(sessions_unsettled, segments_unaccounted)` of both passes. On the base those were the only two fields the old `unsettled_debug` slice covered (`sessions_unsettled` is followed only by `segments_unaccounted` in `RestoreReport` on the base), so this restores the old strength exactly. | `crates/custodian/tests/restore_open_fence.rs:765-768` |
| C5 survivors `Cut::text -> ""` / `"xyzzy"` (`restore.rs:1803`), noted low priority | P3 now asserts the summary says `the session fence did not finish` (`restore_open_fence.rs:822`). New leg G test: a newer pass opens 2 while this pass fences, so this pass's completion conflicts; asserts `Err`, record stays the newer pass's, and the surviving summary says `completion write did not land`, not the fence-cut text, with `fence_finished=true`, `fence_generation=1`, `fence_generation_complete=false`, `clean=false`, `needs_human=true`. | `crates/custodian/tests/restore_open_fence.rs:901` |
| C5 survivor `restore.rs:798` `>=` → `<` | Not killed, on purpose. See "Equivalent mutant" below. | — |
| T4 Contribution: #664 iteration 1's patch/review are not available to confirm the brief's prior-art claim | Not something Do can settle: it is about the brief's evidence, not the patch. Left for the human. The behaviour that claim is about (skipping every already-`Aborting` session) is still killed by legs N and N-crash (mutant M1 below). | — |
| Failing gate C4-diff-cov: "patch.diff does not apply on origin/main" | Not a patch defect. The patch is stacked on #839–#842 and applies cleanly to the integration branch at `0b48ab7` (checked below). C4-verify resolved that base and passed; C4-diff-cov resolved `origin/main` (`coverage/diff-cov.json`: `"base_ref": "origin/main"`). That base mismatch is in the harness, not the patch, so it likely belongs upstream. I cannot fix it from here. | — |
| Deferred human finding: the runbook (`m4-first-deployment-blueprint.md:653`, step 8 resumes **all** writers) and the CLI's NOT-complete line (`cli.rs:1474`, "keep multipart uploads off") tell the operator different things | Not changed. It is a policy choice (hold all writers vs. only multipart, given no per-verb switch exists before #508). The driver deferred it to sign-off. Whichever rule the human picks, it is a two-sentence edit to one of the two texts. | — |

Two tests moved out of the base-compatible file on purpose. `RestoreReport::fence_generation` and
`decode_fence_generation` are not base symbols. A test naming them in
`restore_fence_generation.rs` would not compile on the RED leg, and the gate would then report
UNVERIFIABLE. `run-verify.sh` reverts every modified file except the *added* test, so tests that
name the new API live in files the patch modifies (`restore_open_fence.rs`, `cli.rs`).
`restore_fence_generation.rs` still names only base symbols, and its module doc says where the
report's copy is checked (`:14-16`).

## What the patch does (unchanged from iteration 2)

1. **The record** (`crates/core/src/multipart.rs`). `MPUFENCE_KEY = b"mpufence"` (`:1141`) and
   `FenceGeneration { generation: NonZeroU64, complete: bool }` (`:1955`), with `next` (`:1964`)
   and `decode_fence_generation` (`:1997`). Wire shape `{"generation":N,"complete":B}`, closed
   (`deny_unknown_fields`) and canonical (`require_canonical`, the `mpuctl` model the brief cites).
   Generation 0 is outside `NonZeroU64`, so decode refuses it as `MalformedRecordValue`. No new
   `RecordError` variant: that enum is not `#[non_exhaustive]` on purpose, so a new variant could
   break a match elsewhere. 0016 names the generation (`:723-728`, X17b, `:3017-3021`) but gives
   it no key, so the key is the brief's fallback `mpufence`, 1-based. It does not overlap `mpu:`
   (4th byte `f`) or `mpuctl`.
2. **The pass** (`crates/custodian/src/restore.rs`).
   * `open_generation` (`:901`) is the FIRST thing `reconcile_after_restore` does (`:599`). It
     reads `mpufence` and decodes it (torn → `FenceGenerationFault::Unreadable`, `u64::MAX` →
     `Exhausted`, both `Err` before any write). Then it writes `{N+1,false}` conditioned on the
     bytes read (`require`, or `require_absent` when absent; `:913-916`). Only `Committed` lets the
     pass go on. `Conflict` → `ChangedUnderPass` `Err` (`:923`). Any `Err` (unknown outcome
     included, applied or not) → that `Err`.
   * `close_generation` (`:946`) runs last, only on the `Ok` path of the fence (`:878`). If
     `!report.sessions_settled()` it writes nothing and leaves the generation not complete.
     Otherwise it writes `{N+1,true}` requiring the exact `{N+1,false}` bytes it opened with.
     `Committed` → the report says complete (`:962`). `Conflict` → `Err` (`:965`). `Err` → that
     `Err`.
   * `RestoreReport::sessions_settled` (`:409`) = `sessions_unsettled.is_empty() &&
     segments_unaccounted.is_empty()`, the M-scope predicate. `needs_human` is unchanged.
   * `RestoreReport::fence_generation: Option<FenceGeneration>` (`:239`), set at `:662` and on
     close (`:962`).
   * `commit_marks` (`:987`): a mark batch answering `Conflict` is now an `Err`, not silently
     counted as marks. On the base, `restore.rs` did `ctx.meta.commit(marks).await?` and ignored
     the outcome. Q's rule needs this ("complete only after every earlier write was acknowledged
     committed"), and a `Conflict` is the store saying nothing was written.
   * Audit seam: `action=fence-generation` / `fence-generation-not-complete` /
     `fence-generation-write-failed` / `fence-generation-fault`. The summary line gains
     `fence_generation` and `fence_generation_complete`, and a `Cut` enum (`:1795`) so a failed
     completion write says so instead of reusing the "fence did not finish" text.
3. **Operator text** (`crates/server/src/cli.rs`). `fence_generation_line` (`:1460`), pushed as
   line 2 of the verdict (`:1314`). It does not contain "NEEDS-HUMAN" and does not change the
   exit status: a not-complete generation on a returned report always comes with a session
   finding that already sets it.
4. **Docs**: the key and shape in one place (`05-building-block-view.md:204`), including what it
   cannot tell; a paragraph in `06-runtime-view.md:67`; the m4 runbook's step 7
   (`m4-first-deployment-blueprint.md:647-654`).
5. **Existing tests adjusted** (iteration 2): `restore_completing_fence.rs:647` and
   `restore_open_fence.rs:750` compare store snapshots with `mpufence` removed (every pass now
   rewrites its own generation record, by design). `crates/core/tests/multipart_keys.rs` adds
   `mpufence` to the disjointness matrix. `crates/core/tests/multipart_budget_admission.rs:469`,
   `:497` test the codec round trip and its refusals.

## The fence itself is unchanged

The brief allows touching #840–#842's fence only if N, N-crash or O cannot pass otherwise. They
pass without it: `plan_fence`, `fence_session`, `recheck_fenced` and `check_attempt` are
byte-for-byte the base's. The `Aborting` arm already sends every already-fenced session through
`recheck_fenced`, which is #842's leg K.

## Leg P — which durable fact carries the residue, and why neither interruption loses it

**Choice: re-derive it from each session's own records, every pass. Nothing is stored on the
generation record.** The record holds only `{generation, complete}`.

* **H(ii)** (fenced, still needs a human): the durable facts are the `mpu:` record at
  `Aborting@4`, its `retire:records:s:<id>:3` obligation, the `seg:<nonce>:3:*` range and the
  `part:` range. Every pass reaches it through `plan_fence`'s `Aborting` arm → `recheck_fenced` →
  `check_attempt`, re-reads those ranges, and names it again in `segments_unaccounted` while a
  segment names a chunk no part holds. That blocks `sessions_settled()`.
* **H(i)** (no nonce, unfenced): the durable fact is the undecodable `mpu:` value itself. Every
  pass's `plan_fence` fails `decode_session_record` and names it `ValueUndecodable` in
  `sessions_unsettled`. That blocks `sessions_settled()`.

Why neither N-crash interruption can lose it:
* **(a) stop right after H(ii)'s durable `Completing → Aborting` fence.** The fence's single
  commit put the `Aborting@4` record and both obligations together; the `seg:` and `part:`
  records are untouched. Everything `recheck_fenced` reads is durable the moment the fence commit
  is acknowledged. Nothing the pass held in memory afterwards is needed: the next pass re-reads
  and re-names. The completion write cannot have happened, because it runs only after the fence
  loop returns `Ok`. If residue lived only on the generation record and were written at the
  pass's end, this stop would lose it, and a pass that skipped `Aborting` sessions would then
  complete. That is the brief's SELF-TEST, and mutant M1 shows the test catches it.
* **(b) stop right after the new not-complete record lands, before any fence.** The only write
  was `{N+1,false}`. No session record changed, so the next pass sees the same `mpu:`/`seg:`/
  `part:` state and names the same sessions. The record reads not complete, so no `complete` is
  observable in between.

## Q — ordering and unknown outcomes

* The not-complete write is the pass's first store write, before the staged read and before any
  mark (`restore.rs:599`). I ruled out opening it just before the fence, after the marks: Q(a) on
  a mark commit would then leave a restored `complete` in place while the pass returns `Err`.
  That mutant fails 4 of the tests (M5 below).
* An unknown outcome on the opening write is always `Err`. I did not add a re-read to settle a
  `may_still_commit = false` unknown. The brief only requires "no fence before it knows its write
  landed", and an `Err` there costs a re-run of an idempotent one-shot. A re-read path would add
  about 15 lines and one more branch to test.
* Both generation writes are conditioned on the bytes last read or written there. The opening
  one (`restore.rs:913-916`) is what stops two concurrent passes from both carrying on: the
  adversary's case was P1 and P2 both reading `{3,true}`, P1 opening `{4,false}`, and P2 (if it
  ignored its conflict) closing `{4,false}`→`{4,true}` while P1 still fenced. Now tested by
  `a_record_changed_under_the_pass_is_never_written_over` (mutants A and C below). The completing
  one is what makes Q(d) hold (mutant M4) and what makes a mid-pass newer generation end this
  pass in `Err` (mutant "close Conflict read as Ok").
* An unknown outcome on the completion write is the pass's `Err` (`AGENTS.md`: an unknown commit
  outcome is never a clean result). If it applied, the record reads complete, which is true:
  every earlier write was acknowledged. If not, the next pass completes its own generation.

## Equivalent mutant: `restore.rs:798` `>=` → `<`

That line is the base's mark-batching check (#551). It is in the diff only because the call
inside it changed from `ctx.meta.commit(..)` to `commit_marks(..)`. With `<`, every mark commits
on its own (batches of 1) instead of in batches of up to `MARK_BATCH` = 1000. Every mark still
lands, durably, before the generation can complete: `restore_reconcile.rs:635` checks all 1001
marks across a batch boundary, and Q(a) checks a failed mark commit leaves the generation not
complete. The only observable difference is the number of mark commits. Killing it would mean an
#810 test asserting how many mark commits a pass makes, which ties this slice's test to #551's
batch size. I did not add that. The reviewer of iteration 2 reached the same conclusion
("equivalent mutant ... Not a finding").

## What I chose not to do

* **No change to `needs_human` or `is_clean`.** A not-complete generation on a returned report
  implies `!sessions_settled()`, which implies `needs_human()`. Adding the generation to
  `needs_human` would add nothing in production and would change every hand-built report in the
  CLI tests.
* **No DST coverage.** The brief puts `crates/dst/tests/custodian.rs` out of scope. The new test
  file carries the existing `// deferred: #843` marker (the fence's own Tier-0 DST slice). The
  generation write is a conditioned write on one singleton, not a destructive path. The human
  should confirm at sign-off that #843's scope may cover it.
* **No change to the runbook/CLI wording conflict** (see the table above): deferred to sign-off.

## Red → green (leg R and the falsifiability rule)

Runs: quick red/green runs used `timeout 1500 cargo test -p wyrd-custodian --test
restore_fence_generation` in the worktree. That is the exact target `run-verify.sh` runs, under a
timeout. I did not run `run-verify.sh` itself: it needs a `pdca-verify` branch in the host repo,
and that branch is checked out at `/home/eddie/wyrd/wyrd-verify`, so a second worktree could not
take it, and making a new branch in the host repo is outside my roots. The RED leg below copies
what it does: every modified tracked file reverted to `0b48ab7`, the added test kept. The full
gate was `./engine/xtask.sh ci` (the project wrapper, `pdca.toml [gates] runner`), under
`timeout 5400`.

* **GREEN with the fix:** `restore_fence_generation` 12 passed, 0 failed. `restore_open_fence`
  11 passed (2 new). `restore_completing_fence` 7 passed. `wyrd-server --lib restore_verdict` 7
  passed (1 new).
* **RED without the fix** (`git checkout -- .` over every modified tracked file, the new test
  kept), re-run on the final formatted tree: **12 of 12 tests ran and FAILED, every one by
  assertion.** The file compiled on the base. The failure lines were
  `restore_fence_generation.rs:492, 527, 568, 628, 658, 741, 805, 832, 869, 896, 943, 988`. The
  new race test is red at `:943` (`assert!(outcome.is_err())`: the base offers no `mpufence`
  commit, so the race never fires and the pass returns `Ok`). The fix was then re-applied from
  the saved diff (`git diff` byte-identical to before the revert), and the same target went 12/12
  green again.

### Refute-your-own-test (forced)

* **(a) Genuine red?** Yes. With every production change reverted, all 12 tests in the brief's
  file ran and failed on assertions (lines above). With the fix, all 12 pass. The tests moved to
  modified files cannot have a base-side red (they name new API), so their binding is shown by
  mutants instead: each of the 7 hand mutants below fails at least one of them while the
  unmutated tree passes.
* **(b) Production path?** Yes. Every leg calls the production
  `wyrd_custodian::reconcile_after_restore` through a `GcContext` over in-memory doubles that only
  store bytes and inject commit faults. They hold no copy of the pass's logic. The new CLI test
  goes further: it calls the one-shot's production backend dispatch
  `run_restore_reconcile_over_backend` (`cli.rs:1551`) over a real redb file and the production
  `restore_verdict` (`cli.rs:1271`), and reads the record back from redb by key.
* **(c) Fixture includes the fault?** Yes. The new race fault really writes another pass's record
  between this pass's read and commit, including the hardest case: the racer writes exactly the
  bytes this pass would write (`{4,false}` over `{3,true}`, `{1,false}` over nothing), so only the
  precondition can tell them apart. The completing arm really lands a newer generation before the
  completion. Leg G's not-complete arm uses leg H's real unfenceable sessions (undecodable, `Open`
  and `Completing` at `u64::MAX`). The CLI test's not-complete arm uses a real unparseable `mpu:`
  key in redb. Iteration 2's fixtures stand: #842's real H(i) and H(ii), real N-crash stops,
  definite and unknown commit outcomes applied and not applied, a real late landing, and
  M-scope's real dangling chunk, undecodable `inode:`/`pending:`/`part:`, untrusted `part:`, and
  `mpu:` key naming no upload.

### Mutants run against the tests (each restored afterwards; full `wyrd-custodian` suite)

Iteration 3, all killed:

| Mutant | Killed by |
|---|---|
| A — opening write without `require`/`require_absent` (`restore.rs:914-915`) | `a_record_changed_under_the_pass_is_never_written_over` |
| C — opening `Ok(Conflict)` read as acknowledged (`restore.rs:923`) | `a_record_changed_under_the_pass_is_never_written_over` |
| C5 survivor — delete `fence_generation: Some(generation)` (`restore.rs:662`) | `the_report_carries_the_generation_the_pass_wrote`, `a_completion_that_does_not_land_says_so_in_the_summary`; and in `wyrd-server`, `restore_verdict_prints_the_generation_the_pass_left_in_the_store` |
| hand mutant — delete `report.fence_generation = Some(closed);` (`restore.rs:962`) | `the_report_carries_the_generation_the_pass_wrote`; and in `wyrd-server`, `restore_verdict_prints_the_generation_the_pass_left_in_the_store` |
| completing `Ok(Conflict)` read as `Ok(())` (`restore.rs:965`) | `a_record_changed_under_the_pass_is_never_written_over` |
| C5 survivor — `Cut::text` → `""` | `a_fence_fault_never_hides_the_pass_verdicts`, `a_completion_that_does_not_land_says_so_in_the_summary` |
| C5 survivor — `Cut::text` → `"xyzzy"` | same two |

Iteration 2 (production unchanged since, so these still hold):

| Mutant | Result |
|---|---|
| M1 — skip every already-`Aborting` session (#664 iteration 1, `plan_fence` → `Settled`) | 2 fail: N/O and N-crash; leg I passes, exactly the brief's SELF-TEST |
| M2 — complete on `!needs_human()` instead of `sessions_settled()` | 1 fails: M-scope |
| M4 — completion write without its precondition | 1 fails: Q(d) |
| M5 — open the generation after the marks, just before the fence | 4 fail: I(iv), Q(a), Q(b), torn-record |
| M6 — carry on past a failed opening write | 1 fails: Q(b) |

## External dependencies

`typos` and the docs renderer (`markdown_it`, `yaml` importable) are both present on this host,
so the local `cargo xtask ci` ran the prose gates rather than skipping them. No other dependency
was needed. The new CLI test uses redb and `tempfile`, both already dependencies of `wyrd-server`
(no `Cargo.toml` change).

## For the human after acceptance (from the brief's ordering note, not Do's to do)

Add `Conflicts with: 663, 804` to the brief and re-point #508's `Depends on` from 664 to 810.

## Full gate

`./engine/xtask.sh ci` (the project wrapper; `cargo xtask ci` in this worktree), under `timeout
5400`, on the final tree after `cargo fmt --all`: **exit 0, "xtask ci: all checks passed"**.
Steps it ran: `typos`, `lint_docs.py` (OK), `render_site.py --check` (link audit OK), the
gitlink, unsafe and blackbox guards, `cargo fmt --check`, `cargo clippy --workspace
--all-targets`, build, `cargo test --workspace` (200 `test result: ok` lines, none failed),
`cargo-machete`, `cargo deny` (three runs), conformance vectors, statics, deploy-guard, then
clippy and tests of `wyrd-dst` under `--cfg madsim`. That includes
`crates/dst/tests/custodian.rs`, untouched and still green.

Formatting: `cargo fmt --all -- --check` is clean. It reformatted only this iteration's new test
code. `patch.diff` is `git diff` with the new file added intent-to-add. It applies cleanly to
`0b48ab7` (`git apply --cached --check` against a scratch index read from that commit). Its
production and doc hunks are byte-identical to iteration 2's (per-file hunk comparison). Only
`restore_fence_generation.rs`, `restore_open_fence.rs` and the test module of `cli.rs` differ.
