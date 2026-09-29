# Build notes — #776 seg-record placement-move primitive (iteration 6)

Target: getwyrd/wyrd @ main, worktree base `243241e` (same base as iterations 3–5; the
worktree was clean at that commit when this attempt started, `git status` empty).
One file changed: `crates/core/src/metadata.rs`. `patch.diff` is **49 947 bytes** (budget
50 KB under either reading; iteration 5 was 49 677). Semantic non-test lines added:
**106** counting statements/expressions (144 if lone braces count) — unchanged from
iteration 5, because the only edit this round is four doc-comment lines. Budget ≤ 170.

Line numbers marked "main" are on the base; unmarked ones are in the patched file. The
brief's numbers (`:380`, `:870`, `:1776`, …) come from an older main and have moved, so every
citation below carries the symbol too. Everything below `repoint_chunk`'s doc is **+4** from
iteration 5's numbers.

## What this iteration is

Iteration 5 passed every functional check: C4-ci **pass** (uninterrupted, `gate-logs/C4-ci.log:3733`),
C4-verify green-only as the brief declares, diff coverage 99.7 %, C5 mutants **27 caught /
0 missed / 9 unviable**, adversary "could not refute", code-review "no additional findings".
Its one gating red was the T4 batch review's single finding — **CONVENTION, Docs currency**:
"the new public `repoint_chunk` operation lacks the same-PR living architecture update"
(`iteration-v5/check-review.md:13`, `review-batch.md:3`). Its own reviewer said the
outstanding item "requires a scope decision, not another code rebuild against the same
brief" (`check-review.md:19`), and the adversary put the same choice to the human
(`check-advisory-adversary.md:7`): (a) reject the finding and record the reason, or (b)
widen the child to two files.

The brief forbids (b): "**Scope:** one file … A second file means the shape is wrong — STOP
and hand back" (`brief.md:83-95`). It does not forbid (a), and the target's own rubric
supplies exactly the mechanism: "**Deferrals are settled**: a finding answered with
'Deferred — tracked in #N' (or an in-code `// deferred: #N` marker) is resolved for review
purposes; do not re-raise it in later rounds" (`AGENTS.md`, § Reviewer protocol). So this
iteration:

1. **keeps iteration 5's production code and tests byte-identical** — `diff` of the two
   patches' `+`/`-` lines shows exactly four added lines, all `///` (below);
2. **answers the finding the rubric's way, inside the one file**: a `deferred: #777` marker
   in `repoint_chunk`'s doc (`:3229-3231`), in the tree's own form (e.g.
   `crates/custodian/src/backfill.rs:112`, which already defers "the segmented write path
   (`repoint_chunk`, the record ceilings)" to this lineage's grandparent #682);
3. **records the rejection** for the T4 gate's triage file, `review-rejected.md` in this
   bundle (`<file:line> | CONVENTION | architecture | reason`, the format
   `scripts/review-branch:318-358` reads; it re-anchors to the nearest matching line, so a
   later line shift does not re-open it).

Nothing else moved. The nine `[human]` findings deferred through round 5 stay for sign-off
(last section) — including the **C1 NEEDS-HUMAN on this very point**, which is still in
`deferred-findings.json` and so still reaches §6. Recording the rejection does not hide the
decision from the human; it stops a deterministic gate from failing on a finding the brief
forbids fixing, so the bundle reaches sign-off instead of burning the last unattended round
(`auto-iterate.json`: `count: 5`, the ceiling — this is attempt 6 in `loop-telemetry.json`).

### Why the rejection is honest, not convenient

- **The rubric's trigger is not met.** Docs currency fires for "a port, an API operation, an
  RPC, a CLI flag, or a persisted field" (`AGENTS.md:154-156`). `repoint_chunk` is an
  in-crate library function with zero callers, no wire change, no trait-seam change, no
  persisted field (it writes records that already exist in the shapes they already have).
- **The living doc does not track this level.** Its peers `commit_chunk_map` and
  `resolve_chunk_map` appear nowhere under `docs/design/architecture/` (`grep` over the
  directory: the only `chunk_map` hit is unrelated staging text at `06-runtime-view.md:82`).
- **What the doc says stays true until #777.** `06-runtime-view.md:40` (§6.3 Repair, item 2:
  "update the chunk's location via a single atomic metadata mutation") is precisely the
  batch shape this primitive hands back. `08-crosscutting-concepts.md:85` (§8.7) describes
  what "the maintenance loops that reclaim or move them" do — and until #777 wires this in,
  they do exactly that. The one sentence that goes stale *in spirit* ("an object whose root
  can no longer be re-written is an object whose placement can never be repaired") goes
  stale when a loop can actually repair a segmented placement — #777's change, not this one.
- **The brief is Plan's authority and it says one file.** Plan calibrated the budget, scope
  and shape; a builder widening the scope on a reviewer's reading of a convention is the
  wrong beat for that decision.

### The rejected alternative, costed

Updating the doc is **cheap in bytes** — I rejected it on the brief's scope rule, not on
cost. If sign-off overrules, the whole change is about four lines in two doc files:

- `docs/design/architecture/08-crosscutting-concepts.md:85` (§8.7), the sentence "and an
  object whose root can no longer be re-written is an object whose placement can never be
  repaired" → "… can never be **superseded or retired** — its *placement* is moved in the
  `seg:` record that holds the chunk (`repoint_chunk`), pinned to the root's bytes but never
  re-writing them, so the reserve protects the root's own re-writes".
- `docs/design/architecture/06-runtime-view.md:40` (§6.3 item 2), after "a single atomic
  metadata mutation": "(for a segmented map, the one `seg:` record holding the chunk, with the
  root's bytes as a pin)".

That is the scope change the brief says to hand back rather than make. It is also the
natural content of #777's PR, where the behaviour those sentences describe changes.

### Why not STOP with no patch

"STOP and hand back" is for the case where the *code* needs a second file — the shape is
wrong. The success criterion is met in one file (every functional gate green for two
rounds); the blocking item is a scope/convention conflict that is already a §6 human item.
Handing back an empty bundle would lose the reviewer-verified patch and give the human
less to decide with, not more.

## The change, cited

New code sits between `resolve_current_chunk_map` (main `:3062`) and `mod tests` (main
`:3100`); patched `:3102-3372`.

| What | Where (patched) | Peer it mirrors (main) |
|---|---|---|
| `Repoint` outcome enum: `Prepared(WriteBatch)`, `Refused`, `VersionExhausted`, `Conflict` | `:3113-3150` | — |
| `MalformedReplacement` (call fault, deliberately not a `ChunkMapError`) | `:3161-3175` | `MalformedPlacement`'s `{expected, actual}` shape |
| `repoint_chunk` doc, incl. the `deferred: #777` marker (**this round's only edit**) | `:3177-3231`, marker `:3229-3231` | `backfill.rs:112`'s marker form |
| `repoint_chunk` | `:3232-3328` | `commit_chunk_map` `:2142` |
| placement length check before any read | `:3240-3247` | `ChunkRef::fragment_count` |
| pin 1: root generation bytes, both arms | `:3249` | `commit_chunk_map`'s `require(key, encode(prior))` |
| flat arm: `chunk_at` → checked `version + 1` → `..generation.clone()` (state untouched) | `:3251-3268` | `commit_chunk_map` `:2142-2189` (ADR-0047 metadata preserved) |
| segmented arm: candidates from the root's own table, no `seg:` walk | `:3272-3276` | `SegmentedMap::new` `:1090` |
| over-ceiling live row refused before decode | `:3287-3293` | `read_group_range` `:2830` |
| decode / bounds faults → `retired_or` (retired = `Conflict`, live = typed corruption) | `:3294-3325` | `retired_or` `:2770` |
| pin 2: the segment record's freshly read bytes | `:3316` | — |
| pin 3: `chunk == prior` equality inside `chunk_at` | `:3365` | — |
| ceiling: both arms through `flat_value_ceiling_crossed`, full `MAX_VALUE_BYTES` | `weighed` `:3331-3339` | `flat_value_ceiling_crossed` `:605`, `MAX_VALUE_BYTES` main `:549` |
| `segment_may_hold` (offset in span, or an empty chunk on the end) | `:3346-3351` | — |
| `chunk_at` (offset **and** equality; `checked_add` on the running offset) | `:3359-3371` | — |
| doc of `flat_value_ceiling_crossed` updated: the segmented arm weighs here too | `:596-600` (main `:596-597`) | — |
| in-crate tests, `mod placement_move` | `:4441-5249` | module convention `mod tests` main `:3100` |

No `MAX_ROOT_VALUE_BYTES` (main `:574`) comparison appears in the diff. `commit_chunk_map`
and its segmented refusal are untouched. No custodian file, no new dependency
(`wyrd_testkit::Sim` and `wyrd_metadata_redb` are existing dev-dependencies of `wyrd-core`).
No conformance vector, no proposal text, no clock read, no static.

### The only diff against iteration 5

```
$ diff <(grep '^[+-]' iteration-v5/patch.diff) <(grep '^[+-]' patch.diff)
135a136,139
> +///
> +/// deferred: #777 — the living architecture doc (`06-runtime-view.md` §6.3,
> +/// `08-crosscutting-concepts.md` §8.7) describes what the maintenance loops **do**, and
> +/// nothing calls this yet. It moves with the custodian wiring in #777, which changes that.
```

## Named negations (applied one at a time, run, restored)

Harness: `$PDCA_SCRATCH/pdca-builder-776-neg6/run.sh` (iteration 5's, re-pointed), log
`negations.log` beside it. It copies the final file as the known-good copy, applies one
whole-file substitution (fails loudly if nothing changed), runs
`cargo test -p wyrd-core --lib placement_move` under `timeout 900`, reports a build that did
not run, prints the failing tests, restores, and `cmp`s. Every run ended "restored"; the final
line is "worktree file byte-identical to the good copy", and `patch.diff` reverse-applies to
the worktree afterwards (`git apply --check --reverse`: it is the worktree diff).

| # | Negation (patched line) | Went red |
|---|---|---|
| N1 | `chunk == prior` → `(chunk == prior \|\| black_box(true))` (`:3365`) | 5, incl. **`a_sibling_edit_is_merged_and_an_edit_to_the_planned_chunk_conflicts`** (the same-chunk conflict leg), `a_zero_length_chunk_on_a_segment_boundary_is_found_by_equality`, `chunk_at_needs_both_the_offset_and_the_reference` |
| N2 | root pin dropped: `WriteBatch::new().require(root_key, encode(generation))` → `WriteBatch::new()` (`:3249`) | 3, incl. **`a_superseded_root_fails_the_batch_or_the_move`**, `flat_arm_conflicts_on_a_changed_chunk_and_on_a_superseded_root`, the seeded campaign |
| N8 | ceiling comparison off: `flat_value_ceiling_crossed(&next).filter(\|_\| black_box(false))` (`:3332`) | 2: **both ceiling tests** (`flat_arm_refuses_a_record_…`, `segmented_arm_weighs_its_record_…`) |
| N7 | version advance unchecked: `wrapping_add(1).checked_add(0)` (`:3255`) | 1: **`flat_arm_refuses_a_version_it_cannot_advance`** |
| N4 | over-ceiling live-row guard off (`&& black_box(false)`, `:3287`) | 1: `a_segment_row_over_the_ceiling_is_never_decoded_nor_rewritten` |
| N3 | segment-bytes pin dropped (`:3316` → `root_pin`) | 2: `a_sibling_edit_…`, `seeded_moves_race_…` |

N1, N2, N8 and N7 are the brief's four named negations; each went red on the test the brief
names. (The `>` of the ceiling lives in the pre-existing `flat_value_ceiling_crossed`, so N8
negates this diff's use of it.)

### cargo mutants

Re-run this round through the gate's own script, `scripts/mutants-in-diff` (`cargo mutants
--in-diff patch.diff --no-shuffle`, `PDCA_BUNDLE` = this bundle, `PDCA_WORKTREE` = this
worktree), log `$PDCA_SCRATCH/pdca-builder-776-mutants-it6.log`:
**36 mutants tested in 76 s: 27 caught, 9 unviable, 0 missed** — identical to the
iteration-4 and iteration-5 gate results, as expected: the production code is byte-identical
and cargo-mutants generates no mutants from doc comments. Iteration 4's notes list the 9
unviable mutants hand-applied in lint-clean form (all red) and 7 call-site argument swaps
(all red); the adversary in round 5 re-ran the argument-level class independently
(`iteration-v5/check-advisory-adversary.md:17-21`); none of that code moved.

## Refute-your-own-test

- **(a) Genuine red?** Yes, per negation rather than per revert — the defect is an absent
  API, so reverting the patch removes the symbols and the tests stop compiling (the brief's
  declared posture: green-only under C4-verify; `cargo mutants` + named negations are the
  oracle). Six negations on the final file, each red on the expected test (table above).
- **(b) Production path?** Yes. Every test calls the production `repoint_chunk`,
  `segment_may_hold` and `chunk_at`; batches are applied through `MetadataStore::commit` on
  the real redb backend (`RedbMetadataStore::in_memory()`, `:4448`); the seeded campaign
  plans through the production resolver `resolve_current_chunk_map` (inside `campaign`,
  `:5077`). No stand-in.
- **(c) Fixture includes the fault?** Yes: a `Pending` generation (`:4571-4594`), a version
  at `u64::MAX` (`:4596`), a garbage neighbour segment while the root still names the
  generation with the move on its edge (`:4632`), a sibling edit before the read and an edit
  to the planned chunk (`:4664`), a root flipped after the plan and a segment reclaimed
  (`:4705`), undecodable / shifted / resized / absent segments (`:4732`), empty chunks on a
  boundary (`:4788`), records one byte under and exactly on the ceiling and a live V+1 row
  (`:4851-4932`), wrong-length replacements in both arms (`:4934`), an overflowing offset sum
  (`:4992`), and 256 seeded campaigns of racing movers, root overwrites, root deletes and
  reclaimed segments with a non-vacuity assertion on every race class (`:5240`). "Nothing
  written" is always a full-store `scan(b"")` compared before and after (`snapshot`, `:4471`).

## Rubric self-review (AGENTS.md § Review rubric & protocol)

- One clock per lifecycle: no clock read in the diff.
- Narrow seams / dependency direction: uses only `&dyn MetadataStore` (`get`, `commit`),
  as `commit_chunk_map` does; nothing new crosses a crate boundary.
- Validation boundaries: the segment decode's structural faults surface as typed
  `ChunkMapError`s via the resolver's own arbiter; a bad argument is its own error type
  (`MalformedReplacement`) so a maintenance loop cannot file a planner bug as per-object
  corruption.
- No DST-reachable shared mutable global state: none added (the statics gate runs in CI).
- Crate root `forbid(unsafe_code)`: no new crate.
- **Docs currency: answered per the protocol's own deferral rule** — in-code
  `deferred: #777` marker (`:3229-3231`) + recorded rejection (`review-rejected.md`); reasons
  above. The C1 `[human]` item stays in §6 for the human to confirm or overturn.
- Protocol input / oversize: a live `seg:` row over `MAX_VALUE_BYTES` is refused before
  decode and never rewritten (`:3287`, test `:4902`).
- Serialization identity: the flat arm re-encodes through `..generation.clone()`, so absent
  optional fields stay absent; the segmented arm pins the row's exact stored bytes.
- Absent entries: `SegmentAbsent` through `retired_or` — an error on a live generation,
  never a silent skip (`:3281`, test `:4732`).
- Transactions: one CAS batch, handed back uncommitted; nothing to roll back.
- Await discipline: the only awaits are the store's own `get`s, the same calls the resolver
  makes; no spawned task, no stream.
- Test fidelity: in-crate seeded Tier-0 campaign; the madsim `crates/dst` question is the
  `[human]` scope item.
- Reviewer protocol, deferrals: the marker names the tracking issue (#777) in the exact form
  the protocol and the tree use.

## Gates run

- `cargo fmt --all -- --check`: clean (rustfmt does not reflow doc comments; no
  `rustfmt.toml` in the tree).
- `typos crates/core/src/metadata.rs`: clean.
- `cargo test -p wyrd-core --lib placement_move` under `timeout 550`: **15/15 pass** (1.13 s).
- `./engine/xtask.sh ci` (= `cargo xtask ci`, the project's runner, `PDCA_WORKTREE` set to
  this worktree) under `timeout 590`, log `$PDCA_SCRATCH/pdca-builder-776-ci-it6.log`:
  **exit 0, "xtask ci: all checks passed"**, started 15:17:14, ended 15:18:26 (72 s
  wall-clock; the worktree's `target/` was warm from the round-5 gate). Every step ran:
  typos, docs lint + render, gitlink-guard, unsafe-guard, `cargo fmt --all -- --check`,
  workspace clippy / build / test (**188** `test result: ok` lines, **0** `FAILED`; the 15
  `placement_move` tests at log `:323-375`; `custodian_gc.rs` — the binary that stalled in
  iteration 4's gate — at log `:1695`, all 10 passed), cargo-machete, the three cargo-deny
  checks, conformance (5 valid + 6 invalid vectors, `:2592`), statics (`:2595`),
  deploy-guard (`:2598`), and the madsim `wyrd-dst` leg.
- `patch.diff` reverse-applies cleanly to the worktree (`git apply --check --reverse`).
- External dependencies the brief lists: `typos`, the docs renderer, `cargo-deny`,
  `cargo-machete` run inside `xtask ci`; `cargo-mutants` as above. No NEEDS-HUMAN external
  dependency.

## Left for the human (the deferred `[human]` findings, unchanged)

- **C1 / T4 Docs currency vs one-file scope** — settled here by deferral to #777 with the
  reason recorded (`review-rejected.md`). To overturn: delete that line, and either widen
  this child to two files (sketch above, ~4 lines) or add the doc update to #777's brief.
- **Seeded Tier-0 DST coverage** (T4, raised 3×): the patch carries an in-crate seeded
  campaign (`:5240`). A madsim `crates/dst` scenario needs a caller, which #777 brings.
- **Flat-arm `Conflict` for a miss no retry can fix**: brief item 7 allows conflict/`Blocked`.
  If #777 wants a distinct non-transient outcome, `MalformedReplacement` shows the shape.
- **`Refused` / `VersionExhausted` on a retired generation**: doc fix from iteration 5
  (`:3125-3129`, `:3138-3140`); the root-read option is ~12 lines if preferred.
- **T5 prior-art corroboration**: Plan's call; nothing in the patch changes it.
- **C4-ci timeout (iteration 4)**: iteration 5's gate ran uninterrupted; see this round's run
  above.

Scratch used, all under `$PDCA_SCRATCH` (`/var/tmp/pdca/wyrd-pdca-9c587031/issue_776`):
`pdca-builder-776-neg6/` (script, log, the good copy of the one file) and
`pdca-builder-776-ci-it6.log`. Builds went into the worktree's own `target/`. Left for the
harness to reclaim, per the no-`rm` rule for builders.
