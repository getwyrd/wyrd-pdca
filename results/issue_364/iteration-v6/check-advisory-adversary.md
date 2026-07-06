# Adversarial review — issue 364 / s3-http-wire-surface (iteration 6)

Skeptic's pass. Attacked the red→green evidence, the fix, and the reviewer verdict.
Ground truth read on `$PDCA_TARGET` = `/home/eddie/wyrd/wyrd.pdca-wt-l1`. Advisory only —
no gate. Scope limited to this diff.

## Findings

- **NEEDS-HUMAN — The `aws-chunked` decoder does not bound the *declared* chunk size, re-opening
  the "stream, don't buffer" OOM cliff (0015:789) and, worse, turning a malformed size into a
  silent `200 OK` truncated write.** `crates/server/src/s3/streaming.rs:195` loops
  `while self.buf.len() < size + 2 { fill }` where `size` comes straight from the attacker's chunk
  header (`parse_chunk_header`, `streaming.rs:238`, `usize::from_str_radix` with **no upper bound**),
  and the chunk signature is only checked *after* the whole chunk is buffered
  (`streaming.rs:210`). Two concrete failing cases, both reachable by an **authenticated** client
  (only the seed signature must verify):
  - *Memory amplification:* a chunk header `40000000;chunk-signature=…` (1 GiB) followed by the
    bytes forces up to 1 GiB resident in `Decoder::buf` before anything drains; k concurrent PUTs →
    k GiB. This is exactly the per-request-cap OOM class iteration 1/2 flagged as inverting the
    "stream, don't buffer" invariant — now unbounded and untested (the streaming tests use tiny
    64 KiB/uneven chunks only).
  - *Overflow → silent truncation:* a chunk header `ffffffffffffffff;chunk-signature=…` makes
    `size + 2` (`streaming.rs:195`) overflow — panic in the debug/test build, out-of-bounds slice
    at `streaming.rs:200` otherwise. The panic occurs in the **spawned** decode task, which merely
    drops the channel sender; `ReceiverStream` then yields `None`, so `put_object_streaming` sees a
    clean EOF and **commits**. Because the streaming body is handed to the writer as
    `PayloadHash::Unsigned` (`crates/server/src/s3/mod.rs:237`), there is *no* end-to-end integrity
    check, so a malformed-framing request that should be `400 InvalidRequest` instead returns
    `200 OK` for a truncated/empty object. This is a fail-open on the write path, contradicting the
    fail-closed intent. No test exercises a large or malformed chunk-size header, so the green gate
    says nothing about it.

- **NEEDS-HUMAN — The DELETE/overwrite "GC reclaims it" backstop cannot run in the one runnable
  server role this patch ships.** The iteration-5 BLOCKING leaks were closed by deferring all
  reclaim to the custodian GC loop: `unlink` / `commit_chunk_map_superseding` write durable orphan
  records (`crates/core/src/metadata.rs`) and nothing reclaims eagerly. But the delivered gateway
  process, `cli::cmd_s3` (`crates/server/src/cli.rs:716`), composes `RedbMetadataStore::open(...)`
  + `MemCoordination::new()` (`cli.rs:745`) and **spawns no custodian** (confirmed by the standing
  comment `cli.rs:53` "The CLI runs no custodian sweep"). The custodian GC lives in a separate
  crate/process; with a single-writer, file-locked redb store owned by the gateway and in-memory
  (non-shared) coordination, no co-process custodian can attach to reclaim. So in the runnable
  `wyrd s3` role, orphaned fragments from every DELETE and every PUT-overwrite accumulate
  unreclaimed for the life of the process — the same leak class three prior iterations treated as
  blocking, now relocated behind a backstop no co-running process can service. The orphan records
  are durable (a later/tikv-shared custodian *could* reclaim), so this is a topology gap a human
  must adjudicate against the #367 deployment plan, not a proof of permanent loss.

- **The "real-SDK interop closes break 1 (percent-encoded key identity)" claim is only half
  demonstrated.** The genuine `aws-sdk-s3` round-trip
  (`crates/server/tests/s3_http_wire.rs`, `real_aws_sdk_put_get_delete_round_trips_byte_identical`)
  uses key `real-sdk/round-trip-object` — **no** space or `%`-requiring character. The specific
  carry-forward break ("boto3 key `my file.txt` stored/keyed as `my%20file.txt`") is still proven
  only by the unit test `percent_decode_recovers_the_true_key` and the gateway's own self-signed
  wire tests (`s3/mod.rs` `percent_decode_utf8`), i.e. self-referentially. No real SDK request with
  an encoding-sensitive key ever hits the listener, so the interop oracle does not actually cover
  the byte the break was about. Low blast radius (S3 single-encodes the S3 canonical URI, so it
  likely holds), but the reviewer's "not self-referential" verdict overstates coverage for this
  case.

## Attempted and could not refute

- DELETE idempotency under a concurrent race: `delete_object` (`crates/server/src/lib.rs`) resolves
  the CAS-conflict path to an idempotent `Ok(false)`; the concurrent-delete test drives real spawned
  tasks. Could not break it.
- PUT-overwrite reclaim: `commit_chunk_map_superseding` (`crates/core/src/metadata.rs`) orphans the
  prior map's fragments in the *same atomic batch* and keeps the current map's fragments; the
  `gc_delete_backstop` test proves prior-gone / current-kept via placement-aware keys. Sound (modulo
  the "who runs GC" gap above).
- SigV4 correctness: the `get-vanilla`, query-sorting, and published streaming known-answer vectors
  are genuine independent oracles; `trim_all` whitespace-collapse and non-sorted `SignedHeaders`
  handling (`s3/sigv4.rs`) address the iteration-5 fail-closed erosions.
- Auth-before-body on the *reject* path (`s3/mod.rs` `handle` verifies before touching the body):
  the never-sent-body test confirms it. (The *accepted* streaming path still amplifies — see
  finding 1.)

## Note on the red→green evidence

Per `check-gates.json`, C4-verify is "red pre-fix, green post-fix," but for this net-new module the
red is a compile-error red (pre-declared acceptable). The green therefore certifies only the cases
the new tests assert; it is silent on the unbounded/malformed chunk-size path (finding 1) and on
the GC-topology gap (finding 2).
