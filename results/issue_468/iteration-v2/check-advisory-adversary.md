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
