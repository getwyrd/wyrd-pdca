# Brief — issue 652 / startup-recovery-total-and-bounded

> Slice **5 of 7** of the #635 re-slicing (slice 4 was split into #651/#681/#682 on 2026-08-02). Mostly **deletion** against the closed PR #647, whose
> own final adversary review called this apparatus *"computing a number nobody reads"*. History
> is on the parent issue — https://github.com/getwyrd/wyrd/issues/635.

- **Slug:** startup-recovery-total-and-bounded
- **Defect:** `metadata::high_water_marks` is what `Gateway::recover` runs **before the gateway
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
- **Success criterion:** The added test target `crates/server/tests/gateway_recover_totality.rs`
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
- **Falsifiability:** RED is an **assertion** red on base-visible symbols, and — unusually for
  this chain — does not depend on the new record shape at all. `C4-verify` resets `../wyrd-verify`
  to this bundle's base, which resolves to **`origin/main`**: this bundle has no `Onto branch` and
  no `stack-base` marker, so `_resolve_base_ref` falls through to the brief base
  (`run-verify.sh:186-192`). **Revised 2026-08-02:** the previous text expected
  `PDCA_VERIFY_BASE=origin/pdca-integration/main` for a wave>0 bundle
  (`src/pdca_harness/gates.py:423-444`). That no longer applies — the driver now runs
  `wave_mode = "merge"` (wyrd-pdca PR #198), so each wave is merged into `main` and the next
  builds off `origin/main` rather than a folded integration branch; `pdca-integration/main` has
  been deleted. `--classify` dry-run confirms the single discriminator
  `ADDED_TEST crates/server/tests/gateway_recover_totality.rs`. The RED leg keeps it, reverts
  `crates/core/src/metadata.rs` and `crates/server/src/lib.rs`, and runs
  `cargo test -p wyrd-server --test gateway_recover_totality`. On that tree `high_water_marks`
  still does `decode(&value)?` and still calls `scan`, so criterion (1) fails (`recover()` is
  `Err`) and criterion (2) fails (`recover()` is `Err(ScanCapExceeded)`) — **assertion failures
  through a signature-stable public entry point**. Driving `Gateway::recover()` rather than
  `high_water_marks` directly is deliberate and load-bearing: this patch changes
  `high_water_marks`'s own signature, so a test naming it would compile-fail on the red leg and be
  scored as a pass. Nothing this patch adds is imported, and no dev-dependency is added
  (`RedbMetadataStore` + `FsChunkStore` + `MemCoordination` is the composition
  `crates/server/tests/s3_http_wire.rs:679-686` already drives; `async-trait` is a normal
  dependency of `wyrd-server`, so the criterion-(2) store double needs nothing new). Plain Linux
  workspace, no topology, no cfg gate, so neither the vacuous `0 tests … ok` branch
  (`:383-389`,`:420-427`) nor a compile-red-scored-as-pass can occur.
- **Invariant to restore:** **C-1 — no permanent or data-losing failure mode is an acceptable
  cost** (`docs/principles.md` §5 C-1 / §6 *Storage lifecycle / reclamation*; maintainer's rule
  2026-07-25; `0016:2802-2813`; `../wyrd/crates/custodian/src/gc.rs:22-25`). Over this slice's
  category — **startup recovery, the step that runs before anything is served**:
  - **Recovery is total over stored content.** No arrangement of records — undecodable, corrupt,
    or merely more numerous than a scan cap — may make it refuse. A gateway that will not start is
    a state the store's own contents put it in and nothing but manual repair exits.
  - **Recovery reads only what it can bound** — a walk whose cost is set by the store's size
    rather than the reader's page budget converts growth into unavailability.
  - **A floor an allocator trusts is never a quiet under-approximation.** Any id mark recovery
    produces must be ≥ every id whose bytes still exist; a silently low floor lets the allocator
    re-mint a still-live id and clobber a committed object's fragments. The two legal answers for
    a record it cannot read are *fail closed for that record* or *contribute its true floor* —
    **never zero**.
  - **A number nobody reads is not a safety property** — the right way to satisfy the rule above
    for a mark with no consumer is to remove the mark, not compute it more carefully.
- **Repo + branch target:** getwyrd/wyrd @ main   (resolved and verified at Plan:
  `git -C ../wyrd ls-remote --heads origin main` → `d50f0ca`, matching the sandbox's
  `origin/main`. Carries #648 (PR #672) and #649/#650 (PR #683), so `high_water_marks` is in its
  post-#648 form — which is what makes defect (1) live.)
- **Depends on:** *(none — its only code dependency, #648, is merged to `main`)*
- **Conflicts with:** 682
- **Ordering note:** **Revised 2026-08-02, after slice 4 was split into #651 / #681 / #682.**
  The previous note scheduled this behind #651 because both edited
  `crates/core/src/metadata.rs`. That is no longer true: the re-scoped #651 is restore +
  `desired_state` only and does not touch `metadata.rs` at all — the `repoint_chunk` / record-ceiling
  work that did is now **#682**. Comparing *modified* file sets (not cited ones):
  this slice edits `crates/core/src/metadata.rs` and `crates/server/src/lib.rs`; #651 edits
  `custodian/{restore,desired_state}.rs` and `server/src/cli.rs`; #681 edits
  `custodian/{reconstruction,backfill,rebalance}.rs`. **No overlap with #651 or #681**, so the
  old ordering was needless serialisation — one wave spent for nothing.
  `Conflicts with: 682` is the one real constraint: both edit `crates/core/src/metadata.rs`
  (this slice rewrites `high_water_marks`; #682 adds `repoint_chunk` and the ceiling helpers), and
  neither builds on the other, so they must land in **different waves** rather than be built blind
  on one base. `Depends on:` is empty because the *code* dependency this slice actually has —
  #648's decode-time invariants, which make defect (1) live — is already on `main`
  (PR #672). Under `wave_mode = "merge"` every wave builds off `origin/main`, so nothing
  further is needed to reach it.
- **Surfaces:** data
- **Difficulty:** low
- **Scope:** make **startup recovery** total and bounded, and remove the id-floor half that has no
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
- **Budget:** ≤ ~1,500 added semantic lines (non-blank, non-comment, non-mechanical), ≤ 15 files.
  Mechanical migration: **none expected** — if a `high_water_marks` signature change ripples,
  declare it as *"callsites of `high_water_marks` dropping the discarded second tuple element"*
  and count it separately. This slice should land far under budget: #647's floor region measures
  ~282 semantic lines and is being **removed**, not ported, so a patch approaching the budget is a
  signal you are porting apparatus rather than shrinking it. If mid-build the tree exceeds this,
  STOP and hand back a proposed split instead of finishing — an over-budget patch is
  iterate-to-Plan by default, not another Do round.
- **Repro instruction:** `git -C ../wyrd show origin/main:crates/core/src/metadata.rs` —
  `high_water_marks` at `:847`, `store.scan(b"inode:")` at `:851`, `decode(&value)?` at `:855`,
  the two further `scan`s at `:862`,`:873`; `.../crates/server/src/lib.rs` — `recover` at `:123`
  discards the chunk mark at `:124` while `mint_chunk_id` at `:238-241` puts every id ≥ 2^127.
  To see defect (1) directly: seed a store with a healthy object plus one raw undecodable
  `inode:` value and call `Gateway::recover()` — it returns `Err` and the gateway will not serve.
- **External dependencies:** none. Base Rust toolchain only — no Docker, no protoc, no live
  backend, no new dev-dependency, and (unlike the earlier slices) **no docs paragraph**, so the
  prose gates are not in play for this bundle.
- **Test file:** `crates/server/tests/gateway_recover_totality.rs` — a **NEW** file (this
  project's C4 discriminator is an added `*/tests/*.rs`; an appended or co-located test degrades
  to the green-only branch, `run-verify.sh:392-402`), placed at the `wyrd-server` level on
  purpose so it drives the signature-stable `Gateway::recover()` rather than the
  signature-changing `high_water_marks` — see *Falsifiability*.
  `crates/server/tests/s3_http_wire.rs:665-700` is the precedent for constructing the gateway and
  writing raw metadata keys. Co-located `metadata.rs` unit tests may ship **in addition** —
  `C4-ci` covers them.
- **Verification posture:** default — assertion-red on the base (`origin/main`, which carries
  #648, #649 and #650; this slice needs none of #651/#681/#682), green
  with this patch, both at Check. Nothing deferred: both binding criteria are exercised by the new
  target in this cycle.
- **Citations expected:** cite `path:line` on the target branch for every change. **Salvage —
  extract and adapt from `results/issue_652/sources/salvage.diff` (this bundle, permitted input);
  do not re-derive settled code.** It carries #635's `crates/core/src/metadata.rs` and
  `crates/server/src/lib.rs`. **Take only two things from it: the `for_each_page` helper (with its
  recorded rationale for why `high_water_marks` must not use `scan`) and the paged,
  containment-commented `high_water_marks` body. Leave the whole floor apparatus behind** —
  `RecoveredIds`, `ClassIds`, `unreadable_record_floor`, `raw_chunk_id_floor`,
  `json_chunk_id_floor`, `scavenged_chunk_id_floor`, `torn_digit_escape`, `json_string_token`,
  `segment_chunk_floor`. Peers Do MAY open: **the bounded-paging peer already merged** (#634 /
  PR #645, `18180a2`) — `origin/main:crates/traits/src/lib.rs:1019-1023`, `:1078-1092`,
  `:1037-1046`; mirror its cursor loop. **The minting scheme that makes the mark dead** —
  `crates/server/src/lib.rs:229-241`,`:243-261` and `crates/server/src/cli.rs:1716-1723`. **What
  must not regress** — `crates/server/tests/s3_http_wire.rs:644-700` and
  `crates/server/src/cli.rs:1691` (`seed_next_inode_floor`, the surviving consumer).
- **Prior-art check (triage cycles):** searched by affected file path across merged history, open
  and closed PRs, plus commit archaeology on both functions. `crates/core/src/metadata.rs` (10
  PRs) and `crates/server/src/lib.rs` (19 PRs): **#647 is the only one touching this concern and
  it is CLOSED unmerged** — closed on **reviewability, not correctness**; its floor apparatus and
  the `Optional`-classed-corrupt-root finding are what this slice removes and fixes. Every other
  PR is MERGED and none touches this walk's totality.
  `high_water_marks`'s chunk mark and its discard both trace to the **same merged commit**,
  `fdd34f1` (#487, "make gateway id allocation safe for active-active gateways"):
  `git -C ../wyrd log --oneline -S"_max_chunk" origin/main -- crates/server/src/lib.rs` and
  `-S"random_chunk_epoch"` each return exactly that commit — the change that made the mark
  unnecessary is the one that stopped reading it. `git -C ../wyrd grep -n "for_each_page\|scan_page" origin/main -- crates/core/`
  is empty: `core` does not yet use the #634 seam at all. No merged or rejected prior art makes
  this walk total or bounded.
  **Do-not-re-earn (standing rejections; content-stable — they bind wherever the finding
  re-lands, not at a line):** (i) *caller-side fan-out timeout* — rejected 3× across #508/#636:
  the `ChunkStore` implementation owns the network bound, not the caller; (ii) *retraction of
  already-published bytes* — rejected 4× in #638 on unchanged evidence; (iii) *"`Completed`
  releases its admission slot"* — withdrawn as unsatisfiable; a `Completed` tombstone **stays
  counted**; (iv) every settled decision named in the slice issue's body, in particular that
  removing a floor with no caller is an **accepted outcome**, not a coverage gap — a finding that
  asks for the chunk-id floor to be restored or re-derived must be answered with the *Scope*
  decision above, not by rebuilding the apparatus. Do MUST record each rejection in
  `review-rejected.md` **at every line the finding is reported at**.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR MAY
happen during the cycle. The PR MUST NOT be marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 1): rebuilding for the implementation-level findings — T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 10 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue. 1 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 10 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 2): rebuilding for the implementation-level findings — C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo deny --all-features --config deny-all-features.toml check advisories` failed with exit status: 2. 2 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) — xtask: `cargo deny --all-features --config deny-all-features.toml check advisories` failed with exit status: 2
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 8 mutants tested in 2m: 3 missed, 1 caught, 4 unviable
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: Advisory review (re-run after upgrading codex-cli 0.143.0 -> 0.146.0, whose earlier version could not resolve the gpt-5.6-sol model) surfaced a real, verified gap: the brief's "total over damage" criterion is not actually total. `Gateway::recover()` (crates/server/src/lib.rs:123-125) calls the newly-fixed, bounded/total `high_water_marks` walk, then unconditionally calls `cli::seed_next_inode_floor` (crates/server/src/cli.rs:1696-1704), which still does `std::str::from_utf8(bytes)?.parse()?` on the persisted `meta:next_inode` record. A single corrupted `meta:next_inode` value therefore still makes `recover()` return Err and takes the whole gateway down before it serves anything -- the same one-damaged-record-stops-everything defect class the brief fixes for `inode:` / `pending:` / `orphan:` records, just left open in this third place. For the rebuild: extend the same treatment used for `inode:` rows (attribute on the recovery audit seam, recover what the key alone gives you, do not let a bad value end the walk / abort recovery) to `seed_next_inode_floor`'s read of `meta:next_inode`, so that startup recovery is total over this record too. Keep the rest of this round's scope (bounded scan_page walk, dead chunk-id floor removal) -- only this one path needs closing.
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 4 — carry-forward (from the previous attempt)
- Sign-off rationale: The cli.rs allocator-counter change and the one-line docs deletion are ACCEPTED as in-scope (see §6 C3, cleared): build-notes.md shows both were reactions to a real totality gap found by a prior review round's carry-forward (recovery is two steps — the metadata walk and `cli::seed_next_inode_floor` — and a damaged persisted `meta:next_inode` counter still crashed startup before this round), not scope creep. Do not revert these in the next round unless a new reason emerges. Carry forward the remaining reviewer/adversary findings for the next Do round to close: - T4 batched rubric review: 4 blocking findings (review-branch) — resolve or address explicitly; do not treat the "review-branch absent from my checkout" caveat in the advisory review as dismissing this, the bundle's own T4 gate ran and failed for real. - T5 Judgment [impl]: `parse::<u64>` at cli.rs:1678 accepts `+1` and `01` as valid `1`, so non-writer/malformed bytes evade the damage-attribution/reseed path this round just built — and the test matrix at cli.rs:2789 doesn't cover either case. Tighten the parse to reject non-canonical digit strings and add coverage for both `+1` and `01`. - Validation / fitness-to-purpose (open question, needs an explicit answer next round, not silent deferral): page-bounded memory bounds memory but not time — the loop still walks the entire namespace to exhaustion (metadata.rs:2127), and the runtime fixture only covers rows 100-250 (gateway_recover_totality.rs:499), so large/actively-growing-store startup latency is unmeasured. Either demonstrate this is acceptable for the real deployment scale or note what bound is actually needed. - C4 Verification: reviewer's own run stopped on a host-local read-only cargo advisory-db lock during `cargo deny check`, unable to complete that leg independently — flagged only because the driver's own C4-ci gate result should be treated as authoritative here (it passed); confirm this isn't masking a real dependency-wall issue in the next round's gate run. - Note for context, not required to act on: the "leaf produced no usable verdict" and "review-branch absent from allowed target" flags in this bundle look like environment/checkout artifacts from the advisory reviewer's sandbox rather than defects in the patch — re-verify they don't recur in the next round's evidence. </content>
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_652/review-b
- Full previous attempt preserved in `iteration-v4/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 5 — carry-forward (from the previous attempt)
- Sign-off rationale: Rebuild must fix 3 concrete bugs surfaced by this round's review, not re-litigate scope: 1. Unbounded retry / hang: `seed_next_inode_floor` (crates/server/src/cli.rs:1852) retries CommitOutcome::Conflict forever with no retry budget or backoff (T3 Runtime FAIL) — this is suspected to be the actual cause of the C4 `cargo xtask ci` gate timing out at 7200s, not infra flakiness. Add a bound/backoff so `Gateway::recover` always reaches a result. 2. Unsafe damaged-counter reseed logic (cli.rs:1851, :1869, :1870): recomputing the counter from committed `inode:` keys after a damaged counter can reuse an ID whose fragments are still live under an `orphan:` grace record, or rewind past an ID already handed to a concurrent in-flight allocator — allowing collision / double-mint. Needs a reseed strategy that cannot regress below any live or in-flight id. 3. Lenient `parse_inode_key` (crates/core/src/metadata.rs:2190, :2198): `.parse()` accepts non-canonical forms (`inode:+7`, `inode:007`, `inode:+18446744073709551615`), so damaged- looking keys are treated as valid instead of being flagged/attributed, and can even falsely exhaust the allocator. Tighten the parse to the canonical grammar and extend the acceptance test beyond `inode:not-a-number` to cover these forms. Scope note: the human confirmed the cli.rs / docs-adjacent surface (allocator-repair expansion) was already approved in a prior iteration of this bundle — not a new scope violation, so this is NOT being sent back to Plan. Proceed with iterate-do, not iterate-plan.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_652/review-b
- Full previous attempt preserved in `iteration-v5/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 6 — carry-forward (from the previous attempt)
- Sign-off rationale: Rebuild on the same brief; fix the two blocking findings from this round's batch review: 1. metadata.rs:2212 / metadata.rs:2169 (BUG, blocking) — paging `high_water_marks` via independent `scan_page` calls provides no consistent namespace snapshot. A concurrent legacy writer can insert an `inode:` key behind the scan cursor after its page has already been read, so recovery under-seeds the allocator and the newly-seeded allocator can later re-mint (collide with) that still-live inode ID during a rolling upgrade. Give the paged walk a bound that cannot miss a key inserted behind the cursor during the scan (or otherwise close the gap) and add a regression that forces an insert behind the cursor mid-walk, asserting the seeded floor still exceeds it. Human note at sign-off: the size backstop in §6 (110 KB patch, round 5, threshold 100 KB / 2 rounds) suggested iterate-plan given the pattern of each round surfacing a distinct new implementation-level bug (hang/retry in v5, unsafe reseed logic, lenient parsing, and now this snapshot-consistency race) rather than the same bug recurring. The human weighed this and chose iterate-do on the basis of this round's concrete findings rather than a re-split; if another distinct implementation-shaped bug surfaces next round, iterate-plan should be reconsidered. Other §6 items not yet cleared (carry forward, not addressed by this rebuild alone): - T4 Contribution NEEDS-HUMAN — reviewer's checkout lacked the review-branch log; the driver's own gate is authoritative and did fail for real this round. - Validation / fitness-to-purpose NEEDS-HUMAN — is O(inode namespace) startup time (memory-bounded, not time-bounded) acceptable at real deployment scale? Still open, revisit at next sign-off.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_652/review-b
- Full previous attempt preserved in `iteration-v6/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 7 — carry-forward (from the previous attempt)
- Sign-off rationale: REJECTED because the slice now carries two invariants and only one of them was ever briefed. WHAT IS SOUND AND SHOULD SURVIVE THE SPLIT (metadata.rs + server/src/lib.rs): `high_water_marks` returning `InodeId` alone; deletion of the dead chunk-id mark and the `pending:`/`orphan:` walks that fed it; `for_each_page`; `InodeValueReading` + `read_inode_value` with the id taken from the key BEFORE the value is inspected; the `RECOVERY_AUDIT` seam; `parse_canonical_u64` promoted to a single shared grammar for both `inode:` keys and `meta:next_inode`. This half has been stable since ~v2 and is close to done. WHY IT IS GOING TO PLAN RATHER THAN ANOTHER DO ROUND: The brief scopes two production files (`crates/core/src/metadata.rs`, `crates/server/src/lib.rs`). `crates/server/src/cli.rs` entered in three steps of unequal legitimacy: 1. (v3) `seed_next_inode_floor`'s read of `meta:next_inode` — legitimate: it is literally the second half of `Gateway::recover`, and leaving it fail-loud would give away the totality property the brief exists to establish. In scope by invariant, if not by file list. 2. (v5) bounding that function's CAS retry (`SEED_FLOOR_ATTEMPTS`) — defensible: "total AND bounded" is the brief's own title and an unbounded retry inside `recover` is a gateway that never starts. 3. (v6->v7) the `require_absent(inode_key(id))` guard, conflict-disambiguation `get`, and counter-stepping loop inside `alloc_inode` — NOT justified by the recovery invariant. `alloc_inode` is not called by `Gateway::recover`; it is the PUT path. It was touched purely to compensate for the fact that switching `scan` -> `scan_page` gives up snapshot consistency, so the seeded floor can sit below a live id. Step 3 is where the size went (32 KB at v1 -> 140 KB now, crossing 100 KB at v5) and where every new finding has landed: hang/retry (v5), unsafe reseed (v5), lenient parsing (v5), snapshot race (v6), TOCTOU x4 (v7). Rounds spent: 6, against a threshold of 2. THE ROOT CAUSE IS A MISSING SPEC, NOT A MISSING FIX: The `cli.rs` allocator surface has never been briefed. It arrived via a Do-round carry-forward and was ratified at the v4 sign-off, but no brief ever stated its invariant, its acceptance criterion or its budget. There is therefore nothing for a rebuild to close against, which is exactly why each round yields a NEW finding rather than converging on an old one. A Do round can satisfy a criterion; it cannot author one. Reinforcing this: the patch's own doc comments concede that the collision hazard the `alloc_inode` guard defends against is largely PRE-EXISTING and unclosable by this slice — "the peer never reads the counter this mark seeds, so it keeps allocating after any walk ends ... a floor derived from stored keys is stale the instant it is computed, at any price in round trips." Paging opens only a narrower new hole (keys committed behind the cursor during the walk itself). So a partial guard is being asked to carry an older, larger problem, and reviewers correctly keep finding the parts it does not cover: the cluster path writes fragments BEFORE the metadata commit, so `require_absent` at reservation time does not hold the id across the operation (review-batch cli.rs:1856/:1857), and the guard is skipped entirely in the `Absent` branch (cli.rs:1812) on a justification that answers metadata collision while the stated hazard is fragment overwrite. DIRECTION FOR THE RE-PLAN — three options, all Plan-level; option 1 in particular requires a brief because it drops a stated success criterion: 1. Do not page in this slice. Keep `scan`, ship damage-containment totality + the dead-floor deletion only. The snapshot regression disappears and with it the entire reason to touch `alloc_inode`; defect (2) ("a store merely too large to scan") becomes its own slice. This collapses the patch back toward v2 size and is the cheapest convergent path. 2. Page, and file the inode-allocator collision safety as its own issue. Ship the recovery half, stating plainly in the PR that the floor is a starting point and that the rolling-upgrade re-mint hazard is tracked separately. Only honest if the narrow new hole paging opens is acceptable standalone. 3. Page, and close the window properly here. Needs a genuine reservation — a reserving record, or reordering the cluster path's metadata commit ahead of its fragment writes. Well past this bundle's budget and likely ADR territory. Whichever split is authored, the second child (allocator collision safety) needs its own stated invariant — "the allocator never hands out an id that is live under a concurrent legacy peer" — and its own acceptance criterion covering the fragment-write window and the absent-counter case. Also carry forward: the surviving C5 mutant (`&&`->`||` at cli.rs:1865) shows the new allocator branch is not pinned by tests to the degree its risk warrants. Note for the re-plan: the "leaf produced no usable verdict" and "review-branch absent" §6 items look like reviewer-sandbox artifacts (the driver's own gates ran for real), and the cargo-deny external-dependency item is answered by the driver's green C4-ci.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 16 mutants tested in 2m: 1 missed, 5 caught, 10 unviable
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_652/review-b
- Full previous attempt preserved in `iteration-v7/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
