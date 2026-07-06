# Check — adversarial (skeptic's) pass, #257 / m4.6-tier1-jepsen-tier2

Scope: this diff only. Grounded on the applied patch at `$PDCA_TARGET`
(`/home/eddie/wyrd/wyrd.pdca-wt-l0`). Advisory — no gate.

## Attacks on the evidence (red→green)

- **NEEDS-HUMAN — The gating C4-ci FAIL could not be reproduced, and I cannot explain it away.**
  `check-gates.json:37` records the *gating* check failing: `cargo test --workspace
  --exclude wyrd-dst` exited 101 (a panic). Yet re-running the exact command against the
  applied patch here is clean (exit 0, 86 `test result: ok`, zero failures). A green slice
  cannot rest on a whole-tree gate that flips between FAIL and PASS on identical inputs —
  either a flaky/nondeterministic test entered the tree with this diff, or the gate ran
  against a transient state. This must be adjudicated before accept: the deterministic gate
  says the slice is red. (One plausible latent trigger: if the privileged job's env leaks
  `WYRD_TIKV_PD_ENDPOINTS` into a plain `cargo test` run, `tier1_jepsen_metadata.rs:68`'s
  `nemesis_nodes().unwrap_or_else(|| panic!(…))` panics with exit 101 — but only if the
  `#[ignore]` is bypassed; worth confirming the gate's environment.)

- **The Check-observable red→green is a compile-existence proof, not a behavioral one.**
  Both flippables (`crates/testkit/tests/meta_fault_seam.rs`, `xtask/tests/meta_dispatch_
  orchestration.rs`) go RED under `run-verify` only because reverting the production hunk
  deletes a symbol (`SeededMetaFaults` / `xtask::meta_dispatch`) so `use …` fails to
  *compile*. That proves the module exists, not that its logic is right. It is redeemed
  only by the *content* oracles inside — see next two bullets for where those oracles still
  have gaps.

- **Attempted to refute the `quorum_safe_max` oracle — could not.**
  `crates/testkit/tests/meta_fault_seam.rs:706` asserts `survivors * 2 > n` for `n=1..=12`,
  and the `n/2`-regression genuinely flips it red at `n=4` (fault 2, leaves 2). That is a
  real, implementation-independent oracle over `SeededMetaFaults::quorum_safe_max`
  (`crates/testkit/src/lib.rs:609`). This artifact survives scrutiny.

## Attacks on the fix

- **NEEDS-HUMAN — The Jepsen nemesis faults the wrong tier: it pauses PD, but the metadata
  store is a *single* TiKV node.** `xtask/src/meta_dispatch.rs:95`
  (`META_JEPSEN_PD_NODES = 3`) and `:126` (`jepsen_nemesis_services` → `pd{i}`) draw a
  "quorum-safe minority" over the **PD** ensemble, and the runner
  (`xtask/src/faults.rs:716`) `docker compose pause`s only `pd*` services — never `tikv`.
  But `deploy/small-multi-node/docker-compose.yml:147` defines exactly **one** `tikv`
  service holding all metadata. Pausing a PD minority (majority PD survives → timestamp
  oracle + placement stay fully functional) creates **no data-plane partition** of the
  metadata store. Concrete consequence: `commit_point_linearizable_and_exactly_one_winner_
  under_partition` (`tier1_jepsen_metadata.rs`) runs 8 CAS racers against a healthy single
  TiKV — exactly-one-winner then holds *trivially* from local transactional CAS, proving
  nothing about consistency under partition. This is iteration-1's "passes without the
  required real fault" defect relocated, not fixed: a fault is injected, but against a tier
  whose disruption the consistency clauses do not depend on.

- **NEEDS-HUMAN — The nemesis and the load are not synchronized; the partition likely heals
  before the load runs.** `xtask/src/faults.rs:681` `run_meta_jepsen_with_nemesis` spawns a
  thread that pauses the PD minority, `sleep(Duration::from_secs(5))` (`:719`), then
  unpauses — while `run_meta_scenario_test` (`:725`) launches a **fresh `cargo test …
  --features tikv`** subprocess. That subprocess must compile the tier target (pulling
  `tikv-client`) and only then connect, seed, and drive the racers. The 5-second fault
  window is wall-clock from *thread spawn*, i.e. before cargo even starts; on any cold/warm
  compile it elapses before the CAS load executes, so the load hits a healed cluster. There
  is no barrier/handshake ensuring the load overlaps the pause. Even granting the wrong-tier
  point above, the fault window and the load do not deterministically coincide.

- **`MetaFault::Partition` is injected as a *process pause*, conflating two distinct faults.**
  The plan is drawn as `MetaFault::Partition` (`xtask/src/meta_dispatch.rs:127`) but applied
  via `docker compose pause` (`xtask/src/faults.rs:716`), which `SIGSTOP`s the container —
  the `MetaFault::Pause` mechanism per the seam's own doc (`crates/testkit/src/lib.rs:562`).
  A frozen-then-resumed PD ≠ a network partition (no split-brain, no asymmetric reachability).
  Mechanism identities are "ILLUSTRATIVE" per the brief, so this is advisory, not fatal — but
  combined with the two findings above, the "real partition + clock skew + process pause"
  binding condition (`ClockSkew` is defined at `lib.rs:566` but *never wired into any
  runner*) is not met by what the runner actually injects.

- **The routing test does not pin per-leg correctness — only "some real metadata target".**
  `xtask/tests/meta_dispatch_orchestration.rs:62` checks each leg's `--test` file *exists*
  and the package isn't the chunkstore crate, but never that Integration/Jepsen/Tier2 route
  to their *own distinct* scenarios. If `meta_dispatch(Jepsen).test` were mis-set to
  `"tier1_metadata_integration"` (a real file in the same crate), the test stays green — a
  leg-crossing bug passes. This is materially better than iteration-1's literal tautology
  (the filesystem is now an oracle), but the "routing resolves to the *right* target" claim
  is only half-proven.

## Attacks on the compounding-loop / seed

- **NEEDS-HUMAN — The mandatory "seeded regression promoted back into DST" is a Markdown doc,
  not a seed anything runs.** The binding Success criterion and the "compounding loop is
  mandatory, not optional" invariant require a *committed seeded regression*. The patch
  ships `crates/dst/tests/tikv_surfaced_seeds.md` — prose that `cargo` ignores by design
  (`:24`), status `known-gap` "NOT yet confirmed by a live Tier-1 run" (`:39`). `seed: 17`
  (`:38`) is asserted on by nothing (iteration-1's explicit remediation "PROMOTED_SEED=17
  must be asserted on" is now moot because there is no test at all). The file argues an
  in-slice DST regression is impossible without violating "DST keeps correctness authority."
  That argument may be sound — but whether a documented hypothesis satisfies the DoD bullet
  is exactly the pre-declared human call (brief Known-NEEDS-HUMAN #5). Flagging so it is not
  silently treated as "met."

## What I could not refute

- The trait invariant holds: `crates/traits/` is byte-untouched (git shows no change) — the
  M4 thesis "same system behind the unchanged trait" is not violated by this diff.
- The `quorum_safe_max` / `survivors*2>n` oracle and the seeded-selection reproducibility
  tests carry genuine, implementation-independent oracles and survive scrutiny.
- The off-Check gating posture (tier tests `#[ignore]` + clean skip when
  `WYRD_TIKV_PD_ENDPOINTS` unset) is legitimate and matches the existing tier precedents; my
  objection is not that the tier green is deferred, but that the *deferred* Jepsen leg, as
  wired, would not demonstrate the property it claims even when it does run (findings above).
