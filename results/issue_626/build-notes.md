# Build notes — issue 626 / multipart-commit-protocol (iteration 7)

**Deliverable:** a design document, not code. Reworked draft proposal
`docs/design/proposals/draft/0016-multipart-commit-protocol.md` + its index row in
`docs/design/proposals/README.md`. Patch is two docs paths, nothing else.

## Starting point and method

Per the carry-forward's instruction ("Rework the document — do NOT restart") I started from
the **iteration-6** full document (`iteration-v6/patch.diff`, applied clean to
`$PDCA_WORKTREE` at `cd82a29`), not from iteration-1. Iteration 6 already carries every
accumulated fix that survived rounds 1–5. This iteration is a **surgical** pass over the
**four** T4 findings the iteration-6 gate went red on (`review-batch.md`: 4 blocking, 0
recorded-rejected) plus the one non-scored **ADJUDICATE** note. Nothing the iteration-6
carry-forward told me to PRESERVE was touched:

- fence/epoch state machine (§2)
- restore fence-then-serve, X17/X17b/F13
- per-attempt epoch-scoped `seg:` keys, X37/X40/F18
- committed-object repoint-vs-supersede armor, X47
- exactly-once terminal decrement + counter-only-collision handling, X42/X52
- segmented-GET resolve-retry rule, X51
- byte-budgeted batch inventory (§3)
- `retire:` / reference-build bounded-cost dispositions, X39/X48
- the ⚑ NEEDS-HUMAN serialization-cost Open question (kept flagged, unruled — the human's
  to adjudicate; `0016:968-969`, `0016:1855`, Open questions)

All four findings were **fixed** (each leaves the next T4 run), so **no `review-rejected.md`
entries are needed** — the triage rule is "fixed OR recorded-rejected"; I chose fix for all
four, including finding 3 where record-reject was the offered alternative.

## The four T4 findings — what changed and why

### Finding 1 — staged-part reconstruction re-place strands the rebuilt fragment on a lost CAS (BUG, outcome (a))

The iteration-6 text let a **committed segmented** repoint (X47) pre-mark its destination
before writing, but the **staged** re-place path (reconstruction of an in-flight owned
chunk, Decision 2's Reconstruction row) wrote the replacement fragment to `P_new` *before*
CASing the `part:` record with only `require(mpu == Open@E)`. A Complete/Abort/reaper fence
landing in the write→CAS window fails the CAS and leaves `P_new` referenced by nothing and
evidenced by nothing — permanent under the deployed `Defer` GC policy (`gc.rs:78-105`).

Fix — extend the X47 destination-pre-mark rule to the staged path (the *symmetric* structural
repair, not a symptom guard):
- Reconstruction row rewritten to pre-mark `orphan:<P_new>` before the fragment write, then
  CAS `require(mpu == Open@E)` **and** `require(part:<id>:<n> == prior)`; **win** adopts
  `P_new` and orphans `P_old`, **loss** is a no-op leaving the `P_new` pre-mark for GC
  (`0016:524`).
- New batch-inventory row "Staged part re-place" mirroring the "Segment repoint" row
  (`0016:400`).
- Execution register **X29 rewritten** — it previously said "the re-place CAS fails; the
  obligation stays queued" and *mischaracterized the pre-written fragment as safe*; now states
  the pre-mark closes outcome (a) (`0016:1505`).
- Decision-2 failure-mode observable strengthened to interleave the fence *after* the
  destination write but *before* the CAS and assert the `orphan:<P_new>` pre-mark is present
  (`0016:557`-region).
- Accepted-cost row extended to cover the staged-re-place lost-CAS pre-mark reclamation
  (`0016:1713`-region).

The pre-mark reuses machinery that already exists in the doc (X47) and in the tree (orphan
records are placement-keyed, `metadata.rs:60-70`), so it introduces no new namespace — one
pattern family, per §9's direction.

### Finding 2 — `mpuctl:count` has no bootstrap (BUG)

Every Create CAS requires `require(mpuctl:count == c)`, but on a fresh/upgraded store the
singleton record is absent, so the *first* Create can never satisfy it — multipart is dead on
a new store. Fix — **absent-reads-as-0**, no migration step:
- Records-table row: first Create initializes with `require_absent(mpuctl:count)` + `put = 1`
  (`0016:217`).
- Create-session batch-inventory row split into present-branch (CAS) / absent-branch
  (`require_absent` + `put = 1`) (`0016:388`).
- Decision-6 admission bullet: absent-as-0 read, self-bootstrapping, race-safe by the same CAS
  discipline (`0016:1037`-region, now ~`:1060`).
- New execution register **X53** (`0016:1529`).
- F12 disposition observable gains the first-create-on-fresh-store assertion (`0016:1653`
  -region).

I chose absent-as-0 over a one-time init batch because it needs **no separate migration/init
transaction** and self-heals on upgrade; the concurrent-first-create race is closed by the
same `require_absent`/CAS discipline the steady state uses.

### Finding 3 — `G_orphan == W_write` boundary race (BUG)

GC's grace check is inclusive (`now − orphaned_at ≥ G_orphan`, `gc.rs:171-176`), so the
boundary `G_orphan == W_write` lets GC reclaim a straggler's `orphan:` evidence in the exact
tick the fragment authorized at its deadline lands. Fix — tighten to the **strict**
`G_orphan > W_write + δ_clock`, where `δ_clock` bounds the wall clock's resolution and any
skew between the two evaluation sites (write-path deadline check vs GC grace check). Changed
all six occurrences so the doc is internally consistent:
`0016:797` (knob table), `:875` (Decision 5 definition + reasoning), `:986` (Decision 5
failure observable), `:1455`/`:1456` (clock-lifecycle table, both rows), `:1525` (X49). The
one-clock rule is respected — both stamps stay on the single deployment wall clock
(`AGENTS.md:132-142`); the margin is a resolution/skew allowance, not a second source.

I fixed rather than record-rejected: the strict inequality is a one-token correctness repair
with no downside (a slightly longer grace is already an accepted cost), and the adversary's
`t_mark > t_authorize` argument the carry-forward offered as a rejection basis is *weaker*
than just making the bound strict.

### Finding 4 — summary line wrongly credits renewal refusal with the late-write bound (CONVENTION)

The "What the implementing slices change" bullet claimed the renewal loop's
refuse-rather-than-resurrect bounds a late fragment write "to ≤ half a lease TTL". That
contradicts Decision 5, which is explicit that renewal refusal does **not** cancel an
in-flight write — the bound is the fail-closed `W_write` await (the *await discipline* MUST,
`AGENTS.md:181-183`). Rewrote the bullet so renewal refusal is a *supporting* property and
`W_write` (coupled to `G_orphan` by the strict inequality) is the bound (`0016:1567`-region).
This aligns the summary with Decision 5's careful text (`0016:851-875`) and with the
rubric's await-discipline rule.

## ADJUDICATE (non-scored) — bulk `DeleteObjects` obligation-installation

Stated **normatively**: a single `DeleteObject` installs one O(1) obligation, but a bulk
`DeleteObjects` (up to 1,000 keys) installing 1,000 × `retire:bytes:{generation}` (≤ V each)
in one transaction is 1,000 × V ≈ 100 MB — 10× `E_tx`, outcome (d). So the
obligation-**installation** is byte-budgeted (`≤ E_tx/2` per transaction ⇒ ≈ 50 max-generation
obligations, more for smaller records), each transaction per-object preconditioned, never all
1,000 at once. The `≤ E_tx/2` bound is settled here; the per-request batching mechanics are
#509's *within* that bound — explicitly **not** left open. Landed as: Decision 4 rule-1 bullet
(`0016:748`), new execution register **X54** (`0016:1530`), F4 disposition observable
(`0016:1645`-region).

## Verification

**Leg A1 (mechanical prose gates — the gating `C4-ci` legs for a docs patch).** I ran the
exact three subprocess invocations `cargo xtask ci` issues before its heavy Rust steps
(`xtask/src/main.rs:1548-1553`, prose gates run first and fail-fast), against the patched
worktree:
- `typos` (typos-cli **1.48.0** installed, so it *runs*, not warn-skips) → exit 0.
- `python3 docs/publishing/tools/lint_docs.py` → `lint_docs: OK`.
- `python3 docs/publishing/tools/render_site.py --check` (`markdown_it`, `yaml` import OK, so
  the dangling-link audit *runs*) → rendered 98 pages, **link audit OK**.

I did not run the full `cargo xtask ci` (fmt/clippy/build/test/DST) because this patch touches
no Rust — those legs are vacuous on a no-code diff (as the brief notes for `C4-verify` and
`C5-mutants`), and the prose trio is the whole of A1 that a docs change can move. Check re-runs
the real gate.

Also validated mechanically: markdown table column-consistency across the whole document (a
Python pass over every table — no row's pipe-count deviates from its header), and that
`patch.diff` **applies cleanly to `cd82a29`** on a pristine tree (`git apply --check` green).

## The forced self-refutation (adapted for a docs deliverable — there is no test)

The brief declares **`Test file: none`** — a design document has no headless regression test
that drives production code, and manufacturing one would be the fabricated-stand-in the Do
protocol forbids. So the three questions map to the two verification tiers the brief defines:

- **(a) Genuine red?** Leg A1 is genuinely falsifiable and *has* fired: a dangling relative
  link or a typo turns `render_site.py --check` / `typos` red (both toolchains are installed
  and run, not warn-skip). Leg B's "red" is criterion-absence under the Refutation standard,
  exercised by review — and it has gone red on exactly this predicate every prior iteration
  (the T4 gate re-runs 3 fresh passes over the *new* diff, so an unresolved finding re-arises
  mechanically). This pass removes the four constructions that were red.
- **(b) Production path?** N/A for a document — but the design binds against the **real** tree:
  every contract the fixes rest on is a live citation (`gc.rs:171-176`, `write.rs:474-500`,
  `metadata.rs:60-70`/`:344-350`, `traits/src/lib.rs:744-758`/`:825-843`, `AGENTS.md:181-183`),
  re-verified on `cd82a29`. No fix invents a seam that isn't there or a bound the envelope
  can't hold.
- **(c) Fixture includes the fault?** Each fix adds its refuting execution to the **execution
  register** with the fault present, not curated out: X29 (fence lands in the write→CAS
  window), X53 (fresh store, counter absent), X49/clock-table (straggler lands at the grace
  boundary), X54 (1,000 large objects in one bulk delete). Each is written so #508's next Plan
  can lift it as a success-criterion negation.

**Honest limit / NEEDS-HUMAN:** leg B is a judgment tier — the Check reviewer + adversary
(brief-aware, under the Refutation standard) and the human at sign-off decide whether the
settlement bar is met. Per the brief this is a **by-design NEEDS-HUMAN §6 row** (a proposal
change; INTEGRATION §4), and the proposal ships at `status: draft` — ratification (draft →
accepted) is a separate later governance act under ADR-0037, never this cycle's. I did not and
cannot mechanically prove leg B; I addressed every input it grades (the four findings, the
ADJUDICATE), preserved the refutation-surviving core, and left the one ⚑ serialization-cost
question flagged for the human.

## STOP discipline

Draft artifact only. No PR pushed, opened, or marked ready.
