# Add etcd coordination backend for multi-node clusters

## Summary
**User impact:** On a real multi-node deployment, the cluster could not coordinate
across machines. The coordination layer — peer discovery, custodian leader election,
distributed locks, and fencing of stale holders — had only a single, process-local
in-memory implementation, so its state was visible only inside one process. Nodes on
different machines could not discover each other, two nodes could each believe they
were the custodian leader (split-brain), and a stale lock holder could not be fenced
out. Multi-node clusters had to fall back to static endpoint lists and had no live L5
discovery.

This adds a networked, **etcd-backed** implementation of the existing coordination
contract, selectable by server configuration, so a deployed cluster discovers peers,
elects one leader, and fences stale holders across machines — with no changes required
of any caller.

## What to look at
- `crates/coordination-etcd/src/store.rs` — the new backend: leased registration +
  discovery, campaign-based election with a rising fencing token, transactional fenced
  locks, and revisioned config over etcd.
- `crates/coordination-conformance/src/lib.rs` — the single contract suite that both
  the in-memory and etcd backends run, so they are held to the same semantics.
- `crates/server/src/cli.rs` (`cmd_d_server`, backend selection ~line 485) — the one
  place that changed for consumers: the backend is chosen by config. The coordination
  trait and its callers are untouched.
- **To exercise:** `cargo xtask etcd-conformance` brings up a single-node etcd, builds
  the crate with the `etcd` feature, runs the shared suite plus the cross-instance
  properties (single leader, mutual exclusion, discovery) against the live cluster, and
  tears it down. The default `cargo xtask ci` exercises the same store deterministically
  under a simulator.

## Root cause
The coordination trait was pinned by exactly one implementation — an in-memory,
single-process backend — so every cross-process guarantee it promised was untested and
unimplemented for a networked deployment. Cross-machine discovery, single-leader
election, and cross-node fencing had no code path at all.

## Fix
A second implementation over etcd provides every trait method with networked semantics:
leases that expire, a `discover` over leased registrations, an election whose token
rises across terms, mutually exclusive locks that fence, and config with a monotonic
revision. Selection moves to server configuration (`crates/server/src/cli.rs`), leaving
the trait and all callers byte-for-byte unchanged; the in-memory backend stays the
default for local and dev use.

## Verification
- **Claim:** one shared contract suite passes on **both** backends (identical semantics,
  no forked contract).
  - **Checked:** `crates/coordination-conformance/src/lib.rs` — the `contract_*` helpers
    are driven by both `crates/coordination-mem` and `crates/coordination-etcd`.
  - **Test:** `cargo test -p wyrd-coordination-mem` (in-memory) and
    `cargo xtask etcd-conformance` (real etcd) both run these clauses green;
    `crates/coordination-conformance/tests/demonstrated_red.rs` supplies a violating
    stub per clause, so each clause is shown to fail against a broken backend
    (non-vacuous).

- **Claim:** a single leader across instances — no split-brain — on a real cluster.
  - **Checked:** `crates/coordination-etcd/tests/conformance.rs:54` (trait in scope) and
    its cross-instance single-leader clause (~lines 104–115): while instance A holds the
    term, a second instance's campaign must stay pending, not win.
  - **Test:** runs green against live etcd via `cargo xtask etcd-conformance`; the same
    property runs deterministically under the simulator in
    `crates/dst/tests/coordination.rs`, with a two-instance in-memory driver as the
    failing (split-brain) counter-case.

- **Claim:** locks are mutually exclusive on real etcd (a defect the simulator hid).
  - **Checked:** `crates/coordination-etcd/src/store.rs` `lock` (~line 354): the guard
    tests the **held** state (`LOCK_HELD`, store.rs:61) rather than absence, which reads
    identically on real etcd and the simulator. Phrasing it as "value absent" — the
    prior form — never fired on real etcd, so a free lock was wrongly refused.
  - **Test:** `cargo xtask etcd-conformance` was red on this clause before the fix and is
    green after.

- **Claim:** election and registration keyspaces are prefix-isolated (no cross-key
  bleed).
  - **Checked:** `crates/coordination-etcd/src/keyspace.rs` `election_name` encodes the
    logical key into one delimiter-free segment.
  - **Test:** `keyspace::tests::election_prefix_isolates_hierarchical_keys` is red on the
    raw key and green after encoding; it runs in `cargo xtask ci` (no etcd needed).

- **Claim:** no caller edits — selection is a composition swap.
  - **Checked:** the coordination trait, core, and custodian are absent from the diff;
    the only consumer change is backend selection in `crates/server/src/cli.rs`
    (`cmd_d_server`), verified by `crates/server/tests/backend_selection.rs`.

**Known limitations (follow-up):** the etcd client is behind an off-by-default feature,
and the real-etcd conformance runs via `cargo xtask etcd-conformance` (needs Docker +
`protoc`), not the default CI. The backend currently connects without TLS/auth; its
dependency review and transport-security posture are tracked before production exposure.

Fixes #365
