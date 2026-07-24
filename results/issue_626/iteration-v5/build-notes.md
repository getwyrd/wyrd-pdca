# Build notes — issue 626, iteration 5 (0016 multipart commit protocol)

**Deliverable:** a reworked design document (draft proposal 0016) + its index row. No code.
This is a **re-plan/re-do on the same document**: I applied `iteration-v4/patch.diff` to
`$PDCA_WORKTREE` (clean apply on `cd82a29`) and reworked the text to close the iteration-4
carry-forward and the six blocking T4 findings, preserving every construction the adversary
could not refute.

## What was rejected at iteration 4 (and where I fixed it)

The iteration-4 T4 gate went red with **6 blocking findings**; the reviewer failed C5/T2/T4
on the admission bound; the adversary landed four impl-level refutations plus four secondary
findings. I fixed each in the document (no `review-rejected.md` — none is rejected, all are
fixed, so each leaves the next T4 run when the text no longer exhibits it). Citations are
`0016:` new-file lines and `crates/...` on `cd82a29` (re-verified by reading the source).

### The core defect — mechanical aggregate admission (reviewer C5/T2/T4; review-batch #3)

The iteration-4 doc set `MAX_SESSIONS` as a **chosen** knob with a narrative range (≈104 / ≈11
/ ≈1), and its `W_ref` formula charged each committed `part:` record as **one unit**. Both are
wrong: a part expands to up to `MAX_PART_CHUNKS` chunk-refs held in memory, and a flat chosen
`MAX_SESSIONS` derived from a small-part assumption is overrun by a later large-part session —
the distribution-dependent hole C5/T2/T4 named.

Fix (the reviewer's explicitly-endorsed option: "derive `MAX_SESSIONS` from the worst legal
`MAX_PART_CHUNKS`"):
- New derived quantity `U_ref = (MAX_PARTS_PER_SESSION + MAX_INFLIGHT_PARTS) × MAX_PART_CHUNKS`
  — the worst-case per-session staged-reference footprint, **each committed part charged its
  full `MAX_PART_CHUNKS`** (knob table `0016`, decision 2 build-cost, decision 6 prose, register).
- `MAX_SESSIONS = ⌊W_ref / U_ref⌋` is **derived, not chosen**. The serialized counter enforcing
  it bounds `Σ_sessions(actual) ≤ MAX_SESSIONS × U_ref ≤ W_ref` **for every part-size
  distribution**, because every session is charged worst-case. An implementer cannot pick a
  small-part value; `MAX_SESSIONS` is not on the "chosen by #508" list (Open questions).
- `W_ref` restated as a **memory** budget (not `SCAN_CAP` — that conflation was a bug; `SCAN_CAP`
  bounds a single scan, `W_ref` bounds the in-memory reference set).

Honest numbers, recomputed at **in-range** `MAX_PART_CHUNKS ≤ 381`: with `W_ref = 4 M` chunk-refs,
small parts (`MAX_PART_CHUNKS = 5`) ⇒ `MAX_SESSIONS ≈ 79`; max in-range parts
(`MAX_PART_CHUNKS = 381`) ⇒ `MAX_SESSIONS ≈ 1`. This is ugly but honest and mechanical — and it
is the direction the reviewer required. **Why I accepted the ugly worst-case number rather than a
prettier one:** the only way to get high common-case concurrency is to charge *actual* footprint,
which needs a running fleet total that part commits must update — a fleet hot key on the part
path, which D-C ruled out (retry-storm). Charging worst-case at Create is the only mechanically-
sound reservation known at Create time. The escape is FU-3 (incremental/cached reference build).
The capacity trade (small parts → many sessions; large parts → few, more RAM) is stated as a real
number in the register.

### The knob-rule self-contradiction (adversary; part-size ceiling)

Iteration 4 computed the arithmetic at `MAX_PART_CHUNKS = 5,120` (a "5 GiB part") while its own
knob rule caps a `part:` value at `⌊50000/b_ref⌋ = 165–381`. A `part:` record is one JSON value,
exactly like the inode map — so a 5,120-chunk part is inadmissible. Fix (decision 4 arithmetic,
knob table, register):
- Stated `max_part_bytes = MAX_PART_CHUNKS × chunk_size` = **165–381 MiB at 1 MiB chunks** as a
  real number; `UploadPart` **refuses** a larger part with `400 EntityTooLarge` (never an
  over-`V` commit — the iteration-1/-4 hidden-ceiling class, here surfaced and bounded).
- Registered the S3-conformance consequence (S3 allows 5 GiB parts; we accept ≤ `max_part_bytes`
  at default chunks) as an **accepted capacity cost** with the tuning lever (raise `chunk_size`,
  a 5 GiB part fits at ≥ ~31 MiB chunks) and **FU-5** (part-record segmentation). The >10 GiB
  *object* requirement is met with many in-range parts, so this cost does not block launch.
- Recomputed all `W_ref`/`MAX_SESSIONS` scenarios in-range; purged every "5 GiB part = 5,120
  chunks admissible" claim (the remaining "5 GiB generation = 5,120 chunks" is correct — a
  *generation/object* can be 5 GiB, it is the per-*part* value that is bounded).

**This is the one judgment call I want the human to weigh at sign-off** (§6): capping parts at
`max_part_bytes` narrows strict S3 part-size conformance. I dispositioned it as a bounded accepted
cost (it is not a refutation outcome (a)–(d): a refusal is clean, the session stays usable, and
the object requirement is met otherwise) — but a maintainer may prefer FU-5 (part-record
segmentation) *in* this proposal rather than as follow-up. The design works either way; the number
is now honest and visible, which is what iteration 1's hidden-ceiling failure demanded.

### sidx: value carried no placement (review-batch #1, #4, #5, #6; adversary)

Four of the six blocking findings collapse to one root cause: the `sidx:` owned entry was
`PendingEntry{owner, lease}` with **no placement**, so the record-only reaper could not compute
`orphan:<dserver>:<chunk>:<index>` keys (placement-keyed, `metadata.rs:60-70`) and drain's
`genuinely_holds` could not count in-flight fragments per server (`(DServerId, FragmentId)` pairs,
`gc.rs:228-247`, `desired_state.rs:157-164`). Fix:
- A **second** additive optional field on `PendingEntry`: `staged: Option<StagedPlacement{scheme,
  placement: Vec<DServerId>}>` — the same `(scheme, per-fragment D-server vector)` a committed
  `ChunkRef` carries (`metadata.rs:124-136`). `write::intent` writes the `WritePlan` placement
  (`write.rs:45-54`) into it at intent time. Both fields `skip_serializing_if`, so a legacy
  `pending:` entry (both `None`) and an owned `sidx:` entry (both `Some`, across its own lease
  renewals) round-trip byte-identically — the serialization-identity treatment the brief said to
  preserve, now extended to `staged` (§1 field block, decision 5 rule 1, backward-compat,
  graduation criteria).
- This closes review-batch #1/#4 directly (reaper orphan keys; drain counting).
- **Late fragment (review-batch #5/#6):** a fragment authorized before the fence but landing after
  the reaper deleted its `sidx:` entry. Closed by (a) the reaper orphan-marking the chunk's **full**
  `staged` placement (every position it could land on), so a late fragment lands on an `orphan:`-
  covered position; and (b) the write path's renewal loop **refuses rather than resurrects** once
  the session leaves `Open` (`write.rs:474-478`), bounding a late write to ≤ half a lease TTL past
  the fence — far inside the orphan grace window, so GC never reclaims the evidence first. This is
  the *same* mechanism ordinary abandoned streaming writes rely on. New execution row X49, failure
  modes in decisions 5/6, and the teardown gate clarified as **empty `sidx:` range** (the true
  residue set), *not* `sinf == 0` (which would deadlock, since reaped crashed-part residue never
  releases its slot — F11a).

### Publication retry version (review-batch #2)

`publish_target` froze the version at fence time, but a supersede must publish `prior.version + 1`
(`metadata.rs:551`,`:595`,`:656`), so a lost-CAS retry against a newly-superseded prior recorded a
stale version. Fix: `publish_target` fixes the target **inode key** and the fence **epoch** (for
deterministic segment keys) but **not** the version; each flip attempt recomputes
`prior.version + 1` from the re-read prior (§1 mpu: row, §2, decision 3, X1, decision 3 failure mode).

### retire: cursor walk needs a store-seam change (adversary)

`MetadataStore::scan` is prefix-only, complete-or-`ScanCapExceeded` (`traits/src/lib.rs:772-776`,
`SCAN_CAP` `:275-292`) — no cursor/limit. The `retire:` namespace grows unbounded (overwrites),
so "cursor-keyed bounded ranges" is not expressible with `scan(prefix)`, and fixed sharding only
divides an unbounded population by a constant. I **named the seam change**: `MetadataStore` gains a
paginated `scan_page(prefix, after, limit)` implemented by every backend, the DST sim store, and
the conformance suite (the one narrow-seam addition, ADR-0010/0016). "What the implementing slices
change" now leads with it; the `retire:` row and reaper algorithm reference it. I chose to name the
seam rather than force a `scan(prefix)`-only redesign because no such redesign robustly bounds an
adversarially-concentrated `retire:` population below `SCAN_CAP` — I show that cost in the row.

### F13 restore-then-serve-before-fence (adversary)

The restore fence lived in the restore pass but nothing ordered it against gateways resuming
service. Added a **normative** line (decision 1.4, decision 2 restore row, backward-compat, X17b):
the restore-fence generation MUST complete before any gateway serves multipart verbs on the
restored image (wait, or refuse multipart until the fence generation is observed).

### Secondary adversary findings

- **Stale "≈ 52 × SCAN_CAP".** Purged every occurrence (decision 2 restore row, decision 5,
  X25, Alternatives, F11 observable); replaced with the honest `MAX_OWNED_FLEET` bound (W_ref-
  coupled, read only per-session). The disjointness argument survives without the inflated number.
- **seg: maintenance writers + repoint-vs-drain race.** §1 `seg:` row now lists reconstruction
  re-place and rebalance evacuation as writers of **committed** segment records, each an exact-bytes
  CAS `require(seg == prior)` **and** `require(inode == prior)` so a supersede/delete race is
  invalidated, not lost (decision 2 reconstruction/rebalance rows, decision 7 failure mode, X47).
- **Segmented reference-build work uncharged.** Decision 7(e) now charges committed segmented
  objects' resolution (up to ~1.78 M pairs per max object) as an **operational capacity cost** —
  not admission-bounded (durable data is not refusable), monitored via `W_ref_committed` telemetry
  + alarm (X48, FU-3). It is not a refutation outcome (nothing stranded/absorbing/over-map/over-
  envelope), only memory pressure.
- **Grace-start contradiction.** Decision 4 now says grace (`orphaned_at`) starts at the **drain**
  orphan-mark — *later* than today's inline mark (`metadata.rs:470-486`), so reclamation is never
  *earlier* than today (strictly safe); consistent with the register row and X45.

## What I preserved (per the iteration-4 carry-forward "PRESERVE")

Untouched: the fence/epoch state machine; the O(1) session-precondition publication proof (every
part/intent/slot batch carries `require(mpu == Open@E)`); per-attempt epoch-scoped `seg:` keys and
the X40 rollback→re-Complete-while-obligation-pending trace; the exactly-once terminal `mpuctl:count`
decrement (exact-bytes precondition, X42); the byte-budgeted batch inventory (§3); the restore fence
(F13/X17); the D-F arithmetic frame; the flagged D-C/D-D NEEDS-HUMAN open question.

## Alternatives ruled out (with cost)

- **A running fleet reference-work counter charged at every part commit** (would let admission
  charge *actual* not worst-case footprint, giving pretty concurrency numbers). Rejected: it puts a
  fleet hot key on the part-commit path — exactly the retry-storm D-C forbids — and re-opens the
  "part commits stay counter-free" guarantee. Cost of the rejected path: every one of the up-to-10k
  part commits per session CASes a global counter. Deferred to FU-3 (incremental/cached build).
- **Part-record segmentation now** (accept 5 GiB parts at default chunks). Rejected *for this
  cycle* on scope + surface: it adds a new record class and a second staged-publication family for
  parts (≈ another decision-7-sized section, ~80–120 lines and a new namespace for the adversary to
  attack), for a conformance case (parts > ~381 MiB at default chunks) that (a) most SDKs never hit
  and (b) chunk_size tuning already covers. Surfaced as FU-5 and flagged for the human — cheaper to
  add later than to defend an under-specified version now.
- **`sinf == 0` as the teardown gate** (the literal iteration-3/-4 finding-6 ask). Rejected: reaped
  crashed-part residue never releases its slot (kept counted, F11a), so `sinf` can stay `> 0`
  forever — gating on it deadlocks teardown. The **empty `sidx:` range** is the true residue set
  (intent precedes every fragment), so it is the correct gate; `sinf:` is discarded with the session.

## Refuting my own work (the three questions — honest answers)

This is a **docs-only design artifact**; the brief declares `Test file: none` and verification
posture (a) NET-NEW/born-at-tier — "red" for the settlement leg is criterion-absence judged by the
Check reviewer/adversary/human, and it *has* fired (iterations 1–4 went red on exactly this
predicate). There is no production code path and no fixture, so questions (b)/(c) are N/A by
construction; I did **not** manufacture a stand-in test.

- **(a) Genuine red?** Leg A1 (mechanical) is green on the patched tree (below). Leg B's red is
  demonstrated historically: on the iteration-4 text, the T4 gate produced 6 blocking findings and
  the reviewer failed C5/T2/T4 — i.e. reverting my rework re-exhibits the red. Each of the 6
  findings and 4 secondary findings is addressed in text with a named observable, so a fresh T4/
  adversary pass no longer has the same construction to land. I cannot *prove* leg B green (only the
  human can, at sign-off) — recorded honestly, not asserted.
- **(b) Production path?** N/A — the document *is* the artifact; my edits are the production change.
  No mock/copy exists to drive.
- **(c) Fixture includes the fault?** N/A — no fixture. The failure-mode tables and execution
  register embed the faults themselves (each way-to-implement-wrong names the observable that
  fails), which is the design-doc analogue and what leg B is judged against.

## Mechanical verification (leg A1) — run via the project's prose gates

Ran the exact tools `cargo xtask ci` runs first (`xtask/src/main.rs:1552-1553`), on the patched
`$PDCA_WORKTREE`:
- `typos` (whole tree): **OK**.
- `python3 docs/publishing/tools/lint_docs.py`: **OK**.
- `python3 docs/publishing/tools/render_site.py --check`: **link audit OK** (98 pages, no dangling
  link — the index row's relative link to `draft/0016-...md` resolves).

Patch touches **exactly two docs paths** (`git diff --cached --stat`): the new draft + the index
row — no `crates/`/`xtask/` change (leg A honest bound). Frontmatter matches the template
(`type: proposal`, `status: draft`, `author: Eduard Ralph`, `tracking-issue: "#626"`, tags incl.
`proposal`/`s3`/`multipart`/`metadata`); no model/tool attribution anywhere (grep clean). The full
`cargo xtask ci` additionally compiles/tests the Rust workspace, which this docs-only change does
not affect (iteration 4 established those pass modulo one unrelated flaky `gateway-s3` test).

## Self-review against the target's AGENTS.md review rubric

- *Serialization identity* (`AGENTS.md:170-174`): the new `staged` field is `#[serde(default)]` +
  `skip_serializing_if`; round-trip identity stated for both legacy `pending:` and owned `sidx:`,
  round-trip tests named in Graduation criteria. ✓
- *One clock per lifecycle* (`:132-142`): no new clock source; the late-fragment/grace reasoning uses
  the one deployment wall clock; the clock table is unchanged. ✓
- *Narrow trait seams* (`:143-145`): the one seam addition (`scan_page`) is named explicitly with
  its backend/DST/conformance obligations. ✓
- *Absent/unsupported entries* (`:175-177`): observables assert the property (exact orphan keys,
  `reconciliation_status == Pending`, version monotonicity), not counts. ✓
- *Docs currency* (`:154-157`): the proposal lands no code; "What the implementing slices change"
  states the living-architecture update lands in the implementing slice. ✓
