# Result — issue 652 / startup-recovery-total-and-bounded

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: `metadata::high_water_marks` is what `Gateway::recover` runs **before the gateway
  serves anything** (`crates/server/src/lib.rs:123-124` on `origin/main`), and it is neither total
  nor bounded — while half of what it computes has no consumer.
  1. **One damaged record stops the gateway from starting.** It decodes every `inode:` value with
     `?` (`crates/core/src/metadata.rs:2081`), so a single undecodable record makes `recover()`
     return `Err` and costs **every healthy object** its availability. #648 enforces the segmented
     root's structural invariants at decode, which widens the set of values that can fail it — so
     this goes from latent to live.
  2. **A store merely too large to scan stops it just as effectively.** All three walks use
     `MetadataStore::scan` (`metadata.rs:2077` `inode:`, `:2094` `pending:`, `:2105` `orphan:`),
     complete-or-fail-loud at `SCAN_CAP` (`crates/traits/src/lib.rs:286` the cap, `:275` "returns
     **no** partial `Vec`", `:288-304` the typed refusal). The bounded-page primitive that exists precisely to
     escape that cap — `scan_page`, merged as #634 / PR #645, commit `18180a2`, *"a page never
     fails with `ScanCapExceeded`: escaping that failure is the method's whole purpose"*
     (`crates/traits/src/lib.rs:1086-1087`) — is not used here.
  3. **The chunk-id half has no caller, and #647 made it silently wrong.** `Gateway::recover`
     discards it (`crates/server/src/lib.rs:124`, `let (max_inode, _max_chunk) = …`), and #647
     grew it into several hundred production lines of byte-level JSON scavenging
     (`RecoveredIds` / `ClassIds` / `torn_digit_escape` / `json_string_token`) carrying its own
     defect: a corrupted flat root such as `{"chunk_map":{"a":1},…}` fails to decode, is classed
     `Optional`, its JSON walk finds no `id` field, and it therefore reports a *complete* reading
     and contributes **0** — turning a fail-closed answer into a silently low floor, the exact
     under-approximation a recovery path must never produce.
- Success criterion: The added test target `crates/server/tests/gateway_recover_totality.rs`
  passes and binds the issue's acceptance, driven through `Gateway::recover()` — whose signature
  `(&self) -> Result<()>` is unchanged by this patch:
  1. **Total over damage.** With a healthy committed object **and** an undecodable raw `inode:<N>`
     value in the same store, `recover()` returns `Ok(())`; the healthy object still reads back
     byte-identically; a subsequent new-key PUT **commits** with an inode id strictly greater
     than `N` (the damaged record's id comes from its **key**, readable even when its value is
     not); and the damaged record is attributed on the audit seam rather than swallowed.
  2. **Total over size.** Against a metadata-store double whose `scan` returns `ScanCapExceeded`
     while `scan_page` works normally, `recover()` still returns `Ok(())` and seeds the same
     floor — i.e. recovery reads its namespaces in bounded pages, never through `scan`.
  3. **The dead half is gone** — the issue's second permitted outcome for the chunk-id floor (see
     *Scope*): `git -C ../wyrd grep -n "_max_chunk" origin/<branch> -- crates/` returns nothing,
     and no `RecoveredIds` / `ClassIds` / byte-scavenging apparatus is introduced.
  4. **No regression on the case `recover` exists for** — the existing
     `recover_seeds_the_allocator_over_a_legacy_store_without_meta_next_inode`
     (`crates/server/tests/s3_http_wire.rs:666-700`) still passes unchanged.
- Repo + branch target: getwyrd/wyrd @ main   (resolved and verified at Plan:
  `git -C ../wyrd ls-remote --heads origin main` → `d50f0ca`, matching the sandbox's
  `origin/main`. Carries #648 (PR #672) and #649/#650 (PR #683), so `high_water_marks` is in its
  post-#648 form — which is what makes defect (1) live.)
- Scope (one logical fix) / out of scope: make **startup recovery** total and bounded, and remove the id-floor half that has no
  caller. `crates/core/src/metadata.rs` — `high_water_marks` walks its namespaces in **bounded
  pages** via the `scan_page` seam instead of `scan`; an `inode:` value it cannot decode is
  **attributed and does not end the walk** (its id still recovered from its *key*).
  `crates/server/src/lib.rs` — `Gateway::recover` and the doc comments describing what recovery
  does and does not recover.

  **Plan decision on the issue's binary ("wire `max_chunk` to a real caller **or** delete it"):
  DELETE — the second outcome, which the issue's acceptance explicitly permits ("the computed
  floor has a real caller, *or the dead half is gone*"). ACCEPTED by the maintainer at Plan,
  2026-08-02 — this is settled, not Do's to revisit.**

  **The decisive fact is history, not analysis: the consumer was already removed, deliberately,
  by `fdd34f1` — "server: make gateway id allocation safe for active-active gateways" (#487,
  2026-07-08, on `main`).** That one commit did both halves:

  - *before it*, `mint_chunk_id` was `ChunkId::from(self.next_chunk.fetch_add(1, …))` — a plain
    sequential counter starting at 0 — and `recover()` **consumed** the floor:
    `let next_chunk = u64::try_from(max_chunk)…saturating_add(1); self.next_chunk.fetch_max(…)`.
    The floor was genuinely load-bearing then;
  - *after it*, the id is `(chunk_epoch << 64) | seq` with the epoch's top bit set, so every
    minted id is **≥ 2^127** (`crates/server/src/lib.rs:238-241`, `:257-263`). Disjointness
    between processes now comes from the **random per-process epoch**, not from a recovered
    floor — which is why `next_chunk_seq` is `AtomicU64::new(0)` (`lib.rs:104`) and is **never
    seeded from anything**. #487 orphaned the floor in the same commit, rewriting the callsite to
    `let (max_inode, _max_chunk) = …` (`lib.rs:124`).

  So this slice removes **dead code left behind by #487**, not a safety property: the tree has
  been running without a consumed chunk-id floor since 2026-07-08. The second minter never
  needed it either — `cli::chunk_id_minter` yields `(inode_id << 64) | seq`
  (`crates/server/src/cli.rs:1716-1723`) and `alloc_inode` returns **1** on an empty store
  (`cli.rs:1656`, `None => 1`) and only increments, so `inode_id ≥ 1` and every cluster-path id
  is **≥ 2^64**; it resumes from the persisted `meta:next_inode` counter that the **inode** mark —
  the half this slice keeps — already seeds. Nothing in the tree mints into the `< 2^64`
  in-process space the floor guards (`IN_PROCESS_CHUNK_CEILING`, `metadata.rs:2074`); the tree's
  own doc says so at `lib.rs:250-251`. Wiring would mean inventing a consumer.

  The chunk-id mark therefore goes, and with it the `pending:` and `orphan:` walks that exist only
  to compute it (`metadata.rs:2094`, `:2105`) — two fewer unbounded `scan` calls at startup, which
  serves this slice's own "total and bounded" goal. Acceptance bullet 1 (floor ≥ every live chunk
  id, `seg:` ranges included) is discharged **by construction** rather than by a stronger
  implementation.

  **Delete the standing test with it — and say why in the commit.**
  `high_water_marks_refuses_a_segmented_root_rather_than_re_mint_its_chunk_ids`
  (`crates/core/src/metadata.rs:3417`) reasons that "a segmented root read as 'owns no chunks'
  would contribute nothing to `max_chunk`, so the next PUT could mint an id that object's
  fragments already occupy". That premise **expired with #487**: the scenario needs a minter
  allocating below 2^64, and neither minter has since 2026-07-08. It is a durability-shaped test
  whose hazard is unreachable, and removing it will read as deleting a safety guard unless the
  reasoning above travels with it — state it in `build-notes.md` **and** the commit body. Its
  live half (a segmented root must not silently under-count) is not lost: it is superseded by
  criterion (1), which requires the walk to be total over records it cannot read at all.

  **Out of scope:** any change to the chunk-id **minting** scheme (ADR-0019 / #487 — settled;
  changing it is a new ADR, INTEGRATION §2/§4); the maintenance passes (#650/#651); the committer
  (#653); any new/edited ADR / spec / proposal; any conformance-vector change; **any docs
  paragraph** — the living-architecture edits belong to #648 (record shape), #649–#651
  (resolver/containment) and #653 (staged publication), and #647's sentences about deriving an
  id-allocator floor from segment records describe behaviour this slice removes.

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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 15 mutants tested in 2m: 6 caught, 9 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_652/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — scripts/pdca contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Reviewing issue #652: make gateway startup recovery tolerate damaged or scan-capped metadata, preserve the inode floor, and remove the unused chunk-id floor.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance contract is unambiguous: keep `Gateway::recover() -> Result<()>`, contain damaged inode values and scan refusal, preserve the inode floor, and delete the unused chunk floor; the stable entry point remains `crates/server/src/lib.rs:139`. |
| C2 Reproduction (red pre-fix) | PASS | On detached base `d50f0ca` with only the added target retained, all 5 tests compiled and failed by assertion through `Gateway::recover()`, including the damage and scan-refusal cases at `crates/server/tests/gateway_recover_totality.rs:249` and `crates/server/tests/gateway_recover_totality.rs:497`. |
| C3 Change | NEEDS-HUMAN | Approve Plan re-entry for allocator-counter repair/exhaustion and the architecture edit, or remove them—the brief limits production scope to `metadata.rs`/`lib.rs` and explicitly excludes docs, while this patch changes allocator semantics at `crates/server/src/cli.rs:1666` and `crates/server/src/cli.rs:1737` plus architecture text at `docs/design/architecture/08-crosscutting-concepts.md:85`. |
| C4 Verification (red→green) | NEEDS-HUMAN | Rerun `cargo deny check` where Cargo's advisory-database lock is writable—the 5-test red→green, legacy regression, typos, docs, fmt, clippy, build, and workspace tests reproduced, but the aggregate gate stopped on the host's read-only advisory lock after exercising `crates/server/tests/gateway_recover_totality.rs:249`. |
| C5 Causal adequacy | PASS | The change removes both asserted causes rather than adding a capability probe: the walk advances via `scan_page` at `crates/core/src/metadata.rs:2129`, takes the mark before decoding at `crates/core/src/metadata.rs:2212`, and the independently rerun 15-mutant set had 6 caught and 9 unviable. |
| T1 Structure | PASS | Paging and damage classification remain in metadata, the composition root only consumes the resulting floor, and the public recovery signature stays stable at `crates/server/src/lib.rs:139`; no touched rubric surface violates dependency direction, unsafe, global-state, or clock rules. |
| T2 Shape | PASS | The API now returns only its consumed `InodeId` and models value outcomes explicitly at `crates/core/src/metadata.rs:2071` and `crates/core/src/metadata.rs:2193`; `_max_chunk` and the rejected scavenging apparatus are absent from `crates/`. |
| T3 Runtime | PASS | With the patch applied, all 5 end-to-end recovery cases pass, including whole-namespace paging and fail-closed exhaustion at `crates/server/tests/gateway_recover_totality.rs:497` and `crates/server/tests/gateway_recover_totality.rs:600`, and the unchanged legacy recovery regression also passes. |
| T4 Contribution | NEEDS-HUMAN | Resolve or rerun the required batched rubric review before treating the contribution as review-complete—`scripts/review-branch` is absent from the allowed target, so its reported 4 unresolved blockers are provisional; affected-path merged history and all 10 closed-unmerged PRs were independently checked, with only PR #647 overlapping this concern. |
| T5 Judgment | NEEDS-HUMAN [impl] | The documented canonical three-reading claim is not fully enforced or tested: `parse::<u64>` at `crates/server/src/cli.rs:1678` accepts `+1` and `01` as readable `1`, so non-writer bytes evade damage attribution/reseed while the test matrix at `crates/server/src/cli.rs:2789` omits both cases. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether page-bounded memory is an adequate startup bound for production scale—the loop still walks the entire namespace to exhaustion at `crates/core/src/metadata.rs:2127`, while the runtime fixture covers only rows 100–250 at `crates/server/tests/gateway_recover_totality.rs:499`, so large or actively growing-store startup latency remains unmeasured. |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C3 Change — Approve Plan re-entry for allocator-counter repair/exhaustion and the architecture edit, or remove them—the brief limits production scope to `metadata.rs`/`lib.rs` and explicitly excludes docs, while this patch changes allocator semantics at `crates/server/src/cli.rs:1666` and `crates/server/src/cli.rs:1737` plus architecture text at `docs/design/architecture/08-crosscutting-concepts.md:85`. — Human reviewed build-notes.md §2/§3: the cli.rs allocator-counter change closes a real totality hole (a damaged persisted `meta:next_inode` counter still crashed startup, since `recover()` is two steps) surfaced by a prior review's carry-forward, not scope creep; the one-line docs deletion is truth-maintenance for a clause the fix itself falsifies. Both accepted as in-scope.
- [ ] C4 Verification (red→green) — Rerun `cargo deny check` where Cargo's advisory-database lock is writable—the 5-test red→green, legacy regression, typos, docs, fmt, clippy, build, and workspace tests reproduced, but the aggregate gate stopped on the host's read-only advisory lock after exercising `crates/server/tests/gateway_recover_totality.rs:249`.
- [ ] T4 Contribution — Resolve or rerun the required batched rubric review before treating the contribution as review-complete—`scripts/review-branch` is absent from the allowed target, so its reported 4 unresolved blockers are provisional; affected-path merged history and all 10 closed-unmerged PRs were independently checked, with only PR #647 overlapping this concern.
- [ ] T5 Judgment — The documented canonical three-reading claim is not fully enforced or tested: `parse::<u64>` at `crates/server/src/cli.rs:1678` accepts `+1` and `01` as readable `1`, so non-writer bytes evade damage attribution/reseed while the test matrix at `crates/server/src/cli.rs:2789` omits both cases.
- [ ] Validation — fitness-to-purpose — Decide whether page-bounded memory is an adequate startup bound for production scale—the loop still walks the entire namespace to exhaustion at `crates/core/src/metadata.rs:2127`, while the runtime fixture covers only rows 100–250 at `crates/server/tests/gateway_recover_totality.rs:499`, so large or actively growing-store startup latency remains unmeasured.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_652/review-b
- [ ] size backstop — this slice is behaving oversized: 3 round(s) already spent (threshold 2). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.
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
- Iteration delta (if iterating): The cli.rs allocator-counter change and the one-line docs deletion are ACCEPTED as in-scope (see §6 C3, cleared): build-notes.md shows both were reactions to a real totality gap found by a prior review round's carry-forward (recovery is two steps — the metadata walk and `cli::seed_next_inode_floor` — and a damaged persisted `meta:next_inode` counter still crashed startup before this round), not scope creep. Do not revert these in the next round unless a new reason emerges. Carry forward the remaining reviewer/adversary findings for the next Do round to close: - T4 batched rubric review: 4 blocking findings (review-branch) — resolve or address explicitly; do not treat the "review-branch absent from my checkout" caveat in the advisory review as dismissing this, the bundle's own T4 gate ran and failed for real. - T5 Judgment [impl]: `parse::<u64>` at cli.rs:1678 accepts `+1` and `01` as valid `1`, so non-writer/malformed bytes evade the damage-attribution/reseed path this round just built — and the test matrix at cli.rs:2789 doesn't cover either case. Tighten the parse to reject non-canonical digit strings and add coverage for both `+1` and `01`. - Validation / fitness-to-purpose (open question, needs an explicit answer next round, not silent deferral): page-bounded memory bounds memory but not time — the loop still walks the entire namespace to exhaustion (metadata.rs:2127), and the runtime fixture only covers rows 100-250 (gateway_recover_totality.rs:499), so large/actively-growing-store startup latency is unmeasured. Either demonstrate this is acceptable for the real deployment scale or note what bound is actually needed. - C4 Verification: reviewer's own run stopped on a host-local read-only cargo advisory-db lock during `cargo deny check`, unable to complete that leg independently — flagged only because the driver's own C4-ci gate result should be treated as authoritative here (it passed); confirm this isn't masking a real dependency-wall issue in the next round's gate run. - Note for context, not required to act on: the "leaf produced no usable verdict" and "review-branch absent from allowed target" flags in this bundle look like environment/checkout artifacts from the advisory reviewer's sandbox rather than defects in the patch — re-verify they don't recur in the next round's evidence. </content>
- By / date: Eduard Ralph / 2026-08-02

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
