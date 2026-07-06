# Brief (pointer) — issue 366 / obs-floor-observability

> A Plan artifact that is a **pointer**: the planning decision already lives in a
> governed proposal (**0010**, the observability floor — primary spec), with **0015**
> (accepted M4 plan of record) binding it where the runnable-custodian role is a named
> M4 deployment prerequisite. This file references those and carries the fields the
> driver parses; Do reads the **Planning artifact** as the authoritative plan and does
> not restate it here.

- **Slug:** obs-floor-observability

- **Planning artifact:**
  `docs/design/proposals/draft/0010-observability-floor-for-first-deployment.md`
  — **primary, authoritative spec.** Read specifically: §"Scope boundary" (the seven
  in-scope floor items and the explicit out-of-scope carve-outs), §"Crate touch-points",
  §"DST and tests", §"Suggested PR sequence" (the seven independently-PR-able slices,
  each with its own DoD), §"Sequencing note — the one M4 collision", and §"Graduation
  criteria". **0015 binds where the custodian role is an M4 prerequisite:**
  `docs/design/proposals/accepted/0015-milestone-4-production-metadata-backend-revised.md`
  — read the *Deployment prerequisite* note (L448–478) and slice-5 gating (L660–710):
  0015 names "**runnable gateway/custodian process roles**" (the custodian binary was
  deferred in M3, [0005]) as a prerequisite for slice-5 "peers discovered through L5",
  so this issue's floor-item 2 (custodian as a deployable role) is half of that
  prerequisite. Per the **2026-07-04 maintainer decision**, #366 (custodian process) + #364
  (gateway S3 server) satisfy 0015's process-role half — no separate process-roles issue.
  Ground against the design corpus, read in place under `../wyrd` (never
  copied): architecture **§7.4** (`docs/design/architecture/07-deployment-view.md:51`,
  the single-zone day-one verification ordering — step 4 is the kill-a-D-server /
  under-replicated-rise-then-zero loop), the `m4-first-deployment-blueprint.md` (the
  day-one checklist and the `wyrd custodian --otlp-endpoint …` bring-up command 0010
  makes true), and **ADR-0012** (OpenTelemetry, dual-export, no hardcoded backend) +
  **ADR-0011** (durability telemetry).

- **Defect / goal:** running Wyrd in production has **no operational-visibility floor**.
  Verified against the tree (0010 Motivation, confirmed on the target branch): the
  durability telemetry seam is library-only — `DurabilityTelemetry`/`ExporterConfig`
  (`crates/custodian/src/telemetry.rs:31,67,76`) have no non-test caller; the custodian
  is a `dst`-only dependency (`crates/dst/Cargo.toml:44`) and the server runs no
  custodian loop (`crates/server/src/cli.rs:47` "The CLI runs no custodian sweep"); no
  `tracing` subscriber is installed at any binary entry; the request/capacity planes are
  uninstrumented; errors are opaque `BoxError` (`crates/traits/src/lib.rs:59,62`) except
  the one typed `IntegrityFault` (`:86`); and `health()`
  (`crates/chunkstore-fs/src/lib.rs:320`, the `Health` enum at
  `crates/traits/src/lib.rs:224`, trait method `:275`) is exposed through no
  readiness/liveness probe. Deliver the **minimum floor**: structured logs, health +
  readiness probes, metrics (request health + a storage-capacity / durability signal),
  **and the background data-repair custodian runnable as its own deployable process**.
  This floor **gates** the M4 real-world campaign (#367).

- **Success criterion:**
  - **BINDING (demonstrable at Check, the day-one signal):** through the **wired,
    runnable custodian role** (not the library alone), after an injected fragment loss
    the **under-replicated / durability count RISES and then returns to ZERO**,
    observed via the emitted metric read back in-process
    (`DurabilityTelemetry::gather_prometheus`, `crates/custodian/src/telemetry.rs:135`).
    This closes the sim-only gap 0010 names and satisfies architecture §7.4 day-one
    step 4. The metric mechanism / label names are ILLUSTRATIVE; the binding condition
    is "killing a D-server makes the durability count rise then return to zero,
    observable via a metric emitted by a running process."
  - **ALSO built (per 0010 graduation criteria, may land across the 7-PR sequence):** a
    `tracing` subscriber + exporter installed at role entry with `RUST_LOG`/`--log-level`
    working; a `tonic-health` readiness probe reflecting `health()` (unhealthy store
    reads not-ready); request-plane RED (per-op latency + error-by-class) and
    capacity-plane admission events emitting over the dual Prometheus/OTLP seam with no
    backend hardcoded (ADR-0012); errors classifying transient vs terminal at the trait
    seam with `IntegrityFault` still distinct.
  - **DEFERRED (off-Check, supplementary evidence — 0010 DST section, ADR-0012):** a
    live Prometheus scrape / OTLP collector run against the blueprint's day-one checklist
    on a Tier-2 single node. Confirmed by an operator / CI-eval run; ultimately the
    first-deployment gate (#367).

- **Repo + branch target:** getwyrd/wyrd @ `feat/m4-production-metadata-backend`
  (the M4 integration branch — resolves to `origin/feat/m4-production-metadata-backend`;
  0010 runs "in parallel with M4 implementation" and its custodian-role item is an M4
  deployment prerequisite, so the floor slices are PR'd **into** this integration base.
  0010's own suggested per-slice branch is `feat/obs-floor.<n>-<slug>`, PR'd into this
  base, not `main`.)

- **Depends on:** builds on the merged M3 custodian work ([0005]: `telemetry.rs`, the
  maintenance loops, the failure-domain model). Otherwise **parallel with M4** — 0010's
  PRs 1–2 and 4–7 are disjoint from M4's metadata surface and parallelize freely. The
  **typed-errors** slice (item 6, `crates/traits/src/lib.rs`) has a sequencing coupling
  with M4.4 (#255), which swaps the metadata backend behind the same trait — see
  Ordering note.

- **Conflicts with:** **#255 (M4.4)** — the typed-error enum (floor item 6) lives at the
  trait seam `crates/traits/src/lib.rs` that M4.4 also churns (backend selection behind
  `MetadataStore`). A richer enum ripples into the TiKV implementation, which must
  produce the new variants (0010 Sequencing note).

- **Scope:** the minimum operational-visibility floor (0010 §"Scope boundary" items 1–7):
  (1) telemetry handle + `tracing` subscriber/exporter wired at every role entry;
  (2) **`wyrd custodian` as a runnable, deployable role** (server depends on the
  `custodian` crate, runs the leader-elected loop, installs the telemetry handle);
  (3) operational logging (`--log-level`/`RUST_LOG` EnvFilter, structured stderr);
  (4) request-plane RED (per-op latency + error-by-class); (5) capacity-plane signals
  (admission admitted/shed/timed-out events, in-flight/stream gauges, per-failure-domain
  utilization); (6) typed transient/terminal errors extending `IntegrityFault`;
  (7) `tonic-health` liveness/readiness reflecting `health()`. Optionally the shared
  `crates/telemetry` extraction (0010 Open questions; confirm at build). Each item is
  independently PR-able per 0010's suggested sequence.
  / **Out of scope:** full dashboards / operator portal / management API / auth / RBAC /
  audit log → M8 ([0008]); tracing beyond the floor (OTel span graph, cross-plane
  request↔durability correlation → a future ADR-0036); alerting rules; the d-server
  performance program ([0009]); the TiKV metadata backend and M4's backend composition
  (#255 and the M4 slices); the etcd-backed `Coordination` + gateway process role (the
  *other* half of 0015's deployment prerequisite — its own body of work); any change to
  the commit protocol, consistency contract (ADR-0015), or on-disk format (ADR-0002/0019).

- **Ordering note:** land floor items **1 then 2** first (0010's keystone: the subscriber
  wiring is a prerequisite for every other emission item; item 2's runnable custodian is
  what makes the already-built durability plane real and delivers the binding day-one
  signal). Item **6 (typed errors) must be sequenced against #255 (M4.4)** before the two
  run in parallel — land the enum first so M4's TiKV backend targets the final shape, or
  land it after and adapt (0010 Sequencing note; decision must be **recorded** per 0010
  graduation criteria). Items 4, 5, 7 parallelize freely.

- **Do model:** opus-xhigh
- **Difficulty:** **high** — a milestone-scoped body of work spanning ~6 crates (`server`,
  `custodian`/new `telemetry`, `core`, `traits`, `chunkstore-grpc`, `dst`), introducing a
  new runnable process role and a trait-seam error enum with a known M4 collision. It
  **does not fit one `patch.diff`**: it decomposes into 0010's seven independently-PR-able
  slices. The single load-bearing at-Check demonstrable is the day-one durability signal
  through the wired custodian role (items 1–2); the driver/human should treat the
  remaining items as follow-on slices under the same milestone.

- **Test file:** `crates/dst/tests/custodian.rs` (extend — the M3 custodian property
  campaign already lives here; blob confirmed on the target branch) **or** a new
  `crates/dst/tests/observability_floor.rs` (path ILLUSTRATIVE). The flippable at-Check
  regression: in a simulated custodian role with the subscriber + Prometheus surface
  installed, after an injected loss assert the under-replicated count **rises then
  returns to zero** via `gather_prometheus` read-back — RED before the role/plane is
  wired (nothing emits), GREEN after. Companion DST properties named by 0010 (emission
  read-back per plane, typed errors surviving the gRPC seam, an unhealthy store flipping
  the readiness probe) attach to their own slices.

- **Citations expected:** Do must cite `path:line` on the target branch
  `feat/m4-production-metadata-backend` **and** proposal 0010 for every change. Confirmed
  anchors (target branch): `crates/custodian/src/telemetry.rs:31,67,76,116,135`;
  `crates/dst/Cargo.toml:44` (custodian is a `dst`-only dep today);
  `crates/server/src/cli.rs:47` (no custodian sweep); `crates/traits/src/lib.rs:59,62`
  (`BoxError`), `:86` (`IntegrityFault`), `:224` (`Health` enum), `:275` (`health()`);
  `crates/chunkstore-fs/src/lib.rs:320` (`health()` impl). Line numbers were re-verified
  on the target branch; if a slice rebases, **confirm at build**. The M0 logging-deferral
  anchor (`cli.rs:8-9`) is cited by 0010 against an earlier tree — **confirm at build**.

- **Disposition hint:** likely-fix (for the keystone slice — items 1–2 delivering the
  day-one durability signal); the full floor is a multi-slice milestone whose later items
  land as their own bundles.

## Invariants to hold

- **Purely additive instrumentation + wiring.** Touch **no** commit protocol, **no**
  consistency contract (ADR-0015), **no** on-disk format / `format_version`
  (ADR-0002/0019). Custodian loops, EC path, and gRPC data path gain emission points, not
  new logic (0010 §"What carries over, unchanged").
- **No concrete telemetry backend leaks into a leaf crate** — the dual Prometheus/OTLP
  export seam stays behind `ExporterConfig`, backend chosen at role entry (ADR-0012,
  ADR-0010).
- **Reuse, don't rebuild** the durability telemetry seam (`crates/custodian/src/telemetry.rs`);
  generalize it to the request/capacity consumers rather than forking a second path.
- **`stdout` is payload** — operational logs go to stderr; preserve the M0
  stdout-is-payload discipline when lifting the logging deferral.
- The typed-error enum is **additive** on the pre-1.0 trait seam; existing `BoxError`
  callers keep compiling through a `From`/boxing path; `IntegrityFault` stays a distinct
  terminal class.
- **redb stays dev/eval only**; this floor changes no backend's production status
  (ADR-0014) — it is orthogonal to M4's TiKV work except the item-6 collision.

## Known NEEDS-HUMAN

- **Milestone decomposition.** This issue is milestone-scoped, not a single fix. The
  human/driver must decide how it maps onto 0010's seven-PR sequence and which slice this
  bundle carries (the keystone items 1–2 are the recommended first bundle).
- **Typed-errors × M4.4 (#255) sequencing** — 0010 requires this decision be **recorded**
  before the two run in parallel (land the enum before or after M4's TiKV backend).
  Human call, not the builder's.
- **Shared `crates/telemetry` extraction vs keep-in-`custodian`** — 0010 leans extract
  (second/third consumer now exists, ADR-0016) but defers the call to implementation.
- **Live-exporter evidence is off-Check** — the Prometheus-scrape / OTLP day-one run
  needs a Tier-2 host and is supplementary; declare it a pre-agreed sign-off item, not a
  surprise NEEDS-HUMAN.

## STOP discipline

Do reads **`brief.md` only** and produces `patch.diff`, the named test, and
`build-notes.md`. The test must fail pre-fix and pass post-fix; cite `path:line` on the
target branch for every change. Do MAY push to a feature/draft branch and open a **draft**
PR into `feat/m4-production-metadata-backend` (useful for CI). Do MUST NOT mark a PR ready
or merge it — that is the human's sign-off step (enforced by the builder PreToolUse hook).
Do NOT restate proposal 0010/0015 — reference them; do NOT invent scope beyond the seven
floor items; do NOT fabricate code facts — cite `path:line` or mark "confirm at build".

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Rebuild to (1) extract the telemetry seam into a shared `crates/telemetry` crate — `DurabilityTelemetry`/`ExporterConfig`/`metrics_layer` must not be anchored in `custodian`; the request-plane (item 4) and capacity-plane (item 5) consumers need it flexible, and extracting now avoids the painful refactor once `server`/gateway depend on it (maintainer decision: T5(a), 0010 Open questions -> extract). (2) Deliver the deployable custodian process — wire `wyrd custodian` in the `server` crate (server depends on `custodian`/new `telemetry`, runs the leader-elected loop, installs the telemetry handle), not just the library `CustodianRole` seam; #366 is the sole owner of this half (2026-07-04 decision) and #367's day-one runbook needs it runnable. Keep the day-one signal (under-replicated rises->zero via `gather_prometheus`) green through the wired binary. </content> </invoke>
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected: the day-one durability signal is only demonstrated on a curated happy path, and the metric that IS the signal blind-spots the most severe event. The rebuild must CORRECT the following. Must be corrected: - Adv-1: the deployable custodian crashes on a killed D-server. An unreachable/ timed-out fetch is classified transient (reconstruction.rs:341), so the error propagates reconcile -> reconcile_step -> reconcile_pass -> run_until (.await?) -> cmd_custodian and the process exits on the very fault it exists to repair. The at-Check test dodges this by hand-building a healthy_fleet that EXCLUDES the killed server (custodian_day_one.rs:1074-1079). Fix: exercise the real production path — fleet built from all --endpoints including the node that dies — and make the role survive the kill rather than exit. - Adv-2: the under-replicated gauge undercounts the worst losses. emit_under_ replicated(plans.len()) counts only Repairable chunks (survivors >= k); chunks with survivors < k (Unrepairable) and malformed placements emit 0. Concrete case: kill two fragments of an RS(2,1) chunk -> gauge reads 0 while the chunk is un- reconstructable. Fix the gauge so the binding "rises then returns to zero" signal covers un-repairable / lost-beyond-tolerance chunks, not just auto-repairable loss. - §6.3 coordination: `wyrd custodian` advertises "single-active" leadership but constructs a process-local MemCoordination that always grants leadership to the lone process (coordination-mem/src/lib.rs:184). Two deployed custodians do not fence each other and both run reconstruction. Implement cross-process leader election correctly so the deployable role is genuinely single-active. Related to fix while in there (adversary/codex weaker notes): - The role installs no global tracing subscriber, yet a comment claims it does (custodian.rs:606-608); non-metric events (audit lines, malformed-placement warn) are dropped in production. Either wire the subscriber or correct the comment. - DServerId assigned by --endpoints index (cli.rs:519-527) can mis-key placements and undermine failure-domain distinctness; use the D-server registration's stable id / failure_domain. Not raised for this iteration: §6.1 T5 record (milestone-slice-#1 confirmation, typed-errors x #255 sequencing) and §6.2 live-exporter fitness remain open judgment/ validation items to be revisited at the next sign-off.
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected: both gates red, and the keystone fails its own binding purpose — the deployable custodian is wired to the wrong metadata plane. Primary defect (must fix): cmd_custodian (cli.rs:516) hardcodes `open_local_meta_redb(data_dir)` — no resolve_backend call, no --metadata-backend flag, no TiKV arm. M4 production runs the TiKV metadata backend (redb is dev/eval only), so on a real cluster the custodian opens an empty local redb, sees zero chunks / zero repair obligations, and the day-one under-replicated gauge never rises — the signal is undemonstrable on the very deployment (#367) it gates. The solution MUST route the custodian through the backend: reuse the established seam (resolve_backend + --metadata-backend flag + `#[cfg(feature="tikv")] MetadataBackend::Tikv => open_tikv_meta().await?`), exactly as cmd_put/cmd_get and the helpers at cli.rs:865/910 already do. And the day-one signal must be demonstrated driving cmd_custodian against the backend, NOT a hand-built in-memory MemMeta (custodian_day_one.rs:352 masks the defect). Also fix (implementation, all flagged and unaddressed/renamed since iteration 2): - No-timeout hang: live-fleet probe awaits health() with no deadline via the no-timeout GrpcChunkStore::connect (cli.rs:558 / custodian.rs:606). A paused/partitioned peer hangs the reconcile loop forever; the day-one "survive a killed D-server" only holds for connection-refused. Use connect_with_timeout (it exists for exactly this) so partition/pause is survived. Test's DeadDServer returns Err instantly and never exercises the hang. - Fabricated failure domains: cmd_custodian keys D-servers positionally with synthetic domain-{i} (cli.rs:572-574), ignoring each server's registered failure_domain — a rebuilt fragment can be re-placed into the same real domain as a survivor, defeating the durability invariant. Use the D-server's stable id/failure_domain, don't invent topology. - Test must drive the real loop: custodian_day_one.rs calls live_reconstruction_view/reconcile_pass directly and never run_reconstruction_until/cmd_custodian; the advertised survival behaviour (Store->continue, Fenced->stop) is untested. Gate notes: C4-ci (gating) red at rebalance.rs:884 — a flaky tracing-callsite-race read-back test, pre-existing (patch doesn't touch rebalance.rs) but it's the SAME process-global read-back mechanism this keystone's own evidence rides on; re-run/stabilize. C4-verify red is a new-crate rename artifact in run-verify, so per-fix red->green was never mechanically established and the "red" is a compile error, not the defect. Open judgment for the next Check (not blocking the do): cross-process leader election (iteration-2 §6.3) was re-scoped to out-of-scope; the MemCoordination single-active is host-local only. Confirm the deferral is acceptable or require it. §6 items: none ticked — cannot accept while gates are red; open items are the reject basis.
- Failing gate: C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) — xtask: `cargo test --workspace --exclude wyrd-dst` failed with exit status: 101
- Failing gate: C4 per-fix red->green: this patch's test red pre-fix, green post-fix (advisory) — error: pathspec 'crates/telemetry/src/lib.rs' did not match any file(s) known to git
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 4 — carry-forward (from the previous attempt)
- Sign-off rationale: Gating C4-ci is RED (exit 101) and, unlike a wave-off flake, the red is in the exact seam the keystone's binding evidence rides on. The adversary reproduced it (pass, pass, FAIL on the 3rd run) at `crates/custodian/tests/rebalance.rs:884`: `tracing` caches per-callsite interest process-globally, so under the parallel gate a sibling test evaluates the metric callsite first with no MetricsLayer active (Interest::never) and `gather_prometheus()` returns only target_info. This is the SAME DurabilityTelemetry->tracing->OTel->gather_prometheus read-back mechanism the day-one signal asserts through (custodian_day_one.rs:1372), and the patch does not fix it — it adds MORE read-back tests on the fragile seam. The harness-level flake is captured as an Act candidate (§10); the deterministic gate is correctly RED. Substantive rebuild items (not just a re-run): - Iteration-3 primary defect REGRESSED: `crates/server/src/cli.rs:585,589` still fabricates D-server identity/topology when `--ids`/`--failure-domains` are absent (keys positionally by endpoint index; uses the endpoint URL as the failure domain) — and the brief's own canonical day-one command omits those flags, so it lands in this branch. Two D-servers in one physical failure domain reached at different URLs get distinct fabricated domains -> a rebuilt fragment can be re-placed into the same real domain as a survivor, defeating durability. Reject missing/mismatched topology for static endpoints or derive it from the real registration seam; the docstring even claims it is NOT fabricated positionally, but the default path does exactly that. - The binding day-one signal never drives the real binary: `cmd_custodian` (cli.rs:518 — arg parse, resolve_backend, backend open, connect_with_timeout, fleet build) is exercised by NO test; tests 1-3 hand-build MemMeta + fleet and call reconcile_pass/run_reconstruction_until directly. The iteration-3 rejection (custodian opening the wrong metadata backend) could regress verbatim with every gate green. Add a backend-driven process test through cmd_custodian. - Fix the read-back race so the binding evidence is deterministic (register the metric callsite / serialize the read-back tests) rather than adding more tests on the flaky seam. Human calls to settle at next Check (record, do not resolve silently): - MemCoordination "single-active" (cli.rs:541) is redb-file-lock-only and UNSOUND on the production TiKV backend (no --data-dir lock): two custodian processes on one host both self-grant leadership and run reconstruction concurrently while logging "single-active". Confirm the deferral or require real Coordination. - C5 probe-and-drop membership (custodian.rs:128) reads around the loss with a TOCTOU window and keeps Health::Unhealthy peers eligible for survivor reads/placement (codex) — decide acceptable-for-#367 or require the classification seam to treat unreachable-during-reconstruction as missing. - Gauge is a level over the repair QUEUE, not the chunk population (reconstruction.rs:135-143) — the "EVERY under-replicated chunk" docstring overstates; a lost-but-not-yet-enqueued chunk contributes 0.
- Failing gate: C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) — xtask: `cargo test --workspace --exclude wyrd-dst` failed with exit status: 101
- Full previous attempt preserved in `iteration-v4/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 5 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected (iter-5): genuine progress (clean telemetry extraction, callsite-race not reproducible, clean-store rise→zero holds, backend-open now tested through a real on-disk redb), but the BINDING success criterion is undercut on a populated store and the exact iter-3/iter-4 reject surface is still uncovered — with a real day-one-breaking bug living in the gap. Required fixes: (1) BLOCKING — the binding "returns to ZERO" is unobservable on a populated store. `crates/custodian/src/reconstruction.rs:164-167` adds `under_replicated += 1` for `Assessment::Malformed`, but `assess` returns `Malformed` (reconstruction.rs:263) BEFORE any fragment is fetched — a chunk with all fragments physically present is counted on a gauge named *under-replicated*, and malformed placements are never auto-repaired, so the obligation is re-counted every pass. A store with one pre-existing malformed chunk floors the day-one drill gauge at ≥1 forever → returns to 1, never 0 (brief.md:52-59 BINDING criterion). Decide/fix: don't count non-fetchable-classification chunks (Malformed) on the under-replicated durability gauge, or split them onto a distinct metric so the rise→zero shape is observable on a populated store. (2) BLOCKING — the binding evidence still never drives the real binary entry `cmd_custodian`; it drives the factored-out `run_reconstruction_over_backend` one layer below. Uncovered glue between them: the `GrpcChunkStore::connect_with_timeout` dial loop (cli.rs:730-735) and the `id: ids[i]`/`failure_domain: domains[i]` fleet assembly (cli.rs:741-749) — the exact surfaces iter-3 (wrong backend) and iter-4 (fabricated topology) were rejected on. iteration-4 asked specifically for a backend-driven process test THROUGH cmd_custodian; the builder tested the helper below it and labelled it "the exact production path" (build-notes §3, cli.rs docstring 774-781) — narrowly false. Build the injectable seam so this is coverable headlessly: (a) abstract the concrete connect call behind an injected `DServerConnector` trait/closure (production wires gRPC by default; tests pass a fake that returns an in-memory ChunkStore and can return Err for one endpoint); (b) introduce an OWNED fleet type so a fake store can be injected — `ConfiguredDServer<'_>` currently borrows (`store: &dyn ChunkStore`), forcing the whole assembly to live inside cmd_custodian; (c) extract the dial+assemble into one testable `connect_fleet(...)` function (require_aligned_topology + connect loop + id/domain mapping in one place); (d) reuse the existing in-memory D-server fake the day-one tests already build, injected THROUGH the connector rather than below it. (3) BLOCKING (lives in the gap #2 leaves) — cmd_custodian exits during the very incident it must survive. The dial loop `connect_with_timeout(...).await ... ?` (cli.rs:731) returns Err on the FIRST unreachable endpoint at startup, so a custodian started/restarted during the day-one killed-D-server incident exits instead of repairing around the down node — contradicting the §7.4 day-one step-4 drill and the code's own comment (cli.rs:736-740). No test catches it because tests inject already-connected in-memory fleets, bypassing the loop. Make the connect-failure policy an explicit, tested decision (start-degraded / repair around a startup-down peer, per the drill), and add a test through the new connector seam where one endpoint errors, asserting the custodian proceeds on the reachable subset rather than exiting. (4) REQUIRED — single-active is advertised but FALSE on production TiKV. cli.rs:549/703-708 print "host-local single-active via the store lock", justified by redb's exclusive file lock — but `--metadata-backend tikv` has no --data-dir/file lock; each process builds its own process-local `MemCoordination` and self-grants, so two `wyrd custodian --metadata-backend tikv` on one host both elect and reconstruct concurrently. Disposition: real cross-host fencing legitimately defers to the out-of-scope etcd `Coordination` backend (#365) — but a process MUST NOT log/document a safety property it does not hold. At minimum correct the log line + docstring to state honestly that single-active fencing is NOT enforced on the tikv arm pending #365. (Human may harden to a must-fix at the next Check.) Standing human calls owed at next Check (not blocking): gauge-over-repair-queue-not-chunk-population scope (reconstruction.rs docstring overstates it); Unhealthy-but-reachable peers kept as survivor/replacement targets (custodian.rs:128) — accept probe stand-in for #367 or require health-state filtering; the `--ids`/`--failure-domains` runbook contract change (usage says optional, require_aligned_topology now rejects); Validation in-process read-back vs live-exporter run; milestone slice-#1 decomposition confirmation. Verification hygiene: C4-verify failed on the new-crate rename artifact (`pathspec 'crates/telemetry/src/lib.rs'`), so per-fix red→green was reasoned not mechanically run — fix run-verify's new-crate-rename handling so the harness can establish red→green, or accept the code-derived reds explicitly.
- Failing gate: C4 per-fix red->green: this patch's test red pre-fix, green post-fix (advisory) — error: pathspec 'crates/telemetry/src/lib.rs' did not match any file(s) known to git
- Full previous attempt preserved in `iteration-v5/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 6 — carry-forward (from the previous attempt)
- Sign-off rationale: Why rejected: the `Unrepairable` (below-k, un-reconstructable) case is mishandled. An Unrepairable chunk is the storage system failing its primary responsibility — data that was meant to be durable is actually lost. That is MORE severe than `Malformed` (a metadata/placement error). Yet the patch gives Malformed the dignified treatment (its own `reconstruction_malformed_placement` metric + a NEEDS-HUMAN audit line) while folding Unrepairable silently into the generic `reconstruction_under_replicated` gauge (reconstruction.rs:189). That both buries the emergency and floors that gauge at >=1 forever, breaking the binding day-one "rises then returns to zero" signal for any store carrying a permanent loss. What to change next: 1. Give `Unrepairable` its own dedicated high-severity metric + NEEDS-HUMAN / audit surface (>= parity with Malformed's emit_needs_human), and REMOVE it from `reconstruction_under_replicated` so that gauge stays a true repairable-backlog level that returns to zero (preserving the binding day-one signal). Add a test: a second reconcile pass over an Unrepairable chunk shows the under-replicated gauge returns to zero AND the distinct data-loss signal is raised. 2. CLI help-text contract (codex): cli.rs:170 still advertises --ids/--failure-domains as optional, but --endpoints now requires them (cli.rs:709). Fix help text / canonical bring-up so operators don't hit a startup error, or derive topology from registration. 3. cmd_custodian end-to-end coverage (T5c): iter-5 asked for a test THROUGH cmd_custodian; still only the factored halves are covered. Add it or ratify. Accepted deferrals — do NOT re-litigate, but they must LAND WITH #365 (in the active development sequence; see SUMMARY §10): - C5 probe-and-drop fleet membership (live_reconstruction_view) -> #365 registration/lease Coordination. - T5(a) / codex tikv single-active fencing (local MemCoordination, warning-only) -> real fencing from #365. - Validation: in-process gather_prometheus read-back is the accepted at-Check fitness evidence; live Prometheus/OTLP scrape deferred to #367.
- Full previous attempt preserved in `iteration-v6/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 7 — carry-forward (from the previous attempt)
- Sign-off rationale: issue_366 — the keystone slice (telemetry seam + runnable custodian + the rise/return-to-zero durability signal) is close, and the governance/scope calls are settled (see below). But the BINDING day-one signal mis-fires on a routine transient fault, and that must be fixed before this ships. MUST FIX — the transient-vs-permanent distinction at the membership/classification seam: - `live_reconstruction_view` (crates/server/src/custodian.rs:195-213) DROPS any D-server whose health probe errs, so a transiently-down server (rolling restart / partition isolating m+1 nodes) has its fragments counted as MISSING in `assess`. When survivors fall below k, `assess` returns `Unrepairable` and the pass fires `emit_data_loss` at ERROR severity ("DATA IS LOST; NEEDS-HUMAN", reconstruction.rs:179 -> :618-625) — a FALSE permanent-data-loss alarm on physically-intact fragments that fully recover when the nodes return. The drop-path bypasses the very transient/permanent distinction `assess` otherwise implements. Fix: do not classify a below-k that is driven by REACHABILITY (server dropped from the view) as confirmed data loss / Unrepairable; distinguish "unreachable right now" from "fragments confirmed gone" before emitting the high-severity data-loss signal. - Add a test that actually exercises it: RS(2,1) (or equivalent), transiently down enough D-servers that survivors < k, assert NO `reconstruction_data_loss` is emitted and the signal recovers when the nodes return. The current suite never covers this — every day-one drill hands the custodian a spare server (server 3), so the false-alarm path is uncovered. ALSO FIX (revised at sign-off, §6.5) — an all-unreachable startup fleet must FAIL LOUD, not exit silently: - `crates/server/src/cli.rs:628` currently prints a notice and `return Ok(())` when `connect_fleet` yields an empty fleet (every D-server unreachable at startup). For a deployable long-running custodian that is exit 0 — the supervisor does not restart and the operator sees nothing. Change it to PANIC (non-zero exit + diagnostic) so a transient fleet-wide outage / bad `--endpoints` is a loud, restartable failure rather than a clean vanish. (The per-peer "start degraded around ONE down server" behaviour stays; only the ALL-unreachable case changes.) ALSO FIX (same binding-signal-integrity class, also uncovered) — "returns to ZERO" must hold for a Repairable chunk whose repair repeatedly aborts: - `RepairOutcome::Aborted` (reconstruction.rs, no free/distinct domain — e.g. minimal cluster at exactly n, one killed) offsets `reconstruction_aborted` but leaves the obligation queued; next pass it re-classifies `Repairable` and re-counts under_replicated, pinning the gauge at >=1 forever. Same floored-gauge failure the iter-5/6 rejections chased, surviving in the never-repaired-Repairable case. Add a no-free-domain drill and make the gauge return to zero (or route the un-repairable-for-now case off the repairable-backlog gauge). RATIFIED this iteration — do NOT re-litigate: - §6.1 membership/fencing (probe-and-drop + warning-only tikv fencing) deferred to the #365 etcd Coordination backend (M4-adjacent; NOT M5/step-ca). #365 is human-accepted with §6 cleared + gates green, so the deferral target is real. - §6.2 (a) keystone = 0010 PRs 1-2; (c) telemetry extraction into crates/telemetry. (b) typed-errors x #255 sequencing is DETERMINED: #255 is merged onto the M4 branch, so typed-errors (item 6 / PR-3) lands AFTER M4 and adapts the TiKV MetadataStore; recorded in SUMMARY §10 for Act to formalize in 0010/an ADR. - §6.3 in-process gather_prometheus read-back accepted as at-Check evidence; live-exporter run deferred to #367; the collector substrate #367 needs is now tracked as getwyrd/wyrd#446. - §6.4 the two-metric split (repairable-backlog gauge + dedicated reconstruction_data_loss) is the ACCEPTED operator contract. (§6.5 was NOT accepted — see the fail-loud/panic MUST FIX above.) Verification-hygiene note (not blocking, recurring): C4-verify is red on the new-crate-rename artifact (git mv custodian/src/telemetry.rs -> telemetry/src/lib.rs), so per-fix red->green is reasoned-in-tests, not mechanically run. Either teach run-verify new-crate-rename handling or keep accepting code-derived reds explicitly.
- Failing gate: C4 per-fix red->green: this patch's test red pre-fix, green post-fix (advisory) — error: pathspec 'crates/telemetry/src/lib.rs' did not match any file(s) known to git
- Full previous attempt preserved in `iteration-v7/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
