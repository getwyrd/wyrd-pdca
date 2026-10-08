# Build notes — issue 738 / s3-role-chunk-size-flag (iteration 2)

## Base I built on

`$PDCA_WORKTREE` HEAD = `4bda59c` — the pdca-integration stack: `main` at `36f006d` plus the
folded #774, #839 and #840. **#736 is NOT on this base**: the `s3` role's `role started`
event has no `version` field (`crates/server/src/cli.rs:2262-2269` on base) and `serve_s3`
sets only `config.region` (`cli.rs:2440-2441` on base). So, per the brief's ordering note, I
added no `version` field and extended nothing of #736's.

## What changed (path:line — base → patched)

**Flag, parse, thread (scope a–c, e)**
- `crates/server/src/cli.rs:2199` (base) → new `:2199-2202`: `cmd_s3` reads `--chunk-size`
  via `parse_s3_chunk_size`, after the credentials and before any backend is resolved or the
  listener bound (bind is at base `:2245`).
- New `parse_s3_chunk_size` at patched `cli.rs:2295-2316`. Absent → `DEFAULT_CHUNK_SIZE`.
  Present → must be digits only, parse as `usize`, and lie in `1..=MAX_CHUNK_SIZE`; otherwise
  `Err("s3: invalid --chunk-size `…`")` or `Err("s3: --chunk-size `…` is out of range; it
  must be 1..=1073741824 bytes")`. Mirrors `cmd_put` (`cli.rs:555-560` base) plus the
  zero/ceiling refusal. The digits-only check is mine: `usize::from_str("+1")` is `Ok(1)`,
  and the rubric's grammar-strictness class names exactly that (`+`/`-` via `from_str`).
- `role started` event gains `chunk_size` (patched `cli.rs:2271`).
- `serve_s3_role` gains `chunk_size: usize` after `endpoints` (base `cli.rs:2306`, patched
  `:2339-2343`), passed on to `serve_s3_dispatch` in both chunk-plane arms (patched
  `:2361`, `:2378`). `serve_s3_dispatch` gains it (patched `:2402`) and **all six** arms call
  `.with_chunk_size(chunk_size)`: patched `:2415, :2423, :2431, :2438, :2445, :2453`.
- Usage line (base `cli.rs:492`, patched `:491`) lists `[--chunk-size N]`.

**One default (scope g)**
- `crates/server/src/lib.rs:51` (base) `const` → `pub const DEFAULT_CHUNK_SIZE` (patched
  `:54`). `crates/server/src/cli.rs:64` (base) deleted; `cli.rs:40` imports
  `DEFAULT_CHUNK_SIZE, MAX_CHUNK_SIZE` from the crate root. `cmd_put`'s default line
  (base `cli.rs:559`) is textually unchanged — it now resolves to the shared constant. Same
  value (`1 << 20`), same acceptance of `0` by `wyrd put`.
- The two `default_chunk_size_is_one_mib` tests (base `lib.rs:774`, `cli.rs:2918`) collapse
  to the `lib.rs` one; the `cli.rs` slot now holds the parse unit test (patched `:2963`).

**Ceiling (settled by the brief, not changed)**
- `pub const MAX_CHUNK_SIZE: usize = 1 << 30` at patched `lib.rs:65`, inclusive, with its
  rationale in the doc comment (`lib.rs:55-64`). Compile-time checks: default ≤ ceiling
  (`lib.rs:67`), and a ceiling-size fragment fits the gRPC message bound (`lib.rs:73-82`).

**Doc comment (scope d)** — `with_chunk_size` (base `lib.rs:144`, patched `:175-179`): it is
the role's configuration seam; the `.max(1)` floor stays as a library-caller guard.

**Transport (new this iteration — carry-forward P1)**
- `crates/chunkstore-grpc/src/lib.rs:38-50`: `pub const MAX_MESSAGE_BYTES = 192 << 20`.
- `crates/chunkstore-grpc/src/client.rs:251` (base) → patched `:256-258`: `GrpcChunkStore::new`
  sets `max_decoding_message_size` and `max_encoding_message_size` to it. `connect` and
  `connect_with_timeout` both go through `new`, so the gateway's fan-out and the custodian's
  fleet clients get it.
- `crates/server/src/dserver.rs:1206` (base) → patched `:1206-1214`: the D server's
  `ChunkStoreServer` sets the same two limits.

**Callers / templates / docs (scope f)**
- `crates/server/tests/s3_gateway_cluster.rs:157-159`: passes `wyrd_server::DEFAULT_CHUNK_SIZE`.
- `deploy/dist/env/s3.env.example:12-16` (comment) and `:23` (`--chunk-size 1048576` on
  the live `WYRD_S3_ARGS`).
- `deploy/small-multi-node-fdb/docker-compose.yml:387,404,421`: all three gateway commands
  gain `"--chunk-size", "1048576"` (the default, named explicitly — brief open question 2).
- `xtask/tests/dist_templates.rs:188-201`: see carry-forward item 2.
- Docs currency: `docs/design/architecture/m4-first-deployment-blueprint.md:1115-1126` (the
  B.5 `wyrd s3` invocation + comment) and a new "Fragment message bound" bullet in
  `docs/design/architecture/08-crosscutting-concepts.md:111` (§8.9), because the gRPC
  message cap changes what the PutFragment/GetFragment RPCs accept.

## Carry-forward — how each item is addressed

1. **C4 diff coverage 0/16 (advisory).** Two separate causes, handled separately:
   - *The frozen report's positions are from a different tree.* The gate log says
     `base_ref: origin/main` (`36f006d`), but this bundle builds on the stack base `4bda59c`,
     and `git diff --stat origin/main HEAD -- crates/server/src/cli.rs` = **112 inserted
     lines** (from #839/#840). So every `cli.rs` line number in that report is off. Example:
     it lists `cli.rs:2199-2200` as missed code, but on the patched tree those are comment
     lines. That is a gate/harness defect (diff-cov should use the same base C4-verify
     uses), not something a patch can fix. **Proposed for the human:** file it against the
     run-diff-cov script / harness so `C4-diff-cov` diffs against the bundle's stack base.
   - *Lines that run only in a killed child are never profiled.* `wyrd s3` has no graceful
     shutdown; the test must kill it, and a killed process never writes its `.profraw`.
     Adding a shutdown path to the role is out of scope. What I did instead, all real
     behaviour, no coverage-only code: (a) the D server now runs **in the test process**
     (production `DServer::serve`), so `dserver.rs`'s changed lines are profiled; (b) a
     non-numeric/signed refusal leg and a usage-line leg, both of which exit normally.
   - **Measured** with `cargo llvm-cov test -p wyrd-server --test s3_chunk_size_flag` in a
     scratch target dir, on the correct base, final test file: `dserver.rs` 4/4, `cli.rs`
     12/22, `lib.rs` 0 instrumentable (consts and doc comments). **16/26 = 62% for
     `wyrd-server`** — still under the 80% floor. The 10 lines left are exactly the
     ones that run only while the role serves: `cli.rs:2279, 2303, 2314, 2315, 2344, 2361,
     2378, 2402, 2414, 2415` (the default-return, the `Ok(chunk_size)`, the threading, and
     the redb+mem `with_chunk_size`). They are proven by behaviour instead: every one of
     them is on the path the chunk-count assertions observe, and those go red when the fix
     is reverted (below). The `chunkstore-grpc` lines (`client.rs:256-258`) are scored by
     that crate's existing suite, which calls `GrpcChunkStore::connect` → `new`.
2. **Template assertion could not go red.** I did NOT add `--chunk-size` to the whole-file
   `contains` list (that was the hollow part). `xtask/tests/dist_templates.rs:188-201` now
   finds the `WYRD_S3_ARGS=` line and asserts `--chunk-size` is on it followed by a numeric
   value. **Reproduced red:** with ` --chunk-size 1048576` deleted from `s3.env.example:23`
   and the comment block left in place (grep still finds the flag once), the test fails:
   `s3.env.example's WYRD_S3_ARGS must pass --chunk-size <bytes>`. Restored → green.
3. **P1: accepted chunk sizes exceeded the cluster transport limit.** Fixed by making the
   transport carry the whole advertised range (the reviewer's requested fix; the ceiling
   itself is the brief's settled value, so lowering it was not mine to do). See "Transport"
   above and "Sizing the cap" below. Covered by a new PUT **and** GET above the old 4 MiB
   boundary through the gRPC composition: `a_chunk_past_the_4_mib_grpc_default_round_trips_through_a_d_server`
   (32 MiB chunk → ~5.3 MiB fragments; the reviewer's repro value).
4. **P2: pin the test's backends.** `spawn_role` always passes `--metadata-backend redb
   --coordination-backend mem` (flags win over `WYRD_METADATA_BACKEND` /
   `WYRD_COORDINATION_BACKEND`, `cli.rs:287-289, 377-379`).

The deferred-to-sign-off item that asked the same transport question as a human decision
("lower the ceiling, raise both tonic limits, or document") is now answered by option 2. The
same deferred note said the ceiling doc counted only PUT memory: fixed — `lib.rs:55-64` now
also states the GET figure (4 chunks buffered in the read channel, `lib.rs:411,515` on
base, plus the one being read → ~5 GiB per in-flight GET at 1 GiB).

## Sizing the cap — and the alternatives I rejected

Largest message the `s3` role can produce at the ceiling: one RS(6,3) shard of a 1 GiB chunk
= `ceil(2^30 / 6)` rounded up to the coder's 64-byte alignment (`crates/core/src/erasure.rs:16,79-82`)
= 178,956,992 bytes, plus the 44-byte header and 4-byte CRC trailer
(`crates/chunk-format/src/codec.rs:32-61`) = 178,957,040, plus < 64 bytes of protobuf
envelope ≈ **170.7 MiB**. The reviewer's observed 32 MiB message was 5,592,514 bytes, which
matches this formula exactly (5,592,448 + 48 + 18). I chose **192 MiB**: covers the ceiling
with ~21 MiB slack, and `lib.rs:73-82` fails the build if `MAX_CHUNK_SIZE` or the default
`k` ever change so that it no longer fits.

Rejected:
- **Cap = 1 GiB + slack** (covers `EcScheme::None` / `k=1` at the ceiling). The `s3` role
  always uses `DEFAULT_DURABILITY` (no `with_durability` call in `serve_s3_dispatch`), so
  that extra headroom serves only `wyrd put --durability … --chunk-size …`, whose chunk size
  is explicitly out of scope. It would let one request pin 1 GiB on a D server instead of
  192 MiB — 5.3× the per-request memory for no in-scope gain. Same diff size either way.
- **A shared helper in `chunkstore-grpc` (e.g. `ChunkStoreService::into_server()`)** that
  applies the server limits. ~12 lines in `server.rs` vs the 3 lines at `dserver.rs:1210-1213`,
  for one production host. The single constant already ties both halves together.
- **Making the cap configurable** (`wyrd d-server --max-message-bytes`). A new flag on a
  different role, its own docs/tests; not needed for the role this issue is about.
- **Lowering the ceiling to ≤ ~24 MiB or refusing large values only with `--endpoints`.**
  The brief settles 1 GiB and names the accepted/rejected pair in the criterion; changing it
  is a Plan decision. Not done.

## Review points to state up front

- **Public API changes (deliberate).** `serve_s3_role` (`pub`) gains a parameter; its only
  caller outside the module is `crates/server/tests/s3_gateway_cluster.rs:153`, updated.
  New `pub const`s: `wyrd_server::DEFAULT_CHUNK_SIZE` (was private), `wyrd_server::MAX_CHUNK_SIZE`,
  `wyrd_chunkstore_grpc::MAX_MESSAGE_BYTES`. All additive; no existing value changes.
- **D server memory.** Before: one request could make a D server decode at most 4 MiB.
  After: at most 192 MiB. With the default admission bound of 64 concurrent requests
  (`dserver.rs:61`) the theoretical worst case is ~12 GiB of request buffers — reached only
  if clients actually send near-ceiling fragments. This is the cost of supporting the
  brief's 1 GiB ceiling over gRPC. If that is unwelcome, the lever is the ceiling (a Plan
  decision), not this cap.
- **A fleet can disagree on chunk size.** Each gateway uses its own `--chunk-size`. That is
  benign: the chunk map is stored per object and reads follow the map, not a process-wide
  assumption, so any gateway reads objects any other wrote. A very small chunk size grows the
  chunk map faster, which meets the segmented-root ceiling #739 tracks — one more reason the
  CLI refuses rather than clamps.
- **Ceiling.** I agree with 1 GiB as a refusal bound but note it is far above what anyone
  should run (~1.5 GiB per PUT, ~5 GiB per GET, 192 MiB D-server messages). Reported, not
  changed.

## Verification — red/green (cargo test, the brief's command, under `timeout`)

Command: `cargo test -p wyrd-server --test s3_chunk_size_flag` (6 tests, ~5 s green).

| Tree | Result |
|---|---|
| Fix applied | **6/6 pass** |
| Production fully reverted (`crates/server/src/{cli,lib,dserver}.rs`, `crates/chunkstore-grpc/src/{client,lib}.rs` at HEAD; tests kept) | **6/6 fail on assertions, no hang** (5 checked in one run before the usage leg existed; the usage leg checked separately): `--chunk-size 524288` → 2 chunks not 4; `0` / `1MiB` / `1073741825` → role serves instead of refusing; 32 MiB over gRPC → 32 chunks not 1; usage line lacks `[--chunk-size N]` |
| Only the transport reverted (client.rs + dserver.rs at HEAD) | gRPC test fails: PUT → **500** InternalError; the other 5 pass |
| Only the client limit reverted | gRPC test fails: PUT 200, GET returns **0 of 33,554,432 bytes** |
| Template test, flag removed from live `WYRD_S3_ARGS` only | **fails**; restored → passes |

Also run green: `cargo test -p wyrd-server --lib chunk_size` (3 unit tests),
`--test s3_gateway_cluster`, `cargo test -p wyrd-chunkstore-grpc` (all suites).

### Refute-my-own-test (forced)

- **(a) Genuine red?** Yes. With every production file reverted to HEAD and the test file
  kept, all tests compile (the file uses no symbol the fix adds — the binary via
  `env!("CARGO_BIN_EXE_wyrd")`, plus `DServer`, `DSERVER_GROUP`, `FsChunkStore`,
  `MemCoordination`, `sigv4::sign`, all on base) and every test fails on its assertion in
  ~2.5 s. Partial reverts isolate the transport half (PUT 500) and the client half (GET
  truncated). Evidence in the table above.
- **(b) Production path?** Yes. The gateway is the real `wyrd` binary started as `wyrd s3`
  (`cmd_s3` → `serve_s3_role` → `serve_s3_dispatch` → `Gateway::with_chunk_size`). The D
  server is the production `DServer::bind/register/serve` (which builds the
  `ChunkStoreServer` at `dserver.rs:1206-1214`), not a test-built `ChunkStoreServer::new`.
  The client limit is exercised inside the gateway child through `connect_fanout` →
  `GrpcChunkStore::connect` → `new`. Requests are signed with the production `sigv4::sign`.
- **(c) Fixture includes the fault?** Yes. The failing element is the size of the payload
  relative to the limits: the gRPC leg uses a 32 MiB chunk whose fragments (~5.3 MiB)
  exceed tonic's 4 MiB default on both the D server's decode (PUT) and the gateway's decode
  (GET); the one-D-server fixture still fans out all nine fragments (they stack on that
  server), so every fragment crosses the wire. The chunk-count legs count real
  `FsChunkStore` directories written by the role. The refusal legs drive the exact
  boundary values (`0`/`1`, `1073741824`/`1073741825`).

## Commit-readiness

- `cargo fmt --all -- --check`: clean.
- `cargo clippy --workspace --exclude wyrd-dst --all-targets`: clean.
- `cargo clippy -p wyrd-server --features tikv,etcd,fdb --tests`: clean — compiles all six
  `serve_s3_dispatch` arms (toolchains present on this host: protoc 3.21.12, cmake 4.2.3,
  libfdb_c + headers).
- madsim: `madsim-tonic-build` 0.6.0's generated client/server stubs define
  `max_decoding_message_size` / `max_encoding_message_size` as no-ops
  (`madsim-tonic-build-0.6.0+0.14/src/client.rs:87-95`, `server.rs:61-69`), so `client.rs`
  still compiles under `--cfg madsim`.
- Full gate `./engine/xtask.sh ci` on the worktree (typos, docs, gitlink/unsafe guards,
  fmt, clippy, build, workspace tests, machete, deny, conformance, statics, madsim DST
  clippy + 50-seed tests): **`xtask ci: all checks passed`, exit 0**. Its run included the
  final 6-test `s3_chunk_size_flag`, the new `cli.rs`/`lib.rs` unit tests and the updated
  `env_examples_name_every_load_bearing_flag`.
- `patch.diff` reverse-applies cleanly to the worktree (`git apply --check -R`), i.e. it is
  exactly the worktree's change against `4bda59c`; 12 files.

## Scratch

Scratch used: `$PDCA_SCRATCH/pdca-builder-738-{redleg,cov,ci}` (file copies for the revert
legs, an instrumented `target/` for llvm-cov, the gate log). The cov dir holds an
instrumented build of several GB; the harness owns cleanup of `$PDCA_SCRATCH`.
