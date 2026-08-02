# Build notes — issue 635 / segmented-chunk-map (iteration 8)

*Withheld from the reviewer; written for the human at sign-off.*

## 0. Where this round starts, and why

Iteration 7 was **not** rejected on its design or on its gates: C4-ci, C4-verify, C1, C3, T1,
T2 and C5 (0 missed mutants) were all green. **One gate failed — T4, the batched rubric
review — with 6 blocking findings, 0 recorded-rejected**, and the sign-off rationale added
two items from the adversarial review. So this round is **iteration 7's patch plus the fixes
for those eight items**, not a rebuild: re-deriving a 10 000-line implementation that seven
gates had already accepted, under a fresh set of typos, is how a slice this size loses
another two rounds.

**Base check, as the brief demands.** `$PDCA_BASE` and `$PDCA_VERIFY_BASE` are **unset**;
there is no `stack-base` file in the bundle (only the dead `stack-634-fold.diff` from
iteration 5); `$PDCA_WORKTREE=/home/eddie/development/wyrd/wyrd.pdca-wt-l0` was clean at
`9120f7a` (`origin/main`, carrying #634 via PR #645). `run-verify.sh --print-base` ⇒
`origin/main`. Build base == test base.

## 1. The eight items and what each one cost

Every fix below is in production code, with a test that goes red without it (§3(a) has the
measured reverts). The finding ids are `review-batch.md`'s.

### 1.1 `metadata.rs:2260` — the flat map's currency asymmetry (the one I thought hardest about)

`resolve_live_chunk_map` promised "the live root" and delivered it only for the **segmented**
shape: the segmented resolve re-reads the root anyway (decision 7(h) makes it), while a flat
map *is* the caller's record, so a stale snapshot came back looking current. Every custodian
pass resolves from a `scan("inode:")` snapshot.

I considered three fixes and rejected two **on measured cost, not on adjectives**:

* **re-read the root inside `resolve_live_chunk_map` for both shapes.** Rejected: that entry
  is also the two read paths' (`crates/core/src/read.rs:514`,
  `crates/server/src/lib.rs:358`,`:448`), which call it **immediately after reading the root
  themselves** — one `get` per object read, on the hot GET path, buying nothing (the record
  is microseconds old and the next instant can supersede it either way).
* **fix it inside the custodian only** (`crates/custodian/src/resolve.rs`). Rejected: the
  false promise is at the core API, so the next consumer (#636, #508) inherits the same trap;
  and the finding is filed against `metadata.rs`.
* **chosen: two entries, and the difference is in the name.**
  `resolve_current_chunk_map` / `resolve_current_chunk_homes` (`crates/core/src/metadata.rs:2315`,
  `:2451`) read the root themselves; `resolve_live_chunk_map` / `_homes` keep the
  snapshot contract for the read paths and now document exactly what they do and do not
  promise (`:2361-2366`). The custodian routes through the current entries
  (`crates/custodian/src/resolve.rs:76`,`:96`).

**Cost, since I chose the more expensive resolve for the maintenance plane:** one `get` per
committed inode per pass — a pass that already `scan`s every `inode:` row *and* calls
`list_fragments()` on every D server in the fleet over the network (`gc.rs:159`,
`restore.rs:206`). Read paths pay **zero**.

The custodian test is the one that matters and it is flat→flat on purpose: a *segmented*
stale snapshot is caught by the pre-fix code too (its resolve re-reads the root anyway), so
a segmented fixture would have been green before and after — the fixture has to be the shape
that has no second read in it.

### 1.2 `metadata.rs:2932` — the flip must **move** its fence, not merely pin it

The rollback that reclaims a crashed attempt is fenced on the same record. A flip that left
`Completing@E` in place published the root and left the rollback's own precondition **true**,
so it commits *after* the flip and deletes the segments the live root now names — the
live-root-naming-deleted-segments hazard the fence exists to close, one step later.
`check_fence_transitioned` (`:3438`) refuses a flip whose contribution neither deletes the
fence key nor puts a value differing from every value it pins on that key, with
`ChunkMapError::FenceNotTransitioned` (`:547`). Zero I/O, so it joins the deterministic
refusals *before* the first `seg:` write (leg B(iv)).

This is the protocol, not an invention: the publication instant is normatively the batch that
carries `session → Completed` (`0016:2338-2345`). Fixture cost: every flip contribution in
the suite and in the DST gained its transition put (~8 call sites).

### 1.3 `metadata.rs:3031` (×2) + the adversary's shorter-plan refutation — one rule

`verify_resume_prefix` became **`verify_durable_range`** (`:3161`) and runs on *every*
publication, not only a resumed one. Three refusals from the one bounded range read the
resolver already does:

1. a key in the group's range that does not parse — `read_segments` refuses it with a strict
   `?`, so a row the publisher ignored makes **every later resolve fail, permanently**;
2. a durable index **past the end of this plan** (`PublicationTailStranded`, `:524`) — the
   adversary's reproduction: attempt 1 writes 2 segments, attempt 2 publishes a 1-segment
   plan at the same `(nonce, epoch)` with `resume_from = 0`, gets `Committed`, and every read
   from then on fails `SegmentUnknown`. The `resume_from == 0` short-circuit was exactly why
   the round-6 fix missed it;
3. the claimed prefix, byte-for-byte, as before.

`flip()` verifies too (`:3244`), because a completer that drives the phases separately (the
recovery shape) would otherwise flip over a range this committer never looked at.

**I chose refusal over deleting the stranded tail in the flip batch** (the adversary offered
either). Deleting is atomic with the flip and looks tidier, but a concurrent reader holding
the *old* root would then find segment 1 absent, re-read the root, see the **same group and
epoch** still named, and take the `SegmentAbsent` fail-closed arm — a torn read manufactured
by the publisher. A same-epoch plan change is a protocol violation in the first place
(`0016:2352-2356`: same-epoch recovery is idempotent *because* the content is fixed), so
failing closed is the honest answer and it is 12 lines against ~40 for the delete path plus
its own interleaving test.

### 1.4 `metadata.rs:3452` — the id floor may not under-report

`"id":"57"` read as zero was the finding's example; "truncated" was its generalisation. The
fix folds **two readers and takes the larger** (`raw_chunk_id_floor`, `:3691`):

* `serde_json` itself — most values that reach here are *structurally* invalid rather than
  syntactically broken (this slice enforces the segmented root's invariants at decode), so a
  real parse reads every `id` field exactly, at any depth, in either spelling, with no
  grammar of ours to get wrong (the rubric's *prefer extending a shared parser*);
* the byte scan — for bytes that are not JSON at all, **and** for what a parsed value
  discards: duplicate object keys collapse to the last one in a `serde_json::Value`, so
  `{"id":77,"id":3}` parses to 3 and scans to 77. Each reader is tested on input the other
  cannot read (`each_id_reader_recovers_what_the_other_cannot`).

A **truncated** token (digits running to the end of the value) now folds
`widest_id_with_prefix` (`:3818`) — the largest in-process id those digits admit — because
`"id":12` cut short may have been any of `12`…`1.3e19` and only the largest is safe. Worst
case that burns ~55 % of the id space and leaves ~8.4×10¹⁸ ids; under-counting loses data.
What neither reader can read is counted (`RecoveredIds::unreadable`) and attributed on the
audit event plus `metadata_chunk_id_unrecoverable` — the honest edge of the containment rule,
said out loud rather than left as a silent skip.

### 1.5 `dst/tests/custodian.rs:1811` — the mid-phase apply-then-unknown interleaving

A sixth interleaving on the same seeded fixture (`crates/dst/tests/custodian.rs:1912-2064`),
at a fresh generation so it does not disturb the five before it: a segment batch **lands** and
is reported `CommitUnknownResult`, and the store — not the process's memory — is the
recovery's authority. It asserts the ambiguous batch's records **and** its cursor are durable,
that a re-run from zero writes nothing (compared on the bytes, not on an outcome), that
resuming from the recovered cursor completes and leaves the range **exactly** the plan, and
that the recovered publication publishes and resolves to the whole list. 50 seeds.

### 1.6 The adversary's totality refutation — `high_water_marks` is now total *as claimed*

Three of its four walks were unpaged `scan`s, which are complete-or-fail-loud at `SCAN_CAP`
(2²⁰). A store with more than 1 048 576 inodes — or an orphan ledger past the same cap, which
one maximum segmented retirement approaches on its own (~1.78 M marks) — made
`Gateway::recover` return `Err` and the whole gateway refuse to start: the exact blast radius
the containment table forbids, arriving through size rather than corruption. All four walks
now go through `for_each_page` (`:3623`). The test builds a store with
`with_scan_cap(1)` and asserts a `scan` of each of the four prefixes fails **while the floor
is exact** — the fault is in the fixture, not asserted around it.

### 1.7 What I did *not* change, and why

* **GC/restore abort on an unresolvable object** rather than continuing with it treated as
  fully referenced (the adversary's second judgment call). The brief's containment table
  pre-authorises aborting, and "continue, protecting it" is not implementable *precisely*
  here: the chunk ids of the **missing** segment are exactly what cannot be read, so a
  continuing pass could not protect them by id, and restore would mark them stranded. Abort
  is the only shape that guarantees "no fragment deleted", which is what leg A(vii)(d)
  asserts.
* **The ranged read still resolves the whole map** (the adversary's first judgment call). The
  root's `byte_offset`/`byte_len` index would let a 1-byte range read one segment; wiring it
  in is a range-scoped resolver, which is #508's slice. Raised again in §4 as a NEEDS-HUMAN
  scope call, not silently dropped.
* **The round-1/5/6 recorded rejections** stand, and this round re-pins them at their current
  lines in `review-rejected.md` so a re-review that reports them at a shifted line still meets
  a recorded decision.

## 2. What is inherited from iteration 7 (unchanged)

The record shape (`Flat | Segmented`, JSON-type discriminated), the `seg:`/`seggrp:` records
and key helpers, the staged-publication committer with the fence and the caller's flip
contribution as parameters, the one shared resolver and all nine `.chunk_map` consumers routed
through it, the containment table's per-consumer behaviour, leg A's consumer test (unchanged
this round — not one byte), the two co-located gateway legs in `crates/server/src/lib.rs`, the
X51 DST interleavings, and the architecture-doc edits (extended, §1.2/1.3/1.6).

**The brief's two "state which you chose" questions**, restated because the human reads this
file rather than the previous one:

* *the flip stays in this slice*, with the caller contributing preconditions **and** mutations
  to the single flip batch — and now required to transition the fence in it;
* *backfill resolves, then skips* a segmented map with a stated reason and fails closed when
  something was left unfilled, rather than rewriting the map.

## 3. Refuting my own test (forced, recorded)

**(a) Genuine red?** Yes — each fix reverted **in place on the exact tree being shipped**, the
suite re-run, the fix restored, and the file then diffed back to byte-identical:

| Fix reverted | Result |
|---|---|
| the stranded-tail refusal in `verify_durable_range` | `a_publication_refuses_to_publish_over_a_range_no_resolve_could_read` **FAILED**: *"a shorter republication must refuse the tail it cannot name: Committed"* — the publication succeeds, and the test's own next step shows the published root then resolving to `SegmentUnknown` for ever |
| `check_fence_transitioned` → always `Ok` (the pre-fix rule: pinning is enough) | `the_flip_must_move_its_fence_record_not_merely_pin_it` **FAILED** (`unwrap_err` on an `Ok` flip batch) **and** `a_deterministically_refused_publication_writes_no_segment_at_all` **FAILED**: *"a flip that pins its fence without moving it must be refused: Conflict"* |
| the maintenance plane's current-root routing → iteration 7's snapshot resolve | `a_pass_resolves_the_live_root_and_reports_that_its_snapshot_was_stale` **FAILED** `left: Satisfied, right: Changed` — the pass answered from its stale snapshot and left the live generation's empty placement unfilled |
| the quoted/truncated shapes **and** the JSON reader (the pre-fix scanner) | `the_id_recovery_reads_every_shape_the_bytes_still_spell` **FAILED** `left: 0, right: 57`; `each_id_reader_recovers_what_the_other_cannot` **FAILED** `left: 0, right: 57` |
| `for_each_page` → `scan` for the `inode:` walk | `the_id_floor_is_total_over_a_store_too_large_to_scan` **FAILED**: `Err(ScanCapExceeded { cap: 1, prefix: "inode:" })` — i.e. `Gateway::recover` refusing to start |
| an ambiguous segment commit collapsed into `Ok(Conflict)` | DST `staged_publication_is_atomic_at_the_flip` **FAILED** at `:1947`: *"the second batch landed and was reported unknown: Conflict"* (the earlier legs stay green, so the new interleaving is what caught it) |

And the binding leg-A red is the whole-slice one, measured through the project's own runner
(`./engine/scripts/run-verify.sh`): **9 tests ran on the RED leg, 9 failed, as assertions** —
transcript in §6.

**(b) Production path?** Yes. Every assertion runs production functions over a real backend or
the production custodian pass: `metadata::high_water_marks` / `segment_chunk_floor` /
`raw_chunk_id_floor`, `SegmentedPublication::{publish, write_segments, flip, flip_batch}`,
`resolve_current_chunk_map` / `_homes`, and `backfill::reconcile` — over
`RedbMetadataStore::in_memory()` (including one configured `with_scan_cap(1)`) or over the
in-test `MetadataStore` doubles the custodian suites already use. Leg A drives the real
`reconcile_step`, `reconcile_after_restore`, `reconciliation_status` and `high_water_marks`.
The only stand-in anywhere is the *caller* that supplies the publication precondition (there
is no session record until #636 — brief `Production reach`).

**(c) Fixture includes the fault?** Yes, and this round's fixtures are where the findings
lived:

* the stranded-tail fixture writes attempt 1's **whole** range first and cuts attempt 2's plan
  at a segment boundary of that same list, so the shared records are byte-identical and the
  **tail alone** is what refuses — a fixture with a different prefix would have tripped
  `SegmentBoundsMismatch` first and proved nothing about the tail;
* the currency fixture is **flat → flat** over a `scan`-is-a-snapshot double, i.e. exactly the
  shape the pre-fix code cannot notice; a segmented snapshot would have passed before the fix;
* the totality fixture is a store whose `scan` genuinely fails (cap 1, asserted on all four
  prefixes) rather than one that merely has many rows;
* the id-recovery fixtures drive each reader on input the **other** cannot read (escaped field
  names for the parse, duplicate keys and non-JSON bytes for the scan), so neither can pass by
  the other's work;
* the DST leg's ambiguous batch is asserted to have **landed** (`durable_now == 2`, cursor at
  2) before recovery runs — an ambiguity whose write did not land would prove nothing.

## 4. NEEDS-HUMAN at sign-off

1. **NEEDS-HUMAN (pre-declared, brief `Open questions` 4 / carry-forward T3):** this slice
   lands a `Completing`-less **precursor** committer — a persistence API no production path
   reaches until #636 supplies the session fence. The maintainer must accept landing it now
   rather than folding staged publication into #636. Nothing in the code can settle this.
2. **The pinned JSON encoding** (brief `Open questions` 1) is the brief's decision, not
   0016's, and leg A's fixture is hand-written from it.
3. **Scope call (from the adversarial review, unchanged this round):** the gateway's ranged
   read resolves the **whole** map and then selects covering chunks
   (`crates/server/src/lib.rs:448`,`:479`), so a 1-byte `Range:` GET on a maximal object reads
   the whole `seg:` range even though the root's byte index could name the one segment that
   covers it. Correct, but the affordance is unused; a range-scoped resolver is plausibly
   #508's. Confirm or redirect.

No `NEEDS-HUMAN external dependency` items: `typos` and `docs-renderer` were both present and
both ran inside `cargo xtask ci` (the log shows `$ typos` and
`$ python3 docs/publishing/tools/render_site.py --check`), and `cargo-mutants` is installed.

## 4b. Where the three artifacts are

* `patch.diff` — the whole change, generated from `$PDCA_WORKTREE` after the refutation runs
  had been reverted and the files diffed back to identical, so the bundle carries exactly the
  tree the gates ran on. 47 files, +11 238 / −438; **no `Cargo.toml` is touched** (the brief's
  `Falsifiability` 1: a modified manifest is reverted on the RED leg and would turn leg A's
  assertion-red into a build error).
* The named test, `crates/custodian/tests/segmented_map_consumers.rs`, ships **inside**
  `patch.diff` as an added file — the convention every prior iteration used and the one
  `run-verify.sh` reads (`--classify` returns it as the single `ADDED_TEST`). It is not
  duplicated at the bundle root, where nothing would read it and where it could drift from the
  patch. It is **unchanged from iteration 7**: this round's findings were all in production
  code or in leg B / the DST.
* `build-notes.md` — this file. `review-rejected.md` (bundle root) carries the round-7 triage
  and re-pins the standing decisions at their current lines.

## 5. Gate evidence from this round

* `./engine/xtask.sh ci` (the project's own runner, C4-ci): **exit 0**, `xtask ci: all checks
  passed`, 164 `test result: ok` lines including the DST leg at 50 seeds and both prose gates.
* `cargo fmt --all` run over the tree; `cargo fmt --all --check` clean (the target's commit
  hook runs fmt + clippy, both inside `ci` above). Clippy is `-D warnings` and green,
  including `manual_contains`, which caught one line of mine.
* `./engine/scripts/run-verify.sh`: **PASS**, numbers in §6.
* `scripts/mutants-in-diff`: `393 mutants tested in 9m: 214 caught, 179 unviable` — **0 missed,
  0 timeouts**.

## 6. Runner-measured RED leg and mutation results

### The RED leg (brief `Falsifiability` 3 — the numbers, and *what kind* of red)

`--print-base` ⇒ `origin/main`; `--classify` ⇒ exactly one
`ADDED_TEST crates/custodian/tests/segmented_map_consumers.rs` plus six `CRATE` rows:

```
run-verify.sh: GREEN — cargo test -p wyrd-custodian --test segmented_map_consumers (fix applied)
running 9 tests
test result: ok. 9 passed; 0 failed; …
run-verify.sh: RED — cargo test … (production reverted, test kept)
running 9 tests
test result: FAILED. 0 passed; 9 failed; …
run-verify.sh: PASS — red without the fix, green with it.
```

**9 tests ran on the RED leg and all 9 failed, and the red is ASSERTIONS, not a build error** —
the file compiles against the reverted tree (it names only base-visible symbols) and every
failure is a runtime panic carrying the base's own decode error propagating out of production
code, e.g. `maintenance_resolves_a_segmented_map_and_never_reclaims_its_fragments`:
*"reconcile_step must resolve a segmented chunk map, not fail on it: Some(\"reconciliation
store access: invalid type: map, expected a sequence\")"*, and
`a_damaged_segmented_object_never_costs_the_store_its_other_objects`,
`a_drain_of_a_server_holding_a_segmented_fragment_is_pending_not_satisfied`,
`the_core_read_path_resolves_a_segmented_object_byte_for_byte`, the two backfill legs, the
rebalance, reconstruction and post-grace GC legs — all the same shape. (`TESTS_RAN == 9`, so
the gate's zero-test guard is not what produced the PASS.)

### C5 — mutation coverage of this bundle's diff

`scripts/mutants-in-diff` (the gate's own command) against the final patch:

```
393 mutants tested in 9m: 214 caught, 179 unviable
EXIT=0
```

The first run of this round reported **17 missed**, all in the code this round added, and they
were real signal — worth recording because each one named a test that was passing on the
other reader's work:

* 12 in the two id readers (`json_chunk_id_floor`, `json_chunk_id`,
  `scavenged_chunk_id_floor`, `widest_id_with_prefix`): the two readers agree on almost every
  input, so `max(a, b)` hid each one's mutants behind the other. Bound by
  `each_id_reader_recovers_what_the_other_cannot`, which drives each on input only it can read.
* 1 in `verify_durable_range`'s nonce/epoch check (`||` → `&&`): bound by two legs over the
  `Impostor` double, one stranger row per spelling — the same shape the resolver's identical
  check already had.
* 2 in `chunks_of`/`homes_of`'s "was the snapshot stale?" comparison: folded into **one**
  `note_currency` helper (one place, one meaning) and bound by the telemetry test.
* 1 in `emit_record_unreadable`'s `if unreadable > 0` guard around the counter: the guard is
  gone — the counter is emitted with its value, and a zero increment is a no-op for a sum.
* 1 was `json_chunk_id_floor`'s `Array` arm, which nothing reached because every array-nested
  id in the fixtures was also visible to the byte scan.

## 7. Scratch

Everything transient went under `$PDCA_SCRATCH` (`/var/tmp/pdca/pdca-builder-635-logs`,
`/var/tmp/pdca/pdca-builder-635-refute`): the CI/gate logs and the two file snapshots used for
the refutation runs. `cargo mutants` uses its own `/var/tmp/pdca/cargo-mutants-*.tmp` build
copy and the worktree's `mutants.out*/` are git-ignored, so none of it reaches the patch. All
removed at the end of the run.
