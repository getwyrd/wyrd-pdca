# Adversarial review — issue 653 (staged segment publication), round 3

**Bottom line.** I could not break the committer's safety logic. Every red leg the brief demands still goes red against this round's tests, and I found no input that commits a root naming a missing or different `seg:` record. I did find two refusal/accounting paths the tests never pin (a broken check passes all 21 core tests), one weak test claim, and one design gap at the flat/segmented boundary that needs a human decision.

All probes ran in a scratch copy of `$PDCA_TARGET`: `cargo test -p wyrd-core --test segmented_publication` (21 tests), plus the DST file under `--cfg madsim` with `MADSIM_TEST_NUM=50`.

## Findings

- NEEDS-HUMAN [impl] — `crates/core/src/staged_publication.rs:910`: the root value-ceiling refusal has no test, but leg (C) requires *each* deterministic refusal to leave zero commits. I replaced the check with `if false` and all 21 core tests still passed. With the check gone, a 632-chunk publication whose `meta.content_type` is `MAX_VALUE_BYTES` long committed 4 `seg:` rows and then a **100,388-byte root** on redb. On FDB (100,000-byte hard limit) that flip fails at the store *after* the segment phase, which is exactly the "refusal after durability" that (C) forbids. The shipped code is correct: I checked that it returns `ValueOverCeiling { phase: Flip, key: "inode:8" }` with zero `seg:` rows. Other evidence: C5 survivors `910:22` (`==`, `>=`) and C4-diff-cov MISS on lines 911-915. Two smaller gaps of the same kind: `ChunkTooLarge` at `:588` (diff-cov MISS 589-592, C5 survivors at 588), and `VersionOverflow` at `:881` (swapping `checked_add` for `wrapping_add` passes all tests). Fix: add rows to the existing (C) table at `crates/core/tests/segmented_publication.rs:827`: an over-ceiling `meta.content_type` (expect `ValueOverCeiling { Flip, inode:8 }`), and a `Supersede` whose `prior.version == u64::MAX` (expect `VersionOverflow`).

- NEEDS-HUMAN [impl] — `crates/core/src/staged_publication.rs:617` (and the split test at `:670`): leg (B)'s byte test never runs at the budget boundary, so nothing checks how the committer counts the bytes of its own rows. I changed `2 * row.key.len()` to `2 + row.key.len()`, which drops the `require_absent` key from each row's cost. All 21 tests passed. I then set the byte budget to 25,252, one byte under the measured two-row transaction (25,253 B, padded contribution as in the shipped test). The shipped code keeps one row per batch (largest batch 17,375 B). The mutant packs two rows and commits a **25,253-byte transaction over a 25,252 budget**. The shipped test at `crates/core/tests/segmented_publication.rs:674` uses `BUDGET = 23_000`, about 2.3 KB below the two-row cost, so it only catches errors of several KB. Round 1's T5 finding asked for "a boundary case", and that part is still open. C5's surviving mutants `617:8` (×2), `652:18`, `659:16`, `670:22`, `670:44`, `587:28` and `595:73` are the same kind. Fix: measure the two-row transaction from an unbounded run, then run once at `two - 1` (assert no batch holds two rows and every batch is within budget) and once at exactly `two` (assert the first batch holds two rows; this kills the `>`→`>=` survivors at `:670`).

- NEEDS-HUMAN [impl] — `crates/core/src/staged_publication.rs:968` (low priority): deleting the witnessed-row pin (`assembled.require(row.key, row.value)` replaced by nothing) passes all 21 tests. The comment at `crates/core/tests/segmented_publication.rs:729-730` says the recovery "pins them (`require(key, value)`, no put)", but the test only asserts "no put", and D3 does the same. This is not a C-1 hole: the full re-read at `:462` and the fence still guard the flip. But the brief's Design names the pin, and the test text claims a check it does not make. Fix: assert that the recovery's first batch carries `require(seg_key(i), bytes)` for each row that was already durable.

- NEEDS-HUMAN [human] — `crates/core/src/staged_publication.rs:534` / `:418`: the flat-or-segmented choice counts chunks only, while the segment split (`:580-600`) closes segments on encoded bytes. I measured with refs at the widest rendering the project sizes against (`crates/core/src/multipart.rs:4893-4908`):
  - `rs(10,4)` refs are 408 B, so 158 chunks publish a **64,699-byte flat root**. That is over the `V/2 = 50,000` budget that `MAX_MAP_CHUNKS` promises ("Never an over-budget value", `multipart.rs:4958`) and that `metadata.rs:574` calls a durability reserve.
  - `rs(20,10)` refs are 745 B, so 158 chunks are refused `ValueOverCeiling` (117,945 B) on every attempt, while **159** of the same chunks publish fine as 3 segments with a 330-byte root. Fewer chunks fail; more chunks succeed.

  `--durability rs(k,m)` is a live CLI flag (`crates/server/src/cli.rs:550-570`). The brief's Design says "at most `MAX_MAP_CHUNKS` chunks: the root is flat", so the builder did what was asked. A human should decide whether the committer should also segment when a flat root would pass `V/2`, or whether #658 must pass the scheme-aware `KnobSet` cap (`multipart.rs:5430`). This sharpens round 1's note about compile-time caps vs `KnobSet`: even with the shipped constants, the flat path has no byte rule.

- (Calibration, not a defect.) `crates/dst/tests/segmented_publication.rs:11-13`: the DST now mints a fresh inode id per attempt, as round 1 asked, and the header says only the flip's pins on the fence and the dirent keep a second root out. But when I removed the Fresh flip's dirent pin (`staged_publication.rs:920`), all 3 DST tests stayed green over 50 madsim seeds and the 64-seed sweep. The fence pin alone already stops a second flip. So the dirent pin's real red is the core test `a_the_flip_pins_the_dirent_on_both_branches`, not (G). The DST does go red when the flip's contribution preconditions are dropped (resolve panic at `crates/dst/tests/segmented_publication.rs:275`), so (G) does catch a missing fence pin.

- Tried to refute, could not. I re-ran the brief's demanded reds against this round's tests, and each still fails:
  - pre-flip check over only `0..resume_from`: D2 fails;
  - fixed two-row batches: (B) and (D1) fail;
  - fence pin dropped from the flip: F-r fails at `tests:1346` (a root over a drained range);
  - owned-key check off: (C) fails;
  - cycle check off: (F) fails;
  - `read_group` stopping after one page: `a_a_map_over_one_page…` fails.

  These attempts to get a root over a missing or different `seg:` row all failed:
  - a fence that cycles only under another split, or only after a resume: refused on the first attempt (`:790-832`);
  - a stale or inflated `resume_from`: the pre-read and the full re-read catch it;
  - a non-canonical alias key such as `seg:…:3`: `parse_seg_key` enforces the 6-digit width (`metadata.rs:1540`);
  - a flip that deletes the fence: accepted as a move. A flip that both puts and deletes it: `DuplicateMutation`;
  - a fence key inside the owned range: `OwnedKey`.

  I did not raise unbounded awaits: `metadata.rs:2812-2820` documents that backends bound their own waits and that core holds no timer. C4-verify's UNVERIFIABLE is the pre-declared net-new case; the reruns above stand in for its missing red.
<!-- pdca:leaf-complete -->
