# Check review — issue 197 / reconstruction-repaired-success-identity

> Advisory, artifact-only, decorrelated. Inputs: `patch.diff`, `brief.md`,
> `check-gates.json` (build-notes.md withheld). Citations ground on the read-only
> target checkout `/home/eddie/wyrd/wyrd` (an explicit working directory; resolves
> `getwyrd/wyrd @ main`, found in **pre-fix** state — its source matches every `-`
> context line in `patch.diff`, e.g. `reconstruction.rs:172` `Aborted => {}`,
> `:159-161`/`:432-433` identity docs). Where a claim is purely about the diff,
> the cite is `patch.diff`.

## Verdict matrix (5 / 5 / 1)

| Item | Verdict | Basis |
| --- | --- | --- |
| C1 Spec | PASS | `brief.md:18-30` states a concrete, binding, in-process-testable success criterion — derived-successes (`repaired − conflict − aborted`) must equal the `Committed` count, with ≥1 `Aborted` plan in the pass; root cause + invariant + scope are named. |
| C2 Reproduction (red pre-fix) | PASS | New test `an_aborted_repair_is_not_counted_as_a_successful_repair` (`patch.diff` tests +178-294): pre-fix the Aborted arm offsets nothing (`reconstruction.rs:172`) so `aborted=0` → derived `=repaired(2)−conflict(0)−0=2 ≠ committed 1`, failing the final `assert_eq!` (`patch.diff` +288); structural asserts are fix-invariant so they isolate that one line. Corroborated by `check-gates.json` C4-verify=pass. |
| C3 Change | PASS | Minimal and on-seam: wires the Aborted arm to a new `emit_aborted` (`patch.diff` +27, +62-70) mirroring `emit_conflict` (`reconstruction.rs:448-456`), keeps the deliberate up-front emission (no late `tracing`→OTel emission introduced), updates the in-code identity docs to `repaired − conflict − aborted`; all `-` context matches target (`:172`, `:294-296`, `:432-436`). |
| C4 Verification (red→green) | PASS | `check-gates.json`: gating `C4-ci`=pass ("xtask ci: all checks passed" — fmt/clippy/build/test/deny/conformance) and `C4-verify`=pass (per-fix red→green). |
| C5 Causal adequacy | PASS | Root cause = the `Aborted` arm offset by nothing (`reconstruction.rs:172`; abort itself originates at `:333` when the selector picks a server outside the fleet `stores`). Fix offsets exactly that arm on its own counter at the same authoritative seam — causally complete, and the brief's diagnosis is uncontested (the fix matches it 1:1). |
| T1 Structure | PASS | Test compiles on the pre-fix harness — every dependency pre-exists: `write_new_object_placed` (core/src/write), `four_domains` (tests/reconstruction.rs:221), `Topology::register`/`set_utilization` (core/src/placement.rs:89,:99), `DurabilityTelemetry::{metrics_layer,gather_prometheus,flush}` (custodian/src/telemetry.rs:116,:135); the `read_inode`→`read_inode_id` refactor preserves the existing caller (`patch.diff` +78-88). |
| T2 Shape | PASS | Asserts the BINDING identity directly (`derived_successes == committed_count`, `patch.diff` +287-289) and derives `committed_count` from an INDEPENDENT oracle — queue drain + inode-version deltas (`patch.diff` +256-277) — not from the metric, so it is not self-confirming. |
| T3 Runtime | PASS | Green-post-fix (C4-verify) implies the manufactured split actually occurred: commit chunk loses domain C, re-places on free in-fleet C → `Committed` (`reconstruction.rs:376`); abort chunk loses B (util-loaded to 100), least-util free domain is ghost G/server 7, absent from `recon_fleet` `[0,1,2]` → `Aborted` (`:333`). Were the split wrong the structural asserts would also fail post-fix. Selector API `select_distinct_domains_excluding` (placement.rs:265) supports the util/label tie-break relied on. |
| T4 Contribution | PASS | Net-new coverage for the abort-accounting path, disjoint from the existing leg-4 durability assertions (which exercise only the three original M3 metrics, tests header :26-28); it fails pre-fix on exactly the regressed identity, so it earns the red→green and guards against reintroduction. |
| T5 Judgment | PASS | Outcome construction is legitimate — it drives a real `Aborted` via the variant's documented cause (out-of-fleet placement, `reconstruction.rs:294-296`). Advisory caveat (non-blocking): it is coupled to the selector's util/lowest-label tie-break and a ghost-domain trick; if that ordering changes the test could flip outcomes rather than fail loudly. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Always-human. Does restoring the identity by ADDING a 4th durability-plane counter (`reconstruction_aborted`) — vs. gating emission on `Committed` — meet the project's intent, and does the new public metric warrant a companion update to the canonical contract docs the brief cites (proposal 0005 §326-332 "three M3 repair metrics", ADR-0011)? The patch updated only in-code docs. See §6. |

## §6 — Human sign-off items

1. **Validation / fitness-to-purpose (V → NEEDS-HUMAN).**
   - The brief (`brief.md:24-27`) made the *mechanism* Do's call (gate-on-`Committed`
     vs. add-a-counter-and-update-the-identity) and Do chose to add
     `reconstruction_aborted` and redefine the derived-success identity to
     `repaired − conflict − aborted`. Confirm this is the intended public contract.
   - **Contract-doc reach (possible scope residual):** adding a 4th durability-plane
     metric changes the metric set defined in proposal 0005 §326-332 and ADR-0011
     (cited as the Tier-C invariant source in `brief.md:31-37`). The patch updates
     only the *in-code* identity comments, not those external docs. Decide whether a
     companion ADR/proposal update is required and in-scope for this issue, or a
     tracked follow-up (alongside the already-deferred `time_to_repair` item,
     `brief.md:42-48`).

> Note: root cause is **not** contested and scope is **not** ambiguous at the spec
> level (the brief pre-authorized the new-counter approach and explicitly fences off
> `time_to_repair`), so C5 and the scope dimension are passed advisory rather than
> escalated; the only residual human decision is the governance/contract question above.
