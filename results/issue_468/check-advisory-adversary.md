# Adversarial review — issue 468 / metadata-fdb-dst-story (iteration 4)

**Posture: I tried hard to refute this bundle and largely could not.** I re-ran the
asserted red→green on the target, ground the model against production source, and hunted
for a breaking input on each of the four legs and the purity guard. The residual items
below are fidelity/verification *ratifications a human must sign*, not defects I can pin.

## Evidence I reproduced (attempts to refute the red→green that failed)

- Ran `cargo test -p wyrd-dst --test no_fdb_linkage` (bare, no `--cfg madsim`): **9/9 green**,
  incl. `the_graph_scanner_is_red_on_a_cfg_madsim_gated_rename_and_transitive_edge` and
  `the_graph_scanner_is_red_on_the_real_fdb_backend_with_its_feature_on`. The cfg(madsim)
  planted red genuinely resolves under forced `--cfg madsim` and vanishes under `Bare` —
  the iteration-2/3 finding is closed, not papered over.
- Ran `RUSTFLAGS="--cfg madsim" MADSIM_TEST_NUM=50 cargo test -p wyrd-dst --test commit_ambiguity`:
  **13/13 green**, incl. all six `should_panic` reds (a/b torn, c blind, d 1031≠1021,
  e/f contended 1031 deferral). The reds are `#[should_panic(expected=...)]` pinned to the
  *specific* load-bearing message, so an unrelated early panic fails the test rather than
  passing it — I could not turn any red into a false green.
- Checked model fidelity against production at `crates/metadata-fdb/src/lib.rs:212-219`
  (`classify_commit_error`: 1021/1031 → `UnknownResult` for *every* batch, before the
  `conditional` check) and `:247-249` (`may_still_commit`: false/true for 1021/1031). The
  model at `support/mod.rs` reproduces both exactly (nemesis not batch-shape-aware;
  `SimCommitUnknownResult::may_still_commit`). Iteration-1 findings 1 (blind ambiguity) and
  2 (1031-with-a-code) are genuinely closed.
- Checked the torn-apply red is not incidental: `commit_chunk_map_superseding`
  (`crates/core/src/metadata.rs:474-476`) builds the batch inode-put **first**, then orphan
  puts, so `TornApplyOnAmbiguity`'s `puts.first()` publishes the inode while dropping the nine
  `orphan:` records — exactly the "fragments leak forever" narrative the red claims. Not a
  tautology (iteration-1 finding 4 closed): `assert_batch_applied_whole` reads the store's
  own orphan count, independent of the observer.
- Traced leg 1 (1021) and leg 4 (1031-under-contention) by hand for double-counting /
  two-winner races: the mutex serialises resolver+apply+strike with no await, disjoint
  per-writer chunk-id ranges make `settled.chunk_map == chunk_map` a faithful winner oracle,
  and `settle_in_flight`'s precondition re-check rejects every stale deferred batch. Could
  not construct a seed where `version_bump != winners`.

## Residual items for human adjudication

- **NEEDS-HUMAN — fidelity ratification (T5), narrowings must be signed, not just built.**
  `crates/dst/tests/support/mod.rs:67-70` (and Open Question 1) ask the human to ratify:
  (i) storage modelled as a plain `BTreeMap`, not versioned MVCC; (ii) the faithful model
  **structurally cannot tear** — `commit_optimistic` applies inside the mutex with no await
  (`support/mod.rs`, phase 3), so "no torn inode observable" is demonstrated only against the
  *violating* `TornApplyOnAmbiguity` twin, never by the faithful store failing to tear; and
  (iii) a **blind batch under 1031** is exercised by no leg (leg 2 arms only 1021; the 1031
  legs are all conditional CAS). None of these is a defect, but each is a fidelity narrowing
  the human must accept as sufficient for goal (a)/(b). They are disclosed in the module doc
  — this bullet only ensures the sign-off actually rules on them.

- **NEEDS-HUMAN — the `C4-verify` gates row overstates the per-fix red (already tracked, not
  this diff's to fix).** `check-gates.json` C4-verify reads "red without the fix, green with
  it," but `commit_ambiguity.rs:71` is `#![cfg(madsim)]`, so under `run-verify.sh`'s bare
  `cargo test` it compiles to **0 tests** (I confirmed the file is fully cfg-gated). The only
  thing that goes red under bare verify is the *structural* string-scan
  `the_dst_support_module_declares_the_simulated_fdb_store` (`no_fdb_linkage.rs:1645-1674`).
  The behavioural red→green (criteria 1–3) is real **only** under `C4-ci`/`cargo xtask dst`.
  The brief is candid about this and prior carry-forwards logged it as a §10 harness-wide Act
  matter; I flag it so the sign-off does not read the row's wording as behavioural evidence.

## Weaker points I found but could not weaponise

- The **policy** scanner `scan_line` (`no_fdb_linkage.rs:1483-1508`) strips everything from
  the first `#`, so a contrived `fdb = { git = "https://host/r#frag", package = "foundationdb" }`
  (the `package=` sitting *after* a `#` in a git URL) would slip the rename detector. But this
  is invariant (2), explicitly the "strictly weaker as evidence of linkage" guard; the
  load-bearing invariant (1), `the_dst_dependency_graph_links_no_libfdb_c`, resolves the real
  `cargo tree` graph and would surface `foundationdb`/`foundationdb-sys` as package nodes
  regardless of manifest text. So the bypass is redundantly covered. Not a refutation.
- I probed whether `cargo tree -p wyrd-dst` feature-unification could diverge from the real
  `cargo test -p wyrd-dst` workspace build and let `foundationdb` in unseen. It cannot add a
  dependency edge to wyrd-dst's closure that the scan misses — feature unification toggles
  features of shared deps, it does not graft new edges — and the policy guard bans *naming*
  `wyrd-metadata-fdb` at all. Attempted, could not refute goal (c)'s "mechanically guaranteed"
  claim.

**Bottom line:** attempted to refute the evidence, the model's fidelity, the torn/1031
reds, the winner-counting invariants, and the purity guard's rename/transitive/cfg-madsim
coverage; could not. The open items are ratifications, not breaks.
