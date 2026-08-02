# Result — issue 508 / multipart-upload

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: the full S3 multipart-upload verb set — `CreateMultipartUpload`, `UploadPart`,
- Success criterion: five legs, all over the wire against an in-process gateway. Legs A, B
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope: the six multipart verbs, their records, their publication path and their staged

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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 589 mutants tested in 39m: 216 missed, 85 caught, 286 unviable, 2 timeouts

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 58 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of issue #508’s full S3 multipart-upload verb set, fenced commit/lifecycle protocol, segmented publication, and ordinary-PUT sizing.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | NEEDS-HUMAN | Architecture board must correct proposal 0016’s decision-7 summary from upload-id keying to the independently reserved segment-group nonce — leaving contradictory normative keying risks live-segment overwrite after upload-id reuse (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:2677`). |
| C2 Reproduction (red pre-fix) | PASS | On clean `22d71b4`, both added test binaries compiled and all 16 tests executed and failed by assertion (5 lifecycle, 11 upload), including independent ordinary-PUT sizing reds (`crates/server/tests/s3_multipart_lifecycle.rs:435`, `crates/server/tests/s3_multipart_upload.rs:347`). |
| C3 Change | FAIL | The change cannot meet the bounded-retirement/data-protection contract: production stores inherit the explicitly capped `scan()`-backed pagination default, while GC still treats a segmented inode’s empty inline map as its complete committed reference set (`crates/traits/src/lib.rs:809`, `crates/custodian/src/gc.rs:300`). |
| C4 Verification (red→green) | PASS | The same 16 tests passed with the patch, and `cargo xtask ci` passed in full after moving cargo-deny’s read-only host advisory DB into reviewer scratch; the initial lock error was a host caveat, not a patch failure (`crates/server/tests/s3_multipart_upload.rs:347`, `crates/server/tests/s3_multipart_lifecycle.rs:435`). |
| C5 Causal adequacy | NEEDS-HUMAN | Decide whether the missing-reaper dependency must be verified/refused at startup instead of guarded by a caller-set boolean and first-Create warning — a false assertion admits sessions whose required lifecycle exits do not exist (`crates/server/src/lib.rs:150`, `crates/server/src/lib.rs:184`); the 589-mutant candidate list reproduced, but the unavailable `scripts/mutants-in-diff` wrapper leaves its reported 216 survivors provisional. |
| T1 Structure | FAIL | The advertised shared segmented-map resolver is used by gateway reads but not by GC, restore, reconstruction, or rebalance, so maintenance and serving have structurally different object models (`crates/server/src/multipart.rs:852`, `crates/custodian/src/reconstruction.rs:607`). |
| T2 Shape | NEEDS-HUMAN | Maintainer must choose whether this 7,718-insertion cross-cutting bundle remains reviewable or re-enters Plan for tracked seam splits — the current omissions across stores and maintenance consumers show why that scope decision affects defect risk. |
| T3 Runtime | FAIL | A body/store error after one or more chunks are staged discards the staged set and invokes empty compensation, leaving live-session `sidx:` residue; additionally, a published-parts retirement over 500 parts repeatedly deletes the same first 1,000 keys and never converges (`crates/server/src/multipart.rs:357`, `crates/core/src/write.rs:741`, `crates/core/src/multipart.rs:2045`). |
| T4 Contribution | FAIL | Independent review finds the hard test-fidelity contract unmet: no seeded multipart crash/race DST and no shared-backend `scan_page` conformance were added; the unavailable external `review-branch` wrapper is not the basis for this failure (`AGENTS.md:188`, `crates/metadata-conformance/src/lib.rs:24`). |
| T5 Judgment | NEEDS-HUMAN | Confirm the withheld rejected iteration-v1/v2/v3 artifacts were compared before another build — affected-path merged history and GitHub closed-work search found no competing implementation, but rejected/private prior art could not be mechanically settled and may contain already-settled objections. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Approve only after deploying the full 508→625→633 tree and running `p="${PDCA_SCRATCH:-${TMPDIR:-/tmp}}/pdca-reviewer-508-9g"; truncate -s 9G "$p-src"; aws --endpoint-url "$S3_ENDPOINT" s3 cp "$p-src" "s3://$S3_BUCKET/pdca-review-508-9g"; aws --endpoint-url "$S3_ENDPOINT" s3 cp "s3://$S3_BUCKET/pdca-review-508-9g" "$p-dst"; sha256sum "$p-src" "$p-dst"; rm -f "$p-src" "$p-dst"` and accepting only identical hashes — the deployed topology and large-object dependency were not exercised at Check. |

### Advisory — adversary

# Adversarial review — issue #508 (multipart-upload)

Advisory only; I never gate. Four refutations landed, one scope/fitness call for the human,
and a list of attacks that failed (recorded because they are evidence *for* the patch).

## Refutations

- **NEEDS-HUMAN [impl] — a published *segmented* object is invisible to every maintenance
  consumer, and an operator restore pass then deletes it.** `crates/core/src/multipart.rs:1491-1496`
  publishes a segmented root with `chunk_map: Vec::new()` and the map in `seg:` records, but
  `crates/custodian/src/gc.rs:305` and `crates/custodian/src/restore.rs:397` iterate
  `record.chunk_map` **directly** — and the custodian never reads a `seg:` record anywhere
  (grep for `seg:` across `gc.rs`/`restore.rs`/`desired_state.rs` is empty). The patch's own
  doc at `crates/core/src/metadata.rs:322` says to resolve through `resolve_chunk_map` and
  "never by reading `chunk_map` directly"; the gateway read path obeys
  (`crates/server/src/lib.rs:492,573`), the maintenance plane does not.
  **Concrete failing case, using the patch's own fixture:** `crates/server/tests/s3_multipart_lifecycle.rs:739`
  uploads 6 × 5 MiB parts at a 64 KiB chunk size = 480 chunks > `MAX_MAP_CHUNKS = 165`
  (`crates/core/src/multipart.rs:64`), so it publishes segmented. Once the retirement drain
  deletes that session's `part:` records, its chunks are in neither `ReferenceSet::placed` nor
  `::staged`; `reconcile_after_restore` (`crates/custodian/src/restore.rs:231-286`) then
  orphan-marks **every fragment of that live object**, and the next GC pass past the grace
  window deletes them — the `get_object` at `:818` would return corrupt/short data. Add
  `run_restore(&meta, &fleet, wall_now() + 10 * GRACE)` to that test and it fails today.
  `desired_state.rs:169`'s `holds_any` inherits the same blindness, so a drain of a server
  holding a segmented object's fragments answers `Satisfied` — the F6 wipe trace, this time
  over *committed* data. Nothing catches it: `run_restore` is called only at
  `s3_multipart_lifecycle.rs:485` (an `Open` session), and the segmented test at `:739` runs
  neither `run_gc` nor `run_restore`. This is the brief's C-1 invariant ("a permanent or
  data-losing failure mode is never an acceptable cost") failing on the slice's own new path.

- **NEEDS-HUMAN [impl] — `drain_records`'s paged branch can never make progress; records and
  the obligation are retained forever.** `crates/core/src/multipart.rs:2043-2067`. For
  `RetireRecords::Parts` the key list is derived purely from the stored payload
  (`expand_part_ranges(parts)` → `part_key` + `psum_key`, `:2045-2050`), a list that does not
  shrink as keys are deleted. When `record_keys.len() > B_OPS` the branch deletes
  `record_keys.iter().take(B_OPS)` — always the **same first 1,000 keys** — and returns
  `Ok(false)` **without rewriting the payload and without storing a cursor**. The only `put`s
  of that key are `require_absent`-guarded creations (`:1355,:1401,:1556`), so the payload can
  never change; every later pass recomputes an identical list and re-deletes already-absent
  keys.
  **Concrete failing case:** any Complete naming ≥ 501 parts (2 keys/part > `B_OPS = 1_000`,
  `:93`) — i.e. the brief's own headline deferred leg, `aws s3 cp` of an 8 GB file, which at
  the AWS CLI's default 8 MiB `multipart_chunksize` is ~1,000 parts / ~2,000 keys. Every
  `part:`/`psum:` record past the first 1,000 keys is never deleted and the `retire:records:`
  obligation is never drained. The sibling arms converge precisely because they re-read live
  state (`RetireRecords::Segments` scans `seg:` at `:2053`; `drain_bytes`'s `UnnamedParts`
  filters on `store.get` at `:1937`) — that asymmetry is why this reads as an oversight rather
  than a design. Two aggravators: `report.records_deleted += B_OPS` fires on every futile pass,
  so `run_drain`'s no-progress break (`:1799-1805`) never trips and the counter is simply
  wrong; and each Complete/Abort spawns a detached drain that burns all
  `INLINE_DRAIN_PASSES = 8` passes on this futile work forever
  (`crates/server/src/multipart.rs:40,157-172`). No test reaches it — the suites max out at 6
  parts (`s3_multipart_lifecycle.rs:757`, `s3_multipart_upload.rs:1040`).

- **NEEDS-HUMAN [impl] — `scan_page` ships as a default-only seam, and its own doc asserts two
  things that are false in this tree.** `crates/traits/src/lib.rs:812` states a production
  backend "**overrides** it with a native cursored range read", and `:793-795` states the four
  ordering/cursor/no-skip rules are "asserted on every backend by `metadata-conformance`".
  Neither holds: `scan_page` appears nowhere outside `crates/traits/src/lib.rs` and its single
  caller `crates/core/src/multipart.rs:1824` (no override in `metadata-redb`, `metadata-fdb`,
  `metadata-tikv` or the DST sim store), and `crates/metadata-conformance/src/lib.rs` gained no
  `contract_scan_page_*` — the patch's only `metadata-*` change is a `PendingEntry` constructor
  migration in `crates/metadata-redb/tests/conformance.rs`. So `RedbMetadataStore` inherits the
  default, which calls `self.scan(prefix)`, which returns `Err(ScanCapExceeded)` above the cap
  (`crates/metadata-redb/src/lib.rs:124-129`, `SCAN_CAP = 1 << 20`).
  **Concrete failing case:** a `retire:` namespace grown past 1,048,576 obligations makes
  `drain_step` (`crates/core/src/multipart.rs:1823-1825`) propagate that error with `?` on
  **every** pass, so no obligation is ever drained again — the seam fails in exactly the
  population the brief and the doc say it exists for ("a range that cannot be enumerated is
  bytes and records retained **forever**"). The brief's Scope names all four implementations
  plus the conformance assertion explicitly; the shipped seam is the one line of it that a
  default `impl` makes look done.

- **NEEDS-HUMAN [impl] — leg C(iii)(b) and (c) are neither implemented nor tested, and the
  brief pre-emptively rejected the "does not act" pass they currently get.**
  `crates/custodian/src/scrub.rs:99` builds its work list from `referenced.placed` only and
  never reads `referenced.staged`, so a **corrupt staged fragment is never fetched, never
  verified and never enqueues a repair** — the brief requires the positive ("enqueues a
  repair, not merely 'walks' it"). `crates/custodian/src/reconstruction.rs` contains no
  `staged` / `sidx:` / `part:` handling at all, so reconstruction of a lost staged fragment
  never repoints the part record's placement under the fenced repoint CAS — and the brief
  warned verbatim that "a committed-only implementation that does nothing here passes a 'does
  not silently re-place' oracle — so assert the positive". No test covers either:
  `s3_multipart_lifecycle.rs` implements C(i) (`:436`), C(iii)(a) (`:517`), C(ii) (`:576`) and
  leg D (`:739`, `:903`) and stops. This is the exact outcome the brief predicted: "A patch can
  satisfy legs A, B and C(i)–(ii) in full while failing any of these."

## For the human

- **NEEDS-HUMAN — the slice ships with zero seeded Tier-0 DST coverage, which AGENTS.md makes a
  merge requirement.** The rubric's *Test fidelity* class ("a new destructive or concurrent
  path lands with seeded Tier-0 DST coverage") and the brief's enumerated minimum (publication
  crash points F5/X37/X40, the restore fence over a `Completing` session X57, the
  inode-before-`part:` handoff X67, the drain-vs-intent race X59, the staged re-place losing to
  a session fence X29, and the classification sweep) are both unmet: the only DST change is the
  `PendingEntry::leased` constructor migration in `crates/dst/tests/custodian.rs` (+3/−9, zero
  added test functions). Left unmarked deliberately — six seeded scenarios for a new fenced
  commit protocol is a scope/fitness call (and interacts with the #625 ordering), not a nit to
  bounce straight back to Do. Note also that `check-gates.json` rows C1/C3 are `"none"`, so
  nothing mechanically checked the brief's Scope list against the diff; the four findings above
  are all Scope items that a green C4 cannot see.

## Attacks that failed (recorded as evidence for the patch)

- **The red-leg build-error hazard the brief flagged did not fire.** Both added test files
  reference only base-visible symbols — no `staged_skipped`, `PendingEntry`, `ScanPage`,
  `StagedPlacement`, `InodeRecord::segments`, `holds_any` or `MultipartFault`, and the
  `GcContext` / `reconcile_step` shapes are untouched by the patch. I could not turn
  C4-verify's "red" into a disguised compile failure.
- **`stranded_marked == 0` is not vacuous.** I expected the classic count-that-passes-trivially;
  `crates/server/tests/s3_multipart_lifecycle.rs:496-503` guards it by asserting no committed
  inode exists in the fixture, so a committed-only reference set would have marked everything.
- **The ETag composition matches the settled formula** (`crates/core/src/multipart.rs:799-813`:
  raw digest bytes concatenated, lowercase hex, `-N`). The lenient `unhex` at `:841-852`
  silently drops malformed pairs (two different corrupt digests could collide), but it is not
  client-reachable: `digests` come from the stored `part:` records and the client's ETag must
  equal the stored digest first (`crates/server/src/multipart.rs:719-721`).
- **The *Transactions* rubric item does not apply.** `CommitOutcome` is two-variant
  (`crates/traits/src/lib.rs`), so `== CommitOutcome::Committed` cannot mis-rank a
  `CommitUnknownResult`; that case arrives as an `Err` and is propagated by `?`.
- **Leg E's arithmetic holds.** `chunk_size_for_put` (`crates/core/src/multipart.rs:2156-2167`)
  is a correct ceiling selection with the `EntityTooLarge` refusal, and the configuration
  precondition is genuinely wired at load (`crates/server/src/lib.rs:242`), not just unit-tested.
- **Serialization identity holds.** `skip_serializing_if` on both new `PendingEntry` fields and
  on `InodeRecord::segments` keeps decode→encode the identity for legacy records, which the
  surrounding CAS paths depend on.
- **The detached drain task is defensible** — bounded at 8 passes with re-derivable work
  (`crates/server/src/multipart.rs:151-172`), not the unbounded spawned helper the
  await-discipline rule targets.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C1 Spec — Architecture board must correct proposal 0016’s decision-7 summary from upload-id keying to the independently reserved segment-group nonce — leaving contradictory normative keying risks live-segment overwrite after upload-id reuse (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:2677`).
- [ ] C5 Causal adequacy — Decide whether the missing-reaper dependency must be verified/refused at startup instead of guarded by a caller-set boolean and first-Create warning — a false assertion admits sessions whose required lifecycle exits do not exist (`crates/server/src/lib.rs:150`, `crates/server/src/lib.rs:184`); the 589-mutant candidate list reproduced, but the unavailable `scripts/mutants-in-diff` wrapper leaves its reported 216 survivors provisional.
- [ ] T2 Shape — Maintainer must choose whether this 7,718-insertion cross-cutting bundle remains reviewable or re-enters Plan for tracked seam splits — the current omissions across stores and maintenance consumers show why that scope decision affects defect risk.
- [ ] T5 Judgment — Confirm the withheld rejected iteration-v1/v2/v3 artifacts were compared before another build — affected-path merged history and GitHub closed-work search found no competing implementation, but rejected/private prior art could not be mechanically settled and may contain already-settled objections.
- [ ] Validation — fitness-to-purpose — Approve only after deploying the full 508→625→633 tree and running `p="${PDCA_SCRATCH:-${TMPDIR:-/tmp}}/pdca-reviewer-508-9g"; truncate -s 9G "$p-src"; aws --endpoint-url "$S3_ENDPOINT" s3 cp "$p-src" "s3://$S3_BUCKET/pdca-review-508-9g"; aws --endpoint-url "$S3_ENDPOINT" s3 cp "s3://$S3_BUCKET/pdca-review-508-9g" "$p-dst"; sha256sum "$p-src" "$p-dst"; rm -f "$p-src" "$p-dst"` and accepting only identical hashes — the deployed topology and large-object dependency were not exercised at Check.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 58 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue

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
- Iteration delta (if iterating): Rejected on the strength of the T4 rubric review (58 blocking, gating fail) plus the adversarial review's four concrete, cited defects — fix all of them in the next Do pass: 1. Segmented objects are invisible to the maintenance plane: GC (`crates/custodian/src/gc.rs:305`) and restore (`crates/custodian/src/restore.rs:397`) iterate `record.chunk_map` directly instead of resolving through `resolve_chunk_map`/reading `seg:` records, so a restore pass strands and a later GC pass deletes a live, committed segmented object's fragments. Route every maintenance consumer through the shared segmented-map resolver, not just the gateway read path. 2. `drain_records`'s paged branch (`crates/core/src/multipart.rs:2043-2067`) never converges past `B_OPS` (1,000) keys — it recomputes and re-deletes the same first 1,000 keys forever with no cursor, so any Complete naming ≥501 parts (i.e. the brief's own headline `aws s3 cp` 8GB case at default chunk size) never fully drains. Needs a cursor/rewritten-payload progress mechanism like the sibling arms (`Segments`, `UnnamedParts`) already have. 3. `MetadataStore::scan_page` ships as a default-only shim over `scan()` on all four backends, so it still inherits `SCAN_CAP` and does not escape the bound it exists to escape; no `metadata-conformance` assertions were added either. Needs native cursored implementations (or an honest scope-cut agreed with Plan) plus the conformance tests the brief's Scope names. 4. Legs C(iii)(b) and (c) — scrub and reconstruction over staged fragments — are unimplemented and untested: scrub only reads `referenced.placed`, never `referenced.staged`, so a corrupt staged fragment is never repaired; reconstruction has no staged/`sidx:`/`part:` handling at all. Brief explicitly named these as required positives, not "does nothing" passes. Also carry forward for the rebuild: the zero seeded Tier-0 DST coverage gap (T4 "For the human" item) should be addressed in the same pass rather than deferred again, given it covers the same new destructive/concurrent paths this iteration touches.
- By / date: Eduard Ralph / 2026-07-25

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
