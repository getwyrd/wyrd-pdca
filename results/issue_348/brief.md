# Brief — issue 348 / maintenance-loops-reject-malformed-placement

> The Plan artifact (docs 02 §PLAN). Human-authored. Do reads ONLY this file.
> The field labels below are parsed by the driver — keep the `- **Label:** value`
> shape. The success criterion is the load-bearing field.

- **Slug:** maintenance-loops-reject-malformed-placement
- **Defect:** The durability-plane maintenance loops walk a committed chunk's placement
  through the deliberately **liberal** expansion helper (`ChunkRef::fragments()` /
  `placed_dserver`, `crates/core/src/metadata.rs` ~`:120`/`:142`), which applies the
  identity fallback unconditionally and never validates the vector's length. So a
  **malformed** committed `placement` — non-empty but `len != fragment_count()`, which
  today can only mean truncation/corruption (no writer emits a short non-empty vector) —
  is silently identity-filled for its missing tail rather than rejected. Consequences:
  GC/scrub build their reference set over fabricated placement
  (`crates/custodian/src/gc.rs:referenced_fragments` ~`:194`, consumed by
  `scrub.rs:reconcile`); reconstruction and rebalance act on fabricated placement
  (`crates/custodian/src/reconstruction.rs:assess` ~`:231`,
  `crates/custodian/src/rebalance.rs:plan_evacuations` ~`:166`). A corrupt placement is
  thus masked as a silent identity resolution instead of surfacing as an operator signal.
- **Success criterion:** For a committed chunk whose `placement` is non-empty and of the
  wrong length (e.g. `EcScheme::ReedSolomon` with `fragment_count() == 6` and a length-2
  vector), each maintenance loop rejects it **before** expansion instead of fabricating
  identity entries: GC/scrub treat the chunk as **fully referenced** (never reclaim any of
  its fragments) and emit an audit signal on the durability-plane seam (ADR-0011);
  reconstruction and rebalance **skip** the chunk and flag it NEEDS-HUMAN — while the
  **read path is unchanged** and still resolves the same chunk via the per-index identity
  fallback. Demonstrable at C4-verify by the per-loop tests below (red pre-fix — the loops
  silently fabricate identity, so GC's reference set omits fragments / reconstruction acts
  on a fabricated vector; green post-fix). A single-source classifier
  (`placement_state`/`checked_fragments`/`placement_is_valid`) is the ILLUSTRATIVE shape
  ADR-0040 decision 2 suggests; the BINDING conditions are the per-loop behaviours above.
- **Invariant to restore:** **Liberal read, strict maintenance** (ADR-0040 decisions 3–4,
  Accepted): a committed `placement` vector is valid **iff** it is empty (pre-M3 →
  identity fallback) or `len == fragment_count()`; any other non-empty length is
  **malformed**. A maintenance loop MUST classify the committed placement **before**
  expanding it and MUST NEVER fabricate an identity entry for a malformed vector — GC/scrub
  fail safe (fully referenced, never reclaim, audit), reconstruction/rebalance skip + flag
  NEEDS-HUMAN. The read path stays liberal (availability first). (Source: `docs/design/adr/
  0040-mixed-era-placement-expansion.md` decisions 3–4; proposal 0005; audit-event
  obligation per ADR-0011.) SELF-TEST: not satisfiable by guarding one module — it is a
  property over four maintenance loops plus their shared classifier in `core`.
- **Repo + branch target:** getwyrd/wyrd @ main   (Wyrd has no maintenance branches; INTEGRATION §2)
- **Conflicts with:** 330
- **Ordering note:** #348 and #330 both edit the durability-maintenance seam — the shared
  reference set `gc.rs:referenced_fragments` and the scrub loop `scrub.rs` — with no
  build-on relationship, so schedule them in DIFFERENT waves (never built blind on the
  same base). The prerequisite named in the issue ("best landed after the
  `ChunkRef::fragments()` helper") is `#347` / PR #361, **already merged to `main`** (commit
  `5803c48`), so there is no in-batch `Depends on`; the classifier extends that helper's
  file.
- **Surfaces:** data
- **Difficulty:** high
- **Scope:** Stop the maintenance loops silently fabricating identity placement for a
  malformed (non-empty, wrong-length) committed placement: classify the committed vector
  once, in one place, reused by every loop; GC/scrub fail safe (fully referenced + audit
  signal on the durability seam); reconstruction/rebalance skip + flag NEEDS-HUMAN. /
  out of scope: the **read path** — it stays liberal and unchanged (must still resolve a
  malformed-placement chunk via per-index fallback); the write/commit path; removing the
  empty-vector migration fallback (that is #350, behind its own removal gate, ADR-0040
  decision 6); repoint-writes-full-length correctness (ADR-0040 decision 5 / #346).
- **Repro instruction:** On `getwyrd/wyrd@main`, in the custodian integration-test harnesses
  (`crates/custodian/tests/gc.rs`, `scrub.rs`, `reconstruction.rs`, `rebalance.rs`) and a
  `ChunkRef` unit test in `crates/core/src/metadata.rs`: commit a chunk with
  `EcScheme::ReedSolomon { k, m }` (so `fragment_count() == k+m`) and a non-empty
  `placement` of a different length. Drive each loop. Pre-fix: GC/scrub build the reference
  set over the identity-filled tail (fragments can be reclaimed); reconstruction/rebalance
  act on the fabricated vector. Post-fix: GC/scrub treat the chunk as fully referenced +
  audit; reconstruction/rebalance skip + NEEDS-HUMAN; read still resolves it.
- **Test file:** crates/custodian/tests/gc.rs   (GC/scrub fail-safe leg; companion legs in
  crates/custodian/tests/reconstruction.rs and crates/custodian/tests/rebalance.rs, plus a
  classifier unit test in crates/core/src/metadata.rs, and a read-path-unchanged assertion)
- **Citations expected:** Do must cite path:line on `getwyrd/wyrd@main` for every change
  (the classifier in `metadata.rs`; each of `gc.rs`, `scrub.rs`, `reconstruction.rs`,
  `rebalance.rs`).
- **Prior-art check (triage cycles):** Searched merged history and open PRs by file path
  (`metadata.rs`, `gc.rs`, `scrub.rs`, `reconstruction.rs`, `rebalance.rs`) and by symbol
  (`placement_state`, `placement_is_valid`, `checked_fragments`, `Malformed`). `#347`/PR
  #361 landed the liberal `ChunkRef::fragments()` helper and deliberately left the fallible
  companion classifier to this issue (`metadata.rs` doc-comment names
  `checked_fragments()` / `placement_is_valid()`, "#348"). ADR-0040 (Accepted) is the
  design foundation. No classifier / strict-maintenance code exists yet; no open PR. Net-new.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: T5(a) rejected as unbriefed and too coarse: desired_state.rs holds EVERY drain/decommission Pending cluster-wide whenever ANY malformed chunk exists, with no attribution (patch.diff:147-151). Rework is ATTRIBUTION-ONLY: KEEP the cluster-wide fail-safe block — do NOT scope it to servers named by the corrupt record (trusting a malformed vector's contents is the decision class #348 forbids) — but surface WHY in the answer itself: the malformed chunk ids alongside Pending (richer status surface and/or an audit event on the loops' existing wyrd.custodian.*.audit seam), pinned by a test. T5(b) RATIFIED at sign-off: the #349 short-placement supersession (short non-empty = malformed; scrub fails safe, reconstruction skips + NEEDS-HUMAN, record never rewritten; read path stays liberal) is the intended reading of ADR-0040 decisions 3-4. Preserve it verbatim in the rebuild — do not re-litigate. Everything else is verified and to be KEPT — this is a policy-scoped delta to desired_state.rs (+ the tests pinning its semantics, e.g. rebalance.rs drain assertion), not a rebuild. Sign-off re-ran the failed gates on origin/main + patch: green-with-fix across wyrd-core/wyrd-custodian (incl. the pre-existing gc short-placement test the reviewer feared), all four #348 loop tests red with production reverted, fmt/clippy/build/doc-tests/conformance clean. The bundle's C4-ci/C4-verify gate FAILs were host toolchain artifacts, not patch verdicts: cargo absent from the gate shell's PATH; the ~/.local/bin cc/gcc zig-cc shims reject --target=x86_64-unknown-linux-gnu (a /tmp wrapper translating it to `-target x86_64-linux-gnu` fixes it); cargo-deny and cargo-machete not installed (deny is dependency-metadata only — the patch touches no Cargo.toml/Cargo.lock). Expect the same gate artifacts on the re-check unless the gate host toolchain is fixed.
- Failing gate: C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) — ./engine/xtask.sh: line 30: exec: cargo: not found
- Failing gate: C4 per-fix red->green: this patch's test red pre-fix, green post-fix (advisory) — run-verify.sh: FAIL — the bundle's test is RED *with* the fix applied (not green).
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
