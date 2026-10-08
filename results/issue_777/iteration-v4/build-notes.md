# Build notes — #777, iteration 4: reconstruction completes the `seg:` repair through #776's primitive

Target: getwyrd/wyrd @ main. Worktree HEAD `4bda59c` (the integration branch, which already holds
#776, so `metadata::repoint_chunk` is on the base at `crates/core/src/metadata.rs:3241`). Line
numbers are **post-patch** unless marked `base:`.

This iteration starts from `iteration-v3/patch.diff` (it applied cleanly). The v3 design passed
the T4 batch review (0 blocking) and every gate; v4 changes only what the round-3 carry-forward
asked for, plus one comment the same finding made stale. Everything in v3's notes about the
design, the fourth file, the #682 DST deferral and the #776 docs marker still holds and is
summarised at the end.

## Carry-forward items and what I did

| finding | fix |
|---|---|
| **The abort offset on a contained move was untested.** Deleting `emit_aborted(plan.chunk_id);` at the `Contained` arm survived the whole `wyrd-custodian` suite, so a contained move could count as a successful repair (`repaired − conflict − aborted − ceiling_refused` up by one). | `torn_under_the_move` now asserts, from the captured audit/metric rows, `(repaired ticks, aborted ticks, abort rows with reason unresolvable-chunk-map) == (n, n, n)` where `n = owed.len()` (`crates/custodian/tests/segmented_map_repoint.rs:721-732`). It runs in leg 6 (`n = 1`) and leg 7 (`n = 2`, two obligations in one torn record). Checking `repaired` as well as `aborted` shows the success identity nets to zero, not just that some abort fired. |
| **Nit: the abort row still said "could not place the rebuilt shard(s)"** — the wrong reason for a contained move. | `emit_aborted` takes a `reason: &'static str` and prints it as a structured field, the same shape `emit_staged(chunk, reason)` already uses (`reconstruction.rs:1387-1396`). The message is now cause-neutral: "reconstruction aborted a dispatched repair; nothing was committed and the obligation stays queued" (`reconstruction.rs:1411-1429`). Call sites: `RepairOutcome::Aborted => emit_aborted(plan.chunk_id, "unplaced")` (`:443`, the base's fleet-view / staged abort, same meaning as the old message) and `emit_aborted(plan.chunk_id, "unresolvable-chunk-map")` in the `Contained` arm (`:450`), which reuses the action string of the containment row that names the object, so one grep joins the two rows. |
| (same finding, follow-on) The base comment at `reconstruction.rs:386-393` defined `reconstruction_aborted` as only "the selector chose a server outside the fleet view". After this patch a contained move also lands there, so the comment would be incomplete — the "comments the patch made false" class round 1 raised. | One-line edit at `:389`: "(the selector chose a server outside the fleet view, or the move found its object unusable)". |

### Why a `reason` field and not just a new message string

Rejected: keep `emit_aborted(chunk)` and only make the message cause-neutral. That leaves the
`:443` call site untouched (no diff-coverage line), but the fleet-view abort's row would lose
the only text saying *why* it aborted — on the base that message was the cause. A structured
`reason` keeps the cause for both paths, and it is the pattern this file already uses for
`kept-staged`. Cost of the chosen form over the rejected one: +1 line (`reason,`) and one more
changed call site (`:443`).

## Budget

| file | + | − |
|---|---|---|
| `crates/custodian/src/reconstruction.rs` | 209 | 218 |
| `crates/custodian/src/reconstruction/staged.rs` | 0 | 1 |
| `crates/custodian/tests/segmented_map_reconstruction.rs` (forced edit) | 50 | 51 |
| `crates/custodian/tests/segmented_map_repoint.rs` (new) | 865 | 0 |

- Added non-blank, non-comment production lines: **85** (budget ≤ 100; v3 was 81. +4: the
  `reason` parameter line, `reason,`, the new message, the `:443` call site).
- `patch.diff`: **84,990 bytes** (budget ≤ 85 KB; under 85,000 even read as decimal KB). v3 was
  84,430. The new assertion and production change cost about 1,500 bytes, so I shortened
  doc comments in the new test file to pay for them: the module doc's leg list and race
  paragraph, and the docs on `MemMeta::faulting`, `Race::IntoCommit`, `Capture`, `seed`, `owe`,
  `run`, `assert_lost`, and legs 1, 2, 3, 5, 8, 10, 11 and `flat_contained`. No assertion, code
  line, or cited reference was removed; each leg's named negation is still stated.

## Test: `crates/custodian/tests/segmented_map_repoint.rs` (new, no `#![cfg]`)

Unchanged from v3 except the assertion above and the comment trims. It drives only
`reconcile_step` and asserts on the store plus the audit seam. It names only base symbols
(`ReconcileError`, `MAX_VALUE_BYTES`, `orphan_key`, …) and string literals; the new assertion
adds only string literals, so the file still compiles with production reverted. 11 legs (the
brief's 5 plus 6 added in v2/v3 at reviewers' request), so C4-verify reports **11 ran, 10
failed** on the red leg, not "5 ran, 4 failed".

## Red → green (final tree)

`engine/scripts/run-verify.sh` creates `git worktree add -B …` under `../wyrd-verify*` in the
host repo, outside the roots I may write. So, as in v2/v3, I ran its red leg by hand in the
cycle worktree with the same `cargo test --quiet -p wyrd-custodian --test segmented_map_repoint
--test segmented_map_reconstruction` the gate runs (plus `--no-fail-fast`, so the second binary
runs after the first fails), bounded by `timeout 900`. Production files copied to
`$PDCA_SCRATCH/pdca-builder-777-v4`, reset with `git checkout HEAD --`, tested, copied back;
`git diff HEAD` before/after compared with `cmp` (identical).

- **Green (fix applied):** `segmented_map_repoint` **11 passed**; `segmented_map_reconstruction`
  6 passed.
- **Red (`reconstruction.rs` + `staged.rs` at `HEAD`, tests kept):** `segmented_map_repoint`
  **11 ran, 10 failed**:
  - leg 1 `:478` `left: Blocked, right: Changed`
  - leg 2 `:537` `left: Blocked, right: Changed`
  - leg 3 `:458` `left: Blocked, right: Satisfied`
  - leg 4 `:442` "fixture: the race never landed" (base never reaches `commit`)
  - legs 6, 7 `:716` `left: (0, 0, true), right: (1, 1, true)` (they fail on the containment
    assertion before reaching the new one)
  - leg 8 `:764` `left: [41472], right: [3584, 41472]`
  - leg 9 panicked at base `reconstruction.rs:1157:18` (`version + 1` overflow)
  - leg 10 `:797` `left: Changed, right: Blocked`
  - leg 11 `:846` "fixture: the fault never fired"
  - leg 5: ok (green by construction, as the brief expects)

  `segmented_map_reconstruction`: the rewritten leg fails at `:500` (`left: Blocked, right:
  Changed`); the other 5 pass.

Legs 3 and 4 going red only shows "refused" versus "attempted"; their pins are bound by the
mutation table below, not by that red.

## Named negations (mutation oracle)

New this round, run on the final tree with `$PDCA_SCRATCH/pdca-builder-777-v4/final.py`, which
restores the file in a `finally`; `git diff HEAD` before/after compared with `cmp` (identical):

| # | mutation | result |
|---|---|---|
| M12 | the carry-forward's mutant: delete `emit_aborted(plan.chunk_id, "unresolvable-chunk-map");` (`:450`) | **legs 6 and 7 only** fail, at `:723`: `left: (1, 0, 0), right: (1, 1, 1)` and `left: (2, 0, 0), right: (2, 2, 2)`. The other 9 legs and all 6 `segmented_map_reconstruction` legs pass. |
| M13 | the nit: contained arm emits `"unplaced"` instead of `"unresolvable-chunk-map"` | **legs 6 and 7 only** fail, at `:723`: `left: (1, 1, 0)` / `left: (2, 2, 0)`. |

v3's negations M1–M11 (v3 notes, "Named negations") were not re-run this round: the code they
mutate is unchanged, and the only test edits are one added assertion and comment text.

### The `"unplaced"` call site and diff coverage

`:443` (`RepairOutcome::Aborted => emit_aborted(plan.chunk_id, "unplaced")`) is now a changed
line, and neither test binary the diff-coverage gate runs reaches it: `attempt` builds the fleet
and the topology from the same `free` server, so no leg can make the selector choose a server
outside the fleet. Expect it as **one MISS** in C4-diff-cov. It *is* covered by the crate's
existing suite: replacing that arm with `panic!("probe: unplaced arm reached")` makes
`crates/custodian/tests/reconstruction.rs:1878`
(`an_aborted_repair_is_not_counted_as_a_successful_repair`) fail with
`panicked at crates/custodian/src/reconstruction.rs:443:39: probe: unplaced arm reached`
(tree restored and `cmp`-checked after). That test asserts the abort counter, not the new
`reason` value, so the literal `"unplaced"` itself is bound by no test.

Not added: a leg in the new file that reaches it. It needs `attempt` to take a topology server
that is not in the fleet, which means a new parameter at all 10 `run`/`attempt` call sites or a
second ~25-line copy of `attempt`; roughly 700-1,000 bytes of diff, and the patch has 10 bytes
of headroom under 85,000. Asserting `"unplaced"` in `tests/reconstruction.rs` would be a fifth
file. Left for the human: accept the one MISS, or raise the byte/file budget.

## Refute-your-own-test

- **(a) Genuine red? Yes.** With `reconstruction.rs` and `staged.rs` at `HEAD` and the tests
  kept: 11 ran, 10 failed, each for the reason listed above. The new assertion is also bound on
  its own: the carry-forward's exact mutant (M12) and the wrong-reason mutant (M13) each turn
  legs 6 and 7 red at the new assertion (`:723`) with everything else green.
- **(b) Production path? Yes.** Every leg calls the public `reconcile_step` with a real
  `ReconstructionContext`; the pass runs the production `read_committed` → `assess` →
  `repair_chunk` → `metadata::repoint_chunk` → `MetadataStore::commit`. The new assertion reads
  the rows the production `emit_repaired` / `emit_aborted` actually emitted, captured through a
  real `tracing_subscriber` JSON layer (the same capture the existing `contained` helper uses).
  The only doubles are the in-memory `MetadataStore` / `ChunkStore` the brief names.
- **(c) Fixture includes the fault? Yes.** Legs 6/7 tear segment 1 with real undecodable bytes
  after the resolver's page and assert the race landed (`meta.raced()`), so the move really
  meets the torn record and really takes the `Contained` arm; leg 7 owes two chunks in that one
  record, so the abort count is checked at `n = 2`, where once-per-object containment and
  once-per-plan aborts give different numbers (1 row, 2 aborts).

## Gates run locally

- `cargo fmt --all -- --check`: clean.
- `cargo clippy -p wyrd-custodian --all-targets -- -D warnings`: clean.
- `typos` over the four changed files: clean.
- `./engine/xtask.sh ci` (the project's runner, `cargo xtask ci`, `PDCA_WORKTREE` set to the
  cycle worktree, bounded by `timeout 5400`) on the **final tree**: `xtask ci: all checks
  passed`, rc=0. The log shows each external tool the brief names actually running, none
  skipped: `typos`, `render_site.py --check` ("link audit OK"), `cargo-machete`, `cargo deny`
  (three invocations), plus fmt, clippy, build, the workspace tests (including
  `segmented_map_repoint`), conformance vectors, statics, deploy-guard, and the `--cfg madsim`
  DST clippy + tests. `cargo-mutants` is the advisory C5 gate's tool; I did not run it — M12/M13
  above are the hand-run equivalents for this round's lines. `git diff HEAD` was `cmp`-identical
  before and after the run.
- Diff coverage: `cargo llvm-cov --lcov -p wyrd-custodian --test segmented_map_repoint --test
  segmented_map_reconstruction`, scored with the gate's own hooks (`run-diff-cov.sh
  --changed-lines patch.diff`, then `--score`): **`TOTAL 66 67`** (98.5%, floor 80%), the one
  line being `MISS crates/custodian/src/reconstruction.rs:443` — the `"unplaced"` call site
  explained above. v3's store-fault and contained-arm lines are all hit.

## Things for the human (unchanged from v3 unless marked)

- **New: ADR-0011's metric table** (`docs/design/adr/0011-durability-telemetry-and-declarative-management.md:34`)
  describes `reconstruction_aborted` as "the repair **could not place** the rebuilt shard (the
  failure-domain selector chose a server outside the fleet view)". After this patch it also
  counts contained moves (and on the base it already counted staged aborts). The ADR names
  `reconstruction.rs` as the source of truth for exact emission (`:42`), and the brief forbids
  ADR edits, so it is unchanged. The rubric's docs-currency trigger (port, API operation, RPC,
  CLI flag, persisted field) is not hit: an audit-row field is none of these. Decide whether the
  ADR row wants a follow-up.
- **New: the `"unplaced"` reason is unbound by any test** — see "The `"unplaced"` call site".
- **C3, fourth file:** `RepairPlan.chunk_index` had one reader, the base's inline record build
  that the primitive replaces; left in place it is a `dead_code` error under clippy `-D
  warnings`. The patch deletes its one writer at base `staged.rs:369` (−1 line, no behaviour
  change).
- **DST deferral to #682:** `reconstruction.rs:1157-1158` and the test's module doc carry
  `deferred: #682` for the seeded Tier-0 DST case. #682 must not be closed when #722 lands unless
  its DST case covers the reconstruction caller too.
- **#776's docs marker** (`crates/core/src/metadata.rs:3238-3240`) says "deferred: #777 … nothing
  calls this yet". After this patch something calls it; `metadata.rs` is off-limits here. No
  sentence in `06-runtime-view.md` §6.3 or `08-crosscutting-concepts.md` became false.
- **#698 interaction (not fixed, by instruction):** a segmented root under `inode:01` now
  conflicts every pass and answers `Satisfied`, where the base refused and answered `Blocked`.
  The flat arm has always behaved this way; #698 owns noncanonical keys.
- **Flat-arm side effect (leg 10):** a flat record whose chunk lengths overflow `u64` before the
  owed chunk is now contained every pass; the base repaired it by index. Defensible under
  ADR-0045 ("strict in maintenance paths"), but it is a flat-arm behaviour change.
- **Ceiling refusal on a superseded generation** pages a human without re-checking the root is
  still current (primitive contract, `metadata.rs:3134-3138`). Raised in round 3, deferred.
- `emit_conflict`'s "collectable garbage" wording is unchanged; #723 owns it.

## Scratch

Under `$PDCA_SCRATCH` (`/var/tmp/pdca/wyrd-pdca-9c587031/issue_777`): `pdca-builder-777-v4/`
holds the reverted-file copies, `before.diff`/`after*.diff`, `red.log`, `mutate.py`, `final.py`
and the CI logs; `pdca-builder-777-v4.diff` is the size-measurement copy of the patch. Nothing
deleted; the harness reclaims the root.
