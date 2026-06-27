# Check review — issue 250 / tier1-jepsen-consistency-harness

**Advisory. Artifact-only, decorrelated from the builder (build-notes.md withheld).**

## Grounding note (target state)
`$PDCA_TARGET` could not be read via the tool sandbox (approval-blocked). I grounded
on the canonical `getwyrd/wyrd @ main` checkout at `/home/eddie/wyrd/wyrd`, whose
pre-patch `xtask/src/faults.rs` matches the patch base **exactly**: `execute`/`run_shell`
at lines 70–107, `run_jepsen` at `faults.rs:170` calling
`execute("Tier-1 Jepsen", plan, "WYRD_TIER1_JEPSEN_CMD")` at `:177`. Confirmed absent
on target: `jepsen/` directory and `.github/workflows/tier1-jepsen.yml`; sibling models
`tier1-disk-faults.yml` (cron 03:00) and `tier2-kill-reconstruct.yml` (cron 05:00) present
(`integration-nightly.yml`/`mutants.yml` at 04:00) — so the patch's chosen 06:00 slot is
genuinely non-colliding. Patch applies and is correct against this base; no target
staleness affects the verdicts.

## Verdict table (5/5/1)

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | All three binding deliverables present: dispatch rewire (`run_jepsen` → `run_jepsen_harness()` at patch L56/L75; replaces the `WYRD_TIER1_JEPSEN_CMD` shell-out at target `faults.rs:177`), real in-repo Clojure/Jepsen+Elle suite (`jepsen/{project.clj,src/wyrd/jepsen.clj}`), and privileged `tier1-jepsen.yml`. Matches success criterion (1)(2)(3). |
| C2 Reproduction (red pre-fix) | PASS | Red is declared **criterion-absence** (born-at-tier, posture (ii)): pre-patch `jepsen_harness_dir` did not exist, so the new test fails to compile. Legitimate per brief, but the red is structural, not a behavioural failing assertion — see T4. |
| C3 Change | PASS | `execute`/`run_shell` correctly demoted to `#[cfg(test)]` (patch L16/L24): target shows `run_jepsen` at `:177` was the **last production caller** of `execute`; remaining callers are the two in `mod tests` (target `:319`,`:329`), so the gating prevents dead-code/clippy failure. Dispatch mirrors `run_disk_faults`→`run_tier1_scenario` (target `faults.rs:118-165`). |
| C4 Verification (red→green) | PASS | Gate `C4-ci` (fmt/clippy/build/test/deny/conformance) = pass and `C4-verify` red→green = pass in check-gates.json. The Rust dispatch wiring compiles and the unit test goes green; consistent with target matching patch base. |
| C5 Causal adequacy | PASS | Fix **removes the cause** (inert scaffolding shelling to a nonexistent external command) and builds the real harness — it does not guard a present capability. Symptom-guard smell-test does **not** fire: `tool_available("lein")`/`plan()` gating is the pre-existing opt-in pattern shared by both merged siblings, not a probe papering over a load-time side effect. |
| T1 Structure | PASS | Files land in conventional locations (`jepsen/` lein project layout, `.github/workflows/`), mirroring the two merged sibling legs. |
| T2 Shape | PASS | Rust dispatch shape mirrors `run_disk_faults`; workflow mirrors `tier1-disk-faults.yml`/`tier2-kill-reconstruct.yml`; cron 06:00 verified non-colliding against 03:00/04:00/05:00. |
| T3 Runtime | NEEDS-HUMAN | The off-Check harness cannot run at the Rust gate, and `tier1-jepsen.yml` hardcodes fixed host ports 50051–50055 (`WYRD_JEPSEN_ENDPOINTS` + the `nc -z` readiness loop), but the reused `crates/chunkstore-grpc/tests/docker-compose.yml:18-21` publishes **target-only/ephemeral** host ports under `--scale` (its own comment: resolve via `docker compose port --index`). Decision owed: confirm the cluster is actually reachable on the assumed ports in the first run — as written the client likely dials closed ports. |
| T4 Contribution | PASS | Single logical change, scoped to `run_jepsen` + net-new files; no production repair code touched (correctly — that is #251, merged). Caveat for the human: the sole Check-exercised unit test asserts only `jepsen_harness_dir(root) == root.join("jepsen")` — a path-join tautology; it does **not** cover the actual `Plan::Run → run_jepsen_harness()` rewire, so the behavioural claim "no longer reads `WYRD_TIER1_JEPSEN_CMD`" is unverified by Check. |
| T5 Judgment | NEEDS-HUMAN | New non-Cargo toolchain (JVM/Clojure/Leiningen/Jepsen/Elle), pre-declared per INTEGRATION §4 — decision owed: accept the toolchain footprint and decide whether a short ADR recording the non-Rust test-toolchain choice is warranted. Plus harness-design judgment: Jepsen's SSH control-node/`c/exec` model vs a docker-compose–started cluster with a no-op `WyrdDB`, and the nemesis restart hardcoding `--bind 0.0.0.0:50051` for every killed node. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decision owed (maintainer Eduard Ralph, first `tier1-jepsen.yml` `workflow_dispatch` run): does the suite actually drive and correctly assert consistency over the **production** custodian repair/reconstruction path (no stale/torn reads, commit-point-atomic repair), and does the Elle checker self-test demonstrate a real planted-anomaly catch? None of this is observable at the Rust Check gate — the explicitly accepted Option-A deferred tradeoff. |

## Notes for §6 (human-owed)
- **T3 / Validation (concrete):** Port-mapping mismatch — `tier1-jepsen.yml` assumes fixed
  50051–50055 while the compose file assigns ephemeral host ports under `--scale`. Highest-value
  item to verify (or fix) before the first real run; it would make the live run dial nothing.
- **T5:** New JVM/Clojure/lein/Jepsen/Elle toolchain — accept + decide on ADR (pre-declared,
  not blocking the brief).
- **Validation:** First privileged run must confirm green incl. the checker self-test
  (`lein test`, planted version-cycle flagged) and the full live consistency run.
