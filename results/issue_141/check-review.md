# Check review — issue 141 / m3.3-custodian-skeleton (Option B)

Advisory, artifact-only, decorrelated from the builder. Inputs: `patch.diff`,
`brief.md`, `check-gates.json`. `build-notes.md` deliberately withheld. Citations
re-derived against the target source at `/home/eddie/wyrd/wyrd` (pre-patch base —
no `custodian` crate present, so it is the `index % n` base) and against
`patch.diff`.

## Verdict table

| Item | Verdict | Basis |
|------|---------|-------|
| C1 — C1 Spec | PASS | Defect and binding criteria are crisply specified (`brief.md:29-48`); leg (3) "production write path wired… records a distinct-domain placement (NOT the identity vector)" is the unambiguous target. Spec is verifiable. |
| C2 — C2 Reproduction (red pre-fix) | NEEDS-HUMAN | No gate (`check-gates.json:15-21`). The "flippable" leg `domain_placement.rs` calls **net-new** API (`write::write_new_object_placed`, `wyrd_core::placement::*`, patch.diff:1094) — pre-patch it cannot compile, so there is no behavioral red to flip; it is not the genuine red→green the brief claims (`brief.md:96-99`). The required demonstrated negation-red for the NET-NEW legs lives in withheld `build-notes.md`; cannot confirm from artifacts. |
| C3 — C3 Change | PASS | Concrete, coherent diff: new `custodian` crate (patch.diff:1168-1751), `core::placement` selector (patch.diff:471-718), write/registration rewire; gate `C4-ci` green (`check-gates.json:33-39`) proves it builds + clippy/fmt/deny pass. |
| C4 — C4 Verification (red→green) | FAIL | Gate `C4-verify` = **fail** (`check-gates.json:42-48`). A green `C4-ci` proves build/test/deny green, NOT red→green. The behavioral leg can't go red pre-fix because it drives a brand-new function rather than flipping existing write behavior (patch.diff:878-901 vs the unchanged production write). |
| C5 — C5 Causal adequacy | FAIL | Root cause = the **production write records identity `index % n`** (`core/write.rs` chunk_refs/plan_write). The fix adds a *parallel* `write_new_object_placed` that calls `.place()` (patch.diff:878-901) but the production gateway write `cluster_store_put` is **unchanged in body** — it still calls `write::write_new_object` (target `crates/server/src/cli.rs:460`), whose plan keeps the default identity placement `(0..fragments.len())` (patch.diff:817) and never calls `.place()`. `index % n` is **not** retired on the real write path. |
| T1 — T1 Structure | PASS | Three well-formed new test files exist as the brief names them: `crates/custodian/tests/skeleton.rs` (patch.diff:1597), `crates/core/tests/domain_placement.rs` (patch.diff:906), plus `crates/server/tests/failure_domain_registration.rs` (patch.diff:1994); all compile/run under green `C4-ci`. |
| T2 — T2 Shape | PASS | Assertions encode the right binding shapes: placement ≠ identity + n distinct domains + read reconstructs (patch.diff:1122-1141), fence rejects a deposed leader (patch.diff:1660-1664), selector refusal (patch.diff:1706-1710), metric exported (patch.diff:1747-1750). Caveat: `skeleton.rs` leg-2 re-derives domain labels from id (patch.diff:1689-1695) rather than reading the topology — weaker but not wrong. |
| T3 — T3 Runtime | PASS | Tests execute and pass — `C4-ci` (which runs `test`) is green (`check-gates.json:33-39`); the `Both` exporter (Prometheus + OTLP-tonic) constructs without a live collector (patch.diff:1721-1728). |
| T4 — T4 Contribution | FAIL | The tests do **not** constrain the production write. `domain_placement.rs` drives the test-only `write_new_object_placed` (patch.diff:1094); `failure_domain_registration.rs` drives the test-only `discover_topology` (patch.diff:2065). Neither has a production caller (only references at patch.diff:893/1094 and 2017/2065). A regression leaving production on identity placement is **not caught** — indeed that is the shipped state, and the suite is green. This is the iteration-1 "selector co-located, not shared by the write fan-out" failure the brief said not to repeat (`brief.md:148-153`). |
| T5 — T5 Judgment | NEEDS-HUMAN | Oracle is "reviewer + human sign-off" (`check-gates.json:96-102`). Telemetry-seam fidelity looks genuine — real `opentelemetry-prometheus` + `opentelemetry-otlp` exporters wired, no backend hardcoded (patch.diff:1506-1534), not a bare in-memory stub — but the judgment is the human's. |
| V — Validation — fitness-to-purpose | NEEDS-HUMAN | Oracle is "human at sign-off" (`check-gates.json:104-112`). Additionally the brief routes the new dependencies (`tracing-opentelemetry`/`opentelemetry-otlp`/`prometheus`, patch.diff:446-456) to the ADR-0003 cargo-deny three-test audit + `deny.toml` allowlist — a project human-only item regardless of a green `deny` gate (`brief.md:106-109`). |

## §6 — items the human must clear

1. **C5 / T4 — leg (3) production wiring NOT delivered (blocking, re-derived).**
   The binding criterion (`brief.md:38-42`) and the iteration-2 carry-forward
   (`brief.md:148-153`) require the **production** write to record the selector's
   distinct-domain placement and to "actually share" the selector with the write
   fan-out. The patch instead adds a parallel `write_new_object_placed`
   (patch.diff:878-901) and `discover_topology` (patch.diff:1982) that **no
   production code calls** — `cluster_store_put` still calls `write_new_object`
   (target `cli.rs:460`, unchanged body; the diff only widens its trait bound to
   satisfy `write_fragments`' new `PlacementChunkStore`, patch.diff:1812-1813).
   So in production the committed placement is still identity `index % n`. This is
   the same defect that rejected iteration-1. **Human must decide** whether a
   library-only capability + test demonstration satisfies "Option B full
   production wiring," or whether the gateway write must be rewired to build a
   topology from discovery and call the placed write. (Marked FAIL because the
   gap is factually re-derivable; the disposition is the human's.)

2. **C2 / C4 — reproduction & red→green unverifiable / failing.** `C4-verify`
   gate is **fail** (`check-gates.json:42-48`). The behavioral leg is implemented
   as a new function, so no pre-fix behavioral red exists; the NET-NEW legs'
   required demonstrated negation-red (`brief.md:101-105,148`) is in the withheld
   `build-notes.md`. Human must confirm the negation-red (fence + distinctness)
   was actually demonstrated and why `C4-verify` is red.

3. **T5 — telemetry-seam fidelity (judgment).** Confirm the dual Prometheus+OTLP
   export (patch.diff:1506-1534) is the intended seam and that the in-process
   read-back assertion (patch.diff:1565-1572) is acceptable in lieu of a live
   scrape/collector run (supplementary off-Check per `brief.md:104-105`).

4. **V — validation fitness-to-purpose + new-dependency audit.** Human-only:
   ADR-0003 cargo-deny three-test audit + `deny.toml` allowlist for the new
   OpenTelemetry/Prometheus dependencies (`brief.md:106-109`), and overall
   fitness against proposal 0005.

## Note on scope of grounding

Target source confirms the pre-patch base still routes the production write
through `write_new_object` (identity placement) at `crates/server/src/cli.rs:460`,
and the patch leaves that body untouched — the basis for the C5/T4 FAILs above.
