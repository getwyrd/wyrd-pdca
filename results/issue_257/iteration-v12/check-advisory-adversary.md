# Adversarial review — issue_257 (iteration 12), advisory only

Refutation attempts against the patch, the red→green evidence, and the reviewer's posture.
Grounded on `$PDCA_TARGET` (`/home/eddie/wyrd/wyrd.pdca-wt`). I never gate.

## The fix — a concrete input that the "binding" correctness evidence cannot catch

- **NEEDS-HUMAN — The off-Check Tier-1 consistency leg — now the *sole* binding correctness
  evidence for the redb→TiKV swap under the ratified Option-B posture — has no teeth for the
  exact defect the slice exists to guard.** `run()` in
  `crates/metadata-tikv/tests/tier1_metadata_consistency.rs:101-236` drives **strictly
  sequential, single-writer** commits (create → partition → rename → heal → read/version →
  delete); there is **no second concurrent writer** contending the version key across the
  partition window (no `spawn`/`join` anywhere in the file). The production defect this whole
  M4.6 thesis targets — a missing/mis-ordered commit-point re-check — lives in
  `crates/metadata-tikv/src/lib.rs:555-573`, the `get_for_update` precondition loop, which only
  produces a `Conflict` **under concurrent write-write contention**. With a single sequential
  writer the value never changes between the snapshot read and `txn.commit()`
  (`lib.rs:597`), so **deleting or weakening the re-check would leave every assertion in this
  leg green.** Compounding this, the leg isolates a **minority** voter (`WYRD_TIER1_ISOLATED=1`
  of 3, `faults.rs` runner), and the majority side of a linearizable Raft group stays writable
  regardless — this is *precisely* the "minority partition against a linearizable store never
  changes that outcome" hollow flip the brief forbids (`brief.md:139`, and the iter-1..4
  rejection). Concrete failing case: a hypothetical `metadata-tikv/src` regression that drops
  the `get_for_update` re-check passes `tier1_metadata_consistency` and `tier2_metadata_io`
  identically to correct code. The iter-8 acceptance test ("perturbing the get_for_update
  re-check must flip an at-Check artifact") is not met even *off-Check*.

- **NEEDS-HUMAN — At the unprivileged Check gate the entire live-scenario code path is neither
  compiled nor executed, so no at-Check artifact flips on a regression in it.** The feature-gated
  type-check `cargo check -p wyrd-metadata-tikv --features tikv --tests` is emitted **only when
  `WYRD_TIKV_TOOLCHAIN` is set** (`xtask/src/main.rs:846-847,887`). On the default at-Check run
  the `#[cfg(feature = "tikv")]` bodies — `SymmetricPartition`, its `Drop` heal, the PD-side
  heartbeat oracle, and the `partition_took_effect`/`heal_is_complete`/`consistency_passes`
  wiring — are **not built** (confirmed: `cargo test -p wyrd-metadata-tikv` with default features
  compiles only the skeleton and passes). The tier1 docstring's claim that the code is
  "type-checked … in the whole-tree gate" (`tier1_metadata_consistency.rs:280-286`) is true only
  on a privileged box. Net effect combined with the finding above: **there is currently zero
  compiled-or-executed evidence, at Check, for the whole live-scenario code path**, and the
  privileged run that would exercise it is itself unconfirmed (an owed NEEDS-HUMAN). A type error
  or logic regression in `SymmetricPartition` flips no Check artifact.

## The evidence — the gating verdict does not reproduce

- **NEEDS-HUMAN — The gating C4-ci failure does not reproduce at `$PDCA_TARGET`.**
  `check-gates.json` records `C4-ci` = **fail**, `cargo test --workspace --exclude wyrd-dst`
  exit 101 (the only gating row). On the target I ran the full gate steps: `cargo fmt --all --
  --check` (exit 0), `cargo clippy --workspace --exclude wyrd-dst --all-targets` (clean), and
  `cargo test --workspace --exclude wyrd-dst` (**exit 0, all green**, twice). So the blocking
  signal is either environmental/state-dependent or was produced on a differently-configured box
  (e.g. `WYRD_TIKV_TOOLCHAIN` set, which would additionally compile the pre-1.0 `tikv-client`
  tree and can fail on missing protoc/grpcio — a *different* failure than the recorded test-step
  101). A human should establish the real cause before trusting the gate in *either* direction:
  a non-reproducing red is as untrustworthy as a rationalized green.

## The verdict — where the posture may be over-credited

- The `C4-verify` "red→green" row is marked **pass** (advisory), but note the at-Check flip it
  demonstrates can only be one of the **pure arithmetic oracles** (`testkit` quorum/heartbeat
  functions) or the **redb coverage seed** — none of which is behavioural against
  `TikvMetadataStore::commit`. That is the *declared* Option-B posture, but combined with the two
  findings above it means the patch ships **no executed behavioural evidence, at Check or in a
  confirmed off-Check run, that a real commit-point regression is caught.** The reviewer's
  acceptance of Option-B as a *posture* is reasonable; treating the tier-1 leg as the correctness
  bar it "defers to" is not, until the concurrency/teeth gap above is closed or the human
  explicitly accepts that the ADR-0015 commit-point contract remains **unproven by any artifact
  in this slice**.

## Attempted refutations that did NOT stick (reported for signal)

- Attacked the fault-effect oracle for a false-green: `fault_materialized =
  partition_materialized(3,1) && partition_took_effect(before, during)`
  (`tier1_metadata_consistency.rs:145-160`). A broken/parse-failing oracle yields
  `connected_before = false` → `partition_took_effect = false` → leg **fails**, not passes. This
  path **fails safe**; could not turn it into a false-green.
- Attacked `parse_store_last_heartbeat` / `heartbeat_is_fresh` (`testkit/src/lib.rs:1089-1125`)
  for a wrong-store or threshold false-positive; for the fixed `deploy/tikv-multi-replica`
  topology (distinct loopback IPs, unique `127.0.0.2` target) the substring match and strict-`<`
  age threshold are correct, and the unit tests use hand-computed expectations (non-tautological).
  Could not refute these as arithmetic.
- Attacked `ci_type_checks_feature_gated_metadata_scenario` (`xtask/src/main.rs:1130-1168`) as an
  iter-10-style tautology; it now genuinely drives `run_ci_steps` with a recording executor and
  would flip if the wiring loop or the toolchain gate were removed. Could not refute (the *gap*
  is that the gated step is off by default — see finding 2 — not that the test is hollow).
