## Summary
**User impact:** Anyone running the S3 gateway cannot choose how large the pieces are
that their objects get split into: every gateway uses 1 MiB, whatever the hardware or
workload. Passing `--chunk-size` does not help: the gateway starts, quietly ignores it,
and keeps using 1 MiB. So a deployment can believe it runs a setting it never ran, and a
measurement tool has to guess the size.

This PR makes `wyrd s3 --chunk-size N` work, keeps 1 MiB when the flag is left out, and
refuses at startup any value the gateway cannot actually handle (zero, anything above
16 MiB, or text that is not a byte count), instead of failing later on a real upload.

## What to look at
- The new flag on `wyrd s3`, how it is checked, and how it reaches the gateway on every
  storage combination the role can run.
- Why the limit is 16 MiB: the cluster's storage servers accept at most 4 MiB per message,
  each chunk is sent as six data pieces plus parity, so roughly 24 MiB and up already
  fails. 16 MiB leaves about 1.3 MiB to spare. The network side is not changed here.
- The deploy templates (dist env example, M4 blueprint, the FDB compose stack) now spell
  out the default `--chunk-size 1048576`, so a deployment records its value. Behaviour is
  unchanged.

To try it on `main` before this PR: start
`wyrd s3 --access-key k --secret-key s --s3-listen 127.0.0.1:8080 --data-dir /tmp/wyrd-cs --metadata-backend redb --coordination-backend mem --chunk-size 65536`,
upload a 256 KiB object, and count the directories under `/tmp/wyrd-cs/chunks`: one, not
four. `--chunk-size 0` also starts. With this PR the first gives four, and the second
exits with `s3: --chunk-size 0 is out of range: …` before it listens.

## Root cause
`cmd_s3` never read `--chunk-size`, so none of the six `Gateway::new` arms in
`serve_s3_dispatch` called `Gateway::with_chunk_size`, and the gateway's
`DEFAULT_CHUNK_SIZE` (1 MiB) always applied. The flag was also accepted silently, because
`ParsedArgs::parse` stores any unknown `--flag value` without complaint.

## Fix
- `crates/server/src/lib.rs:70`: new `MAX_CHUNK_SIZE = 16 << 20` (`pub(crate)`) beside
  `DEFAULT_CHUNK_SIZE` (`lib.rs:52`). Its doc comment gives the transport reason and the
  dependence on the default durability's six data fragments. `with_chunk_size`'s doc now
  calls it the role's configuration seam. Its `.max(1)` floor is kept (`lib.rs:171`).
- `crates/server/src/cli.rs:2186`: `cmd_s3` calls `parse_s3_chunk_size` with the other
  flag reads, before the runtime starts and before `TcpListener::bind` (`cli.rs:2232`).
  `parse_s3_chunk_size` (`cli.rs:2447-2456`) parses the way `wyrd put` does
  (`str::parse::<usize>`). `check_s3_chunk_size` (`cli.rs:2461-2473`) refuses anything
  outside `1..=MAX_CHUNK_SIZE`. Both errors use the role's `s3:` prefix and name the flag.
- `serve_s3_role` (`cli.rs:2301`) takes `chunk_size: Option<usize>`, re-checks the range
  (`cli.rs:2312-2314`) because it is `pub`, and passes the value on both chunk planes to
  `serve_s3_dispatch` (`cli.rs:2366`). All six arms (`cli.rs:2383, 2389, 2396, 2403, 2409,
  2416`) build through one helper, `compose_s3_gateway` (`cli.rs:2426-2442`): `None` is
  plain `Gateway::new`, the same composition as today; `Some(n)` adds `.with_chunk_size(n)`.
- **Public signature change:** `serve_s3_role` has a new fifth parameter. Its one external
  caller, `crates/server/tests/s3_gateway_cluster.rs`, passes `None`.
- Usage: the `wyrd s3` line gains `[--chunk-size N]` (`cli.rs:492`), plus a short note on
  the default and range (`cli.rs:513`).
- Templates: `deploy/dist/env/s3.env.example:12-20,27`,
  `docs/design/architecture/m4-first-deployment-blueprint.md:1108,1117`,
  `deploy/small-multi-node-fdb/docker-compose.yml:381,389,406,423`.
- `xtask/tests/dist_templates.rs:188-214`: checks that the live `WYRD_S3_ARGS=` line, not
  just the comment above it, carries `--chunk-size` with a value in range.

Gateways in one fleet may run different chunk sizes. That is safe, because reads follow
each object's own chunk map, not a process-wide setting. Nothing on the wire changes.

Out of scope: the gRPC message limits (raising the ceiling needs its own transport change),
`wyrd put` (still accepts `0`), and the default value.

## Verification
Line numbers are on `main` (`36f006d`) with this patch applied.

- **Claim:** the flag sets the chunk size, and leaving it out keeps 1 MiB.
  **Checked:** `cli.rs:2186` → `serve_s3_role` `cli.rs:2301` → `compose_s3_gateway`
  `cli.rs:2426-2442`, on all six arms (`cli.rs:2383-2416`).
  **Test:** `crates/server/tests/s3_chunk_size_flag.rs:440` (leg A). The real `wyrd`
  binary with `--chunk-size 524288` stores a 2 MiB object as exactly 4 chunks, and GET
  returns it byte-equal. With no flag the same object is 2 chunks. Before the fix: 2,
  not 4.
- **Claim:** the range is exactly `1..=16777216`, and a bad value is refused before the
  listener binds, naming the flag.
  **Checked:** `cli.rs:2447-2473`, called at `cli.rs:2186`, before `cli.rs:2232`.
  **Test:** `s3_chunk_size_flag.rs:491` (leg B). `1` and `16777216` serve. `0`,
  `16777217` and `1MiB` exit non-zero, stderr names `--chunk-size`, and no listen line
  appears. Before the fix: `0` starts serving.
- **Claim:** the 16 MiB ceiling fits today's cluster transport.
  **Checked:** `lib.rs:52-70`. The storage server's gRPC limits are not touched by this
  diff.
  **Test:** `s3_chunk_size_flag.rs:596` (leg C). A real in-process storage server with its
  stock 4 MiB limits, and a role with `--endpoints` and `--chunk-size 16777216`. A 16 MiB
  PUT returns 200, the server holds exactly 1 chunk, and GET returns the same bytes.
  Before the fix: 16 chunks.
- **Claim:** the usage text lists the flag.
  **Checked:** `cli.rs:492`.
  **Test:** `s3_chunk_size_flag.rs:628` (leg D). Before the fix: no `[--chunk-size N]`.
- **Fails before / passes after:** with `cli.rs` and `lib.rs` reverted and the test file
  kept, the file compiles and all 4 tests fail on their assertions. With the fix, 4 pass.
  The test uses nothing this PR adds, so the same file runs on both trees.
- **Supporting tests:** `cli.rs:2985` (parse/range table, including `""`, `-1` and
  overflow), `cli.rs:3008` (`compose_s3_gateway` applies `Some`, leaves `None` at 1 MiB),
  `cli.rs:3028` (`serve_s3_role` refuses `Some(0)` and `Some(16 MiB + 1)` without creating
  `chunks/`). `dist_templates` fails if `--chunk-size` is removed from the
  `WYRD_S3_ARGS=` line even while the comment still mentions it.
- **Whole gate:** `cargo xtask ci` passes. `cargo clippy -p wyrd-server --features
  tikv,etcd,fdb --all-targets` is clean, so the feature-gated arms compile too.

Fixes #738
