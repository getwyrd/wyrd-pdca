# Build notes — issue 468 / metadata-fdb-dst-story (iteration 2)

## What this ships

A **second parametrization** of the `#258/#447` simulated-store skeleton — a
`SimFdbMetadataStore` beside `SimTikvMetadataStore` in `crates/dst/tests/support/mod.rs`
— plus the scenario, seed sweeps, and purity guard the brief's Success criterion names.
Test-scope only; nothing under `crates/*/src/` changes; no Cargo dependency added.

Files:
- `crates/dst/tests/support/mod.rs` — the model (`SimFdbMetadataStore`, `FdbFidelity`,
  `FdbObservations`, `SimCommitUnknownResult` with its `code` + `may_still_commit`).
- `crates/dst/tests/conformance.rs` — the third `run_all` parametrization
  (`sim_fdb_backend_passes_shared_contract`), nemesis off.
- `crates/dst/tests/commit_ambiguity.rs` — the behavioural red→green (three legs).
- `crates/dst/tests/no_fdb_linkage.rs` — the purity guard (graph property) + the
  `C4-verify` structural discriminator.

## This is iteration 2 — the four carry-forward defects, and how each is fixed

The v1 sign-off kept the machinery but rejected the *fidelity*: the model diverged from
the production contract it cites, and the purity guard did not guarantee goal (c). Each
fix is proven by a mutation that reverts it and shows the suite goes red (see
"Refuting the test" below).

**(1) Blind batches can now be ambiguous — the `&& conditional` gate is gone.**
Production `classify_commit_error` (`crates/metadata-fdb/src/lib.rs:212-215`, doc `:191-204`)
returns `UnknownResult` for 1021/1031 *before* the `conditional` check — so a blind batch
is exactly as ambiguous as a conditional one. v1 gated the nemesis on `&& conditional`
(old `support/mod.rs:494`), so the four-phase protocol's blind `put_pending` could never
come back ambiguous. Fixed: the strike arm (`commit_optimistic`) draws only on the nemesis
budget, never on batch shape. New leg 2 (`ambiguous_pending_put_over`) drives the Intent
phase under 1021 and asserts a chunk whose fragments are on disk always has a
pending-ledger entry — the thing that keeps the custodian GC from reclaiming it. The
comment at the *Conflict* arm (`support/mod.rs`) documents that `conditional` still gates
1020→`Conflict` only, matching `classify_commit_error:216-218`.

**(2) 1031 is modelled distinctly from 1021.** `SimCommitUnknownResult` now carries
`code: i32` and reproduces `classify::CommitUnknownResult::may_still_commit`
(`crates/metadata-fdb/src/lib.rs:240-249`): after 1021 the transaction is out of flight
(re-read settles it); after 1031 the batch "promises nothing" (`:165`) and may land
*after* the settling re-read. The model implements that: a 1031 strike has three
seed-selected fates (landed / still-in-flight / never-sent), and an in-flight batch lands
on later store traffic through the resolver (so a deferred CAS that another writer beat is
rejected — exactly-one-winner survives deferral). New leg 3 (`timed_out_commit_over`)
drives it, and its demonstrated red `treating_a_timed_out_commit_like_1021_fails_the_sweep`
proves the distinction is load-bearing.

**(3) Goal (c): linkage is now a feature-unified GRAPH property, not a manifest-text
property.** v1's `scan_line` keyed on the text before the first `=`/`.`, so the **rename**
form `fdb = { package = "foundationdb", … }` — the house style at
`crates/dst/Cargo.toml:56,66,68` for `tonic`/`etcd-client`/`tokio` — and a transitive edge
both slipped through. `no_fdb_linkage.rs` now resolves the graph with `cargo tree -e
features` and looks for the `foundationdb` / `foundationdb-sys` **package nodes** (cargo
prints resolved names, so a rename shows as `foundationdb`). Two demonstrated reds, not
one:
  - `the_graph_scanner_is_red_on_a_planted_rename_and_transitive_edge` — a local-path
    fixture carrying *both* blind spots.
  - `the_graph_scanner_is_red_on_the_real_fdb_backend_with_its_feature_on` — the **real**
    `foundationdb 0.10` dep (`Cargo.toml:108`), surfaced by turning on
    `wyrd-metadata-fdb`'s default-off `fdb` feature. This addresses v1's "fixture not
    derived from the real dependency" note — the red now comes from the genuine article at
    the version the workspace pins.
  The old manifest-text scan is *kept* as assertion (2), but re-scoped honestly to a
  **policy** ("the manifest may not even *name* an FDB dep, so nobody flips the feature on
  later") — strictly stronger than the linkage claim as policy, strictly weaker as
  evidence, which is why the graph assertion (1) is the one the file's title claims. The
  manifest scanner also now catches the rename form (`renamed_package`) and quoted
  sections, and `the_dst_manifest_declares_no_fdb_dependency` accepts that `wyrd-metadata-fdb`
  is banned here even though it links nothing by default (the over-reach v1 was faulted
  for is now *intended* and documented as a policy, on the graph-backed assertion doing the
  real linkage work).

  Cost of the rejected alternative (keep the text scan, "just add more cases"): it cannot
  see feature unification or a transitive edge at all — those are properties of the
  resolved graph, which no single-manifest parse reconstructs. A `cargo metadata`/`cargo
  tree` call is ~40 lines here and needs no new crate (cargo is already running the test);
  a text scan that tried to chase transitive edges would have to re-implement the resolver.

**(4) The torn-inode claim now has a real demonstrated red; the tautology is gone.** v1's
`settled.chunk_map == expected` was `x == x` on the landed path. The atomicity check now
reads the **store's own** state, independent of the observer: `commit_chunk_map_superseding`
(`crates/core/src/metadata.rs:474-488`) stages the inode put *and* one `orphan:` record
per fragment of the superseded chunk map in **one** `WriteBatch`, so an atomic apply that
moved the version must also have written those orphan records.
`assert_batch_applied_whole` asserts `scan("orphan:").len()` equals the expected count.
The violating `FdbFidelity::TornApplyOnAmbiguity` store applies only the *first* put of an
ambiguous landed batch (dropping the orphan records), and
`a_torn_apply_of_an_ambiguous_commit_is_caught` (`#[should_panic]`) proves the assertion
catches it. This is a genuinely reachable torn state (the old object's fragments would
leak forever — GC reads the orphan ledger), not a state the model cannot produce.

## Disclosures for the human at sign-off (Open Question 1 ratification)

The brief's Open Question 1 asks the human to ratify fidelity re MVCC-vs-BTreeMap. Two
**further narrowings** the brief did not surface, both now built and disclosed here so the
ratification covers what was actually built (v1 sign-off asked for exactly this):

1. **Optimistic-resolver fidelity, not a real read-conflict-set resolver.** The model
   rejects a conditional batch iff its *preconditions* no longer hold at commit time
   (full-value CAS), which is what `commit_chunk_map_superseding` relies on. It does **not**
   model FDB's byte-range read-conflict sets for keys a batch reads but does not
   precondition — there are none in this trait's usage, but a future caller that read
   without preconditioning would not see a 1020 here. This matches the trait contract
   (`crates/traits/src/lib.rs:346-350`), which is precondition-based, not the full FDB
   resolver.

2. **The nemesis is a single-strike budget per scenario, armed explicitly after the
   fixture.** It is not a per-commit coin (that would waste most seeds on no fault). Each
   scenario arms it at the protocol point it wants ambiguous. So the *coverage* claim is
   "the ambiguity space is searched across seeds at one armed point per run", not "every
   commit is independently ambiguous". `FdbObservations` makes a vacuous sweep visible
   (`ambiguous_*` stay 0), and every sweep asserts the nemesis fired.

Both are the honest scope of "the 1021/1031 ambiguity space, searched exhaustively": one
armed commit, all seeds, both codes, both/all fates.

## Refuting the test (forced self-check)

**(a) Genuine red?** Yes — four independent mutations, each reverting one fix, each caught:
  - re-add `&& conditional` to the strike → leg 2's two blind tests FAIL
    ("the nemesis must strike the Intent phase's blind put" / should_panic not satisfied).
  - collapse 1031 into a 1021-style coin → `a_timed_out_commit_may_still_land…` and
    `treating_a_timed_out_commit_like_1021…` FAIL.
  - tautologise `assert_batch_applied_whole` (`orphans == orphans`) →
    `a_torn_apply_of_an_ambiguous_commit_is_caught` FAILs (no longer panics).
  - neuter `renamed_package` → `scan_line_catches_every_manifest_dependency_shape` and
    `scan_manifest_is_red_when_an_fdb_dependency_is_planted` FAIL.
  And the **C4-verify RED phase** (production reverted, tests kept, *bare* cargo test):
  `no_fdb_linkage::the_dst_support_module_declares_the_simulated_fdb_store` FAILs because
  the reverted `support/mod.rs` no longer declares `SimFdbMetadataStore` — simulated
  empirically (see below). The madsim-gated `commit_ambiguity` compiles to 0 tests under
  bare cargo (the harness limitation the brief's Verification posture documents); its
  behavioural red lives in `C4-ci`.

**(b) Production path?** Yes — the ambiguity scenario drives the real four-phase write
protocol (`wyrd_core::write::{intent,write_fragments,commit_overwrite,release}` →
`wyrd_core::metadata::{put_pending,commit_chunk_map_superseding,sweep_pending}`) over the
`MetadataStore` **trait**, exactly as `concurrency.rs`/`conformance.rs` drive redb and
simulated-TiKV. The store under test is the shipped model, not a copy. The conformance leg
runs the *identical* `wyrd_metadata_conformance::run_all` — not a fork. The purity guard's
graph red resolves the **real** `wyrd-metadata-fdb`/`foundationdb` dependency, not a
stand-in.

**(c) Fixture includes the fault?** Yes — the nemesis is *armed and asserted to have
fired* (`obs.ambiguous_* >= 1`), not curated out. Each sweep asserts both fates of the
ambiguity space are reached (landed / not-landed; and for 1031, landed-after-re-read),
so a run that never exercised the fault fails loudly. The torn-apply red uses a store that
actually tears; the blind red uses a batch that is actually blind
(`WriteBatch` with no preconditions, verified by `plan.chunk_ids().len()==1` → one blind
commit).

## How it was run (project runner, with a timeout — no hand-rolled invocation)

- `./engine/xtask.sh dst` (= `cargo test -p wyrd-dst` with `--cfg madsim`,
  `MADSIM_TEST_NUM=50`) — **green**, all binaries incl. the 3-way conformance and the
  9-test `commit_ambiguity`.
- `./engine/xtask.sh ci` (the gating `C4-ci`) — **exit 0, "all checks passed"** (fmt,
  clippy -D warnings, build, test incl. DST, cargo-deny, conformance vectors).
- `cargo fmt --all -- --check` — clean. `cargo clippy -p wyrd-dst --all-targets` under
  **both** bare and `--cfg madsim` — clean (fixed one `manual_is_multiple_of`).
- `no_fdb_linkage` runs green under a bare `cargo test -p wyrd-dst --test no_fdb_linkage`
  (not madsim-gated, as required).
- Patch verified to `git apply --check` cleanly on the base `HEAD` (b1ccca3) in a
  throwaway worktree.
- C4-verify RED phase simulated by hand: reverted `support/mod.rs` + `conformance.rs` to
  base, kept the two new test files, ran bare `cargo test -p wyrd-dst --test
  commit_ambiguity --test no_fdb_linkage` → `the_dst_support_module_declares_the_simulated_fdb_store`
  **FAILED** (genuine structural red), everything else green.

## §10 ACT candidate (carried from v1 — not this bundle's to fix)

The `C4-verify` gates row reads "red without the fix, green with it", but its only
non-madsim discriminator here is a text scan of `support/mod.rs` (assertion (b)/3), a file
`run-verify.sh` never compiles in either phase. The row would read identically for an
empty struct. The brief is candid this is a *structural* red; the gates table is not. Only
`C4-ci` is real evidence for criteria 1–3. Harness-wide scanning hazard, recorded for Act.

## Scope discipline held

No `Undeterminable` `CommitOutcome` variant. `crates/metadata-conformance/` untouched. No
new Cargo dependency (graph guard shells the already-present `cargo`). Nothing under
`crates/*/src/` changed. `xtask/src/main.rs` untouched (the new files are ordinary
`crates/dst/tests/` binaries `run_dst()` already sweeps). `exactly_one_writer_wins_over`
in `concurrency.rs` was **not** weakened — redb and simulated-TiKV keep their strict
`.unwrap()`; the ambiguity scenario has its own property body in `commit_ambiguity.rs`.
