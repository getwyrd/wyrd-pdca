# Brief — issue 197 / reconstruction-repaired-success-identity

> The Plan artifact (docs 02 §PLAN). Human-authored. Do reads ONLY this file.
> The field labels below are parsed by the driver — keep the `- **Label:** value`
> shape. The success criterion is the load-bearing field: it is the sentence
> Check tests "did this work" against.

- **Slug:** reconstruction-repaired-success-identity
- **Defect:** The reconstruction custodian's durability telemetry over-reports successful
  repairs. `emit_repaired` fires once per plan **up front**, before the rebuild/commit
  loop (`crates/custodian/src/reconstruction.rs:162-165`), incrementing
  `reconstruction_repaired` for every plan the pass attempts. The loop's
  `RepairOutcome::Conflict` arm is offset by `reconstruction_conflict`, but the
  `RepairOutcome::Aborted` arm (`:170`) is offset by **nothing**. The metric's own
  documented identity — "successful repairs are `reconstruction_repaired − conflict`"
  (`:159-160`, `:435-436`) — therefore over-counts true successes by the number of
  Aborted plans, so the durability plane reports more repairs than actually committed.
- **Success criterion:** After a reconstruction pass over a plan set whose outcomes are a
  mix of `Committed`, `Conflict`, and `Aborted`, the quantity the telemetry defines as
  "successful repairs" equals **exactly the count of `Committed` plans** — neither
  `Conflict` nor `Aborted` plans inflate it. Demonstrable in-process at C4-verify via the
  custodian telemetry-capture harness (`tracing_subscriber`, already used in
  `crates/custodian/tests/reconstruction.rs`): construct a pass with at least one Aborted
  plan and assert the success identity holds. BINDING is the identity (committed-count ==
  derived-successes); whether Do restores it by gating the emission on `Committed` or by
  adding a distinct aborted counter and updating the documented identity is ILLUSTRATIVE
  — Do's call. Note the load-bearing constraint Do must preserve: the up-front emission is
  deliberate (`:151-161`) — the `tracing`→OTel bridge can drop events emitted *after* the
  heavy erasure-decode/commit section under load — so the fix must not reintroduce
  unreliable late emission of the metrics that matter.
- **Invariant to restore:** The durability-plane telemetry's success identity must hold
  over the reconstruction-pass category: the value the metrics define as "successful
  repairs" equals the number of plans whose outcome was `Committed`. A plan that aborts
  (no commit) or loses its CAS race (conflict) is **not** a success and must not be
  counted as one. Source: proposal 0005 §326-332 (the three M3 repair metrics and their
  contract) and ADR-0011 (durability telemetry) — internal project invariant (Tier C),
  also stated in-code at `reconstruction.rs:159-160`. (Behavioural accounting fix per
  principles.md §1.1; not a structural/lifecycle defect — the Plan-exit structural gate
  does not apply.)
- **Repo + branch target:** getwyrd/wyrd @ main   (resolve here at Plan — do not leave to Do)
- **Surfaces:** data
- **Scope:** restore the success identity for `reconstruction_repaired` so an Aborted
  plan is not counted as a successful repair. / out of scope: the `time_to_repair`
  elapsed-window correction (it records the pass's absolute instant `now_millis` at
  `:436`, not a true elapsed repair window) — a real but separate defect that requires
  carrying a per-obligation enqueue stamp through the shared repair queue's value
  encoding (a data-model change, self-flagged at `:427-431` as a later refinement); file/
  track it under this issue but do not bundle it into this one logical change.
- **Repro instruction:** On `main` @ `c2223a5`, drive a `reconcile_step` reconstruction
  pass (per the harness in `crates/custodian/tests/reconstruction.rs`) over a plan set
  engineered to yield at least one `RepairOutcome::Aborted` alongside a `Committed` plan
  (e.g. a plan whose rebuild/commit cannot proceed). Capture the emitted
  `reconstruction_repaired` and `reconstruction_conflict` counters; observe that
  `reconstruction_repaired − reconstruction_conflict` exceeds the number of plans that
  actually committed, by the Aborted count.
- **Test file:** crates/custodian/tests/reconstruction.rs   (a pass with a Committed + an
  Aborted plan; assert derived-successes == committed-count — red pre-fix, green post-fix)
- **Citations expected:** Do must cite path:line on the target branch for every change.
- **Prior-art check (triage cycles):** searched `crates/custodian/src/reconstruction.rs`
  across merged history (only `5fb905c` / PR #190 "reconstruct under-replicated chunks",
  which introduced this emission), open PRs (`gh pr list --state open` — none touch this
  file), and closed PRs — no prior or in-flight fix for this accounting defect.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.
