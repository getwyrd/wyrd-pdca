# Brief — issue 554 / deployed-custodian-runs-gc

> The Plan artifact (docs 02 §PLAN). Do reads ONLY this file.

- **Slug:** deployed-custodian-runs-gc
- **Defect:** The deployable custodian role never runs GC. Both `reconcile_pass` calls in
  the production run loop `run_reconstruction_until` pass `None` where the `GcContext`
  goes (`crates/server/src/custodian.rs:442` and `:456`); the only `GcContext` the server
  crate constructs (`custodian.rs:350`) belongs to the #551 post-restore pass, which
  **marks, never deletes** ("GC does the reclaiming … later", `custodian.rs:338`). GC is
  the ONLY thing that reclaims fragment bytes — a delete/overwrite orphans fragments into
  the ledger for GC to reap (`metadata::unlink`, `crates/core/src/metadata.rs:369-408`;
  overwrite: `crates/core/src/write.rs:266-284`), and expired pending leases are likewise GC's input
  (`crates/custodian/src/gc.rs:109-146`) — so on a real cluster every delete, overwrite,
  completed reconstruction, and rebalance leaks its displaced bytes forever. The gc.rs
  module doc acknowledges deployment was deferred ("standing up the host that drives the
  loop against live stores is a later slice", `gc.rs:61-63`), and
  `crates/server/src/cli.rs:80-81` names #554 as the issue that makes GC actually run.
- **Success criterion:** The deployed custodian role reclaims collectable garbage: driven
  over a store carrying a deleted object whose grace window has elapsed, the role's run
  loop (the SAME production wiring `wyrd custodian` drives — `cli::cmd_custodian` →
  `run_reconstruction_until` → `reconcile_pass`) physically removes the orphaned
  fragments from the D-servers, while an object still inside its grace window (or still
  live) loses nothing. Demonstrated by the new deployed-role test below: red on base
  (bytes remain forever), green with the fix, under C4-verify.
- **Falsifiability:** RED is producible in-process on the base toolchain: the day-one
  suite (`crates/server/tests/custodian_day_one.rs`) already drives the REAL deployed
  role over in-memory metadata + trait-store fleets with a logical clock — no Docker, no
  live cluster. On `origin/main`, put→delete→advance-clock-past-grace→run the role: the
  fragments are still present on every store (GC never ran) — the new test's
  bytes-reclaimed assertion fails. Plain test binary, no cfg gate; C4-verify's red leg
  reverts `crates/server/src/custodian.rs` (+ any cli.rs change) and keeps the added
  test file.
- **Invariant to restore:** Every reconciliation responsibility of the durability plane
  actually RUNS in the deployed role — a loop that is correct only under `cargo test` is
  not deployed (the gap class #455 already exposed for scrub, fixed at
  `custodian.rs:421-456`; gc.rs:61-63 documents GC as the remaining one). Specifically:
  orphaned/expired garbage is reclaimed after a reader-safe grace window that is
  **derived from reader version-hold / lease semantics, never a magic constant**
  (proposal 0005:585-586; `GcContext::grace_window_millis` doc, `gc.rs:57-71`; the
  shipped derivation precedent `RESTORE_GRACE_WINDOW_MILLIS = LEASE_TTL_MILLIS`,
  `cli.rs:70-83`).
- **Repo + branch target:** getwyrd/wyrd @ main
- **Surfaces:** data
- **Difficulty:** high
- **Scope:** one logical change: construct a `GcContext` over the metadata store + the
  assembled fleet in the deployed role's run loop and hand it to a reconcile pass
  (`Some(&gc)`).
  GRACE WINDOW — be honest about what can and cannot be derived: the checkout has NO
  reader version-hold / maximum-read-duration mechanism today, so NO derivation can
  currently PROVE a grace value reader-safe (do not claim one does). What Do ships is
  (a) the MECHANISM — the deployed role honours `grace_window_millis` relative to the
  recorded evidence (`orphaned_at` / lease expiry): never reclaims before it elapses,
  reclaims after; (b) a deployed VALUE derived from the one timescale the system already
  trusts — the pending-lease TTL, exactly as the shipped restore-pass precedent does
  (`RESTORE_GRACE_WINDOW_MILLIS = LEASE_TTL_MILLIS`, `cli.rs:68-83`) — with a doc-comment
  stating this is a floor, not a proven reader-safety bound; and (c) a PRE-DECLARED
  sign-off item: the deployed grace VALUE is the maintainer's call (proposal 0005:585-586
  calls the exact value "a measurement question"), flag it in build-notes so it lands in
  SUMMARY §6 rather than passing as settled. The shipped test pins the mechanism
  (no-reclaim-before / reclaim-after), NOT reader safety.
  PASS PLACEMENT — prefer a DISTINCT fenced `reconcile_pass` for GC rather than folding
  it into the scrub or reconstruction pass: `reconcile_step` runs GC FIRST within a pass
  (`crates/custodian/src/reconciliation.rs:77-105`), and the run loop classifies a
  non-fenced fault in the scrub pass as scrub degradation and continues
  (`custodian.rs:441-451`) — folding GC in means a GC store fault suppresses scrub for
  the interval and is mislabelled (the same fault-isolation rule that split scrub from
  reconstruction, Codex #461). Whatever placement Do chooses, GC must not race the repair
  loop's placement rewrites within an interval — say why in build-notes.
  FLEET VIEW — reclaiming over the REACHABLE fleet only (the run loop's
  `live_reconstruction_view` drops unreachable servers) is safe for bytes: marks are
  idempotent and persist, and a skipped server's garbage is reaped on a later pass — but
  the pass MUST preserve orphan records for servers it skipped (never treat "skipped" as
  "collected") and must not report a partial reachable-fleet pass as fleet-wide
  convergence.
  / out of scope: #551's post-restore evidence pass (merged, `dc503cd`); any change to
  the GC library loop itself (`gc::reconcile` is tested and correct); the
  delete/orphaning producers; building a reader version-hold mechanism (a separate work
  item — this bundle must not invent one); an operator-facing manual-cleanup command;
  multi-zone concerns.
- **Repro instruction:** On `origin/main`:
  `grep -rn "GcContext {" ../wyrd/crates/server/src/` → one hit, `custodian.rs:350`, the
  restore-marking pass only; `sed -n '440,460p' ../wyrd/crates/server/src/custodian.rs`
  shows both run-loop `reconcile_pass` calls passing `None` for gc. Behaviourally: put an
  object through the production write path, delete it via `metadata::unlink` (orphan
  marks written atomically, `metadata.rs:369-408`), advance the clock past any grace
  window, drive
  `run_reconstruction_until` — every fragment byte is still on the stores.
- **External dependencies:** none (the deployed-role test runs in-process over trait
  stores + a logical clock, as `custodian_day_one.rs` does; no Docker, no live backend)
- **Test file:** crates/server/tests/custodian_gc.rs   (NEW file — the C4-verify gate
  earns its red only from an added `*/tests/*.rs`; also its own test binary for the same
  tracing-subscriber isolation reason `custodian_day_one.rs` documents. SHAPE THE RED
  HONESTLY: prefer wiring GC WITHOUT changing the deployed entry's signature
  (`run_reconstruction_until` — derive the grace window inside the role, as
  `RESTORE_GRACE_WINDOW_MILLIS` already does in cli.rs) so the test compiles on the
  reverted base and fails by ASSERTION (bytes still present). If a signature change is
  genuinely required, the test's red leg degrades to a compile error — say so in
  build-notes rather than let the gate's red pass silently for the wrong reason.)
- **Citations expected:** Do must cite path:line on `main` for every change. Composition
  peers Do MAY open: `crates/server/tests/custodian_day_one.rs` (the deployed-role test
  harness to mirror — real role, in-memory fleet, logical clock, kill/populate helpers)
  and the restore pass's GcContext assembly (`crates/server/src/custodian.rs:339-360`)
  plus the grace-window derivation it reuses (`crates/server/src/cli.rs:68-83`) — the
  wiring shape this fix generalises into the run loop.
- **Prior-art check (triage cycles):** searched by file path — `git -C ../wyrd log` over
  `crates/server/src/custodian.rs` and `crates/custodian/src/gc.rs`: #551 (5e1e7af…,
  merged dc503cd) added the post-restore MARKING pass and explicitly left "make GC run"
  to #554 (`cli.rs:80-81`); e65cf69 (#450) deployed scrub+reconstruction but not GC. The
  run loop still passes `None` for gc on today's `main` (verified by Read). No competing
  open work found.
- **Disposition hint:** likely-fix

## Ordering / batch note

No `Depends on` / `Conflicts with`: this bundle edits `crates/server/src/custodian.rs`
(+ possibly `cli.rs`), disjoint from #430/#431 (`crates/core/src/read.rs`,
`crates/core/src/repair.rs`, custodian maintenance loops' call-sites) and #490
(`crates/core/src/metadata.rs`, `write.rs`). Relationship to #551 is complete: #551
supplies the evidence (marks); this makes the collector run.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected pending correction of the reviewer FAILs and the adversary refutations; the wiring shape and the red→green proof stand. - Fleet-view defect (C3/C5/T3): a partial reachable-fleet pass must never retire chunk-wide expired-pending evidence when servers were skipped — preserve the evidence so a skipped server's copy stays reclaimable on a later pass; add the T3 test (one server unreachable during GC, reachable later, its orphan/pending record survives). - Adversary 1 (CLI clock domain): the deployed collector must not reclaim in-flight CLI writes whose leases are stamped at logical time 0 and read as born-expired against the custodian's wall clock on a shared backend; also reconcile the now-falsified cli.rs:65-66 rationale comment. - Adversary 2: the expired-pending-lease input must get a grace window beyond the bare TTL so a slow (>30 s) gateway PUT is not collectable mid-flight the moment its unrenewed lease expires. - Adversary 3: fix the grace-window doc-comment — there are two trusted lease timescales (60 s CLI, 30 s gateway); keeping 60 s is fine but the "one timescale the system already trusts" justification is factually wrong as stated. - Adversary 4: pin both secondary obligations with tests — expired-pending reclaim exercised through run_reconstruction_until, and skipped-server evidence preservation — so a regression in either goes red. Note: if closing adversary 1/2 turns out to require a commit-time lease-liveness check (an architecture change the brief puts out of scope), stop and route back — that decision overlaps the #490 re-plan.
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected for the NEW adversary finding, not the iteration-1 themes: the whole-fleet GC gate is defeated at startup. `connect_fleet` silently omits any peer unreachable when the custodian starts (custodian.rs:197-204; cli.rs:876-884), so `configured` is already partial, `unreachable.is_empty()` passes, and the very first GC pass can retire chunk-wide `pending:` evidence for a fragment a still-down server holds — a permanent leak, the exact stranding the deferral exists to prevent. FIX: gate GC on live fleet == operator-CONFIGURED fleet (thread the operator endpoint count / a completeness flag into run_reconstruction_until, as the #551 restore pass does at cli.rs:906-918), plus a test that drives the loop with `configured` shorter than the operator fleet — the existing T3 test hand-assembles all four servers and cannot catch this. Also in the rebuild: - Re-run the full `cargo xtask ci` on a host permitting loopback sockets so the green gate is non-provisional (reviewer's independent rerun stopped only at list_delete_over_grpc with PermissionDenied). - Pin the exact grace boundary (now == orphaned_at + grace reclaims, gc.rs:136) in the tests — currently probed only at ±1 ms (adversary conformance nit). - State the pause-under-outage trade-off explicitly in the run-loop doc (any single unreachable/decommissioned-but-configured server pauses ALL reclamation indefinitely) so the maintainer decision is visible at next sign-off (§6 item 7). RESOLVED at this sign-off — do NOT rework: the lease-liveness hazard (adversaries 1 & 2) is accepted as document-and-ship. #490's lease-conditional commit (obligation d) fail-closes both the born-expired CLI lease and the slow-gateway-PUT scenarios (refused write, never a torn committed object); residual mid-pass race is tracked as #557. Do not build any of the out-of-scope lease mechanisms; carry a sequencing note into the PR description instead: do not run `wyrd custodian` against a shared write-taking backend before #490 merges. T4 contribution-overlap and the T5 60s grace value (floor, not proven bound) remain human calls — keep them pre-declared in build-notes so they land in §6 again.
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected on process evidence, not on the approach: the advisory reviewer produced no verdict (bundle has no independent review) and the gating C4 check failed (`cargo test --workspace --exclude wyrd-dst` exit 101), while build-notes claims a fully green `./engine/xtask.sh ci` on the builder host. Next pass: re-run the Check reviewer so a verdict exists, and diagnose the C4 exit-101 vs builder-green discrepancy (name the failing test; check gate-host environment, e.g. loopback availability) before reworking the patch. The fleet-completeness gate design (operator_fleet_size) itself was not rejected — do not discard it blind.
- Failing gate: C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) — xtask: `cargo test --workspace --exclude wyrd-dst` failed with exit status: 101
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 4 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected on two confirmed findings, not on the approach — the GC wiring, the operator_fleet_size whole-fleet gate, and the red→green evidence all stand; do not rework them. 1. Duplicate `--endpoints` reaches the deleting GC sweep unvalidated (adversary [impl], confirmed against the patch): the uniqueness refusals exist only in the `--reconcile-after-restore` block (cli.rs:940-967); the run-loop path this patch activates (cli.rs:1040-1073) performs none, and the whole-fleet gate counts the duplicate on both sides. Fix mechanically: hoist the two existing refusals so they also guard the run-loop path, and add a test that drives the loop with duplicated `--endpoints`/`--ids` and asserts refusal (the live-data-loss route is repair displacement between two ids naming the same box). 2. Stale grace rationale (reviewer C3 FAIL, an iteration-1 carry-forward obligation still unmet): the RESTORE_GRACE_WINDOW_MILLIS doc-comment (cli.rs:70-83) still claims "the one timescale the system already trusts" (there are two: 60 s CLI, 30 s gateway) and still defers the derivation to "#554's job" — false once this patch lands. Update that comment to match the correct two-timescale derivation the patch already added in custodian.rs. Carry the C4 environment evidence forward so it is not re-litigated: the gating C4-ci exit-101 is a gate-host environment fault (`/tmp` disk quota, os error 122), independently reproduced by the adversary at the target (full gate command exit 0, 129 binaries green) and by the sign-off host on issue 430's cycle. The pre-resolved maintainer calls (lease-liveness document-and-ship pending #490, 60 s grace floor, pause-under-outage) remain accepted — keep them pre-declared, do not rework.
- Failing gate: C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) — xtask: `cargo test --workspace --exclude wyrd-dst` failed with exit status: 101
- Full previous attempt preserved in `iteration-v4/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
