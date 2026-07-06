# Result — issue 257 / m4.6-real-commit-over-madsim-tikv

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: 
- Success criterion: three layers — one BINDING and demonstrable **at Check**, one BINDING but
- Repo + branch target: getwyrd/wyrd @ `feat/m4-production-metadata-backend`
- Scope (one logical fix) / out of scope: (i) add the **`madsim-tikv-client` cfg-alias** to `crates/metadata-tikv/Cargo.toml`

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix (implement — accepted-plan test-evidence slice behind Accepted
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it.
- C5 Causal adequacy: none — reviewer + human sign-off

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 Contribution: none — (no gate configured)
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

# Check review — issue 257 / m4.6-real-commit-over-madsim-tikv

**Task under review:** extend Tier-1/Tier-2 test evidence across the redb→TiKV metadata-backend swap and author the await-inside-commit DST seed (proposal 0015 PR-sequence item 6) — the builder took the brief's **declared Option-B fallback** (`madsim-tikv-client` reported nonexistent at build, patch.diff:42-51), so the seed is a determinism-gap coverage artifact and the binding correctness bar moved to the off-Check privileged Tier-1 legs.

**Reviewer grounding caveat:** `$PDCA_TARGET` was unresolvable in this sandbox and all network/env probes were blocked, so per protocol every citation below grounds on `patch.diff` alone; the deterministic gate record (`check-gates.json`: C4-ci PASS, C4-verify red→green PASS) is the mechanical witness I could not independently re-run. This is a target-state caveat, not a patch defect.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Patch matches the brief's Option-B contract exactly as pre-declared (brief.md:81-90): seed-as-coverage-artifact with the fallback declared where Check reads it (patch.diff:42-51), surviving pure oracles kept and extended, live legs built-but-deferred. Minor deviation for the human's awareness: `metadata-tikv/Cargo.toml` gains a `wyrd-testkit` test dep (patch.diff:199-203) where the brief said "alias only" — the alias is moot under Option B; no `metadata-tikv/src` or `traits` hunk exists anywhere in the diff. |
| C2 Reproduction (red pre-fix) | PASS | The reproduced defect is the unsound `concurrency.rs:3-4` "no await inside commit" rationale; the seed reproduces the gap deterministically and non-tautologically — the await shape overlaps two commit critical sections and loses an update (`max_in_flight == 2`, `version == 1` after two commits, patch.diff:150-165) while the negative-control synchronous shape provably cannot (patch.diff:174-189) — a real madsim scheduling fact with discriminating power, not the v6 `x && !x` shape. Caveat: I could not execute it here (no target); the C4-verify gate record is the run witness. |
| C3 Change | PASS | Diff touches exactly the briefed blast radius: the DST seed, the `testkit` fault seam (patch.diff:715-825), pure `xtask` dispatch/oracles + runners (patch.diff:1021-1352), the ≥3-replica compose (patch.diff:942-1012), tier scenarios, `metadata-tikv/Cargo.toml` (test-dep only) + `Cargo.lock`. Confirmed by inspection of the full 1484-line diff: **no** `crates/traits` hunk, **no** `crates/metadata-tikv/src` hunk — the two byte-for-byte invariants hold. |
| C4 Verification (red→green) | PASS | Gating gate green per record (`check-gates.json`: "xtask ci: all checks passed"; C4-verify "red without the fix, green with it"). Caveats, not blockers: (i) for a brand-new test file the stash-red is file-absence — under Option B the meaningful at-Check flips are the pure-oracle negation reds (patch.diff:842-935, 1447-1483) and the seed's in-test negative control, which is where I graded; (ii) the seed is `#![cfg(madsim)]` (patch.diff:63) so plain `cargo test` compiles it to nothing — see T3. Not presenting the unreachable target as a verification FAIL. |
| C5 Causal adequacy | NEEDS-HUMAN | Two decisions owed (the first pre-declared at Plan, brief.md:233-249). **(1) Accept the Option-B posture:** at-Check binding correctness evidence is now pure arithmetic + a self-authored *model* of the commit shape (patch.diff:68-106 — honest as a coverage artifact, but it drives no production code), and "no `madsim-tikv-client` exists" (patch.diff:43-45) is Do's confirm-at-build claim I could not independently verify (network blocked); the human confirms the registry check and accepts that swap-correctness proof lives wholly in the off-Check Tier job. **(2) Partition symmetry is contestable:** `SymmetricPartition::apply` drops INPUT+OUTPUT by `--dport` only (patch.diff:500-519) — traffic *to* the isolated store's port is cut, but connections the isolated node *initiates* outbound to peers/PD are not (no `--sport`/conntrack rule), so under `network_mode: host` tikv-1's self-originated Raft links may survive while the runner's port-probe oracle (patch.diff:490-498) still reports unreachable — arguably a partial-isolation cousin of the v6 no-op shape Invariant B forbids; the human decides whether the Tier job needs sport-side rules before the live leg counts as evidence. Symptom-guard smell-test: does **not** fire — env/feature gating of privileged tiers is by-design tier plumbing mirroring the existing chunkstore tier tests, not a capability probe papering over a load-time cause. |
| T1 Structure | PASS | Every artifact lands at the brief-named path: `crates/dst/tests/tikv_await_commit_interleaving.rs` (the exact path brief.md:195 names), pure logic in `xtask/src/metadata_faults.rs` with tests in `xtask/tests/metadata_faults_orchestration.rs` (mirroring `disk_faults`/`jepsen_dispatch`), seam in `crates/testkit/src/lib.rs`, tier scenarios in `crates/metadata-tikv/tests/tier{1,2}_*`, compose under `deploy/` outside the workspace (ADR-0010 note, patch.diff:946-947). |
| T2 Shape | PASS | Mirrors the standing idioms: `plan()/opted_in()/tool_available` runner shape (patch.diff:1038-1051), dispatch-as-enum with both routes representable like `jepsen_dispatch` (patch.diff:1277-1301), `#[ignore]` + env-gated + feature-cfg'd tier tests like the chunkstore siblings, `#[must_use]` pure fns, RAII heal guard. Minor: `pd_endpoints()` duplicated across the two tier tests (patch.diff:254-264, 584-594) — acceptable copy for independent test targets. |
| T3 Runtime | NEEDS-HUMAN | Decision owed: **confirm the seed actually executes in some at-Check gate.** It is `#![cfg(madsim)]` (patch.diff:63), so `cargo test --workspace` compiles it to nothing by design; only a `--cfg madsim` sweep (`cargo xtask dst`) runs it, and the recorded ci gate names fmt/clippy/build/test/deny/conformance — whether that includes the dst sweep is invisible from here (no target). If it does not, the flagship seed is committed-but-unexecuted at Check and its "green under every madsim seed" claim (patch.diff:54-59) is unexercised. Everything else is runtime-safe by inspection: tier tests double-gated (`#[ignore]` + endpoint check, patch.diff:269-279) and skip cleanly; new xtask subcommands defer unless `WYRD_TIER1/2=1`; heal runs on unwind via `Drop` (patch.diff:522-543; leaks only on SIGKILL — best-effort, acknowledged in-code). |
| T4 Contribution | NEEDS-HUMAN | The substantive prior-art question — the six rejected iterations — is settled by inspection: no `CommitMode` flag, no `admit = commit_point_ok` tautology, independent `read_after_commit`/`converged_once` signals (patch.diff:366-382), multi-key create/rename/delete leg (patch.diff:303-397), heal-every-port + readiness-wait (patch.diff:469-488, 527-541), assertions across (not before) the heal (patch.diff:363-380) — every named v1–v6 defect is demonstrably not re-instantiated. Decision owed: the **mechanical prior-art check by affected file path** (merged history + closed/rejected work — e.g. that no merged slice already carries a metadata tier harness or multi-replica compose) could not run in this sandbox; the human/driver confirms it ran clean. |
| T5 Judgment | PASS | The judgment calls are the brief's own, correctly executed and documented where Check reads them: Option-B fallback declared in-code with the honest narrower claim (patch.diff:41-51); the removed external route kept representable-but-hard-error exactly like `jepsen_dispatch` (patch.diff:1058-1067); static-endpoints reduced bar cited to #365 (patch.diff:963-964); the "minority partition can never flip exactly-one-winner red" lesson encoded as a unit test, not re-litigated (patch.diff:853-861). No un-briefed scope creep found. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decision owed: does the Option-B evidence bundle serve proposal 0015 item 6's purpose — at-Check only the pure oracles + a coverage seed; the actual "swap holds ADR-0015 on real TiKV" proof deferred to the privileged Tier job — and who runs/confirms that job (brief.md:256-257)? Bundled pre-declared human items: accept the static-endpoints reduced bar until #365 (brief.md:266-268); the metadata-nemesis ADR-refinement question is the board's (brief.md:258-265 — this patch correctly mints no ADR); #258 ordering (this slice lands in the earlier wave, brief.md:159-171). Runnable steps for the deferred leg: on a privileged Docker host run `WYRD_TIER1=1 cargo xtask metadata-tier1` (stands up `deploy/tikv-multi-replica`, exports `WYRD_TIKV_PD_ENDPOINTS=127.0.0.1:2379`, partitions port 20161, runs the `#[ignore]`d scenario with `--features tikv`); expect the three independent signals green, and a failure if the partition no-ops. |

## Notes for §6 (lifted from the NEEDS-HUMAN rows)

- [ ] **C5(1):** Confirm `madsim-tikv-client` truly has no usable release tracking `tikv-client 0.4` (Do's claim, patch.diff:43-45; reviewer's registry check was network-blocked) and accept the Option-B posture: swap-correctness proof now lives entirely off-Check in the privileged Tier-1 legs.
- [ ] **C5(2):** Rule on partition symmetry — `--dport`-only INPUT/OUTPUT drops (patch.diff:500-519) do not cut the isolated node's *self-initiated* outbound peer links under host networking; decide whether Invariant B requires sport-side/conntrack rules before the live Tier-1 leg counts as evidence.
- [ ] **T3:** Confirm the at-Check gate actually executes the `cfg(madsim)` seed (does the gate path run `cargo xtask dst`?); if not, wire it in — otherwise the seed is committed-but-unexecuted.
- [ ] **T4:** Confirm the file-path prior-art check ran clean (merged + closed work) for `crates/dst/`, `crates/testkit/`, `xtask/`, `deploy/tikv-multi-replica/`, `crates/metadata-tikv/tests/`.
- [ ] **V:** Accept fitness-to-purpose of the Option-B bundle; name who confirms the privileged Tier-1/Tier-2 job; accept the #365 static-endpoints reduced bar; route the metadata-nemesis ADR question to the architecture board.

### Advisory — adversary

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

### Advisory — codex

- NEEDS-HUMAN — `crates/metadata-tikv/tests/tier1_metadata_consistency.rs:290`: the claimed bidirectional partition only drops packets whose destination port is the isolated store port (`--dport 20161`) on both INPUT and OUTPUT. That makes host probes to `127.0.0.1:20161` fail, but it does not block traffic initiated by the isolated TiKV node to peers on their destination ports (for example 20160/20162), so the fault-effect oracle can pass without a symmetric node isolation.
- NEEDS-HUMAN — `crates/metadata-tikv/tests/tier1_metadata_consistency.rs:334`: `iptables` failures are ignored, and the RAII heal path also ignores delete failures. In the privileged leg, a missing permission/module or a failed cleanup can either turn the scenario into a false no-op/failure with little diagnostic signal or leak host firewall state after the test.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Two decisions owed (the first pre-declared at Plan, brief.md:233-249). **(1) Accept the Option-B posture:** at-Check binding correctness evidence is now pure arithmetic + a self-authored *model* of the commit shape (patch.diff:68-106 — honest as a coverage artifact, but it drives no production code), and "no `madsim-tikv-client` exists" (patch.diff:43-45) is Do's confirm-at-build claim I could not independently verify (network blocked); the human confirms the registry check and accepts that swap-correctness proof lives wholly in the off-Check Tier job. **(2) Partition symmetry is contestable:** `SymmetricPartition::apply` drops INPUT+OUTPUT by `--dport` only (patch.diff:500-519) — traffic *to* the isolated store's port is cut, but connections the isolated node *initiates* outbound to peers/PD are not (no `--sport`/conntrack rule), so under `network_mode: host` tikv-1's self-originated Raft links may survive while the runner's port-probe oracle (patch.diff:490-498) still reports unreachable — arguably a partial-isolation cousin of the v6 no-op shape Invariant B forbids; the human decides whether the Tier job needs sport-side rules before the live leg counts as evidence. Symptom-guard smell-test: does **not** fire — env/feature gating of privileged tiers is by-design tier plumbing mirroring the existing chunkstore tier tests, not a capability probe papering over a load-time cause.
- [ ] T3 Runtime — Decision owed: **confirm the seed actually executes in some at-Check gate.** It is `#![cfg(madsim)]` (patch.diff:63), so `cargo test --workspace` compiles it to nothing by design; only a `--cfg madsim` sweep (`cargo xtask dst`) runs it, and the recorded ci gate names fmt/clippy/build/test/deny/conformance — whether that includes the dst sweep is invisible from here (no target). If it does not, the flagship seed is committed-but-unexecuted at Check and its "green under every madsim seed" claim (patch.diff:54-59) is unexercised. Everything else is runtime-safe by inspection: tier tests double-gated (`#[ignore]` + endpoint check, patch.diff:269-279) and skip cleanly; new xtask subcommands defer unless `WYRD_TIER1/2=1`; heal runs on unwind via `Drop` (patch.diff:522-543; leaks only on SIGKILL — best-effort, acknowledged in-code).
- [ ] T4 Contribution — The substantive prior-art question — the six rejected iterations — is settled by inspection: no `CommitMode` flag, no `admit = commit_point_ok` tautology, independent `read_after_commit`/`converged_once` signals (patch.diff:366-382), multi-key create/rename/delete leg (patch.diff:303-397), heal-every-port + readiness-wait (patch.diff:469-488, 527-541), assertions across (not before) the heal (patch.diff:363-380) — every named v1–v6 defect is demonstrably not re-instantiated. Decision owed: the **mechanical prior-art check by affected file path** (merged history + closed/rejected work — e.g. that no merged slice already carries a metadata tier harness or multi-replica compose) could not run in this sandbox; the human/driver confirms it ran clean.
- [ ] Validation — fitness-to-purpose — Decision owed: does the Option-B evidence bundle serve proposal 0015 item 6's purpose — at-Check only the pure oracles + a coverage seed; the actual "swap holds ADR-0015 on real TiKV" proof deferred to the privileged Tier job — and who runs/confirms that job (brief.md:256-257)? Bundled pre-declared human items: accept the static-endpoints reduced bar until #365 (brief.md:266-268); the metadata-nemesis ADR-refinement question is the board's (brief.md:258-265 — this patch correctly mints no ADR); #258 ordering (this slice lands in the earlier wave, brief.md:159-171). Runnable steps for the deferred leg: on a privileged Docker host run `WYRD_TIER1=1 cargo xtask metadata-tier1` (stands up `deploy/tikv-multi-replica`, exports `WYRD_TIKV_PD_ENDPOINTS=127.0.0.1:2379`, partitions port 20161, runs the `#[ignore]`d scenario with `--features tikv`); expect the three independent signals green, and a failure if the partition no-ops.
- [ ] **The C4-verify "red without the fix" is a compile flip, the exact shape the
- [ ] **The "symmetric" partition is one-way, the mirror image of the v6 defect
- [ ] **The Option-B seed asserts a self-authored toy, and nothing can flip it.**
- [ ] `crates/metadata-tikv/tests/tier1_metadata_consistency.rs:290`: the claimed bidirectional partition only drops packets whose destination port is the isolated store port (`--dport 20161`) on both INPUT and OUTPUT. That makes host probes to `127.0.0.1:20161` fail, but it does not block traffic initiated by the isolated TiKV node to peers on their destination ports (for example 20160/20162), so the fault-effect oracle can pass without a symmetric node isolation.
- [ ] `crates/metadata-tikv/tests/tier1_metadata_consistency.rs:334`: `iptables` failures are ignored, and the RAII heal path also ignores delete failures. In the privileged leg, a missing permission/module or a failed cleanup can either turn the scenario into a false no-op/failure with little diagnostic signal or leak host firewall state after the test.

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): issue_257 (iteration 7) — the Option-B POSTURE is accepted (see ratified), but the at-Check + Tier-job evidence that remains is not sound: the fault the deferred proof rides on is a no-op behind a blind oracle, and two other legs re-instantiate previously-rejected shapes. Fix these before re-submitting; do NOT re-open Option-B. MUST FIX — the partition leg (load-bearing: the whole off-Check correctness proof depends on it): 1. Partition is one-way, not symmetric. `SymmetricPartition::apply` (crates/metadata-tikv/tests/tier1_metadata_consistency.rs:288-307) drops only `--dport 20161` on INPUT+OUTPUT; under `network_mode: host` both rules match the same direction (packets TO tikv-1), so tikv-1's self-initiated outbound Raft/PD links (dport 2379/20160/20162) and its established-connection replies survive — a receive-only blackout, not isolation (the v6 no-op-fault shape Invariant B forbids). Make it truly bidirectional: add `--sport 20161` / conntrack rules, or isolate the node's netns, so the node can neither send nor receive. 2. The fault-effect oracle is blind to (1). `fault_materialized` / the inline `before && during` (tier1_metadata_consistency.rs:114-127,124) TCP-probe the very dport the DROP guarantees to fail, so it verifies the rule blocks THE PROBE, not that the node is isolated — a one-way or probe-only cut passes the Invariant-B gate that must be RED for a no-op. Make the oracle observe real isolation (e.g. from the peer/PD side that tikv-1's raft/heartbeats stop), and WIRE the already-tested pure oracles `partition_took_effect` / `heal_is_complete` (xtask/src/metadata_faults.rs:109,119) into the scenario — they are currently dead code (consumed by nothing but their own tests), so regressing them flips nothing at Check (v1 "computed, never applied", applied to oracles). 3. Heal is silent-lossy (codex). The RAII heal (tier1_metadata_consistency.rs:310-336) `let _ =`s the `iptables -D` result and records no dropped/healed sets and never probes reachable-after-heal — a failed heal leaks host firewall state, the exact thing `heal_is_complete` was minted to catch. Surface iptables failures and verify the heal. MUST FIX — make the at-Check evidence behavioural, not tautological: 4. The flagship seed is an unflippable self-authored toy. `crates/dst/tests/tikv_await_commit_interleaving.rs` imports only std+madsim, never touches `TikvMetadataStore::commit` (crates/metadata-tikv/src/lib.rs:540-600); both halves are true BY CONSTRUCTION (Mutex semantics / a 1ms sleep), so no madsim seed and no production regression can flip it — if the real commit dropped its await window the seed stays green while its premise rots (the v6 "prove a patch-authored broken branch is broken" shape). Either bind the seed to the real commit path so a production regression can turn it red, or drop the "green under every madsim seed proves the swap" claim and label it honestly as pure coverage with no correctness weight. 5. C4-verify red is a compile-flip, not behavioural. run-verify's red is an unresolved-import compile error from deleting xtask/src/metadata_faults.rs while keeping its test; in the green phase the cfg(madsim) seed + #[ignore]d tiers run zero tests — precisely the v3 rejection ("the binding on-Check flip is behavioural, not a compile flip", brief.md:292). The at-Check flip must be behavioural. RATIFIED this iteration — do NOT re-litigate: - Option-B posture (axis 1): `madsim-tikv-client` genuinely does not exist (cargo search confirms no release tracking tikv-client 0.4), so the real ADR-0015-on-TiKV proof legitimately lives off-Check in the privileged Tier-1/Tier-2 job; the reduced at-Check bar (pure oracles + coverage seed) is accepted AS A POSTURE. The reason it still iterates is that the deferred-to Tier leg is itself broken (must-fix 1-3), not that Option-B is wrong. - T3: the seed IS executed at Check via `cargo xtask ci` -> `run_dst` under `--cfg madsim` (xtask/src/main.rs:825,836-857) — it is compiled and run, so it is NOT committed-but-unexecuted. (It is still unflippable — see must-fix 4.) - Invariants hold: no `crates/metadata-tikv/src` and no `crates/traits` edit (only a dev-dependency added to metadata-tikv/Cargo.toml). - #258 ordering (this slice in the earlier wave) and the metadata-nemesis ADR question routed to the architecture board (patch correctly mints no ADR) stand as pre-declared; the #365 static-endpoints reduced bar is accepted. Overarching note for the builder: this is iteration 7, and must-fix 1/4/5 each re-enter a shape the brief already rejected (v6 no-op-fault, v6 broken-toy, v3 compile-flip). The bar is BEHAVIOURAL evidence — a real bidirectional partition observed by an oracle that isn't probe-only, and a seed/flip a genuine production regression can turn red.
- By / date: Eduard Ralph / 2026-07-04

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
