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
- C4 diff coverage: changed lines executed by the patch's tests: unverifiable — gate exceeded its 7200s timeout
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 20 mutants tested in 2m: 3 caught, 17 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_686/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.32s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of #686: the patch removes the streamed GET's flat chunk-map copy; no implementation defect found, with verification and fitness decisions remaining.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The primary-path allocation invariant is measurable and explicitly excludes restart-path copying; both buffer identity and absence of hidden record copies are required (`brief.md:23`, `brief.md:59`). |
| C2 Reproduction (red pre-fix) | NEEDS-HUMAN | Accept the declared mutation-based substitute for a pre-fix behavioral red — reverting production produces E0432 before any test executes, while reintroducing the copy fails the pointer assertion (`brief.md:106`, `reviewer-red-base.log:175`, `reviewer-copy-mutant.log:17`). |
| C3 Change | PASS | The change addresses the owning streamed GET without altering persisted data or response semantics; framing travels with the served generation and the prior misleading comments are corrected (`target/crates/server/src/lib.rs:432`, `target/crates/core/src/metadata.rs:1606`, `target/crates/core/src/metadata.rs:3018`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Determine the disposition of the existing TiKV advisory failures and incomplete gate evidence — targeted tests pass, but the all-features advisory scan remains red and diff coverage has no completed result (`reviewer-deny-all-features.log:9`, `reviewer-deny-all-features.log:39`, `gate-logs/C4-diff-cov.log:7`). |
| C5 Causal adequacy | PASS | The ownership loss is removed at its source: the entire primary path contains no record/map/list clone or capability guard, and the forbidden-copy mutation is caught (`target/crates/core/src/metadata.rs:3122`, `target/crates/server/src/lib.rs:454`, `reviewer-copy-mutant.log:18`). |
| T1 Structure | PASS | The owning entry retains the existing metadata trait boundary and shares segment validation, ordering, and restart machinery, preserving the resolver's error semantics (`target/crates/core/src/metadata.rs:3010`, `target/crates/core/src/metadata.rs:3146`, `target/crates/core/src/metadata.rs:3156`). |
| T2 Shape | PASS | Framing remains coherent without a partially emptied inode; the new test forbids unsafe code, affected documentation is current, and formatting/spelling/docs checks pass (`target/crates/core/src/metadata.rs:3067`, `target/crates/core/tests/streamed_read_chunk_map_ownership.rs:22`, `target/docs/design/architecture/06-runtime-view.md:29`, `reviewer-scanners.log:5`). |
| T3 Runtime | PASS | Independent execution preserves flat contents/framing, segmented order, restart and deletion behavior; all four ownership tests and 42 tests across the four requested server suites pass (`reviewer-restored-green.log:13`, `reviewer-green-server.log:191`, `reviewer-green-server.log:216`, `reviewer-green-server.log:239`, `reviewer-green-server.log:247`). |
| T4 Contribution | N/A | Commit/PR artifacts are absent by design; their substantive audit must rerun at publish, so this deferred row is neither a pass nor a human clearance item (`gate-logs/T4-contribution.log:10`). |
| T5 Judgment | PASS | The fix stays within the primary-path scope, preserves the explicitly excluded restart copy, and does not repeat the excessive scope of the overlapping rejected prior art (`brief.md:85`, `target/crates/core/src/metadata.rs:3206`, `reviewer-prior-art.log:221`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Confirm that preserving the primary streamed GET's allocations meets #686's performance objective while leaving restart-path optimization out of scope — allocation identity is demonstrated, and the scope boundary is explicit (`target/crates/core/tests/streamed_read_chunk_map_ownership.rs:139`, `brief.md:59`). |

The remaining concerns are verification decisions, not defects introduced by this patch. Source citations above refer to the supplied disposable `$PDCA_TARGET` at `target/`; evidence citations refer to files in this review directory. The target was readable and matched the patch. After the temporary stash and copy mutation, the original patch was restored and `git apply --reverse --check patch.diff` succeeded; no reviewer implementation changes remain.

- **The no-copy claim is supported by execution and source inspection.** The flat fixture owns three chunks with non-empty placements and checks the original outer and first placement pointers, contents, and framing (`target/crates/core/tests/streamed_read_chunk_map_ownership.rs:117`, `target/crates/core/tests/streamed_read_chunk_map_ownership.rs:139`). `committed_inode` returns the decoded record by value, the resolver destructures it, and the server moves the returned list into its reader closure (`target/crates/core/src/read.rs:592`, `target/crates/core/src/metadata.rs:3122`, `target/crates/server/src/lib.rs:468`). No clone of the record hides elsewhere on that primary path. Temporarily replacing the flat answer's `chunks` with `chunks.to_vec()` produces exactly one failing test; restoring it gives four passes (`reviewer-copy-mutant.log:27`, `reviewer-restored-green.log:13`). The shared segment and restart paths remain in use, with distinct live-generation framing checked by the restart test (`target/crates/core/tests/streamed_read_chunk_map_ownership.rs:199`).

- **CI remains incomplete for reasons outside this patch.** The independent `cargo xtask ci` run passed spelling, docs lint/render, repository guards, formatting, workspace clippy/build/tests, and cargo-machete, then stopped on the sandbox's read-only advisory-cache lock (`reviewer-ci.log:4`, `reviewer-ci.log:3234`, `reviewer-ci.log:3240`). Rerunning cargo-deny with copies of the original configs that change only `advisories.db-path` to this directory discharges that host limitation: the default scan passes (`reviewer-deny-default.log:3`), while the all-features scan reproduces RUSTSEC-2026-0346 and RUSTSEC-2026-0349 through rustls 0.21.12 / tonic / tikv-client (`reviewer-deny-all-features.log:9`, `reviewer-deny-all-features.log:24`, `reviewer-deny-all-features.log:39`). The frozen C4-ci log reports the same failures (`gate-logs/C4-ci.log:6478`, `gate-logs/C4-ci.log:6508`). Dependency manifests, lockfile, and advisory policy are unchanged. This is an existing deterministic gate failure requiring disposition, not a C4 patch-defect FAIL. Neither full CI invocation reached its later conformance/statics/DST steps (`target/xtask/src/main.rs:1683`). The brief's external tool dependency, `typos`, was present and actually ran (`reviewer-scanners.log:5`).

- **Every frozen gate row was accounted for.** C4-verify's four green tests and non-executing red are independently reproduced (`gate-logs/C4-verify.log:10`, `reviewer-red-base.log:175`). C4-diff-cov shows four passing core tests, then a 7200-second timeout during the server measurement; it establishes no completed coverage score or server coverage pass (`gate-logs/C4-diff-cov.log:7`, `gate-logs/C4-diff-cov.log:33`). C5-mutants reports three caught and seventeen unviable mutations, not twenty behavioral detections (`gate-logs/C5-mutants.log:13`); the independent forbidden-copy mutation supplements that evidence. T4-batch-review records zero blocking findings, but its supplied log contains only the summary (`gate-logs/T4-batch-review.log:10`). T4-contribution is **N/A**, deferred to the mandatory publish audit (`gate-logs/T4-contribution.log:10`). Host-tikv's successful crate and server feature compilations were independently reproduced with the named clippy commands (`gate-logs/host-tikv.log:112`, `gate-logs/host-tikv.log:211`, `reviewer-tikv.log:105`, `reviewer-server-tikv.log:101`). Instance-root review/mutation/coverage wrappers were adjudicated from their supplied logs; their absence from the target is not a finding.

- **Prior art was checked by affected file path, including closed/rejected work.** GitHub's merged commit history was queried for all five changed paths; the two principal paths contain both `bbdb7c5` (the original move) and `99c7fcf` (the resolver replay), matching the brief (`reviewer-prior-art.log:12`, `reviewer-prior-art.log:43`, `reviewer-prior-art.log:223`, `reviewer-prior-art.log:250`, `reviewer-prior-art.log:253`). All 381 closed PRs were enumerated, and the changed-file lists of all 19 unmerged PRs were compared with those paths. The sole overlap was #647, a broad segmented-map change closed with an explicit request to make it smaller; it does not reject this narrow ownership fix (`reviewer-prior-art.log:68`, `reviewer-prior-art.log:78`, `reviewer-prior-art.log:221`). No unresolved prior-art decision was found.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — issue 686 (streamed GET moves the flat chunk list)

Bottom line: I could not break the fix. The flat arm of `resolve_chunk_map_owned` moves the list and copies nothing, and the red→green proof holds for a real copy. There is one gap a human should rule on: nothing ties the server to the new entry. Swapping the server back to the exact pre-fix copying code keeps every test green. That is how #686 happened in the first place.

## Findings

- NEEDS-HUMAN [human] — The call site is not pinned by any test. `crates/server/src/lib.rs:431-454` is the only place that applies the fix to real GETs. I reverted those lines to the pre-fix code (`metadata::resolve_chunk_map(.., &inode)` plus `resolved.chunks.into_owned()` and `record.etag.clone()` etc.), which brings back the deep copy. All four GET suites the brief names stayed green: `read_path` 4/4, `s3_http_wire` 19/19, `s3_range_conditional` 17/17, `segmented_object_read` 2/2. The new test (`crates/core/tests/streamed_read_chunk_map_ownership.rs`) only calls the core helper, so it can't see the swap. Commit `99c7fcf` lost the move the same way, with a refactor at this call site. So the only guard against a repeat is the comment at `crates/server/src/lib.rs:427-430`. The brief accepted source review for the wiring (`brief.md:44-47`, `brief.md:106-119`), so this is not a defect in the patch. The human should decide whether a comment is enough. The alternative is a server-side pin, which the brief already allows (`brief.md:103-105`: a public server item tested from a new `crates/server/tests/` file).

- The red→green evidence holds for a realistic copy. I changed the flat arm at `crates/core/src/metadata.rs:3141` to `chunks: chunks.to_vec()`. `a_flat_record_hands_over_its_own_chunk_list_by_move` then fails at `crates/core/tests/streamed_read_chunk_map_ownership.rs:139` (`left: 0x75966c00eaf0`, `right: 0x75966c00ee30`). On the unmodified tree all 4 tests pass. The C4-verify result is UNVERIFIABLE because the test doesn't compile on the base (E0432, `gate-logs/C4-verify.log`). The brief declared that outcome up front, and it does not hide anything.

- The test is weaker than it could be, though it still meets the brief. It pins only the first placement's pointer (`crates/core/tests/streamed_read_chunk_map_ownership.rs:132`, `:144`). This mutant stays green on all 4 tests: `chunks.into_iter().enumerate().map(|(i, mut c)| { if i > 0 { c.placement = c.placement.clone(); } c }).collect()`. Rust's in-place `collect` keeps the outer buffer, and placement 0 is left alone. The mutant is contrived, and the brief asked only for the first placement (`brief.md:31-33`), so I am not tagging it. If there is another round, comparing every placement's pointer would close it at almost no cost.

- Source check (ii) from the brief passes. The flat path at `crates/core/src/metadata.rs:3122-3143` takes the record apart by value and moves each field. The server takes `size`, `etag`, `content_type`, `modified` and `chunks` by move (`crates/server/src/lib.rs:445-454`). I found no `.clone()`, `to_owned()`, `into_owned()` or `to_vec()` of an `InodeRecord`, `ChunkMap` or `[ChunkRef]` between the owned record and the reader task. The restart path still copies a flat successor's list at `crates/core/src/metadata.rs:3206`, which the brief puts out of scope (`brief.md:62-66`). One small wording slip: the comment at `crates/core/src/metadata.rs:3159-3160` credits the `'static` lifetime for both `Cow`s being `Owned`. The real reason is that `resolve_current_chunk_map` always builds `Cow::Owned` (`:3206-3207`). The claim is still true today.

- I looked for ways the new entry could behave differently from `resolve_chunk_map`. `resolve_chunk_map_owned` repeats the three-way Answer/Superseded/Gone dispatch (`crates/core/src/metadata.rs:3146-3168` against `:3045-3052`). Every check that matters lives in the shared `read_segments` / `read_group_range` / `root_dropped`, including the `TooManySegments` ceiling at `:2835`. Neither entry checks `state` on the caller's own snapshot, so dropping `state` changes nothing. I found no input today where the two entries give different answers.

- Earlier-round findings are fixed, and I found no new stale comments. `ChunkMap::into_flat` is deleted, and no `.rs` file in the tree (including `wyrd-dst`, which the `cargo xtask ci` build skips with `--exclude wyrd-dst`) still refers to it. The comments at `crates/core/src/metadata.rs:1605-1611` and `:3018-3023` and `crates/core/src/read.rs:575-577` now describe both entries. `docs/design/architecture/06-runtime-view.md:29` now says "one resolver". A search for "streamed read" / `resolve_chunk_map` turned up no remaining claim that the streamed read borrows.

- The red C4-ci gate is not caused by this diff. It fails on RUSTSEC-2026-0346 and RUSTSEC-2026-0349 (security advisories) against `rustls 0.21.12`, which comes in through `tikv-client 0.4.0` (`gate-logs/C4-ci.log:3472-3549`). The patch touches no `Cargo.toml` or `Cargo.lock`. fmt, clippy, build and the whole test run passed, including the 4 new tests (`gate-logs/C4-ci.log:1296-1302`) and the four named server GET suites. C4-diff-cov timed out at 7200 s, which is an environment limit and tells us nothing about the fix.

Refutation attempts that did not land: a copy of the outer list (caught), a record clone hidden behind the pointer test (none in the source), different behaviour from `resolve_chunk_map` on the segmented, superseded and deleted paths (none found), a stale `into_flat` caller anywhere in the workspace (none), and a stale doc comment (none left).
<!-- pdca:leaf-complete -->

### Advisory — code-review

- No introduced correctness findings. The owned resolver moves the flat chunk list and framing fields (`crates/core/src/metadata.rs:3122`), and streamed GET passes that list into its reader without cloning it (`crates/server/src/lib.rs:432`, `crates/server/src/lib.rs:454`). The regression test passes a non-empty record by value and checks both buffer pointers plus contents (`crates/core/tests/streamed_read_chunk_map_ownership.rs:134`). Segmented resolution preserves error propagation, live-generation framing, restart, and deletion behavior (`crates/core/src/metadata.rs:3146`).
- No actionable reuse, simplification, or efficiency findings. Both resolver entries share segment assembly (`crates/core/src/metadata.rs:3010`); the owned entry reuses segment validation and the existing restart resolver (`crates/core/src/metadata.rs:3146`, `crates/core/src/metadata.rs:3156`). The primary flat path neither clones the record nor copies its chunk list or placements.

Validation used read-only source inspection and frozen gate evidence; no tests were rerun. The four new tests and all four GET suites named in the brief passed. Overall CI remains failed on dependency advisories outside this diff; base-revert verification is unverifiable because the new API is absent, as anticipated in the brief, and diff coverage timed out. This clean advisory result does not override those gate outcomes.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C2 Reproduction (red pre-fix) — Accept the declared mutation-based substitute for a pre-fix behavioral red — reverting production produces E0432 before any test executes, while reintroducing the copy fails the pointer assertion (`brief.md:106`, `reviewer-red-base.log:175`, `reviewer-copy-mutant.log:17`).
- [ ] C4 Verification (red→green) — Determine the disposition of the existing TiKV advisory failures and incomplete gate evidence — targeted tests pass, but the all-features advisory scan remains red and diff coverage has no completed result (`reviewer-deny-all-features.log:9`, `reviewer-deny-all-features.log:39`, `gate-logs/C4-diff-cov.log:7`).
- [ ] Validation — fitness-to-purpose — Confirm that preserving the primary streamed GET's allocations meets #686's performance objective while leaving restart-path optimization out of scope — allocation identity is demonstrated, and the scope boundary is explicit (`target/crates/core/tests/streamed_read_chunk_map_ownership.rs:139`, `brief.md:59`).
- [ ] The call site is not pinned by any test. `crates/server/src/lib.rs:431-454` is the only place that applies the fix to real GETs. I reverted those lines to the pre-fix code (`metadata::resolve_chunk_map(.., &inode)` plus `resolved.chunks.into_owned()` and `record.etag.clone()` etc.), which brings back the deep copy. All four GET suites the brief names stayed green: `read_path` 4/4, `s3_http_wire` 19/19, `s3_range_conditional` 17/17, `segmented_object_read` 2/2. The new test (`crates/core/tests/streamed_read_chunk_map_ownership.rs`) only calls the core helper, so it can't see the swap. Commit `99c7fcf` lost the move the same way, with a refactor at this call site. So the only guard against a repeat is the comment at `crates/server/src/lib.rs:427-430`. The brief accepted source review for the wiring (`brief.md:44-47`, `brief.md:106-119`), so this is not a defect in the patch. The human should decide whether a comment is enough. The alternative is a server-side pin, which the brief already allows (`brief.md:103-105`: a public server item tested from a new `crates/server/tests/` file).
- [ ] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [ ] C4 diff coverage: changed lines executed by the patch's tests unverifiable — gate exceeded its 7200s timeout
- [ ] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo deny --all-features --config deny-all-features.toml check advisories` failed with exit status: 1
- [ ] The ownership oracle can pass without removing the reported cost. `brief.md:23-28` checks that the returned chunk buffer is the input buffer, but the existing result also carries a complete `InodeRecord` (`$PDCA_TARGET/crates/core/src/metadata.rs:2688-2693`), whose derived `Clone` includes `chunk_map` (`:1610-1622`). An ownership-taking implementation could clone the record for `resolved.record`, then move the original vector into `resolved.chunks`: pointer identity and contents would pass while every chunk and placement vector still gets copied. Revise the criterion to require no full-map clone anywhere in the owned flat resolution/handoff, including preservation of the returned record, and make that an explicit source-review check. Require a nonempty chunk vector with nonempty placements for the pointer test; an empty vector also defeats the “any resolution that copies … returns a fresh allocation” claim at `brief.md:36-38`.
- [ ] The restored invariant and the permitted scope disagree about restarted reads. `brief.md:42-49` requires that an owning consumer moves its list and says a restart “needs no copy either”; `brief.md:68-69` explicitly makes removing the restart copy optional. The target really does retain that copy: `resolve_current_chunk_map` calls `chunks.into_owned()` and separately retains the full owned record (`$PDCA_TARGET/crates/core/src/metadata.rs:3088-3092`); a flat successor supplies borrowed chunks (`:3007`). A primary-path-only fix can therefore satisfy the success criterion while violating the stated restored invariant. Narrow the invariant to the non-restarted streamed GET, or explicitly include the restart optimization and corresponding verification in scope.
- [ ] The gating `C4-ci` red is not caused by this diff. It fails at `cargo deny --all-features --config deny-all-features.toml check advisories`, on RUSTSEC-2026-0346 and RUSTSEC-2026-0349 against `rustls 0.21.12`, which only `tikv-client 0.4.0` pulls in (`gate-logs/C4-ci.log:3470-3548`). The patch changes none of `Cargo.lock`, `Cargo.toml`, `deny.toml` or `deny-all-features.toml`. This is the same chain as the existing #543 ignore entries (`deny-all-features.toml:103-152`), so `main` fails the same way. Because `cargo xtask ci` stops at the first failure (`xtask/src/main.rs:1683-1687`), the conformance, statics, deploy-guard and DST steps never ran in this gate. I ran them on the patched tree: `xtask statics` passed, `xtask conformance` passed (5 valid + 6 invalid vectors), and `xtask dst` passed (exit 0; custodian 33/33, all other suites green). The deploy guard can't trip because the diff adds no imports. A human has to decide whether to waive this red for #686 or land the new advisory ignores on `main` under #543 first. Either way that is out of this diff's scope.

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
- Iteration delta (if iterating): Auto-iterate (round 2): rebuilding for the implementation-level findings — C2 Reproduction (red pre-fix) — Accept the declared mutation-based substitute for a pre-fix behavioral red — reverting production produces E0432 before any test executes, while reintroducing the copy fails the pointer assertion (`brief.md:106`, `reviewer-red-base.log:175`, `reviewer-copy-mutant.log:17`).; C4 Verification (red→green) — Determine the disposition of the existing TiKV advisory failures and incomplete gate evidence — targeted tests pass, but the all-features advisory scan remains red and diff coverage has no completed result (`reviewer-deny-all-features.log:9`, `reviewer-deny-all-features.log:39`, `gate-logs/C4-diff-cov.log:7`).; C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo deny --all-features --config deny-all-features.toml check advisories` failed with exit status: 1. Deferred, not addressed here: 3 finding(s) that need human judgment, held in deferred-findings.json for the human at handover — not build work
- By / date: auto-iterate / 2026-10-14

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 2 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
