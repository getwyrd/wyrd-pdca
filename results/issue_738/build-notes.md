# Build notes — issue 738 / s3-role-chunk-size-flag (iteration 4, re-plan)

## Base

Built on the run's folded integration branch, `pdca-integration/r-a834e98f67b952c61db7a03084c626e8/main`
at `4bda59c` (`pdca-integrate: issue_840`). It is `main` @ `36f006d` plus #774, #839, #840.
Only #839 touches `crates/server/src/cli.rs`, and as the brief predicted it shifts `cmd_s3`
down by 25 lines (`cmd_s3` is `cli.rs:2151` on `main`, `cli.rs:2176` on this base before my
change, `cli.rs:2183` after it). Nothing from #736/#778/#779 is on the base. All `path:line`
below are on the patched tree unless marked "base".

## What changed and why

**The ceiling, stated once, beside the default** — `crates/server/src/lib.rs:52-70`.
`MAX_CHUNK_SIZE = 16 << 20` (`pub(crate)`) sits directly under the gateway's
`DEFAULT_CHUNK_SIZE` (`lib.rs:52`, the default `wyrd s3` actually runs, applied at
`lib.rs:119`). Its doc comment gives the reason: one fragment per unary gRPC message,
tonic's 4 MiB receive limit on both receivers, 24 MiB fails, 16 MiB leaves ~1.3 MiB of
headroom, k=6 dependence, one ceiling for both planes. `pub(crate)`, not `pub`: nothing
outside the crate needs it, and the binding test must not import it anyway. The
`DEFAULT_CHUNK_SIZE` doc (`lib.rs:50-51`) no longer says "tests override it"; it says
what `wyrd s3` runs with no flag.

**`with_chunk_size` doc** — `lib.rs:163-169`. Now says it is the `wyrd s3` role's
configuration seam, that the role refuses out-of-range values before it starts, and that
the builder itself only floors `0` to `1`. The `.max(1)` is untouched (`lib.rs:171`), as
the brief requires.

**Parse + refuse in `cmd_s3`, before the runtime or listener exists** —
`cli.rs:2207-2211`. `parse_s3_chunk_size(parsed.flag("chunk-size"))?` runs among the
other flag reads, before `tokio_runtime()` and long before `TcpListener::bind`
(`cli.rs:2257`). `parse_s3_chunk_size` (`cli.rs:2469-2481`) mirrors `cmd_put`
(`cli.rs:555-560`): `str::parse::<usize>`, error `s3: invalid --chunk-size \`{raw}\``.
Then `check_s3_chunk_size` (`cli.rs:2483-2497`) refuses anything outside
`1..=MAX_CHUNK_SIZE` with `s3: --chunk-size N is out of range: …`. Both messages use the
`s3:` prefix the role's credential refusals use (base `cli.rs:2193,2199`). The numbers in
the message and in the usage note are formatted from the constant, so they cannot drift.

**Threading to all six arms** — `serve_s3_role` gains `chunk_size: Option<usize>`
(`cli.rs:2331`), passes it to `serve_s3_dispatch` on both chunk planes (`cli.rs:2351`
gRPC fan-out, `cli.rs:2368` local FS), and `serve_s3_dispatch` gains the same parameter
(`cli.rs:2396`). Each of the six arms now builds its gateway through one helper,
`compose_s3_gateway` (`cli.rs:2447-2467`; arms at `cli.rs:2408, 2414, 2421, 2428, 2434,
2441`). `None` is literally `Gateway::new(...)` wrapped in `Arc`, which is exactly the
pre-change composition. `Some(n)` adds `.with_chunk_size(n)`.

**`serve_s3_role` re-checks the range** — `cli.rs:2337-2339`. It is `pub` and has an
external caller, so a library caller that skips `cmd_s3` could otherwise start a role
with `Some(64 MiB)` and fail the first large PUT, which is the exact invariant violation
the brief names. Three lines; same `check_s3_chunk_size`. Pinned by a unit test (below).

**Signature change, one external caller updated** — `serve_s3_role`'s public signature
changed (new 5th parameter). Its only external caller,
`crates/server/tests/s3_gateway_cluster.rs:153`, now passes `None` (`:158-160`), i.e. the
same 1 MiB default it got before. Grep over `crates/`, `xtask/`, `docs/` found no other
caller.

**Usage** — the `wyrd s3` usage line gains `[--chunk-size N]` (`cli.rs:492`), placed after
`[--data-dir DIR]` as on the `wyrd put` line (`cli.rs:479`). A two-line note at the end of
`usage()` (`cli.rs:512-518`) states the default, the range and the reason, since the range
is otherwise only discoverable by hitting the error.

**Templates** —
* `deploy/dist/env/s3.env.example:12-20` (comment, in the file's `#   --flag   text`
  style: what it decides, default, range, refusal, transport reason; no memory figure) and
  `:27` (`--chunk-size 1048576` on the live `WYRD_S3_ARGS=` line, before `--endpoints` so
  the long list stays last).
* `docs/design/architecture/m4-first-deployment-blueprint.md:1117-1119,1126` — the
  blueprint invocation the env file mirrors (brief cited `:1109-1114` on `main`; it is at
  `:1118-1123` on this base). This is also the "docs currency" item from the rubric: it is
  the only living architecture doc that lists `wyrd s3`'s flags (grep of
  `docs/design/architecture/*.md` and `README.md`).
* `deploy/small-multi-node-fdb/docker-compose.yml:389,406,423` — `"--chunk-size",
  "1048576"` in all three gateway `command:` arrays, plus a two-line comment at `:381-382`.
* `xtask/tests/dist_templates.rs:188-214` — finds the single live `WYRD_S3_ARGS=` line,
  tokenises it, requires a `--chunk-size <value>` pair, and requires the value to be in
  `1..=16777216`. It looks at that line only.

## Mixed fleets

A fleet of gateways may run different chunk sizes. That is safe: the chunk size only
decides how a *new* write is split. Reads follow each object's own committed chunk map, not
a process-wide setting, so a gateway at 1 MiB reads an object another gateway wrote at
16 MiB and vice versa. Nothing on the wire changes in this slice (every value accepted
fits today's transport), so there is no mixed-version transport concern either.

## Alternatives ruled out (with costs)

* **Apply the chunk size inline in each of the six arms** instead of `compose_s3_gateway`.
  Each arm would become `let gateway = Gateway::new(..); let gateway = match chunk_size
  { Some(n) => gateway.with_chunk_size(n), None => gateway }; let gateway =
  Arc::new(gateway);` — about 5 lines × 6 arms = 30 lines, four of them in
  feature-gated arms the default build never compiles, so a missed arm is invisible. The
  helper is 21 lines once plus a one-line change per arm, and an arm cannot skip it
  without visibly not calling it.
* **`chunk_size: usize` defaulting to 1 MiB, always calling `with_chunk_size`.** Same
  behaviour, but the default would have to come from somewhere: either cli's own
  `DEFAULT_CHUNK_SIZE` (`cli.rs:64`, the `wyrd put` one, conflating the two constants the
  brief says stay separate) or by widening lib's private one. `Option` keeps the absent
  path literally `Gateway::new`, which is what "composed exactly as today" asks for.
* **Validate only in `cmd_s3`.** Leaves the `pub` `serve_s3_role` accepting any value. The
  check there is 3 lines (`cli.rs:2337-2339`).
* **Make `with_chunk_size` return `Result` and enforce the ceiling.** Widens the public
  API and removes the `.max(1)` behaviour the brief says to keep; also wrong layer, since
  the ceiling is a transport property of the role, not of the gateway library.
* **Reject a leading `+` (`"+5".parse::<usize>()` is `Ok(5)`).** The brief says to parse
  the way `cmd_put` parses (`str::parse::<usize>`), and the rubric's sign rule is about
  RFC-format parsers. Left as `cmd_put` has it.

## Tests

### The binding test: `crates/server/tests/s3_chunk_size_flag.rs` (new file)

Drives `env!("CARGO_BIN_EXE_wyrd")`. Every role is `wyrd s3 --access-key … --secret-key …
--region us-east-1 --s3-listen 127.0.0.1:0 --data-dir <tmp> --metadata-backend redb
--coordination-backend mem` plus the leg's flags. The port comes from the role's own
listen line. Four `#[test]`s, one per leg:

* (A) `--chunk-size 524288`: signed PUT of 2,097,152 bytes → 200, GET → 200 byte-equal,
  exactly 4 chunk dirs under `<data-dir>/chunks`. No flag: same PUT → 200, exactly 2.
* (B) `1` and `16777216` print the listen line. `0`, `16777217`, `1MiB` exit non-zero,
  stderr names `--chunk-size`, no listen line.
* (C) in-process production `DServer` (`bind`/`register`/`serve`, `FsChunkStore` in a
  temp dir, `DSERVER_GROUP`, gRPC limits untouched) + role with `--endpoints <it>
  --chunk-size 16777216`: PUT 16,777,216 bytes → 200, D-server store holds exactly 1
  chunk dir, GET → 200, body byte-equal.
* (D) bare `wyrd` exits 2; the `wyrd s3 …` usage line contains `[--chunk-size N]`.

Bounding and cleanup: every child is wrapped in `WyrdProcess`, whose `Drop` kills and
reaps it; stderr is drained on a helper thread into a channel and read with
`recv_timeout` against a 120 s deadline; a role that starts when it should be refused is
returned as `Launch::Serving` and the test panics at once (red, not hang). HTTP is a raw
`TcpStream` with connect/read/write timeouts and an overall deadline. Responses are
parsed strictly: `content-length` must equal the bytes received, chunked bodies must have
every CRLF including the final one and nothing after, and a body with neither framing is
an error. The in-process D server's `Drop` sends shutdown, waits up to 10 s for `serve` to
return, then `shutdown_timeout(10 s)`.

It imports nothing this change adds: only `DServer`, `DSERVER_GROUP`, `FsChunkStore`,
`MemCoordination`, `sigv4::{sign, format_amz_date, Credentials}`, `wyrd_traits::Result` —
all on the base. Numbers are hard-coded.

### Supporting tests (green-only; not the discriminator)

* `cli.rs:3005-3027` `s3_chunk_size_accepts_exactly_one_to_sixteen_mib` — absent → `None`;
  `1`, `1048576`, `16777216` accepted; `0`, `16777217`, `1MiB`, `""`, `-1`,
  `18446744073709551616` refused with `s3: …--chunk-size…`. In-process, so coverage sees
  the parse/range code the spawned child runs.
* `cli.rs:3029-3046` `compose_s3_gateway_applies_the_chunk_size_only_when_given` — the
  helper every dispatch arm uses: `Some(524288)` and `Some(16 MiB)` reach the gateway's
  `chunk_size`, `None` leaves `Gateway::new`'s 1 MiB. Gives in-process coverage of the
  `Some` branch, which otherwise runs only inside the spawned child.
* `cli.rs:3048-3083` `serve_s3_role_refuses_an_out_of_range_chunk_size_before_opening_anything`
  — `serve_s3_role(…, Some(0) | Some(16 MiB + 1), …)` returns `Err` naming `--chunk-size`
  within 10 s and creates no `<data-dir>/chunks`. With the 3-line check at
  `cli.rs:2337-2339` removed it goes red (`Elapsed(())` after 10 s: the role served).
* `xtask/tests/dist_templates.rs` `env_examples_name_every_load_bearing_flag` — with
  `--chunk-size 1048576` deleted from the `WYRD_S3_ARGS=` line but the comment block left
  in place (the file still contains `--chunk-size` once), it fails at
  `dist_templates.rs:207` "line lost `--chunk-size N`". That is the iteration-1 hole, now
  closed.
* `crates/server/tests/s3_gateway_cluster.rs` still passes with the `None` argument.

### How I ran them

The project runner is `engine/xtask.sh` → `cargo xtask <task>`; it has no single-test
entry point (tasks are `ci`, `conformance`, `dst`, …; `xtask/src/main.rs:106-140`). For
the fast red/green pass I ran the exact command the brief names and C4-verify runs
(`cargo test -p wyrd-server --test s3_chunk_size_flag`, cf. `run-verify.sh:441`) under a
hard `timeout 900`. The full gate went through the project runner (see "Gate" below).

Green (fix applied): `test result: ok. 4 passed; 0 failed` in 2.56 s.

Red (production reverted: `git checkout -- crates/server/src/cli.rs crates/server/src/lib.rs`,
test kept; it compiled): `0 passed; 4 failed`:
* (A) `left: 2 right: 4` — flag ignored, 1 MiB chunks.
* (B) "`--chunk-size 0` must be refused, but the role is serving on 127.0.0.1:33353".
* (C) `left: 16 right: 1` — 16 one-MiB chunks on the D server.
* (D) usage line printed without `[--chunk-size N]`.

## Refute-your-own-test

* **(a) Genuine red?** Yes. With `cli.rs` and `lib.rs` reverted to the base and the new
  test file kept, the test target compiled and all four tests failed on assertions, each
  for the reason the brief predicts (numbers above). Fix restored with `cp` from scratch
  copies; `cmp` confirmed byte-identical restore.
* **(b) Production path?** Yes. The test spawns the built `wyrd` binary
  (`CARGO_BIN_EXE_wyrd`), so `run → cmd_s3 → parse_s3_chunk_size → serve_s3_role →
  serve_s3_dispatch → compose_s3_gateway → Gateway::with_chunk_size` is the code under
  test, unmodified. Leg (C)'s D server is the production `wyrd_server::dserver::DServer`
  served through `DServer::serve` (which builds `ChunkStoreServer::new(service)`,
  `dserver.rs:1206`, with no message-size override), not a hand-built tonic server. The
  gateway's gRPC client is the production `connect_fanout` the role uses. Signing is the
  production `sigv4::sign`.
* **(c) Fixture includes the fault?** Yes. The faults are "flag ignored", "bad value
  accepted" and "ceiling past the transport". (A) and (C) count real on-disk chunk
  directories written by the real role (the ignored flag shows up as 2 and 16 on the
  reverted tree). (B) feeds the exact bad values (`0`, `16777217`, `1MiB`) to the real
  binary and treats a bound listener as failure. (C) pushes a full 16 MiB chunk (the
  largest accepted value) through a real gRPC hop with the real 4 MiB tonic limits, both
  directions (PUT fragments to the D server, GET fragments back to the gateway client),
  and checks the bytes come back equal. Nothing is curated out: single D server means all
  9 fragments of the chunk cross that one link.

## Feature-gated arms

`cargo clippy -p wyrd-server --features tikv,etcd,fdb --all-targets` is clean (FDB client
headers are installed on this host, `/usr/include/foundationdb/fdb_c.h`), so all six
`serve_s3_dispatch` arms compile with the new parameter and helper.

## Formatting / commit readiness

`cargo fmt --all -- --check` clean. `cargo clippy -p wyrd-server --all-targets` and
`cargo clippy -p xtask --all-targets` clean (the workspace denies `clippy::all`). The
target repo has no pre-commit hooks (`core.hooksPath` unset, no `.pre-commit-config.yaml`,
`.githooks`, `lefthook`); `cargo xtask ci` is the commit check.

## Gate

Final run on the finished tree, through the project runner: `./engine/xtask.sh ci`
(→ `cargo xtask ci` in `$PDCA_WORKTREE`). Result: `xtask ci: all checks passed`, exit 0,
195 `test result: ok` lines. `s3_chunk_size_flag` ran inside it (4 passed, 2.50 s), as did
`custodian_day_one` (15 passed) and `dist_templates`.

One earlier gate run (before the last two small edits) hung in
`crates/server/tests/custodian_day_one.rs`: 12 minutes at zero CPU, every thread parked
on a futex. I stopped it (my own process only). That binary does not reach any code this
patch changes. Alone on the patched tree it passed 21 of 21 runs (one via cargo, then 20
direct runs of the test binary under `timeout 60`), and it passed inside the final gate. At
the same time a different bundle's gate run (issue 777, its own build dir) had
`custodian_gc` stuck the same way for about 16 hours, so this looks like an intermittent
hang in the custodian tests on this host, not something this patch causes. Worth a
tracking issue if it recurs; I did not file one. If Check's C4-ci row samples the hang,
the confirm-once re-run should clear it.

## Out of scope, left alone

Transport limits, `crates/chunkstore-grpc`, D-server construction, DST mounts; `wyrd put`
(still accepts `0`); both `DEFAULT_CHUNK_SIZE` definitions (values and locations
unchanged); `.max(1)` in `with_chunk_size`; the `role started` event (#778); the TiKV
compose stack; chunk-map ceilings (#739); any memory figure.
