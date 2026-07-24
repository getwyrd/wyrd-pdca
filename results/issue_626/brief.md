# Brief — issue 626 / multipart-commit-protocol (iteration 2)

> The Plan artifact (docs 02 §PLAN). Do reads ONLY this file. Field labels are parsed by
> the driver — keep the `- **Label:** value` shape. All target citations are grounded on
> **getwyrd/wyrd origin/main @ cd82a29** (unchanged since iteration 1 — re-verified at this
> Plan by `git -C ../wyrd fetch --dry-run`, nothing to fetch) and were re-verified by
> reading that commit — none is carried from the tracker or from memory.
>
> **The deliverable of this bundle is a DESIGN DOCUMENT, not code.** Do REWORKS draft
> proposal `docs/design/proposals/draft/0016-multipart-commit-protocol.md` (ADR-0037's
> vehicle: editable while `status: draft`) that settles the multipart commit protocol.
> #508 (the implementing slice) is blocked on this document; #625 (the reaper) is the
> implementing slice of its reaper design.
>
> **THIS IS ITERATION 2 — a re-plan, not a re-do.** Iteration 1's artifact went through
> the full cycle and was REJECTED at sign-off (2026-07-23): the gating T4 batch review
> went red with 11 blocking findings, the reviewer failed C5/T2/T4, and the adversary
> landed seven refutations — all preserved in `iteration-v1/` in this bundle. The human's §9 recorded
> a SCOPE CHANGE (chunk-map segmentation enters scope: the release that ships multipart
> MUST support objects over 10 GiB — maintainer decision) plus eight BINDING settled
> directions (§Binding sign-off directions below — the human's calls; record and apply,
> do NOT re-derive or relitigate them). Plan's structural call (delegated by §9):
> **expand 0016 in place** — segmentation is designed WITH the bounded-obligation
> machinery of decision 4 as one pattern family, not beside it in a companion proposal;
> one document keeps one failure-mode register (a split register is two half-registers
> for the adversary to attack).
>
> **Starting artifact:** `$PDCA_BUNDLE/iteration-v1/patch.diff` — apply it to
> `$PDCA_WORKTREE` (`git apply`) and rework the document. Do NOT start from scratch: the
> iteration-1 adversary explicitly could NOT refute (preserve these load-bearing parts):
> the fence/epoch state machine (ABA, lost-CAS, rollback races), the O(1) publication
> proof of decision 1.3 (session-precondition ⇒ part-set immutability), X29's GC
> reference-before-orphan precedence, and the `PendingEntry.owner` serialization-identity
> treatment. Rework what was refuted: the bounding story (admission, `pending:`/`retire:`
> cardinality), session lifetime, the restore trace, the clock-lifecycle table, the
> reaper's edge modes, the arithmetic — per the directions and registers below.

- **Slug:** multipart-commit-protocol
- **Kind:** enhancement (design artifact — draft proposal per ADR-0037)
- **Defect:** The multipart **commit protocol** — what happens *underneath* a
  CompleteMultipartUpload — is unsettled, and #508 is unimplementable until it is. Multipart
  is the first consumer of several metadata-layer contracts written for a different shape of
  write (a single streaming upload under a 30 s lease), and it breaks each one's stated
  assumptions: publication is lease-conditional and TTL-timed
  (`crates/core/src/metadata.rs:763-793`, `crates/server/src/lib.rs:53`); the maintenance
  planes gate on **committed** state only — GC / restore / scrub / drain share a
  committed-only reference set (`crates/custodian/src/gc.rs:217-228`), and reconstruction /
  rebalance independently scan committed inodes; the metadata model assumes small records
  far inside the inherited 10 MB / 5 s transaction envelope (`crates/traits/src/lib.rs:744-758`);
  and batches are explicitly non-idempotent with replay safety the caller's to design
  (`crates/traits/src/lib.rs:833-843`). Iteration 1 produced a full draft whose publication
  fence held under adversarial review but whose BOUNDING story did not: admission was
  scan-then-create (not atomic), the `pending:` and `retire:` namespaces escaped the
  admission formula, session lifetime was unbounded by design (contradicting the reaper and
  the drain bound), the restore trace could mint a publication over reclaimed bytes, and the
  document's own arithmetic — never computed — put the no-segmentation object ceiling at
  ~165–390 MiB (default 1 MiB chunks), below the 5 GB single-PUT ceiling the Motivation
  claims to surpass and far below the >10 GiB the release now requires.
- **Success criterion:** two legs, both evaluated at Check on the patched tree.
  **(A) Artifact + mechanical validity.** The patch adds/changes exactly two docs paths and
  nothing else: (1) the reworked draft proposal
  `docs/design/proposals/draft/0016-multipart-commit-protocol.md`, keeping
  `docs/design/templates/proposal.md`'s frontmatter and section set, with `type: proposal`,
  `status: draft`, `author: Eduard Ralph`, `tracking-issue:` #626; (2) its row in the index
  table of `docs/design/proposals/README.md` (status `draft (settlement: #626)`, mirroring
  0014's draft-row shape). Two sub-legs with different deciders:
  **(A1, mechanical)** `cargo xtask ci` (the gating `C4-ci` row) passes on the patched
  worktree with the prose gates actually executing — `typos`, the docs lint, and the
  `render_site --check` dangling-link audit (probes re-verified installed at this Plan:
  typos-cli 1.48.0, markdown-it/yaml import OK — these legs run rather than warn-skip).
  Honest bounds: the audit fails on a dangling or wrong LINK but cannot see a MISSING index
  row, a wrong number, absent frontmatter, or a dropped section. **(A2, inspected)** Those
  are a Check-time inspection checklist for the reviewer and the human.
  **(B) Settlement bar** (the load-bearing leg; judged by the Check reviewer and adversary
  leaves — which read THIS brief and are bound by the Refutation standard below — and by
  the human at sign-off. The gating `T4-batch-review` runs 3 fresh codex passes armed with
  the target `AGENTS.md` rubric over the raw diff; it blocks publish on untriaged findings
  but is NOT handed this predicate — defence in depth, never leg-B enforcement). Leg B
  requires ALL of:
  (i) for EACH of the **seven** decision areas (§Decisions below — the six of iteration 1
  plus Decision 7, segmentation) the proposal states the decision taken, the invariant it
  preserves, and an enumerated **failure-mode table** — for every known way to implement
  that decision wrong, a named observable that would fail — written so #508's next Plan can
  lift its success criterion directly;
  (ii) each known failure mode **F1–F18** (§Known failure modes below; F11–F18 are new,
  distilled from iteration 1's rejection) is either **eliminated by the design** or
  **recorded as an explicit accepted cost** with rationale, a stated bound, and a named
  follow-up — accepted-cost disposition is available ONLY for bounded availability /
  latency / capacity / operational trade-offs; an execution exhibiting outcome (a)–(d) of
  the Refutation standard may NEVER be disposed as an accepted cost by the builder — if
  genuinely unavoidable it is a flagged NEEDS-HUMAN sign-off question, stated as such;
  (iii) every one of the eight **binding sign-off directions** (§Binding sign-off
  directions) is APPLIED in the design — they are constraints, not open questions; a
  direction the design cannot honour is a flagged NEEDS-HUMAN question, never silently
  dropped or re-derived differently;
  (iv) the **arithmetic is computed and stated as real numbers** in the accepted-costs
  register: the object-size ceiling per chunk-size choice under the settled encoding rule,
  the segment/root ceilings segmentation introduces, and the concurrent-session capacity
  that falls out of the reworked bounding formula — a bound stated symbolically where a
  number is computable is a leg-B failure (iteration 1's "well under S3's 5 TiB" obscured
  a ~165–390 MiB reality);
  (v) each of the **11 carried T4 findings** (§Carried T4 findings) is resolved in the
  reworked text — the T4 gate re-runs fresh passes over the new diff, so an unresolved one
  re-arises mechanically and blocks publish;
  (vi) the **sequencing consequence** is answered normatively (a state whose only exit is
  the reaper forces #625 with/before #508 — stated as a requirement, not an aside);
  (vii) none of the seven decisions nor F1–F18 is parked in Open questions
  (template-sanctioned open points only for matters that do NOT gate #508, each stating WHY
  it is non-gating and naming its owner or follow-up).
  **Refutation standard (what makes B red):** a reviewer constructs a concrete execution
  under the proposed protocol — a crash point, a lost CAS, a race, an operator
  drain/restore/decommission, a clock-epoch mismatch, a segment-write crash — that
  (a) strands bytes or metadata with no bounded reclamation path, (b) leaves a state no
  verb, pass, or documented driver can exit, (c) publishes or preserves a chunk map over
  bytes a maintenance pass may reclaim or has reclaimed, or (d) installs an obligation
  exceeding the inherited transaction envelope — and that execution is **absent from the
  proposal's execution register / failure-mode enumeration**. One such execution =
  criterion not met. Outcomes (a)–(d) are non-negotiable.
- **Falsifiability:** RED is producible at Check on this host, no cluster or topology
  needed — proven, not hypothesized: iteration 1 of THIS bundle went red exactly here
  (T4-batch-review: 11 blocking findings; reviewer C5/T2/T4 FAIL; seven adversary refutations
  under this Refutation standard — `iteration-v1/check-*.md`). Leg A1 goes red
  mechanically: `C4-ci` runs in `$PDCA_WORKTREE`, and this host has typos-cli 1.48.0 and
  the renderer imports (re-verified at this Plan), so a typo, a dangling relative link, or
  an index row naming a missing file fails the gating check for real. Legs A2 and B go red
  at the judgment tier: the Check reviewer and the difficulty-gated adversary read this
  brief and apply the Refutation standard; final arbiter is the human — a proposal change
  is a by-design NEEDS-HUMAN §6 row (INTEGRATION §4). Note honestly: the per-fix
  `C4-verify` row exits 0 **vacuously** for a patch touching no crate ("docs/CI only —
  nothing to verify per-fix"), and `C5-mutants` reports "no Rust source files" — both
  carry no evidence either way and must not be read as green proof (so it was recorded in
  iteration 1's gates, correctly).
- **Invariant to restore:** For the defect category **assembled writes — writes whose
  durable staging outlives any single request/lease window** (multipart now; server-side
  copy #504 step 2 and any future resumable write share the shape), the metadata layer's
  documented safety contracts must hold **by design, not by assumption**:
  (1) a chunk map is never published over bytes any maintenance pass is free to reclaim —
  the #490 obligation (`crates/core/src/metadata.rs:763-793`) — including after a metadata
  RESTORE (a records-only publication proof must not outlive the records' truth);
  (2) every durable byte is at every instant classifiable as committed-referenced,
  staged-with-an-exit, or garbage-with-a-sound-reclamation-path — no fourth category
  "protected forever by residue" (`crates/custodian/src/gc.rs:217-228` is today's
  two-class world; multipart adds the third class and must give it an exit);
  (3) no lifecycle state is absorbing, and no residency is unbounded — for every state
  some verb, pass, or documented driver exits it, and one administrative ceiling bounds
  total session residency (publication correctness stays record-proved, never timer-proved);
  (4) every obligation installed at publication or teardown is drained in **bounded work**
  (`crates/traits/src/lib.rs:744-758`; the bounded-batch precedent
  `crates/custodian/src/restore.rs:92-100`), replay safety is built into each batch
  (`crates/traits/src/lib.rs:833-843`), and **every namespace the protocol grows is
  bounded by an enforced admission formula or accessed only through bounded key ranges**
  — `MetadataStore::scan` fails loud above `SCAN_CAP = 1 << 20` with no partial result
  (`crates/traits/src/lib.rs:286-292`), so an unbounded namespace is a custodian-plane
  halt, and a bound that concurrent racers can collectively overshoot is not a bound.
  SELF-TEST: not satisfiable by guarding a single module — this is a protocol property
  spanning core, custodian, and server; the proposal's job is to make it hold across
  every maintenance consumer at once.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Difficulty:** high — blast-radius as held-in-view surface, not diff size: the diff is
  two docs files, but the decisions bind the entire publication + custodian contract
  surface (core metadata, GC, restore, scrub, reconstruction, drain/rebalance, the store
  trait envelope, the S3-visible lifecycle — now plus the published object-map record
  shape itself, via segmentation). Rated up deliberately so the adversary leaf fires.
- **Do model:** opus-xhigh
- **Scope:** ONE logical change: REWORK draft proposal 0016 (starting from
  `iteration-v1/patch.diff`) so it settles: (1) the publication-time proof for assembled
  writes, (2) the protection class for durable-but-unpublished bytes and its per-consumer
  visibility, (3) lifecycle states and failure semantics — now including the `W_session`
  residency ceiling, (4) the bounded-work pattern for unbounded objects, (5) reclamation
  evidence for failed in-flight work, (6) the abandoned-upload reaper — protocol half AND
  algorithm (per iteration 1's scope extension; #625 stays the implementing slice), and
  (7) **chunk-map segmentation** — NEW in scope by maintainer decision at sign-off
  2026-07-23: the release that ships multipart MUST support objects over 10 GiB, the
  honest no-segmentation ceiling is ~5–10 GiB even with large chunks, so FU-1 graduates
  from parked follow-up into this proposal (designed WITH decision 4's staged-obligation
  machinery as one pattern family — a segmented map cannot be published in one batch, so
  segmentation needs its own staged publication: write segments, then root flip); plus
  the normative implementation-order answer for #625 vs #508; and the updated index row.
  The proposal **decides**; it does not implement. / Out of scope: any change under
  `crates/` or `xtask/` (no code, no tests); the reaper's operational knob VALUES and
  metrics/alert/CLI wiring (0016 settles each correctness-relevant knob's VALID RANGE and
  bounding invariant; #625/#508 choose values inside it); authoring ADRs (the proposal
  RECOMMENDS which decisions graduate — segmentation's record shape is expected ADR-scale
  — but authoring them is follow-up work under ADR-0037); editing ANY accepted/stable
  document (host `docs-immutability` gate); rewriting #508's brief (that happens at
  #508's next Plan, consuming 0016); relitigating the ETag basis (ADR-0047) or the S3
  wire surface (stable across all review rounds — remains #508's).
- **Repro instruction:** Not a runtime defect — the reproduction is that no implementable
  success criterion exists for #508's commit half, and iteration 1's artifact demonstrably
  failed its own settlement bar: (1) `$PDCA_BUNDLE/review-batch.md` (the iteration-1 T4
  output; overwritten on each T4 run) — 11 findings, all blocking, none rejected; (2) the sharpest new trace (adversary, iteration 1): snapshot
  taken while a session is Open with staged parts → session aborted/reaped → retirement
  drain orphan-marks → GC reclaims the fragments → metadata restored to the snapshot →
  the session and its `part:` records resurrect, the orphan evidence is rewound away, and
  a retried Complete passes the records-only publication proof and **publishes a chunk
  map over bytes GC has reclaimed** — outcome (c), absent from iteration 1's execution
  register; (3) the arithmetic trace: iteration 1's own knob rule
  (`max_chunkref_bytes × MAX_MAP_CHUNKS ≤ V/2`) against its own encoding estimate
  (~131–302 B/ChunkRef) forces ~165–390 chunks ⇒ a ~165–390 MiB object ceiling at the
  default 1 MiB chunk size (`crates/server/src/lib.rs:51`) — below the 5 GB single-PUT
  ceiling its Motivation claims to surpass, and far below the >10 GiB launch requirement.
- **External dependencies:** `typos`, `docs-renderer` — the two prose-gate toolchains that
  make C4-ci's spell check and dangling-link audit execute rather than warn-skip on a
  docs-only patch (INTEGRATION §3 laptop/CI asymmetry); both are registered as doctor rows
  under those exact ids and both re-verified installed on this host at this Plan
  (typos-cli 1.48.0; `python3 -c "import markdown_it, yaml"` OK). Nothing else: no
  Docker, no cluster, no live backend, no topology — the deliverable is a document.
- **Test file:** none — docs-only deliverable; no regression test exists or is possible
  for a design document (mechanical validity is the gating whole-tree prose gate; content
  adequacy is judged by the brief-aware Check reviewer and adversary with the human
  deciding — the gating batch review is diff-and-rubric defence in depth, never the leg-B
  decider).
- **Verification posture:** declared posture (a), NET-NEW / born-at-tier: there is no
  prior failing assertion to flip — "red" for leg B is criterion-absence under the
  Refutation standard, exercised by review (and it HAS fired: iteration 1 went red on
  exactly this predicate). What IS exercised at Check: leg A1 mechanically by the gating
  `C4-ci` prose gates; legs A2 and B by the brief-aware Check reviewer and the claude
  adversary under the Refutation standard, with the human deciding; the gating
  `T4-batch-review` (3 fresh codex passes, diff + rubric only — it never sees this brief)
  as defence in depth. `C5-mutants` and `C4-verify` are vacuous on a no-code diff and
  carry nothing. Deferred beyond Check: ONLY the governance act of accepting the proposal
  (`draft` → `accepted`, architecture-board / founding-maintainer authority per
  GOVERNANCE; ADR-0037) — the bundle ships the artifact at `status: draft`.
- **Citations expected:** Do must cite `path:line` on origin/main @ cd82a29 for every
  contract the proposal binds against (start from the verified citations in this brief and
  in the iteration-1 document; re-cite them in the proposal's own text so it stands alone
  in the target repo). Composition peers Do MAY open (docs edition — the "peer callsite"
  is a peer document): `$PDCA_BUNDLE/iteration-v1/patch.diff` (the STARTING ARTIFACT — see
  header); `docs/design/templates/proposal.md` (the mandated skeleton);
  `docs/design/proposals/draft/0014-milestone-7-failover-and-dr-single-dc.md` (form peer);
  the index-row pattern at `docs/design/proposals/README.md` (0014's row); ADR-0037
  (lifecycle + immutability); ADR-0046 (a new namespace concept gets **real records** with
  key shape / writer / deleter / scan visibility stated — `bucket_key`,
  `crates/core/src/metadata.rs:48-50`); ADR-0047 (ETag settled; its §Consequences bullet
  on multipart; segmentation's record shape is the expected "successor to ADR-0047's
  record shape" ADR candidate); the bounded-batch precedent
  `crates/custodian/src/restore.rs:92-100`; the S3 lifecycle precedent for `W_session` is
  Amazon's `AbortIncompleteMultipartUpload` bucket-lifecycle rule (cite as external
  precedent — days-after-initiation, deployment-side, not client-opt-in).
- **Prior-art check (triage cycles):** By affected path `docs/design/proposals/`: number
  **0016** re-verified free at this Plan (accepted through 0015; drafts
  0006/0008/0009/0010/0014) and the getwyrd/wyrd open-PR list is empty (re-verified — no
  number collision in flight; iteration 1 never published, so no draft PR exists).
  Merged history: no multipart design document has ever existed; draft 0006 names
  resumable/multipart upload as "a separate API proposal" — 0016 IS that proposal;
  ADR-0047 settled the ETag basis and deferred only the multipart composition.
  Closed/rejected work: #508's two rejected Do iterations are archived at
  `results/issue_508/iteration-v1..v2` with a do-not-implement banner on its brief; THIS
  bundle's own rejected iteration is `results/issue_626/iteration-v1/` — the reworked
  document must not silently re-ship any construction that rejection named.
- **Disposition hint:** new-feature

## Binding sign-off directions (§9, 2026-07-23 — the human's calls; APPLY, do not re-derive)

These eight are settled. The design records each as a decision taken; disagreement is a
flagged NEEDS-HUMAN question in the proposal, never a silent alternative.

- **D-A — Session lifetime:** a deployment-wide DEFAULT hard ceiling **`W_session`**,
  measured from session initiation (Amazon `AbortIncompleteMultipartUpload` precedent,
  NOT client-opt-in); per-bucket policy may later tighten below it, never loosen.
  Rewrite decision 1's "no timer on total life" row to: "no CORRECTNESS timer —
  publication is proved by records; one ADMINISTRATIVE ceiling bounds residency" (the row
  as written contradicts the reaper for any idle T ≥ W_open — iteration 1's D1
  failure-mode row demanded idle-then-resume success for unbounded T while decision 6
  reaped it). F6's drain bound re-derives as `W_session`; FU-2 becomes the urgent-drain
  remedy, not the bound. The reaper stays record-only (no topology coupling).
- **D-B — Restore trace (outcome c), KISS:** no resumption semantics across a restore. A
  metadata restore **fences/aborts every session open in the restored image**; an aborted
  upload starts from the beginning. Add the execution-register row (the F13 trace below).
- **D-C — Admission is GUARANTEED, not approximate:** serialized slot reservation — a
  counter CAS'd in the create batch, released in the terminal delete; contention at the
  counter IS the `503 SlowDown` backpressure. This reverses iteration 1's "no hot
  counter" stance for **Create only** (the retry-storm objection applies to part commits,
  which stay counter-free). Rationale: cap overrun halts the maintenance plane —
  data-loss-class, so the bound must be enforced, not observed.
- **D-D — Namespace cardinality (structural rework):** owned `pending:` entries get
  **per-session indexed access** (no global `pending:` scan in the reaper); `retire:` is
  walked in **bounded key ranges** (sharded or cursor-keyed) and its growth from ordinary
  overwrites is bounded or alarmed; **in-flight part concurrency gets an admission cap**
  so the pending population has an enforceable formula; admission counts **ALL session
  records including Completing/Aborting/Completed tombstones** (with their bounded
  retention); drain health (obligation count, oldest age) is a first-class alarm.
- **D-E — Mechanical repairs:** `W_completing` measured from the FENCE instant (stamp it
  in the session record — not from last part progress, which makes a healthy Complete
  begun late immediately rollback-eligible); UploadPart cumulative early-refusal restated
  as BEST-EFFORT with the authoritative check at Complete (concurrent part commits share
  only a read-only session precondition and can collectively overshoot); the
  clock-lifecycle table owns honestly that the reaper evaluates owned-lease liveness
  (abandonment condition ii — the lease stamp IS read by a reclamation decider); add the
  reaper stale-snapshot rule ("the step-5 judgment must be no staler than the entry it
  condemns") with its DST observable.
- **D-F — Honest arithmetic:** state the computed ceilings as real numbers in the
  accepted-costs register — max object size per chunk-size choice, the segmentation
  ceilings, and the concurrent-session capacity that falls out of the reworked bounding
  formula. (Leg B(iv).)
- **D-G — Segmentation in scope (the scope change):** >10 GiB objects required at launch;
  design segmentation's staged publication (write segments, then root flip) WITH the
  retirement-ledger machinery of decision 4, as one pattern family. (Decision 7 below.)
- **D-H — Structure:** one document — expand 0016 in place (Plan's call, delegated by §9).

## Carried T4 findings (iteration 1's `review-batch.md` — resolve EVERY one)

The T4 gate re-runs 3 fresh passes over the NEW diff each cycle: a finding "clears" only
when the reworked text no longer exhibits it. Line anchors are iteration-1 patch lines —
locate by section, not line. None of these is expected to need a recorded rejection; all
eleven are real (they motivated §9's directions).

1. `0016:456` BUG — concurrent UploadPart commits for distinct part numbers share only a
   read-only session precondition, so each can observe capacity below `MAX_MAP_CHUNKS`
   and collectively exceed it. → D-E (best-effort early refusal, authoritative at
   Complete) + D-D (in-flight part cap).
2. `0016:583` BUG — coordination-free admission has no enforced bound on fleet-wide
   concurrent creates; overshoot is unbounded and cannot be absorbed by fixed headroom.
   → D-C (serialized slot reservation).
3. `0016:606` BUG — `retire:` is not bounded by session admission: repeated part
   replacements and losing writers create arbitrarily many uniquely keyed obligations.
   → D-D (bounded key-range walks; growth bounded or alarmed).
4. `0016:583` BUG — scan-then-create admission has no atomic reservation. → D-C.
5. `0016:602` BUG — `Completing` staleness measured from session/part progress rather
   than the fence instant: a healthy Complete begun late is immediately
   rollback-eligible. → D-E (stamp the fence instant).
6. `0016:606` BUG — ordinary overwrites also create `retire:` obligations, so a stalled
   drain can exceed `SCAN_CAP` and prevent all reaping. → D-D.
7. `0016:614` BUG — scanning all `pending:` is not bounded by the session/part admission
   formula (per-chunk entries of in-flight parts; concurrent replacements add more): the
   backstop can fail with `ScanCapExceeded` — the iteration-1 adversary's F7 refutation
   is the same hole seen from the deployed `Defer` posture, where this scan is the ONLY
   reclaimer of owned entries. → D-D (per-session indexed access; in-flight cap).
8. `0016:582` BUG — `scan("mpu:")` includes Completing/Aborting/Completed tombstones, so
   bounding only the OPEN population bounds neither this scan nor the derived
   namespaces while tombstones accumulate. → D-D (count ALL records; bounded retention).
9. `0016:583` BUG — concurrent creates can all observe sub-cap population and commit;
   "headroom" cannot enforce the SCAN_CAP invariant without serialized reservation. → D-C.
10. `0016:651` CONVENTION — the clock table claims lease expiry "decides no reclamation"
    while abandonment condition (ii) has the reaper evaluate exactly that expiry —
    conflicting clock ownership. → D-E (own it honestly; fail direction is safe —
    skew ⇒ false-positive reap ⇒ the fenced, bounded FU-4 cost — so this is a
    conformance repair to F10's deliverable, not a redesign).
11. `0016:651` CONVENTION — same table: "evaluated by nothing that decides reclamation"
    is false for the owned lease read at the abandonment rule. → D-E.

## Context Do needs (condensed; the issue thread has no comments)

Issue #626 (milestone 0.1 Alpha, label `research`) splits #508 in two. The S3 **wire
surface** half — routing, denylist removal, the percent-encoding fence, exact
status/error codes — is stable and NOT in question. What is in question is everything
underneath Complete. Per ADR-0037 the vehicle is a draft proposal (editable while
`draft`); one or two ADRs may later fall out of it for the decisions that outlive
multipart — server-side copy (#504 step 2) and any future resumable-write path have the
same shape; segmentation's record shape is the strongest ADR candidate. Each decision is
"done" when a brief could state a success criterion whose **negations are enumerable**.
Two issue-scope lines are superseded by recorded maintainer decisions: the reaper
algorithm is designed here (iteration 1's Plan), and segmentation is designed here
(this iteration's §9).

## The seven decisions to settle (grounding verified on cd82a29)

1. **The publication contract for an assembled write.** Today "these bytes are still safe
   to publish over" is proved by **lease liveness**: every chunk must hold an unexpired
   `pending:` lease, checked by CAS preconditions riding in the publishing batch
   (`live_lease_guards`, `crates/core/src/metadata.rs:763-793`, issue #490 — absent OR
   lapsed entry ⇒ fail closed with `Conflict`). The TTL is 30 s
   (`crates/server/src/lib.rs:53`), renewed only while a single `stream_write_data` call
   is in flight (`crates/core/src/write.rs:474-500`). A multipart Complete assembles a
   chunk map from parts committed minutes or hours apart: **what replaces lease liveness
   as the publication-time proof?** Iteration 1's answer (records-as-proof + the
   fence/epoch machine) HELD under adversarial review — keep it. What changes (D-A): the
   "no timer on total life" framing is rewritten — no correctness timer; `W_session`
   administratively bounds residency; the D1 failure-mode row that demanded
   idle-then-resume success for unbounded T is re-stated with its bound (T within the
   session's remaining `W_session` budget and the reaper's liveness terms). What changes
   (D-B/F13): the records-only proof must be stated as valid only while the records are
   the truth — a restore that rewinds them fences/aborts the resurrected sessions.
2. **A protection class for durable-but-unpublished bytes, per consumer.** GC's safety
   predicate is "referenced by a *committed* inode chunk map" (`referenced_fragments`
   scans `inode:` only, skips non-Committed, `crates/custodian/src/gc.rs:249-260`;
   pending maps excluded by design `:217-228`). Consumers, verified — TWO KINDS: sharing
   the one `ReferenceSet` — GC's gate (`gc.rs:162`, `:244-246`), restore ("SAFETY GATE,
   identical to GC's", `crates/custodian/src/restore.rs:218-224`), scrub
   (`crates/custodian/src/scrub.rs:95-110`), drain/desired-state
   (`crates/custodian/src/desired_state.rs:157-164`); scanning committed `inode:`
   independently — reconstruction (`crates/custodian/src/reconstruction.rs:313-325`,
   `:182-195`) and rebalance (`crates/custodian/src/rebalance.rs:147-151`). Iteration 1's
   per-consumer table stands, with the F6 drain bound re-derived as `W_session` (D-A) —
   the iteration-1 claim that every staged fragment on a draining server exits within
   `W_open` was refuted (a live, progressing session bounded it by nothing). Decision 7
   adds a new consumer obligation: every pass that resolves inode → chunk map must
   resolve the SEGMENTED shape in bounded work.
3. **Lifecycle states and failure semantics.** Iteration 1's fenced state machine
   (`Open → Completing | Aborting`, monotone epochs, exact-bytes CAS on every transition,
   publish + `Completed` in one batch) survived the adversary — keep it. Repairs: the
   `W_completing` rollback window is measured from the FENCE instant, stamped in the
   session record (D-E); the state machine gains the `W_session` administrative exit
   (D-A) and the restore fence/abort rule (D-B); tombstone retention is bounded and
   counted by admission (D-D). Batches stay non-idempotent with replay safety designed
   into each batch (`crates/traits/src/lib.rs:833-843`).
4. **Bounded work for unbounded objects.** Publication orphans the prior version with one
   `put` per prior fragment in a single batch (`commit_chunk_map_superseding`,
   `crates/core/src/metadata.rs:582-619`): at 1 MiB chunks and RS(6,3)
   (`crates/server/src/lib.rs:49-51`, 9 fragments/chunk), superseding a single 5 GiB part
   owes 46,080 orphan puts — over the envelope (`crates/traits/src/lib.rs:744-758`), a
   PERMANENT publish failure. Iteration 1's retirement ledger (durable obligation
   installed atomically, drained in bounded batches; `MARK_BATCH = 1_000` precedent,
   `crates/custodian/src/restore.rs:92-100`) is the right pattern — keep it; repair its
   cardinality story per D-D (bounded key-range walks; overwrite-driven growth bounded or
   alarmed; drain-health alarm). UploadPart early refusal restated per D-E.
5. **Reclamation evidence for failed in-flight work.** Deployed GC runs
   `ExpiredPendingPolicy::Defer` (`gc.rs:78-105`; the CLI passes `Defer` unless the
   operator attests, `crates/server/src/cli.rs:975-979`), so `pending:` residue is never
   reclaimed in deployment. Iteration 1's answer (ownership tag, loser compensation,
   reference-based backstop) stands EXCEPT its backstop scanned `pending:` globally —
   the F11 hole. Rework per D-D: per-session indexed access; the pending population gets
   an enforceable formula via the in-flight part admission cap.
6. **The abandoned-upload reaper — designed here, implemented in #625.** Iteration 1's
   detection rule (idle past the window AND no unexpired owned lease), rollback of stale
   `Completing`, and bounded-batch teardown remain the skeleton. Repairs, all binding:
   the reaper walks sessions and each session's owned entries via the per-session index —
   NO global `pending:` scan (D-D); the stale-snapshot rule with its DST observable
   (D-E); the clock table owns the reaper's evaluation of owned-lease liveness (D-E);
   `W_session` is the reaper's administrative ceiling arm (D-A — reaper stays
   record-only); the failure-mode table keeps iteration 1's rows (false positive incl. a
   single part streaming past the window; reap racing Complete; crash mid-teardown;
   false negative; sweep bound; arrival-outruns-drain; reaper unavailability) and adds
   the stale-snapshot mode and the tombstone-accumulation mode.
7. **Chunk-map segmentation (NEW — D-G/D-H).** The launch requirement is objects over
   10 GiB; the settled encoding rule forces a no-segmentation ceiling of ~165–390 chunks
   (~165–390 MiB at the default 1 MiB chunk; ~5–10 GiB even at large chunks traded
   against gateway memory `chunk_size × max_concurrent_encodes`). Settle: (a) the
   segmented published-map record shape per ADR-0046 — a root record plus segment
   records, each with key shape, writer, deleter, and which scans see it; (b) **staged
   publication**: segments are written first (bounded batches), then ONE root flip inside
   the publishing batch — the flip is the publication instant, carrying the same
   fence/epoch proof as decision 1 (a segmented map cannot be published in one batch:
   envelope, `crates/traits/src/lib.rs:744-758`); (c) the crash story: a publisher that
   dies between segment writes and root flip leaves segment records that are evidenced
   and reclaimed by the SAME staged/retirement machinery as decisions 4/5 — never a new
   unbounded or unevidenced namespace (one pattern family, per §9); (d) Complete
   idempotency (F5) extends over the segment-write phase — a resumed Complete must not
   double-write or half-flip; (e) every maintenance consumer that resolves a committed
   inode's chunk map (decision 2's full list) resolves the segmented shape in bounded
   work, and reference-set construction stays under `SCAN_CAP` with segment records
   COUNTED in the admission/cardinality formula; (f) supersede/overwrite of a segmented
   object retires the prior generation (segments + fragments) through the retirement
   ledger; (g) the arithmetic (D-F): segment size/count ceilings, the resulting object
   ceiling as a real number per chunk-size choice, and the remaining hard bounds
   (e.g. `MAX_PARTS_PER_SESSION = 10_000` × part size; state the S3 5 TiB comparison
   honestly). Whether the single-PUT path also uses segmentation above a threshold is
   Do's design choice within these invariants — state it either way. Recommend the record
   shape's ADR graduation (successor to ADR-0047's shape).

## Known failure modes the protocol must dispose of (F1–F18)

F1–F10 as iteration 1 (verified grounding unchanged); F11–F18 are new, distilled from
iteration 1's rejection. For each, exactly ONE of the three dispositions leg B defines:
**eliminated** (with the observable that would catch a regression); **accepted** ONLY as
a bounded non-safety trade-off with bound, rationale, and named follow-up; or **flagged**
NEEDS-HUMAN. Silence on any fails leg B.

- **F1 — absorbing terminal state on publication CAS loss** (no crash needed): Complete
  fenced; a concurrent PutObject wins the publish CAS (`metadata.rs:366-382` /
  `:582-619`); with no defined exit the session and its staged bytes are stuck forever.
- **F2 — staging-record disposal at publication:** surviving staging records keep feeding
  the protected set; a later overwrite orphans fragments still protected by stale records.
- **F3 — abort-race residue unreclaimable under `Defer`** (`gc.rs:78-105`,
  `cli.rs:975-979`): the everyday Ctrl-C case; forever-retained residue is outcome (a).
- **F4 — obligation fan-out vs. the transaction envelope** (46,080 orphan puts for one
  5 GiB part supersede; `traits/src/lib.rs:744-758`).
- **F5 — non-idempotent Complete / retry after unknown outcome**
  (`traits/src/lib.rs:833-843`, re-read remedy `:738-745`) — now including the
  segment-write phase (decision 7d).
- **F6 — maintenance-pass visibility split:** invisible staged bytes ⇒ the drain-wipe
  trace (outcome c); visible everywhere ⇒ drain stall — now bounded by `W_session`, with
  FU-2 as the urgent-drain remedy (D-A).
- **F7 — unbounded staged state halts the custodian plane:** `scan` fails loud above
  `SCAN_CAP = 1 << 20`, no partial result (`traits/src/lib.rs:286-292`); reference-set
  construction crossing it aborts the whole reconcile step
  (`crates/custodian/src/reconciliation.rs:75-112`). The bound must hold BY CONSTRUCTION
  under arrival-outrunning-drain and reaper unavailability.
- **F8 — a vacuous or unsound reaper design:** observable must be one the protocol
  writes; detection clock-sound; edge modes enumerated; sequencing answer normative.
- **F9 — client-visible semantics of fenced states unpinned:** exact answers for
  UploadPart/Complete/Abort/ListParts under `Completing`/`Aborting`; Abort's
  bounded-latency response; listing visibility. Now also: what a client sees when
  `W_session` expires a session (D-A) and during a restore fence (D-B).
- **F10 — clock-lifecycle ownership for any new stamp** (`AGENTS.md:132-142`, the
  #557/#565 class; worked precedent `crates/core/src/write.rs:305-311` evaluated at
  `gc.rs:172-176`) — the table must own the reaper's owned-lease read honestly (D-E).
- **F11 — namespace cardinality escapes the admission formula** (iteration 1's central
  bounding hole, three surfaces): (a) owned `pending:` entries are per-chunk of in-flight
  parts with no concurrency bound; (b) `retire:` grows with part replacements, losing
  writers, and ORDINARY overwrites, unbounded by session admission; (c) `mpu:` tombstones
  accumulate outside the open-session count. Disposal per D-D: per-session indexed
  access, bounded key-range walks, an in-flight part cap, all-records admission
  accounting, bounded tombstone retention, drain-health alarms — with the reworked
  formula stated and its capacity computed (D-F).
- **F12 — admission race: scan-then-create is not a bound** — arbitrarily many concurrent
  creates each observe sub-cap population and all commit. Disposal per D-C: serialized
  slot reservation (counter CAS'd in the create batch, released in the terminal delete);
  counter contention IS the 503 backpressure. Create only — part commits stay
  counter-free.
- **F13 — restore resurrection mints a publication over reclaimed bytes** (outcome (c),
  the iteration-1 adversary's sharpest trace — see Repro instruction (2)). Disposal per
  D-B: restore fences/aborts every session open in the restored image; execution-register
  row added. The records-only publication proof is explicitly scoped to unrewound records.
- **F14 — unbounded session residency:** a live, progressing session (or a client
  uploading one part per window) holds staged fragments — and a draining server —
  indefinitely; iteration 1's `W_open` label on the drain bound was unwarranted. Disposal
  per D-A: `W_session` from initiation, deployment default, tighten-only per bucket; the
  drain bound and F6's cost row re-derive from it.
- **F15 — reaper stale-snapshot judgment:** step-5-style filtering against a session list
  older than the entry it condemns reclaims the in-flight entries of a session created
  mid-pass (orphan-marking a live part's fragments, killing its renewals —
  `crates/core/src/write.rs:474-478` refuses rather than resurrects). Disposal per D-E:
  the no-staler rule, its DST observable, and the failure-mode row.
- **F16 — `W_completing` measured from the wrong instant:** progress-relative staleness
  rolls back a healthy Complete begun late. Disposal per D-E: fence-instant stamp in the
  session record.
- **F17 — cumulative admission at UploadPart overstated:** concurrent part commits under
  a read-only session precondition can collectively overshoot `MAX_MAP_CHUNKS`. Disposal
  per D-E: best-effort early refusal, authoritative check at Complete — stated as such —
  plus D-D's in-flight cap where a hard bound is needed.
- **F18 — segmentation's staged publication leaves residue:** a crash between segment
  writes and root flip strands segment records/bytes; a resumed Complete double-writes or
  half-flips; a maintenance pass fails to resolve (or unboundedly resolves) a segmented
  map; a superseded segmented generation drops into the unevidenced void. Disposal:
  decision 7's design (one pattern family with decisions 4/5), each mode in the
  execution register with its observable.

## Settled inputs — record, do not relitigate

- **ETag basis:** ADR-0047 — lowercase-hex SHA-256 opaque change-token; MD5 rejected;
  only the multipart *composition* deferred. Record as settled.
- **S3 wire surface:** stays in #508 (stable across all review rounds).
- **One-clock rule and the Defer posture** (`AGENTS.md:132-142`, `gc.rs:78-105`, #557):
  constraints on the design space.
- **Record pattern:** ADR-0046 — real, first-class records; any new record class states
  key shape, writer, deleter, and which scans see it (existing namespaces:
  `metadata.rs:30-69`; `desired_state.rs:33`).
- **The eight sign-off directions above** — the human's recorded calls.
- **Iteration 1's unrefuted constructions** (fence/epoch machine, records-as-proof, O(1)
  publication precondition, GC precedence, serialization identity) — keep; do not
  redesign what survived the adversary.

## Deliverable mechanics

- **Start from** `$PDCA_BUNDLE/iteration-v1/patch.diff` applied to `$PDCA_WORKTREE`; the
  output is a reworked `docs/design/proposals/draft/0016-multipart-commit-protocol.md` +
  the index row in `docs/design/proposals/README.md` (`draft (settlement: #626)`,
  mirroring 0014's row; relative link must resolve — the render audit checks it).
- **Frontmatter:** `created:` (DD.MM.YYYY HH:MM), `type: proposal`, `status: draft`,
  `author: Eduard Ralph` (human identity only — no model/tool attribution anywhere,
  maintainer's standing rule), `tracking-issue:` #626, `tags:` incl. `proposal`, `s3`,
  `multipart`, `metadata`.
- **Findability:** per-decision failure-mode tables, the F1–F18 disposition list, the
  accepted-costs register WITH computed numbers, the execution register (extended with
  the F13/F14/F15/F18 traces), and the clock-lifecycle table must be findable as such,
  not diffused into prose. The sequencing answer stays normative and findable.
- **Follow-ups:** FU-1 is DISSOLVED into decision 7 (in scope); FU-2 re-labelled as the
  urgent-drain remedy; keep/renumber the remainder coherently.
- **Immutability:** the patch touches only the new draft file and the proposals index —
  NO accepted/stable document (ADR-0037 §CI enforcement; host `docs-immutability` gate).
- **Do not weaken an invariant to make its enumeration easier** — if the design concludes
  a clause cannot be met, surface it as a flagged NEEDS-HUMAN question in the proposal.
  Only sub-safety trade-offs (availability / latency / capacity) may be recorded as
  accepted costs, each with its bound and follow-up — never a silent rescope.

## STOP discipline

Draft only until Check sign-off. The deterministic publish step opens the draft PR on
accept; the PR MUST NOT be marked ready before sign-off accepts — and the proposal ships
at `status: draft`: its ratification (draft → accepted) is a separate, later governance
act under ADR-0037, never this cycle's.

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected on the advisory findings (T4 gate: 7 blocking; reviewer C3/C5/T2/T4 FAIL; adversary refutations). The settled directions (D-A..D-H) and the load-bearing core (fence/epoch machine, O(1) session-precondition publication proof, restore fence, D-F arithmetic) survived attack — keep them. Fix in the same document: 1. Envelope defect: the segment-write and drain batch rows claim "B inside the envelope" but B=1,000 values of up to V=100 KB can reach ~100 MB, 10x the inherited 10 MB transaction bound (0016:314/:322 vs traits/src/lib.rs:750). Replace the count-based B with a byte-derived batch limit so no batch can exceed the envelope; restate the inventory rows in bytes. 2. F18 "Eliminated" is refuted: rollback -> re-Complete while the old retire:records:{seg} obligation is still pending deletes a published object's segment records (outcome (c)). Close it (e.g. the re-fence batch cancels the session's pending seg obligation, or attempt/epoch-scoped segment keys) and extend X37 to cover rollback->re-Complete-while-obligation-pending. 3. F11a is refuted: crashed mid-stream part attempts accumulate owned pending:/sidx: residue unbounded while the session stays Open; past SCAN_CAP the sidx: teardown scan fails and the backstop reclaims nothing. Include unreclaimed owned residue in in-flight accounting, state the MAX_INFLIGHT_PARTS enforcement point (observed-not-enforced was iteration 1's rejection class), and/or give sidx: teardown the cursor-keyed walk retire: already has. 4. Internal contradiction: Complete under Completing is simultaneously 409 (verb x state table, 0016:489) and resume (decision 3 F5 bullet :473, decision 7(c) :880-882). Pick one; if 409 wins, carry the W_completing-rollback recovery latency in the accepted-costs register. 5. Terminal-delete double-decrement: the delete mpu:<id> + CAS mpuctl:count-1 batch is concurrent-capable (gateway inline vs reaper) with no precondition on the session record, so racing drainers can decrement twice, drifting the counter low and breaking F12's "exact" bound. Precondition the terminal batch on the session record's exact bytes. 6. Overstated range claim (0016:1273): at top-of-range MAX_SESSIONS the staged reference build is ~10^10 reads per reconcile pass. Give MAX_SESSIONS a work/memory-coupled bound, not just the scan-cap coupling. Also re-check the F18/F11 disposition rows (:1143/:1136) after the fixes — they were the rows a confirmatory pass would have wrongly accepted.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 7 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected on the advisory review's grounded findings against the reworked 0016 text; the iteration-2 carry-forwards are confirmed closed — do not reopen them. Fix, in the doc: 1. Teardown/fence race (C3/C5/T2): part-intent commits carry no session precondition and terminal deletion does not require sinf == 0 / empty sidx, so an uploader can create owned residue after fence-and-scan that nothing can ever discover. Add a serialization edge between in-flight intent creation and terminal teardown (e.g. terminal delete preconditions on sinf == 0, or intents precondition on session state). 2. Completed-path residue (adversary outcome a): the Completed teardown never walks sidx:, so a crashed part attempt's owned residue is stranded forever when the session Completes. Walk sidx: in the Completed teardown before the terminal delete; add the missing exit-table/backstop/register coverage. 3. Scan-cap contradiction (X25): reconcile_after_restore does a global pending: scan; the doc's own fleet arithmetic (MAX_SESSIONS x MAX_INFLIGHT_PARTS x MAX_PART_CHUNKS) can exceed SCAN_CAP ~52x. Add a fleet-wide owned-pending constraint to the knob table and re-derive the restore pass's scan bound. 4. Drain invisibility (outcome c): a still-streaming part's chunks are invisible to drain/desired-state, so an operator can wipe a server and a later Complete publishes a map naming wiped fragments. Make in-flight owned pending: count as held for drain, or state and register the widened window honestly. 5. DeleteObject of a segmented object (outcome d): inline unlink fan-out at the settled ceilings is 463K-1.78M orphan puts in one batch — permanently over E_tx. Route delete (and bulk DeleteObjects, #509) through the retirement drain; add the batch-inventory row and register coverage. 6. Two smaller fixes: surface the D-C tension (part commits are not counter-free — sinf: CAS at every part boundary serializes same-session part commits) as an explicit flagged question rather than resolving it silently; and either spec the clock_source field on owned pending entries or derive the reaper's clock guard from the session record and fix the clock-table row.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 9 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 4 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected on the foundation of the advisory review and the adversary review; the brief's directions stand — rework the document, do not re-plan. Carry forward: 1. Triage the red T4 gate: all 6 blocking findings in review-batch.md must be fixed or recorded-rejected in review-rejected.md — the gate blocks while any is unchecked. 2. Core defect (reviewer C5/T2/T4, 0016:1419): admission must MECHANICALLY enforce the aggregate W_ref bound. A flat per-session counter is distribution-dependent (~104 small-part sessions vs ~11 or ~1 large-part). Either derive MAX_SESSIONS from the worst legal MAX_PART_CHUNKS, reserve weighted reference-work units, or an equivalent aggregate budget — a narrative range does not preserve the maintenance bound. 3. Adversary refutations to close: - Knob-rule self-contradiction: W_ref/MAX_SESSIONS arithmetic is computed at MAX_PART_CHUNKS = 5,120 while the doc's own knob rule caps it at 165–381. State max_part_bytes per chunk size as a real number, recompute scenarios in-range or design part-record segmentation, and register the S3 5 GiB part-size consequence. - sidx: value carries no placement, so the reaper cannot compute orphan keys and genuinely_holds cannot count in-flight fragments. Write the WritePlan placement into the sidx: value at intent time; state the record shape per ADR-0046 and extend the serialization-identity section beyond `owner`. - The cursor-keyed retire: walk needs a ranged/limited scan the MetadataStore seam does not have (scan is prefix-only, complete-or-fail). Name the seam change in "What the implementing slices change" or redesign the walk within scan(prefix). - F13 window: add a normative line that the restore fence completes before the store serves multipart verbs (restore-then-serve-before-fence). - Secondary: fix the stale "≈ 52 × SCAN_CAP" passages to agree with the knobs; add seg: maintenance-writer rows + exact-bytes CAS rule + a register row for the repoint-vs-drain race; charge segmented reference-build work (budget/knob/alarm + register row); resolve the grace-start contradiction on the delete path. 4. PRESERVE what the adversary could not refute: the fence/epoch machine, the O(1) session-precondition publication proof, per-attempt epoch-scoped seg: keys, the exactly-once terminal decrement, and the byte-budgeted batch inventory.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 6 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- Full previous attempt preserved in `iteration-v4/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 5 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected at sign-off: gating T4-batch-review red (6 blocking, 0 recorded-rejected) plus three adversary refutations. Rework the document, do not restart — the fence/epoch machine, O(1) publication proof, exactly-once decrement, byte-budgeted batches, and restore fence all survived refutation and must be preserved. Fix (or record-reject in review-rejected.md) the six T4 findings: - 0016:393 — shared `sinf:` CAS makes concurrent part commits conflict; compensation path discards a valid upload on a counter-only collision. - 0016:522 — segment repoint vs supersede race: repoint doesn't touch the inode, supersede doesn't precondition the segment; moved fragment stranded. - 0016:844 — renewal refusal doesn't cancel an already-authorized fragment write; can land after orphan grace and stay unevidenced. - 0016:1164 — single-PUT segmentation underspecified: no upload id / session / Completing epoch to anchor decision 7's machinery. - 0016:925 (x2) — named regression test requires `sinf`→0, contradicting the protocol's own rule (crashed slots stay counted; `sinf` deleted outright). Address the three adversary refutations (two corroborate T4 findings): - Segmented single-PUT has no crash-evidence/reclamation half — design it (group token, fence analogue, crash evidence, reclaimer) or explicitly carve it out of the doc; add register coverage. - Reader-transparency claim falsified for segmented objects: `seg:` records deleted without reader grace tears a concurrent GET — grace-delay the record delete or specify a resolve-retry rule; add the register row. - X47 over-claims "outcome (a) closed" for the seg repoint: pre-evidence the destination position (orphan pre-mark deleted by the winning CAS) or honestly dispose the residue as bounded cost with a named reclaimer; correct the register row. The ⚑ serialization-cost question (0016:1722) remains unruled — keep it flagged as a sign-off question; it is the human's to adjudicate at the next sign-off, not the builder's to resolve silently.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 6 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- Full previous attempt preserved in `iteration-v5/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 6 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected at sign-off (issue_626): gating T4-batch-review red — 4 blocking, 0 recorded-rejected. A fresh adversary pass (re-run on opus[1m]; the driver's leaf had failed to produce a verdict) corroborated the two load-bearing findings as builder-fixable [impl] and could NOT refute the design's core. Rework the document — do NOT restart. PRESERVE (adversary attempted and could not refute — do not redesign): the fence/epoch state machine; restore fence-then-serve (X17/F13); per-attempt epoch-scoped seg: keys (X37/X40/F18); the committed-object repoint-vs-supersede armor (X47); the exactly-once terminal decrement and counter-only-collision handling (X42/X52); the segmented-GET resolve-retry rule (X51); byte-budgeted batch inventory; retire:/reference-build bounded-cost dispositions (X39/X48). FIX (every T4 finding must be fixed so it leaves the next run, or recorded-rejected in review-rejected.md — the gate blocks while any is unchecked): 1. (T4 #1 / adversary, 0016:523) Staged-part reconstruction re-place strands the rebuilt destination fragment on a lost CAS — outcome (a), permanent under Defer/GC (gc.rs:183-187): the fragment is written to P_new before the CAS, and a Complete/Abort/reap fence in that window fails the CAS, leaving P_new referenced by nothing and evidenced by nothing. Extend the committed branch's destination pre-mark rule (X47) to the STAGED re-place path, and correct the X29 register row that mischaracterizes this as safe. 2. (T4 #2 / adversary, 0016:217/:388) mpuctl:count has no bootstrap — first CreateMultipartUpload on a fresh/upgraded store cannot satisfy require(mpuctl:count == c). Define the absent-as-zero read (or a one-time init batch) and add its round-trip/first-create observable. 3. (T4 #3, 0016:780) G_orphan == W_write boundary race: tighten to G_orphan strictly > W_write with clock-resolution margin, OR record-reject with the adversary's t_mark > t_authorize reasoning (deferable to #625's knob choice) in review-rejected.md. 4. (T4 #4, 0016:1532) Summary line wrongly attributes the late-write bound to lease-renewal refusal; the bound is the fail-closed W_write timeout (Decision 5). Correct the doc-consistency slip. ADJUDICATE (adversary note, not scored): state normatively whether a bulk DeleteObjects (#509) must byte-budget its obligation-installation across transactions (1,000 large generations x ~V ≈ 100 MB, over-envelope), or explicitly assign that contract to #509. Do not re-attempt the rejected approach unchanged.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- Full previous attempt preserved in `iteration-v6/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
