# Brief (pointer) — issue 145 / m3.7-rebalance-drain-decommission

> Plan artifact for an implementation slice whose design ALREADY lives in an
> accepted, immutable host artifact — proposal 0005 (Milestone 3 — custodians),
> PR-sequence slice **7** (`0005:537-540`). This brief POINTS at 0005 (it does not
> restate or re-decide the design; INTEGRATION §2/§6 — an accepted proposal is not
> re-opened, and most Wyrd work plans through its own artifacts) and carries the
> fields the driver/Do parse, plus the structural-slice fields (invariant, posture,
> prior-art) this category needs. Do reads the **Planning artifact** as authoritative.
>
> This is a **structural / lifecycle slice** (it hangs a new maintenance loop off the
> fenced `reconcile_step` control point and introduces a declarative desired-state
> lifecycle — "drain"/"decommission" — plus a new capacity-plane emission), so the
> invariant is stated wide and Scope names no mechanism. It passes the category-gated
> Plan-exit gate: Scope names no probe/guard/helper, and the invariant is not
> satisfiable by guarding one module (it spans the desired-state read/write surface +
> the rebalance loop dispatch + the failure-domain selector + the telemetry seam).

- **Slug:** m3.7-rebalance-drain-decommission
- **Planning artifact:** `docs/design/proposals/accepted/0005-milestone-3-custodians.md`
  — §"The four custodian loops" / **Rebalance** (`0005:297-303`); the shared
  commit-point-atomic re-place pattern §"Repair-vs-serve" (`0005:305-317`) and the
  atomicity graduation line (`0005:486`); §"The durability plane" — the **capacity
  plane's per-failure-domain utilization** (`0005:341-343`); §"Declarative management
  hook" — desired-state read/write + the changed-vs-satisfied moments (`0005:346-356`,
  esp. `0005:351-352`); the PR-sequence DoD (slice 7, `0005:537-540`). Backed by
  architecture §6.3 step 3 (the failure-domain distinctness invariant), §8.4 (desired
  state / reconciliation), §8.3 (capacity plane), and **ADR-0011** rule 2 (declarative
  self-reconciling management) / **ADR-0013** (the full API-first surface + CLI stays
  deferred). AUTHORITATIVE — Do treats the proposal as the spec; this brief adds no
  design of its own.
- **Defect / goal:** M3's custodian has GC (#142), scrub (#143), and reconstruction
  (#144, slice 6 in flight) hung off the fenced `reconcile_step` seam, but **no
  rebalance loop and no declarative drain/decommission**: an operator cannot mark a D
  server **draining / decommissioning** and have its fragments proactively evacuated,
  so a planned server removal cannot preserve durability the way an unplanned loss
  (reconstruction) does. There is also **no per-failure-domain capacity emission**
  (`core/placement.rs` tracks per-`DServerId` used-bytes, `placement.rs:76-114`, but
  nothing aggregates or emits utilization per failure domain). Realize the rebalance
  loop (drain/decommission evacuation), the desired-state reconciliation **hook**
  (single-zone), and the capacity-plane per-failure-domain utilization emission.
- **Success criterion:** Demonstrable at C4-verify, in-process over the trait stores
  (Option A — no deployed custodian process exists yet, `0005:519-523`), dispatched
  through the real fenced `reconcile_step` control point:
  (1) An operator writes **desired state** marking a D server **draining /
  decommissioning**; the custodian reconciles by **evacuating** that server's
  referenced fragments onto healthy D servers in **distinct failure domains** via the
  **same commit-point-atomic, version-conditional `MetadataStore::commit` re-place as a
  reconstruction** (`0005:298-299`, `0005:486`) — after which the drained server holds
  **no referenced fragment** and every affected chunk retains **full redundancy in
  distinct domains**, with **spread preserved** (where rebalance and durability spread
  conflict, **spread wins**, `0005:302-303`). BINDING (the evacuation + spread
  preservation + atomic re-place); the queue-scan / move-selection shape is ILLUSTRATIVE.
  (2) **"Policy changed"** (desired state recorded) and **"policy satisfied"** (reality
  matches — the drained server is empty) are **distinct, observable moments**
  (`0005:351-352`). BINDING (the two observable moments); the concrete desired-state
  encoding and reconciliation-status surface shape are ILLUSTRATIVE.
  (3) **Per-failure-domain utilization** is emitted on the `DurabilityTelemetry` seam
  (`tracing` → OTel, Prometheus + OTLP, no backend hardcoded, ADR-0011/0012). BINDING
  (the per-failure-domain capacity surface); the in-process read-back mechanism is
  ILLUSTRATIVE.
- **Invariant to restore:** Every custodian **re-placement** — whether triggered by an
  unplanned loss (reconstruction) or a planned **drain/decommission** (rebalance) —
  **preserves the failure-domain distinctness invariant** and is **commit-point-atomic**:
  the location update is a single version-conditional commit, so a crashed move leaves
  **collectable garbage, never a torn / hybrid chunk**, and where rebalance and
  durability spread conflict, **spread wins** (durability is gate-zero). And declarative
  management is **observably reconciling**: "policy changed" and "policy satisfied" are
  distinct moments. (Stated over the re-placement / declarative-reconciliation CATEGORY,
  not the repro server; spans the desired-state surface + the rebalance dispatch + the
  shared failure-domain selector + the telemetry seam — NOT satisfiable by guarding one
  module.) Source: proposal 0005 §Rebalance (`0005:297-303`), the commit-point-atomic
  graduation line (`0005:486`), §Declarative management hook (`0005:346-356`),
  architecture §6.3 step 3 / §8.4; ADR-0011 rule 2.
- **Repo + branch target:** getwyrd/wyrd @ main   (INTEGRATION §2: single line, no
  maintenance branches; host suggests `feat/m3.7-rebalance-drain-decommission`,
  `0005:508-509`)
- **Depends on:** 144
- **Ordering note:** #144 (M3.6 reconstruction, slice 6) builds the commit-point-atomic,
  version-conditional re-place machinery (gather survivors → place in distinct domains →
  one version-conditional commit) that a rebalance **move reuses verbatim** (`0005:298-299`
  "the same commit-point-atomic re-place as a reconstruction"); it must be COMPLETE first.
  #142 (GC) and #143 (scrub) are already merged to `main` (provenance only).
- **Surfaces:** data (backend / custodian logic; DST is the substrate, no GUI)
- **Scope:** the rebalance loop and its declarative drain/decommission hook plus the
  capacity-plane emission, as proposal 0005 §Rebalance / §Declarative management hook /
  §"The durability plane" (capacity) specify — i.e. (a) a custodian loop, dispatched
  through the existing fenced `reconcile_step` seam, that evacuates fragments off a D
  server the operator has marked draining/decommissioning, re-placing them in distinct
  failure domains via the shared version-conditional commit-point-atomic re-place so
  spread is preserved and the displaced fragment becomes GC-eligible; (b) the
  single-zone desired-state read/write + reconciliation-status hook (desired state folds
  into the local metadata / coordination config) so "changed" and "satisfied" are
  distinct observable moments; (c) emit per-failure-domain utilization on the durability
  telemetry seam. / **out of scope:** **hot-spot rebalance** — the lighter,
  measurement-driven balancing that "may trail the drain/decommission path"
  (`0005:301-302`) — is NOT binding here (drain/decommission is the load-bearing leg;
  hot-spot may land in a follow-on); the full **API-first management surface and its CLI**
  (ADR-0013, `0005:355-356`, deferred); sharded scrub/repair (Open question, not M3);
  multi-zone / cross-zone placement + the replication-lag metric (L2 / M5/M6,
  `0005:129`, `0005:334-335`); dashboards / alerting / UI (ADR-0013); no new coding math
  and no on-disk-format change.
- **Repro instruction:** On `main` at `../wyrd` (after #144 lands), the custodian crate
  (`crates/custodian/src/`) has gc / leadership / reconciliation / scrub / telemetry but
  **no `rebalance` module**, and `reconcile_step` dispatches only `gc` + `scrub`
  (`crates/custodian/src/reconciliation.rs:55-70` — "Reconstruction / rebalance (slices
  6–7) are not yet dispatched"). There is no desired-state (drain/decommission) surface
  anywhere under `crates/`. Drive a DST scenario that marks a fragment-holding D server
  draining: without a rebalance loop the server is never evacuated and the operator's
  desired state is never satisfied.
- **Test file:** `crates/custodian/tests/rebalance.rs` (mirrors the existing
  `tests/gc.rs` / `tests/scrub.rs`), exercising in the simulator: a drained/decommissioned
  D server is evacuated to distinct failure domains via one version-conditional commit,
  spread preserved, "changed" vs "satisfied" observable, and per-failure-domain
  utilization emitted on the telemetry seam. A bug-finding seed promotes to a permanent
  seeded regression (ADR-0009 rule).
- **Verification posture:** mostly **NET-NEW coverage** (the rebalance loop, the
  drain/decommission desired-state surface, and per-failure-domain utilization are all
  born here), so "red" is partly criterion-ABSENCE on a new file. The default
  flippable-test posture HOLDS for the in-simulator legs via the stable `reconcile_step`
  seam (Do adds the rebalance/desired-state dispatch slot the loop "does not yet
  dispatch"), and because the test ships as its own file (`tests/rebalance.rs`, the
  gate's ADDED_TEST discriminator) C4-verify keeps it on the revert leg and requires it
  red. For the load-bearing evacuation/spread leg (1), capture a **demonstrated**
  assertion-level red — e.g. temporarily negate the spread-preserving placement or the
  desired-state read so the drained server is **not** evacuated (à la scrub's
  `fragment_intact` negation, `tests/scrub.rs:16`) — proving the seam is load-bearing
  rather than resting red on non-existence; record it in `build-notes.md`. Capacity
  emission (leg 3) is read back in-process via the telemetry seam as in `gc.rs`. All
  legs observable at C4-verify; no off-Check/deferred green. Whole-gate confirmation:
  `./engine/xtask.sh ci` (incl. the DST sweep) exits 0 in `$PDCA_WORKTREE`.
- **Production reach:** Option-A in-process — there is **no deployed custodian runtime**
  yet (`0005:519-523`), so the test dispatches through the real fenced `reconcile_step`
  control point in-process; (a) the in-process loop honours the seam now, while the
  operator-facing **live** desired-state write is single-zone "folds into local
  metadata / coordination config" (`0005:353-354`) — there is no networked operator
  API yet; (b) the production operator-facing API + CLI lands later under **ADR-0013**
  (deferred, `0005:355-356`); (c) the in-process dispatch exercises the rebalance seam
  load-bearingly (it drives the real `reconcile_step` + the real failure-domain
  selector + the real version-conditional commit, not dead scaffolding).
- **Citations expected:** Do must cite path:line on the target branch (`main`) AND the
  Planning artifact (0005 line refs, e.g. `0005:297-303`, `0005:346-356`,
  `0005:341-343`) for every design claim and change.
- **Prior-art check (triage cycles):** searched by file path — merged history / open /
  closed PRs — `crates/custodian/src/rebalance.rs` and `crates/custodian/tests/rebalance.rs`
  do **not** exist; no `drain` / `decommiss` / `desired_state` / `desired-state` token
  anywhere under `crates/` or `xtask/`; `core/placement.rs` tracks per-`DServerId`
  used-bytes (`set_utilization`, `placement.rs:76-114`) but **no per-failure-domain
  aggregation or emission**; `reconcile_step` dispatches only `gc` + `scrub`
  (`reconciliation.rs:55-70`), with reconstruction (slice 6, #144) and rebalance
  (slice 7) explicitly "not yet dispatched". No prior or in-flight rebalance /
  drain-decommission work. Net-new slice 7.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. A draft PR MAY be opened for CI; the PR MUST NOT be
marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Design and approach are sound; the gap is test coverage of two reachable branches the current fixtures never construct. Rebuild must add both tests (be thorough): 1. Multi-fragment evacuation (`evac.len() > 1`): wider topology with >=2 servers marked draining that hold fragments of the SAME chunk, plus enough spare distinct domains to re-place each. Current tests use RS(2,1) one-fragment-per-server and drain a single server, so `evac` is always length 0 or 1. This test also closes the C5 claim that `select_distinct_domains_excluding` preserves `n` distinct domains for the multi-fragment case. 2. Lost-CAS `EvacOutcome::Conflict` / `emit_conflict`: inject a concurrent inode mutation between `plan_evacuations` (read) and `evacuate_chunk` (commit) so the `.require(prior)` precondition fails and the commit returns `CommitOutcome::Conflict`. This is NOT a fixture-size issue — it needs a concurrency seam (e.g. a MemMeta wrapper that bumps the inode version once before the commit). This branch is the slice's headline safety claim ("a racing writer loses rather than corrupts the record") and is currently asserted only in prose. Not defects, carried forward as context for the next reviewer: - C2 pre-fix red IS already demonstrated (build-notes: temporary `draining.clear()` flips `drains_a_d_server...` to left:Satisfied,right:Changed) — the reviewer flagged it only because build-notes were withheld. - V (fitness) is an accepted Option-A scope boundary: in-process demonstration, operator API/CLI deferred to ADR-0013 per proposal 0005 sequencing. - #144 ordering gate: build-notes claim #144 is merged on the worktree base (5fb905c) supplying `select_distinct_domains_excluding` / `Topology::domain_of` / `gc::orphan_key`; confirm it is on `main` before the next sign-off.
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
