# Check review — issue 268 / grpc-block-read-fault-wire-signal

Advisory, artifact-only. Grounded on target `$PDCA_TARGET =
/home/eddie/wyrd/wyrd.pdca-wt` (readable, base at `6152a29`; the diff's
pre-image context matches the target byte-for-byte at every cited line —
server.rs:83-89, client.rs:25-34, reconstruction.rs:254/259/306-334,
traits/lib.rs:107-116 — so the patch applies cleanly and the target is **not**
stale). Re-derived independently of build-notes (withheld).

## Verdict table (5/5/1)

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Defect + binding success criterion are crisp and falsifiable: remote `EIO` must classify the same as local (`is_permanent_read_fault` true) while `UNAVAILABLE`/`DEADLINE_EXCEEDED` stay `TransportError`; mechanism declared illustrative, classification-across-seam binding (brief §Success criterion / §Invariant). |
| C2 Reproduction (red pre-fix) | PASS | Repro is real and encoded: pre-fix `server.rs:83` maps a non-integrity error to `Status::internal`, client `classify_get_status` (client.rs:25-33) → `TransportError`, so `is_permanent_read_fault` is false. `check-gates.json` C4-verify re-ran red→green (`run-verify.sh: PASS — red without the fix`). |
| C3 Change | PASS | Minimal and coherent: server OR-adds `is_block_read_fault` (server.rs:83-89), predicates hoisted into `wyrd_traits` (`is_block_read_fault`/`is_permanent_read_fault`, traits/lib.rs +147-194) and the custodian copies removed (reconstruction.rs -306-334) with the call re-pointed at the trait (reconstruction.rs:259) — no dangling caller (only reconstruction.rs referenced them). NIT: `async-trait` added to `[dev-dependencies]` though already a normal dep of the crate (Cargo.toml:18) — integration tests already inherit normal deps, so the dev-dep line is redundant (harmless; xtask ci green). |
| C4 Verification (red→green) | PASS | Gating `C4-ci` (`xtask ci`: fmt/clippy/build/test/deny/conformance) = pass and `C4-verify` red→green = pass in check-gates.json; target confirmed current so the green is against the patch's true base, not a stale checkout. I could not re-execute the harness scripts (not present under the worktree) — verdict rests on the recorded gate + the clean ground of every cited line on target. |
| C5 Causal adequacy | NEEDS-HUMAN | Fix removes the cause (server no longer flattens `EIO`→`internal`; no capability-probe/guard added, smell-test does not fire), so the reconstruction loop now reads around remote dead sectors — primary defect resolved. DECISION OWED: the chosen wire contract reuses `Code::DataLoss`, which the client rebuilds as `IntegrityFault` (client.rs:25-33). Downstream, `scrub.rs:102` branches on `is_integrity_fault` and then **`emit_corruption` (scrub.rs:104)** + enqueues a "scrub" repair — so a remote dead sector is now recorded as a *corruption* finding, and remote scrub diverges from the local/fs path (where a raw `io::Error(EIO)` is NOT an IntegrityFault and instead propagates at scrub.rs:108). The brief sanctions the DataLoss=IntegrityFault carrier, but the human must confirm mislabeling a block-read fault as corruption in scrub telemetry — and the remote-vs-local scrub divergence — is acceptable and not a new defect. |
| T1 Structure | PASS | Test lives at the brief-specified path `crates/chunkstore-grpc/tests/read_fault_seam.rs`; fault-injecting fakes implement the real `ChunkStore` trait and the service is mounted over a real tonic loopback (patch.diff:300-330), matching the "wire seam, not a trait mock" requirement. |
| T2 Shape | PASS | Two complementary cases: `EioStore` (errno-5 → must be permanent + not `TransportError`) and `GenericErrorStore` (`io::Error::other` → must stay transient `TransportError`). The negative case pins the no-spurious-re-placement half of the invariant. |
| T3 Runtime | PASS | check-gates C4-verify confirms the new test is red without the fix and green with it; assertions key on `wyrd_traits::is_permanent_read_fault` and `downcast_ref::<TransportError>()`, which are the consumer's real decision points. |
| T4 Contribution | PASS | The test fails on exactly the shipped bug (pre-fix `Status::internal`→`TransportError`→`is_permanent_read_fault` false), and exercises the full wire round-trip rather than the predicate in isolation — it would catch a regression of the server mapping. |
| T5 Judgment | NEEDS-HUMAN | The suite asserts permanence via the `IntegrityFault` carrier and does not (cannot, by this design) pin that a dead sector stays distinguishable from corruption past the seam. DECISION OWED: human confirms the two-case shape fully encodes the binding condition and that no added coverage is owed for the corruption-vs-dead-sector telemetry collapse noted in C5. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | DECISION OWED (human, always): does surfacing remote block-read faults as `IntegrityFault` at the client seam meet the production intent — the read-around reaches networked D servers (the goal) — while accepting that scrub now counts dead sectors as corruption (C5)? This is the closure-of-"permanent block-layer fault" decision the brief asks to be made once on the wire contract; confirm it is the intended one. |

## Prior-art

Brief's prior-art check grounds: `is_permanent_read_fault`/`is_block_read_fault`
landed for the local path in #251 and `chunkstore-grpc` was the named deferred
half; on target these predicates exist only in `reconstruction.rs` (now hoisted)
— consistent with "unstarted remote half." The "no open PR touches these files"
claim is not mechanically re-checkable from this sandbox (no repo network); it is
asserted by the human-authored brief and not contradicted by the target tree.

## Caveats
- Advisory only — no gate is changed here; all blocking gates (C4-ci) already pass.
- C4 rests on recorded gate evidence (harness scripts absent from the worktree),
  cross-checked against clean grounding of every cited line on the live target.
