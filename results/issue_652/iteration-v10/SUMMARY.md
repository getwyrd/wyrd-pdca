# Result — issue 652 / recovery-total-over-damage

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: `metadata::high_water_marks` is what `Gateway::recover` runs **before the gateway
  serves anything** (`crates/server/src/lib.rs:123-124` on `origin/main`). It refuses to finish
  over content it cannot read, and half of what it computes has no consumer.
  1. **One damaged record stops the gateway from starting.** The `inode:` walk decodes every
     value with `?` (`crates/core/src/metadata.rs:2081`) and rejects a segmented root outright
     (`:2082-2087`), so a single unreadable record makes `recover()` return `Err` and costs
     **every healthy object** its availability. #648 enforces the segmented root's structural
     invariants at decode, which widens the set of values that can fail it — so this is live,
     not latent. The **same namespace** is already walked correctly elsewhere: GC's
     `referenced_fragments` contains a decode failure per-record and walks on
     (`crates/custodian/src/gc.rs:378-382`).
  2. **The second half of `recover` fails the same way.** `seed_next_inode_floor` parses the
     persisted counter with `std::str::from_utf8(bytes)?.parse()?`
     (`crates/server/src/cli.rs:1696`), so corrupt `meta:next_inode` bytes are equally fatal to
     startup. Leaving this fail-loud would give away the very property this slice establishes.
  3. **The chunk-id half has no caller.** `Gateway::recover` discards it
     (`lib.rs:124`, `let (max_inode, _max_chunk) = …`) and `recover`'s own doc already says so:
     "Chunk ids need **no** recovery: they are coordination-free (a per-gateway random
     `chunk_epoch`, ADR-0019)" (`lib.rs:114-117`). Computing it forces two further complete scans (`metadata.rs:2094`
     `pending:`, `:2105` `orphan:`) whose only product is that discarded number, and #647's
     attempt to harden it produced a floor that silently reported **0** for a corrupted flat
     root — an under-approximation a recovery path must never produce.
- Success criterion: The added test target `crates/server/tests/gateway_recover_totality.rs`
  passes and binds the issue's acceptance, driven through `Gateway::recover()` — whose signature
  `(&self) -> Result<()>` is unchanged by this patch:
  1. **Total over an unreadable `inode:` value.** With a healthy committed object **and** a raw
     undecodable `inode:<N>` value in the same store, `recover()` returns `Ok(())`; the healthy
     object still reads back byte-identically; a subsequent new-key PUT **commits** with an inode
     id strictly greater than `N`; and the unreadable record is attributed rather than swallowed.
  2. **Total over a segmented root.** Same, with a structurally valid **segmented** root in place
     of the undecodable value: `recover()` returns `Ok(())` and the mark is still ≥ that record's
     key-derived id. (On `origin/main` this is the explicit refusal at `metadata.rs:2082-2087`.)
     A ready fixture exists — the JSON literal `SEGMENTED_ROOT_OK` (`metadata.rs:2693`) is a
     `#[cfg(test)]` const in `core`, so it is not importable, but its **bytes** can be pasted into
     the new test as a raw literal; `metadata::{InodeRecord, ChunkMap, encode}` are all `pub`
     (`:1350`, `:986`, `:1536`) if Do would rather construct one.
  3. **Total over a corrupt counter, in bounded time.** With `meta:next_inode` holding
     non-numeric bytes, `recover()` returns `Ok(())` **and the test completes** — a
     never-committing retry loop must fail this criterion, not hang it. Afterwards the counter is
     ≥ the recovered floor, so the next PUT still commits above every committed inode id.
  4. **The dead half is gone** — the issue's second permitted outcome:
     `git -C ../wyrd grep -n "_max_chunk" origin/<branch> -- crates/` returns nothing, and no
     `RecoveredIds` / `ClassIds` / byte-scavenging apparatus is introduced.
  5. **No regression on the case `recover` exists for** — the existing
     `recover_seeds_the_allocator_over_a_legacy_store_without_meta_next_inode`
     (`crates/server/tests/s3_http_wire.rs:666`) still passes unchanged.
- Repo + branch target: getwyrd/wyrd @ main   (resolved and verified at Plan 2026-08-04:
  `git -C ../wyrd fetch origin` then `log origin/main` → `d50f0ca`, unchanged since the previous
  brief. Carries #648 (PR #672) and #649/#650 (PR #683), so `high_water_marks` is in its
  post-#648 form — which is what makes defect (1) live.)
- Scope (one logical fix) / out of scope: make **startup recovery total over content it cannot read**, and remove the id-floor
  half that has no caller. Three production files, one function each:
  - `crates/core/src/metadata.rs` — `high_water_marks` yields the **inode mark alone**. A record
    whose value cannot be read — undecodable bytes, or a segmented root this function has no
    resolver for — still **contributes its key-derived id to the mark**, is **attributed**, and
    does **not** end the walk. The `pending:` and `orphan:` walks and the
    `IN_PROCESS_CHUNK_CEILING` chunk logic (`:2074`, `:2088-2110`) go with the mark they fed.
  - `crates/server/src/lib.rs` — `Gateway::recover` (`:123-124`) takes the narrowed result; the
    doc comments at `:229-263` describing what recovery does and does not recover follow it.
  - `crates/server/src/cli.rs` — `seed_next_inode_floor` (`:1691`) **only**: a `meta:next_inode`
    value that cannot be read must not end recovery, and the function must still leave the
    counter ≥ `floor`, terminating. Nothing else in this file is in scope.

  **`scan` STAYS.** Do not introduce `scan_page`, `for_each_page`, or any cursor walk. The
  `MetadataStore::scan` seam returns "one consistent cut" (`crates/traits/src/lib.rs:1020`);
  `scan_page` explicitly declines snapshot isolation (`:1061`), which *weakens* the recovered
  floor and is why the previous seven rounds ended in `alloc_inode`. That trade, and the
  allocator safety it requires, is **#687** and is not this slice's to pre-empt. The peer to
  mirror — `gc.rs:360-385` — walks this same namespace with `scan` and contains per record.

  **Plan decision on the issue's binary ("wire `max_chunk` to a real caller **or** delete it"):
  DELETE.** Ratified by the maintainer at Plan 2026-08-02, unchanged — **settled, not Do's to
  revisit.** The decisive fact is history, and Do must carry it into the commit body: the
  consumer was removed deliberately by `fdd34f1` (#487, 2026-07-08), which did both halves at
  once. Before it, `mint_chunk_id` was a plain counter from 0 and `recover()` *consumed* the
  floor. After it, ids are `(chunk_epoch << 64) | seq` with the epoch's top bit set — every
  minted id ≥ 2^127 (`lib.rs:229-241`), disjointness from the random per-process epoch,
  `next_chunk_seq` never seeded — and the same commit rewrote the callsite to
  `let (max_inode, _max_chunk) = …`. The cluster minter never needed it either:
  `chunk_id_minter` yields `(inode_id << 64) | seq` (`cli.rs:1716-1723`) with `inode_id ≥ 1`, so
  every cluster id is ≥ 2^64. Nothing in the tree mints below 2^64, so wiring would mean
  inventing a consumer. The issue's acceptance bullet 1 (floor ≥ every live chunk id, `seg:`
  ranges included) is thereby discharged **by construction**.

  **The standing test goes with it, and the same reasoning must travel.**
  `high_water_marks_refuses_a_segmented_root_rather_than_re_mint_its_chunk_ids`
  (`metadata.rs:3417`) reasons that a segmented root read as "owns no chunks" would let the next
  PUT mint an id its fragments occupy — a premise that **expired with #487** (it needs a minter
  allocating below 2^64; neither has since). Removing it reads as deleting a safety guard unless
  the reasoning above accompanies it in `build-notes.md` **and** the commit body. Its live half
  is not lost: criterion 2 supersedes it, requiring that same segmented root to be *contained*.

  **Out of scope:** paging / `scan_page` / bounded walks, and everything else in **#687** —
  including any `alloc_inode` change, any `require_absent(inode_key(…))` guard, and bounding
  `seed_next_inode_floor`'s retry count (its *termination* is criterion 3; choosing an attempt
  budget is #687's, which owns allocator contention). Also out: the chunk-id **minting** scheme
  (ADR-0019 / #487 — settled; changing it is a new ADR, INTEGRATION §2/§4); #651/#681/#682/#653;
  any new or edited ADR / spec / proposal; any conformance-vector change; **any docs paragraph**
  — the living-architecture edits belong to #648, #649–#651 and #653.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it.
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 8 mutants tested in 2m: 2 caught, 6 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_652/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — scripts/pdca contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Reviewing issue #652’s startup-recovery fix: contain unreadable inode/counter metadata, preserve the inode floor, and remove the unused chunk-id recovery half.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief gives assertion-level criteria for unreadable inode values, segmented roots, corrupt counters, attribution, allocator floors, and deletion of the unused chunk-id half, with the allocator-contention deferral explicitly settled in #687. |
| C2 Reproduction (red pre-fix) | PASS | The exact-base red leg compiled and ran all four added tests; all four failed by assertion on the pre-fix recovery errors through the stable gateway/counter entry points (`crates/server/tests/gateway_recover_totality.rs:217`, `crates/server/tests/gateway_recover_totality.rs:487`). |
| C3 Change | PASS | Scope is the briefed three production functions plus one added test target, with no paging or allocator-floor apparatus; 421 added nonblank/non-comment lines remain within the ~450 semantic-line budget (`crates/core/src/metadata.rs:2149`, `crates/server/src/cli.rs:1734`, `crates/server/src/lib.rs:133`). |
| C4 Verification (red→green) | PASS | Independent runs produced 0/4 passing on the base and 4/4 passing patched; the unchanged legacy recovery regression and all CI components passed, including cargo-deny from a writable scratch Cargo home (`crates/server/tests/gateway_recover_totality.rs:217`, `crates/server/tests/s3_http_wire.rs:666`). |
| C5 Causal adequacy | PASS | The fatal per-value decode/segmented handling and corrupt-counter parse are addressed at their causal sites, the end-to-end tests bind each cause plus the CAS race, and all eight in-diff mutants were caught or unviable (`crates/core/src/metadata.rs:2151`, `crates/server/src/cli.rs:1740`). |
| T1 Structure | PASS | The change stays on the existing core/server `MetadataStore` seams and the end-to-end target composes the real redb and filesystem adapters (`crates/core/src/metadata.rs:2149`, `crates/server/tests/gateway_recover_totality.rs:124`). |
| T2 Shape | PASS | `Gateway::recover(&self) -> Result<()>` remains stable, `high_water_marks` narrows to its sole consumed mark, and no `_max_chunk` or replacement floor apparatus remains (`crates/server/src/lib.rs:133`, `crates/core/src/metadata.rs:2149`). |
| T3 Runtime | PASS | Real-backend recovery, healthy-object reads, subsequent PUTs, and the counter race complete within the 60-second harness bound; the workspace and DST runtime suites are green (`crates/server/tests/gateway_recover_totality.rs:73`, `crates/server/tests/gateway_recover_totality.rs:197`). |
| T4 Contribution | NEEDS-HUMAN | Human must obtain and triage the two blockers reported by the unavailable driver-only `scripts/review-branch` report — exact-path merged/unmerged history and PR search were clean except closed-unmerged #647, but the failing batch-review gate itself cannot be independently reproduced. |
| T5 Judgment | NEEDS-HUMAN [impl] | Rebuild must add seeded Tier-0 DST coverage for the new destructive counter repair and concurrent-CAS path — the binding rubric requires it, while current coverage is only a real-redb forwarding fixture (`AGENTS.md:188`, `crates/server/tests/gateway_recover_totality.rs:422`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Maintainer must confirm that audit-and-repair to the stored inode floor is the acceptable operational response to a corrupt counter — automation proves the mechanics, not whether this availability/evidence tradeoff is fit for deployment (`crates/server/src/cli.rs:1712`, `crates/server/src/cli.rs:1791`). |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T4 Contribution — Human must obtain and triage the two blockers reported by the unavailable driver-only `scripts/review-branch` report — exact-path merged/unmerged history and PR search were clean except closed-unmerged #647, but the failing batch-review gate itself cannot be independently reproduced.
- [ ] T5 Judgment — Rebuild must add seeded Tier-0 DST coverage for the new destructive counter repair and concurrent-CAS path — the binding rubric requires it, while current coverage is only a real-redb forwarding fixture (`AGENTS.md:188`, `crates/server/tests/gateway_recover_totality.rs:422`).
- [ ] Validation — fitness-to-purpose — Maintainer must confirm that audit-and-repair to the stored inode floor is the acceptable operational response to a corrupt counter — automation proves the mechanics, not whether this availability/evidence tradeoff is fit for deployment (`crates/server/src/cli.rs:1712`, `crates/server/src/cli.rs:1791`).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_652/review-b
- [ ] size backstop — this slice is behaving oversized: 2 round(s) already spent (threshold 2). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.
- [ ] leaf produced no usable verdict (needs a human) — re-run the Check reviewer; this bundle has no advisory review and must not be accepted until one exists.
- [ ] external dependency: cargo-deny CLI compatible with `xtask`'s `--config`-before-subcommand invocation — blocked the last two steps of a local `cargo xtask ci` (`deny --all-features … advisories`, then licences/bans/sources), so I could not produce a full local C4-ci pass; every other step, including `cargo test --workspace` (158 suites), conformance, statics and dst, is green with this patch, and the failure reproduces on a tree without this patch's production changes. If the driver's C4-ci is green at Check, this is already answered.

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Converging, and the split (#687) was recent — the remaining gap is triage bookkeeping, not substance. Every substantive gate is green (C4-ci, C4-verify red->green 0/4 -> 4/4, C5-mutants 8/8 caught-or-unviable) and the advisory reviewer PASSed C1-C5 and T1-T3. Do NOT rework the patch shape; it is what the brief's Success criterion prescribes. The only work this round is to make T4-batch-review's two findings triage cleanly. Verified against the gate's own logic (`load_rejected` / `is_rejected` in `scripts/review-branch`): a finding is suppressed only on an exact three-way match of `file:line` + CLASS + a MATCH phrase that is a case-insensitive substring of the finding's ACTUAL rationale text. 1. `crates/server/src/cli.rs:1762` | TEST-GAP | "The new destructive, contention-sensitive repair path has only redb integration coverage and lacks the rubric-required seeded Tier-0 DST regression." This is a pure line-anchor miss — the substance is already recorded and its reasoning stands. TEST-GAP entries with MATCH `tier-0 dst` exist at cli.rs:1745, cli.rs:1717 and gateway_recover_totality.rs:315/364/486/487, and that phrase DOES match this rationale; they are simply anchored at the wrong line. The entry Do wrote for it at cli.rs:1747 fails twice over: wrong line, and its MATCH phrase ("a new destructive or concurrent path lands with seeded Tier-0 DST coverage (`crates/dst`)") does not occur in the reviewer's wording. Fix: record it at cli.rs:1762 as class TEST-GAP with a MATCH phrase lifted verbatim from the finding's own rationale (e.g. `tier-0 dst`), carrying the existing rejection reason (the crates/dst location costs a ~250-line cross-crate move of the persisted-allocator protocol out of wyrd-server, which cannot compile under --cfg madsim). 2. `crates/server/src/cli.rs:1745` | CONVENTION | "Changing `meta:next_inode` from fail-on-corruption to audit-and-repair alters persisted-field semantics without updating the living architecture documentation." Genuinely NEW — never answered in any round. No CONVENTION entry anywhere has a MATCH phrase occurring in this rationale (the ones at that very line carry `scan_page` / `paging` / `snapshot isolation` / `alloc_inode`; the rest are all unbounded-await findings). Answer it with a recorded rejection, NOT a code or docs change: the brief puts "**any docs paragraph**" out of scope explicitly, and assigns the living-architecture edits to #648, #649-#651 and #653. Anchoring discipline for this round: take the MATCH phrase verbatim out of the finding text in review-batch.md rather than paraphrasing it, and record each rejection at the line THIS round's review-batch.md reports it at. Two rounds have now been spent on rejections that were correct in substance but missed on line or phrase. The three judgment items in SUMMARY.md section 6 (T4 contribution, T5 DST coverage, fitness-to-purpose) are NOT cleared and remain open for the human at the next sign-off; they are not defects for Do to chase. The two stale section 6 entries carried in deferred-findings.json — "leaf produced no usable verdict" and the cargo-deny external dependency — are answered by this round's own evidence (check-review.md is populated; C4-ci is green) and should not drive any work.
- By / date: Eduard Ralph / 2026-08-02

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
