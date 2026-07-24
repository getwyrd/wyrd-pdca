# Result — issue 626 / multipart-commit-protocol

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: The multipart **commit protocol** — what happens *underneath* a
- Success criterion: two legs, both evaluated at Check on the patched tree.
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope: ONE logical change: REWORK draft proposal 0016 (starting from

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
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 9 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of the multipart commit protocol proposal intended to settle safe publication, bounded staging, reclamation, reaping, and segmented maps for assembled writes.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief gives a falsifiable two-leg settlement bar, exact two-file scope, seven decisions, F1–F18, and explicit human-only acceptance criteria (`brief.md:62`). |
| C2 Reproduction (red pre-fix) | NEEDS-HUMAN | Decide whether the archived rejected iteration is sufficient red evidence — the supplied artifacts contain only its narrative, so the asserted prior review failure could not be independently rerun (`brief.md:191`). |
| C3 Change | FAIL | The protocol must close the fence/stream race: part-intent commits have no session precondition, yet terminal deletion does not require `sinf == 0` or an empty `sidx`, so an uploader can create owned residue after fence-and-scan that an absent session can no longer lead the reaper to (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:339`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Decide whether to accept the recorded green or rerun on a host with a writable cargo advisory lock — typos, docs lint, and link audit passed, but independent `cargo xtask ci` stopped when `cargo deny` could not lock `/home/eddie/.cargo/advisory-dbs/db.lock`; the docs-only per-fix row is vacuous (`check-gates.json:1`). |
| C5 Causal adequacy | FAIL | The design still admits refutation outcome (a): after the reaper fences and scans, an already-reserved uploader may commit a new unpreconditioned intent, crash, and leave owner-tagged residue that expiry skips and no session-indexed pass can discover (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:688`). |
| T1 Structure | PASS | The diff is confined to the required proposal and index paths, and the target contains the required draft index row (`docs/design/proposals/README.md:32`). |
| T2 Shape | FAIL | The lifecycle shape lacks a serialization edge between in-flight intent creation and terminal teardown; “fence-then-walk” therefore does not establish that the walked set is closed (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:906`). |
| T3 Runtime | N/A | This cycle changes only a design document and index; no runtime implementation is delivered or directly exercisable (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:1185`). |
| T4 Contribution | FAIL | The recorded three-pass affected-diff review is red, and the independently grounded late-intent execution above is an unenumerated blocking safety defect; the brief confirms prior art was checked by affected proposal path across merged and rejected work (`brief.md:240`). |
| T5 Judgment | NEEDS-HUMAN | Decide whether the proposal can proceed after the teardown race is redesigned and re-reviewed — accepting it now would make #508 inherit a path to permanently protected residue (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:703`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the corrected proposal is fit to become the implementation contract for #508/#625 — governance acceptance of this draft is deliberately human-only and determines whether the safety model is authoritative (`brief.md:128`). |

### Advisory — adversary

# Adversarial review — issue 626 / multipart-commit-protocol (iteration 3)

Posture: assumed the reworked 0016 is wrong and tried to prove it. All six iteration-2
carry-forward defects ARE closed in the text (verified individually, see "could not refute"
below) — which is exactly the state in which a confirmatory pass relaxes. The refutations
below are **new** executions, each grounded on the patched doc and the cd82a29 source.
Note the bundle is already red at the gating T4 row (9 blocking, `check-gates.json`), so
these annotate a rejected-as-is artifact.

- NEEDS-HUMAN [impl] — **Outcome (a): owned residue of a session that COMPLETEs is stranded
  forever — the next F11a hole, one exit further on.** Concrete execution: a part upload
  crashes mid-stream (slot held; owned `pending:`+`sidx:` entries and fragments durable),
  the client re-uploads that part number under fresh chunk ids, Completes successfully →
  tombstone expires → terminal delete removes `mpu:`/`sinf:`. Nothing ever reclaims the
  crashed attempt's residue: the reaper walks `sidx:` only in the `Aborting` branches — the
  `Completed` branch is terminal-delete only
  (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:884-885`); the backstop
  triggers enumerate "aborted, reaped, or fenced by a restore" — Complete is missing
  (0016:703-709); the flip batch names only `part:` records (0016:343); after the terminal
  delete, "or vanishes" (0016:302) has **no discovery mechanism** — the design forbids the
  global `pending:`/`sidx:` scan that could find an ownerless range (0016:708-709); and the
  expiry sweeps "MUST skip session-owned entries" even under attested `Reclaim`
  (0016:710-715), while deployed GC is `Defer` (`crates/custodian/src/gc.rs:146-149`). So the
  exit-table claim "orphan-marked and deleted when its session leaves `Open` or vanishes"
  (0016:302) is unbacked for the Complete exit, the residue is a permanent fourth class, and
  it accumulates without bound across sessions over time. Absent from the register — X8
  (0016:1122), X25 (:1139) and X41 (:1155) all take the abort/reap path. Fix is iterable
  (e.g. walk `sidx:` in the `Completed` teardown before the terminal delete).

- NEEDS-HUMAN [impl] — **X25's "no global `pending:` scan exists to fail ScanCapExceeded"
  (0016:1139) is unwarranted: the in-tree restore pass IS one.** `reconcile_after_restore`
  builds its pending set via `meta.scan(b"pending:")`
  (`crates/custodian/src/restore.rs:185`, `:416-423`), and `MetadataStore::scan` fails loud
  above `SCAN_CAP = 1 << 20` (`crates/traits/src/lib.rs:286-292`). The doc bounds owned
  `pending:` only **per session** (≤ `SCAN_CAP/2`, 0016:648); its own fleet formula
  `MAX_SESSIONS × MAX_INFLIGHT_PARTS × MAX_PART_CHUNKS` (0016:737-739) reaches ~5.5×10^7 ≈
  **52× SCAN_CAP** at settled top-of-range knobs (104 × 524,288), and even modest choices
  (16 in-flight × 381 chunks × 104 sessions ≈ 634K) eat ~60% of the cap before ordinary
  pending traffic. Failing case: restore halts `ScanCapExceeded` — and the restore pass is
  **D-B/F13's enforcement point** (0016:445), so the F13 remedy is unavailable exactly when
  a restore is being attempted. Decision 2 modifies this very pass yet never re-derives its
  scan bound; a fleet-wide owned-pending constraint (< SCAN_CAP minus ordinary-pending
  headroom) is missing from the knob table. (Secondary: `gc.rs:300`
  `expired_pending_chunks` does the same global scan under attested `Reclaim`.)

- NEEDS-HUMAN [impl] — **Outcome (c) through the drain-invisibility window of an in-flight
  part; F6 "Eliminated (safety half)" (0016:1276) and X15's "the pre-wipe status was
  `Pending`" (0016:1129) are overstated.** The staged reference set is built "from the
  `part:` records of sessions that still exist" (0016:433-434); a still-streaming part's
  chunks have only owned `pending:` entries, and `reconciliation_status` answers from
  `referenced.placed` (`crates/custodian/src/desired_state.rs:157-164`). Failing case: a
  multi-hour part (explicitly supported, 0016:773-775) places chunks on server D; operator
  requests a drain of D afterwards; status reads `Satisfied` (fragments in neither arm);
  operator wipes; the part later commits (metadata-only, nothing verifies bytes) and
  Complete publishes a map naming wiped fragments. Also falsifies "the staged population on
  a draining server is fixed at the instant the drain is requested" (0016:456-458) — it
  *grows* at each part commit. Pre-existing at 30 s lease scale for single PUT; this
  protocol stretches the window to hours while registering only the part-record case
  (X14/X15, 0016:1128-1129).

- NEEDS-HUMAN [impl] — **Outcome (d): DeleteObject of a segmented object is a permanent
  commit failure — the >10 GiB object this proposal exists to enable cannot be deleted.**
  Today's delete is inline orphan fan-out: `delete_object` → `metadata::unlink`
  (`crates/server/src/lib.rs:544-559`), which "grace-records every fragment … in the SAME
  atomic commit" (`crates/core/src/metadata.rs:488-518`). Decision 4.1 converts only
  `commit_chunk_map_superseding{,_leased}` (0016:617-621); the batch inventory that claims
  to list "every batch this protocol commits" has no unlink row (0016:314-316), backward
  compatibility names only the overwrite and segmented-PUT changes (0016:1395-1400), and no
  register row covers DELETE (X20/X21 are overwrites, 0016:1134-1135). At decision 7's own
  ceilings: 51,480–198,120 chunks × 9 fragments = **463K–1.78M orphan puts (~23–89 MB of
  mutations) in one unlink batch** — permanently over `E_tx = 10 MB`. §1's `seg:` table even
  promises deletion routes through "the retirement drain … when the object … is
  superseded/**deleted**" (0016:217) — a mechanism no decision, batch row, or X row backs
  for the delete verb (and bulk `DeleteObjects`, issue #509, fans it out).

- NEEDS-HUMAN — **D-C tension resolved in-document rather than flagged: "part commits stay
  counter-free" vs. the `sinf:` CAS at every part boundary.** Each `UploadPart` CASes
  `sinf:` twice — reserve before streaming (0016:338) and release **inside the part-commit
  batch** (0016:213, :340). The doc re-derives D-C as fleet-counter-only (0016:732-741,
  :1201-1209) instead of surfacing the conflict as the flagged NEEDS-HUMAN question the
  brief demands for a direction the design cannot honour verbatim. Substantive side-effect:
  two concurrent same-session part commits now conflict at the `sinf:` key, so one whole
  commit batch (part-record put included) fails and retries — the serialization decision
  1.3 claims to avoid ("N concurrent part commits … do not conflict with each other",
  0016:377-381), and the accepted-costs register carries no row for the part-path retry
  cost. The reading may well be inside the maintainer's D-C/D-D intent (D-D *mandates* an
  enforced in-flight cap) — that adjudication is the human's, not the builder's.

- NEEDS-HUMAN [impl] — **Unbacked record claim in the clock table:** "owned entries carry
  the session's `clock_source`" (0016:1094), but the only `PendingEntry` change specified
  anywhere is `owner: Option<String>` (0016:240-243, :1160-1162;
  `crates/core/src/metadata.rs:344-350` is today's two-field record). The reaper's
  fail-safe skip for foreign-clock owned leases has no field to read. Either spec the field
  (with the same serialization-identity treatment §1 makes load-bearing for `owner`) or
  derive the guard from the session record's `clock_source` alone and fix the table row.

## Attempted and could not refute

- **X40/F18 epoch-scoped segment keys** (0016:982-1026): traced rollback→re-Complete→
  stale-obligation-drain in both orders, plus fence-release-then-retry and same-epoch
  unknown-result recovery — the per-attempt key ranges stay disjoint; the iteration-2
  refutation is closed by construction.
- **Terminal-delete exactly-once** (0016:305-312, X42): the `require(mpu:<id> == prior)`
  precondition makes the gateway/reaper race single-winner; no counter drift execution found.
- **Byte-budgeted batch inventory** (0016:314-357): recomputed every row; `B_seg = 50` and
  the ~1,000-mark drain rows are correct; no row exceeds `E_tx/2` (the iteration-2 envelope
  defect is fixed) — except via the un-inventoried delete verb, filed above.
- **D-F arithmetic** (0016:595-609, :1046-1064, :1290-1307): recomputed all ceilings —
  165–381 chunks, 312–520 segments, 50.3–193.5 GiB at 1 MiB chunks, 26.5–102 MiB chunks for
  5 TiB, `MAX_SESSIONS ≈ 104` — all check out.
- **Leg A mechanics**: frontmatter and section set match `docs/design/templates/proposal.md`;
  the index row mirrors 0014's shape; patch touches exactly the two allowed paths; `typos`
  and `render_site.py --check` re-run green on the patched tree (link audit OK, 98 pages).

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C2 Reproduction (red pre-fix) — Decide whether the archived rejected iteration is sufficient red evidence — the supplied artifacts contain only its narrative, so the asserted prior review failure could not be independently rerun (`brief.md:191`).
- [ ] C4 Verification (red→green) — Decide whether to accept the recorded green or rerun on a host with a writable cargo advisory lock — typos, docs lint, and link audit passed, but independent `cargo xtask ci` stopped when `cargo deny` could not lock `/home/eddie/.cargo/advisory-dbs/db.lock`; the docs-only per-fix row is vacuous (`check-gates.json:1`).
- [ ] T5 Judgment — Decide whether the proposal can proceed after the teardown race is redesigned and re-reviewed — accepting it now would make #508 inherit a path to permanently protected residue (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:703`).
- [ ] Validation — fitness-to-purpose — Decide whether the corrected proposal is fit to become the implementation contract for #508/#625 — governance acceptance of this draft is deliberately human-only and determines whether the safety model is authoritative (`brief.md:128`).
- [ ] **Outcome (a): owned residue of a session that COMPLETEs is stranded
- [ ] **X25's "no global `pending:` scan exists to fail ScanCapExceeded"
- [ ] **Outcome (c) through the drain-invisibility window of an in-flight
- [ ] **Outcome (d): DeleteObject of a segmented object is a permanent
- [ ] **D-C tension resolved in-document rather than flagged: "part commits stay
- [ ] **Unbacked record claim in the clock table:** "owned entries carry
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 9 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Rejected on the advisory review's grounded findings against the reworked 0016 text; the iteration-2 carry-forwards are confirmed closed — do not reopen them. Fix, in the doc: 1. Teardown/fence race (C3/C5/T2): part-intent commits carry no session precondition and terminal deletion does not require sinf == 0 / empty sidx, so an uploader can create owned residue after fence-and-scan that nothing can ever discover. Add a serialization edge between in-flight intent creation and terminal teardown (e.g. terminal delete preconditions on sinf == 0, or intents precondition on session state). 2. Completed-path residue (adversary outcome a): the Completed teardown never walks sidx:, so a crashed part attempt's owned residue is stranded forever when the session Completes. Walk sidx: in the Completed teardown before the terminal delete; add the missing exit-table/backstop/register coverage. 3. Scan-cap contradiction (X25): reconcile_after_restore does a global pending: scan; the doc's own fleet arithmetic (MAX_SESSIONS x MAX_INFLIGHT_PARTS x MAX_PART_CHUNKS) can exceed SCAN_CAP ~52x. Add a fleet-wide owned-pending constraint to the knob table and re-derive the restore pass's scan bound. 4. Drain invisibility (outcome c): a still-streaming part's chunks are invisible to drain/desired-state, so an operator can wipe a server and a later Complete publishes a map naming wiped fragments. Make in-flight owned pending: count as held for drain, or state and register the widened window honestly. 5. DeleteObject of a segmented object (outcome d): inline unlink fan-out at the settled ceilings is 463K-1.78M orphan puts in one batch — permanently over E_tx. Route delete (and bulk DeleteObjects, #509) through the retirement drain; add the batch-inventory row and register coverage. 6. Two smaller fixes: surface the D-C tension (part commits are not counter-free — sinf: CAS at every part boundary serializes same-session part commits) as an explicit flagged question rather than resolving it silently; and either spec the clock_source field on owned pending entries or derive the reaper's clock guard from the session record and fix the clock-table row.
- By / date: Eduard Ralph / 2026-07-23

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
