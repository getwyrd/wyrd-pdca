# Adversarial review — issue #681 (advisory, non-gating)

Re-ran the asserted red→green on a throwaway copy of `$PDCA_TARGET` (scratch dir removed).
**Red is real:** with only `crates/custodian/src/{backfill,rebalance,reconstruction}.rs` reverted to
`origin/main` @ `339da46` and the new test file kept, all 6 legs *compile* and fail behaviourally
(`find_chunk met a segmented chunk map…`, `key must be a string`, `left: Satisfied right: Blocked`) —
not a compile error, not a degraded UNVERIFIABLE. **Green is real:** 6/6 pass with the patch, and the
legs drive the production entry points (`reconcile_step` through `Custodian::elect`/`FencedZone`, and
`backfill::reconcile`), not a re-implementation. The `C4-verify` row is therefore not refuted.

## Findings

- **NEEDS-HUMAN [impl] — `crates/custodian/src/rebalance.rs:196` — the over-containment control for
  rebalance is load-bearing and bound by nothing.** `Refusals::refuse` returns without counting when
  `fragments + unreadable == 0`; that guard is the only thing stopping a *healthy* segmented object
  from blocking a drain it has no stake in. Concrete failing case, run and measured: seed one
  segmented object whose chunks are placed entirely off the draining server
  (`seed_seg(SEG, NONCE, 2, [(S_DRAIN_A,[0,1,4],…),(S_DRAIN_B,[0,1,4],…)])`) plus a fully-placed flat
  object, mark server 3 draining, and run `step(false, true)`. With the guard present the pass
  answers `Satisfied` (correct). Replace the guard body with a no-op and the pass answers **`Blocked`
  — and all six legs in `crates/custodian/tests/segmented_map_passes.rs` still pass, and the entire
  `cargo test -p wyrd-custodian` suite still passes** (verified: 11 green binaries, 0 failures). That
  is the *common* production shape — most objects hold nothing on the one draining server — so the
  unguarded behaviour would mean no decommission ever certifies on any store that has published a
  multipart object, i.e. exactly the class of defect this slice exists to remove, in mirror image.
  The brief builds this control explicitly for reconstruction
  (`crates/custodian/tests/segmented_map_passes.rs:698`, *"the control against OVER-containment …
  this store's answer is the certifying one"*) and for the store-fault path (leg 6), but leg 2 only
  ever seeds a segmented object that *does* hold draining fragments
  (`crates/custodian/tests/segmented_map_passes.rs:512`, `:562`), so rebalance's certifying answer
  over a segmented object is never asserted. Fix is one sub-assertion in leg 1 or leg 5 (a
  `step(false, true)` over a segmented object with nothing on the draining server, asserting the
  answer is not `Blocked`). The same hole exists in backfill for a segmented record with no empty
  placement (`crates/custodian/src/backfill.rs:142`/`:156`), though there the decline is structurally
  gated by `to_fill.is_empty()` and a mutation of it is caught incidentally by leg 2's gauge
  assertion.

- **The `C5-mutants` row is weaker evidence than its "45 caught, 37 unviable, 0 missed" reads.**
  `cargo mutants` mutates operators and replaces function bodies; it does not express *"delete this
  early-return guard"*. For `rebalance.rs:196` every mutant it can generate is caught (`==`→`!=` and
  `+`→`*` both flip leg 2 to non-`Blocked`; `+`→`-` panics on `0usize - 1`; the whole-body mutant
  kills leg 2), while the one change that actually matters survives the whole suite. A reviewer
  reading `0 missed` as "the refusal predicates are pinned" would be rationalising; it pins the
  arithmetic, not the predicate.

## Attempted and could not refute

- **Pinned decision 4 (a write is CAS'd on, and framed by, the generation its chunks were read
  from).** Tried to reach a commit framed by a *restarted* resolve. Cannot: all three passes branch
  on the **snapshot's** shape (`backfill.rs:156`, `rebalance.rs:265`, `reconstruction.rs:825`) and a
  flat snapshot returns `Resolution::Answer(Cow::Borrowed(chunks))` with no store read and no
  supersede check (`crates/core/src/metadata.rs:2585-2586`), so `resolved.chunks` is provably the
  snapshot's own list on every path that writes. A segmented snapshot that restarted onto a live
  *flat* root is (conservatively) declined/refused, not written.
- **Two obligations inside one record now sharing one stale `prior_bytes`.** Suspected the single
  per-pass index would lose the second repair where the base's per-obligation `find_chunk` would not.
  Built the case and ran it on base and on the patch with a canonically-encoded record: both answer
  `Changed`, both repair chunk 0 and leave chunk 1 queued on a lost CAS (`reconstruction.rs:663`).
  Identical — not a regression.
- **`unparsable-inode-key` as a new permanent `Blocked` state** (`backfill.rs:105`,
  `rebalance.rs:239`, `reconstruction.rs:790`), which `gc::referenced_fragments` — the containment
  rule the brief mandates copying — does *not* have. Measured: a store with one `inode:-1` row makes
  all three passes answer `Blocked` forever and withholds every drain. But `metadata::inode_key` is
  the sole writer of the prefix (grep across `crates/`), and #652's `high_water_marks`
  (`crates/core/src/metadata.rs:2158-2172`) already names-and-continues on exactly this row with
  exactly this reasoning, so the divergence has repo precedent and is strictly better than the base's
  silent skip.
- **Draining an obligation on an incomplete reading.** `assess` gates `Drain` on
  `index.unaccounted == 0` (`reconstruction.rs:396`) and `Ok(None)` from the resolver is the resolver's
  own "no live committed generation" (`crates/core/src/metadata.rs:2630`), which the base treated the
  same way. Could not construct a store where a live reference is drained.
- **`Reconciled::Blocked` leaking into an operator-visible "decommission is safe" answer.** Grepped
  every consumer: outside tests nothing reads these three loops' `Reconciled`, and the drain-status
  surface (`desired_state::reconciliation_status`, `crates/custodian/src/desired_state.rs:181-246`) is
  computed from `gc::referenced_fragments`, which already resolves segmented maps and already answers
  `Pending`. No fitness gap there.
- **Test tautology / over-broad assertions.** `assert_gauge`
  (`crates/custodian/tests/segmented_map_passes.rs:190`) parses the full digit run and requires
  exactly one sample, so a 10× over-report or a duplicate emission fails; `assert_seam` searches the
  quoted key so `"inode:0"` cannot match `"inode:006"`; the CAS-key and CAS-bytes rules are bound
  indirectly but soundly by `assert_flat_work_done` (`:475`) — re-deriving `metadata::inode_key(id)`
  or re-encoding the record loses the CAS against the fixture's deliberately non-canonical `stored()`
  spelling (`:272`), and the "the work happened" assertions then fail.
