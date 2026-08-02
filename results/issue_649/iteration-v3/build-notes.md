# Build notes — issue 649 / shared-segmented-map-resolver-and-read-paths (iteration 3)

Withheld from the reviewer; for the human at sign-off.

Line citations are against `$PDCA_WORKTREE` = `/home/eddie/development/wyrd/wyrd.pdca-wt`,
whose HEAD is `6e7c255` (`pdca-integrate: issue_648`) = `origin/pdca-integration/main`, the
base `C4-verify` resolves for this wave>0 bundle (verified: both rev-parse to
`6e7c25561c33b9dbff84b439a720d9e44e536f48`).

Iterations 1 and 2 are preserved in `iteration-v1/` / `iteration-v2/`. **This round is a
delta on iteration 2's patch**, which was green on every gate except the batched rubric
review (`iteration-v2/check-gates.md:14-16,25` — C4-ci pass, C4-verify pass, C5 0 missed,
T4 fail with 4 blocking findings). So the work here is exactly: settle those four findings,
and prove the settlement red→green. §2 is the delta; §§3-7 restate what the whole patch is
and answer the three refutation questions for it, since the reviewer sees neither this file
nor the earlier ones.

---

## 1. The four carry-forward findings, one line each

| Finding (`review-batch.md`) | Disposition |
|---|---|
| `metadata.rs:2170` **BUG** — an oversized snapshot is rejected before rechecking whether its root was superseded, so a concurrent overwrite to a valid generation returns `TooManySegments` instead of restarting | **FIXED** (§2.1) |
| `segmented_map_resolution.rs:534` **TEST-GAP** — every fixture fits one page, so the continuation-cursor path is never exercised | **FIXED** (§2.2) |
| `metadata.rs:2166` **CONVENTION** — the range-read await has no timeout | **RECORDED-REJECTED** (§2.3, `review-rejected.md`) |
| `metadata.rs:2182` **CONVENTION** — `scan_page` and the root re-reads await without deadlines | **RECORDED-REJECTED** (§2.3, `review-rejected.md`) |

## 2. What each got

### 2.1 The ceiling is now *described*, not judged — the BUG finding

The finding is real and it is a C-1 shape: a reader holding a snapshot whose table is past
the resolve ceiling (a torn or truncated root, or one written by a build with a larger
ceiling) got a hard `TooManySegments` **even when the store had already replaced that root
with a perfectly resolvable generation**. A live object made permanently unreadable by
bytes nothing points at any more is precisely the failure mode the slice's invariant
forbids.

The fix keeps the bound exactly where the brief puts it and moves only the *verdict*:

- `crates/core/src/metadata.rs:2185-2192` — the ceiling check still runs **before the first
  page is asked for**, but now returns `Ok(GroupRange::Anomaly(TooManySegments))` instead
  of `Err`.
- `crates/core/src/metadata.rs:2271` — `read_segments` settles it through the one
  resolve-retry arbiter `retired_or` (`:2113-2130`), exactly like the five anomaly shapes
  already routed there. Root still names this generation ⇒ `Err(TooManySegments)`, fail
  closed, scoped to the object. Root moved on or gone ⇒ `MapResolution::Retired` ⇒ the
  caller restarts onto the live root.
- The re-read this costs is one **root** `get` (`:2090`); no `seg:` row is read on either
  arm, so "refused before any range read is performed" (criterion 2) is untouched — and the
  test asserts it on both arms (`segmented_map_resolution.rs:597-600`, `:637-640`).
- Docs kept true: `:2120-2123` (the arbiter's own comment), `:2104-2110` (its doc),
  `:2162-2170` (`read_group_range`'s ceiling paragraph), `:2134-2144` (`GroupRange::Anomaly`), `:473-491` (the `TooManySegments` variant now says "while
  the root still names that generation", matching `SegmentAbsent`'s wording at `:501-512`),
  and the living-architecture sentence
  (`docs/design/architecture/06-runtime-view.md:29`).

**Red→green, measured**: with only the four-line verdict reverted to iteration 2's `Err`,
`an_over_ceiling_root_superseded_mid_read_restarts_onto_the_live_generation`
(`crates/core/tests/segmented_map_resolution.rs:620-641`) fails with
`called Result::unwrap() on an Err value: TooManySegments { segments: 513 }`; restored, 17/17
pass.

*Alternative rejected — keep `Err` and have the CALLERS retry on `TooManySegments`.*
Concretely: a `match` on the downcast at each of the three entries plus the two gateway
callers, i.e. 5 sites × ~8 lines ≈ 40 lines, and a second place that answers "retired, or
corrupt?" — the exact duplication the slice's "resolution is single-sourced" invariant
exists to prevent, and one that would drift the moment a sixth consumer lands in #650/#651.
The shipped fix is **net −1 semantic line** in production (measured: `metadata.rs` 267 →
266 added semantic lines).

*Alternative rejected — refuse the ceiling in `read_segments` before calling
`read_group_range`.* Same behaviour, ~4 lines either way, but it moves the budget check out
of the function that spends the budget; a later caller of `read_group_range` (the #651
maintenance paths are the obvious ones) would then be unbounded by construction. Keeping
the check at the spend site and only the *verdict* at the arbiter costs nothing and cannot
be lost that way.

### 2.2 The continuation cursor is now exercised — the TEST-GAP finding

`scan_page` explicitly permits a store to hand back fewer rows than asked for
(`items.len() <= min(limit, the store's effective cap)`, `crates/traits/src/lib.rs:1069-1071`),
so the resolver's `accounted + 1` limit is an upper bound a real backend need not honour.
Every fixture in iteration 2 fitted one redb page, so the walk's cursor arm was dead code
under test.

- `Quirk::PageCap(usize)` (`crates/core/tests/segmented_map_resolution.rs:305-310`,
  `:444-457`) makes the double a **conforming** store with a small page cap: it truncates
  each `seg:` page and derives the contract's cursor, `Some(last key returned)` (clause 3,
  `crates/traits/src/lib.rs:1054-1056`). Nothing else about the page changes.
- The exact-ceiling fixture — the one the finding named — now resolves through it at 200
  rows/page, so 512 segments is a three-page walk, and the test asserts the **exact** cursor
  sequence `[None, seg_key(199), seg_key(399)]` as well as the byte-identical read
  (`:643-688`; the fixture at `:658-688`).
- `PAGE_WALK_GUARD` (`:313-316`, applied at `:445-451`) makes a resolver that dropped the cursor fail **loudly**
  instead of hanging: the double refuses after 16 pages over one group.

**Red→green, measured** (production mutated, test unchanged):

| Mutation of `read_group_range` | Result |
|---|---|
| `after = Some(cursor)` → `after = None` (cursor dropped) | FAIL — `"17 pages over one group's range: the walk is not advancing on the cursor it was handed"` |
| return after the first page (cursor arm deleted) | FAIL — `SegmentAbsent { index: 200 }` |
| unmutated | 17/17 pass |

*Alternative rejected — a second, small multi-page test instead of re-using the ceiling
fixture.* ~25 more semantic lines for a strictly weaker fixture (5 segments vs 512), on a
patch already over budget (§5), and it would have left the finding's literal subject — "even
the 512-segment boundary resolves in one page" — untouched.

### 2.3 The two await-discipline findings are recorded-rejected, not silenced

Full reasoning and the gate-format rows are in **`review-rejected.md`** (14 rows: the two
reported lines plus the lines those same awaits occupy in this patch, since the class binds
to the code and the line moves between iterations; class `CONVENTION` throughout, so a
genuine BUG at any of those lines is still blocking). In short:

1. The `MetadataStore` contract makes termination the **backend's** obligation — *"a backend
   must bound its own waiting rather than block a caller forever on an unreachable
   cluster"* (`crates/traits/src/lib.rs:1000-1012`) — and both networked drivers impose
   their own deadline (`crates/metadata-fdb/src/lib.rs:78-89`,
   `crates/metadata-tikv/src/lib.rs:143-172`, #517).
2. It is **not expressible in this crate**: `wyrd-core` has no runtime dependency at all
   (`crates/core/Cargo.toml:11-15` — "no tokio, no executor … ADR-0009"; tokio is
   dev-only), and its async is driven in-tree by `pollster::block_on`
   (`crates/core/tests/placement_record.rs:140`,`:185`; 39 sites in the crate). Adding
   `tokio::time::timeout` means adding a runtime dependency to an executor-free crate and
   tying `read::read_object` to a tokio reactor.
3. No caller in `crates/core/src`, `crates/custodian/src` or the gateway wraps a metadata
   await; bounding only the six awaits this patch adds would create a second convention and
   leave the class untouched everywhere else (`metadata.rs:1612`, `:1650` are the same
   shape).
4. It is the brief's standing rejection (i), rejected 3× across #508/#636: the store
   implementation owns the network bound, not the caller.

What the patch *did* do about it is make the convention visible at the site rather than
leave the next reviewer to re-derive it: a `WHERE THE TIME BOUND LIVES` note at the
resolution section header (`crates/core/src/metadata.rs:2051-2060`), which also states what
this caller **does** bound — the *work*: one root read plus one group range, paged to the
root's own claim, refused above the ceiling unread.

**For the human at sign-off:** if you disagree, the change is not small and not local —
it is a `tokio` dependency in `wyrd-core` plus a deadline policy the caller has no basis to
pick, and it belongs in a cycle of its own across all metadata callers, not in this slice.

---

## 3. What the whole patch is (unchanged from iteration 2 except §2)

- `crates/core/src/metadata.rs` — `MapResolution` (`:2073`), `root_still_names` (`:2085`),
  the single resolve-retry arbiter `retired_or` (`:2113`), the described-anomaly
  `GroupRange` (`:2131`), the bounded paged range read `read_group_range` (`:2180`; ceiling
  at `:2185`), `decode_segment_record` (`:2244`), `read_segments` (`:2263`), and the three
  entries `resolve_chunk_map` (`:2328`) / `resolve_current_chunk_map` (`:2371`) /
  `resolve_live_chunk_map` (`:2418`), plus the `ChunkMapError` variants they raise.
- `crates/custodian/src/resolve.rs` (new) + `crates/custodian/src/lib.rs` — `chunks_of`
  (`:43`, always against the live root) and `classify_root` (`:98`, the containment arm
  every `scan("inode:")` loop needs), with 5 unit tests. Its pass consumers are #650/#651.
- `crates/core/src/read.rs` — `read_object_from` → `read_object_chunks(chunks, map, size)`
  (`:69`); `read_object` resolves first (`:513-515`); `committed_inode` returns
  `(InodeId, InodeRecord)` so the gateway has the root key a retry needs.
- `crates/server/src/lib.rs` — `get_object_streaming` (`:344`) and `get_object_range`
  (`:430`) resolve through `metadata::resolve_live_chunk_map` and frame the response from
  the generation the bytes came from (`served`).
- `crates/dst/tests/custodian.rs` — Tier-0 property 9, `segmented_resolve_never_tears`
  (shipped and run by `cargo xtask ci`, per the brief's verification posture; not a Check
  discriminator).
- `docs/design/architecture/06-runtime-view.md:29` — the resolver paragraph only, now also
  stating that a table past the reader's ceiling is refused **unread**.
- The two added test targets (17 + 4 tests) — the C4-verify discriminators.
- Declared mechanical migration: 10 files whose only change is the `read_object_chunks`
  callsite form (205 semantic lines, allowed on top of the budget).
- `Cargo.lock` (2 lines) — `event-listener` 5.4.1 → 5.4.2, the upstream remediation for
  RUSTSEC-2026-0221. **The one hunk outside the brief's declared scope**, kept from
  iteration 2: the advisory is red on the unmodified base too, but C4-ci is gating and
  `cargo deny` is part of it. Dropping that hunk leaves the fix intact and C4-ci red.
  Flagged again here so it is a decision at sign-off, not a surprise.

## 4. Standing design choices (restated; the reviewer sees no earlier file)

- **Scope cut vs `sources/salvage.diff`**: #651's `ChunkHome`/`repoint_chunk` and #650's
  five-pass containment carry no caller in this slice (salvage's `resolve.rs` alone is 367
  lines against this one's 120 production + 170 test).
- **`scan_page`, not `scan`**: `scan` is complete-or-fail-loud at `SCAN_CAP`
  (`crates/traits/src/lib.rs:953-961`), so a damaged range would cost the caller the whole
  call — one damaged object ending a fleet-wide pass.
- **Raw `seg:` seeding in every fixture, never a committer** — the brief's verbatim rework
  note; this slice ships no producer.
- **Nothing this patch adds is imported by either test file** — criteria (2)-(3) are
  observed through `read_object`/`read_path`/`ObjectGateway` plus the instrumented double.
- **One double, five quirks** (this round added `PageCap` and `SupersedeAfterRootRead`):
  every test runs against a store that records its whole access footprint, so a channel the
  oracle does not watch cannot appear unnoticed.

## 5. Budget — over the brief's figure, declared again, with the delta

Measured on the two patches with one identical counter (added lines, non-blank,
non-comment; the declared mechanical-migration files excluded):

| | non-mechanical | mechanical | files |
|---|---|---|---|
| iteration 2 | 1 581 | 205 | 20 (10 non-mechanical) |
| **this patch** | **1 661** | 205 | 20 (10 non-mechanical) |

Against the brief's `≤ ~1,500` and `≤ 15 files` (files: 10 non-mechanical ✓). The +80 is
**entirely test body** — `segmented_map_resolution.rs` 634 → 715 — and production went
*down* by one line (`metadata.rs` 267 → 266). Production **proper** across the whole patch is ~373 semantic
lines (`metadata.rs` 266, `read.rs` 27, `resolve.rs`'s non-test half 50, the gateway 29,
`custodian/lib.rs` 1) against the brief's own ~660 estimate — the other ~1 288 are test
bodies (`segmented_map_resolution.rs` 715, `segmented_object_read.rs` 267, the DST property
152, `resolve.rs`'s unit module 151, `Cargo.lock`/docs 3). The overshoot has always been
test bodies, and every line of this round's is one of the two Check findings above.

I did **not** stop and hand back a split, and that judgement is the human's to overturn:
the brief's split fallback (*read paths* out of *resolver*) leaves the resolver slice
carrying `metadata.rs` (266) + `resolve.rs` (201) + the core test file (715) + the DST
property (152) ≈ 1 334 by itself and duplicates fixtures across two test targets, so the
two slices' combined count is **higher**, not lower — and the split would discard two
completed Check rounds to re-earn the same tests. If you want the split anyway, the seam is
clean: `crates/server/src/lib.rs` + `crates/server/tests/segmented_object_read.rs` (296
lines) lift out whole.

## 6. The three refutation questions

**(a) Genuine red?** Yes — measured by the project's own gate, not inferred.
`PDCA_VERIFY_BASE=origin/pdca-integration/main ./engine/scripts/run-verify.sh` (the
C4-verify gate, on the final `patch.diff` in this bundle) reports **PASS — red without the
fix, green with it**: on the reverted tree both files still *compile* (they import only
base-visible #648 symbols) and **all 17** core tests fail as assertions
(`SegmentedMapUnsupported { operation: "read_object_collecting" }`), 0 passed — no
compile-red scored as pass, no vacuous `0 tests … ok`. `--classify` on this file set shows
exactly the two `ADDED_TEST` discriminators and no cfg gate.

Per-defect red for **this round's** two changes was measured the same way, by mutating one
thing at a time and re-running (each restored from a scratch copy afterwards):

| Mutation | Test that goes red |
|---|---|
| ceiling verdict `Ok(Anomaly)` → `Err` (iteration 2's ordering) | `an_over_ceiling_root_superseded_mid_read_restarts_onto_the_live_generation` — `Err(TooManySegments { segments: 513 })` |
| `after = Some(cursor)` → `after = None` | `a_root_at_exactly_the_segment_ceiling_resolves_across_paged_reads` — the 17-page guard fires |
| cursor arm deleted (stop after page 1) | same test — `SegmentAbsent { index: 200 }` |

(Iteration 2's own per-defect red legs — ceiling `>`→`>=`, the key check `||`→`&&`, the
extent check `||`→`&&`, the malformed-key propagation, and `retired_or` forced to fail
closed taking the DST property red — are recorded in `iteration-v2/build-notes.md` §5 and
are unchanged by this round.)

**(b) Production path?** Yes. Both test files drive `wyrd_core::read::{read_object,
read_path}` and `wyrd_gateway_core::ObjectGateway::{get_object_streaming, get_object_range}`
over a real `RedbMetadataStore` + `FsChunkStore`; the resolver under test is always
`wyrd_core::metadata::resolve_*`, never a copy. The `Probe` double **wraps** that real
backend and forwards every call unchanged except the one quirk each test names — including
the new `PageCap`, which truncates a real redb page and derives the contract's own cursor
rather than synthesising rows. The custodian wrapper's tests call `chunks_of`/`classify_root`
directly.

**(c) Fixture includes the fault?** Yes — every anomaly is a real, present condition of the
store the resolver reads, never curated out: the over-ceiling root genuinely decodes and is
genuinely in the store when the read starts (and in the new test it is genuinely superseded
mid-read, by a commit the double applies the instant after the reader's own root read
returns); the absent segment is never written; the undecodable bytes are genuine garbage;
the malformed key is genuinely stored under the group's range; the extent disagreements are
real byte values; the exact-ceiling object has all 512 records genuinely present and is read
through a store that genuinely serves them 200 at a time. The bounded-access oracle asserts
over the **whole** recorded footprint (gets + scans + pages), not a filtered view.

## 7. Gate results measured in this Do beat (on the final artifact)

| Gate | Iteration 2 | This round |
|---|---|---|
| **C4-ci** `./engine/xtask.sh ci` (gating) | pass | **PASS, exit 0** — "xtask ci: all checks passed" (fmt, clippy `-D warnings`, build, `cargo test --workspace`, the 50-seed `--cfg madsim` DST sweep, three cargo-deny legs, conformance vectors, statics/unsafe/deploy/gitlink guards, `typos`, docs render). Run **twice**, the second time on the frozen final tree with no edit in flight (`sha1 940ff57…` on the core test file, which is byte-identical to the bundle copy) |
| **C4-verify** per-fix red→green | pass | **PASS** — "red without the fix, green with it"; red leg 0 passed / 17 failed. Re-run against the **final** `patch.diff` byte-for-byte (the artifact in this bundle): PASS both times |
| **C5-mutants** (advisory) | 0 missed | see below |
| **T4-batch-review** (gating) | 4 blocking | 2 fixed in the patch, 2 recorded in `review-rejected.md` (the gate re-samples at Check) |

Other runs: `cargo test -p wyrd-core --test segmented_map_resolution` 17 passed;
`cargo test -p wyrd-server --test segmented_object_read` 4 passed;
`cargo test -p wyrd-core -p wyrd-custodian -p wyrd-server` all green (no regressions from
the `read_object_chunks` / `committed_inode` shapes); `cargo fmt --all` applied and
`--check` clean; `cargo clippy --workspace --all-targets` clean — all inside `xtask ci`'s
own legs as well.

`scripts/mutants-in-diff` (C5, advisory): **56 mutants, 14 caught, 42 unviable, 0 missed**
(same population as iteration 2 — this round's production delta is one changed return
expression, and both of its mutants are killed by the two red legs in §6).

Scratch: everything throwaway lived under
`${PDCA_SCRATCH}/pdca-builder-649-redleg/` and is removed at the end of the beat.

## 8. STOP discipline

No push, no branch, no PR. The patch, the two test files, `review-rejected.md` and this
file are the whole output.
