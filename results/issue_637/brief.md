# Brief — issue 637 / staged-byte-protection (split parent)

> Plan artifact for the **parent** of a split. The work is carried by four children — #661,
> #662, #663, #664, existing sub-issues of this one (637.1–637.4). Their briefs are
> `results/issue_661/brief.md` … `results/issue_664/brief.md`: materialised from
> `split-proposal.md` at acceptance (2026-09-12) and authoritative over it; #661's was amended by
> this brief's plan-review pass (see the end). This brief states the whole outcome once, names
> the owner of every part of it (the coverage table below), and is never built as one patch.
> Re-planned 2026-09-12 after iteration v1 (`iteration-v1/`) was sent back at sign-off as too
> big for one slice (334 KB across 20 files, several unrelated safety gaps, one unsettled
> design call). Revised the same day over `plan-advisory-plan-reviewer.md`.

- **Slug:** staged-byte-protection
- **Kind:** enhancement (split parent)
- **Defect:** four distinct defects, one per child. They read one shared reference set, but each
  fails on its own and each child brief states its own.
  (1) **The `orphan:` ledger cannot be read at size, and some marks are never visited (#661).**
  GC and restore read the whole ledger with one `scan` (`crates/custodian/src/gc.rs:522-538`,
  `crates/custodian/src/restore.rs:308`), which fails outright past `SCAN_CAP`
  (`crates/traits/src/lib.rs:286`). A mark over a position with no fragment is never visited,
  because GC consumes marks only while iterating `list_fragments()` (`gc.rs:183-219`). This is a
  ledger defect, not a staged-byte one. It is in 637 because the tracker scopes it here ("GC,
  ledger walk") and because one maximum segmented-object retirement would install ~1.78 M marks
  (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:1392-1398`, X90 at `:2619`).
  (2) **Staged bytes are in no protected class, and GC destroys before it records (#662).**
  `ReferenceSet` holds committed placements only (`gc.rs:265-340`), so GC reclaims a marked staged
  fragment, and restore marks a live upload's fragments stranded (its gate is the same predicate,
  `restore.rs:383`). A pending byte retirement protects nothing. In the same reclaim decision sit
  two ledger-protocol defects: GC deletes the fragment (`gc.rs:214`) before it commits the key
  cleanup (`gc.rs:231`), and only the bare-decimal mark value decodes (`gc.rs:526-535`).
  (3) **Staged redundancy is never maintained (#663).** Scrub walks committed placements only
  (`crates/custodian/src/scrub.rs:88`), and reconstruction drops a repair obligation for a chunk
  it finds in no committed map (`crates/custodian/src/reconstruction.rs:613`).
  (4) **Drain status and restore claim more than is true (#664).** A drain reports `Satisfied` for
  a server holding only staged bytes (`crates/custodian/src/desired_state.rs:191-196`), and restore
  fences no resurrected session and records no fence generation (`0016:717-728`).
  Source: proposal 0016 decision 2 (`0016:765-893`) plus the ledger rules decision 2 depends on
  (`0016:1189-1404`).
- **Success criterion:** two checks. **(1) The split is complete — checked at this parent's
  sign-off.** Every row of the coverage table below names an owner: a child leg, an
  already-merged change, or a named deferral with its reason. Each child leg a row cites exists in
  that child's brief as a red→green assertion on the child's own NEW test file. A row with no
  owner, or a cited leg the child brief does not contain, fails the split.
  **(2) The outcome — checked once the last wave has landed on `main`.**
  `cargo test -p wyrd-custodian --test gc_ledger_walk --test staged_protection --test staged_repair --test staged_drain_restore`
  passes, and `cargo xtask ci` is green; `ci` also runs #663's seeded X29 case under
  `--cfg madsim` through `run_dst` (`xtask/src/main.rs:1567`). What those files pin, one line per
  child: **#661** GC walks the ledger in pages, exactly `B` entries per pass while `B` remain, and
  resumes where the last pass stopped even though each pass builds a fresh `GcContext`, so every
  mark is visited within `⌈P / B⌉ + 1` passes (legs B, C); restore never reads the ledger whole and
  never re-stamps a mark it did not read — an existing mark on any page keeps its bytes (leg F);
  a mark with no fragment is swept only past the late-write deadline, and only on this pass's own
  listing (leg D). **#662** GC and restore protect staged bytes through the shared reference set;
  a pending byte retirement protects its fragments; reclamation intent is durable before bytes
  are destroyed; all three mark value shapes decode. **#663** scrub checks committed-part fragments
  and queues repair; reconstruction repairs a staged chunk under the pre-mark and session-fence
  rules. **#664** drain status counts staged bytes as held; restore fences every resurrected
  session (a `Completing` one with its segment records' deleter in the same batch) and writes the
  fence-generation record. The parent ships no patch. **This is not all of tracker #637:** the
  seeded DST races, bar X29, stay with #665 (coverage table), so the tracker issue stays open
  until #665 lands.
- **Falsifiability:** the parent has no red of its own; it ships no code. Check (1) fails on a
  table row with no owner, or on a cited leg missing from its child brief — a read of five files.
  Check (2)'s reds are the children's: each child's NEW test file fails by assertion on its own
  wave's base (`main` for #661; `main` + #661 for #662; `main` + #661 + #662 for #663 and #664),
  and each child brief's `Falsifiability` lists which of its legs go red there and which are
  guards. On today's `main` (`3969a3a`) none of the four test files exists.
- **Invariant to restore:** every durable byte is, at every instant, classifiable as
  committed-referenced, staged-with-a-named-exit, or garbage-with-a-sound-reclamation-path, and
  every maintenance pass acts on that classification rather than on the absence of one. Source:
  0016 invariant (2) (`0016:869-871`) as decision 2 instantiates it per consumer
  (`0016:820-871`); the custodian's rule that a referenced fragment is never reclaimed
  (proposal 0005, `docs/design/proposals/accepted/0005-milestone-3-custodians.md:294-296`); ADR-0045 (a maintenance loop never rewrites metadata it cannot parse).
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** 634, 691, 715, 716, 771, 772
- **Ordering note:** all six prerequisites are COMPLETE and merged (`origin/main` @ `3969a3a`).
  **#636 is discharged for this work, not dropped.** The tracker names #636 ("the multipart record
  seam") because nothing is staged until `sidx:` / `part:` exist. #636 was split (636.1–636.7,
  #654–#660), and #654 — the record family — split again. The record types this work reads landed
  in its leaves: #691 key grammar (PR #703), #715 the `mpuctl` budget/admission records (PR #724),
  #716 session, slot and part records (PR #725), #771 retire obligations (PR #792), #772 owned
  `sidx:` entries (PR #793). #636 stays open for its protocol slices (#655–#660); none of them is
  needed here, because every staged record in the children's tests is seeded by hand. #634 is the
  `scan_page` seam. The children are **not** in `Depends on`: the dependency runs the other way.
  They build after the split, and `split-lineage.json` names them (each child's own
  `split-lineage.json` names 637 as parent). The children run as three waves: #661, then #662,
  then #663 and #664 together; each child brief carries its own `Depends on` / `Conflicts with`.
  **Intake cap (wyrd-pdca-P1):** the split was authored over the cap on an explicit override by
  the human in this Plan session (2026-09-12), for 637 only, at
  `planned 21/6 (cap) — room for 0, need 1: Plan intake closed`; this brief made 637 itself PLANNED
  (22/6), and accepting materialised the four existing sub-issues, taking the count to 26. The
  plan-review pass added no bundle. **#665** (637.5, DST race cases) stays open as #637's fifth
  sub-issue and is not materialised: it depends on #659, which has no bundle yet.
- **Surfaces:** data
- **Difficulty:** high
- **Do model:** opus-max
- **Scope:** proposal 0016 decision 2 across the custodian — the staged protection class and
  each consumer's stated answer (GC, restore, scrub, reconstruction, rebalance, drain status) —
  plus the `orphan:` ledger rules it depends on (paged walk, marks with no fragment,
  reclamation intent before destruction, the three value shapes, keyed protection while a byte
  retirement is pending), and restore's session fence. / out of scope: the seeded DST sweeps
  other than X29, and the full-plane observable (#665, after #659); the orphan-identity
  migration gate and its cleanup pass (X92, `0016:1249-1273` — it guards the retirement paths
  #659 turns on); the retire drain (#659); the reaper (#625); the S3 verbs (#508); upload-side
  placement and its drain fence (#657, X59); evacuation of committed segmented objects
  (#653/#722); any edit to 0016 or to an ADR.
- **Repro instruction:** on `origin/main`: (a) seed an `Open` session (`mpu:`) with one `part:`
  record whose fragment sits on a D server, give that fragment an `orphan:` mark older than the
  grace window, and run one GC pass — the fragment is deleted (#662's repro); (b) seed more
  `orphan:` keys than a store double's lowered `scan` cap and run one GC pass — `orphan_leases`
  returns `ScanCapExceeded` and the pass errors (#661's repro). Each child brief has its own.
- **External dependencies:** none
- **Test file:** n/a at the parent — each child ships its own NEW file:
  `crates/custodian/tests/gc_ledger_walk.rs` (#661), `crates/custodian/tests/staged_protection.rs`
  (#662), `crates/custodian/tests/staged_repair.rs` (#663),
  `crates/custodian/tests/staged_drain_restore.rs` (#664); plus #663's seeded X29 case appended
  to the existing `crates/dst/tests/custodian.rs`.
- **Citations expected:** per child.
- **Prior-art check (triage cycles):** searched by path (`crates/custodian/src/`,
  `crates/core/src/multipart.rs`) across merged history and open PRs: no merged change adds a
  staged reference class or pages the `orphan:` ledger (`git log -S'sidx' -- crates/custodian/src`
  is empty on `origin/main`); no open PR touches `crates/custodian/`. Rejected prior art: this
  issue's own iteration v1 (`iteration-v1/`, sent back as oversized) and #508's 4th and 7th
  attempts (a resolver GC and restore never used; an unbounded `loop { scan_page }` into one
  `HashMap`).
- **Disposition hint:** likely-fix

## Coverage — every outcome, and its owner

Kinds: **leg** — a deterministic regression in the child's NEW test file, driving one chosen
interleaving through a store-double hook, red on the child's base by assertion; **DST** — a seeded
madsim sweep in `crates/dst/tests/custodian.rs`, run by `cargo xtask ci`; **merged** — already on
`main`; **deferred** — owned outside the four children, for the reason given.

**Tracker acceptance** (`notes.json`, "Acceptance"):

| Tracker outcome | Owner | Kind |
|---|---|---|
| staged fragments survive GC, and restore marks none of them stranded | #662 A (GC), B (restore, `stranded_marked`) | leg |
| …survive scrub, reconstruction and rebalance | #663 A–C; #664 D | leg |
| …the same as one full-plane reconcile step with `stranded_marked == 0` | #665 (its body's "full-plane observable") | deferred — needs #659 |
| a corrupt staged fragment enqueues repair, and the repair updates the part placement | #663 A, B | leg |
| rebalance's answer is disjoint from the staged set | #664 D | leg |
| `desired_state` counts in-flight owned fragments as held | #664 A (in-flight), B (committed part), C (guard) | leg |
| the ledger walk is bounded per pass; a population past `SCAN_CAP` is processed without being materialised whole | #661 A, B, C, F | leg |
| seeded DST race: staged re-place vs session fence (X29) | #663 C (leg), F (DST) | leg + DST |
| seeded DST race: drain request vs upload intent (X59) | the fence and its mutant tests: #657; the seeded race: #665 | deferred — #665 needs #659, which needs #657 |
| `cargo xtask ci` green | every child, leg (G/H/G/K) | — |

**Tracker "What" items the acceptance list does not name:**

| Item | Owner | Kind |
|---|---|---|
| GC resolves segmented maps by bounded `seg:` ranges | PR #683 (#649/#650): `gc.rs:337-340`, `:402` | merged |
| restore's `pending:` scan bounded again | #772 (PR #793) moved owned entries to `sidx:`, so `restore.rs:770` no longer grows with uploads | merged |
| reclamation intent before destruction; three value shapes | #662 F, G | leg |
| fragment-less mark sweep | #661 D | leg |
| restore fences resurrected sessions | #664 F, G, H, I | leg |

**0016's required cases in decision 2's domain** (`0016:2876-2907`; "Tier-0 DST is the
correctness authority", `:2878`):

| Case | Owner | Kind |
|---|---|---|
| X17 restore fence, `Open` (`:2545`) | #664 F, I | leg; seeded form with #665 or #660, settled when #665 is briefed |
| X57 `Completing`-with-segments restore fence (`:2587`) | #664 G, H | leg; seeded form as X17 |
| X29 staged re-place vs session fence (`:2558`) | #663 C, F | leg + DST |
| X59 drain request vs intent (`:2588`) | #657 (fence), #665 (seeded race) | deferred |
| X61 adoption CAS vs GC-deleted destination (`:2590`) | #662 F(iii) | leg |
| X67 inode-before-`part:` handoff (`:2596`) | #662 C(ii) | leg |
| X75 legacy `orphan:` value (`:2604`) | #662 G | leg |
| X86 crash between the `reclaiming` CAS and the delete (`:2615`) | #662 F(iv) | leg |
| X87 fragment-less mark sweep (`:2616`) | #661 D(i)–(iv) | leg |
| X88 paused worker's stale pre-mark (`:2617`) | #663 D(v) | leg |
| X90 `orphan:` pagination, custodian side (`:2619`) | #661 A–C, F | leg |
| X90 backend-conformance scale case: one maximum segmented retirement past `SCAN_CAP` (`:2905-2907`) | #665 | deferred — the marks come from #659's retirement routing; #665's brief must add it, as its tracker body does not name it |
| X91 sweep vs pre-mark (`:2620`) | #661 D(ii) + E (deadline includes `W_repoint`, strictly inside grace) with #663 D(ii), D(v) | leg; no single leg runs both, the seeded composition is #665's |
| X96 stale-listing sweep (`:2625`) | #661 D(i), D(v) | leg |
| X97 keyed pending-retirement protection (`:2626`) | #662 E | leg; the backed-up drain itself is #659's |
| X63, X92 stale marks and the identity migration gate | #659 | deferred (out of scope) |

The legs are regressions of one chosen interleaving each, not seeded sweeps. Apart from X29, the
seeded sweep for each row belongs to #665, whose tracker body owns "the seeded DST cases proposal
0016 requires of decision 2". That is a stated gap in this split, carried by #665, not a claim
that the four children close it.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Plan-review response (revision pass, 2026-09-12 — `plan-advisory-plan-reviewer.md`)

Five findings. **All five revised the brief; F2 also amended child #661, and F1 and F4 each
keep one point that stands.** Claims re-run against `origin/main` @ `3969a3a` and the live
tracker. The reviewer's sandbox held only this brief and `notes.json`, so it could not see the
child briefs; this revision makes the parent readable without them.

- **F1 — no self-contained gate.** *Revised.* The criterion is now two checks: split
  completeness at this sign-off (the coverage table, a row with no owner fails), and one
  aggregate command for the outcome after the last wave. Falsifiability and repro are concrete.
  *Stands:* #661–#664 are not added to `Depends on`. They are this parent's children, built after
  the split, and listing them would hold the parent behind work it spawned; `split-lineage.json`
  is their record.
- **F2 — a budgeted partial read could re-stamp an off-page mark.** *Revised, and the finding is
  right.* Restore writes a fresh stamp for any fragment missing from its read
  (`restore.rs:413-416`, `:426-429`), and #661's leg F did not force an existing mark off the first
  page, so a one-page restore could pass it. #661 leg F now places those marks after the first `B`
  keys and the first lowered-cap's worth, with old stamps, and requires their bytes unchanged.
  #661's invariant now forbids overwriting as well as deleting on a partial read. #661 leg D
  gained case (v) for the stale listing (X96). GC's cursor semantics were already pinned (#661 B:
  exactly `B` per pass, `⌈P / B⌉` passes; C: the tail within `⌈(head + tail) / B⌉ + 1` passes,
  fresh `GcContext` each time); the parent now states them, and no longer says restore works under
  the per-pass budget.
- **F3 — the seeded DST commitment was dropped.** *Revised.* The coverage table maps every case
  the reviewer named, and says which are deterministic legs and which seeded sweeps. It corrects
  the old claim that X59 "moved to #657": #657 owns the fence, not the seeded race, which stays on
  #665. The X90 conformance scale case is named as #665's, with the note that #665's body lacks it.
  The criterion now says tracker #637 stays open until #665 lands.
- **F4 — the ledger changes are a separate defect.** *Revised:* the Defect field now lists four
  distinct defects, each with its child. *Stands:* the reclaim-intent ordering and the value
  shapes stay in #662. They change the same reclaim decision in `gc.rs` as staged protection, and
  X97's keyed protection reads the structured value; a child of their own would sit on the same
  file in a wave of its own and cost a cycle.
- **F5 — #636 replaced without saying so.** *Revised.* The ordering note traces #636 → #654 → the
  five merged leaves, with their PRs, and says why #636's still-open protocol slices are not needed.
