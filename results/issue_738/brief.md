# Brief — issue 738 / s3-role-chunk-size-flag

> Plan artifact (docs 02 §PLAN). Do reads ONLY this file (plus the peer callsites cited
> under **Citations expected**). The `- **Label:** value` lines are parsed by the driver.
>
> **Re-plan (iteration 4).** Iteration 3 was sent back to Plan at sign-off (2026-09-30):
> its 1 GiB ceiling did not fit the cluster transport, and the transport change it forced
> (a fixed 192 MiB gRPC limit on every D server) was rejected. This brief keeps the
> ceiling inside what today's transport carries and leaves the transport alone. What
> changed and why is listed under "Re-plan notes" at the end.

- **Slug:** s3-role-chunk-size-flag
- **Track:** blackbox
- **Kind:** enhancement
- **Defect:** The `s3` role cannot be told what chunk size to use, so every gateway chunks
  at the 1 MiB default whatever its workload or fleet. The seam exists and the sibling
  command already exposes it; only the wiring is missing. Verified on `main` at `36f006d`:
  `Gateway::with_chunk_size` exists (`crates/server/src/lib.rs:144-148`, doc-commented
  "mainly so tests can force multi-chunk objects"); `wyrd put --chunk-size N` is parsed at
  `crates/server/src/cli.rs:555-560`; `cmd_s3` (`cli.rs:2151-2263`) parses no chunk size,
  so none of the six `Gateway::new` arms in `serve_s3_dispatch`
  (`cli.rs:2352,2358,2365,2372,2378,2385`) calls `with_chunk_size`, and the gateway's own
  `DEFAULT_CHUNK_SIZE` (`lib.rs:51`, `1 << 20`, applied at `lib.rs:100`) always wins. Worse
  than missing: `ParsedArgs::parse` (`cli.rs:2539-2561`) stores any unknown `--flag value`
  without complaint, so `wyrd s3 --chunk-size 65536` today starts, ignores the flag, and
  chunks at 1 MiB.
- **Goal:** `wyrd s3 --chunk-size N` is honoured end to end on both chunk planes (local FS
  and the `--endpoints` gRPC fan-out); leaving the flag out composes exactly today's
  gateway (1 MiB chunks); a value the role cannot carry is refused at startup, before the
  listener binds, never discovered later as a failed PUT; and the deployment templates
  record the value a deployment runs.
- **Success criterion:** BINDING — one NEW integration test file,
  `crates/server/tests/s3_chunk_size_flag.rs`, that drives the **built `wyrd` binary**
  (`env!("CARGO_BIN_EXE_wyrd")`) as an `s3` role on `--s3-listen 127.0.0.1:0` and asserts
  all four legs below. Every role it starts is pinned with `--metadata-backend redb
  --coordination-backend mem`. Chunks are counted as `<32-hex>` directories holding
  `.frag` files (`FsChunkStore` layout, `crates/chunkstore-fs/src/lib.rs:91-93`).
  * **(A) The flag sets the chunk size; its absence keeps 1 MiB.** With
    `--chunk-size 524288`, a signed PUT of one 2,097,152-byte object returns 200 and leaves
    exactly **4** chunk directories under `<data-dir>/chunks`. The same object PUT through a
    role started with NO `--chunk-size` leaves exactly **2**.
  * **(B) Exact accept/refuse boundaries.** The accepted range is `1 ..= 16777216`
    (16 MiB, inclusive). `1` and `16777216` are ACCEPTED: the role prints its
    `wyrd s3: serving S3-compatible HTTP on <addr>` line. `0`, `16777217` and `1MiB` are
    REFUSED: the process exits non-zero, stderr names `--chunk-size`, and the listen line
    never appears.
  * **(C) The ceiling fits today's cluster transport.** One production D server
    (`wyrd_server::dserver::DServer`, in-process, with its gRPC limits exactly as on `main`)
    and a role started with `--endpoints <that D server> --chunk-size 16777216`: a signed PUT
    of one 16,777,216-byte object returns 200, the D server's store then holds exactly **1**
    chunk directory, and a signed GET returns 200 with a body byte-equal to what was PUT.
  * **(D) The usage text lists the flag.** Bare `wyrd` exits 2 and its `wyrd s3 …` usage
    line contains `[--chunk-size N]`.
  The ceiling value is SETTLED and is not Do's to change; see Design. It is the maintainer's
  decision, taken in this re-plan's Plan session on 2026-10-01 (asked "is 16 MiB OK?",
  answered "yes, ceiling of 16 MB is fine"). It answers the 2026-09-30 sign-off's
  instruction "Settle a max chunk size that fits the D-server gRPC limit"
  (`iteration-v3/SUMMARY.md` §9). That sign-off did not name a value.
  "Default unchanged" means the chunk count in (A), not byte-identical persisted state: the
  gateway stamps `modified: Some(now_millis())` (`lib.rs:197`) and draws a random chunk-id
  epoch per process (`lib.rs:103`, `lib.rs:274`), so no two runs agree byte for byte.
- **Falsifiability:** RED is producible on the ordinary developer harness Do is pointed at:
  `cargo test -p wyrd-server --test s3_chunk_size_flag`, no Docker, no FDB, no cargo
  feature. On the pre-fix tree the test COMPILES (it names only the binary and APIs that
  already exist) and FAILS on assertions: (A) gets 2 chunks, not 4, because the flag is
  ignored; (B) sees `0`, `16777217` and `1MiB` start a serving role; (C) gets 16 chunk
  directories, not 1; (D) finds no `[--chunk-size N]`. (C) is also the leg that goes red if
  the ceiling is ever set past what the transport carries: iteration 3's reviewer and
  adversary each reproduced a PUT at a 24 MiB or larger chunk returning HTTP 500
  (`decoded message length too large … the limit is: 4194304 bytes`). The test must not
  import any symbol this change adds (a new constant, a new parameter): it would not
  compile on the reverted tree, and C4-verify scores that UNVERIFIABLE, not red. Hard-code
  the numbers. Every spawned child must be killed on every exit path and every wait must be
  bounded, because the role blocks forever; a pre-fix role that wrongly starts must be a red,
  never a hang.
- **Invariant to restore:** *A write-path setting the operator gives a role is honoured on
  every composition that role can run, or refused when the role starts; it is never accepted
  and then failed on some later request.* Stated over the configuration surface, not the one
  missing call: the chunk size is a deployment property (it sets the erasure-coding unit, the
  per-object fan-out, the chunk-map growth rate and per-PUT memory), and a value that boots
  but fails the first object large enough to fill a chunk is the failure iteration 3
  shipped. Sources: the tracker's own scope ("Reject a zero or absurd value at parse time
  rather than clamping silently"); the repo's configuration convention quoted in `cmd_s3`
  itself ("Select the gateway's backends BY CONFIGURATION, exactly as every other
  cluster-facing role does", `cli.rs:2176-2181`); and the fail-closed rule the deploy
  templates already enforce for credentials (`xtask/tests/dist_templates.rs:188-198`).
  Self-test: a check in the parser alone does not satisfy it. Leg (C) requires that the
  accepted maximum actually crosses the gRPC transport, and leg (A) requires that the value
  reaches the gateway on every composition arm.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Conflicts with:** 742, 778, 779
- **Ordering note:** No dependency in either direction; these share files, so they must not
  build blind on one base. #778 edits `cmd_s3`'s `role started` event (`cli.rs:2237-2244`);
  #779 edits `serve_s3_role` → `serve_s3_dispatch` → `serve_s3` (`cli.rs:2281-2426`), the
  same chain this slice threads the chunk size through; #742 edits
  `xtask/tests/dist_templates.rs`'s s3 flag list (`:174-187`) and `deploy/dist/`. All three
  already declare `Conflicts with: 738` back. #736, which the iteration-3 brief worried
  about, was split into #778/#779, and neither is built yet. So nothing from #736 is on your
  base and none of its code is to be preserved or added. Your base is the run's folded
  integration branch, not bare `main`: it carries earlier waves' accepted work (today #774,
  #839, #840; only #839 touches `cli.rs`, in the custodian restore code around `:1230-1370`,
  which shifts `cmd_s3` down by about 25 lines). Line numbers here are `main` at `36f006d`;
  read the functions as they stand on your base and say in `build-notes.md` which base you
  built on.
- **Surfaces:** data
- **Difficulty:** medium
- **Do model:** opus
- **Scope:** (a) `cmd_s3` reads `--chunk-size`. When the flag is absent, the gateway is
  composed exactly as today. (b) The value reaches the gateway on all six
  `(metadata, coordination)` arms of `serve_s3_dispatch`, on both chunk planes. If that
  changes `serve_s3_role`'s public signature, update its one external caller
  (`crates/server/tests/s3_gateway_cluster.rs:153`) and say so in `build-notes.md`.
  (c) Refuse, at startup and before the listener binds, any value that does not parse the
  way `cmd_put` parses it (`str::parse::<usize>`, `cli.rs:555-558`) or falls outside
  `1 ..= 16777216`, naming the flag in the `s3:` style of the role's other refusals. Don't rely on the silent `.max(1)` clamp in `with_chunk_size`. (d) State the
  ceiling and its reason (the D-server transport limit, below) once in code, beside the
  default, where whoever next wants to raise it will read why. (e) `with_chunk_size`'s doc
  comment: it is the role's configuration seam now, not a test affordance. (f) The `wyrd s3`
  usage line (`cli.rs:492`) gains `[--chunk-size N]`. (g) `deploy/dist/env/s3.env.example`'s
  `WYRD_S3_ARGS=` line gains `--chunk-size 1048576`, with a comment in the file's existing
  style naming what it decides, the accepted range, and that the ceiling comes from the
  D-server transport. The M4 blueprint's `wyrd s3` invocation
  (`docs/design/architecture/m4-first-deployment-blueprint.md:1109-1114`), which that file
  says it mirrors, gains the same flag. The three `small-multi-node-fdb` gateway `command:`
  arrays (`deploy/small-multi-node-fdb/docker-compose.yml:387,404,421`) gain
  `"--chunk-size", "1048576"`, the default stated explicitly so the stack records its value
  without changing behaviour. (h) `xtask/tests/dist_templates.rs`'s
  `env_examples_name_every_load_bearing_flag` checks that the live `WYRD_S3_ARGS=` line
  carries `--chunk-size`. It must check that line, not the whole file: iteration 1's
  version passed with the flag deleted, because the new comment alone satisfied a
  whole-file `contains`.
  **/ out of scope:** any change to the gRPC transport: message-size limits, the D server's
  service construction, `crates/chunkstore-grpc`, the DST mounts. Raising the ceiling past
  today's transport is a separate follow-up issue on the Foundations milestone. The
  maintainer said in the 2026-10-01 Plan session that they will file it. It is not filed
  yet (no matching issue on getwyrd/wyrd as of 2026-10-01), so it has no number here, and
  nothing in this slice waits on it. Also out: collapsing the two `DEFAULT_CHUNK_SIZE` definitions
  (`lib.rs:51`, `cli.rs:64`) — both stay as they are; any change to `wyrd put` (it keeps
  accepting `0`); the default VALUE; per-object or per-bucket chunk size; `--chunk-size` on
  any other role; adding the chunk size to the `role started` event (#778 owns that event);
  the TiKV compose stack (`deploy/small-multi-node/`); removing `.max(1)` inside
  `with_chunk_size`; the chunk-map ceilings `MAX_ROOT_SEGMENTS` / `MAX_ROOT_VALUE_BYTES`
  (`crates/core/src/metadata.rs:544,574`; #739 tracks how a small chunk size shrinks the
  largest storable object); any peak-memory figure in docs or templates (two earlier rounds
  stated one, and both were wrong).
- **Repro instruction:** In the target checkout on `main` at `36f006d`: bare
  `cargo run --bin wyrd` prints a `wyrd s3` usage line with no `--chunk-size` (`cli.rs:492`).
  Start `wyrd s3 --access-key k --secret-key s --s3-listen 127.0.0.1:8080 --data-dir
  /tmp/wyrd-cs --metadata-backend redb --coordination-backend mem --chunk-size 65536`: it
  starts (the flag is stored and never read, `cli.rs:2539-2561`). PUT a 256 KiB object
  through it and count directories under `/tmp/wyrd-cs/chunks`: one, not four.
  `--chunk-size 0` also starts.
- **External dependencies:** none — base Rust toolchain only. The test runs one in-process
  D server and child `wyrd` processes on loopback ports. No Docker, FDB, etcd or TiKV, and
  no system `protoc` (`crates/proto/build.rs:2`).
- **Test file:** `crates/server/tests/s3_chunk_size_flag.rs` — a NEW file. This instance's
  C4-verify treats an ADDED `*/tests/*.rs` as the per-fix discriminator
  (`engine/scripts/run-verify.sh:138-144`), keeps it, reverts the production change, and
  scores a compile failure on the reverted tree UNVERIFIABLE (`:202-229`). A dry run of its
  classifier on this slice's expected file set returned `ADDED_TEST
  crates/server/tests/s3_chunk_size_flag.rs`. Appending to `cli_roundtrip.rs` or to
  `cli.rs`'s test module would earn no red. The `dist_templates.rs` change in (h) is a
  modified file, so it is green-only supporting evidence.
- **Verification posture:** Default: a flippable regression test, red pre-fix, green
  post-fix. One known gap to plan for: the C4 diff-coverage gate (advisory) cannot see code
  that runs only inside the spawned `wyrd` child, which the test kills before it can write a
  coverage profile. All three earlier rounds reported low coverage for that reason. Unit
  tests in `cli.rs`'s existing `#[cfg(test)] mod tests` that pin the accept/refuse decision
  in-process (absent, `0`, `1`, `16777216`, `16777217`, non-numeric) make that coverage
  visible. They are supporting tests, not the discriminator.
- **Citations expected:** Do must cite `path:line` on its base for every change. Peer
  callsites Do MAY open and mirror:
  * **The flag's own precedent:** `crates/server/src/cli.rs:555-560`, `cmd_put` parsing
    `--chunk-size` and naming it in its error. Mirror it with the `s3:` prefix the role's
    other refusals use (`cli.rs:2157,2168,2174`), and add the range check `cmd_put` lacks.
  * **The threading target:** `crates/server/src/cli.rs:2281-2389`, `serve_s3_role` and
    `serve_s3_dispatch`. Four of the six arms are `#[cfg(feature = …)]`-gated (tikv, etcd,
    fdb), so an arm missed there is invisible in the default build. Check's `host-tikv`
    gate compiles the tikv and etcd arms. The fdb arms need the FDB client headers; where
    they are installed, `cargo check -p wyrd-server --features fdb,etcd --tests` is useful
    extra evidence, not a requirement. Either way, read all six arms. The builder is
    `Gateway::with_chunk_size`, `crates/server/src/lib.rs:144-148`.
  * **Driving the built binary:** `crates/server/tests/cli_roundtrip.rs:11-18`
    (`const WYRD: &str = env!("CARGO_BIN_EXE_wyrd");`). This test's child is a long-running
    server: spawn it, read stderr until the listen line (`cli.rs:2221-2224`, which prints
    `listener.local_addr()`, so the ephemeral port is parsed, not guessed), then kill it.
  * **Signing a request by hand:** `crates/server/tests/s3_http_wire.rs:94-109` (production
    `sigv4::sign` with `format_amz_date(SystemTime::now())`). Parse every response strictly:
    check a declared `content-length` against the bytes received, and for a chunked body
    check every delimiter including the final CRLF. Iteration 2's lax parser let a truncated
    GET read as "0 bytes that differ" instead of failing as a truncation.
  * **An in-process production D server:** `crates/server/src/dserver.rs:729` (`bind`),
    `:875` (`endpoint`), `:897` (`register`, group `DSERVER_GROUP` at `:38`), `:933`
    (`serve`), over a `FsChunkStore` rooted in a temp dir so leg (C) can count its chunk
    directories. Shut it down on every exit path.
  * **What the templates must name:** `deploy/dist/env/s3.env.example:6-18`,
    `deploy/small-multi-node-fdb/docker-compose.yml:387,404,421`,
    `xtask/tests/dist_templates.rs:174-187`.
- **Prior-art check (triage cycles):** By affected path on `main` at `36f006d`:
  `git log 65ca4fd..origin/main` over `crates/server/src/{cli,lib,dserver}.rs`,
  `crates/chunkstore-grpc/src/client.rs`, `deploy/dist/env/s3.env.example`, the fdb compose
  file and `xtask/tests/dist_templates.rs` shows custodian restore, multipart and
  fragment-deadline work only. Nothing touches `cmd_s3`'s flags or the gRPC message limits.
  `gh pr list --state all --search chunk-size` → #647 (CLOSED, segmented chunk maps, a
  `crates/core` record change); #801, #672, #675, #610 (merged, unrelated). No issue or PR
  on gRPC message size. This bundle's own iterations 1–3 are the only attempt at the flag.
  Their shape (flag + 1 GiB ceiling + fixed 192 MiB transport limit) was rejected at
  sign-off, and this brief is the re-plan.
- **Disposition hint:** new-feature

## Motivation

**Operators cannot tune the write path.** Chunk size sets the unit of erasure coding, the
fan-out per object, the chunk-map growth rate and the memory each in-flight PUT holds
(`crates/core/src/write.rs:534`, `:574`). One value cannot suit a 9-node NVMe fleet and a
single-node loopback alike, and today the value cannot even be written down.

**It blocks measurement.** `wyrd-validate` sizes its test objects to straddle the chunk
boundary (proposal 0017 §Dependencies, §4). With the chunk size fixed and hidden, the
driver must assume 1 MiB.

**The dist package cannot record it.** `deploy/dist/env/s3.env.example` mirrors the
blueprint's production invocation, and a deployment's chunk size is part of its identity.

## Design

### Why the ceiling is 16 MiB

The ceiling is set by the cluster transport, not by memory. Each chunk is erasure-coded on
its own into RS(6,3) fragments (`DEFAULT_DURABILITY`, `crates/server/src/lib.rs:49`). Each
fragment is about one sixth of the chunk, rounded up to 64 bytes
(`crates/core/src/erasure.rs:16,79-82`), plus a 44-byte header
(`crates/chunk-format/src/header.rs:11`) and a 4-byte checksum. Each fragment travels whole
in ONE unary gRPC message (`PutFragment` / `GetFragment`,
`crates/proto/proto/wyrd/v0/chunk.proto:114-115`; `put_fragment` in
`crates/chunkstore-grpc/src/client.rs`). The receiver of every such message keeps tonic's
default 4 MiB receive limit (`DEFAULT_MAX_RECV_MESSAGE_SIZE`, tonic 0.14.6
`src/codec/mod.rs:101`): the D server for PUTs (`ChunkStoreServer::new(service)`,
`crates/server/src/dserver.rs:1206`), and the gateway's client for GETs
(`ChunkStoreClient::new(channel)`, `crates/chunkstore-grpc/src/client.rs:251`).

So the chunk can be at most about 6 × 4 MiB. Measured in iteration 3: a 23 MiB chunk
round-trips, and a 24 MiB chunk fails with HTTP 500 because its fragment payload is exactly
4 MiB before the header. **16 MiB** gives fragments of about 2.67 MiB, about 1.3 MiB under
the limit. That leaves room for envelope changes, and it is a power of two, which is how
operators pick chunk sizes. The bound is inclusive: `16777216` is accepted and `16777217`
refused. The floor stays at `1`; very small chunk sizes shrink the largest storable object
through the chunk-map ceilings, which is #739's question, not a validity one.

The ceiling depends on the default durability's k (6 data fragments). If that default ever
moved to a smaller k, 16 MiB could stop fitting. Leg (C) of the criterion is what notices,
which is why it is binding.

One ceiling for both chunk planes, though the local-FS plane has no gRPC: a value that works
on a single node must not start failing when `--endpoints` is added.

### Refuse, do not clamp

`with_chunk_size` turns `0` into `1` silently (`lib.rs:146`). The role must refuse an
invalid value instead, before binding, with the flag named, the way it refuses missing
credentials (`cli.rs:2163-2174`). Keep the `.max(1)` floor inside `with_chunk_size` itself:
it guards library callers against a divide-by-zero on a path this slice does not test, and
removing it widens the public API for nothing.

### Templates

The env example and the blueprint state the default explicitly (`--chunk-size 1048576`), so
the file records a deployment's identity without changing behaviour. The comment says what
the value decides, the accepted range, and that the ceiling comes from the D-server
transport. No memory multiplier: iterations 1–3 stated one twice, and both were wrong.

## Alternatives considered

**Raise the gRPC limit so the ceiling can be higher** (iteration 3's shape). Rejected at
sign-off. A fixed 192 MiB limit on every D server raises its worst-case request buffering
from 64 × 4 MiB = 256 MiB to about 12 GiB (`DEFAULT_MAX_CONCURRENT_REQUESTS = 64`,
`dserver.rs:61`), even on fleets that keep 1 MiB chunks. It also needs every D server,
custodian and gateway upgraded before any gateway uses a large chunk. That belongs to its
own issue (Foundations milestone), where a configurable or chunk-size-derived limit can be
designed.

**Ceiling 20 MiB or 23 MiB.** Both fit today, with about 0.67 MiB and about 170 KiB of
headroom. 23 MiB breaks easily if the envelope grows. The maintainer chose 16 MiB in the
2026-10-01 Plan session.

**Environment variable instead of a flag.** Every other role knob is a flag. The env slots
that exist (`WYRD_S3_ACCESS_KEY`, `WYRD_FDB_CLUSTER_FILE`) are for secrets and paths a
supervisor injects.

**Test through `serve_s3_role` instead of the binary.** Easier to write, but the test would
reference the changed signature and fail to compile on the reverted tree. C4-verify scores
that UNVERIFIABLE, not red (`engine/scripts/run-verify.sh:202-229`). Drive the binary.

## Impact & compatibility

Backwards compatible: the flag is optional and its absence composes today's gateway, so
every existing invocation (compose stacks, systemd units, tests) behaves the same. Changes
outside `cmd_s3`: `serve_s3_role`'s signature if Do threads a parameter through it (it is
`pub`, with one external caller); `with_chunk_size`'s doc comment; the two templates, the
blueprint, and the template test. A fleet of gateways may run different chunk sizes. That is
safe, because reads follow each object's own chunk map, not a process-wide assumption. Say
so in `build-notes.md`.

## Re-plan notes (iteration 3 → this brief)

Sign-off on 2026-09-30 (Eduard Ralph; recorded in `iteration-v3/SUMMARY.md` §9 and
`iteration-v3/session-carry-forward`) sent this back to Plan with these instructions; how
each is handled:

* **Ceiling did not fit the transport.** 1 GiB → **16 MiB**, inclusive, derived above. The
  sign-off asked for a value that fits; the maintainer picked 16 MiB in the 2026-10-01 Plan
  session. A binding leg (C) proves it crosses a real D server with today's limits.
* **Do not ship a fixed 192 MiB limit fleet-wide; transport change is its own issue.** All
  transport work is out of scope. The maintainer will file the follow-up on the Foundations
  milestone (said in the 2026-10-01 Plan session; not filed yet, so no number). It also owns the doc point that a mixed fleet fails at chunk sizes of 24 MiB
  or more (not "above ~24 MiB"). That point does not arise here, because nothing on the
  wire changes.
* **Drop "byte-identical".** Dropped. The default contract is the chunk count in leg (A).
* **Drop `wyrd put` validation and the forced `DEFAULT_CHUNK_SIZE` merge.** Both out of
  scope; `cmd_put` is untouched.
* **Resolve the #736 base claim.** #736 is a closed split parent (#778, #779, both unbuilt).
  Nothing of it is on the base; see the Ordering note.

Lessons from the three test reviews, now in the criterion or scope: pin the local backends
in every spawned role; parse responses strictly; make the template check look at the
`WYRD_S3_ARGS=` line, not the whole file; never import a new symbol into the discriminator
test.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR
MAY happen during the cycle. The PR MUST NOT be marked ready before sign-off accepts.

Plan-review response: finding 1 (unsupported sign-off claim) is partly right and is fixed
in place. The 16 MiB ceiling is a human decision, but it was taken in the 2026-10-01 Plan
session, not at a sign-off; the brief now says so (Success criterion, Alternatives,
Re-plan notes) and cites the 2026-09-30 sign-off record (`iteration-v3/SUMMARY.md` §9)
for the instructions that came from it. The transport follow-up is now described as
"the maintainer will file it", with no number, because no such issue exists on the
tracker yet. The value itself stands; it is not reopened for adjudication.
