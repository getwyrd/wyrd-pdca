# Result — issue 348 / maintenance-loops-reject-malformed-placement

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: The durability-plane maintenance loops walk a committed chunk's placement
- Success criterion: For a committed chunk whose `placement` is non-empty and of the
- Repo + branch target: getwyrd/wyrd @ main   (Wyrd has no maintenance branches; INTEGRATION §2)
- Scope (one logical fix) / out of scope: Stop the maintenance loops silently fabricating identity placement for a

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): fail — ./engine/xtask.sh: line 30: exec: cargo: not found
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: fail — run-verify.sh: FAIL — the bundle's test is RED *with* the fix applied (not green).
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

# Check review — issue 348 / maintenance-loops-reject-malformed-placement (iteration 2)

**Task under review:** stop the four durability-maintenance loops (GC, scrub, reconstruction, rebalance) from silently identity-filling a malformed (non-empty, wrong-length) committed `placement` vector — classify once in core (`placement_is_valid` / `checked_fragments`), GC/scrub fail safe (fully referenced + audit), reconstruction/rebalance skip + NEEDS-HUMAN, read path unchanged (ADR-0040 decisions 3–4); iteration-2 rework scope: attribute the cluster-wide drain block to the blocking malformed chunk ids (T5(a) carry-forward).

**Grounding note:** reviewed against the target worktree `/home/eddie/wyrd/wyrd.pdca-wt` (patch applied there; source readable). Command execution on this host is approval-gated for the reviewer (even pre-built test binaries), so test *runs* could not be re-executed by me; compile success is evidenced by the fresh full build in the worktree (`target/debug/deps/{gc,scrub,reconstruction,rebalance,wyrd_custodian}-*` binaries + `libwyrd_core.rlib`/`libwyrd_custodian.rlib`, built 00:10–01:00 today). The gating C4-ci FAIL string is `exec: cargo: not found` (check-gates.json:37) — I independently confirmed `cargo` is absent from this host's default PATH while a working toolchain evidently exists (the worktree build) — a host-toolchain artifact, exactly as the iteration-1 carry-forward predicted (brief.md:92–94), not a patch defect.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Brief is complete and grounded: defect, per-loop success criterion, invariant, scope fences, repro, test files. The claimed invariant text exists verbatim in the target — ADR-0040 decisions 3–4 at `docs/design/adr/0040-mixed-era-placement-expansion.md:75-93` mandate exactly the per-loop behaviours the brief binds to; prerequisite #347 helper is present (`crates/core/src/metadata.rs:142`). |
| C2 Reproduction (red pre-fix) | PASS | The pre-fix defect is mechanically evident in the removed base code: `gc.rs:referenced_fragments` expanded every committed chunk via the liberal `chunk.fragments()` (patch.diff:312), reconstruction `assess` likewise (patch.diff:470), rebalance `plan_evacuations` likewise (patch.diff:393) — identity fabrication for a wrong-length vector by construction. Each new test documents a concrete flip that re-reddens it. Actual red re-run is execution-gated on this host — folded into the C4 human item. |
| C3 Change | PASS | Patch applies and compiles (applied in `$PDCA_TARGET`, full build artifacts 01:00). Single-source classifier `placement_is_valid`/`checked_fragments`/`MalformedPlacement` in `crates/core/src/metadata.rs:159-199`; all four loops route through it (`gc.rs:237`, `scrub.rs:74+95` via shared `ReferenceSet`, `reconstruction.rs:240`, `rebalance.rs:167`); read path untouched (`crates/core/src/read.rs:104` still `placed_dserver`, patch.diff never touches read.rs); scope fences respected (empty-vector fallback of #350 untouched; write path untouched). |
| C4 Verification (red→green) | NEEDS-HUMAN | Both gate FAILs are host-toolchain artifacts, not patch verdicts: C4-ci is literally `exec: cargo: not found` (check-gates.json:37; corroborated — no cargo on this host's PATH, yet the worktree holds a complete successful build from 01:00, so the gate shell's PATH is what is broken); C4-verify's "RED with fix" was attributed to the same broken toolchain by the iteration-1 sign-off (brief.md:92-94), which re-ran everything green-with-fix/red-with-revert. I could not re-execute (all execution approval-gated, including pre-built binaries). **Decision owed:** fix the gate host PATH (add `~/.cargo/bin`; zig-cc shim wrapper per brief.md:92) and re-run, or run manually in `/home/eddie/wyrd/wyrd.pdca-wt`: `PATH="$HOME/.cargo/bin:$PATH" cargo test -p wyrd-core -p wyrd-custodian` (or execute the pre-built `target/debug/deps/gc-a688967ce7719dc7`, `scrub-33adb4a87040eaa9`, `reconstruction-077bc2cb5711049e`, `rebalance-53c64dfc101bf5e8`, `wyrd_custodian-d0bbcb0caa96f9f7` directly), then revert `crates/{core/src,custodian/src}` (keep tests) and confirm the four #348 tests go red. Do NOT read the C4-ci FAIL as a patch blocker. |
| C5 Causal adequacy | PASS | The cause — unconditional liberal expansion *inside maintenance loops* — is removed at every maintenance call site: no `.fragments()` caller remains anywhere in `crates/custodian/src` (grep; only `metadata.rs` defines/tests it, and `read.rs:104` keeps the liberal primitive by design). Smell-test: the added classifier is not a capability probe papering over an eager cause — it is the ADR-mandated validity gate that *transforms* corrupt-data handling (reject-before-expand), per `0040:82-93`. The contested short-placement supersession (#349 cells re-classified) was ratified at prior sign-off as T5(b) (brief.md:92) and is preserved verbatim — not re-litigated. |
| T1 Structure | PASS | Exactly the brief's "classify once, in one place, reused by every loop": one classifier in core (`metadata.rs:159-199`), one shared `ReferenceSet {placed, malformed}` with a `protects()` gate (`gc.rs:202-219`) consumed by GC (`gc.rs:112`), scrub (`scrub.rs:74`), and drain status (`desired_state.rs:157`); reconstruction/rebalance classify per-chunk at their expansion points. Satisfies the brief's SELF-TEST (property over four loops + shared core classifier, brief.md:41-42). |
| T2 Shape | PASS | API deltas are contained: `referenced_fragments` now returns `ReferenceSet` — all three consumers updated; `ReconciliationStatus` gains `PendingMalformed{chunks}` and drops `Copy` for `Clone` (patch.diff:150-151) — no consumer outside `crates/custodian` exists (grep across crates; the `lib.rs:34` export is used only by custodian's own tests). `MalformedPlacement` carries expected/actual for the operator signal. Emitters mirror the existing per-loop `emit_*` pattern. |
| T3 Runtime | PASS | Static + compile evidence: `checked_fragments` returns `Result` (no new panic paths); all four emitters use the loops' existing audit seams (`wyrd.custodian.{gc,scrub,reconstruction,rebalance}.audit`, verified in target at gc.rs:316, scrub.rs:211, reconstruction.rs:528, rebalance.rs:349) plus monotonic counters; malformed warnings re-emit per reconcile pass — consistent with existing `emit_skip` churn. Live observation folded into the C4 run item. |
| T4 Contribution | PASS | Tests pin every binding condition: GC fail-safe (protects a real fragment the fabricated tail would abandon, `tests/gc.rs:605+`), scrub fail-safe (no phantom repair, `tests/scrub.rs` re-classified cell), reconstruction skip + obligation-stays-queued (`tests/reconstruction.rs` re-classified cell), rebalance skip + record-never-repointed + drain attribution `PendingMalformed{chunks:[CHUNK]}` (`tests/rebalance.rs:1325-1334`), classifier unit tests incl. the read-path-stays-liberal pin (`metadata.rs:357-404`). Prior-art check corroborated in-tree: `metadata.rs:140` doc-comment names `checked_fragments()`/`placement_is_valid()` as #348's deliberate handoff from #347/PR #361. |
| T5 Judgment | NEEDS-HUMAN | The iteration-1 rework mandate (attribution-only delta to `desired_state.rs`) is implemented as prescribed: the cluster-wide fail-safe block is KEPT — deliberately *not* scoped to servers the corrupt vector names (`desired_state.rs:167-176`) — and the stall is attributed via `PendingMalformed{chunks}` (sorted ids, `desired_state.rs:177-179`), pinned by test (`tests/rebalance.rs:1325-1334`). **Decision owed (the judgment the prior sign-off reserved):** (a) the sign-off asked for "richer status surface and/or an audit event" — the builder shipped the status-surface arm only, no desired-state audit event; confirm the "and/or" is satisfied. (b) Ratify the operational cost: ANY single malformed chunk blocks EVERY drain/decommission cluster-wide until a human resolves the corrupt record — deliberate per ADR-0040, but it trades operations availability for safety and only surfaces to callers of `reconciliation_status`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Human-only by design. **Decision owed:** does the fail-safe posture actually serve operators — a malformed chunk becomes fully-referenced-forever (GC never reclaims), its repair obligation queues forever, its REAL fragments are excluded from scrub entirely (bit rot on them goes undetected while awaiting resolution — inherent to the ratified fail-safe, worth naming), and all drains stall cluster-wide — with warn-level audit events + counters + `PendingMalformed` as the only resolution path (no repair tooling exists yet)? Verify the signal is loud enough in your observability stack to reach an operator before the stalled drains do. Runnable check after fixing the gate PATH: the four loop tests above, plus `tests/rebalance.rs:1325` for the attribution surface. |

## Notes for §6

- **C4:** gate FAILs are host artifacts (cargo not on gate PATH; zig-cc shim `--target` rejection per brief.md:92). Fix host, re-run `./engine/xtask.sh ci` + `run-verify.sh`, or use the manual commands in the C4 row. The patch itself compiles clean in the worktree.
- **T5:** ratify the status-surface-only attribution and the cluster-wide drain-block cost.
- **V:** fitness call on fail-safe-forever semantics + scrub blind spot on a malformed chunk's real fragments + audit-signal loudness.

### Advisory — codex

- No advisory findings. I did not identify a correctness bug or actionable reuse/simplification/efficiency cleanup in the patch.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C4 Verification (red→green) — Both gate FAILs are host-toolchain artifacts, not patch verdicts: C4-ci is literally `exec: cargo: not found` (check-gates.json:37; corroborated — no cargo on this host's PATH, yet the worktree holds a complete successful build from 01:00, so the gate shell's PATH is what is broken); C4-verify's "RED with fix" was attributed to the same broken toolchain by the iteration-1 sign-off (brief.md:92-94), which re-ran everything green-with-fix/red-with-revert. I could not re-execute (all execution approval-gated, including pre-built binaries). **Decision owed:** fix the gate host PATH (add `~/.cargo/bin`; zig-cc shim wrapper per brief.md:92) and re-run, or run manually in `/home/eddie/wyrd/wyrd.pdca-wt`: `PATH="$HOME/.cargo/bin:$PATH" cargo test -p wyrd-core -p wyrd-custodian` (or execute the pre-built `target/debug/deps/gc-a688967ce7719dc7`, `scrub-33adb4a87040eaa9`, `reconstruction-077bc2cb5711049e`, `rebalance-53c64dfc101bf5e8`, `wyrd_custodian-d0bbcb0caa96f9f7` directly), then revert `crates/{core/src,custodian/src}` (keep tests) and confirm the four #348 tests go red. Do NOT read the C4-ci FAIL as a patch blocker.
- [x] T5 Judgment — The iteration-1 rework mandate (attribution-only delta to `desired_state.rs`) is implemented as prescribed: the cluster-wide fail-safe block is KEPT — deliberately *not* scoped to servers the corrupt vector names (`desired_state.rs:167-176`) — and the stall is attributed via `PendingMalformed{chunks}` (sorted ids, `desired_state.rs:177-179`), pinned by test (`tests/rebalance.rs:1325-1334`). **Decision owed (the judgment the prior sign-off reserved):** (a) the sign-off asked for "richer status surface and/or an audit event" — the builder shipped the status-surface arm only, no desired-state audit event; confirm the "and/or" is satisfied. (b) Ratify the operational cost: ANY single malformed chunk blocks EVERY drain/decommission cluster-wide until a human resolves the corrupt record — deliberate per ADR-0040, but it trades operations availability for safety and only surfaces to callers of `reconciliation_status`.
- [x] Validation — fitness-to-purpose — Human-only by design. **Decision owed:** does the fail-safe posture actually serve operators — a malformed chunk becomes fully-referenced-forever (GC never reclaims), its repair obligation queues forever, its REAL fragments are excluded from scrub entirely (bit rot on them goes undetected while awaiting resolution — inherent to the ratified fail-safe, worth naming), and all drains stall cluster-wide — with warn-level audit events + counters + `PendingMalformed` as the only resolution path (no repair tooling exists yet)? Verify the signal is loud enough in your observability stack to reach an operator before the stalled drains do. Runnable check after fixing the gate PATH: the four loop tests above, plus `tests/rebalance.rs:1325` for the attribution surface.
- [x] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — ./engine/xtask.sh: line 30: exec: cargo: not found

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: merged-wider
- Iteration delta (if iterating):
- By / date: Eduard Ralph / 2026-07-02

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
