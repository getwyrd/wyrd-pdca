# Brief — issue 490 / renew-pending-lease-resurrection (iteration 2)

> The Plan artifact (docs 02 §PLAN). Do reads ONLY this file.
> Iteration 2: iteration 1's renewal-seam fix was proven red→green but REJECTED as a
> partial fix — the adversary probe showed the COMMIT seam still publishes over reaped
> leases. This brief covers BOTH seams. The full previous attempt is preserved in
> `iteration-v1/` (see Citations expected — Do MAY reuse its renewal fix).

- **Slug:** renew-pending-lease-resurrection
- **Defect:** A streaming PUT can publish an object whose chunk map points at fragments
  the custodian GC is entitled to reclaim, through TWO seams of the pending-lease
  lifecycle. Seam 1 (renewal): `metadata::renew_pending`
  (`crates/core/src/metadata.rs:525-538`) does a blind `put` of every `pending:<id>`
  entry ("A plain overwrite … so it is idempotent", its own doc, `metadata.rs:523-524`);
  if the upload stalls past `lease_ttl_millis` between two chunks and the sweep runs
  (`write::sweep_expired_leases`, `crates/core/src/write.rs:563-583`, reaping at
  `lease_expiry_millis <= now`, `write.rs:572`), the next chunk's renewal
  (`lease_write_chunk`, `write.rs:431-440` — which also DISCARDS the `CommitOutcome`)
  RECREATES the swept entries. Seam 2 (commit): phase 3 is UNCONDITIONAL on the pending
  ledger — `commit_create` (`write.rs:247-261` → `metadata::create`,
  `metadata.rs:287-303`) and `commit_overwrite` (`write.rs:271-287` →
  `metadata::commit_chunk_map_superseding`, `metadata.rs:459-490`) `require` nothing on
  any `pending:` key — so a lapse the renewal never observes (a stall AFTER the last
  chunk but BEFORE end-of-stream; or the window between `stream_write_data` returning
  and the caller driving phase 3, e.g. during the payload-hash check,
  `crates/server/src/lib.rs:294-302`) commits over reaped leases even with seam 1 fixed
  (adversary probe, iteration 1: sweep on the EOF pull reaps all leases, yet
  `stream_write_data` returns `Ok`, `commit_create` returns `Committed`, `read_path`
  serves the object). Note the bytes are reclaimable while an entry is merely
  present-but-expired: GC's expired-lease input deletes fragment bytes keyed on expiry,
  not on entry deletion (`crates/custodian/src/gc.rs:142-144` — "the lease TTL is its
  grace"). Pre-existing in the streaming write path (#364), surfaced by Codex on the
  #489 M4 integration review, deferred to its own cycle.
- **Success criterion:** No committed inode ever references a chunk whose pending lease
  had lapsed (absent, or `lease_expiry_millis <= now`) when authority over it was last
  exercised — through EITHER seam: (1) renewal refuses to resurrect or extend a lapsed
  lease and the upload aborts instead of proceeding; (2) phase-3 commit refuses (fails
  closed) unless every chunk in the plan still holds a live, unexpired pending entry,
  atomically with the commit itself. The healthy paths are unchanged: a slow upload that
  renews BEFORE expiry still commits and reads back byte-identical (the existing
  `crates/core/tests/stream_lease_renewal.rs` contract stays green, updated only for any
  signature change), and the whole suite (`cargo xtask ci`, incl. DST) stays green.
  Demonstrated by the new test file below: red on base, green with the fix, under
  C4-verify.
- **Falsifiability:** RED is producible in-process on the base toolchain, on `origin/main`,
  against the production `write::stream_write_data` (`write.rs:481`) over
  `RedbMetadataStore::in_memory()` + `FsChunkStore` (tempdir) — the peer suite's harness,
  no cfg gate. Three independent reds (see Repro): (A) mid-upload lapse — the
  test-controlled input stream sweeps between chunk yields; base resurrects and the
  upload proceeds; (B) EOF-window lapse — the stream sweeps on the EOF pull; base
  returns an `Ok` plan and `commit_overwrite` publishes it; (C) present-but-expired at
  commit — no sweep at all; base commits over entries whose bytes `gc.rs:142-144`
  already treats as reclaimable. C4-verify's red leg reverts EVERY modified file
  (production AND modified test suites) and keeps only the ADDED test file, so the added
  test MUST COMPILE against the base tree too — a compile-error "red" passes the gate
  mechanically but is dishonest and will be refuted at review. Hence the binding
  constraint: the added test drives phase 3 ONLY through signature-stable APIs —
  `write::commit_overwrite(meta, inode_id, &prior, &plan, now)` already carries a
  commit-time instant (`orphaned_at_millis`, `write.rs:271-287`); it must NOT call
  `commit_create` if the fix changes that signature (it has no `now` today,
  `write.rs:247-253`, and will need one).
- **Invariant to restore:** A lapsed lease is DEAD — **through publish**. Renewal may
  only extend a lease that still exists and has not expired, and a commit may only
  publish chunks whose leases are still alive at the commit's own atomic decision point;
  neither may re-create or trade on authority the sweep contract already revoked. "A
  stall longer than the TTL between two chunks still lapses — a genuinely dead upload
  the sweep should reap" is the write path's own stated contract (`write.rs:417-418`);
  until commit, an in-flight chunk's fragments are protected ONLY by an unexpired
  pending lease (proposal 0005 pending-ledger semantics; `write.rs:409-415`), and GC
  reclaims their bytes keyed on expiry of even a still-present entry
  (`crates/custodian/src/gc.rs:142-144`) — so an upload whose protection lapsed must
  fail closed rather than publish missing fragments (the never-wrong-bytes / durability
  floor, issue #364 finding 2). Both lease consumers agree on the boundary: dead at
  `lease_expiry_millis <= now`, exactly as the sweep reaps (`write.rs:572`).
- **Repo + branch target:** getwyrd/wyrd @ main
- **Surfaces:** data
- **Difficulty:** high   (the pending-lease CONTRACT spans the write-path producer
  (`write.rs`), the ledger (`metadata.rs`), the phase-3 committers and their shared
  gateway driver (`server/src/lib.rs:178-203`), and the sweep/GC reapers; a
  lease-conditional commit also touches every existing suite that drives phase 3
  directly — `server/tests/{erasure_path,write_path,dst_commit,read_path}.rs`,
  `dst/tests/{concurrency,network,commit_ambiguity,tikv_await_commit_interleaving}.rs` —
  any that skip `intent` must be updated to follow the protocol's phase order)
- **Scope:** one logical fix — a lapsed lease is dead through publish — five obligations:
  (a) [carried from iteration 1, proven red→green — reuse `iteration-v1/patch.diff`]
  make lease renewal CONDITIONAL and ATOMIC: read the current pending entries back, then
  commit `require(pending_key, current-value)` + `put(new-value)` per chunk in ONE
  `WriteBatch` (`crates/traits/src/lib.rs:664-680`; preconditions and writes evaluate
  atomically). A read-verify-then-blind-renew in TWO commits is NOT acceptable — a sweep
  can interleave between the check and the put;
  (b) treat existence and expiry both as renewal conditions: refuse when an entry is
  ABSENT (swept) **or** present but `lease_expiry_millis <= now` — the `<=` boundary,
  matching the sweep's reap condition (`write.rs:572`); iteration 1 used `<` and its doc
  called `now == expiry` a healthy deadline renewal — that disagrees with the reaper and
  must be aligned;
  (c) [carried from iteration 1] SURFACE the renewal refusal: `lease_write_chunk`
  currently discards the `CommitOutcome` (`write.rs:431-440`); a refusal must become a
  hard error that aborts the upload BEFORE the next chunk is written toward commit;
  (d) [NEW — the iteration-1 rejection] make phase-3 commit LEASE-CONDITIONAL: both
  `commit_create` and `commit_overwrite` must refuse unless, for EVERY chunk in the
  plan, the `pending:<id>` entry is present, unexpired (`lease_expiry_millis > now` at
  the commit's own instant), and unchanged since read-back — enforced by threading
  `require(pending_key, read-back-value)` per chunk into the SAME `WriteBatch` as the
  inode create/CAS (`metadata::create`, `metadata.rs:294-301`;
  `commit_chunk_map_superseding`, `metadata.rs:474-476`), so a racing sweep yields
  `Conflict`, never a publish. A separate verify-then-commit in two commits is NOT
  acceptable, same interleave argument as (a). Absent-or-expired detected at read-back
  may surface as a distinct hard error; the mechanism split is Do's call, but the
  refusal must fail the PUT (the gateway's existing `Conflict → GatewayError::Conflict`
  mapping, `server/src/lib.rs:196-202`, is an acceptable client surface);
  (e) the commit-time instant: `commit_overwrite`'s existing `orphaned_at_millis`
  parameter IS the commit-time now at its one production callsite
  (`server/src/lib.rs:184` passes `now_millis()`) — reuse/re-document it, do NOT change
  its signature (the added test depends on its stability, see Falsifiability);
  `commit_create` needs a `now` parameter added — its callers all have one in scope
  (`write_new_object`, `write.rs:309/316`; `write_new_object_placed`,
  `write.rs:343/351`; `server/src/lib.rs:193` via `now_millis()`), and
  `stream_lease_renewal.rs:92` plus any other direct test caller is updated (modified
  files are reverted on C4-verify's red leg, so this breaks nothing there).
  Consequence, in scope: existing suites that drive phase 3 without phase 1 (`intent`)
  must be updated to run `intent` first — the protocol's own phase order — never by
  weakening the commit condition.
  / out of scope: `metadata::commit_chunk_map` (`metadata.rs:421-439` — used by
  reconstruction/backfill whose chunks are already committed and have no pending
  entries; deliberately left unconditional, mirroring its non-orphaning rationale at
  `metadata.rs:453-455`); GC/sweep behaviour (`sweep_expired_leases` and
  `gc::reconcile` are correct — the defect is resurrection/publish, not the reap); the
  residual GC-mid-pass race (a GC pass whose reference set predates a commit that won
  with then-unexpired leases can still reclaim just-committed bytes if expiry elapses
  between the two checks — an instantaneous window inherent to GC's non-transactional
  snapshot, vs today's UNBOUNDED window; closing it means reordering GC to retire
  ledger entries atomically before byte deletion — tracked as the follow-up issue
  getwyrd/wyrd#557, not this cycle); gateway-level error taxonomy/retry beyond the existing Conflict mapping;
  #554's GC deployment wiring.
- **Repro instruction:** On `origin/main`, mirror the harness of
  `crates/core/tests/stream_lease_renewal.rs` (logical clock, TTL=30_000, chunk_size 4,
  scheme None, `RedbMetadataStore::in_memory()` + `FsChunkStore` on a tempdir,
  `pollster::block_on`). Make every scenario OVERWRITE-shaped so phase 3 is drivable
  through the signature-stable `commit_overwrite`: first publish an initial object under
  the key via `write_new_object` (signature unchanged by this fix), keep its payload,
  and resolve `prior` via `read::resolve` + `read::read_inode` (as
  `server/src/lib.rs:179-183` does). The deterministic mid-upload seam is the INPUT
  STREAM (an async generator, e.g. `futures_util::stream::unfold`, sharing the logical
  clock and `&meta`):
  (A) mid-upload lapse (renewal seam): between yielding chunk 1's and chunk 2's bytes,
  jump the clock to 2·TTL and await `write::sweep_expired_leases(&meta, 2*TTL)` —
  assert the sweep really reclaimed the in-flight chunk's id (fault-injection guard),
  then, ON THE `Ok` ARM ONLY, drive `commit_overwrite(&meta, inode, &prior, &plan,
  now)` — this makes the "nothing published" binding genuinely falsifiable (iteration
  1's binding #3 was tautological: it never drove phase 3, so its read-back could never
  return `Some`). Assert: `stream_write_data` errored; the swept `pending:<id>` was NOT
  resurrected (`meta.scan(b"pending:")`); the key still reads back the ORIGINAL
  payload. Base: blind renewal resurrects (`metadata.rs:535`), the stream returns `Ok`,
  the commit publishes, the read returns the new payload → red.
  (B) EOF-window lapse (commit seam — the adversary's probe): the generator sweeps at
  2·TTL on the EOF pull (AFTER the last payload item, i.e. when it returns `None`); no
  next chunk ever fires renewal, so `stream_write_data` returns `Ok(plan)` on BOTH
  trees — then drive `commit_overwrite(&meta, inode, &prior, &plan, 2*TTL)`. Assert:
  the commit REFUSES (`Conflict` or a hard error — accept either refusal shape, not
  `Committed`) and the key still reads back the ORIGINAL payload. Base: `Committed`,
  read returns the new payload → red.
  (C) present-but-expired at commit (no sweep at all): stream completes normally with
  the clock at t; jump the clock past every lease's expiry and drive
  `commit_overwrite(..., now_past_expiry)`. Assert refusal + original payload intact —
  this pins the `<=`-boundary expiry check at commit, the arm `gc.rs:142-144` makes
  load-bearing. Base: `Committed` → red.
  Healthy control (green on both trees, guards fail-closed overreach): a normal
  overwrite whose leases are live and unexpired at commit still returns `Committed`
  and reads back the new payload.
- **External dependencies:** none
- **Test file:** crates/core/tests/stream_lease_lapse.rs   (NEW file — iteration 1's
  patch was never merged, so this path does not exist on `main`; the C4-verify gate
  classifies the red leg on an added `*/tests/*.rs` and runs ONLY that test. HARD
  CONSTRAINT: this file must compile against BOTH the base and the fixed tree — the red
  leg reverts all modified files (including `stream_lease_renewal.rs`) and keeps only
  this file, and a compile error would count as a mechanical "red" that fails for the
  wrong reason. So: drive phase 3 only via `commit_overwrite` (stable signature), never
  via `commit_create`; use only APIs unchanged across the fix. Iteration 1's version of
  this file is preserved at the bundle root (`stream_lease_lapse.rs`) and in
  `iteration-v1/patch.diff` — reuse its generator/clock scaffolding, but reshape per the
  Repro above. `stream_lease_renewal.rs` stays green with only the mechanical
  `commit_create` call-site update.)
- **Citations expected:** Do must cite path:line on `main` for every change. Composition
  peers Do MAY open: `iteration-v1/patch.diff` in this bundle (the PROVEN renewal-seam
  fix — atomic read-back + `require` + `put`, and the surfaced abort in
  `lease_write_chunk`; reuse it, aligning the expiry boundary to `<=` per obligation
  (b)); `crates/core/tests/stream_lease_renewal.rs` (the production-path streaming
  harness to mirror); `WriteBatch::require` / `require_absent`
  (`crates/traits/src/lib.rs:664-680` — the atomic conditional-commit primitive);
  `metadata::create` (`metadata.rs:287-303`) and `commit_chunk_map_superseding`
  (`metadata.rs:459-490`) — the batches the per-chunk requires thread into;
  `crates/custodian/src/gc.rs:142-144` (why present-but-expired is already reclaimable);
  `crates/server/src/lib.rs:178-203` (`commit_written`, the shared phase-3 driver both
  PUT paths flow through).
- **Prior-art check (triage cycles):** re-verified on today's `main` (dc503cd): the
  blind `put` is still at `metadata.rs:535` and both phase-3 committers are still
  unconditional on the pending ledger (Read, `metadata.rs:421-439`, `287-303`,
  `459-490`) — iteration 1's patch was NOT merged (rejected at sign-off,
  `iteration-v1/SUMMARY.md` §9); `git -C ../wyrd log` over `crates/core/src/metadata.rs`
  / `write.rs` since the renewal landed for #364 shows no competing fix; the issue was
  explicitly deferred out of the #489 integration merge to its own cycle.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Iteration 2 — carry-forward (from the previous attempt)

- Sign-off rationale (iteration 1, `iteration-v1/SUMMARY.md` §9): rejected as a PARTIAL
  fix — the renewal-seam fix was proven red→green and its mechanism confirmed sound
  (atomic conditional renewal, surfaced abort), but the adversary probe showed the
  commit seam still publishes over reaped leases (EOF-window stall; the
  stream-return-to-commit window). The success criterion was broader than the fix's
  (too-narrow) Scope.
- This brief's deltas, per the sign-off: Scope now covers BOTH seams (obligation (d):
  lease-conditional commit, atomic with the inode CAS); the expiry boundary is aligned
  to the sweep's contract (`<=`, obligations (b)/(d)); the "nothing published" test
  binding must be non-tautological (drive phase 3 on the `Ok` arm, Repro (A)).
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md,
  SUMMARY.md, check-*). REUSE the renewal fix; do NOT re-ship it alone — the rejected
  outcome was "renewal seam only".

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 1): Check found implementation-level items only, no architectural judgment required — C4 Verification (red→green) — A human must rerun full `cargo xtask ci` on a host permitting loopback binds — focused red→green is independently confirmed, but this sandbox stopped the full suite at `list_delete_over_grpc` with `PermissionDenied` after fmt/clippy/build passed (`crates/core/tests/stream_lease_lapse.rs:319`).; T4 Contribution — A human must confirm closed/rejected work contains no competing affected-path fix — local merged history by affected file showed none after `HEAD` dc503cd, but closed/rejected remote work was unavailable to settle mechanically (`crates/core/src/metadata.rs:640`).; **The `<=` expiry boundary — obligation (b)/(d)'s headline delta over; **The create-seam refusal is untested: dropping the guard threading
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
