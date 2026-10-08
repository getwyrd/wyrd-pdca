# Adversarial review — issue 738 (`wyrd s3 --chunk-size`)

Bottom line: I could not break the red→green proof or the 16 MiB ceiling. I reproduced
both on my own scratch copy. Two judgment calls remain, both about inputs the role still
accepts when it should not, plus one claim in the brief that overstates what leg (C) guards.

## Findings

- NEEDS-HUMAN [human] — **`--chunk-size=N` and misspellings still start the role silently
  at 1 MiB.** `crates/server/src/cli.rs:2652-2662` (`ParsedArgs::parse`) stores
  `--chunk-size=524288` as an unknown flag named `chunk-size=524288`, and that flag takes
  the NEXT argument as its value. `cmd_s3` only looks up `chunk-size`
  (`cli.rs:2211`), so it sees nothing. Reproduced on the built patched binary:
  `wyrd s3 --access-key k --secret-key s --s3-listen 127.0.0.1:0 --data-dir D
  --metadata-backend redb --coordination-backend mem --chunk-size=524288 --region zone-a`
  printed `serving S3-compatible HTTP on …` and `SigV4 required (region us-east-1 …)`. So
  the role runs 1 MiB chunks AND the wrong region, because `--region` was eaten as the
  value. `--chunksize 524288` (typo) also starts at 1 MiB. The brief's Defect calls this
  silent acceptance "worse than missing", and its Invariant says an operator's setting is
  "honoured … or refused when the role starts". The patch closes only the exact
  `--chunk-size N` spelling. This is not a regression: every flag already behaves this way,
  e.g. `--region=zone-a`. A real fix means `cmd_s3` rejects unknown flags and stray
  positionals, which touches every s3 flag and falls outside scope items (a)–(h). Scope
  call: accept and file a follow-up, or widen this slice.

- NEEDS-HUMAN [human] — **The floor of `1` is accepted, but tiny chunk sizes are too slow
  to use.** `cli.rs:2487` accepts `1`. On a debug build with the local-FS plane, a role at
  `--chunk-size 1` returned 200 for PUTs of 4,096 and 81,000 bytes. A 200,000-byte PUT got
  no response within 120 s: the read timed out with `Resource temporarily unavailable`.
  Every byte becomes its own RS(6,3) chunk, which means 9 fragment files per byte. The
  brief kept the floor at 1 based only on the chunk-map ceilings it hands to #739. This is
  a different cost, and it hits the same invariant: the role starts, then a modest PUT does
  not finish. If #739 is meant to cover every small-chunk cost, treat this as settled.
  Otherwise a human should decide whether to raise the floor (for example to 4 KiB). Low
  priority. A release build will be faster, but the cost still grows with object size
  divided by chunk size.

- **The brief overstates leg (C).** The brief says "(C) is also the leg that goes red if
  the ceiling is ever set past what the transport carries." The test as written does not
  do that. Leg C hard-codes `--chunk-size 16777216` and a 16 MiB object
  (`crates/server/tests/s3_chunk_size_flag.rs:599,604`), so raising `MAX_CHUNK_SIZE`
  (`crates/server/src/lib.rs:70`) to, say, 32 MiB leaves C green. Only leg B's exact
  boundary (`s3_chunk_size_flag.rs:492`, where `16777217` must be refused) goes red.
  Someone who raises the ceiling and updates B's numbers gets no transport proof unless
  they also move C. The template test hard-codes a second copy of the ceiling
  (`xtask/tests/dist_templates.rs:212`). C IS a real transport probe: in my scratch copy,
  with the ceiling at 32 MiB and C moved to a 24 MiB chunk and object, the PUT returned
  `500 InternalError`. The brief forbids importing the constant into the test, so this is
  a wording point, not a code defect. A one-line note on `MAX_CHUNK_SIZE` saying "move leg C
  with this value" would close it.

- **The C4-diff-cov fail is a measurement artifact, not missing tests.** The coverage
  runner measured only `--test s3_chunk_size_flag` (see `gate-logs/C4-diff-cov.log`). That
  test runs the role in a child process, which never writes a coverage profile. The
  in-process unit tests that run `parse_s3_chunk_size`, `check_s3_chunk_size`,
  `compose_s3_gateway` and the `serve_s3_role` refusal (`cli.rs:3005-3084`) show up as
  UNSCORED. The MISS lines (`cli.rs:2480-2498` and others) are exactly what those tests
  run. I ran them: 3 passed.

## Attacks that failed

- **The red→green proof reproduces independently.** On a scratch copy of `$PDCA_TARGET`,
  the patched tree passes 4 of 4 tests. With production code reverted (`cli.rs`, `lib.rs`,
  `s3_gateway_cluster.rs`) and the new test kept, it fails 4 of 4, at the same lines and
  for the same reasons as `gate-logs/C4-verify.log`: A gets 2 chunks instead of 4, B's `0`
  starts serving, C gets 16 chunk directories instead of 1, and D has no `[--chunk-size N]`.
  The test drives the built binary and a production `DServer::bind`/`serve`
  (`crates/server/src/dserver.rs:729,933`, with `ChunkStoreServer::new` at `:1206`). No
  gRPC size override exists anywhere in the tree, so the test is not a parallel copy of
  production.
- **All six dispatch arms compile.** `cargo check -p wyrd-server --features fdb,etcd,tikv
  --tests` passes here (FDB headers present). CI never built the fdb arms; `host-tikv`
  covers only tikv and etcd. Every arm goes through `compose_s3_gateway` (`cli.rs:2451`),
  and `None` there is plain `Gateway::new`, the same as before.
- **A mixed fleet, or a restart with a different size, reads old objects correctly.** The
  gateway reads `self.chunk_size` only on its two write paths (`lib.rs:204`, `:344`). GET
  walks the object's stored chunk map.
- **The ceiling's arithmetic and transport both hold.** A 16 MiB chunk gives fragments of
  about 2.67 MiB, under tonic's 4 MiB default. A 16 MiB PUT and GET cross a real D server
  with byte-equal bodies (leg C, green here). A 24 MiB chunk fails with 500, as shown above.
- **Edge inputs are refused correctly.** A trailing `--chunk-size` with no value exits 2
  with "flag `--chunk-size` needs a value". `0x100`, `-5`, `""`, `1MiB` and 2^64 are
  refused with the flag named. `+524288` is accepted, which matches the brief's
  `str::parse::<usize>` contract, so I did not raise it. A repeated flag uses the last
  value, which is the documented convention (`deploy/dist/systemd/wyrd-s3.service:22`).
- **The refusal comes before bind.** `parse_s3_chunk_size` runs at `cli.rs:2211`, before
  the runtime starts and the listener binds (`cli.rs:2247`, `:2257`). `serve_s3_role`
  checks the range again (it is `pub`), and a unit test pins that check.
- **C5-mutants is not adjudicable.** The log reports "2 caught, 8 unviable" but does not
  name the mutants, so I cannot judge it either way.
