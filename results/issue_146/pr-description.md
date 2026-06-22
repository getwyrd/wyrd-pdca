# Add the Tier-0 custodian property campaign (M3 graduation gate)

## Root cause
M3's four custodian loops (GC #142, scrub #143, reconstruction #144,
rebalance #145) shipped with *per-slice* tests, but the consolidated
verification campaign that proposal 0005 makes M3's graduation criterion
(`0005:500-502`) was never built — there was no Tier-0 property suite sweeping
seeds over the six §13/§10 correctness properties, no storage fault seam to
drive them, and no Tier-1/Tier-2 runners. So the custodian correctness
properties were not continuously machine-checked nor seed-reproducible as
ADR-0009 requires.

## Fix
A net-new verification campaign, no production logic touched:

- **`crates/dst/tests/custodian.rs`** (new, the binding deliverable) — the
  Tier-0 custodian property campaign. All six properties (`0005:378-403`) run
  through the **real** `reconcile_step` fenced control point over in-memory
  `MetadataStore`/`ChunkStore` trait stores, under `--cfg madsim`, swept over
  50 seeds with a committed `REGRESSION_SEEDS` set replayed every run
  (ADR-0009): reconstruct-to-full-redundancy in a distinct failure domain (Q1),
  commit-point-atomic repair under crash (never a hybrid; the stray fragment is
  collectable garbage), scrub-detects-bit-rot-then-reconstructs (Q2), GC
  reclaims only true orphans (Q3), a fenced stale leader lands nothing, and
  durability emission rises then returns to zero.
- **`crates/testkit/src/lib.rs`** — a `StorageFault` / `SeededStorageFaults`
  storage seam (bit-rot / fragment-loss + D-server-kill, `0005:434-435`), the
  storage-plane sibling of the existing `SeededNetFaults`, with unit tests.
- **`xtask/src/faults.rs`** (new) + **`xtask/src/main.rs`** — the deferred
  (off-Check) Tier-1/Tier-2 runners (`disk-faults`, `jepsen`,
  `kill-reconstruct`, `0005:437-438`). They are deferred by default — without
  an explicit `WYRD_TIER1`/`WYRD_TIER2` opt-in they print what they require and
  exit cleanly, so they never run in the unprivileged `cargo xtask ci`. Only the
  pure gating decision is unit-tested; the privileged harness runs in the
  dedicated off-Check job that supplies it.
- **`crates/dst/Cargo.toml`** / `Cargo.lock` — dev-dependency edges
  (`wyrd-custodian`, `wyrd-coordination-mem`, `tracing`, `tracing-subscriber`).

## Verified against
- `crates/dst/tests/` (`main`) — holds only `concurrency.rs` + `network.rs`
  (the M0–M2 DST); no custodian property campaign exists, so the suite is born
  here.
- `crates/testkit/src/lib.rs:196-226` (`main`) — `SeededNetFaults` is the only
  seeded fault plan; there is no storage-plane (bit-rot / fragment-loss /
  D-server-kill) seam. The new `SeededStorageFaults` mirrors its
  `pick`/`faults`/`is_faulted` shape and stays equally import-light.
- `xtask/src/main.rs:31-36` (`main`) — the task dispatch exposes
  `ci|conformance|gen-vectors|dst|integration|bench` with no dm-flakey / Jepsen
  / kill-reconstruct runner.
- `crates/custodian/src/reconciliation.rs:62` (`main`) — `reconcile_step`, the
  anti-#141 fenced entry the suite drives every property through; the loop
  sources (`gc.rs`, `scrub.rs`, `leadership.rs`, `reconstruction.rs`) are
  byte-identical to `origin/main` after the red-demonstration negations were
  reverted — this slice adds no custodian behaviour.

## Test
The suite **is** the deliverable. Tier-0 `crates/dst/tests/custodian.rs` is the
red-before/green-after surface: each of the six assertions was demonstrated
load-bearing by temporarily negating the production guard it protects (e.g.
non-CAS commit, disabled scrub checksum gate, disabled GC reference/grace gate,
authorize-never-fences), confirming each is red without resting on file
absence; all negations were reverted. `PDCA_WORKTREE=… ./engine/xtask.sh ci`
exits 0 — fmt `--check`, clippy `-D warnings`, build, test, machete, deny,
conformance, and the 50-seed madsim DST sweep (7 custodian tests green incl. the
committed regression seeds). Tier-1/Tier-2 are the deferred-posture deliverables
(`0005:405-411`): inert at Check, their green confirmed off-Check by the host
CI Tier-1/Tier-2 jobs and the maintainer at sign-off — a pre-declared sign-off
item.

Fixes #146
