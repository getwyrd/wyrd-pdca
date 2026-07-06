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
