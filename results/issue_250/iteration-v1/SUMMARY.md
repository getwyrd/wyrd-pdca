# Result — issue 250 / tier1-jepsen-consistency-harness

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: The Tier-1 **Jepsen** consistency leg of proposal 0005 §13.2 (`0005:408`) was
- Success criterion: **DECISION (the human/maintainer chose Option A): build the genuine
- Repo + branch target: getwyrd/wyrd @ main   (per INTEGRATION §2 — Wyrd targets `main`;
- Scope (one logical fix) / out of scope: Build the Tier-1 Jepsen consistency leg as the genuine Jepsen framework

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass —                as its own file to earn the full red->green.
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

### Advisory — codex

- `.github/workflows/tier1-jepsen.yml:57`: The job hard-codes `WYRD_JEPSEN_ENDPOINTS` to host ports 50051-50055, but the compose file it starts publishes scaled `dserver` replicas on ephemeral host ports (`crates/chunkstore-grpc/tests/docker-compose.yml:20`). The readiness loop also falls through after retries, so the full run will dial mostly dead endpoints instead of the containers it just started.
- `jepsen/src/wyrd/jepsen.clj:109`: The read path shells out to `wyrd ls --prefix`, but the CLI only dispatches `put`, `get`, `d-server`, and `demo` (`crates/server/src/cli.rs:56`). Any Jepsen read op will fail with an unknown command before Elle can check a history.
- `jepsen/src/wyrd/jepsen.clj:83`: The workload only drives `wyrd put`/read-listing operations; nothing in the harness invokes the custodian repair/reconstruction loop required by the brief. As written, even a green Jepsen run would be testing gateway writes against D servers, not the production repair path under faults.
- `jepsen/src/wyrd/jepsen.clj:176`: The crash nemesis uses Jepsen remote-control commands (`pkill`, then `wyrd d-server ... &`) against Jepsen nodes, while the workflow starts D servers through Docker Compose and never passes node/container targets to the harness. That means the nemesis is not wired to kill/restart the actual compose replicas under test.
- `jepsen/test/wyrd/checker_test.clj:59`: The checker self-test compares `(:valid? result)` to the keyword `:valid`; Jepsen checker results use boolean `:valid?`, so the "consistent" control appears to fail even when Elle accepts the history.
- NEEDS-HUMAN — `jepsen/project.clj:21`: This introduces the pre-declared non-Cargo toolchain/dependency set (JVM/Clojure/Leiningen plus Jepsen/Elle), which remains outside `deny.toml`/Cargo review. Maintainer sign-off should explicitly accept that dependency posture or record it in an ADR.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T3 Runtime — The off-Check harness cannot run at the Rust gate, and `tier1-jepsen.yml` hardcodes fixed host ports 50051–50055 (`WYRD_JEPSEN_ENDPOINTS` + the `nc -z` readiness loop), but the reused `crates/chunkstore-grpc/tests/docker-compose.yml:18-21` publishes **target-only/ephemeral** host ports under `--scale` (its own comment: resolve via `docker compose port --index`). Decision owed: confirm the cluster is actually reachable on the assumed ports in the first run — as written the client likely dials closed ports.
- [ ] T5 Judgment — New non-Cargo toolchain (JVM/Clojure/Leiningen/Jepsen/Elle), pre-declared per INTEGRATION §4 — decision owed: accept the toolchain footprint and decide whether a short ADR recording the non-Rust test-toolchain choice is warranted. Plus harness-design judgment: Jepsen's SSH control-node/`c/exec` model vs a docker-compose–started cluster with a no-op `WyrdDB`, and the nemesis restart hardcoding `--bind 0.0.0.0:50051` for every killed node.
- [ ] Validation — fitness-to-purpose — Decision owed (maintainer Eduard Ralph, first `tier1-jepsen.yml` `workflow_dispatch` run): does the suite actually drive and correctly assert consistency over the **production** custodian repair/reconstruction path (no stale/torn reads, commit-point-atomic repair), and does the Elle checker self-test demonstrate a real planted-anomaly catch? None of this is observable at the Rust Check gate — the explicitly accepted Option-A deferred tradeoff.
- [ ] `jepsen/project.clj:21`: This introduces the pre-declared non-Cargo toolchain/dependency set (JVM/Clojure/Leiningen plus Jepsen/Elle), which remains outside `deny.toml`/Cargo review. Maintainer sign-off should explicitly accept that dependency posture or record it in an ADR.

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
- Iteration delta (if iterating): Rejected: the current harness is built against an API surface the product does not have, and would test nothing on its first run. Rebuild the Jepsen leg addressing the codex advisory findings (issue_250): 1. Read primitive missing — the read path shells `wyrd ls --prefix`, but the CLI (crates/server/src/cli.rs:56) only dispatches put/get/d-server/demo. There is no list/prefix-enumeration command. Either use a read strategy built on put/get with client-tracked keys, or add a list op to the product first; the list-append/Elle model currently has no backing observation primitive. 2. Port mismatch — tier1-jepsen.yml hardcodes host ports 50051-50055, but the reused crates/chunkstore-grpc/tests/docker-compose.yml publishes ephemeral host ports under `--scale dserver=5` (resolve via `docker compose port --index`). As written the client dials closed ports. 3. Repair path never driven — the workload only does `wyrd put` + read-listing; nothing invokes the custodian repair/reconstruction loop, which is the core brief requirement. A green run would test gateway writes, not the production repair path. 4. Nemesis not wired to the cluster — the crash nemesis uses Jepsen remote-control (pkill / `wyrd d-server &`) against Jepsen nodes, but the cluster is started via Docker Compose with no node/container targets passed. The nemesis does not kill/restart the actual compose replicas under test. Also revisit the hardcoded `--bind 0.0.0.0:50051` for every killed node. 5. Inverted self-test — jepsen/test/wyrd/checker_test.clj:59 compares (:valid? result) to the keyword :valid; Jepsen results use boolean :valid?, so the "consistent" control fails even when Elle accepts the history. Fix so the self-test actually demonstrates a planted-anomaly catch and a clean pass. Toolchain posture (JVM/Clojure/lein/Jepsen/Elle, project.clj) is acceptable as the pre-declared Option-A footprint; not the reason for rejection.
- By / date: Eduard Ralph / 2026-06-26

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
