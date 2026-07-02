# Build notes — #348 maintenance-loops-reject-malformed-placement (iteration 2)

Target: `getwyrd/wyrd @ main`, tip `458db14` (worktree `$PDCA_WORKTREE`
= `/home/eddie/wyrd/wyrd.pdca-wt`). All `path:line` citations are against that tip / the
patched worktree.

## What changed since iteration 1 (the carry-forward is the spec for this rebuild)

The sign-off ratified **everything in v1 except one point**, and was explicit that this is a
**policy-scoped, attribution-only delta to `desired_state.rs`** (+ the test pinning it), *not*
a rebuild. So the v1 patch is reproduced verbatim and only the `reconciliation_status`
attribution + its rebalance-test assertion are reworked.

### The one rejected point (T5a) and the exact rework applied

v1 held **every** drain/decommission `Pending` cluster-wide whenever *any* malformed chunk
existed, **with no attribution** (v1 `desired_state.rs:147-151`,
`still_holds = … || !referenced.malformed.is_empty()` → a bare `Pending`). The sign-off's
instruction was: **KEEP the cluster-wide fail-safe block** — do **not** scope it to the
servers the corrupt record names (trusting a malformed vector's contents is exactly the
decision class #348 forbids) — **but surface WHY in the answer itself**: the malformed chunk
ids alongside `Pending`, pinned by a test.

Applied as a **richer status surface** (`desired_state.rs`):
- `ReconciliationStatus` gains a `PendingMalformed { chunks: Vec<ChunkId> }` variant
  (`desired_state.rs:98-101`); the enum drops `Copy` (a `Vec` field can't be `Copy`) but
  keeps `Debug, Clone, PartialEq, Eq` — every existing call site only compares by `==`
  (`rebalance.rs` tests), so nothing else breaks, and no `match` outside this module is
  affected (the only consumers are `assert_eq!`s; no exhaustive match exists to update).
- `reconciliation_status` (`desired_state.rs:154-180`) now distinguishes the two blocking
  reasons instead of collapsing them:
  - a **valid** committed placement resolving a fragment onto `dserver` → honest `Pending`
    (`:160-165`);
  - else, if any committed placement is **malformed**, the drain stays blocked
    **cluster-wide** (fail safe — `:174` returns `Satisfied` *only* when
    `referenced.malformed.is_empty()`) and the blocking chunk ids are **sorted and returned
    in the answer** as `PendingMalformed { chunks }` (`:177-179`), so an operator can
    attribute the stall to specific corruption rather than see an unexplained `Pending`.

The cluster-wide block is **preserved verbatim in spirit**: it is *not* scoped to the servers
the malformed vector names — a malformed vector's contents are untrustworthy, so any drain
could be the one it (corruptly) references. Attribution is added *on top of*, not *in place
of*, the fail-safe.

### T5b (the #349 short-placement supersession) — preserved verbatim

Per the sign-off ("RATIFIED at sign-off … preserve it verbatim, do not re-litigate"): a short
non-empty placement is malformed; `scrub` fails safe, `reconstruction` skips + NEEDS-HUMAN,
the record is never rewritten, the read path stays liberal. The two rewritten #349 tests
(`scrub.rs:short_placement_is_malformed_scrub_fails_safe`,
`reconstruction.rs:short_placement_is_malformed_reconstruction_skips_and_flags_needs_human`)
are carried over unchanged.

## Why a richer status surface, not an audit event (the sign-off allowed "and/or")

The carry-forward permits "richer status surface **and/or** an audit event on the loops'
existing `wyrd.custodian.*.audit` seam". I chose the status surface **alone**:

- `reconciliation_status` is a **query**, not a loop — it has no side effects today. Emitting
  a `tracing::warn!` + `monotonic_counter` from it would fire **on every poll** of a drain's
  status (operator dashboards poll), spamming the seam and the metric with duplicates for a
  single unresolved corruption. The four *loops* already emit the malformed / NEEDS-HUMAN
  audit events once per pass (ratified v1: `gc::emit_malformed`, `scrub::emit_malformed`,
  `reconstruction::emit_needs_human`, `rebalance::emit_needs_human`) — that seam is already
  covered. The missing piece was purely that the **status answer** was a black box; putting
  the chunk ids *in the answer* (the sign-off's "in the answer itself") fixes exactly that,
  side-effect-free.

## The rest of the change (ratified v1, reproduced) — one classifier, four loops

Unchanged from the ratified v1; summarised for the human (full detail was in v1 build-notes):
- **Single-source classifier** — `crates/core/src/metadata.rs`: `placement_is_valid()`,
  `checked_fragments() -> Result<_, MalformedPlacement>`, and the `MalformedPlacement`
  struct. `fragments()` (the read path) is untouched — liberal read preserved.
- **GC/scrub fail safe** — `gc::referenced_fragments` returns `ReferenceSet { placed,
  malformed }`; a malformed chunk is treated as fully referenced (`ReferenceSet::protects`)
  and audited (`emit_malformed`); scrub iterates `placed` only (no phantom repair) and
  audits.
- **Reconstruction / rebalance skip + NEEDS-HUMAN** — both classify via
  `checked_fragments()` *before* expanding; a malformed vector → `Assessment::Malformed`
  (reconstruction) / `continue` (rebalance), each emitting a NEEDS-HUMAN signal; the corrupt
  record is never repointed.

## Tests (red→green), driving production end-to-end

Named test file: `crates/custodian/tests/gc.rs`
(`malformed_placement_gc_treats_chunk_as_fully_referenced`, `gc.rs:642`). Companion legs:
`tests/rebalance.rs:malformed_placement_rebalance_skips_and_leaves_fragment_in_place` (now
pins the **attribution**: `PendingMalformed { chunks: vec![CHUNK] }`, `rebalance.rs:1324`),
`tests/scrub.rs`, `tests/reconstruction.rs`, and 4 classifier unit tests in
`crates/core/src/metadata.rs` (incl. the read-path-unchanged assertion).

Red proof (flippable): revert the classifier to permissive
(`metadata.rs` `checked_fragments`: `if true || self.placement_is_valid()`) so malformed
vectors are identity-filled. With that reverted, all four loop tests fail:
- `gc` → `malformed_placement_gc_treats_chunk_as_fully_referenced` FAILED (fragment reclaimed);
- `rebalance` → attribution test FAILED (the chunk is evacuated / `still_holds` via `placed`
  returns bare `Pending`, not `PendingMalformed`);
- `scrub` → phantom repair enqueued, FAILED;
- `reconstruction` → repoints over a fabricated vector, FAILED.

Green with the real classifier (this worktree, `cargo` = `~/.cargo/bin/cargo`):
custodian `gc` 9, `rebalance` 9, `scrub` 12, `reconstruction` 11 — all pass; `wyrd-core`
lib `metadata::tests` 4/4 pass. `cargo fmt -p wyrd-core -p wyrd-custodian -- --check` clean;
`cargo clippy -p wyrd-core -p wyrd-custodian --all-targets` clean (0 warnings).

## Test-runner / environment note (unchanged from v1 — host toolchain artifacts)

Two environment artifacts, **not** patch verdicts (the carry-forward predicted them):
1. **`cargo` is not on the gate shell's `PATH`.** It lives at `~/.cargo/bin/cargo`; I ran
   with `export PATH="$HOME/.cargo/bin:$PATH"`. `engine/xtask.sh` execs `cargo xtask` and
   will fail with `cargo: not found` unless the gate host puts cargo on PATH.
2. **`cc` is a `zig cc` shim that rejects the rustc triple `x86_64-unknown-linux-gnu`**,
   which an old `cc-rs` in a `wyrd-core` **dev-dependency** (`alloca`, pulled by `criterion`)
   emits — so building `wyrd-core`'s *lib test binary* fails for a reason wholly unrelated to
   this change. To actually run the metadata unit tests I set
   `CC_x86_64_unknown_linux_gnu=/tmp/cc-shim.sh`, a one-line wrapper that rewrites
   `--target=x86_64-unknown-linux-gnu` to zig's `-target x86_64-linux-gnu`. It changes **no
   code** — only the C-compiler invocation for that dev-dep. The custodian integration tests
   (the load-bearing red→green proof) don't pull `alloca` and need no workaround.
   `cargo deny` / `cargo machete` are not installed in this env (the patch touches no
   `Cargo.toml` / `Cargo.lock`, so `deny` is a no-op for it).

## Alternatives rejected (with cost)

- **Emit an audit event from `reconciliation_status` instead of the status surface** —
  rejected: it's a query polled repeatedly, so the event fires per-poll (unbounded duplicate
  warns/metric increments for one corruption). The status surface carries the attribution
  once, on demand, side-effect-free. (The loops already own the once-per-pass audit seam.)
- **Scope the block to the servers the malformed vector names** — rejected explicitly by the
  sign-off and by ADR-0040: a malformed vector's contents are untrustworthy, so scoping the
  drain-block to the ids it lists trusts exactly the bytes #348 says must not be trusted. The
  block stays cluster-wide; only the *explanation* is added.
- **Keep `Copy` by returning the ids out-of-band (e.g. a second return value / an out param)**
  — rejected: it fragments the single "what is this drain's status" answer into two values a
  caller must remember to read together. Dropping `Copy` costs nothing here (0 call sites rely
  on it; all 15 compare by `==`) and keeps one cohesive answer.
