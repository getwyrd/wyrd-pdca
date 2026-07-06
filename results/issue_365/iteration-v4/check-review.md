# Check review — issue 365 / coordination-etcd-l5-backend

**Task under review:** the L5 `Coordination` trait (`crates/traits/src/lib.rs:434`) has exactly one
implementation (process-local `coordination-mem`), so a real multi-node cluster cannot discover peers,
elect a single custodian leader, or fence stale holders across machines. Build the ADR-0006 REQUIRED
**second, networked implementation** — a `coordination-etcd` crate over etcd — selectable by `server`
composition with no caller edits, and one **shared conformance suite both backends pass**. This is
iteration 4; iterations 1–3 were rejected for unproven distributed correctness (split-brain, no gated
real exercise of the store, vacuous single-leader/config clauses).

## Verdict table (5/5/1)

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Brief is a governed pointer to 0015's "Deployment prerequisite" (`brief.md:12-58`); binding facts (second etcd impl; one shared suite green on both; `traits/core/custodian`+callers untouched) are unambiguous and testable. |
| C2 Reproduction (red pre-fix) | PASS | Net-new capability, so "red" is codified not historical: `coordination-conformance/tests/demonstrated_red.rs` runs each shared clause against per-clause violating stubs (`#[should_panic]`), and `dst/tests/coordination.rs:3077-3099` runs the cross-instance clauses against two process-local `mem` instances and shows them go RED — pinning non-vacuity. `check-gates.json` C4-verify notes no pre-patch state to isolate against (#88). |
| C3 Change | PASS | Adds `coordination-etcd` (store.rs:2262-2431 implements all 10 trait methods over etcd), the shared `coordination-conformance` crate, mem-suite lift, `server` composition selection (cli.rs), `deploy/etcd-single-node`, and `xtask etcd-conformance` — the scoped four-part change. |
| C4 Verification (red→green) | PASS | Gate `C4-ci` (`cargo xtask ci`) = pass in check-gates.json; the store is compiled + driven under `--cfg madsim` by `cargo xtask dst` (in ci), which recompiles `wyrd-dst` aliasing `etcd-client`→`madsim-etcd-client` (xtask/src/main.rs:982-1004, dst/Cargo.toml:2735-2751). I re-derived the test structure statically; I could **not** independently re-execute cargo (harness withheld run approval), so this rests on the green gate, not a personal re-run — flagged for the human. |
| C5 Causal adequacy | PASS | Root cause = "only one, process-local `Coordination` impl"; the fix builds the real second impl rather than guarding a symptom. Symptom-guard smell-test does NOT fire: `is_lost()`/keep-alive (store.rs:2079,2262-2299) is a lease-liveness mechanism, not a capability probe (no `hasattr`/`try-import`/optional-capability fallback) papering over a load-time side effect. Simulator-vs-real-etcd fidelity of the split-brain proof is a judgment routed to T5/V, not a causal defect. |
| T1 Structure | PASS | New crate placed under `crates/`, workspace + `Cargo.toml` wired, ADR-0016 dependency discipline (depends on `traits` + own client + runtime, never `core`/a sibling concrete — Cargo.toml:1700-1724); etcd tree gated behind OFF-by-default `etcd` feature. |
| T2 Shape | PASS | Contract lifted into ONE shared `coordination-conformance` suite generic over `&impl Coordination` (lib.rs:1004-1206), driven by both backends via `run_all` — no etcd-only fork; `traits/core/custodian` absent from the diff (verified: no `crates/{traits,core,custodian}/` hunks), so the byte-for-byte invariant holds and no trait seam was silently edited. |
| T3 Runtime | NEEDS-HUMAN | Deterministic proof runs in-ci under madsim; but **criterion (b) real-etcd GREEN is earnable only off-ci** via `cargo xtask etcd-conformance` (needs docker + system `protoc`, xtask/src/main.rs:3596-3635). Decision owed: a human must actually run that job and see it green before this backend enters the shipped graph — the in-ci simulator is fidelity-bounded, not a substitute. (The iteration-3 false-green is fixed: missing tooling now hard-fails, xtask/src/main.rs:3607-3626.) |
| T4 Contribution | PASS | Directly closes every iteration-3 rejection: single-leader test asserts B stays PENDING while A leads (dst/coordination.rs:2990-3013), config-only advancement clause bites (conformance/lib.rs:1171-1180 + store config_revision via max mod_revision, store.rs:2407-2429), renew/revoke covered (lib.rs:1043-1067), lapse-recovery + orphan-safety + transient-vs-real loss distinguished (dst/coordination.rs:3134-3231). |
| T5 Judgment | NEEDS-HUMAN | Three standing decisions the code cannot settle: (1) **DST-fidelity** — is `madsim-etcd-client` a faithful enough stand-in for real etcd's min-create-revision election/lease semantics to carry the split-brain proof (the #264/#258 mirror)? (2) **etcd-client dependency review** — ADR-0003 three-test audit + `deny.toml` allowlist + TLS/auth posture (`connect(endpoints, None)`, cli.rs:3326, ships no TLS/auth). (3) **sequencing governance** — explicit M4 slice vs preceding coordination milestone (0015 :461-463). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Owner must confirm the delivered crate actually fits 0015's purpose end-to-end: real-etcd suite observed green (see T3), the `server` etcd selection exercised against a live etcd, and prior-art on the affected paths (net-new crate; prior attempts preserved in `iteration-v1..v3/`; closed/rejected-work check by path is the maintainer's). Half (2) of the prerequisite (process roles) is out of scope per the 2026-07-04 decision (#364+#366). |

## Notes for the human
- **I did not personally re-run the suite.** The harness declined to approve any `cargo`
  invocation (including sandbox-disabled), so C4/T3 rest on the green `C4-ci` gate plus a full
  static re-derivation of the madsim test design, not a fresh red→green I watched. Re-running
  `cargo xtask dst` and `cargo xtask etcd-conformance` yourself is the concrete step that clears
  the residual doubt.
- **Runnable real-etcd check (T3 / Validation):** from the target worktree,
  `cargo xtask etcd-conformance` (requires docker + `protoc`). Expect it to stand up
  `deploy/etcd-single-node`, run `cargo test -p wyrd-coordination-etcd --features etcd --test
  conformance` green, and tear the stack down; a missing-tooling run now errors loudly rather
  than false-greening.
- Correctness rebuild looks materially complete versus iterations 1–3; the remaining gates are
  judgment/observation the reviewer cannot discharge, not code defects.
