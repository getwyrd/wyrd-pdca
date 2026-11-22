Review of #1014’s orphan-identity migration gate and large-object retirement routing: the routing regression is independently red→green, but three reproduced defects undermine safe reclamation and cleanup progress.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance criteria distinguish eventual reclamation from premature deletion and require restore-aware completion; they give executable counterexamples for the intended invariant (`brief.md:36`, `brief.md:95`, `brief.md:262`). |
| C2 Reproduction (red pre-fix) | PASS | With the patch stashed, an independently written test using existing APIs executes and fails because the 100-chunk unlink installs no retirement obligation (`review-base-red.log:10`; `crates/core/src/metadata.rs:2196`). |
| C3 Change | PASS | The submitted changes stay within migration, routing, fixture adaptation and living documentation; reverse-applying the supplied patch checks cleanly against the restored target (`crates/custodian/src/reconciliation.rs:174`; `docs/design/architecture/05-building-block-view.md:258`). |
| C4 Verification (red→green) | PASS | The same routing discriminator passes after stash-pop, and the existing core/custodian suite passes 1,112 tests; this establishes the routing change, while the additional safety failures are recorded below (`review-base-green.log:8`; `review-green.log:163`; `review-target-tests.log`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Prevent the migration from certifying unsafe restored marks or erasing published-part unlink evidence—both sequences violate the required lifetime guarantee in executable counterexamples (`crates/custodian/src/gc.rs:696`; `crates/custodian/src/orphan_identity.rs:563`; `review-counterexamples.log:6`). |
| T1 Structure | PASS | Production routing and cleanup remain on the existing metadata and reconciliation seams, with the deployed retirement leg retained before cleanup (`crates/core/src/metadata.rs:2261`; `crates/custodian/src/reconciliation.rs:158`). |
| T2 Shape | PASS | Marker decoding rejects noncanonical representations, absent fields remain omitted, and persisted key ownership and restore validity are documented (`crates/core/src/orphan_identity.rs:92`, `crates/core/src/orphan_identity.rs:400`; `docs/design/architecture/05-building-block-view.md:258`). |
| T3 Runtime | FAIL | Two conflicting drops can require eight keyed reads after reserving seven; the reproduced budget-boundary case panics in the production cleanup (`crates/custodian/src/orphan_identity.rs:142`, `crates/custodian/src/orphan_identity.rs:920`; `review-counterexamples.log:17`). |
| T4 Contribution | NEEDS-HUMAN | Confirm the relevant closed/rejected-work dispositions before sign-off—affected-path history and closed-work searches ran, but bounded search results cannot establish the complete rejected-diff history; the frozen batch review also has three independently confirmed defects awaiting fixes or recorded rejection (`review-prior-art.txt:1`; `gate-logs/T4-batch-review.log:10`; `AGENTS.md:221`). |
| T5 Judgment | NEEDS-HUMAN [impl] | Correct the retry budget and retain regressions combining damaged-session recovery, preserved reclaiming marks with routed deletion, and two conflicts at the read limit—the current 15 tests pass despite all three failures (`crates/custodian/tests/retirement_routing.rs:1066`; `crates/custodian/src/orphan_identity.rs:863`; `review-counterexamples.log:28`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the corrected migration is ready for restored production metadata and operational rollout—reader grace must survive reopening the gate, and the documented restore procedure must run before gateways return (`docs/design/architecture/06-runtime-view.md:96`; `review-counterexamples.log:14`). |

Source citations above and below resolve under `$PDCA_TARGET` (`./target`); review logs and probe files resolve in this report’s directory. The target is the disposable snapshot based on `3c600b657ccb7e54a4a60c72b220b7bf1d33ad4a`. The patch was restored after the independent base run, and `git -C "$PDCA_TARGET" apply --reverse --check ../patch.diff` succeeds. No production source was edited by the reviewer.

**Three actionable findings are reproduced against the applied patch.** The frozen review’s five entries collapse to these three defects; its three budget reports describe the same failure.

1. **[P1] A preserved reclaiming mark permits deletion before the new grace window.** Cleanup leaves every reclaiming mark untouched (`crates/custodian/src/orphan_identity.rs:934`) and can nevertheless certify completion (`:266`). After a restored live object is deleted through the new obligation route, GC resumes the old reclaiming mark directly, without checking the new retirement or grace (`crates/custodian/src/gc.rs:696`). The new obligation does not restore the committed-reference protection that the delete removed.

   The counterexample creates a 100-chunk RS(6,3) object, plants one ancient reclaiming mark on a live fragment, runs the production post-restore pass and cleanup, then calls production `unlink`. Before any drain or elapsed new grace, one GC-only pass deletes that fragment (`review-counterexamples.log:14`). This can truncate a reader holding the just-deleted generation. The existing F4 test proves preservation, but stops before this subsequent delete (`crates/custodian/tests/retirement_routing.rs:1066`). Preserve the required mark semantics while preventing gate reopening/routing from authorizing this premature reclaim; add the combined restore→cleanup→delete→GC regression.

2. **[P1] An unreadable completed session loses legitimate unlink evidence.** A session decode failure becomes `Phase::Unknown` (`crates/custodian/src/orphan_identity.rs:540`), which is classified as staged (`:563`). Publication obligations are examined only for a successfully decoded completed session (`:543`), and the final verification again treats decode failure as live (`:1053`). An unreadable value does not prove that its parts were never published.

   The counterexample publishes a part through production Complete, freezes its records obligation, deletes the object inline, then damages the completed session value. Cleanup writes a complete marker and deletes all six unlink marks (`review-counterexamples.log:6`). After restoring the original session bytes and draining its records obligation, the parts are gone and GC leaves the fragments permanently unmarked and unreclaimed (`review-counterexamples.log:9`). This is the F3 trap under a damaged session. Preserve the marks and block certification when publication history cannot be established; an exact-byte precondition on unreadable session bytes does not make that history known.

3. **[P2] Retry accounting can panic and stop migration progress.** `READS_PER_MARK` reserves `1 + 2 * 3 = 7` reads (`crates/custodian/src/orphan_identity.rs:142`), but `judge` repeats the retirement-token lookup on each attempt (`:897`, `:938`). A staged or segmented drop can therefore consume two lots of one token lookup, two namer reads and one conflict reread: eight reads. The final unchecked decrement underflows (`:920`).

   The counterexample places 4,089 structured marks requiring one lookup each ahead of a staged mark, leaving seven reads. Its metadata wrapper refuses both drop commits on that final mark. The unmodified production function panics with `attempt to subtract with overflow` (`review-counterexamples.log:17`). Charge the complete worst case before admitting a mark, or stop safely before each read, and keep a regression at the actual pass limit.

**Independent execution establishes both the successful routing change and the failing safety cases.** The probes live outside the target source tree in `pdca-reviewer-1014-probes/`; the safety probes reuse the patch’s real-redb/placement-aware test rig and add the missing sequences.

```sh
# From this report's directory, with the patch applied:
CARGO_TARGET_DIR="$PDCA_TARGET/target" cargo test --offline \
  --manifest-path pdca-reviewer-1014-probes/Cargo.toml \
  --test counterexamples reviewer_ -- --nocapture

CARGO_TARGET_DIR="$PDCA_TARGET/target" cargo test --offline \
  --manifest-path pdca-reviewer-1014-probes/Cargo.toml \
  --test base_discriminator -- --nocapture
```

The first command runs three tests and currently fails all three for the reasons above. The second currently passes; it failed its obligation assertion with the patch stashed, rather than failing compilation. That discriminator checks routing after production reconciliation on an empty store; it does not alone prove reclamation. The submitted 15-test suite separately exercises production cleanup/drain, Complete, unlink and overwrite with fragment-presence and fragment-absence assertions and passes independently (`review-green.log:145`).

`cargo test -p wyrd-core -p wyrd-custodian` passed 1,112 tests across 68 result groups, with one explicitly ignored privileged Tier-1 disk-fault test (`review-target-tests.log`). `cargo clippy -p wyrd-core -p wyrd-custodian --all-targets -- -D warnings`, `cargo fmt --all -- --check`, `typos`, docs lint, and docs render/link audit all passed (`review-clippy.log:165`, `review-fmt.log`, `review-typos.log`, `review-docs-lint.log:1`, `review-docs-render.log:3`). The brief’s `typos` and `docs-renderer` dependencies were actually exercised. Tier-1 disk faults and Tier-2 deployed kill/reconstruct remain follow-up observation choices for this custodian durability change, not claimed evidence from this run.

**Frozen gate evidence has specific limits; no missing wrapper is treated as a defect.** The instance-root wrapper paths are outside this leaf’s inputs. All seven named logs were readable; the full CI, mutation campaign, host feature checks and batch-review wrappers were not independently repeated.

| Gate | Adjudication | Evidence |
|------|--------------|----------|
| C4-ci | PASS, from frozen output; relevant tests and scanners also rerun | The log runs fmt, clippy, build, tests, dependency checks and DST and ends with all checks passed (`gate-logs/C4-ci.log:5025`). |
| C4-verify | Original result UNVERIFIABLE; routing red→green independently recovered | Its green leg passes 15 tests; its reverted leg fails imports before running tests (`gate-logs/C4-verify.log:10`, `:16`). The reviewer’s existing-API discriminator supplies a real assertion failure and subsequent success (`review-base-red.log:10`; `review-base-green.log:8`). |
| C4-diff-cov | Coverage unmeasured; base-selection caveat | The wrapper cannot apply the stacked patch to `origin/main` (`gate-logs/C4-diff-cov.log:10`). The supplied target applies and compiles; this does not demonstrate a patch application or compilation defect. |
| C5-mutants | Recorded survivors require interpretation | The log reports 243 mutants: 100 missed, 118 caught, 25 unviable (`gate-logs/C5-mutants.log:113`). Survivors include budget arithmetic and publication classification (`:50`, `:66`); the three executed counterexamples provide stronger defect evidence than treating every survivor as a bug. The log does not identify all eight brief-required named negations as separately executed. |
| T4-batch-review | FAIL, three distinct findings confirmed | Five reports include three duplicates of the budget bug; the remaining two and that budget bug reproduce independently (`gate-logs/T4-batch-review.log:10`; `review-counterexamples.log:28`). |
| T4-contribution | N/A | Contribution artifacts are absent by design; their substantive audit re-runs at publish (`gate-logs/T4-contribution.log:10`). No human escalation is owed for this deferral. |
| host-tikv | PASS, from frozen output | The captured TiKV crate and server feature clippy commands both finish successfully; this proves compilation, not a live TiKV deployment (`gate-logs/host-tikv.log:2`, `:211`). |

**Prior-art searching ran by all 28 affected paths, with a remaining completeness decision.** `review-prior-art.json` records each path’s main-branch commit lookup and closed issue/PR search; `review-prior-art.txt` is the compact index. The queries fetched up to five recent commits and ten closed matches per path. I also read the closed predecessor issues [#636](https://github.com/getwyrd/wyrd/issues/636) and [#637](https://github.com/getwyrd/wyrd/issues/637), consistent with the brief’s rejected routing-without-drain and published-part cleanup history (`brief.md:211`). These bounded, text-based searches do not certify every closed/unmerged patch’s changed-file list; T4 leaves that historical disposition with the human rather than asserting exhaustive absence.

The standing rubric was applied. The explicit seeded-DST deferral to #660 remains settled (`crates/custodian/src/orphan_identity.rs:99`; `AGENTS.md:215`). The migration gate is a specified persisted-state precondition, not a capability probe concealing an eager load-time cause. No DCO finding is inferred from commit inspection. The findings concern required lifecycle safety and progress within this patch’s scope.

<!-- pdca:leaf-complete -->
