# Build notes — issue 490 (iteration 3): renew-pending-lease-resurrection

## What changed vs iteration 2

The iteration-2 **production** fix was sound — Check passed C1/C3/C5/T1/T2/T3/T5 and the
adversary could not refute the wiring, the atomicity, or the red→green proof. It was parked
only on two **implementation-level** test-coverage gaps the adversary landed, plus two
NEEDS-HUMAN environment items (full `cargo xtask ci` couldn't bind loopback in the sandbox;
remote closed/rejected work couldn't be settled mechanically). This iteration reuses the
iteration-2 production diff **unchanged** and closes the two test gaps. No production line
moved — verified: `patch.diff`'s `crates/core/src/{metadata,write}.rs` and
`crates/server/src/lib.rs` hunks are byte-identical to `iteration-v2/patch.diff`.

### Gap 1 — the `<=` expiry boundary was not pinned (adversary refutation #1)
Repro (A)/(B)/(C) all acted a **full TTL** past the boundary (commit/renew at `2·TTL` vs
expiry `TTL`), so a regression from `<=` back to `<` — iteration 1's exact rejected bug,
obligation (b)'s headline delta — stayed green through the whole gate.

Closed with three exact-deadline tests, one per seam, each acting at **exactly
`now == lease_expiry_millis == TTL`**:
- `crates/core/tests/stream_lease_lapse.rs:324` `commit_refuses_at_exact_lease_deadline`
  — the **overwrite** seam (`live_lease_guards`, `metadata.rs:694`). This lives in the
  **added** file, so it is part of the primary red→green: red on base (unconditional commit →
  `Committed`), green with the fix.
- `crates/core/tests/stream_lease_renewal.rs:125` `renewal_at_exact_deadline_aborts_the_upload`
  — the **renewal** seam (`renew_pending`, `metadata.rs:658`).
- `crates/core/tests/stream_lease_renewal.rs:227` `create_commit_refuses_at_exact_lease_deadline`
  — the **create** seam (`create_leased` via `live_lease_guards`).

Mutation-proven: flipping both `<=` to `<` in `crates/core/src/metadata.rs:658,694` turns all
three red (the exact regression the adversary demonstrated), and restores green when reverted.

### Gap 2 — the create seam's refusal was untested (adversary refutation #2)
Every lapse scenario in the added file is overwrite-shaped (the compile-against-base
constraint forbids the changed `commit_create` signature), so `create_leased`'s guard was
only ever hit on its healthy pass — a mutant that dropped the guard from `create_leased`
alone survived. Closed with two create-seam tests in the (red-leg-reverted)
`stream_lease_renewal.rs`, which may use the changed signature:
- `create_commit_refuses_when_lease_swept` (`:170`) — absent lease (the `None` branch).
- `create_commit_refuses_at_exact_lease_deadline` (`:227`) — present-but-expired at the boundary.

Both drive production `write::commit_create` → `metadata::create_leased` and assert the create
refuses **and publishes no dirent** (`read::resolve == None`).

## Why the new create/renewal tests are NOT in the added file
C4-verify's red leg reverts every modified file and keeps only the added `stream_lease_lapse.rs`,
which must compile against the **base** tree — where `commit_create` has no `now` param and
`renew_pending` no `now` param. A create/renewal-seam test therefore cannot live in the added
file without breaking the compile-against-base honesty constraint. The adversary anticipated
this exactly and recommended placing them in the already-modified `stream_lease_renewal.rs`
(reverted on the red leg anyway) as post-fix regression guards. That is what I did. The
overwrite boundary test *can* live in the added file (stable `commit_overwrite` signature), so
the one seam that carries the primary red→green does.

## Refute-my-own-test (recorded, per Do discipline)
- **(a) Genuine red?** YES. C4-verify's disposable red leg (reverts all 11 modified files, keeps
  only `stream_lease_lapse.rs`) ran and the added file failed 4/4 defect assertions on base —
  including the new `commit_refuses_at_exact_lease_deadline` (`left: Committed`) — while the
  healthy control passed; gate printed "PASS — red without the fix, green with it." The two
  renewal-file guards are additionally mutation-proven red (flip `<=`→`<`) rather than
  fix-reverted, since the red leg does not run them.
- **(b) Production path?** YES. Every test drives real `write::stream_write_data`,
  `write::commit_overwrite`, `write::commit_create`, `write::write_new_object`,
  `write::sweep_expired_leases`, `read::{resolve,read_inode,read_path}` over
  `RedbMetadataStore::in_memory()` + `FsChunkStore` (tempdir). No mock, copy, or
  re-implementation.
- **(c) Fixture includes the fault?** YES. The swept scenarios assert the sweep actually
  reclaimed the in-flight leases (`reclaimed` non-empty / `== vec![201]`); the present-but-
  expired scenarios assert the leases are still *present* at commit (`scan("pending:").len()
  == plan.chunks.len()`) so the refusal is purely the expiry check; the new exact-deadline
  tests act at `now == expiry`, the precise instant the reaper contract makes load-bearing.

## Tests run (worktree `$PDCA_WORKTREE`, via the project's C4-verify gate + cargo)
- `./engine/scripts/run-verify.sh` (C4-verify, red→green on the added file): **PASS**.
- `cargo test -p wyrd-core --test stream_lease_lapse --test stream_lease_renewal`: 5/5 + 4/4
  green.
- Mutation check `<=`→`<` on `metadata.rs:658,694`: the three exact-deadline tests go red;
  reverting restores green.
- `cargo fmt --check -p wyrd-core`: clean. `cargo clippy -p wyrd-core --tests`: 0 findings.

## NEEDS-HUMAN carried from iteration 2 (unchanged environment limits, not defects)
The sandbox cannot bind loopback, so the full `cargo xtask ci` (server/dst gRPC legs, real
etcd/tikv fidelity, on-disk conformance) still cannot run here — it stopped at
`list_delete_over_grpc` with `PermissionDenied`. This is the same environment limit iteration 2
hit; the focused red→green and the wyrd-core suite are independently green. A human must rerun
full `cargo xtask ci` on a loopback-capable host (C4-ci gate), and confirm no competing
closed/rejected remote fix on the affected paths (T4). Only core **test** files changed this
iteration, so the server/dst callsite updates iteration 2 already verified are untouched.

## Out of scope (per brief, untouched)
`metadata::commit_chunk_map` (reconstruction/backfill, no pending entries); GC/sweep behaviour;
the residual GC-mid-pass race (getwyrd/wyrd#557); gateway error taxonomy beyond the existing
`Conflict → GatewayError::Conflict` mapping; #554 GC deployment wiring.
