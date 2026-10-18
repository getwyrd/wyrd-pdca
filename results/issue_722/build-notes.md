# Build notes — issue #722, iteration 4 (segmented drain evacuation completes)

Target: getwyrd/wyrd @ main, base `0fba76c` (worktree `$PDCA_WORKTREE`, re-fetched this
iteration: `origin/main` is still `0fba76c`). #721's primitive is on this base:
`metadata::repoint_chunk` (`crates/core/src/metadata.rs:3240`, commit `842f48e`), shipped by
PR #850 (merged 2026-10-07); issue #721 is CLOSED. Line numbers below are on the patched tree
unless marked "base".

Four files, the brief's budget. Non-test change: **105** added non-blank, non-comment lines in
`rebalance.rs` (cap 200). `patch.diff` is **90,650 bytes** (cap 95 KB). No edit to
`metadata.rs`, `reconstruction.rs` or any other out-of-scope file.

## What iteration 4 changed: nothing in the code, on purpose

**`patch.diff` and the test file are byte-identical to iteration 3** (`cmp` against
`iteration-v3/patch.diff` and `iteration-v3/segmented_map_evacuate.rs`: identical). I applied
iteration 3's patch to a clean worktree at `0fba76c`; it applied without fuzz.

Why no change: iteration 3 passed every gate (C4-ci, C4-verify, diff coverage 97.6%, C5 15
caught / 0 missed, T4 batch review 0 blocking, tikv compile). The round-3 carry-forward names
exactly one item, and it is not a defect in the patch:

> T4 Contribution — Confirm merged and closed/rejected prior art for all four affected paths:
> this artifact contains one synthetic commit and no remote, so its history cannot settle
> duplication or rejected direction.

The reviewer's copy of the target has one synthetic commit and no remote, so it could not run
the history check. No code or test change can give it history. What I can do is run the check
against the real remote and record the result, so the human can clear the item at sign-off.
That is below.

**Harness note for the human (not build work):** auto-iterate classified this item as
"implementation-level" and sent the bundle back to Do, but it is a NEEDS-HUMAN item caused by
the reviewer's sandbox. Iteration 3's reviewer itself filed it under §6 NEEDS-HUMAN and wrote
"No additional builder-fixable defect was established." If the reviewer keeps getting a
history-less checkout, this item will come back every round and auto-iterate will keep
rebuilding an unchanged patch. Two possible fixes, both in the harness: give the reviewer a
checkout with `origin/main` history, or have auto-iterate route "cannot check prior art from
this artifact" to the human instead of to Do.

## Prior art — checked against the live remote (clears the carried-forward T4 item)

Run on 2026-10-10 from the worktree after `git fetch origin` (`origin/main` = `0fba76c`), and
with `gh pr list -R getwyrd/wyrd --state all --limit 400 --json number,title,state,files`
(374 PRs returned, numbers 1–883, so the list is complete — fewer than the limit).

**Merged history by path** (`git log origin/main -- <path>`):

- `crates/custodian/src/rebalance.rs` — 13 commits. Most recent: `c40948a` (#822, drain status
  counts staged multipart bytes), `9470de5` (#770, fragment-write deadline), `d2609b2` (#718 /
  issue #710, refuse oversized flat placement writes), `3829097` (#705 / issue #696, contain
  per-object faults — the refusal this patch completes), `3e05891` (#672, segmented record
  shape). Older: #193 (the drain loop), #357 (pre-M3 identity placement), #361, #397, #531,
  #564, #594.
- `crates/custodian/tests/segmented_map_rebalance.rs` — 2 commits: `3829097` (#705), `9470de5`
  (#770).
- `crates/dst/tests/custodian.rs` — 30 commits, the Tier-0 custodian property set; latest
  `fc7eecc` (#858). None adds a drain repoint property: the base file has no
  `drain_repoint…` test (`git show origin/main:crates/dst/tests/custodian.rs | grep -n 'fn
  drain_'` → nothing).
- `crates/custodian/tests/segmented_map_evacuate.rs` — no history (new file).

**Not a duplicate.** The base still refuses: `let Some(prior_chunks) = scanned_flat else`
at base `rebalance.rs:371`, `emit_refused` at base `:398` / `:660`, and the base file has zero
calls to `repoint_chunk`. #850 (issue #721) converted only the **repair** caller
(`reconstruction.rs`); it did not touch `rebalance.rs`.

**Closed or rejected work touching these paths** (from the full PR list, filtered to the four
paths):

- **#647** — CLOSED unmerged 2026-07-30, touched `rebalance.rs` and `dst/tests/custodian.rs`.
  Closing comment: "This PR will not work as is and needs to be amde smaller. closing" —
  closed for size, not direction, as the brief says. This patch does not bring back #647's
  custodian-local `crates/custodian/src/resolve.rs`; it reads through the shared
  `metadata::resolve_chunk_map` (unchanged) and moves through `metadata::repoint_chunk`.
- **#336** — CLOSED unmerged, a docs ADR PR that happened to touch `dst/tests/custodian.rs`.
  Unrelated.
- **No open PR** touches any of the four paths.

**Related tracker state:** #721 CLOSED (via #850). #722 OPEN (this). #723 OPEN, milestone
*Foundations* — owns the inherited lost-CAS leak this patch extends to `seg:` records (see
below). #682 OPEN (the parent).

**In-harness rejected work:** `results/issue_638/review-rejected.md:15-16` (do not retract a
losing write). This patch retracts nothing: on a lost CAS the copied fragment is left in place
(leg 4 asserts it).

## The change (unchanged from iteration 3; reviewed again, still correct)

- **The refusal is gone.** Base `rebalance.rs:371-374` (`scanned_flat` else-refuse), base
  `:398-399` (the per-object `emit_refused`) and base `:660` (`emit_refused` itself) are
  removed. Every owed chunk now gets a plan, flat or segmented.
- **The move is #721's primitive.** `evacuate_chunk` calls `metadata::repoint_chunk`
  (`rebalance.rs:590-598`), mirroring the repair caller. The orphan marks for the vacated
  positions are added to the batch the primitive returns (`rebalance.rs:640-645`), so the
  placement change and the orphan evidence are one commit (`0005:298-299`, ADR-0015). The
  hand-built `InodeRecord` (base `:519-530`), the local ceiling check (base `:541-547`) and the
  hand-built `require`/`put` batch (base `:557-560`) are removed — one ceiling check, inside the
  primitive.
- **Bounded memory.** `EvacObject` (`rebalance.rs:105-111`) holds one root snapshot per object
  that owes a move, shared by every plan in it (`rebalance.rs:436-442`). The base deep-copied
  the whole flat chunk list into every plan (base `prior_chunks: prior_chunks.to_vec()`); the
  patched plan holds one `ChunkRef` and a byte offset.
- **Per-(object, chunk) planning kept, no cross-object dedup** (doc at `rebalance.rs:304-307`),
  so two objects sharing a `ChunkId` both get repointed (leg 2).
- **Address by byte offset, with overflow contained.** The offset is an `Option<u64>` built
  with `checked_add` (`rebalance.rs:382-385`); an owed chunk with no address contains the object,
  named once (`:418-424`), as the peer does.
- **Non-canonical segmented root key contained** (`rebalance.rs:374-375`, `:413-417`), as the
  peer does.
- **Each contained object named once per pass** — one `contained` set and one `contain` helper
  (`rebalance.rs:254`, `:260-264`), used at scan time and at move time (`:189-193`).
- **Verdict mapping mirrors the peer** (`rebalance.rs:599-629`): a terminal verdict
  (`Refused`, `VersionExhausted`) on a root that has since moved is a stale plan and is answered
  as a conflict (`still_current`, `:662-665`); on a live root it is the object's own refusal or
  containment. A `ChunkMapError` from the move's re-read contains the object; any other error
  ends the pass.
- **The move pins the generation the resolve answered from** (`rebalance.rs:439`,
  `resolved.record`), not the scan's snapshot — bound by leg 12.
- **Not touched, per the brief:** the "collectable garbage" wording (module doc, the conflict
  comment, `emit_conflict`), owned by getwyrd/wyrd#723.

## Red → green evidence (this iteration)

Runner: the gate's own script, `PDCA_BUNDLE=<bundle> PDCA_LANE=0 timeout 1500
./engine/scripts/run-verify.sh` (it applies `patch.diff` to a clean `../wyrd-verify-l0`
worktree at the base and runs `cargo test -p wyrd-custodian --test segmented_map_evacuate`
both ways). Result: **`run-verify.sh: PASS — red without the fix, green with it`**, exit 0.

- **Green (fix applied):** `running 12 tests` … `12 passed; 0 failed`.
- **Red (production reverted, test kept):** `running 12 tests` … `1 passed; 11 failed`.
  Legs 1 and 2 fail on the refusal itself: `segmented_map_evacuate.rs:434` "the move landed —
  left: Blocked, right: Changed" and `:492` "both moves landed — left: Blocked, right: Changed".
  Leg 3 passes (refused for the segmented reason, as the brief predicts). Leg 4 fails because its
  racer never fires (the ordinary reason the brief describes). Legs 5–12 fail because the base
  never builds a plan, panics on `version + 1` (base `rebalance.rs:525`), or never names the
  object.
- **The C4-verify count is 12 ran / 11 failing, not the 4 / 3 the brief's posture predicts.**
  Legs 1 and 2 are the discriminating evidence for the success criterion; legs 5–12 exist
  because the iteration-1 and iteration-2 reviews required them.
- `crates/custodian/tests/segmented_map_rebalance.rs` (the forced rewrite of its leg 2, now
  `an_owed_segmented_evacuation_lands_in_the_seg_records` at `:338`): 5 passed.
- DST property 19, `RUSTFLAGS="--cfg madsim" MADSIM_TEST_NUM=50 cargo test -p wyrd-dst --test
  custodian drain_repoint_versus_supersede` (`crates/dst/tests/custodian.rs:6384`): passed.
- `cargo fmt --all -- --check`: clean. `cargo clippy -p wyrd-custodian --all-targets -- -D
  warnings`: clean. `typos` over the four files: clean.
- **Not re-run this iteration:** the full `./engine/xtask.sh ci`, diff coverage and C5. The
  patch and the base are byte-identical to iteration 3, where all three passed; Check re-runs
  them.

### Named negations (mutation oracle) — re-run this iteration on this tree

Each negation edited `crates/core/src/metadata.rs` (not in the patch) and was restored with
`git checkout crates/core/src/metadata.rs`. After all runs, `git diff` of the worktree compared
byte-identical to `patch.diff` (`cmp`).

| Negation | Test | Result (this iteration) |
|---|---|---|
| root `require` deleted: `metadata.rs:3257` → `let root_pin = { let _ = encode(generation); WriteBatch::new() };` | DST property 19, 50 seeds | **red** at `crates/dst/tests/custodian.rs:6163` "a lost move still rewrote the record (drained 0, racer Supersede, move first: false)" |
| `seg:` `require` deleted: `metadata.rs:3324` → `{ let _ = bytes; root_pin },` | DST property 19, 50 seeds | **red** at `custodian.rs:6167` "a lost move still marked a fragment for reclamation (drained 1, racer Rewrite, move first: false)" |
| same `seg:` `require` deletion | `segmented_map_evacuate` | leg 4 **red** at `:623` "a lost move does not certify — left: Changed, right: Blocked"; other 11 green |
| ceiling check deleted: `metadata.rs:3340` `match flat_value_ceiling_crossed(&next)` → `match None::<usize>` | `segmented_map_evacuate` | leg 3 **red** at `:563`, leg 5 **red** at `:683`; other 10 green |
| `rebalance.rs:439` pins the scan snapshot (`record.clone()`) | `segmented_map_evacuate` | leg 12 **red** at `:900` (`Blocked` vs `Changed`); other 11 green — iteration 3's run, unchanged bytes; iteration 3's reviewer reproduced it |

Leg 4: I found no fixture seam that lands the competing write before the fix and still proves a
lost CAS after it. Before the fix no plan exists, so no commit happens for the racer to beat.
Leg 4's conflict property rests on the `seg:` `require` negation above and on DST property 19,
as the brief allows. The DST property's doc comment (`crates/dst/tests/custodian.rs:5921-5929`)
says it proves the two commit preconditions and does not reach the read→prepare window.

## Refute-your-own-test (forced)

- **(a) Genuine red?** Yes. `run-verify.sh` reverted `rebalance.rs` to the base, kept the test,
  and got 11 of 12 legs failing; legs 1 and 2 fail on `Blocked` vs `Changed`, which is the
  refusal itself. Each named negation above turns its bound leg red while the rest stay green.
  Leg 3 is green on the base by design and is bound by the ceiling negation instead.
- **(b) Production path?** Yes. Every leg calls `wyrd_custodian::reconcile_step` →
  `rebalance::reconcile` → `plan_evacuations` → `metadata::resolve_chunk_map` → `evacuate_chunk`
  → `metadata::repoint_chunk`. The in-memory doubles only store bytes, check `require`
  preconditions, and inject one write or fault at a named seam. The test names no symbol this
  patch adds.
- **(c) Fixture includes the fault?** Yes. The draining server is in the fleet and holds the
  real fragment. Segmented objects are raw `seg:` records plus a segmented root — the exact shape
  the base refused. Leg 2 really shares one `ChunkId` across two objects. Leg 12's supersede
  lands between the resolver's page read and its root re-read (asserted: the hook fired). Legs
  4, 5, 7, 8 and 9 assert their injected race or fault actually fired.

## Things for the human at sign-off (carried from iteration 3, still open)

1. **Prior art** — evidence above; this is the item the round-3 carry-forward named.
2. **Leg 3 is keyed to `MAX_VALUE_BYTES`, not V/2.** The merged primitive weighs a `seg:`
   record against the full `MAX_VALUE_BYTES` (`metadata.rs:593-600`, `:3338-3347` →
   `flat_value_ceiling_crossed`), not `MAX_ROOT_VALUE_BYTES` (`metadata.rs:574`). A record seeded
   just under 50,000 bytes is repointed and committed. Keying leg 3 to V/2 would need a second
   ceiling in `rebalance.rs` (the brief forbids it) or a primitive change (#721's file, out of
   scope). Already in `deferred-findings.json`.
3. **C4-verify count** is 12 ran / 11 failing, not 4 / 3. Legs 1 and 2 remain the
   discriminating evidence.
4. **Docs currency.** No port, API operation, RPC, CLI flag or persisted field changes.
   `docs/design/architecture/08-crosscutting-concepts.md:89` names only the reconstruction pass
   as a `seg:` writer, and `metadata.rs:3238` calls reconstruction "the caller" of
   `repoint_chunk`. Both are now incomplete; a fifth file would break the budget. Suggested
   follow-up sentence: "The rebalance pass moves a draining server's fragment the same way,
   adding the orphan marks to the same batch."
5. **Stale deferral in the peer.** `reconstruction.rs:1166-1167` says "deferred: #682 — the
   seeded Tier-0 DST property for this move (repoint versus supersede)". Property 19 is that
   property, through the drain caller. `reconstruction.rs` is #721's file; I did not edit it.
6. **Telemetry removed.** The `rebalance_refused_records` counter and the `refused-segmented`
   audit action go away with the refusal. The only remaining mention in the repo is the test that
   asserts the line is gone (`segmented_map_rebalance.rs:384`).
7. **Inherited leak (getwyrd/wyrd#723), not new.** A lost CAS at commit leaves the copied
   destination fragment unreferenced and unmarked. The flat path already does this; this patch
   extends it to `seg:` records, as the brief decided. Leg 4 asserts the copy is left in place,
   not retracted. Prepare-time losses (leg 9) copy nothing.
8. **Behaviour shifts on the flat path**, both from routing through the primitive: a flat record
   at `version == u64::MAX` is now contained and named (the base overflowed `version + 1`, base
   `rebalance.rs:525`); a flat map whose chunk lengths overflow `u64` is contained and named
   instead of having its chunk moved by index.
