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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 38 mutants tested in 32s: 7 caught, 31 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 9 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — ${PDCA_CLI:-.venv/bin/wyrd-pdca} contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Reviewing issue #638’s optional, D-server-enforced fragment-write authorization deadline: expired or parked writes must never remain stored, while live and deadline-less writes retain existing behavior.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance surface is decision-complete and falsifiable: server refusal, parked-queue enforcement, live/absent compatibility, local-store parity, typed classification, and whole-repository verification each have an observable outcome. |
| C2 Reproduction (red pre-fix) | PASS | The independent base run executed all 5 tests: the 3 deadline/refusal assertions failed and both live/absent controls passed, with no compile-shaped red (`crates/chunkstore-grpc/tests/write_deadline.rs:386`, `crates/chunkstore-grpc/tests/write_deadline.rs:418`, `crates/chunkstore-grpc/tests/write_deadline.rs:517`). |
| C3 Change | PASS | The change remains within the declared trait/proto/gRPC/filesystem/callsite/DST/docs surfaces, and optional proto3 tag 3 preserves the pre-existing request shape (`crates/proto/proto/wyrd/v0/chunk.proto:25`). |
| C4 Verification (red→green) | PASS | The same 5 tests became 5/5 green; focused fs/gRPC suites, full workspace build/tests, spelling, 98-page docs render/link audit, conformance, statics, DST clippy/tests, cargo-machete, cargo-deny, and 38/38 mutation outcomes also passed—the exact `cargo xtask ci` invocation stopped only on the sandbox’s read-only advisory-db lock, then the identical deny check passed from writable scratch. |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Rebuild must make the expiry verdict atomic and crash-safe with publication—the code publishes first and explicitly admits a reader-visible interval before retraction, so a crash can leave late bytes even though `WriteDeadlineExpired` promises “not applied” (`crates/chunkstore-fs/src/lib.rs:296`, `crates/chunkstore-fs/src/lib.rs:319`, `crates/traits/src/lib.rs:565`). |
| T1 Structure | PASS | The store owns one injected clock for the lifecycle, the shared seam owns only comparison/classification, and the RPC change updates living architecture as required (`crates/chunkstore-fs/src/lib.rs:31`, `crates/traits/src/lib.rs:607`, `docs/design/architecture/08-crosscutting-concepts.md:106`). |
| T2 Shape | PASS | Explicit proto3 presence preserves absence, while one inclusive comparison and one typed cross-backend refusal keep wire and local semantics aligned (`crates/proto/proto/wyrd/v0/chunk.proto:39`, `crates/traits/src/lib.rs:593`). |
| T3 Runtime | FAIL | Publication/retraction is non-atomic: the pre-rename existence snapshot races same-id puts and deletes, and the implementation acknowledges it can retract another acknowledged writer’s fragment; a crash after rename can instead retain the late one (`crates/chunkstore-fs/src/lib.rs:286`, `crates/chunkstore-fs/src/lib.rs:294`, `crates/chunkstore-fs/src/lib.rs:328`). |
| T4 Contribution | NEEDS-HUMAN | Determine whether the harness-reported 9 blocking review findings are valid or settled—the `scripts/review-branch` runner is absent, so that red cannot be rerun; independent merged-history and all 9 closed-unmerged-PR affected-path checks found no deadline prior art, and open PR #645 is the brief’s declared #634 conflict. |
| T5 Judgment | NEEDS-HUMAN [impl] | Rebuild needs seeded destructive/concurrency/crash coverage: the DST parks before entering `FsChunkStore` and therefore exercises admission refusal, while the post-publication tests prove only eventual cleanup and a sequential pre-existing duplicate (`crates/dst/tests/network.rs:586`, `crates/chunkstore-fs/tests/conformance.rs:423`, `crates/chunkstore-fs/tests/conformance.rs:485`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether silent degradation against an old D server is acceptable for Alpha—without capability exchange a mixed-version fleet does not receive the deadline guarantee, which determines whether this mechanism is fit for rollout (`crates/proto/proto/wyrd/v0/chunk.proto:39`). |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Rebuild must make the expiry verdict atomic and crash-safe with publication—the code publishes first and explicitly admits a reader-visible interval before retraction, so a crash can leave late bytes even though `WriteDeadlineExpired` promises “not applied” (`crates/chunkstore-fs/src/lib.rs:296`, `crates/chunkstore-fs/src/lib.rs:319`, `crates/traits/src/lib.rs:565`).
- [ ] T4 Contribution — Determine whether the harness-reported 9 blocking review findings are valid or settled—the `scripts/review-branch` runner is absent, so that red cannot be rerun; independent merged-history and all 9 closed-unmerged-PR affected-path checks found no deadline prior art, and open PR #645 is the brief’s declared #634 conflict.
- [ ] T5 Judgment — Rebuild needs seeded destructive/concurrency/crash coverage: the DST parks before entering `FsChunkStore` and therefore exercises admission refusal, while the post-publication tests prove only eventual cleanup and a sequential pre-existing duplicate (`crates/dst/tests/network.rs:586`, `crates/chunkstore-fs/tests/conformance.rs:423`, `crates/chunkstore-fs/tests/conformance.rs:485`).
- [ ] Validation — fitness-to-purpose — Decide whether silent degradation against an old D server is acceptable for Alpha—without capability exchange a mixed-version fleet does not receive the deadline guarantee, which determines whether this mechanism is fit for rollout (`crates/proto/proto/wyrd/v0/chunk.proto:39`).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 9 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- [ ] C1 Spec — Decide whether Alpha accepts silent mixed-fleet loss of the safety guarantee or requires capability negotiation—old servers ignore the additive deadline field, so enforcement depends on server version (`crates/proto/proto/wyrd/v0/chunk.proto:37`).
- [ ] C1 Spec — Maintainers must decide whether silent deadline loss against an old D server is acceptable for Alpha or requires capability negotiation — a mixed-version fleet otherwise does not receive the promised bound (`crates/chunkstore-grpc/src/client.rs:243`).

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
- Iteration delta (if iterating): Auto-iterate (round 3): rebuilding for the implementation-level findings — C5 Causal adequacy — Rebuild must make the expiry verdict atomic and crash-safe with publication—the code publishes first and explicitly admits a reader-visible interval before retraction, so a crash can leave late bytes even though `WriteDeadlineExpired` promises “not applied” (`crates/chunkstore-fs/src/lib.rs:296`, `crates/chunkstore-fs/src/lib.rs:319`, `crates/traits/src/lib.rs:565`).; T4 Contribution — Determine whether the harness-reported 9 blocking review findings are valid or settled—the `scripts/review-branch` runner is absent, so that red cannot be rerun; independent merged-history and all 9 closed-unmerged-PR affected-path checks found no deadline prior art, and open PR #645 is the brief’s declared #634 conflict.; T5 Judgment — Rebuild needs seeded destructive/concurrency/crash coverage: the DST parks before entering `FsChunkStore` and therefore exercises admission refusal, while the post-publication tests prove only eventual cleanup and a sequential pre-existing duplicate (`crates/dst/tests/network.rs:586`, `crates/chunkstore-fs/tests/conformance.rs:423`, `crates/chunkstore-fs/tests/conformance.rs:485`).; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 9 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_. 2 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-07-26

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
