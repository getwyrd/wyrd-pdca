# Brief — issue 288 / read-repair-enqueue-integrityfault

> The Plan artifact (docs 02 §PLAN). Human-authored. Do reads ONLY this file.

- **Slug:** read-repair-enqueue-integrityfault
- **Defect:** The read path is documented to feed the shared repair queue when it excludes
  a checksum-failing fragment, but it only does so when the store returns corrupt *bytes*
  that `read.rs` decodes locally. Real verifying stores (`FsChunkStore`, and the gRPC
  client mapping `DATA_LOSS`) return `IntegrityFault` from `get_fragment_at` instead of
  bytes, and `read.rs` does not classify those as corruption findings:
  `EcScheme::None` (`crates/core/src/read.rs:128-137`) propagates the error with `?` before
  `corrupt.push` can run; `EcScheme::ReedSolomon` (`read.rs:188-213`) handles only
  `if let Ok(Some(fragment))`, so every `Err` — including `IntegrityFault` — is silently
  treated as an absent shard. Read-time repair is therefore silently skipped for corrupt
  fragments discovered through the real disk/gRPC backends; single-fragment reads can fail
  without leaving the promised repair-queue entry.
- **Success criterion:** When `get_fragment_at` returns an `IntegrityFault` (the verifying
  store's corruption signal), `read.rs` records the chunk as a repair obligation
  (`corrupt.push(chunk.id)`): for ReedSolomon it reads around the faulted shard and
  continues; for `EcScheme::None` it enqueues the chunk before surfacing the error. A
  transient/non-integrity error is NOT enqueued as corruption. Demonstrated by a regression
  using a store that returns `IntegrityFault` from `get_fragment`/`get_fragment_at`: red
  pre-fix (no repair entry), green post-fix (chunk enqueued).
- **Invariant to restore:** A checksum-failing fragment must NEVER be silently absorbed —
  on the read path it must become a **durable repair obligation** regardless of whether the
  store surfaced corruption as raw bytes (decoded locally) or as a typed `IntegrityFault`;
  the corruption-vs-transient classification must be applied uniformly at every fragment
  fetch site. Source: the `IntegrityFault` seam contract,
  `crates/traits/src/lib.rs` ("a consumer that walks fragments — … the read path — must
  turn it into a durable repair obligation … never retry it"), and proposal 0005 (M3,
  read/scrub repair-enqueue, `0005:174-176`); scrub already honours both shapes
  (`crates/custodian/src/scrub.rs:84,102`, `is_integrity_fault`). Stated over the category
  "corruption surfaced as `IntegrityFault` at a read fetch site," not one repro.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Conflicts with:** 287
- **Ordering note:** Conflicts-with 287 because both land on the `crates/core` read/repair
  path; 288 edits `crates/core/src/read.rs` directly and 287's expansion-centralization may
  touch the same file (`fragment_dserver`) — schedule into different waves so neither builds
  blind on the other's base. No build-on dependency either way.
- **Surfaces:** data
- **Difficulty:** medium
- **Scope:** Mirror scrub's classifier in `read.rs`: treat an `IntegrityFault` from
  `get_fragment_at` as a corruption finding (push the chunk id), read around it for
  ReedSolomon, and enqueue before surfacing the error for `EcScheme::None`. / out of scope:
  changing scrub or the store backends; changing how transient errors are retried;
  reclassifying non-integrity errors.
- **Repro instruction:** On `main`, run an object read through a `ChunkStore` whose
  `get_fragment_at` returns `Err(IntegrityFault { … })` for a target fragment (instead of
  raw corrupt bytes). Observe that no chunk id reaches the repair queue: for `EcScheme::None`
  the error propagates with no `corrupt.push`; for ReedSolomon the shard is dropped as if
  merely absent.
- **Test file:** crates/core/tests/read_repair.rs
- **Verification posture:** Flippable regression at Check. The existing `read_repair.rs`
  tests use an in-memory store returning raw corrupt bytes; Do adds a test double that
  returns `IntegrityFault` from `get_fragment`/`get_fragment_at` (red pre-fix, green
  post-fix). An integration regression with `FsChunkStore`/the gRPC client is desirable but
  the binding criterion is satisfied by the `IntegrityFault`-returning store double — which
  reproduces the EXACT shape the real backends emit (`chunkstore-fs/src/lib.rs` and
  `chunkstore-grpc/src/client.rs` both map to `IntegrityFault`), so it is load-bearing, not
  a mock of the bug away.
- **Citations expected:** Do must cite path:line on the target branch for every change
  (`crates/core/src/read.rs:128-137`, `:188-213`; the scrub classifier it mirrors,
  `crates/custodian/src/scrub.rs:102`).
- **Prior-art check (triage cycles):** Searched `crates/core/src/read.rs` and
  `crates/core/tests/read_repair.rs` history — the read-repair enqueue path landed in
  `6a33a33` and misplaced-fragment handling in `5aece0e`, but neither classifies a typed
  `IntegrityFault` from the fetch; scrub (`6a33a33`/`8c2adcf`) already does. No open PR
  touches these files (`gh pr list` empty). Net-new fix.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.
