# Build notes — issue 635 / segmented-chunk-map (iteration 3)

**Withheld from the reviewer.** For the human at sign-off.

Base: `$PDCA_WORKTREE` = `/home/eddie/development/wyrd/wyrd.pdca-wt-l0`, detached at
`b0cd199` = `origin/main`. All `path:line` citations below are **post-patch** lines in that
worktree, which is what `patch.diff` produces.

---

## 1. What this iteration is

Iterations 1 and 2 built the slice; iteration 2 passed `cargo xtask ci` and the per-fix
red→green, and was rejected on two rows:

* **T4 batched rubric review — 10 blocking findings** (`review-batch.md`).
* **C5 mutants — 1 survivor of 246** (advisory).

So this iteration is **not a rebuild of the approach** — the approach (settled encoding,
one shared resolver, every consumer routed through it, staged publication) survived review
twice and is unchanged. It is a targeted repair of the ten findings plus the survivor, each
with a test that goes red when the repair is reverted (§4). The previous patch was
re-applied to the clean base and then edited, so the diff is iteration 2's plus the deltas
below.

`review-rejected.md`'s four recorded rejections (the X47 destination pre-mark, deferred to
#636) are untouched and their in-code `// deferred: #636` marker is still at
`crates/core/src/metadata.rs:2067-2076`.

---

## 2. The ten findings, and what each became

### (a) `metadata.rs:2268` ×3 — "batch sizing counts only segment puts … can exceed `MAX_BATCH_BYTES`"

Real, and the worst of the ten: a batch over the transaction envelope is rejected by the
backend **permanently** and not as a `Conflict`, so a publication that hit it could never
complete — the exact failure mode staged publication exists to avoid.

The split now charges the **whole assembled batch**, not the segment records it happens to
own. `batch_bytes` (`crates/core/src/metadata.rs:2561`) sums every precondition key +
expected value, every put key + value, and every delete key — the currency the trait states
the envelope in (`crates/traits/src/lib.rs:747-752`).

The awkward part is that the caller's contribution is a function of the split (it is called
once per batch with `SegmentBatchInfo`), while the split is a function of the contribution's
size. `staged_batches` (`:2373-2442`) resolves it by iteration: split with a reserve,
assemble, measure; if any batch is over, raise the reserve to the observed non-segment bytes
and re-split; bounded by `MAX_SPLIT_ATTEMPTS = 4` (`:2553`). A contribution whose size does
not depend on the batch count converges on pass 2. When it cannot converge — one segment
record plus that caller's contribution does not fit at all — it **fails closed** with
`ChunkMapError::BatchOverBudget` (`:444`, raised at `:2435`), naming the batch, its bytes
and the budget.

The same class applies to the **flip**, whose caller contribution is unbounded in principle
(`session → Completed`, `retire:records:{parts}`, `0016:654-663`) and which cannot be split
because it *is* the publication instant. `flip_batch` charges and refuses it the same way
(`:2484-2508`). This was not in the findings; it is the same defect one method over.

**On the "operation-count cap" half of one finding.** I did not add one. The seam declares
its envelope in **bytes** only ("10 KB key, 100 KB value, 10 MB and 5 s per transaction",
`crates/traits/src/lib.rs:747-752`); inventing a synthetic op count here would be a limit no
backend contract states and no test could calibrate. Every operation is charged through its
own key bytes, so an op-heavy batch is charged for being op-heavy. Because that is a partial
decline rather than a fix, it is **recorded** in `review-rejected.md` under "Round 2 — one
partial decline", per the T4 triage rule, so it does not silently re-surface next round.

### (b) `metadata.rs:2272` ×2 and `:2120` — "every batch reuses the same exact `segment_fence` … all subsequent multi-batch commits conflict"

Real, and a *design* contradiction rather than a slip: `segment_fence` was an exact-value
precondition on the `mpu:` record, and `segment_progress` was documented as CASing
`segments_written` on that same record. Batch 0 commits, the record changes, batch 1's
pinned `require` no longer matches — so **a publication large enough to need staging could
never finish**. The two fields could not both be right.

Fixed by collapsing them into **one** per-batch hook:
`segment_batch: Option<&dyn Fn(&SegmentBatchInfo) -> WriteBatch>`
(`crates/core/src/metadata.rs:2219`), whose returned preconditions *and* mutations are merged
into that batch. `SegmentBatchInfo::first_index` is documented as doubling as the
`segments_written` value the batch's fence requires (`:2260-2264`), so the caller states the
fence for the batch in front of it. This also *reduces* API surface (two fields → one) rather
than adding a knob.

### (c) `metadata.rs:2056` — "the segmented repoint arm never verifies that `prior_root` names the segment's group"

Real and the most dangerous of the ten. `require(seg == prior)` holds against a retired
segment record the reaper has not reached; `require(inode == prior_root)` holds against
whatever root the caller passed. Paired, a home resolved from a retired generation commits
and reports success while writing into a record no consumer will read again — the repair is
lost and the caller was told the fragment moved.

`repoint_chunk` now binds the home to the root's generation *before* it builds either
precondition (`:2122-2140`): the `seg:` key is parsed, its `(nonce, epoch)` must equal the
root's group (else `SegmentGroupMismatch`, `:464`), and the index must be one the root's
table names (else the existing `SegmentUnknown`).

### (d) `dst/tests/custodian.rs:1585` — "the 'ambiguous' arm never injects `CommitUnknownResult`"

Correct: the arm just re-ran `publish` and asserted `Conflict`, which is idempotency, not
ambiguity. `CrashMeta` now carries two **ordinal** faults (`crates/dst/tests/custodian.rs:177`,
`:184`): `die_at(n)` drops the n-th commit and every later one, `unknown_at(n)` **applies**
the n-th and then returns `CommitUnknownResult` (`:226`). The ambiguous arm now injects a
real one (`:1721`) and asserts the flip surfaces it as an `Err` that downcasts to
`CommitUnknownResult` (`:1727`) — never collapsed into an `Ok(Conflict)` a caller would read
as "nothing happened" over a write that did land. The re-read then settles it and the blind
recovery's flip loses its CAS.

### (e) `dst/tests/custodian.rs:1529` — "multiple segments but still only one 5 MB batch, so Tier-0 never exercises crashes, fencing, or cursor recovery between segment batches"

Correct. The fixture needed the phase to be several *transactions*, which at the real 5 MB
envelope needs ~113 000 chunks — unaffordable in a 50-seed sweep. Two changes:

* `SegmentedPublication::batch_budget: Option<usize>` (`crates/core/src/metadata.rs:2231`),
  **clamped** to `MAX_BATCH_BYTES` so a caller can only make batches smaller, never larger.
  The DST fixture sets it to one segment record per commit (`:1610`).
* `SegmentedPublication::resume_from: u32` (`:2225`) — how many leading segments a recovered
  `segments_written` cursor says are already durable. Without it the recoverable-progress
  story the code already *claimed* could not close: a restarted completer re-running the
  whole phase fences itself out against the cursor its own crashed predecessor advanced.

Property 10 now runs five arms (`:1549-1575`): interrupted **mid-phase**, resumed from the
recovered cursor, interrupted before the flip, raced, ambiguous. Each seed varies the chunk
count, so the batch boundary moves.

### (f) `backfill.rs:134` — "a segmented map with empty placements is skipped while reconciliation can return `Satisfied`; telemetry is neither an explicit error nor a repair obligation"

Correct against `AGENTS.md:175-177`. A gauge is a count, and a count nobody reads is exactly
the silent skip the rule forbids — worse, the pass then returned `Satisfied` over it.

The pass now records each such record and fails with a typed
`SegmentedPlacementUnfillable` (`crates/custodian/src/backfill.rs:305`, raised at `:204`,
re-exported at `crates/custodian/src/lib.rs:34`).

**Why the failure is raised at the END of the pass, not at the record.** Failing on the first
one would suppress `emit_remaining`'s drain gauge — the very observability the finding is
about — and would let one undrainable record stop the whole store from draining. So the pass
completes, fills everything it can, emits the gauge, and *then* fails
(`crates/custodian/src/backfill.rs:76-86`, `:204-211`). The error names the first record, its
unfillable chunk count, and how many such records the pass met.

**Why an error and not a queued repair obligation** (the rule allows either): the repair a
segmented empty placement needs is a *segment repoint*, and there is no queue for
"materialize a placement" — enqueuing a `repair:` record would mean "a fragment is missing",
which is false. The condition is also structurally impossible (a segmented map is produced
only by a multipart Complete, which always writes a full-length placement), so it fires on
corruption or on a producer that broke the rule. Both are cases where halting is right, and
ADR-0045 makes maintenance paths the strict side of the read/maintenance boundary.

---

## 3. The C5 survivor — and the five my own new code introduced

### 3.1 The reported one

`crates/custodian/src/backfill.rs:158:13: delete field size from struct InodeRecord
expression in reconcile` — the only miss of 246.

It is an **equivalent mutant**: the expression was

```rust
let next = InodeRecord { size: record.size, chunk_map: …, state: InodeState::Committed,
                         version: record.version + 1, ..record.clone() };
```

and `..record.clone()` supplies exactly `record.size` when the field is deleted. No test can
kill it, because there is no behaviour to observe. Writing one would be theatre.

Fixed by deleting the dead field instead (`crates/custodian/src/backfill.rs:177-186`).
`state: InodeState::Committed` went with it for the same reason — the scan filters on
`record.state == Committed` at `:81-83`, so it too was equivalent and would have become the
*next* survivor once the diff hunk shifted onto it. What remains named is `chunk_map` and
`version`, both of which real tests kill.

### 3.2 Five new ones, found by running the gate on my own work

Running `./scripts/mutants-in-diff` mid-iteration on the repaired diff reported **5 missed
of 280** — every one of them in the envelope accounting I had just written, and every one a
real hole rather than an equivalent mutant:

| Mutant | Why it survived |
|---|---|
| `:2415` `*bytes > budget` → `>=` | nothing exercised a batch landing *exactly* on the envelope |
| `:2427` `bytes - segments` → `+` | the re-split's reserve was over-estimated, giving one record per batch — still inside the envelope, so nothing noticed the wasted transactions |
| `:2500` `bytes > budget` → `>=` | same boundary, in `flip_batch` |
| `:2565` `key.len() + expected.len()` → `*` | `batch_bytes` was only ever compared against itself |
| `:2572` `+ deletes…` → `- deletes…` | same |

Killed by three additions, not by loosening anything:

* `batch_bytes_counts_every_key_value_and_precondition`
  (`crates/core/src/metadata.rs:3418`) — the unit of account asserted against a
  **hand-counted** batch (3+5, 2+0 for an absent precondition, 4+7, 10), with distinct
  non-degenerate lengths so `+`→`*` and `+`→`-` both diverge.
* An **inclusive-ceiling** arm in each of `the_split_charges_…` (`:3376-3390`) and
  `the_flip_batch_refuses_…` (`:3552-3560`): a batch of exactly *N* bytes fits an *N*-byte
  envelope, and *N*−1 does not.
* A **minimality** arm in `the_split_charges_…` (`:3363-3372`) over a budget several records
  wide (`RESPLIT_BUDGET = 1_400`), asserting each batch plus the next batch's first record
  exceeds the envelope — the property an over-estimated reserve violates while staying
  "inside the budget".

Final run: **280 mutants, 0 missed, 135 caught, 145 unviable.**

---

## 4. Refuting my own tests (the three forced questions)

### (a) Genuine red — does it fail with the fix reverted?

**Yes, and measured, not asserted.**

**Leg A, through the project's own runner** (`./engine/scripts/run-verify.sh`, which reverts
every modified production file, deletes added non-test files, and keeps only the added test):

```
run-verify.sh: GREEN — cargo test -p wyrd-custodian --test segmented_map_consumers (fix applied)
test result: ok. 8 passed; 0 failed
run-verify.sh: RED — (production reverted, test kept)
test result: FAILED. 0 passed; 8 failed
run-verify.sh: PASS — red without the fix, green with it.
```

The brief asks for this explicitly: **8 tests ran on the RED leg and all 8 failed, and the
red is assertions, not a build error.** The only `error:`-prefixed line in the whole RED log
is cargo's own `error: test failed, to rerun pass …` summary; there is no `error[Ennnn]`. The
panics are at `segmented_map_consumers.rs:504`, `:607`, `:671`, `:704`, `:775`, `:865`,
`:971`, `:1024` — e.g.

```
reconcile_step must resolve a segmented chunk map, not fail on it:
  Some("reconciliation store access: invalid type: map, expected a sequence at line 1 column 23")
```

**This iteration's own deltas, each reverted individually and re-run** (all restored
afterwards; `git diff` is empty against the index):

| Reverted | Test that went red |
|---|---|
| per-batch fence → pinned (`first_index: pending[0]…`) | `metadata::tests::a_multi_batch_phase_re_fences_each_batch_and_runs_to_completion`, **and** DST `staged_publication_is_atomic_at_the_flip` |
| envelope charged on segment puts only | `the_split_charges_the_callers_contribution_against_the_envelope` — *"batch 0 carries 1622 bytes, over the 1400-byte envelope"* |
| repoint generation binding removed | `a_segment_repoint_refuses_a_home_from_another_generation` |
| `resume_from` ignored | `a_resumed_publication_writes_only_the_segments_the_cursor_has_not` |
| flip envelope check removed | `the_flip_batch_refuses_a_caller_contribution_over_the_envelope` |
| backfill records no unfillable entry | `the_remaining_gauge_counts_a_segmented_records_empty_placement_too` **and** leg A's `backfill_never_rewrites_a_segmented_map_even_with_an_empty_placement` |
| `CrashMeta::die_at` a no-op | DST property — *"the completer died mid-phase: Committed"* |
| `CrashMeta::unknown_at` a no-op | DST property — *"an unknown result is an Err, never an Ok outcome: Committed"* |
| `*bytes > budget` → `>=` (`:2415`) | `the_split_charges_the_callers_contribution_against_the_envelope` |
| `bytes - segments` → `+` (`:2427`) | same |
| `bytes > budget` → `>=` (`:2500`) | `the_flip_batch_refuses_a_caller_contribution_over_the_envelope` |
| `key.len() + expected.len()` → `*` (`:2565`) | `batch_bytes_counts_every_key_value_and_precondition` |
| `+ deletes…` → `- deletes…` (`:2572`) | same |

Each revert was applied to the real source, re-run, and reverted; `git diff` against the
index is empty at the end of the run. The last five rows were additionally confirmed by
`cargo mutants` itself (§3.2) — they *are* the mutants it generates.

### (b) Production path — does the test drive the real thing?

**Yes.** Leg A drives the real `reconcile_step`, `reconcile_after_restore`,
`reconciliation_status`, `backfill::reconcile`, `read_object` and the real reconstruction /
rebalance passes over in-memory *trait* implementations (the store and D-server seams), not
doubles of the passes. Leg B drives the real `SegmentedPublication` and the real
`RedbMetadataStore::in_memory()`. The DST property drives the real committer over the DST
campaign's own store double, which is the harness's established seam.

The only stand-in anywhere is the **caller** that supplies the publication precondition and
the per-batch contribution — which is exactly the seam the brief declares as #636's
(`Production reach` (a)).

### (c) Fixture includes the fault?

**Yes, and the finding-driven ones are the point of this iteration.** The DST crash is
*injected* (`die_at`), not simulated by not-calling; the unknown result is injected on a
commit that **actually applied**, so the arm exercises the case where the write landed. The
backfill fixture carries a segmented record with a genuinely **empty** placement — the one
input that reaches the rewrite — rather than a full-length one that would short-circuit
before the decision. The envelope test uses a contribution big enough to breach the budget
(≈316 B against 1 400 B, which overflows the first, unreserved split) and then one big
enough that no split fits at all (5 000 B against 700 B), so the converging arm, the
inclusive-boundary arm and the fail-closed arm are each exercised.

Leg A(ii)'s drain oracle is still the positive one the brief demands: the segmented object's
fragments make the drain answer `Pending`, which a resolver that decoded the shape but never
read the `seg:` range would fail.

---

## 5. Gates run here

All three run **against the final `patch.diff`**, through the project's own runners, not
hand-rolled invocations:

| Gate | Result |
|---|---|
| `./engine/xtask.sh ci` (fmt, clippy, build, test, machete, deny, conformance, statics, guards, **DST 50 seeds**, typos, docs) | **`xtask ci: all checks passed`**, exit 0 |
| `./engine/scripts/run-verify.sh` | **`PASS — red without the fix, green with it.`** — GREEN 8 passed / 0 failed, RED 0 passed / **8 failed**, exit 0 |
| `./scripts/mutants-in-diff` | **280 mutants, 0 missed**, 135 caught, 145 unviable, exit 0 |
| `cargo fmt --all -- --check` | clean — run last, before the diff was cut; this is the target's own commit hook and no PDCA gate models it |

---

## 6. Base resolution — read this, it is the one thing a gate cannot tell you

The brief's `Falsifiability` section says this is a **wave-1** bundle whose base is
`origin/pdca-integration/main` (= `origin/main` + #634), and that the added test's `MemMeta`
"must implement `scan_page` (#634's required method, one delegating line)".

**That is not the tree I was given.** `$PDCA_WORKTREE` is detached at `b0cd199` =
`origin/main`; `origin/pdca-integration/main` does not exist; and `grep -rn scan_page
--include=*.rs` over the whole worktree returns **nothing**. #634 exists only as an unmerged
branch, `origin/enhancement/634-scan-page-seam` @ `18180a2`.

So adding `scan_page` to any double would be `E0407` ("method is not a member of trait") on
this base and would fail the **gating** C4-ci. I built against the tree as it is. This is
also why iteration 1's sign-off saw `E0046` on the "normative stack": the stack wants the
method, this base rejects it.

**What the fold onto #634 will need** — five one-line `scan_page` delegations, one per
`impl MetadataStore` this patch *adds*:

| File | double |
|---|---|
| `crates/core/src/metadata.rs` (test module) | `Shuffling`, `Impostor`, `MovingRoot` |
| `crates/custodian/tests/segmented_map_consumers.rs` | `MemMeta` |
| `crates/server/src/lib.rs` (test module) | the co-located store double |

Every *pre-existing* double this patch touches (`crates/dst/tests/custodian.rs`'s `MemMeta`
and `CrashMeta`, `crates/custodian/tests/*.rs`, …) already receives its `scan_page` from
#634's own patch, so the fold conflict is confined to the five above. `pdca.toml` has
`regate_between_waves = false`, so no gate will catch this for you at fold time.

---

## 7. Mutation run

`./scripts/mutants-in-diff` over the final `patch.diff`:

```
280 mutants tested in 6m: 135 caught, 145 unviable
```

**0 missed** (iteration 2: 1 missed of 246; the diff grew, so more mutants). The
iteration-2 survivor is gone by construction — the line it mutated no longer exists — and
the five my own new envelope code introduced are killed by the tests in §3.2.

The gate was run three times: on the repaired diff (5 missed → the §3.2 work), on the diff
with those tests added (0 missed), and once more on the final diff after two **comment-only**
clarifications (§8's rubric pass), which generate no mutants either way.

---

## 8. Self-review against the target's standing rubric

`AGENTS.md`'s `## Review rubric & protocol` (`:122-210`) is what the reviewers apply, so I
walked the diff against it as the last step. Two clauses produced an edit; the rest are
recorded so the human can see they were checked, not skipped.

**Hard conventions.** No clock read is added anywhere (ADR-0009 n/a). No new dependency edge:
the resolver and `batch_bytes` live in `core`, which `custodian` already depends on, and no
crate root is created (so `#![forbid(unsafe_code)]` is n/a). No DST-reachable global mutable
state — `CrashMeta`'s three atomics are per-instance fields inside the test, and the statics
gate inside `cargo xtask ci` is green. Metadata validation boundaries: every new failure
(`BatchOverBudget`, `ResumePastPlan`, `SegmentGroupMismatch`, `SegmentedPlacementUnfillable`)
is a typed error at the boundary, never a value a consumer could half-use. Docs currency: this
iteration adds no port, API operation, RPC, CLI flag or persisted field — the segmented
record classes were already documented in `06-runtime-view.md` / `08-crosscutting-concepts.md`
and that edit is retained; the per-batch fence, resume cursor and envelope charge are internal.

**Recurring defect classes.** Two hit:

* *Transactions* — "an aggregate error must let `CommitUnknownResult` outrank `Conflict` —
  never report a dropped write as a clean conflict." Two places. (1) `commit_batches` and
  `flip` propagate a store `Err` with `?` and never fold it into `Ok(Conflict)`; that is now
  **pinned** by the DST ambiguous arm rather than merely true. (2) `backfill::reconcile`
  now *aggregates* — it defers `SegmentedPlacementUnfillable` to the end of the pass — so I
  had to check the ranking holds: a store error still pre-empts the diagnostic, because every
  `?` in the loop fires first. I added that sentence to the function's doc
  (`crates/custodian/src/backfill.rs:82-84`) rather than leave a reviewer to derive it.
* *Test fidelity* — "DST/sim models mirror the production adapter's error and seam
  semantics." `CrashMeta::unknown_at` returns the real `wyrd_traits::CommitUnknownResult`
  with `may_still_commit: false`, which is FoundationDB's 1021 shape. `die_at` deliberately
  returns a plain error instead: nothing was applied, so nothing is indeterminate, and
  labelling it `CommitUnknownResult` would model a case the arm is not testing. I made that
  explicit in its doc (`crates/dst/tests/custodian.rs:173-176`) so it reads as a decision.
  The same clause's "a new destructive or concurrent path lands with seeded Tier-0 DST
  coverage" is why `resume_from` and `batch_budget` arrived with property-10 arms rather
  than unit tests alone.

The others do not touch this diff: no RFC grammar is parsed, no probe or readiness surface
moves, no workflow file is edited, no new unbounded await is introduced, and serialization
identity is unchanged (and still asserted by leg B(i)).

**Reviewer protocol.** The four `review-rejected.md` deferrals are *settled* under the
"Deferrals are settled" rule and were not re-litigated; their `// deferred: #636` marker
stands. Nothing here is a DCO finding.

---

## 9. Things I deliberately did **not** do

* **Did not touch `0016`, any ADR, or any spec.** The editorial contradiction in 0016
  decision 7(a) (upload-id-keyed groups) is implemented per the corrective §1 rule and left
  for #628 to fix in prose, as the brief directs.
* **Did not add a `Cargo.toml` change.** The brief forbids it (a modified manifest is
  reverted on the RED leg, which would turn leg A's assertion-red into a build error). None
  was needed.
* **Did not add a second `tests/*.rs` file.** Leg B stays co-located in the production
  modules and the X51 interleaving stays appended to the existing
  `crates/dst/tests/custodian.rs`, exactly so `run-verify.sh` keeps leg A alone on the RED
  leg.
* **Did not fold the root flip into #636** (the brief's `Open questions` 4 alternative). The
  fence-and-mutations-as-a-parameter shape works and is now genuinely per-batch, so the
  atomicity requirement ("the caller's mutations ride the flip's own batch") is met without
  moving the committer.
* **Did not implement the X47 destination pre-mark.** Still deferred to #636 per
  `review-rejected.md`; the in-code marker is unchanged.

---

## 10. Scratch

Everything throwaway went to `${PDCA_SCRATCH}/pdca-builder-635-*` (`-ci*.log`,
`-verify*.log`, `-mutants*.log`, and the `.bak` files used for the refutation reverts). All
removed at the end of the run; nothing was written to `/tmp`. The worktree ends with
`git diff` empty against its index and no untracked files, so `patch.diff` is exactly the
tree the three gates were run against.

## 11. STOP discipline

Nothing was pushed, no branch was created, no PR was opened or marked ready. The bundle is
`patch.diff` + `build-notes.md` (+ the one added test, carried inside the patch at
`crates/custodian/tests/segmented_map_consumers.rs`).
