# Check review — issue 196 / tier2-kill-reconstruct-harness (iteration 2)

**Decorrelation / grounding note.** Artifact-only review from `{patch.diff, brief.md,
check-gates.json}`; `build-notes.md` withheld. `$PDCA_TARGET` is unreadable in this
sandbox (env reads are blocked) and the protocol forbids wandering into other checkouts
on the machine (a `/home/eddie/wyrd` tree exists but I did **not** read it). All citations
are therefore grounded on `patch.diff` line numbers. Per protocol I do **not** raise a
"cannot verify against target" condition as a blocking C4 FAIL — unverifiable target-side
semantics are routed to NEEDS-HUMAN, not treated as patch defects.

**Iteration-2 focus.** The carry-forward rejected iteration 1 on **T4**: (1) the
`assert_garbage_not_corruption` helper was logically inverted; (2) the three `assert_*`
helpers were orphaned in the `xtask` crate, unreachable from the real (different-crate)
scenario and merely duplicating its inline asserts; (3) a broken intra-doc link at
`faults.rs:149`. I re-derived each against this patch.

- **(1) Inversion fixed.** Helper now returns `Err` when `!committed_placement_has_victim`
  i.e. **passes** when the victim is still in the committed placement
  (`tier2_kill_reconstruct.rs` diff 345-352), and its unit test encodes the same direction
  (`(true,true).is_ok()`, diff 420-428; plus `(true,false)`→"hybrid" err, diff 443-456).
  Both directions covered → the green is **not** vacuous. Scenario call site is consistent:
  it first asserts `placement[VICTIM_INDEX] == VICTIM_INDEX` post-crash (diff 767-771) then
  feeds `committed_has_victim` to the helper (diff 782-786).
- **(2) Orphan/duplication resolved.** The three helpers were **moved into** the scenario
  test crate (`crates/chunkstore-grpc/tests/tier2_kill_reconstruct.rs`, diff 328-412) where
  they are both **called by the scenario** (diff 785, 840, 852) and **covered by
  non-`#[ignore]`d unit tests** (diff 420-531) that run in `cargo xtask ci`'s `cargo test
  --workspace`. `xtask/src/kill_reconstruct.rs` now retains only `select_victim_index` /
  `victim_container_name` (the genuinely-wired helpers, diff 1055-1064). No dead duplicate
  copy remains.
- **(3) Broken link removed.** `faults.rs` doc now links only `crate::kill_reconstruct::
  KR_DSERVER_COUNT` (resolvable; module added in `main.rs` diff 1098) and references the
  scenario file as plain text (diff 922-924). The old `crate::kill_reconstruct_test::…`
  dangling link is gone.

Core success-criterion check: the `WYRD_TIER2_CMD` external-command shell-out is **removed**
and replaced by real in-repo orchestration (`run_kill_reconstruct`, diff 928-976) that
stands up the compose cluster, kills a D server, and invokes a real API-bound scenario test
driving the production `wyrd_custodian::reconcile_step` path (imports diff 167-177). That is
criterion (a). Criterion (b) born-at-tier unit coverage runs at Check. Criterion (c) the
privileged run is gated behind `WYRD_TIER2=1` and a new off-Check workflow.

## Verdict table

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Brief's success criterion is concrete and testable; the diff targets it directly — real in-repo harness replacing `WYRD_TIER2_CMD` (`patch.diff` 936 removed → 928-976 added). |
| C2 Reproduction (red pre-fix) | PASS | No standalone C2 gate; "red" is criterion-absence + a *demonstrated* red (stub a helper → its unit test fails). C4-verify gate re-ran red→green (`check-gates.json` C4-verify: pass); helper unit tests cover both pass/fail directions (`patch.diff` 420-531), so the seam is load-bearing not vacuous. |
| C3 Change | PASS | One coherent change — build the Tier-2 harness: workflow, scenario test, `xtask` helper module + orchestration, Cargo deps. No unrelated edits; the `WYRD_TIER2_CMD` bypass is fully excised (`patch.diff` 936). |
| C4 Verification (red→green) | PASS | Deterministic gates re-ran green: `C4-ci` (`./engine/xtask.sh ci`) pass and `C4-verify` (red without fix, green with) pass (`check-gates.json` 33-49). I could not independently rebuild (target unreadable) — gates own the mechanical green; this is **not** a verification FAIL. |
| C5 Causal adequacy | PASS | Root cause = harness was never built (inert `WYRD_TIER2_CMD` dispatch). Fix **removes** the cause (deletes the shell-out, builds real harness) rather than guarding it. Symptom-guard smell-test does **not** fire: the `tool_available("docker")` / `WYRD_TIER2` opt-in and the `WYRD_DSERVER_ENDPOINTS` skip (`patch.diff` 928-944, 577-596) are the ADR-0016 deferred-tier gating sanctioned by the brief, not a capability probe papering over a load-time side effect. |
| T1 Structure | PASS | Files land where the brief/precedent dictate: scenario sibling to `tier2_integration.rs`, `xtask/src/kill_reconstruct.rs` module wired via `main.rs` `mod` decl (`patch.diff` 1098), workflow under `.github/workflows/`. |
| T2 Shape | PASS | Harness is real, API-bound Rust (calls `reconcile_step`, `ReconstructionContext`, `erasure::encode/reconstruct`), not an env-var string; helpers return `Result`-style with named violations; unit tests assert both success and failure paths. |
| T3 Runtime | PASS | Check-exercised runtime = the helper unit tests + `xtask` helper unit tests, green under `cargo xtask ci` (C4-ci pass). The `#[ignore]`d scenario *body* is deferred off-Check by design; its live runtime is the Validation row below. |
| T4 Contribution | PASS | The three iteration-1 rejection points are all addressed: inversion fixed with a matching non-vacuous unit test (`patch.diff` 345-352, 420-456), orphaned helpers re-homed into the scenario crate and wired + unit-tested (328-531, 785/840/852), broken intra-doc link removed (922-924). `select_victim_index`/`victim_container_name` remain correctly wired (1055-1064, 1072-1088). |
| T5 Judgment | NEEDS-HUMAN | Decision owed: does the chosen fidelity — **in-memory** `MemMeta`/`CrashMeta` metadata seam with only chunk fragments crossing real gRPC to containers (`patch.diff` 215-306, comment 210-213) — satisfy proposal 0005 §13.2's literal "real NVMe/fsync" Tier-2 mandate, or does Tier-2 require a real metadata store? The brief reinterprets toward the DST in-memory shape; that reinterpretation is the author's to ratify, and it materially bounds what the campaign actually proves durable. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decision owed: confirm the deferred green that Check cannot observe — that the privileged `WYRD_TIER2=1` job (`tier2-kill-reconstruct.yml`) actually runs the scenario GREEN on a real node with a real Docker daemon, since the scenario's load-bearing assumptions are unverified at Check: that `CrashMeta`'s "only the version-conditional repoint carries a positive precondition" (`patch.diff` 295-305) faithfully isolates the crash point in the *production* reconstruction loop, and that the selector re-places onto spare server 9 / domain J (`patch.diff` 682-695). Also confirm the base the gates ran against contains **merged #195** (declared `Depends on (merged): 195`, shared edits to `faults.rs`/`main.rs`) — a stale/trailing base here would be a target-state caveat, not a patch defect. (This is getwyrd/wyrd @ main, not a cross-version cherry-pick, so fork-discipline §3 pick-correctness is N/A.) |

## Notes for §6 (human must clear)

- **T5 fidelity:** in-memory metadata seam vs. "real NVMe/fsync" Tier-2 mandate — ratify the brief's reinterpretation or require a real metadata store.
- **Validation deferred green:** confirm the privileged `WYRD_TIER2=1` run is green on a real node, that `CrashMeta`'s single-positive-precondition crash model matches the production reconstruction commit sequence, and that the gate base contains merged #195.
- **Prior-art:** the brief documents a by-affected-file-path search (#146/PR#194 introduced the inert `run_kill_reconstruct`; overlap with #195 handled via scheduling fields). Confirmed as documented in the brief; the #195-merged precondition is folded into the Validation row above.
