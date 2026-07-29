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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 47 mutants tested in 37s: 11 caught, 36 unviable

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

Reviewing issue #638: make fragment-write authorization deadlines enforceable at every `ChunkStore`/D-server boundary without changing deadline-less writes.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | NEEDS-HUMAN | Choose whether Alpha may silently lose the bound against an old D server or requires capability negotiation — this determines whether a mixed-version deployment can claim `W_write` (`crates/proto/proto/wyrd/v0/chunk.proto:56`). |
| C2 Reproduction (red pre-fix) | PASS | On the base, all five tests ran and the three deadline legs failed by assertion while two controls passed, establishing an observable pre-fix red (`crates/chunkstore-grpc/tests/write_deadline.rs:389`). |
| C3 Change | FAIL | A publication may complete after the deadline and remain durable as `Unknown`, so the change does not implement the specified “within `W_write` or never” state transition (`crates/chunkstore-fs/src/lib.rs:389`; `docs/design/proposals/draft/0016-multipart-commit-protocol.md:1561`). |
| C4 Verification (red→green) | PASS | The same test became 5/5 green, and independent `cargo xtask ci` plus `cargo mutants --in-diff patch.diff` completed green with 47 mutants (11 caught, 36 unviable) (`crates/chunkstore-grpc/tests/write_deadline.rs:389`). |
| C5 Causal adequacy | NEEDS-HUMAN | Decide whether indeterminate post-rename publication is admissible or publication must become deadline-cancellable/transactional — the orphan-grace proof assumes no accepted write applies after `W_write`, while this backend deliberately retains possibly late bytes (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:1561`; `crates/chunkstore-fs/src/lib.rs:396`). |
| T1 Structure | PASS | The deadline has one typed seam contract and each filesystem lifecycle reuses its injected clock, preserving the repository’s clock-ownership boundary (`crates/traits/src/lib.rs:1060`; `crates/chunkstore-fs/src/lib.rs:284`). |
| T2 Shape | PASS | The wire field is optional on a new tag and the full workspace compile confirms the breaking Rust seam was migrated consistently (`crates/proto/proto/wyrd/v0/chunk.proto:56`; `crates/traits/src/lib.rs:1126`). |
| T3 Runtime | FAIL | Arbitrarily many concurrent refusals may each remove the shared empty chunk directory, but a live write retries only twice, so deadline traffic can still make a valid live write fail with `NotFound` (`crates/chunkstore-fs/src/lib.rs:195`; `crates/chunkstore-fs/src/lib.rs:311`). |
| T4 Contribution | NEEDS-HUMAN | Triage the gate’s nine reported blockers before contribution — affected-path prior art was clear in merged history and all nine closed-unmerged PRs, but `scripts/review-branch --bundle` and its blocker report are absent here, so the red scanner row cannot be independently reproduced. |
| T5 Judgment | NEEDS-HUMAN [impl] | Replace the rollback race test with a live-on-entry, expired-at-publication interleaving — its “expired” writers use deadline `9_000` against clock `10_000` and therefore exit before rollback, so the claimed concurrency guarantee is untested (`crates/chunkstore-fs/tests/concurrent_put.rs:257`; `crates/chunkstore-fs/src/lib.rs:272`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether silent mixed-version degradation and retained indeterminate publications are fit for the multipart safety proof — approval determines whether `W_write` may soundly size `G_orphan` (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:1478`). |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C1 Spec — Choose whether Alpha may silently lose the bound against an old D server or requires capability negotiation — this determines whether a mixed-version deployment can claim `W_write` (`crates/proto/proto/wyrd/v0/chunk.proto:56`).
- [ ] C5 Causal adequacy — Decide whether indeterminate post-rename publication is admissible or publication must become deadline-cancellable/transactional — the orphan-grace proof assumes no accepted write applies after `W_write`, while this backend deliberately retains possibly late bytes (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:1561`; `crates/chunkstore-fs/src/lib.rs:396`).
- [ ] T4 Contribution — Triage the gate’s nine reported blockers before contribution — affected-path prior art was clear in merged history and all nine closed-unmerged PRs, but `scripts/review-branch --bundle` and its blocker report are absent here, so the red scanner row cannot be independently reproduced.
- [ ] T5 Judgment — Replace the rollback race test with a live-on-entry, expired-at-publication interleaving — its “expired” writers use deadline `9_000` against clock `10_000` and therefore exit before rollback, so the claimed concurrency guarantee is untested (`crates/chunkstore-fs/tests/concurrent_put.rs:257`; `crates/chunkstore-fs/src/lib.rs:272`).
- [ ] Validation — fitness-to-purpose — Decide whether silent mixed-version degradation and retained indeterminate publications are fit for the multipart safety proof — approval determines whether `W_write` may soundly size `G_orphan` (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:1478`).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 9 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- [ ] C1 Spec — Decide whether Alpha accepts silent mixed-fleet loss of the safety guarantee or requires capability negotiation—old servers ignore the additive deadline field, so enforcement depends on server version (`crates/proto/proto/wyrd/v0/chunk.proto:37`).
- [ ] C1 Spec — Maintainers must decide whether silent deadline loss against an old D server is acceptable for Alpha or requires capability negotiation — a mixed-version fleet otherwise does not receive the promised bound (`crates/chunkstore-grpc/src/client.rs:243`).
- [ ] C1 Spec — Decide whether Alpha may silently lose the guarantee against an old D server or must add capability negotiation—the mixed-version contract changes rollout safety (`crates/chunkstore-grpc/src/client.rs:245`).
- [ ] C5 Causal adequacy — Decide whether “checked immediately before rename” satisfies “lands within `W_write`,” or whether publication latency needs its own bound—the implementation admits visibility may occur after expiry, while normative `δ_clock` covers only clock resolution/skew (`crates/chunkstore-fs/src/lib.rs:300`; `docs/design/proposals/draft/0016-multipart-commit-protocol.md:1566`).
- [ ] C3 Change — Decide whether `PublishedLate` is an admissible scope change — the normative contract forbids application at or after `W_write`, while the new seam expressly preserves bytes that land after it (`crates/traits/src/lib.rs:624`).
- [ ] C5 Causal adequacy — Decide whether post-facto classification can replace prevention — a rename may still publish after the deadline and the implementation deliberately leaves that fragment durable, so the grace proof no longer bounds every landing (`crates/chunkstore-fs/src/lib.rs:378`, `crates/chunkstore-fs/src/lib.rs:392`).

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
- Iteration delta (if iterating): Rebuild for the 9 blocking T4 batch-review findings on this iteration's patch (review-batch.md), not yet addressed in review-rejected.md: - Primary: the `CREATE_RETRIES = 2` margin protecting a live write from a racing rollback's directory removal may still be insufficient under 3+ staggered concurrent expirers (crates/chunkstore-fs/src/lib.rs:311) — build-notes itself calls this "measured, not proven" and unforceable by mutation at this seam, so it needs either a stronger bound/mechanism or a convincing argument plus adversarial-scale evidence, not just a 2000-round measurement. - Coupled: the regression test meant to cover that race (crates/chunkstore-fs/tests/concurrent_put.rs:257) uses writers already expired at admission, so they're rejected by the fast admission check and never reach rollback/removal — the claimed coverage for the retry margin is illusory. Fix the test to actually drive expiry-after-admission before judging the margin resolved. - Secondary: WriteEffect::Unknown's Display/guidance text tells callers to "re-authorize" when the correct remedy is "re-read" (crates/traits/src/lib.rs:1002,1025) — wrong recovery guidance in the new typed error, should be a small fix. Do not re-treat the CREATE_RETRIES margin as settled by measurement alone this round — either close the race deterministically or land a test that genuinely forces it before claiming green.
- By / date: Eduard Ralph / 2026-07-26

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
