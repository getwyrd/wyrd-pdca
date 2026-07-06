# Drive both metadata backends through the simulator to pin the trait

> One logical change: extend the deterministic concurrency/contract tests to run
> over a second `MetadataStore` implementation inside the simulator.

## Summary
**User impact:** Wyrd is adding a production TiKV metadata backend alongside redb.
A TiKV `commit()` awaits on network I/O part-way through, so a second writer can be
scheduled *inside* another writer's in-progress commit — a class of concurrency
schedules the redb backend (whose commit is one atomic write transaction) can never
produce. Until now the deterministic simulator only ever drove redb, so those
interleavings had **no automated coverage**: a future regression in commit ordering
could let two concurrent writers both "win" (lost updates / corrupted metadata) and
slip through the test suite unnoticed. The concurrency module even documented "no
await inside" as the reason the tests were sound — which is untrue for a backend that
awaits on the network.

This PR drives the **identical** shared property and contract suite through **both**
backends inside the deterministic simulator, corrects that rationale, and adds a
committed seed that reproduces the mid-commit interleaving forever.

## What to look at
- `crates/dst/tests/conformance.rs` — the shared `wyrd-metadata-conformance` contract
  suite (`crates/metadata-conformance/src/lib.rs:291`) now runs under the simulator
  over both the redb backend and a deterministic simulated-TiKV model. Same clauses,
  no fork.
- `crates/dst/tests/concurrency.rs` — the exactly-one-winner race is now written
  against the `MetadataStore` trait and driven over both backends, plus two
  seed-pinned tests at `concurrency.rs:162-193`.
- `crates/dst/tests/support/mod.rs:420-462` — the crux: the simulated-TiKV model's
  commit renders TiKV's 2PC (begin/TSO hop → atomic prewrite lock-grab → **mid-commit
  await** → atomic apply). This is a small in-memory test model, **not** a real or
  containerized TiKV (which is deliberately kept out of the simulator).
- Reproduce: `cargo xtask dst` (sets `--cfg madsim`; the tests compile to nothing
  without it), or the full gate `cargo xtask ci`.

## Root cause
The deterministic simulator exercised only the redb backend, whose commit is a single
indivisible transaction, so the scheduler could only reorder *whole* commits — a
strictly smaller schedule space than a backend that awaits mid-commit. The
concurrency module's comment mistook that redb-specific property for the general
ground of determinism, leaving the await-inside-commit interleavings both unmodelled
and undocumented.

## Fix
Add a deterministic simulated-TiKV `MetadataStore` model whose commit awaits on
(simulated) network I/O mid-flight, and drive the existing shared contract suite and
the exactly-one-winner race over **both** it and redb through the same code paths
("shared, not forked"). Rewrite the module rationale to rest determinism on madsim's
seed-reproducible scheduler rather than on atomic commits. Commit a fixed seed
(`PINNED_INTERLEAVING_SEED`, `concurrency.rs:58`) that replays the mid-commit
interleaving on every run. Exactly one writer still wins because the decision is taken
at an atomic prewrite lock-grab, not across the await. The `MetadataStore` trait
(`crates/traits/src/lib.rs:338-350`) is unchanged and every addition is test-scope.

## Verification
- **Claim:** The identical property/contract suite runs green over both backends
  inside the deterministic simulator.
  **Checked:** `crates/dst/tests/conformance.rs` and `crates/dst/tests/concurrency.rs`
  drive the shared `wyrd_metadata_conformance::run_all`
  (`crates/metadata-conformance/src/lib.rs:291`) and the trait-generic
  `exactly_one_writer_wins_over` over redb and the simulated-TiKV model — the same
  clauses redb and TiKV share out-of-simulator
  (`crates/metadata-redb/tests/conformance.rs`,
  `crates/metadata-tikv/tests/conformance.rs`, the latter endpoint-gated). Green across
  the 50-seed sweep under `cargo xtask ci`.

- **Claim:** The await-inside-commit interleaving is genuinely reachable, and the
  coverage is load-bearing — not a tautology.
  **Checked:** `crates/dst/tests/concurrency.rs:162-193` — the regression test
  `sim_tikv_reaches_mid_commit_interleaving_and_one_wins` asserts a writer's prewrite
  observes another writer mid-commit at the pinned seed; its twin
  `synchronous_redb_shaped_commit_never_reaches_the_interleaving` (`#[should_panic]`)
  runs the same seed and same race against an indivisible, redb-shaped commit and
  **fails to reach** that schedule. Flipping the model from "no await inside" to "await
  inside" is exactly what makes the interleaving reachable while CAS still yields one
  winner.
  **Test:** `crates/dst/tests/concurrency.rs` — the two seed-pinned tests are the
  red→green: the redb-shaped (synchronous) fidelity is the reverted case that fails the
  `>= 1` observation assertion, the await-inside fidelity passes it. These tests require
  `--cfg madsim` (`cargo xtask dst` / `cargo xtask ci`); a plain `cargo test` builds
  none of `crates/dst`'s simulator tests.

- **Claim:** The Tier-0 spine is not regressed and the trait is not evolved.
  **Checked:** the redb exactly-one-winner and contract tests stay green across the
  50-seed sweep; `crates/traits/src/lib.rs:338-350` is untouched; no real/containerized
  TiKV is introduced into the simulator (the model in
  `crates/dst/tests/support/mod.rs` is in-memory and seed-reproducible).

## Open question for reviewers
The simulated-TiKV model renders TiKV's commit at one level of fidelity —
pessimistic-lock at an atomic prewrite (`crates/dst/tests/support/mod.rs:420-462`),
matching the pessimistic (`get_for_update`) commit path this backend ships. It does
not model async-commit/1PC, TSO clock skew, partial prewrite/commit failures, or the
optimistic (non-locking) path. Whether this fidelity is faithful enough for the pin is
the open design point tracked in #264; reviewer judgment on that is welcome. Separately,
confirming that the real `tikv-client` transaction futures are `Send + Sync` for the
object-safe trait remains a build-time check against the pinned client, out of scope
for this test-only model.

Fixes #258
