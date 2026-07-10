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
