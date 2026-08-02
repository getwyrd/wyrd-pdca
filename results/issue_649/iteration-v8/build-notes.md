# Build notes — issue 649 (slice 2/6 of #635), iteration 8

Target branch: `getwyrd/wyrd @ main`; build/verify base = the wave fold
`origin/pdca-integration/main` = **6e7c255** (carries #648). Every `path:line` below is
against that base unless it names a file this patch adds.

---

## 1. What the patch does, and why this shape

The defect: after #648 an `InodeRecord.chunk_map` can be `ChunkMap::Segmented`, and
**nothing can resolve one**. All three readers took the inline list off the record and
failed closed — `crates/core/src/read.rs:96-97`, `crates/server/src/lib.rs:364-365` and
`:459-460`. There was no shared resolution call anywhere in `crates/`.

The patch adds **one** resolver and routes the three in-slice readers through it.

### `crates/core/src/metadata.rs` (+271 semantic)

* **Two public entries, one result type** (the brief caps this at two; iteration 7 shipped
  three entries and three result types, a named cause of the review surface):
  - `resolve_chunk_map(store, root_key, &record)` — resolve a caller's own committed
    snapshot; if that generation was retired under the read, it **restarts** onto the live
    root itself (`crates/core/src/metadata.rs:2422-2434`).
  - `resolve_current_chunk_map(store, root_key)` — read the live root and resolve it,
    bounded by `MAX_RESOLVE_RESTARTS` (`:2446-2469`). This is the restart the first entry
    takes, and the entry #650/#651 will use from a stale scan snapshot.
  - Both answer `ResolvedChunkMap { record: Cow<InodeRecord>, chunks: Cow<[ChunkRef]> }`
    (`:2141-2152`). One type, not two, and both fields are `Cow` so the ordinary case — a
    flat map on a still-live snapshot — copies nothing and costs **zero** extra store
    calls. Carrying the *record* is not decoration: a restart answers chunks the caller's
    snapshot does not describe, so framing (`size`, `etag`, `modified`) must come from the
    generation the bytes came from. That is what makes the gateway's `206`/`Content-Length`
    coherent across an overwrite.
* **Internals** (all private): `root_still_names` (`:2157-2167`), the one arbiter
  `retired_or` (`:2183-2198`), `read_group_range` (`:2242-2308`), `decode_segment_record`
  (`:2314-2325`), `read_segments` (`:2331-2379`), `resolve_snapshot` (`:2384-2404`).
* **Seven new `ChunkMapError` variants** (`:473-570`) — one per anomaly the criteria name:
  `TooManySegments`, `SegmentUnknown`, `SegmentAbsent`, `SegmentBoundsMismatch`,
  `SegmentValueOverCeiling`, `SegmentRecordUndecodable`, `MapResolutionUnstable`.

**Why every anomaly goes through one arbiter.** `read_group_range` *describes* an anomaly
(`GroupRange::Anomaly`) instead of raising it, and `read_segments` hands every shape to
`retired_or`, which re-reads the root. Deciding locally would answer "corrupt" for a
generation the store has already moved off — a hard read failure invented out of an
ordinary overwrite, the exact arm decision 7(h) exists to prevent (`0016:2452-2471`). This
also fixes the ordering bug the previous round's review found (over-ceiling root refused
*before* the supersede re-check), and it is bound by
`an_over_ceiling_root_superseded_mid_read_restarts_onto_the_live_generation`.

**The bounds, stated for what they are.** The reader bounds the **work**: a table past
`MAX_ROOT_SEGMENTS` is refused before its range is read at all (`:2247-2251`); the range is
the group's *and epoch's* own prefix (`:2252`); and each page asks for
`SEGMENT_PAGE_LIMIT = 128` (`:2111-2129`, `:2257`) — **this reader's constant, never the
root's claim**, which is the change from iteration 7 (it passed `accounted + 1`, letting a
record size a page). The **bytes** are the seam's, assigned to getwyrd/wyrd#674 and said so
in the code (`:2076-2109`, `:523-541`); the sentence claiming a
`MAX_ROOT_SEGMENTS × MAX_VALUE_BYTES` *memory* bound is deleted, not defended.

Termination of the paged walk rests on the contract, not on a page counter: the cursor is
exclusive and strictly advancing (clauses 2/3, `crates/traits/src/lib.rs:1105`), and every
row the walk keeps has a distinct parsed index below the root's claim — the first row the
table cannot account for ends the walk where it is found. So a group costs at most
`claim + 1` rows however it is paged.

### `crates/core/src/read.rs` (+28 semantic)

`read_object` resolves through `resolve_chunk_map` before assembling bytes (`:518-534`),
and the size framing comes from the resolved record. `read_path` inherits it (it calls
`read_object`).

**`read_object_from` keeps its signature** (`crates/core/src/read.rs:60` on the base; `:65` after the patch) and
keeps failing closed on a segmented map: it takes no `MetadataStore`, so it *cannot*
resolve. The byte half is factored out into a private `read_chunks_collecting`, and
`read_object_collecting` keeps its old shape as the `as_flat()`-then-delegate wrapper. Cost
of the alternative, measured on iteration 7's own diff: changing that signature touched
**10 files** and ~200 lines of pure call-site churn
(`iteration-v7/patch.diff` — `crates/chunkstore-grpc/tests/tier2_integration.rs`,
`crates/core/benches/throughput.rs`, `crates/core/tests/placement_record.rs`,
`crates/dst/tests/custodian.rs`, `crates/server/tests/{dst_commit,dst_erasure,dst_read_fanout,erasure_path,read_fanout,read_path,write_fanout}.rs`)
and bought nothing: the entry has no production caller (20 call sites, all tests and one
bench).

`committed_inode` now returns `(InodeId, InodeRecord)` (`:577-591`). This is the one
signature change, and it ripples over exactly **two** files — `read.rs` itself and the
three gateway call sites in `crates/server/src/lib.rs` — which is the brief's stated
limit. The alternative (a second `committed_inode_id` entry) would have left
`committed_inode` with zero callers, i.e. dead public API.

### `crates/server/src/lib.rs` (+27 semantic)

The two sites the brief names resolve through the resolver instead of `as_flat()`:
`get_object_streaming` (`:344`, resolving at `:354`) and `get_object_range` (`:427`, resolving at `:442`). Both now frame the
response from `resolved.record`, so a read that restarted onto a live generation cannot
emit the retired generation's `Content-Length`/`ETag`/`Last-Modified` alongside the new
bytes. `head_object` destructures the id away — HEAD never resolves a chunk map, which is
what keeps it one metadata round-trip. `ChunkMapError` is no longer imported by this file.

### `crates/dst/tests/custodian.rs` (+136 semantic)

Property 9, `segmented_resolve_never_tears`: a segmented root is retired in the window
between the reader's root read and its `seg:` page, and **on half the seeds the drain has
already reclaimed one of the retired generation's records**, so the old map cannot be
completed at all. The read must answer the whole live generation — never a byte mix, never
short, never `NoSuchKey`. Added to the *existing* file deliberately: a new
`crates/dst/tests/*.rs` would join the added-test set and force `RUSTFLAGS=--cfg madsim` +
50 seeds onto the whole `C4-verify` invocation.

### `docs/design/architecture/06-runtime-view.md` §6.2 step 2 (+1)

The resolver paragraph, and only that one. It states the bound honestly (the reader bounds
the work; the bytes are the store's) and **does not** claim universal consumer routing — it
says "every consumer that can resolve — the whole-object, streaming and ranged read paths
today", because the custodian passes still fail closed until #650/#651. Claiming otherwise
is what earned iteration 7's C1 finding.

---

## 2. The five salvage deltas the brief required (against `iteration-v7/patch.diff`)

1. **Dropped** `crates/custodian/src/resolve.rs` and its `lib.rs` line — 202 semantic lines
   with no in-slice caller. ✅ (not in this patch at all)
2. **Reverted** the `read_object_from` signature change and deleted the 10-file migration.
   ✅ (see above)
3. **Replaced** the delegating `PageCap` double with a **self-contained** fake and asserted
   on its request log, including the new fixed-page-bound clause 2(c). ✅
4. **Corrected** every doc comment and error text claiming a memory/materialisation bound,
   and wrote the `review-rejected.md` rows. ✅
5. **Pruned** the co-located resolver unit tests (this patch adds **zero** lines to
   `metadata.rs`'s `#[cfg(test)]` module — iteration 7 added ~200) and collapsed three
   resolve entries + three result types to **two entries and one type**. ✅

---

## 3. Budget: measured, and over. Read this before sign-off.

`git diff` + the two new files, counting non-blank non-comment added lines:

| file | measured | brief's expected shape |
|---|---|---|
| `crates/core/src/metadata.rs` | **271** | ~350 |
| `crates/core/src/read.rs` | **28** | ~30 |
| `crates/server/src/lib.rs` | **27** | ~30 |
| `docs/.../06-runtime-view.md` | **1** | ~10 |
| `crates/dst/tests/custodian.rs` | **136** | ~100 |
| `crates/core/tests/segmented_map_resolution.rs` (new) | **589** | ~350 |
| `crates/server/tests/segmented_object_read.rs` (new) | **197** | ~130 |
| **total** | **1,249 / 7 files** | ~1,000 / ≤10 files |

**The production side is under budget (327 vs ~410). The 249-line overage is entirely in
the two discriminator test files**, and I could not remove it without removing a Check
criterion. I am flagging it rather than hiding it, because the brief says an over-budget
patch is iterate-to-Plan by default.

What I did prune, with the numbers:

* iteration 7's co-located `metadata.rs` unit tests: **−~200** (none re-added; every case
  they bound is bound by the integration file, which is the discriminator);
* `crates/custodian/src/resolve.rs`: **−202**;
* the `read_object_from` migration: **−~200 across 10 files**;
* within the core test file, this build's own passes: 663 → **589** (merged the
  superseded/deleted retired arms into one two-arm loop; folded the small-root page-limit
  clause into the group-scoping test and dropped its duplicate fixture; flattened the
  6-case anomaly table; inlined a helper; replaced the `Quirk` enum with one bool; made the
  fake's `scan` *refuse* rather than implement an unpaged scan, which is both 7 lines
  shorter and a stronger oracle).

Why the remainder is irreducible: the core file carries **10 cases**, and every one binds a
distinct clause the Check criterion names — 1 (core byte-identity), 2(a) group+epoch
scoping, 2(b) ceiling refused unread, 2(c) fixed page bound + the ceiling's inside edge,
2(d) fail-closed scoped to the object, and 3's five distinct obligations (six fail-closed
shapes, the two retired arms, the ceiling-through-the-arbiter case, parsed-index ordering,
and totality under endless retirement). Dropping any one drops a criterion. The remaining
~190 lines are the self-contained fake store itself, which is what retired the
`PageCap` TEST-GAP finding: it must own its rows and answer all four trait methods from
them, and a delegating wrapper (which is what would be shorter) is exactly what the review
rejected. For calibration, iteration 7 measured **1,961 / 19 files**; this is **1,249 / 7**
— the direction the sign-off asked for, from a base the brief itself estimated at ~1,000.

**A split that would fit does not exist inside this criterion:** the gateway reads
(`wyrd_gateway_core::ObjectGateway`) are named in the Success criterion, and the DST
property is explicitly "built and run this cycle, not deferred". Per `docs/principles.md`
§1.2/§2 — with an *Invariant to restore* named (C-1), the target is the smallest change
that restores the invariant, not the smallest diff — I shipped the smallest binding set and
am reporting the measurement instead of dropping coverage to hit a proxy number. **If the
human disagrees, the cheapest correction is to move criterion 3's fail-closed table (six
shapes, ~48 lines) and/or the totality case (~33 lines) to #650 — but both are Check
criteria today, so that is a Plan decision, not mine.**

---

## 4. Refuting my own test (the three forced questions)

**(a) Genuine red?** **Yes — measured, not asserted.** `./engine/scripts/run-verify.sh`
(the configured `C4-verify` gate) applies the patch on `origin/pdca-integration/main`, then
re-runs with the production reverted and the two test files kept:

```
run-verify.sh: GREEN — cargo test -p wyrd-core --test segmented_map_resolution -p wyrd-server --test segmented_object_read (fix applied)
test result: ok. 10 passed; 0 failed
test result: ok.  2 passed; 0 failed
run-verify.sh: RED — (production reverted, test kept)
test result: FAILED. 0 passed; 10 failed
run-verify.sh: PASS — red without the fix, green with it.
```

The harness stops at the first failing target, so I ran the **server** file separately in
the same reverted worktree: `0 passed; 2 failed`, panicking on
`SegmentedMapUnsupported { operation: "Gateway::get_object_streaming" }`. Both files are
red on the base; **12 of 12** cases fail without the fix.

The red is an *assertion* red, not a compile red: both files import only base-visible
symbols (`ChunkMap`, `SegmentedMap`, `SegmentGroup`, `SegmentRef`, `SegmentRecord`,
`seg_key`, `seg_group_prefix`, `seg_range_prefix`, `parse_seg_key` is not needed,
`MAX_ROOT_SEGMENTS`, `MAX_VALUE_BYTES`, `encode`/`inode_key`, `read::{read_object,
read_path}`, `ObjectGateway`) and name **nothing** this patch adds. `assert_fails_closed`
additionally refuses to accept `SegmentedMapUnsupported` as a pass, so a "the read must
fail" case cannot be satisfied by the base's blanket refusal — the failure has to be the
resolver's own typed anomaly.

**(b) Production path?** **Yes.** Every assertion is driven through
`wyrd_core::read::{read_object, read_path}` and `wyrd_gateway_core::ObjectGateway`'s
`get_object_streaming` / `get_object_range` on a real `wyrd_server::Gateway`. The resolver
is never called directly — not once in either file. The only doubles are a `MetadataStore`
(a seam the production code calls **through**) and the real on-disk `FsChunkStore`; the
code under test is the shipped `crates/core/src/metadata.rs` resolver reached through the
shipped read paths. Criterion 1 runs against the **real redb** backend.

**(c) Fixture includes the fault?** **Yes, in each case the fault is *in* the fixture, not
curated out of it:**
* the missing-segment cases seed the root that *names* the segment and then leave it out —
  the object under test is the broken one, and a second well-formed object proves the
  refusal is scoped rather than global;
* the over-ceiling case seeds a root naming `MAX_ROOT_SEGMENTS + 1` segments, and the
  boundary case seeds exactly `MAX_ROOT_SEGMENTS` and requires it to **read back whole** —
  so a ceiling sitting one either side fails;
* the value-ceiling case is the *correct* record padded with JSON-ignorable whitespace to
  `MAX_VALUE_BYTES + 1`: it would parse back to exactly the right record, so its **size** is
  the only thing wrong with it;
* the retirement cases apply the mutation **inside** the store, mid-resolve, at the exact
  window (`When::RootRead` / `When::SegPage`), and also delete a record of the retired
  generation — so a reader that did not restart has no old-generation answer left to
  succeed with by accident;
* the ordering case makes the store hand every `seg:` page back **reversed**, so
  arrival-order concatenation returns the same length and the same chunks with the halves
  swapped — silently wrong content, which the byte comparison catches;
* the DST property injects the retirement under madsim across **50 seeds**, with the
  drain's record deletion on half of them.

One fixture bug found and fixed during this build, which is itself evidence the oracle
bites: the fail-closed-per-object case originally keyed both objects to the same
`SegmentGroup`, so the well-formed object's row satisfied the broken object's missing
segment and the read *succeeded*. Each object now owns its own group, as each publishing
session mints its own nonce.

---

## 5. Evidence run (the brief requires the tiers `ci` stops short of)

| gate | result |
|---|---|
| `C4-verify` (`./engine/scripts/run-verify.sh`, `PDCA_VERIFY_BASE=origin/pdca-integration/main`) | **PASS** — red without the fix, green with it |
| `cargo xtask conformance` | **PASS** — 5 valid + 6 invalid vectors |
| `cargo xtask statics` (ADR-0035) | **PASS** — no DST-reachable shared mutable global state |
| `cargo xtask dst` (madsim, `MADSIM_TEST_NUM=50`) | **PASS** — 11 properties in `custodian.rs` incl. the new `segmented_resolve_never_tears` |
| `cargo xtask ci` | **RED at `cargo deny check`**, see below |
| `cargo fmt --all -- --check` | clean |
| `cargo clippy --workspace --exclude wyrd-dst --all-targets` | clean (`-D warnings`) |
| `cargo test --workspace --exclude wyrd-dst` | all green |
| `cargo doc -p wyrd-core --no-deps` | not a gate here; I still removed the two public→private intra-doc links my first draft introduced, so this patch adds **no** new rustdoc diagnostic |

`cargo xtask ci` reached, in order: typos ✅, docs lint ✅, docs render ✅, gitlink guard ✅,
unsafe-forbid guard ✅, fmt ✅, clippy ✅, build ✅, test ✅, machete ✅ — then failed at
`cargo deny check` on **RUSTSEC-2026-0221** (`event-listener` 5.4.1, unsound). I confirmed
this is a **base-tree** condition, not this patch's: `cargo deny check` fails identically in
the verify worktree with the production reverted. It is tracked as **getwyrd/wyrd#673**.
Per the brief, `deny.toml` is a declared zero-tolerance wall and the lockfile is not this
slice's to touch (an unscoped 5.4.1→5.4.2 bump was rejected at iteration 6 and did not clear
the advisory anyway). **This patch touches no `Cargo.toml` and no `Cargo.lock`.**

No new dev-dependency was needed: `async-trait`, `tokio`, `tempfile`, `wyrd-metadata-redb`
and `wyrd-chunkstore-fs` are already dev-dependencies of `wyrd-core`
(`crates/core/Cargo.toml:24-46`), and the server test uses only existing normal/dev deps.

Both declared external dependencies were **present** (`typos-cli 1.48.0`; `markdown_it` +
`yaml` importable), so the prose gates actually ran rather than warn-skipping. **No
NEEDS-HUMAN external dependency.**

---

## 6. Alternatives considered and rejected (with costs)

* **A caller-side timeout/deadline on the resolver's awaits.** Rejected — standing
  rejection (i), 3× across #508/#636; recorded in `review-rejected.md`. `wyrd-core` has no
  runtime dependency to spend a deadline from (`crates/core/Cargo.toml:11-15`), the trait
  puts termination on the backend (`crates/traits/src/lib.rs:1000-1012`), and no other
  caller in `crates/core`, `crates/custodian` or the gateway wraps a metadata await.
* **A byte ceiling inside the resolver.** Settled at Plan, tracked as **#674**, argued in
  full in `review-rejected.md`. Concretely: the cheapest honest fix is a trait change
  (`scan_page`'s return type or a byte budget parameter) + **5 backends** (redb, TiKV, FDB,
  the testkit double, the mem double) + the shared `wyrd-metadata-conformance` suite. A
  guard in this one function would leave the identical hole in `high_water_marks`' `inode:`
  walk, GC's `orphan:` ledger and every `get` in the tree.
* **Passing `accounted + 1` as the page limit** (iteration 7's shape). Rejected: it lets
  the record size the page. Cost of the fix: one constant and one call-site argument
  (`crates/core/src/metadata.rs:2129`, `:2257`) — and it is now bound by
  `the_widest_admissible_root_still_sizes_no_page_and_reads_back_whole`, which asserts a
  512-segment root and a 2-segment root produce the *same* limit.
* **A `MapResolution { Resolved, Retired }` enum with the restart left to each caller.**
  Rejected on duplication: three in-slice consumers × ~8 lines of restart logic = ~24 lines
  of the same decision written three times, and the resolve-retry rule is precisely the
  thing that must not be re-derived per consumer (decision 7(e)). Folding the restart into
  the snapshot entry costs 4 lines (`:2422-2433`).
* **Keeping `committed_inode`'s return type and adding a second entry.** Rejected: it
  leaves `committed_inode` with zero callers. The change as shipped touches 4 lines across
  2 files.
* **Making the fake store wrap redb** (iteration 7's `PageCap`). Rejected: that is the
  finding. A wrapper reports what it forwarded, not what the resolver asked for, so it
  cannot bind criterion 2 at all.

---

## 7. Things a reviewer may reasonably want to look at

* `SEGMENT_PAGE_LIMIT` is `pub`. It is part of the declared reader-side bound and the doc
  comment is where that bound is written down; it is not a resolve entry (the brief's cap
  of two entries is on entries).
* `resolve_current_chunk_map` is `pub` although its only in-slice caller is
  `resolve_chunk_map`. The brief sanctions exactly these two entries ("one from a
  caller-held snapshot record, one that reads the live root"); #650/#651 call the live-root
  one from a stale scan snapshot.
* The resolver takes `&dyn MetadataStore` rather than `&impl MetadataStore` (the module's
  usual style): one shared resolver, not one monomorphised copy per consumer. Callers pass
  `&M` and the unsized coercion is implicit.
* This resolver is the **first production caller of `scan_page`** in the tree (#634/PR #645
  landed the seam; `crates/core/src`, `crates/custodian/src` and `crates/server/src` had
  none). That is why `read_group_range` spells out which paging clause each step relies on.

---

## 8. Self-review against the target's standing rubric (`AGENTS.md` §"Review rubric & protocol")

Run as an explicit final step, since these are the criteria the reviewers apply.

**Hard conventions:**
* *One clock per correctness lifecycle* — this patch reads **no** clock. The fixtures use
  fixed constants for the write path's lease stamping. Nothing new to attribute.
* *Narrow trait seams / dependency direction* — the resolver names only `MetadataStore`
  (`get`, `scan_page`); the gateway uses only what its seam grants (ADR-0046). No new crate
  dependency, normal or dev.
* *Metadata validation boundaries (ADR-0045)* — decode-time structural invariants are
  untouched (#648 owns them). Everything this patch checks is **contextual** and surfaces as
  a typed error, never as a value; and the capacity ceiling is enforced exactly where
  `MAX_ROOT_SEGMENTS`' own doc on the base says it belongs — "where a segment table becomes
  work … the ranged read that would spend it (#649/#653)" (`crates/core/src/metadata.rs:310-315`
  on the base).
* *No DST-reachable shared mutable global state* — `cargo xtask statics` green.
* *`#![forbid(unsafe_code)]`* — no new crate; both new test files carry it anyway, and the
  `unsafe-guard` step passed.
* *Docs currency* — §6.2 step 2 lands in this same patch.

**Recurring defect classes** (the ones this diff's surface touches):
* *Absent or unsupported entries* — every absent/undecodable/oversize/unnamed/mismatched
  entry produces an explicit typed error; there is no silent skip and no silent success.
  The tests assert **bytes**, not counts, so none can pass while the property fails.
* *Protocol input* — oversize and malformed input under the `seg:` range is an error, never
  silently accepted; the key grammar is the existing strict `parse_seg_key`, not a new
  hand-rolled parser.
* *Await discipline* — the awaits this patch adds carry no caller-side timeout. That is the
  standing rejection recorded in `review-rejected.md` Class 2 with the citations, not an
  oversight: the seam contract puts termination on the backend and `wyrd-core` is
  executor-free. The paged walk's termination is argued in place from the paging clauses
  the shared conformance suite enforces on every backend
  (`crates/core/src/metadata.rs:2121-2128`), which is why it carries no page counter of its
  own. No new spawned task; the gateway's existing spawn is unchanged.
* *Test fidelity* — the new concurrent path (resolve racing retirement) lands with seeded
  DST coverage over the 50-seed sweep, and the sim double mirrors the seam's semantics
  (the four paging clauses, honestly, with the one deliberately broken clause named).
* *Serialization identity / transactions / probes / workflow edits* — untouched by this
  diff.

**Reviewer protocol:** no DCO claims from my side; the deferral to getwyrd/wyrd#674 is
recorded in the format the gate parses (326 rows parse, verified by running the loader's own
logic); nothing out-of-scope was fixed in-passing.
