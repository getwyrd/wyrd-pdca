# Result — issue 469 / fdb-deploy-profiles

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: TiKV has deploy recipes at all three ADR-0043 fixture tiers; FoundationDB —
- Success criterion: `cargo test -p xtask --test fdb_deploy_profiles` (a NEW test file —
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope: Bring FoundationDB to parity with TiKV across the ADR-0043 fixture tiers, and

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
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

Reviewing issue 469: add FoundationDB deploy profiles at the missing ADR-0043 tiers and document the TiKV/FDB single-zone pair.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The decision target is explicit: close the missing FDB multi-replica and single-zone deploy parity gap without renaming or adding an xtask bring-up arm; see `brief.md:86`. |
| C2 Reproduction (red pre-fix) | PASS | The red must not depend on Docker; in a scratch reverse-patch run, `cargo test -p xtask --test fdb_deploy_profiles` failed with no such test target, and the added test pins that red at `xtask/tests/fdb_deploy_profiles.rs:49`. |
| C3 Change | PASS | The scoped change adds the FDB profile matrix and the two missing FDB compose profiles, which is the intended review surface at `deploy/README.md:8`, `deploy/fdb-multi-replica/docker-compose.yml:56`, and `deploy/small-multi-node-fdb/docker-compose.yml:57`. |
| C4 Verification (red→green) | NEEDS-HUMAN | Decide whether the available rerun is an acceptable substitute for the configured harness: `./engine/xtask.sh ci` is absent here, but scratch reverse/forward was red→green, `cargo test -p xtask --test fdb_deploy_profiles`, `cargo test -p xtask --test deploy_no_orchestrator_coupling`, and `cargo xtask ci` all passed; gate oracle is `check-gates.json:33`. |
| C5 Causal adequacy | PASS | The fix addresses the named cause rather than adding a runtime probe: FDB gains the missing >=3-process/fault-sidecar tier and explicit single-zone pairing/canonicality at `deploy/fdb-multi-replica/docker-compose.yml:8` and `deploy/README.md:25`. |
| T1 Structure | PASS | The structure decision stays within the brief's boundary: deploy assets remain outside the workspace and the new guard is a separate test file, not an edit to the existing deploy-coupling test; see `deploy/README.md:1` and `xtask/tests/fdb_deploy_profiles.rs:24`. |
| T2 Shape | PASS | The shape decision is supported by compose config and source: the FDB fault tier declares three FDB processes plus the fault sidecar, and the single-zone FDB stack declares etcd/FDB/D-server/custodian/gateway roles at `deploy/fdb-multi-replica/docker-compose.yml:65` and `deploy/small-multi-node-fdb/docker-compose.yml:66`. |
| T3 Runtime | NEEDS-HUMAN | Decide whether to accept without daemon-backed runtime evidence: Docker daemon/buildx was not usable here, the live FDB bring-up/partition-heal was not exercised, and the target tree lacks dependency #470's `deploy/docker/wyrd/Dockerfile` that this stack references at `deploy/small-multi-node-fdb/docker-compose.yml:178`. |
| T4 Contribution | PASS | The contribution is testable and localized: the new test covers pure filesystem red plus compose-config structure while preserving the existing deploy coupling test's role; see `xtask/tests/fdb_deploy_profiles.rs:11` and `xtask/tests/fdb_deploy_profiles.rs:123`. |
| T5 Judgment | NEEDS-HUMAN | Decide whether the prior-art check is sufficient from artifacts: local merged history by affected new paths was empty, but closed/rejected PR state cannot be mechanically settled from the provided files; the affected profile names are anchored at `deploy/README.md:16`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Human sign-off must clear the intended production smoke bar: the full `small-multi-node-fdb` bring-up and S3 gateway response are maintainer-deferred runnable steps, not exercised here, at `deploy/README.md:207`. |

### Advisory — adversary

# Adversarial review — issue #469 / fdb-deploy-profiles

Lens: refute the red→green evidence and the reviewer's verdict; find the input that
breaks the fix. Grounded on the target at `/home/eddie/wyrd/wyrd.pdca-wt` (HEAD `84a9afb`).

## Evidence re-run (could not refute)

- **Re-ran the per-fix proof.** `cargo test -p xtask --test fdb_deploy_profiles` → **7/7
  green**, and the two Docker-backed tests *actually executed* (not skipped): `docker
  compose` v5.2.0 is present and `docker compose -f
  deploy/small-multi-node-fdb/docker-compose.yml config` renders a real 901-line config
  (exit 0), likewise `fdb-multi-replica` with `--profile fault` (exit 0). The asserted RED
  is over-determined and honest: on `origin/main` the two compose files do not exist
  (`both_new_fdb_stacks_exist` panics) and `deploy/README.md` has zero `fdb` occurrences
  (`git show origin/main:deploy/README.md | grep -ci fdb` → 0), so
  `readme_profile_matrix_names_all_six_profiles` also fails pre-fix. **Attempted to refute
  the red-without / green-with claim; could not.**
- **Attempted to refute the role-completeness assertion as weaker-than-claimed; could
  not.** The brief says the new `small_multi_node_fdb_compose_config_is_structurally_valid`
  makes "the same role-completeness assertion" as the TiKV peer. It does: the peer at
  `xtask/tests/deploy_no_orchestrator_coupling.rs:184-198` uses the identical
  `dserver0`/`dserver8` spot-check + one-each `custodian0`/`gateway0` convention. The new
  test is in fact *stronger* on backend wiring — the TiKV peer asserts no
  `--metadata-backend` at all.

## Fix — concrete cases that break / slip past the guard

- **NEEDS-HUMAN — `deploy/small-multi-node-fdb/docker-compose.yml:178` references a
  Dockerfile that does not exist in the target tree.** The `dserver0` build stanza points
  at `deploy/docker/wyrd/Dockerfile` (context `../..`), which is #470's artifact — but
  `deploy/docker/` is absent at HEAD; that Dockerfile lives only on the unmerged branch
  `origin/enhancement/470-wyrd-fdb-oci-image` (commit `c233380`, `git merge-base
  --is-ancestor c233380 HEAD` → **not** an ancestor). The tests stay green because `docker
  compose config` only parses and never validates build-context contents, but the
  README/compose-documented `docker compose -f .../small-multi-node-fdb up -d` would fail
  immediately at build. The brief made #470 a *hard* precondition ("If it is absent when Do
  runs, the fold has failed — **stop and say so**"). Whether the wave fold supplies #470 at
  merge time, or the bundle was authored on a broken precondition, is a human call —
  verdict provisional (fold-state, not a patch defect I can prove).
- **Over-broad / tautological backend-wiring assertion —
  `xtask/tests/fdb_deploy_profiles.rs:948-951.`** The structural test's final check is
  `merged.contains("--metadata-backend") && merged.contains("fdb")`. The token `fdb`
  appears unconditionally in the rendered config via the image (`foundationdb/foundationdb`),
  the service names (`fdb0..fdb2`) and the volume (`fdb-cluster`) — 84 occurrences — so this
  conjunction is true regardless of what backend the wyrd roles actually pass. The only
  *precise* guard is the pure-fs test at `:761`,
  `compose.contains("\"--metadata-backend\", \"fdb\"")`, which requires just **one**
  occurrence, and the three custodians (`docker-compose.yml:305,316,327`) already satisfy
  it. **Concrete regression the guard misses:** flip the three `gatewayN` commands
  (`:344,361,378`) to `"--metadata-backend", "tikv"` while leaving custodians on `fdb` —
  all 7 tests still pass, yet the S3 front door of the "FDB single-zone stack" would be
  wired to the wrong backend. Neither test asserts *every* role uses fdb, nor that *no*
  role uses tikv.
- **Check exercises parse-only, never bring-up — semantic validity is unverified.** Every
  Docker-backed assertion is `docker compose config` (YAML render). It cannot catch: a
  non-existent/unpullable image tag, an `fdbserver` that never reaches `configuration
  missing`→configured, a `command:` arg the wyrd binary rejects, or that FDB is actually
  reachable. The brief pre-declares the 21-container bring-up as deferred/maintainer-
  confirmed, so this is advisory rather than a refutation — but it means "the FDB stack
  works" is not among the facts this patch demonstrates at Check; only "its YAML parses and
  names the expected services/strings" is.

## Verdict / reviewer claims to weigh

- The `C4-verify` row ("red without the fix, green with it") is sound as far as the *test
  file* goes, but note its RED is carried entirely by the pure-filesystem
  non-existence checks; the Docker legs would be green-or-red purely on whether `docker
  compose` is installed, and add no independent binding signal beyond "the YAML parses."
- **The one claim I would not sign without a human:** that this slice ships a *runnable*
  `small-multi-node-fdb` stack (brief "Deferred ≠ unbuilt … every service definition has
  been parsed and role-checked"). Parsed, yes; **buildable in this target, no** — its wyrd
  image Dockerfile is not present (finding above). Adjudicate whether the fold is
  contracted to land #470 before this merges.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Decide whether the available rerun is an acceptable substitute for the configured harness: `./engine/xtask.sh ci` is absent here, but scratch reverse/forward was red→green, `cargo test -p xtask --test fdb_deploy_profiles`, `cargo test -p xtask --test deploy_no_orchestrator_coupling`, and `cargo xtask ci` all passed; gate oracle is `check-gates.json:33`.
- [ ] T3 Runtime — Decide whether to accept without daemon-backed runtime evidence: Docker daemon/buildx was not usable here, the live FDB bring-up/partition-heal was not exercised, and the target tree lacks dependency #470's `deploy/docker/wyrd/Dockerfile` that this stack references at `deploy/small-multi-node-fdb/docker-compose.yml:178`.
- [ ] T5 Judgment — Decide whether the prior-art check is sufficient from artifacts: local merged history by affected new paths was empty, but closed/rejected PR state cannot be mechanically settled from the provided files; the affected profile names are anchored at `deploy/README.md:16`.
- [ ] Validation — fitness-to-purpose — Human sign-off must clear the intended production smoke bar: the full `small-multi-node-fdb` bring-up and S3 gateway response are maintainer-deferred runnable steps, not exercised here, at `deploy/README.md:207`.
- [ ] external dependency: #470's `deploy/docker/wyrd/Dockerfile` + `wyrd:fdb` image (absent from the worktree — the wave fold did not deliver #470) — blocks the deferred full bring-up of `deploy/small-multi-node-fdb/` (21 containers) and the "an S3 gateway answers with --metadata-backend fdb" smoke bar; the binding criterion (assertions 1–3) and the live `fdb-multi-replica` leg are unaffected and were exercised here.

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
- Iteration delta (if iterating): #470 is now merged and ready — the hard-precondition blocker (absent `deploy/docker/wyrd/Dockerfile` / `wyrd:fdb` image) is cleared, so the single-zone stack is now buildable in the target tree. Rebuild against that merged base and address the reviewer's actionable items: - Tighten the tautological backend-wiring assertion in `xtask/tests/fdb_deploy_profiles.rs:948-951`. `contains("--metadata-backend") && contains("fdb")` is true regardless of what backend the roles pass ("fdb" appears 84x via image/service/volume names). Assert that EVERY wyrd role (dservers, custodians, gateways) opens `--metadata-backend fdb` and that NO role uses `tikv` — the current guard passes even if the three gateways are flipped to tikv. - Now that #470 supplies `wyrd:fdb`, exercise the deferred single-zone leg beyond parse-only: at minimum confirm the build context resolves and the image builds; run the full configured harness (`./engine/xtask.sh ci`), not just the scratch substitute, so C4 has the configured-oracle evidence.
- By / date: Eduard Ralph / 2026-07-10

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
