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

Issue 468 adds a simulated-FoundationDB DST story for commit ambiguity while keeping `libfdb_c` out of the simulator.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The owed behavior is explicit: add a simulated-FDB `MetadataStore`, exercise 1021 commit ambiguity in DST, and guard DST from FDB linkage (`brief.md:9`). |
| C2 Reproduction (red pre-fix) | PASS | A clean `HEAD` archive with only `no_fdb_linkage.rs` copied in failed on the missing `SimFdbMetadataStore` seam, the intended structural red (`crates/dst/tests/no_fdb_linkage.rs:486`). |
| C3 Change | PASS | The patch adds the simulated-FDB store and `MetadataStore` impl, plus the shared conformance leg and linkage/ambiguity tests (`crates/dst/tests/support/mod.rs:556`, `crates/dst/tests/support/mod.rs:735`, `crates/dst/tests/conformance.rs:59`). |
| C4 Verification (red→green) | PASS | Re-ran `cargo xtask ci` green; separately re-ran `cargo test -p wyrd-dst --test no_fdb_linkage` green and reproduced the base red described above (`crates/dst/tests/commit_ambiguity.rs:323`, `crates/dst/tests/no_fdb_linkage.rs:466`). |
| C5 Causal adequacy | PASS | The fix models the unavailable 1021 failure shape directly and proves the settling read is load-bearing with demonstrated-red observers rather than a capability probe or load-time guard (`crates/dst/tests/commit_ambiguity.rs:389`, `crates/dst/tests/commit_ambiguity.rs:413`). |
| T1 Structure | PASS | The model is a second parametrization beside the existing test support skeleton, not production FDB code or a new framework (`crates/dst/tests/support/mod.rs:393`, `crates/dst/tests/support/mod.rs:556`). |
| T2 Shape | PASS | The behavioral ambiguity test is madsim-gated while the linkage discriminator remains bare cargo-test runnable, matching the split verification posture (`crates/dst/tests/commit_ambiguity.rs:55`, `crates/dst/tests/no_fdb_linkage.rs:63`). |
| T3 Runtime | PASS | Runtime evidence exercised both cargo contexts: madsim `wyrd-dst` ran 9/9 commit-ambiguity tests and bare `no_fdb_linkage` ran 9/9, including graph and manifest scanners (`crates/dst/tests/no_fdb_linkage.rs:216`, `crates/dst/tests/no_fdb_linkage.rs:450`). |
| T4 Contribution | PASS | The tests include non-vacuity checks for landed/not-landed ambiguity, blind pending puts, torn apply, and 1031-vs-1021 behavior, so the contribution is load-bearing (`crates/dst/tests/commit_ambiguity.rs:343`, `crates/dst/tests/commit_ambiguity.rs:563`, `crates/dst/tests/commit_ambiguity.rs:704`). |
| T5 Judgment | NEEDS-HUMAN | Closed/rejected PR prior-art could not be mechanically confirmed from this sandbox; local merged history by affected path showed no FDB hits under `crates/dst`, but PR-state sign-off still matters (`brief.md:151`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Human sign-off owes the product judgment that this simulator model is an adequate substitute for a real topology that cannot deterministically emit 1021 (`brief.md:39`). |

### Advisory — adversary

# Adversarial review — issue 468 / metadata-fdb-dst-story (iteration 2)

Skeptic's pass. I re-ran the red→green reasoning against the target source, tried to
break each demonstrated red, and attacked the linkage guard and the C4-verify verdict.
Findings below; two are NEEDS-HUMAN, one is an honest "could not refute."

## Attacks that landed

- **NEEDS-HUMAN — the graph-linkage invariant is vacuous for this manifest's own house
  style under the bare (non-madsim) invocation.** `the_dst_dependency_graph_links_no_libfdb_c`
  (`crates/dst/tests/no_fdb_linkage.rs:983-995`) delegates target-cfg resolution to
  `cargo tree` (`:874-894`). `crates/dst/Cargo.toml:55` puts the manifest's *renamed* deps
  (`tonic`, `etcd-client`, `tokio`, lines 56/66/68) under `[target.'cfg(madsim)'.dev-dependencies]`
  — the exact rename form the guard exists to catch, in the exact section a future FDB dep
  would be added in house style. `cargo tree` only resolves that section when `--cfg madsim`
  is in the ambient RUSTFLAGS. Under `cargo xtask ci`/`run_dst()` it is, so C4-ci scans it;
  but the file is deliberately **not** `#![cfg(madsim)]`-gated and also runs under
  `run-verify.sh`'s bare `cargo test -p wyrd-dst --test no_fdb_linkage` (no `--cfg madsim`),
  where `cargo tree` evaluates `cfg(madsim)=false` and **omits the entire section**. The
  doc's claim that "under a bare `cargo test` it is the ordinary one … Both must be clean"
  (`no_fdb_linkage.rs:799-801`) treats the two graphs as symmetric; they are not — the bare
  graph is strictly smaller and blind to the `[target.'cfg(madsim)']` section. Concrete
  failing case: an FDB dep added as `fdb = { package = "foundationdb", … }` under
  `[target.'cfg(madsim)'.dev-dependencies]` links `libfdb_c` into every DST binary yet
  passes this test in the bare invocation. (It is still caught by the *text* policy scan
  `the_dst_manifest_declares_no_fdb_dependency`/`scan_line`, and by the graph test under
  C4-ci — so goal (c) is not wholly unenforced — but the invariant this file's *title*
  claims is not what runs under `run-verify.sh`.)

- **NEEDS-HUMAN — no planted red covers the target-cfg-gated rename form, so the guard's
  ability to catch it under `--cfg madsim` is assumed, not demonstrated.**
  `the_graph_scanner_is_red_on_a_planted_rename_and_transitive_edge`
  (`no_fdb_linkage.rs:997-1053`) plants the rename dep under a **plain** `[dev-dependencies]`
  (`:1037-1040`), never under `[target.'cfg(madsim)'.dev-dependencies]`. So the two
  "demonstrated reds" the doc says carry "both blind spots" (`:796-797`) prove the *scanner*
  parses rename/transitive output, but leave the load-bearing behavior — that `cargo tree`
  actually surfaces a `cfg(madsim)`-gated FDB node into that output — resting on the
  planted-red discipline's own forbidden assumption ("resting green on non-existence"). This
  is precisely the manifest section (`Cargo.toml:55`) where the real risk lives.

## Attacks I attempted and could not make land (reported as a signal, not a pass)

- **Tried to break the CAS red→green** (`commit_ambiguity.rs:183-309`,
  `assuming_an_ambiguous_commit_did_not_land_fails_the_sweep` `:395-405`). The nemesis
  strikes only *after* `preconditions_hold` (`support/mod.rs:1674` region), so once one
  struck commit lands (version→2) every later writer's `require(prior)` fails and returns
  `Ok(Conflict)` un-struck; a landed commit is therefore always returned as `Err`, so
  `AssumeNotCommitted` (winners=0) always mismatches the `version-bump == winners`
  assertion on any landed seed, and `the_settling_re_read_covers_both_halves_of_the_ambiguity_space`
  (`:349-383`) guarantees ≥1 landed seed. The `should_panic` is genuine, not a
  tautology. Could not refute.

- **Tried to show the torn-inode red is still a tautology** (carry-forward item 4).
  It is no longer `x==x`: `assert_batch_applied_whole` (`commit_ambiguity.rs:131-149`) counts
  `orphan:` records from the store's own `scan` against `orphan_records_for(prior)` (=9 for
  the `b"v0"` RS{6,3} prior), independent of the observer. `TornApplyOnAmbiguity::apply_landed`
  drops all but `puts.first()` (the inode put — confirmed the batch order at
  `crates/core/src/metadata.rs:474-488`: inode `.put` first, then orphan puts), so a landed
  torn seed bumps the version to 2 (winner counted correctly, chunk-map whole) yet holds 0
  orphans where 9 were staged → the "must apply its batch WHOLE" panic fires. Genuine
  demonstrated red on the atomicity clause. Could not refute.

- **Tried to break the 1031 leg** (`commit_ambiguity.rs:614-716`). `may_still_commit`
  faithfully mirrors production (`crates/metadata-fdb/src/lib.rs:246-249`: true iff 1031).
  `settle_in_flight(false)` draws its coin only when `in_flight` is non-empty
  (`support/mod.rs:1546-1549`), so 1021 runs and the nemesis-free conformance suite are
  bit-for-bit unperturbed; the `treating_a_timed_out_commit_like_1021` red panics exactly on
  the seeds `a_timed_out_commit_may_still_land_after_the_settling_re_read` proves exist, and
  both observers draw the identical RNG stream (observer choice adds no rng). Consistent.
  Could not refute.

- **Tried to find a blind-batch escape as `Ok(Conflict)`** (`support/mod.rs` Conflict arm).
  `preconditions_hold` over an empty list is vacuously true (`support/mod.rs:176-180`), so the
  `assert!(conditional)` guard is unreachable-but-correct, matching production's
  "blind batch never `Conflict`" contract (`crates/metadata-fdb/src/lib.rs:25-31`,
  `classify_commit_error` order at `:212-218`). Could not refute.

## The verdict I distrust most

- **NEEDS-HUMAN (re-raise of the prior iteration's §10 candidate) — C4-verify's row
  overstates what it verified.** `check-gates.json` C4-verify reads *"red without the fix,
  green with it."* But `run-verify.sh` runs a bare `cargo test` (no `--cfg madsim`), under
  which `commit_ambiguity.rs` (`#![cfg(madsim)]`, `:61`) compiles to **nothing** — so the
  behavioural ambiguity property (criteria 1–3) never executes in the per-fix verify. The
  only discriminator that goes red pre-fix is the **text-scan** structural assertion
  `the_dst_support_module_declares_the_simulated_fdb_store` (`no_fdb_linkage.rs:1232-1261`),
  which greps `support/mod.rs` for strings. The needle set is now richer (const values,
  impl header, `may_still_commit`), which raises the bar, but it remains a *structural* red:
  the row would read identically for a model whose `commit_optimistic` body was wrong, as
  long as those six strings were present. Only C4-ci is real evidence for criteria 1–3. The
  brief is candid about this (`brief.md:120-124`); the gates table is not.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T5 Judgment — Closed/rejected PR prior-art could not be mechanically confirmed from this sandbox; local merged history by affected path showed no FDB hits under `crates/dst`, but PR-state sign-off still matters (`brief.md:151`).
- [ ] Validation — fitness-to-purpose — Human sign-off owes the product judgment that this simulator model is an adequate substitute for a real topology that cannot deterministically emit 1021 (`brief.md:39`).

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
- Iteration delta (if iterating): Rejected on the adversarial reviewer's findings against goal (c) — the libfdb_c purity guard. Rebuild against the same brief; do NOT re-plan. What to fix: 1. The graph-linkage invariant is asymmetric between invocations. Under bare `run-verify.sh` (`cargo test`, no `--cfg madsim`), `cargo tree` omits the entire `[target.'cfg(madsim)'.dev-dependencies]` section — which is exactly the section, and exactly the rename form (`fdb = { package = "foundationdb", … }`), where a real FDB dep would be added in this manifest's house style (crates/dst/Cargo.toml:55). So the invariant the file's title claims is NOT what runs under verify: the bare graph is strictly smaller and blind to the madsim section. Make the guard scan the graph the FDB risk actually lives in (resolve with `--cfg madsim`, or otherwise cover the target-gated section) under BOTH invocations, so goal (c)'s "mechanically guaranteed" claim holds where it runs. 2. No planted red covers the target-cfg-gated rename form. The demonstrated red (no_fdb_linkage.rs:997-1053) plants the rename dep under a PLAIN [dev-dependencies], never under [target.'cfg(madsim)'.dev-dependencies]. So the load-bearing behavior — that the scan actually surfaces a cfg(madsim)- gated FDB node — is assumed, not demonstrated. Plant the red under the madsim-gated section so the guard's ability to catch the real-risk shape is proven, not resting on non-existence. Not blocking, but keep disclosed at the next sign-off: C4-verify's gates row ("red without the fix, green with it") overstates what it verified — under bare verify the behavioural ambiguity property compiles to nothing, so only a structural string-scan goes red. C4-ci is the real evidence. Already a §10 Act candidate; the gates-row wording is a harness matter, not this bundle's to fix.
- By / date: Eduard Ralph / 2026-07-10

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
