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

> Advisory, artifact-only, decorrelated. Inputs: `patch.diff`, `brief.md`, `check-gates.json`
> (build-notes.md withheld). Citations re-derived against the target source at the M2.8
> checkout (post-patch: `crates/server/tests/gateway_cluster.rs` present there), read-only.

## Verdict table

| Item | Verdict | Basis |
| --- | --- | --- |
| C1 — C1 Spec | PASS | `brief.md` is self-contained with a load-bearing Success criterion (brief.md:20-30): a static-endpoints `--endpoints` mode composing a `FanoutChunkStore<GrpcChunkStore>` + a user compose with fixed ports + a README walk-through; C4-verify = byte-identical loopback round-trip and `cargo xtask ci` green. Scope/out-of-scope explicit (brief.md:33-38). |
| C2 — C2 Reproduction (red pre-fix) | PASS | No C2 gate configured (check-gates.json); re-derived from source. The new test `cluster_gateway_round_trips_distinct_objects_across_separate_compositions` (patch.diff: gateway_cluster.rs:409-510) encodes the exact iteration-2 failure mode — a second distinct key over a separate composition on one `--data-dir`. A per-process counter would re-allocate inode 1 → bogus `Conflict` and reuse chunk id 1; so the test is red pre-fix by construction. Not independently executed (artifact-only). |
| C3 — C3 Change | PASS | `patch.diff` is coherent and single-purpose: `--endpoints` dispatch in `cmd_put`/`cmd_get` (cli.rs:150-153, 202-208), the cluster client path (cli.rs:399-559), a root `docker-compose.yml`, a README section, and a loopback test. Reuses existing seams (`FanoutChunkStore::new` fanout.rs:37, `GrpcChunkStore::connect` client.rs:28) — no trait/proto change, matching out-of-scope. |
| C4 — C4 Verification (red→green) | PASS | check-gates.json gating row `C4-ci` = pass ("xtask ci: all checks passed", `./engine/xtask.sh ci`), covering fmt/clippy/build/test/deny/conformance. Reported gate accepted as advisory; not re-run here. |
| C5 — C5 Causal adequacy | PASS | Root cause (iteration-2: cluster path drew inodes from `Gateway`'s in-process `AtomicU64` lib.rs:71, colliding with persistent redb) is uncontested and directly addressed: `cluster_store_put` allocates via the persisted `meta:next_inode` counter `alloc_inode` (cli.rs:456 → cli.rs:340-358) and inode-derived chunk ids (`chunk_id_minter` cli.rs:362), the same composition as the local-disk path (cli.rs:158-159). |
| T1 — T1 Structure | PASS | Test lives at the crate's integration path `crates/server/tests/gateway_cluster.rs`; stands up real tonic D servers over loopback (gateway_cluster.rs:379-398) and drives the shipping `cluster_store_put`/`cluster_store_get`/`connect_fanout`/`open_cluster_meta` exports (cli.rs:448/478/416/435), not a stand-in. |
| T2 — T2 Shape | PASS | Asserts the load-bearing case: two distinct keys committed across two *separate* compositions over one data-dir (gateway_cluster.rs:449-476), both round-trip byte-identically (gateway_cluster.rs:481-496), and a miss returns `Ok(None)` (gateway_cluster.rs:499-505). Re-reading obj/one after obj/two's PUT proves no chunk-id clobber. |
| T3 — T3 Runtime | PASS | No T3 gate configured; the suite ran under the green `C4-ci` gate (check-gates.json), which includes `test`. Not independently executed here (artifact-only). |
| T4 — T4 Contribution | PASS | Net-new coverage of the `--endpoints` path (none existed — prior-art check, brief.md:42-46), and specifically the multi-object-across-invocations case the prior single-put test and single-object README walk-through both missed (iteration-2 carry-forward, brief.md:65). |
| T5 — T5 Judgment | NEEDS-HUMAN | Mechanism divergence from the brief's wording: the Success criterion names "composing `Gateway` … over a `FanoutChunkStore<GrpcChunkStore>`" (brief.md:21-22), but the patch deliberately bypasses the `Gateway` struct and calls `write::write_new_object`/`read::read_path` directly (cli.rs:448-487). This is well-justified — routing through `Gateway` would reuse its in-process counter (lib.rs:71), resurrecting the iteration-2 bug — but a human should confirm the divergence is acceptable rather than requiring `Gateway` to grow a persisted allocator. |
| V — Validation — fitness-to-purpose | NEEDS-HUMAN | Topology is an explicit human call (brief.md:49-53 sign-off note): client-side gateway with locally-held metadata vs a shared-metadata gateway daemon, and whether static-endpoints is the accepted M2.8 shape vs waiting for M3 discovery. Iteration-2 carry-forward records the topology as *settled* (brief.md:65), but fitness-to-purpose at sign-off is reserved for the human (check-gates.json: oracle "human at sign-off"). |

## §6 — items the human must clear

1. **T5 — `Gateway` bypass (mechanism).** The brief's Success criterion names composing `Gateway` over the fan-out store; the patch instead replicates the local-disk path (`alloc_inode` + `chunk_id_minter` + `write_new_object`) directly, holding metadata locally, because composing `Gateway` would reintroduce the in-process inode counter (lib.rs:71) that iteration 2 rejected. Confirm this divergence is the intended resolution (and that `Gateway` is not expected to be the carrier of the cluster path at M2.8).

2. **Validation — topology / fitness-to-purpose.** Accept (or not) the static-endpoints, client-side-gateway-with-local-metadata shape as the M2.8 human-testing on-ramp, vs a shared-metadata daemon or waiting for M3 discovery (ADR-0006). The brief treats this as settled from iteration 2 but reserves the final fitness call for sign-off.

## Reviewer notes (non-gating)

- The iteration-1 carry-forward (ship four D servers, not three, for rs(6,3) headroom) is satisfied: `docker-compose.yml` defines `dserver1..4` on fixed ports 50051-50054 with a volume each (patch.diff: docker-compose.yml:539-591), the README endpoint list is four (README:34), and the loopback test uses four servers (gateway_cluster.rs:417).
- The iteration-2 carry-forward (persisted inode allocation + a two-distinct-keys regression test) is satisfied — see C5 / T2.
- Minor, non-blocking: all four compose services build the same image from `crates/chunkstore-grpc/tests/dserver/Dockerfile`; `dserver1` lists `build` before `image` while 2-4 list `image` before `build` (cosmetic). The user-facing compose reuses the *test* Dockerfile path — intentional per the header comment, but a human-facing asset depending on a `tests/` build context is worth a glance.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] T5 — T5 Judgment — Mechanism divergence from the brief's wording: the Success criterion names "composing `Gateway` … over a `FanoutChunkStore<GrpcChunkStore>`" (brief.md:21-22), but the patch deliberately bypasses the `Gateway` struct and calls `write::write_new_object`/`read::read_path` directly (cli.rs:448-487). This is well-justified — routing through `Gateway` would reuse its in-process counter (lib.rs:71), resurrecting the iteration-2 bug — but a human should confirm the divergence is acceptable rather than requiring `Gateway` to grow a persisted allocator.
- [x] V — Validation — fitness-to-purpose — Topology is an explicit human call (brief.md:49-53 sign-off note): client-side gateway with locally-held metadata vs a shared-metadata gateway daemon, and whether static-endpoints is the accepted M2.8 shape vs waiting for M3 discovery. Iteration-2 carry-forward records the topology as *settled* (brief.md:65), but fitness-to-purpose at sign-off is reserved for the human (check-gates.json: oracle "human at sign-off").

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: merged-wider
- Iteration delta (if iterating):
- By / date: Eduard Ralph / 2026-06-21

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
