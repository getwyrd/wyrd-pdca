# Result — issue 365 / coordination-etcd-l5-backend

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: 
- Success criterion: a **shared `Coordination` conformance / contract suite** — lifted
- Repo + branch target: getwyrd/wyrd @ `feat/m4-production-metadata-backend`
- Scope (one logical fix) / out of scope: 

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix (net-new crate + shared suite; a real, exercisable second
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass —                no pre-patch state to isolate a RED against; C4-ci gates the whole tree (#88).
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

# Check review — issue 365 / coordination-etcd-l5-backend (iteration 3)

**Task under review:** Build the ADR-0006 *second* implementation of the L5 `Coordination`
trait — a networked `coordination-etcd` crate (leased discovery, single-leader election with
rising fencing tokens, fenced mutually-exclusive locks, revisioned config) selectable by
`server` composition with **no caller edits**, plus **one shared contract suite** that both
`coordination-mem` and `coordination-etcd` pass. Iterations 1 & 2 were rejected because the
etcd store was compiled only under an OFF-by-default feature (never exercised by any gate) and
carried split-brain/orphan-lease/unconditional-unlock defects.

> Grounding note: `$PDCA_TARGET` was not readable in this sandbox, so per protocol every
> citation is grounded on `patch.diff` alone (I did not search other checkouts). I could not
> independently re-run the build/gates here (no target worktree / toolchain); rows that turn on
> a gate result rest on the recorded `check-gates.json` (C4-ci = pass) plus artifact-level
> re-derivation. This is a target-state caveat, not a patch defect — I do **not** raise it as a
> C4 FAIL.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Brief is a pointer to 0015's named deployment prerequisite with concrete, binding success criteria (second impl + shared suite green on both backends + composition-only selection). Scope boundaries (half-2 process roles owned by #364/#366; trait byte-for-byte unchanged) are explicit. `brief.md:40-58`. |
| C2 Reproduction (red pre-fix) | PASS | Net-new crate: no pre-existing bug to reproduce. The flippable RED is codified as `#[should_panic]` violating-stub tests proving every shared clause is load-bearing, incl. the previously-vacuous config-monotonicity clause (`config_catches_a_frozen_revision_after_read_back_passes`, expected "strictly raises the revision") and renew/revoke (`patch.diff:1360-1590`). C4-verify row correctly records "no pre-patch state to isolate a RED against." |
| C3 Change | PASS | Diff matches spec exactly: new `coordination-etcd` + shared `coordination-conformance` crates, `server` `etcd` feature + `CoordinationBackend` selection, `xtask etcd-conformance` job, `deploy/etcd-single-node` compose, madsim `dst` proof. No scope creep. `Cargo.toml:872-906`, `patch.diff:1595-2438`. |
| C4 Verification (red→green) | PASS | Central iter-2 gap closed: store is now compiled AND exercised in `ci` — `crates/dst/tests/coordination.rs` (`#![cfg(madsim)]`) drives the real store over the madsim etcd simulator (`dst` is in `ci`), asserting single-leader/handoff/fencing/expiry/no-orphan deterministically (`patch.diff:2637-2912`; wired at `crates/dst/Cargo.toml:2617-2628`). Recorded `check-gates.json` C4-ci = pass. Caveat (not a defect): real-etcd GREEN is off-`ci` (`xtask etcd-conformance`, needs docker+protoc) and was not run in this env — see Validation. |
| C5 Causal adequacy | NEEDS-HUMAN | Root cause (trait pinned by one process-local impl) is addressed at the cause, not guarded: holds own an etcd lease kept alive by a background task that revokes-on-drop, `unlock` revokes only OUR lease (conditional-by-construction), cancelled campaign drops its guard → revoke (`store.rs` `patch.diff:1977-2273`). Symptom-guard smell-test does NOT fire (the `#[cfg(feature="etcd")]`/`cfg(madsim)` gates are conditional compilation, not runtime capability probes). **Decision owed:** two prior iterations shipped split-brain here and a production L5 backend that lapses a leader is catastrophic — a human must confirm the revoke-on-drop + keep-alive design leaves no residual dual-leadership window, given the deterministic proof is on the *simulator* and real-etcd confirmation is off-`ci`. |
| T1 Structure | PASS | Crates placed correctly; ADR-0016 dependency discipline held — `coordination-conformance` depends only on `wyrd-traits` (`patch.diff:926-933`), etcd client is optional/`etcd`-feature-only; pure helpers (`fencing`/`hold`/`keyspace`) carry no etcd dep and are unit-tested on every build. |
| T2 Shape | PASS | Mirrors the established `metadata-tikv`/`metadata-conformance` shape; trait surface untouched (no `crates/traits` hunk in the diff); `CoordinationBackend` selection mirrors `MetadataBackend` (`cli.rs` `patch.diff:2953-2996`). One `run_all` runner keeps both backends on the identical clause set. |
| T3 Runtime | PASS | Keep-alive lifecycle (`spawn_keepalive`, `KeepAlive::Drop` vs `stop_without_revoke`) and cross-instance mutual-exclusion/handoff are exercised deterministically by the madsim `dst` tier (`patch.diff:2009-2042`, `2743-2912`). Verdict rests on that tier, which I could not re-run in-sandbox. |
| T4 Contribution | PASS | Coherent and complete against scope: shared suite lifted (mem's `trait_contract` now drives `run_all`, mem-specific rising-lease/config-zero retained — `patch.diff:2555-2593`), cross-instance clauses, Tier-2 automation, throwaway compose. Miswired-Tier-2 false-green from iter-1 is now a hard panic (`patch.diff:2431-2438`). |
| T5 Judgment | NEEDS-HUMAN | The patch *chose* the DST-fidelity story (madsim-etcd-client simulator over a contract harness) and introduces a new networked dependency. **Decision owed:** confirm the madsim-etcd-client DST approach is the accepted #264/#258-mirror answer, and run the ADR-0003 three-test audit + `deny.toml` allowlist + TLS/auth posture review for `etcd-client 0.14` before it can enter the shipped graph (it is OFF-by-default today, so `ci` is unaffected). `Cargo.toml:896-906`, brief `:178-182`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | **Decision owed:** success criterion (b) requires the shared suite GREEN against **real etcd**; that run is off-`ci` (`xtask etcd-conformance`, needs docker + system protoc) and was not executed in this environment — a human must run it once to earn the real-etcd GREEN (deterministic madsim proof stands in `ci`, but the brief pins real-etcd). Also owed: the sequencing-governance call (explicit M4 slice vs preceding coordination milestone, board-visible), brief `:175-177`. Concrete steps: from the target worktree run `cargo xtask etcd-conformance` (brings up `deploy/etcd-single-node`, sets `WYRD_ETCD_ENDPOINTS`, builds `--features etcd`, drives `crates/coordination-etcd/tests/conformance.rs`); expect the shared suite + cross-instance mutual-exclusion/discovery to pass. |

### Advisory — adversary

# Adversarial review — issue 365 / coordination-etcd-l5-backend (iteration 3)

Skeptic's pass. Grounded on `$PDCA_TARGET` = `/home/eddie/wyrd/wyrd.pdca-wt-l0`
(patch applied to the worktree). I did **not** re-run the full madsim `dst` tier
(cost); every finding below is grounded in source, and several are test-adequacy
defects that hold regardless of whether that tier is green. Advisory only — I gate
nothing.

## What the builder genuinely closed (attempted to refute, could not)
- The prior "store.rs is never compiled by any gate" objection is materially
  addressed: `crates/coordination-etcd/src/lib.rs:36` compiles `store` under
  `cfg(madsim)`, and `crates/dst/Cargo.toml` + `crates/dst/tests/coordination.rs`
  drive it under `--cfg madsim` inside `run_dst()`, which `run_ci()` calls
  (`xtask/src/main.rs:973`). So store.rs *is* compiled and exercised in C4-ci — via
  the simulator (see finding 2).
- `renew`/`revoke` now have contract coverage (`coordination-conformance/src/lib.rs:91`),
  the config-monotonicity clause is present and shown non-vacuous past the read-back
  (`demonstrated_red.rs:349`), the rising-lease-id claim was deliberately relaxed to
  `assert_ne!` (`lib.rs:77`), and the protoc/real-etcd build is gated out of ci.
  These carry-forward items are real fixes; I could not refute them.

## Refutations the human must weigh

- **NEEDS-HUMAN — The single-leader test does not assert single leadership; a
  split-brain `elect_leader` would pass it.** `crates/dst/tests/coordination.rs:107-143`
  (`only_one_of_two_instances_leads_then_hands_off`) has exactly one assertion:
  `lead_b.token > lead_a.token`. Two *sequential* campaigns satisfy that even if the
  backend granted A and B leadership **concurrently**. The test never asserts B's
  campaign was pending while A led (no check that `b_task` was unresolved before
  `drop(a)`; its result is consumed after the drop regardless of when it resolved), so
  under a split-brain implementation B resolves early, the later `b_task.await` returns
  the already-computed value, and the test still passes. The crate's headline property —
  a *single* custodian leader through `elect_leader` (the M3.3/#141 path) — is therefore
  verified by **no** gated test. Only the lock analogue asserts mutual exclusion
  (`:148-173`, via `is_none()`), and that is a different code path (txn value-compare,
  not `campaign`). The invariant "single leader across processes" rests on an assertion
  that cannot fail on split-brain.

- **NEEDS-HUMAN — No gate exercises real etcd; success-criterion (b) rests entirely on
  madsim-etcd-client fidelity.** `run_ci()` (`xtask/src/main.rs:947-976`) runs
  `cargo test --workspace` (etcd feature OFF → store.rs not compiled there, per
  `lib.rs:36`) and `run_dst()`; it does **not** call `run_etcd-conformance` (that job is
  docker+protoc-gated and excluded from ci by design, `xtask/src/main.rs:69-104`,
  `:264-279`). So the only in-ci compile+run of the store is `--cfg madsim` against
  `madsim-etcd-client 0.6.0+0.14.0` (Cargo.lock:1566) — a *re-implementation* of etcd,
  not etcd. The brief's criterion (b) ("shared suite green on both backends, real etcd
  via a Tier-2 compose target", brief.md:50) is **not** demonstrated by any gate; it is
  earnable only by a manual `cargo xtask etcd-conformance`. Any reviewer claim that (b)
  is satisfied by the green gate is unwarranted — it proves the simulator's etcd model
  agrees, which is precisely the DST-fidelity question the brief itself flags open
  (brief.md:178). Campaign-blocking, lease-expiry, and same-value-proclaim-bumps-revision
  are all load-bearing and unverified against real etcd.

- **NEEDS-HUMAN — `config_revision` has materially different semantics on the two
  backends, and the shared suite cannot catch it.** mem returns a config-only counter
  (`+1` per `set_config`, `coordination-mem/src/lib.rs:218,228`); etcd returns the
  **global cluster mvcc revision** (`coordination-etcd/src/store.rs:379-393`), which is
  bumped by *every* write — locks, registrations, elections, other namespaces. The
  shared clause `contract_config_is_revisioned` asserts only the relative `r1 > r0`
  (`coordination-conformance/src/lib.rs:193,206`), so it can never see the divergence.
  Concrete production consequence: the trait shapes `config_revision` as a pollable
  watch signal ("a watcher re-reads when it advances", store.rs:380-384); on etcd a
  config watcher wakes and re-reads on every unrelated coordination write, not only on
  config changes. Two backends satisfy the same trait method with different meaning — a
  human should confirm this is acceptable.

- **Re-election treats every proclaim error as lease-expiry, self-inflicting a stall on
  a transient blip.** `crates/coordination-etcd/src/store.rs:264-270`: when re-proclaim
  on a cached `LeaderKey` returns `Err`, the code assumes the lease expired, does
  `stop_without_revoke()` on the still-live keep-alive, and falls through to a fresh
  campaign. On a *transient* proclaim RPC error (lease still valid) this aborts renewal
  of a live lease without revoking it, so the old candidate key (lower create-revision)
  lingers up to `HOLD_TTL_SECS=6` (store.rs:57) with no renewal — and the fresh campaign
  then blocks behind this instance's **own** orphaned candidacy for up to 6s. A
  self-inflicted leadership stall + lease leak on any transient network error during
  re-election. Untested: the only re-proclaim path in the suite runs against the
  never-erroring simulator.

- **Cross-instance clauses have no demonstrated-red (non-vacuity) proof.**
  `crates/coordination-conformance/tests/demonstrated_red.rs` proves only the
  **single-instance** shared clauses catch a violating stub (`:167,212,261,307,351,396`).
  The two-instance properties that actually justify the crate — single leader, cross-
  instance mutual exclusion, cross-process discovery (`dst/tests/coordination.rs:107,148,178`)
  — have **no** red counterpart proving a broken/split-brain store would fail them. Their
  non-vacuity therefore rests on unverified simulator fidelity, compounding the two
  NEEDS-HUMAN items above (and directly enabling the finding-1 false green).

## Scope note
All findings are on files this diff adds/edits. I did not file pre-existing debt. The
trait surface (`crates/traits/src/lib.rs`) is untouched by the patch (verified: not in
the diff), so the invariant "trait byte-for-byte unchanged" holds; the `server`
composition edit (`crates/server/src/cli.rs`) is correctly `#[cfg(feature = "etcd")]`-gated.

### Advisory — codex

- NEEDS-HUMAN — [crates/coordination-etcd/tests/conformance.rs:72](/home/eddie/wyrd/wyrd.pdca-wt-l0/crates/coordination-etcd/tests/conformance.rs:72) says the real-etcd run covers cross-instance properties, but the block only asserts lock exclusion/re-fencing and discovery ([crates/coordination-etcd/tests/conformance.rs:83](/home/eddie/wyrd/wyrd.pdca-wt-l0/crates/coordination-etcd/tests/conformance.rs:83), [crates/coordination-etcd/tests/conformance.rs:98](/home/eddie/wyrd/wyrd.pdca-wt-l0/crates/coordination-etcd/tests/conformance.rs:98)); single-leader election is only proven in the madsim test, so a real-etcd regression in campaign behavior would not fail `xtask etcd-conformance`.
- NEEDS-HUMAN — [xtask/src/main.rs:286](/home/eddie/wyrd/wyrd.pdca-wt-l0/xtask/src/main.rs:286) and [xtask/src/main.rs:301](/home/eddie/wyrd/wyrd.pdca-wt-l0/xtask/src/main.rs:301) make `cargo xtask etcd-conformance` return `Ok(())` locally when Docker or `protoc` is missing, so the named real-etcd proof can exit successfully without compiling/running the etcd backend outside CI; decide whether this local false-green posture is acceptable for the sign-off evidence.
- NEEDS-HUMAN — [Cargo.toml:101](/home/eddie/wyrd/wyrd.pdca-wt-l0/Cargo.toml:101) introduces the production `etcd-client = "0.14"` dependency while the same stanza records unresolved ADR-0003 / `deny.toml` / TLS-auth review ([Cargo.toml:97](/home/eddie/wyrd/wyrd.pdca-wt-l0/Cargo.toml:97)); this dependency/posture approval remains a human sign-off item.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Root cause (trait pinned by one process-local impl) is addressed at the cause, not guarded: holds own an etcd lease kept alive by a background task that revokes-on-drop, `unlock` revokes only OUR lease (conditional-by-construction), cancelled campaign drops its guard → revoke (`store.rs` `patch.diff:1977-2273`). Symptom-guard smell-test does NOT fire (the `#[cfg(feature="etcd")]`/`cfg(madsim)` gates are conditional compilation, not runtime capability probes). **Decision owed:** two prior iterations shipped split-brain here and a production L5 backend that lapses a leader is catastrophic — a human must confirm the revoke-on-drop + keep-alive design leaves no residual dual-leadership window, given the deterministic proof is on the *simulator* and real-etcd confirmation is off-`ci`.
- [ ] T5 Judgment — The patch *chose* the DST-fidelity story (madsim-etcd-client simulator over a contract harness) and introduces a new networked dependency. **Decision owed:** confirm the madsim-etcd-client DST approach is the accepted #264/#258-mirror answer, and run the ADR-0003 three-test audit + `deny.toml` allowlist + TLS/auth posture review for `etcd-client 0.14` before it can enter the shipped graph (it is OFF-by-default today, so `ci` is unaffected). `Cargo.toml:896-906`, brief `:178-182`.
- [ ] Validation — fitness-to-purpose — **Decision owed:** success criterion (b) requires the shared suite GREEN against **real etcd**; that run is off-`ci` (`xtask etcd-conformance`, needs docker + system protoc) and was not executed in this environment — a human must run it once to earn the real-etcd GREEN (deterministic madsim proof stands in `ci`, but the brief pins real-etcd). Also owed: the sequencing-governance call (explicit M4 slice vs preceding coordination milestone, board-visible), brief `:175-177`. Concrete steps: from the target worktree run `cargo xtask etcd-conformance` (brings up `deploy/etcd-single-node`, sets `WYRD_ETCD_ENDPOINTS`, builds `--features etcd`, drives `crates/coordination-etcd/tests/conformance.rs`); expect the shared suite + cross-instance mutual-exclusion/discovery to pass.
- [ ] [crates/coordination-etcd/tests/conformance.rs:72](/home/eddie/wyrd/wyrd.pdca-wt-l0/crates/coordination-etcd/tests/conformance.rs:72) says the real-etcd run covers cross-instance properties, but the block only asserts lock exclusion/re-fencing and discovery ([crates/coordination-etcd/tests/conformance.rs:83](/home/eddie/wyrd/wyrd.pdca-wt-l0/crates/coordination-etcd/tests/conformance.rs:83), [crates/coordination-etcd/tests/conformance.rs:98](/home/eddie/wyrd/wyrd.pdca-wt-l0/crates/coordination-etcd/tests/conformance.rs:98)); single-leader election is only proven in the madsim test, so a real-etcd regression in campaign behavior would not fail `xtask etcd-conformance`.
- [ ] [xtask/src/main.rs:286](/home/eddie/wyrd/wyrd.pdca-wt-l0/xtask/src/main.rs:286) and [xtask/src/main.rs:301](/home/eddie/wyrd/wyrd.pdca-wt-l0/xtask/src/main.rs:301) make `cargo xtask etcd-conformance` return `Ok(())` locally when Docker or `protoc` is missing, so the named real-etcd proof can exit successfully without compiling/running the etcd backend outside CI; decide whether this local false-green posture is acceptable for the sign-off evidence.
- [ ] [Cargo.toml:101](/home/eddie/wyrd/wyrd.pdca-wt-l0/Cargo.toml:101) introduces the production `etcd-client = "0.14"` dependency while the same stanza records unresolved ADR-0003 / `deny.toml` / TLS-auth review ([Cargo.toml:97](/home/eddie/wyrd/wyrd.pdca-wt-l0/Cargo.toml:97)); this dependency/posture approval remains a human sign-off item.

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
- Iteration delta (if iterating): Rejected despite green gates: the crate's headline safety property is caught by no gated test, and criterion (b) (real-etcd green) is unearned. These are test-adequacy defects that need a rebuild, not a one-time confirmation. Direction for the rebuild: - Single-leader test can't catch split-brain (the exact class that got iterations 1 & 2 rejected). only_one_of_two_instances_leads_then_hands_off (dst/tests/coordination.rs:107) asserts only `lead_b.token > lead_a.token`, which two SEQUENTIAL campaigns satisfy even under a concurrent (split-brain) grant. Assert that B's campaign stays PENDING while A leads (b_task unresolved before drop(a)) so a store granting A and B concurrently fails. The single-custodian-leader property (elect_leader / campaign path) must be verified by a gated test. - Cross-instance clauses have no demonstrated-red: single leader, cross-instance mutual exclusion, cross-process discovery have no violating-stub counterpart proving a broken store fails them. Add demonstrated-red coverage so their non-vacuity doesn't rest solely on simulator fidelity. - Real etcd is exercised by no gate: criterion (b) is earnable only via manual `cargo xtask etcd-conformance` (docker+protoc, off-ci), and that job returns Ok(()) when docker/protoc are missing (local false-green, xtask/src/main.rs:286/301). Fix the false-green (missing tooling must not pass), and the real-etcd green must actually be produced before this backend enters the shipped graph. - config_revision semantics diverge across backends: mem returns a config-only counter; etcd returns the global cluster mvcc revision (bumped by every write). Shared clause asserts only r1>r0 so it can't catch it; on etcd a config watcher wakes on every unrelated coordination write. Either normalize the semantics or tighten the contract clause to pin config-only advancement. - Re-election treats every proclaim Err as lease-expiry (store.rs:264-270): a transient blip does stop_without_revoke + fresh campaign, self-inflicting a <=6s stall and a lease leak behind the instance's own orphaned candidacy. Distinguish transient RPC error from actual lease loss; add a test that errors the proclaim path (the simulator never errors today). Open sign-off items for the next Check (not the do itself): ADR-0003 three-test audit + deny.toml allowlist + TLS/auth review for etcd-client 0.14 before it enters the shipped graph; the sequencing-governance call. Credit retained: the iteration-2 "store never compiled by any gate" blocker is genuinely closed (store.rs compiled+driven under cfg(madsim) in the dst tier). §6 items: none ticked — driving the reject on the reported issues.
- By / date: Eduard Ralph / 2026-07-04

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
