# Adversarial review — #1009 (segmented publication, segment-charged budget)

Bottom line: I could not break the fix itself. The red→green proof holds up when I re-run it. What I did find are gaps in the tests: one real production path that no test exercises, a few surviving mutants, and a stale bundle.

## The evidence (re-run at `$PDCA_TARGET`, in a scratch copy)

- **Red→green is real, not a tautology.** C4-verify only reported `PASS (green-only)`, so I ran the red leg myself. I took the pre-fix `crates/core/src/multipart.rs` and `crates/core/src/multipart/complete.rs` and added only a stub `DoubtfulCommit::Publication` variant so the new test file compiles. Without that stub the new file fails with `E0599`. With it, `cargo test -p wyrd-core --test multipart_complete` gives **6 failed / 56 passed**: B (both legs), D3, H2-seg, J-seg and G0-seg fail, and every child-1/child-2 leg passes. With the patch it gives 62/62. The legs drive `complete_upload` over a real `RedbMetadataStore` through the wrapper. That is the production path, not a copy of it.
- **All four of the brief's demonstrated reds reproduce** as mutations of `complete.rs`:
  - attempts restored on a release after segments landed → D3, H2-seg and J-seg fail;
  - the `{seg}` obligation dropped from `release_batch` (`complete.rs:1074`) → D3, H2-seg and J-seg fail;
  - the segment count re-derived as `⌈chunks / MAX_SEG_CHUNKS⌉` at `complete.rs:1494` → B, D3, G0-seg, H2-seg and J-seg fail;
  - an unknown outcome treated as `Conflict` → G0-seg and G0 fail.
- **My own mutations that the tests caught:**
  - `resume_from: 0` always → D3 fails;
  - dropping the session re-read on the fault path → J-seg fails;
  - spending a release round when the session is found at a later cursor → J-seg (read fails) fails;
  - always classifying the in-doubt commit as `Flip` → G0-seg fails.

## Refutation attempts that landed

- NEEDS-HUMAN [impl] — **No test covers resuming from the middle of the segment phase.** That is the case `0 < own.cursor < plan.segments` at `crates/core/src/multipart/complete.rs:1350` (`resume_from: own.cursor`). Every leg stages exactly one segment batch (`SEGMENT_BATCHES = 1`, `crates/core/tests/multipart_complete.rs:4349`), so the only resume ever tested is at the full plan (D3, rounds 2–3).
  - Concrete case: I mutated `:1350` to `resume_from: if own.cursor < plan.segments { 0 } else { own.cursor }`, and **all 62 legs pass**.
  - In production that mutant breaks any round that follows a later batch's `Conflict` (for example a TiKV lock on batch 2 after batch 1 landed). The round would re-send batch 0 requiring the session at cursor 0, lose on the session pin every time, and burn all `R_PUBLISH` rounds. It would end charged with `OperationAborted`.
  - This path is reachable well inside the session ceiling. `MAX_BATCH_OPS` is 500 and each row costs 2 ops, so one batch holds at most about 249 segments, or roughly 39k narrow chunks. `MAX_STAGED_CHUNKS` is 80,896.
  - `OwnFence::follow` (`complete.rs:1057`) following the cursor to a mid-plan position during a release is untested for the same reason.
  - Fix options: add a leg that forces at least two segment batches, or record an explicit `// deferred: #660` for this path. The existing #660 marker covers races, not this sequential resume.
- NEEDS-HUMAN [impl] — **Five C5 mutants survived and the gates don't record why.**
  - The four mutants on the cursor bound at `crates/core/src/multipart.rs:2726` (`written > MAX_SEGMENT_INDEX + 1`) survive because no plan gets near 1,000,000. That leaves the doc comment's claim ("no record it returns is one the decoder refuses") untested: under `>` → `==`, `with_segments_written(1_000_002)` returns a record the decoder at `multipart.rs:2846` refuses. A unit test at `MAX_SEGMENT_INDEX + 1` and `+ 2` kills all four.
  - The `follow` guard mutant at `complete.rs:1062` (`cursor >= self.cursor` → `true`) is equivalent in practice, because no writer moves the cursor backwards. Accepting it needs a written reason.
  - One more of mine survives: `writes_segments = plan.segments > 0` at `complete.rs:1340`. No leg sends an unknown outcome in a resumed round that only sends the flip, so whether that round reports `DoubtfulCommit::Flip` or `DoubtfulCommit::Publication` is never checked.
- NEEDS-HUMAN [impl] — **The bundle is stale, so diff coverage was never measured.** `gate-logs/C4-diff-cov.log` says "patch.diff does not apply on origin/main — the bundle is stale; rebase Do". The patch rewrites the pre-fence arm of `fence_complete` (`complete.rs:807-834`, where `Admit::refuses` is removed). The brief's ordering note assigns that region to child-5 ("the pre-fence and frozen-set checks") and claims no conflict. I can't check this here, because the target has no `origin/main` ref. The builder should rebase, then re-run C4-ci and C4-diff-cov.

## Attempted, could not refute

- **A release from a stale cursor 0 cannot land over written `seg:` records.** Each segment batch moves `segments_written` in the same batch that writes its records. The cursor-0 release requires the fence's cursor-0 bytes (`complete.rs:1074`), so once any batch lands, that release can only `Conflict`. The `follow` call then re-sends it charged, with `{seg}` (J-seg, read-fails leg). So "a fence that wrote none never spends the budget" and "every written `seg:` record is named" both hold.
- **The obligation key can't be taken by anyone else.** `retire:records:s:<id>:<E+1>` is otherwise written only by the flip and by `completing_teardown`, and both move the session too. So a `require_absent` conflict on it with the session unchanged has no legitimate cause.
- **The segment count can't drift.** `planned_segments` and the committer's `plan` both call `split_segments` (`crates/core/src/staged_publication.rs:417-423`, `:545`). Also, `fence_track` refuses any hook that is not a function of the cursor, before any write.
- **The flat path is unchanged.** `Placed::at(0)` re-encodes the fence record byte for byte, and all child-2 flat legs pass.
- **The release loop is bounded.** A round is skipped only when the cursor goes up, and only this Complete's batches move it.
- **`TooManySegments` behind the fence** (a non-answer on every retry) is reachable only with chunk refs near their widest encoding. It belongs to the value-ceiling slice (child-5 / #1011), so I'm not raising it.

## Where the verdict over-claims

- C4-verify's `path_line` says the red can't be isolated because "the test is co-located with the fix". That is wrong: the test is a separate file, `crates/core/tests/multipart_complete.rs`. The real reason is that the modified test file doesn't compile against pre-fix source. The behavioural red I measured above makes this harmless, but "green-only" should not be read as "red unproven".
- `overall: pass` sits on top of a failed diff-coverage row and five unexplained surviving mutants. Both rows are non-gating. Together they are the reason the mid-plan resume gap above went unseen.

<!-- pdca:leaf-complete -->
