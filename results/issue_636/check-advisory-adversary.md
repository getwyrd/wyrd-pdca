# Adversarial review — issue 636 (multipart commit protocol)

Advisory only; I never gate. Toolchain was available: I rebuilt the bundle in a scratch copy and
ran `cargo test -p wyrd-core` (138 lib + `multipart_protocol` 36 + `multipart_admission_and_drain` 6,
all green), then attacked the fix with targeted mutations and one purpose-built probe against the
**production** DELETE path. Two attempts landed; several did not.

## Findings

- NEEDS-HUMAN [human] — **The ordinary DELETE/overwrite path stops reclaiming space when this slice
  merges, and nothing in the shipped system ever un-stops it.** `crates/core/src/metadata.rs:3233`
  (`unlink` now calls `retire_generation`) and `crates/core/src/metadata.rs:714-728`
  (`generation_retires_inline`: inline only while `marks * 2 <= MAX_BATCH_OPS/2`, i.e. **≤ 250
  fragments**) route every larger generation to a `retire:bytes:g:<inode>:<version>` obligation —
  but no production code drains one. `drain_obligation` / `drain_step` have callers **only in
  tests** (grep across `crates/*/src`: zero); the loop that would call them is #625, explicitly out
  of scope. Concrete failing case, run in-process against the pristine bundle (probe:
  `metadata::create` a 30-chunk RS(6,3) object = 270 fragments ≈ 30 MiB at the deployed defaults
  `crates/server/src/lib.rs:49,51`; `metadata::unlink`; then `custodian::reconcile_step` at
  t=10,000,000 with a 50 ms grace):
  `orphan marks: 0 | retire obligations: 1 (retire:bytes:g:7:1) | fragments resident after GC: 270`.
  Pre-patch the same `unlink` wrote all 270 `orphan:` marks and GC reclaimed them, so this is a
  regression of a live path (`Gateway::delete_object`, `crates/server/src/lib.rs:582`, and both
  superseding committers at `:3349`,`:3407`), not new-protocol behaviour. Two claims are therefore
  unwarranted as written: `docs/design/architecture/08-crosscutting-concepts.md:85` ("a background
  drain writes the evidence in bounded batches … reclamation is if anything slightly delayed" —
  there is no background drain until #625, so it is suspended indefinitely, not delayed) and the
  brief's *Impact & compatibility* → "Client-visible: nothing" (an operator's disk fills; deleting a
  30 MiB object frees nothing). The brief carries a merge-order obligation only for **#508**
  ("#625/#633 with-or-before #508"); this regression lands with **#636** on paths #508 never
  touches, so that obligation does not cover it. The human owns the call: gate this slice's merge on
  #625, ship a caller here, or raise the inline ceiling for flat generations until the reaper exists.
  Note the bundle's own H2 oracle cannot see this — `FragmentClass::ObligatedForRetirement`
  (`crates/core/src/multipart.rs:5364`, applied at `:5587`) counts an installed, undrained
  obligation as a *safe* class, so `assert_no_classification_gap` reports "sound" on the leaked
  state, and `an_ordinary_overwrite_of_a_large_generation_retires_it_through_an_obligation`
  (`crates/core/tests/multipart_protocol.rs:1784`) only produces the evidence because the **test
  itself** calls `mpu::drain_obligation` at `:1852`.

- NEEDS-HUMAN [impl] — **The drain fence and the draining-server exclusion are not tested at all;
  both mutants survive the whole suite.** `crates/core/src/write.rs:828-830` (the per-server
  `require_absent(desired:dserver:<S>)`) and `:802` (`topology.excluding(&draining)`) are named
  in the brief's *In scope* and in `stage_intent`'s own doc as load-bearing (items 4 and 1). I
  deleted the entire `require_absent` loop → `cargo test -p wyrd-core` **green** (0 failures); I
  separately replaced `topology.excluding(&draining)` with `topology.clone()` → **green** again. No
  test anywhere writes a `desired:dserver:` key and then stages a part (grep over `crates/**`,
  including `crates/dst/tests/concurrency.rs`, whose three cases are publication CAS, slot cap and
  two drainers). Consequently the "re-plan on failure, not a refusal" behaviour
  (`MAX_STAGE_REPLANS`, `write.rs:727,853-860`) and `draining_servers`' fail-closed malformed-key
  arm (`write.rs:731-753`) are unexercised too. A part staged onto a server the operator is
  draining, with `reconciliation_status(S)` free to answer `Satisfied` while fragments are in
  flight, is exactly the hole `0016:837-857` describes — and the bundle would not go red for it.
  Buildable fix: one test that sets `desired:dserver:<S>` (a) before `upload_part` and asserts the
  placement avoids `S`, and (b) between the topology read and the commit and asserts the re-plan.

- NEEDS-HUMAN [impl] — **Stale in-code contract on the changed path.** `crates/server/src/lib.rs:567-576`
  still documents DELETE as "`metadata::unlink` writes an orphan grace record for each fragment in
  the *same atomic batch* … a crash never strands the bytes either." After this patch that is false
  for every object above the inline ceiling (finding 1). The patch updated
  `docs/design/architecture/{06,08}` but not the doc comment on the caller whose behaviour changed
  (AGENTS.md *Docs currency* / *Serialization identity* neighbourhood, `AGENTS.md:154-157`).

- NEEDS-HUMAN [human] — **A knob retune permanently wedges `CreateMultipartUpload`, with no path in
  this slice to unwedge it.** `mpuctl.profile` is written once by the bootstrap arm
  (`crates/core/src/multipart.rs:2145-2148`) and every later mutation preserves it
  (`..admission` at `:5266`, and the create CAS at `:2145-2148`); a process whose compiled
  `Budget::deployed()` differs refuses with `Refusal::ProfileSkew` (the create-path check at `:2070`). Concrete case: a later build changes `W_REF`, `MAX_CHUNKREF_BYTES` or `MAX_VALUE_BYTES`
  (the last two *derive* `MAX_PART_CHUNKS`, `multipart.rs:112-142`) — every create then fails for
  ever, including after `count` returns to 0, because nothing deletes or rewrites `mpuctl`. `0016:348`
  names the remedy ("an explicit operator CAS gated on the live population having drained"), but no
  such path is in this slice and none of #508/#625/#633 is stated to own it. This may be a legitimate
  decline-with-issue-reference (operator surface, out of scope) — that is the human's call, not a
  build defect.

## Attempted refutations that failed (the fix held)

- *Leg B is not vacuous.* I mutated `publish_fenced` to assemble the map from **every** staged part
  while keeping the ETag over the named subset (`crates/core/src/multipart.rs:3792-3800`) — exactly
  the wrong-bytes-with-a-right-ETag shape the brief warns about.
  `b_subset_complete_publishes_only_named_parts_and_evidences_the_rest` went red.
- *The `PendingEntry` round-trip is real.* Dropping `skip_serializing_if` on `owner`/`staged`
  (`crates/core/src/metadata.rs:2744-2748`) reddened
  `multipart::tests::pending_entry_round_trips_byte_identically_in_both_shapes`
  (`crates/core/src/multipart.rs:5775`) — the "`owner:null` ⇒ permanent Conflict" class is pinned.
- *The retirement routing boundary is pinned.* Forcing `generation_retires_inline` to `true`
  reddened both `an_ordinary_overwrite_of_a_large_generation_retires_it_through_an_obligation` and
  `a_stale_orphan_mark_is_restamped_by_the_event_that_unreferences_it`.
- *Leg F is not a sequential test in a concurrency costume.* `multipart_admission_and_drain.rs`
  asserts pairwise-distinct ids, exactly N `mpu:` records, `mpuctl.count == N` **and**
  `meta.commits() > creators + 1` (real CAS contention). I could not construct a passing-for-the-
  wrong-reason path through it.
- I could not find a wrong answer in `complete`'s verb×state table, in `tombstone_answer`'s
  fingerprint comparison (length-prefixed, injective), or in `terminal_delete`'s exactly-once
  decrement (`checked_sub`, fenced on exact session bytes and on `mpuctl`'s whole value).
- On the standing C2 doubt ("the red was a build error, so the new assertions may not be load-
  bearing"): the four mutations above are independent evidence that at least legs B, the routing
  boundary and the serialization-identity rule *are* load-bearing. That doubt should not be the
  reason to reject; findings 1 and 2 should be weighed instead.

## Notes on the gates

- C5 (`cargo mutants --in-diff`) is `unverifiable` — it timed out. Findings 2's two survivors are
  exactly what that gate would have reported, so treat the missing C5 verdict as material here, not
  as a formality.
- T4 (`review-branch`, 31 blocking) is red and its report is outside the permitted target, so I
  could not triage it; I did not attempt to.
