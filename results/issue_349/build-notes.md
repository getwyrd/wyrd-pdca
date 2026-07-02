# Build notes — issue 349 / mixed-era-placement-test-matrix

## What this is (and isn't)

Per the brief's "Invariant to restore" field: this slice changes **no production
code**. It is coverage-LOCKING — the matrix pins a property that already holds on
`main` (the resolvers landed via #139/#287/#346/#356, all merged). `patch.diff`
touches only the six test files the brief names, nothing under `crates/*/src/`.
Confirmed: `git diff --stat` against `origin/main` (`9f8d3d9`, post #357/#358
fetch) shows exactly those six files, 0 production lines.

## Verification posture actually used

The brief's own "Verification posture" field overrides the generic red→green
proxy here: these cells are "born green" (no prior failing assertion to flip).
So I did NOT try to manufacture a patch-level red/green pair. Instead, per the
brief's FORCING FUNCTION, for **every** new cell I:

1. confirmed it's green on `main` as shipped (`cargo test -p <crate> --test
   <file>`, recorded below per file);
2. applied a **temporary, local negation** of the production resolver the cell
   exercises;
3. re-ran the same test binary and confirmed the new cell(s) — and only the
   cells that genuinely depend on the fallback — went red;
4. reverted the negation (`git diff --stat` on the `src/` file showed empty
   after each revert) and re-confirmed green.

None of the negations are in `patch.diff` — they were applied/reverted in place
during this session, never committed. This section is the evidence trail Check
can't get from the diff alone (the brief says Do must record this here).

## Per-file additions, citations, and negation traces

### Read — `crates/core/tests/placement_record.rs`

Gap (brief Defect): only full rs(6,3) was covered (`write_records_placement_..`
:137, `moved_fragment_resolved_..`:192). Added (all green pre-existing, all new
green too):

- `empty_placement_ec_none_resolves_via_identity_fallback` (:285) —
  `EcScheme::None`, `placement: vec![]`.
- `empty_placement_rs_6_3_resolves_via_identity_fallback` (:345) — rs(6,3),
  `placement: vec![]`. Also satisfies the brief's "≥1 RS{6,3} cell" for Read.
- `short_placement_rs_6_3_mixed_explicit_and_fallback` (:398) — rs(6,3),
  `placement: vec![<3 explicit, off-identity>]`, indices 3..9 fall back.

Scope note: rs(6,3) "short" needs ≥2 fragments to be non-degenerate with
"empty"; `EcScheme::None` has exactly 1 fragment, so a "short" None case would
be indistinguishable from the empty case (both index out of bounds at 0) — I
did not add a separate None-short cell; `empty_placement_ec_none_..` already
covers the only index None has.

Negation: `crates/core/src/read.rs:103-105` (`fragment_dserver`), changed
`chunk.placed_dserver(index)` → `chunk.placement[index as usize]`. Result:
`cargo test -p wyrd-core --test placement_record` → the 3 new cells panicked
("index out of bounds"); the 2 pre-existing FULL-placement cells stayed green
(their vectors are already length `n`, so raw indexing never goes out of
bounds — exactly why full-only coverage was insufficient). Reverted; `git diff
--stat crates/core/src/read.rs` empty; all 5 green again.

### Scrub — `crates/custodian/tests/scrub.rs`

Gap: only full(None) (`detects_a_bitflip_..`:264). Added a `commit_chunk`
helper (:174, generalizes `commit_reference`:147 to arbitrary scheme/placement)
plus 4 cells, mirroring `gc.rs`'s 4a/4b/4c and the brief's explicit list:

- `detects_corruption_on_an_empty_placement_none_chunk` (:641) — 4a-equivalent.
- `detects_corruption_on_an_empty_placement_rs_chunk_above_index_zero` (:691) —
  4b-equivalent (RS{2,1}, index 1).
- `detects_corruption_at_a_short_placement_vectors_fallback_index` (:743) —
  4c-equivalent (RS{2,1}, `placement: vec![5]`).
- `detects_corruption_in_a_full_rs_6_3_placement` (:798) — the brief's RS{6,3}
  cell, full explicit 9-server placement, one corrupt fragment mid-vector.

Scrub resolves its reference set via `crate::gc::referenced_fragments`
(`scrub.rs:30` imports it) — i.e. it shares GC's resolver, not its own. So the
negation is GC's:

Negation: `crates/custodian/src/gc.rs:197-204` (`referenced_fragments`'s
expansion loop), changed the `(0..chunk.fragment_count()).map(|i|
chunk.placed_dserver(i))` expansion to raw `chunk.placement.iter().enumerate()`.
Result: `cargo test -p wyrd-custodian --test scrub` → the 3 empty/short cells
went red (`Satisfied` instead of `Changed` — the corrupt fragment was excluded
as "unreferenced" and never scrubbed); the full(None) and the new full
RS{6,3} cell stayed green. Reverted; `git diff --stat
crates/custodian/src/gc.rs` empty; all 10 green again.

### GC — `crates/custodian/tests/gc.rs`

Gap: empty/short matrix existed only at RS{2,1} (4a/4b/4c, :307/:375/:441).
Added the brief's required RS{6,3} cells:

- `identity_fallback_rs_6_3_empty_placement_protects_an_inner_index` (:504) —
  RS{6,3}, `placement: vec![]`, protects index 7.
- `short_placement_vector_rs_6_3_fallback_protects_fallback_index` (:563) —
  RS{6,3}, `placement: vec![50,51,52]` (short, off-identity explicit prefix),
  protects fallback index 8.

Negation: same `gc.rs:197-204` change as above. Result: `cargo test -p
wyrd-custodian --test gc rs_6_3` → both new cells went red (`Changed` instead
of `Satisfied` — the orphan won and the fragment was reclaimed). Reverted;
clean; all 8 green again. (This is the same negation point 4a/4b/4c already
pin — I re-confirmed the two new RS{6,3} cells independently rather than
assuming, since they're a different scheme/index shape.)

### Reconstruction — `crates/custodian/tests/reconstruction.rs`

Gap: only full RS{2,1} (`kills_a_d_server_..`:286); empty/short and the empty→
re-placement case were unpinned (brief Defect). `assess`
(`crates/custodian/src/reconstruction.rs:226-232`) expands placement through
`placed_dserver`; `repair_chunk` (`:388-418`) clones THAT expanded vector as
the base it repoints. Added:

- `reconstructs_a_pre_m3_chunk_with_empty_placement_to_a_full_length_record`
  (:472) — **the brief's required re-placement pin.** Writes a normal RS{2,1}
  object (`write_rs_2_1`, places at `[0,1,2]`), then downgrades the committed
  record's `placement` to `vec![]` via `metadata::commit_chunk_map` (the
  physical fragments are untouched — this is a faithful pre-M3 fixture, not a
  fabricated one). Kills server 1, reconstructs, and asserts: `Changed`;
  `placement.len() == fragment_count()` (3, not 0 or short); the exact value
  `[0,3,2]`; 3 distinct domains; reads succeed before AND after.
- `reconstructs_a_short_placement_chunk_resolving_the_fallback_index` (:590) —
  short case: copies index-0's bytes to an out-of-band server (9, domain "Z"),
  commits `placement: vec![9]` (length 1 < 3), kills server 2 (the
  fallback-resolved index), reconstructs, asserts the full-length result
  `[9,1,3]`.
- `kills_a_d_server_and_reconstructs_an_rs_6_3_chunk_to_full_redundancy` (:754)
  — the brief's required RS{6,3} cell. New `ten_domains()` (:701) /
  `write_rs_6_3` (:722) helpers; kills server 4 of a full 9-fragment placement,
  asserts the rebuilt fragment lands on the one free domain (J/server 9) and
  the result is `[0,1,2,3,9,5,6,7,8]` across 9 distinct domains.

Scope note: `assess` returns `Assessment::Unrepairable` for `EcScheme::None`
(`reconstruction.rs:220-223`, unmodified) — there is no redundancy to
reconstruct from a single fragment, so reconstruction inherently doesn't apply
to `EcScheme::None`. The brief's "across EcScheme::None and Reed-Solomon"
matrix dimension is therefore RS-only for this consumer; I did not add a
None case (it would just assert `Unrepairable`, which is existing, untouched
behaviour, not part of the placement-resolution closure this issue pins).

Negation: `crates/custodian/src/reconstruction.rs:230-232` (`assess`'s
placement expansion), changed to `let placement: Vec<DServerId> =
chunk_ref.placement.clone();` (raw). Result: `cargo test -p wyrd-custodian
--test reconstruction` → both empty/short pin tests went red (`Satisfied`
instead of `Changed` — `missing` stayed empty since the raw vector has 0/1
entries to iterate, so `assess` returned `Assessment::Drain` and nothing was
rebuilt); the full RS{2,1} and the new RS{6,3} cell stayed green (their
vectors are already length `n`). Reverted; `git diff --stat
crates/custodian/src/reconstruction.rs` empty; all 11 green again.

### Rebalance — `crates/custodian/tests/rebalance.rs`

Note: the empty-placement evacuation cases (`EcScheme::None` single fragment,
and RS with the draining fragment at index > 0) were **already present** on
`main` (:338, :435) — they shipped as part of #346's own fix (PR #357,
`2116119`), not a gap this issue still needs to close. The brief's defect text
("currently unlocked") describes the audit's pre-#357 state; #357 is already
merged into this worktree's base (`git log` confirms `2116119` is an ancestor
of `9f8d3d9`). What was still missing, per the repro instructions, is the
RS{6,3} cell:

- `drains_a_d_server_and_evacuates_an_rs_6_3_chunk_to_a_distinct_domain` (:581)
  — new `ten_domains()` (:529) / `write_rs_6_3` (:550) helpers; drains server 4
  of a full 9-fragment placement (graceful drain, server stays alive), asserts
  the evacuated fragment lands on domain J (server 9), full-length result
  `[0,1,2,3,9,5,6,7,8]`, 9 distinct domains, reads succeed.

Negation: `crates/custodian/src/rebalance.rs:165-167` (`plan_evacuations`'s
placement expansion), changed to `let placement: Vec<DServerId> =
chunk.placement.clone();` — the brief's specifically-named "pre-#346 raw-vector
path". Result: `cargo test -p wyrd-custodian --test rebalance` → the two
EXISTING empty-placement cells went red (exactly as their own docstrings
predict — they were written to catch this); my NEW RS{6,3} cell stayed green
(`Changed`, unaffected). This is expected, not a gap in the new test: a FULL
placement vector is identical whether read raw or through `placed_dserver`
(every index is explicit, the fallback branch is never reached), so a
full-only cell cannot, by construction, catch a fallback-resolution
regression — that's exactly why the matrix needs separate full vs. empty/short
cells per consumer, and why I didn't try to make the RS{6,3} cell also redden
here (it would require fabricating a short/empty RS{6,3} commit, which the
existing #346-era empty-placement cells already prove at smaller scale; the
RS{6,3} cell's job is the orthogonal scheme-size gap). Reverted; clean; all 8
green again.

### DST — `crates/server/tests/dst_erasure.rs`

Gap: `mixed_era_read` (:210, pre-existing) mixes *schemes* but writes FULL
placement from the real write path (`write.rs:171`), so it never exercises an
empty/short vector. Added:

- `maintenance_resolved` (:263) — a local helper mirroring how GC / scrub /
  reconstruction expand a chunk map (`crates/custodian/src/gc.rs:197-204`,
  `reconstruction.rs:230-232`, `rebalance.rs:165-167` — all
  `(0..chunk.fragment_count()).map(|i| chunk.placed_dserver(i))`). `dst_erasure.rs`
  is in `crates/server/tests/`, which does not depend on `wyrd-custodian`
  (checked `crates/server/Cargo.toml` dev-dependencies — no custodian crate),
  so I could not literally call GC's `referenced_fragments` (it's
  `pub(crate)` in `wyrd-custodian` besides). This helper is the smallest
  faithful stand-in: it calls the SAME public `ChunkRef::placed_dserver`/
  `fragment_count()` primitives the custodian loops call, so "maintenance"
  here means literally the same resolution a maintenance loop performs, not a
  reimplementation that could drift.
- `empty_placement_resolves_identically` (:272) — seeded property: commits an
  inode with explicit `placement: vec![]` on every chunk (the genuine pre-M3
  shape; no live writer ever emits it, mirrors `write_pre_m3_chunk` already
  established in `rebalance.rs:304`), then asserts (a) `read::read_object_from`
  reconstructs byte-identical, and (b) `maintenance_resolved` covers every one
  of `fragment_count()` fragments (never the raw vector's zero), each at its
  identity D server — i.e. read and maintenance resolve the SAME closure.
  Driven across all 64 seeds (`empty_placement_reads_and_maintenance_agree_
  across_seeds`, :373) and pinned at `REGRESSION_SEED` (:382, alongside the
  other properties).

Negation (test-local, since `maintenance_resolved` is itself test code, not
production — the brief's framing "the old `index % n`/raw path mis-resolves"
maps to the raw-vector form of this same helper): changed `maintenance_resolved`
to `chunk.placement.iter().enumerate()...` (raw). Result: `cargo test -p
wyrd-server --test dst_erasure` → `empty_placement_reads_and_maintenance_agree_
across_seeds` and `ec_properties_hold_at_pinned_regression_seed` both failed
("left: 0, right: 9" — maintenance resolved zero fragments against read's 9)
at every seed exercised, including the pinned regression seed. The other 5
properties (which don't touch empty placement) stayed green. Reverted; all 7
green again.

I also considered literally reverting `read.rs:103-105` (the production
negation used for the Read-consumer cells above) inside this DST test instead
of/in addition to the local `maintenance_resolved` negation — rejected: that
would only prove READ catches the regression (already proven by the
`placement_record.rs` cells), not that read and maintenance AGREE, which is
this scenario's whole point per the brief ("asserts read AND maintenance
resolve it identically"). Negating only the maintenance side isolates exactly
the property this cell is supposed to pin: two independent call sites of the
same resolver must produce the same answer, and an old raw-vector consumer
breaks that agreement.

## Why I didn't add a literal "short" case for Rebalance / a literal
   "EcScheme::None short" anywhere

Both are documented inline above (short is degenerate for `n=1`; rebalance's
short case is implicitly covered by the pre-existing `evacuates_a_pre_m3_chunk_
with_empty_placement_reed_solomon_index_gt_zero`, which is empty not short —
I did not duplicate a short-specific rebalance cell beyond what #346 already
shipped, since the brief's repro instructions for Rebalance only ask for "the
empty-placement evacuation case ... and an RS{6,3} case", not a short one; the
short-resolution mechanism itself (`.get(i).unwrap_or(i)`) is already pinned at
Read/Scrub/GC/Reconstruction, all sharing the identical `placed_dserver`
primitive rebalance also calls).

## Alternatives considered and rejected

**Ship the negations as part of `patch.diff` (a literal revert-and-restore
commit), so Check's automated gate sees a literal red→green pair.** Rejected:
the brief's Verification posture field explicitly forbids treating the
*absence* of a red→green flip as a failure, and explicitly assigns the redness
demonstration to `build-notes.md` via temporary negation, not to the shipped
patch. Shipping a negation in the patch would also violate "out of scope: any
production diff at all — this slice is tests only" (brief Scope field).

**Add a generic `placed_dserver` unit-test sweep in `core` instead of six
per-consumer integration cells.** Rejected on the brief's own terms: the
point of #349 (item 4 of the #292 audit) is that the *resolver* was already
correctly unit-tested but several *consumers* weren't proven to call it
correctly through their own real control points (`reconcile_step`, the real
write/read path). A `core`-only sweep would be testing the resolver again,
not closing the audit's actual gap (uneven consumer coverage) — it's the
"narrower proxy" the harness instructions warn against, not the real end
result the success criterion names.

**For the Reconstruction short-placement test, drive the explicit override
through the real `select_distinct_domains` ordering (registering a domain
labelled before "A"-"D" alphabetically) instead of physically copying a
fragment to a new server.** Considered, rejected on cost: it would require
either re-deriving the registration order that makes the selector pick a
specific non-identity server (coupling the test to the selector's *internal*
tie-break algorithm, which `placement.rs:185-195`'s own docs call
"ILLUSTRATIVE", not contractual) or accepting whatever the selector picks and
asserting against it post-hoc (weaker pin, doesn't let me choose a
deliberately off-identity value to demonstrate genuine override semantics).
Copying the already-written, already-checksum-valid fragment bytes to a
manually-chosen out-of-band server (9) is ~4 lines (`d9.put_fragment(frag(0),
bytes0)` + one field assignment) versus coupling to non-contractual selector
internals, and keeps the test's intent (mixed explicit + fallback resolution)
independent of the selector's tie-break choices.

## Commands run (this session, in the worktree)

- `cargo test -p wyrd-core --test placement_record`
- `cargo test -p wyrd-custodian --test scrub` / `--test gc` / `--test
  reconstruction` / `--test rebalance` (each pre/post each negation above)
- `cargo test -p wyrd-server --test dst_erasure`
- `cargo test -p wyrd-core -p wyrd-custodian -p wyrd-server` (full crate sweep,
  all green)
- `cargo fmt --all -- --check` (failed once on formatting, fixed via `cargo
  fmt --all`, re-checked clean) + one `clippy::unusual_byte_groupings` fix
  (`placement_record.rs`: `0x349_00u128` → `0x34900u128`)
- `cargo clippy -p wyrd-core -p wyrd-custodian -p wyrd-server --all-targets`
  (clean)
- `cargo xtask ci` (the project's full gate: fmt --check, clippy -D warnings,
  build --all-targets, `cargo test --workspace --exclude wyrd-dst`,
  cargo-machete, cargo-deny, conformance vectors, ADR-0035 statics scan, and
  the madsim DST sweep) → **"xtask ci: all checks passed"**, confirming the
  patch is commit-ready against the target repo's own gate, not just the
  scoped crate tests above.

`git status --porcelain` after all of the above: exactly the six files listed
in the brief's "Test file" field, modified; nothing else.
