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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — ERROR cargo test failed in an unmutated tree, so no mutants were tested

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_652/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — scripts/pdca contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Reviewing issue #652: make gateway startup recovery total over unreadable inode records and counters while removing the unused chunk-id recovery mark.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief makes availability, allocator-floor, attribution, bounded-return, dead-mark deletion, and no-paging boundaries independently decidable at the actual pre-serve recovery entry point (`crates/server/src/lib.rs:108`). |
| C2 Reproduction (red pre-fix) | PASS | In a scratch base checkout with only the added test, all three tests compiled and failed on the expected `recover()` errors at `crates/server/tests/gateway_recover_totality.rs:195`, `:263`, and `:317`. |
| C3 Change | PASS | The scoped decision is implementable through the one inode scan, counter seeding, recovery callsite, and end-to-end test without reopening allocator or paging design (`crates/core/src/metadata.rs:2121`; `crates/server/src/cli.rs:1704`; `crates/server/src/lib.rs:133`). |
| C4 Verification (red→green) | PASS | The same three assertion-red tests became 3/3 green with the production patch; the unchanged legacy recovery test and full `cargo xtask ci`, including real `cargo-deny`, also passed (`crates/server/tests/gateway_recover_totality.rs:195`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Rebuild must add and assert attribution for a structurally valid segmented root — it decodes successfully, the only audit call is under `if let Err`, and criterion 2 discards `_audit`, leaving the briefed attribution case unproved (`crates/core/src/metadata.rs:2142`; `crates/server/tests/gateway_recover_totality.rs:272`). |
| T1 Structure | PASS | The four-file slice preserves crate boundaries, keeps the stable public recovery seam, and gives the new test root the required unsafe prohibition (`crates/server/tests/gateway_recover_totality.rs:34`). |
| T2 Shape | PASS | The patch is 45,113 bytes and approximately 303 added nonblank/non-comment lines across four files, below both scope limits, with no added paging, allocator, or forbidden chunk-floor apparatus. |
| T3 Runtime | PASS | The decision-relevant path retains one consistent-cut inode scan and performs a direct guarded repair; focused green recovery completed in 0.07s and the full runtime suite passed (`crates/core/src/metadata.rs:2123`; `crates/server/src/cli.rs:1735`). |
| T4 Contribution | NEEDS-HUMAN | Human must inspect or obtain the five reported batch-review blockers before treating contribution review as complete — exact-path merged/closed prior art was independently checked, but the driver-only `scripts/review-branch` tool and its report were unavailable to rerun or triage. |
| T5 Judgment | PASS | No additional architecture or scope decision is owed: chunk-floor deletion is Plan-settled and paging/allocator work is tracked in #687; the remaining rebuild-fixable gap is isolated in C5. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Maintainer must decide whether per-row warning/audit events plus automatic counter repair are operationally sufficient for damaged-store startup — automated evidence proves availability and floor behavior, not that operators can reliably discover and repair the damage (`crates/core/src/metadata.rs:2053`). |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Rebuild must add and assert attribution for a structurally valid segmented root — it decodes successfully, the only audit call is under `if let Err`, and criterion 2 discards `_audit`, leaving the briefed attribution case unproved (`crates/core/src/metadata.rs:2142`; `crates/server/tests/gateway_recover_totality.rs:272`).
- [ ] T4 Contribution — Human must inspect or obtain the five reported batch-review blockers before treating contribution review as complete — exact-path merged/closed prior art was independently checked, but the driver-only `scripts/review-branch` tool and its report were unavailable to rerun or triage.
- [ ] Validation — fitness-to-purpose — Maintainer must decide whether per-row warning/audit events plus automatic counter repair are operationally sufficient for damaged-store startup — automated evidence proves availability and floor behavior, not that operators can reliably discover and repair the damage (`crates/core/src/metadata.rs:2053`).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_652/review-b
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
- Iteration delta (if iterating): Auto-iterate (round 3): rebuilding for the implementation-level findings — C5 Causal adequacy — Rebuild must add and assert attribution for a structurally valid segmented root — it decodes successfully, the only audit call is under `if let Err`, and criterion 2 discards `_audit`, leaving the briefed attribution case unproved (`crates/core/src/metadata.rs:2142`; `crates/server/tests/gateway_recover_totality.rs:272`).; T4 Contribution — Human must inspect or obtain the five reported batch-review blockers before treating contribution review as complete — exact-path merged/closed prior art was independently checked, but the driver-only `scripts/review-branch` tool and its report were unavailable to rerun or triage.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_652/review-b. 2 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-08-02

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
