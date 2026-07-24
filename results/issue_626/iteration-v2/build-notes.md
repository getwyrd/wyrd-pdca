# Build notes — issue 626 / multipart-commit-protocol (iteration 2)

*Withheld from the reviewer; written for the human at sign-off.*

## What I shipped

A **rework** of draft proposal 0016, started from `iteration-v1/patch.diff` applied to
`$PDCA_WORKTREE` (applied cleanly), reworking the document per the eight binding sign-off
directions and the 11 carried T4 findings, adding decision 7 (segmentation), and computing the
arithmetic as real numbers. Exactly the two docs paths the brief's leg A specifies, nothing
else (`git diff --stat`: 2 files, 1302 insertions):

1. `docs/design/proposals/draft/0016-multipart-commit-protocol.md` — the reworked draft (1301
   lines; template section set intact — Motivation / Design / Alternatives considered /
   Graduation criteria / Backward compatibility / Open questions; frontmatter `type: proposal`,
   `status: draft`, `author: Eduard Ralph`, `tracking-issue: "#626"`, tags incl. `proposal`,
   `s3`, `multipart`, `metadata`).
2. `docs/design/proposals/README.md` — the index row (unchanged from iteration 1: `draft
   (settlement: #626)`, mirroring 0014's draft-row shape at `README.md:30`).

No file under `crates/` or `xtask/` is touched; no accepted/stable document is touched.

## Preserved from iteration 1 (the adversary could not refute these — brief says keep)

Kept verbatim or lightly edited: the fence/epoch state machine (§2; ABA/lost-CAS/rollback all
serialize on exact-bytes CAS + monotone epoch); the **O(1) publication proof** of decision 1.3
(one session precondition proves the whole part set is immutable after the fence, because every
`part:` mutation carries `require(mpu == Open@E)`); X29→X34 GC **reference-before-orphan**
precedence (`gc.rs:159-176`); and the `PendingEntry.owner` **serialization-identity** treatment
(`skip_serializing_if`, the round-trip test, `AGENTS.md:170-174`). I did not redesign what
survived.

## The eight directions, applied (each is a decision taken, not re-opened)

- **D-A — session lifetime.** Rewrote decision 1's "no timer on total life" row: there is **no
  correctness timer** (publication is record-proved), and one **administrative** ceiling
  `W_session` (from initiation, deployment default, per-bucket tighten-only; Amazon
  `AbortIncompleteMultipartUpload` precedent) bounds residency. The reaper gets a second
  abandonment arm (arm ii, `now - created_at > W_session`) and stays record-only. F6's drain
  bound **re-derives as `W_session`** (the iteration-1 `W_open` label was the unbounded-cost
  bug — a live, progressing session is bounded by `W_open` by nothing); FU-2 becomes the
  urgent-drain remedy, not the bound.
- **D-B — restore (outcome c), KISS.** No resumption across a restore: the restore pass
  **fences/aborts every restored `Open`/`Completing` session to `Aborting`**. This closes the
  iteration-1 adversary's sharpest trace (F13/X17): a records-only publication proof cannot see
  that a restore falsified the records, so the records-only proof is explicitly scoped to
  **unrewound** records (decision 1.4), and the restore's fence-all step re-establishes that
  scope. An in-flight upload at restore time restarts from the beginning (registered cost).
- **D-C — admission is guaranteed.** Reversed iteration 1's scan-then-create / "no hot counter"
  stance **for Create only**: a **serialized slot reservation** — the singleton `mpuctl:count`
  CAS'd `+1` in the create batch (with `require_absent(mpu:<id>)`), `-1` in the terminal delete
  (reserve/CAS, ADR-0007). Contention at the counter **is** the `503 SlowDown`. Part commits
  never touch it, so the per-part retry-storm objection does not apply. Resolves T4 findings
  2/4/9 and F12. Rationale for the reversal (stated in the doc): a cap overrun halts the
  maintenance plane — data-loss-class — so the bound must be **enforced, not observed**; ADR-0046
  already flagged the identical scan-then-commit shape as race-prone for `DeleteBucket`.
- **D-D — namespace cardinality (the structural rework).** This is where iteration 1 was
  central-holed. Concretely:
  - owned `pending:` entries get a **per-session index** `sidx:<upload-id>:<chunk-id>` (written
    by `intent` alongside the entry), so the reaper reclaims a session's owned entries via the
    **bounded** range `scan("sidx:<id>:")` — **there is no global `scan("pending:")` anywhere**
    (the F11a / T4-finding-7 hole);
  - an **in-flight part cap** `MAX_INFLIGHT_PARTS` bounds the owned-pending population per
    session to `MAX_INFLIGHT_PARTS × MAX_PART_CHUNKS`;
  - `retire:` is walked in **cursor-keyed bounded key ranges** (`B` per drain step), never one
    scan, so overwrite-driven growth cannot fail `ScanCapExceeded` (findings 3/6); a lagging
    drain raises an **oldest-obligation-age alarm** instead of crossing a cap;
  - the admission counter counts **all** session records incl. Completing/Aborting/Completed
    tombstones, whose retention is bounded by `W_tombstone` (finding 8);
  - I added the bounding-formula table in decision 6: the **only** global scan multipart adds is
    one bounded `scan("mpu:")`.
- **D-E — mechanical repairs.** `W_completing` measured from a stamped `fenced_at_millis`
  (finding 5, F16); `UploadPart` cumulative refusal restated as **best-effort** with the
  authoritative check at Complete on the frozen part set (finding 1, F17); the clock-lifecycle
  table now **owns honestly** that the reaper reads the owned-lease stamp as abandonment
  condition (i)'s liveness input (findings 10/11, F10 — replacing "nothing that decides
  reclamation"); the reaper **stale-snapshot rule** ("step-5 judgment no staler than the entry
  it condemns", with its DST observable) is added as fence-then-walk per-session reclamation
  (F15/X19), which makes the mode unconstructible.
- **D-F — honest arithmetic.** Computed real numbers in the accepted-costs register (below).
- **D-G — segmentation in scope.** Decision 7 (below), designed **with** decision 4's staged
  obligation machinery as one pattern family.
- **D-H — structure.** One document, 0016 expanded in place.

## Decision 7 (segmentation) — the design choice, and why this shape

- **Record shape (ADR-0046).** `InodeRecord.chunk_map` becomes `Flat(Vec<ChunkRef>)` (unchanged
  small-object shape) or `Segmented { owner, segment_count, segments }`, with the chunks living
  in `seg:<upload-id>:<index>` records. Key/writer/deleter/scan-visibility stated in §1.
- **Session-scoped segment keys (`seg:<upload-id>:…`), not `(inode,version)`-keyed.** This is the
  load-bearing design choice. It gives publish-retry idempotency *for free*: keys and content
  are fixed by the frozen part set, so a lost publication CAS retries against the same segment
  records — **no per-retry dangling generations**. `(inode,version)` keying would (a) collide
  when two completers race the same version, and (b) churn all segments on every publish retry.
  The cost: the winning generation's `seg:` records outlive the session (the inode stores
  `owner`), a small wart I state explicitly (2^-128 upload-id reuse, the X36 basis).
- **Staged publication = write segments (bounded batches), then O(1) root flip.** The flip is the
  publication instant, carrying decision 1's fence proof. This is the mirror image of decision
  4's staged *retirement* — one pattern family (D-G).
- **Crash story (F18/X37).** Dangling segments after a completer crash are protected (the parts
  still protect the fragments; the flip hasn't installed `retire:records:{parts}`) and evidenced
  (`publish_target` while `Completing`; `retire:records:{seg}` on rollback). Bounded (≤
  `MAX_ROOT_SEGMENTS`, one generation). Never a new unbounded/unevidenced namespace.
- **Uniform flat-or-segmented for multipart Complete AND single-PUT supersede.** One publication
  path; this also fixes the pre-existing single-PUT value-ceiling hazard (a 5 GiB PUT already
  exceeds `MAX_MAP_CHUNKS`).

## The arithmetic (computed, reproducible)

`V = 100 KB`, headroom `V/2 = 50 KB`, `b_ref = 131–302 B`, `SCAN_CAP = 1<<20 = 1,048,576`.

- **Flat ceiling:** `MAX_MAP_CHUNKS = ⌊50000/b_ref⌋ = 165–381 chunks` ⇒ **165–381 MiB** at
  1 MiB chunks; reaching 5 GiB flat needs **13–32 MiB** chunks; ~5–12 GiB even at large chunks.
  This is *below* the 5 GB single-PUT ceiling — the fact that killed iteration 1's motivation
  and forces segmentation into scope.
- **Segmented ceiling:** `MAX_ROOT_SEGMENTS = ⌊50000/b_segref⌋ = 312–520` (b_segref 96–160 B) ×
  `MAX_SEG_CHUNKS = 165–381` = 51,480–198,120 chunks ⇒ **50.3–193.5 GiB at 1 MiB chunks** (over
  the >10 GiB launch requirement with worst-case margin, at the default chunk); S3's **5 TiB**
  reachable only at **26.5–102 MiB** chunks (traded against `chunk_size × max_concurrent_encodes`).
  `MAX_PARTS_PER_SESSION = 10,000` is **not** binding in `[10 GiB, 5 TiB]` (5.1–524 MiB/part).
- **Concurrent-session capacity:** enforced exactly by the serialized counter; hard bound
  `MAX_SESSIONS ≤ SCAN_CAP ≈ 1,048,576` (the `mpu:` scan). Iteration 1's **global-part-scan**
  design forced `≤ SCAN_CAP / 10000 ≈ 104`; the per-session-indexed rework (D-D) removes that
  coupling, leaving `MAX_SESSIONS` a policy knob. This number is the point of D-F: iteration 1's
  "well under S3's 5 TiB" hid a ~165–390 MiB object reality and a ~100-session capacity.

Verified: `python3` computation reproduced in the run log; the numbers in the accepted-costs
register match.

## This is an "invariant to restore", so the axis is smallest-change-that-restores, not smallest-diff

The brief names an Invariant to restore (the four assembled-write safety contracts holding *by
design*). Per `docs/principles.md` §1.2/§2 the target is the smallest change that restores the
invariant across every maintenance consumer — not the smallest diff. Two places where I chose the
*more* invasive design deliberately, with the cost shown, because the cheaper one leaves the
invariant only *guarded*, not *restored*:

- **Serialized admission counter (D-C) over scan-then-create.** The cheaper "read the population,
  then create" guards the symptom (usually the cap holds) but does not restore the bound: N
  concurrent creates each read sub-cap and all commit — unbounded overshoot, and a cap overrun
  halts the custodian plane (`ScanCapExceeded`, no partial result). Cost of the enforced fix:
  one hot key on the Create path only (rare vs. part commits). I show that cost in the
  Alternatives section rather than calling it "heavier".
- **Segmentation now (decision 7) over deferring it (iteration 1's FU-1).** Deferring is the
  smaller diff, but it ships a feature that provably cannot meet its own >10 GiB requirement (the
  arithmetic). The cost of doing it now — the `InodeRecord.chunk_map` shape changes for the ~19
  `.chunk_map` read sites — is real and is why I recommend it for ADR graduation (FU-1), but a
  deferral would be a silent rescope of the launch requirement, which the brief forbids.

## Alternatives ruled out, with the cost shown (in the document's Alternatives section)

Scan-then-create admission (unbounded overshoot; race-prone per ADR-0046); a per-part mutation
counter (turns 4–16 concurrent parts into a retry storm on one hot key); a global
`scan("pending:")` backstop (owned cardinality is per-chunk of in-flight parts, crosses
`SCAN_CAP`); per-part exact-value preconditions at Complete (10,000 part values > envelope);
staging inside `pending:` (ADR-0046 synthesized-encoding rejection + #557 hazard); `W_open`-only
session bounding (unbounded live-session residency, F14); resumable uploads across a restore
(a records-only image cannot prove the bytes exist); deferring segmentation (below the launch
requirement); unscrubbed staged bytes (durability is not an allowed accepted-cost class).

## Verification — the three refutation questions

This is a **docs-only design artifact**; the brief states `Test file: none` and I ship no test.
A design document has no honest headless regression test that drives *production* code, because
this diff contains no production code — fabricating a stand-in that "passes" would be exactly the
hollow evidence the process warns against. What is actually exercised:

**(a) Genuine red?** *Leg A1 (mechanical):* **yes, demonstrated on this reworked file.** I
injected a dangling relative link (`[a46]` → a nonexistent ADR) and ran the real gate
(`./engine/xtask.sh ci` → `cargo xtask ci` in `$PDCA_WORKTREE`): the render audit went **red**,
`render_site: ERROR dangling link in output: proposals/draft/0016-…html -> ../../adr/0046-DANGLING-XX.md`,
gate EXIT=1; restoring the real link made it green (`render_site: link audit OK`, `xtask ci: all
checks passed`, EXIT=0). typos-cli 1.48.0 and the renderer imports are live on this host, so the
prose gates execute rather than warn-skip. *Leg B (judgment):* "red" is criterion-absence under
the Refutation standard, judged by the Check reviewer/adversary/human — and it **already fired**
in iteration 1 of this bundle (T4: 11 blocking findings; reviewer C5/T2/T4 FAIL; seven adversary
refutations, `iteration-v1/check-*.md`). I did not manufacture a test to simulate that judgment.

**(b) Production path?** Yes. The gate ran against the actual worktree files that constitute the
deliverable (the real 0016 and the real index row), rendered by the real `render_site.py --check`
across all 98 pages — no copy, stand-in, or fixture in between. The artifact *is* the unit under
test for a design document.

**(c) Fixture includes the fault?** Yes. The injected fault (the dangling link) was on the real
file that ships, not a scratch copy, and the failing element (the link the audit resolves) is one
the patch adds; the green run renders all 98 pages including the new one. Nothing was curated out.

## Environment / dependencies

Both external dependencies the brief registered were present and **executed**: `typos`
(typos-cli 1.48.0) and `docs-renderer` (`markdown_it` + `yaml` imported; the render-and-link
audit ran on 98 pages and failed on the injected dangling link — proof it did not warn-skip). No
undeclared dependency was needed: no Docker, no cluster, no backend. **No NEEDS-HUMAN external
dependency to declare.** The full `cargo xtask ci` (prose gates + fmt/clippy/build/test/deny/
conformance) exited 0 on the patched worktree.

## What remains for the human at sign-off

- The proposal ships at `status: draft` by design; ratification (draft → accepted,
  architecture-board / founding-maintainer authority under ADR-0037) is explicitly not this
  cycle's act. Leg B (settlement adequacy) is the judgment the brief assigns to the reviewer, the
  adversary, and you; the place to attack it is the execution register (X1–X39) plus the seven
  per-decision failure-mode tables and the F1–F18 disposition list.
- Every F is disposed as **eliminated** or a **bounded non-safety cost**; **none** is flagged
  NEEDS-HUMAN, because the design honours all eight directions and every F. If you disagree with a
  disposition, that is the sign-off lever.
- `C4-verify` and `C5-mutants` are vacuous on a no-code diff and carry no evidence either way
  (recorded correctly in iteration 1's gates).
- Scratch: the only files I created outside the bundle and the worktree are gate logs and one
  backup under `$PDCA_SCRATCH` (`pdca-builder-626-*`), removed at the end of the run; the
  apply-check worktree was `git worktree remove`d.
