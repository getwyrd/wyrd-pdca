# Result — issue 458 / d-server-advertise-addr

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: `wyrd d-server` registers, for discovery, the endpoint derived from its
- Success criterion: A d-server bound to a wildcard/loopback address but given a
- Repo + branch target: getwyrd/wyrd @ feat/m4-production-metadata-backend   (M4 integration branch per INTEGRATION §2)
- Scope (one logical fix) / out of scope: Add `--advertise-addr ADDR` to `wyrd d-server` and thread it so the endpoint

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

Review task: issue 458 adds a d-server advertise address so discovery registration can publish a routable endpoint distinct from the bound socket address.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The owed behavior is explicit: advertised endpoint wins while unset preserves bound-address registration, with compose DNS names in scope and discovered consumers out of scope (`brief.md:19`, `brief.md:60`). |
| C2 Reproduction (red pre-fix) | NEEDS-HUMAN | Red was not independently rerunnable here: no C2 gate is configured and the available runtime denies listener bind before the regression assertion can execute, so a runtime-capable host must confirm the pre-fix failure (`check-gates.json:15`, `crates/server/tests/advertise_addr_registration.rs:30`). |
| C3 Change | PASS | The decision turns on whether the advertised value actually reaches the registration record; the patch threads CLI input through params into the DServer endpoint used by registration (`crates/server/src/cli.rs:473`, `crates/server/src/cli.rs:903`, `crates/server/src/cli.rs:930`, `crates/server/src/dserver.rs:250`). |
| C4 Verification (red→green) | NEEDS-HUMAN | The configured wrappers are absent and this sandbox blocks `TcpListener::bind`: `cargo fmt --check`, `cargo check -p wyrd-server`, and `cargo test -p wyrd-server --test advertise_addr_registration --no-run` pass, but the actual test fails with `PermissionDenied` before exercising registration, so red-green remains provisional (`check-gates.json:33`, `check-gates.json:42`, `crates/server/tests/advertise_addr_registration.rs:30`). |
| C5 Causal adequacy | PASS | No capability probe or runtime guard smell is present; the fix removes the deferred cause by making the advertised endpoint explicit before registration consumes it (`crates/server/src/dserver.rs:218`, `crates/server/src/dserver.rs:250`). |
| T1 Structure | PASS | The scope stays localized to CLI parsing/parameter threading, DServer endpoint construction, the focused regression, and compose commands; no unrelated role or consumer path is pulled in (`brief.md:55`, `crates/server/src/cli.rs:473`, `deploy/small-multi-node/docker-compose.yml:238`). |
| T2 Shape | PASS | The public shape matches the brief's ADDR-not-SocketAddr requirement: `--bind` remains a `SocketAddr`, while `--advertise-addr` is stored as a string and converted to the existing `http://` endpoint form (`brief.md:66`, `crates/server/src/cli.rs:463`, `crates/server/src/dserver.rs:219`). |
| T3 Runtime | NEEDS-HUMAN | A runtime-capable host must decide the actual register/discover behavior because both the new test and the existing peer bind test fail on this host with `Operation not permitted` before Coordination assertions run (`crates/server/tests/advertise_addr_registration.rs:30`, `crates/server/tests/failure_domain_registration.rs:38`). |
| T4 Contribution | PASS | Affected-path history check did not surface prior advertise support; the existing source marks split-horizon advertisement as the deferred gap now being closed (`brief.md:104`, `crates/server/src/dserver.rs:183`). |
| T5 Judgment | PASS | The advisory judgment is that the patch addresses the specified input-side registration gap without scope creep into discovery consumers, which the brief keeps for a later slice (`brief.md:88`, `brief.md:91`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Human sign-off must decide fitness in the intended deploy context: the compose live cross-container dial is explicitly off-Check, and this review could only compile the seam, not run the listener-backed green path here (`brief.md:84`, `deploy/small-multi-node/docker-compose.yml:224`). |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C2 Reproduction (red pre-fix) — Red was not independently rerunnable here: no C2 gate is configured and the available runtime denies listener bind before the regression assertion can execute, so a runtime-capable host must confirm the pre-fix failure (`check-gates.json:15`, `crates/server/tests/advertise_addr_registration.rs:30`).
- [x] C4 Verification (red→green) — The configured wrappers are absent and this sandbox blocks `TcpListener::bind`: `cargo fmt --check`, `cargo check -p wyrd-server`, and `cargo test -p wyrd-server --test advertise_addr_registration --no-run` pass, but the actual test fails with `PermissionDenied` before exercising registration, so red-green remains provisional (`check-gates.json:33`, `check-gates.json:42`, `crates/server/tests/advertise_addr_registration.rs:30`).
- [x] T3 Runtime — A runtime-capable host must decide the actual register/discover behavior because both the new test and the existing peer bind test fail on this host with `Operation not permitted` before Coordination assertions run (`crates/server/tests/advertise_addr_registration.rs:30`, `crates/server/tests/failure_domain_registration.rs:38`).
- [x] Validation — fitness-to-purpose — Human sign-off must decide fitness in the intended deploy context: the compose live cross-container dial is explicitly off-Check, and this review could only compile the seam, not run the listener-backed green path here (`brief.md:84`, `deploy/small-multi-node/docker-compose.yml:224`).

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
- By / date: Eduard Ralph / 2026-07-06

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
