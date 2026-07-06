# Build notes — issue 257 / m4.6-tier1-jepsen-tier2

Target branch: `getwyrd/wyrd @ feat/m4-production-metadata-backend` (worktree
`$PDCA_WORKTREE = /home/eddie/wyrd/wyrd.pdca-wt-l0`). Planning artifact: accepted
proposal **0015** §"DST and tests", §"Crate touch-points", §"Suggested PR sequence" item 6.

## What this slice is (and what it deliberately is not)

Proposal 0015's slice 6 extends the **realism-ladder Tier-1 (software faults + Jepsen)
and Tier-2 (single machine)** lines across the redb→TiKV **metadata backend swap**. The
load-bearing green (a live cluster under `tc netem`/`iptables`/cgroup/`libfaketime`) is,
by the brief's own **Verification posture**, **DEFERRED / privileged-off-Check** — exactly
like the pre-existing `tier1_jepsen_consistency` / `tier2_integration` targets, which skip
without `WYRD_TIER1`/`WYRD_TIER2` + endpoints. So the **Check-observable** deliverable is
the pure, unit-testable **xtask runner dispatch/routing** + the **`testkit` real-TiKV
fault-seam decision logic** (brief: "mirroring the existing `jepsen_dispatch` dispatch test
at `xtask/src/faults.rs:179`"), and the tier targets are BUILT (compile in the whole-tree
gate) with their live green observable only off-Check.

It does **not** touch `traits`, `core`, `custodian`, or the metadata-backend commit logic
(brief invariant "the trait stays unchanged" — `crates/traits/src/lib.rs:338` byte-for-byte
untouched; DST keeps correctness authority — no re-proving atomicity against TiKV).

## Changes (path:line on `feat/m4-production-metadata-backend`)

1. **`crates/testkit/src/lib.rs`** — new **real-TiKV fault seam** `MetaFault` +
   `SeededMetaFaults` (partition/latency/pause/clock-skew), the metadata-plane sibling of
   the existing `NetFault`/`SeededNetFaults` (`:152`) and `StorageFault`/`SeededStorageFaults`
   (`:240`). Import-light (no `tikv-client`), so the whole-tree gate compiles it on a
   machine with no TiKV. The **load-bearing decision** is `SeededMetaFaults::quorum_safe_max(n)
   = ⌊(n-1)/2⌋` and `minority()` / `minority_from_seed()`: the Tier-1 nemesis may fault only
   a **strict minority** of a single-zone cluster so a **quorum majority always survives** —
   the premise that makes the single-zone consistency clauses (proposal 0015 §"DST and
   tests"; ADR-0015 clause 2) *testable under fault* rather than trivially unavailable.
   Unit tests: `quorum_safe_max_is_a_strict_minority_that_leaves_a_majority`,
   `seeded_meta_faults_minority_is_reproducible_and_quorum_safe`, `seeded_meta_faults_reports_per_node`.

2. **`xtask/src/faults.rs`** — new **metadata-swap runner dispatch** `MetaTier` +
   `MetaDispatch` + pure `meta_dispatch(tier)`, mirroring the existing `JepsenDispatch` /
   `jepsen_dispatch(..)` value+decision (`:160`/`:179`). Each metadata leg routes to a
   **`wyrd-metadata-tikv`** scenario — never `wyrd-chunkstore-grpc` /
   `tier1_jepsen_consistency` (the M2/M3 repair path). That is precisely the defect the
   brief states: "Today the tree's realism-ladder tiers exercise the M2/M3 chunkstore +
   repair path, not the metadata swap." Three runners `run_meta_integration` /
   `run_meta_jepsen` / `run_meta_tier2` reuse the existing `plan(..)` opt-in gate (`:40`)
   and stand up the `deploy/` stacks (single-node for Tier-2, multi-node for Tier-1). The
   Jepsen runner draws a reproducible, **quorum-safe** nemesis plan through the testkit seam
   (`meta_jepsen_nemesis` → `SeededMetaFaults::minority_from_seed`) and exports it in
   `WYRD_TIER1_NEMESIS_NODES` — wiring the seam into its architectural consumer (proposal
   0015 §"Crate touch-points": the testkit seam is "for the Tier-1 integration + Jepsen runs").

3. **`xtask/src/main.rs`** — wire `meta-integration` / `meta-jepsen` / `meta-tier2`
   subcommands + usage + module doc.

4. **`crates/metadata-tikv/tests/{tier1_metadata_integration,tier1_jepsen_metadata,tier2_metadata_io}.rs`**
   (new) — the metadata-swap tier targets. They drive the **production** `MetadataStore`
   trait (`WriteBatch` multi-key atomic create/rename/delete; N-racer CAS exactly-one-winner
   + read-after-commit linearizability; durability across a fresh client). Structured exactly
   like the existing `conformance.rs`: compile-clean **without** `--features tikv`, skip
   cleanly when `WYRD_TIKV_PD_ENDPOINTS` is unset (so `cargo xtask ci` stays green), real body
   only under `#[cfg(feature = "tikv")]`. Import-light on the default build (no `tikv-client`
   pulled in) — headless-safe.

5. **`crates/dst/tests/tikv_surfaced_regressions.rs`** (new) — the **compounding loop** (a
   DoD bullet, mandatory). Promotes the one redb-unmodeled behavior proposal 0015 §"Pinning
   the trait" names explicitly: the `concurrency.rs` rationale "each `commit()` is internally
   synchronous (no `await` inside)" is **not true of TiKV**, which awaits network I/O
   mid-commit. An `AwaitingStore` decorator yields the madsim scheduler *inside* `commit`,
   admitting the mid-commit interleaving the redb model never exercised, and asserts
   exactly-one-winner + commit-point linearizability **still hold through the trait
   contract** under every seed the sweep explores. `PROMOTED_SEED = 17` is the committed
   regression fixture that feeds the slice-7 / #258 harness (this slice authors the seed, not
   that harness). Runs under `cargo xtask dst` (`#![cfg(madsim)]`), off the `cargo test
   --workspace` gate.

6. **`xtask/Cargo.toml`** — `wyrd-testkit` dep (the seam consumer, item 2). **`Cargo.lock`** —
   the resulting lock delta.

## Red→green proof (Check-observable, load-light, headless)

Both flippables were negated and confirmed **red**, then reverted to **green** — run via
targeted `cargo test` under the Bash-tool timeout (load-light pure units; the real suite is
`cargo test --workspace` inside `cargo xtask ci`):

- **xtask dispatch** — `faults::tests::meta_dispatch_routes_to_the_metadata_swap_not_the_chunkstore_path`.
  Negation: re-point the Jepsen leg at `package: "wyrd-chunkstore-grpc", test:
  "tier1_jepsen_consistency"` (the pre-M4 repair path) → **FAILED** at `faults.rs:923`
  ("must NOT route to the M2/M3 chunkstore path"). Reverted → **ok** (17 passed).
- **testkit seam** — `tests::quorum_safe_max_is_a_strict_minority_that_leaves_a_majority`.
  Negation: `quorum_safe_max(n) = n/2` (a majority-faulting bound) → **FAILED** at
  `lib.rs:627` (n=4 would fault 2, leaving only 2 — not a majority). Reverted → **ok**
  (12 passed).

Gate-parity checks all clean in the worktree: `cargo fmt --all -- --check` (FMT-CLEAN),
`cargo clippy -p xtask -p wyrd-testkit -p wyrd-metadata-tikv --all-targets` (no warnings —
the workspace `-D warnings` policy), `cargo build --workspace --exclude wyrd-dst
--all-targets` (Finished), `cargo-machete` (no unused deps). The new metadata-tikv tier
targets compile on the default (feature-off) build and skip cleanly, so `cargo xtask ci`
stays green with no TiKV and no privileged fault injection (brief "Gate honesty").

## Why this shape (alternatives ruled out, with cost)

- **Test the runner's `cargo test` argv instead of a `meta_dispatch` value.** Rejected —
  that is exactly the iter-6/7/8 tautology the existing `jepsen_dispatch` comment
  (`faults.rs:153-159`) documents: a downstream-argv assertion stays green when the live
  route regresses. Binding the test to the **value the runner consumes on its `Plan::Run`
  path** is what makes re-pointing a leg at the chunkstore path flip red (proven above).
- **Put the fault seam only in the tier test, not `testkit`.** Rejected — proposal 0015
  §"Crate touch-points" names `testkit` as the home of the seam "for the Tier-1 integration
  + Jepsen runs", and a seam local to one test target can't be the reproducible plan the
  xtask runner exports to the test. The `testkit` seam is the single source; the runner and
  (via env) the test both consume it.
- **A richer fault-mechanism model (encode tc/iptables/cgroup/libfaketime as the decision).**
  Rejected — the brief marks component/mechanism identities **ILLUSTRATIVE**; over-fitting the
  pure decision to a mechanism string would be a brittle test of an illustrative detail. The
  binding decision is the **quorum-safe minority** invariant, which is what actually gates
  whether the consistency clauses are observable.

## NEEDS-HUMAN (pre-declared; expected by the brief, not build defects)

1. **Privileged-off-Check live green (C2/C4).** Tier-1 integration + Jepsen + Tier-2 green
   is observable only in the privileged CI/eval Tier job (Docker + root + `tc`/`iptables`/
   cgroup/`libfaketime` + a TiKV cluster) via `WYRD_TIER1=1`/`WYRD_TIER2=1`. Not runnable in
   the Check worktree; confirmed by that job's recorded run. The Check-observable red→green is
   the two pure unit tests above.
2. **#256 dependency (the cluster to fault).** The metadata Tier-1 legs need the `deploy/`
   multi-node TiKV/PD stack. `feat/m4-production-metadata-backend` carries slice 5 / #256
   (`deploy/small-multi-node/`, merged — `git log` shows PR #428), but the live job's
   existence/staging is the human's to confirm. Endpoints are **static** per proposal 0015's
   Deployment-prerequisite note (#365 / L5-discovery is a later slice) — the reduced-bar
   posture the human accepts.
3. **`meta_pd_endpoint` / nemesis node count are "confirm at build" facts.** The multi-node
   PD ports (`127.0.0.1:23791`) and `META_JEPSEN_PD_NODES = 3` mirror
   `xtask/src/main.rs` `SMALL_MULTI_NODE_ENDPOINTS` (`:262`) — but the exact PD member the
   transactional client should dial on the #256 stack is confirmed when the live job first
   runs; they do not affect the Check-observable dispatch/seam decisions.
4. **Compounding-loop seed provenance.** Per the brief's Known NEEDS-HUMAN: the DoD wants a
   *real* discovery promoted. `PROMOTED_SEED` promotes the await-inside-commit interleaving
   proposal 0015 §"Pinning the trait" names — a genuine redb-unmodeled behavior expressed
   through the trait — but whether that satisfies "a real-cluster surprise" or is a
   documented known-gap seed is the human's judgement at sign-off. It feeds #258 either way.
5. **Jepsen tooling shape.** Confirmed in-repo (the post-#250 `JepsenDispatch` routes to an
   in-repo Rust scenario, not an external Clojure/JVM shell-out); the metadata Jepsen leg
   follows suit (`tier1_jepsen_metadata` in-repo scenario). Noted per the brief's item.

## Bundle test file

`tier1_jepsen_metadata.rs` (the brief-named illustrative path) is copied into the bundle.
Its live red→green is privileged/off-Check; the **demonstrable** red→green at Check is the
inline `meta_dispatch` + `quorum_safe_max` unit tests in the patch (proven above).
