# Build notes — issue 257 / m4.6-tier1-jepsen-tier2 (iteration 3)

Target branch: `getwyrd/wyrd @ feat/m4-production-metadata-backend` (worktree
`$PDCA_WORKTREE = /home/eddie/wyrd/wyrd.pdca-wt-l0`). Planning artifact: accepted
proposal **0015** §"DST and tests", §"Crate touch-points", §"Suggested PR sequence" item 6.

## The gating result first

`engine/scripts/run-verify.sh` (the C4-verify per-fix gate) now **PASSes — red without the
fix, green with it**. RED phase: production reverted (`xtask/src/meta_dispatch.rs` removed,
`lib.rs`/`testkit` reverted) → `use xtask::meta_dispatch::…` and
`wyrd_testkit::MetaClusterFaultPlan` no longer resolve → the added flippable test targets
fail to compile → RED. GREEN phase: 10 pure unit tests pass; every tier target compiles and
skips cleanly (ignored). This directly clears the iteration-1 failing gate (C4-verify FAIL:
"test passes without the fix").

`cargo xtask ci` stays deterministically green with no TiKV: the on-Check tests read **only
files this patch adds** (`crates/metadata-tikv/tests/*.rs`, a crate `Cargo.toml`) — never a
`deploy/` artifact — so they cannot panic on a missing/late-landing file. That removes the
iteration-2 C4-ci flap (exit 101), whose cause was `pd_service_exists` hard-reading
`deploy/small-multi-node/docker-compose.yml` and panicking when the cycle worktree predated
#256/#428.

## Carry-forward, point by point

### Iteration-2 gating — C4-ci FAIL (panic / flap) → **fixed by removing external coupling**
The new `xtask/tests/meta_dispatch_orchestration.rs` no longer reads the deploy compose. Its
oracles are self-contained: the routed `Cargo.toml` name + the routed `tests/<name>.rs` file
(both added by this patch), pairwise-distinctness, and a behavioral marker read from the
routed test file. No `unwrap`/`panic!` on any not-yet-landed dependency. The Jepsen scenario
test's "refuse without a nemesis" is behind `#[ignore]` + endpoints-set, so it never fires in
the unprivileged `cargo test --workspace` (which does not pass `--ignored`).

### Adversary #1 — wrong-tier nemesis (PD minority proves nothing) → **fault the data plane**
Confirmed: `deploy/small-multi-node/docker-compose.yml:147` declares a **single** `tikv`
service — the sole metadata replica. Pausing a PD *minority* leaves that node serving CAS
locally, so exactly-one-winner holds trivially. Fixed structurally:
- The `wyrd-testkit` seam is redesigned from a PD-minority plan into a **single-zone cluster
  plan** (`MetaClusterFaultPlan`, `crates/testkit/src/lib.rs`): the **load-bearing** fault is
  a bounded `Pause` of the **`tikv` data node itself** (always present), and a quorum-safe PD
  minority is a *secondary* fault that keeps the TSO quorum. The runner
  (`run_meta_jepsen_with_nemesis`, `xtask/src/faults.rs`) `docker compose pause`s
  `META_DATA_PLANE_SERVICE = "tikv"`, not just PD.
- The on-Check oracle encodes this: `nemesis_pauses_the_data_node_healably_and_keeps_pd_quorum`
  asserts the data-plane target is `tikv` (never a `pd<i>`) and is faulted healably, *and*
  separately that a PD majority survives.

### Adversary #2 — nemesis/load not synchronized → **handshake barrier**
The runner passes the scenario a signal-file path (`WYRD_TIER1_FAULT_SIGNAL`). The scenario
does a warm-up commit, then creates the file **only once its CAS load is live**; the runner
`wait_for_signal`s on the file (bounded, 180 s → hard error on timeout) **before** pausing
`tikv`. So the pause provably overlaps live load — it cannot hit a healed cluster. The load
loop runs 25 s wall-clock, outlasting the ≤8 s pause window.

### Adversary #3 — pause result discarded → **every compose result checked**
`meta_compose(...)` returns `Result` and every `pause`/`unpause` in the nemesis path is
`?`-propagated (no `let _ =` on the fault path). A pause that failed to apply is now a hard
error, so the leg cannot pass with the nemesis un-injected. (Teardown `down` stays
best-effort by design — a failed teardown must not mask a real result.)

### Adversary #4 — Partition≡pause conflation; ClockSkew unwired → **honest fault kinds**
`MetaFault::Pause` is documented as exactly what `docker compose pause` implements (a
container `SIGSTOP`), distinct from `Partition` (a network cut). `MetaFault::window_ms()`
distinguishes **time-bounded** faults (`Pause`/`Latency` → `Some`) from **hold-until-heal**
faults (`Partition`/`ClockSkew` → `None`); the data-node "healable" invariant rests on that
discriminator (`only_time_bounded_faults_have_a_window` proves it is not a tautology on
`Pause`). The container-only default nemesis honestly uses `Pause` (no root); `Partition` /
`Latency` / `ClockSkew` remain the illustrative privileged mechanisms (`tc`/`iptables`/
`libfaketime`) the model names but the root-free CI does not apply.

### Adversary #5 — routing test only checks file EXISTS → **pin each leg to its own scenario**
`the_three_legs_route_to_distinct_scenarios` (pairwise-distinct test names) +
`only_the_jepsen_leg_routes_to_the_nemesis_scenario` (a **behavioral marker**: only the
Jepsen scenario performs the `WYRD_TIER1_FAULT_SIGNAL` handshake). A leg-crossing re-route
(Jepsen → the integration scenario, or vice-versa) lands the marker on the wrong file and
goes red — a crossing the "file exists" check missed.

### Adversary #6 — compounding-loop seed is Markdown, asserted on by nothing → **NEEDS-HUMAN**
A genuine executable red→green DST seed is **impossible within this slice's boundary**: (a) a
DST test that re-derives exactly-one-winner over redb re-proves Tier-0's own atomicity (the
iteration-1 defect, and an invariant violation); (b) a test built on a *copy* of the TiKV
commit protocol passes vacuously (drives a copy, not production — explicitly forbidden); (c)
a real one requires editing the metadata backend (`crates/metadata-tikv/src`), forbidden
here. So the honest artifact is the committed, documented seed registry
`crates/dst/tests/tikv_surfaced_seeds.md` (SEED-0001 await-inside-commit; SEED-0002
data-node-pause recovery, the shape this slice's runner is built to surface). This is exactly
the case the brief carves out as **Known NEEDS-HUMAN #5** — the human judges at sign-off
whether the documented known-gap seed satisfies the DoD bullet, or waits for a live-cluster
discovery. I did **not** manufacture a vacuous atomicity test to fake a green.

## Files changed (path:line on `feat/m4-production-metadata-backend`)

- **`crates/testkit/src/lib.rs`** (+~180, after `:322`) — the `MetaFault` model
  (`window_ms`) + `MetaClusterFaultPlan` single-zone seam (`pd_quorum_safe_max`,
  `from_seed`, `data_plane_fault_is_healable`, `pd_quorum_survives`). Import-light (no
  `tikv-client`), so the whole-tree gate compiles it with no TiKV.
- **`crates/testkit/tests/meta_fault_seam.rs`** (new) — the seam flippable; independent
  oracles `survivors*2>n` (PD majority) + the data-node healable-window invariant.
- **`xtask/src/meta_dispatch.rs`** (new) — pure dispatch + nemesis-plan core (delegates the
  plan to the testkit seam); exposed via the xtask lib target.
- **`xtask/src/lib.rs:18`**, **`xtask/src/main.rs`** — `mod meta_dispatch;` + the
  `meta-integration` / `meta-jepsen` / `meta-tier2` subcommands + usage/doc.
- **`xtask/src/faults.rs`** (after `:549`) — the three privileged runners, the
  data-node-pause nemesis with the load handshake + result-checked compose, and
  `wait_for_signal`.
- **`xtask/tests/meta_dispatch_orchestration.rs`** (new) — the dispatch/nemesis flippable
  (filesystem-resolved routing + distinct-scenario + leg-crossing + data-plane oracles).
- **`crates/metadata-tikv/tests/{tier1_metadata_integration,tier1_jepsen_metadata,tier2_metadata_io}.rs`**
  (new) — the metadata-swap tier targets; `#[ignore]` + clean skip without
  `WYRD_TIKV_PD_ENDPOINTS`, real body only under `--features tikv`. The Jepsen body models
  the honest single-replica-pause Jepsen semantics (Err tolerated as unavailability;
  never a consistency violation; recovery required for non-vacuity).
- **`crates/dst/tests/tikv_surfaced_seeds.md`** (new) — the committed compounding-loop seed
  registry (data; NEEDS-HUMAN #5).
- **`xtask/Cargo.toml`** + **`Cargo.lock`** — the `wyrd-testkit` dep the seam needs.

## Invariants held (brief §"Invariants to hold")

- **Trait untouched:** `crates/traits/src/lib.rs` `MetadataStore` byte-for-byte unmodified.
- **DST keeps correctness authority:** no atomicity re-proved against TiKV; the seed is a
  documented registry, not a decorative redb re-proof.
- **Single-zone only; static endpoints:** the nemesis keeps a PD majority; the client dials
  the published static PD endpoints (`23791..=23793`) per the Deployment-prerequisite note.
- **Gate honesty:** `cargo xtask ci` stays green (and now deterministic) with no TiKV / no
  privileged fault injection — the tier targets skip cleanly.

## Verification run (this worktree)

- `run-verify.sh` (C4-verify): **PASS** — RED (production reverted → E0432 on
  `xtask::meta_dispatch`) / GREEN (10 unit tests pass, tier targets skip).
- `cargo fmt --all -- --check`: clean.
- `cargo clippy -p xtask -p wyrd-testkit -p wyrd-metadata-tikv --all-targets` (`-D
  warnings`): clean.
- `cargo check -p wyrd-metadata-tikv --features tikv --tests`: Finished (the off-Check
  tikv-feature bodies type-check against the pinned `tikv-client`).
- `cargo test -p xtask --test meta_dispatch_orchestration -p wyrd-testkit --test
  meta_fault_seam`: 10 passed.

## Rejected alternatives (with cost)

- **Keep the PD-minority nemesis (iteration 2).** Rejected — the data plane is one `tikv`
  node (`docker-compose.yml:147`), so a PD-minority pause leaves it serving CAS locally and
  proves nothing. Cost of keeping it: the entire Jepsen leg is vacuous under fault (adversary
  #1). The fix relocates the *primary* fault onto the sole data node (a ~1-line target
  change in the runner + the seam's `from_seed` always emitting a data-plane `Pause`), not a
  larger rewrite.
- **Read `deploy/…/docker-compose.yml` in the on-Check test to assert services exist
  (iteration 2).** Rejected — that hard read is exactly what panicked/flapped C4-ci when the
  worktree predated #256. Cost: a non-deterministic gate (the iteration-2 blocker). The
  service-name mapping (`tikv`, `pd<i>`) is a pure constant, unit-tested without any
  filesystem coupling; whether the compose declares them is verified off-Check when the
  runner actually runs.
- **Manufacture an executable DST "promoted regression".** Rejected on boundary, not size:
  its green requires either re-proving redb atomicity (invariant violation), a vacuous copy
  of the backend protocol (forbidden), or editing `crates/metadata-tikv/src` (forbidden
  surface). Committed as a documented seed for #258 + surfaced as NEEDS-HUMAN #5 instead.

## NEEDS-HUMAN (pre-declared)

1. **Privileged-off-Check live green (C2/C4).** The live Tier-1 integration + Jepsen +
   Tier-2 green (docker + root-free `docker compose pause` on the #256 cluster;
   `WYRD_TIER1=1`/`WYRD_TIER2=1`) is confirmed only by the privileged CI/eval Tier job — not
   in the Check worktree. The Check-observable red→green is the dispatch + seam unit tests.
2. **#256 dependency.** The metadata Tier-1 legs need `deploy/small-multi-node` (on the base,
   PR #428). The live job's staging is the human's to confirm.
3. **#365 / L5-discovery reduced bar.** Static endpoints until #365; human confirms the
   reduced-bar posture is acceptable.
4. **Compounding-loop seed provenance (the point the adversary flagged).**
   `crates/dst/tests/tikv_surfaced_seeds.md` is a committed **documented** seed registry, not
   yet a live-cluster discovery (the job that would surface one is off-Check). Human judges
   whether it satisfies the DoD bullet or waits for a live discovery — brief Known
   NEEDS-HUMAN #5. It feeds #258 either way.
5. **Jepsen tooling shape.** Confirmed in-repo (an in-repo Rust scenario driven by `cargo
   xtask meta-jepsen`, mirroring the post-#250 chunkstore Jepsen route), with the nemesis
   applied by the runner via `docker compose pause` of the `tikv` data node, synchronized by
   the load handshake. Noted per the brief's item — not Jepsen-proper/Clojure.
