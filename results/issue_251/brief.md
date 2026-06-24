# Brief — issue 251 / reconstruction-read-around-fragment-read-fault

> The Plan artifact (docs 02 §PLAN). Human-authored. Do reads ONLY this file.
> The field labels below are parsed by the driver — keep the `- **Label:** value`
> shape. The success criterion is the load-bearing field: it is the sentence
> Check tests "did this work" against.

- **Slug:** reconstruction-read-around-fragment-read-fault
- **Defect:** `reconstruction::assess` reads each placed fragment with
  `store.get_fragment(frag).await?` (`crates/custodian/src/reconstruction.rs:246`). The
  `?` propagates any non-`NotFound` error, so a single block-layer read fault (a
  `dm-error` / dead-sector `EIO`) on **one** placed fragment makes `assess` return `Err`
  and aborts the whole per-chunk reconciliation — one faulted D server stalls repair for
  *every* chunk on the shared queue, and a disk that goes bad *after* its data lands can
  never be repaired. The read path already tolerates exactly this (`crates/core/src/read.rs:189`
  admits only `if let Ok(Some(_))`, reading an unreadable fragment around and rebuilding
  from the `k` survivors); reconstruction does not.
- **Success criterion:** With the patch applied in isolation at Check, the
  `crates/custodian/tests/reconstruction.rs` regression is green: for a `ReedSolomon{k,m}`
  chunk with ≥`k` readable survivors plus one placed fragment whose store returns a
  **permanent** read fault (an `EIO`-class `Err`, NOT `NotFound`), `assess` returns
  `Assessment::Repairable` — reading around the faulted fragment and rebuilding from the
  survivors — instead of returning `Err`/aborting; AND a placed fragment whose store
  returns a **transient** (healthy-server) error is NOT converted into permanent fragment
  loss / a re-placement (the fragment is not dropped or moved off its server). BINDING:
  the read-around-on-permanent-fault behaviour and the no-spurious-re-placement-on-transient
  behaviour. ILLUSTRATIVE: the exact `Assessment` variant names and whatever classifier
  Do uses to draw the permanent-vs-transient line.
- **Invariant to restore:** A consumer that walks **placed** fragments and acts on a
  fetch fault must preserve the seam's **permanent-loss-vs-transient** distinction: a
  permanent durability fault on a placed fragment (the device cannot return the bytes — a
  corruption/integrity fault, or a block-layer read fault such as `EIO` / dead sector) is
  a loss the rebuild **reads around** and reconstructs from the ≥`k` survivors, while a
  **transient** fault (unreachable / timed-out / busy on a healthy server) is **propagated**
  to the retry policy and never silently converted into permanent fragment loss / a
  re-placement. Source: the `IntegrityFault` seam contract in
  `crates/traits/src/lib.rs:64` — "a **corruption** fault, categorically distinct from a
  **transient** one (unreachable / timed out / busy) … the two faults are handled
  differently (repair vs. retry), so they must stay distinguishable along the whole path
  from the store to the consumer's decision point" (ADR-0010); internal project invariant
  (Tier C). Precedent: `scrub` already honours this at `crates/custodian/src/scrub.rs:102`
  (`is_integrity_fault` → repair-and-continue; other `Err` → propagate), and the read path
  reads around at `crates/core/src/read.rs:189`. SELF-TEST: an over-broad
  `.ok().flatten()` in `assess` alone (guarding the single module) **fails** this invariant —
  it misclassifies a transient fault as permanent loss and triggers a spurious permanent
  re-placement — so the narrow one-module fix visibly fails the stated property.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:**
- **Depends on (merged):**
- **Conflicts with:**
- **Ordering note:** Split out of #195 (Tier-1 disk-fault harness): the harness stays in
  #195; this is the production reconstruction behaviour change it flushed out. No
  build-on dependency is declared because #251 ships its own focused in-process
  fault-injection regression and is verifiable independently at Check. Natural merge
  order is #251 (the fix) **before** #195's harness, since the #195 harness asserts the
  *fixed* behaviour — #251 does not wait on #195. If #195's in-process `EIO` stand-in is
  authored in `crates/custodian/tests/reconstruction.rs`, the two share that test file;
  whichever lands second rebases onto the merged result. (Only #251 is in this wave, so
  no co-scheduling collision arises here.)
- **Surfaces:** data
- **Scope:** Remove the abort-on-read-fault defect in `reconstruction::assess`: a fetch
  fault on a placed fragment must not propagate out of the per-chunk assessment and abort
  the shared repair queue. A permanent durability fault on a placed fragment becomes a
  read-around loss (the chunk is rebuilt from its ≥`k` survivors); a transient /
  healthy-server fault must not be turned into permanent fragment loss or a re-placement.
  The mechanism that draws the permanent-vs-transient line is Do's to choose (prefer
  removing the cause over an over-broad swallow). / out of scope: changing `scrub` (it
  already classifies at `scrub.rs:102`) or the read path (`read.rs`); redefining the
  `IntegrityFault` type's meaning or touching the on-disk format; broadening to
  non-placed-fragment fetches; the #195 Tier-1 disk-fault harness and its privileged
  `dm-error` scenario (those ship under #195); reintroducing the rejected
  `.ok().flatten()` candidate.
- **Repro instruction:** On getwyrd/wyrd @ main, in
  `crates/custodian/tests/reconstruction.rs`, build a `ReedSolomon{k,m}` chunk with `k`
  readable survivors placed, plus one additional placed fragment whose backing store
  returns a non-`NotFound` `Err` (`EIO`-class) from `get_fragment`; call `assess`.
  Pre-fix: `assess` returns `Err` (the `?` at `reconstruction.rs:246` propagates), so the
  chunk is never reconstructed and the queue aborts. Post-fix: `assess` returns
  `Assessment::Repairable`, reading around the faulted fragment.
- **Test file:** crates/custodian/tests/reconstruction.rs
- **Verification posture:** DEFAULT flippable for the permanent-fault read-around (red
  pre-fix: `assess` returns `Err`; green post-fix: `Assessment::Repairable`). PLUS a
  discriminating guard for the transient case (asserts a transient/healthy-server error
  is not converted into a permanent re-placement) — green with the correct fix, red with
  the over-broad `.ok().flatten()`; its value is catching the over-broad regression rather
  than a red→green flip, so it ships alongside the flippable assertion. DEFERRED /
  off-Check: the privileged `dm-error` / dead-sector scenario that drives a *real*
  block-layer `EIO` is exercised by the **#195 Tier-1 disk-fault harness** (privileged,
  needs device-mapper) — supplementary evidence confirmed under #195, NOT the Check gate
  here. The seam exercised at Check (the fault-injecting mock `ChunkStore`) is itself
  built and unit-exercised in this slice; nothing is left as inert scaffolding.
- **Production reach:** the in-process `EIO` stand-in (a mock `ChunkStore` returning an
  `EIO`-class `Err`) load-bearingly exercises the **production** read-around path: the live
  `assess` runs the same classification at Check regardless of whether the fault
  originates from the mock or a real dead sector. A real-device `EIO` is exercised only by
  the deferred privileged scenario (#195). The production path traverses the fix at Check;
  this note records that the Check-time fault is injected in-process, not raised by real
  hardware.
- **Citations expected:** Do must cite path:line on the target branch (main) for every change.
- **Prior-art check (triage cycles):** Searched by file path across merged history and the
  seam. `crates/custodian/src/reconstruction.rs` history — `3f80642` ("don't count aborted
  repairs as successes") and `5fb905c` (initial reconstruct-from-queue) — neither addresses
  `get_fragment` read-fault classification. The over-broad `.ok().flatten()` candidate was
  tried and **rejected** in #195 iteration 2 (C5/V over-broad-swallow advisory); do not
  reintroduce it. No open/closed PR on main currently fixes `assess`'s read-fault handling.
  The canonical classify-and-continue pattern already exists at `scrub.rs:102` — a
  precedent to mirror, not a duplicate of this fix.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.
</content>
</invoke>

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected on the T5 gap: the permanent-fault classifier's source-chain walk in is_block_read_fault (patch.diff:62-72) is never exercised with a non-trivial chain — the test only feeds a bare Box<io::Error> at depth 0 (permanent_eio_fault, patch.diff:125-127). If production chunkstore-fs wraps the io::Error without preserving source(), the classifier returns false, a real dead sector is mis-classified as transient, and the fix silently no-ops (still aborts the drain) while the mock stays green. Next attempt must close this against the real path, not just the mock: - Add a permanent-fault test fixture that WRAPS the io::Error the way chunkstore-fs actually surfaces it (so the source() walk is exercised at non-zero depth), AND/OR - Cite the chunkstore-fs (fs::read) backend source on getwyrd/wyrd@main showing the EIO io::Error reaches assess with raw_os_error()==Some(5) reachable via std::error::Error::source(). The bare depth-0 fixture is not sufficient evidence of production reach.
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
