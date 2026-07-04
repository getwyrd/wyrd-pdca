# server: select the metadata backend (redb / TiKV) by configuration

## Summary

An operator running the `wyrd` server had no supported way to point its
metadata at a production TiKV cluster — the binary always used the embedded
redb store, fixed at build time — and a persistent metadata write-conflict
could make inode allocation spin forever instead of failing cleanly. This
change lets the metadata backend be chosen by configuration
(`--metadata-backend redb|tikv`, or `WYRD_METADATA_BACKEND`), defaulting to
redb so existing dev/single-binary usage is unchanged.

## What to look at

All of the change is in `crates/server` (plus its `Cargo.toml` / `Cargo.lock`
edge); `core`, `custodian`, and the storage trait are untouched.

- `crates/server/src/cli.rs` — the new `MetadataBackend` selector and the store
  helpers (`alloc_inode`, the local/cluster put+get paths), now generic over the
  `MetadataStore` trait so both backends run the same code.
- To exercise it: `wyrd put`/`wyrd get` default to redb exactly as before;
  passing `--metadata-backend tikv` (or an unknown name) is validated up front.
  The regression test `crates/server/tests/backend_selection.rs` drives the redb
  path and the retry bound directly.

## Root cause

`server` was the crate that named a concrete metadata store, and it named only
redb: the CLI helpers took a `RedbMetadataStore` by type and the local paths ran
on a blocking executor that a tokio-bound TiKV client cannot use. Separately,
`alloc_inode` looped without bound on a `Conflict` commit — harmless over
sub-microsecond embedded redb, but a latency/load footgun over a distributed
store where each retry is a network round-trip.

## Fix

- **Backend by config, not by build.** A `MetadataBackend { Redb, Tikv }`
  selector resolves from the `--metadata-backend` flag or `WYRD_METADATA_BACKEND`
  (absent ⇒ redb), and dispatches at each store call site.
- **One code path for both backends.** The store helpers are parameterized over
  `M: MetadataStore`, so redb (dev) and TiKV (prod) run identical composition —
  selection is "pass a different concrete", not a refactor of any consumer.
- **Shared runtime.** The local-disk paths now use the same tokio runtime the
  cluster paths already used, so an async TiKV client can run there.
- **Bounded allocation.** `alloc_inode` retries a lost race with capped
  exponential backoff and returns a typed exhaustion error after a fixed number
  of attempts instead of spinning.
- TiKV sits behind an **off-by-default `tikv` build feature**, so the default
  build and its dependency audit are unchanged; redb stays the default backend.

## Verification

- **Claim:** the metadata backend is selected by configuration, defaulting to
  redb, and an unknown name is rejected.
  - **Checked:** `crates/server/src/cli.rs:67-98` — the `MetadataBackend`
    selector and `from_config` (absent/`redb` ⇒ redb; unknown ⇒ config error).
  - **Test:** `crates/server/tests/backend_selection.rs:28` — asserts the default
    and explicit-`redb` selection and rejects `"nonsense"`.

- **Claim:** both backends run the identical store path — the swap is
  composition, not a refactor.
  - **Checked:** `crates/server/src/cli.rs:515` — `alloc_inode` and the local /
    cluster helpers are generic over `MetadataStore`; existing redb callers infer
    the same concrete unchanged. `crates/server/src/cli.rs:676` — the local paths
    now share the cluster paths' tokio runtime.

- **Claim:** a persistent metadata conflict fails cleanly instead of spinning.
  - **Checked:** `crates/server/src/cli.rs:515-542` — bounded retry-with-backoff
    returning a typed error after a fixed attempt count.
  - **Test:** `crates/server/tests/backend_selection.rs:77-85` — `alloc_inode`
    against an always-`Conflict` store returns a bounded error inside a 5s
    timeout. Pre-fix this hangs (the unbounded loop) and the generic mock does not
    even compile; post-fix both assertions pass. `cargo xtask ci`
    (fmt/clippy/build/test/deny/conformance) is green.

> Note for reviewers: the TiKV selection arm is compiled out of the default
> build (the off-by-default `tikv` feature) and is exercised on demand via
> `cargo xtask tikv-conformance` against a running cluster, not in the default
> CI run.

Fixes [#255](https://github.com/getwyrd/wyrd/issues/255)
