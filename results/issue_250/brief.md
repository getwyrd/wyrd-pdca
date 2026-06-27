# Brief — issue 250 / tier1-jepsen-consistency-harness

> The Plan artifact (docs 02 §PLAN). Human-authored. Do reads ONLY this file.
> The field labels below are parsed by the driver — keep the `- **Label:** value`
> shape. The success criterion is the load-bearing field: it is the sentence
> Check tests "did this work" against.

- **Slug:** tier1-jepsen-consistency-harness

- **Planning artifact:** proposal 0005 (M3 — Custodians) §13.2 — the accepted file
  `docs/design/proposals/accepted/0005-milestone-3-custodians.md` (`0005:405-411`; the
  Tier-1 "**Jepsen** consistency runs over the repair path" line is `0005:408`). Consistency
  contract asserted: **ADR-0015** (namespace ops linearizable; per-file writes linearizable
  at the home zone; read-your-writes + monotonic reads). Tier model: **ADR-0009** (a
  bug-finding run promoted to a permanent regression) and **ADR-0016** (privileged tiers kept
  out of the unprivileged, container-free `cargo xtask ci`). Commit-point-atomicity of repair:
  `0005:50`, `0005:260`, `0005:386-388` ("a crashed repair placed-but-did-not-commit is
  collectable garbage, not corruption"); the repair read→reconstruct→re-place pipeline
  `0005:275`. DST partition model this leg is the live complement of:
  `crates/dst/tests/network.rs` (clog/unclog links; "an injected partition/timeout aborts the
  write before commit, leaving only leased garbage, never a half-committed chunk").

- **Defect:** The Tier-1 consistency-over-repair leg of proposal 0005 §13.2 (`0005:408`) was
  never built. On getwyrd/wyrd@origin/main, `xtask/src/faults.rs::run_jepsen` (`faults.rs:170`)
  is **inert dispatch scaffolding**: it gates on `WYRD_TIER1=1`, probes for `lein`, and shells
  out via `execute("Tier-1 Jepsen", plan, "WYRD_TIER1_JEPSEN_CMD")` (`faults.rs:74-92`,
  `run_shell` `:94-110`) to an externally-supplied command that **exists nowhere in-repo**.
  Only the opt-in gating decision (`plan()`, `faults.rs:40`) is unit-tested. There is no
  in-repo harness asserting consistency over the custodian repair/reconstruction path under
  **partitions and crashes**, and no privileged CI job to run one. This is the #146 "deferred ≠
  unbuilt" gap — a tier waved through as deferred but never built — the same gap #195 (disk-fault
  leg) and #196 (kill-reconstruct leg) closed for their legs in this same file.

  **Why this bundle is being re-planned (read before building).** Eight prior iterations
  (`iteration-v1/`..`iteration-v8/`) all failed. Iterations 1-5 pursued literal
  Clojure/Jepsen/Elle (Option A) and produced only vacuous histories (Elle's list-append
  presupposes a mutable linearizable register; Wyrd is an immutable single-write-per-key object
  store, so the "list" is invented client-side and the checker validates the harness's own
  bookkeeping, not Wyrd). The maintainer chose **Option B** — an in-repo Rust scenario mirroring
  the two merged siblings. Iterations 6-8 then all failed C2/C4 on **one recurring Plan-level
  design gap**, correctly diagnosed at the iter-8 sign-off: the routing decision ("which harness
  to run") was welded inside the private, container-spawning runner, so the only Check-time test
  surface was a constant or a net-new-module-deletion *compile* seam — **nothing ever forced a
  behavioural flip when `run_jepsen` regressed to the external shell-out.** Re-running the same
  brief reproduced the same shape. This brief therefore re-plans the **seam**, not just the test,
  and folds in two further items the iter-8 sign-off named (real partition injection; one oracle).

- **Success criterion:** **STRUCTURAL DECISION (maintainer-chosen Option B, mirroring the two
  merged sibling legs): build the consistency-over-repair leg as an in-repo Rust scenario that
  drives the PRODUCTION repair path and asserts the ADR-0015 contract directly.** Literal
  Clojure/Jepsen/Elle (Option A) is explicitly NOT this bundle (re-filed as a follow-on — see
  Ordering note). Three BINDING parts, demonstrable at Check (C4-verify, the patch applied in
  isolation):

  1. **Testably-routed dispatch (the primary fix — the iter-6/7/8 failure).** `run_jepsen` no
     longer shells out to the nonexistent external `WYRD_TIER1_JEPSEN_CMD`; it routes to the
     in-repo Tier-1 consistency scenario, gating on **`docker`** (mirroring `run_kill_reconstruct`,
     `faults.rs:196`), not the obsolete `lein`. The routing decision MUST be an **observable
     value that `run_jepsen` actually consumes**, with the container spawn placed strictly
     *downstream* of it, so a Check-time unit test in `faults.rs`'s own `#[cfg(test)] mod tests`
     (the slot the existing `plan()` tests already use, `faults.rs:300+`) can assert the route
     targets the in-repo scenario. The flippable regression MUST be **red iff the dispatch
     regresses to the external shell-out** — NOT red merely because a net-new module was deleted
     (the iter-7/8 compile-seam tautology), and NOT a test over a constant the runner never reads
     (the iter-6 tautology). Reverting `run_jepsen` to `execute(..., "WYRD_TIER1_JEPSEN_CMD")`
     must turn the test red.
  2. **Real, buildable harness driving the production path under partitions AND crashes.** The
     in-repo scenario exists as real Rust whose `#[ignore]`d body compiles and type-checks under
     `cargo xtask ci` (so an API regression stubbing `reconcile_step`/`reconstruction::reconcile`
     fails the merge gate, exactly as `tier2_kill_reconstruct.rs:447-453` already does). It drives
     the **production** custodian repair path (`custodian::reconcile_step`, `reconciliation.rs:65`
     → `reconstruction::reconcile`, `reconstruction.rs:121`) against a real containerized D-server
     cluster, and injects BOTH **crashes** (a killed node — `docker kill`, the sibling fault) AND
     a **real network partition** — a node *alive-but-unreachable* (a *transient* fault in
     `is_permanent_read_fault` terms, `reconstruction.rs:312+`), distinct from the crash — injected
     **mid-repair and then healed**, asserting repair converges **exactly once** across the heal
     (read-after-commit holds; no torn/stale reads; repair neither lost nor duplicated;
     commit-point-atomic — a partition/crash before commit leaves collectable garbage, never a
     duplicate placement or torn chunk).
  3. **One privileged CI job.** A new privileged **`tier1-jepsen.yml`** (nightly schedule +
     `workflow_dispatch`, `WYRD_TIER1=1`) runs the scenario, on a **non-colliding cron slot** (the
     siblings hold 03:00 and 05:00 UTC; use e.g. 02:00 or 06:00), kept OUT of the unprivileged
     container-free `cargo xtask ci` (ADR-0016).

  DEFERRED / off-Check supplementary evidence (NOT the Check gate — see Verification posture):
  the scenario running **green** against the live cluster, confirmed in the `tier1-jepsen.yml`
  run. ILLUSTRATIVE (Do's call, not a scope NEEDS-HUMAN): the exact scenario file path, the
  precise cluster size / compose layout, the exact partition *mechanism* (`docker pause/unpause`
  recommended primary; `docker network disconnect/connect` the named alternative — both
  alive-but-unreachable and reversible; `iptables`/`tc` asymmetric partitions are OUT of scope),
  the exact shape of the routing value (an enum, a returned cargo invocation, …), the exact
  assertion phrasing, and the cron minute.

- **Invariant to restore:** An off-Check verification tier must be a **real, built, exercised
  harness whose routing decision is an observable value a Check-time test binds to** — so a
  regression to inert dispatch scaffolding (re-pointing to an external command absent in-repo)
  **flips a test**, not merely compiles — AND it must **drive the PRODUCTION path and assert the
  genuine consistency contract**, not a client-invented observable the product never linearizes.
  "Deferred ≠ unbuilt": a tier that only decides *whether* to run, with nothing in-repo to run
  and no test bound to the routing it claims, has not been built. The property the leg makes true
  is **Wyrd's stated consistency contract over the repair path** (ADR-0015: read-after-commit, no
  torn/stale reads; repair commit-point-atomic, neither lost nor duplicated — `0005:386-388`),
  exercised under both crash and **partition** faults. Sources: the #146 verification-posture
  forcing function (reproduced in `templates/brief.md.tpl` "Verification posture" and this issue's
  DoD) — internal project rule (Tier C); **ADR-0015** (the contract asserted); **ADR-0009** (the
  bug-finding run promoted to a permanent regression); and the in-repo precedent of the two
  merged siblings (`run_disk_faults` → `tier1_disk_faults`, #195; `run_kill_reconstruct` →
  `tier2_kill_reconstruct`, #196). SELF-TEST: could Do satisfy this by guarding a single module —
  re-pointing the env var, adding a `plan()` branch in `faults.rs`, or testing a constant? **No** —
  with no in-repo scenario, no testably-bound routing value, no partition fault, and no privileged
  job, there is nothing to dispatch to and nothing the test would catch; the invariant demands the
  production reconcile path be driven and the contract asserted under partition + crash, and a
  routing value a test actually flips on.

- **Repo + branch target:** getwyrd/wyrd @ main   (per INTEGRATION §2 — Wyrd targets `main`; no
  maintenance branches)
- **Onto branch:**
- **Depends on:**
- **Depends on (merged):**
- **Conflicts with:**
- **Stacks on:**
- **Ordering note:** Last of the #195 split family. Its two sibling legs are **already merged on
  origin/main** — `run_disk_faults`/`run_tier1_scenario` (#195) and `run_kill_reconstruct` (#196)
  are present in `xtask/src/faults.rs`; `run_jepsen` (`faults.rs:170`) is the lone remaining stub.
  The production reconstruction fix (#251) is independently merged. No build-on dependency and no
  co-scheduling conflict: this edits only `run_jepsen` in `faults.rs` (the sibling runners are
  merged, not concurrently edited) plus net-new files (a scenario, a shared oracle module if Do
  extracts one, and a workflow). **Structural decision recorded (after 5 Option-A rejections):**
  build the leg as an in-repo Rust scenario (Option B), NOT literal Clojure/Jepsen/Elle.
  **Two pre-declared sign-off items (kept — they do not block the brief):** (a) **T4 / ADR** —
  Option B changes the *how* of accepted proposal 0005, which names "Jepsen" literally
  (`0005:408`); the maintainer should weigh a short clarifying/superseding ADR recording "Tier-1
  consistency-over-repair as an in-repo Rust scenario now; the literal public Jepsen credibility
  artifact as a later follow-on once the substrate supports a non-vacuous run" (accepted ADRs are
  immutable — INTEGRATION §2 — so it is a *new* ADR, never an edit). (b) **Two follow-on issues
  to file:** the literal-Jepsen credibility artifact (Option A, blocked on substrate fixes), and
  the **missing-fragment detection product gap** (production scrub `continue`s on `Ok(None)` and
  the read path enqueues only present-but-corrupt — see Production reach).

- **Surfaces:** data   (the custodian repair/reconstruction path + xtask dispatch + CI; no GUI)

- **Difficulty:** high   (net-new infrastructure spanning an `xtask` dispatch **restructure** —
  the routing seam must become a test-observable value — a new in-repo scenario driving the
  production reconcile path under BOTH partition and crash faults against a live containerized
  cluster, a unified consistency oracle, and a new privileged CI workflow. Wide, cross-cutting
  reach a reviewer must hold in view; rated up per the safe default and the 8-iteration history.)

- **Scope:** Build the Tier-1 consistency-over-repair leg as an in-repo Rust scenario (Option B):
  (1) **restructure `run_jepsen`** so it routes to the in-repo scenario (gating on `docker`, opt-in
  `WYRD_TIER1=1`) through a routing decision that is observable to a Check-time unit test, with the
  container spawn downstream — replacing the external `WYRD_TIER1_JEPSEN_CMD` shell-out and the
  obsolete `lein` probe; (2) an **in-repo Rust scenario test** that stands up a real containerized
  Wyrd/D-server cluster (reusing the Tier-2 compose plumbing `compose_up`/`resolve_endpoints`/
  `finish_integration`/`finalize_panic_safe`, `xtask/src/main.rs:229-360`), injects **both a crash
  and a real network partition mid-repair** and **heals the partition**, drives the **production**
  repair path (`reconcile_step` → `reconstruction::reconcile`), and asserts the ADR-0015 contract
  over the repair path (read-after-commit; no stale/torn reads; repair neither lost nor duplicated;
  commit-point-atomic), converging **exactly once** across the heal — modelled on the merged
  `tier2_kill_reconstruct.rs`; (3) a **single, shared consistency oracle**: the assertion helpers
  the Check unit tests cover MUST be the *same* helpers the live scenario asserts with (no decorative
  second oracle that Check exercises but the live run never touches — the iter-8 T2/T4 finding), and
  the oracle MUST treat a committed value becoming **unreadable** (an observed read with no write-id
  after a commit) as a **violation** of read-after-commit, not as valid; (4) a new **`tier1-jepsen.yml`**
  privileged CI job (nightly + `workflow_dispatch`, `WYRD_TIER1=1`) on a non-colliding cron slot,
  modelled on `tier1-disk-faults.yml`/`tier2-kill-reconstruct.yml`, kept out of `cargo xtask ci`
  (ADR-0016). / **out of scope:** literal Clojure/Jepsen/Elle (re-filed as a follow-on credibility
  artifact — Ordering note); `iptables`/`tc` asymmetric/one-way partitions (symmetric, reversible
  isolation is sufficient and faithful for the repair-path contract); fixing the missing-fragment
  **detection** product gap (use the sanctioned `enqueue_repair` test stand-in per the merged #196
  precedent — see Production reach — with the gap re-filed as a follow-on); changing the production
  repair/reconstruction code (that was #251, already merged); the disk-fault leg (#195) and
  kill-reconstruct leg (#196), already built and merged; adding any new toolchain to the
  unprivileged `cargo xtask ci` merge gate; making `tier1-jepsen.yml` a required PR/merge-gate
  status check (it is post-merge, nightly + on-demand, like its siblings).

- **Repro instruction:** On getwyrd/wyrd @ main: `run_jepsen` at `xtask/src/faults.rs:170` calls
  `execute("Tier-1 Jepsen", plan, "WYRD_TIER1_JEPSEN_CMD")` — opting in (`WYRD_TIER1=1`) with any
  fabricated `WYRD_TIER1_JEPSEN_CMD` shells out to an external command, but **no in-repo consistency
  harness exists** (no Tier-1 Jepsen/consistency scenario test, no `tier1-jepsen.yml` workflow —
  `git -C ../wyrd ls-files` / `cat-file -e` confirm both absent on origin/main). Pre-change: `cargo
  xtask jepsen` can only ever shell out to a nonexistent/foreign command; the leg is inert, and
  there is no test surface bound to its routing. Post-change: an in-repo Rust scenario exists;
  `run_jepsen` routes to it through a test-observable value; the `tier1-jepsen.yml` nightly/dispatch
  job runs it privileged against a live cluster under partitions + crashes.

- **Test file:** `xtask/src/faults.rs` — the in-file `#[cfg(test)] mod tests` block (NOT an
  `xtask/tests/` integration test: `faults` is a binary module, unreachable from `xtask/tests/` —
  the iter-7 dead-end; the in-file unit module CAN see the private routing value, as the existing
  `plan()` tests do, `faults.rs:300+`). The Check-time **flippable regression**: assert that
  `run_jepsen`'s routing decision, when opted in + `docker` available, targets the **in-repo**
  Tier-1 scenario invocation (the `cargo test --test <scenario> -- --ignored` shape its siblings use)
  and does **not** read the external `WYRD_TIER1_JEPSEN_CMD` — red pre-change (routing goes to the
  env-supplied external command), green post-change (routing goes to the in-repo scenario), and red
  again if reverted to the shell-out. Plus the net-new scenario itself (ILLUSTRATIVE path, e.g.
  `crates/chunkstore-grpc/tests/tier1_jepsen_consistency.rs`, sibling to `tier2_kill_reconstruct.rs`):
  its `#[ignore]`d body compiles and type-checks under `cargo xtask ci`, and the **shared consistency
  oracle** is unit-tested over hand-authored histories — including a partition-induced **negative
  control** (a planted anomaly the oracle must CATCH).

- **Verification posture:** DECLARED — net-new + DEFERRED/off-Check, stated up front so Check lands
  it as a pre-declared sign-off item, not a surprise NEEDS-HUMAN. (i) **Built AND exercised at Check**
  (`cargo xtask ci`): the `run_jepsen` **routing restructure** — now a test-observable value — with a
  flippable unit test bound to the dispatch (red iff it regresses to the external shell-out, per
  Success criterion §1 and Test file; this is the load-bearing improvement over iters 6-8); the
  scenario's Rust harness **compiles and type-checks under the merge gate** (the `#[ignore]`d body
  binds the production `reconcile_step`/`reconstruction::reconcile` API, so an API regression fails
  `cargo xtask ci`, as `tier2_kill_reconstruct.rs` already does); and the **shared consistency
  oracle** unit-tested over hand-authored histories at Check — the SAME oracle the live scenario
  asserts with (no decorative second oracle). (ii) **NET-NEW, born-at-tier** (red = criterion-ABSENCE
  for the new files): the scenario and the `tier1-jepsen.yml` workflow are new; the flippable
  behavioural red is the dispatch-routing test above. (iii) **DEFERRED / off-Check**: the live
  consistency run (real containerized cluster + partition + crash mid-repair + heal, driving the
  production reconcile path) is observable **only** in the privileged `tier1-jepsen.yml` job
  (containers excluded from the unprivileged merge gate, ADR-0016). WHO confirms the deferred green:
  the **maintainer (Eduard Ralph)** reviewing the first on-demand `tier1-jepsen.yml`
  (`workflow_dispatch`) run. FORCING-FUNCTION honesty (#146): the shared oracle MUST be exercised by a
  **demonstrated red** — a negative-control / planted-anomaly unit assertion proving it is
  load-bearing (e.g. that a withheld repair / heal, a stale-or-torn read, a post-commit unreadable
  value, or a duplicated re-placement is actually CAUGHT) per ADR-0009 — rather than resting green on
  non-existence.

- **Production reach:** DECLARED. The repair **trigger** is a **sanctioned test stand-in**, following
  the merged #196 precedent verbatim: `tier2_kill_reconstruct.rs:545` calls
  `repair::enqueue_repair(&meta, CHUNK, "tier2-test")` as the health-check-producer stand-in. This is
  necessary because **no production path enqueues repair for a simply-missing fragment** — verified on
  origin/main: production scrub `continue`s on `Ok(None)` (`crates/custodian/src/scrub.rs`, the "loss
  for GC/reconstruction to notice" arm), a killed/partitioned server cannot be `list_fragments()`'d,
  and the read path enqueues only present-but-corrupt fragments (`crates/core/src/read.rs`,
  `corrupt.push`). (a) WHAT honours the seam now: the scenario enqueues the repair obligation (the
  stand-in) and then drives the **production** `reconcile_step` → `reconstruction::reconcile` over it
  against the live cluster under partition + crash — the reconstruction path is genuinely traversed,
  not stubbed; (b) WHERE production wiring lands: the **follow-on missing-fragment-detection product
  fix** (Ordering note) — until then the enqueue stand-in is the accepted bridge, exactly as #196
  accepted it; (c) the stand-in is **load-bearing**: the scenario must assert reconstruction actually
  FIRED (the affected fragment is rebuilt and re-placed in a distinct failure domain, the placement no
  longer references the dead/partitioned server) and converged **exactly once** across the heal — not
  merely that `reconcile_step` returned `Ok` — so an empty-queue no-op cannot pass (the iter-2
  advisory).

- **Citations expected:** Do must cite path:line on the target branch (main) for every change — the
  `run_jepsen` routing restructure in `xtask/src/faults.rs`, the new scenario, the shared oracle
  module, and the new workflow file.

- **Prior-art check (triage cycles):** Searched by file path across merged history and open/closed
  work. `xtask/src/faults.rs` history — #195 (disk-fault leg) and #196 (kill-reconstruct leg) built
  the two sibling runners in **this same file**; `run_jepsen` (`faults.rs:170`) is the only one still
  stubbed against an external command. `crates/chunkstore-grpc/tests/tier2_kill_reconstruct.rs` is the
  **binding precedent** for both the scenario shape (drives production `reconcile_step`/
  `reconstruction::reconcile` against a live containerized cluster, `#[ignore]`d body compiled by the
  merge gate) AND the repair-enqueue stand-in (`:545`). No Tier-1 Jepsen/consistency scenario test and
  no `.github/workflows/tier1-jepsen.yml` exist on origin/main (`git ls-files` / `cat-file -e` confirm
  absent). `tier1-disk-faults.yml` (cron 03:00, `WYRD_TIER1=1`) and `tier2-kill-reconstruct.yml`
  (cron 05:00, `WYRD_TIER2=1`) are the workflow models to mirror. Iterations 1-5 of THIS bundle
  pursued Option A (literal Clojure/Jepsen/Elle) and were all rejected (preserved in `iteration-v1/`..
  `iteration-v5/`); iterations 6-8 pursued Option B but all failed C2/C4 on the routing-seam gap this
  brief re-plans (preserved in `iteration-v6/`..`iteration-v8/`). No open or closed PR builds the
  Tier-1 consistency leg. Not a duplicate — the siblings are the **pattern precedent**, not the same
  fix.

- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.
