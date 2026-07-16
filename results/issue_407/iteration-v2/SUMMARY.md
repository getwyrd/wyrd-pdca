# Result — issue 407 / m4-metadata-nemesis-partition-skew-pause

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: the metadata Tier-1 scenario can be driven under a composable **nemesis** with
- Success criterion: the nemesis exposes three leg kinds (partition / clock-skew /
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope: the three-leg nemesis seam + its materialization oracles + the pure

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it.
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

Review of issue #407: add a reusable three-leg metadata nemesis (partition, clock skew, and process pause) with materialization/heal oracles and a runnable Tier-1 entry point.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance boundary distinguishes default-compiled orchestration/oracle evidence from the deferred privileged campaign, so the decision is testable without conflating code-read with a live fault result (`xtask/src/main.rs:35`). |
| C2 Reproduction (red pre-fix) | PASS | In an isolated target copy with both added tests retained and production changes reversed, both named tests exited 101 on unresolved `nemesis` imports; this establishes a non-vacuous pre-fix red at `xtask/tests/nemesis_orchestration.rs:18` and `crates/metadata-fault-conformance/tests/nemesis_oracles.rs:20`. |
| C3 Change | FAIL | The no-leaked-fault invariant is not met when a workload panics and healing fails: the original panic resumes before `heal_result` and heal completeness are checked, so a paused/cut/skewed cluster can escape cleanup (`crates/metadata-fault-conformance/src/nemesis.rs:340`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Accept the named red→green evidence (pre-fix exit 101; post-fix 4/4 and 10/10 pass), but decide whether the unreproduced full CI assertion is sufficient—this host could launch Cargo tests yet `cargo xtask ci` could not spawn its nested `cargo`, so the reported whole-gate green remains provisional (`xtask/src/main.rs:1616`). |
| C5 Causal adequacy | FAIL | The human must require cleanup failure to be surfaced or otherwise made fail-safe before accepting the claimed invariant, because calling `heal` is not causally adequate when its error/completeness result is bypassed on panic (`crates/metadata-fault-conformance/src/nemesis.rs:337`). |
| T1 Structure | PASS | The ownership boundary is coherent for downstream reuse: lifecycle and live implementations reside in the conformance crate, while xtask owns only campaign dispatch (`crates/metadata-fault-conformance/src/lib.rs:65`; `xtask/src/lib.rs:20`). |
| T2 Shape | PASS | The public typed evidence and lifecycle seam make all three fault outcomes representable and keep the workload between materialization and healing (`crates/metadata-fault-conformance/src/nemesis.rs:106`; `crates/metadata-fault-conformance/src/nemesis.rs:331`). |
| T3 Runtime | NEEDS-HUMAN | Witness `WYRD_TIER1=1 cargo xtask metadata-nemesis` on the privileged ≥3-process FDB topology and confirm all three legs materialize and heal; this host lacks the libfaketime shared object, so runtime confidence currently rests on pure tests and code-read rather than the real topology (`deploy/fdb-multi-replica/docker-compose.faketime.yml:47`). |
| T4 Contribution | NEEDS-HUMAN | Decide whether local merged-history-by-affected-path plus the brief's tracker assertion is enough to rule out duplicate closed/rejected work; new affected paths have no merged history locally, but closed/rejected PR state could not be mechanically queried, which matters for additive-scope justification (`xtask/src/nemesis.rs:1`). |
| T5 Judgment | FAIL | Do not sign off the lifecycle safety claim until the panic-plus-heal-failure case is defined and tested, because the current test proves only that `heal` was called, not that leaked fault state was prevented (`crates/metadata-fault-conformance/tests/nemesis_oracles.rs:385`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether a witnessed real-cluster campaign demonstrates Jepsen/Elle fitness for #408—the automated checks validate orchestration and arithmetic, but cannot establish that the production-like topology exhibits the intended partition/skew/pause behavior (`xtask/src/fdb_faults.rs:362`). |

### Advisory — adversary

# check-advisory-adversary.md — issue #407, iteration 2 (adversarial pass)

Evidence re-run at `$PDCA_TARGET`: both named Check tests are **green** (`cargo test -p
wyrd-metadata-fault-conformance --test nemesis_oracles` → 10/10; `cargo test -p xtask --test
nemesis_orchestration` → 4/4, after `cargo clean -p` — the first runs failed with `E0432
unresolved import ...::nemesis` from a **stale build cache**, which incidentally exhibits the
exact red the verify gate claims once the production modules are reverted, so the red→green
mechanism for these two files is genuine, not a tautology). Findings, strongest first:

- NEEDS-HUMAN [impl] — **The clock-skew leg deterministically fails on every live run: the probe
  container ID goes stale the moment `apply()` recreates the node.** The runner resolves the skew
  container ONCE, before any leg, as a container **ID** (`container_of` = `docker compose ps -q`,
  `xtask/src/fdb_faults.rs:81-106`, called at `xtask/src/fdb_faults.rs:389`, exported at
  `xtask/src/fdb_faults.rs:454`). The leg's `apply()` then runs `docker compose up -d
  --force-recreate fdb2` (`crates/metadata-fault-conformance/src/nemesis.rs:781,817`), which
  destroys that container and creates one with a NEW ID — after which every probe `docker exec
  <old-id> date +%s` (`nemesis.rs:768`) fails, `wait_execable` times out at 90s
  (`nemesis.rs:818,791-792`), `apply` errors, and the leg can never materialize. Concrete failing
  case: `WYRD_TIER1=1 cargo xtask metadata-nemesis` → skew leg fails with "target container never
  became exec-able" on **every** run. This falsifies the in-tree claim that the single runner
  resolution makes disagreement "structurally impossible"
  (`crates/metadata-fdb/tests/tier1_metadata_nemesis.rs:122-127`, `nemesis.rs:751-753`) and
  repeats the exact defect class (live skew leg cannot materialize on the default run) that got
  iteration 1 rejected as carry-forward item 2. Fix direction: probe by stable compose container
  *name* (or re-resolve `compose ps -q` after each recreate) instead of a pre-recreate ID.

- NEEDS-HUMAN [impl] — **Cross-leg staleness: the skew leg's recreates poison the netns map for
  the process-pause leg that runs after it.** `netns_map` is also resolved once, pre-campaign
  (`xtask/src/fdb_faults.rs:388`), and the campaign order is partition → clock-skew → pause
  (`xtask/src/nemesis.rs:80-86`). Both the skew `apply` and its `heal` force-recreate `fdb2`
  (`nemesis.rs:781,831` — heal runs even when apply fails, via `drive_leg`'s heal-on-failure), so
  by the pause leg `fdb2`'s mapped container ID is dead. If the post-restart master lands on
  `fdb2`, `resolve_role_holder` hands back the stale ID
  (`crates/metadata-fdb/tests/support/mod.rs:81-82`) and `docker pause <stale-id>`
  (`nemesis.rs:684`) fails — a probabilistic live failure of a leg whose logic is otherwise
  correct. Same root cause and same fix as the previous bullet.

- NEEDS-HUMAN — **The third added test file earns no red, and its live body is compiled by no
  gate anywhere in this cycle — the verify PASS covers only two of the three added tests.**
  Verified at target: `cargo test -p wyrd-metadata-fdb --test tier1_metadata_nemesis` under
  default features is green with `0 passed; 3 ignored` regardless of whether the fix is present
  (its imports of the new modules live only under `#[cfg(feature = "fdb")]`,
  `crates/metadata-fdb/tests/tier1_metadata_nemesis.rs:107,133,138`), so the brief's
  falsifiability sentence "the gate loops every added `*/tests/*.rs`, whose imports/assertions
  against the new modules then fail to compile/pass" is unwarranted for this file. I attempted to
  type-check the fdb-feature body (`cargo check -p wyrd-metadata-fdb --features fdb --test
  tier1_metadata_nemesis`) and CANNOT: `/usr/include/foundationdb/fdb.options` is absent —
  toolchain unavailable, NOT scored as a refutation (issue #236); verdict on that body is
  provisional. A manual API cross-check found no mismatch (`support::{processes,
  resolve_role_holder, survivor}` at `crates/metadata-fdb/tests/support/mod.rs:26,75,100`;
  `FdbMetadataStore::open`/`with_prefix` at `crates/metadata-fdb/src/lib.rs:1260,1275`;
  `WriteBatch` builder at `crates/traits/src/lib.rs:659-690`), but the brief's declared posture —
  "'compiled by ci' is claimed ONLY for the default-compiled surface" — means the maintainer's
  witnessed `WYRD_TIER1=1` run is the FIRST compile of this file's live body, and per the two
  [impl] findings above that run would fail today. The sign-off open question is not currently
  satisfiable.

- NEEDS-HUMAN [impl] — **`drive_leg` silently drops a heal failure when the workload panics,
  contradicting its own leak-free claim.** On the panic path, `resume_unwind` at
  `crates/metadata-fault-conformance/src/nemesis.rs:343` executes BEFORE `heal_result?` and the
  `heal_is_complete` check (`nemesis.rs:346-353`), so a failed `docker unpause`/recreate during a
  panicking workload leaks a paused container / skewed node with only the workload panic
  reported — while the module doc claims "no leg may leave a cut cluster, a paused container, or
  a skewed clock behind" (`nemesis.rs:50-51`). The guard test
  (`crates/metadata-fault-conformance/tests/nemesis_oracles.rs:383-401`) asserts only
  `heal_count >= 1`, so it cannot catch this. Concrete case: #408's checked workload panics on a
  violation AND the unpause errors → the leaked-fault error is unreported. Low severity inside
  `xtask metadata-nemesis` (the runner tears the whole stack down,
  `xtask/src/fdb_faults.rs:400-403`), but #408 imports `drive_leg` directly with no such
  teardown. Fix: log/append the heal failure before `resume_unwind`.

- Nit (no adjudication needed): `parse_tests_run`'s shape check is self-defeating —
  `tail.starts_with("test")` (`xtask/src/nemesis.rs:135`) subsumes the other two arms and accepts
  e.g. `running 3 testimonials`; it also takes the FIRST matching line of interleaved
  stdout+stderr. Harmless today (one test binary per leg), but the guard is weaker than its
  comment claims.

Refutations attempted and FAILED (signal, not filler): the oracle arithmetic boundary cases
(zero floor, |offset| == floor, crash-vs-partition `target_running_during`, single-probe pause)
are all pinned by discriminating assertions; the mock-`drive_leg` tests genuinely flip red if
the inconclusive bail (`nemesis.rs:320-329`) or the `heal_is_complete` check
(`nemesis.rs:347-353`) is deleted (they assert error text + workload-ran flags, not just
`is_err`); `PartitionLeg::heal` returns previously-removed rules too (`nemesis.rs:563-579`), so
the `applied ⊆ healed` completeness check holds across the double-heal path; the orchestration
tests assert independent expectations, not the returned literals. The Check-core half of this
patch withstands attack; the live-leg half does not.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Accept the named red→green evidence (pre-fix exit 101; post-fix 4/4 and 10/10 pass), but decide whether the unreproduced full CI assertion is sufficient—this host could launch Cargo tests yet `cargo xtask ci` could not spawn its nested `cargo`, so the reported whole-gate green remains provisional (`xtask/src/main.rs:1616`).
- [ ] T3 Runtime — Witness `WYRD_TIER1=1 cargo xtask metadata-nemesis` on the privileged ≥3-process FDB topology and confirm all three legs materialize and heal; this host lacks the libfaketime shared object, so runtime confidence currently rests on pure tests and code-read rather than the real topology (`deploy/fdb-multi-replica/docker-compose.faketime.yml:47`).
- [ ] T4 Contribution — Decide whether local merged-history-by-affected-path plus the brief's tracker assertion is enough to rule out duplicate closed/rejected work; new affected paths have no merged history locally, but closed/rejected PR state could not be mechanically queried, which matters for additive-scope justification (`xtask/src/nemesis.rs:1`).
- [ ] Validation — fitness-to-purpose — Decide whether a witnessed real-cluster campaign demonstrates Jepsen/Elle fitness for #408—the automated checks validate orchestration and arithmetic, but cannot establish that the production-like topology exhibits the intended partition/skew/pause behavior (`xtask/src/fdb_faults.rs:362`).
- [ ] **The clock-skew leg deterministically fails on every live run: the probe
- [ ] **Cross-leg staleness: the skew leg's recreates poison the netns map for
- [ ] **The third added test file earns no red, and its live body is compiled by no
- [ ] **`drive_leg` silently drops a heal failure when the workload panics,

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Rejected on the advisory review (C3/C5/T5 FAIL) and the adversarial pass. The Check-core half (oracle arithmetic, lifecycle seam, orchestration, red→green for the two named tests) withstood attack and should be preserved; the live-leg half is defective. Fix in the next Do attempt: 1. Clock-skew leg fails deterministically on every live run: the runner resolves the skew target as a container ID once, pre-campaign (`container_of` via `docker compose ps -q`, xtask/src/fdb_faults.rs:81-106, used at :389, exported at :454), but the leg's `apply()` force-recreates fdb2 (nemesis.rs:781,817), invalidating that ID — every `docker exec <old-id>` probe then fails and `wait_execable` times out (nemesis.rs:768,791-792,818). Probe by stable compose container NAME, or re-resolve `compose ps -q` after each recreate. This repeats iteration 1's carry-forward item 2 defect class (live skew leg cannot materialize on the default run) — do not resolve container identity before a recreate again. 2. Same root cause cross-leg: `netns_map` is resolved once pre-campaign (fdb_faults.rs:388) and the skew leg's apply AND heal both recreate fdb2 (nemesis.rs:781,831), so the later pause leg can receive a stale container ID from `resolve_role_holder` (metadata-fdb tests/support/mod.rs:81-82) and `docker pause <stale-id>` fails. Same fix: name-based or post-recreate re-resolution for ALL legs. 3. `drive_leg` must not drop a heal failure when the workload panics: `resume_unwind` (nemesis.rs:343) runs BEFORE `heal_result?` and the `heal_is_complete` check (nemesis.rs:346-353), so a failed unpause/recreate during a panicking workload leaks fault state silently — contradicting the module's own no-leaked-fault claim (nemesis.rs:50-51). Surface/record the heal failure before resuming the panic, and strengthen the guard test (nemesis_oracles.rs:383-401 currently asserts only heal_count >= 1) to pin the panic-plus-heal-failure case. 4. The fdb-feature Tier-1 wiring (crates/metadata-fdb/tests/tier1_metadata_nemesis.rs) earns no red and is compiled by no gate this cycle; its "structurally impossible" disagreement claim (:122-127) is falsified by item 1. After fixing 1-3, the brief's sign-off open question (a witnessed WYRD_TIER1=1 run of all three legs materializing and healing) must be satisfiable — today it would fail on the skew leg every time. Minor (fix opportunistically): `parse_tests_run`'s `tail.starts_with("test")` arm (xtask/src/nemesis.rs:135) subsumes the others and accepts non-test lines; tighten or align the comment.
- By / date: Eduard Ralph / 2026-07-15

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
