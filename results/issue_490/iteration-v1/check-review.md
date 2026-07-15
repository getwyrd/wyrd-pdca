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
