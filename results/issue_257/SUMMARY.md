# Result — issue 257 / m4.6-real-commit-over-madsim-tikv

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: 
- Success criterion: three layers — one BINDING and demonstrable **at Check**, one BINDING but
- Repo + branch target: getwyrd/wyrd @ `feat/m4-production-metadata-backend`
- Scope (one logical fix) / out of scope: (i) add the **`madsim-tikv-client` cfg-alias** to `crates/metadata-tikv/Cargo.toml`

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix (implement — accepted-plan test-evidence slice behind Accepted
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: fail — run-verify.sh: patch.diff does not apply on origin/feat/m4-production-metadata-backend — the bundle is stale; rebase Do.
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

### Advisory — adversary

# check-advisory-adversary.md — issue 257 (m4.6, iteration 13) — adversarial pass

Skeptic's pass over `patch.diff` + `brief.md` + `check-gates.json` only, grounded on
`$PDCA_TARGET` (= `/home/eddie/wyrd/wyrd.pdca-wt`, base 5d87cc4). Ratified items
(Option-B posture, exit-(b) seed labelling, the `last_heartbeat` oracle choice, the
toolchain-gated compile step) were **not** re-litigated; they were attacked only where the
iteration-13 code changes their standing.

## Refutations / findings

- NEEDS-HUMAN — **The entire recorded green sits on a stale base; the red→green was never
  demonstrated against the actual target.** Verified: `git apply --check patch.diff` on
  `origin/feat/m4-production-metadata-backend` @ 9cf4e8e **fails** (`error: patch failed:
  xtask/src/main.rs:82`) — the integration branch gained #448/#449/#450 (+194 lines in
  `xtask/src/main.rs`, +1213 in `Cargo.lock`) after the bundle's base 5d87cc4.
  `check-gates.json` records this honestly (`C4-verify: fail — "bundle is stale; rebase
  Do"`) yet carries `"overall": "pass"`. Every green in this bundle — the gating C4-ci
  pass, the pure-oracle tests, and (presumably) the live-leg evidence — was produced three
  merges behind the branch this patch must land on. The C4-ci `pass` row therefore
  certifies a tree that no longer exists; treating `overall: pass` as a merge verdict is
  the unwarranted claim. A rebased Do + re-run gates are required before any adequacy
  judgment is meaningful.

- NEEDS-HUMAN — **The binding behavioural evidence (Success criterion (d), the four-leg
  mutation acceptance run) is invisible to this review and absent from the target.**
  `results/issue_257/evidence/` does not exist at `$PDCA_TARGET` and `patch.diff` carries
  no evidence artifact; the brief's iteration-12 amendment makes those captured logs THE
  behavioural red→green Option B defers to ("RUN and CAPTURED, not argued";
  brief.md:124-133, 246-255). A human must verify, from the bundle's evidence dir: (1) the
  real-cut leg GREEN with `fault_materialized = true`; (2) the no-op negative control RED
  with `fault_materialized = false`; (3) the scratch mutation deleting the `get_for_update`
  re-check (`crates/metadata-tikv/src/lib.rs:555-573`) flipping the contention leg RED via
  an **observed lost update** (`committed_contenders == 2` and/or the stale probe
  admitted — `crates/metadata-tikv/tests/tier1_metadata_consistency.rs:235-276`), not via a
  connection error or panic; (4) restored GREEN. Without leg (3)'s log the slice has, once
  again, zero executed behavioural red→green anywhere — the exact iteration-12 rejection.

- **Leader-identity is never verified at cut time — the outcome-neutral follower cut the
  brief forbids can pass GREEN as "leader isolation".**
  `resolve_leader_ip` (`tier1_metadata_consistency.rs:779`) resolves the leader of the
  **first region PD lists, cluster-wide** (`wyrd_testkit::parse_first_region_leader_store_id`,
  `crates/testkit/src/lib.rs:532`), once, **before** the cut (`:455-471`), and nothing ever
  re-checks that the cut store (a) still leads anything when the rules land, or (b) leads
  the region holding the test keys (`wyrd-tier1-consistency/<pid>/dir/version`). Concrete
  failing case: PD's balancer moves/splits leadership between resolve and apply (or the
  first listed region is a system region led by a different store than the test-key
  region) → a **follower** is cut → its heartbeat still goes stale →
  `fault_materialized = true` → all four signals green — the precise "minority-voter cut
  provably never changes the outcome" hollow flip brief.md:112-118 forbids, now passing
  silently under a `WYRD_TIER1_ISOLATE=leader` label. Parser edge worsens it: a leaderless
  first region emitting `"leader":{}` makes `parse_first_region_leader_store_id` read the
  next `"store_id"` in the byte stream — an arbitrary peer of a *different* region (the
  unit test at `crates/testkit/src/lib.rs:915-925` covers only a fully absent `leader`
  key). Fix shape: assert post-cut (from PD) that the isolated store WAS the test-key
  region's leader at cut time, or fail the leg as not-materialized.

- **No asserted commit is ever in flight during the leader election — the docstring's
  teeth claim is overstated.** The scenario blocks in `pd_still_sees_target_live_after`
  (`tier1_metadata_consistency.rs:195`, up to 45s; staleness threshold 20s, `:506`) until
  the heartbeat is provably stale, and only then issues the rename (`:210`) and releases
  the contenders (`:235`). TiKV re-elects in ~10s, so by construction every asserted
  commit runs against a settled 2/3-quorum cluster; the claim "forcing a leader election
  while the contenders' commits are in flight" (`:32-33`) is unwarranted. The mutation
  flip (leg 3) fires identically with **no partition at all** — the partition contributes
  only the materialization signal, not perturbation of the asserted commit path. Not a
  false-green by itself, but the leg proves less than its docstring and the brief's
  "contending … across the fault window" framing suggest.

- **The fault-effect oracle can classify a NO-OP cut as materialized on a single failed
  poll.** `pd_sees_target_live` maps *PD unreachable / HTTP timeout / parse failure* to
  `false` (`tier1_metadata_consistency.rs:647-656`, `None => false` at `:654`), and
  `pd_still_sees_target_live_after` (`:661-670`) returns "isolated" on the **first** poll
  that comes back `false`. Concrete failing case: during a no-op run (rules never matched),
  one 3s connect timeout or a momentary PD hiccup inside the 45s window yields
  `connected_during = false` → `partition_took_effect(true, false) = true` →
  `fault_materialized = true` for a fault that never existed — the oracle whose whole
  Invariant-B job is "red when the fault is a no-op" goes green on infrastructure noise.
  Requiring N consecutive successfully-parsed stale reads (distinguishing "PD said stale"
  from "no answer") closes it. This also taints the no-op negative-control evidence (leg 2)
  as a single-run artifact.

- **The iter-10 guard gap moved up one level rather than closing.**
  `ci_type_checks_feature_gated_metadata_scenario` (`xtask/src/main.rs:1128`) drives
  `run_ci_steps` directly; nothing asserts that `run_ci` (`xtask/src/main.rs:897-898`)
  still calls `run_ci_steps`. Deleting/inlining that one call re-creates the iter-9 hole
  (feature-gated bodies never compiled by the real gate) with the guard test still green —
  the comment "the sole cargo-step source run_ci iterates" (`:1121`) is asserted nowhere.
  Materially better than the iter-10 constant-restating tautology (the loop and the
  toolchain gate now live inside the tested function), but the specific sign-off wording
  "assert run_ci invokes the feature-gated check" is satisfied only one hop removed.

## Attempted to refute; could not

- **Contention teeth (in-code):** traced `TikvMetadataStore::commit`
  (`crates/metadata-tikv/src/lib.rs:540-601`): with the `get_for_update` precondition loop
  deleted, both pessimistic contenders eagerly lock, wait, and blind-write → **two**
  `Committed` → `no_lost_update(2, _) = false`; the stale probe (`require` on a
  two-bumps-stale version, own key only) commits → red independently. The mutation flip is
  real in principle — its *executed* proof is the NEEDS-HUMAN evidence item above.
- **Pure oracles:** ran `cargo test -p wyrd-testkit --lib` and `cargo test -p xtask` on the
  patched target — green; the quorum/heartbeat/heal/no-lost-update tables use hand-computed
  expectations, and negating any single `ConsistencySignals` field fails
  `consistency_passes` (`crates/testkit/src/lib.rs` tests). No tautology found.
- **Compile-flip / dead-code:** ran `cargo check -p wyrd-metadata-tikv --features tikv
  --tests` on the patched target — green in 8.4s (tikv-client 0.4.0 builds), so the
  feature-gated scenario bodies type-check and the iter-11 "confirm the toolchain-gated
  check actually compiles them" item is satisfiable; `partition_took_effect` /
  `heal_is_complete` / `heartbeat_is_fresh` are consumed by the live scenario, not dead.
- **Invariants:** `crates/metadata-tikv/src` and `crates/traits` untouched by the diff;
  `wyrd-testkit` lands under `[dev-dependencies]` (`crates/metadata-tikv/Cargo.toml:46`);
  the DST seed's docstring claims no correctness weight (exit (b), ratified) and I found no
  smuggled teeth claim in it.

### Advisory — codex

- `crates/metadata-tikv/tests/tier1_metadata_consistency.rs:195` waits for PD to observe the isolated leader's heartbeat as stale before the rename and contended CAS are even started at lines 210 and 241. That means the "teeth" path is mostly a post-election majority-side workload, not commits already in flight across the leader-isolation window the comments and brief rely on; a commit-ordering regression that only appears during the disrupted leader handoff can still be missed.
- NEEDS-HUMAN — `crates/dst/tests/tikv_await_commit_interleaving.rs:7` explicitly says the Check-time DST seed is redb-only and cannot fail on a `TikvMetadataStore::commit` regression. That may be the intended Option-B fallback, but it does not satisfy the originally requested at-Check real-TiKV/madsim commit-path oracle; human sign-off should decide whether the declared fallback plus off-Check Tier-1 evidence is acceptable.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C2 Reproduction (red pre-fix) — By ratified Option-B the binding behavioural red is the off-Check four-leg mutation-acceptance run (Success criterion (d): real-cut GREEN / no-op RED / deleted `get_for_update` re-check → contention leg RED / restored GREEN). The captured logs (`results/issue_257/evidence/`) are **not in the patch** and cannot be re-run here (no privileged Docker/TiKV cluster). Decision owed: confirm leg-3's captured log actually flips the contention leg RED when `metadata-tikv/src/lib.rs:555-573` is weakened — the whole correctness claim rests on this artifact existing and having teeth.
- [x] C4 Verification (red→green) — Gating `C4-ci` = pass. Non-gating `C4-verify` = fail **only** because the bundle is stale on its base (patch does not apply) — a rebase/ordering caveat, NOT a patch defect; explicitly not a blocker. The at-Check layer has no behavioural red→green by design (Option B); the behavioural flip is the off-Check (d) run. Decision owed: (a) rebase Do onto current `feat/m4-production-metadata-backend` so the bundle applies; (b) verify the off-Check red→green logs (same artifact as C2).
- [x] C5 Causal adequacy — Contested symptom-vs-root-cause across 13 iterations; oracle is reviewer+human sign-off. The symptom-guard smell-test does **not** fire — no capability-probe/runtime guard is added to production code (`metadata-tikv/src` untouched); the `#[cfg(feature="tikv")]` / `WYRD_TIKV_PD_ENDPOINTS` gates are test-harness skips, not a papered-over load-time side effect. Decision owed: does the Option-B evidence architecture (pure oracles at Check + live teeth off-Check) genuinely close the determinism/evidence gap the slice exists for, given the at-Check layer alone carries no TiKV-correctness weight?
- [x] T5 Judgment — Oracle is reviewer+human sign-off. Judgment owed: accept that the binding correctness evidence for the entire slice lives off-Check (privileged Tier job), with the at-Check gate certifying only routing/arithmetic/coverage. Also owed per brief §NEEDS-HUMAN: the metadata-nemesis methodology ADR question (is symmetric-partition-by-analogy to ADR-0039 sufficient, or does the metadata swap need its own ADR refinement — architecture-board call, patch correctly mints no ADR) and the #365 static-endpoints reduced bar.
- [x] Validation — fitness-to-purpose — Whether this slice, as delivered, actually demonstrates the redb→TiKV swap upholds ADR-0015 on a real cluster is a human sign-off decision. It turns entirely on the off-Check captured evidence under `results/issue_257/evidence/` (real-cut GREEN, no-op negative-control RED, mutated re-check RED, restored GREEN) with `fault_materialized=true` from the PD-heartbeat oracle — none of which is re-runnable in this artifact-only review. Confirm those logs exist, were produced on a live ≥3-replica pingcap/tikv v8.5.1 cluster, and show the required flips before accepting.
- [x] **The entire recorded green sits on a stale base; the red→green was never
- [x] **The binding behavioural evidence (Success criterion (d), the four-leg
- [x] `crates/dst/tests/tikv_await_commit_interleaving.rs:7` explicitly says the Check-time DST seed is redb-only and cannot fail on a `TikvMetadataStore::commit` regression. That may be the intended Option-B fallback, but it does not satisfy the originally requested at-Check real-TiKV/madsim commit-path oracle; human sign-off should decide whether the declared fallback plus off-Check Tier-1 evidence is acceptable.

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
