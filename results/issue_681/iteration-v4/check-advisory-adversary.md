# Adversarial review — issue 681 (`passes-read-through-resolver-contained`)

Advisory only; nothing here gates. Toolchain was available: I rebuilt the workspace in scratch
and re-ran the asserted red→green **in both directions** myself.

## The evidence held up

- **Red reproduced.** Copied the tree to scratch, reverted only the three production files to
  `HEAD` (`339da46`) keeping `crates/custodian/tests/segmented_map_passes.rs`: **6/6 fail**, all
  behaviourally (compile is clean — no symbol this patch introduces is named, so the red does not
  degrade to UNVERIFIABLE): `find_chunk met a segmented chunk map`, `key must be a string`,
  `read ONCE, not once per obligation left: 3 right: 1`, `ambiguity is no repair left: Changed`.
  **Green reproduced:** 6/6 pass with the patch restored.
- **It is the production path.** Every leg drives `reconcile_step` (the real fenced control point,
  `crates/custodian/tests/segmented_map_passes.rs:450`) and `backfill::reconcile` (`:458`) over
  trait doubles — no re-implementation, no mock of the code under test. Nothing is tautological:
  each leg asserts *positive* work (placements moved to `[0,1,2]`, bytes landed on server 2,
  obligations discharged) beside the "`Ok` was returned" half.
- One brief inaccuracy for the record: leg 6 is declared "*NOT* base-red … passes before and
  after" (brief line 119), but it fails on base too (the undecodable record raises a decode error
  before the injected store fault). That makes the red stronger, not weaker.

## Findings

- NEEDS-HUMAN [human] — **The brief's decision-4 "unreachable by construction" claim is false, and
  with it the pre-declared recorded-rejection of the Tier-0 DST leg. Demonstrated, not argued.**
  `crates/custodian/src/backfill.rs:121` (same shape at `crates/custodian/src/reconstruction.rs:823`
  and `crates/custodian/src/rebalance.rs:240`): when the scan snapshot is *segmented* and the live
  root has since been replaced by a *flat* one, `resolve_chunk_map` answers `Superseded`
  (`crates/core/src/metadata.rs:2338-2339`), restarts onto the live root, and the pass then finds
  `resolved.record.chunk_map.is_segmented() == false` (`backfill.rs:166`) and **writes**. I built a
  metadata double whose `scan` answers a stale segmented root while `get` answers a live flat root
  carrying an empty placement: `backfill::reconcile` returned `Changed` and committed the record
  from `version 9` to `version 10` — a commit framed by, and CAS'd on, a generation the pass never
  scanned. The *write* is correct (it obeys decision 4 exactly), but the brief's premise — "only a
  segmented resolve can restart … and every segmented write here is refused. So every commit this
  slice performs is framed by the scan snapshot exactly as today" — and the rejection reason
  "every write it performs is on a FLAT object and keeps its existing version-conditional CAS on
  the scan snapshot, byte-for-byte the behaviour on the base" (brief §Verification posture) are
  both untrue on that path. It is not reachable in the deployed build *today* (nothing can publish
  a segmented root until #653), which is why this is a scope/fitness call rather than a bug: the
  brief itself required it to reach sign-off ("If Do finds a commit path that CAN be reached
  through a restarted resolve, that falsifies this reasoning … leave it for sign-off"), and the
  human should confirm whether Do surfaced it and whether the DST rejection still stands.
- NEEDS-HUMAN [impl] — **The refusal vocabulary this patch introduces is asserted by nothing; the
  discriminator would stay green with every reason inverted.** The only audit-vocabulary assertion
  in the whole file is `"action":"refused"` on the *rebalance* seam
  (`crates/custodian/tests/segmented_map_passes.rs:566`); `assert_named` (`:204`) matches on the
  object name alone. Concrete cases that keep all six legs green: (a) swap `REFUSED_SEGMENTED` and
  `REFUSED_INCOMPLETE` at `crates/custodian/src/reconstruction.rs:399` and `:407`, so an operator
  is told a `seg:`-resident chunk is an "incomplete reading" and vice versa; (b) give all three
  `cannot_account_for` call sites one `action` label (`reconstruction.rs:803`, `:817`, `:830`;
  `backfill.rs:102`, `:116`, `:128`; `rebalance.rs:221`, `:235`, `:247`), collapsing "the bytes
  will not decode" / "the map will not resolve" / "the key is not an `inode:` key" into one
  indistinguishable row; (c) delete any of the five new counters
  (`*_unaccounted_records`, `backfill_declined_records`, `reconstruction_repair_refused`,
  `reconstruction_ambiguous_chunk_id`, `rebalance_evacuation_refused`) — nothing reads one back.
  This is inside the brief's own scope, not beyond it: backfill is required to leave the record
  "declined **with a stated reason** on the audit seam **and counted**" (§Scope), and "a stated
  reason, never a silent skip" is what `emit_declined` (`backfill.rs:300`) claims to deliver.
  Cheapest binding: one added assertion per existing leg on the `reason`/`action` string.
- NEEDS-HUMAN [impl] — **Two comments assert a byte-exactness property the code does not
  establish.** `crates/custodian/src/reconstruction.rs:660` ("the CAS requires those EXACT bytes
  back … no re-encoding sits between the read and the precondition") and
  `crates/custodian/src/rebalance.rs:429` ("on the EXACT bytes it answered with, so no re-encoding
  sits between the read and the precondition") both describe `prior_bytes` — which is
  `metadata::encode()` of the *decoded* record (`reconstruction.rs:867`, `rebalance.rs:187`), i.e.
  precisely a re-encoding, the same one the base performed at `require(key, encode(&plan.prior))`.
  Behaviour is unchanged, so this is not a regression; but the CAS still depends on decode→encode
  being byte-identical (guaranteed by `docs/design/architecture/08-crosscutting-concepts.md:85`,
  not by this code), and the rubric's *Serialization identity* class is exactly the place where a
  comment claiming a guarantee the code borrows from elsewhere hides the next defect. Either
  restate the comment or carry the scan value's own bytes on the flat path.

## Attempted refutations that failed

- **`C5-mutants` (red, advisory) is not a real signal here, despite the brief pre-declaring that
  "a survivor here is a real signal … not noise."** All four survivors are *equivalent* mutants:
  `mutants.out/missed.txt` names only `delete field size` / `delete field state` from the
  `InodeRecord` expressions at `crates/custodian/src/rebalance.rs:419`/`:421` and
  `crates/custodian/src/reconstruction.rs:672`/`:674` — and each struct expression ends in
  `..prior.clone()` (`rebalance.rs:426`, `reconstruction.rs:679`), whose `size` is `prior.size` and
  whose `state` is `Committed` on every path that reaches there (both walks skip
  `state != Committed`, and `resolve_current_chunk_map` answers `Ok(None)` for a non-committed
  root). Deleting either field produces a byte-identical record, so no test can kill them. The same
  shape existed on the base; they surface only because the diff touched those lines.
- **Draining an obligation on an incomplete reading** — I tried to reach `Assessment::Drain` with a
  hole in the reading (`reconstruction.rs:403`): the guard is `index.unaccounted == 0`, and every
  containment site increments it through the single `cannot_account_for` entry point, including the
  unparsable-key branch the base silently skipped. Could not.
- **Repairing on an incomplete reading** *is* still permitted (`reconstruction.rs:394-408` refuses
  the drain but not the repoint), so a record the pass could not read may still reference a chunk
  whose fragments a repair orphan-marks. I could not turn it into loss: GC withholds *every*
  reclamation while its own reference set is incomplete (`crates/custodian/src/gc.rs:186-190`), and
  once the record is readable again its fragments are protected as referenced.
- **Duplicate-`ChunkId` rule** — tried the two-in-one-record case bypassing the `Entry::Occupied`
  arm (`reconstruction.rs:737`): `hits` (`reconstruction.rs:839-845`) collects both positions and `note` (`:732`) is called twice, so
  it lands on the ambiguity arm exactly as the two-record case. Could not.
- **Over-containment** — tried to force a spurious `Blocked`: a segmented object with no fillable
  chunk, no draining fragment and no queued chunk raises no refusal in any of the three passes
  (`backfill.rs:153`, `rebalance.rs:289`, `reconstruction.rs:846`). Could not.
- **The Q×N property** — tried to find a second namespace walk: `emit_remaining` no longer scans
  (`backfill.rs:221`) and `locate_queued_chunks` is called once (`reconstruction.rs:165-169`).
  Could not.
- **Memory bound** — `prior_bytes` is one `Arc<[u8]>` per object *holding an obligation*, shared
  across that object's obligations (`reconstruction.rs:867`, `rebalance.rs:313`); strictly less
  retention than the base's per-plan `InodeRecord` clone. Could not.

## Observations (no action implied)

- Budget is at the ceiling and the *test* allocation is over: added semantic (non-blank,
  non-comment) lines are reconstruction 180 / backfill 80 / rebalance 93 / test **525** against
  allocations of 210 / 100 / 100 / **470**. The aggregate (878) fits under 880 only because the
  three production files came in under theirs; the binding STOP conditions (4 files, 758 ≤ 780 raw
  test lines) are met.
- Pinned decision 5 ("a refusal is reported once per object, not once per chunk") is enforced for
  rebalance (`rebalance.rs:328-330`) but reconstruction emits one refusal line per *obligation*
  (`reconstruction.rs:259-262`), so one `seg:` object holding Q queued chunks yields Q lines. The
  decision's second sentence scopes it to rebalance and an obligation is reconstruction's unit of
  work, so I am not raising it as a finding — but it is the reading a later reviewer will re-open.
