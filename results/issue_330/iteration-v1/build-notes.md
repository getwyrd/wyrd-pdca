# Build notes — issue 330 / scrub-detect-missing-placed-fragment

## What I changed, and why

**Site:** `crates/custodian/src/scrub.rs:54-176` (`scrub::reconcile`), plus a doc-only
follow-up in `crates/traits/src/lib.rs:253-263` (`ChunkStore::list_fragments` doc, now
stale about who walks it) and the module doc header `scrub.rs:1-35`.

Pre-fix, `reconcile` drove its walk off `store.list_fragments().await?`
(`scrub.rs:62`, pre-fix) and only *then* filtered to the fragments a committed chunk
map references (`scrub.rs:64-66`, pre-fix). A fragment that is placed by the chunk map
but genuinely **absent** from the store never appears in `list_fragments()`'s own
output — by definition, an absent fragment isn't in the listing of what's present — so
the loop body that checks `get_fragment` and could classify `Ok(None)` never even runs
for it. The `Ok(None) => continue` arm (`scrub.rs:92`, pre-fix) only ever fired for the
narrow "vanished between the walk and the fetch" TOCTOU case, and even there did
nothing but skip — no obligation, ever, for plain absence.

**The fix inverts which set drives the walk.** `reconcile` now groups the *already
computed* reference set (`referenced_fragments`, unchanged — same helper GC's safety
gate uses, `gc.rs:179`) by placed D server (`scrub.rs:73-77`, new `by_dserver` map),
and for each `(dserver, fragment)` pair calls `store.get_fragment(frag)` **directly**
— never consulting `list_fragments()` at all. `ChunkStore::get_fragment`'s own contract
(`traits/src/lib.rs:249-251`, unchanged) is exactly "`Ok(None)` if this store holds no
fragment for `id`" — so asking for precisely the fragment the chunk map places there
gets a truthful, unambiguous absence signal, which the old list-driven walk could never
produce. `Ok(None)` now gets its own arm (`scrub.rs:129-141`) that mirrors the
corruption arm one-for-one: emit a durability finding (`emit_missing`, `scrub.rs:196-207`,
new `scrub_missing_detected` counter + `action = "missing"` audit event, parallel to
`emit_corruption`) and `repair::enqueue_repair(ctx.meta, frag.chunk, "scrub")` — the
same shared queue (`repair.rs:78`) the corruption arm and the read path both feed.

## Alternative considered and ruled out (with cost)

**Alternative: keep the `list_fragments()`-driven walk, and separately diff
`referenced` against what `list_fragments()` returned** (i.e., after the existing loop,
compute `referenced - {(dserver, f) : f in store.list_fragments()}` per dserver and
enqueue the difference). This is strictly *more* code than the chosen fix — it keeps
the existing loop unchanged, then needs a second pass building a per-dserver `HashSet`
from `list_fragments()` and a set-difference against `referenced` (roughly the same
~15-line grouping helper the chosen fix already needs, *plus* the original loop body
kept intact, i.e. +~15 lines net). It buys nothing over calling `get_fragment` directly
per referenced fragment: `list_fragments()` still has to enumerate the *entire* store
just to compute a diff, where the direct approach fetches exactly the N referenced
fragments and gets the same Ok(None)/Ok(Some)/Err(fault) trichotomy `get_fragment`
already exposes, with no second pass. So the alternative is strictly heavier for no
behavioural gain — ruled out.

**Alternative: keep the missing-fragment enqueue in the `read.rs` `corrupt.push` path
instead of / in addition to scrub** (the brief's cited `crates/core/src/read.rs:151,158`).
Ruled out because it is read-triggered, not a *production maintenance path* — the
Success criterion is explicit that a scrub reconciliation pass (`reconcile_step` → scrub)
must produce the obligation on its own, without needing a read to occur first. Read-path
detection (already present for corruption) stays as-is; this brief's scope is the
proactive scrub loop.

## False-positive guardrails (brief's binding condition)

- **Unreferenced/orphan fragments:** structurally impossible to false-positive — the
  walk is now driven *entirely* by the `referenced` set (`by_dserver`, built solely
  from `referenced_fragments`); an orphan is never in that set, so `get_fragment` is
  never even called for it. (Pinned by the pre-existing
  `walks_and_verifies_referenced_fragments_through_reconcile_step` test, which still
  passes post-fix — an unreferenced-but-corrupt fragment produces no finding.)
- **In-flight (pending, uncommitted) writes:** `referenced_fragments` (`gc.rs:184-186`)
  filters to `InodeState::Committed` only; a pending inode's provisional chunk map is
  never included. Since Wyrd's four-phase write commits the chunk map only after
  *every* fragment has acked (`crates/core/src/write.rs:220`, "M2 commits only after
  all `n` ack"), a committed reference's bytes are guaranteed to already exist at
  commit time — so there is no window where a legitimately-in-flight fragment could be
  mistaken for a loss. Pinned by the new negative test
  `does_not_flag_an_in_flight_pending_writes_fragment_as_missing` (below).
- **Pending-GC / grace-window races:** GC's own safety gate (`gc.rs:126-129`) never
  reclaims a fragment in the `referenced` set — the same set scrub now walks. A
  fragment scrub finds via `get_fragment` genuinely `Ok(None)` therefore cannot be a GC
  fragment mid-grace-window (GC only reclaims orphan/expired-pending fragments, both
  disjoint from `referenced` by construction), so `Ok(None)` on a referenced fragment
  can only mean genuine loss.
- **Killed/partitioned D server (explicitly out of scope):** an unreachable server's
  `get_fragment` calls fail with a *transient* (non-`IntegrityFault`) `Err`, which the
  existing `Err(e) => return Err(e)` arm (unchanged) propagates rather than enqueues —
  matching the brief's scope note that this needs a separate desired-state/topology
  detector. Pinned by the pre-existing `scrub_propagates_a_transient_get_fault_without_enqueuing`
  test, unchanged and still green.

## Tests added (`crates/custodian/tests/scrub.rs`)

1. `detects_a_missing_placed_fragment_and_enqueues_for_reconstruction` — the brief's
   repro: commits a chunk map placing a fragment on `d0`, but `d0` never receives any
   bytes for it (no `put_fragment` call at all — simply absent, not corrupt). Asserts
   `reconcile_step` returns `Reconciled::Changed` and the chunk is on
   `repair::queued_repairs`. This is the central, flippable leg: reverting the new
   `Ok(None) => { emit_missing(...); enqueue_repair(...); }` arm back to
   `Ok(None) => continue` makes this test fail (`Reconciled::Satisfied` / empty queue).
2. `does_not_flag_an_in_flight_pending_writes_fragment_as_missing` — the false-positive
   guardrail: an `InodeState::Pending` record (created via `metadata::create` directly,
   bypassing `commit_reference`/`commit_chunk`'s `Committed` state) referencing a
   fragment `d0` doesn't have. Asserts `Reconciled::Satisfied` and an empty repair
   queue — confirming a not-yet-committed write is never treated as a loss.

Both were run through the target's own runner, `cargo test` inside the Wyrd checkout
(the same command `cargo xtask ci`'s `run_ci` step issues, `xtask/src/main.rs:550`),
scoped to the touched package for a fast sanity pass rather than the full `ci` (fmt +
clippy + build + test + machete + deny + conformance + statics + dst) that Check's
`C4-ci`/`C4-verify` gates run in full:

- **Pre-fix (red):** `git stash` the two production files
  (`crates/custodian/src/scrub.rs`, `crates/traits/src/lib.rs`), keeping the new test
  file, then `cargo test -p wyrd-custodian --test scrub`:
  `detects_a_missing_placed_fragment_and_enqueues_for_reconstruction` **FAILED**
  (`left: Satisfied, right: Changed`); all 11 other tests (including the new negative
  guardrail test, which is fix-independent) passed. `git stash pop` restored the fix.
- **Post-fix (green):** same command, all **12/12** tests pass.
- Also ran the fuller `cargo test -p wyrd-custodian` (all custodian test binaries:
  `gc.rs`, `gc_telemetry.rs`, `rebalance.rs`, `reconstruction.rs`, `scrub.rs`,
  `skeleton.rs`) — **44/44 pass**, confirming no regression in the sibling maintenance
  loops that share `referenced_fragments`/the fleet-walk shape.
- `cargo fmt --all -- --check` — clean (no diff) over the whole workspace.
- `cargo clippy -p wyrd-custodian -p wyrd-traits --all-targets` — clean, no warnings.
- `cargo doc -p wyrd-custodian --no-deps` / `-p wyrd-traits --no-deps` — the intra-doc
  link errors surfaced (`reconciliation.rs:58-60`, `rebalance.rs:65`,
  `reconstruction.rs:63`, `traits/lib.rs:12` `async_trait` ambiguity) are **all
  pre-existing on `main`** (confirmed by re-running `cargo doc` with the production
  files stashed) and untouched by this patch; `cargo doc` is not part of `run_ci`
  (`xtask/src/main.rs:530-558` runs fmt/clippy/build/test/machete/deny/conformance/
  statics/dst only) so it isn't a gate regression either way. I edited my own new doc
  text (`scrub.rs:10`) to avoid *adding* a new such warning (switched a
  `[`referenced_fragments`]` intra-doc link to plain code-span text, since it is
  `pub(crate)` and unresolvable without `--document-private-items`, matching how the
  rest of the file already refers to private items in prose).

### Environment note

This sandbox ships no Rust toolchain and no C compiler by default (no `cargo`, no
`cc`/`gcc`). I installed `rustup` (pinned by the repo's `rust-toolchain.toml` to
1.96.0/1.96.1) and, since `apt`/`sudo` are blocked for this agent, used a user-local
`pip`-installed `ziglang` (`zig cc`) as the C linker driver (`CC`/`cc` on `PATH`) so
`cargo` could link. A full `cargo build --workspace --all-targets` (which also builds
Criterion benches) hits one unrelated, pre-existing build-script incompatibility
(`alloca v0.4.0`, a `criterion` dev-dependency of `wyrd-core`'s benches, choking on
`zig cc`'s `--target` query format) — irrelevant to this fix (no bench target touches
scrub/gc, and `cargo test -p wyrd-custodian` never needs to build benches at all, so it
built and ran clean). Not a real-CI concern: the project's actual `C4-ci`/`C4-verify`
gates run with a real `cc`, where this shim workaround is unnecessary.

## Scope discipline

Left untouched, per the brief's explicit out-of-scope list:
- The killed/partitioned D-server case (unreachable server, `list_fragments()`/
  `get_fragment` failing entirely) — still propagates as a transient `Err`, unhandled
  by design; needs a desired-state/topology-aware detector, not this fix.
- The reconstruction/dequeue/re-placement path — untouched; `scrub::reconcile` still
  only ever *produces* obligations (no `delete_fragment` call anywhere in the diff).
- The `#250`/`#196` `enqueue_repair` test stand-ins
  (`crates/chunkstore-grpc/tests/tier2_kill_reconstruct.rs:545`,
  `tier1_jepsen_consistency.rs`) — not removed; a documented follow-up, per the brief.
