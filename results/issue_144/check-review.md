# Check review — issue 144 / reconstruction-custodian

**Posture:** advisory, artifact-only, decorrelated from the builder (build-notes.md withheld).
**Grounding:** `$PDCA_TARGET` could not be read directly (sandbox blocked the env read), so
citations are re-derived against the explicitly-granted working copy at
`/home/eddie/wyrd/wyrd` — the wyrd repo matching the brief's `../wyrd`. Every cited
path:line was opened on that target (pre-patch source); patch-side claims cite `patch.diff`.
No other checkouts were searched.

## Verdict table

| Item | Verdict | Basis |
|------|---------|-------|
| C1 — C1 Spec | PASS | Spec present and binding in the oracle `brief.md:18-36` (defect, success criterion, BINDING vs ILLUSTRATIVE split) plus the authoritative plan pointer `brief.md:9-17` → proposal 0005 §reconstruction. Unambiguous and testable. |
| C2 — C2 Reproduction (red pre-fix) | PASS | No gate configured for C2, but I re-derived the red: `reconstruction.rs` is absent from `crates/custodian/src/` and `crates/custodian/tests/` on target (dir listing), and `reconcile_step` is 5-arg pre-patch (`crates/custodian/src/reconciliation.rs:60-66`), so the kept 6-arg test call is a genuine build-level red on the revert leg — exactly the brief's pre-declared nuance (`brief.md:84-91`). C4-verify gate confirms red→green (`check-gates.json:42-48`). CAVEAT: the *assertion-level* non-vacuous red lives in the withheld `build-notes.md`; I could not re-derive it directly (see §6). |
| C3 — C3 Change | PASS | Diff is coherent and scoped to the M3.6 slice: net-new `reconstruction.rs` consumer (`patch.diff:265-726`), `reconcile_step` dispatch wiring (`patch.diff:241-262`), and four supporting `core` primitives — `domain_of`/`select_distinct_domains_excluding` (`patch.diff:14,43`), `repair::intact_shard` (`patch.diff:139`), `write::encode_ec_fragment` (`patch.diff:164`). No rebalance/scheduler/UI bleed. |
| C4 — C4 Verification (red→green) | PASS | Gating C4-ci `pass` (fmt/clippy/build/test/deny/conformance, `check-gates.json:33-40`) and C4-verify `pass` ("red without the fix, green with it", `check-gates.json:46`). Change ships its own regression test file. |
| C5 — C5 Causal adequacy | PASS | Root cause (no consumer of the shared repair queue) is uncontested and the fix targets it directly: the loop drains `repair::queued_repairs`, rebuilds scheme-driven, and repoints via one CAS commit. Re-derived correctness: `encode_ec_fragment` (`patch.diff:164-171`) is byte-identical to the write path's `encode_chunk` RS branch (`crates/core/src/write.rs:120-133`), and `erasure::{reconstruct,encode}` are pure/deterministic (`crates/core/src/erasure.rs:66,89`), so the rebuilt shard verifies its checksum and the chunk reads back identically. The binding "one version-conditional commit" is a single `WriteBatch` `.require(prior)`+repoint+drain+orphan (`patch.diff:634-643`; traits `WriteBatch::require`, `CommitOutcome` at `crates/traits/src/lib.rs:228,191`); a lost CAS leaves only collectable garbage (`patch.diff:645-650`). Residual durability/crash-interleaving judgment routed to T5/V (§6). |
| T1 — T1 Structure | PASS | Test lives at the gate's discriminator path `crates/custodian/tests/reconstruction.rs` (`patch.diff:776`), mirrors `tests/gc.rs`/`tests/scrub.rs`, and drives the real fenced `reconcile_step` seam — not a test-only entry (`patch.diff:1107`). |
| T2 — T2 Shape | PASS | Asserts on outcomes via public APIs only — `repair::queued_repairs`/`fragment_intact`, `read_object`, the committed inode record — not module internals (`patch.diff:1116-1160`). Checks full redundancy, distinct domains, exactly-one version bump, drained queue. Matches the scrub-test idiom. |
| T3 — T3 Runtime | PASS | C4-ci `pass` means the whole suite (incl. these 5 tests) is green on the builder's run (`check-gates.json:33-40`); tests are `tokio::test`/plain, in-memory, deterministic (no clock/RNG). |
| T4 — T4 Contribution | PASS | Non-vacuous by inspection: assertions pin placement `vec![0,3,2]`, `version==2`, an empty repair queue, all-fragments-intact, 3 distinct domains (`patch.diff:1124-1153`). Skipping the binding commit would leave the queue undrained and placement unchanged → these fire. Five tests map 1:1 to the criterion's binding legs + the checksum-exclusion and priority legs. |
| T5 — T5 Judgment | NEEDS-HUMAN | Oracle is reviewer + human sign-off. Whether the suite adequately covers "every read succeeds **throughout** the repair / a crashed repair leaves collectable garbage, never corruption" is a judgment: the tests prove atomicity via a single commit + version bump but exercise no mid-repair crash interleaving and no concurrent reader during the commit window. See §6. |
| V — Validation — fitness-to-purpose | NEEDS-HUMAN | Always-human. Does this in-process (Option A) slice fulfil M3.6's purpose? Open judgment items: the repair-vs-serve *seat* of proposal 0004 is referenced but not wired (the priority function only orders the drain — `patch.diff:420`), and `time_to_repair` emits the logical instant `now_millis`, not an elapsed window (`patch.diff:704-706`, self-declared placeholder). See §6. |

## §6 — Items the human must clear

1. **(C2) Assertion-level red is unverifiable from artifacts.** The revert-leg red I re-derived is *build-level* (the reverted production drops the dispatch the kept test calls). The brief commits Do to also showing an *assertion-level* red in `build-notes.md` (negate the placement/commit step, à la scrub's `fragment_intact` negation, and show the test fails on "chunk stays under-replicated / obligation not drained"). That file is withheld from this review — confirm the assertion-level red was actually demonstrated, so C4-verify is non-vacuous and not merely a missing-symbol compile error.

2. **(T5) Crash-safety / "reads never error throughout" coverage.** The binding invariant is that a crash mid-repair leaves collectable garbage, never a torn/hybrid chunk, and reads never error during the repair. The implementation structures this correctly (rebuilt fragments written *before* the single CAS commit; displaced fragments orphaned in the *same* commit — `patch.diff:595-643`), but no test exercises a crash between the fragment writes and the commit, nor a reader concurrent with the commit. Confirm this coverage gap is acceptable for sign-off.

3. **(V) Repair-vs-serve seat not wired.** Brief leg (b) asks to "build the seat + priority function." The patch builds the priority *function* and uses it to order the drain, but I see no wiring to the read-retry reserved seat of proposal 0004 — preemption of foreground work is described as conditional/future in the module doc (`patch.diff:356-357`). Confirm "priority function only, seat deferred" satisfies the slice's intent.

4. **(V) Metric fidelity.** `reconstruction_repaired` is incremented for every assessed plan *before* the commit loop (`patch.diff:432-435`); the doc reconciles successes as `repaired − conflict`, but an **Aborted** repair (selector picks a server outside the fleet view — `patch.diff:600-604`) is counted in `repaired` yet subtracted by neither `conflict` nor anything else, so the success count can over-report under that path. And `time_to_repair` reports `now_millis` (absolute logical instant), not an elapsed window. Both are fitness-to-purpose calls, not correctness bugs — confirm acceptable for this milestone.

## Note (advisory, non-gating)

- `find_chunk` linearly scans all `inode:` records per obligation (`patch.diff:655-675`) — O(inodes × obligations) per pass. Fine for the DST substrate; flag for the eventual deployed custodian.
- No FAIL findings. C1–C4 and T1–T4 re-derived as PASS; the two NEEDS-HUMAN rows are the designed always-human gates plus the withheld-artifact caveat in §6.1.
