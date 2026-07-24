# Build notes — issue 626, iteration 4 (rework of 0016)

**Deliverable:** a design document, not code. Reworked
`docs/design/proposals/draft/0016-multipart-commit-protocol.md` (started from
`iteration-v3/patch.diff` applied to `$PDCA_WORKTREE`, the latest state) + its index row in
`docs/design/proposals/README.md`. Two docs paths, nothing else (leg A confirmed:
`git diff --stat` = README +1 line, 0016 new file).

I built on **iteration-v3** (not the v1 the brief header names): v3 is the accumulated state and
the iteration-3 carry-forward is the operative fix list. The iteration-2 carry-forwards are
confirmed closed by the human — I did **not** reopen them (byte-derived batch `B`, epoch-scoped
`seg:` keys, residue-counted `sinf:`, 409-not-resume, terminal-delete exact-bytes precondition,
work-coupled `MAX_SESSIONS`). Every load-bearing part the iteration-1 adversary could not refute
is preserved (fence/epoch machine, O(1) session-precondition publication proof, restore fence,
GC reference-before-orphan precedence, the `PendingEntry.owner` serialization-identity technique,
D-F arithmetic).

## The six iteration-3 carry-forward findings and how each is closed

Every finding was first **grounded in the production code it binds against** before I designed the
fix (Read of `write.rs:198-214` intent, `restore.rs:417-429` `pending_chunks`, `gc.rs:60-260`
reference build + Defer conservative arm, `desired_state.rs:150-179` `genuinely_holds`,
`metadata.rs:488-534` `unlink`). All six were real.

**F1 — teardown/fence race (part-intent commits carry no session precondition; terminal delete
does not require empty owned state).** Fix: the **serialization edge at creation**, not a patch at
teardown — every owned-`sidx:` intent put (and the `sinf:` slot reserve) carries
`require(mpu == Open@E)`. So once the session leaves `Open`, **no owned entry can be created**; the
reaper's single `sidx:` walk (fence-then-walk) sees a frozen, complete set and nothing refills it.
This is the reviewer's *second* offered remedy ("intents precondition on session state"). Touches
§1 sidx row, §3 Part-intent/Part-slot-reserve rows, decision 5.1, X43, the reaper stale-snapshot
rule, and the flagged NEEDS-HUMAN question (below).

**F2 — Completed-path residue stranded.** The `Completed` teardown never walked `sidx:`, so a
crashed in-flight part's residue (owned entry, no `part:` record) was stranded forever — outcome
(a). Fix: the reaper walks `sidx:` for **both** `Aborting` **and** `Completed` before the terminal
delete, which now gates on the `sidx:` range **observed empty in-pass**. §2 exit table, reaper
"what it may exit", reaper algorithm, decision 5.3, X44, a new reaper failure-mode row.

**F3 — scan-cap contradiction (`reconcile_after_restore` does a global `scan("pending:")`; fleet
owned ≈ 52× SCAN_CAP).** This is the structural change of the iteration. Owned entries were
`pending:` entries; the restore/sweep global `pending:` scan therefore enumerated them and would
`ScanCapExceeded` at fleet scale (halt). Fix: owned entries become a **record class disjoint from
`pending:`** — `sidx:<upload-id>:<part-number>:<chunk-id>` carrying the `PendingEntry` (with
`owner`, so the serialization-identity technique the brief calls load-bearing is *preserved*, and
ordinary `pending:` entries — `owner=None` — round-trip byte-identically, unchanged). No global
scan of owned entries exists anywhere; the restore/sweep `pending:` scan is re-derived to the
ordinary (non-multipart) population, exactly today's bound. I added a `MAX_OWNED_FLEET` knob row
(fleet owned, `W_ref`-coupled not `SCAN_CAP`-coupled). §1 table + owner paragraph, decision 2
restore row, decision 5.1, knob table, namespace table, X25/X27, backward-compat, alternatives.

*Why the disjoint prefix and not the reviewer's other phrasing ("bound fleet-wide owned in the
knob table").* Keeping owned under `pending:` forces `MAX_SESSIONS × MAX_INFLIGHT_PARTS ×
MAX_PART_CHUNKS ≤ SCAN_CAP`. The per-session product is capped at `SCAN_CAP/2 = 524,288`, so even
`MAX_SESSIONS = 2` already reaches `SCAN_CAP`; at a 5 GiB part (`MAX_PART_CHUNKS = 5,120`,
`MAX_INFLIGHT_PARTS = 16`) the per-session footprint is 81,920 and `MAX_SESSIONS ≤ 6`. That
collapses concurrency to near-single-session. The doc's own X25 already *claimed* "no global
`pending:` scan"; the finding proved the shared prefix falsified it. The disjoint prefix is the
smallest change that **restores the invariant** ("every namespace is bounded by an admission
formula or accessed only through bounded key ranges") and makes the existing claim true. I still
added the fleet-wide constraint the reviewer asked for (`MAX_OWNED_FLEET`) so the number is stated
honestly.

**F4 — drain invisibility of in-flight owned fragments.** A still-streaming part's fragments (no
`part:` record) were invisible to drain/desired-state → operator wipes → later Complete publishes
over wiped bytes, outcome (c). Fix: the staged reference set includes in-flight owned (`sidx:`)
fragments as well as committed `part:` fragments; `genuinely_holds` counts the union; both
committed and in-flight placement exclude draining servers. I scoped the *verification* honestly:
`protects()`/restore/drain count in-flight owned, but scrub/reconstruction act only on the
scheme-bearing committed-`part:` subset (an in-flight chunk carries no committed EC scheme yet).
Decision 2 staged-set paragraph, GC/Drain rows, drain-stall paragraph, two failure-mode rows, X14.

**F5 — DeleteObject of a segmented object over the envelope.** `unlink` orphan-marks every
fragment inline in one batch (`metadata.rs:514-531`); a max segmented object is 463 K–1.78 M
fragment orphans — over `E_tx`, outcome (d). Fix: `DeleteObject`/`DeleteObjects` (#509) route
through the retirement ledger like supersede — one `retire:bytes:{generation}`, drained in
`B`-batches. Decision 4 rule 1 (now a two-bullet list), §3 batch inventory (new "Object delete"
row), decision-4 failure-mode row, F4 disposition, X45, public-API + accepted-cost rows, `[i509]`.

**F6 — two smaller fixes.** (a) The D-C tension (part commits are not counter-free: `sinf:` CAS at
every part boundary serializes same-session part starts, and now every `sidx:` intent reads the
session record) is **surfaced as an explicit flagged NEEDS-HUMAN sign-off question** ("Flagged for
sign-off" section), not silently resolved. (b) The reaper's clock guard is **derived from the
session record's `clock_source`, read first** — the reaper skips a foreign-clocked session before
evaluating any owned lease, so owned entries need no `clock_source` field. Clock-lifecycle table
row, reaper clock bullet, F10 disposition, X46.

## A hole I caught in my own first pass, and corrected (self-refutation)

My first draft implemented finding 1/2's teardown side with `require(sinf == 0)` on the terminal
delete + per-part slot release during reclamation. **That deadlocks**: a part can reserve a slot
(`sinf +1`) and then be fenced *before writing its first `sidx:` intent* — a slot with no `sidx:`
entry, which the reaper's `sidx:` walk can never see, so `sinf` never reaches 0 and the terminal
delete leaks the session + counter forever (a slow `MAX_SESSIONS` over-count). I removed it. The
correct, hole-free gate is **`sidx:` observed empty in-pass** (which the *fenced intent* keeps
true — nothing can refill it); `sinf` is deleted outright with the session because its only job is
in-flight-cap enforcement *while `Open`*, and a reserved-but-unwritten slot leaves no residue
(intent precedes any fragment). This is why the fenced-intent edge (finding 1's primary remedy),
not `sinf == 0`, is load-bearing. Corrected across §1/§2/§3/decision 5/decision 6/X43/X44/register.

## Verification — a design doc, no regression test (brief "Test file: none")

Leg A1 (mechanical validity) is the gating whole-tree prose gate; I ran the three legs exactly as
`cargo xtask ci`'s `docs-check` invokes them, in `$PDCA_WORKTREE`:

- `typos` (typos-cli 1.48.0) → exit 0
- `python3 docs/publishing/tools/lint_docs.py` → `lint_docs: OK`
- `python3 docs/publishing/tools/render_site.py --check` → `link audit OK` (98 pages; the new
  0016 and its README index-row relative link resolve)

I invoked these three directly rather than the whole `cargo xtask ci` (fmt/clippy/build/test over
every crate) because the change is docs-only: these are the load-bearing legs, they are fast and
non-hanging, and they are the exact commands the gate's `docs_check()` runs (`xtask/src/main.rs:1808`,
`:1829-1887`). The full gate re-runs at Check.

**The three refutation questions, adapted (there is no code test to revert):**

- (a) *Genuine red?* There is no prior failing assertion to flip — "red" for this deliverable is
  criterion-absence under the Refutation standard, judged at Check, and it **has fired**: iterations
  1–3 of this bundle went red exactly here (T4 blocking findings + adversary refutations). This
  iteration's job is to make the previously-red iteration-3 findings green; each is closed by
  construction with a named observable a #508 Plan can lift.
- (b) *Production path?* Every fix binds against the real contracts, re-cited `path:line` in the
  proposal (verified by Read this cycle: `write.rs:198-214`, `restore.rs:417-429`,
  `gc.rs:228-260`, `desired_state.rs:150-179`, `metadata.rs:488-534`, `traits/src/lib.rs:744-758`).
  The document is the production artifact; there is no stand-in.
- (c) *Fixture includes the fault?* Each finding's failure-mode/execution row names the concrete
  fault it disposes (the fenced-out-then-refill race X43, the Completed residue X44, the
  `≈52×SCAN_CAP` restore scan X25, the drain-wipe of in-flight owned X14, the segmented-delete
  fan-out X45, the foreign-clock owned lease X46) — the fault is *in* the enumeration, not curated
  out.

No honest headless code test exists for a design document, and I did **not** fabricate one (a
green test driving a stand-in would be worse than none). The mechanical leg is the prose gates
above; content adequacy (legs A2/B) is the brief-aware Check reviewer + adversary under the
Refutation standard, with the human deciding — the standard posture for this bundle.

## NEEDS-HUMAN items

- **By-design (INTEGRATION §4):** a proposal change is a NEEDS-HUMAN §6 row; the human decides the
  settlement at sign-off.
- **In-document flagged sign-off question (finding 6a):** the *cost* of the enforced in-flight cap
  — a per-session part-boundary `sinf:` CAS plus a per-chunk `sidx:` intent session read — is
  flagged in the "Flagged for sign-off (NEEDS-HUMAN)" section for the maintainer to rule on. It is
  a cost question, not a direction left unhonoured (the design is correct as written; every lighter
  alternative I could see weakens the F11a bound to *observed* — the iteration-2 rejection class —
  or reintroduces a global scan, the finding-3 hole).

No missing external dependency: `typos` and `docs-renderer` are both registered doctor rows and
both installed on this host (typos-cli 1.48.0; `import markdown_it, yaml` OK).

## Scratch

All throwaway render output under `${PDCA_SCRATCH:-…}/pdca-builder-626-docs*` and removed.
