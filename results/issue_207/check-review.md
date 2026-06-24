# Check review — issue 207 / scrub-corruption-enqueue-and-continue

Advisory, artifact-only. Grounded against the target source at
`$PDCA_TARGET = /home/eddie/wyrd/wyrd` (read-only — the pre-fix branch the patch
applies to), plus `patch.diff`, `brief.md`, `check-gates.json`. `build-notes.md`
withheld by design; verdicts below were re-derived independently.

## Verdict table

| Item | Verdict | Basis |
| --- | --- | --- |
| C1 — C1 Spec | PASS | brief.md has a single load-bearing success criterion with binding legs (a)+(b)+(c) + corrupt-vs-transient distinguishability (brief.md:32-43), plus a named invariant (brief.md:44-61) and a plan-exit self-test that forbids a one-module fix. Oracle is unambiguous and testable. |
| C2 — C2 Reproduction (red pre-fix) | PASS | Defect is real on target: `scrub.rs:70` `let Some(bytes) = store.get_fragment(frag).await? else { continue }` — the `?` propagates a verifying backend's `Err` out of `reconcile` before the corruption branch (`scrub.rs:79-85`) runs. New tests reproduce it red pre-fix (`crates/custodian/tests/scrub.rs` FsChunkStore test `.unwrap()` panics on the aborting `Err`; gRPC `get_of_a_rotten_fragment…` fails since pre-fix `server.rs:73` maps to `Status::internal`→`Rpc`, `is_integrity_fault` false). C4-verify gate = pass corroborates red→green. |
| C3 — C3 Change | PASS | patch.diff is a coherent structural change across exactly the seams the brief's plan-exit self-test names: new seam type `IntegrityFault`/`is_integrity_fault` (traits/src/lib.rs, vs. target which jumps from BoxError lib.rs:58 straight to Health lib.rs:63), FsChunkStore put+get map verify failure to it (vs. target lib.rs:74,95 bare `?`), gRPC server emits `DataLoss`/`InvalidArgument` (vs. target server.rs:59,73 uniform `Status::internal`), client reconstructs `IntegrityFault` from `DataLoss`, scrub `match` arm classifies integrity-vs-transient and continues. |
| C4 — C4 Verification (red→green) | PASS | check-gates.json: C4-ci (`xtask ci` fmt/clippy/build/test/deny/conformance) = pass and C4-verify (per-fix red→green) = pass, overall = pass. Mechanical green only — confirms it builds and the per-fix test flips, not semantic adequacy (covered by C5/T5). |
| C5 — C5 Causal adequacy | PASS | Root cause (brief.md:9-31) is the aborting `?` + the absence of a corrupt-vs-transient signal across the store→gRPC→scrub path; not contested. The diff fixes the named cause at each named seam: scrub's Err arm now repairs-and-continues on `is_integrity_fault` and propagates otherwise (`scrub.rs` match), and the distinction is carried end-to-end (FsChunkStore→`DataLoss`→client `IntegrityFault`). Invariant "one rotten fragment never stalls the pass" is restored — the integrity arm does not early-return; read-path consumer (read.rs) is correctly left to #198 (brief.md:84). Final human sign-off is the gate's design (oracle: reviewer + human). |
| T1 — T1 Structure | PASS | Tests land in the brief's named files: `crates/custodian/tests/scrub.rs` (scrub legs) and `crates/chunkstore-grpc/tests/round_trip.rs` (gRPC classification legs) — matching brief.md:98-102. Helpers reused from the existing harness (`frag`, `valid_fragment`, `corrupt_fragment`, `commit_reference`, `elect`, `reconcile_step`, `connected`) all exist on target (scrub.rs:120-175; round_trip.rs:19-60). |
| T2 — T2 Shape | PASS | Assertions match the binding legs: FsChunkStore test asserts `Reconciled::Changed`, both rotten chunks enqueued (leg a + continuation/leg c — order-independent proof), `scrub_corruption_detected`+`scrub_coverage` exposed (leg b), corrupt never served / intact still served. gRPC tests assert `DataLoss`⇒`is_integrity_fault` & NOT a `TransportError` (distinguishability), and malformed PUT ⇒ `InvalidArgument`. Transient test asserts propagate + no enqueue (the other half of the distinction). |
| T3 — T3 Runtime | PASS | C4-ci gate (includes `test`) = pass; new tests compile against the added deps (`wyrd-chunkstore-fs`, `tempfile` in custodian/Cargo.toml; Cargo.lock updated) and run. |
| T4 — T4 Contribution | PASS | `fschunkstore_corruption_is_enqueued_and_the_pass_continues` is genuinely red pre-fix (aborting `Err`→panic) and pins continuation via "both rotten enqueued", which a single-module non-`?` patch would not satisfy. gRPC tests are red pre-fix (uniform `internal`). Note: `scrub_propagates_a_transient_get_fault_without_enqueuing` is a **guard** test (green both pre- and post-fix — target `scrub.rs:70` `?` already propagates a transient `Err`), pinning the no-over-classification half; valid contribution, just not itself a red→green. |
| T5 — T5 Judgment | PASS | Tests are non-tautological, exercise both arms of the corrupt-vs-transient decision and both seams (local FsChunkStore + gRPC wire), and assert observable telemetry rather than internal state. No mock-only theater — the FsChunkStore test rots real on-disk bytes behind the store's back. Final human sign-off is the gate's design (oracle: reviewer + human). |
| V — Validation — fitness-to-purpose | NEEDS-HUMAN | Always-human (gate oracle: human at sign-off). Whether this end-to-end restoration is the *right* fix for the production durability concern — and that `DataLoss`/`InvalidArgument`/`IntegrityFault` is the intended plumbing among the brief's illustrative options (brief.md:40-43) — is a human fitness call, not re-derivable from artifacts. |

## §6 — items the human must clear

1. **V — Validation fitness-to-purpose (NEEDS-HUMAN).** Confirm the chosen plumbing
   (`TransportError`-bypassing `IntegrityFault` seam type + gRPC `Code::DataLoss` on
   read + `Code::InvalidArgument` on malformed PUT) is the intended scheme — the brief
   marks the exact mechanism ILLUSTRATIVE (brief.md:40-43), so the equivalence of this
   choice to the durability contract is a human judgment.

## Reviewer notes (non-gating)

- All verdicts re-derived against target source; pre-fix abort mechanism confirmed at
  `crates/custodian/src/scrub.rs:70`, uniform error mapping at
  `crates/chunkstore-grpc/src/server.rs:59,73`, and absence of `IntegrityFault` on
  target `crates/traits/src/lib.rs` (BoxError:58 → Health:63).
- `is_integrity_fault(e.as_ref())` relies on trait-object coercion from
  `&(dyn Error+Send+Sync)` to `&(dyn Error)`; the C4-ci build gate (pass) covers that it
  compiles on the toolchain in use — not independently re-checked here.
- Scope is well-bounded and matches the brief (read.rs untouched per #198; #203/#204
  merge-chain respected), so no "ambiguous scope" NEEDS-HUMAN was raised.
