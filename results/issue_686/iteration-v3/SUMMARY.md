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
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 100.0% — 74 of 74 instrumentable changed lines executed (floor 80%); 74 of 162 changed lines were instrume
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 20 mutants tested in 2m: 3 caught, 17 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_686/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.56s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

The #686 patch eliminates the primary streamed-GET flat chunk-map copy while preserving framing and segmented reads; no implementation defect was found, but reproduction and verification exceptions require human sign-off.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The allocation-identity requirement is falsifiable and explicitly excludes restart-path copying; non-empty chunks and placements prevent a vacuous pointer test (`brief.md:23`, `brief.md:62`, `crates/core/tests/streamed_read_chunk_map_ownership.rs:117`). |
| C2 Reproduction (red pre-fix) | NEEDS-HUMAN | Accept the declared mutation-based substitute for a pre-fix behavioral red — reverting production yields E0432 before tests execute, while independently reintroducing the copy fails the pointer assertion (`brief.md:106`, `reviewer-red-base.log:175`, `reviewer-copy-mutant.log:17`). |
| C3 Change | PASS | The actual streamed GET transfers its owned record through the tested entry and retains the served generation's framing; inspection found no hidden record, map, or list copy on the primary flat path (`crates/server/src/lib.rs:432`, `crates/server/src/lib.rs:445`, `crates/core/src/metadata.rs:3122`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Determine the disposition of the existing TiKV advisory failures and require completion of full CI — focused tests pass, but the independent scan reproduces RUSTSEC-2026-0346/0349 in unchanged dependencies (`reviewer-deny-all-features.log:9`, `reviewer-deny-all-features.log:39`, `gate-logs/C4-ci.log:6549`, `Cargo.lock:3294`). |
| C5 Causal adequacy | PASS | Ownership transfer removes the copying cause without a capability probe or symptom guard; source inspection closes the hidden-record-clone loophole and the copy mutant is caught (`crates/core/src/metadata.rs:3134`, `crates/server/src/lib.rs:454`, `reviewer-copy-mutant.log:18`). |
| T1 Structure | PASS | Shared segment validation, ordering, and retirement arbitration remain authoritative through the narrow metadata-store seam; the owned entry introduces no backend dependency or alternate parser (`crates/core/src/metadata.rs:2938`, `crates/core/src/metadata.rs:3010`, `crates/core/src/metadata.rs:3146`). |
| T2 Shape | PASS | The answer carries coherent framing without retaining an emptied or cloned inode; comments now distinguish borrowed and owning consumers, and the unused owning helper is removed (`crates/core/src/metadata.rs:3067`, `crates/core/src/metadata.rs:3018`, `crates/core/src/read.rs:575`, `docs/design/architecture/06-runtime-view.md:29`). |
| T3 Runtime | PASS | Independent runs preserve flat ownership, segment order, successor framing, and deletion behavior; all four ownership tests and all 42 tests in the required GET suites pass (`reviewer-restored-green.log:13`, `reviewer-get-suites.log:191`, `reviewer-get-suites.log:216`, `reviewer-get-suites.log:239`, `reviewer-get-suites.log:247`). |
| T4 Contribution | N/A | Contribution artifacts are absent by design at Check; their substantive audit must rerun at publish, as the deferred gate explicitly records (`gate-logs/T4-contribution.log:10`). |
| T5 Judgment | PASS | The repair stays within primary-path ownership and respects the prior rejection of a broad segmented-map change; merged history and closed/unmerged work were checked by affected file path (`reviewer-prior-art.log:2`, `reviewer-prior-art.log:5`, `brief.md:85`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the proven primary-path allocation guarantee satisfies the operational objective with restart-path copying explicitly excluded — pointer identity and source inspection establish the scoped property, while the restart still copies (`crates/core/tests/streamed_read_chunk_map_ownership.rs:139`, `crates/core/src/metadata.rs:3206`, `brief.md:62`). |

Source citations resolve under `$PDCA_TARGET`; evidence citations resolve in this review directory. The disposable target's base matches the patch preimages, and all five final files match the patch's resulting blob hashes (`reviewer-target-integrity.log:1`). The stash and temporary copy mutation were restored completely. No stale-target caveat applies.

Verification supports the change but does not establish a green full gate:

- The independent `cargo xtask ci` run passes `typos`, documentation lint/render, repository guards, formatting, clippy, build, workspace tests, and dependency-use checks. It then encounters a sandbox restriction on the shared advisory database lock (`reviewer-ci.log:4`, `reviewer-ci.log:3240`). This is a host fault, not a patch defect. The brief's external `typos` dependency was exercised.
- Repeating the dependency scans with a local copy of the advisory database and unchanged policy reproduces the frozen all-features advisory failure. Default dependency checks and all-features licenses/bans/sources pass (`reviewer-deny-default.log:3`, `reviewer-deny-all-licenses.log:3`). Only the database location changed; no advisory was suppressed. These dependencies and policies are outside this diff, so this is a verification disposition for the human, not an implementation finding against #686.
- Conformance and the shared-state scanner pass independently (`reviewer-conformance.log:3`, `reviewer-statics.log:5`). CI's later orchestration/DST stages were not reached by either full run; no full-CI pass is claimed (`xtask/src/main.rs:1683`).

The instance-scoped wrapper results are adjudicated from their captured logs. Diff coverage reports 74/74 instrumentable changed lines covered, with 88 other changed lines unscored (`gate-logs/C4-diff-cov.log:641`). Mutation testing reports three caught and seventeen unviable mutants, not twenty behaviorally tested mutants (`gate-logs/C5-mutants.log:13`); the independent copy mutation supplies the relevant ownership discriminator. The batched review reports zero findings (`gate-logs/T4-batch-review.log:10`), and the TiKV gate records real feature-enabled clippy completion for the crate and server (`gate-logs/host-tikv.log:2`, `gate-logs/host-tikv.log:211`). These recorded passes do not override the red advisory gate.

Prior-art investigation found the original move (`bbdb7c5`) and resolver replacement (`99c7fcf`) in path-filtered merged history. Among all 19 closed-unmerged PRs returned, #647 was the only affected-path match; its closing comment required a smaller change. The present ownership repair satisfies that scope distinction. The closed-PR search for #686 returned no results (`reviewer-prior-art.log:3`, `reviewer-prior-art.log:6`, `reviewer-prior-art.log:8`).

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — issue 686 (streamed GET moves the flat chunk map)

**Verdict: could not refute.** I tried to break the evidence, the fix, and the verdict. I re-ran the proof on a scratch copy of the patched tree (`cargo test -p wyrd-core --test streamed_read_chunk_map_ownership`) and tried three mutants. None of it turned up a failing case on the primary path. One gating red remains, and the diff did not cause it.

## Attempts

- **The red→green evidence holds, and I reproduced it myself.** On the patched tree the new suite passes 4/4. I changed the flat arm at `crates/core/src/metadata.rs:3141` from `chunks` to `chunks.to_vec()`. `a_flat_record_hands_over_its_own_chunk_list_by_move` then fails at `crates/core/tests/streamed_read_chunk_map_ownership.rs:139` with two different buffer addresses (`left: 0x74ef9c014b10`, `right: 0x74ef9c014e50`). The other three tests stay green. The test calls `resolve_chunk_map_owned`, the same function production calls at `crates/server/src/lib.rs:432`, not a copy of it. The fixture is non-empty, with non-empty placements (`:38-49`), so the pointer check cannot pass by accident on a dangling empty-`Vec` pointer. C4-verify's UNVERIFIABLE (exit 77, `gate-logs/C4-verify.log`) is the E0432 that `brief.md:106-119` declared ahead of time. It is not a missing red.
- **The hidden record clone (check ii) is a real blind spot in the test, but the source has no such clone.** I added `let _hidden_copy = std::hint::black_box(record.clone());` at the top of `resolve_chunk_map_owned` (`crates/core/src/metadata.rs:3121`). All 4 tests still pass, as `brief.md:34-40` predicted, so only source review guards against this. I walked the owned flat path end to end:
  - `committed_inode` returns the record by value (`crates/core/src/read.rs:577-592`; it only reads and checks state).
  - The server moves it in (`crates/server/src/lib.rs:432`).
  - The function destructures it and moves `chunks` straight into the answer (`crates/core/src/metadata.rs:3122-3143`).
  - The server moves the framing fields and `chunks` out (`crates/server/src/lib.rs:445-454`).
  - The reader task only borrows the list (`crates/server/src/lib.rs:474-476`).

  I found no `.clone()`, `to_owned()`, `into_owned()` or `to_vec()` of an `InodeRecord`, `ChunkMap` or `[ChunkRef]` anywhere on that path. The framing travels as separate fields (`OwnedResolvedChunkMap`, `metadata.rs:3066-3080`), so the record is never kept whole.
- **A server regression would pass every test.** Suppose `get_object_streaming` went back to `resolve_chunk_map(&inode)` plus `resolved.chunks.into_owned()`. The new test only covers the core function, and the server GET suites (`crates/server/tests/segmented_object_read.rs:159,172,227,244`) only check bytes, so all of them would stay green. The brief already declares this limit (`brief.md:106-115`: a counting allocator would need `unsafe`, which the repo forbids), and the move at `lib.rs:432` is plainly visible in source. So this is a known gap, not a defect in the patch.
- **The segmented, superseded and deleted arms behave the same as the borrowing resolver.**
  - `resolve_chunk_map_owned` (`metadata.rs:3146-3168`) handles the same three outcomes of `read_segments` as `resolve_chunk_map` / `resolve_snapshot` (`:2995-3003`, `:3045-3052`), with the same `?` error path.
  - The fail-closed arm runs through the server's streamed GET in `an_unresolvable_object_fails_closed_without_ending_another_objects_read` (`crates/server/tests/segmented_object_read.rs:211-227`). It passes in the frozen CI log (`gate-logs/C4-ci.log`, around line 2795).
  - The superseded test (`streamed_read_chunk_map_ownership.rs:184-205`) gives the stale and live generations different `version`, `etag` and `modified` values. So framing the answer from the stale snapshot would be caught, and so would treating an overwrite as a delete (`.expect("an overwrite is not a deletion")`).
  - I could not build an input where the owned path gives a different answer from the borrowing one.
- **Removing `ChunkMap::into_flat` breaks nothing.** I grepped the whole tree, including the custodian, DST, tikv and fdb code and the docs, and found no remaining reference. The `host-tikv` gate passes (`gate-logs/host-tikv.log`).
- **One comment gives a loose reason, but its claim is true (minor, not a defect).** `crates/core/src/metadata.rs:3159-3160` says both `Cow`s are `Owned` "(`ResolvedChunkMap<'static>`)". A `'static` lifetime does not by itself rule out `Cow::Borrowed`. The claim holds because `resolve_current_chunk_map` always builds `Cow::Owned` (`:3206-3207`). That restart path still copies a flat successor's list and then drops the original inside `framed_by` (`:3085-3094`). `brief.md:62-66` puts that copy out of scope, so I am not raising it.
- **The reviewer's verdict.** I found no claim in `check-gates.json` that the evidence doesn't support. Diff coverage reports 74 of 74 instrumentable lines executed. Mutation testing caught 3 mutants, 17 were unviable (did not compile), and none survived. The T4 batch review found 0 blocking issues. The only gating red is described in the next bullet.
- NEEDS-HUMAN [human] — The one gating failure, `C4-ci` (`check-gates.json` row `C4-ci`), is not caused by this diff, and another rebuild cannot fix it. `gate-logs/C4-ci.log:6471-6549` shows `cargo deny --all-features ... check advisories` failing on RUSTSEC-2026-0346 and RUSTSEC-2026-0349. Both are advisories against `rustls v0.21.12`, which comes in through `tokio-rustls 0.24.1 → tonic 0.10.2 → tikv-client 0.4.0 → wyrd-metadata-tikv`. The patch changes no `Cargo.toml`, `Cargo.lock` or `deny*.toml` (`git diff --stat` on those files is empty). Every earlier step in the same run is green: fmt, clippy, build, the full workspace test run (including the four GET suites the brief names), machete and plain `cargo deny check`. Two auto-iterations (brief.md:144-154) have already rebuilt for this same red. A human needs to choose: accept this patch with that red recorded as unrelated, or fix the tikv/rustls advisory under its own issue. Please don't route it back to Do again.

<!-- pdca:leaf-complete -->

### Advisory — code-review

- No introduced correctness defects found. The owned flat path moves the original chunk list and framing fields (`crates/core/src/metadata.rs:3122`), and the streamed GET moves them into its response and reader (`crates/server/src/lib.rs:432`, `crates/server/src/lib.rs:454`). Restart framing and terminal deletion behavior are preserved (`crates/core/src/metadata.rs:3155`). The regression test checks non-empty outer and placement buffers plus unchanged contents (`crates/core/tests/streamed_read_chunk_map_ownership.rs:115`).
- No actionable reuse, simplification, or efficiency findings. Both resolver entries share segment assembly (`crates/core/src/metadata.rs:3010`) and the existing segment reader; the owned entry delegates restarts to the existing current-root resolver (`crates/core/src/metadata.rs:3146`). No record or chunk-list copy occurs on the primary flat path.

Validation: reviewed the supplied frozen gate evidence; no builds were rerun. The four new tests and all four GET suites named in the brief passed; instrumentable diff coverage was 100%. Full CI remains failed on unchanged TiKV/rustls dependency advisories. The pre-fix verification leg is UNVERIFIABLE because the new API is absent on the reverted base, as declared in the brief. These existing gate outcomes are not additional findings against this diff.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C2 Reproduction (red pre-fix) — Accept the declared mutation-based substitute for a pre-fix behavioral red — reverting production yields E0432 before tests execute, while independently reintroducing the copy fails the pointer assertion (`brief.md:106`, `reviewer-red-base.log:175`, `reviewer-copy-mutant.log:17`).
- [ ] C4 Verification (red→green) — Determine the disposition of the existing TiKV advisory failures and require completion of full CI — focused tests pass, but the independent scan reproduces RUSTSEC-2026-0346/0349 in unchanged dependencies (`reviewer-deny-all-features.log:9`, `reviewer-deny-all-features.log:39`, `gate-logs/C4-ci.log:6549`, `Cargo.lock:3294`).
- [ ] Validation — fitness-to-purpose — Decide whether the proven primary-path allocation guarantee satisfies the operational objective with restart-path copying explicitly excluded — pointer identity and source inspection establish the scoped property, while the restart still copies (`crates/core/tests/streamed_read_chunk_map_ownership.rs:139`, `crates/core/src/metadata.rs:3206`, `brief.md:62`).
- [ ] The one gating failure, `C4-ci` (`check-gates.json` row `C4-ci`), is not caused by this diff, and another rebuild cannot fix it. `gate-logs/C4-ci.log:6471-6549` shows `cargo deny --all-features ... check advisories` failing on RUSTSEC-2026-0346 and RUSTSEC-2026-0349. Both are advisories against `rustls v0.21.12`, which comes in through `tokio-rustls 0.24.1 → tonic 0.10.2 → tikv-client 0.4.0 → wyrd-metadata-tikv`. The patch changes no `Cargo.toml`, `Cargo.lock` or `deny*.toml` (`git diff --stat` on those files is empty). Every earlier step in the same run is green: fmt, clippy, build, the full workspace test run (including the four GET suites the brief names), machete and plain `cargo deny check`. Two auto-iterations (brief.md:144-154) have already rebuilt for this same red. A human needs to choose: accept this patch with that red recorded as unrelated, or fix the tikv/rustls advisory under its own issue. Please don't route it back to Do again.
- [ ] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [ ] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo deny --all-features --config deny-all-features.toml check advisories` failed with exit status: 1
- [ ] The ownership oracle can pass without removing the reported cost. `brief.md:23-28` checks that the returned chunk buffer is the input buffer, but the existing result also carries a complete `InodeRecord` (`$PDCA_TARGET/crates/core/src/metadata.rs:2688-2693`), whose derived `Clone` includes `chunk_map` (`:1610-1622`). An ownership-taking implementation could clone the record for `resolved.record`, then move the original vector into `resolved.chunks`: pointer identity and contents would pass while every chunk and placement vector still gets copied. Revise the criterion to require no full-map clone anywhere in the owned flat resolution/handoff, including preservation of the returned record, and make that an explicit source-review check. Require a nonempty chunk vector with nonempty placements for the pointer test; an empty vector also defeats the “any resolution that copies … returns a fresh allocation” claim at `brief.md:36-38`.
- [ ] The restored invariant and the permitted scope disagree about restarted reads. `brief.md:42-49` requires that an owning consumer moves its list and says a restart “needs no copy either”; `brief.md:68-69` explicitly makes removing the restart copy optional. The target really does retain that copy: `resolve_current_chunk_map` calls `chunks.into_owned()` and separately retains the full owned record (`$PDCA_TARGET/crates/core/src/metadata.rs:3088-3092`); a flat successor supplies borrowed chunks (`:3007`). A primary-path-only fix can therefore satisfy the success criterion while violating the stated restored invariant. Narrow the invariant to the non-restarted streamed GET, or explicitly include the restart optimization and corresponding verification in scope.
- [ ] The gating `C4-ci` red is not caused by this diff. It fails at `cargo deny --all-features --config deny-all-features.toml check advisories`, on RUSTSEC-2026-0346 and RUSTSEC-2026-0349 against `rustls 0.21.12`, which only `tikv-client 0.4.0` pulls in (`gate-logs/C4-ci.log:3470-3548`). The patch changes none of `Cargo.lock`, `Cargo.toml`, `deny.toml` or `deny-all-features.toml`. This is the same chain as the existing #543 ignore entries (`deny-all-features.toml:103-152`), so `main` fails the same way. Because `cargo xtask ci` stops at the first failure (`xtask/src/main.rs:1683-1687`), the conformance, statics, deploy-guard and DST steps never ran in this gate. I ran them on the patched tree: `xtask statics` passed, `xtask conformance` passed (5 valid + 6 invalid vectors), and `xtask dst` passed (exit 0; custodian 33/33, all other suites green). The deploy guard can't trip because the diff adds no imports. A human has to decide whether to waive this red for #686 or land the new advisory ignores on `main` under #543 first. Either way that is out of this diff's scope.
- [ ] The call site is not pinned by any test. `crates/server/src/lib.rs:431-454` is the only place that applies the fix to real GETs. I reverted those lines to the pre-fix code (`metadata::resolve_chunk_map(.., &inode)` plus `resolved.chunks.into_owned()` and `record.etag.clone()` etc.), which brings back the deep copy. All four GET suites the brief names stayed green: `read_path` 4/4, `s3_http_wire` 19/19, `s3_range_conditional` 17/17, `segmented_object_read` 2/2. The new test (`crates/core/tests/streamed_read_chunk_map_ownership.rs`) only calls the core helper, so it can't see the swap. Commit `99c7fcf` lost the move the same way, with a refactor at this call site. So the only guard against a repeat is the comment at `crates/server/src/lib.rs:427-430`. The brief accepted source review for the wiring (`brief.md:44-47`, `brief.md:106-119`), so this is not a defect in the patch. The human should decide whether a comment is enough. The alternative is a server-side pin, which the brief already allows (`brief.md:103-105`: a public server item tested from a new `crates/server/tests/` file).

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
- Iteration delta (if iterating): Auto-iterate (round 3): rebuilding for the implementation-level findings — C2 Reproduction (red pre-fix) — Accept the declared mutation-based substitute for a pre-fix behavioral red — reverting production yields E0432 before tests execute, while independently reintroducing the copy fails the pointer assertion (`brief.md:106`, `reviewer-red-base.log:175`, `reviewer-copy-mutant.log:17`).; C4 Verification (red→green) — Determine the disposition of the existing TiKV advisory failures and require completion of full CI — focused tests pass, but the independent scan reproduces RUSTSEC-2026-0346/0349 in unchanged dependencies (`reviewer-deny-all-features.log:9`, `reviewer-deny-all-features.log:39`, `gate-logs/C4-ci.log:6549`, `Cargo.lock:3294`).; C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo deny --all-features --config deny-all-features.toml check advisories` failed with exit status: 1. Deferred, not addressed here: 3 finding(s) that need human judgment, held in deferred-findings.json for the human at handover — not build work
- By / date: auto-iterate / 2026-10-14

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 2 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
