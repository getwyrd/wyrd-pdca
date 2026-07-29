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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 45 mutants tested in 37s: 10 caught, 35 unviable

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

Task under review: enforce an optional fragment-write authorization deadline at every `ChunkStore`/D-server publication point while preserving no-deadline compatibility and a typed refusal outcome.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The normative contract is explicit: an accepted write must not take effect after `W_write`, and the strict orphan-grace margin depends on that bound (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:1551`). |
| C2 Reproduction (red pre-fix) | PASS | In a scratch clone of the target base, the new wire test compiled and ran five tests: the three deadline/refusal assertions failed while the live and absent-deadline controls passed (`crates/chunkstore-grpc/tests/write_deadline.rs:388`). |
| C3 Change | FAIL | The required no-late-effect guarantee is not restored: `rename` may finish after the deadline and the implementation intentionally leaves the published bytes in place, so an accepted write can still take effect arbitrarily late (`crates/chunkstore-fs/src/lib.rs:389`, `crates/chunkstore-fs/src/lib.rs:410`). |
| C4 Verification (red→green) | PASS | The same five-test red leg became 5/5 green with the patch, 29 targeted filesystem/concurrency/gRPC tests passed, and a scratch rerun of `cargo xtask ci` completed all checks including the 50-seed DST suite (`crates/chunkstore-grpc/tests/write_deadline.rs:388`). |
| C5 Causal adequacy | NEEDS-HUMAN | Decide whether a publication-unverified `Unknown` result may replace the specified no-late-effect invariant, or whether the publication primitive must prevent/cancel late effects — bytes landing after grace would invalidate the orphan-evidence proof (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:1557`, `crates/chunkstore-fs/src/lib.rs:394`). |
| T1 Structure | PASS | The deadline follows the narrow seam from additive protobuf field through the service into the store, and the store owns one injected clock for the lifecycle (`crates/proto/proto/wyrd/v0/chunk.proto:62`, `crates/chunkstore-grpc/src/server.rs:82`, `crates/chunkstore-fs/src/lib.rs:286`). |
| T2 Shape | NEEDS-HUMAN | Decide whether the added `ABORTED`/indeterminate second outcome belongs in this refusal-only slice — it expands the public wire and caller-recovery contract beyond the specified typed refusal and affects interoperability (`crates/proto/proto/wyrd/v0/chunk.proto:39`, `crates/chunkstore-grpc/src/client.rs:147`). |
| T3 Runtime | PASS | The real in-process tonic server refused expired and parked writes without storing them, while live and deadline-absent writes round-tripped byte-identically (`crates/chunkstore-grpc/tests/write_deadline.rs:388`, `crates/chunkstore-grpc/tests/write_deadline.rs:452`). |
| T4 Contribution | NEEDS-HUMAN | Decide the disposition of the five gate-reported blocking review findings — the `review-branch --bundle` runner and its finding artifact are absent here, so that red row is provisional; the independent affected-path audit found no semantic duplicate in merged history or all nine closed-unmerged PRs. |
| T5 Judgment | NEEDS-HUMAN | Decide whether Alpha may silently lose the guarantee when a new client reaches an old D server — accepting degradation weakens the invariant during mixed rollout, while rejecting it requires the out-of-scope capability exchange (`crates/chunkstore-grpc/src/client.rs:259`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether this mechanism is fit for the orphan-GC safety argument — queue refusal is exercised, but the implementation’s admitted possibly-late durable publication is the exact outcome the strict `G_orphan > W_write + δ_clock` proof must exclude (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:1561`, `crates/dst/tests/network.rs:942`). |


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C5 Causal adequacy — Decide whether a publication-unverified `Unknown` result may replace the specified no-late-effect invariant, or whether the publication primitive must prevent/cancel late effects — bytes landing after grace would invalidate the orphan-evidence proof (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:1557`, `crates/chunkstore-fs/src/lib.rs:394`). — Human: accepts the standing `review-rejected.md` rationale (rename is not atomic/cancelable; `Unknown` + orphan-evidence coverage is the correct trade; retraction was already tried and rejected as worse).
- [x] T2 Shape — Decide whether the added `ABORTED`/indeterminate second outcome belongs in this refusal-only slice — it expands the public wire and caller-recovery contract beyond the specified typed refusal and affects interoperability (`crates/proto/proto/wyrd/v0/chunk.proto:39`, `crates/chunkstore-grpc/src/client.rs:147`). — Human: ticked; accepted as part of the same accepted `Unknown`/indeterminate design.
- [x] T4 Contribution — Decide the disposition of the five gate-reported blocking review findings — the `review-branch --bundle` runner and its finding artifact are absent here, so that red row is provisional; the independent affected-path audit found no semantic duplicate in merged history or all nine closed-unmerged PRs. — Human: dispositioned all five (mixed-fleet finding out of scope; rename-atomicity finding + its DST-test-gap companion accepted per the standing rejection); will manually verify.
- [x] T5 Judgment — Decide whether Alpha may silently lose the guarantee when a new client reaches an old D server — accepting degradation weakens the invariant during mixed rollout, while rejecting it requires the out-of-scope capability exchange (`crates/chunkstore-grpc/src/client.rs:259`). — Human: older D-servers do not need to be considered; degrading silently against an old server is accepted for this slice.
- [x] Validation — fitness-to-purpose — Decide whether this mechanism is fit for the orphan-GC safety argument — queue refusal is exercised, but the implementation’s admitted possibly-late durable publication is the exact outcome the strict `G_orphan > W_write + δ_clock` proof must exclude (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:1561`, `crates/dst/tests/network.rs:942`). — Human: accepted; late-landing bytes are covered by orphan evidence per the standing rejection argument.
- [x] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_ — Human: will manually test/verify rather than requiring an automated re-run before accept.
- [x] C1 Spec — Decide whether Alpha accepts silent mixed-fleet loss of the safety guarantee or requires capability negotiation—old servers ignore the additive deadline field, so enforcement depends on server version (`crates/proto/proto/wyrd/v0/chunk.proto:37`). — Human: older D-servers do not need to be considered; not required for this slice.
- [x] C1 Spec — Maintainers must decide whether silent deadline loss against an old D server is acceptable for Alpha or requires capability negotiation — a mixed-version fleet otherwise does not receive the promised bound (`crates/chunkstore-grpc/src/client.rs:243`). — Human: same decision as above, duplicate item.
- [x] C1 Spec — Decide whether Alpha may silently lose the guarantee against an old D server or must add capability negotiation—the mixed-version contract changes rollout safety (`crates/chunkstore-grpc/src/client.rs:245`). — Human: same decision as above, duplicate item.
- [x] C5 Causal adequacy — Decide whether “checked immediately before rename” satisfies “lands within `W_write`,” or whether publication latency needs its own bound—the implementation admits visibility may occur after expiry, while normative `δ_clock` covers only clock resolution/skew (`crates/chunkstore-fs/src/lib.rs:300`; `docs/design/proposals/draft/0016-multipart-commit-protocol.md:1566`). — Human: accepted per the standing rejection rationale.
- [x] C3 Change — Decide whether `PublishedLate` is an admissible scope change — the normative contract forbids application at or after `W_write`, while the new seam expressly preserves bytes that land after it (`crates/traits/src/lib.rs:624`). — Human: accepted per the standing rejection rationale.
- [x] C5 Causal adequacy — Decide whether post-facto classification can replace prevention — a rename may still publish after the deadline and the implementation deliberately leaves that fragment durable, so the grace proof no longer bounds every landing (`crates/chunkstore-fs/src/lib.rs:378`, `crates/chunkstore-fs/src/lib.rs:392`). — Human: accepted per the standing rejection rationale.

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: merged-wider
- Iteration delta (if iterating):
- By / date: Eduard Ralph / 2026-07-26

## 10. Act candidates (hints for the next Act review)
- New issue (assigned to milestone M6): a D-server-local, **scheduled** (not opportunistic) periodic reaper inside `chunkstore-fs` to reclaim empty chunk directories left behind by refused fragment writes (`crates/chunkstore-fs/src/lib.rs:210`, `:383`), which today are only reclaimed at the next `open`, letting a long-lived server accumulate unbounded empty directories under sustained late/expired writes. Distinct from #625's custodian-level multipart-session reaper (metadata-store records, cluster-wide via `reconcile_step`) — this is local filesystem inode housekeeping on a single D-server and needs its own mechanism. Keep it off the write hot path: use the same atomic `remove_dir`-on-empty safety the existing `open`-time reap already relies on, run on a timer, not per-write/per-refusal.
