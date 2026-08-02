# Build notes — issue 635 / segmented-chunk-map (iteration 11)

Withheld from the reviewer; written for the human at sign-off.

## What this iteration is

Iteration 10 was rejected at sign-off on **two verified implementation bugs** found by the
adversary review, plus a note that the T4 batch review and the Check reviewer leaf had both
failed on transient quota with no usable output ("do not treat this round's silence as a clean
bill"). Nothing about the design, the encoding, the resolver, the committer or the containment
table was rejected.

So this iteration starts from iteration 10's patch — applied to a clean `origin/main` @
`9120f7a` worktree (`$PDCA_WORKTREE`, verified `git apply --check` clean) — and changes
**exactly the two bugs**, their tests, and the two doc sentences the second bug touches —
then, after running the T4 reviewer myself before handing off, **two more** (§"Bug 3", §"Bug 4"),
and one correction to bug 2's own placement that the same reviewer caught.
Nothing else moved: the rest of the diff is rounds 1–10 work the carry-forward says not to
regress. The out-of-scope item the sign-off named (the reconstruction/rebalance containment gap
for a damaged chunk-map object, filed as an Act candidate) is **not** touched, deliberately.

**Environment check the brief demands (`Falsifiability` 2):** no `$PDCA_BASE`, no
`$PDCA_VERIFY_BASE`, no `stack-base` file in the bundle. `run-verify.sh --print-base` ⇒
`origin/main`. Build base == test base.

## Bug 1 — a publication could mint a chunk no pass in the system can drain

**The finding.** `plan_with` validated a chunk's placement with `checked_fragments()`, which
accepts an **empty** vector (the pre-M3 identity fallback). So a segmented publication could
persist a `seg:` record whose chunk carries `placement: []`. `crates/custodian/src/backfill.rs`
states that combination is structurally impossible, and raises `SegmentedPlacementUnfillable`
when it meets one — a pass-level `Err` that repeats on every future backfill run, because
nothing can ever fill that record: the `ChunkRef` lives in a `seg:` record and the pass's only
move is an inode CAS.

**The fix — at the cause, not at the symptom.** The write boundary refuses it:
`crates/core/src/metadata.rs:3348-3365` (`plan_with`), new typed variant
`ChunkMapError::WritePlacementEmpty` (`:777-796`, Display at `:1010-1013`). It sits with the other
deterministic, zero-I/O refusals, so a refused publication is decided **before** the first
`seg:` record is durable (the leg B(iv) ordering rule).

That makes the downstream premise **true by construction**, which is what the carry-forward
asked for ("make the 'structurally impossible' premise true by construction"). The two doc
comments in `backfill.rs` that asserted the premise as a claim about a future producer now cite
the two mechanical reasons instead (`crates/custodian/src/backfill.rs:145-151`, `:317-323`): the
publication refuses an empty placement, and a segment repoint's replacement vector must be
exactly `fragment_count()` long (`crates/core/src/metadata.rs:1499-1513`). Those are the only
two writers of a `ChunkRef` into a `seg:` record in the workspace.

**What I did *not* do, and why.** The carry-forward offered a second option — "or make the
backfill skip non-fatal for that pass". Rejected, on two grounds:

* It treats the symptom while leaving the store able to *acquire* the record. The premise
  ("structurally impossible") would still be false, and the next reviewer to notice would be
  right.
* It contradicts the rubric's *Absent or unsupported entries* rule (`AGENTS.md:175-177`) and the
  code's own stated reasoning: a segmented record with an empty placement is a population **no
  pass drains**, so downgrading it to telemetry lets a `Satisfied` pass certify "nothing left to
  do" over it. That is the count-based-reassurance shape the rule forbids.

With the producer closed, the remaining route to that state is corruption — where halting the
pass *is* the safe answer, and where the pass still drains every other object first (the error
is raised after the loop and after the gauge, `backfill.rs:220-227`). Cost of the rejected
option, concretely: it is a 3-line change (drop the `unfillable` push and the trailing `if
let … return Err`), and it would delete the `SegmentedPlacementUnfillable` type plus its two
tests — cheaper in lines and strictly worse in safety, which is why the line count is not the
axis.

**Not fixed at decode, deliberately.** `SegmentRecord`'s decode still *accepts* an empty
placement. That is the repo's stated boundary rule — structural invariants at decode,
**contextual** checks (placement length is the named example) liberal on read and strict in
maintenance/write paths (ADR-0045, `AGENTS.md:146-149`). Rejecting it at decode would also make
leg A's own corruption fixture unreadable and would turn a pre-M3 flat record class into a
decode error.

## Bug 2 — the resolve budget was the record's own claim

**The finding.** `read_group_range` pages the group's `seg:` range at `accounted + 1` rows,
where `accounted` is the root's declared `segment_count`. Nothing bounded that number: a root
declaring 2 000 segments encodes to ~94 KB — inside `MAX_VALUE_BYTES` (100 000), so every
backend stores it without a word — while the `seg:` records it authorises are up to 100 KB
**each**: ~200 MB materialised per resolve, by every consumer, on every pass, for one damaged
object. A budget taken from the record being validated is not a bound.

**The fix — and I got its *placement* wrong first, so read this part.** The ceiling is enforced
where the table becomes work: `read_group_range` (`crates/core/src/metadata.rs:2567-2572`) —
the one function that turns a segment table into a read — **refuses** a budget past
`MAX_ROOT_SEGMENTS` with the typed `ChunkMapError::TooManySegments` before it asks the store for
a single row. Per object, contained by the existing `ChunkMapError` downcast, and unread.

*Refused, not clamped* (the carry-forward's literal suggestion): a clamp would silently re-file
a row the table *does* name as one it does not (`GroupRange::Beyond`), so a budget disagreement
would be reported as a phantom corruption — worse evidence than the real fault, for one line
less.

*And not at decode*, which is where I first put it (`SegmentedMap::new`). My own T4 pre-check
round caught that, and it was right: `MAX_ROOT_SEGMENTS` is a derived **capacity** constant, and
the repo's boundary rule puts capacity checks on the write/maintenance paths and keeps decode
liberal (ADR-0045; `AGENTS.md:146-149` — structural invariants at decode, *contextual* checks
liberal on read). The constant's own doc had already recorded that decision, and my first patch
contradicted it in the same file. Concretely: at decode, lowering the constant would make
already-published objects **undecodable**, which is the on-disk rule 08-crosscutting-concepts
states ("old-format data is read, never rejected"). At the reader, lowering it makes them
unresolvable while the limit stands — refused, diagnosable, and still readable as records. The
test asserts both arms.

`MAX_ROOT_SEGMENTS` is the same number `plan_with` refuses to *plan* past, so the two ends of
the ceiling — writing a table and reading one — now agree on one constant, and the constant's
doc (`:284-292`) says which two places enforce it and why neither is `decode`.

**Blast radius checked.** The containment table requires `high_water_marks` to be **total**. It
is unaffected either way — it never resolves a root, and reads a segmented object's chunk ids
from the `seg:` records themselves — and the test asserts that leg directly, in the same store
that holds the over-ceiling root.

## Bug 3 — found by running the T4 gate myself, before handing off

The sign-off said the T4 review produced no usable result last round and told me not to read
that silence as a clean bill. So I ran `scripts/review-branch --bundle` against this patch
(with `--out` to a scratch path, so the bundle's `review-batch.md` stays the driver's) before
declaring done. It returned **three** findings, which are **two** claims: two passes
independently reported the same fence bug, one pass reported the startup-scan cost.

**The fence bug, fixed.** `check_fence_transitioned` accepted a **delete** of the fence record
as a valid transition in *either* phase. At the flip that is right — it is the strongest
transition there is, and nothing later is fenced on the record. In phase 1 it is
self-destruction with durable consequences, and I confirmed it on the pre-fix tree with a
throwaway probe rather than by reasoning:

```
PROBE outcome=Ok(Conflict) durable_seg_rows=2 fence=None
```

Two `seg:` records durable, the fence record deleted, and the caller handed a *retryable-
looking* `Conflict` for a state no retry can ever leave — the remaining batches and the flip
are all fenced on the record that is now gone, and so is the rollback that would have reclaimed
those two rows. Fixed at `crates/core/src/metadata.rs:4120-4126` with a distinct typed refusal
(`ChunkMapError::FenceRemovedBeforeFlip`, `:654-674`), decided with the other zero-I/O
refusals. The phase labels are now constants (`PHASE_SEGMENTS` / `PHASE_FLIP`, `:4042-4046`)
because the new rule *keys* on the phase, and a rule that turned on a string typo would be
silently permissive.

**The declined one, and why I did not just take it.** `crates/core/src/metadata.rs:4775` —
"`Gateway::recover` discards the chunk-ID floor …, so scanning every `seg:` record here adds
full-namespace startup latency". The premise is **true**, and I checked it against the base
rather than trusting either side: `crates/server/src/lib.rs:124` on `origin/main` already reads
`let (max_inode, _max_chunk) = metadata::high_water_marks(…)`. The chunk half of that call is
vestigial in production **on the base** — in-process chunk ids have been coordination-free
since ADR-0019 — so this slice inherits the wart rather than creating it. But the remedy the
finding implies (do not walk `seg:` at startup) is excluded by this bundle's own success
criterion: leg A(vii)(a) *requires* the floor to be ≥ every chunk id in any `seg:` record, and
the containment table forbids under-approximating it — and a segmented object's ids exist
nowhere but its `seg:` records. So it is **recorded-rejected** with that reasoning
(`review-rejected.md` § Round 11), and flagged here because the underlying question is real and
is **the human's, not mine**: should `Gateway::recover` compute a chunk floor it throws away at
all? That is a base-wide change to `crates/server/src/lib.rs` outside this slice's `Scope`, and
a good follow-up issue.

## Bug 4 — and the second pre-check round found the ABA half that *is* fixable

Re-running the reviewer over the fixed patch returned four more findings; the three above did
not reappear. Two are fixed, two are recorded-rejected (`review-rejected.md` § Round 11 pre-check,
second pass). The two fixes:

* **`check_fence_transitioned` is a per-batch rule, and a phase can satisfy it at every batch
  while walking the fence `A → B → A`.** The instant the fence is back at `A`, every
  precondition satisfiable at `A` is satisfiable again — so a second completer of the same
  generation overwrites this attempt's `seg:` records while it is still writing them, and the
  root it flips names a hybrid of two plans. Round 10 declined the *value-grammar* half of ABA
  (which values are terminal is #636's to know, and that decline stands); this half needs no
  grammar at all — that the fence repeated a state is visible from the batches before any of
  them commits. `check_fence_never_cycles` (`crates/core/src/metadata.rs:4169-4200`) refuses it
  with `ChunkMapError::FenceCycled`, zero I/O. I first wrote it watching only what each
  batch *pins*, and caught the hole myself on re-reading: the same `A → B → A` can be
  spelled in the **puts** (`pin A, put B` then `pin B, put A`), which a pins-only rule sees
  one batch late or not at all. It now reads both halves and collapses only *adjacent*
  repeats — one batch's put is the next batch's pin in every honest contribution, so only a
  non-adjacent repeat is a return. The test asserts the **batch index** of the refusal (1,
  the put that restores; a pins-only rule answers 2), so the weaker rule fails it.
* **The capacity-at-decode mistake in my own bug-2 fix** — see §"Bug 2". The reviewer found it
  in code I had written this round, which is the strongest argument for having run the gate
  before handing off rather than after.

**Where I stopped.** The target rubric's definition of done is "deterministic gates green plus
**one** deep, multi-pass review whose findings are each fixed or rejected with a recorded
reason — do not iterate review rounds chasing silence". I ran two pre-check rounds (the second
because the first round's fixes were substantial enough to be worth re-reviewing, and it caught
a defect in my own new code), triaged all seven findings — four fixed, three recorded-rejected —
and stopped. I did **not** run a third.

## Docs currency (a merge requirement, `AGENTS.md:154-157`)

The new invariant is part of the persisted record class's stated shape, so the two living
architecture docs the slice already edits gain it, in this PR:
`docs/design/architecture/08-crosscutting-concepts.md:83` (capacity limits are *not* decode-time)
and `06-runtime-view.md:32` (the resolve is "never a budget the root sets for itself"). Both
prose gates (`typos`, the doc renderer) are installed here and ran inside `cargo xtask ci`.

## Refuting my own tests (forced; the human reads these at sign-off)

Both new tests are **co-located `#[cfg(test)]` units in the production module**, per the brief's
leg B placement rule — a second added `tests/*.rs` file naming types this slice adds would join
C4-verify's cargo invocation on the RED leg and destroy leg A's assertion red.

**(a) Genuine red?** Yes — reverted individually and re-run, not argued.

1. *Bug 1.* Reverted the six-line refusal in `plan_with` (`crates/core/src/metadata.rs:3360`,
   nothing else) ⇒ `a_segmented_publication_refuses_a_placement_no_pass_could_ever_fill`
   (`:6377`) **FAILED**: *"a chunk with an empty placement must be refused: Committed"*. Pre-fix
   the publication *commits* the undrainable record.
2. *Bug 2.* Reverted the six-line ceiling in `read_group_range` (`:2567`) ⇒
   `a_root_table_over_the_ceiling_is_refused_before_it_is_read` (`:8485`) **FAILED**:
   `left: Some(SegmentAbsent { …, index: 4 }) / right: Some(TooManySegments { segments: 2000 })`
   — i.e. pre-fix the maintenance-plane consumer really does walk the range the root authorises
   (it got as far as the 4 seeded rows before missing one) instead of refusing the budget. (The
   earlier, wrongly-placed decode version of this fix was also probed red — `Ok(Some(2000))` from
   `decode` — before the pre-check round moved it; the current test asserts that decode
   **succeeds**, which is the corrected boundary.)
3. *Bug 3.* Reverted the six-line phase-1 fence-delete refusal (`:4120`) ⇒
   `a_deterministically_refused_publication_writes_no_segment_at_all` (`:6055`) **FAILED**: *"a
   segment batch that deletes the fence the rest of the publication needs must be refused:
   Conflict"* — and the throwaway probe quoted in §"Bug 3" shows what that `Conflict` leaves
   behind (2 durable `seg:` rows, fence deleted). The same test's new flip control passes both
   before and after, which is the point of having it: the fix must not have banned the delete
   outright.
4. *Bug 4.* Neutered the cycle refusal inside `check_fence_never_cycles` (removing the call
   instead trips `-D dead-code`, which is a build error and not a useful red) ⇒
   `a_phase_whose_fence_returns_to_a_state_it_held_is_refused` (`:6281`) **FAILED**: *"a phase
   that walks its fence back must be refused: Conflict"* — the cycling phase runs, every batch's
   precondition genuinely holds (the fixture seeds the fence at the value batch 0 pins), and the
   publication ends in a `Conflict` with segments already written.
5. *The bundle's binding test, through the project's own runner.*
   `PDCA_BUNDLE=results/issue_635 ./engine/scripts/run-verify.sh` ⇒ **PASS — red without the fix,
   green with it**. GREEN leg: **9 tests ran, 9 passed**. RED leg (production reverted, the added
   test kept): **9 tests ran, 9 failed, and the red is assertions, not a build error** — e.g.
   `maintenance_resolves_a_segmented_map_and_never_reclaims_its_fragments` at
   `crates/custodian/tests/segmented_map_consumers.rs:644` with *"reconcile_step must resolve a
   segmented chunk map, not fail on it"*, and
   `a_damaged_segmented_object_never_costs_the_store_its_other_objects` at `:1196` with *"one
   damaged object must not fail the id floor the gateway starts from"*. (Brief `Falsifiability`
   clause 3 asks for exactly this count and classification.)

**(b) Production path?** Yes, for all three.

* Bug 1's test drives the real `SegmentedPublication::publish` against a real
  `RedbMetadataStore` and then asserts over the **bytes the committer actually made durable**
  (every chunk of every `seg:` record it wrote carries an explicit placement) — not over its
  return value alone.
* Bug 2's test drives the real `metadata::decode`, the real maintenance-plane entry
  `resolve_current_chunk_map`, and the real `high_water_marks`, over a real redb store wrapped
  only by the existing `Counting` double that counts the rows the backend served.
* Bug 3's test drives the real `publish` — both phases, the real batch assembly and the real
  commit loop — and reads the resulting store state back through the same store.

No stand-in for anything under test; the only double in any of them is the caller-supplied
publication precondition, which is #636's, exactly as the brief's *Production reach* states.

**(c) Fixture includes the fault?** Yes, and this is where I was most careful, because each
fixture could have been written to prove nothing:

* Bug 2's over-ceiling root is **committed to the store and asserted to be there**, and the test
  first asserts it is **inside `MAX_VALUE_BYTES`** — otherwise the backend would have refused it
  and the test would have been measuring the value ceiling, not the table's own bound.
* Four **real** `seg:` records are seeded under that group's range, so a resolve that believed
  the table has something to walk into (and, pre-fix, walks it — see (a) 2). A fixture with an
  empty range would have passed on the buggy code by accident.
* The row counter is zeroed immediately before the consumer call, so the `0 rows` assertion is
  about that call and not about the seeding.
* Bug 1's fixture keeps the empty-placement chunk **in the middle** of the list (position 7 of
  40), so a refusal that only checked the first chunk would fail, and the negative control (the
  same list with a full placement) is published in the **same store**, so "nothing is durable"
  is measured against a store that demonstrably accepts the admissible publication.
* Bug 3's fixture makes the *deleting* contribution otherwise **valid** — it pins the fence
  correctly and would have satisfied the transition rule — so the refusal under test is the
  delete itself and not some other defect of the hook; and the flip control asserts the opposite
  arm in the same test, so a fix that simply banned fence deletes everywhere would fail.

## Gates I ran here

* `./engine/xtask.sh ci` (the project runner ⇒ `cargo xtask ci`: fmt, clippy `-D warnings`,
  build, full test suite incl. DST, cargo-deny, conformance, statics/unsafe/gitlink guards,
  `typos`, docs renderer) — **all checks passed** (`xtask ci: all checks passed`), on the final
  tree. Two earlier runs failed and were fixed before re-running: `cargo fmt --check` over my new
  test code, and one clippy lint (`manual_is_multiple_of`) in the bug-4 fixture. Both are exactly
  the class the target's own commit hooks would have rejected at publish.
* `./engine/scripts/run-verify.sh` (C4-verify) — **PASS**, base `origin/main`, exactly one
  `ADDED_TEST crates/custodian/tests/segmented_map_consumers.rs` (`--classify` confirmed), re-run
  on the final patch.
* `scripts/review-branch --bundle` (the T4 gate) run **pre-handoff**, twice, with `--out` to a
  scratch path so the bundle's `review-batch.md` stays the driver's. Round 1: three findings (two
  the same bug) ⇒ two fixed, one recorded-rejected. Round 2 over the fixed patch: the three do not
  reappear; four new ⇒ two fixed (including a defect in this round's own new code), two
  recorded-rejected. Every one is triaged in `review-rejected.md` § Round 11. Note for the human:
  the gate at Check runs a *fresh* review, and this reviewer's own docstring says to expect
  roughly one new pre-existing finding per round — a non-empty result there is not evidence that
  these were not triaged.
* Formatter / commit-hook readiness: `cargo fmt --all -- --check` clean over every touched file.

## Where the test file lives

The brief's named test file `crates/custodian/tests/segmented_map_consumers.rs` ships **inside
`patch.diff`** (as in every prior iteration of this bundle) — it is unchanged this round; this
round's tests are co-located units in `crates/core/src/metadata.rs` (`:6377` bug 1, `:8485`
bug 2, the extended `:6055` for bug 3 and `:6281` for bug 4), which is where the brief's leg B requires anything
naming this slice's new types to live.

## Still for the human (carried, not silently dropped)

Unchanged by this iteration; none is resolvable by code:

1. **T3 — landing a `Completing`-less precursor committer** before #636 supplies the real
   session fence (brief `Open questions` 4).
2. **Fitness of synthetic fixtures pre-#636** — no production path publishes a segmented map
   when this slice merges, which the brief states is correct.
3. **C5 mutants** (advisory) — not re-run here (≈17 min in the last round). The two new tests
   are written to bind their mutable surface: the ceiling is asserted at **both** sides of the
   boundary (512 admitted, 513 refused) rather than only past it, and the refusal's `position`
   and `expected` fields are asserted exactly.
4. **T4 contribution-history provisionality.**
5. **NEW, and worth a follow-up issue rather than a code change here:** `Gateway::recover`
   computes an id-allocator **chunk** floor and throws it away (`crates/server/src/lib.rs:124`,
   on the base as well as here), because in-process chunk ids have been coordination-free since
   ADR-0019. This slice adds a `seg:` walk to that computation because the brief's leg A(vii)(a)
   requires the floor to cover segment records — so the walk is *required* here while its only
   production consumer discards the result. Deciding whether recovery should compute a chunk
   floor at all is a base-wide call about ADR-0019's allocator, not about segmentation. Raised
   by the T4 reviewer this round and recorded-rejected with this reasoning
   (`review-rejected.md` § Round 11).

## Scratch

Everything transient lived under `$PDCA_SCRATCH` (`/var/tmp/pdca/pdca-builder-635-*.log`, plus
`pdca-builder-635-redprobe/` holding the two revert-probe copies of `metadata.rs`). All removed
before handoff. The only worktrees touched are `$PDCA_WORKTREE` and the `../wyrd-verify`
worktree `run-verify.sh` manages itself.
