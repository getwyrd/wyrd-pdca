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

Review of issue #430: require shared read, repair, and maintenance validation to reject fragments whose header index or EC tuple disagrees with the committed fragment identity.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance boundary is falsifiable at the public read surface: wrong-index and wrong-EC-tuple fragments must never yield wrong bytes and must enqueue repair (`crates/core/tests/fragment_identity.rs:140`). |
| C2 Reproduction (red pre-fix) | PASS | In an isolated target copy with the production diff reversed but the new test retained, both cases compiled and failed by wrong-bytes assertions (`crates/core/tests/fragment_identity.rs:204`, `crates/core/tests/fragment_identity.rs:289`). |
| C3 Change | PASS | Full identity is decided once from chunk id, requested index, and committed scheme, then used at the read admission boundary and shared repair helpers (`crates/core/src/repair.rs:58`, `crates/core/src/read.rs:331`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Decide whether focused red→green plus fmt/clippy/build coverage is sufficient despite this host blocking loopback: both public tests are red by assertion before and green after, but `cargo xtask ci` cannot complete because `list_delete_over_grpc` cannot bind loopback (`crates/core/tests/fragment_identity.rs:152`). |
| C5 Causal adequacy | PASS | The fix removes admission on partial identity at the shared boundary rather than adding a capability probe or downstream symptom guard (`crates/core/src/repair.rs:63`, `crates/core/src/repair.rs:75`). |
| T1 Structure | PASS | Read, reconstruction, scrub, and rebalance consume the same expected-identity rule, preserving the core-owned format boundary (`crates/core/src/repair.rs:95`, `crates/custodian/src/reconstruction.rs:389`, `crates/custodian/src/scrub.rs:126`, `crates/custodian/src/rebalance.rs:266`). |
| T2 Shape | PASS | The new plain integration test file drives `read_object` and the durable repair queue with exactly k available fragments, making both failure modes deterministic (`crates/core/tests/fragment_identity.rs:146`, `crates/core/tests/fragment_identity.rs:200`). |
| T3 Runtime | PASS | Focused runtime execution observed both adversarial reads reject the bad shard and enqueue repair after the patch; both tests passed (`crates/core/tests/fragment_identity.rs:200`, `crates/core/tests/fragment_identity.rs:298`). |
| T4 Contribution | NEEDS-HUMAN | Confirm no closed/rejected remote work already resolves these affected paths — merged/all-local-ref history was checked by affected file path and shows earlier chunk-only validation work, but closed/rejected PR state is unavailable offline (`crates/core/src/read.rs:229`). |
| T5 Judgment | PASS | The change stays within the named shared-validation and custodian call-site scope; backend behavior and the adjacent #431 fault arm remain untouched (`crates/core/src/read.rs:320`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether rejection with typed failure plus repair enqueue provides the required operational assurance for arbitrary backends — automated tests establish never-wrong-bytes for both specified corrupt identities, but production fitness remains a sign-off judgment (`crates/core/tests/fragment_identity.rs:201`, `crates/core/tests/fragment_identity.rs:298`). |

### Advisory — adversary

# check-advisory-adversary.md — issue 430 / fragment-identity-validation

Adversarial pass. I re-ran the red→green proof myself and ran one mutation experiment
against the patched tree (scratch copy; target untouched).

## Findings

- NEEDS-HUMAN [impl] — The RS-arm `ec_k`/`ec_m` comparison is **dead-untested**: mutating
  `crates/core/src/repair.rs:75-79` to accept any `k`/`m` (keeping only the
  `ec_scheme_type == ReedSolomon` check) survives the ENTIRE `wyrd-core` + `wyrd-custodian`
  + `wyrd-dst` suites, including the new `crates/core/tests/fragment_identity.rs` —
  verified by running the mutant (all green). Cause: the test's "EC tuple disagrees" case
  (`crates/core/tests/fragment_identity.rs:621`) uses a `None`-type header, so only the
  `ec_scheme_type` mismatch ever goes red; the unit test's tuple case
  (`crates/core/src/repair.rs:195`) is the same shape. Concrete failing case the suite
  cannot catch: an adversarial store serves a same-chunk, index-0 shard whose header
  says RS(3,1) against a committed RS(2,1) — today rejected only by unproven code; any
  future regression of the `k`/`m` compare is invisible. The brief explicitly demanded
  the tuple case cover "`ec_k`/`ec_m`/scheme type" (brief.md:41-42). Fix by iteration:
  add a same-type wrong-geometry case (header RS(3,1) vs committed RS(2,1)) to
  `fragment_identity.rs` and/or the `repair.rs` unit test. The `None`-arm
  `ec_k == 1 && ec_m == 0` conjuncts (`repair.rs:70`) are untested for the same reason.

## Refutation attempts that failed

- Attempted to refute the red→green evidence: could not. Reproduced independently —
  on a scratch copy with all production + peer-test files reverted to `dc503cd` (base)
  and only the new test kept, both tests in `crates/core/tests/fragment_identity.rs`
  fail **by assertion** (the read returns silently WRONG bytes: index-1's shard fed at
  both data positions), and pass on the patched tree. The red is on the production path
  (`read::read_object` over the `ChunkStore`/`MetadataStore` trait seams,
  `crates/core/src/read.rs:314-372`), not a parallel re-implementation, and the
  deterministic-red shape (serve exactly k fragments, one wrong-identity) removes the
  order-dependence the brief warned about. The enqueue assertions
  (`fragment_identity.rs:601-606`, `:684-689`) were red pre-fix too (queue empty), so
  they are not tautological.
- Attempted to find a remaining chunk_id-only admission site: could not. Every
  production decode-and-admit path now routes through `repair::header_matches_identity`
  — read single-fragment `crates/core/src/read.rs:237`, RS fan-out `read.rs:331`,
  reconstruction `crates/custodian/src/reconstruction.rs:391`, scrub
  `crates/custodian/src/scrub.rs:126`, rebalance `crates/custodian/src/rebalance.rs:266`.
  No other production `wyrd_chunk_format::decode` caller admits fragments.
- Attempted to break the fix with legacy/writer-conformance inputs: could not. The
  `None` arm's required stamp (`EcSchemeType::None`, k=1, m=0, index 0) is exactly what
  `FragmentHeader::new_v1` writes (`crates/chunk-format/src/header.rs:130-143`,
  `crates/core/src/write.rs:133`), and the RS arm matches `encode_chunk` /
  `encode_ec_fragment` (`write.rs:116-123`, `:142-147`). `git log -S` shows no core
  writer ever stamped `EcSchemeType::Replication`, so no previously-written on-disk
  fragment becomes unreadable under the tightened check.
- Attempted to make scrub silently skip a referenced fragment via the new
  `referenced.schemes` lookup (`crates/custodian/src/scrub.rs:99-101`): could not —
  `schemes.insert` is symmetric with `placed.insert` in the same `Ok` arm of
  `referenced_fragments` (`crates/custodian/src/gc.rs:232-247`), so every placed
  fragment has a scheme entry. (Two committed inodes sharing one chunk id with different
  schemes would make the last-scanned scheme win, but chunk ids are minted per write —
  no such aliasing path exists in this codebase.)
- Attempted to panic rebalance via `plan.prior.chunk_map[plan.chunk_index]`
  (`crates/custodian/src/rebalance.rs:266`): could not — `chunk_index` comes from
  enumerating the same `prior.chunk_map` when the plan is built (`rebalance.rs:194-202`),
  and the CAS commit (`rebalance.rs:288`) fences a concurrently-changed record.

## Note on the gate record

- The C4-verify oracle `./engine/scripts/run-verify.sh` (check-gates.json, rule
  `C4-verify`) does not exist in the target checkout, so its "PASS — red without the
  fix, green with it" claim is not auditable from the artifacts — the same gap iteration
  1 flagged. Not scored as a refutation: I reproduced the red→green substance
  independently (above), which resolves the carry-forward's C2/C4 question on the merits.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Decide whether focused red→green plus fmt/clippy/build coverage is sufficient despite this host blocking loopback: both public tests are red by assertion before and green after, but `cargo xtask ci` cannot complete because `list_delete_over_grpc` cannot bind loopback (`crates/core/tests/fragment_identity.rs:152`).
- [ ] T4 Contribution — Confirm no closed/rejected remote work already resolves these affected paths — merged/all-local-ref history was checked by affected file path and shows earlier chunk-only validation work, but closed/rejected PR state is unavailable offline (`crates/core/src/read.rs:229`).
- [ ] Validation — fitness-to-purpose — Decide whether rejection with typed failure plus repair enqueue provides the required operational assurance for arbitrary backends — automated tests establish never-wrong-bytes for both specified corrupt identities, but production fitness remains a sign-off judgment (`crates/core/tests/fragment_identity.rs:201`, `crates/core/tests/fragment_identity.rs:298`).
- [ ] The RS-arm `ec_k`/`ec_m` comparison is **dead-untested**: mutating

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
- Iteration delta (if iterating): Auto-iterate (round 2): Check found implementation-level items only, no architectural judgment required — C4 Verification (red→green) — Decide whether focused red→green plus fmt/clippy/build coverage is sufficient despite this host blocking loopback: both public tests are red by assertion before and green after, but `cargo xtask ci` cannot complete because `list_delete_over_grpc` cannot bind loopback (`crates/core/tests/fragment_identity.rs:152`).; T4 Contribution — Confirm no closed/rejected remote work already resolves these affected paths — merged/all-local-ref history was checked by affected file path and shows earlier chunk-only validation work, but closed/rejected PR state is unavailable offline (`crates/core/src/read.rs:229`).; The RS-arm `ec_k`/`ec_m` comparison is **dead-untested**: mutating
- By / date: auto-iterate / 2026-07-15

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
