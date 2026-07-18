# Design proposal — issue 577 / typed-transient-terminal-errors

> Plan artifact (design-proposal form — the change is at the workspace's central trait
> seam, `crates/traits`, and every backend and consumer sees it). The design itself is
> already ratified in proposal 0010 (`docs/design/proposals/draft/0010-observability-floor-for-first-deployment.md`,
> §"Scope boundary" item 6); this brief concretizes it for one Do cycle. Do reads ONLY
> this file (plus the cited peer callsites).

- **Slug:** typed-transient-terminal-errors
- **Kind:** enhancement (design proposal)
- **Goal:** failure *class* — transient ("try again") vs terminal ("retry cannot help") —
  survives the `crates/traits` seam and the chunkstore gRPC seam, instead of dying into an
  untyped `BoxError` string. A typed error enum (or seam-crate classifier over typed error
  structs) extends the existing `IntegrityFault` precedent.
- **Success criterion:** with the patch applied, (a) a transient fault and a terminal fault
  raised behind the `crates/traits` seam each classify through a seam-crate classifier that
  returns a **public class value with a stable, bounded label form** (an enum-like
  `ErrorClass` — NOT merely boolean predicates in the `is_integrity_fault` shape,
  `crates/traits/src/lib.rs:123`: issue 575's error counter consumes the value's label
  form, so a bool cannot satisfy this), with `IntegrityFault` remaining a distinct
  *terminal* class; and (b) the class **survives the chunkstore gRPC seam** — a transient
  fault and a terminal fault, each produced by a **real default-compiled producer** (e.g.
  terminal: a genuine `FsChunkStore` integrity fault; transient: a genuine transport-level
  unreachable/timed-out failure), NOT by a test double injecting the new types directly,
  reconstruct to the correct class client-side (the `IntegrityFault`-over-`DATA_LOSS`
  precedent, `crates/chunkstore-grpc/src/server.rs:81-94` / `client.rs:23-36`). Asserted by
  the named test at Check (C4-verify red→green); the whole-tree `cargo xtask ci` stays green.
- **Falsifiability:** the added test `crates/chunkstore-grpc/tests/error_class.rs` runs
  in-process over a real loopback tonic connection (peer:
  `crates/chunkstore-grpc/tests/round_trip.rs` — plain `#[tokio::test]`, no cfg gate, so it
  executes under the gate's bare `cargo test` invocation). RED is real on the base
  toolchain: with the production change reverted, a transient fault crossing the wire
  collapses to an unclassifiable transport string (only the `DATA_LOSS`/`IntegrityFault`
  mapping exists today), so the classification assertions fail — and where the test
  references the new seam types, the RED leg fails to compile, which the gate also counts
  as red. No special environment is needed for the binding criterion.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Ordering note:** wave 1 of this batch. 575 (request-plane RED) declares `Depends on:
  577` — its error-by-class counter keys on THIS enum (0010 item 4). Keep this bundle's
  file set off `crates/gateway-s3` and `crates/server/src/{cli,dserver}.rs` (575/576 own
  those); the counter keying is 575's job, not this one's.
- **Surfaces:** data
- **Difficulty:** high — the seam crate plus every producer/consumer: `crates/traits`,
  `crates/core` (the read path's private `FaultClass`, `crates/core/src/read.rs:140`,
  already draws this distinction locally — align/subsume it), `crates/chunkstore-grpc`
  (both directions of the status mapping), `crates/chunkstore-fs`, `crates/metadata-redb`,
  and the feature-gated `metadata-tikv`/`metadata-fdb`. Wide cross-file reach.
- **Scope:** the additive typed transient/terminal classification at `crates/traits`, its
  producers at the default-compiled backends (chunkstore-fs, chunkstore-grpc mapping;
  metadata-redb is embedded and has no transient class to produce — verify its faults
  classify terminal, a supplementary unit test suffices), the gRPC round-trip of the
  class, adaptation of the
  feature-gated TiKV and FDB `MetadataStore` backends to produce the new variants
  (compile-verified feature-on, see Verification posture), and **recording the ratified
  sequencing decision in proposal 0010** (see Design). / out of scope: the request-plane
  error *counter* keyed by these classes (issue 575), health probes (issue 576), any
  change to the gRPC wire proto (classes ride existing status-code conventions per 0010),
  any behaviour change to retry/repair logic (classification only — consumers may *read*
  the class; changing what they *do* with it is later work).
- **Repro instruction:** on `main` (e47cb88), observe that the only typed error classes at
  the seam are `IntegrityFault` (`crates/traits/src/lib.rs:98`), `BlockReadFault` (`:164`),
  `CommitUnknownResult` (`:231`), `ScanCapExceeded` (`:304`) — four *specific* faults
  (three retry-cannot-help ones plus the deliberately *indeterminate*
  `CommitUnknownResult`, which is neither transient nor terminal and must NOT be forced
  into the binary partition); there is no transient/terminal partition and no classifier
  for it, and
  `crates/chunkstore-grpc/src/client.rs:23-36` reconstructs only the `DATA_LOSS` →
  `IntegrityFault` case. A timeout/unreachable fault arrives as an opaque string.
- **External dependencies:** `openssl dev (pkg-config + libssl)` (build: the feature-on tikv compile pulls openssl-sys), `libfdb_c loadable` (build: the feature-on fdb compile links the FDB C client), `fdb headers (bindgen)` (build: foundationdb-sys bindgens fdb_c.h), `docker` (runtime: the optional off-Check live conformance legs only) — all four already registered doctor rows; the binding at-Check criterion needs none of them, the base toolchain suffices.
- **Test file:** crates/chunkstore-grpc/tests/error_class.rs — a NEW file (the C4-verify
  gate classifies its red leg on an *added* `*/tests/*.rs` file; an appended or co-located
  test degrades to green-only). Additional unit tests in `crates/traits` /
  feature-gated backend modules are welcome but supplementary.
- **Verification posture:** default red→green at Check for the binding criterion (seam
  classification + gRPC survival — all default-compiled code). ONE pre-declared deferred
  item: the TiKV and FDB backend adaptations live behind the OFF-by-default `tikv` / `fdb`
  features (`crates/metadata-tikv/Cargo.toml:11-20`; same pattern in `metadata-fdb`), which
  **no Check gate compiles** — `cargo xtask ci` builds them as skeletons. What IS built and
  exercised at Check: the seam types, classifiers, gRPC mapping, and default-backend
  producers, via the named test. What is deferred: the feature-on backends' variant
  production. Do MUST still (i) compile them feature-on — `cargo check -p wyrd-metadata-tikv
  --features tikv` and `cargo check -p wyrd-metadata-fdb --features fdb` (the features are
  per-package; the fdb check needs libfdb_c on the host — declare, don't silently
  skip, if it is absent) — and (ii) ship feature-gated unit tests over the classification
  mapping (the `#[cfg(feature = …)]` module pattern already used in those crates), so the
  deferred deliverable is built and exercised by something, not inert scaffolding. The
  deferred green is confirmed by the human running the tikv-/fdb-conformance legs off-Check.
- **Citations expected:** Do must cite path:line on `main` for every change. Peer
  callsites Do MAY open: the wire mapping to mirror in both directions —
  `crates/chunkstore-grpc/src/server.rs:81-94` (fault → status) and
  `crates/chunkstore-grpc/src/client.rs:23-36` (status → typed fault); the seam-crate
  classifier shape — `crates/traits/src/lib.rs:123` (`is_integrity_fault`); the
  in-process loopback test shape — `crates/chunkstore-grpc/tests/round_trip.rs`.
- **Prior-art check (by affected file path):** merged: #515 gave the undetermined-commit
  class a typed seam-crate error (`CommitUnknownResult`, `crates/traits/src/lib.rs:231`) —
  the narrower precedent this generalizes, not a duplicate; `crates/core/src/read.rs:140`
  carries a *private* `FaultClass` (incl. a `Transient` arm) on the read path — prior art
  to align with, not duplicate. Open PRs: only getwyrd/wyrd#578 (issue 409, consistency-CI
  files — disjoint file set). No merged/closed work implements the seam-wide enum.
- **Disposition hint:** new-feature

## Motivation

Callers, retry logic, and operators cannot tell "try again" from "data is gone / config is
wrong": everything crossing the trait seam except the four typed specifics is a `BoxError`
string (proposal 0010 §Motivation, "Errors are opaque"). 0010 item 6 names this the last
piece of "why did this request fail". Sequencing was the one M4 collision; it is resolved
(see Design) and M4 is complete (getwyrd/wyrd PR #489 merged), so this lands now, adapting
the TiKV backend to the new variants.

## Design

Per proposal 0010 §"Scope boundary" item 6 and §"Crate touch-points": an **additive** typed
transient/terminal classification at `crates/traits`, extending the `IntegrityFault`
precedent — transient covers unreachable / timed-out / busy; terminal covers integrity
faults, permanent store errors, invalid config. `IntegrityFault` stays a distinct terminal
class. Existing `BoxError` callers keep compiling (additive; classification via
`downcast_ref` chain-walk like `is_integrity_fault`). The class rides existing gRPC status
conventions (no proto change), reconstructed client-side as `DATA_LOSS`→`IntegrityFault`
already is. Whether concrete errors stay typed structs or become one `enum` is Do's
mechanism call — but the **exported contract is fixed** (575 consumes it): a public class
*value* (an enum-like `ErrorClass`) with a stable, bounded label form (e.g. a
`&'static str` snake-case name), returned by one seam-crate classify function. Boolean
predicates alone do not satisfy it.

**Class mapping of the existing typed faults (explicit, so Do does not guess):**
`IntegrityFault` and `BlockReadFault` → terminal, with integrity distinct;
`ScanCapExceeded` → terminal; `CommitUnknownResult` → **indeterminate** — it is
deliberately neither transient nor terminal (`crates/traits/src/lib.rs:205-229`: retrying
is forbidden, yet the write may still land), so the classification must carry it as its
own outcome, never collapse it into the binary. Unclassifiable errors default to
**terminal** (fail-safe: retry logic must act only on *known-transient* signals — a
default-transient would convert every unknown fault into a retry storm). The invariant is
*class survives both seams*.

**Sequencing record (ratified — do not re-litigate):** the #366 keystone sign-off
(2026-07-04, PDCA bundle §10 / iteration-7 ratification) determined #255 (TiKV backend)
merged first and the typed-error enum lands AFTER M4, adapting the TiKV backend. 0010's
graduation criteria require this recorded in 0010/an ADR; it is not yet (0010's
§"Sequencing note" still poses it as open — verified on e47cb88). Append the decision to
0010's sequencing note as part of this change. 0010 is a *draft proposal* (not an Accepted
ADR — no immutability conflict), but any proposal edit is a **project-defined human-only
item** (INTEGRATION §4): pre-declared here as an expected NEEDS-HUMAN sign-off row, not a
surprise.

## Alternatives considered

Weighed in 0010 (deferring to a later cleanup was rejected — "why did it fail" is a
day-one operator question; the gRPC-status-code ride was chosen over a proto change to keep
the wire additive). Not re-litigated here.

## Impact & compatibility

Additive on a pre-1.0 internal seam; no published API, no on-disk format, no consistency
contract change (0010 §"What carries over, unchanged"). No new dependency. Existing
`BoxError` paths keep compiling.

## Open questions

None blocking. Enum vs classifier-structs shape is Do's; the 0010 edit is pre-declared
NEEDS-HUMAN.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.
