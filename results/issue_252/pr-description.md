# feat(metadata-tikv): backend skeleton + shared conformance suite (M4.1)

## Summary

Wyrd's metadata could only be stored in the embedded, single-node `redb`
backend — there was no distributed option, and nothing proved the
`MetadataStore` contract could be met by a second implementation. This adds the
first distributed backend as a skeleton: a `TikvMetadataStore` covering the
basic `get` / `scan` / `commit` shapes over TiKV's transactional API, and lifts
the existing trait-contract tests into one shared suite that both backends run.

## What to look at

- **`crates/metadata-tikv/src/lib.rs`** — `impl MetadataStore for
  TikvMetadataStore` (the `get` / `scan` / `commit` implementation).
- **`crates/metadata-conformance/src/lib.rs`** — the shared trait-contract
  suite, depending only on the traits crate so both backends drive the
  *identical* assertions.
- **`crates/metadata-redb/tests/conformance.rs`** — redb now calls the shared
  suite instead of its former in-file copies (verbatim lift, not a fork).
- **`crates/metadata-tikv/Cargo.toml`** — the TiKV dependency tree is an
  optional, off-by-default `tikv` feature; this is what keeps the default gate
  green without TiKV.
- **`deploy/tikv-single-node/`** + **`xtask/src/main.rs`** — a throwaway
  single-node TiKV and the `cargo xtask tikv-conformance` task that runs the
  suite against it.

To exercise it:

- **No TiKV (default gate):** `cargo xtask ci` — green; the TiKV conformance
  test skips with a "no endpoint configured" line.
- **Against real TiKV:** `docker compose -f deploy/tikv-single-node/docker-compose.yml up -d`
  then `cargo xtask tikv-conformance`.

## Root cause

Only one concrete `MetadataStore` existed (`redb`), so the trait's suitability
as a backend seam was unproven and its contract tests were tied to that single
backend. A distributed backend was needed both to serve production topologies
and to demonstrate the trait can be satisfied by more than one implementation.

## Fix

- New crate `crates/metadata-tikv` implementing the `get` / `scan` / `commit`
  basics over TiKV's transactional API; it depends on the traits crate plus the
  TiKV client and runtime only — never on core or a sibling backend.
- New crate `crates/metadata-conformance` holding the backend-agnostic
  trait-contract suite, lifted verbatim out of the redb test file so the two
  backends cannot drift; redb's storage-model and property tests stay put.
- The TiKV dependency tree is gated behind an off-by-default `tikv` feature, so
  the default build/lint/test path never compiles it and stays green on a host
  with no TiKV; the conformance test probes for an endpoint and skips cleanly
  when none is set.
- A throwaway single-node TiKV under `deploy/` (outside the Cargo workspace)
  plus a `cargo xtask tikv-conformance` task to run the shared suite against it.
- The TiKV client is pinned in the workspace manifest. Because its tree sits
  behind the off-by-default `tikv` feature, the **audited (default, shipped) build
  graph excludes it** and `cargo deny check` is clean on what this slice ships —
  see the *Dependency audit (ADR-0003)* note below for the decision on the full tree.

The `MetadataStore` trait is untouched — a second implementation satisfying the
same frozen contract is the deliverable.

## Verification

- **Claim:** the shared conformance suite passes on both backends, and redb
  still passes the same suite.
  **Checked:** `crates/metadata-conformance/src/lib.rs:24-115` (shared
  `contract_*` functions, traits-only dependency) driven by
  `crates/metadata-redb/tests/conformance.rs:26-37`.
  **Test:** `cargo test -p wyrd-metadata-redb` → 5 passed (including redb's
  model tests).

- **Claim:** the TiKV backend implements the `get` / `scan` / `commit` shapes
  against the frozen trait.
  **Checked:** `crates/metadata-tikv/src/lib.rs:158-207` against the trait at
  `crates/traits/src/lib.rs:338-351` (unchanged by this PR).

- **Claim:** the default gate stays green with no TiKV, and the TiKV test skips
  cleanly.
  **Checked:** `crates/metadata-tikv/Cargo.toml:19-20` (TiKV tree behind an
  off-by-default feature) and `crates/metadata-tikv/tests/conformance.rs:24-31`
  (endpoint probe → early return before any TiKV code).
  **Test:** the named conformance test skips → 1 passed; the dependency-free
  `keyspace` unit tests → 4 passed.

- **Claim:** CI can reach a TiKV via a throwaway single-node in `deploy/`.
  **Checked:** `deploy/tikv-single-node/docker-compose.yml` plus the
  `tikv-conformance` task at `xtask/src/main.rs:154` (brings the stack up, runs
  the suite with `--features tikv`, tears down).
  **Test (manual):** `docker compose -f deploy/tikv-single-node/docker-compose.yml up -d`
  then `cargo xtask tikv-conformance`.

- **Claim:** the audited (default, shipped) build is `cargo deny`-clean; the full
  tikv-client tree's audit is a documented ADR-0003 deferral (see the note below).
  **Checked:** `Cargo.toml` (the `tikv-client` pin) + `crates/metadata-tikv/Cargo.toml`
  (the tree behind the off-by-default `tikv` feature).
  **Test:** `cargo deny check` (default features — the shipped graph) → advisories,
  bans, licenses, sources all ok.

Rigorous write-conflict/CAS semantics, native paged scans, the backend
selector, and the production deploy tier are intentionally out of scope for
this slice and tracked as follow-ups.

## Dependency audit (ADR-0003) — feature-gated deferral

`tikv-client 0.4.0` and its transitive tree sit behind the **off-by-default `tikv`
feature**, so they are **not in the default build/ship graph** — which is exactly what
`cargo xtask ci`'s `cargo deny check` audits, and it is clean. Per ADR-0003 we audit the
graph Wyrd actually builds and ships; on that graph this slice is clean, and it ships
nothing from the tikv-client tree.

For completeness, `cargo deny --all-features check` (the tree is only pulled in once a
future slice enables the backend) currently reports items that are all **upstream in
`tikv-client 0.4.0`'s pinned TLS stack** (tonic 0.10 → rustls 0.21 → `rustls-webpki 0.101.7`):

- **RUSTSEC-2026-0099** / **RUSTSEC-2026-0104** — `rustls-webpki 0.101.7` (wildcard
  name-constraint bypass; reachable panic in CRL parsing). A fixed `rustls-webpki
  0.103.13` is already elsewhere in the tree, but tikv-client 0.4.0 pins the old one.
- an **ISC**-licensed crate not yet on the `deny.toml` allowlist (ISC is permissive).

**Decision (M4.1):** these are **deferred to M4.2** — the slice that enables the `tikv`
backend and actually ships the tree. There the full tree enters the audited graph
(`cargo deny --all-features`) and the RUSTSEC advisories + ISC allowlist are resolved
(an upstream `tikv-client`/TLS bump, or a documented `[advisories] ignore` + `deny.toml`
license entry). This slice ships nothing from that tree, so the shipped graph stays clean.

Fixes [#252](https://github.com/getwyrd/wyrd/issues/252)
