# Result — issue 1012 / multipart-retirement-drain-step

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: Every multipart slice so far installs `retire:` obligations (Abort's
  `{session, all}`, #657's per-part `{chunks}`, #658's `{parts}` and `{generation}`), and nothing
  drains them: staged bytes stay protected for ever and Complete's superseded records never
  leave. #636's attempts at this step failed in two named ways: one re-derived the same first
  keys forever (no cursor); another truncated derivation while marking the obligation drained
  (silent part loss). This child adds a byte- and operation-budgeted, cursor-keyed drain step for
  `retire:bytes:` (mark, then delete the naming records) and `retire:records:` (delete only,
  never mark), and dispatches it from `reconcile_step` in the deployed custodian.
- Success criterion: A NEW test file `crates/custodian/tests/multipart_retirement_drain.rs`
  (core-level codec tests MAY go in another NEW `crates/core/tests/*.rs` file), over
  `RedbMetadataStore::in_memory()` and a real chunk store or placement-aware double, driving the
  drain **through `reconcile_step`** with the contexts the deployed loop passes
  (`crates/server/src/custodian.rs:397`), not through a test-only entry. It shows all of:
  **(B) Complete, then drain.** After a Complete (#658) and reconcile steps: no `part:` or
  `psum:` record of a published part remains; **no** `orphan:` mark names a chunk of the
  published map (`0016:1503`); a subset Complete's unnamed part has every fragment marked
  **before** its `part:` record is deleted (a recording wrapper shows the order) and, after
  reconcile steps past the orphan grace, its fragments are gone from the chunk store.
  **(C) Convergence at scale.** A `{parts}` obligation over **at least 4,001 parts** (planted as
  canonical records is fine) drains to empty across many steps, each commit within
  `MAX_BATCH_BYTES` and `MAX_BATCH_OPS` (preconditions counted); running the whole drain a second
  time is a no-op. A single entry whose own deletes exceed one batch's budget still makes
  progress every step and finishes. The obligation's stored cursor advanced every step; a drain
  restarted from a fresh process state (nothing in memory) resumes from the stored cursor; the
  obligation is deleted only in its last batch.
  **(C') Mark guard, after the `reclaiming` check.** For a position that already carries a
  mark, first: **(0) a mark that decodes as `reclaiming` is never replaced, whatever event it
  carries.** That shape is GC's recorded decision to reclaim
  (`crates/core/src/metadata.rs:109-140`: "No writer overwrites a `reclaiming` mark … Every mark
  writer that reads before it writes — the three-arm drain (#659) … inherits this rule"). GC
  deletes the key blind after destroying the fragment (`crates/custodian/src/gc.rs:804-824`), so
  a value written over it vanishes with it and leaves the new event with no evidence. The step
  leaves it **byte-identical** (assert the stored value before and after), keeps the obligation
  and its cursor (at or before that position) so the work resumes, and on a later pass, once
  GC has deleted the key, writes the position under `require_absent` (arm c). Then the three
  arms (`0016:1220-1226`): (a) same event ⇒ skipped, byte-for-byte unchanged; (b) different
  event ⇒ re-stamped under `require(== prior)` with this event and a fresh `orphaned_at`;
  (c) absent ⇒ written under `require_absent`. The fixture for arm 0 plants a `reclaiming` mark
  carrying a different event on a position the obligation names, runs a step, asserts it
  unchanged and the obligation still present, deletes the key as GC would, runs the next step,
  and asserts a fresh structured mark of this event.
  **(E-hold) Validated placements.** A `part:` record under a `retire:bytes:` obligation whose
  placement fails validation (length not the scheme's fragment count, e.g. a decoded `RS(0,0)`)
  is **never deleted unmarked**: the step neither marks from that placement nor deletes the
  record; it stops at that entry with a typed outcome and an audit event, leaves the obligation
  and its cursor in place, and every fragment of that chunk stays (GC's `StagedSet` keeps
  holding it, `crates/custodian/src/gc.rs:1337-1360`). Child-2 replaces the hold with retirement
  by chunk id.
  **(G) Restore stays accurate between drain batches** (the `deferred: #659` at
  `crates/custodian/src/restore.rs:1156`). An `Aborting@E+1` session fenced from `Completing@E`
  with segments written and parts staged (its two obligations, `retire:bytes:s:<id>:<E>` and
  `retire:records:s:<id>:<E>` owing `{seg}`, seeded as `restore_completing_fence.rs` seeds them)
  is drained **one step at a time**. After **every** step, the production
  `reconcile_after_restore` (`restore.rs:597`) runs and does not name the session (no
  `ChunkInNoPart`, `restore.rs:1225-1234`, and no `NoDeleter`, `:1172-1177`), until both
  obligations are gone.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: the retirement drain step for both modes; the cursor component inside
  `RetirePayload`; the mark guard with the `reclaiming` check (arm 0) ahead of the three arms,
  and its one seeded DST row; validation of placements before marking, with the hold for an untrusted
  `part:` record; the segments-before-parts rule between a session token's two obligations; the
  drain's dispatch from `reconcile_step` in the deployed custodian; replacing the
  `deferred: #659` comment at `restore.rs:1156`; the docs. / out of scope:
  - the owned `sidx:` walk, the terminal session delete, the `mpuctl` decrement, the tombstone
    input, and retirement by chunk id of untrusted records (child-2);
  - the orphan-identity cleanup pass, its marker, GC's gate on it, and any change to `unlink`
    or `commit_chunk_map_superseding{,_leased}` (child-3);
  - the reaper's fences and the `W_tombstone` clock (#625); the operator abort (#633);
  - any seeded DST scenario other than the one drain-vs-GC `reclaiming` row (#660 owns "two
    concurrent drainers" and its other two cases);
  - the bulk `DeleteObjects` batching (#509); the gateway / S3 verbs (#668–#670 under #508);
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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 208 mutants tested in 46m: 78 missed, 98 caught, 32 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1012/review-
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.50s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of #1012’s cursor-based multipart retirement drain and deployed custodian dispatch: one confirmed liveness defect requires builder iteration.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Convergence, mark-before-delete safety, reclaiming protection and restore consistency have falsifiable acceptance cases and explicit child boundaries (`brief.md:39`, `brief.md:123`). |
| C2 Reproduction (red pre-fix) | N/A | This is net-new functionality; the retained discriminator cannot compile against the old API, as independently reproduced after stashing the tracked patch (`brief.md:137`, `review-red.log:4`); acceptance of this limitation belongs to C4. |
| C3 Change | NEEDS-HUMAN | Approve the minimal lockfile scope exception needed by the required redb test—`brief.md:136` excludes `Cargo.lock`, but the new dev dependency necessarily adds its existing package to the custodian dependency list (`crates/custodian/Cargo.toml:56`, `Cargo.lock:4988`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Decide whether to accept the predeclared absence of a behavioral pre-fix red and the unavailable diff coverage—new tests and DST pass, but the reverted test never executes (`brief.md:145`, `review-red.log:4`, `review-ci.log:1714`, `gate-logs/C4-diff-cov.log:10`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Bound inspected obligations, including non-committing holds—otherwise a growing retirement backlog still monopolizes maintenance despite the commit budget (`crates/custodian/src/retirement.rs:174`, `crates/custodian/src/retirement.rs:227`, `review-held-probe.log:141`). |
| T1 Structure | PASS | The deployed reconciliation seam reaches the drain through existing metadata context; concrete redb remains a test dependency, preserving backend independence (`crates/custodian/src/reconciliation.rs:156`, `crates/custodian/Cargo.toml:39`). |
| T2 Shape | PASS | Persisted compatibility and corruption handling are covered: absent cursors preserve installer bytes, cursor round trips are exact, and invalid positions are rejected (`crates/core/src/multipart.rs:6512`, `crates/core/src/multipart.rs:6524`, `crates/core/src/multipart.rs:6565`). |
| T3 Runtime | FAIL | A single pass inspected all 4,096 held obligations in 65 pages; retirement precedes deployed GC, so its unbounded scan delays reclamation and return to the run loop (`review-held-probe.log:143`, `crates/custodian/src/retirement.rs:181`, `crates/server/src/custodian.rs:609`). |
| T4 Contribution | N/A | Contribution artifacts are intentionally drafted after Check; the publish gate owes their substantive audit (`gate-logs/T4-contribution.log:10`); the separate batch-review findings are adjudicated below. |
| T5 Judgment | NEEDS-HUMAN [impl] | Add a regression enforcing per-pass work limits for held and productive backlogs—the six new tests still pass when the budget increases instead of decreasing (`review-mutants.log:3`, `crates/custodian/tests/multipart_retirement_drain.rs:997`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether convergence after the liveness fix, retention-only holds pending child-2, and noisy downgrade restore reports meet rollout needs (`brief.md:128`, `docs/design/architecture/06-runtime-view.md:84`). |

Source citations without a `target/` prefix refer to the supplied `$PDCA_TARGET`; evidence and brief citations refer to this review directory. The target is readable, the supplied patch passes `git apply --reverse --check`, and all tracked changes were restored after the red-leg attempt. No production source was edited by this review.

**F1 — High priority: held obligations bypass the pass budget.** `steps_left` decreases only inside the `Some(commit)` branch at `crates/custodian/src/retirement.rs:227`. An unreadable obligation returns a plan without a commit at `:451`; owned residue and reclaiming marks can also produce non-committing holds. The outer loop therefore keeps fetching pages at `:181` with its budget unchanged. The ledger has no cardinality bound, and the server awaits this leg before its GC pass (`crates/server/src/custodian.rs:619`, `:634`). This is a liveness defect even though individual reads and commits are bounded.

An independent harness against the unchanged target exercised the public `reconcile_step` with a real `RedbMetadataStore::in_memory()` and a recorder. One pass inspected 64/1,024/4,096 unreadable obligations in 2/17/65 ledger-page reads respectively, always consuming the entire backlog (`review-held-probe.log:141`). Its generous 16-page ceiling failed. Reproduce from this directory with `TMPDIR="$PWD" CARGO_TARGET_DIR="$PWD/target/target" cargo test --offline --manifest-path pdca-reviewer-1012-probe/Cargo.toml -- --nocapture`. Bound planning/inspection as well as commits, and ensure bounded passes can resume beyond a permanently held prefix so later obligations are not starved. Add assertions for both held and productive backlogs. This single finding accounts for C5, T3 and T5.

**Verification supports the safety cases but does not establish the promised behavioral red.** The standalone six-test suite passed (`review-green.log:146`), then the tracked patch was stashed while the added discriminator remained. Cargo exited 101 on missing `RetireCursor`, `RetireWork` and the redb dev dependency, before running a test (`review-red.log:4`, `:18`). After restoration, the full CI invocation again passed all six tests (`review-ci.log:1714`), the four cursor codec tests (`review-ci.log:803`), and both new drain/GC DST tests (`review-ci.log:4214`, `:4227`). The frozen red-leg log reports the same compile-only limitation (`gate-logs/C4-verify.log:74`). The six deliberate fault-injection reds requested in `brief.md:148` are not shown by that log; the withheld builder notes were not consulted.

The independent full `cargo xtask ci` rerun passed spelling, docs lint/render, formatting, clippy/build, conformance, dependency-usage and statics checks, and the DST campaign. It exited 1 because this synthetic snapshot has no `.github/advisory-db` gitlink: the index assertion and advisory pin check fail (`review-ci.log:3678`, `:4326`). This is a snapshot limitation, not a patch defect. The frozen CI log actually shows the pinned database and successful deny checks (`gate-logs/C4-ci.log:4192`, `:4204`), followed by overall success (`:4839`). Both declared external tools, `typos` and the docs renderer, were exercised again (`review-ci.log:2`, `:7`); neither remains an unmet dependency.

The frozen diff-coverage gate measured no coverage because its patch application to `origin/main` failed (`gate-logs/C4-diff-cov.log:10`). That base-state caveat does not invalidate the executable target or establish a compilation defect. The frozen TiKV gate shows actual feature compilation for the adapter and server (`gate-logs/host-tikv.log:9`); the independent rerun also passed both compilations (`review-tikv.log:103`, `:202`).

The frozen mutation run reports 78 missed, 98 caught and 32 unviable mutants (`gate-logs/C5-mutants.log:91`). These totals alone are not 78 defects. A focused independent rerun reproduced the relevant survivor: replacing `steps_left -= 1` with `steps_left += 1` still passes all six new tests (`review-mutants.log:3`). The current scale test checks commit size and eventual convergence, but never constrains the work done by one pass (`crates/custodian/tests/multipart_retirement_drain.rs:997`, `:1029`).

**The batch review contains one duplicated defect and one out-of-scope request.** Its first three entries all identify F1 and are consolidated here (`gate-logs/T4-batch-review.log:10`). Its request for additional part/segment DST scenarios is rejected for this slice: the brief explicitly restricts seeded DST to the drain-versus-GC reclaiming race and assigns other scenarios to #660 (`brief.md:133`, `:157`). The delivered race checks both winners and reaches the conflicting-read/reclaiming-reread interleaving (`crates/dst/tests/custodian.rs:6825`); independent execution passed. No capability-probe/load-time symptom guard was introduced: the reclaiming and malformed-record checks implement the protocol’s required safety boundaries.

**Prior art was checked by affected path.** Read-only GitHub queries covered merged history and closed work for all 13 changed paths (`review-prior-art.log:1`). Neither new retirement file has merged history or closed-work matches (`:15`, `:17`); the reconciliation history includes the paged GC and staged-protection prerequisites (`:11`). The closed, not-planned parent #636 explicitly records the prior no-cursor and premature-completion failures (`review-prior-art-636.json:1`), and a separate closed-PR retirement search is captured in `review-prior-art-drain.json:1`. These searches support the brief’s prior-art account. No `INTEGRATION.md` was supplied or found in the target; the standing rubric in `AGENTS.md:137` was applied. The custodian change warrants observation in the project’s existing Tier-1/Tier-2 follow-up campaigns; those are not merge gates (`AGENTS.md:82`).
<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — issue 1012 (retirement drain step, #659 child-1)

Run against the patched tree at `$PDCA_TARGET`, using a scratch copy to build and to try breaking it. Short version: the legs the brief asks to be shown red really do go red. I found one input that wedges the drain, and two guarantees that no test protects.

## Refutation attempts that landed

- NEEDS-HUMAN [impl] — **An obligation installed just under the value ceiling wedges the whole retirement leg.** `crates/custodian/src/retirement.rs:584-585` writes the obligation back with its cursor added and never checks the new length against `MAX_VALUE_BYTES` (100,000, `crates/core/src/metadata.rs:577`). The cursor adds up to `CURSOR_BYTES` (`retirement.rs:140`). Nothing that installs obligations leaves room for it: Complete's flip does no size check on its records (`crates/core/src/multipart/complete.rs:598`, `deferred: #1011`). FoundationDB refuses any value over 100,000 bytes with the permanent error `2103 value_too_large` (`crates/metadata-fdb/src/lib.rs:1990-1991`). The `?` at `retirement.rs:228` then ends the whole leg, and every pass restarts the walk at the first `retire:` key (`retirement.rs:163-184`), so the failure repeats every pass. **Reproduced:** `retire:records:s:<…01>:1` holding `{"parts":[[100000,100000],[100002,100002],…]}` with 6,249 non-adjacent six-digit part numbers is 99,995 bytes. That is installable, since core allows part numbers up to 999,999 (`multipart.rs:1043`). A second obligation, `retire:records:s:<…02>:1` with 3 planted parts, sits later in key order. Over a redb wrapper that refuses puts over 100,000 bytes the way FDB does, I ran 10 `reconcile_step` passes with the reconstruction input. All 10 returned `Err(2103 value_too_large)`, and the second obligation and its 3 parts were never touched. The S3 range of 1–10,000 can't reach this size, so it stays hidden until a caller uses core's wider range. Fix: in `finish()`, turn an over-ceiling cursor value into a typed hold before committing, so a fault in one obligation can't stop the rest of the ledger. Also record the `CURSOR_BYTES` headroom requirement against #1011.
- NEEDS-HUMAN [impl] — **The owned-residue hold has no test, and removing it deletes a live obligation.** `retirement.rs:497-501` / `:606-613` hold a `{session}` obligation while `sidx:` residue exists. The code is correct: with `{"session":true,"parts":"all"}` plus one `sidx:<id>:000001:42` entry, the pass answers `Partial` and keeps the obligation. But C5 reports `retirement.rs:607:9 owned_residue -> Ok(None)` as MISSED, and I confirmed it. With that mutation the same input answers `Changed`, **the obligation is deleted, and the `sidx:` entry is left behind**, while all 6 tests in `crates/custodian/tests/multipart_retirement_drain.rs` still pass. The residue then has nothing to drain it under, which child-2's owned walk relies on. Add a leg that plants a `sidx:` entry under a `{session}` obligation and asserts the obligation stays.
- NEEDS-HUMAN [impl] — **No test checks the per-pass and per-step bounds the docs promise.** C5 MISSED: `steps_left -= 1` → `+=` and `/=` (`retirement.rs:227`), so a pass that drains the whole ledger at once still passes (C). Also MISSED: the lost-step bound (`:242-243`); the ledger paging arm (`:181`), because no test has more than `OBLIGATION_PAGE` = 64 obligations, and with that arm deleted every obligation past the first page never drains; and `positions_read += 1` → `*=` (`:1010`), the per-step read bound. The new runtime-view paragraph (`docs/design/architecture/06-runtime-view.md:84`, "a pass takes a fixed number of steps") and the module doc (`retirement.rs:67-71`) promise bounds nothing checks. This overlaps T4's blocking finding at `retirement.rs:174`/`:178` (held obligations use up no step budget). The fix for that should come with a test that counts commits and reads per pass and includes more than 64 obligations.

## Evidence I re-ran (could not refute)

- **Red→green, three of the six required reds, on a scratch copy:** removing the dispatch (`crates/custodian/src/reconciliation.rs:166` replaced by `Ok(Satisfied)`) makes all 6 tests fail. Removing arm 0 (`retirement.rs:1029` guard forced false) makes (C′) fail at the test's line 1200 ("(0) is never written"). Not storing the cursor (`retirement.rs:584` encodes `payload` instead of `advanced`) makes (B), (C) and (C′) fail. Unpatched: 6/6 pass. The tests drive the production `reconcile_step`, not a copy. (G) installs its obligations through the production restore fence (test line 1409), not hand-seeded values. C4-verify's UNVERIFIABLE is the pre-declared compile-only case, not a hidden red.
- **The C5 total of "78 missed" overstates the gap.** The 12 survivors in `crates/core/src/multipart.rs` were judged against core's tests only. I applied `RetirePayload::work -> empty()` (`:4031`) by hand, and all 6 custodian drain tests fail. The byte-budget arithmetic survivors (`retirement.rs:144, 397, 413, 684, 814, 1006, 1016, 1040`) are equivalent mutants: a step's worst case is about 2 × 100 KB of obligation plus 249 marks under 600 B each, far below `MAX_BATCH_BYTES` = 5 MB (`multipart.rs:5737`, `:5754`), so the byte budget can never bind. The survivors that matter are the ones in the two bullets above, plus the audit emitter `emit_step` (`retirement.rs:1142`).
- **Tried and failed to break:**
  - Arm (a) skipping without a precondition is safe. GC's keyed check (`crates/custodian/src/gc.rs:756-762`) protects any mark that names a live `retire:bytes:` obligation, so GC can't reclaim it under the step.
  - Segments-before-parts is keyed on the same token restore checks. `recheck_fenced` (`crates/custodian/src/restore.rs:1170-1177`) reads `retire:records:s:<id>:<E'-1>`, and `completing_teardown` (`multipart.rs:2697`) installs both obligations under that token.
  - I tried for a stuck `reclaiming` mark over a fragment that a staged record also protects. It needs a reused chunk id, and chunk ids are freshly minted (`crates/core/src/write.rs:168`).
  - Cursor room in `{chunks}` and flat `{generation}` values is fine: chunk lists are capped near 50 KB (`multipart.rs:5632-5638`).
  - The DST row (`crates/dst/tests/custodian.rs`, property 21) races the production `reconcile_step` against GC over the simulated TiKV store, and C4-ci ran both of its tests (`gate-logs/C4-ci.log:4727`, `:4739`).

## Scope and process notes

- NEEDS-HUMAN [human] — The brief lists `Cargo.lock` as out of scope, but the patch edits it: it adds `wyrd-metadata-redb` to `wyrd-custodian`'s dependency list (patch `Cargo.lock` hunk). The brief's own success criterion forces this, because it asks for `RedbMetadataStore::in_memory()` in a custodian test. Please confirm that this one internal dev-dependency line is acceptable.
- C4-diff-cov did not measure anything, because "patch.diff does not apply on origin/main". All of Check's evidence is against base `db1a066`, so the bundle needs a rebase before publishing (`multipart.rs` has sibling churn from #1010/#1011). This doesn't refute the fix. It means the merged result hasn't been tested yet.

<!-- pdca:leaf-complete -->

### Advisory — code-review

- NEEDS-HUMAN [impl] — **P1: Retirement marks can predate the retirement and expire too early.** `crates/custodian/src/retirement.rs:466` stamps every step with the pass's cached `now_millis`; `crates/custodian/src/reconciliation.rs:165` awaits reconstruction before dispatching retirement with that same timestamp. A Complete that supersedes an object during a long reconstruction pass can install an obligation after that timestamp. The drain then marks its fragments with the older time and removes the obligation; GC (`crates/custodian/src/gc.rs:683`) can immediately reclaim them if the intervening delay exceeded grace, tearing a reader that overlapped the supersede. Read the existing `ctx.clock` when constructing each drain step, after observing its obligation, and add a regression that advances that clock while reconstruction runs and installs a new retirement before the drain reads it.

- NEEDS-HUMAN [impl] — **P2: Held obligations bypass the pass's work bound.** `crates/custodian/src/retirement.rs:227` decrements `steps_left` only when a plan produces a commit, while the ledger walk at `crates/custodian/src/retirement.rs:173` continues through every held obligation. A backlog of unreadable obligations, owned-residue holds, or reclaiming marks therefore permits arbitrarily many reads and audit events in one pass, delaying the deployed loop's subsequent GC and shutdown. Bound planning visits/reads as well as commit attempts, with continuation that permits later obligations to be visited. Cover a backlog larger than the visit budget whose plans all return without committing. This confirms the repeated bounded-work finding in the frozen review log.

- NEEDS-HUMAN [impl] — **P3: Sparse part-set traversal repeatedly scans completed runs.** `crates/core/src/multipart.rs:3477` searches from the first run on every `first_from` call; the drain calls it for every successive member (`crates/custodian/src/retirement.rs:691`, `crates/custodian/src/retirement.rs:708`). For N singleton runs this performs O(N²) run comparisons across the drain. The runs are already sorted, so `partition_point` on their upper bounds provides the same lookup in O(log N) without materializing members. The scale fixture's single contiguous run does not exercise this cost.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C3 Change — Approve the minimal lockfile scope exception needed by the required redb test—`brief.md:136` excludes `Cargo.lock`, but the new dev dependency necessarily adds its existing package to the custodian dependency list (`crates/custodian/Cargo.toml:56`, `Cargo.lock:4988`).
- [ ] C4 Verification (red→green) — Decide whether to accept the predeclared absence of a behavioral pre-fix red and the unavailable diff coverage—new tests and DST pass, but the reverted test never executes (`brief.md:145`, `review-red.log:4`, `review-ci.log:1714`, `gate-logs/C4-diff-cov.log:10`).
- [ ] C5 Causal adequacy — Bound inspected obligations, including non-committing holds—otherwise a growing retirement backlog still monopolizes maintenance despite the commit budget (`crates/custodian/src/retirement.rs:174`, `crates/custodian/src/retirement.rs:227`, `review-held-probe.log:141`).
- [ ] T5 Judgment — Add a regression enforcing per-pass work limits for held and productive backlogs—the six new tests still pass when the budget increases instead of decreasing (`review-mutants.log:3`, `crates/custodian/tests/multipart_retirement_drain.rs:997`).
- [ ] Validation — fitness-to-purpose — Decide whether convergence after the liveness fix, retention-only holds pending child-2, and noisy downgrade restore reports meet rollout needs (`brief.md:128`, `docs/design/architecture/06-runtime-view.md:84`).
- [ ] **An obligation installed just under the value ceiling wedges the whole retirement leg.** `crates/custodian/src/retirement.rs:584-585` writes the obligation back with its cursor added and never checks the new length against `MAX_VALUE_BYTES` (100,000, `crates/core/src/metadata.rs:577`). The cursor adds up to `CURSOR_BYTES` (`retirement.rs:140`). Nothing that installs obligations leaves room for it: Complete's flip does no size check on its records (`crates/core/src/multipart/complete.rs:598`, `deferred: #1011`). FoundationDB refuses any value over 100,000 bytes with the permanent error `2103 value_too_large` (`crates/metadata-fdb/src/lib.rs:1990-1991`). The `?` at `retirement.rs:228` then ends the whole leg, and every pass restarts the walk at the first `retire:` key (`retirement.rs:163-184`), so the failure repeats every pass. **Reproduced:** `retire:records:s:<…01>:1` holding `{"parts":[[100000,100000],[100002,100002],…]}` with 6,249 non-adjacent six-digit part numbers is 99,995 bytes. That is installable, since core allows part numbers up to 999,999 (`multipart.rs:1043`). A second obligation, `retire:records:s:<…02>:1` with 3 planted parts, sits later in key order. Over a redb wrapper that refuses puts over 100,000 bytes the way FDB does, I ran 10 `reconcile_step` passes with the reconstruction input. All 10 returned `Err(2103 value_too_large)`, and the second obligation and its 3 parts were never touched. The S3 range of 1–10,000 can't reach this size, so it stays hidden until a caller uses core's wider range. Fix: in `finish()`, turn an over-ceiling cursor value into a typed hold before committing, so a fault in one obligation can't stop the rest of the ledger. Also record the `CURSOR_BYTES` headroom requirement against #1011.
- [ ] **The owned-residue hold has no test, and removing it deletes a live obligation.** `retirement.rs:497-501` / `:606-613` hold a `{session}` obligation while `sidx:` residue exists. The code is correct: with `{"session":true,"parts":"all"}` plus one `sidx:<id>:000001:42` entry, the pass answers `Partial` and keeps the obligation. But C5 reports `retirement.rs:607:9 owned_residue -> Ok(None)` as MISSED, and I confirmed it. With that mutation the same input answers `Changed`, **the obligation is deleted, and the `sidx:` entry is left behind**, while all 6 tests in `crates/custodian/tests/multipart_retirement_drain.rs` still pass. The residue then has nothing to drain it under, which child-2's owned walk relies on. Add a leg that plants a `sidx:` entry under a `{session}` obligation and asserts the obligation stays.
- [ ] **No test checks the per-pass and per-step bounds the docs promise.** C5 MISSED: `steps_left -= 1` → `+=` and `/=` (`retirement.rs:227`), so a pass that drains the whole ledger at once still passes (C). Also MISSED: the lost-step bound (`:242-243`); the ledger paging arm (`:181`), because no test has more than `OBLIGATION_PAGE` = 64 obligations, and with that arm deleted every obligation past the first page never drains; and `positions_read += 1` → `*=` (`:1010`), the per-step read bound. The new runtime-view paragraph (`docs/design/architecture/06-runtime-view.md:84`, "a pass takes a fixed number of steps") and the module doc (`retirement.rs:67-71`) promise bounds nothing checks. This overlaps T4's blocking finding at `retirement.rs:174`/`:178` (held obligations use up no step budget). The fix for that should come with a test that counts commits and reads per pass and includes more than 64 obligations.
- [ ] The brief lists `Cargo.lock` as out of scope, but the patch edits it: it adds `wyrd-metadata-redb` to `wyrd-custodian`'s dependency list (patch `Cargo.lock` hunk). The brief's own success criterion forces this, because it asks for `RedbMetadataStore::in_memory()` in a custodian test. Please confirm that this one internal dev-dependency line is acceptable.
- [ ] **P1: Retirement marks can predate the retirement and expire too early.** `crates/custodian/src/retirement.rs:466` stamps every step with the pass's cached `now_millis`; `crates/custodian/src/reconciliation.rs:165` awaits reconstruction before dispatching retirement with that same timestamp. A Complete that supersedes an object during a long reconstruction pass can install an obligation after that timestamp. The drain then marks its fragments with the older time and removes the obligation; GC (`crates/custodian/src/gc.rs:683`) can immediately reclaim them if the intervening delay exceeded grace, tearing a reader that overlapped the supersede. Read the existing `ctx.clock` when constructing each drain step, after observing its obligation, and add a regression that advances that clock while reconstruction runs and installs a new retirement before the drain reads it.
- [ ] **P2: Held obligations bypass the pass's work bound.** `crates/custodian/src/retirement.rs:227` decrements `steps_left` only when a plan produces a commit, while the ledger walk at `crates/custodian/src/retirement.rs:173` continues through every held obligation. A backlog of unreadable obligations, owned-residue holds, or reclaiming marks therefore permits arbitrarily many reads and audit events in one pass, delaying the deployed loop's subsequent GC and shutdown. Bound planning visits/reads as well as commit attempts, with continuation that permits later obligations to be visited. Cover a backlog larger than the visit budget whose plans all return without committing. This confirms the repeated bounded-work finding in the frozen review log.
- [ ] **P3: Sparse part-set traversal repeatedly scans completed runs.** `crates/core/src/multipart.rs:3477` searches from the first run on every `first_from` call; the drain calls it for every successive member (`crates/custodian/src/retirement.rs:691`, `crates/custodian/src/retirement.rs:708`). For N singleton runs this performs O(N²) run comparisons across the drain. The runs are already sorted, so `partition_point` on their upper bounds provides the same lookup in O(log N) without materializing members. The scale fixture's single contiguous run does not exercise this cost.
- [ ] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1012/review-
- [ ] size backstop — this slice is behaving oversized: patch is 199 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Plan
- Iteration delta (if iterating): Split the slice: 199 KB over 13 files (2x the size backstop), with real defects in several separate areas. Author the split in Plan (`pdca split 1012`). Each child must carry forward these findings from Check: - P1 (safety): the drain stamps marks with the pass's cached `now_millis` (retirement.rs:466; reconciliation.rs:165 runs reconstruction first). An obligation installed during the pass gets an old timestamp, and GC may reclaim it at once. Read `ctx.clock` per step, after the obligation is read, and add a regression for it. - Liveness: held / non-committing obligations don't consume the pass budget (retirement.rs:174-181, :227). Bound visits/reads as well as commits, resume past a held prefix, and test with more than 64 obligations (OBLIGATION_PAGE) for both held and productive backlogs. This is also the T4 batch-review blocker (3 of its 4 findings). - Wedge: writing the cursor back can push an obligation past MAX_VALUE_BYTES (retirement.rs:584-585); FDB 2103 then stops the whole leg every pass. Turn it into a typed hold and note the CURSOR_BYTES headroom against #1011. - Missing test: owned-residue hold (retirement.rs:607): a `{session}` obligation with `sidx:` residue must stay. - P3: `first_from` (multipart.rs:3477) is O(N^2) over sparse part sets; use partition_point. - The T4 finding asking for part/segment DST is out of scope (#660 owns it, per the brief); record-reject it. Open for the re-plan: whether the brief should allow the Cargo.lock dev-dependency line the redb test needs, and whether the compile-only red is accepted. The human did not decide these at this sign-off. Suggested split: (a) the drain step with the cursor plus the timestamp fix; (b) per-pass work bounds plus dispatch from reconcile_step.
- By / date: Eduard Ralph / 2026-11-10

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
