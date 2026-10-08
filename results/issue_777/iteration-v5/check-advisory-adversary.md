# Adversarial review — issue #777 (seg:-resident repair through `repoint_chunk`)

Verdict: **could not refute the fix.** I found no input that makes production write a wrong
placement, drain an obligation over a hole, or certify a pass it should not. What I did find
are two changed lines that no test pins: hand-made mutants of them survive the whole
`wyrd-custodian` suite. Both are cheap test additions. Neither is a safety bug.

## Findings

- NEEDS-HUMAN [impl] — **The choice of which root the move is pinned to is untested.**
  `crates/custodian/src/reconstruction.rs:702` sets `prior: resolved.record.as_ref().clone()`.
  The comment at `:648-651` gives the reason: the move must be conditioned on the generation
  the resolver *answered from*, which is the live root when a supersede during the resolve
  made it restart. I changed it to `prior: record.clone()` (the scanned, retired root) and
  ran `cargo test -p wyrd-custodian`: **every test binary passed.** Concrete case: arm
  `Race::AfterSegmentPage` with the same `records()` written under a new group
  `SegmentGroup::new(NONCE, EPOCH + 1)` plus a version-2 root naming that group. The
  resolver sees the group change and restarts onto the new root. Production answers
  `Changed`, the new group's `seg:…:1` names `[[0,2],[0,1]]`, the old group is untouched,
  and the queue is empty. That is correct, and I checked it. The mutant answers
  `Satisfied` with `CHUNK` still queued and nothing repointed, because it pins the retired
  root and loses the CAS. This only costs liveness (one lost pass per supersede, never a
  bad write, because the root pin still fails closed). But the property the comment claims
  has no test. Fix: add this leg to `crates/custodian/tests/segmented_map_repoint.rs`. It
  uses only base symbols and the existing `MemMeta` race hook.

- NEEDS-HUMAN [impl] — **A prepare-time conflict can be relabelled as an abort and no test
  notices.** At `crates/custodian/src/reconstruction.rs:1185`,
  `Ok(metadata::Repoint::Conflict) => return Ok(RepairOutcome::Conflict)`. When I mutated it
  to `RepairOutcome::Aborted`, the whole suite still passed. With that mutant, leg 3
  (`segmented_map_repoint.rs:554`) ticks `reconstruction_aborted` with
  `reason:"unplaced"` instead of `reconstruction_conflict`. That tells the operator "no
  server to place on" for what was a lost race. The verdict and the store do not change.
  `assert_lost` (`:442`) checks the store and the verdict but no counter. Fix: in
  `assert_lost`, assert one `reconstruction_conflict` tick and zero `reconstruction_aborted`
  ticks (legs 3 and 4 both go through it).

- NEEDS-HUMAN [impl] — **A docs sentence is wrong for the flat arm.** At
  `docs/design/architecture/06-runtime-view.md:40`: "a compare-and-swap on the root
  generation, on that record as the move itself re-reads it". The flat arm of
  `repoint_chunk` (`crates/core/src/metadata.rs:3259-3276`) re-reads nothing. It pins
  `encode(generation)` from the pass's own snapshot. Only the segmented arm re-reads (its
  `seg:` record). Suggested wording: "…and, for a segmented map, on the segment record as
  the move itself re-reads it…". A wording fix only.

## The gate evidence and the reviewer's verdict

- **C5 "pass" (16 mutants: 6 caught, 10 unviable) says little about test strength.**
  `cargo mutants` made almost no mutants for the new match arms, the `read_committed` guards
  or the `prior` choice. Earlier rounds (carry-forward items 1–3 in `brief.md`) were sent
  back for exactly this, and the two survivors above are the same class. Of my other hand
  mutants, all were caught: dropping the once-per-object dedupe in `Reading::contain`
  (`:539`), disabling the canonical-key guard (`:659`), skipping `(None, None)` (`:667`),
  containing without naming (`:449`), and mapping the move's `ChunkMapError` to `Conflict`
  (`:1194-1195`).
- **C4-verify red→green holds up.** I re-ran the new file on the target: 12/12 green. The
  frozen red log matches the base: `refused-segmented` rows, `Blocked`, the base's
  unchecked `version + 1` overflow panic at the base's `:1157`, and "fault never fired" for
  leg 11, since the base issues no `get` on a `seg:` key. Every leg goes through
  `reconcile_step` and checks the store. The test names no symbol this patch adds. The race
  timing in leg 3 really discriminates: if the racing write had landed before the resolve,
  the plan would be built from `[0,7]` and the pass would answer `Changed`, not
  `Satisfied`. Leg 11 cannot pass for the wrong reason, because the resolver code is the
  base's and the base never `get`s a `seg:` key.
- Diff-cov's one MISS (`reconstruction.rs:443`, the `Aborted` arm) is a base arm with a new
  reason string. Not worth a round.

## Attempted and could not refute

- **Addressing.** Segments are validated at decode to tile the object contiguously and
  are never empty (`metadata.rs:1385`, `SegmentedMap::new`). So the summed `byte_offset`
  built at `reconstruction.rs:677-680` always matches the primitive's
  `segment_may_hold` + `chunk_at` lookup. I found no valid, stable object that conflicts
  forever at prepare time.
- **Root CAS identity.** The segmented root's group, nonce and table go through strict,
  validating serde (`metadata.rs:987-1071`). The segment pin uses the raw re-read bytes
  (`:3324`). So a segmented move opens no new decode→encode mismatch. (The flat
  `placement` `#[serde(default)]` without `skip_serializing_if` at `:360` is a base
  behaviour of the flat arm, not this diff.)
- **Drain over a hole.** A `Contained` move sets `reading.incomplete` inside the repair loop
  (`:448-451`), before the drain gate at `:477`. So an object found unusable at the move
  never lets `drain_only` discard an obligation.
- **Ceiling boundary.** The writer and the resolver both use `> MAX_VALUE_BYTES`
  (`metadata.rs:606`, `:2887`, `:3295`), so the move never writes a record the read side
  then refuses.
- **Store fault versus object damage.** A non-`ChunkMapError` from the move ends the pass
  (`:1196`). Leg 11 pins this.
- Out of scope or settled, so not raised: the flat noncanonical key (#698); the stranded
  destination fragment (#723); the DST property (`// deferred: #682` at `:1165`, accepted
  at sign-off); the ceiling-refusal false alarm (accepted at sign-off); duplicate chunk ids
  (#700).
