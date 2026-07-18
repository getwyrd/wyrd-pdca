# Build notes — issue 577 / typed-transient-terminal-errors

Target: `getwyrd/wyrd @ main`. Built in `$PDCA_WORKTREE`
(`/home/eddie/development/wyrd/wyrd.pdca-wt`), base `d1c3958` (the brief cites `e47cb88`;
`d1c3958` is the merge of #578, the disjoint consistency-CI file set the brief's prior-art
check already names — no overlap with this file set).

Planning artifact read: `docs/design/proposals/draft/0010-observability-floor-for-first-deployment.md`
(§"Scope boundary" item 6, §"Crate touch-points", §"Sequencing note", §"Graduation criteria").

---

## 1. The shape chosen, and why

The brief left the mechanism to Do but fixed the exported contract: a **public class value
with a stable, bounded label form**, one seam-crate classify function, `IntegrityFault`
still distinct, `CommitUnknownResult` never collapsed into the binary.

**Chosen:** a seam-crate typed error `TransientFault` + a value enum `ErrorClass` +
`classify()` (`crates/traits/src/lib.rs:355-540`).

- `ErrorClass{Transient, Terminal, Integrity, Indeterminate}` with `as_str()` (stable
  single-word snake labels), `ALL` (the bounded label space #575 pre-registers series
  from), `is_transient()`, `is_terminal()`. Not booleans — the brief rules those out
  explicitly because #575 keys a counter on the label form.
- `classify()` chain-walks `source()` exactly like `is_integrity_fault`
  (`crates/traits/src/lib.rs:123`), which it generalizes. Outermost recognised seam type
  wins.
- **The transient class needs a *type*, because that is the only thing that survives a
  `BoxError`.** There is no way for `crates/traits` to classify a `tonic::Status` or a
  `tikv_client::Error` — the dependency rule (ADR-0010) forbids the seam knowing a
  backend, and Rust cannot downcast `dyn Error` to a trait. So each producer puts a
  `TransientFault` in the chain. This is the `IntegrityFault`/`BlockReadFault` precedent,
  not a new idea.

### `classify()` has three arms, not six — deliberate

`BlockReadFault`, `ScanCapExceeded`, a raw `EIO` `io::Error` and "anything else" all map to
`Terminal`, which is the fail-safe default, so explicit arms for them would be code that
returns what the default already returns. I wrote the full mapping table in the doc comment
and **pinned every row with a unit test** (`the_permanent_faults_classify_terminal`,
`an_unclassifiable_error_defaults_to_terminal_not_transient`). The mapping is binding via
tests rather than redundant branches.

Fail-safe direction is the brief's and it is load-bearing: default-transient would turn
every unrecognised fault into a retry storm.

### Two nesting orders, chosen per-site — this is the crux of the diff

`TransientFault` can be **outside** the backend's error (wrap) or **inside** its `source()`
chain (the `BlockReadFault` synthetic-`io_source` trick, `traits/src/lib.rs:169-172`). The
choice is forced, site by site, by what consumers already downcast **at the top level**:

| site | order | forced by |
|---|---|---|
| `chunkstore-grpc` client | **wrap** (`TransientFault` → `TransportError` → `Status`) | mirrors `DATA_LOSS`→`IntegrityFault`, which also replaces the top-level box; no production consumer downcasts `TransportError` (grep: tests only) |
| `metadata-tikv` `OperationTimedOut` | **inside** (synthetic source) | `tests/deadline.rs:118` asserts `downcast_ref::<OperationTimedOut>().is_none()` for a timed-out *commit*. Wrapping would make that assertion **pass vacuously** — a false green on a `#515` safety property |
| `metadata-fdb` `RetryBudgetExhausted` | **inside** (synthetic source, carrying a copy of `last`) | `src/lib.rs:1975` downcasts it at the top level |

The tikv row is the one I want the reviewer to check. Wrapping there is *cheaper* (no
struct change, no dropped derives) and would have been silently, dangerously wrong.

## 2. Producers

- **chunkstore-grpc, both directions** (the brief names both):
  - client `class_of()` (`client.rs:20-56`): `UNAVAILABLE | CANCELLED | DEADLINE_EXCEEDED
    | RESOURCE_EXHAUSTED` → Transient; `DATA_LOSS` → Integrity; else Terminal.
  - server `transient_or_internal()` (`server.rs:18-35`): a store's transient fault rides
    `UNAVAILABLE`; everything else stays `INTERNAL` (unchanged).
  - Applied at the `TransportError::from` choke point, so **every** RPC benefits, not just
    `get_fragment`.
- **chunkstore-fs**: no change needed — it already produces `IntegrityFault`, which now
  classifies `Integrity`. Verified end-to-end by the named test (real rot on real disk).
- **metadata-redb**: no transient class to produce (embedded, no network). Asserted rather
  than assumed — 3 unit tests incl. a *genuine* redb-native open failure.
- **metadata-tikv**: `OperationTimedOut` carries the class. See §4.
- **metadata-fdb**: `RetryBudgetExhausted` carries the class — and FDB's **own**
  `is_retryable()` predicate is what says it is transient (it gates the retry loop), so no
  FDB error-code taxonomy is guessed at.

## 3. Two real defects found in passing

1. **`CANCELLED` is the timeout code, not `DEADLINE_EXCEEDED`.** My first mapping used the
   textbook reading and the timeout leg went red with `left: Terminal, right: Transient`.
   tonic renders an expired channel deadline as `Status::cancelled("Timeout expired")`
   (`tonic-0.14.6/src/status.rs:644-646`). Excluding `CANCELLED` would have made the seam's
   transient class **miss the timeout case entirely** — the very case 0010 names. Included,
   with the citation in the doc.
2. **`client.rs:74-78`'s existing doc claim was false**: "such a request instead fails with
   a transient `DEADLINE_EXCEEDED` Status". It does not. Corrected in place, since leaving
   a known-false claim beside the new mapping that rides those codes would mislead.

`ABORTED` stays terminal (a precondition conflict's retry belongs to the layer owning the
precondition). `Endpoint::try_from` failure (malformed URI = invalid config) stays terminal
— only the *dial* is transient. That distinction is why `dial_error` exists rather than
mapping every `transport::Error` to transient.

## 4. Rejected alternatives — with the cost, not an adjective

- **Boolean predicates (`is_transient()`/`is_terminal()` free functions).** Rejected by the
  brief itself: #575's counter consumes a label form. Cost of the bool route is not size,
  it is that #575 could not be built on it.
- **Add a field to `TransportError`'s transient variants** (`Unavailable(Status,
  TransientFault)`), keeping the top-level type. Rejected on measured cost: **9 sites** —
  construction `error.rs:33,34,35`; patterns `error.rs:43-46`, `error.rs:54-57`,
  `error.rs:73-83`; `round_trip.rs:104-107`; `dserver.rs:245-248` — *plus* `Connect` is
  used as a fn-pointer `map_err(TransportError::Connect)` at `client.rs:60,62,84,88`, which
  a new field breaks into 4 closures, *plus* it duplicates the `Status` (the fault needs
  its own copy for the chain). The wrap costs **2 test helpers** (`dserver.rs:240`,
  `round_trip.rs:99`) becoming chain-walkers — which is the codebase's own idiom
  (`is_integrity_fault`, and `metadata-fdb/tests/timeout.rs:228` already walks for exactly
  this reason).
- **Attaching the marker to `Status` via `Status::set_source`.** Tempting (zero API
  change), rejected on correctness: tonic already sets a client-side status's source to the
  underlying transport error, and there is no public getter for the `Arc`, so setting it
  would **destroy the original cause** with no way to preserve it.
- **A `&'static TransientFault` marker returned from `source()`.** Rejected: it would drop
  the `Status`/`transport::Error` out of the chain, and `error.rs:88`'s test
  `source_exposes_the_wrapped_status` would still pass — its *name* would become a lie.

## 5. Scope discipline

- `crates/core`'s private `FaultClass` (`read.rs:140`) — **untouched**. The brief lists it
  under *Difficulty* ("align/subsume"), but *Scope* does not name `crates/core`, and *out
  of scope* is explicit: "any behaviour change to retry/repair logic (classification only —
  consumers may *read* the class; changing what they *do* with it is later work)". Rewiring
  `FaultClass::Transient` to derive from `classify()` would change the emitted per-class
  counters (`read_fragment_transient` et al.) — that is precisely 0010 item 4 / issue
  #575's error-by-class counter, which the brief's Ordering note assigns elsewhere. The
  seam enum *subsumes* the partition (its Transient arm is now expressible at the seam);
  making core consume it is 575's slice. **Flagging for the reviewer as a deliberate
  reading, not an oversight.**
- File set stays off `crates/gateway-s3` and `crates/server/src/{cli,dserver}.rs` (575/576
  own those). `crates/server/tests/dserver.rs` **is** touched — a test file, not those src
  files — and only its `status_code` helper, which had to become a chain-walker. That break
  was *informative*: it is the proof the change reaches the real d-server shed/timeout path.
- `proto` / the wire: unchanged, as 0010 requires (classes ride existing status codes).

## 6. Verification

Runner: the project's own `./engine/xtask.sh ci` (→ `cargo xtask ci`), per `pdca.toml`
`[gates] runner`. **`xtask ci: all checks passed`** on the final tree (fmt, clippy `-D
warnings`, build, test, machete, deny, conformance, statics, DST 50 seeds).

`cargo fmt --all` run over every touched file (it caught one badly-wrapped `assert!` in
metadata-redb; fixed). Commit-hook-ready.

### The three forced questions

**(a) Genuine red?** Yes — twice, and the first is the strong one.

- *Semantic red* (revert **only** `chunkstore-grpc/src/client.rs`, keep the seam types so
  everything still compiles): **4 of 5 legs fail on real classification assertions** —
  `a timed-out request is the transient class … left: Terminal, right: Transient`. This is
  the leg that proves the test binds the **gRPC-seam survival**, not merely the existence
  of a new symbol.
- *Full production revert* (what C4-verify does — all 6 src files reverted, added test
  kept): `error[E0432]: unresolved imports wyrd_traits::classify, wyrd_traits::ErrorClass`
  → red by compile, which the gate counts.
- Tree restored and re-verified green after both.

**(b) Production path?** Yes. The test drives `GrpcChunkStore` / `ChunkStoreService` — the
production client and server — over a **real loopback tonic connection** (real HTTP/2, real
prost), hosting the real `FsChunkStore`. `classify` is the production seam function. No copy,
no mock of the classifier.

**(c) Fixture includes the fault?** Yes, and one attempt at this was caught red-handed:

- *Terminal*: the fragment is genuinely rotted **on the server's own disk** (`fragment_path`
  → flip the checksum byte) and detected by the real `FsChunkStore` verify. Not injected.
- *Transient*: my first draft killed the server with `server.abort()` — and the test
  **failed with `Ok(None)`**: tonic spawns each accepted connection on its own task, so
  aborting the acceptor leaves the connection serving happily. A "killed" server that still
  answers would have made the whole transient leg vacuous. Fixed with
  `serve_with_incoming_shutdown` + awaiting the serve future (`Killable::kill`), so the
  listener *and* the connections are genuinely gone. The reason is written into the test's
  doc comment so it cannot regress silently.
- The second transient leg (`ParkedStore`) uses a store that parks — but it injects **no
  seam type**: the fault is manufactured by tonic's own channel timeout and classified by
  the real client mapping.

### Test inventory

- `crates/chunkstore-grpc/tests/error_class.rs` (**the named file, new**) — 5 legs.
- `crates/traits/src/lib.rs` — 9 unit tests (every mapping row, the label space, the
  partition property, the fail-safe default).
- `crates/metadata-tikv/src/lib.rs` `deadline::tests` — 3 new, **feature-free**.
- `crates/metadata-fdb/src/lib.rs` `store::tests` — 2 new, feature-on **run green**.
- `crates/metadata-redb/src/lib.rs` — 3 new unit tests.

Backend tests are unit tests (`#[cfg(test)] mod`), **not** new `tests/*.rs` files —
deliberate: `engine/scripts/run-verify.sh:92-93` keys the red leg on *added* `*/tests/*.rs`
files, so a second added test file would muddy which test binds the criterion. The brief
wants `error_class.rs` to be that file.

### Deferred item (pre-declared by the brief) — status

- **FDB: fully discharged.** `cargo check -p wyrd-metadata-fdb --features fdb` ✅ and
  `cargo test -p wyrd-metadata-fdb --features fdb --lib` → **40 passed**, including the two
  new class tests. libfdb_c + headers present on this host.
- **TiKV: compile NOT possible here.** See §7.

## 7. NEEDS-HUMAN

NEEDS-HUMAN external dependency: openssl dev (pkg-config + libssl) — blocks `cargo check -p
wyrd-metadata-tikv --features tikv` (brief Verification-posture item (i) for TiKV), so the
feature-on TiKV compile evidence could not be produced. `openssl-sys v0.9.117` build script
exits 101: "Could not find directory of OpenSSL installation". Host has `libssl.so.3` but
**no** `/usr/include/openssl/ssl.h` and `pkg-config --exists openssl` fails.

I did **not** work around it: no `OPENSSL_DIR` shim, no vendored-openssl patch to
`Cargo.toml`, no substituting a code-read for the compile.

**Residual risk is small and bounded by construction — this is why the tikv change is
shaped as it is.** The entire class production and all 3 of its tests live in the
**feature-free** `deadline` module, which `cargo xtask ci` compiles and runs on this host
(verified: `deadline::tests::an_abandoned_operation_classifies_transient_at_the_seam ...
ok`). Inside `#[cfg(feature = "tikv")]` exactly **one** code line changed: the struct
literal `OperationTimedOut { op, after_ms }` → `OperationTimedOut::new(op, after_ms)` in
`under_deadline`. Its shape (incl. the `#[allow]` moved onto a `let` **statement** —
statement attributes are stable, expression attributes are not) was compile-probed in
isolation. Dropped derives `Clone, PartialEq, Eq` on `OperationTimedOut`: grepped every
in-tree usage (`tests/deadline.rs:85,96,118`, `src/lib.rs:1065`) — all are `downcast_ref` +
field reads + `Display`; none uses them.

**Human validation:** `cargo check -p wyrd-metadata-tikv --features tikv` and
`cargo xtask tikv-conformance` on a host with `libssl-dev`.

The doctor row **already exists** (`pdca.toml` `[[doctor.checks]] id = "openssl dev
(pkg-config + libssl)"`, `level = "WARN"`) and the brief listed it under External
dependencies — so Plan registered it correctly. The gap is that WARN never blocks, so a
bundle whose scope *requires* the feature-on compile started anyway. The actionable delta:

```toml
[[doctor.checks]]
id    = "openssl dev (pkg-config + libssl)"
cmd   = "pkg-config --exists openssl"
hint  = "apt-get install pkg-config libssl-dev — required to compile the `tikv` feature (openssl-sys) for `cargo xtask tikv-conformance`; REQUIRED for any bundle whose scope includes a feature-on TiKV compile (#577)"
level = "MISSING"   # was WARN — a bundle that must compile tikv feature-on cannot proceed without it
```

(Keep it WARN if the row is meant to stay global — the honest fix may instead be that a
brief naming a feature-on compile as a deliverable should make its dependency row required
for *that* cycle.)

**Second pre-declared NEEDS-HUMAN (brief §Design, INTEGRATION §4):** this patch edits
`docs/design/proposals/draft/0010-observability-floor-for-first-deployment.md` — appending
the ratified sequencing decision (the graduation criterion "the typed-errors/M4 sequencing
decision is recorded"), and striking the now-resolved Open question. 0010 is a *draft*
proposal (no immutability conflict), but any proposal edit is a project-defined human-only
item. The decision itself is recorded, not re-litigated; I added the retrospective evidence
of *why* "adapt after" cost nothing, and the generalisable rule (an additive seam change
ripples as new production sites; a backend swap ripples as contract change — the additive
side is the one that can go second).
