# Build notes — issue 257 / m4.6-real-commit-over-madsim-tikv (iteration 8)

**Withheld from the reviewer.** Rationale for the human at sign-off.

## What changed vs iteration 7, mapped to the five must-fixes

Iteration 7's Option-B posture was **ratified** (no `madsim-tikv-client` release exists), but
the deferred Tier leg was broken, the seed was an unflippable toy, and the at-Check red was a
compile-flip. This iteration keeps the ratified posture and fixes exactly those.

### MUST-FIX 4 + 5 — the seed is now bound to production code and flips behaviourally

`crates/dst/tests/tikv_await_commit_interleaving.rs` was a self-authored `KeyModel` toy
(imports only `std` + `madsim`, both halves true by construction). It is **rewritten to drive
the real, unchanged production commit path**: `wyrd_core::write::commit_overwrite` →
`wyrd_core::metadata::commit_chunk_map`, which issues the trait-level CAS
`MetadataStore::commit(WriteBatch::require(prior).put(next))`
(`crates/core/src/metadata.rs:299-317`) over a real `RedbMetadataStore::in_memory()`. Two
madsim-scheduled writers read the same prior, cross an `.await` window, and race to commit; the
seed asserts **exactly one wins, the stale committer gets `Conflict`, and the version advances
by exactly one**.

That CAS is the **same `MetadataStore::commit` contract `TikvMetadataStore::commit` implements**
behind the byte-for-byte-unchanged trait, so the assertion is validated by production code, not
by a branch I wrote — the independence v1–v7 lacked.

**Demonstrated red→green against production (the brief's required temporary, discarded
perturbation):**
- GREEN (production intact): `RUSTFLAGS="--cfg madsim" MADSIM_TEST_NUM=50 cargo test -p wyrd-dst
  --test tikv_await_commit_interleaving` → `ok. 1 passed` (holds under all 50 seeds).
- RED: I temporarily deleted the `.require(encode(prior))` precondition from
  `wyrd_core::metadata::commit_chunk_map` (the production commit-point re-check). The seed went
  **red behaviourally**: `assertion left == right failed … left: 2 right: 1` — two committers
  won, a real lost update (`MADSIM_TEST_SEED=1783272044338746889`). I then **discarded** the
  perturbation (`git diff crates/core/src/metadata.rs` is empty; `crates/core` is **not** in
  `patch.diff`).

This is a behavioural flip against production ordering — not a `CommitMode` flag, not
file-absence, not a compile error. It executes at Check via `cargo xtask ci` → `run_dst` under
`--cfg madsim`.

**Honest scope (must-fix 4 option b, kept explicit in the module docs):** redb realizes this
contract with a *synchronous* commit, so the seed cannot exhibit TiKV's await-*inside*-`commit()`
percolator window. It therefore makes **no** correctness claim about that window — that leg is
Option-B/off-Check. The seed's claim is narrow and true: the commit-point re-check contract holds
against production code under the interleaving `concurrency.rs:3-4` declares impossible.

### MUST-FIX 1 — truly bidirectional partition (was a receive-only cut)

The v7 leg dropped `--dport <port>` on a **shared** `127.0.0.1` loopback — receive-only, and it
could not even identify the node's outbound traffic (shared netns). Fixed structurally:
`deploy/tikv-multi-replica/docker-compose.yml` now binds each node to a **distinct loopback IP**
(pd/tikv-0 = 127.0.0.1, tikv-1 = **127.0.0.2**, tikv-2 = 127.0.0.3, via `--advertise-addr`). The
node is now identifiable by IP, and `SymmetricPartition::rules()` drops traffic with **both
`-s <ip>` and `-d <ip>` on INPUT and OUTPUT** — the node can neither send nor receive. The host
still reaches every store on loopback (host networking retained), so PD hands back reachable
addresses.

### MUST-FIX 2 — peer-side fault-effect oracle + the pure oracles are wired in

The oracle no longer probes the dropped port from the test host. It asks **PD** (the peers'
coordinator) whether it still sees the store `Up`, via `/pd/api/v1/stores`
(`pd_store_state`, a dependency-free raw-HTTP GET). `connected_before`/`connected_during` feed
`wyrd_testkit::partition_took_effect(before, during)` — a one-way or probe-only cut leaves PD's
view `Up`, so the oracle returns `false` and the leg fails (red when the fault is a no-op).

The previously-dead `partition_took_effect` / `heal_is_complete` are **moved to `wyrd-testkit`**
(so the scenario in `metadata-tikv`, which dev-deps testkit, can call them — it cannot depend on
`xtask`) and are now **called by the live scenario**, not just their own tests. `xtask::metadata_faults`
keeps only the run-routing dispatch (mirroring `jepsen_dispatch`).

### MUST-FIX 3 — verified, non-lossy heal

`SymmetricPartition::heal()` removes **every** applied rule, **surfaces** each `iptables -D`
failure (returns `Err`, `eprintln!`s — no silent `let _ =`), records the healed set, and the
scenario waits until **PD reports the store `Up` again** before accepting the heal via
`heal_is_complete(applied, healed, connected_after)`. The `Drop` impl is now a panic-safety net
that **warns loudly** on any residual rule instead of leaking host firewall state silently.

## Evidence architecture (what is graded where)

- **At-Check, behavioural, against production:** the DST seed (above) — runs under
  `cargo xtask ci` → `run_dst`, flips on a real production commit-point regression.
- **At-Check, pure independent oracles (adversary "could not refute" in v7 — kept):** the
  `wyrd-testkit` quorum/partition arithmetic, `ConsistencySignals`/`consistency_passes`,
  `converged_exactly_once`, and the two fault-effect oracles — 18 testkit unit tests + 3 xtask
  dispatch tests, all hand-computed expectations (not the literal the fn returns). They go red
  behaviourally in `cargo test --workspace` when the arithmetic/routing is regressed.
- **Off-Check (Option-B, privileged Tier job):** the live ADR-0015-on-real-TiKV proof — Tier-1
  integration (multi-key create/rename/delete) + consistency (independent read-after-commit /
  exactly-once signals across the heal) + Tier-2 single-node I/O. Built and type-checked under
  `--features tikv`; runs only with a Docker host + `WYRD_TIER1=1`/`WYRD_TIER2=1`.

## Gates run (in `$PDCA_WORKTREE`, on `feat/m4-production-metadata-backend`)

- `cargo fmt --all` — clean.
- `cargo clippy -p wyrd-testkit -p xtask -p wyrd-metadata-tikv --features
  wyrd-metadata-tikv/tikv --tests -- -D warnings` — clean (the tikv-feature scenario path
  type-checks and lints).
- `cargo xtask ci` — **all checks passed** (fmt, clippy, build, `--workspace` tests, cargo-deny,
  conformance vectors). Note: one **pre-existing flake** surfaced once and passed on re-run —
  `wyrd-custodian --test reconstruction::an_aborted_repair_is_not_counted_as_a_successful_repair`
  (a shared global-telemetry-registry race in the custodian test binary; my diff touches no
  custodian code). It passed cleanly on the second full `cargo xtask ci` run.
- Seed red→green demonstrated as above.

## Invariants

- `crates/traits/src/lib.rs` — **untouched** (not in the diff).
- `crates/metadata-tikv/src/**` — **untouched** (only a `[dev-dependencies]` line added to
  `crates/metadata-tikv/Cargo.toml`).
- `crates/core/**` — **untouched** (the demonstration perturbation was discarded).
- No new ADR minted (the metadata-nemesis methodology question stays the architecture board's,
  per the brief's NEEDS-HUMAN).

## Honest limitations for the human

1. **The off-Check Tier-1/Tier-2 legs are unexecuted here** (no privileged Docker host). They
   compile and type-check under `--features tikv`; the `pd_store_state` raw-HTTP parser and the
   `iptables`/PD-heartbeat timings are validated by inspection only. Manual validation:
   `WYRD_TIER1=1 cargo xtask metadata-tier1` on a Docker host stands up `deploy/tikv-multi-replica`,
   isolates 127.0.0.2 bidirectionally, expects PD to mark it `Disconnected` (~20–45 s), the
   majority to keep committing, a complete heal (PD sees it `Up` again), and the three
   independent ADR-0015 signals green; flip the partition to one-way and the fault-effect oracle
   must go red. `WYRD_TIER2=1 cargo xtask metadata-tier2` runs the single-node I/O cycle. This is
   a pre-declared C2/C4 off-Check item — name the Tier-job owner at sign-off.
2. **C4-verify (advisory, `run-verify.sh`)** cannot exercise this slice's behavioural flip: the
   flippable seed is `#![cfg(madsim)]` (like `concurrency.rs`) and the tier legs are
   cluster-gated, while `run-verify` runs plain `cargo test`. This is an additive test-evidence
   slice with no revertable production `src` change, so `run-verify` has nothing to isolate a
   per-fix red against. The genuine behavioural red→green is the seed under `cargo xtask dst`
   (demonstrated above) — I did **not** engineer a compile-flip to make `run-verify` falsely
   report a behavioural red (the v7 defect). Grade the at-Check flip on `cargo xtask ci`.
3. **`madsim-tikv-client` absence** (the Option-B trigger) is a build-time `cargo search` claim;
   the human confirms the registry has no release tracking `tikv-client 0.4`.
