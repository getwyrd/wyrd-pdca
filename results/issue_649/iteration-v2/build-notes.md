# Build notes — issue 649 / shared-segmented-map-resolver-and-read-paths (iteration 2)

Withheld from the reviewer; for the human at sign-off.

Line citations are against `$PDCA_WORKTREE` = `/home/eddie/development/wyrd/wyrd.pdca-wt`,
whose HEAD is `6e7c255` (`pdca-integrate: issue_648`) = `origin/pdca-integration/main`,
the base `C4-verify` resolves for this wave>0 bundle.

Iteration 1's full artifacts are preserved in `iteration-v1/`. This file documents the
**delta**, then re-states the standing rationale and the three refutation answers for the
whole patch (the reviewer never sees either file, so this one has to stand alone).

---

## 1. Carry-forward: what each finding got

### T4 batch review — 7 blocking findings, all **fixed**, none rejected

| Finding (round 1) | Fix |
|---|---|
| `metadata.rs:2156` **BUG** ×3 — "a malformed segment key is propagated before `root_still_names` … a concurrently retired generation causes an error instead of restarting" | The range walk no longer *judges* anomalies. `GroupRange::Beyond(u32)` became `GroupRange::Anomaly(ChunkMapError)` (`crates/core/src/metadata.rs:2111-2121`); the unparsable-key arm now returns `Ok(GroupRange::Anomaly(..))` (`:2175-2179`), as do the foreign-nonce/epoch arm (`:2184-2189`) and the past-the-claim arm (`:2192-2197`). `read_segments` settles **all** of them through the one arbiter (`:2239`). So *every* shape a `seg:` range can show — unparsable key, foreign key, unnamed row, absent row, undecodable row, extent disagreement — now asks the same question first: does the root still name this generation? |
| `segmented_map_resolution.rs:226` **TEST-GAP** — "the recording double neither records `scan` calls nor page limits" | The double records **all three** channels — `get` keys, unpaged `scan` prefixes, and `scan_page` prefix **with its limit** (`crates/core/tests/segmented_map_resolution.rs:310-316`, `:345-391`). The oracle now asserts (a) every page prefix is the group's own **and** every limit ≤ the root's own claim + 1, (b) **zero** unpaged `scan` calls, (c) no `get` of any `seg:` key, (d) nothing in the whole footprint intersects either decoy group's range (`:437-491`). |
| `custodian.rs:1477` **TEST-GAP** — "the DST mutation only flips the root without deleting any old segment and accepts old bytes" | The nemesis now deletes segment 1 of the retired generation on half the seeds (`crates/dst/tests/custodian.rs:1488-1493`), and the property asserts the answer is **exactly** the live generation's bytes (`:1502-1509`) instead of "old *or* new". On the reclaiming seeds the old generation is genuinely incompletable, so a reader that failed to re-read the root and restart has no old-generation answer left to succeed with. |
| `segmented_object_read.rs:249`/`:253` **TEST-GAP** ×2 — "both generations have the same 32-byte size and absent metadata, so restart tests cannot detect stale `Content-Length`, ETag, content type, modification time" | The two generations now differ in **every** framed field: length 32 vs 40 and distinct ETag / content type / modified (`crates/server/tests/segmented_object_read.rs:84-93`, `:126-134`, `:280-287`). Both restart tests assert all four come from the live generation (`:317-321`, `:351-354`); the ranged one asks for a span that lies **entirely past** the superseded generation's last byte (`:347`), so a 416 computed from the stale size is a failure, not a silent pass. The whole-object test asserts the segmented generation's *own* framing rides out (`:177-180`). |

### C5 — three surviving mutants, all killed (verified by hand-mutating each line and re-running)

| Mutant | Killed by |
|---|---|
| `metadata.rs:2142:18 replace > with >=` — the segment ceiling (now `:2156`) | `a_root_at_exactly_the_segment_ceiling_resolves` (`segmented_map_resolution.rs:534`) — a root naming exactly `MAX_ROOT_SEGMENTS` (512) segments, seeded with all 512 real `seg:` records over 512 real chunks, must read back byte-identical. With `>=` it is refused and the test fails. |
| `metadata.rs:2159:40 replace \|\| with &&` — the nonce/epoch key check (now `:2183`) | `a_row_from_another_epoch_is_never_spliced_into_this_generation` (`:830`) — the double splices a row keyed to the *previous epoch* of the same group, whose extents satisfy the root's table exactly, so only the key says it is not ours; with `&&` it is accepted, the retired generation's chunks replace segment 0, and the read succeeds where the test demands a refusal. |
| `metadata.rs:2239:56 replace \|\| with &&` — the extent check (now `:2260`) | `root_unchanged_extent_offset_only_mismatch_fails_closed` (`:637`) and `..._len_only_...` (`:668`) — the round-1 fixture bent **both** coordinates of one segment, which `&&` still catches; these bend exactly **one** each. |

### T5 — "assert the recorded metadata-get keys as well as scan prefixes"

Done: `gets` is asserted, not merely collected — clause (c) forbids any `seg:` key
arriving by `get`, clause (d) runs the decoy-intersection check over the union of gets,
scans and pages (`segmented_map_resolution.rs:467-490`). The `footprint()` helper
(`:337-342`) exists so a future channel cannot be added to the double without the oracle
seeing it.

### C4-ci — `cargo deny check` failed on RUSTSEC-2026-0221

Not this patch's doing (`event-listener` 5.4.1, unsound, reachable only via
`madsim` → the DST dev-graph) and red on the unmodified base too — but the gate is
**gating**, so "pre-existing" does not make the bundle shippable. Fixed with the
upstream-recommended remediation, `cargo update -p event-listener --precise 5.4.2`:
**5 lines of `Cargo.lock`** (`Cargo.lock:1202-1210`; 2 insertions, 3 deletions), no
dependency added, no `Cargo.toml` touched. All three deny legs `cargo xtask ci` runs are
now green (default graph; `--all-features --config deny-all-features.toml advisories`;
`--all-features licenses bans sources`).

*Rejected alternative — waive it in `deny.toml`:* 1 line in the `ignore` list plus a
justification comment (~6 lines), i.e. cheaper by nothing that matters and **worse**:
`deny.toml:16-27` states that its `ignore` list is keyed by advisory ID alone and applies
to the whole graph, so parking RUSTSEC-2026-0221 there would also suppress it if a future
*default* dependency pulled an affected `event-listener` — "silently holing the very wall
this file exists to be", in that file's own words. A waiver also survives the fix, whereas
the lock bump disappears the moment upstream's own bump lands.

*Rejected alternative — leave it red and declare it pre-existing:* that is what iteration 1
did; the gating gate stayed red, and every sibling bundle in this wave would re-inherit it.

**Flag for the human:** the `Cargo.lock` hunk is the one file outside the brief's declared
scope. It is separable — dropping that hunk leaves the fix intact and C4-ci red on an
advisory unrelated to this change.

---

## 2. What shipped (whole patch)

- `crates/core/src/metadata.rs` — `MapResolution` (`:2056`), `root_still_names` (`:2068`),
  the single resolve-retry arbiter `retired_or` (`:2094`), the described-anomaly
  `GroupRange` (`:2111`), the bounded paged range read `read_group_range` (`:2151`, ceiling
  refused before the first page at `:2156`), `decode_segment_record`, `read_segments`
  (`:2231`), and the three entries `resolve_chunk_map` (`:2296`) /
  `resolve_current_chunk_map` (`:2339`) / `resolve_live_chunk_map` (`:2386`), plus the six
  new `ChunkMapError` variants they raise. `InodeRecord::chunk_map`'s doc now points at the
  resolver instead of "read it through `as_flat` until a resolver exists (#649)"
  (`:1272-1279`).
- `crates/custodian/src/resolve.rs` (new) + `crates/custodian/src/lib.rs` — `chunks_of`
  (the maintenance plane's door, always against the **live** root) and `classify_root` (the
  containment arm every `scan("inode:")` loop needs), with 5 unit tests.
- `crates/core/src/read.rs` — `read_object_from` → `read_object_chunks(chunks, map, size)`;
  `read_object` resolves first (`:503-520`); `committed_inode` returns `(InodeId,
  InodeRecord)` so the gateway has the root key a retry needs.
- `crates/server/src/lib.rs` — `get_object_streaming` and `get_object_range` resolve through
  `metadata::resolve_live_chunk_map`, and frame the response from the generation the bytes
  came from (`served`).
- `crates/dst/tests/custodian.rs` — Tier-0 property 9, `segmented_resolve_never_tears`.
- `docs/design/architecture/06-runtime-view.md` §6.2 step 2 — the resolver paragraph only.
- The two new test targets (16 + 4 tests) — the C4-verify discriminators.
- Mechanical migration (declared, counted separately): 10 files whose only change is
  `read_object_from(&s, &r)` → `read_object_chunks(&s, r.chunk_map.as_flat().expect(..),
  r.size)`.

---

## 3. Design choices and what I ruled out (this iteration)

**Anomaly *described*, not raised — vs. plumbing `root_key` into `read_group_range`.**
The finding could also be closed by giving `read_group_range` the root key and letting it
call `retired_or` itself. Cost: 2 extra parameters threaded through, and — the real
objection — **two** places that answer "retired, or corrupt?", which is precisely the
"single-sourced resolution" invariant this slice exists to establish. The chosen shape has
exactly one arbitration site (`metadata.rs:2239`) and a range walk that only *reports* what
it saw. Same line count either way (the `Anomaly` arm replaced the `Beyond` arm; the
`SegmentUnknown` construction moved from `read_segments` to `read_group_range`).

**Four store doubles → one instrumented `Probe` with a `Quirk`.** Round 1 had
`RecordingStore`, `SupersedeMidResolve`, `SmugglingStore` and `ShufflingStore`, each with
its own ~30-line `MetadataStore` impl. Consolidating to one double + a 4-variant `Quirk`
enum (`segmented_map_resolution.rs:287-391`) removed **67 semantic lines** (719 → 652 in
that file, measured) *and* means every test — not just the two that used the recorder — now
runs against a store that records its whole access footprint. This was the budget headroom
that paid for the new cases.

**Exact-ceiling: a real 512-segment object, not an error-shape assertion.** The cheap way to
kill the `>`→`>=` mutant is to assert that a 512-segment root gets *a different error* than
a 513-segment one. Cost of the real thing instead: ~12 lines and ~40 ms (2 048 bytes → 512
chunks → 512 `seg:` records through the existing helpers). It binds the end result — an
object written right up to the ceiling still reads — rather than an error taxonomy, and a
ceiling that refuses *at* its own limit is exactly the permanent-unreadability mode C-1
forbids.

**One-coordinate extent fixtures.** The root's table is validated at decode (contiguity,
and `size == span`), so "bend one coordinate of the root" is not expressible without
bending a second. Both new fixtures therefore bend the **stored record** instead: same
chunks one byte later (offset-only, `:637`), and a single-segment root claiming one byte
more than the record carries (length-only, `:668`). The length-only case is why
`assert_resolved_typed_refusal` documents that a downstream `ReadError::SizeMismatch` fails
the assertion: with `&&` the resolver passes an under-described map on and the *byte* layer
notices, which is not "fails closed".

**The smuggled-row fault model.** A conforming `MetadataStore` never returns a row outside
the prefix it was asked for, so this row can only come from a double. It is kept because
the production code carries the check, `MetadataStore` has five independent backends, and
the repo's own mutants policy is explicit that a killable survivor gets a regression test
and never an `exclude_re` entry (`.cargo/mutants.toml:1-11`). The test also states the
invariant positively: the resolver pins the epoch by parsing every key it consumes.

**DST: always-delete vs. per-seed choice.** Deleting the segment on *every* seed would drop
coverage of the stale-but-complete arm (root moved, all records still present, caught only
by the post-read root re-check at `metadata.rs:2281`). The rng picks per seed and the
assertion is the same either way, so 50 seeds cover both arms and each individual seed
asserts an exact answer.

**Standing choices from iteration 1** (unchanged, restated because the reviewer sees
neither file): scope cut vs. `sources/salvage.diff` — #651's `ChunkHome`/`repoint_chunk` and
#650's five-pass containment carry no caller in this slice (salvage's `resolve.rs` alone is
367 lines vs. this one's 120 production + 170 test); `scan_page` rather than salvage's
`scan`, because `scan` is complete-or-fail-loud at `SCAN_CAP`; raw `seg:` seeding in every
fixture, never a committer (the brief's verbatim rework note); and no import of anything
this patch adds in either test file.

---

## 4. Budget — over the brief's figure, declared

Measured on the diff (added lines, non-blank, non-comment), mechanical-migration files
excluded per the brief's carve-out:

| File | Semantic added lines |
|---|---|
| `crates/core/tests/segmented_map_resolution.rs` | 652 |
| `crates/server/tests/segmented_object_read.rs` | 273 |
| `crates/core/src/metadata.rs` | 270 |
| `crates/custodian/src/resolve.rs` | 209 (52 production + 157 its unit module) |
| `crates/dst/tests/custodian.rs` | 153 |
| `crates/server/src/lib.rs` | 29 |
| `crates/core/src/read.rs` | 27 |
| `Cargo.lock` / `custodian/src/lib.rs` / the docs line | 4 |
| **total** | **1 617** (+ 205 mechanical, allowed on top) |

Against the brief's `≤ ~1,500`. Iteration 1 measured **1 508** by the identical method, so
the overshoot is 109 lines and every one of them is a test the Check round demanded (7 batch
findings + 3 mutants + T5), after the double-consolidation already gave back 67.

Two things worth weighing at sign-off rather than hiding:

- **Production is 379 lines against the brief's own ~660 estimate** — the overshoot is
  entirely test bodies, in the direction the brief's "prune the co-located resolver tests to
  the binding cases" line was worried about, not extra surface.
- **The named fallback split does not bite here.** Splitting *read paths* out of *resolver*
  leaves the resolver slice carrying metadata.rs (270) + resolve.rs (209) + the core test
  file (652) + the DST property (153) ≈ 1 284 by itself, and the two slices' combined line
  count is *higher* (duplicated fixtures across two test targets). I judged shipping at
  +7.8 % better than iterating to Plan for a split that reduces nothing; that judgement is
  the human's to overturn, and the honest number is above.

Files: 10 non-mechanical (≤ 15 ✓), 20 counting the declared mechanical migration.

---

## 5. The three refutation questions

**(a) Genuine red?** Yes — measured by the project's own gate, not inferred.
`./engine/scripts/run-verify.sh` (C4-verify) with `PDCA_VERIFY_BASE=origin/pdca-integration/main`
reports **PASS — red without the fix, green with it**: on the reverted tree both test files
still *compile* (they import only base-visible #648 symbols) and all 16 core tests fail as
`SegmentedMapUnsupported` panics/assertions — no compile-red scored as pass, no vacuous
`0 tests … ok`. `--classify` shows exactly the two `ADDED_TEST` discriminators.

Per-defect red was measured the same way, by reverting one thing at a time and re-running
(each restored afterwards from a scratch copy):
- ceiling `>` → `>=` ⇒ `a_root_at_exactly_the_segment_ceiling_resolves` FAILS.
- key check `||` → `&&` ⇒ `a_row_from_another_epoch_is_never_spliced_into_this_generation` FAILS.
- extent check `||` → `&&` ⇒ both one-coordinate tests FAIL.
- malformed key routed back to `parse_seg_key(&key)?` (the reviewed defect) ⇒
  `root_superseded_with_a_malformed_seg_key_still_restarts` FAILS.
- `retired_or` forced to always fail closed ⇒ the **DST** property
  `segmented_resolve_never_tears` FAILS over the 50-seed sweep (so the DST leg is not
  decorative either).

**(b) Production path?** Yes. Both files drive `wyrd_core::read::{read_object, read_path}`
and `wyrd_gateway_core::ObjectGateway::{get_object_streaming, get_object_range}` over a real
`RedbMetadataStore` + `FsChunkStore`; the resolver under test is always
`wyrd_core::metadata::resolve_*`, never a copy. The single double (`Probe`) wraps that real
backend and forwards every call unchanged except the one quirk each test names — it
re-implements nothing. The custodian wrapper's own tests call `chunks_of`/`classify_root`
directly over an in-memory store.

**(c) Fixture includes the fault?** Yes — each anomaly is a real, present condition of the
store the resolver reads, never curated out: the absent segment is never written; the
undecodable bytes are genuine garbage; the unnamed row genuinely sits under the group's
range; the malformed key is genuinely stored under it; the extent disagreements are real
byte values; the ceiling-breaching root genuinely decodes; the exact-ceiling object has all
512 records genuinely present; the smuggled row genuinely carries another generation's
chunks; the mid-resolve supersede/delete genuinely commits *inside* the resolver's own
`scan_page` call. The bounded-access oracle asserts over the **whole** recorded footprint
rather than a filtered view of it.

---

## 6. Verification run log (this cycle)

- `./engine/scripts/run-verify.sh` (C4-verify, project gate) — **PASS**: red without the fix
  (16 failed / 0 passed), green with it. Re-run against the **final** `patch.diff` byte-for-byte
  (the artifact in this bundle), not an earlier draft: PASS both times.
- `./engine/xtask.sh ci` (C4-ci, project gate — fmt, clippy `-D warnings`, build, test incl.
  the 50-seed DST sweep, cargo-deny ×3, conformance vectors, statics, typos, docs render) —
  see §7 below.
- `scripts/mutants-in-diff` (C5, advisory) — **56 mutants tested in 2m: 14 caught, 42
  unviable, 0 missed** (round 1: 3 missed, 11 caught, 42 unviable).
- `cargo test -p wyrd-core --test segmented_map_resolution` — 16 passed.
- `cargo test -p wyrd-server --test segmented_object_read` — 4 passed.
- `cargo test -p wyrd-core -p wyrd-custodian -p wyrd-server` — 70 test binaries green, no
  regressions from the `read_object_chunks` rename or `committed_inode`'s new shape.
- `RUSTFLAGS="--cfg madsim" MADSIM_TEST_NUM=50 cargo test -p wyrd-dst --test custodian` —
  11 properties green in 12.2 s, including `segmented_resolve_never_tears`.
- `cargo clippy --workspace --all-targets` and `RUSTFLAGS="--cfg madsim" cargo clippy -p
  wyrd-dst --all-targets` — clean. `cargo fmt --all -- --check` — clean.
- `cargo deny check`, `cargo deny --all-features --config deny-all-features.toml check
  advisories`, `cargo deny --all-features check licenses bans sources` — all ok.
- `typos` — clean repo-wide (the first `xtask ci` run caught `mis-key` / `mis-description` in
  two of my doc comments; reworded, re-run clean).

Scratch: everything throwaway lived under `${PDCA_SCRATCH}/pdca-builder-649-*` and was
removed.

## 7. Gate results measured in this Do beat (all four, on the final artifact)

| Gate | Round 1 | Now |
|---|---|---|
| **C4-ci** `cargo xtask ci` (gating) | FAIL — `cargo deny check` exit 1 | **PASS, exit 0** — "xtask ci: all checks passed", including `cargo test --workspace`, the 50-seed `--cfg madsim` DST sweep, all three cargo-deny legs, conformance vectors, the statics/unsafe/deploy/gitlink guards, `typos` and the docs render |
| **C4-verify** per-fix red→green | PASS | **PASS** — "red without the fix, green with it" |
| **C5-mutants** (advisory) | 3 missed | **0 missed** (14 caught, 42 unviable) |
| **T4-batch-review** (gating) | 7 blocking | all 7 fixed in the patch (the gate itself re-samples at Check) |

## 8. review-rejected.md

Not created, and deliberately: all **7** batch-review findings were fixed, none rejected, so
there is nothing to record (`review-batch.md`'s triage rule: fixed *or* recorded-rejected).
None of the brief's four standing "Do-not-re-earn" rejections is touched by this patch — no
caller-side fan-out timeout is added (the resolver adds no network bound; `ChunkStore` still
owns it), nothing retracts published bytes, no admission-slot logic is touched, and no
settled decision in the slice issue's body is re-litigated.
