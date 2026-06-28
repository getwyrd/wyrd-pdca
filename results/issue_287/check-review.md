# Check review — issue 287 / gc-identity-placement-fallback

> Advisory, artifact-only. Inputs: patch.diff, brief.md, check-gates.json
> (build-notes.md withheld by design). `PDCA_TARGET` not readable in this
> sandbox and multiple `wyrd` checkouts exist on the machine, so per protocol
> I did **not** wander into them — every citation is grounded on `patch.diff`,
> whose context lines self-corroborate the brief's cited locations
> (`metadata.rs:94`, `read.rs:96-105`, `gc.rs:186`, `reconstruction.rs:223`).

## Verdict table (5/5/1)

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Brief states a precise invariant: GC's reference set must equal the read path's resolved placement closure incl. identity fallback (brief §Invariant; `gc.rs:24,110`). Success criterion is concrete and falsifiable. Decision owed: none. |
| C2 Reproduction (red pre-fix) | PASS | Three regressions assert the pre-fix red path explicitly: `placement.iter()` over `vec![]`/short vector yields an empty/partial set → orphan triggers `delete_fragment` (`patch.diff:143-149,215,281`). check-gates `C4-verify` confirms red→green ran. |
| C3 Change | PASS | Coherent: one shared resolver `ChunkRef::placed_dserver`/`fragment_count` added in `metadata.rs` (`patch.diff:15,31`); read/gc/reconstruction all delegate (`patch.diff:60,84,114`). GC loop changes from `placement.iter().enumerate()` to `0..fragment_count()` (`patch.diff:72→81`) — exactly the cited defect site. |
| C4 Verification (red→green) | PASS | check-gates.json: gating `C4-ci` (fmt/clippy/build/test/deny/conformance) = pass; `C4-verify` per-fix red→green = pass; overall = pass. No re-run available locally (target not resolvable), so resting on the recorded gate result, which is the authoritative oracle. |
| C5 Causal adequacy | PASS | Root cause = GC's reference set diverging from the read path's placement resolution; the fix *removes* the divergence by giving all four callers one resolver, not by guarding a symptom. The C5 probe/guard smell-test does NOT fire: the `unwrap_or` identity fallback is pre-existing read-path semantics (changing it is brief-scoped-out), being mirrored — cause transformed, not papered over. The alternative root cause (migrate pre-M3 records to explicit placement so no fallback is needed) was settled out-of-scope by the planner; the residual judgment rides on the Validation row. |
| T1 Structure | PASS | Resolver lives on `ChunkRef` in `metadata.rs` (owner of the data); callers delegate. Eliminates the 3rd inlined copy the prior iteration was rejected for (`patch.diff:9-37,55-60,113-114`). |
| T2 Shape | PASS | Tests added as discrete `#[tokio::test]` fns in the designated `crates/custodian/tests/gc.rs` (`patch.diff:151,219,285`), each isolating one expansion case — matches the brief's Test file. |
| T3 Runtime | PASS | `cargo xtask ci` green per check-gates (build+clippy+test). `fragment_count(): u16` over `u8` k,m cannot overflow (max 510); loop/`FragmentId.index` types align (u16). |
| T4 Contribution | PASS | Net change is the binding requirement (GC reference set) plus the brief's *preferred* centralization, with no scope creep into orphan/lease semantics or read-path behaviour. Prior-art was documented in the brief (gc.rs/read.rs history, no open PR) but I could not mechanically re-run `gh`/git here — see NEEDS-HUMAN note below if unconfirmed at sign-off. |
| T5 Judgment | PASS | Both prior-iteration rejection flags are resolved: RS k+m expansion is now tested (4b, orphan at index 1) and short/partial-vector merge is tested (4c, orphan at fallback index 2), and the logic is centralized. Tests are flippable as documented. Test selection adequately covers the stated category, not just the `vec![]` repro. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decision owed: confirm that mirroring the read path's identity fallback into GC (rather than migrating committed pre-M3 records to explicit placement) is the durability posture wyrd wants long-term — the patch makes GC *tolerant* of fallback-placed fragments forever, which is correct-by-the-brief but is a standing design commitment a human must own at sign-off. Also confirm the prior-art check (merged history + closed/rejected PRs on gc.rs/read.rs/metadata.rs/reconstruction.rs) since it could not be mechanically re-run in this sandbox. |

## Notes
- All three regressions re-derive as correct and genuinely red pre-fix:
  4a None/`vec![]`→index0→dserver0; 4b RS{2,1}/`vec![]`→index1→dserver1
  (proves `fragment_count`=3); 4c RS{2,1}/`vec![5]`→index2→dserver2 (proves
  mixed explicit+fallback). Each orphan sits exactly on the fallback-resolved
  `(dserver, FragmentId)` so the pre-fix empty/partial set would reclaim it.
- No blocking findings. C4 not independently re-run (target unreadable); this
  is a target-state caveat, not a patch defect — the recorded gate is green.
