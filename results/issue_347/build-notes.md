# Build notes — issue #347 (chunkref-fragments-placement-expansion-helper)

## What I built

`ChunkRef::fragments()` in `crates/core/src/metadata.rs:126-144` (target branch
`getwyrd/wyrd@main`, base `9f8d3d9`):

```rust
pub fn fragments(&self) -> impl Iterator<Item = (u16, DServerId)> + '_ {
    (0..self.fragment_count()).map(move |i| (i, self.placed_dserver(i)))
}
```

placed directly after `placed_dserver` (`metadata.rs:119-124`), matching ADR-0040
decision 2's signature and semantics verbatim (`../wyrd/docs/design/adr/0040-mixed-era-placement-expansion.md:61-62`):
"`ChunkRef::fragments() -> impl Iterator<Item = (u16, DServerId)>` over the full
index space... `fragment_count()` and `placed_dserver()` remain its primitives" —
and liberal per decision 2 (`0040:66-67`): "applies the identity fallback
unconditionally and does *not* validate length." I did not reimplement the
fallback logic inside `fragments()` — it delegates to `placed_dserver` per fragment
index, so `fragments()` cannot drift from the read path's resolution by
construction (the same reason the doc comment states the equivalence as a
structural fact, not an assertion to keep in sync by hand).

Also updated the `placed_dserver` doc comment (`metadata.rs:110-118`, brief scope
line 59-60) to add rebalance to the caller list, since `plan_evacuations` now
transitively calls it through `fragments()`.

### Routing the three read-expansion consumers

- `crates/custodian/src/gc.rs:197` (was `gc.rs:197-205` pre-fix) —
  `referenced_fragments` now does `for (index, dserver) in chunk.fragments()`
  instead of `for index in 0..chunk.fragment_count() { ... chunk.placed_dserver(index) ... }`.
- `crates/custodian/src/reconstruction.rs:230` (was `reconstruction.rs:230-232`
  pre-fix) — `assess` builds its initial `placement: Vec<DServerId>` via
  `chunk_ref.fragments().map(|(_, dserver)| dserver).collect()` instead of
  `(0..chunk_ref.fragment_count()).map(|i| chunk_ref.placed_dserver(i)).collect()`.
- `crates/custodian/src/rebalance.rs:166` (was `rebalance.rs:165-167` pre-fix,
  fixed under #346/PR #357) — `plan_evacuations` builds its `placement` the same
  way.

I left every downstream use of the resulting `placement: Vec<DServerId>` inside
`assess` / `plan_evacuations` untouched (the read of `stores.get(&dserver)`, the
`evac` filter, `survivor_domains`, `RepairPlan`/`EvacPlan` construction) — those
consume the already-resolved vector; nothing about the routing changes their
behaviour, which is the point (a pure centralization, brief line 41: "The change
is behaviour-preserving").

### What stayed untouched (brief's explicit out-of-scope list, verified)

- `read.rs:104-105` (`fragment_dserver`) — still a bare
  `chunk.placed_dserver(index)`, the per-index read-path use the brief says
  "stays."
- The write/repoint sites, confirmed unchanged by `grep -rn '\.placement\b'
  crates/custodian/src crates/core/src` post-patch: `write.rs:84,103,234`;
  `reconstruction.rs:386,404,416` (shifted 2 lines from the brief's cited
  388/406/418 by my comment edit two lines shorter); `rebalance.rs:238,241,270`
  (shifted similarly from 239/242/263/271). These build/index the *plan* structs'
  `.placement` field (`RepairPlan.placement`, `EvacPlan.placement`) or commit a
  freshly-materialized full-length vector — not `ChunkRef.placement` iteration —
  and are decision-5 territory, not decision-2's.
- `#348`'s `checked_fragments()` / `placement_is_valid()` companion and the
  strict/malformed-length maintenance classification: not built. `fragments()`
  stays liberal per decision 2; the doc comment says so explicitly and points at
  #348 for the fallible companion, so a future reader isn't tempted to bolt
  validation onto this helper.
- `#360`'s CI grep-gate: not built (explicitly ordered *after* this issue in the
  brief's Ordering note, so it has a green tree to protect).

## Alternatives considered and ruled out

**Inlining the fallback logic in `fragments()` instead of delegating to
`placed_dserver`.** E.g. `(0..n).map(|i| (i, self.placement.get(i as usize).copied().unwrap_or(i as u64)))`.
Rejected: it re-encodes the identity-fallback rule a second time in the same
`impl` block, which is exactly the "single definition" ADR-0040 decision 1 rules
out — the two would have to be kept in sync by hand instead of `fragments()`
being *structurally* incapable of disagreeing with `placed_dserver` (which the
delegating version is, by construction — no diff to review for behavioural
drift). Cost of the rejected form: same line count (~3 lines), so this isn't a
cost tradeoff — it's a correctness/invariant one (per the brief's "Invariant to
restore" framing, cost-vs-minimalism isn't the deciding axis here anyway).

**A free function `fragments(chunk: &ChunkRef) -> impl Iterator<...>` instead of
a method.** Rejected: `fragment_count()` and `placed_dserver()` are both
inherent methods on `ChunkRef` (`metadata.rs:103,119`), and the brief's Planning
artifact citation is the method signature `ChunkRef::fragments()`
(ADR-0040 decision 2, `0040:61`) — a free function wouldn't match the brief's
BINDING signature clause.

**Returning `Vec<(u16, DServerId)>` instead of `impl Iterator`.** Rejected:
ADR-0040 decision 2 states the return type verbatim as
`impl Iterator<Item = (u16, DServerId)>` (`0040:61-62`) — that's BINDING per the
brief's success criterion, not a style choice to relitigate. It also lets the
three consumers `.collect()` into whatever shape they need (`gc.rs` inserts into
a `HashSet` per-item via a `for` loop; `reconstruction.rs`/`rebalance.rs`
`.collect()` into `Vec<DServerId>` via `.map(|(_, d)| d)`) without an
intermediate allocation `fragments()` itself doesn't need.

**Changing `assess`/`plan_evacuations` to iterate `chunk_ref.fragments()`
directly in their main loop** (dropping the intermediate `placement: Vec<DServerId>`
entirely, indexing `chunk_ref.fragments()` fresh per iteration) instead of
collecting once up front. Rejected: both functions index the resolved
placement multiple times later in the same function by raw fragment index
(`reconstruction.rs:245` `stores.get(&dserver)` inside a `for (index, &dserver)
in placement.iter().enumerate()`; `rebalance.rs:169-186` filters/enumerates
`placement` twice for `evac` and `survivor_domains`) — collecting once into a
`Vec<DServerId>` and reusing it is what the pre-fix code already did (I preserved
that shape) rather than re-walking `fragments()` (which re-invokes
`placed_dserver` per index, an O(n) `Vec` lookup — negligible per-chunk, but
re-collecting twice is still wasted work for no behavioural gain). This keeps the
diff to swapping the *expansion expression* only, not restructuring the
functions' control flow — smaller, more reviewable, and exactly what "pure
centralization" (brief line 41) calls for.

## Test

`crates/core/tests/placement_record.rs:268-377` (new `mod fragments_matrix`),
the brief's named test file (the existing placement-record test home, brief line
78-80). Six `#[test]` fns plus a shared `assert_matches_placed_dserver` helper
that checks `fragments()` against an independently-computed
`(0..fragment_count()).map(|i| (i, placed_dserver(i)))` walk (so the test doesn't
just re-derive the same expression `fragments()` uses internally — it recomputes
the expected value via the primitive `placed_dserver`, which is the actual
oracle ADR-0040 decision 1 names):

- `none_empty_placement_is_pure_identity` / `none_full_placement_resolves_from_record`
  — `EcScheme::None` (`fragment_count() == 1`), empty and full (`len == 1`).
- `none_malformed_length_placement_still_resolves_liberally` — `EcScheme::None`'s
  "short" case is degenerate: `fragment_count() == 1` means a non-empty vector
  can never be *shorter* than the index space (the only length below 1 is 0,
  already the empty case), so the only reachable non-empty-wrong-length shape is
  *longer*. I used a length-2 vector to exercise "non-empty, `len !=
  fragment_count()`, no validation" for `None` — documented inline why the
  literal "short" framing doesn't apply to this scheme.
- `rs_empty_placement_is_pure_identity` / `rs_full_placement_resolves_from_record`
  / `rs_short_placement_mixes_record_and_identity_fallback` — `ReedSolomon{k:6,m:3}`
  (`fragment_count() == 9`): empty, full (`len == 9`), and short (`len == 4`,
  a genuine `len < fragment_count()` case, so it exercises the interesting
  per-index mix of record-resolved and identity-fallback entries in one chunk).

This covers the brief's success criterion literally: "asserts `fragments()`
yields exactly the per-index `placed_dserver` resolution for `EcScheme::None`
and `ReedSolomon{k,m}` across empty, full ... and short placement vectors."

### Red -> green, run through the project's own runner

Verification posture per the brief (line 72-77): NET-NEW coverage, so "red" is
criterion-absence (the helper doesn't exist / doesn't compile), not a flipped
assertion.

I used `cargo test` (Wyrd's own test invocation — the exact command
`cargo xtask ci` itself runs at `xtask/src/main.rs:550`,
`cargo test --workspace --exclude wyrd-dst`), scoped by package via the Bash
tool (which supplies the timeout), rather than the full `cargo xtask ci`. I did
not skip `cargo xtask ci` for convenience — I ran its other steps directly too
(see below) and the ONLY step I didn't reproduce standalone is the 50-seed
madsim DST sweep (`run_dst`, `xtask/src/main.rs:566+`), which this change cannot
affect (`ChunkRef::fragments()` is a pure, synchronous, non-DST-instrumented
helper; DST exercises the commit-protocol/simulated-fault surface, not
`metadata.rs` chunk-map expansion) and which risks the Bash tool's 10-minute cap
on this box. Check's `cargo xtask ci` gate (`C4-ci`, gating) re-runs the whole
thing including DST.

Steps, in the `$PDCA_WORKTREE` checkout (`/home/eddie/wyrd/wyrd.pdca-wt`, HEAD =
`9f8d3d936db00ff26e02f34b11fd8fd7a2903f03` = `origin/main` tip):

1. **RED**: `git stash push --keep-index -- crates/core/src/metadata.rs
   crates/custodian/src/gc.rs crates/custodian/src/rebalance.rs
   crates/custodian/src/reconstruction.rs` (reverts the four production files,
   keeps the test-file edit) → `cargo test -p wyrd-core --test placement_record`
   → **7 compile errors**, `error[E0599]: no method named 'fragments' found for
   struct 'ChunkRef'`, at every new test call site. This is the brief's
   criterion-absence red.
2. `git stash pop` — restored all five files.
3. **GREEN**: `cargo test -p wyrd-core --test placement_record` → all 8 tests in
   the file pass (6 new + the 2 pre-existing `write_records_placement...` /
   `moved_fragment_resolved...` regression tests, confirming I didn't disturb
   them).
4. **Consumer regression**: `cargo test -p wyrd-custodian` → all pre-existing GC
   (6) / rebalance (7) / reconstruction (8) / scrub (6) / skeleton (3) tests
   still pass — the routing is behaviour-preserving, as the brief requires
   (line 41, line 76-77: "stays covered by the existing custodian GC /
   rebalance / reconstruction tests, which must remain green").
5. `cargo fmt --all -- --check` → clean, no diff.
6. `cargo clippy -p wyrd-core -p wyrd-custodian --all-targets` → clean (the
   workspace denies all clippy + rustc warnings via `[workspace.lints]`,
   `Cargo.toml:144,151`, so a clean run here is a real signal, not just "no
   flag passed").
7. `cargo build --workspace --exclude wyrd-dst --all-targets` → clean (the same
   scope `cargo xtask ci`'s build step covers).
8. `cargo test --workspace --exclude wyrd-dst` → every crate's test suite green
   (the same scope `cargo xtask ci`'s test step covers, i.e. everything `ci`
   does except fmt/clippy/build I already ran above, cargo-machete, cargo-deny,
   conformance vectors, and the DST sweep).

I did not run `cargo-machete` / `cargo-deny` / `run_conformance` standalone:
this patch adds no dependency, touches no `Cargo.toml`, and touches no on-disk
format / conformance-vector surface, so none of those three checks can be
affected by it — re-running them would validate the untouched 99.9% of the tree,
not this change.

## Formatter / commit-hook readiness

`cargo fmt --all -- --check` (step 5 above) passed with no diff on every file
`patch.diff` touches — the patch is exactly what `rustfmt` (Wyrd's configured
formatter, invoked by `cargo xtask ci`'s first step, `xtask/src/main.rs:533`)
already produces, so there is nothing for the target's own commit hook to
reformat.

## Scope check against the brief

Brief line 56-60 scope: "add `ChunkRef::fragments()` ... alongside
`fragment_count()`/`placed_dserver()`; route the three hand-rolled
read-expansion consumers ... through it; document `fragments()` and update the
`placed_dserver` doc comment ... to list all callers including rebalance." All
four done, nothing else touched — `git diff --stat` shows exactly 5 files
(the one production helper file, the three consumer files, the one test file),
matching the brief's difficulty note ("blast-radius: ~5 files, all localized
single-site edits", line 68-71).
