# Check review — issue_257 / M4.6 real-commit-over-madsim-tikv (iteration 13)

**Task under review (evidence slice, not a code bugfix):** prove the redb→TiKV metadata-backend
swap upholds the ADR-0015 single-zone consistency contract behind the *unchanged* `MetadataStore`
trait, by (i) an at-Check layer of pure, non-tautological `testkit` quorum/oracle arithmetic +
`xtask` dispatch + a redb DST coverage seed, and (ii) an off-Check live ≥3-replica TiKV Tier-1/
Tier-2 scenario with a *symmetric leader* partition, ≥2 concurrent-writer CAS teeth, and a
four-leg mutation-acceptance run (Success criterion (d)). This is the 7th Do attempt on the
re-planned slice; Option-B (madsim-tikv-client does not exist → binding correctness moves
off-Check) is ratified and not re-openable.

**Grounding note (target-state caveat, NOT a patch defect):** `check-gates.json` records the
non-gating `C4-verify` row as *fail* because `patch.diff does not apply on
origin/feat/m4-production-metadata-backend — the bundle is stale; rebase Do.* `$PDCA_TARGET` did
not resolve to a readable issue_257 checkout in this sandbox, so **all citations below are grounded
on `patch.diff`**, and the stale base is treated as a rebase/ordering caveat per the reviewer
contract — it is **not** surfaced as a blocking C4 failure. The **gating** `C4-ci` row is *pass*.
No `crates/traits` or `crates/metadata-tikv/src` bytes are touched (invariant confirmed by diff).

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Patch implements the ratified Option-B posture + iteration-12 amendments exactly: pure oracles (`crates/testkit/src/lib.rs:1204-1483`), leader-isolation + concurrent-writer teeth (`crates/metadata-tikv/tests/tier1_metadata_consistency.rs:449-476`), toolchain-gated compile step (`xtask/src/main.rs:2206-2276`). Scope invariants hold — no `traits`/`metadata-tikv/src` edit; only a `wyrd-testkit` dev-dep added (`crates/metadata-tikv/Cargo.toml:203-212`). |
| C2 Reproduction (red pre-fix) | NEEDS-HUMAN | By ratified Option-B the binding behavioural red is the off-Check four-leg mutation-acceptance run (Success criterion (d): real-cut GREEN / no-op RED / deleted `get_for_update` re-check → contention leg RED / restored GREEN). The captured logs (`results/issue_257/evidence/`) are **not in the patch** and cannot be re-run here (no privileged Docker/TiKV cluster). Decision owed: confirm leg-3's captured log actually flips the contention leg RED when `metadata-tikv/src/lib.rs:555-573` is weakened — the whole correctness claim rests on this artifact existing and having teeth. |
| C3 Change | PASS | Coherent, cited, well-scoped: `testkit` seam + oracles, `xtask` metadata-tier dispatch/runners, live Tier-1/Tier-2 scenarios, `deploy/tikv-multi-replica` bridge stack + `iptables-agent`. Container/IP wiring is internally consistent (netns map `xtask/src/faults.rs:1891-1892` matches compose service/IP layout `deploy/tikv-multi-replica/docker-compose.yml:1803-1846`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Gating `C4-ci` = pass. Non-gating `C4-verify` = fail **only** because the bundle is stale on its base (patch does not apply) — a rebase/ordering caveat, NOT a patch defect; explicitly not a blocker. The at-Check layer has no behavioural red→green by design (Option B); the behavioural flip is the off-Check (d) run. Decision owed: (a) rebase Do onto current `feat/m4-production-metadata-backend` so the bundle applies; (b) verify the off-Check red→green logs (same artifact as C2). |
| C5 Causal adequacy | NEEDS-HUMAN | Contested symptom-vs-root-cause across 13 iterations; oracle is reviewer+human sign-off. The symptom-guard smell-test does **not** fire — no capability-probe/runtime guard is added to production code (`metadata-tikv/src` untouched); the `#[cfg(feature="tikv")]` / `WYRD_TIKV_PD_ENDPOINTS` gates are test-harness skips, not a papered-over load-time side effect. Decision owed: does the Option-B evidence architecture (pure oracles at Check + live teeth off-Check) genuinely close the determinism/evidence gap the slice exists for, given the at-Check layer alone carries no TiKV-correctness weight? |
| T1 Structure | PASS | Files sit where their precedents do: DST seed in `crates/dst/tests/`, live tiers in `crates/metadata-tikv/tests/`, seam in `crates/testkit/src/lib.rs`, dispatch in `xtask/src/metadata_faults.rs` (registered `xtask/src/lib.rs:2145`), orchestration stack under `deploy/` (outside the workspace, ADR-0010). |
| T2 Shape | PASS | Mirrors the sanctioned precedents (`jepsen_dispatch`, chunkstore tier tests). Pure-oracle unit tests use hand-computed expectations, not the literal the function returns (`crates/testkit/src/lib.rs:1500-1744`), and the `run_ci` guard test drives the real `run_ci_steps` wiring rather than restating a constant (`xtask/src/main.rs:2296-2334`) — the iter-1/iter-10 tautology shapes are avoided. |
| T3 Runtime | PASS | At-Check artifacts execute in `cargo xtask ci`: `testkit`/`xtask` unit tests via `cargo test --workspace`, the seed via `run_dst` under `--cfg madsim`. Live Tier-1/Tier-2 tests are `#[ignore]`d + endpoint-gated and skip cleanly with no cluster (`tier1_metadata_consistency.rs:336-346`). Note: the load-bearing live bodies only *run* off-Check; the toolchain-gated `cargo check --features tikv` type-checks them but does not execute them. |
| T4 Contribution | PASS | The DST seed is honestly relabelled pure redb coverage with **no** TiKV correctness weight (`crates/dst/tests/tikv_await_commit_interleaving.rs:19-31,90`), taking iter-8 exit (b) — it no longer claims teeth it cannot have. The pure oracles are wired into the live scenario (`tier1_metadata_consistency.rs:406-421,500-515`), not dead code — a field/threshold regression flips both the unit test and the live leg. |
| T5 Judgment | NEEDS-HUMAN | Oracle is reviewer+human sign-off. Judgment owed: accept that the binding correctness evidence for the entire slice lives off-Check (privileged Tier job), with the at-Check gate certifying only routing/arithmetic/coverage. Also owed per brief §NEEDS-HUMAN: the metadata-nemesis methodology ADR question (is symmetric-partition-by-analogy to ADR-0039 sufficient, or does the metadata swap need its own ADR refinement — architecture-board call, patch correctly mints no ADR) and the #365 static-endpoints reduced bar. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Whether this slice, as delivered, actually demonstrates the redb→TiKV swap upholds ADR-0015 on a real cluster is a human sign-off decision. It turns entirely on the off-Check captured evidence under `results/issue_257/evidence/` (real-cut GREEN, no-op negative-control RED, mutated re-check RED, restored GREEN) with `fault_materialized=true` from the PD-heartbeat oracle — none of which is re-runnable in this artifact-only review. Confirm those logs exist, were produced on a live ≥3-replica pingcap/tikv v8.5.1 cluster, and show the required flips before accepting. |

## Notes for the human (what I could and could not verify)

- **Could verify (from `patch.diff`):** the pure oracles are non-tautological and wired into the
  live scenario; the seed is honestly labelled coverage (exit b); the `run_ci` feature-gated step
  is gated on `WYRD_TIKV_TOOLCHAIN` and its guard test exercises real wiring (iter-9/10 fixes
  present); the netns/bridge topology and symmetric `-s`/`-d` × INPUT/OUTPUT rule set are
  genuinely bidirectional by construction (iter-13 fix); heal surfaces `iptables -D` failures and
  only sets `healed=true` when every rule came out (iter-8 codex fix); the runner waits for PD +
  every store port before asserting (iter-8 codex fix); no `traits`/`metadata-tikv/src` edit.
- **Could NOT verify (needs a privileged live run / rebased base):** the four-leg mutation
  acceptance logs (Success criterion (d)); that the PD `last_heartbeat`-freshness oracle actually
  observes isolation in-window on v8.5.1 (the iter-11 defect class); that the leader-isolation
  cut gives the contention leg real teeth on a live cluster; and that the stale bundle rebases
  clean onto the current integration base.
