# Build notes — issue 626, iteration 6 (rework of 0016 draft proposal)

**Deliverable:** a design document, not code. I reworked
`docs/design/proposals/draft/0016-multipart-commit-protocol.md` (starting from the
iteration-5 artifact, `iteration-v5/patch.diff`, applied to `$PDCA_WORKTREE`) plus its
index row in `docs/design/proposals/README.md`. Patch touches **exactly** those two doc
paths and nothing else (verified: `git diff` shows only the README `+1` and the new 0016
file). No `crates/`/`xtask/` change — out of scope by the brief.

## What I was asked to fix (iteration-5 carry-forward)

Six T4 findings (from `$PDCA_BUNDLE/review-batch.md`) + three adversary refutations +
keep the ⚑ serialization-cost question flagged. I addressed **all** by fixing the text
(none recorded-rejected, so each leaves the next fresh T4 run). Located by section, not by
the iteration-5 line numbers, as the brief instructs.

### Finding 1 (`0016:393`) — shared `sinf:` CAS makes concurrent part commits conflict; the generic compensation path discards a valid upload on a counter-only collision
**Fix (retry the benign collision — exactly the fix the reviewer named).** The part-commit
batch carries three preconditions (`mpu == Open@E`, the part key, and the `sinf: -1`
slot-release CAS). On `Conflict` the committer **re-reads to classify** (rubric *re-read to
establish what happened*, `traits/src/lib.rs:738-745`): session-left-`Open` ⇒ `404`, no
self-compensate (X7); same-part-number loss ⇒ genuine losing-writer compensation (X10);
**counter-only collision ⇒ retry the batch against the re-read counter**, never
compensate — terminates in ≤ `MAX_INFLIGHT_PARTS` rounds, monotone, no livelock, **loses
no upload** (new X52). Touched: §1 `sinf:` row, batch-inventory part-commit row, decision
5 rule 2 (rewritten as the three-way classification), the in-flight-concurrency paragraph,
the ⚑ open question, and register X52.

*Alternative considered and rejected — decouple the release into a standalone batch (remove
the cause rather than guard the collision).* Cost shown: it would change §1 `sinf:` row +
part-commit batch row (drop the CAS) + a new standalone release-step batch row + decision 5
rules 2/3 + the teardown reasoning (~6 edits vs my 5), **and** introduce a new
"committed-but-unreleased slot" accounting state (a committed part whose separate release
batch is lost holds a slot until teardown) — a fresh surface for the next adversary. It is
also *not required to honour D-C*: D-C's "part commits stay counter-free" is about the
**global** `mpuctl:count` (which part commits still never touch), **not** the per-session
`sinf:` in-flight cap (a D-D device). The reviewer's finding literally says "instead of
retrying the counter-only collision" — retry is the named fix, keeps the ⚑ question intact
(brief mandate: keep it flagged), and adds no new state.

### Finding 2 (`0016:522`) + adversary C — segment repoint vs supersede race strands the moved fragment; X47 over-claimed "outcome (a) closed"
**Fix (pre-evidence the destination — the adversary's own suggested remedy).** A
reconstruction/rebalance repoint of a committed segmented object's `seg:` fragment now
(i) **pre-marks `orphan:<P_new>` before writing the destination fragment**, then (ii) CASes
`require(seg == prior)` **and** `require(inode == prior)` that, on **win**, adopts `P_new`
(deletes the pre-mark) and orphans the vacated `P_old`, and on **loss** is a no-op that
**leaves the `P_new` pre-mark standing** for GC. The superseded-generation retirement names
the segments **by `seg:` key** and the drain re-reads each seg's **current** placement at
drain time (never a frozen one); post-supersede no repoint can win (`require(inode ==
prior)` fails). So on **every** interleaving every position is seg-referenced,
pre-marked, or orphaned — the strand the earlier X47 missed (repoint winning *before* the
supersede read) is closed. Touched: §1 `seg:` writer clause, decision 2 reconstruction +
rebalance rows, decision 7f, a new batch-inventory "segment repoint" row, rewritten X47,
the decision-7 failure-mode repoint row, and a new accepted-cost row (a lost repoint's
destination fragment reclaimed by GC — one fragment per lost repoint, bounded, named
reclaimer).

### Finding 3 (`0016:844`) — lease-renewal refusal doesn't cancel an already-authorized fragment write; it can land after orphan grace and stay unevidenced
**Fix (a real deadline + one-clock grace, replacing the unfounded "≈15 s").** The
iteration-5 argument that a straggler lands "≤ ~half a lease TTL (≈15 s) after the fence"
was the hole: *refusing to renew does not cancel an in-flight write*. The bound is now the
**fragment-write deadline `W_write`** — every fragment write to a D server is a bounded,
fail-closed await (the rubric's *await discipline* MUST, `AGENTS.md:181-183`), so it cannot
land arbitrarily late — coupled to the orphan grace by **`G_orphan ≥ W_write`** under the
one deployment wall clock, so an authorized fragment lands *before* its position's grace
elapses. Position coverage (the reaper marks the full `staged` placement, fixed at intent)
plus the deadline close it. The renewal refusal is now stated as a *supporting* property,
not the bound. Touched: decision 5 rule 1 (rewritten as two properties), a new knob-table
row (`W_write`/`G_orphan`), two clock-lifecycle rows (Orphan grace constraint + a new
Fragment-write deadline row), X49, and the decision-5 late-fragment failure row.

### Finding 4 (`0016:1164`) + adversary A — "uniform" single-PUT segmentation is underspecified (no upload id / session / `Completing` epoch to anchor it)
**Fix (carve single-PUT out of segmentation — the brief's first-offered option).**
Segmentation is now **multipart-only**: staged publication needs an upload-id, a fenced
`Completing@E`, and the fence epoch `E` that keys `seg:<id>:<epoch>:<i>` and its
crash-evidence/rollback/reaper machinery — none of which a single `PutObject` has. Instead a
single PUT publishes a **flat** map and reaches large sizes by **choosing a chunk size that
fits its declared `Content-Length` inside `MAX_MAP_CHUNKS`** (`chunk_size_effective =
max(DEFAULT_CHUNK_SIZE, ⌈Content-Length/MAX_MAP_CHUNKS⌉)`); a single PUT that cannot fit even
at `chunk_size_max` is **refused `400 EntityTooLarge`** (multipart required), never silently
segmented against a session that does not exist. Real numbers: S3's 5 GiB single-PUT max
fits once `chunk_size ≥ 5 GiB/MAX_MAP_CHUNKS ≈ 13.4–31 MiB` — reachable, the only cost is
gateway memory `chunk_size × max_concurrent_encodes`, registered. Touched: decision 7 intro
(new carve-out block), decision 4 map-value-ceiling bullet, the single-PUT open question
(now a pure code-factoring note, not a parked decision), backward-compat public-API bullet,
a new accepted-cost row, and register X50. This also removes the adversary-A gap (segmented
single-PUT had no crash-evidence/reclamation half) — by construction, since single-PUT no
longer segments.

### Findings 5 & 6 (`0016:925` ×2) — named regression tests require the reaper to drive `sinf → 0`, contradicting the protocol (crashed slots stay counted; `sinf` deleted outright)
**Fix (correct the test observable to the protocol's actual gate).** Both named tests
(decision 5 and decision 6 failure tables) now assert the reaper observes the session's
`sidx:<id>:` range **empty** before the terminal delete (which deletes `sinf:` **outright**;
`sinf` may still read `> 0` from a crashed slot) — the gate is the empty `sidx:` range,
**never** `sinf == 0`, exactly as §2 states. This is also the rubric's *count-based
assertion that can pass while the property fails* class: the old assertion (`sinf == 0`)
could never hold for a session with crashed residue, so the stated regression could not
validate the teardown.

### Adversary B — segmented GET tears when `seg:` records are deleted without reader grace
**Fix (a clock-free resolve-retry rule, new decision 7h).** Retirement/rollback deletes
`seg:` records; a concurrent GET (or maintenance pass) that read the root and finds a `seg:`
record absent mid-resolution **re-reads the root** — a changed/absent root means the
generation was superseded/deleted, so restart against the current root or answer
`NoSuchKey`; an *unchanged* root with an absent segment is **fail-closed** (a live
generation never loses a segment — `seg:` deletion always follows the root flip). No new
grace clock, no record-grace timer; closes the reader-transparency over-claim on every
interleaving. Touched: new decision 7h, backward-compat public-API bullet (qualified the
"reader-transparent" claim), a decision-7 failure-mode row, and register X51.

### ⚑ serialization-cost question — kept flagged (brief mandate)
The finding-1 correction closes the *correctness* hole (a benign counter collision is
retried, not compensated) but leaves the *cost* the ⚑ question is about intact — same-session
part starts and ends still serialize on the per-session `sinf:` counter (bounded by
`MAX_INFLIGHT_PARTS`), and every per-chunk `sidx:` intent still reads the session record. I
updated the ⚑ text to note the benign-retry, and it remains a NEEDS-HUMAN sign-off question
(owner: sign-off / #625), not resolved silently.

## Preserved (adversary-unrefuted) constructions — untouched
Per the iteration-5 carry-forward: the fence/epoch state machine, the O(1)
session-precondition publication proof, the per-attempt epoch-scoped `seg:` keys (F18), the
exactly-once terminal `mpuctl:count` decrement (X42), the restore fence (D-B/F13/X17/X17b),
the byte-budgeted batch inventory, and the derived-`MAX_SESSIONS`/`U_ref` aggregate bound.
My changes are additive refinements to reclamation evidence, the repoint, single-PUT scope,
reader safety, and test wording — none redesigns a survived construction.

## Rubric self-review (AGENTS.md §Review rubric & protocol — the section present in the worktree)
- **One clock per correctness lifecycle** (ADR-0009): the new `W_write`/`G_orphan` coupling
  and the Fragment-write-deadline clock row both name **one** source (deployment wall
  clock); no logical/wall mix. ✓
- **Absent/unsupported entries → explicit error, never silent skip or count-only assertion**:
  the resolve-retry rule (h) fail-closes on the impossible "unchanged root + absent
  segment"; the findings-5/6 fix replaces a count-only `sinf == 0` assertion with the true
  property (empty `sidx:` range). ✓
- **Transactions** (CommitUnknownResult outranks Conflict; re-read): the finding-1 fix keeps
  `CommitUnknownResult` distinct from `Conflict` and re-reads to classify (X38, rule 2). ✓
- **Await discipline** (every external await bounded): the finding-3 fix is grounded on
  exactly this MUST (`W_write` fail-closed). ✓
- **Serialization identity**: `PendingEntry` `owner`/`staged` `skip_serializing_if` treatment
  untouched. ✓
No new rubric violation introduced.

## Verification (docs-only deliverable — no regression test exists or is possible)
The brief declares **Test file: none** and **verification posture (a) NET-NEW**: leg B "red"
is criterion-absence exercised by the Check reviewer + adversary under the Refutation
standard (it fired for real in iteration 5 — 6 T4 blocking + 3 adversary refutations). There
is no production *code* path to drive and no honest headless unit test to write for a design
document; fabricating one would be the vacuous stand-in the process forbids. So I ship
`patch.diff` + these notes and **no test**, which is the declared, sanctioned posture (not a
NEEDS-HUMAN gap).

**Leg A1 (mechanical, gating `C4-ci` prose gates) — run green on the patched tree, using the
gate's own commands (not hand-rolled):**
- `typos` (typos-cli 1.48.0) — exit 0 on the whole worktree.
- `python3 docs/publishing/tools/lint_docs.py` — `lint_docs: OK`.
- `python3 docs/publishing/tools/render_site.py --check` — `link audit OK`, 98 pages (no
  dangling/relative-link break; the new 0016 index row resolves).
Re-run **also** against a pristine `cd82a29` worktree with `patch.diff` applied
(`git apply --check` OK, exactly the two doc paths) — all three green there too, so the
patch is commit-ready for the target's prose hooks. (No rustfmt/clippy concern: zero crate
files touched.)

### The three refutation questions, adapted to a docs deliverable
- **(a) Genuine red?** Yes — iteration 5 went red on precisely these 6 T4 findings + 3
  adversary refutations under the Refutation standard; each is now fixed in the text (so it
  leaves the next fresh T4 run) rather than silenced. The mechanical leg was green before and
  after (docs validity), so leg A carries no evidence either way — correctly, as the brief
  notes; the binding "red" is the judgment tier.
- **(b) Production path?** The deliverable **is** the production artifact (the settled design
  document that #508/#625 lift their success criteria from), not a test of separate code. No
  stand-in/mock is involved; the reviewer/adversary read this very document.
- **(c) Fixture includes the fault?** N/A for a design doc; the analogue is that each fix is
  written as an enumerated failure-mode/execution-register entry with the concrete crash/race
  it disposes of (X47 both interleavings, X49 straggler-past-grace, X50 unfittable single
  PUT, X51 concurrent seg-delete GET, X52 counter collision), so the fault is named, not
  curated out.

## Manual-validation steps for the human at sign-off
1. Confirm the two doc paths only (`git apply --stat patch.diff`).
2. Read the seven decisions' failure-mode tables + F1–F18 disposition + the accepted-costs
   register (now with X50/X51/X52 and the two new cost rows) + the ⚑ open question (still
   flagged).
3. Adjudicate the ⚑ per-session `sinf:` serialization/retry cost (the one NEEDS-HUMAN item).
4. The gating `T4-batch-review` re-runs 3 fresh codex passes over the new diff; the six
   iteration-5 findings should not re-arise (each fixed).

## Scratch discipline
All throwaway work (the apply-check worktree, render `--out` dirs) was created under
`$PDCA_SCRATCH` with `pdca-builder-626-*` names and `rm -rf`/`git worktree remove`d before
finishing. No PR pushed/opened/marked ready (STOP discipline; draft-only until sign-off).
