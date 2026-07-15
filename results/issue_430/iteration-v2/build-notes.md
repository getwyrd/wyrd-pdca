# Build notes — issue 430 / fragment-identity-validation (iteration 2)

## Status of the previous attempt

Iteration 1's *design* was not rejected on merits — the driver auto-iterated because the
artifact-only reviewer could not, from artifacts alone, **independently reproduce the red
leg** (it cannot stash/revert the target) and native `cargo xtask ci` stopped on an
unrelated loopback-bind `PermissionDenied` in the sandbox. Those are Check-side
verification gaps, not a wrong approach. So iteration 2 keeps the same minimal
invariant-restoring change (it is the smallest change that restores the invariant the
brief names) and closes the verification gap: I applied the fix in `$PDCA_WORKTREE`, drove
the new test **red with production reverted** and **green with it applied** through the
project's cargo toolchain, and ran the affected `wyrd-core` / `wyrd-custodian` /
`chunkstore-grpc` suites + `fmt --check` + `clippy -D warnings` here. The red→green
transcript is reproduced below so Check has the evidence the artifact reviewer could not
generate itself.

## The defect (root cause)

The shared read/repair validation admitted a decoded fragment on `chunk_id` **alone**:

- `repair::fragment_intact` / `repair::intact_shard` matched only
  `decoded.header.chunk_id == chunk` (`crates/core/src/repair.rs:53-55`, `:66-71` on `main`).
- The read path's inline decodes mirrored that half-check — the single-fragment path
  (`crates/core/src/read.rs:234`) and the RS fan-out (`crates/core/src/read.rs:319`).

A backend that returned a validly-encoded fragment of the SAME chunk at the WRONG
`ec_fragment_index` — or one whose header EC tuple (`ec_scheme_type`/`ec_k`/`ec_m`,
`crates/chunk-format/src/header.rs:113-119`) disagreed with the committed
`ChunkRef.scheme` — passed the gate, and the RS read pushed its payload under the
*requested* index (`crates/core/src/read.rs:319-321`): wrong reconstruction input →
silently wrong bytes, no repair enqueue. Only `FsChunkStore::verify` independently checked
chunk **and** index (`crates/chunkstore-fs/src/lib.rs:115-130`), masking the gap for
today's fs backend while leaving the assurance in the backend, not the shared core code.

## The invariant restored (and where) — smallest change that restores it

A fragment is admitted into any read/repair/maintenance path only when its decoded header
proves the FULL identity requested: `chunk_id`, `ec_fragment_index`, and (for RS) an EC
tuple consistent with the committed scheme — enforced **in the shared core code** for ANY
backend, per the brief's "Invariant to restore".

- **One new predicate** `repair::header_matches_identity(header, expected: FragmentId,
  scheme: EcScheme)` (`crates/core/src/repair.rs:248-271` on the patched tree) — the single
  definition of "against the chunk map", mirroring `FsChunkStore::verify` (chunk + index)
  and *widening* it with the committed EC tuple only the core layer holds.
- `repair::fragment_intact` / `repair::intact_shard` **widened** to
  `(bytes, expected: FragmentId, scheme: EcScheme)` and routed through the predicate
  (`crates/core/src/repair.rs:289-320`). This is the brief's suggested shape; Do owns the
  exact signatures.
- Read-path inline gates now call `header_matches_identity` at both decode sites —
  `EcScheme::None` path (`crates/core/src/read.rs:234` region) and RS fan-out
  (`crates/core/src/read.rs:319` region) — same gate, no drift.
- Custodian call-sites pass the expected identity/scheme through:
  - `reconstruction.rs` — `intact_shard(b, frag, chunk_ref.scheme)` (`:389`; `chunk_ref` is
    already in scope at `crates/custodian/src/reconstruction.rs:325`).
  - `rebalance.rs` — `fragment_intact(&bytes, frag, plan.prior.chunk_map[plan.chunk_index].scheme)`
    (`crates/custodian/src/rebalance.rs:264`).
  - `scrub.rs` — `fragment_intact(&bytes, frag, scheme)` (`crates/custodian/src/scrub.rs:119`),
    where `scheme` is threaded via a new `schemes: HashMap<ChunkId, EcScheme>` on
    `gc::ReferenceSet` populated alongside `placed` in `referenced_fragments`
    (`crates/custodian/src/gc.rs:192,213,240,251`). Scrub previously carried only
    `(dserver, FragmentId)`; the committed scheme was the one piece missing to check the
    header's full identity.

## Why widen the helpers rather than add parallel ones (footgun vs. churn)

The brief names `fragment_intact` / `intact_shard` as *the* defective gate ("The same
helpers are the gate in reconstruction, scrub and rebalance"). Leaving them `chunk_id`-only
and adding parallel full-identity helpers would leave a half-verification function in the
shared API — exactly the "verification against half the chunk map" the Invariant forbids; a
future admission call to `fragment_intact` would silently reintroduce the bug. So I widened
the signatures.

Concrete cost of that choice (measured, not asserted): **3 production call-sites** +
**~18 test-assertion call-sites** across the 15 touched files (see `git diff --stat`:
601 insertions / 110 deletions, of which the 303-line new test is the bulk). Each test
caller verifies a rebuilt/placed fragment for a *known* index and scheme, so each became
`fragment_intact(&bytes, frag(i), EcScheme::…)` — mechanical. This churn is inherent to
fixing the *shared* gate; it is not scope creep, and it is smaller than the alternative
(which keeps the footgun *and* still needs every caller audited).

## Test-fixture corrections (production change legitimately invalidated them)

Several existing tests wrapped RS shards with `FragmentHeader::new_v1` (`ec_scheme_type =
None`, `ec_k = 1`, `ec_m = 0`, `ec_fragment_index = 0`) — headers the production **write**
path never stamps for an RS chunk (it uses `write::encode_ec_fragment`,
`crates/core/src/write.rs:116-123`). Once the read path checks index+scheme, those
unrealistic fixtures are correctly rejected. Corrected to stamp the real RS header:
`read_repair.rs` (4 RS tests + 2 `#530` audit tests), `mutation_regressions.rs`
(`read_with_fewer_than_k_fragments_reports_insufficient`, plus removing the now-unused
`fragment()` helper and its `wyrd_chunk_format` import to keep clippy green). Scrub's tests
use `EcScheme::None` chunks (correct `new_v1` headers) and needed no fixture change.

## Test — `crates/core/tests/fragment_identity.rs` (NEW, 303 lines)

Two `#[tokio::test]`s drive the PUBLIC surface (`read::read_object` +
`repair::queued_repairs`) over an in-memory test-double store (mirrors `MemChunks` in
`crates/core/tests/read_repair.rs:74-151`), so a production revert fails them by
**assertion**, not compile error (the brief's explicit "shape the red honestly" constraint):

1. `ec_read_rejects_a_same_chunk_wrong_index_fragment` — slot 0 serves index 1's fragment
   (header `ec_fragment_index = 1`), slot 1 correct, slot 2 absent (only `k` served, one
   wrong-identity — the brief's deterministic-red recipe: the RS fan-out stops at `k`, so
   serving exactly `k` with one wrong forces the decoder to consume it pre-fix).
2. `ec_read_rejects_a_fragment_whose_ec_tuple_disagrees_with_scheme` — slot 0 serves a
   `none`-scheme header over index 1's bytes against a committed RS(2,1) chunk.

Each asserts (a) the read NEVER returns wrong bytes — it reconstructs the true object or
fails with a typed error; here < k intact remain post-fix, so it fails — and (b) the chunk
is enqueued on the shared repair queue (as the misplaced-fragment arm already does).

Load-light: pulls only `wyrd_core` + `wyrd_traits` + `wyrd_chunk_format` + `bytes` /
`async_trait` (no GUI/display/IO), so the headless runner loads it cleanly.

## Refutation (forced self-check — run this iteration, not carried over)

- **(a) Genuine red?** YES — reproduced this iteration. `git checkout -- crates/core/src/repair.rs
  crates/core/src/read.rs` (revert ONLY the two production files, keep the new test) →
  `cargo test -p wyrd-core --test fragment_identity` FAILS both by assertion: the read
  returns index-1's shard bytes at data position 0
  (`left = [128,129,…,198,0, 0…, 128,129,…]`) instead of the true `[0,1,2,…]`
  (panics at `fragment_identity.rs:204` and `:289`). Re-applying the fix → both pass.
- **(b) Production path?** YES — the tests call `read::read_object`, the real read entry;
  no copy/mock of the decode logic. The wrong fragment is served through the `ChunkStore`
  trait exactly as an adversarial backend would, and the fix under test
  (`header_matches_identity` at the two inline decode gates) is the production code the read
  path executes.
- **(c) Fixture includes the fault?** YES — the wrong-identity fragment is actually stored
  in the served set (slot 0), and only `k` fragments are served with one of them wrong, so
  the decoder is forced to consider it, not curated around it.

## Verification run (project cargo toolchain, in `$PDCA_WORKTREE`)

All run via `cargo` resolved through `engine/lib/ensure-cargo.sh` (the same toolchain
`engine/xtask.sh` / `cargo xtask ci` uses), in the driver's per-cycle worktree:

- RED leg (production reverted, test kept): `fragment_identity` → **2 failed by assertion**.
- GREEN leg (fix applied): `fragment_identity` → **2 passed**.
- `cargo test -p wyrd-core --test fragment_identity --test read_repair --test
  mutation_regressions --lib` → green (5 + 8 + lib).
- `cargo test -p wyrd-custodian --test scrub --test reconstruction --test rebalance --lib`
  → green (12 + 9 + 14).
- `cargo fmt --all -- --check` → clean (commit-hook gate).
- `cargo clippy -p wyrd-core -p wyrd-custodian --all-targets -- -D warnings` → clean;
  `-p wyrd-chunkstore-grpc --all-targets` → clean.
- Compile-only for the heavy/ignored legs: `RUSTFLAGS="--cfg madsim" cargo test -p wyrd-dst
  --test custodian --no-run` → compiles; `cargo test -p wyrd-chunkstore-grpc --test
  tier1_jepsen_consistency --test tier2_kill_reconstruct --no-run` → compiles.

Not run here (require privileged/container environments; compiled only): the `#[ignore]`d
Tier-1/Tier-2 legs (`WYRD_TIER1=1`/`WYRD_TIER2=1`, docker) and the full `run_dst` madsim
seed sweep. The C4 `cargo xtask ci` gate re-runs the real suite at Check. No external
dependency beyond the toolchain was needed (brief: External dependencies — none), so no
NEEDS-HUMAN external-dependency marker is raised.
