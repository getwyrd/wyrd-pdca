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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 16 mutants tested in 2m: 1 missed, 5 caught, 10 unviable

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

Review of issue #652: make `Gateway::recover()` total over damaged metadata, page-bounded over large stores, and remove the unused chunk-id recovery floor.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance contract is executable and signature-stable: startup must preserve healthy reads, attribute damage, seed above key-derived inode ids, avoid capped `scan`, and delete the unused chunk-floor path (`crates/server/src/lib.rs:155`). |
| C2 Reproduction (red pre-fix) | PASS | With only the production fix stashed, the added target compiled and all 7 tests failed by assertion through `Gateway::recover()`, covering both binding damage and size failures (`crates/server/tests/gateway_recover_totality.rs:443`). |
| C3 Change | PASS | The five-file change stays within the accepted recovery/allocator/docs surface and the brief's 15-file/~1,500-semantic-line budget while preserving the public recovery signature (`crates/server/src/lib.rs:155`). |
| C4 Verification (red→green) | PASS | The identical target changed from 0/7 passing pre-fix to 7/7 passing post-fix, and an independent `cargo xtask ci` completed with all checks passed (`crates/server/tests/gateway_recover_totality.rs:682`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Add a regression that distinguishes counter contention from an occupied inode: the reproduced `&&`→`\|\|` survivor bypasses backoff and falsely enters recovery correction on every reserved-counter conflict, so the claimed contention/load behavior is unbound (`crates/server/src/cli.rs:1865`). |
| T1 Structure | PASS | Canonical persisted-number parsing and the recovery audit seam remain core-owned while server composition continues through the unchanged `MetadataStore` boundary (`crates/core/src/metadata.rs:2077`). |
| T2 Shape | PASS | One bounded paging helper feeds one inode-mark walk, replacing three complete scans and the discarded tuple half without leaving dead recovery symbols or callsites (`crates/core/src/metadata.rs:2158`). |
| T3 Runtime | PASS | Real redb/filesystem composition plus seam-faithful scan refusal, permanent conflict, and behind-cursor fixtures all pass, exercising damage containment and bounded retry at runtime (`crates/server/tests/gateway_recover_totality.rs:787`). |
| T4 Contribution | NEEDS-HUMAN | Inspect and disposition the four blocking findings reported by the driver batch review—its wrapper/log is absent from the allowed artifacts, so those findings cannot be independently reproduced; affected-path prior art was mechanically rechecked and the only closed overlap is PR #647. |
| T5 Judgment | PASS | The patch removes the eager failure causes rather than adding a capability probe: marks are taken before value decoding and low non-snapshot floors are contained at the atomic allocator guard (`crates/core/src/metadata.rs:2305`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide the acceptable production startup-latency envelope—paging bounds resident rows and prevents `SCAN_CAP` refusal, but recovery still performs `ceil(inodes / 128) + 1` pre-service round trips, so namespace-scale time remains deployment-dependent (`crates/core/src/metadata.rs:2249`). |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Add a regression that distinguishes counter contention from an occupied inode: the reproduced `&&`→`\
- [ ] T4 Contribution — Inspect and disposition the four blocking findings reported by the driver batch review—its wrapper/log is absent from the allowed artifacts, so those findings cannot be independently reproduced; affected-path prior art was mechanically rechecked and the only closed overlap is PR #647.
- [ ] Validation — fitness-to-purpose — Decide the acceptable production startup-latency envelope—paging bounds resident rows and prevents `SCAN_CAP` refusal, but recovery still performs `ceil(inodes / 128) + 1` pre-service round trips, so namespace-scale time remains deployment-dependent (`crates/core/src/metadata.rs:2249`).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_652/review-b
- [ ] size backstop — this slice is behaving oversized: patch is 137 KB (threshold 100 KB); 6 round(s) already spent (threshold 2). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.
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
- Outcome: iterated-to-Plan
- Iteration delta (if iterating): REJECTED because the slice now carries two invariants and only one of them was ever briefed. WHAT IS SOUND AND SHOULD SURVIVE THE SPLIT (metadata.rs + server/src/lib.rs): `high_water_marks` returning `InodeId` alone; deletion of the dead chunk-id mark and the `pending:`/`orphan:` walks that fed it; `for_each_page`; `InodeValueReading` + `read_inode_value` with the id taken from the key BEFORE the value is inspected; the `RECOVERY_AUDIT` seam; `parse_canonical_u64` promoted to a single shared grammar for both `inode:` keys and `meta:next_inode`. This half has been stable since ~v2 and is close to done. WHY IT IS GOING TO PLAN RATHER THAN ANOTHER DO ROUND: The brief scopes two production files (`crates/core/src/metadata.rs`, `crates/server/src/lib.rs`). `crates/server/src/cli.rs` entered in three steps of unequal legitimacy: 1. (v3) `seed_next_inode_floor`'s read of `meta:next_inode` — legitimate: it is literally the second half of `Gateway::recover`, and leaving it fail-loud would give away the totality property the brief exists to establish. In scope by invariant, if not by file list. 2. (v5) bounding that function's CAS retry (`SEED_FLOOR_ATTEMPTS`) — defensible: "total AND bounded" is the brief's own title and an unbounded retry inside `recover` is a gateway that never starts. 3. (v6->v7) the `require_absent(inode_key(id))` guard, conflict-disambiguation `get`, and counter-stepping loop inside `alloc_inode` — NOT justified by the recovery invariant. `alloc_inode` is not called by `Gateway::recover`; it is the PUT path. It was touched purely to compensate for the fact that switching `scan` -> `scan_page` gives up snapshot consistency, so the seeded floor can sit below a live id. Step 3 is where the size went (32 KB at v1 -> 140 KB now, crossing 100 KB at v5) and where every new finding has landed: hang/retry (v5), unsafe reseed (v5), lenient parsing (v5), snapshot race (v6), TOCTOU x4 (v7). Rounds spent: 6, against a threshold of 2. THE ROOT CAUSE IS A MISSING SPEC, NOT A MISSING FIX: The `cli.rs` allocator surface has never been briefed. It arrived via a Do-round carry-forward and was ratified at the v4 sign-off, but no brief ever stated its invariant, its acceptance criterion or its budget. There is therefore nothing for a rebuild to close against, which is exactly why each round yields a NEW finding rather than converging on an old one. A Do round can satisfy a criterion; it cannot author one. Reinforcing this: the patch's own doc comments concede that the collision hazard the `alloc_inode` guard defends against is largely PRE-EXISTING and unclosable by this slice — "the peer never reads the counter this mark seeds, so it keeps allocating after any walk ends ... a floor derived from stored keys is stale the instant it is computed, at any price in round trips." Paging opens only a narrower new hole (keys committed behind the cursor during the walk itself). So a partial guard is being asked to carry an older, larger problem, and reviewers correctly keep finding the parts it does not cover: the cluster path writes fragments BEFORE the metadata commit, so `require_absent` at reservation time does not hold the id across the operation (review-batch cli.rs:1856/:1857), and the guard is skipped entirely in the `Absent` branch (cli.rs:1812) on a justification that answers metadata collision while the stated hazard is fragment overwrite. DIRECTION FOR THE RE-PLAN — three options, all Plan-level; option 1 in particular requires a brief because it drops a stated success criterion: 1. Do not page in this slice. Keep `scan`, ship damage-containment totality + the dead-floor deletion only. The snapshot regression disappears and with it the entire reason to touch `alloc_inode`; defect (2) ("a store merely too large to scan") becomes its own slice. This collapses the patch back toward v2 size and is the cheapest convergent path. 2. Page, and file the inode-allocator collision safety as its own issue. Ship the recovery half, stating plainly in the PR that the floor is a starting point and that the rolling-upgrade re-mint hazard is tracked separately. Only honest if the narrow new hole paging opens is acceptable standalone. 3. Page, and close the window properly here. Needs a genuine reservation — a reserving record, or reordering the cluster path's metadata commit ahead of its fragment writes. Well past this bundle's budget and likely ADR territory. Whichever split is authored, the second child (allocator collision safety) needs its own stated invariant — "the allocator never hands out an id that is live under a concurrent legacy peer" — and its own acceptance criterion covering the fragment-write window and the absent-counter case. Also carry forward: the surviving C5 mutant (`&&`->`||` at cli.rs:1865) shows the new allocator branch is not pinned by tests to the degree its risk warrants. Note for the re-plan: the "leaf produced no usable verdict" and "review-branch absent" §6 items look like reviewer-sandbox artifacts (the driver's own gates ran for real), and the cargo-deny external-dependency item is answered by the driver's green C4-ci.
- By / date: Eduard Ralph / 2026-08-02

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
