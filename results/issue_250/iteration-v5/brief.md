# Brief — issue 250 / tier1-jepsen-consistency-harness

> The Plan artifact (docs 02 §PLAN). Human-authored. Do reads ONLY this file.
> The field labels below are parsed by the driver — keep the `- **Label:** value`
> shape. The success criterion is the load-bearing field: it is the sentence
> Check tests "did this work" against.

- **Slug:** tier1-jepsen-consistency-harness
- **Planning artifact:** proposal 0005 (M3 — Custodians) §13.2, `0005:408` ("**Jepsen**
  consistency runs over the repair path"); crate touch-point `0005:437-438`; PR-sequence
  slice 8 `0005:541-545`. Architecture framing: `docs/design/architecture/10-quality-risks-glossary.md:115`
  ("a clean public Jepsen result is itself a credibility artifact, like the conformance
  vectors"). Consistency contract: ADR-0015. Tier model: ADR-0009 (Tier-2 Jepsen = real
  networked backends under containers; a bug-finding run promoted as a permanent
  regression) and ADR-0016 (privileged tiers kept out of the unprivileged `cargo xtask ci`).
- **Defect:** The Tier-1 **Jepsen** consistency leg of proposal 0005 §13.2 (`0005:408`) was
  never built. `xtask/src/faults.rs::run_jepsen` (`xtask/src/faults.rs:170`, getwyrd/wyrd@main)
  is **inert dispatch scaffolding**: it gates on `WYRD_TIER1=1` and shells out via
  `execute(..., "WYRD_TIER1_JEPSEN_CMD")` to an externally-supplied command that **does not
  exist anywhere in-repo**; only the opt-in gating decision (`plan()`) is unit-tested. There
  is no real harness asserting consistency over the custodian repair/reconstruction path
  under partitions and crashes, and no privileged CI job to run one. This is the #146
  "deferred ≠ unbuilt" gap: a tier waved through as "deferred" but never actually built —
  the same gap #195 (disk-fault leg) and #196 (kill-reconstruct leg) closed for their legs.
- **Success criterion:** **DECISION (the human/maintainer chose Option A): build the genuine
  Jepsen framework harness (Clojure/`lein` + Elle checker), accepting the JVM/Clojure
  toolchain footprint in a pure-Rust workspace** — the in-repo Jepsen-style Rust check
  (Option B) was explicitly NOT chosen, because the architecture wants the real, public
  Jepsen credibility artifact. BINDING and **demonstrable at Check (C4-verify, the patch
  applied in isolation)**: (1) `run_jepsen` no longer shells out to the nonexistent external
  `WYRD_TIER1_JEPSEN_CMD`, but **dispatches to the in-repo Jepsen harness** (mirroring how
  `run_disk_faults` dispatches to the in-repo `tier1_disk_faults` scenario at
  `faults.rs:118-165`), and that dispatch wiring + its opt-in gating is unit-tested inside
  `cargo xtask ci`; (2) the in-repo Jepsen suite exists as real, buildable harness code — a
  Clojure/`lein` project that wires Jepsen's nemesis (partitions + crashes) and the Elle
  checker against a containerized Wyrd cluster and drives the **production** custodian
  repair/reconstruction path; (3) a new privileged `tier1-jepsen.yml` CI job (nightly schedule
  + `workflow_dispatch`, `WYRD_TIER1=1`) runs it, kept OUT of the unprivileged container-free
  `cargo xtask ci` (ADR-0016). DEFERRED / off-Check supplementary evidence (NOT the Check
  gate — see Verification posture): the suite running **green** — no stale or torn reads
  (ADR-0015), repair neither lost nor duplicated (commit-point-atomic; a crash mid-repair
  leaves collectable garbage, never corruption or duplicate placement, `0005:277`,
  `0005:385-389`) — confirmed in the `tier1-jepsen.yml` run. BINDING parts: the dispatch
  rewire, the harness existing as real Jepsen+Elle code, and the privileged job. ILLUSTRATIVE:
  the exact Clojure project layout, the precise Elle consistency model invoked, and the cron
  minute.
- **Invariant to restore:** An **off-Check verification tier must be a real, built, and
  exercised harness — not inert dispatch scaffolding that shells out to a command that does
  not exist in-repo** ("deferred ≠ unbuilt"). A tier that only decides *whether* to run, with
  nothing in-repo to run, has not been built. Source: the #146 verification-posture forcing
  function (reproduced verbatim in `templates/brief.md.tpl` "Verification posture" and in this
  issue's DoD) — internal project rule (Tier C); corroborated by ADR-0009 (the Jepsen/Tier-2
  leg is *real* fault injection whose bug-finding run is promoted to a permanent regression)
  and the established in-repo precedent of its two sibling legs (`run_disk_faults` →
  `tier1_disk_faults`, #195; `run_kill_reconstruct` → `tier2_kill_reconstruct`, #196).
  SELF-TEST: could Do satisfy this by guarding a single module — e.g. only re-pointing the
  env var or adding another `plan()` branch in `faults.rs`? **No** — with no in-repo harness
  and no privileged job there is nothing to dispatch to, so a dispatch-glue-only change
  visibly fails the invariant.
- **Repo + branch target:** getwyrd/wyrd @ main   (per INTEGRATION §2 — Wyrd targets `main`;
  no maintenance branches)
- **Depends on:**
- **Depends on (merged):**
- **Conflicts with:**
- **Ordering note:** Last of the #195 split family. Its two sibling Tier-1/2 legs are
  **already merged on origin/main** — `run_disk_faults`/`run_tier1_scenario` (#195, commit
  `0b5fea3`) and `run_kill_reconstruct` (#196, commit `02983aa`) are present in
  `xtask/src/faults.rs`; this issue is the lone remaining stub (`run_jepsen`). The production
  reconstruction read-around fix (#251) — the behaviour the suite asserts — is independently
  merged. No build-on dependency and no co-scheduling conflict: this touches only `run_jepsen`
  in `faults.rs` (the sibling runners are merged, not concurrently edited) plus net-new files
  (`tier1-jepsen.yml`, the harness directory). **Pre-declared sign-off item (not blocking the
  brief):** Option A introduces a new external toolchain — JVM + Clojure + Leiningen (`lein`)
  + Jepsen + Elle. These are not Cargo crates (so not `deny.toml`/`cargo-deny`-gated), but
  they are a new dependency set; per INTEGRATION §4 the reviewer will emit NEEDS-HUMAN, and
  the maintainer should weigh at sign-off whether a short ADR recording the non-Rust
  test-toolchain decision is warranted (proposal 0005 already accepts "Jepsen" in principle,
  so this is the *how*, not a new decision).
- **Surfaces:** data   (the custodian repair/reconstruction path + xtask + CI; no GUI)
- **Difficulty:** high   (net-new infrastructure spanning `xtask` dispatch + a new in-repo
  Jepsen suite + a new privileged CI workflow + a new external toolchain; the harness must
  correctly drive and assert over the production repair path — wide cross-cutting reach a
  reviewer must hold in view. Rated up per the safe default.)
- **Scope:** Build the Tier-1 Jepsen consistency leg as the genuine Jepsen framework
  (Option A): (1) an **in-repo Jepsen suite** (Clojure/`lein`) that stands up a containerized
  Wyrd cluster, uses Jepsen's nemesis to inject partitions and crashes **mid-repair**, drives
  the production custodian repair/reconstruction path, records a history, and uses the **Elle**
  checker to assert consistency over the repair path; (2) **rewire `xtask::run_jepsen`** to
  dispatch to that in-repo harness instead of the nonexistent external `WYRD_TIER1_JEPSEN_CMD`
  shell-out (mirror `run_disk_faults`'s in-repo dispatch shape); (3) a new
  **`tier1-jepsen.yml`** privileged CI job (nightly schedule + `workflow_dispatch`, opted in
  with `WYRD_TIER1=1`), modelled on `tier1-disk-faults.yml`/`tier2-kill-reconstruct.yml`, on
  **its own non-colliding cron slot** (03:00/04:00/05:00 UTC are taken by the existing
  staggered nightly jobs "to avoid runner contention" — use e.g. 02:00 or 06:00 UTC), kept out
  of the unprivileged container-free `cargo xtask ci` (ADR-0016). / **out of scope:** changing
  the production repair/reconstruction code (that is #251, already merged); the disk-fault leg
  (#195) and kill-reconstruct leg (#196), already built and merged; adding the Jepsen toolchain
  to the unprivileged `cargo xtask ci` merge gate; reimplementing a consistency checker (use
  Elle); making `tier1-jepsen.yml` a required PR/merge-gate status check (it is post-merge,
  nightly + on-demand, like its siblings).
- **Repro instruction:** On getwyrd/wyrd @ main: `run_jepsen` at `xtask/src/faults.rs:170`
  calls `execute("Tier-1 Jepsen", plan, "WYRD_TIER1_JEPSEN_CMD")` — opting in
  (`WYRD_TIER1=1`) with any fabricated `WYRD_TIER1_JEPSEN_CMD` runs an external command, but
  **no Jepsen harness exists in-repo** (no `jepsen/` project, no `tier1-jepsen.yml` workflow —
  `git -C ../wyrd ls-files` / `cat-file -e` confirm both absent on origin/main). Pre-change:
  `cargo xtask jepsen` can only ever shell out to a nonexistent/foreign command; the leg is
  inert. Post-change: an in-repo Jepsen suite exists; `run_jepsen` dispatches to it; the
  `tier1-jepsen.yml` nightly/dispatch job runs it privileged.
- **Test file:** `xtask/src/faults.rs` (the `#[cfg(test)] mod tests` block) — the Check-time
  flippable regression: assert `run_jepsen`'s dispatch now targets the **in-repo** Jepsen
  harness invocation rather than reading the external `WYRD_TIER1_JEPSEN_CMD` env command
  (red pre-change: dispatch routes to the env-supplied external command; green post-change:
  dispatch routes to the in-repo harness). Plus the deferred-tier exercise: the Jepsen suite's
  own **checker self-test** (Clojure, in the harness directory) — feed Elle a known-anomalous
  history and assert it is flagged — run by `lein test` in the `tier1-jepsen.yml` job (see
  Verification posture).
- **Verification posture:** DECLARED — net-new + DEFERRED/off-Check, an accepted Option-A
  consequence (state it up front so Check lands it as a pre-declared sign-off item, not a
  surprise NEEDS-HUMAN). (i) **Built AND exercised at Check** (`cargo xtask ci`): only the
  Rust `run_jepsen` **dispatch rewire** + its opt-in/gating logic, unit-tested in
  `xtask/src/faults.rs`. (ii) **NET-NEW, born-at-tier** (red = criterion-ABSENCE, no prior
  failing assertion to flip): the in-repo Jepsen suite and the `tier1-jepsen.yml` workflow are
  new. (iii) **DEFERRED / off-Check** — because Option A's harness is **Clojure**, it is NOT
  built or run by the pure-Rust, container-free merge gate (ADR-0016); the actual Jepsen
  consistency run (live cluster + nemesis + Elle) is observable **only** in the privileged
  `tier1-jepsen.yml` job. **This is the explicitly accepted Option-A tradeoff: the harness
  substance cannot be exercised by the Rust Check gate** (the maintainer chose the real Jepsen
  artifact over the Check-verifiable in-repo Rust check). WHO confirms the deferred green: the
  **maintainer (Eduard Ralph)** reviewing the first on-demand `tier1-jepsen.yml`
  (`workflow_dispatch`) run. FORCING-FUNCTION honesty (#146): to honour "deferred ≠ unbuilt"
  as far as the toolchain allows, the suite MUST ship a **checker self-test** (a planted-
  anomaly history Elle must flag, runnable via `lein test`) so the deferred deliverable is
  exercised by SOMETHING, and Do should capture a **demonstrated red** — the harness catching
  a planted anomaly — in a job run rather than resting green on non-existence; a bug-finding
  run is promoted as a permanent regression (ADR-0009).
- **Production reach:** The Jepsen suite drives the **production** custodian
  repair/reconstruction path against a **live containerized cluster** (no in-process
  stand-in) — but only in the off-Check `tier1-jepsen.yml` job. At Check the production path
  is **not** traversed (the suite does not run in `cargo xtask ci`); only the Rust dispatch
  wiring is exercised. This note records that the live-path traversal is deferred to the
  nightly/dispatch job — the accepted Option-A posture above — not a hidden test-double seam.
- **Citations expected:** Do must cite path:line on the target branch (main) for every change
  — the `run_jepsen` dispatch edit in `xtask/src/faults.rs`, and the new harness + workflow
  files.
- **Prior-art check (triage cycles):** Searched by file path across merged history and open/
  closed work. `xtask/src/faults.rs` history — `0b5fea3` (#195, the disk-fault leg) and
  `02983aa` (#196, the kill-reconstruct leg) built the two sibling Tier-1/2 runners in **this
  same file**; `run_jepsen` is the only one still stubbed against an external command. No
  `jepsen` harness directory and no `tier1-jepsen.yml` exist on origin/main (`git ls-files` /
  `cat-file -e` confirm absent). `tier1-disk-faults.yml` (#195) and `tier2-kill-reconstruct.yml`
  (#196) are the workflow models to mirror. No open or closed PR builds the Jepsen leg. Not a
  duplicate — the siblings are the **pattern precedent**, not the same fix.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.
</content>

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected: the current harness is built against an API surface the product does not have, and would test nothing on its first run. Rebuild the Jepsen leg addressing the codex advisory findings (issue_250): 1. Read primitive missing — the read path shells `wyrd ls --prefix`, but the CLI (crates/server/src/cli.rs:56) only dispatches put/get/d-server/demo. There is no list/prefix-enumeration command. Either use a read strategy built on put/get with client-tracked keys, or add a list op to the product first; the list-append/Elle model currently has no backing observation primitive. 2. Port mismatch — tier1-jepsen.yml hardcodes host ports 50051-50055, but the reused crates/chunkstore-grpc/tests/docker-compose.yml publishes ephemeral host ports under `--scale dserver=5` (resolve via `docker compose port --index`). As written the client dials closed ports. 3. Repair path never driven — the workload only does `wyrd put` + read-listing; nothing invokes the custodian repair/reconstruction loop, which is the core brief requirement. A green run would test gateway writes, not the production repair path. 4. Nemesis not wired to the cluster — the crash nemesis uses Jepsen remote-control (pkill / `wyrd d-server &`) against Jepsen nodes, but the cluster is started via Docker Compose with no node/container targets passed. The nemesis does not kill/restart the actual compose replicas under test. Also revisit the hardcoded `--bind 0.0.0.0:50051` for every killed node. 5. Inverted self-test — jepsen/test/wyrd/checker_test.clj:59 compares (:valid? result) to the keyword :valid; Jepsen results use boolean :valid?, so the "consistent" control fails even when Elle accepts the history. Fix so the self-test actually demonstrates a planted-anomaly catch and a clean pass. Toolchain posture (JVM/Clojure/lein/Jepsen/Elle, project.clj) is acceptable as the pre-declared Option-A footprint; not the reason for rejection.
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected: the harness is mechanically wired but still has substantive gaps (per codex advisory) that would make the first live run vacuous — the same "tests nothing on its first run" failure class as iteration-1, now in the Clojure/harness logic the gates don't exercise. Resolve all three before re-submitting: 1. jepsen/src/wyrd/jepsen.clj:357 — the nemesis only injects :kill/:heal. There is NO network-partition nemesis. The brief requires a "partitions + crashes" Jepsen leg. Add a real partition nemesis to the generator. 2. crates/chunkstore-grpc/tests/jepsen_custodian_step.rs:229 — the custodian step treats any successful reconcile_step (including Reconciled::Satisfied) as a pass. Combined with the read path only enqueuing repair for present-but-corrupt/ misplaced fragments (crates/core/src/read.rs:189) and NOT for the killed server's missing fragment, the step can run against an empty repair queue and never exercise reconstruction. Make the step assert that reconstruction actually fired (non-empty repair queue / fragment rebuilt), not just that reconcile_step returned Ok. 3. jepsen/src/wyrd/jepsen.clj:237,246 — failed `wyrd get` calls are filtered into a shortened read value but the transaction is still returned as :ok (line 246). This records availability failures as successful list observations, which can create false Elle anomalies or mask that the read did not observe the full tracked list. A failed/partial read must not be recorded as an :ok full observation. Not re-litigated here: the API bindings, dispatch rewire, and red->green gate all pass — the rebuild's structure is sound; the gaps are in the harness substance. The T5/ADR toolchain question (§6) is unresolved but secondary to the above; revisit at the next sign-off once the harness actually exercises the repair path.
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected for the same "vacuous on first run" class that sank iterations 1-2 — this time in the consistency model itself, not the plumbing. Fix before re-submit: - T5 (item 3) — the Elle check is near-vacuous: :concurrency 1 (single redb writer) plus a distinct immutable key per append (jepsen/<slot>/<seq>, read back individually) collapses "list-append" toward "did I read back what I wrote," leaving no concurrent interleaving for Elle to find G1/G2 anomalies. Give the workload genuine concurrency so the consistency checker has real interleaved histories to verify. If single-writer redb is a hard constraint, redesign the workload (or the metadata access) so the campaign is non-vacuous rather than retaining a per-key read-back. - Item 5 (codex advisory, jepsen/src/wyrd/jepsen.clj:495) — list-append uses (rand-int 1000000) as the appended value; Elle's list-append histories require appended elements to be unique within a list. With the current run length and five slots, duplicate values are plausible and can make a healthy run look anomalous or a real anomaly ambiguous. Use the already-allocated per-slot sequence/key as the appended value instead of a random value. Re-submit only with a demonstrated red from a live tier1-jepsen.yml dispatch (the harness catching a planted anomaly over a non-vacuous history) — the Validation/T3 items in §6 still stand for the next Check.
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 4 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected: the dispatch rewire is sound, but the Jepsen/Elle harness substance — the actual deliverable — has structural defects flagged by both reviewers and is unverified. Keep the brief; fix the harness at Do level. Resolve the code advisories: 1. jepsen.clj:317 / :341 — a wrong-value read throws "data integrity violation" and the broad catch records it as :fail, masking the strongest consistency failure (corrupt/stale bytes) as a mere availability failure Elle ignores. Corrupt/stale reads must fail the run (or be recorded as a successful bad observation the checker sees), never be swallowed as :fail. 2. jepsen.clj:188 / :884 (T5, the recurring iter-1/2/3 "vacuous history" class) — the :r op rebuilds list order from the client-side slot-writes atom sorted by :seq, never from Wyrd's stored state. The order is invented client-side, so Elle has no Wyrd-induced interleaving to check (vacuous) AND can synthesize orders it flags even when Wyrd is linearizable (false positive). List order must come from the actual completed/linearized append order observed from Wyrd, not client-side seq allocation. 3. jepsen.clj:453 / :490 — docker network disconnect failures are only logged; the nemesis still returns :info partitioned. A wrong network name then means the required partition fault is silently absent and the run passes without partitioning. Nonzero docker exits must fail the nemesis op / run. Also still open (not blocking the Do fix, but carry forward): C5 — reconstruction is driven by the test-only detect_and_enqueue_missing because the production read path does not enqueue repair for missing (only present-but-corrupt) fragments; confirm this still counts as exercising the production trigger. T3/Validation — the leg has never been run; needs a live tier1-jepsen.yml dispatch with a demonstrated planted-anomaly catch over a non-vacuous history, validated against clean upstream main. T4 — non-Cargo toolchain (JVM/Clojure/Lein/Jepsen/Elle) may warrant a short ADR.
- Full previous attempt preserved in `iteration-v4/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 5 — carry-forward (from the previous attempt)
- Sign-off rationale: Why rejected (issue_250): 5th attempt, same "vacuous history" class that sank iterations 1-4. The cause is at the PLAN level, not the patch: Elle's list-append check presupposes a mutable, linearizable shared register, but Wyrd is an immutable single-write-per-key object store (`wyrd put`/`get` of distinct keys `jepsen/<slot>/<seq>`). The "list" and its order are therefore invented in the Jepsen client (the `slot-writes`/`slot-positions` atoms, `alloc-position!`), so Elle checks the harness's own bookkeeping, not state Wyrd linearized. Each iteration has patched a symptom of this (seq->completion order, catch-Exception swallowing, network nemesis throw, ephemeral ports) without changing the observable — so the class recurs. Handing this to Do again under the same model will very likely reproduce it a 6th time. What to change in the plan (not the patch): - Replace the list-append-over-immutable-store framing with an observable Wyrd actually linearizes. Check properties Wyrd genuinely has — read-after-commit, no torn/stale reads, repair commit-point-atomicity (ADR-0015, `0005:277`, `0005:385-389`) — against the metadata store's versioned commits, rather than list-append over a client-invented list. - Decide C5 at plan: the repair trigger is currently a test-only `detect_and_enqueue_missing`; production read path does not enqueue simply-missing fragments (`crates/core/src/read.rs:189`). Either the missing-fragment detection gap is the real product defect to fix, or the spec must state that the test-injected enqueue is an accepted stand-in — don't leave it ambiguous for a 6th Do. - Address the substrate constraints the codex advisories raise, because they make a live history vacuous even if the model were right: * redb takes an exclusive file lock at open (`crates/server/src/cli.rs:536`, `open_cluster_meta`); each op is a separate `wyrd` process, so concurrent CLI ops fail rather than serialize -> no real concurrency. (The patch's architecture comment claims the opposite; verify before re-planning.) * `wyrd get` builds its fanout by dialing every endpoint and fails on the first unreachable D-server (`crates/server/src/cli.rs:451`), so reads :fail during the exact fault windows -> no under-fault read history. * post-repair unreadable values are caught as :fail and ignored by the checker (`jepsen/src/wyrd/jepsen.clj:390`,`:718`); the post-repair readability assertion needs a hard-fail path. - Reconfirm the Option-A decision in light of the above: if a genuine non-vacuous Jepsen/Elle run is not reachable given Wyrd's immutable-store semantics + the per-process redb lock, the plan should say so and pick the property-based framing rather than re-attempting list-append. What IS solidly verified and need not be rebuilt: the Rust dispatch rewire (`run_jepsen` -> in-repo `lein run test`), its opt-in gating, and the workflow/compose plumbing (`cargo xtask ci` green, red->green confirmed). The miss is the consistency model and the observable, not the wiring.
- Full previous attempt preserved in `iteration-v5/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
