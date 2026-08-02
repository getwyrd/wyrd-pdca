# Build notes — issue 635 / segmented-chunk-map (iteration 9)

> Withheld from the reviewer; written for the human at sign-off.

## 0. What this iteration is

Iteration 8's bundle passed C1/C3/T1/T2, `cargo xtask ci` and C4-verify. It was rejected on
**T4 batch review**: six blocking findings (four distinct bugs), none fixed or
record-rejected. This iteration is **iteration 8's patch plus five fixes and their tests**,
not a rebuild — the design (encoding, resolver, staged committer, consumer routing) is
unchanged and was not re-litigated. Every fix below is proven red→green by reverting it.

Base: `origin/main` @ `9120f7a` (carries #634's `scan_page`). `$PDCA_BASE` and
`$PDCA_VERIFY_BASE` are **unset** and there is no `stack-base` file in the bundle — checked,
as the brief's `Falsifiability` 2 demands.

## 1. The four `review-batch.md` bugs (round 8), each fixed

### (1) `metadata.rs:3200` — the flip published over a range it never checked

`flip()` is a **public** phase-2 entry point (the recovery shape drives the phases
separately). It called `verify_durable_range`, whose presence check was `planned[..claimed]`
with `claimed == resume_from` — so at the ordinary `resume_from == 0` it read **no planned
record at all** and an empty range passed. The published root is live, so the object is
`SegmentAbsent` for ever at every read/GC/restore/rebalance pass, with no prior generation to
fall back to and no crashed attempt for a rollback to find.

Fix: the depth is now the caller's question, not a constant —
`DurableRange::{ResumePrefix,WholePlan}` (`crates/core/src/metadata.rs:2814`), threaded
through `verify_durable_range` (`:3226`). `flip` (`:3346`) requires the **whole plan** and
refuses with the new `ChunkMapError::PublicationIncomplete` (`:547`), which names the first
planned index the store does not hold; `write_segments` (`:3160`) still requires only the
claimed prefix, since it is about to write the rest.

`publish` (`:3381`) deliberately keeps the **prefix** check and does *not* re-read the range
before its flip: it verifies the claimed prefix before the first write, writes every
remaining segment itself, and reaches the flip only on `Committed`, so the whole plan is
durable at the publication instant by construction. A second range read there would cost a
round trip per page to confirm what the store just acknowledged, and the fence is what
protects the window (a rollback that deleted these segments must first move the fence record
that every batch and the flip pin, which turns the flip into a `Conflict`). That asymmetry
is documented at `:3381-3392` so it reads as a decision rather than an omission.

*Rejected alternative*: making `flip` re-derive and re-commit the missing segments itself.
That turns the publication instant into a write phase and hides a lost fence — the caller
must learn phase 1 did not finish.

### (2) `metadata.rs:3455` — "some put differs" was not a fence transition

`check_fence_transitioned` accepted a flip if **any** put on the fence key differed from the
pinned values. A contribution that writes the fence twice — to the terminal value and then
back to the pinned one — passed, and a `WriteBatch` states **no apply order within one key**
(`crates/traits/src/lib.rs:1178-1186`), so the durable fence is backend-dependent: on the
backend where the restoring put lands last, the racing rollback's `require(mpu ==
Completing@E)` is still true *after* the flip and it deletes the published generation's
segments.

Fix (`crates/core/src/metadata.rs:3562`): **every** put on the fence key must differ from
every value the batch pins there; a delete still counts as the strongest transition. An
honest contribution never writes its own fence twice, so refusing the ambiguity costs a
caller nothing — and accepting it would make the publication instant depend on which backend
applied the batch.

### (3) `metadata.rs:3757` — the id scan was blind to JSON escapes

The scavenging reader matched the four literal bytes `"id"`. The *parse* reader keeps only
the **last** duplicate of a key. Compose the two blind spots and there is exactly one
arrangement of bytes that defeats both: `{"\u0069d":999,"id":3}` — the parse sees 3, the
literal scan sees 3, and 999 (an id whose fragments are on disk) never reaches the allocator
floor. The next restart re-mints it over live fragments (issue #364).

Fix (`:3898`): at every quote the scan lexes a **bounded** JSON string token and compares
what it *unescapes to*, with `serde_json` doing the unescaping (`json_string_token`, `:3987`)
— so there is no second string grammar in this module (the rubric's *prefer extending a
shared parser*). Two bounds keep it linear and keep it from regressing:
`MAX_ID_FIELD_BYTES = 14` (`:3963`) is the widest spelling of the key `id`, and
`MAX_ID_VALUE_BYTES = 236` (`:3972`) the widest spelling of a quoted `u128`. The scan is
**non-consuming** (each quote is a candidate, no token is swallowed), which matters: a
consuming tokenizer would lose ids the old literal scan found in damaged, unbalanced bytes —
an under-report in the one direction this reader may not err in.

*Rejected alternative*: a duplicate-preserving `serde` visitor replacing the `Value` parse
(~110 lines of `Deserializer`/`Visitor` code covering every JSON type). It closes the same
hole for *valid* JSON only, adds the larger mutation surface, and still needs the byte scan
for bytes that are not JSON. The escape-aware lex is ~35 lines including the two bounds and
covers both populations.

### (4) `metadata.rs:3834` — the truncated-token widening under-reported at the ceiling

`widest_id_with_prefix` appended nines and stopped at the first candidate past the ceiling —
which throws away a whole width-range when only its *top* is out of bounds. At the real
`2^64` ceiling, prefix `18` was worth `1_899_999_999_999_999_999` while ids up to
`18446744073709551615` start with those digits; prefix `1` likewise. A floor a factor of ten
below live ids is the re-mint hazard the function exists to prevent.

Fix (`:4037`): walk the ranges a prefix admits (`[p·10^k, p·10^k + 10^k − 1]`), **cap** each
at `ceiling − 1` rather than skipping it, and stop only when the range's *lowest* member is
out of bounds. One case is called out in code: a token whose first digit is `0` stays the
whole id, because no id renders with a leading zero — without it the loop would widen `0`
into `ceiling − 1` and park the allocator at the top of its range.

The T5 §6 item asked for an **independent oracle**, and that is
`the_truncated_token_widening_matches_an_independent_prefix_oracle` (`:8141`): it enumerates
the ids below a ceiling and filters by decimal-string prefix (the definition, sharing no
arithmetic with the function), exhaustively over every prefix and ceiling below 300, then
pins the real `2^64` boundaries the small space cannot reach. The previous suite asserted the
buggy numbers back at the buggy rule, which is how this survived four rounds.

## 2. The adversary's `[impl]` finding, also fixed — the containment table's
   `reconciliation_status` row

With one damaged object seeded, `reconciliation_status` returned **`Err` for every D server
in the store**: `referenced_fragments` propagated the resolver's error with `?`. That is the
store-wide blast radius the brief's containment table exists to prevent — the row asks for
the `PendingMalformed` shape (refuse to certify, **attribute**, keep going), and "`Err` is
not `Satisfied`" is exactly the rationalisation that let it slip.

Fix, in three parts:

* `referenced_fragments` (`crates/custodian/src/gc.rs:292`) contains a `ChunkMapError` per
  object into `ReferenceSet::unresolvable` (`:250`) and attributes it on the audit seam
  (`emit_unresolvable`, `:368`) instead of ending the walk. Anything that is **not** the
  object's own fault (a store error, an undecodable record) still propagates.
* `ReferenceSet::protects` (`:268`) treats an incomplete set as protecting **everything**.
  This is the structural half: an unresolvable map hides *which chunks the object owns*, so
  no fragment can be shown not to be one of them, and every deletion-capable pass already
  gates on this one predicate (`gc.rs:162`, `restore.rs:222`) — so the containment holds for
  all of them or for none, rather than depending on each caller remembering. The brief
  permits exactly this shape ("continuing while treating the damaged object as fully
  referenced is also acceptable"). The cost is a leak until the object is repaired.
* `reconciliation_status` (`crates/custodian/src/desired_state.rs:167`) answers the new
  `ReconciliationStatus::PendingUnresolvable { objects }` (`:114`) — never `Err`, never
  `Satisfied`, and naming the objects to repair. A server that genuinely holds a referenced
  fragment still gets the ordinary `Pending`.

Attribution is by **rendered inode key** (`inode:<id>`) rather than a parsed id: it is what
the audit seam emits, and a key that failed to parse would otherwise drop a blocker.

## 3. The three answers the Do beat must record

**(a) Genuine red?** Yes — each fix was reverted in the worktree and the test re-run:

| Fix | Revert applied | Result |
|---|---|---|
| flip whole-plan | `DurableRange::WholePlan => claimed` | `the_flip_refuses_a_range_that_is_not_the_whole_plan` FAILED — *"a flip over an empty range must refuse: Committed"* |
| fence transition | restore `.any(|(_, v)| !pinned.contains(&v))` | `the_flip_must_move_its_fence_record_not_merely_pin_it` FAILED — `unwrap_err()` on an `Ok` |
| escape-aware scan | restore the literal `"id"` window match | `each_id_reader_recovers_what_the_other_cannot` FAILED — `left: 3, right: 999` |
| ceiling widening | restore the all-nines walk | `the_id_recovery_reads_every_shape…` and `each_id_reader…` FAILED (`1899999999999999999` vs `18446744073709551615`); `the_truncated_token_widening_matches_an_independent_prefix_oracle` FAILED at the small-space boundary |
| containment | restore `chunks_of(...).await?` | `a_drain_stays_blocked_and_attributed_while_a_map_cannot_be_resolved` FAILED (`Err(SegmentAbsent)`) and leg A's `a_damaged_segmented_object_never_costs_the_store_its_other_objects` FAILED — *"one damaged object must not blank the drain surface for the fleet"* |

And the whole-bundle leg: **C4-verify PASS** — `cargo test -p wyrd-custodian --test
segmented_map_consumers` is **9 passed / 0 failed** with the fix and **0 passed / 9 failed**
with production reverted, and every red-leg failure is a `panicked at
crates/custodian/tests/segmented_map_consumers.rs:<line>` — **assertions, not a build
error**, which is what the brief's `Falsifiability` 3 requires the builder to state.

**(b) Production path?** Yes. Leg A drives the real `reconcile_step`,
`reconcile_after_restore`, `reconciliation_status`, `metadata::high_water_marks`,
`wyrd_core::read` and `backfill::reconcile` over in-memory trait doubles of the *stores*
only; the code under test is production. Leg B drives the real `SegmentedPublication` against
a real `RedbMetadataStore::in_memory()`. The containment fix is asserted through
`reconciliation_status` itself, not a copy of it.

**(c) Fixture includes the fault?** Yes. The damaged object is *in the same store* as the
healthy flat and healthy segmented objects for every assertion (id floor, both reads, the
typed per-object failure, the fragment counts, and now the drain surface) — nothing is
curated out. The flip legs assert over a store whose `seg:` range is genuinely empty or
genuinely partial (the partial one commits `batches[..1]` for real and reads back what is
durable), and the fence legs drive the real `flip_batch` assembly.

## 4. NEEDS-HUMAN — carried forward explicitly, per the iteration-8 sign-off

Two of the four §6 items are **fixed** above (the `flip()` range; the containment-table
`reconciliation_status` row). The rest are human calls, stated here with a recommendation
rather than carried silently:

1. **T3 — landing a `Completing`-less committer before #636.** Unchanged and unresolvable by
   me: the brief takes the fence-as-parameter shape (`Open questions` 4) so this slice can
   land before the session exists. *Recommendation: accept.* The alternative (fold the root
   flip into #636) leaves this slice the encoding + resolver only, which is a legitimate
   choice — but it is the maintainer's, and it would discard leg B(iii)–(v).
2. **`high_water_marks`'s `max_chunk` half is unreachable from production, and its stated
   #364 justification is stale on this tree.** Verified: `Gateway::recover` discards it
   (`crates/server/src/lib.rs:124`, `let (max_inode, _max_chunk) = …`) and chunk ids are
   minted ≥ 2^127 from a random per-gateway epoch (`:238`), so no live allocator can reach
   the hazard. I did **not** change it, because the brief pins the behaviour (leg A(vii)(a)
   and the containment table's `high_water_marks` row) and a builder may not overrule a
   settled brief. *Recommendation:* if the maintainer agrees the chunk floor is dead weight,
   the cheapest follow-up is to drop `segment_chunk_floor` from the startup path (one call
   site, `crates/core/src/metadata.rs:4233`) and keep the byte-level recovery for the
   `inode:` half — but that is a brief change, and it should be a tracked issue rather than
   an in-PR edit.
3. **Maintenance passes now issue one root `get` per committed object per pass** (and
   reconstruction up to Q×N over a queue of Q obligations). Deliberate: it is round 7's fix
   for "a flat map is returned without re-reading the root, so a stale scan can miss the live
   generation" — the snapshot *is* the map for a flat record, so nothing but a re-read can
   notice a supersede. Scoping the re-read to segmented roots (the adversary's suggestion)
   re-opens exactly that finding for flat objects, so I declined it rather than trade one
   review round's fix for another's. *Recommendation:* if the round-trip cost is judged
   unacceptable on a networked backend, the honest fix is a design change (resolve lazily at
   the point of decision, or re-validate only at delete time) and belongs to #637, not here.
4. **Validation — fitness for the >10 GiB purpose.** Unchanged: no production path publishes
   a segmented map until #636, which the brief states is correct
   (`Production reach`, `0016:2287-2299`). The evidence here is raw-record and redb, by
   design.

## 5. What I did **not** touch

* No `Cargo.toml` edits (the brief forbids them; C4-verify reverts them on the RED leg and
  that would destroy leg A's assertion red).
* No new added `tests/*.rs` file: the flip, fence and id-floor tests are co-located
  `#[cfg(test)]` units in `crates/core/src/metadata.rs`, and the typed containment test went
  into the **existing** `crates/custodian/tests/rebalance.rs` beside its `PendingMalformed`
  peer. Leg A's file gained only assertions naming base symbols (`reconciliation_status`,
  `ReconciliationStatus::Satisfied`, `set_lifecycle`) — the typed variant is asserted where
  it may be named, and leg A checks the same property through the answer's rendering.
* No ADR/spec/proposal edits. `docs/design/architecture/{06,08}` gained the two clauses this
  round's behaviour changes require (the flip's completeness refusal; the report-only
  surface that refuses to certify and attributes) — docs currency is a merge requirement
  (`AGENTS.md:154-157`).
* The five round-5 destination-fence findings stay **record-rejected** (`review-rejected.md`,
  round 5 and the re-pinned rows in round 8), per the brief's carry-forward item 5 and the
  maintainer's Plan confirmation.

## 6. Self-review against the target's standing rubric (`AGENTS.md` §Review rubric)

* *Serialization identity* — untouched by this round; leg B(i) still asserts the legacy
  decode→encode byte identity, and no encoding changed.
* *Absent or unsupported entries* — the two new refusals are typed errors
  (`PublicationIncomplete`, and the `FenceNotTransitioned` widening), and the containment
  path **attributes** on the audit seam plus a counter rather than skipping silently.
* *Metadata validation boundaries* — the new checks are at the publication boundary (before
  any durable write) and at decode, not in a consumer's opinion.
* *Test fidelity* — the DST leg is unchanged and still passes; the new tests use the same
  in-memory trait doubles the existing suite uses, including `scan_page` delegating to
  `wyrd_testkit::test_double_scan_page`.
* *Docs currency* — the two architecture clauses above, in this PR.
* `cargo fmt --all` run; `cargo xtask ci` green end to end (fmt, clippy `-D warnings`,
  build, test, deny, conformance, and the prose gates — `typos` and the doc renderer are both
  installed on this host).

## 7. Gate evidence on the exact tree `patch.diff` carries

Both re-run **after** the last edit, so they measure the artifact and not an earlier one:

* `./engine/xtask.sh ci` (i.e. `cargo xtask ci` in `$PDCA_WORKTREE`) — **exit 0**, "all checks
  passed" (`${PDCA_SCRATCH}/pdca-builder-635-ci4.log`).
* `./engine/scripts/run-verify.sh` (C4-verify) — **PASS**
  (`${PDCA_SCRATCH}/pdca-builder-635-verify3.log`): GREEN leg **9 passed / 0 failed**, RED leg
  **0 passed / 9 failed**, every red-leg failure a panic inside
  `crates/custodian/tests/segmented_map_consumers.rs` — assertions, not a build error.
* `cargo fmt --all` clean, and `patch.diff` was regenerated from the formatted tree
  (`git diff` byte-compared against the working tree afterwards).

## 8. Scratch

`${PDCA_SCRATCH}/pdca-builder-635-refute/` (pre-refutation file copies) and
`${PDCA_SCRATCH}/pdca-builder-635-v8tree/` (a detached worktree carrying iteration 8's patch,
used to read this iteration's own delta) were created and **removed**. The two gate logs named
above are left in `${PDCA_SCRATCH}` for the human; nothing was written under `/tmp`.
