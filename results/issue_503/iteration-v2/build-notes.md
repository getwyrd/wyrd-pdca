# Build notes — issue 503 / object-metadata-model (Iteration 2)

## Scope of this iteration

The iteration-1 **implementation stands** (sign-off rationale: "the implementation itself
stands — do not redesign it, add the missing tests"). This iteration re-ships the full
iteration-1 patch **unchanged** and adds the two test-coverage guards the adversary flagged
as load-bearing-but-unguarded, closing the carry-forward:

1. **Repair-preservation** — the `..prior.clone()` metadata-preservation invariant across
   the four repair/backfill commit sites, previously vacuously true because every existing
   test seeds all-`None` metadata (`..Default::default()`).
2. **Overwrite freshness** — a second PUT of new content must stamp a **new**
   ETag/Last-Modified (the shipped iteration-1 test did exactly one PUT + one GET, so a
   stale-ETag regression on the overwrite path passed every gate).

Nothing in the production diff changed from iteration-1; the `iteration-v1/patch.diff`
production hunks are byte-identical here. The delta is five new tests (and one `cargo fmt`
reflow of an `assert_eq!` in the new core test).

Settled context I did **not** revisit (per carry-forward): T4 prior-art cleared; T5
ADR-0047 decisions (SHA-256 opaque ETag, flat record) approved; the cargo-deny CI env
issue and MD5-compat question are §10 Act candidates, not rebuild scope.

## The new tests, and the exact line each guards

The preservation invariant is implemented as a struct-update spread at **four** independent
call sites. Deleting a spread entirely fails to compile (missing fields), so the *silent*
regression the tests must catch is `..prior.clone()` → `..Default::default()` (compiles,
drops the trio to `None`). Each guard is refuted against exactly that mutation below.

| Guard | Test | File:line guarded |
|---|---|---|
| core reconstruction/backfill commit | `commit_chunk_map_preserves_object_metadata_across_a_repair` (`crates/core/tests/mutation_regressions.rs`) | `crates/core/src/metadata.rs:535` (`..prior.clone()`) |
| custodian identity backfill | `backfill_preserves_object_metadata_while_filling_placement` (`crates/custodian/tests/backfill.rs`) | `crates/custodian/src/backfill.rs:130` (`..record.clone()`) |
| custodian rebalance evacuation | `evacuation_preserves_object_metadata` (`crates/custodian/tests/rebalance.rs`) | `crates/custodian/src/rebalance.rs:288` (`..plan.prior.clone()`) |
| custodian reconstruction repair | `reconstruction_preserves_object_metadata_across_a_repair` (`crates/custodian/tests/reconstruction.rs`) | `crates/custodian/src/reconstruction.rs:579` (`..plan.prior.clone()`) |
| overwrite freshness (wire) | `a_second_put_of_new_content_stamps_a_fresh_etag_and_content_type` (`crates/server/tests/s3_object_metadata.rs`) | `crates/core/src/metadata.rs:639-641` (`commit_chunk_map_superseding_leased`, the variant the wire overwrite path actually uses — see below) |

### Coverage choices

- The three custodian preservation tests are added to the **existing** custodian test files
  next to the scenarios they extend, reusing their in-process `MemMeta`/`MemDServer` fleet
  scaffolding (no D-server network, no Docker — headless-safe, deterministic). Each seeds
  the ADR-0047 metadata trio onto the committed record via the same prior-record CAS the
  durability paths use (`stamp_object_meta`), then drives the real repair through
  `reconcile`/`reconcile_step`, then asserts the trio survived on the re-committed record.
  All four sites are guarded individually because each is a *separate* struct literal — a
  core-only test would not catch a regression in `rebalance.rs`'s own spread.
- The overwrite-freshness test is added to the **brief-named** file
  `crates/server/tests/s3_object_metadata.rs` and drives the production wire path (real TCP
  loopback listener → SigV4 → `Gateway::put_object_streaming` → `commit_overwrite` →
  `commit_chunk_map_superseding_leased`). Its ETag oracle is an independent SHA-256 of each
  body, so an echo of the prior value fails.

### A correctness finding while refuting (recorded so Check can see the reasoning)

My first refutation of the freshness test mutated `commit_chunk_map_superseding`
(the **non-leased** variant) to carry `prior.*` forward — and the test still **passed**.
Reason: `write::commit_overwrite` (`crates/core/src/write.rs:319`) calls
`commit_chunk_map_superseding_**leased**`, not the non-leased one. Mutating the *leased*
variant's metadata block (`metadata.rs:639-641`) to `prior.*` makes the test go RED
(GET serves `05cb…` where the overwrite committed `211c…`). So the freshness test genuinely
binds the code path the wire overwrite executes — confirmed, not assumed.

## Refuting my own tests (forced — actually run, reverted, re-run)

Each guard was mutated to the exact silent regression it exists to catch, re-run RED, then
reverted (verified no residue: `grep MUTATION` clean, preservation lines intact at their
cited positions).

- **(a) Genuine red?** YES, for all five:
  - core preservation: `..prior.clone()`→`..Default::default()` at `metadata.rs:535` ⇒
    `FAILED … a repair PRESERVES the ETag`.
  - backfill preservation: `..record.clone()`→`..Default::default()` at `backfill.rs:130` ⇒
    `FAILED … backfill PRESERVES the ETag`.
  - rebalance preservation: `..plan.prior.clone()`→`..Default::default()` at
    `rebalance.rs:288` ⇒ `FAILED … evacuation PRESERVES the ETag`.
  - reconstruction preservation: `..plan.prior.clone()`→`..Default::default()` at
    `reconstruction.rs:579` ⇒ `FAILED … reconstruction PRESERVES the ETag`.
  - overwrite freshness: leased-superseding metadata block → `prior.*` at
    `metadata.rs:639-641` ⇒ `FAILED … GET serves the FRESH ETag … not the stale first one`.
  The brief-named file is additionally RED on the *base* (no metadata model at all: no ETag
  header, hardcoded `application/octet-stream`), as established in iteration-1 and re-checked
  here (patch applies clean on a pristine base worktree).
- **(b) Production path?** YES. The wire tests drive the real `S3Gateway::serve` listener
  over TCP → real SigV4 verify → `Gateway::{put,get}_object_streaming` → real
  `commit_create` / `commit_chunk_map_superseding_leased` on a real `RedbMetadataStore` +
  `FsChunkStore`. The preservation tests drive the real `commit_chunk_map` /
  `wyrd_custodian::{backfill::reconcile, reconcile_step}` production functions over the
  `MetadataStore` seam — no mock/re-implementation of the code under test.
- **(c) Fixture includes the fault?** YES. Each preservation fixture **seeds a record that
  actually carries** `etag`/`content_type`/`modified` (the failing element the old tests
  curated out by seeding all-`None`), and the repair really fires
  (`Reconciled::Changed`, placement genuinely re-pointed / backfilled). The freshness
  fixture actually overwrites with **different bytes and a different content type**, and
  asserts `etag_2 == SHA-256(second) != etag_1`.

## Verification through the project's checks

- Targeted red→green via `cargo test -p wyrd-{core,custodian,server} --test …` (bounded by
  the Bash tool timeout; deterministic in-process tests, no hang/headless risk) — all five
  new assertions GREEN with the fix, RED under the cited mutation.
- `cargo fmt --check` — clean (one `assert_eq!` reflow applied by `cargo fmt`).
- `cargo clippy -p wyrd-core -p wyrd-custodian -p wyrd-server --tests` — clean
  (`-D warnings` workspace policy).
- Patch verified to `git apply --check` cleanly against a pristine base worktree off `HEAD`.
- The whole-tree `cargo xtask ci` (C4-ci) and the bundle-scoped C4-verify gate re-run the
  real suite at Check.

## Pre-declared sign-off item (expected, not a defect — unchanged from iteration-1)

The patch ships a **new ADR** (`docs/design/adr/0047-object-metadata-model.md`, status
`Accepted` pending sign-off) + the README index row. An ADR change is a project-defined
human-only item (INTEGRATION §4) — the reviewer routes it to §6 NEEDS-HUMAN, and the
maintainer is the accepting authority. 0047 is a *new* record (supersedes nothing), so the
ADR-immutability gate is not implicated.
