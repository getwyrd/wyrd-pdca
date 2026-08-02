# Adversarial review — issue 635 / segmented-chunk-map (advisory, non-gating)

Attacked the red→green evidence, the encoding/resolver core, and every `.chunk_map`
consumer the diff touches. Leg A's evidence held up (see "could not refute" below); the
landed findings are all on the **write/retirement side** of the slice, which no leg of the
success criterion observes.

## Findings

- **NEEDS-HUMAN [impl] — the three `SegmentedRetirementUnsupported` refusals ship with zero
  test coverage.** `crates/core/src/metadata.rs:1392` (`unlink`), `:1490` and `:1557`
  (`commit_chunk_map_superseding{,_leased}`) are three `.chunk_map` consumers the brief's
  design table does not list — the very hazard `Open questions` 3 flags — and each now hard-
  fails on a segmented prior. Nothing asserts it. Concrete failing case: mutate `:1391` from
  `.as_flat().ok_or(…)?` to `.as_flat().unwrap_or(&[])` and the entire suite stays green,
  while `unlink` would delete a segmented object's `inode:`+dirent and write **zero**
  `orphan:` grace records — the "unreferenced-but-undeadlined fragment ⇒ GC keeps it
  forever" hazard this same function documents at `:1341-1343`. Backfill's analogous
  unlisted-consumer decision got an error type *and* an assertion
  (`crates/custodian/tests/segmented_map_consumers.rs:961`); these three got only prose.

- **NEEDS-HUMAN [impl] — `SegmentedPublication`'s `Supersede` arm drops the prior
  generation's orphan fan-out.** `crates/core/src/metadata.rs:2489`: `flip_batch` emits
  `require(inode == encode(prior))` + `put(root)` + the caller's merged batch, and nothing
  writes `orphan:` records for the fragments the superseded generation placed — while every
  other supersede in the module does it inline and documents why (`:1487-1504`). Concrete
  failing case: publish a segmented map with `RootPrecondition::Supersede(prior)` where
  `prior` is a **flat** committed record holding N chunks (the only prior shape reachable
  today) and `flip = WriteBatch::new()`; the flat generation's fragments become unreferenced
  with no grace record and, per `:1341-1343`, are kept forever. The envelope argument the
  refusals above cite does not apply here — a flat map is ≤ the 100 KB value ceiling by
  construction, so its fan-out is bounded. It is invisible because **every** publication test
  uses `prior = InodeRecord::new_empty()` — a prior with zero chunks
  (`:3123`, `:3190`, `:3829`). Fix: fan out the flat prior's orphans, or state the caller's
  obligation on `RootPrecondition::Supersede` and assert it.

- **NEEDS-HUMAN [impl] — `seggrp_key` / `SEGGRP_MARKER` are dead code, and the docs edit
  claims the guarantee they would provide.** `crates/core/src/metadata.rs:998` and `:282`
  have no caller anywhere in `crates/`: `staged_batches` emits bare `put(seg_key(…))`
  (`:2390`) with no `require_absent(seggrp:<nonce>)`, and no path ever writes the marker. So
  the corrective rule the brief makes normative ("reserved by `require_absent(seggrp:<nonce>)`
  plus the marker record", 0016:499-527) is unenforced by the only committer that exists.
  Meanwhile the architecture edit this PR makes states it as fact —
  "`seggrp:<nonce>` is a presence-only reservation marker **that makes a group nonce
  unrepeatable**" (`docs/design/architecture/08-crosscutting-concepts.md`) — a durable
  property the shipped code does not implement. `seggrp_key`'s byte shape is also unasserted
  (`seg_key`'s is, `:3053`): delete the `:` from `format!("seggrp:{nonce}")` and nothing goes
  red. Either wire the reservation into the committer, or add the key-shape test and reword
  the doc to attribute the reservation to #636.

- **NEEDS-HUMAN [human] — the halting arm the brief calls forbidden.** The brief's *Invariant
  to restore* says a consumer that cannot resolve the map "either fails safe (halting
  maintenance) or, worse, concludes the bytes are unowned. **Both are outcomes 0016
  forbids.**" With `crates/core/src/metadata.rs:1392`/`:1490`/`:1557` in place, once #636
  publishes a segmented map an S3 `DELETE` (`crates/server/src/lib.rs:582` → `unlink`) and a
  PUT-overwrite of that key both return a hard error until the `retire:bytes:{generation}`
  obligation lands. That may well be the right stack ordering, but it is a scope/fitness call
  the brief does not settle, and the in-code deferral carries no tracked-issue marker at those
  three sites (unlike `repoint_chunk`'s `deferred: #636` at `:2070`), so the rubric's
  "Deferrals are settled" protection does not attach to it.

- **NEEDS-HUMAN [human] — the root's byte index is carried but never used to bound a ranged
  read.** `SegmentRef::byte_offset`/`byte_len` exist so "the root alone answers 'which segment
  covers byte N' without reading any segment record" (brief §Design), yet
  `crates/server/src/lib.rs:448` resolves through `resolve_live_chunk_map`, which reads
  **every** segment of the group (`crates/core/src/metadata.rs:1780`) and then walks the whole
  chunk list. At the >10 GiB launch target that is up to `MAX_ROOT_SEGMENTS` × ~50 KB ≈ 25 MB
  of metadata per one-byte ranged GET, and the same whole-map cost lands on
  `high_water_marks` (`:2705`) at every gateway restart. The co-located oracle
  (`crates/server/src/lib.rs:1118`) uses a 2-segment fixture, so it cannot see it. Whether to
  bound this now or in #508 is a design call.

- **NEEDS-HUMAN [human] — C5's red is an unrelated flake, so this iteration carries no
  mutation evidence at all.** `mutants.out/log/baseline.log` shows the *unmutated* baseline
  failing on exactly one test, `crates/server/tests/health_probe.rs:239`
  `shutdown_publishes_not_serving_before_draining` ("connect to the configured health endpoint
  within budget: transport error") — a file this diff does not touch, in a port-binding test,
  under a fully parallel `cargo test -p wyrd-core -p wyrd-custodian -p wyrd-server`. Per issue
  #236 that is **not** a refutation of the fix. But it does mean `C5 surviving mutants`
  measured **zero** mutants for this diff (iteration 2's "1 missed" was a different diff),
  which is exactly how the three unpinned surfaces above survived. Recommend re-running C5
  (serially, or with `health_probe` excluded) before any sign-off leans on causal adequacy.

## Attempted and could not refute

- **The red→green mechanics.** `crates/custodian/tests/segmented_map_consumers.rs` names only
  base symbols — `MemMeta` implements exactly the base trait's `get`/`scan`/`commit`, and the
  bundle base has no `scan_page` (`crates/traits/src/lib.rs:776`), so the file compiles pre-fix
  and its RED is a genuine assertion (`metadata::decode` cannot parse a JSON-object
  `chunk_map` into `Vec<ChunkRef>`, so `gc.rs:256`'s `?` propagates out of `reconcile_step`).
  Post-fix green is independently visible outside the gate's own claim, in
  `mutants.out/log/baseline.log:1420-1427` (all 8 consumer tests) plus the ~30 co-located
  `metadata::tests::*` at `:1110-1161`.
- **"Passes for the wrong reason" on leg 2.** The two objects sit on disjoint halves of the
  fleet, so `Pending` for server 0 can only come from reading the `seg:` range, and the
  server-9 mirror rules out vacuity (`segmented_map_consumers.rs:661-686`). Same for the
  backfill leg, which is driven by the discriminating empty-placement input (`:965`).
- **Flat byte-identity.** `deserialize_any` dispatches a JSON array to `ChunkMap::Flat`,
  `Serialize` emits the bare array, and `#[serde(try_from = "InodeRecordWire")]` preserves the
  field order and the `default`/`skip_serializing_if` pairing — legacy round-trip is asserted
  at `crates/core/src/metadata.rs:2848` and I could not construct a legacy record that
  re-encodes differently.
- **Scan-order dependence.** The resolver keys a `BTreeMap` on the *parsed* index
  (`:1779`,`:1792`) and the `Shuffling` double reverses every scan (`:3901`); a
  concatenate-in-arrival-order resolver dies there.
- **Key-grammar ambiguity.** `+7`, `07`, `007`, 5- and 7-digit indices, and a short nonce are
  all rejected (`parse_canonical_u64` at `:1038`, width check at `:1017`), so one segment has
  exactly one key.
- **Aggregate overflow.** Chunk-length sums and the root tiling are both `checked_add` with
  typed errors at decode (`:930`, `:712`), including the forged-`byte_len`-matching-a-wrapped-
  total case.
- **An unrouted production `.chunk_map` consumer.** Every remaining direct read in
  `crates/*/src` is either `as_flat()`-guarded (`core/src/read.rs:72`, and the three refusals
  above) or an `is_segmented()` dispatch (`custodian/src/backfill.rs:153`); GC, scrub,
  desired-state, restore, rebalance, reconstruction, backfill, both gateway read paths, the
  core read path and `high_water_marks` all go through the one resolver.
