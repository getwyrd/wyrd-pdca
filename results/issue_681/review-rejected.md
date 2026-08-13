# Recorded-rejected review findings — issue 681
#
# Format (ONE physical line per rejection, as `scripts/review-branch` parses it):
#   `<file:line>` | <CLASS> | <MATCH> | <reason>
# MATCH is a case-insensitive substring of the finding's own rationale, so a *different* defect
# landing at the same line still blocks. `loc` must match EXACTLY, so a finding whose code moved in
# this rebuild is recorded at BOTH its old line and the line the equivalent code now sits on.
# Everything else from the previous round's `review-batch.md` was fixed in code — see
# `build-notes.md` §1.

# ---------------------------------------------------------------------------------------------
# Round 7 (this attempt). TWO of the three T4-blocking findings are recorded-rejected rather than
# fixed, because BOTH ask for the same behaviour change — "do not repair / do not evacuate while
# any object is unreadable" — which is (a) the defect this slice exists to remove, pinned against
# by the brief, and (b) aimed at a data-loss chain that the tree already closes at the GC seam.
# The third (backfill.rs:190, the stale gauge on a CAS conflict) IS fixed in code
# (`crates/custodian/src/backfill.rs:192-201`, bound by
# `crates/custodian/tests/segmented_map_passes.rs:626-637`).
# ---------------------------------------------------------------------------------------------

`crates/custodian/src/rebalance.rs:130` | BUG | orphan-marks for GC | Not reachable, and the change it asks for re-introduces the defect this slice removes. (1) The loss chain cannot close: the evacuation does NOT delete the source fragment, it orphan-MARKS it (`crates/custodian/src/rebalance.rs:425-430`), and GC reclaims a marked fragment only through `ReferenceSet::protection` (`crates/custodian/src/gc.rs:306-316`, consulted before every delete at `:191-194`) — which withholds EVERY fragment in the fleet while any object is unreadable (`incomplete-reference-set`) and withholds one that ANY readable committed map still places (`referenced`). So the object this scan could not read either keeps its fragment (still unreadable ⇒ GC reclaims nothing) or, once readable, protects it; if it ceased to exist, the reclaim is correct. (2) Withholding the planned moves while `refused > 0` is exactly "one damaged record costs every healthy object its evacuation", the C-1 violation this slice restores the invariant against (`brief.md:160-175`), and it is pinned against by leg 3, which REQUIRES the healthy object's evacuation and fill to still happen beside two unreadable objects (`brief.md:96-101`). What an incomplete scan changes is the ANSWER — `Blocked`, so no drain is certified — which is asserted at `crates/custodian/tests/segmented_map_passes.rs:562`. The reasoning is now in the code at `crates/custodian/src/rebalance.rs:132-140` so the next reader does not have to re-derive it.

`crates/custodian/src/rebalance.rs:141` | BUG | orphan-marks for GC | Same finding, recorded at the line the evacuation loop now occupies after this rebuild — see the entry for `rebalance.rs:130`.

`crates/custodian/src/rebalance.rs:142` | BUG | orphan-marks for GC | Same finding, recorded at the `evacuate_chunk` call itself — see the entry for `rebalance.rs:130`.

`crates/custodian/src/reconstruction.rs:383` | BUG | orphan-mark a fragment still referenced by the hidden object | Not reachable, and the change it asks for re-introduces the defect this slice removes. (1) The loss chain cannot close: `repair_chunk` orphan-MARKS the displaced fragment (`crates/custodian/src/reconstruction.rs:665-671`), it never deletes it, and GC reclaims a marked fragment only through `ReferenceSet::protection` (`crates/custodian/src/gc.rs:306-316`, consulted before every delete at `:191-194`) — which withholds EVERY fragment while any object is unreadable and withholds one that ANY readable committed map still places. A hidden duplicate reference therefore protects its own fragment the moment it becomes readable, and nothing is reclaimed while it is not. The displaced fragment is also, by construction, one the assessment proved MISSING or checksum-failing at its placed server (`crates/custodian/src/reconstruction.rs:467-486`). (2) Refusing `Repairable` whenever `index.unaccounted > 0` is "one damaged record costs every healthy object its repair" — the C-1 violation this slice exists to remove (`brief.md:160-175`) — and it is pinned against by leg 3, which REQUIRES the healthy object's repair to still happen beside two unreadable objects (`brief.md:96-101`), and by pinned decision 2 (`brief.md:243-247`), which settles that an incomplete reading changes what the pass may CLAIM (`Blocked`) and what it may DISCARD (never an obligation — `REFUSED_INCOMPLETE`), not what it may do. Both are asserted at `crates/custodian/tests/segmented_map_passes.rs:592-621`. The reasoning is now in the code at `crates/custodian/src/reconstruction.rs:292-300`.

`crates/custodian/src/reconstruction.rs:210` | BUG | orphan-mark a fragment still referenced by the hidden object | Same finding, recorded at the line the `Assessment::Repairable` arm now occupies after this rebuild — see the entry for `reconstruction.rs:383`.

`crates/custodian/src/reconstruction.rs:302` | BUG | orphan-mark a fragment still referenced by the hidden object | Same finding, recorded at the repair loop that executes the plans — see the entry for `reconstruction.rs:383`.

`crates/custodian/src/reconstruction.rs:387` | BUG | orphan-mark a fragment still referenced by the hidden object | Same finding, recorded at `assess`'s site lookup, where the `Repairable` classification is decided — see the entry for `reconstruction.rs:383`.

# ---------------------------------------------------------------------------------------------
# Carried forward from earlier rounds — still standing, re-checked at this rebuild.
# ---------------------------------------------------------------------------------------------

`crates/custodian/tests/segmented_map_passes.rs:1` | TEST-GAP | seeded Tier-0 DST coverage | Settled at Plan as recorded-rejected (`brief.md:349-360`) and re-checked at Do: this slice introduces no new destructive or concurrent path. Every write it performs is on a FLAT object and keeps its existing version-conditional CAS on the scan snapshot, byte-for-byte the behaviour on the base — a flat map resolves to a borrow with no store read and no supersede check (`crates/core/src/metadata.rs:2584-2586`), so the resolver adds no new race to any path that commits. What the slice adds on the segmented side is refusal, which writes nothing at all; the seeded Tier-0 case for the segmented WRITE path belongs to #682, which builds it. All three passes branch on the SNAPSHOT's shape before any write (`crates/custodian/src/backfill.rs:156`, `crates/custodian/src/rebalance.rs:265`, `crates/custodian/src/reconstruction.rs:825`), so no commit in this slice is reachable through a restarted resolve. The brief also states in as many words that no leg may be built for that path (`brief.md:264-268`).

`crates/custodian/src/backfill.rs:139` | BUG | a malformed chunk with an empty placement | Not reachable: a chunk is classified malformed ONLY when its `placement` is non-empty. `ChunkRef::placement_is_valid` is `self.placement.is_empty() || self.placement.len() == self.fragment_count() as usize` (`crates/core/src/metadata.rs:204-206`), so an empty vector is always `Ok(_)`, never `Err(MalformedPlacement)`. "Malformed AND empty" is an empty set, and the removed population scan counted `chunk.placement.is_empty()` — which for a malformed chunk was already 0. The gauge is now asserted BY VALUE in the discriminator, beside the number of objects the pass could not draw it over, on the same sample (`crates/custodian/tests/segmented_map_passes.rs:175-186`, `:532`, `:604`, `:637`), so a real drift in this number is caught rather than argued about.
