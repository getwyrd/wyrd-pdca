# Build notes — issue 405 / networked-client-observable

## Target base

Worked directly in `$PDCA_WORKTREE` (`/home/eddie/wyrd/wyrd.pdca-wt`). Its `HEAD`
(`3f5a8ee6cca728ce7eec87b57e0580b9cabd8bbd`) is bit-identical to
`origin/feat/m4-production-metadata-backend` (verified: `git rev-parse HEAD` ==
`git rev-parse origin/feat/m4-production-metadata-backend` == `3f5a8ee…`) — the
worktree's local branch happened to be named `feat/m4.5-deploy-tikv-pd-etcd`
locally, but it points at the exact tip the brief's target names, so `patch.diff`
(a plain `git diff` against that `HEAD`) applies cleanly to the brief's stated base.
`./engine/scripts/run-verify.sh` independently confirms this — it resolves the base
from the brief itself (`origin/feat/m4-production-metadata-backend`) and applied the
patch there cleanly.

## What I built

- `crates/server/src/consistency_observable.rs` (new): `ObservableS3Client` — a
  reusable, networked S3 client that drives signed PUT/GET/DELETE over a fresh
  `TcpStream` against a live S3 HTTP wire listener and records a `History` of
  `OpRecord`s (op kind, key, observed register version, HTTP status, `[start, end]`
  real-time span).
- `crates/server/src/lib.rs:17` — `pub mod consistency_observable;` (one line,
  alphabetically inserted between `cli` and `custodian`).
- `crates/server/tests/consistency_observable.rs` (new, the brief's named test):
  starts the loopback gateway exactly as `s3_http_wire.rs::start_gateway_with_handle`
  does (`s3_http_wire.rs:70-84`), drives PUT v1→v2→v3 with an interleaved GET after
  each commit, then DELETE + a post-delete GET, and asserts the recorded history is
  non-vacuous (8 ops, the right op-kind mix), carries the exact expected per-op
  version at every position, is well-formed (`start <= end` everywhere), and is
  version-monotone per key (no stale/torn reads).
- No `Cargo.toml` change needed: `tokio` and `wyrd-gateway-s3` are already plain
  (non-dev) dependencies of `wyrd-server` (`crates/server/Cargo.toml:66-67,84`), so
  the observable module can use `tokio::net::TcpStream` and
  `wyrd_gateway_s3::sigv4::{sign, format_amz_date, Credentials}` without any new dep
  line — narrower than the brief's difficulty note anticipated ("likely a Cargo.toml
  dep line").

## Design decisions and why

**Where the module lives.** The brief left this to Do, ruling only out
`crates/testkit` (no async/HTTP deps there). I put it in `crates/server` (not a new
crate) because: (a) it is a small, single-purpose harness client with no reuse need
outside the `server`/`gateway-s3` composition it drives; (b) `server` already depends
on `gateway-s3` + the concrete backends the loopback gateway needs, so no new
cross-crate wiring; (c) it sits as a peer module to the crate that hosts the wire
test it drives (`s3_http_wire.rs`), matching the brief's steer. A new crate was
rejected on cost: it would need its own `Cargo.toml` (gateway-s3 + tokio deps),
workspace-member registration, and would still have to re-expose `wyrd_server::Gateway`
type wiring for the test's `start_gateway` — a needless crate boundary for one
~300-line client with a single consumer (the #329 harness, itself downstream of this
same crate family). Concretely: one new `[dependencies]` block duplicating two lines
already in `server/Cargo.toml`, one new `Cargo.toml` file, one workspace-members line,
vs. the one `pub mod` line actually shipped.

**How "register version" is carried.** ADR-0041 decision 1 models the register as the
inode `version` bumped by `commit_overwrite`/`commit_chunk_map` — but the S3 wire
floor (`crates/gateway-s3/src/lib.rs:40`, issue #364 scope) returns no version/ETag
header; a GET returns only bytes + status. I considered three options:
1. **Add a response header/verb exposing the raw inode version.** Rejected: this is
   a wire-surface change beyond PUT/GET/DELETE, and the brief's scope section is
   explicit that wire-verb work (list/rename) is a *later* #364 follow-on, not this
   slice — extending that same reasoning, inventing a *new* response header is also
   wire-surface surgery this brief did not scope in, and touches
   `crates/gateway-s3/src/lib.rs`'s GET handler (`lib.rs:319-338`) plus its own
   conformance tests — a materially bigger, differently-shaped change than "add a
   client".
2. **Read the version out-of-band via the in-process `Gateway` handle** (as
   `s3_http_wire.rs::start_gateway_with_handle` lets its test do for the percent-
   encoding assertion). Rejected: that is exactly the "in-process" shortcut the
   brief is building the observable to get *away* from — a "networked observable"
   that peeks at backend internals out-of-band is not itself driving the register
   through the wire, and could not be pointed at a real deployed cluster later.
3. **(Shipped) Encode the version as the object's own bytes** — the client writes
   the decimal tag as the PUT body and decodes it back on GET. This needed **zero**
   production-code changes outside the new client, keeps every op genuinely over the
   wire, and is a standard Jepsen/Elle-style register-client technique (the value
   *is* the version). The trade­off: the client cannot observe the raw backend
   counter (e.g., it can't tell "this GET saw version 7" if some other, unmodeled
   writer bumped it) — acceptable because this slice's workload is exactly the
   single-observable-client sequence the brief specifies (v1→v2→v3 with interleaved
   reads); the multi-writer/nemesis case is explicitly the *next* #329 slice on
   #257's cluster, not this one.

**Sequential, not concurrent, workload.** The brief's Falsifiability section is
explicit that no partition nemesis or concurrent-writer race is needed to falsify
this slice ("the client observable's own correctness is a register history over the
single-node loopback gateway"); Scope also excludes "wiring the observable into a
live nemesis run." A sequential v1→v2→v3 + interleaved-read workload against a real
network listener is the minimal drive that produces the "non-vacuous register
history" the Success criterion names, without building the concurrent-race harness
that's explicitly a later slice.

## Alternatives ruled out

- **A brand-new `crates/consistency-observable` crate.** Cost shown above (new
  `Cargo.toml` + workspace registration vs. one `pub mod` line) — rejected as
  unnecessary crate-boundary overhead for a single-consumer harness client.
- **A wire-level version/ETag verb.** Cost: touches `gateway-s3`'s handler and adds
  a new response header contract with its own conformance tests — a materially
  larger, differently-scoped change (see design decision above); rejected in favor
  of the value-carries-the-version technique that needs no wire change.
- **Peeking at the in-process `Gateway` handle for the "real" version** (as
  `s3_http_wire.rs`'s handle-returning `start_gateway_with_handle` variant would
  allow). Rejected: defeats the entire point of a *networked* observable (ADR-0041's
  stated need — "there is no client-facing object API today… the harness needs the
  S3 HTTP wire surface… to drive… with client-observed real-time order").

## Demonstrated red (recording is load-bearing, not inert scaffolding)

Per the brief's Verification posture, I deliberately broke the recording twice,
confirmed the shipped assertions catch each break, then restored:

1. **Dropped the version read** (`consistency_observable.rs`, `get()`): recorded
   `version: None` unconditionally instead of the decoded value. Result: **FAILED**
   — `history.ops()` version-sequence assertion in the test
   (`consistency_observable.rs:135` in the shipped test) caught it immediately
   (`left: [Some(1), None, Some(2), None, …]` vs `right: [Some(1), Some(1), …]`).
   This is why the test asserts the *exact* recorded per-op version sequence, not
   only monotonicity — an earlier draft that checked only
   `versions_monotone_per_key()` stayed **green** under this same break (monotone
   trivially holds when every version is `None`), which is precisely the "green
   mechanical check on something adjacent" failure mode to avoid; I hardened the
   test once I found that gap.
2. **Reversed the PUT's timestamp span** (`end = start - 1s`): Result: **FAILED** —
   `history.well_formed()` caught it (`"every recorded op must have a
   non-reversed start<=end real-time span"`).

Both breaks were reverted (confirmed via `git diff` showing no residual changes)
and the suite re-verified green before shipping.

## Refutation (forced questions)

**(a) Genuine red?** Yes. `./engine/scripts/run-verify.sh` (run from
`wyrd-pdca`, `PDCA_BUNDLE=results/issue_405`) applies `patch.diff` to a clean
`../wyrd-verify` worktree at the brief's base, runs green, then reverts the
production file (`lib.rs`'s `pub mod` line — the only modified, non-added file)
and re-runs: **RED**, `error[E0432]: unresolved import
'wyrd_server::consistency_observable'`, because `run-verify.sh` deletes the
patch's *added* non-test file (the module itself) on the RED leg and reverts the
modified file, exactly reproducing "no observable type to construct" the brief's
Falsifiability section describes. Full transcript: `PASS — red without the fix,
green with it.`

**(b) Production path?** Yes. The test drives the real `wyrd_gateway_s3::S3Gateway`
over a real `tokio::net::TcpListener` on loopback, backed by the real
`RedbMetadataStore` + `FsChunkStore` + `MemCoordination` composed the same way
`s3_http_wire.rs::start_gateway_with_handle` does (`s3_http_wire.rs:70-84`) — the
identical production write/read/delete path (`Gateway::put_object_streaming` /
`get_object_streaming` / `delete_object`, `crates/server/src/lib.rs`) that backs
every other wire test in that file. `ObservableS3Client` is not a mock of the wire
protocol — it signs with the production `wyrd_gateway_s3::sigv4::sign` and speaks
real HTTP/1.1 framing over the real socket; nothing in the observable or the test
stands in for the gateway.

**(c) Fixture includes the fault?** Yes, for what this slice is scoped to falsify.
The brief's Falsifiability section is explicit that the fault this slice must
exhibit is "no client type and no recorded history" (pre-fix) / "a broken observable
(missing/reversed timestamps, non-monotone version, a vacuous single-op history)" —
not a partition/replica fault (that's #257's cluster, a later slice, explicitly
out of scope here). The fixture is the real in-process loopback gateway named in
Falsifiability (redb + fs + mem behind the HTTP listener) — nothing is curated out
of it; the two demonstrated-red edits above are exactly instances of the "broken
observable" fault class the brief names, and both were caught.

## Test runner used

`./engine/scripts/run-verify.sh` (the `C4-verify` gate, `pdca.toml:626`) — the
project's own per-fix red→green wrapper, run as
`PDCA_BUNDLE=results/issue_405 ./engine/scripts/run-verify.sh` from the
`wyrd-pdca` root. No hand-rolled runner was used. Development-time iteration used
plain `cargo test -p wyrd-server --test consistency_observable` /
`cargo test -p wyrd-server --lib consistency_observable` inside `$PDCA_WORKTREE`
(not the official red/green claim — just compile/debug feedback), and
`cargo fmt --all -- --check` plus `cargo clippy --workspace --exclude wyrd-dst
--all-targets -- -D warnings` (the exact invocation `xtask/src/main.rs`'s
`run_ci_steps` uses) confirm the patch is commit-ready: both are clean on the
touched files and at workspace scope. (An unrelated pre-existing clippy finding in
`crates/server/src/cli.rs:139,201` only surfaces under `--all-features`, which
`cargo xtask ci`/`run_ci_steps` never passes — confirmed present identically on
the unmodified base via `git stash`, so it is not something this patch introduces
or must fix.)

## Headless / GUI concerns

None apply. This is a plain async Rust integration test over a loopback TCP
socket (`tokio::test`) — no GUI/display dependency, nothing pulled in at load
time beyond the existing `tokio`/`axum`/`hyper` stack the `s3_http_wire.rs` peer
test already exercises in this same (headless) runner.

## External dependencies

None beyond the base Rust toolchain, matching the brief's "External dependencies:
none" — no Docker, no TiKV, no JVM/Elle. Confirmed by `cargo test` running clean
with no `WYRD_TIKV_TOOLCHAIN` / feature flags set.

## STOP discipline

No PR opened, no `gh pr` command run. Draft-only per the brief; sign-off is the
human's step.
