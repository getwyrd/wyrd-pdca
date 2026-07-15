# Result — issue 490 / renew-pending-lease-resurrection

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: `metadata::renew_pending` (`crates/core/src/metadata.rs:525-538`) does a
- Success criterion: A streaming PUT whose in-flight lease LAPSES (the sweep reclaims
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope: one logical fix, three obligations:

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

Review of issue #490: prevent a lapsed streaming-write lease from being resurrected and publishing a plan that references reclaimable fragments.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance boundary is decidable: absent or expired authority must abort while renewal at the deadline remains valid, matching the lease contract at `crates/core/src/write.rs:417`. |
| C2 Reproduction (red pre-fix) | NEEDS-HUMAN | Accept the asserted pre-fix failure or require a rerun in a disposable worktree — the read-only target prevented stashing production changes, although the applied regression scenario did sweep chunk 1 and pass at `crates/core/tests/stream_lease_lapse.rs:102`. |
| C3 Change | PASS | The safety decision is enforced at both required boundaries: conditional equality prevents delete/renew races at `crates/core/src/metadata.rs:563`, and conflict aborts before another chunk write at `crates/core/src/write.rs:446`. |
| C4 Verification (red→green) | NEEDS-HUMAN | Decide whether focused green tests plus fmt/clippy are sufficient without an independently reproduced red leg or aggregate CI — both focused tests passed, but the asserted `./engine/xtask.sh` and `run-verify.sh` wrappers were absent; the principal green assertion is `crates/core/tests/stream_lease_lapse.rs:113`. |
| C5 Causal adequacy | PASS | The change removes blind resurrection authority rather than adding a capability probe or downstream symptom guard, and atomically couples the observed value to renewal at `crates/core/src/metadata.rs:553`. |
| T1 Structure | PASS | The responsibility split remains coherent: ledger validity is decided in metadata and the streaming producer converts refusal into upload failure at `crates/core/src/write.rs:437`. |
| T2 Shape | PASS | Scope is confined to the renewal contract, its sole caller, and one production-path regression test; the only call site is `crates/core/src/write.rs:437`. |
| T3 Runtime | PASS | The deterministic mid-upload lapse test and healthy slow-renewal test both passed locally, exercising failure and compatibility around `crates/core/tests/stream_lease_lapse.rs:75`. |
| T4 Contribution | NEEDS-HUMAN | Confirm no closed/rejected competing work exists — merged/all-local-ref history was searched by every affected path and showed no later renewal fix, but unavailable forge PR refs prevent mechanically settling closed/rejected work. |
| T5 Judgment | PASS | The fail-closed durability tradeoff is proportionate: a revoked upload loses progress rather than allowing a commit plan over GC-eligible bytes, as enforced at `crates/core/src/write.rs:446`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether aborting the upload with the boxed lease-lapse error is the intended operator/client experience — this determines whether the safety fix is usable at the real streaming PUT boundary beyond the in-process proof at `crates/core/tests/stream_lease_lapse.rs:113`. |

### Advisory — adversary

# check-advisory-adversary.md — issue 490 / renew-pending-lease-resurrection

Adversarial pass. I independently re-ran the red→green proof in a scratch copy of the
target tree (green with the patch; red on base production files with the test kept —
fails at the binding "upload must abort" assertion, on the real `stream_write_data` +
`RedbMetadataStore` + `FsChunkStore` production path, no mocks). The evidence is genuine
and the healthy-path suite (`crates/core/tests/stream_lease_renewal.rs`) stays green.
The fix's mechanism is sound where it applies: read-back + `require(current)`+`put` in one
batch (`crates/core/src/metadata.rs:550-565`) closes the check/put interleave, and
`lease_write_chunk` now hard-errors on `Conflict` (`crates/core/src/write.rs:446-451`).
I could not refute the renewal seam itself. I did refute the *invariant claim*:

- NEEDS-HUMAN — **The invariant is not restored; the fix closes the renewal seam but the
  commit seam still publishes over reaped leases.** Renewal only fires inside
  `lease_write_chunk` when a *next* chunk arrives (`crates/core/src/write.rs:431`); a
  stall past the TTL **after the last chunk but before end-of-stream** (a client that
  wrote all bytes and then hangs before closing) never triggers it, and
  `commit_create`/`commit_chunk_map` are unconditional on the pending ledger
  (`crates/core/src/write.rs:247-261`, `crates/core/src/metadata.rs:421-439` — no
  `require` on any `pending:` key). **Probe confirmed on the patched tree**: a 3-chunk
  stream whose generator sweeps at 2·TTL on the EOF pull has all three leases reaped
  mid-call (`sweep_expired_leases` returns [1,2,3]), yet `stream_write_data` returns
  `Ok`, `commit_create` returns `Committed`, and `read_path` serves the object — a
  committed inode referencing chunks whose leases the sweep revoked, exactly the
  never-wrong-bytes violation the brief says can "no longer publish" (success criterion)
  and must "fail closed" (invariant). The same window exists between `stream_write_data`
  returning and the caller driving phase 3 (e.g. during a payload-hash check). The brief's
  Scope narrowed the mechanism to renewal + surfacing, so the patch conforms to its brief —
  but the brief's success criterion ("no committed inode references reclaimed fragments")
  is broader than what the fix delivers. Closing it fully needs a design decision
  (lease-conditional commit, or a pre-commit lease re-verification protocol) — human
  scope call: accept as a partial fix with a follow-up issue, or extend this cycle.

- NEEDS-HUMAN [impl] — **Boundary disagreement between reaper and renewer at
  `now == expiry`.** `sweep_expired_leases` reaps at `lease_expiry_millis <= now_millis`
  (`crates/core/src/write.rs:584`); the new renewal refuses only at strict `<`
  (`crates/core/src/metadata.rs:559`), and its doc claims `now == expiry` is a "healthy
  renewal-at-the-deadline path" (`metadata.rs:534`) — but per the sweep's own contract
  that lease is already dead (a sweep at that same instant reaps it). No unsound
  interleave results (the atomic `require` serializes renewal against the sweep, so
  whichever commits first wins cleanly), so this is a conformance/doc nit, not a
  durability hole — but the two consumers of the lease contract should agree on the
  boundary (note: the brief itself specified `<` in obligation (b), so aligning to `<=`
  needs a one-word brief amendment).

- NEEDS-HUMAN [impl] — **Test binding #3 is tautological as written.**
  `crates/core/tests/stream_lease_lapse.rs:281-287` asserts `read_path(...) == None`
  under the banner "nothing was committed" — but the test never drives phase 3
  (`commit_create`) on any path, so no implementation could ever make that read return
  `Some`; the assertion can't go red on its own (pre-fix the test dies earlier at binding
  #1). The red→green is legitimately carried by bindings #1 and #2 (verified), so this is
  a test-strength nit only: committing the plan on the `Ok` arm (as the pre-fix behaviour
  allows) would make #3 a genuine "committed inode references reclaimed chunk" red.

Attempted and could NOT refute: (a) the red→green proof — reproduced independently, red
fails for the right reason on the production path; (b) the atomicity of the conditional
renewal — `WriteBatch` preconditions are evaluated with the writes in one commit
(`crates/traits/src/lib.rs:648-670`), so a sweep between read-back and commit yields
`Conflict`, not resurrection; (c) hidden callers — `renew_pending` has exactly one caller
(`write.rs:437`), so the signature change breaks nothing else; (d) a mid-stream lapse the
renewal misses — for any lapse observable on the writer's own clock, `now ≥ expiry >
renew_at` forces the renewal to fire before the next chunk is written, so the only
escapes are the end-of-stream/commit window (finding 1) and cross-process clock skew
(subsumed by finding 1's commit-seam gap).

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C2 Reproduction (red pre-fix) — Accept the asserted pre-fix failure or require a rerun in a disposable worktree — the read-only target prevented stashing production changes, although the applied regression scenario did sweep chunk 1 and pass at `crates/core/tests/stream_lease_lapse.rs:102`.
- [ ] C4 Verification (red→green) — Decide whether focused green tests plus fmt/clippy are sufficient without an independently reproduced red leg or aggregate CI — both focused tests passed, but the asserted `./engine/xtask.sh` and `run-verify.sh` wrappers were absent; the principal green assertion is `crates/core/tests/stream_lease_lapse.rs:113`.
- [ ] T4 Contribution — Confirm no closed/rejected competing work exists — merged/all-local-ref history was searched by every affected path and showed no later renewal fix, but unavailable forge PR refs prevent mechanically settling closed/rejected work.
- [ ] Validation — fitness-to-purpose — Decide whether aborting the upload with the boxed lease-lapse error is the intended operator/client experience — this determines whether the safety fix is usable at the real streaming PUT boundary beyond the in-process proof at `crates/core/tests/stream_lease_lapse.rs:113`.
- [ ] **The invariant is not restored; the fix closes the renewal seam but the
- [ ] **Boundary disagreement between reaper and renewer at
- [ ] **Test binding #3 is tautological as written.**

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Plan
- Iteration delta (if iterating): Rejected as a partial fix: the adversary probe confirmed on the patched tree that the commit seam still publishes over reaped leases — a stall after the last chunk but before end-of-stream never triggers renewal, and commit_create/commit_chunk_map are unconditional on the pending ledger, so the brief's success criterion ("no committed inode references reclaimed fragments") is not met even though the patch conforms to its (too-narrow) Scope. Rewrite the brief's Scope to cover BOTH seams: keep the renewal fix (atomic conditional renewal + surfaced abort, already proven red→green) and add the commit-seam design decision — lease-conditional commit or a pre-commit lease re-verification protocol — so the invariant "a lapsed lease is dead" holds through publish. While amending: align the expiry boundary in obligation (b) to the sweep's contract (`<=` not `<`, sweep reaps at lease_expiry_millis <= now), and require test binding #3 to be non-tautological (drive phase 3 on the Ok arm so "committed inode references reclaimed chunk" can genuinely go red).
- By / date: Eduard Ralph / 2026-07-15

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
