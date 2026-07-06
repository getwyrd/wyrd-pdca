# Build notes — issue 257 / m4.6-real-commit-over-madsim-tikv (iteration 9)

**Withheld from the reviewer.** Rationale for the human at sign-off.

## What iteration 8 accepted, and what it rejected

The iteration-8 carry-forward is narrow and decisive:

> "Rejected on the flagship at-Check seed, not the posture: Option B stays ratified (do NOT
> re-open it), and the pure testkit oracles, xtask dispatch, tier1/tier2 scenario rework
> (must-fixes 1-3), and the no-src/no-traits invariants all survive — keep them."

So this iteration **keeps the entire v8 patch unchanged** except for the three things it
flagged. I started from the v8 `patch.diff` applied verbatim onto
`feat/m4-production-metadata-backend`, then changed exactly:

1. the flagship seed (`crates/dst/tests/tikv_await_commit_interleaving.rs`) — iter-8 must-fix;
2. codex advisory A — `tier1_metadata_consistency.rs` `heal()`/`Drop` lossy-heal;
3. codex advisory B — `xtask/src/faults.rs` metadata runner PD/store readiness.

Nothing else moved. The surviving pieces (testkit quorum/consistency/fault-effect oracles +
their 18 unit tests, the xtask dispatch + its 3 tests, the reworked bidirectional/peer-oracle
Tier-1 scenario, the Tier-2 scenario, the deploy compose, the no-`src`/no-`traits` invariants)
are byte-identical to v8.

## The seed — I took ENFORCED exit (b), because exit (a) is structurally impossible

Iteration 8 enforced a binary choice:

> "(a) bind the seed so a behavioural perturbation of the code under swap (metadata-tikv/src,
> or a seam demonstrably equivalent to its await-inside-commit window) flips it at Check; or
> (b) keep it as pure coverage and rewrite the docstring to claim NO correctness weight and NO
> newly-reachable interleaving."

**Exit (a) cannot be honoured here — two independent, checkable blockers:**

1. **No third-party sim exists.** `cargo search madsim-tikv-client` (run at build) returns no
   release tracking `tikv-client 0.4`. This is the ratified Option-B trigger — there is no
   `madsim-tikv` to cfg-alias the way `crates/chunkstore-grpc/Cargo.toml` aliases `madsim-tonic`.
2. **The store is not injectable.** `TikvMetadataStore` holds a **concrete**
   `tikv_client::TransactionClient` (`crates/metadata-tikv/src/lib.rs:420-421`), constructed
   only by `connect(pd_endpoints)` → `TransactionClient::new(..).await`
   (`crates/metadata-tikv/src/lib.rs:435-436`). It needs a real cluster and is **not generic**
   over a fake client. The only way to drive `commit()` (`lib.rs:540-600`) against an
   in-process percolator model at Check would be to make the client injectable — i.e. **edit
   `crates/metadata-tikv/src`**, which the slice invariant forbids byte-for-byte (proposal
   0015; ADR-0006). iter-8's parenthetical "a seam demonstrably equivalent to its
   await-inside-commit window" is exactly that forbidden edit; there is no seam reachable from
   dev-deps alone.

So a behavioural at-Check flip of the **production TiKV commit code** is unreachable — which is
*precisely why Option B was ratified* and the real ADR-0015-on-TiKV proof lives off-Check. Any
attempt to make the seed "bind to redb but assert teeth" is the third option iter-8 explicitly
forbade (and the v1/v6/v8 shape).

**Exit (b), executed honestly.** The seed is rewritten to say what is true and only that:

- **Renamed** `stale_committer_across_the_await_window_loses_the_commit_point` →
  `redb_overwrite_cas_classifies_the_stale_writer_as_conflict`. The name no longer advertises a
  TiKV await window it doesn't touch.
- The module docs now state **in the first paragraph**: pure redb coverage, **NO correctness
  weight for `TikvMetadataStore::commit`**, a TiKV commit regression **cannot** flip it, and
  **no newly-reachable interleaving** (redb `commit()` is synchronous, so it exhibits nothing
  `concurrency.rs` doesn't already schedule).
- The brief's Option-B line ("assert the `concurrency.rs` rationale is unsound; here is a
  newly-reachable interleaving") is **explicitly conceded off-Check** in the seed's own docs:
  the `concurrency.rs:3-4` "no await inside commit" rationale *is* false for
  `TikvMetadataStore::commit` (`lib.rs:540-600` awaits between `get_for_update` at `:560` and
  `txn.commit().await` at `:597`), but that unsoundness is a redb-vs-TiKV divergence property,
  observable only against a real cluster — the live Tier-1 consistency scenario
  (`tier1_metadata_consistency.rs`, `WYRD_TIER1` job). At Check the seed makes no such claim.
- What it *legitimately* adds over `concurrency.rs` (which only counts winners): it asserts the
  loser is classified **precisely as `CommitOutcome::Conflict`** — honest, redb-only additive
  coverage of the trait's conflict-classification contract, labelled as such.

This is not a duplicate-with-a-lie; it is honest coverage with a docstring that a reader cannot
mistake for a TiKV correctness proof. The **binding at-Check evidence** is — as the brief's
Success criterion §2 says and as v5–v8 kept — the **pure decision-logic oracles**, not the seed.

## The genuine at-Check red→green (the pure oracles)

Demonstrated behaviourally this iteration (temporary, discarded perturbation):
perturbing `wyrd_testkit::quorum` from `total/2 + 1` to `total/2` turns **three** testkit unit
tests RED (`quorum_is_strict_majority`, `majority_partition_makes_the_isolated_side_writable`,
`even_split_stalls_rather_than_diverges`); reverting → GREEN. These use hand-computed
expectations (a quorum table), not the literal the function returns — the non-tautological bar.
`git diff crates/testkit/src/lib.rs` against the staged v8 is empty (perturbation discarded).

## Codex advisory A — non-lossy heal (`tier1_metadata_consistency.rs`)

v8 `heal()` set `self.healed.set(true)` **unconditionally**, then returned `Err` on a partial
heal; the caller's `heal().expect(..)` panicked, and `Drop` saw `healed == true` → skipped
cleanup → leaked host firewall rules. Fixed:

- `healed` is set to `true` **only when every rule was removed** (`first_err.is_none()`). A
  partial heal returns `Err` with `healed == false`, so `Drop`'s panic-safety net still fires.
- Added a `removed: RefCell<Vec<String>>` recording each rule a successful `iptables -D` took
  out. `Drop` now retries **only** the residue (`applied` minus `removed`), so it neither
  double-removes a healed rule nor emits a false "leaked" warning for one that came out cleanly.

## Codex advisory B — PD/store readiness (`xtask/src/faults.rs`)

The metadata Tier runner dialed immediately after `docker compose up -d`. Added
`wait_metadata_cluster_ready(tier)`, called **inside** the `finalize_panic_safe` closure (so a
readiness timeout still tears the stack down): it waits for PD (`127.0.0.1:2379`) and, for
Tier-1, every store port (`METADATA_TIER1_STORE_ADDRS`), reusing the same bounded
`crate::wait_for_port` poll `run_tikv_conformance` uses (`xtask/src/main.rs:339`). A store that
never comes up is a surfaced hard error, not a spurious mid-scenario failure.

## Gate — the authoritative runner

`PDCA_WORKTREE=… ./engine/xtask.sh ci` (delegates `cargo xtask ci`) → **all checks passed**:
fmt, clippy `-D warnings`, build, `cargo test --workspace` (incl. the renamed seed
`redb_overwrite_cas_classifies_the_stale_writer_as_conflict ... ok`, the 18 testkit oracle
tests, the 3 xtask dispatch tests), cargo-deny, conformance vectors. `cargo fmt --all` clean.

## Invariants

- `crates/traits/src/lib.rs` — **untouched** (not in the diff).
- `crates/metadata-tikv/src/**` — **untouched** (only a `[dev-dependencies]` line in
  `crates/metadata-tikv/Cargo.toml`, from v8).
- `crates/core/**` — **untouched**.
- No new ADR (metadata-nemesis methodology stays the architecture board's, per the brief's
  NEEDS-HUMAN).

## Honest limitations for the human (NEEDS-HUMAN)

1. **The seed carries no correctness weight** (ratified Option B). The at-Check binding
   evidence is the pure oracles (demonstrated red→green above); the seed is honest redb
   coverage that a TiKV regression cannot flip. This is exit (b), which iter-8 offered — please
   confirm the labelling now matches (no teeth claimed, newly-reachable-interleaving conceded
   off-Check).
2. **The off-Check Tier-1/Tier-2 legs are unexecuted here** (no privileged Docker host). They
   compile/type-check under `--features tikv`. Manual validation: `WYRD_TIER1=1 cargo xtask
   metadata-tier1` on a Docker host stands up `deploy/tikv-multi-replica`, waits for PD + all
   three store ports, isolates 127.0.0.2 bidirectionally, expects PD to mark it
   Disconnected, the majority to keep committing, a **complete verified heal** (every rule
   removed AND PD sees it Up again — now leak-safe on a heal failure), and the three
   independent ADR-0015 signals green; flip the partition to one-way and the peer-side
   fault-effect oracle must go red. `WYRD_TIER2=1 cargo xtask metadata-tier2` runs the
   single-node I/O cycle. Pre-declared C2/C4 off-Check item — name the Tier-job owner at sign-off.
3. **C4-verify (advisory, `run-verify.sh`)** cannot isolate a per-fix behavioural red for this
   test-only slice: the flippable seed is `#![cfg(madsim)]` (skipped by plain `cargo test`) and
   the oracle fns + their tests are both added by the patch (reverting production removes the
   fns the tests call → a compile diff, not a behavioural one). The genuine behavioural
   red→green is the pure-oracle perturbation shown above under `cargo test --workspace`
   (C4-ci). This posture was ratified at iter-8; it is not re-engineered here.
4. **`madsim-tikv-client` absence** is a build-time `cargo search` claim; the human confirms
   the registry has no release tracking `tikv-client 0.4`.
