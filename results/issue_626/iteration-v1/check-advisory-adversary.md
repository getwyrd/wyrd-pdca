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
