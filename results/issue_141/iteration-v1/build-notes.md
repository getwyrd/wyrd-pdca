# Build notes — issue 141 / m3.3-custodian-skeleton

Target: `getwyrd/wyrd @ main` (worktree base `3ca818b`). All `path:line` citations
below are against that base / the new files in this patch.

## What the brief asked for (Success criterion)

A new `custodian` crate that demonstrates three things at C4-verify:
1. a single active custodian is **elected and fenced** — a deposed leader's
   coordination action is **rejected** (stale fencing token);
2. the failure-domain selector places `n` fragments across `n` **distinct** domains
   where the topology offers ≥ n domains;
3. the exporter emits a **first custodian metric** with **no backend hardcoded**.
BINDING: the crate depends only on `traits`/`core`/`proto` (+ tracing), never a
concrete backend (ADR-0010).

## What I built

A new L4 crate `crates/custodian` plus the shared selector primitive in `core`:

- `crates/custodian/src/leadership.rs` — `Custodian::elect` campaigns through the
  **existing** `Coordination::elect_leader` (`crates/traits/src/lib.rs:209-211`),
  taking its fenced `Leadership` token (`crates/traits/src/lib.rs:247-252`).
  `LeadershipFence` tracks the zone's current term (the highest fencing token
  granted); `guard` rejects a token strictly below it. This is the first of the two
  guards proposal 0005 §"Single active custodian, fenced" names (the version-
  conditional commit is the second, for the later mutating slices).
- `crates/custodian/src/reconcile.rs` — the reconciliation control-loop skeleton:
  `reconcile_once` gates a (no-op) tick on `fence.guard`, so a deposed custodian
  converges nothing. The loop *behaviour* is slices 4–7, explicitly out of scope.
- `crates/core/src/placement.rs` (new) + `crates/core/src/lib.rs:13` — the
  failure-domain selector: opaque `DomainId`, stable `ServerId`, and
  `select_distinct_domains`, which enforces the n-distinct-domains invariant or
  errors (`InsufficientDomains`) rather than collapsing two fragments into one
  domain. `crates/custodian/src/selector.rs` re-exports it.
- `crates/custodian/src/telemetry.rs` — the backend-agnostic OTel seam: a
  `MetricExporter` trait (no concrete backend named), `emit_startup_metrics` emits
  the `custodian_up` first metric and raises it as a `tracing` event (the OTel
  ingestion point), and `InMemoryExporter` makes emission assertable in-process.
- Workspace wiring: `Cargo.toml` `members` + `[workspace.dependencies]`
  (`wyrd-custodian`, `tracing`).
- Test: `crates/custodian/tests/skeleton.rs` (the path the brief names).

## Key design decision — where the selector lives (an explicit Do call)

The brief's Open question leaves this to Do: "where it lives — core vs. custodian,
re-exported." I put it in **`core::placement`** and re-export from
`wyrd_custodian::selector`. Reason: proposal 0005 requires the selector be **shared
by the write fan-out and custodian re-placement**, and the write fan-out itself
lives in `core` (`crates/core/src/write.rs:162` `write_fragments`;
`crates/chunkstore-grpc/src/fanout.rs:16-18` records that the fan-out logic lives in
`core::write_fragments`). Co-locating the selector in `core` makes "shared with the
write fan-out" a same-crate fact, with **no cross-crate cycle** (custodian → core is
the only new edge; core does not depend on custodian). The custodian re-export keeps
`wyrd_custodian::selector` as the stated public surface.

### Why I did NOT rewire `FanoutChunkStore` in this slice

The brief's Citations expected names `crates/chunkstore-grpc/src/fanout.rs` adopting
the selector, but the brief also marks this slice **"Conflicts with #139"** and
notes #139 "retires `index % n`". #139 is **not merged** on the target base, and the
destructive adoption it owns is what makes the fan-out domain-aware. Quantified cost
of doing it here instead: `FanoutChunkStore::new(Vec<C>)` has **no** domain/server
labels (`crates/chunkstore-grpc/src/fanout.rs:37`), so making `route` domain-aware
forces a new constructor signature carrying per-store `DomainId`, which ripples to
**every** call site — `crates/server/src/cli.rs:427`,
`crates/core/benches/throughput.rs:92`, `crates/dst/tests/network.rs:221,274,331,380,440`,
`crates/server/tests/read_fanout.rs:136`, `crates/server/tests/write_fanout.rs:93`,
`crates/chunkstore-grpc/tests/tier2_integration.rs:95` (≈ 11 sites) — and collides
head-on with the `index % n` retirement #139 is already doing in that same file. The
shared home in `core` is precisely what makes #139's adoption a one-import change.
This is the smallest change that satisfies the Success criterion (the selector +
its distinctness property, which is fully proven in the custodian test) without
pre-empting #139's conflicting edit.

## OTel scope — what is wired now vs. the NEEDS-HUMAN follow-up

The brief makes OTel via `tracing` + `tracing-opentelemetry` (Prometheus + OTLP)
BINDING, **but** its own Impact section and Open questions defer the
`tracing-opentelemetry`/`opentelemetry-otlp`/Prometheus-exporter **versions and
their `deny.toml` allowlist entries** to a sign-off **NEEDS-HUMAN** (ADR-0003
three-test audit; INTEGRATION §4/§10 marks any new dependency human-only). Those two
pulls cannot both hold inside a gate-green, model-only patch: adding the OTel SDK
crates would require editing the `deny.toml` allowlist, which is the human's call.

Resolution: I wired the seam to be **backend-agnostic now** and added only the base
`tracing` facade (which the brief's BINDING list explicitly allows — "+ tracing").
`tracing` introduces only MIT/Apache-2.0 crates, already in the `deny.toml` allowlist
(`deny.toml` licenses), so `cargo deny check` stays green (verified). The criterion's
own verification posture asks exactly for this: the exporter leg is "observable
in-process (assert on the exported metric/span)" and "backend-agnosticism is
confirmed by the dependency-only-on-traits/core/proto build". The `MetricExporter`
trait names no backend, so the Prometheus + OTLP exporter (and its audited deps) drop
in behind it unchanged. I used `default-features = false, features = ["std"]` on
`tracing` to drop the `attributes` proc-macro we don't use, keeping the license
surface minimal. **Expect a NEEDS-HUMAN at sign-off for the eventual OTel exporter
deps** — that is the brief's own stated disposition.

## Verification (red→green)

Posture is NET-NEW infrastructure: "red" is criterion-absence (the crate/API does
not exist on the base, so the test does not compile/the package is unknown). Beyond
that, I proved the two genuine properties are load-bearing by negation:

- **Fenced deposed leader.** Negating the guard (`leadership.token < self.current`
  → `false`, deposing nobody) makes
  `elected_custodian_is_fenced_and_a_deposed_leader_is_rejected` **FAIL** ("a
  deposed custodian's action MUST be rejected"). Reverted → green.
- **Selector distinctness.** Negating the distinctness skip (always insert) makes
  `selector_places_fragments_across_n_distinct_domains` **FAIL** (`distinct_domains
  == 2`, expected 3). Reverted → green.
- Exporter leg asserted in-process on the recorded `custodian_up` sample.

Ran through the project's own gate wrapper (`./engine/xtask.sh ci` →
`cargo xtask ci` in `$PDCA_WORKTREE`): fmt `--check`, clippy `-D warnings`, build,
`cargo test --workspace`, DST (`--cfg madsim`), `cargo deny check` (advisories, bans,
**licenses**, sources all ok), and conformance — **all checks passed**. `cargo fmt
--all -- --check` is clean (commit-hook ready). The test stays import-light (no GUI /
heavy load-time deps), safe on a headless runner.

## Out of scope (per brief / proposal 0005)

The four loops' behaviour (GC, scrub, reconstruction, rebalance — slices 4–7), the
version-conditional repair location-update, the `server` custodian subcommand/role
(ADR-0014/0016), the full DST property campaign + Tier-1/Tier-2 fault injection
(slice 8), and dashboards/UI (ADR-0013). `proto` is in the BINDING *allowed* set but
unused by this skeleton, so it is not added as a dependency (avoids an unused-dep
finding); the allowed-set rule ("depends only on traits/core/proto + tracing") holds.
