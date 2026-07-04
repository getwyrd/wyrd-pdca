# Build notes — issue 253 / atomic-conditional-commit-conflict-semantics (M4.2)

Target branch: `feat/m4-production-metadata-backend` (worktree
`$PDCA_WORKTREE=/home/eddie/wyrd/wyrd.pdca-wt`, tip `2572132`). Built to
proposal 0007 / **accepted** 0015 §"Suggested PR sequence" item 2, §"Mapping the
contract onto TiKV", §"…contract M4 must honor verbatim".

## The invariant to restore

The contract's load-bearing partition (`crates/traits/src/lib.rs:346-361`,
`CommitOutcome`): a **losing writer is `Ok(Conflict)`, never `Err`**; `Err` is
reserved for genuine faults. Pre-#253, `commit()` routed *every* backend error —
including a real TiKV write-write race — through `rollback_then` (→ `Err`)
(`crates/metadata-tikv/src/lib.rs:128-131` on the pre-fix tip). A losing writer
therefore surfaced as a fault. This slice restores the partition; it is not a
smallest-diff exercise but the smallest change that puts the `Conflict`-vs-`Err`
line back where the trait draws it.

## (a) Write-conflict classification — `crates/metadata-tikv/src/lib.rs`

Two helpers added inside `mod store` (post-fix lines):

- `is_write_conflict(&tikv_client::Error) -> bool` (`src/lib.rs:149-160`):
  `true` only for `Error::KeyError(ke)` with `ke.conflict.is_some()`, recursing
  (any) through `MultipleKeyErrors`/`ExtractedErrors` and into
  `PessimisticLockError { inner, .. }`. Every other variant (network,
  region-unavailable, PD-timeout, `locked`, `deadlock`, `abort`, undetermined) →
  `false` → stays `Err`.
- `conflict_or_err(txn, err) -> Result<CommitOutcome>` (`src/lib.rs:173-184`):
  best-effort `let _ = txn.rollback().await`, then `is_write_conflict` ?
  `Ok(Conflict)` : `Err(err.into())`.

Routing in `commit()`:
- `get_for_update` error arm → `conflict_or_err` (`src/lib.rs:275-277`): a lock
  failure can itself be a lost race.
- Precondition-miss cleanup rollback made best-effort `let _ = …`
  (`src/lib.rs:284-286`) so a cleanup fault can't mask a legitimate `Conflict`.
- Final `txn.commit()` error arm → `conflict_or_err` (`src/lib.rs:306-308`):
  prewrite is where the race is usually lost.
- `put`/`delete` keep `rollback_then` (→ `Err`); they buffer locally and cannot
  raise a write-conflict. `get`/`scan` untouched.

### Verified backend facts confirmed against the pinned source

Rather than trust the brief's summary blind, I checked the vendored
`tikv-client-0.4.0` in the registry cache:
- `Error` enum: `KeyError(Box<ProtoKeyError>)`, `ExtractedErrors(Vec<Error>)`,
  `MultipleKeyErrors(Vec<Error>)`, `PessimisticLockError { inner: Box<Error>,
  success_keys }` — `src/common/errors.rs:86,89,92,122-126`.
- `kvrpcpb::KeyError.conflict: Option<WriteConflict>` —
  `src/generated/kvrpcpb.rs:1347`.
All variant/field names in the patch match verbatim; the `--features tikv`
compile below is the proof.

### Drop-safety (why `conflict_or_err` is correct)

`Transaction`'s `Drop` panics only while status is `Active`. `commit()` moves to
`StartedCommit` *before* its RPC (so a failed commit is already drop-safe) and
`rollback()` accepts `StartedCommit` (so the best-effort rollback is valid and
releases prewrite locks a loser left). After a `get_for_update` error the txn is
still `Active`, so `conflict_or_err`'s rollback is what makes that arm drop-safe.

## (b) Contention property tests — `crates/metadata-tikv/tests/contention.rs` (NEW)

Endpoint-gated exactly like `tests/conformance.rs` (`WYRD_TIKV_PD_ENDPOINTS`
probe → clean skip). Two tests, each a fresh namespace:
- `write_write_race_exactly_one_winner`: seed `k=v0`, then `WRITERS=8`
  independent connections concurrently `commit(require(k,v0).put(k,"w{i}"))` via
  `futures_util::future::join_all` → assert exactly one `Committed`,
  `WRITERS-1` `Conflict`, **zero `Err`** (a fault panics), final value = winner's.
- `require_absent_race`: same shape on an absent key with `require_absent(k)`.

Each writer gets its **own** `TikvMetadataStore` connection sharing the
namespace, on a multi-thread tokio runtime, so the race is real
cross-connection contention at the cluster, not a same-txn artifact.

`futures-util` added to `[dev-dependencies]` (`crates/metadata-tikv/Cargo.toml`);
it is the workspace's join-combinator crate (the workspace pins `futures-util`,
not `futures`, so `futures_util::future::join_all` is the in-repo idiom — same
combinator the brief's `futures::future::join_all` names). `join_all` needs the
`alloc` feature, which the workspace pin already enables.

**Import-light / headless-safe:** the test's top level uses only `std`; every
reference to `tikv-client`/`tokio`/`bytes`/`futures-util`/the store lives inside
`#[cfg(feature = "tikv")]` bodies. With the default (feature-off) build — what
`cargo xtask ci` runs — the file compiles to two `#[test]`s that skip, pulling in
no heavy dependency at load.

## (c) xtask — `xtask/src/main.rs`

`run_tikv_conformance_test` (`main.rs:190-197`) now loops `["conformance",
"contention"]` through a new `run_tikv_test(test)` (`main.rs:199-229`) that keeps
the per-binary env + 5-attempt backoff. So `cargo xtask tikv-conformance`
exercises the new tests.

## Invariants held

- **Trait untouched:** `crates/traits/src/lib.rs` `MetadataStore` / `WriteBatch`
  / `Precondition` / `CommitOutcome` byte-for-byte unchanged — not in the diff.
- **Never retry a write-conflict here:** `conflict_or_err` maps to `Conflict` and
  returns; no retry loop.
- **Value equality:** the test asserts the final value byte-equals the winner's
  write; the store still stores byte-identically.

## Verification run

Via the worktree, cached deps, no network/container needed for compile:
- `cargo test -p wyrd-metadata-tikv --no-run` (default/no-tikv) — compiles;
  `--test contention` **runs green, both tests skip cleanly** (no
  `WYRD_TIKV_PD_ENDPOINTS`).
- `cargo test -p wyrd-metadata-tikv --features tikv --no-run` — production code +
  gated test **compile** (tikv-client 0.4.0 built; the error-variant/field names
  are thus confirmed real).
- `cargo fmt --check -p wyrd-metadata-tikv -p xtask` — clean.
- `cargo clippy -p wyrd-metadata-tikv --features tikv --tests`,
  `-p wyrd-metadata-tikv --tests`, `-p xtask` — clean.
- `cargo machete crates/metadata-tikv` — no unused deps (the new `futures-util`
  is seen used).

## NEEDS-HUMAN — endpoint-gated red→green (expected, per the brief)

The behavioral red→green **cannot** be observed in this worktree: there is no
TiKV, so both tests skip (green pre- and post-fix). This is the brief's
documented endpoint-gated situation, identical to #252. The honest proof is a
`cargo xtask tikv-conformance` run against the throwaway `deploy/` single-node
TiKV:

1. `cargo xtask tikv-conformance` (brings up `deploy/` TiKV, sets
   `WYRD_TIKV_PD_ENDPOINTS`, rebuilds `--features tikv`, runs `--test
   conformance` **and** `--test contention`).
2. Expect: `write_write_race_exactly_one_winner` and `require_absent_race`
   **green** — 1 `Committed`, 7 `Conflict`, 0 `Err`, final = winner.
3. Red check (optional, to see the pre-fix failure): with `git stash` of only the
   `src/lib.rs` §(a) hunks (revert `commit()` to `rollback_then` on both arms) and
   the tests kept, the same run goes **red** — losers surface as `Err`, tripping
   the "zero `Err`"/exactly-one-`Committed` asserts. Record the winner/loser
   tallies here at sign-off.

I did **not** fabricate a non-TiKV stand-in: the race is irreducibly a live-TiKV
behavior, and a mock would prove nothing about the `KeyError.conflict` partition.
Shipping the honest endpoint-gated test + this manual-validation recipe is the
correct surface for the §6 NEEDS-HUMAN.

## Deferred (not resolved here, per brief)

ADR-0003 full-tree `tikv-client` audit (RUSTSEC-2026-0099/-0104 + ISC allowlist)
stays deferred behind the off-by-default `tikv` feature — this slice ships nothing
from that tree into the default build.
