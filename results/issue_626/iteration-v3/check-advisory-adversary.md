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
