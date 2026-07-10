# Build notes — issue 468 / metadata-fdb-dst-story

**Target branch:** `getwyrd/wyrd @ main`. Built in `$PDCA_WORKTREE`
(`/home/eddie/wyrd/wyrd.pdca-wt-l0`), whose HEAD is `b1ccca3` == `origin/main` (verified
`git merge-base --is-ancestor HEAD origin/main`). `patch.diff` re-verified to `git apply
--check` cleanly on a pristine `b1ccca3` tree. All `path:line` citations below are against
the **post-patch** file at that base unless prefixed "base:".

Test-scope only: nothing under `crates/*/src/` changes; no crate gains a dependency.

---

## What was built, and where

| Criterion | Artifact | Line |
|---|---|---|
| 1 — shared contract, not forked | `crates/dst/tests/conformance.rs:58` `sim_fdb_backend_passes_shared_contract` | drives the identical `wyrd_metadata_conformance::run_all` |
| 2 — commit-ambiguity seed sweep | `crates/dst/tests/commit_ambiguity.rs:223` `commit_ambiguity_invariants_hold_under_the_dst_seed_sweep` | `#[madsim::test]`, swept by `MADSIM_TEST_NUM=50` |
| 3 — demonstrated red | `crates/dst/tests/commit_ambiguity.rs:281` `assuming_an_ambiguous_commit_did_not_land_fails_the_sweep` | `#[should_panic]` |
| 4a — purity guard + planted red | `crates/dst/tests/no_fdb_linkage.rs:165`, `:140` | non-madsim |
| 4b — structural discriminator | `crates/dst/tests/no_fdb_linkage.rs:182` | non-madsim; the `C4-verify` red |
| the model | `crates/dst/tests/support/mod.rs:395` `SimFdbMetadataStore` | second parametrization |

Gate evidence on this machine:

* `cargo xtask ci` (the gating `C4-ci`, which calls `run_dst()`): **`xtask ci: all checks
  passed`**, rc=0.
* `cargo xtask dst` standalone: green; `commit_ambiguity` 3/3, `conformance` 3/3 (redb,
  sim-TiKV, sim-FDB), `no_fdb_linkage` 5/5.
* `cargo test -p wyrd-dst --test commit_ambiguity --test no_fdb_linkage` (the *exact*
  `C4-verify` command, no `--cfg madsim`): rc=0 — `commit_ambiguity` "running 0 tests",
  `no_fdb_linkage` 5 passed. This is the brief's predicted GREEN phase.
* `cargo fmt --all -- --check` rc=0; `rustfmt --edition 2024 --check` run individually over
  all four touched files, clean; `typos crates/dst/tests/` clean. **Commit-ready for the
  target's own hooks** (`cargo fmt --all -- --check` is `run_ci_steps`' first step,
  `xtask/src/main.rs:1290`).

---

## The forced refutation (a) / (b) / (c)

### (a) Genuine red? — **yes**, proven three ways, by actually reverting/mutating.

The brief's verification posture is split, so the red is too. I ran each.

1. **`C4-verify`'s structural red.** Reverted `crates/dst/tests/support/mod.rs` to base
   (`git checkout --`), kept the added tests — exactly what `engine/scripts/run-verify.sh:256-271`
   does — and ran the gate's own command:

   ```
   $ cargo test --quiet -p wyrd-dst --test no_fdb_linkage
   the_dst_support_module_declares_the_simulated_fdb_store --- FAILED
   panicked at crates/dst/tests/no_fdb_linkage.rs:190:5:
   .../crates/dst/tests/support/mod.rs must declare `SimFdbMetadataStore` …
   test result: FAILED. 4 passed; 1 failed
   ```

   Confirms the brief's engineered red. Without 4(b) this bundle would have tripped
   `run-verify.sh:269` ("the test PASSES without the fix") because a `#![cfg(madsim)]` file
   compiles to nothing under the bare `cargo test`.

2. **The behavioural red, by mutating the model into the shape production forbids.** The
   pre-fix red for `commit_ambiguity.rs` is a mere absence red (the model does not exist), so
   I did not stop there. I mutated `commit_optimistic` (`support/mod.rs:503-504`) to return
   `Ok(CommitOutcome::Conflict)` instead of `Err(SimCommitUnknownResult)` for a struck commit
   — i.e. exactly the thing `crates/metadata-fdb/src/lib.rs:67-73` and `:150-153` forbid
   ("1021 … **never** `Conflict`"). Result:

   ```
   test the_settling_re_read_covers_both_halves_of_the_ambiguity_space ... FAILED
   test commit_ambiguity_invariants_hold_under_the_dst_seed_sweep ... FAILED
   panicked at crates/dst/tests/commit_ambiguity.rs:171:5:
   the settled winner count (0) must equal the inode's version bump (1 -> 2)
     — an ambiguous commit was counted without a settling re-read
     left: 1   right: 0
   ```

   So the invariant genuinely binds the 1021→`Err` contract, not just the existence of a file.

3. **The vacuity red.** I mutated `arm_commit_ambiguity` (`support/mod.rs:439`) into a no-op.
   **All three** `commit_ambiguity` tests failed — including the `#[should_panic]` twin, which
   stops panicking and is therefore failed by `should_panic`:

   ```
   test the_settling_re_read_covers_both_halves_of_the_ambiguity_space ... FAILED
     panicked at commit_ambiguity.rs:246: the nemesis must fire once at seed 0  (left: 0, right: 1)
   test commit_ambiguity_invariants_hold_under_the_dst_seed_sweep ... FAILED
   test assuming_an_ambiguous_commit_did_not_land_fails_the_sweep - should panic ... FAILED
   ```

   A sweep that silently stopped arming the nemesis cannot pass. That is what the brief's
   "`Observations` records how often it fired, so a sweep that never armed it is visibly
   vacuous" asks for, made into an assertion rather than a comment.

All mutations were reverted; the tree was re-run green afterwards (`xtask dst: GREEN`,
`xtask ci: all checks passed`).

### (b) Production path? — **yes, for the code this patch changes.**

Honest scoping, stated plainly. This slice's "production" surface is the DST tier itself —
the brief puts `crates/metadata-fdb/` **out of scope** and forbids touching
`crates/metadata-conformance/`. What the tests drive:

* `commit_ambiguity.rs` drives the **real** `wyrd_core::write::{intent, write_fragments,
  commit_overwrite}` and `wyrd_core::read::{read_inode, read_path}` (production `core`
  metadata/commit path, `crates/core/src/write.rs:271`, `crates/core/src/metadata.rs:459`)
  through the **real** `MetadataStore` trait (`crates/traits/src/lib.rs:350`), over a real
  `FsChunkStore`. No mock of `core`, no re-implementation of the commit protocol. Same
  composition as the peer `crates/dst/tests/concurrency.rs:75`.
* `conformance.rs:58` drives the **shared, unmodified** `wyrd_metadata_conformance::run_all`
  (`crates/metadata-conformance/src/lib.rs:291`) — the same seven clauses redb, sim-TiKV and
  the real `metadata-fdb` driver pass. Shared, not forked: I added a caller, not a clause.
* `no_fdb_linkage.rs` scans the **real** `crates/dst/Cargo.toml` and the **real**
  `crates/dst/tests/support/mod.rs`, via the same `scan_manifest_at` (`:83`) the planted-red
  test calls — one scanner, two call sites (the shape of
  `xtask/tests/deploy_no_orchestrator_coupling.rs:67-99`).

`SimFdbMetadataStore` is a **model**, and I want to be exact about what that does and does not
buy. It is not a mock *of the code under test*: the code under test is `core`'s commit
protocol and the trait contract, and the model is the backend beneath it — the same status
`SimTikvMetadataStore` has held since #447. It cannot be the real driver, because `libfdb_c`
spawns its own network thread (`crates/metadata-fdb/src/lib.rs:846-869`), which ADR-0009 /
ADR-0035 forbid inside madsim; that impossibility is the *reason* issue #468 exists. What
transfers from model to concrete is the shared contract suite (the equivalence anchor the
brief keeps) plus the 1020/1021 classification the model reproduces verbatim from
`crates/metadata-fdb/src/lib.rs:17-40` and `:67-73`. **What the model cannot prove is that the
real driver implements that classification** — that is pinned separately by
`metadata-fdb`'s own `store::tests` and `xtask fdb-conformance`, both out of scope here.
Reviewer should hold that boundary in view; it is the honest limit of this slice.

### (c) Fixture includes the fault? — **yes.**

The nemesis is **armed on the commits under test** and its firing is asserted, not assumed:

* `ambiguous_commit_settles_over` (`commit_ambiguity.rs:88`) builds the fixture with the
  nemesis **disarmed** — deliberately, so the ambiguity lands on the four-writer CAS race and
  not on `commit_create`, which is also a conditional batch — then calls
  `meta.arm_commit_ambiguity(1)` at `:106`, *before* spawning the writers.
* Every one of the four racers is a real participant; **none is curated out**. The struck
  writer is the first whose CAS the resolver accepts — i.e. the writer that would otherwise
  have won.
* `commit_ambiguity.rs:228-231` asserts `obs.ambiguous_commits == 1` **at every one of the 50
  sweep seeds**; `:241` asserts that across 0..64 seeds the sweep sees **both halves** of the
  ambiguity space (≥1 seed where the ambiguous commit landed, ≥1 where it did not). A fixture
  that quietly excluded the fault fails these (proven in (a)-3).

I also verified the `MADSIM_TEST_NUM=50` sweep actually reaches the `#[madsim::test]` rather
than running one seed, instead of assuming it: I temporarily asserted
`Handle::current().seed() < 3` inside the test and `cargo xtask dst` went **red**, so seeds ≥3
are genuinely driven. Probe removed. (`xtask/src/main.rs:1337` `DST_SEEDS = "50"`, `:1355`.)

---

## Design decisions, and what I ruled out

### Why the nemesis has a **budget of 1** rather than a per-commit coin

`support/mod.rs:370-377` (`nemesis_budget`). With a per-commit strike coin, a schedule where
all four writers are struck and none lands yields **zero** winners with the version unbumped —
a legitimate FDB availability event, but one that makes the brief's criterion 2(ii) ("exactly
one writer won and the inode version bumped exactly once") false. A budget of one strike makes
2(ii) literally true at every seed: the struck commit either landed (it is the winner) or did
not (the next writer wins). The seed still owns both remaining degrees of freedom — *which*
writer the schedule puts first at the resolver, and *whether* its mutation landed.

I did not want to weaken the property to `winners <= 1` and call it done, so the body asserts
**both**: `winners <= 1` (`:164`) and the strictly stronger, load-bearing
`settled.version - prior.version == winners` (`:171`) — which is the assertion the violating
observer trips, and which would also catch a zero-winner-but-version-bumped tear. The
`winners == 0` arm (`:194-198`) asserts the inode is byte-identical to `prior`, so a "nothing
landed" schedule is still checked rather than merely tolerated.

### Nemesis scoped to **conditional** batches

`support/mod.rs:493-495`. `write::intent` / `release` issue **blind** batches. A 1021 on a
blind batch is a different concern, already held by the production driver's blind-retry gate
(which never re-applies 1021 — `crates/metadata-fdb/src/lib.rs:53-56`) and pinned by that
crate's `store::tests`. Striking blind batches here would only make `intent().unwrap()` panic
in the fixture, testing nothing about the trait contract. Documented at the method.

The 1020 half is pinned structurally rather than by a nemesis: `commit_optimistic`
(`support/mod.rs:481-485`) **asserts** that a precondition-free batch can never reach the
`Conflict` arm, which reproduces `crates/metadata-fdb/src/lib.rs:62-64` ("a write-only
transaction has an empty read-conflict set, so the resolver cannot reject it with 1020").

### Alternatives rejected — with the cost, not an adjective

* **Weaken `exactly_one_writer_wins_over` (`concurrency.rs:75`) to tolerate `Err` and reuse it
  for the FDB store.** Rejected; the brief names this as the hazard. Concretely it means
  changing `concurrency.rs:104-110` from `.unwrap()` to a match, which **silently relaxes redb
  and simulated-TiKV too** — for them an `Err` from `commit` genuinely is a bug, and 3 of the
  5 tests in that file (`:140`, `:149`, `:177`) plus `network.rs:468` depend on that strictness.
  Cost: 1 file, ~8 lines changed, but it *removes* an invariant from two backends to add a
  scenario for a third. A separate 285-line property body costs one new file and removes
  nothing.
* **Add `CommitOutcome::Undeterminable`.** Rejected. Concrete cost: `crates/traits/src/lib.rs:355`
  is consumed across **10 crates** — `git grep -l "CommitOutcome::" -- crates/` →
  `chunkstore-grpc, core, custodian, dst, metadata-conformance, metadata-fdb, metadata-redb,
  metadata-tikv, server, traits`; every exhaustive `match` on it becomes non-exhaustive. And it would be *less* faithful: the driver
  deliberately expresses ambiguity as `Err` (`crates/metadata-fdb/src/lib.rs:67-69`). Modelling
  something production does not do is anti-fidelity, so this loses on correctness before cost.
* **A TOML parse for the linkage guard.** Rejected per the brief's `External dependencies`:
  `crates/dst/Cargo.toml` has no `toml`/`serde` dep; adding one triggers ADR-0003 §2's
  three-test audit + `deny.toml` allowlist, which INTEGRATION §4 makes a **human-only**
  decision. Cost of the text scan instead: `scan_line` is 24 lines (`no_fdb_linkage.rs:58-81`)
  and handles the three manifest shapes (`k = v`, `k.workspace = v`, `[deps.k]`), each pinned
  by `:109` and `:129` (comments and prose must not false-positive — this crate's own docs
  discuss FoundationDB at length).
* **A second simulation framework.** Rejected by issue item 4 and by the tree: `#258`/`#447`
  landed `Fidelity`/`Observations`/a trait-generic property body *to be* parametrized. The FDB
  model reuses `network_hop` (`support/mod.rs:121`), `preconditions_hold` (`:127`), `apply`
  (`:146`) and the `Mutex<Inner>` + `observations()` discipline verbatim.

### Why `FdbFidelity`/`FdbObservations` are new types rather than reused enums

`Fidelity`'s variants (`AwaitInsideCommit`, `SynchronousRedbShaped`) and `Observations`'
fields (`mid_commit_lock_conflicts`, `max_inflight`) are TiKV-2PC-specific — a pessimistic
prewrite lock has no analogue in FDB's optimistic resolver. The brief asks to reuse *the
shape* (`support/mod.rs:43`, `:61`, `:86`), which is what I did: same instance-only state,
same `Mutex<Inner>`, same `observations()` seam, same "a sweep that never armed it is visibly
vacuous" role. Reusing the *values* would have meant a `SimFdbMetadataStore` carrying a
`SynchronousRedbShaped` variant it can never be in.

---

## Two things for the human at sign-off (both are the brief's own open questions)

1. **Ratify the simulated-FDB fidelity.** Recorded in the module doc at
   `crates/dst/tests/support/mod.rs:31-61`, mirroring how the simulated-TiKV decision was
   recorded at base:`support/mod.rs:1-29` — optimistic conflict at commit + seed-selected
   commit ambiguity, storage modelled as a plain `BTreeMap` rather than a versioned MVCC
   keyspace. The module doc states the argument (the version CAS is a full-value precondition;
   FDB stores keys/values byte-identically, `crates/metadata-fdb/src/lib.rs:873-875`) and
   explicitly says *"the human ratifies this at sign-off"*, exactly as its TiKV predecessor
   does. **This is the ratification the brief asks for.**
2. **The recorded decision lives in two places**, per the brief's open question 2: this design
   proposal (`brief.md`) is the record, and a condensed version is in the `SimFdbMetadataStore`
   module doc (`support/mod.rs:31-61`). **No accepted ADR or proposal was edited**
   (docs-immutability, ADR-0037/ADR-0001) — `git diff --stat` touches only `crates/dst/tests/`.

Also worth a reviewer's eye: `C4-verify`'s red for this bundle is **structural** (assertion
4(b): the `SimFdbMetadataStore` seam exists), not behavioural. That is by design and stated
plainly in the brief's `Verification posture`; the behavioural red→green lives in `C4-ci`,
whose evidence is section (a) above. `C4-verify` is `gating = false`.

## External dependencies

**None needed, none used.** `cargo xtask dst` is container-free and seed-deterministic; no
Docker, no `libfdb_c`, no live cluster was required or wanted at any point — which is the
substance of the brief's `Falsifiability` argument, not a convenience. No new Cargo dependency:
the model uses `rand`/`rand_chacha`, already dev-deps of `wyrd-dst`
(base:`crates/dst/Cargo.toml:40-41`), through the same `rng_u64` helper shape as
`crates/dst/tests/network.rs:521-525`.

No NEEDS-HUMAN external dependency.

## STOP discipline

Nothing pushed, no branch created, no PR opened. Bundle contains `patch.diff`, the two test
files, and these notes.
