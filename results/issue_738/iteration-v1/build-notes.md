# Build notes — issue 738 / s3-role-chunk-size-flag

## Base found

Worktree `$PDCA_WORKTREE` HEAD = `4bda59c` (`pdca-integrate: issue_840`), on the integration
stack `pdca-integration/r-a834e98f67b952c61db7a03084c626e8/main` (bundle `stack-base`).
**#736 is NOT on this base**: the `role started` event in `cmd_s3` (base `cli.rs:2262-2269`)
has no `version` field, and `serve_s3` (base `cli.rs:2422-2451`) sets only `config.region`.
So, as the brief says, I did not add either; I extended the functions as they stand. Line
numbers below are on this base (they differ from the brief's `65ca4fd` numbers).

## What changed (base `path:line` → what)

- `crates/server/src/lib.rs:50-51` — `DEFAULT_CHUNK_SIZE` becomes `pub` (the ONE definition);
  new `pub const MAX_CHUNK_SIZE: usize = 1 << 30` beside it, with the brief's rationale in its
  doc comment. Additive to `wyrd-server`'s public API; no existing item changes shape or value.
- `crates/server/src/lib.rs:144` — `with_chunk_size` doc comment: now the role's
  configuration seam, not "mainly so tests…". The `.max(1)` floor is KEPT (brief §Design).
- `crates/server/src/lib.rs:772-776` — the default-is-1-MiB unit test stays (now the only
  one); added `max_chunk_size_is_one_gib` and a compile-time `const _: () =
  assert!(DEFAULT_CHUNK_SIZE <= MAX_CHUNK_SIZE)` (a runtime `assert!` on constants trips
  clippy's `assertions_on_constants`, which the workspace denies).
- `crates/server/src/cli.rs:40` — import the shared `DEFAULT_CHUNK_SIZE`, `MAX_CHUNK_SIZE`.
- `crates/server/src/cli.rs:64` — the duplicate `DEFAULT_CHUNK_SIZE` deleted. `cmd_put`
  (`cli.rs:555-560`) is untouched textually; it now resolves the shared constant. Same
  value, still accepts `0` — no behaviour change for `wyrd put`.
- `crates/server/src/cli.rs:492` — usage line gains `[--chunk-size N]`.
- `crates/server/src/cli.rs:2199` (after the secret-key parse, before `tokio_runtime()` /
  `TcpListener::bind` at `:2235/:2245`) — `let chunk_size =
  parse_s3_chunk_size(parsed.flag("chunk-size"))?;`. So the refusal happens before any
  listener is bound; `run()` prints `wyrd: s3: …--chunk-size…` and exits 1 (`cli.rs:428-433`).
- `crates/server/src/cli.rs:2289` — new `fn parse_s3_chunk_size(Option<&str>) ->
  Result<usize, String>`: absent → `DEFAULT_CHUNK_SIZE`; `parse::<usize>()` failure →
  `s3: invalid --chunk-size `{s}`` (mirrors `cmd_put`'s message with the `s3:` prefix);
  `0` or `> MAX_CHUNK_SIZE` → `s3: --chunk-size `{s}` is out of range; it must be
  1..=1073741824 bytes`. `usize::from_str` accepts a leading `+`, so `+65536` is accepted as
  65536 — same as `cmd_put`; the grammar-strictness rubric item targets RFC formats, not
  this decimal CLI knob, and matching the peer parser was the brief's instruction.
- `crates/server/src/cli.rs:2267` — `role started` event records `chunk_size` (so a
  deployment's logs show the value it runs, helpful given gateways can now disagree).
- `crates/server/src/cli.rs:2270-2279, 2306-2315, 2322-2349` — `chunk_size` threaded
  `cmd_s3` → `serve_s3_role` (new 5th parameter, after `endpoints`) → both
  `serve_s3_dispatch` calls.
- `crates/server/src/cli.rs:2361-2412` — `serve_s3_dispatch` takes `chunk_size`, and ALL
  SIX arms apply `.with_chunk_size(chunk_size)` (`:2377, :2383, :2390, :2397, :2403,
  :2410`). Verified the four `#[cfg]` arms compile: `cargo clippy -p wyrd-server --features
  tikv,etcd,fdb --lib --bins` → exit 0, no warnings.
- `crates/server/src/cli.rs:2916-2920` — the duplicate `default_chunk_size_is_one_mib`
  unit test replaced by `s3_chunk_size_absent_is_the_shared_default_and_bounds_are_refused`
  (absent ⇒ default; `1`, `65536`, `1073741824` accepted; `0`, `1073741825`, `-1`, `abc`,
  empty, `1MiB`, `2^64` refused, each message naming `--chunk-size` with the `s3: ` prefix).
- `crates/server/tests/s3_gateway_cluster.rs:153-166` — the one outside caller of
  `serve_s3_role` passes `wyrd_server::DEFAULT_CHUNK_SIZE`. **Deliberate `pub` signature
  change** of `serve_s3_role` (8 → 9 args); the only other caller is `cmd_s3`.
- `deploy/dist/env/s3.env.example:11-17` — `--chunk-size` described in the file's
  per-flag comment style and added to `WYRD_S3_ARGS` as `--chunk-size 1048576`.
- `deploy/small-multi-node-fdb/docker-compose.yml:387,404,421` — each gateway `command:`
  gains `"--chunk-size", "1048576"` (open question 2: name the default explicitly, so the
  stack's behaviour is unchanged but the file records it).
- `xtask/tests/dist_templates.rs:174-184` — `"--chunk-size"` added to the pinned list.
- `docs/design/architecture/m4-first-deployment-blueprint.md:1115-1123` — the living
  blueprint's `wyrd s3` invocation gains `--chunk-size 1048576` plus a one-line note. Not in
  the brief's scope list, but the rubric's "Docs currency" MUST (a new CLI flag updates the
  living architecture doc in the same PR) requires it, and the env example says it
  "mirrors the M4 blueprint's B.5 invocation". The doc is `status: living`, so editing in
  place is allowed (not a ratified/frozen doc).

## The test — `crates/server/tests/s3_chunk_size_flag.rs` (NEW)

Drives `env!("CARGO_BIN_EXE_wyrd")` (idiom from `cli_roundtrip.rs:11-18`), `s3 --s3-listen
127.0.0.1:0 --data-dir <tmp> --access-key … --secret-key …`, reads stderr on a drain thread
until the `wyrd s3: serving S3-compatible HTTP on <addr>` line and parses the port from it.
PUT is a hand-signed SigV4 request over a std `TcpStream` using the production
`wyrd_gateway_s3::sigv4::sign` + `format_amz_date(now)` (shape from `s3_http_wire.rs:95-110`),
sync instead of tokio because nothing here needs async. A `Drop` guard kills + reaps the
child on every path; every wait (start, exit, socket read/write) is bounded at 60 s.

Chunk size under test `N = 512 KiB`, object `4N = 2 MiB`: flag case must give 4 chunk dirs;
default case must give `2 MiB / 1 MiB = 2`. I chose N so the default count is itself >1 —
with the brief's `65536` example the default count would be 1, which a "chunking disabled"
mutant would also produce. Chunk dirs are counted as 32-hex-named directories under
`<data-dir>/chunks` holding ≥1 `.frag` (`crates/chunkstore-fs/src/lib.rs:598-599`).

Refusal legs: `0` and `1073741825` must exit non-zero, name `--chunk-size` on stderr, and
never print the listen line; `1` and `1073741824` must print the listen line. Accept legs
never PUT, so no chunk-sized buffer is allocated.

### Refuting my own test

- **(a) Genuine red? YES.** Reverted every production file (`git apply -R` of the non-test
  part of the diff, kept the new test), ran `timeout 900 cargo test -p wyrd-server --test
  s3_chunk_size_flag`: all 3 tests FAILED in 0.17 s, for the honest reasons —
  `left: 2, right: 4` ("`--chunk-size 524288` must split a 2097152-byte object into 4
  chunks"), and both refusal tests panicked at the `Outcome::Serving` arm (the pre-fix role
  accepted `0` / `1073741825` and started serving). Compiles on the reverted tree (no new
  symbol referenced), no hang. Re-applied → 3/3 pass; `git diff HEAD` is byte-identical to
  `patch.diff` after the round trip.
- **(b) Production path? YES.** It runs the built `wyrd` binary — `run()` → `cmd_s3` →
  `serve_s3_role` → `serve_s3_dispatch` redb+mem arm → `Gateway::with_chunk_size` → real
  `FsChunkStore` on disk. The signing helper is the production `sigv4::sign`. No stand-in.
- **(c) Fixture includes the fault? YES.** The fault is "flag parsed but ignored"; the
  fixture passes the flag to the real role and inspects the real on-disk store it wrote.
  The refusal fixture passes the exact bad values from the criterion.

Limit: only the redb+mem arm is exercised at runtime. The other five arms are covered by the
feature-enabled clippy compile (they fail to compile without the argument) but not by a
runtime test — tikv/etcd/fdb need live services. The per-arm edit is the same one-liner.

### Why I did not run `engine/scripts/run-verify.sh`

It creates its worktree at `../../../wyrd-verify<lane>` and a `pdca-verify*` branch in the
main Wyrd checkout — outside the roots I'm allowed to write. I ran the same revert-and-test
procedure by hand inside `$PDCA_WORKTREE` under `timeout`, with scratch in
`$PDCA_SCRATCH/pdca-builder-738-redleg`. Check's C4-verify re-runs it for real.

## Other checks

- `cargo fmt --all` — clean (it rewrapped my test; the shipped copy is the formatted one).
- `cargo clippy -p wyrd-server --all-targets` — exit 0, no warnings.
- `cargo test -p wyrd-server --lib chunk_size` — 3 pass; `--test s3_gateway_cluster` — pass;
  `cargo test -p xtask --test dist_templates` — 14 pass.
- `cargo xtask ci` — see the result line at the end of this file.

## Review points the brief asked me to state

- **Gateways in one fleet can now disagree on chunk size.** Benign today: the chunk map is
  stored per object and reads follow the map, not a process-global size. An object written
  at 512 KiB by gateway A is read correctly by gateway B running 1 MiB. The new
  `chunk_size` field on the `role started` event makes a mismatch visible in logs.
- **Small chunk sizes grow the chunk map fast.** `--chunk-size 1` is accepted (the brief's
  criterion requires it) and would make a large object's chunk map huge; that interacts
  with the segmented-root ceiling #739 tracks. Not addressed here (out of scope).
- **Ceiling.** I kept `1 << 30` inclusive as settled. No disagreement to report: 1 GiB is
  loose but the brief's reasoning (it only needs to stop the absurd end) holds.
- **Scope (g)** is in the diff as specified; it is separable (drop the `lib.rs:51` `pub`,
  keep `cli.rs:64`) if sign-off prefers.

## Alternatives ruled out

- Testing via `serve_s3_role` directly: would not compile on the reverted tree → the gate
  scores UNVERIFIABLE, not red (brief §Falsifiability).
- Removing the `.max(1)` clamp in `with_chunk_size`: out of scope per brief §Design.
- Rejecting unknown flags in `ParsedArgs::parse`: would have made the pre-fix red come from
  a different cause and changes every role's CLI; not asked for.

## `cargo xtask ci` result

`timeout 3500 cargo xtask ci` on the patched worktree: exit 0, last line `xtask ci: all checks passed`.
