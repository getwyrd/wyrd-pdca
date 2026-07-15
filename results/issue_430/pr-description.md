# fix(430): verify a fragment's full identity, not its chunk id alone

## Summary
**User impact:** A read of an erasure-coded object could complete "successfully"
and hand back the wrong data. If the storage layer served a valid-looking piece
of the right object but from the wrong position — a misrouted write, a confused
disk, a misbehaving storage node — the system accepted it, reassembled the
object with it, and returned corrupted bytes with no error and no repair
scheduled. It hits any deployment whose backend can ever serve a piece at the
wrong slot, and it is silent.

This PR makes the shared verification check a fragment's **full identity** —
which chunk it belongs to, which position it holds, and which coding layout it
was written under — before the fragment is ever used, so a wrong fragment is
read around (or the read fails cleanly) and the damaged chunk is queued for
repair.

Reported in #430.

## What to look at
The crux is one new shared check in `crates/core/src/repair.rs`
(`header_matches_identity`) and the two places the read path in
`crates/core/src/read.rs` admits a fetched fragment; the maintenance loops
(reconstruction, scrub, rebalance) now pass the expected identity through to
the same check. To see the defect and the fix in one run:
`cargo test -p wyrd-core --test fragment_identity` passes on this branch;
revert `crates/core/src/read.rs` and `crates/core/src/repair.rs` to the merge
base (keeping the test file) and all three tests fail by assertion — the read
returns the wrong shard's bytes and queues no repair.

## Root cause
The shared helpers `repair::fragment_intact` / `repair::intact_shard` and both
inline decode gates on the read path compared only the decoded header's
`chunk_id` against the committed chunk map, so a checksum-valid fragment of the
same chunk at the wrong `ec_fragment_index` — or one whose header EC tuple
(`ec_scheme_type`/`ec_k`/`ec_m`) disagreed with the committed
`ChunkRef.scheme` — was accepted, and the RS read pushed its payload to the
decoder under the requested index: wrong reconstruction input, returned as
object bytes. Only `FsChunkStore::verify` independently checked chunk **and**
index, which masked the gap for today's fs backend while leaving the
never-wrong-bytes assurance delegated to backend goodwill instead of the
shared core code.

## Fix
- New shared predicate `repair::header_matches_identity(header, expected:
  FragmentId, scheme: EcScheme)`: the header must name the expected chunk
  **and** index, and its EC tuple must be consistent with the committed scheme
  (`None`/`k=1`/`m=0` for single-copy; the exact `k`/`m` stripe geometry for
  Reed-Solomon).
- `repair::fragment_intact` / `repair::intact_shard` widened to take the
  expected `FragmentId` + `EcScheme` and gate through it.
- Both read-path decode gates (single-fragment and RS fan-out) use the
  predicate; a rejected fragment is excluded from the decoder and its chunk is
  pushed onto the shared repair queue, exactly as the existing
  misplaced-fragment arm already did.
- Custodian call-sites pass the expected identity/scheme through:
  reconstruction, rebalance, and scrub (which now carries each referenced
  chunk's committed scheme via `ReferenceSet.schemes` in `gc.rs`).
- The rest of the diff is mechanical: existing tests updated to the widened
  signatures, stamping full RS headers where they previously stamped
  chunk-id-only ones.

Out of scope, untouched: backend `ChunkStore` implementations (FsChunkStore
already verifies), new metrics, and maintenance-loop behavior.

## Verification
- **Claim:** a store that returns a validly-encoded fragment of the same chunk
  at a different index — or one whose header EC tuple disagrees with the
  committed scheme — is rejected by the shared validation: never fed to the
  decoder under the requested index; the read never returns wrong bytes (it
  reads around when ≥ k intact fragments remain, or fails with a typed error),
  and the affected chunk is enqueued for repair.
- **Checked** (on `main`, dc503cd6d9d0b8bb2e3d64bb88a206a6857b52bb):
  `crates/core/src/repair.rs:53-70` — `fragment_intact` / `intact_shard`
  compare `chunk_id` only; `crates/core/src/read.rs:234` (single-fragment) and
  `crates/core/src/read.rs:319` (RS fan-out) admit on `chunk_id` alone; the
  same helpers gate `crates/custodian/src/reconstruction.rs:389`,
  `crates/custodian/src/scrub.rs:119`, and
  `crates/custodian/src/rebalance.rs:264`. The store-level precedent mirrored
  here is `crates/chunkstore-fs/src/lib.rs:117-130` (`FsChunkStore::verify`,
  chunk **and** index).
- **Test:** `crates/core/tests/fragment_identity.rs` (new) — three cases drive
  the public surface (`read::read_object` + `repair::queued_repairs`) over a
  trait-level store double that serves exactly `k` fragments, one of them
  wrong-identity, so the decoder must consume it pre-fix: (1) same-chunk
  fragment at the wrong index, (2) header EC tuple disagreeing with the
  committed scheme, (3) same scheme type but wrong `k`/`m` stripe geometry.
  All three fail **by assertion** pre-fix (the read returns the wrong shard's
  bytes; nothing enqueued) and pass post-fix. The widened unit test in
  `repair.rs` pins the `k`/`m` conjuncts specifically with an
  RS(3,1)-vs-RS(2,1) pair: a mutant that drops the `k`/`m` compare is killed
  by case 3 plus that unit test.
- **Suite health:** `cargo test --workspace --all-targets --no-run` compiles
  clean; `cargo fmt --all -- --check` and `cargo clippy --all-targets -- -D
  warnings` are clean on the touched crates; the madsim-gated
  `crates/dst/tests/custodian.rs` suite passes under `RUSTFLAGS="--cfg
  madsim"` (10/10).

Fixes #430
