# Design proposal — issue 468 / metadata-fdb-dst-story

> The Plan artifact for the **exception**: a change significant enough to warrant a
> design proposal. Do reads ONLY this file and implements it; Check runs the regular
> gated check on the code.

- **Slug:** metadata-fdb-dst-story
- **Kind:** enhancement (design proposal)
- **Goal:** Hold `metadata-fdb` — an FFI backend that can never run inside the
  deterministic simulator — to the same standard as the backends it replaces, by (a)
  recording the simulated-FDB-vs-contract-harness decision, (b) adding a **simulated-FDB**
  `MetadataStore` model as a *second parametrization* of the existing simulated-TiKV
  skeleton, whose distinguishing capability is the **1021 `commit_unknown_result`
  commit-ambiguity** failure shape, and (c) mechanically guaranteeing that no `libfdb_c`
  symbol is ever reachable from a DST build target.
- **Success criterion:** `cargo xtask dst` is green with the new legs, and
  `cargo test -p wyrd-dst --test no_fdb_linkage` passes, asserting all four of:
  1. `sim_fdb_backend_passes_shared_contract` — the simulated-FDB model passes the
     **identical** `wyrd_metadata_conformance::run_all` clauses
     (`crates/metadata-conformance/src/lib.rs:291`) that redb and simulated-TiKV already
     pass in-simulator (`crates/dst/tests/conformance.rs:31`, `:42`). Shared, not forked.
  2. A **commit-ambiguity seed sweep**: with the 1021 nemesis armed, across the seed sweep
     `cargo xtask dst` drives, no invariant is violated — (i) an ambiguous commit is never
     counted as a winner **or** as a loser without a settling re-read, (ii) once re-read
     settles it, exactly one writer won and the inode version bumped exactly once, and
     (iii) no torn/hybrid inode is ever observable.
  3. A **demonstrated red** proving (2) is load-bearing: an observer that *assumes* an
     ambiguous commit did not land fails the sweep. Mirror the violating-store technique at
     `crates/metadata-conformance/tests/demonstrated_red.rs:1-15`.
  4. `no_fdb_linkage` (a **non-madsim** test — see `Test file`) asserts two things:
     (a) `crates/dst/Cargo.toml` declares neither `wyrd-metadata-fdb` nor `foundationdb`, and
     with either planted in a temp-fixture manifest the scanner goes RED (planted-red pattern:
     `xtask/tests/deploy_no_orchestrator_coupling.rs:67`); **and (b)** `crates/dst/tests/support/mod.rs`
     declares `SimFdbMetadataStore` — the second parametrization this bundle ships. Assertion
     (b) is what gives `C4-verify` a real per-fix RED (see `Verification posture`); (a) is the
     standing invariant and is green on the base by design.

  Plus `cargo xtask ci` (the gating `C4-ci`) stays green — `run_dst()` is inside it.
- **Falsifiability:** RED is producible **only in the simulator**, with **zero external
  dependencies**, and this is the substance of the decision rather than a convenience.
  - *Why no real topology can produce it.* The binding new property concerns FDB error
    **1021 `commit_unknown_result`**. `crates/metadata-fdb/src/lib.rs:71` states it
    outright: *"A healthy `fdbserver` cannot be made to emit 1021 on demand."* Neither the
    single-node `deploy/fdb-single-node/` cluster nor #469's future 3-process cluster can
    be driven to exhibit it deterministically; #442's fault battery can only **sample** the
    ambiguity space. So a criterion scoped to the real backend would be one that *cannot
    fail on the environment Do gets* — the exact Plan-blocking shape this field exists to
    catch. The resolution is not to narrow the criterion but to **provision the
    environment**: the environment is the simulator, and building it is this bundle.
  - *Where the red actually lives.* `cargo xtask dst` (`--cfg madsim`, container-free, and
    already inside `cargo xtask ci`) on the plain `$PDCA_WORKTREE` / `../wyrd-verify`
    checkout. Pre-fix: `SimFdbMetadataStore` does not exist and `tests/commit_ambiguity.rs`
    does not exist (compile/absence red). Post-fix, criterion 3 is a *live* demonstrated
    red — the assume-not-committed observer fails — so the property is proven non-vacuous
    rather than resting on non-existence. Criterion 4's red is a planted manifest.
- **Repo + branch target:** getwyrd/wyrd @ main
  (`feat/m4-production-metadata-backend` merged as PR #489, commit `182ae4f`; branch
  deleted. The issue's "Depends on #438 (the driver)" is satisfied — #438 merged as PR
  #492. The issue's "Coordinates with #258 (M4.7) on the shared structure": **#258 is
  CLOSED and its skeleton landed** as PR #447 (`9374758`), so the fix-forward branch of
  item 4 is the live one — reuse, do not rebuild.)
- **Surfaces:** data
- **Difficulty:** high
- **Scope:** Give the FFI backend a DST story of the same strength as the TiKV backend's.
  One logical change with real cross-file reach: a second parametrization of the shared
  simulated-store skeleton, the new invariant it makes reachable, and the linkage guard
  that keeps the sim graph pure. / **out of scope:**
  - **The contract-harness leg (issue item 2) — ALREADY SATISFIED, do not rebuild.**
    `crates/metadata-fdb/tests/conformance.rs` (landed by #438) drives the same
    `run_all` suite; `xtask fdb-conformance`'s five legs (`xtask/src/main.rs:410-417`) run
    it against a real `fdbserver`. The equivalence anchor exists. Do MUST NOT touch
    `crates/metadata-conformance/` — its clauses are shared by four implementations and are
    provisionally pinned by ADR-0044.
  - **`crates/metadata-fdb/` production code.** Nothing in this slice changes the real
    driver. Its 1020/1021 classification is already correct and already pinned
    (`crates/metadata-fdb/src/lib.rs:148`, `:153`, and the `store::tests` blind-retry unit
    tests). This slice models that behaviour in the simulator; it does not re-decide it.
  - **`xtask/src/main.rs`.** `run_dst()` already sweeps seeds; the new tests are ordinary
    `crates/dst/tests/` binaries and need no dispatch change. (This also keeps 468 off the
    file 439 and 469 edit — see Ordering note.)
  - The fault battery (#442), the contract-doc consolidation (#437), the deploy profiles
    (#469).
- **External dependencies:** **none.** `cargo xtask dst` is container-free and
  seed-deterministic by construction (ADR-0009); adding a container to it is explicitly
  rejected (`crates/dst/tests/support/mod.rs:6-8`). If Do believes it needs Docker,
  `libfdb_c`, or a live cluster for any part of this slice, that is a design error — say so
  in `build-notes.md` rather than working around it.
  **No new Cargo dependency either.** `crates/dst/Cargo.toml` has no `toml`/`serde` dep, so
  the linkage guard must scan the manifest as **plain text**, not parse it. Adding any crate
  triggers the ADR-0003 §2 three-test audit + the `deny.toml` allowlist, which INTEGRATION §4
  makes a **human-only** decision — it would turn this bundle into a NEEDS-HUMAN at sign-off
  for no benefit.
- **Test file:** `crates/dst/tests/commit_ambiguity.rs` (new; the binding **behavioural**
  red→green, executed by `C4-ci`'s `run_dst()`) and `crates/dst/tests/no_fdb_linkage.rs` (new;
  the purity guard **and** the `C4-verify` structural discriminator). The model itself lands in
  `crates/dst/tests/support/` alongside `SimTikvMetadataStore`.
  **Where these actually run — verify before assuming.** `run_ci_steps` runs
  `cargo test --workspace --exclude wyrd-dst` (`xtask/src/main.rs`), so nothing in
  `crates/dst/` runs in the ordinary workspace test step. `wyrd-dst` is driven **only** by
  `run_dst()`, which shells `cargo test -p wyrd-dst` with `RUSTFLAGS=… --cfg madsim` and
  `MADSIM_TEST_NUM=<seeds>`, and `run_dst()` is called from `run_ci()` — so both new files
  DO execute in the gating `C4-ci`. `commit_ambiguity.rs` must be `#![cfg(madsim)]`-gated
  like its peers (`crates/dst/tests/conformance.rs:20`); `no_fdb_linkage.rs` must **not** be,
  so it also passes under a bare `cargo test -p wyrd-dst --test no_fdb_linkage`.
- **Verification posture:** **Split, and the split is forced by the harness — read this before
  writing a line of code.**
  - **The gating evidence is `C4-ci`.** `cargo xtask ci` → `run_ci()` → `run_dst()`, which
    shells `cargo test -p wyrd-dst` with `RUSTFLAGS=… --cfg madsim` and `MADSIM_TEST_NUM`.
    That is the ONLY place the behavioural property (criteria 1–3) actually executes, and
    `C4-ci` is `gating = true` (`pdca.toml`). Red pre-fix (the model and the scenario do not
    exist), green post-fix.
  - **`C4-verify` cannot see the madsim leg, and would FAIL the bundle if not for assertion
    4(b).** `engine/scripts/run-verify.sh` runs a **bare** `cargo test -p wyrd-dst --test
    commit_ambiguity --test no_fdb_linkage` — no `--cfg madsim` (`run_test`, `:230`). A
    `#![cfg(madsim)]` file compiles to nothing under that: verified empirically on this
    checkout, `cargo test -p wyrd-dst --test conformance` prints **"running 0 tests … test
    result: ok"** and exits **0**. So in the RED phase (`:256-271`, production reverted, added
    tests kept) `commit_ambiguity` would vacuously pass, and `run-verify.sh:269` would report
    *"the test PASSES without the fix, so it does not catch the bug (no red)"* and exit 1.
    Assertion **4(b)** is the fix: it lives in the non-madsim `no_fdb_linkage.rs`, and when the
    RED phase reverts `crates/dst/tests/support/mod.rs` the `SimFdbMetadataStore` declaration
    is gone, so the assertion fails and `C4-verify` sees a genuine red. It is a *structural*
    red (the seam exists), not a behavioural one — stated plainly rather than dressed up; the
    behavioural red lives in `C4-ci`.
  - `C4-verify` is `gating = false, promote_after = 3` (`pdca.toml`), so this is a sign-off
    note, not a blocker — but a bundle that structurally cannot go red would poison the
    promote counter, which is why it is engineered rather than tolerated.
  - **Deferred ≠ unbuilt:** nothing here is deferred off-Check. The whole slice — model,
    scenario, seed sweep, purity guard — is built and executed by the gating `C4-ci` on this
    machine, container-free.
- **Citations expected:** Do must cite `path:line` on `origin/main` for every change. Do
  MAY open these cited peer callsites — this is a composition slice and the skeleton to
  mirror already exists:
  - `crates/dst/tests/support/mod.rs:1-29` — the module doc that records the *previous*
    fidelity decision (and names #264 as the open question this bundle answers for FDB);
    `:43` (`enum Fidelity`), `:86` (`struct SimTikvMetadataStore`), `:93`/`:98`
    (`new` / `with_fidelity`), `:106` (`observations`), `:171` (`commit_await_inside`),
    `:233`/`:254` (the `MetadataStore` impl and its `commit`).
  - `crates/dst/tests/conformance.rs:31` and `:42` — how the two implementations are driven
    through the identical `run_all`. Add a third function beside them.
  - `crates/dst/tests/concurrency.rs:75` (`exactly_one_writer_wins_over`) — **read this
    carefully**; see the hazard in §Design 4.
  - `crates/metadata-fdb/src/lib.rs:17-40` (the `Conflict`-exactly-when contract and what
    keeps a blind batch out of `Conflict`) and `:67-73` (the 1021 rule) — the authoritative
    in-repo statement of the 1020/1021 semantics this model must reproduce, including the
    "A healthy `fdbserver` cannot be made to emit 1021 on demand" sentence at `:71`.
  - `crates/traits/src/lib.rs:350` (`async fn commit`) and `:355` (`enum CommitOutcome`) —
    the trait surface. Note it has exactly two variants.
  - `xtask/tests/deploy_no_orchestrator_coupling.rs:67` — the planted-red scanner shape for
    the linkage guard.
- **Prior-art check (triage cycles):** Searched by affected file path across merged
  history, open PRs, and closed/rejected PRs.
  - `crates/dst/tests/` history: `9374758` (#447/#258 — the simulated-TiKV skeleton this
    slice parametrizes), `e65cf69`, `47bd35c`, `20d06f4`. `git -C ../wyrd grep -in "fdb"
    origin/main -- crates/dst/` → **no hits**: no FDB work has ever touched the DST tier.
  - `#258` (M4.7) and `#264` (the fidelity open question) are both **CLOSED**; `#447` is
    **MERGED**. So item 4's conditional ("If #258 lands a … structure") has resolved to
    *yes* — the FDB model must be a second parametrization, not a second framework.
  - Rejected work: one non-merged closed PR in the last 60 (#400, docs/proposal scope) —
    unrelated to `crates/dst/`.
- **Disposition hint:** new-feature

## Motivation

Wyrd's correctness argument rests on a deterministic simulator that replays every timing
accident on demand (ADR-0009). redb and the simulated-TiKV model both live inside it, so
the `MetadataStore` trait is pinned by two implementations (ADR-0006).

`metadata-fdb` cannot join them. The `foundationdb` crate binds `libfdb_c`, which **spawns
its own network thread** and does real, non-deterministic I/O — `ensure_network()` boots it
behind a process-wide `OnceLock` (`crates/metadata-fdb/src/lib.rs:846-869`). Putting that
inside madsim would violate seed determinism outright (ADR-0035, and the explicit rejection
at `crates/dst/tests/support/mod.rs:6-8`).

So the backend Wyrd has *chosen for production* (ADR-0042) is currently the only one held
to a weaker standard than the ones it replaces. That is the gap.

## Design

**1. The decision, recorded.** Both legs, not one:

- **Contract-harness leg: keep (already built).** The shared `run_all` suite is the
  equivalence anchor — the DST-resident stores and the real `metadata-fdb` driver pass the
  *identical* seven clauses, so what DST proves about the trait transfers to the FDB
  concrete to the extent the contract pins it. This exists today and needs no work.
- **Simulated-FDB leg: BUILD IT.** The deciding argument is sourced, not aesthetic.
  `crates/metadata-fdb/src/lib.rs:71`: *"A healthy `fdbserver` cannot be made to emit 1021
  on demand."* 1021 `commit_unknown_result` is the one genuinely new failure shape FDB
  introduces, and it is precisely the one the real fault battery (#442) can only *sample*
  and never *search*. A DST nemesis that returns "unknown" on commit and later reveals
  either outcome is the only way to explore that ambiguity space exhaustively across seeds.
  This answers issue #264's open fidelity question for the FDB backend.

**2. The 1021 question, answered explicitly.** `CommitOutcome` (`crates/traits/src/lib.rs:355`)
has exactly two variants — `Committed` and `Conflict`. There is **no `Undeterminable`
variant, and this slice must not add one**: the driver already models the ambiguous outcome
as `Err(classify::CommitUnknownResult)` (`crates/metadata-fdb/src/lib.rs:153`, `:67-73`),
never as `Ok(Conflict)`. The simulated model therefore reproduces exactly that contract:

- **1020 `not_committed`** → `Ok(CommitOutcome::Conflict)`, and **only for a batch carrying
  preconditions**. A blind (precondition-free) batch must never yield `Conflict`
  (`crates/metadata-fdb/src/lib.rs:17-18`, `:31-40`).
- **1021 `commit_unknown_result`** → `Err(..)`. The commit **may or may not have landed**.
  The model decides, from the seed, which — and *does not tell the caller*. The observer's
  only correct move is a re-read.

**3. `SimFdbMetadataStore` — a second parametrization, not a second framework.**
It lives beside `SimTikvMetadataStore` in `crates/dst/tests/support/`, reusing the
`Fidelity`/`Observations` shape (`support/mod.rs:43`, `:61`, `:86`). Two FDB-shaped modes:

- `OptimisticConflictAtCommit` — the resolver rejects a conditional batch at commit time
  (the 1020 class), as distinct from TiKV's pessimistic prewrite lock-grab.
- `CommitUnknownResult` — the nemesis. On a seed-selected commit, apply-or-don't (also
  seed-selected), then return `Err`.

**ADR-0035 constraint, inherited:** instance state only, never a `static`. `Mutex<Inner>`
inside the store, exactly as `SimTikvMetadataStore` does (`support/mod.rs:19-22` explains
why this keeps it outside the global-mutable-state gate).

**4. The hazard Do must not walk into.** `exactly_one_writer_wins_over`
(`crates/dst/tests/concurrency.rs:75`) is the **shared** property body driving *both* redb
and simulated-TiKV, and it calls `.unwrap()` on the commit result (`:106`). An
ambiguity-capable store makes it panic. Do **MUST NOT** weaken that body to accommodate the
new store — redb and simulated-TiKV must keep their strict `.unwrap()`, because for them an
`Err` from `commit` genuinely is a bug. The ambiguity scenario gets its **own** property
body in `crates/dst/tests/commit_ambiguity.rs`: race → collect `Ok`/`Err` outcomes →
**settle every `Err` by re-reading the inode** → then assert exactly-one-winner and
version-bumped-once over the settled set. The new invariant is *"an ambiguous commit is
never assumed either way; after a re-read the outcome is exactly one of committed or not,
and the store's history is consistent with that."*

Run it with the ambiguity nemesis **off** for `sim_fdb_backend_passes_shared_contract` — the
shared contract suite legitimately assumes determinate commits, and arming the nemesis there
would be testing the suite, not the store.

**5. The purity guard.** `crates/dst/tests/no_fdb_linkage.rs` asserts `crates/dst/Cargo.toml`
declares neither `wyrd-metadata-fdb` nor `foundationdb`, with a planted-red proving the
scanner works. This is a *manifest* scan, not a symbol scan: it is cheap, deterministic,
runs in `cargo xtask ci`, and catches the only way `libfdb_c` could enter the sim graph —
someone adding the dependency. (Verified: `crates/dst/Cargo.toml` dev-deps today list
`wyrd-metadata-redb` and `wyrd-metadata-conformance`, never `wyrd-metadata-fdb`. The
acceptance criterion is true *today*; this guard is what keeps it true.)

## Alternatives considered

- **Contract-harness only, no simulated-FDB.** This is the option the issue asks us to
  weigh, and it loses on a checkable basis: **none of `run_all`'s seven `contract_*` clauses
  touches commit ambiguity** (verified by enumerating them at
  `crates/metadata-conformance/src/lib.rs:24,41,60,85,136,167,244`), and none could be driven
  to — the suite runs against redb and the real backends, none of which can produce 1021
  (`crates/metadata-fdb/src/lib.rs:71`). Choosing
  contract-harness-only means the 1021 ambiguity space is verified by **nothing**: not the
  contract suite (no clause), not the fault battery (#442 samples, cannot force), not the
  simulator (absent). The one genuinely new failure shape FDB introduces would ship
  unexercised.
- **Add an `Undeterminable` variant to `CommitOutcome`.** Rejected: it changes a trait
  shared by four implementations, and the driver already expresses ambiguity as `Err` —
  deliberately, with a comment explaining that a `WriteBatch` is not guaranteed idempotent
  (`crates/metadata-fdb/src/lib.rs:67-69`). Modelling something the production code does
  not do would make the simulation *less* faithful, not more.
- **A second simulation framework for FDB.** Explicitly rejected by issue item 4, and by
  the evidence: `#258`/`#447` landed a skeleton (`Fidelity`, `Observations`, a
  trait-generic property body) that was *designed* to take a second parametrization.
- **Run a real `fdbserver` inside DST.** Rejected by ADR-0009 and ADR-0035, and structurally
  impossible: `libfdb_c` owns its network thread.

## Impact & compatibility

- **Test-scope only.** Nothing in `crates/*/src/` changes. No production behaviour, no
  public API, no on-disk format, no dependency added to any shipped crate.
- `crates/dst/Cargo.toml` gains no new dependency (the model is hand-written; that is the
  point of the purity guard).
- `cargo xtask ci` gets slower by one conformance run plus one seed sweep. Acceptable: the
  DST tier is already the long pole and this is the tier's purpose.
- Blast radius a diff-reviewer must hold in view: `support/mod.rs` is imported by three test binaries
  (`conformance.rs`, `concurrency.rs`, and the new `commit_ambiguity.rs`) via
  `#[path = "support/mod.rs"]`. A refactor there touches all three. This is why the
  difficulty is `high` despite the change being test-only.
- Risk: an over-eager nemesis makes the seed sweep flaky. Mitigation — the nemesis fires on
  a **seed-derived** decision, never on wall-clock or thread scheduling, so a failing seed
  replays exactly (ADR-0009); and `Observations` records how often it fired, so a sweep that
  never armed it is visibly vacuous.

## Open questions

1. **For the human at sign-off:** the simulated-TiKV module doc says *"the human ratifies
   that choice at sign-off"* about its fidelity level (`crates/dst/tests/support/mod.rs:24-29`).
   The same ratification is asked here for the simulated-FDB fidelity: **optimistic conflict
   at commit + seed-selected commit ambiguity**, with the *storage* layer modelled as a plain
   `BTreeMap` rather than a versioned MVCC keyspace. This brief's position is that MVCC
   fidelity buys nothing for the trait's contract (the version CAS is a full-value
   precondition, and FDB stores keys/values byte-identically —
   `crates/metadata-fdb/src/lib.rs:873-875`), but the call is yours.
2. Where should the recorded decision live? The issue permits "the issue or a short doc
   note". This proposal is the record; Do should additionally place a condensed version in
   the `SimFdbMetadataStore` module doc, mirroring how the simulated-TiKV decision was
   recorded at `support/mod.rs:1-29`. **No accepted ADR or proposal may be edited**
   (`docs-immutability`; ADR-0037/ADR-0001).

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: The goals (a)(b)(c) and the brief stand; the DST machinery is real and worth keeping. What fails is fidelity: the simulated model provably diverges from the production contract it cites, in two ways the brief never surfaced, and the purity guard does not guarantee what goal (c) claims. Rebuild against the same brief — do NOT re-plan. Sign-off could not ratify §6 T5 ("the human must ratify the fidelity claim") because the claim as built is narrower than advertised. Fix the model, then re-present the ratification with the narrowings disclosed. WHAT IS REAL — keep it, do not churn - The behavioural red->green is genuine: `commit_ambiguity` panics with the pinned message under a wrong observer, and `#[should_panic(expected = "must equal the inode's version bump")]` pins the load-bearing assertion, not any panic. - The both-halves sweep is not fragile: replaying ChaCha8Rng for seeds 0..64 gives 28 landed / 36 not; first landed at seed 0. - The nemesis strikes the intended four-writer `commit_overwrite` CAS by construction (`put_pending`/`sweep_pending` build precondition-free batches; `arm_commit_ambiguity` runs after the fixture), so `obs.ambiguous_commits == 1` holds by construction. - `assert!(conditional)` in the Conflict arm (support/mod.rs:481) is unreachable but correct: `preconditions_hold` is vacuously true for an empty list, which is the FDB behaviour it models. - Scope discipline held: no `Undeterminable` variant, `crates/metadata-conformance/` untouched, no new Cargo dependency, nothing under `crates/*/src/` changed. WHAT TO FIX (four items) 1. The model contradicts the production contract it cites. Production `classify_commit_error` (crates/metadata-fdb/src/lib.rs:213-215, doc at :194) returns `UnknownResult` for 1021/1031 "for *every* batch, conditional or not" — the code/timeout check returns BEFORE the `conditional` check. The model gates the nemesis on `&& conditional` (crates/dst/tests/support/mod.rs:494), so a blind batch can never be ambiguous in the simulator, and the four-phase write protocol's behaviour under an ambiguous pending-ledger put/delete is exercised by nothing. The justification at support/mod.rs:435-438 cites lib.rs:53-56, but that passage says 1021 is never *re-applied* by the blind retry gate — it does not say a blind batch is never ambiguous. The sweep is green BECAUSE the model is narrower than the contract. FIX: drop `&& conditional` and make the blind-batch ambiguity path pass. Expect commit_ambiguity.rs:116 (`write::intent(..).await.unwrap()` -> blind `put_pending`) to panic on the first writer until the protocol is handled. 2. Only the strong half of the ambiguity class is modelled. Production maps 1021 AND 1031 to `CommitUnknownResult` carrying `code`, because (lib.rs:165) "Where 1021 promises the transaction is out of flight, 1031 promises nothing." `SimCommitUnknownResult` is a unit struct with no code, and the settling re-read at commit_ambiguity.rs:131 assumes the ambiguous txn is out of flight — exactly the guarantee 1031 withholds. "The 1021 ambiguity space, searched exhaustively" is really the out-of-flight half of it. FIX: carry the code on `SimCommitUnknownResult` and model 1031 distinctly from 1021. 3. Goal (c) is not met: the purity guard is blind to the renamed-dependency form, which is this manifest's own house style. `scan_line` (crates/dst/tests/no_fdb_linkage.rs:58,77) keys on the text before the first `=`/`.`/whitespace, so `fdb = { package = "foundationdb", version = "0.10", features = ["fdb-7_3"] }` links libfdb_c into every DST test binary and `the_dst_manifest_declares_no_fdb_dependency` (:165) stays green. So does `[dependencies."foundationdb"]` — the section branch never strips the quotes. This is not exotic: crates/dst/Cargo.toml:56,66,68 already declares tonic, etcd-client and tokio in exactly that rename form, in the very file the guard scans. The doc claim at no_fdb_linkage.rs:10 ("there is exactly one way libfdb_c could enter this graph") and brief goal (c) ("mechanically guaranteeing that no libfdb_c symbol is ever reachable") are unwarranted as stated. Also: the guard is over-broad the other way — `foundationdb` is optional behind a default-off `fdb` feature (crates/metadata-fdb/Cargo.toml:11-22), so a bare `wyrd-metadata-fdb` dev-dep trips it without linking libfdb_c at all. And the planted red (no_fdb_linkage.rs:148) is over-fitted: it plants exactly the two shapes scan_line recognises, neither the rename form nor a transitive edge, and pins `version = "0.9"`/`fdb-7_1` while the workspace pins foundationdb 0.10/fdb-7_3 (Cargo.toml:108) — the fixture was not derived from the real dependency. FIX: linkage is a feature-unified GRAPH property, not a manifest-text property. Resolve it from `cargo metadata` (or `cargo tree -e features`) rather than scanning one manifest's lines. Plant the rename form and a transitive edge in the red. 4. The torn-inode assertion is a tautology on exactly the path the file exists to test. When the ambiguous commit landed, the nemesis struck the first accepted CAS, so the sole winner is the `Err` writer — selected at commit_ambiguity.rs:150 by `settled.chunk_map == *chunk_map`. Line :184 then asserts `settled.chunk_map == expected` where `expected` is that same chunk_map: `x == x`, unfalsifiable. It carries content only on the not-landed half, which concurrency.rs:126 already covers for redb/sim-TiKV. The model cannot produce a torn inode anyway — `apply()` runs inside the Mutex guard with no await (support/mod.rs:470-508). `assert!(winners <= 1)` at :163 is likewise unreachable-by-construction. FIX: give brief criterion 2(iii) ("no torn/hybrid inode is ever observable") a real demonstrated red, or withdraw the claim. Do not leave it asserted by a tautology. DISCLOSE AT THE NEXT SIGN-OFF The brief's Open Question 1 asks the human to ratify fidelity w.r.t. MVCC-vs-BTreeMap. It does not surface the blind-batch narrowing (1) or the 1031 narrowing (2). Both belong in that ratification. Put them in build-notes.md so the next sign-off ratifies what was actually built. RECORDED AS A §10 ACT CANDIDATE — not this bundle's to fix The `C4-verify` gates row reads "red without the fix, green with it" while its discriminator here is a `String::contains("pub struct SimFdbMetadataStore")` over support/mod.rs — a file run-verify.sh never compiles in either phase (it runs without `--cfg madsim`). The row would read identically for an empty struct with no MetadataStore impl, no nemesis and no RNG. The brief is candid that this is "a *structural* red"; the gates table is not. Only C4-ci is real evidence for criteria 1-3. That is a harness-wide scanning hazard, not a defect of this patch.
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected on the adversarial reviewer's findings against goal (c) — the libfdb_c purity guard. Rebuild against the same brief; do NOT re-plan. What to fix: 1. The graph-linkage invariant is asymmetric between invocations. Under bare `run-verify.sh` (`cargo test`, no `--cfg madsim`), `cargo tree` omits the entire `[target.'cfg(madsim)'.dev-dependencies]` section — which is exactly the section, and exactly the rename form (`fdb = { package = "foundationdb", … }`), where a real FDB dep would be added in this manifest's house style (crates/dst/Cargo.toml:55). So the invariant the file's title claims is NOT what runs under verify: the bare graph is strictly smaller and blind to the madsim section. Make the guard scan the graph the FDB risk actually lives in (resolve with `--cfg madsim`, or otherwise cover the target-gated section) under BOTH invocations, so goal (c)'s "mechanically guaranteed" claim holds where it runs. 2. No planted red covers the target-cfg-gated rename form. The demonstrated red (no_fdb_linkage.rs:997-1053) plants the rename dep under a PLAIN [dev-dependencies], never under [target.'cfg(madsim)'.dev-dependencies]. So the load-bearing behavior — that the scan actually surfaces a cfg(madsim)- gated FDB node — is assumed, not demonstrated. Plant the red under the madsim-gated section so the guard's ability to catch the real-risk shape is proven, not resting on non-existence. Not blocking, but keep disclosed at the next sign-off: C4-verify's gates row ("red without the fix, green with it") overstates what it verified — under bare verify the behavioural ambiguity property compiles to nothing, so only a structural string-scan goes red. C4-ci is the real evidence. Already a §10 Act candidate; the gates-row wording is a harness matter, not this bundle's to fix.
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: Rebuild against the same brief — do NOT re-plan. The design proposal, the simulated-FDB model, and the linkage guard all stand: the adversarial pass confirmed the iteration-1 findings (torn-assertion tautology, blind-batch ambiguity, 1031-modelled-with-a-code) and the iteration-2 finding (the linkage guard blind to the `--cfg madsim` section / rename form) are all genuinely closed, and checked the model's fidelity against production `classify_commit_error` as matching. Do not churn any of that. What to fix — the two coverage gaps the adversary landed, both advisory but real: 1. Criterion 2(ii) "exactly one writer won" is demonstrated ONLY for 1021. The multi-writer race (`ambiguous_cas_settles_over`, 4 writers) arms only the 1021 nemesis, and the 1031 leg (`timed_out_commit_over`) drives a SINGLE writer — so the two never combine and the resolver-rejection branch of `settle_in_flight` (`support/mod.rs:539`, `deferred_rejections += 1`), whose doc claims "exactly one writer wins survives the deferral," is dead in every test. Add a multi-writer 1031 leg with a demonstrated red: four writers race the version CAS with the 1031 nemesis armed, writer A's timed-out batch left in flight, landing deferred AFTER writer B has already won and bumped the version; assert the model rejects A's stale-precondition batch at deferral so "exactly one winner" survives a 1031 deferral under contention. Do not instead narrow the criterion — the human chose to close the gap, not scope it down. 2. Same root, minor: the 1031/torn observation counters (`deferred_landings`, `deferred_rejections`, `resolver_conflicts`, `torn_applies` in `FdbObservations`) are incremented but asserted by no test, so the brief's anti-vacuity argument ("Observations records how often it fired") does not actually cover the 1031-deferral counters. Assert the relevant counters in the new leg so a sweep that never armed the deferral branch is visibly vacuous. §6 was not cleared (C4-ci full rerun on a loopback-permitting host, T4 prior art, T5 fidelity ratification, fitness-to-purpose) — carry those forward to the rebuild's sign-off. Note the C4-verify structural-red / gates-row wording remains a harness-wide §10 Act matter, already tracked, NOT this bundle's to fix.
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
