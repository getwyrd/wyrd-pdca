Review of #814: rebuild committed multipart chunks under the Open-session fence; one confirmed repair-progress defect remains.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief defines observable rebuild, losing-race, deadline, drain-fence and seeded-race outcomes, with explicit exclusions; `brief.md:24`, `brief.md:100`, `brief.md:117`. |
| C2 Reproduction (red pre-fix) | PASS | Stashing the production changes while retaining the new test reproduced the missing rebuild: 18 assertion failures, two passes, no compilation failure; `reviewer-red.log:165`, `crates/custodian/tests/staged_repair.rs:815`. |
| C3 Change | PASS | The change addresses the specified staged lifecycle and updates its living architecture description without adding context fields or changing the committed repair operation; `crates/custodian/src/reconstruction.rs:441`, `docs/design/architecture/06-runtime-view.md:82`. |
| C4 Verification (red→green) | PASS | Restoring the patch passed all 20 new tests, 51 existing tests and both new DST tests across 50 seeds; full CI and coverage are supported by frozen logs, subject to the local rustfmt limitation below; `reviewer-green.log:90`, `reviewer-dst.log:248`, `gate-logs/C4-ci.log:3708`. |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Fix the shared pre-mark timing across multiple writes and add a latency regression — a successful 15-second first write prevents the second from ever being attempted on repeated passes, leaving the original redundancy deficit; `crates/custodian/src/reconstruction/staged.rs:636`, `reviewer-slow-writes.log:13`. |
| T1 Structure | PASS | The staged repair uses the existing metadata/chunk-store seams and a shared staged read, preserving source-before-destination ordering and avoiding a second inconsistent snapshot; `crates/custodian/src/gc.rs:1536`, `crates/custodian/src/reconstruction.rs:237`. |
| T2 Shape | PASS | Adoption preserves non-placement part fields and pins the exact session, part and mark bytes; no persisted field or public API shape is introduced; `crates/custodian/src/reconstruction/staged.rs:667`, `crates/custodian/src/reconstruction/staged.rs:732`, `crates/custodian/tests/staged_repair.rs:829`. |
| T3 Runtime | FAIL | F1 leaves a repairable RS(2,2) chunk degraded indefinitely while every pass reports Satisfied; four independent passes reproduced unchanged placement and a queued obligation; `crates/custodian/src/reconstruction/staged.rs:637`, `crates/custodian/src/reconstruction.rs:506`, `reviewer-slow-writes.log:13`. |
| T4 Contribution | FAIL | The batch review's slow-write finding is independently confirmed and unresolved; its two late-publication findings are outside #814's explicit scope, while the contribution-artifact audit is N/A until publish; `gate-logs/T4-batch-review.log:10`, `brief.md:120`, `gate-logs/T4-contribution.log:10`. |
| T5 Judgment | PASS | The review preserves the agreed split and exclusions, checks all eight affected paths against merged history and all 16 closed/unmerged PRs, and grounds the remaining defect in execution rather than mutation counts; `brief.md:145`, `reviewer-prior-art.json:2`, `reviewer-slow-writes.log:13`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether staged-upload recovery meets the intended operational recovery expectations after F1 is fixed — automated seam/DST evidence does not supply deployment fitness sign-off; consider Tier-1 disk-fault and Tier-2 kill/reconstruct follow-up under the standing rubric; `AGENTS.md:78`, `brief.md:79`. |

F1 — **FAIL, high priority: successful slow writes can prevent staged reconstruction from making any progress.** All destinations receive one pre-mark timestamp before the write loop. The loop awaits each write before checking the next destination's pre-mark age. With two missing fragments moving to new servers, the first write can legitimately succeed after 15 seconds, inside the 30-second write window, yet exceed the second destination's 10-second authorization window. The function returns before the adoption CAS, so the next pass still reads the original placement and repeats the same first write. The timing constants are at `crates/custodian/src/gc.rs:202` and `crates/custodian/src/gc.rs:218`; the common stamp, sequential await and later adoption are at `crates/custodian/src/reconstruction/staged.rs:606`, `crates/custodian/src/reconstruction/staged.rs:641` and `crates/custodian/src/reconstruction/staged.rs:667`.

The independent scratch probe reuses the supplied deadline-enforcing doubles, with RS(2,2), survivors on servers 0/1, lost fragments on 2/3, and free destinations 4/5. A stored-write hook advances the shared ManualClock by 15 seconds. Across four passes it observed:

```text
pass=1 outcome=Satisfied now=25000 placement=[0, 1, 2, 3] writes=1 queued=true
pass=2 outcome=Satisfied now=40000 placement=[0, 1, 2, 3] writes=2 queued=true
pass=3 outcome=Satisfied now=55000 placement=[0, 1, 2, 3] writes=3 queued=true
pass=4 outcome=Satisfied now=70000 placement=[0, 1, 2, 3] writes=4 queued=true
```

Every written fragment remained named or marked in this probe; the demonstrated defect is failure to restore redundancy, accompanied by a misleading pass result. Preserve the stale-mark prohibition while arranging authorization/dispatch or fresh marking so that one successful write does not consume another destination's authorization window. Add a regression that combines multiple losses with successful write latency. The supplied two-loss test uses immediate writes, and its stale-mark test has only one missing fragment (`crates/custodian/tests/staged_repair.rs:1096`, `crates/custodian/tests/staged_repair.rs:1284`). Reproduce from this review directory:

```sh
CARGO_TARGET_DIR="$PWD/reviewer-build" cargo test --offline \
  --manifest-path pdca-reviewer-814-probe/Cargo.toml \
  --test slow_writes reviewer_two_missing -- --nocapture
```

The probe is at `pdca-reviewer-814-probe/slow_writes.rs:1668`; its failed assertion is at line 1688. It calls the unchanged target production code. No fix was applied to the reviewed source.

The asserted red→green evidence reproduced independently. The disposable target was readable and matched the patch; its base was stashed, the new test retained, and the stash restored before verification. The restored tree passed 15 reconstruction tests, 36 staged-protection tests and 20 staged-repair tests (`reviewer-green.log:22`, `reviewer-green.log:64`, `reviewer-green.log:90`). With `RUSTFLAGS="--cfg madsim" MADSIM_TEST_NUM=50`, both new staged-replace DST tests passed, including the assertion that all four fence positions were reached (`reviewer-dst.log:245`, `crates/dst/tests/custodian.rs:4960`). Final `git apply --reverse --check ../patch.diff` and `git diff --check` passed; the target retains the supplied patch.

Every frozen gate log was available. The following records distinguish independent reruns from frozen evidence; C4's PASS describes the specified regression checks, not the additional failing F1 probe.

| Gate | Disposition | Evidence |
|------|-------------|----------|
| C4-verify | PASS, independently reproduced | Base: 18 assertion failures; restored patch: 20/20 new tests pass; `reviewer-red.log:165`, `reviewer-green.log:90`. |
| C4-ci | PASS in frozen evidence; local host caveat | Frozen log ends with all checks passed and includes DST; the independent run passed typos, docs lint, 99-page render/link audit, gitlink and unsafe guards, then stopped because cargo-fmt is absent; `gate-logs/C4-ci.log:3708`, `reviewer-ci.log:19`. The statics scanner also passed separately (`reviewer-statics.log:3`). |
| C4-diff-cov | PASS in frozen evidence | 444/467 instrumentable changed lines executed, 95.1%; 690 other changed production lines were unscored, not proved covered; `gate-logs/C4-diff-cov.log:753`, `gate-logs/C4-diff-cov.log:789`. |
| C5-mutants | FAIL, advisory scanner result retained | 83 mutants: 8 missed, 26 caught, 48 unviable, 1 timeout; the log does not establish a production defect for each survivor; `gate-logs/C5-mutants.log:13`, `gate-logs/C5-mutants.log:22`. |
| T4-batch-review | FAIL; one confirmed finding remains | F1 is independently reproduced. The two remaining entries both concern late publication after GC has reclaimed evidence, explicitly excluded from this #814 slice by `brief.md:120`; their underlying deadline-contract context is #638 and `crates/traits/src/lib.rs:871`. They are declined for this slice, not claimed fixed; `gate-logs/T4-batch-review.log:10`. |
| T4-contribution | N/A | PR/commit contribution artifacts are drafted after Check; their substantive audit must rerun at publish; `gate-logs/T4-contribution.log:10`. |
| host-tikv | PASS in frozen evidence | The log shows the TiKV crate and server feature selections compiled under clippy; this is compile evidence, not a live TiKV integration claim; `gate-logs/host-tikv.log:2`, `gate-logs/host-tikv.log:209`. |

The brief's external dependencies, typos and docs-renderer, were exercised both in the frozen CI run and independently (`brief.md:128`, `reviewer-ci.log:2`, `reviewer-ci.log:10`). Missing local rustfmt does not invalidate the captured full-CI result or constitute a patch defect. Instance-scoped coverage/mutation/review wrappers were adjudicated from their supplied logs; no missing-wrapper finding is raised. No additional capability probe or eager-load workaround was found: session, mark, drain and deadline checks enforce the protocol's stated preconditions.

The mutation misses do not justify eight new defect claims. Four change the final field-equality conjunctions in `repointed_part`; successful normal encodings satisfy every equality, so those survivors do not demonstrate that this patch corrupts records. The other misses concern a held-chunk guard, first-reference filtering and the reachable-versus-lost arithmetic; the log identifies test-strength limits, and does not make the scanner green. F1 supplies the independently demonstrated causal/test gap for rebuild.

The prior-art check ran by all eight affected paths against GitHub's main-branch commit history and every closed, unmerged PR's file list. The only closed/unmerged overlaps were #647 (segmented chunk maps) and #336 (DST global-state policy), not a staged re-place implementation; neither new staged file had main-branch history. Results and issue #637/#663 split context are preserved in `reviewer-prior-art.json`. The prior rejected iterations' dispositions remain as recorded in `brief.md:145`; this review does not reopen the accepted split or its excluded late-write residual.

Source citations such as `crates/...`, `docs/...` and `AGENTS.md` are grounded in the supplied `$PDCA_TARGET` (`target/`); review and gate-log citations refer to this bundle. No target-state caveat was needed. This report is advisory and does not override deterministic gate decisions.
