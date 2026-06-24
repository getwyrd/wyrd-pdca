# Brief — issue 207 / scrub-corruption-enqueue-and-continue

> The Plan artifact (docs 02 §PLAN). Human-authored. Do reads ONLY this file.
> The field labels below are parsed by the driver — keep the `- **Label:** value`
> shape. The success criterion is the load-bearing field: it is the sentence
> Check tests "did this work" against.

- **Slug:** scrub-corruption-enqueue-and-continue
- **Defect:** Bit-rot detection and repair are silently broken for the `FsChunkStore`
  backend (the only on-disk backend — dev/NAS *and* the interim production networked D
  server, ADR-0032). `FsChunkStore::get_fragment` verifies on read and returns **`Err`**
  for a corrupt/misfiled fragment (`crates/chunkstore-fs/src/lib.rs:87-97`,
  `Self::verify(id, &bytes)?`; `verify` at `:53-66` checks valid decode **and**
  `chunk_id`/`index == id`). But scrub assumes `get_fragment` returns raw bytes for it to
  check: `let Some(bytes) = store.get_fragment(frag).await? else { continue };`
  (`crates/custodian/src/scrub.rs:70`) — the `?` **propagates the `Err` out of the whole
  `reconcile` pass**, *before* the corruption branch (`if !repair::fragment_intact(...)`
  → `emit_corruption` → `repair::enqueue_repair`) at `:79-83` ever runs. Consequences:
  (1) the corruption branch is **dead code** for this backend (any bytes reaching `:79`
  already passed the same checks `verify` makes); (2) **bit rot is never enqueued for
  repair**, the opposite of scrub's contract; (3) the pass **aborts at the first** corrupt
  fragment every pass (deterministic, self-perpetuating — fragments after it never get
  scrubbed); (4) **the telemetry lies** — `emit_scrubbed`/`emit_corruption` sit after the
  aborting `get`, so `scrub_coverage` plateaus and `scrub_corruption_rate` stays `0`
  while data rots (the "silently below the redundancy floor reports all-green" failure
  mode, `docs/design/architecture/08-crosscutting-concepts.md:50`). Holds across the
  network path too: server maps the store `Err` → `Status::internal`
  (`crates/chunkstore-grpc/src/server.rs:46-60`) → client lumps it into a catch-all `Rpc`
  (`crates/chunkstore-grpc/src/error.rs:30-38`) → scrub's `?` aborts. There is **no
  `DATA_LOSS` code and no `TransportError::Corrupt` variant**, so "corrupt → repair" is
  indistinguishable from "transient → retry."
- **Success criterion:** A scrub pass over a D server holding **one bit-rotten referenced
  fragment among several** (a) **enqueues that fragment's chunk for reconstruction** on
  the shared repair queue, (b) **emits a corruption metric**, and (c) **continues** to
  scrub the remaining referenced fragments — it never aborts the pass; and the corrupt
  bytes are never decoded/served as valid payload. The corruption signal is
  **distinguishable** from a transient/unavailable fault at scrub's decision point (so
  scrub can choose enqueue+continue for corruption vs. propagate/retry for transient),
  including across the gRPC seam. Demonstrable at C4-verify by a flippable scrub test.
  BINDING is legs (a)+(b)+(c) plus the corrupt-vs-transient distinguishability; the exact
  plumbing (a `TransportError::Corrupt` variant, server `Code::DataLoss`, a typed
  store-corruption error vs. another distinguishing scheme) is ILLUSTRATIVE — Do's call,
  provided corruption is actionable and distinct from transient faults.
- **Invariant to restore:** Scrub's durability guarantee holds for every backend: a
  checksum-failing / misplaced **referenced** fragment is **never silently absorbed** — it
  is always turned into a durable repair obligation (`enqueue_repair`) and recorded on the
  corruption/coverage telemetry, and a single such fragment never stalls scrub for the
  rest of that D server (the pass continues). This requires that a *corruption* finding be
  **distinguishable** from a *transient* fault along the path from the store to scrub's
  decision point, so the two are handled differently (repair vs. retry) rather than
  collapsed. Source: scrub's documented contract in proposal 0005 (the M3.5 scrub slice)
  + architecture §8.3 admission/observability — a storage system silently below its
  redundancy floor must **not** report all-green
  (`docs/design/architecture/08-crosscutting-concepts.md:50`). Internal project invariant
  (Tier C), authoritative project docs. (Structural fix — a control-flow/error-contract
  seam per principles.md §1.2; target is the smallest change that restores the guarantee,
  not the smallest diff. Plan-exit self-test: a one-module patch confined to scrub.rs —
  e.g. merely not `?`-propagating — visibly **fails** this, because without the
  corrupt-vs-transient distinction scrub cannot correctly decide enqueue+continue vs.
  retry; the invariant spans the store error contract and the gRPC classification seam,
  so it is not satisfiable by guarding one module.)
- **Repo + branch target:** getwyrd/wyrd @ main   (resolve here at Plan — do not leave to Do)
- **Depends on (merged):** 203   (both this brief and 203 edit
  `crates/chunkstore-fs/src/lib.rs`; 203 is that file's chain base, so building this on
  203's **merged** result avoids a conflict on the shared `verify`/`get_fragment` region.
  The structural-rewrite brief depends on THIS one in turn — do not invert. The adjacent
  read-path brief edits only `crates/core/src/read.rs`, which this brief excludes, so it
  carries no constraint here.)
- **Surfaces:** data

> **Merge chain — `crates/chunkstore-fs/src/lib.rs`: 203 → 207 → 204.** This brief is the
> middle link: held until 203's PR is merged (so its `get_fragment`/`verify` edits land on
> 203's unique-temp `put_fragment`), and the I/O-offload brief is held until THIS one
> merges. The scrub / gRPC-classification edits (`scrub.rs`,
> `chunkstore-grpc/src/{server,error}.rs`) touch no other brief's files; only the
> `get_fragment` corruption-error change shares `chunkstore-fs/src/lib.rs`, which the chain
> serializes.
- **Scope:** restore scrub's bit-rot guarantee end-to-end — a corrupt referenced fragment
  becomes a durable repair obligation, emits a corruption metric, and the pass continues;
  corruption is carried distinguishably from transient faults from the local store across
  the gRPC seam to scrub's decision point; and a client's malformed-fragment **PUT**
  verify failure is reclassified as an invalid-argument (client) fault rather than an
  internal (server) fault that invites futile retries (same error-classification seam,
  `server.rs:46-60` / `error.rs:30-38`). / out of scope: the **read-path** `chunk_id`
  recheck (DoD item 5 — confirm/repair `crates/core/src/read.rs`): **#198 owns it**; this
  brief does not edit `read.rs` (declared as a conflict). Also out: the temp-path race
  (#203), the blocking-I/O offload (#204), the d-server backpressure tuning (#205), and
  the wire/encoding layers (confirmed sound by the issue — chunk-id `fixed64` reassembly,
  `optional bytes` absent-vs-empty, `u16::try_from` index).
- **Repro instruction:** On `main` @ `c2223a5`, in a custodian scrub test: populate a
  `FsChunkStore` with several referenced fragments, then corrupt one referenced
  fragment's on-disk bytes (flip bytes so its checksum fails, or misfile it so
  `header.chunk_id` ≠ its id) such that it is **not the last** in iteration order. Drive a
  `reconcile_step` scrub pass. Observe the pass returns `Err` (aborts) at that fragment:
  no `enqueue_repair` for its chunk, no `emit_corruption`, and the referenced fragments
  after it are never scrubbed — `scrub_coverage` plateaus and `scrub_corruption_rate`
  stays 0.
- **Test file:** crates/custodian/tests/scrub.rs   (one bit-rotten referenced fragment →
  pass enqueues its chunk for reconstruction, emits a corruption metric, and still scrubs
  the rest — red pre-fix where the pass aborts, green post-fix; the gRPC corrupt-status /
  PUT-`INVALID_ARGUMENT` classification legs covered by unit assertions in
  crates/chunkstore-grpc/tests/round_trip.rs)
- **Citations expected:** Do must cite path:line on the target branch for every change.
- **Prior-art check (triage cycles):** searched `crates/custodian/src/scrub.rs`,
  `crates/chunkstore-grpc/src/{server,error}.rs`, and `crates/chunkstore-fs/src/lib.rs`
  across merged history (`6a33a33`/PR #189 introduced scrub with this aborting `?`;
  `76cd59b` introduced the catch-all client classification; neither distinguishes
  corruption), open PRs (`gh pr list --state open` — none touch these files), and closed
  PRs — no prior or in-flight fix.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.
