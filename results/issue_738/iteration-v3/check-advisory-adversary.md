# Adversarial review — issue 738 (`wyrd s3 --chunk-size`)

I tried to refute the fix and could not break its core claims. Two findings remain: one doc
threshold that is off by one step, and one design call on D-server memory that a human should
make. Everything else below is evidence I checked and found sound.

All runs were in a scratch copy of `$PDCA_TARGET`. I confirmed it was byte-identical to the
target's patched files before running anything.

## Findings

- NEEDS-HUMAN [impl] — **The "above ~24 MiB" upgrade threshold is wrong at exactly 24 MiB.**
  `deploy/dist/env/s3.env.example:17`, `docs/design/architecture/m4-first-deployment-blueprint.md:1120`
  and `docs/design/architecture/08-crosscutting-concepts.md:111` all tell operators that the
  old-binary 4 MiB gRPC limit only bites "above ~24 MiB". But `--chunk-size 25165824`
  (exactly 24 MiB, a natural round value) already fails. Each RS(6,3) data shard is then exactly
  4 MiB (`crates/core/src/erasure.rs:80-83`), and the 48-byte fragment header/trailer plus the
  protobuf envelope push the message past 4,194,304. **Reproduced:** in the scratch copy I
  removed both `max_*_message_size` setters (old-binary behaviour) and set the test's `BIG_CHUNK`
  (`crates/server/tests/s3_chunk_size_flag.rs:59`) to `24 << 20`. The PUT returned `500` at
  `s3_chunk_size_flag.rs:588`. With `23 << 20` the same test passed. An operator on a mixed fleet
  who reads "above ~24 MiB" and picks 24 MiB gets the 500s the note is meant to prevent. Fix:
  say "24 MiB or more" (or "above 23 MiB") in all three places.

- NEEDS-HUMAN [human] — **Every D server now accepts 192 MiB messages, whatever chunk size the
  fleet uses, and this goes beyond the brief's scope.** `MAX_MESSAGE_BYTES = 192 << 20`
  (`crates/chunkstore-grpc/src/lib.rs:54`) is applied to every D server through `into_server`
  (`crates/server/src/dserver.rs:1209`). With the global 64-request admission limit
  (`dserver.rs:61`, `:1180`), the worst-case request buffering per D server rises from about
  256 MiB to about 12 GiB, even in a deployment that keeps the 1 MiB default.
  `08-crosscutting-concepts.md:111` says that worst case is "reached only when clients actually
  send near-ceiling fragments". That is only true for resident memory. tonic 0.14.6 calls
  `self.buf.reserve(len)` as soon as it reads the 5-byte gRPC header
  (`~/.cargo/registry/src/*/tonic-0.14.6/src/codec/decode.rs:199`), so the declared length alone
  causes the reservation. On default Linux overcommit that is only virtual memory. On a host with
  strict overcommit (`vm.overcommit_memory=2`), 64 headers that each claim ~192 MiB use up about
  12 GiB of commit charge, and Rust aborts the process when an allocation fails.

  None of the transport work is in the brief's Scope (a)–(g). It adds public API to
  `wyrd-chunkstore-grpc` (`MAX_MESSAGE_BYTES`, `ChunkStoreService::into_server`,
  `crates/chunkstore-grpc/src/server.rs:82`), and the brief's "Impact & compatibility" names only
  two changes outside `cmd_s3`. It came from an earlier review round's P1, and it is a real fix
  for a real failure. Sign-off should still choose deliberately among three options:
  1. Accept a fixed 192 MiB bound fleet-wide.
  2. Make the bound configurable, or derive it from the configured chunk size.
  3. Revisit the "settled" 1 GiB `MAX_CHUNK_SIZE` (`crates/server/src/lib.rs`). The brief chose
     that ceiling without considering the transport.

  This is not a correctness bug in the patch.

## Attacks on the evidence (did not refute)

- **The gate's red leg does not prove the transport half, but the tests do.** In the frozen
  `gate-logs/C4-verify.log`, `a_chunk_past_the_4_mib_grpc_default_round_trips_through_a_d_server`
  went red at the chunk-count assertion (`left: 32, right: 1`, `s3_chunk_size_flag.rs:595`). That
  red comes from the flag being ignored, not from the 4 MiB limit. `round_trip.rs:119` is in a
  modified file and calls `into_server`, so it cannot compile on the reverted tree and was never
  red-checked by the gate. I filled that gap by hand with the full patch applied:
  - Removing both size setters makes the cluster PUT return `500`.
  - Removing only the client setters makes the cluster GET fail the strict parser ("declared
    content-length 33554432, but 0 body bytes arrived").
  - With the client setters removed, `round_trip.rs`'s test also fails with `OUT_OF_RANGE`
    ("found 6291509 bytes, the limit is: 4194304").

  Both transport tests do discriminate.
- **`check-gates.json`'s C4-verify `path_line` says "8 test(s) ran red". The log shows 6 failed
  and 2 passed.** The 2 that passed are the response-parser self-tests, which should pass on
  either tree. This is a counting error in the harness and does not change the verdict.
- **C4-diff-cov's 40% "fail" is not evidence against the fix.** Its MISS positions do not match
  this tree. `crates/server/src/cli.rs:2199-2200` and `:2331-2338` are comment and doc-comment
  lines. `:2312-2316` is `parse_s3_chunk_size`'s out-of-range return, which the binary-driven
  refusal tests visibly execute (they go red pre-fix and green post-fix). The likely cause is that
  the gate does not see coverage from the `wyrd` child process, or that its line positions are
  shifted. This is the same harness issue earlier rounds hit.
- **Green leg reproduced.** `cargo test -p wyrd-server --test s3_chunk_size_flag` passed 8/8.
- **The template test now goes red.** I deleted ` --chunk-size 1048576` from the live
  `WYRD_S3_ARGS=` line and `env_examples_name_every_load_bearing_flag` failed at
  `xtask/tests/dist_templates.rs:196`. The iteration-1 finding is fixed.

## Attacks on the fix (could not break)

- **Parsing** (`cli.rs:2295-2315`): `+1`, `-1`, empty, `1MiB`, `18446744073709551616`, leading
  space and non-ASCII digits are all refused. A trailing `--chunk-size` with no value already
  errors in `ParsedArgs::parse` (`cli.rs:2618-2620`). `--chunk-size --endpoints x` takes
  `--endpoints` as the value and refuses it.
- **Refusal happens before bind:** the parse is at `cli.rs:2202` and the listener binds at `:2248`.
- **All six arms apply `with_chunk_size`** (`cli.rs:2411-2455`). The tikv/etcd arms compile under
  the `host-tikv` gate. No gate compiled the fdb arms, so I ran
  `cargo check -p wyrd-server --features fdb,etcd` myself and it compiled cleanly.
- **No bare `ChunkStoreServer::new` / `ChunkStoreClient::new` is left** anywhere in the tree.
  The client is built only at `crates/chunkstore-grpc/src/client.rs:256` (bounded), and the
  custodian (`cli.rs:1603`) and the gateway fan-out both go through it.
- **The compile-time fragment-fit assertion** (`crates/server/src/lib.rs:75-83`) checks only
  `DEFAULT_DURABILITY`. That is enough: the `s3` role never calls `with_durability`, so every
  accepted `--chunk-size` is covered.
- **DST does not model the size bound at all:** madsim-tonic's generated setters are no-ops. The
  code says so at `crates/chunkstore-grpc/src/server.rs:79`. DST never modelled the old 4 MiB
  default either, so this is not a new fidelity gap from this diff.
- **Not raised, because already settled:** the chunk-map blow-up at tiny chunk sizes (deferred to
  #739 by the brief) and the per-PUT `Vec::with_capacity(chunk_size)` at the ceiling (the brief
  accepted this with the 1 GiB ceiling).
- **Informational, outside the brief's scope (f):** `deploy/small-multi-node/docker-compose.yml:393,408,423`
  (the TiKV stack) still omits `--chunk-size`, while the FDB stack now names it.
