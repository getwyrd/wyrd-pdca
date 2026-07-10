# Build notes — issue 468 / metadata-fdb-dst-story (iteration 3)

## What this iteration changes, and why it is narrow

Iteration 2's sign-off **rejected on goal (c) — the `libfdb_c` purity guard — only.** The
adversary explicitly "could not refute" the behavioural machinery (the four-phase-protocol
ambiguity red→green, the 1021-vs-1031 distinction, the torn-inode atomicity red, the
blind-batch path). So the model (`support/mod.rs`), the scenario (`commit_ambiguity.rs`) and
the shared-contract leg (`conformance.rs`) are carried **byte-identical** from iteration 2 —
churning code the reviewer signed off would be the opposite of what the carry-forward asks.

The whole delta of iteration 3 lives in **`crates/dst/tests/no_fdb_linkage.rs`** and fixes
the two carry-forward items:

> 1. The graph-linkage invariant is asymmetric between invocations. Under bare
>    `run-verify.sh` (`cargo test`, no `--cfg madsim`), `cargo tree` omits the entire
>    `[target.'cfg(madsim)'.dev-dependencies]` section — exactly the section, and exactly the
>    rename form, where a real FDB dep would be added in this manifest's house style
>    (`crates/dst/Cargo.toml:55`). Make the guard scan the graph the FDB risk actually lives
>    in under BOTH invocations.
> 2. No planted red covers the target-cfg-gated rename form. Plant the red under the
>    madsim-gated section so the guard's ability to catch the real-risk shape is proven, not
>    resting on non-existence.

## The root cause, in two sentences

`cargo tree` decides whether `[target.'cfg(madsim)'.dev-dependencies]` is in the graph by
evaluating `cfg(madsim)` against the *effective* `RUSTFLAGS`, and iteration 2 inherited that
value from the ambient invocation. So under `run_dst()` (ambient `--cfg madsim`) the guard
saw the madsim section, but under `run-verify.sh`'s bare `cargo test` it did not — an FDB dep
added in house style under that section would link `libfdb_c` into every DST binary yet pass
the graph guard in the bare invocation.

I verified this empirically on the target before writing the fix
(`crates/dst/Cargo.toml:55-73`): `cargo tree -p wyrd-dst` shows `madsim-etcd-client` /
`wyrd-coordination-etcd` (the real cfg(madsim) deps) **only** with `RUSTFLAGS=--cfg madsim`
set; without it those nodes are absent. That is the same blindness the FDB rename form would
exploit.

## The fix

The guard now **forces** the cfg it resolves under instead of inheriting it
(`crates/dst/tests/no_fdb_linkage.rs`):

- A `Resolve` enum (`no_fdb_linkage.rs:~130`) with `Madsim` (`RUSTFLAGS=--cfg madsim`) and
  `Bare` (`RUSTFLAGS=""`). `cargo_tree` sets `RUSTFLAGS` explicitly and `env_remove`s
  `CARGO_ENCODED_RUSTFLAGS` (which would otherwise silently override `RUSTFLAGS` and
  reintroduce the ambient asymmetry). The madsim graph is a superset of the bare graph for
  the FDB question — the only custom cfg any workspace manifest gates a `[target.'cfg(...)']`
  section on is `madsim`, and `cargo tree` only resolves, never builds, so no other ambient
  flag changes the result.
- `the_dst_dependency_graph_links_no_libfdb_c` now resolves with `Resolve::Madsim`, so the
  invariant holds identically under `cargo xtask dst`/`run_dst()` **and** under
  `run-verify.sh`'s bare `cargo test` — the fix for carry-forward item 1.
- `the_graph_scanner_is_red_on_a_cfg_madsim_gated_rename_and_transitive_edge` (replacing v2's
  plain-section planted red) plants the FDB dep under `[target.'cfg(madsim)'.dev-dependencies]`
  in the rename form + a transitive edge, and asserts **both** directions: caught under
  `Resolve::Madsim`, invisible under `Resolve::Bare`. The `Bare` control is what proves the
  cfg-forcing is load-bearing rather than decorative — the fix for carry-forward item 2.

The module doc (`no_fdb_linkage.rs:26-49`) is rewritten: v2 claimed the two graphs were
symmetric ("Both must be clean"); it now states the guard forces `--cfg madsim` and why.

### Cost of the rejected alternative (kept text scan, "just add more cases")

Rejected in v2 already and still correct: a single-manifest text scan cannot see feature
unification or a transitive edge at all — those are properties of the resolved graph. It also
cannot distinguish "named but not linked" (`wyrd-metadata-fdb` with its default-off `fdb`
feature) from "linked". The `cargo tree` approach is ~60 lines here and needs no new crate
(cargo is already running the test). The text scan is **kept** as the strictly-separate
*policy* assertion (2) (`the_dst_manifest_declares_no_fdb_dependency`), not as the linkage
evidence.

## Verification — red→green through the project runner

All runs through `./engine/xtask.sh` (the configured gate runner, with its timeout) or a
bounded bare `cargo test` matching what `run-verify.sh` itself shells; no hand-rolled
container invocation.

- **`./engine/xtask.sh dst` (the gating `C4-ci` behavioural evidence) — exit 0.**
  `commit_ambiguity` 9/9 incl. the three `should_panic` demonstrated reds
  (`assuming_an_ambiguous_commit_did_not_land_fails_the_sweep`,
  `assuming_an_ambiguous_blind_put_landed_leaves_a_chunk_unprotected`,
  `treating_a_timed_out_commit_like_1021_fails_the_sweep`); `conformance` 3/3 incl.
  `sim_fdb_backend_passes_shared_contract`; `no_fdb_linkage` 9/9.
- `no_fdb_linkage` bare (`cargo test -p wyrd-dst --test no_fdb_linkage`) — 9/9, and again
  under `RUSTFLAGS=--cfg madsim` ambient — 9/9. So the guard is invocation-independent, which
  was the whole point.
- `cargo fmt -p wyrd-dst -- --check` clean; `cargo clippy -p wyrd-dst --tests` clean under
  both bare and `--cfg madsim`.
- `patch.diff` applies cleanly on a pristine base (`git reset --hard HEAD && git apply`).

### Refuting the test (forced self-check)

- **(a) Genuine red?** Yes, and demonstrated by mutation, not assertion. Reverting the fix
  (`Resolve::Madsim => ""`, i.e. resolving the madsim graph like the pre-fix ambient-bare
  invocation) makes `the_graph_scanner_is_red_on_a_cfg_madsim_gated_rename_and_transitive_edge`
  **FAIL**: `left: []` vs `right: ["foundationdb","foundationdb-sys"]` — the cfg(madsim)-gated
  FDB node becomes invisible, exactly the bug. Restored after. The behavioural reds are the
  three `should_panic` tests, red→green through `C4-ci`. The `C4-verify` structural red
  (`the_dst_support_module_declares_the_simulated_fdb_store`) was reproduced by hand: revert
  `support/mod.rs`+`conformance.rs` to base, keep the two added test files, bare
  `cargo test -p wyrd-dst --test commit_ambiguity --test no_fdb_linkage` → that one test
  **FAILED** (support no longer declares the model), everything else green.
- **(b) Production path?** Yes. The graph guard resolves the **real** `wyrd-dst` and real
  `wyrd-metadata-fdb`/`foundationdb 0.10` dependencies via cargo's own resolver, not a
  stand-in; the real-backend red (`the_graph_scanner_is_red_on_the_real_fdb_backend_with_its_feature_on`)
  turns on `wyrd-metadata-fdb`'s default-off `fdb` feature and sees the genuine article. The
  ambiguity scenario drives the shipped `SimFdbMetadataStore` through the `MetadataStore`
  trait over the real four-phase write protocol (unchanged from v2).
- **(c) Fixture includes the fault?** Yes. The planted graph red carries the actual
  fault — an FDB dep under the actual `[target.'cfg(madsim)'.dev-dependencies]` section in the
  actual rename form plus a transitive edge — and the `Bare` control proves that fault is
  cfg-gated (present in one resolution, absent in the other), so neither half rests on
  non-existence. The nemesis in the behavioural sweep is armed and asserted to have fired
  (`obs.ambiguous_* >= 1`), unchanged from v2.

## Disclosures for the human at sign-off (carry these into Open Question 1 ratification)

The v1 sign-off asked that the fidelity ratification cover what was *actually* built. The
model is unchanged from v2, so the same three narrowings still stand and belong in the
ratification:

1. **Storage modelled as a plain `BTreeMap`, not a versioned MVCC keyspace** (brief Open
   Question 1). The brief's position is this buys nothing for the trait's precondition-based
   contract; the call is the human's.
2. **Blind-batch narrowing.** [Resolved in v2, retained.] The nemesis no longer gates on
   `&& conditional`, so a blind (precondition-free) batch can be ambiguous — matching
   production `classify_commit_error` (`crates/metadata-fdb/src/lib.rs:212-218`), which
   returns `UnknownResult` for 1021/1031 before the `conditional` check.
3. **1031 vs 1021.** [Resolved in v2, retained.] `SimCommitUnknownResult` carries `code` and
   models 1031's "promises nothing" (a batch may land *after* the settling re-read) distinctly
   from 1021's "out of flight", per `crates/metadata-fdb/src/lib.rs:240-249`.
4. **The nemesis is a single-strike budget per scenario, armed after the fixture**, not a
   per-commit coin — so "the ambiguity space searched exhaustively" means "one armed point,
   all seeds, both codes, all fates", and `FdbObservations` makes a vacuous sweep visible.

## §10 ACT candidate (carried, not this bundle's to fix)

`C4-verify`'s gates row reads "red without the fix, green with it", but `run-verify.sh` runs a
**bare** `cargo test` under which `commit_ambiguity.rs` (`#![cfg(madsim)]`) compiles to
nothing — so the behavioural property (criteria 1–3) never executes in the per-fix verify.
The only per-fix discriminator that goes red is the **structural** text-scan
`the_dst_support_module_declares_the_simulated_fdb_store`. Only `C4-ci` is real evidence for
criteria 1–3; the brief is candid about this (`brief.md:113-127`), the gates table is not.
Harness-wide scanning hazard, recorded for Act — not a defect of this patch.

## Scope discipline held

No `Undeterminable` `CommitOutcome` variant. `crates/metadata-conformance/` untouched. **No
new Cargo dependency** (the graph guard shells the already-present `cargo`; that is why it
scans via `cargo tree` rather than parsing TOML — `crates/dst` has no `toml`/`serde` dep, and
adding one would trip ADR-0003 §2 + the `deny.toml` allowlist, a human-only decision).
Nothing under `crates/*/src/` changed. `xtask/src/main.rs` untouched. `concurrency.rs`'s
`exactly_one_writer_wins_over` was **not** weakened — redb and simulated-TiKV keep their strict
`.unwrap()`; the ambiguity scenario has its own property body in `commit_ambiguity.rs`.

## External dependencies

None. `cargo xtask dst` is container-free and seed-deterministic; the graph guard runs
offline (`cargo tree --offline --locked`) against dependencies already resolved. No Docker,
no `libfdb_c`, no live cluster required or used.
