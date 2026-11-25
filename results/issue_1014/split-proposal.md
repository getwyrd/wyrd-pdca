<!-- pdca:split-proposal v1 -->
# Split proposal — issue 1014

> **Intake-cap override (wyrd-pdca-P1).** The human (Eduard Ralph) explicitly overrode the rule
> in this Plan session on **2026-11-18** for **#1014's split only** ("split as you suggest;
> follow your recommendation for decision 2 too"). `scripts/plan-cap --track alpha --need 3`,
> run while this proposal was drafted:
>
> ```
> planned 12/6 (cap, track alpha) — room for 0, need 3: Plan intake closed
>   PLANNED            7  1016 1017 1018 1019 668 669 670
>   BUILT              4  508 625 633 654
>   ITERATE_DO         1  1015
>   other tracks (their own room): blackbox 1
>   rule wyrd-pdca-P1 (docs/INTEGRATION.md §11): no new brief in track alpha, no `pdca split --accept`, no split or reslicing proposal until its sign-offs make room.
> ```
>
> #1014 itself is UNPLANNED (iterate-to-Plan archived its brief), so it is not in that count.
> Accepting this proposal puts three children in, and the split parent sits at BUILT (counted)
> until its sign-off confirms the split: alpha goes from 12 to **16**, and drops to 15 once #1014
> is signed off. The override covers these three children and nothing else. (Aside for Act: #508,
> #625 and #633 carry `close-disposition = split` yet `pdca status` lists them BUILT, so the true
> count is probably three lower; the cap is closed either way.)
>
> **Authored by the planner, in the splitter's format.** `[leaves.splitter]` is an interactive
> leaf and cannot run inside the Plan session's shell; a split is brief authoring, so the planner
> wrote this file directly (precedent: #1012's split, 2026-11-11).

## Why this slice is oversized

Three builds of #1014 (2026-11-10 to 2026-11-18, none published) each closed the previous
round's list and each Check reproduced **new** data-safety defects. The third patch was 303 KB
over 28 files (backstop: 100 KB / 20 files): the cleanup pass alone was ~100 KB of production
code (`crates/custodian/src/orphan_identity.rs` 78 KB, `crates/core/src/orphan_identity.rs`
24 KB) and most of a 98 KB test file; the routing was ~17 KB. The iteration-3 sign-off chose
`iterate-plan`, a split, and two decisions the brief had left open. The findings fall into three
groups that touch different code and can each ship on their own:

- **The cleanup and its marker** (rounds 1–3, most findings): cursor damage and progress, paging
  past the scan cap, memory per pass, what "complete" requires, the restore binding, and the
  per-mark rules (reclaiming marks, draining marks, untrusted placements, staged records).
- **The routing** (round 3): a segmented delete with a missing `seg:` row leaks that row for good;
  a malformed flat placement becomes an obligation the drain cannot finish; the gate pin on routed
  writes has no test; and (round 1) GC's expired-lease arm reclaims an unmarked, owed fragment.
- **Stale marks created after the marker** (the iteration-2 adversary, undecided twice): committed
  repair and evacuation never look at the destination position's mark, so a fragment moved back
  onto a server within grace stays referenced under an old mark. Proposal 0016 already requires
  those moves to pre-mark their destination (`0016:1287-1294`, X47 `0016:2587`); only the staged
  path does (`crates/custodian/src/reconstruction/staged.rs:634-656`, `:721-751`).

## The split line

1. **child-1 — the cleanup pass, its completion marker and GC's closed gate.** Safe on its own:
   until the marker is valid GC is *more* cautious than the base, and after it GC behaves as on
   the base. It also closes a live gap on the base: multipart Complete already supersedes a prior
   generation without writing marks (`crates/core/src/multipart/complete.rs:1904-1911`), so a
   stale mark on one of its fragments lets GC reclaim before the drain marks it.
2. **child-2 — committed repair and evacuation pre-mark their destinations (0016 X47).** The
   human's decision 2 (2026-11-18): its own child, a prerequisite of the routing. Independent of
   child-1's code. Also closes two live gaps on the base: a move that loses its commit leaves its
   written fragment unmarked (GC keeps it for ever), and a move whose destination carries an
   expired mark can have its fresh fragment reclaimed before the move commits.
3. **child-3 — the routing of `unlink` and supersede**, behind child-1's marker, after child-2.

**The one design change from iteration 3 (planner's, for the human to confirm):** a stale mark
on a fragment that a multipart **staged** record names (`sidx:` entry, `part:` record, published
or not) is neither dropped nor kept-and-certified any more: the lap **blocks** on it and judges it
again once that record has published (the fragment is then committed: drop) or been retired
(the drain marks it, or nothing names it: GC's input). Iteration 3 split this case into
"unpublished: drop" and "published, records pending: keep byte-identical", which needed the
session phase, the `retire:records:` `{parts}` ledger and the undecodable-session rules, and that
is where most of rounds 1–3's defects sat. Blocking costs liveness only while a staged record
names a stale mark, which on today's stores never happens (no production path creates staged
records before #508) and after a restore lasts until the fence teardowns and records drain
finish. GC's own `StagedSet` rules (`crates/custodian/src/gc.rs:1361-1392`) already decide what
"a staged record names this position" means.

Rough sizes: child-1 **~120–160 KB** (still over the backstop: the walk is inherently large, but
the simplification removes the multipart-phase logic and its tests), child-2 **~40–60 KB**,
child-3 **~60–90 KB**. All three are depth-2 children: they cannot be split again without
`--force`.

## Wave sketch

```
wave 1: child-1 (cleanup, marker, GC gate)   ||   child-2 (destination pre-mark)
wave 2: child-3 (routing behind the marker)
```

- **child-1 and child-2 are independent** and share no source file: child-1 edits
  `crates/custodian/src/gc.rs`, `crates/custodian/src/reconciliation.rs` and new modules;
  child-2 edits `crates/custodian/src/reconstruction.rs` and `crates/custodian/src/rebalance.rs`.
  Both touch `docs/design/architecture/06-runtime-view.md`, in different paragraphs (child-1 the
  GC paragraphs at `:78-92`, child-2 the Reconstruct and Rebalance steps at `:40-41`); each brief
  confines its edit to its paragraphs, so no `Conflicts with` is declared.
- **child-3 depends on both.** It needs child-1's marker to gate on (0016's rule: routing only
  after the cleanup has completed) and child-2's moves (routing removes the inline re-stamp that
  today hides a stale destination mark).
- **Outside this proposal** (ordering fields name siblings only, `split.py:_validate_ordering`;
  added by hand after acceptance):
  - every child's `Depends on` gains **`1013`** (#1013's PR #1082 carries #1062 and #1061, none
    merged yet; the citations are on its head `f54db83`, and child-2's tests need the
    `ReconstructionContext` fields #1013 adds, `crates/custodian/src/reconstruction.rs:142-155`).
  - **#668, #669, #670** name `1014` in `Depends on`; a split parent counts as met at once, so
    each gets `1014` replaced by child-3's id (child-3 brings the other two).

## Decisions recorded this session (2026-11-18)

- **The human:** split three ways as above; the repair/rebalance stale-mark question becomes
  child-2, and child-3 depends on it.
- **Planner's, for the human to confirm or veto before `--accept`:**
  1. Staged-named stale marks block the lap (above).
  2. child-3 also gives multipart Complete's superseded-prior obligation the same refusals it
     gives `unlink` and supersede (missing `seg:` row, malformed flat placement): the gap is the
     same, on the same kind of obligation (`complete.rs:1904-1911`), and is live on `main` today.
  3. child-3 owns the expired-lease arm (iteration 1's "pending-lease bypass"): routing and
     Complete are the two unreferencing paths that leave a fragment unmarked until the drain.

<!-- pdca:child child-1 -->
# custodian: orphan-identity cleanup pass, completion marker and GC's closed gate (636.6c-1) — issue 1014 child-1

> Child 1 of 3 of #1014's split (#1014 is #659's child-3, slice 6 of #636). Do reads ONLY this
> file. Keep the `- **Label:** value` lines. The design is normative in proposal 0016,
> `docs/design/proposals/draft/0016-multipart-commit-protocol.md`: read **decision 4**'s keyed
> protection and migration gate (`0016:1228-1282`) and X111 (`0016:2650`) before writing code.
> Citations are verified on `f54db83` (#1013's PR head, which carries #1062 and #1061 on `main`
> @ `cdc4047`; `main` @ `57aefe0` changes nothing under `crates/` or `docs/` since). Re-locate
> every anchor on the base this bundle is given.
>
> **This is a re-plan.** #1014 was built three times with the routing; each Check reproduced
> new defects in the cleanup. Every one of them is a requirement below with a leg that tests it.
> Do not port a previous attempt; build to this brief.

- **Slug:** orphan-identity-cleanup-gate
- **Track:** alpha
- **Kind:** enhancement (design proposal 0016, decision 4)
- **Defect:** GC trusts a fragment's `orphan:` mark to name the event that last unreferenced it:
  for a mark whose stamp has aged past grace it asks only whether *that mark's* event's
  `retire:bytes:` obligation is still present (`crates/custodian/src/gc.rs:686-707`,
  `retirement_draining` `:760-766`), and a legacy mark names no event at all. 0016 allows that
  keyed lookup only after a one-time cleanup has removed stale marks (marks older than their
  fragment's latest unreference) and recorded its completion durably; until then GC must treat
  every mark as protecting (`0016:1251-1282`, X111 `0016:2650`). The base has no cleanup, no
  marker and no gate, and already has an unreference that writes no marks: multipart Complete
  supersedes a prior generation by installing one `retire:bytes:g:<inode>:<version>` obligation
  (`crates/core/src/multipart/complete.rs:1904-1911`), which the drain marks later. A fragment of
  that prior that carries a stale mark (from a restored image, an older custodian, or a move,
  see child-2) is reclaimed on the stale stamp before the drain marks it — inside the grace a
  reader of the prior generation relies on.
- **Success criterion:** A NEW test file `crates/custodian/tests/orphan_identity_cleanup.rs`,
  over `RedbMetadataStore::in_memory()` (with `.with_scan_cap(n)` where a leg says so,
  `crates/metadata-redb/src/lib.rs:86`) and a placement-aware chunk-store double, driving every
  pass **through `reconcile_step`** with the contexts the deployed loop passes
  (`crates/server/src/custodian.rs:397`); objects are written, superseded and published by the
  production functions (`write::commit_overwrite`, `metadata::unlink`, the production multipart
  Complete), and restores by `reconcile_after_restore`. "Stale mark" means a legacy mark stamped
  long before the test's clock, planted as a restored image or an older custodian leaves one.
  "Before the drain" means GC-only passes (no reconstruction context, so no drain and no cleanup)
  with the clock past the stale stamp's grace. It shows all of:
  **(G) The gate.** (G1) With no valid marker, GC reclaims no marked fragment, whatever the mark's
  age or shape: a stale mark on an unreferenced fragment is held. (G2) The base's live gap: a
  committed flat object one of whose fragments carries a stale mark is superseded by a
  production Complete; GC-only passes before the drain reclaim **none** of its fragments. Then
  full passes: the cleanup completes and writes the marker, the drain marks every fragment of
  the prior, and GC reclaims each only after the grace those marks start, never before; all are
  gone at the end. The same for a segmented prior (published by the production Complete).
  (G3) A valid marker present: GC's keyed lookup works as on the base.
  **(J) The per-mark rules** (Design), one case each, every one on the real fixture:
  (J1) committed reference, trusted placement: the stale mark is dropped, on a flat and on a
  segmented committed object. (J2) a pending `retire:bytes:` obligation names the position
  through a trusted placement (a flat and a segmented generation from the production Complete,
  drain held off): the stale mark is dropped; the drain then marks it, and GC reclaims nothing
  before those marks' grace. (J3) a staged record names it (an `sidx:` entry; an unpublished
  part's `part:` record; a published part's records awaiting the records drain): the mark is
  left and the lap reports **blocked** naming the record; after the part publishes, a later lap
  drops it (committed); after the records drain retires the part records, a later lap leaves it
  (named by nothing) and the marker is written; at no step is a fragment reclaimed before the
  grace of the evidence it then has. (J4) a placement the walk cannot trust names the chunk
  whole (a committed map with a wrong-length placement; an undecodable `sidx:` entry): blocked,
  never dropped. (J5) a `reclaiming` mark: under a committed or staged reference, blocked (and
  after the marker, never resumed by GC without grace); under pending obligations only, or
  nothing, left and not blocking, and the iteration-2 cycle completes: a production Complete's
  generation obligation, a `reclaiming` mark on one of its fragments, the production post-restore
  pass, then alternating passes until the marker is written, GC finishes the reclaim, the drain
  completes and the obligation is gone, within a stated number of passes. (J6) a structured mark
  whose own obligation is present: left byte-identical and not blocking on its own obligation's
  position; **blocked** when, besides, the committed class names the position, the staged class
  names it, or **another** obligation names it — three cases, each then run through a Complete
  supersede and GC before the drain to show no reclaim. (J7) undecodable mark: left, never acted
  on. (J8) unnamed mark: left (GC's ordinary input).
  **(R) Races.** Each drop requires the bytes it judged by: a wrapper pauses the cleanup between
  its reading and its drop commit, runs a production inline `unlink` of the object (gate closed),
  then lets the drop commit: the drop fails, the unlink's mark survives, and the fragments go
  only after its grace. The same with an overwrite, and with a production Complete superseding
  the object in place of the unlink.
  **(K) Completion.** (K1) A lap that meets an unreadable inode, `seg:` record, `part:` record,
  `sidx:` entry or `retire:bytes:` obligation (one fixture each) reaches the end and writes
  **no** marker; it reports a typed blocked outcome naming the record; after repair a later lap
  completes. (K2) A lap interrupted mid-way writes no marker. (K3) With a steady stream of inline
  deletes and overwrites under way (gate closed), the marker is still written within a stated,
  asserted number of passes. (K4) Diagnostics are bounded: 40 unreadable records, all counted,
  at most a stated number named (unit test).
  **(W) The walk's position.** (W1) A stored position that is not a canonical `orphan_key` the
  walk could have judged — `{"after":"ff"}`, `orphan:~` (`6f727068616e3a7e`), `orphan:\xff`, a
  truncated or over-long key — restarts the lap; it never writes the marker. Each then run
  through a Complete supersede and GC before the drain: nothing reclaimed. (W2) More than one
  window of malformed ledger keys sharing a long prefix: successive passes advance past them or
  report blocked naming them; no pass re-reads the same window for ever (assert progress across
  passes).
  **(P) Bounded reads.** (P1) The lap completes on a store whose `inode:` count, staged-record
  count and `retire:` count each exceed the backend's scan cap (`with_scan_cap`). (P2) Many
  obligations naming one marked position: the evidence the pass keeps for that position does not
  grow with their number (assert through a test-visible count or a unit test). (P3) The pass's
  store reads are counted through the test's store wrapper and never exceed the stated per-pass
  bound, including a pass in which two drops lose their commit at the bound.
  **(T) Restore.** A marker, then the production post-restore pass with stale marks planted as a
  restored image carries them: the marker reads as **not** valid, GC holds every mark, and the
  cleanup walks again and writes a marker valid for the new fence generation.
- **Falsifiability:** RED is reachable in-process on `cargo test -p wyrd-custodian`. (G1) and (G2)
  are red **on the base** through existing APIs only (GC reclaims the stale-marked fragment), so
  the test file can earn a real C4-verify red if it compiles against the base (see Verification
  posture). Every other leg is red on the mutation named for it there.
- **Invariant to restore:** **C-1** (`docs/principles.md` §5 C-1 and its §6 row *storage
  lifecycle / reclamation*): every durable fragment is, at every instant, named by a record or
  evidenced for reclamation by a mark whose grace a reader can rely on — here: no fragment is
  reclaimed on the stamp of an event older than the one that last unreferenced it. GC's keyed
  lookup may judge a mark by its own stamp only while the marker is valid; the marker is valid
  only after a lap that judged every mark against a complete, current reference set, left no
  mark on a position anything still names (except where the rules leave it and say why), and
  ran under the restore-fence generation still current. **Sources:** the maintainer's rule
  2026-07-25 (C-1); 0016's refutation standard (`0016:2530-2535`); decision 4 (`0016:1228-1282`)
  and X111 (`0016:2650`); `crates/core/src/metadata.rs:154-169` (no writer replaces a
  `reclaiming` mark). SELF-TEST: it binds GC's judgement, the cleanup, the deployed dispatch and
  the restore pass; no single module can satisfy it.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Ordering note:** wave 1 of #1014's split, beside child-2 (no shared source file; the docs
  edits are in different paragraphs). `1013` is added to `Depends on` after acceptance: the
  cleanup runs beside #1062's drain in `reconcile_step`'s retirement leg, which #1013's PR head
  carries (`crates/custodian/src/reconciliation.rs:159-172`). Downstream: child-3 gates on this
  child's marker. **Intake-cap override (wyrd-pdca-P1):** the human's 2026-11-18 override for
  #1014's split (top of `results/issue_1014/split-proposal.md`).
- **Scope:** a one-time, page-wise cleanup of stale `orphan:` marks with the per-mark rules in
  Design; its durable completion marker and the marker's validity (completion, restore); GC's
  conservative behaviour until a valid marker is observed; the deployed dispatch of the cleanup;
  the docs (`06-runtime-view.md`, the GC paragraphs only). / out of scope:
  - any change to `unlink`, supersede or Complete's routing (child-3); repair and evacuation
    destination marks (child-2); the expired-lease arm (child-3);
  - GC cancelling a `reclaiming` mark under a live reference (a GC protocol change; such a mark
    blocks the marker and is named for a human, J5);
  - a canonical-but-wrong stored position (a valid key past real marks): grammar cannot detect
    it; say so in the docs, do not build integrity for it;
  - seeded DST (#660); any edit to proposal 0016 or an ADR; `Cargo.lock` and `deny*.toml`.
- **Repro instruction:** on `f54db83`: commit a flat RS(2,1) object with `write::commit_overwrite`;
  put the legacy value `1000` at `orphan_key(d, frag)` for one of its fragments; supersede the
  object with the production multipart Complete; run `reconcile_step` with only a GC context and
  the clock past `1000 + grace`: GC reclaims that fragment (its mark is legacy, so
  `retirement_draining` answers false, `gc.rs:694`, `:760-766`) while the
  `retire:bytes:g:<inode>:<version>` obligation that owes it a fresh mark is still pending.
- **Difficulty:** high
- **Do model:** opus
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/custodian/tests/orphan_identity_cleanup.rs`, a **NEW** file (C4-verify
  treats an added `crates/*/tests/*.rs` as the per-fix discriminator; dry-run of
  `engine/scripts/run-verify.sh --classify` on this child's file set: `ADDED_TEST` for it).
- **Verification posture:** default (a flippable test), with one condition: the file must compile
  against the **base's** public API — no `use` of anything this child adds; reach this child's
  records by their key bytes and its behaviour through `reconcile_step`. Then C4-verify's red leg
  runs and (G1)/(G2) fail on the base. Unit tests that need this child's types go inline in its
  modules. If a leg cannot be written that way, move it inline and say so in `build-notes.md`;
  if the file still cannot compile on the base, C4-verify reports UNVERIFIABLE (exit 77): record
  a base red for (G2) by hand in `build-notes.md`. **Demonstrated reds (required, one mutation
  each, recorded in `build-notes.md`):** the gate read as open with no marker → (G1); a staged-named
  stale mark dropped → (J3); a whole-chunk namer allowed to drop → (J4); a `reclaiming` mark
  under a committed reference not blocking → (J5); the "another obligation" block removed → (J6);
  a drop without the judged records' bytes → (R); the marker written at the end of a lap that met
  an unreadable record → (K1); any `orphan:`-prefixed position accepted → (W1); the resume
  position not advancing past a malformed key → (W2); the read bound unchecked → (P3); the
  marker read without its generation → (T).
- **Production reach:** (a) production behaviour changes on deploy: until the cleanup has
  completed, GC reclaims **no** marked fragment (marked reclamation stops, it is not merely
  delayed; the docs say so plainly). On a store with no stale marks the cleanup completes in a
  few passes. (b) After a restore, marked reclamation stops again until the cleanup has walked the
  restored ledger; this relies on the post-restore pass running, as the runbook requires
  (`crates/server/src/cli.rs:1482-1494`). (c) A `reclaiming` mark under a live reference, or a
  staged-named stale mark, keeps the marker unwritten until it is resolved; both are named on the
  audit seam. The live path traverses all of it at Check (`reconcile_step` as deployed).
- **Citations expected:** Do must cite `path:line` on the target branch for every change.
  - **GC's judgement, to gate:** `judge` (`crates/custodian/src/gc.rs:651`), its safety gate
    (`:658-672`), the stamped arm and keyed lookup (`:686-707`, `retirement_draining` `:760-766`).
  - **Peer for "what names a position":** GC's two classes — the committed `ReferenceSet` built
    through the shared resolver (`referenced_fragments`, `gc.rs:1224`; note its whole-prefix read
    at `:1229`, which this child must not copy) and the staged `StagedSet` (`gc.rs:1361-1392`,
    built by `staged_fragments` `:1536`, paged, `:1510-1535`). Apply their rules (trusted vs
    untrusted placement, whole-chunk holds, unreadable records), reading in pages.
  - **Peer for the walk's position:** GC's ledger cursor (`is_resumable_cursor`,
    `gc.rs:2031-2037`, and `ORPHAN_KEY_MAX_LEN` `:2029`) — whose rule is too weak for this
    walk (W1), and whose comment explains why a malformed key can end a window (W2).
  - **Marks:** `orphan_key` (`crates/core/src/metadata.rs:73`), the three shapes and the
    `reclaiming` rule (`:128-169`), `encode_orphan_mark` `:285`, `decode_orphan_mark` `:305`.
  - **Obligations:** `retire_key` (`crates/core/src/multipart.rs:1651`), the payload table
    (`:4121-4139`), `RetiredMap` (`:3708`).
  - **Dispatch:** `reconcile_step`'s retirement leg (`crates/custodian/src/reconciliation.rs:159-172`).
  - **Restore:** `MPUFENCE_KEY` (`crates/core/src/multipart.rs:1312`); the post-restore pass opens
    a new fence generation as its first write (`crates/custodian/src/restore.rs:598-606`,
    `open_generation` `:918`).
  - **New record:** the marker (and any walk-position record) is new: state its key shape,
    writer, deleter and which scans see it (ADR-0046).
- **Prior-art check (triage cycles):** by path, on `f54db83`: no cleanup, marker or gate exists
  (`git grep -i 'orphan.identity\|cleanup complete'` under `crates/`: nothing); GC's keyed lookup
  came with #662 (merged), its reclaim intent with #812, its fragment-less sweep with #821/#823.
  The new test file does not exist. Rejected: #1014's three unpublished attempts (2026-11-10..18;
  `results/issue_1014/iteration-v1..v3`), whose findings are this brief's legs; #636's
  single-slice attempt; #637's first attempt (dropped a lapsed legacy mark whenever any `part:`
  record named the fragment).
- **Disposition hint:** new-feature

## Design

**The gate.** Until a valid marker is observed, GC treats every mark as protecting, `reclaiming`
marks included (its existing conservative arm). With a valid marker it judges as on the base.

**Per mark, in key order** — the lap reads a bounded window of marks, then the references that
name those positions (committed maps through the shared resolver, staged records by `StagedSet`'s
rules, pending `retire:bytes:` obligations), keeping per position only the evidence that decides
its verdict:

- undecodable ⇒ leave (ADR-0045 decision 3);
- `reclaiming` ⇒ leave (no writer replaces one, `metadata.rs:154-169`); **block** when the
  committed or staged class names the position; otherwise not blocking (GC finishes it once the
  gate opens, and the drain then completes, J5);
- structured, its own event's `retire:bytes:` obligation present ⇒ leave; **block** when the
  committed class, the staged class, or another obligation also names the position (J6);
- any other mark (legacy, or structured with its obligation gone):
  - named by a committed map or a pending obligation through a trusted placement ⇒ **drop**;
  - named by any staged record, or only whole-chunk (an untrusted placement) ⇒ **block**;
  - named by nothing ⇒ leave.

**No re-stamping.** A re-stamped mark would be the stale mark with a fresher date: no identity it
could carry is sure to be the event that next unreferences the fragment (plan review,
2026-11-11).

**Every drop is conditional.** Its batch requires the mark's exact bytes and the exact bytes of
every record the verdict read (the inode, the `seg:` record, the obligation). The inline `unlink`
and supersede delete the inode and put their mark in one batch
(`crates/core/src/metadata.rs:2143-2195`, `:2263-2308`), so a drop and an unreference cannot both
commit on the same reading.

**Completion.** The marker is written only by a lap that covered the whole ledger from its head,
judged every mark against a complete reference set (no unreadable record, GC's own refusal at
`gc.rs:527-536`), blocked on nothing, and saw every drop it attempted settle (committed, or
re-read and found moot). Anything else ends the lap with a typed outcome naming why, and a later
lap starts again from the head.

**The marker** names the restore-fence generation current when its lap began (`MPUFENCE_KEY`; an
absent record is a value too), its write requires that record's bytes, and it is valid only while
that generation is still current. Every post-restore pass opens a new generation first
(`restore.rs:598-606`), so a marker an image brings back is invalid. Do may choose another
mechanism with the same property and says why in `build-notes.md`.

**Docs currency** (`AGENTS.md:169-172`): `docs/design/architecture/06-runtime-view.md`, the GC
paragraphs (`:78-92`) only: the marker, what it gates, that marked reclamation stops until the
cleanup has completed, and that a restore stops it again until the cleanup has walked the
restored ledger. Add a paragraph rather than rewriting a long one; child-2 edits `:40-41`.

## Alternatives considered

- **Iteration 3's staged rules** (drop for an unpublished part, keep byte-identical for a
  published part awaiting its records drain). Needed the session phase, the `{parts}` ledger and
  undecodable-session rules, where most defects of rounds 1–3 sat. Rejected for blocking (above).
- **Re-stamp a mark on a still-referenced fragment** (0016's "re-stamps or drops"). Rejected:
  no sure identity (plan review 2026-11-11).
- **A marker valid for ever once written.** A restore brings back an old marker with the stale
  marks 0016 says a restore produces (`0016:1251-1259`). Rejected.
- **Reuse GC's committed reference set as read.** It reads `inode:` whole (`gc.rs:1229`) and
  fails past the scan cap; the iteration-3 sign-off requires paging (P1).

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR MAY
happen during the cycle (useful for CI feedback). The PR MUST NOT be marked ready before
sign-off accepts.
<!-- pdca:end child-1 -->

<!-- pdca:child child-2 -->
# custodian: committed repair and evacuation pre-mark their destinations (0016 X47) (636.6c-2) — issue 1014 child-2

> Child 2 of 3 of #1014's split. Do reads ONLY this file. Keep the `- **Label:** value` lines.
> The rule is normative in proposal 0016, `docs/design/proposals/draft/0016-multipart-commit-protocol.md`:
> read the pre-mark rule (`0016:1287-1312`), X47 (`0016:2587`) and X29 (`0016:2568`) first.
> Citations are verified on `f54db83` (#1013's PR head on `main` @ `cdc4047`); re-locate them on
> the base this bundle is given.

- **Slug:** committed-move-destination-premark
- **Track:** alpha
- **Kind:** enhancement (design proposal 0016, X47) / defect
- **Defect:** The committed-path moves — reconstruction's `repair_chunk`
  (`crates/custodian/src/reconstruction.rs:1103`) and rebalance's `evacuate_chunk`
  (`crates/custodian/src/rebalance.rs:516`) — write the destination fragment, then commit the
  repoint with a fresh legacy mark on each **vacated** position only
  (`reconstruction.rs:1226-1240`, `rebalance.rs:634-649`). They never read or touch the mark at
  the **destination** position. 0016 X47 requires the committed repoint to pre-mark its
  destination before writing the fragment and to adopt it in the repoint's commit under a
  precondition on that pre-mark (`0016:1287-1294`, `:2587`); the staged path does exactly that
  (`crates/custodian/src/reconstruction/staged.rs:557-576`, `:634-656`, `:721-751`), the
  committed paths do not. Three failures follow on the base: (1) a fragment moved back onto a
  server within grace (evacuate 5→2 marks `orphan:5:c:i`; a later move 2→5) stays referenced
  under that old mark, and once an unreference that writes no fresh mark comes (multipart
  Complete's supersede today; #1014's routed `unlink` next), GC reclaims it on the old stamp
  before the drain marks it; (2) a destination already carrying an expired mark can have the
  move's freshly written fragment reclaimed by GC before the move commits, after which the commit
  publishes a placement over missing bytes (0016 refutation outcome (c)); (3) a move that loses
  its commit leaves its written fragment unreferenced and unmarked, which GC keeps for ever
  (`gc.rs:740-742`).
- **Success criterion:** A NEW test file `crates/custodian/tests/move_destination_marks.rs`, over
  `RedbMetadataStore::in_memory()` and a placement-aware chunk-store double, driving repair and
  evacuation **through `reconcile_step`** with the contexts the deployed loop passes
  (`crates/server/src/custodian.rs:397`), with a topology that forces the destination server. It
  shows all of, for **both** movers, on a flat and on a segmented committed object:
  (D1) A stale legacy mark at the destination position before the move: after the move commits,
  no mark exists at any position the new placement names (rebuilt-in-place positions included),
  and the vacated positions carry their mark as today. (D2) The adversary's round trip:
  evacuate a fragment from server 5 to server 2, then, within grace, back to 5: after the second
  commit no mark exists at server 5's position. (D3) The consequence: after (D1) or (D2), a
  production multipart Complete supersedes the object; GC-only passes with the clock past the old
  stamp's grace reclaim none of its fragments before the drain marks them, and all are gone after
  those marks' grace. (D4) A destination whose mark is `reclaiming`, or undecodable, is never
  written: the move picks another server or aborts, and names it on the audit seam. (D5) GC
  records its reclaim intent on the destination's mark while the move is in flight (a wrapper
  runs a GC pass between the destination write and the move's commit): the move's commit fails,
  no placement names the reclaimed position, and a later pass repairs the chunk. (D6) A move that
  loses its commit (a wrapper changes the root first): its written destination fragment is
  reclaimed by GC after grace, not kept for ever.
- **Falsifiability:** RED is reachable in-process on `cargo test -p wyrd-custodian`, **on the
  base**, with existing APIs: (D1)/(D2) the destination mark survives, (D3) GC reclaims early,
  (D5) the commit lands over a reclaimed position, (D6) the fragment is held for ever.
- **Invariant to restore:** **C-1** (`docs/principles.md` §5 C-1, §6 row *storage lifecycle /
  reclamation*): every durable fragment is, at every instant, named by a record or evidenced for
  reclamation by a mark whose grace a reader can rely on; a mark never outlives, on a position a
  committed reference names, the event it evidenced; and no commit publishes a placement over a
  position GC has decided to reclaim (0016 outcome (c), `0016:2530-2535`). **Sources:** C-1
  (maintainer's rule 2026-07-25); 0016 X47/X29 and the pre-mark rule; `metadata.rs:154-169`
  ("once every move that re-places a chunk adopts under its mark's precondition, nothing can
  reference that position again before its key is gone"). SELF-TEST: it binds both movers and
  GC's reclaim intent; guarding one module cannot satisfy it.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Ordering note:** wave 1 of #1014's split, beside child-1 (no shared source file; this child's
  docs edit is confined to `06-runtime-view.md:40-41`, child-1's to `:78-92`). `1013` is added to
  `Depends on` after acceptance: the base's `ReconstructionContext` gains fields in #1013's stack
  (`crates/custodian/src/reconstruction.rs:142-155`) that this child's tests construct.
  Downstream: child-3 depends on this child (routing removes the inline re-stamp that today hides
  a stale destination mark). **Intake-cap override (wyrd-pdca-P1):** the human's 2026-11-18
  override for #1014's split.
- **Scope:** the committed-path repair and evacuation moves keep 0016 X47's destination rule:
  no mark older than the move survives on a position the move's new placement names, the move
  never publishes over a position GC has decided to reclaim, and a move that does not commit
  leaves what it wrote reclaimable; the docs. / out of scope:
  - the staged re-place (already keeps the rule) and #825's residuals (settling an aborted
    move's pre-marks), which apply here equally and stay #825's;
  - backfill and every other caller of `commit_chunk_map` that does not move a fragment;
  - GC, the cleanup and the marker (child-1); routing (child-3);
  - seeded DST for these interleavings (#682); `Cargo.lock`, `deny*.toml`, 0016, any ADR.
- **Repro instruction:** on `f54db83`: a committed RS(2,1) object, three servers in distinct
  domains, one fragment on server 5; put the legacy value `1000` at that fragment's `orphan_key`
  on server 2; mark server 5 draining so rebalance evacuates the fragment to server 2; after the
  pass, `orphan:2:<chunk>:<index>` still holds `1000` while the inode's placement names server 2.
- **Difficulty:** medium
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/custodian/tests/move_destination_marks.rs`, a **NEW** file (dry-run of
  `run-verify.sh --classify`: `ADDED_TEST`; the only crate touched is `crates/custodian`).
- **Verification posture:** default: a flippable test, red on the base through existing APIs
  (keep the file free of anything this child adds). **Demonstrated reds (recorded in
  `build-notes.md`):** destination mark left in place → (D1)/(D2); the destination's mark not
  required at the commit → (D5); a `reclaiming` destination accepted → (D4).
- **Citations expected:** Do must cite `path:line` on the target branch for every change.
  - **The peer to mirror:** the staged re-place — reading the destination's mark and refusing a
    `reclaiming` or undecodable one (`position`, `crates/custodian/src/reconstruction/staged.rs:557-576`),
    the pre-mark commit before any fragment write (`:634-656`, its event `move_event` `:805`), and
    the adoption that requires and deletes the pre-mark and marks the vacated position
    (`:721-751`).
  - **The two committed movers:** `repair_chunk` (`crates/custodian/src/reconstruction.rs:1103`,
    writes `:1226-1230`, vacated marks `:1232-1237`, commit `:1240`); `evacuate_chunk`
    (`crates/custodian/src/rebalance.rs:516`, copies `:634-640`, vacated marks `:642-647`,
    commit `:649`); the repoint they share (`metadata::repoint_chunk`).
  - **GC's side:** reclaim intent before deletion (`crates/custodian/src/gc.rs:768-790`); the
    no-mark arm that keeps an unmarked fragment (`:740-742`).
  - **Docs:** `docs/design/architecture/06-runtime-view.md:40` (Reconstruct) and `:41`
    (Rebalance) only.
- **Prior-art check (triage cycles):** by path, on `main`: `reconstruction.rs` (659ed31,
  9599aad, 19cec1a, 5377850) and `rebalance.rs` (f481cd3, 0538720, c40948a): none touches a
  destination mark on the committed path; the staged path got the rule in 5377850 (#826). Open:
  #825 (staged pre-mark settling, related, not overlapping). Tracker search for "destination
  mark" / "stale mark repair rebalance": nothing. Raised by #1014's iteration-2 adversary and
  reproduced through `metadata::repoint_chunk` by its iteration-3 reviewer.
- **Disposition hint:** likely-fix

## Docs currency

`AGENTS.md:169-172`: `06-runtime-view.md` steps 2 and 3 (`:40-41`) say the move marks its
destination before writing it and clears that mark in the commit that adopts the fragment, so a
mark never outlives its event on a referenced position.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR MAY
happen during the cycle (useful for CI feedback). The PR MUST NOT be marked ready before
sign-off accepts.
<!-- pdca:end child-2 -->

<!-- pdca:child child-3 -->
# core,custodian: route unlink and supersede through one retirement obligation behind the cleanup marker (636.6c-3) — issue 1014 child-3

> Child 3 of 3 of #1014's split. Do reads ONLY this file. Keep the `- **Label:** value` lines.
> The design is normative in proposal 0016,
> `docs/design/proposals/draft/0016-multipart-commit-protocol.md`: read **decision 4** rule 1
> (`0016:1079-1106`, the routing) and the batch budgets before writing code. Citations are
> verified on `f54db83` (#1013's PR head); this child's base also carries child-1 (the cleanup
> marker and its gate reading) and child-2 (destination pre-marks), so re-locate every anchor.
>
> **The two rules this child keeps:** the routing flips only where the deployed custodian already
> drains (#1062, on the base), and only while child-1's marker is valid (0016 X111). The
> discontinued #636 patch flipped the routing with no drain dispatched: deleting a 30-chunk
> RS(6,3) object left all 270 fragments on disk for good.

- **Slug:** retirement-routing-unlink-supersede
- **Track:** alpha
- **Kind:** enhancement (design proposal 0016, decision 4)
- **Defect:** `unlink` writes every orphan mark inline in its delete batch and refuses a segmented
  object (`crates/core/src/metadata.rs:2143-2195`, refusal `:2174-2179`);
  `commit_chunk_map_superseding{,_leased}` do the same for the prior map (`:2263-2308`,
  `:2462-…`). Past 55 chunks at RS(6,3) that batch exceeds the protocol's `MAX_BATCH_OPS` (500,
  `crates/core/src/multipart.rs:5926`): a 100-chunk object's unlink is 904 operations, a maximal
  object's marks exceed the transaction envelope (`0016:1091-1096`), and a segmented object
  cannot be deleted or overwritten at all. 0016 moves this fan-out to one
  `retire:bytes:g:<inode>:<version>` obligation the drain marks (`0016:1081-1106`), as multipart
  Complete already does for the prior it supersedes (`crates/core/src/multipart/complete.rs:1904-1911`).
  Round 3 of #1014 found three ways such an obligation goes wrong, all also live in Complete's
  path today: a segmented generation retired by its group alone, with a `seg:` row missing,
  drains "complete" across the hole and leaks that row's fragments for good; a flat chunk with a
  wrong-length placement becomes an obligation the drain refuses for ever
  (`Rule::Committed`, `crates/custodian/src/retirement/marks.rs:940-945`), after the inode that exposed it
  is gone; and GC's expired-lease arm reclaims a fragment that is unreferenced and not yet marked
  at once, with no grace, when its chunk holds an expired `pending:` entry
  (`crates/custodian/src/gc.rs:713-739`, under `ExpiredPendingPolicy::Reclaim`, `:367-375`).
- **Success criterion:** A NEW test file `crates/custodian/tests/retirement_routing.rs`, over
  `RedbMetadataStore::in_memory()` and a placement-aware chunk-store double, driving the cleanup,
  the drain and GC **through `reconcile_step`** with the deployed contexts
  (`crates/server/src/custodian.rs:397`); deletes and overwrites through the production
  `metadata::unlink` and `write::commit_overwrite`; the marker made valid by running child-1's
  cleanup to completion, never by writing it by hand. It shows all of:
  **(Gt) The gate.** Without a valid marker, `unlink` and overwrite behave exactly as on the base
  (inline marks; segmented refused). A restore's new fence generation (or anything else that
  invalidates the marker) landing between the routing's gate read and its commit makes the commit
  fail with `Conflict` and remove nothing (a wrapper forces the interleaving).
  **(A) Positive reclamation, marker valid.** (A1) A committed 30-chunk RS(6,3) object (274
  operations inline, which fits one batch) deleted with `unlink`: after passes past grace, all 270
  fragments are gone. (A2) A committed flat 100-chunk RS(6,3) object deleted with `unlink`:
  **before any pass**, its batch installed exactly one `retire:bytes:g:<inode>:<version>` and no
  `orphan:` mark; after passes past the drain's grace, all 900 fragments are gone and no
  `retire:` or `orphan:` record of it remains. **Boundary, preconditions counted:** an 83-chunk
  RS(4,2) object (498 marks + 2 deletes + 2 preconditions = 502; 500 if preconditions were
  ignored) also takes the obligation path. (A3) The same as A2 for an overwrite
  (`commit_overwrite`, and `commit_chunk_map_superseding` direct), for the prior's fragments; and
  for `unlink` and overwrite of a **segmented** object published by the production Complete.
  **And present while they must be:** in A2 and A3, GC-only passes between the unreference and
  the drain's marks reclaim none of the object's fragments, and passes inside the drain's grace
  reclaim none; only later passes leave them gone.
  **(S) A segmented object missing a `seg:` row** (the middle one; the last one): `unlink`,
  overwrite **and** a production Complete over it each refuse with a typed error and change
  nothing; no obligation is installed; GC still reports the hole as before.
  **(L) A flat object with a wrong-length placement**, large enough to route: `unlink` and
  overwrite refuse with a typed error before removing the inode (or retire it in a way the drain
  finishes — Do's choice, stated); the same for a Complete superseding it.
  **(E) Expired lease:** an object whose `pending:` lease was never released (release omitted),
  GC under `ExpiredPendingPolicy::Reclaim`: after a routed delete, and after a Complete
  supersede, GC reclaims none of its fragments before the drain's marks and their grace. The
  docs state what this costs unrelated expired-lease garbage while a retirement is pending.
- **Falsifiability:** RED is reachable in-process on `cargo test -p wyrd-custodian` (and
  `-p wyrd-core`). (A2) is red on the base (the base, which carries child-1, unlinks with 904
  inline marks): a real C4-verify red. (A2)/(A3) are also red on any build where the routing flips
  but `reconcile_step` does not dispatch the drain (the #636 defect). (S) is red on the base for
  Complete (it installs the obligation; the row's fragments leak). (E) is red on the base for
  Complete (the expired-lease arm reclaims at once).
- **Invariant to restore:** **C-1** (`docs/principles.md` §5 C-1, §6 row *storage lifecycle /
  reclamation*): every durable fragment is, at every instant, named by a record or evidenced for
  reclamation: a delete or overwrite never leaves a fragment unnamed (the obligation names it
  until the drain marks it), no GC arm reclaims an owed fragment before the drain's mark and its
  grace, every installed obligation names exactly what the removed map named (no hole, nothing
  the drain cannot finish), and the routing is taken only under a valid marker. **Sources:** C-1
  (maintainer's rule 2026-07-25); 0016's refutation standard (`0016:2530-2535`), decision 4 rule 1
  (`0016:1079-1106`), X111 (`0016:2650`); the rubric's "absent entries: explicit error, never
  silent success". SELF-TEST: it binds core's routing, Complete's obligation, GC's arms and the
  drain; no single module can satisfy it.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** child-1, child-2
- **Ordering note:** wave 2 of #1014's split. child-1 provides the marker this routing gates on
  (0016 X111); child-2 makes repair and evacuation clear destination marks, without which routing
  regresses `unlink`/overwrite (the inline re-stamp it removes hides a stale destination mark
  today). `1013` is added to `Depends on` after acceptance (the drain dispatch, #1062 via #1013).
  **Downstream:** #668, #669, #670 replace `1014` with this child's id in `Depends on`.
  **Intake-cap override (wyrd-pdca-P1):** the human's 2026-11-18 override for #1014's split.
- **Scope:** with a valid marker, `unlink` and `commit_chunk_map_superseding{,_leased}` retire a
  removed map through one generation obligation whenever the inline batch would exceed one batch's
  budget, and always for a segmented map; every generation obligation (theirs and Complete's)
  names exactly what the removed map named or the operation refuses; no GC arm reclaims an owed
  fragment before the drain's mark and grace; the docs. / out of scope:
  - the cleanup, the marker and GC's mark gate (child-1); move destination marks (child-2); the
    drain (#1062);
  - bulk `DeleteObjects` batching (#509, inside `0016:1107-1122`); the S3 verbs (#668–#670);
  - seeded DST (#660); any edit to 0016 or an ADR (ADR-0045's refinement is #811); `Cargo.lock`
    and `deny*.toml`.
- **Repro instruction:** on the base: commit a flat 100-chunk RS(6,3) object and `unlink` it:
  the batch carries 900 inline `orphan:` puts plus 2 deletes and 2 preconditions (904 > 500); a
  segmented object's `unlink` returns `SegmentedMapUnsupported` (`metadata.rs:2174-2179`).
- **Difficulty:** high
- **Do model:** opus
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/custodian/tests/retirement_routing.rs`, a **NEW** file (dry-run of
  `run-verify.sh --classify`: `ADDED_TEST`; crates `crates/custodian`, `crates/core`).
- **Verification posture:** default: a flippable test; the base carries child-1, so the file may
  use child-1's types and still compile on the red leg. **Demonstrated reds (recorded in
  `build-notes.md`):** drain dispatch removed with routing left on → (A2); the budget counting
  mutations only → (A2)'s boundary; the gate pin dropped from the routed batch → (Gt); the row
  check removed → (S); the expired-lease protection removed → (E).
- **Production reach:** (a) production behaviour changes once the marker is valid: every delete
  and overwrite above one batch's fragments, and every segmented one, installs an obligation the
  deployed custodian drains; reclamation moves from the request to the drain (never earlier than
  today, `0016:1098-1104`). (b) Segmented objects become deletable and overwritable. (c) A damaged
  object (missing row, malformed placement) is now refused loudly by delete, overwrite and
  Complete instead of leaking. The live path traverses all of it at Check.
- **Citations expected:** Do must cite `path:line` on the target branch for every change.
  - **Routing:** `metadata::unlink` (`crates/core/src/metadata.rs:2143-2195`),
    `commit_chunk_map_superseding` (`:2263-2308`), `_leased` (`:2462`), `write::commit_overwrite`
    (`crates/core/src/write.rs:323`).
  - **The peer that already installs this obligation:** Complete's `superseded`
    (`crates/core/src/multipart/complete.rs:1904-1911`), its `require_absent(retire:bytes:g:…)`
    row (`:164`) and its install (`:1812-1845`); `RetireObligation::superseded_generation`
    (`crates/core/src/multipart.rs:4713`), `RetiredMap` (`:3708`), `retire_key` (`:1651`).
  - **Budgets:** `MAX_BATCH_BYTES` (`multipart.rs:5887`), `MAX_BATCH_OPS` (`:5926`).
  - **The drain's rule the obligation must satisfy:** `Rule::Committed` in
    `crates/custodian/src/retirement/marks.rs`; segmented range exhaustion in the same file.
  - **GC's arms:** expired-lease (`crates/custodian/src/gc.rs:713-739`), `ExpiredPendingPolicy`
    (`:367-375`).
  - **The gate:** child-1's marker reading, on this child's base.
- **Prior-art check (triage cycles):** by path: `metadata.rs`/`write.rs` on `main` route nothing
  (unlink still inline, `:2174-2179` refusal); Complete's obligation came with #1008/#1009
  (merged 2026-11-12). The new test file does not exist on `f54db83`. Rejected: #636's routing
  flip without dispatch; #1014's three unpublished attempts, whose routing was reviewer-confirmed
  red→green but carried the (S), (L) and gate-pin defects this brief now tests.
- **Disposition hint:** new-feature

## Design

**Routing** (`0016:1081-1106`). With a valid marker, `unlink` and the superseding commits remove
the references and install **one** `retire:bytes:g:<inode>:<version>` in the same batch, under
`require_absent` on it and a precondition that pins the marker's validity, instead of inline
marks. A small flat object keeps the inline path when its **whole** batch fits — preconditions plus
deletes plus mark puts against `MAX_BATCH_OPS`, its bytes against `MAX_BATCH_BYTES`; above that,
and for every segmented object, the obligation is the only path. The drain re-reads each `seg:`
record's current placement at drain time.

**What the obligation must name.** Before removing the root, the operation proves the obligation
names everything the removed map named: for a segmented map, every row its root's table
expects; for a flat map, a placement the drain accepts for each chunk. Otherwise it refuses with a
typed error and changes nothing. Complete's superseded prior goes through the same proof.

**Expired leases.** A fragment an obligation owes and the drain has not marked yet is not
reclaimed by the expired-lease arm. Do picks the mechanism, keeps GC's per-pass memory bounded
(no expansion of the `retire:` ledger into memory, `0016:1236-1242`), and documents what it costs
unrelated expired-lease garbage while a retirement is pending (the iteration-3 review called a
blanket deferral a conservative trade-off; T4 called it unbounded under a permanently held
obligation — say which holds and why).

**Docs currency** (`AGENTS.md:169-172`): `06-runtime-view.md`, the GC paragraphs: deletes and
overwrites install a retirement obligation the custodian drains, marking before deleting; grace
starts at the drain; the refusals; the expired-lease rule.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR MAY
happen during the cycle (useful for CI feedback). The PR MUST NOT be marked ready before
sign-off accepts.
<!-- pdca:end child-3 -->
