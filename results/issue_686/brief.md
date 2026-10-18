# Brief — issue 686 / streamed-get-moves-flat-chunk-map

> The Plan artifact (docs 02 §PLAN). Do reads ONLY this file. The `- **Label:** value`
> lines are parsed by the driver — keep their shape.
>
> Citations verified against `getwyrd/wyrd` `origin/main` @ `7bf7eb9` on 2026-10-14.

- **Slug:** streamed-get-moves-flat-chunk-map
- **Track:** alpha
- **Defect:** Every streamed GET of a flat (non-segmented) object deep-copies the object's
  whole chunk list — every `ChunkRef` and its placement vector — on the hottest read path,
  while the comment above the copy says it is a move. In `get_object_streaming`
  (`crates/server/src/lib.rs:418-451`), `read::committed_inode` returns an **owned** inode
  (`:419`), `metadata::resolve_chunk_map(.., &inode)` borrows it (`:427-428`), and for a flat
  map the resolver answers `Cow::Borrowed` (`crates/core/src/metadata.rs:3007`, record borrowed
  at `:3048`). `resolved.chunks.into_owned()` at `crates/server/src/lib.rs:451` therefore runs
  `<[ChunkRef]>::to_vec()`. The comment at `:447-450` ("`into_owned`, not a deep copy … moves
  its chunk list … into the reader task") is wrong. History: `bbdb7c5` (#648 follow-up) made this
  site move the list with `inode.chunk_map.into_flat()`; `99c7fcf` (the shared-resolver replay,
  PR #683) replaced that with the resolver seam and lost the move. `ChunkMap::into_flat`
  (`crates/core/src/metadata.rs:1267-1276`) now has no caller. Correctness is unaffected; the
  cost is an allocation plus a copy proportional to the chunk count on every flat-object GET.
- **Success criterion:** For a committed **flat** object whose resolve did not restart, the
  chunk list the streamed GET hands to its reader task is the committed record's **own** heap
  allocation — moved, not copied — and **nothing on that path copies the chunk list in any other
  form either**. Two checks, both required:
  (i) **Test.** A NEW test hands a flat `InodeRecord` **by value** to the same resolution path
  the streamed GET uses. The record's flat list is **non-empty** (at least two `ChunkRef`s) and
  each `ChunkRef` has a **non-empty `placement`** (`crates/core/src/metadata.rs:361`). An empty
  `Vec` would defeat the check, because a copied empty `Vec` has the same dangling pointer as the
  original. The test asserts that the resolved chunk list's buffer pointer (`as_ptr()`) equals
  the record's original outer `Vec` pointer, that the first `ChunkRef`'s `placement.as_ptr()`
  equals its original too, and that the contents are unchanged.
  (ii) **Source review at Check.** The pointer test cannot see a clone of the **record**: the
  resolved value also carries an `InodeRecord` (`ResolvedChunkMap.record`,
  `crates/core/src/metadata.rs:2688-2694`), whose derived `Clone` (`:1610`) copies `chunk_map`.
  An implementation could clone the record for `record` and move the original list into
  `chunks`, and (i) would pass while every chunk and placement vector is still copied. So the
  owned flat path must contain no `.clone()` / `to_owned()` / `into_owned()` / `to_vec()` of an
  `InodeRecord`, a `ChunkMap` or a `[ChunkRef]`, from the owned record to the reader task. The
  framing the GET needs (`size`, `etag`, `content_type`, `modified`, `crates/server/src/lib.rs:439-445`)
  is carried without copying the list. Do names in `build-notes.md` how the record's framing is
  kept, so the reviewer can check it. The same test shows the path still answers as before for
  (a) a **segmented** record — the chunk list assembled from its `seg:` records in table order —
  and (b) a record **superseded mid-resolve** — a restart onto the live generation, framed by
  that generation's record. `get_object_streaming` uses that path, and the comment at
  `crates/server/src/lib.rs:447-450` says what the code does. The existing streamed and ranged
  GET suites stay green: `crates/server/tests/segmented_object_read.rs`,
  `crates/server/tests/s3_http_wire.rs`, `crates/server/tests/s3_range_conditional.rs`,
  `crates/server/tests/read_path.rs`.
- **Falsifiability:** RED is reachable in-process on the plain `cargo test -p wyrd-core` target,
  no special topology: any resolution that copies the list — today's
  `Cow::Borrowed(..).into_owned()` — returns a fresh allocation for a **non-empty** list, so the
  pointer-identity assertion fails. A copy hidden in a record clone is caught by check (ii),
  not by the test; that is why (ii) is required. The caveat is C4-verify,
  not the environment: the discriminating test has to call the ownership-taking path this fix
  adds, so it cannot compile on the reverted base (see `Verification posture`). Do proves the red
  by temporarily making that path copy (`.to_vec()`) and recording the failing run.
- **Invariant to restore:** On the streamed GET's **primary** path (a resolve that did not
  restart), the consumer that **owns** the committed record takes its flat chunk list by
  **move**, and copies neither the list nor the record that holds it. The list is copied only
  where its source is genuinely shared. **Not claimed here:** the restart path. A resolve that
  restarts onto the live root (`resolve_current_chunk_map`, `crates/core/src/metadata.rs:3074-3109`)
  still copies a flat successor's list (`:3091`) while keeping the whole record (`:3092`). That
  path runs only when a generation is superseded mid-read, and changing what its `record` holds
  reaches the custodian consumers of the same resolver. It stays as it is (see Scope).
  **Source (internal, Tier C):** the rule `bbdb7c5` recorded on `ChunkMap::into_flat`
  (`crates/core/src/metadata.rs:1267-1270`, "the owning counterpart of `as_flat` for a consumer
  that holds the map … and would otherwise deep-clone every placement vector") and the streamed
  read's own stated contract (`crates/server/src/lib.rs:447-450`). SELF-TEST: it is stated over
  every owning consumer of the resolver, not the one line at `:451`.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:**
- **Conflicts with:**
- **Ordering note:** Wave 0, no scheduling edge. It edits `get_object_streaming`
  (`crates/server/src/lib.rs:418-451`) and possibly the resolver in
  `crates/core/src/metadata.rs:2990-3110`. #666 adds a separate `impl` block to the same server
  file, and #671/#768 edit its PUT path (`:234-410`). Those are different functions, far enough
  apart that their hunks do not overlap. **Intake-cap override (wyrd-pdca-P1):** the human allowed
  all seven ids of this batch (656, 666, 671, 767, 768, 620, 686) in over the alpha cap in the
  2026-10-14 Plan session. Count at that moment: alpha 4/6 planned (508, 625, 633, 654), room 2,
  need 7.
- **Surfaces:** data
- **Difficulty:** low
- **Scope:** the copy of the flat chunk list on the streamed GET's primary path (a resolve that
  did not restart), so the reader task owns the record's own list, and the comment at
  `crates/server/src/lib.rs:447-450` / out of scope: the ranged GET (`crates/server/src/lib.rs:505-560`)
  and `read::read_object` (`crates/core/src/read.rs:524-545`), which borrow the list and never
  copy it whole; the custodian consumers of `resolve_chunk_map` (gc, backfill, rebalance,
  reconstruction, restore); the restart path's copy at `crates/core/src/metadata.rs:3091`
  (left as it is; the invariant does not claim it, and `resolve_current_chunk_map`'s output
  stays unchanged for its other callers); anything a GET returns or how it frames the response.
- **Repro instruction:** On `origin/main`, read `crates/server/src/lib.rs:418-451`. The inode is
  owned (`:419`) but only borrowed by the resolver (`:427-428`). For a flat map
  `resolved.chunks` is `Cow::Borrowed` (`crates/core/src/metadata.rs:3007`), so `.into_owned()`
  at `:451` copies the slice. `git -C ../wyrd show bbdb7c5 -- crates/server/src/lib.rs` shows the
  move this site used to make, and `git -C ../wyrd log -S "into_flat()" -- crates/server/src/lib.rs`
  shows `99c7fcf` removing it.
- **External dependencies:** `typos`
- **Test file:** `crates/core/tests/streamed_read_chunk_map_ownership.rs` — a **NEW** file.
  C4-verify (`engine/scripts/run-verify.sh`) treats only an **added** `crates/*/tests/*.rs` as
  the per-fix discriminator; a test appended to an existing file, or inline in `src/`, silently
  degrades to green-only. If Do places the ownership-taking path in `wyrd-server` as a public
  item rather than in `wyrd-core` beside the resolver, the new file goes under
  `crates/server/tests/` instead; it must still be a new file.
- **Verification posture:** C4-verify cannot isolate a red here, and this is declared up front
  so it becomes a sign-off item instead of a surprise. No pre-existing public API exposes
  whether the streamed GET copied, because the chunk list never leaves the reader task. The two
  ways to observe it through existing API are both closed. A counting `#[global_allocator]`
  needs `unsafe impl GlobalAlloc`, and `AGENTS.md` requires `#![forbid(unsafe_code)]` in every
  crate root, which includes every integration-test file. An allocation-counting dev-dependency
  needs the ADR-0003 dependency audit plus a `deny.toml` entry, a human sign-off item out of
  proportion to this fix. So the discriminating test calls the ownership-taking path this fix
  adds. On the reverted base it does not compile, and C4-verify reports **UNVERIFIABLE**
  (exit 77). That outcome is expected here and is not a defect. **Built and exercised at Check:**
  the path and its new test (green), and the server's streamed GET through it via the existing
  GET suites named in the success criterion. **Demonstrated red:** Do temporarily makes the path
  copy, runs the new test, sees the pointer assertion fail, and pastes that output into
  `build-notes.md`.
- **Citations expected:** Do must cite `path:line` on the target branch for every change. The
  move this path used to make is `bbdb7c5`'s use of `ChunkMap::into_flat`
  (`crates/core/src/metadata.rs:1267-1276`). The restart path already returns an owned
  `ResolvedChunkMap<'static>` (`crates/core/src/metadata.rs:3074-3109`), so the owned path can
  hand a restart's result through unchanged; only the non-restarted flat case must move.
- **Prior-art check (triage cycles):** searched by path `crates/server/src/lib.rs` and
  `crates/core/src/metadata.rs`. Merged history: `bbdb7c5` (the original move, #648 follow-up)
  and `99c7fcf` (its loss, PR #683, merged 2026-08-02). Open PRs on getwyrd/wyrd: none at all.
  Closed or rejected PRs mentioning #686 or this copy: none.
- **Disposition hint:** likely-fix

Plan-review response: both findings checked against `7bf7eb9` and revised in place. (1) The
criterion now forbids a copy of the list in any form on the owned flat path, including a clone
of the returned record (`metadata.rs:2688-2694`, `:1610`), as an explicit source-review check;
the pointer test now requires a non-empty list with non-empty placements and also pins the first
placement's pointer. (2) The invariant is narrowed to the non-restarted streamed GET, and the
restart copy at `metadata.rs:3091` is out of scope rather than optional.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 1): rebuilding for the implementation-level findings — C2 Reproduction (red pre-fix) — Accept the declared mutation-based substitute for a pre-fix behavioral red — stashing production changes produces E0432 before any test executes; independently adding the forbidden copy makes the pointer assertion fail (`reviewer-red-base.log:173`, `reviewer-copy-mutant.log:14`, `brief.md:106`).; C4 Verification (red→green) — Resolve the existing dependency-gate failure before declaring full verification — patch tests pass, but frozen CI fails on unchanged TiKV/rustls dependencies; this sandbox's CI instead stops at the advisory-cache lock (`gate-logs/C4-ci.log:6470`, `gate-logs/C4-ci.log:6500`, `reviewer-ci.log:3354`).; Three nearby doc comments now say something the code no longer does. That is the same defect class as #686 itself, which was a comment claiming a move while the code made a copy. (1) `crates/core/src/metadata.rs:3028-3030`: `resolve_chunk_map` still calls itself "the entry for … the gateway's streaming and ranged reads". The streaming read now uses `resolve_chunk_map_owned` (`crates/server/src/lib.rs:432`). (2) `metadata.rs:1617-1620`: `InodeRecord::chunk_map` says `resolve_chunk_map` / `resolve_current_chunk_map` "is the ONE way" to resolve the map, and does not mention the new public entry. (3) `metadata.rs:1267-1270`: `ChunkMap::into_flat` names "a streamed read moving the list into its reader task" as its consumer. That read now destructures the record inside `resolve_chunk_map_owned` and never calls `into_flat`, which still has no caller. Fix: update the three comments, either deleting `into_flat` or dropping the consumer claim from its comment. Optionally, adjust "one call" in `docs/design/architecture/06-runtime-view.md:29`; the logic is still shared, but there are now two public entries.; C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo deny --all-features --config deny-all-features.toml check advisories` failed with exit status: 1. Deferred, not addressed here: 3 finding(s) that need human judgment, held in deferred-findings.json for the human at handover — not build work
- Failing gate: C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) — xtask: `cargo deny --all-features --config deny-all-features.toml check advisories` failed with exit status: 1
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 2): rebuilding for the implementation-level findings — C2 Reproduction (red pre-fix) — Accept the declared mutation-based substitute for a pre-fix behavioral red — reverting production produces E0432 before any test executes, while reintroducing the copy fails the pointer assertion (`brief.md:106`, `reviewer-red-base.log:175`, `reviewer-copy-mutant.log:17`).; C4 Verification (red→green) — Determine the disposition of the existing TiKV advisory failures and incomplete gate evidence — targeted tests pass, but the all-features advisory scan remains red and diff coverage has no completed result (`reviewer-deny-all-features.log:9`, `reviewer-deny-all-features.log:39`, `gate-logs/C4-diff-cov.log:7`).; C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo deny --all-features --config deny-all-features.toml check advisories` failed with exit status: 1. Deferred, not addressed here: 3 finding(s) that need human judgment, held in deferred-findings.json for the human at handover — not build work
- Failing gate: C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) — xtask: `cargo deny --all-features --config deny-all-features.toml check advisories` failed with exit status: 1
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 3): rebuilding for the implementation-level findings — C2 Reproduction (red pre-fix) — Accept the declared mutation-based substitute for a pre-fix behavioral red — reverting production yields E0432 before tests execute, while independently reintroducing the copy fails the pointer assertion (`brief.md:106`, `reviewer-red-base.log:175`, `reviewer-copy-mutant.log:17`).; C4 Verification (red→green) — Determine the disposition of the existing TiKV advisory failures and require completion of full CI — focused tests pass, but the independent scan reproduces RUSTSEC-2026-0346/0349 in unchanged dependencies (`reviewer-deny-all-features.log:9`, `reviewer-deny-all-features.log:39`, `gate-logs/C4-ci.log:6549`, `Cargo.lock:3294`).; C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo deny --all-features --config deny-all-features.toml check advisories` failed with exit status: 1. Deferred, not addressed here: 3 finding(s) that need human judgment, held in deferred-findings.json for the human at handover — not build work
- Failing gate: C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) — xtask: `cargo deny --all-features --config deny-all-features.toml check advisories` failed with exit status: 1
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
