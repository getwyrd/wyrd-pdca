# Build notes — issue 635 / segmented-chunk-map (iteration 7)

*Withheld from the reviewer; written for the human at sign-off.*

## 0. Where this round starts, and why

Iteration 6 was **not** rejected on its design. It passed C1 Spec, C3 Change, T1 Structure,
T2 Shape, `cargo xtask ci` (C4-ci) and the per-fix red→green (C4-verify) on this same base.
Two things failed:

* **C5 (advisory)** — 3 surviving mutants, all three the *same* comparator:
  `crates/core/src/metadata.rs:3473:29 replace < with ==/>/<= in high_water_marks`
  (`iteration-v6/`'s tree). The carry-forward's T5 line says the same thing: the tests did
  not protect the ID-space boundary.
* **T4 batched review (gating)** — 6 blocking findings, 0 recorded-rejected, in two classes:
  the resume-prefix check verifying only its last record (3 findings, 3 independent passes),
  and the id-allocator floor's containment (3 findings).

So this round is **iteration 6's patch plus the fixes for those findings**, not a rebuild:
throwing away a 9 400-line implementation that four gates had already accepted, to re-derive
it under a fresh set of typos, is how a slice this size loses another two rounds. The delta
is below; everything else in `patch.diff` is iteration 6's, unchanged.

**Base check, as the brief demands.** `$PDCA_BASE` and `$PDCA_VERIFY_BASE` are **unset**;
there is no `stack-base` file in the bundle (only the now-dead `stack-634-fold.diff` from
iteration 5); the worktree `$PDCA_WORKTREE=/home/eddie/development/wyrd/wyrd.pdca-wt-l0` is
clean at `9120f7a`, which is `origin/main` carrying #634 (PR #645). Build base == test base.

## 1. The delta this round (all of it in `crates/core/src/metadata.rs`)

### 1.1 The resumed publication authenticates the WHOLE durable prefix (`:3010`)

Findings `:3009` ×3 and the carry-forward's C5 line. Iteration 6 read back exactly one
record — `seg:<nonce>:<epoch>:<resume_from - 1>` — and argued in its own doc comment that
the last record was sufficient because "a segment's `byte_offset` is the running sum of
every chunk length before it". That argument only covers divergences that **move bytes**. A
re-derived chunk list whose chunk *ids* or *placements* differ at the same encoded width
leaves every `byte_offset`/`byte_len` in the map identical — so the last record matches, the
root matches, and the flip publishes a **hybrid** map: attempt 1's chunks for the prefix,
attempt 2's for the tail. It resolves cleanly and serves the wrong bytes for ever. That is
strictly worse than the unresolvable map the check was written for, because nothing
downstream ever notices.

`verify_resume_prefix` now re-reads the whole claimed prefix and refuses at the **first**
index that disagrees (`:3010-3056`), and the two error variants' docs were corrected to match
(`:504`, `:515`).

**Cost, since I chose the more expensive check.** One `get` → one bounded range `scan` of
`seg:<nonce>:<epoch>:` — the *same* range read the resolver already performs
(`read_segments`, `:2076`), bounded by `MAX_ROOT_SEGMENTS` = 512 rows against a `SCAN_CAP`
of 2^20 (`crates/traits/src/lib.rs:286`). So the extra cost is **one round trip's worth of
rows instead of one row, on the resume path only** (a fresh publication, `resume_from == 0`,
still reads nothing at all: `:3011`), plus 512 × ≤100 KB of transient heap in the worst case
— the same worst case the resolver already accepts on every segmented read. The rejected
alternative — N sequential `get`s, one per claimed segment — has the same completeness and
costs up to 512 sequential round trips instead of 1.

### 1.2 The id-allocator floor is total **and** never under-approximates (`:3413`, `:3506`, `:3593`)

Findings `:3461`, `:3397`, `:3405`. These are one design question with two failure modes,
and iteration 6 shipped one of each:

* the `inode:` walk still had the base's `decode(&value)?` (`:3461`). This slice *widens*
  what fails that decode — a segmented root's structural invariants are now enforced at
  decode (parse-don't-validate, `AGENTS.md:146-149`) — so one malformed root would make
  `Gateway::recover` (`crates/server/src/lib.rs:123-124`) refuse to start and take every
  **healthy** object offline. That is precisely the blast radius the brief's containment
  table forbids and iteration 5 was rejected for;
* `segment_chunk_floor` contained an undecodable `seg:` record by **skipping** it (`:3397`,
  `:3405`), which buys totality by giving up the other half of the same rule: the floor may
  then sit *below* an id whose fragments are on disk, and the allocator re-mints it over
  them (issue #364). Iteration 6's own test asserted the under-report as if it were the
  intended behaviour (`the_id_floor_is_total_over_a_damaged_segmented_object`, the tail).

Both are now contained the same way: the record is **attributed** (audit event on
`wyrd.metadata.audit` naming the key, plus `metadata_segment_record_unreadable` /
`metadata_inode_record_unreadable` counters) and its ids are recovered from the raw bytes by
`raw_chunk_id_floor` (`:3413`), which folds every `"id":<decimal>` the value still spells
under the same in-process ceiling. Total (it cannot error) and an over-approximation
(over-counting only starts the allocator higher; under-counting loses data).

**What I did not do, and why.** The obvious cheaper spelling is to drop the typed decode
entirely and derive the floor from raw bytes for *every* record. It is ~6 lines shorter and
removes a branch. I kept the typed path as primary and the lenient scan as the **fallback
only** (`:3506`, `:3601`): the healthy path stays parse-don't-validate, and the hand-rolled
scan — which the target's rubric is right to be suspicious of ("prefer extending a shared
parser", `AGENTS.md:165-169`) — only ever runs on bytes that already failed the real parser,
where by definition no parser can help.

The CONVENTION half of finding `:3405` ("return an error or queue repair") is **declined and
recorded** in `review-rejected.md` (round 6 section): an `Err` here is exactly what the
containment table forbids, and writing a repair obligation from a read-only startup scan
would make gateway startup depend on a metadata write. The object-level obligation is the
custodian's `PendingMalformed` (`crates/custodian/src/desired_state.rs:166-179`).

### 1.3 The allocator boundary is now pinned by tests (`:6804`, `:6892`)

The three surviving mutants were all `chunk.id < IN_PROCESS_CHUNK_CEILING` in the **flat**
branch of `high_water_marks`: every fixture in the suite used *segmented* roots near the
boundary, so nothing bound the flat comparator.
`the_id_floor_is_exclusive_at_the_coordination_free_boundary` (`:6804`) asserts the pair
`{2^64 - 1 counts, 2^64 does not}` at **all five** readers of the bound — the flat map, the
`seg:` walk, the lenient recovery, and the `pending:` and `orphan:` ledgers — which kills
`<` → `==`, `>` and `<=` at each. Verified by hand-mutating `<` to `<=` at the flat branch
(test fails `left: 18446744073709551616, right: 18446744073709551615`) and again at the
orphan branch (same failure). The last two legs came out of *this* round's own mutation run,
which surfaced the surviving `<=` at the `orphan:` comparator (`:3626` on the intermediate
tree) — pre-existing code the diff carries, and one line of fixture to bind.

### 1.4 Tests changed, not added, where the old assertion encoded the bug

* `a_resumed_publication_refuses_a_durable_prefix_that_is_not_its_own` (`:4649`) now drives
  **three** divergences (a length change, two chunk ids transposed, one placement moved)
  over a prefix of **more than one** segment (the fixture asserts `durable >= 2`, `:4683`),
  and expects the refusal at index **0** — inside the prefix, not at its end. Under the old
  last-record check the two same-extent cases return `Committed` (measured, §3).
* `the_id_floor_is_total_over_a_damaged_segmented_object` (`:6627`): its tail asserted that
  an undecodable `seg:` record costs the floor "that record's ids and nothing else". That
  assertion *was* the finding. It now keeps the totality half (bytes that spell no id cost
  nothing) and the recovery half moved to its own test.

### 1.5 Docs currency for the delta (`docs/design/architecture/06-runtime-view.md:32`)

The inherited doc edit already said startup recovery "derives the id-allocator floor from the
segment records themselves … so one damaged object can never stop a gateway from serving all
the healthy ones". Before this round that sentence was **not true** of an *undecodable*
record — the `inode:` walk still propagated. The behaviour now matches the claim, and the
sentence gained the clause that says so (totality plus id recovery from the bytes). Docs
currency is a merge requirement in this repo (`AGENTS.md:154-157`), and a claim the code does
not honour is worse than a missing one.

## 2. What is inherited from iteration 6 (unchanged, and why it stays)

The record shape (`Flat | Segmented`, JSON-type discriminated, `:1007`), the `seg:`/`seggrp:`
records and key helpers, the staged-publication committer with the fence and the caller's
flip contribution as parameters, the one shared resolver (`resolve_chunk_map` `:2201`,
`resolve_live_chunk_map` `:2255`) and all nine `.chunk_map` consumers routed through it, the
containment table's per-consumer behaviour, leg A's consumer test, the two co-located gateway
legs in `crates/server/src/lib.rs`, the X51 DST interleavings appended to
`crates/dst/tests/custodian.rs`, and the architecture-doc currency edits. The brief's
`Carry-forward` items 2 (refuse before writing), 3 (verify the resumed prefix — now
strengthened), and 6 (the earlier rounds' fixes) are all still in place.

**Answers to the brief's two "state which you chose" questions** (unchanged from iteration 6,
restated because the human reads this file rather than the previous one):

* *The flip stays in this slice*, with the caller contributing both preconditions **and**
  mutations to the single flip batch (`SegmentedPublication::flip`, `:2608` field; merge at
  `merge_contribution` `:3288`). The alternative the brief allows — move the flip into #636 —
  was not needed: the API is clean and leg B(iii) asserts atomicity both ways.
* *Backfill resolves, then skips* a segmented map with a stated reason rather than rewriting
  it (`crates/custodian/src/backfill.rs`), asserted by
  `backfill_resolves_a_segmented_map_and_leaves_it_byte_identical`.

## 3. Refuting my own test (forced, recorded)

**(a) Genuine red?** Yes — each fix reverted **in place on the exact tree being shipped**, the
suite re-run, the fix restored (and the file diffed back to identical afterwards):

| Fix reverted | Result |
|---|---|
| whole-prefix → last record only (`&planned[claimed - 1..claimed]`) | `a_resumed_publication_refuses_a_durable_prefix_that_is_not_its_own` **FAILED**: `two chunk ids transposed: a resume onto another list's prefix must refuse: Committed` — the publication *succeeds*. That is the silent-hybrid corruption itself, not a weaker assertion: the old code publishes a root over another list's prefix and every later read resolves it cleanly. |
| drop the `seg:` raw recovery | `the_id_floor_recovers_ids_from_records_that_do_not_decode` **FAILED** `left: 41, right: 57`; `the_id_floor_is_exclusive_at_the_coordination_free_boundary` **FAILED** `left: 0, right: 18446744073709551615` |
| restore `let record: InodeRecord = decode(&value)?` | `the_id_floor_recovers_ids_from_records_that_do_not_decode` **FAILED**: `an unreadable record may not fail the floor the gateway starts from: Error("segment_count 9 disagrees with 1 segments present", line: 1, column: 158)` |
| `chunk.id < CEILING` → `<=` (flat branch) | `the_id_floor_is_exclusive_at_the_coordination_free_boundary` **FAILED** `left: 18446744073709551616, right: 18446744073709551615` |
| `frag.chunk < CEILING` → `<=` (orphan ledger) | same test **FAILED**, same values |

And the binding leg A red is the whole-slice one, measured through the project's own runner
(`./engine/scripts/run-verify.sh`): **9 tests ran on the RED leg, 9 failed, as assertions** —
transcript and failure messages in §6.

**(b) Production path?** Yes. Every assertion above runs the **production** functions:
`metadata::high_water_marks` / `segment_chunk_floor` / `raw_chunk_id_floor` and
`SegmentedPublication::publish` → `verify_resume_prefix`, over a real
`RedbMetadataStore::in_memory()` — not a double of the store and not a copy of the logic. Leg
A drives the real `reconcile_step`, `reconcile_after_restore`, `reconciliation_status` and
`high_water_marks`; the only stand-in anywhere is the *caller* that supplies the publication
precondition (there is no session record until #636 — brief `Production reach`).

**(c) Fixture includes the fault?** Yes, and this is where the round's findings lived:

* the resume fixture leaves a durable prefix of **more than one** segment and puts the
  divergence in segment **0** — i.e. it deliberately includes the record a last-record check
  would skip. A fixture with `durable == 1` (iteration 6's, 1 500 chunks at one segment per
  batch) cannot distinguish the two implementations at all, which is why the bug survived
  three review rounds;
* the floor fixtures include the **undecodable** records themselves (a structurally invalid
  segmented root, an inode value with a bogus `state`, a `seg:` record whose chunks do not
  sum to its span) *in the same store as* the healthy objects, and assert on the healthy
  ones too — the damaged record is not curated out;
* the boundary fixture includes the id **at** the ceiling (`2^64`) rather than only ids below
  it, so the exclusive comparison is what the assertion turns on.

## 4. NEEDS-HUMAN at sign-off

1. **NEEDS-HUMAN (pre-declared, brief `Open questions` 4 / carry-forward T3):** this slice
   lands a `Completing`-less **precursor** committer — a persistence API no production path
   reaches until #636 supplies the session fence. The brief takes that shape deliberately
   (the fence is a parameter); the maintainer must accept landing it now rather than folding
   staged publication into #636. Nothing in the code can settle this.
2. **The pinned JSON encoding** (brief `Open questions` 1) is the brief's decision, not
   0016's, and leg A's fixture is hand-written from it. If the maintainer wants a different
   spelling it must be said before the next Do run.

No `NEEDS-HUMAN external dependency` items: `typos` and `docs-renderer` were both present and
both ran inside `cargo xtask ci` (the log shows `$ typos` and
`$ python3 docs/publishing/tools/render_site.py --check` at the top), and `cargo-mutants` is
installed.

## 4b. Where the three artifacts are

* `patch.diff` — the whole change, generated from `$PDCA_WORKTREE` and byte-compared against
  a regeneration after the refutation runs, so what the bundle carries is exactly the tree the
  gates were run on. 47 files, +9 893 / −438; **no `Cargo.toml` is touched** (the brief's
  `Falsifiability` 1: a modified manifest is reverted on the RED leg and would turn leg A's
  assertion-red into a build error).
* The named test, `crates/custodian/tests/segmented_map_consumers.rs`, ships **inside**
  `patch.diff` as an added file — the bundle convention every prior iteration used, and the
  one `run-verify.sh` reads (`--classify` returns it as the single `ADDED_TEST`). It is not
  duplicated at the bundle root, where nothing would read it.
* `build-notes.md` — this file. `review-rejected.md` (bundle root) carries the round-6 triage.

## 5. Gate evidence from this round

* `./engine/xtask.sh ci` (the project's own runner, C4-ci): **exit 0**, `xtask ci: all checks
  passed`, 164 `test result: ok` lines including the DST leg
  (`staged_publication_is_atomic_at_the_flip`, `segmented_resolve_never_tears_on_retirement`,
  `segmented_repoint_never_races_a_supersede`) and the prose gates.
* `cargo fmt --all` run over the tree; `cargo fmt --all --check` clean (the target's own
  commit hook runs fmt + clippy, both inside `ci` above).
* `./engine/scripts/run-verify.sh` and `scripts/mutants-in-diff`: see §6.

## 6. Runner-measured RED leg and mutation results

Filled in from the actual runs (the brief's Falsifiability item 3 requires the RED-leg
numbers and whether the red was assertions or a build error):

### The RED leg (brief `Falsifiability` 3 — the numbers, and *what kind* of red)

`./engine/scripts/run-verify.sh` with `PDCA_BUNDLE` on this bundle, base resolved by the
brief's own field (`--print-base` ⇒ `origin/main`; `--classify` ⇒ exactly one
`ADDED_TEST crates/custodian/tests/segmented_map_consumers.rs` plus the six `CRATE` rows):

```
run-verify.sh: GREEN — cargo test -p wyrd-custodian --test segmented_map_consumers (fix applied)
running 9 tests
test result: ok. 9 passed; 0 failed; …
run-verify.sh: RED — cargo test … (production reverted, test kept)
running 9 tests
test result: FAILED. 0 passed; 9 failed; …
run-verify.sh: PASS — red without the fix, green with it.
```

**9 tests ran on the RED leg and all 9 failed, and the red is ASSERTIONS, not a build
error** — the file compiled against the reverted tree (it names only base-visible symbols;
its `seg_key` is a *local* helper, `crates/custodian/tests/segmented_map_consumers.rs:274`),
and every failure is a runtime panic carrying the base's own decode error propagating out of
production code, e.g.

* `maintenance_resolves_a_segmented_map_and_never_reclaims_its_fragments`: *"reconcile_step
  must resolve a segmented chunk map, not fail on it: Some(\"reconciliation store access:
  invalid type: map, expected a sequence\")"*;
* `a_damaged_segmented_object_never_costs_the_store_its_other_objects`: *"one damaged object
  must not fail the id floor the gateway starts from: Error(\"invalid type: map, expected a
  sequence\")"*;
* `the_core_read_path_resolves_a_segmented_object_byte_for_byte`,
  `a_drain_of_a_server_holding_a_segmented_fragment_is_pending_not_satisfied`,
  `backfill_*`, `rebalance_*`, `reconstruction_*`, `a_gc_pass_past_the_grace_window_*` —
  all the same shape.

That is the brief's leg A(i)–(vii) failing as assertions on the base, which is what makes the
green meaningful. (`TESTS_RAN == 9`, so the gate's zero-test guard at
`engine/scripts/run-verify.sh:416-427` is not what produced the PASS.)

### C5 — mutation coverage of this bundle's diff

`scripts/mutants-in-diff` (the gate's own command, `cargo mutants --in-diff patch.diff
--no-shuffle`) against the final patch:

```
350 mutants tested in 8m: 187 caught, 163 unviable
EXIT=0
```

**0 missed, 0 timeouts** — against iteration 6's `324 mutants tested in 7m: 3 missed, 164
caught, 157 unviable`. The run in between (after the resume + containment fixes but before
the last two test edits) reported `1 missed, 1 timeout`, and both were real signal rather than
noise, which is why they are worth recording:

* `MISSED crates/core/src/metadata.rs:3626:27: replace < with <= in high_water_marks` — the
  **`orphan:`** ledger's ceiling comparison. Pre-existing code the diff carries; no fixture
  had ever put an orphan record at exactly `2^64`. Bound by legs (4)/(5) of
  `the_id_floor_is_exclusive_at_the_coordination_free_boundary`.
* `TIMEOUT crates/core/src/metadata.rs:3443:25: replace + with - in raw_chunk_id_floor` — the
  cursor advance in my first spelling of the scanner (`while let Some(at) = rest.windows(…)
  .position(…)`), where a mutated advance rewinds and the loop never terminates. A loop whose
  termination depends on its own arithmetic is a hazard on damaged input by definition, so I
  rewrote it to be driven by the window iterator (`for (at, window) in
  value.windows(FIELD.len()).enumerate()`, `:3443`): linear and terminating by construction,
  whatever the bytes are. The mutant is now caught rather than hanging.

## 7. Scratch

Everything transient went under `$PDCA_SCRATCH` (`/var/tmp/pdca/pdca-builder-635-*`): the CI
and gate logs and the two file snapshots used for the refutation runs. `cargo mutants` put its
build copy in `/var/tmp/pdca/cargo-mutants-*.tmp` (its own naming) and the worktree's
`mutants.out*/` are git-ignored (`.gitignore:14`), so none of it reaches the patch. All
removed at the end of the run.
