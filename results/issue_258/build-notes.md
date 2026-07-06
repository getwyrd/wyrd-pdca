# Build notes — issue 258 / m4.7-dst-pin-second-impl

*Withheld from the reviewer. Rationale, alternatives, and the pre-declared MIXED
verification posture for the human at sign-off.*

## What the brief asked for (the real end result)

M4 is the **second** `MetadataStore` implementation, so DST must **pin and harden**
the trait, not merely use it. Binding success criterion (brief lines 34–48):

- **(a)** the DST drives **both** backends — the deterministic backend and a second
  implementation (a simulated-TiKV model *or* a trait-level contract harness) —
  through the **identical** property suite, green and seed-reproducible;
- **(b)** the `concurrency.rs` determinism rationale (`:3-6`) no longer asserts "no
  await inside" as the ground of determinism, and the exactly-one-winner interleaving
  coverage is revisited for an **await-inside-commit** commit path;
- **(c)** **new seed(s) committed**, including at least one that reproduces forever.
- **Invariant:** Tier-0 DST stays green/seed-reproducible on the deterministic
  backend; the `MetadataStore` trait stays **unchanged**.

The *mechanism* (simulated-TiKV model vs contract harness) is ILLUSTRATIVE. I chose
**both** in the cheapest honest combination (see below): a deterministic
simulated-TiKV **model** (so I can render the await boundary, which a pure
contract-over-redb harness cannot) driven through the **existing shared
`wyrd-metadata-conformance` suite** (so the property suite is provably identical, not
forked).

## What I built (cite `path:line` on the target branch, head `5d87cc4`)

Target branch `feat/m4-production-metadata-backend`. Everything is `tests/`-scope; the
`MetadataStore` trait (`crates/traits/src/lib.rs:337-351`) is untouched, and no
`metadata-tikv`/`server`/`core` code changes.

1. **`crates/dst/tests/support/mod.rs` (new)** — `SimTikvMetadataStore`, the
   deterministic simulated-TiKV *model*. Its commit renders TiKV's 2PC as
   begin(TSO) hop → **atomic prewrite** (pessimistic lock-grab + precondition check,
   one critical section, no await) → **mid-commit `.await`** (commit RPC round-trip) →
   atomic apply+unlock. A `Fidelity` toggle also offers a `SynchronousRedbShaped`
   commit (indivisible, "no await inside") used only for the demonstrated-red twin.
   Instance-only state (no `static`), so it is outside the ADR-0035 global-state gate
   (which scans `src/` only, `xtask/src/main.rs:732`) and cannot leak across seeds.
2. **`crates/dst/tests/conformance.rs` (new)** — drives the **existing** shared
   `wyrd_metadata_conformance::run_all` (`crates/metadata-conformance/src/lib.rs:291`)
   under madsim over **both** redb and the sim-TiKV model. This is criterion (a): the
   same clauses redb and TiKV share out-of-simulator
   (`crates/metadata-redb/tests/conformance.rs`,
   `crates/metadata-tikv/tests/conformance.rs`) now run in-simulator over both.
3. **`crates/dst/tests/concurrency.rs` (rewrite of `:1-9` header + body)** —
   - header rationale corrected: determinism now rests on madsim's seed-reproducible
     scheduler, **not** on "each commit is synchronous, no await inside" (criterion b);
   - the exactly-one-winner race body is refactored into one trait-generic
     `exactly_one_writer_wins_over<M: MetadataStore>` and driven over **both** backends
     (`_redb`, `_sim_tikv`) — the byte-identical property, shared not forked;
   - two **seed-pinned** tests at a committed, reproduces-forever seed
     (`PINNED_INTERLEAVING_SEED = 0x4D34_2E37`, `b"M4.7"`): the await model reaches a
     schedule where a writer's prewrite observes another writer **mid-commit** *and*
     exactly one still wins; the synchronous twin proves that schedule is unreachable
     for an indivisible commit (criteria b + c).
4. **`crates/dst/Cargo.toml` / `Cargo.lock`** — add `wyrd-metadata-conformance` as a
   dev-dep (dev-scope; no shipped-surface change).

## Red → green (the flippable regression; MIXED posture, pre-declared)

The brief (Test file, lines 91–101; Disposition hint, 111–114) predicted a **MIXED**
posture: there is no pre-existing *production* hunk to revert, because the "second
implementation" is itself a DST model. So the honest red→green lives **inside** the
test, as the demonstrated-red discipline the repo already uses for load-bearing
properties (`crates/metadata-conformance/tests/demonstrated_red.rs`):

- `sim_tikv_reaches_mid_commit_interleaving_and_one_wins` — **GREEN**: the
  await-inside-commit model, at the pinned seed, records ≥1 mid-commit observation
  (`obs.mid_commit_lock_conflicts >= 1`) and exactly one winner.
- `synchronous_redb_shaped_commit_never_reaches_the_interleaving` — the **RED** twin
  (`#[should_panic]`): the *same* seed, *same* race, but the redb-shaped synchronous
  commit is indivisible, so it records **0** mid-commit observations and the `>= 1`
  assertion panics. This is exactly the redb-shaped assumption the old header stated
  as the ground of determinism — shown here to be the thing that *hides* the
  interleaving.

Together these are a genuine red→green on the corrected coverage: flipping the model
from "no await inside" (redb-shaped) to "await inside" is what makes the schedule
reachable, and the CAS still yields one winner. Verified via the project runner
`./engine/xtask.sh dst`:

```
test synchronous_redb_shaped_commit_never_reaches_the_interleaving - should panic ... ok
test sim_tikv_reaches_mid_commit_interleaving_and_one_wins ... ok
test exactly_one_concurrent_writer_wins_sim_tikv ... ok
test exactly_one_concurrent_writer_wins_redb ... ok
... tests/conformance.rs: sim_tikv_backend / redb_backend ... ok
```

Full gate `./engine/xtask.sh ci` (fmt + clippy -D warnings + build + test incl. DST
50-seed sweep + cargo-deny + conformance) exits 0 — commit-ready for the target's own
hooks. (First run flagged a `cargo fmt` diff on one call; fixed and re-run clean.)

Note for the C4-verify harness: because the change ships no separate production hunk,
per-fix verify degrades to green-only for the outer property tests; the RED evidence
is the `#[should_panic]` twin above, which is itself the reverted-model case.

## Fidelity choice — the crux (NEEDS-HUMAN, do NOT resolve unilaterally)

Issue #264 / proposal 0015 lines 798–801: *how faithfully* a simulated-TiKV must model
2PC/TSO interleavings vs a trait-level contract harness is an **explicitly open** M4
design point. I **propose** and **demonstrate** one fidelity level; the human ratifies:

- **Proposed level:** pessimistic-lock-at-an-atomic-prewrite. The commit spans two
  await boundaries (begin/TSO and commit RPC), but the *winner decision* is a single
  indivisible prewrite lock-grab — the Percolator/TiKV pessimistic-transaction shape.
  This is enough to make the mid-commit interleaving **reachable** (a loser's prewrite
  hits the held lock) while keeping exactly-one-winner true.
- **What it deliberately does NOT model** (for the human to weigh): async-commit/1PC,
  TSO clock skew, prewrite/commit *partial* failures and lock-resolution, and
  optimistic (non-locking) transactions. A stricter harness could model an optimistic
  read-then-write with no lock — which would (correctly) surface *two* winners and thus
  be a bug-finder rather than a pin. I left that out because M4.2 pins TiKV's commit as
  **pessimistic** (`get_for_update`, proposal 0015 lines 683–689); modeling the
  optimistic path would assert behaviour M4 explicitly does not use.
- **Interleaving-coverage adequacy** (second NEEDS-HUMAN, brief 144–148): the
  exactly-one-winner invariant holds under CAS regardless; my claim is that the await
  boundary makes a *new* schedule reachable (a writer observing another **mid-commit**,
  not merely before/after it), and the pinned seed demonstrates it. Whether that is the
  *required* new coverage or a corrected-comment-plus-illustration is the human's call.

## `tikv-client` `Send + Sync` (confirm-at-build, brief 149–151)

**NOT asserted as a verified backend fact.** The model's futures are `Send + Sync`
because the trait requires it and the model is trivially so; this does **not** confirm
that the real `tikv-client` transaction futures are `Send + Sync` for the object-safe
simulator-driven trait. That remains a build-time confirmation against the pinned
`tikv-client` (the standing #253 lesson), out of this slice's scope.

## Alternatives considered / ruled out

- **Trait-level contract harness only (no model), driving redb through the shared
  suite.** Rejected as *insufficient for criterion (b)*: redb's commit is atomic, so a
  contract-over-redb harness structurally **cannot** render an await-inside-commit
  boundary — the exact thing the brief says must be revisited. Cost of the gap: the
  redb-shaped rationale would be corrected in prose but never *demonstrated* wrong. The
  model adds 244 lines (`support/mod.rs`) but is the only way to make the interleaving
  reachable and the demonstrated-red bite.
- **Modeling the optimistic (non-locking) commit as the primary model.** Rejected: it
  would report two winners (a real bug shape), not pin the *pessimistic* semantics M4.2
  actually ships. Kept the concept only in prose as the "stricter fidelity" the human
  may want; the `SynchronousRedbShaped` twin is the demonstrated-red instead, because
  it isolates the *await-boundary reachability* claim without asserting un-shipped
  behaviour.
- **A `metadata-tikv`-hosted or containerized TiKV in DST.** Rejected outright — ADR-0009
  forbids it and it breaks seed determinism (proposal 0015 lines 484–499, 600–603).
- **Explicit seed pinning via `Runtime::with_seed_and_config`** (vs relying on
  `#[madsim::test]`'s `MADSIM_TEST_NUM` sweep). Chosen for the two interleaving tests
  because `#[madsim::test]`'s base seed defaults to the wall clock
  (`madsim-0.2.34` builder: `MADSIM_TEST_SEED` → seconds-since-epoch), so a swept test
  is **not** reproducible run-to-run. A pinned `#[test]` runtime is the "reproduces
  forever" seed criterion (c) demands. The `_redb`/`_sim_tikv` exactly-one-winner tests
  stay `#[madsim::test]` so they also ride the 50-seed sweep.

## Invariants — all held

- Tier-0 DST green + seed-reproducible on the deterministic backend: all 10 custodian
  + 5 network + `_redb` tests pass under the 50-seed sweep.
- `MetadataStore` trait unchanged (`crates/traits/src/lib.rs` untouched in the diff).
- No real/containerized TiKV in DST; model is in-memory and deterministic.
- Property suite identical across backends: both call the same
  `wyrd_metadata_conformance::run_all` / the same `exactly_one_writer_wins_over`.
- A bug-finding/interleaving seed reproduces forever: fixed `PINNED_INTERLEAVING_SEED`.

## STOP discipline

No PR pushed, opened, or marked ready. Patch is against
`feat/m4-production-metadata-backend`; the slice branch would PR **into** that base,
not `main`.
