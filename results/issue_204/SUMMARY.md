# Result — issue 204 / fschunkstore-offload-blocking-io

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: `FsChunkStore` implements the **async** `ChunkStore` trait with
- Success criterion: Under a sustained burst of concurrent storage I/O (writes and/or
- Repo + branch target: getwyrd/wyrd @ main   (resolve here at Plan — do not leave to Do)
- Scope (one logical fix) / out of scope: move the filesystem store's blocking `std::fs` I/O (including the

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
- C5 Causal adequacy: none — reviewer + human sign-off

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 Contribution: none — (no gate configured)
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

# Check review — issue 204 / fschunkstore-offload-blocking-io

> Advisory, artifact-only, decorrelated from the builder. Inputs: `patch.diff`,
> `brief.md`, `check-gates.json` (build-notes.md withheld). Citations are
> re-derived against the target source at `$PDCA_TARGET=/home/eddie/wyrd/wyrd`
> (read-only). NOTE on grounding: the target file is plain `main` (the brief's
> pre-fix defect lines reproduce exactly there), but the patch is authored on the
> **203+207-merged** base and lands last in that file's merge chain — so the
> patch's *added* code cannot be located on the target; those rows cite
> `patch.diff` by hunk and the target only for the lines the patch replaces.

## Verdict table

| Item | Verdict | Basis |
|------|---------|-------|
| C1 — C1 Spec | PASS | Brief states a binding, testable property — reactor liveness: "blocking filesystem syscalls no longer run on a reactor worker thread … heartbeat/other RPCs stay live under storage load" — and explicitly separates BINDING from ILLUSTRATIVE mechanism (`brief.md:25-35`); invariant scoped to *every* blocking syscall in the async path (`brief.md:36-49`). Clear oracle. |
| C2 — C2 Reproduction (red pre-fix) | PASS | Re-derived: pre-fix `list_fragments` body is synchronous `std::fs` with no `.await` (`lib.rs:99-136`), so on a 1-worker runtime the burst task polls 500 ready-immediately futures and never yields, pinning the sole worker → co-scheduled timer cannot run → `during == 0 < 5` (`tests/blocking_offload.rs:341,414-420` in patch). Valid red. Gate `C4-verify` corroborates "red without the fix" (`check-gates.json:46`). |
| C3 — C3 Change | PASS | Diff adds an `offload` helper and wraps every async `ChunkStore` method's blocking syscalls in it — put/get/list/delete/health (`patch.diff` hunks at lib.rs `+125,+171,+211,+264,+279`) — and applies the in-scope steady-state `create_dir_all` skip via `ErrorKind::NotFound`-on-write (`patch.diff` lib.rs `+139-149`). Matches brief Scope (`brief.md:65-77`). |
| C4 — C4 Verification (red→green) | PASS | Both gates green: `C4-ci` (fmt/clippy/build/test/deny/conformance) PASS and `C4-verify` "PASS — red without the fix, green with it" (`check-gates.json:33-48`); `overall: pass` (`check-gates.json:3`). Post-fix each method awaits `spawn_blocking`, freeing the worker so the timer interleaves — consistent with the asserted green. |
| C5 — C5 Causal adequacy | PASS | Re-derived that the invariant (no blocking fs syscall on a reactor worker thread, across **all** async store methods) is fully restored: put/get/list/delete/health each route their `std::fs` calls through `offload` → `spawn_blocking` when a tokio runtime is current (`patch.diff` offload `+73-78`). On-reactor residue is CPU-only (`Self::verify`, path joins), not a syscall. `open()`'s `create_dir_all` (`lib.rs:33`) is out of scope — a sync constructor run before `runtime.block_on` (`cli.rs:266,274`). Root cause (blocking the executor thread starves timers) is uncontested and matches the runtime contract cited in the brief (`brief.md:41-44`); d-server starvation path confirmed: multi-thread runtime (`cli.rs:271`, `new_multi_thread().enable_all()`) and serve+renew share one `tokio::select!` (`dserver.rs:~178`). |
| T1 — T1 Structure | PASS | Net-new integration test at the brief-specified path `crates/chunkstore-fs/tests/blocking_offload.rs` (`brief.md:84`); single focused test + `populate` helper, documented intent (`tests/blocking_offload.rs:293-321,356`). |
| T2 — T2 Shape | PASS | Shape matches the verification posture (`brief.md:86-93`): 1-worker `new_multi_thread` runtime, co-scheduled timer task vs. storage-I/O burst over the populated store, assert timer advanced *during* the burst (`tests/blocking_offload.rs:365-420`). Exercises `list_fragments`, the brief's named worst starvation source. |
| T3 — T3 Runtime | PASS | Determinism re-derived: burst = 500 walks × (64 dirs × 16 files) ≈ 5×10^5 dir-entry ops (`tests/blocking_offload.rs:331-336`), far exceeding the ~5 ms a 1 ms timer needs for 5 ticks, so post-fix margin is large and pre-fix is a hard 0; threshold 5 sits inside that gap (`tests/blocking_offload.rs:337-341`). No real-disk-timing dependence; gate `C4-verify` confirms it flips. Residual: relies on entry-volume rather than an injected slow-syscall seam — acceptable per brief, the thinnest part of the evidence. |
| T4 — T4 Contribution | PASS | Genuine regression guard: reverting the offload returns the test to 0 ticks (red). It asserts a real behavioural property, not a tautology, and pins the binding criterion. Caveat (not failing): only the `list_fragments` path is behaviourally exercised; put/get/delete/health offload is verified structurally (C5), not by this test. |
| T5 — T5 Judgment | PASS | Test is honest — no mock of the offload, the assertion is the actual property under test, threshold is justified in-line (`tests/blocking_offload.rs:337-341`), and the failure message names the defect (`tests/blocking_offload.rs:414-420`). No gaming detected by re-derivation. |
| V — Validation — fitness-to-purpose | NEEDS-HUMAN | Always-human. The C4 test demonstrates the property on a *constrained 1-worker* proxy; the brief itself scopes the production confirmation — heartbeat stays live on a live d-server under real load — as supplementary deferred-green, off-Check (`brief.md:96-98`). A human must judge that the constrained-runtime evidence is fit to accept the production claim. → §6.1 |

## §6 — Human items

1. **§6.1 Validation fitness-to-purpose (V).** Confirm the constrained 1-worker
   behavioural test is accepted as sufficient evidence for the production
   criterion (lease-renew heartbeat survives storage-load bursts on the live
   multi-thread d-server), per the brief's own supplementary/deferred-green
   framing (`brief.md:96-98`). This is the only blocking human sign-off.

## Notes (non-gating, for the human)

- **Base mismatch is expected, not a defect.** The patch's `put_fragment`
  comments assume a *unique-per-call* scratch name ("its name is unique";
  `patch.diff` lib.rs `+126-138`); that uniqueness is 203's contribution. The
  target's `temp_path` is still the fixed `{:05}.tmp` (`lib.rs:45-49`) because
  203 is not merged into the read-only target. Consistent with the brief's
  declared 203→207→204 merge chain (`brief.md:51-64`); verify on the merged base,
  not the target.
- **`offload` panic propagation.** A panic in the blocking closure surfaces via
  `.expect("storage blocking task panicked")` (`patch.diff` offload `+75-76`)
  rather than as an error — same severity as the pre-fix inline panic, so no
  regression.
- **tokio feature minimality is correct.** `spawn_blocking` and
  `Handle::try_current` both live in the `rt` feature, matching the
  `default-features = false, features = ["rt"]` non-dev dep (`patch.diff`
  Cargo.toml `+29`); the `madsim`/`not(madsim)` split follows the established
  workspace `--cfg madsim` convention (proto/build.rs, dst crate).


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] V — Validation — fitness-to-purpose — Always-human. The C4 test demonstrates the property on a *constrained 1-worker* proxy; the brief itself scopes the production confirmation — heartbeat stays live on a live d-server under real load — as supplementary deferred-green, off-Check (`brief.md:96-98`). A human must judge that the constrained-runtime evidence is fit to accept the production claim. → §6.1

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: merged-wider
- Iteration delta (if iterating):
- By / date: Eduard Ralph / 2026-06-24

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
