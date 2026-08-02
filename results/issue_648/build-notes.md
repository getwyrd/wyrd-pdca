# Build notes — issue 648 (iteration 7)

*Withheld from the reviewer; written for the human at sign-off.*

## What this round changed, and why only this

The round-6 sign-off accepted the patch except for **one** finding, and named it precisely:

> Rejecting on the T4 batched-review TEST-GAP finding: add raw-byte decode test coverage for
> empty segmented maps (`NoSegments`) and zero-byte `SegmentRef`s (`EmptySegment`), per
> `crates/core/tests/segmented_map_record.rs:111`, so removing those guards would be caught.
> Other §6 items (C3 budget overage, C4 deny-check environment gap, T4 contribution tooling
> visibility, staged-delivery fitness, T5 PR #647 provenance) are not blocking — leave those
> as-is / not needing rework this round.

So the delta versus `iteration-v6/patch.diff` is **exactly two added test functions and nothing
else**. `crates/core/src/metadata.rs` and all 40 ripple/doc files are byte-identical to round 6
(verified: `git diff` of the restored worktree reproduced the round-6 hunks for every file but
the test). Production is untouched this round — the finding was a *test* gap, not a code defect.

**Added** (both raw-byte, base-visible-API only, per the brief's Test-file rule):

| Test | Guard it binds | Path:line |
|---|---|---|
| `an_empty_segmented_map_is_err` | `NoSegments` — `crates/core/src/metadata.rs:676-678` | `crates/core/tests/segmented_map_record.rs:186-219` |
| `a_zero_byte_segment_is_err` | `EmptySegment` — `crates/core/src/metadata.rs:704-708` | `crates/core/tests/segmented_map_record.rs:269-303` |

### Why the fixtures are shaped the way they are

The finding is a *mutation* claim ("removing those guards would leave structurally unresolvable
roots accepted"), so each fixture is built so that **no other invariant refuses it** — otherwise
the test would pass for the wrong reason and still not catch the guard's removal:

- **Empty map**: `segment_count: 0`, `segments: []`, `"size": 0`. The count agrees with the list
  (so `SegmentCountMismatch` at `metadata.rs:757-762` cannot fire), there is no index to
  duplicate/gap and no span to overlap, and `SegmentedMap::span()` returns 0 for an empty table
  (`metadata.rs:721-725`, `map_or(0, …)`), which `"size": 0` then confirms — so
  `SizeSpanMismatch` (`metadata.rs:1254-1259`) cannot fire either. Only `NoSegments` refuses it.
- **Zero-byte segment**: three segments `0..5`, `5..5`, `5..12` under `"size": 12`. Indices are
  0,1,2 in order (no `SegmentIndexOutOfOrder`), each `byte_offset` equals the running offset
  because a zero-length span advances it by nothing (no `SegmentsNotContiguous`), and the table
  spans exactly `size` (no `SizeSpanMismatch`). Only `EmptySegment` refuses it.

Each test also carries a **control**: the minimally-different well-formed root (one segment;
the same three-segment table with a one-byte middle segment) asserted `is_ok()`. That pins the
`Err` above to the element under test rather than to anything else in the bytes — the
"fixture includes the fault, and is otherwise valid" discipline. It also makes both new tests
participate in the red→green transition (see (a) below), which a bare negative case would not.

### Alternatives considered and rejected

1. **Assert the error *message*** (`err.to_string().contains("names no segments")`) instead of
   the control-twin. Rejected: it couples the test to `ChunkMapError`'s `Display` wording
   (`metadata.rs:480`, `:501`) — a wording edit would red the suite with no behaviour change —
   and it buys nothing the control does not, since the control already proves attribution. Cost
   comparison is not the axis here; both are ~4 lines.
2. **Put the cases in the co-located `metadata.rs` tests** (calling `SegmentedMap::new` with an
   empty `Vec`). Rejected on the brief: the co-located tests are reserved for the two invariants
   that *cannot* be reached without patch-added symbols (a wrong-width `seg:` key index, and a
   `SegmentRecord` whose chunk lengths do not sum). Both of these **are** reachable over raw
   bytes through base-visible `decode::<InodeRecord>`, so they belong in the named test target,
   which is also where the finding was reported (`segmented_map_record.rs:111`). A
   `SegmentedMap::new` unit call would also not prove the *decode* path routes through the
   constructor — the raw-byte form does.
3. **Trimming the patch to close C3's budget overage while I was in here.** Not done: the
   sign-off explicitly said to leave the non-blocking §6 items as-is this round. The number is
   restated honestly below so the human sees what my delta did to it.

## Forced refutation of my own test

**(a) Genuine red?** Yes — twice over.

- *Whole-fix revert (the project's own runner, `./engine/scripts/run-verify.sh`, i.e. the
  C4-verify gate):* `PASS — red without the fix, green with it.` GREEN leg `16 passed; 0
  failed`; RED leg (production reverted, test kept) `10 passed; 6 failed`. Both new tests are
  in the 6: their control legs assert `is_ok()` on a well-formed segmented root, which on
  `origin/main` fails as an **assertion** — `got Some(Error("invalid type: map, expected a
  sequence", line: 1, column: 22))` at `segmented_map_record.rs:213` — never a compile error,
  because the file still names nothing this patch adds.
- *Targeted guard-removal (the finding's own claim), same runner, mutated production tree in a
  throwaway scratch bundle:*
  - delete `metadata.rs:676-678` (`NoSegments`) → `test result: FAILED. 15 passed; 1 failed`,
    the one failure being `an_empty_segmented_map_is_err` at `segmented_map_record.rs:204`.
  - delete `metadata.rs:704-708` (`EmptySegment`) → `test result: FAILED. 15 passed; 1 failed`,
    the one failure being `a_zero_byte_segment_is_err` at `segmented_map_record.rs:288`.

  One guard removed ⇒ exactly one test red, and it is the matching one. That is the finding
  answered on its own terms: the guards are no longer deletable without failing the suite.
  (Before this round, neither guard had **any** covering case: `grep` of the co-located tests
  shows `SegmentedMap::new` used only at `metadata.rs:2222`, `:2344`, `:2375` — the
  unaddressable-index and span-overflow cases — and no `segments: vec![]` or `byte_len: 0`
  root anywhere.)

**(b) Production path?** Yes. Every case goes through `wyrd_core::metadata::decode` /
`encode` — the production codec at `crates/core/src/metadata.rs:1352-1359` — over raw stored
bytes; the two CAS cases commit through a real `wyrd_metadata_redb::RedbMetadataStore` with the
production `require(key, encode(prior))` precondition. No mock, no re-implementation, no
stand-in: the only thing the test hand-authors is the *stored value*, which is the subject.

**(c) Fixture includes the fault?** Yes. The malformed element is present in the decoded bytes,
not curated out: `"segments":[]` with `"segment_count":0` in the first, and
`{"index":1,"byte_offset":5,"byte_len":0}` sitting in the middle of the table in the second.
The controls are separate byte strings, so nothing about the negative fixture is softened.

## Criterion (1)'s *demonstrated* red (the brief's verification posture)

Criterion 1 (legacy byte-identity + CAS) is trivially true on the base, so the brief requires a
demonstrated red instead: with the patch applied, make `ChunkMap` serialize as a **tagged**
enum and show the legacy assertions fail. Re-run this round, through the same runner, on the
shipped tree with only `impl Serialize for ChunkMap` (`metadata.rs:863-870`) changed to
`serialize_newtype_variant(…)`:

```
legacy_flat_record_round_trips_byte_identically --- FAILED
  segmented_map_record.rs:65: decode -> encode must be the identity on a byte sequence
  origin/main already wrote, or every CAS over a pre-existing record turns into a permanent Conflict
legacy_flat_record_cas_still_commits_against_the_original_bytes --- FAILED
  segmented_map_record.rs:102: require(key, encode(prior)) must commit against a store holding
  the original bytes    left: Conflict
test result: FAILED. 11 passed; 5 failed
```

`left: Conflict` is literally the C-1 failure mode the brief names — every overwrite, backfill,
reconstruction and rebalance of every pre-existing object permanently stuck. Reverted; the
shipped tree is byte-identical to `patch.diff` (checked by regenerating the diff and comparing).

## Gates run locally before hand-off

| Check | Command (project's own) | Result |
|---|---|---|
| per-fix red→green | `PDCA_BUNDLE=… ./engine/scripts/run-verify.sh` | **PASS** — green 16/16, red 6 failing |
| formatter | `cargo fmt --all -- --check` (the first step of `xtask ci`, `xtask/src/main.rs:1512`) | clean |
| lints | `cargo clippy -p wyrd-core --all-targets` (workspace lints, warnings-as-errors) | clean |
| prose | `typos crates/core/tests/segmented_map_record.rs crates/core/src/metadata.rs` | clean |

The full `cargo xtask ci` is Check's `C4-ci` row and was green on the round-6 tree; this round's
delta is two test functions in one file, covered by the four checks above. I did not re-run the
whole gate locally to avoid burning ~20 minutes on a re-run the Check beat performs anyway.

## Honest status of the items the sign-off told me to leave alone

- **C3 budget.** My delta adds **71 lines / 36 semantic** to the test file. Region totals now:
  `crates/core/src/metadata.rs` +1836 lines (1218 non-blank/non-comment) and
  `crates/core/tests/segmented_map_record.rs` +683 (385) → **1603 semantic** in the
  shape+codec+test region, against the brief's `≤ ~1,500` (the round-6 reviewer measured it
  ~1,640 with a slightly different rule). The other 40 files are +468 lines / 408 semantic and
  are the declared mechanical migration (`.into()` at construction sites, `.as_flat()` at read
  sites), counted separately per the brief. So the overage the human already saw is unchanged
  in kind and larger by 36 lines — spent entirely on the coverage the sign-off asked for.
- **C4 `cargo deny` advisory-DB write**, **T4 review/contribution tooling visibility**,
  **staged-delivery fitness**, **PR #647 provenance** — untouched, per the sign-off.
- **C5 mutants (advisory, non-gating).** The 3 survivors the last two rounds reported are the
  equivalent `size:` deletions in `crates/custodian/src/{backfill,rebalance,reconstruction}.rs`
  that the round-6 C5 review judged equivalent (the field is immediately inherited unchanged
  through `..clone()`); nothing in this round's delta moves them, and killing an equivalent
  mutant is not possible by construction.

## Scratch discipline

All mutation demos ran in `"${PDCA_SCRATCH:-${TMPDIR:-/tmp}}/pdca-builder-648-mutdemo"` (a
scratch bundle holding only a mutated `patch.diff` + the pristine `metadata.rs` backup), reusing
the gate's own `../wyrd-verify` worktree rather than cloning the target. The directory is
removed.
