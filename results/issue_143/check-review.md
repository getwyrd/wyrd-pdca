# Check review — issue 143 / m3.5-scrub-custodian

Advisory, artifact-only, decorrelated. Inputs: `patch.diff`, `brief.md`, `check-gates.json`
(build-notes withheld). Citations re-derived against the target source
`$PDCA_TARGET = /home/eddie/wyrd/wyrd` (read-only, pre-patch `main`) and against `patch.diff`
for files the patch creates. Confirmed on target that `crates/core/src/repair.rs`,
`crates/custodian/src/scrub.rs`, `crates/custodian/tests/scrub.rs`, and
`crates/core/tests/read_repair.rs` do **not** exist — the slice is net-new as the brief claims.

## Verdict table

| Item | Verdict | Basis |
|------|---------|-------|
| C1 — C1 Spec | PASS | Defect is concrete and grounded: the read path excludes a checksum-failing fragment and reconstructs around it but **never records a repair obligation** (`crates/core/src/read.rs:12-16` doc; `read.rs:142-153` silently drops the `decode` `Err` arm), and no scrub loop / repair queue exists on target. Invariant + 4 binding legs stated in `brief.md:52-60`, anchored to `0005:262-267`/`0005:174-176`. |
| C2 — C2 Reproduction (red pre-fix) | PASS | Net-new posture (`brief.md:84-91`): "red" is partly criterion-absence — target lacks `repair.rs`/`scrub.rs` (verified), so the new assertions cannot pass pre-fix. Load-bearing legs carry documented flippable demonstrations (`patch.diff:254` read-path enqueue; `patch.diff:1073-1075` the `header.chunk_id` half). The executed red→green run lives in withheld build-notes; the `C4-verify` gate ("red without the fix, green with it", `check-gates.json:46`) is the only machine evidence and was not independently re-run here. |
| C3 — C3 Change | PASS | The diff realizes exactly the scoped seam: shared repair queue (`patch.diff:145-233`, new `core/src/repair.rs`), scrub loop (`patch.diff:603-732`, new `custodian/src/scrub.rs`), `reconcile_step` dispatch of scrub (`patch.diff:568-601`), and the read-path feed onto the same queue (`patch.diff:124-141`). No dequeue/rebuild — scope held (`brief.md:74-78`). |
| C4 — C4 Verification (red→green) | PASS | Per gates only: `C4-ci` pass (`check-gates.json:33-39`) and `C4-verify` pass (`check-gates.json:42-48`). Mechanical gate results — I cannot re-run them artifact-only; they verify build/test/fmt/clippy green and one red→green cycle, not correctness of the causal claim (see C5). |
| C5 — C5 Causal adequacy | NEEDS-HUMAN | Root cause (no proactive scrub, no shared queue, read silently absorbs) is addressed, BUT an asymmetry: scrub's verify checks both checksum **and** `header.chunk_id == chunk` (`patch.diff:203-205`), while the read path's inline `decode` only catches a checksum `Err` and never re-checks the header on the `Ok` arm (`patch.diff:106-122`). A misplaced-but-intact fragment is thus excluded+enqueued by scrub yet silently fed to the decoder on read. Whether that is in-scope (binding leg 4 names only checksum failure, `brief.md:48-51`) is a contested root-cause judgment; oracle is reviewer + human (`check-gates.json:53`). |
| T1 — T1 Structure | PASS | Two well-formed, in-tree test files modelled on the existing `crates/custodian/tests/gc.rs`: `crates/custodian/tests/scrub.rs` (`patch.diff:782-1181`) and `crates/core/tests/read_repair.rs` (`patch.diff:234-494`), exactly the homes the brief names (`brief.md:79-83`). |
| T2 — T2 Shape | PASS | Assertions test the binding behaviours, not incidentals: enqueue onto the **shared** queue via `repair::repair_key` (`patch.diff:434-443`, `1042-1053`), read-around recovery (`patch.diff:425-431`), scrub-never-deletes (`patch.diff:1057-1060`), telemetry surfaces read back in-process (`patch.diff:1173-1180`), and the misplaced-header detection (`patch.diff:1110-1121`). |
| T3 — T3 Runtime | PASS | Tests are `#[tokio::test]`, driven through the real `reconcile_step` fenced control point (`patch.diff:994`, `1031`, `1099`, `1163`), not a test-only entry. Green status rests on the `C4-ci` gate (`check-gates.json:33-39`); not independently executed here. |
| T4 — T4 Contribution | PASS | Each binding leg is pinned by a load-bearing/flippable assertion: read-path enqueue survives a failed read (`patch.diff:484-493`), the EC read enqueues while still reconstructing (`patch.diff:425-443`), and the scrub misplaced-header case isolates the `chunk_id` half (`patch.diff:1102-1121`). Leg-3 telemetry uses a substring check on the Prometheus surface (`patch.diff:1173-1180`) — weaker, but consistent with the established `gc.rs` pattern. |
| T5 — T5 Judgment | NEEDS-HUMAN | The iteration-1 carry-forward gap (`brief.md:107`) — exercise the misplaced-but-intact path — is **closed for scrub** (`patch.diff:1076-1128` asserts detect+exclude+enqueue and pins `header.chunk_id == chunk`). The brief allowed scrub and/or read-path (`brief.md:107`), so the literal ask is met; but no read-path regression exists and the read path cannot detect that case (see C5). Whether read-path parity is required is a judgment for sign-off; oracle reviewer + human (`check-gates.json:98`). |
| V — Validation — fitness-to-purpose | NEEDS-HUMAN | Always-human (`check-gates.json:105-107`). Does the realized slice — scrub producing repair obligations + read feeding the same queue, no consumer yet — actually serve the milestone intent (`0005:528-530`)? Human at sign-off. |

## §6 — items the human must clear

1. **C5 (contested root-cause / scope).** Read-path inline `decode` does not apply the
   `header.chunk_id == chunk` half of `repair::fragment_intact` (`patch.diff:106-122` vs
   `patch.diff:203-205`), so a misplaced-but-intact fragment is detected by scrub but silently
   absorbed by the read decoder. The `repair.rs` doc asserts the read path "decodes for the same
   effect inline" (`patch.diff:201-202`) — that overstates parity. Decide whether this asymmetry
   is acceptable scope (leg 4 names only checksum failure) or a genuine read-path corruption gap.

2. **T5 (judgment).** The misplaced-intact regression exists only for scrub
   (`patch.diff:1076-1128`). Confirm the scrub-only coverage discharges the iteration-1
   carry-forward, or require an equivalent read-path guard + regression.

3. **V (fitness-to-purpose).** Confirm the slice as built meets the milestone-3 scrub DoD
   (`0005:528-530`) given it only *produces* repair obligations (no reconstruction consumer until
   slice 6).

## Caveats on this review

- C2/C4/T3 lean on gate results in `check-gates.json` that are mechanical and were not
  independently re-executed in this artifact-only pass; a green gate confirms build/test
  passage, not the causal claim. The demonstrated-red detail for C2 lives in the withheld
  build-notes.
- All other rows were re-derived against the target source; cited `read.rs`/`gc.rs`/`traits`
  /`metadata.rs`/`chunk-format` symbols and signatures were verified to exist as the patch uses them.
