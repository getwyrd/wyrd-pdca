# Build notes — #776 seg-record placement-move primitive (iteration 5)

Target: getwyrd/wyrd @ main, worktree base `243241e` (same base as iterations 3 and 4).
One file changed: `crates/core/src/metadata.rs`. `patch.diff` is **49 677 bytes** (budget
50 KB; iteration 4 was 49 324). Semantic non-test lines added: **106** counting only
statements/expressions (144 if lone braces count) — budget ≤ 170.

Line numbers marked "main" are on the base; unmarked ones are in the patched file. The
brief's numbers (`:380`, `:870`, `:1776`, …) come from an older main and have moved, so
every citation below carries the symbol too.

## What this iteration is

Iteration 4's patch passed every substantive check: T4 batch review 0 blocking, C5 mutants
36 tested / 27 caught / **0 missed** / 9 unviable, diff coverage 99.7%. Its one failure was
the gating C4-ci row: `cargo xtask ci` was killed at its 7200 s bound while
`crates/server/tests/custodian_gc.rs` sat at "has been running for over 60 seconds". The
iteration-4 carry-forward is that C4 item. So this iteration:

1. **keeps iteration 4's production code and tests** — the test module is byte-identical
   (checked with `diff` on the two patches' test hunks), and the production hunks differ
   only in two doc-comment blocks (below);
2. **corrects one inaccurate doc** the reviewer flagged (the `[human]` "`Refused` is not a
   race" finding) in the way that changes no behaviour;
3. **diagnoses the C4-ci timeout** with evidence that it is a pre-existing, intermittent
   stall in a test this patch cannot reach, and records an uninterrupted CI run of my own.

Nothing else moved. The five other `[human]` findings are left for sign-off (last section).

## The change, cited

New code sits between `resolve_current_chunk_map` (main `:3062`) and `mod tests` (main
`:3100`); patched `:3103-3364`.

| What | Where (patched) | Peer it mirrors (main) |
|---|---|---|
| `Repoint` outcome enum: `Prepared(WriteBatch)`, `Refused`, `VersionExhausted`, `Conflict` | `:3113-3150` | — |
| `MalformedReplacement` (call fault, deliberately not a `ChunkMapError`) | `:3161-3178` | `MalformedPlacement`'s `{expected, actual}` shape |
| `repoint_chunk` | `:3228-3324` | `commit_chunk_map` `:2139` |
| placement length check before any read | `:3237` | `ChunkRef::fragment_count` |
| pin 1: root generation bytes, both arms | `:3245` | `commit_chunk_map`'s `require(key, encode(prior))` |
| flat arm: `chunk_at` → checked `version + 1` → `..generation.clone()` (state untouched) | `:3247-3264` | `commit_chunk_map` `:2139-2189` (ADR-0047 metadata preserved) |
| segmented arm: candidates from the root's own table, no `seg:` walk | `:3269-3271` | `SegmentedMap::new` `:1090` |
| over-ceiling live row refused before decode | `:3283-3289` | `read_group_range` `:2827` |
| decode / bounds faults → `retired_or` (retired = `Conflict`, live = typed corruption) | `:3290-3321` | `retired_or` `:2767` |
| pin 2: the segment record's freshly read bytes | `:3312` | — |
| pin 3: `chunk == prior` equality inside `chunk_at` | `:3361` | — |
| ceiling: both arms through `flat_value_ceiling_crossed`, full `MAX_VALUE_BYTES` | `weighed` `:3327-3335` | `flat_value_ceiling_crossed` `:602`, `MAX_VALUE_BYTES` `:549` |
| `segment_may_hold` (offset in span, or an empty chunk on the end) | `:3342-3347` | — |
| `chunk_at` (offset **and** equality; `checked_add` on the running offset) | `:3355-3364` | — |
| doc of `flat_value_ceiling_crossed` updated: the segmented arm weighs here too | `:596-600` (main `:596-597`) | — |
| in-crate tests, `mod placement_move` | `:4437-5245` | module convention `mod tests` main `:3100` |

No `MAX_ROOT_VALUE_BYTES` (main `:574`) comparison appears in the diff. `commit_chunk_map`
and its segmented refusal are untouched. No custodian file, no new dependency
(`wyrd_testkit::Sim` and `wyrd_metadata_redb` are existing dev-dependencies of `wyrd-core`).

### The one edit since iteration 4 (doc only)

`Repoint::Refused` (`:3123-3136`) and `Repoint::VersionExhausted` (`:3137-3143`). The
reviewer showed by probe that `Refused` can be returned for a generation the root has
already left (segment row still there, root flipped to a successor) while its doc said
"not transient … an operator signal rather than a retry". `VersionExhausted` has the same
shape because the flat arm reads nothing from the store. The reviewer offered two fixes:
spend a root read before returning either, or make the doc say the caller must confirm
liveness before escalating. I took the doc fix, for a reason and not by coin toss:

- A root read before `Refused` cannot close the window, only narrow it — the root can flip
  right after the read, so a caller still has to confirm liveness before escalating. A check
  that looks like it closes the window but does not is worse than a doc that says whose job
  it is. The batch's root pin is what really decides liveness, and a refusal carries no batch.
- The flat arm today reads nothing after the plan; the seeded campaign's model depends on
  that (`campaign`, `:5073`: "the flat arm reads nothing past the plan"). Adding a read there
  changes a documented property for a race the reviewer measured as rare (a non-conforming
  row within ~19 bytes of V, plus a retirement in the window).
- Cost of the rejected option, concretely: ~12 lines — a liveness helper comparing
  `store.get(&root_key)` to `encode(generation)` (the flat arm has no `SegmentGroup`, so
  `retired_or` does not fit it), called at the two return sites, plus a test each — and a new
  `Conflict` path in the campaign's model for a refusal-turned-conflict.

The new text (`:3125-3129`, `:3139-3141`) states exactly what the code does: weighed on the
planned generation alone, no pin confirms the root still names it, not transient **while
the generation is live**, confirm with a fresh resolve before escalating. If sign-off
prefers the root read, the sketch above is the whole change.

## The C4-ci carry-forward: what the timeout was, with evidence

**Where the two hours went.** `iteration-v4/gate-logs/C4-ci.log` is 2255 lines. Every step
before the workspace tests completed (`typos`, docs lint + render, gitlink-guard,
unsafe-guard, `cargo fmt --check`, clippy, build: log `:11-583`), and `cargo test
--workspace --exclude wyrd-dst` ran through every crate's test binaries green up to line
2241 — including all 15 `placement_move` tests. Line 2243 starts
`crates/server/tests/custodian_gc.rs`; four of its ten tests passed (`:2245-2248`), then the
six still in flight printed "has been running for over 60 seconds" (`:2250-2255`) and the
log ends there. Nothing after that for the rest of the 7200 s.

**It is not test logic, and not this patch.**

- The six that stalled are simply the six in flight at that instant. Two of them have
  near-identical siblings that passed in the same run: `deployed_run_loop_refuses_duplicate_ids`
  passed while `…_refuses_duplicate_endpoints` stalled (same `custodian_args` +
  `cmd_custodian` shape, `custodian_gc.rs:1036-1068` / `:1071`), and
  `armed_deployed_role_reclaims_expired_pending_lease_garbage` passed while
  `deployed_role_reclaims_orphaned_bytes_after_grace_elapses` stalled (same
  `drive_deployed_loop_operator` fixture, `:455-481`). A whole-process stall, not a hang in
  one test's logic.
- The fixture is in-memory with a logical clock: `MemMeta`, `MemDServer`, `MemCoordination`,
  `move || now_millis`, a 60 ms sleep for shutdown and a 10 ms pass interval
  (`custodian_gc.rs:14-17`, `:462-480`). No Docker, no network. The duplicate-endpoints test
  refuses at `crates/server/src/cli.rs:1085`, before any redb open (`:1727`, `:1768`,
  `:2078`), so no file lock is shared either.
- None of these tests can reach this patch: `repoint_chunk` has **zero callers** on the tree
  (the brief: nothing calls it until #777), and the diff adds no statics, no clock, no
  global. The only production edits are additive items in `wyrd-core`.
- Reproduction attempt here, on the patched tree: `cargo test -p wyrd-server --test
  custodian_gc` passed 10/10 in 0.17 s, and the built binary run **20 times in a row under
  a 60 s timeout passed 20/20** (`custodian_gc-109efc4cc004658b`). Not reproducible in
  isolation.
- History in this bundle: iteration 3's builder saw the same stall once and it did not
  recur on re-run (iteration-4 notes); iteration 4's builder ran the full `xtask ci` to
  "all checks passed"; iteration 4's gate then stalled on it; the iteration-4 reviewer's
  own re-run got through the workspace tests and stopped later on an unrelated read-only
  advisory-cache lock in its sandbox.

**Conclusion:** an intermittent whole-process stall in a pre-existing `wyrd-server`
integration test, independent of this diff. I could not find its cause without leaving the
brief's one-file scope, so I did not try to fix it. Two things a human can do: re-run the
C4-ci row (it is a deterministic gate re-sampling a flaky substrate — exactly what
`confirm_gating_fail` exists for, but a timeout is recorded `unverifiable`, not `fail`, so
the confirm-once never fired), and file the stall against getwyrd/wyrd with the log lines
above so it gets a per-test timeout or a fix.

**My own uninterrupted run of the gate** — `./engine/xtask.sh ci` (= `cargo xtask ci`) with
`PDCA_WORKTREE` set to this worktree, under `timeout 7100`, on the final file:
see "Gates run" below.

## Named negations (applied one at a time, run, restored)

Harness: `$PDCA_SCRATCH/pdca-builder-776-neg5/run.sh`; log `negations.log` beside it. It
copies the known-good file, applies one whole-file substitution (fails loudly if nothing
changed), runs `cargo test -p wyrd-core --lib placement_move` under `timeout 900`, reports
a build that did not run, prints the failing tests, restores, and `cmp`s against the good
copy. Every run ended "restored"; the final line is "worktree file byte-identical to the
good copy", and `patch.diff` was diffed against `git diff` afterwards (identical).

| # | Negation (patched line) | Went red |
|---|---|---|
| N1 | `chunk == prior` → `(chunk == prior \|\| black_box(true))` (`:3361`) | 5, incl. **`a_sibling_edit_is_merged_and_an_edit_to_the_planned_chunk_conflicts`** (the same-chunk conflict leg) |
| N2 | root pin dropped: `WriteBatch::new().require(root_key, encode(generation))` → `WriteBatch::new()` (`:3245`) | 3, incl. **`a_superseded_root_fails_the_batch_or_the_move`** |
| N8 | ceiling comparison off: `flat_value_ceiling_crossed(&next).filter(\|_\| black_box(false))` (`:3328`) | 2: **both ceiling tests** (`flat_arm_refuses_a_record_…`, `segmented_arm_weighs_its_record_…`) |
| N7 | version advance unchecked: `wrapping_add(1).checked_add(0)` (`:3251`) | 1: **`flat_arm_refuses_a_version_it_cannot_advance`** |
| N4 | over-ceiling live-row guard off (`&& black_box(false)`, `:3283`) | 1: `a_segment_row_over_the_ceiling_is_never_decoded_nor_rewritten` |
| N3 | segment-bytes pin dropped (`:3312` → `root_pin`) | 2: `a_sibling_edit_…`, `seeded_moves_race_…` |

N1, N2, N8 and N7 are the brief's four named negations; each went red on the test the brief
names. (The `>` of the ceiling lives in the pre-existing `flat_value_ceiling_crossed`, so
N8 negates this diff's use of it, as in iteration 4.)

### cargo mutants

Not re-run this iteration, on purpose: the production code is byte-identical to iteration
4's except the two doc-comment blocks (shown by `diff` of the two patches' production hunks:
only `///` lines differ), and cargo-mutants generates no mutants from comments. Iteration
4's C5-mutants gate on that code: **36 mutants, 27 caught, 0 missed, 9 unviable** (76 s).
The gate re-runs at Check on this diff. Iteration 4's notes also list the 9 unviable mutants
hand-applied in lint-clean form (all red) and 7 call-site argument swaps (all red); none of
that code moved.

## Refute-your-own-test

- **(a) Genuine red?** Yes, per negation rather than per revert — the defect is an absent
  API, so reverting the patch removes the symbols and the tests stop compiling (the brief's
  declared posture: green-only under C4-verify; `cargo mutants` + named negations are the
  oracle). Six negations on the final file, each red on the expected test (table above);
  iteration 4's 0-missed mutants run on the same production code.
- **(b) Production path?** Yes. Every test calls the production `repoint_chunk`,
  `segment_may_hold` and `chunk_at`; batches are applied through `MetadataStore::commit` on
  the real redb backend (`RedbMetadataStore::in_memory()`, `:4443`); the seeded campaign
  plans through the production resolver `resolve_current_chunk_map` (`:5125`). No stand-in.
- **(c) Fixture includes the fault?** Yes: a `Pending` generation (`:4571`), a version at
  `u64::MAX` (`:4594`), a garbage neighbour segment while the root still names the
  generation with the move on its edge (`:4632-4640`), a sibling edit before the read and
  an edit to the planned chunk (`:4660-4699`), a root flipped after the plan and a segment
  reclaimed (`:4701-4726`), undecodable / shifted / resized / absent segments
  (`:4728-4782`), empty chunks on a boundary (`:4784-4845`), records one byte under and
  exactly on the ceiling and a live V+1 row (`:4847-4928`), wrong-length replacements in both
  arms (`:4930-4961`), an overflowing offset sum (`:5003-5005`), and 256 seeded campaigns of
  racing movers, root overwrites, root deletes and reclaimed segments with a non-vacuity
  assertion on every race class (`:5236-5245`). "Nothing written" is always a full-store
  `scan(b"")` compared before and after.

## Rubric self-review (AGENTS.md § Review rubric & protocol)

- One clock per lifecycle: no clock read in the diff.
- Narrow seams / dependency direction: uses only `&dyn MetadataStore` (`get`, `commit`),
  as `commit_chunk_map` does; nothing new crosses a crate boundary.
- Validation boundaries: the segment decode's structural faults surface as typed
  `ChunkMapError`s via the resolver's own arbiter; a bad argument is its own error type
  (`MalformedReplacement`) so a maintenance loop cannot file a planner bug as per-object
  corruption (iteration 2's finding).
- No DST-reachable shared mutable global state: none added (the statics gate runs in CI).
- Crate root `forbid(unsafe_code)`: no new crate.
- Docs currency: a library function, an enum and an error struct — no port, API operation,
  RPC, CLI flag or persisted field. The C1 `[human]` item on this is left to sign-off.
- Protocol input / oversize: a live `seg:` row over `MAX_VALUE_BYTES` is refused before
  decode and never rewritten (`:3283`, test `:4898`).
- Serialization identity: the flat arm re-encodes through `..generation.clone()`, so absent
  optional fields stay absent; the segmented arm pins the row's exact stored bytes.
- Absent entries: `SegmentAbsent` through `retired_or` — an error on a live generation,
  never a silent skip (`:4771-4782`).
- Transactions: one CAS batch, handed back uncommitted; nothing to roll back.
- Await discipline: the only awaits are the store's own `get`s, the same calls the resolver
  makes; no spawned task, no stream.
- Test fidelity: in-crate seeded Tier-0 campaign; the madsim `crates/dst` question is the
  `[human]` scope item.

## Gates run

- `cargo fmt -p wyrd-core`, then `cargo fmt --all -- --check`: clean.
- `typos crates/core/src/metadata.rs`: clean.
- `cargo test -p wyrd-core --lib placement_move` under `timeout 1500`: 15/15 pass (1.10 s).
- `cargo test -p wyrd-server --test custodian_gc` under `timeout 480`: 10/10 pass; the binary
  20/20 under `timeout 60` each.
- `./engine/xtask.sh ci` under `timeout 7100`, log
  `$PDCA_SCRATCH/pdca-builder-776-ci-it5.log`: **exit 0, "xtask ci: all checks passed"**,
  started 14:42:27, ended 14:43:57 (90 s wall-clock — the worktree's `target/` was warm from
  iteration 4's gate and my scoped runs, so every step was a no-op build plus the tests).
  Every step ran: typos, docs lint + render (link audit OK), gitlink-guard, unsafe-guard,
  `cargo fmt --all -- --check`, workspace clippy / build / test (188 `test result: ok`
  lines, 0 `FAILED`; the 15 `placement_move` tests among them; `custodian_gc.rs` — the
  binary that stalled in iteration 4's gate — 10/10 in 0.17 s at log `:1696`),
  cargo-machete, the three cargo-deny checks, conformance (5 valid + 6 invalid vectors),
  statics, deploy-guard, and the madsim `wyrd-dst` clippy + test leg (log `:2601-3186`).
  Nothing in the machine was competing: load average 0.45 at start, no other cargo process.
- `patch.diff` is byte-identical to `git diff` in the worktree (`cmp`).
- External dependencies the brief lists: `typos`, the docs renderer, `cargo-deny`,
  `cargo-machete` run inside `xtask ci`; `cargo-mutants` was exercised on this production
  code in iteration 4 and runs again at Check. No NEEDS-HUMAN external dependency.

## Left for the human (the deferred `[human]` findings)

Not changed, with what I would say to each:

- **Seeded Tier-0 DST coverage** (T4, raised 3×): the patch carries an in-crate seeded
  campaign (`seeded_moves_race_each_other_and_the_roots_retirement`, `:5236`). A madsim
  `crates/dst` scenario needs a caller, which #777 brings; I would record the T4 row as
  rejected-with-reference to #777.
- **Flat-arm `Conflict` for a miss no retry can fix**: brief item 7 allows conflict/`Blocked`.
  If #777 wants a distinct non-transient outcome, `MalformedReplacement` shows the shape.
- **`Refused` / `VersionExhausted` on a retired generation**: addressed by the doc fix above;
  the root-read option is sketched there if preferred.
- **T5 prior-art corroboration** and **C1 docs currency vs the one-file scope**: Plan's
  calls; nothing in the patch changes them.
- **C4-ci timeout**: diagnosed above; my own uninterrupted run is in "Gates run".

Scratch used, all under `$PDCA_SCRATCH` (`/var/tmp/pdca/wyrd-pdca-9c587031/issue_776`):
`pdca-builder-776-neg5/` (script, log, two copies of the one file) and
`pdca-builder-776-ci-it5.log`. Builds went into the worktree's own `target/`. Left for the
harness to reclaim, per the no-`rm` rule for builders.
