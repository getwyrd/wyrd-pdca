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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 46 mutants tested in 39s: 3 missed, 9 caught, 34 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 8 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- T4 contribution artifacts complete (user-impact opener + tracker id in both): pass — ${PDCA_CLI:-.venv/bin/wyrd-pdca} contribcheck
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of issue #638: enforce fragment-write authorization deadlines at the D server so expired or parked writes cannot take effect late.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The governing decision is explicit: an authorized write must land within `W_write` or be abandoned, and the D server must refuse a past-deadline write so the orphan-grace inequality remains sound (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:1551`). |
| C2 Reproduction (red pre-fix) | PASS | On a clean base all five wire tests ran; three failed by behavioral assertion because expired and parked writes were accepted, while the live and absent-deadline controls passed (`crates/chunkstore-grpc/tests/write_deadline.rs:389`). |
| C3 Change | NEEDS-HUMAN | Decide whether `PublishedLate` is an admissible scope change — the normative contract forbids application at or after `W_write`, while the new seam expressly preserves bytes that land after it (`crates/traits/src/lib.rs:624`). |
| C4 Verification (red→green) | PASS | The same five wire tests pass with the patch, and an independent full `cargo xtask ci` completed through typos, docs render, fmt, clippy, build, tests, dependency audits, conformance, guards, and seeded DST (`crates/chunkstore-grpc/tests/write_deadline.rs:421`). |
| C5 Causal adequacy | NEEDS-HUMAN | Decide whether post-facto classification can replace prevention — a rename may still publish after the deadline and the implementation deliberately leaves that fragment durable, so the grace proof no longer bounds every landing (`crates/chunkstore-fs/src/lib.rs:378`, `crates/chunkstore-fs/src/lib.rs:392`). |
| T1 Structure | PASS | The required ownership split is preserved: additive RPC field, trait-level contract, concrete backend enforcement, one new gRPC test file, and DST appended to the existing network target (`crates/proto/proto/wyrd/v0/chunk.proto:56`, `crates/dst/tests/network.rs:634`). |
| T2 Shape | PASS | Optional protobuf tag 3 preserves absent-field compatibility and the optional deadline flows through the typed `ChunkStore` seam without changing unrelated operations (`crates/proto/proto/wyrd/v0/chunk.proto:56`, `crates/traits/src/lib.rs:853`). |
| T3 Runtime | PASS | Real tonic loopback tests with the production filesystem store and seeded madsim cases independently pass the expired, parked, live, absent, store-internal-expiry, and typed-error paths (`crates/chunkstore-grpc/tests/write_deadline.rs:389`, `crates/dst/tests/network.rs:800`). |
| T4 Contribution | NEEDS-HUMAN | Decide whether the eight gate-reported batch-review blockers are valid or settled — `scripts/review-branch` is absent and contribcheck lacks its `pdca.toml` context, so those results are provisional; the independent affected-path audit found no semantic duplicate in merged history or nine closed-unmerged PRs. |
| T5 Judgment | NEEDS-HUMAN [impl] | The evidence must distinguish three real surviving mutants — rollback state checks at `crates/chunkstore-fs/src/lib.rs:196` and `crates/chunkstore-fs/src/lib.rs:198`, plus ordinary gRPC reconstruction of `ABORTED` at `crates/chunkstore-grpc/src/client.rs:172` — or the claimed cleanup and wire-effect coverage remains incomplete. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether silent deadline loss during a mixed-version rollout is acceptable — an old D server ignores the additive field, so the fleet does not provide the promised bound until every acceptor is upgraded (`docs/design/architecture/08-crosscutting-concepts.md:106`). |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C3 Change — Decide whether `PublishedLate` is an admissible scope change — the normative contract forbids application at or after `W_write`, while the new seam expressly preserves bytes that land after it (`crates/traits/src/lib.rs:624`).
- [ ] C5 Causal adequacy — Decide whether post-facto classification can replace prevention — a rename may still publish after the deadline and the implementation deliberately leaves that fragment durable, so the grace proof no longer bounds every landing (`crates/chunkstore-fs/src/lib.rs:378`, `crates/chunkstore-fs/src/lib.rs:392`).
- [ ] T4 Contribution — Decide whether the eight gate-reported batch-review blockers are valid or settled — `scripts/review-branch` is absent and contribcheck lacks its `pdca.toml` context, so those results are provisional; the independent affected-path audit found no semantic duplicate in merged history or nine closed-unmerged PRs.
- [ ] T5 Judgment — The evidence must distinguish three real surviving mutants — rollback state checks at `crates/chunkstore-fs/src/lib.rs:196` and `crates/chunkstore-fs/src/lib.rs:198`, plus ordinary gRPC reconstruction of `ABORTED` at `crates/chunkstore-grpc/src/client.rs:172` — or the claimed cleanup and wire-effect coverage remains incomplete.
- [ ] Validation — fitness-to-purpose — Decide whether silent deadline loss during a mixed-version rollout is acceptable — an old D server ignores the additive field, so the fleet does not provide the promised bound until every acceptor is upgraded (`docs/design/architecture/08-crosscutting-concepts.md:106`).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 8 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- [ ] C1 Spec — Decide whether Alpha accepts silent mixed-fleet loss of the safety guarantee or requires capability negotiation—old servers ignore the additive deadline field, so enforcement depends on server version (`crates/proto/proto/wyrd/v0/chunk.proto:37`).
- [ ] C1 Spec — Maintainers must decide whether silent deadline loss against an old D server is acceptable for Alpha or requires capability negotiation — a mixed-version fleet otherwise does not receive the promised bound (`crates/chunkstore-grpc/src/client.rs:243`).
- [ ] C1 Spec — Decide whether Alpha may silently lose the guarantee against an old D server or must add capability negotiation—the mixed-version contract changes rollout safety (`crates/chunkstore-grpc/src/client.rs:245`).
- [ ] C5 Causal adequacy — Decide whether “checked immediately before rename” satisfies “lands within `W_write`,” or whether publication latency needs its own bound—the implementation admits visibility may occur after expiry, while normative `δ_clock` covers only clock resolution/skew (`crates/chunkstore-fs/src/lib.rs:300`; `docs/design/proposals/draft/0016-multipart-commit-protocol.md:1566`).

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
- Iteration delta (if iterating): Auto-iterate (round 5): rebuilding for the implementation-level findings — T4 Contribution — Decide whether the eight gate-reported batch-review blockers are valid or settled — `scripts/review-branch` is absent and contribcheck lacks its `pdca.toml` context, so those results are provisional; the independent affected-path audit found no semantic duplicate in merged history or nine closed-unmerged PRs.; T5 Judgment — The evidence must distinguish three real surviving mutants — rollback state checks at `crates/chunkstore-fs/src/lib.rs:196` and `crates/chunkstore-fs/src/lib.rs:198`, plus ordinary gRPC reconstruction of `ABORTED` at `crates/chunkstore-grpc/src/client.rs:172` — or the claimed cleanup and wire-effect coverage remains incomplete.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 8 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_. 6 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-07-26

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
