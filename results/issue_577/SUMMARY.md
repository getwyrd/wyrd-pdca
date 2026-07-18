# Result — issue 577 / typed-transient-terminal-errors

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: failure *class* — transient ("try again") vs terminal ("retry cannot help") —
- Success criterion: with the patch applied, (a) a transient fault and a terminal fault
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope: the additive typed transient/terminal classification at `crates/traits`, its

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
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

Review of issue #577: preserve typed transient, terminal, integrity, and indeterminate failure classes across the trait and chunkstore gRPC seams, including backend adaptations and the ratified sequencing record.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance boundary is decision-complete: consumers receive a bounded public class value, integrity remains terminal-but-distinct, unknown errors fail safe, and indeterminate commits remain outside the retry binary (`crates/traits/src/lib.rs:421`). |
| C2 Reproduction (red pre-fix) | PASS | Independently retaining `crates/chunkstore-grpc/tests/error_class.rs:179` while reversing the production patch failed to compile because `wyrd_traits::{classify, ErrorClass}` did not exist, establishing the claimed pre-fix absence. |
| C3 Change | PASS | The patch stays within the declared seam, gRPC translations, backend producers/tests, compatibility test adjustment, and sequencing record; the public classifier and stable labels are localized at `crates/traits/src/lib.rs:421`. |
| C4 Verification (red→green) | NEEDS-HUMAN | Decide whether to accept the otherwise reproduced red→green with an incomplete whole-tree rerun — the named patched test passed 5/5 at `crates/chunkstore-grpc/tests/error_class.rs:137`, but `cargo xtask ci` reached `cargo deny check` and then failed only because its advisory-database lock path was read-only. |
| C5 Causal adequacy | PASS | The change introduces typed causes and deterministic seam translations rather than a capability probe or fallback guard; fail-safe classification is defined at `crates/traits/src/lib.rs:535` and reconstructed at `crates/chunkstore-grpc/src/client.rs:49`. |
| T1 Structure | PASS | One seam-owned class contract feeds backend production and the two gRPC translation directions, avoiding duplicated consumer taxonomies (`crates/traits/src/lib.rs:509`, `crates/chunkstore-grpc/src/server.rs:28`). |
| T2 Shape | PASS | The closed four-value label space preserves the non-binary indeterminate outcome and makes integrity terminal without collapsing its identity (`crates/traits/src/lib.rs:437`). |
| T3 Runtime | NEEDS-HUMAN | Decide whether the feature-gated TiKV adaptation may sign off without a real feature-on build — the default loopback runtime passed unreachable, timeout, integrity, and detail-preservation tests (`crates/chunkstore-grpc/tests/error_class.rs:179`), and FDB feature-on checked, but TiKV feature-on stopped at missing OpenSSL development metadata before compiling the backend at `crates/metadata-tikv/src/lib.rs:235`. |
| T4 Contribution | NEEDS-HUMAN | Decide whether the prior-art search is sufficient — merged history was rechecked by affected paths and showed only narrower typed-fault precedents, but closed/rejected work could not be mechanically established from the supplied artifacts/target; the generalized contract lands at `crates/traits/src/lib.rs:421`. |
| T5 Judgment | NEEDS-HUMAN | Approve the project-defined human-only proposal edit — it records the already-ratified M4-before-enum sequencing and affects architectural history used for later graduation decisions (`docs/design/proposals/draft/0010-observability-floor-for-first-deployment.md:280`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether these four stable classes and existing gRPC status conventions are the right operator-facing vocabulary for issue #575 and future retry policy — that product/operational fitness judgment exceeds automated evidence (`crates/traits/src/lib.rs:458`, `crates/chunkstore-grpc/src/client.rs:21`). |

### Advisory — adversary

# check-advisory-adversary.md — issue 577 (typed-transient-terminal-errors)

Independent re-run performed in a scratch clone (base d1c3958 + patch.diff), then deleted.

## Refutation attempts that landed

- NEEDS-HUMAN — **tikv feature-on compile is unverifiable on this host and unevidenced in the gates.** The brief's Verification posture item (i) makes `cargo check -p wyrd-metadata-tikv --features tikv` mandatory ("declare, don't silently skip"). On this host it FAILS before reaching the crate: openssl-sys 0.9.117 cannot find libssl dev (exit 101) — an environment fault, not a fix defect (#236), so this is provisional, not a refutation. But `check-gates.json` carries no row for either feature-on compile, and I cannot see build-notes.md, so nothing visible to me proves Do ran it. The tikv change that only compiles feature-on — `under_deadline`'s call to `OperationTimedOut::new` inside the gated `mod store` (`crates/metadata-tikv/src/lib.rs:786`) — therefore remains compile-unverified here. I *did* independently verify the fdb half: `cargo check -p wyrd-metadata-fdb --features fdb` exits 0 on this host (libfdb_c present), and the new feature-gated seam tests compile and pass (2/2) — note `cargo check` alone does NOT compile `#[cfg(test)]` code, so a gates row citing only `cargo check` would under-claim there too.

- NEEDS-HUMAN [impl] — **the server's outbound transient mapping has zero test coverage; the Transient class is never actually proven to cross the wire as a status.** `crates/chunkstore-grpc/src/server.rs:26-32` (`transient_or_internal`, the `Code::Unavailable` arm): no default-compiled store raises a `TransientFault` (FsChunkStore cannot), and no test uses a double to drive one through the server. Both transient legs of `tests/error_class.rs` are generated on the client side of the wire — unreachable (no server answers at all) and timeout (tonic's own channel deadline, `CANCELLED`) — so only Integrity's `DATA_LOSS` round trip demonstrates server→client class survival. Concrete surviving mutation: replace `Status::new(Code::Unavailable, e.to_string())` with `Status::internal(e.to_string())` and the entire tree stays green while a store-side transient (e.g. a proxied/chained D server) silently classifies Terminal at the far client. A supplementary loopback test with a transient-raising store double closes it — the brief's "no doubles" constraint binds only the binding-criterion producers, not supplementary coverage of this new production arm.

- NEEDS-HUMAN [impl] — **new `RetryBudgetExhausted` doc misstates the safety-critical gate.** `crates/metadata-fdb/src/lib.rs:989-991` (patched) claims the class is sound because every failure "was retryable by FDB's own `is_retryable()` predicate ... the gate on the retry loop below". The actual gate is the strictly narrower `is_retryable_not_committed()` (`blind_commit_step`, lib.rs:1515), and the same file's module docs warn that widening to `is_retryable()` turns 1021 into a silent double-apply. The classification itself survives my attack *because* of the narrower gate (a 1021 can never become `RetryBudgetExhausted`→`Transient`, so Indeterminate is never laundered) — but the doc teaches the next maintainer the wrong predicate at exactly the spot where the distinction is load-bearing. One-line doc fix.

- NEEDS-HUMAN [impl] — **the timeout test's doc names the wrong status code, and two `class_of` transient arms are untested.** `crates/chunkstore-grpc/tests/error_class.rs:211-212` says the request "fails with a genuine tonic `DEADLINE_EXCEEDED`"; the actual failure (observed in my red-leg run) is `CANCELLED "Timeout expired"` — which `client.rs:163-166`'s own corrected note documents. The test passes for the right reason (CANCELLED is in the transient set), but consequently the `DeadlineExceeded` and `ResourceExhausted` arms of `class_of` (`client.rs:49-51`) are pinned by no test in the tree — remove either from the transient set and everything stays green (the dserver tests only chain-walk to the `TransportError`, they never assert the class).

- NEEDS-HUMAN — **`dial_error` classifies every dial failure Transient, including permanent config errors.** `crates/chunkstore-grpc/src/client.rs:96-101`: a DNS NXDOMAIN from a typo'd endpoint hostname (`GrpcChunkStore::connect("http://no-such-host.invalid:50051")`) is invalid config — terminal per 0010 and per the brief's own fail-safe rule ("retry logic must act only on *known-transient* signals") — yet it classifies Transient, licensing a retry loop against a name that will never resolve. The doc deliberately draws the terminal line only at malformed-URI (`Endpoint::try_from`); DNS failure sits between and went to the transient side. Defensible as "unreachable", but it is a classification-policy call the brief's fail-safe principle cuts against — human judgment, not a rebuild.

- Minor (no routing needed): `crates/metadata-redb/src/lib.rs:919-934` `this_backend_has_no_transient_class_to_produce` is vacuous — it asserts a *successful* `get` returns `None`, which cannot fail for any classification-related reason; the "negative half" it claims to pin is pinned only by the two terminal-classification tests above it. Supplementary, cosmetic.

## Attempted and could NOT refute

- **The red→green evidence is real and stronger than the gate needed.** Green: 5/5 in `error_class.rs` on the patched tree. I ran a *harder* red than run-verify.sh's (which reds on a compile failure): reverting only `chunkstore-grpc/src/{client,server}.rs` while keeping the seam types and the test compiling → 4/5 tests fail on genuine classification assertions (`Terminal != Transient`), 1 passes (the pre-existing DATA_LOSS/Integrity leg, correctly). The test exercises the production path — real `FsChunkStore`, real loopback tonic, no parallel re-implementation — and is not tautological.
- **Fail-safe default**: unknown errors classify Terminal, pinned (`traits/src/lib.rs` unit tests, incl. raw `io::Error`); no default-transient retry-storm path found.
- **Indeterminate never collapses**: `CommitUnknownResult` pinned as neither transient nor terminal at traits and at the fdb seam; the 1021-laundering attack via `RetryBudgetExhausted` fails because the loop gate is `is_retryable_not_committed()` (see doc nit above).
- **The #575 contract**: `ErrorClass` is a value with stable bounded labels (`ALL`, `as_str`, Display≡as_str, distinctness all pinned) — the brief's "not merely boolean predicates" requirement is met.
- **Downcast stability**: `OperationTimedOut` and `RetryBudgetExhausted` stay top-of-box (source-chain marker, not wrapper) — pinned by tests; the chain-walk helpers in `round_trip.rs`/`dserver.rs` were correctly widened. Traits/redb/tikv-deadline unit suites: all pass on the patched tree (17 + 3 + 9).
- **0010 edit**: the draft-proposal append is present as the brief *pre-declared* human-only item — already an expected sign-off row, not filed again here.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C4 Verification (red→green) — Decide whether to accept the otherwise reproduced red→green with an incomplete whole-tree rerun — the named patched test passed 5/5 at `crates/chunkstore-grpc/tests/error_class.rs:137`, but `cargo xtask ci` reached `cargo deny check` and then failed only because its advisory-database lock path was read-only.
- [x] T3 Runtime — Decide whether the feature-gated TiKV adaptation may sign off without a real feature-on build — the default loopback runtime passed unreachable, timeout, integrity, and detail-preservation tests (`crates/chunkstore-grpc/tests/error_class.rs:179`), and FDB feature-on checked, but TiKV feature-on stopped at missing OpenSSL development metadata before compiling the backend at `crates/metadata-tikv/src/lib.rs:235`.
- [x] T4 Contribution — Decide whether the prior-art search is sufficient — merged history was rechecked by affected paths and showed only narrower typed-fault precedents, but closed/rejected work could not be mechanically established from the supplied artifacts/target; the generalized contract lands at `crates/traits/src/lib.rs:421`.
- [x] T5 Judgment — Approve the project-defined human-only proposal edit — it records the already-ratified M4-before-enum sequencing and affects architectural history used for later graduation decisions (`docs/design/proposals/draft/0010-observability-floor-for-first-deployment.md:280`).
- [x] Validation — fitness-to-purpose — Decide whether these four stable classes and existing gRPC status conventions are the right operator-facing vocabulary for issue #575 and future retry policy — that product/operational fitness judgment exceeds automated evidence (`crates/traits/src/lib.rs:458`, `crates/chunkstore-grpc/src/client.rs:21`).
- [x] **tikv feature-on compile is unverifiable on this host and unevidenced in the gates.** The brief's Verification posture item (i) makes `cargo check -p wyrd-metadata-tikv --features tikv` mandatory ("declare, don't silently skip"). On this host it FAILS before reaching the crate: openssl-sys 0.9.117 cannot find libssl dev (exit 101) — an environment fault, not a fix defect (#236), so this is provisional, not a refutation. But `check-gates.json` carries no row for either feature-on compile, and I cannot see build-notes.md, so nothing visible to me proves Do ran it. The tikv change that only compiles feature-on — `under_deadline`'s call to `OperationTimedOut::new` inside the gated `mod store` (`crates/metadata-tikv/src/lib.rs:786`) — therefore remains compile-unverified here. I *did* independently verify the fdb half: `cargo check -p wyrd-metadata-fdb --features fdb` exits 0 on this host (libfdb_c present), and the new feature-gated seam tests compile and pass (2/2) — note `cargo check` alone does NOT compile `#[cfg(test)]` code, so a gates row citing only `cargo check` would under-claim there too.
- [x] **the server's outbound transient mapping has zero test coverage; the Transient class is never actually proven to cross the wire as a status.** `crates/chunkstore-grpc/src/server.rs:26-32` (`transient_or_internal`, the `Code::Unavailable` arm): no default-compiled store raises a `TransientFault` (FsChunkStore cannot), and no test uses a double to drive one through the server. Both transient legs of `tests/error_class.rs` are generated on the client side of the wire — unreachable (no server answers at all) and timeout (tonic's own channel deadline, `CANCELLED`) — so only Integrity's `DATA_LOSS` round trip demonstrates server→client class survival. Concrete surviving mutation: replace `Status::new(Code::Unavailable, e.to_string())` with `Status::internal(e.to_string())` and the entire tree stays green while a store-side transient (e.g. a proxied/chained D server) silently classifies Terminal at the far client. A supplementary loopback test with a transient-raising store double closes it — the brief's "no doubles" constraint binds only the binding-criterion producers, not supplementary coverage of this new production arm.
- [x] **new `RetryBudgetExhausted` doc misstates the safety-critical gate.** `crates/metadata-fdb/src/lib.rs:989-991` (patched) claims the class is sound because every failure "was retryable by FDB's own `is_retryable()` predicate ... the gate on the retry loop below". The actual gate is the strictly narrower `is_retryable_not_committed()` (`blind_commit_step`, lib.rs:1515), and the same file's module docs warn that widening to `is_retryable()` turns 1021 into a silent double-apply. The classification itself survives my attack *because* of the narrower gate (a 1021 can never become `RetryBudgetExhausted`→`Transient`, so Indeterminate is never laundered) — but the doc teaches the next maintainer the wrong predicate at exactly the spot where the distinction is load-bearing. One-line doc fix.
- [x] **the timeout test's doc names the wrong status code, and two `class_of` transient arms are untested.** `crates/chunkstore-grpc/tests/error_class.rs:211-212` says the request "fails with a genuine tonic `DEADLINE_EXCEEDED`"; the actual failure (observed in my red-leg run) is `CANCELLED "Timeout expired"` — which `client.rs:163-166`'s own corrected note documents. The test passes for the right reason (CANCELLED is in the transient set), but consequently the `DeadlineExceeded` and `ResourceExhausted` arms of `class_of` (`client.rs:49-51`) are pinned by no test in the tree — remove either from the transient set and everything stays green (the dserver tests only chain-walk to the `TransportError`, they never assert the class).
- [x] **`dial_error` classifies every dial failure Transient, including permanent config errors.** `crates/chunkstore-grpc/src/client.rs:96-101`: a DNS NXDOMAIN from a typo'd endpoint hostname (`GrpcChunkStore::connect("http://no-such-host.invalid:50051")`) is invalid config — terminal per 0010 and per the brief's own fail-safe rule ("retry logic must act only on *known-transient* signals") — yet it classifies Transient, licensing a retry loop against a name that will never resolve. The doc deliberately draws the terminal line only at malformed-URI (`Endpoint::try_from`); DNS failure sits between and went to the transient side. Defensible as "unreachable", but it is a classification-policy call the brief's fail-safe principle cuts against — human judgment, not a rebuild.
- [x] external dependency: openssl dev (pkg-config + libssl) — blocks `cargo check -p

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
- By / date: Eduard Ralph / 2026-07-17

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
- issue_577: server-side Transient arm untested — filed as wyrd#580 (supplementary double-driven loopback test)
- issue_577: RetryBudgetExhausted / timeout-test doc fixes + unpinned class_of arms — filed as wyrd#581 (milestone M4)
- issue_577: dial_error DNS-failure-as-Transient classification policy — filed as wyrd#582 (milestone Foundations), decide before #575 retry policy consumes the class
