# Brief — issue 662 / staged-reference-set-and-reclaim-intent (split parent)

> Re-plan after `iterate-plan` at sign-off (2026-09-15). v1 built the whole slice as one
> 188 KB patch (`iteration-v1/`) against the 100 KB size threshold, and sign-off sent it back
> to be split along the brief's own defect groups. **This brief records the whole slice and
> that decision; it is not built.** `pdca split 662 --accept` (2026-09-15) filed the two
> children as **#803** and **#804**. Evidence: on getwyrd/wyrd, #803 and #804 are open
> sub-issues of #662 (`gh`, checked 2026-09-15); in this bundle, `close-disposition` reads
> `split` and `split-lineage.json` lists children `803` and `804`. Do reads each child's own
> brief, `results/issue_803/brief.md` and `results/issue_804/brief.md` (the two child sections
> of `split-proposal.md`, with the ids filled in) — never this file. "Closed as split" below is
> this bundle's disposition only: on the tracker #662 stays OPEN as the children's parent until
> they close. `notes.json` was fetched before the split, so its 2026-09-13 comment's order
> `#661 → #662 → #800` is out of date: #661 merged as PR #802, and #800 now depends on #804.
> `path:line` citations are on getwyrd/wyrd `origin/main` @ `78f9859` (re-verified
> 2026-09-15). `docs/principles.md` and `engine/scripts/run-verify.sh` are in this harness
> repo (@ `c812671`), not the target. #662 is itself child 2 of #637's split (637.2).

- **Slug:** staged-reference-set-and-reclaim-intent
- **Kind:** enhancement
- **Defect:** the custodian's maintenance plane does not protect staged bytes, and GC destroys
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
- **Success criterion:** each bullet of #662's tracker acceptance is proven by a named leg in a
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
- **Falsifiability:** every leg above goes RED in-process, by assertion, with no container:
  #803's on `origin/main` @ `78f9859`; #804's on `origin/main` once #803 and #664 have merged
  (wave mode `merge`); #664's on its own base, `main` plus #803. They can, because every record class they
  seed exists on `main` (#691, #715, #716, #771, #772) while nothing in the maintenance plane
  reads it, and no `orphan:` value but the bare decimal decodes. Each child's test names only
  symbols that exist on `main` today, so a red leg fails by assertion, never by a compile error
  (`engine/scripts/run-verify.sh:520-541` calls that UNVERIFIABLE), and C4-verify earns its red
  from the added `*/tests/*.rs` file (`run-verify.sh:139-144`, `:388-392`). Each child also
  appends one seeded DST property to the existing `crates/dst/tests/custodian.rs`
  (`#![cfg(madsim)]`), which only `cargo xtask ci` runs.
- **Invariant to restore:** C-1 — no permanent or data-losing failure mode is an acceptable
  cost: every durable byte is, at every instant, protected by a record that names it **or**
  evidenced for reclamation, and every state has an actor that exits it in bounded time
  (`docs/principles.md` §5 C-1 and the §6 storage-lifecycle row, harness repo: maintainer's rule
  2026-07-25; `0016:2802-2813`; `gc.rs:30-33`). 0016 states the same property as its
  invariant (2) (`0016:869-871`), as a no-gaps claim, never a partition (`0016:2911-2922`).
- **Repo + branch target:** getwyrd/wyrd @ main
- **Ordering note:** accepted 2026-09-15 as **#803** (child-1, staged reference set) and
  **#804** (child-2, reclaim intent, mark shapes and keyed retirement protection), filed as
  sub-issues of #662. #661 (the paged ledger walk) is merged as PR #802, so the children have
  no unmerged prerequisite. They declare `Conflicts with` each other (shared `gc.rs`,
  `crates/dst/tests/custodian.rs` and `06-runtime-view.md`; no build-on), and the scheduler
  builds #803 first. #804 also conflicts with #664 (both edit `06-runtime-view.md`), declared on
  #804 at the human's direction in this revision session, so the run is #803 · #664 · #804 ·
  #663 · #800 (checked with `waves.compute_waves`). Downstream re-pointing, done in this session right after acceptance: #664
  depends on #803, #800 on #804, #663 on both. Two unbriefed issues must also name #804 when
  they are briefed: #659, whose tracker body names #662, because the migration gate it owns
  (`0016:1259-1273`) guards the retirement writers that make #804's keyed lookup live; and
  #723, because its pre-mark adoption is only safe once GC records a reclaim before it
  destroys (gap 2). **Intake-cap override (wyrd-pdca-P1):** granted by the human (Eduard Ralph)
  in this re-plan session, 2026-09-15, for #662's split into two children. The count at the
  override was `planned 22/6 (cap) — room for 0, need 2: Plan intake closed`. The split nets
  +1: two children filed, this bundle closed as split.
- **Surfaces:** data
- **Difficulty:** high
- **Scope:** decomposed into two children, each independently shippable.
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
- **Repro instruction:** on `origin/main`, seed an `Open` session (`mpu:`), one `part:` record
  and one owned `sidx:` entry whose fragments sit on a D server, give each an `orphan:` mark
  older than the grace window, and run one GC pass: both fragments are deleted.
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** none here — each child ships its own NEW test file, named per leg in the
  Success criterion above (kept path-free here, so the driver reads no test file for this
  parent).
- **Prior-art check (triage cycles):** by path (`crates/custodian/src/gc.rs`, `restore.rs`,
  `crates/core/src/metadata.rs`) across merged history, open PRs and closed ones: no merged
  change adds a staged class or a `reclaiming` state (`git log -S'sidx' origin/main --
  crates/custodian/src` is empty); no open PR touches these files; the one closed-unmerged PR
  touching `gc.rs`, #647 (segmented maps), is unrelated. Rejected prior art: #508's 4th
  attempt (a resolver only the read path used — restore stranded parts, GC deleted them);
  #637 v1 (the whole class inside a 334 KB patch); #662 v1 (`iteration-v1/`, 188 KB, every
  gate but the review green, returned as oversized — its review findings are carried into the
  children).

## Plan-review response (revision pass, 2026-09-15 — `plan-advisory-plan-reviewer.md`)

Five findings; all five changed the brief, and the reviewer was right in part on each. Claims
re-checked against `origin/main` @ `78f9859`, the live tracker, and this harness repo @
`c812671`. The reviewer had only this brief and a `notes.json` fetched before the split, so it
could not see `split-proposal.md` or the child briefs. Several findings came from that; the
brief now stands on its own.

- **F1 — split recorded with no tracker evidence.** *Revised; the split stands.* The header
  now cites the evidence: #803 and #804 are open sub-issues of #662 on the tracker, and this
  bundle's `close-disposition` and `split-lineage.json` record them. "Closed" meant this
  bundle's disposition; the brief now says #662 itself stays OPEN on the tracker, and that
  `notes.json`'s `#661 → #662 → #800` order is out of date.
- **F2 — gap 2 is not a live data-loss path.** *Revised; upheld.* Confirmed: no code on
  `main` adopts a pre-mark under an exact-value `orphan:` precondition (#653's pre-mark is
  unmerged too). Gap 2 now calls itself a prerequisite and names the two slices that will add
  that CAS, #663 and #723; the Ordering note says #723 must depend on #804. #804's leg B(iii)
  still tests the order, using a hook that stands in for that future CAS.
- **F3 — child-1 hides drain and read-order work.** *Revised; half upheld.* The read order and
  the handoff-interleaving regression were already in #803 (its Scope and leg C); both are now
  quoted here. Drain status is not #803's: 0016 keeps the staged member disjoint from `placed`,
  and #664 (depends on #803) owns the drain answer in its legs A and B. The success criterion
  now says so.
- **F4 — circular success criterion pointing at an unseen file.** *Revised; upheld.* The
  criterion now maps each tracker acceptance bullet to the named child leg that proves it, with
  the base red for each, and Falsifiability says where each leg goes red.
- **F5 — keyed retirement protection vs the migration gate.** *Revised; took the reviewer's
  second option.* #804 is constrained to GC-side reading: no production writer of an
  event-carrying mark or of a `retire:` key, which Check can confirm from the diff. The gate
  binds #659's writers. #659 already depends on #662 on the tracker, and the Ordering note now
  says its brief must name #804.
- **Also fixed in the verify pass.** `docs/principles.md` and `run-verify.sh` are marked as
  harness-repo paths. The `Test file` field stays path-free: the driver's `test_files()` reads
  path tokens from it, and this bundle still holds v1's copy of `staged_protection.rs`.
- **Left for the child and sibling briefs (not edited in this pass).** #804's Defect item 1
  still words the adoption CAS as a live path (F2). #664 has no leg that runs a handoff
  between drain status's reads (F3). #662's tracker acceptance still names "the drain answer"
  as its observable, though #664 owns it.
- **Shared file found in the batch check, and resolved.** #804 and #664 were in one wave, and
  both edit `docs/design/architecture/06-runtime-view.md` (#804 §6.7 step 2; #664 the restore
  fence). At the human's direction #804 now declares `Conflicts with: 664`; the ordering
  sentences in #804, #664 and #663 were updated to match. The run grows by one wave: #803 ·
  #664 · #804 · #663 · #800. Because #664's restore fence now lands before #804, gap 4 and
  #804's scope rule are worded against #804's own diff, not against `main`.
