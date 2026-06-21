# Result — issue 155 / m2.8-local-distributed-cluster-static-endpoints

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: 
- Success criterion: a static-endpoints gateway client mode lands — e.g. `wyrd gateway
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope: (a) a static-endpoints gateway client mode composing

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C5 Causal adequacy: none — reviewer + human sign-off

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 Contribution: none — (no gate configured)
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

# Check review — issue 155 / m2.8-local-distributed-cluster-static-endpoints

Advisory, artifact-only, decorrelated from the builder. Inputs: `patch.diff`, `brief.md`,
`check-gates.json` (build-notes.md withheld by design). Citations re-derived against the
target source at `$PDCA_TARGET = /home/eddie/wyrd/wyrd` (read-only, `main`).

## Verdict table

| Item | Verdict | Basis |
| --- | --- | --- |
| C1 — C1 Spec | PASS | `brief.md:20-30` gives a concrete, load-bearing success criterion: a `--endpoints` gateway mode composing `Gateway` over `FanoutChunkStore<GrpcChunkStore>`, a fixed-port compose, a README walk-through, and a byte-identical in-process round-trip with `cargo xtask ci` green. Scope in/out is explicit (`brief.md:33-38`). |
| C2 — C2 Reproduction (red pre-fix) | N/A | Net-new feature, not a defect fix (`brief.md:46` "Net-new (this is the M2.8 step)"); `check-gates.json:15-22` configures no C2 gate. There is no pre-existing bug to drive red — the criterion is a green-state demonstration (compiles + round-trip), and the red state is simply that `connect_gateway`/`--endpoints` did not exist on `main` (confirmed: `cli.rs:79-85` usage and `cmd_get` at `cli.rs:180` have no endpoint path pre-patch). |
| C3 — C3 Change | PASS | The composition is sound and in-scope: `ClusterGateway = Gateway<RedbMetadataStore, GrpcFanout, MemCoordination>` (patch:106) matches `Gateway<M,C,Co>` generics (`lib.rs:42,56-61`); `GrpcFanout = FanoutChunkStore<GrpcChunkStore>` impls `ChunkStore` (`fanout.rs:57`); the new `use crate::{Gateway, GatewayError, …}` resolves (`lib.rs:162`, `pub enum GatewayError`) and the `downcast_ref::<GatewayError>() == Some(&GatewayError::Conflict)` guard (patch:250) is valid since `GatewayError` derives `PartialEq, Eq` (`lib.rs:161`). All three deliverables present: CLI mode (patch:194-286), root compose (patch:406-489), README (patch:9-63). See §6 for the cross-process inode caveat raised under C5. |
| C4 — C4 Verification (red→green) | PASS | `check-gates.json:33-40` records the gating `cargo xtask ci` (fmt/clippy/build/test/deny/conformance) as `pass`. The shipped test `gateway_put_get_byte_identical_across_grpc_cluster` drives the same `connect_gateway` composition the CLI uses across four real loopback gRPC D servers and asserts byte-identical round-trip (patch:351-390), satisfying the brief's C4-verify criterion. |
| C5 — C5 Causal adequacy | NEEDS-HUMAN | Two judgment calls. (1) **Topology** — client-side gateway vs gateway-daemon, and whether static-endpoints is the accepted M2.8 shape vs waiting for M3 discovery; the brief defers this to the human (`brief.md:49-53`). (2) **Cross-process inode divergence** (re-derived, not in any artifact note): the cluster path allocates inode ids from `Gateway`'s in-process `AtomicU64::new(1)` (`lib.rs:52,71,126`), but the local CLI path uses the *persisted* `meta:next_inode` counter (`cli.rs:321`). With a persistent redb under `--data-dir` and `metadata::create` doing `require_absent(inode_key(id))` (`metadata.rs:144`), a second new-key `wyrd put --endpoints` in a fresh process re-allocates inode 1 and fails as a spurious `Conflict` — only one distinct object per data-dir survives across invocations. Whether that limit is acceptable for an explicitly pre-production on-ramp is a human call. |
| T1 — T1 Structure | PASS | Test lives at the correct seam (`crates/server/tests/gateway_cluster.rs`, patch:291-295), exercises the real shipping `connect_gateway` composition (patch:368) over real tonic loopback transport (patch:322-340), not a stand-in. |
| T2 — T2 Shape | PASS | Meaningful assertions: byte-identical round-trip on a 40 KiB+ payload at an 8 KiB chunk size so each chunk's nine `rs(6,3)` fragments actually fan across four servers (patch:371-390), plus a miss-returns-`None` check through the same path (patch:392-400). Not a trivial/tautological test. |
| T3 — T3 Runtime | PASS | Re-derived as buildable/runnable: `tonic` and `tokio-stream` are regular deps and `tempfile` a dev-dep of `wyrd-server` (`crates/server/Cargo.toml:31-38`); `ChunkStoreServer`/`ChunkStoreService` are exported (`chunkstore-grpc/src/lib.rs:31,36`). Corroborated by the green `cargo xtask ci` gate (`check-gates.json:33-40`), which runs the test tier. |
| T4 — T4 Contribution | PASS | The test would catch regressions in the composition it targets — any break in the `Gateway`+fan-out+gRPC wiring fails it. Coverage gap (noted, not failing): it does not exercise the CLI surface (`parse_endpoints`, `cluster_put`/`cluster_get` `GatewayError` handling) nor the multi-object cross-process path that surfaces the C5 inode issue — a single-`put` test cannot. |
| T5 — T5 Judgment | PASS | The test is honest: real `FsChunkStore`, real tonic HTTP/2 framing, real fan-out across distinct ephemeral ports (patch:322-362) — no over-mocking and no assertion-weakening; it proves the networked path rather than a loopback fake of it. |
| V — Validation — fitness-to-purpose | NEEDS-HUMAN | Always human at sign-off (`check-gates.json:96-102`, oracle "human at sign-off"). Does this static-endpoints, single-gateway, metadata-local shape match what "run and exercise the distributed system on one machine" should mean for M2.8 (`brief.md:11-30`)? The brief's own sign-off note flags topology as the decision point (`brief.md:49-53`). |

## §6 — Items the human must clear

1. **(from C5 / V) Topology & milestone shape.** Accept the client-side gateway with
   static endpoints and locally-held metadata as the M2.8 shape, vs a gateway-daemon
   holding shared metadata, vs deferring to M3 dynamic discovery. The brief explicitly
   parks this for the human (`brief.md:49-53`).

2. **(from C5) Cross-process inode allocation in the cluster path.** The cluster mode
   allocates inode ids from `Gateway`'s in-process counter (`lib.rs:52,71,126`) while the
   local CLI path uses the persisted `meta:next_inode` counter (`cli.rs:321`). Because the
   cluster path pairs a persistent redb (`--data-dir`) with that in-process counter, the
   *second* distinct-key `wyrd put --endpoints` in a new process collides on inode 1 via
   `require_absent(inode_key(id))` (`metadata.rs:144`) and fails as a misleading "concurrent
   writer won" `Conflict`. The documented walk-through (one `put` → one `get` of the same
   key, `README` patch:35-44) and the single-`put` C4 test (patch:377-390) both stay inside
   the one-object window, so neither catches it. Decide whether single-object-per-`--data-dir`
   is acceptable for this pre-production on-ramp or whether the cluster path must route inode
   allocation through the persisted counter as the local path does.

3. **(from V) Fitness-to-purpose sign-off.** Confirm the three deliverables together
   actually let a human "run and exercise the distributed system on one machine"
   (`brief.md:11`) to the intended depth, given item 2.

## Notes (non-gating)

- README phrasing: "The loopback round-trip test uses the same four" (patch:26) reads most
  naturally as the *new* `gateway_cluster.rs` test (which uses four); the pre-existing
  `chunkstore-grpc/tests/round_trip.rs` uses a single server and `tier2_integration.rs`
  scales N≥2. Cosmetic, not a correctness issue.
- The root `docker-compose.yml` is correctly net-new and distinct from the CI fixture
  (`crates/chunkstore-grpc/tests/docker-compose.yml`, confirmed present on `main` and using
  ephemeral ports / `--scale`); the new file uses fixed published ports 50051-50054 with a
  volume per server (patch:438-488), matching deliverable (b).

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 — C5 Causal adequacy — Two judgment calls. (1) **Topology** — client-side gateway vs gateway-daemon, and whether static-endpoints is the accepted M2.8 shape vs waiting for M3 discovery; the brief defers this to the human (`brief.md:49-53`). (2) **Cross-process inode divergence** (re-derived, not in any artifact note): the cluster path allocates inode ids from `Gateway`'s in-process `AtomicU64::new(1)` (`lib.rs:52,71,126`), but the local CLI path uses the *persisted* `meta:next_inode` counter (`cli.rs:321`). With a persistent redb under `--data-dir` and `metadata::create` doing `require_absent(inode_key(id))` (`metadata.rs:144`), a second new-key `wyrd put --endpoints` in a fresh process re-allocates inode 1 and fails as a spurious `Conflict` — only one distinct object per data-dir survives across invocations. Whether that limit is acceptable for an explicitly pre-production on-ramp is a human call.
- [ ] V — Validation — fitness-to-purpose — Always human at sign-off (`check-gates.json:96-102`, oracle "human at sign-off"). Does this static-endpoints, single-gateway, metadata-local shape match what "run and exercise the distributed system on one machine" should mean for M2.8 (`brief.md:11-30`)? The brief's own sign-off note flags topology as the decision point (`brief.md:49-53`).

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Rejected: the cluster (--endpoints) path allocates inodes from Gateway's in-process counter (lib.rs:71 AtomicU64::new(1)), reset every invocation, while pairing it with a persistent redb under --data-dir. Because metadata::create enforces require_absent(inode_key(id)) (metadata.rs:144), the SECOND distinct-key `wyrd put --endpoints` in a fresh process re-allocates inode 1 and fails as a misleading "concurrent writer won" Conflict — so only one distinct object per --data-dir survives across invocations. Required next: route the cluster path's inode allocation through the persisted meta:next_inode counter, exactly as the local-disk path already does via alloc_inode (cli.rs:319+). M2.8 is a human-testing on-ramp and the poking must be more than one simple round-trip: storing several distinct objects across separate invocations must work. Also: add regression coverage that stores TWO distinct keys across separate gateway compositions (fresh process / new connect_gateway over the same --data-dir) and round-trips both — the current single-put test and single-object README walk-through both stay inside the one-object window and miss this. Topology decision (item 1) is settled and NOT the reason for iterating: static-endpoints client-side gateway with locally-held metadata is accepted as the M2.8 human-testing shape; dynamic discovery / shared-metadata daemon stays M3.
- By / date: Eduard Ralph / 2026-06-21

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
