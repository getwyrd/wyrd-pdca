# Brief — issue 652 / recovery-total-over-damage

> **Re-plan, 2026-08-04.** This bundle returned to Plan at the v7 sign-off after seven Do rounds
> (patch 32 KB → 140 KB). The diagnosis was *two invariants, one briefed*. The paging half —
> and the allocator-safety work it dragged in — is now **#687**, filed 2026-08-04 on the
> 0.1 Alpha milestone. This brief is the remaining half, which is what issue #652's own body
> asks for. Slice 5 of the #635 chain. History: https://github.com/getwyrd/wyrd/issues/635

- **Slug:** recovery-total-over-damage
- **Defect:** `metadata::high_water_marks` is what `Gateway::recover` runs **before the gateway
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
- **Success criterion:** The added test target `crates/server/tests/gateway_recover_totality.rs`
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
- **Falsifiability:** RED is an **assertion** red on base-visible symbols. `C4-verify` resets
  `../wyrd-verify` to `origin/main` (no `Onto branch`; `wave_mode = "merge"` leaves no folded
  branch, so `_resolve_base_ref` falls through to the brief's base, `run-verify.sh:202-208`).
  Verified at Plan by dry-running the classifier on the planned file set: one discriminator,
  `ADDED_TEST crates/server/tests/gateway_recover_totality.rs`, plus `CRATE crates/core` and
  `CRATE crates/server`. The RED leg keeps the test, reverts all three production files
  (`run-verify.sh:420-431`) and runs `cargo test -p wyrd-server --test
  gateway_recover_totality`; on that tree criteria 1–3 all fail with `recover()` returning `Err`.
  **Driving `Gateway::recover()` rather than `high_water_marks` is load-bearing**: this patch
  changes `high_water_marks`'s signature, so a test naming it would *compile*-fail on the red leg
  and be scored as a pass. Plain Linux workspace; no `crates/server/tests/*.rs` carries a
  crate-level `cfg` (checked at Plan, all 39 files), no feature flag, no topology, no new
  dev-dependency — the composition is the one `s3_http_wire.rs:679-686` already drives — so
  neither the vacuous `0 tests … ok` branch (`run-verify.sh:434-440`) nor a
  compile-red-scored-as-pass can occur.
- **Invariant to restore:** **C-1 — no permanent or data-losing failure mode is an acceptable
  cost** (`docs/principles.md` §6 *Storage lifecycle / reclamation* → §5 C-1; maintainer's rule
  2026-07-25; `0016:2802-2813`; `../wyrd/crates/custodian/src/gc.rs:22-25`). Over this slice's
  category — **startup recovery, the step that runs before anything is served**:
  - **Recovery is total over stored content.** No arrangement of records it cannot read may make
    it refuse. A gateway that will not start is a state the store's own contents put it in and
    nothing but manual repair exits. One record's fault is **contained**: attributed, with the
    walk and every other object's availability continuing — the doctrine `gc.rs:22-31` already
    states for the same namespace.
  - **A floor an allocator trusts is never a quiet under-approximation.** Any id mark recovery
    produces must be ≥ every id whose bytes still exist. The two legal answers for a record it
    cannot read are *fail closed for that record* or *contribute its true floor* — **never zero**.
  - **A number nobody reads is not a safety property** — the right way to satisfy the rule above
    for a mark with no consumer is to remove the mark, not to compute it more carefully.
- **Repo + branch target:** getwyrd/wyrd @ main   (resolved and verified at Plan 2026-08-04:
  `git -C ../wyrd fetch origin` then `log origin/main` → `d50f0ca`, unchanged since the previous
  brief. Carries #648 (PR #672) and #649/#650 (PR #683), so `high_water_marks` is in its
  post-#648 form — which is what makes defect (1) live.)
- **Depends on:** *(none — its only code dependency, #648, is merged to `main`)*
- **Conflicts with:** 651, 682
- **Ordering note:** Both are shared-file conflicts with no dependency either way, so they belong
  in **different waves** rather than built blind on one base. **#651** (in `ITERATE_DO`) edits
  `crates/server/src/cli.rs` at `cmd_custodian` (`:1193`, `:1261`, ~+100 lines) while this slice
  edits `seed_next_inode_floor` (`:1691`) — different regions, but #651's insertion shifts every
  line below it. **This is a change from the previous brief**, which declared no #651 conflict
  because this slice did not then touch `cli.rs`. **#682** (no bundle yet) adds `repoint_chunk`
  and the ceiling helpers to `crates/core/src/metadata.rs`, which this slice rewrites.
  `Depends on:` is empty: #648 is already on `main`, and under `wave_mode = "merge"` every wave
  builds off `origin/main`. **#687 is downstream** of this slice — it builds on what ships here.
- **Surfaces:** data
- **Difficulty:** low
- **Scope:** make **startup recovery total over content it cannot read**, and remove the id-floor
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
- **Budget:** ≤ ~450 added semantic lines (non-blank, non-comment) across ≤ 5 files. v1 of the
  previous attempt was `+113/-103` in `metadata.rs` with **zero** `cli.rs`, and that half was
  judged sound at sign-off; this brief is that shape plus a small `cli.rs` containment and one
  more criterion. **If the patch approaches 100 KB or reaches into `alloc_inode`, STOP and hand
  back** — that is the previous failure re-running, and it is #687's work, not this slice's.
- **Repro instruction:** `git -C ../wyrd show origin/main:crates/core/src/metadata.rs` —
  `high_water_marks` at `:2073`, `store.scan(b"inode:")` at `:2077`, `decode(&value)?` at
  `:2081`, the segmented refusal at `:2082-2087`, the two further scans at `:2094`,`:2105`;
  `.../crates/server/src/lib.rs` — `recover` at `:123` discarding the chunk mark at `:124`;
  `.../crates/server/src/cli.rs:1696` — the counter parse. To see defect (1) directly: seed a
  store with a healthy object plus one raw undecodable `inode:` value and call
  `Gateway::recover()` — it returns `Err` and the gateway will not serve.
- **External dependencies:** `cargo-deny`
  — needed by the gating C4-ci row: cargo xtask ci's ADR-0003 dependency wall passes --config at
  the root, which cargo-deny 0.19.x rejects outright. It cost this bundle a NEEDS-HUMAN item last
  cycle and is now registered as a `[[doctor.checks]]` row with id = "cargo-deny" (pdca.toml:733),
  so it preflights. Nothing else beyond the base Rust toolchain: no Docker,
  no protoc, no live backend, no new dev-dependency, and no docs paragraph, so the prose gates
  are not in play for this bundle.
- **Test file:** `crates/server/tests/gateway_recover_totality.rs` — a **NEW** file. This
  project's C4 discriminator is an **added** `*/tests/*.rs` (`run-verify.sh:93` `_is_test_file`,
  `:91` `_added_files`); an appended or co-located test degrades to the green-only branch
  (`:408-417`) and proves nothing. Placed at the `wyrd-server` level on purpose so it drives the
  signature-stable `Gateway::recover()` rather than the signature-changing `high_water_marks` —
  see *Falsifiability*. `crates/server/tests/s3_http_wire.rs:666-700` is the precedent for
  constructing the gateway and writing raw metadata keys. Co-located `metadata.rs` unit tests may
  ship **in addition** — `C4-ci` covers them.
- **Verification posture:** default — assertion-red on `origin/main`, green with this patch, both
  at Check. Nothing deferred: every binding criterion is exercised by the new target this cycle.
- **Citations expected:** cite `path:line` on the target branch for every change. **The peer
  callsite to mirror is `crates/custodian/src/gc.rs:360-385`** — `referenced_fragments` walks the
  **same `inode:` namespace** through `scan`, and on a decode failure records the fault against
  the key and `continue`s (`:378-382`) instead of `?`. Contain the same way, for the reason its
  module doc gives at `:22-31` ("the one object's fault is contained: it is attributed, and the
  walk — and every other object's protection — continues"). Other peers Do MAY open: **the
  minting scheme that makes the chunk mark dead** — `crates/server/src/lib.rs:229-263` and
  `crates/server/src/cli.rs:1716-1723`; **what must not regress** —
  `crates/server/tests/s3_http_wire.rs:645-700` and `crates/server/src/cli.rs:1691`. **Salvage:**
  `results/issue_652/iteration-v1/patch.diff` (this bundle, permitted input) carries the sound
  early shape of the `metadata.rs` rewrite. Take the containment structure only —
  **leave every paging symbol behind** (`for_each_page`, `RECOVERY_PAGE`) and the whole floor
  apparatus (`RecoveredIds`, `ClassIds`, `torn_digit_escape`, `json_string_token`,
  `scavenged_chunk_id_floor`, `segment_chunk_floor`).
- **Prior-art check (triage cycles):** searched by affected file path across merged history, open
  and closed PRs, plus commit archaeology on both functions; re-run at Plan 2026-08-04.
  `crates/core/src/metadata.rs` (10 PRs) and `crates/server/src/lib.rs` (19 PRs): **#647 is the
  only one touching this concern and it is CLOSED unmerged** — on reviewability, not correctness.
  Every other PR is MERGED and none touches this walk's totality; no PR references #652
  (`gh pr list --search 652` → empty). The chunk mark and its discard trace to the **same merged
  commit**, `fdd34f1` (#487): `log -S"_max_chunk"` and `-S"random_chunk_epoch"` each return
  exactly it. The only production caller of `high_water_marks` is `lib.rs:124`, so the signature
  narrowing ripples nowhere else.
  **Do-not-re-earn (standing rejections; content-stable — they bind wherever the finding
  re-lands, not at a line):** (i) *caller-side fan-out timeout* — rejected 3× across #508/#636:
  the `ChunkStore` implementation owns the network bound, not the caller; (ii) *retraction of
  already-published bytes* — rejected 4× in #638 on unchanged evidence; (iii) *"`Completed`
  releases its admission slot"* — withdrawn as unsatisfiable; (iv) **that the chunk-id floor
  should be restored, re-derived or wired** — settled DELETE, see *Scope*; answer with that
  decision, do not rebuild the apparatus; (v) **that this slice should page, bound the walk, or
  guard `alloc_inode`** — that is **#687** by an explicit Plan decision after seven rounds;
  answer any such finding by citing #687, not by widening this patch. Do MUST record each
  rejection in `review-rejected.md` **at every line the finding is reported at**.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR MAY
happen during the cycle. The PR MUST NOT be marked ready before sign-off accepts.

## Iteration 8 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 3): rebuilding for the implementation-level findings — C5 Causal adequacy — Rebuild must add and assert attribution for a structurally valid segmented root — it decodes successfully, the only audit call is under `if let Err`, and criterion 2 discards `_audit`, leaving the briefed attribution case unproved (`crates/core/src/metadata.rs:2142`; `crates/server/tests/gateway_recover_totality.rs:272`).; T4 Contribution — Human must inspect or obtain the five reported batch-review blockers before treating contribution review as complete — exact-path merged/closed prior art was independently checked, but the driver-only `scripts/review-branch` tool and its report were unavailable to rerun or triage.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_652/review-b. 2 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — ERROR cargo test failed in an unmutated tree, so no mutants were tested
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_652/review-b
- Full previous attempt preserved in `iteration-v8/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 9 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 4): rebuilding for the implementation-level findings — T4 Contribution — Human must obtain and triage the two blockers reported by the driver-only batch review — its `review-branch` tool and report are absent here, so contribution review remains provisional despite independently clean exact-path prior art and contribution metadata.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_652/review-b. 2 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_652/review-b
- Full previous attempt preserved in `iteration-v9/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 10 — carry-forward (from the previous attempt)
- Sign-off rationale: Converging, and the split (#687) was recent — the remaining gap is triage bookkeeping, not substance. Every substantive gate is green (C4-ci, C4-verify red->green 0/4 -> 4/4, C5-mutants 8/8 caught-or-unviable) and the advisory reviewer PASSed C1-C5 and T1-T3. Do NOT rework the patch shape; it is what the brief's Success criterion prescribes. The only work this round is to make T4-batch-review's two findings triage cleanly. Verified against the gate's own logic (`load_rejected` / `is_rejected` in `scripts/review-branch`): a finding is suppressed only on an exact three-way match of `file:line` + CLASS + a MATCH phrase that is a case-insensitive substring of the finding's ACTUAL rationale text. 1. `crates/server/src/cli.rs:1762` | TEST-GAP | "The new destructive, contention-sensitive repair path has only redb integration coverage and lacks the rubric-required seeded Tier-0 DST regression." This is a pure line-anchor miss — the substance is already recorded and its reasoning stands. TEST-GAP entries with MATCH `tier-0 dst` exist at cli.rs:1745, cli.rs:1717 and gateway_recover_totality.rs:315/364/486/487, and that phrase DOES match this rationale; they are simply anchored at the wrong line. The entry Do wrote for it at cli.rs:1747 fails twice over: wrong line, and its MATCH phrase ("a new destructive or concurrent path lands with seeded Tier-0 DST coverage (`crates/dst`)") does not occur in the reviewer's wording. Fix: record it at cli.rs:1762 as class TEST-GAP with a MATCH phrase lifted verbatim from the finding's own rationale (e.g. `tier-0 dst`), carrying the existing rejection reason (the crates/dst location costs a ~250-line cross-crate move of the persisted-allocator protocol out of wyrd-server, which cannot compile under --cfg madsim). 2. `crates/server/src/cli.rs:1745` | CONVENTION | "Changing `meta:next_inode` from fail-on-corruption to audit-and-repair alters persisted-field semantics without updating the living architecture documentation." Genuinely NEW — never answered in any round. No CONVENTION entry anywhere has a MATCH phrase occurring in this rationale (the ones at that very line carry `scan_page` / `paging` / `snapshot isolation` / `alloc_inode`; the rest are all unbounded-await findings). Answer it with a recorded rejection, NOT a code or docs change: the brief puts "**any docs paragraph**" out of scope explicitly, and assigns the living-architecture edits to #648, #649-#651 and #653. Anchoring discipline for this round: take the MATCH phrase verbatim out of the finding text in review-batch.md rather than paraphrasing it, and record each rejection at the line THIS round's review-batch.md reports it at. Two rounds have now been spent on rejections that were correct in substance but missed on line or phrase. The three judgment items in SUMMARY.md section 6 (T4 contribution, T5 DST coverage, fitness-to-purpose) are NOT cleared and remain open for the human at the next sign-off; they are not defects for Do to chase. The two stale section 6 entries carried in deferred-findings.json — "leaf produced no usable verdict" and the cargo-deny external dependency — are answered by this round's own evidence (check-review.md is populated; C4-ci is green) and should not drive any work.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_652/review-b
- Full previous attempt preserved in `iteration-v10/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
