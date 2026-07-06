# Check — adversarial review (issue #257, iteration 3) — advisory, non-gating

Skeptic's pass over `patch.diff` / `brief.md` / `check-gates.json`, grounded on the target
source at `/home/eddie/wyrd/wyrd.pdca-wt-l0`. The two genuinely-independent oracles from
iteration 2 survived scrutiny; the load-bearing *behavioral* evidence still does not.

## Refutations a human must adjudicate

- **NEEDS-HUMAN — The Jepsen leg's headline property (`exactly-one-winner`) cannot fail on
  this topology, so binding criterion (b) is not demonstrable and verges on the forbidden
  re-proof.** `crates/metadata-tikv/tests/tier1_jepsen_metadata.rs:170` asserts
  `winners <= 1`, but the metadata store is a **single** TiKV node
  (`deploy/small-multi-node/docker-compose.yml:147` declares exactly one `tikv` service) and
  the injected fault is a `docker compose pause tikv` — a whole-node `SIGSTOP`, **not a
  partition**. A single serializing node can *never* admit two CAS winners regardless of
  concurrency or the pause: while paused every op `Err`s (skipped by the
  `continue` at `:181`), while up TiKV's Percolator txn serializes them. So `winners <= 1`
  is true by construction — it cannot go red. Brief binding condition (b) demands
  "exactly-one-winner **under genuine concurrency** / **real partition**"; this leg creates
  no partition and the property is unfalsifiable here. Worse, the only bug it *could* catch
  (a non-atomic CAS in `wyrd-metadata-tikv`) is precisely the atomicity the brief's invariant
  says "M4 must NOT re-prove … that is DST's job." The leg is caught between an impossible
  partition and a forbidden re-proof. Human must judge whether a single-node pause satisfies
  the Tier-1 Jepsen success criterion.

- **NEEDS-HUMAN — The compounding-loop DoD is still satisfied only by a Markdown doc; the
  "seeds" are asserted on by nothing.** `crates/dst/tests/tikv_surfaced_seeds.md:40,59`
  register seeds `17`/`29` as `status: known-gap` prose. Iteration 1 explicitly required
  `PROMOTED_SEED=17` be *asserted on, not just eprintln'd*, and iteration 2 required "a
  committed **executable** DST regression … or explicit human sign-off." This iteration
  removed even the `eprintln` and ships pure documentation — no `.rs` references seed 17 or
  29. The brief's mandatory (non-optional) DoD bullet ("promoted back into DST as a new
  seeded regression, with the seed committed") is met by a registry file only. This is
  Known-NEEDS-HUMAN #5; a human must decide the known-gap doc suffices, because deterministic
  evidence for it does not exist.

## Unwarranted claims in the patch

- **`crates/metadata-tikv/tests/tier1_metadata_integration.rs:14-15` claims the Check build
  "COMPILES and type-checks the body — an API regression on `MetadataStore` would fail to
  build." This is false.** The real body (`fn run`) and `TikvMetadataStore` itself live under
  `#[cfg(feature = "tikv")]` (`crates/metadata-tikv/src/lib.rs:299` gates the whole `store`
  module), and `cargo xtask ci` never enables that feature (default = `[]`;
  `crates/metadata-tikv/Cargo.toml`). At Check the `#[cfg(not(feature="tikv"))]` stub compiles
  instead, so a signature change to `MetadataStore::commit/get`/`WriteBatch` would **not**
  break the Check build. The same over-claim recurs in `tier1_jepsen_metadata.rs` and
  `tier2_metadata_io.rs`. The tier bodies get zero Check-time type-safety net.

## Attacks on the C4-verify red→green

- **The demonstrated flip is compile-level, not behavioral.** `C4-verify` is marked `pass`
  because reverting the fix deletes the imported symbols — `use wyrd_testkit::MetaClusterFaultPlan`
  (`crates/testkit/tests/meta_fault_seam.rs`) and `use xtask::meta_dispatch::…`
  (`xtask/tests/meta_dispatch_orchestration.rs`) fail to *resolve*, so the targets go red by
  **failing to compile**, not by a failing assertion. Any symbol removal would flip it. The
  genuine independent oracle (`survivors * 2 > n` at `meta_fault_seam.rs:56`, floor-values at
  `:75`) **does** survive scrutiny and would catch a logic mutation (e.g. `n/2`), but the
  gate never exercises it: a mutation to `pd_quorum_safe_max`'s *body* (not its existence)
  was not shown to go red. The flippable proof is real but weaker than "the logic is
  correct."

## Lesser findings scoped to this diff

- **Dead nemesis wiring.** `xtask/src/faults.rs:834` exports `WYRD_TIER1_NEMESIS_NODES` to the
  scenario subprocess, but no code reads it (grep: sole writer, zero readers;
  `tier1_jepsen_metadata.rs` reads only `WYRD_TIKV_PD_ENDPOINTS` and `WYRD_TIER1_FAULT_SIGNAL`).
  The comment at `faults.rs:833` calls it "diagnostic reproduction," but the PD-minority plan
  is never consumed by the scenario — only the runner's own `docker compose pause` uses it.
  Vestigial; harmless but misleading.

- **The dispatch "resolves to a runnable target" oracle does not check the feature it runs
  with.** `xtask/tests/meta_dispatch_orchestration.rs` verifies the `--test` file exists and
  the package name matches `Cargo.toml`, but the real runner invokes
  `cargo test --features tikv` (`faults.rs`). A route naming a real file in a crate that
  lacked the `tikv` feature would pass this test green yet error at run. Low value given the
  fixed single target crate, but the "filesystem is the oracle, not a mirror" claim is only
  partial.

## Attempted but could not refute

- The `testkit` seam oracles (`pd_quorum_safe_max` → `survivors*2>n`, `window_ms`
  discrimination between `Pause/Latency` and `Partition/ClockSkew`, seed reproducibility and
  seed-drives-schedule) carry genuine, implementation-independent inequalities and run
  un-`#[ignore]`d at Check. Tried to find a seed or `n` where the invariant passes vacuously
  or the oracle restates the impl — could not.
- The determinism fix holds: the on-Check `meta_dispatch_orchestration.rs` reads only files
  this patch adds (never a `deploy/` compose), and the `#[ignore]`d tier tests never run
  under `cargo xtask ci` (no `--ignored`), so the iteration-2 flapping panic path is closed.
- The trait invariant holds: the diff does not touch `crates/traits/src/lib.rs`.
