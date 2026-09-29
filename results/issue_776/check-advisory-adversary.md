# Adversarial review — issue #776 (`repoint_chunk`, round 6)

Verdict: **I could not break the production code.** The one thing I would raise is a
test gap: two hand-made mutants survive, both on properties the brief names. The code
itself behaves correctly on both inputs.

## Finding

- NEEDS-HUMAN [human] — **Two brief-named properties have no test that goes red, and C5's
  "27 caught, 0 missed" hides this.** cargo-mutants cannot generate either mutant below, so
  reading C5 as "every pin is pinned" is not warranted. I applied each mutant by hand in a
  scratch copy and all 15 `placement_move` tests stayed green:
  1. `crates/core/src/metadata.rs:3241`: change `usize::from(expected)` to
     `prior.placement.len()`. Every test prior carries a full-length placement, including
     the campaign's (`:5092`). So a check against the fragment count and a check against
     the old vector's length look the same to the tests. Failing case under the mutant: a
     pre-M3 RS(2,1) chunk with `placement: []`. The mutant **accepts** an empty replacement
     and **refuses** a correct `[20, 21, 22]` one. The patch's own doc (`:3156`) names
     exactly this empty-vector case. `a_replacement_placement_must_name_every_fragment`
     (`:4934`) never moves such a chunk.
  2. `crates/core/src/metadata.rs:3316`: pin `encode(&record)` (the re-encoded record)
     instead of `bytes` (the bytes actually read). Brief item 3 and the doc at `:3197` say
     the segment pin is "the segment record's own bytes as read here". Failing case under
     the mutant: a stored segment row that decodes but is not in `encode`'s spelling (I
     used the same fields in a different order). Every move on that row would lose its
     CAS, pass after pass: the "refused forever" C-1 class this lineage exists to remove.
  I added one probe test for each case. Both pass on the patch and each fails under its
  mutant (`probe_legacy_empty_placement_prior`,
  `probe_noncanonical_segment_row_is_pinned_by_its_own_bytes`), so the production code is
  right and only the tests are weak. I tagged this `[human]`, not `[impl]`, because it is
  a close call. No writer in the tree produces either input today. This is round 6, and
  the rubric says not to chase silence. `patch.diff` is 49,947 bytes against a ≤ 50 KB
  budget, so adding the two probes (~1.2 KB) means trimming something else. If you want
  it fixed, it is a small, mechanical `[impl]` rebuild.

## Attempted and could not refute

- **Red→green.** C4-verify is green-only, as the brief declares (in-crate tests only).
  That is not a refutation. I re-ran the 15 tests green on a copy of `$PDCA_TARGET`. Then
  I checked the brief's four named negations by hand, and each goes red: drop
  `chunk == prior` (`:3365` in `chunk_at`); drop the root pin; bypass the ceiling in
  `weighed`; replace `checked_add` with `wrapping_add`. The C4-ci log shows all 15 tests
  and "all checks passed", with no timeout this round. The two lines diff-cov missed
  (`:4495`, `:5200`) are `panic!` arms inside tests.
- **Wrong-record writes.** Zero-length chunks on a segment boundary: equality picks the
  record, and a mismatch in both candidates is a `Conflict`. A chunk with bytes that
  starts a segment never reads the previous segment: the garbage-neighbour test at offset
  5 kills the iteration-3 mutant.
- **Over-ceiling and damaged rows.** A row is weighed before it is decoded
  (`:3287`, the same `>` as `read_group_range`). Live versus retired goes through
  `retired_or`. Exactly-V is admitted in both directions.
- **Flat arm.** `state` and ADR-0047 metadata are kept (the test uses a `Pending` root).
  The version advance is checked. The root pin fails a superseded batch and leaves the
  store byte-identical.
- **Seeded campaign.** It uses the real redb store and the production resolver. Its model
  is blind to the pins, and it asserts that all four race classes ran, so it cannot pass
  with nothing tested. The prepare runs as one step, but no read inside it can be
  interleaved in a way the pins do not already cover.
- **Considered and not raised.** (a) Mutant M7, `continue` instead of `Conflict` after a
  *retired* anomaly (`:3324`), survives. It is nearly equivalent: it only changes the
  answer for a generation that is already retired, and the root pin dooms that batch
  anyway. (b) No `MAX_ROOT_SEGMENTS` or extra-row check: the brief bans a range walk, and
  that constant's doc limits the guard to publication and the ranged read. (c) Docs
  currency carries `deferred: #777`, which the protocol treats as settled. (d) The
  `:2493` citation in the `flat_value_ceiling_crossed` doc is stale, but it was already
  stale on the base.
- **Scope.** One file. 144 semantic non-test lines (≤ 170). No `MAX_ROOT_VALUE_BYTES`
  comparison. `commit_chunk_map` is untouched.
