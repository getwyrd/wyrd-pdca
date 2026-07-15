# Brief — issue 430 / fragment-identity-validation

> The Plan artifact (docs 02 §PLAN). Do reads ONLY this file.

- **Slug:** fragment-identity-validation
- **Defect:** The shared read/repair validation accepts a decoded fragment on
  `chunk_id` alone: `repair::fragment_intact` / `repair::intact_shard`
  (`crates/core/src/repair.rs:53-70`) and the read path's inline decodes
  (`crates/core/src/read.rs:234` single-fragment path, `read.rs:319` RS fan-out) never
  check that the fragment header's `ec_fragment_index` matches the requested
  `FragmentId.index`, nor that the header's EC tuple (`ec_k`/`ec_m`/scheme type,
  `crates/chunk-format/src/header.rs:115-119`) matches the committed `ChunkRef.scheme`.
  A backend that returns a valid same-chunk shard for the WRONG index passes the check
  and the RS read pushes its payload under the requested index
  (`read.rs:306-321`) — wrong reconstruction input. The same helpers are the gate in
  reconstruction (`crates/custodian/src/reconstruction.rs:389`), scrub
  (`crates/custodian/src/scrub.rs:119`) and rebalance
  (`crates/custodian/src/rebalance.rs:264`). Only `FsChunkStore::verify`
  (`crates/chunkstore-fs/src/lib.rs:117-130`) independently checks chunk **and** index,
  which masks the gap for today's fs backend only — the assurance boundary sits in the
  backend, not the shared core code.
- **Success criterion:** A store that returns a validly-encoded fragment of the SAME
  chunk but a DIFFERENT `ec_fragment_index` (and likewise one whose header EC tuple
  disagrees with the committed scheme) is rejected by the shared core/custodian
  validation: the shard is never fed to the RS decoder under the requested index — the
  read NEVER returns wrong bytes: it reads around when ≥ k intact fragments remain, or
  fails with a typed error otherwise — and the affected chunk is enqueued on the shared
  repair queue (as the existing misplaced-fragment arm already does, `read.rs:332-343`).
  Demonstrated by the new test file below going red on base, green with the fix, under
  C4-verify.
- **Falsifiability:** RED is producible in-process on the base toolchain: on
  `origin/main`, a test-double `ChunkStore` (mirror `MemChunks`,
  `crates/core/tests/read_repair.rs:74`) that serves a same-chunk wrong-index fragment
  is accepted today — the new test's never-wrong-bytes/enqueue assertions fail. MAKE THE
  RED DETERMINISTIC: the RS fan-out stops as soon as k shards are accepted
  (`read.rs:319-325`), so with n fragments available the wrong-identity shard may never
  be examined and the pre-fix red would be order-dependent. Serve only k fragments total,
  ONE of them wrong-identity (e.g. RS(2,1): index 0 answers with index 1's bytes, index 1
  correct, index 2 absent) — the decoder then necessarily consumes the wrong shard
  pre-fix (silently wrong bytes, no enqueue → red) and rejects it post-fix (typed error
  or read-around + enqueue → green). Cover BOTH cases: a wrong `ec_fragment_index` AND a
  header EC tuple (`ec_k`/`ec_m`/scheme type) disagreeing with the committed scheme.
  Plain `#[test]` (pollster/tokio as the peer suites do), no cfg gate, so the C4-verify
  legs compile and run it; the red leg reverts the production files and keeps the added
  test file.
- **Invariant to restore:** A fragment is admitted into any read/repair/maintenance path
  only when its decoded header proves the FULL identity requested: `chunk_id`,
  `ec_fragment_index`, and (for RS) an EC tuple consistent with the committed
  `ChunkRef.scheme` — verification is "against the chunk map", not against half of it
  (proposal 0005:262-267, the scrub/verify contract; the store-level precedent is
  `FsChunkStore::verify`, `chunkstore-fs/src/lib.rs:117-130`). The never-wrong-bytes
  assurance must hold in the shared core code for ANY backend, including an adversarial
  or corrupted one — not be delegated to backend goodwill.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Conflicts with:** 431
- **Ordering note:** #430 and #431 both edit the RS fan-out match in
  `crates/core/src/read.rs` (adjacent arms of the same `while let` loop, ~:306-380).
  No build-on dependency either way — schedule them in different waves so neither is
  built blind on the other's base. Suggested order: 430 first (it reshapes the shared
  helpers; 431's single-arm change rebases trivially on top).
- **Surfaces:** data
- **Difficulty:** high
- **Scope:** one logical fix — the shared validation boundary in `crates/core`
  (`repair.rs` helpers + the two inline decode gates in `read.rs`) verifies the full
  expected fragment identity, and the custodian call-sites (`reconstruction.rs:389`,
  `scrub.rs:119`, `rebalance.rs:264`) pass the expected identity/scheme through. The
  issue's suggested shape (helpers take the expected `FragmentId` + `EcScheme`) is a
  reasonable one; Do owns the exact signatures. / out of scope: backend/`ChunkStore`
  implementations (FsChunkStore already verifies; do not touch), new metrics beyond the
  existing `FaultClass` emission (`read.rs:178-186`), any behavioural redesign of the
  maintenance loops, and #431's block-fault repair question (a different arm of the same
  match — explicitly not this bundle's).
- **Repro instruction:** On `origin/main`: build an RS(2,1) chunk with
  `wyrd_chunk_format::encode` (as `crates/core/tests/read_repair.rs:186-194` does), then
  serve fragment index 1's bytes when index 0 is requested via a test-double store.
  `repair::intact_shard(&bytes, chunk)` returns `Some(payload)` (repair.rs:66-69) and the
  RS read path pushes it at the requested index (read.rs:319-321) — the wrong-index shard
  is accepted everywhere the shared helpers gate.
- **External dependencies:** none
- **Test file:** crates/core/tests/fragment_identity.rs   (NEW file — the C4-verify gate
  earns its red only from an added `*/tests/*.rs`; do not append to an existing suite.
  SHAPE THE RED HONESTLY: the test must exercise the PUBLIC surface — `read::read_object`
  / `read_path` + `repair::queued_repairs` over the wrong-index store — so that with the
  production change reverted it fails by ASSERTION. A test that calls the widened helper
  signatures directly would fail the red leg by COMPILE ERROR (the old signatures return
  pre-revert) — a degenerate red that proves nothing about the behaviour.)
- **Citations expected:** Do must cite path:line on `main` for every change. Composition
  peers Do MAY open: `FsChunkStore::verify` (`crates/chunkstore-fs/src/lib.rs:117-130`)
  — the full-identity check to mirror at the shared layer; and the test-double store
  pattern in `crates/core/tests/read_repair.rs:74-151` (`MemChunks`,
  `IntegrityFaultingStore`) for the wrong-index store the test needs.
- **Prior-art check (triage cycles):** searched by file path — `git -C ../wyrd log` over
  `crates/core/src/repair.rs`, `crates/core/src/read.rs`: no commit addresses fragment
  index/scheme validation (most recent: 1d2a469 fault-naming, 482c3f3 invalid-scheme
  rejection at read time — a different check). The helpers still compare `chunk_id` only
  on today's `main` (verified by Read). No closed/rejected PR found for this path.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 1): Check found implementation-level items only, no architectural judgment required — C2 Reproduction (red pre-fix) — Confirm the public-surface tests fail by assertion on the production-reverted base — the artifact-only reviewer could not mutate/stash the target, and the reported `engine/scripts/run-verify.sh` wrapper is absent, so the claimed red leg was not independently reproduced (`crates/core/tests/fragment_identity.rs:200`).; C4 Verification (red→green) — Decide whether the unavailable red-leg rerun plus sandbox-blocked full CI is acceptable — both focused green tests passed, but native `cargo xtask ci` stopped at an unrelated loopback bind `PermissionDenied`, so complete red→green/CI verification remains provisional (`crates/core/tests/fragment_identity.rs:151`).; T4 Contribution — Confirm no closed/rejected remote work already resolves these affected paths — merged/all-local-ref history was checked by file path and showed no full index-plus-scheme fix, but closed/rejected PR state was not mechanically available offline (`crates/core/src/read.rs:229`).
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 2): Check found implementation-level items only, no architectural judgment required — C4 Verification (red→green) — Decide whether focused red→green plus fmt/clippy/build coverage is sufficient despite this host blocking loopback: both public tests are red by assertion before and green after, but `cargo xtask ci` cannot complete because `list_delete_over_grpc` cannot bind loopback (`crates/core/tests/fragment_identity.rs:152`).; T4 Contribution — Confirm no closed/rejected remote work already resolves these affected paths — merged/all-local-ref history was checked by affected file path and shows earlier chunk-only validation work, but closed/rejected PR state is unavailable offline (`crates/core/src/read.rs:229`).; The RS-arm `ec_k`/`ec_m` comparison is **dead-untested**: mutating
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: Not a rejection of the approach — gates were green (C4-ci pass, C4-verify red→green pass) and the main review passed C1/C3/C5/T1/T2/T3/T5. Rejected because the §6 items could not be cleared this session: the adversary advisory leaf produced no artifact (substantive gap), and an independent re-run of the verification was requested at sign-off but could not be performed (sign-off host shell failure). Next pass: re-run the adversary leaf so findings exist, keep the deterministic C4-verify/xtask-ci evidence visible to the reviewer, and surface T4 (no closed/rejected upstream PR covers these paths) and validation fitness-to-purpose for the human with whatever remote-PR evidence can be gathered. Do not rework the identity-validation predicate itself (chunk + index + scheme type + stripe geometry at the shared admission boundary) — it was not faulted.
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
