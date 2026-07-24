# Brief — issue 626 / multipart-commit-protocol

> The Plan artifact (docs 02 §PLAN). Do reads ONLY this file. Field labels are parsed by
> the driver — keep the `- **Label:** value` shape. All target citations are grounded on
> **getwyrd/wyrd origin/main @ cd82a29** and were re-verified at Plan by reading that
> commit — none is carried from the tracker or from memory.
>
> **The deliverable of this bundle is a DESIGN DOCUMENT, not code.** Do authors a draft
> proposal under `docs/design/proposals/draft/` (ADR-0037's vehicle: editable while
> `status: draft`, unlike an Accepted ADR) that settles the multipart commit protocol —
> the design that four rounds of adversarial plan review on #508's brief established is
> not a brief-scale decision. #508 (the implementing slice) is blocked on this document.
> #625 (the reaper) is the IMPLEMENTING slice of this proposal's reaper design: scope was
> extended at Plan (2026-07-23, maintainer decision in-session) — the issue's "NOT in
> scope: the reaper's algorithm" line is superseded; the algorithm is DESIGNED here, its
> implementation stays #625. The SAME decision resolves the issue's internal tension on
> sweeper-only exits (its decision-6 text says the protocol "must not strand uploads in
> states only a sweeper can exit", while its sequencing section contemplates exactly such
> states): with the reaper designed here, sweeper-only exits are PERMITTED, and their
> existence forces the implementation order #625 with/before #508.
>
> Plan-stage review: four cross-vendor codex rounds. Round 1
> (`results/plan-review-codex-626.md`, NOT-READY): five blockers — gate attribution, an
> accepted-cost loophole, leg-A overclaim, two citation drifts, a 501 overclaim. Round 2
> (`results/plan-review-codex-626-r2.md`): closed three + all cautions, caught the first
> two echoed in downstream fields. Round 3 (`results/plan-review-codex-626-r3.md`):
> READY on revision 3 — the leg-B deciders and the three-way disposition (eliminate /
> bounded-non-safety-cost / flag NEEDS-HUMAN) stated identically everywhere. Revision 4
> then EXTENDED SCOPE (the reaper algorithm folded in — maintainer decision, see Scope);
> round 4 (`results/plan-review-codex-626-r4.md`) attacked the delta and its three
> findings (the half-recorded tracker reversal, the F7 hard-bound loophole, the
> mechanism/knob boundary) are incorporated in THIS revision. Dispositions:
> `results/plan-review-codex-626.verdict.md`.

- **Slug:** multipart-commit-protocol
- **Kind:** enhancement (design artifact — draft proposal per ADR-0037)
- **Defect:** The multipart **commit protocol** — what happens *underneath* a
  CompleteMultipartUpload — is unsettled, and #508 is unimplementable until it is. Multipart
  is the first consumer of several metadata-layer contracts written for a different shape of
  write (a single streaming upload under a 30 s lease), and it breaks each one's stated
  assumptions: publication is lease-conditional and TTL-timed
  (`crates/core/src/metadata.rs:763-793`, `crates/server/src/lib.rs:53`); the maintenance planes gate on
  **committed** state only — GC / restore / scrub / drain share a committed-only reference
  set (`crates/custodian/src/gc.rs:217-228`), and reconstruction / rebalance independently
  scan committed inodes; the metadata model assumes small records far inside
  the inherited 10 MB / 5 s transaction envelope (`crates/traits/src/lib.rs:744-758`); and
  batches are explicitly non-idempotent with replay safety the caller's to design
  (`crates/traits/src/lib.rs:833-843`). Four review rounds each closed the prior round's
  findings and surfaced a defect one layer deeper — the failure modes were not yet
  enumerable, so no implementable success criterion could be stated.
- **Success criterion:** two legs, both evaluated at Check on the patched tree.
  **(A) Artifact + mechanical validity.** The patch adds exactly two docs changes and
  nothing else: (1) the new draft proposal
  `docs/design/proposals/draft/0016-multipart-commit-protocol.md`, using
  `docs/design/templates/proposal.md`'s frontmatter and section set, with `type: proposal`,
  `status: draft`, `author: Eduard Ralph`, `tracking-issue:` #626; (2) its row in the index
  table of `docs/design/proposals/README.md` (status `draft (settlement: #626)`, mirroring
  0014's draft-row shape at `proposals/README.md:30`). Two sub-legs with different deciders:
  **(A1, mechanical)** `cargo xtask ci` (the gating `C4-ci` row) passes on the patched
  worktree with the prose gates actually executing — `typos`, the docs lint, and the
  `render_site --check` dangling-link audit (the renderer imports the gate probes are
  installed on this host, verified at Plan, so these legs run rather than warn-skip).
  Honest bounds: the audit fails on a dangling or wrong LINK — an index row pointing at a
  filename that does not exist, a bad relative link from the new file — but it cannot see
  a MISSING index row, a wrong proposal number, absent frontmatter values, or a dropped
  section. **(A2, inspected)** Those are a Check-time inspection checklist for the
  reviewer and the human: exactly the two changes above, the exact path and number 0016,
  the frontmatter values, the template's full section set, and the index row present with
  the settlement annotation.
  **(B) Settlement bar** (the load-bearing leg; judged by the Check reviewer and adversary
  leaves — which read THIS brief and are bound by the Refutation standard below — and by
  the human at sign-off. The gating `T4-batch-review` runs 3 codex passes armed with the
  target `AGENTS.md` rubric over the raw diff; it blocks publish on BUG-class findings but
  is NOT handed this predicate — count it as defence in depth, never as leg-B enforcement). For EACH of the six decision
  areas (§Decisions below) the proposal states: (i) the decision taken; (ii) the invariant
  it preserves; (iii) an enumerated **failure-mode table** — for every known way to
  implement that decision wrong, a named observable that would fail — written so the
  implementing brief (#508's next Plan iteration) can lift its success criterion directly.
  AND each of the ten known failure modes **F1–F10** (§Known failure modes below) is either
  **eliminated by the design** or **recorded as an explicit accepted cost** with rationale,
  a stated bound, and a named follow-up issue — where accepted-cost disposition is
  available ONLY for bounded availability / latency / capacity / operational trade-offs
  that preserve every outcome clause of the Refutation standard: an execution exhibiting
  outcome (a), (b), (c) or (d) may NEVER be disposed of as an accepted cost by the
  builder; if the design concludes such an outcome is genuinely unavoidable, that is not
  an acceptance but a flagged sign-off question (NEEDS-HUMAN, founding-maintainer
  authority), stated as such in the proposal. AND the **sequencing consequence** is answered normatively:
  with the reaper designed HERE, the question is implementation order — whether the reaper
  implementation (#625) MUST land with/before the multipart implementation (#508); a state
  whose only exit is the reaper forces "with/before" — stated as a requirement, not an
  aside. AND
  none of the six decisions nor F1–F10 is parked in the proposal's Open-questions section
  (template-sanctioned open points are allowed only for matters that do NOT gate #508, and
  each surviving open question must state WHY it is non-gating and name its owner or
  follow-up — an unresolved protocol dependency relabelled non-gating is a leg-B failure).
  **Refutation standard (what makes B red):** a reviewer constructs a concrete execution
  under the proposed protocol — a crash point, a lost CAS, a race, an operator
  drain/restore/decommission, a clock-epoch mismatch — that (a) strands bytes or metadata
  with no bounded reclamation path, (b) leaves a state no verb, pass, or documented driver
  can exit, (c) publishes or preserves a chunk map over bytes a maintenance pass may
  reclaim or has reclaimed, or (d) installs an obligation exceeding the inherited
  transaction envelope — and that execution is **absent from the proposal's failure-mode
  enumeration**. One such execution = criterion not met. Outcomes (a)–(d) are
  non-negotiable: enumerating one and "accepting" it does not satisfy leg B (see above).
- **Falsifiability:** RED is producible at Check on this host, no cluster or topology
  needed — the environment that exhibits failure is the document against the codebase's
  stated contracts, both present. Leg A1 goes red mechanically: `C4-ci` runs in
  `$PDCA_WORKTREE` carrying the patch, and this host has typos-cli 1.48.0 and the renderer
  imports the gate probes (markdown-it-py + PyYAML — the gate probes imports, not exact
  pins; verified at Plan), so a typo, a dangling relative link from the new file, or an
  index row linking a filename that does not exist fails the gating check — these legs
  execute for real here, they do not warn-skip (a MISSING index row is invisible to them —
  that is checklist A2). Legs A2 and B go red at the judgment tier: the Check reviewer and
  the difficulty-gated adversary leaf read this brief and apply the Refutation standard as
  a decidable attack predicate; final arbiter is the human — a proposal change is a
  by-design NEEDS-HUMAN §6 row (INTEGRATION §4), architecture-board / founding-maintainer
  authority. The **gating** `T4-batch-review` (3 parallel codex passes armed with the
  target `AGENTS.md` rubric over the diff — BUG-class findings block publish until triaged)
  runs as well, but it never sees this brief: defence in depth, not leg-B enforcement.
  Note honestly: the per-fix `C4-verify` row exits 0 **vacuously** for a patch touching no
  crate ("docs/CI only — nothing to verify per-fix", run-verify's early exit) — it carries
  no evidence either way and must not be read as green proof.
- **Invariant to restore:** For the defect category **assembled writes — writes whose
  durable staging outlives any single request/lease window** (multipart now; server-side
  copy #504 step 2 and any future resumable write share the shape), the metadata layer's
  documented safety contracts must hold **by design, not by assumption**:
  (1) a chunk map is never published over bytes any maintenance pass is free to reclaim —
  the #490 obligation, stated normatively in the publication guard's contract
  (`crates/core/src/metadata.rs:763-793`);
  (2) every durable byte is at every instant classifiable as committed-referenced,
  staged-with-an-exit, or garbage-with-a-sound-reclamation-path — no fourth category
  "protected forever by residue" (the committed-only reference set,
  `crates/custodian/src/gc.rs:217-228`, is today's two-class world; multipart adds the
  third class and must give it an exit);
  (3) no lifecycle state is absorbing — for every state some verb, pass, or documented
  driver exits it;
  (4) every obligation installed at publication or teardown is drained in **bounded work**
  (the inherited envelope is contractual context, `crates/traits/src/lib.rs:744-758`; the
  bounded-batch precedent and its rationale, `crates/custodian/src/restore.rs:92-100`), and
  replay safety is built into each batch rather than assumed
  (`crates/traits/src/lib.rs:833-843`).
  SELF-TEST: not satisfiable by guarding a single module — this is a protocol property
  spanning core, custodian, and server; the proposal's job is to make it hold across
  every maintenance consumer at once, both the shared-reference-set members and the
  independent committed-inode scanners.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Difficulty:** high — blast-radius as held-in-view surface, not diff size: the diff is
  two docs files, but the decisions bind the entire publication + custodian contract
  surface (core metadata, GC, restore, scrub, reconstruction, drain/rebalance, the store
  trait envelope, and the S3-visible lifecycle), and a reviewer must hold all of it to
  judge one decision. Rated up deliberately so the adversary leaf fires.
- **Do model:** opus-xhigh
- **Scope:** ONE logical change: author draft proposal 0016 — the multipart commit
  protocol: (1) the publication-time proof for assembled writes, (2) the protection class
  for durable-but-unpublished bytes and its per-consumer visibility, (3) lifecycle states
  and failure semantics, (4) the bounded-work pattern for unbounded objects, (5)
  reclamation evidence for failed in-flight work, (6) the abandoned-upload REAPER —
  both its protocol-facing half and its detection/sweep ALGORITHM (decision 6; a Plan-time
  maintainer decision supersedes the issue's "NOT in scope: the reaper's algorithm" line
  AND resolves its sweeper-only-exit tension — such exits are permitted, with the forced
  #625-with/before-#508 ordering; see the header note)
  — plus the normative implementation-order answer for #625 vs #508; and register the
  document in the proposals index. The proposal **decides**; it does not implement. / out
  of scope: any change under `crates/` or `xtask/` (no code, no tests — the reaper is
  DESIGNED here; its implementation stays #625, which becomes a thin implementing slice of
  this proposal's reaper section); the reaper's operational knob VALUES and
  its metrics/alert/CLI wiring (mirroring decision 6's knob rule: 0016 settles each
  correctness-relevant knob's VALID RANGE and bounding invariant, and #625 chooses and
  wires a value inside that settled range — only a knob whose entire range is safe is
  #625's freely); authoring ADRs (the proposal RECOMMENDS which of its decisions
  graduate to ADRs — the issue expects one or two — but authoring them is follow-up work
  under ADR-0037); editing ANY accepted/stable document (host `docs-immutability` gate;
  ADR-0037 supersession discipline); rewriting #508's brief (that happens at #508's next
  Plan, consuming 0016); relitigating the ETag basis (settled — ADR-0047) or the S3 wire
  surface (stable across all four review rounds — remains #508's).
- **Repro instruction:** Not a runtime defect — the reproduction is that no implementable
  success criterion exists for #508's commit half. Observables on origin/main @ cd82a29:
  (1) every ordinary raw-spelled multipart query form is refused 501 by the subresource
  denylist (`crates/gateway-s3/src/lib.rs:335-346`, object-route guard `:1696-1709`; the
  documented residual — a percent-encoded key dodges the raw match and executes as a plain
  object verb, `:387-396` — is #508's fence work) — there is no multipart code whose
  behaviour could be specified against; (2) four adversarial plan-review rounds
  on #508's brief each closed the prior round's findings and surfaced a deeper design
  defect; round 4 recorded ten open design findings (distilled into F1–F10 below, so this
  brief is self-contained); (3) the sharpest single trace, **no crash required**: fence
  Complete `Open → Completing`; a concurrent PutObject to the same key wins the
  publication CAS — `create`'s `require_absent` on inode + dirent
  (`crates/core/src/metadata.rs:366-382`) or the superseding CAS on the prior inode bytes
  (`:582-619`) — so Complete's publish returns `Conflict`; with no defined exit for the
  fenced state, the session is stuck forever and every staged byte it references is
  protected forever.
- **External dependencies:** `typos`, `docs-renderer` — the two prose-gate toolchains that make C4-ci's spell check and dangling-link audit execute rather than warn-skip on a docs-only patch (the laptop/CI asymmetry of INTEGRATION §3); both are registered as doctor rows under those exact ids and both were verified installed on this host at Plan. Nothing else: no Docker, no cluster, no live backend, no topology — the deliverable is a document, so the base toolchain plus these two suffices.
- **Test file:** none — docs-only deliverable; no regression test exists or is possible for
  a design document (see Verification posture; mechanical validity is the gating whole-tree
  prose gate; content adequacy is judged by the brief-aware Check reviewer and adversary
  with the human deciding — the gating batch review is diff-and-rubric defence in depth
  only, never the leg-B decider).
- **Verification posture:** declared posture (a), NET-NEW / born-at-tier: there is no prior
  failing assertion to flip — "red" for leg B is criterion-absence under the Refutation
  standard, exercised by review, not by a compiled test. What IS built and exercised AT
  Check: the document itself — leg A1 mechanically by the gating `C4-ci` prose gates over
  the patched worktree (probes verified at Plan, so the legs genuinely run); legs A2 and B
  by the brief-aware Check reviewer and the claude adversary under the Refutation
  standard, with the human deciding. The **gating** `T4-batch-review` (diff and rubric
  only — it never sees this brief) is defence in depth, not leg-B evidence; `C5-mutants`
  and `C4-verify` are vacuous on a no-code diff and carry nothing, as noted above. Deferred beyond Check: ONLY the governance act
  of accepting the proposal (`draft` → `accepted`, architecture-board / founding-maintainer
  authority per GOVERNANCE; ADR-0037) — deliberately not part of this cycle's criterion;
  the bundle ships the artifact at `status: draft` and the human's §9 records fitness.
  Nothing unbuilt hides behind this posture: the document is the whole deliverable and is
  fully present and attacked at Check.
- **Citations expected:** Do must cite `path:line` on origin/main @ cd82a29 for every
  contract the proposal binds against (start from the verified citations in this brief;
  re-cite them in the proposal's own text so it stands alone in the target repo).
  Composition peers Do MAY open (the narrow read-beyond-brief exception, docs edition —
  this is a document-composition slice, so the "peer callsite" is a peer document):
  `docs/design/templates/proposal.md` (the mandated skeleton — use its exact section set);
  `docs/design/proposals/draft/0014-milestone-7-failover-and-dr-single-dc.md` (form peer:
  what a live draft proposal looks like); the index-row pattern at
  `docs/design/proposals/README.md:30` (0014's row — mirror `draft (authoring: #368)` as
  `draft (settlement: #626)`); ADR-0037 (lifecycle + immutability rules this bundle must
  obey); ADR-0046 (precedent: a new namespace concept gets **real records**, not
  synthesized encodings — `bucket_key`, `crates/core/src/metadata.rs:48-50`); ADR-0047
  (what is already settled and what was deferred, its §Consequences bullet on multipart).
  All code citations in §Decisions and §Known failure modes below were read and verified
  at Plan on cd82a29.
- **Prior-art check (triage cycles):** By affected path `docs/design/proposals/`: next free
  number is **0016** (accepted through 0015; drafts 0006/0008/0009/0010/0014; verified by
  listing both dirs) and no open PRs exist on getwyrd/wyrd at Plan time (verified — the
  open-PR list is empty, so no number collision in flight). Merged history: no multipart
  design document has ever existed; draft proposal 0006 names resumable/multipart upload
  as "composes onto the pending-chunk ledger; a separate API proposal" (`0006:218`) — 0016
  IS that proposal; ADR-0047 settled the ETag basis and explicitly deferred only the
  multipart composition to the implementing slice (`0047` §Consequences). Closed/rejected
  work: #508's two rejected Do iterations are archived in this project at
  `results/issue_508/iteration-v1..v2`, and its round-4-reviewed brief stands BLOCKED with
  a do-not-implement banner; its stable wire-surface half is #508's to keep, not this
  bundle's to modify.
- **Disposition hint:** new-feature

## Context Do needs (condensed from the tracker; the issue thread has no comments)

Issue #626 (milestone 0.1 Alpha, label `research`) splits #508 in two. The S3 **wire
surface** half — routing, denylist removal, the percent-encoding fence, exact status/error
codes — was stable and sound across all four review rounds and is NOT in question. What is
in question is everything underneath Complete: the publication + lifecycle half. Per
ADR-0037 the vehicle is a draft proposal (editable while `draft`); one or two ADRs may
later fall out of it for the decisions that outlive multipart — server-side copy (#504
step 2) and any future resumable-write path have the same shape. Each decision is "done"
when a brief could state a success criterion whose **negations are enumerable** — for
every way to implement it wrong, there is an observable that fails. Four review rounds
failed exactly that test because the failure modes were not yet known. F1–F10 below ARE
the now-known failure modes; the proposal must make the enumeration closed enough that the
adversary's Refutation standard finds nothing outside it.

## The six decisions to settle (each with its verified grounding)

1. **The publication contract for an assembled write.** Today "these bytes are still safe
   to publish over" is proved by **lease liveness**: every chunk must hold an unexpired
   `pending:` lease, checked by CAS preconditions riding in the publishing batch
   (`live_lease_guards`, `crates/core/src/metadata.rs:763-793`, issue #490 — absent OR
   lapsed entry ⇒ fail closed with `Conflict`). The TTL is 30 s
   (`crates/server/src/lib.rs:53`) and is renewed only while a single
   `stream_write_data` call is in flight (`crates/core/src/write.rs:474-500` — renewal is
   conditional and aborts the upload rather than resurrect a lapsed lease). A multipart
   Complete assembles a chunk map from parts committed minutes or hours apart: **what
   replaces lease liveness as the publication-time proof?** Whatever it is must preserve
   #490's obligation (never publish over bytes GC may reclaim) and must not put a timer on
   an inherently long-lived operation.
2. **A protection class for durable-but-unpublished bytes.** GC's safety predicate is
   "referenced by a *committed* inode chunk map" (`referenced_fragments` scans `inode:`
   only and skips non-Committed records, `crates/custodian/src/gc.rs:249-260`; a pending
   inode's provisional map is **excluded by design**, `:217-228`). Multipart introduces a
   third category besides published bytes and leased garbage: committed, durable,
   referenced by something that is not an inode, not yet published. Decide whether that is
   a first-class concept and **which consumers of the reference set see it**. The
   consumers, verified — TWO KINDS, and the decision must state each one's visibility:
   sharing the one `ReferenceSet` — GC's gate (`gc.rs:162`, via `ReferenceSet::protects`,
   `:244-246`); the restore pass — the SAME `protects` gate, "SAFETY GATE, identical to
   GC's" (`crates/custodian/src/restore.rs:218-224`); scrub, which walks
   `referenced.placed` (`crates/custodian/src/scrub.rs:95-110`); and drain/desired-state
   (`crates/custodian/src/desired_state.rs:157-164` computes `genuinely_holds` from
   `referenced.placed`). Scanning committed `inode:` records INDEPENDENTLY (not
   `ReferenceSet` consumers) — reconstruction, whose `assess` resolves each obligation to
   its committed chunk map itself (`crates/custodian/src/reconstruction.rs:313-325`;
   obligation-drain semantics `:182-195`), and rebalance evacuation planning
   (`crates/custodian/src/rebalance.rs:147-151`). Widening the set indiscriminately makes
   a drain never reach `Satisfied`; leaving staged bytes out entirely lets a maintenance
   pass strand a live upload (F6 gives the wipe trace). Decide per consumer, and record
   each choice's cost under the SAME three-way disposition as F1–F10: the availability
   side (a drain that waits on live staged uploads) is a boundable accepted cost; the
   safety side (the F6 wipe trace — outcome (c)) is never acceptable, only eliminable or
   flagged NEEDS-HUMAN.
3. **Lifecycle states and failure semantics** — the part that broke review round 4. A
   fenced state machine (e.g. `Open → Completing | Aborting`) makes concurrent parts and
   Complete safe, but the proposal must answer: what happens when publication loses the
   CAS race (`create`'s `require_absent`, `metadata.rs:366-382`, or the superseding CAS,
   `:582-619`, against an ordinary concurrent PutObject) — does the fence roll back, is
   the upload dead, what does the client see? **No state may be absorbing unless something
   can exit it.** What durable evidence distinguishes "published" from "about to publish",
   so a retried or resumed Complete is idempotent rather than double-publishing —
   remembering batches are explicitly NOT idempotent and replay safety must be designed
   into the batch (`traits/src/lib.rs:833-843`)? Who drives resumption after a crash
   mid-teardown?
4. **Bounded work for unbounded objects.** Multipart makes objects beyond the 5 GB
   PutObject ceiling reachable for the first time, then permits overwriting them.
   Publication orphans the prior version with one `put` per prior fragment **in a single
   batch** (`commit_chunk_map_superseding`, `metadata.rs:582-619`), so the fan-out this
   feature makes reachable can exceed the inherited FoundationDB envelope
   (`traits/src/lib.rs:744-758`) — a **permanent** failure to publish, not a slow one.
   Scale, from verified constants: at `DEFAULT_CHUNK_SIZE` 1 MiB
   (`crates/server/src/lib.rs:51`) and RS(6,3) (`:49`, 9 fragments/chunk), superseding a
   single 5 GiB part owes 5120 × 9 = **46,080** orphan puts. Decide the general pattern
   for **a durable obligation installed atomically and drained in bounded batches** —
   `MARK_BATCH = 1_000` is the partial precedent, with the rationale spelled out
   (`restore.rs:92-100`: bounded batches make partial progress durable BECAUSE the pass
   is idempotent) — and apply it to every unbounded surface the protocol introduces
   (publication orphaning of a prior multipart-sized version, part re-upload/supersede,
   abort teardown of a 10,000-part session).
5. **Reclamation evidence for failed in-flight work.** A write fenced or refused
   mid-stream leaves `pending:` entries. Deployed GC runs `ExpiredPendingPolicy::Defer` —
   correctly, because reclaiming on a producer's stamp is unsound when producers do not
   share the reconciler's clock (`gc.rs:78-105`, the #557 defect class; the deployed loop
   takes the policy as a caller-chosen parameter whose contract names `Defer` as the
   deployed default, `crates/server/src/custodian.rs:456-472`, and the CLI passes `Defer`
   unless the operator attests `--gc-expired-pending`, `crates/server/src/cli.rs:975-979`). So that residue is never reclaimed in deployment. Decide whether a
   losing writer must commit **compensating orphan evidence** for its own chunks (the
   orphan input is clock-safe by construction: written only after the referencing commit
   is gone, `gc.rs:92-95`) or whether `pending:` gets a sound reclamation story. The
   everyday case is Ctrl-C during `aws s3 cp` with parts in flight.
6. **The abandoned-upload reaper — designed here, implemented in #625** (scope extended
   at Plan; see Scope). The belongs-where test: anything whose wrongness would change the
   protocol's records or state machine is settled HERE. Knobs are classified by
   CORRECTNESS-RELEVANCE, not by whether they touch a record: for every knob a safety
   property depends on (a timeout, a cadence, a batch size), this proposal settles the
   VALID RANGE and the invariant that bounds it — #625 picks a value inside that settled
   range and wires it; only a knob whose entire range is safe is #625's freely. (a) Protocol-facing: the observable distinguishing a progressing upload from
   an abandoned one — an OUTCOME constraint, not a chosen mechanism: it must be derivable
   from records the protocol durably writes (the false-premise correction previously
   exported to #625: no settled design carries a per-part session counter, so no imagined
   progress marker may be assumed); WHICH records carry it and what counts as liveness
   evidence is this decision's to settle, not this brief's to prescribe. Which states the
   reaper may exit, with what idempotent, preconditioned batches — once the reaper
   exists, NO protocol state may remain unexitable (this is what closes F1 globally).
   The clock lifecycle that owns the abandonment judgment, its owner named (F10): the
   #557 trap — a record stamped under one clock epoch, read by a wall-clocked reaper —
   is the known killer, and `pending:` stamps are exactly such records. (b) The
   algorithm: the detection rule and its safety argument; the sweep structure — bounded
   batches per decision 4, strictly under the F7 cap by construction; and the reaper's
   own crash/resume story (it is decision 3's resumption driver, so its teardown must be
   idempotent and re-enterable). Decision 6's failure-mode table must cover at least:
   reaping a PROGRESSING upload (the false positive — including a single `UploadPart`
   streaming longer than any fixed window); the reap racing a concurrent Complete
   (teardown vs publication CAS — neither publishing over just-reaped bytes nor
   stranding a just-published object's records); the reaper crashing mid-teardown;
   never-reaping (the false negative — F7's unbounded growth); the sweep itself
   violating the bounded-work pattern; ARRIVAL OUTRUNNING DRAIN — valid uploads arriving
   faster than the reaper reclaims, under which an asynchronous collector alone bounds
   nothing; and REAPER UNAVAILABILITY long enough for the staged namespace to cross the
   F7 cap.

## Known failure modes the protocol must dispose of (F1–F10)

Distilled from the four review rounds and re-verified against cd82a29. For each, the
proposal takes exactly ONE of the three dispositions leg B defines: **eliminates** it (the
failure-mode table naming the observable that would catch a regression); **accepts** it
ONLY if it is a bounded non-safety trade-off — availability / latency / capacity — with
the bound, rationale, and a named follow-up; or, for an outcome (a)–(d) the design cannot
eliminate, **flags** it as a NEEDS-HUMAN sign-off question (never self-accepted). Silence
on any of them fails leg B.

- **F1 — absorbing terminal state on publication CAS loss (no crash needed).** Complete
  fenced `Open→Completing`; a concurrent PutObject wins the publish CAS
  (`metadata.rs:366-382` / `:582-619`); Complete gets `Conflict`. If no verb or driver
  exits the fenced state, the session is stuck forever, and every staged fragment it
  references stays protected forever. Must define: the state transition on CAS loss, the
  client-visible answer, and the no-absorbing-states proof for the whole machine.
- **F2 — staging-record disposal at publication.** If the staging records of parts that
  WERE published survive Complete, they keep feeding the protected set; when the object is
  later overwritten, the prior version's fragments are orphaned
  (`metadata.rs:582-619`) yet still protected by the stale staging records — the orphaned
  bytes of a live object's prior version are never reclaimed. Must define the disposal of
  every staging record at publication and the observable proving none remain.
- **F3 — abort-race residue unreclaimable under Defer.** An in-flight part that loses its
  session precondition leaves `pending:` entries; deployed GC defers expired-pending
  reclamation (`gc.rs:78-105`; the CLI default, `server/src/cli.rs:975-979`), so that
  residue is retained forever on the commonest abort path there is (Ctrl-C with parts in
  flight).
  Interacts with decision 5: either the loser leaves compensating orphan evidence, or
  `pending:` gets a sound reclamation story — forever-retained residue is outcome (a) and
  is NOT an acceptable cost; if the design concludes it is unavoidable on this base, that
  is a flagged NEEDS-HUMAN sign-off question, not an acceptance. (A bounded retention
  window with a named reclaimer would be a boundable cost; "forever" is not.)
- **F4 — obligation fan-out vs. the transaction envelope.** Any single-batch obligation
  proportional to object size (46,080 orphan puts for one 5 GiB part supersede; more for
  whole-object overwrite) exceeds the inherited 10 MB / 5 s ceiling
  (`traits/src/lib.rs:744-758`) — permanent publish/supersede failure. Every escape hatch
  must actually be able to commit (its preconditions satisfiable in the state it runs
  from), and a superseded generation must remain evidenced somewhere until reclaimed —
  never dropped into the unreferenced-but-unevidenced void.
- **F5 — non-idempotent Complete / retry after unknown outcome.** A backend may not replay
  a batch whose outcome is unknown (`traits/src/lib.rs:833-843` and the re-read remedy at
  `:738-745`); a resumed Complete must be distinguishable from a first Complete by durable
  evidence, or it double-publishes / re-orphans. Name the evidence and the re-read
  protocol.
- **F6 — maintenance-pass visibility split.** If staged bytes are invisible to
  drain/rebalance: operator drains a D-server holding staged fragments →
  `reconciliation_status` computes `genuinely_holds` from `referenced.placed` only
  (`desired_state.rs:157-164`) → reports `Satisfied`; `plan_evacuations` scans `inode:`
  only (`rebalance.rs:147-151`) → nothing evacuates; operator wipes the server; Complete
  then publishes a chunk map naming wiped bytes — the object is born under-replicated (or
  below k after a second drain). If staged bytes are visible everywhere: a drain can never
  reach `Satisfied` under a slow upload — unbounded stall. Per-consumer visibility is
  decision 2; this trace and its mirror are the two costs to weigh IN WRITING.
- **F7 — unbounded staged state halts the custodian plane.** `MetadataStore::scan` fails
  loud above `SCAN_CAP = 1 << 20` — no truncation, no partial result
  (`traits/src/lib.rs:286-292`; backends clamp to it, `metadata-redb/src/lib.rs:73-78`).
  Any reference-set construction that scans a staging namespace inherits that: once
  abandoned staging records cross the cap, the scan errors and the WHOLE reconcile step
  aborts before GC/scrub/reconstruction/rebalance run (each leg `?`-propagates as a store
  error, `crates/custodian/src/reconciliation.rs:75-112`) — a client-driven prefix takes
  the durability plane down. The protocol must keep reference-set construction strictly below the cap BY
  CONSTRUCTION — an incremental/paginated build, admission control on open uploads, a
  join that never scans an unbounded namespace, or the decision-6 reaper — which counts as
  a bounding construction ONLY together with an enforceable admission/backpressure
  invariant that prevents crossing the cap under arrival-outrunning-drain and reaper
  unavailability (an asynchronous collector alone establishes no cardinality bound);
  merely choosing a smaller bound only makes the halt arrive earlier, and an unbounded
  namespace with a hoped-for reaper is not a disposal either.
- **F8 — a vacuous or unsound reaper design.** With the reaper designed in this proposal,
  sweeper-only exits are permitted — but the reaper section becomes load-bearing: its
  observable must be one the protocol actually writes (the false-premise correction,
  decision 6); its detection must be clock-sound (F10); its false-positive,
  false-negative, race, and crash modes must appear in decision 6's failure-mode table;
  and the sequencing answer must order #625 against #508 accordingly — a state whose only
  exit is the reaper makes #625 land with/before #508, normatively, not as an aside.
- **F9 — client-visible semantics of fenced states unpinned.** What do
  UploadPart/Complete/Abort/ListParts answer while a session is in `Completing` /
  `Aborting`? Is Abort's HTTP response bounded-latency (teardown of a 10,000-part session
  cannot ride inside one request under leg F4's bounds — so what returns when)? Does the
  uploads listing show fenced sessions? The implementing brief needs exact, decidable
  answers; leave these open and #508 is still unbriefable.
- **F10 — clock-lifecycle ownership for any new stamp.** Any grace/staging stamp the
  protocol adds is written by one component and evaluated by another; the target's
  standing rubric makes "one clock per correctness lifecycle … a new clock read states
  which source owns its lifecycle" a MUST (`AGENTS.md:132-142`, the #557/#565 class). The
  worked precedent to mirror: the overwrite path's `orphaned_at_millis` IS the commit's
  own instant, stamped by the gateway (`crates/core/src/write.rs:305-311`) and evaluated
  by the custodian against the grace window (`gc.rs:172-176`). The proposal names the
  owner of every clock lifecycle it introduces.

## Settled inputs — record, do not relitigate

- **ETag basis:** ADR-0047 — lowercase-hex SHA-256 as an opaque change-token; MD5
  explicitly rejected (would need the ADR-0003 dependency audit); only the multipart
  *composition* of part digests was deferred to the implementing slice. The proposal
  records the composition question as #508's, or settles it in one paragraph if decision 3
  needs it — but the basis is closed.
- **S3 wire surface:** routing, denylist removal, percent-encoding fence, exact
  status/error codes — stable across all four review rounds; stays in #508.
- **The one-clock rule and the Defer posture** (`AGENTS.md:132-142`, `gc.rs:78-105`,
  #557) are constraints on the design space, not open questions.
- **Record pattern:** ADR-0046 — a new namespace concept gets real, first-class records
  (`bucket:` via `bucket_key`, `metadata.rs:48-50`), not encodings synthesized into an
  existing namespace. Any new record class the protocol introduces states its key shape,
  who writes it, who deletes it, and which scans see it (existing namespaces for
  reference: `inode:` / `dirent:` / `pending:` / `bucket:` / `orphan:` at
  `metadata.rs:30-69`; `desired:dserver:` at `desired_state.rs:33`).

## Deliverable mechanics

- **File:** `docs/design/proposals/draft/0016-multipart-commit-protocol.md` — number 0016
  verified free at Plan. Copy `docs/design/templates/proposal.md`; keep its section set
  (Motivation / Design / Alternatives considered / Graduation criteria / Backward
  compatibility / Open questions). Frontmatter: `created:` (DD.MM.YYYY HH:MM), `type:
  proposal`, `status: draft`, `author: Eduard Ralph` (human identity only — no model/tool
  attribution anywhere, maintainer's standing rule), `tracking-issue:` #626, `tags:`
  including `proposal`, `s3`, `multipart`, `metadata`.
- **Index row:** add 0016 to the table in `docs/design/proposals/README.md`, mirroring the
  0014 draft-row shape; the relative link must resolve (the render link audit checks it).
- **Placement of the failure-mode tables:** under Graduation criteria or as a Design
  subsection — Do's choice; presence per decision is what leg B requires. The
  per-decision tables plus an explicit F1–F10 disposition list (eliminated-by /
  bounded-non-safety-cost-with-follow-up / flagged-NEEDS-HUMAN) must be findable as such,
  not diffused into prose.
- **Sequencing answer:** a normative statement on implementation order — #625 (the reaper
  implementation) relative to #508 (multipart) — placed where the reader finds it
  (Motivation or a dedicated subsection).
- **ADR graduation:** name which decisions (if any) should graduate to ADRs because they
  outlive multipart (#504 step 2, resumable writes) — as recommendations only.
- **Immutability:** the patch adds a new draft file and edits the proposals index README —
  both explicitly allowed by the docs-immutability rule; it touches NO accepted/stable
  document (ADR-0037 §CI enforcement).
- **Do not weaken an invariant to make its enumeration easier** — a clause of the
  Invariant above is not the builder's to trade away: if the design concludes one cannot
  be met, surface it as a flagged sign-off question (NEEDS-HUMAN) in the proposal. Only
  sub-safety trade-offs (availability / latency / capacity) may be recorded as accepted
  costs, each with its bound and follow-up — never a silent rescope.

## STOP discipline

Draft only until Check sign-off. Whether a draft branch/PR appears mid-cycle is the
driver's and the active leaf's contract, not this brief's to grant — the deterministic
publish step opens the draft PR on accept (useful CI: the host's always-on prose jobs and
`docs-immutability` run there). The PR MUST NOT be marked ready before sign-off accepts —
and the proposal itself ships at `status: draft`: its ratification (draft → accepted) is a
separate, later governance act under ADR-0037, never this cycle's.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Why rejected: gating T4 review red (11 blocking findings, none rejected) plus reviewer FAILs on C5/T2/T4 and seven adversary refutations — the bounding story and the reaper's edges are unsettled. Decisive for re-plan rather than re-do: the release that ships multipart MUST support objects over 10 GiB, so chunk-map segmentation (FU-1) can no longer be a parked follow-up — it enters scope, which changes the brief. Scope change (the reason this is iterate-plan, not iterate-do): - Segmentation in scope: >10 GiB objects required at launch; the honest no-segmentation ceiling is ~5–10 GiB even with large chunks. Planner decides the structure (expand 0016 vs. a companion proposal 0016 depends on). Note: a segmented map cannot be published in one batch (transaction envelope), so segmentation needs its own staged publication (write segments, then root flip) — same class as 0016's retirement-ledger machinery; design it with, not beside, that pattern. Settled directions to carry forward (human's calls at sign-off — do not re-derive): - Session lifetime: deployment-wide DEFAULT hard ceiling W_session measured from initiation (Amazon AbortIncompleteMultipartUpload precedent, NOT opt-in); per-bucket policy may later tighten below it, never loosen. Rewrite D1's "no timer on total life" row to "no correctness timer — publication is proved by records; one administrative ceiling bounds residency" (row as written contradicts the reaper for T >= W_open). F6's drain bound re-derives as W_session; FU-2 becomes the urgent-drain remedy, not the bound. Reaper stays record-only (no topology coupling). - Restore trace (outcome c): KISS — no resumption semantics. A metadata restore fences/aborts every session open in the restored image; an aborted upload starts from the beginning. Add the execution-register row. - Admission: the limit is GUARANTEED, not approximate — serialized slot reservation (counter CAS'd in the create batch, released in the terminal delete); contention at the counter is the 503 SlowDown backpressure. Reverses the "no hot counter" stance for Create only (the retry-storm objection applies to part commits, which stay counter-free). Rationale: cap overrun halts the maintenance plane — data-loss-class. - Namespace cardinality (structural rework): owned pending: entries get per-session indexed access (no global pending: scan in the reaper); retire: is walked in bounded key ranges (sharded or cursor-keyed) and its growth from ordinary overwrites is bounded or alarmed; in-flight part concurrency gets an admission cap so the pending population has an enforceable formula; admission counts ALL session records including Completing/Aborting/Completed tombstones; drain health (obligation count, oldest age) is a first-class alarm. - Mechanical repairs: W_completing measured from the fence instant (stamp it in the session record); UploadPart cumulative early-refusal restated as best-effort with the authoritative check at Complete; clock-lifecycle table must own honestly that the reaper evaluates owned-lease liveness (abandonment condition ii); add the reaper stale-snapshot rule ("step-5 judgment no staler than the entry it condemns") with its DST observable. - Honest arithmetic: state the computed ceilings as real numbers in the accepted-costs register — max object size per chunk-size choice, and the concurrent-session capacity that falls out of whatever the reworked bounding formula is.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 11 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
