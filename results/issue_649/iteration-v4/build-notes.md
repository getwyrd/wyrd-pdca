# Build notes — issue 649 / shared-segmented-map-resolver-and-read-paths (iteration 4)

Withheld from the reviewer; for the human at sign-off.

Line citations are against `$PDCA_WORKTREE` = `/home/eddie/development/wyrd/wyrd.pdca-wt`,
whose HEAD is `6e7c255` (`pdca-integrate: issue_648`) = `origin/pdca-integration/main`, the
base `C4-verify` resolves for this wave>0 bundle.

Iterations 1–3 are preserved in `iteration-v1/` … `iteration-v3/`. **This round is a delta
on iteration 3's patch**, which was green on every gate — `C4-ci` pass, `C4-verify` pass,
`C5` 0 missed, `T4-batch-review` **0 blocking**, `T4-contribution` pass
(`iteration-v3/check-gates.md:14-16,25-26`). What came back was a single
implementation-level review finding plus two the reviewer routed to the human. §1 is the
disposition, §2 the change and its measurement; §§3-7 restate the whole patch and answer
the three refutation questions, since the reviewer sees neither this file nor the earlier
ones.

---

## 1. The carry-forward, one line each

| Carry-forward item (`iteration-v3/check-review.md`) | Disposition |
|---|---|
| **T5 Judgment `[impl]`** — "Rebuild must assert that every recorded metadata `get` is the target inode key — the bounded-footprint oracle only forbids direct `seg:` gets at `crates/core/tests/segmented_map_resolution.rs:546`, so unrelated metadata reads would still pass" | **FIXED** (§2) |
| **C3 Change** `NEEDS-HUMAN` — the lockfile-only `event-listener` 5.4.1→5.4.2 hunk | **Still the human's call** (§2.3) — it is deferred in `deferred-findings.json`, and dropping it turns the gating `C4-ci` red |
| **T4 Contribution** `NEEDS-HUMAN` — `scripts/review-branch` / `scripts/pdca` absent from the reviewer's sandbox, so those wrappers could not be re-run | **Not actionable from the target tree** — they are PDCA-harness wrappers, not Wyrd files; nothing in `patch.diff` can change what the reviewer's sandbox contains |

## 2. The oracle's `get` channel is now a whitelist, not a blacklist

### 2.1 What was wrong

The finding is a real hole, and it is the third round of the *same* class getting
progressively closed (iteration 2 watched only `scan_page` prefixes; iteration 3 added a
`get` check but spelled it as a **blacklist** — "no key may start with `seg:`"). A
blacklist bounds one namespace. Criterion (2) is a **budget**: "reading one object touches
the root **plus only** the range `seg:<nonce>:<epoch>:`". Under the blacklist a resolver
that consulted a sibling object's root, a directory entry, an index key — anything outside
`seg:` — spent work no one was watching and the oracle still went green.

### 2.2 What it is now

Two assertions, both positive, in one helper on the existing `Probe` double:

- `crates/core/tests/segmented_map_resolution.rs:388` — `assert_gets_only_the_root_of`:
  **every** recorded `get` key must equal `inode_key(inode_id)`, the object's own root, and
  there must be at least one. It returns the count.
- `crates/core/tests/segmented_map_resolution.rs:324` — `CLEAN_RESOLVE_ROOT_GETS = 2`: the
  root read the reader itself takes plus the ONE re-read that settles whether the generation
  just resolved is still live (`crates/core/src/metadata.rs:2313`). Pinned at
  `:587-589` (two segments, one page) **and** `:735-737` (512 segments, three pages), so the
  claim asserted is "the root-read count is the reader's constant, **independent of the
  root's table**" — the same C-1 clause the ceiling check serves ("the work a record can
  demand of a reader is bounded by the reader, not the record"), on the `get` channel.

Applied at **every** site that owns a `Probe`, not only the two the finding named — the
class binds to the code, and a bound that holds on the happy path but not on the retry arms
is not a bound: `:587` (bounded read), `:640` (over-ceiling refusal), `:681` (retired
over-ceiling restart), `:735` (exact ceiling, paged), `:935`, `:953`, `:979`, `:1013`
(the four supersede arms), `:1057` (the smuggled foreign-epoch row), `:1081` (reversed
page order). Two negative loops were deleted in the process (`:546`, `:601` on iteration
3's file), and the over-ceiling refusal also gained the missing "no unpaged `scan` either"
assertion (`:641-644`).

Production is **byte-identical to iteration 3** — `crates/core/src/metadata.rs`,
`read.rs`, `resolve.rs`, `custodian/lib.rs`, `server/lib.rs`, the DST property and the docs
paragraph are untouched this round. The finding was that the oracle could not *see* an
out-of-budget read, not that one happened; the whitelist now proves the real resolver takes
none.

### 2.3 Red→green, measured — and measured against the OLD oracle too

A strengthened assertion is only worth its lines if something it now catches used to pass.
So each mutation was run **twice**: once against this round's file, once against iteration
3's shipped test file byte-for-byte (the bundle-root copy the driver preserved from round 3,
staged in scratch as `test.old` before I overwrote it). Production restored from a scratch
copy after each.

| Mutation of production (`crates/core/src/metadata.rs`) | iteration 3's oracle | **this round's oracle** |
|---|---|---|
| **A — one unrelated metadata key**: `let _ = store.get(&inode_key(9_999)).await?;` at the head of `read_group_range` (`:2193`) | **17/17 pass — blind** | **8 tests FAIL**: `the ONLY metadata key a resolve may fetch is the object's own root (inode 1); this read also fetched inode:9999` |
| **B — root re-read per named segment**: `root_still_names(...)` inside `read_segments`' `for segment in map.segments()` loop (`:2283`) | **17/17 pass — blind** | **3 tests FAIL**: `the root-read count is the reader's constant, not a function of the table`, **left: 514, right: 2** (and 4 vs 2 on the two-segment fixture) |
| unmutated | 17/17 pass | **17/17 pass** |

Mutation B is the one the count constant exists for: every key it fetches *is* the object's
own root, so the whitelist alone would have waved it through while the read spent 512 extra
round trips at the root's own instruction — the record setting the reader's budget.

### 2.4 Alternatives rejected, with their costs

- **Enumerate more forbidden prefixes** (add `inode:`-other-than-mine, `dir:`, … to the
  existing blacklist). Cost: ~4 lines per namespace per test × 10 Probe sites ≈ 40+ lines,
  and it *never closes* — the hole reopens the day a new key namespace lands (`plc:`,
  `lease:` …) with nothing to fail. The whitelist is 20 lines once and is closed by
  construction.
- **Assert the exact `get` sequence** (`assert_eq!(gets, vec![inode_key(1); 2])`) at every
  site. Cost: 10 literals, six of which are 3 or 4 depending on which retry arm the fixture
  takes — a restatement of the resolver's internal call order, which is not the property; it
  would go red on any refactor that changes *when* the root is re-read while still keeping
  it bounded. The count is therefore pinned only where it is a genuine constant (the two
  clean resolves), and the whitelist carries the rest.
- **Make the `Probe` panic inside `get` on a non-root key** instead of asserting after the
  fact. Cost: about the same 6 lines, but it fires *inside* the resolver's own async call —
  the panic (or the `Err` a store double would more naturally return) is then something the
  resolver can convert into a typed anomaly, so a "fails closed" test could go green **for
  the wrong reason**. Recording and asserting on the footprint afterwards is what this file
  already does for the `scan` and `scan_page` channels; the `get` channel now matches.
- **Change production** so the extra root re-read is removed and the constant becomes 1.
  Rejected as out of scope and wrong: that re-read (`crates/core/src/metadata.rs:2306-2315`)
  is what makes a *complete* segmented resolution non-stale, i.e. the resolve-retry rule's
  own arm. The finding was about the oracle, and production needed no change to satisfy it.

## 3. What the whole patch is (unchanged from iteration 3 except §2)

- `crates/core/src/metadata.rs` — `MapResolution` (`:2073`), `root_still_names` (`:2085`),
  the single resolve-retry arbiter `retired_or` (`:2113`), the described-anomaly
  `GroupRange` (`:2131`), the bounded paged range read `read_group_range` (`:2180`; ceiling
  refused *unread* at `:2185`), `decode_segment_record` (`:2244`), `read_segments` (`:2263`),
  and the three entries `resolve_chunk_map` (`:2328`) / `resolve_current_chunk_map` (`:2371`)
  / `resolve_live_chunk_map` (`:2418`), plus the `ChunkMapError` variants they raise.
- `crates/custodian/src/resolve.rs` (new) + `crates/custodian/src/lib.rs` — `chunks_of`
  (`:43`, always against the live root) and `classify_root` (`:98`, the containment arm every
  `scan("inode:")` loop needs), with 5 unit tests. Its pass consumers are #650/#651.
- `crates/core/src/read.rs` — `read_object_from` → `read_object_chunks(chunks, map, size)`
  (`:69`); `read_object` resolves first (`:513-515`); `committed_inode` returns
  `(InodeId, InodeRecord)` so the gateway has the root key a retry needs.
- `crates/server/src/lib.rs` — `get_object_streaming` (`:344`) and `get_object_range`
  (`:430`) resolve through `metadata::resolve_live_chunk_map` and frame the response from
  the generation the bytes came from (`served`).
- `crates/dst/tests/custodian.rs` — Tier-0 property 9, `segmented_resolve_never_tears`
  (shipped and run by `cargo xtask ci`, per the brief's verification posture; not a Check
  discriminator).
- `docs/design/architecture/06-runtime-view.md:29` — the resolver paragraph only.
- The two added test targets (17 + 4 tests) — the C4-verify discriminators.
- Declared mechanical migration: 10 files whose only change is the `read_object_chunks`
  callsite form (205 semantic lines, allowed on top of the budget).
- `Cargo.lock` (2 lines) — `event-listener` 5.4.1 → 5.4.2, the upstream remediation for
  RUSTSEC-2026-0221. **The one hunk outside the brief's declared scope.** The advisory is
  red on the unmodified base too, but `cargo deny` is a leg of the gating `C4-ci`, so
  dropping the hunk leaves the fix intact and the gate red. This is the reviewer's C3
  `NEEDS-HUMAN`, carried into §6 by `deferred-findings.json` — flagged here so it is a
  decision at sign-off, not a surprise.

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
- **One double, five quirks**: every test runs against a store that records its whole access
  footprint — `gets`, `scans`, `pages` — and, as of this round, all three channels are
  asserted **positively**, so a channel the oracle does not watch cannot appear unnoticed.
- **The two await-discipline findings stay recorded-rejected** in `review-rejected.md`
  (14 rows, class `CONVENTION`): termination is the `MetadataStore` backend's contractual
  obligation (`crates/traits/src/lib.rs:1000-1012`), `wyrd-core` is executor-free by design
  (`crates/core/Cargo.toml:11-15`, ADR-0009), and it is the brief's standing rejection (i),
  rejected 3× across #508/#636. Those rows cite `metadata.rs` lines that this round does not
  move.

## 5. Budget — over the brief's figure, declared again, with the delta

Measured on iteration 3's and this round's `patch.diff` with **one** counter (added lines,
non-blank, non-comment; the 10 declared mechanical-migration files excluded):

| | non-mechanical | mechanical | files |
|---|---|---|---|
| iteration 3 | 1 699 | 205 | 20 (10 non-mechanical) |
| **this patch** | **1 730** | 205 | 20 (10 non-mechanical) |

(Iteration 3's own build-notes reported 1 661 with a slightly different counter; the table
above re-counts both files with the same script, so the **+31** is like-for-like.) The
whole delta is in `crates/core/tests/segmented_map_resolution.rs` (735 → 766); **production
is byte-identical to iteration 3.** Against the brief's `≤ ~1,500` and `≤ 15 files`
(files: 10 non-mechanical ✓). Production proper across the whole patch is ~373 semantic
lines against the brief's own ~660 estimate; the overshoot has always been test bodies, and
this round's 31 lines are the Check finding above.

I did **not** stop and hand back a split, and that judgement remains the human's to
overturn: the brief's fallback split (*read paths* out of *resolver*) leaves the resolver
slice carrying `metadata.rs` (269) + `resolve.rs` (209) + the core test file (766) + the
DST property (153) ≈ 1 397 by itself and duplicates fixtures across two test targets, so
the two slices' combined count is **higher**, not lower — and the split would discard four
completed Check rounds to re-earn the same tests. If you want the split anyway, the seam is
clean: `crates/server/src/lib.rs` + `crates/server/tests/segmented_object_read.rs` (302
lines) lift out whole.

## 6. The three refutation questions

**(a) Genuine red?** Yes, at two levels, both measured rather than argued.

1. *The bundle's own discriminator*: `PDCA_VERIFY_BASE=origin/pdca-integration/main
   PDCA_BUNDLE=… ./engine/scripts/run-verify.sh` — the C4-verify gate, run on the **final**
   `patch.diff` in this bundle — reports **PASS: red without the fix, green with it**. On the
   reverted tree both added files still *compile* (they import only base-visible #648
   symbols) and the core target fails **17/17 by assertion**
   (`SegmentedMapUnsupported { operation: … }`), 0 passed — no compile-red scored as a pass,
   no vacuous `0 tests … ok`.
2. *This round's own delta*: §2.3's two mutations, each of which iteration 3's file passed
   **17/17 while blind** and this round's file fails loudly (8 and 3 tests respectively).
   That is the specific red the T5 finding asked for.

**(b) Production path?** Yes. Both test files drive `wyrd_core::read::{read_object,
read_path}` and `wyrd_gateway_core::ObjectGateway::{get_object_streaming, get_object_range}`
over a real `RedbMetadataStore` + `FsChunkStore`; the resolver under test is always
`wyrd_core::metadata::resolve_*`, never a copy. The `Probe` **wraps** that real backend and
forwards every call unchanged except the one quirk each test names. The new whitelist reads
the footprint of that real read — the keys the production resolver actually asked redb for.

**(c) Fixture includes the fault?** Yes, and this round widens exactly that: the bounded
fixture keeps **both** decoy groups seeded and present (a second nonce, and the same nonce
one epoch back, `crates/core/tests/segmented_map_resolution.rs:536-542`) rather than curating
them out, and the oracle now proves nothing
outside the object's own root and its own group's range was read *by any channel*, instead
of proving only that no `seg:`-prefixed key was `get`. Every other anomaly remains a real,
present condition of the store the resolver reads: the over-ceiling root genuinely decodes
and is genuinely in the store (and genuinely superseded mid-read in `:658`), the absent
segment is never written, the undecodable bytes are genuine garbage, the malformed key is
genuinely stored under the group's range, the extent disagreements are real byte values, and
the exact-ceiling object has all 512 records present, served 200 at a time.

## 7. Gate results measured in this Do beat (on the final artifact)

Every number below was produced by the project's own runners
(`./engine/xtask.sh ci`, `./engine/scripts/run-verify.sh`, `scripts/mutants-in-diff`), on the
exact `patch.diff` / test files in this bundle (`sha1 b08556e5…` on the core test file,
byte-identical in the worktree and the bundle).

| Gate | Iteration 3 | This round |
|---|---|---|
| **C4-ci** `./engine/xtask.sh ci` (gating) | pass | **PASS, exit 0** — "xtask ci: all checks passed": `typos`, `lint_docs.py`, `render_site.py --check`, gitlink/unsafe guards, `fmt --check`, `clippy --all-targets`, build, `cargo test --workspace`, `cargo-machete`, three `cargo deny` legs, statics + deploy guards, and the `--cfg madsim` DST run — in which **`segmented_resolve_never_tears` passes** (this patch's Tier-0 property, the brief's declared non-discriminator) |
| **C4-verify** per-fix red→green | pass | **PASS** — "red without the fix, green with it": red leg **0 passed / 17 failed**, every failure an assertion (`… this build cannot yet resolve a segmented map`), green leg **17/17 + 4/4**. `--classify` on this patch prints exactly two `ADDED_TEST` rows and no cfg gate |
| **C5-mutants** (advisory) | 0 missed | **56 mutants, 14 caught, 42 unviable, 0 missed** |

Targeted runs behind those: `cargo test -p wyrd-core --test segmented_map_resolution` 17/17;
`cargo test -p wyrd-server --test segmented_object_read` 4/4; `cargo fmt --all` applied and
`--check` clean, so the patch is commit-ready for the target's own hooks.

**One thing for the human, unrelated to this patch.** The *first* `xtask ci` run of this beat
hung — not failed — inside `crates/server/tests/custodian_gc.rs` (7 of its 10 tests stuck; the
process sat at **0 s of CPU across 23 minutes**, all threads in `futex_wait`, load average
0.08). I killed it and re-ran: the second run is the green one above, and the same target
passes standalone in 0.16 s in parallel and 0.60 s at `--test-threads=1`. Nothing in this
round's delta is in that crate (the delta is test-only, in `wyrd-core`'s test target), and
iteration 3 ran the same production code through `xtask ci` twice without it. So it reads as a
latent parallel-execution flake in that suite (its tests each build a
`DurabilityTelemetry::new(ExporterConfig::Prometheus)`), worth an issue of its own — it will
bite an unattended cycle again, as a gate that never returns rather than one that fails.

Scratch: everything throwaway lived under
`${PDCA_SCRATCH}/pdca-builder-649-redleg/` and is removed at the end of the beat.

## 8. STOP discipline

No push, no branch, no PR. The patch, the two test files, `review-rejected.md` and this
file are the whole output.
</content>
</invoke>
