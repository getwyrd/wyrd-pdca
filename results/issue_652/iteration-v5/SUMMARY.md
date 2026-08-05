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
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): unverifiable — gate exceeded its 7200s timeout and was killed (no verdict — re-run it, or raise the check's timeout_secs / [gates] defa
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it.
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 15 mutants tested in 2m: 6 caught, 9 unviable

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

Reviewing issue #652’s startup-recovery change: tolerate damaged inode metadata, page the recovery walk, preserve safe inode allocation, and remove the unused chunk-ID floor.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief makes damage totality, bounded paging, audit attribution, dead-floor deletion, and the legacy regression falsifiable through the stable recovery entry point at `crates/server/src/lib.rs:139`. |
| C2 Reproduction (red pre-fix) | PASS | An independent base-plus-test run produced assertion RED (0/7), including damaged-record and scan-refusal failures at `crates/server/tests/gateway_recover_totality.rs:284` and `crates/server/tests/gateway_recover_totality.rs:532`. |
| C3 Change | NEEDS-HUMAN | The Plan owner must authorize the allocator-repair and living-architecture expansion or return the slice to Plan—the brief limits the implementation surface and expressly excludes docs paragraphs, while the patch substantively changes `crates/server/src/cli.rs:1634` and `docs/design/architecture/08-crosscutting-concepts.md:85`; that changes scope, prior-art, and conflict assumptions. |
| C4 Verification (red→green) | NEEDS-HUMAN | Sign-off must obtain a complete `cargo xtask ci`/`cargo deny` result—isolated red→green, the legacy regression, core units, fmt, relevant clippy, typos, docs lint/render, and machete passed, but the supplied full gate timed out after 7200s and the local deny rerun hit a read-only advisory-database lock, leaving workspace CI/conformance/security provisional (`crates/server/tests/gateway_recover_totality.rs:284`). |
| C5 Causal adequacy | PASS | The patch removes the two stated failure mechanisms rather than probing or guarding an optional capability: the floor is taken before value decoding and the namespace is traversed with `scan_page` at `crates/core/src/metadata.rs:2237`, while the dead chunk mark has no caller at `crates/server/src/lib.rs:140`. |
| T1 Structure | PASS | The recovery walk remains in core metadata, composition remains at `Gateway`, backend access stays behind `MetadataStore`, and the signature-stable acceptance test is correctly isolated as a server integration target (`crates/core/src/metadata.rs:2140`, `crates/server/tests/gateway_recover_totality.rs:45`). |
| T2 Shape | PASS | The patch remains within the 15-file and approximately 1,500-semantic-line limits, confines the signature ripple to the one caller, and leaves no `_max_chunk` or scavenging apparatus in `crates/` (`crates/server/src/lib.rs:140`, `crates/core/src/metadata.rs:2237`). |
| T3 Runtime | FAIL | Startup can still remain indefinitely pre-serving under sustained metadata conflicts because `seed_next_inode_floor` retries `CommitOutcome::Conflict` forever with neither retry budget nor backoff; this violates the standing bounded-await rule and keeps `Gateway::recover` from reaching a result (`crates/server/src/cli.rs:1852`). |
| T4 Contribution | NEEDS-HUMAN | The maintainer must complete prior-art and review triage for the newly affected CLI and architecture paths—the recorded path search covered only metadata/lib, while the batch-review gate reports five untriaged blockers; without that decision the contribution is not review-complete (`crates/server/src/cli.rs:1634`, `docs/design/architecture/08-crosscutting-concepts.md:85`). |
| T5 Judgment | NEEDS-HUMAN [impl] | Rebuild must close the strict-grammar test gap: Rust’s parser accepts `inode:+7` and `inode:007`, so those damaged keys evade the promised repair attribution because `parse_inode_key` still uses `.parse()` and the acceptance test covers only `inode:not-a-number` (`crates/core/src/metadata.rs:2040`, `crates/server/tests/gateway_recover_totality.rs:290`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | The maintainer must accept that paging bounds each materialization but not total startup time, and decide whether the newly added damaged-counter repair semantics fit this slice’s operational rollout; that choice controls availability and migration risk (`crates/core/src/metadata.rs:2201`, `crates/server/src/cli.rs:1823`). |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C3 Change — The Plan owner must authorize the allocator-repair and living-architecture expansion or return the slice to Plan—the brief limits the implementation surface and expressly excludes docs paragraphs, while the patch substantively changes `crates/server/src/cli.rs:1634` and `docs/design/architecture/08-crosscutting-concepts.md:85`; that changes scope, prior-art, and conflict assumptions.
- [ ] C4 Verification (red→green) — Sign-off must obtain a complete `cargo xtask ci`/`cargo deny` result—isolated red→green, the legacy regression, core units, fmt, relevant clippy, typos, docs lint/render, and machete passed, but the supplied full gate timed out after 7200s and the local deny rerun hit a read-only advisory-database lock, leaving workspace CI/conformance/security provisional (`crates/server/tests/gateway_recover_totality.rs:284`).
- [ ] T4 Contribution — The maintainer must complete prior-art and review triage for the newly affected CLI and architecture paths—the recorded path search covered only metadata/lib, while the batch-review gate reports five untriaged blockers; without that decision the contribution is not review-complete (`crates/server/src/cli.rs:1634`, `docs/design/architecture/08-crosscutting-concepts.md:85`).
- [ ] T5 Judgment — Rebuild must close the strict-grammar test gap: Rust’s parser accepts `inode:+7` and `inode:007`, so those damaged keys evade the promised repair attribution because `parse_inode_key` still uses `.parse()` and the acceptance test covers only `inode:not-a-number` (`crates/core/src/metadata.rs:2040`, `crates/server/tests/gateway_recover_totality.rs:290`).
- [ ] Validation — fitness-to-purpose — The maintainer must accept that paging bounds each materialization but not total startup time, and decide whether the newly added damaged-counter repair semantics fit this slice’s operational rollout; that choice controls availability and migration risk (`crates/core/src/metadata.rs:2201`, `crates/server/src/cli.rs:1823`).
- [ ] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) unverifiable — gate exceeded its 7200s timeout and was killed (no verdict — re-run it, or raise the check's timeout_secs / [gates] defa
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_652/review-b
- [ ] size backstop — this slice is behaving oversized: patch is 109 KB (threshold 100 KB); 4 round(s) already spent (threshold 2). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.
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
- Iteration delta (if iterating): Rebuild must fix 3 concrete bugs surfaced by this round's review, not re-litigate scope: 1. Unbounded retry / hang: `seed_next_inode_floor` (crates/server/src/cli.rs:1852) retries CommitOutcome::Conflict forever with no retry budget or backoff (T3 Runtime FAIL) — this is suspected to be the actual cause of the C4 `cargo xtask ci` gate timing out at 7200s, not infra flakiness. Add a bound/backoff so `Gateway::recover` always reaches a result. 2. Unsafe damaged-counter reseed logic (cli.rs:1851, :1869, :1870): recomputing the counter from committed `inode:` keys after a damaged counter can reuse an ID whose fragments are still live under an `orphan:` grace record, or rewind past an ID already handed to a concurrent in-flight allocator — allowing collision / double-mint. Needs a reseed strategy that cannot regress below any live or in-flight id. 3. Lenient `parse_inode_key` (crates/core/src/metadata.rs:2190, :2198): `.parse()` accepts non-canonical forms (`inode:+7`, `inode:007`, `inode:+18446744073709551615`), so damaged- looking keys are treated as valid instead of being flagged/attributed, and can even falsely exhaust the allocator. Tighten the parse to the canonical grammar and extend the acceptance test beyond `inode:not-a-number` to cover these forms. Scope note: the human confirmed the cli.rs / docs-adjacent surface (allocator-repair expansion) was already approved in a prior iteration of this bundle — not a new scope violation, so this is NOT being sent back to Plan. Proceed with iterate-do, not iterate-plan.
- By / date: Eduard Ralph / 2026-08-02

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
