# Check review — issue 116 / m2.6-tier1-network-dst

Advisory, artifact-only, decorrelated. Inputs: `patch.diff`, `brief.md`,
`check-gates.json` (build-notes deliberately withheld). Citations re-derived
against the target checkout `/home/eddie/wyrd/wyrd` on branch
`feat/m2.6-tier1-network-dst` (read-only; `$PDCA_TARGET` not readable in this
sandbox, but this checkout is the wyrd repo on the exact feature branch, with
the patch applied). I did **not** run the 50-seed `xtask dst` sweep myself; the
green-after claim rests on `check-gates.json` plus the re-derived `xtask` wiring.

## Verdict table

| Item | Verdict | Basis |
| --- | --- | --- |
| C1 — C1 Spec | PASS | A precise, testable spec exists: `brief.md:25-39` enumerates the five Tier-1 properties, proposal 0004 is authoritative; `crates/dst/tests/network.rs:443-454` maps 1:1 onto them. |
| C2 — C2 Reproduction (red pre-fix) | NEEDS-HUMAN | `network.rs` is a *new* file (`patch.diff` `new file mode 100644`) and `check-gates.json` C2 = "none" — no captured pre-fix red run. Red-before rests only on the file's non-existence, not on a demonstrated failing assertion that the seam is what turns it green. |
| C3 — C3 Change | PASS | One logical change = cfg-alias `tonic`→`madsim-tonic` (`crates/proto/Cargo.toml:958-967`, `crates/chunkstore-grpc/Cargo.toml:388-395`, dual-backend `crates/proto/build.rs:20-35`) + testkit network seam (`crates/testkit/src/lib.rs:117-159`) + `network.rs`; scoped to step 6(a–d), no unrelated edits. |
| C4 — C4 Verification (red→green) | PASS | `check-gates.json` C4 (gating) = pass, path_line "xtask ci: all checks passed". Re-derived: `run_ci` invokes `run_dst` (`xtask/src/main.rs:88`), which runs `cargo test -p wyrd-dst` under `--cfg madsim` with `MADSIM_TEST_NUM=50` (`xtask/src/main.rs:98-110`) — so the green gate **does** build and sweep `network.rs`. Green-after is evidenced; red-before is not (see C2). |
| C5 — C5 Causal adequacy | PASS | Addresses the stated root cause (no network-fault coverage) via the *real* wire path, not the recorded fallback fake: `chunkstore-grpc` resolves `tonic`→`madsim-tonic` under `--cfg madsim` (`crates/chunkstore-grpc/Cargo.toml:394`) and `network.rs:476-477` drives the real `GrpcChunkStore`/`ChunkStoreServer`. Properties 3 (corrupt read-around) and 4 (clog→timeout→fail-closed) are load-bearing. Root cause is not contested. Meaningfulness caveat tracked under T5. |
| T1 — T1 Structure | PASS | Seam is well-formed: `NetFault` enum, `NetFaultInjector` trait, `SeededNetFaults` with seed-driven Fisher–Yates pick (`crates/testkit/src/lib.rs:1084-1159`); test harness `Cluster`/`on_client` is coherent (`network.rs:576-638`). `#![cfg(madsim)]` gates the file (`network.rs:462`). |
| T2 — T2 Shape | PASS | Five `#[madsim::test]` fns, one per property, each arrange/act/assert (`network.rs:642,689,745,796,861`), plus two focused testkit unit tests for the seam (`crates/testkit/src/lib.rs:1169,1186`). |
| T3 — T3 Runtime | PASS | Every symbol the test names exists on target: `GrpcChunkStore::connect` (`crates/chunkstore-grpc/src/client.rs:28`), `ChunkStoreService::from_arc` (`server.rs:40`), `FanoutChunkStore::new`/`route`-by-`index%n` (`fanout.rs:37,52`), core `plan_write/intent/write_fragments/commit_create/commit_overwrite/release/write_new_object/sweep_expired_leases` (`crates/core/src/write.rs:104-244`), `read_inode/read_object/read_path` (`crates/core/src/read.rs:42,103,160`), `ChunkId=u128`/`FragmentId{chunk,index:u16}` (`crates/traits/src/*`). Any-`k` read uses `FuturesUnordered`+`break` at `k` (`read.rs:108-129`), so clogging `m` does not hang. Gate's `run_dst` compiled + swept 50 seeds green. |
| T4 — T4 Contribution | NEEDS-HUMAN | Adds real new coverage (4 network-fault properties + seam unit tests). But brief criterion (5) names a three-property commit suite — concurrent-writer-one-wins, **atomicity, no-hybrid-read** — re-run over the gRPC store; only one-wins is re-run (`network.rs:861`). Existing DST suite `crates/dst/tests/concurrency.rs` is itself a single test, while atomicity/no-hybrid-read live in `crates/server/tests/{dst_commit,write_path}.rs` (not in the `wyrd-dst` madsim sweep). Whether those two must also re-run over gRPC is **ambiguous scope** — human to resolve. |
| T5 — T5 Judgment | NEEDS-HUMAN | Properties 3/4/5 are non-vacuous. Property 2 (`k_of_n_read_survives_dropped_fetches`, `network.rs:689`) clogs exactly `m=3` of `n=9` then reads first-`k=6`; it would pass even if `clog_node` were a no-op (all 9 still resolve), and there is no `>m`-dropped negative case — so its fault injection is not proven load-bearing. No bug-finding seed was committed (acceptable only if none was found). Human to confirm campaign strength. |
| V — Validation — fitness-to-purpose | NEEDS-HUMAN | Always-human: only the human at sign-off can judge that the campaign genuinely fulfils proposal 0004 Tier-1's intent over the real wire code. |

## §6 — Items the human must clear

1. **C2 (red pre-fix not captured).** Confirm `network.rs` was demonstrated red
   *because the seam is absent* (e.g. land tests first, or temporarily disable
   the cfg-alias/fault injection and show the assertions fail), not merely red
   because the file did not previously exist.
2. **T4 (ambiguous scope — commit suite).** Brief criterion (5) lists
   atomicity and no-hybrid-read alongside one-wins; only one-wins is re-run over
   the gRPC `ChunkStore`. Decide whether re-running the full M0/M1 suite over
   gRPC is in scope for step 6, or whether one-wins (the only existing DST
   commit test) satisfies it.
3. **T5 (test strength — property 2).** Confirm whether `k_of_n_read_…` should
   assert the fault is load-bearing (e.g. that `>m` drops force
   `InsufficientFragments`, or that the clogged links are observably never
   read), since the current form passes regardless of whether `clog_node` bites.
4. **V (validation fitness-to-purpose).** Human sign-off that the five-property
   network-DST campaign over the real `GrpcChunkStore` is fit for purpose.

## Notes

- The `xtask ci` gate is genuinely informative for this deliverable because
  `run_ci` chains `run_dst` (`xtask/src/main.rs:88`); a green `ci` therefore
  *does* include the `--cfg madsim` / 50-seed sweep of `network.rs`. This is the
  one place where the otherwise-narrow "ci passed" check rises to verifying the
  actual artifact.
- The patch takes the primary path (real `madsim-tonic` wire code), not the
  recorded ChunkStore-level fake fallback — consistent with `brief.md:56-61`.
