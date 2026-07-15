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

Review of issue #490: prevent streaming PUTs from renewing or committing through absent or expired pending leases.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance decision is bounded by one explicit invariant—leases dead at `expiry <= now` must block both renewal and publish—and names the atomicity and healthy-path obligations (`brief.md:31`). |
| C2 Reproduction (red pre-fix) | PASS | The base-compatible lapse test compiled on archived `HEAD` and independently failed all four defect cases while its live-lease control passed, demonstrating renewal, swept-commit, expired-commit, and exact-deadline failures (`crates/core/tests/stream_lease_lapse.rs:185`). |
| C3 Change | PASS | The change is acceptable if both authority decisions remain atomic with their writes; exact-value requirements are in the renewal and create/overwrite commit batches (`crates/core/src/metadata.rs:339`, `crates/core/src/metadata.rs:583`, `crates/core/src/metadata.rs:662`). |
| C4 Verification (red→green) | NEEDS-HUMAN | A human must rerun full `cargo xtask ci` on a host permitting loopback binds—focused red→green and nine patched regression tests passed independently, but this sandbox stopped the full suite at `list_delete_over_grpc` with `PermissionDenied` after fmt/clippy/build passed (`crates/core/tests/stream_lease_lapse.rs:185`). |
| C5 Causal adequacy | PASS | The causal decision is whether revoked authority is eliminated rather than masked; conditional same-batch requirements close both renewal and publish races without a capability probe or downstream symptom guard (`crates/core/src/metadata.rs:328`, `crates/core/src/metadata.rs:558`, `crates/core/src/metadata.rs:652`). |
| T1 Structure | PASS | The structural boundary remains the write protocol plus metadata atomic-commit layer, with production callers selecting leased phase-3 operations and reconstruction's unrelated unconditional path left intact (`crates/core/src/write.rs:268`, `crates/core/src/write.rs:303`). |
| T2 Shape | PASS | The public shape change is limited to adding commit time to `commit_create`; overwrite keeps its stable signature and reuses its existing commit instant, preserving the base-compatible repro contract (`crates/core/src/write.rs:254`, `crates/core/src/write.rs:296`). |
| T3 Runtime | PASS | Runtime behavior is supported by independently passing focused tests for swept, expired, exact-deadline, create, overwrite, renewal, and healthy live-lease paths (`crates/core/tests/stream_lease_renewal.rs:125`, `crates/core/tests/stream_lease_renewal.rs:170`, `crates/core/tests/stream_lease_lapse.rs:206`). |
| T4 Contribution | NEEDS-HUMAN | A human must confirm closed/rejected remote work contains no competing affected-path fix—local merged history by `metadata.rs` and `write.rs` showed none after base `dc503cd`, but closed/rejected work cannot be mechanically settled from the supplied artifacts (`crates/core/src/metadata.rs:640`). |
| T5 Judgment | PASS | The judgment is whether fail-closed conflicts preserve intended healthy writes; the live overwrite and slow-renewal controls passed while lapse cases refused publication (`crates/core/tests/stream_lease_lapse.rs:370`, `crates/core/tests/stream_lease_renewal.rs:125`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | A human must decide whether conflict-on-lapse is the acceptable production PUT outcome and operational contract—the automated evidence establishes safety and healthy-path behavior, but not deployment-level fitness or client expectations (`crates/core/src/write.rs:248`). |

### Advisory — adversary

# check-advisory-adversary.md — issue 490 / renew-pending-lease-resurrection (iteration 3 patch)

Skeptic's pass. I independently re-ran the red→green evidence (not just re-read the gate) and
attacked the fix's boundaries, atomicity claims, and caller contracts. The target worktree was
restored byte-identically after the red leg (live diff sha256 verified before/after).

## Evidence — attempted to refute, could not

- **Red leg re-run (independent):** reverted all 11 modified files to base `dc503cd`, kept only the
  added `crates/core/tests/stream_lease_lapse.rs`, ran it: it **compiles clean against base** (no
  mechanical compile-error "red") and fails with 4 genuine assertion failures for the right reasons —
  `Committed` where refusal is required (`crates/core/tests/stream_lease_lapse.rs:262`, `:305`,
  `:356`) and an `Ok` plan + publish where an abort is required (`stream_lease_lapse.rs:185`); the
  healthy control passes on base. **Green leg re-run:** all 5 added tests plus all 4 in the modified
  `stream_lease_renewal.rs` pass on the patched tree. The C4-verify claim holds.
- **Tautology probe:** Repro (A) drives `commit_overwrite` on the `Ok` arm before asserting
  (`crates/core/tests/stream_lease_lapse.rs:180-181`), so "nothing published" is genuinely
  falsifiable — iteration 1's tautological binding is fixed as the brief demanded. The
  fault-injection guard (`stream_lease_lapse.rs:172-176`) pins that the sweep really reclaimed the
  in-flight chunk id.
- **Production-path probe:** the tests drive `write::stream_write_data`, `write::commit_overwrite`,
  `write::sweep_expired_leases` and `read::read_path` over `RedbMetadataStore::in_memory()` +
  `FsChunkStore` — the production seams; no parallel re-implementation, no mocks.

## Fix — attempted to refute, could not

- **Atomicity / require-after-put ordering:** both leased committers append the per-chunk lease
  `require`s *after* the puts in the builder chain (`crates/core/src/metadata.rs:339-341`,
  `:583-585`). Not a hazard: `WriteBatch` keeps `preconditions` in a separate vec evaluated as a
  set (`crates/traits/src/lib.rs:648-655`) and redb checks every precondition inside the serialized
  write transaction before applying writes (`crates/metadata-redb/src/lib.rs:139-141`). The
  interleave the brief forbade (sweep between read-back and commit) turns the require false — the
  guard value is the exact read-back bytes (`crates/core/src/metadata.rs:694-697`).
- **`<=` boundary:** the new guards (`crates/core/src/metadata.rs:658`, `:694`), the sweep
  (`crates/core/src/write.rs:610`) and GC's expired-lease input (`crates/custodian/src/gc.rs:254`)
  all agree on `expiry <= now`. The exact-deadline tests (`stream_lease_lapse.rs:341-356`, in the
  added, red-proven file; `stream_lease_renewal.rs:851`, `:953`) kill a `<` mutant — iteration 1's
  rejected boundary bug is closed at both seams.
- **Renewal-cadence gap probe:** tried to construct a lapse the renewal check misses: leases are
  renewed en masse to `now+TTL` with `renew_at = now+TTL/2` (`crates/core/src/write.rs:478`,
  `:491`), so any lapse implies `now >= min_expiry > renew_at`, meaning the conditional renewal
  always fires and refuses before the next chunk is written. No gap found.
- **Existing-suite conformance:** suites driving phase 3 directly all run `intent` first
  (pre-existing, or added by this patch, e.g. `crates/server/tests/erasure_path.rs:153-156`);
  `metadata::commit_chunk_map` stays deliberately unconditional for reconstruction/backfill per the
  brief's out-of-scope carve-out.
- **Create-seam coverage:** the create-seam refusal tests live in the *modified*
  `stream_lease_renewal.rs` (`:896-945`, `:953-1000`), so they are post-fix regression guards, not
  part of the red→green proof — but that placement is what the brief's compile-against-base
  constraint itself mandates (the added file may not call the changed-signature `commit_create`).
  Both guards pass green and pin the seam a `create_leased`-only mutant would reopen. Could not
  turn this into a refutation.

## Findings

- NEEDS-HUMAN — **Buffered PUT availability edge (fitness call, not a defect):** the buffered
  `put_object` path (`crates/server/src/lib.rs:158-166`) has **no lease renewal** — it stamps
  `now+TTL` at intent and commits at a fresh `now_millis()` (`crates/server/src/lib.rs:184`,
  `:193`). With `DEFAULT_LEASE_TTL_MILLIS = 30_000` (`crates/server/src/lib.rs:49`), any buffered
  PUT whose data phase takes longer than 30s now deterministically fails at commit with `Conflict`,
  and a retry re-runs the same >TTL data phase, so it can never succeed. This is the invariant
  working as specified (those bytes genuinely are GC-reclaimable; before the patch this published
  the durability hole instead), but it converts silent corruption into hard unavailability for
  large/slow buffered PUTs. A human should confirm this failure mode is acceptable or file a
  follow-up (renew on the buffered path, or route buffered PUTs through the streaming path).
- NEEDS-HUMAN — **Caller-supplied `now` leaves the expiry arm vacuous on the `write_new_object`
  compositions (brief-prescribed, latent):** `write_new_object` / `write_new_object_placed` pass
  their start-of-call `now_millis` as the commit instant (`crates/core/src/write.rs:343`, `:378`),
  so on those paths `expiry = now+TTL > now` is always true — only the absent/changed (`require`)
  arm of the create guard can fire. Today's only production callers are the frozen-clock CLI
  (`crates/server/src/cli.rs:67`, `NOW_MILLIS = 0`, documented "the CLI runs no custodian sweep"),
  so there is **no live defect**; but a future caller reusing these helpers with a real clock
  inherits exactly the Repro-(C) shape (present-but-expired publishes) because wall time elapsed
  inside `write_fragments` is invisible to the guard. Obligation (e) of the brief itself prescribed
  passing this in-scope `now`, so this is a scope/architecture note for the human, not a builder
  iteration item.

## Reviewer-verdict probe

- `check-gates.json` C4 rows: both re-verified independently here (see above); no rationalized
  claim found. The one thing I could **not** re-verify is the full `cargo xtask ci` environment
  (the iteration-2 carry-forward noted a prior sandbox `PermissionDenied` on loopback binds at
  `list_delete_over_grpc`); the gate row records "all checks passed" and is deterministic, so I
  treat it as credible — environment doubt, not substantive doubt (issue #236).

**Bottom line:** attempted to refute the red→green evidence, the batch-atomicity claim, the `<=`
boundary at both seams, the renewal-cadence coverage, and the existing-suite updates; **could not**.
The two NEEDS-HUMAN findings above are consequence/latency notes for sign-off, not holes in the fix.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C4 Verification (red→green) — A human must rerun full `cargo xtask ci` on a host permitting loopback binds—focused red→green and nine patched regression tests passed independently, but this sandbox stopped the full suite at `list_delete_over_grpc` with `PermissionDenied` after fmt/clippy/build passed (`crates/core/tests/stream_lease_lapse.rs:185`).
- [x] T4 Contribution — A human must confirm closed/rejected remote work contains no competing affected-path fix—local merged history by `metadata.rs` and `write.rs` showed none after base `dc503cd`, but closed/rejected work cannot be mechanically settled from the supplied artifacts (`crates/core/src/metadata.rs:640`).
- [x] Validation — fitness-to-purpose — A human must decide whether conflict-on-lapse is the acceptable production PUT outcome and operational contract—the automated evidence establishes safety and healthy-path behavior, but not deployment-level fitness or client expectations (`crates/core/src/write.rs:248`).
- [x] **Buffered PUT availability edge (fitness call, not a defect):** the buffered
- [x] **Caller-supplied `now` leaves the expiry arm vacuous on the `write_new_object`

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: merged-wider
- Iteration delta (if iterating):
- By / date: Eduard Ralph / 2026-07-15

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
- File follow-up, milestone 0.1 Alpha: buffered `put_object` has no lease renewal, so a buffered PUT slower than the 30 s TTL now deterministically fails with Conflict and cannot succeed on retry — renew on the buffered path or route buffered PUTs through streaming (adversary finding, #490 §6 item 4).
- Follow-up: `write_new_object` / `write_new_object_placed` pass their start-of-call `now` as the commit instant, so the commit guard's present-but-expired arm is vacuous on those helper paths (latent — no live production caller today); document/harden the contract (fresh commit-instant or threaded clock) so a future real-clock caller doesn't inherit the Repro-(C) publish-over-expired shape (adversary finding, #490 §6 item 5).
