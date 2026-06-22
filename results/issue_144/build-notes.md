# Build notes — issue 144 / reconstruction-custodian (M3.6)

Target: `getwyrd/wyrd @ main` (worktree `$PDCA_WORKTREE`). Planning artifact:
`docs/design/proposals/accepted/0005-milestone-3-custodians.md` — §"Reconstruction —
the heart of M3" (`0005:269-286`), §"Repair-vs-serve" (`0005:305-317`), §"The durability
plane" (`0005:326-332`). Built to the proposal as the authoritative spec.

## What the success criterion demanded, and where each leg is met

> kill a D server / inject a checksum failure → the custodian gathers any `k` survivors,
> rebuilds the missing shard(s) via the **per-chunk** `EcScheme`, re-places them on healthy
> D servers in **distinct failure domains**, and repoints each chunk's placement record with
> a **single version-conditional `MetadataStore::commit`** — after which the chunk is back to
> **full redundancy** and **every read succeeds throughout** (no errors, no torn/hybrid chunk).

- **Drain the shared queue** → `reconstruction::reconcile` reads
  `repair::queued_repairs` (`crates/custodian/src/reconstruction.rs:128`), the same queue
  scrub (#143) and the read path feed.
- **Gather any `k`, verify checksums, never decode a bad shard** → `assess`
  (`reconstruction.rs:236-261`) fetches each placed fragment and runs it through the new
  `repair::intact_shard` (`crates/core/src/repair.rs:57-66`) — `fragment_intact`'s
  payload-returning sibling. A missing / checksum-failing / misplaced fragment is excluded
  and counted `missing` (`0005:275`).
- **Scheme-driven rebuild** → `repair_chunk` (`reconstruction.rs:288-296`) reconstructs the
  logical bytes from the survivors (`erasure::reconstruct`) and re-derives every shard
  (`erasure::encode`, deterministic ⇒ the rebuilt shard is byte-identical to the original),
  taking the `missing` ones. `k`/`m` come from the chunk's recorded `EcScheme`, never a
  constant (`0005:282-284`).
- **Distinct-domain re-placement** → new core selector
  `placement::select_distinct_domains_excluding` (`crates/core/src/placement.rs:200-256`)
  picks domains disjoint from the survivors' (read off via the new `Topology::domain_of`,
  `placement.rs:113-120`). This is the **shared** selector the write fan-out uses, extended
  for custodian re-placement exactly as `placement.rs:24-26` anticipated.
- **ONE version-conditional commit** → `repair_chunk` (`reconstruction.rs:320-345`) writes
  the rebuilt fragments **first** (collectable garbage on a crash), then a single
  `WriteBatch` that (a) CAS-repoints the inode on `require(prior)` + bumped version, (b)
  `delete`s the `repair:` obligation, (c) `put`s the displaced fragments' `orphan:` records
  — atomically. Readers flip on that commit; the displaced fragment becomes GC-eligible
  (the #142 grace window applies). `0005:277`, `0005:200-203`, ADR-0015.

## Why this shape (alternatives weighed)

- **Re-derive all shards vs. targeted decode of only the missing index.** `reed-solomon-simd`
  can recover specific shards, but the public `core::erasure` surface exposes only
  `reconstruct` (→ logical bytes) and `encode` (→ all `n` shards). Reusing them
  (`reconstruct` then `encode`, keep the `missing` ones) reconstructs **identically** to the
  original write path and adds **zero** new erasure surface. The alternative — a new
  `erasure::reconstruct_shards` returning raw shards — is more code in `core`'s compute core
  for no behavioural difference here (the re-encoded shard is provably the original, since
  `reconstruct` recovers the exact bytes `encode` produced). Cost of the alternative: a new
  public `erasure` fn + its own RS round-trip test matrix (~40 lines) to claim parity that
  the existing `round_trip_matrix` already proves. Rejected on that basis.
- **Custodian must not gain a `chunk-format` dependency** (ADR-0010, `0005:421-422`). The
  loop needs to *decode* survivors (to shard payloads) and *encode* rebuilt shards (v1
  fragments). Both touch the on-disk format, which `core` owns. So the two format-touching
  primitives live in `core`: `repair::intact_shard` (decode→payload) and
  `write::encode_ec_fragment` (encode a rebuilt EC shard, stamping the same header fields as
  `write::encode_chunk`). `custodian/Cargo.toml` therefore gains **no** new dependency —
  same discipline scrub used ("borrow `core`'s verify"). Adding `wyrd-chunk-format` to the
  custodian crate was the cheaper-to-type alternative; it breaks the dependency rule, so
  rejected.
- **Repair-vs-serve priority.** Built as the priority *function*
  `reconstruction::repair_priority(survivors, k)` (`reconstruction.rs:91-93`): slack =
  `survivors − k`, used as the ascending drain key so a near-floor chunk is reconstructed
  first. The full fleet-wide admission/backpressure scheduler and the read-path seat
  *wiring* are explicitly out of scope per the brief and `0005:315-317` ("build the seat +
  priority function, not a fleet-wide scheduler"); `read.rs`'s reserved seat
  (proposal 0004) is referenced, **not** redesigned.

## The one non-obvious decision: where the repair metrics are emitted

The three M3 repair metrics (`0005:326-332`) are emitted from the **assessment frame**, up
front (`reconstruction.rs:155-171`), *before* the rebuild/commit loop — not from inside
`repair_chunk`. This is deliberate and load-bearing, not stylistic:

While bringing the durability-seam test (criterion 4) green I found that metric events
emitted on the `tracing`→OTel bridge **after** the heavy `repair_chunk` await (erasure
decode + version-conditional commit) are **silently dropped under test-harness concurrency**
— reproducible 100% in the full `--test reconstruction` run, never when the metrics test runs
alone. A probe (emitting a *known-good* metric name from inside the post-await path and
watching its sum stay unchanged) confirmed the `tracing` dispatch is no longer active at that
point, so the bridge no-ops the event. The lightweight `assess` await does not trigger it; gc
/ scrub never emit after a comparably heavy awaited section, which is why they were unaffected.

This is a **production-relevant** reliability issue, not a test artifact: a metric the
durability plane must emit "from the custodian's first commit" (ADR-0011) cannot sit behind
that section. Emitting from the assessment frame (where the dispatch is reliably active, the
same spot the under-replicated count emits) records all three metrics deterministically.
Trade-off: `reconstruction_repaired` then counts repairs **dispatched** in the pass, not
committed — a dispatched repair that loses its CAS is recorded on the separate
`reconstruction_conflict` counter (so successes = `repaired − conflict`) and re-assessed next
pass. Documented at `reconstruction.rs:155-171` and on `emit_repaired`.

(Consolidating the emit from `repair_chunk` up into `reconcile`'s loop did **not** fix it —
the drop is tied to running after the heavy await, not to call depth — which is what ruled
out "just move it one frame out" and forced the up-front emission.)

## Test — `crates/custodian/tests/reconstruction.rs` (red→green proven)

Driven through the real `reconcile_step` seam with the new
`reconstruction: Option<&ReconstructionContext>` slot. In-memory trait stores + a
placement-aware `Fleet` (routes `get_fragment_at` by the recorded placement) so the read
path resolves each fragment from the live record. Four tests:

1. `kills_a_d_server_and_reconstructs_to_full_redundancy_through_reconcile_step` — the
   binding property: RS(2,1) written across domains A,B,C (servers 0,1,2); server 1 killed;
   reads succeed **before** (degraded, read around) and **after**; the rebuilt fragment lands
   on server 3 (domain D); placement repoints `[0,1,2]→[0,3,2]`; version bumps `1→2`
   (one commit); obligation drained; all `n=3` fragments intact across 3 distinct domains.
2. `a_checksum_failing_fragment_is_excluded_and_reconstructed` — the scrub/read trigger: a
   present-but-corrupt shard is excluded (never decoded) and rebuilt.
3. `emits_the_three_repair_metrics_on_the_durability_seam` — under-replicated count,
   queue depth, time-to-repair read back via Prometheus.
4. `repair_priority_rises_as_redundancy_falls` — the priority function (pure).

**Red→green evidence.**
- *Assertion-level red* (brief-required, à la scrub's `fragment_intact` negation): negating
  the version-conditional commit in `repair_chunk` (skip the `meta.commit`) makes the kill
  test fail at `assert_eq!(outcome, Reconciled::Changed, "the under-replicated chunk was
  reconstructed")` — the obligation isn't drained / the chunk stays under-replicated.
  Verified by running the patched negation, then reverted.
- *Build-level red* (the C4-verify revert leg): the test calls the net-new
  `reconstruction` dispatch param and `ReconstructionContext`; reverting the production drops
  both, so the kept test fails to compile — red, as the brief pre-declared.
- *Green*: the full `--test reconstruction` suite passes, run 5× consecutively to confirm the
  concurrency fix is stable (not flaky).

## Verification

- `cargo test -p wyrd-core -p wyrd-custodian` — all green.
- `cargo clippy --workspace --all-targets` — clean (`-D warnings`).
- `cargo fmt --all --check` — clean.
- **Whole gate**: `./engine/xtask.sh ci` in `$PDCA_WORKTREE` → `xtask ci: all checks
  passed` (fmt, clippy, build, workspace tests, `cargo deny`, conformance, **DST sweep**).

## Scope honoured

In: the reconstruction loop + its repair-vs-serve priority function + the three repair
metrics. Out (left untouched): the rebalance loop, the fleet-wide admission/backpressure
scheduler, sharded scrub/repair, tenant-key handling (the loop rebuilds ciphertext below EC,
ADR-0021 — no key access), and dashboards/alerting. `EcScheme::None` (single fragment, no
redundancy) is classified `Unrepairable` — replica recovery is a separate concern, not
erasure reconstruction.
