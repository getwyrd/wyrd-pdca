# Build notes — issue 652 / recovery-total-over-damage (iteration 10)

*Withheld from the reviewer. Written for the human at sign-off.*

## 0. What this round had to change (the carry-forward)

Iteration 9 was **not** rejected on substance: the advisory reviewer PASSed C1–C5, T1–T3 and
T5, `C4-ci` was green, `C4-verify` was green (0/4 → 4/4), and `C5-mutants` was green (8
mutants, 2 caught, 6 unviable). The one gating failure was **T4 batch review: "2 blocking, 0
recorded-rejected"** — the two findings in `review-batch.md` had never been *triaged*. The
gate's own rule is "fix it, or record it in `review-rejected.md` as
`<file:line> | <CLASS> | <MATCH> | <reason>`"; iteration 9 did neither, so the gate blocked on
unchecked findings rather than on the patch.

So this round keeps the shape the reviewer already validated (it is what the brief's Success
criterion prescribes) and adds the missing work:

1. **Both findings are dispositioned**, each recorded twice — at the line round 9 reported it
   at and at the line the same content lands on here — in `review-rejected.md` §"Iteration 10"
   (5 entries).
2. **Each is answered in the tree, not only in the bundle**: finding 2 carries an in-code
   `deferred: getwyrd/wyrd#687` marker at both sites a reader meets it
   (`crates/core/src/metadata.rs:2124-2132`, `crates/server/src/lib.rs:135-140`) — the target's
   own reviewer protocol treats an in-code `// deferred: #N` as settled (`AGENTS.md`
   *Reviewer protocol*, "Deferrals are settled"); finding 1's residual is written out where the
   repair happens (`crates/server/src/cli.rs:1715-1728`) instead of living only in a bundle file.
3. Two small strengthenings that fell out of the analysis: the core unit test now also carries
   a row whose **key** does not parse (`crates/core/src/metadata.rs:3487-3530`), covering the
   third containment arm end-to-end at unit level; and the `seed_next_inode_floor` doc now
   states *which* ids a stored-record floor cannot witness and why re-issuing one cannot
   re-mint a chunk id on the path that calls it.

Nothing about the previous approach is re-submitted "unchanged": the disposition work is
exactly what the failing gate asked for, and it is what a reviewer re-running the same passes
will now find already answered.

## 1. The two round-9 findings, and why neither is fixed inside this slice

### Finding 1 — `cli.rs:1740` (round-9 anchor) "the repair can rewind past deleted or allocated-but-not-live inode IDs"

The finding has a **false half** and a **true half**, and the true half is not this call's.

*False half* — "rewind a live allocator". The repair is a compare-and-set on the **exact bytes
that were read** (`crates/server/src/cli.rs:1758-1762`). A peer that writes a readable counter
between recovery's read and its commit wins; the retry re-reads, sees a value at or above the
floor, and leaves it. That is pinned deterministically, not by luck, in
`the_counter_repair_yields_to_an_allocator_that_won_the_race`
(`crates/server/tests/gateway_recover_totality.rs:487-527`) — a forwarding store that lets the
peer land its write inside the production `get`, driving production
`cli::seed_next_inode_floor`. Replace the guard with an unconditional `put` and the final
assertion fails.

*True half* — an id that **no live record names** (deleted; or handed out and not yet
committed) is not witnessed by any floor derived from stored records. Three reasons this stays:

- The brief's Success criterion 3 requires the repair ("Afterwards the counter is ≥ the
  recovered floor, so the next PUT still commits above every committed inode id"). The
  alternative — leave the unreadable bytes in place — starts a gateway that is **write-dead**:
  `alloc_inode` parses the same key (`crates/server/src/cli.rs:1655`), so every new-key PUT
  fails until a human hand-writes the counter. That is the permanent, manual-repair-only
  failure mode C-1 forbids and this slice exists to end.
- The residual **predates this patch and is unchanged by it**: the absent-counter arm of the
  same loop (`cli.rs:1753`, `None => Some(1)`) has seeded exactly this floor over a store with
  no counter since #364 finding 1, and `alloc_inode` itself defaults to id 1 on an absent
  counter (`cli.rs:1656`).
- The finding's collision mechanism needs a minter that derives chunk ids **from the inode**.
  This function's only production caller is `Gateway::recover` (`crates/server/src/lib.rs:141`),
  whose minter is the per-process random epoch (`lib.rs:246-258`, ADR-0019) — inode reuse
  re-mints nothing. The inode-derived minter is the CLI cluster path (`chunk_id_minter`,
  `cli.rs:1809-1816`), which allocates through `alloc_inode` (`cli.rs:1903`) and never seeds,
  so this repair cannot rewind it. Reaching the hazard needs an operator pointing both
  compositions at one store — the #364 residual, not a new one.

**Cost of the rejected alternative, concretely.** Witnessing deleted ids means restoring the
`orphan:` ledger scan this slice deletes (base `crates/core/src/metadata.rs:2105-2111`) plus a
scheme classifier and an `inode = chunk >> 64` derivation: ≈ +40 lines, and it rebuilds the
byte-scavenging floor apparatus the Plan-settled DELETE removes (brief do-not-re-earn (iv)).
It is also **wrong**: a gateway-minted chunk id is `(random_epoch << 64) | seq` with the
epoch's top bit set (`crates/server/src/lib.rs:274-281`), so `chunk >> 64` is ≥ 2^63 — the
"floor" it produces would seed the inode counter to ≈ 9.2 × 10^18 and exhaust the id space
over the very orphan record it meant to respect.

### Finding 2 — `metadata.rs:2143` "an unreadable `inode:<u64::MAX>` seeds `u64::MAX` and `alloc_inode` overflows at `id + 1`"

Real, and **not introduced here**. The mark has always come from the row's *key*
(`parse_inode_key`; base `crates/core/src/metadata.rs:2078`, this patch
`crates/core/src/metadata.rs:2155`), so an ordinary **decodable** record at
`inode:18446744073709551615` produces `mark == u64::MAX`, a saturating floor and the same
unguarded `id + 1` (`crates/server/src/cli.rs:1662`) on `origin/main` today. What this patch
widens is which *values* at that key survive the walk — not the allocator's arithmetic. (The
workspace sets no `overflow-checks`, so a release build would wrap; that is precisely why the
guard belongs where ids are handed out.)

Fixing it inside this slice would mean either seeding **below** the mark — the one thing the
brief's *Invariant to restore* forbids ("a floor an allocator trusts is never a quiet
under-approximation") — or refusing to start over an exhausted id space, which is the totality
this slice restores. The guard belongs in `alloc_inode`, which the brief puts out of scope
twice ("Out of scope: … any `alloc_inode` change"; do-not-re-earn (v)) and which #687 owns
after the v7 re-plan. So it is deferred **in the tree**, with the marker at both sites.

## 2. The change, file by file

- `crates/core/src/metadata.rs:2149-2184` — `high_water_marks` returns the inode mark alone and
  is **total**: the mark comes from the key *before* the value is touched
  (`:2154-2165`); an undecodable value, a segmented root, and a row whose key is not
  `inode:<id>` are each **attributed** and the walk continues (`:2166-2181`); a `scan` failure
  still propagates (`:2151`), the split `crates/custodian/src/gc.rs:355-359` already draws.
  Attribution helper at `:2035-2083`, shaped like the read path's fault reporter
  (`crates/core/src/read.rs:212-240`) and rendering keys with `escape_ascii` (injective, so two
  damaged keys never print alike). The `pending:` / `orphan:` walks, the
  `IN_PROCESS_CHUNK_CEILING` logic and `parse_pending_chunk_key` are gone with the mark they
  fed (base `:2033-2040`, `:2088-2111`).
- `crates/core/src/metadata.rs:3487-3530` — the standing test
  `high_water_marks_refuses_a_segmented_root_rather_than_re_mint_its_chunk_ids` (base `:3417`)
  is **replaced** by `high_water_marks_is_total_over_records_it_cannot_read`, with the reason
  the removal is not a dropped guard written into the test's own doc (the hazard needed a
  minter allocating below 2^64; `fdd34f1` (#487) removed the last one and rewrote the callsite
  to discard the floor in the same commit).
- `crates/server/src/lib.rs:133-142` — `recover` takes the narrowed result; `_max_chunk` is
  gone. Docs at `:114-132`, `:246-258` and `:261-273` follow the fact.
- `crates/server/src/cli.rs:1734-1773` — `seed_next_inode_floor` reads the counter leniently, so
  unreadable bytes state *nothing* (`None`), are attributed with a bounded, escaped quote of
  what the repair replaces (`:1791-1805`), and are repaired to the floor under the existing CAS.
  Termination: every writer of the key only ever raises a numeric counter, so the winning
  values strictly increase and the loop cannot cycle (`:1766-1771`).

Size: **+833 / −118**, 4 files, **421 semantic (non-blank, non-comment) added lines** — inside
the brief's ≤ ~450 / ≤ 5 budget. No `scan_page`, no `for_each_page`, no `RecoveredIds` /
`ClassIds` / `torn_digit_escape` / `scavenged_chunk_id_floor`, no `alloc_inode` change, no
`require_absent(inode_key(…))` guard, no docs paragraph, no ADR/spec/proposal, no new
dependency.

## 3. Alternatives ruled out (with their cost)

| Alternative | Why not | Cost, concretely |
|---|---|---|
| Keep the chunk mark and harden it | Settled DELETE at Plan (maintainer, 2026-08-02); no minter allocates below 2^64 since #487, so wiring it means inventing a consumer | #647's attempt reported **0** for a corrupted flat root — an under-approximation a recovery path must never produce |
| Page the walk (`scan_page`) | Brief forbids it; `scan_page` declines snapshot isolation (`crates/traits/src/lib.rs:1061`), which *weakens* the floor and is what dragged the last seven rounds into `alloc_inode` | that trade plus its allocator safety **is** #687 (patch grew 32 KB → 140 KB last time) |
| Leave the corrupt counter in place, let `alloc_inode` fail closed | Fails Success criterion 3, and converts a startup outage into a permanently write-dead gateway | every new-key PUT errors at `cli.rs:1655` until a human invents the counter the store's own `inode:` keys already prove |
| Derive an inode floor from the `orphan:` ledger | Rebuilds the deleted apparatus; and is arithmetically wrong for epoch-minted ids | ≈ +40 lines; would seed the counter to ≈ 9.2 × 10^18 (see §1) |
| Guard `alloc_inode`'s `id + 1` here | Out of scope twice over; #687 owns allocator safety | — (deferred in-tree instead) |

## 4. Forced refutation of my own test

**(a) Genuine red?** **Yes** — measured through the project's own runner, not by hand:
`PDCA_BUNDLE=… ./engine/scripts/run-verify.sh` → *"run-verify.sh: PASS — red without the fix,
green with it."* The RED leg keeps the added test, reverts all three production files, and
reports `test result: FAILED. 0 passed; 4 failed`, with **assertion** failures on base-visible
symbols — `"high_water_marks met a segmented chunk map, which this build cannot yet resolve"`,
`"expected ident at line 1 column 2"`, and `"invalid digit found in string"` (×2) — not compile
errors. That is why the target is `Gateway::recover()` (signature unchanged) rather than
`high_water_marks` (signature changed): a compile-red leg would have been scored as a pass.

**(b) Production path?** **Yes.** The tests construct the real composition
(`RedbMetadataStore` + `FsChunkStore` + `MemCoordination`,
`crates/server/tests/gateway_recover_totality.rs:125-131`) and drive production
`Gateway::{recover, put_object, get_object}` plus `wyrd_core::read::resolve` for the id a PUT
actually committed under. The one wrapper (`PeerWinsTheCas`, `:430-467`) forwards **every**
read and write to the real redb store and injects only the interleaving; the function under
test is the production `cli::seed_next_inode_floor`. No mock store, no re-implementation, no
stand-in for the behaviour under test.

**(c) Fixture includes the fault?** **Yes**, and the fault is load-bearing. Each test writes
the damaged record into the **same** store as a healthy committed object and asserts the mark
that only the damaged record can produce: the healthy object is inode 1, so a walk that
skipped the damaged record would leave `meta:next_inode = 2` and fail `counter > 50` /
`counter > 37`. Criterion 3 corrupts the very key recovery must repair and then requires a real
PUT to commit above the committed id. The core unit test carries all three damaged shapes at
once, with the unparsable key **last**, so a walk that stopped early is still caught
(`crates/core/src/metadata.rs:3504-3530`).

Bounded-time is enforced rather than assumed: recovery runs on a worker thread and a missed
`RECOVER_BUDGET` is an assertion failure (`:171-195`), so a never-committing retry loop fails
the suite instead of hanging it (brief criterion 3).

## 5. Gates run locally (in `$PDCA_WORKTREE`, off `d50f0ca`)

- `./engine/xtask.sh ci` → **`xtask ci: all checks passed`** (exit 0, 168 suites) — fmt,
  clippy `-D warnings`, build, whole workspace test, `cargo deny`, conformance, statics/DST
  guards. Run three times, the last on the exact tree `patch.diff` encodes. This is also the
  commit-hook answer:
  the patch is `cargo fmt`-clean and clippy-clean, including
  `clippy::doc_lazy_continuation`, which caught one reflowed doc paragraph mid-round.
- `./engine/scripts/run-verify.sh` (C4-verify) → **PASS**, red→green as quoted above.
- Targeted: `cargo test -p wyrd-server --test gateway_recover_totality` 4/4;
  `cargo test -p wyrd-core --lib metadata` 20/20; `cargo test -p wyrd-server --test
  s3_http_wire` 19/19 — including the criterion-5 regression
  `recover_seeds_the_allocator_over_a_legacy_store_without_meta_next_inode`
  (`crates/server/tests/s3_http_wire.rs:666`) and
  `restart_recovers_id_allocators_over_orphan_ledger_no_reclaim_loss` (`:770`), which the chunk
  mark's removal could plausibly have broken and does not (it rests on ADR-0019 epoch
  disjointness, as its own doc says at `:764-768`).
- Criterion 4 mechanically: `git grep -n "_max_chunk" -- crates/` in the worktree → **no
  matches**.

No external dependency beyond the base toolchain was needed; `cargo-deny` (the brief's one
listed dependency) is present and the deny legs passed inside `xtask ci`. **No NEEDS-HUMAN
external-dependency item from this round.**

## 6. Proposed commit body (the reasoning the brief requires to travel)

The brief requires the #487 history to be carried into the commit body, not just the bundle.
Suggested body for publish:

> Startup recovery ran before the gateway served anything and refused over content it could
> not read: one undecodable `inode:` value, or one segmented root, made `Gateway::recover`
> return `Err`, costing every healthy object its availability (`docs/principles.md` §5 C-1).
> `high_water_marks` now derives the inode mark from each row's key, attributes a row it
> cannot account for on `wyrd.metadata.recovery.audit`, and walks on — the containment the
> custodian's GC walk already gives this same namespace (`crates/custodian/src/gc.rs:378-382`).
> `seed_next_inode_floor` is total over the counter's bytes the same way.
>
> The chunk-id half is deleted rather than wired. Its consumer was removed deliberately by
> `fdd34f1` (#487, 2026-07-08), which did both halves at once: before it, `mint_chunk_id` was a
> plain counter from 0 and `recover` consumed the floor; after it, ids are
> `(chunk_epoch << 64) | seq` with the epoch's top bit set — every minted id ≥ 2^127, drawn
> from a random per-process epoch, `next_chunk_seq` never seeded — and the same commit rewrote
> the callsite to `let (max_inode, _max_chunk) = …`. The cluster minter never needed it either:
> `chunk_id_minter` yields `(inode_id << 64) | seq` with `inode_id ≥ 1`, so every cluster id is
> ≥ 2^64. Nothing in the tree mints below 2^64, so wiring it would mean inventing a consumer,
> and a number nobody reads is not a safety property.
>
> The standing test `high_water_marks_refuses_a_segmented_root_rather_than_re_mint_its_chunk_ids`
> goes with it for the same reason: its hazard needs a minter allocating below 2^64, and
> neither minter has since #487. Its live half is not lost — a segmented root must now be
> *contained*, contributing its key-derived id, which is what the replacement test and the new
> acceptance target assert.
>
> Fixes #652

## 7. Open residuals a human may want to weigh at sign-off

1. The counter repair is **destructive by design** over bytes nobody can read (§1, finding 1).
   The advisory reviewer flagged exactly this as fitness-to-purpose last round: "is
   warning-and-continuing over damaged inode rows plus CAS replacement of corrupt allocator
   bytes the intended operational tradeoff?" That is a judgment call the brief settled at
   Plan (criterion 3) and the human owns at sign-off.
2. The exhausted-allocator overflow is **deferred to #687** in-tree (§1, finding 2). If the
   human would rather it be closed now, that is a scope decision — it is an `alloc_inode`
   change, which this brief excludes twice.
