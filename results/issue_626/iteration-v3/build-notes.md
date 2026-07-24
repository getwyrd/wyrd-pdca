# Build notes — issue 626, iteration 3 (rework of iteration-2 draft 0016)

## What this cycle is

A **re-plan re-do**: the brief header is iteration 2, but the `## Iteration 2 —
carry-forward` block records that the iteration-2 artifact was REJECTED at sign-off (T4:
7 blocking; reviewer C3/C5/T2/T4 FAIL; adversary refutations) and lists **six** specific,
fixable defects. Per the carry-forward ("Fix in the same document"), I started from the
most recent attempt — `iteration-v2/patch.diff` — applied it to `$PDCA_WORKTREE`
(`git apply`, clean), and reworked the six defects **in place**. I did **not** restart from
`iteration-v1` (that would discard the load-bearing core the iteration-2 adversary could not
refute: the fence/epoch machine, the O(1) session-precondition publication proof, the
restore fence, the D-F arithmetic — all preserved).

Deliverable: two docs paths only —
`docs/design/proposals/draft/0016-multipart-commit-protocol.md` (reworked, 1441 lines) and
its already-present index row in `docs/design/proposals/README.md:32`. No `crates/`,
`xtask/`, or test changes (out of scope; docs-only per brief).

## The six carry-forward defects and how each is closed

### 1. Envelope defect — count-based `B` blows the transaction envelope
**Defect:** the segment-write and drain rows claimed "`B` inside the envelope" while `B` was
a fixed count `MARK_BATCH = 1_000` (`crates/custodian/src/restore.rs:100`). That count is
safe only for *small* mutations: a `restore.rs` orphan mark is a key + a few-byte value. But
a `seg:` write is a value up to `V = 100 KB` (`crates/traits/src/lib.rs:750`), so
`1_000 × 100 KB = 100 MB` — **10× the 10 MB transaction ceiling** (`traits/src/lib.rs:750`),
a permanent commit failure.
**Fix:** redefined `B` as a **byte-derived** count. Introduced `E_tx = 10 MB` (the
transaction ceiling, named distinctly from the fence epoch `E` to avoid the `@E` collision),
budgeted every drain/segment batch at `≤ E_tx/2 = 5 MB`, so the *count* is
`B = ⌊(E_tx/2)/(bytes per mutation)⌋` — ~1,000 for small orphan marks (the `MARK_BATCH`
precedent still holds for *those*) but `⌊5 MB / V⌋ = 50` for `seg:` writes. Restated the
batch-inventory rows, the knob table, decision 4 rule 1, decision 7(b), and the reaper
algorithm in bytes; added a failure-mode observable that asserts commit **bytes** ≤ `E_tx/2`,
not count.

### 2. F18 refuted — rollback → re-Complete while the prior `retire:records:{seg}` is pending
**Defect (the sharpest of the six, outcome c):** iteration-2 keyed segments by upload-id
only and claimed (0016 old §7c) "segment keys are upload-id-scoped, not per-attempt, so
publish retries reuse them". The refutation: roll a Completing attempt back (installing
`retire:records:{seg}` over `seg:<id>:*`), then re-Complete and **publish** while that
obligation is still pending; the drain later deletes `seg:<id>:*` — which are now the
**published** object's segment records → a published object loses its chunk map.
**Fix — attempt/epoch-scoped segment keys** (the brief's second sanctioned option). Segment
keys become `seg:<upload-id>:<epoch>:<index>`, the segment-group id is `(upload-id, fence
epoch)`, and the `Segmented` root records it. A rolled-back attempt at epoch `E` installs
`retire:records:{seg:<id>:<E>}` naming **only** epoch `E`'s keys; a later attempt fences to a
**new** epoch `E'` and writes the **disjoint** range `seg:<id>:<E'>:*`. The stale obligation
can now drain concurrently with or after the new publication and can never touch the
published epoch-`E'` segments. Added execution **X40** (the exact refuting trace) and a
decision-7 failure-mode row; updated the F18 disposition to name epoch-scoping + X40.

**Why option (b) over option (a) ("re-fence cancels the pending seg obligation"):** option
(a) is a smaller textual change but has a residual leak I could not close cheaply — if the
part set changed across the rollback, the re-Complete produces *fewer* segments (say `M' <
M`), so cancelling the obligation leaves `seg:<id>:M'..M` from the prior attempt dangling and
unevidenced; closing *that* requires the flip to know the prior attempt's max index (an extra
recorded field) — i.e. it grows toward the same per-attempt bookkeeping epoch-scoping gives
for free. Option (b) closes the hole **by construction** (disjoint key ranges), which matters
because the F18 row was rejected precisely for a *wrongly-accepted* "Eliminated" — a
by-construction argument is what a confirmatory pass cannot wave through. Cost of (b): the
`seg:` key gains an epoch component and the root records it; ~10 localized edits to the seg
machinery (§1 row, batch inventory, decision 7 a–f, X37/X40, F18 row). That is the smaller
*risk*, not the smaller diff — and for an invariant-restoring change the brief's axis is
smallest change that restores the invariant, not smallest diff.

### 3. F11a refuted — unbounded owned residue while a session stays Open
**Defect:** `MAX_INFLIGHT_PARTS` bounded only *live* parts. A part upload that **crashes
mid-stream** leaves owned `pending:`/`sidx:` residue that is reclaimed only when the session
leaves Open — so while Open, a crash-loop accumulates residue unboundedly; past `SCAN_CAP`
the per-session `scan("sidx:<id>:")` teardown itself fails and the backstop reclaims nothing
(outcome a). The carry-forward named this "observed-not-enforced".
**Fix — enforced, residue-counting in-flight cap.** Added a per-session slot counter
`sinf:<upload-id>` (the residue-counting analogue of the fleet `mpuctl:count`, one level
down). `UploadPart` CASes it `+1` **before streaming any chunk** and **refuses `503` at
`MAX_INFLIGHT_PARTS`** (the enforcement point); the slot releases (`-1`) only on commit or
compensation. A crashed part **never releases its slot**, so residue is *counted against the
cap*. Bounding invariant `MAX_INFLIGHT_PARTS × MAX_PART_CHUNKS ≤ SCAN_CAP/2` (knob table)
keeps the per-session `sidx:` range below `SCAN_CAP` by construction, so the teardown scan
can never fail. A session wedged at the cap by residue is a bounded availability cost (client
aborts to recover), registered. Updated decision 5, decision 6 formula, knob table, F11 row,
X25/X41, and a failure-mode row.

**Why enforce rather than "give sidx: the cursor-keyed walk retire: has" (the third
sanctioned option):** I read the store trait — `scan(&self, prefix)` returns the **complete**
set or fails loud, **order unspecified** (`crates/traits/src/lib.rs:776`, cap at
`:286`/`ScanCapExceeded`). There is **no** cursor/limit/start-after parameter, so genuine
lexicographic pagination of `sidx:` is not available from the API, and "cursor-keyed walk"
would be an unimplementable claim (the D-D wording is "sharded **or** cursor-keyed" for
exactly this reason). Sharding `sidx:` would only raise the cliff, not enforce a bound — still
"observed-not-enforced" in spirit. So enforcement (a per-session cap that counts residue) is
the honest closure. I kept the per-session counter *per part boundary* (≤16 concurrent, S3
default), never per chunk and never on the session record — so decision 1's "part commits
don't conflict on the session record" and D-C's fleet-counter-free chunk path both hold; this
is exactly D-D's mandated in-flight cap, distinct from D-C's fleet counter.

### 4. Internal contradiction — Complete under Completing is both 409 and "resume"
**Defect:** the verb×state table said `409 OperationAborted` while decision 3 (F5) and
decision 7(c) read as "resume".
**Fix — 409 wins (the carry-forward's steer: "if 409 wins, carry the W_completing recovery
latency").** A client `CompleteMultipartUpload` arriving while `Completing` returns `409` for
**every** client verb (including a timeout-retry). The only "resume" is the *internal*
recovery of the one gateway that already owns the fence re-reading its own
`CommitUnknownResult` (same epoch, idempotent); a **crashed** completer is recovered by the
reaper's rollback to `Open` after `W_completing`, not by any client verb. Removed "a second
gateway picks it up"; reframed decision 3 F5, decision 7(c)/(d), added a verb-table note, and
registered the `W_completing` recovery-latency accepted cost. This also makes the F18
epoch story cleaner: there is provably never a second publisher writing segments in parallel.

### 5. Terminal-delete double-decrement
**Defect:** `delete mpu:<id> + CAS mpuctl:count -1` had no precondition on the session record,
so a gateway inline drain and the reaper could each read-modify-write the counter for the
**same** session deletion (the `delete` is idempotent; each retries its `-1` until it lands),
drifting the counter low and breaking F12's *exact* bound — which, since the counter enforces
`MAX_SESSIONS < SCAN_CAP`, is a maintenance-plane-halt hazard.
**Fix:** the terminal batch now `require`s the session record's **exact prior bytes**
(`require(mpu:<id> == prior)`), so exactly one drainer wins the delete-and-decrement and the
other's whole batch is a no-op — an exactly-once decrement. Updated §2 prose, the records
table (`mpuctl:count`, `mpu:`), a new "Terminal session delete" batch-inventory row, the
reaper algorithm, the F12 row, and added execution X42.

### 6. Overstated MAX_SESSIONS range (~10^10 reads/pass)
**Defect:** the register put `MAX_SESSIONS ≤ SCAN_CAP ≈ 1.05M`, claiming the per-session
index "lifts" iteration-1's coupling. But the staged reference-set build reads, per reconcile
pass, `MAX_SESSIONS × MAX_PARTS_PER_SESSION` part records — at top of range that is
`~1.05M × 10_000 ≈ 10^10` reads/pass, which no pass can do.
**Fix:** made `MAX_SESSIONS` **work/memory-coupled**. Introduced a per-pass staged-reference
budget `W_ref` with `MAX_SESSIONS × MAX_PARTS_PER_SESSION ≤ W_ref ≤ SCAN_CAP`. At
`MAX_PARTS_PER_SESSION = 10_000` and `W_ref = SCAN_CAP`, `MAX_SESSIONS ≈ 104` — the honest
ceiling is `~10^2`, **not** `~10^6`. Corrected decision 2, decision 6 (new paragraph), the
knob table (added `W_ref`), and the accepted-costs "Concurrent-session capacity" row, stating
plainly that the per-session-index rework removed the single-*scan* halt but **not** the
aggregate-*work* coupling. Re-checked the F18/F11 disposition rows after the fixes (they were
the rows a confirmatory pass would wrongly accept) — both rewritten with the new mechanisms
and observables.

## Naming collision fixed proactively

Introducing an envelope symbol `E` would have collided with the fence-epoch `E` (`@E`,
`Completing@E`) used throughout. Renamed the transaction ceiling to `E_tx` everywhere and
noted the distinction at first use — otherwise a reviewer reading `E/2` next to `Completing@E`
would (rightly) flag ambiguity.

## Grounding re-verified on the worktree base (cd82a29)

- transaction envelope 100 KB value / 10 MB / 5 s: `crates/traits/src/lib.rs:744-758` (read).
- `scan(prefix)` is complete-or-fail-loud, order unspecified, **no cursor param**:
  `crates/traits/src/lib.rs:776`; `SCAN_CAP = 1<<20` `:286`; `ScanCapExceeded` `:288-298`.
- `MARK_BATCH = 1_000` and its FoundationDB-cap rationale: `crates/custodian/src/restore.rs:100`.
- review rubric (self-reviewed against): `AGENTS.md:122-210` — serialization identity
  (`:170-174`, the `PendingEntry.owner` round-trip is preserved), one-clock
  (`:132-142`, clock table unchanged), transactions/`CommitUnknownResult` outranks `Conflict`
  (`:178-180`, decision 5 honours it), "no count-based assertion that can pass while the
  property fails" (`:175-177` — my new observables assert bytes/exactly-once/survival, not
  counts alone).

## Verification — honest posture

Per the brief, **no test file** ships: this is a design artifact; no regression test exists
or is possible for a proposal (`Test file: none`). The mechanical gate IS the whole-tree
prose gate.

- **Leg A1 (mechanical, gating `C4-ci`):** I ran the exact three commands
  `cargo xtask ci`'s `docs_check()`/`typos_check()` invoke, against the patched
  `$PDCA_WORKTREE`: `typos` (exit 0), `python3 docs/publishing/tools/lint_docs.py`
  (`lint_docs: OK`), `python3 docs/publishing/tools/render_site.py --check`
  (`link audit OK`, 98 pages). These are the project's own prose-gate tools (typos-cli
  1.48.0; `markdown_it`/`yaml` import OK), not a hand-rolled runner. Green.
- **Leg A2 (inspected):** exactly two docs paths change (`git apply --stat`); frontmatter is
  `type: proposal` / `status: draft` / `author: Eduard Ralph` / `tracking-issue: "#626"` with
  the required tags; index row present at README.md:32 mirroring 0014's draft-row shape; no
  model/tool attribution anywhere.
- **Leg B (judgment tier):** criterion-absence under the Refutation standard — judged by the
  Check reviewer + adversary + the human, not mechanically provable here. This is the
  by-design NEEDS-HUMAN §6 row (INTEGRATION §4).

### Forced self-refutation (adapted to a no-test docs deliverable)
- **(a) Genuine red?** The mechanical leg (A1) would go red on a dangling relative link, a
  typo, or a missing render — the render audit checks the index→file link. Leg B has *already
  gone red twice on exactly this predicate* (iterations 1 and 2), so the gate is demonstrably
  binding; there is no fix to "revert" for a docs artifact, but the six fixes each turn a
  named refutation (a T4/adversary finding) into a closed execution-register row with a stated
  observable.
- **(b) Production path?** The deliverable *is* the production artifact; the gates ran against
  the real patched tree, not a copy.
- **(c) Fixture includes the fault?** Each of the six refutations is now *present* in the
  document as a named failing execution (X40 rollback-while-pending; X41 crash-loop residue;
  X42 concurrent-drainer decrement; the byte-budget/`seg:` overflow observable; the 409
  clarification; the ~10^2 capacity number) with the observable that would catch a regression
  — the analogue of "the fixture includes the killed node".

Nothing needs a NEEDS-HUMAN external-dependency marker: no build tool, service, or topology
was missing (`typos`, the doc renderer both installed; no cluster/Docker/backend needed for a
document).

## Commit-readiness

The target's commit hooks for a docs-only change are the prose gates (`typos` + `lint_docs` +
`render_site --check`), all run green above. There is no markdown formatter in the target's
hook set (`fmt`/`clippy` touch no file here). The patch applies cleanly to a pristine
`cd82a29` worktree (verified via `git stash -u` + `git apply --check`).
