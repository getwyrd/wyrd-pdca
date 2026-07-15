# check-advisory-adversary.md — issue 490 / renew-pending-lease-resurrection (iteration 2)

Skeptic's pass. I re-ran the red→green proof independently and ran two mutation
attacks against the fix. One landed.

## Evidence — re-verified, could not refute

- Reproduced both legs myself in a scratch copy of `$PDCA_TARGET`: green leg
  (`cargo test -p wyrd-core --test stream_lease_lapse`) → 4/4 pass; red leg (reverted all
  11 modified files, kept only the added `crates/core/tests/stream_lease_lapse.rs`) →
  the test **compiles against base** (the brief's honesty constraint holds — no
  compile-error "red") and fails on the three substantive assertions
  (`stream_lease_lapse.rs:185`, `:262`, `:305` — base returns `Committed` / an `Ok` plan),
  while the healthy control stays green on both trees. The proof exercises the production
  `write::stream_write_data` → `write::commit_overwrite` path over
  `RedbMetadataStore::in_memory()` + `FsChunkStore`; no parallel re-implementation, no
  tautology (Repro (A) drives phase 3 on the `Ok` arm, closing iteration 1's tautological
  binding). C4-verify's PASS claim is corroborated.

## Refutations that landed

- NEEDS-HUMAN [impl] — **The `<=` expiry boundary — obligation (b)/(d)'s headline delta over
  iteration 1 — is not pinned by any test; a regression to `<` (iteration 1's exact rejected
  bug) survives the entire red→green gate.** Verified by mutation: changing
  `crates/core/src/metadata.rs:658` (`renew_pending`) and `metadata.rs:694`
  (`live_lease_guards`) from `<=` to `<` leaves `stream_lease_lapse` (4/4) and
  `stream_lease_renewal` (1/1) green, and no other suite plausibly pins it (the dst/server
  suites all commit at `now=0` with expiry `NOW+TTL`). Cause: Repro (C) claims to pin the
  boundary ("this pins the `<=`-boundary expiry check at commit",
  `crates/core/tests/stream_lease_lapse.rs:294-309`) but commits at `2*TTL` against leases
  expiring at `TTL` — a full TTL past the boundary, not at it; likewise Repro (A)'s renewal
  fires at `2*TTL`. Concrete failing case under the `<` mutant: a commit (or renewal) at
  exactly `now == lease_expiry_millis` publishes/extends while `sweep_expired_leases`
  (`crates/core/src/write.rs:610`, `<=`) and GC (`crates/custodian/src/gc.rs:254`, `<=`) at
  the same instant are entitled to reap — the precise reaper-disagreement the brief ordered
  closed. Builder fix: add a commit at exactly `now = TTL` in Repro (C) (must refuse) and a
  renewal-at-exact-deadline case; the updated `stream_lease_renewal.rs:46-53` clock comment
  even documents the `now == expiry` refusal but nothing tests it.

- NEEDS-HUMAN [impl] — **The create-seam refusal is untested: dropping the guard threading
  in `create_leased` alone would survive the suite.** All lapse scenarios are
  overwrite-shaped (a brief-imposed compile-against-base constraint on the *added* file),
  so `metadata::create_leased`'s guard loop (`crates/core/src/metadata.rs:328-341`) is
  exercised only on its healthy pass path (`stream_lease_renewal.rs:96`, live leases) —
  no test ever drives `commit_create` with a lapsed/absent lease. A mutant that skips the
  up-front `live_lease_guards` refusal or the `batch.require` loop in `create_leased` only
  (leaving the overwrite path intact) stays green everywhere. The constraint bound only the
  added file — a post-fix-only refusal case (e.g. in the already-modified
  `stream_lease_renewal.rs`, which the red leg reverts anyway) closes this cheaply.

## Refutations attempted that did not land

- **Production wiring bypass** — both gateway phase-3 arms flow through the leased variants
  with a fresh commit-time instant (`crates/server/src/lib.rs:184` overwrite via
  `now_millis()`-as-`orphaned_at`, `lib.rs:193` create via `now_millis()`); the CLI/cluster
  PUTs go through `write_new_object` → patched `commit_create`; grep finds no production
  caller left on unconditional `metadata::create`/`commit_chunk_map_superseding`
  (`commit_chunk_map` is brief-scoped out). Could not refute.
- **Wrong-reason passes in existing suites** — the pre-existing one-winner/Conflict
  assertions (`server/tests/write_path.rs:236-243`, `dst_commit.rs:96-103`,
  `dst/tests/concurrency.rs:102-110`, etc.) all run `intent` with expiry `NOW+TTL` (or
  `LEASE_EXPIRY=6000`) before committing at `now=0`, so their `Conflict` still comes from
  the inode CAS, not the new lease guard; erasure_path.rs was the only intent-skipper and
  the patch adds it (`erasure_path.rs:150-153`). Could not refute.
- **Atomicity of the guard** — the per-chunk `require(pending_key, read-back-value)` rides
  in the same `WriteBatch` as the create/CAS (`metadata.rs:339-341`, `:589-591`), so a sweep
  interleaving between read-back and commit flips the precondition and yields `Conflict`;
  the sequential non-snapshot read-back is re-validated by the batch. Could not refute.
- **Clock staleness between read-back and commit** (leases expire in real time after the
  guard evaluated at the caller's `now`) — this is the residual GC-mid-pass instantaneous
  window the brief explicitly scopes out to getwyrd/wyrd#557; not chargeable to this diff.
- Noted, not a refutation: `commit_overwrite` doubling `orphaned_at_millis` as the lease
  `now` (`crates/core/src/write.rs:303-311`) is a footgun for any future caller passing a
  stale/zero orphan stamp (every dst test passes `0`), but it is exactly what obligation (e)
  mandated for test-signature stability.
