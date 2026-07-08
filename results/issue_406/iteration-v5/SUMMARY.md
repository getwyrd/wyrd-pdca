# Result — issue 406 / consistency-workload-history-and-elle-serialization

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: (gap / need) ADR-0041 §Decision names three deliverables for #329's
- Success criterion: In `cargo xtask ci` (the pure-Rust, container-free, JVM-free gate):
- Repo + branch target: getwyrd/wyrd @ feat/m4-production-metadata-backend
- Scope (one logical fix) / out of scope: Build, on top of the merged #405 observable, (1) a **concurrent workload driver**

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

Reviewing issue 406: add the concurrent consistency workload, Elle EDN serializers, session checks, directory-as-set history, and off-Check verdict dispatch.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The target is net-new checker substrate, and the patch exposes a dedicated workload/serializer module rather than an in-gate linearizability verdict; the module is exported at `crates/server/src/lib.rs:18`. |
| C2 Reproduction (red pre-fix) | PASS | Reverse-applying `patch.diff` made `cargo test -p wyrd-server --test consistency_workload register_history_serializes_byte_exact_to_elle_edn` red with "no test target named `consistency_workload`", matching the net-new absence claimed by the brief. |
| C3 Change | PASS | The change covers the requested substrate pieces: process-tagged merged histories and read/write overlap witness at `crates/server/src/consistency_workload.rs:79`, EDN serialization at `crates/server/src/consistency_workload.rs:376`, session checks at `crates/server/src/consistency_workload.rs:461`, directory serialization at `crates/server/src/consistency_workload.rs:670`, and off-Check dispatch at `crates/server/src/consistency_workload.rs:787`. |
| C4 Verification (red→green) | NEEDS-HUMAN | Decide whether CI evidence discharges the wire leg: I confirmed reverse-patch red and 9 socket-free tests green, but this sandbox denies loopback bind at `crates/server/tests/consistency_workload.rs:411`, so the two wire-driven tests and full `cargo xtask ci` green remain provisional here. |
| C5 Causal adequacy | PASS | The prior false-accept risks are addressed at the cause visible in this slice: concurrency now requires read/write overlap at `crates/server/src/consistency_workload.rs:158`, indeterminate outcomes map to `:info` at `crates/server/src/consistency_workload.rs:341`, and delete RYW obligations are retained at `crates/server/src/consistency_workload.rs:485`. |
| T1 Structure | PASS | The implementation is contained in the server consistency substrate and reuses the #405 observable history via the narrow crafted-history constructor at `crates/server/src/consistency_observable.rs:88`. |
| T2 Shape | PASS | The test asserts byte-exact Elle-style operation history fields and `:info` outcomes for register and set histories at `crates/server/tests/consistency_workload.rs:82` and `crates/server/tests/consistency_workload.rs:133`. |
| T3 Runtime | NEEDS-HUMAN | Confirm the real loopback workload in an environment that permits binding: local execution failed only at `TcpListener::bind("127.0.0.1:0")` on `crates/server/tests/consistency_workload.rs:411`, while the socket-free subset passed. |
| T4 Contribution | NEEDS-HUMAN | Confirm no closed/rejected #406 work outside local git should supersede this: local affected-path history shows only the #405 observable substrate (`8af8e97`) for these paths, but issue-tracker rejected-work history is not available in the provided artifacts. |
| T5 Judgment | PASS | The patch stays on the ADR boundary by serializing and routing the verdict off-Check instead of deciding linearizability in Rust; the default dispatch is the privileged off-Check job at `crates/server/src/consistency_workload.rs:793`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide fitness after privileged validation: real Elle parser/verdict acceptance and the wire-driven CI green are intentionally outside what this sandbox could exercise, so sign-off must clear that deferred compatibility and runtime evidence. |

### Advisory — adversary

# Adversarial review — issue #406 (consistency workload + Elle serialization)

Skeptic's pass. I re-ran the asserted proof at `$PDCA_TARGET`: `cargo test -p wyrd-server
--test consistency_workload` → **11/11 green** (incl. the 2 wire-driven — loopback bind is
permitted in this sandbox). The per-fix "red" is a whole-file **compile** failure (the
`consistency_workload` module is absent pre-fix), not a per-assertion red; the golden /
rejection assertions themselves are non-tautological and genuinely flippable. I could not run
the full `cargo xtask ci` (C4-ci) — that gating pass I take provisionally. I attacked the two
iteration-4 tightenings the sign-off demanded and the reviewer marked closed. Two land.

## Refutations

- **NEEDS-HUMAN — Iteration-4 tightening 2 is only half-applied: `session_read_your_writes`
  manufactures a *definite* obligation from an *indeterminate* write/delete, false-REJECTING
  valid histories.** `crates/server/src/consistency_workload.rs:480-487` — the `Put` arm
  inserts `AtLeast(v)` and the `Delete` arm inserts `Absent` **without** checking
  `is_indeterminate(op.status)`, even though the `Get` arm guards it (`:490`). A 5xx/timeout
  write "might or might not have committed" (the module's own words, `:31-40`), yet the check
  treats it as a committed obligation. Concrete valid histories that get **rejected**:
  `[PUT k v=2 status=500; GET k v=1 status=200]` and `[PUT k v=1 status=200; DELETE k
  status=500; GET k v=1 status=200]`. I transcribed the exact algorithm and ran it: **both
  return `false`** (reject) — but both are legitimate, since the indeterminate mutation may
  never have landed. This is the *same* "bakes an indeterminate outcome into a definite one"
  fault the sign-off flagged, merely relocated from the serializer's `:fail` (which was fixed)
  and the *read* side (which was guarded) to the **obligation-establishing** side (which was
  not). It directly violates success-criterion (c) "ACCEPT a valid one" and the module's own
  invariant at `:38` (which conspicuously scopes the promise to "an indeterminate *read*"). It
  is not merely crafted: `ObservableS3Client::put` records `version: Some(version)` on **every**
  PUT regardless of wire status (`crates/server/src/consistency_observable.rs:173-180`), and a
  5xx is a real wire outcome, so a real workload history containing a 500 write feeds this bug.
  The reviewer's "tightening 2 addressed" claim in the brief is unwarranted for the session
  check.

- **NEEDS-HUMAN — Iteration-4 tightening 1 is over-broad the other way: the "genuine
  concurrency" witness ignores the object key, so a *cross-key* read↔write overlap counts —
  as vacuous for register linearizability as the read↔read overlap it was meant to exclude.**
  `crates/server/src/consistency_workload.rs:158-171` (`read_write_overlapping_pairs_across_processes`),
  `:236-238` (`is_read_write_pair`), `:224-226` (`spans_overlap`) test only `process`, span
  overlap, and op-kind — **never** `a.record.key == b.record.key`. The doc claims this is "the
  only overlap non-vacuous for register linearizability" (`:155-157`, `:173-176`), but a read
  of key `"a"` overlapping a write of key `"b"` places no ordering constraint on *any single
  register* — exactly the vacuity the tightening targeted. Concrete case:
  `MultiProcessHistory::from_process_ops([P1 GET "a" [0,10], P0 PUT "b" [5,15]])
  .is_genuinely_concurrent()` returns **`true`**. The wire test happens to drive a single key
  (`register-object`), so its green is sound *there*, but the exported witness over-accepts and
  the guarantee its docstring advertises does not hold in general. Whether cross-key overlap
  should count for the Elle register model is a judgment call → for human adjudication.

## Weaker points (noted, not scored as refutations)

- **Asymmetric indeterminate handling between the two session checks.**
  `session_read_your_writes` guards `is_indeterminate` on reads (`:490`) but
  `session_monotonic_reads` (`:514-529`) does not — it relies solely on `version == None`. For
  *real* client histories the two coincide (the client sets `version = None` on any non-200
  GET, `consistency_observable.rs:191-195`), but the two **public** checks would treat a crafted
  indeterminate GET that carries a version differently. Low impact; latent inconsistency in the
  exported surface.

- **`version_climbs_for_key` is tautological for the shipped workload.**
  `crates/server/src/consistency_workload.rs:182-196` derives "the version climbs" from the
  PUT's *written* (caller-supplied) version, i.e. the writer's loop counter `1..=overwrites`,
  never a backend-observed value — so for `run_concurrent_register_workload` it is `true` by
  construction and can never fail whatever the backend does. It adds no falsifiable signal to
  criterion (a); the real observation is `per_process_reads_monotone` (which does read observed
  GET versions). Not wrong, but weaker evidence than the brief implies.

## Attempted and could not refute

- The register and directory **byte-exact golden serializers** (`to_elle_edn` /
  `directory_to_elle_edn`): the expected EDN is written independently of the algorithm, the
  `:info` mappings for 5xx are correct, event ordering `(time, process, seq, phase)` keeps
  per-process invoke→completion nesting deterministic (no HashMap in the output path). I could
  not make it pass for the wrong reason.
- The **verdict-dispatch** seam (`verdict_dispatch`): default routes off-Check, the in-gate
  shell-out arm is representable but non-default; the test uses independent expectations and a
  panic arm. Non-tautological; could not refute.
- The **delete-then-read RYW** case the sign-off asked for is present and correct for
  *determinate* deletes (`:485-487`, tests at `tests/consistency_workload.rs`) — my refutation
  above is narrowly about the *indeterminate* delete/write, a distinct path.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Decide whether CI evidence discharges the wire leg: I confirmed reverse-patch red and 9 socket-free tests green, but this sandbox denies loopback bind at `crates/server/tests/consistency_workload.rs:411`, so the two wire-driven tests and full `cargo xtask ci` green remain provisional here.
- [ ] T3 Runtime — Confirm the real loopback workload in an environment that permits binding: local execution failed only at `TcpListener::bind("127.0.0.1:0")` on `crates/server/tests/consistency_workload.rs:411`, while the socket-free subset passed.
- [ ] T4 Contribution — Confirm no closed/rejected #406 work outside local git should supersede this: local affected-path history shows only the #405 observable substrate (`8af8e97`) for these paths, but issue-tracker rejected-work history is not available in the provided artifacts.
- [ ] Validation — fitness-to-purpose — Decide fitness after privileged validation: real Elle parser/verdict acceptance and the wire-driven CI green are intentionally outside what this sandbox could exercise, so sign-off must clear that deferred compatibility and runtime evidence.

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
- Iteration delta (if iterating): Rejected on the adversary's two refutations (both verified against patch.diff). Not fitness-to-purpose yet. The two iteration-4 tightenings were each fixed only at the exact spot flagged, then the SAME fault-class reappeared in an adjacent path. Re-scope both as surface-wide invariants, not point fixes, so the next Do closes the class rather than the instance: 1. Indeterminate-never-yields-a-definite-obligation must hold EVERYWHERE, not just in the serializer. `session_read_your_writes` establishes definite obligations from indeterminate mutations: the Put arm (AtLeast(v)) and Delete arm (Absent) do NOT guard is_indeterminate(op.status); only the Get arm does. Because ObservableS3Client::put records version: Some(_) on every PUT regardless of wire status, a 5xx write followed by a valid older read is FALSE-REJECTED — the same "bake indeterminate into definite" fault that sank v1-v3, relocated from the serializer :fail (fixed at iter-4) to the obligation-establishing side (missed). Plan the invariant across the whole check surface, and add a crafted [PUT k v=2 status=500; GET k v=1 status=200] (and the delete analogue) that must be ACCEPTED. 2. The genuine-concurrency witness must be per-register (same key). `read_write_overlapping_pairs_across_processes` tests process + span-overlap + read/write kind but NOT a.record.key == b.record.key, so a cross-key read↔write overlap counts as "genuinely concurrent" — vacuous for any single register, the same vacuity class the iter-4 read↔read tightening targeted. Require same-key overlap and add a crafted cross-key-only history that must NOT pass as concurrent. (The shipped single-key workload masks this, so a socket-free crafted red is needed.) C4/T3 loopback and T4 tracker items were NOT the basis for rejection (gate green; adversary re-ran 11/11 incl. both wire tests). Do NOT re-attempt the iter-4 approach unchanged — generalise the two tightenings.
- By / date: Eduard Ralph / 2026-07-07

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
