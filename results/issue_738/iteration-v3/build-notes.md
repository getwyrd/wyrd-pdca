# Build notes — issue 738 / s3-role-chunk-size-flag (iteration 3)

## Base I built on

`$PDCA_WORKTREE` HEAD = `4bda59c` (the pdca-integration stack: `origin/main` `36f006d` plus the
folded #774, #839, #840) — the same base as iteration 2. **#736 is NOT on this base**: the `s3`
role's `role started` event carries no `version` field (`crates/server/src/cli.rs:2265-2273`)
and `serve_s3` sets only `config.region` (`cli.rs:2483-2484`). So I added no `version` field.

I started from the iteration-2 patch (it applied cleanly to `4bda59c`) and changed only what
the carry-forward asked for, plus two wording fixes from the deferred list. Iteration-2 work
that no reviewer objected to (flag parsing, threading through all six arms, the constant
collapse, the templates) is unchanged.

## What changed this iteration (path:line on the patched tree, base `4bda59c`)

1. **Strict response parsing in the new test** (T4 blocking + T5 + P2 parser finding).
   `crates/server/tests/s3_chunk_size_flag.rs:283-418`: `signed_request` now returns through
   `parse_response` (`:304`), which requires `HTTP/1.1 <3 digits> `, rejects a header line
   without a colon or with whitespace in its name, requires exactly ONE framing —
   `content-length` (digits only, no duplicate, and exactly that many bytes, no fewer and no
   more) or `transfer-encoding: chunked` — and refuses an unframed body. `dechunk` (`:363`)
   checks every delimiter: hex-only size (no `+`), only `;ext` after the size, CRLF after each
   chunk's data, a trailer section ending in the empty line, and nothing after it. Two new
   pure tests pin it: `response_parser_accepts_well_framed_responses` (`:624`) and
   `response_parser_refuses_torn_or_malformed_framing` (`:651`, 22 cases, including the
   reviewer's `1\r\naxx0\r\n` and the missing final CRLF).
2. **One bounded-server constructor, used by every mount** (DST fidelity finding).
   `crates/chunkstore-grpc/src/server.rs:65-91`: `ChunkStoreService::into_server()` returns
   the tonic server with both message limits set to `MAX_MESSAGE_BYTES`. Used at:
   production `crates/server/src/dserver.rs:1209`; the four DST models
   `crates/dst/tests/network.rs:234,662,843,987`; and every other in-repo fixture that mounted
   the bare server — `crates/chunkstore-grpc/tests/{round_trip.rs:66,list_delete.rs:62,
   error_class.rs:114,read_fault_seam.rs:144,write_deadline.rs:366}`,
   `crates/server/tests/{gateway_cluster.rs:52,closed_write_path.rs:92,s3_gateway_cluster.rs:63}`,
   `crates/core/benches/throughput.rs:81`. After this, `grep ChunkStoreServer::new` finds only
   `into_server` itself and one doc comment. Re-export doc updated
   (`crates/chunkstore-grpc/src/lib.rs:33-38`).
   - **Correction to the finding's premise**, verified in the vendored source: under madsim
     the generated `max_decoding_message_size` / `max_encoding_message_size` are no-ops
     (`madsim-tonic-build-0.6.0+0.14/src/server.rs:61-71`,
     `madsim-tonic-0.6.0+0.14/src/client.rs:196-205`) and the sim transport passes messages
     in memory. So the DST models never had a 4 MiB limit, before or after this patch, and
     "a DST scenario over ~24 MiB would get `OUT_OF_RANGE`" cannot happen. I made the change
     anyway because it removes the cause the reviewer named (each host had to remember the
     limit), and I said so in `into_server`'s doc comment (`server.rs:78-80`): DST does not
     model this bound; the real-tonic tests below do.
   - madsim's generated `ChunkStoreServer` has a second type parameter (`ChunkStoreServer<T, F>`,
     `target/.../out/sim/wyrd.v0.rs:326`), so the return type is a private alias chosen by
     `cfg(madsim)` (`server.rs:65-70`), following the existing paired-cfg precedent at
     `crates/chunkstore-fs/src/lib.rs:236,255`. My first attempt without it failed the gate's
     madsim clippy step (`E0107`); this version passes it.
3. **Transport test where the transport lives.** `crates/chunkstore-grpc/tests/round_trip.rs:112-148`
   `a_fragment_past_the_4_mib_grpc_default_round_trips`: a 6 MiB fragment PUT and GET through
   `GrpcChunkStore` and an `into_server` mount. Fast (0.1 s), in-process, real tonic.
4. **Rolling-upgrade order documented.** `docs/design/architecture/08-crosscutting-concepts.md:111`
   (the §8.9 "Fragment message bound" bullet, next to the `W_write` mixed-version note the
   reviewer pointed at): every D server, gateway and custodian must run a binary with the bound
   before any gateway sets `--chunk-size` above about 24 MiB; until then an old D server
   refuses those PUTs and an old gateway or custodian cannot read them back. Same warning in
   `deploy/dist/env/s3.env.example:12-20` and
   `docs/design/architecture/m4-first-deployment-blueprint.md:1117-1121`.
5. **PUT memory guidance corrected.** The old "~1.5×" was wrong. `crates/core/src/write.rs:574`
   keeps the `chunk_size` buffer alive while `lease_write_chunk` awaits, and
   `write.rs:428-453` holds all nine RS(6,3) fragments (~1/6 chunk each) until
   `try_join_all` finishes: at least ~2.5× before encoder and gRPC copies. Fixed in
   `crates/server/src/lib.rs:55-65`, `s3.env.example:12-15`, `m4-first-deployment-blueprint.md:1118`.
   The settled 1 GiB ceiling is unchanged.
6. **Deferred findings 7 and 8 (human decisions) — facts added, nothing decided.**
   "Every accepted value is transportable" overclaimed (size only, not time): now "fits one
   ChunkStore gRPC message" (`lib.rs:64-65`, the const-assert comment `lib.rs:69-74`,
   `chunkstore-grpc/src/lib.rs:50-52`). The §8.9 bullet now also states the aggregate worst
   case (64 admitted requests × 192 MiB ≈ 12 GiB per D server; `dserver.rs:61`) and that the
   D server's 30 s (`dserver.rs:73`) and the custodian's 10 s (`cli.rs:863`) per-request
   timeouts still bound transfer time. Whether to keep 1 GiB, make the D-server cap
   configurable, or lower the ceiling stays the human's call.
7. **Template assertion tightened.** `xtask/tests/dist_templates.rs:196-201`: the value after
   `--chunk-size` must be digits only (`u64::from_str` alone accepts `+1048576`). Red with
   `+1048576` and red with the flag removed from the live `WYRD_S3_ARGS=` line (comment block
   left in place); green on the shipped file.

## Carry-forward — each item

| Item | Disposition |
|---|---|
| T4 blocking: `dechunk` accepts a truncated terminator | **Fixed** (item 1). |
| T5 / P2: response framing (`Content-Length` ignored, CRLFs unchecked); "cover rejection" | **Fixed** with 22 rejection cases (item 1). Red legs below show both a truncated GET and the reviewer's malformed chunked input now fail. |
| DST models don't match production | **Fixed as asked** (item 2), with the premise corrected: madsim never enforced the limit. |
| Rolling-upgrade order undocumented | **Fixed** (item 4). |
| P2: PUT memory ~1.5× is wrong | **Fixed** (item 5). |
| C4 diff coverage (advisory) — "decide how to discharge" | **Measured on the right base; two harness causes reported.** See next section. Not something a patch can make the gate report correctly. |

## C4 diff coverage — what is real and what is the harness

**Measured on the correct base** (`cargo llvm-cov` in this worktree, numbering = `git diff -U0
HEAD` against `4bda59c`, script in `$PDCA_SCRATCH/pdca-builder-738-cov/diffcov.py`):

- `wyrd-server`, scored by `--test s3_chunk_size_flag` exactly as the gate does: **13/23**
  instrumentable changed lines (56.5%).
- `wyrd-chunkstore-grpc`, scored by its own suite: **8/8** (`client.rs:256-258`,
  `server.rs:82-86`).
- Together **21/31 = 67.7%**.

The 10 missed `wyrd-server` lines are all on the serving path inside the `wyrd s3` child:
`cli.rs:2279` (arg to `serve_s3_role`), `:2303` (absent ⇒ `DEFAULT_CHUNK_SIZE`), `:2314-2315`
(`Ok(chunk_size)`), `:2344`, `:2361`, `:2378`, `:2402` (threading), `:2414-2415` (redb+mem
`with_chunk_size`). They run — the chunk-count assertions observe their effect, and those
assertions go red when the fix is reverted (table below) — but the child blocks forever and the
test must SIGKILL it, and LLVM writes a `.profraw` only at normal exit. The refusal legs exit
normally, which is why the parse/refuse lines are HIT.

**Why the gate's figure is wrong regardless (harness defect, proposed for eduralph/pdca-harness):**
`run-diff-cov.sh:685` asks `run-verify.sh --print-base`, which returns `origin/main` for this
bundle (I ran it: `PDCA_BUNDLE=…/issue_738 ./engine/scripts/run-verify.sh --print-base` →
`origin/main`), even though `stack-base` names `pdca-integration/r-a834…/main`, which resolves
locally (`git -C ~/wyrd/wyrd rev-parse --verify …` → `4bda59c`). The coverage checkout
`~/wyrd/wyrd-cov-l0` is at `36f006d`. The patch is numbered against `4bda59c`; on `36f006d`,
`cli.rs` is 25 lines shorter before `cmd_s3` (`git diff origin/main HEAD -- crates/server/src/cli.rs`
hunks +1, +5, +2, +17), so the gate looks up each changed line 25 lines away from where it is.
That is why last round's MISS list named comment lines. Patch content can't fix that.

**Rejected ways to raise the number (with cost):**
- *In-process leg via `wyrd_server::cli::run`* (pub on base, `cli.rs:398`, so it would not break
  the red leg). ~45 test lines; `run` never returns for `s3`, so the test would leak a live
  server thread per call, would have to pre-pick a port by bind-then-drop (a race with the
  other legs' ephemeral binds), and the in-process `eprintln!` listen line is captured by the
  test harness, so readiness would be a connect-poll. It duplicates the binary leg only to feed
  the profiler. Not done.
- *Graceful Ctrl-C shutdown for the `s3` role* so the child exits normally: ~8 lines of
  `tokio::select!` in `cmd_s3` plus an external `kill -INT` in the test. That is a production
  behaviour change (exit status, in-flight request handling) outside every scope item (a)–(g).
  Not done; it would be its own issue.
- *LLVM continuous mode (`%c` in `LLVM_PROFILE_FILE`)*: needs `-runtime-counter-relocation` in
  the instrumented build, which the gate does not set. Not done.

## Other alternatives considered this round

- **Reuse the existing parser** `crates/server/src/consistency_observable.rs:440` (rubric:
  prefer extending a shared parser). It is private; making it `pub` would make the new test
  reference a symbol this patch adds, so the C4-verify red leg would not compile → UNVERIFIABLE
  (`run-verify.sh:201-215`; brief §Falsifiability). It is also laxer than this slice needs:
  `from_str_radix` takes `+1`, an unframed body is accepted, both framings at once are not
  refused. A shared strict test-support parser would be a separate issue.
- **A new test file in `chunkstore-grpc` for the transport.** C4-verify runs every ADDED
  `*/tests/*.rs` together in the red leg (`run-verify.sh:404-413`); a new file calling
  `into_server` would not compile with production reverted and would turn the whole verify
  UNVERIFIABLE. So the transport test went into the existing `round_trip.rs`, which the red
  leg reverts with the rest.
- **Switch only `dserver.rs` + the four DST mounts** (what the finding literally named): 5 sites
  in 2 files. I switched all 14 sites in 11 files (+9 one-line call changes, +8 import edits).
  The extra cost is mechanical; the gain is that no in-repo host mounts the unbounded server,
  which is the cause the reviewer named, and `chunkstore-grpc`'s own suite now exercises
  `into_server` (its 8/8 above).
- **Testing the bound's upper edge** (a message over 192 MiB is refused): needs a >192 MiB
  fragment built in memory, plus an encode copy, in a unit suite. Not done; the bound is the
  tonic setting itself.

## Review points to state up front

- **Public API (deliberate, all additive):** `serve_s3_role` (`pub`) gains `chunk_size`
  (`cli.rs:2344`); its one outside caller `crates/server/tests/s3_gateway_cluster.rs:158-159` is
  updated. New `pub`: `wyrd_server::DEFAULT_CHUNK_SIZE` (was private), `wyrd_server::MAX_CHUNK_SIZE`,
  `wyrd_chunkstore_grpc::MAX_MESSAGE_BYTES`, `ChunkStoreService::into_server`.
- **A fleet can disagree on chunk size.** Each gateway uses its own `--chunk-size`. Benign: the
  chunk map is stored per object and reads follow the map, so any gateway reads what any other
  wrote. A very small chunk size grows chunk maps faster, which meets the segmented-root ceiling
  #739 tracks — one reason the CLI refuses rather than clamps.
- **Out of scope, noted:** `deploy/small-multi-node/docker-compose.yml:393,408,423` (the TiKV
  stack) also runs `wyrd s3`; the brief names only the three `small-multi-node-fdb` commands,
  so I left it. The `s3` role has no graceful shutdown (see coverage section).
- **Ceiling:** I still think 1 GiB is far above anything a deployment should run (≥2.5 GiB per
  PUT, ≥5 GiB per GET, 192 MiB D-server messages). Reported, not changed — the brief settles it.

## Verification — red/green

Runner: `cargo test` under `timeout` for the targeted legs (the brief's command), and the
project's gate `./engine/xtask.sh ci` for the whole tree.

| Tree | Result |
|---|---|
| Patch applied | `s3_chunk_size_flag`: **8/8 pass** (4.9 s). `round_trip`: **7/7 pass**. |
| All modified files reverted to `4bda59c`, new test kept (what C4-verify does) | **6 product tests FAIL on assertions** in 2.5 s, no hang: `--chunk-size 524288` → 2 chunks not 4; `0`, `1MiB`, `1073741825` → role serves instead of refusing; 32 MiB over gRPC → 32 chunks not 1; usage line lacks `[--chunk-size N]`. The 2 parser self-tests pass (they test the helper, not the product). |
| Both message limits removed, API kept (`into_server` → bare `new`; client bare) | gRPC leg: PUT → **500 InternalError**. `round_trip` new test: **`OutOfRange` "found 6291533 bytes, the limit is: 4194304"**. |
| Only the client limit removed | gRPC leg: PUT 200, GET → **"declared content-length 33554432, but 0 body bytes arrived"** — the truncation is now a framing failure, as T5 asked. `round_trip`: fails on the get. |
| Parser weakened: skip the post-chunk CRLF check | `response_parser_refuses…` **FAILS** on `1\r\naxx0\r\n\r\n` → `Ok([97])`. |
| Parser weakened: skip the content-length length check | **FAILS** on a 3-byte body declared as 4. |
| Template: `+1048576`, or flag removed from the live line only | `env_examples_name_every_load_bearing_flag` **FAILS** both ways; green on the shipped file. |

Every mutated file was restored from a saved copy; `git diff HEAD` was byte-compared against the
saved diff after each leg (`RESTORED IDENTICAL`).

### Refute-my-own-test (forced)

- **(a) Genuine red?** Yes. With every production file reverted and the test kept, all 6
  product tests compile (the file uses only base symbols: the binary via
  `env!("CARGO_BIN_EXE_wyrd")`, `DServer`, `DSERVER_GROUP`, `FsChunkStore`, `MemCoordination`,
  `sigv4::sign`) and fail on their assertions. Partial reverts isolate the server-side and the
  client-side limit separately (table rows 3-4).
- **(b) Production path?** Yes. The gateway is the real `wyrd` binary started as `wyrd s3`
  (`cmd_s3` → `parse_s3_chunk_size` → `serve_s3_role` → `serve_s3_dispatch` →
  `Gateway::with_chunk_size`). The D server is production `DServer::bind/register/serve`, which
  mounts via `into_server` (`dserver.rs:1209`). The gateway's client limit is exercised inside
  the child through `connect_fanout` → `GrpcChunkStore::connect` → `new`. The transport test in
  `round_trip.rs` drives the shipped `GrpcChunkStore` against an `into_server` mount over real
  loopback HTTP/2. The parser tests exercise the test's own helper, which is what they claim.
- **(c) Fixture includes the fault?** Yes. The chunk-count legs count real `FsChunkStore`
  directories the role wrote. The gRPC legs use payloads past the 4 MiB default (32 MiB chunk →
  ~5.3 MiB fragments; a 6 MiB fragment), so the old limit is actually crossed in both
  directions; the single-D-server fixture still receives all nine fragments over the wire. The
  refusal legs drive the exact boundary pairs (`0`/`1`, `1073741824`/`1073741825`). The parser
  cases include the reviewer's exact malformed input.

## Commit-readiness

- `cargo fmt --all -- --check`: clean.
- `./engine/xtask.sh ci` (typos, docs lint/render, gitlink/unsafe guards, fmt, clippy, build,
  workspace tests, machete, deny, statics, deploy-guard, madsim DST clippy, madsim DST tests):
  **`xtask ci: all checks passed`, exit 0** (log: `$PDCA_SCRATCH/pdca-builder-738-ci/ci2.log`).
  The first run failed only at madsim clippy (`E0107`, item 2) and passed everything before it.
- The template tightening (item 7) came after that run: `cargo fmt --check`, `cargo clippy -p
  xtask --all-targets` and the `dist_templates` test re-run clean.
- Not re-run this round: `cargo clippy -p wyrd-server --features tikv,etcd,fdb`. The six
  feature-gated `serve_s3_dispatch` arms are unchanged since iteration 2, where that build was
  clean; the one `dserver.rs` change is not feature-gated.
- `patch.diff` reverse-applies to the worktree (`git apply --check -R`) and applies to a clean
  `4bda59c` export. It also applies to `origin/main` `36f006d` (what the coverage gate uses);
  there two identical-context `cli.rs` hunks land at offsets −9/−42 instead of −25, and I
  checked the resulting `serve_s3_role` is byte-identical to the worktree's.

## Scratch

`$PDCA_SCRATCH/pdca-builder-738-{redleg,ci,cov,applycheck,applymain}`: saved diff and file
copies for the revert legs, gate logs, an instrumented `target/` for llvm-cov (several GB), and
two source exports for the apply checks. Left for the harness to reclaim.
