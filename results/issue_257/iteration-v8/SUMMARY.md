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

# Check review — issue 257 / m4.6-real-commit-over-madsim-tikv (iteration 8)

**Task under review:** extend the realism-ladder Tier-1 (integration + consistency-over-the-swap, in-repo Rust scenario per ADR-0039) and Tier-2 lines across the redb→TiKV metadata swap, and author the await-inside-commit determinism-gap DST seed — driving the *real, unchanged* production commit path (no `crates/traits` or `crates/metadata-tikv/src` edit). This is a re-plan after **seven** rejected iterations; the declared **Option-B** posture is in force (`madsim-tikv-client` does not exist, so the real-TiKV-commit-under-sim leg is off-Check) and was **ratified** at iteration 7 — do not re-open it.

> Grounding note: no target source was reachable and `$PDCA_TARGET` could not be confirmed (env inspection sandbox-blocked; no wyrd checkout under CWD). Citations are grounded on `patch.diff`. I could not independently re-run `cargo xtask ci` / `run-verify.sh`; I rely on the harness-recorded deterministic gates (`check-gates.json`: C4-ci **gating** PASS, C4-verify PASS) for what they cover, and confine my advisory judgment to structure and the v7 must-fix shapes.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Brief specifies the Option-B fallback precisely (brief.md:81-90, 305-313); patch implements it — no `madsim-tikv-client` alias (it doesn't exist, ratified brief.md:331), seed drives the real commit path over redb, pure oracles + off-Check tiers. Scope matches (patch.diff: metadata-tikv/Cargo.toml dev-dep only, no src/trait edit). |
| C2 Reproduction (red pre-fix) | NEEDS-HUMAN | No C2 gate configured (check-gates.json:15-22). v7's decisive rejection was that the on-Check red was a **compile-flip, not behavioural** (brief.md:331 must-fix 5; v3 shape brief.md:290). build-notes is withheld, so I cannot see *how* the run-verify red was produced for a src-less (test-only) patch; the human must confirm the seed's red (drop `require(prior)` → `committed==2`, tikv_await_commit_interleaving.rs:162-166) is a genuine behavioural flip against production code, not an unresolved-import compile error. |
| C3 Change | PASS | Invariants hold byte-for-byte: patch touches no `crates/traits` and no `crates/metadata-tikv/src` — only a test-only `wyrd-testkit` dev-dep in metadata-tikv/Cargo.toml (patch.diff:187-200), new DST/tier tests, testkit pure oracles, xtask runners, and a deploy compose. Seed drives `write::commit_overwrite` (production), not a patch-authored mode flag. |
| C4 Verification (red→green) | PASS | Gating gate C4-ci PASS ("xtask ci: all checks passed", check-gates.json:33-38) is authoritative and implies the `#[cfg(madsim)]` seed and workspace pure tests compile+run green; C4-verify PASS (check-gates.json:42-48). Caveat: I could not re-run either gate here, and the *mechanism* of the per-fix red is the open C2/T5 question — the gate result does not by itself certify the red was behavioural. |
| C5 Causal adequacy | NEEDS-HUMAN | Contested symptom-vs-root-cause under Option B: the root gap is that `concurrency.rs:3-4`'s "no await inside commit" rationale is false for `TikvMetadataStore::commit`'s percolator window (metadata-tikv/src/lib.rs:540-600, per brief.md:48-51). The at-Check seed exercises the CAS commit-point contract over **redb** (tikv_await_commit_interleaving.rs:98-186) and honestly disclaims proving TiKV's await-inside-commit window (lines 52-61); that leg's correctness is deferred off-Check. Human must confirm the redb-CAS seed + reworked off-Check TiKV legs adequately close the determinism gap. (C5 symptom-guard smell-test does NOT fire: gating is `#[cfg(feature=tikv)]`/endpoint test-gating, not a capability probe papering a production load-time side effect.) |
| T1 Structure | PASS | Pure routing/arithmetic in xtask::metadata_faults + wyrd-testkit; privileged runners in xtask/src/faults.rs; scenarios in metadata-tikv/tests — mirroring the `jepsen_dispatch` / `tier1_jepsen_consistency` precedents the brief names (brief.md:202-209). Module wired (xtask/src/lib.rs:18; main.rs dispatch patch.diff:1437-1438). |
| T2 Shape | PASS | New public seam is coherent and sibling-shaped: `quorum`/`partition_outcome`/`partition_materialized`/`converged_exactly_once`/`ConsistencySignals`/`consistency_passes`/`partition_took_effect`/`heal_is_complete` (testkit/src/lib.rs new fns) with independent hand-computed unit oracles (patch.diff:1012-1145), and a `MetadataTierDispatch` enum with both routes representable (metadata_faults.rs). |
| T3 Runtime | PASS | At-Check artifacts execute: seed runs under `--cfg madsim` via `run_dst` (brief.md:331 confirms T3), pure oracle + dispatch tests run in `cargo test --workspace`; tier scenarios `#[ignore]` + endpoint-gate and skip cleanly (tier1_metadata_consistency.rs:278-286; tier2_metadata_io.rs:740-748). Live tier execution is off-Check by design (deferred). |
| T4 Contribution | PASS | Keeps the v5/v6 survivors (quorum arithmetic, `partition_materialized`) and now **wires** the previously-dead `partition_took_effect`/`heal_is_complete` oracles into the live scenario (tier1_metadata_consistency.rs:344-346, 380) — directly addressing v7 must-fix 2's "computed, never applied". No regression to protected code. |
| T5 Judgment | NEEDS-HUMAN | The load-bearing judgment across 7 prior rejections: has iteration 8 escaped the rejected shapes? Must-fix 1 (bidirectional `-s`/`-d` on distinct loopback IPs, tier1:505-520 + compose distinct IPs patch.diff:1191-1228), must-fix 2 (PD-side oracle `pd_store_state`, tier1:667-690), must-fix 3 (surfaced non-lossy heal, tier1:559-635), must-fix 4 (seed bound to `write::commit_overwrite`) all *appear* addressed on the artifacts — but soundness of the live partition/oracle can only be confirmed on a privileged cluster (off-Check), and whether the redb seed is a genuinely non-vacuous binding vs a re-proof of redb atomicity (v1 shape) is the human's call. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Whether this slice, as the earlier-wave prerequisite #257 feeds (#258 folds in the seed; brief.md:159-171), and whether the reduced at-Check bar (pure oracles + coverage seed) plus the off-Check binding Tier-1/Tier-2 legs together satisfy M4.6's success criterion, is the human sign-off. Also confirm: the privileged CI/eval Tier job that will observe the live green (name the confirmer), the static-endpoints reduced bar (#365), and that the patch correctly mints no metadata-nemesis ADR (architecture-board routing, brief.md:258-265). |

### Advisory — adversary

# Adversarial review — issue_257 (iteration 8)

Advisory only; never gates. Grounded on `$PDCA_TARGET`
(`/home/eddie/wyrd/wyrd.pdca-wt-l0` @ `feat/m4-production-metadata-backend`).
Attack order: the evidence → the fix → the verdict.

## The evidence (the at-Check red→green)

- **NEEDS-HUMAN — The flagship at-Check seed drives `RedbMetadataStore`, not the TiKV
  code #257 exists to cover, and is a near-clone of the pre-existing `concurrency.rs` —
  re-instantiating the v1 rejected shape.**
  `crates/dst/tests/tikv_await_commit_interleaving.rs:110-186` instantiates
  `RedbMetadataStore::in_memory()` (`:112`) and races two writers through
  `write::commit_overwrite` (`:140`) → `metadata::commit_chunk_map`
  (`crates/core/src/metadata.rs:299-317`) → the **generic** `store.commit`. That is the
  identical store, identical helper, and identical await window (intent + write_fragments
  between reading `prior` and committing) already exercised by
  `crates/dst/tests/concurrency.rs:35-94` (`exactly_one_concurrent_writer_wins`). The only
  behavioural delta is 4 writers → 2 and an added `conflicted == 1` assert (which, with
  `committed == 1` and two writers, is arithmetically implied). **A production regression in
  `TikvMetadataStore::commit` — the `get_for_update`(`crates/metadata-tikv/src/lib.rs:560`)
  → `txn.commit().await`(`:597`) window the brief names as the defect surface — cannot flip
  this seed, because the seed never constructs a `TikvMetadataStore`.** The docstring itself
  concedes it (`tikv_await_commit_interleaving.rs:54-56`: "What redb cannot exhibit — and
  this seed therefore does NOT claim to prove — is TiKV's await-inside-`commit()` percolator
  window"). Concrete failing case for the claim: delete the `get_for_update` re-check at
  `metadata-tikv/src/lib.rs:555-574` and the seed stays green. This is v1's "decorative DST
  seed that re-proved redb's atomicity."

- **NEEDS-HUMAN — C4-verify's "red without the fix, green with it" is unverifiable from the
  supplied artifacts and, given the seed above, is at best redundant with `concurrency.rs`
  and at worst a compile/file-absence flip (the iter-1/v3 forbidden shape, iter-7 must-fix 5).**
  `check-gates.json:42-48` marks C4-verify PASS via `./engine/scripts/run-verify.sh`, which
  does **not** exist in the target worktree, so the perturbation that produced "red" cannot be
  inspected. Because the sole at-Check flippable artifact drives redb, the only production
  code whose perturbation could flip it is the **shared** `commit_chunk_map` /
  `RedbMetadataStore::commit` — but that same perturbation flips `concurrency.rs` identically,
  so the new seed adds zero incremental red→green over an already-committed test. A human must
  confirm the red was a *behavioural* perturbation of production code that **this seed catches
  and `concurrency.rs` does not**; otherwise C4-verify is redundant or non-behavioural.

## The fix (find the input that breaks it)

- **NEEDS-HUMAN — Even accepting the ratified Option-B posture, the seed fails its own
  narrowed Option-B mandate.** `brief.md:86-89` requires the seed-as-coverage-artifact to
  assert "the `concurrency.rs` synchronous-commit rationale is unsound; here is a
  newly-reachable interleaving." The seed exhibits **no** newly-reachable interleaving: its
  await window is byte-for-byte the `concurrency.rs` window, and it runs over redb, where the
  "no await inside commit" rationale (`concurrency.rs:3-4`) is *true*. It therefore cannot
  demonstrate that rationale is unsound — it re-confirms it. The docstring's "under the
  interleaving the `concurrency.rs` rationale declares impossible"
  (`tikv_await_commit_interleaving.rs:60-61`) is the reverse of what the code does.

- The Tier-1 live scenario (`crates/metadata-tikv/tests/tier1_metadata_consistency.rs`) and
  Tier-2 leg are `#[ignore]`d and endpoint-gated, so they execute **only** off-Check in the
  privileged Tier job — none of their partition/heal/oracle behaviour is exercised at Check.
  The pure `wyrd_testkit` oracles (`partition_took_effect`, `heal_is_complete`,
  `converged_exactly_once`, `consistency_passes`, `partition_materialized`,
  `crates/testkit/src/lib.rs:889-995`) and the xtask dispatch tests are genuine
  non-tautological unit coverage — **I attempted to refute these as tautologies and could
  not**: each asserts a hand-computed expectation independent of the function body
  (`testkit/src/lib.rs:1012-1145`). They are, however, *arithmetic about* a partition, not
  evidence *of* one; their green says nothing about whether the live leg's `SymmetricPartition`
  actually isolates a node — that remains an off-Check, human-confirmed claim.

## The verdict (where the reviewer may have rationalized)

- **NEEDS-HUMAN — The docstring's independence claim is the specific unwarranted verdict.**
  `tikv_await_commit_interleaving.rs:36-38` ("a real production regression in the commit-point
  re-check … produces a real lost update this seed catches. This is the independence the six
  rejected iterations lacked") asserts teeth against production code the seed does not invoke.
  Iter-7 must-fix 4 offered two clean exits: bind the seed to the real commit path so a
  production regression flips it, **or** label it honestly as pure coverage with no correctness
  weight. The patch does neither: it binds to redb (not the swap under test) while still
  asserting independence/teeth. A reviewer crediting this docstring as "the honest red→green
  the six iterations lacked" has been fooled by prose, not code.

## Attempted-but-could-not-refute

- The pure quorum/consistency oracles and the `metadata_tier_dispatch` routing tests are
  non-tautological and flippable at Check; I could not reduce them to `concurrency.rs`-style
  redundancy or boolean vacuity.
- The invariant "no `crates/metadata-tikv/src` and no `crates/traits` edit" holds in the diff
  (only `metadata-tikv/Cargo.toml` gains a `wyrd-testkit` dev-dep; `traits/src/lib.rs`
  untouched) — I could not find a stealth trait/`src` change.

### Advisory — codex

- crates/metadata-tikv/tests/tier1_metadata_consistency.rs:365 — `heal()` marks the partition as healed even when one or more `iptables -D` calls failed and it is about to return `Err`; if the following `.expect(...)` panics, `Drop` sees `healed == true` at crates/metadata-tikv/tests/tier1_metadata_consistency.rs:410 and skips the panic-safety cleanup, so a partial heal can leak host firewall rules.
- xtask/src/faults.rs:646 — the new metadata Tier runner starts the ignored TiKV scenario immediately after `docker compose up -d`; unlike the existing TiKV conformance runner, it does not wait/retry for PD/store readiness before dialing, so opted-in Tier-1/Tier-2 runs can fail spuriously while the TiKV cluster is still bootstrapping.
- NEEDS-HUMAN — crates/dst/tests/tikv_await_commit_interleaving.rs:94 — the at-Check DST seed uses `RedbMetadataStore`, not `TikvMetadataStore` or a `madsim-tikv-client` alias, so it does not exercise the TiKV await-inside-`commit()` path described in the brief; accept only if the declared Option-B fallback/evidence posture is what sign-off intends.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C2 Reproduction (red pre-fix) — No C2 gate configured (check-gates.json:15-22). v7's decisive rejection was that the on-Check red was a **compile-flip, not behavioural** (brief.md:331 must-fix 5; v3 shape brief.md:290). build-notes is withheld, so I cannot see *how* the run-verify red was produced for a src-less (test-only) patch; the human must confirm the seed's red (drop `require(prior)` → `committed==2`, tikv_await_commit_interleaving.rs:162-166) is a genuine behavioural flip against production code, not an unresolved-import compile error.
- [ ] C5 Causal adequacy — Contested symptom-vs-root-cause under Option B: the root gap is that `concurrency.rs:3-4`'s "no await inside commit" rationale is false for `TikvMetadataStore::commit`'s percolator window (metadata-tikv/src/lib.rs:540-600, per brief.md:48-51). The at-Check seed exercises the CAS commit-point contract over **redb** (tikv_await_commit_interleaving.rs:98-186) and honestly disclaims proving TiKV's await-inside-commit window (lines 52-61); that leg's correctness is deferred off-Check. Human must confirm the redb-CAS seed + reworked off-Check TiKV legs adequately close the determinism gap. (C5 symptom-guard smell-test does NOT fire: gating is `#[cfg(feature=tikv)]`/endpoint test-gating, not a capability probe papering a production load-time side effect.)
- [ ] T5 Judgment — The load-bearing judgment across 7 prior rejections: has iteration 8 escaped the rejected shapes? Must-fix 1 (bidirectional `-s`/`-d` on distinct loopback IPs, tier1:505-520 + compose distinct IPs patch.diff:1191-1228), must-fix 2 (PD-side oracle `pd_store_state`, tier1:667-690), must-fix 3 (surfaced non-lossy heal, tier1:559-635), must-fix 4 (seed bound to `write::commit_overwrite`) all *appear* addressed on the artifacts — but soundness of the live partition/oracle can only be confirmed on a privileged cluster (off-Check), and whether the redb seed is a genuinely non-vacuous binding vs a re-proof of redb atomicity (v1 shape) is the human's call.
- [ ] Validation — fitness-to-purpose — Whether this slice, as the earlier-wave prerequisite #257 feeds (#258 folds in the seed; brief.md:159-171), and whether the reduced at-Check bar (pure oracles + coverage seed) plus the off-Check binding Tier-1/Tier-2 legs together satisfy M4.6's success criterion, is the human sign-off. Also confirm: the privileged CI/eval Tier job that will observe the live green (name the confirmer), the static-endpoints reduced bar (#365), and that the patch correctly mints no metadata-nemesis ADR (architecture-board routing, brief.md:258-265).
- [ ] crates/dst/tests/tikv_await_commit_interleaving.rs:94 — the at-Check DST seed uses `RedbMetadataStore`, not `TikvMetadataStore` or a `madsim-tikv-client` alias, so it does not exercise the TiKV await-inside-`commit()` path described in the brief; accept only if the declared Option-B fallback/evidence posture is what sign-off intends.

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
- Iteration delta (if iterating): Rejected on the flagship at-Check seed, not the posture: Option B stays ratified (do NOT re-open it), and the pure testkit oracles, xtask dispatch, tier1/tier2 scenario rework (must-fixes 1-3), and the no-src/no-traits invariants all survive — keep them. The defect is tikv_await_commit_interleaving.rs: it instantiates RedbMetadataStore, never a TikvMetadataStore, so it is a near-clone of the pre-existing concurrency.rs (v1's rejected shape) while its docstring claims "the independence the six rejected iterations lacked." Acceptance test the next attempt must survive: perturbing the TiKV commit-point re-check (delete/weaken the get_for_update re-check at metadata-tikv/src/lib.rs:555-574) must flip an at-Check artifact — today the seed stays green under that perturbation, and the only perturbation that flips it also flips concurrency.rs identically (zero incremental red->green). Iter-7 must-fix 4's two exits are now ENFORCED, pick exactly one: (a) bind the seed so a behavioural perturbation of the code under swap (metadata-tikv/src, or a seam demonstrably equivalent to its await-inside-commit window) flips it at Check; or (b) keep it as pure coverage and rewrite the docstring to claim NO correctness weight and NO newly-reachable interleaving — no third option where it binds to redb but asserts teeth. If (b), the brief's Option-B line "assert the concurrency.rs rationale is unsound; here is a newly-reachable interleaving" must be satisfied by some other at-Check artifact or explicitly conceded as off-Check in the seed's labeling. While in there, fix the two codex advisories: tier1_metadata_consistency.rs:365 — heal() sets healed=true even when iptables -D failed and it returns Err, so a panic skips Drop cleanup and leaks host firewall rules; xtask/src/faults.rs:646 — the metadata Tier runner does not wait for PD/store readiness after docker compose up -d (the existing TiKV conformance runner shows the pattern).
- By / date: Eduard Ralph / 2026-07-04

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
