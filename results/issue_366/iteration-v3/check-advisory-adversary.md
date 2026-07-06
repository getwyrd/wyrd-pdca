# Adversarial review — issue 366 (obs-floor / deployable custodian)

Lens: refute the red→green evidence and the reviewer's verdict. Grounded on the target
source at `$PDCA_TARGET` (patch applied to the working tree). C4 is already a **gating
FAIL**; the findings below explain *why*, and attack the fix's fitness where the gates are
silent.

## The fix is broken for its own binding purpose

- **NEEDS-HUMAN — The deployable `wyrd custodian` cannot reach the M4 production metadata
  backend.** `cmd_custodian` unconditionally calls `open_local_meta_redb(data_dir)`
  (`crates/server/src/cli.rs:516`) — it never calls `resolve_backend` and exposes no
  `--metadata-backend` flag (contrast `cmd_put`/`cmd_get`, which do:
  `cli.rs:233`/`cli.rs:320`, with a real TiKV arm at `cli.rs:257`). The brief's **BINDING**
  success is "the day-one durability signal through the wired, runnable custodian role" as
  an **M4 deployment prerequisite**, and M4 production runs the **TiKV** metadata backend
  (redb is dev/eval only, ADR-0014). Against a real M4 cluster, `wyrd custodian` opens an
  empty local redb file, finds no chunks and no repair obligations, and the under-replicated
  gauge **never rises** — the signal is undemonstrable on the very deployment it gates. The
  at-Check test hides this by handing the role a hand-built in-memory `MemMeta`
  (`custodian_day_one.rs:352`) instead of driving `cmd_custodian`'s backend open.

- **Concrete hang: the "survive the killed D-server" path stalls on a silent peer.**
  `cmd_custodian` dials each endpoint with `GrpcChunkStore::connect(...)`
  (`cli.rs:558`), which builds a channel with **no request/connect timeout**.
  `live_reconstruction_view` then probes `store.health().await` each pass
  (`crates/server/src/custodian.rs:606`). The codebase's own
  `GrpcChunkStore::connect_with_timeout` (`crates/chunkstore-grpc/src/client.rs:79`) exists
  precisely because "an RPC to a server that has stopped responding mid-call — a
  `docker pause`d node or an injected network partition that leaves the connection
  established but the peer silent — would hang the future indefinitely" (client.rs:70-78).
  The day-one fault is *kill a D-server*; a process-kill yields connection-refused (→ `Err`
  → dropped, fine), but a **partition / pause** leaves `health()` hanging forever, stalling
  the whole reconcile loop — the role neither survives nor emits. The test's `DeadDServer`
  returns `Err` from `health()` **instantly** (`custodian_day_one.rs:329-…`), so the hang
  is never exercised.

- **NEEDS-HUMAN — Fabricated failure domains can defeat the durability invariant the
  custodian exists to uphold.** `cmd_custodian` keys each D-server positionally,
  `id: i as DServerId` with a synthetic `failure_domain: format!("domain-{i}")`
  (`cli.rs:573-574`), ignoring each D-server's real registered failure domain. Two servers
  in the **same real** rack/domain are therefore modelled as distinct domains, so a rebuilt
  fragment can be re-placed onto a server in the same real failure domain as a survivor —
  the exact domain-collapse the reconstruction placement guard is meant to prevent. This is
  the iteration-2 adversary's DServerId concern; the builder *documented* it (cli.rs:570-577
  comment) but did **not** fix it. The at-Check test never exercises this: it hand-builds
  `ConfiguredDServer` with correct distinct domains A/B/C/D (`custodian_day_one.rs:…`,
  `four_domains`), bypassing the binary's synthetic assignment.

- **The "single-active" leadership advertised by the binary is hollow in production.**
  `cmd_custodian` constructs `MemCoordination::new()` (`cli.rs:524`), which always grants
  leadership to the lone process; the eprintln advertises "single-active". The builder
  re-scoped the iteration-2 fix to "host-local single-active via the redb store lock" — but
  that lock only exists for the redb file, and (per finding 1) the role can't open a shared
  store at all. For any multi-host deployment two custodians are unfenced. The rebuild did
  **not** implement cross-process election as iteration-2 §6.3 required; it renamed the gap.

## The evidence is weaker than the brief claims

- **The at-Check test does not exercise the production loop it names.** The module/test
  comments claim the "exact production path `cli::cmd_custodian → run_reconstruction_until →
  live_reconstruction_view + reconcile_pass`" (`custodian_day_one.rs:12,367`), but the test
  body calls only `live_reconstruction_view` + `reconcile_pass` directly
  (`custodian_day_one.rs:395,417,433,500,521`); it never calls `run_reconstruction_until`,
  `cmd_custodian`, or `open_local_meta_redb`. The loop's advertised survival behaviour —
  `ReconcileError::Store` → log-and-continue, `ReconcileError::Fenced` → stop
  (`custodian.rs:766-781`) — is **untested**. "Survive the kill" is demonstrated only by
  `live_reconstruction_view` dropping a health-erroring stub plus one manual pass.

- **NEEDS-HUMAN — the per-fix red→green was never mechanically established, and the "red"
  is a compile error, not the defect.** `check-gates.json` C4-verify failed with
  `pathspec 'crates/telemetry/src/lib.rs' did not match any file(s) known to git`. Both
  at-Check tests import `wyrd_server::custodian` and `wyrd_telemetry`
  (`custodian_day_one.rs:51,50`) — entirely new module + crate. Reverting the patch to
  reach "red" deletes the imported symbols, so pre-fix the test **does not compile**; a
  compile-error "red" is not a red→green proof that the test catches the defect. Separately,
  the modified DST property (`crates/dst/tests/custodian.rs`, monotonic→gauge) runs **only**
  in `wyrd-dst`, which the C4 gate **excludes** (`--exclude wyrd-dst`), so the gauge-semantics
  change's principal property test is outside the gate.

- **NEEDS-HUMAN — the gating C4 failure is a flaky, tracing-callsite-race read-back test.**
  `cargo test --workspace --exclude wyrd-dst` fails at
  `crates/custodian/tests/rebalance.rs:884`
  (`emits_per_failure_domain_utilization_on_the_durability_seam`), reproduced
  non-deterministically (2 of 3 parallel standalone runs; passes in isolation). Cause:
  `tracing` caches per-callsite *interest* in process-global state, so under parallel
  scheduling a sibling no-subscriber test poisons the `capacity_domain_utilization` callsite
  before this test installs its metrics subscriber. The patch does not touch rebalance.rs
  (so likely **pre-existing** — human to confirm), **but** it is the reason C4 (gating) is
  red *on this patch*, and it is the **same** process-global read-back mechanism the patch's
  own evidence depends on — the builder even isolates `custodian_day_one.rs` into its own
  binary for exactly this reason (test header, lines 43-45). The keystone's whole test
  strategy rides on a mechanism the suite already demonstrates is racy.

## Attempted and could not refute

- The `reconstruction.rs` gauge change itself (`emit_under_replicated` counting
  `Repairable + Unrepairable + Malformed`, monotonic→gauge, `reconstruction.rs:160,179,187,
  202,231`) is sound and genuinely covered: `under_replicated_gauge_counts_a_loss_beyond_
  tolerance` asserts `1` where pre-fix `plans.len()` was `0` for the below-`k` case — a real
  behavioural distinction. I ran the at-Check binary 12× under parallelism; it was stable
  (0 failures). I could not make the gauge read a wrong value for the covered cases.
