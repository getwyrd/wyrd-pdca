# Brief — issue 650 / gc-scrub-through-resolver-fail-closed-containment

> Slice **3 of 6** of the #635 re-slicing (0016 decision 7(e)). History and the closed PR #647
> are on the parent issue — https://github.com/getwyrd/wyrd/issues/635.

- **Slug:** gc-scrub-through-resolver-fail-closed-containment
- **Defect:** Two defects in the **reference set** GC and scrub both gate on.
  1. **It cannot see a segmented object's chunks.** `referenced_fragments` builds the protected
     set from each committed record's inline chunk list (`crates/custodian/src/gc.rs:251` on
     `origin/main`), so a segmented object's fragments are simply **absent from it** — and GC's
     safety gate (`gc.rs:159`; the loop's load-bearing invariant, whose violation is named
     *silent corruption*, at `gc.rs:22-25`) would pass them to `delete_fragment`. Scrub reads the
     same set (`scrub.rs:43`,`:75`) and would never verify them while still reporting the store
     clean.
  2. **An incomplete reference set still certifies — the finding still open at #647's close.** In
     the closed PR `ReferenceSet::protects` short-circuits `true` while the set is incomplete
     (correct for *reclamation*, and why nothing is deleted), but `gc_step` then audits **every**
     otherwise-unprotected fleet fragment as `skip reason="referenced"` and still returns
     `Reconciled::Satisfied` — it reports the store converged while holding a set it knows is
     partial. Scrub's twin **was** fixed in #647 (it returns `Blocked` for the identical
     condition); the GC side was not. Two passes reading one set answer differently about the
     same store.
- **Success criterion:** The added test target `crates/custodian/tests/segmented_map_consumers.rs`
  passes and binds the issue's acceptance, driven through the real fenced control point
  `wyrd_custodian::reconcile_step` (base-visible):
  1. **A segmented object's fragments survive GC + scrub, asserted positively.** After a GC pass
     **past the grace window** and a scrub pass over an object seeded as raw `seg:` records + a
     segmented root, every fragment it owns is still present on every D server that held it, **and**
     a drain of a server holding one answers `ReconciliationStatus::Pending` — the positive
     observable, because "GC deleted nothing" also passes when GC did nothing at all.
  2. **With one unresolvable inode, GC and scrub never certify and reclaim nothing.** Both steps
     return `Ok(_)` and **not** `Reconciled::Satisfied`, and nothing of that object is reclaimed;
     the blocker is attributed on the audit seam naming the object. (The positive
     `Reconciled::Blocked` match ships in the appended `crates/custodian/tests/{gc,scrub}.rs`
     legs, which `C4-ci` runs — see *Test file* for why the discriminator asserts the
     base-expressible half.)
  3. **One damaged object does not end the walk** — with that object present the reference build
     still completes over the rest of the store, a healthy object's fragments are still protected
     and still verified. A store-access fault (not one object's fault) still propagates.
- **Falsifiability:** RED is an **assertion** red on base-visible symbols. `C4-verify` resets
  `../wyrd-verify` to this bundle's base — a wave>0 bundle gets
  `PDCA_VERIFY_BASE=origin/pdca-integration/main` (`src/pdca_harness/gates.py:371-389`), honoured
  ahead of the brief base (`run-verify.sh:186-192`), so the patch applies onto a tree already
  carrying #648 and #649. `--classify` dry-run confirms the single discriminator
  `ADDED_TEST crates/custodian/tests/segmented_map_consumers.rs`. The RED leg keeps it, reverts
  `crates/custodian/src/{gc,scrub,reconciliation}.rs`, and runs
  `cargo test -p wyrd-custodian --test segmented_map_consumers`. On that tree GC/scrub still read
  the inline list, so criterion (1) fails (the segmented object's fragments are unprotected /
  the drain answers wrongly) and criterion (2) fails on `Ok(_)` — assertion failures, and the file
  compiles because it imports only `reconcile_step`, `Reconciled`, `GcContext`, `ScrubContext`,
  `Custodian`, `FencedZone` and `desired_state::*`, all `pub` on `origin/main` and already used
  this way at `crates/custodian/tests/gc.rs:33-40`. No dev-dependency is added. Plain Linux
  workspace over in-memory trait doubles, no topology, no cfg gate on
  `crates/custodian/tests/*.rs`, so neither the vacuous `0 tests … ok` branch
  (`:383-389`,`:420-427`) nor a compile-red-scored-as-pass can occur.
- **Invariant to restore:** **C-1 — no permanent or data-losing failure mode is an acceptable
  cost** (`docs/principles.md` §5 C-1 / §6 *Storage lifecycle / reclamation*; maintainer's rule
  2026-07-25; `0016:2802-2813`; corroborated in-tree by `../wyrd/crates/custodian/src/gc.rs:22-25`,
  whose violation is named **silent corruption**). Over this slice's category — **passes that
  reclaim or certify durable bytes on the strength of a reference set**:
  - **A partial reference set authorizes nothing and certifies nothing.** A pass that cannot see
    every committed object's chunks may not reclaim a byte *and* may not report the store
    converged, verified or clean. Reclamation-safety and certification-honesty are the same
    property read twice; a pass with only the first tells an operator to decommission a server
    whose bytes a live object may still own.
  - **Every pass reading one set gives the same answer about it.** Two passes disagreeing over the
    same incomplete set is a state the operator cannot resolve from outside.
  - **Containment is per object — a narrower blast radius, never a weaker rule.** One unreadable
    record is attributed and the walk continues; nothing of it is verified, moved or reclaimed on
    the way past. Ending the walk is the same availability loss for every healthy object; skipping
    it silently is the reclaim-live-bytes failure.
- **Repo + branch target:** getwyrd/wyrd @ main   (resolved and verified at Plan:
  `git ls-remote --heads origin main` → `9120f7a`, matching the sandbox's `origin/main`)
- **Depends on:** 649
- **Conflicts with:** *(none — this slice is custodian-only; #651 depends on it and #652 sits
  behind #651)*
- **Ordering note:** Third of the serial chain — it calls #649's resolver from the reference
  build, and #651's restore calls `gc::referenced_fragments` and gates on `protects`, so #651 must
  build on this slice's accepted result.
- **Surfaces:** data
- **Difficulty:** medium
- **Scope:** route the **reference build** through the resolver and make **certification** honest
  in both passes that read it. `crates/custodian/src/gc.rs` — the reference build resolves a
  committed map through #649's resolver by bounded `seg:` ranges; `ReferenceSet` carries the
  objects it could **not** resolve; `gc_step`'s outcome reflects that incompleteness instead of
  reporting convergence. `crates/custodian/src/scrub.rs` — inherits the same set and the same
  outcome rule; the two passes' answers must be identical for identical input.
  `crates/custodian/src/reconciliation.rs` — the outcome type gains the "cannot certify" answer
  and the rule for combining outcomes across loops. Plus their existing test files and the added
  fixture. **Caller-first:** every production symbol introduced here has a caller **in this
  slice** — the incompleteness field is read by `protects` and by both steps' outcome; this slice
  lands no behaviour flip and no producer of segmented maps. **Out of scope:** restore,
  reconstruction, backfill, rebalance and `desired_state` (#651) — do not route them here even
  though they will share this fixture; the record-ceiling helpers and `repoint_chunk` (#651); the
  chunk-id floor (#652); the committer (#653); any new/edited ADR / spec / proposal; any
  conformance-vector change.
- **Budget:** ≤ ~1,500 added semantic lines (non-blank, non-comment, non-mechanical), ≤ 15 files.
  Mechanical migration — **`match` arms over `Reconciled` gaining the third variant**, 22 files
  across 4 crates on `origin/main` — is counted separately and allowed on top; declare it as that
  pattern. Salvage **production** here is very small (`gc.rs` +33, `scrub.rs` +15,
  `reconciliation.rs` +30 = **78 semantic lines**; the re-slicing's ~800 figure is production
  **plus** its test files), so the whole budget risk is tests: take the binding legs for criteria
  (1)–(3), not every test body #647 accumulated. If mid-build the tree exceeds this, STOP and hand
  back a proposed split instead of finishing — an over-budget patch is iterate-to-Plan by default,
  not another Do round.
- **Repro instruction:** `git -C ../wyrd show origin/main:crates/custodian/src/gc.rs` —
  `referenced_fragments` at `:251` reads the inline list only, the safety gate is at `:159`, and
  `gc_step` returns `Changed`/`Satisfied` at `:211-213` with no third answer (`Reconciled` has
  exactly two variants, `crates/custodian/src/reconciliation.rs:20-25`). For defect (2), read the
  closed PR in `sources/salvage.diff`: `protects` short-circuits, the audit loop skips everything
  as `"referenced"`, and the step still returns `Satisfied`, while `scrub.rs` returns `Blocked`
  for the identical condition.
- **External dependencies:** `typos`, `docs-renderer` — registered doctor.checks rows (ids
  `typos`, `docs-renderer`), named because this slice edits a living-architecture paragraph and
  `cargo xtask ci`'s prose gates warn-and-skip when those tools are absent locally
  (INTEGRATION §3). Nothing else beyond the base Rust toolchain — the custodian loops run over the
  `traits`/`core` seams with in-memory doubles (the ADR-0010 boundary restated at `gc.rs:28-30`).
  No Docker, no protoc, no live backend, no new dev-dependency.
- **Test file:** `crates/custodian/tests/segmented_map_consumers.rs` — a **NEW** file (this
  project's C4 discriminator is an added `*/tests/*.rs`; a leg appended to the existing
  `crates/custodian/tests/{gc,scrub}.rs` degrades to the green-only branch,
  `run-verify.sh:392-402`). It MUST import only symbols visible on this slice's base — the
  `crates/custodian/tests/gc.rs:33-40` import set — so the red leg is an assertion failure, not a
  compile error. That is why criterion (2)'s discriminator assertion is `Ok(_)` **and not
  `Satisfied`**: `Reconciled::Blocked` is added by *this* patch, so naming it here would compile-fail
  on the red leg and score as a pass. The positive `Blocked` match ships in the appended
  `{gc,scrub}.rs` legs, which `C4-ci` runs. Keep the fixture helpers factored so #651 can reuse
  them without appending its binding legs here — it must ship its own added file.
- **Verification posture:** default — assertion-red on the base (which carries #648 and #649),
  green with this patch, both at Check. No deferred green, no off-Check environment.
- **Citations expected:** cite `path:line` on the target branch for every change. **Salvage —
  extract and adapt from `results/issue_650/sources/salvage.diff` (this bundle, permitted input);
  do not re-derive settled code.** It carries #635's `gc.rs`, `scrub.rs`, `reconciliation.rs`,
  their two test files, the shared `segmented_map_consumers.rs` fixture and the docs file. **Take
  only the fixture's GC/scrub legs** — the "maintenance resolves and never reclaims", "post-grace
  GC reclaims nothing" and "drain answers `Pending`" legs, plus the in-memory stores and raw-record
  seeding helpers. The read-path leg is already covered by #649's own file, and the
  reconstruction / rebalance / backfill / containment legs belong to **#651** — do not pull them
  forward. Peers Do MAY open: **the peer that already solves defect (2)** — scrub's outcome
  contract and its `Blocked` return in the salvage; GC must return the same answer for the same
  condition, do not invent a second rule. And **the containment shape this repo already uses** —
  `origin/main:crates/custodian/src/desired_state.rs:82-98`,`:150-179`
  (`ReconciliationStatus::PendingMalformed`: attribute the blocker, name the chunks, keep
  answering); the drain-status surface must keep behaving that way. Normative design: 0016
  decision 7(e) `:2393-2415` and decision 2's per-consumer protection table `:765-830`.
- **Docs-currency:** `docs/design/architecture/06-runtime-view.md` §6.2 step 2 — extend #649's
  resolver paragraph with **the GC/scrub containment sentences this slice lands, and only those**:
  no pass that can reclaim bytes acts on an incomplete reference set; a pass that only *verifies*
  does not certify one either — it verifies every other object, names the one it could not, and
  reports the store **not certified**; a report-only surface answers "blocked" and names the object
  to repair. The repair/evacuation-walk sentences belong to #651 and staged publication to #653.
- **Prior-art check (triage cycles):** searched by affected file path across merged history, open
  and closed PRs. `crates/custodian/src/gc.rs`: 13 PRs — **#647 CLOSED unmerged** (the salvage
  source, closed on **reviewability, not correctness**; its GC certification finding is what this
  slice fixes) and 12 MERGED, none about chunk-map resolution (#564, #559, #555, #531, #489, #448,
  #397). `scrub.rs`: 7 PRs — #647 CLOSED, six MERGED (#564, #531, #397, #362, #247, #189).
  `reconciliation.rs`: 6 PRs — #647 CLOSED, five MERGED (#193, #190, #189, #188, #187), and none
  adds a third `Reconciled` variant. No merged or rejected prior art for resolver-backed reference
  building or for a "cannot certify" outcome.
  **Do-not-re-earn (standing rejections; content-stable — they bind wherever the finding
  re-lands, not at a line):** (i) *caller-side fan-out timeout* — rejected 3× across #508/#636:
  the `ChunkStore` implementation owns the network bound, not the caller; (ii) *retraction of
  already-published bytes* — rejected 4× in #638 on unchanged evidence; (iii) *"`Completed`
  releases its admission slot"* — withdrawn as unsatisfiable; a `Completed` tombstone **stays
  counted**; (iv) every settled decision named in the slice issue's body, in particular that
  `protects` short-circuiting `true` on an incomplete set is the **correct reclamation rule** —
  the defect is the step's return value, not the predicate. Do MUST record each rejection in
  `review-rejected.md` **at every line the finding is reported at**.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR MAY
happen during the cycle. The PR MUST NOT be marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 1): rebuilding for the implementation-level findings — C4 Verification (red→green) — Handling of the inherited advisory is owed — red→green plus typos, docs render, fmt, clippy, build, workspace tests, conformance, statics, mutants, and DST passed, but a scratch-local real `cargo deny` remains red for RUSTSEC-2026-0221 against unchanged `event-listener` 5.4.1 at `Cargo.lock:1204`, so the required whole gate is not green.; C5 Causal adequacy — Rebuild must cover structurally unreadable committed roots — a direct test of a segment-count-mismatched committed inode returned `Err` at the unconditional decode propagation in `crates/custodian/src/gc.rs:314`, ending the walk instead of the per-object containment promised at `crates/custodian/src/gc.rs:294`.; T4 Contribution — Contribution-review disposition is owed — affected-file history independently covered merged and closed/unmerged work and found #647 as the sole unmerged prior art, but `scripts/review-branch` and `scripts/pdca contribcheck` are absent from the artifact-only inputs, so the reported 19 findings and contribution PASS cannot be independently reproduced.; T5 Judgment — Rebuild must bind operator attribution — criterion 2 asserts only outcomes and byte survival at `crates/custodian/tests/segmented_map_consumers.rs:468` and `crates/custodian/tests/segmented_map_consumers.rs:498`, while inode naming is supported only by code-read at `crates/custodian/src/gc.rs:458` and `crates/custodian/src/scrub.rs:224`, so an audit regression would pass.; C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo deny check` failed with exit status: 1; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 19 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue.
- Failing gate: C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) — xtask: `cargo deny check` failed with exit status: 1
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 19 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 2): rebuilding for the implementation-level findings — C5 Causal adequacy — Rebuild must contain a root that becomes unreadable during resolver restart — a driven valid-root→unreadable-root race returned `Err(Store(...))` because `crates/core/src/metadata.rs:2517` emits a raw decode error that `crates/custodian/src/gc.rs:389` propagates instead of attributing to the object.; T4 Contribution — Contribution-review disposition is owed — affected-path merged and closed/unmerged history was independently checked and found #647 as the sole functionally relevant unmerged prior art, but the named `scripts/review-branch` and `scripts/pdca` tools are absent, so the reported 8 blocking findings and contribution-check result cannot be reproduced.; T5 Judgment — Rebuild must make the reverse-order aggregation test genuinely exercise `Changed`→`Blocked` — an explicit fixture check returned `Satisfied` for the supposed converging GC context at `crates/custodian/tests/segmented_map_consumers.rs:947`, so that leg currently proves only `Satisfied`→`Blocked`.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 8 blocking, 2 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_. 1 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 8 blocking, 2 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 3): rebuilding for the implementation-level findings — T4 Contribution — Contribution-review disposition is owed — affected-path merged and closed/unmerged history confirms #647 as the sole functionally relevant prior art, but absent `scripts/review-branch` and `scripts/pdca` tools leave the reported 2 blockers and contribution-artifact check unreproducible.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_. 2 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 4 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 4): rebuilding for the implementation-level findings — T4 Contribution — Contribution disposition is owed — affected-path GitHub history found #647 as the only functionally relevant unmerged prior art (#336 is unrelated), but `scripts/review-branch`, `scripts/pdca`, and their outputs are absent, so the reported three-agent review and contribution-check passes remain provisional.. 3 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Full previous attempt preserved in `iteration-v4/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
