# Check review — issue 251 / reconstruction-read-around-fragment-read-fault

> Advisory, artifact-only, decorrelated from the builder. Inputs: `patch.diff`,
> `brief.md`, `check-gates.json`. `build-notes.md` deliberately withheld.
>
> **Grounding note:** `$PDCA_TARGET` could not be confirmed (every env-read was
> blocked by the sandbox). Per the standing rule, citations below are therefore
> re-derived against `patch.diff` alone; I did **not** read other checkouts on the
> machine. Any claim about how the *production* `chunkstore-fs` backend shapes an
> `EIO` is therefore unverified against target source and is flagged for the human
> (see §6).

## Verdict table

| Item | Verdict | Basis |
|------|---------|-------|
| C1 — C1 Spec | PASS | Brief's success criterion is well-formed and testable: read-around on a *permanent* placed-fragment fault → `Repairable`; a *transient* fault is NOT converted to re-placement (`brief.md:18-28`). It declares the exact `Assessment` variant ILLUSTRATIVE and the two behaviours BINDING, so the patch's choice to assert at the `reconcile_step` integration layer (`Reconciled::Changed`) rather than on the `Assessment` enum is within spec. No spec gap owed. |
| C2 — C2 Reproduction (red pre-fix) | PASS | The flippable test is genuinely red pre-fix: pre-fix `assess` does `get_fragment(frag).await?` so the `EIO` propagates → `reconcile_step` returns `Err` → `result.expect(...)` panics (`patch.diff:192-194`). Post-fix it is classified to `None` and read around. `check-gates.json` C4-verify = pass corroborates the red→green flip ("as its own file to earn the full red->green"). The transient test is correctly characterised as a *discriminating guard*, not a red→green flip (`patch.diff:230-235`) — appropriate, not a C2 defect. |
| C3 — C3 Change | PASS | Diff is minimal and scope-faithful: only `reconstruction.rs` production code touched; `scrub.rs`/`read.rs`/on-disk format/non-placed fetches all untouched, matching the brief's out-of-scope list (`brief.md:61-72`). Replaces the bare `?` with an explicit three-arm classify (`patch.diff:21-25`), mirroring the `scrub.rs:102` precedent the brief names. The rejected `.ok().flatten()` is not reintroduced. |
| C4 — C4 Verification (red→green) | PASS | `check-gates.json`: gating `C4-ci` (fmt/clippy/build/test/deny/conformance) = pass; `C4-verify` (per-fix red→green) = pass. Both green; the one gating row passed. Re-derivable: the patch compiles to a closed match and adds only pure-fn helpers, consistent with a clean `xtask ci`. |
| C5 — C5 Causal adequacy | PASS | Root cause is uncontested and correctly removed at source: the `?` at the `get_fragment` call propagated *every* non-`NotFound` error (`brief.md:9-17`); the fix classifies instead of swallowing — `is_integrity_fault \|\| is_block_read_fault` → read-around, else propagate (`patch.diff:54-56`, `patch.diff:21-25`). The transient leg is preserved by classification, not by an over-broad swallow, so the brief's SELF-TEST against `.ok().flatten()` is satisfied. **Carried to §6:** the production-faithfulness of the `EIO` classification (does real `chunkstore-fs` expose `raw_os_error()==Some(5)` reachable via `source()`?) is not provable from artifacts. |
| T1 — T1 Structure | PASS | Tests live in the brief-named file `crates/custodian/tests/reconstruction.rs` (`brief.md:80`), reuse the existing harness (`MemDServer`, `Fleet`, `reconcile_step`, `elect`, `write_rs_2_1`), and add one focused mock (`FaultGetStore`, `patch.diff:93-119`) plus two named tests. Mock delegates all non-faulting ops to a healthy inner store so placement of the rebuilt fragment still works (`patch.diff:100-118`). |
| T2 — T2 Shape | PASS | Assertions are substantive, not tautological. Permanent leg pins `Reconciled::Changed`, drained queue, `version==2` (one commit), exact placement `vec![0,3,2]`, and re-checks the rebuilt fragment's checksum/ownership (`patch.diff:195-222`). Transient leg pins `is_err`, queue still `contains(&CHUNK)`, `version==1`, unchanged placement `vec![0,1,2]` (`patch.diff:277-301`). Both legs assert the *full* observable effect, including the negative (no re-placement). |
| T3 — T3 Runtime | PASS | Both tests ran green under the passing `C4-ci`/`C4-verify` gates. Fault shapes are constructible and classify as designed: `permanent_eio_fault` = `Box<io::Error::from_raw_os_error(5)>` → `downcast_ref::<io::Error>().raw_os_error()==Some(5)` → permanent (`patch.diff:62-72,125-127`); `transient_fault` = `io::Error::new(ConnectionReset, …)` whose `raw_os_error()` is `None` → not permanent → propagated (`patch.diff:132-137`). |
| T4 — T4 Contribution | PASS | Net-new coverage of the new seam: the read-around leg (previously absent) and the discriminating transient guard that specifically pins out the rejected `.ok().flatten()` regression (`patch.diff:230-235`). Not redundant with the existing `a_checksum_failing_fragment_is_excluded_and_reconstructed` test (that exercises the integrity, not the read-fault, path). |
| T5 — T5 Judgment | NEEDS-HUMAN | DECISION OWED: the in-process permanent fault is a **bare** `Box<io::Error>` found at source-chain depth 0 (`patch.diff:125-127`), so the `while … e.source()` walk in `is_block_read_fault` (`patch.diff:62-72`) — the exact logic that classifies a *wrapped* production EIO — is never exercised with a non-trivial chain. The "Production reach" claim (`brief.md:92-98`) that this load-bearingly covers the real path rests on `chunkstore-fs` surfacing `EIO` with `raw_os_error()==Some(5)` reachable through `source()`. A human must confirm that against the real backend, or the test should wrap the `io::Error` the way the backend does; the mock could pass while production silently mis-classifies a wrapped EIO as transient. |
| V — Validation — fitness-to-purpose | NEEDS-HUMAN | DECISION OWED (always-human): (a) confirm the chosen permanent boundary — `EIO` (errno 5) as the *sole* block-layer permanent errno (`patch.diff:37`) — is the right line for Wyrd's targets, not too narrow (e.g. other device errnos) nor too broad (a transient condition that can surface as EIO). (b) Confirm the accepted residual: a *transient* fault still propagates out of `reconcile_step` and so still stalls the shared per-chunk drain for that pass (`patch.diff:24`, transient test asserts `is_err`, `patch.diff:277-281`); the brief endorses "propagate to the retry policy" (`brief.md:33-35`), but whether that retry policy actually exists/bounds the stall is a fitness question outside these artifacts. |

## §6 — Items the human must clear

1. **(from T5) Production EIO shape is untested.** The classifier's source-chain
   walk is only ever hit at depth 0 by a bare `Box<io::Error>`. Verify that real
   `chunkstore-fs` (`fs::read`) surfaces an `EIO` whose `raw_os_error()==Some(5)`
   is reachable via `std::error::Error::source()` through whatever wrapper the
   backend applies. If the backend wraps without preserving the source, the fix
   no-ops in production (a real dead sector would be mis-classified as transient
   and would *still* abort the drain). Cheapest closure: add a test fault that
   wraps the `io::Error` the way `chunkstore-fs` does, or cite the backend source
   showing the chain is preserved.

2. **(from V) Permanent/transient boundary at errno granularity.** Decide whether
   `EIO` alone is the correct permanent block-layer signal for Wyrd's platforms.
   Both directions carry risk: a permanent device fault that reports a *different*
   errno would be propagated (stalling the queue), and any transient path that can
   legitimately yield `EIO` would be silently absorbed as permanent loss + a
   re-placement. The brief names `EIO`/dead-sector as the representative
   (`brief.md:30-33`) but does not enumerate the full set.

3. **(from V) Residual shared-queue stall on transient faults.** The fix de-stalls
   the drain only for *permanent* faults; a transient fault on one placed fragment
   still returns `Err` from the whole pass. Confirm the downstream retry policy
   bounds this so a single transiently-unreachable D server cannot indefinitely
   stall repair for every chunk that touches it — the original symptom the brief
   set out to remove (`brief.md:13-17`), now narrowed to the transient case by
   design.

## Summary

Eight of eleven elements PASS on re-derivation: the change is minimal,
scope-faithful, mirrors the named `scrub.rs:102` precedent, removes the root-cause
`?` propagation rather than swallowing it, and ships both a flippable red→green
assertion and a discriminating guard against the rejected `.ok().flatten()`
regression. The gating `C4-ci` is green. Two always-human elements (T5, V) plus the
single substantive technical gap they surface — that the in-process bare-`io::Error`
fault never exercises the source-chain walk that production wrapping would require —
are the items owed to the human at sign-off.
