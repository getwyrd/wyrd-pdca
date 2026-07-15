# Build notes — issue 430 / fragment-identity-validation

## What the defect was

The shared read/repair validation admitted a decoded fragment on `chunk_id` **alone**:

- `repair::fragment_intact` / `repair::intact_shard` (`crates/core/src/repair.rs:53-70`
  on `main`) matched only `decoded.header.chunk_id == chunk`.
- The read path's inline decodes mirrored that half-check: the single-fragment path
  (`read.rs:234`) and the RS fan-out (`read.rs:319`).

So a backend that returned a validly-encoded fragment of the SAME chunk but the WRONG
`ec_fragment_index` — or one whose header EC tuple (`ec_scheme_type`/`ec_k`/`ec_m`)
disagreed with the committed `ChunkRef.scheme` — passed the gate, and the RS read pushed
its payload under the *requested* index (wrong reconstruction input → silently wrong
bytes, no repair enqueue). Only `FsChunkStore::verify`
(`crates/chunkstore-fs/src/lib.rs:117-130`) independently checked chunk **and** index —
masking the gap for today's fs backend, but leaving the assurance in the backend, not the
shared core code.

## The invariant restored (and where)

A fragment is admitted into any read/repair/maintenance path only when its decoded header
proves the FULL identity requested: `chunk_id`, `ec_fragment_index`, and (for RS) an EC
tuple consistent with the committed scheme. This is now enforced **in the shared core
code** for ANY backend, not delegated to backend goodwill:

- **New single predicate** `repair::header_matches_identity(header, expected: FragmentId,
  scheme: EcScheme)` (`repair.rs`). One definition of "against the chunk map", mirroring
  `FsChunkStore::verify` (chunk + index) and *widening* it with the committed EC tuple
  that only the core layer can check.
- `repair::fragment_intact` and `repair::intact_shard` **widened** to
  `(bytes, expected: FragmentId, scheme: EcScheme)` and routed through the predicate. The
  brief's suggested signature shape; Do owns the exact form.
- Read path inline gates now call `header_matches_identity` at both decode sites
  (`read.rs`, `EcScheme::None` path and RS fan-out) — same gate, no drift.
- Custodian call-sites pass the expected identity/scheme through:
  - `reconstruction.rs` — `intact_shard(b, frag, chunk_ref.scheme)`.
  - `rebalance.rs` — `fragment_intact(&bytes, frag, plan.prior.chunk_map[plan.chunk_index].scheme)`.
  - `scrub.rs` — `fragment_intact(&bytes, frag, scheme)`, where `scheme` is threaded via a
    new `schemes: HashMap<ChunkId, EcScheme>` field on `gc::ReferenceSet` (populated
    alongside `placed` in `referenced_fragments`). Scrub's reference set previously carried
    `(dserver, FragmentId)` only — the committed scheme was the one missing piece needed to
    check the header's full identity.

## Why widen the helpers rather than add new ones (footgun vs. churn)

The brief names `repair::fragment_intact` / `repair::intact_shard` as *the* defective gate
("accepts a decoded fragment on chunk_id alone ... The same helpers are the gate in
reconstruction, scrub and rebalance"). Leaving them chunk-only and adding parallel
full-identity helpers would leave a half-verification function in the shared API — exactly
the "verification against half the chunk map" the Invariant forbids; a future admission
call to `fragment_intact` would silently reintroduce the bug. So I widened the signatures.

Concrete cost of that choice: **3 production call-sites** + **~18 test-assertion
call-sites** across 6 files had to pass the fragment's index+scheme. The test callers
(`dst`, `tier1_disk_faults`, `custodian/tests/{reconstruction,rebalance}`,
`chunkstore-grpc/tests/tier{1,2}`) verify a rebuilt/placed fragment for a known index and
scheme, so each became `fragment_intact(&bytes, frag(i), EcScheme::…)` — mechanical, and
the fragments there are production-written so the full-identity check passes. This churn is
inherent to fixing the shared gate; it is not scope creep.

## Test-fixture corrections (production change legitimately invalidated them)

Several existing tests wrapped RS shards with `FragmentHeader::new_v1` (`ec_scheme_type =
None`, `ec_k = 1`, `ec_m = 0`, `ec_fragment_index = 0`) — headers the production **write**
path never stamps for an RS chunk (it uses `encode_ec_fragment`). Once the read path
checks index+scheme, those unrealistic fixtures are rejected. Corrected to stamp the real
RS header via `wyrd_core::write::encode_ec_fragment`:

- `read_repair.rs` — 4 RS tests (`ec_read_excludes…`, `ec_read_enqueues_integrity_fault…`,
  `ec_read_treats_a_misplaced…`, and the two `#530` audit tests).
- `mutation_regressions.rs` — `read_with_fewer_than_k_fragments_reports_insufficient`
  (also removed the now-unused `fragment()` helper + its unused `wyrd_chunk_format`
  import to keep clippy's warnings-as-errors green).

Scrub's tests use `EcScheme::None` chunks (correct `new_v1` headers), so they needed no
fixture change and still pass — including the misplaced-fragment and RS-above-index-0
cases.

## Test — `crates/core/tests/fragment_identity.rs` (NEW)

Two `#[tokio::test]`s drive the PUBLIC surface (`read::read_object` +
`repair::queued_repairs`) over an in-memory test-double store (mirrors `MemChunks`), so a
production revert fails them by **assertion**, not compile error:

1. `ec_read_rejects_a_same_chunk_wrong_index_fragment` — slot 0 serves index 1's fragment
   (header `ec_fragment_index = 1`), slot 1 correct, slot 2 absent (only `k` served, one
   wrong — deterministic red per the brief's recipe).
2. `ec_read_rejects_a_fragment_whose_ec_tuple_disagrees_with_scheme` — slot 0 serves a
   `none`-scheme header over index 1's bytes against a committed RS(2,1) chunk.

Each asserts (a) the read NEVER returns wrong bytes (it reconstructs the true object or
fails with a typed error — here < k intact remain, so it fails) and (b) the chunk is
enqueued on the shared repair queue.

## Refutation (forced self-check)

- **(a) Genuine red?** YES. Reverted `repair.rs` + `read.rs` (the read-path production),
  kept the test: both tests FAIL by assertion — the read returns shard-1's bytes at data
  position 0 (`left = [128,129,…,198,0,…]`) instead of the true `[0,1,2,…]`. Restored and
  they pass. (Verified via `git stash` of just the two production files.)
- **(b) Production path?** YES. The test calls `read::read_object`, the real read entry —
  no copy/mock of the decode logic; the wrong fragment is served through the `ChunkStore`
  trait exactly as an adversarial backend would.
- **(c) Fixture includes the fault?** YES. The wrong-identity fragment is actually stored
  in the served set (slot 0), and only `k` fragments are served with one of them wrong —
  the decoder is forced to consider it, not curated around it.

## Verification run (project toolchain, in `$PDCA_WORKTREE`)

- `cargo test -p wyrd-core --test fragment_identity --test read_repair --test
  mutation_regressions` → green (new tests pass; corrected fixtures pass).
- `cargo test -p wyrd-custodian --test scrub --test reconstruction --test rebalance` →
  green.
- `cargo test -p wyrd-core --lib repair` → green (widened unit test).
- `cargo build --workspace --exclude wyrd-dst --all-targets` → compiles (all `#[ignore]`d
  heavy tests type-check with the new signatures).
- `RUSTFLAGS="--cfg madsim" cargo test -p wyrd-dst --test custodian --no-run` → compiles.
- `cargo clippy --workspace --exclude wyrd-dst --all-targets` → clean (warnings-as-errors).
- `cargo fmt --all -- --check` → clean.

Not run here (require privileged / container environments; only compiled): the `#[ignore]`d
Tier-1/Tier-2 legs (`WYRD_TIER1=1`, `WYRD_TIER2=1`, docker) and the full `run_dst` 50-seed
madsim sweep. `cargo xtask ci` re-runs the real suite at Check.
