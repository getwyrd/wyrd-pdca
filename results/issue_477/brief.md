# Brief — issue 477 / gateway-cluster-coordinated-id-allocation

> The Plan artifact (docs 02 §PLAN). Human-authored. Do reads ONLY this file.
> The field labels below are parsed by the driver — keep the `- **Label:** value`
> shape. The success criterion is the load-bearing field: it is the sentence
> Check tests "did this work" against.

- **Slug:** gateway-cluster-coordinated-id-allocation
- **Defect:** The runnable gateway (`crates/server/src/lib.rs`) mints inode and chunk
  ids from **per-process `AtomicU64` counters** (`next_inode`/`next_chunk`,
  `lib.rs:67-68`), seeded once at startup by `recover()` from
  `metadata::high_water_marks` (`lib.rs:102`) and then advanced with `fetch_add`
  (inode at `lib.rs:176`, chunk at `lib.rs:196`). That is safe for **one** gateway
  process, but two or more gateway processes against the same fleet each seed from the
  **same** persisted high-water mark and then `fetch_add` **independently**, so both
  mint the **same** next inode and — critically — the **same** next chunk id. The
  create-commit CAS rejects the losing inode (`commit_create` → `metadata::create`, the
  conditional create; `write.rs:247`), but
  the **chunk fragments are written to the D servers *before* that commit**
  (`put_object`/`stream_write_data` mint chunk ids at `lib.rs:149`/`lib.rs:243` and
  write fragments before `commit_written`), so the second gateway's fragments overwrite
  the first object's fragments under the colliding chunk id — silent data loss /
  corruption of a committed object. The CLI cluster path does **not** have this bug: it
  allocates inodes via the CAS-backed `meta:next_inode` allocator (`alloc_inode`,
  `cli.rs:1027`) and derives chunk ids from the inode (`chunk_id_minter`, `cli.rs:1064`);
  the gateway was never moved onto that shared, process-independent scheme.
- **Success criterion:** Two `Gateway` instances composed over one **shared**
  `MetadataStore` and one **shared, read-back-observable** `ChunkStore`, both recovered
  from the **same** baseline before either commits (the active-active case), can each
  store a **distinct** object and both objects read back **byte-identical** — with the
  in-process regression test (below) **red before** the fix and **green after**, and
  `cargo xtask ci` exit 0. (Pre-fix the two gateways mint the same first chunk id, so
  the second gateway's fragment write clobbers the first object's fragments on the shared
  chunk store and/or the second create fails with a bogus `Conflict`; either way one
  object no longer reads back byte-identical.)
- **Falsifiability:** RED is producible **in-process on the plain `cargo xtask ci`
  environment** — no real multi-node cluster is needed, because the shared
  `MetadataStore` is itself the coordination point the two gateways collide on. Point Do
  at an in-process integration test (the `gateway_cluster.rs` / `closed_write_path.rs`
  family) that stands up two `Gateway<M,C,Co>` values over a single shared in-memory /
  `FsChunkStore` chunk store and a single shared metadata store, `recover()`s **both**
  from the same (e.g. empty) baseline so both counters seed identically, then drives each
  gateway's PUT of a distinct key and reads both back. Pre-fix the shared per-process
  counters mint colliding chunk id 1 → deterministic clobber → the readback assertion
  fails (RED). Post-fix the coordinated/coordination-free ids no longer collide → both
  round-trip (GREEN). This is the two-active-gateways analogue of the existing
  two-sequential-CLI-invocations test at `crates/server/tests/gateway_cluster.rs`.
- **Invariant to restore:** Every object identifier the gateway mints — the inode id and
  every chunk id — is **globally unique across all concurrently-active gateway processes
  over the same fleet** (and across a single process's restarts): two gateways seeded
  from the same persisted high-water mark MUST NOT mint the same inode or chunk id, so no
  fragment a gateway writes can overwrite an inode or chunk another gateway has committed
  or has in flight. Sources: ADR-0015 **guarantee 2** — "a file's writes are linearizable
  at its home zone; the commit point totally orders its versions … exactly one concurrent
  writer wins" (authoritative, internal); ADR-0019 §2/§Consequences — the chunk id is
  **u128 precisely so identifiers can be generated without central coordination
  (random/UUID-style)**, the sanctioned cluster-safe chunk-id scheme (authoritative,
  internal); and the project's own established allocation rule — the CAS-backed
  `meta:next_inode` allocator (`alloc_inode`, `cli.rs:1027`; the CLI cluster path already
  obeys it) is the process-independent inode invariant the gateway must share (Tier-C
  internal rule). The M4 blueprint (#465, merged; #454) depends on this: "one shared front
  door, N gateways, no LB affinity, plain round-robin." SELF-TEST: a one-module guard
  cannot satisfy this — the property is over *all* id-minting across *separate* gateway
  processes, so it is only restored by making allocation coordinated through the shared
  store (or coordination-free by construction), never by guarding a single call site.
- **Repo + branch target:** getwyrd/wyrd @ feat/m4-production-metadata-backend
  (M4 slice — stacks on the integration branch per INTEGRATION §2, not `main`)
- **Difficulty:** high — structural, cross-cutting change to the gateway write path: it
  removes the per-process `next_inode`/`next_chunk` counters as the id source and rethreads
  both the buffered PUT (`put_object`, `lib.rs:149`) and the **streaming** PUT
  (`stream_write_data`, `lib.rs:243`) plus the create-commit inode allocation
  (`lib.rs:176`) and `recover()` (`lib.rs:102`); minting inodes via the async CAS
  allocator likely makes chunk-id minting cross an async/lifetime boundary it does not
  today. Effects propagate to the S3 composition root (`serve_s3`, `cli.rs:1415`) and to
  every test that leans on the counters. A diff-reviewer must hold the whole gateway
  id-allocation surface in view.
- **Scope:** Remove the per-process `AtomicU64` counters (`lib.rs:67-68`) as the gateway's
  source of inode and chunk ids, so both are allocated in a way that cannot collide across
  concurrently-active gateway processes over the shared fleet — inodes coordinated through
  the shared store, chunk ids either coordinated or coordination-free by construction. One
  logical change: the gateway's id-allocation path. / out of scope: the CLI paths (already
  correct); changing the on-disk chunk/inode key formats or the `MetadataStore` trait; the
  chunk **read**/placement path; any cross-zone / multi-region concern (M10/M11); adding
  new metadata-store backends. Do MUST NOT reintroduce a *same-inode overwrite* collision:
  an overwrite reuses the existing inode but writes a **new version's** fragments, so the
  chunk-id scheme must not re-mint the prior version's chunk ids for that inode (the CLI
  `chunk_id_minter`'s `(inode<<64)|seq`-from-0 scheme is overwrite-unsafe for the gateway;
  see ADR-0019's coordination-free/random option) — the mechanism is Do's to choose, but
  this constraint binds it.
- **Repro instruction:** On `feat/m4-production-metadata-backend`, read
  `crates/server/src/lib.rs:60-197` and `crates/server/tests/gateway_cluster.rs`. Reproduce
  by composing two `Gateway` instances over one shared metadata store + one shared chunk
  store (mirror the loopback D-server setup in `gateway_cluster.rs`, or a shared in-memory
  chunk store), `recover()` both from the same empty baseline, then have gateway A
  `put_object("a", …)` and gateway B `put_object("b", …)`, and assert both
  `get_object` calls return the original bytes. Observe the clobber / bogus `Conflict`.
- **External dependencies:** none beyond the base Rust toolchain — the red→green is an
  in-process integration test over shared in-memory / loopback backends (the
  `gateway_cluster.rs` family already stands up real loopback gRPC D servers with no
  Docker). No live TiKV / etcd / multi-node cluster is required for this criterion.
- **Test file:** `crates/server/tests/gateway_multi_writer.rs` (new) — two active gateways
  over shared metadata + chunk stores; red pre-fix (colliding chunk ids clobber / bogus
  conflict), green post-fix. Do MAY additionally seed a madsim/DST property that two
  gateways never mint a colliding id under contention (the issue's "conformance/DST
  property" ask), but the binding red→green rests on this in-process test so it is
  C4-verify-flippable.
- **Citations expected:** Do must cite path:line on `feat/m4-production-metadata-backend`
  for every change. **Composition slice — mirror the peer that already solves this:** the
  CLI cluster path is the reference. Resolve the inode from the shared store exactly as
  `cluster_store_put` does — `let inode_id = alloc_inode(meta).await?`
  (`crates/server/src/cli.rs:1158`, allocator body at `cli.rs:1027`) — rather than a
  private counter; and mint chunk ids the process-independent way ADR-0019 sanctions
  (coordination-free/random, or inode-derived but overwrite-safe), cf. `chunk_id_minter`
  (`cli.rs:1064`) and the `< 2^64` in-process vs `(inode<<64)|seq` disjoint-space note in
  `high_water_marks` (`crates/core/src/metadata.rs:585-595`). Do MAY open those cited
  callsites to copy the composition; anything else stays out of Do's input.
- **Prior-art check (triage cycles):** Searched `crates/server/src/lib.rs` and
  `crates/server/src/cli.rs` history and open/closed PRs on
  `feat/m4-production-metadata-backend`. The gateway's per-process counters and `recover()`
  were added for the **single-process** restart-replay fix (issue #364 finding 1) — that
  fix is intact and in scope to preserve; this issue is its explicit multi-process
  follow-on. The CAS `alloc_inode` allocator (#255, M4.4) and the CLI cluster path
  (`cluster_store_put`, #155) already exist and are the peer to mirror — not duplicated
  work. Surfaced by the Codex review of #465 (merged); #465 was corrected to say only a
  single active gateway is safe today and to point here. No prior/closed attempt at the
  gateway multi-writer allocation itself. Not a duplicate.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Approach accepted (coordination-free gateway id allocation — shared-CAS inodes + per-process random chunk epoch). C4 is green on a real host rerun (xtask ci: all checks passed) and T5 is clear (no prior closed/rejected attempt supersedes). Iterating only to strengthen the tests before merge: - The active-active regression test (crates/server/tests/gateway_multi_writer.rs) MUST be genuinely CONCURRENT: drive gateway A and B under join!/spawn so it actually exercises contended CAS on the shared meta:next_inode allocator. Today A's PUT fully completes before B's (A->1, B->2, uncontended), so the concurrent invariant the fix rests on is never exercised. - Correct restart_without_recover_is_safe_by_construction (crates/server/tests/s3_http_wire.rs): its claim does not hold for the migration case recover exists for — an older single-process store with inode: keys and no meta:next_inode, started WITHOUT recover, returns id 1 and collides. Either rescope/rename the test to what it actually proves, or make it exercise the recover-from-legacy path so the name matches the mechanism. Do NOT relitigate: the chunk-id probabilistic guarantee (per-process 63-bit epoch) is accepted. The collision-detection gap is out of scope here and is filed as getwyrd/wyrd#478 (Foundations milestone).
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
