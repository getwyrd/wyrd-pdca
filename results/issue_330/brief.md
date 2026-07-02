# Brief — issue 330 / scrub-detect-missing-placed-fragment

> The Plan artifact (docs 02 §PLAN). Human-authored. Do reads ONLY this file.
> The field labels below are parsed by the driver — keep the `- **Label:** value`
> shape. The success criterion is the load-bearing field.

- **Slug:** scrub-detect-missing-placed-fragment
- **Defect:** On `origin/main` **no production path enqueues a repair obligation for a
  committed-referenced fragment that is simply absent** (present-but-corrupt fragments
  and explicit losses are covered; plain absence is not). The scrub loop only ever walks
  the fragments a D server actually returns from `list_fragments()` and, for a referenced
  one, verifies its checksum — so a fragment that is *missing* from the store is never
  observed: the `Ok(None)` fetch arm simply `continue`s
  (`crates/custodian/src/scrub.rs`, the "vanished between the walk and the fetch … skip
  it" arm ~`:91`), and enqueue happens only on a checksum/integrity fault (`scrub.rs`
  ~`:85`, ~`:105`). The read path likewise appends only present-but-corrupt fragments to
  the repair set (`crates/core/src/read.rs` `corrupt.push`, ~`:151`/`:158`), never
  missing ones. Because of this gap the #250 (Tier-1) and #196 (Tier-2) reconstruction
  scenarios must inject the obligation with a sanctioned test stand-in
  (`repair::enqueue_repair(&meta, CHUNK, …)`, `crates/chunkstore-grpc/tests/tier2_kill_reconstruct.rs:545`
  and `tier1_jepsen_consistency.rs`) — the reconstruction path is genuinely exercised;
  only the **detection trigger** is stubbed.
- **Success criterion:** A scrub reconciliation pass (the production `reconcile_step` →
  scrub path) over a committed chunk one of whose **placed** fragments is absent from its
  holding D server (`get_fragment`/`list_fragments` do not return it) results in that
  chunk being **enqueued on the shared repair queue** — the same durable obligation a
  corrupt fragment produces today. Demonstrable at C4-verify by a scrub test (below) that
  is red pre-fix (no enqueue for the missing fragment) and green post-fix. The scrub loop
  as the site is ILLUSTRATIVE; the BINDING condition is that a *production* detector turns
  a simply-missing committed-referenced fragment into a repair obligation, **without**
  false positives for unreferenced/orphan fragments or for fragments legitimately not-yet-
  present during an in-flight write / pending-GC window.
- **Invariant to restore:** For a committed chunk map, **every referenced fragment is
  either present-and-intact or becomes a durable repair obligation** — a placed fragment's
  *absence* is a loss in the same category as its *corruption*, and neither may be silently
  absorbed. (Proposal 0005 §reconstruction / §scrub: "a checksum-failing fragment is never
  absorbed silently — it always becomes a durable repair obligation", `0005:262-267`, and
  the shared repair queue the read path also feeds, `0005:174-176`; ADR-0040 fixes the
  committed-placement resolution these loops walk.) SELF-TEST: this is not satisfiable by a
  single-module guard — it is a property over the whole detection surface for referenced
  fragments; the fix restores it for the missing-fragment case.
- **Repo + branch target:** getwyrd/wyrd @ main   (Wyrd has no maintenance branches; INTEGRATION §2)
- **Conflicts with:** 348
- **Ordering note:** #330 and #348 both edit the durability-maintenance seam — the shared
  reference set `gc.rs:referenced_fragments` and the scrub loop `scrub.rs` — but neither
  builds on the other's result. Schedule them in DIFFERENT waves so a Do never builds blind
  on the other's uncommitted edit to the same files. No build-on dependency in either
  direction, hence `Conflicts with`, not `Depends on`.
- **Surfaces:** data
- **Difficulty:** medium
- **Scope:** Close the missing-fragment detection gap: a committed-referenced fragment that
  is simply absent from its placed D server must yield a durable repair obligation on the
  shared repair queue, from a production maintenance path (not a test stand-in), with the
  false-positive guardrails above (in-flight writes, pending/expired-lease GC, and
  unreferenced/orphan fragments must NOT be flagged). / out of scope: the
  killed/partitioned D-server case whose fragments are unobservable because the server
  cannot be `list_fragments()`'d at all (that needs desired-state/topology awareness — a
  separate detector); the reconstruction/dequeue/re-placement path itself (already
  exercised); removing the #250/#196 `enqueue_repair` test stand-ins (may be dropped or
  kept belt-and-suspenders once real detection lands — a follow-up, not this fix).
- **Repro instruction:** On `getwyrd/wyrd@main`, in the `crates/custodian/tests/scrub.rs`
  harness (`MemMeta` + `MemStore` + `commit_chunk`/`commit_reference` + `elect`/
  `reconcile_step`): commit a chunk map referencing a fragment, but have the holding D
  server's `list_fragments()`/`get_fragment` NOT return that fragment (simply absent, not
  corrupt). Run the scrub reconcile via `reconcile_step`. Pre-fix: the chunk is never
  enqueued on the shared repair queue. Post-fix: it is.
- **Test file:** crates/custodian/tests/scrub.rs
- **Citations expected:** Do must cite path:line on `getwyrd/wyrd@main` for every change
  (scrub detection site; any `referenced_fragments`/present-set comparison).
- **Prior-art check (triage cycles):** Searched merged history and open PRs by file path
  (`scrub.rs`, `gc.rs:referenced_fragments`, `read.rs`). `#347`/PR #361 (the
  `ChunkRef::fragments()` expansion helper) and `#287` (GC reference-set fix) are merged;
  neither adds missing-fragment detection. No open PR targets this gap. No prior
  closed/rejected attempt found. Net-new detection.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: issue_330: Patch content accepted as-is — no change of approach needed. The only blocker is that the C4 gates never actually ran: cargo/rustc were absent in the Do sandbox, so C4-ci ("cargo: not found") and C4-verify ("test RED with fix") are toolchain-absence artifacts, not observed defects. Re-run Do in an environment with the Rust toolchain so `cargo xtask ci` and `run-verify.sh` execute and confirm both new scrub tests are red pre-fix / green post-fix. Also make the `codex` advisory leaf available (or accept its absence) so the second reviewer produces findings.
- Failing gate: C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) — ./engine/xtask.sh: line 30: exec: cargo: not found
- Failing gate: C4 per-fix red->green: this patch's test red pre-fix, green post-fix (advisory) — run-verify.sh: FAIL — the bundle's test is RED *with* the fix applied (not green).
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
