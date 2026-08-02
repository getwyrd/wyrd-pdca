# Build notes — issue #508 (multipart-upload), iteration 5

Withheld from the reviewer; written for the human at sign-off.

## What this iteration is

Iteration 4's patch was rejected on the adversary's **four cited defects** plus the zero-DST
gap, and on T4's 58 blocking rubric rows. This pass **keeps** iteration 4's protocol
implementation (it is the shape 0016 settles, and its 16 wire tests were green and remain green)
and fixes every named defect, then re-verifies red→green. It is not a re-submission: the four
defects are all in the *maintenance/lifecycle* half, and closing them changed
`gc.rs`, `restore.rs`, `scrub.rs`, `reconstruction.rs`, `rebalance.rs`, the drain, three
metadata backends, the DST campaign and both delete/overwrite publication paths.

Read order I actually used: `brief.md` → proposal 0016 (invariants `:100-160`, decision 2
`:765-892`, decision 7 `:2280-2500`, the `scan_page` seam + failure tables `:2620-2690`) →
`AGENTS.md` §"Review rubric & protocol" → the carry-forward's four defects
(`iteration-v4/check-advisory-adversary.md`) → `review-batch.md`.

## The four carry-forward defects, and what closed each

### 1. Segmented objects were invisible to the maintenance plane (the C-1 invariant failing)

`gc::referenced_fragments` and `restore::committed_chunks` iterated `record.chunk_map`, which a
segmented root leaves **deliberately empty** — so once the publication's
`retire:records:{parts}` drained, a live segmented object's chunks were in neither
`ReferenceSet::placed` nor `::staged`, a restore pass orphan-marked every one of them, and the
next GC pass past the grace window deleted a published object.

Fixed by centralising the resolution: `gc::resolve_committed_map` (one function; GC, restore and
every consumer that reads a committed map go through it), so `protects()`, `holds_any()`
(drain/desired-state), the restore gate and scrub all inherit it at once. Reconstruction and
rebalance additionally gained a **segmented arm** that CASes the owning `seg:` record under
`require(inode == prior)` — without it a segmented object's lost fragment was unrepairable and a
drain of a server holding its fragments would now stay `Pending` for ever with nothing able to
move them (an absorbing operational state I would otherwise have introduced by fixing only the
reference set).

I also took the adversary's *"add `run_restore` to that test and it fails today"* literally:
`a_published_segmented_object_survives_maintenance_and_its_delete_retires_it` polls the part
records away first (that is what opens the window), then asserts `stranded_marked == 0`.

### 2. `drain_records`'s paged branch never converged

The `Parts` arm derived its key list from the immutable payload, so past `B_OPS` it deleted the
same first 1,000 (already-absent) keys on every pass, for ever — every Complete naming ≥ 501
parts, i.e. the brief's own headline `aws s3 cp` shape.

Fixed by deriving the list from **live** state (`scan_page` over the session's own `part:` range,
filtered to the named set), which shrinks as keys are deleted. While in there I restructured
`drain_bytes` the same way, into indivisible `RetirementUnit`s whose orphan marks and record
deletes ride in **one** batch — this is strictly safer than the old two-phase mark-then-delete
(evidence and unprotection are now simultaneous rather than merely ordered) and it is what makes
"absent means already-evidenced-by-this-obligation" a sound reading rather than a silent skip
(T4 rows 18/56). The per-pass work is now bounded on both axes
(`DRAIN_PAGES_PER_STEP` × `DRAIN_BATCHES_PER_PASS`), which closes T4 51/58 without capping
convergence.

Counter accounting fixed too: `records_deleted` counts records actually deleted, so
`run_drain`'s no-progress break can trip.

### 3. `scan_page` was a default-only shim

Native cursored implementations now exist on **all four**: `metadata-redb` (`table.range`),
`metadata-fdb` (`RangeOption` with `limit`, retry-on-`is_retryable`), `metadata-tikv`
(`txn.scan` inside `under_deadline`), and both DST sim stores (`BTreeMap::range`). All four use
the same exclusive-cursor arithmetic (`after` + trailing `0x00`) and the same one-key look-ahead
so `next` is `Some` **only** when a further key genuinely exists.

`metadata-conformance` gained the two clauses the brief's Scope names —
`contract_scan_page_orders_and_pages` (rules 1–3 + the paged walk) and
`contract_scan_page_is_no_skip_under_mutation` (rule 4) — wired into `run_all`, so **every**
backend driver picks them up with no per-driver list to drift.

The conformance clauses cannot discriminate a native impl from the default (the default is
*semantically* conforming; it only inherits the cap), so the discriminating assertion lives where
the cap is reachable: `crates/metadata-redb/tests/scan.rs`
`scan_page_walks_a_population_past_the_cap_that_scan_refuses_whole`, which first asserts the
whole-range `scan` **fails** on that population so the test cannot pass vacuously.

**fdb/tikv are compile-verified here** (`cargo check -p wyrd-metadata-fdb --features fdb` and
`-p wyrd-metadata-tikv --features tikv` both pass in this worktree — `libfdb_c` and the vendored
`tikv-client` are present). What is *not* verified is their runtime behaviour: that needs
`cargo xtask fdb-conformance` / `tikv-conformance`, i.e. a container runtime and a live cluster.
The new conformance clauses run automatically in those jobs.

### 4. Legs C(iii)(b)/(c) — scrub and reconstruction over staged fragments

`ReferenceSet` gained `staged_committed` — the **committed-`part:`** subset, with schemes, which
is exactly the subset 0016 decision 2 makes scrub/reconstruction-visible (an in-flight `sidx:`
chunk carries no committed EC scheme, so it is protected but not yet verifiable). Scrub walks
`placed ∪ staged_committed`; `reconstruction::find_chunk` gained a `Staged` arm that CASes the
`part:` record under `require(mpu == Open@E)` + `require(part == prior)`.

While implementing the repoint I added the **destination pre-mark** rule (0016 decision 2,
finding 1) to *all three* arms, including the pre-existing flat one: the rebuilt fragment is
orphan-marked before its bytes are written, the mark is dropped in the batch that installs the
reference, and a lost CAS leaves it standing. The base's comment said "the rebuilt fragments are
collectable garbage" — they were not: unreferenced **and** unevidenced is exactly what
`gc.rs:158-190` retains for ever.

### 5. The DST gap (the "For the human" item, addressed rather than deferred)

Three seeded properties appended to the **existing** `crates/dst/tests/custodian.rs` (never a new
file — a new `#![cfg(madsim)]` file beside the added server tests would compile the whole C4
invocation under `--cfg madsim`, which swaps `chunkstore-fs` behaviour):

- `segmented_publication_survives_a_crash_before_the_flip` — F5/X37 (same-epoch recovery is
  byte-identical, never a double-write) **and** X40 (a stale epoch-`E` rollback obligation
  drained *after* an epoch-`E'` publication cannot touch the published segments).
- `restore_fence_over_a_completing_session_retires_its_segments` — X57 (the `Completing` fence
  must install `retire:records:{seg}`, and that range must drain empty).
- `staged_replace_losing_to_a_fence_leaves_its_premark` — X29/X59 (a repoint that loses its
  fenced CAS is a no-op whose destination pre-mark stands).

Each closes with the **classification sweep** (`assert_no_classification_gap`), which is a
restore pass asserting `stranded_marked == 0` — the public observable of invariant (2)'s
*no-gaps* claim. Deliberately **not** a disjointness assertion: this protocol overlaps protection
across every handoff, and X94 says a partition assertion would fail correct executions.

All three are also in `REGRESSION_SEEDS`' sweep. `cargo xtask dst` is green (clippy under
`--cfg madsim` included).

## The T4 rubric rows (58 blocking)

Deduplicated (three passes, heavy overlap) they are ~25 distinct findings. All fixed except the
three recorded in `review-rejected.md`. The non-obvious ones:

- **`retire:bytes:{generation}` on the ordinary delete/overwrite paths** (rows 5/6/19/20/34/35).
  `unlink` and both `commit_chunk_map_superseding*` now route a **segmented** prior through
  `multipart::retire_prior_generation` — one O(1) obligation, no inline fan-out — and keep the
  inline expansion for a flat prior. I deliberately did **not** route flat priors through the
  ledger: 0016 decision 4 says "supersede and unlink stop expanding orphans inline", but a flat
  map is bounded by `MAX_MAP_CHUNKS` so its fan-out fits a batch, `RetireBytes::Generation`'s
  payload could not hold a legacy over-ceiling map anyway (the 100 KB value ceiling), and routing
  it would defer every existing overwrite's reclamation by a drain pass — changing observable
  behaviour for ~19 existing tests with no invariant to earn it. The segmented leak is the
  failure mode; that is what is closed. **Flagged for the human:** the legacy over-ceiling flat
  map (an object written by the base at 1 MiB chunks, e.g. 1 GiB ⇒ ~9,216 orphan puts in one
  unlink batch) is a *pre-existing* bound this slice neither introduces nor fixes; #625's reaper
  is where a page-wise treatment of it belongs.
- **`MAX_CHUNKREF_BYTES = 302` (row 4).** The number is only worst-case up to ~9 placements,
  while `ReedSolomon { k: u8, m: u8 }` admits 510 — which would make every derived ceiling wrong
  by ~35× and the published inode value unpublishable. Rather than re-derive every ceiling per
  scheme, the assumption became a **checked configuration precondition**
  (`check_durability_config`, called from `with_durability`, which is now fallible) plus a
  compile-time assert tying `MAX_FRAGMENTS_PER_CHUNK` to the byte budget. Same pattern as X107's
  lengthless-stream precondition: resolved at load, never discovered mid-publication. Two
  existing test call sites updated.
- **Signed payload digests (rows 13/14/27/28/39/40).** `MultipartGateway::upload_part` gained
  `expected: ContentHash` (the same seam `put_object_streaming` already takes) and the S3 layer's
  payload match is now **exhaustive**, so a new mode cannot silently skip the check.
  `CompleteMultipartUpload` verifies the buffered body against the signed
  `x-amz-content-sha256`, mirroring `delete_objects` verbatim (`:2528-2536`) — the same
  destructive-fan-out precedent PR #612's review established, and Complete *is* destructive: the
  document decides which staged parts are discarded.
- **`Content-Type` dropped at Create (12/26/38)** — read off the request head before the body
  stream, exactly as the plain PUT does.
- **`400 EntityTooLarge` vs `500` (row 42)** — `classify()` in gateway-s3 gained a
  `MultipartFault` arm; an ordinary oversized `PutObject` was reporting the gateway as broken.
- **Compensation (rows 10/11/23/24/25/30/31/32/33)** — `stream_staged_data` now returns its
  partial staging on **every** path (`StagingOutcome::{Complete,OverLimit,Failed}`), and every
  post-staging exit in `upload_part` goes through one `abandon()` helper preconditioned on
  `staged.slot_bytes` (the *advanced* bytes — the old code used the pre-stream bytes, so the
  compensation CAS lost the moment any chunk landed). `publish_fenced` now advances a shared
  `FencedSession`, so the caller's fence release CASes against what the store holds.
- **`max_chunks` (rows 7/29)** — the check was in a block that was dead in-loop; it is now
  checked *before* each chunk is staged, so an oversized part creates at most `MAX_PART_CHUNKS`
  entries and is then refused.
- **Silent skips → fail closed (16/17/18/47/54/55/56/57)** — `require_scoped_number`,
  `parse_sidx_suffix`, `strict_positions` (no fabricated identity placement in a maintenance
  path), and `unhex` (a malformed digest can no longer compose an ETag).
- **`resolve_chunk_map` structural validation (50/53)** — `validate_segmented_root`:
  flat/segmented exclusivity, table length vs. `segment_count`, contiguous indices, spans that
  tile `[0, size)`, and each record's own span; a duplicate or out-of-range `seg:` key is an
  error, not a skip.
- **Spurious 404 (row 22)** — `resolved_committed_object` does ONE resolve and applies 0016 §h's
  resolve-**retry** rule; `Ok(None)` from the resolver was answering `NoSuchKey` for an object
  that had merely been overwritten mid-read.
- **Buffered read path (row 9)** — `read::read_object` resolves through the resolver, so a
  segmented object no longer fails with a size mismatch on the CLI/demo path.
- **`SINGLE_PUT_MAX_BYTES` (row 43)** — checked first in `chunk_size_for_put`; a generously
  configured deployment was accepting a 6 GiB single PUT.

## One brief requirement iteration 4 had simply missed

The brief's *Citations expected* → **decode→encode identity** asks for **both** `PendingEntry`
round-trip tests ("legacy `pending:` with both fields absent; owned `sidx:` with both present"),
and the *Serialization identity* rubric class says the same ("add the round-trip test").
Iteration 4 shipped neither — only a constructor migration. Both now exist as unit tests beside
the type (`crates/core/src/metadata.rs`): `a_legacy_pending_entry_round_trips_byte_identically`
asserts the re-encoded bytes are **literally** `{"lease_expiry_millis":100}` (so an
`"owner":null` regression reds it), and `an_owned_staging_entry_round_trips_with_both_fields_present`
pins the owned shape. They sit in the lib's test module rather than a new
`crates/core/tests/*.rs` file deliberately: a third *added* test file would join the C4-verify
invocation and, referencing `PendingEntry::owned` (a symbol the patch adds), would turn the RED
leg into a build error — the exact hazard the brief tells Do to defend against.

## Final counts

`cargo test --workspace`: **163 test targets, 0 failures**. The two added files hold 141
assertions across 19 tests (11 + 8). `cargo xtask dst`: 13 campaign properties + the
regression-seed sweep, green.

## Numbers the brief asks me to record

**RED leg (both added test files, unmodified, against `origin/main` @ `22d71b4`).** I ran this
in a throwaway `git worktree` at the base with only the two test files copied in:

- both files **compile** against the base (`cargo check --test …` exits 0) — the gate hazard the
  brief names did not fire;
- `s3_multipart_upload`: **11 tests ran, 11 failed**;
- `s3_multipart_lifecycle`: **8 tests ran, 8 failed**;
- total **19 ran / 19 failed**, every one an assertion panic (`panicked at
  crates/server/tests/…`), not a build error. So the aggregate red is assertions.

The base-visibility rule held: the one violation I introduced while writing the new lifecycle
test (`wyrd_core::multipart::drain`, a symbol the patch adds) was removed — the delete path now
drains off the request path itself and the test polls for the terminal condition, which is both
base-compilable *and* better production behaviour.

**Leg D arithmetic** (unchanged from iteration 4, re-checked): 64 KiB chunks × six 5 MiB parts
⇒ ⌈5 MiB / 64 KiB⌉ = 80 chunks per part × 6 = **480 chunks**, crossing
`MAX_MAP_CHUNKS = ⌊(100 KB / 2) / 302 B⌋ = 165` while each part's 80 chunks stays under
`MAX_PART_CHUNKS` (165) and every non-final part is exactly at the 5 MiB minimum.

## Refutation — the three forced questions

**(a) Genuine red? Yes — six separate revert-and-rerun runs, each recorded:**

| Fix reverted | Test that went red | Failure |
|---|---|---|
| `drain_records` → the exact iteration-4 payload-derived branch | `core::multipart::drain_tests::a_records_obligation_larger_than_one_batch_drains_to_completion` | `left: 200, right: 0` — 200 of 700 `part:` records never deleted, obligation retained |
| `gc::referenced_fragments` + `restore::committed_chunks` → read `chunk_map` directly | `a_published_segmented_object_survives_maintenance_and_its_delete_retires_it` | the `stranded_marked == 0` assertion — a restore pass marks a live segmented object's fragments |
| `scrub` → `placed`-only | `scrub_verifies_staged_fragments_and_enqueues_a_repair_for_a_corrupt_one` | no repair obligation for a corrupt staged fragment |
| `reconstruction::find_chunk` → committed-only (staged arm deleted) | `reconstruction_repoints_a_lost_staged_fragment_under_the_session_fence` | the part record's placement is unchanged |
| `metadata-redb::scan_page` deleted (trait default) | `scan_page_walks_a_population_past_the_cap_that_scan_refuses_whole` | the first page errors — the default inherits the cap |
| `unlink`'s `retire_prior_generation` call | `a_published_segmented_object_survives_maintenance_and_its_delete_retires_it` | the delete half: no orphan evidence, `seg:` records survive |

One negative result worth recording honestly: my *first* refutation attempt for the drain kept
the new `commit_units` structure and only made the key list payload-derived — and the test
**still passed**, because the new structure converges either way (it walks all units across up to
8 batches in one pass). So the binding refutation is the one above, against the shipped
iteration-4 code shape. I would have reported a false "genuine red" if I had stopped at the
first attempt.

Plus the whole-file red leg: 19/19 on the base.

**(b) Production path? Yes.** Every test drives the real gateway over HTTP (`aws-sdk-s3` against
an in-process `S3Gateway` on a real TCP listener) or the real custodian loops through the real
`reconcile_step` / `reconcile_after_restore` fenced control point, over the same trait stores the
gateway wrote through. Nothing is mocked: the "fleet" is a `PlacementChunkStore` whose
`dserver` **is** the server index, so the server the reference set names is the server actually
holding the bytes. The core drain unit tests drive `wyrd_core::multipart::drain` over a real
`RedbMetadataStore::in_memory()` — the production redb backend, including its native
`scan_page`. The three DST properties drive the real batch builders and the real maintenance
passes; none re-implements a transition.

**(c) Fixture includes the fault? Yes, and in the cases that matter it is what makes the test
discriminating:**

- the segmented-maintenance test **polls the `part:` records away first** — that is precisely the
  window in which a segmented object is in no class; a fixture that ran restore before the drain
  would have passed on the broken code;
- the scrub test **corrupts a fragment in place** on the server the placement names and asserts a
  clean pass enqueues nothing first, so "scrub enqueues indiscriminately" cannot produce the pass;
- the reconstruction test **deletes** a staged fragment *and* models its server as gone (excluded
  from the topology it re-places against) — with the server still present the selector rebuilds
  in place and the placement legitimately does not move, which is why my first version of this
  test failed for the wrong reason;
- the X40 DST property drains the stale epoch-`E` obligation **after** the epoch-`E'`
  publication, i.e. in the order that breaks a session-scoped key space;
- the delete half asserts the *terminal* state (orphan evidence present, `seg:` gone, fragments
  reclaimed past the grace window) rather than an intermediate one, so it cannot pass by racing
  the drain. The O(1)-batch claim itself is pinned deterministically in a core unit test where
  the batch is visible (`deleting_a_segmented_generation_installs_one_obligation_and_no_inline_orphans`),
  because the wire test would race the drain for it.

## Gates run in this worktree

- `cargo xtask ci` (fmt + clippy + build + test + deny + conformance): **all checks passed**.
- `cargo xtask dst` (clippy and tests under `--cfg madsim`): **green**, including the three new
  properties and the regression-seed sweep.
- `cargo check -p wyrd-metadata-fdb --features fdb` / `-p wyrd-metadata-tikv --features tikv`:
  both clean (the gated arms are compile-verified, not runtime-verified — see §3).
- `cargo fmt --all` run over every touched file, so the target's commit hook has nothing to
  reject.

## What I did NOT do, and why

- **No ADR, no proposal edit, no spec.** Open question 2 and the segment-group-keying precedence
  ruling both stay NEEDS-HUMAN: 0016 decision 7 (`:2314-2320`) and its implementation summary
  (`:2678`) still name the *upload id* as the segment-group nonce, which §1 (`:499-526`)
  supersedes. The code implements §1's independent nonce; the proposal needs the editorial fix.
- **No split.** Open question 1 says Do reports rather than splits. The patch is ~10.6 K added
  lines across 41 files. The seams the brief names are still the natural ones, and after this
  pass two of them are visibly separable: **(i)** the `scan_page` seam + four backends +
  conformance (~330 lines, self-contained, zero coupling to the rest), and **(iii)** the
  custodian-side reference-set / scrub / reconstruction / rebalance changes (~700 lines, coupled
  to the record model but not to the wire surface). If the maintainer wants a smaller review
  unit, those two lift out cleanly; the verbs and decision 7 do not separate from each other.
- **No opt-in switch** (open question 4 settled as (a)).
- **The 8 GB `aws s3 cp` leg** stays the pre-declared off-Check item for §9.

## NEEDS-HUMAN at sign-off

1. **The 8+ GB `aws s3 cp` round-trip** (pre-declared in the brief): needs a deployed stack,
   the `aws` CLI and ≥8 GB free disk. Everything it exercises is built and driven at Check with
   smaller bodies.
2. **The 0016 editorial correction** (segment-group nonce) — architecture-board authority.
3. **fdb/tikv `scan_page` runtime conformance** — compile-verified here; the behavioural proof is
   `cargo xtask fdb-conformance` / `tikv-conformance`, which need a container runtime and a live
   cluster. The new conformance clauses are already wired into both jobs, so the human need only
   run them.
4. **`with_durability` is now fallible** (a public API change on the composition root). Two
   in-tree call sites updated; if any out-of-tree caller exists, it needs the same `?`/`expect`.
