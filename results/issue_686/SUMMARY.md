# Result — issue 686 / streamed-get-moves-flat-chunk-map

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: Every streamed GET of a flat (non-segmented) object deep-copies the object's
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
- Success criterion: For a committed **flat** object whose resolve did not restart, the
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
- Repo + branch target: getwyrd/wyrd @ main
- Scope: the copy of the flat chunk list on the streamed GET's primary path (a resolve that
  did not restart), so the reader task owns the record's own list, and the comment at
  `crates/server/src/lib.rs:447-450` / out of scope: the ranged GET (`crates/server/src/lib.rs:505-560`)
  and `read::read_object` (`crates/core/src/read.rs:524-545`), which borrow the list and never
  copy it whole; the custodian consumers of `resolve_chunk_map` (gc, backfill, rebalance,
  reconstruction, restore); the restart path's copy at `crates/core/src/metadata.rs:3091`
  (left as it is; the invariant does not claim it, and `resolve_current_chunk_map`'s output
  stays unchanged for its other callers); anything a GET returns or how it frames the response.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — N/A — close disposition (no patch to verify)
- C3 Change: none — patch.diff
- C4 Verification (red→green): none — N/A — close disposition (no patch to verify)
- C5 Causal adequacy: none — reviewer + human sign-off

## 4. Conformance (Check — stack)
- T1 Structure: none — N/A — close disposition (no patch to verify)
- T2 Shape: none — N/A — close disposition (no patch to verify)
- T3 Runtime: none — N/A — close disposition (no patch to verify)
- T4 Contribution: none — N/A — close disposition (no patch to verify)
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

# Advisory review — SKIPPED (builder-declared external dependency confirmed, #341)

The builder declared an unmet external dependency in build-notes.md and the claim was CONFIRMED deterministically (the named [[doctor.checks]] row's detect cmd exited non-zero), so the Check beat was not spent adjudicating a patch already stated to be unverifiable. Gates are recorded N/A; no reviewer or adversary ran. The bundle is resumable.

- NEEDS-HUMAN — External dependency `deny-all-features-advisories` confirmed absent (detect cmd `cd "${WYRD_REPO:-../wyrd}" && cargo deny --offline --all-features --config deny-all-features.toml check advisories >/dev/null 2>&1` exited 1). Install hint: the target's base fails the all-features advisory wall (ADR-0003, #543), so the gating C4-ci row goes red on EVERY bundle for a reason in none of them. Land the advisory decision on main first (an ignore entry in deny-all-features.toml + the fdb_harness list, or a dependency bump), then re-gate. Provide it and answer iterate-do to resume the full Do+Check band, or discontinue at sign-off.


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] External dependency `deny-all-features-advisories` confirmed absent (detect cmd `cd "${WYRD_REPO:-../wyrd}" && cargo deny --offline --all-features --config deny-all-features.toml check advisories >/dev/null 2>&1` exited 1). Install hint: the target's base fails the all-features advisory wall (ADR-0003, #543), so the gating C4-ci row goes red on EVERY bundle for a reason in none of them. Land the advisory decision on main first (an ignore entry in deny-all-features.toml + the fdb_harness list, or a dependency bump), then re-gate. Provide it and answer iterate-do to resume the full Do+Check band, or discontinue at sign-off.
- [x] external dependency: deny-all-features-advisories — `main` (`7bf7eb9`) fails the gating C4-ci step `cargo deny --all-features --config deny-all-features.toml check advisories` on RUSTSEC-2026-0346 and RUSTSEC-2026-0349 (rustls 0.21.12 ← tokio-rustls 0.24.1 / tonic 0.10.2 ← tikv-client 0.4.0 ← wyrd-metadata-tikv). No patched rustls exists on the 0.21 line (both advisories: `patched = [">= 0.23.46"]`), tikv-client 0.4.0 can't reach 0.23, and this patch changes no dependency or deny file. A green C4-ci needs a maintainer to waive the two advisories under #543 (tested change below) or accept this patch with the red recorded as unrelated.
- [x] The ownership oracle can pass without removing the reported cost. `brief.md:23-28` checks that the returned chunk buffer is the input buffer, but the existing result also carries a complete `InodeRecord` (`$PDCA_TARGET/crates/core/src/metadata.rs:2688-2693`), whose derived `Clone` includes `chunk_map` (`:1610-1622`). An ownership-taking implementation could clone the record for `resolved.record`, then move the original vector into `resolved.chunks`: pointer identity and contents would pass while every chunk and placement vector still gets copied. Revise the criterion to require no full-map clone anywhere in the owned flat resolution/handoff, including preservation of the returned record, and make that an explicit source-review check. Require a nonempty chunk vector with nonempty placements for the pointer test; an empty vector also defeats the “any resolution that copies … returns a fresh allocation” claim at `brief.md:36-38`.
- [x] The restored invariant and the permitted scope disagree about restarted reads. `brief.md:42-49` requires that an owning consumer moves its list and says a restart “needs no copy either”; `brief.md:68-69` explicitly makes removing the restart copy optional. The target really does retain that copy: `resolve_current_chunk_map` calls `chunks.into_owned()` and separately retains the full owned record (`$PDCA_TARGET/crates/core/src/metadata.rs:3088-3092`); a flat successor supplies borrowed chunks (`:3007`). A primary-path-only fix can therefore satisfy the success criterion while violating the stated restored invariant. Narrow the invariant to the non-restarted streamed GET, or explicitly include the restart optimization and corresponding verification in scope.
- [x] The gating `C4-ci` red is not caused by this diff. It fails at `cargo deny --all-features --config deny-all-features.toml check advisories`, on RUSTSEC-2026-0346 and RUSTSEC-2026-0349 against `rustls 0.21.12`, which only `tikv-client 0.4.0` pulls in (`gate-logs/C4-ci.log:3470-3548`). The patch changes none of `Cargo.lock`, `Cargo.toml`, `deny.toml` or `deny-all-features.toml`. This is the same chain as the existing #543 ignore entries (`deny-all-features.toml:103-152`), so `main` fails the same way. Because `cargo xtask ci` stops at the first failure (`xtask/src/main.rs:1683-1687`), the conformance, statics, deploy-guard and DST steps never ran in this gate. I ran them on the patched tree: `xtask statics` passed, `xtask conformance` passed (5 valid + 6 invalid vectors), and `xtask dst` passed (exit 0; custodian 33/33, all other suites green). The deploy guard can't trip because the diff adds no imports. A human has to decide whether to waive this red for #686 or land the new advisory ignores on `main` under #543 first. Either way that is out of this diff's scope.
- [x] The call site is not pinned by any test. `crates/server/src/lib.rs:431-454` is the only place that applies the fix to real GETs. I reverted those lines to the pre-fix code (`metadata::resolve_chunk_map(.., &inode)` plus `resolved.chunks.into_owned()` and `record.etag.clone()` etc.), which brings back the deep copy. All four GET suites the brief names stayed green: `read_path` 4/4, `s3_http_wire` 19/19, `s3_range_conditional` 17/17, `segmented_object_read` 2/2. The new test (`crates/core/tests/streamed_read_chunk_map_ownership.rs`) only calls the core helper, so it can't see the swap. Commit `99c7fcf` lost the move the same way, with a refactor at this call site. So the only guard against a repeat is the comment at `crates/server/src/lib.rs:427-430`. The brief accepted source review for the wiring (`brief.md:44-47`, `brief.md:106-119`), so this is not a defect in the patch. The human should decide whether a comment is enough. The alternative is a server-side pin, which the brief already allows (`brief.md:103-105`: a public server item tested from a new `crates/server/tests/` file).
- [x] The one gating failure, `C4-ci` (`check-gates.json` row `C4-ci`), is not caused by this diff, and another rebuild cannot fix it. `gate-logs/C4-ci.log:6471-6549` shows `cargo deny --all-features ... check advisories` failing on RUSTSEC-2026-0346 and RUSTSEC-2026-0349. Both are advisories against `rustls v0.21.12`, which comes in through `tokio-rustls 0.24.1 → tonic 0.10.2 → tikv-client 0.4.0 → wyrd-metadata-tikv`. The patch changes no `Cargo.toml`, `Cargo.lock` or `deny*.toml` (`git diff --stat` on those files is empty). Every earlier step in the same run is green: fmt, clippy, build, the full workspace test run (including the four GET suites the brief names), machete and plain `cargo deny check`. Two auto-iterations (brief.md:144-154) have already rebuilt for this same red. A human needs to choose: accept this patch with that red recorded as unrelated, or fix the tikv/rustls advisory under its own issue. Please don't route it back to Do again.

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: merged-wider
- Iteration delta (if iterating):
- By / date: Eduard Ralph / 2026-10-14

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 2 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
- RustSec advisories: when a new one is published against an unchanged dependency, it turns the gating C4-ci red on every bundle at once and stalls the whole development process (here RUSTSEC-2026-0346/0349 on rustls 0.21 via tikv-client cost 3 auto-iterations per bundle across the batch); find a better way to handle them.
- #686 follow-up: no test pins the server call site (crates/server/src/lib.rs:431-454) to resolve_chunk_map_owned — reverting it to the copying resolver keeps every GET suite green; add a server-side pin (a public server item tested from a new crates/server/tests/ file).
