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

# Check Review — issue 250 / tier1-jepsen-consistency-harness

**Posture:** Advisory, artifact-only. Inputs: `patch.diff`, `brief.md`,
`check-gates.json` (build-notes.md withheld by design).

**Grounding note:** `$PDCA_TARGET` could not be resolved from this sandbox
(`printenv`/`env` blocked; multiple `wyrd` worktrees present and the resolved
target is ambiguous — I did **not** pick one, per "do not wander into other
checkouts"). Citations are therefore grounded on `patch.diff` plus the
human-authored brief's documented target state. The C4 gates ran green off the
base (`check-gates.json`: `C4-ci pass`, `C4-verify pass`), so there is **no**
stale/unreadable-target condition to mistake for a patch defect — C4 is not
failed on ordering grounds.

This is the 5th Check attempt; iterations 1–4 were all rejected for the same
"vacuous on first run" class (brief §§"Iteration 1–4 carry-forward"). The
re-derivation below tracks whether that class is genuinely closed.

## Verdict table (5/5/1)

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Success criterion is explicit and decomposed into 3 binding parts (dispatch rewire, real in-repo Jepsen+Elle suite, privileged `tier1-jepsen.yml`); patch supplies all three (`xtask/src/faults.rs` run_jepsen, `jepsen/`, `.github/workflows/tier1-jepsen.yml`). No spec ambiguity to escalate. |
| C2 Reproduction (red pre-fix) | PASS | Pre-fix `run_jepsen` shelled to the nonexistent `WYRD_TIER1_JEPSEN_CMD` and no `jepsen/`/workflow existed (born-at-tier criterion-absence). Decision owed: the flippable "red" is a *compile-absence* red (remove `pub mod jepsen` → integration test won't compile), not a behavioral failing assertion on the dispatch routing — accept as the declared net-new posture or require a behavioral red on the `Plan::Run → run_jepsen_harness` branch. |
| C3 Change | PASS | `faults.rs:run_jepsen` now matches `Plan` and routes `Plan::Run → run_jepsen_harness()` (`lein run test` in `jepsen/`), dropping the env-cmd shell-out; `execute`/`run_shell` correctly demoted to `#[cfg(test)]`. Mirrors the merged `run_disk_faults`/`run_tier1_scenario` shape as the brief requires. Note: `jepsen_harness_dir` is duplicated in `faults.rs` and `xtask/src/jepsen.rs` (doc claims a re-export; it is a copy) — minor, non-blocking. |
| C4 Verification (red→green) | PASS | `check-gates.json`: `C4-ci` (gating) pass and `C4-verify` pass — the Rust dispatch rewire compiles, lints, tests, and flips red→green. Decision owed: the gate verifies **only** the Rust dispatch wiring; by the declared Option-A posture the Jepsen/Elle substance (the actual deliverable) is exercised by **no** gate — see T3/Validation. |
| C5 Causal adequacy | NEEDS-HUMAN | Reconstruction is triggered by the **test-only** `detect_and_enqueue_missing` (`crates/chunkstore-grpc/tests/jepsen_custodian_step.rs`), which the file's own doc-comment admits the production read path does NOT do for simply-missing fragments (only present-but-corrupt; brief cites `crates/core/src/read.rs:189`). Decision owed: does driving production `reconcile_step`→`reconstruction::reconcile` off a test-injected enqueue count as exercising the **production repair trigger**, or is the missing-fragment detection gap itself the root cause that should live in the product? Carried open since iter-4; cannot be settled mechanically. |
| T1 Structure | PASS | New files land in conventional locations (`jepsen/`, `xtask/src/jepsen.rs`, `*/tests/*.rs`, workflow under `.github/workflows/`); dispatch change is localized to `run_jepsen`. Only smell is the duplicated `jepsen_harness_dir` helper (C3) — cosmetic. |
| T2 Shape | NEEDS-HUMAN | The suite models Wyrd as an Elle **list-append** register, but Wyrd is an immutable object store (`wyrd put`/`get` of distinct keys `jepsen/<slot>/<seq>`); there is no list state in Wyrd — the "list" is assembled entirely client-side. Decision owed: is list-append the right consistency model for an immutable-key store, or does the model/observation primitive mismatch (flagged since iter-1) make the Elle check structurally unable to find Wyrd-induced anomalies? |
| T3 Runtime | NEEDS-HUMAN | The Jepsen leg has never been run: no live `tier1-jepsen.yml` dispatch evidence, and `lein test`/`lein run test` are not exercised by `cargo xtask ci` (off-Check, ADR-0016). Decision owed: the brief requires a **demonstrated planted-anomaly catch over a non-vacuous live history** before this is trustworthy — the maintainer must run the first `workflow_dispatch` and confirm green-after-red. Until then runtime behavior of the deliverable is unverified. |
| T4 Contribution | NEEDS-HUMAN | Introduces a non-Cargo toolchain (JVM + Clojure + Leiningen + Jepsen + Elle; `jepsen/project.clj`) — outside `deny.toml`/`cargo-deny`. Pre-declared at plan (brief §"Ordering note", INTEGRATION §4). Decision owed: accept the new external test-toolchain footprint and decide whether a short ADR recording the non-Rust test-toolchain choice is warranted (the *how*, not the *whether* — 0005 already accepts Jepsen in principle). |
| T5 Judgment | NEEDS-HUMAN | The recurring iter-1→4 "vacuous history" class is **not** demonstrably closed. List order now derives from `alloc-position!` (client-side write-**completion** order), still invented in the Jepsen client, not read from Wyrd's stored/linearized state; with immutable single-write keys every successful `:r` returns a prefix of one monotonic client sequence, so Elle's list-append checker is near-guaranteed to pass and the only real assertion is the per-key data-integrity `AssertionError`. Decision owed: does this campaign give Elle genuinely Wyrd-induced interleavings, or is it still effectively checking the client's own bookkeeping? Same root-cause class that rejected all four prior attempts. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Whole-deliverable fitness is human-only and off-Check: the maintainer (Eduard Ralph) must validate a live `tier1-jepsen.yml` run against **clean upstream main** showing (a) a planted anomaly caught over a non-vacuous history (ADR-0009 "bug-finding run promoted to regression"), and (b) no stale/torn reads, repair commit-point-atomic (ADR-0015, `0005:277`,`0005:385-389`). None of this is observable at Check. |

## Notes for the human (feed §6)

- **Prior-art check** is documented by affected file path in the brief
  (§"Prior-art check": `faults.rs` history `0b5fea3`/#195, `02983aa`/#196 as
  pattern precedent; no existing `jepsen/` or `tier1-jepsen.yml`; no open/closed
  PR builds the Jepsen leg). I could not independently re-run it against the
  target (target unresolved) — confirm not-a-duplicate at sign-off.
- **C5 / T2 / T5 are the load-bearing escalations** and are interrelated: the
  test-injected repair trigger (C5), the list-append-over-immutable-keys model
  (T2), and the client-invented list order (T5) together determine whether the
  first live run is non-vacuous. They are the same failure class that rejected
  iterations 1–4; a reviewer cannot settle them mechanically because the
  substance runs only off-Check.
- **What IS solidly verified:** the Rust dispatch rewire (`run_jepsen` →
  in-repo `lein run test`), its opt-in gating, and the workflow/compose
  plumbing — `cargo xtask ci` green and red→green confirmed by the gates.
- Iter-4 advisory items 1 (corrupt/stale read now throws `AssertionError` past
  `catch Exception`) and 3 (docker network disconnect/connect now throw on
  nonzero exit) appear addressed in `jepsen/src/wyrd/jepsen.clj`; item 2
  (client-side list order) is only **partially** addressed — see T5.

### Advisory — codex

- jepsen/src/wyrd/jepsen.clj:657 - The harness expects reads to keep succeeding from the four surviving servers during a kill/partition, but every `wyrd get` first constructs the CLI fanout by dialing every endpoint and returns an error on the first unreachable D-server (`crates/server/src/cli.rs:451`). During the actual fault windows, reads therefore become `:fail` before the any-k read path can run, leaving Elle with little or no under-fault read history to check. Use a harness read path that can tolerate missing endpoints when constructing the fanout, or inject faults below an already-built client set.
- jepsen/src/wyrd/jepsen.clj:698 - The stated non-vacuous concurrency premise does not match the target metadata backend. Each Jepsen op shells out to a separate `wyrd` process, and each process opens the shared redb file through `open_cluster_meta` (`crates/server/src/cli.rs:536`); redb takes an exclusive database file lock at open, so concurrent CLI processes fail instead of naturally serializing through write transactions. That means the five-worker workload mainly creates `:fail` operations rather than real overlapping successful appends/reads for Elle.
- jepsen/src/wyrd/jepsen.clj:390 - Missing values are converted into an exception caught as `:fail`, including the final-read phase that is supposed to verify post-repair readability (`jepsen/src/wyrd/jepsen.clj:718`). Since the composed checker only runs Elle over successful operations, committed data that remains unreadable after repair can be ignored rather than failing the Jepsen result; final/post-repair reads need a hard-fail path or a checker assertion that they all succeed.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Reconstruction is triggered by the **test-only** `detect_and_enqueue_missing` (`crates/chunkstore-grpc/tests/jepsen_custodian_step.rs`), which the file's own doc-comment admits the production read path does NOT do for simply-missing fragments (only present-but-corrupt; brief cites `crates/core/src/read.rs:189`). Decision owed: does driving production `reconcile_step`→`reconstruction::reconcile` off a test-injected enqueue count as exercising the **production repair trigger**, or is the missing-fragment detection gap itself the root cause that should live in the product? Carried open since iter-4; cannot be settled mechanically.
- [ ] T2 Shape — The suite models Wyrd as an Elle **list-append** register, but Wyrd is an immutable object store (`wyrd put`/`get` of distinct keys `jepsen/<slot>/<seq>`); there is no list state in Wyrd — the "list" is assembled entirely client-side. Decision owed: is list-append the right consistency model for an immutable-key store, or does the model/observation primitive mismatch (flagged since iter-1) make the Elle check structurally unable to find Wyrd-induced anomalies?
- [ ] T3 Runtime — The Jepsen leg has never been run: no live `tier1-jepsen.yml` dispatch evidence, and `lein test`/`lein run test` are not exercised by `cargo xtask ci` (off-Check, ADR-0016). Decision owed: the brief requires a **demonstrated planted-anomaly catch over a non-vacuous live history** before this is trustworthy — the maintainer must run the first `workflow_dispatch` and confirm green-after-red. Until then runtime behavior of the deliverable is unverified.
- [ ] T4 Contribution — Introduces a non-Cargo toolchain (JVM + Clojure + Leiningen + Jepsen + Elle; `jepsen/project.clj`) — outside `deny.toml`/`cargo-deny`. Pre-declared at plan (brief §"Ordering note", INTEGRATION §4). Decision owed: accept the new external test-toolchain footprint and decide whether a short ADR recording the non-Rust test-toolchain choice is warranted (the *how*, not the *whether* — 0005 already accepts Jepsen in principle).
- [ ] T5 Judgment — The recurring iter-1→4 "vacuous history" class is **not** demonstrably closed. List order now derives from `alloc-position!` (client-side write-**completion** order), still invented in the Jepsen client, not read from Wyrd's stored/linearized state; with immutable single-write keys every successful `:r` returns a prefix of one monotonic client sequence, so Elle's list-append checker is near-guaranteed to pass and the only real assertion is the per-key data-integrity `AssertionError`. Decision owed: does this campaign give Elle genuinely Wyrd-induced interleavings, or is it still effectively checking the client's own bookkeeping? Same root-cause class that rejected all four prior attempts.
- [ ] Validation — fitness-to-purpose — Whole-deliverable fitness is human-only and off-Check: the maintainer (Eduard Ralph) must validate a live `tier1-jepsen.yml` run against **clean upstream main** showing (a) a planted anomaly caught over a non-vacuous history (ADR-0009 "bug-finding run promoted to regression"), and (b) no stale/torn reads, repair commit-point-atomic (ADR-0015, `0005:277`,`0005:385-389`). None of this is observable at Check.

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
- Iteration delta (if iterating): Why rejected (issue_250): 5th attempt, same "vacuous history" class that sank iterations 1-4. The cause is at the PLAN level, not the patch: Elle's list-append check presupposes a mutable, linearizable shared register, but Wyrd is an immutable single-write-per-key object store (`wyrd put`/`get` of distinct keys `jepsen/<slot>/<seq>`). The "list" and its order are therefore invented in the Jepsen client (the `slot-writes`/`slot-positions` atoms, `alloc-position!`), so Elle checks the harness's own bookkeeping, not state Wyrd linearized. Each iteration has patched a symptom of this (seq->completion order, catch-Exception swallowing, network nemesis throw, ephemeral ports) without changing the observable — so the class recurs. Handing this to Do again under the same model will very likely reproduce it a 6th time. What to change in the plan (not the patch): - Replace the list-append-over-immutable-store framing with an observable Wyrd actually linearizes. Check properties Wyrd genuinely has — read-after-commit, no torn/stale reads, repair commit-point-atomicity (ADR-0015, `0005:277`, `0005:385-389`) — against the metadata store's versioned commits, rather than list-append over a client-invented list. - Decide C5 at plan: the repair trigger is currently a test-only `detect_and_enqueue_missing`; production read path does not enqueue simply-missing fragments (`crates/core/src/read.rs:189`). Either the missing-fragment detection gap is the real product defect to fix, or the spec must state that the test-injected enqueue is an accepted stand-in — don't leave it ambiguous for a 6th Do. - Address the substrate constraints the codex advisories raise, because they make a live history vacuous even if the model were right: * redb takes an exclusive file lock at open (`crates/server/src/cli.rs:536`, `open_cluster_meta`); each op is a separate `wyrd` process, so concurrent CLI ops fail rather than serialize -> no real concurrency. (The patch's architecture comment claims the opposite; verify before re-planning.) * `wyrd get` builds its fanout by dialing every endpoint and fails on the first unreachable D-server (`crates/server/src/cli.rs:451`), so reads :fail during the exact fault windows -> no under-fault read history. * post-repair unreadable values are caught as :fail and ignored by the checker (`jepsen/src/wyrd/jepsen.clj:390`,`:718`); the post-repair readability assertion needs a hard-fail path. - Reconfirm the Option-A decision in light of the above: if a genuine non-vacuous Jepsen/Elle run is not reachable given Wyrd's immutable-store semantics + the per-process redb lock, the plan should say so and pick the property-based framing rather than re-attempting list-append. What IS solidly verified and need not be rebuilt: the Rust dispatch rewire (`run_jepsen` -> in-repo `lein run test`), its opt-in gating, and the workflow/compose plumbing (`cargo xtask ci` green, red->green confirmed). The miss is the consistency model and the observable, not the wiring.
- By / date: Eduard Ralph / 2026-06-27

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
