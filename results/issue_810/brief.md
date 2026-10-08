# Brief — restore-fence-generation

> Child 3 of 3 of #664's split (itself 637.4). Do reads ONLY this file. Keep the
> `- **Label:** value` lines. `path:line` citations are on `origin/main` @ `243241e`
> (re-verified 2026-09-29). This bundle's base is `origin/main` **plus the accepted patches of
> #809's split children #839–#842** (the session fence in both shapes, the nonce,
> `sessions_fenced` / `sessions_unsettled` / `staged_skipped`, and #842's cases H(i)–H(vi)):
> locate those by symbol on the base. "#842 H(ii)" below means #842's leg of that name. Background: 0016 decision 1.4 / D-B
> (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:717-728`, `:723-728`), the
> deployment ordering (`:3017-3021`), X17b.

- **Slug:** restore-fence-generation
- **Kind:** enhancement
- **Defect:** no durable record tells a gateway the restore fence has run, and residue does not
  survive a re-fence. 0016 requires the restore-fence generation to complete before any gateway
  serves multipart verbs on the restored image (`0016:723-728`, `:3017-3021`, X17b). Iteration
  1's version was unsound: a session fenced to `Aborting` with unrepaired residue was skipped
  on every later pass, so the documented repair-and-rerun remedy cleared the residue finding
  and certified the generation complete with the obligations still unmet. A gateway trusting
  that marker would resume multipart verbs over an unfenced image.
- **Success criterion:** the NEW file `crates/custodian/tests/restore_fence_generation.rs`
  passes over in-memory doubles. The generation record is read by **raw key**, so the test
  compiles on the base. Seeded session records carry `segment_nonce` in #840's spelling
  (immediately after `clock_source`, every state), except the #842 H(i) record, which by
  definition lacks it.
  *What the record can and cannot claim.* It records the post-restore pass's own progress, in the
  store that pass fences. A restore rewinds the **whole** database
  (`docs/design/architecture/m4-first-deployment-blueprint.md:563-566`), this record included, so
  an image captured after an earlier pass completed comes back reading `complete` until the new
  pass starts. The record alone therefore cannot tell this restore's completion from one the
  image carried. In this slice X17b is closed by the deployment ordering 0016 allows
  (`0016:3017-3021`), which the m4 runbook already follows: writers stay stopped from step 7, the
  pass, until step 8 (`m4-first-deployment-blueprint.md:581-626`). The docs written for #508
  must say so (Scope). Legs:
  **(I) The generation record, on durable state.** (i) On a store no post-restore pass has
  touched, the record is absent. (ii) **During** a pass, read through a double hook at the first
  fence commit, it names the pass's generation and reads not-complete. (iii) After the pass it
  reads complete for that generation. (iv) *A restored old marker.* Seed the record as
  `complete` for generation N, as an image captured after pass N would carry it, beside a
  resurrected `Open` session. At the new pass's first fence commit the record already reads N+1
  not-complete, and after the pass N+1 complete: once a pass has started, no fence it owes runs
  under a `complete` reading. "Complete" becomes observable only after every write the pass
  makes, the mark batches included.
  **(M) A session finding that needs a human blocks completion.** For each of #842's cases that
  needs a human — a `Completing` record with no nonce (#842 H(i): left unfenced, named) and a
  `Completing` session whose `seg:` records name a chunk no `part:` record holds (#842 H(ii):
  fenced, still named) — the pass ends with `needs_human()` true and the generation **not**
  complete.
  **(M-scope) Only session findings block completion.** Every session in the store fenced
  cleanly, plus one unrelated dangling committed chunk (fewer than k fragments anywhere,
  `restore.rs:569-572`): the pass ends with `needs_human()` true **and** the generation
  complete. Blocks completion: a session the pass could not read or could not fence (an `mpu:`
  record whose key or value will not read; child-3's unsettled sessions), and a fenced session
  still named as needing a human (#842's H cases). Does not block it: findings about committed
  objects (dangling, misplaced, an unreadable `inode:` record), the pending ledger, or `part:` /
  `sidx:` records (#839's untrusted staged records included). None of those lets a session
  publish, and withholding multipart over a lost file is an availability policy 0016 does not
  state.
  **(N) Residue survives a re-fence.** After M, run the pass again in a fresh context with
  **nothing repaired**. The #842 H(ii) session is already `Aborting`; the H(i) one still does
  not decode. The second pass names both, and the new generation is **not** complete. This is
  the regression test for #664 iteration 1's `restore.rs:916` (it skipped every already-`Aborting`
  session); it must fail against that logic.
  **(N-crash) Residue survives an interrupted pass.** Two interruptions, each followed by a
  fresh-context pass with nothing repaired: (a) stop the pass immediately after the durable
  `Completing → Aborting` fence of the #842 H(ii) session (the double fails every commit after
  that one); (b) stop a later pass immediately after it writes its new generation's not-complete
  record, before any fence. After each, the next pass names the H(ii) session and does not read
  complete; after O's repair, a pass does. This constrains crash behaviour without choosing where
  residue lives (leg P).
  **(O) Repair, then a pass, and only then complete.** After M, apply the two repairs: (a) put
  the missing `part:` record back for the H(ii) session: the next pass names nothing for it and
  writes no second obligation (its fence obligations stay byte-identical); (b) replace the H(i)
  record with a decodable one carrying its nonce, standing in for the operator's repair: the next
  pass fences it, and its obligations decode through `decode_retire_obligation`. **Only** when
  both are repaired does the generation read complete. The counter-arm: repairing one of the two
  leaves it not complete.
  **(P) The judgement comes from durable state.** Every later pass above runs in a fresh context
  that shares nothing in memory with the one before. State in `build-notes.md` which durable fact
  carries the residue — re-deriving it from each `Aborting` session's records each pass (#842's
  leg K already does), or a field on the generation record — and why neither N-crash
  interruption can lose it.
  **(Q) "Complete" is never written over unfinished or unknown work.** The rule: the pass writes
  complete only after every earlier write it made was acknowledged committed, and runs no fence
  before it knows its not-complete write landed. The store may report an error for a write that
  landed, or land it after reporting (`crates/traits/src/lib.rs:204-247`, `:1301-1312`), so each
  boundary gets its own leg:
  (a) *mid-pass:* a fence or mark commit fails, once as a definite error and once as an unknown
  outcome. The pass returns `Err` and the generation reads not-complete.
  (b) *the initial invalidation:* its commit answers an unknown outcome a re-read cannot settle
  (`may_still_commit`, `lib.rs:240-247`), once applied and once not. The pass returns `Err` with
  every session untouched. Where it did not apply, the record reads what it read before (a
  restored `complete`, as in I(iv): the pre-pass state this slice documents rather than closes).
  (c) *the final completion write:* its commit answers an unknown outcome, once applied and once
  not. Where it applied the record reads complete, which is true because every earlier write was
  acknowledged. Where it did not, the record reads not-complete and the next pass completes.
  (d) *a late landing:* generation N's completion write answers unknown with `may_still_commit`,
  a new pass writes N+1 not-complete, and then the N write lands (the double applies it late,
  judging its preconditions when it lands). The record still reads N+1 not-complete: a stale
  completion never masks a newer pass.
  **(R) `cargo xtask ci` green.**
- **Falsifiability:** RED is produced in-process on the base (`origin/main` + #839–#842) by
  **assertion**: that base writes no generation record, so I(ii), I(iii), I(iv), M-scope's
  "complete" arm, O's final arm and Q(c)'s applied arm fail there. The "not complete" arms of M,
  N, N-crash and Q are vacuously true on the base and earn their keep against mutants, so pair
  each with a positive arm in the same test. The test may name only symbols visible on its base.
  A compile failure on the RED leg reports UNVERIFIABLE (`engine/scripts/run-verify.sh`). Record
  in `build-notes.md` how many tests ran red.
- **Invariant to restore:** a restored image is declared fenced only when every session it
  resurrected can no longer publish or is named as needing a human, every record a fenced session
  wrote has a named deleter, and no later pass can withdraw that judgement by forgetting what an
  earlier one could not repair. The declaration never reads complete over unfinished or unknown
  work, and never masks a newer pass. It is a claim about the pass that wrote it, not proof that
  the current restore's pass ran: until #508 adds a restore-scoped signal, that rests on the
  deployment ordering. Source: 0016 D-B and decision 1.4 (`:717-728`), `:3017-3021`; the C-1
  rule that a certification over an incomplete picture is a defect (`docs/principles.md` §5);
  `AGENTS.md:178-180` (an unknown commit outcome is never a clean result); ADR-0045. SELF-TEST: a
  pass that skips every already-`Aborting` session passes leg I and fails leg N; a completion
  predicate of plain `needs_human()` fails M-scope; residue kept only in the generation record
  and written at the pass's end fails N-crash (a) unless re-derived.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Repro instruction:** on the base, run `reconcile_after_restore` over a store with one `Open`
  session, then scan the metadata store for any fence-generation key: there is none, before or
  after.
- **Scope:** one small durable record naming the post-restore pass's generation and whether it
  finished, written not-complete before the first fence (known to have landed before any fence
  runs), and complete only after the last write, only when every earlier write was acknowledged,
  and only when no session finding needs a human (M-scope); re-evaluation of already-fenced
  sessions on every pass; the record's key and shape documented for #508 in one place
  (`05-building-block-view.md:202`, plus a paragraph in `06-runtime-view.md` and the m4
  blueprint's restore steps), **including what it cannot tell**: a `complete` restored with the
  image reads the same as this restore's until the new pass starts, so #508's gate needs a
  restore-scoped signal beside it, and until #508 the guarantee is the runbook's ordering (step 7
  before step 8); the operator text in `crates/server/src/cli.rs` saying whether the generation
  completed. Key name: use the one 0016 gives; if it gives none, `mpufence`, 1-based so that
  absent is the only spelling of "no pass has run". / out of scope: the fence itself and the
  nonce (#840–#842 — change them only if leg N, N-crash or O cannot pass otherwise, and say so in
  `build-notes.md`); a restore-scoped signal for the gateway (#508's design); the gateway's reading of the record (#508); drain status (child-1);
  `scrub.rs`, `reconstruction.rs` (#663); the mark codec (#804); the retire drain (#659);
  `crates/dst/tests/custodian.rs` unless an existing case stops passing; any edit to 0016 or an
  ADR.
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/custodian/tests/restore_fence_generation.rs` — a **NEW** file; the
  C4-verify gate earns its red only from an added `*/tests/*.rs`. No `Cargo.toml` change.
- **Difficulty:** high
- **Depends on:** 842
- **Conflicts with:** 808, 813, 814, 804
- **Ordering note:** wave 3 of #664's split. Builds on child-2's fence. **Re-pointed
  2026-09-29:** #809 (child-2) was split at its re-plan into #839–#843, and the fence it built
  on now lands in #841 (`Open`) and #842 (`Completing`); `Depends on` names #842, which depends
  on #840 and #841, which depend on #839. A dependency on the split parent #809 would read as met
  with no fence on the base. Conflicts with child-1
  only over the shared paragraph at `06-runtime-view.md:78`. After acceptance add
  `Conflicts with: 663, 804` (same doc), and re-point #508's `Depends on` from 664 to this id.
  **Settled at Plan, 2026-09-18:** leg P deliberately leaves *where* residue lives to Do. The
  brief binds the property — the judgement is re-derivable from durable state after a crash
  between passes — and naming the store for it here would seat the fix shape, which a brief must
  not do (`docs/principles.md` §3.1). Do picks, and records the choice and its crash argument in
  `build-notes.md`; leg P is what tests it either way. **Re-pointed 2026-09-19:** #663 was split at its re-plan into #813 (scrub checks committed staged fragments; reconstruction keeps their repair queued) and #814 (reconstruction rebuilds a staged chunk); the field above names both in place of `663`.
- **Surfaces:** data
- **Do model:** opus-max
- **Production reach:** the pass under test is the production `reconcile_after_restore`. The
  generation record has **no reader** until #508's gateway gate, and even then it cannot, alone,
  tell a `complete` restored with the image from this restore's (see the success criterion).
  Until #508 the guarantee rests on the deployment ordering 0016 also allows: run the
  post-restore pass before re-enabling gateways (`0016:3017-3021`), as the m4 runbook's step 7
  before step 8 already does (`m4-first-deployment-blueprint.md:581-626`). Declared so sign-off
  weighs it rather than discovering it.
- **Citations expected:** Do must cite `path:line` on the target branch for every change. Peer
  callsites Do MAY open and mirror:
  * `crates/custodian/src/restore.rs:312` (`reconcile_after_restore`), `:111-217`
    (`MARK_BATCH`, `RestoreReport`, `needs_human` `:212`), `:569-572` (a dangling chunk), and
    #841's and #842's fence, by symbol.
  * `crates/core/src/multipart.rs:2250` (`decode_session_record`), `:3455`
    (`decode_retire_obligation`); the `mpuctl` singleton's codec as the model for a
    one-record singleton.
  * `crates/traits/src/lib.rs:204-247` (`CommitUnknownResult`, `may_still_commit`).
  * `crates/server/src/cli.rs:1256-1369` (`restore_verdict`), tests from `:2898`.
- **Prior-art check (triage cycles):** re-run 2026-09-29 on `243241e`: no merged change adds a
  fence-generation record (no `mpufence` or fence-generation symbol in `crates/`); since
  `f41e9c5` the only change to `restore.rs` / `cli.rs` is `f683dbe` (#813, staged scrub and
  reconstruction), which adds none; no open PR touches these paths. Rejected prior art: #664 iteration 1
  (`results/issue_664/iteration-v1/patch.diff`, `review-batch.md`) — three blocking findings,
  all the same defect at its `restore.rs:916`. Its generation arms (leg I) drew no finding and
  MAY be mirrored; its skip of `Aborting` sessions MUST NOT.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR MAY
happen during the cycle. The PR MUST NOT be marked ready before sign-off accepts.

Plan-review response (2026-09-29): all four findings revised in place. (1) The claim that a
later restore invalidates an earlier completion is gone: a whole-database restore brings an old
`complete` back, so the brief now says the record cannot alone identify this restore, keeps the
runbook's step-7-before-step-8 ordering as the guarantee, has the #508 docs say so, and adds
I(iv) (a restored old marker is overwritten before the first fence). (2) Q is split by boundary:
mid-pass, the initial invalidation, the final write, and a late landing, including
applied-but-unacknowledged commits. (3) New N-crash interrupts right after a durable fence and
right after a generation rollover, then retries in a fresh context. (4) New M-scope narrows
completion blockers to session findings and tests an unrelated dangling file; this resolves the
brief's own invariant-versus-scope conflict toward the sourced invariant, and the human may
reverse it at sign-off. Also fixed while verifying: M/N/O now name #842's cases (O no longer
expects a new obligation for the missing-part case, which #842 fences on the first pass), the
base is `#839–#842`, and citations are re-checked on `243241e`.

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 1): rebuilding for the implementation-level findings — T4 Contribution — Confirm the recorded rejection/disposition of #664 iteration 1—merged history and closed PRs were checked by every affected path, but its named local patch/review are absent and GitHub #664 supplies no review comments, so that part of prior art cannot be independently settled (`brief.md:182`, `reviewer-prior-art.log:13`, `reviewer-prior-art-detail.log:168`).; T5 Judgment — Cover the incomplete generation returned by the real pass—the surviving field-deletion mutant can suppress the required CLI status while all tests remain green; current CLI tests supply the field themselves (`gate-logs/C5-mutants.log:13`, `crates/custodian/src/restore.rs:662`, `crates/server/src/cli.rs:1313`, `crates/server/src/cli.rs:3417`).; **No test checks `RestoreReport::fence_generation` from a real pass, and that field is the only thing the CLI reads.** Hand mutant: delete `report.fence_generation = Some(closed);` at `crates/custodian/src/restore.rs:962`. The full `wyrd-custodian` test suite still passes. Concrete failing case: on a clean store the pass writes `{"generation":1,"complete":true}` to the store, but the operator sees `restore-fence generation 1 NOT complete — an upload session needs a human (named below). Keep multipart uploads off this store`, and nothing is named below it (`crates/server/src/cli.rs:1313`, `:1460-1478`). The other arm is C5's surviving mutant at `restore.rs:662` (delete `fence_generation: Some(generation)`). With that mutant, a run left NOT complete prints no generation line at all. The only CLI test (`crates/server/src/cli.rs:3408`) builds the report by hand. Fix: in a test file that may name base symbols (`restore_open_fence.rs` / `restore_completing_fence.rs` already import `MPUFENCE_KEY`), assert that `report.fence_generation` equals the stored record decoded, on one complete pass and one not-complete pass.; **No test checks the opening write's compare-and-set (a write conditioned on the exact bytes read).** Two hand mutants both survive the full `wyrd-custodian` suite. (A) Drop the `require`/`require_absent` at `crates/custodian/src/restore.rs:913-916`. (C) Treat the opening `Ok(CommitOutcome::Conflict)` at `:923-927` as acknowledged. No double in `restore_fence_generation.rs` can change `mpufence` between the pass's `get` and its `commit`, so the opening path's `FenceGenerationFault::ChangedUnderPass` is never reached by any test. Yet `crates/core/src/multipart.rs:1934` and `docs/design/architecture/05-building-block-view.md:204` both promise "both writes are conditioned on the bytes last read there". Concrete failing case under (C): passes P1 and P2 both read `{3,true}`. P1 opens `{4,false}`. P2's open conflicts but P2 carries on, finishes, and its close (`{4,false}`→`{4,true}`) lands while P1 is still fencing. The record now reads complete over P1's unfinished pass. Q(d) only checks the close's precondition. Fix: add a `Fault` that rewrites `mpufence` between the read and the commit, and assert `Err` with no mark and no fence written. Severity is moderate: the runbook runs one pass with writers stopped.; (minor) The patch weakens the K check (a second pass changes nothing) in `crates/custodian/tests/restore_open_fence.rs:763`. The old `unsettled_debug` compare covered every report field from `sessions_unsettled` onward, which included `segments_unaccounted`. The replacement compares `sessions_unsettled` only. Also compare `first.segments_unaccounted == second.segments_unaccounted` to keep the old strength.; `crates/custodian/tests/restore_fence_generation.rs:545`, `crates/server/src/cli.rs:3417`: The incomplete-generation test checks the stored marker but never the returned report's generation, while the CLI test supplies that field itself. The frozen C5 evidence confirms that deleting `fence_generation: Some(generation)` at `crates/custodian/src/restore.rs:662` survives. That regression would leave incomplete reports with `None`, silently suppressing the operator's generation/“NOT complete” line at `crates/server/src/cli.rs:1313`. Add a post-fix test asserting that a real report with unsettled sessions contains the same non-complete generation as the durable marker; keep the base-compatible regression file free of new API dependencies.. 5 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 31 mutants tested in 4m: 4 missed, 9 caught, 18 unviable
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
