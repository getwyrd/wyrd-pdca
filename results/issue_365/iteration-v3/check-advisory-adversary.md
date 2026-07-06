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
