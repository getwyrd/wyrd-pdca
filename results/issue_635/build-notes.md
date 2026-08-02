# Build notes — issue 635 / segmented-chunk-map (iteration 17)

**Withheld from the reviewer.** Rationale, evidence, and what I ruled out.

## What this iteration is

Iteration 16's patch was green on the two gates that judge the *change* (`C4-ci` — the whole
`cargo xtask ci` — and `C4-verify` — the per-fix red→green) and red on exactly two rows:

* **T4 batched rubric review (gating)** — 3 blocking findings, 0 triaged.
* **C5 surviving mutants (advisory)** — 14 missed mutants on the bundle diff.

So this round is **iteration 16's patch plus the fixes for those two rows** — nothing else was
re-decided, nothing already-settled was re-derived (the brief's carry-forward is explicit that
iteration 16 passed C1/C3/T1/T2 and both C4 rows). The base is unchanged: plain
`origin/main` @ `9120f7a`, which carries #634.

`patch.diff` grew by **+1152 insertions over 9 files** relative to `iteration-v16/patch.diff`
(48 → 50 files; the two new files are `reconciliation.rs` and `scrub.rs`, both pre-existing in
the tree, newly *touched* by this round). Every one of those lines is one of the three fixes,
its test, or a mutant-killing test — verified by a per-file numstat diff of the two patches.

## Environment declarations the brief demands (Falsifiability 2 and 3)

* `$PDCA_BASE` — **not set**. `$PDCA_VERIFY_BASE` — **not set**. No `stack-base` file in the
  bundle. (`env | grep PDCA` shows only `PDCA_FLOW_INHIBITED`, `PDCA_SCRATCH`,
  `PDCA_WORKTREE`.) So build base == test base == PR base, as the brief requires; nothing to
  report under Falsifiability 2.
* **RED leg, from `engine/scripts/run-verify.sh`:** `cargo test -p wyrd-custodian --test
  segmented_map_consumers` with production reverted and the added test kept ran **12 tests, 12
  failed**, and the red is **assertions / runtime `Err`**, not a build error — the failures are
  `Result::unwrap()` on `Err(Error("invalid type: map, expected a sequence"))` and named
  assertion messages (e.g. "one unreadable object must not end evacuation planning for the
  fleet"). Verdict line: `run-verify.sh: PASS — red without the fix, green with it.`
* External dependencies: `typos` (1.48.0) and the doc renderer are installed and both ran
  inside `cargo xtask ci` (`xtask/src/main.rs:1552-1553` runs them first and *fails* rather
  than warn-skips when they are absent). No NEEDS-HUMAN external dependency this round.

## The three T4 findings — each fixed at its root, each with a binding test

### 1. `gc.rs:335` — scrub certified a store it could not read

*Finding:* containing an unresolvable inode in a partial `ReferenceSet` lets `scrub::reconcile`,
which never checks `unresolvable`, skip that object's fragments and return `Satisfied`.

*Why it is real:* an unresolvable object contributes no fragments to the reference set, so scrub
never fetches, checksums or enqueues any of them. `Satisfied` from that pass means "every
referenced fragment was verified and matched" — a clean bill for *part* of the store carrying the
name of one for all of it. That is the rubric's *Absent or unsupported entries* class
(`AGENTS.md:175-177`: never silent success, never a silent skip), and scrub is the one loop whose
`Satisfied` is a **verification claim** rather than a "nothing happened" report.

*Fix:* a third outcome. `Reconciled::Blocked` (`crates/custodian/src/reconciliation.rs:43`) with
the aggregation rule `least_certified` (`:54`), and scrub emits one operator signal per blocked
object before the fleet walk (`crates/custodian/src/scrub.rs:103`, `emit_unscrubbable` at `:214`)
then refuses to certify (`:199`). Emitted *before* the walk so a later transient store fault
cannot cost the operator the attribution; the walk still runs over everything that IS in the set.

*Why not the two cheaper spellings:*
* **Return `Err`.** 1-line change, and wrong: `reconcile_step` short-circuits on the first loop's
  error (`reconciliation.rs:100-125`), so one damaged object would skip reconstruction and
  rebalance for the whole store — the fleet-wide blast radius the brief's containment table
  forbids, and the same failure iteration 5 was rejected for.
* **Keep `Satisfied`, just emit the signal.** Fixes the silent-skip half and leaves the finding's
  actual claim ("incorrectly return `Satisfied`") standing.
* **Cost of the chosen fix, measured:** `Reconciled` has **no exhaustive `match` anywhere in the
  workspace** — 125 uses, all `==`/`assert_eq!` (`grep -rn "Reconciled::" crates/`), and the
  production run loop ignores the value (`crates/server/src/custodian.rs:519`, `:533`, `:610` all
  `Ok(_) => {}`). So the variant costs 30 lines of enum + docs and breaks nothing.
* **Why only scrub gets it:** GC's `Satisfied` means "nothing was reclaimed", which is *true* over
  an incomplete set (its `protects` rule reclaims nothing); reconstruction's refusal is the
  obligation it leaves queued; rebalance's is the drain surface's `PendingUnresolvable`. Scrub was
  the one consumer of the reference set with no refusal of its own. That reasoning is in the
  variant's doc comment so the next round does not re-litigate it.

*Test:* `crates/custodian/tests/scrub.rs:1032` — a store with a rotten fragment on a healthy
object **and** a committed segmented root whose `seg:` records were never written. Asserts
`Blocked` (damaged) vs `Changed` (control, same store minus the damaged object), that the rotten
fragment is enqueued in **both** cases (so the refusal is not an abort), and that
`scrub_unresolvable_records` appears on the Prometheus surface only when damaged.

### 2. `metadata.rs:5162` — a `seg:` value like `{}` contributed **zero** to the id floor

*Finding:* a syntactically valid but structurally undecodable `seg:` value is marked *complete*
with no ids, so `high_water_marks` contributes zero and can under-report chunk ids.

*Why it is real:* `complete` counts id *tokens* the readers could not read — the only loss a token
can show. `{}` shows none: it parses, names no `id`, and reads as a complete reading with an empty
answer. But a `seg:` record **is** a chunk list (an empty one is refused at decode,
`EmptySegmentRecord`), so a `seg:` value naming no id has *lost* its ids while their fragments are
still on disk. That is issue #364's re-mint hazard exactly, and the module's own doc
(`metadata.rs:5127-5140`) already argues the direction: over-approximate, never under.

*Fix:* the missing premise is a fact about the **record class**, not about the bytes, so the class
is now a parameter. `ClassIds::{Required, Optional}` (`metadata.rs:5072`), a `found` counter on
`RecoveredIds` (`:5110`) that counts ids **read at any magnitude**, `contribution(ceiling, class)`
(`:5148`) via `covers` (`:5168`), and the two walks pass their class:
`segment_chunk_floor` → `Required` (`:5612`), the `inode:` walk → `Optional` (`:5753`).

*Why `found` and not the cheaper proxy:* the 1-line proxy is "treat `floor == 0 && unreadable ==
0` as loss" — no new field. It is wrong for a **cluster-mode** object: a damaged `seg:` record
whose ids are all `≥ 2^64` reads perfectly, has `floor == 0`, and would be declared destroyed,
parking the in-process allocator at `2^64 - 1` for a record nothing is wrong with. The `found`
counter distinguishes "no ids here" from "ids above this floor's range" — and the control for it
is asserted (`metadata.rs:12480`, control 1).

*Why `Optional` for `inode:`:* an empty flat map and a segmented root both legitimately name no
chunk id (a segmented root's ids live in its `seg:` records, which the `seg:` walk covers without
the root). Making roots `Required` too would park the whole in-process chunk range on any store
with one damaged root — iteration 5's rejected blast radius in a new spelling. Control 2 of the
same test pins it.

*Test:* `metadata.rs:12480` — the unit reading of `{}` (complete, `found == 0`), its two
contributions (`Required` ⇒ `ceiling - 1`, `Optional` ⇒ 0), and three end-to-end
`high_water_marks` fixtures over a real redb store: `{}` under a `seg:` key ⇒ floor
`2^64 - 1`; a `seg:` record whose ids are cluster-space strings ⇒ floor stays at the healthy
object's id; a damaged **root** naming no id ⇒ floor stays at the healthy object's id.

### 3. `metadata.rs:2874` — a damaged segment bypassed the root re-read

*Finding:* segment decode and bounds failures bypass the root re-read, so a request racing an
overwrite can fail on corruption in the retired generation instead of resolving the healthy
replacement.

*Why it is real:* retirement **deletes** a generation's rows and the space is reused, so a reader
holding a stale root can see a row absent, half-overwritten, or carrying the *replacement's*
extents. Two of those four anomalies went through `retired_or` and two did not — which is the
same defect shape iteration 9 was told to fix "at the foundation, not by whack-a-mole".

*Fix:* `retired_or` is now the **single arbiter for every anomaly the range can show**
(`metadata.rs:2885` for a row that does not decode, `:2920` for a bounds disagreement, plus the
two arms that already used it), with the rule restated in one place at `retired_or`'s doc
(`:2960-2975`) and in `read_segments`'s (`:2847-2853`).

*Deliberately NOT routed through it:* a malformed `seg:` **key** (`read_group_range`'s
`parse_seg_key` / nonce-epoch check). A retirement deletes rows; it never writes a key, so the
root cannot exonerate a key that does not parse — it is a backend-bug class, and it stays a hard
typed fault. Left as-is rather than widened, and the reason is stated so a later round does not
read it as an oversight.

*Tests:* `metadata.rs:10731` (both spellings × both arms: root unchanged ⇒ typed error; root moved
on ⇒ `MapResolution::Retired` and `resolve_live_chunk_map` returns the replacement's chunks and
reports the restart) **and** the interleaving itself in the Tier-0 DST property — a new arm 3 in
`crates/dst/tests/custodian.rs:1577` inside the existing X51 property
(`prop_segmented_resolve_never_tears_on_retirement`), so the concurrent path lands with seeded DST
coverage as the rubric's *Test fidelity* class requires. Verified red: with the fix reverted,
`cargo xtask dst` fails `segmented_resolve_never_tears_on_retirement` (and the regression-seed
test), 11 passed / 2 failed.

*No `review-rejected.md` entry this round:* all three findings are **fixed**, so the triage rule's
other arm (a recorded rejection) does not apply. Nothing was "re-reviewed to silence".

## The 14 surviving mutants (C5, advisory but named at sign-off)

Ten are killed by new assertions; four are **provably equivalent** (no input distinguishes them),
argued below rather than papered over with a contorted test. Every kill was verified by applying
the mutation by hand, running only the relevant test through cargo, and restoring the file —
script kept at `$PDCA_SCRATCH/pdca-builder-635-refute*.py` during the run and deleted after.

Line numbers in the *mutant* column are the sites as the C5 report named them **on iteration 16's
diff**, so they can be checked against `iteration-v16/check-review.md` and `mutants.out/missed.txt`
directly; `metadata.rs` gained ~580 lines of co-located tests this round, so the same code now sits
lower in this patch (the equivalence arguments below quote this patch's lines).

| # | mutant | killed by | verified |
|---|---|---|---|
| 1 | `metadata.rs:2084` guard `span != size` → `true` | `a_segmented_root_fault_names_the_disagreement_it_actually_found` (`metadata.rs:6303`) | RED |
| 2 | `metadata.rs:2385` delete `state` field in `commit_chunk_map` | `the_commit_point_publishes_a_committed_record_over_a_pending_prior` (`:5965`) | RED (`left: Pending, right: Committed`) |
| 3 | `metadata.rs:4065` `written: index + 1` → `* 1` | `the_prefix_reconstruction_reports_the_cursor_each_batch_left` (`:7432`) | RED |
| 4 | `metadata.rs:5198` delete the `Array` arm | `the_parsed_id_reader_corrects_what_the_byte_scan_reads_differently` (`:12420`) | RED |
| 5 | `metadata.rs:5192` `unreadable += 1` → `*= 1` | same | RED |
| 6 | `metadata.rs:5244` `id < ceiling` → `id > ceiling` | same | RED |
| 7 | `backfill.rs:344` `unreadable += 1` → `*= 1` | `the_unreadable_level_counts_a_record_whose_map_cannot_be_resolved` (`tests/backfill_telemetry.rs:586`) | RED |
| 8 | `rebalance.rs:210` `unresolvable += 1` → `*= 1` | `the_evacuation_blind_spot_level_counts_a_record_whose_map_cannot_be_resolved` (`tests/rebalance.rs:1515`) | RED |
| 9 | `reconstruction.rs:236` `unresolvable += 1` → `*= 1` | third case of `an_absent_chunks_obligation_drains_past_an_unreadable_uncommitted_record` (`tests/reconstruction.rs:1837`) | RED |
| 10 | `restore.rs:161` `is_clean -> false` (now `:160`) | `a_clean_restore_reports_clean_and_an_unreadable_object_makes_it_not_clean` (`tests/restore_reconcile.rs:1019`) | RED |

**What 7–9 revealed, and it was a real gap:** all three counters have two arms — the root that
does not **decode** and the root that decodes whose map cannot be **resolved** — feeding one
gauge, and every existing fixture used the decode arm only. The new cases seed a *valid,
committed* segmented root whose `seg:` records were never written, so the resolve arm is the one
that runs. That is the arm a production segmented object actually reaches.

**The four equivalent mutants** (documented, not tested — a test that "kills" them would have to
assert an internal that no behaviour depends on):

* `metadata.rs:3626` `DurableSegments::is_empty -> false` (now `:3655`). The `if` at `:4010` is a
  pure short-circuit: its `else` calls `assemble_segment_batches(pending, &encoded, &ranges,
  durable)` with the same arguments the loop already used at `:3930-3935` with
  `DurableSegments::default()`, and with an empty witness the two are the same function of the same
  inputs. The mutant makes it recompute an identical value.
* `metadata.rs:5431:37` and `:5431:41` (now `:5534`) — `MAX_ID_FIELD_BYTES = 2 + 6 + 6` → `2 * 6 +
  6` / `2 + 6 * 6`. Both **widen** the scan's candidate window. The window bounds a key-token lex,
  and a token that unescapes to `id` is at most 14 bytes (`"` + `i` + `d` + `"`, each letter
  literal or a 6-byte `\u00XX`; JSON has no other escape for an ASCII letter), so a token the
  14-byte window closes is closed identically by an 18- or 38-byte one, and any token *longer* than
  14 bytes cannot unescape to `id` and is skipped by the name check either way.
* `metadata.rs:5454` (now `:5557`) delete the `b'\\' => at += 2` arm in `json_string_token`
  (`:5549`). The two-byte skip
  differs from byte-at-a-time only when the escape *contains a quote* (`\"`); `\\` lands on the
  same closing quote either way. For the mutant to lose an id the token would have to unescape to
  `id` **and** contain `\"` — impossible, `id` has no quote. For it to gain one, the early-closed
  slice ends in `\` + `"`, which `serde_json` rejects as an unterminated string ⇒ `None`. So no
  input distinguishes them.

The four `timeout` rows (`:5343`, `:5454:25`, `:5461:21` ×2) are `+=` → `-=`/`*=` mutations that
make the scan's cursor stop advancing, i.e. non-terminating — cargo-mutants classes them as
timeouts, not misses, and the loop's termination is already asserted structurally
(`scavenged_chunk_id_floor` is driven by `for at in 0..value.len()`).

## Forced self-refutation (recorded per the Do beat's rules)

**(a) Genuine red?** Yes, and measured three ways, each with the fix actually reverted and
re-run:
1. The brief's binding leg A through the project's own verifier: `run-verify.sh` ⇒ 12 tests ran,
   12 failed by assertion/runtime `Err` on the base, all 12 green with the patch.
2. Each of the three T4 fixes reverted individually ⇒ its own new test fails
   (`scrub_refuses_to_certify…`, `a_class_that_must_name…`, `a_damaged_row_of_a_retired…`), and
   for finding 3 also the Tier-0 DST property under `cargo xtask dst`.
3. Each of the ten mutant kills re-applied by hand ⇒ RED (table above).

**(b) Production path?** Yes. Nothing new is a stand-in. The scrub test drives the real
`reconcile_step` → `scrub::reconcile` → `gc::referenced_fragments`; the floor tests drive the real
`metadata::high_water_marks` over a real `RedbMetadataStore::in_memory()`; the resolve tests drive
the real `resolve_chunk_map` / `resolve_live_chunk_map`; the three telemetry tests drive the real
`backfill::reconcile` / `rebalance` / `reconstruction` passes through the real
`DurabilityTelemetry` Prometheus surface. The only doubles are the in-memory `MetadataStore` /
`ChunkStore` seams the suites already use (each implementing `scan_page` via
`wyrd_testkit::test_double_scan_page`, mirroring `crates/custodian/tests/gc.rs:73-80`), and no
`Cargo.toml` was touched.

**(c) Fixture includes the fault?** Yes, and this is where the earlier rounds' weakness was. The
damaged object is *in the store the assertion reads*, never curated out: the scrub fixture keeps
the rotten fragment **and** the unresolvable object in one store and asserts both outcomes; the
floor fixtures keep the healthy object beside the damaged record so the assertion distinguishes
"floor parked" from "floor at the healthy id" rather than measuring an empty store; the three
telemetry fixtures keep a healthy object that must still drain/evacuate/repair past the damaged
one; the DST arm mutates the live store mid-resolve at a seeded victim index. The two controls in
the floor test (a cluster-mode damaged record; a damaged root) exist precisely so a blunt
"damaged ⇒ park the allocator" implementation fails.

## Carried forward — still NEEDS-HUMAN, not resolved by this round

Unchanged from iteration 16's §6; none of them is an implementation gap this round could close:

* **T3 / Open question 4** — landing a `Completing`-less precursor committer before #636 supplies
  the real session fence (the brief pre-declares this as a cheap confirm).
* **Validation** — whether raw-seeded in-memory fixtures are fitness-for-purpose for a precursor
  slice with no production producer of a segmented map (0016 forbids one until #636).
* **T3 runtime cost** — one root `get` per committed object per maintenance pass.
* The `seggrp:` marker is written by no code path in this slice (it is #636's Create batch); this
  slice ships the record, the key helper and the emptiness predicate, per the brief's
  `Design § The one editorial contradiction`.

## Scratch discipline

Everything throwaway lived under `$PDCA_SCRATCH` (`/var/tmp/pdca/pdca-builder-635-*.py`,
`/var/tmp/pdca/{old,new}*.txt`) and is deleted. No `/tmp` paths were used; the only build
directory is the worktree's own pre-existing `target/`.
