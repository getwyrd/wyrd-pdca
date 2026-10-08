# Build notes — #810 restore-fence-generation

Base: the per-cycle worktree at `0b48ab7` (`origin/main` + #839–#842 folded). Every `path:line`
below is on that tree with this patch applied.

## What the patch does

1. **The record** (`crates/core/src/multipart.rs`). `MPUFENCE_KEY = b"mpufence"` (`:1141`) and
   `FenceGeneration { generation: NonZeroU64, complete: bool }` (`:1955`), with `next`
   (`:1964`) and `decode_fence_generation` (`:1997`). Wire shape `{"generation":N,"complete":B}`,
   closed (`deny_unknown_fields`) and canonical (`require_canonical`, the `mpuctl` model the brief
   cites). Generation 0 is outside `NonZeroU64`, so it is refused at decode as
   `MalformedRecordValue` — no new `RecordError` variant (that enum is not `#[non_exhaustive]`,
   `multipart.rs:4161` says so on purpose, so a new variant could break a match elsewhere).
   0016 names the generation (`:723-728`, `:2546` X17b, `:3017-3021`) but gives no key, so the key
   is the brief's fallback `mpufence`, 1-based. Disjoint from `mpu:` (4th byte `f`) and `mpuctl`.
   Module docs: the keyed-class table and the writer-side API sentence.
2. **The pass** (`crates/custodian/src/restore.rs`).
   * `open_generation` (`:901`) is the FIRST thing `reconcile_after_restore` does (`:599`): read
     `mpufence`, decode (torn → `FenceGenerationFault::Unreadable`, `u64::MAX` → `Exhausted`, both
     `Err` before any write), CAS `{N+1,false}` over the bytes read (`require`, or `require_absent`
     when absent). Only `Committed` lets the pass go on. `Conflict` → `ChangedUnderPass` `Err`. Any
     `Err` (unknown outcome included, applied or not) → that `Err`.
   * `close_generation` (`:946`) runs last, only on the `Ok` path of the fence (`:878`). If
     `!report.sessions_settled()` it writes nothing and leaves the generation not complete. Else it
     CAS-writes `{N+1,true}` requiring the exact `{N+1,false}` bytes it opened with. `Committed` →
     report says complete; `Conflict` → `Err`; `Err` → that `Err`.
   * `RestoreReport::sessions_settled` (`:409`) = `sessions_unsettled.is_empty() &&
     segments_unaccounted.is_empty()` — the M-scope predicate. `needs_human` is unchanged.
   * `RestoreReport::fence_generation: Option<FenceGeneration>` (`:239`), set at `:662` and on close.
   * `commit_marks` (`:987`): a mark batch answering `Conflict` is now an `Err`, not silently
     counted as marks. Before this, `restore.rs` did `ctx.meta.commit(marks).await?` and ignored the
     outcome. Needed for Q's rule "complete only after every earlier write was acknowledged
     committed": a `Conflict` is the store saying nothing was written.
   * Audit seam: `action=fence-generation` / `fence-generation-not-complete` /
     `fence-generation-write-failed` / `fence-generation-fault` (`:1742`–`:1784`); the summary line
     gains `fence_generation` and `fence_generation_complete`, and a `Cut` enum (`:1795`) so a failed
     completion write says so instead of reusing the "fence did not finish" text.
3. **Operator text** (`crates/server/src/cli.rs`). `fence_generation_line` (`:1460`), pushed as
   line 2 of the verdict (`:1314`): "restore-fence generation N COMPLETE — … run this pass BEFORE
   re-enabling any gateway" or "… N NOT complete — … keep multipart off, repair, re-run". It does
   not contain "NEEDS-HUMAN" and does not touch the exit status: a not-complete generation on a
   returned report always comes with a session finding that already sets it. New unit test
   `restore_verdict_says_whether_the_fence_generation_completed` (`:3408`).
4. **Docs** — the key and shape in one place (`05-building-block-view.md:204`), including what it
   cannot tell; a paragraph in `06-runtime-view.md:67`; the m4 runbook's step 7
   (`m4-first-deployment-blueprint.md:647-654`): re-enable no gateway (step 8) until a run says
   COMPLETE, because step 5 brought back whatever record the backup held.
5. **Existing tests adjusted** (both are #841/#842 "a second pass is idempotent" legs, which
   asserted the second pass writes nothing at all — now every pass rewrites its own generation
   record, by design):
   * `restore_completing_fence.rs:647` and `restore_open_fence.rs:750`: compare the store
     snapshots with `mpufence` removed. Everything else in those legs is unchanged.
   * `restore_open_fence.rs:763`: `unsettled_debug(&first) == unsettled_debug(&second)` sliced the
     report's `Debug` from `sessions_unsettled:` to the END, which now includes the per-pass
     generation number. Replaced with a direct field comparison
     `first.sessions_unsettled == second.sessions_unsettled` — what the leg means.
   * `crates/core/tests/multipart_keys.rs:629-634`, `:726`: `mpufence` joins the disjointness matrix
     and the per-session-range "foreign key" list.
   * `crates/core/tests/multipart_budget_admission.rs:469`, `:497`: codec round trip
     (decode→encode byte-identical, counts from 1, `next` at `u64::MAX` is `None`) and refusal of
     generation 0 / unknown field / missing field / wrong type / not JSON / noncanonical spellings.
     Put in the existing `mpuctl` codec file, not a new file, so the C4-verify gate still sees one
     added `*/tests/*.rs` (the brief's named test) and its red leg is not confused by a second new
     file that cannot compile on the base.

## The fence itself is unchanged

The brief allows touching #840–#842's fence only if N, N-crash or O cannot pass otherwise. They
pass without it: `plan_fence` (`restore.rs:1100`), `fence_session` (`:1015`), `recheck_fenced`
(`:1149`) and `check_attempt` (`:1185`) are byte-for-byte the base's. The `Aborting` arm
(`:1121`) already sends every already-fenced session through `recheck_fenced`, which is #842's
leg K. The only non-generation change on the pass's write path is `commit_marks` (above), which is
the mark half, not the fence.

## Leg P — which durable fact carries the residue, and why neither interruption loses it

**Choice: re-derivation from the session's own records, every pass. Nothing on the generation
record.** The generation record holds only `{generation, complete}`.

What carries each case:
* **H(ii)** (fenced, still needs a human): the durable facts are the `mpu:` record at `Aborting@4`,
  its `retire:records:s:<id>:3` obligation, the `seg:<nonce>:3:*` range and the `part:` range.
  Every pass reaches it through `plan_fence`'s `Aborting` arm → `recheck_fenced` →
  `check_attempt`, re-reads those ranges, and names it again in `segments_unaccounted` while a
  segment names a chunk no part holds. That blocks `sessions_settled()`.
* **H(i)** (no nonce, unfenced): the durable fact is the undecodable `mpu:` value itself. Every
  pass's `plan_fence` fails `decode_session_record` and names it `ValueUndecodable` in
  `sessions_unsettled` (`restore.rs:1104`). That blocks `sessions_settled()`.

Why neither N-crash interruption can lose it:
* **(a) stop right after H(ii)'s durable `Completing → Aborting` fence.** The fence's single
  commit put the `Aborting@4` record and both obligations together; the `seg:` and `part:` records
  are untouched. So everything `recheck_fenced` reads is already durable the instant the fence
  commit is acknowledged, and nothing the pass held in memory afterwards (its `report`) is needed:
  the next pass re-reads and re-names. The completion write cannot have happened, because it runs
  only after the fence loop returns `Ok`. If residue lived only in the generation record and were
  written at the pass's end, the stop would lose it, and a pass that skipped `Aborting` sessions
  would then complete — the brief's SELF-TEST, and what the test catches (see mutants below).
* **(b) stop right after the new not-complete record lands, before any fence.** The only write
  was `{N+1,false}`. No session record changed, so the next pass sees the same `mpu:`/`seg:`/
  `part:` state and names the same sessions. The record reads not complete, so no `complete` is
  observable in between.

## Q — ordering and unknown outcomes

* The not-complete write is the pass's first store write, before the staged read and before any
  mark (`restore.rs:599`). I first considered opening it just before the fence, after the marks. I
  ruled that out: Q(a) on a mark commit would then leave a restored `complete` in place while the
  pass returns `Err`, and I(iv) says "complete is observable only after every write the pass
  makes, the mark batches included". The test shows the cost: that mutant fails 4 of the 11 tests
  (mutant M5 below).
* An unknown outcome on the opening write is always `Err`. I did not add a re-read to settle a
  `may_still_commit = false` unknown: the brief only requires "no fence before it knows its write
  landed", and an `Err` there costs a re-run of an idempotent one-shot. A re-read path would add
  about 15 lines and one more branch to test.
* The completion write's precondition is on the exact `{N+1,false}` bytes. That is what makes Q(d)
  hold: a late landing finds the newer pass's `{N+2,false}` and writes nothing. Without it, the
  test fails (mutant M4).
* An unknown outcome on the completion write is the pass's `Err` (`AGENTS.md`: an unknown commit
  outcome is never a clean result). If it applied, the record reads complete, which is true:
  every earlier write was acknowledged. If not, the next pass completes its own generation.

## What I chose not to do

* **No change to `needs_human` or `is_clean`.** A not-complete generation on a returned report
  implies `!sessions_settled()`, which implies `needs_human()`. Adding the generation to
  `needs_human` would add nothing in production. It would also change every hand-built report in
  the CLI tests.
* **No test for the opening write's `Conflict`** (record changed between read and commit). The
  double has no hook between a `get` and a `commit`. Adding one is about 10 lines; the code path is
  4 lines that map `Conflict` to a typed `Err`. Flagging it here instead.
* **No DST coverage.** The brief puts `crates/dst/tests/custodian.rs` out of scope. The new test
  file carries the existing `// deferred: #843` marker (the fence's own Tier-0 DST slice). The
  generation write is a CAS on one singleton, not a destructive path. The human should confirm at
  sign-off that #843's scope may cover it.

## Red → green (leg R and the falsifiability rule)

Runner: quick red/green runs used `timeout 900 cargo test -p wyrd-custodian --test
restore_fence_generation` in the worktree. That is a narrowed form of the gate's own test step,
with a timeout. The full gate was `./engine/xtask.sh ci` (the project wrapper,
`pdca.toml [gates] runner`), run under `timeout 5400` (result at the end of this file).

* **GREEN with the fix:** 11 passed, 0 failed.
* **RED without the fix** (production `multipart.rs` and `restore.rs` reverted, test kept):
  **11 of 11 tests ran and FAILED, every one by assertion.** The file compiles on the base: it
  names the record only by raw key `b"mpufence"` and uses only symbols the base has. The
  "not complete" arms (M, N, N-crash, Q) each sit beside an exact-bytes arm in the same test that
  is red on the base. Examples: `record() == Some({"generation":1,"complete":false})`, and
  Q(a)/Q(b) seed a restored `{"generation":3,"complete":true}`, so "reads not complete" is false
  on the base.

### Refute-your-own-test (forced)

* **(a) Genuine red?** Yes. With the production change reverted, all 11 tests ran and failed on
  assertions (failure lines `restore_fence_generation.rs:469, 500, 531, 587, 613, 674, 738, 765,
  799, 826, 857` in the red run). With the fix, all 11 pass.
* **(b) Production path?** Yes. Every leg calls the production
  `wyrd_custodian::reconcile_after_restore` through a `GcContext` over an in-memory
  `MetadataStore`/`ChunkStore`. The doubles only store bytes and inject commit faults. They hold
  no copy of the pass's logic. The record is read back by raw key, and its bytes are compared to
  the codec's own spelling.
* **(c) Fixture includes the fault?** Yes. M/N/O seed #842's real H(i) (nonce stripped, fails
  `decode_session_record`, asserted in the seed) and H(ii) (segment naming chunk `0xD2F` that no
  part holds). N-crash injects real stops: the double refuses every commit after the first one
  putting H(ii)'s `mpu:` key, then every commit after the first `mpufence` write, and the test
  asserts the stopped writes did not land. Q injects definite errors and `CommitUnknownResult {
  may_still_commit: true }`, applied and not applied. Q(d) holds the batch in flight and lands it
  after the next pass's opening write, judging its precondition then. M-scope includes a real
  dangling committed chunk (no fragment anywhere), an undecodable `inode:`, an undecodable
  `pending:`, an undecodable `part:`, an untrusted `part:` (placement of the wrong length), and an
  `mpu:` key naming no upload.

### Mutants run against the test (each restored afterwards)

| Mutant | Result |
|---|---|
| M1 — skip every already-`Aborting` session (#664 iteration 1, `plan_fence` → `Settled`) | 2 fail: N/O and N-crash; leg I passes — exactly the brief's SELF-TEST |
| M2 — complete on `!needs_human()` instead of `sessions_settled()` | 1 fails: M-scope |
| M4 — completion write without its precondition | 1 fails: Q(d) |
| M5 — open the generation after the marks, just before the fence | 4 fail: I(iv), Q(a), Q(b), torn-record |
| M6 — carry on past a failed opening write | 1 fails: Q(b) |

## External dependencies

`typos` (typos-cli 1.48.0) and the docs renderer (`markdown_it`, `yaml` importable) are both
present on this host, so the local `cargo xtask ci` ran the prose gates rather than skipping them.
No other dependency was needed.

## For the human after acceptance (from the brief's ordering note, not Do's to do)

Add `Conflicts with: 663, 804` to the brief and re-point #508's `Depends on` from 664 to 810.

## Full gate

`./engine/xtask.sh ci` (the project wrapper; `cargo xtask ci` in this worktree), under `timeout
5400`: **exit 0, "xtask ci: all checks passed"**. Steps it ran: `typos`, `lint_docs.py`,
`render_site.py --check` (link audit OK), `cargo fmt --check`, `cargo clippy --workspace
--all-targets`, build, `cargo test --workspace`, `cargo-machete`, `cargo deny` (three runs),
then clippy and test of `wyrd-dst` under `--cfg madsim`. No test failed anywhere. That includes
`crates/dst/tests/custodian.rs`, untouched and still green with the extra opening commit.

While the gate ran I added two doc-comment sentences to `restore.rs` (`open_generation` /
`close_generation`: "awaits are bounded by the `MetadataStore` implementation (#508/#636)"). After
it finished I re-ran `cargo fmt --all -- --check` (clean) and `cargo clippy -p wyrd-custodian -p
wyrd-core -p wyrd-server --all-targets -- -D warnings` (clean) on the final tree. `patch.diff` is
`git diff` plus the new file, and it applies cleanly to `0b48ab7` (`git apply --cached --check`
against a scratch index).
