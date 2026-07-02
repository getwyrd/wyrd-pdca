# Build notes — issue 285 / validate-ec-scheme-at-read-boundary

## What broke, precisely

`erasure::reconstruct` (`crates/core/src/erasure.rs:89-132` pre-fix) guards against too
few shards with `available.len() < k` (`erasure.rs:95`). For `k == 0` that comparison is
`0 < 0`, which is always false, so the guard never trips. Execution falls through to
`available[0].1.len()` (`erasure.rs:101`) and panics on an empty `available` slice. The
read path (`crates/core/src/read.rs:175-176` pre-fix) casts the stored `EcScheme::
ReedSolomon { k, m }` straight to `usize` and reaches the same call with no re-check —
`read_chunk`'s own `shards.len() < k` guard (`read.rs:243` pre-fix) has the identical
`0 < 0` blind spot. The CLI already refuses `rs(0,m)` at parse time
(`crates/server/src/cli.rs:110`), but that check only guards the *write* path's argument
parser; a scheme read back from committed (and therefore untrusted — corrupted, bit-rotted,
or tampered) metadata never passes back through it.

## Fix

Two validation points, matching the brief's "Invariant to restore" (validate at the
erasure/read API boundary, before shard indexing or fan-out):

1. **`erasure::reconstruct`** (`crates/core/src/erasure.rs:105-118` post-fix): a new
   `ErasureError::InvalidScheme { k, m }` variant, and a `k == 0` check as the very first
   statement in `reconstruct`, before the `available.len() < k` guard. This is the
   authoritative boundary — every caller of `reconstruct` (today just `read_chunk`, but
   also future callers, e.g. custodian-side reconstruction) gets the guarantee for free
   without having to duplicate the check.

2. **`read_chunk`'s `ReedSolomon` arm** (`crates/core/src/read.rs:175-183` post-fix): a
   `k == 0` check immediately on entry, before the fragment fan-out (the `FuturesUnordered`
   of `n = k + m` fetches) is even built. A new `ReadError::InvalidEcScheme { chunk_id, k,
   m }` variant carries the chunk id through so the error message is actionable. This
   matters because reconstruct's own guard fires only *after* the (pointless, for `k == 0`)
   fan-out has already happened — validating at the read boundary too means a corrupted
   chunk's record is rejected before the read path spends any I/O on it, per the brief's
   "before they index shard buffers or drive fan-out" wording.

Both layers are needed to fully satisfy the brief: the erasure-level check is the
`Success criterion`'s literal demonstration (`erasure::reconstruct(0, 1, 0, &[])` returns
`Err`, not a panic — exactly the repro instruction), and the read-level check is the
"committed inode whose stored chunk scheme is `ReedSolomon { k: 0, m }`" leg. Skipping
either one leaves a gap: erasure-only would still let `read_chunk` waste a full fan-out on
a scheme it's about to be told is invalid; read-only would leave `erasure::reconstruct`
itself panicking for any *other* future caller that doesn't happen to pre-validate `k`.

## What I ruled out

- **Validating only in `read_chunk`, not in `reconstruct`.** Rejected: `reconstruct` is a
  `pub fn` in a library crate — nothing stops a different caller (custodian scrub/repair
  reconstruction, a future CLI subcommand, a test) from calling it directly with an
  untrusted `k`. The brief's success criterion is stated in terms of `erasure::reconstruct`
  itself ("Calling `erasure::reconstruct` with `k == 0` … returns a typed error"), which is
  the binding demonstration — validating only one layer up would leave that literal
  criterion unmet.
- **Validating only in `erasure::reconstruct`, not in `read_chunk`.** Rejected on the
  invariant text itself: "validated at the erasure/read API boundary before they index
  shard buffers or **drive fan-out**". `reconstruct` alone can't stop the fan-out — by the
  time it's called, `read_chunk` has already fired `n = k + m` `get_fragment_at` futures
  (`read.rs:188-197`, unchanged). Only a check inside `read_chunk` itself, before building
  `inflight`, can avoid that fan-out for a scheme known to be invalid up front.
- **Widening the check to reject "unsupported k+m" generally (e.g. an upper bound, or
  `m == 0`).** Rejected as out of scope and unfounded: the brief's concrete, demonstrable
  defect is `k == 0`; `m == 0` is a legitimate (if degenerate — no parity) scheme that
  `reed-solomon-simd::encode`/`decode` already accept, and any true upper-bound violation
  the underlying coder rejects is already surfaced as `ErasureError::Coder` (existing,
  correct behaviour) — inventing a second bound with no cited failure mode would be
  speculative scope creep the brief explicitly doesn't ask for ("Scope: … the `k == 0`
  panic being the concrete symptom, plus unsupported `k + m`" is the *defect description*,
  not a request to enumerate every invalid `(k, m)` pair; the binding success criterion is
  only the `k == 0` case).
- **Fixing it by making the `available.len() < k` / `shards.len() < k` comparisons `usize`-
  safe for `k == 0` some other way** (e.g. `available.is_empty() && k == 0` special-cased
  inline at the existing guard sites) instead of an explicit named error. Rejected: this
  would still let a `k == 0` scheme with `available.len() >= 1` (say a single stray
  survivor) sail past the guard and panic at `available[0].1.len()` immediately after —
  the guard's blind spot is `k` itself being invalid, not merely the shard count being
  wrong relative to it. A dedicated up-front `k == 0` check closes the whole class instead
  of patching one specific triggering input.
- **Changing the CLI parse rule** (`cli.rs:110`) — explicitly out of scope per the brief;
  it already does the right thing, this defect is about the read/reconstruct layers not
  re-checking what the CLI already validated once.
- **Touching the on-disk metadata format / codec schema** (ADR-0002) — explicitly out of
  scope (human-only); not needed here since the fix is pure input validation on values
  already decoded into `EcScheme`, not a schema change.

## Test placement and why

Per the brief's `Test file` field, the primary regression lives in
`crates/core/src/erasure.rs`'s existing `#[cfg(test)] mod tests` (co-located with the
fix — `erasure.rs` has no separate `tests/*.rs` file, matching every other test in that
module, e.g. `fewer_than_k_shards_is_an_error`). Two tests:

- `reconstruct_with_k_zero_is_a_typed_error_not_a_panic` — the literal repro from the
  brief (`reconstruct(0, 1, 0, &[])`).
- `reconstruct_with_k_zero_and_nonempty_available_is_still_a_typed_error` — the same
  scheme with real (encoded) shards present, so the fix isn't accidentally piggy-backing
  on the `available.len() < k` guard coincidentally still catching the empty-slice case;
  it proves the `k == 0` check itself is what's firing, independent of shard count.

The companion read-path assertion (brief: "Do adds a companion read-path assertion in
`crates/core/src/read.rs` tests for the stored-scheme case") is a new
`#[cfg(test)] mod tests` in `read.rs` (that file had no inline tests before this change —
its existing coverage lives in `crates/core/tests/read_repair.rs`, but that file can only
reach `read_chunk` indirectly through the fully-`pub` entry points, and `read_chunk` is
private (`fn`, not `pub fn`, at `read.rs:120`) — so the stored-scheme boundary check can
only be exercised directly from inside the module). `read_chunk_rejects_a_stored_k_zero_scheme`
constructs a `ChunkRef` with `scheme: EcScheme::ReedSolomon { k: 0, m: 1 }` (exactly the
brief's "committed `InodeRecord` whose chunk `scheme` is `ReedSolomon { k: 0, m: 1 }`") and
drives `read_chunk` directly against a `ChunkStore` that holds nothing (`EmptyChunks`,
`get_fragment` always `Ok(None)`) — deliberately trivial, because the point is that the
`k == 0` scheme must be rejected *before* any fragment is even fetched, so a store that
would immediately return "nothing found" either way still proves the rejection is coming
from the new validation, not from starving on missing fragments. Drives it through
`pollster::block_on` — the same "no runtime tie-in" pattern the crate's own dev-dependency
comment documents (`crates/core/Cargo.toml:23-24`) and other in-crate integration tests use
(`crates/core/tests/write_fanout.rs:14`), rather than pulling in `#[tokio::test]` (which
`crates/core/tests/read_repair.rs` uses, but that's a full external integration test file,
not a same-module unit test) — `read_chunk`'s `FuturesUnordered` polling needs no executor
beyond a bare `block_on`.

## Red → green, verified directly (not just through the gate)

`run-verify.sh` (C4-verify) treats this patch as "green-only" (its own printed message:
"test is co-located with the fix … so the per-fix RED can't be isolated") because both
production edits and both new tests live inside modified `.rs` files under `crates/core/
src/`, not a separate `tests/*.rs` the gate's revert-and-rerun logic can isolate. That is
expected and matches the brief's own framing of the test location. To still prove the
red→green contract honestly (not just trust the green-only gate), I manually reverted only
the two production hunks (the `k == 0` checks in `erasure::reconstruct` and `read_chunk`,
keeping every test) and re-ran `cargo test -p wyrd-core --lib`:

- **Pre-fix (production reverted, tests kept):**
  `thread 'erasure::tests::reconstruct_with_k_zero_is_a_typed_error_not_a_panic' panicked
  at crates/core/src/erasure.rs:117:16: index out of bounds: the len is 0 but the index is 0`
  — the exact panic the brief describes — and
  `thread 'read::tests::read_chunk_rejects_a_stored_k_zero_scheme' panicked at
  crates/core/src/erasure.rs:117:16: index out of bounds…` (same panic, reached through the
  read path). Both new tests failed red, for the right reason (a panic, not an assertion
  mismatch).
- **Post-fix (production restored):** `cargo test -p wyrd-core --lib` — 21 passed, 0
  failed (`erasure::tests::reconstruct_with_k_zero_is_a_typed_error_not_a_panic`,
  `erasure::tests::reconstruct_with_k_zero_and_nonempty_available_is_still_a_typed_error`,
  and `read::tests::read_chunk_rejects_a_stored_k_zero_scheme` all green, along with every
  pre-existing test in the crate — no regression).

Ran through the project's own runner throughout (`cargo test -p wyrd-core --lib`, the same
invocation `run-verify.sh` uses internally), plus `run-verify.sh` itself
(`PDCA_BUNDLE=results/issue_285 ./engine/scripts/run-verify.sh`) against the bundle's
`patch.diff` applied to a clean `origin/main` worktree — it applies cleanly and all 21
`wyrd-core` lib tests plus the crate's other integration suites pass green with the fix
applied.

## Commit-readiness

`cargo fmt --all -- --check` — clean, no diff, over the whole workspace (not just the
touched files). `cargo clippy -p wyrd-core --all-targets` — no warnings (the workspace's
`[lints]` in the root `Cargo.toml` are inherited and enforced as warnings-as-errors per
`xtask/src/main.rs:533-541`'s `run_ci`; ran the crate-scoped equivalent for speed, same
lint set). No new dependencies were added (`bytes`, `async-trait`, `pollster` are already
declared: `bytes` as a normal dependency at `crates/core/Cargo.toml:14`, `async-trait` and
`pollster` as existing dev-dependencies at `crates/core/Cargo.toml:24,26`), so no
`cargo-machete`/`cargo-deny` impact.

One incidental fix while I was in `erasure.rs`: the mutation-testing doc comment above
`coder_error_exposes_its_wrapped_source` cited `:48`/`:49` as the lines of the `source()`
match arms it guards against; my insertion shifted `impl std::error::Error for ErasureError`
from lines 46-53 to 62-69, so I updated the citation to `:64`/`:65` to keep it accurate
rather than leaving a now-wrong line reference behind.
