# Result — issue 1013 / multipart-session-terminal-delete

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: After child-1 a session's `retire:` obligations drain, but nothing ends the
  session: its owned `sidx:` residue (crashed in-flight intents from #657's staging) stays and
  keeps its fragments protected; its `mpu:` and `slot:` records stay; and `mpuctl.count` only
  rises, until every Create answers `503`. And an untrusted staged record on a dead upload (a
  placement that fails validation, or an `sidx:` value that will not decode) is held for ever:
  child-1 holds it, GC holds every fragment of its chunk, and the session can never reach its
  exit. This child adds the owned `sidx:` walk, retirement of untrusted records by chunk id for
  `Aborting` sessions, and the terminal session delete with its exactly-once `mpuctl` decrement,
  dispatched from the drain leg child-1 added to `reconcile_step`.
- Success criterion: A NEW test file
  `crates/custodian/tests/multipart_session_terminal.rs`, over `RedbMetadataStore::in_memory()`
  and a real chunk store or placement-aware double, driving everything **through
  `reconcile_step`** with the contexts the deployed loop passes
  (`crates/server/src/custodian.rs:397`). It shows all of:
  **(D) Owned walk and terminal delete.** An `Aborting` session holding committed parts and
  crashed in-flight `sidx:` residue, and a `Completed` session holding in-flight residue, each
  drain to: every owned fragment marked from its `sidx:` placement, then the entry deleted; no
  `sidx:` record of the session left. **`Aborting`:** the terminal delete fires **once**,
  `mpuctl.count` down by exactly one (two drainers racing the terminal delete through a wrapper
  still decrement once); no `slot:` record left; the `seggrp:` marker deleted when no `seg:`
  record names the nonce and **kept** when one does (`0016:674`). **`Completed`:** with the
  contexts the deployed loop passes, the `mpu:` record (the tombstone) is **kept** after its
  obligations and residue drain, and an identical Complete retry still answers
  `AlreadyCompleted` (`crates/core/src/multipart.rs:4789-4795` on `12dd082`); with the drain
  context's tombstone input saying the window has passed, the terminal delete fires once, as for
  `Aborting`. The terminal delete does not fire while the session's `sidx:` range is non-empty,
  nor while a session-scoped `retire:` obligation is left.
  **(E) Untrusted records.** For an `Aborting` session, each of these is retired **by chunk id**
  on a pass whose fleet is the whole operator-configured fleet: (E1) a `part:` record whose
  placement has the wrong length under a supported scheme (an RS(4,2) chunk placed on 5 servers;
  the maintainer's case); (E2) an owned `sidx:` entry with the same wrong-length placement (it
  decodes: `StagedPlacement` admits a length mismatch, `crates/core/src/multipart.rs:4441` on
  `db1a066`); (E3) an owned `sidx:` value that will not decode (e.g. an unsupported scheme, which
  `StagedPlacement` refuses, `multipart.rs:4481`), its chunk id taken from its key
  (`parse_sidx_key`, `multipart.rs:1436`) exactly as GC's hold takes it
  (`crates/custodian/src/gc.rs:1392`). For each: every fragment bearing that chunk id that the
  whole-fleet listing finds is marked, and for E2 every position the entry itself names as well;
  then the record is deleted (a recording wrapper shows the order); after GC past grace those
  fragments are gone and the terminal delete has fired.
  **Late landing** (a wrapper forces listing → record deletion → fragment publication): for E2,
  a fragment that lands at a position the entry names finds this retirement's mark and is gone
  after grace; for E3, a fragment that lands after the listing stays on disk across GC passes,
  never reclaimed and never an error: the stated residual (Design).
  **Controls:** an `Open` session with the same kind of record keeps the record and every
  fragment of its chunk; a committed object's fragments on the same servers are untouched; a
  server left out at startup, and a server unreachable this pass (each leaving the fleet short of
  `operator_fleet_size`), each leave the record and every fragment of its chunk in place with a
  typed hold, and once the server is back and the fleet is whole the record retires and that
  server's fragments are gone after grace too; a `part:` value that will not decode (an
  unsupported scheme such as `RS(0,0)`, which `decode_part_record` refuses) is held with a typed
  outcome and an audit event saying it needs a human, and the session's terminal delete does not
  fire.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: the owned `sidx:` walk for `Aborting` and `Completed` sessions; retirement by chunk
  id of untrusted `part:` and `sidx:` records for `Aborting` sessions, on whole-fleet passes only
  (replacing child-1's hold for that case); the terminal session delete and its exactly-once
  `mpuctl` decrement; the tombstone input on the drain context, with the deployed caller passing
  "retain"; the docs. /
  out of scope:
  - the drain step itself, its cursor, and the segments-before-parts rule (child-1);
  - the migration gate and the routing switch (child-3);
  - the reaper's fences (idle `Open`, `W_completing` rollback, `W_session`) and the
    `W_tombstone` clock (#625); the operator abort (#633);
  - recovering a `part:` value that will not decode: its key names no chunk
    (`part:<id>:<n>`, `multipart.rs:1386`), so this child holds it (E's control);
  - seeded DST (#660 owns "two concurrent drainers"; add no DST scenario);
  - the gateway / S3 verbs (#668–#670 under #508);
  - any edit to proposal 0016 or to any ADR; `Cargo.lock` and `deny*.toml`.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: unverifiable —                why this slice has no isolable red (the cargo output is above).
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage not measured — patch.diff does not apply on origin/main
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 247 mutants tested in 81m: 50 missed, 175 caught, 20 unviable, 2 timeouts

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 2 blocking, 1 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1013/review-
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.45s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #1013: finish multipart-session teardown by draining owned staging residue, safely retiring untrusted records, and releasing admission slots exactly once while retaining Completed tombstones.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Acceptance separates reclaimable residue from intentional retention, protects Complete retries, and states the whole-fleet requirement; the latest carry-forward explicitly authorizes the two progress repairs (`brief.md:26`, `brief.md:248`, `brief.md:342`). |
| C2 Reproduction (red pre-fix) | N/A | This is a declared net-new enhancement: retaining its test while stashing production changes reproduces four missing-API compile errors, not an executed behavioral red; the resulting verification decision is recorded under C4 (`brief.md:123`, `reviewer-red.log:3`). |
| C3 Change | PASS | The patch addresses the admitted teardown and progress failures without expanding into reaper timing, routing, or gateway work; mark-before-delete and the single-winner admission release remain intact (`crates/custodian/src/retirement/sessions.rs:766`, `crates/custodian/src/retirement/sessions.rs:899`, `brief.md:342`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Accept the predeclared mutation-based verification posture — 40 integration tests and 11 deliberate behavioral reds are reproduced, but no historical behavioral red or diff-coverage measurement exists; the local full-CI failure is a missing harness gitlink, not a patch defect (`brief.md:123`, `reviewer-mutations.json:1`, `reviewer-ci.log:4481`, `gate-logs/C4-diff-cov.log:10`). |
| C5 Causal adequacy | PASS | The tests distinguish removal of each required safety condition and both latest progress repairs from the restored passing implementation; no capability probe conceals a load-time cause (`reviewer-mutations.json:1`, `crates/custodian/src/retirement/sessions.rs:769`, `crates/custodian/src/retirement/sessions.rs:816`). |
| T1 Structure | PASS | Existing metadata/chunk-store seams, shared mark decoding, and the deployed loop's clock remain the correctness boundaries; living architecture documents cover the persisted progress records and teardown behavior (`crates/custodian/src/retirement/sessions.rs:133`, `crates/custodian/src/retirement/sessions.rs:602`, `docs/design/architecture/06-runtime-view.md:92`). |
| T2 Shape | PASS | The implementation remains the single slice explicitly retained at sign-off; the main added production module is 49,412 bytes including comments, and the broad context-field updates accompany focused integration coverage (`brief.md:347`, `crates/custodian/src/reconstruction.rs:142`, `crates/custodian/tests/multipart_session_terminal.rs:9`). |
| T3 Runtime | PASS | Bounded mark work, progress past held pages, whole-fleet refusal, and conservative tombstone retention are exercised; repeated fleet listing has an explicit cost bound and does not borrow GC's snapshot (`crates/custodian/src/retirement.rs:158`, `crates/custodian/src/retirement/sessions.rs:816`, `crates/server/tests/custodian_gc.rs:1380`). |
| T4 Contribution | PASS | Affected-path merged/closed-work searches found no competing terminal drain; the two batch-review DST findings are settled by the explicit #660 deferral, and the contribution-artifact audit is N/A until its mandatory publish rerun (`reviewer-prior-art.json:2`, `reviewer-prior-art.json:117`, `brief.md:347`, `gate-logs/T4-contribution.log:10`). |
| T5 Judgment | PASS | No new builder-fixable defect was established by the source review or counter-tests; previously settled scope, DST, and fragmentless-mark deferrals are respected, while unmeasured verification and operational fitness remain explicit human decisions (`brief.md:342`, `crates/custodian/src/retirement/sessions.rs:223`, `gate-logs/C5-mutants.log:65`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Approve the operational tradeoff — an unnamed late fragment can remain forever, fragmentless marks remain for possible late writes, incomplete fleets pause retirement, and Completed uploads retain admission slots until #625; tests demonstrate these limits but cannot choose their acceptability (`crates/custodian/tests/multipart_session_terminal.rs:1566`, `crates/custodian/src/retirement/sessions.rs:96`, `crates/server/src/custodian.rs:577`, `brief.md:347`). |

No new implementation defect is established; sign-off still owes the verification exception and operational-fitness decisions above. The safety checks produce genuine test failures when negated, the reported progress stalls have regression coverage, and the outstanding gate discrepancies have specific evidence rather than inferred patch failures. This advisory report does not change `check-gates.json` or clear its deterministic failures.

Repository `path:line` citations resolve in the supplied `$PDCA_TARGET`; brief, gate-log, and reviewer-evidence citations resolve in this report's directory. The target is readable and its patched source builds. `git apply --reverse --check patch.diff` and `git diff --check` succeed after the stash/pop exercise, confirming that the reviewed patch was restored. Temporary mutations were confined to a separate copy under the harness cwd.

The independent execution supports the feature's safety and progress claims:

- `cargo +1.96.0 test -p wyrd-custodian --test multipart_session_terminal --locked`: **40 passed**, both after restoring the stashed patch and after restoring the mutation copy (`reviewer-restored-green.log:188`, `reviewer-mutations-restored-green.log:47`). The initial run also passed on the host default toolchain; subsequent checks used the repository's pinned 1.96.0.
- Removing the owned-range gate, Completed retention, terminal session precondition, chunk-listing marks, whole-fleet restriction, or named-position coverage each caused an executed test failure. These are the six required negations in `brief.md:127`; the race failed its “released once” assertion, and the missing mark cases failed their fragment-evidence assertions (`reviewer-mutant-session-cas.log:12`, `reviewer-mutant-chunk-listing-marks.log:12`, `reviewer-mutant-named-positions.log:12`).
- Disabling scan progress past held entries, requiring positive named-position progress before bookmarking a listed-only entry, and discarding the untrusted-part cursor each turned its regression red. The restored suite passed (`reviewer-mutant-held-page-scan.log:12`, `reviewer-mutant-listed-entry-bookmark.log:12`, `reviewer-mutant-part-cursor.log:12`).
- Giving the deployed retirement context the reachable fleet count, or changing its tombstone input to `Elapsed`, separately failed the real `run_reconstruction_until` integration test. Restoring both inputs passed that test (`reviewer-mutant-deployed-fleet.log:71`, `reviewer-mutant-deployed-tombstone.log:18`, `reviewer-deployed-restored-green.log:8`). The complete 11-case manifest and runnable driver are `reviewer-mutations.json` and `reviewer-mutations.py`.

The full-CI discrepancy is a property of the harness copy. The independent `cargo +1.96.0 run --package xtask -- ci` run passed typos, docs lint/render and link audit, formatting, clippy, build, conformance, repository/static-state guards, and the DST campaign. It exited 1 because the synthetic index contains no `.github/advisory-db` gitlink: both the gitlink unit test and the advisory-pin stage report that same absence (`reviewer-ci.log:3831`, `reviewer-ci.log:4481`; the assertion is `xtask/tests/advisory_db.rs:67`). Consequently this local run did **not** execute the pinned dependency wall successfully. The frozen original-environment log explicitly records the pin fetch and successful default/all-feature dependency checks, then full CI success (`gate-logs/C4-ci.log:4343`, `gate-logs/C4-ci.log:4357`, `gate-logs/C4-ci.log:4992`). No synthetic replacement pin or tool shim was used.

Both TiKV feature checks were independently repeated successfully: metadata-tikv with `tikv`, and server with `tikv,etcd` (`reviewer-tikv-crate.log:103`, `reviewer-tikv-server.log:100`). Gitleaks scanned the patch and reported no leaks (`reviewer-gitleaks.log:2`). The brief's external dependencies, typos and docs rendering, actually ran successfully (`reviewer-ci.log:2`, `reviewer-ci.log:7`); they are not undischarged dependencies.

| Frozen gate | Adjudication | Evidence |
|-------------|--------------|----------|
| C4-ci | Original run PASS; independent rerun has the documented harness gitlink fault, not a source regression. | `gate-logs/C4-ci.log:4992`; `reviewer-ci.log:4481`. |
| C4-verify | UNVERIFIABLE historical red is independently reproduced; post-fix execution is green. | `gate-logs/C4-verify.log:16`; `reviewer-red.log:3`; `reviewer-restored-green.log:188`. |
| C4-diff-cov | Unmeasured: the oracle could not apply the stacked patch to origin/main. This is the already recorded base-ordering caveat, not a compilation defect in the supplied target. | `gate-logs/C4-diff-cov.log:10`; `brief.md:347`. |
| C5-mutants | Non-gating campaign remains 50 missed, 175 caught, 20 unviable, 2 timeouts. Reviewed its captured survivor list and independently exercised 11 focused negations; did not rerun the full 81-minute campaign or claim every survivor equivalent. | `gate-logs/C5-mutants.log:13`; `gate-logs/C5-mutants.log:65`; `reviewer-mutations.json:1`. |
| T4-batch-review | Both recorded blockers request seeded DST for this slice. Do not re-raise them: the latest sign-off explicitly says to add no DST scenario because #660 owns it. | `gate-logs/T4-batch-review.log:10`; `brief.md:347`. |
| T4-contribution | N/A: publication artifacts are absent by design; their substantive audit must rerun at publish. | `gate-logs/T4-contribution.log:10`. |
| host-tikv | PASS in the frozen evidence and both independent feature checks. | `gate-logs/host-tikv.log:211`; `reviewer-tikv-crate.log:103`; `reviewer-tikv-server.log:100`. |

The prior-art investigation used affected paths, not just the issue title. GitHub history queries for `crates/core/src/multipart.rs`, `crates/server/src/custodian.rs`, and the retirement modules identified the existing protocol/reconstruction work and no landed retirement module on the default branch. Closed-PR searches included the multipart and retirement source paths; query URLs and returned identifiers are preserved in `reviewer-prior-art.json`. The rejected predecessor is [#636](https://github.com/getwyrd/wyrd/issues/636): its [closure and split record](https://github.com/getwyrd/wyrd/issues/636#issuecomment-6094881252) identifies oversized scope and routing without a production drain. This patch stays within the accepted split and exercises the deployed caller. Its [tombstone correction](https://github.com/getwyrd/wyrd/issues/636#issuecomment-5081190714) also confirms that retaining Completed admission slots before #625 is intentional.

The measured integration tests use real redb and a placement-aware chunk-store double, as the brief permits; they explicitly create the forbidden partial-fleet, race, and late-publication conditions. Real deployment load, Tier-1 disk faults, and Tier-2 kill/reconstruct behavior were not measured here. Follow-up observation in those custodian campaigns is warranted, without reopening the settled #660 DST deferral. No additional `INTEGRATION.md` was present in the supplied target. The permanent fragmentless-mark policy remains the human tracking decision associated with #825, as directed by `brief.md:347`, rather than a new builder item.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — issue 1013, iteration 5 (owned walk, by-chunk retirement, terminal delete)

All claims below come from a scratch copy of `$PDCA_TARGET` (the patch applied), with
`cargo test -p wyrd-custodian --test multipart_session_terminal` re-run there. The green
baseline reproduces: 40 passed.

- **This round's red→green holds (not a refutation).** I removed each of this round's two fixes
  by hand and re-ran the file. (1) Scan position off (`after` forced to `None` at
  `crates/custodian/src/retirement/sessions.rs:581`, the move at `:816` disabled): 8 tests fail.
  Among them are the behavioural ones: the three-held test's "retires within 8 passes"
  (`crates/custodian/tests/multipart_session_terminal.rs:2890`) and both page-of-held-entries
  legs (`:2659`, Aborting on a short fleet and Completed with `Tombstone::Elapsed`). (2) The old
  `covered > 0` rule put back on the bookmark choice (`sessions.rs:769`):
  `d_an_undecodable_entry_listed_to_the_brim_behind_two_held_ones_retires` fails (`:2820`). The
  tests run the production `reconcile_step`, with the context built the way the deployed caller
  builds it (`multipart_session_terminal.rs:697-736`, `operator_fleet_size = configured.len()`),
  not a copy of the logic. C4-verify's UNVERIFIABLE (the test calls API the patch adds) is the
  pre-declared posture; these hand reds stand in for it.

- NEEDS-HUMAN [impl] — **Held wide entries move the bookmark on every pass, for ever, and each
  move costs a session step. Sixteen such sessions starve every session listed after them.**
  How it happens:
  - When a step runs out of reads re-reading an entry from position 0, that entry becomes the
    `advanced` candidate (`sessions.rs:769-770`) even if it marked nothing.
  - `advanced` beats a held entry's saved count (`:779`), so the bookmark moves (`:787`).
  - A moved bookmark skips the free idle return (`:831`), so the commit is charged a step
    (`:840`), and the visit then ends (`:857`).
  - The session listing restarts at the head every pass (`:241`), so the same sessions pay
    first, every pass.

  **Repro:** take the fixture of `d_a_trusted_entry_behind_three_held_part_way_retires`
  (`multipart_session_terminal.rs:2863`) without its trusted entry: three RS(128,128) entries,
  each with a mark of none of the three shapes at position 150. In steady state that session
  commits 2 session-leg batches every pass. The bookmark cycles (entry 3 at 150) → (entry 2 at
  81) → (entry 2 at 150) → (entry 3 at 81) → … and never settles. Now list 16 such `Aborting`
  sessions ahead of a 17th aborted session that owns nothing. After 150 deployed passes the
  17th still has its `mpu:` record and its `mpuctl` slot. Every pass answers `Blocked` (for the
  churners' holds), and no audit event names the starved session. With 15 churners it ends
  normally.

  This breaks the module's own stated rule. `sessions.rs:71-73` and
  `crates/custodian/src/retirement.rs:166-167` say a held session must not spend a step every
  pass, because "enough of them would starve every session listed after them". That is the
  reason the scan position was put outside the step budget, but the bookmark commit is still
  inside it. It also breaks brief C-1 ("every session state the drain owns has an exit") for
  healthy sessions.

  **Test gap:** the existing three-held test only asserts that the trusted entry retires. It
  never asserts that the held steady state goes quiet. The page-of-held-entries test does
  (`:2692-2700`).

  **Trigger:** wide placements, where re-reading the held entries' already-marked positions
  costs more than the 233 reads a step has, plus holds that cost reads (unreadable or
  `reclaiming` marks, or a failed listing behind wide named positions). It is rare, but it is
  the same stall family as the iteration-3 and iteration-4 sign-off items.

  **Fix direction (the builder's call):** don't charge a step for a commit that only moves the
  bookmark while the step holds something, or don't let an entry that was re-read from 0 and
  marked nothing take the bookmark from a held entry's count. Add a steady-state-quiet assertion
  to the three-held test and a 17-session starvation regression.

- NEEDS-HUMAN [human] — **The gating T4 batch review is red only on DST scope.** Its two
  blocking items (`gate-logs/T4-batch-review.log`: `sessions.rs:586`, `:841`) ask for seeded
  Tier-0 DST (the repo's deterministic-simulation tests) coverage of the destructive owned-entry
  and terminal-delete paths. The brief says "add no DST scenario (#660 owns it)", and every
  sign-off repeated that. But the in-code marker at `sessions.rs:223` defers only "two drainers
  of one session". The rubric's test-fidelity MUST covers any "new destructive or concurrent
  path", so the reviewer's reading is not baseless. A human has to choose: record the rejection
  with the brief's reason, widen the `// deferred: #660` marker (if #660 really owns the
  single-drainer path), or open a tracking issue. Builder iteration can't settle this.

- **Tried to refute these; could not:**
  - *Whole-fleet gate.* `ByChunk::whole` (`retirement.rs`, `fleet.len() ==
    operator_fleet_size`) uses the same `fleet` and `operator_fleet_size` the deployed GC gate
    uses (`crates/server/src/custodian.rs:577`, and the `if fleet.len() == operator_fleet_size`
    GC branch). A failed `list_fragments` is a `PartialFleet` hold, not a short retirement.
  - *Scan-position hold counting* (`sessions.rs:814-815`). Every hold pushed in `walk_step`
    names its own entry key (`:403`, `:415`, `:429`, `:507`, `:518`, `:748`, `:759`, and
    `partial_fleet`), so a held entry is never miscounted as not held, or the reverse.
  - *Terminal delete on a non-empty range.* `Walked::Empty` comes only from the head page
    (`:586-597`), so entries before a scan position can't be skipped into an early delete.
  - *Exactly-once decrement.* The batch carries preconditions on the exact session-record bytes
    and the exact `mpuctl` bytes (`:899`, `:927`).
  - *Surviving C5 mutants.* The batch-budget arithmetic (`:483-484`, `:544-549`, `:659-668`)
    can't bind: the batch limit is 5 MB against at most ~2 MB of marks and deletes, and the op
    count is pinned by the `const` assert at `:163-164`. The `read_entry` owner guard (`:410`)
    is always true inside `sidx_range`. I found no survivor that hides a safety defect.

Scratch used: `$PDCA_SCRATCH/pdca-adversary-1013-redleg`, removed after this report.

<!-- pdca:leaf-complete -->

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/custodian/src/retirement/sessions.rs:795`: A delayed bookmark-only commit can recreate `custodian:owned-walk:<id>` after terminal deletion. With a 256-position entry whose marks already carry this walk's event and no bookmark (the supported recovery case at `crates/custodian/tests/multipart_session_terminal.rs:2333`), drainer A reads 233 marks and prepares only `require_absent(bookmark)` plus a bookmark put. Pause A before commit; drainer B can finish the walk and terminal-delete the session and bookmark. A's absence precondition now succeeds again. Its subsequent empty-range visit stops when it discovers the session gone, leaving the recreated bookmark permanently: future passes enumerate only `mpu:` records. Reserve and add the exact session-record precondition to bookmark-producing commits, as `move_scan` already does at `crates/custodian/src/retirement/sessions.rs:1065`. Cover this ordering with a wrapper that delays A's bookmark commit until B finishes; the current terminal-cleanup test at `crates/custodian/tests/multipart_session_terminal.rs:3162` covers only a bookmark written before terminal deletion.

No additional correctness or reuse/simplification/efficiency findings. Review used the target source and frozen gate evidence; no target files were changed and no builds were rerun.
<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Accept the predeclared mutation-based verification posture — 40 integration tests and 11 deliberate behavioral reds are reproduced, but no historical behavioral red or diff-coverage measurement exists; the local full-CI failure is a missing harness gitlink, not a patch defect (`brief.md:123`, `reviewer-mutations.json:1`, `reviewer-ci.log:4481`, `gate-logs/C4-diff-cov.log:10`).
- [ ] Validation — fitness-to-purpose — Approve the operational tradeoff — an unnamed late fragment can remain forever, fragmentless marks remain for possible late writes, incomplete fleets pause retirement, and Completed uploads retain admission slots until #625; tests demonstrate these limits but cannot choose their acceptability (`crates/custodian/tests/multipart_session_terminal.rs:1566`, `crates/custodian/src/retirement/sessions.rs:96`, `crates/server/src/custodian.rs:577`, `brief.md:347`).
- [ ] **Held wide entries move the bookmark on every pass, for ever, and each move costs a session step. Sixteen such sessions starve every session listed after them.** How it happens:
- [ ] **The gating T4 batch review is red only on DST scope.** Its two blocking items (`gate-logs/T4-batch-review.log`: `sessions.rs:586`, `:841`) ask for seeded Tier-0 DST (the repo's deterministic-simulation tests) coverage of the destructive owned-entry and terminal-delete paths. The brief says "add no DST scenario (#660 owns it)", and every sign-off repeated that. But the in-code marker at `sessions.rs:223` defers only "two drainers of one session". The rubric's test-fidelity MUST covers any "new destructive or concurrent path", so the reviewer's reading is not baseless. A human has to choose: record the rejection with the brief's reason, widen the `// deferred: #660` marker (if #660 really owns the single-drainer path), or open a tracking issue. Builder iteration can't settle this.
- [ ] `crates/custodian/src/retirement/sessions.rs:795`: A delayed bookmark-only commit can recreate `custodian:owned-walk:<id>` after terminal deletion. With a 256-position entry whose marks already carry this walk's event and no bookmark (the supported recovery case at `crates/custodian/tests/multipart_session_terminal.rs:2333`), drainer A reads 233 marks and prepares only `require_absent(bookmark)` plus a bookmark put. Pause A before commit; drainer B can finish the walk and terminal-delete the session and bookmark. A's absence precondition now succeeds again. Its subsequent empty-range visit stops when it discovers the session gone, leaving the recreated bookmark permanently: future passes enumerate only `mpu:` records. Reserve and add the exact session-record precondition to bookmark-producing commits, as `move_scan` already does at `crates/custodian/src/retirement/sessions.rs:1065`. Cover this ordering with a wrapper that delays A's bookmark commit until B finishes; the current terminal-cleanup test at `crates/custodian/tests/multipart_session_terminal.rs:3162` covers only a bookmark written before terminal deletion.
- [ ] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 2 blocking, 1 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1013/review-
- [x] **Criterion (E) conflates rejected geometry with a decoded placement mismatch, hiding a possible decoder-recovery change.** `brief.md:43-45` requires a "decoded `RS(0,0)`" example. The current part decoder rejects unsupported schemes (`crates/core/src/multipart.rs:2940-2947`, `:3180`, `:3208`), and `StagedPlacement` rejects them at construction/decode (`:4285-4287`, `:4306-4309`). Consequently an invalid-scheme `part:` makes GC's staged set incomplete, rather than merely holding its chunk (`crates/custodian/src/gc.rs:1405-1415`, `:1360-1361`). Revise (E) to use a supported scheme with the wrong placement length; test invalid-scheme `sidx:` separately through its key-derived chunk id. If recovering invalid-scheme parts is intended, explicitly scope how all their chunk ids are recovered without weakening the decoder. The current fixture cannot exercise the claimed path.
- [x] **A listing cannot evidence fragments that land afterward.** `brief.md:154-158` marks only listed fragments and then deletes the untrusted record, but its invariant requires marking "every position its chunk could occupy" (`brief.md:55-57`). A staged write authorized before Abort can still publish after that listing: `crates/core/src/multipart/stage.rs:33-53` explicitly distinguishes abandoning a write from retracting it, and `crates/traits/src/lib.rs:1158` defines listing as the fragments currently held. If an undecodable `sidx:` is removed while its chunk is absent from the listing, a subsequent landing has neither staging evidence nor an orphan mark; GC keeps such an unevidenced fragment (`crates/custodian/src/gc.rs:736-738`). Specify durable coverage or a justified quiescence condition before deletion, and require a wrapper-forced listing → record deletion → fragment publication regression. Waiting out GC grace alone cannot repair a mark never created.
- [x] **Chunk-wide retirement lacks an explicit whole-fleet prerequisite.** `brief.md:112-113` leaves fleet-listing integration open, while `:156-158` authorizes deleting the record after marking what those listings find. The deployed loop can have startup-omitted or temporarily unreachable servers; it deliberately gates GC on `fleet.len() == operator_fleet_size` to prevent deleting chunk-wide evidence while a missing server still holds fragments (`crates/server/src/custodian.rs:636-658`, `:680`). Neither the drain-context contract nor criterion (E) requires equivalent coverage. Specify that untrusted records and terminal deletion remain held unless every configured server's listing succeeds, or define durable evidence for omitted servers. Add startup-missing and runtime-unreachable controls followed by server return; otherwise the proposed recovery can strand exactly the bytes it promises to reclaim.
- [x] size backstop — this slice is behaving oversized: patch is 325 KB (threshold 100 KB); patch touches 25 files (threshold 20). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Keep iteration 5's design and patch (session leg, terminal delete with exactly-once mpuctl decrement, resumable part cursor, owned-walk bookmark, scan position past held pages, whole-fleet gate, deployed Tombstone::Retain); the safety core held again under hand-mutations. Fix only the two new bookmark bugs: 1. Starvation (adversary) — sessions.rs:769-787/831-840: an entry re-read from position 0 that marked nothing still takes the bookmark from a held entry's count, so held wide entries move the bookmark every pass and each commit costs a session step; 16 such Aborting sessions starve a 17th forever (no audit event). Do not charge a step for a bookmark-only move while the step holds something, or do not let a zero-progress re-read displace a held entry's count. Add a steady-state-quiet assertion to the three-held test (multipart_session_terminal.rs:2863, no trusted entry) and a 17-session starvation regression through repeated reconcile_step. 2. Recreated bookmark (code review) — sessions.rs:795: a bookmark-only commit guarded only by require_absent(bookmark) can recreate custodian:owned-walk:<id> after another drainer terminal-deleted the session. Add the exact session-record precondition to every bookmark-producing commit, as move_scan does at sessions.rs:1065. Add a wrapper test that delays drainer A's bookmark commit until B finishes the terminal delete, asserting no bookmark is left. Size: still one slice under the iteration-3 rule (logic code ~84 KB, under 150 KB). Diff coverage stays unmeasurable while stacked; not a builder defect. Add no DST scenario (#660 owns it), unchanged from prior rounds.
- By / date: Eduard Ralph / 2026-11-10

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
