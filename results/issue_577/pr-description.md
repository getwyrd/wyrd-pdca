# PR description

## Summary
**User impact:** When a read or write against the store failed, the caller could not
tell whether trying again might help. A node that was merely unreachable, slow, or
busy looked exactly like data that was gone or a configuration that was wrong —
everything came back as an opaque error string — so retry logic either gave up on a
healthy-but-busy cluster or hammered failures no retry could ever fix, and an
operator asking "why did this request fail" could not get even the most basic
answer: is this temporary or permanent?

This PR gives every failure crossing the storage seams a typed class — transient
("try again"), terminal ("retry cannot help"), integrity (corruption: terminal, and
carrying a repair obligation of its own), or indeterminate (an unknown commit
outcome: must not be retried, yet the write may have landed) — and makes that class
survive the network hop between a client and the chunk server, instead of dying
into a string at the first boundary.

Implements item 6 of the observability-floor proposal (0010); issue #577.

## What to look at
The contract lives in one place and everything else produces or transports it:

1. `crates/traits` — the new class value with four stable labels, the one
   classification function, and the transient fault type that *wraps* a backend's
   own error rather than replacing it (nothing a backend already reported is lost);
2. `crates/chunkstore-grpc` — the two directions of the wire mapping: the server
   sends a store-named transient fault on an existing status code (no protocol
   change), and the client reconstructs the class from the status codes this stack
   actually produces;
3. the backends — TiKV and FoundationDB timeouts/retry-exhaustion now carry the
   transient class; the embedded redb backend has nothing transient to report, and
   that is asserted by test rather than assumed.

To try it: `cargo test -p wyrd-chunkstore-grpc --test error_class` — five tests
that corrupt a fragment on a real on-disk store, kill a real loopback gRPC server,
and let a request outlive its deadline, then assert the class the client gets back.

One deliberate call worth a critical read: unknown/unclassified errors default to
**terminal**, never transient — a retry policy may only act on a *known*-transient
signal, because defaulting the unknown to transient would turn every unrecognised
fault into a retry storm.

## Root cause
The trait seam had four *specific* typed faults (`IntegrityFault`,
`BlockReadFault`, `CommitUnknownResult`, `ScanCapExceeded` — `crates/traits/src/lib.rs:98,164,231,304`
on the pre-change `main`) but no general transient/terminal partition and no
classifier over it, so every other failure crossed the seam as an untyped
`BoxError` string. Over the gRPC seam the client reconstructed exactly one class
from the wire — `DATA_LOSS` → `IntegrityFault`
(`crates/chunkstore-grpc/src/client.rs:23-36` pre-change) — so a transport-level
timeout or unreachable-node failure collapsed to an unclassifiable string.

## Fix
- **`crates/traits/src/lib.rs`**: `ErrorClass { Transient, Terminal, Integrity,
  Indeterminate }` — a value with `as_str()` (stable single-word labels) and `ALL`
  (the closed label space issue #575's error counter pre-registers series from);
  `classify()`, a source-chain walker generalizing `is_integrity_fault`
  (`lib.rs:123` pre-change); and `TransientFault`, which keeps the backend's own
  error reachable via `source()`. The full mapping table is in the `classify()`
  doc, and every row — including the fail-safe terminal default — is pinned by a
  unit test. `Indeterminate` exists because `CommitUnknownResult` is genuinely
  neither: retrying is forbidden, yet the write may have landed.
- **`crates/chunkstore-grpc/src/server.rs:18-35`** (`transient_or_internal`): a
  store fault that classifies transient rides `UNAVAILABLE` — the same
  existing-status-convention trick `DATA_LOSS` plays for `IntegrityFault`;
  everything else stays `INTERNAL`, unchanged.
- **`crates/chunkstore-grpc/src/client.rs:20-101`** (`class_of`,
  `transport_error`, `dial_error`): `UNAVAILABLE | CANCELLED | DEADLINE_EXCEEDED |
  RESOURCE_EXHAUSTED` reconstruct as transient, applied at the `TransportError`
  choke point so every RPC benefits. `CANCELLED` is included deliberately: tonic
  renders an expired channel deadline as `Status::cancelled("Timeout expired")`
  (tonic-0.14.6 `src/status.rs:644-646`), not `DEADLINE_EXCEEDED` — the textbook
  exclusion would miss the timeout case entirely (a pre-existing false doc claim at
  `client.rs:74-78` saying otherwise is corrected in place). A failed *dial* is
  transient (unreachable); a malformed endpoint URI stays terminal (invalid
  config).
- **`crates/metadata-tikv/src/lib.rs`** / **`crates/metadata-fdb/src/lib.rs`**:
  `OperationTimedOut` and `RetryBudgetExhausted` each expose a synthetic
  `TransientFault` through their `source()` chain rather than being wrapped in one
  — wrapping would push them off the top of the box and silently break existing
  top-level downcasts, including `metadata-tikv/tests/deadline.rs:118`'s safety
  assertion that a timed-out *commit* is **not** an `OperationTimedOut` (it would
  have started passing vacuously). FDB's transient claim rests on FDB's own
  retryability predicate gating the retry loop, not on a guessed error-code
  taxonomy; the last `FdbError` stays reachable both as the public `last` field
  and through the chain.
- **`crates/metadata-redb/src/lib.rs`**: three unit tests asserting this embedded
  backend's faults all classify terminal — including a genuine redb-native open
  failure — so "redb produces no transient class" is a pinned property.
- **`crates/chunkstore-grpc/tests/round_trip.rs` / `crates/server/tests/dserver.rs`**:
  two test helpers that downcast the top-level error became source-chain walkers
  (the codebase's existing idiom), since a known-transient status now carries the
  class above the transport error. The d-server helper breaking was informative:
  it proves the class reaches the real shed/timeout path.
- **`docs/design/proposals/draft/0010-observability-floor-for-first-deployment.md`**:
  appends the ratified typed-errors/M4 sequencing decision (2026-07-04, the #366
  keystone sign-off) to the sequencing note and marks the open question resolved,
  as the proposal's graduation criteria require. A draft-proposal edit —
  maintainer authority applies.

## Verification
Citations are on the branch this PR targets (`main`); post-change line numbers are
as landed by this diff.

- **Claim:** a transient and a terminal fault raised behind the seam classify
  through one seam-crate function to a public class value with stable, bounded
  labels, `IntegrityFault` remaining distinct and `CommitUnknownResult` never
  collapsing into the binary.
  **Checked:** `crates/traits/src/lib.rs:421-540` (`ErrorClass`, `ALL`, `as_str`,
  `classify`) — nine unit tests pin every mapping row, label distinctness,
  Display≡as_str, the partition property, and the terminal fail-safe default
  (including a raw `io::Error`).
- **Claim:** the class survives the chunkstore gRPC seam from **real
  default-compiled producers**, not injected doubles.
  **Test:** `crates/chunkstore-grpc/tests/error_class.rs` (new) — five tests over
  a real loopback tonic connection hosting the real `FsChunkStore`: bytes rotted
  on the server's own disk reconstruct as `integrity`; a genuinely killed server
  (graceful shutdown of listener *and* connections — an aborted acceptor still
  answers, which the test documents) reconstructs as `transient`; a request
  outliving its channel deadline reconstructs as `transient`; the transport detail
  stays reachable under the class; and the two classes are mutually
  distinguishable with the exact labels (`integrity` / `transient`) issue #575
  keys on.
- **Claim:** red pre-fix, green post-fix.
  **Checked:** with the production change reverted the named test fails to compile
  (`unresolved imports wyrd_traits::classify, wyrd_traits::ErrorClass`); with only
  the gRPC client mapping reverted (seam types kept, so everything compiles), 4 of
  5 legs fail on genuine classification assertions (`left: Terminal, right:
  Transient`) — the test binds the wire survival, not the mere existence of a
  symbol. Green: 5/5 on the patched tree.
- **Claim:** existing consumers keep working — additive change, no trait signature
  touched, `BoxError` callers keep compiling, top-level downcasts intact.
  **Checked:** `crates/metadata-tikv/src/lib.rs` deadline tests (the typed error
  stays the top-level error a caller downcasts, and carries its class),
  `crates/metadata-fdb/src/lib.rs` store tests (exhausted retry budget classifies
  transient; an unknown commit result stays indeterminate; the last `FdbError`
  reachable by both routes).
- **Whole gate:** `cargo xtask ci` (fmt, clippy `-D warnings`, build, test,
  machete, deny, conformance, statics, DST) — all checks passed on the final tree.
- **Feature-gated backends:** `cargo check -p wyrd-metadata-fdb --features fdb`
  passes and its lib tests run green (40, including the two new class tests).
  `cargo check -p wyrd-metadata-tikv --features tikv` could **not** run on the
  build host (missing OpenSSL dev headers — declared, not worked around); the
  TiKV class production and its three tests live in the feature-free `deadline`
  module every `cargo xtask ci` compiles and runs, and the only changed line
  inside the gated module is a constructor call.
- **Known follow-ups, filed from review:** #580 — the server's outbound
  `UNAVAILABLE` arm has no test driving a store-raised transient fault through it
  (a supplementary double-driven loopback test); #581 — two doc corrections plus
  the `DEADLINE_EXCEEDED`/`RESOURCE_EXHAUSTED` client arms being pinned by no
  test; #582 — whether a DNS resolution failure on dial should classify transient
  or terminal (a policy call to settle before #575's retry policy consumes the
  class).

Fixes #577
