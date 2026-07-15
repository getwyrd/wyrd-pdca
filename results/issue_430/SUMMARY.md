# Result — issue 430 / fragment-identity-validation

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: The shared read/repair validation accepts a decoded fragment on
- Success criterion: A store that returns a validly-encoded fragment of the SAME
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope: one logical fix — the shared validation boundary in `crates/core`

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): fail — xtask: `cargo test --workspace --exclude wyrd-dst` failed with exit status: 101
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

Review of issue #430: reject fragments whose decoded index or EC tuple does not match the committed fragment identity, preventing wrong RS reconstruction input and queuing repair.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance boundary is falsifiable and complete: the requested fragment's chunk, index, scheme type, and stripe geometry must all agree before admission (`crates/core/src/repair.rs:58`). |
| C2 Reproduction (red pre-fix) | NEEDS-HUMAN | Confirm the public-surface tests fail by assertion with production changes reverted — this artifact-only review could not mutate/stash the target, and the asserted `engine/scripts/run-verify.sh` red-leg wrapper is absent (`crates/core/tests/fragment_identity.rs:152`). |
| C3 Change | PASS | The shared predicate covers the complete identity and is used before both single-copy return and RS shard admission, closing the backend-independent wrong-bytes path (`crates/core/src/repair.rs:63`, `crates/core/src/read.rs:237`, `crates/core/src/read.rs:331`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Decide whether focused green plus partial CI is sufficient without an independent red rerun or full host-capable CI — all 3 public tests and the helper test passed, while `cargo xtask ci` reached an unrelated loopback bind `PermissionDenied` after fmt/clippy/build passed (`crates/core/tests/fragment_identity.rs:152`). |
| C5 Causal adequacy | PASS | The change removes the incomplete admission condition at the shared boundary rather than adding a capability probe or downstream runtime guard (`crates/core/src/repair.rs:63`). |
| T1 Structure | PASS | One shared identity predicate governs core helpers and both inline read gates, while maintenance callers supply their already-committed identity context (`crates/core/src/repair.rs:58`, `crates/custodian/src/reconstruction.rs:391`). |
| T2 Shape | PASS | The dedicated new integration test drives the public read and repair-queue surfaces with deterministic below-k adversarial stores, including same-type wrong geometry (`crates/core/tests/fragment_identity.rs:152`, `crates/core/tests/fragment_identity.rs:322`). |
| T3 Runtime | PASS | Independently running the focused target tests completed successfully: 3/3 public identity cases and the shared-helper identity test passed (`crates/core/tests/fragment_identity.rs:152`). |
| T4 Contribution | NEEDS-HUMAN | Confirm no closed/rejected upstream PR already covers the affected paths — all-local-ref history was checked by path and shows only earlier chunk-only validation, but GitHub lookup was unavailable (`crates/core/src/read.rs:237`). |
| T5 Judgment | PASS | No scope or root-cause ambiguity remains in the reviewed patch: it changes only validation propagation and fixtures required by the widened invariant (`crates/core/src/repair.rs:58`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether public in-process adversarial-store coverage is representative enough for the production assurance boundary — it proves rejection and repair queuing but sign-off owns fitness for real backend corruption (`crates/core/tests/fragment_identity.rs:152`). |

### Advisory — adversary

# check-advisory-adversary.md — issue 430 / fragment-identity-validation (iteration 4)

Adversarial pass. I assumed the patch was wrong and tried to prove it. Evidence below is
from independent re-execution at `$PDCA_TARGET` (patch applied) and a scratch copy with
`crates/core/src/read.rs` + `crates/core/src/repair.rs` reverted to `HEAD` (red leg).

## Refutation attempts that FAILED (the fix survived)

- **Red→green is genuine, independently reproduced.** With the two production files
  reverted to base and the new test file kept, all 3 tests in
  `crates/core/tests/fragment_identity.rs:204` / `:373` fail **by assertion** — the real
  `read::read_object` returns wrong bytes (shard-1 payload duplicated at both data
  positions), exactly the defect claimed — and all 3 pass on the patched tree. Not a
  compile-error red, not a tautology: the tests drive the public production path
  (`read::read_object` + `repair::queued_repairs`) over a trait-level store double.
- **Attempted a "passes for the wrong reason" attack; could not.** The deterministic-red
  shape (serve exactly k fragments, one wrong-identity, slot 2 absent —
  `fragment_identity.rs:598`) forces the decoder to consume the bad shard pre-fix
  regardless of fan-out completion order (`crates/core/src/read.rs:314` polls all n; both
  served slots are admitted pre-fix), so the red cannot be order-dependent.
- **Attempted to find an unconverted admission gate; could not.** Every production
  admission site now goes through `repair::header_matches_identity`
  (`crates/core/src/repair.rs:58`): both inline read gates (`read.rs:237`, `read.rs:331`),
  reconstruction (`crates/custodian/src/reconstruction.rs:391`), scrub
  (`crates/custodian/src/scrub.rs:126`), rebalance (`crates/custodian/src/rebalance.rs:266`).
  No remaining chunk-id-only `decoded.header.chunk_id ==` admission exists in
  core/custodian src (grep-verified).
- **Attempted to break the k/m conjuncts (iteration-2's "dead-untested" finding); could
  not.** Case 3 (`fragment_identity.rs:734`) and the unit test's RS(3,1)-vs-RS(2,1) pair
  (`crates/core/src/repair.rs:212-221`) pin `ec_k`/`ec_m` specifically — the scheme-type
  check passes there, so only the k/m compare can reject, and the positive RS(3,1) control
  proves the compare (not the type) gates it.
- **Attempted a legacy/compat break via the stricter `EcScheme::None` arm
  (`repair.rs:69-71`); could not.** The production writer stamps exactly
  `None`/`ec_k=1`/`ec_m=0`/`index 0` (`crates/core/src/write.rs:133`,
  `FragmentHeader::new_v1`, `crates/chunk-format/src/header.rs:130-143`) and RS writers
  stamp the full tuple (`write.rs:116-123`, pre-existing) — no correctly-written on-disk
  fragment is newly rejected. The pre-M3 empty-placement test
  (`crates/custodian/tests/rebalance.rs:431`) passes.
- **Attempted to break the enqueue claim; could not.** `read_object` flushes `corrupt`
  to `repair::enqueue_repair` before surfacing the result (`read.rs:451-457`), and the
  streaming per-chunk path does the same (`read.rs:496-503`), so the below-k failure
  still leaves the durable obligation the tests assert.
- **Attempted to break the untested-under-the-gate dst suite.** The patch edits
  `crates/dst/tests/custodian.rs`, which the CI gate excludes (`--exclude wyrd-dst`) and
  a plain `cargo test` never compiles (`#![cfg(madsim)]`, `custodian.rs:51`). Ran it
  myself under `RUSTFLAGS="--cfg madsim"`: compiles, 10/10 pass — no hidden break.
- Full `cargo test --workspace --exclude wyrd-dst` passes here (exit 0) with a healthy
  TMPDIR — including the patched grpc tier1/tier2 suites and wyrd-core/wyrd-custodian.

## Findings

- NEEDS-HUMAN — **The gating C4-ci red is an environment fault, not this patch** (issue
  #236: provisional, not a refutation — but the deterministic gate still blocks and only
  a human can accept it). Reproduced the failure mode locally: the sole workspace failure
  is `crates/server/tests/cli_roundtrip.rs:43` panicking on `wyrd: Disk quota exceeded
  (os error 122)` (the host's quota-limited `/tmp`); with `TMPDIR` on a non-quota
  filesystem the **entire** `cargo test --workspace --exclude wyrd-dst` exits 0 on the
  patched tree. The gate log's `exit status: 101` names no test; the human should confirm
  the gate host's failure is the same class (quota / loopback-bind sandboxing, as
  iterations 1–2 recorded) and not a patch-caused failure — my evidence says it is not.
- NEEDS-HUMAN — **The C4-verify oracle is not inspectable at the target**: `engine/`
  (`./engine/scripts/run-verify.sh`, `./engine/xtask.sh`) is absent from
  `$PDCA_TARGET`, so the recorded "PASS — red without the fix, green with it" cannot be
  audited as-run. Mitigation: I reproduced the red→green independently (above), so the
  *claim* stands on my evidence even if the oracle's own run remains unauditable.

## Non-blocking observations (no action demanded)

- `custodian/src/gc.rs:938` (`ReferenceSet.schemes`) is last-write-wins per chunk id: if
  two committed inodes ever referenced the same chunk id under different schemes, scrub
  (`scrub.rs:105-126`) would verify with one of them and could false-flag the other's
  fragments. Failure direction is safe (false corruption → repair enqueue, never wrong
  bytes) and duplicate-id-different-scheme is already a metadata anomaly — noted only.
- A wrong-identity fragment that arrives *after* k good shards are accepted is never
  examined (any-k-arrive-first, `read.rs:338-342`), so no enqueue happens for it. That is
  the pre-existing fan-out design, not a regression; the brief's never-wrong-bytes
  criterion holds regardless.

**Verdict:** attempted to refute the red→green evidence, the admission-gate coverage, the
k/m conjuncts, backward compatibility of the stricter predicate, the enqueue claim, and
the gate-excluded dst suite; **could not**. The only red is the C4-ci gate, and my
reproduction attributes it to the gate host's environment, not the patch.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C2 Reproduction (red pre-fix) — Confirm the public-surface tests fail by assertion with production changes reverted — this artifact-only review could not mutate/stash the target, and the asserted `engine/scripts/run-verify.sh` red-leg wrapper is absent (`crates/core/tests/fragment_identity.rs:152`).
- [x] C4 Verification (red→green) — Decide whether focused green plus partial CI is sufficient without an independent red rerun or full host-capable CI — all 3 public tests and the helper test passed, while `cargo xtask ci` reached an unrelated loopback bind `PermissionDenied` after fmt/clippy/build passed (`crates/core/tests/fragment_identity.rs:152`).
- [x] T4 Contribution — Confirm no closed/rejected upstream PR already covers the affected paths — all-local-ref history was checked by path and shows only earlier chunk-only validation, but GitHub lookup was unavailable (`crates/core/src/read.rs:237`).
- [x] Validation — fitness-to-purpose — Decide whether public in-process adversarial-store coverage is representative enough for the production assurance boundary — it proves rejection and repair queuing but sign-off owns fitness for real backend corruption (`crates/core/tests/fragment_identity.rs:152`).
- [x] **The gating C4-ci red is an environment fault, not this patch** (issue
- [x] **The C4-verify oracle is not inspectable at the target**: `engine/`
- [x] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo test --workspace --exclude wyrd-dst` failed with exit status: 101

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
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
