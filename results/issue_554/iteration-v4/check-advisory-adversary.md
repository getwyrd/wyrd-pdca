# check-advisory-adversary.md — issue 554 / deployed-custodian-runs-gc (iteration 4)

Adversarial pass. I attempted to refute the red→green evidence, the gate results, and the
fix itself, re-running everything at `$PDCA_TARGET`. Verdicts below; experiments were run on
throwaway copies, never the target.

## Refutations / findings

- NEEDS-HUMAN — **The gating C4 red is, on this host, an environment fault — but it is the
  third consecutive cycle the gate is red without naming a failing test.** I re-ran the exact
  gate command `cargo test --workspace --exclude wyrd-dst` at `$PDCA_TARGET`: **exit 0, 129
  test binaries green**, including the new `crates/server/tests/custodian_gc.rs` (6/6). The
  only failure I could produce was
  `crates/server/tests/cli_roundtrip.rs:43` (`put_then_get_round_trips_across_separate_invocations`)
  panicking with `wyrd: Disk quota exceeded (os error 122)` when `tempfile::tempdir()` lands
  on this sandbox's quota-limited `/tmp`; with `TMPDIR` on a writable filesystem it passes.
  That is the same environment-fault class as iteration 2's `list_delete_over_grpc`
  `PermissionDenied` (loopback). Per issue #236 this is **not** scored as a refutation of the
  fix — but the deterministic gate still blocks, `check-gates.json` C4's `path_line` again
  reports only "exit status: 101" without naming the failing test (the diagnostic iteration 3
  explicitly demanded), and I cannot inspect the gate host. A human must either fix the gate
  host (disk quota / tmpdir / loopback) and re-run C4, or adjudicate the discrepancy; my
  candidate culprit is `cli_roundtrip` under a tempdir-write restriction. Verdict on C4 is
  provisional (toolchain/environment unavailable to me).

- NEEDS-HUMAN [impl] — **Duplicate `--endpoints` now reaches a deleting GC sweep with no
  validation — the exact live-data-loss case the restore path refuses.** The uniqueness
  checks live only inside the `--reconcile-after-restore` block
  (`crates/server/src/cli.rs:940-967`); the run-loop path (`cli.rs:1040-1073`) performs none,
  and `require_aligned_topology` checks lengths only. The restore-path comment itself names
  the hazard: a box listed twice under two ids "answers to both … a later GC sweep through
  the duplicate reaches them as B and deletes them" (`cli.rs:929-939`). Before this patch
  that "later GC sweep" never ran in deployment; this patch is what makes it run every
  interval. Concrete failing case: operator fat-fingers `--endpoints A,A --ids 1,2
  --failure-domains x,y`; the whole-fleet gate passes (`fleet.len() == operator_fleet_size`
  counts the duplicate on both sides, `crates/server/src/custodian.rs:582`); a repair
  displaces fragment `f` from id 1 to id 2 — same physical box, same `FragmentId` key — and
  commits an orphan record for `(1, f)` (`crates/custodian/src/reconstruction.rs:580-585`);
  after grace, GC sweeps fleet entry `(1, boxA)`: `referenced.protects(1, f)` is false (the
  placement now references `(2, f)`), the orphan evidence exists, so `delete_fragment(f)`
  removes the one physical copy the **live committed placement** points at
  (`crates/custodian/src/gc.rs:124`, `gc.rs:134-136`, `gc.rs:151-153`) — silent loss of live
  data. Fix is mechanical: hoist the two uniqueness refusals (`cli.rs:940-967`) so they also
  guard the run-loop path, plus a test. (Delete-orphan and expired-pending inputs are safe
  under duplicates — evidence-gated; the repair-displacement route is the one that kills.)

## Attempted refutations that failed (the strong signal)

- **"The red is only a compile error, so the green may pass for the wrong reason" — refuted
  by experiment.** The mechanical C4-verify red leg on a reverted base is an E0061 compile
  error (the `operator_fleet_size` parameter), disclosed in the test module doc
  (`crates/server/tests/custodian_gc.rs:421-427`) exactly as the brief's Test-file note
  requires. I established assertion-level binding independently on a scratch copy: (a) with
  the GC block's gate forced to `false` (GC never runs — base behaviour, signature kept),
  **5 of 6 tests fail by assertion** (`custodian_gc.rs:837`, `:964`, `:1089` among them);
  (b) with only the gate reverted to iteration-2's `unreachable.is_empty()`, the
  startup-partial test fails by assertion at `custodian_gc.rs:791`. The tests bind to the
  production path (`run_reconstruction_until` → `reconcile_pass` → `gc::reconcile`, real
  write path via `write_new_object_placed`, physical byte checks on the stores) — no
  parallel re-implementation, no tautology.
- **Whole-fleet gate arithmetic:** could `fleet.len() == operator_fleet_size` pass on a
  partial fleet? No — `live_reconstruction_view` only ever drops entries from `configured`
  (`crates/server/src/custodian.rs:233-256`) and `connect_fleet` never produces more entries
  than `endpoints` (`custodian.rs:188-204`), so equality holds iff every operator endpoint is
  connected and reachable; `cmd_custodian` passes `endpoints.len()` (`cli.rs:1046-1051`).
  (Duplicate endpoints defeat the *intent* — see the [impl] finding — but not the arithmetic.)
- **Chunk-wide `pending:` retirement:** retired only inside a whole-fleet pass, and a
  mid-sweep store fault propagates before the cleanup batch commits (`gc.rs:151-170`), so
  partial sweeps preserve evidence; deletes are idempotent on retry.
- **Boundary conformance:** the inclusive `now == orphaned_at + grace` reclaim (`gc.rs:136`
  `>=`) is now pinned (`custodian_gc.rs:1052-1096`), closing the iteration-2 ±1 ms nit.
- **Doc-claim check:** the two-timescale grace derivation is now stated correctly — gateway
  30 s at `crates/server/src/lib.rs:49`, CLI 60 s at `cli.rs:23`, floor = the longer
  (`custodian.rs:114-138`) — the iteration-1 adversary-3 misstatement is fixed.

## Pre-declared human items (not new findings — keep visible)

- The deployed 60 s grace VALUE (floor, not a proven reader-safety bound,
  `custodian.rs:118-123`), the pause-under-outage trade-off (any absent configured server
  pauses ALL reclamation, `custodian.rs:177-183`), and the accepted document-and-ship
  lease-liveness hazard (`cli.rs:9-20` comment) remain maintainer calls per the iteration-2
  sign-off. I cannot verify from this bundle that the required **#490 sequencing note**
  ("do not run `wyrd custodian` against a shared write-taking backend before #490 merges")
  made it into the PR description — build-notes is withheld from this lens; confirm at
  sign-off.
