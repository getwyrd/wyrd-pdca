# Result — issue 638 / fragment-write-deadline

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal:
  make `W_write` a **real** bound by giving the fragment-write path an authorization
  deadline **the D server itself enforces**. Today the `ChunkStore` seam cannot express it —
  `put_fragment` takes only `(id, fragment)` (`crates/traits/src/lib.rs:575-577`) and
  `FragmentPutRequest` carries only ID and bytes
  (`crates/proto/proto/wyrd/v0/chunk.proto:25-29`) — so the only bound in existence is the
  client's channel timeout (`crates/chunkstore-grpc/src/client.rs:169-190`), which bounds how long
  the **writer waits**, not when an already-accepted write **takes effect**.
- Success criterion:
  one **NEW** test file, `crates/chunkstore-grpc/tests/write_deadline.rs`,
  driving a **real in-process gRPC D server** (the idiom `crates/chunkstore-grpc/tests/` already
  uses), plus a seeded DST case appended to an existing file (see `Test file`).
  **(A) The server refuses an expired write — at the SERVER, not the client.** Send a
  `put_fragment` whose deadline has already passed, with the **client-side timeout generous** so a
  client-side bound cannot be what produces the refusal. Assert: the RPC is refused with a typed,
  operator-classifiable error, **and the fragment is not stored** — read back through
  `get_fragment` and assert `None`. Asserting only the error would pass an implementation that
  refuses the caller *after* persisting the bytes, which is the exact outcome (a) leak this slice
  exists to prevent.
  **(B) A write parked past its deadline is refused when it is finally processed** — 0016's own
  failure-mode row (`0016:1784`): "authorize a fragment write, park it in the D server's accept
  queue past `W_write`, and assert it is refused". Drive it with a server-side delay/pause seam so
  the deadline elapses **between acceptance and application**; assert refusal and absence. This is
  the leg that distinguishes a server-enforced deadline from a caller timeout, and **a
  client-timeout implementation fails it**: the client is still waiting happily.
  **(C) A live write is unaffected.** A `put_fragment` well inside its deadline stores and reads
  back byte-identical. Without this the slice can pass A and B by refusing everything.
  **(D) Absent deadline ⇒ exactly today's behaviour (additive compatibility).** A request carrying
  **no** deadline field stores normally. Assert it over the wire, because this is what keeps every
  existing writer — the ordinary write path, backfill, reconstruction, rebalance — working
  unchanged. A proto field is additive on the wire; the *trait* change is not source-compatible,
  so state in `build-notes.md` how existing callsites were migrated.
  **(E) The seam means the same thing on every implementation.** `chunkstore-fs`
  (`crates/chunkstore-fs/src/lib.rs:180`) honours the identical contract, so a caller cannot get
  a weaker guarantee by holding a local store. Assert A and C against it too — cheaply, in the
  same file or beside it.
  **(F) The refusal is classifiable, not a bare string.** It must be distinguishable by a caller
  from a genuine backend fault, in the register the seam already uses for typed cross-backend
  errors (`ScanCapExceeded`/`IntegrityFault`/`BlockReadFault` all live in `crates/traits` so every
  backend raises the *same* type — `crates/traits/src/lib.rs:288-300`). A deadline refusal is an
  expected, non-fault outcome and a caller must be able to tell it from "the disk is broken".
  **(G) `cargo xtask ci` green.**
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope:
  the `ChunkStore::put_fragment` seam (`crates/traits`), the additive
  `FragmentPutRequest` field (`crates/proto`), the gRPC client send and the **D-server service
  refusal** (`crates/chunkstore-grpc/src/{client.rs:208, server.rs:66}`), the same contract on
  `chunkstore-fs` (`:180`), the typed refusal class, migration of every existing `put_fragment`
  callsite and test double, and the seeded DST case. / **out of scope:** choosing `W_write`'s
  **value** and the `G_orphan > W_write + δ_clock` margin — **#625** owns the windows; this slice
  ships the *mechanism* and takes the deadline as a parameter. Also out: the multipart records
  (#636), the staged protection class (#637), the S3 verbs (#508), `scan_page` (#634), the
  segmented map (#635), and any file under `docs/design/adr/` or `docs/design/specs/`.

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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 19 mutants tested in 28s: 1 caught, 18 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — ${PDCA_CLI:-.venv/bin/wyrd-pdca} contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Reviewing issue #638: enforce fragment-write authorization deadlines at the D server/store application point while preserving deadline-absent compatibility.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | NEEDS-HUMAN | Decide whether Alpha accepts silent mixed-fleet loss of the safety guarantee or requires capability negotiation—old servers ignore the additive deadline field, so enforcement depends on server version (`crates/proto/proto/wyrd/v0/chunk.proto:37`). |
| C2 Reproduction (red pre-fix) | PASS | Test-only base ran five tests: A/B/F failed by assertion (3 failed, 2 passed), proving the old server accepts deadline-bearing writes; the patched same five pass (`crates/chunkstore-grpc/tests/write_deadline.rs:350`, `crates/chunkstore-grpc/tests/write_deadline.rs:380`). |
| C3 Change | PASS | The specified seam, RPC, backends, docs, and source-breaking callsites are covered; optional tag 3 and `None` compatibility preserve old writers (`crates/proto/proto/wyrd/v0/chunk.proto:29`, `crates/traits/src/lib.rs:647`, `docs/design/architecture/08-crosscutting-concepts.md:106`). |
| C4 Verification (red→green) | PASS | Independent full `cargo xtask ci` is green and mutation rerun matched 1 caught/18 unviable; real gRPC and seeded DST legs pass (`crates/chunkstore-grpc/tests/write_deadline.rs:350`, `crates/dst/tests/network.rs:613`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Rebuild must move or repeat the expiry check at the atomic publish point—the clock read precedes `spawn_blocking`, disk write, and rename, so blocking-pool or I/O delay can still publish after `W_write` (`crates/chunkstore-fs/src/lib.rs:160`, `crates/chunkstore-fs/src/lib.rs:203`, `crates/chunkstore-fs/src/lib.rs:263`). |
| T1 Structure | PASS | Layering remains narrow: the trait owns the contract/type, gRPC translates, the store enforces, and fanout forwards without introducing a concrete-backend dependency (`crates/traits/src/lib.rs:631`, `crates/chunkstore-grpc/src/server.rs:65`). |
| T2 Shape | PASS | The wire shape is additive and presence-sensitive—optional `uint64` tag 3 keeps absence distinct from zero and avoids renumbering (`crates/proto/proto/wyrd/v0/chunk.proto:29`). |
| T3 Runtime | FAIL | New real-network tests leave readiness/RPC awaits unbounded, so a hung regression can stall rather than fail closed, contrary to await discipline and the brief's generous-timeout requirement (`crates/chunkstore-grpc/tests/write_deadline.rs:240`, `crates/chunkstore-grpc/tests/write_deadline.rs:245`, `crates/dst/tests/network.rs:642`). |
| T4 Contribution | NEEDS-HUMAN | Decide how the five reported batch-review blockers will be discharged—the `review-branch` runner is absent here, so that red is provisional; contribcheck is green and the 50-path merged/closed prior-art audit found only unrelated withdrawn PR #336. |
| T5 Judgment | NEEDS-HUMAN [impl] | Rebuild evidence with a post-check/pre-rename delay on `FsChunkStore` and a genuine backend-fault control—the current layer delays before store entry, the DST fake uses a stronger check order, and leg F only compares malformed input (`crates/chunkstore-grpc/tests/write_deadline.rs:297`, `crates/dst/tests/network.rs:131`, `crates/chunkstore-grpc/tests/write_deadline.rs:465`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether mechanism-only evidence is fit for multipart safety after rebuild—the live writers still pass `None` and no #636 caller or `G_orphan` margin is exercised end-to-end, so production purpose is not yet observable (`crates/core/src/write.rs:248`, `docs/design/architecture/08-crosscutting-concepts.md:106`). |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C1 Spec — Decide whether Alpha accepts silent mixed-fleet loss of the safety guarantee or requires capability negotiation—old servers ignore the additive deadline field, so enforcement depends on server version (`crates/proto/proto/wyrd/v0/chunk.proto:37`).
- [ ] C5 Causal adequacy — Rebuild must move or repeat the expiry check at the atomic publish point—the clock read precedes `spawn_blocking`, disk write, and rename, so blocking-pool or I/O delay can still publish after `W_write` (`crates/chunkstore-fs/src/lib.rs:160`, `crates/chunkstore-fs/src/lib.rs:203`, `crates/chunkstore-fs/src/lib.rs:263`).
- [ ] T4 Contribution — Decide how the five reported batch-review blockers will be discharged—the `review-branch` runner is absent here, so that red is provisional; contribcheck is green and the 50-path merged/closed prior-art audit found only unrelated withdrawn PR #336.
- [ ] T5 Judgment — Rebuild evidence with a post-check/pre-rename delay on `FsChunkStore` and a genuine backend-fault control—the current layer delays before store entry, the DST fake uses a stronger check order, and leg F only compares malformed input (`crates/chunkstore-grpc/tests/write_deadline.rs:297`, `crates/dst/tests/network.rs:131`, `crates/chunkstore-grpc/tests/write_deadline.rs:465`).
- [ ] Validation — fitness-to-purpose — Decide whether mechanism-only evidence is fit for multipart safety after rebuild—the live writers still pass `None` and no #636 caller or `G_orphan` margin is exercised end-to-end, so production purpose is not yet observable (`crates/core/src/write.rs:248`, `docs/design/architecture/08-crosscutting-concepts.md:106`).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_

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
- Iteration delta (if iterating): Auto-iterate (round 1): rebuilding for the implementation-level findings — C5 Causal adequacy — Rebuild must move or repeat the expiry check at the atomic publish point—the clock read precedes `spawn_blocking`, disk write, and rename, so blocking-pool or I/O delay can still publish after `W_write` (`crates/chunkstore-fs/src/lib.rs:160`, `crates/chunkstore-fs/src/lib.rs:203`, `crates/chunkstore-fs/src/lib.rs:263`).; T4 Contribution — Decide how the five reported batch-review blockers will be discharged—the `review-branch` runner is absent here, so that red is provisional; contribcheck is green and the 50-path merged/closed prior-art audit found only unrelated withdrawn PR #336.; T5 Judgment — Rebuild evidence with a post-check/pre-rename delay on `FsChunkStore` and a genuine backend-fault control—the current layer delays before store entry, the DST fake uses a stronger check order, and leg F only compares malformed input (`crates/chunkstore-grpc/tests/write_deadline.rs:297`, `crates/dst/tests/network.rs:131`, `crates/chunkstore-grpc/tests/write_deadline.rs:465`).; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_. 1 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-07-26

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
