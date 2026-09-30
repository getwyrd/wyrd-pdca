# Build notes — #776 seg-record placement-move primitive (iteration 3)

Target: getwyrd/wyrd @ main (worktree base `243241e`). One file changed:
`crates/core/src/metadata.rs`. `patch.diff` is **49 615 bytes** (budget 50 KB; iteration 2
was 49 337). Line numbers below are in the patched file unless marked "main". The brief's
own numbers (`:380`, `:870`, `:1776`, …) come from an older main and have moved, so I cite
by symbol too.

Production semantic lines added (not blank, not comment/doc, before the test module):
**145** (budget ≤170), the same count as iteration 2: the two removed `ChunkMapError`
variants and their `Display` arms roughly balance the new error struct, its two impls,
and the new `Repoint` variant.

## What this iteration changed

Iteration 2's patch (`iteration-v2/patch.diff`) is the base. Its gates were all green; the
auto-iterate came from two adversary findings marked `[impl]`. Both are fixed. The third
adversary finding (`[human]`, flat-arm `Conflict` on a miss no retry can fix) was deferred
to sign-off by the driver and is **not** changed here (see "Left for the human").

### 1. The flat arm's "`state` left as it was" was not pinned

The adversary showed that adding `state: InodeState::Committed` to the flat arm's
`InodeRecord { .. }` (the `commit_chunk_map` idiom, main `metadata.rs:2158`) left all 16
tests green, because every test built a `Committed` generation.

I kept the documented behaviour (preserve `state`) and pinned it, rather than refusing a
non-`Committed` generation:

- Test: `flat_arm_moves_the_placement_and_advances_the_version_preserving_metadata`
  (`metadata.rs:4563`) now builds its generation as `Pending` (`:4566-4569`). The existing
  whole-record assertion (`..root`) then requires the stored record to still be `Pending`.
- The other direction (forcing `Pending`) is caught by the seeded campaign and by the flat
  ceiling test, whose generations are `Committed` (negation N11 below).
- Doc: `repoint_chunk` now says why (`metadata.rs:3200-3203`): "a move is not a
  publication, so unlike `commit_chunk_map` it never marks a generation `Committed`."

Why not refuse `Pending`: a refusal needs a new outcome, its doc and a test, for a case no
caller reaches (the only planned caller's loop skips non-committed records before it
resolves, `crates/custodian/src/reconstruction.rs:633`). And refusing is a lifecycle
policy decision (should a repair touch an in-flight upload?) that belongs to #777, which
owns the caller. The primitive's job is to move one placement and nothing else, which is
exactly what the pinned behaviour says.

### 2. The two new `ChunkMapError` variants broke that enum's contract

The adversary's point: every custodian consumer downcasts `ChunkMapError` to mean "this
object is unreadable, contain it and keep walking" (`reconstruction.rs:639-645`; same at
`gc.rs:1245`, `rebalance.rs:316`, `restore.rs:734`, `backfill.rs:163`). A caller bug
(wrong-length placement) would then be filed as per-object corruption.

The diff now adds **no** `ChunkMapError` variant; that enum and its `Display` impl are
untouched. The two conditions went to where their meaning puts them:

- **Wrong-length replacement → `MalformedReplacement`** (`metadata.rs:3148-3171`), a new
  error struct with `Display` and `std::error::Error`. It is a fault of the **call**, so it
  is not a `ChunkMapError`: when #777 matches on `downcast::<ChunkMapError>()`, a planner
  bug falls through to the "not this object's fault" arm and surfaces as a bug instead of
  being contained. The check itself is unchanged (`:3232-3239`, before anything is read).
  - Why not reuse `MalformedPlacement` (main `metadata.rs:460`): its doc defines it as a
    **committed** vector that is non-empty and the wrong length, found by
    `checked_fragments`, and it is an operator-signal type for an object fault. The move's
    refusal also covers the **empty** vector and is a caller fault, so reusing it would move
    the same confusion into a smaller type. It also has no `Display`/`Error` impl, so it
    cannot go into the boxed error without editing an existing type.
- **Version at `u64::MAX` → `Repoint::VersionExhausted { version }`** (`metadata.rs:3133-3140`),
  an outcome rather than an error, returned at `:3247-3251`.
  - Why not keep it as a `ChunkMapError` with an amended doc: the enum's header says "a
    structural violation of the segmented chunk-map shape", and a flat record's version is
    not that. The object is readable, so "contain as unreadable" would be the wrong label.
  - Why not put it in an error type of its own: any non-`ChunkMapError` error aborts the
    custodian pass (`reconstruction.rs:646-648`, `Err(err) => return Err(err)`). One object
    at `u64::MAX` would then stop repair for every object, every pass: the "refused every
    pass, forever" failure this lineage exists to remove, made worse.
  - As an outcome it sits next to `Repoint::Refused` (the ceiling), which is the same kind
    of thing: the object's own state, not a race, not transient, handled per object.
  - Brief item 6 ("checked version advancement, no wrap, no panic") still holds: the
    advance is `checked_add(1)`, and the refusal writes nothing.

Tests changed to match:
- `flat_arm_refuses_a_version_it_cannot_advance` (`:4588`) asserts
  `Ok(Repoint::VersionExhausted { version: u64::MAX })` and a byte-identical store.
- `a_replacement_placement_must_name_every_fragment` (`:4936`) asserts, for each wrong
  length in both arms, that the error downcasts to `MalformedReplacement { expected: 3,
  actual }` (`:4958`) and shows as `replacement needs 3 D servers, got {actual}` (`:4960`).
  The `Display` assertion exists because `cargo mutants` generates a `fmt → Ok(())` mutant
  for the new impl; it is unviable under the workspace lints, but the hand-applied form
  (N13) is caught by this assertion.

## Unchanged from iteration 2 (still holds)

- Flat arm = `commit_chunk_map`'s idiom (fn at `metadata.rs:2142`, untouched, including
  its segmented refusal): `..generation.clone()` (ADR-0047 metadata kept), next version via
  `checked_add`, `require(inode, encode(prior)) + put(inode, encode(next))`.
- Segmented arm finds candidates in the root's own table (`segment_may_hold`, `:3338`). No
  `seg:` range walk, at most two `get`s, the root is never `put`.
- The three pins, as settled: root generation bytes (`:3241`); the segment record's bytes
  **as read fresh here** (`:3308`; the docs say plainly this is not the resolve's bytes);
  the `ChunkRef` itself (`chunk_at`, `:3351`).
- Both arms weighed by `flat_value_ceiling_crossed` (full `MAX_VALUE_BYTES`) in `weighed`
  (`:3323`). No `MAX_ROOT_VALUE_BYTES` comparison in the diff; its one added mention is doc
  text on `flat_value_ceiling_crossed` (`:596-600`).
- An over-ceiling live `seg:` row is refused before decode, through `retired_or` (`:3279`,
  `:3316`).
- Seeded Tier-0 campaign `seeded_moves_race_each_other_and_the_roots_retirement` (`:5242`,
  driving `campaign` at `:5079`), 256 seeds of `wyrd_testkit::Sim` over the real redb store
  and the production resolver. Unchanged.
- Untouched: `commit_chunk_map`, `ChunkMapError`, the resolver and read side, every
  custodian file, proposal 0016, conformance vectors. No new dependency.

## Verification posture

Green-only under C4-verify by design, as the brief says: no new `tests/*.rs`, all tests
in-crate `#[cfg(test)]`. The binding oracle is `cargo mutants` plus the named negations.

### Named negations (applied one at a time, run, then restored)

Harness: `$PDCA_SCRATCH/pdca-builder-776-neg3/run.sh`. It copies the known-good file,
applies one whole-file perl substitution (and fails loudly if nothing changed), runs
`cargo test -p wyrd-core --lib placement_move` under `timeout 1500`, prints the failures,
restores, and `cmp`s against the good copy. Every run ended "restored", and the final file
is byte-identical to the good copy. Logs: `negations-{a,b,c}.log` in that directory.

| # | Negation | Tests that went red |
|---|---|---|
| N1 | `chunk == prior` → `(chunk == prior \|\| black_box(true))` (`:3357`) | 5: `chunk_at_needs_…`, `flat_arm_conflicts_…`, `a_zero_length_chunk_…`, **`a_sibling_edit_…` (same-chunk conflict leg)**, `seeded_moves_race_…` |
| N2 | root pin dropped (`:3241` → `WriteBatch::new()`) | 3: `flat_arm_conflicts_…`, **`a_superseded_root_fails_…`**, `seeded_moves_race_…` |
| N3 | segment-bytes pin dropped (`:3308`) | 2: `a_sibling_edit_…`, `seeded_moves_race_…` |
| N4 | oversize-row guard off (`:3279`, `&& black_box(false)`) | 1: `a_segment_row_over_the_ceiling_…` |
| N5 | placement-length check off (`:3233`) | 1: `a_replacement_placement_must_name_every_fragment` |
| N6 | `checked_add(..)?` → `saturating_add` in `chunk_at` (`:3360`) | 1: `chunk_at_needs_both_…` |
| N7 | version advance unchecked (`wrapping_add(1).checked_add(0)`, `:3247`) | 1: `flat_arm_refuses_a_version_it_cannot_advance` |
| N8 | output ceiling off (`black_box(None::<usize>)` in `weighed`, `:3324`) | 2: both ceiling tests |
| N9 | `retired_or` skipped (`:3316`) | 2: `a_damaged_segment_…`, `a_segment_row_over_the_ceiling_…` |
| **N10** | flat arm adds `state: InodeState::Committed` (**the adversary's surviving mutant**) | 1: `flat_arm_moves_the_placement_…` |
| **N11** | flat arm adds `state: InodeState::Pending` | 2: `seeded_moves_race_…`, `flat_arm_refuses_a_record_…` |
| **N12** | placement refusal returned as a `ChunkMapError` (the iteration-2 shape) | 1: `a_replacement_placement_must_name_every_fragment` |
| **N13** | `MalformedReplacement::fmt` writes nothing, returns `Ok(())` | 1: `a_replacement_placement_must_name_every_fragment` |
| **N14** | `VersionExhausted` reports `version: 0` | 1: `flat_arm_refuses_a_version_it_cannot_advance` |

The brief's four named negations are N1 (equality → same-chunk conflict test red), N2
(root pin → superseded-root test red), N8 (ceiling comparison → ceiling tests red; the `>`
itself lives in the pre-existing `flat_value_ceiling_crossed`, so the negation removes
this diff's use of it) and N7 (unchecked version → exhaustion test red).

### cargo mutants

`cargo mutants --in-diff <this diff> --no-shuffle -p wyrd-core --test-package wyrd-core
-- --lib` (cargo-mutants 27.1.0, on the final diff; output in
`$PDCA_SCRATCH/pdca-builder-776-mutants3`): **36 mutants, 27 caught, 0 missed, 0 timeout,
9 unviable**, in 76 s.

The 9 unviable mutants fail to compile under the workspace's `warnings = "deny"` lints or
need a `Default` that does not exist. I re-applied every one that can exist in a
lint-clean form (`if black_box(true) { return X; }` at the top of the body) through the
same harness. Every one went **red**:

| Unviable mutant | Hand-applied result |
|---|---|
| `segment_may_hold → true` | 3 failed |
| `segment_may_hold → false` | 11 failed |
| `chunk_at → None` | 13 failed |
| `chunk_at → Some(0)` | 7 failed |
| `chunk_at → Some(1)` | 7 failed |
| delete field `version` (as `version: version - 1`) | 2 failed |
| `MalformedReplacement::fmt → Ok(())` | N13 above, 1 failed |
| `repoint_chunk → Ok(Default::default())`, `weighed → Default::default()` | cannot exist: `Repoint` has no `Default` |

## Refute-your-own-test

- **(a) Genuine red?** Yes, per negation rather than per revert. The defect is an absent
  API, so reverting the patch removes the symbols and the tests stop compiling, as the
  brief says. Each piece of behaviour was negated on its own and went red: 14 named
  negations plus 6 hand-applied unviable mutants, and the `cargo mutants` run has 0 missed.
  The two findings this iteration fixes each have their own red: N10 (the adversary's
  exact mutant) and N12 (the iteration-2 error shape).
- **(b) Production path?** Yes. Every test calls the production `repoint_chunk`,
  `segment_may_hold` and `chunk_at` directly. Batches are applied through
  `MetadataStore::commit` on the real redb backend (`RedbMetadataStore::in_memory()`). The
  campaign plans through the production resolver `resolve_current_chunk_map`. The error
  type test downcasts the real boxed error the function returns.
- **(c) Fixture includes the fault?** Yes. The faults are in the store or the arguments:
  a `Pending` generation (the case the adversary said was missing), a version at
  `u64::MAX`, wrong-length replacements (including empty) in both arms, a V+1-byte live
  row whose move would shrink it, a V-byte row that must still be read, offset `u64::MAX`
  against a list whose sum overflows, and, in the campaign, real concurrent moves, root
  overwrites, root deletes and reclaimed segment records over 256 seeds. "Nothing written"
  is always a full-store `scan(b"")` compared before and after.

## Gates run

- `./engine/xtask.sh ci` (= `cargo xtask ci`, the C4-ci gate) with `PDCA_WORKTREE` set to
  the worktree, on the final file: **exit 0, "xtask ci: all checks passed"**. Every step
  ran: typos, docs lint and render, gitlink-guard, unsafe-guard, `cargo fmt --all --check`,
  workspace clippy/build/test (all 16 `placement_move` tests among them), cargo-machete,
  the three cargo-deny checks, conformance, statics, deploy-guard, and the madsim
  `wyrd-dst` leg. Log: `$PDCA_SCRATCH/pdca-builder-776-ci4.log`.
- **A hang on the first CI attempt, not caused by this diff.** In the first run
  (`$PDCA_SCRATCH/pdca-builder-776-ci3-hung.log`), six tests in
  `crates/server/tests/custodian_gc.rs` all hung together (`deployed_role_defers_gc_…`,
  `deployed_role_keeps_orphaned_bytes_…`, `deployed_role_reclaims_…` ×2,
  `deployed_role_defers_gc_when_the_operator_fleet_is_startup_partial`,
  `deployed_run_loop_refuses_duplicate_ids`) for over 10 minutes, with low host load and
  every thread in a futex or epoll wait. That binary finished in 0.17 s in iteration 2's
  CI. Run alone it passed at once, and the whole file then passed 6 times in a row
  (0.18-0.19 s each, under `timeout 30`). The re-run of the full gate passed. This diff
  cannot reach that code: it changes nothing any custodian or server path calls, and every
  existing type is as on main. `ptrace` is restricted on this host (`ptrace_scope = 1`),
  so I could not take a stack trace. This looks like a rare deadlock in the deployed
  custodian run loop's tests (possibly in the shutdown path, since `drive_deployed_loop`
  ends on a 60 ms `sleep`). Worth an upstream issue; it is outside this change.
- `cargo fmt -p wyrd-core` was applied, so the patch is what the target's formatter
  produces.
- While iterating: `cargo test -p wyrd-core --lib placement_move` under `timeout 1500`
  (xtask has no scoped test subcommand). 16/16 pass.
- The external dependencies the brief lists were all exercised: `typos`, the docs renderer,
  `cargo-deny`, `cargo-machete` (inside `xtask ci`) and `cargo-mutants` (above). No
  NEEDS-HUMAN external dependency.

## Left for the human

- **Deferred `[human]` finding, not changed:** in the flat arm a `chunk_at` miss is a pure
  function of the caller's own arguments, yet it returns `Repoint::Conflict`, whose doc
  says "re-plan next pass". A planner with a deterministic offset bug would retry forever.
  Brief item 7 allows "conflict/`Blocked`", and a caller that re-resolves between plan and
  move can hit this miss legitimately, so I left it. If you decide #777 needs a distinct
  outcome, `MalformedReplacement` shows the shape: a call-fault error that is not a
  `ChunkMapError`.
- **`ChunkMapError`'s header doc** (main `metadata.rs:609-613`: "raised at decode or at an
  unwired site") was already stale on main (e.g. `TooManySegments` is raised at resolve).
  This diff adds no variant to it any more; the segmented arm raises the resolver's
  existing variants with the resolver's meaning ("while the root still names that
  generation"). I left the header alone as out of scope.
- **Docs currency rubric.** This adds a public library function, an outcome enum and an
  error struct in `wyrd-core`. No port, gateway/API operation, RPC, CLI flag or persisted
  field, and the brief limits the change to one file, so I did not touch the living
  architecture doc. If "API operation" is read to include core library entry points, that
  update belongs with #777, where the primitive gets its first caller.
- Scratch used: `$PDCA_SCRATCH/pdca-builder-776-{target,neg3,mutants3,u3.diff,ci3.log,
  ci3-hung.log,ci4.log,hang.txt}` plus earlier iterations' leftovers. The CI runs built
  into the worktree's own `target/`. Left for the harness to clean up, per the builder
  rule against rm-style commands.
