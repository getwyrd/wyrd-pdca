# Build notes — #348 maintenance-loops-reject-malformed-placement

Target: `getwyrd/wyrd @ main`, tip `458db14` (worktree `$PDCA_WORKTREE`). All `path:line`
citations are against that tip / the worktree.

## What the invariant demands (and why this is not a one-module guard)

ADR-0040 decisions 3–4 (the brief's "Invariant to restore"): a committed `placement` is
valid **iff** empty (pre-M3 → identity) or `len == fragment_count()`; any other non-empty
length is **malformed**. Maintenance loops must classify **before** expanding and must
never fabricate an identity entry for a malformed vector — GC/scrub fail safe (fully
referenced, never reclaim, audit); reconstruction/rebalance skip + NEEDS-HUMAN. The read
path stays liberal.

The SELF-TEST in the brief is explicit: this is a property over the **shared classifier in
`core`** plus **four maintenance loops**, not something a single guard satisfies. So the
change is one single-source classifier consumed by all four loops — the smallest change
that restores the invariant, per `docs/principles.md` §1.2 (invariant target, not smallest
diff).

## The change

### 1. Single-source classifier — `crates/core/src/metadata.rs`
- `ChunkRef::placement_is_valid()` (`metadata.rs:159`) — `empty || len == fragment_count()`.
- `ChunkRef::checked_fragments()` (`metadata.rs:174`) — the **strict** companion to the
  liberal `fragments()` (`metadata.rs:142`): returns the same iterator when valid, else
  `Err(MalformedPlacement { expected, actual })` **before** any expansion.
- `MalformedPlacement` struct (`metadata.rs:193`) carries the mismatch for the audit/NEEDS-
  HUMAN signal.
- This is exactly the companion the #347 doc-comment on `fragments()` named for #348
  (`metadata.rs:137`, "`checked_fragments()` / `placement_is_valid()`"). `fragments()`
  itself is untouched — the read path stays liberal.

### 2. GC/scrub — fail safe (fully referenced + audit)
- `gc::referenced_fragments` now returns a `ReferenceSet { placed, malformed }`
  (`gc.rs:202`): valid chunks expand into `placed` via `checked_fragments()`; a malformed
  chunk is recorded in `malformed` (its id → classification) and **not** identity-filled; the set
  is built in `referenced_fragments` (`gc.rs:219`).
  `ReferenceSet::protects()` (`gc.rs:212`) treats *any* fragment of a malformed chunk as
  referenced.
- `gc::reconcile` (`gc.rs:112`) emits `emit_malformed` per malformed chunk and the safety
  gate uses `protects()` (`gc.rs:136`) so no fragment of a malformed chunk is ever reclaimed.
- `scrub::reconcile` (`scrub.rs:74`) iterates `referenced.placed` (malformed chunks are
  absent, so no phantom `Ok(None)` repair is fabricated over an identity-filled tail) and
  emits `emit_malformed` per malformed chunk (`scrub.rs:81`).

### 3. Reconstruction — skip + NEEDS-HUMAN
- `assess` (`reconstruction.rs:240`) resolves via `checked_fragments()`; on `Err` returns
  new `Assessment::Malformed`. `reconcile` (`reconstruction.rs:149`) maps it to
  `emit_needs_human(chunk)` and leaves the obligation queued (never rebuilds over a
  fabricated vector, never repoints).

### 4. Rebalance — skip + NEEDS-HUMAN
- `plan_evacuations` (`rebalance.rs:167`) resolves via `checked_fragments()`; on `Err` emits
  `emit_needs_human(chunk.id)` and `continue`s — the fragment stays put, nothing is copied,
  nothing is committed back.

### 5. `reconciliation_status` — conservative fail-safe (`desired_state.rs:143`)
Consumes the new `ReferenceSet`. A drain stays `Pending` while **any** malformed committed
placement exists (`!referenced.malformed.is_empty()`), because rebalance refuses to
evacuate a malformed chunk, so the drain genuinely cannot complete. Without this, a drain
could be reported `Satisfied` while an unresolved corrupt placement might still name the
draining server — an unsafe report. This is the minimal consistent extension of the fail-
safe stance; it is not one of the brief's named per-loop tests but is a caller of the
changed reference set that would otherwise be silently wrong.

## Why this shape, and alternatives rejected (with cost)

- **Guarding one module (e.g. only GC) — rejected.** The brief's SELF-TEST rules it out:
  the property spans four loops + the shared classifier. A GC-only guard leaves
  reconstruction/rebalance still acting on fabricated placement (data corruption of the
  committed record via a repoint) and scrub still enqueuing phantom repairs.
- **Making `fragments()` itself fallible / strict — rejected.** That would change the read
  path (availability first, ADR-0040 decision 4) and force every read-path caller to handle
  an error. The read path must stay liberal. A separate `checked_fragments()` is the ADR's
  prescribed shape (decision 2) and touches zero read-path callers.
- **Returning a bare `HashSet` from `referenced_fragments` and probing malformed separately
  in each loop — rejected.** That re-derives the classification twice (once for the set,
  once per loop) and risks drift between GC's and scrub's notion of "malformed". Threading
  `malformed` through the one `ReferenceSet` the two loops already share keeps a single
  source. Cost of the rejected path: a second scan of every committed chunk map per loop
  (GC + scrub both call `referenced_fragments`), plus duplicated classification logic.

## Two pre-existing #349 tests had to be updated (not new behavior — a reversed contract)

The #349 "mixed-era matrix" added two tests that assert a **short (non-empty, wrong-length)**
placement is a resolvable steady state — the exact thing ADR-0040 decision 3 rejects
("short non-empty vectors are NOT a supported steady state") and #348 reverses for the
maintenance loops:

- `scrub.rs` `detects_corruption_at_a_short_placement_vectors_fallback_index`
  (RS{2,1}, `placement: vec![5]`) → rewritten as `short_placement_is_malformed_scrub_fails_safe`.
- `reconstruction.rs` `reconstructs_a_short_placement_chunk_resolving_the_fallback_index`
  (`placement: vec![9]`) → rewritten as
  `short_placement_is_malformed_reconstruction_skips_and_flags_needs_human`.

Both now assert the strict contract, and the reconstruction one additionally asserts the
**read path is unchanged** (still resolves the object via the liberal fallback, before and
after the maintenance pass). Their section-header/flippable comments were updated to say the
short cell was re-classified by #348. The EMPTY-placement cells (valid pre-M3) are untouched
and still resolve. The GC `short_placement_vector_rs_6_3...` test (`vec![50,51,52]` on RS{6,3})
still passes unchanged — its assertion ("fragment not reclaimed") holds under both the old
identity-fallback and the new fully-referenced fail-safe, so no edit was needed.

## Tests (red→green)

Added / updated, driving production `checked_fragments`/`placement_is_valid` end-to-end:
- `crates/core/src/metadata.rs` unit mod (`tests`): classifier valid/malformed + the
  **read-path-unchanged** assertion (`fragments()` still identity-fills a malformed vector).
- `tests/gc.rs` `malformed_placement_gc_treats_chunk_as_fully_referenced` (the brief's named
  file): a malformed chunk's real fragment, orphaned, is reclaimed pre-fix / protected post-fix.
- `tests/scrub.rs` `short_placement_is_malformed_scrub_fails_safe`.
- `tests/reconstruction.rs` `short_placement_is_malformed_reconstruction_skips_and_flags_needs_human`.
- `tests/rebalance.rs` `malformed_placement_rebalance_skips_and_leaves_fragment_in_place`.

Red proven by making `checked_fragments` permissive (`if true || …`) — all four loop tests
fail (gc reclaims the fragment; scrub enqueues a phantom repair; reconstruction repoints;
rebalance evacuates). Green with the real classifier.

Green results (this worktree): core lib 18 passed (incl. 4 new metadata tests); custodian
gc 9, scrub 12, reconstruction 11, rebalance 9, gc_telemetry 1, skeleton 3, tier1 (ignored).
`cargo clippy -p wyrd-core -p wyrd-custodian --all-targets` clean. `cargo fmt` applied.

## Test-runner / environment note

The environment's `cc` is a `zig cc` shim that rejects the rustc-style triple
(`x86_64-unknown-linux-gnu`) that an old `cc-rs` in a `wyrd-core` **dev-dependency**
(`alloca`, pulled transitively by `criterion`) emits — so `cargo test -p wyrd-core --lib`
fails to build the test binary for a reason wholly unrelated to this change (the custodian
integration tests do not pull `alloca` and build/run fine). To actually *run* the pure-logic
metadata unit tests I pointed `CC` at a one-line shim that rewrites the target to zig's
`x86_64-linux-gnu` — it changes no code, only the C-compiler invocation for that dev-dep.
The custodian loop tests (the load-bearing red→green proof) need no such workaround. If the
project's `cargo xtask ci` gate runs in a properly-configured toolchain this is moot; flagged
here in case the core lib-test build needs the same triple fix in the gate environment.
