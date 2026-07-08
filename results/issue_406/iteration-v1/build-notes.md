# Build notes — issue 406 (elle-register-listappend-models-and-workload-recorder)

Target branch: `getwyrd/wyrd @ feat/m4-production-metadata-backend` (worktree HEAD
`a7c7408`, the branch tip). Net-new subsystem implementing accepted **ADR-0041**
(§Decision 1/2/3) — the mutable-metadata-register consistency-checker substrate for #329
slice 3. Minimalism maxim does not govern (principle 1.3); there is no invariant to restore.

## What I built

A pure-Rust consistency-checker substrate plus the in-process workload that feeds it, split
so the DST seam library stays import-light and the gateway-driving lives in the (dev-only)
test:

- **`crates/testkit/src/consistency.rs`** (new, 1047 lines) — pure, no async, no server, no
  redb, no JVM. Three checker model families over a recorded history, a history recorder, an
  Elle-compatible EDN serialization, and the off-Check verdict seam:
  - `check_register` — rw-register linearizability of the commit point (ADR-0041 §Decision 1,
    ADR-0015 guarantee 2). Uses the inode `version` as the commit point's linearization index
    ("the commit point totally orders its versions", ADR-0041) so it is polynomial, not a
    linearization search: exactly-one-writer-wins (pass 1), no torn read / version→value is a
    function + value provenance (pass 2), no stale read / no version regression over
    real-time-non-overlapping observations (pass 3).
  - `check_list_append` — namespace linearizability (ADR-0041 §Decision 2, guarantee 1): no
    lost create, no resurrected delete. Conservative over real-time intervals (asserts only on
    *definite* adds/removes) so it never false-rejects a linearizable history.
  - `check_read_your_writes` / `check_monotonic_reads` — per-session guarantee 3, over the
    register and `meta:version` (`META_VERSION_KEY`).
  - `HistoryRecorder` — accumulates the register + namespace histories in one monotone
    recording order (the real-time proxy).
  - `register_to_edn` / `namespace_to_edn` — the checker-compatible Elle op-map EDN the
    off-Check verdict consumes.
  - `verdict_dispatch` — the pure routing value that keeps the JVM/Elle verdict off the
    unprivileged gate, mirroring `xtask/src/metadata_faults.rs:52` `metadata_tier_dispatch`.
- **`crates/testkit/src/lib.rs`** — added `pub mod consistency;` (one module decl).
- **`crates/testkit/Cargo.toml`** — dev-only deps (server/core/redb/fs/mem/gateway-core +
  async-trait/bytes/tempfile/tokio) so the library itself gains **no** new dependency.
- **`crates/testkit/tests/consistency_models.rs`** (new, the brief's named test file) — the
  flippable red→green assertions (a)/(b)/(c) over crafted histories, and (d) the concurrent
  workload driving the **in-process `Gateway`** to produce a non-vacuous, checkable history.

## Why this shape (alternatives ruled out, with cost)

- **Models in the library, gateway-driving in the test.** The DST seam crate is explicitly
  runtime-agnostic ("no wall-clock source enters this DST seam crate", `crates/testkit/Cargo.toml:15`).
  Putting the models there keeps them pure and headless (the runner is headless; a heavy
  runtime dep at load would recur every iterate). Driving the real `Gateway` needs
  server/redb/tokio, so it lives in the **dev-dependency** graph of the test only — the
  library ships **zero** new normal deps (`cargo machete` clean). Alternative — a `wyrd-server`
  normal dep on testkit — would leak tokio + the whole gateway tree into every one of the 8
  crates that depend on `wyrd-testkit`; rejected.
- **Observing the real commit-point version without touching the Gateway** (out of scope:
  "any change to the gateway or `MetadataStore`"). The gateway's `put_object` returns
  `Result<()>` — no version. I share **one** redb `Database` by `Arc` behind a
  `SharedRedb(Arc<RedbMetadataStore>)` `MetadataStore` newtype: the gateway gets a writer
  handle, the workload keeps a reader handle, and redb serves concurrent read transactions
  alongside the gateway's writes. Each register op reads `(version, size)` from **one**
  `read::read_inode` snapshot — atomic — and the write payload's **length is** its value
  token (`payload(v)` = `v` bytes), so `inode.size` recovers the value from the same snapshot
  with no second object read that could race an overwrite. Alternative — reopen the store
  between phases like `closed_write_path.rs` does — cannot express *concurrent* contention (redb
  is single-writer per open handle), so it could not produce the non-vacuous overlapping
  history the criterion (d) demands; rejected.
- **Guaranteed non-vacuity, not probabilistic.** A `tokio::sync::Barrier` makes every process
  record its first write-invoke before any completes → the recorded history is *guaranteed* to
  contain overlapping ops (`max_register_concurrency >= 2`), and retry-until-committed
  overwrites of a hot key *guarantee* the inode version climbs past `commit_create`'s 1
  (`max_observed_version >= 2`). Ran the workload 15× — no flake.

## Forced refutation (the three questions)

- **(a) Genuine red?** YES — behavioral, on real inputs, not a missing symbol. I weakened all
  three model families to accept-everything (`if !ops.is_empty() { return Ok(()) }`) and
  re-ran: the 7 crafted-rejection assertions went **red**
  (`register_rejects_a_torn_read`, `…_version_regression`, `…_two_winners_at_one_commit_point`,
  `list_append_rejects_a_lost_create`, `…_resurrected_delete`,
  `session_rejects_a_read_your_writes_violation`,
  `session_rejects_a_monotonic_read_violation_over_meta_version`) while the accept-valid and
  workload/serialization tests stayed green. Restored the real model → 12/12 green. (Separately,
  my first-draft "valid" history was actually a genuine stale read and the model *rejected* it —
  independent evidence the register check has teeth, not a rubber stamp.)
- **(b) Production path?** YES. The workload drives the **production** `wyrd_server::Gateway`
  (`put_object` / `ObjectGateway::delete_object`) over the real `wyrd_core::write` commit path
  and reads back through `wyrd_core::read::{resolve,read_inode}` — the same in-process gateway
  `crates/server/tests/closed_write_path.rs` drives. The models under test are the shipped
  deliverable itself (pure functions), exercised directly — not a copy or mock.
- **(c) Fixture includes the fault?** YES. Each crafted history *contains* the anomaly it
  asserts on (the torn value, the regressing read, the duplicate-version writes, the omitted
  create, the resurrected name, the backwards `meta:version` read). The workload fixture
  *includes* the non-vacuity it claims: barrier-forced overlap and real, retried overwrites
  that bump the commit point — asserted present, not curated out.

## Verification posture / deferred leg (pre-declared, per brief)

The **Check-exercised core** — the two models, the session checks, the recorder, the
serialization, and the non-vacuous in-process history production — is fully built and green
under `cargo test`. The **live Elle/JVM verdict** over the serialized EDN is the
brief's pre-declared **DEFERRED / off-Check** leg (ADR-0016/ADR-0041 keep JVM/Clojure out of
`cargo xtask ci`), and per the brief's *External dependencies* it is explicitly **not a
build/verify dependency of this bundle** — so I pulled in no JVM/Clojure and there is **no
NEEDS-HUMAN external dependency** here. `verdict_dispatch` encodes that routing as a pure,
unit-checked value; the off-Check job runs the same history the Check-exercised recorder
produces. This is the pre-declared sign-off item the brief's Verification posture names, not a
surprise.

## Commit-readiness (gate steps run in the worktree)

- `cargo fmt --all -- --check` — clean (formatter applied to both new files).
- `cargo clippy --workspace --exclude wyrd-dst --all-targets` — clean (workspace lints =
  `warnings = "deny"`).
- `cargo machete crates/testkit` — no unused dependencies.
- `cargo deny check` — advisories/bans/licenses/sources ok.
- `cargo test -p wyrd-testkit --test consistency_models` — 12/12 green; workload leg 15×
  non-flaky.

The `wyrd-server` (dev) → `wyrd-testkit` and new `wyrd-testkit` (dev) → `wyrd-server` edges
form a **dev-only** dependency cycle, which Cargo permits (dev-deps compile only for the
crate's own tests).

## Citations (path:line on feat/m4-production-metadata-backend)

- ADR-0041 `docs/design/adr/0041-consistency-checker-substrate.md:57-89` — register/list-append
  targeting decision and the JVM-off-Check constraint.
- ADR-0015 `docs/design/adr/0015-consistency-contract.md:22-25` — the three guarantees.
- Commit point / version bump: `crates/core/src/write.rs:271` `commit_overwrite`,
  `crates/core/src/metadata.rs:243` `InodeRecord.version`, `crates/core/src/write.rs:258`
  (`commit_create` version 1), `crates/core/src/metadata.rs:27` `VERSION_KEY`.
- In-process gateway driving pattern (peer): `crates/server/tests/closed_write_path.rs:224-240`
  (Gateway::new + put_object over a shared store), `crates/server/tests/e2e.rs:18-23`
  (in-memory redb + fs chunks + mem coordination).
- Gateway API consumed: `crates/server/src/lib.rs:148` `put_object`, `:315` `delete_object`,
  `:40` `ROOT`; `crates/gateway-core/src/lib.rs:75-98` `GatewayError::Conflict`.
- "Deferred ≠ unbuilt" seam mirrored: `xtask/src/metadata_faults.rs:39-60`
  (`MetadataTierDispatch` / `metadata_tier_dispatch`).
- Testkit oracles peer style: `crates/testkit/src/lib.rs:441` `consistency_passes`,
  `:394` `partition_materialized`.
