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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 27 mutants tested in 29s: 3 caught, 24 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 7 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — ${PDCA_CLI:-.venv/bin/wyrd-pdca} contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Reviewing issue #638: add a D-server-enforced fragment-write authorization deadline across the ChunkStore seam, protobuf/gRPC path, filesystem store, and seeded DST coverage.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | NEEDS-HUMAN | Decide whether Alpha may silently lose the guarantee against an old D server or must add capability negotiation—the mixed-version contract changes rollout safety (`crates/chunkstore-grpc/src/client.rs:245`). |
| C2 Reproduction (red pre-fix) | PASS | The base-compatible wire test compiled and ran five tests pre-fix: three assertion failures covered expired, parked, and classification legs while both live/absent controls passed (`crates/chunkstore-grpc/tests/write_deadline.rs:395`). |
| C3 Change | PASS | The mechanism-only scope is covered across the trait, additive wire field, client/service, local store, routing, callsite migration, and living architecture without selecting the policy-owned window values (`crates/traits/src/lib.rs:722`). |
| C4 Verification (red→green) | PASS | Independent red→green, targeted filesystem/gRPC tests, the 50-seed DST, typos, docs render, fmt/clippy/build/tests, machete, deny, conformance, statics, and topology guard all passed; cargo-deny required a scratch advisory DB because the default lock path was read-only (`crates/dst/tests/network.rs:633`). |
| C5 Causal adequacy | NEEDS-HUMAN | Decide whether “checked immediately before rename” satisfies “lands within `W_write`,” or whether publication latency needs its own bound—the implementation admits visibility may occur after expiry, while normative `δ_clock` covers only clock resolution/skew (`crates/chunkstore-fs/src/lib.rs:300`; `docs/design/proposals/draft/0016-multipart-commit-protocol.md:1566`). |
| T1 Structure | PASS | The architecture keeps one acceptor-owned injected clock through admission and publication checks, and the statics gate found no DST-reachable shared mutable global (`crates/chunkstore-fs/src/lib.rs:228`). |
| T2 Shape | PASS | The wire/seam shape preserves absence distinctly with optional tag 3 / `Option<u64>` and supplies a seam-level typed refusal, so compatibility and operator classification are structurally expressible (`crates/proto/proto/wyrd/v0/chunk.proto:40`). |
| T3 Runtime | PASS | Real loopback gRPC and real `FsChunkStore` runtime legs cover expired, parked, live, absent, backend-fault, concurrency, and seeded-network outcomes (`crates/chunkstore-grpc/tests/write_deadline.rs:388`; `crates/dst/tests/network.rs:633`). |
| T4 Contribution | NEEDS-HUMAN | Determine whether the gate-reported seven batch-review blockers are valid or settled—the `scripts/review-branch --bundle` runner is absent, so that red is provisional; affected-path merged and closed/rejected history found no prior server-write-deadline implementation. |
| T5 Judgment | NEEDS-HUMAN [impl] | Rebuild must surface cleanup failure as a backend fault and add the combined regression—an injected unlink denial left one scratch entry while the call returned `WriteDeadlineExpired` because the cleanup error is discarded (`crates/chunkstore-fs/src/lib.rs:311`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the deadline guarantee and silent mixed-version degradation are fit for Alpha operations—the mechanism is exercised, but rollout policy and the deferred `W_write`/`G_orphan` sizing determine whether the production safety claim holds (`docs/design/architecture/08-crosscutting-concepts.md:106`). |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C1 Spec — Decide whether Alpha may silently lose the guarantee against an old D server or must add capability negotiation—the mixed-version contract changes rollout safety (`crates/chunkstore-grpc/src/client.rs:245`).
- [ ] C5 Causal adequacy — Decide whether “checked immediately before rename” satisfies “lands within `W_write`,” or whether publication latency needs its own bound—the implementation admits visibility may occur after expiry, while normative `δ_clock` covers only clock resolution/skew (`crates/chunkstore-fs/src/lib.rs:300`; `docs/design/proposals/draft/0016-multipart-commit-protocol.md:1566`).
- [ ] T4 Contribution — Determine whether the gate-reported seven batch-review blockers are valid or settled—the `scripts/review-branch --bundle` runner is absent, so that red is provisional; affected-path merged and closed/rejected history found no prior server-write-deadline implementation.
- [ ] T5 Judgment — Rebuild must surface cleanup failure as a backend fault and add the combined regression—an injected unlink denial left one scratch entry while the call returned `WriteDeadlineExpired` because the cleanup error is discarded (`crates/chunkstore-fs/src/lib.rs:311`).
- [ ] Validation — fitness-to-purpose — Decide whether the deadline guarantee and silent mixed-version degradation are fit for Alpha operations—the mechanism is exercised, but rollout policy and the deferred `W_write`/`G_orphan` sizing determine whether the production safety claim holds (`docs/design/architecture/08-crosscutting-concepts.md:106`).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 7 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
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
- Iteration delta (if iterating): Auto-iterate (round 4): rebuilding for the implementation-level findings — T4 Contribution — Determine whether the gate-reported seven batch-review blockers are valid or settled—the `scripts/review-branch --bundle` runner is absent, so that red is provisional; affected-path merged and closed/rejected history found no prior server-write-deadline implementation.; T5 Judgment — Rebuild must surface cleanup failure as a backend fault and add the combined regression—an injected unlink denial left one scratch entry while the call returned `WriteDeadlineExpired` because the cleanup error is discarded (`crates/chunkstore-fs/src/lib.rs:311`).; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 7 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_. 4 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-07-26

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
