# Build notes — issue 638 / fragment-write-deadline

(Withheld from the reviewer; for the human at sign-off.)

## What I built

A server-enforced fragment-write authorization deadline (`W_write`, proposal 0016
decision 5, `0016:1551-1576`), exactly the mechanism the brief scopes:

- `crates/traits/src/lib.rs:647-663` — `ChunkStore::put_fragment` gains a third
  parameter, `deadline_millis: Option<u64>` (epoch milliseconds). `None` = today's
  unbounded behaviour.
- `crates/traits/src/lib.rs:552-604` — a new typed seam error, `WriteDeadlineExpired`
  (`id` + `detail: String`, mirroring `IntegrityFault`'s own shape at
  `crates/traits/src/lib.rs:97-104`), plus `is_write_deadline_expired`, mirroring
  `is_integrity_fault` (`:118-132`).
- `crates/traits/src/lib.rs:723-733` — `PlacementChunkStore::put_fragment_at`'s
  default forwards `deadline_millis` unchanged.
- `crates/proto/proto/wyrd/v0/chunk.proto:31-39` — `FragmentPutRequest` gains
  `optional uint64 deadline_millis = 3` (new tag, proto3 explicit presence, same
  idiom `FragmentGetResponse.fragment` already uses at `:40`).
- `crates/chunkstore-fs/src/lib.rs:182-215` — `FsChunkStore::put_fragment` is the
  **enforcement point**: if a deadline is given, it reads its own wall clock
  (`SystemTime::now()`, `#[allow(clippy::disallowed_methods)]` with the AGENTS.md
  #619 rubric's required per-site reason) and refuses — before touching the
  filesystem — if the deadline has already elapsed, returning
  `WriteDeadlineExpired`. This is deliberately **not** in `server.rs`: the check
  lives where the write is actually applied, so a caller using `FsChunkStore`
  directly (no gRPC) gets the identical guarantee (leg E), and a write "parked in
  the D server's accept queue" (0016's failure-mode row) is still caught once the
  handler task finally runs and calls into the store.
- `crates/chunkstore-grpc/src/server.rs:66-97` — the gRPC handler passes
  `request.deadline_millis` straight through and maps `WriteDeadlineExpired` to
  `Code::FailedPrecondition` (distinct from `INVALID_ARGUMENT`'s "bad bytes" and
  the transient/internal fallback).
- `crates/chunkstore-grpc/src/client.rs:118-172,215-232` — `GrpcChunkStore::put_fragment`
  sends the field; `classify_put_status` reconstructs `WriteDeadlineExpired` from
  `FAILED_PRECONDITION`, mirroring `classify_get_status`'s existing trick for
  `IntegrityFault`/`BlockReadFault`.
- `crates/chunkstore-grpc/src/fanout.rs` — `FanoutChunkStore`/its `PlacementChunkStore`
  override forward the parameter through to the routed backend.
- Every other `ChunkStore` implementor in the workspace (~45 impls) and every
  `put_fragment`/`put_fragment_at` call site (~108) is migrated: production
  callsites (`crates/core/src/write.rs:248,447`, `crates/custodian/src/{rebalance,reconstruction}.rs`)
  pass `None` (today's behaviour, unchanged); test doubles that don't model the
  deadline take `_deadline_millis: Option<u64>` (unused); wrapper/wrap-and-forward
  types (`Fleet`/`MemDServer`-style `put_fragment_at` delegates) forward it through.
- `docs/design/architecture/08-crosscutting-concepts.md` §8.9 gets a new bullet —
  the docs-currency rule (`AGENTS.md:154-157`) since this changes an RPC.
- **New test:** `crates/chunkstore-grpc/tests/write_deadline.rs` (legs A/B/C/D/F).
- **Existing files extended** (not the brief's "Test file", so not held to the
  base-compiles bar below): `crates/chunkstore-grpc/tests/round_trip.rs` (the
  production `GrpcChunkStore`/`WriteDeadlineExpired` reconstruction, end to end)
  and `crates/chunkstore-fs/tests/conformance.rs` (leg E asserted directly against
  `FsChunkStore`, no gRPC).
- **DST case appended** to `crates/dst/tests/network.rs` (not a new file, per the
  brief): `a_write_parked_past_its_deadline_is_refused_over_the_simulated_network`,
  0016's own failure-mode row, over the real gRPC `ChunkStore` on madsim's
  simulated network. `DStore` (the existing in-memory D-server fake this file
  already uses in place of `FsChunkStore` under simulation) gained an
  `apply_delay: Duration` field and reimplements the identical deadline check
  `FsChunkStore` uses — consistent with this file's existing convention of
  hand-mirroring the production contract in a madsim-native fake (it already
  reimplements "verify on put"), not a new pattern.

## Design choices, and why

**Deadline, not instant, travels on the wire** (Design §1's open choice). The
receiver (the D server) never needs to know the sender's `W_write` value — it only
compares `now >= deadline`. Carrying the raw authorization instant would make the
D server re-derive the deadline, which means it would need to know `W_write` too —
coupling the chunk store to a policy value #625 owns and the D server is
deliberately ignorant of (mirrors the "Alternatives considered" rejection of
deriving the deadline from a lease lookup — ADR-0010, keeping the D server
session/lease-oblivious). Represented as **epoch milliseconds** (`u64`), matching
the existing wire idiom for exactly this kind of value elsewhere in the tree
(`orphaned_at_millis`, `crates/custodian/src/gc.rs:114`; `lease_expiry_millis`,
`crates/core`) — not a new vocabulary.

**Clock ownership** (0016's own "Clock ownership" note, Design section, and
`clippy.toml`'s `disallowed-methods` ban on a bare `SystemTime::now()`, AGENTS.md
Review rubric, wyrd#619): the *only* wall-clock read in production code is inside
`FsChunkStore::put_fragment`, at the point the write is applied — no other new
site reads real time. That is the "one clock owns this lifecycle" the rubric asks
for: the enforcement lifecycle is entirely local to the store's own `put_fragment`
call, so there is exactly one site to annotate and reason about, not one per
backend or per RPC hop.

**Enforcement lives in the store, not the service handler.** I considered putting
the check in `crates/chunkstore-grpc/src/server.rs` (the "D-server service") since
the brief's Scope line names it as part of the D-server refusal. I kept it in
`FsChunkStore` instead and left `server.rs` as pure pass-through + status mapping,
because:
- The brief's own leg E requires `chunkstore-fs` to "honour the identical
  contract" so "a caller cannot get a weaker guarantee by holding a local store" —
  that is only true if the *store itself* enforces it, not a network-only gateway
  in front of it.
- 0016's failure-mode row is specifically about a write parked **between
  acceptance and application** — the store call is the actual "application" point;
  checking earlier (at request receipt) would miss exactly the delayed-application
  case leg B exists to catch.
- Cost of the rejected alternative (checking only in `server.rs`): a caller using
  `FsChunkStore` directly (an embedded/non-gRPC deployment, or any future
  in-process composition) would get **zero enforcement** — a silent, weaker
  guarantee that violates the Invariant to restore ("a durable write... must
  either take effect within a bounded, enforced window... or never take effect at
  all — stated over the category, not over multipart/over one transport").

**No `ErrorClass`/`classify` change.** I added `WriteDeadlineExpired` +
`is_write_deadline_expired` (mirroring `IntegrityFault`/`is_integrity_fault`
exactly) but did **not** add a new `ErrorClass` variant or a `classify()` row.
Cost of doing so: `classify()`'s docstring table (`crates/traits/src/lib.rs:508-521`)
enumerates exactly four rows plus a fail-safe default and is pinned by tests in
the same file; adding a fifth would touch that table, its tests, and force a
decision about whether a deadline refusal is "transient" (retryable) or
"terminal" — a retry-policy question the brief explicitly scopes out ("choosing
`W_write`'s value and the margin — #625 owns the windows"). The brief's leg F only
asks for "distinguishable... in the register the seam already uses for typed
cross-backend errors" (the `is_*` predicate pattern), which `is_write_deadline_expired`
satisfies without opening that question.

## The Falsifiability requirement — what it cost, and why I paid it

The brief's Falsifiability section makes an unusually specific demand: legs A, B,
E, F must be **red by assertion, not by build error**, because a trait-arity break
is a "real red" that `run-verify.sh`'s exit-code check cannot distinguish from a
build break, and a build-error red proves nothing about the *behaviour* gap.

**What this actually requires, worked through:** `run-verify.sh`'s RED leg reverts
every production file the patch touches (including `crates/proto/proto/...`,
`crates/traits/src/lib.rs`, `crates/chunkstore-grpc/Cargo.toml` if changed) and
keeps *only* the added test file verbatim. Since Rust compiles a test file as one
unit, if **any** function in `write_deadline.rs` referenced the new 3-arg
`put_fragment` (or the patched `FragmentPutRequest.deadline_millis` field, or a
new Cargo.toml dependency), the **whole file** fails to compile pre-fix — which
would silently swallow the assertion-red I was trying to get for the *other*
legs in the same file, because "0 tests ran" is indistinguishable at the harness
level from "5 tests ran, 3 failed." So achieving genuine per-leg red/green
required that **every** function in the file compile against the *fully reverted*
production tree, not just the specific legs the brief names.

That forced three decisions, each with a concrete alternative I rejected and why:

1. **The client side never calls `GrpcChunkStore::put_fragment` or names
   `FragmentPutRequest.deadline_millis`.** Instead `write_deadline.rs` hand-encodes
   the wire bytes itself (`encode_put_fragment_request`, `write_varint`/`write_tag`,
   ~30 lines) and drives a bare `tonic::client::Grpc<Channel>` with a custom
   `Codec` (`RawEncoder`/`RawResponseDecoder`/`RawCodec`, ~40 lines) at the same
   `/wyrd.v0.ChunkStore/PutFragment` path the generated client uses. Rejected
   alternative: use `GrpcChunkStore` directly — costs nothing extra to write, but
   makes the whole file fail to *compile* pre-fix (build-error red for every leg,
   the exact failure mode the brief calls out by name).
   I first tried reusing `prost::encoding::{encode_key, encode_varint}` +
   `tonic_prost::ProstCodec` to save ~15 lines of hand-rolled varint math — but
   that needs `prost` (and, for the codec, effectively still bespoke work) as a
   **new** `[dev-dependencies]` entry in `crates/chunkstore-grpc/Cargo.toml`.
   Cargo.toml is a production file the RED leg reverts too, so the crate
   wouldn't even resolve the `prost`/`tower` names pre-fix — another build error.
   I removed both additions and hand-rolled the ~15 lines instead (`write_varint`/
   `write_tag`, only 20 lines total) so the test needs **zero** new dependencies.
2. **The server-side "parked in the accept queue" delay (leg B) is a hand-written
   `tonic::codegen::Service` wrapper (`DelayedService`, ~35 lines), not a
   `tower::Layer`.** I first wrote this as a `tower::Layer`/`tower::Service` pair
   (the idiomatic tonic-ecosystem way — `Server::builder().layer(...)`), which
   needed `tower` as a new dev-dependency — same base-compile problem as above.
   Tonic re-exports `tower_service::Service` as `tonic::codegen::Service` and
   `tonic::server::NamedService` (both already reachable through the crate's
   *existing*, unpatched `tonic` dependency), but does **not** re-export
   `tower_layer::Layer` anywhere — so I wrap the generated `ChunkStoreServer<S>`
   directly with `DelayedService { inner, delay }` and pass it straight to
   `.add_service(...)`, forwarding `Response`/`Error`/`NAME` from the inner
   service. No new dependency; same behaviour (sleep, then delegate).
3. **`chunkstore-fs`'s leg E is asserted twice, in two different files, for two
   different reasons — not once in `write_deadline.rs`.** (a) Implicitly: every
   leg in `write_deadline.rs` already runs the real `ChunkStoreService<FsChunkStore>`
   — "chunkstore-fs" *is* the backing store under test throughout, satisfying
   "assert A and C against it too... in the same file." (b) Directly, in
   `crates/chunkstore-fs/tests/conformance.rs` (an existing file — not the
   brief's "Test file", so not held to the base-compiles bar): two new tests call
   `FsChunkStore::put_fragment` with the 3-arg signature directly, no gRPC. I
   *did* first write these as part of `write_deadline.rs` — that's what forced
   the realisation above (a direct `FsChunkStore::put_fragment(id, frag, Some(x))`
   call, like the client call, needs the patched trait to even parse) — so I
   moved them to `conformance.rs`, which is a modified-not-added file and is
   naturally excluded from the red-by-assertion bar `run-verify.sh` applies only
   to newly-added test files.

**Cost, quantified:** the new file is ~500 lines instead of the ~260 my first,
simpler-but-build-error-red draft was; two existing files (`round_trip.rs`,
`conformance.rs`) each gained ~25-30 lines. The Cargo.toml/Cargo.lock diff is now
**zero** (I added, then reverted, `prost`+`tower`) — the whole patch touches no
manifest at all beyond the workspace-wide trait-arity migration.

### The three refutation questions (forced, recorded)

**(a) Genuine red?** Yes, and by assertion, not build error. Reverted every
production file (`git stash` the full patch, restore only
`crates/chunkstore-grpc/tests/write_deadline.rs` from the stash) and ran
`cargo test -p wyrd-chunkstore-grpc --test write_deadline`:

```
running 5 tests
test a_deadline_refusal_is_distinguishable_from_a_malformed_fragment_fault ... FAILED
test expired_deadline_is_refused_by_the_server_and_never_stored ... FAILED
test a_live_write_within_its_deadline_stores_and_reads_back_byte_identical ... ok
test absent_deadline_stores_exactly_as_before_issue_638 ... ok
test a_write_parked_past_its_deadline_is_refused_when_finally_applied ... FAILED

test result: FAILED. 2 passed; 3 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.30s
```

**5 tests ran, 3 failed, 2 passed** — the file compiled cleanly against the fully
reverted base (no build error at all), and every failure is a genuine assertion
panic: `expired deadline must be refused: ()` / `a write parked past its deadline
must be refused...: ()` (i.e. `.expect_err(...)` panicking because the call
returned `Ok(())` — the base server *applied* the write, ignoring the unknown wire
field 3, exactly as proto3's forward-compatibility rule predicts). Legs C
(`a_live_write_...`) and D (`absent_deadline_...`) correctly stay green on the
base too — they assert behaviour the fix does not change (additive
compatibility), so a "no fix yet" tree already satisfies them; that is expected
and matches the brief's own framing of leg D as meaningful only in contrast to
A/B, not independently red-required. Restored via `git stash pop` afterward;
diffed the restored file against the stash to confirm bit-for-bit identity before
continuing.

**(b) Production path?** Yes for the server side (real, unmodified
`ChunkStoreService<FsChunkStore>`/`FsChunkStore` in every leg of
`write_deadline.rs`, and in the DST case). For the *client* side,
`write_deadline.rs` deliberately drives a hand-rolled encoder instead of
`GrpcChunkStore` (see above) — so `round_trip.rs`'s new
`put_with_an_expired_deadline_is_refused_and_reconstructs_as_write_deadline_expired`
is what proves the actual shipped `GrpcChunkStore::put_fragment` /
`classify_put_status` path end to end; it is green-only (compile-shaped, since an
existing file, not subject to the red-by-assertion bar) — a hole I did not want
silently unfilled. `chunkstore-fs/tests/conformance.rs`'s two new tests drive
`FsChunkStore::put_fragment` directly, also production code, also green-only for
the same reason.

**(c) Fixture includes the fault?** Yes — every fixture is the real fault the
issue exists to close: leg A authorizes with a deadline already in the past; leg
B authorizes with a deadline that is live at send time but injects a genuine
300ms delay before the request reaches the store (via `DelayedService`, real
tonic dispatch, not a mock), so the deadline provably elapses **between
acceptance and application** — the exact 0016 failure-mode row, not a
stand-in for it. Neither fixture excludes the failing element to make the test
pass.

## Migration mechanics (every existing `put_fragment` callsite/test double)

Mechanical, in three passes, verified by a script that finds every remaining
under-arity `.put_fragment(`/`.put_fragment_at(` call or `fn put_fragment`
definition after each pass (zero remaining outside the 5 seam files I hand-edited):
1. `async fn put_fragment(&self, id: FragmentId, fragment: Bytes) -> Result<()> {`
   (and its `_id`/`_fragment`/`wyrd_traits::Result`/`WyrdResult` variants) → add
   `_deadline_millis: Option<u64>` (unused in every pre-existing test double).
2. `async fn put_fragment_at(...)` bodies that delegate
   (`store.put_fragment(id, fragment)`) → add `deadline_millis: Option<u64>` and
   forward it; bodies that don't delegate (a raw `HashMap` insert) → add
   `_deadline_millis: Option<u64>`, unused.
3. Every `.put_fragment(`/`.put_fragment_at(` call site with too few arguments →
   append `, None` (balanced-paren-aware, handling multi-line calls and existing
   trailing commas — the first pass had a bug here, caught by `cargo check`
   reporting `expected expression, found ','`, fixed by re-scanning for the
   `,\n\s*, None\)` pattern the trailing-comma case produced).

## Alternatives ruled out (with cost)

- **A caller-side timeout only** (status quo) — 0016 rejects this explicitly
  (`:1557-1564`): a client giving up after `W_write` bounds nothing about when
  the D server actually applies an already-accepted write.
- **Server enforces from a lease lookup** rather than a wire-carried deadline —
  would make every fragment write a metadata read on the hot data path and
  couples the chunk store to the metadata/lease plane (ADR-0010 independence).
  Cost: a new cross-crate dependency edge (`chunkstore-fs`/`chunkstore-grpc` →
  metadata) plus a synchronous read added to every write's critical path.
- **Enforce only in the fan-out wrapper** (`fanout.rs`) — still caller-side; a
  second gateway or a retry reaches the service directly and bypasses it. Cost of
  rejecting: none — the brief's own "Alternatives considered" already rules this
  out; I did not re-litigate it, only confirmed the shipped design doesn't
  accidentally rely on it (fan-out only forwards the parameter, enforces nothing
  itself).

## Verified

- `cargo fmt --all -- --check`, `cargo clippy --workspace --exclude wyrd-dst
  --all-targets`, `RUSTFLAGS="--cfg madsim" cargo clippy -p wyrd-dst
  --all-targets` — all clean.
- `cargo test --workspace --exclude wyrd-dst` — 152 test-result blocks, 0 failed.
- `cargo xtask ci` (the project's real, single-sourced gate) — full green,
  including the madsim DST tier (50 seeds) with the new seeded case, `cargo
  deny`, `cargo machete`, the conformance vectors, the statics/deploy-orchestrator
  guards, and (since both tools happen to be installed in this environment)
  `typos` and the docs-renderer/link-audit gate the brief flagged as an external
  dependency.
- `typos` and `python3 docs/publishing/tools/render_site.py --check` individually
  re-run clean after the `08-crosscutting-concepts.md` edit.
- Patch applies cleanly (`git apply --check`) against a fresh worktree at
  `origin/main @ b0cd199` (this bundle's resolved base in this environment; see
  NEEDS-HUMAN note below for the wave-1 base caveat).

## NEEDS-HUMAN / open items for sign-off

- **Wave-1 base ($PDCA_VERIFY_BASE).** The brief states this is a wave-1 bundle
  and the driver exports `$PDCA_VERIFY_BASE=origin/pdca-integration/main` when run
  through the full `pdca flow`/wave scheduler (so this slice verifies against
  #635's folded state, per the brief's Ordering note — no dependency, shares wave
  1, disjoint crates). Running standalone in this environment, no such branch
  exists yet (`origin/pdca-integration/main` is not present) and no
  `$PDCA_VERIFY_BASE`/stack-base marker was set for me, so I built and verified
  against `origin/main @ b0cd199` (the brief's plain "Repo + branch target"). Since
  #635 touches disjoint crates (no file overlap declared), I don't expect this to
  change red/green here, but the real wave-scheduled verify (when this bundle
  runs inside `pdca flow`'s multi-bundle batch) should re-confirm `patch.diff`
  still applies and `cargo xtask ci` stays green on top of #635's folded commit,
  not just `origin/main`.
- **Open question 2 (mixed-version fleet)** from the brief: I did not add a
  capability exchange — an old D server silently does not enforce, which the
  brief already flags as "my assumption" for Alpha and explicitly defers a
  decision on to the maintainer at sign-off. No code change needed unless
  overruled.
- **Open question 3 (chunkstore-fs "queueing")**: leg B's parked-write scenario is
  gRPC/tonic-dispatch-native and not reachable through `FsChunkStore` alone (no
  accept queue exists for a direct library call) — per the brief's own
  permission, I asserted legs A and C against `chunkstore-fs` and did not
  fabricate a queue for B there.
