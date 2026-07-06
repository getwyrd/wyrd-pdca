# Check review — issue 364 / s3-http-wire-surface (iteration 7)

**Task under review:** give the in-process gateway a real client-facing **HTTP/S3 wire
surface** — bucket-scoped object PUT/GET/DELETE over S3-compatible HTTP with mandatory
SigV4 auth and streaming bodies — so the blueprint's day-one byte-identical S3 round-trip
can run over the network (brief.md:44-56). This iteration answers iter-6's reject:
(1) extract the wire layer to a dedicated crate, (2) close the streaming fail-open,
(3) resolve trailer-framing half-accept, (4) quarantine the lease-expiry wall-clock flake.

**Grounding:** target `$PDCA_TARGET` = `/home/eddie/wyrd/wyrd.pdca-wt-l1`, readable and
patch-applied (gateway-s3/gateway-core crates present; lease-expiry SKEW slack present).
The target is **not** stale. Cargo/git/test-binary execution is blocked by the sandbox, so
I could **not** re-run the gate or the test binaries myself; runtime rows lean on the
recorded gate plus static reading of the applied source.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Brief pins a demonstrable floor — signed bucket-scoped PUT→GET→DELETE byte-identical + unsigned/bad-sig refused, streaming, fail-closed (brief.md:48-56); the patch targets exactly this. Spec is unambiguous. |
| C2 Reproduction (red pre-fix) | PASS | Net-new module: red is the compile/absence red, made behavioral per brief.md:108. Non-gating C4-verify recorded PASS ("red without the fix, green with it", check-gates.json:46); I could not re-run it (sandbox). |
| C3 Change | PASS | Change delivers the endpoint: `gateway-s3` listener + SigV4 verify (`crates/gateway-s3/src/sigv4.rs:472-490`), aws-chunked streaming decode (`streaming.rs:114-252`), verbs mapped onto the client seam (`crates/server/src/lib.rs:131,199,279`). Grounds on target. |
| C4 Verification (red→green) | **FAIL** | **Gating** C4-ci `cargo test --workspace --exclude wyrd-dst` recorded **exit 101** (check-gates.json:33-39) — a runtime test failure on an applied, compiling patch (test binaries built in target/), not a stale-apply artifact. Decision owed: this is the blocker; it must be re-run to an authoritative GREEN before accept. Sandbox prevented me from re-running/localizing. |
| C5 Causal adequacy | PASS | Root cause (no wire endpoint) is removed, not guarded. Prior leak causes stay closed: overwrite orphans the superseded map atomically (`crates/core/src/write.rs:271-287` → `crates/core/src/metadata.rs:459-485`); DELETE honours the grace window, no eager reclaim (`crates/server/src/lib.rs:290-294`). Symptom-guard smell-test: no capability-probe / load-time runtime guard introduced. Orphan-ledger design was ratified in iter-6 — not re-litigated. |
| T1 Structure | PASS | iter-6 T5(a) reject answered: wire layer extracted to `crates/gateway-s3` over a shared `crates/gateway-core` seam (new crates present in target), composition-root wiring preserved — it no longer calcifies inside `crates/server`. |
| T2 Shape | PASS | Fail-closed shape: closed-set sentinel classification refuses unknown STREAMING-* cleanly (`sigv4.rs:472-490,506-514`), declared chunk size bounded before buffering (`streaming.rs:57,215-219`), XML error `<Message>` escaped (`gateway-s3/src/lib.rs:324-345`). All named iter-6 fail-open/half-accept holes addressed. |
| T3 Runtime | NEEDS-HUMAN | Decision owed: is the recorded exit-101 the historical `gateway_lease_expiry.rs` wall-clock flake (this iter slackens the lower bound 20s, `crates/server/tests/gateway_lease_expiry.rs:148-152`) or a real regression in the new real-SDK/concurrency tests (`s3_http_wire.rs:331,382,792`)? Impact: it decides whether C4 is a durable green or a live defect. Sandbox blocked my re-run; human must localize on a clean host. |
| T4 Contribution | PASS | Contributes the load-bearing wire seam gating #367; core/custodian edits (overwrite-reclaim, GC backstop) are the in-scope leak fixes prior iterations demanded, not scope creep. Real `aws-sdk-s3` dev-dep is the independent oracle (`crates/server/tests/s3_http_wire.rs:792`, Cargo.toml:82), retiring the self-signing objection. |
| T5 Judgment | NEEDS-HUMAN | Ratified in iter-6 (crate boundary, SigV4 header-only scope, M4 sequencing, TLS deferral, aws-sdk-s3 deny-clean) — do not re-litigate. Residual open call: the **minimal S3 error-code floor** the gateway must return correctly (auth-failure / not-found subset — brief Known-NEEDS-HUMAN item 4, brief.md:146-148) is not yet ratified. Human must fix the floor to avoid the S3-conformance rabbit hole. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decision owed: does a **plaintext-loopback** signed PUT→GET→DELETE round-trip with backend-run (not at-Check) fragment reclamation constitute an acceptable first-deployment-gate floor, with live public-TLS + running custodian topology deferred to #367 (brief.md:57-62,150-160)? This is the human's fitness call — and it cannot be signed while the gating C4-ci above is red. |
