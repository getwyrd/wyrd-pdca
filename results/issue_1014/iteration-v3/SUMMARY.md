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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 323 mutants tested in 69m: 103 missed, 188 caught, 32 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 17 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1014/review
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.48s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of #1014’s orphan-identity migration gate and unlink/supersede retirement routing: revisions are needed to prevent premature reclamation, lost retirement evidence, and cleanup stalls.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief defines observable grace, completion, restore, and reclamation requirements, including the prior rejected failure modes; `brief.md:33`, `brief.md:338`, `brief.md:355`. |
| C2 Reproduction (red pre-fix) | PASS | An independently compiled, base-compatible routing test failed after stashing the patch: zero obligations and 900 inline marks instead of one obligation and zero marks; `reviewer-red-green.log:18`. |
| C3 Change | PASS | The changed production surfaces and living architecture documentation fit the authorized migration/routing slice; implementation defects are adjudicated below; `crates/core/src/metadata.rs:2150`, `crates/custodian/src/reconciliation.rs:174`, `docs/design/architecture/05-building-block-view.md:258`. |
| C4 Verification (red→green) | FAIL | Routing has an executed red→green and the supplied 30 tests pass, but independent fixed-tree probes reproduce premature reclaim, silent retirement completion, and stalled progress; `reviewer-probes.log:14`, `reviewer-scan-cap.log:7`; this verdict is not based on stale coverage or the host’s missing gitlink. |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Repair the reproduced cursor and retirement-input defects before relying on the marker for reader grace or complete reclamation; R1–R4 below, `crates/core/src/orphan_identity.rs:248`, `crates/core/src/metadata.rs:2277`. |
| T1 Structure | PASS | Core owns the durable marker/conditional batch and custodian owns reconciliation through the existing metadata seam; no new backend dependency, clock source, or shared mutable global was introduced; `crates/core/src/orphan_identity.rs:335`, `crates/custodian/src/reconciliation.rs:174`. |
| T2 Shape | FAIL | A checkpoint accepted as structurally valid can sort beyond every canonical ledger key and authorize completion without examining stale marks; `crates/core/src/orphan_identity.rs:248`, `crates/custodian/src/orphan_identity.rs:307`. |
| T3 Runtime | FAIL | Cleanup can repeat an oversized-key window forever, exceed the inode scan cap, and retain namers proportional to overlapping backlog rather than window size; R4–R6, `crates/core/src/orphan_identity.rs:196`, `crates/custodian/src/orphan_identity.rs:691`, `crates/custodian/src/orphan_identity.rs:909`. |
| T4 Contribution | NEEDS-HUMAN | Complete the affected-file merged/closed/rejected prior-art review — the supplied target has one synthetic commit and no remote/archive, so the brief’s issue-level note cannot establish that check; `reviewer-prior-art.log:1`, `brief.md:211`; contribution-artifact auditing is deferred to publish. |
| T5 Judgment | NEEDS-HUMAN | Decide the scope and ordering of the unresolved steady-state repoint/stale-mark problem — the production repoint seam reproduced premature reclaim after migration, so one-time cleanup does not establish an enduring invariant; `brief.md:355`, `crates/custodian/src/reconstruction.rs:1232`, `reviewer-repoint.log:7`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether rollout/restore operations can tolerate an indefinitely closed gate and whether the eventual repair/rebalance resolution establishes reader grace in deployment; automated checks establish neither operational acceptance nor that unresolved design premise; `docs/design/architecture/06-runtime-view.md:96`, `crates/custodian/src/gc.rs:799`. |

Source citations above and below resolve under `$PDCA_TARGET`; brief, log, and reviewer-harness citations resolve in this report’s directory. The target contains the patch applied to its disposable base. No production source was edited; the stash/pop check restored all affected source and documentation bytes exactly.

The strongest evidence is a reproduced reader-safety failure, followed by retirement and progress failures. The following findings are consolidated rather than counted once per repeated batch-review report.

- **R1 — P1: a damaged cursor opens the gate over stale marks.** `decode_cleanup_walk` checks only prefix and maximum length (`crates/core/src/orphan_identity.rs:248`). `{"after":"6f727068616e3a7e"}` decodes to `orphan:~`, sorts past every canonical mark, and is accepted. The empty window then permits the completion marker (`crates/custodian/src/orphan_identity.rs:288`, `crates/custodian/src/orphan_identity.rs:307`). In the probe, a live 100-chunk object retained an ancient mark, cleanup certified completion, production `unlink` installed its obligation, and GC reclaimed fragment `(server 0, chunk 171000, index 0)` before the drain marked it (`reviewer-probes.log:14`). Reject checkpoints that cannot safely represent a ledger position and add this end-to-end regression; the existing `ff` test does not cover an invalid key inside the prefix.

- **R2 — P1: segmented routing discards the evidence needed to detect a missing segment.** The new route stores only `map.group()`, dropping the root’s segment table without validating its rows (`crates/core/src/metadata.rs:2277`). The existing drain treats an exhausted range as complete (`crates/custodian/src/retirement/marks.rs:1100`). I published a segmented object with production Complete, completed migration, removed segment zero, then unlinked and drained it. The root and retirement obligation disappeared, but fragment `(server 0, chunk 130000, index 0)` survived without a retirement mark even after grace (`reviewer-probes.log:22`). Preserve a detectable obligation/error for missing expected rows before removing the root; this affects overwrite through the same helper.

- **R3 — P2: a malformed flat placement becomes an undrainable retirement.** Routing copies the flat chunks without checking the placement condition required by the drain (`crates/core/src/metadata.rs:2316`; `crates/custodian/src/retirement/marks.rs:1074` shows the same committed-placement rule on segment records). A 100-chunk RS(6,3) fixture with one eight-server placement was successfully unlinked, but its generation obligation remained after eight reconciliation passes (`reviewer-probes.log:18`). The flat drain also applies `Rule::Committed` (`crates/custodian/src/retirement/marks.rs:224`). Refuse or explicitly handle the undrainable input before removing its live inode, and cover supersede too.

- **R4 — P2: truncating an oversized key can prevent all forward progress.** `CleanupWalk::advanced` persists only the first `ORPHAN_KEY_MAX_LEN` bytes (`crates/core/src/orphan_identity.rs:196`), while the next scan is strictly after that prefix. With 16,385 malformed keys sharing that prefix, each 16,384-entry window is reread; three passes still report `Walking` (`reviewer-probes.log:26`). The separate ordering probe also confirms that the stored cursor sorts before the key just judged (`reviewer-probes.log:7`). Use a progress representation that advances safely or report the obstruction explicitly instead of an endless partial lap.

- **R5 — P2: the new cleanup cannot finish above the backend’s inode scan cap.** `read_committed` uses whole-prefix `scan`, even for an empty orphan window (`crates/custodian/src/orphan_identity.rs:645`, `crates/custodian/src/orphan_identity.rs:909`). With the real Redb backend’s supported cap lowered to two and three empty committed inodes, a reconciliation pass fails with `ScanCapExceeded` for `inode:` (`reviewer-scan-cap.log:7`). The production cap is 1,048,576 (`crates/traits/src/lib.rs:286`). Page the new reference walk; an empty repair queue otherwise avoids reconstruction’s existing inode scan (`crates/custodian/src/reconstruction.rs:248`), so it does not explain away this new migration failure.

- **R6 — P2: the promised namer-memory bound does not hold.** `Index::place` pushes every matching record into `namers` before deciding whether it improves the retained position (`crates/custodian/src/orphan_identity.rs:691`); `hold_stronger` updates only the index and never removes discarded namers (`crates/custodian/src/orphan_identity.rs:1060`). `place_whole` does likewise (`crates/custodian/src/orphan_identity.rs:720`). Thus one marked position named by N overlapping obligations retains N namers, despite a fixed mark window. This is source-derived, not an observed OOM. Retain only the bounded winning evidence and aggregate class information, and test overlapping records rather than only disjoint chunks.

**The steady-state decision remains owed.** This was explicitly left undecided in the prior sign-off (`brief.md:355`), not deferred to a settled tracking issue. Existing committed reconstruction and rebalance mark displaced positions but do not clear destination marks (`crates/custodian/src/reconstruction.rs:1232`, `crates/custodian/src/rebalance.rs:642`); the completed cleanup returns immediately (`crates/custodian/src/orphan_identity.rs:269`). I exercised production `metadata::repoint_chunk` and its returned commit, with the displaced-source mark that maintenance adds, then production unlink and GC. A stale mark at destination server 6 reclaimed chunk 180000 before the new drain marked it (`reviewer-repoint.log:7`). This exercises the production repoint seam, not the entire reconstruction driver. The human must settle scope/ordering for preserving the invariant after migration; the builder should not silently choose that policy.

**Verification distinguishes executed evidence from gate limitations.** The retained harness is runnable from this directory:

```sh
CARGO_TARGET_DIR="$PDCA_TARGET/target" cargo test --offline --manifest-path reviewer-harness/Cargo.toml --test routing_base
CARGO_TARGET_DIR="$PDCA_TARGET/target" cargo test --offline --manifest-path reviewer-harness/Cargo.toml --test probes reviewer_ -- --nocapture
```

The first test uses only pre-existing APIs and a raw completion-marker fixture. It passed with the patch, failed its runtime assertion with the patch stashed, and passed after restoration (`reviewer-red-green.log:18`, `reviewer-red-green.log:86`). The second command intentionally exposes the findings above; it does not change production code. The supplied integration suite independently passed all 30 tests (`target/reviewer-green.log`). Formatting, typos, docs lint, and docs rendering/link audit also passed locally. Both dependencies named in `brief.md:153` were actually exercised.

**Host caveat:** the independent `cargo xtask ci` completed with two failed steps caused by the supplied snapshot omitting `.github/advisory-db` from its Git index: the index-pin integration test and the advisory-database check (`reviewer-ci.log:3873`, `reviewer-ci.log:4523`). Formatting, clippy/build, docs, repository guards, conformance, and the DST run completed successfully; the workspace test command stopped at that index-pin test. This is a snapshot limitation for the human to consider alongside the frozen successful CI run, not a patch defect and not the reason for C4’s FAIL. No Git index or production file was changed to bypass it.

Frozen gate evidence was read for every configured row:

- **C4-ci:** the original full gate reports all checks passed (`gate-logs/C4-ci.log:5038`). The independent full rerun has the snapshot caveat stated above.
- **C4-verify:** its green leg ran 30 tests; its original red leg never ran a test because the new module imports were absent (`gate-logs/C4-verify.log:10`, `gate-logs/C4-verify.log:16`). The separate base-compatible routing probe supplies an executed red, but does not imply that every requested mutation was demonstrated.
- **C4-diff-cov:** coverage was not measured because the patch did not apply to the gate’s `origin/main` (`gate-logs/C4-diff-cov.log:10`). This is a base/ordering caveat, not a compile or correctness defect in the supplied applied target.
- **C5-mutants:** the log records 323 tested, 103 missed, 188 caught, and 32 unviable (`gate-logs/C5-mutants.log:116`). The 69-minute mutation campaign was not repeated. Counts alone do not establish 103 defects; the independent failures above establish specific missed behavior.
- **T4-batch-review:** its 17 entries include repeated findings (`gate-logs/T4-batch-review.log:10`). R1–R6 are grounded independently. The blanket expired-lease pause at `crates/custodian/src/gc.rs:496` is an observable conservative tradeoff that can defer unrelated garbage while any retirement remains; operational acceptance belongs with validation, not a claim of premature deletion.
- **T4-contribution: N/A.** The artifacts are intentionally drafted after Check; the substantive audit must rerun at publish (`gate-logs/T4-contribution.log:10`). Nothing is missing for a human to waive at this stage.
- **host-tikv:** the captured command successfully type-checked both the TiKV backend and server feature selection (`gate-logs/host-tikv.log:2`, `gate-logs/host-tikv.log:211`). This is compile evidence, not a live TiKV topology run.

The optional-capability/load-time symptom-guard smell test does not independently fire: this patch’s migration gate is the specified durable correctness precondition. The unresolved continuing source of stale marks is addressed explicitly in T5 instead.

The seeded cleanup-race work is already deferred to #660 (`crates/custodian/src/orphan_identity.rs:129`, `brief.md:143`); it is settled for this review and is not re-raised as a missing-DST finding. Tier-1 disk-fault and Tier-2 kill/reconstruct follow-up are warranted for the durable cursor and retirement transitions; neither was exercised here. No `INTEGRATION.md` was supplied or found in the target, so no additional human-only checklist is inferred. Prior-art searches were attempted using every affected path, but the single synthetic commit cannot supply merged history or closed/rejected work (`reviewer-prior-art.log:6`).

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — issue 1014 (orphan-identity gate + retirement routing), iteration 3

I built the post-fix tree in a scratch copy (cargo 1.96 was available) and ran the new test file,
my own probe tests, and hand-applied mutations of the production code. Probe tests were added only
to the scratch copy. Every red below was re-run after a forced rebuild, and the tree passed 30/30
again afterwards.

## Refutations that landed

- NEEDS-HUMAN [impl] — **A damaged walk cursor still writes the marker without judging a single mark, and the result is a premature reclaim.** `crates/core/src/orphan_identity.rs:248` checks only the `orphan:` prefix and the length bound, so `{"after":"6f727068616e3a7e"}` (`orphan:~`; `~` sorts after every digit) decodes. The unit test at `crates/core/src/orphan_identity.rs:502-505` even asserts that `orphan:\xff` counts as "inside". The lap resumes there (`crates/custodian/src/orphan_identity.rs:278-283`), the window comes back empty, and the marker is written (`:307-313`). Probe, run with the harness from `retirement_routing.rs`: a live 100-chunk object with one ANCIENT legacy mark, plus that cursor. One repair pass returns `Changed`, status is `Complete`, and the stale mark is still there. After the routed `unlink`, with the drain frozen, three GC-only passes **reclaimed that fragment before the drain marked it**. This is the exact property sign-off 2, item 1 asked for. F6 (`crates/custodian/tests/retirement_routing.rs:1888-1906`) only tries the literal `ff`, so it passes. Fix: refuse any cursor that is not a canonical `orphan_key`. One catch: GC's own comment (`crates/custodian/src/gc.rs:2114-2119`) says a non-canonical key already in the ledger can legitimately become the cursor, so the lap must then record the last canonical key it judged. Residual: even a canonical cursor past the last real mark (e.g. `orphan:99:…`) skips everything. If damage of this kind is in the threat model, the lap needs an integrity check beyond grammar.

- NEEDS-HUMAN [impl] — **A routed delete of a segmented object with one missing `seg:` row succeeds silently and leaks that row's fragments for good.** `segmented_gate` (`crates/core/src/metadata.rs:2244-2262`) reads only the gate. `with_retirement` then retires `RetiredMap::Segmented(map.group().clone())` (`:2277`), which drops the root's segment table, the only record of which rows should exist. The drain can't tell that a row is missing. Probe: gate open, a production Complete of 2×90 RS(2,1) chunks (2 rows), one `seg:` row deleted, then `md::unlink`. The unlink returns `Committed`, the drain finishes, and GC settles past 10× the grace period: **474 of 474 fragments of that row are still on the fleet, 0 marked, root gone.** Before the delete, GC saw `SegmentAbsent` and refused to certify (a loud signal); the delete turns that into a silent leak. That breaks the rubric's "absent entries: explicit error, never silent success" rule and the brief's (A3) "fragments absent". Fix: resolve or count the rows (`segment_count`) before routing, and refuse (`SegmentedMapUnsupported`, or a typed error) when they don't match. This confirms T4's `metadata.rs:2277` item with an executed case.

- NEEDS-HUMAN [impl] — **Three of the four rules that should make a draining mark block the marker have no test.** The rule at `crates/custodian/src/orphan_identity.rs:1210-1214` (the fix for sign-off 2, item 2) is supposed to block when the mark's position is also named by another obligation, a live session or an undecodable session. I replaced `|| index.foreign.contains(&slot)` (`:1211`) with `false`, and the whole `wyrd-custodian` and `wyrd-dst` suite still passed (434/434). The C5 survivors on the custodian suite point the same way:
  - `:1062` `|=`→`&=`: a position named by both a published part and a committed map collapses to "no class", so the mark reads as not live.
  - `:1212` `||`→`&&` and `:1214`.
  - `:459-462`: `|`→`&` drops Staged or Unknown from `LIVE`.

  Only the committed-map case is tested (`f4_a_draining_mark_on_a_live_fragment…`). The case these checks exist for (reasoned, not executed): a mark whose event's obligation is present but names other chunks, sitting on a fragment that only a Complete's superseded-generation obligation names. Without the check, the lap certifies. Once the first obligation drains, GC judges the mark by its old stamp and reclaims the fragment before the superseded generation's drain marks it. Add one F4 case each for the other-obligation, live-session and undecodable-session rows, run through GC before the drain.

- NEEDS-HUMAN [impl] — **The gate pin on routed writes is claimed but never tested.** `crates/core/src/metadata.rs:2163-2165` and `crates/core/src/orphan_identity.rs:322-341` say a routed batch requires the marker's and the fence record's exact bytes, so a restore's new generation landing mid-write makes the commit fail. I made `OpenGate::pinned` return its batch with no preconditions, and **all 1,130 `wyrd-core` + `wyrd-custodian` tests still passed**. The live risk is small: the runbook runs the post-restore pass with writers stopped (`crates/server/src/cli.rs:1482-1494`). But either add a race test (open a generation between `read_gate` and `commit`, expect `Conflict`) or stop claiming the property.

- NEEDS-HUMAN [impl] — **The read-budget regression can't fail for the reason it names.** `f_two_lost_drops_at_the_read_budget_never_run_it_out` (`crates/custodian/tests/retirement_routing.rs:1470-1516`) checks only that the mark goes eventually. `Pass::read` saturates (`crates/custodian/src/orphan_identity.rs:1371-1373`), so an overrun is invisible. That's why the C5 mutants at `:1111` (`<`→`==`, `<`→`<=`, `||`→`&&`) survive the custodian suite. The test's own doc ("with 7 reads left the window stops before it, with 8 it spends exactly those") is never asserted. Count reads in the `Store` wrapper and assert the cap. This is the rubric's "an assertion that can pass while the property fails". It's low severity now that the subtraction saturates.

- NEEDS-HUMAN [human] — **Repair and rebalance can still create stale marks after the marker exists. Sign-off 2 left this undecided, and the living docs don't mention it.** Committed-path reconstruction (`crates/custodian/src/reconstruction.rs:1233-1238`) and evacuation (`crates/custodian/src/rebalance.rs:640-646`) put a legacy mark on each vacated position. Neither checks or clears a mark at the destination. The staged path does both (`crates/custodian/src/reconstruction/staged.rs:640-650`, `:727-735`). Concrete path: fragment (c,i) is evacuated from server 5 to server 2, which writes `orphan:5:c:i`. Within grace, rebalance moves it back to 5. Now (5,c,i) is referenced again and carries the old mark. A later routed delete lets GC reclaim it on that old stamp before the drain marks it, which truncates a concurrent streaming GET, the case grace exists to prevent (`crates/server/src/lib.rs:698`). On the base, the inline unlink re-stamps the mark, so this is a regression the routing introduces. Proposal 0016 (`0016:1252-1256`) calls the stale population "bounded rather than steady-state", and `docs/design/architecture/06-runtime-view.md:96` names only "an older custodian, or a metadata restore". I can't see `build-notes.md`, so I can't confirm it was recorded as sign-off 2 required. Someone needs to decide: fix it here (clear or re-stamp a destination mark in the move's commit), defer it with an issue, or document it.

## Attempted to refute; could not

- **The red→green evidence holds even though C4-verify can't run it.** C4-verify reports UNVERIFIABLE (exit 77) because the test imports the new modules (`gate-logs/C4-verify.log`). So I broke the production code by hand, once for each of the brief's required reds, and each one turned its target test red:
  - drain dispatch removed (`reconciliation.rs:169-173`): A2, A3 ×2, F1, F2 fail (16 in all).
  - batch count ignoring preconditions: `a2_the_boundary_counts_preconditions` fails.
  - trap mark dropped: both F3 tests fail.
  - staged mark kept: F2 fails.
  - drop committed without its pins: F5 fails.
  - marker written whatever the lap met: F6 and five other blocking tests fail.
  - marker read without its generation: F7 fails.

  The tests exercise the production path (`reconcile_step`, `md::unlink`, `write::commit_overwrite`, the production Complete), not a copy.
- The C5 "MISSED" lines for `crates/core/src/metadata.rs` and `crates/core/src/orphan_identity.rs` (e.g. `fits_one_batch`, `read_gate` `delete !`) ran only core's 6 s suite, because cargo-mutants scopes tests to the mutated crate. The custodian suite catches the ones I re-checked. Don't read them as gaps. The custodian-side survivors cited above are real.
- Other attacks that failed:
  - **Expired-lease deferral race:** GC reads staged, then references, then expired leases, then the `retire:bytes:` probe, then the mark window (`crates/custodian/src/gc.rs:437-505`). A routed delete on either side of the probe is covered.
  - **A drop under an obligation the drain has already passed:** the drain re-stamps legacy and foreign marks with its own event (`crates/custodian/src/retirement/marks.rs:44-47`), so the token check leaves them.
  - **A fence generation of 0:** it can't decode (`NonZeroU64`, `crates/core/src/multipart.rs:2191-2193`).
  - **A marker brought back by a restore image:** it can't match, because the post-restore pass always opens the image's generation + 1.
  - **The F5 race against inline unlink and overwrite:** the drop pins the inode bytes, and the test goes red without the pins.

<!-- pdca:leaf-complete -->

### Advisory — code-review

- NEEDS-HUMAN [impl] — **P1: A damaged cursor can certify an unexamined ledger.** `crates/core/src/orphan_identity.rs:248` accepts `{"after":"6f727068616e3aff"}` (`orphan:\xff`): it satisfies the prefix/length checks but sorts after every canonical orphan key. The empty window then permits marker publication at `crates/custodian/src/orphan_identity.rs:307`, leaving stale live-fragment marks available for premature reclamation after a routed delete. The unit test even accepts this cursor at `crates/core/src/orphan_identity.rs:502`. Reject this unsafe resume state and extend the damaged-cursor regression through deletion and GC.

- NEEDS-HUMAN [impl] — **P1: Segmented retirement loses evidence of missing segments.** `crates/core/src/metadata.rs:2277` retains only the group, discarding the root's segment table without resolving it. With the gate already open, unlink/overwrite can therefore remove a root whose expected segment row is missing. The drain treats the remaining range's end as completion (`crates/custodian/src/retirement/marks.rs:1101`) and removes the obligation without marking the missing row's fragments. Preserve enough expected extent to detect holes, or refuse the transition while the root cannot be resolved; test a missing middle/final segment after migration.

- NEEDS-HUMAN [human] — **P1: Resolve the outstanding repair/rebalance prerequisite for routing.** The new obligation path at `crates/core/src/metadata.rs:2317` stops refreshing marks inline, while cleanup permanently short-circuits after its marker (`crates/custodian/src/orphan_identity.rs:274`). A fragment moved away from a server and later rebuilt there can retain the earlier displacement mark: committed reconstruction only marks displaced positions (`crates/custodian/src/reconstruction.rs:1233`). Once that old stamp expires, a routed delete lets GC reclaim before the new drain marks, violating reader grace. This is the brief's explicitly undecided carry-forward: decide the scope/prerequisite for safe destination adoption before relying on one-time cleanup.

- NEEDS-HUMAN [impl] — **P2: Cleanup cannot complete above the inode scan cap.** `crates/custodian/src/orphan_identity.rs:909` uses whole-prefix `scan` on every window, including an empty orphan ledger. Redb rejects more than `SCAN_CAP` entries (`crates/metadata-redb/src/lib.rs:133`), so such a store repeatedly fails cleanup and never enables segmented deletion or bounded retirement routing. Reuse the checked page walk for inodes; exercise this with a reduced backend scan cap.

- NEEDS-HUMAN [impl] — **P2: The supposedly bounded index retains discarded namers.** `crates/custodian/src/orphan_identity.rs:691` appends every matching record before `hold_stronger` chooses the winner; losing/replaced entries remain in `namers`. Many obligations naming one marked position therefore consume memory proportional to the entire backlog, despite the fixed window. `place_whole` has the same issue at line 720. Retain only the selected evidence plus class/foreign-obligation flags, and test many overlapping obligations rather than only disjoint positions.

- NEEDS-HUMAN [impl] — **P2: Truncated cursors can prevent all forward progress.** `crates/core/src/orphan_identity.rs:198` truncates oversized ledger keys to 73 bytes. With 16,385 malformed keys sharing those first 73 bytes, each 16,384-entry window ends with the same persisted prefix, which sorts before every entry; every subsequent pass rereads that identical window. The migration never completes. Preserve a strictly advancing resume position or surface an explicit blocked outcome instead of silently repeating the window; add a multi-window malformed-key regression.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Repair the reproduced cursor and retirement-input defects before relying on the marker for reader grace or complete reclamation; R1–R4 below, `crates/core/src/orphan_identity.rs:248`, `crates/core/src/metadata.rs:2277`.
- [ ] T4 Contribution — Complete the affected-file merged/closed/rejected prior-art review — the supplied target has one synthetic commit and no remote/archive, so the brief’s issue-level note cannot establish that check; `reviewer-prior-art.log:1`, `brief.md:211`; contribution-artifact auditing is deferred to publish.
- [ ] T5 Judgment — Decide the scope and ordering of the unresolved steady-state repoint/stale-mark problem — the production repoint seam reproduced premature reclaim after migration, so one-time cleanup does not establish an enduring invariant; `brief.md:355`, `crates/custodian/src/reconstruction.rs:1232`, `reviewer-repoint.log:7`.
- [ ] Validation — fitness-to-purpose — Decide whether rollout/restore operations can tolerate an indefinitely closed gate and whether the eventual repair/rebalance resolution establishes reader grace in deployment; automated checks establish neither operational acceptance nor that unresolved design premise; `docs/design/architecture/06-runtime-view.md:96`, `crates/custodian/src/gc.rs:799`.
- [ ] **A damaged walk cursor still writes the marker without judging a single mark, and the result is a premature reclaim.** `crates/core/src/orphan_identity.rs:248` checks only the `orphan:` prefix and the length bound, so `{"after":"6f727068616e3a7e"}` (`orphan:~`; `~` sorts after every digit) decodes. The unit test at `crates/core/src/orphan_identity.rs:502-505` even asserts that `orphan:\xff` counts as "inside". The lap resumes there (`crates/custodian/src/orphan_identity.rs:278-283`), the window comes back empty, and the marker is written (`:307-313`). Probe, run with the harness from `retirement_routing.rs`: a live 100-chunk object with one ANCIENT legacy mark, plus that cursor. One repair pass returns `Changed`, status is `Complete`, and the stale mark is still there. After the routed `unlink`, with the drain frozen, three GC-only passes **reclaimed that fragment before the drain marked it**. This is the exact property sign-off 2, item 1 asked for. F6 (`crates/custodian/tests/retirement_routing.rs:1888-1906`) only tries the literal `ff`, so it passes. Fix: refuse any cursor that is not a canonical `orphan_key`. One catch: GC's own comment (`crates/custodian/src/gc.rs:2114-2119`) says a non-canonical key already in the ledger can legitimately become the cursor, so the lap must then record the last canonical key it judged. Residual: even a canonical cursor past the last real mark (e.g. `orphan:99:…`) skips everything. If damage of this kind is in the threat model, the lap needs an integrity check beyond grammar.
- [ ] **A routed delete of a segmented object with one missing `seg:` row succeeds silently and leaks that row's fragments for good.** `segmented_gate` (`crates/core/src/metadata.rs:2244-2262`) reads only the gate. `with_retirement` then retires `RetiredMap::Segmented(map.group().clone())` (`:2277`), which drops the root's segment table, the only record of which rows should exist. The drain can't tell that a row is missing. Probe: gate open, a production Complete of 2×90 RS(2,1) chunks (2 rows), one `seg:` row deleted, then `md::unlink`. The unlink returns `Committed`, the drain finishes, and GC settles past 10× the grace period: **474 of 474 fragments of that row are still on the fleet, 0 marked, root gone.** Before the delete, GC saw `SegmentAbsent` and refused to certify (a loud signal); the delete turns that into a silent leak. That breaks the rubric's "absent entries: explicit error, never silent success" rule and the brief's (A3) "fragments absent". Fix: resolve or count the rows (`segment_count`) before routing, and refuse (`SegmentedMapUnsupported`, or a typed error) when they don't match. This confirms T4's `metadata.rs:2277` item with an executed case.
- [ ] **Three of the four rules that should make a draining mark block the marker have no test.** The rule at `crates/custodian/src/orphan_identity.rs:1210-1214` (the fix for sign-off 2, item 2) is supposed to block when the mark's position is also named by another obligation, a live session or an undecodable session. I replaced `|| index.foreign.contains(&slot)` (`:1211`) with `false`, and the whole `wyrd-custodian` and `wyrd-dst` suite still passed (434/434). The C5 survivors on the custodian suite point the same way:
- [ ] **The gate pin on routed writes is claimed but never tested.** `crates/core/src/metadata.rs:2163-2165` and `crates/core/src/orphan_identity.rs:322-341` say a routed batch requires the marker's and the fence record's exact bytes, so a restore's new generation landing mid-write makes the commit fail. I made `OpenGate::pinned` return its batch with no preconditions, and **all 1,130 `wyrd-core` + `wyrd-custodian` tests still passed**. The live risk is small: the runbook runs the post-restore pass with writers stopped (`crates/server/src/cli.rs:1482-1494`). But either add a race test (open a generation between `read_gate` and `commit`, expect `Conflict`) or stop claiming the property.
- [ ] **The read-budget regression can't fail for the reason it names.** `f_two_lost_drops_at_the_read_budget_never_run_it_out` (`crates/custodian/tests/retirement_routing.rs:1470-1516`) checks only that the mark goes eventually. `Pass::read` saturates (`crates/custodian/src/orphan_identity.rs:1371-1373`), so an overrun is invisible. That's why the C5 mutants at `:1111` (`<`→`==`, `<`→`<=`, `||`→`&&`) survive the custodian suite. The test's own doc ("with 7 reads left the window stops before it, with 8 it spends exactly those") is never asserted. Count reads in the `Store` wrapper and assert the cap. This is the rubric's "an assertion that can pass while the property fails". It's low severity now that the subtraction saturates.
- [ ] **Repair and rebalance can still create stale marks after the marker exists. Sign-off 2 left this undecided, and the living docs don't mention it.** Committed-path reconstruction (`crates/custodian/src/reconstruction.rs:1233-1238`) and evacuation (`crates/custodian/src/rebalance.rs:640-646`) put a legacy mark on each vacated position. Neither checks or clears a mark at the destination. The staged path does both (`crates/custodian/src/reconstruction/staged.rs:640-650`, `:727-735`). Concrete path: fragment (c,i) is evacuated from server 5 to server 2, which writes `orphan:5:c:i`. Within grace, rebalance moves it back to 5. Now (5,c,i) is referenced again and carries the old mark. A later routed delete lets GC reclaim it on that old stamp before the drain marks it, which truncates a concurrent streaming GET, the case grace exists to prevent (`crates/server/src/lib.rs:698`). On the base, the inline unlink re-stamps the mark, so this is a regression the routing introduces. Proposal 0016 (`0016:1252-1256`) calls the stale population "bounded rather than steady-state", and `docs/design/architecture/06-runtime-view.md:96` names only "an older custodian, or a metadata restore". I can't see `build-notes.md`, so I can't confirm it was recorded as sign-off 2 required. Someone needs to decide: fix it here (clear or re-stamp a destination mark in the move's commit), defer it with an issue, or document it.
- [ ] **P1: A damaged cursor can certify an unexamined ledger.** `crates/core/src/orphan_identity.rs:248` accepts `{"after":"6f727068616e3aff"}` (`orphan:\xff`): it satisfies the prefix/length checks but sorts after every canonical orphan key. The empty window then permits marker publication at `crates/custodian/src/orphan_identity.rs:307`, leaving stale live-fragment marks available for premature reclamation after a routed delete. The unit test even accepts this cursor at `crates/core/src/orphan_identity.rs:502`. Reject this unsafe resume state and extend the damaged-cursor regression through deletion and GC.
- [ ] **P1: Segmented retirement loses evidence of missing segments.** `crates/core/src/metadata.rs:2277` retains only the group, discarding the root's segment table without resolving it. With the gate already open, unlink/overwrite can therefore remove a root whose expected segment row is missing. The drain treats the remaining range's end as completion (`crates/custodian/src/retirement/marks.rs:1101`) and removes the obligation without marking the missing row's fragments. Preserve enough expected extent to detect holes, or refuse the transition while the root cannot be resolved; test a missing middle/final segment after migration.
- [ ] **P1: Resolve the outstanding repair/rebalance prerequisite for routing.** The new obligation path at `crates/core/src/metadata.rs:2317` stops refreshing marks inline, while cleanup permanently short-circuits after its marker (`crates/custodian/src/orphan_identity.rs:274`). A fragment moved away from a server and later rebuilt there can retain the earlier displacement mark: committed reconstruction only marks displaced positions (`crates/custodian/src/reconstruction.rs:1233`). Once that old stamp expires, a routed delete lets GC reclaim before the new drain marks, violating reader grace. This is the brief's explicitly undecided carry-forward: decide the scope/prerequisite for safe destination adoption before relying on one-time cleanup.
- [ ] **P2: Cleanup cannot complete above the inode scan cap.** `crates/custodian/src/orphan_identity.rs:909` uses whole-prefix `scan` on every window, including an empty orphan ledger. Redb rejects more than `SCAN_CAP` entries (`crates/metadata-redb/src/lib.rs:133`), so such a store repeatedly fails cleanup and never enables segmented deletion or bounded retirement routing. Reuse the checked page walk for inodes; exercise this with a reduced backend scan cap.
- [ ] **P2: The supposedly bounded index retains discarded namers.** `crates/custodian/src/orphan_identity.rs:691` appends every matching record before `hold_stronger` chooses the winner; losing/replaced entries remain in `namers`. Many obligations naming one marked position therefore consume memory proportional to the entire backlog, despite the fixed window. `place_whole` has the same issue at line 720. Retain only the selected evidence plus class/foreign-obligation flags, and test many overlapping obligations rather than only disjoint positions.
- [ ] **P2: Truncated cursors can prevent all forward progress.** `crates/core/src/orphan_identity.rs:198` truncates oversized ledger keys to 73 bytes. With 16,385 malformed keys sharing those first 73 bytes, each 16,384-entry window ends with the same persisted prefix, which sorts before every entry; every subsequent pass rereads that identical window. The migration never completes. Preserve a strictly advancing resume position or surface an explicit blocked outcome instead of silently repeating the window; add a multi-window malformed-key regression.
- [ ] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 17 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1014/review
- [ ] **Re-stamping a live part's mark does not establish the claimed identity invariant.** `brief.md:43-48` requires retaining/re-stamping that mark, then enabling routing; `brief.md:73-75` claims it now names the most recent unreference event. The brief never specifies the replacement identity or how it follows a later publication/deletion. Target `crates/custodian/src/gc.rs:756-761` checks only the obligation named by the mark, and returns unprotected for a legacy/non-token event. Counterexample to the stated procedure: migrate a stale mark on a live part, let its refreshed grace expire, publish its bytes into a large object and retire the part records, then unlink before the generation drain refreshes the mark. The mark need not name the new `g:<inode>:<version>` obligation, so GC can reclaim against the old stamp. Specify the identity handoff/protection that closes this interval. Extend (F)/(A) to assert bytes remain during the pending retirement and fresh reader grace; the eventual-absence assertions at `brief.md:49-63` also pass after premature reclamation.
- [ ] **The permitted cleanup delete can erase a concurrent unlink's only reclamation evidence.** `brief.md:178-184` permits dropping a mark based on a committed reference while keeping legacy unlink active, but requires no atomic validation of that reference and the mark. Target `crates/core/src/metadata.rs:2161-2193` removes the reference and puts its legacy mark in one batch. A cleanup can observe the committed reference, unlink can commit, and the cleanup can then delete the new mark using its obsolete observation, leaving unreferenced bytes without a mark. Require conditional cleanup writes that validate the evidence at commit, and a deterministic interleaving proving unlink/supersede wins cannot lose their marks. “Drop only against committed references” needs this transactional meaning, not merely a prior lookup.
- [ ] **Reaching the last page is insufficient evidence for the completion marker.** `brief.md:46-48,178-183` specifies interruption handling but says to write the marker when the walk finishes, without requiring a complete, trustworthy reference classification. Target `crates/custodian/src/gc.rs:1184-1188` explicitly continues walking after an unreadable inode/segment while marking the reference set incomplete; `:524-533` then refuses certification. Such a walk can finish without establishing which stale marks remain on live fragments. Define completion to exclude unresolved references and unsettled cleanup writes, with an observable blocked/retry outcome. Add a test where an unreadable inode, segment, or part prevents the marker, then repair it and prove cleanup resumes before routing enables.
- [ ] **The singleton marker has no specified validity after metadata restore.** `brief.md:23-34,178-184` describes a one-time cleanup and asks Do to choose the marker's deleter, but specifies no invalidation/revalidation trigger. The normative proposal explicitly includes “a metadata restore rewinding the ledger” among the stale-mark sources (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:1251-1259`). Target `crates/server/src/cli.rs:1483-1495` warns that restore brings back a previously complete marker; its existing restore-fence protocol opens a new generation before certifying completion (`crates/custodian/src/restore.rs:917-974`). State how this marker becomes invalid after restore, who owns that integration, and test a restored completed marker with stale marks. Otherwise the claimed migration precondition can be bypassed by restoring the marker itself, and restore integration becomes an unacknowledged addition to the stated scope.
- [ ] size backstop — this slice is behaving oversized: patch is 303 KB (threshold 100 KB); patch touches 28 files (threshold 20). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): Re-plan (and likely split) this slice instead of a 4th rebuild. Three rounds each reproduced NEW data-safety defects, logic grew to ~130 KB (was ~102 KB), and the brief leaves design questions open. The re-plan must turn the round-3 builder-level problems into explicit brief requirements, each with a test that fails today: 1. Damaged sweep cursor (P1): `{"after":"6f727068616e3a7e"}` (orphan:~) or orphan:\xff decodes, the lap sees an empty window and certifies the marker; a routed unlink then lets GC reclaim a live fragment before the drain marks it (core/src/orphan_identity.rs:248, custodian/src/orphan_identity.rs:278-313). Accept only canonical orphan keys and record the last canonical key judged (mind gc.rs:2114-2119); regression through routed unlink -> GC before drain. 2. Segmented delete with a missing seg: row (P1): routing keeps only map.group() and drops the segment table (metadata.rs:2277); unlink commits and the row's fragments leak forever. Check the rows against segment_count before routing and refuse with a typed error; cover unlink and overwrite, missing middle and final segment. 3. Malformed flat placement becomes an undrainable obligation (metadata.rs:2316 vs retirement/marks.rs:224): refuse or handle before removing the live inode; cover supersede. 4. Truncated cursor never advances (core/src/orphan_identity.rs:196-198): use a strictly advancing resume position or report a blocked outcome; multi-window malformed-key regression. 5. Inode read uses a whole-prefix scan (custodian/src/orphan_identity.rs:645, :909) and fails past SCAN_CAP: page it; test with a lowered backend cap. 6. Unbounded namers (custodian/src/orphan_identity.rs:691, :720, :1060): keep only the winning evidence plus class flags; test overlapping obligations. 7. Tests that cannot fail: the foreign/live-session/undecodable-session blocking rules (:1210-1214) need one F4 case each; the gate pin on routed writes (metadata.rs:2163, OpenGate::pinned) needs a Conflict race test or the claim removed; the read-budget test must count reads and assert the cap (:1111, :1371). Also resolve in the brief the open plan-advisory items: what a re-stamped mark's identity is, conditional (commit-time-validated) cleanup deletes vs a concurrent unlink, what "sweep complete" requires when references are unreadable, and how the marker is invalidated after a metadata restore. The repair/rebalance steady-state stale-mark question (reconstruction.rs:1232, rebalance.rs:642) was not decided at this sign-off; the re-plan must decide it explicitly (fix in scope or a prerequisite issue), not leave it to Do. Suggested split: (a) sweep + marker (cursor, paging, memory bound, completion, restore invalidation); (b) routed delete for flat and segmented objects, depending on (a).
- By / date: Eduard Ralph / 2026-11-10

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 4 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
