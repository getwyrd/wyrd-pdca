# Brief — issue 711 / repoint-chunk-segmented-placement-moves

> **This bundle ships no code.** #711 was decomposed at Plan on 2026-08-10 into **#721** and
> **#722**, both filed as GitHub **sub-issues** of #711 and materialised as their own bundles.
> This brief is the parent's Plan artifact: it records the decomposition, maps every element of
> #711's original scope onto a child (or onto a recorded disposition), and hands the human the
> one decision left — confirm the split, and choose what happens to the tracker issue.
> `close-disposition: split` is already on disk, so the driver skips the builder and reviewer
> leaves and routes straight to sign-off (`driver.py:68-72`); the gate matrix lands N/A by
> construction, with no gate command executed (`gates.py:153-171`).
>
> Authoritative detail lives in `split-proposal.md` (this bundle) and in the two child briefs.
> This file does not restate them.

- **Slug:** repoint-chunk-segmented-placement-moves
- **Defect:** **A chunk that lives in a `seg:` record can never be repaired or evacuated.**
  #695/#696/#697 stopped the maintenance passes aborting on a segmented object, but they write
  nothing: a repair obligation or a drain evacuation for a `seg:`-resident chunk is refused and
  stays queued, every pass, forever, because the only placement writers in the tree rebuild an
  **inode** record and can address a `seg:<nonce>:<epoch>:<index>` record not at all. That defect
  is real and unfixed — it is now carried by **#721** (repair) and **#722** (evacuation). Nothing
  is fixed by THIS bundle.
- **Success criterion:** the decomposition is **complete and correct**: every element of #711's
  original scope is either carried by a filed, briefed child or explicitly dispositioned in the
  mapping below; both children exist as tracker sub-issues of #711 and as `PLANNED` bundles; and
  no element is left unassigned. Verified at Plan (2026-08-10) and re-checkable at sign-off with
  two commands — `gh api graphql -f query='{repository(owner:"getwyrd",name:"wyrd"){issue(number:711){subIssues(first:10){nodes{number title state}}}}}'`
  and the scope mapping in §"What each child carries" read against each child's `Scope` / `Budget`
  file set.
- **Falsifiability:** the criterion goes RED if a parent-scope element maps to neither child nor to
  a recorded disposition, if a child's bundle or tracker issue is missing, or if the two children's
  file sets overlap (which would re-create the collision the split exists to remove). All three
  were checked at Plan: the sub-issue query returns #721 and #722 OPEN; the file sets are disjoint
  (#721 owns `core/src/metadata.rs` + `custodian/src/reconstruction.rs` + the two
  reconstruction-side tests, #722 owns `custodian/src/rebalance.rs` + the two rebalance-side tests
  + `dst/tests/custodian.rs`); one element is carried as a recorded withdrawal, called out below.
  **No gate can evaluate this** — a close-disposition bundle runs no gates — which is why it is a
  sign-off check, declared under `Verification posture`.
- **Invariant to restore:** none here — no code ships from this bundle. The invariant the WORK must
  restore (**a chunk's redundancy and its evacuability do not depend on which record shape holds
  its `ChunkRef`** — C-1, no permanent states, `docs/principles.md:137` §6 *Storage lifecycle /
  reclamation*, sourced to §5 C-1 at `:109`) is carried unchanged by #721 and #722 and stated in
  both child briefs.
- **Repo + branch target:** getwyrd/wyrd @ main   (inherited from the original slice and carried
  unchanged by both children. **No PR opens from this bundle** — publish exits 0 with *"nothing to
  contribute; close the tracker item by hand"* (`publish.py:161-166`), so the tracker action is the
  human's, at sign-off.)
- **Ordering note:** no `Depends on` / `Conflicts with` — deliberately. This bundle touches no file
  and builds nothing, so it can neither block nor collide with anything. The ordering lives in the
  children: **#721 is wave 0**; **#722 `Depends on: 721`** (it consumes the `repoint_chunk`
  primitive #721 authors and cannot compile without it); **both carry `Conflicts with: 717`**,
  which #717's own brief reciprocates (`Conflicts with: 710, 721, 722`). Every external
  prerequisite of the original slice is already merged into `origin/main @ 92e1b4b` — #710 as PR
  #718, #695/#696/#697 as PRs #704/#705/#706 — so neither child needs a `Depends on (merged):`.
- **Surfaces:** data
- **Difficulty:** low
  — zero files changed; the blast radius of a disposition record. The original slice's `high`
  is carried by #721; #722 is `medium`.
- **Scope:** record the decomposition of #711 and close the parent — this brief, plus the human's
  sign-off decision. / **out of scope:** any implementation of the repoint primitive or its callers
  (that is #721 and #722, and reopening this bundle to a fix path would rebuild the very
  1595-line shape the split exists to abandon); re-litigating the split itself; re-opening the
  withdrawn dedup finding recorded below.
- **Repro instruction:** confirm the split state rather than the defect — the defect's repro lives
  in the child briefs. On this bundle: `close-disposition` reads `split`; `split-proposal.md`
  carries the two children; `iteration-v1/` holds the abandoned attempt (7 files / 1595 added
  lines / 124 KB against the brief's hard ≤6 files / ≤450 semantic lines — `iteration-v1/size-signal.json`)
  and its sign-off (`iteration-v1/SUMMARY.md` §9, outcome `iterated-to-Plan`, Eduard Ralph /
  2026-08-10). On the tracker: #721 and #722 are OPEN sub-issues of #711.
- **External dependencies:** none
- **Test file:** none — no code ships from this bundle, so there is no test to flip. The regression
  tests for the work live in the children: `crates/custodian/tests/segmented_map_repoint.rs` (#721,
  new) and `crates/custodian/tests/segmented_map_evacuate.rs` (#722, new), plus the DST
  repoint-versus-supersede property appended to the existing `crates/dst/tests/custodian.rs` (#722).
- **Verification posture:** declared, because the default does not hold. This is a close / no-fix
  disposition: there is no production change, so no flippable red→green exists and every gate
  element lands N/A without running (`gates.py:155-165`). Verification is the **human's
  confirmation at sign-off** against the mapping below — the same standing as any close bundle.
  Nothing is deferred-but-unbuilt here: the work itself is not deferred, it is **reassigned** to two
  bundles that are filed, briefed, reviewed and schedulable today.
- **Citations expected:** none of the code kind — nothing is built. The claims in this brief are
  cited to `split-proposal.md` (this bundle), `iteration-v1/SUMMARY.md` §9,
  `iteration-v1/size-signal.json`, the two child briefs, and the tracker.
- **Prior-art check (triage cycles):** by affected file path — **none**, this bundle touches no
  file. By disposition: three sibling split parents in this instance. **#681** is the completed
  precedent — split after 7 attempts, signed off `merged-wider` on 2026-08-08, tracker issue
  CLOSED. **#682** (this issue's own parent) and **#692** are split and still in flight. #681 is
  the model for what sign-off records here.
- **Disposition hint:** likely-close
  — the `close-disposition` marker already on disk reads `split` and outranks this hint outright
  (`driver.py:210-217`), so the hint is a record, not the control input. `split` is deliberately
  not in `[driver].close_dispositions` (verified: `close_class('split')` returns `''`, while
  `close_class('likely-close')` returns `likely-close`), which is why the configured token is
  written here and the marker carries the real disposition.

## Why this was split

The attempt archived in `iteration-v1/` built exactly what #711 asked for — one primitive and its
two callers — and landed **7 files / 1595 added lines / 124 KB** against a hard budget of ≤6 files /
≤450 semantic lines, over the driver's 100 KB backstop. Sign-off returned it `iterate-plan` rather
than `iterate-do` because **two builder attempts had already converged on the same oversized
shape**.

The 7th file was not drift. Each caller's conversion invalidates that caller's *own* refusal test —
`segmented_map_reconstruction.rs:484` and `segmented_map_rebalance.rs:328` both assert the #697 /
#696 placeholder this work removes — so landing both callers in one bundle **forces** both edits,
where the brief allowed one. Splitting by caller dissolves that by construction: one forced edit
per child.

Sign-off suggested a different seam (primitive apart from callers). `split-proposal.md` rejects it
with the gate's own behaviour: a primitive-only child can earn no evidence, because `run-verify.sh`
reverts production on the RED leg (so a test naming `repoint_chunk` fails to compile →
`UNVERIFIABLE`, exit 77) and an in-crate `#[cfg(test)]` test adds no `*/tests/*.rs` (so the gate
takes the green-only branch and proves no red). Splitting along the **caller** axis, carrying the
primitive with its first consumer, was confirmed by dry-running the gate's classifier on synthetic
patches of each child's file set.

## What each child carries — #711's scope, fully mapped

| #711 scope element | Carried by | Note |
|---|---|---|
| `core/src/metadata.rs` — the `repoint_chunk` primitive (flat + segment arms, exact-bytes pinning, conflict writes nothing, routed through #710's ceiling helpers) | **#721** | authored with its first consumer |
| `custodian/src/reconstruction.rs` — the repair caller stops refusing | **#721** | forces the `segmented_map_reconstruction.rs` refusal-test edit |
| `custodian/src/rebalance.rs` — the evacuation caller stops refusing | **#722** | forces the `segmented_map_rebalance.rs` refusal-test edit |
| New regression test for the repair path | **#721** | `custodian/tests/segmented_map_repoint.rs` (new) |
| New regression test for the evacuation path | **#722** | `custodian/tests/segmented_map_evacuate.rs` (new) |
| DST repoint-versus-supersede property | **#722** | appended to the **existing** `dst/tests/custodian.rs` |
| Duplicate `ChunkId` gets one plan, not independent ones | **neither — WITHDRAWN at Plan** | see below |

**The one element not carried forward**, stated plainly so sign-off decides it rather than
discovers it: #711's constraint that two committed references to one `ChunkId` must get one plan
was **withdrawn** at the children's Plan, after three independent reviews. The recorded reasoning
(`results/issue_722/brief.md`): it is not a safety failure — GC never reclaims a fragment a
committed map names (`gc.rs:143-146`, `:185-193`), so a duplicate orphan mark on a still-referenced
position is inert; the obvious fix is backwards — reconstruction's first-committed-reference-wins
dedup applied here would leave the second object naming the drained server; and it is an
independent change, since the same per-object planning already applies to flat maps. **No tracker
item exists for it.** If the duplicate *work* (not risk) is worth removing later, #722's brief says
it belongs in its own item against `rebalance.rs`, flat and segmented alike.

Two related carve-outs were filed rather than dropped: the CAS-loser stranding leak is
**getwyrd/wyrd#723** (*"reconstruction/rebalance strand an unreclaimable fragment when the placement
CAS loses"*, OPEN), and proposal 0016's committer, destination pre-mark, drain fence, rollback and
resume remain **#653** (*"core: staged segment publication committer (635.6)"*, OPEN).

## What sign-off decides

1. **Confirm the split** (or override it with iterate-to-Do, which archives the close marker and
   re-enables the full Do+Check band on a slice two attempts already failed to fit).
2. **Confirm the withdrawn dedup element** above, or ask for a tracker item.
3. **Choose the tracker action for #711 itself** — the driver does none of it (publish exits with
   *"close the tracker item by hand"*). Two defensible options: **close it**, as #681 was closed at
   its split sign-off, with a comment pointing at the sub-issues; or **keep it open as an umbrella**
   until #721 and #722 merge. The GitHub sub-issue links preserve lineage either way. **This is the
   human's call and is deliberately left open here** — no default is assumed.

## Harness note — the parent's breadcrumb is fragile

Filed upstream as **eduralph/pdca-harness#481** (bug, Milestone 0.60.0): a split parent whose brief
was archived by an earlier `iterate-plan` reads `UNPLANNED` despite its close marker, so it
re-enters the interactive Plan leaf on every run and never freezes. That is why this brief exists at
all. A consequence worth knowing when reading this bundle: once the driver advances it,
`_do_close` **overwrites `build-notes.md`** (`driver.py:232`) with generic close text, destroying
the `The work lives in the child bundles: issue_721, issue_722` breadcrumb `split --accept` wrote.
The child pointers are recorded **in this brief** for that reason. `results/issue_654` is stuck in
the same state and needs the same treatment.

## STOP discipline

Nothing to build, nothing to publish. Draft only until Check sign-off; the tracker action for #711
is the human's.
