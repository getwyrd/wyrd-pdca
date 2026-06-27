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
