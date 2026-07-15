# Brief — issue 490 / renew-pending-lease-resurrection

> The Plan artifact (docs 02 §PLAN). Do reads ONLY this file.

- **Slug:** renew-pending-lease-resurrection
- **Defect:** `metadata::renew_pending` (`crates/core/src/metadata.rs:525-538`) does a
  blind `put` of every `pending:<id>` entry ("A plain overwrite … so it is idempotent",
  its own doc). If a streaming PUT stalls past `lease_ttl_millis` and the pending sweep
  runs (`core::sweep_expired_leases`, `crates/core/src/write.rs:560-582`, promoted to the
  custodian GC's expired-lease input, `crates/custodian/src/gc.rs:109-146`), the pending
  entries AND their uncommitted fragment bytes can already be reclaimed; the next renewal
  from `lease_write_chunk` (`crates/core/src/write.rs:431-440`) then RECREATES the
  pending records, and the upload later commits a chunk map pointing at bytes GC
  reclaimed — an object published with missing fragments. Pre-existing in the streaming
  write path (#364), surfaced by Codex on the #489 M4 integration review and deferred out
  of that merge to its own cycle.
- **Success criterion:** A streaming PUT whose in-flight lease LAPSES (the sweep reclaims
  its pending entries mid-upload) can no longer publish: renewal refuses to resurrect
  pending entries that no longer exist, the upload fails instead of committing, and no
  committed inode references reclaimed fragments. The healthy path is unchanged — a slow
  upload that renews BEFORE expiry still commits and reads back byte-identical (the
  existing `crates/core/tests/stream_lease_renewal.rs` contract must stay green).
  Demonstrated by the new test file below: red on base, green with the fix, under
  C4-verify.
- **Falsifiability:** RED is producible in-process on the base toolchain: on
  `origin/main`, drive the production `write::stream_write_data`
  (`crates/core/src/write.rs:481`) with a test-controlled INPUT STREAM whose async
  generator, between two chunk yields, advances the logical clock past the TTL and awaits
  `sweep_expired_leases` (reclaiming the early chunk's pending entry MID-upload — the
  stream is the deterministic seam; a clock jump alone cannot run the sweep inside the
  call, see Repro). Today the blind renewal then resurrects the swept entries and the
  upload COMMITS; the new test's "upload must error / nothing commits / no `pending:<id>`
  resurrected" assertions fail. Plain `#[test]` over `RedbMetadataStore::in_memory()` +
  `FsChunkStore` (tempdir), exactly the peer suite's harness; no cfg gate; C4-verify's
  red leg reverts the production files and keeps the added test file.
- **Invariant to restore:** A lapsed lease is DEAD: renewal may only extend a lease that
  still exists — it must never re-create authority the sweep already revoked. "A stall
  longer than the TTL between two chunks still lapses — a genuinely dead upload the sweep
  should reap" is the write path's own stated contract (`crates/core/src/write.rs:417-418`);
  until commit, an in-flight chunk's fragments are protected ONLY by an unexpired pending
  lease (proposal 0005 pending-ledger semantics; `write.rs:409-415`), so an upload whose
  protection lapsed must fail closed rather than publish missing fragments (the
  never-wrong-bytes / durability floor, issue #364 finding 2).
- **Repo + branch target:** getwyrd/wyrd @ main
- **Surfaces:** data
- **Difficulty:** high   (raised from medium after adversarial review: the blast radius is
  not two files but the pending-lease CONTRACT shared across the write-path producer
  (`write.rs`), the ledger (`metadata.rs`), and the sweep/GC reaper consumers — a
  reviewer must hold the whole lease lifecycle plus an atomic-CAS renewal protocol and an
  upload-abort path in view; rated up per the when-unsure rule)
- **Scope:** one logical fix, three obligations:
  (a) make lease renewal CONDITIONAL and ATOMIC — read the current pending entries back,
  then commit `require(pending_key, current-value)` + `put(new-value)` per chunk in ONE
  `WriteBatch` (`crates/traits/src/lib.rs:664`; the store evaluates preconditions and
  writes atomically). A read-verify-then-blind-renew in TWO commits is NOT an acceptable
  mechanism — a sweep can interleave a delete between the check and the put;
  (b) treat existence and expiry both as conditions: refuse renewal when an entry is
  ABSENT (swept) **or** present but its recorded `lease_expiry_millis` < now (lapsed but
  not yet reaped — renewing it would resurrect authority the contract already revoked,
  `write.rs:417-418`);
  (c) SURFACE the refusal: `lease_write_chunk` currently discards the `CommitOutcome`
  (`write.rs:431-440` — `.await?` unwraps the `Result` and drops the value, so a
  `CommitOutcome::Conflict` would sail through today). The renewal path must turn a
  failed condition into a hard error that aborts the upload BEFORE the next chunk is
  written toward commit.
  / out of scope: GC/sweep behaviour (`sweep_expired_leases` and `gc::reconcile` are
  correct — the defect is the resurrection, not the reap), the non-streaming write path
  (its lease is stamped once and never renewed), gateway-level retry/error mapping, and
  #554's GC deployment wiring.
- **Repro instruction:** On `origin/main`, mirror the harness of
  `crates/core/tests/stream_lease_renewal.rs` (logical clock, TTL=30_000, 3-chunk
  stream) — but note its ONE inadequacy for this repro: that suite runs the sweep only
  AFTER `stream_write_data` returns, and the whole upload happens inside that single
  call (`write.rs:505-555`), so a clock jump alone can never place the sweep MID-upload.
  The deterministic mid-upload seam is the input stream itself, which the test controls:
  build it with an async generator (e.g. `futures_util::stream::unfold`) that, BETWEEN
  yielding chunk 1's and chunk 2's bytes, advances the shared logical clock past the TTL
  and awaits `sweep_expired_leases(&meta, now)` (both borrow `&meta` shared — fine
  in-process). Then let the stream finish. Observe pre-fix: `renew_pending` re-puts the
  swept `pending:<chunk1>` (metadata.rs:535-537) and the upload commits an inode whose
  chunk map includes the reclaimed chunk.
- **External dependencies:** none
- **Test file:** crates/core/tests/stream_lease_lapse.rs   (NEW file — the C4-verify gate
  earns its red only from an added `*/tests/*.rs`; the existing
  `stream_lease_renewal.rs` suite stays untouched and green. SHAPE THE RED HONESTLY:
  `sweep_expired_leases` deletes ONLY the ledger entries — the fragment BYTES stay on the
  store ("reclaiming them needs a chunk-store delete", `write.rs:561-562`; the byte
  deletion is the custodian GC's, not run here) — so a pre-fix read-back of the committed
  object still SUCCEEDS in-process. Do NOT assert on read-back loss; assert that the
  upload ERRORS / nothing commits / the swept `pending:<id>` keys are not resurrected —
  those are red pre-fix, green post-fix. Optionally ALSO delete the reclaimed chunk's
  fragment bytes by hand to demonstrate the end-to-end missing-fragment publish, but the
  binding red must not depend on it.)
- **Citations expected:** Do must cite path:line on `main` for every change. Composition
  peers Do MAY open: `crates/core/tests/stream_lease_renewal.rs` (the production-path
  streaming harness — logical clock + `stream_write_data` + `sweep_expired_leases` — to
  mirror for the lapse test), and `WriteBatch::require` / `require_absent`
  (`crates/traits/src/lib.rs:664-680`) — the conditional-commit primitive the metadata
  stores already implement.
- **Prior-art check (triage cycles):** searched by file path — `git -C ../wyrd log` over
  `crates/core/src/metadata.rs` / `write.rs`: the renewal itself landed for #364
  (stream_lease_renewal suite) but `renew_pending` is still a blind `put` on today's
  `main` (verified by Read, metadata.rs:535-537); nothing since (47bd35c, 985867c,
  5803c48 touch other seams). The issue was explicitly deferred OUT of the #489
  integration merge to its own cycle — no competing open work found.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected as a partial fix: the adversary probe confirmed on the patched tree that the commit seam still publishes over reaped leases — a stall after the last chunk but before end-of-stream never triggers renewal, and commit_create/commit_chunk_map are unconditional on the pending ledger, so the brief's success criterion ("no committed inode references reclaimed fragments") is not met even though the patch conforms to its (too-narrow) Scope. Rewrite the brief's Scope to cover BOTH seams: keep the renewal fix (atomic conditional renewal + surfaced abort, already proven red→green) and add the commit-seam design decision — lease-conditional commit or a pre-commit lease re-verification protocol — so the invariant "a lapsed lease is dead" holds through publish. While amending: align the expiry boundary in obligation (b) to the sweep's contract (`<=` not `<`, sweep reaps at lease_expiry_millis <= now), and require test binding #3 to be non-tautological (drive phase 3 on the Ok arm so "committed inode references reclaimed chunk" can genuinely go red).
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
