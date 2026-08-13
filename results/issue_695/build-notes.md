# build-notes — issue 695, iteration 9

*Withheld from the reviewer; written for the human at sign-off.*

## What this iteration actually changes, and why it is almost all not-code

The iteration-8 carry-forward is unusual: it is a **human override that forbids a code fix**.

> "Human overrides the size-backstop's iterate-plan recommendation … The single live T4 blocker
> (review-batch.md, `backfill.rs:268` — CAS conflict / stale-or-zero gauge / `Satisfied`) is the
> round-6 blocker (2) resurfacing at a new line: **do NOT fix it in code — RECORD-REJECT it in
> `review-rejected.md`.**"

So the deliverable for this round is *not* a different containment design. Round 8's patch had
`C4-ci` pass, `C4-verify` PASS (5 tests red on the base), `C5` 0 survivors, `T4-contribution`
pass — one gate red, and the human's ruling was that the finding behind it must be recorded, not
coded around. Re-deriving the production change from scratch would have thrown away four gates'
worth of evidence to arrive at the same place; the honest delta is therefore small and I am
stating its size plainly rather than dressing it up:

| Artifact | Delta vs. iteration 8 | Why |
|---|---|---|
| `crates/custodian/src/backfill.rs` | **+5 comment lines** (`:270-275`) | an in-code `// deferred: #699` marker at the CAS-conflict arm, which `AGENTS.md:200-203` ("Deferrals are settled") makes a *protocol* answer to the recurring finding — the mechanism that stops round 10 being round 9 |
| `crates/custodian/tests/segmented_map_backfill.rs` | **byte-identical** | it already binds all five legs; it is 237/240 semantic and 394/400 raw, so it has no room and needs none |
| `review-rejected.md` | rewritten | the recorded decisions were keyed to **iteration-7 line numbers** and had stopped binding to anything; refreshed to the patched file's real lines, plus the round-8 lost-CAS class the human directed be recorded |

**I did not "fix" the blocker in code, deliberately.** Both available code fixes break a pinned
requirement, and I can show the exact line that goes red for each:

* *Mark the pass incomplete on a conflict* (`incomplete += 1` in the `CommitOutcome::Conflict`
  arm, `backfill.rs:276`) — **1 added line**, and it turns `crates/custodian/tests/backfill.rs`'s
  `a_racing_writer_wins_the_cas_and_backfill_retries_on_a_later_pass` (`:279`, the leg the brief
  cites as `:278-325`) red: that test pins `Reconciled::Satisfied` after a racing writer wins the
  version-conditional commit. The brief forbids exactly this
  generalisation ("'Declined work ⇒ `Blocked`' must NOT be generalised to 'any unfilled record ⇒
  `Blocked`'"), and the brief's §Scope out-of-scope list says the existing suites stay green
  **unmodified**.
* *Re-read the winner to settle its placements* — a `ctx.meta.get(&inode_key).await?` +
  `metadata::decode` + `resolve_chunk_map` in the conflict arm, ~8 lines, and it adds a **second
  read of a record this pass already read**. Leg 4 of the Success criterion asserts
  `reads(&meta, b"inode:") == 1` and `reads(&meta, b"seg:") <= S`; the general form of the
  re-read is a comparison of two generations, which §Scope carves out to **#699** by name ("A
  generation-restart comparison … **DO NOT BUILD**. Tracked as #699").

The finding's premise is also partly wrong on the code as written, which is worth recording
because it will recur: the gauge after a lost CAS is never "zero". `remaining += to_fill.len()`
runs at `backfill.rs:202`, **before** the commit, and the only subtraction is inside the
`CommitOutcome::Committed` arm (`:259`). A lost CAS therefore leaves the placement on the gauge
by construction — which is precisely answer rule 2's requirement ("including one whose CAS was
lost").

## What the production change is (unchanged in substance from round 8)

Two inline `record.chunk_map.as_flat().ok_or(ChunkMapError::SegmentedMapUnsupported{..})?` reads
on `origin/main` — `backfill.rs:95-101` in `reconcile` and `:178-183` in `emit_remaining` — each
ended the pass for the **whole store** on the first segmented object. Both are gone:

* `backfill.rs:156-177` — read every committed object through `metadata::resolve_chunk_map`, the
  same shared resolver GC and restore already use. The three arms and the `ChunkMapError`
  downcast rule are copied from the two merged peers, `gc.rs:402-416` and `restore.rs:644-657`,
  including `Ok(None) => continue` (`gc.rs:404`, `restore.rs:646`) and `Err(err) => return
  Err(err)` for a fault that is **not** a chunk-map anomaly.
* `backfill.rs:135-142` — a record whose own bytes will not `decode` is contained rather than
  `?`-propagated, mirroring `gc.rs:378-384` / `restore.rs:631-637`, and named *before* any store
  read that follows it (`gc.rs:155-166`'s placement, for `gc.rs:159-160`'s reason).
* `backfill.rs:219-223` — a fill this pass may not perform (a segmented scanned generation) is
  **declined**: nothing written at all, so the root and its `seg:` records stay byte-identical,
  and the segmented write path stays #682's.
* `backfill.rs:202` / `:259` / `:280` / `:315-320` — the drain gauge is counted in the **same**
  walk that fills (the base's second `scan(b"inode:")` at `:173` is deleted), published beside a
  new `gauge.backfill_placement_incomplete` on the same event, each as its own `gauge.`-prefixed
  instrument.
* `backfill.rs:282-294` — the refusal to certify over an incomplete reading, reusing
  `gc.rs:234-246`'s shape and `Reconciled::Blocked` (`reconciliation.rs:44`) rather than
  inventing an outcome.
* `backfill.rs:334-343` / `:360-369` — the two emitters. `action = "unresolvable-chunk-map"` is
  the **same string** `gc.rs:563-573` and `restore.rs:825-835` already publish, so one grep finds
  all three; `action = "declined-segmented"` is its own action so a reader tells "I could not
  read this" from "I read it fine and may not write it". Naming goes through
  `gc::object_name` (`gc.rs:470-480`), whose escaping is injective.

**Frozen at the base, verified byte-for-byte against `git show
origin/main:crates/custodian/src/backfill.rs`:** `parse_inode_key` (now `:70-76`, base `:64-70`),
its skip (`:146-148` / base `:84-86`), the CAS key + `metadata::encode(&record)` precondition
(`:249-252` / base `:142-145`), and the `inode_id` audit fields (`:379`, `:407` / base `:195`,
`:223`). That is the #698 carve-out; it is what produced the sole blocking finding in rounds 3
and 5, and touching it is what the brief forbids.

## Budget

| Limit | Actual |
|---|---|
| exactly 2 files | 2 (`git status`: `M src/backfill.rs`, `A tests/segmented_map_backfill.rs`) |
| `src/backfill.rs` ≤ 95 added semantic lines | **61** |
| test ≤ 240 semantic / 400 raw | **237 / 394** |
| no third file, no `crates/dst/` hunk, no `Cargo.toml` | none |

## Refuting my own test (forced, recorded)

**(a) Genuine red?** **Yes.** `engine/scripts/run-verify.sh` reverted `crates/custodian/src/backfill.rs`
to `origin/main` while keeping the test file, and all **5** legs failed *behaviourally* — not on a
compile error, which is the failure mode that would have degraded this to UNVERIFIABLE:

```
a_healthy_segmented_object_…      left: []  right: [0, 1, 2]        (the flat record was never filled)
a_fill_this_pass_may_not_perform… Err(SegmentedMapUnsupported { operation: "backfill::reconcile" })
an_unreadable_committed_object_…  contained: SegmentedMapUnsupported { … }
one_reading_of_the_namespace_…    one namespace reading  left: 2  right: 1
a_fault_that_is_not_one_objects…  wrong error: …met a segmented chunk map, which this build cannot yet resolve
run-verify.sh: PASS — red without the fix, green with it (5 test(s) ran red).
```

That the red is an *assertion* red is load-bearing here: the brief forbids the discriminator
naming any symbol the patch introduces, because the red leg compiles the test against the
reverted base. It names only `wyrd_custodian::backfill::{reconcile, BackfillContext}`,
`wyrd_custodian::Reconciled` (`Blocked` exists at `reconciliation.rs:44` on the base) and
`wyrd_core::metadata::*`; the whole added audit/metric vocabulary is asserted as the **strings**
the durability seam publishes (`DECLINED`, `UNREADABLE`, `DECLINES`, `UNREADS`, `REMAINING`,
`INCOMPLETE` at `segmented_map_backfill.rs:44-49`).

**(b) Production path?** **Yes.** Every leg calls the real public entry
`wyrd_custodian::backfill::reconcile(&BackfillContext { meta })` — the same function the patch
edits — over doubles that implement the **production** `wyrd_traits::MetadataStore` trait. No
copy, no mock of the pass, no re-implementation. The fixture builds its records through the real
validating constructors (`SegmentGroup::new`, `SegmentRecord::new`, `SegmentedMap::new`,
`metadata::seg_key`, `metadata::encode`), and the resolver it exercises is the production
`metadata::resolve_chunk_map`, called by the fixture itself to prove the damage is real.

**(c) Fixture includes the fault?** **Yes**, and it is a *property* of the fixture, not luck.
The store is `BTreeMap`-backed (`segmented_map_backfill.rs:51-58`, the shape
`segmented_map_consumers.rs:77-116` uses for the same reason), and the damaged inodes are `1`
and `2` against the healthy `9` — so the walk meets the fault **first**, and "the healthy record
was still filled" cannot pass on an implementation that abandons the store at its first blocker.
The fixture refuses to be curated: `seed`'s `unreadable` branch (`:189-198`) decodes the root
(asserting the *root's own bytes* are fine) and then asserts `resolve_chunk_map` genuinely
errors (`:197`), so a seeding slip cannot silently turn the resolver-refusal leg into a second
undecodable-record leg; and `Shape::Undecodable` (`:153-157`) asserts its bytes really do not
decode (`:156`). Leg 3 additionally re-runs **each damaged class alone** beside the healthy
record (`:320-329`) — that sub-loop is what killed the two mutants that survived round 6, where
changing either `incomplete += 1` to `*=` was invisible because the combined store still had the
other blocker.

## Gates re-run on this exact artifact

| Gate | Result |
|---|---|
| `engine/scripts/run-verify.sh` (C4-verify) | **PASS** — red without the fix, green with it, 5 tests ran red |
| `engine/xtask.sh ci` (C4-ci: fmt/clippy/build/test/deny/conformance) | **PASS** — "xtask ci: all checks passed", exit 0 |
| `scripts/mutants-in-diff` (C5) | **PASS** — 17 mutants: 11 caught, 6 unviable, **0 missed** |
| `cargo fmt --all -- --check` (commit-readiness) | clean (also re-run inside `xtask ci`) |

`cargo xtask ci` runs the whole workspace test suite, so it is also the evidence that
`crates/custodian/tests/backfill.rs` and `crates/custodian/tests/backfill_telemetry.rs` stay
green **unmodified** — the brief's explicit out-of-scope condition, and the tripwire for the
answer having moved further than intended. The target repo has no custom git hooks
(`/home/eddie/wyrd/wyrd/.git/hooks` contains only samples), so `cargo fmt` + `cargo clippy` via
`xtask ci` is the whole of the commit-hook surface.

## Self-review against `AGENTS.md` §*Review rubric & protocol* (the criteria the reviewers apply)

**Hard conventions.** *One clock* — the diff reads no clock. *Narrow trait seams / dependency
direction (ADR-0010, ADR-0016)* — the only added import is intra-crate (`use crate::gc::object_name`,
`backfill.rs:58`; `object_name` is `pub(crate)` at `gc.rs:470`); the pass stays over
`traits`/`core`/`tracing` exactly as its module docs claim (`:51-53`), and no `Cargo.toml` moves.
*Metadata validation boundaries (ADR-0045)* — the decode failure surfaces as an error contained
per object, never as a value (`:135-142`); contextual placement-length checking still goes through
the strict maintenance companion `checked_fragments`, untouched. *No DST-reachable shared mutable
global state (ADR-0035)* — production adds no statics; the test's `static INIT: Once`
(`segmented_map_backfill.rs:219`) is the merged precedent from
`segmented_map_consumers.rs:325-331`, and `xtask ci`'s statics gate passes. *`#![forbid(unsafe_code)]`*
— the new test file carries it (`:22`); no new crate. *Docs currency* — **verified, not assumed**:
`docs/design/architecture/06-runtime-view.md` §6.2 already states this behaviour fleet-wide
(`:31`: *"A maintenance pass resolves **every** committed object this way … a pass that only
verifies does not certify … the damaged record is attributed and the walk continues"*). I also
checked the one sentence that could have gone stale — *"a consumer that has not yet adopted it
refuses a segmented map outright"* — and it is still true: `ChunkMapError::SegmentedMapUnsupported`
is still constructed by `crates/core/src/read.rs:96` and by the two sibling loops #696/#697. No
port, API operation, RPC, CLI flag or persisted field changes. **No docs edit owed.**

**Recurring defect classes.** *Absent or unsupported entries* — this is the class the fix is
about: no silent success and no silent skip. Every unreadable object is named on the seam and
counted, every declined fill is named under a **different** action and counted, and the pass
answers `Blocked`; the assertions are positive observables (a placement actually `[0,1,2]`, a
whole-store `BTreeMap` actually byte-equal, a name actually present), never "no error was
raised". The one skip that stays silent is `Ok(None) => continue` (`:158-162`), which is
verbatim both merged peers (`gc.rs:404`, `restore.rs:646`) and pinned by the brief. *Serialization
identity* — the CAS's `require(inode_key, metadata::encode(&record))` (`:251`) is `origin/main`'s
own line, unchanged, and the decline writes nothing at all. *Transactions* — no early return
crosses a live transaction: every `continue` happens before the `WriteBatch` is built (`:250`),
and `CommitOutcome`'s two arms are the base's. *Await discipline* — the one new await
(`resolve_chunk_map`, `:156`) is bounded by the `MetadataStore` implementation, not the caller
(#508/#636), and is fail-closed in both directions; this is `gc.rs:394-401`'s stated rule, copied.
*Test fidelity* — no new concurrent or destructive path (pre-declared at Plan, recorded in
`review-rejected.md`).

**Reviewer protocol.** Two deferrals now carry in-code `// deferred: #N` markers, which
`AGENTS.md:200-203` makes settled for review: `#682` for the segmented write path (`:112-113`,
`:217-218`) and `#699` for the generation comparison (`:113-115`, `:355-358`, and **new this
round** at the CAS-conflict arm, `:271-275`).

## For the human at sign-off — two things to look at

1. **`review-rejected.md` is a decision I recorded on your instruction, not one I invented.**
   Please confirm you still hold the round-8 ruling. I refreshed the *whole* file's line numbers
   because a recorded decision binds to an exact `<file:line>` (`scripts/review-branch`,
   `load_rejected`/`is_rejected`) and every entry was keyed to iteration-7's layout — they were
   suppressing nothing. The lost-CAS class is recorded at nine locations across the commit /
   conflict / gauge / outcome block with `MATCH = conflict`, and the "overcount" phrasing at four
   with `MATCH = overcount`. The spread exists because the reviewer picks the line it cites (round
   6 said `:252`, round 8 said `:268`, on the same defect claim); the `MATCH` substring is what
   keeps a *genuinely new* BUG at those lines still blocking, which is the mechanism's own design.
   If you would rather the spread were narrower, cutting it to `:268` and `:276` is a two-line
   edit — it just risks a round 10 on a one-line drift.
2. **The T4-contribution / prior-art item from the iteration-7 carry-forward.** That round's
   objection was that `scripts/review-branch` and `scripts/pdca contribcheck` "are absent from the
   supplied target". They are — they live in **this** repo (`wyrd-pdca/scripts/`), not in
   `getwyrd/wyrd`, so a reviewer handed only the target checkout cannot reproduce those rows by
   construction. Both were re-run here and are green (`T4-contribution` passed in round 8;
   `review-branch` is the gate under discussion). Prior art by affected path, re-run on this
   worktree: `git log origin/main --oneline -- crates/custodian/src/backfill.rs` → `3e05891`
   (#648 — the segmented record shape, which *created* the two defect sites), `68403eb`,
   `8b5365b`, `fddb448` (the original identity-placement backfill); the two the brief names are
   the only ones that touched the chunk-map read, and nothing has touched the file since
   `3e05891`. No merged commit ever touched `crates/custodian/tests/segmented_map_backfill.rs` —
   it is a new file. I have no way from inside the Do beat to enumerate *closed/rejected* tracker
   work; that half stays a NEEDS-HUMAN.

## Scratch

One log under `$PDCA_SCRATCH` (`pdca-builder-695-ci.log`), removed. No `/tmp` paths, no
throwaway checkouts, no branch pushed, no PR touched.
