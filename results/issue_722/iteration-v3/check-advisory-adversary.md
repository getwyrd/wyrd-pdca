# Adversarial review — #722 (a drain holding a `seg:`-resident fragment completes)

**Result: I could not refute the fix.** I re-ran the evidence, ran each named negation myself, and probed six edge cases against the patched pass. Nothing broke. No NEEDS-HUMAN items.

- **The red→green evidence holds.** Green reproduces locally (12/12 in `segmented_map_evacuate`, 5/5 in `segmented_map_rebalance`). In the frozen red log (`gate-logs/C4-verify.log`), legs 1 and 2 fail on the base for the defect itself: "the move landed: Blocked vs Changed", next to the base's `refused-segmented` line (`crates/custodian/tests/segmented_map_evacuate.rs:418`, `:479`). Every leg drives the real `reconcile_step` and checks the store, so no test runs against a copy of production logic. The test hooks are faithful too. The resolver reads `seg:` records only through `scan_page` (`crates/core/src/metadata.rs:2839-2854`), so a `seg:` `get` is always the move's own re-read (`metadata.rs:3288`). The hook at `segmented_map_evacuate.rs:115` therefore cannot fire during the scan and pass for the wrong reason.

- **The named negations hold. I ran each one.** Deleting the root-generation `require` (`metadata.rs:3257`) turns the DST (deterministic simulation) property red at `MADSIM_TEST_NUM=50`. Deleting the `seg:` `require` (`metadata.rs:3324`) turns the DST property red and custodian leg 4 red (`segmented_map_evacuate.rs:604`). Disabling the ceiling check in `weighed` (`metadata.rs:3340`) turns legs 3 and 5 red. C4-ci really does sweep 50 seeds (`xtask/src/main.rs:1716`, `:1750`), and its log shows `drain_repoint_versus_supersede_is_all_or_nothing ... ok`. The property's doc comment (`crates/dst/tests/custodian.rs:5921-5929`) correctly says it does not reach the read→prepare window.

- **Edge-case probes against `crates/custodian/src/rebalance.rs:364-452` and `:590-630` did not break it:**
  - two drained chunks in one `seg:` record: one pass, `Changed`, drain `Satisfied`;
  - the same `ChunkRef` twice in one `seg:` record: both positions moved in one pass;
  - one chunk shared by a flat object and a segmented object: both repointed;
  - a segmented root superseded mid-resolve by a *flat* generation: moved through the flat arm on the successor (version 7→8);
  - one object with two `seg:` records, the second torn under the move: the first move lands, the object is named once, `Blocked`;
  - a pre-M3 chunk with an empty placement inside a `seg:` record: `Changed`, and the equality pin matches what the resolver returned.

- **The shared-chunk case (brief leg 2) is safe even when only one of the two moves lands.** The first move's orphan mark then sits on a fragment the second object still names. GC checks references before it judges any mark, including a `Reclaiming` one (`crates/custodian/src/gc.rs:662-669`), and builds that reference set through the shared resolver, `seg:` records included (`gc.rs:1242`). The fragment is kept, so this is not a hole.

- **Note, no action: leg 3 is keyed to `MAX_VALUE_BYTES`, not the V/2 the brief asked for** (`brief.md:39-45`). The brief's premise was wrong. #721's primitive weighs a `seg:` record against the full ceiling and documents why (`metadata.rs:593-600`, `:3218-3219`). The builder followed the primitive and added no second ceiling, which is what the brief's scope line asks (`brief.md:142-144`). The test is right and the brief text is stale.

- **Note: C4-verify's "12 test(s) ran red" overstates the evidence.** Only legs 1 and 2 test the defect. Leg 10 (`segmented_map_evacuate.rs:816`) is red on the base because the base *did* drain that corrupt flat object (the red log shows an `evacuate` line for chunk `c4a1`). The patch now names the object and holds the drain until an operator repairs it. That is a deliberate change. It matches the reconstruction peer (`crates/custodian/src/reconstruction.rs:693-698`), and only a corrupt flat record (chunk lengths summing past `u64`) can reach it, so I don't count it as a refutation. It just shouldn't be counted as evidence of the defect either.

- **Note, pre-existing and not this diff:** a *flat* object owing two evacuations takes two passes (probe: `Blocked`, then `Changed`, then `Satisfied`). The second plan pins the generation the first move just replaced, so it loses its CAS (compare-and-swap) after copying. The base pinned the scanned record per plan in the same way, and that copy falls under the #723 leak class. Segmented objects don't have this problem because the move never rewrites their root.

- **Note, below the rubric's docs-currency bar:** `metadata.rs:3238` still says reconstruction is "the caller" of `repoint_chunk`, and `docs/design/architecture/08-crosscutting-concepts.md:89` names only reconstruction as a `seg:` writer. Both are now incomplete. The brief forbids editing `metadata.rs` and caps the patch at 4 files. The diff also changes no port, API operation, RPC, CLI flag or persisted field. So this is a follow-up, not a blocker.

Attempted to refute: the red→green proof, all three named negations, the hook fidelity, offset addressing over repeated, shared and identity-placed chunks, a mid-resolve supersede onto both shapes, partial failure within one object, and GC safety for a partly moved shared chunk. None of these attempts succeeded.
<!-- pdca:leaf-complete -->
