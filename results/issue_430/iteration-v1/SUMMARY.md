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

Task under review: reject same-chunk fragments whose index or EC tuple disagrees with committed metadata, preventing wrong RS reconstruction input and enqueueing repair.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance boundary is falsifiable at the public read surface: wrong-index and wrong-scheme fragments must not yield wrong bytes and must create a repair obligation (`crates/core/tests/fragment_identity.rs:140`). |
| C2 Reproduction (red pre-fix) | NEEDS-HUMAN | Confirm the public-surface tests fail by assertion on the production-reverted base — the artifact-only reviewer could not mutate/stash the target, and the reported `engine/scripts/run-verify.sh` wrapper is absent, so the claimed red leg was not independently reproduced (`crates/core/tests/fragment_identity.rs:200`). |
| C3 Change | PASS | The shared admission predicate now binds checksum-valid bytes to chunk, requested index, and committed EC tuple, which is the identity boundary required to keep mislabeled payloads out of every decoder consumer (`crates/core/src/repair.rs:58`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Decide whether the unavailable red-leg rerun plus sandbox-blocked full CI is acceptable — both focused green tests passed, but native `cargo xtask ci` stopped at an unrelated loopback bind `PermissionDenied`, so complete red→green/CI verification remains provisional (`crates/core/tests/fragment_identity.rs:151`). |
| C5 Causal adequacy | PASS | The change removes chunk-id-only admission at the shared and inline decode boundaries rather than adding a capability probe or runtime fallback, so the malformed identity is rejected before shard insertion (`crates/core/src/read.rs:330`). |
| T1 Structure | PASS | The regression is isolated in the required new integration-test file and exercises `read_object` plus the durable repair queue rather than widened helper signatures (`crates/core/tests/fragment_identity.rs:151`). |
| T2 Shape | PASS | Each case exposes exactly two available RS(2,1) slots, forcing the wrong-identity slot to participate pre-fix and leaving fewer than k valid shards post-fix, avoiding arrival-order ambiguity (`crates/core/tests/fragment_identity.rs:162`). |
| T3 Runtime | PASS | The applied target ran both focused Tokio tests successfully (2 passed, 0 failed), directly observing rejection/enqueue behavior through the public read path (`crates/core/tests/fragment_identity.rs:215`). |
| T4 Contribution | NEEDS-HUMAN | Confirm no closed/rejected remote work already resolves these affected paths — merged/all-local-ref history was checked by file path and showed no full index-plus-scheme fix, but closed/rejected PR state was not mechanically available offline (`crates/core/src/read.rs:229`). |
| T5 Judgment | PASS | The touched production surfaces are the shared verifier, both read admission points, and existing maintenance consumers; the additional test-fixture header updates are necessary consequences of enforcing the already-recorded per-chunk scheme (`crates/custodian/src/scrub.rs:98`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether failure-with-repair below k and read-around at or above k meet the operational never-wrong-bytes objective for all deployed backends — automated tests establish the adversarial in-process cases, but production fitness remains a sign-off judgment (`crates/core/tests/fragment_identity.rs:201`). |

### Advisory — adversary

# check-advisory-adversary.md — issue 430 / fragment-identity-validation

Adversarial pass. I attempted to refute the evidence, the fix, and the verdict; I
**independently re-ran the red→green proof** in a scratch clone of the target base
(`dc503cd`) rather than trusting the C4-verify gate.

## Refutation attempts — and their outcomes

- **Attempted to refute the red→green evidence: could not.** In a scratch clone of base
  `dc503cd` with only `crates/core/tests/fragment_identity.rs` retained (all production
  and modified-test files reverted), both new tests fail **by assertion** — the read
  returns silently wrong bytes through the production `read::read_object` path
  (`crates/core/src/read.rs:435`), exactly the defect claimed. With the patch applied
  they pass. The red is honest: the test file compiles against the base production code
  (it never calls the widened helper signatures directly), so the red is behavioural,
  not a degenerate compile-error red. The deterministic-red shaping (serve only `k`
  fragments, one wrong-identity) holds — the decoder necessarily consumed the wrong
  shard pre-fix.

- **Attempted to break the fix with a legitimately-written fragment (mixed-era
  data-loss angle): could not.** If any historical writer had emitted RS fragments with
  unstamped EC header fields, `repair::header_matches_identity`
  (`crates/core/src/repair.rs:248-271`) would reject ALL existing RS data — a data-loss
  event. Checked: the very first RS write commit (`a0f9dad`) already stamped
  `ec_scheme_type`/`ec_k`/`ec_m`/`ec_fragment_index` (today
  `crates/core/src/write.rs:142-146`), and no production writer has ever emitted
  `EcSchemeType::Replication` (`crates/chunk-format/src/header.rs:79` — only codec unit
  tests use it). No legitimate on-disk fragment is rejected by the widened check.

- **Attempted to refute the untested half of the success criterion (read-around when
  ≥ k intact fragments remain): could not.** The new test file only exercises the
  below-`k` typed-error path. I wrote a scratch test (3 slots, slot 0 wrong-index,
  slots 1–2 genuine) against the patched code: the read returns the TRUE bytes via
  the `Ok(decoded) if header_matches_identity(...)` fan-out arm
  (`crates/core/src/read.rs:330-343`). Read-around works.

- **Attempted to break the custodian call-site threading: could not.**
  `reconstruction.rs` builds `frag` from the placement slot index
  (`crates/custodian/src/reconstruction.rs:359-363`) so `intact_shard(b, frag,
  chunk_ref.scheme)` (`reconstruction.rs:391`) verifies the identity the slot expects;
  the `EcScheme::None` early-return at `reconstruction.rs:346` keeps the scheme RS
  there. `rebalance.rs:266` indexes `plan.prior.chunk_map[plan.chunk_index]`, which is
  in-bounds by construction (`rebalance.rs:155,197`). Full `wyrd-core` + `wyrd-custodian`
  suites (23 targets) pass with the patch; `wyrd-chunkstore-grpc` test targets
  compile (`frag_id` is in scope in the tier tests as the patch assumes).

## Non-blocking observations (no `[impl]` rebuild warranted; recorded for the record)

- `crates/custodian/src/scrub.rs:100` — the new `if let Some(&scheme) =
  referenced.schemes.get(&frag.chunk)` **silently drops** a placed fragment from scrub
  coverage when its chunk has no scheme entry. Today this is unreachable — `placed` and
  `schemes` are populated in the same `Ok` arm of `referenced_fragments`
  (`crates/custodian/src/gc.rs:232-247`) — but the invariant is enforced only by
  co-location; a future refactor that decouples them would silently shrink scrub
  coverage rather than fail loudly. A `debug_assert!`/comment-free skip is a latent
  coupling, not a live defect.
- `crates/custodian/src/gc.rs:246` — `schemes.insert` is last-write-wins: two committed
  inodes sharing a chunk id under DIFFERENT schemes would make scrub verify one inode's
  fragments against the other's scheme. Chunk ids are minted uniquely per write, so this
  is a corrupted-metadata scenario where a (spurious) repair enqueue is a defensible
  outcome; not a failing case for this diff.
- Enqueue on the ≥-k read-around path is order-dependent (the fan-out may accept `k`
  good shards at `crates/core/src/read.rs:338` before ever examining the wrong-identity
  slot, so no repair obligation is recorded that pass). This matches the pre-existing
  misplaced-arm semantics the brief explicitly names as the model ("as the existing
  misplaced-fragment arm already does"); scrub independently catches it. Conformant.

## Verdict

Attempted to refute the red→green proof, the mixed-era/legitimate-data rejection angle,
the untested read-around branch, and the custodian identity threading; **could not**.
The evidence is genuine (behavioural red on the production path, independently
reproduced), the fix covers the named edge cases, and I found no concrete failing input.
No `NEEDS-HUMAN` findings.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C2 Reproduction (red pre-fix) — Confirm the public-surface tests fail by assertion on the production-reverted base — the artifact-only reviewer could not mutate/stash the target, and the reported `engine/scripts/run-verify.sh` wrapper is absent, so the claimed red leg was not independently reproduced (`crates/core/tests/fragment_identity.rs:200`).
- [ ] C4 Verification (red→green) — Decide whether the unavailable red-leg rerun plus sandbox-blocked full CI is acceptable — both focused green tests passed, but native `cargo xtask ci` stopped at an unrelated loopback bind `PermissionDenied`, so complete red→green/CI verification remains provisional (`crates/core/tests/fragment_identity.rs:151`).
- [ ] T4 Contribution — Confirm no closed/rejected remote work already resolves these affected paths — merged/all-local-ref history was checked by file path and showed no full index-plus-scheme fix, but closed/rejected PR state was not mechanically available offline (`crates/core/src/read.rs:229`).
- [ ] Validation — fitness-to-purpose — Decide whether failure-with-repair below k and read-around at or above k meet the operational never-wrong-bytes objective for all deployed backends — automated tests establish the adversarial in-process cases, but production fitness remains a sign-off judgment (`crates/core/tests/fragment_identity.rs:201`).

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
- Iteration delta (if iterating): Auto-iterate (round 1): Check found implementation-level items only, no architectural judgment required — C2 Reproduction (red pre-fix) — Confirm the public-surface tests fail by assertion on the production-reverted base — the artifact-only reviewer could not mutate/stash the target, and the reported `engine/scripts/run-verify.sh` wrapper is absent, so the claimed red leg was not independently reproduced (`crates/core/tests/fragment_identity.rs:200`).; C4 Verification (red→green) — Decide whether the unavailable red-leg rerun plus sandbox-blocked full CI is acceptable — both focused green tests passed, but native `cargo xtask ci` stopped at an unrelated loopback bind `PermissionDenied`, so complete red→green/CI verification remains provisional (`crates/core/tests/fragment_identity.rs:151`).; T4 Contribution — Confirm no closed/rejected remote work already resolves these affected paths — merged/all-local-ref history was checked by file path and showed no full index-plus-scheme fix, but closed/rejected PR state was not mechanically available offline (`crates/core/src/read.rs:229`).
- By / date: auto-iterate / 2026-07-15

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
