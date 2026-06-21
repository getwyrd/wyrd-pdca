# Result — issue 117 / m2.7-tier2-integration-throughput-bench

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: 
- Success criterion: a new Tier-2 integration test, run on a checkout with a
- Repo + branch target: getwyrd/wyrd @ main   (INTEGRATION §2 — single line; no maintenance branches)
- Scope (one logical fix) / out of scope: realize **PR step 7 only** — (a) a Tier-2 container integration test

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

# Check review — issue 117 / m2.7-tier2-integration-throughput-bench

> Advisory, artifact-only, decorrelated. Inputs: `patch.diff`, `brief.md`,
> `check-gates.json` (build-notes deliberately withheld). Every Basis below is
> re-derived against the target source at `$PDCA_TARGET = /home/eddie/wyrd/wyrd`
> (read-only) or against `patch.diff` directly.

## Verdict table

| Item | Verdict | Basis |
|------|---------|-------|
| C1 — C1 Spec | PASS | Brief is a governed pointer to proposal 0004 PR step 7 with an explicit, testable success criterion (`brief.md:23-36`); the patch targets exactly that — Tier-2 container write/read byte-identical + tracked throughput bench wired into `cargo xtask`. |
| C2 — C2 Reproduction (red pre-fix) | NEEDS-HUMAN | Net-new born-at-M2 tier: no pre-existing failing test to flip. The "red" is *criterion unmet / tier absent on main* (prior-art search, `brief.md:61-66`), whose green demonstration is observable only on a Docker-capable host (`brief.md:32-36`); the new test no-ops without `WYRD_DSERVER_ENDPOINTS` (`patch.diff` tier2_integration.rs:236-245), so it is inert, not red, artifact-only. Build-notes withheld → no repro evidence visible. |
| C3 — C3 Change | PASS | Diff is coherent, minimal, on-scope; every API it calls grounds on `main`: `FanoutChunkStore::new`/`route(i%n)`/`health` (`crates/chunkstore-grpc/src/fanout.rs:37,52,70`), `GrpcChunkStore::connect` (`crates/chunkstore-grpc/src/client.rs:28`), `ChunkStoreService::new` + re-exported `ChunkStoreServer` (`crates/chunkstore-grpc/src/server.rs:32`, `src/lib.rs:31,36`), `plan_write`/`write_fragments`/`chunk_refs` (`crates/core/src/write.rs:104,162,60`), `read_object_from` (`crates/core/src/read.rs:56`), `InodeRecord{size,chunk_map,state,version}`/`InodeState::Committed` (`crates/core/src/metadata.rs:84,48`), `Health::Healthy` (`crates/traits/src/lib.rs:55`), `DEFAULT_DURABILITY = rs(6,3)` (`crates/server/src/lib.rs:28`), `FsChunkStore::open` (`crates/chunkstore-fs/src/lib.rs:31`). |
| C4 — C4 Verification (red→green) | PASS | Configured Check gate `cargo xtask ci` is green (`check-gates.json:33-39`, gating, result `pass`); it compiles both new targets via clippy+build `--all-targets` (`xtask/src/main.rs:259,269`) and runs `cargo deny check` + conformance (`xtask/src/main.rs:272-273`). Per brief the in-CI obligation is compile+wired+ci-green (`brief.md:33-36`); the container red→green run is supplementary/post-merge (rolled into C2/V), not gated. |
| C5 — C5 Causal adequacy | NEEDS-HUMAN | Always-human (gate oracle "reviewer + human sign-off", `check-gates.json:42-43`). Judgment owed: does this wiring actually make the §10 Q6 "no shared bottleneck"/throughput-scaling claim *adequately measurable*, given the bench measures an **in-process loopback** cluster (`patch.diff` throughput.rs:404-436), not the containerized cluster, as the Q6 proxy. |
| T1 — T1 Structure | PASS | Files land in the brief's named homes: test at `crates/chunkstore-grpc/tests/tier2_integration.rs` (`brief.md:53`), bench at `crates/core/benches/throughput.rs` (`brief.md:56-58`), plus `xtask integration`/`bench` subcommands and the nightly workflow — conventional workspace layout, no stray files. |
| T2 — T2 Shape | PASS | Idiomatic and consistent with existing peers: one `#[tokio::test]` gated `#[ignore]` + env-skip guard (`patch.diff` tier2_integration.rs:233-245), criterion groups with CI-bounded sampling mirroring the M1.7 EC bench (`patch.diff` throughput.rs:484-492), loopback cluster mirrors `tests/round_trip.rs`. |
| T3 — T3 Runtime | PASS | By inspection the runtime guards are correct: default `cargo test` never needs Docker — the test is `#[ignore]`d (`patch.diff` tier2_integration.rs:234) and additionally no-ops when endpoints are absent (tier2_integration.rs:236-245); `xtask integration` runs it `--ignored` against live endpoints with unconditional teardown (`patch.diff` xtask main.rs:584-593). The d-server CLI contract the containers invoke exists and is dependency-free for this path — `wyrd d-server --bind --data-dir` (`crates/server/src/cli.rs:58,83`; `[[bin]] name="wyrd"` `crates/server/Cargo.toml:12`) uses a process-local `MemCoordination` (`crates/server/src/cli.rs:247`), so the test's explicit-endpoint injection (not cross-process discovery) is the right path. Live container execution is supplementary nightly evidence (see V), unobservable artifact-only. |
| T4 — T4 Contribution | PASS | Genuine net-new coverage: a real-tonic/HTTP-2/prost transport tier that no in-process Tier-0/1 test exercises (`patch.diff` tier2_integration.rs:170-191) plus the first aggregate throughput data points; prior-art search confirms net-new, no duplication (`brief.md:61-66`). Not a tautological/vacuous test — it asserts byte-identical read over the wire (tier2_integration.rs:306-309). |
| T5 — T5 Judgment | NEEDS-HUMAN | Always-human (gate oracle "reviewer + human sign-off", `check-gates.json:88-89`). Design calls a human should accept: docker-CLI-shellout instead of the `testcontainers` crate (avoids a new cargo dep — see §6.1), in-process loopback as the Q6 throughput proxy, and nightly-not-PR gating of the container job. |
| V — Validation — fitness-to-purpose | NEEDS-HUMAN | Always-human (gate oracle "human at sign-off", `check-gates.json:98`). Whether the change satisfies proposal 0004 PR step 7 *end-to-end* (real-hardware throughput numbers, the §10 Q6 basis) is the human's fitness call; it also introduces new **non-cargo** external surfaces requiring governance sign-off (see §6.1). |

## §6 — Items the human must clear (each NEEDS-HUMAN row)

1. **Dependency / supply-chain audit — reframed (V, T5, C5).** The brief anticipated a
   new `testcontainers` crate triggering the ADR-0003 three-test dependency audit
   (`brief.md:69-75`). **Re-derived: the patch adds no new cargo crate** — it drives
   Docker via `std::process::Command` (`patch.diff` xtask main.rs:597-718), and
   `Cargo.lock` gains only dependency-*list* edits to existing workspace crates, no new
   `[[package]]` stanza (`patch.diff:68-93`). The cargo audit is therefore covered by the
   green `cargo deny check` (`xtask/src/main.rs:272`). **However**, the change introduces
   external surfaces `deny.toml` does **not** govern, which the human should clear under
   INTEGRATION §4's spirit:
   - Two Docker base images — `rust:1.96-bookworm` and `debian:bookworm-slim`
     (`patch.diff` Dockerfile:11,17).
   - Two third-party GitHub Actions — `actions/checkout@v4`, `Swatinem/rust-cache@v2`
     (`patch.diff` integration-nightly.yml:54,60).

2. **Red→green reproduction on real containers (C2, and the supplementary half of C4).**
   The artifact-only reviewer cannot run Docker. The classical red-pre-fix / green-post-fix
   demonstration is host-dependent (`brief.md:32-36`) and the new test is inert without
   live endpoints. Human confirms the container job goes green on a Docker-capable host
   (the nightly lane, `patch.diff` integration-nightly.yml).

3. **Causal adequacy / fitness of the throughput proxy (C5, V).** The `cargo xtask bench`
   throughput sweep runs an **in-process loopback** tonic cluster (`patch.diff`
   throughput.rs:404-436), not the containerized cluster, and the numbers are tracked-not-
   gated (`patch.diff` throughput.rs:484-492). Human judges whether this is an adequate
   first measurement of the §10 Q6 "scales close to linearly with D-server count" claim,
   or whether the claim needs the container path's numbers to be considered satisfied.

## Notes the human may find useful (re-derived, not gating)

- **No build cycle.** The `core ↔ chunkstore-grpc` cross-references are both
  `[dev-dependencies]` (`crates/core/Cargo.toml:34`, `crates/chunkstore-grpc/Cargo.toml:42`);
  cargo permits dev-dependency cycles, so the `--all-targets` build in `xtask ci` is the
  proof these compile — consistent with `check-gates.json` overall `pass`.
- **Scope is clean.** The patch touches only step-7 surfaces; the brief's out-of-scope
  items (`ChunkStore` trait, `wyrd.v0` proto, commit protocol — `brief.md:48-52`) are
  untouched in the diff.
</content>
</invoke>

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C2 — C2 Reproduction (red pre-fix) — Net-new born-at-M2 tier: no pre-existing failing test to flip. The "red" is *criterion unmet / tier absent on main* (prior-art search, `brief.md:61-66`), whose green demonstration is observable only on a Docker-capable host (`brief.md:32-36`); the new test no-ops without `WYRD_DSERVER_ENDPOINTS` (`patch.diff` tier2_integration.rs:236-245), so it is inert, not red, artifact-only. Build-notes withheld → no repro evidence visible.
- [x] C5 — C5 Causal adequacy — Always-human (gate oracle "reviewer + human sign-off", `check-gates.json:42-43`). Judgment owed: does this wiring actually make the §10 Q6 "no shared bottleneck"/throughput-scaling claim *adequately measurable*, given the bench measures an **in-process loopback** cluster (`patch.diff` throughput.rs:404-436), not the containerized cluster, as the Q6 proxy.
- [x] T5 — T5 Judgment — Always-human (gate oracle "reviewer + human sign-off", `check-gates.json:88-89`). Design calls a human should accept: docker-CLI-shellout instead of the `testcontainers` crate (avoids a new cargo dep — see §6.1), in-process loopback as the Q6 throughput proxy, and nightly-not-PR gating of the container job.
- [x] V — Validation — fitness-to-purpose — Always-human (gate oracle "human at sign-off", `check-gates.json:98`). Whether the change satisfies proposal 0004 PR step 7 *end-to-end* (real-hardware throughput numbers, the §10 Q6 basis) is the human's fitness call; it also introduces new **non-cargo** external surfaces requiring governance sign-off (see §6.1).

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
- issue_117: throughput/scaling numbers (§10 Q6) deliberately deferred — to be measured as a post-merge follow-up outside this commit, on real hardware off the nightly lane.
