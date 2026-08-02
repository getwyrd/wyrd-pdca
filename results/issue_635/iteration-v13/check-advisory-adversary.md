# Adversarial review — issue 635 / segmented-chunk-map (advisory, non-gating)

Re-ran the asserted red→green independently in scratch (clone of `origin/main` @ `9120f7a`,
the single added test file copied in, production untouched): **9 tests, 0 passed, 9 failed
with `invalid type: map, expected a sequence` propagating out of `reconcile_step` /
`high_water_marks` / `read_object`** — a genuine assertion red, not a build error. The same
tree with `patch.diff` applied: **9 passed**. The C4-verify claim holds; I could not refute it.
Findings below are what survived the attempt.

- NEEDS-HUMAN [human] — **One damaged object stops *all* space reclamation, cluster-wide, and
  the pass still reports success.** `crates/custodian/src/gc.rs:269` — `protects()` answers
  `true` for *every* `(dserver, fragment)` in the fleet as soon as `unresolvable` is non-empty.
  Demonstrated with a probe in a scratch copy of the bundle: (control) a stray fragment on
  server 5 with a valid `orphan:` record stamped at t=0 is reclaimed by a GC pass at
  t=`GRACE*10`; (probe) seed the leg-7 damaged object — an unrelated object, other servers,
  other chunk ids — and the *same* stray survives, with `reconcile_step` returning
  `Ok(Satisfied)`. The stall lasts as long as the damaged record does, and this slice ships no
  repair path for it; `crates/custodian/src/desired_state.rs:191` blocks every drain on the
  same condition. The brief's containment table can be read as sanctioning this ("an incomplete
  reference set may not authorize any reclamation"), which is why this is a maintainer call and
  not a build defect: it trades one damaged object for the whole cluster's GC and drain
  progress — the same blast-radius trade the brief *rejected* for `high_water_marks`. Note the
  leg-7 assertions "nothing of the damaged object is reclaimed" / "the healthy objects'
  fragments are untouched"
  (`crates/custodian/tests/segmented_map_consumers.rs:1275`,`:1319`) pass for this reason —
  nothing anywhere is reclaimed — so they do not discriminate a per-object containment from a
  fleet-wide one; there is no positive control that genuine garbage is still collected.
- NEEDS-HUMAN [impl] — **The blocked pass mis-attributes itself.**
  `crates/custodian/src/gc.rs:162-170`: a fragment protected only because the reference set is
  incomplete is emitted with `reason = "referenced"`, which is false — it is referenced by
  nothing, and an operator reading the audit trail cannot tell the two apart. And
  `crates/custodian/src/gc.rs:128` returns `Reconciled::Satisfied` for that pass (observed
  `Ok(Satisfied)` above): a "policy satisfied" verdict for a pass that reclaimed nothing
  *because it was blocked* is the rubric's *Absent or unsupported entries* forbidden move
  ("never silent success … or a count-based assertion that can pass while the property fails",
  `AGENTS.md:175-177`). A third skip reason (`reference-set-incomplete`) and a non-`Satisfied`
  outcome would close it; the `gc_unresolvable_chunk_map` counter at `gc.rs:430` is the only
  signal today and it does not say that reclamation stopped.
- NEEDS-HUMAN [impl] — **A transient concurrency outcome is classified as object damage.**
  `crates/core/src/metadata.rs:3026` raises `ChunkMapError::MapResolutionUnstable` when a
  segmented generation is retired under the resolver `MAX_RESOLVE_RESTARTS` times, and
  `crates/custodian/src/gc.rs:339` contains *any* `ChunkMapError` as "this object's map cannot
  be resolved". Concrete case: a segmented object republished three times while one GC pass
  resolves it — no corruption anywhere — is recorded in `unresolvable`, which (previous
  findings) stops every reclamation in the fleet for that pass and names a **healthy** object
  to the operator on the drain surface (`desired_state.rs:191`). Nothing in the bundle tests
  it; the fix is to distinguish a retry-exhausted resolve from a structural fault.
- NEEDS-HUMAN [impl] — **The containment classifier itself is unpinned.** `cargo mutants`
  reports both guards — `crates/custodian/src/gc.rs:307` and `:339`,
  `err.downcast_ref::<ChunkMapError>().is_some()` — replaced by `true` with the suite still
  green (`mutants.out/missed.txt`). The documented contract at `gc.rs:286-291` ("anything that
  is **not** the object's own fault — a store error … — still propagates") therefore has no
  test: under that mutation a transient metadata-store failure mid-pass is silently recorded as
  a damaged object and the pass reports success. A store double whose `get`/`scan_page` fails,
  asserting propagation, would go red on the mutant.
- NEEDS-HUMAN [human] — **Two extra metadata point reads per committed object per pass, for
  objects that are entirely flat.** Measured with a counting `MetadataStore` over 50 committed
  **flat** objects, same probe on both trees: base `backfill::reconcile` = `gets=0`, patched =
  `gets=100`; base restore pass = `gets=0`, patched = `gets=100`. The cause is
  `crates/custodian/src/resolve.rs:81`, which re-reads the root through
  `metadata::resolve_current_chunk_map` for *every* record of every pass
  (`gc.rs:337`, `restore.rs:383`, `rebalance.rs:168`, `backfill.rs:109`,`:252`), including flat
  maps that need no resolution. In `crates/custodian/src/reconstruction.rs:615` the multiplier
  is per **repair obligation** — `find_chunk` re-resolves every committed object in the store,
  including every segmented object's whole `seg:` range, once per queued repair. The brief
  asked for one shared resolver, not for maintenance passes to switch from their scan snapshot
  to a live per-object re-read; the narrower option exists and is already used by the read
  paths (`metadata::resolve_live_chunk_map`, `crates/core/src/metadata.rs:3063`). It is
  documented as deliberate at `resolve.rs:27-44`, so overriding it is a scope/fitness decision —
  but no gate measures it and no test covers the cost.
- NEEDS-HUMAN [impl] — **Backfill resolves every committed object twice per pass.**
  `crates/custodian/src/backfill.rs:109` resolves each record to classify it and then
  `crates/custodian/src/backfill.rs:252` (`emit_remaining`) resolves the whole store again for
  the gauge — for a segmented object that is a second full `seg:` range read of every segment
  record, discarding the list the first loop already had. Accumulating the count in the main
  loop removes the second walk without changing the gauge.
- NEEDS-HUMAN [impl] — **Orphaned doc comment on the containment attribution surface.**
  `crates/custodian/src/gc.rs:401-408` is written for `emit_unresolvable` ("Emit a committed
  object whose chunk map could not be resolved …") but sits directly on
  `emit_uncommitted_unreadable` (`:418`), whose own doc begins at `:409`; the real
  `emit_unresolvable` (`:429`) is left undocumented. rustdoc renders the wrong description for
  the function, on exactly the surface leg 7's containment story rests on.

## Attempted and could not refute

- The red→green itself (reproduced independently, above) — and the test drives the **real**
  `reconcile_step`, `reconcile_after_restore`, `reconciliation_status`,
  `metadata::high_water_marks`, `core::read::read_object`, `backfill::reconcile` and the real
  rebalance/reconstruction repoints, not doubles of them; the only stand-in is the store/fleet.
- Flat-record byte identity: `ChunkMap::Flat` serializes as a bare JSON array
  (`crates/core/src/metadata.rs:1382-1389`), field order and every `skip_serializing_if` are
  unchanged (`:1723-1763`), and leg 5 asserts stored `inode:` bytes byte-for-byte after every
  pass — all green in my run.
- `high_water_marks` totality (`crates/core/src/metadata.rs:5456`): I could not construct an
  arrangement of store contents that returns `Err` — every walk is paged, an undecodable
  `inode:`/`seg:` value is attributed and its ids recovered from the bytes, and no root is
  resolved.
- Resolver boundedness and ordering: `read_group_range`
  (`crates/core/src/metadata.rs:2763-2805`) refuses a table past `MAX_ROOT_SEGMENTS`, pages one
  row wider than the root's own claim, and orders by the **parsed** index; `seg_range_prefix`
  ends in `:` so epoch `1` cannot bleed into epoch `12`; `parse_seg_key` rejects a non-canonical
  epoch and a non-fixed-width index.
- Missed mutants in `json_chunk_id_floor` / `json_string_token`
  (`crates/core/src/metadata.rs:5044-5275`, `mutants.out/missed.txt`): I expected a floor
  under-approximation, but `raw_chunk_id_floor` takes `max(parsed, scanned)` and the byte
  scanner recovers the same ids, so those survivors are redundancy, not a hole.
