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
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it.
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

**Advisory.** Artifact-only, decorrelated from the builder (no `build-notes.md`).
Target `$PDCA_TARGET=/home/eddie/wyrd/wyrd.pdca-wt` is **present and has the patch applied**
(jepsen/ dir + `jepsen_harness_dir` live in `xtask/src/`), so citations are grounded on
live target source — not a stale-target caveat. I independently re-derived every Rust API
the new code binds against; I trust the deterministic gate runs (`check-gates.json`:
C4-ci PASS, C4-verify PASS) for build/test *execution*.

## Independent grounding performed
- CLI surface (iteration-1 rejection #1 "no list command"): `wyrd put`/`get` accept
  `--key/--data-dir/--durability/--endpoints/--out` — `crates/server/src/cli.rs:132-150,199-204`.
  The harness now tracks keys in an atom and reads via `wyrd get` (no `wyrd ls --prefix`). ✔ fixed.
- Repair causal chain: `wyrd get` (cluster path) → `read::read_path` → `repair::enqueue_repair`
  at `crates/core/src/read.rs:251`; the custodian step opens the *same* `{data_dir}/meta.redb`
  (`crates/server/src/cli.rs:467`) and drains the queue. ✔ real path, not a stand-in.
- Custodian API: `reconcile_step(&zone,&custodian,None,None,Some(&ctx),None,0)` matches the
  7-arg signature at `crates/custodian/src/reconciliation.rs:65` (cf. sibling calls in
  `crates/custodian/tests/reconstruction.rs:330`); `ReconstructionContext{meta,fleet,topology}`
  at `crates/custodian/src/reconstruction.rs:69-78`; `Topology::new/register/distinct_domains/
  domain_of` at `crates/core/src/placement.rs:81,89,106,118`; `FailureDomain(pub String)` `:40`;
  `Custodian::elect`/`.leadership()` `crates/custodian/src/leadership.rs:31,51`;
  `GrpcChunkStore::connect` `crates/chunkstore-grpc/src/client.rs:49`. ✔ all bind (rejection
  "API surface the product does not have" addressed for the Rust side).
- Dispatch rewire: `xtask/src/main.rs:43` (`jepsen` → `run_jepsen`) → `faults.rs:178-216`
  (`Plan::Run → run_jepsen_harness` → `lein run test`); `execute`/`run_shell` now `#[cfg(test)]`
  — production no longer reads `WYRD_TIER1_JEPSEN_CMD`.
- Cron non-collision: `tier1-jepsen.yml` `0 2 * * *` is unique (03=disk-faults, 04=integration+
  mutants, 05=kill-reconstruct). ✔ matches brief guidance.

## Verdict table

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Success criterion's three BINDING deliverables are well-formed and present: dispatch rewire + unit test (`xtask/src/faults.rs:178-216,395-410`), in-repo Jepsen/Elle harness (`jepsen/`), privileged `tier1-jepsen.yml`. No spec ambiguity owed. |
| C2 Reproduction (red pre-fix) | PASS | Born-at-tier (declared posture ii): red = criterion-absence. Removing `pub mod jepsen`+`jepsen.rs` makes `xtask/tests/jepsen_orchestration.rs` fail to compile → non-zero exit = RED; gate `C4-verify` confirms red-without-fix. Note for human: this is a *compile-absence* red, not a behavioral assertion flip, as the brief accepts for a net-new tier. |
| C3 Change | PASS | `run_jepsen` dispatches to `run_jepsen_harness()` (in-repo `lein run test`), `execute`/`run_shell` gated `#[cfg(test)]` — `WYRD_TIER1_JEPSEN_CMD` is no longer consulted in production (`faults.rs:178-216`). Mirrors the merged `run_disk_faults`→`run_tier1_scenario` sibling shape. |
| C4 Verification (red→green) | PASS | `check-gates.json`: C4-ci PASS (fmt/clippy/build/test/deny/conformance) + C4-verify PASS. Independently confirmed every Rust API the `#[ignore]`d-but-compiled `jepsen_custodian_step.rs` binds exists with the cited signature. The full live Jepsen *green* run is explicitly DEFERRED off-Check (posture iii) — not asserted here. |
| C5 Causal adequacy | PASS | Root cause (inert dispatch to a nonexistent external command) is *removed and transformed*, not guarded: replaced by real in-repo dispatch + a harness driving the production `reconcile_step`→`reconstruction::reconcile` via `read.rs:251` `enqueue_repair`. Symptom-guard smell-test does **not** fire — no capability probe / runtime guard around a present capability; the env-var checks in `jepsen_custodian_step.rs:364-380` gate a live-cluster test, not a production path. (Adequacy of the harness *substance* → Validation row.) |
| T1 Structure | PASS | Mirrors the two merged sibling legs (#195/#196) in shape and placement. Minor (non-blocking): `jepsen_harness_dir` is duplicated in `xtask/src/faults.rs:200` and `xtask/src/jepsen.rs:29` — deliberate (lib export for the flippable test) and acknowledged in the doc comment, but a DRY smell a maintainer may want collapsed. |
| T2 Shape | PASS | Files land where the architecture expects: `jepsen/` Clojure project, `.github/workflows/tier1-jepsen.yml`, `xtask/` dispatch, `crates/chunkstore-grpc/tests/jepsen_custodian_step.rs`; `Cargo.lock`/`Cargo.toml` add only `wyrd-metadata-redb` dev-dep. |
| T3 Runtime | NEEDS-HUMAN | Live behavior is observable only in the off-Check `tier1-jepsen.yml` run. Decision owed: confirm the first run does not hit redb **single-writer lock contention** — the `cargo test` custodian step opens the same `meta.redb` during `:heal` while the (`:concurrency 1`) client worker may hold it — and that the fabricated `jepsen_topology` domains (A–E) reconcile correctly against a cluster whose `d-server` containers carry **no** `--failure-domain` (`jepsen/docker-compose.yml` command line). Why it matters: a lock stall or topology mismatch would make a "green" run vacuous. |
| T4 Contribution | PASS | One logical change — build the Tier-1 Jepsen leg. Production repair code untouched (#251 merged separately); scope confined to `run_jepsen` + net-new harness/workflow/test files, per brief out-of-scope list. |
| T5 Judgment | NEEDS-HUMAN | New non-Cargo external toolchain (JVM + Clojure + Leiningen + Jepsen + Elle) — pre-declared at plan (brief §Ordering-note, INTEGRATION §4), not `deny.toml`-gated. Decision owed: maintainer weighs at sign-off whether a short ADR recording the non-Rust test-toolchain decision is warranted (proposal 0005 accepts "Jepsen" in principle; this is the *how*). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | The iteration-1 rejection was "tests nothing on its first run." The rebuild fixes all five cited defects mechanically, but whether the harness — on its first `workflow_dispatch` `tier1-jepsen.yml` run — *actually* drives the production repair path AND Elle meaningfully flags Wyrd consistency anomalies (the list-append encoding of a KV object store; the planted version-cycle self-test in `jepsen/test/wyrd/checker_test.clj` truly returning `:valid? false`; the "demonstrated red" the brief requires) is empirically verifiable ONLY by running it. Decision owed: maintainer (Eduard Ralph) reviews the first run output before accept. |

## Prior-art / fork-discipline
- Prior-art ran **by affected file path** (brief §Prior-art): `xtask/src/faults.rs` history shows
  the two sibling legs (`0b5fea3` #195, `02983aa` #196) as *pattern precedent*, not duplicate;
  `jepsen/` and `tier1-jepsen.yml` confirmed net-new (added blobs in patch, no prior copy). PASS, not a duplicate.
- Fork-discipline §3/§4: target is `getwyrd/wyrd@main` with no maintenance branches — this is a
  net-new build, **not** a cross-version cherry-pick — so the "applies-clean ≠ correct-on-target"
  and "validate against clean upstream" hazards do not apply here. N/A.

### Advisory — codex

- jepsen/src/wyrd/jepsen.clj:357 — The harness still only injects `:kill`/`:heal`; there is no network partition nemesis in the generator, so it does not satisfy the brief's required "partitions + crashes" Jepsen leg.
- crates/chunkstore-grpc/tests/jepsen_custodian_step.rs:229 — The custodian step treats any successful `reconcile_step` result as a pass, including `Reconciled::Satisfied`; combined with the current read path only enqueuing repair for present-but-corrupt/misplaced fragments (`crates/core/src/read.rs:189`) and not for the killed server's missing fragment, this can run against an empty repair queue and never exercise reconstruction.
- jepsen/src/wyrd/jepsen.clj:237 — Failed `wyrd get` calls are filtered into a shortened read value but the transaction is still returned as `:ok` at line 246; that records availability failures as successful list observations, which can either create false Elle anomalies or mask that the read did not actually observe the full tracked list.
- NEEDS-HUMAN — jepsen/project.clj:26 — This adds the pre-declared non-Rust test toolchain footprint (`jepsen`/`elle` via Leiningen, plus the workflow-installed JVM/lein at `.github/workflows/tier1-jepsen.yml:76`); advisory does not gate it, but it is the explicit sign-off item from the brief.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T3 Runtime — Live behavior is observable only in the off-Check `tier1-jepsen.yml` run. Decision owed: confirm the first run does not hit redb **single-writer lock contention** — the `cargo test` custodian step opens the same `meta.redb` during `:heal` while the (`:concurrency 1`) client worker may hold it — and that the fabricated `jepsen_topology` domains (A–E) reconcile correctly against a cluster whose `d-server` containers carry **no** `--failure-domain` (`jepsen/docker-compose.yml` command line). Why it matters: a lock stall or topology mismatch would make a "green" run vacuous.
- [ ] T5 Judgment — New non-Cargo external toolchain (JVM + Clojure + Leiningen + Jepsen + Elle) — pre-declared at plan (brief §Ordering-note, INTEGRATION §4), not `deny.toml`-gated. Decision owed: maintainer weighs at sign-off whether a short ADR recording the non-Rust test-toolchain decision is warranted (proposal 0005 accepts "Jepsen" in principle; this is the *how*).
- [ ] Validation — fitness-to-purpose — The iteration-1 rejection was "tests nothing on its first run." The rebuild fixes all five cited defects mechanically, but whether the harness — on its first `workflow_dispatch` `tier1-jepsen.yml` run — *actually* drives the production repair path AND Elle meaningfully flags Wyrd consistency anomalies (the list-append encoding of a KV object store; the planted version-cycle self-test in `jepsen/test/wyrd/checker_test.clj` truly returning `:valid? false`; the "demonstrated red" the brief requires) is empirically verifiable ONLY by running it. Decision owed: maintainer (Eduard Ralph) reviews the first run output before accept.
- [ ] jepsen/project.clj:26 — This adds the pre-declared non-Rust test toolchain footprint (`jepsen`/`elle` via Leiningen, plus the workflow-installed JVM/lein at `.github/workflows/tier1-jepsen.yml:76`); advisory does not gate it, but it is the explicit sign-off item from the brief.

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
- Iteration delta (if iterating): Rejected: the harness is mechanically wired but still has substantive gaps (per codex advisory) that would make the first live run vacuous — the same "tests nothing on its first run" failure class as iteration-1, now in the Clojure/harness logic the gates don't exercise. Resolve all three before re-submitting: 1. jepsen/src/wyrd/jepsen.clj:357 — the nemesis only injects :kill/:heal. There is NO network-partition nemesis. The brief requires a "partitions + crashes" Jepsen leg. Add a real partition nemesis to the generator. 2. crates/chunkstore-grpc/tests/jepsen_custodian_step.rs:229 — the custodian step treats any successful reconcile_step (including Reconciled::Satisfied) as a pass. Combined with the read path only enqueuing repair for present-but-corrupt/ misplaced fragments (crates/core/src/read.rs:189) and NOT for the killed server's missing fragment, the step can run against an empty repair queue and never exercise reconstruction. Make the step assert that reconstruction actually fired (non-empty repair queue / fragment rebuilt), not just that reconcile_step returned Ok. 3. jepsen/src/wyrd/jepsen.clj:237,246 — failed `wyrd get` calls are filtered into a shortened read value but the transaction is still returned as :ok (line 246). This records availability failures as successful list observations, which can create false Elle anomalies or mask that the read did not observe the full tracked list. A failed/partial read must not be recorded as an :ok full observation. Not re-litigated here: the API bindings, dispatch rewire, and red->green gate all pass — the rebuild's structure is sound; the gaps are in the harness substance. The T5/ADR toolchain question (§6) is unresolved but secondary to the above; revisit at the next sign-off once the harness actually exercises the repair path.
- By / date: Eduard Ralph / 2026-06-26

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
