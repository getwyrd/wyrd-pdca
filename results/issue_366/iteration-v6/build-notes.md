# Build notes — issue 366 / obs-floor-observability (keystone, iteration 6)

Withheld from the reviewer. Rationale + what I ruled out.

## Scope of this iteration

Observability-floor **keystone** (proposal 0010 items 1–2): the durability telemetry seam
extracted to a shared `wyrd-telemetry` crate, and `wyrd custodian` wired as a **runnable,
deployable role** in `server`, proving the day-one signal — kill a D-server → the
under-replicated count **rises then returns to zero**, read back through the role's own
`gather_prometheus` surface.

I did **not** rebuild from scratch. I took **iteration-5's patch as the base** (it applied
cleanly to the current target branch) and corrected the **four rejections** the iteration-5
sign-off recorded, each proven or verified below. I did not re-submit the rejected approach:
every reject item is materially changed.

## The four iteration-5 rejections, each addressed

### 1. BLOCKING — the "returns to ZERO" was unobservable on a populated store

**Reject:** `reconstruction.rs` counted `Assessment::Malformed` on the
`reconstruction_under_replicated` gauge. But `assess` classifies a chunk `Malformed` (a
wrong-length committed placement) **before any fragment is fetched** — a chunk whose fragments
may all be physically present. Malformed chunks are never auto-repaired (ADR-0040 decision 4:
never rebuild over a fabricated identity vector), so the obligation is re-counted every pass →
a store carrying **one** pre-existing malformed chunk floors the gauge at ≥1 forever. The
binding "rise then return to zero" then returns to 1, never 0, on the very production store
(#367) the floor gates.

**Fix (`crates/custodian/src/reconstruction.rs:178`):** the `Malformed` arm now emits only
`emit_needs_human(chunk)` — which already carries a **distinct** metric
(`reconstruction_malformed_placement`, a monotonic counter + NEEDS-HUMAN audit line,
`reconstruction.rs:568`) — and no longer touches `under_replicated`. The gauge now counts the
physical-loss set only (`Repairable` + `Unrepairable`), so it returns to zero once real losses
are repaired, while the corruption is still surfaced on its own metric. Comments at the tally
(`reconstruction.rs:135`) and emit site updated to state the split.

*This is the smallest change that restores the invariant* ("the durability level must be able
to reach 0 on a populated store"): I did not add a new metric or a new code path — the distinct
`reconstruction_malformed_placement` counter already existed, so the fix is a one-line removal
of the erroneous increment plus doc. The alternative (a second gauge for malformed) is strictly
larger and redundant with the existing counter.

**Red→green (mechanical, in an EXISTING file so the verify harness can establish it):**
`crates/custodian/tests/reconstruction.rs:1438`
`under_replicated_gauge_excludes_malformed_so_it_returns_to_zero` drives a **populated** store
with both a repairable loss (chunk A) and a pre-existing malformed chunk (chunk B). Post-fix the
gauge reads **1** (pass 1, the real loss only) then **0** (pass 2, after repair). Reverting only
the `under_replicated += 1` on the `Malformed` arm makes it read **2** then **1** — FAILED,
`left: Some(1.0), right: Some(0.0)` on the return-to-zero assert. Captured live.

### 2 & 3. BLOCKING — the binding evidence never drove the fleet-assembly glue, and the role exited on the day-one fault

**Reject 2:** the evidence drove `run_reconstruction_over_backend` one layer below
`cmd_custodian`; the dial loop + `id`/`failure_domain` fleet assembly (the surface iter-3/iter-4
were rejected on) was covered by no test because `ConfiguredDServer<'_>` **borrowed** its store,
forcing the assembly to live inside `cmd_custodian`. **Reject 3:** the connect loop returned
`Err` on the **first** unreachable endpoint at startup, so a custodian (re)started during the
incident exited on the very fault it exists to repair.

**Fix — the exact seam iteration-5 asked for:**
- **(2b) owned fleet type** — `ConfiguredDServer` now holds `store: Arc<dyn ChunkStore>`
  (`crates/server/src/custodian.rs:97`), so the fleet is a self-contained owned value.
- **(2a) injected connector** — `DServerConnector` trait (`custodian.rs:113`); production wires
  `GrpcDServerConnector` (`crates/server/src/cli.rs:735`,
  `GrpcChunkStore::connect_with_timeout`), tests inject a fake returning in-memory stores.
- **(2c) one testable assembler** — `connect_fleet(...)` (`custodian.rs:143`) holds
  `require_aligned_topology` + the dial loop + the `ids[i]`/`domains[i]` mapping in one place;
  `cmd_custodian` now calls it (`cli.rs:605`) instead of inlining the assembly.
- **(3) start-degraded** — `connect_fleet` **skips** a peer whose `connect` returns `Err`
  (`custodian.rs:165`, logs "starting degraded and repairing around it") and returns the
  reachable subset, rather than propagating the first `Err`. A custodian started/restarted
  during the kill now comes up and repairs around the down node (architecture §7.4 step 4).
- **(2d) fake injected THROUGH the connector** — the day-one tests' existing in-memory
  D-server fake is handed back by a `FakeConnector`, not spliced in below the seam.

**Red→green:** `crates/server/tests/custodian_day_one.rs:974`
`connect_fleet_starts_degraded_around_a_startup_down_peer_and_repairs` drives the exact
production `connect_fleet` + `require_aligned_topology`, with endpoint `e1` down at startup. It
asserts the fleet comes up as `{0,2,3}` (skipping e1, keeping each server's operator-supplied
domain) and the run loop repairs around it → gauge returns to **0**. Reverting the skip to
`Err(e) => return Err(e)` makes it FAIL: `connect_fleet ... : "fake connector: D server 'e1'
unreachable at startup"`. Captured live.

Note on the "irreducibly manual" boundary: I did **not** stand up a real gRPC network. The
connector seam is the honest injection point — the test drives the **production** `connect_fleet`
+ `require_aligned_topology` + owned-fleet assembly + the run loop; only the concrete gRPC dial
(the one line `GrpcChunkStore::connect_with_timeout`) is behind the trait and is exercised by
the production `GrpcDServerConnector`, not re-implemented. That concrete dial against live gRPC
is the DEFERRED live-exporter evidence (§below), consistent with the brief's off-Check item.

### 4. REQUIRED — single-active advertised but false on the tikv backend

**Reject:** the role logged/documented "host-local single-active via the store lock", justified
by redb's exclusive file lock — but `--metadata-backend tikv` has no such lock, so two
`wyrd custodian --metadata-backend tikv` on one host both self-grant via the process-local
`MemCoordination`.

**Fix (`crates/server/src/cli.rs:565`):** the startup log line is now **backend-aware** — redb
states the genuine file-lock single-active; **tikv prints a WARNING** that single-active is NOT
enforced pending the etcd `Coordination` backend (#365), noting CAS commits keep it
corruption-free meanwhile. The `cmd_custodian` docstring (`cli.rs:470`) states the same per
backend. Real cross-process fencing legitimately defers to the out-of-scope etcd `Coordination`
(#365 / 0015's other prerequisite half); this change only stops the process **claiming** a
safety property it does not hold. Left as a recorded human call (below), not silently hardened.

## Commit-readiness

- `cargo fmt --all --check` → exit 0.
- `cargo clippy -p wyrd-server -p wyrd-custodian --all-targets` (workspace `-D warnings`) → exit
  0 (fixed a `needless_lifetimes` the owned-Arc change exposed in `live_reconstruction_view`).
- `cargo test --workspace --exclude wyrd-dst` (the iteration-3/4 **failing gate**, exit 101) now
  exits **0**.
- `cargo test -p wyrd-dst --no-run` compiles (the dst custodian rise→zero property is a single
  repairable chunk, unaffected by the Malformed split).
- `async-trait` promoted from `[dev-dependencies]` to `[dependencies]` in
  `crates/server/Cargo.toml` (the `DServerConnector` object-safe async trait is now built into
  the lib, not just tests).

## Verification hygiene (iteration-5 note)

The iteration-5 `C4-verify` red was a harness artifact: `git checkout HEAD -- <new file>` fails
for files not on the base (the new `wyrd-telemetry` crate). That recurs for any new file. I
mitigated it by placing the **strongest** mechanical red→green (BLOCKING #1) in an **existing**
file the harness can revert (`crates/custodian/tests/reconstruction.rs`), and confirmed both
red→green flips by hand (above). The keystone integration test (`custodian_day_one.rs`) is new,
so its "red" is code-derived (the module it drives does not exist on base); the deterministic
behavioural reds are the meaningful within-patch flips.

## Carried-forward open items (human calls, recorded — not silently resolved)

- **Cross-process leader election** (iteration-2 §6.3 / -5 REQUIRED #4 disposition): `tikv`
  single-active is NOT fenced; the role now says so honestly. Real fencing = etcd `Coordination`
  (#365, out of scope). Human may harden to a must-fix.
- **Reachability probe vs registration/lease membership** (`custodian.rs` `live_reconstruction_view`):
  a stand-in for lease-driven fleet membership; accept for #367 or require the classification
  seam to treat unreachable-during-reconstruction as missing.
- **Gauge is a level over the repair QUEUE**, not the full chunk population — a lost-but-not-yet-
  enqueued chunk contributes 0 (unchanged; documented).
- **Milestone decomposition + typed-errors × #255 sequencing (item 6) + shared-`telemetry`
  extraction confirmation** — the pre-agreed §6 sign-off items.
- **Live-exporter evidence is off-Check** (DEFERRED, brief §Success): a Prometheus-scrape / OTLP
  run on a Tier-2 node against the day-one checklist — the concrete gRPC dial behind
  `GrpcDServerConnector` is exercised there. Pre-agreed sign-off item, not a surprise NEEDS-HUMAN.
