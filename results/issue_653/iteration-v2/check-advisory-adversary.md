# Adversarial review — #653 staged segment publication (iteration 2)

**Bottom line.** I could not break the production fence logic or any of the round-1 fixes. I did find three guards in `staged_publication.rs` that the shipped tests never exercise. In a scratch copy I removed each one, and all 16 core tests stayed green. Each gap below has a concrete failing case that I wrote as a scratch test: it passes on the patch and fails once the guard is removed. All three are test gaps the builder can close; none is a bug in the code as shipped.

## What I tried to refute and could not

- **Fence track** (`crates/core/src/staged_publication.rs:507-518`, `:790-832`, `:837-857`). I replayed round 1's counterexample: a hook giving `(0,2): A→B`, `(0,1): A→C`, `(1,2): C→B`, with a flip `B→C`. The track is now `[A, C, B]`, so the flip to `C` is refused as `FenceCycled` on the **first** attempt, before any write. A recovery no longer gets a different verdict from the first attempt. Any split's real fence history is a subsequence of the track, and a cycle-free track has only cycle-free subsequences, so the per-batch `FenceOffCursor` check (`:515`) plus `check_track` closes the hole. `f_the_fence_verdict_is_the_same_on_every_attempt` (`crates/core/tests/segmented_publication.rs:1117-1149`) covers a resume after a whole-phase `A→B` batch.
- **Round-1 test fixes really go red.** In a scratch copy:
  - Removing the operations break at `:670` fails `b_the_operation_envelope_binds_where_bytes_do_not`.
  - Swapping out either dirent pin (`:920`, `:924`) fails `a_the_flip_pins_the_dirent_on_both_branches`.
  - Removing the per-batch track check at `:515` fails `c_every_deterministic_refusal_leaves_zero_commits`.
  - D2 now deletes index 3 **and** index 0 (`tests/segmented_publication.rs:932`).
  - The DST (deterministic simulation test) mints a fresh inode per attempt (`crates/dst/tests/segmented_publication.rs:145-147`), and ran in C4-ci (`gate-logs/C4-ci.log:4276-4278`).
- **Verify-then-flip is not atomic.** The brief accepts this under caller obligation 1, and F-r and F-p test both race orders through the real store.
- **C4-verify UNVERIFIABLE (exit 77)** comes from the brief's pre-declared net-new-API exception: the test cannot compile without the new module. This is not a refutation. The scratch mutation runs above are independent red evidence for the legs they touch.

## Findings

- NEEDS-HUMAN [impl] — **The flip's inode pins are untested on both branches.** `crates/core/src/staged_publication.rs:925` (Supersede `require(inode, encode(prior))`) and `:919` (Fresh `require_absent(inode)`). Replacing either with a precondition on an unrelated absent key leaves all 16 core tests green, and the DST only publishes `Fresh`. The brief's Design and `0016:663` require both pins. **Concrete failing case:**
  1. The caller reads inode 8 at version 1 as `prior`.
  2. A concurrent single PUT overwrites inode 8 to version 2 under the same dirent.
  3. `RootTarget::Supersede { id: 8, prior }` then publishes a segmented map.

  The patch returns `Conflict` and keeps the PUT's version 2. With the pin removed, the flip commits over it: the PUT's generation is lost, its chunks get no `retire:` obligation (the caller retires the generation it read), and two different generations both carry version 2. My scratch test of exactly this passes on the patch and fails on the mutant. The existing dirent test (`tests/segmented_publication.rs:512-555`) never changes the inode, so it cannot tell. Fix: add this case, plus `Fresh { id }` over an inode record that already exists.
- NEEDS-HUMAN [impl] — **`read_group`'s paging path has never run in a test.** `crates/core/src/staged_publication.rs:1008-1009`: C4-diff-cov reports `:1009` as MISS (`gate-logs/C4-diff-cov.log`). `SEGMENT_PAGE_LIMIT` is 128 (`crates/core/src/metadata.rs:2839`). The largest map any test publishes is 5 segments (`tests/segmented_publication.rs:40`). The 512-segment case at `:496-500` only calls `planned_segments` and never publishes. A `read_group` that returned after the first page passes all 16 tests. **Concrete failing case for that bug:** a 130-segment map (20,540 chunks, about 20 GiB at 1 MiB chunks, well within `MAX_ROOT_SEGMENTS = 512`) is refused `SegmentNotDurable` on every attempt and can never publish. My scratch test that publishes and resolves 130 segments passes on the patch (0.2 s in memory) and fails with first-page-only reading. Fix: publish one map with more than 128 segments.
- NEEDS-HUMAN [impl] — **The `StrandedRow` and `ForeignRow` refusals never run.** `crates/core/src/staged_publication.rs:1003-1004` and `:997-1002` are both diff-cov MISS (`:998-1001`, `:1004`). The stranded-row check matters for safety: the shared resolver rejects any row at index ≥ `segment_count` as `SegmentUnknown` (`crates/core/src/metadata.rs:3043-3048`), so a root flipped over a stray row never resolves. **Concrete failing case:** a durable `seg:<nonce>:4:00005` sitting next to a 5-segment plan at the same epoch. The patch refuses it with `StrandedRow { index: 5 }`. With the check removed, all 16 tests pass and the publish commits a root that the resolver will reject every time. My scratch test catches this. Fix: one test for each refusal.

## Low priority — not worth a rebuild on its own

- The surviving C5 mutants (`gate-logs/C5-mutants.log`) are mostly boundary changes (`>` to `>=` at `:652`, `:659`, `:670`, `:930`), plus three accounting ones:
  - `row_bytes` at `:617`: `2 * key` becomes `2 + key`, an undercount of about 45 B per row.
  - `batch_bytes` / `batch_ops` at `:696` / `:702`: deletes are subtracted instead of added.

  The real-world error is tiny compared with the headroom between `MAX_BATCH_BYTES = E_TX_BYTES / 2` (`crates/core/src/multipart.rs:5067`) and the transaction limit. A byte budget set one byte below the true two-row cost would kill the `:617` mutant. The `:847` mutant survives because no test has a flip that deletes the fence, so the `:848` branch never runs. That code is correct; only the test is missing.

## Where the verdict may have been too generous

- The brief's Design says the flip "pins the target dirent and inode on both branches" (`0016:663`). Round 1's carry-forward quoted that sentence but asked only for dirent tests. Iteration 2 added them, and they really discriminate. Nothing has tested the inode half in either round (first finding). The T4 pass (`gate-logs/T4-batch-review.log`, 0 blocking) did not catch this, and cargo-mutants cannot, because it never deletes a builder call.
<!-- pdca:leaf-complete -->
