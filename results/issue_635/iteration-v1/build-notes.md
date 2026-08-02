# Build notes — issue 635 / segmented-chunk-map

*(Withheld from the reviewer; written for the human at sign-off.)*

Target branch: `getwyrd/wyrd @ main`. Built in `$PDCA_WORKTREE`
(`/home/eddie/development/wyrd/wyrd.pdca-wt-l0`) at `b0cd199` (= `origin/main`).
Planning artifact read in full before writing code: proposal 0016 decision 7,
`docs/design/proposals/draft/0016-multipart-commit-protocol.md:2280-2496`, plus the `seg:`
/ `seggrp:` §1 rows (`:354`, `:499-527`), the object-ceiling arithmetic (`:218-232`) and
the batch-inventory rows (`:654-663`).

---

## 1. BASE DISCREPANCY the human must know about (not a blocker, but read this)

The brief's `Falsifiability` block says this bundle's base is "`origin/main` **plus #634**"
and instructs that the added test's `MemMeta` double **must implement `scan_page`**
("#634's required method, one delegating line").

**That is not the base I was given.** `$PDCA_WORKTREE` is at `b0cd199`, which is exactly
`origin/main`; `scan_page` does not exist in `crates/traits/src/lib.rs` there, and
`origin/pdca-integration/main` (the ref the brief expects `$PDCA_VERIFY_BASE` to name)
**does not exist on the remote** — #634 is still an unmerged branch
(`origin/enhancement/634-scan-page-seam`, 1 commit, +5726 lines).

So implementing `scan_page` on the in-test double would have been an immediate compile
error ("method `scan_page` is not a member of trait `MetadataStore`") on the tree I build
and the tree `run-verify.sh` resets to. I therefore did **not** implement it. Consequences:

* everything below is verified against `origin/main`, and the C4 gates agree (`run-verify.sh`
  resolved the same base and produced a genuine assertion-RED, see §4);
* **at the wave fold onto #634 the new test file will need `scan_page` added to its
  `MemMeta`** (the same 12-line delegation #634 adds to every other in-test double). That is
  the file conflict the brief's `Ordering note` predicts; it is one added method, and the
  fold is where it belongs. Nothing else in this slice consumes `scan_page`.

This slice makes **no `Cargo.toml` change** (verified: zero `Cargo.toml` hunks in
`patch.diff`), so the RED leg's Cargo revert cannot turn leg A's assertion-red into a build
error.

---

## 2. What I built, and the decisions the brief asked me to record

### The record shape — exactly the settled encoding

`InodeRecord.chunk_map` is now `ChunkMap` (`crates/core/src/metadata.rs:690`), a
two-variant value discriminated **by JSON type** via a hand-written `Deserialize` visitor
(`visit_seq` → `Flat`, `visit_map` → `Segmented`). Flat is a JSON array, byte-identical to
today in both directions; segmented is exactly the object the brief pins, field order
included. I did **not** use `#[serde(untagged)]`: it buffers through `Content` and collapses
every failure into "data did not match any variant", which would have made the
per-invariant decode errors (below) impossible to distinguish.

Parse-don't-validate is taken literally, because the brief made it a leg and `AGENTS.md:146-149`
makes it a hard convention:

* `SegmentGroup` and `SegmentedMap` have **private fields** and validating constructors, and
  their `Deserialize` impls route through those constructors. A `SegmentedMap` whose
  `segment_count` disagrees, whose indices duplicate/skip/reorder, whose byte spans do not
  tile contiguously from 0, or whose nonce is not 32 lowercase hex **cannot be
  constructed at all**, in memory or from bytes.
* `SegmentRecord`'s `byte_len == sum(chunk.len)` is enforced at decode too, so the root's
  segment table and the segment record can never disagree about coverage.
* `parse_seg_key` is strict about the **fixed six-digit width**, so one segment has exactly
  one key.

What I deliberately did **not** enforce at decode: `MAX_ROOT_SEGMENTS`. It is a *derived
capacity* constant (0016 computes 312–520); rejecting a stored record against it would make
a durable object unreadable if the constant ever moved. It is a publication-time guard
instead (`SegmentedPublication::plan` → `TooManySegments`), which is ADR-0045's
"liberal on read, strict in maintenance" boundary. Flagged here in case the reviewer
expects the stricter reading.

### The committer — the flip takes the caller's mutations, not just its preconditions

`SegmentedPublication` (`metadata.rs`) exposes `plan()` / `segment_batches()` /
`flip_batch()` / `write_segments()` / `flip()` / `publish()`. The brief's forced correction
is honoured: `flip` is a full `WriteBatch` the caller hands in, whose **preconditions and
mutations** are merged into the one batch that carries the root CAS. `wyrd_traits::WriteBatch`
already *is* `{preconditions, puts, deletes}` with public fields, so the caller's
contribution needed no new type. Leg B asserts both arms — a false caller precondition
leaves the root un-flipped *and* the caller's mutation unwritten (and the already-written
segments in place); a true one lands both together.

I did **not** take the alternative ("move the flip into #636") — the API came out clean.

The split is **byte-budgeted** (each segment record filled to `SEGMENT_TARGET_BYTES`
= 50 KB, i.e. the 100 KB ceiling with the 2× headroom 0016's arithmetic assumes), and the
segment-write batches to `MAX_BATCH_BYTES` = 5 MB (`E_tx/2`). No fixed record count anywhere
— that was the iteration-2 envelope defect.

### The resolver — one call, with the root's identity in its signature

`resolve_chunk_map(store, root_key, record) -> MapResolution` and its repoint-carrying
sibling `resolve_chunk_homes`. Taking the **root key** (not just the decoded record) is what
makes decision 7(h) expressible: on an absent segment the resolver re-reads the root and
either answers `MapResolution::Retired` (root moved on / gone → restart or drop) or fails
closed with a typed `ChunkMapError::SegmentAbsent`. Both arms are asserted, in unit tests
*and* as a DST interleaving.

Ordering is **explicit**: segments are collected into a `BTreeMap<u32, _>` keyed by the
index parsed out of the key, never concatenated in scan order. `MetadataStore::scan` says
"Order is unspecified" (`crates/traits/src/lib.rs:770-775`) and #634 makes byte order
normative only for `scan_page`. Leg B proves it with a store double that returns the range
**reversed**, and the same double records the scan prefixes so the "bounded per-object
range, never a global `seg:` scan" property is asserted positively (the recorded prefix list
is exactly `["seg:<nonce>:<epoch>:"]`).

### Every consumer, routed

| Consumer | Site (post-patch) | How |
|---|---|---|
| GC reference build (→ scrub) | `crates/custodian/src/gc.rs:265` | `resolve::chunks_of` |
| Restore | `crates/custodian/src/restore.rs:383` | `resolve::chunks_of` |
| Backfill (+ its gauge) | `crates/custodian/src/backfill.rs:104-110`, `:195` | `resolve::chunks_of`, then a **stated skip** |
| Rebalance evacuation | `crates/custodian/src/rebalance.rs:165`, `:301` | `resolve_chunk_homes` + `repoint_chunk` |
| Reconstruction (`find_chunk`/`assess`/repair) | `crates/custodian/src/reconstruction.rs:613`, `:579` | `resolve_chunk_homes` + `repoint_chunk` |
| Gateway whole-object read | `crates/server/src/lib.rs:367` | `resolve_chunk_map` |
| Gateway ranged read | `crates/server/src/lib.rs:459` | `resolve_chunk_map` |
| Core read path | `crates/core/src/read.rs:505-516` | `resolve_chunk_map` |
| Core write / publication | `crates/core/src/write.rs:274` | `.into()` (a single PUT stays flat) |
| **`high_water_marks`** (a 9th consumer the table misses) | `crates/core/src/metadata.rs:2064` | `resolve_chunk_map` |

The 9th one is worth the human's eye: `high_water_marks` scans every `inode:` for the
largest in-process chunk id so a restart never re-mints a live id (issue #364 finding 1). A
segmented record whose chunk ids it could not see would let the gateway re-mint an id that
still backs a live object — the same class of silent loss the brief is about. It is not in
the brief's eight-site table; I routed it.

`crate::resolve::chunks_of` (`crates/custodian/src/resolve.rs`, 53 lines) exists so the
"drop a retired generation's stale resolution" rule is written **once** and emitted on the
durability seam rather than being five silent `continue`s (the rubric's *Absent or
unsupported entries* rule). The doc comment there records *why* dropping is safe for the
reference set: GC reclaims only on evidence (an `orphan:` past grace, an expired lease), and
a live generation's fragments carry neither.

### Open question 3 — backfill's decision: **resolve, then skip, with a reason**

Backfill resolves the map like every other consumer (so its remaining-empty-placement gauge
counts segmented chunks honestly) and then **skips the rewrite** for a segmented record,
emitting `wyrd.custodian.backfill.audit action=skip-segmented`. Two structural reasons,
both in the code comment: the fill is an inode CAS and a segmented chunk's `ChunkRef` does
not live in the inode (rewriting it is the decision-7(f) segment repoint), and nothing it
would fill can exist there anyway — an empty `placement` is a pre-M3 artefact while a
segmented map is produced only by a multipart Complete, which always records full-length
placement. Leg A asserts the pass returns `Satisfied` **and** the record comes out
byte-identical (root *and* segment records).

### Two writer paths that **refuse** a segmented generation, deliberately

`metadata::unlink` and `commit_chunk_map_superseding{,_leased}` return
`ChunkMapError::SegmentedRetirementUnsupported` when the *prior* generation is segmented,
instead of orphaning inline. Retiring a segmented generation is the staged
`retire:bytes:{generation}` obligation of 0016 decision 4/7(f): for a max segmented object
that is ~1.78 M orphan marks plus the `seg:` deletes, which `0016:668` requires to be
drained in byte-budgeted batches, **never inline**. The honest options were (a) an explicit
fail-closed error now, or (b) an inline fan-out that would exceed the transaction envelope
on FoundationDB and tear the delete. I took (a) — the rubric's *Absent or unsupported
entries* rule wants an explicit error over a silent partial. No production path can create a
segmented object until #636, so nothing reachable today hits it; #636 supplies the
obligation. **This is a scope call the human may want to re-decide at sign-off.**

`read::read_object_from` (the store-less snapshot entry, used by benches and a few tests)
likewise raises `ReadError::SegmentedMapNeedsStore` rather than reading a segmented map as
zero chunks — a short read is exactly the silent failure the invariant forbids.

### `committed_inode` now returns `(InodeId, InodeRecord)`

The resolver needs the root's identity, and the gateway only had the record. Three call
sites, all in `crates/server/src/lib.rs`. No test called it.

---

## 3. Cost of the alternatives I rejected (numbers, not adjectives)

* **A resolver taking only `(store, &InodeRecord)`** (no root key). Diff cost: −1 parameter
  at 10 call sites, ≈10 lines. Rejected because it makes decision 7(h) *unexpressible*: with
  no way to re-read the root, an absent segment is indistinguishable from a concurrent
  retirement, so the resolver must either fail closed on a benign supersede (breaking every
  reader that races a delete) or silently return a short map (data loss). The brief calls
  this out and it is the one place I let the API get wider.
* **`#[serde(untagged)]` instead of a hand-written visitor.** Diff cost: −70 lines of
  `Serialize`/`Deserialize` impls in `metadata.rs`. Rejected because untagged collapses every
  structural violation into one opaque "did not match any variant" error, which would have
  made the six per-invariant negative cases of leg B(ii) untestable as distinct outcomes and
  would have surfaced a corrupt record as "not a chunk map" rather than as *which* invariant
  broke.
* **`ChunkMap: Deref<Target=[ChunkRef]>` (or `Index`/`iter`) to avoid the test churn.**
  Diff cost saved: ≈150 call-site edits across 30 test files (measured: the mechanical churn
  is 32 files, +1143/−163, of which 754 lines are the new test file and ~110 the DST property
  — so ≈280/163 across the existing files). **Rejected outright**: a `Deref` that yields the
  empty slice for a segmented map is exactly the "understood by one consumer, opaque to
  another" failure — every un-migrated `.chunk_map` walk would compile and silently see zero
  chunks. That is #508-attempt-4 reproduced in a new spelling. A panicking `Index` was
  rejected too: a panic inside the GC loop is not "fail safe".
* **Deferring reconstruction/rebalance write-back for segmented maps** (resolve but refuse to
  repoint). Diff cost saved: `repoint_chunk` (≈55 lines) and the `ChunkHome` plumbing (≈25
  lines across two plan structs). Rejected because the invariant statement forbids *both*
  outcomes: a consumer that cannot act "either fails safe (halting maintenance) or concludes
  the bytes are unowned". Halting repair for every segmented object is the first of those.

---

## 4. Forced self-refutation (the three questions)

**(a) Genuine red? YES — and it is an assertion, not a build error.**
Through the project's own runner, `./engine/scripts/run-verify.sh` (base resolved to
`origin/main`):

```
run-verify.sh: GREEN — cargo test -p wyrd-custodian --test segmented_map_consumers (fix applied)
test result: ok. 5 passed; 0 failed; …
run-verify.sh: RED — … (production reverted, test kept)
test result: FAILED. 0 passed; 5 failed; …
run-verify.sh: PASS — red without the fix, green with it.
```

**Tests that actually ran on the RED leg: 5. Failed: 5. Passed: 0.** The file **compiled**
on the reverted tree; every failure is an assertion/`unwrap` on a propagated decode error —
`Error("invalid type: map, expected a sequence", line: 1, column: 23)` — i.e. exactly the
`metadata::decode(&value)?` at `gc.rs:256` and its siblings refusing a segmented value. The
binding leg reports it as a message, not a panic-on-compile:
`reconcile_step must resolve a segmented chunk map, not fail on it: Some("reconciliation store access: invalid type: map, expected a sequence…")`.
(I also confirmed this independently before writing the patch, by `git stash`-ing only the
tracked source changes and running the test alone: same 0/5.)

**(b) Production path? YES.** Every leg drives the real entry point:
`wyrd_custodian::reconcile_step` (the fenced control point, all four loops supplied),
`reconcile_after_restore`, `reconciliation_status`, `backfill::reconcile`,
`wyrd_core::read::read_object`, and — in `crates/server/src/lib.rs`'s co-located tests — the
real `Gateway::get_object_streaming` / `get_object_range` over real redb + a real
`FsChunkStore`. The only stand-ins are the `MetadataStore` / `ChunkStore` **seams** the
custodian loops are defined over (the same doubles `tests/gc.rs` and `tests/reconstruction.rs`
use), plus the caller that supplies the publication precondition — which is #636's, by
design. Leg B drives the real committer and the real resolver against a real
`RedbMetadataStore::in_memory()`.

**(c) Fixture includes the fault? YES.**
* the segmented object is *in* the `inode:` range GC, restore, rebalance, reconstruction and
  backfill scan — nothing curates it out;
* leg 2 drains a server that holds **only** the segmented object's fragments (the two objects
  are placed on disjoint halves of the fleet, servers 0–2 vs 3–5), so `Pending` cannot come
  from the flat object — a resolver that decodes the new root but never reads the `seg:`
  range answers `Satisfied` there;
* leg 6's reconstruction leg **actually deletes** a fragment of a chunk that lives in the
  **second** segment record (so a resolver that stops at the first segment cannot find it),
  enqueues a real obligation, and asserts the rebuilt fragment lands on the free domain and
  that the *segment record* — not the root — carries the new placement;
* leg 4 advances past the orphan grace window and re-runs GC, asserting on the **fragments
  themselves** (12 of them, `chunks × 3`), not on a `Reconciled` value.

Deliberate anti-vacuity guard: leg 2 also asserts a server holding *nothing* answers
`Satisfied`, so the `Pending` assertion is not true of every server.

---

## 5. Gates

* `cargo xtask ci` — **all checks passed** (typos + docs renderer + gitlink/unsafe guards +
  fmt + clippy `-D warnings` + build + workspace test + machete + deny + conformance vectors
  + statics + the madsim DST tier). Both prose-gate tools (`typos`, the doc renderer) are
  installed on this host, so the docs-currency edit is genuinely gated, not warn-skipped.
* `./engine/scripts/run-verify.sh` — PASS (red→green), quoted above.
* `cargo fmt --all` run over every touched file before the final `patch.diff`, so the
  target's own commit hook has nothing to reject.

## 6. Where leg B lives, and why not as a second test file

Leg B is **co-located `#[cfg(test)]` unit tests**: 15 tests in
`crates/core/src/metadata.rs`'s existing `mod tests` (encoding identity, the six decode
negatives, the key-width negatives, staged publication + the flip's atomicity with the
caller's mutation, the shuffling/bounded resolver, epoch scoping, both retry-rule arms, the
group-adoption predicate) and 2 in `crates/server/src/lib.rs` (the gateway's whole-object
and boundary-spanning ranged reads). The X51 interleaving is appended to the **existing**
`crates/dst/tests/custodian.rs` as property 9 (`segmented_resolve_never_tears_on_retirement`)
and added to the committed-regression-seed sweep. None of these is an added `*/tests/*.rs`
target, so none can join C4-verify's cargo invocation and turn leg A's assertion-red into a
build error.

## 7. One honest gap the human should note (not a NEEDS-HUMAN; a scope boundary)

0016 decision 7(f) / decision 2 require a **destination pre-mark** (`orphan:<P_new>` written
*before* the fragment lands) around a repoint, so a lost CAS never strands the destination
(execution X47). This slice's segment repoint has the same shape as the **existing flat
repoint** — write the fragment, then CAS — and therefore inherits the same pre-existing gap;
it does not add a new one, and adding the pre-mark protocol for segmented maps alone would
leave the two paths inconsistent. It belongs with decision 2's repoint rules (#637 / the
multipart slices). What this slice *does* add on top of the flat shape is the second
precondition 0016 requires: `require(inode == prior)` alongside `require(seg == prior)`, so a
repoint loses to a concurrent supersede/delete of the generation.

No external dependency was missing: `typos` and `docs-renderer` (the brief's two) are both
installed here and both ran inside `cargo xtask ci`.
