# build-notes — #695 backfill reads through the resolver, contained (iteration 2)

Two files, as budgeted:

| File | Change | Size |
|---|---|---|
| `crates/custodian/src/backfill.rs` | modified | **127** added semantic lines (cap 130) |
| `crates/custodian/tests/segmented_map_backfill.rs` | **new** | **459 raw / 314 semantic** (cap 520 / 320) |

Gates run locally through the project's own runners, in the cycle worktree
`$PDCA_WORKTREE = /home/eddie/wyrd/wyrd.pdca-wt-l1` (base `339da46`, clean):

- `./engine/xtask.sh ci` → **`xtask ci: all checks passed`** (fmt, clippy, build, test, deny,
  conformance) — this is also the target's own commit-hook surface, so the patch is
  commit-ready.
- `PDCA_BUNDLE=… ./engine/scripts/run-verify.sh` → **`PASS — red without the fix, green with it
  (8 test(s) ran red)`**; the red leg reported `test result: FAILED. 1 passed; 7 failed` — the
  one pass is leg 7, the pre-declared non-red over-containment guard.
- `./scripts/mutants-in-diff` → **43 mutants tested: 30 caught, 13 unviable, 0 missed**
  (iteration 1 was 4 missed).

---

## 1. What the carry-forward asked for, and where each item landed

### C5 — "make every restarted resolution non-certifying"

The finding: `Ok(None)` and an empty successor worklist bypassed refusal, and reviewer cases
with already-filled or Pending successors answered `Satisfied` instead of Rule A's `Blocked`.

Iteration 1 tried to make Rule A **unreachable by construction** — it branched on the *scanned*
snapshot's shape (`record.chunk_map.is_segmented()`) and argued that a segmented scan is
declined before a restarted resolve's answer is used. That reasoning has a hole the reviewer
found: the decline sits *after* `to_fill.is_empty() → continue`, so a segmented scan whose
**restarted** resolve answers a live generation with nothing owed fell straight through to
`Satisfied`, and `Ok(None)` did too — in both cases the pass certified a store it had not read.

This iteration makes Rule A an **explicit test on every resolve, before any classification**:

```rust
// crates/custodian/src/backfill.rs:132-159
let resolved = match metadata::resolve_chunk_map(ctx.meta, &key, &record).await {
    Ok(Some(resolved)) => resolved,
    Ok(None) => { refused.changed_under_scan(&key, RETIRED_UNDER_SCAN); continue; }   // :138
    Err(err) => …                                                                      // :141-150
};
if *resolved.record != record {                                                        // :157
    refused.changed_under_scan(&key, RESTARTED_UNDER_SCAN);
    continue;
}
```

`changed_under_scan` counts into `refuses_to_certify()` (`:323`), so **every** restarted
resolution — fillable successor, already-filled successor, or no live committed generation at
all — is non-certifying, writes nothing, and is named. It is decided before `to_fill` exists, so
there is no worklist shape that can bypass it.

**How the two generations are told apart, and why this way.** The brief left the mechanism to Do
(`resolve_chunk_map` hands back `ResolvedChunkMap.record`, `crates/core/src/metadata.rs:2256-2272`).
Three candidates:

| Mechanism | Exact? | Cost | Verdict |
|---|---|---|---|
| `matches!(resolved.record, Cow::Owned(_))` | yes *today* | O(1) | rejected — it reads a representation detail (`resolve_chunk_map`'s Answer arm happens to borrow, `resolve_current_chunk_map` must own for `'static`), not a stated contract. A future resolver that owns in both arms would silently disable Rule A with no test able to see it. |
| `resolved.record.version != record.version` | no | O(1) | rejected — a delete-and-recreate can land the successor on the same version number, and that case then classifies the *live* generation's chunks against the *scanned* row. |
| `*resolved.record != record` (chosen, `:157`) | yes | one structural compare per committed record, bounded by the object | chosen — it asks the question the rule actually asks ("did the resolver answer from the generation I scanned?") in terms of values, so it stays correct under any resolver refactor. |

The chosen compare costs a full `InodeRecord` equality even in the ordinary (borrowed) case, on
the same order as the classification loop that immediately follows — inside the brief's
"work proportional to one object at a time" constraint. `PartialEq` is available because
`ResolvedChunkMap` derives it (`metadata.rs:2265`).

### T5 / C5 mutants — conflict telemetry and combined refusal accounting

The four survivors were all in the lost-CAS accounting (`-=` at `:209`, the `+` at `:219`, `+=`
in `Refusals::superseded` at `:271` of the v1 patch). They survived because nothing asserted the
published numbers. Two changes:

1. **The defect they were hiding is gone.** The conflict branch no longer subtracts the record's
   empty placements from the population (`:239` — `CommitOutcome::Conflict => refused.superseded(&key)`,
   with no `remaining -=`), and `superseded` is now inside the caveat gauge
   (`Refusals::incomplete`, `:317`). So a lost-CAS pass publishes `remaining ≥ 1, incomplete ≥ 1`
   where v1 published `0, 0` — the exact false drain-to-zero the batch review flagged at
   `:202`/`:210`/`:219`. This matters beyond telemetry: `remaining` reading zero is ADR-0040
   decision 6's first precondition for **removing the identity-placement fallback** (#363), and
   removing it over records that still need it is a data-losing move (C-1).
2. **Leg 8 asserts the published pair**, and every other leg that can move it asserts it too
   (leg 1 `("0","0")`, leg 2 `("1","1")`, leg 5 `.0 == "0"`). 0 missed mutants now.

### T2 — "one parameterised seeding helper"

v1 shipped `seed_flat` + `seed_seg`. There is now exactly one planting helper,
`seed(store, key, chunks, seg)` (`tests/segmented_map_backfill.rs:163`), where `seg` is
`None` for a flat object and `Some((nonce tag, epoch, segments written))` for a segmented one.
The pure record constructor `root(chunks, version, seg)` (`:143`) sits beside it because two
legs need a generation that is **built but never planted** (the stale scan snapshot); every
generation in the file — planted, live or stale — goes through that one struct literal.

### T3 — the false `0/0` drain signal

Same fix as T5 item 1. The runtime claim v1 made ("a lost CAS's empties leave the population,
the live generation is the next pass's to count") is now rejected in the code comment at
`:231-238`: the pass cannot know whether the winner filled them, and a **floor** that subtracts
an unsettled record can read zero over work that is still owed. Over-counting a placement the
winner already filled is a bounded, self-correcting error; under-counting is the one that
licenses fallback removal.

---

## 2. The one thing I deliberately did NOT change: a lost CAS still lets the pass certify

`crates/custodian/tests/backfill.rs:279-295` (which the brief forbids editing: *"a need to edit
it signals an answer changed further than intended"*) pins that a pass whose only candidate lost
its CAS race answers `Reconciled::Satisfied`. So `refuses_to_certify()` (`:323`) sums
`unaccounted + declined + changed_under_scan` and deliberately leaves `superseded` out.

The line is not arbitrary, and it is stated in the code (`Refusals`' doc, `:262-269`):
**certification is about what the pass READ; the caveat gauge is about what it can STAND
BEHIND.** A lost CAS was read, classified and attempted — only the write lost, and racing the
second fence is ordinary. An unreadable object, a declined fill, or a generation that moved
under the scan means the pass has no reading of that object at all. That distinction is what
`Reconciled::Blocked` was introduced for (`reconciliation.rs:25-44`: "the loop ran over
everything it could read and refuses to certify the rest").

If the human disagrees, the change is one term in `refuses_to_certify()` — but it would require
editing `tests/backfill.rs`, which the brief rules out, so it is a **#682-or-later** decision,
not this slice's.

## 3. Other choices worth the reviewer's time

- **Rule C's CAS requires the row's own stored bytes** (`:222`, `.require(key.clone(),
  value.clone())`) rather than `encode(decode(bytes))`. `08-crosscutting-concepts.md:85` states
  the CAS shape as `require(key, encode(prior))` and notes the whole byte-identity requirement
  that hangs off it; requiring the *scanned bytes* is strictly stronger — it cannot diverge from
  the stored spelling even if a future build's encoder does, which is the failure mode that
  paragraph is worried about. It also removes the re-derived `metadata::inode_key(id)` that made
  `inode:007` read one record and CAS another.
- **A committed record under a key the `inode:` grammar refuses is contained, not skipped**
  (`:129`). The base `continue`d silently. Named + counted + non-certifying is the rubric's
  *absent-or-unsupported* posture and gc's (`gc.rs` keys `unresolvable` by raw bytes and never
  parses).
- **`changed_under_scan` gets its own `info!` counter, not the `warn!` unaccounted one**
  (`:398`). A racing overwrite or delete is ordinary; folding it into the "go and repair this
  record" signal would train the operator to ignore it. Rule E's load-bearing name still goes out
  for genuinely damaged records, at the point of the read, before the fills that follow.
- **No docs edit.** `08-crosscutting-concepts.md:85` already states the fleet-wide rule this
  change makes backfill obey ("Every existing consumer of a chunk map … treat a shape they cannot
  resolve as a typed error **for that object** rather than as an empty chunk list"); the change
  removes a violation of a documented invariant rather than altering one. No port, API operation,
  RPC, CLI flag or persisted field moves. Metric names are not enumerated in any doc (#650/#651
  added `gc_unresolvable_records` / `restore_unresolvable_records` the same way).
- **`#[rustfmt::skip]` on the test's trait impl and two helpers** is there to hold the brief's
  520-raw/320-semantic budget; it mirrors iteration 1 (which passed C4-ci) and `cargo fmt --check`
  is clean.

## 4. Rejected alternatives, with their cost

- **Keep the "unreachable by construction" framing and just add the two missing guards**
  (`Ok(None)` and the empty worklist). Rejected on principle rather than cost: the brief's
  Rule A says *"if the resolve did not answer from the scanned generation … contain it"*, and
  patching two symptoms leaves the third (a same-version successor) live. §1.2/§2: smallest
  change that restores the invariant, not smallest diff. The explicit test is **+3 semantic
  lines** over the two guards it replaces (`:157-160` vs a guard at each of the two arms).
- **Contain a lost CAS as `Blocked` too** (one term in `refuses_to_certify`, a **1-line** change).
  Rejected because it turns `crates/custodian/tests/backfill.rs:291-295` red, which the brief
  forbids editing — see §2.
- **A second scan for the gauge** (the base's shape, `emit_remaining(meta)`). Rejected by leg 6:
  over a store of S segmented objects it doubles every `seg:` range read for a number already in
  hand.
- **An 8th leg instead of extending leg 4's** — leg 8 is a new test, over the brief's seven. The
  brief's seven are the binding minimum; the carry-forward explicitly requires assertions for
  conflict telemetry, which none of the seven can carry without becoming two legs in one. Leg 4
  did grow, to a 3-case loop, because the C5 finding names three successor shapes.

## 5. Forced refutation of my own test

**(a) Genuine red?** Yes, measured, not asserted. `run-verify.sh` reverts
`crates/custodian/src/backfill.rs` to the base and keeps the added test:
`test result: FAILED. 1 passed; 7 failed`. Sample base failures — leg 1/2/3/4/6 die on
`SegmentedMapUnsupported { operation: "backfill::reconcile" }` (the defect); leg 5 dies with the
base's own audit line `"action":"conflict","inode":7` and `"gauge.backfill_placement_remaining":1`,
i.e. the Rule C defect caught in the act (it CASed `inode:7` while reading `inode:007`, left the
fill undone and still answered `Satisfied`); leg 8 dies on the missing caveat gauge. Leg 7 passes
on the base **by declaration** — it is the over-containment guard, and the brief pre-declares it
non-red. Overall verdict line: `run-verify.sh: PASS — red without the fix, green with it`.

**(b) Production path?** Yes. Every leg calls `wyrd_custodian::backfill::reconcile` — the
production entry, the same one `crates/custodian/tests/backfill.rs` and
`backfill_telemetry.rs` drive — over the real `MetadataStore` trait seam, and the containment it
exercises runs through the real `wyrd_core::metadata::resolve_chunk_map`. Nothing in the test
re-implements or mocks the pass; the only doubles are the *store* (an in-memory `MetadataStore`,
the seam production is defined over) and the tracing subscriber. The assertions read the store's
own bytes (`raw`, `:182`) and the durability seam's real JSON output.

**(c) Fixture includes the fault?** Yes, and each fault is asserted to be real before the pass
runs:
- leg 3 asserts `resolve_chunk_map` genuinely errors on the seeded root (`:290-295`) — the
  damaged objects are seeded FIRST in key order over a `BTreeMap`-backed store, so "the walk
  continued past them" is a fixture property, and the healthy record sits in the **same** store,
  not a curated second one;
- leg 4 asserts the resolve genuinely **restarts** — `!matches!(&restarted, Some(answer) if
  *answer.record == stale)` (`:346`) — for each of the three successor shapes;
- leg 8's conflict is a real `CommitOutcome::Conflict` from the double's precondition check
  against bytes the pass never saw, not an injected return value;
- leg 7's store fault is a real `Err` out of `get`, met inside the resolver.

## 6. Bundle extras

- `review-rejected.md` — the standing Tier-0 DST TEST-GAP, settled at Plan (`brief.md`
  § Verification posture), re-pinned at the two lines of THIS patch where it can re-land. The
  three lost-CAS BUG findings are **fixed**, not rejected, and should leave the next run.
- `segmented_map_backfill.rs` at the bundle root is a byte-identical copy of the shipped test.

No NEEDS-HUMAN external dependency: everything ran on the base Rust toolchain plus the five
registered `[[doctor.checks]]` tools the brief listed. No Docker, no live backend, no new
dependency, no `Cargo.toml` change.
