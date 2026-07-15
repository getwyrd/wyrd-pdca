# Build notes — issue 490 (iteration 2): renew-pending-lease-resurrection

## What the fix does (one logical change: a lapsed lease is dead *through publish*)

Both seams of the pending-lease lifecycle are closed, sharing one read-back-and-`require`
primitive so the check and the write are always atomic.

### Seam 1 — renewal (carried from iteration 1, boundary re-aligned)
`metadata::renew_pending` (`crates/core/src/metadata.rs:539-591`) gains a `now_millis`
parameter and is now **conditional**: per chunk it reads the current `pending:<id>` entry and
refuses (`CommitOutcome::Conflict`, nothing written) when the entry is **absent** (swept) or
present but `lease_expiry_millis <= now_millis`. On the live path it pairs
`require(pending_key, current) + put(pending_key, new)` in one `WriteBatch`, so a sweep that
interleaves between read-back and commit turns the precondition false → `Conflict`.
`lease_write_chunk` (`write.rs:431-456`) now inspects the outcome and aborts the upload with a
hard `WriteError::LeaseLapsed` (`write.rs:619-655`) **before** the next chunk is written toward
commit, rather than discarding it.

**Obligation (b) — the `<=` boundary.** Iteration 1 used `<` and called `now == expiry` a
healthy deadline renewal. That disagrees with the reaper (`sweep_expired_leases`,
`write.rs:572`, reaps `expiry <= now`). This iteration aligns renewal to `<=`, so both lease
consumers agree: dead at `expiry <= now`.

### Seam 2 — commit (new; the iteration-1 rejection)
`metadata::create_leased` (`metadata.rs:305-348`) and
`metadata::commit_chunk_map_superseding_leased` (`metadata.rs:522-575`) take the plan's chunk
ids + a commit-time `now` and thread a per-chunk `require(pending_key, read-back-value)` into
**the same** batch as the inode create / CAS (and, for the overwrite, the `orphan:` records).
A shared helper `live_lease_guards` (`metadata.rs:593-618`) does the read-back + expiry check
(`<=` boundary) and returns `None` (→ `Conflict`, fail-closed) on absent-or-expired.
`write::commit_create` (`write.rs:247-288`) gains a `now_millis` param and calls
`create_leased`; `write::commit_overwrite` (`write.rs:271-303`) keeps its signature and reuses
its existing `orphaned_at_millis` as the commit-time `now` (obligation (e)) into
`commit_chunk_map_superseding_leased`.

### Non-breaking wrappers (deliberate, to bound the blast radius)
`metadata::create` and `metadata::commit_chunk_map_superseding` are **left untouched**. Only
the phase-3 wrappers use the `_leased` variants. This keeps the ~25 direct `metadata::create`
callers (custodian/conformance/jepsen suites that legitimately create committed inodes with no
pending ledger) and `gc_delete_backstop.rs:244`'s `commit_chunk_map_superseding` compiling
unchanged. Adding a `pending`+`now` parameter to `metadata::create` itself would have forced a
mechanical edit on every one of those callsites (25+ files) for zero semantic gain — those
callers are not phase 3 of a streaming write. Cost avoided, concretely: the workspace `--tests`
build touches `metadata::create` at `crates/custodian/tests/{gc,scrub,rebalance,gc_delete_backstop,
gc_telemetry,tier1_disk_faults}.rs`, `crates/metadata-redb/tests/conformance.rs`,
`crates/chunkstore-grpc/tests/{tier1_jepsen_consistency,tier2_kill_reconstruct}.rs`,
`crates/core/tests/placement_record.rs`, `crates/dst/tests/custodian.rs` — every one would
have needed `, &[], 0` appended. The wrapper approach edits **zero** of them.

## Test callsite updates (the in-scope consequence)
Phase-3 commit is now lease-conditional, so suites that drove phase 3 without phase 1 had to
run `intent` first (the protocol's own order) — never by weakening the commit condition:
- `commit_create` gained a `now` argument at every callsite (server + dst suites).
- `erasure_path.rs`'s overwrite race skipped `intent` entirely — added it for `plan_a`/`plan_b`.
- All other server/dst suites already ran `intent`; only the `now` arg was added.

## The `stream_lease_renewal.rs` clock change (deviates from the brief's literal wording)
The brief says `stream_lease_renewal.rs` "stays green with only the mechanical `commit_create`
call-site update." That is **inconsistent with obligation (b)** for that test's specific clock:
its clock advances an *exact* TTL between chunks `[0, TTL, 2·TTL]`, so each renewal fires at
`now == expiry` of the prior chunk — which obligation (b) explicitly re-defines as **dead**
(`<=`). Under `<`, iteration 1 kept it green; under the reaper-aligned `<=` it must refuse.

I followed the substantive, argued, repeated obligation (b) and shifted the clock to advance
*just under* a full TTL `[0, TTL-1, 2·(TTL-1)]`, so every renewal fires strictly before expiry
— a healthy slow upload under the corrected contract. The test's purpose (a slow upload's
renewal survives a mid-upload sweep, then commits byte-identical) is unchanged and still green.
**Flagging for sign-off:** this is a behavioral edit to a peer test, not the "signature-only"
change the brief predicted; it is the unavoidable consequence of aligning the renewal boundary
to the reaper. The C4-verify red leg reverts this file anyway (keeps only the added test).

## Refutation of the added test (`crates/core/tests/stream_lease_lapse.rs`)
Four scenarios, all overwrite-shaped, all driving production `write::stream_write_data` +
`write::commit_overwrite` + `write::write_new_object` over `RedbMetadataStore::in_memory()` +
`FsChunkStore`. Uses only signature-stable APIs (never `commit_create`), so it compiles against
both base and fixed trees (C4-verify's hard constraint).

- **(a) Genuine red?** YES — verified by stashing only the three production files
  (`metadata.rs`, `write.rs`, `server/src/lib.rs`) and running the added test against the base
  tree: 3 of 4 fail —
  - `mid_upload_lapse…` FAILS ("must abort, not produce a commit plan"),
  - `eof_window_lapse…` FAILS ("commit must refuse… left: Committed right: Committed"),
  - `present_but_expired…` FAILS (same),
  - `live_overwrite…` (healthy control) PASSES on both trees, as intended (guards must not
    refuse a live upload). Post-fix: all 4 pass.
- **(b) Production path?** YES — the test calls `write::stream_write_data`,
  `write::commit_overwrite`, `write::write_new_object`, `write::sweep_expired_leases`,
  `read::{resolve,read_inode,read_path}` — the real production functions the fix changes (or
  that call them). No mock, copy, or re-implementation.
- **(c) Fixture includes the fault?** YES — Repro (A) asserts the mid-upload sweep actually
  reclaimed the in-flight lease (`reclaimed == vec![201]`); Repro (B) asserts the EOF sweep
  reclaimed the streamed leases (`!reclaimed.is_empty()`); Repro (C) asserts the leases are
  still *present* at commit (`scan("pending:").len() == plan.chunks.len()`) so the refusal is
  purely the expiry check, not an absent entry. The fault is injected and observed, not
  curated out.

## Tests run (worktree `$PDCA_WORKTREE`, via cargo; DST under `--cfg madsim`)
- `wyrd-core` (incl. `stream_lease_lapse`, `stream_lease_renewal`): green; red proven pre-fix.
- `wyrd-server` `write_path`/`erasure_path`/`read_path`/`dst_commit`: green.
- `wyrd-dst` full suite `MADSIM_TEST_NUM=50/64` (incl. `commit_ambiguity` anti-vacuity floors,
  `concurrency`, `network`, `tikv_await_commit_interleaving`): green. The lease read-back adds
  `store.get` await points inside `commit_overwrite`; I specifically checked the ambiguity
  sweeps still reach both halves and the `>= 1` floors still hold at 50 and 64 seeds.
- `wyrd-custodian`, `wyrd-metadata-redb`: green.
- `cargo fmt --all --check`: clean. `cargo clippy` (core, server; dst under `--cfg madsim`):
  no warnings.

## Not run here (Check's gates cover them)
Full `cargo xtask ci` (deny, on-disk conformance vectors, real-etcd/tikv fidelity legs, every
crate's suite). No external dependency was missing for the parts in scope — nothing to declare
as NEEDS-HUMAN beyond the `stream_lease_renewal.rs` clock note above.

## Out of scope (per brief, untouched)
`metadata::commit_chunk_map` (reconstruction/backfill, no pending entries); GC/sweep behaviour;
the residual GC-mid-pass race (getwyrd/wyrd#557); gateway error taxonomy beyond the existing
`Conflict → GatewayError::Conflict` mapping; #554 GC deployment wiring.
