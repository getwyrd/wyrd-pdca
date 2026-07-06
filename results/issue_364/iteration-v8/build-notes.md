# Build notes — issue #364 (s3-http-wire-surface), iteration 8

*Withheld from the reviewer. Rationale for the human at Check sign-off.*

## What this iteration changes vs. the accepted-in-substance v7 attempt

The v7 feature floor was accepted in substance (signed loopback PUT→GET→DELETE
byte-identical, differentiated auth codes, streaming fail-closed, RustCrypto SigV4,
committed crate boundary `gateway-core` + `gateway-s3`, DELETE crash-leak backstop,
real-`aws-sdk-s3` interop). It was rejected for **two durability defects** that sit on
the M4 production-metadata-backend durability seam, plus one folded-in adversary finding.
This iteration **starts from the v7 patch** (applied cleanly on the current target base
`5d87cc4`) and closes exactly those three, touching nothing else in the accepted floor.

The three carry-forward items (iteration-7 sign-off) and how each is closed:

### Finding 1 — durable id allocator / startup recovery  *(BLOCKING)*
**Defect:** `Gateway::new` seeded `next_inode`/`next_chunk` to `1` on every process start
(`crates/server/src/lib.rs:85` on the v7 tree). Over a persistent redb+fs store a restart
replays ids from 1: a new-key PUT re-mints a committed inode id — `metadata::create` is
`require_absent` on the inode key (`crates/core/src/metadata.rs:295`), so it is spuriously
rejected — and its write fan-out overwrites the prior object's chunk-1 fragments *before*
the losing commit, corrupting object A even though B failed.

**Fix (smallest change that restores the invariant "ids never collide across a restart"):**
- `metadata::high_water_marks(store) -> (InodeId, ChunkId)` (`crates/core/src/metadata.rs`)
  scans `inode:` records (their id + every chunk id in their chunk map) and the `pending:`
  ledger for the largest id already on disk. Chunk ids are projected to the `< 2^64`
  in-process allocation space on purpose: the *cluster* client mode derives chunk ids as
  `(inode << 64) | seq` (`server::cli::chunk_id_minter`, `crates/server/src/cli.rs:573`)
  and recovers *its* allocator from the durable `meta:next_inode` counter, so those
  disjoint above-`2^64` ids are not the in-process counter's to resume from (and never
  collide with it — the in-process counter only mints below `2^64`).
- `Gateway::recover(&self)` (`crates/server/src/lib.rs`) `fetch_max`es the two atomics past
  the marks — monotone/idempotent, so it only ever raises the counters.
- Wired at the **composition root** the serve path uses: `cmd_s3` calls
  `gateway.recover().await?` before `serve` (`crates/server/src/cli.rs:755`). This mirrors
  the existing durable allocator the cluster path already relies on (`alloc_inode`,
  `cli.rs:536`) — the S3-serve `Gateway` was the one composition that lacked it.

**Why recovery, not a per-PUT durable CAS counter:** the carry-forward offered "durable
allocator **OR** recover the high-water marks." Recovery is the minimal restoration for a
*single-process* gateway (the `Gateway` doc already scopes it to one process,
`lib.rs:61`). A per-PUT CAS on a hot counter key is the cluster path's concern for
*multi-writer* allocation and is strictly larger (every PUT gains a metadata round-trip and
a retry loop — see `alloc_inode`'s ~25-line backoff loop, `cli.rs:536-571`); it would be
solving a multi-writer problem this single-process seam does not have. Multi-process
concurrent allocation remains the cluster path's job, unchanged.

### Finding 2 — per-chunk lease deadlines / in-flight renewal  *(BLOCKING)*
**Defect:** `stream_write_data` computed one `lease_expiry = now + ttl` and stamped it on
**every** chunk (`crates/core/src/write.rs:437` on the v7 tree). A streaming PUT commits
only after its *last* chunk, so until then an early chunk's fragments are protected from
the custodian GC **solely by its pending lease** — they are in no committed chunk map, so
GC's committed reference set does not cover them (`custodian::gc::reconcile`,
`gc.rs:100/142`). A slow authenticated upload running past the single deadline let the GC
reclaim its early chunks as `expired-lease` garbage before the commit, publishing an object
with missing fragments.

**Fix:** `stream_write_data` now takes a **clock closure** (`now_fn: impl FnMut() -> u64`)
read **per chunk** instead of a fixed instant, and `lease_write_chunk`
(`crates/core/src/write.rs`) **renews** every in-flight lease (batched,
`metadata::renew_pending`) whenever the clock passes a half-TTL renewal point, stamping the
new chunk from its own write time. Every in-flight lease therefore stays a fresh TTL ahead
of the most recent write; only a stall *longer than a TTL between two chunks* lapses — a
genuinely dead upload the sweep should reap. Taking a clock *closure* (not an instant) keeps
DST runs on a deterministic logical clock (ADR-0009); the production path passes the free
`now_millis` fn (`crates/server/src/lib.rs:216`).

Only production caller of `stream_write_data` is `Gateway::put_object_streaming`; no other
call site or test drove the old signature, so the change is contained.

### Folded-in — streaming GET mid-stream fault framing  *(adversary, same theme)*
**Defect:** the streaming GET emitted `200 OK` then a body stream with no `Content-Length`;
a chunk fault mid-stream (e.g. a fragment reclaimed by a racing DELETE) ended the body early
and the client could not distinguish a truncated read from a complete object (a single-chunk
object truncated to zero bytes silently).

**Fix (decision: accurate `Content-Length`, the S3-standard framing):** the GET seam now
carries the committed object size alongside the stream —
`wyrd_gateway_core::ObjectRead { size, stream }` replaces the bare `ObjectStream`
(`crates/gateway-core/src/lib.rs`) — and the S3 handler sets `content-length: <size>`
(`crates/gateway-s3/src/lib.rs`). With an accurate declared length a body truncated by a
mid-stream fault is a **detectable short read** on the client, not a silent success. A true
in-band S3 error code after `200 OK` is impossible once the status line is flushed (inherent
to streaming), so declared-length framing is the correct standard answer. The complementary
grace-window protection against GET-during-DELETE truncation on the *happy* path is already
present (v7's `get_streaming_resolved_before_delete_is_not_truncated`); this closes the
genuinely-faulted case.

## Tests (red → green, driving production)

- `crates/core/tests/stream_lease_renewal.rs` — **behavioural** guard for finding 2. Drives
  the production `write::stream_write_data` with a logical clock advancing a full TTL between
  three chunks; asserts every in-flight lease is renewed past the sweep horizon, that
  `sweep_expired_leases` at that horizon reclaims **nothing**, and the object reads back
  byte-identical. Demonstrated red: temporarily disabling the renewal branch fails the test
  (`expiry=30000` stuck at the start-of-upload deadline, swept) — so the test guards the
  renewal logic, not merely the new signature.
- `crates/server/tests/s3_http_wire.rs` (the brief-named file):
  - `restart_recovers_id_allocators_no_collision` — finding 1: PUT A, drop handles (restart),
    reopen the same persisted store, `recover()`, PUT a new key; both objects survive
    byte-identical. Drives production `Gateway::recover` + `put_object`.
  - `restart_without_recover_collides_showing_the_bug` — proves `recover` is load-bearing:
    the same restart *without* recovery makes the new-key PUT collide (err), i.e. the bug is
    real and `recover` is not a no-op.
  - `get_declares_accurate_content_length_for_truncation_detection` — folded-in: asserts the
    raw GET response declares `content-length == object length`. Behavioural red pre-fix (no
    such header).

Net-new module APIs (`recover`, the `now_fn` signature, `ObjectRead`) make the first two a
compile-error red on the unpatched tree (permitted for net-new coverage); the assertions are
behavioural (collision-vs-no-collision, renewed-vs-swept, length-declared), so they stay red
against a wrong implementation that still compiles.

## Gate / verification

- `cargo test --workspace --exclude wyrd-dst` — **0 failures** (88 `test result: ok`).
- `cargo clippy -p wyrd-core -p wyrd-server -p wyrd-gateway-core -p wyrd-gateway-s3
  --all-targets -- -D warnings` — clean.
- `cargo fmt --check` on the touched crates — clean (commit-ready).
- The historically-flaky `gateway_lease_expiry.rs` wall-clock test was **already quarantined
  in the carried-forward v7 patch** (20s `SKEW_ALLOWANCE_MILLIS` lower-bound slack +
  `finished`-based upper bound); it re-ran green here. This is the recurring C4-red root
  cause named across iterations 3/4/7 — not a regression from this diff.
- The real `aws-sdk-s3` interop tests (`real_aws_sdk_put_get_delete_round_trips_byte_identical`,
  `real_aws_sdk_unsigned_client_is_refused`) pass, confirming the new `Content-Length` GET
  framing round-trips against a real SDK (not just the hand-rolled client).

## Standing human calls (pre-declared, not blocking the rebuild)

Per the iteration-7 sign-off these are *ratified* and were **not** re-litigated:
- TLS: plaintext-loopback-at-Check accepted; public TLS deferred to #367 (rustls provider
  deny.toml/license decision when wired).
- Minimal S3 error-code floor is sufficient; the full conformance sweep stays out of scope.
- M4 sequencing: target is `feat/m4-production-metadata-backend`.
- `aws-sdk-s3` dev-dep: accepted (`cargo deny` clean per iteration-6/7).

## Deferred residuals (honest, off-Check)
- Replay-within-freshness-window and UNSIGNED-PAYLOAD-on-plaintext remain tied to the
  accepted TLS deferral (#367).
- Finding-2 renewal reaps a client that stalls *longer than a full TTL between two chunks*
  as a dead upload — intended, not a gap.
