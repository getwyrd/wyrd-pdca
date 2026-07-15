# Result — issue 490 / renew-pending-lease-resurrection

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: A streaming PUT can publish an object whose chunk map points at fragments
- Success criterion: No committed inode ever references a chunk whose pending lease
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope: one logical fix — a lapsed lease is dead through publish — five obligations:

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it.
- C5 Causal adequacy: none — reviewer + human sign-off

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 Contribution: none — (no gate configured)
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of issue #490: prevent streaming PUT renewal or commit from publishing chunks after their pending leases lapse.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance boundary is testable at both renewal and phase-3 commit, including the sweep-aligned `expiry <= now` boundary and an unchanged live path (`crates/core/tests/stream_lease_lapse.rs:184`). |
| C2 Reproduction (red pre-fix) | PASS | An exact-HEAD disposable red leg compiled the added test, failed all three defect assertions, and retained the passing live control (`crates/core/tests/stream_lease_lapse.rs:258`). |
| C3 Change | PASS | Publication now turns on live-lease authority pinned atomically in the inode batch, while renewal refusal is surfaced before another chunk is written (`crates/core/src/metadata.rs:328`; `crates/core/src/write.rs:458`). |
| C4 Verification (red→green) | NEEDS-HUMAN | A human must rerun full `cargo xtask ci` on a host permitting loopback binds — focused red→green is independently confirmed, but this sandbox stopped the full suite at `list_delete_over_grpc` with `PermissionDenied` after fmt/clippy/build passed (`crates/core/tests/stream_lease_lapse.rs:319`). |
| C5 Causal adequacy | PASS | The decision point is the authority itself: absent/expired leases refuse, and read-back values become same-batch preconditions, so neither checked seam relies on a symptom guard or capability probe (`crates/core/src/metadata.rs:682`). |
| T1 Structure | PASS | Renewal, create, and overwrite enforce the invariant at the shared metadata/write boundaries used by production phase 3 (`crates/server/src/lib.rs:178`). |
| T2 Shape | PASS | Both create and overwrite carry every planned chunk into lease guards without weakening the unconditional reconstruction/backfill API (`crates/core/src/write.rs:268`; `crates/core/src/write.rs:303`). |
| T3 Runtime | PASS | The applied target passed all four focused runtime scenarios, including byte-identical publication for a live lease (`crates/core/tests/stream_lease_lapse.rs:337`). |
| T4 Contribution | NEEDS-HUMAN | A human must confirm closed/rejected work contains no competing affected-path fix — local merged history by affected file showed none after `HEAD` dc503cd, but closed/rejected remote work was unavailable to settle mechanically (`crates/core/src/metadata.rs:640`). |
| T5 Judgment | PASS | The patch is confined to the two specified lease seams, the required commit-time plumbing, protocol-correct test setup, and regression coverage; no ambiguous scope re-entry was found (`crates/core/src/write.rs:296`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | The human must decide whether fail-closed `Conflict` behavior is operationally acceptable for real streaming PUT clients — it preserves durability but may surface retries at the gateway (`crates/server/src/lib.rs:196`). |

### Advisory — adversary

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

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — A human must rerun full `cargo xtask ci` on a host permitting loopback binds — focused red→green is independently confirmed, but this sandbox stopped the full suite at `list_delete_over_grpc` with `PermissionDenied` after fmt/clippy/build passed (`crates/core/tests/stream_lease_lapse.rs:319`).
- [ ] T4 Contribution — A human must confirm closed/rejected work contains no competing affected-path fix — local merged history by affected file showed none after `HEAD` dc503cd, but closed/rejected remote work was unavailable to settle mechanically (`crates/core/src/metadata.rs:640`).
- [ ] Validation — fitness-to-purpose — The human must decide whether fail-closed `Conflict` behavior is operationally acceptable for real streaming PUT clients — it preserves durability but may surface retries at the gateway (`crates/server/src/lib.rs:196`).
- [ ] **The `<=` expiry boundary — obligation (b)/(d)'s headline delta over
- [ ] **The create-seam refusal is untested: dropping the guard threading

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Auto-iterate (round 1): Check found implementation-level items only, no architectural judgment required — C4 Verification (red→green) — A human must rerun full `cargo xtask ci` on a host permitting loopback binds — focused red→green is independently confirmed, but this sandbox stopped the full suite at `list_delete_over_grpc` with `PermissionDenied` after fmt/clippy/build passed (`crates/core/tests/stream_lease_lapse.rs:319`).; T4 Contribution — A human must confirm closed/rejected work contains no competing affected-path fix — local merged history by affected file showed none after `HEAD` dc503cd, but closed/rejected remote work was unavailable to settle mechanically (`crates/core/src/metadata.rs:640`).; **The `<=` expiry boundary — obligation (b)/(d)'s headline delta over; **The create-seam refusal is untested: dropping the guard threading
- By / date: auto-iterate / 2026-07-15

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
