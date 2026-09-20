# Result — issue 662 / staged-reference-set-and-reclaim-intent

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: the custodian's maintenance plane does not protect staged bytes, and GC destroys
  bytes before recording that it is doing so. Four gaps:
  1. **Staged bytes are unprotected.** `ReferenceSet` holds committed placements only
     (`crates/custodian/src/gc.rs:383-413`), built from the `inode:` scan alone (`:478-573`,
     the scan at `:483`). A committed part's fragments and an upload's in-flight owned
     (`sidx:`) fragments are in no protected set, so GC reclaims a marked one past grace
     (`gc.rs:272`, `:277-326`), and restore — through the same predicate
     (`crates/custodian/src/restore.rs:385`) — marks them stranded (`restore.rs:440-443`).
     → **#803**. Drain status has the same blind spot: `reconciliation_status` counts
     `referenced.placed` only (`crates/custodian/src/desired_state.rs:188-196`), and 0016
     keeps the staged class disjoint from `placed` (`0016:767-769`, `:881`), so a server
     holding only staged bytes reads `Satisfied`. → **#664** (a sibling from #637's split,
     which depends on #803).
  2. **GC destroys first and records second.** It calls `delete_fragment` (`gc.rs:314`) and
     only queues the key delete (`:321`), which commits after the whole fleet sweep (`:346`).
     For that window the ledger holds a mark's exact bytes for a fragment that is gone. **No
     code on `main` reads that window yet:** reconstruction and rebalance CAS only the inode
     and write fresh bare marks (`crates/custodian/src/reconstruction.rs:937-947`,
     `crates/custodian/src/rebalance.rs:538-547`), and nothing adopts a pre-mark under
     `require(orphan:<pos> == prior)`. So this gap is a prerequisite, not a live data-loss
     path. 0016's repoint and re-place moves adopt a pre-mark under that precondition
     (`0016:1285-1292`), and the precondition can only see a reclaim that GC records first
     (`0016:1293-1320`). Two open slices will add such an adoption CAS: #663 (the staged
     re-place), which already depends on #804 for exactly this, and #723 (pre-marking in
     reconstruction and rebalance), not yet briefed. → **#804**.
  3. **Only one `orphan:` value shape decodes** (`gc.rs:811-817`); 0016's structured and
     `reclaiming` shapes (`0016:1190-1211`, `:1321-1338`) read as unreadable and are kept
     forever. → **#804**.
  4. **A pending byte retirement protects nothing** (`0016:1226-1248`). This is dormant on
     `main`: nothing writes a `retire:bytes:` obligation or an event-carrying mark yet.
     `retire_key` has no caller outside `crates/core`, and every mark writer writes the bare
     decimal (`gc.rs:187-190`, `restore.rs:440-443`, `reconstruction.rs:943-946`,
     `rebalance.rs:543-546`, `crates/core/src/metadata.rs:1896`, `:2004`, `:2077`). #664's
     restore fence, which builds before #804, adds the first obligation writer; #659 adds the
     first event-carrying marks. The keyed lookup only fires on a mark that names an event, so
     it stays dormant until #659. → **#804**, GC-side reading only (see Scope).
- Success criterion: each bullet of #662's tracker acceptance is proven by a named leg in a
  child's NEW test file — red on the child's base by assertion, green with its fix — and
  `cargo xtask ci` is green on each child. This parent carries no patch.
  (a) *Staged fragments survive a GC pass, by a positive observable, not "nothing was
  deleted".* #803, `crates/custodian/tests/staged_protection.rs`: leg A — an `Open` session's
  committed `part:` fragment and owned `sidx:` fragment, each marked past grace, survive
  `reconcile_step` while an unprotected control is reclaimed (base: both reclaimed); leg B —
  `reconcile_after_restore` marks neither and `stranded_marked` excludes them, and a later GC
  pass keeps both (base: marked, then deleted); leg C — a part commit, and separately a
  publication, landing between the builder's two reads leave the fragment unreclaimed (base:
  reclaimed). The tracker's example observable, "the drain answer for a server holding them",
  is #664's legs A and B: `reconciliation_status` answers `Pending` for a server holding only
  an `sidx:` fragment, and for one holding only a `part:` fragment (base: `Satisfied`). It sits
  there because #803 leaves drain status unchanged (gap 1).
  (b) *Intent recorded before destruction.* #804, `crates/custodian/tests/gc_reclaim_intent.rs`,
  leg B: when the intent commit fails the fragment is still present; a `delete_fragment` hook
  that commits `require(orphan:<pos> == <bytes GC read>)` gets `Conflict`; a `reclaiming` mark
  is finished next pass with no grace test (base: GC deletes first, the hook's commit succeeds,
  and `reclaiming` does not decode). Leg C: intents are batched, at most `CLEANUP_BATCH` per
  commit (base: no commit carries one).
  (c) *All three shapes decode.* #804 leg A: a bare decimal,
  `{"orphaned_at_millis":N,"event":"E"}` and `{…,"reclaiming":true}` are each honoured; a value
  matching none is kept byte-identical, never reclaimed, and named on GC's audit seam (base: a
  structured mark past grace licenses nothing).
  (d) *Keyed protection while a retirement is pending* (the 2026-09-13 scope comment). #804
  leg D: a structured mark past grace whose `event` names an existing `retire:bytes:` key keeps
  its fragment; once that key is deleted the next pass reclaims it; no `scan` reads `retire:`
  (base: red on the second half — the structured shape does not decode).
- Repo + branch target: getwyrd/wyrd @ main
- Scope: decomposed into two children, each independently shippable.
  **#803 (gap 1):** the staged class in the shared reference set, read through bounded
  per-session ranges in source-before-destination order — `sidx:` → `part:` → the `inode:` scan
  (`0016:782-800`; the build starts at `inode:` today, `gc.rs:483`) — as its own member,
  disjoint from `placed`, honoured by the shared predicate GC and restore gate on. Its leg C is
  the handoff-interleaving regression, for both handoffs. Scrub and drain status keep today's
  answers. Drain status counting the staged member is #664's; drain status already calls the
  same build (`desired_state.rs:188`), so reading the member from it carries #803's read order.
  **#804 (gaps 2–4):** reclaim intent committed before `delete_fragment`; the three `orphan:`
  value shapes in one codec beside `orphan_key`; keyed protection while a `retire:bytes:`
  obligation is pending. **GC-side reading only:** #804's diff adds no production writer of an
  event-carrying mark and no call to `retire_key` — `mark_orphaned`'s output stays the bare
  decimal, and the only new value GC writes is its own `reclaiming` transition. So #804
  enables no retirement path, and 0016's "MUST NOT be enabled until that pass has COMPLETED"
  (`0016:1259-1273`) binds #659, which turns those paths on and owns the gate (#659's tracker
  scope note from #637's re-plan, 2026-09-12). Until #659 lands no mark names an event —
  #664's restore fence writes obligations but no marks — so the keyed lookup never fires
  outside a test. Do builds the children, never this brief. / out of scope: everything each
  child's own out-of-scope names — drain status, rebalance, restore's staged counters and
  session fence (#664); scrub and reconstruction, including the first adoption CAS (#663); the
  fragment-less mark sweep (#800); the orphan-identity migration gate, its cleanup pass and the
  three-arm mark write (X92/X111, `0016:1218-1224`, `:1249-1280`; all #659's); any edit to 0016
  or an ADR.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: Fixed
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — N/A — close disposition (no patch to verify)
- C3 Change: none — patch.diff
- C4 Verification (red→green): none — N/A — close disposition (no patch to verify)
- C5 Causal adequacy: none — reviewer + human sign-off

## 4. Conformance (Check — stack)
- T1 Structure: none — N/A — close disposition (no patch to verify)
- T2 Shape: none — N/A — close disposition (no patch to verify)
- T3 Runtime: none — N/A — close disposition (no patch to verify)
- T4 Contribution: none — N/A — close disposition (no patch to verify)
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

# Advisory review — SKIPPED (close disposition)

The reviewer leaf was skipped: this bundle's Plan concluded a close / no-fix disposition (split), so there is no patch to review.

- NEEDS-HUMAN — Confirm the close disposition 'split' (no patch was built). Override to a fix path (iterate-to-Do) if the close is wrong.


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] Confirm the close disposition 'split' (no patch was built). Override to a fix path (iterate-to-Do) if the close is wrong.
- [x] The brief has changed the tracker item from an open, buildable #662 into a closed, non-buildable split parent without supplied tracker evidence. `brief.md:3-8` and `brief.md:43-50` say the split was accepted as #803/#804 and this parent closed, but `notes.json` says `"number": 662, "state": "OPEN"`; its only comment says, “What stays here: bullets 1–3” and gives the ordering `#661 → #662 → #800`. No `split-proposal.md` or `dependency-state.json` was supplied to resolve #803/#804. The planner must reconcile the issue state and target before Do can know whether to build #662 or two children.
  - Resolved: superseded by the re-plan (2026-09-15). `split-proposal.md` now exists in the bundle; `brief.md` cites `close-disposition` and `split-lineage.json` as evidence. Verified live against GitHub: #662 OPEN as parent, #803 and #804 OPEN as filed children — matches the brief's claim.
- [x] Gap 2 is framed as a currently reachable data-loss defect, but only its first half exists on the target. GC does delete bytes before ledger cleanup (`crates/custodian/src/gc.rs:314-321`, cleanup committed at `:346`), while the current repoint paths copy bytes and CAS only the inode (`crates/custodian/src/reconstruction.rs:931-953`; `crates/custodian/src/rebalance.rs:530-556`); neither adopts a pre-mark under an exact-value `orphan:` precondition. Thus `brief.md:22-24`’s “adoption CAS ... can land after the fragment is gone” is a future-protocol prerequisite, not a source-supported current execution. The brief must name the change/dependency that introduces that CAS or narrow the claimed root cause.
  - Resolved: the current brief now says this itself — "this gap is a prerequisite, not a live data-loss path."
- [x] Child 1 hides required consumer and ordering work behind “the staged protection class in the shared reference set” (`brief.md:53-56`). Merely adding staged membership to `ReferenceSet::protection` cannot meet the tracker’s positive drain observable: drain status directly examines `referenced.placed` (`crates/custodian/src/desired_state.rs:188-196`), while the staged class must remain disjoint. The target design also makes the read order `sidx:` → `part:` → committed inodes normative to avoid missing both sides of an atomic handoff (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:782-800`), whereas the current builder begins with `inode:` (`crates/custodian/src/gc.rs:478-505`). Name the desired-state change and a handoff-interleaving regression in child 1; otherwise its stated scope/test can pass the seeded GC repro while leaving the drain race unfixed.
  - Resolved: child 1's scope now names the `sidx:` → `part:` → `inode:` read order explicitly and hands drain-status counting to #664 by name.
- [x] The parent’s success criterion is partly circular and delegates the falsifiable details to an unavailable file: “both children accepted at sign-off” is a human outcome, and `brief.md:32-35` points to `split-proposal.md`, which is not among the supplied inputs. The named new files and `cargo xtask ci` (`brief.md:28-31`) are commands/locations, but not the exact red assertions. Copy each child’s observable into this brief (the tracker already supplies “positive drain answer,” “intent-before-destruction,” and three-value decoding) so Check does not depend on an absent artifact or sign-off itself.
  - Resolved: `split-proposal.md` is now supplied in the bundle, and the parent brief copies each child's specific test legs inline.
- [x] Child 2 claims “keyed protection while a byte retirement is pending” while declaring the orphan-identity migration gate out of scope and “no unmerged prerequisite” (`brief.md:45-46`, `brief.md:55-60`). The target design says the keyed lookup is sound only after stale marks are cleared and that new retirement paths **MUST NOT** be enabled until the durable completion marker exists (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:1242-1247`, `:1259-1273`). Either declare the gate-owning issue as an ordering dependency and test the disabled-before-marker behavior, or explicitly constrain child 2 to dormant decode/GC support that cannot enable retirement writers; the present “independently shippable” scope leaves that safety ordering unverifiable.
  - Resolved: the brief now states child 2 is "GC-side reading only," writes no event-carrying marks, and stays dormant until #659 turns that path on.

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: merged-wider
- Iteration delta (if iterating):
- By / date: Eduard Ralph / 2026-09-15

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 5 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
