# FoundationDB backend: fail closed on a client/cluster version mismatch

## Summary
**User impact:** An operator running Wyrd against a FoundationDB cluster whose
version does not match the client library got no honest signal that the two were
mismatched. FoundationDB requires the client and the cluster to speak the exact
same wire protocol — a client built for one version cannot talk to a cluster
running another, ever — and a version mismatch is the most common way to
misconfigure it. Yet a mismatched client failed with the same generic
"transaction timed out" that a cluster which is simply down or unreachable
produces. Two very different problems looked identical, sending operators to
debug the wrong one.

This PR makes the FoundationDB backend check readiness when it connects: on a
version mismatch it fails right away with a message that names the cluster's
protocol version and how to fix it, instead of the anonymous timeout. It also
adds a knob for the multi-version-client upgrade path and writes down the
packaging contract for the backend.

Reported in #441.

## What to look at
The change is a startup readiness probe that runs on the operator path (`wyrd
… --metadata-backend fdb`) and nowhere else. Reviewers who want to exercise it:
stand up a FoundationDB cluster on an older version than the client was built
against (e.g. `foundationdb/foundationdb:7.1.61` against a 7.3 client), point a
cluster file at it, and run a `put`. It fails in ~200 ms with exit 1 and a
message containing `client/cluster protocol version mismatch`, the cluster's
protocol version, and a pointer to the upgrade procedure — where before it took
the full transaction timeout and said only that the transaction timed out. The
default (`redb`) build is untouched; nothing here compiles unless you build
`--features fdb`.

## Root cause
FoundationDB's version coupling is absolute, but Wyrd never checked it: the first
real transaction was where a mismatched client discovered the cluster, and by
then every failure — mismatch, cluster down, network partition — surfaced as the
same bounded-but-anonymous timeout. The client also had no way to load additional
`libfdb_c` versions, so bridging a lockstep cluster upgrade was impossible.

## Fix
A new pure, always-compiled `preflight` module classifies a client-status probe
into `Ready` / `VersionSkew` / `Unreachable`, and the production constructor
`connect()` runs that probe (bounded by the same transaction deadline) before
returning success, translating a non-ready verdict into an actionable error. The
discriminator is `Compatible == false` on a connection that is `"connected"`;
anything the probe cannot positively call a mismatch degrades to "unreachable"
*with* a version-coupling hint, never a guessed mismatch. `connect()` and the
probe are `async` and await on the caller's runtime — the shape the TiKV backend
already uses — so no invocation spawns a nested runtime. A new
`WYRD_FDB_EXTERNAL_CLIENT_DIR` feeds FoundationDB's multi-version-client
directory into the network boot for lockstep upgrades; unset, behaviour is
unchanged. The deployment guide gains a packaging-and-version-coupling section.

## Verification
- **Claim:** The version-skew classifier is a pure, non-feature-gated module,
  and it maps the three real client-status shapes (skew / unreachable / healthy)
  to the right verdict with an actionable message.
  - **Checked:** `crates/metadata-fdb/src/lib.rs` — the `preflight` module
    (`verdict`/`message`, `ClientStatus`, `Verdict`) takes no FoundationDB type,
    so it compiles and its tests run on the default build.
  - **Test:** `crates/metadata-fdb/tests/preflight.rs` — its own integration
    binary, so `cargo test -p wyrd-metadata-fdb --test preflight` runs it on the
    default build (no `fdb` feature, no `libfdb_c`, no Docker). Fails pre-fix
    (the `preflight` import does not resolve), passes post-fix; 6 tests green.
    Negating the skew arm turns the skew and fail-honest cases red, confirming
    the classifier is load-bearing, not decorative.
- **Claim:** The probe sits on the real operator path and does not spawn a nested
  runtime, so `wyrd --metadata-backend fdb` does not panic.
  - **Checked:** `crates/server/src/cli.rs` — `open_fdb_meta` is now `async` and
    `.await`ed at all seven call sites; `crates/metadata-fdb/src/lib.rs` —
    `connect()` awaits `preflight()` on the caller's runtime.
  - **Test:** `crates/metadata-fdb/tests/timeout.rs` drives the production probe
    against an unreachable coordinator from inside a Tokio runtime and asserts it
    returns an "unreachable" error rather than panicking or claiming a mismatch;
    `crates/metadata-fdb/tests/conformance.rs` drives the whole `connect()`
    entry point against a live matched cluster and asserts `Ready`. Both run
    under `cargo xtask fdb-conformance`. Reintroducing the nested-runtime shape
    turns the timeout case red with the exact "Cannot start a runtime from within
    a runtime" panic.
- **Claim:** The mismatched cluster produces the guided error end to end; a
  genuinely unreachable cluster does not get a false mismatch; a healthy cluster
  is not rejected.
  - **Checked:** `docs/design/architecture/07-deployment-view.md` §7.6 records
    the manual repro and its observed output.
  - **Test:** Run manually on a `libfdb_c` 7.3.77 host against a live
    `foundationdb/foundationdb:7.1.61` cluster: connect failed in ~203 ms, exit
    1, message naming protocol `fdb00b071010000` and the upgrade path (vs. a
    10 s anonymous timeout before). An unreachable coordinator reported
    "unreachable … rather than a guessed version skew"; a matched 7.3.77 cluster
    completed a `put`/`get` round trip in ~304 ms. The gating `cargo xtask ci`
    passes with the `fdb` feature off.

Fixes #441
