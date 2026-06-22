# Build notes — issue 146 / m3.8-dst-campaign

Target: `getwyrd/wyrd @ main`, built in `$PDCA_WORKTREE`
(`/home/eddie/wyrd/wyrd.pdca-wt`). Planning artifact: proposal 0005
(`docs/design/proposals/accepted/0005-milestone-3-custodians.md`), §"DST and tests
(the heart of M3)" `0005:369-411`, graduation criteria `0005:500-502`, slice 8
`0005:541-545`, crate touch-points `0005:434-438`. ADR-0009 (DST is the correctness
authority; every discovery becomes a permanent seeded regression).

## What this slice is

The **consolidated verification campaign** that is M3's graduation gate — not a
behavioural fix. Net-new test infrastructure across three crates, per the proposal's
`0005:434-438` touch-points:

1. **`crates/testkit/src/lib.rs`** — the storage-fault seam the campaign drives: a
   `StorageFault { Lost, BitRot }` model + `SeededStorageFaults` (the bit-rot /
   fragment-loss seam **and** the D-server-kill seam, `0005:434-435`), the storage-plane
   sibling of the existing `NetFault` / `SeededNetFaults`. Import-light (only `rand` /
   `rand_chacha`, as the existing net seam is), with unit tests mirroring the net-seam
   tests. New code added after `SeededNetFaults` (≈ testkit `lib.rs:230-330` on the
   patched tree) + four unit tests.
2. **`crates/dst/tests/custodian.rs`** (new, the BINDING deliverable) — the Tier-0
   custodian property campaign: all six §13/§10 properties, driven through the **real**
   `reconcile_step` fenced control point over in-memory `MetadataStore`/`ChunkStore`
   trait stores (Option A, `0005:524-527`), swept over seeds under `--cfg madsim`
   (`0005:436`), with a committed regression-seed set (ADR-0009).
3. **`crates/dst/Cargo.toml`** — dev-deps the suite needs: `wyrd-custodian`,
   `wyrd-coordination-mem`, `tracing`, `tracing-subscriber` (all already workspace deps;
   Cargo.lock gains only the dev-dependency edges).
4. **`xtask/src/faults.rs`** (new) + **`xtask/src/main.rs`** — the deferred (off-Check)
   Tier-1 / Tier-2 runners as `cargo xtask` subcommands (`disk-faults`, `jepsen`,
   `kill-reconstruct`; `0005:437-438`).

No production logic changed — the four custodian loops are pristine (verified:
`leadership.rs` / `gc.rs` / `scrub.rs` / `reconstruction.rs` byte-identical to
`origin/main` after the red-demonstration negations were reverted). This slice only
*verifies* #142/#143/#144/#145; brief Scope "out of scope: any new custodian behaviour".

## Why the Tier-0 suite is shaped this way

- **Runs in the simulator, driven by the seed.** The file is `#![cfg(madsim)]` and each
  property is a `#[madsim::test]`; `cargo xtask dst` sets `--cfg madsim` and
  `MADSIM_TEST_NUM=50`, so madsim reruns every property across 50 seeds (`xtask/src/main.rs:411-439`).
  Fault selection (which D server is killed/rotted) is drawn from
  `ChaCha8Rng::seed_from_u64(madsim::runtime::Handle::current().seed())` — the same
  `rand_seed()` idiom `tests/network.rs:516-519` uses — so the whole campaign is a pure
  function of the run seed (ADR-0009).
- **Through the real control point, over the seams.** Every property calls the production
  `wyrd_custodian::reconcile_step(...)` (the anti-#141 fenced entry, `reconciliation.rs:62`)
  over `MemMeta`/`MemDServer`/`Fleet` — the same in-memory stores the per-slice tests use
  (`crates/custodian/tests/{gc,scrub,reconstruction}.rs`). No test-only loop entry.
- **In-memory stores under madsim, not the network stack.** Tier-0 needs the simulator's
  deterministic scheduling + seed, not its network; the custodian loops are driven
  directly over the trait seams (Option A). Keeping the unit import-light (no gRPC/tonic
  at load) is also what the headless runner needs.
- **RS(2,1) on a 4-domain topology.** Smallest genuinely erasure-coded scheme that
  survives one loss (k=2, n=3 on servers 0,1,2 = domains A,B,C; server 3 = D is the spare
  a rebuild flips onto). A killed server's slot rebuilds onto D (its own domain leaves the
  healthy topology); a bit-rotted server rebuilds in place (its domain sorts before D in
  the util-0 selector, confirmed against `placement.rs:227-266` and the per-slice
  reconstruction test `crates/custodian/tests/reconstruction.rs:408-409`).

### Two modelling choices worth flagging for sign-off

- **Crash model (property 2).** A crash before the version-conditional commit is modelled
  by `CrashMeta`, a `MetadataStore` wrapper that, while armed, drops the **one** batch
  carrying a positive precondition (the repoint CAS, `reconstruction.rs:364-367`) without
  applying it. At the store boundary a crash-before-commit and a lost CAS are
  indistinguishable — both leave the inode at its prior value with the rebuilt fragment
  already on disk (repair writes fragments *before* the commit, `reconstruction.rs:325-349`).
  The test asserts the post-crash invariant the proposal names (`0005:385-389`): inode
  version unchanged (fully old, never hybrid), the stray fragment unreferenced
  (collectable garbage, not corruption), the read still correct; then a disarmed restart
  completes to full redundancy. This is a faithful Tier-0 model — the alternative
  (reorder put-after-commit to manufacture a torn chunk) is a structural production change
  this verification slice must not make.
- **Durability assertion mechanism (property 6).** A minimal `tracing_subscriber::Layer`
  (`MetricCapture`) records the numeric value each metric event emits, so the suite
  asserts the **exact emitted values** — under-replicated rises to 1 then returns to 0,
  queue-depth 1 then 0, a time-to-repair sample — across two passes. Chosen over reading
  the real `DurabilityTelemetry` Prometheus surface back because (a) a monotonic counter
  cannot show "returns to zero" (only a per-pass emitted value can), and (b) it pulls in
  no OpenTelemetry runtime, so it is fully deterministic under the simulator. The
  in-process assertion mechanism is explicitly ILLUSTRATIVE (`telemetry.rs:13-16`); the
  dual-export surface itself is BINDING and already proven under the per-slice tests
  (`crates/custodian/tests/skeleton.rs:119-154`).

## Red→green proof (verification posture (a))

The suite is net-new, so "red" is partly criterion-absence on a new file. Per the brief
I additionally captured a **demonstrated assertion-level red** for each of the six
properties by temporarily negating the load-bearing production guard, confirming each
assertion is load-bearing (not resting green on non-existence). All negations were
reverted; production files are byte-identical to `origin/main`.

| Property | Negation (production file) | Observed red |
|---|---|---|
| 1 — reconstruct to full redundancy | `reconstruction.rs` repoint `new_placement[index] = old` (don't move to rebuilt server) | `custodian.rs:536` "the killed server no longer holds a referenced fragment" |
| 2 — commit-point atomic under crash | `reconstruction.rs` drop `.require(...)` (non-CAS commit) | `custodian.rs:585` "no version-conditional commit landed" |
| 3 — scrub detects bit-rot (Q2) | `scrub.rs:79` `if false && !fragment_intact(...)` | `custodian.rs:670` "scrub detected the bit-flip ... and enqueued it" |
| 4a — GC never reclaims referenced (Q3) | `gc.rs:126` `if false && referenced.contains(...)` | `custodian.rs:831` "a fragment a committed chunk map references is NEVER reclaimed" (needed an adversarial stale orphan on the live fragment so the reference gate is the sole protector — mirrors `crates/custodian/tests/gc.rs:250-283`) |
| 4b — GC grace window (Q3) | `gc.rs:133` `if true \|\| now_millis >= ...` | `custodian.rs:849` "an orphan within its grace window is never reclaimed" |
| 5 — fenced stale leader | `leadership.rs:88` `if false` (authorize never fences) | `custodian.rs:892` "a deposed leader's reconciliation is rejected by its stale fencing token" |
| 6 — durability emission | `reconstruction.rs` `emit_under_replicated(0)` | `custodian.rs:959` "the under-replicated count rises to 1 after the injected loss" |

(Line numbers above are the assertion-message lines on the final formatted file; the
panic locations the red runs printed sit on the `assert*!` macro a few lines above each.)

Note on 4a: the first attempt (negating only the reference gate) stayed green, because
GC's reason logic only reclaims a fragment with a deadline (orphan/expired-lease) and the
live fragment had none. I strengthened property 4 to plant a stale, long-expired orphan
record on the referenced fragment so the **reference check is the only thing protecting
it** — the same adversarial setup the per-slice test uses. This is a real
correctness-of-test fix, not a workaround.

## Committed seeds

`REGRESSION_SEEDS` (`custodian.rs`) is a fixed set run by
`committed_regression_seeds_stay_green`, replaying all six properties per seed
independent of the madsim sweep (ADR-0009, `0005:374`). No bug-finding seeds exist yet
(net-new, no bug surfaced); the constant is the home a future discovery is appended to.
The 50-seed madsim sweep is the broad coverage.

## Deferred (off-Check) Tier-1 / Tier-2 runners

`disk-faults` (dm-flakey/dm-error, `dmsetup`), `jepsen` (`lein`), `kill-reconstruct`
(Tier-2, `docker`) are wired in `xtask/src/main.rs` and implemented in `faults.rs`. They
are **deferred by default** (`Plan::Deferred`) — without an explicit opt-in
(`WYRD_TIER1=1` / `WYRD_TIER2=1`) they print what they require and exit cleanly, so they
are INERT at Check and never run in `cargo xtask ci` (which stays unprivileged). The
gating decision (`plan`) is pure and unit-tested (5 tests, run inside `ci` via
`cargo test --workspace`); the privileged scenario body shells to an env-supplied harness
command and runs only in the dedicated off-Check job that supplies it. These are the
DEFERRED-posture deliverables (brief Verification posture (b)): their green is confirmed
off-Check by the host CI Tier-1/Tier-2 jobs and the maintainer at sign-off
(`0005:405-411`, `0005:544`) — a pre-declared sign-off item, not a surprise NEEDS-HUMAN.

I deliberately did **not** author the privileged real-I/O harness (device-mapper / Jepsen
/ real-node tests): it needs root, a real block layer, and a real node, none verifiable
in the C4-verify worktree, and DST determinism forbids containerizing it (INTEGRATION §3).
Faking it would be a green mechanical check on something adjacent. The runners *exist* and
gate correctly; the harness lands with its environment.

## Verification run

`PDCA_WORKTREE=… ./engine/xtask.sh ci` → **`xtask ci: all checks passed`** (fmt --check,
clippy -D warnings, build, test, cargo-machete, cargo-deny, conformance, and the madsim
DST sweep). The Tier-0 custodian suite: 7 tests green across the 50-seed sweep (incl. the
committed regression seeds). `cargo fmt --all` and `cargo clippy` (incl. `-p wyrd-dst`
under `--cfg madsim`) are clean, so the patch is commit-ready for the target's hooks.
