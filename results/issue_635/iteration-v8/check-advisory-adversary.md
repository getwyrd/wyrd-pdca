# Adversarial review — issue 635 / segmented-chunk-map

Attempted to refute: the leg-A red→green story, the settled encoding + decode invariants,
the staged committer's refuse-before-durable ordering, the resume-prefix verification, the
epoch scoping, the repoint CAS, and the flat decode→encode identity. **Could not** — the
encoding is byte-identical for legacy records (`crates/core/src/metadata.rs:4161`), every
invariant in the brief's leg B(ii) list has a raw-byte negative case
(`crates/core/src/metadata.rs:4234`), leg A's fixture names only base symbols so its RED is
genuinely an assertion (`crates/custodian/tests/segmented_map_consumers.rs:53-73`, verified
against `git show HEAD:crates/core/src/read.rs`), and the repoint binds home to root
generation (`crates/core/src/metadata.rs:2595-2614`). Three refutations landed.

- NEEDS-HUMAN [impl] — **`flip()` publishes a root over a range it never checked is
  complete.** `crates/core/src/metadata.rs:3244` calls `verify_durable_range`, whose only
  presence check is `planned[..claimed]` where `claimed == self.resume_from`
  (`crates/core/src/metadata.rs:3163`, `:3200`); the tail check
  (`PublicationTailStranded`, `:3188`) only catches *extra* records. Concrete case, using
  the repo's own recovery flow: `crates/dst/tests/custodian.rs:2005` builds `recovered`
  with `resume_from = 2` and `planned_next.len() > 2`. Drop the `write_segments` call at
  `:2018` (or let it return `Conflict` — `commit_batches` stops at the first batch that
  does not commit, `crates/core/src/metadata.rs:3228`, and a caller that retries the flip
  reaches exactly this state) and `recovered.flip(&meta)` returns `Ok(Committed)`: the
  durable range holds indices {0,1}, the flipped root names N segments, and every later
  `resolve_chunk_map` of that inode is `Err(SegmentAbsent)` — permanently unresolvable,
  "silent at publication, terminal at read", the exact failure class carry-forward item 3
  and `:3141-3160` were written for. The doc at `:3239-3243` claims the flip re-verifies
  precisely for "a completer that drove the phases separately", but with `resume_from <
  planned.len()` — the normal recovery value — it verifies only the prefix that was already
  durable before the phase ran. Fix is phase-specific: at flip time the whole plan must be
  present, not `planned[..resume_from]`.

- NEEDS-HUMAN [impl] — **the containment table's `reconciliation_status` row is not
  implemented and not tested.** With the damaged object of leg A(vii) seeded,
  `reconciliation_status` (`crates/custodian/src/desired_state.rs:157`) calls
  `referenced_fragments`, which now propagates the resolver's error with `?` at
  `crates/custodian/src/gc.rs:265` → `retired_or_fail` → `Err(SegmentAbsent)`
  (`crates/core/src/metadata.rs:2233`). So a *single* damaged object makes the drain-status
  surface return `Err` for **every** D server in the store, where the brief settles the
  answer as "`PendingMalformed` — refuse to certify, **attribute** the blocker, keep going"
  (Design § Failure containment, mirroring `desired_state.rs:166-179`). A reviewer can
  rationalise this as conforming ("`Err` is not `Satisfied`"), which is exactly why it slipped:
  the row asks for containment, and `Err` is the store-wide blast radius the containment table
  exists to prevent. Leg A(vii) never calls `reconciliation_status` over the damaged store
  (`crates/custodian/tests/segmented_map_consumers.rs:1097-1198` asserts only the id floor,
  the healthy reads, the typed per-object failure and the fragment counts), and the drain leg
  that does call it (`:707`) seeds no damaged object — so no test in this bundle would have
  gone red on it.

- NEEDS-HUMAN [human] — **the whole `max_chunk` half of `high_water_marks` is unreachable
  from production, and its stated justification is false on this tree.** The sole production
  caller discards it: `crates/server/src/lib.rs:124` — `let (max_inode, _max_chunk) = …`,
  because chunk ids are minted `≥ 2^127` from a random per-gateway epoch
  (`crates/server/src/lib.rs:238`, base code, unchanged by this patch). Yet this diff adds
  `segment_chunk_floor` (`crates/core/src/metadata.rs:3870`, a **paged walk of the entire
  `seg:` namespace executed on every `Gateway::recover`**) plus ~200 lines of byte-level id
  scavenging (`:3691`, `:3710`, `:3753`, `:3808`, `:3823`) and eight tests, all justified at
  `:3947-3954` by "a floor below a live id costs an object its bytes (issue #364)" — a
  hazard no live allocator can reach here. The result is a startup cost that scales with the
  segment namespace, bought for a discarded value, in the slice whose iteration-7 rejection
  was reviewability. This needs a human call because the brief itself asked for it (leg
  A(vii)(a) pins the chunk-floor property), so it is the brief that is stale, not the build.

- NEEDS-HUMAN [human] — **maintenance passes go from one scan to O(N) point reads per pass,
  and reconstruction to O(queue x N).** Every consumer now resolves through
  `resolve_current_chunk_map`/`_homes`, which re-`get`s the root per object even for a
  **flat** map (`crates/custodian/src/resolve.rs:81`, `:101`; `crates/core/src/metadata.rs:2320`).
  `crates/custodian/src/reconstruction.rs:602` (`find_chunk`) is called once per queued
  repair from `assess`, and now performs a `get` per inode inside its `inode:` scan — so a
  pass with Q obligations over N inodes issues up to Q x N sequential round trips where the
  base issued Q. On a networked backend (FDB/TiKV) that is a scalability cliff, not a
  constant factor. It is deliberate (`resolve.rs:27-44` argues the stale-snapshot hazard),
  which is why it is a human call rather than an iteration: keeping the live-root re-read
  for the *segmented* shape only, or resolving lazily in `find_chunk`, would keep the stated
  safety property for the shape that needs it without the flat-map round trips.
