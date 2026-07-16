# Act log — Wyrd PDCA

> Append-only, cross-cycle (docs 02 §ACT). Each entry records which frozen
> bundles an Act review considered, what their records exposed, the concrete
> process deltas applied (each located by a path / rule ID / template field), and
> how the next review will judge whether the delta worked. Act never re-decides a
> contribution's disposition. Newest entries on top.

<!-- Template for a new entry:

# Act review — <date> — cycles considered: <issue_ids>

## What the cycles' records exposed
- <pattern across one or more cycles, citing SUMMARY §6/§7/§10>

## Process deltas
- Spec template: <field added/clarified/removed>            (path)
- Ruleset: <rule added/retired/relaxed/tightened>           (path:line)
- Gates: <check added/promoted/moved>                       (path:line)
- Agent role prompts: <agents/*.md / skill adjustment>      (path:line)

## Follow-ups routed (not process deltas — work handed to an owner)
- Another bug (project/component): filed <tracker> #NNNN    (link)
- Design issue: <name> → dedicated design phase, owner <who>
- Harness/driver issue: this repo's tracker | template feedback upstream  (link)
- Other open Act item: <item> → owner <who>, next step <…>

## How effectiveness will be judged
- The next Do phases should not recreate <specific issue>. Watch the next K cycles.
-->

# Act review — 2026-07-16 — cycles considered: issue_407, issue_408 (new since 07-10) + issue_399, issue_406, issue_469, issue_470 (froze around the 07-10 review without a narrative entry; issue_438's signals were part of the 07-10 FDB-batch ledger sweep)

## What the cycles' records exposed

- **RECURRING & ACTIONABLE — unregistered host prerequisites discovered at sign-off, third
  occurrence of the class; the strongest evidence yet that the upstream forcing function
  (eduralph/pdca-harness#263, filed 07-07, unimplemented) is overdue.** #407 §10: the witnessed
  `WYRD_TIER1=1 cargo xtask metadata-nemesis` run was **blocked at sign-off** on two missing host
  packages — `foundationdb-clients` (libfdb_c / fdb_c.h / fdb.options / fdbcli; a *provisioning*
  miss — the doctor rows for it already exist, added with #438/#492) and `libfaketime` (the skew
  leg's `WYRD_TIER1_SKEW_SO` bind-mount, `deploy/fdb-multi-replica/docker-compose.faketime.yml:44`
  — never registered). #408 §6/§10: `unzip` is a hard preflight failure for the consistency
  runner (`unzip -p` reads the elle-cli version from the jar; elle-cli 0.1.9 has no `--version`),
  was asked for as a doctor row in **that same issue's v4 carry-forward, and still never landed**
  — a within-cycle registration ask slipping through is exactly the failure mode #263's forcing
  function exists to prevent. Prior occurrences: docker/openssl #252-254 (registered reactively,
  getwyrd/wyrd-pdca#96); protoc #365 (registered at the 07-04 review). The instance-owned half is
  a delta this review APPLIES (below); the forcing function stays upstream at #263.
- **The already-routed leaf-sandbox capability classes recurred as predicted — watch items now
  confirmed overdue, no new delta.** Loopback-bind denial (harness#261) again forced C4 to
  provisional in #407 (`list_delete.rs:55`) and #408 (`consistency_observable.rs:62`); Docker-API
  denial (harness#276) is the T3 story of #438/#469/#470 (and #399's NET_ADMIN Jepsen leg);
  the no-network prior-art tax (harness#277) forced T4/T5 NEEDS-HUMAN in every one of the seven.
  Each was read correctly by the reviewer as provisional-not-defect; the fixes are upstream.
- **Working well — record as evidence, no delta.** (a) #406's §6 shows the intended lifecycle of
  sandbox-artifact NEEDS-HUMAN rows: the human re-ran the full Check on a capable box and marked
  each row **CLEARED** inline with the re-run evidence — the class retired properly instead of
  accumulating. (b) #408's adversary ran fully **execution-backed** ("none provisional": re-ran
  the pinned elle-cli jar, reproduced both live verdicts, mutated the history to prove verdict
  sensitivity, byte-diffed the committed report against the runner artifact) — the strongest
  Check in the record, showing the toolchain-provisioning chain (ensure-cargo.sh, #236, doctor
  rows) paying off where the leaf sandbox permits execution. (c) #407's iteration-2/3
  carry-forward defects were each verified fixed on the target by the adversary — the
  iteration loop converging as designed.
- **Fitness-to-purpose (V) NEEDS-HUMAN in all seven — always-human by design** (INTEGRATION.md
  §4); #408's absorbed-fault question ("the materialized fault never touched a single op — is
  this the checked-run-under-failure #329 intends?") is the model of the class: disclosed by the
  artifact itself, decided by the human, follow-up routed to the tracker (below). Structural — no
  delta, consistent with every prior review.

## Process deltas

- **Instance config (this repo): two new `pdca doctor` prerequisite rows** — `unzip (elle-cli
  jar preflight)` (`unzip -v`; the #408 consistency-run preflight hard-fails without it; the
  twice-asked v4 carry-forward, now landed) and `libfaketime (Tier-1 skew leg)`
  (`test -f "${WYRD_TIER1_SKEW_SO:-/usr/lib/x86_64-linux-gnu/faketime/libfaketime.so.1}"`; the
  #407 skew-leg bind-mount, discovered missing at sign-off). Both WARN, group `engine`, same
  posture as the docker/openssl/protoc/fdb rows: only the opt-in off-Check legs need them.
  (`pdca.toml` — after the elle-cli row, ~`pdca.toml:722-751`.) The uncommitted `java` +
  `elle-cli` rows added at the #408 sign-off are recorded here as part of the same registration
  wave and land in the same commit.
- **No spec-template / ruleset / gate / agent-skill delta warranted.** The recurring signals are
  either upstream capability gaps already filed (#261/#276/#277/#278), the unimplemented upstream
  forcing function (#263 — reinforced by the #408 unzip evidence, not re-filed), or always-human
  classes working as designed. A forced change would be worse than none.

## Follow-ups routed (not process deltas — work handed to an owner)

- **Tracker (Wyrd, #408 §10 + adversary finding) — witnessed consistency-run under a
  client-visible / quorum-costing fault.** The #408 report's fault was fully absorbed by FDB's
  quorum (720/720 ops ok, `info: 0`), so the checked histories are observationally identical to
  healthy-cluster ones; the follow-up run must let the fault escape the tolerance envelope so the
  history itself carries it (`info > 0`). Post-0.1-alpha, alongside the real-hardware /
  long-duration campaign. → **FILED getwyrd/wyrd#573** (2026-07-16).
- **Upstream reinforcement (no new issue) — harness#263.** The #408 unzip case (a doctor-row ask
  carried forward *within* the same issue's iterations and still unlanded at sign-off) is direct
  evidence the registration must be a forcing function, not a convention. Recorded here; fold the
  case into #263 when it is implemented.
- **No routing needed for #399/#406/#438/#469/#470** — every §6/§10 signal maps onto an
  already-filed item (#261 loopback, #276 Docker API, #277 gh network, #278 infra-empty leaf,
  getwyrd/wyrd#442 FDB go/no-go).

## Still open (carried)

- Upstream harness: **#261** (loopback bind), **#276** (Docker API), **#277** (gh network),
  **#278** (infra-empty advisory), **#262** (base parser), **#263** (doctor forcing function —
  reinforced this review) — all recurred or stayed load-bearing this batch; confirm progress
  next review.
- **issue_115** (ACCEPTED, 2026-06-20) and **issue_153** (discontinued, 2026-06-21) remain absent
  from the frozen index → carried.
- Prior reviews' carried items unchanged: getwyrd/wyrd#426 (shared-conformance-pin governance),
  #442 (FDB go/no-go — #407/#408 build directly on its battery; watch it record before the FDB
  default flips), #367 (first-deployment gate convergence), the #455/#256/#364/#365 tracker
  follow-ups.

## How effectiveness will be judged

- The next off-Check witnessed run (nemesis, consistency-run, or the #442 battery) should find
  its host prerequisites **preflighted by `pdca doctor`**, not discovered at sign-off — a fourth
  occurrence of the class after these rows land means the instance-side registration is not
  enough and #263 must be prioritized.
- **getwyrd/wyrd#573** should appear scheduled (post-0.1-alpha) at the next review; the #408
  report's absorbed-fault caveat stays auditable through it.
- The upstream capability items (#261/#276/#277) recurring for a third consecutive review would
  make them the dominant standing tax on Check — escalate priority upstream if so.

# Act review — 2026-07-15 — cycles considered: issue_398, issue_399, issue_406, issue_430, issue_431, issue_469, issue_470, issue_490, issue_554

> The nine frozen cycles the ledger had not yet counted (58 → 67): #469/#470 froze 2026-07-10
> alongside the FDB batch but were not in that review's scope; #399/#406 froze 07-08; #398/#430/
> #431/#490/#554 froze 07-15 and are the first batch run on **harness v0.54.0** (bumped `3f1e643`,
> 07-14; release cut 07-12 *after* upstream #277/#278/#291/#293 closed). No contribution
> disposition is re-decided.

## What the cycles' records exposed

- **DOMINANT & ACTIONABLE — the codex-reviewer sandbox denials persist *after* their upstream
  fixes closed, because this instance never adopted the new opt-in knob (4×: #430, #431, #490,
  #554 — all on v0.54.0).** In all four, the reviewer's independent `cargo xtask ci` rerun stops
  at a loopback-bind `PermissionDenied` (e.g. `list_delete_over_grpc`; #431 self-nominates it in
  §10), and `api.github.com` is unreachable, forcing the T4 closed/rejected-PR prior-art check to
  NEEDS-HUMAN — the exact symptoms of harness#261 (closed 07-09) and harness#277 (closed 07-12).
  Root cause traced this review: the harness#291 fix ships the grant as an **opt-in config key**
  — `[leaves.sandbox] network_access = true` (codex's denial is its seccomp/network layer; no
  path grant fixes it) — and this repo's `pdca.toml` carried it **commented out**. So this is an
  **instance-config adoption gap**, not an upstream regression: the one finding that meets the
  process-delta bar this review (delta applied below). Corroborating evidence: #406's original
  loopback NEEDS-HUMAN rows were all **CLEARED** by a full re-Check on the human's box (loopback
  works outside the sandbox), and #399/#469/#470's Docker/gh denials all *pre-date* the fixes
  (froze 07-08/07-10) — the earlier routes were right, the fixes just hadn't been switched on here.
- **NEW harness bug (evidence-integrity class) — lane worktree not reset between iterations, so a
  *gating* C4-ci green attested the PREVIOUS iteration's code (#554 §10).** `wyrd.pdca-wt-l1` kept
  iteration-4 state into iteration 5 (likely trigger: host standby mid-process); the reviewer
  caught it independently ("`$PDCA_TARGET` contains the preceding iteration … `patch.diff` applies
  cleanly to its `HEAD`", `check-review.md`). The strongest failure class a deterministic gate can
  have — a green certifying a tree that doesn't match `patch.diff`. Harness machinery → routed
  upstream (below).
- **RECURRING harness bug — the harness's own disk footprint false-reds gating gates (2×: #430,
  #554).** #430's gating C4-ci red was `cli_roundtrip.rs:43` panicking on `Disk quota exceeded
  (os error 122)`; #554's C4 gate failed at `dst_commit.rs` for quota headroom. The #554 sign-off
  diagnosed the cause: accumulated `../wyrd-*/target` dirs + stale lane/verify worktrees **>200 G**
  exhaust the user quota. Echoes #252 §10's "lazy worktree cleanup" (2026-07-04), whose footprint
  axis was never addressed. Quota exhaustion mid-`cargo test` yields an arbitrary failing test
  name, so the red is misattributed to the patch until a human traces it — twice in one batch.
  Harness machinery → routed upstream (below).
- **Working as designed — no delta.** #398 is the first **likely-close** (no-patch) cycle and the
  shape worked: a single §6 row asking the human to confirm the close disposition, cleanly
  confirmable at sign-off. #406 demonstrates the full-re-Check-at-sign-off path clearing every
  provisional row. #469/#470 carried their Docker/gh gaps as pre-declared T3/T5 items with the
  #442 go/no-go owner already recorded (2026-07-10 review) — nothing new owed.
- **C5 / T5 / V remain NEEDS-HUMAN by design in every cycle** (fitness-to-purpose in all nine;
  the always-human classes, INTEGRATION.md §4). The structural constant — no delta, consistent
  with every prior review.
- **Wyrd code follow-ups from §10, none a process delta (4 items: #490 ×2, #554 ×2)** — routed to
  the tracker below: buffered-PUT lease renewal; `write_new_object` commit-instant contract;
  custodian peer re-dial after degraded boot; identity-keyed fleet uniqueness.

## Process deltas

- **Gates/leaf config (this repo, applied by the human this session): `[leaves.sandbox]
  network_access = true`** (`pdca.toml:582-584`, previously commented out; verified parsing with
  an unquoted boolean and a single table declaration). Opens the codex reviewer leaf's
  socket/network layer per harness#291: loopback binds (the 4× C4-rerun blocker), `api.github.com`
  (the standing T4 prior-art tax, harness#277), and the Docker socket (harness#276/#291).
  Trade-off accepted knowingly: the grant applies to every command in that leaf (no per-domain
  scoping); filesystem confinement is retained; rootless docker remains the preferred hardening.
  The claude-side `unsandboxed_commands` key stays commented — no claude-family leaf currently
  needs a Docker-backed conformance leg.
- **No spec-template / ruleset / agent-skill delta warranted.** The two new harness findings
  (worktree reset, disk footprint) are `src/pdca_harness/**` machinery → upstream; the rest is
  working-as-designed or always-human.

## Follow-ups routed (not process deltas — work handed to an owner)

- **Harness/driver (upstream) — lane worktree not reset between iterations; gating green can
  attest stale code (#554).** Ask: reset/re-populate `$PDCA_WORKTREE` before gates and verify the
  tree matches `patch.diff`, failing CLOSED on mismatch. → **FILED eduralph/pdca-harness#296**
  (2026-07-15, bug) — https://github.com/eduralph/pdca-harness/issues/296
- **Harness/driver (upstream) — prune/bound the harness's worktree/target footprint (#430, #554).**
  Ask: sweep lane/verify worktrees on publish/freeze, GC per-lane `target/` dirs, optional doctor
  row warning near-quota. → **FILED eduralph/pdca-harness#297** (2026-07-15, bug) —
  https://github.com/eduralph/pdca-harness/issues/297
- **Tracker (Wyrd, #490 §10) — buffered `put_object` lease renewal** (a PUT slower than the 30 s
  TTL deterministically Conflicts, cannot succeed on retry; renew on the buffered path or route
  through streaming). → **FILED getwyrd/wyrd#560** (milestone 0.1 Alpha) —
  https://github.com/getwyrd/wyrd/issues/560
- **Tracker (Wyrd, #490 §10) — `write_new_object`/`write_new_object_placed` commit-instant
  contract** (start-of-call `now` makes the present-but-expired guard arm vacuous on the helper
  paths; latent today). → **FILED getwyrd/wyrd#561** —
  https://github.com/getwyrd/wyrd/issues/561
- **Tracker (Wyrd, #554 §10) — custodian never re-dials a peer dropped at a degraded boot** (GC
  pause lasts the process lifetime; re-dial each pass so the run-loop doc's "recovered on the next
  whole-fleet pass" becomes true). → **FILED getwyrd/wyrd#562** (milestone M7) —
  https://github.com/getwyrd/wyrd/issues/562
- **Tracker (Wyrd, #554 §10) — key the fleet-uniqueness refusal on attested identity, not endpoint
  string equality** (endpoint aliasing accepted as a trust assumption at the #554 sign-off; close
  structurally at M5 step-ca). → **FILED getwyrd/wyrd#563** (milestone M5) —
  https://github.com/getwyrd/wyrd/issues/563

## Still open (carried)

- **issue_115** (ACCEPTED, 2026-06-20) and **issue_153** (discontinued/handed off, 2026-06-21)
  remain absent from the frozen index → carried, out of scope.
- Result dirs **issue_250/262/263/264/265/291/367/407** exist under `results/` but are not in the
  frozen Act index → out of scope this review (same handling as prior reviews).
- Upstream closures observed this review: harness **#261, #262, #263, #276, #277, #278, #291,
  #293** all CLOSED — the adoption knob is now switched on here; effectiveness judged below.
- Prior carried items unchanged: getwyrd/wyrd#426 (shared-conformance-pin governance); #442 (FDB
  go/no-go, owns the #438–#441/#470 fitness deferral); #268 ADR-0010 `BlockReadFault` amendment;
  #356 fan-out id-map; the #367 first-deployment-gate convergence (M4 itself merged as
  getwyrd/wyrd#489 per INTEGRATION.md — confirm #367's runbook items landed with it next review).

## How effectiveness will be judged

- **The `network_access` knob is the testable delta:** the next codex-reviewed cycles should stop
  raising loopback-bind C4-rerun provisionals and gh-unreachable T4 prior-art NEEDS-HUMAN rows.
  If either recurs **with the knob active**, that is a genuine upstream defect (a #291-style
  family gap for loopback) — file it with the evidence; do not re-toggle config.
- **harness#296** should make a stale-worktree gating green impossible (fail-closed mismatch);
  watch for any reviewer grounding note of the "`$PDCA_TARGET` contains the preceding iteration"
  form — one more occurrence before the fix confirms severity, any occurrence after it means the
  fix regressed.
- **harness#297** should end quota-exhaustion gate false-reds; a third `os error 122`-class
  gating red (after #430/#554) before the fix lands makes it overdue.
- The four Wyrd follow-ups (#560–#563) should appear as scheduled/closed at their milestones —
  #560 before 0.1 Alpha ships the lease guard to users.

# Act review — 2026-07-10 — cycles considered: issue_439, issue_440, issue_441, issue_468 (FDB batch; issue_477 already recorded 07-07)

## What the cycles' records exposed

- **The dominant §6 recurrence is leaf-sandbox *capability*, not a spec/gate gap (4 of 4:
  #439, #440, #441, #468).** Each FoundationDB slice's live verification is a Docker-backed
  conformance run (`cargo xtask fdb-conformance` → single-node cluster via `docker compose`),
  and each surfaces `T3 Runtime` NEEDS-HUMAN for the *same* reason: the reviewer/adversary
  leaf sandbox denies **Docker API** access. The host is Docker-capable (`docker info` OK,
  CLI + compose present), but the socket is denied inside the sandbox, so the live leg skips
  — #440 "socket permission denied", #441 "Docker CLI/compose installed but Docker API
  permission was denied", #439/#468 same class. This is a direct sibling of the
  already-fixed **#261** (loopback-bind denial): a physical capability the leaf sandbox
  withholds, so the green cannot be earned at Check even when the operator wants it and the
  host can do it.
- **A standing per-bundle prior-art tax from the same sandbox posture (3 of 4: #439, #440,
  #441).** The codex reviewer runs `codex exec --sandbox workspace-write` with no network
  grant, so it cannot reach `api.github.com`; the closed/rejected-PR prior-art check
  (T4/T5) is forced to NEEDS-HUMAN every bundle — a mechanical check handed to the human on
  principle each time. #441 self-nominates this for Act.
- **The adversary leaf's empty artifact is ambiguous (#439).** When the codex adversary
  couldn't reach Docker it produced *no* verdict; an infra-abort reads identically to "ran,
  found nothing" (§10: "empty verdict was infra, not substance"). Cf. #330 where the same
  leaf failed with `'codex'` not found — also surfaced only as an opaque empty result.
- **What is working-as-designed and needs no delta.** The Docker-gated runtime leg is
  *pre-declared*: #440/#441/#468 each carry a `Verification posture` field with a named
  confirmer (#470), so the T3 NEEDS-HUMAN is a *declared* deferred sign-off item — exactly
  the conversion the 2026-06-21 (`Verification posture`) and 2026-07-04 (M4 endpoint-gated,
  working-as-designed) reviews intended. The deferral itself is correct; only the
  can't-run-even-when-capable capability gap is the bug.
- **Validation fitness-to-purpose is NEEDS-HUMAN in every bundle — always-human by design**
  (INTEGRATION.md §4). No delta warranted, consistent with prior reviews.

## Process deltas

- **None warranted.** Every recurring signal this review is a **leaf-sandbox harness
  capability gap**, which belongs upstream to the harness (template-vs-instance boundary),
  not to a Wyrd spec-template / ruleset / gate / agent-skill change. The deferred-runtime
  pattern is already covered by the `Verification posture` field; the prior-art gap is a
  sandbox capability, not a process rule. A forced template/gate change here would be worse
  than none.

## Follow-ups routed (not process deltas — work handed to an owner)

- **Harness/driver (upstream) — Docker API denied in the leaf/reviewer sandbox.** Filed
  **eduralph/pdca-harness#276** (sibling of #261). Docker-gated conformance legs (FDB, TiKV,
  etcd) can't be exercised at Check even on a Docker-capable host — recurs #439/#440/#441/#468
  T3. → owner: Eduard; next step: grant scoped Docker-API access to the leaf/gate sandbox.
- **Harness/driver (upstream) — reviewer leaf has no network grant.** Filed
  **eduralph/pdca-harness#277**. `codex exec` can't reach `api.github.com`, forcing the
  closed/rejected-PR prior-art check to NEEDS-HUMAN on every bundle (#439/#440/#441 T4/T5).
  → owner: Eduard; next step: grant scoped github.com access or a read-only `gh` proxy.
- **Harness/driver (upstream) — adversary leaf empty-on-infra-failure.** Filed
  **eduralph/pdca-harness#278** (relates to #138). The advisory leaf should mark an
  infra-abort as `infra-empty` (distinct from "no findings") so §6 can label it "leaf did
  not run" and prompt a re-run (#439). → owner: Eduard; next step: leaf output-contract delta.
- **Already routed elsewhere (no action here).** The planner-vs-harness gaps this batch
  surfaced — a `Test file` that can't earn per-fix RED, a vacuous `#![cfg(madsim)]` test, a
  private-binary symbol — are covered by upstream **#275** + instance-side
  **getwyrd/wyrd-pdca#104**; wave-base / agent-drift by **#273 / #274**; the C4-verify
  structural-red wording is tracked (noted in #468). Recorded so they are not re-filed.
- **Already tracked (project tracker) — FDB operator-fitness / go-no-go.** #440/#441 leave
  "is compiled-and-selected FDB support without a locally reproduced live Docker round trip
  sufficient?" as an always-human sign-off call. This is **not** a loose Act item: the
  production-fitness decision is owned by **getwyrd/wyrd#442** (release-gating "FDB go/no-go:
  fault + contention battery against `metadata-fdb`, then flip the production default"), which
  depends on #438–#441, reuses #257's harness, and names #470 as the live-container confirmer.
  No PDCA delta; recorded here only so the deferral is cross-linked to its existing owner
  rather than carried untracked. → next step: nothing for Act; the go/no-go verdict lands under
  #442. Watch that it records before the FDB default is flipped.

## How effectiveness will be judged

- Once **#276/#277** land, the next Docker-gated / prior-art cycles should stop raising T3 and
  T4/T5 as *capability-forced* NEEDS-HUMAN — they should either earn their green at Check or
  reduce to a genuine judgment call. Watch the next ~5 backend-conformance cycles.
- **#278** should convert the next infra-aborted advisory leaf into an explicit "leaf did not
  run" row rather than a silent empty pass.
- The #470 live-FDB confirmer should appear as run-green (or an explicit "still deferred") at
  the next review, keeping the deferral auditable.

# Act review — 2026-07-07 — cycles considered: issue_257, issue_405, issue_454, issue_455, issue_458

> The five cycles that froze since the 2026-07-04 (cont.) review (which covered 255/256/258/364/365/366).
> Four (405/454/455/458) are the **M4 closed-write-path / S3-gateway / registration** batch just recorded
> (commit `655e8fa`, "record completed bundles 454, 455, 458 (M4 closed write path)"); #257 is the
> redb→TiKV "Option-B" swap slice that also froze 2026-07-06. issue_115 (ACCEPTED) and issue_153
> (discontinued) remain absent from the frozen index → carried, out of scope; the result dirs
> issue_262/263/264/265/291 exist under `results/` but are **not** yet in the frozen Act index → out of
> scope this review (same handling as #204/#207 on 2026-06-23). No contribution disposition is re-decided.

## What the cycles' records exposed

- **NEW & RECURRING (actionable) — the reviewer/gate leaf sandbox cannot bind a loopback socket, so a
  whole class of loopback-gRPC runtime tests cannot earn an independent red→green at Check and always
  defers to a socket-capable host (4× this batch: #405, #454, #455, #458).** In every one the named
  runtime test panics before its assertion because `TcpListener::bind("127.0.0.1:0")` returns
  `Operation not permitted` / `PermissionDenied`: #405 `consistency_observable.rs:49` (C4 provisional,
  T3 "run on a host that permits local listeners", `:38,:75`); #454 `s3_gateway_cluster.rs:54,:115` (C4
  gRPC S3 red→green + T3 provisional); #455 `closed_write_path.rs:83` (C2, C4, T3 all provisional — and
  its own §10 flags the recurrence "recurs in C2/C4/T3"); #458 `advertise_addr_registration.rs:30`
  (C2, C4, T3). Two things make this a **harness route, not a PDCA delta**: (a) the reviewer skill
  already classifies each as *provisional-not-defect* (the compile/unit/fmt legs pass; the gate fail is
  read as an environment caveat with runnable steps for a capable host, not a blocking C4 FAIL) — the
  same generalisation the cargo-not-found / stale-target caveats produced, so **no reviewer-skill delta**
  is owed; (b) unlike the M4 endpoint-gated TiKV legs (252/253/254) — which **skip cleanly** via
  `WYRD_TIKV_PD_ENDPOINTS`, so the `Verification posture` field pre-declares them — these **hard-fail on
  bind**, so a brief-template pre-declaration would *not* make them runnable at Check. The load-bearing
  gap is a leaf-sandbox **capability** (loopback networking), which is `src/pdca_harness/**` machinery →
  routes **upstream** (below), in the same class as the closed `ensure-cargo.sh` and the zig-cc gate-host
  items. **No spec/ruleset/gate/skill delta.**
- **RECURRING (harness bug, reinforces eduralph/pdca-harness#235) — the publish/C4-verify base parser
  mis-resolves the base again (#454; earlier #252; adjacent #257).** #454 §10 records the
  `_brief_base`/`_clean_ref` "backtick span wins over first token" rule taking the FIRST backtick span
  *anywhere* after `@`, so a backticked branch name in a **trailing prose aside** ("not on `main`")
  hijacked the base → resolved `origin/main` instead of `feat/m4-production-metadata-backend`,
  false-failing `C4-verify` ("patch does not apply — stale") and would misdirect publish's PR base. This
  is the same root bug already routed as **harness#235** (whose #252 case was the mirror direction —
  "main (feature branch `feat/…`)" resolving to the feature branch), now recurring in the opposite
  direction. #257's non-gating `C4-verify` "stale base — patch does not apply" caveat sits on the same
  base/ordering axis (adjacent, plausibly a genuine rebase-needed rather than the parser). The recurrence
  confirms harness#235 is **overdue** and adds a concrete test case (backtick in trailing prose beating
  the `@`-token; fix = anchor the parse to the token immediately after `@`). Reinforces the existing
  upstream route — **no new delta**.
- **#257 Option-B off-Check evidence + the DST/#264 simulator-fidelity axis — pre-declared
  Verification-posture working as designed, no delta.** #257's binding correctness evidence for the
  redb→TiKV swap deliberately lives **off-Check** on a privileged ≥3-replica TiKV cluster (four-leg
  real-cut/no-op/mutated-re-check/restored flips), while the at-Check gate certifies only
  routing/arithmetic/coverage and the DST seed is redb-only by design — all surfaced as *pre-declared*
  C2/C4/C5/T5/V sign-off items, none as a surprise. This is the same deferred-≠-unbuilt conversion the
  `Verification posture` field exists to produce, converging (like #256/#364/#365/#366) on the #367
  first-deployment gate and the always-human #264 fidelity ratification. **Working as designed — no delta.**
- **C5 / T5 / V remain NEEDS-HUMAN by design in every cycle.** Fitness-to-purpose in all five; T5 in
  257/405; C5 in 257. The always-human classes (INTEGRATION.md §4) — the structural constant, not a new
  pattern. The digest's "2× t3 runtime" / "2× validation — human sign-off must decide" are the
  loopback-bind batch (405/454/455/458) surfacing through the C2/C4/T3 rows, already accounted above.
  **No delta warranted** — consistent with every prior review.
- **Single-cycle §10 nit, not recurring — routed, not a delta.** #455: add a **killed/unreachable-D-server**
  fault scenario to `closed_write_path` (the slice proves reachable-fragment-loss only; the brief asked
  for killed-D-server backlog behaviour). This is tied to the human's own #455 sign-off scope call
  (whether reachable-fragment-loss closes #455 or a further iteration is owed) — routes as a Wyrd tracker
  follow-up. 1×; does not meet the spec/ruleset/gate/skill bar.

## Process deltas

- **No spec-template / ruleset / gate / agent-skill delta APPLIED in-band this review.** The one new
  recurring finding (loopback-bind unavailable in the leaf sandbox) is a harness **capability** gap upstream
  of this repo, and the reviewer skill already defers it correctly — a brief-template pre-declaration would
  not make the tests runnable, so it would be motion without effect. The base-parser recurrence reinforces an
  already-open upstream item. The always-human classes (V/C5/T5) and the Option-B/#367/#264 deferrals are
  structural / working-as-designed.
- **One process improvement identified but NOT implemented in-band (routed for later, per human direction):
  a doctor-registration forcing function for new dependencies.** The M4 backend chain kept introducing host
  prerequisites that were registered in `pdca doctor` only *reactively* — docker/openssl added after they
  false-failed gates #252-254 (getwyrd/wyrd-pdca#96), and `protoc` for the etcd leg (#365, `Cargo.toml:110`)
  never registered at all. The brief template only *softly* asks to "seed the render's `[[doctor.checks]]`
  where you can" (`templates/brief.md.tpl` `External dependencies`). Making that a forcing function (mandatory
  doctor row + builder proposes it + reviewer flags an unregistered dep + optional `doctor` reconcile
  mechanism) spans template-provided artifacts (brief template, `agents/*.md`, the doctor schema/mechanism)
  → it is a scheduled implementation task, **filed rather than applied here** (below). A candidate patch was
  drafted this session and reverted at the human's direction — implementation is done later.

## Follow-ups routed (not process deltas — work handed to an owner)

- **Harness/driver issue (upstream template) — the reviewer/gate leaf sandbox must permit loopback
  (`127.0.0.1`) socket binds so gRPC/loopback runtime tests earn a real red→green at Check (#405, #454,
  #455, #458).** Today `TcpListener::bind("127.0.0.1:0")` returns `Operation not permitted` in the leaf
  sandbox, so C2/C4/T3 for the whole M4 loopback-gRPC test class (S3 gateway, closed write path,
  advertise/registration, consistency-observable) can only ever be *provisional* at Check and lean on a
  hand run on a socket-capable host. Ask: allow loopback binds in the leaf/gate/advisory sandbox profile
  (or provide a documented socket-capable gate lane). `src/pdca_harness/**` sandbox profile → routes
  **upstream to the template**. → **FILED eduralph/pdca-harness#261** (2026-07-07, label `bug`);
  confirm progress next review.
- **Harness/driver issue (upstream) — base-parser recurrence of a CLOSED issue.** #454's `_clean_ref`
  mis-resolution (a backtick span in trailing prose beating the `@`-token → wrong base, false `C4-verify`
  stale-fail, mis-directed publish PR base) is a second occurrence of harness#235 **in the opposite
  direction** — and #235 is **CLOSED COMPLETED** (2026-07-04), so its fix special-cased the *parenthetical*
  form and left this bare-backtick-span case open. Fix: anchor the parse to the token immediately after
  `@`, never let a backtick span elsewhere in the base string win (both directions), + add the #454 case.
  → **FILED eduralph/pdca-harness#262** (2026-07-07, label `bug`, references #235); confirm progress next
  review.
- **Harness/driver enhancement (upstream template) — doctor-registration forcing function for new
  dependencies.** When a change introduces a dependency a human must install/provide (build tool, system
  lib, runtime service, capability), the system must REGISTER it — list it in `pdca doctor` with an install
  hint and prompt the human — instead of letting it surface as a cryptic build failure (docker/openssl
  #252-254 registered reactively; `protoc` #365 never registered). Scope (implementation later): mandatory
  registration in the brief template `External dependencies` field; builder proposes the `[[doctor.checks]]`
  row for a discovered dep; reviewer flags an unregistered dep NEEDS-HUMAN; optional `pdca doctor` reconcile
  mechanism. Template/agents/doctor-schema = template-provided → routes **upstream**. → **FILED
  eduralph/pdca-harness#263** (2026-07-07, label `enhancement`; downstream instance follow-up noted there:
  register `protoc` in this repo's `pdca.toml` once the convention lands). Not implemented in-band.
- **Tracker (Wyrd, #455 §10) — add a killed/unreachable-D-server fault scenario to `closed_write_path`.**
  The slice proves reachable-fragment-loss only; the brief asked for killed-D-server backlog behaviour, so
  broader fault coverage is later hardening and is coupled to the human's #455 sign-off scope decision.
  → owner: Eduard; next step: file against getwyrd/wyrd (M4 closed write path) if the human's #455 sign-off
  keeps it as a follow-up rather than a re-iteration; record id next review.

## Still open (carried)

- **issue_115** (ACCEPTED, 2026-06-20) and **issue_153** (discontinued/handed off, 2026-06-21) still
  have not had an Act review and remain absent from this index. → next Act review, if still in scope.
- From 2026-07-04: design issue **getwyrd/wyrd#426** (shared-conformance-pin governance, #419/#254/#261)
  and upstream harness items **eduralph/pdca-harness#235** (fail-closed worktree isolation + base parser,
  CLOSED — its base-parser residual now filed as **#262** from #454) / **#236** (zig-cc-shimmed gate host)
  should show progress next review; the new **#261** (leaf-sandbox loopback-bind) should too;
  **#367** (first-deployment gate) remains the convergence point for the deferred live greens of
  #256/#364/#365/#366 **and now #257** — confirm it lands or is explicitly still deferred.
- Prior reviews' carried items: `pdca doctor` docker+openssl rows (getwyrd/wyrd-pdca#96); the closed
  `ensure-cargo.sh` cargo-not-found fix; the upstream `run-verify.sh` bespoke-env/rename run-hook item
  (2026-07-04 cont.); harness#120/#121; getwyrd/wyrd#242/#243 + doc #244; #268 ADR-0010 `BlockReadFault`
  amendment; #356 fan-out id-map enhancement; the #256 gateway/custodian process-role tracking issue.

## How effectiveness will be judged

- WATCH the next ~5 cycles for **another loopback-bind-denied deferral**: once the upstream sandbox
  capability lands, the M4 loopback-gRPC tests (closed write path, S3 gateway, registration,
  consistency-observable) should earn a real automated red→green at Check instead of a provisional
  human-run caveat; if the denial recurs before the fix, the routed harness item is overdue. A fifth
  occurrence (after 405/454/455/458) confirms this is now the dominant Check-time verification gap for M4.
- WATCH for **another base-parser mis-resolution**: a third occurrence (after #252, #454) makes
  harness#235 overdue, and the fix must be confirmed to cover *both* directions (prose-aside backtick and
  parenthetical-branch).
- The deferred live greens (#256/#364/#365/#366/**#257**) should converge on a real **#367** run — the
  next review should see each off-Check observation either landed at #367 or explicitly still deferred, so
  the deferral chain stays auditable.

# Act review — 2026-07-04 (cont.) — cycles considered: issue_255, issue_256, issue_258, issue_364, issue_365, issue_366

> The six cycles that froze after the earlier 2026-07-04 entry (which covered 252/253/254/285/286/
> 290/347/348/349/350/419). These six are the **M4 "first-deployment" chain**: #255 server redb/TiKV
> config selection, #256 small-multi-node deploy topology, #258 the DST second-impl pin, #364 the S3
> gateway floor, #365 the etcd Coordination backend, #366 the custodian/reconstruction rebuild that
> consumes #365. This entry does not re-open the earlier same-day deltas/routes. issue_115 (ACCEPTED)
> and issue_153 (discontinued) remain absent from the frozen index → carried, out of scope. No
> contribution disposition is re-decided.

## What the cycles' records exposed

- **NEW & RECURRING (actionable) — `run-verify.sh` / the non-gating `C4-verify` gate cannot run a
  crate that needs a bespoke test environment, so it reports a *bogus* "RED with fix applied" FAIL
  (2× this batch: #258 madsim, #366 new-crate rename — and #366 records it "recurring since iter-3").**
  `run-verify.sh` runs a plain `cargo test`. When the bundle's test only exists under a special
  invocation, that runner sees zero tests / fails to build and emits a red the patch did not cause:
  #258's DST tests are `#![cfg(madsim)]` (`concurrency.rs:34`) and compile to nothing without
  `--cfg madsim` (which only `cargo xtask dst` sets), so `C4-verify` false-failed while the **gating**
  `C4-ci` ran the same suite green under `--cfg madsim`×50 seeds (`SUMMARY.md:47-65`; both advisories
  reproduced green by hand with `RUSTFLAGS='--cfg madsim'`); #366's `git mv` into a new crate broke
  `run-verify`'s pathspec so it "cannot handle new-crate renames … forced accepting a reasoned rather
  than observed red." In every case the gating `C4-ci` carried the real red→green and the reviewer
  skill correctly read the `C4-verify` fail as a **posture/tooling mismatch, not a patch defect** (the
  2026-06-22 stale-/unreadable-target caveat generalised to cover it) — so nothing wrong shipped. But
  this is now a recurring **harness** limitation that repeatedly denies madsim-gated / crate-renaming
  bundles an automated green-with-fix and leans the whole verdict on `C4-ci` + hand runs. `run-verify.sh`
  is harness machinery (`src/pdca_harness/**`) → routes **upstream**; the recurrence promotes it from
  a one-off §10 nit to a filed item (below). Not a PDCA spec/ruleset/gate/skill delta.
- **The DST / simulator "second-implementation" fidelity question is the always-human #264 axis — now
  recurring across #258 AND #365, working as a *pre-declared* sign-off item, no delta.** Both slices
  pin a trait by building a deterministic **model** as the second implementation — #258 a
  `SimTikvMetadataStore` (pessimistic-lock-at-prewrite, `support/mod.rs:404-462`), #365 a
  madsim-etcd-client simulator — and in both the C5/T5 crux is identical: is the deterministic
  simulator proof an accepted stand-in for real distributed correctness (2PC/TSO interleavings; real
  etcd), or must a real-backend green precede shipping? This is the explicitly-open #264 judgment the
  briefs *pre-declare* (0015:798-801), so it lands as a ratification item, not a surprise C5/T5 — the
  `Verification posture` + `Production reach` fields doing their job on the fidelity axis. Always-human
  (INTEGRATION.md §4) — **no delta**.
- **The off-Check live-deployment green converges on #367 (the first-deployment gate) across four of
  the six (#256, #364, #365, #366) — Verification posture working as designed; #367 is the tracked
  convergence dependency, not a delta.** Each slice builds a piece whose *live* proof (docker bring-up
  #256; public-TLS S3 listener #364; real-etcd conformance #365; live Prometheus/OTLP exporter #366) is
  deliberately deferred to the #367 day-one runbook gate. All surfaced as **pre-declared** V items, none
  as surprise C2/C4. Confirmed #367 is OPEN ("Define the first-deployment gate — the blueprint's day-one
  runbook, executable end to end"). This is exactly the deferred-≠-unbuilt conversion the fields exist to
  produce (each names what IS exercised at Check — redb CI path, in-process read-back, plaintext loopback,
  the docker-compose `config` parse). **No delta** — carried as the #367 convergence watch.
- **A new pre-1.0 production dependency needing the ADR-0003 audit recurs — #365's `etcd-client 0.14`
  echoes the `tikv-client 0.4` chain (252/253/255).** `store.rs:207` dials `Client::connect(endpoints,
  None)` (no TLS/auth) and `etcd-client` enters the production feature graph (`Cargo.toml:101`); like
  tikv-client it owes the ADR-0003 three-test audit + `deny.toml` allowlist + a TLS/auth posture before
  production exposure. This is the standing always-human governance item at each M4-backend sign-off, not
  a process pattern to fix — routes as a tracked follow-up (below), no delta.
- **C5 / T5 / V remain NEEDS-HUMAN by design in every cycle.** Fitness-to-purpose in all six; T5 in
  256/258/364/365/366; C5 in 258/365/366. The always-human classes (INTEGRATION.md §4) — the structural
  constant, not a new pattern. **No delta warranted** — consistent with every prior review.
- **Single-cycle §10 nits, none recurring — routed, not deltas.** #256 gateway/custodian process-role
  tracking issue (confirm/file) + `SMALL_MULTI_NODE_ENDPOINTS` D-server-port readiness gap (a
  crash-looping `dserver*` still prints success) + the `xtask` self-scan / D-server-count sizing calls;
  #364 no in-tree `wyrd s3` client / smoke harness (live sign-off fell back to the in-suite real-SDK
  test because `aws` CLI was absent). Each is 1×; none meets the spec/ruleset/gate/skill bar.

## Process deltas

- **No spec-template / ruleset / gate / agent-skill delta warranted this review.** The `Verification
  posture` + `Production reach` fields are converting the M4 chain's deferred greens and the DST/etcd
  simulator-fidelity questions into pre-declared sign-off items as designed; the always-human classes
  (V/C5/T5) are structural; the one new recurring finding (`run-verify.sh` bespoke-env / rename
  limitation) is **harness machinery** upstream of this repo, not a PDCA spec/ruleset/gate/skill change.
  A forced delta here would be worse than none.

## Follow-ups routed (not process deltas — work handed to an owner)

- **Harness/driver issue (upstream template) — `run-verify.sh` (C4-verify) must handle bespoke-env
  crates and crate renames instead of emitting a bogus "RED with fix applied" (#258, #366; #366 says
  recurring since iter-3).** Give the per-fix runner a per-repo/per-crate **run hook** (a parameter or
  alternate validation script) so a `--cfg madsim` / `cargo xtask dst` bundle earns a real automated
  green-with-fix, and teach it the **`git mv`-into-new-crate** case so a renamed test path does not break
  its pathspec. Until then, madsim-gated and crate-renaming bundles rest entirely on the gating `C4-ci` +
  a hand run. `src/pdca_harness/**` → routes **upstream to the template**. → owner: Eduard; next step:
  file against eduralph/pdca-harness when bumping the harness; record the id next review.
- **Tracker (Wyrd, #256) — confirm-or-file the gateway/custodian process-role tracking issue.** The
  brief flags this role as likely UNTRACKED and asks Act to confirm it exists (believed to, unconfirmed);
  a tracker search here did not surface it. → owner: Eduard; next step: search getwyrd/wyrd and file
  (milestone M4) if absent; record id next review.
- **Tracker (Wyrd, #256 §10) — small-multi-node smoke check false-greens on a crashed D server.**
  `SMALL_MULTI_NODE_ENDPOINTS` (`xtask/src/main.rs`) waits only on etcd/PD/TiKV, not the D-server ports,
  so a crash-looping `dserver*` still prints success; add the D-server ports to the readiness wait. →
  owner: Eduard; next step: file against getwyrd/wyrd (M4 deploy); record id next review.
- **Tracker (Wyrd, #364 §10) — no in-tree S3 client / smoke-test harness.** Live sign-off had to fall
  back to the in-suite real-SDK test because the `aws` CLI was absent and no `wyrd s3` client subcommand
  exists, so a signed PUT/GET/DELETE over the wire could not be driven by hand — a gap #367's day-one
  runbook will need closed. → owner: Eduard; next step: file against getwyrd/wyrd (M4 S3); record id next
  review.
- **Tracker/governance (Wyrd, #365 §10) — `etcd-client 0.14` ADR-0003 audit + TLS/auth posture.**
  Accepted as this slice's posture but owed before production etcd exposure: the ADR-0003 three-test
  audit + `deny.toml` allowlist + real TLS/auth over `Client::connect(endpoints, None)` (`store.rs:207`).
  Companion to the standing `tikv-client 0.4` audit (INTEGRATION §4). → owner: Eduard; next step: file
  against getwyrd/wyrd (M4); record id next review.
- **#367 runbook qualifications carried from #366 §10 (into the #367 gate, not a delta):** the day-one
  runbook must state that `reconstruction_data_loss` does NOT fire on node death until #365 membership
  lands (watch `reconstruction_unreachable` as the dead-node proxy), and that which gauge signals a kill
  is capacity-conditional (spare capacity → `under_replicated` rise→zero; bare exactly-n → lands on
  `reconstruction_repair_blocked`). → owner: Eduard; fold into #367 when authored.

## Still open (carried)

- **issue_115** (ACCEPTED, 2026-06-20) and **issue_153** (discontinued/handed off, 2026-06-21) still
  have not had an Act review and remain absent from this index. → next Act review, if still in scope.
- From the earlier 2026-07-04 entry: the design issue **getwyrd/wyrd#426** (may an executable
  conformance pin be frozen into the SHARED multi-backend suite before its decision is ratified — the
  #419/#254/#261 axis) and the two upstream harness items **eduralph/pdca-harness#235** (fail-closed
  worktree isolation) / **#236** (zig-cc-shimmed gate host) should show progress next review.
- **#367** (first-deployment gate, OPEN) is the convergence point for the deferred live greens of
  #256/#364/#365/#366 — confirm it lands (or is explicitly still deferred) at the next review.
- Prior reviews' carried items: `pdca doctor` docker+openssl rows (getwyrd/wyrd-pdca#96); the closed
  `ensure-cargo.sh` cargo-not-found fix; harness#120/#121; getwyrd/wyrd#242/#243 + doc #244; #268
  ADR-0010 `BlockReadFault` amendment; #356 fan-out id-map enhancement.

## How effectiveness will be judged

- WATCH the next ~5 cycles for **another `run-verify.sh` bogus-red** on a bespoke-env crate (madsim,
  `cargo xtask dst`, feature-gated) or a crate rename: if it recurs after the upstream run-hook lands,
  the route was right; if it recurs before, the routed harness item is overdue. A third occurrence
  (after #258/#366) confirms the promotion from watch to action was warranted.
- The four deferred live greens should converge on a real **#367** run — the next review should see
  #256/#364/#365/#366's off-Check observations either landed at #367 or explicitly still deferred, so the
  deferral chain stays auditable rather than silently dropping.
- The DST/etcd simulator-fidelity axis (#264) should keep landing as a *pre-declared* ratification item,
  not a surprise C5/T5; if a future second-impl slice raises it as a surprise, the `Production reach`
  field needs a fidelity sub-field.
- The routed follow-ups (upstream run-hook; the four Wyrd tracker items) should appear as tracker/issue
  ids (or an explicit "still deferred") at the next review.

# Act review — 2026-07-04 — cycles considered: issue_252, issue_253, issue_254, issue_285, issue_286, issue_290, issue_347, issue_348, issue_349, issue_350, issue_419

> Considers the eleven cycles that froze since the 2026-07-01 review. Six of them
> (285/286/290/347/349/350) were swept into the ledger on 2026-07-02 (`.act-reviewed` 30→36,
> commit `1cafaec` — "no act-log.md entry yet; pending an act-log delta"); the newest five
> (252/253/254/348/419) were swept into the working-tree ledger since. **This entry is that
> deferred narrative**, over all eleven. issue_115 (ACCEPTED) and issue_153 (discontinued)
> remain absent from the frozen index → still out of scope, carried. No disposition is re-decided.

## What the cycles' records exposed

- **The toolchain-absent gate failure recurred (4×: #330, #348, #350, #252) — AND its 2026-07-01
  upstream route has already been acted on. Record as evidence + one residual provisioning gap.**
  The dominant recurring signal (ledger + digest "3× C4 … FAILED — cargo: not found") is that the
  Do/gate subprocess inherited a bare `PATH` without `~/.cargo/bin`, so `C4-ci` false-failed as
  `exec: cargo: not found` (#348 `check-gates.json:37`; #330), the `cc`→`zig cc` shim rejected a
  bench-only native-C dep (#350), and cargo-deny/cargo-machete/pkg-config/libssl-dev were absent
  (#350, #252). Crucially the harness fix this chain **routed upstream on 2026-07-01 has landed** —
  `d3f0452 chore(engine): resolve cargo under a bare PATH for the gate subprocess` adds
  `engine/lib/ensure-cargo.sh` (sources rustup's env when cargo is off `PATH`), which `engine/xtask.sh`
  + `run-verify.sh` call before first cargo use. So the `cargo: not found` root cause is **closed**;
  the reviewer skill again correctly read every such fail as a "toolchain/target-state caveat, not a
  patch defect," not a blocking C4 FAIL (#348/#350 §6). The **residual** is host *provisioning* for the
  M4 TiKV leg (docker + pkg-config + libssl-dev), which the doctor never preflighted — the one item
  that meets the process-delta bar this review (below).
- **The `Verification posture` field is doing its job across the whole M4/TiKV chain (252/253/254) —
  record as evidence, no new delta.** Every M4 slice's red is **endpoint-gated** (skips without
  `WYRD_TIKV_PD_ENDPOINTS`/docker TiKV), so `C4-verify` "fails" as a *designed, non-gating* artifact —
  and in all three it surfaced as a **pre-declared** sign-off item, explicitly "NOT a real regression …
  do not read it as a blocking verification failure" (#254 §6 C4; #253 §6 C4; #252 §6 T3), tracked to
  the #146 deferred-posture sign-off convention and now to the #91 C4-verify base caveat. This is
  exactly the surprise-C2/C4 → declared-item conversion the 2026-06-21/06-22 deltas exist to produce.
  Working as designed — **no delta**.
- **C5 / T5 / V remain NEEDS-HUMAN by design in every cycle.** Fitness-to-purpose in all eleven; T5
  judgment in 252/253/254/348/349/350/419; C5 in 349/419. These are the always-human classes
  (INTEGRATION.md §4) — the digest (3× T5, 2× C5, 2× V, 2× "decision owed: confirm") is that
  structural constant, not a new pattern. **No delta warranted** — consistent with all prior reviews.
- **NEW (design/governance, #419+#254) — an executable conformance pin was frozen into the SHARED,
  multi-backend suite before the decision it encodes was ratified.** #419 lands
  `contract_read_after_commit` / `contract_scan_is_consistent_cut` into
  `crates/metadata-conformance/src/lib.rs`, encoding #261's read-consistency semantics — but #261 is
  **open/unratified** (proposal 0015 still lists it under §Open questions, no ADR; #419 §6 V), and #254
  *inherits the frozen suite unchanged* for TiKV. Whether a shared, governing, multi-backend pin may be
  frozen ahead of the governed decision is a genuine architecture/governance question, not a code fix —
  routes as a design issue (below), not a process delta.
- **Worktree isolation silently fell back to in-place, mutating the human's primary checkout (#252) —
  harness machinery, partially addressed upstream, residual routes.** `publish._clean_ref` preferred a
  backticked span over the first token, so a brief base of "main (feature branch `feat/…`)" resolved to
  the feature branch → nonexistent ref → silent in-place fallback (Do + C4 ran in `../wyrd`), with no
  per-gate record of which tree ran and no persisted test stdout to attribute the exit-101 red (#252
  §10 ×4). The recent **milestone integration-branch convention** work (`65be7a2`, `16804fd` #91,
  `9789ff8` #93 — run-verify base now follows the brief's integration branch) addresses the base-resolution
  axis; the residual (fail-CLOSED instead of silent in-place; the `_clean_ref` token-vs-parenthetical
  parser; persisting gate stdout) is `src/pdca_harness/**` machinery → routes upstream.
- **Single-cycle §10 nits, none recurring — noted, not deltas.** #285 rs(k,0) hard-read-error fitness
  (in ledger, one-off V); #253 stale "conformance suite" log line at `xtask/src/main.rs:183`; #254
  `SCAN_CAP` product-visibility + caller-side audit-signal calls; #350 unfenced `pub backfill::reconcile`
  entrypoint; #290 dropped read-path preallocation. Each is 1×; none meets the spec/ruleset/gate/skill bar.

## Process deltas

- **Instance config (this repo): two active `pdca doctor` prerequisite rows for the M4 TiKV leg** —
  `docker info` and `pkg-config --exists openssl` (both `level = "WARN"`, group `engine`). The M4 chain
  (252/253/254, with #256/#258 still to come) repeatedly needs Docker + pkg-config + libssl-dev for the
  on-demand `cargo xtask tikv-conformance` path (tikv-client 0.4 → native-tls → openssl-sys), and #252
  §10 explicitly asked to add both to `pdca doctor`; absent them, the runner even mis-retried a
  deterministic OpenSSL build failure 5× as "TiKV may still be bootstrapping." These are INSTANCE
  prerequisites (this repo's config values), not harness machinery, so they land here — as WARN, since
  the default feature-off `cargo xtask ci` needs neither.   (`pdca.toml` — new active `[[doctor.checks]]`
  rows after the commented examples, ~`pdca.toml:446-465`) — tracked as **getwyrd/wyrd-pdca#96**
  (also covers adding the same prerequisites to the CI workflow that runs the TiKV leg).
- **No spec-template / ruleset / gate / agent-skill delta warranted.** The `cargo: not found` root cause
  is closed upstream (`ensure-cargo.sh`); the `Verification posture` field converts the M4 endpoint-gated
  reds to declared items as designed; the always-human classes are structural. A forced change would be
  worse than none.

## Follow-ups routed (not process deltas — work handed to an owner)

- **Design issue (governance, #419/#254/#261) — may an executable conformance pin be frozen into the
  SHARED multi-backend suite before its decision is ratified into a governed doc?** #419 froze #261's
  read-consistency semantics into `crates/metadata-conformance/src/lib.rs` (inherited unchanged by #254's
  TiKV backend) while #261 is still open and proposal 0015 lists it under §Open questions. This needs a
  governance/architecture decision (ratify #261 first? allow provisional pins with a marker? gate shared
  pins on ratification?), not a code change. → **FILED getwyrd/wyrd#426** (milestone M4 — Production
  metadata backend, 2026-07-04); do NOT author a brief here — governance decision first.
- **Harness/driver issue (upstream template) — worktree isolation must fail CLOSED, not silently fall
  back to in-place (#252).** Residual after the #90/#91/#93 integration-branch-convention work: (a)
  `publish._clean_ref` should take the token before a parenthetical, not a backticked span, so a base of
  "main (feature branch `feat/…`)" resolves to `main`; (b) a nonexistent/unresolvable base must abort the
  beat, never short-circuit to mutating the operator's primary checkout; (c) persist each gate's test
  stdout into the bundle and record which tree each gate ran in. `src/pdca_harness/**` + publish parser →
  routes **upstream to the template**. → **FILED eduralph/pdca-harness#235** (2026-07-04).
- **Harness/driver issue (upstream, #350) — the Wyrd gate host should not be a `zig cc`-shimmed /
  ephemeral toolchain.** `cc`→`zig cc` (`~/.local/bin/cc` → a `/tmp/pyzig` venv) made C4-ci false-fail on
  a bench-only native-C dep, and cargo-deny/cargo-machete were absent; with a real `cc` + the two CLIs the
  gate passes (regenerated via `pdca gates 350`). Companion to the closed `ensure-cargo.sh` fix: provision
  the gate host with a real `cc` and the deny/machete CLIs (or scope `--all-targets` off bench-only C
  deps). → **FILED eduralph/pdca-harness#236** (2026-07-04).
- **Tracker (Wyrd, one-off §10 nits) — file if/when the owning file is next touched:** #253 stale log
  line `xtask/src/main.rs:183` (says "conformance suite" though `contention` also runs now); #350 unfenced
  `pub backfill::reconcile`; #290 read-path preallocation follow-up. Minor; not standalone-issue-worthy
  unless they recur. → owner: Eduard; fold in on next touch.

## Still open (carried)

- **issue_115** (ACCEPTED, 2026-06-20) and **issue_153** (discontinued/handed off, 2026-06-21) still have
  not had an Act review and remain absent from this index. → next Act review, if still in scope.
- Prior reviews' routed items to confirm at the next review: the Wyrd `cargo doc`/rustdoc-deny gate step
  and `tier1-disk-faults.yml` sudo fix (2026-06-25); the upstream leaf-env-loading item — **closed** by
  `d3f0452` (`ensure-cargo.sh`); harness#120 target-pin / #121 reviewer-Basis; getwyrd/wyrd#242/#243
  design issues + doc #244; #268 ADR-0010 `BlockReadFault` amendment; #356 fan-out id-map enhancement.

## How effectiveness will be judged

- WATCH the next ~5 cycles for **any recurrence of a toolchain-absent / provisioning gate failure** after
  `ensure-cargo.sh` + the two new doctor rows: if `cargo: not found` recurs, the upstream fix regressed;
  if a *TiKV-leg* provisioning miss recurs, the doctor rows should now have preflight-warned it (so the
  miss is operator error, not a silent trap). A silent in-place fallback recurring makes the fail-closed
  harness route overdue.
- The design issue (#419/#261 shared-conformance-pin governance) and the two upstream harness items should
  appear as a scheduled discussion / tracker ids (or explicit "still deferred") at the next review.
- The routed-work-becomes-merged-PDCA-cycle loop should continue.

# Act review — 2026-07-03 — cycles considered: issue_252, issue_253

> A targeted entry, not a periodic sweep: it records one cross-cycle insight surfaced by the
> post-merge Codex review of the M4.1/M4.2 slices (getwyrd/wyrd#421, #422). No contribution
> disposition is re-decided; #422's gap is fixed in getwyrd/wyrd#423.

## What the cycles' records exposed

- **A brief's embedded "Verified backend facts" can launder a wrong assumption into ground
  truth — worse than no fact at all.** #253's brief (`results/issue_253/brief.md`) carried a
  *Verified backend facts* block to spare Do re-deriving tikv-client behaviour. One fact —
  "a pessimistic `put`/`delete` buffers locally, no write-conflict" — was **wrong**
  (tikv-client 0.4.0's `put`/`delete` eagerly `pessimistic_lock(...)` before buffering). Do
  took it as settled, wrote `commit()`'s put/delete arms to return `Err` on that basis, **and
  the contention test it wrote inherited the same blind spot** — the race exercised only the
  `get_for_update`/`commit` path, never a put-only conflict. Every gate was green (the
  endpoint-gated TiKV tests skip under `C4-*`, and the live-TiKV run only covered the modelled
  path), so the defect surfaced only at the post-merge Codex review (P2 on #422 → fixed in
  #423, which adds `put_only_write_race` and proves red→green on live TiKV). The failure mode
  is specific to the *authority* of a "Verified" label: it **suppresses** Do's own source
  check and shapes the regression test to the wrong model, so the test cannot catch it. This
  differs from an ordinary spec error — the planner's confidence became Do's ground truth with
  no re-derivation step in between.

## Process deltas

- Spec template / planner: any **"verified fact" a brief embeds must carry its source** (the
  dependency `path:line`, or a reproducible one-line check) OR be marked *assumption — confirm
  at build*. An **uncited** "verified" fact is a claim Do must re-check, not ground truth — the
  label must never substitute for the citation.   (`templates/brief.md.tpl` — require a source
  on any facts/assumptions a brief asserts; `.claude/agents/planner.md`)
- Test-design (Do) + reviewer: a property/contention test must exercise **each code path that
  implements the property**, not one representative case — here a conflict via the *put* path,
  not only via the *precondition* path. Reviewer flags when a deferred-green test's covered
  paths are **narrower than** the code paths implementing the claimed property.
  (`.claude/agents/reviewer.md`; `AGENTS.md`)

## Follow-ups routed (not process deltas — work handed to an owner)

- The concrete code gap is already fixed: **getwyrd/wyrd#423** (draft, onto the M4 branch),
  addressing the P2 on #422. By decision, the #253 brief is **left as-authored** — the wrong
  fact stays in the frozen record, and this entry is the correction of record.   → owner: Eduard.

## How effectiveness will be judged

- The next M4 slices (#254–#258) all touch this backend: their briefs' embedded backend facts
  should carry citations, and their property tests should enumerate per-path cases. Watch the
  next ~5 cycles for a recurrence of "a confidently-stated brief fact shaped both the code and
  the very test meant to catch it."

# Act review — 2026-07-01 — cycles considered: issue_268, issue_287, issue_288, issue_330, issue_346, issue_356

> Considers the six cycles that froze since the 2026-06-25 review (the index's 30 minus the 24
> covered through 2026-06-25). issue_115 (ACCEPTED, 2026-06-20) and issue_153 (discontinued,
> 2026-06-21) remain absent from the frozen index → still out of scope, carried. No contribution
> disposition is re-decided.

## What the cycles' records exposed

- **C5 / T5 / V remain NEEDS-HUMAN by design in every cycle.** Fitness-to-purpose in all six;
  T5 judgment in #268/#346/#356; C5 causal adequacy where a root cause is contested. These are
  the always-human classes (INTEGRATION.md §4) — the recurring-signal digest (3× T5, 2× V, 2× C5)
  is that structural constant, not a new pattern. **No delta warranted** — consistent with all five
  prior reviews.
- **The `Verification posture` delta is taking effect — record as evidence, no new delta.** #268
  carries `Verification posture: Flippable regression at Check — the test drives a real tonic …`
  (`results/issue_268/brief.md`) and #288 carries `Verification posture: Flippable regression at
  Check …` (`results/issue_288/brief.md`). None of the six raised a *surprise* C2/C4 NEEDS-HUMAN;
  where a fidelity question existed (#356's `dserver % n` fan-out models the real fleet only for
  dense ids) it landed as a *pre-declared* V item, not a C2/C4 surprise. Working as designed.
- **NEW (one cycle, #330) — the Do/advisory sandbox did not load the toolchain env, so the gate
  and one advisory leaf failed as ENVIRONMENT artifacts, not patch defects.** `~/.cargo/bin` was
  off `PATH` in a fresh session (`results/issue_330/build-notes.md` "Toolchain setup"), so `C4-ci`
  failed as `./engine/xtask.sh: line 30: exec: cargo: not found` and the `codex` advisory leaf
  failed as `[Errno 2] No such file or directory: 'codex'` (`check-advisory-codex.error.log`). The
  other five cycles gated green — this is isolated to #330's session, not systemic. Two observations,
  both **evidence not delta**: (a) the **leaf-failure surfacing improved** — where #195 (2026-06-25)
  had a reviewer leaf fail *silently*, #330 now surfaces the failed advisory leaf as a §6 NEEDS-HUMAN
  row + a "NOT COMPLETED" artifact + an error log; the harness bump v0.43→v0.45 (PR #73) landed
  between batches, so the prior review's harness-robustness watch-item shows real progress. (b) The
  **reviewer skill already generalized correctly** — it treated the `cargo: not found` gate fail as a
  "toolchain/target-state caveat, not a patch defect" and wrote NEEDS-HUMAN, not a blocking C4 FAIL
  (`results/issue_330/check-review.md` grounding note) — the 2026-06-22 stale-/unreadable-target
  caveat covered this class without a new rule. The residual root cause (leaf env-loading; a gate
  that conflates "couldn't execute (env)" with "ran and failed") is **harness machinery**, routes
  upstream — not a PDCA spec/ruleset/gate/skill change.
- **Two single-cycle §10 items, neither recurring — each routes as a follow-up, not a process
  delta.** #268: author the ADR amendment/companion for the new `BlockReadFault` seam category
  (seam-doc shipped this cycle; the ADR is the human's to author separately). #356: an M3 enhancement
  — when dynamic discovery lands, route the fan-out by a real D-server-id→store map instead of
  `dserver % n` (opaque/sparse ids alias under modulo) + an in-range guard at the M2/M3 boundary.
  Both are one-offs; neither recurs across cycles, so neither meets the bar for a spec/ruleset/gate/
  skill change.

## Process deltas

- **None warranted this review.** The recurring NEEDS-HUMAN classes (V/C5/T5) are always-human by
  design; the `Verification posture` delta is carried in the briefs and taking effect; and the one
  new finding (#330's toolchain/env-loading gap) is **not** a PDCA spec/ruleset/gate/skill change —
  its fix is harness machinery (leaf env-loading + an EXEC-ERROR-vs-FAIL gate status) upstream of
  this repo, the leaf-failure surfacing it exposed is already improving via the v0.45 bump, and the
  reviewer skill already generalized the "target-state caveat, not a patch defect" rule to cover it.
  A forced PDCA delta here would be worse than none.

## Follow-ups routed (not process deltas — work handed to an owner)

- **Harness/driver robustness (upstream template) — leaf sandboxes do not load the toolchain env, so
  gates and command-mode leaves fail as environment artifacts (#330).** `~/.cargo/bin` off `PATH`
  (and `codex` not on `PATH`) turned a real red→green into a `cargo: not found` C4-ci fail + a
  `codex not found` advisory-leaf fail. This is the recurrence that promotes the 2026-06-25
  harness-robustness item from "watch" toward action. Two upstream asks: (1) harden leaf env-loading
  so command-mode leaves + `engine/xtask.sh` inherit the operator's toolchain env (source the
  profile / pass through `PATH`); (2) let the gate distinguish "could not execute (env)" from "ran
  and failed" (an EXEC_ERROR status) so `check-gates.json overall` isn't a bare `fail` a reviewer must
  hand-classify. Harness machinery (`src/pdca_harness/**` / leaf orchestration + `pdca.toml` gate
  schema) → routes **upstream to the template**. → owner: Eduard; next step: open a harness/
  template-feedback issue when bumping the harness; record the id next review.
- **Tracker (Wyrd, #268 §10) — author the ADR amendment/companion for the new `BlockReadFault` seam
  category** (ADR-0010 / telemetry ADR-0011). The seam-doc shipped this cycle; the ADR is
  architecture-board authority, authored separately by the human (Do correctly did NOT author one).
  → owner: Eduard; next step: file against getwyrd/wyrd (milestone M3); record id next review.
- **Tracker (Wyrd, #356 §10) — M3 enhancement, fan-out routing by real id→store map.** When dynamic
  discovery lands, replace `route_dserver`'s `dserver % stores.len()` with a real D-server-id→store
  map and add an in-range guard at the M2/M3 boundary, so a discovery-fleet placement naming an
  opaque/sparse/gapped id (e.g. `10, 20, 30`, already selectable per
  `crates/server/tests/failure_domain_registration.rs`) is never silently mis-routed under modulo.
  Not a bug in this slice (the brief blesses `dserver % n` as illustrative for the contiguous M2
  fan-out). → owner: Eduard; next step: file against getwyrd/wyrd (M3 / discovery); record id next review.

## Still open (carried)

- **issue_115** (ACCEPTED, 2026-06-20) and **issue_153** (discontinued/handed off, 2026-06-21) still
  have not had an Act review and remain absent from this index. → next Act review, if still in scope.
- Prior reviews' routed items to confirm at the next review: the Wyrd `cargo doc` / rustdoc-deny gate
  step and the `tier1-disk-faults.yml` sudo fix (2026-06-25); the upstream reviewer-leaf-robustness
  item (2026-06-25) — **partial progress observed** via the v0.45 bump's improved leaf-failure
  surfacing (#330), residual is the env-loading root cause routed above; harness#120 target-pin /
  #121 reviewer-Basis; getwyrd/wyrd#242/#243 design issues + doc #244.

## How effectiveness will be judged

- WATCH the next ~5 cycles for a **recurrence of a toolchain-absent / env-loading gate or leaf
  failure**: if it recurs after the upstream leaf-env-loading fix lands, the route was right; if it
  recurs before, the routed harness item is overdue. A second silent-vs-surfaced leaf failure would
  confirm the v0.45 surfacing improvement held.
- The two routed Wyrd items (#268 ADR amendment; #356 fan-out id-map) and the upstream harness
  env-loading item should appear as tracker/issue ids (or an explicit "still deferred") at the next
  review — follow-ups stay auditable.
- The routed-work-becomes-merged-PDCA-cycle loop should continue (as #195/#196/#197/#198 did).

# Act review — 2026-06-25 — cycles considered: issue_195, issue_196, issue_204, issue_207, issue_251

> Considers the five frozen cycles that had not yet had an Act review (the index's 24 minus
> the 19 covered through 2026-06-23). issue_115 (ACCEPTED, 2026-06-20) and issue_153
> (discontinued, 2026-06-21) remain absent from the frozen index → still out of scope, carried.
> No contribution disposition is re-decided.

## What the cycles' records exposed

- **Two of these cycles ARE the PDCA fixes for last chain's routed Wyrd work — the routing
  loop closed again.** issue_195 lands the **Tier-1 dm-flakey/dm-error** disk-fault campaign
  for **getwyrd/wyrd#195** and issue_196 lands the **Tier-2 kill-and-reconstruct** scenario for
  **getwyrd/wyrd#196** — the two M3.9/M3.10 tier-split items this review chain filed in the
  2026-06-23 follow-up (the #146 "Tier-1/Tier-2 not functionally implemented" gap). Bundle id =
  issue id by init-from-issue, so the correspondence is auditable. issue_251 *self-filed* its own
  §10 gap as **getwyrd/wyrd#268** (gRPC seam strips the `EIO` signal). Evidence the prior reviews'
  routed follow-ups progressed to merged PDCA cycles + a tracked issue, not silent drops — **no
  delta**, recorded as the close (cf. #197/#198 last review).
- **The `Verification posture` + `Production reach` deltas are carried in the briefs and taking
  effect — record as evidence, no new delta.** Both #195 and #196 carry an explicit
  `Verification posture: DEFERRED/off-Check, NET-NEW` line invoking the 2026-06-22 "deferred ≠
  unbuilt" forcing function (`results/issue_195/brief.md:129,147`; `issue_196/brief.md:112,128`)
  and a `Production reach: N/A as a production seam` line (`issue_195/brief.md:153`;
  `issue_196/brief.md:132`). The residual fidelity questions — #195's dm-flakey-vs-direct-`fs::write`
  scrub leg, #196's in-process `MemMeta`/`CrashMeta` metadata vs proposal 0005 §13.2's
  "real NVMe/fsync" mandate — correctly land as **pre-declared §6 ratification / sign-off items**,
  not surprise C2/C4 NEEDS-HUMAN. That is exactly the conversion those fields exist to produce;
  the human judgment they surface is *expected*, not eliminated. Working as designed — **no delta**.
- **C5 / T5 / V remain NEEDS-HUMAN by design in every cycle.** V fitness-to-purpose in all five
  (204, 207, 251, 195, 196); C5 + T5 in #251; T5 + V in #196. Always-human (INTEGRATION.md §4).
  **No delta warranted** — consistent with all four prior reviews.
- **NEW — a denied rustdoc lint is never exercised by the gate, so a broken-intra-doc-link defect
  class passes Check and surfaces only at sign-off (recurring: #196 iteration 1 AND iteration 2,
  plus the #195 sibling).** Root `Cargo.toml:170` sets `rustdoc::broken_intra_doc_links = "deny"`,
  but `cargo xtask ci` runs **no `cargo doc` step** (`results/issue_196/build-notes.md:11-13`;
  `check-review.md:43-44`), so the lint is never exercised by C4-ci. A green C4-ci therefore does
  **not** clear it. It bit #196 twice — iteration 1 (`faults.rs:149` dangling link,
  `iteration-v1/SUMMARY.md:62`) and iteration 2 (the re-homed `assert_*` helpers left three dangling
  references after the orphan was fixed) — each caught only by the codex advisory at sign-off, each
  forcing a reject. This is a **gate-coverage gap**, not a model error. Its fix lives in Wyrd's
  single-source gate (`cargo xtask ci`, ADR-0016) — adding it PDCA-side would *drift* from Wyrd's
  one gate definition (INTEGRATION.md §4/§9, "Wyrd owns the gate defs, no drift") — so it **routes to
  Wyrd**, not a `pdca.toml` delta.
- **NEW — a reviewer leaf failed to run yet the cycle still assembled SUMMARY and reached
  AWAITING_SIGNOFF with no advisory review (#195).** §6: *"re-run the Check reviewer; this bundle
  has no advisory review and must not be accepted until one exists"* — the Claude reviewer leaf
  silently failed to run; only the codex cross-vendor review existed. The C6 accept-guard + the §6
  row **held** (the iteration was rejected, `issue_195/brief.md:227`), so nothing wrong merged — but
  a leaf failing *without a hard error* while the Check beat still composes a sign-off-ready SUMMARY
  is a harness-robustness gap. Single occurrence → route + watch, not a forced delta.

## Process deltas

- **None warranted this review.** The recurring NEEDS-HUMAN classes (V/C5/T5) are always-human by
  design; the `Verification posture` / `Production reach` / "deferred ≠ unbuilt" deltas are carried
  in the briefs and taking effect; and the two new findings (the rustdoc gate gap, the reviewer-leaf
  robustness gap) are **not** PDCA spec/ruleset/gate/skill changes — the first belongs in Wyrd's
  single-source `cargo xtask ci` (adding it to `pdca.toml` would drift from Wyrd's one gate
  definition), the second is harness machinery upstream of this repo. Both route below. A forced
  PDCA delta here would be worse than none.

## Follow-ups routed (not process deltas — work handed to an owner)

- **Gate gap (Wyrd) — `cargo xtask ci` never exercises the denied `broken_intra_doc_links` lint.**
  `cargo doc` is run by no gate step, so a dangling intra-doc link passes C4-ci and is caught only by
  the advisory reviewer at sign-off — it bit #196 across both iterations (`faults.rs:149`, then the
  re-homed `assert_*` helpers). Fix lives in Wyrd's gate (ADR-0016 single source): add a `cargo doc`
  / rustdoc step to `cargo xtask ci`. → owner: Eduard; next step: file against getwyrd/wyrd
  (milestone M3); record id next review. Not a `pdca.toml` change (would drift from the single gate).
- **Workflow bug (Wyrd, #195 §10 codex advisory) — privileged Tier-1 harness runs without sudo.**
  `.github/workflows/tier1-disk-faults.yml:66` runs the harness as the default Actions user, but the
  test performs root-only ops (`losetup`/`dmsetup`/`mount`/`drop_caches`); on hosted Ubuntu the job
  will fail before exercising the harness unless the step runs under `sudo`
  (`results/issue_195/SUMMARY.md:43,68`). Non-blocking advisory, but a real off-Check CI defect that
  lands in Wyrd's tree. → owner: Eduard; next step: file against getwyrd/wyrd (milestone M3.9);
  record id next review.
- **Harness/driver robustness (upstream template) — a reviewer leaf can fail silently and the Check
  beat still assembles a sign-off-ready SUMMARY with no advisory-review artifact (#195).** The C6
  guard + §6 caught it this time, but a failed leaf should surface as a hard error / the Check beat
  should refuse to reach AWAITING_SIGNOFF with no advisory review present, rather than relying on the
  reviewer to write a "re-run me" §6 row. This is harness machinery (`src/pdca_harness/**` /
  leaf orchestration), so it routes **upstream to the template** the harness is rendered from.
  → owner: Eduard; next step: open a harness/template-feedback issue when bumping the harness;
  record id next review.
- **issue_251 §10 — already self-filed getwyrd/wyrd#268** (M3 epic #147): the gRPC seam strips the
  `EIO` signal, so the #251 read-around no-ops for remote D servers until the wire carries a distinct
  block-read-fault code. No Act action — recorded as auditable (the cycle filed its own follow-up).

## Still open (carried)

- **issue_115** (ACCEPTED, 2026-06-20) and **issue_153** (discontinued/handed off, 2026-06-21) still
  have not had an Act review and remain absent from this index. → next Act review, if still in scope.
- Prior reviews' upstream harness follow-ups (eduralph/pdca-harness#120 target-pin, #121
  reviewer-Basis) and the design issues getwyrd/wyrd#242/#243 + doc #244 — confirm progressed at the
  next review.

## How effectiveness will be judged

- The two new routed items (Wyrd `cargo doc` gate step; the tier1-disk-faults.yml sudo fix) and the
  upstream reviewer-leaf-robustness item should appear as tracker/issue ids (or an explicit "still
  deferred") at the next review — follow-ups stay auditable.
- WATCH the next ~5 cycles for a **recurrence of a broken-intra-doc-link (or any rustdoc-deny)
  defect reaching sign-off**: if it recurs after the Wyrd `cargo doc` gate step lands, the gate fix
  was the right call; if it recurs *before* it lands, the routed item is overdue.
- WATCH whether another bundle reaches AWAITING_SIGNOFF with a silently-failed reviewer leaf; one
  more occurrence promotes the harness-robustness route from "watch" to "overdue."
- The #195/#196 close-the-loop should continue — routed M3 tier work becoming merged PDCA cycles.

# Act review — 2026-06-23 — cycles considered: issue_197, issue_198, issue_203, issue_205

> Considers the four cycles that froze on 2026-06-23 and had not yet had an Act
> review. The same-dated "follow-up filing" entry below it is a *separate* beat
> (it closed the three prior reviews' routed items and added no cycles); this
> entry does not re-open it. issue_204 / issue_207 have result dirs but are not in
> the frozen Act index → out of scope here. No contribution disposition is re-decided.

## What the cycles' records exposed

- **Two of these cycles ARE the PDCA fixes for last review's routed Wyrd bugs — the
  routing loop closed.** issue_197 lands the `reconstruction_aborted` accounting fix for
  **getwyrd/wyrd#197** (the #144 telemetry over-count this review chain filed 2026-06-23);
  issue_198 lands the misplaced-but-intact read-path fix for **getwyrd/wyrd#198** (the #143
  `header.chunk_id` recheck asymmetry). The PDCA bundle id matches the Wyrd issue id by
  init-from-issue, so the correspondence is auditable. Evidence the prior reviews' routed
  follow-ups progressed to merged fixes, not silent drops — **no delta**, recorded as the close.
- **C5 / T5 / V remain NEEDS-HUMAN by design in every cycle (all 4).** Always-human
  (INTEGRATION.md §4): V fitness-to-purpose in all four; C5 causal adequacy + T5 judgment in
  #203 and #205. **No delta warranted** — consistent with all three prior reviews.
- **The earlier `Verification posture` delta is taking effect — record as evidence, no new
  delta.** #203's timing-dependent behavioural red (64×16 concurrent writers) surfaced as a
  *pre-declared sign-off item* ("the brief explicitly defers [whether the red reliably fires on
  the CI host] to sign-off", §6 T5), not a surprise C2/C4 NEEDS-HUMAN. None of the four raised a
  surprise C2/C4. Keep watching.
- **Three single-cycle §10 items, none recurring — each routes as a follow-up, not a process
  delta.** All three were re-verified against `origin/main` before filing (verify-don't-recall),
  which materially re-framed two of them: **#197** — the §6 V item asks whether the new *public*
  metric warrants a contract-doc update. Verified against `origin/main`: the canonical docs name
  *five durability metrics* (proposal 0005 §319-340) and ADR-0011 enumerates **none** of the
  `reconstruction_*` accounting counters. (The in-code phrase "the **three** M3 repair metrics" at
  `reconstruction.rs:3,156` is *accurate* and refers to a different trio — under-replicated count,
  queue-depth, time-to-repair — not the accounting counters, so it is **not** stale.) The real gap
  is that the public OTel counters `reconstruction_repaired` / `reconstruction_conflict` /
  `reconstruction_aborted` (the last new in #238, `reconstruction.rs:441/455/471`) and their
  success-netting identity are undocumented in any canonical telemetry contract — a documentation
  addition, not a governance ADR/proposal *decision* change. **#203** — the §10
  read as "comments cite the non-existent ADR-0034"; in fact **ADR-0034 now exists on `origin/main`**
  (PR #237, merged 2026-06-23 21:46) and states exactly "Model A — one D server per disk," grounding
  the citation. The issue_203 review base predated #237's merge, so the reviewer correctly saw no
  ADR-0034 — a **stale/in-flight-target artifact** (same class as #145/#146, already routed as
  harness#120), **not** a builder recall-fabrication. The only residual is the genuine §10
  comment-precision nit: the comments tie the safety to "Model A" specifically, but the load-bearing
  invariant is *exclusive-open-per-root*, which holds under both Model A and the reserved Model B, so
  citing "Model A" over-narrows. **#205**: `install_metric_dispatch()` correctness rests on every
  `#[madsim::test]` calling it first — a convention, not an enforced barrier; a future test added
  without it silently reintroduces the tracing interest-cache flake. Each is a one-off (1×); none
  recurs across cycles, so none meets the bar for a spec/ruleset/gate/skill change.

## Process deltas

- **None warranted this review.** The recurring NEEDS-HUMAN classes (V/C5/T5) are always-human by
  design; the prior deltas (`Verification posture`, `Production reach`, `Success criterion`
  BINDING/ILLUSTRATIVE, reviewer-Basis skill) are taking effect; and the three §10 items are
  single-cycle follow-ups, not patterns. A forced change here would be worse than none. (The
  #203 "fabricated-ADR" worry this entry originally flagged was **withdrawn on verification** —
  ADR-0034 exists on `origin/main`; the reviewer saw it absent only because the review base
  predated PR #237's merge. That is the stale/in-flight-target axis already routed upstream as
  harness#120, *not* a builder recall-fabrication — so no new builder-skill delta is warranted;
  the existing target-pin follow-up covers it.)

## Follow-ups routed (not process deltas — work handed to an owner)

- **Documentation update (#197) — FILED getwyrd/wyrd#244:** the public OTel counters
  `reconstruction_repaired` / `reconstruction_conflict` / `reconstruction_aborted` (the last new in
  PR #238) and their `successes = repaired − conflict − aborted` identity are documented only in
  in-code module comments; no canonical telemetry contract (proposal 0005 §319-340's five durability
  metrics, ADR-0011) describes them, so an operator scraping `reconstruction_aborted` has no
  reference. Filed as a documentation issue (milestone M3 — Custodians), noting the maintainer must
  pick the home and that ADR-0011 is Accepted/immutable (a superseding ADR, not an in-place edit, if
  that is the chosen home). Not a code change; the metric itself is correct and shipped. Grounded
  against `origin/main` (`crates/custodian/src/reconstruction.rs:441/455/471`).
- **Comment-precision nit (#203) — NO ACTION (human decision):** the "non-existent ADR-0034"
  premise is **false** — ADR-0034 exists on `origin/main` (PR #237) and states exactly "Model A —
  one D server per disk," grounding the citation; the reviewer saw it absent only because the review
  base predated #237's merge (stale/in-flight target, harness#120). The sole residual is the genuine
  §10 nit: the comments tie safety to "Model A" specifically, but the load-bearing invariant is
  *exclusive-open-per-root*, which holds under both Model A and the reserved Model B — so citing
  "Model A" over-narrows (`results/issue_203/patch.diff:22-23,43-44,77`). Very minor; review caught
  nothing wrong with the *code*. **No action taken — human decision (2026-06-24): not worth a
  standalone issue;** fold the comment wording in if that file is next touched. Closed here.
- **Design issue (Wyrd, future footgun, #205) — needs planning + review, not a one-line fix:**
  the `install_metric_dispatch()` barrier is enforced only by convention — every
  `#[madsim::test]` must call it first, and `set_global_default`'s error is swallowed — so a
  future test added without the call silently reintroduces the tracing interest-cache flake.
  Making it *enforced* is an architecture choice with options to weigh (a compile-time/test-harness
  wrapper that installs the dispatch for every test, a custom test macro/attribute, a lint, or at
  minimum surfacing the swallowed `set_global_default` error) — it needs a dedicated
  planning/design phase outside this PDCA cycle and a review of the chosen mechanism, not a
  quick cleanup. **Filed getwyrd/wyrd#243** (milestone M3 — Custodians; grounded against
  `origin/main` custodian.rs:354-358 + the 7 per-test call sites). Do not author a brief for it
  here — design + review first.
- **Design issue (Wyrd, DST determinism, #205) — needs planning + review:** the root cause
  behind the footgun above is that `tracing`'s **global, process-wide** per-callsite interest
  cache is mutable state shared across madsim tests — once poisoned to `never` it persists and
  produces a non-deterministic DST flake, which undercuts the determinism premise DST rests on
  (ADR-0009). The #205 patch addresses the one observed callsite (scoped `with_subscriber` +
  `install_metric_dispatch()`), but the general invariant — *no shared mutable global may cross
  into the DST substrate and defeat seed-determinism* — is unaddressed and broader than tracing
  (any global static is suspect). Resolving it (forbid/isolate global state in DST: a harness that
  resets or sandboxes such statics per seed, a lint/audit, or an ADR-0009 amendment stating the
  rule) is an architecture decision needing a dedicated planning/design phase and review, outside
  this PDCA cycle. **Filed getwyrd/wyrd#242** (milestone M3 — Custodians; root-cause companion to
  #243; grounded against `origin/main` custodian.rs:354/952/975 + ADR-0009). Likely resolved via
  an ADR-0009 follow-on. Do not author a brief for it here — design + review first.

## Still open (carried)

- **issue_115** (ACCEPTED, 2026-06-20) and **issue_153** (discontinued/handed off, 2026-06-21)
  still have not had an Act review and remain absent from this index. → next Act review, if still
  in scope.
- Prior reviews' upstream harness follow-ups (eduralph/pdca-harness#120 target-pin,
  #121 reviewer-Basis) — confirm progressed at next review.

## How effectiveness will be judged

- All actionable §10 items are dispositioned: filed getwyrd/wyrd#242, #243 (the two #205 design
  items) and #244 (#197 documentation); #203 closed as no-action (human decision). The next review
  should see the #197/#198 close pattern continue — routed bugs becoming merged PDCA cycles.
- The #203 "non-existent ADR" turned out to be a **stale/in-flight-target** artifact, not a builder
  fabrication — the same axis already routed as harness#120 (pin the review target to the base the
  gates ran against). WATCH the next dependency-/concurrent-ADR-chained cycles: if a reviewer again
  flags a cited ADR/spec "absent" that is merely in-flight on the real `origin/main`, the harness
  target-pin is overdue. No builder-skill delta — the earlier framing was withdrawn on verification.

## Session note (2026-06-24 follow-up filing)

- Filed **getwyrd/wyrd#242** (DST: global mutable state can defeat seed-determinism) and
  **getwyrd/wyrd#243** (`install_metric_dispatch()` barrier relies on convention) — the two #205
  design items, milestone M3 — Custodians, grounded against `origin/main`.
- Grounding the remaining two against `origin/main` re-scoped both: **#197** filed as
  **getwyrd/wyrd#244** (documentation: the public `reconstruction_*` OTel counters + their netting
  identity are undocumented in any canonical telemetry contract — not the governance ADR change the
  §6 implied). **#203** closed **no-action** at the human's direction — its "non-existent ADR-0034"
  premise was false (ADR-0034 exists; in-flight-target artifact), leaving only a trivial comment nit
  not worth a standalone issue.
- Verify-don't-recall note: three of the §10/§6 framings (the #203 "fabrication", the #197
  "governance doc change", and this session's own first-pass "three→four" mis-derivation) were
  corrected only by reading `origin/main` directly. The local `../wyrd` checkout was **stale**
  (behind `origin/main`, missing #237/#241) — the same target-drift the harness#120 pin addresses.

# Act review — 2026-06-23 — follow-up filing (no new cycles)

> Not a cycle review — this entry closes the audit loop the three prior reviews
> (2026-06-21, 2026-06-22, 2026-06-22 cont.) left open: every routed follow-up was to
> "appear as a tracker id (or an explicit 'still deferred') at the next review." All seven
> were unfiled. Each was re-reviewed with the human and the code-bug claims re-verified
> against `getwyrd/wyrd` `origin/main` @ `82be6ae` before filing (verify-don't-recall).
> No contribution disposition is re-decided.

## Follow-ups resolved (now filed)

- **Stale/unreadable `$PDCA_TARGET` drifts reviewer grounding** (from 2026-06-22 cont.) —
  filed **eduralph/pdca-harness#120** (upstream; the deterministic pin/fetch-to-`origin/main`
  fix that the downstream agent-skill caveat only backstops).
- **Tier-1 / Tier-2 fault testing must be implemented** (from 2026-06-22 cont.) — re-verified:
  not built on `main` (no `WYRD_TIER1`/`WYRD_TIER2` runner; #146/M3.8 landed only Tier-0 DST),
  so this is net-new *implementation*, not "remove inert scaffolding." Filed as two issues,
  each with an off-Check harness-code leg + a privileged-CI leg: Tier-1 **getwyrd/wyrd#195**
  (M3.9, dm-flakey/dm-error + Jepsen), Tier-2 **getwyrd/wyrd#196** (M3.10, kill-and-reconstruct).
- **#144 reconstruction telemetry** (from 2026-06-22) — filed **getwyrd/wyrd#197**. Re-phrased
  from the log's wording after verification: `emit_repaired` fires upfront for every plan
  (`reconstruction.rs:163-172`), and the `Aborted` arm is offset by nothing, so the module's own
  `successes = repaired − conflict` identity (`:432-433`) over-counts by the Aborted count;
  `time_to_repair` is an absolute instant (`:436`), not elapsed.
- **#143 read-path `chunk_id` recheck asymmetry** (from 2026-06-22, scope was "contested") —
  filed **getwyrd/wyrd#198** as a bug. Verification removed the contest: `repair.rs:50-51`
  documents the verify as *shared with the read path*, but `read.rs:138`/`:176` check only the
  checksum — a misplaced-but-intact fragment is silently decoded. The module's own stated
  invariant is violated.
- **#144 crash-safety coverage gap** (from 2026-06-22, was an open Act item) — filed
  **getwyrd/wyrd#199** as a tracked M3.6 test-debt item (crash between fragment writes and the
  CAS commit; reader concurrent with the commit window).
- **Reviewer Basis must state context + impact, not re-derive the diff** (from 2026-06-21) —
  filed **eduralph/pdca-harness#121** (upstream; generic to every rendered reviewer instance).
- **#117 post-merge throughput/scaling measurement** (from 2026-06-21) — filed
  **getwyrd/wyrd#200** so the deferred real-hardware measurement stays auditable.

## Still open (not in this session's scope)

- Two frozen bundles have not yet had an Act review: **issue_115** (ACCEPTED, 2026-06-20) and
  **issue_153** (discontinued / handed off, 2026-06-21). → next Act review.

## How effectiveness will be judged

- The prior reviews' watch-items stand unchanged; the seven follow-ups are now auditable as
  tracker ids — the next review should see them progressed/closed, not silently dropped.

# Act review — 2026-06-22 (cont.) — cycles considered: issue_145, issue_146

> These two froze after the earlier 2026-06-22 review (which covered #139–#144). This
> entry considers only the newly-frozen pair; it does not re-open the earlier entry's
> deltas or follow-ups.

## What the cycles' records exposed

- **The review/verify TARGET drifts from the base the gates actually ran against — a stale
  or unreadable `$PDCA_TARGET` made the reviewer fabricate / lose grounding (2×: #145, #146).**
  In #145 the reviewer grounded citations on a **stale local `../wyrd`** checkout (`aaee133`,
  pre-#144) and produced a *false-blocking* C4 headline — "patch cannot apply/compile against
  the target" (`SUMMARY.md:40-66`) — yet the patch applies cleanly to `origin/main` (`41c8165`,
  which carries the declared `Depends on: 144`), and the gates (C4-ci, C4-verify) ran green off
  that base in `$PDCA_WORKTREE`. The reviewer's own §10 self-nominates the fix: *"Pin the review
  target to `origin/main` (or fetch first) so a stale checkout can't fabricate ordering-gate
  blockers."* In #146 the same surface failed the other way: `$PDCA_TARGET` "was not readable
  in this environment (`env`/`printenv` were denied)", so the reviewer correctly grounded on
  `patch.diff` alone (`SUMMARY.md:42-43`) — no fabrication, but no target grounding either.
  Root cause is deterministic-harness setup: `$PDCA_TARGET` is resolved to the human's sibling
  `../wyrd` checkout (INTEGRATION.md §2), which can lag `origin/main` (the base the worktree +
  `../wyrd-verify` actually use) or be sandbox-unreadable. Two failure modes, one cause.
- **A "deferred (off-Check) posture" label let an ABSENT deliverable pass (#146).** The prior
  review's `Verification posture` field (added 2026-06-21) converts a surprise C2/C4 into a
  pre-declared sign-off item — working as intended for #146's Tier-0 born-at-tier suite. But
  #146's §10 exposes the flip side: Tier-1 (dm-flakey/dm-error + Jepsen) and Tier-2 (single-node
  kill-reconstruct) landed as **inert dispatch scaffolding only** (`xtask/src/faults.rs` runners
  exit unless `WYRD_TIER1`/`WYRD_TIER2` is set, `patch.diff:1289-1292`, `:1429-1440`), yet the
  deferred-posture label carried them through Check as if merely unverifiable-here. As the
  bundle's own §10 puts it: *"no forcing function distinguishes 'can't be verified here' from
  'isn't built'."* That is a gap in the field the prior review added — `deferred` was readable as
  *unbuilt*, not only *built-but-off-Check*.
- **C5 / T5 / V remain NEEDS-HUMAN by design (both cycles).** Always-human (INTEGRATION.md §4);
  **no delta warranted** — consistent with both prior reviews.

## Process deltas

- Spec template: **`Verification posture` tightened with a deferred-≠-unbuilt forcing function**
  — a deferred/off-Check posture is ONLY for code that EXISTS but can't be verified here; it must
  not wave through an unbuilt deliverable. Plan must state what IS built and exercised at Check vs.
  what is deferred, and confirm the deferred deliverable is itself built and exercised by something
  at Check (e.g. unit tests over the harness code), never inert dispatch scaffolding; a
  not-yet-implemented tier/job is a SEPARATE work item, not a deferred-verification line. Directly
  actions #146's §10 "no forcing function" gap.   (`templates/brief.md.tpl` — `Verification posture`)
- Agent skills: **stale-/unreadable-target grounding caveat** — a SET-but-stale `$PDCA_TARGET`
  (its base lacks a declared `Depends on` the worktree/gates already ran against, off `origin/main`)
  is a target-state caveat, not a patch defect; the reviewer notes the staleness and grounds the
  affected citations on `patch.diff`, and must NOT present a stale-/unreadable-target "patch cannot
  apply/compile" as a blocking C4 FAIL. Backstops the harness fix below; actions #145's §10.
  (`AGENTS.md` "What you do"; `.claude/agents/reviewer.md` "What you do")

## Follow-ups routed (not process deltas — work handed to an owner)

- Harness/driver issue (root cause of #145/#146, upstream): the deterministic fix is to pin/fetch
  `$PDCA_TARGET` to the SAME base the gates run against (`origin/main`) before the reviewer leaf
  runs — so a stale or unreadable sibling checkout cannot drift the reviewer's grounding from the
  worktree. This is harness machinery (`src/pdca_harness/**` worktree/target resolution), so it
  routes **upstream to the template** the harness is rendered from (the agent-skill delta above is
  only the backstop). → owner: Eduard; next step: open a harness/template-feedback issue when
  bumping the harness; record the id next review.
- Work/design item (#146 Tier-1/Tier-2 split): the two higher tiers are not functionally
  implemented — only inert scaffolding landed. Each must be split into (a) an **off-Check
  harness-code** item (Tier-1 needs root + device-mapper / Jepsen harness; Tier-2 needs a real
  node — NVMe/fsync, docker — and ADR-0016 keeps `xtask ci` unprivileged) and (b) a **privileged
  CI job** that runs it green; do not fold them into a deterministic-worktree slice with a single
  C4 DoD. Not a process delta — scheduling/scoping work. → owner: Eduard; next step: file as new
  M3 implementation issues on getwyrd/wyrd; record ids next review.

## How effectiveness will be judged

- The next cross-cycle / dependency-ordered slices (a fix that `Depends on` an unmerged sibling)
  should NOT produce a "patch cannot apply/compile against target" C4 NEEDS-HUMAN that turns out
  to be a stale-checkout artifact. Watch the next ~5 dependency-chained cycles; if it recurs after
  the agent-skill delta, the harness pin (routed above) is overdue.
- The next deferred-posture brief should name what is built-and-exercised-here vs. deferred, so an
  unbuilt tier surfaces as its own work item — not a green-by-deferral §6 row. Watch the next
  off-Check / multi-tier cycles.
- The routed harness issue + the #146 tier-split issues should appear as tracker ids (or an
  explicit "still deferred") at the next review — follow-ups stay auditable.

# Act review — 2026-06-22 — cycles considered: issue_139, issue_140, issue_141, issue_142, issue_143, issue_144

## What the cycles' records exposed

- **Seam built ahead of its production consumer — the "production-wired" claim is honoured
  only by a test double / hand-authored fixture / in-process stand-in (3+ of 6: #139, #141,
  #142, #144).** These M3.x (proposal-0005) foundation slices build a seam, but the live
  path still collapses to the old behaviour while the BINDING criterion is met only off the
  production path: #139 — production record is inert (`WritePlan::chunk_refs` hardcodes the
  identity vector, read routes `index % n`, `get_fragment_at` defaults to ignore `dserver`);
  the *only* honouring consumer is the `Fleet` test double, and Property 2 reads a
  *hand-authored* record bypassing the write path (§6 C5/T5). #141 — its own §10 candidate:
  criterion (3) "production write wired" accepted at the **library/test level only**;
  live-CLI placement deferred because "a discovery-driven gateway write must exist first" —
  and it explicitly asks Act to *"re-scope future 'production wiring' criteria to a reachable
  write path."* #142/#144 — in-process Option-A green; #144's repair-vs-serve seat
  "referenced but not wired" (priority only orders the drain, `patch.diff:420`). Each recurs
  as a *surprise* C5/T5 NEEDS-HUMAN ("is a seam whose sole honouring consumer is a test
  double causally sufficient?"). This is a **different axis** from the existing `Verification
  posture` field: there the worry is red/green *observability* at Check; here the test is
  green — the open question is whether *production* reaches the seam at all. The brief had no
  slot for Plan to pre-declare consumer reach, so it kept surfacing as adjudication.
- **The prior review's `Verification posture` delta is taking effect — no new delta, record
  as evidence.** #142's brief pre-declared its net-new born-at-tier red + in-process green as
  "a pre-agreed sign-off item, not a NEEDS-HUMAN surprise" (`brief.md:143-145`) — exactly the
  intended conversion of a surprise C2 into a declared sign-off item. (#141, also 06-21, still
  raised C2 as a net-new surprise — its brief predates the field.) Keep watching.
- **Validation fitness-to-purpose is NEEDS-HUMAN in all 6 cycles — working as designed.**
  Always-human (INTEGRATION.md §4); **no delta warranted**, consistent with 2026-06-21.

## Process deltas

- Spec template: **new `Production reach` field** — Plan declares up-front when a slice builds
  a SEAM ahead of its production consumer, so the BINDING criterion is honoured only by a test
  double / hand-authored fixture / in-process (Option-A) stand-in while the live path still
  collapses to the old behaviour. It names (a) what honours the seam now vs. what production
  still does, (b) where the production wiring lands and what must exist first, and (c) that the
  double exercises the seam load-bearingly (not dead scaffolding) — converting the recurring
  "test-double-only seam causally sufficient?" C5/T5 question into a *pre-declared* sign-off
  item. Directly actions #141's §10 self-nomination.
  (`templates/brief.md.tpl` — new `Production reach`, after `Verification posture`)

## Follow-ups routed (not process deltas — work handed to an owner)

- Another bug (Wyrd): **#144 telemetry/accounting inaccuracies** — `time_to_repair` emits the
  absolute logical instant `now_millis`, not an elapsed window (self-declared placeholder,
  `patch.diff:704-706`); `reconstruction_repaired` over-counts the Aborted path (counted in
  repaired, subtracted by neither conflict nor anything). Real code defects, out of scope to
  fix here. → owner: Eduard; next step: file against getwyrd/wyrd Issues (`Fixes #` cross-link
  per INTEGRATION.md §1); record id next review.
- Another bug (Wyrd, contested in-scope): **#143 read-path header-recheck asymmetry** — scrub
  verifies checksum AND `header.chunk_id == chunk` (`patch.diff:203-205`) but the read path's
  inline decode re-checks only checksum, so a misplaced-but-intact fragment is silently fed to
  the decoder on read. Whether in-scope is contested (binding leg 4 names only checksum). →
  owner: Eduard; next step: confirm scope at the next M3 read/repair slice; file against
  getwyrd/wyrd if confirmed.
- Open Act item: **#144 crash-safety coverage gap** — code is structure-correct (rebuilt
  fragments written before the single CAS commit; displaced orphaned in the same commit) but no
  test exercises a crash between the fragment writes and the commit, nor a reader concurrent
  with the commit window. Test debt, not a system change. → owner: Eduard; next step: carry as
  a coverage item for the M3.6 follow-on; revisit next review.

## How effectiveness will be judged

- The remaining proposal-0005 slices that wire seams into the live path (relocatable fan-out,
  custodian-aware routing, the deferred discovery-driven gateway write) should carry a
  `Production reach` line, so the "sole honouring consumer is a test double" question lands as a
  *pre-declared* sign-off item — not a surprise §6 C5/T5. Watch the next ~5 seam/foundation
  cycles for recurrence.
- The routed #144/#143 items should appear as tracker ids (or an explicit "still deferred")
  at the next review — the follow-ups must stay auditable, not silently drop.

# Act review — 2026-06-21 — cycles considered: issue_116, issue_117, issue_150, issue_151, issue_152, issue_154, issue_155

## What the cycles' records exposed

- **C2/C4 cannot be demonstrated at Check for net-new / environment-gated work
  (4 of 7: #116, #117, #150, #151).** `C2 Reproduction (red pre-fix)` is
  `result:"none"`, oracle `"(no gate configured)"` in *every* bundle's
  `check-gates.json` — never machine-checked, always a human call (by design). But these
  four recur for one *structural* reason the human keeps re-adjudicating: the work is
  **net-new coverage/infrastructure**, not a defect-to-remove, so there is no failing test
  to flip. "Red" rests on file non-existence / criterion-absence (#116 new `network.rs`;
  #117 born-at-M2 tier `tier2_integration.rs:236-245`; #151 net-new gate), and the green is
  observable only off-Check — Docker host / `WYRD_DSERVER_ENDPOINTS` / a live GitHub
  Actions PR / real hardware (#117, #150, #151) — so the shipped test is *inert* at Check
  (and #116's fault injection was flagged "not proven load-bearing", `network.rs:689`). The
  brief template assumed a defect with a flippable repro and had **no slot** to declare an
  inherently-deferred verification posture, so each cycle re-raised C2 as a *surprise*
  NEEDS-HUMAN.
- **Brief prose read as binding mechanism; builder reasonably diverges (3 of 7: #116,
  #152, #155).** The template already forbids naming a mechanism in *Scope*/*Invariant*,
  but the divergences came through *Success criterion* prose and Scope wording: #152
  "README additions only" (builder added a Rust test `readme_dev_section.rs`), #155
  "composing `Gateway` over `FanoutChunkStore<GrpcChunkStore>`" (builder deliberately
  bypassed `Gateway`, `cli.rs:448-487`), #116 named a three-property suite (one re-run,
  `network.rs:861`). Each forced a human "is this divergence acceptable" call.
- **Reviews skew implementation-heavy.** Across cycles the reviewer's per-item Basis tends
  to re-derive the diff rather than state the *context and impact* the human's sign-off
  decision turns on — making §6 NEEDS-HUMAN rows describe code instead of naming the
  decision owed. (Human observation at this review.)
- **Validation fitness-to-purpose is NEEDS-HUMAN in all 7 cycles — working as designed.**
  It is an explicit always-human item (INTEGRATION.md §4); **no delta warranted** there.

## Process deltas

- Spec template: **new `Verification posture` field** — Plan declares up-front when C2's
  red is criterion-absence vs a flippable assertion, and when the green is observable only
  off-Check; names where/who confirms the deferred green and asks Do to capture a
  *demonstrated* red where feasible.   (`templates/brief.md.tpl` — `Verification posture`,
  after `Test file`)
- Spec template: **`Success criterion` clarified** — state the BINDING observable
  condition; any named mechanism/component/API/file is marked BINDING or merely
  ILLUSTRATIVE, so Do diverging on mechanism (binding condition still holding) is a Do call,
  not a scope NEEDS-HUMAN.   (`templates/brief.md.tpl` — `Success criterion`)
- Agent skills: **reviewer Basis must state context + impact, not re-derive the
  implementation** — for NEEDS-HUMAN rows especially, name the decision owed and why it
  matters.   (`AGENTS.md` "What you do" bullet; `.claude/agents/reviewer.md` verdict-table
  note, line 62)

## Follow-ups routed (not process deltas — work handed to an owner)

- Harness/template feedback (upstream): the reviewer-Basis agent-skill delta above is
  *generic* (every rendered instance benefits, not just Wyrd). Propagate it upstream to the
  template the reviewer contract is rendered from, so it does not drift instance-only.
  → owner: Eduard; next step: open template-feedback issue when bumping the harness.
- Open Act item: **#117 §10 Q6** — throughput/scaling numbers deliberately deferred to a
  post-merge measurement on real hardware off the nightly lane. Not a system change; a
  tracked work item. → owner: Eduard; next step: file against the Wyrd tracker (per
  INTEGRATION.md §1) or carry forward; revisit next review.

## How effectiveness will be judged

- The next net-new / environment-gated cycles (Tier-2 container, DST, CI-gate work) should
  carry a `Verification posture` line so C2/C4 land as a *pre-declared* sign-off item — not
  a surprise §6 NEEDS-HUMAN. Watch the next ~5 cycles for recurrence of "inert test / red
  rests on non-existence".
- Mechanism/scope-divergence NEEDS-HUMAN (the #152/#155 shape) should drop once briefs mark
  named mechanisms BINDING vs ILLUSTRATIVE.
- §6 rows should read as decisions-owed (context + impact), not diff restatements.
