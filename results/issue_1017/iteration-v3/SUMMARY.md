# Result — issue 1017 / multipart-reaper-terminal-exits

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: After 625.1, two states still have no exit when no client verb arrives.
  - **`Completing`:** a gateway that died holding the fence leaves the session `Completing`
    forever. It is never rolled back at `W_completing`, and when over-age it is never moved to
    `Aborting`.
  - **`Completed`:** #659's drain retains the tombstone, because its deployed tombstone input
    says "retain" until #625 supplies the window. The record and its `mpuctl` slot are
    therefore held forever.

  Two more gaps fall to this slice:
  - The grace window `G_orphan` is not yet bound to the strict inequality 0016 assigns to #625.
  - `B_ops` rests on a per-operation latency "assumed, not measured" (`SLOWEST_OP_MILLIS`,
    `crates/core/src/multipart.rs:5082-5105`), whose replacement the source assigns to #625.
- Success criterion: a NEW test file `crates/server/tests/multipart_reaper_exits.rs`
  drives the reaper through 625.1's one-shot as a subprocess (the same mechanics; the seeding
  helpers are **copied** into this file, not shared, per `run-verify.sh:141-144`). Every leg
  asserts on durable store state.
  **(D) `Completing` is exited, both ways.**
  - A `Completing` session whose `fenced_at_millis` is older than `W_completing`, and which is
    **not** over-age, rolls back to `Open@E+1` and installs `retire:records:{seg:<g>:<E>}` for
    that attempt's segments.
  - A `Completing` session that **is** over-age (`now - created_at > W_session`) goes
    **straight** to `Aborting@E+1`, in one batch that also installs that obligation. It is
    never rolled back first (`0016:1918-1930`).
  - Assert that the named epoch's `seg:` range drains empty and that a **later** epoch's
    segments are untouched.

  **(E-terminal) The clock guard on the terminal states.**
  - A foreign-clocked `Completing` session whose `fenced_at_millis` is far past `W_completing`
    is **not** rolled back: its state, epoch, `seg:` range and `retire:` records are all
    unchanged.
  - A foreign-clocked `Completed` session keeps its record, because its tombstone window
    cannot be judged. Assert that the record **survives**. Its only exit is #633's
    terminal-expiry verb.

  **(G) Tombstone expiry.** A local-clock `Completed` session whose `completed_at_millis` is
  older than `W_tombstone`, with no session `retire:` obligation left and an empty `sidx:`
  range, has its record deleted and `mpuctl.count` decremented by exactly one. A `Completed`
  session inside `W_tombstone` keeps its record, and an identical Complete retry still answers
  `AlreadyCompleted` (`multipart.rs:4789-4795`).

  **(H-terminal)** With a mismatched `mpuctl.profile`, a stale `Completing` session is not
  rolled back either (625.1's check guards every arm).

  **(F) Teardown over the composed reaper path is idempotent and waits for GC's decision.**
  The `orphan:` mark guard (arms (0), (a), (b), (c), and fail-closed on undecodable values) is
  **#659 child-1's** code and tested there. This slice does not implement, copy or re-test it
  arm by arm. It asserts only what the reaper's path adds on top:
  - Run the one-shot twice over a partially drained obligation of a session the reaper
    fenced. It converges to the same terminal state with **no double-decrement**.
  - **The reaper's teardown waits for the obligation.** In an `Aborting` session's obligation,
    one position carries a `reclaiming` mark. After the one-shot, the `mpu:` record is
    present, the count is unchanged, the mark is byte-identical, and the obligation is still
    there. The test then deletes that key, as GC's blind delete would (`gc.rs:804-824`), and
    runs the one-shot again. The position gets a fresh mark under `require_absent`, the
    obligation and the session are gone, and the count drops by exactly one.

  **`G_orphan` boundary** (`cargo xtask ci` evidence, appended to the existing
  `crates/custodian/tests/gc.rs`): pin the tick at which a mark written by a fence becomes
  reclaimable, and show that `G_orphan > W_write + δ_clock` holds **strictly**. GC's grace check
  is inclusive (`gc.rs:172-176`).
- Repo + branch target: getwyrd/wyrd @ main
- Scope: 
  - the `Completing` rollback at `W_completing` and the over-age `Completing → Aborting` edge,
    both installing `retire:records:{seg:<g>:<E>}`, through the existing fenced transitions;
  - the clock guard applied to `W_completing` and `W_tombstone`;
  - the `W_tombstone` judgment, which replaces "retain" as #659's tombstone input via
    `ReaperPolicy`;
  - the knob values `W_completing`, `W_tombstone` and **`G_orphan`**: each inside 0016's range
    and cited at its definition, with `G_orphan > W_write + δ_clock` and, where a repoint can
    intervene, `G_orphan > W_repoint + W_write + δ_clock` (`0016:1478-1479`, `:2497-2509`);
  - the calibration experiment;
  - docs in `08-crosscutting-concepts.md`.

  **Not this slice's to choose:** `W_ref`, `B_bytes`, `B_ops`, the profile tuple and
  `MAX_SESSIONS` are #655's single value set, consumed unchanged.

  **Out of scope:**
  - the `Open` arms (625.1) and the deployed wiring (625.2);
  - the operator abort and terminal-expiry verbs and the foreign-clock alarm (#633);
  - the `orphan:` mark guard, including arm (0) (`reclaiming`), and its DST rows (#659
    child-1), and X56 (#660);
  - #806, #630, FU-2 and FU-4;
  - ADRs, specs, and any edit to 0016;
  - any change to the records or state machine of the slices below.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (8 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage not measured — patch.diff does not apply on origin/main
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 115 mutants tested in 8m: 1 missed, 90 caught, 24 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 13 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1017/review
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 21.76s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Changes requested for #1017’s multipart reaper terminal exits: verify stale Completing recovery, Completed tombstone expiry, orphan grace, and batch-budget calibration.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Durable-state acceptance criteria and the later scope exceptions are explicit; the two-minute tombstone value is already decided (brief.md:29, brief.md:230, brief.md:242, brief.md:245). |
| C2 Reproduction (red pre-fix) | PASS | Independently stashing production changes while retaining the new test executes eight tests: six fail, including both Completing exits and expired tombstones (reviewer-red.log:295; crates/server/tests/multipart_reaper_exits.rs:576). |
| C3 Change | PASS | The changes address the specified exits and the subsequently authorized publication-stamp correction; server edits match the granted exceptions (crates/custodian/src/reaper.rs:1123; crates/core/src/multipart/complete.rs:1653; crates/server/src/custodian.rs:120; brief.md:245). |
| C4 Verification (red→green) | PASS | The eight-test suite passes independently with the fix, including after restoration in the pinned-toolchain CI run; original full CI passed, while the independent full run has only the synthetic-gitlink caveat detailed below (reviewer-green.log:276; reviewer-ci.log:3255; gate-logs/C4-ci.log:5105). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Bind segment verification to the store receiving the flip — the new public API can commit a root whose five segments exist only in another store, independently reproduced (crates/core/src/staged_publication.rs:510; reviewer-probe.log:54). |
| T1 Structure | PASS | Production remains behind existing metadata traits; backend calibration imports core only through dev-dependencies, with one shared experiment (crates/metadata-fdb/Cargo.toml:46; crates/metadata-tikv/Cargo.toml:39; crates/metadata-conformance/src/lib.rs:1623). |
| T2 Shape | PASS | The new exits use the existing fence/drain decomposition, and the deployed grace reads the tested constant; living architecture documents cover the changed lifecycle (crates/custodian/src/reaper.rs:549; crates/server/src/custodian.rs:120; docs/design/architecture/08-crosscutting-concepts.md:114). |
| T3 Runtime | NEEDS-HUMAN | Confirm the live calibration evidence and its applicability to the intended topology — FDB is unavailable and TiKV unconfigured here; the supplied logs prove compilation, not the quoted timing bounds on networked deployments (reviewer-calibration-environment.log:1; crates/core/src/multipart.rs:5977; brief.md:169). |
| T4 Contribution | NEEDS-HUMAN | Supply the affected-path merged-history and closed/rejected-work check — this snapshot contains only a synthetic base commit and no remote, so prior-art disposition cannot be mechanically settled (reviewer-prior-art.log:1; brief.md:213). |
| T5 Judgment | NEEDS-HUMAN [impl] | Repair the race fixture’s publication clock and assert retained retry evidence after the winning flip — its frozen fence-time callback allows a just-published tombstone to disappear without any further clock advance (crates/dst/tests/custodian.rs:8242; crates/dst/tests/custodian.rs:8781; reviewer-dst-probe.log:258). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Confirm that the intended retry/capacity contract accepts stamping immediately before the final commit, whose elapsed time consumes part of the two-minute window — the brief asks for a full window after publication, while the implementation documents this residual interval (brief.md:242; crates/custodian/src/reaper.rs:269). |

The requested reaper behavior reproduces red→green, but two implementation concerns remain. The independent full-CI limitation is harness metadata, and deployment calibration and prior art still need human evidence. This is an advisory review, not an acceptance gate. Source citations resolve under `$PDCA_TARGET`; brief and evidence-log citations resolve in this report’s directory.

1. **C5: the new ready-to-flip value loses the identity of the verified store.** `publish_segments` verifies and writes store A, but `ReadyFlip` retains only the publication and prepared plan; `flip` accepts an unrelated store B (`crates/core/src/staged_publication.rs:471`, `:495`, `:510`). In an independent probe, A held five segments, B held only the matching fence, and `ready.flip(&B, ...)` returned `Committed`. B then held a segmented root and zero segment records (`reviewer-probe.log:54`). The current Complete caller supplies the same store, so this demonstrates an unsafe public API invariant, not corruption observed through that caller. Retain the originating store in the token, or keep both phases inside an API that cannot switch stores. Add a regression preventing this cross-store publication.

2. **T5: the new clock seam is not faithfully exercised by the rollback race.** The helper supplies `&|| at`, where `at` is the fence timestamp. The race’s reaper instead reads `t0 + W_completing + jitter` under the same `wall` identity (`crates/dst/tests/custodian.rs:8242`, `:8713`). A scratch copy of the existing seeded test, with one additional post-drain retention assertion, failed at seed 1: publication stamp `86402593201`, reaper clock `86404459571`, tombstone absent after three passes with no clock advancement (`reviewer-dst-probe.log:258`). Keep the stale fence timestamp, but read publication time from the same advancing clock used by the reaper, and assert the winning flip’s tombstone and identical retry survive inside the window. The separate slow-Complete regression passes (`reviewer-ci.log:1960`); this finding concerns the race fixture, not that regression or the already-corrected crash behavior.

The two independent probes are preserved under `pdca-reviewer-1017-scratch/`. They copy test fixtures and exercise unchanged target code. From this report’s directory, reproduce them with:

```sh
TMPDIR="$PWD/pdca-reviewer-1017-scratch" CARGO_TARGET_DIR="$PWD/pdca-reviewer-1017-scratch/probe-build" cargo +1.96.0 test --offline --manifest-path pdca-reviewer-1017-scratch/probe/Cargo.toml --test publication reviewer_ready_flip -- --nocapture
TMPDIR="$PWD/pdca-reviewer-1017-scratch" CARGO_TARGET_DIR="$PWD/pdca-reviewer-1017-scratch/probe-build" RUSTFLAGS='--cfg madsim' MADSIM_TEST_SEED=1 MADSIM_TEST_NUM=1 cargo +1.96.0 test --offline --manifest-path pdca-reviewer-1017-scratch/dst-probe/Cargo.toml --test custodian reaper_completing_rollback_race_both_interleavings -- --nocapture
```

The first probe passes by demonstrating the invalid committed root; the second fails its added retention assertion.

The gate evidence supports the following, without treating missing instance wrappers as defects:

| Gate | Adjudication |
|------|--------------|
| C4-ci | Frozen log: PASS, including docs rendering, scanners and DST (`gate-logs/C4-ci.log:5105`). Independent `cargo +1.96.0 xtask ci`: fmt, clippy, build, relevant tests, conformance and DST run successfully; overall exit 1 arises from the missing synthetic advisory-db gitlink, described below. |
| C4-verify | PASS independently: eight green, then two passed/six failed with tracked changes stashed, then eight green again after restoration (`reviewer-green.log:276`, `reviewer-red.log:295`, `reviewer-ci.log:3255`). |
| C4-diff-cov | Coverage not measured: the frozen wrapper could not apply the patch to its `origin/main` base (`gate-logs/C4-diff-cov.log:10`). This is a base-state caveat, not a demonstrated patch defect. The supplied patched target is readable and reverse-application checks cleanly. |
| C5-mutants | Frozen run: 115 tested, 90 caught, 24 unviable, one missed (`gate-logs/C5-mutants.log:14`). The survivor changes `+ deletes.len()` to subtraction at `crates/metadata-conformance/src/lib.rs:1651`; both calibration builders produce zero deletes, so this mutation is equivalent for their current inputs. No defect is inferred from that survivor. |
| T4-batch-review | Frozen result remains FAIL with 13 entries (`gate-logs/T4-batch-review.log:10`); the deduplicated dispositions below distinguish confirmed findings from policy questions and unsupported defect claims. |
| T4-contribution | N/A: contribution artifacts are deliberately not drafted at Check; the substantive audit must rerun at publish (`gate-logs/T4-contribution.log:10`). |
| host-tikv | PASS independently for both feature-enabled clippy commands, matching the frozen evidence (`reviewer-host-tikv.log:212`, `:402`; `gate-logs/host-tikv.log:214`). This compiles the calibration but does not measure it. |

**The independent CI errors are caused by the disposable snapshot’s missing gitlink.** The unchanged advisory-db index test reports an empty entry for `.github/advisory-db`; the advisory scanner rejects the same absent pin (`reviewer-ci.log:4007`, `:4025`). The real gate’s captured run passed. No source failure is attributed to the patch from these errors, and the pin was not fabricated to make this rerun green. Initial red/green used the available default Rust 1.96.1; the restored CI and TiKV runs used the repository’s pinned 1.96.0. The stash was popped, the stash list is empty, and `git apply --reverse --check patch.diff` confirms restoration. The F tests fail on the base before the reaper fences their Completing fixture; those failures do not establish a defect in #659’s reclaiming-mark guard (`crates/server/tests/multipart_reaper_exits.rs:999`, `:1086`).

**The 13 batch-review entries reduce to two implementation findings and the explicit human decisions above.** The three store-switch reports are C5; the three frozen-clock reports are T5. The two final-commit timestamp reports identify the Validation contract question: the unbounded pre-segment delay was removed, but a literal full window after visibility is not established by a timestamp taken before commit. The three calibration reports concern extrapolating the specified experiment to other operation mixes and deployments; the brief expressly prescribes the two shapes and retains budget valuation with #655 (`brief.md:154`, `:167`). They feed T3 rather than an unrequested redesign. The flat-race report alone does not establish missing segment-safety coverage: the existing seeded segmented-publication race checks both winners and a mid-phase recovery (`crates/dst/tests/segmented_publication.rs:333`), and the new crash row checks segment ownership across rollback boundaries (`crates/dst/tests/custodian.rs:9091`). Finally, the unknown-flip recovery report requires a recovery read after tombstone expiry; `NoSuchUpload` after that expiry is an explicitly documented outcome even when the object exists (`crates/custodian/src/reaper.rs:280`). It is not independently promoted to an additional patch defect.

**Calibration and prior art remain evidence decisions, not inferred failures.** `fdbcli ... --exec 'status minimal'` reports the database unavailable; no TiKV endpoint is configured (`reviewer-calibration-environment.log:1`). The frozen logs contain no raw first-attempt FDB/TiKV timings. The prior sign-off says the calibration evidence holds (`brief.md:241`), so this review does not claim those measurements never happened; their raw evidence and deployment applicability are outside the supplied bundle. Confirm that evidence at sign-off, or run `cargo xtask fdb-conformance` and `cargo xtask tikv-conformance` on the equipped host and retain the first-attempt output as required by `brief.md:169`. The affected-path history command actually ran and returned only the harness’s synthetic base (`reviewer-prior-art.log:1`); closed/rejected history was not supplied. Watch the existing Tier-1/Tier-2 custodian durability campaigns after integration, as the standing rubric requests (`AGENTS.md:82`).

The C5 symptom-guard smell-test found no capability probe masking an eager/load-time cause. Clock-identity and missing-clock checks implement explicit fail-closed lifecycle semantics. No DCO finding or tracked-issue deferral is reopened.
<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — #1017 (slice 625.3), iteration 3

**Bottom line.** The main fix this round holds up. The publication stamp is now read just before the flip (`crates/core/src/multipart/complete.rs:1655`). I reverted it in a scratch copy, reading the stamp once before the fence as the old code did, and both new regression tests went red. I found no input that breaks the production logic. I did find two test gaps that would let real regressions merge green: one the builder can fix, one that needs a scope decision.

## Findings

- NEEDS-HUMAN [impl] — `crates/dst/tests/custodian.rs:8242`: the DST (deterministic simulation test) helper `complete()` stamps every publication with a frozen `&|| at`, which is the fence time `t0`. The reaper reads a separate `ManualClock` set to `stale = t0 + W_completing + draw` (`:8713`). So the race rows model a world where a tombstone (the `Completed` record kept so a retry can be answered) is already past `W_tombstone` when its flip lands. That is the exact defect this iteration fixed. I ran both consequences under `--cfg madsim`:
  1. **Leg (ii), flip-first (`:8770`–`:8830`).** The flip that wins the race publishes a tombstone, and the three reaps at `:8812` delete it at once. I added a probe assertion that the record survives those reaps. It failed with `stamp=86440527892 reaper_now=86444215601 age_ms=3687709 tombstone_kept=false`. The shipped row passes anyway, because it never checks that the record it just published survives.
  2. **Fix reverted.** With the stamp read once before the fence (the pre-fix behaviour, patched into `complete_upload` in scratch), all 9 `reaper_*` DST rows stay green, both race rows included. The seeded Tier-0 coverage therefore does not guard the defect this iteration fixed (rubric: *Test fidelity*; DST is the correctness authority, ADR-0009).
  - **Fix:** drive the publication stamp from the reaper's `ManualClock`. Advance that clock past `W_completing` after the fence commits, to model the stalled completer. Then in leg (ii), assert that the record is still `Completed` after the reaps and that an identical retry answers `AlreadyCompleted`. The leg then goes red on the reverted fix.

- NEEDS-HUMAN [human] — `crates/server/src/custodian.rs:638`: no test sees the deployed fleet loop's tombstone input (`tombstone: policy.tombstone()`) expire a tombstone. In scratch I changed it to `wyrd_custodian::Tombstone::Retain`, and the three server test files that exercise the reaper and the deployed loop all still passed: `custodian_gc` 12/12, `multipart_reaper` 13/13, `multipart_reaper_exits` 8/8.
  - Leg G goes through the one-shot. The one-shot reads `reaper.policy.tombstone()` through `reconcile_record_only` (`crates/custodian/src/retirement.rs:631-636`), not this line.
  - The only test of the deployed loop's tombstone handling (`crates/server/tests/custodian_gc.rs:1472`, `:1534`) asserts that the record *stays*.
  - So the slice's invariant, that in the deployed shape every `Completed` session has an exit, rests on a line nothing tests. The leg that would close this belongs in `custodian_gc.rs`, which is child-2's file (#1016). It is outside this slice's scope exception, which covers only the fixture timestamp.
  - **Decision needed:** widen the exception for one deployed-loop expiry leg, or have #1016 carry it.

## Evidence notes (no action needed)

- **The C4-verify row overstates.** `check-gates.json` says "8 test(s) ran red", but the red leg reads `2 passed; 6 failed` (`gate-logs/C4-verify.log:304`).
  - The two that pass on the base are `completing_session_fenced_moments_ago_is_kept_however_old_its_parts` (`crates/server/tests/multipart_reaper_exits.rs:734`) and `profile_mismatch_refuses_the_completing_arms` (`:934`). Both are vacuous on the base, as the brief predicts.
  - That gate also never covers this round's main fix. I checked it by hand: with the stamp read before the fence, `d_a_slow_completes_tombstone_lasts_the_whole_window_after_its_publication` (`crates/custodian/tests/multipart_session_terminal.rs:1389`) fails at `:1413`, and `the_publication_stamp_is_read_at_the_flip_behind_every_segment_batch` (`crates/core/tests/multipart_complete.rs:7195`) fails at `:7172` (`left: 2500`).
  - The fix is properly evidenced. Only the harness wording is off.
- **C5's one surviving mutant is equivalent.** It is at `crates/metadata-conformance/src/lib.rs:1651` (`+` → `-` before `built.deletes.len()`). Calibration batches carry no deletes, so the value is unchanged. This is not a test gap.

## Attempted, could not refute

- **Unknown-outcome flip whose tombstone the reaper deletes before the recovery read** (`complete.rs:1719-1724`, after which `moved` answers `NoSuchUpload` at `:1456-1457`, a T4 BUG). For this to happen, more than `W_tombstone` (2 min) plus a full drain must pass between the flip's commit and the `get` that immediately follows it. I found no bounded path that long, and 0016 accepts `NoSuchUpload` after the window (`0016:2863`).
- **`ReadyFlip::flip` accepting any store** (`crates/core/src/staged_publication.rs:508`, a T4 BUG). The only caller passes the same store it ran the segment phase on (`complete.rs:1670`). This is an API footgun, not a reachable failure.
- **Rebuilt flip refused after the segments landed** (`complete.rs:1655-1669`). `overwrite_modified` cannot fail when a stamp is present (`crates/core/src/metadata.rs:2409-2429`). The only thing left that could refuse it is the byte budget changing with the stamp's digit count, and millisecond stamps stay 13 digits until the year 2286.
- **`judge_completing`** (`crates/custodian/src/reaper.rs:1107`). I checked the following and found no failing case:
  - the ceiling is judged before the rollback;
  - both boundaries use strict `>` and have exact-tick unit tests;
  - a fresh fence is kept however old the parts are;
  - any completer write between the reaper's read and its fence turns into a `Conflict`;
  - `lost_any` classifies each required-absent key.
- **Tombstone clock identity** (`crates/custodian/src/retirement.rs:425`, `crates/custodian/src/reconciliation.rs:189`). The window is judged on the reaper's own clock and `clock_source` pair. Tests cover both directions and the no-reaper case, which fails closed as `Blocked`. No failing case found.
<!-- pdca:leaf-complete -->

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/core/src/multipart/complete.rs:1655`: The tombstone timestamp still precedes the successful flip commit. A delay after this read consumes the retry window before publication; if it exceeds two minutes, `crates/custodian/src/retirement.rs:432` treats the newly published tombstone as already expired and deletes it after draining, so an immediate retry can get `NoSuchUpload`. The claimed five-second transaction margin does not bound this interval on every backend (redb enters its blocking write transaction at `crates/metadata-redb/src/lib.rs:295`). The new regression delays a part read (`crates/custodian/tests/multipart_session_terminal.rs:1396`), leaving this case uncovered. Add a delayed **successful flip** regression and ensure commit latency cannot consume the promised post-publication retention window.

- NEEDS-HUMAN [impl] — `crates/core/src/staged_publication.rs:510`: The new public `ReadyFlip::flip` accepts a fresh store argument, although its segment verification happened on the store passed to `publish_segments`. Calling the two phases on different stores can commit a root in the second store when its root/fence preconditions match, even though that store has none of the verified segments: `flip` performs no segment read. Bind the verified store reference into `ReadyFlip` and remove this argument, or keep the split private behind one operation that owns the store throughout. This also simplifies the API and preserves the original single-store publication invariant.

- NEEDS-HUMAN [impl] — `crates/dst/tests/custodian.rs:8242`: The migrated Complete helper supplies `&|| at` as its publication clock. In the new rollback race, `at` is `t0`, while the reaper's `ManualClock` already reads more than `W_completing` later (`crates/dst/tests/custodian.rs:8713`, `crates/dst/tests/custodian.rs:8781`). Both claim the same `wall` identity, but a winning flip therefore publishes an already-expired tombstone. The subsequent passes check only that fragments remain unmarked (`crates/dst/tests/custodian.rs:8815`), so premature tombstone deletion passes. Keep the old fence timestamp, but read publication time from the same advancing clock as the reaper; assert the winning publication's stamp and that an immediate identical retry still succeeds after those passes.

No additional actionable reuse or efficiency findings. Review was static; validation evidence came from the supplied frozen gate logs, with no tests rerun or target files changed.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Bind segment verification to the store receiving the flip — the new public API can commit a root whose five segments exist only in another store, independently reproduced (crates/core/src/staged_publication.rs:510; reviewer-probe.log:54).
- [ ] T3 Runtime — Confirm the live calibration evidence and its applicability to the intended topology — FDB is unavailable and TiKV unconfigured here; the supplied logs prove compilation, not the quoted timing bounds on networked deployments (reviewer-calibration-environment.log:1; crates/core/src/multipart.rs:5977; brief.md:169).
- [ ] T4 Contribution — Supply the affected-path merged-history and closed/rejected-work check — this snapshot contains only a synthetic base commit and no remote, so prior-art disposition cannot be mechanically settled (reviewer-prior-art.log:1; brief.md:213).
- [ ] T5 Judgment — Repair the race fixture’s publication clock and assert retained retry evidence after the winning flip — its frozen fence-time callback allows a just-published tombstone to disappear without any further clock advance (crates/dst/tests/custodian.rs:8242; crates/dst/tests/custodian.rs:8781; reviewer-dst-probe.log:258).
- [ ] Validation — fitness-to-purpose — Confirm that the intended retry/capacity contract accepts stamping immediately before the final commit, whose elapsed time consumes part of the two-minute window — the brief asks for a full window after publication, while the implementation documents this residual interval (brief.md:242; crates/custodian/src/reaper.rs:269).
- [ ] `crates/dst/tests/custodian.rs:8242`: the DST (deterministic simulation test) helper `complete()` stamps every publication with a frozen `&|| at`, which is the fence time `t0`. The reaper reads a separate `ManualClock` set to `stale = t0 + W_completing + draw` (`:8713`). So the race rows model a world where a tombstone (the `Completed` record kept so a retry can be answered) is already past `W_tombstone` when its flip lands. That is the exact defect this iteration fixed. I ran both consequences under `--cfg madsim`:
- [ ] `crates/server/src/custodian.rs:638`: no test sees the deployed fleet loop's tombstone input (`tombstone: policy.tombstone()`) expire a tombstone. In scratch I changed it to `wyrd_custodian::Tombstone::Retain`, and the three server test files that exercise the reaper and the deployed loop all still passed: `custodian_gc` 12/12, `multipart_reaper` 13/13, `multipart_reaper_exits` 8/8.
- [ ] `crates/core/src/multipart/complete.rs:1655`: The tombstone timestamp still precedes the successful flip commit. A delay after this read consumes the retry window before publication; if it exceeds two minutes, `crates/custodian/src/retirement.rs:432` treats the newly published tombstone as already expired and deletes it after draining, so an immediate retry can get `NoSuchUpload`. The claimed five-second transaction margin does not bound this interval on every backend (redb enters its blocking write transaction at `crates/metadata-redb/src/lib.rs:295`). The new regression delays a part read (`crates/custodian/tests/multipart_session_terminal.rs:1396`), leaving this case uncovered. Add a delayed **successful flip** regression and ensure commit latency cannot consume the promised post-publication retention window.
- [ ] `crates/core/src/staged_publication.rs:510`: The new public `ReadyFlip::flip` accepts a fresh store argument, although its segment verification happened on the store passed to `publish_segments`. Calling the two phases on different stores can commit a root in the second store when its root/fence preconditions match, even though that store has none of the verified segments: `flip` performs no segment read. Bind the verified store reference into `ReadyFlip` and remove this argument, or keep the split private behind one operation that owns the store throughout. This also simplifies the API and preserves the original single-store publication invariant.
- [ ] `crates/dst/tests/custodian.rs:8242`: The migrated Complete helper supplies `&|| at` as its publication clock. In the new rollback race, `at` is `t0`, while the reaper's `ManualClock` already reads more than `W_completing` later (`crates/dst/tests/custodian.rs:8713`, `crates/dst/tests/custodian.rs:8781`). Both claim the same `wall` identity, but a winning flip therefore publishes an already-expired tombstone. The subsequent passes check only that fragments remain unmarked (`crates/dst/tests/custodian.rs:8815`), so premature tombstone deletion passes. Keep the old fence timestamp, but read publication time from the same advancing clock as the reaper; assert the winning publication's stamp and that an immediate identical retry still succeeds after those passes.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 13 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1017/review
- [ ] size backstop — this slice is behaving oversized: patch is 341 KB (threshold 100 KB); patch touches 30 files (threshold 20). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Plan
- Iteration delta (if iterating): Split the slice; it is two concerns, and the second keeps generating findings. Patch grew 151 -> 267 -> 349 KB / 30 files over three rounds. Production logic is ~550 code lines (+976 incl. comments) in 10 files; the rest is tests. Keep (sound across three rounds, adversary could not refute): the Completing exits (W_completing rollback to Open@E+1, over-age straight to Aborting@E+1, both with retire:records:{seg:<g>:<E>}), the clock guard on terminal states, the G_orphan strict boundary, and the calibration evidence. Suggested split for the re-plan (pdca split): - Slice A: Completing exits + clock guard + G_orphan boundary + calibration (reaper.rs / retirement.rs side). - Slice B: tombstone expiry (W_tombstone) together with the publication-stamp fix in the Complete path (core/src/multipart/complete.rs, staged_publication.rs — ~300 logic lines). This is where every round-3 finding lives. Findings Slice B must address: - ReadyFlip::flip (crates/core/src/staged_publication.rs:510) accepts a different store than the one publish_segments verified; bind the store into the token or keep both phases behind one store-owning call; add a cross-store regression. - DST race fixture (crates/dst/tests/custodian.rs:8242/:8713) stamps publication with a frozen fence-time clock; with the stamp fix reverted all 9 reaper_* rows stay green. Drive the stamp from the reaper's ManualClock and assert the winning flip's tombstone survives the reaps and an identical retry answers AlreadyCompleted. - Stamp is read before the final flip commit (complete.rs:1655); a slow commit eats the 2-minute window. Add a delayed-successful-flip regression. Open questions for Plan (not decided at sign-off): - Whether "stamp immediately before the final commit" is acceptable, or the window must start after the commit lands. - Who owns the deployed-loop tombstone-expiry test (crates/server/src/custodian.rs:638 is untested; swapping it to Retain passes all tests): this split or sibling #1016 (custodian_gc.rs). - Live FDB/TiKV first-attempt calibration output was not available to the reviewer; Plan should require it be retained (brief.md:169).
- By / date: Eduard Ralph / 2026-11-10

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
