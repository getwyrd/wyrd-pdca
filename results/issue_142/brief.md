# Brief (pointer) — issue 142 / m3.4-gc-custodian

> Plan artifact for an implementation slice whose design ALREADY lives in an
> accepted, immutable host artifact — proposal 0005 (Milestone 3 — custodians),
> PR-sequence step **4** (the GC custodian). This brief POINTS at 0005 (it does not
> restate or re-decide the design; INTEGRATION §2/§6) and carries the fields the
> driver/Do parse, plus the structural-slice fields (invariant, posture, prior-art)
> this category needs. Do reads the **Planning artifact** as authoritative.
>
> This is the direct successor of **#141 (M3.3 skeleton, PR #187 — merged)**: it hangs
> the first *running* maintenance loop off the fenced reconciliation control point that
> slice stood up.

- **Slug:** m3.4-gc-custodian
- **Planning artifact:** `docs/design/proposals/accepted/0005-milestone-3-custodians.md`
  — §"The four custodian loops" / **GC** (`0005:288-295`), the GC step of the
  reconstruction pipeline (`0005:279`), §"The durability plane (telemetry + audit log)"
  (`0005:319-344`), the correctness argument **Q3** (`0005:394-397`), the graduation
  invariants (`0005:486-488`), the PR-sequence **slice 4** (`0005:524-527`), and the
  open question **GC grace-window length is a measurement question, not a magic
  constant** (`0005:585-586`). Supporting: **ADR-0011** (telemetry from first commit),
  **ADR-0012** (backend-agnostic OTel), **ADR-0002** (versioned `ChunkStore` wire),
  architecture §5 (the pending-ledger sweep pattern) and §6.7 (GC inputs + grace
  window). Authoritative; Do cites it for every design claim.
- **Defect / goal:** GC is still a **test-invoked stand-in**, not a running custodian
  loop: `core::sweep_expired_leases` (`crates/core/src/write.rs:251`) deletes expired
  `pending:` **ledger entries** in one atomic commit but explicitly **does not reclaim
  the fragment bytes** ("Orphaned *fragments* are collectable garbage; reclaiming them
  needs a chunk-store delete (a later milestone)", `write.rs:249-250`). So the leased
  garbage M1/M2 produce on failed/partial fan-out, and the orphans deletes /
  completed reconstructions leave, **accumulate unreclaimed**. Promote the stand-in
  into a running GC loop on the fenced custodian that reclaims both input classes via
  the now-existing `delete_fragment`, after a reader-safe grace window.
- **Success criterion:** Demonstrable at C4-verify on base `origin/main` @ `40c3413`:
  (1) the GC loop, **invoked through the real `custodian::reconcile_step` fenced
  control point** (NOT a parallel test-only entry — when a custodian runtime eventually
  runs, it must run THIS code; this is the binding anti-#141 guard), reclaims **both**
  input classes — (a) the **byte** behind an **expired pending-ledger lease** (crashed
  write/repair garbage) and (b) an **orphaned fragment** (present in a D server's
  `list_fragments` but referenced by **no** committed chunk map) — by calling
  `ChunkStore::delete_fragment`. BINDING (the two reclaim inputs AND the real-control-point
  invocation; the loop's internal scheduling is ILLUSTRATIVE).
  (2) a fragment **referenced by a committed chunk map** (a `ChunkRef.placement`
  entry) is **never** passed to `delete_fragment`. BINDING — the silent-corruption
  invariant; this is the flippable red→green (negate the reference check → a referenced
  fragment is deleted → the assertion fires).
  (3) an orphan whose grace window has **not** yet elapsed is **not** reclaimed — a
  reader holding the prior version within the window still resolves its fragment;
  reclamation happens **only after** the reader-safe grace window. BINDING (the window
  is honoured; its numeric *length* is out of scope — see Scope).
  (4) GC actions (reclamations, and skips of still-referenced / within-grace
  fragments) are emitted on the existing durability-plane seam
  (`custodian::DurabilityTelemetry`) as **metric + append-only audit events**,
  read back **in-process** (`DurabilityTelemetry::gather_prometheus`). BINDING that
  emission occurs through the backend-agnostic seam (ADR-0012); the in-process
  assertion mechanism is ILLUSTRATIVE; a live Prometheus scrape / OTLP collector run
  is supplementary evidence off-Check.
  (5) BINDING boundary: the loop stays inside `custodian` over `traits`/`core`/`proto`
  (+ `tracing`) — **no** concrete backend dependency (ADR-0010).
- **Invariant to restore:**
  • **Never reclaim a referenced fragment** — GC deletes a fragment's bytes only if
  **no** committed chunk map references it (no `ChunkRef.placement` entry points at it)
  AND it is genuinely expired/orphaned. Violation is **silent corruption** — the
  request plane reports success over data GC destroyed. Source: 0005 §GC (`0005:294-295`),
  Q3 (`0005:394-397`), graduation invariant (`0005:488`). (Spans orphan detection
  across the zone's committed chunk maps + the delete decision — not satisfiable by
  guarding one module.)
  • **Readers are never torn (grace window)** — an in-flight reader holding the prior
  version is never torn: a fragment is reclaimed only **after** a reader-safe grace
  window, long enough that no in-flight reader still depends on it. The window is
  **derived from reader version-hold / pending-lease semantics, not a magic constant**.
  Source: 0005 §GC (`0005:291-294`), Q3 (`0005:397`), open question (`0005:585-586`),
  the pending-ledger sweep pattern (architecture §5). (Spans the timing/lease model +
  the reclaim gate — not a single-module guard.)
  • **Commit-point-atomic / collectable-garbage** — a crash mid-GC leaves collectable
  garbage, never corruption or a torn read (a partial reclaim is itself collectable on
  the next pass). Source: 0005 graduation invariants (`0005:486-488`).
  • **Telemetry + audit from first commit** — every GC transition is observable on the
  backend-agnostic OTel seam from this loop's first commit; no backend hardcoded.
  Source: ADR-0011, ADR-0012, 0005 §"The durability plane" (`0005:336-340`).
- **Repo + branch target:** getwyrd/wyrd @ main   (INTEGRATION §2 — single line, no
  maintenance branches)
- **Surfaces:** data   (custodian loop + chunk-store byte reclaim + telemetry/audit
  emission; no GUI — dashboards/UI deferred per ADR-0013)
- **Scope:** promote the GC stand-in into a **running GC custodian loop** driven
  through the fenced reconciliation control point (`custodian::reconcile_step`,
  `crates/custodian/src/reconciliation.rs`), reading authoritative state and reclaiming
  its **two inputs** (`0005:288-291`): expired **pending-ledger leases** — extend
  reclamation to the **fragment bytes** behind an expired lease, not just the `pending:`
  ledger entry the stand-in already removes (`core/write.rs:251`) — and **orphaned
  fragments** discovered by diffing a D server's `ChunkStore::list_fragments`
  (`traits/src/lib.rs:100`) against the committed chunk maps' placement records
  (`core::metadata::ChunkRef.placement`, `crates/core/src/metadata.rs:94`); reclaim
  bytes via `ChunkStore::delete_fragment` (`traits/src/lib.rs:108`) **only after** a
  reader-safe grace window derived from reader version-hold / lease semantics; emit GC
  metric + append-only audit events on the existing `custodian::DurabilityTelemetry`
  seam.
  / **out of scope:** **the running custodian process/runtime** — no deployed custodian
  host exists for ANY loop yet (the crate is lib-only; `reconcile_step` has no production
  caller; `server/src/cli.rs:49` "runs no custodian sweep"). Standing up a custodian
  binary / spawned-task that elects leadership and drives the loop against live stores is
  a cross-cutting concern gating all four loops, not slice 4's (Option A, agreed with the
  human). This slice makes the GC reconciler **correct over the `MetadataStore`/`ChunkStore`
  abstractions and reachable through the real `reconcile_step` control point**; deploying
  it in a running process is a later concern. ALSO out of scope: the other three loops —
  **scrub** (slice 5), **reconstruction + repair-vs-serve priority** (slice 6),
  **rebalance + drain/decommission** (slice 7);
  the **exact grace-window length** (a measurement/tuning question per `0005:585-586` —
  this slice makes the window reader-safe and configurable/derived, not a tuned
  constant); the **DST campaign + Tier-1/Tier-2 fault injection** (slice 8); the
  version-conditional commit-point-atomic *relocation* of a live fragment (slice 6); any
  change to the on-disk format (unchanged, `0005:552-554`).
- **Test file:** `crates/custodian/tests/gc.rs` (new) — driving GC **through the real
  `reconcile_step` control point** (not a test-only entry), the loop reclaims (a) the
  byte behind an expired pending lease and (b) an orphaned fragment absent from every
  committed chunk map; a fragment referenced by a committed chunk map is **never**
  passed to `delete_fragment`; an orphan within its grace window is **not** reclaimed
  and a reader holding the prior version still resolves it, and **is** reclaimed once
  the window elapses; the GC actions are emitted on `DurabilityTelemetry` and read back
  in-process.
- **Verification posture:** Mixed.
  (a) **Flippable red→green** (default) for the invariant legs: never-reclaim-referenced
  (criterion 2) and grace-window-honoured (criterion 3) are genuine red→green — negate
  the reference check / the window gate and the orphaned/within-grace assertion fires.
  The two-input reclaim (criterion 1) is flippable against the current stand-in, which
  reclaims **no** fragment bytes at all.
  (b) **NET-NEW coverage — a red/born-at-tier C4 is acceptable here (agreed with the
  human).** The GC reconciler, the orphan scan, and the running-loop construct are all
  **new code** arriving WITH their test (the reconciliation scaffold currently returns
  `Reconciled::Satisfied` unconditionally, `reconciliation.rs`) — so the C4 "red" is
  legitimately **criterion-ABSENCE** (no prior failing assertion to flip), not a flip of
  pre-existing behaviour. C4-verify need NOT show a clean red→green against existing
  code, and Check should not treat a born-at-tier red as a defect. Where a flippable
  demonstration is *cheap*, Do SHOULD still take it (it confirms the seam is
  load-bearing rather than green-on-nonexistence): negate the reference-check to show a
  referenced fragment getting deleted, or the window gate to show early reclaim — but
  this is a SHOULD-where-feasible, not a gating MUST. The telemetry/audit emission is
  asserted **in-process** via `gather_prometheus`; a live Prometheus scrape / OTLP
  collector run is supplementary evidence **off-Check** (the human/CI confirms).
  (c) **DECLARED deferred posture — no running custodian process (Option A, agreed).**
  No deployed custodian runtime exists on base for ANY loop (lib-only crate;
  `reconcile_step` has no production caller; `cli.rs:49`). So this slice's green is the GC
  reconciler exercised **in-process through the real `reconcile_step` control point** over
  the trait stores — NOT a live deployed sweep. This is declared HERE so C2/C4 land it as
  a **pre-agreed sign-off item, not a NEEDS-HUMAN surprise** (cf. #141's T4 FAIL, where
  test-only callers were shipped as "production wiring"). The anti-#141 guard is binding:
  the loop MUST be the code `reconcile_step` dispatches to (one production entry), never a
  parallel test-only function. The deployed-process host is confirmed off-Check, in the
  later runtime/DST slices (0005 slice 8); whoever wires the custodian binary runs THIS
  loop unchanged.
  (d) **No new dependencies expected** — `list_fragments`/`delete_fragment` (#140,
  merged) and the OTel/Prometheus telemetry deps (#141/#187, merged) are already in
  the workspace; if Do nonetheless needs a new crate, that is a sign-off NEEDS-HUMAN
  (cargo-deny three-test audit + `deny.toml`, ADR-0003 / INTEGRATION §4/§10).
- **Citations expected:** Do cites `path:line` on `origin/main` (@ `40c3413`) AND
  proposal 0005 for every change. Pre-existing seams: `core/write.rs:251`
  (`sweep_expired_leases` stand-in; `:249-250` the deferred byte-reclaim note);
  `core/src/metadata.rs:94` (`ChunkRef.placement` — the committed references GC must
  never reclaim), `:196` (`commit_chunk_map`), `MetadataStore::scan`
  (`traits/src/lib.rs:180`); `traits/src/lib.rs:100` (`list_fragments`), `:108`
  (`delete_fragment`); `custodian/src/reconciliation.rs` (`reconcile_step`, the fenced
  control point), `custodian/src/leadership.rs` (`Custodian::term` / `FencedZone::
  authorize`), `custodian/src/telemetry.rs` (`DurabilityTelemetry`, `gather_prometheus`),
  `custodian/src/lib.rs:23-29` (module exports).
- **Prior-art check (triage cycles):** searched merged history + open/closed PRs by
  file path. **#141 (PR #187) is MERGED** into base (`origin/main` @ `40c3413`): the
  `custodian` crate exists with `leadership.rs` (fenced `Custodian`/`FencedZone`),
  `reconciliation.rs` (the `reconcile_step` scaffold that returns `Satisfied` and
  **explicitly defers the loops' behaviour to slices 4–7**), and `telemetry.rs` (the
  OTel dual-export seam) — this slice is built on, not in conflict with, it. **#140
  (PR #186, merged)** added `list_fragments`/`delete_fragment`. **#139 (PR #185,
  merged)** added the placement record (`ChunkRef.placement`). **No** GC loop, orphan
  scan, or fragment-byte reclaim exists on `origin/main` — `sweep_expired_leases` is
  the only GC code and it reclaims ledger entries only. No open/closed PR carries a GC
  custodian loop.
- **Disposition hint:** likely-fix
- **Ordering note:** #141 (PR #187), #140 (PR #186) and #139 (PR #185) are **already
  merged into `origin/main`** (this slice's base, `40c3413`), so all prerequisites —
  the custodian crate + fence + reconciliation scaffold + telemetry seam (#141), the
  `ChunkStore` enumerate/delete (#140), and the placement record (#139) — are satisfied
  **in-base**. No `Depends on` / `Conflicts with` scheduling line is needed.

## STOP discipline

Draft only until Check sign-off. A draft PR MAY be opened for CI; the PR MUST NOT be
marked ready before sign-off accepts.
