# Build notes — issue 635 (segmented-chunk-map), iteration 14

**Withheld from the reviewer.** For the human at sign-off.

## What this round is

Iteration 13's patch was gate-green on `cargo xtask ci` and C4-verify; it failed **T4**
(4 blocking findings) and C5 (advisory, 13 missed mutants). The sign-off rationale said the
four findings are *"implementation completeness bugs against already-settled spec … Fix in
place; no scope/design change needed."*

So this round starts from `iteration-v13/patch.diff` applied to the brief's base
(`origin/main` @ `9120f7a`, verified clean: `git apply --check` OK, worktree at `9120f7a`,
no `$PDCA_BASE`/`$PDCA_VERIFY_BASE` in the environment and no `stack-base` file in the
bundle — the brief's `Falsifiability` 2 STOP condition does not fire) and changes **only**
what the four findings and their root causes require. Relative to iteration 13 the delta is
**+862 / −295 lines** in `crates/core/src/metadata.rs` and nothing else
(`diff -u` of the reconstructed v13 file against the current one; scratch copy under
`$PDCA_SCRATCH/pdca-builder-635-redleg/mychanges.diff`, deleted at the end of the run).

## The four findings, and what each one actually was

### 1 + 4 — unchecked `version + 1` (`metadata.rs:3268`, `:2426`)

Two spellings of one class. The staged publication had refused
`RootVersionOverflow` since round 12, but the flat repoint and the three flat committers
reached the same arithmetic by a different route. **Fixed at the foundation** rather than
per site: one `next_root_version` helper (`crates/core/src/metadata.rs:2346`), used by all
five (`:2386`, `:2448`, `:2523`, `:3299`, `:3755`). `grep -n "version + 1"` over the
production half of the module now returns nothing but doc prose.

Finding 4 is the *ordering* half: `commit_chunk_map_superseding` computed `next` (and with
it the version) **before** the segmented-prior refusal, so a segmented root at `u64::MAX`
panicked instead of answering `SegmentedRetirementUnsupported`. The refusal now runs first
(`:2437`), matching the leased twin (`:2509`) which already had the order right — the same
"deterministic refusals before any work" rule the staged publication follows (leg B(iv)).

Three of the five `+ 1` sites are **base** code (`9120f7a:crates/core/src/metadata.rs:551`,
`:595`, `:656`). Making them checked is a behaviour change to code the slice did not
introduce, and I made it deliberately: they are all inside this patch's blast radius
already (each grew a segmented-shape refusal this slice added), and leaving one door
unchecked next to four checked ones is precisely the state that produced this finding.

### 2 — the flat repoint's missing ceiling charge (`metadata.rs:3273`)

The segmented arm charged `check_record_ceilings`; the flat arm — the older path, the one
every pre-#635 evacuation and reconstruction repair takes — did not.

**The interesting part is that the obvious fix is wrong.** Charging the ceiling
unconditionally at the flat arm contradicts a decision *this same bundle* recorded in round
13 (`review-rejected.md`, `metadata.rs:1897`): a flat root over the 100 KB ceiling can
legitimately exist (the base's `commit_chunk_map` writes a map of any size and redb / TiKV /
etcd store it), and refusing to rewrite one turns "a record FoundationDB would reject" into
"an object no maintenance pass can ever repair" — strictly worse. So the charge is on
**growth**: `if next_bytes.len() > prior_bytes.len() { check_value_ceiling(…)? }`
(`:3332`). A repoint may not make a record's ceiling problem worse; it is not the place
that resolves one it inherited (that is `PutObject`'s chunk-size selection, #508).

The test binds both directions and I proved it (see refutation (a) below): with the guard
removed the growth leg passes silently; with the charge made *unconditional* the
legacy-repair control fails with `ValueOverCeiling { bytes: 199992 }`. Only the
growth-conditional shape passes.

The segmented arm keeps its unconditional charge, and the asymmetry is stated in the code:
a `seg:` record has exactly one writer (`SegmentedPublication`), which charges every batch
it assembles, so an over-ceiling segment record cannot exist to be repaired.

### 3 — the id floor's silent under-report (`metadata.rs:5366`)

The real defect is bigger than the line the finding names, and this is where I did not do
the minimal thing. `raw_chunk_id_floor` answered *"the largest id I could read"* and the
caller treated it as *"the largest id this record names"*. Those differ exactly when the
damage destroyed an id — which is the only case the function exists for. Bytes naming ids
`5` and `900` whose second token is destroyed read as `5`, and a floor of 5 is a floor below
live fragments (issue #364).

Fixed at the reading:

* `RecoveredIds` gains `complete` (`:5051`) — *did this reading account for every id the
  record names?* Only a **parse** can prove it (it enumerates every field, wherever
  nested); a byte scan over bytes no parse survived cannot, and never claims to. A scan can
  still *break* completeness — an `id` it could not read is one the parse dropped as a
  duplicate key.
* `RecoveredIds::contribution` (`:5078`) answers `floor` when complete and `ceiling - 1`
  when not.
* Both walks go through one helper, `unreadable_record_floor` (`:5095`), so the `seg:`
  (`:5499`) and `inode:` (`:5636`) halves cannot drift apart again — the round-9 directive
  ("fix at the foundation, not by whack-a-mole per call site") applied to the same pair of
  call sites it was written about.

Two consequences I want the human to see:

**(a) `widest_id_with_prefix` is deleted** (−45 lines of code, −60 of tests). A truncated
`"id":18` token is now simply *unreadable*, and an unreadable id escalates the whole
record's contribution to `ceiling - 1` — which is at or above every completion that
arithmetic could ever have named. The arithmetic has been the subject of a finding in
rounds 8, 9, 11 and 13; it is dominated by the new rule, so keeping it would have left ~120
lines whose only remaining effect was telemetry precision, and a fifth round of findings
against a refinement that cannot change any answer.

**(b) the escalated floor is deliberately wide, and it costs nothing today.** One
unreadable record makes `high_water_marks` return `2^64 - 1` as the chunk floor. That is
what the brief's containment table asks for — "totality plus a floor that is a strict
over-approximation" — and `Gateway::recover` **discards** the chunk floor on this base
(`crates/server/src/lib.rs:124`, `let (max_inode, _max_chunk) = …`; chunk ids are
coordination-free per ADR-0019), so no production path is affected: the gateway still
starts, every healthy object still serves, and the number becomes conservative for whoever
reads it next. It never reaches the cluster's coordination-free space (`< 2^64`), and it is
attributed on the audit seam as it is raised (`emit_record_unreadable` now carries
`contributed_floor`, `:5531`), so an operator sees *which* record widened it rather than
inferring it.

I also fixed a false-alarm in the same area that the escalation would otherwise have
amplified: a **bare** JSON integer above `u64::MAX` (i.e. every cluster-mode chunk id) is
parsed by `serde_json` as a float and does not read back as an integer, so the parse reader
counted it *unreadable* — which under the new rule would raise the in-process floor to its
top over a record whose id was read perfectly well. `json_chunk_id` (`:5225`) now gives the
same three-way answer the byte scan gives, with the out-of-range arm read through the
float. The repo's own rationale for the scan's `OutOfRange` arm is the argument here
("counting it would raise an unrecoverable-id alarm about a record whose id was read
perfectly well"); the two readers now agree.

### A defect nobody reported: a doc comment split across two tests

`metadata.rs` (v13) carried a doc block that ran `"… The generation is therefore"` straight
into the next test's `"**A repoint moves fragments…**"`, with its own tail (`"checked, not
assumed."`) stranded on the test it belonged to. An edit had inserted a test *inside*
another test's doc comment. It compiles, so no gate saw it. Repaired: the paragraph is
reunited with `a_segment_repoint_refuses_a_home_from_another_generation` (`:10704`).

## Alternatives I rejected, with their cost

* **Fix finding 3 only at `segment_chunk_floor` (the line the finding names).** −40 lines
  versus what I shipped. Rejected: the identical bug sits at the `inode:` walk 140 lines
  below, through the same helper, and "fixed one of the two call sites" is what turned the
  round-9 finding into the round-13 finding. The foundation fix is the same helper for
  both.
* **Escalate only when the recovery found *nothing* (`floor == 0`)** — the literal reading
  of the finding ("no recoverable ID digits"), and ~6 lines cheaper. Rejected as unsound:
  recovering one id out of five is no better evidence than recovering none. Completeness,
  not emptiness, is the property.
* **Keep `widest_id_with_prefix` alongside the escalation** (+0 lines now, −105 lines not
  taken). Rejected: after the escalation its result can never change a contribution (it
  only ever runs on values that failed to parse, which escalate anyway), so it is ~105
  lines of dead refinement with a four-round history of findings against its arithmetic.
* **Make `high_water_marks` return the damage** (a third tuple element or a struct).
  Rejected mechanically, not on taste: the brief's leg A test file must compile **on the
  base** (`Falsifiability` corollary), and it destructures the 2-tuple
  (`crates/custodian/tests/segmented_map_consumers.rs:1194`). Changing the signature would
  turn leg A's assertion-red into a build error — the one thing this slice may not do.
* **Charge the flat repoint's ceiling unconditionally** (−12 lines, no growth comparison).
  Rejected: it breaks repairs of legitimately-stored over-ceiling flat maps, contradicting
  this bundle's own round-13 decline; demonstrated by running it — the legacy-repair
  control fails with `ValueOverCeiling { bytes: 199992, ceiling: 100000 }`.

## Refuting my own tests (forced, recorded)

**(a) Genuine red?** Yes — I reverted each production fix in place and re-ran. Recorded
verbatim from the run:

| Test | Revert | Result |
|---|---|---|
| `every_root_rewrite_refuses_a_version_that_would_overflow` | `next_root_version(prior_root)?` → `prior_root.version + 1` | FAILED — panic "attempt to add with overflow" at `metadata.rs:3295` |
| `a_segmented_prior_is_refused_before_the_version_arithmetic` | refusal moved back after `next` | FAILED — panic at `metadata.rs:2441` |
| `a_flat_repoint_that_would_grow_the_root_past_the_value_ceiling_is_refused` | growth charge removed | FAILED at the `expect_err` (`:11151`) |
| …the same test, **over-fixed** | charge made unconditional | FAILED on the legacy-repair control (`:11176`): `ValueOverCeiling { bytes: 199992 }` |
| `a_record_whose_ids_cannot_be_read_raises_the_floor_rather_than_lowering_it` | `contribution` → `self.floor` | FAILED — floor 5 where 2^64−1 is required |
| `an_incomplete_reading_contributes_the_top_of_the_in_process_range` | same revert | FAILED on the first of seven damage shapes |

Two of the *existing* tests also flip with the fix reverted — the store-level
`{seg: "not a segment record"}` case (`:11341`, was asserting the buggy `21`) and
`the_id_recovery_reads_every_shape_the_bytes_still_spell` — which is the point: they
encoded the defect, and they now encode the property.

**(b) Production path?** Yes. Every new test drives the production functions the fix
changes — `repoint_chunk`, `commit_chunk_map`, `commit_chunk_map_superseding`,
`commit_chunk_map_superseding_leased`, `metadata::high_water_marks` — over a **real**
`RedbMetadataStore::in_memory()`, not a double. `RecoveredIds::contribution` is exercised
both directly and through `high_water_marks`, i.e. through the same call path
`Gateway::recover` uses. No mock, no re-implementation, no parallel copy.

**(c) Fixture includes the fault?** Yes, and in each case the fault is the *subject*: the
version tests seed a record **at `u64::MAX`** (not near it); the ceiling test seeds a root 8
bytes under the ceiling and a second one already **over** it; the floor tests seed the
damaged values in the same store as healthy ones and assert on the store's answer, not on a
curated subset. The leg-A containment test (unchanged this round) still holds the damaged
object in the same store as the healthy flat and segmented ones.

## Gate evidence

* `./engine/xtask.sh ci` (the configured C4-ci gate; fmt + clippy `-D warnings` + build +
  workspace tests + DST + cargo-deny + conformance) — **exit 0**, "xtask ci: all checks
  passed", run against `$PDCA_WORKTREE` after the id-floor and version work; re-run in full
  after the final growth-guard edit. Logs:
  `$PDCA_SCRATCH/pdca-builder-635-redleg/ci{,2}.log` (scratch, removed at the end of the
  run — the second run's tail is quoted in SUMMARY if needed).
* `cargo test -p wyrd-core --lib` — 108 passed, 0 failed.
* `./engine/scripts/run-verify.sh` (the C4-verify gate, run over this bundle's regenerated
  `patch.diff`) — **PASS**. What the brief's `Falsifiability` 3 asks me to record from the
  RED leg: the added target is the one the brief names
  (`cargo test -p wyrd-custodian --test segmented_map_consumers`); **9 tests ran and 9
  failed** with production reverted and the test kept, and the red is **assertions, not a
  build error** — the binary compiled and executed on the pre-fix tree, and every failure is
  the segmented value refusing to decode inside the production path
  (`called Result::unwrap() on an Err value: Error("invalid type: map, expected a sequence",
  line: 1, column: 23)`, e.g. `segmented_map_consumers.rs:811`, `:844`, `:1111`, `:1345`).
  With the fix applied the same 9 pass.
* `cargo fmt --all` run over every touched file (the target's commit hook runs it).

## Still open for the human at sign-off (carried, not resolved by this round)

These are the standing §6 items; none of them is a code defect this round could close.

1. **T3 / `Open questions` 4** — landing a `Completing`-less precursor committer before #636
   supplies the real session fence. The brief takes the fence-as-parameter shape; the
   alternative is moving the root flip into #636. Unchanged this round.
2. **C5 mutants** (advisory) — 13 missed on the round-13 diff. This round deletes the
   arithmetic that several of them lived in (`widest_id_with_prefix`) and adds tests that
   bind the new comparators, so the count should move; I did not re-run `cargo mutants`
   (17 min, and Check re-runs it).
3. **The containment table's blast radius**, re-affirmed not reopened: one damaged object
   stalls a deletion-capable pass (GC/drain). The brief pre-authorises it
   (`Design § Failure containment`, confirmed with the maintainer 2026-07-27).
4. **Fitness-to-purpose of the synthetic fixtures pre-#636** — no production path publishes
   a segmented map until #636 lands; the brief says so explicitly (`Production reach`).
