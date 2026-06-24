# Check review — issue 203 / fschunkstore-unique-temp-per-write

**Posture.** Advisory, artifact-only, decorrelated from the builder. Inputs: `patch.diff`,
`brief.md`, `check-gates.json` (build-notes.md deliberately withheld). Every `path:line`
below was re-derived against the target source, read-only, at
`/home/eddie/wyrd/wyrd` — confirmed ground truth: it is at `c2223a5` (the exact commit the
brief's repro names), `tests/` holds only `conformance.rs` (the patch's `concurrent_put.rs`
is genuinely net-new), and the patch's context/removed lines match the target byte-for-byte
(base == target HEAD). Citations into `crates/...` are the target tree; citations into
`patch.diff` are the diff.

## Verdict table

| Item | Verdict | Basis |
| --- | --- | --- |
| C1 Spec | PASS | `brief.md:25-34` gives a concrete, testable success criterion (N concurrent same-id puts all `Ok` + published `.frag` verifies + `list_fragments` ignores temps); defect mechanism grounded — shared scratch `{:05}.tmp` at `lib.rs:45-49`, raced at `put_fragment` `lib.rs:82-83`; reachability confirmed (`server.rs:28,48-60`: per-request `&self` over `Arc<S>`, no serialization). |
| C2 Reproduction (red pre-fix) | PASS | Gate `C4-verify` observed "red without the fix" (`check-gates.json` rule `C4-verify`); the net-new test amplifies the interleaving race to near-certain red — 64 writers × 16 rounds released by a `Barrier` (`concurrent_put.rs:180-181,192-203`). Caveat: the red is interleaving-dependent (`brief.md:80-88`) and rests on a single observed gate run. |
| C3 Change | PASS | `temp_path` made per-call unique — `{:05}.{pid}.{seq}.tmp` via a process-global `AtomicU64` (`patch.diff` `temp_path`, replacing target `lib.rs:45-49`); atomic rename kept as the sole publish point, own-scratch removed on write/rename error, stale `.tmp` reaped at `open`. In scope — no `fsync`, no I/O offload, no per-id mutex (out-of-scope items at `brief.md:66-71`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Per-fix red→green PASSED (`check-gates.json` `C4-verify`), but the **gating** `cargo xtask ci` FAILED on the madsim DST tier (`wyrd-dst`, exit 101 — `check-gates.json` `C4-ci`). That tier runs **only** `-p wyrd-dst` under `--cfg madsim` (`xtask/src/main.rs:413,428-448`); the new test isn't in it, and the lone DST `FsChunkStore` user writes **distinct**-id fragments on a fresh store (`crates/dst/tests/concurrency.rs:39,60`), so no patch→failure mechanism is visible from the artifacts. Attribution unresolved → §6.1. |
| C5 Causal adequacy | NEEDS-HUMAN | Fix targets the exact mechanism (shared `<index>.tmp` → private per-call scratch; atomic rename still the only publish), so it is causally adequate for the same-id race. But the oracle is reviewer+human sign-off, and `reap_stale_temps` matches `.tmp` **by suffix, not by pid** (`patch.diff` `is_temp_scratch_name` + `reap_stale_temps`) — safe only under one-opener-per-root. Human to confirm the deployment model and clear the DST question → §6.2. |
| T1 Structure | PASS | Net-new integration test at the brief-designated path `crates/chunkstore-fs/tests/concurrent_put.rs` (`brief.md:77`); proper `#[test]`, tempdir-isolated, no fixture leakage. |
| T2 Shape | PASS | Shape matches the criterion: races 64 same-id writes/round asserting every put `Ok` (`concurrent_put.rs:206-226`), then asserts the single published fragment verifies via `get_fragment` and `list_fragments == [id]` (`concurrent_put.rs:228-244`); the fragment is built with real chunk-format APIs (`FragmentHeader::new_v1` `header.rs:130`, `encode` `codec.rs:32`, `ec_fragment_index` `header.rs:119`). |
| T3 Runtime | PASS | Test executes and flips red→green in the normal build (`check-gates.json` `C4-verify`); genuine concurrency via real OS threads + `Barrier` (FsChunkStore I/O is synchronous `std::fs`, `concurrent_put.rs:160-203`); not `#[ignore]`d. Correctly absent from the madsim tier (chunkstore-fs is `--exclude`d there, `xtask/src/main.rs:413`). |
| T4 Contribution | PASS | First coverage of concurrent same-id puts (only `conformance.rs` existed under `tests/`); guards against reintroducing a shared temp path. Caveat: post-fix the regression signal is probabilistic (interleaving-dependent red), not a structural assertion of temp-path uniqueness. |
| T5 Judgment | NEEDS-HUMAN | Oracle is reviewer+human sign-off. Choosing a dynamic-race amplification (64×16) over a deterministic structural/seam assertion is reasonable, but the brief itself flagged a reliably-flipping timing red as possibly impractical (`brief.md:80-88`) — human to accept the red as robust enough or accept resting the regression on the post-fix invariant → §6.3. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Always-human (`check-gates.json` `V` oracle "human at sign-off"): whether eliminating spurious concurrent-same-id failures actually serves d-server robustness in the deployed profiles → §6.4. |

## §6 — items the human must clear

1. **(C4) Is the madsim DST failure this patch's, or pre-existing?** The gate that fails is
   the gating one (`overall: fail` is driven by `C4-ci`). Decisive test: run the madsim DST
   tier on **bare** `main @ c2223a5` (no patch) — `cargo xtask dst` (`xtask/src/main.rs:428`,
   `MADSIM_TEST_NUM=50`). If it also fails → pre-existing / seed-flaky / environmental; file
   separately, does not block 203. If it passes → this patch regressed the DST tier; root-cause
   before sign-off. From artifacts I find **no** mechanism (the new test is excluded from the
   madsim tier; the only DST `FsChunkStore` user writes distinct-id fragments on a fresh store
   and never exercises the changed shared-temp/reap paths). Note: `build-notes.md` — where Do
   would record any DST triage — is withheld from Check, so this cannot be re-derived from the
   artifacts and genuinely needs a human.

2. **(C5) Confirm one-FsChunkStore-opener-per-root.** `reap_stale_temps` runs at `open` and
   deletes **every** `.tmp` under each `<32-hex>` chunk dir by suffix (not scoped to this
   process's pid). Its safety argument ("no write on this store is in flight yet") holds only
   for the opening instance. If any profile runs a second opener over a shared filesystem root
   (e.g. a multi-process NAS layout) while another process has an in-flight `<index>.<pid>.<seq>.tmp`,
   the reap would delete that live scratch. Safe under single-opener-per-root; confirm that is
   the only supported model, or scope the reap to skip pids that are alive / recent files.

3. **(T5) Accept the regression guard's robustness.** The pre-fix red rests on thread
   interleaving (amplified to 64×16), not a structural assertion that the temp path is unique.
   The brief anticipated this and offered the CLAUDE.md "no test because X" route (rest on the
   post-fix invariant). Human to accept the amplified dynamic red as robust enough, or require
   an added structural assertion of per-write scratch privacy.

4. **(V) Fitness-to-purpose sign-off** — the standing always-human gate.

## What I verified vs. could not

- **Verified (re-derived against target):** patch base == target HEAD; defect mechanism and
  its citations (`lib.rs:45-49`, `:82-83`, `:221-223`; `server.rs:28,48-60`); the test's APIs
  exist and match (`chunk-format`); scope adherence (no fsync/offload/mutex); and that the
  failing gate runs `-p wyrd-dst` only, with the lone DST FsChunkStore user writing distinct-id
  fragments.
- **Could not verify (artifact-only, no build-notes, did not run the suite):** whether the
  madsim DST tier is red on bare `main` (the crux of §6.1); the deployment opener model (§6.2);
  and any fitness-to-purpose judgment (§6.4). These are the NEEDS-HUMAN rows above.
