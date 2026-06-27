# Brief — issue 250 / tier1-jepsen-consistency-harness

> The Plan artifact (docs 02 §PLAN). Human-authored. Do reads ONLY this file.
> The field labels below are parsed by the driver — keep the `- **Label:** value`
> shape. The success criterion is the load-bearing field: it is the sentence
> Check tests "did this work" against.

- **Slug:** tier1-jepsen-consistency-harness
- **Planning artifact:** proposal 0005 (M3 — Custodians) §13.2 (`0005:405-411`, the
  Tier-1 "Jepsen consistency runs over the repair path" + Tier-2 kill-and-reconstruct);
  crate touch-point `0005:437-438`; PR-sequence slice 8 `0005:541-545`. Consistency
  contract: **ADR-0015** (namespace ops linearizable; per-file writes linearizable at the
  home zone; read-your-writes + monotonic reads). Tier model: **ADR-0009** (Tier-2 = real
  networked backends under containers; a bug-finding run promoted as a permanent
  regression) and **ADR-0016** (privileged tiers kept out of the unprivileged container-free
  `cargo xtask ci`). Commit-point-atomicity of repair: `0005:277`, `0005:385-389`.
  Architecture framing for the *literal-Jepsen* credibility artifact (NOT this bundle — see
  Ordering note): `docs/design/architecture/10-quality-risks-glossary.md:115`.

- **Defect:** The Tier-1 consistency-over-repair leg of proposal 0005 §13.2 (`0005:408`) was
  never built. `xtask/src/faults.rs::run_jepsen` (`faults.rs:170`, getwyrd/wyrd@main) is
  **inert dispatch scaffolding**: it gates on `WYRD_TIER1=1` and shells out via
  `execute(..., "WYRD_TIER1_JEPSEN_CMD")` to an externally-supplied command that **does not
  exist anywhere in-repo**; only the opt-in gating decision (`plan()`) is unit-tested. There
  is no real harness asserting consistency over the custodian repair/reconstruction path
  under partitions and crashes, and no privileged CI job to run one. This is the #146
  "deferred ≠ unbuilt" gap — a tier waved through as "deferred" but never built — the same
  gap #195 (disk-fault leg) and #196 (kill-reconstruct leg) closed for their legs.

- **Success criterion:** **STRUCTURAL DECISION (the maintainer chose Option B, mirroring the
  two merged sibling legs): build the consistency-over-repair leg as an in-repo Rust scenario
  test that drives the PRODUCTION repair path and asserts the ADR-0015 contract directly.**
  Literal Clojure/Jepsen/Elle (Option A) was explicitly NOT chosen for this bundle — five
  prior iterations established it cannot produce a non-vacuous run against Wyrd's
  immutable-single-write-per-key store with the current substrate; it is re-filed as a
  follow-on (Ordering note). BINDING and **demonstrable at Check (C4-verify, the patch applied
  in isolation)**: (1) `run_jepsen` no longer shells out to the nonexistent external
  `WYRD_TIER1_JEPSEN_CMD`, but **dispatches to the in-repo Tier-1 consistency scenario**
  (mirroring how `run_disk_faults` → `run_tier1_scenario` dispatches to
  `crates/custodian/tests/tier1_disk_faults.rs` at `faults.rs:118-165`, and
  `run_kill_reconstruct` → `tier2_kill_reconstruct.rs` at `faults.rs:196+`), and that dispatch
  wiring + its opt-in gating is unit-tested inside `cargo xtask ci`; (2) the in-repo scenario
  exists as real, **buildable Rust** harness code — its `#[ignore]`d body compiles and
  type-checks under `cargo xtask ci` (so an API regression that stubbed `reconcile_step` would
  fail the merge gate, exactly as `tier2_kill_reconstruct.rs:447-453` already does) — and it
  drives the **production** custodian repair/reconstruction path (`custodian::reconcile_step`
  → `reconstruction::reconcile`) against a real containerized D-server cluster, injecting
  **partitions and crashes** mid-repair, and asserts the consistency outcome over the repair
  path; (3) a new privileged **`tier1-jepsen.yml`** CI job (nightly schedule +
  `workflow_dispatch`, `WYRD_TIER1=1`) runs it, kept OUT of the unprivileged container-free
  `cargo xtask ci` (ADR-0016). DEFERRED / off-Check supplementary evidence (NOT the Check
  gate — see Verification posture): the scenario running **green** against the live cluster —
  read-after-commit holds, no stale or torn reads (ADR-0015), repair neither lost nor
  duplicated (commit-point-atomic; a crash mid-repair leaves collectable garbage, never
  corruption or duplicate placement — `0005:277`, `0005:385-389`) — confirmed in the
  `tier1-jepsen.yml` run. BINDING parts: the dispatch rewire, the scenario existing as real
  Rust harness code that compiles under the merge gate and drives the production reconcile
  path, and the privileged job. ILLUSTRATIVE: the exact scenario file path, the precise
  cluster size / compose layout, the exact assertion phrasing, and the cron minute.

- **Invariant to restore:** An off-Check verification tier must be a **real, built, and
  exercised harness that drives the PRODUCTION path and asserts the genuine consistency
  contract** — not inert dispatch scaffolding that shells out to a command absent in-repo, and
  not a checker run over an observable the product does not actually linearize. "Deferred ≠
  unbuilt": a tier that only decides *whether* to run, with nothing in-repo to run, has not
  been built; equally, a harness that asserts over a client-invented observable (a "list" the
  immutable-key store never linearizes) checks its own bookkeeping, not Wyrd — it is unbuilt
  in substance. The property the leg must make true is **Wyrd's stated consistency contract
  over the repair path** (ADR-0015: read-after-commit, no torn/stale reads; repair
  commit-point-atomic, neither lost nor duplicated — `0005:277`, `0005:385-389`). Sources:
  the #146 verification-posture forcing function (reproduced in `templates/brief.md.tpl`
  "Verification posture" and this issue's DoD) — internal project rule (Tier C); **ADR-0015**
  (the consistency contract the harness asserts); **ADR-0009** (the leg's bug-finding run is
  promoted to a permanent regression); and the established in-repo precedent of the two
  sibling legs (`run_disk_faults` → `tier1_disk_faults`, #195; `run_kill_reconstruct` →
  `tier2_kill_reconstruct`, #196). SELF-TEST: could Do satisfy this by guarding a single
  module — re-pointing the env var, adding a `plan()` branch in `faults.rs`? **No** — with no
  in-repo scenario and no privileged job there is nothing to dispatch to, and the invariant
  demands the production reconcile path actually be driven and the contract asserted; a
  dispatch-glue-only change visibly fails it.

- **Repo + branch target:** getwyrd/wyrd @ main   (per INTEGRATION §2 — Wyrd targets `main`;
  no maintenance branches)
- **Onto branch:**
- **Depends on:**
- **Depends on (merged):**
- **Conflicts with:**
- **Stacks on:**
- **Ordering note:** Last of the #195 split family. Its two sibling legs are **already merged
  on origin/main** — `run_disk_faults`/`run_tier1_scenario` (#195, `0b5fea3`) and
  `run_kill_reconstruct` (#196, `02983aa`) are present in `xtask/src/faults.rs`; `run_jepsen`
  is the lone remaining stub. The production reconstruction read-around fix (#251) is
  independently merged. No build-on dependency and no co-scheduling conflict: this touches
  only `run_jepsen` in `faults.rs` (the sibling runners are merged, not concurrently edited)
  plus net-new files. **Structural decision recorded (after 5 rejected iterations):** this
  bundle builds the leg as an **in-repo Rust scenario (Option B)**, mirroring the two merged
  siblings — NOT literal Clojure/Jepsen/Elle. Iterations 1-5 (preserved in `iteration-v1/`..
  `iteration-v5/`) pursued Option A and were all rejected for the same "vacuous-history" class
  (the iter-5 sign-off identified the cause as Plan-level: Elle's list-append presupposes a
  mutable linearizable register, but Wyrd is an immutable single-write-per-key object store,
  so the "list" is invented client-side and Elle checks the harness's own bookkeeping; the
  per-process redb exclusive lock + `wyrd get` failing on the first unreachable endpoint make
  a live CLI-driven history vacuous regardless). **Two pre-declared sign-off items (not
  blocking the brief):** (a) **T4 / ADR** — Option B changes the *how* of accepted proposal
  0005, which names "Jepsen" literally (`0005:408`); the maintainer should weigh a short
  clarifying/superseding ADR recording "Tier-1 consistency-over-repair as an in-repo Rust
  scenario now; the literal public Jepsen credibility artifact (architecture
  `10-...:115`) as a later follow-on once the substrate supports a non-vacuous run." (b) **Two
  follow-on issues to file:** the literal-Jepsen credibility artifact (Option A, blocked on
  substrate fixes — a fault-surviving gateway client instead of per-process CLI; concurrency
  the redb single-writer can serialize), and the **missing-fragment detection product gap**
  (production scrub `continue`s on `Ok(None)` and the read path enqueues only present-but-
  corrupt — see Production reach).
- **Surfaces:** data   (the custodian repair/reconstruction path + xtask + CI; no GUI)
- **Difficulty:** high   (net-new infrastructure spanning `xtask` dispatch + a new in-repo
  scenario test driving the production reconcile path under partitions/crashes against a live
  containerized cluster + a new privileged CI workflow; the harness must correctly drive and
  assert over the production repair path — wide cross-cutting reach a reviewer must hold in
  view. Rated up per the safe default.)

- **Scope:** Build the Tier-1 consistency-over-repair leg as an in-repo Rust scenario
  (Option B): (1) an **in-repo Rust scenario test** that stands up a real containerized
  Wyrd/D-server cluster, injects **partitions and crashes mid-repair**, drives the
  **production** custodian repair/reconstruction path (`custodian::reconcile_step` →
  `reconstruction::reconcile`), and asserts the ADR-0015 consistency contract over the repair
  path (read-after-commit; no stale/torn reads; repair neither lost nor duplicated;
  commit-point-atomic — a crash mid-repair leaves collectable garbage, never corruption or
  duplicate placement); modelled on the merged `tier2_kill_reconstruct.rs`; (2) **rewire
  `xtask::run_jepsen`** to dispatch to that in-repo scenario instead of the nonexistent
  external `WYRD_TIER1_JEPSEN_CMD` shell-out (mirror the `run_disk_faults`/`run_kill_reconstruct`
  in-repo dispatch shape); (3) a new **`tier1-jepsen.yml`** privileged CI job (nightly schedule
  + `workflow_dispatch`, opted in with `WYRD_TIER1=1`), modelled on
  `tier1-disk-faults.yml`/`tier2-kill-reconstruct.yml`, on **its own non-colliding cron slot**
  (the existing staggered nightly jobs take 03:00/04:00/05:00 UTC "to avoid runner contention"
  — use e.g. 02:00 or 06:00 UTC), kept out of the unprivileged container-free `cargo xtask ci`
  (ADR-0016). / **out of scope:** literal Clojure/Jepsen/Elle (re-filed as a follow-on
  credibility-artifact issue — Ordering note); fixing the missing-fragment **detection**
  product gap (sanctioned test-enqueue stand-in per the merged #196 precedent — see Production
  reach — with the gap re-filed as a follow-on); changing the production repair/reconstruction
  code (that is #251, already merged); the disk-fault leg (#195) and kill-reconstruct leg
  (#196), already built and merged; adding any new toolchain to the unprivileged `cargo xtask
  ci` merge gate; making `tier1-jepsen.yml` a required PR/merge-gate status check (it is
  post-merge, nightly + on-demand, like its siblings).

- **Repro instruction:** On getwyrd/wyrd @ main: `run_jepsen` at `xtask/src/faults.rs:170`
  calls `execute("Tier-1 Jepsen", plan, "WYRD_TIER1_JEPSEN_CMD")` — opting in (`WYRD_TIER1=1`)
  with any fabricated `WYRD_TIER1_JEPSEN_CMD` runs an external command, but **no in-repo
  consistency harness exists** (no Tier-1 Jepsen/consistency scenario test, no
  `tier1-jepsen.yml` workflow — `git -C ../wyrd ls-files` / `cat-file -e` confirm both absent
  on origin/main). Pre-change: `cargo xtask jepsen` can only ever shell out to a
  nonexistent/foreign command; the leg is inert. Post-change: an in-repo Rust scenario exists;
  `run_jepsen` dispatches to it; the `tier1-jepsen.yml` nightly/dispatch job runs it
  privileged against a live cluster.

- **Test file:** `xtask/src/faults.rs` (the `#[cfg(test)] mod tests` block) — the Check-time
  flippable regression: assert `run_jepsen`'s dispatch now targets the **in-repo** Tier-1
  scenario invocation (the `cargo test --test <scenario> -- --ignored` shape its siblings use)
  rather than reading the external `WYRD_TIER1_JEPSEN_CMD` env command (red pre-change:
  dispatch routes to the env-supplied external command; green post-change: dispatch routes to
  the in-repo scenario). Plus the net-new scenario itself (ILLUSTRATIVE path, e.g.
  `crates/chunkstore-grpc/tests/tier1_jepsen_consistency.rs`, sibling to
  `tier2_kill_reconstruct.rs`): its `#[ignore]`d body compiles and type-checks under `cargo
  xtask ci` (the seam is exercised at Check by compilation, like the sibling), and its live
  consistency run is exercised off-Check by `tier1-jepsen.yml` (see Verification posture).

- **Verification posture:** DECLARED — net-new + DEFERRED/off-Check, stated up front so Check
  lands it as a pre-declared sign-off item, not a surprise NEEDS-HUMAN. (i) **Built AND
  exercised at Check** (`cargo xtask ci`): the Rust `run_jepsen` **dispatch rewire** + its
  opt-in/gating logic (unit-tested in `xtask/src/faults.rs`), AND — unlike the rejected
  Option-A Clojure harness, which the pure-Rust gate could not build — the scenario's **Rust
  harness code itself compiles and type-checks under the merge gate** (the `#[ignore]`d body
  is built, so an API regression that stubbed `reconcile_step`/`reconstruction::reconcile`
  fails `cargo xtask ci`, exactly as `tier2_kill_reconstruct.rs` already does). Where the
  scenario factors out a host-independent consistency oracle / assertion helper, unit-test it
  at Check over a hand-authored history. (ii) **NET-NEW, born-at-tier** (red = criterion-
  ABSENCE, no prior failing assertion to flip): the in-repo scenario and the `tier1-jepsen.yml`
  workflow are new; the flippable behavioral red is the dispatch-routing test above. (iii)
  **DEFERRED / off-Check**: the live consistency run (real containerized cluster + partitions
  + crashes mid-repair, driving the production reconcile path) is observable **only** in the
  privileged `tier1-jepsen.yml` job (containers are excluded from the unprivileged merge gate,
  ADR-0016). WHO confirms the deferred green: the **maintainer (Eduard Ralph)** reviewing the
  first on-demand `tier1-jepsen.yml` (`workflow_dispatch`) run. FORCING-FUNCTION honesty
  (#146): the scenario MUST capture a **demonstrated red** — a negative-control / planted-
  anomaly assertion proving the consistency oracle is load-bearing (e.g. that a withheld
  repair, a stale/torn read, or a duplicated re-placement is actually CAUGHT), per ADR-0009
  "a bug-finding run is promoted as a permanent regression" — rather than resting green on
  non-existence.

- **Production reach:** DECLARED. The repair **trigger** is a **sanctioned test stand-in**,
  following the merged #196 precedent verbatim: `tier2_kill_reconstruct.rs:551` calls
  `repair::enqueue_repair(&meta, CHUNK, "tier2-test")` with the comment *"as a health-check
  producer (scrub or read path) would after detecting that server 0 is no longer serving its
  fragment."* This is necessary because **no production path enqueues repair for a simply-
  missing fragment** — verified on origin/main: production scrub `continue`s on `Ok(None)`
  (`crates/custodian/src/scrub.rs`, the "loss for GC/reconstruction to notice, not a checksum
  finding" arm) and a killed server cannot be `list_fragments()`'d at all; the read path
  enqueues only present-but-corrupt fragments (`crates/core/src/read.rs`, `corrupt.push`). (a)
  WHAT honours the seam now: the scenario explicitly enqueues the repair obligation (the
  health-check-producer stand-in) and then drives the **production** `reconcile_step` →
  `reconstruction::reconcile` over it against the live cluster — the reconstruction path is
  genuinely traversed, not stubbed; (b) WHERE production wiring lands: a **follow-on missing-
  fragment-detection product fix** (Ordering note) — until then the enqueue stand-in is the
  accepted bridge, exactly as #196 accepted it; (c) the stand-in is **load-bearing**: the
  scenario must assert reconstruction actually FIRED (the killed fragment is rebuilt and
  re-placed in a distinct failure domain, the placement no longer references the dead server),
  not merely that `reconcile_step` returned `Ok` — so an empty-queue no-op cannot pass (the
  iter-2 advisory).

- **Citations expected:** Do must cite path:line on the target branch (main) for every change
  — the `run_jepsen` dispatch edit in `xtask/src/faults.rs`, and the new scenario + workflow
  files.

- **Prior-art check (triage cycles):** Searched by file path across merged history and
  open/closed work. `xtask/src/faults.rs` history — `0b5fea3` (#195, disk-fault leg) and
  `02983aa` (#196, kill-reconstruct leg) built the two sibling runners in **this same file**;
  `run_jepsen` (`faults.rs:170`) is the only one still stubbed against an external command.
  `crates/chunkstore-grpc/tests/tier2_kill_reconstruct.rs` is the **binding precedent** for
  both the scenario shape (drives production `reconcile_step`/`reconstruction::reconcile`
  against a live containerized cluster, `#[ignore]`d body compiled by the merge gate) AND the
  repair-enqueue stand-in (`:551`). No Tier-1 Jepsen/consistency scenario test and no
  `tier1-jepsen.yml` exist on origin/main (`git ls-files` / `cat-file -e` confirm absent).
  `tier1-disk-faults.yml` (#195) and `tier2-kill-reconstruct.yml` (#196) are the workflow
  models to mirror. Iterations 1-5 of THIS bundle pursued Option A (literal Clojure/Jepsen/
  Elle) and were all rejected (preserved in `iteration-v1/`..`iteration-v5/`); this brief
  deliberately changes the structural approach to Option B. No open or closed PR builds the
  Tier-1 consistency leg. Not a duplicate — the siblings are the **pattern precedent**, not
  the same fix.

- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.
</content>

## Iteration 6 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected on C2 (red pre-fix) and the codex faults.rs:426 note — the same defect: the dispatch regression test is tautological. It only asserts `jepsen_required_tool() == "docker"`, which stays green even if `run_jepsen` still shelled out to the external `WYRD_TIER1_JEPSEN_CMD` command. `run-verify.sh` confirms the test PASSES without the fix — no behavioral red, the #146 "deferred ≠ unbuilt" trap. What to change next: add a behavioral routing test that exercises `run_jepsen`'s `Plan::Run` branch and proves the command path reaches `run_jepsen_consistency_test` (the in-repo `cargo test --ignored` dispatch) rather than the external env-var shell-out. It must be genuinely red pre-fix (fail when the dispatch still routes to the external command) and green post-fix, so the per-fix red→green (C4-verify) holds. Not in dispute / accepted this round (carry forward, do not re-litigate): T3 Runtime and Validation accepted on the deferred-posture (first `tier1-jepsen.yml` workflow_dispatch run to be verified green once checked in); T4 Contribution and T5 Judgment confirmed fine (Option B substance + the `enqueue_repair` #196 stand-in).
- Failing gate: C4 per-fix red->green: this patch's test red pre-fix, green post-fix (advisory) — run-verify.sh: FAIL — the test PASSES without the fix, so it does not catch the bug (no red).
- Full previous attempt preserved in `iteration-v6/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 7 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected: C2 FAIL recurs as the iteration-6 tautology, restructured. The flippable "red" is a compile failure from deleting the net-new xtask/src/jepsen.rs metadata module — it is not bound to the production dispatch. run_jepsen_consistency_test (faults.rs, patch.diff:1021-1031) hand-types its cargo-test args instead of consuming jepsen::consistency_test_cargo_args(); the two arg lists are duplicated, not single-sourced. As a result jepsen_routing.rs only asserts over constants the runner never reads, faults.rs is a binary module outside the lib and so is unreachable by the integration test, and reverting the faults.rs rewire back to the old WYRD_TIER1_JEPSEN_CMD shell-out leaves every routing test green. The dispatch the brief requires be "unit-tested" (brief.md:38-42) has no flippable test bound to it. What to change next (carry-forward): - Make run_jepsen_consistency_test consume jepsen::consistency_test_cargo_args() as the single source of truth: delete the hand-typed arg list in faults.rs and build the Command from the shared function, so reverting the rewire breaks the test. - Then bind the test to the production path — assert over the runner-built command, or drive run_jepsen's Plan::Run branch directly — so the red fails iff the dispatch regresses to the external shell-out. Compile-seam-over-scaffolding is not sufficient; this was the iter-6 rejection class.
- Full previous attempt preserved in `iteration-v7/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 8 — carry-forward (from the previous attempt)
- Sign-off rationale: Why rejected: three iterations (6, 7, 8) have hit the same wall. The brief asks for a Check test that flips when run_jepsen's dispatch regresses to the external WYRD_TIER1_JEPSEN_CMD shell-out, but the module's structure has nowhere to host such a test. The routing decision ("which harness to run") is welded inside the private, docker-spawning run_jepsen_scenario / run_jepsen_consistency_test (xtask/src/faults.rs); the only pure seams the design exposes are plan() (the gating decision) plus the extracted consistency_test_cargo_args() constant. A test over that constant does not prove the dispatch actually uses it, and C4-verify is satisfied by a net-new-module compile-seam red (revert -> jepsen.rs gone -> use fails to compile), so neither the gate nor the available test surface ever forces a behavioral flip. This is a design gap, not a builder miss: re-running the same brief will reproduce the same shape. #250 is the first tier whose harness actually changes (external shell-out -> in-repo scenario), so "route to X not Y" is a meaningful claim for the first time, and the inherited sibling structure has no slot for it. Re-plan the seam, not just the test: 1. Routing seam (primary). Restructure run_jepsen so the routing decision is a returnable value (e.g. the cargo invocation, or a Plan-like enum) that a Check-time unit test can assert routes to the in-repo scenario rather than the external env command, with the docker spawn placed downstream of that value. The flippable regression must fail iff the dispatch regresses to the shell-out, not merely because a net-new module was deleted. 2. Partition injection (scope decision). Tier-1 MUST inject a real network partition, not only a container kill. The brief and tier1-jepsen.yml promise "partitions and crashes" (0005:408); the current scenario covers crash/kill-and-repair only. Specify the partition fault in the brief so the harness delivers what the milestone names. 3. Dual oracle (cleanup). The lib ConsistencyEvent oracle that the Check tests cover (xtask/src/jepsen.rs check_read_after_commit / check_no_duplicate_placement) is NOT the oracle the live scenario uses (the scenario's own assert_* helpers), so Check coverage exercises an oracle the real run never touches. Unify them or drop the decorative one. Also fix that check_read_after_commit treats a committed value becoming unreadable (ReadObserved { write_id: None } after a WriteCommitted) as valid, which weakens the ADR-0015 read-after-commit guarantee.
- Full previous attempt preserved in `iteration-v8/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
