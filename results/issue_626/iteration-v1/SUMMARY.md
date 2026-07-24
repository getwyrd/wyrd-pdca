# Result — issue 626 / multipart-commit-protocol

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: The multipart **commit protocol** — what happens *underneath* a
- Success criterion: two legs, both evaluated at Check on the patched tree.
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope: ONE logical change: author draft proposal 0016 — the multipart commit

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: patch touches no Wyrd crate (docs/CI only) — nothing to verify per-fix; the C4-ci gate covers it.
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — INFO Diff changes no Rust source files

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 11 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Task under review: add draft proposal 0016 and its index entry to settle the multipart publication, staging-protection, bounded-retirement, and abandoned-upload reaper protocol required before #508.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance boundary is decidable: exactly the indexed draft plus six invariant/failure-mode decisions and F1–F10 dispositions, with the artifact grounded at `docs/design/proposals/draft/0016-multipart-commit-protocol.md:15`. |
| C2 Reproduction (red pre-fix) | PASS | This is a born-at-tier design artifact, so the legitimate red is absence of any defined exit after a lost publication CAS; the target source documents that unresolved execution at `docs/design/proposals/draft/0016-multipart-commit-protocol.md:87`, while the unpatched prose gates correctly remain green rather than simulating a regression test. |
| C3 Change | PASS | The human need not arbitrate scope: the target has exactly the new draft and one index edit, and the registration is present at `docs/design/proposals/README.md:32`; no crate, workflow, or frozen design document changed. |
| C4 Verification (red→green) | NEEDS-HUMAN | Accept the mechanical evidence only if the recorded whole gate is trusted — I independently reproduced green `typos`, docs lint, and 98-page render/link audit, but the exact full rerun stopped at `cargo deny check` because its advisory lock is read-only on this host, so the asserted complete green is provisional rather than a patch defect. |
| C5 Causal adequacy | FAIL | The protocol does not yet meet its own bounded-exit invariant: a continuously progressing upload is not reaped (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:598`) and can retain an existing fragment on a draining server indefinitely, contradicting the asserted `W_open` drain bound at `docs/design/proposals/draft/0016-multipart-commit-protocol.md:323`; the design must bound or fence that execution before F6 can be settled. |
| T1 Structure | PASS | The artifact carries the required draft frontmatter at `docs/design/proposals/draft/0016-multipart-commit-protocol.md:1` and exactly the template's six top-level sections, so no structural choice remains for sign-off. |
| T2 Shape | FAIL | The reaper shape still contains an unbounded global `scan("pending:")` at `docs/design/proposals/draft/0016-multipart-commit-protocol.md:614`, while admission bounds only sessions × parts at `docs/design/proposals/draft/0016-multipart-commit-protocol.md:580`; the human cannot accept the “never scans an unbounded namespace” claim until pending entries (including chunks in flight and non-multipart producers) are included in an enforceable bound or indexed per owner. |
| T3 Runtime | N/A | The patch is a design document and index row only; runtime behavior is owed by #625/#508, as the implementation split states at `docs/design/proposals/draft/0016-multipart-commit-protocol.md:831`. |
| T4 Contribution | FAIL | The contribution does not clear the brief's refutation bar because the progressing-upload drain trace and unbounded pending-prefix trace are absent from the execution register beginning at `docs/design/proposals/draft/0016-multipart-commit-protocol.md:657`; either trace defeats an enumerated settlement. |
| T5 Judgment | NEEDS-HUMAN | Architecture/founding-maintainer authority must decide whether proposal 0016 is fit to become the protocol of record after the two settlement failures are repaired, because it changes maintenance-plane safety and deployment ordering (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:102`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the repaired proposal is sufficiently complete to unblock #508 and constrain #625 — acceptance matters because the current draft governs data-loss prevention, bounded reclamation, and upgrade order across both slices. |

Prior-art caveat: affected-path merged history was checked locally and 0016 is the next proposal number, but closed/rejected remote work was not mechanically available in the supplied artifacts; human sign-off should confirm the brief's claimed #508 rejection history and absence of a conflicting closed proposal.

### Advisory — adversary

# Adversarial review — issue 626 (draft proposal 0016, multipart commit protocol)

Lens: the brief's leg-B Refutation standard — construct an execution exhibiting outcome
(a)–(d) that is absent from the proposal's enumeration, or an unwarranted claim in its
disposition tables. All cites on the patched worktree at `$PDCA_TARGET` (cd82a29 + patch).
Note the deterministic side is already red: `check-gates.json` `T4-batch-review` = fail
(11 blocking); nothing below duplicates that gate — these are brief-aware leg-B findings
the diff-and-rubric passes are not armed to make.

## Refutations that landed

- NEEDS-HUMAN [impl] — **F7's "Eliminated" claim has a hole: the reaper's own
  `scan("pending:")` walks a namespace the admission invariant does not bound.** The
  bounding invariant counts sessions × part *records*
  (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:586`), but reaper step 5
  (`0016:614`) — the arm decision 5's deployed-configuration reclamation depends on — scans
  `pending:`, whose owned-entry cardinality is per-*chunk* of *in-flight* parts, and no
  admission fence bounds in-flight part concurrency (only `CreateMultipartUpload` is
  admission-controlled, `0016:581`). Concrete failing case: ~100 open sessions × thousands
  of concurrently started parts each write one durable owned `pending:` entry per chunk at
  intent (before any byte lands, decision 5.1); gateway crashes strand them; once the
  namespace crosses `SCAN_CAP = 1 << 20` (`crates/traits/src/lib.rs:286`), step 5's scan
  fails loud with **no partial result** — the only mechanism that can ever reclaim owned
  entries can no longer run: outcome (a), F7 realized in the deployed (`Defer`) posture
  where today's custodian doesn't even scan `pending:`
  (`crates/custodian/src/gc.rs:146-148` — the scan at `:305` is `Reclaim`-gated). X19/X20
  (`0016:686-687`) and the "Reaper unavailable" row (`0016:636`) count only `mpu:`/staged
  records; no row bounds `pending:`. The design needs an in-flight-part admission bound (or
  a pagination story for owned-entry reclamation) and the corresponding execution row.
- NEEDS-HUMAN — **The drain-stall bound `W_open` is unwarranted; the true bound is session
  lifetime, which the design deliberately refuses to bound.** `0016:327` claims every
  staged fragment on a draining server "exits within `W_open` (published, aborted, or
  reaped)" — false for a *live, progressing* session: `W_open` is the abandonment window,
  and decision 1's own failure row (`0016:299`) requires a session that idles then resumes
  to Complete successfully, i.e. sessions lasting days are supported by design. Concrete
  case: part 1 lands on server X; operator requests drain of X; the client uploads one part
  every `W_open/2` for a week; `reconciliation_status(X)` stays `Pending`
  (`crates/custodian/src/desired_state.rs:157-164` + decision 2's "count staged as held")
  the whole week. The F6 disposition (`0016:782`) and the accepted-cost row (`0016:796`)
  both record the bound as `W_open` — but the brief permits accepted costs only when
  *bounded*, so as stated the F6 drain half is an unbounded availability cost wearing a
  bounded label. Resolution is a design choice (pull FU-2 into scope, bound staged
  residency on a draining server, or flag the unbounded stall for sign-off) — maintainer's
  call.
- NEEDS-HUMAN [impl] — **Decision 1's last failure-mode row contradicts the reaper design.**
  `0016:299` demands: "stages one part, idles past T with a live client, then uploads more
  and Completes MUST succeed" — for any `T`. Decision 6's abandonment rule (`0016:558`)
  reaps exactly that session once `T ≥ W_open` (idle, no live lease — a "live client" that
  isn't uploading holds none, since renewal only runs between chunk writes,
  `crates/core/src/write.rs:485-500`), and FU-4 records the reap of a silent-but-live
  client as an accepted cost. A conformance test lifted verbatim from D1's row with
  `T ≥ W_open` must fail against the design's own reaper. The row needs its bound stated
  (`T < W_open`, or "while holding a live lease").
- NEEDS-HUMAN [impl] — **A restore execution with outcome (c) is absent from the register;
  X16 covers only the benign direction.** `0016:683` disposes "restored to a point before
  the part records existed." The missing trace: snapshot taken while a session is `Open`
  with staged parts → session aborted (or reaped) → retirement drain orphan-marks → GC
  reclaims the fragments from disk after grace → metadata restored to the snapshot. Now
  `mpu:` is `Open@E` and the `part:` records are resurrected, the orphan evidence is
  rewound away, and the bytes are physically gone. A retried/late Complete passes decision
  1's publication proof in full — the proof is *records-only* ("the protecting record is
  still exactly as read", `0016:288-291`) and cannot see that the restore falsified the
  records — and publishes a chunk map over bytes GC **has reclaimed**: outcome (c),
  verbatim. Unlike the pre-existing single-PUT resurrection class, this trace *mints a new
  publication* post-restore. Needs an execution row and a disposition (e.g. a restore
  procedure MUST fence/abort every resurrected session, or a flagged sign-off question).
- NEEDS-HUMAN — **The proposal never computes what its own settled ranges force, and the
  computed numbers undercut its motivation.** (1) Object ceiling: the knob rule
  `max_chunkref_bytes × MAX_MAP_CHUNKS ≤ V/2` (`0016:466`) against the proposal's own
  encoding arithmetic (~131–302 B/`ChunkRef`, `0016:434`) forces `MAX_MAP_CHUNKS` ≈
  165–390, i.e. a **~165–390 MiB max object at the default 1 MiB chunk size** — *below*
  the 5 GB PutObject ceiling the Motivation says multipart surpasses (`0016:76`); reaching
  even 5 GiB needs 13–32 MiB chunks traded against the gateway memory budget
  (`chunk_size × max_concurrent_encodes`). The F4 cost row's framing "well under S3's
  5 TiB" (`0016:780`) obscures this. (2) Session ceiling: the F7 invariant (`0016:586`)
  with `MAX_PARTS_PER_SESSION = 10_000` forces `MAX_OPEN_SESSIONS` ≈ **~100 fleet-wide**
  before headroom — `503 SlowDown` at roughly a hundred concurrent multipart uploads —
  and this capacity cost appears nowhere in the accepted-costs register. Both numbers are
  fitness-to-purpose facts the human should see stated before accepting; whether FU-1
  (segmentation) must be pulled forward is the maintainer's call.
- NEEDS-HUMAN [impl] — **The clock-lifecycle table contradicts decision 6's own abandonment
  rule.** The staging-lease row (`0016:651`) claims the lease stamp is evaluated by
  "**nothing that decides reclamation**", but abandonment condition (ii) (`0016:558`) has
  the reaper — the reclamation decider — evaluate owned-lease liveness ("owns no unexpired
  `pending:` lease"). That read is written by the write path and evaluated cross-component
  by the reaper, exactly the surface `AGENTS.md:132-142` requires the table to own
  honestly (owned entries carry no `clock_source`; the session-level guard vouches only for
  session stamps). Fail direction is safe (skew → false-positive reap → FU-4's fenced,
  bounded cost), so this is a conformance fix to F10's deliverable, not a safety refutation
  — but as written the F10 "Eliminated" row rests on a table that misstates the design.
- NEEDS-HUMAN [impl] — **Decision 6's failure-mode table is missing the stale-snapshot mode
  its own algorithm invites.** Step 5 (`0016:614`) filters `pending:` entries by "owner's
  session is absent or non-Open"; the pass listed sessions in step 1, so the natural
  implementation judges against a snapshot older than the pending scan and reclaims the
  in-flight entries of a session *created mid-pass* — orphan-marking a live streaming
  part's fragments and killing its renewals (`crates/core/src/write.rs:474-478`, "refuse
  rather than resurrect"). The downstream defences that mask it (X29's GC reference
  precedence, the renewal abort) are nowhere cited as load-bearing for this mode, and the
  table (`0016:628-637`) has no row pinning "the step-5 judgment must be no staler than the
  entry it condemns" with its DST observable — the brief demands the reaper's race modes be
  enumerated, and this one is absent.

## Attempted and could not refute

- **Fence/epoch machine (ABA, lost-CAS, rollback races):** epochs are monotone across
  `Completing → Open` releases, every transition CASes exact bytes
  (`crates/traits/src/lib.rs:836-843` semantics), the crashed-completer publish serializes
  against the reaper rollback in either order, and publication + `Completed` transition in
  one batch closes F5's double-publish. Could not construct a wedge or a double-publish.
- **The O(1) publication claim (decision 1.3):** since every `part:` mutation carries
  `require(mpu == Open@E)` and the fence bumps the epoch, the single session precondition
  does prove part-set immutability after the fence; verified nothing else writes `part:`
  (decision 2's reconstruction row is fenced the same way). Holds.
- **X29's precedence claim:** verified GC evaluates the reference gate before orphan input
  (`crates/custodian/src/gc.rs:157-176`), and the conservative no-evidence arm at
  `:183-187`. Holds as cited.
- **Serialization identity (`PendingEntry.owner`):** verified `renew_pending` /
  `live_lease_guards` pin exact stored bytes (`crates/core/src/metadata.rs:748-793`), so
  the proposal's `skip_serializing_if` reasoning and its named round-trip test are the
  right treatment, matching `AGENTS.md:170-174`.
- **Leg A1 evidence:** did not re-run `xtask ci` (compile-heavy; the gating row asserts
  pass) but manually audited the risky surface — all nine relative link targets of the new
  file resolve on the patched tree, the index row mirrors 0014's shape
  (`docs/design/proposals/README.md:32`), frontmatter and section set match
  `docs/design/templates/proposal.md`, and the number 0016 is free. No refutation.
- **Reviewer-verdict attack:** `check-gates.json` honestly reports `T4-batch-review` red
  and declares `C4-verify`/`C5-mutants` vacuous on a no-code diff — no green is claimed
  that the evidence does not carry. Nothing to refute there.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Accept the mechanical evidence only if the recorded whole gate is trusted — I independently reproduced green `typos`, docs lint, and 98-page render/link audit, but the exact full rerun stopped at `cargo deny check` because its advisory lock is read-only on this host, so the asserted complete green is provisional rather than a patch defect.
- [ ] T5 Judgment — Architecture/founding-maintainer authority must decide whether proposal 0016 is fit to become the protocol of record after the two settlement failures are repaired, because it changes maintenance-plane safety and deployment ordering (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:102`).
- [ ] Validation — fitness-to-purpose — Decide whether the repaired proposal is sufficiently complete to unblock #508 and constrain #625 — acceptance matters because the current draft governs data-loss prevention, bounded reclamation, and upgrade order across both slices.
- [ ] **F7's "Eliminated" claim has a hole: the reaper's own
- [ ] **The drain-stall bound `W_open` is unwarranted; the true bound is session
- [ ] **Decision 1's last failure-mode row contradicts the reaper design.**
- [ ] **A restore execution with outcome (c) is absent from the register;
- [ ] **The proposal never computes what its own settled ranges force, and the
- [ ] **The clock-lifecycle table contradicts decision 6's own abandonment
- [ ] **Decision 6's failure-mode table is missing the stale-snapshot mode
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 11 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Plan
- Iteration delta (if iterating): Why rejected: gating T4 review red (11 blocking findings, none rejected) plus reviewer FAILs on C5/T2/T4 and seven adversary refutations — the bounding story and the reaper's edges are unsettled. Decisive for re-plan rather than re-do: the release that ships multipart MUST support objects over 10 GiB, so chunk-map segmentation (FU-1) can no longer be a parked follow-up — it enters scope, which changes the brief. Scope change (the reason this is iterate-plan, not iterate-do): - Segmentation in scope: >10 GiB objects required at launch; the honest no-segmentation ceiling is ~5–10 GiB even with large chunks. Planner decides the structure (expand 0016 vs. a companion proposal 0016 depends on). Note: a segmented map cannot be published in one batch (transaction envelope), so segmentation needs its own staged publication (write segments, then root flip) — same class as 0016's retirement-ledger machinery; design it with, not beside, that pattern. Settled directions to carry forward (human's calls at sign-off — do not re-derive): - Session lifetime: deployment-wide DEFAULT hard ceiling W_session measured from initiation (Amazon AbortIncompleteMultipartUpload precedent, NOT opt-in); per-bucket policy may later tighten below it, never loosen. Rewrite D1's "no timer on total life" row to "no correctness timer — publication is proved by records; one administrative ceiling bounds residency" (row as written contradicts the reaper for T >= W_open). F6's drain bound re-derives as W_session; FU-2 becomes the urgent-drain remedy, not the bound. Reaper stays record-only (no topology coupling). - Restore trace (outcome c): KISS — no resumption semantics. A metadata restore fences/aborts every session open in the restored image; an aborted upload starts from the beginning. Add the execution-register row. - Admission: the limit is GUARANTEED, not approximate — serialized slot reservation (counter CAS'd in the create batch, released in the terminal delete); contention at the counter is the 503 SlowDown backpressure. Reverses the "no hot counter" stance for Create only (the retry-storm objection applies to part commits, which stay counter-free). Rationale: cap overrun halts the maintenance plane — data-loss-class. - Namespace cardinality (structural rework): owned pending: entries get per-session indexed access (no global pending: scan in the reaper); retire: is walked in bounded key ranges (sharded or cursor-keyed) and its growth from ordinary overwrites is bounded or alarmed; in-flight part concurrency gets an admission cap so the pending population has an enforceable formula; admission counts ALL session records including Completing/Aborting/Completed tombstones; drain health (obligation count, oldest age) is a first-class alarm. - Mechanical repairs: W_completing measured from the fence instant (stamp it in the session record); UploadPart cumulative early-refusal restated as best-effort with the authoritative check at Complete; clock-lifecycle table must own honestly that the reaper evaluates owned-lease liveness (abandonment condition ii); add the reaper stale-snapshot rule ("step-5 judgment no staler than the entry it condemns") with its DST observable. - Honest arithmetic: state the computed ceilings as real numbers in the accepted-costs register — max object size per chunk-size choice, and the concurrent-session capacity that falls out of whatever the reworked bounding formula is.
- By / date: Eduard Ralph / 2026-07-23

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
