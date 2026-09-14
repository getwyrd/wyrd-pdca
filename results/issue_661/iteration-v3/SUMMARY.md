# Result — issue 661 / gc-orphan-ledger-paged-walk

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: the `orphan:` ledger is read with **one `scan`**, in two places: GC
  (`orphan_leases`, `crates/custodian/src/gc.rs:522-537`, called at `:177`) and the post-restore
  pass (`crates/custodian/src/restore.rs:308`). `scan` fails whole past
  `SCAN_CAP = 1 << 20` (`crates/traits/src/lib.rs:286`) and returns no partial result
  (`:273-278`). One maximum segmented-object retirement installs ~1.78 M marks
  (`0016:1392-1398`; `crates/traits/src/lib.rs:1362-1368`), so a single large delete takes GC down
  on every pass from then on. The deployed loop runs GC in its own `reconcile_step` call
  (`crates/server/src/custodian.rs:587-611`), so GC stops, and the post-restore command can never
  finish. The failure seals itself: the pass that should shrink the ledger is the pass that
  cannot start. Second, a GC pass commits all of its key deletes in **one batch sized by the
  pass** (`gc.rs:180`, `:216`, `:231`), so the commit grows with the ledger. A pass that reclaims
  many marks hands the backend a transaction past its envelope (10 MB and 5 s per transaction,
  `crates/traits/src/lib.rs:1326-1331`) after it has already deleted the fragments, so their
  marks stay behind. This slice caps that commit at a constant `W`. It does **not** prove that a
  `W`-write commit fits the 5-second half on the slowest backend. That is a calibration, and leg
  B states what its test shows and what it does not.
- Success criterion: the NEW file `crates/custodian/tests/gc_ledger_walk.rs` passes. It runs
  over in-memory doubles. The metadata double's `scan` enforces a **lowered** cap: it returns
  `wyrd_traits::ScanCapExceeded` when the result would exceed the cap, shaped after
  `crates/metadata-redb/tests/scan.rs:9-19`. Its `scan_page` is implemented directly over its
  map with the shared page helpers, **not** through `wyrd_testkit::test_double_scan_page`, which
  pages over `scan` and inherits the cap (`crates/traits/src/lib.rs:1431-1433`). Its pages are
  capped below `B` (below), as a real backend's are, so one pass needs several pages to read
  its budget. The double counts the `orphan:` entries each pass receives, records any `scan`
  that reaches the `orphan:` prefix, and logs every commit. Every pass builds a fresh
  `GcContext`, exactly as the deployed loop does (`crates/server/src/custodian.rs:600-608`). Each
  test asserts on the pass's `Result` before anything else. Legs:
  **(A) GC survives a ledger past the cap.** Seed an `orphan:` population larger than the lowered
  cap, every mark actionable: fragment present, unreferenced, past grace. Every pass of
  `reconcile_step` with a `GcContext` returns `Ok`. Running passes until the ledger is empty
  reclaims every fragment and removes every consumed key. On the base the first pass returns
  `Err(ReconcileError::Store)` (the `?` at `gc.rs:177`) — the red.
  **(B) One pass reads and writes a bounded, pinned amount.** No pass receives more than `B`
  `orphan:` entries, counted across all of its pages. `B` is a named constant with its derivation
  in its doc comment, at most 65,536 (1/16 of `SCAN_CAP`). The test **hard-codes the same
  literal**. While at least `B` entries remain, a pass receives **exactly** `B`, so a budget that
  silently under-reads fails. A population `P > B` drains in exactly `⌈P / B⌉` passes, not fewer.
  No `scan` of the `orphan:` prefix happens anywhere in the step (`0016:1398-1399`: `orphan:` is
  "never read by a single scan"). The walk's own writes are bounded too. No commit it makes
  carries more than `W` writes. `W` is a named constant, at most restore's `MARK_BATCH` of 1,000
  (`crates/custodian/src/restore.rs:96-103`, the base's existing bounded commit against the same
  envelope). The test hard-codes it and asserts that no commit exceeds `W`, and that the commits
  carrying a pass's `n` key deletes number exactly `⌈n / W⌉`. So a batch that is flushed late,
  or never, fails. Round 1 left four batch-control mutants alive for want of this. **What this
  proves, and what it does not.** It proves the commit size is a constant, independent of the
  ledger and of the pass. It does not prove that a `W`-write commit finishes inside 5 s. On TiKV
  each delete takes its own lock round trip, one after another inside the transaction
  (`crates/metadata-tikv/src/lib.rs:1407-1422`), so the time depends on the deployment's
  round-trip time, and 0016 says a batch of ~1,000 small `orphan:` marks can exceed it
  (`0016:630-636`). Fitting that half is 0016's calibrated `B_ops` knob (`0016:640-643`; X98 at
  `:2627`), out of scope here. `W`'s doc comment states its byte bound (the keys are small, so
  `W` of them sit far inside 10 MB) and says plainly that the operation-count half is not
  calibrated, citing `0016:640-643`.
  **(C) The tail does not starve, and the walk wraps.** Seed a **retention-safe head** — more
  than `B` marks still inside their grace window, placed first in key order — followed by an
  **actionable tail**, head plus tail above the lowered cap. Run passes with a fresh `GcContext`
  each time. Every tail fragment is reclaimed within `⌈(head + tail) / B⌉ + 1` passes, and no
  head fragment or head mark is touched. Then age the head past grace: every head fragment is
  reclaimed within `⌈(head + tail) / B⌉ + 1` further passes. So the walk returns to the start
  of the ledger once it reaches the end. An implementation that restarts at the first key every
  pass never reaches the tail, and one that stops at the end never reaches the head again. A
  key written behind the walk's position may be missed until the next lap, and that is
  allowed; a key present throughout the walk is never skipped (`scan_page` clause 4,
  `crates/traits/src/lib.rs:1390-1398`).
  **(D) An unread mark outranks every other reason to reclaim, and no pass strips the last
  evidence from bytes it keeps.** Under `ExpiredPendingPolicy::Reclaim`:
  (i) a fragment whose chunk carries an **expired** `pending:` lease (seeded with
  `metadata::put_pending`, as `crates/custodian/tests/gc.rs:202-216` does) and whose own
  `orphan:` mark is still inside its grace survives every pass until that mark's grace elapses.
  The ledger is several windows long (a window is the up-to-`B` entries one pass reads), so in
  most passes the mark sits **outside** the window being read — ahead of it in some passes and
  behind it in others. Round 1's patch reclaimed it on the lease (three blocking findings).
  (ii) Seed a chunk under an expired lease, with unmarked fragments on several servers whose
  positions fall in different windows. Fill the ledger past the lowered cap with marks that do
  not touch that chunk. Between any two passes, every fragment of that chunk still on disk has
  its `pending:` entry or an `orphan:` mark. The entry is deleted only once no fragment remains
  for it to account for, and every fragment is reclaimed within `⌈P / B⌉ + 1` passes.
  (iii) An unreadable mark counts as a mark. Set up as (i), but the fragment's own mark (as
  `orphan_key` spells it) holds a value that is not the decimal instant `mark_orphaned` writes.
  The fragment survives every pass through two full laps of the walk. The mark's key and value
  bytes are never deleted or changed. The mark is named at least once on the GC audit seam
  (target `wyrd.custodian.gc.audit`), carrying its key, the way `emit_unreadable_pending` names
  an unreadable `pending:` value (`gc.rs:600`). Read the audit lines back as
  `crates/custodian/tests/gc.rs:895-918` and `:1056-1063` do, with the #214 global-default guard
  (`:1143-1146`) installed first. Only this leg seeds an unreadable mark, so no other test in the
  binary fires that callsite. The base treats such a value as no mark at all (`gc.rs:527-533`),
  so its fragment falls to the expired-lease arm (`:204-206`) and the cleanup deletes the mark
  (`:216`). A paged walk that keeps that behaviour fails here.
  On the base all three cases error at the first pass (the ledger is past the cap) — the red.
  **(E) Only a fragment's own key licenses a reclaim.** `parse_orphan_key` reads each field as a
  plain integer (`crates/core/src/metadata.rs:78-85`), so `orphan:5:01:0` and `orphan:5:1:0`
  decode to the same position. Every writer spells the key through `orphan_key`
  (`metadata.rs:72-74`). Seed a fragment whose own mark (as `orphan_key` spells it) is inside
  grace, and a differently spelled key for the same position carrying an old stamp, placed in
  an earlier window (more than `B` keys before the mark; chunk ids between the two spellings,
  such as `orphan:5:10:0` onwards, sort between them). The fragment survives until its own
  mark's grace elapses. The differently spelled key is never deleted, rewritten or acted on. On
  an ordered backend the base's single scan happens to keep this fragment, because the key that
  sorts later wins in its map (`gc.rs:532`). A paged walk that judges an early window alone
  would not. On the base the leg errors past the cap — the red. #800 applies the same rule to
  its sweep.
  **(F) Restore survives the same ledger, and never re-stamps a mark it did not read.**
  `reconcile_after_restore` over a store whose `orphan:` population exceeds the lowered cap
  returns `Ok`. Restore writes a fresh stamp for any stranded fragment it did not find marked
  (`restore.rs:413-416`, the `put` at `:426-429`). So a restore that judged "already marked" from
  part of the ledger would silently restart the grace clock of every mark beyond it. Seed
  pre-marked stranded fragments whose marks sort **after** both the first `B` `orphan:` keys and
  the first lowered-cap's worth, one of them the ledger's last key, each stamped older than the
  pass's clock, and one more whose mark holds an unreadable value, as in D(iii). Every one keeps
  its value bytes unchanged and is counted in `RestoreReport::already_marked`, the field for a
  fragment that "already carried an `orphan:` record" (`restore.rs:112-114`). The base would
  re-stamp the unreadable one, since its read drops the value (`restore.rs:413-429`). A genuine
  stray with no mark **is** marked, so the leg cannot pass on a pass that did nothing. On the
  base `restore.rs:308` returns `Err` — the red.
  **(G) Seeded DST for the paged walk**, appended to the **existing** `crates/dst/tests/custodian.rs`
  (`#![cfg(madsim)]`, `:53` — keep that attribute, and add no new DST file). The rubric requires
  it: "a new destructive or concurrent path lands with seeded Tier-0 DST coverage", and round 1
  drew three findings for its absence. GC passes, each with a fresh `GcContext`, walk an
  `orphan:` ledger several **pages** long over the simulated-TiKV store, with a page cap the seed
  picks. The ledger stays below `B`: `B` is a production constant, and a test cannot lower it
  without a context field or a global, both ruled out below. A concurrent task unlinks further
  objects at a seed-chosen instant, writing fresh marks (inside grace) at positions both ahead
  of and behind the walk's current page. Afterwards: no fragment that is referenced, or whose
  own mark is inside grace, has been deleted, and every fragment actionable at the start has
  been reclaimed by the end of the run. A coverage leg proves the mid-walk landing is reached
  rather than assumed, as
  `prop_restore_two_readings_cover_the_divergence_window` does (`:2132-2165`). It runs under
  `cargo xtask ci` → `run_dst` (`xtask/src/main.rs:1575-1616`). Record the seed count in
  `build-notes.md`.
  **(H) `cargo xtask ci` green.**
- Repo + branch target: getwyrd/wyrd @ main
- Scope: GC's and restore's reads of the `orphan:` ledger, and GC's reclaim decision that
  consumes its read. (1) GC reads at most `B` entries per pass and resumes where the last pass
  stopped, across the per-pass rebuild of `GcContext` the deployed loop does
  (`crates/server/src/custodian.rs:600-608`), returning to the start at the end of the ledger.
  (2) What a pass may conclude from a partial read: legs D and E. That includes what an
  unreadable mark means, because leg D rewrites the expired-lease arm that decides it: a mark
  value that does not parse counts as a mark. Its fragment is kept, neither GC nor restore
  deletes or rewrites the mark, and GC names it on the audit seam, as ADR-0045 decision 3
  requires of a decode site (legs D(iii) and F). Today GC skips it silently, so its fragment
  looks unmarked and falls to an expired lease (`gc.rs:527-533`, `:204-206`, `:216`), and
  restore re-stamps it (`restore.rs:413-429`). (3) The walk's own writes commit in batches of at
  most `W`, never one batch sized by the pass. (4) Restore's "already marked" judgement sees
  every existing mark, with a footprint bounded by a constant. Keep the safety gate
  (`gc.rs:191`), the grace test (`:196-203`) and the conservative arm (`:207-211`) judging
  exactly as they do. The expired-lease arm between them (`:204-206`) is the one leg D changes:
  it may fire only where the fragment has no mark, read or unread. **Writes keep the base's shape.**
  This slice adds no conditional-write-and-retry scheme for `orphan:` marks. Round 2's scheme
  produced both of its findings outside the sweep, and it moves to #800 with the sweep. No
  conclusion about a mark may rest on a commit's `Conflict`, which says only that a
  precondition lost (`crates/traits/src/lib.rs:1461-1465`). Whether the walk also survives a
  leader change is Do's call, stated in `build-notes.md`. Must NOT change `reconcile_step`'s or
  `reconcile_after_restore`'s signature, and must NOT add a field to `GcContext` (`gc.rs:72-82`),
  to any other context, or to `RestoreReport`. The test builds them with struct literals, so
  either change breaks its base compile and turns every red into UNVERIFIABLE. Decoding is
  unchanged: marks remain the bare decimal `mark_orphaned` writes (`gc.rs:117-129`). Two existing
  tests assume restore's whole-ledger scan and may be adjusted, keeping their purpose.
  `crates/custodian/tests/segmented_map_restore.rs:642-656` poisons the `orphan:` scan to prove a
  record already known unreadable is named before a later read fails: keep its `pending:` leg
  and the property. The DST restore campaign's timing (`crates/dst/tests/custodian.rs:1784-1789`,
  coverage leg `:2132-2165`) assumes the pass's two `inode:` readings are three hops apart with
  the `orphan:` scan between them: retune the timing only, never the invariants it asserts.
  Docs currency (`AGENTS.md:154-157`): a persisted continuation is a new persisted record, so
  describe it and the paged walk in `docs/design/architecture/06-runtime-view.md` §6.7 step 2
  (`:74`). / out of scope: the sweep of marks with no fragment, the late-write deadline and its
  relation to the grace window, and deletes that must lose to a refreshed mark (#800);
  calibrating `W` against the 5-second half of the envelope on the slowest backend (0016's
  `B_ops`, `0016:640-643`); decoding or repairing an unreadable mark value (this slice only keeps
  and names it); the staged reference set, reclamation intent, the three `orphan:` value shapes and keyed retire
  protection (#662); the orphan-identity migration gate (X92, #659); pre-marking in
  reconstruction and rebalance (#723); the `pending:` and `inode:` scans (`gc.rs:503`,
  `referenced_fragments`), which are other namespaces; `scrub.rs`, `reconstruction.rs`,
  `rebalance.rs`, `desired_state.rs`, `crates/server/src/*`; any edit to 0016 or an ADR.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (8 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 95.8% — 161 of 168 instrumentable changed lines executed (floor 80%); 168 of 430 changed lines were instru
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 47 mutants tested in 89s: 2 missed, 26 caught, 18 unviable, 1 timeouts

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_661/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.07s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Reviewing issue #661’s bounded, resumable `orphan:`-ledger walk for GC and restore: red→green holds, but one shape defect and one test-fidelity gap remain.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The required decision boundary is explicit: page both ledger readers, bound each GC pass and cleanup commit, preserve partial-read safety, and leave the orphan sweep and five-second calibration out of scope (`brief.md:212`). |
| C2 Reproduction (red pre-fix) | PASS | The defect is reproducible rather than hypothetical — stashing production changes left the new test compilable and all eight cases failed by runtime assertion on the base (`gate-logs/C4-verify.log:15`). |
| C3 Change | PASS | The patch stays on the scoped data-lifecycle surfaces and documents the new persisted cursor; the production entry signatures and context/report shapes remain unchanged (`crates/custodian/src/gc.rs:115`, `crates/custodian/src/restore.rs:281`, `docs/design/architecture/06-runtime-view.md:76`). |
| C4 Verification (red→green) | PASS | The exact test independently reproduced 8 red on the stashed base and 8 green after restore; the reviewer’s full gate rerun stopped only on a read-only advisory-DB lock at `cargo deny`, while frozen CI completed all checks including the seeded DST (`gate-logs/C4-verify.log:10`, `gate-logs/C4-verify.log:84`, `gate-logs/C4-ci.log:3513`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Add an expired-lease/within-grace regression whose own mark is exactly the persisted lower cursor boundary — otherwise the safety-significant `>`→`>=` survivor authorizes a destructive reclaim undetected; the other miss only weakens defense against a backend violating the exclusive-cursor contract (`crates/custodian/src/gc.rs:745`, `crates/custodian/src/gc.rs:795`, `gate-logs/C5-mutants.log:14`). |
| T1 Structure | PASS | The continuation and paging logic remain behind the existing `MetadataStore` seam, with no concrete-backend dependency or new crate, and the persisted-field architecture documentation is current (`crates/custodian/src/gc.rs:52`, `docs/design/architecture/06-runtime-view.md:76`). |
| T2 Shape | FAIL | Restore’s “already marked” lookup allocates `wanted` and `marked` collections proportional to the candidate population, so a restore where candidates cover the ledger still materializes O(ledger) state instead of the constant-bounded footprint required by the brief (`crates/custodian/src/gc.rs:829`, `brief.md:223`). |
| T3 Runtime | PASS | The supported runtime evidence is green: the 16-test simulated-TiKV custodian suite exercises both new concurrent-walk properties, and the TiKV feature arms compile (`gate-logs/C4-ci.log:3447`, `gate-logs/host-tikv.log:207`). |
| T4 Contribution | N/A | Contribution artifacts are absent by design at Check; the mandatory publish-time gate owes their substantive audit (`gate-logs/T4-contribution.log:10`). |
| T5 Judgment | PASS | Exact-path prior-art checks covered merged history and all 5 open/13 closed-unmerged PRs for every affected file; no open overlap exists, closed #647 was the sole rejected overlap, and the frozen multi-pass review reported no separate finding (`gate-logs/T4-batch-review.log:10`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether a fixed 1,000-write batch supported by in-memory/simulated evidence is fit for deployment — the slowest backend’s five-second transaction envelope remains explicitly uncalibrated (`crates/custodian/src/gc.rs:94`). |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Add an expired-lease/within-grace regression whose own mark is exactly the persisted lower cursor boundary — otherwise the safety-significant `>`→`>=` survivor authorizes a destructive reclaim undetected; the other miss only weakens defense against a backend violating the exclusive-cursor contract (`crates/custodian/src/gc.rs:745`, `crates/custodian/src/gc.rs:795`, `gate-logs/C5-mutants.log:14`).
- [ ] Validation — fitness-to-purpose — Decide whether a fixed 1,000-write batch supported by in-memory/simulated evidence is fit for deployment — the slowest backend’s five-second transaction envelope remains explicitly uncalibrated (`crates/custodian/src/gc.rs:94`).
- [ ] The brief silently narrows the tracker rather than implementing the defect the tracker assigns to this slice. The only tracker record says “`orphan_leases` **and the mark sweep** walk the `orphan:` ledger with `scan_page`,” and `notes.json:1` has `comments:[]`; nevertheless `brief.md:10-15` and `brief.md:200-201` move the sweep to #800. There is no supplied tracker comment or dependency-state record that establishes that re-scope. Restore the sweep to this brief or make the split/authority resolvable in the tracker inputs.
- [ ] The claimed transaction-envelope fix has no falsifiable success gate. `brief.md:28-31` promises to prevent commits from exceeding both 10 MB and 5 s, but `brief.md:32-60` tests only an in-memory double and a count of at most `W` writes. The target contract explicitly says it sets no batch-size limit and inherits backend-native limits (`crates/traits/src/lib.rs:1326-1335`), while the cited design says the operation cap must be calibrated on the slowest supported backend (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:640-642`). A green count assertion cannot establish the 5-second claim; the brief needs a resolved/calibrated `W` plus a deterministic validation command, or must narrow the promised outcome to what the count test proves.
- [ ] The scope contains an independent malformed-record/telemetry fix that is not caused by pagination and is absent from the tracker acceptance. `brief.md:176-178` requires malformed marks to be surfaced, while the base silently ignores malformed keys or values even when the ledger is below the cap (`crates/custodian/src/gc.rs:526-533`). That behavior exists independently of the oversized-scan defect in `notes.json:1`; split it or explicitly add it to the tracker-backed problem and acceptance surface.
- [ ] A load-bearing ordering conflict from the tracker was dropped. `notes.json:1` says “#637's plan declares a load-bearing conflict with #625 (#625 widens `reconcile_step`'s signature; 637 builds first, 625 builds on it) — all five 637 slices precede #625,” but `brief.md:160-168` lists only conflict #722 and declares no unmerged prerequisite. The affected signature is still the public seven-argument function at `crates/custodian/src/reconciliation.rs:103-112`. Reinstate #625's ordering/conflict or cite a supplied record that resolves it.
- [ ] Two load-bearing verification citations do not exist on the declared target despite `brief.md:3-8` saying every citation was re-verified there. `brief.md:153` grounds C-1 in `docs/principles.md`, and `brief.md:141` plus `brief.md:213-218` ground RED classification/reversion behavior in `engine/scripts/run-verify.sh`; neither path exists on `origin/main` at the target commit `605b33a`. Replace them with target-resident evidence or remove the claims that depend on them.
- [ ] size backstop — this slice is behaving oversized: patch is 104 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.
- [ ] C1 Spec — Decide whether this slice must enforce `W_repoint`/`W_write` before enabling fragment-less sweeping or defer that sweep — both live maintenance writers still authorize destination writes with no deadline, so a finite `D` is not yet a sound boundary (`crates/custodian/src/reconstruction.rs:934`; `crates/custodian/src/rebalance.rs:534`).

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Rebuild to close the C5 causal-adequacy gap: the surviving mutant on the expired-lease/within-grace boundary (gc.rs:745, gc.rs:795, a > vs >= flip) is not caught by any current test, and that boundary is safety-relevant (a wrong boundary authorizes a destructive reclaim). Add a regression whose own mark sits exactly at the persisted lower cursor boundary so the mutant is killed. Other §6 items (sweep moved to #800 without a tracker comment, dropped #625 ordering conflict, bad doc citations, transaction-envelope claim wording, malformed-record scope, rebalance/reconstruction W_repoint scope, size backstop) are tracker-authority/scope/process questions, not implementation defects, and are explicitly out of scope for this iterate-do. Oversize flag is waived per human instruction.
- By / date: Eduard Ralph / 2026-09-12

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 5 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
