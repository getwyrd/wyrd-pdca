# Result — issue 406 / elle-register-listappend-models-and-workload-recorder

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: (gap / need) Wyrd has **no** externally-recognizable consistency-checker
- Success criterion: In `cargo xtask ci` (the unprivileged, container-free gate):
- Repo + branch target: getwyrd/wyrd @ feat/m4-production-metadata-backend
- Scope (one logical fix) / out of scope: Build the checker **models** (rw-register primary; list-append/set secondary),

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
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

Issue 406 builds the ADR-0041 mutable-metadata consistency-checker substrate: rw-register/list-append models, session checks, history recorder, serialization, and an in-process gateway workload.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The owed behavior is explicit: pure-Rust checker models plus a non-vacuous in-process gateway workload, with live Elle/JVM deferred off-Check; this is the acceptance target, not an inferred one (`brief.md:29`, `brief.md:101`). |
| C2 Reproduction (red pre-fix) | NEEDS-HUMAN | The decision owed is whether the red evidence was reproduced in the proper harness: I confirmed crafted model-weakening assertions exist (`crates/testkit/tests/consistency_models.rs:94`, `crates/testkit/tests/consistency_models.rs:183`) but no C2 gate is configured and `./engine/scripts/run-verify.sh` was not present here (`check-gates.json:15`, `check-gates.json:42`). |
| C3 Change | PASS | The patch reaches the specified surface by exporting a new consistency module and dev-only in-process gateway test deps, so the change is in the testkit/checker substrate rather than production gateway behavior (`crates/testkit/src/lib.rs:18`, `crates/testkit/Cargo.toml:27`). |
| C4 Verification (red→green) | NEEDS-HUMAN | The decision owed is full gate credibility: `cargo test -p wyrd-testkit --test consistency_models` passed 22/22, but `cargo xtask ci` could not complete in this sandbox because loopback bind is denied in an unrelated gRPC test (`crates/chunkstore-grpc/tests/list_delete.rs:53`), and the asserted red→green verifier script was unavailable (`check-gates.json:33`, `check-gates.json:42`). |
| C5 Causal adequacy | NEEDS-HUMAN | The decision owed is whether the hot-key `Mutex` used to capture exact commit versions is acceptable evidence or a curated fixture that serializes away the very write-write interleaving the workload is meant to make credible (`crates/testkit/tests/consistency_models.rs:543`, `crates/testkit/tests/consistency_models.rs:711`). |
| T1 Structure | PASS | The checker lives as a focused testkit module with gateway/runtime deps confined to `[dev-dependencies]`, preserving the production crate shape while making the CI test runnable (`crates/testkit/src/lib.rs:18`, `crates/testkit/Cargo.toml:19`). |
| T2 Shape | PASS | The history shape matches the briefed checker artifact: invoke/ok/fail events, committed-write provenance, namespace rename recording, and EDN serialization are all explicit model surfaces (`crates/testkit/src/consistency.rs:61`, `crates/testkit/src/consistency.rs:376`, `crates/testkit/src/consistency.rs:1033`, `crates/testkit/src/consistency.rs:1070`). |
| T3 Runtime | PASS | The exercised runtime path drives a real in-process `Gateway`, asserts model/session/list checks on the produced history, and checks overlap plus version bumps rather than only static serialization (`crates/testkit/tests/consistency_models.rs:695`, `crates/testkit/tests/consistency_models.rs:755`, `crates/testkit/tests/consistency_models.rs:768`). |
| T4 Contribution | NEEDS-HUMAN | The decision owed is prior-art closure: affected-path git history has no earlier `consistency.rs` / `consistency_models.rs` implementation, but closed/rejected issue state is not mechanically available in this artifact-only review; the brief’s no-duplicate claim remains a sign-off item (`brief.md:142`). |
| T5 Judgment | NEEDS-HUMAN | The decision owed is sequencing: the brief explicitly asks whether to proceed with the in-process core now or hold for #405’s networked observable, which changes the credibility bar humans accept for this slice (`brief.md:54`, `brief.md:67`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | The decision owed is final fitness: humans must accept that pure-Rust models plus the in-process workload are sufficient for this Check slice while the live Elle verdict, wire-observed run, and nemesis remain deferred (`brief.md:120`, `brief.md:126`). |

### Advisory — adversary

# Adversarial review — issue 406 / consistency-checker models + workload + recorder

Lens: refute the red→green and the reviewer's verdict; find the input that breaks the fix.
I compiled the **actual** `crates/testkit/src/consistency.rs` module verbatim (pure `std`)
and drove it with crafted histories to test the claims empirically.

## Findings

- **NEEDS-HUMAN — the register model still FALSE-ACCEPTS a genuine stale/torn read — the same
  false-accept class that sank iterations 1 and 2, only *partially* closed.**
  `crates/testkit/src/consistency.rs:381-385` builds value provenance **per-key**
  (`written: BTreeMap<key, Set<value>>`), and `:432-434` admits any read whose value appears
  *anywhere* in that per-key set; `:419-426` seed `version_value` only from committed writes,
  and `:444-458` reject a read only when its version *already maps to a different value*.
  So a read reporting a **superseded value at a version strictly higher than any committed
  write's version** slips through every pass. Concrete case, **empirically ACCEPTED**:
  `[w k=10@v1 (idx0-1); w k=20@v2 (idx2-3); r k=10@v3 (idx4-5)]` — the read begins (idx 4) in
  real time strictly after the overwrite to 20@v2 completed (idx 3), so a linearizable register
  must return 20 or newer; returning the superseded 10 is a stale read. It also violates the
  model's **own** `TornRead` contract at `:233-235` ("a read returned a `(value, version)` the
  commit point never produced") — no committed write ever produced `(10, v3)`. Pass 0
  (`:427`, `UnresolvedWrite`) closed only the `version=None` sub-case the iteration-2 sign-off
  named; Pass 3 (`:493-521`) catches a stale read only at the value's *real* (lower) version
  (verified: the identical read at `@v1` is correctly `VersionRegression`-rejected; at a phantom
  version *below* the newest committed version, also rejected). The uncovered window is exactly
  a **torn commit** — inode `version` bumped under the CAS while `size`/value is stale — which is
  the canonical metadata corruption this checker exists to catch, and which the workload's own
  `size == value` observe design (`crates/testkit/tests/consistency_models.rs` `observe`/`payload`)
  would surface as `read_ok(old_value, new_version)`. None of the shipped crafted reds exercise
  this shape (the torn-read test uses a never-written value → provenance; the regression test
  uses the value's real version → Pass 3). Brief Success criterion (a) — "the rw-register model
  **rejects** a hand-crafted **inconsistent** history (a stale/torn read …)" — is therefore not
  fully met, and the deferred-Elle "verdict over the SAME history" claim is undercut: a real
  rw-register checker would flag this history, so the Rust model is not a faithful pre-filter.

- **The produced-history leg (criterion (d)) cannot exercise the checker's teeth — it is a
  serialized log dressed as concurrent, so its pass is checker-correctness-blind.**
  `crates/testkit/tests/consistency_models.rs:635,648` call `versioned_put(…, Some(&hot_lock), …)`,
  and `versioned_put` (`:559`) holds `hot_lock` across the **entire** commit+observe of the
  shared HOT key, so every hot-key commit is fully serialized; per-process keys are single-writer.
  Consequence: the produced history contains **zero** CAS conflicts, **zero** `Fail` events,
  **zero** `version=None`, and no torn/stale/two-winner conditions — the `is_conflict` retry loop
  never fires. The barrier (`:634`) overlaps only the *invoke* records, which is enough to satisfy
  `max_register_concurrency >= 2` (`:770`) while the actual mutations run strictly serially. So
  "the register model then passes" (`:757`) is near-guaranteed regardless of the checker's
  correctness — precisely the structural objection the iteration-2 sign-off raised, now relocated:
  all detection teeth live **entirely** in crafted histories, and finding 1 shows a crafted
  stale/torn read the teeth miss. (This is by-brief-design for non-vacuity; flagged so the human
  weighs that criterion (d)'s green proves non-vacuity, not correctness.)

- **The session checks have no `UnresolvedWrite`-equivalent guard — a version-less own-write is
  silently skipped, not rejected.** `crates/testkit/src/consistency.rs:554-558`
  (`check_read_your_writes`) advances the RYW floor only `if let Some(ver) = op.version`, and
  `check_monotonic_reads` (`:585` onward) likewise skips version-less observations. Unlike
  `check_register`'s Pass 0, a committed own-write with `version=None` does not raise the floor,
  so `[w k=50@None; r k=50@v1]` is **ACCEPTED** (empirically confirmed) — a contended-session RYW
  history the register model would reject as `UnresolvedWrite`. Minor, because the produced
  workload records a version on every write; raised for crafted-history coverage parity with the
  register model (a skeptic's note, not a produced-path defect).

- **NEEDS-HUMAN — I cannot confirm the C4-verify "red" is a *model-weakening* red rather than a
  whole-patch *compile-error* red.** `check-gates.json` C4-verify asserts "red without the fix,
  green with it," but `run-verify.sh` / the engine are harness-side and absent from
  `$PDCA_TARGET`, so I cannot inspect what it reverted. The brief's iteration-1 carry-forward
  explicitly requires a model-weakening red (flip a guard, keep compiling), not the
  new-module-revert that merely fails to compile. The test file's flip comments (e.g.
  `consistency_models.rs`: "delete Pass 0 … `consistency.rs` still compiles") describe the correct
  shape, but I could not verify the gate actually exercised a weakening flip. (Toolchain-absent →
  provisional; not scored as a refutation.)

## Attempted but could not refute

- The **namespace** model (`check_list_append`): probed a name present-in-list backed only by a
  *failed* create (correctly `ResurrectedDelete`), a valid rename, and unrelated-remove masking of
  a lost create — all held.
- Two-winners counting (`consistency.rs:459-477`) correctly flags same-value and different-value
  double-commits; the four contended-path crafted reds (`UnresolvedWrite`/`LostWrite`) reject as
  claimed; the real-version stale read is caught by Pass 3.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C2 Reproduction (red pre-fix) — The decision owed is whether the red evidence was reproduced in the proper harness: I confirmed crafted model-weakening assertions exist (`crates/testkit/tests/consistency_models.rs:94`, `crates/testkit/tests/consistency_models.rs:183`) but no C2 gate is configured and `./engine/scripts/run-verify.sh` was not present here (`check-gates.json:15`, `check-gates.json:42`).
- [ ] C4 Verification (red→green) — The decision owed is full gate credibility: `cargo test -p wyrd-testkit --test consistency_models` passed 22/22, but `cargo xtask ci` could not complete in this sandbox because loopback bind is denied in an unrelated gRPC test (`crates/chunkstore-grpc/tests/list_delete.rs:53`), and the asserted red→green verifier script was unavailable (`check-gates.json:33`, `check-gates.json:42`).
- [ ] C5 Causal adequacy — The decision owed is whether the hot-key `Mutex` used to capture exact commit versions is acceptable evidence or a curated fixture that serializes away the very write-write interleaving the workload is meant to make credible (`crates/testkit/tests/consistency_models.rs:543`, `crates/testkit/tests/consistency_models.rs:711`).
- [ ] T4 Contribution — The decision owed is prior-art closure: affected-path git history has no earlier `consistency.rs` / `consistency_models.rs` implementation, but closed/rejected issue state is not mechanically available in this artifact-only review; the brief’s no-duplicate claim remains a sign-off item (`brief.md:142`).
- [ ] T5 Judgment — The decision owed is sequencing: the brief explicitly asks whether to proceed with the in-process core now or hold for #405’s networked observable, which changes the credibility bar humans accept for this slice (`brief.md:54`, `brief.md:67`).
- [ ] Validation — fitness-to-purpose — The decision owed is final fitness: humans must accept that pure-Rust models plus the in-process workload are sufficient for this Check slice while the live Elle verdict, wire-observed run, and nemesis remain deferred (`brief.md:120`, `brief.md:126`).

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Plan
- Iteration delta (if iterating): Third attempt; the same register false-accept class that sank iterations 1 and 2 recurs — the adversary compiled the shipped model and empirically accepted a stale/torn read [w k=10@v1; w k=20@v2; r k=10@v3] (superseded value at a version no committed write produced), so brief Success criterion (a) — "rw-register model rejects a hand-crafted inconsistent history" — is still not met. Two pointed iterate-do carry-forwards did not close it, so the problem is plan-level, not a rebuild detail: re-plan the approach rather than re-issuing the same brief. For the next Plan, reconsider (1) whether a hand-rolled pure-Rust register pre-filter is the right vehicle at all vs. leaning on the real Elle verdict for correctness and scoping the Rust slice to history-production/recording only; (2) the sequencing question — hold 406 for #405's networked observable so the workload isn't a hot-key-serialized in-process fixture that proves non-vacuity but not correctness; (3) make criterion (a) a first-class, model-weakening flippable red covering stale-read-at-phantom-version explicitly.
- By / date: Eduard Ralph / 2026-07-07

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
