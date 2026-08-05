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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 13 mutants tested in 2m: 4 caught, 9 unviable

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

Review of issue #652: make gateway startup recovery total over damaged metadata, page-bounded past scan caps, and remove the unused chunk-id recovery floor.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance contract is falsifiable through the signature-stable gateway entry point and separately identifies damaged records, scan-cap refusal, dead-floor removal, and the legacy regression; it declares no external dependency. |
| C2 Reproduction (red pre-fix) | PASS | Keeping the added public-entry target while stashing production changes compiles and assertion-fails 0/5 on the base, so the red is behavioral rather than a signature artifact (`crates/server/tests/gateway_recover_totality.rs:25`). |
| C3 Change | PASS | The accepted recovery scope is closed at both startup steps: the inode walk contains value damage and the allocator seed contains counter damage without restoring the settled dead chunk floor (`crates/core/src/metadata.rs:2255`, `crates/server/src/cli.rs:1856`, `crates/server/src/lib.rs:145`). |
| C4 Verification (red→green) | PASS | The acceptance target changes from assertion-red 0/5 to green 5/5, the unchanged legacy recovery regression passes, and fmt/clippy/build/workspace tests/docs/DST/conformance plus all dependency-wall checks pass; the host advisory-lock fault was discharged with a scratch-local database (`crates/server/tests/gateway_recover_totality.rs:312`). |
| C5 Causal adequacy | PASS | The change removes the fail-loud decode and capped whole-scan causes, preserves every recoverable key floor, and deletes rather than probes around the consumerless chunk mark (`crates/core/src/metadata.rs:2167`, `crates/core/src/metadata.rs:2255`). |
| T1 Structure | PASS | Recovery remains split across the core metadata walk and server allocator seam, while one shared canonical-decimal parser prevents the key and counter grammars from drifting (`crates/core/src/metadata.rs:1322`, `crates/server/src/cli.rs:1691`). |
| T2 Shape | PASS | The sole production callsite matches the narrowed `Result<InodeId>` shape, and the added acceptance target exercises only the unchanged `Gateway::recover() -> Result<()>` surface (`crates/server/src/lib.rs:145`, `crates/server/tests/gateway_recover_totality.rs:25`). |
| T3 Runtime | PASS | Real-redb recovery crosses multiple bounded pages, survives `scan` refusal, preserves healthy bytes, and terminates after bounded counter conflicts; the relevant runtime target passes 5/5 (`crates/server/tests/gateway_recover_totality.rs:550`, `crates/server/tests/gateway_recover_totality.rs:655`). |
| T4 Contribution | NEEDS-HUMAN | Human must inspect and classify the supplied batch-review gate's two reported blockers — its detailed log and `scripts/review-branch` runner were absent, so that red is provisional; independently, affected-path history found #647 as the sole closed-unmerged overlap and the contribution checker passed. |
| T5 Judgment | PASS | Leaving an unreadable allocator counter unchanged avoids re-minting ids held by in-flight writes or orphan grace records, while failing only fresh allocation closed; the end-to-end test binds that containment (`crates/server/src/cli.rs:1820`, `crates/server/tests/gateway_recover_totality.rs:435`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether page-bounded memory but `O(inode namespace)` startup time is acceptable at real deployment scale — the walk intentionally visits every inode key, so a large or continually growing store has no measured end-to-end startup-latency bound (`crates/core/src/metadata.rs:2219`). |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T4 Contribution — Human must inspect and classify the supplied batch-review gate's two reported blockers — its detailed log and `scripts/review-branch` runner were absent, so that red is provisional; independently, affected-path history found #647 as the sole closed-unmerged overlap and the contribution checker passed.
- [ ] Validation — fitness-to-purpose — Decide whether page-bounded memory but `O(inode namespace)` startup time is acceptable at real deployment scale — the walk intentionally visits every inode key, so a large or continually growing store has no measured end-to-end startup-latency bound (`crates/core/src/metadata.rs:2219`).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_652/review-b
- [ ] size backstop — this slice is behaving oversized: patch is 107 KB (threshold 100 KB); 5 round(s) already spent (threshold 2). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.
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
- Iteration delta (if iterating): Rebuild on the same brief; fix the two blocking findings from this round's batch review: 1. metadata.rs:2212 / metadata.rs:2169 (BUG, blocking) — paging `high_water_marks` via independent `scan_page` calls provides no consistent namespace snapshot. A concurrent legacy writer can insert an `inode:` key behind the scan cursor after its page has already been read, so recovery under-seeds the allocator and the newly-seeded allocator can later re-mint (collide with) that still-live inode ID during a rolling upgrade. Give the paged walk a bound that cannot miss a key inserted behind the cursor during the scan (or otherwise close the gap) and add a regression that forces an insert behind the cursor mid-walk, asserting the seeded floor still exceeds it. Human note at sign-off: the size backstop in §6 (110 KB patch, round 5, threshold 100 KB / 2 rounds) suggested iterate-plan given the pattern of each round surfacing a distinct new implementation-level bug (hang/retry in v5, unsafe reseed logic, lenient parsing, and now this snapshot-consistency race) rather than the same bug recurring. The human weighed this and chose iterate-do on the basis of this round's concrete findings rather than a re-split; if another distinct implementation-shaped bug surfaces next round, iterate-plan should be reconsidered. Other §6 items not yet cleared (carry forward, not addressed by this rebuild alone): - T4 Contribution NEEDS-HUMAN — reviewer's checkout lacked the review-branch log; the driver's own gate is authoritative and did fail for real this round. - Validation / fitness-to-purpose NEEDS-HUMAN — is O(inode namespace) startup time (memory-bounded, not time-bounded) acceptable at real deployment scale? Still open, revisit at next sign-off.
- By / date: Eduard Ralph / 2026-08-02

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
