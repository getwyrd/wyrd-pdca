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
