# Check review — issue 257 (M4.6 real-commit-over-madsim-tikv), **iteration 10**

**Task under review:** extend the realism-ladder Tier-1/Tier-2 lines across the redb→TiKV metadata
swap and author the DST determinism-gap seed — proving (or, under the ratified **Option-B** posture,
honestly conceding off-Check) that the production `TikvMetadataStore::commit` await-inside-commit
window upholds the ADR-0015 single-zone contract, **without editing `metadata-tikv/src` or
`traits`**. This is the 10th pass after nine rejections; the single ratified iter-9 ask was: *add a
`cargo check -p wyrd-metadata-tikv --features tikv --tests` step to `run_ci` so the `#[cfg(feature="tikv")]`
live-scenario bodies are actually compiled/type-checked at Check, and fix the two now-false
compile-at-Check claims* (seed docstring + brief line). Option B, exit-(b) seed relabelling, the
pure testkit oracles, the xtask dispatch, and the tier1/2 scenario rework were all **ratified — not
to be re-litigated**.

**Grounding note (target state).** `$PDCA_TARGET` = `/home/eddie/wyrd/wyrd.pdca-wt-l0` is the
**base** checkout of `feat/m4-production-metadata-backend`; the patch is not applied there (the new
files are absent — expected, this is base, not staleness). I grounded the pre-existing citations
(`crates/metadata-tikv/src/lib.rs`, `crates/dst/tests/concurrency.rs`, `deploy/tikv-single-node`) on
the target and the patch-added files on `patch.diff`. Both C4 gates are green in `check-gates.json`
(`C4-ci` pass, `C4-verify` pass); I did not re-run the full `cargo xtask ci` (it needs the `tikv`
feature + container toolchain), so C4 rows rest on the recorded gate results.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The narrow iter-9 spec is well-formed and grounded: add the feature-gated compile step + relabel the seed. Invariant surfaces (no `traits`, no `metadata-tikv/src`) are pinned in brief.md:149-152 and hold in the patch. |
| C2 Reproduction (red pre-fix) | NEEDS-HUMAN | At Check the only red→green is the pure-oracle negation (arithmetic mutation flips testkit/xtask unit tests — genuine, non-tautological). The **binding correctness reproduction** (a real TiKV commit-point regression → lost update) is *conceded off-Check* per Option B; the fault-effect-oracle "red when the fault is a no-op" is observable only in the privileged Tier job. Decision owed: the Tier-1/Tier-2 job must confirm the live legs actually reproduce red — it cannot be shown at Check. |
| C3 Change | PASS | Additive only: new DST seed, tier1/tier2 scenarios, testkit oracles, xtask dispatch+runners, deploy compose, one dev-dep. No `crates/traits` and no `crates/metadata-tikv/src` edit (target lib.rs:540-601 commit path is untouched) — the M4 thesis invariant (brief.md:149-152) holds byte-for-byte. |
| C4 Verification (red→green) | PASS | `check-gates.json` records `C4-ci` pass and `C4-verify` pass. The iter-9 gap is closed: `feature_gated_checks()` (xtask/src/main.rs, patch) adds `cargo check -p wyrd-metadata-tikv --features tikv --tests`, wired into `run_ci` and guarded by `ci_type_checks_feature_gated_metadata_scenario`, so the `#[cfg(feature="tikv")]` scenario bodies now compile in the whole-tree gate (green ⇒ they compile). Caveat: binding-correctness red→green is off-Check (ratified Option B), not a Check gate. |
| C5 Causal adequacy | NEEDS-HUMAN | Root cause is real and correctly diagnosed — `concurrency.rs:3-4` ("commit() internally synchronous, no await inside") is false for `TikvMetadataStore::commit`, which awaits between `get_for_update` (target lib.rs:560) and `txn.commit().await` (lib.rs:597). The C5 capability-probe smell-test does **not** fire (no `hasattr`/try-import guard in production src; src is untouched). What the human owes: accept that the fix's *binding* causal evidence lives off-Check and the at-Check layer is pure oracles + a coverage seed carrying no TiKV correctness weight — the contested symptom-vs-root-cause axis that drove nine rejections. |
| T1 Structure | PASS | Files sit where their siblings do: seed under `crates/dst/tests/`, scenarios under `crates/metadata-tikv/tests/`, pure dispatch in `xtask/src/metadata_faults.rs` mirroring `faults.rs::jepsen_dispatch`, oracles in `testkit`, compose under `deploy/` (outside the workspace, ADR-0010). |
| T2 Shape | PASS | Pure functions (`quorum`, `partition_outcome`, `partition_took_effect`, `heal_is_complete`, `consistency_passes`, `converged_exactly_once`) take independent inputs and return typed verdicts; `ConsistencySignals` keeps read-after-commit / converged-once / fault-materialized as separate fields (fixes the v6 collapsed-bit defect). Dispatch modelled as a two-variant enum, mirroring the sanctioned precedent. |
| T3 Runtime | PASS | Executed at Check: the redb seed runs under `cargo xtask ci → run_dst --cfg madsim`; the testkit + xtask dispatch unit tests run in `cargo test --workspace`; the tier scenarios are compiled (not run) via the new feature-gated check. Live scenarios `#[ignore]` + endpoint-gate → clean skip with no TiKV. Not committed-but-unexecuted. |
| T4 Contribution | PASS | The at-Check contribution is real and now enforced: a regression in the feature-gated live-scenario bodies flips the gate (the iter-9 hole), and mutating the quorum/oracle arithmetic flips the testkit unit tests with hand-computed expectations (not the literal returned). Honest scope: the seed's docstring disclaims all TiKV correctness weight, so it adds redb CAS-classification coverage only — no false teeth. |
| T5 Judgment | NEEDS-HUMAN | My re-derivation: the tautology that killed v1–v8 is **gone** — `tikv_await_commit_interleaving.rs` explicitly concedes "NO correctness weight for `TikvMetadataStore::commit`" and "a production regression cannot turn it red," satisfying iter-8 exit (b); the Option-B line's "newly-reachable interleaving" is conceded off-Check in the seed's own labelling. Decision owed: whether relabelling the flagship seed as pure coverage (rather than delivering an at-Check behavioural flip) is an acceptable final resolution for #257 after nine iterations — the sign-off axis the gate routes to human. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | The entire binding correctness proof (ADR-0015 contract on real ≥3-replica TiKV under a symmetric partition, Tier-2 real-I/O) is exercised only by the privileged `WYRD_TIER1`/`WYRD_TIER2` Tier job, and the new `SymmetricPartition` (distinct-loopback-IP `-s/-d` bidirectional cut + PD-side peer oracle) can only be shown to genuinely isolate the node **live** — I cannot drive it here (no container host / privileged netns in this worktree). Human/Tier-job must confirm: (a) the live legs land green; (b) the partition provably isolates (PD loses the store's heartbeat, `partition_took_effect` true) and heals with no leaked host firewall state; (c) the reduced Option-B at-Check bar + static-endpoints (#365) posture is acceptable as the deliverable; (d) the metadata-nemesis ADR question routes to the architecture board (patch correctly mints no ADR). |

## Notes for the human (§6 candidates)

- **Off-Check binding legs (C2/C4/V, pre-declared).** Nothing at Check proves the redb→TiKV swap
  upholds ADR-0015; that evidence is entirely in the privileged Tier job. To exercise it a reviewer
  needs a Docker host: `cargo xtask metadata-tier1` (with `WYRD_TIER1=1`, stands up
  `deploy/tikv-multi-replica`, isolates `127.0.0.2` bidirectionally, asserts the independent
  signals across the heal) and `cargo xtask metadata-tier2` (`WYRD_TIER2=1`). Confirm both green and
  that the fault-effect oracle goes **red on a no-op cut**.
- **Symmetric-partition soundness (Invariant B, the iter-7 must-fix locus).** The mechanism is much
  improved over the v6/v7 one-way `--dport` cut — distinct loopback IPs let it drop `-s`/`-d` on
  INPUT+OUTPUT, and the oracle now reads PD's peer view rather than probing the dropped port. But
  "the node can neither send nor receive" is only *asserted* by the design; it must be **observed
  live** (PD reporting the store `Disconnected`). This is the exact axis that failed twice — verify
  it in the Tier job, not from the diff.
- **C5 posture (ratified, not re-opened).** Option B is accepted because `madsim-tikv-client` does
  not exist and `TikvMetadataStore` holds a concrete `TransactionClient` (target lib.rs:421) built
  only via `connect()` (lib.rs:435-436), so an at-Check third-party-sim flip would require editing
  `metadata-tikv/src` — forbidden. Do not re-open; the human decision is only whether the resulting
  reduced at-Check bar is acceptable.
