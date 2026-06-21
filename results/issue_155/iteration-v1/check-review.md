# Check review — issue 155 / m2.8-local-distributed-cluster-static-endpoints

> Advisory, artifact-only. Inputs: `patch.diff`, `brief.md`, `check-gates.json` (build-notes
> withheld). Citations re-derived against the target checkout (post-patch; the patch is applied
> there — `crates/server/tests/gateway_cluster.rs` and root `docker-compose.yml` both present)
> and against `patch.diff`. The C4 gate result is the builder's run, taken on report (not re-run
> by me).

## Verdict table

| Item | Verdict | Basis |
|------|---------|-------|
| C1 — C1 Spec | PASS | `brief.md:20-30` states a concrete, load-bearing success criterion: a `--endpoints` gateway client mode composing `Gateway` over `FanoutChunkStore<GrpcChunkStore>`, a fixed-port user compose, a README, plus a byte-identical in-process round-trip test and `cargo xtask ci` green. Checkable and self-contained. |
| C2 — C2 Reproduction (red pre-fix) | PASS | Net-new feature: the new test `crates/server/tests/gateway_cluster.rs:310` imports `wyrd_server::cli::connect_gateway`, a symbol this patch adds (`cli.rs:94`). Pre-patch the path does not exist → the test cannot compile → structurally red before the change. No gate configured; re-derived from symbol provenance. |
| C3 — C3 Change | PASS | All three scope items present: (a) `--endpoints` cluster mode composing `Gateway` over `FanoutChunkStore<GrpcChunkStore>` (`cli.rs:45-53,94-151,305-417`); (b) user-facing root `docker-compose.yml` with fixed published ports 50051/52/53 and a volume per D server (`docker-compose.yml:1-67`); (c) README "Run a local cluster" walk-through (`README.md` +51 lines). No `ChunkStore`/proto/protocol change — in scope. |
| C4 — C4 Verification (red→green) | PASS | `check-gates.json` C4 = pass (`xtask ci: all checks passed`, gating). The new test asserts the criterion — byte-identical round-trip — at `gateway_cluster.rs:381-385` (`assert_eq!(got.as_deref(), Some(&data[..]))`) over four real loopback gRPC D servers, plus a miss→`None` case (`:388-395`). Gate is the builder's run, not independently re-executed here. |
| C5 — C5 Causal adequacy | NEEDS-HUMAN | Mechanism is causally sound — the patch supplies the missing user-facing cluster path (`cli.rs:305-381` drive `gateway.put_object`/`get_object` over a real `FanoutChunkStore<GrpcChunkStore>`; placement `index % n` confirmed at `crates/chunkstore-grpc/src/fanout.rs:51-52`; coordination is unused on the data path, `lib.rs:110-144`, so the per-call `MemCoordination::new()` and metadata-local design are consistent). But whether *static-endpoints, client-side gateway* is the accepted M2.8 root-cause remedy (vs a gateway daemon, vs deferring to M3 discovery) is the contested-scope call the brief defers to the human (`brief.md:49-53`). |
| T1 — T1 Structure | PASS | `gateway_cluster.rs` stands up four real tonic gRPC D servers over loopback (`spawn_dserver`, `:317-336`), composes via the shipping `connect_gateway` path (`:363`), drives put→get→assert, then aborts the serve tasks (`:397-399`). Temp dirs/handles kept alive for the test's duration — sound structure, real transport not a fake. |
| T2 — T2 Shape | PASS | Asserts exactly the brief's oracle: byte-identical round-trip (`:381-385`) on a multi-chunk payload (`payload(40*1024+777)` at `chunk_size 8 KiB`, `:367-372`) so each chunk's rs(6,3) fragments fan across the four servers; plus negative shape — unknown key → `Ok(None)` not an error (`:388-395`). |
| T3 — T3 Runtime | PASS | `#[tokio::test(flavor = "multi_thread", worker_threads = 4)]` (`:346`), not `#[ignore]`, no docker — plain loopback TCP, so it runs under `cargo test` inside the C4 `xtask ci` that reported pass (`check-gates.json:33-40`). Compilation/run evidenced by the gate, not re-run by me. |
| T4 — T4 Contribution | PASS | Non-vacuous: exercises the real `connect_gateway` composition (`cli.rs:94-151`) over real gRPC, so a regression in fanout routing, gRPC client transport, or gateway commit would fail it. Note (not a fail): the test bypasses the CLI glue — `parse_endpoints` (`cli.rs:59`), flag dispatch (`cmd_put`/`cmd_get`), `cluster_runtime`, the `GatewayError::Conflict` downcast branch (`cli.rs:325`), and stdout/file output are unexercised; coverage is of the composition, not the argument path. |
| T5 — T5 Judgment | NEEDS-HUMAN | Judgment call: the brief itself elects the in-process loopback proof as the Check criterion and treats the containerized `docker compose up` + cross-process `wyrd put/get` as supplementary manual/nightly (`brief.md:25-30`). Whether that substitution is acceptable evidence for "drives a real cluster" — and whether the untested CLI glue (see T4) is acceptable for this milestone — is a reviewer/human judgment, not a mechanical pass. |
| V — Validation — fitness-to-purpose | NEEDS-HUMAN | Always human. Does this static-endpoints client mode + fixed-port compose + README actually let a user "run and exercise the distributed system on one machine" to the team's satisfaction? Requires the human's end-to-end acceptance at sign-off (`brief.md:11-30`, `check-gates.json` oracle "human at sign-off"). |

## §6 — Items the human must clear (each NEEDS-HUMAN above)

1. **C5 — Topology / accepted shape (contested scope).** Confirm that a *client-side* static-endpoints
   gateway (the `wyrd` binary composes the fan-out store over the containers' published ports, metadata
   held locally under `--data-dir`) is the accepted M2.8 remedy — as opposed to a gateway-daemon container
   holding shared metadata, and as opposed to waiting for M3 dynamic discovery (#139/#141, ADR-0006). The
   brief explicitly routes this here (`brief.md:49-53`).
2. **T5 — Test-tier judgment.** Accept (or reject) the in-process loopback round-trip as the standing Check
   evidence, with the containerized cross-process flow left to manual/nightly. Includes whether leaving the
   CLI argument/dispatch path (`parse_endpoints`, flag handling, the conflict-downcast branch, output
   writing) unexercised by an automated test is acceptable for this milestone (per T4).
3. **V — Fitness-to-purpose.** Final human acceptance that the shipped mode + compose + README genuinely
   satisfy "a user-facing way to run and exercise the distributed system on one machine."

## Reviewer notes (advisory, non-gating — no blocking defect found)

- **No correctness bug surfaced.** Spot checks held: `GrpcChunkStore::connect(impl Into<String>)` is async
  (`crates/chunkstore-grpc/src/client.rs:28`) matching `connect_fanout`'s usage; `FanoutChunkStore::new(Vec<C>)`
  matches (`fanout.rs:37`); `GatewayError` derives `PartialEq, Eq` (`crates/server/src/lib.rs:161`) so the
  `downcast_ref::<GatewayError>() == Some(&GatewayError::Conflict)` branch (`cli.rs` cluster_put) is sound;
  `Gateway` is generic over the three seams at `lib.rs:42` as the brief cites; the data path
  (`put_object`/`get_object`, `lib.rs:110-144`) never touches `coord`, so metadata-local + per-call
  `MemCoordination::new()` does not break a separate-invocation `put`→`get`.
- **Compose detail (cosmetic).** `dserver1` orders `build` before `image`; `dserver2/3` order `image` before
  `build` (`docker-compose.yml`). Both valid; all three build the same `wyrd-dserver:local` image from
  `crates/chunkstore-grpc/tests/dserver/Dockerfile` (confirmed present). No action required.
- **Grounding caveat.** `$PDCA_TARGET` could not be read in this environment (env access was gated). I grounded
  against the patch and against the wyrd checkout supplied as a working directory, which already carries the
  applied patch; if that checkout is *not* the intended target, treat the target-file line citations as
  patch-relative.
