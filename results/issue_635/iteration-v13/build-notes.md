# Build notes — issue 635 / segmented-chunk-map (iteration 13)

**Withheld from the reviewer.** For the human at sign-off.

## What this round is

Iteration 12's patch was rejected at sign-off on the **T4 batch-review gate**, not on the
design: the human re-ran `scripts/review-branch --bundle` manually and got three blocking
findings, none of them fixed or triaged. Iterations 1–11's work is unchanged; this round is
**iteration 12's `patch.diff` plus ~755 added lines in `crates/core/src/metadata.rs` and ~85 in
`crates/dst/tests/custodian.rs`** (`git apply --numstat`, this patch vs
`iteration-v12/patch.diff`: 15,979 insertions here against 15,192 there). Everything else in
the 48-file patch is byte-identical to iteration 12.

I applied `iteration-v12/patch.diff` to the worktree first (`git apply --check` ⇒ clean), then
worked the three findings. That is the only prior-cycle material I read: the brief's own
carry-forward names `iteration-v12/` as the preserved previous attempt, and re-deriving a
15,700-line patch from scratch would discard eleven rounds of settled review.

## The three findings, and what I did

### 1. `metadata.rs:3879` [BUG] — segment rows written with a blind `put`

**The hazard, restated so it is checkable.** `publish` re-derives its plan deterministically
from `(nonce, epoch, chunk list)`. A completer whose *flip* returned `CommitUnknownResult`
cannot tell whether the object is published, so it retries the whole publication. If the flip
did land, the object is **live**, and maintenance may already have repointed a fragment inside
one of its `seg:` records (`repoint_chunk`, a `require(seg == prior)` CAS that also writes the
vacated placement's `orphan:` record). The retry's `verify_durable_range` read happens *before*
that repoint in the interleaving that matters; its blind `put` then restores the placement the
repoint vacated — fragments that are already orphaned and, past the grace window, reclaimed —
and the flip that follows loses its root CAS. `publish` returns a plain `Conflict` over a
corrupted live generation.

**The fix.** The range read the publication already performs now *returns what it saw*
(`verify_durable_range` ⇒ `DurableSegments`, `crates/core/src/metadata.rs:3552`,`:4307`), and
`assemble_segment_batches` (`:4041`) gives each row the precondition its observed state calls
for:

* witnessed durable **and byte-identical to this plan's record** ⇒ `require(key, value)`, and
  **no put** (there is nothing to write; what is there is already ours);
* not witnessed ⇒ `require_absent(key)` beside the put.

So phase 1 never writes over a row it did not observe absent, and a row that moved in the
window makes the batch `Conflict` instead of clobbering it.

**Why the witness cannot move the split** — this was the design problem, and getting it wrong
would have been worse than the bug. `publish` assembles both phases *before* its range read,
because leg B(iv) requires every zero-I/O refusal to be decided with nothing durable. If the
witness changed batch boundaries, the pre-read assembly and the committed assembly would
disagree, and with them the `SegmentBatchInfo` sequence the caller's fence and cursor are
derived from — the flip would be charged (`check_fence_never_cycles`) against a trajectory the
phase never committed. So every planned row is **charged at the unwitnessed shape** — key +
value + the `require_absent` key, and `SEGMENT_OPS_PER_RECORD = 2` operations (`:4500`) — which
is the larger of the two shapes in both currencies.

**Charging it was not enough, and I nearly shipped the gap.** The contribution-reserve loop
*measures the assembled batch*, so with the witness applied inside the loop a witnessed batch
measures smaller, the `over` set empties one pass earlier, and the loop returns a **coarser**
split than the unwitnessed run — 9 batches instead of 10 on the 40-chunk fixture, measured. The
loop now assembles and measures the unwitnessed shape throughout and applies the witness once,
to the ranges it has already settled (`staged_batches_over`, `:3783`). That is what makes the
"same batches, fence for fence" claim true rather than plausible, and
`the_durable_witness_never_moves_the_split` (`:7990`) is the assertion that keeps it: same batch
count, same rows per batch in the same order, identical caller contribution, witnessed shape
charged no more than the split reserved. Pre-fix probe: pass `durable` into the loop's assembly
⇒ `the witness may not change how many batches the phase has — left: 9, right: 10`.

Two accounting consequences, both honest rather than cosmetic:

* `batch_ranges`'s third parameter was already *records per batch* while being fed an
  **operations** budget. With a precondition per row that undercount becomes reachable: the
  measured check would refuse batches the split could never make small enough (traced by hand:
  `ops_budget = 4`, contribution 2 ops, 6 records ⇒ the reserve loop converges to 2 records =
  6 ops > 4 and stops). It now divides by `SEGMENT_OPS_PER_RECORD` and the parameter is named
  `max_records`.
* `Measured.bytes - segment_bytes` becomes an underflow the moment a witnessed batch measures
  smaller than the split reserved for it — a **panic** on exactly the resumed-publication path
  that most needs to answer. Both differences are `saturating_sub` now.

### 2. `metadata.rs:4002` [BUG] — supersede accepts a segmented prior

Refused in `SegmentedPublication::root` (`:3664`) with the error the two flat committers
already raise for the same shape (`SegmentedRetirementUnsupported`, `commit_chunk_map:2356`).
The flip replaces the root, and the root is the **only** thing that names a generation's `seg:`
records — the resolver reaches a group through its root and nowhere else — so a flip over a
segmented prior makes that generation's records unreachable to every consumer while its
fragments stay on disk, referenced by nothing and reclaimed by nobody. Retirement is `0016`
decision 4 / 7(f)'s staged `retire:bytes:{generation}` obligation, which #636 owns.

It is raised from `root()` rather than from `flip_batch_after` so that the public
`root()`/`flip_batch`/`flip`/`publish` surface answers identically, and it lands with the other
zero-I/O refusals — nothing of the new generation is durable when it fires.

### 3. `metadata.rs:3965` [CONVENTION] — `flip_batch` swallowed the phase's `Err`

Now `let phase = self.segment_batches()?;` (`:4151`). The in-code rationale for swallowing was
that both writing entry points raise the phase's error first — true, and irrelevant to the
caller the finding names: a bare recovery `flip` is the one route to the publication instant
that does not assemble the phase for its own sake, and the flip's fence-cycle rule is charged
*against* that phase. Swallowed, it was charged against an empty sequence. The legitimate
recovery (`resume_from` at the end of the plan) assembles to an empty phase, which is `Ok`, so
that route is unaffected — the new test asserts both arms.

## Rejected alternatives, with their costs

* **`require_absent` on every row, unconditionally, and let a durable row `Conflict`.** Two
  lines instead of the witness machinery (~60 lines of production code across
  `verify_durable_range`, `staged_batches_over`, `assemble_segment_batches`, `DurableSegments`).
  **Rejected because it breaks same-epoch idempotent recovery**, which `0016:2352-2356` states
  normatively and which `verify_durable_range`'s own doc leans on: a completer that re-runs the
  phase without a cursor would wedge that epoch permanently and could only proceed by rolling
  back and re-minting. It also fails the DST property `staged_publication_is_atomic_at_the_flip`
  asserts in terms ("re-running the segment-write phase at the same epoch is idempotent:
  identical keys, identical bytes").
* **Witnessed row ⇒ `require(key, value)` *plus* the put** (keep the batch shape uniform).
  Costs `key + value` extra bytes per witnessed row — at the real `SEGMENT_TARGET_BYTES` that
  is ~50 KB per row, roughly halving the rows a 5 MB batch holds on the recovery path — and
  buys nothing: the put writes bytes the store already has, under a precondition that they are
  already there.
* **Only CAS the witnessed rows; leave absent rows a blind put** (no `require_absent`). Saves
  ~46 bytes and one operation per row and closes the finding as literally worded. **Rejected**
  because the absent-row put is how two attempts at the same `(nonce, epoch)` with different
  plans interleave into a **hybrid** range — half of each plan, resolving cleanly and reading
  the wrong bytes, which `verify_durable_range`'s own doc calls worse than an unresolvable map.
  `require_absent` makes it structurally impossible rather than fence-dependent.
* **Move the retirement obligation into this slice instead of refusing (finding 2).** That is
  `0016` decision 4 / 7(f) — a `retire:bytes:{generation}` record class, its byte-budgeted
  drain, and the reaper that resumes it — i.e. a second committer of comparable size to the one
  this slice ships, in a slice the brief's `Scope` explicitly excludes ("the multipart
  session/records/protocol (#636)"). Refusing is what the two flat committers already do.

## Collateral test changes (and why none of them is a fixture weakened to pass)

* `the_split_charges_the_callers_contribution_against_the_envelope` — the hand-built `exact`
  budget now counts each row's `require_absent` key. It is the same by-the-byte boundary
  assertion; the batch genuinely carries those bytes.
* `each_segment_batch_carries_the_callers_per_batch_progress` — asserted `recorded.len() ==
  batches.len()`, i.e. that the hook is called exactly once per returned batch. That was
  already only true when the split's contribution fixed point converged on the first pass; it
  now asserts on the **last** attempt's calls (the returned batches') and that there was at
  least one. See the third declined finding below — this is the same property.
* `the_flip_batch_refuses_a_caller_contribution_over_the_envelope` — `batch_budget` is one
  envelope for **both** phases, so setting it to the flip's own size makes the segment phase
  unassemblable, which finding 3's fix now reports. The boundary is asserted through
  `flip_batch_after` over a phase assembled at the real budget: same inclusive-ceiling
  assertion, isolated to the flip.
* `a_fresh_attempt_refuses_to_overwrite_another_plans_durable_records` — its fixture published
  generation 1 and then superseded it, which finding 2 now refuses. The fixture is now the
  *reachable* shape of the same race: an attempt that wrote its segments and stopped before its
  flip (crash, lost fence), so the root is still the flat prior. The assertion —
  `ResumePrefixMismatch`, nothing written, bytes unchanged — is untouched.
* `a_complete_resolve_of_a_superseded_generation_is_retired_and_restarts` — the *resolver* is
  under test, not the committer. Generation B's segments are still written by the production
  committer (`write_segments`); only its root is installed directly, because the committer now
  refuses to publish over a segmented generation. The store state asserted on is the state
  #636's retirement-carrying flip produces.
* `crates/dst/tests/custodian.rs` (`prop_staged_publication_is_atomic_at_the_flip`) — the
  ambiguous-batch leg republished over the published segmented root. It now publishes a
  **second object key** (`RootPrecondition::Fresh`), which changes nothing about the property
  (an ambiguous segment batch, the cursor that rode it, whether recovery is idempotent) and
  adds an assertion that both segmented objects coexist and each resolves to its own range.

**One DST leg added** to the same property (Tier-0, seeded, in the file the brief names): after
the ambiguous flip, the production `repoint_chunk` moves a fragment of the now-live generation
and the recovery re-runs `publish`. It must refuse (`ResumePrefixMismatch`) with the repointed
row byte-for-byte as the repoint left it. **This leg is honestly not red pre-fix** — the repoint
is durable before the recovery's range read, so the pre-existing whole-plan comparison already
catches it — and its comment says so, pointing at the unit test for the arm the read cannot see
(the repoint landing *between* the read and the batch it guards). It is a regression guard for
the pre-write refusal, not evidence for finding 1.

## The four `review-batch.md` findings — declined, with reasons in `review-rejected.md`

`metadata.rs:5283` (absent segment's ids missing from the allocator floor), `metadata.rs:1897`
(`encode_inode` does not enforce `MAX_VALUE_BYTES`), `metadata.rs:3854` (the per-batch hook is
called more than once per batch), `gc.rs:348` (`W_ref_committed` telemetry). Full reasoning is
in `review-rejected.md`'s round-12 section, in the gate's
`<file:line> | <CLASS> | <MATCH> | <reason>` format. One of them produced a **code** change
even though it is declined: the `segment_batch` hook's purity requirement is now stated on the
field (`:3460`) instead of being an unwritten assumption — the finding was right that the
contract was undocumented, and wrong that it was violated. (This round makes the hook's
multiple invocation *more* visible, not less: `publish` now assembles its phase twice by
design. That is stated on the field, and the protocol's own contribution — a function of the
cursor — is unaffected.)

Every standing decision whose anchor moved this round is re-pinned at its new line (nothing
before `metadata.rs:3395` moved).

## Forced refutation — the three questions

**(a) Genuine red?** Yes, each fix reverted individually and re-run through
`cargo test -p wyrd-core --lib`:

* finding 1 — restore the blind `put` in `assemble_segment_batches` ⇒
  `a_segment_phase_never_overwrites_a_row_a_live_repoint_moved` fails with
  `left: …"placement":[0]…` / `right: …"placement":[9]…`: the recovery restored the stale
  placement over the live repoint's. (Transcript above in this session; the diff of the revert
  was two lines.)
* findings 2 and 3 — drop the `root()` guard and restore `unwrap_or_default()` ⇒ both
  `a_publication_over_a_segmented_generation_refuses_rather_than_stranding_it` and
  `a_flip_refuses_a_publication_whose_segment_phase_is_unfenced` fail (`expect_err` on an
  `Ok(Committed)`), 101 passed / 2 failed. Restored ⇒ 103 passed.
* the split-invariance rule — assemble the loop's batches *with* the witness ⇒
  `the_durable_witness_never_moves_the_split` fails, `left: 9, right: 10` batches. Restored ⇒
  104 passed.

The bundle's binding test (leg A, `crates/custodian/tests/segmented_map_consumers.rs`) is red
on the **base** by assertion, not by build error — `./engine/scripts/run-verify.sh` on the final
patch reports `PASS — red without the fix, green with it`: GREEN leg **9 tests, 9 passed**; RED
leg **9 tests ran, 9 failed** (so the `TESTS_RAN == 0` guard is satisfied on both legs), every
failure carrying `invalid type: map, expected a sequence at line 1 column 23` — the base's
`metadata::decode` meeting a segmented value, which is the brief's predicted assertion-red.

**(b) Production path?** Yes. The new tests drive `SegmentedPublication::publish`,
`::flip`, `::root`, `::staged_batches_over` and `repoint_chunk` — the production committer and
the production repoint builder — over `RedbMetadataStore::in_memory()`, the real backend. The
only stand-in is `RacingWriter`, which *wraps* that real store: it commits a real,
production-built `repoint_chunk` batch into it and then delegates the caller's batch unchanged
(preconditions are evaluated by redb, not modelled). Nothing about the publication's behaviour
is simulated.

**(c) Fixture includes the fault?** Yes. `a_segment_phase_never_overwrites_a_row_a_live_repoint_moved`
asserts `store.injected.lock().unwrap().is_none()` at the end — i.e. the racing repoint really
was consumed and committed — and the `RacingWriter` asserts the injected batch itself commits
`Committed` before delegating. A fixture that quietly failed to inject would trip both.
`a_publication_over_a_segmented_generation_refuses_rather_than_stranding_it` asserts the prior
generation *is* segmented before it tries to publish over it.

## Gates

All four run on the **final** tree, in this order, after the last edit:

* `./engine/xtask.sh ci` (the whole Wyrd gate: fmt, clippy `-D warnings`, build, test incl.
  madsim DST, cargo-deny, conformance, prose gates) — **`xtask ci: all checks passed`, exit 0**.
  `typos` and the doc renderer are both installed on this host, so the docs-currency edit is
  really checked.
* `./engine/xtask.sh dst` — exit 0, including `staged_publication_is_atomic_at_the_flip` (which
  carries the new repoint leg) and `committed_regression_seeds_stay_green` over the seed sweep.
* `cargo test -p wyrd-core --lib` — 104 passed (100 before this round; +4 new co-located tests).
* `PDCA_BUNDLE=results/issue_635 ./engine/scripts/run-verify.sh` — **PASS**, exit 0.
* `cargo fmt --all` run over every file touched; the gate's fmt check is green, so the target's
  commit hook has nothing to reject.

## Environment check the brief demanded

`$PDCA_BASE` and `$PDCA_VERIFY_BASE` are both **unset**, and there is no `stack-base` file in
the bundle. The worktree is `origin/main` @ `9120f7a`, which carries #634 (`scan_page` is a
required trait method). Every store double this round adds implements it — `RacingWriter`
delegates to its inner `RedbMetadataStore`, which is the honest body for a wrapper (the
`wyrd_testkit::test_double_scan_page` delegation the brief cites is for doubles whose only
reader is their own `scan`, and the pre-existing `MovingRoot` in the same module uses it).

## What is still open for the human (unchanged from iteration 12, not re-litigated here)

The brief's own §6-class items: the `Completing`-less precursor committer landing before #636
(`Open questions` 4), the fitness of synthetic fixtures pre-#636, and the C5 mutant survivors.
This round adds no new NEEDS-HUMAN item and declares no missing external dependency.
