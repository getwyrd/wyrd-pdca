# Result — issue 250 / tier1-jepsen-consistency-harness

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: The Tier-1 **Jepsen** consistency leg of proposal 0005 §13.2 (`0005:408`) was
- Success criterion: **DECISION (the human/maintainer chose Option A): build the genuine
- Repo + branch target: getwyrd/wyrd @ main   (per INTEGRATION §2 — Wyrd targets `main`;
- Scope (one logical fix) / out of scope: Build the Tier-1 Jepsen consistency leg as the genuine Jepsen framework

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

# Check review — issue 250 / tier1-jepsen-consistency-harness (iteration 3)

**Advisory. Deterministic gates block; this annotates.** No Write/Edit performed.

## Grounding / target-state caveat

`$PDCA_TARGET` could not be read — every env-access path was blocked by the sandbox
approval layer (`printenv`/`env`/`/proc/self/environ` all denied). I therefore ground
against `patch.diff` plus a clean upstream checkout at `/home/eddie/wyrd/wyrd` that
**matches the brief's described origin/main pre-change state exactly**: `run_jepsen`
calls `execute("Tier-1 Jepsen", plan, "WYRD_TIER1_JEPSEN_CMD")` at
`xtask/src/faults.rs:170`; CLI dispatch is put/get/d-server/demo only at
`crates/server/src/cli.rs:56`; no `jepsen/` dir and no `.github/workflows/tier1-jepsen.yml`.
This is a valid grounding surface even if it is not literally the driver-resolved worktree.
Citations below are to that checkout; all also hold against `patch.diff`.

## What I re-derived

- **Pre-change defect confirmed** at `faults.rs:170` (inert dispatch to nonexistent
  `WYRD_TIER1_JEPSEN_CMD`); `jepsen/` and `tier1-jepsen.yml` absent — matches brief.
- **API surface now exists** (the iteration-1 rejection cause): `ReconstructionContext{meta,
  fleet,topology}` (`crates/custodian/src/reconstruction.rs:69`), `reconcile_step`
  (`reconciliation.rs:65`), `Reconciled::{Changed,Satisfied}` (`reconciliation.rs:20`),
  `Custodian::elect(coord,zone)` (`leadership.rs:31`), `Topology::{register,distinct_domains,
  domain_of}` (`core/src/placement.rs:72-118`), `repair::enqueue_repair` (`core/src/repair.rs:78`),
  `RedbMetadataStore::open` (`metadata-redb/src/lib.rs:30`), `GrpcChunkStore::connect`
  (`chunkstore-grpc/src/client.rs:49`), `get_fragment`/`scan` traits. The new Rust test is
  real API-bound code, and C4-ci (compile+test of `cargo test --workspace`) passing confirms it builds.
- **Read-path claim confirmed**: `crates/core/src/read.rs:188-209` admits/repairs only
  *present-but-corrupt* fragments; a missing fragment (`Ok(None)`) is read around, **not**
  enqueued. This grounds both the patch's `detect_and_enqueue_missing` rationale **and** the
  C5 concern below.
- **Cron non-collision confirmed**: existing schedules are 03/04/04/05 UTC; patch uses
  `0 2 * * *` (02:00) — free. Reused Dockerfile/compose paths exist on target.
- **Prior-iteration advisories all mechanically addressed**: partition nemesis added
  (`nemesis/compose`, jepsen.clj); custodian step now asserts `enqueued>0` **and**
  `Reconciled::Changed`; partial reads now throw → `:fail`; self-test uses `(true?/false?
  (:valid? …))`; ports resolved dynamically; nemesis targets compose containers by name.

## Verdict table

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Success criterion is coherent and testable: 3 binding deliverables (dispatch rewire+unit test; in-repo Jepsen/Elle harness; privileged `tier1-jepsen.yml`), with deferral posture stated up front (brief.md:25-46). |
| C2 Reproduction (red pre-fix) | PASS | Pre-fix inert dispatch confirmed at `faults.rs:170`. Note: this is a net-new/born-at-tier leg, so the "red" is criterion-absence — `xtask/tests/jepsen_orchestration.rs` fails to **compile** without the new `xtask::jepsen` module — not a behavioral reproduction of the inert-dispatch defect itself. |
| C3 Change | PASS | Diff matches spec: `run_jepsen`→`run_jepsen_harness` rewire (faults.rs), net-new `jepsen/` Clojure harness + self-test, `jepsen_custodian_step.rs`, `tier1-jepsen.yml`, `xtask::jepsen` helper. One logical change (the Jepsen leg). |
| C4 Verification (red→green) | PASS | check-gates.json: C4-ci and C4-verify both PASS (deterministic; trusted, not re-runnable from the artifact-only review dir). Scope caveat, not a FAIL: the gate exercises ONLY the Rust dispatch + path-helper unit tests + compile of the custodian step. All Clojure/nemesis/Elle/docker substance is unexercised at Check — the declared Option-A deferral. |
| C5 Causal adequacy | NEEDS-HUMAN | Decision owed: missing-fragment detection lives in **test** code (`detect_and_enqueue_missing`) because the production read path (`read.rs:188-209`) never enqueues repair for a wholesale-missing fragment after node loss — so the harness may drive a repair-enqueue that production itself never triggers this way. Human must decide whether a **production** mechanism (scrub / custodian sweep) should detect node-loss and be driven instead, vs. the test-side workaround being acceptable orchestration. Matters because a green run could prove a path production never takes. |
| T1 Structure | PASS | Sensible layout: `jepsen/` lein project, `xtask/src/jepsen.rs` host-independent helper behind `pub mod jepsen`, harness/workflow as net-new files; no unrelated edits. |
| T2 Shape | PASS | Mirrors the merged sibling precedent — dispatch shape matches `run_disk_faults`→`run_tier1_scenario` (`faults.rs:118-165`); workflow modelled on `tier1-disk-faults.yml`/`tier2-kill-reconstruct.yml`. |
| T3 Runtime | NEEDS-HUMAN | Decision owed: runtime correctness is entirely unverified at Check and carries concrete risks the first `tier1-jepsen.yml` run must settle — tmpfs `/data` actually resetting on `docker start` (not recreate) to simulate data loss; `docker network disconnect` vs host-published ports actually partitioning the client's dial path; `--scale dserver=5` yielding the assumed `wyrd-jepsen-dserver-1..5` names. Matters because the prior two iterations were rejected precisely for runtime/substance gaps the Rust gate can't see. |
| T4 Contribution | PASS | Substantial net-new harness is a real contribution. Caveat (not blocking): the Check-time regression guard is thin — it asserts `jepsen_harness_dir` returns `<root>/jepsen` (a path join) and compile-existence, but **no** test asserts `run_jepsen` actually routes to `run_jepsen_harness` rather than `execute(…,"WYRD_TIER1_JEPSEN_CMD")`; the substantive dispatch rewire has no direct behavioral assertion. |
| T5 Judgment | NEEDS-HUMAN | Decision owed: the consistency-model design may make Elle's check vacuous — `:concurrency 1` (single redb writer) plus a distinct immutable key per append (`jepsen/<slot>/<seq>`, read back individually) reduces the "list-append" campaign toward "did I read back what I wrote," with little concurrent interleaving for Elle to find G1/G2 anomalies in. This is the same "tests nothing substantive on first run" class that sank iterations 1–2, now in the model. Human must judge whether this is a credible Jepsen consistency artifact or needs genuine concurrency. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Maintainer (Eduard Ralph) must, before sign-off: (a) run `tier1-jepsen.yml` via `workflow_dispatch` and confirm a **demonstrated red** — the self-test/harness actually catching a planted anomaly (ADR-0009) — not a green resting on a vacuous history; (b) confirm the run is non-vacuous over the **production** repair path; (c) weigh the pre-declared new non-Rust toolchain (JVM+Clojure+lein+Jepsen+Elle, `jepsen/project.clj`) per INTEGRATION §4 and decide whether a short ADR recording the test-toolchain decision is warranted. Prior-art check ran by file path (faults.rs #195/#196 history; no `jepsen/`, no `tier1-jepsen.yml`, no PR builds the leg) and is mechanically corroborated on target — not a duplicate. |

## Summary

Iteration 3 mechanically clears every iteration-1 and iteration-2 advisory and the API
surface is now real (resolving the iteration-1 root cause). Deterministic gates pass. The
open risks are not mechanically settleable at Check and are deferred by the accepted
Option-A posture: (C5) detection-in-test vs production node-loss detection; (T3) unverified
runtime behavior; (T5) a possibly-vacuous single-worker/per-key consistency model; (V) the
mandatory first live `tier1-jepsen.yml` run demonstrating a caught anomaly, plus the
toolchain-ADR call. Given the two prior rejections were both "vacuous on first run," the
maintainer should treat the live dispatch run + demonstrated-red as a hard precondition to
sign-off, not optional supplementary evidence.

### Advisory — codex

- jepsen/src/wyrd/jepsen.clj:179 — The harness runs `wyrd get ... --endpoints <all endpoints>` as a fresh CLI process for every read, but the CLI constructs the fanout by connecting to every endpoint before reading (`crates/server/src/cli.rs:445`). During a crash or partition, one endpoint is intentionally unreachable, so reads fail before the any-`k` reconstruction path can read around the missing server. Elle will mostly see `:fail` reads during the actual fault windows instead of successful observations from surviving fragments.
- jepsen/src/wyrd/jepsen.clj:495 — The list-append workload uses `(rand-int 1000000)` as the appended value. Elle's list-append histories rely on appended elements being unique within a list; with the current run length and five slots, duplicate values are plausible and can make a healthy run look anomalous or ambiguous. Use the already-allocated per-slot sequence/key as the appended value instead of a random value.
- NEEDS-HUMAN — .github/workflows/tier1-jepsen.yml:83 — Option A adds and downloads a non-Cargo test toolchain (JVM/Clojure/Leiningen plus Jepsen/Elle). This was pre-declared in the brief, but a maintainer still needs to decide at sign-off whether the dependency posture should be recorded in a short ADR or equivalent project note.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Decision owed: missing-fragment detection lives in **test** code (`detect_and_enqueue_missing`) because the production read path (`read.rs:188-209`) never enqueues repair for a wholesale-missing fragment after node loss — so the harness may drive a repair-enqueue that production itself never triggers this way. Human must decide whether a **production** mechanism (scrub / custodian sweep) should detect node-loss and be driven instead, vs. the test-side workaround being acceptable orchestration. Matters because a green run could prove a path production never takes.
- [ ] T3 Runtime — Decision owed: runtime correctness is entirely unverified at Check and carries concrete risks the first `tier1-jepsen.yml` run must settle — tmpfs `/data` actually resetting on `docker start` (not recreate) to simulate data loss; `docker network disconnect` vs host-published ports actually partitioning the client's dial path; `--scale dserver=5` yielding the assumed `wyrd-jepsen-dserver-1..5` names. Matters because the prior two iterations were rejected precisely for runtime/substance gaps the Rust gate can't see.
- [ ] T5 Judgment — Decision owed: the consistency-model design may make Elle's check vacuous — `:concurrency 1` (single redb writer) plus a distinct immutable key per append (`jepsen/<slot>/<seq>`, read back individually) reduces the "list-append" campaign toward "did I read back what I wrote," with little concurrent interleaving for Elle to find G1/G2 anomalies in. This is the same "tests nothing substantive on first run" class that sank iterations 1–2, now in the model. Human must judge whether this is a credible Jepsen consistency artifact or needs genuine concurrency.
- [ ] Validation — fitness-to-purpose — Maintainer (Eduard Ralph) must, before sign-off: (a) run `tier1-jepsen.yml` via `workflow_dispatch` and confirm a **demonstrated red** — the self-test/harness actually catching a planted anomaly (ADR-0009) — not a green resting on a vacuous history; (b) confirm the run is non-vacuous over the **production** repair path; (c) weigh the pre-declared new non-Rust toolchain (JVM+Clojure+lein+Jepsen+Elle, `jepsen/project.clj`) per INTEGRATION §4 and decide whether a short ADR recording the test-toolchain decision is warranted. Prior-art check ran by file path (faults.rs #195/#196 history; no `jepsen/`, no `tier1-jepsen.yml`, no PR builds the leg) and is mechanically corroborated on target — not a duplicate.
- [ ] .github/workflows/tier1-jepsen.yml:83 — Option A adds and downloads a non-Cargo test toolchain (JVM/Clojure/Leiningen plus Jepsen/Elle). This was pre-declared in the brief, but a maintainer still needs to decide at sign-off whether the dependency posture should be recorded in a short ADR or equivalent project note.

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
- Iteration delta (if iterating): Rejected for the same "vacuous on first run" class that sank iterations 1-2 — this time in the consistency model itself, not the plumbing. Fix before re-submit: - T5 (item 3) — the Elle check is near-vacuous: :concurrency 1 (single redb writer) plus a distinct immutable key per append (jepsen/<slot>/<seq>, read back individually) collapses "list-append" toward "did I read back what I wrote," leaving no concurrent interleaving for Elle to find G1/G2 anomalies. Give the workload genuine concurrency so the consistency checker has real interleaved histories to verify. If single-writer redb is a hard constraint, redesign the workload (or the metadata access) so the campaign is non-vacuous rather than retaining a per-key read-back. - Item 5 (codex advisory, jepsen/src/wyrd/jepsen.clj:495) — list-append uses (rand-int 1000000) as the appended value; Elle's list-append histories require appended elements to be unique within a list. With the current run length and five slots, duplicate values are plausible and can make a healthy run look anomalous or a real anomaly ambiguous. Use the already-allocated per-slot sequence/key as the appended value instead of a random value. Re-submit only with a demonstrated red from a live tier1-jepsen.yml dispatch (the harness catching a planted anomaly over a non-vacuous history) — the Validation/T3 items in §6 still stand for the next Check.
- By / date: Eduard Ralph / 2026-06-26

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
