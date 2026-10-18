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
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): fail — xtask: `cargo deny --all-features --config deny-all-features.toml check advisories` failed with exit status: 1
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: unverifiable —                why this slice has no isolable red (the cargo output is above).
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 98.6% — 73 of 74 instrumentable changed lines executed (floor 80%); 74 of 148 changed lines were instrumen
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 20 mutants tested in 2m: 3 caught, 17 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_686/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.28s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #686: remove deep copies of flat chunk maps from the primary streamed GET path; no implementation defect found, with verification and sign-off decisions outstanding.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The allocation-identity requirement is falsifiable and explicitly excludes restart-path copying; the hidden-record-clone hazard is also covered by the stated source audit (`brief.md:17`, `brief.md:29`, `brief.md:49`). |
| C2 Reproduction (red pre-fix) | NEEDS-HUMAN | Accept the declared mutation-based substitute for a pre-fix behavioral red — stashing production changes produces E0432 before any test executes; independently adding the forbidden copy makes the pointer assertion fail (`reviewer-red-base.log:173`, `reviewer-copy-mutant.log:14`, `brief.md:106`). |
| C3 Change | PASS | Response framing remains attached to the served generation, and retirement/error handling retains the shared resolver semantics (`target/crates/core/src/metadata.rs:3153`, `target/crates/core/src/metadata.rs:3162`, `target/crates/server/src/lib.rs:432`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Resolve the existing dependency-gate failure before declaring full verification — patch tests pass, but frozen CI fails on unchanged TiKV/rustls dependencies; this sandbox's CI instead stops at the advisory-cache lock (`gate-logs/C4-ci.log:6470`, `gate-logs/C4-ci.log:6500`, `reviewer-ci.log:3354`). |
| C5 Causal adequacy | PASS | The primary-path copy is removed at its ownership boundary, including the hidden-record-copy hazard; no capability probe or symptom guard is introduced, and the deliberate copy mutation is caught (`target/crates/core/src/metadata.rs:3129`, `target/crates/core/src/metadata.rs:3141`, `target/crates/server/src/lib.rs:455`, `reviewer-copy-mutant.log:15`). |
| T1 Structure | PASS | The existing MetadataStore seam, segment validation and bounded restart machinery remain authoritative; owning and borrowing consumers retain separate appropriate results (`target/crates/core/src/metadata.rs:3124`, `target/crates/core/src/metadata.rs:3153`, `target/crates/core/src/metadata.rs:3163`). |
| T2 Shape | PASS | Chunk contents, segment order and generation-specific framing remain intact, with no persisted-field or protocol-operation change; the new test root forbids unsafe code (`target/crates/core/tests/streamed_read_chunk_map_ownership.rs:20`, `target/crates/core/tests/streamed_read_chunk_map_ownership.rs:147`, `target/crates/core/tests/streamed_read_chunk_map_ownership.rs:173`, `target/crates/core/tests/streamed_read_chunk_map_ownership.rs:197`). |
| T3 Runtime | PASS | Independent execution preserves all three ownership/segmentation/restart cases and all 42 tests in the four requested GET suites (`reviewer-green-final.log:10`, `reviewer-ci.log:2482`, `reviewer-ci.log:2590`, `reviewer-ci.log:2651`, `reviewer-ci.log:2683`). |
| T4 Contribution | N/A | Publication artifacts are intentionally absent at Check; the substantive contribution audit must rerun at publish, as the deferred gate explicitly states (`gate-logs/T4-contribution.log:10`). |
| T5 Judgment | PASS | Scope remains the primary streamed GET; path-based merged history and all 19 closed/unmerged PR file lists were checked, with no recorded rejection of this optimization (`brief.md:83`, `reviewer-prior-server.log:5`, `reviewer-prior-core.log:10`, `reviewer-prior-path-audit.json:66`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept the primary-path-only scope and source-audit boundary — allocation tests detect returned-list copies, but excluding a separate record clone rests on source inspection, and restart-path copying is intentionally retained (`brief.md:29`, `brief.md:49`, `target/crates/core/src/metadata.rs:3213`). |

The patch meets the brief's implementation requirements. The owned flat branch destructures the inode and moves its list and framing fields; the gateway consumes those values directly. There is no clone of the inode, map or chunk list between the owned record and reader task. The remaining `Arc::clone` shares the gateway, not the record. Segmented reads reuse `read_segments` and ordered assembly; superseded reads reuse `resolve_current_chunk_map`, including its explicitly excluded flat-successor copy. Source citations above refer to the supplied `$PDCA_TARGET`; it was readable, matched this patch, and showed no stale-base problem.

Independent verification establishes mutation red and patch green, while preserving the declared baseline limitation:

- Stashed only the two production files, kept the new integration test, and ran `cargo test --offline --locked -p wyrd-core --test streamed_read_chunk_map_ownership` in the target. It exited 101 for the missing new resolver, without running a test (`reviewer-red-base.log:173`). Restoring the stash passed all three tests (`reviewer-green-core-pinned.log:10`). This reproduces the frozen gate's UNVERIFIABLE outcome; it is not a behavioral pre-fix failure.
- In a separate scratch copy, changed only the owned flat result from `chunks` to `chunks.to_vec()`. The actual submitted test failed its outer-pointer assertion, while the segmented and restart tests passed (`reviewer-copy-mutant.log:6`, `reviewer-copy-mutant.log:15`). Recompiled the unchanged target afterward: all three passed (`reviewer-green-final.log:1`). The scratch mutation and target initially shared a build directory, which briefly reused the mutant executable; forcing recompilation resolved that reviewer-cache artifact. The submitted files were checked byte-for-byte against `patch.diff` afterward.
- Ran the pinned Rust 1.96.0 `cargo xtask ci`: typos, documentation lint/render, repository guards, formatting, clippy, build, 1,679 tests across 197 suites, and cargo-machete passed. The four requested server suites passed 4, 19, 17 and 2 tests respectively. CI then failed to obtain the advisory database lock on a read-only path (`reviewer-ci.log:3354`); retrying the advisory scans offline hit the same host restriction. The declared `typos` dependency was present and exercised (`reviewer-ci.log:2`).

The frozen evidence establishes an outstanding baseline gate failure, not a defect introduced by this patch. Both captured CI attempts pass the default dependency scan, then fail the all-features scan on RUSTSEC-2026-0346 and RUSTSEC-2026-0349 in rustls 0.21.12 through TiKV (`gate-logs/C4-ci.log:3467`, `gate-logs/C4-ci.log:3471`, `gate-logs/C4-ci.log:3501`). Cargo manifests, lockfile and deny policies are unchanged. The complete CI sequence is therefore not green: later dedicated conformance, statics, orchestrator and DST stages are not reached (`target/xtask/src/main.rs:1683`). The local advisory-lock failure is a host caveat. Neither outcome warrants attributing a verification defect to the ownership change.

The remaining frozen gates support these narrower conclusions:

- Diff coverage reports 73/74 instrumentable changed lines executed (98.6%); the unexecuted line is the owned resolver's deletion return (`gate-logs/C4-diff-cov.log:655`). This is recorded coverage, not an independently reproduced percentage.
- Mutation scanning reports three caught mutants and 17 unviable mutants, with none surviving; unviable mutants do not establish behavioral discrimination (`gate-logs/C5-mutants.log:10`). The separate copy mutation above supplies that discrimination for this defect.
- The batch-review log reports zero blocking findings (`gate-logs/T4-batch-review.log:10`). The TiKV feature log shows actual successful clippy compilations for both selected targets (`gate-logs/host-tikv.log:112`, `gate-logs/host-tikv.log:211`). These are frozen-log observations; the instance-scoped wrappers were not treated as missing target files.
- The contribution gate is deferred by design and remains N/A until its mandatory publish rerun (`gate-logs/T4-contribution.log:10`). All named gate logs were present and read; none requires escalation for missing evidence.

Prior art was checked by both affected production paths against merged history and by file lists for every closed/unmerged PR returned across all pages. Both histories contain the original move (`bbdb7c513`) and resolver replay (`99c7fcf11`). Only closed/unmerged PR #647 touched either production path; its closure comment asks for a smaller change, not rejection of this optimization ([closure decision](https://github.com/getwyrd/wyrd/pull/647#issuecomment-5137104571); `reviewer-prior-path-audit.json:66`). No settled deferral was re-raised. The target rubric's ownership-adjacent validation, framing, trait-seam and unsafe-code conventions were applied; no new clock, persisted format, capability guard, destructive path or external protocol operation was introduced.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — issue 686 (streamed GET moves the flat chunk list)

Bottom line: I tried to break the red→green proof, the no-copy claim, and the new resolver's
equivalence with the old one, and could not. What's left is one doc-comment cleanup the
builder can do, and one gate red that this diff did not cause.

## Refutation attempts that failed (the fix holds)

- **The red→green proof is real, even though C4-verify says UNVERIFIABLE.** C4-verify exits 77
  because the test imports `resolve_chunk_map_owned`, which does not exist on the base. The
  brief predicted this. I re-ran the red myself on a scratch copy of the patched tree. The new
  test passes as shipped (3/3). Then I forced three regressions into
  `crates/core/src/metadata.rs`, one at a time:
  (a) `chunks: chunks.to_vec()` in the flat arm (`:3148`) → red at
  `crates/core/tests/streamed_read_chunk_map_ownership.rs:137` (outer pointer differs);
  (b) cloning only each `placement`, while an in-place `collect` keeps the outer buffer → red
  at `:142` (first placement pointer differs). The outer-pointer check alone would miss this
  one; the placement check catches it;
  (c) framing a restart from the stale snapshot instead of the live root (`:3168-3171`) → red
  at `:198` (size 16 vs 12).
  The test calls the same function the server calls (`crates/server/src/lib.rs:432`), not a
  copy of it. The fixture has 3 chunks, each with a non-empty placement, so a copied empty
  `Vec` cannot pass by accident.
- **No hidden copy of the record (brief check ii): could not refute.** `resolve_chunk_map_owned`
  takes the record apart field by field at `crates/core/src/metadata.rs:3129-3137` and moves
  the flat list out at `:3141-3149`. There is no `.clone()`, `to_owned()`, `into_owned()` or
  `to_vec()` on that path. The only `into_owned()` calls (`:3169-3170`) are on the restart arm,
  where both `Cow`s are always `Owned` (`:3213-3214`), so they copy nothing new. On the server
  side, `lib.rs:446-455` moves the framing fields and `resolved.chunks` out by value. The
  `async move` in `stream::once` (`:469`) runs once and captures the list by move. The reader
  loop only borrows it (`:475`). Build-notes were withheld, so I checked the source directly.
- **Same answers as the borrowing resolver: could not refute.** I compared it arm by arm with
  `resolve_chunk_map` (`metadata.rs:3047-3060`): flat (no read), segmented (the same
  `read_segments` and the extracted `assemble_segments`, `:3020-3026`), superseded (the same
  `resolve_current_chunk_map`), and gone (`Ok(None)`). The `InodeRecord` destructure has no
  `..`, so a future new field breaks the build instead of being dropped silently. The caller
  passes only `Committed` inodes (`crates/core/src/read.rs:588`), the same as before.
- **Server wiring is checked by reading the code, not by a test.** No test fails if
  `lib.rs:432` goes back to `resolve_chunk_map(&inode)` plus `into_owned()`. The brief's
  verification posture accepts this (there's no way to count allocations without `unsafe` or a
  new dependency), and the current code does call the owning entry. Not raised.
- **Gone arm not tested** (`metadata.rs:3174`, the one line diff-cov missed). This is low value.
  A wrong arm there would usually still return `Ok(None)` through `resolve_current_chunk_map`
  reading the absent root. Noted, not routed.
- **Out of scope, not raised:** on restart, a flat successor's list is still copied at
  `metadata.rs:3213`, and `framed_by` then drops the original (`:3099`). The brief's Scope
  explicitly excludes this.

## Findings

- NEEDS-HUMAN [impl] — Three nearby doc comments now say something the code no longer does.
  That is the same defect class as #686 itself, which was a comment claiming a move while the
  code made a copy.
  (1) `crates/core/src/metadata.rs:3028-3030`: `resolve_chunk_map` still calls itself "the
  entry for … the gateway's streaming and ranged reads". The streaming read now uses
  `resolve_chunk_map_owned` (`crates/server/src/lib.rs:432`).
  (2) `metadata.rs:1617-1620`: `InodeRecord::chunk_map` says `resolve_chunk_map` /
  `resolve_current_chunk_map` "is the ONE way" to resolve the map, and does not mention the
  new public entry.
  (3) `metadata.rs:1267-1270`: `ChunkMap::into_flat` names "a streamed read moving the list
  into its reader task" as its consumer. That read now destructures the record inside
  `resolve_chunk_map_owned` and never calls `into_flat`, which still has no caller.
  Fix: update the three comments, either deleting `into_flat` or dropping the consumer claim
  from its comment. Optionally, adjust "one call" in
  `docs/design/architecture/06-runtime-view.md:29`; the logic is still shared, but there are
  now two public entries.
- NEEDS-HUMAN [human] — The gating `C4-ci` red is not caused by this diff. It fails at
  `cargo deny --all-features --config deny-all-features.toml check advisories`, on
  RUSTSEC-2026-0346 and RUSTSEC-2026-0349 against `rustls 0.21.12`, which only
  `tikv-client 0.4.0` pulls in (`gate-logs/C4-ci.log:3470-3548`). The patch changes none of
  `Cargo.lock`, `Cargo.toml`, `deny.toml` or `deny-all-features.toml`. This is the same chain
  as the existing #543 ignore entries (`deny-all-features.toml:103-152`), so `main` fails the
  same way. Because `cargo xtask ci` stops at the first failure (`xtask/src/main.rs:1683-1687`),
  the conformance, statics, deploy-guard and DST steps never ran in this gate. I ran them on
  the patched tree: `xtask statics` passed, `xtask conformance` passed (5 valid + 6 invalid
  vectors), and `xtask dst` passed (exit 0; custodian 33/33, all other suites green). The
  deploy guard can't trip because the diff adds no imports. A human has to decide whether to
  waive this red for #686 or land the new advisory ignores on `main` under #543 first. Either
  way that is out of this diff's scope.

<!-- pdca:leaf-complete -->

### Advisory — code-review

- No introduced correctness bugs found. `crates/core/src/metadata.rs:3141` moves the flat chunk list without cloning the record or placements; `crates/server/src/lib.rs:432` uses this owning path and `crates/server/src/lib.rs:455` moves the result into the reader task. Segmented resolution preserves error, restart, deletion, and generation-framing behavior through the existing helpers (`crates/core/src/metadata.rs:3153`).
- No actionable reuse, simplification, or efficiency findings. Both resolver paths share segment assembly (`crates/core/src/metadata.rs:3020`). The new test passes the record by value and checks non-empty outer and placement allocations plus contents (`crates/core/tests/streamed_read_chunk_map_ownership.rs:119`); the segmented and restart tests check ordered contents and served-generation framing (`crates/core/tests/streamed_read_chunk_map_ownership.rs:160`, `crates/core/tests/streamed_read_chunk_map_ownership.rs:182`).

Validation used the frozen gate logs; no tests were rerun. The three new tests and all four GET suites named in the brief passed. Full CI remains failed on dependency advisories outside this diff. C4-verify's pre-fix leg cannot compile against the new API, matching the brief's declared UNVERIFIABLE posture; it does not establish an executed red test. This advisory review does not override gate results.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C2 Reproduction (red pre-fix) — Accept the declared mutation-based substitute for a pre-fix behavioral red — stashing production changes produces E0432 before any test executes; independently adding the forbidden copy makes the pointer assertion fail (`reviewer-red-base.log:173`, `reviewer-copy-mutant.log:14`, `brief.md:106`).
- [ ] C4 Verification (red→green) — Resolve the existing dependency-gate failure before declaring full verification — patch tests pass, but frozen CI fails on unchanged TiKV/rustls dependencies; this sandbox's CI instead stops at the advisory-cache lock (`gate-logs/C4-ci.log:6470`, `gate-logs/C4-ci.log:6500`, `reviewer-ci.log:3354`).
- [ ] Validation — fitness-to-purpose — Accept the primary-path-only scope and source-audit boundary — allocation tests detect returned-list copies, but excluding a separate record clone rests on source inspection, and restart-path copying is intentionally retained (`brief.md:29`, `brief.md:49`, `target/crates/core/src/metadata.rs:3213`).
- [ ] Three nearby doc comments now say something the code no longer does. That is the same defect class as #686 itself, which was a comment claiming a move while the code made a copy. (1) `crates/core/src/metadata.rs:3028-3030`: `resolve_chunk_map` still calls itself "the entry for … the gateway's streaming and ranged reads". The streaming read now uses `resolve_chunk_map_owned` (`crates/server/src/lib.rs:432`). (2) `metadata.rs:1617-1620`: `InodeRecord::chunk_map` says `resolve_chunk_map` / `resolve_current_chunk_map` "is the ONE way" to resolve the map, and does not mention the new public entry. (3) `metadata.rs:1267-1270`: `ChunkMap::into_flat` names "a streamed read moving the list into its reader task" as its consumer. That read now destructures the record inside `resolve_chunk_map_owned` and never calls `into_flat`, which still has no caller. Fix: update the three comments, either deleting `into_flat` or dropping the consumer claim from its comment. Optionally, adjust "one call" in `docs/design/architecture/06-runtime-view.md:29`; the logic is still shared, but there are now two public entries.
- [ ] The gating `C4-ci` red is not caused by this diff. It fails at `cargo deny --all-features --config deny-all-features.toml check advisories`, on RUSTSEC-2026-0346 and RUSTSEC-2026-0349 against `rustls 0.21.12`, which only `tikv-client 0.4.0` pulls in (`gate-logs/C4-ci.log:3470-3548`). The patch changes none of `Cargo.lock`, `Cargo.toml`, `deny.toml` or `deny-all-features.toml`. This is the same chain as the existing #543 ignore entries (`deny-all-features.toml:103-152`), so `main` fails the same way. Because `cargo xtask ci` stops at the first failure (`xtask/src/main.rs:1683-1687`), the conformance, statics, deploy-guard and DST steps never ran in this gate. I ran them on the patched tree: `xtask statics` passed, `xtask conformance` passed (5 valid + 6 invalid vectors), and `xtask dst` passed (exit 0; custodian 33/33, all other suites green). The deploy guard can't trip because the diff adds no imports. A human has to decide whether to waive this red for #686 or land the new advisory ignores on `main` under #543 first. Either way that is out of this diff's scope.
- [ ] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [ ] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo deny --all-features --config deny-all-features.toml check advisories` failed with exit status: 1
- [ ] The ownership oracle can pass without removing the reported cost. `brief.md:23-28` checks that the returned chunk buffer is the input buffer, but the existing result also carries a complete `InodeRecord` (`$PDCA_TARGET/crates/core/src/metadata.rs:2688-2693`), whose derived `Clone` includes `chunk_map` (`:1610-1622`). An ownership-taking implementation could clone the record for `resolved.record`, then move the original vector into `resolved.chunks`: pointer identity and contents would pass while every chunk and placement vector still gets copied. Revise the criterion to require no full-map clone anywhere in the owned flat resolution/handoff, including preservation of the returned record, and make that an explicit source-review check. Require a nonempty chunk vector with nonempty placements for the pointer test; an empty vector also defeats the “any resolution that copies … returns a fresh allocation” claim at `brief.md:36-38`.
- [ ] The restored invariant and the permitted scope disagree about restarted reads. `brief.md:42-49` requires that an owning consumer moves its list and says a restart “needs no copy either”; `brief.md:68-69` explicitly makes removing the restart copy optional. The target really does retain that copy: `resolve_current_chunk_map` calls `chunks.into_owned()` and separately retains the full owned record (`$PDCA_TARGET/crates/core/src/metadata.rs:3088-3092`); a flat successor supplies borrowed chunks (`:3007`). A primary-path-only fix can therefore satisfy the success criterion while violating the stated restored invariant. Narrow the invariant to the non-restarted streamed GET, or explicitly include the restart optimization and corresponding verification in scope.

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
- Iteration delta (if iterating): Auto-iterate (round 1): rebuilding for the implementation-level findings — C2 Reproduction (red pre-fix) — Accept the declared mutation-based substitute for a pre-fix behavioral red — stashing production changes produces E0432 before any test executes; independently adding the forbidden copy makes the pointer assertion fail (`reviewer-red-base.log:173`, `reviewer-copy-mutant.log:14`, `brief.md:106`).; C4 Verification (red→green) — Resolve the existing dependency-gate failure before declaring full verification — patch tests pass, but frozen CI fails on unchanged TiKV/rustls dependencies; this sandbox's CI instead stops at the advisory-cache lock (`gate-logs/C4-ci.log:6470`, `gate-logs/C4-ci.log:6500`, `reviewer-ci.log:3354`).; Three nearby doc comments now say something the code no longer does. That is the same defect class as #686 itself, which was a comment claiming a move while the code made a copy. (1) `crates/core/src/metadata.rs:3028-3030`: `resolve_chunk_map` still calls itself "the entry for … the gateway's streaming and ranged reads". The streaming read now uses `resolve_chunk_map_owned` (`crates/server/src/lib.rs:432`). (2) `metadata.rs:1617-1620`: `InodeRecord::chunk_map` says `resolve_chunk_map` / `resolve_current_chunk_map` "is the ONE way" to resolve the map, and does not mention the new public entry. (3) `metadata.rs:1267-1270`: `ChunkMap::into_flat` names "a streamed read moving the list into its reader task" as its consumer. That read now destructures the record inside `resolve_chunk_map_owned` and never calls `into_flat`, which still has no caller. Fix: update the three comments, either deleting `into_flat` or dropping the consumer claim from its comment. Optionally, adjust "one call" in `docs/design/architecture/06-runtime-view.md:29`; the logic is still shared, but there are now two public entries.; C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo deny --all-features --config deny-all-features.toml check advisories` failed with exit status: 1. Deferred, not addressed here: 3 finding(s) that need human judgment, held in deferred-findings.json for the human at handover — not build work
- By / date: auto-iterate / 2026-10-14

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 2 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
