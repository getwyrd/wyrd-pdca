# Build notes — issue 651, iteration 13 (Do)

> Withheld from the reviewer by the driver; written for the human at sign-off.

## What this round is

A **rebuild on the same brief**, starting from the v12 tree (`iteration-v12/patch.diff`, applied
to the worktree at `origin/main` = `d50f0ca`, then edited). v12's gates were green — the failing
gate was **T4 batch review, 4 blocking findings** — and the iteration-12 carry-forward directs:
*fix them rather than re-litigate*, and *do not re-scope or split on the size backstop*.

Everything below is my delta over v12. The v12 substance (the containment in
`reconcile_after_restore`, `RestoreReport::unresolvable`, `needs_human`,
`ReconciliationStatus::PendingUnresolvable`, the discriminator, the docs touches) is unchanged
except where a finding named it.

## The four blockers, and what each cost

### 1–3. `cli.rs:1279 / :1291 / :1308` — "the paragraph prints a count, not the record names"

Three of the three review passes filed the same defect against the new UNREADABLE paragraph: it
printed `report.unresolvable.len()` and sent the operator to the audit log, while the runbook this
patch ships (v12's `docs/design/architecture/m4-first-deployment-blueprint.md:608`; now `:610`, reworded to match what the command prints) promised *"the pass
NAMES each one — in the audit log and in the paragraph it prints"*. That is the v10 **T2** defect
class again (claim only what the run can evidence), pointed the other way: the prose was true of
the audit trail and false of the command.

**Fix — the command names them** (`crates/server/src/cli.rs:1322`, helper at `:1351`, bound at
`:1343`). `RestoreReport::unresolvable` already carries every name, escaped by `gc::object_name`,
so printing them costs only line width. The bound (`NAMED_UNREADABLE_RECORDS = 20`) exists because
the paragraph is one stderr line and a store whose whole `inode:` namespace is damaged would
otherwise print every key into it; the remainder is **counted, never dropped**, and the audit trail
plus the report still carry the full list. Both sides are pinned by
`restore_verdict_names_the_blocking_records_and_counts_the_ones_it_cannot_fit`
(`crates/server/src/cli.rs:2806`) and by a per-name assertion inside the existing verdict test
(`:2789`).

**What I did NOT do, deliberately:** the `dangling` / `misplaced` paragraphs still say *"See the
audit log for each chunk id"*. That text and behaviour are `origin/main`'s
(`cli.rs:1215`, `:1226` on the base), the runbook makes no naming promise for them
(`m4-first-deployment-blueprint.md:608` on the base: *"the audit log gives each chunk id"*), and the brief
scopes this file to *"the summary cell, one NEEDS-HUMAN paragraph and the exit code"*. So there is
no promise/behaviour gap there to close, and closing it anyway would edit two base paragraphs this
slice does not own. If a reviewer raises it, it is a scope decline, not a fix.

### 4. `restore.rs:326` — the mark-race between the pass's two readings

The finding: *"A committed object or changed placement appearing between the two namespace scans
is absent from `referenced.protects` but present in `committed`, so its live fragments can be
marked collectable despite the claimed one-reading safety rule."* Correct, and it is the C-1
direction that deletes: v12 extended the two readings' agreement only to **holes** (either read's
unreadable record withholds every mark), not to **references** (the mark gate still consulted the
older read alone). The doc block claimed more than the code did.

**Fix — the gate consults both readings** (`crates/custodian/src/restore.rs:358`), over the
divergence set `AppearedSince` (`:539`, built at `:299` by `appeared_since`, `:564`): every
`(dserver, fragment)` the *report* read places that the reference build never saw, plus every
chunk id the report read found **malformed** (`CommittedChunks::malformed`, `:519`, recorded at
`:652`) — the second rule `gc::ReferenceSet::protects` applies (ADR-0040 decision 4, fail safe).
`canonical` is built over the same union (`:341`) so the displaced branch judges against the map
both readings describe. Only the **difference** is materialised, so in the normal case (writers
stopped, the two readings agree) it is empty and costs nothing.

The docs now state exactly what the code does (`:220-243`), including the honest limit: a commit
landing after *both* reads is still the runbook's writers-stopped contract, not this pass's; what
is this pass's own is that the two readings **it** makes never license a mark between them.

Red→green: `an_object_committed_between_the_two_readings_is_never_marked`
(`crates/custodian/tests/restore_reconcile.rs:984`), driven by a metadata double that publishes
the late records the instant the first `inode:` scan is answered (`:47`, `:58`, `:90`). Against
the v12 production file it fails with `stranded_marked: 3` — the pass marked the late object's
only copy *and* the malformed chunk's fragment. With the fix: `stranded_marked == 1`, exactly the
genuine stray, and no `orphan:` record for either late fragment.

## The alternative I rejected, with its cost

**Read the committed namespace ONCE** — drop `committed_chunks` entirely and derive the report's
per-chunk expectations from `gc::ReferenceSet` (`placed` grouped by chunk id for the fragment
list, `schemes` for `k`). That removes the *cause* rather than reconciling the two readings, and
the brief's criterion (2c) calls a single-reading pass "the better one", so I costed it properly
rather than dismissing it:

- **Net lines: it is smaller, not larger.** ‑101 lines (`committed_chunks` + `CommittedChunks`,
  `restore.rs:504-680`), +~30 for the grouping, so roughly **‑70** against my +33 semantic. Cost is
  *not* the reason to reject it.
- **It changes report semantics outside this brief.** `ReferenceSet.placed` is keyed
  `(dserver, FragmentId)` with no owner, so grouping it by chunk id **merges two committed objects
  that reference one chunk id into a single verdict**. Concretely: object A places chunk C at D1
  (fragment present), object B places C at D2 (absent). Base and this patch report B's chunk
  `misplaced` (`placed = 0 < k`), and the command exits non-zero. The merged form sees
  `frags = {(D1,C0),(D2,C0)}`, `placed = 1 >= k = 1`, `placed < frags.len()` → **`under_replicated`**
  — "the repair loop will rebuild it" over an object that is down. `is_clean()` stays false
  (it counts `under_replicated`), but `needs_human()` flips **true → false**, so the command
  exits **0** over a chunk no read can resolve. That is a certification
  regression in the exact class the brief spent a paragraph declaring out of scope and unreachable
  ("no claim-counting on `Expected` / `committed_chunks`"; ids ≥ 2^127 cannot collide). Buying an
  unreachable-state regression to fix a reachable one is a bad trade even when the diff shrinks.
- It also loses scan-order reporting (a `HashSet` walk) and would force a rewrite of the two
  decay-based regressions (`marks_and_report_rest_on_one_reading` in the discriminator and
  `every_unreadable_committed_record_is_named_and_stops_the_run_being_certified`), whose whole
  subject is a record readable on one read and not the other — a fixture a single-reading pass
  cannot exercise.

The union fix is **not** a probe on a symptom: it removes the unsafe *decision* (marking on a
reference set the pass itself has already superseded), rather than detecting it after the fact.
The invariant it restores is the brief's own — *"a conclusion and the reading it rests on are
one"* — in the fail-safe direction, at the cost of a stray surviving to the next run of an
idempotent pass.

## Refuting my own test (the three forced questions)

**(a) Genuine red?** Yes, three separate reverts, each re-run:

| leg | revert | result |
|---|---|---|
| discriminator (5 legs) | `run-verify.sh` RED leg (production reverted, added test kept) | 5/5 **FAILED** — `SegmentedMapUnsupported { operation: "restore::committed_chunks" }` on (1)/(2a)/(2b), a missing `unresolvable-chunk-map` attribution on (3), a decode error on (2c). Gate verdict: `run-verify.sh: PASS — red without the fix, green with it.` |
| mark-race | `git show :crates/custodian/src/restore.rs` (the v12 file) over mine | `an_object_committed_between_the_two_readings_is_never_marked` **FAILED**: `FragmentId { chunk: 55, index: 0 } was marked collectable … RestoreReport { stranded_marked: 3, … }` |
| CLI naming | v12's count-only paragraph restored | both `restore_needs_human_agrees_with_every_paragraph_it_prints` and `restore_verdict_names_the_blocking_records_and_counts_the_ones_it_cannot_fit` **FAILED** (`the blocking record inode:7 is not named in what the operator reads: …`) |

Every revert was undone and re-run green afterwards.

**(b) Production path?** Yes. The discriminator drives `wyrd_custodian::reconcile_after_restore`
and `wyrd_custodian::desired_state::reconciliation_status` — the real symbols, over in-memory
`MetadataStore` / `ChunkStore` doubles (the seams the pass is written against; no re-implementation
of the pass). `restore_reconcile.rs` drives the same pass. The CLI tests call `restore_verdict`,
the *same* function `cmd_custodian` calls at `cli.rs:1196` for its printed lines and its exit
status — not a copy of the text. Nothing is asserted against a mock of the behaviour under test.

**(c) Fixture includes the fault?** Yes. The damaged object is seeded **into the same store the
pass walks** (`seed_damaged` asserts `resolve_chunk_map` genuinely fails before the leg runs), the
segmented object is a real `seg:`-backed root with a segment that was never written, the
decaying record really is undecodable after the first read (asserted at the end of
`marks_and_report_rest_on_one_reading`), the injected store fault is matched on its own text, and
the race fixture's late objects are **in** the namespace the second read returns with their bytes
on the D server the pass lists. The controls are there too, so "nothing was marked" is never
vacuous: a genuine stray that a complete reading *does* mark (`stranded_marked == 1` in the race
leg; the CONTROL store in the (2c) leg), and `report.is_clean()` asserted **true** on the one store
that has a right to it (`restore_reconcile.rs:330`).

## Gates run here (the project's own runners, not hand-rolled)

- `./engine/scripts/run-verify.sh` (C4-verify) → **PASS**: green with the fix, red without.
- `./engine/xtask.sh ci` (C4-ci, the whole gate) → **exit 0**, `xtask ci: all checks passed`.
  The prose gates really ran on this host (`typos`, `lint_docs: OK`, `render_site: link audit OK`),
  so this is CI parity, not the warn-skip path. `cargo fmt --all` was run over every file touched,
  so the target's commit hooks have nothing left to reject.

## Size, honestly

- **8 files** (the eight the brief names — no ninth).
- **921 added semantic lines** by a crude counter (non-blank, non-comment; multi-line assertion
  messages each count as a line), against the brief's ≤ 700. v12 measured **743** the same way;
  my delta is **+178**: restore.rs +33, cli.rs +54 (44 of it the new test), restore_reconcile.rs
  +91 (the race regression, whose assertion prose inflates the count).
- Patch is **121 KB** (v12: 103 KB).

I did **not** stop and hand back a split, and that is a deliberate reading of the two instructions
that conflict here: the brief's budget clause says stop, the iteration-12 carry-forward — later,
and specific to this bundle — says *"Size backstop noted … but explicitly NOT treated as a split
trigger … do not re-scope or split on this basis"* and directs a Do round on the four blockers.
The growth is entirely review-driven (two regressions and a naming helper the reviewers asked
for), so splitting now would ship the blockers unfixed. **Flagging it for the human anyway**: if
the budget is to bind, the discriminator (443 of the 921) is the item to re-scope, and that is an
iterate-to-Plan decision, not mine.

## Bundle hygiene

`review-rejected.md` line references were re-filed against **this** patch's line numbers (the
gate binds a rejection to `file:line` + CLASS + MATCH, so a moved line reads as an unsuppressed
finding): the `resolve_chunk_map` await moved `543 → 632`, `committed_chunks`'s scan `524 → 613`,
its call site `277 → 293`, `referenced_fragments` `261 → 277`, `orphan_leases` `271 → 287`,
`pending_chunks` `272 → 288`, the counter `721 → 815`, the `deferred: #681` marker `674 → 604`.
Two **scope-decline** rationales still cited the cross-object ambiguity apparatus the brief
dropped (`restore.rs:469/562/913/938` — code that no longer exists); they now state the decline
without citing removed code. No `desired_state.rs` line moved.

## Anything a reviewer might raise that I decided rather than missed

- **A divergence-only malformed chunk is protected silently** — no audit event. Consistent with
  base restore, which emits nothing for its mark-gate skips (`gc::emit_skip` is GC's, not this
  pass's); adding a per-skip event here would be new surface the brief does not ask for.
- **`AppearedSince` duplicates `ReferenceSet::protects`'s two rules** rather than extending
  `ReferenceSet`. `gc.rs` is explicitly untouched by the brief's scope; the method mirrors the
  shape so the two read alike.
- **The 20-record bound is a judgement call.** Documented at the constant, pinned from both sides
  by a test, and it never drops a record silently.

## Self-review against the target's standing rubric (`AGENTS.md` § Review rubric & protocol)

Read before emitting the patch, applied to it as the last step:

- **One clock per correctness lifecycle** — no clock read added or moved; the pass still stamps
  and judges on the caller's `now_millis`.
- **Narrow trait seams / dependency direction (ADR-0010)** — `AppearedSince` is `std` collections
  inside `restore.rs`; no crate, no runtime, no backend enters `custodian`'s traits/core/tracing
  boundary. `named_records` is a `String` helper in `cli.rs`.
- **Metadata validation boundaries (ADR-0045 / ADR-0040 decision 4)** — a malformed placement met
  by the report read is treated as *fully referenced* in the maintenance path (strict), never
  identity-filled; decode failures still surface as errors and are contained per record.
- **No DST-reachable shared mutable global state** — none added; the doubles' `Mutex` fields are
  per-test.
- **`#![forbid(unsafe_code)]`** — no new crate root; the added test file carries it.
- **Docs currency** — the patch alters a library surface (`RestoreReport::unresolvable`,
  `ReconciliationStatus::PendingUnresolvable`) and the command's printed verdict/exit status, and
  both living architecture docs are updated in the same patch (`06-runtime-view.md:31`,
  `m4-first-deployment-blueprint.md:599-618`), with the runbook now claiming only what the command
  actually prints.
- **Absent/unsupported entries** — the whole point of the slice: an unreadable record is named,
  reported and exits non-zero; nothing is silently skipped, and the new assertions check the
  `orphan:` records themselves, not only counts.
- **Await discipline** — the patch adds **no** await. The pre-existing ones carry the standing
  `#508/#636` rejection, re-filed at this patch's line numbers.
- **Transactions** — mark batching, its commit points and its "claim evidence only once durable"
  ordering are untouched.
- **Test fidelity** — the doubles model production semantics (a commit landing between two scans,
  a record undecodable to every read after the first, a store fault that is not a `ChunkMapError`);
  no destructive/concurrent production path is added that would owe a Tier-0 DST leg.
- **Reviewer protocol** — every round-12 finding is fixed (none silenced); out-of-scope items stay
  declines with issue references in `review-rejected.md`.
