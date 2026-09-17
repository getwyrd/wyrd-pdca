# Result — issue 662 / staged-reference-set-and-reclaim-intent

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: staged bytes have no protection class, and GC destroys bytes before recording
  that it is doing so.
  1. **Staged bytes are unprotected.** `ReferenceSet` holds committed placements only
     (`crates/custodian/src/gc.rs:265-340`). A committed part's fragments (`part:` records) and an
     upload's in-flight fragments (owned `sidx:` entries, #772) are in no protected set. GC
     reclaims one as soon as it carries an `orphan:` mark past grace (`gc.rs:191-217`). Restore
     gates on the same predicate (`crates/custodian/src/restore.rs:383`), and since #772 moved
     owned entries out of `pending:`, its `pending:` skip (`restore.rs:420-424`) no longer sees
     them either. So restore marks a live upload's fragments stranded, and the next GC pass
     deletes them.
  2. **Nothing protects the bytes of a retirement still being drained.** A pending
     `retire:bytes:` obligation (`crates/core/src/multipart.rs:1059`, record types from #771)
     protects nothing today.
  3. **GC destroys first and records second.** It calls `delete_fragment` (`gc.rs:214`) before
     committing the key delete (`gc.rs:231`). An adoption CAS preconditioned on a pre-mark's
     original bytes can therefore still succeed after the fragment is gone (`0016:1300-1322`).
  4. **Only one mark format decodes.** The ledger reads only a bare decimal (`gc.rs:526-535`).
     0016's two structured shapes (`0016:1189-1204`, `:1323-1336`) would be misread or dropped.
- Success criterion: the NEW file `crates/custodian/tests/staged_protection.rs` passes over
  in-memory doubles. Session, part and owned-entry records are seeded as the bytes the base
  decoders accept: `SessionRecord` and `PartRecord` have no writer-side constructor
  (`crates/core/src/multipart.rs:2005-2008`, `:2370`), so seed raw JSON in the shape
  `crates/core/tests/multipart_session_records.rs:81-145` builds, and round-trip each fixture
  through `decode_session_record` / `decode_part_record` / `decode_owned_entry` to prove it is
  valid. Legs:
  **(A) GC protects both staged classes, with the evidence present.** Seed an `Open` session
  with a committed `part:` record (fragment `F1`) and an in-flight owned `sidx:` entry
  (fragment `F2`), both placed on D-server doubles. Give each an `orphan:` mark aged past grace,
  then run `reconcile_step` with a `GcContext`: both survive. The mark is what makes the leg
  bite: without it GC's conservative arm keeps an unmarked fragment anyway (`gc.rs:206-210`).
  On the base both are reclaimed — the red.
  **(B) Restore protects them through the same predicate, and the loss it prevents is
  shown.** `reconcile_after_restore` over the same store, with no marks, writes no `orphan:` key
  for `F1` or `F2`, and `RestoreReport::stranded_marked` does not count them. Then advance past
  grace and run GC: both survive. On the base restore marks them and GC deletes them — the red.
  The separate `staged_skipped` counter is child-4's; do not assert it here.
  **(C) Source before destination, for both handoffs.** 0016 makes the read order
  `sidx:` → `part:` → committed inodes normative (`0016:782-800`). A build that reads a
  destination before its source can see a chunk in neither class. Drive each handoff with a
  store double that performs it atomically *between* the builder's two reads: it fires after the
  first read of either range for that session completes, whichever range comes first.
  (i) The part commit: one batch deletes the chunk's `sidx:` entry and writes the `part:`
  record. (ii) The publication: the committed inode naming the chunk is written, and the `part:`
  record is removed as the records drain would remove it, both between the builder's two reads.
  X67 is this variant. In both,
  the fragment is marked and past grace, and GC must not reclaim it. On the base the chunk is
  unprotected throughout, so it is reclaimed — the red.
  **(D) The staged build is bounded per session, never a global scan.** With the double's
  lowered `scan` cap, seed more sessions-with-parts than a global `scan("part:")` could hold
  (0016's `SCAN_CAP / MAX_PARTS_PER_SESSION` row, `0016:890`), each session's own ranges below
  the cap. `reconcile_step` still succeeds, and the double sees no `scan` of the `part:` or
  `sidx:` prefix as a whole. This guards the design and may be green on the base.
  **(E) A pending byte retirement protects its fragments, by keyed lookup (X97).** Seed a mark in
  the structured shape `{orphaned_at_millis, event}`, where `event` is a `RetireToken`'s
  canonical string (`crates/core/src/multipart.rs:1352-1368`). Its fragment is unreferenced and
  past grace. While `retire:bytes:<event>` is present the fragment survives. Delete that key and
  the next pass reclaims it. The protection is one keyed read per candidate, never the `retire:`
  namespace read as a range (`0016:1226-1247`): the double records any `scan` / `scan_page` of
  `retire:`, and the test asserts there is none. On the base the structured value does not
  decode, so the fragment is never reclaimed, and the second half is the red.
  **(F) Reclamation is recorded before destruction (`0016:1312-1336`).** Four cases:
  (i) the double **errors** on the commit that records reclaim intent, and the fragment is
  still present after the pass; (ii) the mark's bytes **change** between GC's read and its
  intent commit, so that commit is a `Conflict`, and that one fragment survives while the other
  fragments in the same pass are still reclaimed (v1's fallback path for a lost CAS had no
  test); (iii) the double's `delete_fragment` hook attempts an adoption CAS
  `require(orphan:<pos> == <the bytes GC read>)` at the instant GC deletes, and it gets
  `Conflict`; (iv) restart: a mark already in the `reclaiming` shape over a present fragment is
  finished on the next pass — the fragment deleted exactly once, then the key — with no second
  grace test, even when its stamp is recent. On the base (i) and (iii) show the fragment deleted
  first, and (iv)'s value does not decode — the reds.
  **(G) All three value shapes decode, and none is rejected (`0016:1189-1204`).** Seed one mark
  of each shape: legacy bare decimal (what `mark_orphaned` writes, `gc.rs:117-129`),
  `{orphaned_at_millis, event}`, and `{orphaned_at_millis, event?, reclaiming: true}`. GC
  honours each one's meaning. Restore treats all three as already marked, and their bytes are
  unchanged after a restore pass: a legacy value is never rewritten on read (the
  `already_marked` property). A value that decodes as **none** of the three fails closed: GC and
  restore leave it byte-identical, never reclaim its fragment, and surface it on the durability
  audit seam (ADR-0045, `docs/design/adr/0045-metadata-validation-boundaries.md:55-59`).
  **(H) `cargo xtask ci` green.**
- Repo + branch target: getwyrd/wyrd @ main
- Scope: the staged protection class in the shared reference set, built from the
  committed `part:` records and owned `sidx:` entries of the sessions listed under `mpu:`. It
  uses bounded per-session ranges in source-before-destination order, and is exposed as its own
  member alongside committed placements. The protection predicate every destructive pass shares
  honours it. It must never cover less than 0016's set; covering every session's records
  whatever its state is acceptable, because it only keeps more. Also: keyed protection while a
  byte retirement is pending; the reclaim-intent ordering and its restart; the three `orphan:`
  value shapes, their dual-format decoding, and fail-closed handling of a value that matches
  none. The codec belongs where #659's drain and child-3's re-place can both reach it.
  `mark_orphaned`'s legacy output is unchanged. Must NOT change `reconcile_step`'s or
  `reconcile_after_restore`'s signature, and must NOT add a field to any context struct or to
  `RestoreReport` (the base-compiling test builds them with struct literals). Docs currency
  (`AGENTS.md:154-157`: new persisted value shapes): describe the staged protection class and the
  `orphan:` value shapes in `docs/design/architecture/08-crosscutting-concepts.md`, extending
  what #635 wrote there. / out of scope: the orphan-identity migration gate and its cleanup
  pass (X92, `0016:1249-1273` — it guards #659's retirement paths, and no identity-carrying
  mark exists before #659); drain status, rebalance, restore's counters and session fence
  (child-4); scrub and reconstruction (child-3); the ledger walk's paging (child-1, #661 — keep
  its rules intact); the sweep of marks with no fragment (#800, split out of #661 on
  2026-09-13, which builds on this slice); `desired_state.rs`, `rebalance.rs`,
  `scrub.rs`, `reconstruction.rs`; any edit to 0016 or an ADR.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (13 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 95.9% — 471 of 491 instrumentable changed lines executed (floor 80%); 491 of 1157 changed lines were instr
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 85 mutants tested in 2m: 1 missed, 40 caught, 44 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 6 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_662/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 12.85s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Reviewing issue #662: protect multipart-staged and retiring fragments from GC, make orphan-mark decoding fail closed, and durably record reclaim intent before fragment deletion.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief fixes the safety invariant, A–H acceptance legs, base-only red oracle, external tools, scope exclusions, and forbidden API changes precisely enough to distinguish success from conservative leakage (`brief.md:30`). |
| C2 Reproduction (red pre-fix) | PASS | My stash-based run independently produced 13/13 assertion failures on the pre-fix production tree, matching the frozen red evidence and its concrete data-loss assertions (`gate-logs/C4-verify.log:15`). |
| C3 Change | PASS | The patch changes exactly the planned data surfaces: the shared reference builder reads staged sources before committed destinations, and reclaim uses an exact-value intent CAS before deletion (`crates/custodian/src/gc.rs:540`, `crates/custodian/src/gc.rs:1254`). |
| C4 Verification (red→green) | PASS | Reapplying the patch made the same 13/13 tests green; frozen CI also exercised `typos`, docs rendering, the full Rust/DST suites, TiKV feature compilation, and 95.9% instrumentable diff coverage (`gate-logs/C4-verify.log:10`, `gate-logs/C4-ci.log:11`, `gate-logs/C4-diff-cov.log:637`, `gate-logs/host-tikv.log:207`). |
| C5 Causal adequacy | PASS | The fix removes the missing shared protection/ordering causes rather than adding a capability probe; the sole surviving mutant only shrinks intent batches to size one and does not weaken correctness (`crates/custodian/src/gc.rs:466`, `crates/custodian/src/gc.rs:695`, `gate-logs/C5-mutants.log:13`). |
| T1 Structure | PASS | The reusable orphan-value codec is in core metadata, while staged discovery and reclamation stay in the custodian shared builder with no concrete-backend dependency (`crates/core/src/metadata.rs:301`, `crates/custodian/src/gc.rs:540`). |
| T2 Shape | PASS | The three mark shapes have one canonical encoding with absent optional fields omitted, and the staged set remains disjoint without changing either public reconciliation signature or report/context layout (`crates/core/src/metadata.rs:207`, `crates/custodian/src/gc.rs:408`, `crates/custodian/src/restore.rs:289`). |
| T3 Runtime | PASS | Production entry points exercise the shared predicate, while seeded DST reaches both staged handoffs and both adoption outcomes under simulated interleavings (`crates/custodian/src/reconciliation.rs:123`, `crates/dst/tests/custodian.rs:3193`, `crates/dst/tests/custodian.rs:3205`). |
| T4 Contribution | FAIL | Restore reports `staged.unresolvable` but never `staged.malformed`, so corrupt staged placement can be silently protected while `needs_human()` and `is_clean()` return false/true respectively; the #800 reclaiming-mark reports are settled out of scope, and the artifact subcheck is N/A until its mandatory publish rerun (`crates/custodian/src/restore.rs:206`, `crates/custodian/src/restore.rs:320`, `brief.md:147`, `gate-logs/T4-contribution.log:10`). |
| T5 Judgment | NEEDS-HUMAN | Confirm the affected-path prior-art result across merged plus closed/rejected work — the supplied self-contained target has one base commit and no remote refs, so that collision check cannot be mechanically settled here and matters for this stacked change. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the operator-facing restore verdict is fit for sign-off after the malformed-staged reporting gap is repaired — automation must not certify a store clean while a durability fault is only being silently held (`crates/custodian/src/restore.rs:192`). |

### Advisory — adversary

# Adversarial review — issue 662 (staged protection class + reclaim intent)

**Verdict: I could not refute the fix.** The red→green evidence holds, the tests drive the
production `reconcile_step` / `reconcile_after_restore`, and every targeted break I tried was
caught. Five follow-ups below. Two of them are the T4 gate's six blocking findings (they reduce
to two issues), with my read on each.

## What I tried and could not break

- **Re-ran the proof** on a scratch copy of `$PDCA_TARGET`: `staged_protection` is 13/13 red on the
  base (every one an assertion panic, no compile error) and 13/13 green with the fix — same as
  `gate-logs/C4-verify.log`.
- **Ran the four new DST properties (the seeded madsim simulation tests) against the base.**
  `gc_reclaim_reaches_both_adoption_outcomes` hits "the adoption published a placement naming a
  fragment GC deleted" at 1 half-ms, and both staged-build properties fail. So the DST legs can
  tell good code from bad; they don't pass by construction.
- **Hand-made probes the mutation gate cannot generate**, each caught by the leg built for it:
  `part:` read before `sidx:` → C(i) red; staged build moved after the `inode:` scan → C(ii) red;
  a batch `Conflict` treated as all-lost (no per-intent retry) → F(ii) red; `superseded` never set
  → F(ii) red; `reclaiming` marks put through the grace test again → F(iv) and G red; a global
  `scan("part:")` filtered down to the session → D red.
- **Stricter legacy parser** (rejects `+5`, `05`): every in-tree `orphan:` writer puts
  `u64::to_string()` (`crates/core/src/metadata.rs:2140`, `:2248`, `:2321`;
  `crates/custodian/src/gc.rs:209`; `restore.rs:466`; `rebalance.rs:544`; `reconstruction.rs:944`),
  so no real mark becomes unreadable.
- **Key/record edges:** `scan("mpu:")` cannot return the `mpuctl` singleton
  (`crates/core/src/multipart.rs:1128-1132`). An owned entry whose owner disagrees with its key
  fails `decode_owned_entry` (`multipart.rs:3748-3752`) but its key still names the chunk, so the
  chunk is held whole rather than skipped.
- **Evidence framing, not a defect:** leg D is red on the base because staged fragments get
  reclaimed (`crates/custodian/tests/staged_protection.rs:898`), not because of a global scan, so
  its place in "13 ran red" says nothing about the scan guard. The global-scan probe above shows
  that assertion does work on its own.

## Findings

- NEEDS-HUMAN [impl] — `crates/custodian/src/gc.rs:1196-1199`, `:1310-1319`: three of the six T4
  blocking findings are one issue. If a pass deletes a fragment and then dies (or hits any later
  `?`) before its `Cleanup` commit, the `reclaiming` mark is left over an absent fragment, and since
  the walk is driven by `list_fragments()` (`gc.rs:297`) no pass ever revisits it. That is the #800
  sweep, which the brief puts out of scope, and the base had the same window with a legacy mark.
  But the code has no `deferred: #800` marker, so the review gate keeps raising it. The patch also
  adds new early-return points after deletes have happened — the retirement `get` (`gc.rs:1426`)
  and the intent commits (`gc.rs:1351`, `:1358`) — and each one drops the queued cleanup deletes.
  Fix: put a `// deferred: #800` marker and one sentence naming the delete-then-die window in the
  `Reclaim` doc. Under the rubric's deferral rule, that settles it.

- NEEDS-HUMAN [human] — `crates/custodian/src/restore.rs:317-324`: the other three T4 blocking
  findings. Restore names `staged.unresolvable` but says nothing about `staged.malformed`, so a run
  over an untrusted staged placement can still report `is_clean()`. I think this finding is weak:
  the chunk is held (the safe direction), GC names it on every pass (`gc.rs:256-258`), and restore
  says nothing about committed `malformed` placements on the base either (`restore.rs` has no emit
  for `referenced.malformed`). The brief also forbids new `RestoreReport` fields and assigns
  restore's counters to child-4. A human should either record the rejection with that reason, or
  ask for an audit-log line in restore (no new field).

- NEEDS-HUMAN [impl] — `crates/custodian/src/gc.rs:1336`: the surviving C5 mutant (`>=` → `<`) is
  a real test gap. I ran the whole `wyrd-custodian` suite with that change and every test passed.
  The mutant commits each reclaim intent on its own. `assert_bounded` checks the ⌈n/W⌉ commit count
  only for commits that carry deletes (`crates/custodian/tests/gc_ledger_walk.rs:567-579`), and the
  patch's own tests never execute the batch-full branch at `gc.rs:1337`
  (`gate-logs/C4-diff-cov.log`). So a regression to one commit per mark — up to 65,536 commits per
  pass at `ORPHAN_WINDOW = SCAN_CAP / 16` — would pass every test. Fix: next to the delete count,
  count the commits that carry an `orphan:` precondition and assert ⌈intents / W⌉.

- NEEDS-HUMAN [impl] — the docs overstate two claims. `docs/design/architecture/06-runtime-view.md:78`
  says the fragments of a still-draining byte retirement "are never reclaimed or marked", but the
  retirement drain is what writes those marks; they are only protected from reclaim.
  `docs/design/architecture/08-crosscutting-concepts.md:91` opens with "No pass destroys a byte
  before the destruction is durable in metadata" with no scope, yet `Reclaim::expired_lease`
  (`gc.rs:1296-1302`) still deletes before recording anything under
  `ExpiredPendingPolicy::Reclaim`. Limit both sentences to the orphan-mark path. Low severity.

- NEEDS-HUMAN [human] — `crates/core/src/metadata.rs:121-122`, `crates/custodian/src/gc.rs:1310-1319`,
  `:1373-1379`: the `reclaiming` state is only safe if no writer ever overwrites it, and nothing
  says so. A concrete case with a future mover (child-3 / #659): GC's intent CAS (compare-and-set)
  lands on `orphan:P` → the mover blind-puts its pre-mark on `P` and writes its destination
  fragment at `P` → GC's `delete_fragment(P)` removes those bytes → the mover's adoption
  `require(orphan:P == pre-mark)` still passes, because GC deletes the key only later, in
  `Cleanup` → a placement now names deleted bytes (outcome (c)). `resume` has the same exposure,
  since it deletes based on the window's read with no CAS at all. 0016 (`:1285-1344`) does not
  state this writer-side rule either. No in-tree writer can hit it today (every current writer
  marks a fragment it has just dereferenced), so it is not a defect in this diff. Suggestion:
  write "a writer never overwrites a `reclaiming` mark" into the `OrphanMark` doc now, so child-3
  inherits the rule instead of rediscovering it.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T5 Judgment — Confirm the affected-path prior-art result across merged plus closed/rejected work — the supplied self-contained target has one base commit and no remote refs, so that collision check cannot be mechanically settled here and matters for this stacked change.
- [ ] Validation — fitness-to-purpose — Decide whether the operator-facing restore verdict is fit for sign-off after the malformed-staged reporting gap is repaired — automation must not certify a store clean while a durability fault is only being silently held (`crates/custodian/src/restore.rs:192`).
- [ ] `crates/custodian/src/gc.rs:1196-1199`, `:1310-1319`: three of the six T4
- [ ] `crates/custodian/src/restore.rs:317-324`: the other three T4 blocking
- [ ] `crates/custodian/src/gc.rs:1336`: the surviving C5 mutant (`>=` → `<`) is
- [ ] the docs overstate two claims. `docs/design/architecture/06-runtime-view.md:78`
- [ ] `crates/core/src/metadata.rs:121-122`, `crates/custodian/src/gc.rs:1310-1319`,
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 6 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_662/review-b
- [ ] size backstop — this slice is behaving oversized: patch is 185 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): Slice is oversized: 188 KB patch vs. the 100 KB threshold, with four distinct GC mechanisms (staged protection class, keyed pending-retirement protection, reclaim-intent ordering with restart recovery, three-shape mark decoding) landing in one gc.rs change. Re-slice in Plan along the brief's own defect groupings: (1) staged-bytes protection class — legs A/B/C/D, the piece that stops live-upload data loss; (2) reclaim ordering + mark decoding + pending-retirement protection — legs E/F/G, the crash-safety/format-completeness piece. Each half still needs its own DST coverage. Run `pdca split 662` then `pdca split 662 --accept` to file the child briefs.
- By / date: Eduard Ralph / 2026-09-15

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
