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

Review task: implement ADR-0041 mutable-metadata consistency checker models, recorder, in-process workload, and checker serialization for issue 406.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief asks for net-new register/list-append/session models plus a non-vacuous in-process workload and explicitly defers the live Elle/JVM verdict off-Check; that scope is represented in the new model module and test surface at `/home/eddie/wyrd/wyrd.pdca-wt-l0/crates/testkit/src/consistency.rs:13` and `/home/eddie/wyrd/wyrd.pdca-wt-l0/crates/testkit/tests/consistency_models.rs:30`. |
| C2 Reproduction (red pre-fix) | NEEDS-HUMAN | I could not independently stash the fix to confirm red because the linked worktree git index is read-only from this sandbox (`/home/eddie/wyrd/wyrd/.git/worktrees/wyrd.pdca-wt-l0/index.lock`), so the human must decide whether the claimed `run-verify.sh` red pre-fix evidence is sufficient. |
| C3 Change | PASS | The patch adds the requested public checker substrate and exposes it from testkit; the decision is whether this new API is the intended reusable surface for later wire/fault slices, grounded at `/home/eddie/wyrd/wyrd.pdca-wt-l0/crates/testkit/src/lib.rs:18` and `/home/eddie/wyrd/wyrd.pdca-wt-l0/crates/testkit/src/consistency.rs:23`. |
| C4 Verification (red→green) | NEEDS-HUMAN | The narrow load-bearing test is green (`cargo test -p wyrd-testkit --test consistency_models`: 12 passed), but full `cargo xtask ci` failed on an unrelated loopback bind permission in `crates/chunkstore-grpc/tests/list_delete.rs:55` and I could not rerun stash-based red, so full red→green remains provisional. |
| C5 Causal adequacy | FAIL | The register model accepts a read whose only provenance is a failed write: failed writes are added to the value provenance set at `/home/eddie/wyrd/wyrd.pdca-wt-l0/crates/testkit/src/consistency.rs:309`, then unknown `(version,value)` observations are inserted as valid at `/home/eddie/wyrd/wyrd.pdca-wt-l0/crates/testkit/src/consistency.rs:379`; my probe returned `ACCEPTED`, so the human must decide how to close this torn-read/read-after-commit gap. |
| T1 Structure | PASS | The implementation keeps server/backend dependencies dev-only for the workload test, so the production testkit library surface stays limited to pure checker code at `/home/eddie/wyrd/wyrd.pdca-wt-l0/crates/testkit/Cargo.toml:27`. |
| T2 Shape | PASS | The history shape uses Elle-style invoke/terminal events with a monotone real-time proxy and model-specific op structs, matching the intended checker input contract at `/home/eddie/wyrd/wyrd.pdca-wt-l0/crates/testkit/src/consistency.rs:38`. |
| T3 Runtime | PASS | The in-process workload was exercised by the narrow test and asserts overlapping operations plus a real version bump before accepting the produced history at `/home/eddie/wyrd/wyrd.pdca-wt-l0/crates/testkit/tests/consistency_models.rs:497`. |
| T4 Contribution | PASS | Affected-file history shows prior consistency tests/oracles but no prior `crates/testkit/src/consistency.rs` or `crates/testkit/tests/consistency_models.rs`, so this is new reusable checker machinery rather than duplicate prior art. |
| T5 Judgment | NEEDS-HUMAN | The brief leaves an explicit sequencing choice for humans: proceed with the in-process-driven core now or hold issue 406 for the networked observable from issue 405; that affects whether this slice is acceptable as reusable machinery before the wire driver lands. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | The live recognized Elle/JVM verdict is intentionally off-Check and was not exercised here; the human must decide whether pure Rust model checks plus EDN serialization are fit for this slice’s externally-recognizable consistency-checker purpose. |

### Advisory — adversary

# Adversarial review — issue #406 (elle-register-listappend-models-and-workload-recorder)

Advisory / non-gating. Grounded on the target source at
`/home/eddie/wyrd/wyrd.pdca-wt-l0`. Toolchain available (cargo 1.96.0); I re-ran the
suite and built a standalone harness that links the **real** `wyrd-testkit` crate.

## Refutations found

- **NEEDS-HUMAN — the register model FALSE-ACCEPTS a read of a definitely-failed
  write's value at a version the commit point never produced.**
  `crates/testkit/src/consistency.rs:310-317` builds the value-provenance domain
  `written` from **every** write op, *without* the `c.ok` filter that Pass 1
  (`consistency.rs:324`) and the Pass-2 seed (`consistency.rs:345`) both apply. So a
  value that only a **`Fail`** write attempted — which the module's own doc
  (`Kind::Fail`, "definitely did not take effect") says never committed — is treated as
  legitimately readable. Concrete failing case, **empirically ACCEPTED by the real
  `check_register`**: `[write_ok "k"=10 @v1; write_fail "k"=99; read_ok "k"=99 @v2]`.
  Value 99 was never committed and version 2 was never produced, yet `check_register`
  returns `Ok`. The degenerate `[write_fail "k"=99; read_ok "k"=99 @v5]` is also
  accepted. This directly contradicts the brief's own TornRead definition ("a read
  returned a (value, version) the commit point never produced"). It is unreachable by
  *this* workload only because `put_with_retry` never records a register `write_fail`
  — but `HistoryRecorder::write_fail` is public API and the wire-driven (#405) /
  fault-injected (#407) histories this "reusable machinery" is built for **will**
  contain rejected writes, where the model will silently under-report torn reads.
  One-line fix: gate `written` on `c.ok`, as the sibling namespace model already does
  (`consistency.rs:767`, `if !c.ok { continue; }`).

- **NEEDS-HUMAN — the recorded C4-verify "red" may be the missing-symbol / compile-error
  red the brief explicitly disavows, not a model-weakening red on real inputs.**
  `check-gates.json` C4-verify asserts "red without the fix, green with it" (non-gating).
  The crafted-rejection tests are `expect_err` assertions; if "without the fix" reverts
  the whole patch, `tests/consistency_models.rs` fails to **compile** (undefined
  `check_register`, `RegOp`, …) — exactly the "red … by a missing symbol" the test
  docstring and brief §Falsifiability say does **not** count. Genuine flippability
  requires weakening a model to accept-all and watching the `expect_err`s go red.
  `run-verify.sh` lives in the driver, not the target, so I cannot confirm which red was
  captured. Human: confirm the recorded red was a model-weakening red, not a compile
  failure.

- **NEEDS-HUMAN — the brief/Scope claims the workload drives "create/delete/rename", but
  rename is entirely unexercised and cannot even be recorded.** `NsOp::rename_invoke/ok`
  (`consistency.rs:575,583`), the `NsF::Rename` remove+add logic in `check_list_append`,
  and the `[:rename …]` EDN branch exist, but `HistoryRecorder` exposes **no** rename
  method and **no** test constructs a rename op (0 occurrences in
  `crates/testkit/tests/consistency_models.rs`). The workload does create/delete/list
  only. The rename branch of the namespace model is dead relative to its coverage — it
  could be wrong with no failing test.

- **The workload does not actually exercise the session checks' reject paths — the
  "gives the session checks live teeth" comment overstates.** In
  `crates/testkit/tests/consistency_models.rs` `run_process`, contended HOT writes
  record `version = None` (`put_with_retry` returns `None` whenever the writer's own
  value was overwritten before `observe`), so `check_read_your_writes` never sets a floor
  for HOT (`consistency.rs:447-451` skips `None`). The only keys with observable
  own-writes are the uncontended `reg/p{p}` — single-writer, monotonically climbing
  versions — where RYW and monotonic-read violations are impossible by construction. So
  the produced history can never trip either session check; their reject logic is
  validated **only** by the hand-crafted histories. Consistent with the brief's declared
  posture, but the inline comment claims more than the code delivers.

- **Exactly-one-writer-wins is only partially modeled.** `consistency.rs:322-339` flags
  `TwoWinners` only when two committed writes report the same `(key, version)` with
  **distinct** values. Two winners with identical values — or a real double-commit that
  `put_with_retry` masks by recording `version = None` — are not surfaced. Not a
  refutation of the crafted test (which uses distinct values), but the guarantee-2
  "exactly-one-writer-wins" clause is weaker than advertised.

## Attempted refutations that did NOT hold up

- **Concurrency-test flakiness:** ran `workload_against_the_in_process_gateway_…` **40×**,
  0 failures. The barrier genuinely forces overlapping recorded invoke intervals, and
  `version >= 2` is robust (`commit_overwrite` bumps `prior.version + 1` per winning CAS,
  `crates/core/src/metadata.rs:471`; ≥2 successful HOT commits are guaranteed). Not flaky.
- **False-reject of the produced history:** traced `check_register` Pass 3
  (real-time-proxy version-regression) and `check_list_append` (lost-create /
  resurrected-delete) against the recorder's index protocol — ok-events are recorded
  *after* the store op commits, so "definitely before the list/observation" implies truly
  committed-before; no spurious rejection path found. The crafted version-regression,
  torn-read, two-winners, lost-create and resurrected-delete rejections are all genuine
  (each goes red if the model returns `Ok`), and the workload drives the real production
  read path (`wyrd_core::read::resolve`/`read_inode`) and real gateway commit point — not
  a parallel re-implementation.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C2 Reproduction (red pre-fix) — I could not independently stash the fix to confirm red because the linked worktree git index is read-only from this sandbox (`/home/eddie/wyrd/wyrd/.git/worktrees/wyrd.pdca-wt-l0/index.lock`), so the human must decide whether the claimed `run-verify.sh` red pre-fix evidence is sufficient.
- [ ] C4 Verification (red→green) — The narrow load-bearing test is green (`cargo test -p wyrd-testkit --test consistency_models`: 12 passed), but full `cargo xtask ci` failed on an unrelated loopback bind permission in `crates/chunkstore-grpc/tests/list_delete.rs:55` and I could not rerun stash-based red, so full red→green remains provisional.
- [ ] T5 Judgment — The brief leaves an explicit sequencing choice for humans: proceed with the in-process-driven core now or hold issue 406 for the networked observable from issue 405; that affects whether this slice is acceptable as reusable machinery before the wire driver lands.
- [ ] Validation — fitness-to-purpose — The live recognized Elle/JVM verdict is intentionally off-Check and was not exercised here; the human must decide whether pure Rust model checks plus EDN serialization are fit for this slice’s externally-recognizable consistency-checker purpose.
- [ ] external dependency** here. `verdict_dispatch` encodes that routing as a pure,

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
- Iteration delta (if iterating): Rejected — the register model FALSE-ACCEPTS a read of a value that only a definitely-FAILED write ever attempted (reviewer C5 FAIL + adversary, empirically ACCEPTED: [write_ok k=10@v1; write_fail k=99; read_ok k=99@v2] returns Ok; value 99 never committed, version 2 never produced). A consistency checker whose whole value is its correctness MUST detect this — if it does not, that is a fault to fix, not to accept. Fix: gate the value-provenance set on committed writes only (skip !c.ok), as the sibling namespace model already does; add a crafted history with a failed-write-value read as a flippable red so the regression is covered. Also address the adversary's improvement remarks in the rebuild: - rename branch of the namespace model is unexercised and unrecordable — HistoryRecorder exposes no rename method and no test constructs a rename op; either wire rename through the recorder with a crafted red, or drop the dead branch. - the produced workload never trips the session (read-your-writes / monotonic-read) reject paths: contended HOT writes record version=None, so only uncontended single-writer keys have observable own-writes and the reject logic is exercised only by crafted histories. Make the workload able to trip them, or scope the inline "live teeth" claim down. - exactly-one-writer-wins is only partially modeled: two winners with identical values, and a double-commit masked as version=None, are not surfaced. - confirm the recorded red->green is a model-WEAKENING red, not a compile-error red (reverting the whole patch fails to compile, which the brief says does not count). C4 full cargo xtask ci not cleared here — CI to be run locally after the correction lands.
- By / date: Eduard Ralph / 2026-07-07

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
