# Build notes — issue #635 (segmented-chunk-map), iteration 5

> Withheld from the reviewer; written for the human at sign-off.
> Line numbers are on the **patched** tree (`$PDCA_WORKTREE` = `wyrd.pdca-wt-l0`, base
> `origin/main` @ `b0cd199`) unless a base ref is named.

## 0. What this iteration is

Iteration 4 was green on `cargo xtask ci`, on the per-fix red→green gate and on the advisory
mutation row, and was auto-iterated on **three** things:

1. **T4 batched review — 8 blocking findings** (`review-batch.md`, round 4).
2. **C5 [impl]** — "route the public snapshot read through the resolver or remove it as a
   segmented-map consumer … one read path opaque despite the every-consumer invariant
   (`crates/core/src/read.rs:72`)".
3. **T5 [impl]** — "restore #634 stack evaluability … a main-only green run cannot establish
   implementation sign-off while the normative stack executes zero binding tests
   (`crates/custodian/tests/segmented_map_consumers.rs:83`)".

All three are addressed. The design is unchanged (the settled `Flat | Segmented` encoding,
the `seg:`/`seggrp:` records, the one shared resolver, the staged-publication committer) —
none of the findings challenged it; every one was an enforcement hole inside it, and (2) was
a consumer that the invariant's own self-test says must not exist.

**One earlier decision is withdrawn, not re-argued.** Round 2 declined the operation-count
half of a finding on the grounds that no backend states an op limit. That was wrong on the
axis that matters: proposal **0016 states `B_ops` normatively** — "`B` is therefore
`min(B_bytes, B_ops)` … an operation-derived cap `B_ops` calibrated so that a batch's
sequential round trips fit the transaction deadline … every row of the batch inventory whose
mutation or precondition **count** can grow is bounded by both" (`0016:640-648`). It is
implemented this round; `review-rejected.md` records the withdrawal.

## 1. The eight findings, C5 and T5 — what each became

| # | Finding (round-4 `review-batch.md`) | Fix | Test |
|---|---|---|---|
| 1 | `metadata.rs:1059` — `replace_chunk` validates only length, so a repoint can change the chunk **id** or **EC scheme** and publish different content under a committed identity | the mutator takes a **placement vector**, not a `ChunkRef`: identity, framing and extent are carried from the CAS'd record and are *unspellable* (`SegmentRecord::repoint`, `crates/core/src/metadata.rs:1127`; shared body `repoint_placement`, `:1143`) | `a_repoint_can_move_a_chunk_and_change_nothing_else` (`:5543`) |
| 3 | `metadata.rs:2238` — the entry accepts a whole `ChunkRef`, and the **flat** arm does not even preserve length | same change one level up (`repoint_chunk`, `:2364`); both arms now go through `repoint_placement`, so they cannot diverge. Callers pass the placement they computed (`crates/custodian/src/reconstruction.rs:577`, `crates/custodian/src/rebalance.rs:305`) | `a_flat_repoint_rewrites_the_map_and_bumps_the_version_by_one` (`:5396`) |
| 2, 4 | `metadata.rs:2752`,`:2800` — `check_fenced` accepts **any** precondition as the publication fence | the publication **declares** its fence record (`SegmentedPublication::fence_key`, `:2495`) and every segment-write batch **and** the flip must carry a *value* precondition on that key (`check_fenced`, `:3053`); `require_absent` does not count — the rollback that deletes the session would satisfy it | `a_publication_not_fenced_on_its_fence_record_is_refused_in_either_phase` (`:4608`) |
| 5, 8 | `metadata.rs:2833` (twice) — caller mutations merged without rejecting collisions with committer-owned keys | `merge_contribution` (`:3108`) refuses a caller put/delete on the inode root, this group's `seg:` range, or the `seggrp:` marker (`OwnedKeys`, `:2980`); preconditions on them stay allowed | `a_caller_contribution_may_not_write_the_publications_own_records` (`:4445`) |
| 6 | `metadata.rs:2085` — a fully readable **stale** snapshot is returned without verifying the current root | the resolve-retry re-read now settles a **complete** read too (`:2096`, `root_still_names` `:2105`): a resolution is `Resolved` only if the root still names the group it resolved, so `resolve_live_chunk_map` restarts onto the live generation | `a_complete_resolve_of_a_superseded_generation_is_retired_and_restarts` (`:4523`) |
| 7 | `metadata.rs:2566` — split by bytes only, no operation-count cap | `MAX_BATCH_OPS` = `B_ops` (`:326`), charged by `batch_ops` (`:2964`), enforced in the split (`batch_ranges`, `:3151`; the reserve loop re-splits against the caller's measured op contribution) and in the unsplittable flip (`:2765`); `ChunkMapError::BatchOverOps` (`:593`); per-publication knob `ops_budget` (`:2545`) | `a_batch_is_bounded_by_operations_as_well_as_bytes` (`:4339`) |
| C5 | `read.rs:72` — the public snapshot read is a `.chunk_map` consumer that can only **refuse** a segmented map | the entry stops being a consumer: `read_object_chunks(chunks, &[ChunkRef], size)` (`crates/core/src/read.rs:69`) takes the **resolved** list, and `ReadError::SegmentedMapNeedsStore` is deleted | `the_snapshot_entry_reads_a_segmented_objects_resolved_chunks` (`crates/core/src/read.rs:933`) — a real segmented object, published by the real committer over a real redb store, spanning >1 `seg:` record, read back byte-for-byte |
| T5 | the normative stack (#634 folded first) did not build | verified end-to-end in a scratch worktree; the 66-line fold delta ships as `stack-634-fold.diff` — see §6 | 8/8 binding tests green **on the stack**, plus 1 073 tests across 154 targets |

Invariant self-test (the brief's): after C5 there is **no** `.chunk_map` consumer left that
understands one representation and is opaque to the other. `git grep '\.chunk_map' crates/*/src`
now yields only: the resolver itself, the two writer-side refusals that are explicitly #636's
scope (`commit_chunk_map*` / `unlink` retirement of a segmented generation, fail-closed with
`SegmentedRetirementUnsupported`), and `backfill.rs:163`'s **stated decision** to skip a
segmented record with a reason (brief Open question 3, asserted by
`backfill_never_rewrites_a_segmented_map_even_with_an_empty_placement`).

## 2. Why these shapes, and what was rejected (with the cost)

**Repoint: remove the door rather than guard it.** The reviewer asked for id/scheme
validation. Validating leaves the wrong thing expressible — a future caller can still *build*
a divergent `ChunkRef` and get a typed error at run time. Taking `Vec<DServerId>` makes the
illegal state unrepresentable, and the diff is *smaller*: 2 call sites lose 2 lines each
(`reconstruction.rs:577`, `rebalance.rs:305` no longer clone-and-mutate a `ChunkRef`), one
struct field disappears (`RepairPlan::chunk_ref`), and the validation shrinks to one
placement-length check shared by both arms. The one thing it does validate is the *length*
(`placement.len() == fragment_count()`), because a short vector identity-fills its missing
tail at the next read (ADR-0040 decisions 3–4, strict on maintenance writes).

**Fence: a declared key, not "some precondition".** Rejected alternatives:
* *Require the same precondition (key **and** value) in every batch* — impossible by design:
  the fence record is also the progress cursor (`0016:350`, decision 7(c)/(d)), so its value
  legitimately changes per batch; requiring value-equality would make any multi-batch
  publication `Conflict` against its own predecessor.
* *Have the flip re-verify the segments* (`require(seg == prior)` per segment) — concrete
  cost: up to `MAX_ROOT_SEGMENTS` = 512 preconditions carrying full segment values at
  ~50 000 B ⇒ ~25 MB in one batch against a 5 MB envelope. Unbuildable for any map big
  enough to need segmentation.
* *Document it* — that is iteration 3, and the finding is that documentation is not a check.

**Contribution collisions: refuse, don't hope.** The alternative is prose ("callers must not
write the publication's records"). The failure it admits is silent by construction: both
mutations ride one transaction, so the backend cannot report a conflict — the publication
returns `Committed` over the caller's bytes. Three `if`s in one loop (`:3108`-`:3130`) versus
a class of corruption no gate could see.

**`B_ops`: the number is sourced, not invented.** `MAX_BATCH_OPS = 1_000` is the largest count
0016's own inventory budgets for (the retirement drain step, "~1,000 small orphan marks",
`0016:667`), and the doc's rule is `min(B_bytes, B_ops)` with `B_ops` "deployment-tunable
inside a range" — which is why it is a clamped per-publication knob (`ops_budget`), not a
hard-wired constant. Rejected: *charging a fixed per-operation byte overhead* instead — it
would bound the count only implicitly, through a number even less grounded than the cap, and
would silently change the byte arithmetic the envelope tests pin.

**The currency re-read: one `get`, on the segmented path only.** Rejected alternatives:
* *Compare a generation token without re-reading* — there is nothing to compare against; the
  root **is** the token, and only a read tells you the current one.
* *Re-read only in maintenance consumers* — that is the #508-attempt-4 failure class in
  mirror image (each consumer deciding its own safety), and it costs exactly the same one
  `get`. Put in the resolver, every consumer inherits it.
* Cost measured, not asserted: one extra `MetadataStore::get` per **segmented** resolve
  (flat resolves read nothing extra — `resolve_chunk_map` returns before the segment path,
  `:2145`).

**C5 — remove the consumer, don't add a store to it.** The tempting alternative is
`read_object_from(meta, chunks, inode_id, &inode)` routed through `resolve_live_chunk_map`.
That does not remove the opaque consumer, it **duplicates an existing one**: `read_object`
(`read.rs:495`) already takes exactly those inputs, resolves through the same call, and
additionally enqueues the repair obligations a read discovers — the two would differ only by
that enqueue, a distinction no caller wants. Concrete churn either way is the same 20 call
sites; the chosen shape is 20 one-line edits **and** deletes a public error variant (12 lines
with its `Display` arm), while the alternative adds an argument to all 20 and leaves the
resolution question answered twice in the same module.

## 3. Red → green evidence (actual numbers, per the brief's Falsifiability clause)

Run through the project's own runner, not a hand-rolled command:
`PDCA_BUNDLE=… ./engine/scripts/run-verify.sh` (the C4-verify gate), which resolves the base
as `origin/main` (INTEGRATION §2 precedence; `$PDCA_VERIFY_BASE` is **not** exported on this
host — see §6).

```
run-verify.sh: GREEN — cargo test -p wyrd-custodian --test segmented_map_consumers (fix applied)
running 8 tests
test result: ok. 8 passed; 0 failed; 0 ignored
run-verify.sh: RED — cargo test … (production reverted, test kept)
running 8 tests
test result: FAILED. 0 passed; 8 failed; 0 ignored
run-verify.sh: PASS — red without the fix, green with it.
```

**8 tests ran on the RED leg and 8 failed, and every failure is an assertion / decode
failure, not a build error** — e.g. `called Result::unwrap() on an Err value:
Error("invalid type: map, expected a sequence", line: 1, column: 23)` at
`crates/custodian/tests/segmented_map_consumers.rs:704`, which is the base's strict
`metadata::decode(&value)?` (`crates/custodian/src/gc.rs:256`) meeting a segmented value. That
is the brief's requirement, and it also proves the base is the one the brief assumes.

**Whole gate**: `./engine/xtask.sh ci` → `xtask ci: all checks passed` (fmt, clippy incl.
`--cfg madsim`, build, test, deny, conformance, statics, deploy-guard, prose gates — `typos`
and the doc renderer are both installed here, so the docs edit is really gated).

**Workspace**: `cargo test --workspace --exclude wyrd-dst` → 0 failures; DST
(`RUSTFLAGS=--cfg madsim`, `MADSIM_TEST_NUM=5`) → 13/13 in `crates/dst/tests/custodian.rs`,
including `staged_publication_is_atomic_at_the_flip` and
`segmented_resolve_never_tears_on_retirement`.

**Mutation (advisory C5 row)**: `scripts/mutants-in-diff` over this bundle's patch —
**`315 mutants tested in 7m: 0 missed, 160 caught, 155 unviable`**. It did not start there:
the first run over this round's code reported **8 missed**, every one of them in the new
operation-cap arithmetic. I killed all eight rather than explain them — see §5b.

## 4. The forced refutation (a)/(b)/(c)

**(a) Genuine red?** Yes, and per fix rather than in aggregate. Leg A: 8/8 fail with
production reverted, 8/8 pass with it (§3). Each of the five new guards was individually
reverted and re-run — §5 records the exact revert and the exact failure. The sixth change
(C5) is a **removal**: its test drives an input the pre-fix entry could not accept at all
(the pre-fix entry took an `InodeRecord` and answered `Err(SegmentedMapNeedsStore)` for a
segmented one — `iteration-v4/patch.diff`, `read.rs:72-74`), so there is no compiling
"revert"; the honest statement is that the test is impossible to write against the old
signature, and the old signature's only segmented behaviour was the refusal the finding
objected to.

**(b) Production path?** Yes. Every new assertion drives production code: `staged_batches` /
`assemble_segment_batches` / `flip_batch` / `merge_contribution` / `check_fenced` on the real
`SegmentedPublication`; `repoint_chunk` and `SegmentRecord::repoint` exactly as
reconstruction and rebalance call them (both call sites updated, both exercised by their own
suites and by the DST repoint property); `resolve_chunk_map` / `resolve_chunk_homes` /
`resolve_live_chunk_map` as GC, restore, scrub, rebalance, reconstruction, backfill and both
read paths call them; `read_object_chunks` over a real `RedbMetadataStore::in_memory()` and a
map published by the real committer. The only stand-ins are the **callers** the committer is
parameterised over (the session's fence and mutations, which are #636's) and store doubles
that inject interleavings a real backend can produce.

**(c) Fixture includes the fault?** Yes, and this round's headline fixture is the clearest
case: `a_complete_resolve_of_a_superseded_generation_is_retired_and_restarts` publishes
generation A, supersedes it with generation B, and **leaves A's `seg:` records in the store**
(asserting they are all still readable) — the failing element is present, not curated out, so
the resolve genuinely reads every segment and must still refuse to call them the object's
chunks. Likewise: the fence tests contribute a real, well-formed precondition **on the wrong
record** (the exact shape a non-emptiness check admits); the collision tests contribute a
real put *and* a real delete on each owned key class; the op-cap test drives the **real**
`MAX_BATCH_OPS` from the caller's own contribution and asserts in the same breath that the
batch is three orders of magnitude **inside** the byte envelope (so it cannot pass by being
big); the repoint tests use an EC chunk whose `fragment_count()` is 3, so a 2-entry placement
is genuinely malformed rather than trivially rejected.

## 5. Per-fix refutation detail (what I reverted, what actually failed)

Executed, not assumed. Each revert was applied to the working tree, the test re-run, and the
original restored immediately (`crates/core/src/metadata.rs` was byte-restored from a backup
under `$PDCA_SCRATCH`; the final tree was re-checked with `cargo fmt --check`, the full core
suite, and a byte-comparison of a freshly regenerated `git diff` against the shipped
`patch.diff` — **identical**). Where deleting a call site would have produced a dead-code
build failure (`-D warnings`), I neutered the *body* instead so the call sites stayed live.

| Fix reverted | Test | Observed |
|---|---|---|
| `check_fenced` → "any precondition" (the pre-fix rule) | `a_publication_not_fenced_on_its_fence_record_is_refused_in_either_phase` | RED — `unwrap_err()` on an `Ok`: the batch fenced on `dirent:1:big.bin` builds and would publish |
| `OwnedKeys::owner_of` → always `None` | `a_caller_contribution_may_not_write_the_publications_own_records` | RED — the assembled segment batch carries `inode:1 = "mine"` beside the publication's own records |
| the currency re-read on the success path disabled | `a_complete_resolve_of_a_superseded_generation_is_retired_and_restarts` | RED — `left: Resolved([… the RETIRED generation's 40 chunks …]) right: Retired` |
| the op cap disabled (split + flip + measure) | `a_batch_is_bounded_by_operations_as_well_as_bytes` | RED — "batch 0 carries 34 operations, over the 20 cap" |
| the placement-length check disabled | `a_repoint_can_move_a_chunk_and_change_nothing_else` **and** `a_flat_repoint_rewrites_the_map_and_bumps_the_version_by_one` | RED — both: a 2-entry placement is accepted onto a 3-fragment chunk and the batch is built |
| the currency re-read disabled (again, against the **DST** arm) | `segmented_resolve_never_tears_on_retirement` (`crates/dst/tests/custodian.rs:1464`) | RED — `left: Resolved([… the retired generation's 2 chunks …]) right: Retired` |

## 5b. The eight surviving mutants, and how each was killed

The first mutation run over this round's code reported 8 survivors — **all** of them in the
new operation-cap arithmetic, none anywhere else in the patch. Two changes killed them:

1. **The refusal is classified where it is measured.** `staged_batches` used to filter for
   "over either envelope" and then re-ask `if first.bytes > budget` to pick the error. The
   re-ask was untestable at its boundary (a batch at exactly the byte budget with too many
   operations), so the mutant `>=` survived. The classification is now built in the same
   pass as the filter (`crates/core/src/metadata.rs:2751-2780`), one comparison per
   currency, and both are pinned by tests at their exact boundary.
2. **The convergence check compares the reserves themselves.** The two-condition
   `if contribution <= reserve && contribution_ops <= reserve_ops` had three survivable
   operators, none of which changed the outcome of any fixture. It is now
   `let next = (reserve.max(contribution), reserve_ops.max(contribution_ops)); if next ==
   (reserve, reserve_ops) { break }` (`:2786-2790`) — one comparison, and mutating it to
   `!=` breaks the very first re-split, which two existing tests require.

Plus four targeted assertions: `batch_ops_counts_every_precondition_and_mutation` (`:4506`),
the inclusive operation boundary on the flip (`:4489`), "the split fills a batch to the cap"
(`:4402`), and the operation cases in `the_batch_split_is_byte_budgeted_and_never_drops_a_record`
(`:4920`).

Rather than trust the gate's next run, I **hand-applied all nine mutants** (the eight
reported plus the one my restructure introduces, `m.ops >= ops_budget`) and re-ran
`cargo test -p wyrd-core --lib` for each: **9 CAUGHT, 0 SURVIVED**, tree byte-restored
afterwards (script and log under `$PDCA_SCRATCH`, removed). The gate then agreed:
`315 mutants tested in 7m: 0 missed`.

## 6. For the human at sign-off

* **NEEDS-HUMAN (unchanged, structural): T3 / Validation are precursor-only.** No production
  path publishes a segmented map until #636 lands the session, which the brief states is
  correct (`0016:2287-2299`). The publication evidence therefore uses a test-supplied caller
  contribution at the seam (`crates/core/src/metadata.rs:2495`,`:2555`). Nothing in this
  slice can change that; it is a maintainer decision about freezing the durable format and
  the committer API before its first production caller.
* **NEEDS-HUMAN (one decision to make): the #634 fold delta.** This bundle's worktree is
  `origin/main` @ `b0cd199`; the wave-1 stack base `origin/pdca-integration/main` **does not
  exist on this host**, and INTEGRATION §2 records why (harness gap eduralph/pdca-harness#273
  — a bundle-scoped gate is never told the fold branch, so C4-verify resolves `origin/main`).
  On `origin/main`, `MetadataStore::scan_page` is not a trait member, so a double carrying it
  is `E0407` and **both** gates go red — which would destroy leg A's assertion red, the most
  valuable evidence this slice has. On the folded stack the same six doubles are `E0046`
  without it. No `cfg` can span the two: #634 makes `scan_page` a *required* method on
  purpose, and `cfg(accessible)` is unstable. So the fold edit cannot live in `patch.diff` —
  it is shipped beside it, **and it is verified**:

  ```
  git worktree add --detach $SCRATCH/pdca-builder-635-stack enhancement/634-scan-page-seam   # 18180a2
  git apply --3way patch.diff        # applied with ZERO conflicts
  git apply stack-634-fold.diff      # 66 lines: 6 delegating `scan_page` bodies
  cargo check --workspace --all-targets                       # clean
  cargo test -p wyrd-custodian --test segmented_map_consumers # 8 passed, 0 failed
  cargo test -p wyrd-core --lib                               # 70 passed, 0 failed
  cargo test --workspace --exclude wyrd-dst                   # 1073 passed across 154 targets, 0 failed
  RUSTFLAGS=--cfg madsim MADSIM_TEST_NUM=5 cargo test -p wyrd-dst   # all green (custodian: 13/13)
  ```

  Re-verified end to end against the **final** `patch.diff` (both diffs applied with
  `--3way`, zero conflicts, in a second scratch worktree that was then removed), and
  re-checked once more after the last comment-only edit.

  The delta is `stack-634-fold.diff` in this bundle: one 9-line delegation to #634's own
  `wyrd_testkit::test_double_scan_page` in each of `Shuffling`, `Impostor`, `MovingRoot`
  (`crates/core/src/metadata.rs`), `MemMeta`
  (`crates/custodian/tests/segmented_map_consumers.rs`), `StaleScan`
  (`crates/custodian/tests/backfill.rs`) and `SupersedeMidResolve`
  (`crates/server/src/lib.rs`). Apply it at the fold (or ask #634 to land first and I will
  fold it into `patch.diff` on the next run). **This is the answer to the T5 row: the
  normative stack now executes 8/8 binding tests, not zero.** I did not reduce the six to
  three by giving the resolver a narrow `get`+`scan` seam of its own: that would add a second
  public store trait to `core` plus an `#[async_trait]` blanket impl and thread a generic
  through eight resolver entry points and their call sites in `core`/`custodian`/`server`
  (~55 lines added, ~30 signature lines changed) to delete 33 lines of one-line test-double
  delegation — a bigger reviewable surface to shrink a mechanical fold.
* **T4**: all eight round-4 findings are **fixed** (table in §1 and in `review-rejected.md`),
  so they should leave the next run. The round-2 op-count decline is **withdrawn** — it is
  now implemented. Rounds 1's four deferral rows (the X47 destination pre-mark, `deferred:
  #636`) still stand; the rubric's *Deferrals are settled* rule covers them.
* **`scripts/review-branch --bundle` is not available to me** (it is the gate's own tool, and
  the previous rounds' Check notes record it as absent here), so I could not re-run the
  batched review myself to confirm the eight are gone. The fixes are each backed by a test
  and a recorded revert (§5).
* **Advisory C5 mutation row**, from `scripts/mutants-in-diff`: **`315 mutants tested in 7m:
  0 missed, 160 caught, 155 unviable`** (first run this round: 8 missed, all in the new
  op-cap code — §5b). It ran on this patch modulo one later **comment-only** edit (a stale
  `[Self::replace_chunk]` rustdoc link, `crates/core/src/metadata.rs:1061`), which changes no
  executable line and therefore no mutant; `cargo xtask ci` and the red→green gate were both
  re-run *after* it and are the numbers quoted in §3.
* **One flake you may hit, and it is not this patch.** Twice today
  `crates/server/tests/custodian_gc.rs`'s `deployed_role_*` tests hung (once inside `xtask
  ci`, once inside the mutation gate's *unmutated baseline*) while several heavy cargo jobs
  were running concurrently; the processes sat in a futex wait, not a loop. That file is
  **not touched by this patch** (`grep -c custodian_gc patch.diff` → 0), it passes in 0.16 s
  standalone (run three times), and both the CI gate and the mutation gate passed on the
  final tree once nothing else was competing for the machine. Those tests drive the real
  deployed loop with a wall-clock shutdown timer and a Prometheus telemetry exporter per
  test, several concurrently in one process — a pre-existing timing sensitivity worth an
  issue of its own, but not a signal about this slice. (Iteration 3 hit the same class:
  "cargo test failed in an unmutated tree".)

## 7. Housekeeping

Scratch lived under `$PDCA_SCRATCH` (`/var/tmp/pdca`), all named `pdca-builder-635-*`: the
two stack worktrees (`…-stack`, `…-stack2`, each removed with `git worktree remove --force`
and their build dirs deleted — 16 GB apiece), the metadata backups used by the §5/§5b
reverts, the mutant-kill script, and the apply/verify/CI/mutation logs. **All removed**
(`ls $PDCA_SCRATCH | grep -c pdca-builder-635` → 0), and `git worktree list` is back to the
seven the driver owns.

Two things I did **not** remove, both outside my naming: `mutants.out/` inside the worktree
(the advisory gate's own artifact, excluded by `.gitignore:14` — verified it is **not** in
`patch.diff`), and the `cargo-mutants-*.tmp` build copies the gate makes under
`$PDCA_SCRATCH`, which it cleans up itself on exit (verified gone).

No PR was pushed, opened, or marked ready.
