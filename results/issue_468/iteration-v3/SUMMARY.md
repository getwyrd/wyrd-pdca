# Result — issue 468 / metadata-fdb-dst-story

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: Hold `metadata-fdb` — an FFI backend that can never run inside the
- Success criterion: `cargo xtask dst` is green with the new legs, and
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope: Give the FFI backend a DST story of the same strength as the TiKV backend's.

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

Review of issue 468 / metadata-fdb-dst-story: add a simulated-FDB DST model for commit ambiguity and keep `libfdb_c` out of DST.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The owed behavior is explicit: model FDB 1021 ambiguity in DST and guard the DST graph from `libfdb_c`; the success criteria name shared conformance, ambiguity sweep, demonstrated red, and linkage guard in `brief.md:16`. |
| C2 Reproduction (red pre-fix) | PASS | The discriminator is load-bearing: in a clean `HEAD` export with only the added tests copied in, `cargo test -p wyrd-dst --test commit_ambiguity --test no_fdb_linkage` failed on the missing `SimFdbMetadataStore`, matching `crates/dst/tests/no_fdb_linkage.rs:557`. |
| C3 Change | PASS | The patch addresses the specified surfaces: shared conformance now runs `SimFdbMetadataStore` through `run_all`, and the model implements the ambiguity-capable `MetadataStore` path at `crates/dst/tests/conformance.rs:59` and `crates/dst/tests/support/mod.rs:735`. |
| C4 Verification (red→green) | NEEDS-HUMAN | Full `C4-ci` must be rerun on a host that permits loopback binds: I reproduced red→green and `RUSTFLAGS=--cfg madsim MADSIM_TEST_NUM=50 cargo test -p wyrd-dst` passed, but `cargo xtask ci` stopped in unrelated `wyrd-chunkstore-grpc` with `bind loopback: Operation not permitted`; the DST leg itself is wired through `xtask/src/main.rs:1342`. |
| C5 Causal adequacy | PASS | The causal question is whether the FDB-only ambiguity shape is searched rather than guarded around; the model returns a typed ambiguous error after resolver acceptance and the tests require settling re-reads plus demonstrated-red observers at `crates/dst/tests/support/mod.rs:675` and `crates/dst/tests/commit_ambiguity.rs:379`. |
| T1 Structure | PASS | The structural decision is second parametrization, not a parallel framework: the new store lives beside the TiKV support model and reuses the shared conformance entry point at `crates/dst/tests/support/mod.rs:556` and `crates/dst/tests/conformance.rs:59`. |
| T2 Shape | PASS | The shape matches the harness split: `commit_ambiguity` is madsim-only while `no_fdb_linkage` is bare-runnable and carries the structural verify discriminator at `crates/dst/tests/commit_ambiguity.rs:323` and `crates/dst/tests/no_fdb_linkage.rs:557`. |
| T3 Runtime | PASS | The runtime decision is container-free DST coverage: the madsim package test passed locally with 50 seeds, including 9 commit-ambiguity tests, 3 conformance tests, and 9 linkage tests; the graph scan asserts no `foundationdb`/`foundationdb-sys` nodes at `crates/dst/tests/no_fdb_linkage.rs:282`. |
| T4 Contribution | NEEDS-HUMAN | Prior-art coverage needs human confirmation for open/closed PR state: local affected-path history showed only the DST skeleton commit and `git grep -i fdb origin/main -- crates/dst` had no hits, but this sandbox had no PR refs to mechanically verify the `brief.md:151` claim. |
| T5 Judgment | NEEDS-HUMAN | The human must ratify the modeling fidelity because correctness transfer depends on accepting optimistic commit plus seed-selected ambiguity over a `BTreeMap` rather than MVCC; the patch explicitly leaves that sign-off decision at `crates/dst/tests/support/mod.rs:67`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Human sign-off must decide whether this simulated-FDB story is fit for the production FDB risk: the evidence proves the simulator/search and linkage guard, while the broader purpose is the non-mechanical validation item required by the matrix and scoped in `brief.md:39`. |

### Advisory — adversary

# Adversarial review — issue 468 / metadata-fdb-dst-story (iteration 3)

Skeptic's pass. I re-ran the asserted evidence on the target worktree
(`/home/eddie/wyrd/wyrd.pdca-wt-l1`, patch already applied) rather than trusting the
gates table. Cargo 1.96.0 present, so the red→green was reproducible.

## What I attempted to refute and could not

- **The graph-linkage mechanism (the iteration-2 carry-forward's whole objection).** The
  claim is that forcing `RUSTFLAGS=--cfg madsim` makes `cargo tree` resolve
  `[target.'cfg(madsim)'.dev-dependencies]` (`crates/dst/Cargo.toml:55`). I tested this
  directly: bare `cargo tree` omits `madsim-tonic`/`madsim-etcd-client` as top-level nodes;
  with `RUSTFLAGS=--cfg madsim` they appear. So `cargo tree` **does** honour the custom cfg
  for target-section resolution, and `no_fdb_linkage.rs:919` (`cargo_tree`) scans the graph
  the real risk lives in. `the_dst_dependency_graph_links_no_libfdb_c`,
  `the_graph_scanner_is_red_on_a_cfg_madsim_gated_rename_and_transitive_edge` and
  `the_graph_scanner_is_red_on_the_real_fdb_backend_with_its_feature_on` all pass, and the
  planted-red plants the rename form *and* a transitive edge under the madsim-gated section
  (`no_fdb_linkage.rs:1116-1122`). The iteration-2 finding is genuinely addressed.
  - I checked the one way forcing `--cfg madsim` could create a blind spot: it drops any
    ambient flag and any other custom cfg. Enumerated every `[target.'cfg(...)']` in the
    workspace — only `madsim` / `not(madsim)` gate sections. A `cfg(not(madsim))` FDB dep
    would **not** be linked into the real `--cfg madsim` DST build anyway, so excluding it is
    correct, not a miss. The doc's "only custom cfg … is madsim" claim
    (`no_fdb_linkage.rs:901-904`) checks out.
- **The behavioural red→green.** Ran `cargo test -p wyrd-dst --test commit_ambiguity`
  under `--cfg madsim`: 9/9 pass, including all four `#[should_panic]` demonstrated reds,
  each pinned to a *specific* load-bearing message (not "any panic"). The torn-apply red
  (`commit_ambiguity.rs:419`, `FdbFidelity::TornApplyOnAmbiguity`) is real — I traced both
  batch-ordering cases and `assert_batch_applied_whole` (`commit_ambiguity.rs:131`) trips in
  each, so the iteration-1 "torn assertion is a tautology" finding is fixed, not papered
  over. Shared contract leg passes too (`sim_fdb_backend_passes_shared_contract`, 3/3).
- **Fidelity vs. production.** Read `crates/metadata-fdb/src/lib.rs:150-249`. The model's
  `SimCommitUnknownResult { code }` / `may_still_commit` (`support/mod.rs`) mirrors
  `classify::CommitUnknownResult` byte-for-byte; the nemesis is not batch-shape-aware and
  the Conflict arm asserts `conditional`, matching `classify_commit_error`'s ordering
  (`lib.rs:212-218`). Iteration-1 findings #1 (blind-batch ambiguity) and #2 (1031 modelled
  with a code) are both genuinely closed. A blind batch cannot escape as `Ok(Conflict)`, as
  production requires.

## Findings a human should weigh (advisory)

- **NEEDS-HUMAN — the 1031-under-contention path is asserted by no test; criterion 2(ii)
  "exactly one writer won" is demonstrated only for 1021.** `timed_out_commit_over`
  (`crates/dst/tests/commit_ambiguity.rs:608`) drives the `SIM_TRANSACTION_TIMED_OUT`
  nemesis with a **single** writer (`:625`, budget 1, one `plan`). The multi-writer race
  (`ambiguous_cas_settles_over`, `:202` `for i in 0..4`) uses only the `1021` nemesis, and
  `1021` never leaves anything in flight (`in_flight == 0`, asserted at `:239`). So the two
  never combine. Consequently the resolver-rejection branch of `settle_in_flight`
  (`crates/dst/tests/support/mod.rs:539`, `deferred_rejections += 1`) — the code whose doc
  (`support/mod.rs:521-522`) claims *"a deferred CAS that another writer has since beaten is
  rejected, and 'exactly one writer wins' survives the deferral"* — is **dead in every
  test**. Concrete uncovered case: four writers race the version CAS with the `1031` nemesis
  armed; writer A's timed-out batch is left in flight and lands *deferred* after writer B has
  already won and bumped the version. Nothing verifies the model rejects A's stale-precondition
  batch at deferral, i.e. that "exactly one winner" survives a 1031 deferral under contention.
  The property rests on code inspection, exactly the vacuity the demonstrated-red discipline
  is meant to forbid for 1021. Either add a multi-writer 1031 leg with a demonstrated red, or
  narrow the criterion-2 claim to "1021 under contention; 1031 single-writer" at sign-off.

- **Minor (same root) — the 1031/torn observation counters are computed but never asserted,
  so their vacuity guard does not exist.** `deferred_landings`, `deferred_rejections`,
  `resolver_conflicts` and `torn_applies` (`support/mod.rs` `FdbObservations`) are incremented
  but no test reads them (only `ambiguous_commits`/`_blind`/`_conditional` and
  `commits_left_in_flight` at `commit_ambiguity.rs:358` are asserted). The brief's
  anti-vacuity argument ("`Observations` records how often it fired, so a sweep that never
  armed it is visibly vacuous", Impact §Risk) therefore does **not** cover the 1031-deferral
  counters — a sweep in which no deferred landing was ever rejected would look identical to
  one where the branch works.

## Already-tracked, not re-raised

- The `C4-verify` gates row ("red without the fix, green with it") still describes a
  structural `String::contains` scan over `support/mod.rs`
  (`no_fdb_linkage.rs:1324` `the_dst_support_module_declares_the_simulated_fdb_store`), which
  `run-verify.sh` never compiles under `--cfg madsim`. I confirmed this remains the case, but
  both prior carry-forwards already recorded it as a §10 Act / harness-wide matter, not this
  bundle's defect. Not re-filing.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Full `C4-ci` must be rerun on a host that permits loopback binds: I reproduced red→green and `RUSTFLAGS=--cfg madsim MADSIM_TEST_NUM=50 cargo test -p wyrd-dst` passed, but `cargo xtask ci` stopped in unrelated `wyrd-chunkstore-grpc` with `bind loopback: Operation not permitted`; the DST leg itself is wired through `xtask/src/main.rs:1342`.
- [ ] T4 Contribution — Prior-art coverage needs human confirmation for open/closed PR state: local affected-path history showed only the DST skeleton commit and `git grep -i fdb origin/main -- crates/dst` had no hits, but this sandbox had no PR refs to mechanically verify the `brief.md:151` claim.
- [ ] T5 Judgment — The human must ratify the modeling fidelity because correctness transfer depends on accepting optimistic commit plus seed-selected ambiguity over a `BTreeMap` rather than MVCC; the patch explicitly leaves that sign-off decision at `crates/dst/tests/support/mod.rs:67`.
- [ ] Validation — fitness-to-purpose — Human sign-off must decide whether this simulated-FDB story is fit for the production FDB risk: the evidence proves the simulator/search and linkage guard, while the broader purpose is the non-mechanical validation item required by the matrix and scoped in `brief.md:39`.

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
- Iteration delta (if iterating): Rebuild against the same brief — do NOT re-plan. The design proposal, the simulated-FDB model, and the linkage guard all stand: the adversarial pass confirmed the iteration-1 findings (torn-assertion tautology, blind-batch ambiguity, 1031-modelled-with-a-code) and the iteration-2 finding (the linkage guard blind to the `--cfg madsim` section / rename form) are all genuinely closed, and checked the model's fidelity against production `classify_commit_error` as matching. Do not churn any of that. What to fix — the two coverage gaps the adversary landed, both advisory but real: 1. Criterion 2(ii) "exactly one writer won" is demonstrated ONLY for 1021. The multi-writer race (`ambiguous_cas_settles_over`, 4 writers) arms only the 1021 nemesis, and the 1031 leg (`timed_out_commit_over`) drives a SINGLE writer — so the two never combine and the resolver-rejection branch of `settle_in_flight` (`support/mod.rs:539`, `deferred_rejections += 1`), whose doc claims "exactly one writer wins survives the deferral," is dead in every test. Add a multi-writer 1031 leg with a demonstrated red: four writers race the version CAS with the 1031 nemesis armed, writer A's timed-out batch left in flight, landing deferred AFTER writer B has already won and bumped the version; assert the model rejects A's stale-precondition batch at deferral so "exactly one winner" survives a 1031 deferral under contention. Do not instead narrow the criterion — the human chose to close the gap, not scope it down. 2. Same root, minor: the 1031/torn observation counters (`deferred_landings`, `deferred_rejections`, `resolver_conflicts`, `torn_applies` in `FdbObservations`) are incremented but asserted by no test, so the brief's anti-vacuity argument ("Observations records how often it fired") does not actually cover the 1031-deferral counters. Assert the relevant counters in the new leg so a sweep that never armed the deferral branch is visibly vacuous. §6 was not cleared (C4-ci full rerun on a loopback-permitting host, T4 prior art, T5 fidelity ratification, fitness-to-purpose) — carry those forward to the rebuild's sign-off. Note the C4-verify structural-red / gates-row wording remains a harness-wide §10 Act matter, already tracked, NOT this bundle's to fix.
- By / date: Eduard Ralph / 2026-07-10

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
