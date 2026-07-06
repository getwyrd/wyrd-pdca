# check-advisory-adversary.md — issue 257 (m4.6-real-commit-over-madsim-tikv)

Skeptic's pass on `patch.diff` vs `brief.md` / `check-gates.json`, grounded on
`$PDCA_TARGET` (`/home/eddie/wyrd/wyrd.pdca-wt-l0`, patch applied on
`feat/m4-production-metadata-backend` @ 5d87cc4).

- NEEDS-HUMAN — **The C4-verify "red without the fix" is a compile flip, the exact shape the
  brief forbids.** `engine/scripts/run-verify.sh:259-266` reverts production files and `rm`s
  added non-test files — including `xtask/src/metadata_faults.rs` — while keeping
  `xtask/tests/metadata_faults_orchestration.rs`, whose `use xtask::metadata_faults::…`
  (`xtask/tests/metadata_faults_orchestration.rs:18-20`) then fails to **compile**. That
  unresolved-import error is the only red: in the green phase the DST seed executes zero tests
  (`crates/dst/tests/tikv_await_commit_interleaving.rs:45` is `#![cfg(madsim)]` and run-verify
  runs plain `cargo test`, no `--cfg madsim`), and `tier1_metadata_consistency.rs:269` /
  `tier2_metadata_io.rs:597` are `#[ignore]`d + endpoint-gated (0 tests run). So the
  `check-gates.json:46` claim "run-verify.sh: PASS — red without the fix, green with it" is
  literally true but is a **compile-seam over a deleted module**, precisely the v3 rejection
  ("the binding on-Check flip is behavioural, not a compile flip", brief.md:292) and the shape
  `xtask/src/faults.rs:767-768` itself disavows. No behavioural red→green was demonstrated by
  the gates; the reviewer's acceptance of C4-verify as evidence is the claim I judge unwarranted.

- NEEDS-HUMAN — **The "symmetric" partition is one-way, the mirror image of the v6 defect
  Invariant B forbids.** `SymmetricPartition::apply`
  (`crates/metadata-tikv/tests/tier1_metadata_consistency.rs:288-307`) adds
  `iptables … --dport 20161 -j DROP` on **both INPUT and OUTPUT** — but on the host-network
  loopback stack (`deploy/tikv-multi-replica/docker-compose.yml`, `network_mode: host`) both
  rules match the *same direction*: packets **addressed to** tikv-1's port. tikv-1's own
  outbound connections (dport 2379 to PD, 20160/20162 to peers) and its replies on established
  connections (sport 20161, ephemeral dport) are untouched. Concrete failing case: peers' raft
  messages to tikv-1 are dropped while tikv-1 keeps transmitting raft messages / PD heartbeats
  to the majority — a receive-only blackout, not the bidirectional isolation the doc comment
  claims ("isolated both ways", :288-289) and the brief mandates (brief.md:140-148). Chain
  count ≠ direction; symmetry needs `--sport 20161` rules at minimum (and tikv-1's
  client-side ephemeral ports are indistinguishable on a shared netns at all).

- **The fault-effect oracle cannot detect the previous finding.** `fault_materialized`
  (`tier1_metadata_consistency.rs:114-127`) is
  `partition_materialized(3, 1) && before && during`: the first conjunct is constant-true
  arithmetic over env vars the runner hardcodes (`xtask/src/faults.rs:1146-1149`), and
  `before`/`during` probe TCP connect **from the test process to dport 20161** — which the
  OUTPUT rule guarantees to fail. The oracle verifies the rule blocks *the probe*, not that
  the node is isolated: a one-way (or even probe-only) cut passes the Invariant-B gate the
  brief says must be red for a no-op fault.

- **The unit-tested pure oracles are dead code w.r.t. the leg they certify.**
  `partition_took_effect` (`xtask/src/metadata_faults.rs:109`) and `heal_is_complete`
  (`xtask/src/metadata_faults.rs:119`) are consumed by **nothing** except their own tests
  (verified by grep over the target): the Tier-1 scenario inlines `before && during`
  (`tier1_metadata_consistency.rs:124`) instead of calling the tested function, and the RAII
  heal (`:310-332`) records no dropped/healed sets, never probes reachable-after-heal, and
  swallows `iptables -D` failure (`let _ =`, `:334-336`) — so a failed heal leaks host state
  silently, the very thing `heal_is_complete` was minted to catch. The module-doc claim that
  regressing "the fault-effect oracle" flips tests RED in `cargo xtask ci`
  (`xtask/src/metadata_faults.rs:15-19`) is unwarranted: regressing the *wired* inline check
  flips nothing at Check; regressing the *tested* function changes no runner behaviour. This
  is the v1 "computed, never applied" shape, applied to oracles.

- NEEDS-HUMAN — **The Option-B seed asserts a self-authored toy, and nothing can flip it.**
  `crates/dst/tests/tikv_await_commit_interleaving.rs` imports only `std` + `madsim` (no
  `use wyrd_*` anywhere — it never touches `TikvMetadataStore::commit` at
  `crates/metadata-tikv/src/lib.rs:540-600` nor any third-party model). Both halves are true
  **by construction**, not by schedule: `commit_synchronous` (`:106-110`) holds the `Mutex`
  across read+write, so `max_in_flight > 1` is unreachable by `std::sync::Mutex` semantics;
  `commit_await_inside` (`:95-100`) sleeps 1 ms, so overlap is guaranteed (task b is runnable
  before sim-time can advance). No madsim seed and **no production regression** can flip any
  assertion (`:133`, `:142`) — if the real commit were refactored to remove the await window,
  the seed stays green while its stated premise rots. Whether "a newly-reachable interleaving"
  demonstrated on a hand-authored `KeyModel` (rather than on any code in the tree) meets the
  Option-B bar, or is the v6 "proving a deliberately-broken, patch-authored branch is broken"
  shape re-entered (brief.md:296-303), is the human's call. (The Option-B **trigger** itself I
  could not refute: `cargo search` confirms no `madsim-tikv-client` release exists.)

- **Dispatch flippability is part tautology, part manufactured history.** The dispatch test
  asserts `test == METADATA_TIER1_SCENARIO_TEST` — the very constant the function returns
  (`xtask/src/metadata_faults.rs:61,91-101`; `xtask/tests/metadata_faults_orchestration.rs:33-36`):
  renaming the constant to a nonexistent test target stays green until the privileged job.
  And unlike the `jepsen_dispatch` precedent, whose `ExternalCommand` route reproduces a
  genuinely removed pre-#250 shell-out (`xtask/src/faults.rs:144-147`),
  `WYRD_TIER1_METADATA_CMD` never existed in this repo — the "regression" the routing test
  guards against is a route this patch invented so it could be tested. The enum-variant flip
  is real; the claimed parity with the jepsen precedent overstates it.

- **`converged_once` is near-redundant where it is measured.** The rename commit is awaited
  to `Committed` **before** the heal (`tier1_metadata_consistency.rs:134-152`), so
  `converged_exactly_once(version_before, version_after)` (`:163`) re-derives what the
  `assert_eq!(rename, Committed)` at `:146-148` already implies for a single awaited CAS.
  The ADR-0015-interesting case — a commit **in flight across** the partition/heal boundary —
  is never exercised; "asserted across the heal" holds for the *reads* only. Secondary: with
  the region leader on the black-holed port, the awaited commit at `:134-145` stalls on raw
  TCP timeouts (DROP, no RST) — an off-Check hang/flake risk in the privileged job.

**Attempted and could not refute:** (1) the Option-B trigger — no `madsim-tikv-client`
tracking `tikv-client 0.4` exists in the registry; (2) the `testkit` quorum/materialization
arithmetic (`crates/testkit/src/lib.rs:751-825`) — its tests use hand-computed quorum tables,
genuinely independent of the functions under test, and `consistency_passes` keeps the three
signals independently load-bearing; (3) invariant "no `metadata-tikv/src` / `traits` edit" —
the patch touches neither (only a dev-dependency added to `crates/metadata-tikv/Cargo.toml:45`);
(4) the seed IS executed at Check via `cargo xtask ci` → `run_dst` under `--cfg madsim`
(`xtask/src/main.rs:825,836-857`), so it is compiled and run by the gating oracle even though
C4-verify never runs it.
