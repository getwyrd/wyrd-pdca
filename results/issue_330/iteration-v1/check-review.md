# Check review — issue 330 / scrub-detect-missing-placed-fragment

**Task under review:** On `main`, no production maintenance path turns a *simply-absent*
committed-referenced fragment into a repair obligation — scrub only inspects fragments a
D server's `list_fragments()` enumerates, so a missing fragment is never even visited and
its `Ok(None)` fetch arm silently `continue`s. The fix must make a production scrub pass
enqueue a durable repair obligation for a placed-but-absent fragment, without false
positives for in-flight writes, pending-GC/expired-lease windows, or orphan fragments.

**Review conditions (caveats, not patch defects):**
- `$PDCA_TARGET` could not be resolved in this sandbox (env access blocked; the per-cycle
  worktree is not present in the review dir). Per protocol I ground citations on
  `patch.diff` alone and did not wander into other checkouts.
- No Rust toolchain is reachable here (`which cargo rustc` → non-zero; gate `C4-ci` reports
  `cargo: not found`). I therefore could **not** independently re-run red→green. Both C4
  gate failures trace to `cargo: not found` — a toolchain-absent environment, **not** a
  patch defect — so I do not raise them as a blocking C4 FAIL (that would fabricate an
  ordering-gate blocker). The red→green claim is assessed by inspection and handed to the
  human to execute with a real toolchain.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Patch targets exactly the briefed gap: scrub now drives off the committed reference set and enqueues on placed-but-absent (`patch.diff` scrub.rs `Ok(None)` arm, +113–117; grouping +75–78). Out-of-scope killed/partitioned D-server case is naturally excluded because the outer loop still iterates `ctx.fleet`, so a referenced dserver absent from the fleet is never probed — matches brief scope. |
| C2 Reproduction (red pre-fix) | PASS | New test `detects_a_missing_placed_fragment...` (`patch.diff` +174–207) is genuinely fix-coupled: d0 holds no bytes, so pre-fix the `list_fragments()`-driven loop never visits the frag and asserts `Changed`/`vec![chunk]` fail; reverting only the `Ok(None)` arm to `continue` also flips it red (comment +170–173). Credible by inspection; not executed (no toolchain). |
| C3 Change | PASS | Minimal, coherent: group reference set by placed dserver, fetch each by id via existing `get_fragment`, treat `Ok(None)` as loss, new `emit_missing` mirrors `emit_corruption` (`patch.diff` +142–152). No signature churn beyond a `HashMap` import (+43). |
| C4 Verification (red→green) | NEEDS-HUMAN | Gate `C4-ci` and `C4-verify` both FAIL on `cargo: not found` (check-gates.json:37, :46) — toolchain-absent, not a patch defect; I confirmed no cargo/rustc here so I could not re-run. **Decision owed:** run `run-verify.sh` / `cargo xtask ci` on a machine with the Rust toolchain and confirm the two new tests are red pre-fix and green post-fix; the reported "RED with fix" is a build-can't-run artifact of the missing toolchain, not observed test failure. |
| C5 Causal adequacy | PASS | Symptom-guard smell-test does **not** fire: the fix adds no capability probe / runtime guard around a present capability — it removes the root gap (iteration bounded by `list_fragments()`) by driving detection off the committed reference set, so an absent fragment is now asked-for by id. Root cause, not a papered-over probe. |
| T1 Structure | PASS | Detection lands in the existing `scrub::reconcile` seam; reuses `repair::enqueue_repair` and the shared queue; `emit_missing` parallels the existing emit helpers (`patch.diff` +137–152). No layering violation (custodian gains no chunk-format knowledge). |
| T2 Shape | PASS | No public API/signature change; `ChunkStore::get_fragment` already existed. `traits/src/lib.rs` doc updated to reflect scrub driving off the reference set (`patch.diff` +265–284). Consistent shape. |
| T3 Runtime | PASS | By inspection: `HashMap` grouping and by-dserver lookup are total, no unwrap/panic on the new path, `Ok(None)` handled explicitly. Rests on C4 execution (above) for empirical confirmation — not run here. |
| T4 Contribution | PASS | Two tests add real coverage: the missing-detection leg plus a false-positive guardrail (`does_not_flag_an_in_flight_pending_writes_fragment...`, `patch.diff` +211–260). Not tautological — coupled to the changed arm. |
| T5 Judgment | NEEDS-HUMAN | The fix treats **any** committed-referenced `Ok(None)` as durable loss. Brief requires guardrails for in-flight writes, **pending/expired-lease GC**, and orphan fragments; only the in-flight case has a test, and both the in-flight and pending-GC guarantees rest on `gc::referenced_fragments` filtering committed-only + `write.rs:220` commit-after-ack — neither visible in the patch nor verifiable here (target unreadable). **Decision owed:** confirm the reference set truly excludes pending-GC/expired-lease and orphan fragments so those windows can't yield false-positive repair obligations, and decide whether the untested pending-GC window warrants added coverage before ship. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | **Decision owed:** confirm this scrub-path detector actually restores the durability invariant in production (not merely the `MemStore`/`MemMeta` harness), that shipping with the killed/partitioned D-server case deliberately out of scope is acceptable with a follow-up tracked, and that leaving the #250/#196 `enqueue_repair` test stand-ins in place is the intended disposition. Fitness-to-purpose is human-owned at sign-off. |
