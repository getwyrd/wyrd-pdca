# Build notes — issue 252 / M4.1 `metadata-tikv` skeleton + shared conformance

**Withheld from the reviewer.** Rationale, what I ruled out, and the honest
NEEDS-HUMAN edges. Target branch: `getwyrd/wyrd @ main` (feature branch
`feat/m4.1-metadata-tikv-skeleton`). Edits made directly in `../wyrd` (`$PDCA_WORKTREE`
was unset). Every `path:line` below is on `main` + the new files.

## What the slice actually requires (Success criterion, decomposed)

1. The shared trait-contract suite (redb's `conformance.rs:20-111`) **lifted, not
   forked**, passes against TiKV for `get`/`scan`/`commit` basics.
2. redb still passes the **same shared** suite.
3. CI can reach a TiKV via a throwaway single-node in `deploy/`.
4. `cargo xtask ci` exits 0 with the pinned `tikv-client` tree (`cargo deny` green) —
   **including on a machine with no TiKV**, where the TiKV-backed test **skips cleanly**.

The load-bearing invariant is **"one suite, two backends"** (brief §Invariants): a
fork would let the two backends' contracts drift, defeating the whole "trait pinned by
both implementations" point of M4 (proposal 0007 §Motivation, §"DST and tests").

## The change

- **New crate `crates/metadata-conformance`** — the shared suite's home. The four
  generic `contract_*` functions were lifted **verbatim** out of
  `crates/metadata-redb/tests/conformance.rs:20-101` into `src/lib.rs:24-115`. It depends
  on `wyrd-traits` **only** (ADR-0016): it never names a concrete backend, so both
  backends' test targets drive the *identical* assertions.
- **`crates/metadata-redb/tests/conformance.rs`** — the forked copies are **deleted**;
  `trait_contract` (`:26-37`) now calls `wyrd_metadata_conformance::contract_*`. The
  redb-specific model/property tests (`create_*`, `rename_*`, `pending_*`,
  `version_cas_*`) **stay** — they exercise redb's serialized-write-transaction
  guarantee directly and are not part of the backend-agnostic contract (brief scope (c):
  "redb's model/property tests stay where they are"). Dev-dep added
  (`crates/metadata-redb/Cargo.toml:23`).
- **New crate `crates/metadata-tikv`** — `impl MetadataStore for TikvMetadataStore`
  over `tikv-client`'s transactional API (`src/lib.rs:158-207`): the atomic conditional
  commit (read+byte-compare every precondition **inside one txn**, `get_for_update`
  locking read, all-or-nothing puts/deletes, precondition miss → `Ok(Conflict)`), a
  bounded range `scan` `[prefix, upper)`, and byte-identical value storage. Deps are
  `wyrd-traits` + `tikv-client` + `tokio` (+ `async-trait`/`bytes`), **never** `core`
  or a sibling concrete (ADR-0016). Its `tests/conformance.rs` drives the **shared**
  suite.
- **`deploy/tikv-single-node/`** — throwaway single-node `pd`+`tikv` docker-compose,
  **outside** the Cargo workspace (ADR-0010), host-networked so a host client reaches
  PD (`127.0.0.1:2379`) and the store address PD hands back.
- **`xtask tikv-conformance`** (`xtask/src/main.rs:154-`) — brings the stack up, sets
  `WYRD_TIKV_PD_ENDPOINTS`, runs `cargo test -p wyrd-metadata-tikv --features tikv`, tears
  down. **Not** part of `run_ci` (needs a container runtime), exactly like
  `run_integration`.
- **Workspace `Cargo.toml`** — the two new members + workspace-dep entries + the
  `tikv-client = "0.4"` pin (`:73-81`).

## The pivotal design decision: `tikv` is an OFF-by-default feature

The rigorous reason the gate can stay green on a no-TiKV machine while the real backend
still ships:

- `tikv-client` + `tokio` are **optional** deps behind a `tikv` feature that is **off by
  default** (`crates/metadata-tikv/Cargo.toml:19-20`). The default
  `cargo build/clippy/test --workspace` compiles `metadata-tikv` as an empty skeleton
  (`src/lib.rs` feature-less = just the `keyspace` module) and **never compiles the
  `tikv-client` tree**. So `cargo xtask ci` on a laptop/worktree with no TiKV is green.
- The named test skips **before** touching any TiKV code: it reads
  `WYRD_TIKV_PD_ENDPOINTS`, and `None` → early `return` (`tests/conformance.rs:24-31`).
  The suite call and `TikvMetadataStore` reference are behind `#[cfg(feature = "tikv")]`,
  so the feature-less test binary compiles and skips cleanly.

### Cost of the alternative I rejected (unconditional `tikv-client` dep)

Making `tikv-client` a **non**-optional dep of `metadata-tikv` would put it in the
default build graph. Concretely that costs:
- **Every** `cargo xtask ci` (laptop, worktree, `pdca gates`) compiles the whole
  `tikv-client` tree — which, per `cargo tree`, transitively pulls `openssl-sys`
  (`prometheus 0.13 → reqwest → native-tls → openssl-sys`), a **native** build needing
  system `libssl-dev` + `pkg-config`. On this PDCA worktree that is a **hard build
  failure** (no dev headers), so the gate would go red for everyone with no TiKV in
  sight — the exact opposite of Success-criterion clause 4.
- +1449 lines of `Cargo.lock` (a parallel older `tonic 0.10`/`prost 0.12`/`hyper
  0.14`/`axum 0.6` stack) compiled on every green run, for code no default path uses.

Feature-gating pays the +1449 `Cargo.lock` lines (unavoidable — an optional dep is still
locked) but **not** the compile/native-toolchain cost on the default path. That is the
minimal change that restores the invariant "the gate stays green without a TiKV."

## Red → green (what I verified, via `cargo test -p …`, bounded)

- **Pre-fix (red):** the named test `crates/metadata-tikv/tests/conformance.rs` and the
  shared crate did not exist — nothing to compile/run.
- **Post-fix (green), verified headless:**
  - redb `trait_contract` drives the **shared** suite and passes — `cargo test -p
    wyrd-metadata-redb` → `5 passed` (incl. the model tests).
  - `metadata-tikv` unit tests (the load-light `keyspace` math — namespacing + prefix
    upper-bound, real production code the backend uses) → `4 passed`.
  - the named test skips cleanly with no endpoint → `1 passed`.
  - `cargo deny check` → **advisories ok, bans ok, licenses ok, sources ok** with the
    full `tikv-client 0.4` tree in the lock.
  - `cargo fmt --all --check` clean; `cargo clippy … --all-targets` clean;
    `cargo-machete` finds no unused deps (the optional backend deps are declared
    `ignored` for the feature-off scan, `crates/metadata-tikv/Cargo.toml:40-42`).
- **The `tikv`-feature backend compiles against the pinned `tikv-client 0.4`.** I could
  not build it in the default worktree (missing `libssl-dev`/`pkg-config`), so I did a
  **throwaway** verification: temporarily added `openssl` with the `vendored` feature so
  `openssl-sys` builds from source, ran `cargo check -p wyrd-metadata-tikv --features
  tikv --tests` → **compiled clean** (my `TransactionClient::new` / `begin_pessimistic`
  / `get`/`get_for_update`/`scan`/`put`/`delete`/`commit`/`rollback`, `BoundRange`, and
  `KvPair` usage all type-check), then **reverted** the vendored dep — it is **not** in
  the shipped patch (`grep openssl crates/metadata-tikv/Cargo.toml` → none). This
  confirms the API shapes the proposal flagged as "reconfirm at build time" (Open
  questions; issue #260).

## NEEDS-HUMAN (surfaces in SUMMARY §6; not something I may decide or fake)

1. **`tikv-client` dependency audit (ADR-0003, INTEGRATION §4).** Pinned
   `tikv-client = "0.4.0"` (current release of a **pre-1.0** crate — draft 0015's named
   maturity/supply-chain risk). The three-test audit + `deny.toml` allowlist *decision*
   is the human's. Mechanically the whole tree is already allowlist-clean (`cargo deny`
   green — no allowlist edit needed), but the human still owns the *judgment* to accept
   it. Supply-chain note for that audit: 0.4 is pure-Rust (no `grpcio`/C toolchain — good)
   but drags a **duplicate legacy stack** (`tonic 0.10`, `prost 0.12`, `hyper 0.14`,
   `reqwest`/`native-tls`/`openssl-sys` via `prometheus 0.13`'s `push` feature). The
   `openssl-sys` pull means the `xtask tikv-conformance` **CI runner needs `libssl-dev`
   + `pkg-config`** (or a rustls path) — an env note for wiring that job.
2. **`Send + Sync` confirmation (proposal Open questions).** `MetadataStore` is
   object-safe and `Send + Sync`; `TikvMetadataStore` holds a `tikv_client::
   TransactionClient` and the `#[async_trait]` impl compiled under `--features tikv`
   (above), so the client's futures satisfy the bound. Worth the human's confirmation
   against the pinned version as issue #260 tracks.
3. **The real-TiKV conformance PASS is not headless-verifiable here** (no TiKV; the
   `deploy/` compose needs Docker + a runner with the native TLS toolchain). I did **not**
   fabricate a stand-in — the honest state is "skips cleanly." A human (or the
   `xtask tikv-conformance` job) validates clause 1 at sign-off:
   `docker compose -f deploy/tikv-single-node/docker-compose.yml up -d && \
    cargo xtask tikv-conformance`.

## Scope discipline

Kept strictly to M4.1 (proposal 0007 §"Suggested PR sequence" item 1). Deferred, as the
brief's Out-of-scope requires: rigorous write-conflict → `Conflict` classification +
CAS-under-contention (M4.2 #253 — the skeleton reads preconditions in one txn with
`get_for_update` but does not yet fold TiKV write-races or run the CAS property suite);
native **paged** scan + read-consistency doc (M4.3 #254 — a single bounded range is the
sanctioned shortcut); the `server` backend selector (M4.4 #255); the production
`deploy/` tier with the etcd ensemble (M4.5 #256); Jepsen/Tier-1/Tier-2 (M4.6);
DST simulated-TiKV (M4.7). **`crates/traits/src/lib.rs:338-351` is byte-for-byte
untouched** — the milestone's premise.
