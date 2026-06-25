# Check review — issue 195 / tier1-disk-fault-harness

Advisory, artifact-only, decorrelated. Verdicts re-derived from `patch.diff`,
`brief.md`, `check-gates.json` and the post-patch target tree at
`/home/eddie/wyrd/wyrd.pdca-wt` (read-only; the patch is already applied there —
`xtask/src/disk_faults.rs` and `crates/custodian/tests/tier1_disk_faults.rs`
exist, and the production APIs the harness calls were verified to exist with
matching signatures). `build-notes.md` was withheld, so anything that turns on
the builder's *demonstrated red* is escalated rather than asserted.

## Verdict table

| Item | Verdict | Basis |
|------|---------|-------|
| C1 — C1 Spec | PASS | `brief.md:19-33` gives a binding, testable success criterion: real in-repo Tier-1 harness, test-exercised, with the privileged path excluded from `ci` (ADR-0016). Well-formed and decidable; the illustrative-vs-binding split is stated. |
| C2 — C2 Reproduction (red pre-fix) | NEEDS-HUMAN | Net-new infra: there is no pre-existing bug to make red. The "red" is criterion-absence — on `main`, `run_disk_faults` shells out to `WYRD_TIER1_DISK_CMD`, a command defined nowhere (`brief.md:91-98`) — PLUS the brief-mandated *demonstrated* red (`brief.md:121-123`: stub an orchestration helper/assertion, show the unit test fails). The automated `C4-verify` gate FAILED ("test PASSES without the fix") exactly as expected here. DECISION OWED: confirm Do captured the demonstrated red (e.g. stubbed `assert_campaign_passed`/`DmTablePlan` → `xtask` unit test goes red), not resting red on non-existence — that evidence is in withheld `build-notes.md`. |
| C3 — C3 Change | PASS | Real in-repo orchestration in `xtask/src/disk_faults.rs` (dm-table plan, setup/teardown step plan, campaign-report parse + verdict, privileged `run()`); `xtask/src/faults.rs:139` routes `Plan::Run → crate::disk_faults::run()`, deleting the `WYRD_TIER1_DISK_CMD` shell-out; `main.rs:45` dispatch already routed `disk-faults → run_disk_faults` (no new arm needed). Matches Scope (`brief.md:62-90`); Jepsen/Tier-2 untouched as required. |
| C4 — C4 Verification (red→green) | NEEDS-HUMAN | GREEN is solid: the gating `C4-ci` passed (`check-gates.json:33-39`) — `cargo xtask ci` compiled the workspace (the `#[ignore]`d scenario type-checks against `reconcile_step`/`ScrubContext`/`ReconstructionContext`/`FsChunkStore`, verified present in target) and ran the `disk_faults` unit tests green. RED is unconfirmable from artifacts: `C4-verify` FAILED (`check-gates.json:42-48`, non-gating, inapplicable to net-new), and the mandated demonstrated red lives in withheld `build-notes.md`. DECISION OWED: same red-capture confirmation as C2 — without it, only "compiles + helper-units pass" is proven, not "a regression flips it red". |
| C5 — C5 Causal adequacy | NEEDS-HUMAN | Always-human, and there is a real adequacy question: at Check only the host-independent helpers + compilation run; the faulted-chunk→full-redundancy outcome is asserted *solely* in the off-Check `#[ignore]`d scenario. Moreover reconstruction is driven over `healthy_view` which EXCLUDES the victim (`tier1_disk_faults.rs:284-297,439-462`) — the rebuild reads survivors and writes the spare, routing *around* the faulted device rather than through it; only the read-during-repair assertion touches the fault. DECISION OWED: judge whether compile-binding + a fault that reconstruct routes around adequately restores the proposal-0005 §13.2 invariant, given the privileged green is observed off-Check (`brief.md:124-133`). |
| T1 — T1 Structure | PASS | Two-part as the brief requires (`brief.md:99-111`): scenario at `crates/custodian/tests/tier1_disk_faults.rs`, orchestration units in `xtask/src/disk_faults.rs` `#[cfg(test)]` (`disk_faults.rs:978-1108`). Mirrors the `tier2_integration.rs` precedent; `#[ignore]` attribution is verbatim (`tier1_disk_faults.rs:365`). |
| T2 — T2 Shape | PASS | Scenario assertions bind the success criterion: domain-distinct placement, read-no-error during repair, `Reconciled::Changed`, repair queue drained, `version==2`, victim dropped from placement, `fragment_intact` over N distinct domains, byte-identical reread, `read_errors==0` (`tier1_disk_faults.rs:412-529`). Unit tests assert the dm-table strings, setup/teardown order, verdict logic and report parsing (`disk_faults.rs:984-1107`) — correct shapes, not tautologies. |
| T3 — T3 Runtime | PASS | The at-Check-runnable tests (the `disk_faults` `#[cfg(test)]` units) run green inside `cargo xtask ci` (`C4-ci` pass). The scenario body is legitimately `#[ignore]`d/off-Check — it needs root + `dmsetup`, excluded from the container-free gate per ADR-0016 (`tier1_disk_faults.rs:365`, `brief.md:124-128`). No runtime claim is made for it at Check. |
| T4 — T4 Contribution | PASS | The orchestration units are structurally flippable: `campaign_fails_when_nothing_was_exercised` guards the inert-dispatch rot (`disk_faults.rs:1081-1087`), and the verdict/parse tests reject under-replication, read errors and malformed reports (`disk_faults.rs:1069-1107`). The scenario contributes by compile-binding — a stub or a return to the shell-string would fail to compile. (The *proof* of load-bearingness via a demonstrated red is the C2/C4 human item.) |
| T5 — T5 Judgment | NEEDS-HUMAN | Always-human. DECISION OWED: is "host-independent helpers unit-tested + scenario compile-bound, substantive outcome deferred off-Check" the right Check bar for this tier — versus, say, an unprivileged in-memory fault simulation that could assert the repair outcome *at* Check? The chosen split is defensible and brief-sanctioned, but it is a design call a human should ratify. |
| V — Validation — fitness-to-purpose | NEEDS-HUMAN | Always-human. DECISION OWED: does this deliver the Tier-1 disk-fault coverage proposal 0005 §13.2 promises? The substantive repair-correctness assertion never executes at Check; fitness rests entirely on the off-Check privileged Tier-1 job (`.github/workflows/tier1-disk-faults.yml`) going green, which Check cannot observe — and on the C5 question of whether a victim-excluded reconstruct exercises the fault meaningfully. |

## §6 — items the human must clear

1. **(C2/C4) Demonstrated red.** `build-notes.md` is withheld here; confirm Do
   captured a demonstrated red (stub an orchestration helper/assertion → the
   `xtask disk_faults` unit test fails), as `brief.md:121-123` requires. The
   automated `C4-verify` red→green gate FAILED — that is expected for net-new
   born-at-tier code, but it means the only evidence the new seam is
   load-bearing is the (unseen) builder capture. Without it, Check has verified
   "compiles + helper-units green", not "a regression goes red".

2. **(C5) Does reconstruct traverse the fault?** Reconstruction runs over
   `healthy_view`, which excludes the victim server (`tier1_disk_faults.rs:284-297`,
   `:439-462`) — the rebuild sources from the `k` survivors and writes the spare,
   routing *around* the faulted device. Only the read-during-repair assertion
   reads through the fault. Decide whether this genuinely exercises "the
   production repair path over a real block-layer fault" (`brief.md:77-78,134-138`)
   or merely a normal degraded read plus a survivor-only rebuild.

3. **(C5/V) Deferred-posture acceptance.** The faulted-chunk→full-redundancy
   outcome is asserted only in the off-Check `#[ignore]`d scenario; at Check it
   is compile-bound only. Accept (or not) that the binding correctness evidence
   is the off-Check privileged job's green, which Check does not observe.

4. **(T5) Check-bar judgment.** Ratify the test-design split (compile-binding
   scenario + host-independent unit tests) as the correct Check bar for this
   tier.

5. **(V) Fitness-to-purpose.** Final human sign-off that the harness honours the
   proposal 0005 §13.2 Tier-1 mandate.

## Advisory notes (non-gating)

- `run_scenario` exports `WYRD_TIER1_DM_SECTORS` (`disk_faults.rs:894`) but the
  scenario never reads it (it reads `VICTIM_ROOT`/`HEALTHY_ROOT`/`DM_NAME`/
  `DM_ERROR_TABLE`/`REPORT` only — `tier1_disk_faults.rs:128-132`). Dead env wiring,
  harmless.
- The off-Check scenario's "reads without error during repair" assertion
  (`tier1_disk_faults.rs:431-435`) depends on the production read path tolerating
  a block-layer error on the victim fragment and decoding from the `k=2`
  survivors. That behaviour runs only in the privileged job — surface it as a
  thing the privileged Tier-1 CI run must actually demonstrate, not assume.
- CI artifact path `target/tier1-disk-faults/campaign-report.txt`
  (`tier1-disk-faults.yml:73`) matches `run()`'s `scratch.join("campaign-report.txt")`
  (`disk_faults.rs:387-388,928`) — consistent.
- Scope/conflict note from the brief (`brief.md:54-60`): 195 and 196 collide on
  `faults.rs`/`main.rs`; this is a sequencing concern for integration, not a
  Check correctness defect.
