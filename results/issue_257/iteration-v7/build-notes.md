# Build notes — issue 257 / m4.6-real-commit-over-madsim-tikv (attempt v7)

## Headline: direction (a) is BLOCKED at build → the brief's declared **Option-B** is live

The brief's binding at-Check evidence had two declared shapes:

- **Direction (a)** — drive the real `TikvMetadataStore::commit` over a third-party
  `madsim-tikv-client` deterministic sim, reached by a `Cargo.toml` cfg-alias.
- **Option B** (declared fallback) — if `madsim-tikv-client` doesn't track `tikv-client
  0.4` or doesn't model the percolator write-conflict, move the binding *correctness* bar
  to the live Tier-1 legs and make the at-Check evidence the **pure oracles + the DST seed
  as a coverage/determinism-gap artifact** (no self-authored-correct-branch tautology).

**Confirm-at-build result (NEEDS-HUMAN item (1)):** `madsim-tikv-client` **does not exist
in the registry at all.**

```
$ cargo info madsim-tikv-client
error: could not find `madsim-tikv-client` in registry ...crates.io-index
$ cargo search madsim-tikv-client      # (no results; madsim-tonic returns fine)
```

So condition (1) fails outright — there is no release, let alone one tracking `0.4` with
faithful commit-conflict semantics. Exactly the "thin bet, genuinely likely-to-miss" the
brief flagged (the Rust `tikv-client` is pre-1.0; only the Go client is upstream-stable).
**I did NOT add a cfg-alias** to a nonexistent crate (it would break `cargo xtask ci` and
violate "metadata-tikv/src / Cargo.toml alias-only" by being a broken alias). Per the
brief: *"If any of (1)–(3) fails, take the declared Option-B fallback and say so."* → done.

**Which layer is live, for Check:** the **Option-B** layer. The binding *correctness* bar
is the live Tier-1 legs (off-Check, privileged). The at-Check binding evidence is the two
pure-oracle families + the DST seed-as-coverage-artifact. Grade against Option B.

I confirmed `tikv-client = "0.4.0"` IS the resolved version (cached; `cargo check -p
wyrd-metadata-tikv --features tikv --tests` is green) — so a *hypothetical* madsim shim
would have had to track 0.4; none does.

## What I did NOT do (avoiding the v1–v6 structural defect)

The decisive v6 lesson: a self-authored DST sim whose "correct" branch hard-codes `admit =
commit_point_ok` makes `admitted_stale = commit_point_ok && !commit_point_ok` identically
false — vacuous. **The seed here carries no such self-certifying assertion.** It never
claims "the branch I wrote to be correct is correct." It asserts a *reachability delta*
between two commit *shapes*, with a **negative control** giving it teeth.

## The three at-Check artifacts (red→green, non-tautological)

### 1. DST seed — `crates/dst/tests/tikv_await_commit_interleaving.rs`
A `#![cfg(madsim)]` coverage/determinism-gap artifact (run by `cargo xtask dst`). It models
the two commit *shapes* over madsim's deterministic scheduler:
- **await-inside** (the `TikvMetadataStore::commit` structure, `lib.rs:540–600`: read
  precondition → `.await` network I/O → terminal write): both committers read version `0`,
  cross the await window, and commit — two critical sections **overlap** (`max_in_flight ==
  2`), producing a **lost update** (`version` advances once, not twice).
- **synchronous** (the redb shape `concurrency.rs:3-4` assumes: one write txn, no await
  inside) — the **negative control**: no overlap is reachable (`max_in_flight == 1`), no
  lost update (`version == 2`).

The delta is the evidence: the await window makes a *new* interleaving — the one
`concurrency.rs:3-4` declares impossible — reachable. This is **not** a correctness proof
of production TiKV (that needed direction (a)); it is the honest, checkable fact the brief's
Option-B seed is scoped to: *"the concurrency.rs synchronous-commit rationale is unsound;
here is a newly-reachable interleaving."* Seed-stable across the 50-seed sweep (the two
committers synchronise their reads, so the interleaving is reached under **every** seed).

**Demonstrated behavioural RED (temporary, discarded):** moving the `.await` window to
*after* the commit (so read+write are adjacent — the synchronous shape) makes the overlap
unreachable and the assertion fails behaviourally, still compiling:
```
assertion `left == right` failed: ... two commit critical sections overlap ...
  left: 1   right: 2
MADSIM_TEST_SEED=1783261324396741810
```
This is a behavioural flip tied to the await structure — **not** a compile flip (I first
tried removing the await entirely; that tripped `unused_import` warnings-as-errors, which
would have been an invalid compile-red, so I used the move-after-commit perturbation
instead) and **not** file-absence. Reverted before shipping.

### 2. Pure `xtask` dispatch + fault-effect oracle — `xtask/src/metadata_faults.rs`
Mirrors the `disk_faults` born-at-tier pattern (lib module, tested from `xtask/tests/`).
Tested by `xtask/tests/metadata_faults_orchestration.rs`:
- `metadata_tier_dispatch(legacy)` — routes to the in-repo `tier1_metadata_consistency`
  scenario; the removed external command is representable but never the default (mirrors
  `jepsen_dispatch`). Re-pointing the default at the external route flips the test red
  **behaviourally** (panic arm), not by a deleted-module compile error.
- `partition_took_effect(before, during)` — the Invariant-B fault-effect oracle: RED unless
  the target was reachable **before** and unreachable **during** (catches the exact v6
  asymmetric inbound-only no-op, where the port stayed reachable).
- `heal_is_complete(dropped, healed, reachable_after)` — RED if any dropped port went
  un-healed (the v6 leak: dropped 20162+20182, healed only 20182) or the target stays
  unreachable.
- `metadata_scenario_args` — carries `--features tikv` (the backend is off by default).

Each test asserts an **independent** expectation (a table), not the literal the function
returns; a mutation flips them red.

### 3. `testkit` fault-seam arithmetic — `crates/testkit/src/lib.rs`
Kept from v5/v6 as genuine non-tautological oracles, extended for the metadata swap:
- `quorum` / `partition_outcome` / `partition_materialized` — ≥3-replica Raft quorum
  arithmetic. Makes concrete the Invariant that a *minority* partition against a
  linearizable store **cannot** cause split-brain/lost-update (so "exactly-one-winner goes
  red" is never the binding flip): isolate 1 of 3 → majority (2) writable, minority (1)
  read-only; even split (2 of 4) → no quorum (stalls, never diverges); isolate 0 or all →
  not materialized (the v6 no-op).
- `converged_exactly_once` — version advanced by exactly 1 (not 0, not ≥2).
- `ConsistencySignals` / `consistency_passes` — the ADR-0015 signals
  (`read_after_commit`, `converged_once`) carried **INDEPENDENTLY** (the v6 defect collapsed
  both into one `scenario.is_ok()` bit), plus the `fault_materialized` gate. Each clause is
  independently load-bearing (negating any one fails the verdict).

## Off-Check (BUILT, compiles + skip-cleans in the whole-tree gate; live-green only in the privileged job)
- `crates/metadata-tikv/tests/tier1_metadata_consistency.rs` — drives the **production**
  `TikvMetadataStore` behind the unchanged trait: multi-key atomic **create / rename /
  delete** (v6 only did create/read/duplicate-create), a **symmetric** RAII self-healing
  partition (`SymmetricPartition`: bidirectional INPUT+OUTPUT DROP, heals **every** dropped
  port on **every** path incl. panic, store-port readiness wait, asserts **across** the
  heal), and the **independent** ADR-0015 signals gated by the fault-effect oracle.
- `crates/metadata-tikv/tests/tier2_metadata_io.rs` — real single-node durable
  create/read/CAS/delete cycle.
- `deploy/tikv-multi-replica/docker-compose.yml` — the ≥3-replica TiKV Raft group (PD's
  default replication factor 3 → a one-node partition is always a minority).
- `xtask` runners `run_metadata_tier1/2` + `metadata-tier1/2` subcommands (deferred by
  default; opt-in `WYRD_TIER1`/`WYRD_TIER2`; route via `metadata_tier_dispatch`).

Both scenario files use the skip-clean pattern of `contention.rs` (endpoint-gated `#[test]`
+ `#[cfg(feature="tikv")]` helpers with `not(feature)` stubs), so `cargo test --workspace`
compiles+type-checks them (real API-bound Rust) and they skip; `#[ignore]`d so even under a
configured cluster they run only with `--ignored`. `wyrd-testkit` is a new **dev-dep** of
`metadata-tikv` (not a src change — the trait and `metadata-tikv/src` are byte-for-byte
untouched) so removing the seam breaks the scenarios' compile (born-at-tier).

## Invariants honoured
- **Trait unchanged**, **`metadata-tikv/src` unchanged** — verified: patch touches only
  `metadata-tikv/Cargo.toml` (a dev-dep line) in that crate.
- **No self-authored-sim tautology** — the seed asserts a reachability delta with a
  negative control, never "my correct branch survives".
- **Invariant B** — symmetric partition, heal-every-port RAII, readiness wait, fault-effect
  oracle red-on-no-op, all modelled and unit-tested.
- **DST keeps correctness authority** — the live legs prove "backend matches the store", and
  no leg/seed rests on "exactly-one-winner goes red" (the quorum arithmetic shows why that
  can't flip against a linearizable store).

## Verification run
- `./engine/xtask.sh ci` → **all checks passed** (fmt --check, clippy -D warnings, build,
  test --workspace, machete, deny, conformance, statics, orchestrator-guard, **dst** sweep
  incl. the new seed). `cargo fmt --all` applied.
- Targeted red→green: testkit 16/16, xtask metadata orchestration 5/5, DST seed green
  across the sweep; behavioural RED demonstrated + reverted (above).
- `--features tikv` type-check green (the privileged job's build path).

## NEEDS-HUMAN / pre-declared sign-off items (expected)
1. **`madsim-tikv-client` unavailable → Option B is live** (this note's headline). Human
   accepts the third-party-sim route is unreachable and grades the Option-B bar.
2. **Live Tier-1/Tier-2 green is off-Check** — observable only in the privileged CI/eval
   Tier job (name the confirmer at sign-off). The scenario files are not headless-testable
   here (irreducibly need a ≥3-replica containerized TiKV + privileged iptables); I did NOT
   fabricate a passing stand-in. Manual validation: bring up `deploy/tikv-multi-replica`,
   `WYRD_TIER1=1 cargo xtask metadata-tier1` (and `WYRD_TIER2=1 … metadata-tier2` against
   `deploy/tikv-single-node`).
3. **Metadata-nemesis methodology ADR** — whether the metadata leg needs its own ADR
   refinement (TiKV runs Raft; a live-partitioned node ≠ a paused one) is the
   architecture-board's call; I authored **no** new ADR. The live partition is symmetric.
4. **Static-endpoints reduced bar (#365)** and **Rust-vs-Go client stability (ADR-0008)** —
   noted, out of scope for #257.
