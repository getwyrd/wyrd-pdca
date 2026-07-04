# Build notes — issue #255 / m4.4-server-backend-selection

Target branch: `feat/m4-production-metadata-backend`. All edits made in
`$PDCA_WORKTREE` (`/home/eddie/wyrd/wyrd.pdca-wt`). Slice 4 of accepted proposal
0015. Line citations below are against the target-branch `cli.rs` as it stood
pre-fix (the numbers the brief lists).

## Success criterion & what "done" means here

Criterion: `server` runs identically on redb (dev) and TiKV (prod) chosen **by
config**, via a composition change **confined to `crates/server`**; three couplings
removed. Binding Check-verifiable (redb path only; the tikv arm is `#[cfg]`-gated
out of the default build): parameterized `server` compiles, and its redb path
passes a red→green regression — roundtrip via the generic helpers on redb, and
`alloc_inode` returns a **bounded** error against a perpetual-`Conflict` store
instead of spinning.

The three couplings, each addressed:

1. **~8 concrete `RedbMetadataStore` sites** (`cli.rs:25,333,361,364,371,464,478,508`)
   → the local + cluster store helpers are now generic over `M: MetadataStore`
   (`alloc_inode`, `local_store_put`, `local_store_get`, `cluster_store_put`,
   `cluster_store_get`), and a `MetadataBackend { Redb, #[cfg(tikv)] Tikv }` config
   selector (`--metadata-backend` flag / `WYRD_METADATA_BACKEND` env, default redb)
   dispatches to the right concrete at each of the four store call sites. Governs:
   proposal 0015 §"Composition, not refactor" item 1, §"Crate touch-points" `server`
   bullet.
2. **`pollster::block_on` local paths** (`cli.rs:157,211`) → both local `put`/`get`
   paths now run on the `tokio` runtime the cluster paths already used
   (`tokio_runtime()`, formerly `cluster_runtime()` at `cli.rs:517`). A `tokio`-bound
   TiKV client cannot run under `pollster::block_on`. `demo` (`cli.rs:338`) stays on
   `block_on` — it is an in-memory-redb smoke check, not a selectable-backend path.
   Governs: §"Composition, not refactor" item 2.
3. **`alloc_inode`'s unbounded `Conflict` spin** (`cli.rs:371-387`) → bounded
   retry-with-backoff: capped exponential backoff (2ms → 64ms) over
   `ALLOC_INODE_MAX_ATTEMPTS = 8`, then a typed exhaustion `Err`. The new error path
   is threaded through its callers via the existing `?` (all callers already return
   `Result<_, BoxError>`). Governs: §"Composition, not refactor" item 3.

## Confinement (the BINDING claim)

Diff outside `metadata-tikv` is confined to `crates/server` (`cli.rs`, `Cargo.toml`)
plus the new test and the `Cargo.lock` edge. `core`/`custodian`/`traits` are
byte-for-byte untouched by this diff (verified: `git diff --stat` lists only
`Cargo.lock`, `crates/server/**`). Note the brief's caveat: `core` already carries
`wyrd-metadata-redb` under `[dev-dependencies]` (pre-existing, dev-only, proposal
0015:346-347) — untouched here.

## The tikv arm is off-Check by construction

`server` gains an OFF-by-default `tikv` feature (`server/Cargo.toml`) forwarding to
`wyrd-metadata-tikv/tikv` (mirror of `metadata-tikv/Cargo.toml:11-20`);
`wyrd-metadata-tikv` is an `optional` dep. The `MetadataBackend::Tikv` variant, its
selection arms, and `open_tikv_meta()` are all `#[cfg(feature = "tikv")]`. So:
- the **default** build (what `cargo xtask ci` runs) never pulls the `tikv-client`
  tree — verified: default `cargo test -p wyrd-server` compiled with no
  `tikv-client` in the graph; `cargo tree -p wyrd-server --features tikv` shows the
  tree only *with* the feature on;
- the tikv arm's compile+run belongs to `cargo xtask tikv-conformance` /
  `--features tikv` (needs `pkg-config`+`libssl-dev`+network), deliberately NOT run
  here. This matches the brief's "Deferred (off-Check)".
- `deny.toml` is **not** touched (deferred to #420 item 2 by human decision
  2026-07-04, per the brief); the feature-off default keeps `cargo deny` green.

## Test — `crates/server/tests/backend_selection.rs`

Integration test (headless, load-light: pulls only redb + traits + a tiny mock, no
GUI/IO). Two assertions:
- (a) `MetadataBackend::from_config` picks redb by default / on `"redb"`, rejects an
  unknown name; the **generic** `alloc_inode` drives redb to monotonic, persisted
  inodes (1, 2) — the roundtrip through the parameterized helper.
- (b) `alloc_inode` against an `AlwaysConflict` mock `MetadataStore` returns a
  bounded `Err` (wrapped in a 5s `tokio::time::timeout` safety net).

The mock is only constructible because `alloc_inode` is now generic over `M` and
public, so the test **load-bears the parameterization seam** — this is what makes
the red honest.

### Red → green proof (via the project's cargo toolchain in the worktree)

- **Green:** `cargo test -p wyrd-server --test backend_selection` → 2 passed.
- **Red:** stashing the production `cli.rs`/`Cargo.toml` changes (keeping the test)
  makes the test **fail to compile** — `no MetadataBackend in cli` and `function
  alloc_inode is private` (E0432/E0603): the seam is genuinely absent without the
  fix. Restored the fix; green again. This mirrors what the C4-verify gate does
  (revert production, keep the test).

Why not run the full `cargo xtask ci` for the red→green sanity: there is no
targeted xtask subcommand and `ci` is the whole heavy gate (clippy workspace + DST
under madsim + deny + conformance); the scoped `cargo test -p wyrd-server` is the
fast pre-fix/post-fix pass. Check's gates re-run the real suite. The bounded test
carries its own 5s internal timeout, so it cannot hang the runner.

## Alternatives considered / rejected

- **`Box<dyn MetadataStore>` for the selector** (one erased handle instead of a
  monomorphized match per arm) — rejected: the proposal's whole thesis is that
  selection is "pass a different concrete" static composition (0015:352-354,
  §"Composition, not refactor"), and `Gateway<M>`/`core` are generic over `M`, not
  `dyn`. A `dyn` reach-through would be the very "config/feature reach-through" the
  proposal says it is *not*. The generic-body-per-arm pattern is ~10 lines × 4 call
  sites; the `dyn` route would also add trait-object-safety friction against the
  already-generic consumers. Cost of the chosen form: the two cluster helpers gained
  one extra `<M: MetadataStore>` param each; existing callers
  (`gateway_cluster.rs:98,121,140`) infer `M = RedbMetadataStore` unchanged —
  verified: the full `wyrd-server` suite (all pre-existing tests) stays green.
- **Bounding `alloc_inode` by wall-clock deadline instead of attempt count** —
  rejected: attempt-count + capped exponential backoff is deterministic and simpler
  to reason about for a dev-CLI allocator; the deployable `Gateway` uses an in-proc
  `AtomicU64` and has no such loop (0015:384-386), so this path is dev-CLI only and
  needn't be tuned to a latency SLO.
- **Moving `demo` onto tokio too** — rejected: unnecessary churn; `demo` is an
  in-memory-redb smoke path with no backend selection, and keeping it on `block_on`
  keeps the diff to the paths that actually gain a selectable backend.

## Commit-readiness

`cargo fmt --all -- --check` clean; `cargo clippy -p wyrd-server --all-targets -D
warnings` clean; `cargo-machete crates/server` clean (the cfg-gated
`wyrd-metadata-tikv` usage is detected, so no `[package.metadata.cargo-machete]`
ignore is needed); `Cargo.lock` updated (the new intra-workspace edge only). Full
`wyrd-server` test suite green.

## Not done here (out of scope, per brief)

`metadata-tikv`'s own `commit`/`scan` (M4.2/#253, M4.3/#254 — already on the branch);
#420 both items (nightly TiKV CI; `deny.toml` tikv-tree audit); `deploy/` stack
(M4.5); Jepsen/Tier-1/2 (M4.6); DST second-impl pin (M4.7); any `MetadataStore`
trait change.
