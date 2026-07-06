# Result — issue 365 / coordination-etcd-l5-backend

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: 
- Success criterion: a **shared `Coordination` conformance / contract suite** — lifted
- Repo + branch target: getwyrd/wyrd @ `feat/m4-production-metadata-backend`
- Scope (one logical fix) / out of scope: 

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix (net-new crate + shared suite; a real, exercisable second
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass —                no pre-patch state to isolate a RED against; C4-ci gates the whole tree (#88).
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

### Advisory — adversary

# Adversarial review — issue 365 (coordination-etcd-l5-backend), iteration 4

Skeptic's pass. Ground truth read at `$PDCA_TARGET`
(`/home/eddie/wyrd/wyrd.pdca-wt-l0`). The store is now genuinely compiled and
driven under `--cfg madsim` in the `dst` tier (`run_dst` is inside `run_ci`,
`xtask/src/main.rs:971`), so iteration-2's "never compiled by any gate" blocker
is truly closed and the split-brain guard now has a gated, demonstrated-red
proof. The refutations below are what survives that.

## Findings

- **NEEDS-HUMAN — The real-etcd job never asserts the crate's headline
  single-leader property.** `crates/coordination-etcd/tests/conformance.rs:82-108`
  drives the shared suite + lock mutual-exclusion + cross-process discovery
  against real etcd, but it does **not** run
  `cross_instance_single_leader_is_exclusive` (the split-brain guard). That
  property — the exact class that got iterations 1–3 rejected — is exercised
  **only** on the madsim simulator (`crates/dst/tests/coordination.rs:120,218,275`).
  Binding criterion (b) names "single leader … on **real** etcd"; so even when a
  human *does* run `xtask etcd-conformance`, the single-custodian-leader property
  is never checked against a real cluster. Add it to the real-etcd conformance
  before claiming criterion (b).

- **NEEDS-HUMAN — No gate produces the real-etcd green; `overall:pass` does not
  evidence criterion (b).** The only gating row in `check-gates.json` is C4-ci,
  whose L5 coverage is `cargo xtask dst` — the **simulator**. The real-etcd job
  (`run_etcd_conformance`, `xtask/src/main.rs:282`) is deliberately **not** in
  `run_ci` (`:945-981`) and hard-fails without docker+protoc (`:293`, `:304`),
  neither of which the CI/PDCA environment provides. So criterion (b) "the shared
  suite is green on **real etcd** via a Tier-2 compose target" is earned by no
  gate that ran. The false-green iteration 3 flagged is genuinely fixed (missing
  tooling now returns `Err`, not `Ok`), but that only converts a false-green into
  a *not-run* — a human must actually execute `xtask etcd-conformance` against a
  real cluster before this backend enters the shipped graph.

- **NEEDS-HUMAN — All gated correctness rests on madsim-etcd-client fidelity (the
  open DST-fidelity decision, brief line 178).** Every gated L5 assertion is the
  simulator's model, not real etcd's: campaign-blocking (single-leader),
  lease tick-expiry (`coordination.rs:347`), mvcc `mod_revision` fencing/config
  (`store.rs:300,375,434`), and the lock recipe's reliance on
  `Compare::value(key, NotEqual, LOCK_HELD)` evaluating **true for an absent key**
  (`store.rs:357-367`) — a real-etcd behaviour that is exercised against
  `SimServer` only. If the simulator's election/compare/mvcc semantics diverge
  from real etcd on any of these, the gated green is fidelity-green, not
  correctness-green. The reviewer must not read the simulator suite as
  criterion-(b) satisfaction; this is the #264/#258 mirror the brief hands off.

- **The transient-proclaim-error anti-churn guarantee is verified by no test.**
  Iteration 3's item — "distinguish transient RPC error from actual lease loss;
  add a test that errors the proclaim path" — is only **half** covered. The
  *loss* half (keep-alive sets `is_lost` → re-campaign) is tested by
  `a_lapsed_leader_recampaigns_after_its_lease_is_lost` (`coordination.rs:424`).
  The *transient* half — a proclaim RPC error while the lease is **still live**
  must propagate `Err` yet **retain** the hold and its renewing lease
  (`store.rs:285-304`, the `?` at `:299`) — rests on code + comment only ("the
  simulator never errors today"). A regression that instead dropped the hold on a
  transient error would reintroduce the iteration-1/2 lease-leak / self-churn and
  go undetected by every gate.

## Attempted refutations that did NOT stick (honest signal)

- Tried to break `unlock`'s "conditional-by-construction" claim (`store.rs:386-398`)
  via lease-id reuse: A's expired lease L1 gets reused for B, A's late
  `unlock`→`revoke(L1)` kills B's lock. etcd's 64-bit lease ids make reuse
  astronomically unlikely and the code acknowledges it (`stop_without_revoke`
  doc, `:82`); could not turn it into a realistic failing case.
- Tried the "is_lost lag window" split-brain (lease expired server-side but the
  keep-alive hasn't observed it yet, so `elect_leader` takes the still-leading
  path): the proclaim errors → propagated as `Err`, the caller is **not** told it
  leads, and B's higher fencing token fences A — the single-active guard holds.
  Could not produce a caller-visible double-leader.
- Config-only-advancement (iteration 3 item) is now genuinely pinned by a shared
  clause and satisfied by both backends
  (`coordination-conformance/src/lib.rs:219-228` vs `store.rs:418-441` using
  `max(mod_revision)` over the config prefix, and mem's config-scoped counter
  `coordination-mem/tests/conformance.rs:156-187`). Could not refute.

### Advisory — codex

- `crates/coordination-etcd/src/keyspace.rs:24` — registration keys are not escaped or length-delimited, so discovery is not exact for keys ending in `/`: `registration_prefix(ns, "svc")` is `reg/svc/`, while `registration_member(ns, "svc/", id)` is `reg/svc//...`, which also matches that prefix. This violates the trait's per-key discovery isolation for the etcd backend; encode the logical key segment or use a delimiter scheme that cannot be present in the key.
- NEEDS-HUMAN — `crates/coordination-etcd/tests/conformance.rs:72` — the real-etcd cross-instance block checks lock exclusion and discovery, but never runs a second `elect_leader` while the first instance holds leadership. The madsim DST covers this, so the question is whether sign-off accepts madsim as the only proof for real etcd's single-leader election path, or wants the deliberate `xtask etcd-conformance` job to exercise it too.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T3 Runtime — Deterministic proof runs in-ci under madsim; but **criterion (b) real-etcd GREEN is earnable only off-ci** via `cargo xtask etcd-conformance` (needs docker + system `protoc`, xtask/src/main.rs:3596-3635). Decision owed: a human must actually run that job and see it green before this backend enters the shipped graph — the in-ci simulator is fidelity-bounded, not a substitute. (The iteration-3 false-green is fixed: missing tooling now hard-fails, xtask/src/main.rs:3607-3626.)
- [ ] T5 Judgment — Three standing decisions the code cannot settle: (1) **DST-fidelity** — is `madsim-etcd-client` a faithful enough stand-in for real etcd's min-create-revision election/lease semantics to carry the split-brain proof (the #264/#258 mirror)? (2) **etcd-client dependency review** — ADR-0003 three-test audit + `deny.toml` allowlist + TLS/auth posture (`connect(endpoints, None)`, cli.rs:3326, ships no TLS/auth). (3) **sequencing governance** — explicit M4 slice vs preceding coordination milestone (0015 :461-463).
- [ ] Validation — fitness-to-purpose — Owner must confirm the delivered crate actually fits 0015's purpose end-to-end: real-etcd suite observed green (see T3), the `server` etcd selection exercised against a live etcd, and prior-art on the affected paths (net-new crate; prior attempts preserved in `iteration-v1..v3/`; closed/rejected-work check by path is the maintainer's). Half (2) of the prerequisite (process roles) is out of scope per the 2026-07-04 decision (#364+#366).
- [ ] `crates/coordination-etcd/tests/conformance.rs:72` — the real-etcd cross-instance block checks lock exclusion and discovery, but never runs a second `elect_leader` while the first instance holds leadership. The madsim DST covers this, so the question is whether sign-off accepts madsim as the only proof for real etcd's single-leader election path, or wants the deliberate `xtask etcd-conformance` job to exercise it too.

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
- Iteration delta (if iterating): Binding criterion (b) is not satisfiable as the tests stand: the headline single-leader / split-brain property (`cross_instance_single_leader_is_exclusive`) runs ONLY on the madsim simulator and is NOT in the real-etcd conformance (`crates/coordination-etcd/tests/conformance.rs:82-108`; codex flags :72). The real-etcd job never runs a second `elect_leader` while the first instance holds leadership, so single-leader is never checked on a real cluster. Criterion (b) explicitly names "single leader ... on REAL etcd" — MUST be checked there, not only in the simulator. Add it to the real-etcd conformance so `cargo xtask etcd-conformance` earns (b). The correctness rebuild is otherwise materially complete vs iters 1-3 (store genuinely compiled + driven under madsim; split-brain guard gated demonstrated-red; lease-id-reuse, is-lost lag-window, and config-only-advancement refutations did not stick). C4-ci gate is green, but that green is the SIMULATOR's (madsim-etcd-client fidelity) — not real-etcd correctness. Also fix before re-accept: - keyspace prefix-collision bug (`crates/coordination-etcd/src/keyspace.rs:24`): registration keys are not escaped / length-delimited, so `reg/svc/` also matches a member keyed `svc/` (`reg/svc//...`) — violates per-key discovery isolation. Encode the logical segment or use a delimiter that cannot appear in the key. - transient-proclaim-error anti-churn path is verified by NO test: a proclaim RPC error while the lease is still live must return Err yet RETAIN the hold and its renewing lease (`store.rs:285-304`). Add a regression that errors the proclaim path so a dropped-hold regression cannot go undetected. Standing human calls to settle at next Check (not blocking the rebuild): DST fidelity acceptance (#264/#258 mirror), etcd-client dependency review (ADR-0003 audit + deny allowlist + the ships-no-TLS/auth `connect(endpoints, None)` posture), and sequencing governance.
- By / date: Eduard Ralph / 2026-07-04

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
