# Adversarial review — issue #697 (advisory, non-gating)

Red→green reproduced independently in a scratch clone (`cargo test -p wyrd-custodian --test
segmented_map_reconstruction`): 6/6 green with the patch, 5/6 fail behaviourally with
`reconstruction.rs` reverted (leg 6 green on base, as declared). Whole `wyrd-custodian` suite
green, `crates/custodian/tests/reconstruction.rs` unmodified. Two refutations landed; the rest
of the attack surface held.

## Findings

- **NEEDS-HUMAN [human] — an obligation is silently DRAINED, and the pass falsely certifies, when
  its object retires under the read.** `crates/custodian/src/reconstruction.rs:472`
  (`Ok(None) => continue`) drops the object without marking the reading incomplete;
  `:596` then classifies every chunk that object held as `Assessment::Drain`; `:323` gates the
  drain batch on `reading.incomplete` **only**, so the delete goes through; `:331` answers
  `Changed`/`Satisfied`. Concrete failing case, executed against this exact patch (in-memory
  doubles, `MetadataStore::get` answering the root that a concurrent writer left behind):
  the scan returns a **committed segmented** root at `inode:1` holding queued chunk `0xA100`;
  by the time `resolve_chunk_map` re-reads that root it carries a **Pending** generation, so
  `crates/core/src/metadata.rs:2663-2665` answers `Ok(None)`. Result: `queued == []` — the
  obligation was **deleted** — with **zero** `refused-segmented` and **zero**
  `unresolvable-chunk-map` rows, and the pass answered **`Reconciled::Changed`**. That is
  exactly "an obligation discarded for want of a reading" and a certification over an object the
  pass never read — the two invariants brief §Invariant says this slice exists to restore, and
  the base could not reach it (a segmented record ended the pass with `Err`, draining nothing).
  Why a human, not a rebuild: the obvious fix (treat `Ok(None)` on a record the *scan* saw
  `Committed` as "retired under the read" → `incomplete`) contradicts a **pinned** brief rule
  ("`Ok(None)` from the resolver is **skipped**, exactly as both merged peers skip it — not
  counted, not named"), and today's reachability is bounded by two facts that will not hold for
  long: the segmented shape has no producer in this build (`crates/core/src/metadata.rs:1460-1463`,
  #653) and nothing writes a `Pending` inode root. Decide: fix in-slice, or file it against
  #653/#682 before either lands.

- **NEEDS-HUMAN [human] — leg 4 is not a Q×N oracle; it is a scan counter, and the Q×N work it
  is claimed to forbid walks straight past it.** `crates/custodian/tests/segmented_map_reconstruction.rs:611-616`
  and `:642` assert only `MemMeta::inode_scans == 1`. I injected the prohibited per-obligation
  whole-record copy at `crates/custodian/src/reconstruction.rs:603` —
  `std::hint::black_box(metadata::encode(&reading.objects[site.object].prior.clone()))`, i.e. a
  full N-entry map clone **and** re-encode for each of the Q obligations — and **all six legs
  stayed green**. So the diff's headline claim ("O(N) rows instead of O(Q×N)", `:154-158`) is
  bound only against re-scanning, not against the clone/encode path, and a future rebuild can
  reintroduce Q×N heap/CPU without a single test going red. This is the item iteration 8's
  sign-off recorded as the one that "MUST be resolved" ("Make the test observe full-map
  clone/rewrite cost"); the *implementation* half was done (the `object: usize` index at `:304`
  / `:521` really does share one snapshot — I confirmed that by reading, not by the test), the
  *oracle* half was not. Human call because it may not be bindable through the `MetadataStore` /
  `ChunkStore` seams at all (copies are invisible there): either accept an allocation-counting
  oracle, or **record-reject the demand explicitly** so it stops re-surfacing each round — do not
  leave it silently unmet a third time.

- **NEEDS-HUMAN [human] — the C-1 "work bounded by the obligations, not their product with the
  namespace" claim is true per pass and false per convergence, and this diff's own leg now
  codifies the difference as expected.** `crates/custodian/tests/segmented_map_reconstruction.rs:636-645`
  asserts convergence takes up to `OWED.len()` passes. Measured on this patch: 8 obligations
  inside one 8-chunk flat object need **8 passes**, and after pass 1 the rebuild target already
  holds all 8 rebuilt fragments — every pass erasure-rebuilds and uploads a fragment for every
  not-yet-landed obligation and then throws all but one away on the CAS (`reconstruction.rs:304-308`,
  one version-conditional commit per chunk). Aggregate cost to drain a queue concentrated in one
  object is therefore Θ(Q) namespace scans and Θ(Q²) fragment rebuilds/uploads. **This is base
  parity — I ran the same fixture against `origin/main:crates/custodian/src/reconstruction.rs` and
  got the identical 8 passes — so it is NOT a regression and the grouping machinery must NOT be
  rebuilt** (iteration 9's sign-off settled that route). The finding is against the *claim*: brief
  §Invariant's fifth bullet reads as a convergence property and only the per-pass property was
  delivered. Narrow the claim, or track the residue.

## Attempted and could not refute

- **C5's two "missed" mutants are equivalent mutants, not a coverage gap.** `reconstruction.rs:864`
  (`size: object.prior.size`) and `:866` (`state: InodeState::Committed`) are both re-supplied by
  the struct-update tail `..object.prior.clone()` at `:871`, and `read_committed` admits only
  `Committed` records (`:467`), so deleting either field is behaviour-preserving. No test can kill
  them; the `C5-mutants` red is noise here.
- **Per-object refusal accounting holds.** My first probe showed one row for two segmented objects
  — that was my fixture reusing one `SegmentGroup` nonce. With two genuinely distinct groups the
  pass emits 2 `refused-segmented` rows, 2 counter ticks, and names both `inode:1` and `inode:2`.
- **A refusal does not starve the flat work beside it**: refusal + under-replicated flat chunk in
  one pass → the flat repoint lands, the refused obligation stays queued, `Blocked`, `(scan,
  scan_page) == (1, 1)`.
- **Both drain paths really are behind one gate.** The "already at full redundancy" drain
  (`reconstruction.rs:688-691`) is also suppressed under an incomplete reading — verified by a
  second pass over a store carrying an undecodable record; the obligation was kept.
- Also probed without success: priority ordering (`:260` unchanged, no inversion), first-committed-
  reference-wins parity with the base's `find_chunk`, the unparsable-key `continue` at `:497`
  (base-identical), an undecodable *uncommitted* row under `inode:` (contained per the pinned
  gc.rs rule), index validity of `plan.object` after the priority sort, and the flat CAS
  precondition (`:871-877`, byte-identical construction to the base).
