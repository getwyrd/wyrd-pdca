# Brief — issue 406 / elle-register-listappend-models-and-workload-recorder

> The Plan artifact (docs 02 §PLAN). Human-authored. Do reads ONLY this file.
> The field labels below are parsed by the driver — keep the `- **Label:** value`
> shape. The success criterion is the load-bearing field: it is the sentence
> Check tests "did this work" against.
>
> NET-NEW functionality (not a bug fix): the "Defect" field states the GAP/need, and
> the minimalism maxim does not govern (principle 1.3); there is no invariant-to-restore.
> This slice implements the **accepted** ADR-0041 (acceptance issue #410, CLOSED) — the
> Plan artifact of record — so no new design proposal is needed.

- **Slug:** elle-register-listappend-models-and-workload-recorder
- **Defect:** (gap / need) Wyrd has **no** externally-recognizable consistency-checker
  artifact for its mutable metadata register. ADR-0041 settled the substrate — model the
  **metadata commit-point register** (inode `version` under the commit CAS), never the
  immutable fragment path — but the checker **models**, the **concurrent workload** that
  produces a non-vacuous history, and the **history recorder** do not yet exist. Slice 3
  of #329 builds exactly those: (1) an Elle-compatible **rw-register** model (primary) —
  concurrent overwrites + reads of a small shared key set, checked for linearizability of
  the commit point (guarantee 2: read-after-commit, no torn/stale read,
  exactly-one-writer-wins, no version regression); (2) a **list-append / set** model
  (secondary) — concurrent create/delete/rename in a directory, checked for namespace
  linearizability (guarantee 1: no lost create, no resurrected delete); (3) session
  **read-your-writes + monotonic-read** checks over the register and `meta:version`
  (guarantee 3); (4) the **history recorder** + a checker-compatible history serialization
  and the **Elle (or equivalent recognized) verdict step**. Without the models a clean run
  is trivially empty; without the concurrent workload the history is vacuous.
- **Success criterion:** In `cargo xtask ci` (the unprivileged, container-free gate):
  (a) the rw-register model **rejects** a hand-crafted **inconsistent** history (a stale/torn
  read, a `version` regression, or two-winners-at-one-commit-point) and **accepts** a valid
  linearizable one; (b) the list-append/set model **rejects** a lost-create /
  resurrected-delete history and accepts a valid one; (c) the session checks reject a
  read-your-writes / monotonic-read violation over the register and `meta:version`; and
  (d) the concurrent workload driver, run against the **in-process gateway**, produces a
  **non-vacuous** recorded history — one containing genuinely concurrent, overlapping ops
  and real overwrites that bump the inode `version` at the commit point — which the
  recorder serializes in the checker-compatible format, and which the register model then
  passes. Each of (a)–(c) is a flippable red→green assertion in the named test file.
- **Falsifiability:** RED is producible **on the plain `cargo xtask ci` environment** — no
  cluster is needed. The models are pure Rust over a recorded history: point Do at a test
  that feeds each model a **crafted-inconsistent** history and asserts rejection (this is
  RED until the model exists / whenever it is weakened) and a valid history and asserts
  acceptance; and that runs the workload against the in-process `Gateway` (as
  `crates/server/tests/closed_write_path.rs` drives it) and asserts the produced history is
  non-vacuous. This does NOT rest on non-existence: the inconsistent-history-rejection
  assertions give a demonstrated red on real inputs. (The *live Elle verdict* and the
  *fault-injected, wire-driven credibility run* are deferred off-Check — see Verification
  posture / Production reach — but the model logic and the non-vacuous-history production
  are exercised here.)
- **Repo + branch target:** getwyrd/wyrd @ feat/m4-production-metadata-backend
  (M4 slice — stacks on the integration branch per INTEGRATION §2, not `main`)
- **Depends on:** (none in-batch — see Ordering note)
- **Ordering note:** Intentionally NO wave dependency is set. The genuine build-on prereq
  #405 is OPEN and NOT in this batch; this slice is scoped so its Check-exercised core does
  not require #405's code, so it is not folded as a wave dependency (a bare `Depends on: 405`
  the batch cannot satisfy would only stall scheduling). HUMAN: confirm this scoping or hold
  406 for #405. #329's slice order is #405 (networked client observable, slice 2,
  **OPEN**) → **#406 (this, slice 3)** → #407 (nemesis run, slice 4). Slice 3's deliverables
  — the checker **models + workload + history recorder** — are the reusable machinery and do
  NOT require #405 to build or to be exercised at Check: the workload can drive the
  **in-process** `Gateway` to produce a genuine non-vacuous history now (ADR-0041 leaves the
  Rust-driver-vs-Clojure choice to #329, and the established "deferred ≠ unbuilt" pattern in
  `xtask/src/metadata_faults.rs` is exactly this shape — a pure Check-testable core now, the
  privileged live leg deferred). #405's **networked** client observable (client-observed
  real-time order over the wire) and #407's **fault nemesis** wire this machinery into the
  full credibility run later. **Open sequencing question for the human:** proceed now with
  the in-process-driven core (a thin, reusable workload driver that #405 later re-targets at
  the wire), which is what this brief specifies — OR hold 406 until #405 lands so the
  workload drives the networked observable from the start. Briefed for the former; confirm
  or hold.
- **Surfaces:** data
- **Difficulty:** high — a **new consistency-checker subsystem**: two checker models,
  session checks, a concurrent workload driver, a history recorder + checker-compatible
  serialization, and the seam to the off-Check verdict leg. Likely a new module/crate in
  the workspace plus an xtask hook for the off-Check runner. Broad new surface a reviewer
  must hold in view, even though it ripples little through existing call sites.
- **Scope:** Build the checker **models** (rw-register primary; list-append/set secondary),
  the **session** read-your-writes + monotonic-read checks over the register and
  `meta:version`, the **concurrent workload driver** (drives overwriting PUT/GET and
  directory create/delete/rename against the in-process gateway), the **history recorder**
  + checker-compatible history serialization, and the **wiring/entry point** that hands a
  recorded history to the Elle (or equivalent recognized) checker. Model on the **mutable
  metadata register only** (ADR-0041: inode `version` under the commit CAS, `dirent` set,
  `meta:version`) — **never** the immutable chunk/fragment path (that is the vacuous-history
  mistake ADR-0041 rejects). / out of scope: the **networked** client observable and its
  wire real-time order (#405); the **real-cluster fault nemesis** run — partition / skew /
  pause (#407, reusing #257); the **execution** of the live Elle/JVM verdict inside CI (it
  is an off-Check privileged job — ADR-0016/ADR-0041 keep JVM/Clojure out of `cargo xtask
  ci`); any cross-zone / `meta:version` failover-fence strengthening (M10/M11); any change to
  the gateway or `MetadataStore` (this slice is a *consumer* of the existing gateway API).
- **Repro instruction:** On `feat/m4-production-metadata-backend`, read
  `docs/design/adr/0041-consistency-checker-substrate.md` (the substrate decision),
  `docs/design/adr/0015-consistency-contract.md` (the three guarantees), the existing
  in-repo consistency scenario `crates/metadata-tikv/tests/tier1_metadata_consistency.rs`
  and the testkit oracles it uses (`consistency_passes` etc.,
  `crates/testkit/src/lib.rs:441`), and the in-process gateway driving pattern in
  `crates/server/tests/closed_write_path.rs`. There is no prior models/workload/recorder to
  run — this slice creates them; "reproduction" is the absence of a checker able to yield a
  non-vacuous, checkable history.
- **External dependencies:** **none for the Check-exercised core** — the models, workload
  driver, and history recorder are pure Rust and run under the base toolchain in `cargo
  xtask ci`. The **Elle verdict** step needs a recognized checker (Elle → JVM/Clojure, or an
  equivalent) which per ADR-0041/ADR-0016 runs **only in a privileged off-Check job and MUST
  NOT enter `cargo xtask ci`** — so it is not a build/verify dependency of this bundle at
  Check. Do MUST NOT pull a JVM/Clojure dependency into the merge gate; if the recorded-history
  format needs to match a specific Elle input schema, encode/serialize to it in Rust and
  defer the verdict execution off-Check.
- **Test file:** `crates/testkit/tests/consistency_models.rs` (new) — feeds each model
  crafted valid/invalid histories (flippable red→green) and asserts the in-process workload
  produces a non-vacuous history the register model accepts. (Do MAY house the model/workload
  implementation in a new module or crate; the regression test is the load-bearing,
  C4-verify-flippable artifact and MUST live where `cargo xtask ci` runs it.)
- **Verification posture:** DEFERRED / net-new — declared so C2/C4 land as a pre-declared
  sign-off item, not a surprise NEEDS-HUMAN. (a) NET-NEW infrastructure: the models,
  workload, and recorder are born-at-tier; but "red" here is NOT rested on non-existence —
  Do MUST ship the crafted-**inconsistent**-history-rejection assertions (a genuine, flippable
  red on real inputs). What IS built AND exercised at Check: the two models + session checks
  (unit-tested against valid/invalid histories) and the workload+recorder (exercised by
  producing and validating a non-vacuous in-process history). (b) DEFERRED off-Check: the
  **live Elle/recognized-checker verdict** on the serialized history — its green is observable
  only in the privileged off-Check job (ADR-0041/ADR-0016), confirmed by the maintainer /
  nightly job, not at Check. The deferred verdict is over the SAME history the Check-exercised
  recorder produces (not inert scaffolding); this slice is not merely dispatch plumbing —
  the model/checker logic is functionally implemented and tested here.
- **Production reach:** This slice builds the checker seam ahead of its production driver.
  What honours the seam now: the **in-process workload** driving the real in-process
  `Gateway` commit point — real overwriting PUTs that bump the inode `version`, real
  directory mutations — producing a genuine non-vacuous history the models check
  (load-bearing, not dead scaffolding). What production/the full artifact still does: the
  **wire-observed, real-time-ordered** driving (#405) and the **fault-injected run on the real
  M4 cluster** (#407, reusing #257's nemesis) are not yet present. Those land in the named
  later slices; a networked client observable (#405) must exist before the workload drives the
  wire surface, and #257's cluster + nemesis before the fault run.
- **Citations expected:** Do must cite path:line on `feat/m4-production-metadata-backend`
  for every change, and cite ADR-0041 for the register/list-append targeting decision and
  the JVM-off-Check constraint. Peer patterns Do MAY open: the in-process gateway driving in
  `crates/server/tests/closed_write_path.rs`; the testkit consistency oracles at
  `crates/testkit/src/lib.rs:394-623` (`partition_materialized`, `consistency_passes`,
  `partition_took_effect`, `heal_is_complete`); and the "deferred ≠ unbuilt" pure-core /
  deferred-live-leg split in `xtask/src/metadata_faults.rs:1-55`.
- **Prior-art check (triage cycles):** Searched the tree on
  `feat/m4-production-metadata-backend` for `jepsen|elle|checker|register|list_append|
  history|consistency`. Existing: the ADR-0039 in-repo Option-B scenario over the
  **immutable repair path** (`.github/workflows/tier1-jepsen.yml`,
  `crates/chunkstore-grpc/tests/tier1_jepsen_consistency.rs`,
  `crates/metadata-tikv/tests/tier1_metadata_consistency.rs`) and the testkit oracles —
  these observe a **different layer** (repair/data path) on purpose (ADR-0041 §Consequences)
  and are NOT the mutable-register checker this slice builds; no duplication. The register /
  list-append **models + workload + recorder** do not exist yet. The parent epic #329 is
  CLOSED (re-sliced into #405/#406/#407); ADR-0041 accepted via #410 (CLOSED). Not a
  duplicate.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected — the register model FALSE-ACCEPTS a read of a value that only a definitely-FAILED write ever attempted (reviewer C5 FAIL + adversary, empirically ACCEPTED: [write_ok k=10@v1; write_fail k=99; read_ok k=99@v2] returns Ok; value 99 never committed, version 2 never produced). A consistency checker whose whole value is its correctness MUST detect this — if it does not, that is a fault to fix, not to accept. Fix: gate the value-provenance set on committed writes only (skip !c.ok), as the sibling namespace model already does; add a crafted history with a failed-write-value read as a flippable red so the regression is covered. Also address the adversary's improvement remarks in the rebuild: - rename branch of the namespace model is unexercised and unrecordable — HistoryRecorder exposes no rename method and no test constructs a rename op; either wire rename through the recorder with a crafted red, or drop the dead branch. - the produced workload never trips the session (read-your-writes / monotonic-read) reject paths: contended HOT writes record version=None, so only uncontended single-writer keys have observable own-writes and the reject logic is exercised only by crafted histories. Make the workload able to trip them, or scope the inline "live teeth" claim down. - exactly-one-writer-wins is only partially modeled: two winners with identical values, and a double-commit masked as version=None, are not surfaced. - confirm the recorded red->green is a model-WEAKENING red, not a compile-error red (reverting the whole patch fails to compile, which the brief says does not count). C4 full cargo xtask ci not cleared here — CI to be run locally after the correction lands.
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected: the register model still FALSE-ACCEPTS on the CONTENDED path — the same false-accept class that sank iteration-1, now recurring where it matters most. Root cause: contended writes record version=None, and every load-bearing check is gated on Some(version), so on exactly the contended ops the model's detection is switched off: - stale read after a committed overwrite is accepted — Pass-3 version-regression excludes version=None writes (consistency.rs:412-419); - provenance is per-key, not per-(key,version), so a superseded value reads clean (consistency.rs:379); - a vanished committed value / lost write is not detected — absent read is skipped (consistency.rs:377); - two-winners-at-one-commit-point is not counted when both writes are version=None (consistency.rs:344-346). NOT acceptable incompleteness: version=None is the COMMON case in the very workload fed to check_register, so on criterion (d)'s contended ops "the register model passes" is near-guaranteed regardless of correctness — the checker's whole value is that it detects these, and here it cannot. What to change (same brief, no re-plan): make the checker fire stale-read / lost-write / two-winners detection on version=None writes too. Preferred: capture the REAL commit version even under contention (thread the committed inode version back through put_with_retry instead of dropping it to None) so Pass-1/Pass-3 observe every committed write; alternatively, have the model conservatively REJECT unresolvable version=None overwrites rather than silently skip them. Add a crafted flippable red for each of the four cases above so the regression is locked in. Do NOT re-run the previous approach unchanged: the builder is already at the top escalation rung (opus-xhigh = opus + max thinking budget), so there is no stronger model to escalate to — this pointed carry-forward is the whole leverage. Do NOT relitigate the accepted deferred-Elle / off-CI verdict split; that scope is fine and was not the reason for rejection.
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: Third attempt; the same register false-accept class that sank iterations 1 and 2 recurs — the adversary compiled the shipped model and empirically accepted a stale/torn read [w k=10@v1; w k=20@v2; r k=10@v3] (superseded value at a version no committed write produced), so brief Success criterion (a) — "rw-register model rejects a hand-crafted inconsistent history" — is still not met. Two pointed iterate-do carry-forwards did not close it, so the problem is plan-level, not a rebuild detail: re-plan the approach rather than re-issuing the same brief. For the next Plan, reconsider (1) whether a hand-rolled pure-Rust register pre-filter is the right vehicle at all vs. leaning on the real Elle verdict for correctness and scoping the Rust slice to history-production/recording only; (2) the sequencing question — hold 406 for #405's networked observable so the workload isn't a hot-key-serialized in-process fixture that proves non-vacuity but not correctness; (3) make criterion (a) a first-class, model-weakening flippable red covering stale-read-at-phantom-version explicitly.
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
