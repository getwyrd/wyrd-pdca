# Result — issue 406 / consistency-workload-history-and-elle-serialization

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: (gap / need) ADR-0041 §Decision names three deliverables for #329's
- Success criterion: In `cargo xtask ci` (the pure-Rust, container-free, JVM-free gate):
- Repo + branch target: getwyrd/wyrd @ feat/m4-production-metadata-backend
- Scope (one logical fix) / out of scope: Build, on top of the landed #405 observable, (1) a **concurrent workload driver**

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

Review of issue 406: add the concurrent consistency workload substrate, Elle EDN serialization, session/local invariants, directory-as-set history, and off-Check verdict dispatch.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief gives a concrete net-new target: concurrent multi-process register history, EDN serialization, session checks, directory-as-set history, and off-Check verdict routing, with INV-1/INV-2 as the load-bearing decisions (`brief.md`). |
| C2 Reproduction (red pre-fix) | NEEDS-HUMAN | A true stash-and-rerun red check could not be completed because the target git index is read-only; non-mutating HEAD checks show `crates/server/src/consistency_workload.rs` and `crates/server/tests/consistency_workload.rs` are absent pre-fix, so the red evidence rests on absence rather than a full rerun. |
| C3 Change | FAIL | The human must decide whether the directory deliverable requires real wire recording: the patch only constructs `DirectoryHistory` from explicit records, while the test uses a crafted `dir_rec` fixture rather than PUT/DELETE/GET-probe capture, so the "recorded directory-as-set history" criterion appears unmet at `crates/server/src/consistency_workload.rs:562` and `crates/server/tests/consistency_workload.rs:99`. |
| C4 Verification (red→green) | NEEDS-HUMAN | Socket-free checks rerun green (`cargo test -p wyrd-server --test consistency_workload -- --skip concurrent_workload_records_a_nonvacuous_genuinely_concurrent_history`: 8 passed), but full runtime verification could not be reproduced because loopback bind is denied at `crates/server/tests/consistency_workload.rs:420`, and `cargo xtask ci` also fails on an unrelated loopback gRPC test. |
| C5 Causal adequacy | FAIL | The implementation covers INV-1/INV-2 for crafted register and serializer arms, but causal adequacy for the directory-as-set deliverable turns on whether a crafted `DirectoryHistory::from_records` fixture is enough without a recorder over the real prefix PUT/DELETE/GET-probe surface (`crates/server/src/consistency_workload.rs:522`). |
| T1 Structure | PASS | The new module is isolated and exported from the server crate without broad call-site churn (`crates/server/src/lib.rs:18`). |
| T2 Shape | PASS | Register history merge, process tagging, same-key read/write overlap filtering, EDN entries, and session invariants are expressed as direct data-shape APIs rather than hidden test-only behavior (`crates/server/src/consistency_workload.rs:90`, `crates/server/src/consistency_workload.rs:143`, `crates/server/src/consistency_workload.rs:279`). |
| T3 Runtime | NEEDS-HUMAN | The socket-free runtime path passed locally, but the wire-driven concurrent workload could not be observed in this sandbox because `TcpListener::bind("127.0.0.1:0")` is denied; run the full test in a loopback-enabled environment and confirm overlap assertions at `crates/server/tests/consistency_workload.rs:505`. |
| T4 Contribution | NEEDS-HUMAN | Merged-history prior-art by affected paths found no prior `consistency_workload` source/test in HEAD, but closed/rejected work is not mechanically available in this artifact-only sandbox, so human sign-off must clear duplicate/rejected-work risk. |
| T5 Judgment | NEEDS-HUMAN | The brief explicitly leaves the writer-supplied version-climb evidence as a human decision point; sign-off must decide whether that remains out of scope or must be strengthened to backend-observed GET versions (`crates/server/tests/consistency_workload.rs:524`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Fitness depends on the privileged off-Check Elle parser/verdict leg consuming the same serialized history; this Check run verifies stable EDN bytes but does not exercise the external Elle/JVM job (`crates/server/src/consistency_workload.rs:275`). |

### Advisory — adversary

# Adversarial review — issue 406 (consistency-workload-history-and-elle-serialization)

Advisory only; I never gate. I re-derived the red→green by reading the target at
`$PDCA_TARGET` and building the suite (`cargo test -p wyrd-server --test consistency_workload
--list` → 9 tests compile clean; the fix is green). I did not add a test to the read-only
target, so the failing cases below are grounded on the code path, not a fresh run.

## Refutations (concrete)

- **NEEDS-HUMAN — INV-1 leak in an un-audited arm: the RYW *read* arm fabricates an `Absent`
  observation from a determinate non-404 failed read** —
  `crates/server/src/consistency_workload.rs:226`–`231`. The `None` branch is commented "A
  determinate absent read (404)" and returns `false` (own-write-lost) whenever a standing
  `AtLeast(_)` obligation exists. But `version` is `None` for **every** non-200 GET, not only
  404: `ObservableS3Client::get` sets `version = Some(..)` iff `status == 200`, else `None`
  (`crates/server/src/consistency_observable.rs:181`–`185`). So `is_indeterminate` (only
  `0`/`≥500`, `:57`–`58`) lets a determinate **4xx-non-404** GET (403, 400, 409, 412, 416…)
  fall into the `None` branch and be treated as a *definite absence*. Concrete failing case:
  `[PUT k v=1 @200 ; GET k version=None @403]` ⇒ `session_read_your_writes()` returns
  **false** — a fabricated RYW violation from a read that observed *nothing* about the
  register. This is exactly the INV-1 fault-class ("never convert an outcome-unknown into a
  definite claim") re-appearing in an arm the crafted reds never probe (the read-arm test uses
  only 500/200/404, `tests/consistency_workload.rs:1088`–`1096`). It is also **internally
  self-contradictory**: the module's own serializer labels a 403 GET `:fail` — "no value
  observed" (`register_completion_type`, `:503`, `:508`) — while the session check treats the
  same op as a determinate absence. The brief's success-criterion (c) claim that the session
  checks are "sound surface-wide (INV-1)" is therefore unwarranted for this arm, and this is
  precisely the point-vs-class scoping failure the re-plan was meant to close.

- **NEEDS-HUMAN — the PUT/DELETE "indeterminate → clear obligation" reds are weaker than
  claimed; deleting the guard leaves them green** — `crates/server/src/consistency_workload.rs:195`–`197`
  (PUT) and `:205`–`207` (DELETE). The tests' comments assert "RED if the PUT/DELETE arm stops
  guarding `is_indeterminate`" (`tests/consistency_workload.rs:1061`–`1083`), but the crafted
  histories carry **no prior obligation**, so the `Obligation::Unknown` clear is never
  observable. Trace: dropping line 197 entirely (keeping only the `else if is_success` arm)
  leaves `put_arm` green — a 500 PUT fails `is_success` too, so the obligation stays unset and
  the later `GET v=1 @200` still passes. Same for `delete_arm`: dropping line 207 leaves the
  standing `AtLeast(1)` from the prior determinate PUT, and reading `v=1` satisfies it → still
  green. The reds only flip against the *specific* historical v5 shape (an **unconditional**
  `AtLeast`/`Absent` insert), not against a general weakening of the indeterminate guard. A red
  that survives deletion of the very line it claims to pin is the weak-red pattern that let v4/v5
  through. A discriminating input the suite is missing:
  `[PUT k v=5 @200 ; PUT k v=2 @500 ; GET k v=1 @200]` — accept requires the indeterminate PUT to
  clear `AtLeast(5)`; without line 197 it stays `AtLeast(5)` and the read is (correctly, under
  that variant) rejected. (Note the flip side: whether *clearing* a determinate `AtLeast(5)`
  on a lower indeterminate write is itself desirable — it can mask a real own-write-lost of v5
  — is a soundness-direction judgement for the human; it errs toward accept, so it is within
  the "Elle owns the verdict" design, but it is untested either way.)

## Attempted and could not refute

- **INV-2 witness (cross-key / read↔read negatives).** I tried to find a vacuous overlap that
  still counts: `read_write_overlapping_pairs_across_processes` (`:149`–`165`) conjoins
  distinct-process ∧ same-key ∧ read↔write ∧ span-overlap; the cross-key and read↔read crafted
  negatives (`tests/consistency_workload.rs:1186`–`1204`) genuinely flip if any conjunct is
  dropped. Could not refute. (Sole nit: `spans_overlap` uses `<=`, so endpoint-touching spans
  count as overlapping — immaterial to the negatives.)

- **Serializer golden bytes.** The expected EDN is a hand-written literal, not recomputed with
  the serializer's own `join`, and the sort key `(time, process, phase)` reproduces the golden
  ordering exactly (`:438`–`447`); an indeterminate PUT maps to `:info` and a 404 GET to a
  definite `:ok` of `nil`. Genuine byte-exact, flippable, on the production `to_elle_edn` path.
  Could not refute the serializer-stability claim (and the brief already scopes it as
  stability-only, not Elle-parser acceptance).

- **Verdict-dispatch.** `consistency_verdict_dispatch` is a pure two-arm function with both
  arms representable and the default routing off-Check (`:744`–`757`); the red flips
  behaviourally if the default is re-pointed. Could not refute.

- **Monotonic-read arms.** `reads_are_monotone` (`:376`–`398`) correctly *skips* a `None`-version
  read via the `let Some(v) = … else continue`, so the 403-GET fabrication above does **not**
  reach the monotonicity checks — only the RYW read arm has it. Could not refute the monotonic arms.

## Scope note on the evidence gate

- The load-bearing socket-free reds are real and flippable (per above, with the two caveats).
  Leg (a)'s **wire-driven** green (`concurrent_workload_records_a_nonvacuous_genuinely_concurrent_history`,
  `tests/consistency_workload.rs:1294`) is timing/loopback-dependent and pre-declared deferred
  in the brief; I did not run it and make no claim about it — it is not the load-bearing red.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C2 Reproduction (red pre-fix) — **CLEARED** (full re-Check on this box: `run-verify.sh` PASS — red without the fix, green with it; the read-only-index limit was a review-sandbox artifact). Original note: A true stash-and-rerun red check could not be completed because the target git index is read-only; non-mutating HEAD checks show `crates/server/src/consistency_workload.rs` and `crates/server/tests/consistency_workload.rs` are absent pre-fix, so the red evidence rests on absence rather than a full rerun.
- [x] C4 Verification (red→green) — **CLEARED** (on this box the full `--test consistency_workload` ran 9/9 green *including* the wire-driven concurrent test — loopback bind works here; `pdca gates 406` C4-ci "xtask ci: all checks passed" and C4-verify PASS). Original note: Socket-free checks rerun green (`cargo test -p wyrd-server --test consistency_workload -- --skip concurrent_workload_records_a_nonvacuous_genuinely_concurrent_history`: 8 passed), but full runtime verification could not be reproduced because loopback bind is denied at `crates/server/tests/consistency_workload.rs:420`, and `cargo xtask ci` also fails on an unrelated loopback gRPC test.
- [x] T3 Runtime — **CLEARED** (the wire-driven concurrent workload WAS observed on this box: `concurrent_workload_records_a_nonvacuous_genuinely_concurrent_history` green in the verify worktree, overlap assertions hold; the "bind denied" was the review sandbox only). Original note: The socket-free runtime path passed locally, but the wire-driven concurrent workload could not be observed in this sandbox because `TcpListener::bind("127.0.0.1:0")` is denied; run the full test in a loopback-enabled environment and confirm overlap assertions at `crates/server/tests/consistency_workload.rs:505`.
- [x] T4 Contribution — **CLEARED** (prior-art check run against GitHub: no closed, open, or in-flight PR touches `consistency_workload`, and the file does not exist on `feat/m4-production-metadata-backend` — no duplicate/rejected-work risk). Original note: Merged-history prior-art by affected paths found no prior `consistency_workload` source/test in HEAD, but closed/rejected work is not mechanically available in this artifact-only sandbox, so human sign-off must clear duplicate/rejected-work risk.
- [x] T5 Judgment — **CLEARED** (human: writer-supplied version-climb stays in scope; backend-observed GET versions are pre-declared a later slice in the brief — accepted as-is, not a blocker for this slice). Original note: The brief explicitly leaves the writer-supplied version-climb evidence as a human decision point; sign-off must decide whether that remains out of scope or must be strengthened to backend-observed GET versions (`crates/server/tests/consistency_workload.rs:524`).
- [x] Validation — fitness-to-purpose — **CLEARED** (human: the Elle/JVM verdict leg is off-Check by design, ADR-0016; this slice's job is the stable serialized EDN history, which the byte-exact golden tests verify — the external leg is tracked separately, not this slice's gate). Original note: Fitness depends on the privileged off-Check Elle parser/verdict leg consuming the same serialized history; this Check run verifies stable EDN bytes but does not exercise the external Elle/JVM job (`crates/server/src/consistency_workload.rs:275`).

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: merged-wider
- Iteration delta (if iterating):
- By / date: Eduard Ralph / 2026-07-08

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
