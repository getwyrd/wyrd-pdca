# Adversarial review — issue 681 (`passes-read-through-resolver-contained`)

Advisory only; nothing here gates. Every citation is grounded on the target tree at
`/home/eddie/wyrd/wyrd.pdca-wt-l1`. The red→green evidence was re-run independently
(source copied to scratch, the three production files reverted with `git checkout`, the new
test kept): **8/8 red pre-fix as assertion/behaviour reds, 8/8 green post-fix** — the C4-verify
row's claim stands, and the reds are not compile reds. `cargo test -p wyrd-custodian` is green
on the target. Findings below come from probe tests I wrote against the patched tree (and,
where a regression is claimed, re-run against the reverted tree); the scratch copy has been
removed.

## Findings

- **NEEDS-HUMAN [impl] — `crates/custodian/src/rebalance.rs:279-280` refuses (and logs) *per
  chunk*, so one ordinary segmented object floods the durability seam every pass.** Concrete
  case, measured: a single segmented object of 64 chunks whose fragments sit on a draining
  server produced **64** `action="refused"` audit warns and **64**
  `rebalance_evacuation_refused` counter increments in one `rebalance::reconcile` pass, while
  the sibling path in this same patch (`crates/custodian/src/backfill.rs:172`) emits **one**
  `action="declined"` record carrying a `chunks` count for the same object. A segmented map
  only exists because the flat map exceeded the 100 KB value ceiling
  (`crates/core/src/metadata.rs:249-254`), i.e. thousands of chunks, and under any RS scheme
  spread across the fleet *every* chunk of that object has a fragment on the draining server —
  so a 10 GiB multipart object emits ~10⁴ warn events plus ~10⁴ counter increments **on every
  rebalance cadence, indefinitely** until #682 lands. The brief's cited peer callsite
  (`crates/custodian/src/gc.rs:155-165`) attributes **per object**; `emit_declined` already
  shows the aggregating shape in this diff. Fix is local: accumulate refused chunks per object
  and emit once with a count (keep `refused` charged per chunk if the certification arithmetic
  wants it).

- **NEEDS-HUMAN [impl] — `crates/custodian/src/reconstruction.rs:271` names a refusal only in
  the assessment loop, so a later transient fault costs the operator the attribution the brief
  told this slice to protect.** Concrete case, measured: two queued obligations — the first
  (`repair:41216`, first in `queued_repairs` order) on a flat record whose fragment fetch
  raises a *transient* chunk-store fault, the second resident in a healthy segmented record.
  The pass propagates at `crates/custodian/src/reconstruction.rs:502` and emits **zero**
  `action="refused"` records: the operator gets `Err(... d server busy)` and no name for the
  object that is actually blocking the repair. The refusal is already known at
  `crates/custodian/src/reconstruction.rs:885` / `:917-928` (where `Site::Refused` is
  recorded); emitting there — as `emit_unresolvable` correctly does, mid-walk — is the
  placement the brief demanded ("attribution emitted by the consumer, per object, **before**
  the rest of the pass, so a later transient fault cannot cost the operator the record's
  name… mirror the placement, not just the call", brief §Citations expected). Note a
  *readable* segmented record produces no `unresolvable` record at all, so `emit_refused` is
  its **only** attribution — this is not a duplicate signal.

- **NEEDS-HUMAN [human] — `crates/custodian/src/reconstruction.rs:917-928` turns a repair the
  base performed into a permanent, never-certifying stall when one committed record names the
  same `ChunkId` twice.** Measured both ways with the same fixture: on the reverted tree the
  pass answers `Changed`, rebuilds the chunk and drains the obligation; on the patched tree it
  answers `Blocked`, logs `ambiguous-chunk-id`, and the obligation stays queued forever — the
  store never converges and reconstruction never certifies again. The brief's duplicate rule
  is written for two *objects* ("both keys are named"), and its stated rationale
  (`crates/custodian/src/reconstruction.rs:402-405`: "repointing the wrong record loses the
  other object's bytes") does not hold when both references live in the one record the pass
  would CAS. Honest caveat, which is why this is a judgment call rather than a build defect:
  the production minter (`crates/server/src/cli.rs:1964-1971`, `(inode<<64)|seq`) cannot emit
  an intra-record duplicate, so the input is corruption-only — but C-1 ("nothing exits the
  state") is exactly the invariant the brief invoked, and the base did exit it.

- **NEEDS-HUMAN [human] — `crates/custodian/src/reconstruction.rs:170-174`: with an empty
  repair queue the pass certifies `Satisfied` over a store holding an undecodable committed
  record, and names nothing.** Measured: one undecodable `inode:1` beside one healthy object,
  empty queue → `outcome=Satisfied`, zero `unresolvable-chunk-map` records. That is precisely
  the claim the same function forbids twenty lines later
  (`crates/custodian/src/reconstruction.rs:176-180`: "even when no obligation names one of
  their chunks, this pass has answered over LESS than the store, and saying `Satisfied` there
  tells an operator redundancy is whole"). It is base-parity and deliberately documented at
  `:164-169` (an idle pass should not resolve the namespace), and success-criterion leg (3)
  cannot see it because its fixture always carries a queued obligation — so the reviewer had
  no gate to trip. The human call: is a certification answer that flips with queue depth the
  intended reading of "a pass that refused work does not certify", or should the carve-out be
  narrowed (e.g. still walk when the previous pass reported unreadable records)?

## Refutations attempted that failed

- *Is the red a real red on the production path?* Reverting only the three `src` files and
  keeping `crates/custodian/tests/segmented_map_passes.rs` reproduces 8 failures, each an
  assertion or a `Store(SegmentedMapUnsupported{...})` behaviour red — no missing-symbol
  compile red, no vacuous green, and the tests drive the real entries (`reconcile_step`,
  `backfill::reconcile`) over trait doubles, not a re-implementation.
- *Does the backfill gauge regress now that `emit_remaining` counts inside the walk
  (`crates/custodian/src/backfill.rs:233`) instead of re-scanning?* I looked for a population
  the old post-pass scan counted and the new walk does not: a malformed chunk cannot have an
  empty `placement` (`crates/core/src/metadata.rs:219-230`), a filled record contributes 0, a
  declined record contributes `to_fill.len()`, a lost CAS re-reads the live generation. Probed
  the mixed fill+malformed+full store: gauge 0 then 0, same as the base would publish. No
  divergence found.
- *Do two obligations in one record now collide on the shared `Arc<InodeRecord>` snapshot?*
  The second repair loses its CAS and stays queued — but the reverted tree behaves identically
  (all assessments precede all repairs there too), so it is pre-existing, not this diff's.
- *Can `plan.prior_chunks` ever disagree with `plan.prior.chunk_map.as_flat()`?* No: a
  segmented `resolved.record` is refused before any plan is built
  (`crates/custodian/src/rebalance.rs:232-239`, `reconstruction.rs:869-889`), and the resolver
  returns the record and chunk list from the *same* generation
  (`crates/core/src/metadata.rs:2619-2632`, `:2652-2687`), including on the supersede restart.
- *Is the store-fault leg mocked away?* No — the injected fault is a plain `io::Error` on
  `get` under `inode:`, which only the segmented resolve's settle re-read issues
  (`crates/core/src/metadata.rs:2570`), and it propagates through the real downcast rule.
- *Docs currency:* `docs/design/architecture/06-runtime-view.md:29-31`'s "a consumer that has
  not yet adopted it refuses a segmented map outright" still reads true for the remaining
  non-adopters (`crates/core/src/read.rs:96`, `commit_chunk_map`, `high_water_marks`), so the
  confirm-only touch was correctly a no-op.
- One further behaviour I deliberately did **not** score as a refutation, since the brief
  settles it and #682 tracks it: a *healthy* chunk resident in a segmented record whose stale
  obligation can never be drained keeps reconstruction permanently `Blocked` on an otherwise
  healthy store (measured over three consecutive passes: `Blocked`, obligation still queued),
  even though discharging it needs no write at all.

## On the gate rows

- `C4-verify` "PASS — red without the fix, green with it (8 test(s) ran red)" — independently
  reproduced; the claim is warranted.
- `T4-batch-review` is **fail (gating)** with 4 blocking findings whose log
  (`review-b…`) is not in this artifact-only bundle, so I could not triage or contest them;
  that row, not this file, is what blocks.
