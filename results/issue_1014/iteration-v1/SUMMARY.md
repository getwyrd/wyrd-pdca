# Result — issue 1014 / retirement-routing-migration-gate

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: `unlink` writes every orphan mark inline in its delete batch
  (`crates/core/src/metadata.rs:2141-2164`) and refuses a segmented object (`:2148-2151`);
  `commit_chunk_map_superseding` does the same for the prior map (`:2235`; a segmented prior is
  refused at `:2188-2193` for `commit_chunk_map`). Past 55 chunks at RS(6,3) that batch exceeds
  the protocol's `MAX_BATCH_OPS` (500): a 100-chunk object's unlink is 904 operations, a maximal
  object's marks exceed the transaction envelope outright (`0016:1091-1096`), and a segmented
  object cannot be deleted at all. 0016 moves this fan-out to the retirement ledger
  (`0016:1081-1106`), but only after a one-time cleanup of stale marks has completed
  (`0016:1251-1278`): until then GC's keyed lookup can find an old event's obligation drained
  and reclaim a just-unreferenced fragment before the drain re-stamps its expired mark. This
  child adds the cleanup pass, its durable completion marker, the gate on it, and the routing
  switch behind the gate.
- Success criterion: A NEW test file `crates/custodian/tests/retirement_routing.rs` (core-level
  tests MAY also go in another NEW `crates/core/tests/*.rs` file), over
  `RedbMetadataStore::in_memory()` and a real chunk store or placement-aware double, driving the
  cleanup pass and the drain **through `reconcile_step`** with the contexts the deployed loop
  passes (`crates/server/src/custodian.rs:397`). It shows all of:
  **(F) The migration gate** (`0016:1251-1278`). Before a **valid** cleanup marker exists, the
  identity-keyed retirement paths are off: GC treats every `orphan:` mark as protecting, and
  `unlink` / supersede behave exactly as on the base. The cleanup pass walks `orphan:` page-wise
  and judges each mark by the rule in Design, shown leg by leg:
  (F1) **Committed reference:** a mark on a fragment a committed map names is dropped.
  (F2) **Staged, never published:** a stale mark (legacy, as an old custodian wrote them) on a
  fragment only an `sidx:` entry or an unpublished part's `part:` record names is dropped. Then
  the plan review's sequence: the part publishes (production Complete), the records drain retires
  its `part:` records, and the object is deleted with the routed `unlink`. GC-only passes with
  the clock past the stale stamp's grace, run **before** the drain marks, reclaim none of its
  fragments; passes inside the drain's fresh grace reclaim none; passes past it reclaim all.
  (F3) **The 2026-09-12 trap:** a mark on a fragment that only a **published** part's records
  name (its number in a pending `retire:records:` `{parts}` obligation) and no committed map names
  (the object was deleted by the inline `unlink`, which wrote the mark) is kept byte-identical;
  after the records drain retires the part records, GC reclaims those fragments after that mark's
  grace. No fragment is left with no mark and no reference.
  (F4) **Left alone:** a `reclaiming` mark, and a structured mark whose event's `retire:bytes:`
  obligation is present (a drain mid-retirement), are byte-identical after the pass, whatever
  names their fragment.
  (F5) **Race with unlink:** a wrapper pauses the cleanup between its read and its commit of an
  F1 drop, runs a production `unlink` of that object (inline, the gate being closed), then lets
  the cleanup commit. The drop fails, the mark the unlink wrote survives, and the fragments go only
  after its grace. The same with an overwrite in place of the unlink.
  (F6) **Completion:** a walk that meets an unreadable inode, an unreadable `seg:` record or an
  unreadable `part:` record (one fixture each) reaches the end of the range and writes **no**
  marker; it reports a typed blocked outcome naming the record, and the routing stays off. After
  the record is repaired, a later walk completes and writes the marker, and only then does (A)'s
  routing apply. A walk interrupted mid-way writes no marker either.
  (F7) **Restore:** a marker written under one restore-fence generation, followed by the
  production post-restore pass (`reconcile_after_restore`, which opens a new generation as its
  first write, `crates/custodian/src/restore.rs:517-522`), reads as **not** complete: with stale
  marks planted as a restored image would carry them, the routing and GC's keyed lookup stay off
  and GC treats every mark as protecting, until the cleanup walks again and writes a marker for
  the new generation.
  **(A) Positive reclamation after `unlink` and overwrite**, all with the marker present.
  (A1) The tracker's historical case: a committed 30-chunk RS(6,3) object (270 fragments, 274
  operations, which fits one batch, so the inline path may still serve it) is deleted with
  `metadata::unlink`. After reconcile steps past the orphan grace, every one of its 270 fragments
  is gone. (A2) **Past the batch budget:** a committed flat 100-chunk RS(6,3) object (900
  fragments, 904 operations inline) is deleted with `metadata::unlink`. **Before any reconcile
  step**, the unlink's own batch has installed exactly one `retire:bytes:g:<inode>:<version>`
  obligation and written no `orphan:` mark. After reconcile steps past the orphan grace,
  **every one of its 900 fragments is gone from the chunk store** and no `retire:` obligation or
  `orphan:` mark of it remains. **The boundary, preconditions counted:** an 83-chunk RS(4,2)
  object (498 marks + 2 deletes + 2 preconditions = 502 operations; 500 if the preconditions
  were ignored) also takes the obligation path, with no inline mark. (A3) The same as A2 for an
  ordinary overwrite (`write::commit_overwrite`) of such an object, for the prior generation's
  fragments, and for an `unlink` of a **segmented** object published by #658's Complete (no
  inline path exists for one). Asserted as fragments absent, never as "no failure observed".
  **And present while they must be:** in (A2) and (A3), GC-only passes run between the unlink (or
  overwrite) and the drain's marks, with the clock past every older mark's grace, reclaim none of
  the object's fragments; passes inside the grace the drain's marks start reclaim none; only
  passes past it leave them gone. An eventual-absence check alone also passes after a premature
  reclaim, so this half is required.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: the orphan-identity cleanup pass, its conditional drops, and its durable completion
  marker, including its validity across a metadata restore (a marker an image brings back never
  reads as complete once the post-restore pass has run); GC's fallback to "every mark protects"
  until a valid marker is observed; the switch of `unlink` and
  `commit_chunk_map_superseding{,_leased}` to one `retire:bytes:{generation}` obligation above
  one batch's budget, behind the marker; the docs. / out of scope:
  - the drain step, its cursor and its dispatch (child-1); the owned walk and the terminal
    delete (child-2);
  - the bulk `DeleteObjects` per-request batching (#509's, inside the per-transaction bound
    `0016:1107-1122`);
  - seeded DST (#660); the gateway / S3 verbs (#668–#670 under #508);
  - any edit to proposal 0016 or to any ADR (ADR-0045's refinement is #811); `Cargo.lock` and
    `deny*.toml`.

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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 243 mutants tested in 57m: 100 missed, 118 caught, 25 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1014/review-
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 16.74s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of #1014’s orphan-identity migration gate and large-object retirement routing: the routing regression is independently red→green, but three reproduced defects undermine safe reclamation and cleanup progress.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance criteria distinguish eventual reclamation from premature deletion and require restore-aware completion; they give executable counterexamples for the intended invariant (`brief.md:36`, `brief.md:95`, `brief.md:262`). |
| C2 Reproduction (red pre-fix) | PASS | With the patch stashed, an independently written test using existing APIs executes and fails because the 100-chunk unlink installs no retirement obligation (`review-base-red.log:10`; `crates/core/src/metadata.rs:2196`). |
| C3 Change | PASS | The submitted changes stay within migration, routing, fixture adaptation and living documentation; reverse-applying the supplied patch checks cleanly against the restored target (`crates/custodian/src/reconciliation.rs:174`; `docs/design/architecture/05-building-block-view.md:258`). |
| C4 Verification (red→green) | PASS | The same routing discriminator passes after stash-pop, and the existing core/custodian suite passes 1,112 tests; this establishes the routing change, while the additional safety failures are recorded below (`review-base-green.log:8`; `review-green.log:163`; `review-target-tests.log`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Prevent the migration from certifying unsafe restored marks or erasing published-part unlink evidence—both sequences violate the required lifetime guarantee in executable counterexamples (`crates/custodian/src/gc.rs:696`; `crates/custodian/src/orphan_identity.rs:563`; `review-counterexamples.log:6`). |
| T1 Structure | PASS | Production routing and cleanup remain on the existing metadata and reconciliation seams, with the deployed retirement leg retained before cleanup (`crates/core/src/metadata.rs:2261`; `crates/custodian/src/reconciliation.rs:158`). |
| T2 Shape | PASS | Marker decoding rejects noncanonical representations, absent fields remain omitted, and persisted key ownership and restore validity are documented (`crates/core/src/orphan_identity.rs:92`, `crates/core/src/orphan_identity.rs:400`; `docs/design/architecture/05-building-block-view.md:258`). |
| T3 Runtime | FAIL | Two conflicting drops can require eight keyed reads after reserving seven; the reproduced budget-boundary case panics in the production cleanup (`crates/custodian/src/orphan_identity.rs:142`, `crates/custodian/src/orphan_identity.rs:920`; `review-counterexamples.log:17`). |
| T4 Contribution | NEEDS-HUMAN | Confirm the relevant closed/rejected-work dispositions before sign-off—affected-path history and closed-work searches ran, but bounded search results cannot establish the complete rejected-diff history; the frozen batch review also has three independently confirmed defects awaiting fixes or recorded rejection (`review-prior-art.txt:1`; `gate-logs/T4-batch-review.log:10`; `AGENTS.md:221`). |
| T5 Judgment | NEEDS-HUMAN [impl] | Correct the retry budget and retain regressions combining damaged-session recovery, preserved reclaiming marks with routed deletion, and two conflicts at the read limit—the current 15 tests pass despite all three failures (`crates/custodian/tests/retirement_routing.rs:1066`; `crates/custodian/src/orphan_identity.rs:863`; `review-counterexamples.log:28`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the corrected migration is ready for restored production metadata and operational rollout—reader grace must survive reopening the gate, and the documented restore procedure must run before gateways return (`docs/design/architecture/06-runtime-view.md:96`; `review-counterexamples.log:14`). |

Source citations above and below resolve under `$PDCA_TARGET` (`./target`); review logs and probe files resolve in this report’s directory. The target is the disposable snapshot based on `3c600b657ccb7e54a4a60c72b220b7bf1d33ad4a`. The patch was restored after the independent base run, and `git -C "$PDCA_TARGET" apply --reverse --check ../patch.diff` succeeds. No production source was edited by the reviewer.

**Three actionable findings are reproduced against the applied patch.** The frozen review’s five entries collapse to these three defects; its three budget reports describe the same failure.

1. **[P1] A preserved reclaiming mark permits deletion before the new grace window.** Cleanup leaves every reclaiming mark untouched (`crates/custodian/src/orphan_identity.rs:934`) and can nevertheless certify completion (`:266`). After a restored live object is deleted through the new obligation route, GC resumes the old reclaiming mark directly, without checking the new retirement or grace (`crates/custodian/src/gc.rs:696`). The new obligation does not restore the committed-reference protection that the delete removed.

   The counterexample creates a 100-chunk RS(6,3) object, plants one ancient reclaiming mark on a live fragment, runs the production post-restore pass and cleanup, then calls production `unlink`. Before any drain or elapsed new grace, one GC-only pass deletes that fragment (`review-counterexamples.log:14`). This can truncate a reader holding the just-deleted generation. The existing F4 test proves preservation, but stops before this subsequent delete (`crates/custodian/tests/retirement_routing.rs:1066`). Preserve the required mark semantics while preventing gate reopening/routing from authorizing this premature reclaim; add the combined restore→cleanup→delete→GC regression.

2. **[P1] An unreadable completed session loses legitimate unlink evidence.** A session decode failure becomes `Phase::Unknown` (`crates/custodian/src/orphan_identity.rs:540`), which is classified as staged (`:563`). Publication obligations are examined only for a successfully decoded completed session (`:543`), and the final verification again treats decode failure as live (`:1053`). An unreadable value does not prove that its parts were never published.

   The counterexample publishes a part through production Complete, freezes its records obligation, deletes the object inline, then damages the completed session value. Cleanup writes a complete marker and deletes all six unlink marks (`review-counterexamples.log:6`). After restoring the original session bytes and draining its records obligation, the parts are gone and GC leaves the fragments permanently unmarked and unreclaimed (`review-counterexamples.log:9`). This is the F3 trap under a damaged session. Preserve the marks and block certification when publication history cannot be established; an exact-byte precondition on unreadable session bytes does not make that history known.

3. **[P2] Retry accounting can panic and stop migration progress.** `READS_PER_MARK` reserves `1 + 2 * 3 = 7` reads (`crates/custodian/src/orphan_identity.rs:142`), but `judge` repeats the retirement-token lookup on each attempt (`:897`, `:938`). A staged or segmented drop can therefore consume two lots of one token lookup, two namer reads and one conflict reread: eight reads. The final unchecked decrement underflows (`:920`).

   The counterexample places 4,089 structured marks requiring one lookup each ahead of a staged mark, leaving seven reads. Its metadata wrapper refuses both drop commits on that final mark. The unmodified production function panics with `attempt to subtract with overflow` (`review-counterexamples.log:17`). Charge the complete worst case before admitting a mark, or stop safely before each read, and keep a regression at the actual pass limit.

**Independent execution establishes both the successful routing change and the failing safety cases.** The probes live outside the target source tree in `pdca-reviewer-1014-probes/`; the safety probes reuse the patch’s real-redb/placement-aware test rig and add the missing sequences.

```sh
# From this report's directory, with the patch applied:
CARGO_TARGET_DIR="$PDCA_TARGET/target" cargo test --offline \
  --manifest-path pdca-reviewer-1014-probes/Cargo.toml \
  --test counterexamples reviewer_ -- --nocapture

CARGO_TARGET_DIR="$PDCA_TARGET/target" cargo test --offline \
  --manifest-path pdca-reviewer-1014-probes/Cargo.toml \
  --test base_discriminator -- --nocapture
```

The first command runs three tests and currently fails all three for the reasons above. The second currently passes; it failed its obligation assertion with the patch stashed, rather than failing compilation. That discriminator checks routing after production reconciliation on an empty store; it does not alone prove reclamation. The submitted 15-test suite separately exercises production cleanup/drain, Complete, unlink and overwrite with fragment-presence and fragment-absence assertions and passes independently (`review-green.log:145`).

`cargo test -p wyrd-core -p wyrd-custodian` passed 1,112 tests across 68 result groups, with one explicitly ignored privileged Tier-1 disk-fault test (`review-target-tests.log`). `cargo clippy -p wyrd-core -p wyrd-custodian --all-targets -- -D warnings`, `cargo fmt --all -- --check`, `typos`, docs lint, and docs render/link audit all passed (`review-clippy.log:165`, `review-fmt.log`, `review-typos.log`, `review-docs-lint.log:1`, `review-docs-render.log:3`). The brief’s `typos` and `docs-renderer` dependencies were actually exercised. Tier-1 disk faults and Tier-2 deployed kill/reconstruct remain follow-up observation choices for this custodian durability change, not claimed evidence from this run.

**Frozen gate evidence has specific limits; no missing wrapper is treated as a defect.** The instance-root wrapper paths are outside this leaf’s inputs. All seven named logs were readable; the full CI, mutation campaign, host feature checks and batch-review wrappers were not independently repeated.

| Gate | Adjudication | Evidence |
|------|--------------|----------|
| C4-ci | PASS, from frozen output; relevant tests and scanners also rerun | The log runs fmt, clippy, build, tests, dependency checks and DST and ends with all checks passed (`gate-logs/C4-ci.log:5025`). |
| C4-verify | Original result UNVERIFIABLE; routing red→green independently recovered | Its green leg passes 15 tests; its reverted leg fails imports before running tests (`gate-logs/C4-verify.log:10`, `:16`). The reviewer’s existing-API discriminator supplies a real assertion failure and subsequent success (`review-base-red.log:10`; `review-base-green.log:8`). |
| C4-diff-cov | Coverage unmeasured; base-selection caveat | The wrapper cannot apply the stacked patch to `origin/main` (`gate-logs/C4-diff-cov.log:10`). The supplied target applies and compiles; this does not demonstrate a patch application or compilation defect. |
| C5-mutants | Recorded survivors require interpretation | The log reports 243 mutants: 100 missed, 118 caught, 25 unviable (`gate-logs/C5-mutants.log:113`). Survivors include budget arithmetic and publication classification (`:50`, `:66`); the three executed counterexamples provide stronger defect evidence than treating every survivor as a bug. The log does not identify all eight brief-required named negations as separately executed. |
| T4-batch-review | FAIL, three distinct findings confirmed | Five reports include three duplicates of the budget bug; the remaining two and that budget bug reproduce independently (`gate-logs/T4-batch-review.log:10`; `review-counterexamples.log:28`). |
| T4-contribution | N/A | Contribution artifacts are absent by design; their substantive audit re-runs at publish (`gate-logs/T4-contribution.log:10`). No human escalation is owed for this deferral. |
| host-tikv | PASS, from frozen output | The captured TiKV crate and server feature clippy commands both finish successfully; this proves compilation, not a live TiKV deployment (`gate-logs/host-tikv.log:2`, `:211`). |

**Prior-art searching ran by all 28 affected paths, with a remaining completeness decision.** `review-prior-art.json` records each path’s main-branch commit lookup and closed issue/PR search; `review-prior-art.txt` is the compact index. The queries fetched up to five recent commits and ten closed matches per path. I also read the closed predecessor issues [#636](https://github.com/getwyrd/wyrd/issues/636) and [#637](https://github.com/getwyrd/wyrd/issues/637), consistent with the brief’s rejected routing-without-drain and published-part cleanup history (`brief.md:211`). These bounded, text-based searches do not certify every closed/unmerged patch’s changed-file list; T4 leaves that historical disposition with the human rather than asserting exhaustive absence.

The standing rubric was applied. The explicit seeded-DST deferral to #660 remains settled (`crates/custodian/src/orphan_identity.rs:99`; `AGENTS.md:215`). The migration gate is a specified persisted-state precondition, not a capability probe concealing an eager load-time cause. No DCO finding is inferred from commit inspection. The findings concern required lifecycle safety and progress within this patch’s scope.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — issue #1014 (orphan-identity gate + retirement routing)

Scope: `patch.diff` against `$PDCA_TARGET` (patch applied in the working tree). Inputs used:
`brief.md`, `check-gates.json`, `gate-logs/`. I re-ran nothing from the gate set; I built a scratch
copy of the patched tree and added one test to check finding 1 (shown below).

**Overall verdict: refuted on liveness, not on safety.** I did not find an input that makes GC
reclaim a fragment too early. I did find an ordinary production interleaving where the cleanup
never writes its marker. While the marker is missing, GC reclaims nothing at all.

## Findings

- NEEDS-HUMAN [impl] — **A delete or overwrite that lands during a cleanup pass discards the whole lap. On a store with steady deletes, the marker is never written.** `crates/custodian/src/orphan_identity.rs:251` reads the reference picture (`Index::read`) once, before the ledger pages at `:854`. Suppose an object the index saw as committed is deleted or overwritten inline (the gate is closed, so this is the normal path) before the page holding its new marks is read. Then `judge` (`:948-951`) picks `Drop` against the stale `Committed` namer. `verified` (`:966-990`) finds the inode gone or no longer naming the slot and returns `None`. `visit` (`:902-907`) then reports the mark `namer-moved` → unsettled. The lap ends with no marker (`:266`, `:287-289`), and the next pass starts a new lap from the head of the ledger. The drop was moot: under a closed gate, every inline writer (`metadata.rs` `with_retirement` inline arm, the superseding commits) rewrites the mark in the same commit that removes the reference. That is exactly the brief's "re-read and found moot" case (brief, Design → Completion), but the code never checks it. **Reproduced:** in a scratch copy of the patched tree, I added a `scan_page(ORPHAN_PREFIX, after=None)` hook to the test file's own `Store` wrapper. The hook runs one production `md::unlink` of a 1-chunk RS(2,1) object per repair pass. Over 10 passes `reconcile_step` returned `Partial` every time, `status()` stayed `Walking { unsettled: true }`, and `migration:orphan-identity` was never written. GC passes at `+100×GRACE` reclaimed **none** of the 30 deleted fragments. Once the deletes stopped, one more pass wrote the marker. Rough rate: each lap expects about (fragments per delete) × (index-relevant deletes per pass, counted between the index read and the page read) unsettled marks, regardless of ledger size. So one 1-chunk RS(6,3) delete per pass gives a lap roughly an e⁻⁹ chance of completing. The pass outcome is `Partial`, not `Blocked`, so nothing escalates. Suggested fix: on `namer-moved`, stop the window *before* that key (`return Ok((lap, false))` without advancing). The next pass then judges it against a fresh index and the lap survives. Or settle as moot when, under the closed gate, the namer was removed by a commit that also wrote the mark. Either way, add a test that lands an inline unlink/overwrite between the index read and the page read and asserts that the marker is still written within a bounded number of passes. The F5 test (`crates/custodian/tests/retirement_routing.rs:1139-1195`) only shows that one race costs one lap, so it passes either way.

- NEEDS-HUMAN [human] — **"Only delays reclamation" is not true as built. The closed gate stops all GC reclamation, and several ordinary states keep it closed with no bound.** `crates/custodian/src/gc.rs:691-695` holds every `Stamped` or `Reclaiming` mark while the gate is closed. On the base, GC reclaimed inline-unlink marks once their grace passed. The new runtime doc says the closed gate "only delays reclamation" (`docs/design/architecture/06-runtime-view.md`, added paragraph). In fact the delay has no bound in each of these cases:
  - Finding 1 on any busy store.
  - Any damaged `retire:bytes:` value, or any `Completed` session's `retire:records:` value (`orphan_identity.rs:619-626`, `:781-786`). These block the cleanup, yet the base GC never reads them, so they never stopped reclamation before.
  - After every metadata restore, until a new lap completes.

  While the gate is closed the `orphan:` ledger only grows, so each lap gets longer. The brief does ask for "GC treats every mark as protecting" (Design, Scope), so this is a fitness question for sign-off, not a code slip. Options: the closed gate could switch off only the keyed lookup and the routing (keep the base's stamp-only reclaim for inline marks); or a cleanup stuck past N laps could surface as `Blocked` or an alert rather than `Partial`.

- NEEDS-HUMAN [impl] — **Confirming T4: the per-mark read reservation is one short, and the shortfall underflows.** `READS_PER_MARK = 1 + ATTEMPTS_PER_DROP * 3 = 7` (`orphan_identity.rs:142`). But the retirement-token lookup at `:937-938` runs on **each** attempt, using an unchecked `-=`, and so does the conflict re-read at `:920`. Trace with `reads_left == 7` at the guard (`:863`): attempt 1 is token (6), `verified` 2 reads (4, via `saturating_sub` at `:1081`), conflict (3). Attempt 2 is token (2), `verified` (0), conflict: `0 - 1`. That panics under overflow checks (tests/debug). In release it wraps to `usize::MAX` (no `[profile]` override in `Cargo.toml`), which disables the per-pass read bound for the rest of the pass. Reaching it needs a structured mark whose obligation is gone, judged `Drop`, and losing both commits. Fix: reserve 8 (`ATTEMPTS_PER_DROP * 4`) or move the token lookup out of the retry loop, and use `saturating_sub` or `checked_sub` at `:920` and `:938`.

- NEEDS-HUMAN [impl] — **Confirming T4: an undecodable session record is treated as live, and that throws away the trap's evidence.** `orphan_identity.rs:540-541` maps a session that will not decode to `Phase::Unknown`. `:563` classes its parts as `Staged`, and `:1053-1054` treats `Unknown` as live, so a mark on such a part is **dropped**. Failing case: a `Completed` session publishes part 1. The inline `unlink` writes the trap mark (F3's fixture). Then the `mpu:` value is damaged. The cleanup drops the trap mark, pinned on the damaged bytes. A human repairs the record back to `Completed`, the records drain deletes `part:` without marking, and the fragments end with no mark and no reference, which GC keeps forever (`gc.rs:761`). That breaks the brief's F3 rule "No fragment is left with no mark and no reference". `Unknown` should be `Leave` (no write) or a blocker, not a drop.

- NEEDS-HUMAN [impl] — **Surviving mutants show the new test file never exercises paths that decide drop vs keep. Broken code there leaves a stale mark on a live fragment, which is the exact hazard the gate exists to prevent.** These mutants were run against the custodian test suite, `retirement_routing.rs` included (`gate-logs/C5-mutants.log`):
  - `Index::read_segments -> Ok(())` (`orphan_identity.rs:744`) and deleting the `(ChunkMap::Segmented, Some(seg))` arm of `verified` (`:981`) both survive. No test puts a stale mark on a **committed segmented** object's fragment, although routing a segmented `unlink` is new in this diff (A3).
  - Flipping the class ranking (`<` → `>`/`<=`/`==` at `:469`, `:505`, `:517`) survives. No test has one position named by two classes. The normal state right after a Complete is such a case: the inode and the published `part:` records name the same chunks, and the ranking alone decides `Drop` (committed) versus `Keep` (published).
  - The resume guard `walk.is_current(..) && !walk.is_ended()` → `false` (`:245`) and `CleanupWalk::is_current -> true` (`crates/core/src/orphan_identity.rs:167`, core-side) both survive. No test has a lap that spans more than one pass (every fixture is under `MARKS_PER_PASS`), and no test lands a restore **mid-lap**. F7 (`retirement_routing.rs:1291`) only restores after the marker exists.
  - Every pagination "next page" arm survives (`:552`, `:601`, `:629`, `:670`, `:760`, `:819`).

  The production code looks right in each spot. The finding is that the tests cannot catch it going wrong.

- NEEDS-HUMAN [impl] — **The green results are on a stale base.** `C4-diff-cov` reports "patch.diff does not apply on origin/main" (`gate-logs/C4-diff-cov.log`). The brief lists a conflict with #1013 on the drain dispatch in `reconcile_step`. The cleanup leg sits right after that dispatch and depends on its order ("after the drain", `crates/custodian/src/reconciliation.rs` cleanup leg). Rebase, then re-run CI and the new test file, before treating `C4-ci: pass` as evidence for main.

## Things I tried that did not break it

- **Gate pinning.** Routed batches `require` the marker and `mpufence` bytes (`crates/core/src/orphan_identity.rs:333-339`), so a restore between the gate read and the commit fails the commit.
- **Marker/generation soundness.** A marker's generation is never above its own image's fence, so a post-restore `next()` always invalidates it.
- **The 502-op boundary.** `fits_one_batch` counts preconditions (`metadata.rs:2214`).
- **Obligation-row drops against child-1's drain.** The drain re-stamps legacy marks (arm b) and leaves its own marks alone (arm a). A cleanup drop ahead of the drain's cursor is re-marked, and its own marks are left alone while the obligation exists.
- **F5 race safety.** The drop requires the inode bytes.

I could not find a premature reclaim in any of these.

The withheld build notes carry the brief's required "demonstrated reds". `C4-verify` is UNVERIFIABLE (the test file does not compile on the base), so the frozen gate evidence holds no red→green for this slice. Sign-off has to take those reds from build-notes on trust.

<!-- pdca:leaf-complete -->

### Advisory — code-review

- NEEDS-HUMAN [impl] — **P1: Routed retirement bypasses grace when an old pending lease survives.** `crates/core/src/metadata.rs:2311` replaces inline marks with an obligation without handling the prior chunks' `pending:` entries. A successful create followed by a crash before `write::release` (`crates/core/src/write.rs:346`) leaves those leases behind. After they expire, a routed unlink/overwrite followed by GC with `ExpiredPendingPolicy::Reclaim` reaches the unmarked expired-lease branch (`crates/custodian/src/gc.rs:732`) and deletes fragments before the drain establishes fresh grace. Preserve protection for this input; add a regression that omits release and runs GC before the drain. The new tests always use `Defer` (`crates/custodian/tests/retirement_routing.rs:586`).

- NEEDS-HUMAN [impl] — **P1: A retained reclaim intent can outlive the migration and bypass the next deletion's grace.** `crates/custodian/src/orphan_identity.rs:934` leaves every `reclaiming` mark without checking whether a live reference names its fragment, yet completion can open the gate (`crates/custodian/src/orphan_identity.rs:266`). F4 explicitly seeds that combination (`crates/custodian/tests/retirement_routing.rs:1084`). For an object large enough to route, the subsequent unlink leaves that old mark intact, and GC's resume branch (`crates/custodian/src/gc.rs:696`) deletes immediately, without testing the new obligation or grace. Keep such unresolved live reclaim intents from authorizing routing; extend F4 through a routed delete and GC before drain.

- NEEDS-HUMAN [impl] — **P1: An unreadable session is incorrectly treated as never published.** `crates/custodian/src/orphan_identity.rs:563` classifies `Phase::Unknown` as staged, and verification accepts undecodable session bytes as live (`crates/custodian/src/orphan_identity.rs:1053`). A completed upload whose object was deleted inline can still have published part records awaiting their records drain. If its session becomes unreadable, cleanup deletes the legitimate unlink marks; after session repair and records drain, the fragments have neither references nor marks and leak permanently. Pinning the corrupt bytes does not establish publication history. Preserve the marks and block uncertain classification; cover F3 with an unreadable completed session.

- NEEDS-HUMAN [impl] — **P2: The per-mark read reservation is one read short.** `crates/custodian/src/orphan_identity.rs:142` reserves seven reads, but each of two conflicting attempts can consume four: retirement-token lookup, two naming-record reads, and the conflict reread. Earlier structured marks can leave exactly seven reads available while the drop budget remains untouched. The last decrement at `crates/custodian/src/orphan_identity.rs:920` then underflows, panicking with overflow checks or wrapping the budget. Reserve eight, or avoid the unnecessary final reread and account for each attempt explicitly; test two conflicts at the budget boundary.

- NEEDS-HUMAN [impl] — **P2, efficiency: Pending obligations are expanded without an aggregate memory bound.** `crates/custodian/src/orphan_identity.rs:769` walks the entire retirement ledger and inserts every named fragment into the index, including every segmented generation via `crates/custodian/src/orphan_identity.rs:809`. This happens before reading the bounded orphan window (`crates/custodian/src/orphan_identity.rs:251`). Pagination limits each response, but all positions accumulate across pages and are rebuilt on every cleanup pass, even when few marks need judging. A large retirement backlog can exhaust custodian memory while the migration gate remains closed. Retain reference information only for the current mark window, preserving the staged/committed/obligation read order and conditional drops.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Prevent the migration from certifying unsafe restored marks or erasing published-part unlink evidence—both sequences violate the required lifetime guarantee in executable counterexamples (`crates/custodian/src/gc.rs:696`; `crates/custodian/src/orphan_identity.rs:563`; `review-counterexamples.log:6`).
- [ ] T4 Contribution — Confirm the relevant closed/rejected-work dispositions before sign-off—affected-path history and closed-work searches ran, but bounded search results cannot establish the complete rejected-diff history; the frozen batch review also has three independently confirmed defects awaiting fixes or recorded rejection (`review-prior-art.txt:1`; `gate-logs/T4-batch-review.log:10`; `AGENTS.md:221`).
- [ ] T5 Judgment — Correct the retry budget and retain regressions combining damaged-session recovery, preserved reclaiming marks with routed deletion, and two conflicts at the read limit—the current 15 tests pass despite all three failures (`crates/custodian/tests/retirement_routing.rs:1066`; `crates/custodian/src/orphan_identity.rs:863`; `review-counterexamples.log:28`).
- [ ] Validation — fitness-to-purpose — Decide whether the corrected migration is ready for restored production metadata and operational rollout—reader grace must survive reopening the gate, and the documented restore procedure must run before gateways return (`docs/design/architecture/06-runtime-view.md:96`; `review-counterexamples.log:14`).
- [ ] **A delete or overwrite that lands during a cleanup pass discards the whole lap. On a store with steady deletes, the marker is never written.** `crates/custodian/src/orphan_identity.rs:251` reads the reference picture (`Index::read`) once, before the ledger pages at `:854`. Suppose an object the index saw as committed is deleted or overwritten inline (the gate is closed, so this is the normal path) before the page holding its new marks is read. Then `judge` (`:948-951`) picks `Drop` against the stale `Committed` namer. `verified` (`:966-990`) finds the inode gone or no longer naming the slot and returns `None`. `visit` (`:902-907`) then reports the mark `namer-moved` → unsettled. The lap ends with no marker (`:266`, `:287-289`), and the next pass starts a new lap from the head of the ledger. The drop was moot: under a closed gate, every inline writer (`metadata.rs` `with_retirement` inline arm, the superseding commits) rewrites the mark in the same commit that removes the reference. That is exactly the brief's "re-read and found moot" case (brief, Design → Completion), but the code never checks it. **Reproduced:** in a scratch copy of the patched tree, I added a `scan_page(ORPHAN_PREFIX, after=None)` hook to the test file's own `Store` wrapper. The hook runs one production `md::unlink` of a 1-chunk RS(2,1) object per repair pass. Over 10 passes `reconcile_step` returned `Partial` every time, `status()` stayed `Walking { unsettled: true }`, and `migration:orphan-identity` was never written. GC passes at `+100×GRACE` reclaimed **none** of the 30 deleted fragments. Once the deletes stopped, one more pass wrote the marker. Rough rate: each lap expects about (fragments per delete) × (index-relevant deletes per pass, counted between the index read and the page read) unsettled marks, regardless of ledger size. So one 1-chunk RS(6,3) delete per pass gives a lap roughly an e⁻⁹ chance of completing. The pass outcome is `Partial`, not `Blocked`, so nothing escalates. Suggested fix: on `namer-moved`, stop the window *before* that key (`return Ok((lap, false))` without advancing). The next pass then judges it against a fresh index and the lap survives. Or settle as moot when, under the closed gate, the namer was removed by a commit that also wrote the mark. Either way, add a test that lands an inline unlink/overwrite between the index read and the page read and asserts that the marker is still written within a bounded number of passes. The F5 test (`crates/custodian/tests/retirement_routing.rs:1139-1195`) only shows that one race costs one lap, so it passes either way.
- [ ] **"Only delays reclamation" is not true as built. The closed gate stops all GC reclamation, and several ordinary states keep it closed with no bound.** `crates/custodian/src/gc.rs:691-695` holds every `Stamped` or `Reclaiming` mark while the gate is closed. On the base, GC reclaimed inline-unlink marks once their grace passed. The new runtime doc says the closed gate "only delays reclamation" (`docs/design/architecture/06-runtime-view.md`, added paragraph). In fact the delay has no bound in each of these cases:
- [ ] **Confirming T4: the per-mark read reservation is one short, and the shortfall underflows.** `READS_PER_MARK = 1 + ATTEMPTS_PER_DROP * 3 = 7` (`orphan_identity.rs:142`). But the retirement-token lookup at `:937-938` runs on **each** attempt, using an unchecked `-=`, and so does the conflict re-read at `:920`. Trace with `reads_left == 7` at the guard (`:863`): attempt 1 is token (6), `verified` 2 reads (4, via `saturating_sub` at `:1081`), conflict (3). Attempt 2 is token (2), `verified` (0), conflict: `0 - 1`. That panics under overflow checks (tests/debug). In release it wraps to `usize::MAX` (no `[profile]` override in `Cargo.toml`), which disables the per-pass read bound for the rest of the pass. Reaching it needs a structured mark whose obligation is gone, judged `Drop`, and losing both commits. Fix: reserve 8 (`ATTEMPTS_PER_DROP * 4`) or move the token lookup out of the retry loop, and use `saturating_sub` or `checked_sub` at `:920` and `:938`.
- [ ] **Confirming T4: an undecodable session record is treated as live, and that throws away the trap's evidence.** `orphan_identity.rs:540-541` maps a session that will not decode to `Phase::Unknown`. `:563` classes its parts as `Staged`, and `:1053-1054` treats `Unknown` as live, so a mark on such a part is **dropped**. Failing case: a `Completed` session publishes part 1. The inline `unlink` writes the trap mark (F3's fixture). Then the `mpu:` value is damaged. The cleanup drops the trap mark, pinned on the damaged bytes. A human repairs the record back to `Completed`, the records drain deletes `part:` without marking, and the fragments end with no mark and no reference, which GC keeps forever (`gc.rs:761`). That breaks the brief's F3 rule "No fragment is left with no mark and no reference". `Unknown` should be `Leave` (no write) or a blocker, not a drop.
- [ ] **Surviving mutants show the new test file never exercises paths that decide drop vs keep. Broken code there leaves a stale mark on a live fragment, which is the exact hazard the gate exists to prevent.** These mutants were run against the custodian test suite, `retirement_routing.rs` included (`gate-logs/C5-mutants.log`):
- [ ] **The green results are on a stale base.** `C4-diff-cov` reports "patch.diff does not apply on origin/main" (`gate-logs/C4-diff-cov.log`). The brief lists a conflict with #1013 on the drain dispatch in `reconcile_step`. The cleanup leg sits right after that dispatch and depends on its order ("after the drain", `crates/custodian/src/reconciliation.rs` cleanup leg). Rebase, then re-run CI and the new test file, before treating `C4-ci: pass` as evidence for main.
- [ ] **P1: Routed retirement bypasses grace when an old pending lease survives.** `crates/core/src/metadata.rs:2311` replaces inline marks with an obligation without handling the prior chunks' `pending:` entries. A successful create followed by a crash before `write::release` (`crates/core/src/write.rs:346`) leaves those leases behind. After they expire, a routed unlink/overwrite followed by GC with `ExpiredPendingPolicy::Reclaim` reaches the unmarked expired-lease branch (`crates/custodian/src/gc.rs:732`) and deletes fragments before the drain establishes fresh grace. Preserve protection for this input; add a regression that omits release and runs GC before the drain. The new tests always use `Defer` (`crates/custodian/tests/retirement_routing.rs:586`).
- [ ] **P1: A retained reclaim intent can outlive the migration and bypass the next deletion's grace.** `crates/custodian/src/orphan_identity.rs:934` leaves every `reclaiming` mark without checking whether a live reference names its fragment, yet completion can open the gate (`crates/custodian/src/orphan_identity.rs:266`). F4 explicitly seeds that combination (`crates/custodian/tests/retirement_routing.rs:1084`). For an object large enough to route, the subsequent unlink leaves that old mark intact, and GC's resume branch (`crates/custodian/src/gc.rs:696`) deletes immediately, without testing the new obligation or grace. Keep such unresolved live reclaim intents from authorizing routing; extend F4 through a routed delete and GC before drain.
- [ ] **P1: An unreadable session is incorrectly treated as never published.** `crates/custodian/src/orphan_identity.rs:563` classifies `Phase::Unknown` as staged, and verification accepts undecodable session bytes as live (`crates/custodian/src/orphan_identity.rs:1053`). A completed upload whose object was deleted inline can still have published part records awaiting their records drain. If its session becomes unreadable, cleanup deletes the legitimate unlink marks; after session repair and records drain, the fragments have neither references nor marks and leak permanently. Pinning the corrupt bytes does not establish publication history. Preserve the marks and block uncertain classification; cover F3 with an unreadable completed session.
- [ ] **P2: The per-mark read reservation is one read short.** `crates/custodian/src/orphan_identity.rs:142` reserves seven reads, but each of two conflicting attempts can consume four: retirement-token lookup, two naming-record reads, and the conflict reread. Earlier structured marks can leave exactly seven reads available while the drop budget remains untouched. The last decrement at `crates/custodian/src/orphan_identity.rs:920` then underflows, panicking with overflow checks or wrapping the budget. Reserve eight, or avoid the unnecessary final reread and account for each attempt explicitly; test two conflicts at the budget boundary.
- [ ] **P2, efficiency: Pending obligations are expanded without an aggregate memory bound.** `crates/custodian/src/orphan_identity.rs:769` walks the entire retirement ledger and inserts every named fragment into the index, including every segmented generation via `crates/custodian/src/orphan_identity.rs:809`. This happens before reading the bounded orphan window (`crates/custodian/src/orphan_identity.rs:251`). Pagination limits each response, but all positions accumulate across pages and are rebuilt on every cleanup pass, even when few marks need judging. A large retirement backlog can exhaust custodian memory while the migration gate remains closed. Retain reference information only for the current mark window, preserving the staged/committed/obligation read order and conditional drops.
- [ ] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1014/review-
- [ ] **Re-stamping a live part's mark does not establish the claimed identity invariant.** `brief.md:43-48` requires retaining/re-stamping that mark, then enabling routing; `brief.md:73-75` claims it now names the most recent unreference event. The brief never specifies the replacement identity or how it follows a later publication/deletion. Target `crates/custodian/src/gc.rs:756-761` checks only the obligation named by the mark, and returns unprotected for a legacy/non-token event. Counterexample to the stated procedure: migrate a stale mark on a live part, let its refreshed grace expire, publish its bytes into a large object and retire the part records, then unlink before the generation drain refreshes the mark. The mark need not name the new `g:<inode>:<version>` obligation, so GC can reclaim against the old stamp. Specify the identity handoff/protection that closes this interval. Extend (F)/(A) to assert bytes remain during the pending retirement and fresh reader grace; the eventual-absence assertions at `brief.md:49-63` also pass after premature reclamation.
- [ ] **The permitted cleanup delete can erase a concurrent unlink's only reclamation evidence.** `brief.md:178-184` permits dropping a mark based on a committed reference while keeping legacy unlink active, but requires no atomic validation of that reference and the mark. Target `crates/core/src/metadata.rs:2161-2193` removes the reference and puts its legacy mark in one batch. A cleanup can observe the committed reference, unlink can commit, and the cleanup can then delete the new mark using its obsolete observation, leaving unreferenced bytes without a mark. Require conditional cleanup writes that validate the evidence at commit, and a deterministic interleaving proving unlink/supersede wins cannot lose their marks. “Drop only against committed references” needs this transactional meaning, not merely a prior lookup.
- [ ] **Reaching the last page is insufficient evidence for the completion marker.** `brief.md:46-48,178-183` specifies interruption handling but says to write the marker when the walk finishes, without requiring a complete, trustworthy reference classification. Target `crates/custodian/src/gc.rs:1184-1188` explicitly continues walking after an unreadable inode/segment while marking the reference set incomplete; `:524-533` then refuses certification. Such a walk can finish without establishing which stale marks remain on live fragments. Define completion to exclude unresolved references and unsettled cleanup writes, with an observable blocked/retry outcome. Add a test where an unreadable inode, segment, or part prevents the marker, then repair it and prove cleanup resumes before routing enables.
- [ ] **The singleton marker has no specified validity after metadata restore.** `brief.md:23-34,178-184` describes a one-time cleanup and asks Do to choose the marker's deleter, but specifies no invalidation/revalidation trigger. The normative proposal explicitly includes “a metadata restore rewinding the ledger” among the stale-mark sources (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:1251-1259`). Target `crates/server/src/cli.rs:1483-1495` warns that restore brings back a previously complete marker; its existing restore-fence protocol opens a new generation before certifying completion (`crates/custodian/src/restore.rs:917-974`). State how this marker becomes invalid after restore, who owns that integration, and test a restored completed marker with stale marks. Otherwise the claimed migration precondition can be bypassed by restoring the marker itself, and restore integration becomes an unacknowledged addition to the stated scope.
- [ ] size backstop — this slice is behaving oversized: patch is 217 KB (threshold 100 KB); patch touches 28 files (threshold 20). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): Rebuild inside this slice; the routing change itself is red->green (reviewer-confirmed). Fix the reproduced safety defects, each with a regression test that fails today: 1. Premature reclaim (P1): a preserved `reclaiming` mark on a live fragment survives cleanup (orphan_identity.rs:934), the marker still certifies (:266), and after a routed unlink GC resumes it with no grace (gc.rs:696). Do not let such marks authorize routing or reclaim before the new obligation's grace. Extend F4 through restore -> cleanup -> routed delete -> GC before drain. 2. Permanent leak (P1): an undecodable session (Phase::Unknown, orphan_identity.rs:540/:563/:1053) is treated as staged/live and its marks are dropped. Unknown must Leave the mark or block certification, never drop. Cover F3 with a damaged Completed session. 3. Read-budget underflow (P2): READS_PER_MARK=7 is one short (orphan_identity.rs:142, :920, :938). Charge the real worst case (or hoist the token lookup out of the retry loop) and use checked/saturating subtraction; add the two-conflicts-at-the-limit regression. 4. Marker never written on a busy store: a "namer-moved" mark from an inline unlink/overwrite between the index read and the page read discards the whole lap (orphan_identity.rs:902-907). Stop the window before that key, or settle it as moot under the closed gate; test that the marker is still written within a bounded number of passes with steady deletes. Also fix: the pending-lease bypass (metadata.rs:2311 / gc.rs:732; add a test with release omitted and ExpiredPendingPolicy::Reclaim); bound memory in the retirement-ledger index walk (orphan_identity.rs:769, :809); add stale-mark cases on committed segmented objects (C5 survivors at :744, :981). Keep the brief's closed-gate design ("every mark protects"), but correct the 06-runtime-view.md wording that it "only delays reclamation". Rebase onto current main (the patch does not apply; conflicts with #1013's drain dispatch in reconcile_step) and re-run CI.
- By / date: Eduard Ralph / 2026-11-10

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 4 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
