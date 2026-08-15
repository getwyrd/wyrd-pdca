# Adversarial review — issue #697 (advisory; never gates)

Evidence re-run in both directions. **Green**: `cargo test -p wyrd-custodian --test
segmented_map_reconstruction` at `$PDCA_TARGET` → 8/8 pass. **Red**: the C4-verify log records 7
behavioural failures on `339da46` (leg 4 `Satisfied != Blocked`, leg 6 `3 != 1` namespace scans, leg
2/3 on `SegmentedMapUnsupported`), leg 7 green both sides exactly as the brief declares. Every leg
drives `reconcile_step`, the real fenced control point — no parallel re-implementation. Findings
below were reproduced in a throwaway clone (since removed) driving the same production entry point.

## Findings

- **NEEDS-HUMAN [impl] — `crates/custodian/src/reconstruction.rs:891`: the refusal row's
  `obligations` field counts *references*, not obligations, so a duplicate id inside one segmented
  object over-reports it.** `hits` (`:870-876`) enumerates *positions in `resolved.chunks`*, and
  `emit_refused(&inode_key, REFUSED_SEGMENTED, hits.len())` publishes that as the obligation count.
  Concrete failing case, reproduced: one committed segmented object `inode:90` whose two `seg:`
  records both reference chunk `S_A` — the duplicate-id anomaly the brief's rule 4 puts explicitly
  in scope ("the rule is the same whether the duplicates sit in one record or two") — with `S_A`
  queued **once**. The pass emits
  `"action":"refused","inode":"inode:90","reason":"segmented-chunk-map","obligations":2` while
  exactly **one** obligation exists and one is kept (`queued=[0xB1]`, outcome `Blocked`). That
  contradicts the field's own stated contract at `:1069-1071` ("the obligation count is a field on
  it … a counter that ticked per chunk would measure the queue rather than the store"). The
  discriminator cannot see it: leg 2 (`crates/custodian/tests/segmented_map_reconstruction.rs:381`)
  uses two *distinct* queued chunks, where `hits.len()` and the obligation count coincide — and the
  oracle there is a whole-log substring search (`logged(&audit).contains(BOTH)`, also `:416`), not
  an assertion tied to the refusal row, so it would stay green with the count on the wrong row.
  Fix: count distinct chunk ids in `hits` (leg 2 stays green under that change).

- **NEEDS-HUMAN [human] — `crates/custodian/src/reconstruction.rs:686`: one object still repairs at
  most ONE obligation per pass, and the other q−1 are reported as lost CAS races that never
  happened.** Every plan built from the same object shares one `prior_bytes` handle (`:121`,
  `:527-528`), so after the first commit rewrites the record, the second
  `.require(inode_key, plan.prior_bytes.to_vec())` at `:686` can never match. Reproduced: a committed
  flat object holding two queued, under-replicated chunks → outcome `Changed`, chunk 0 repointed to
  `[0,1,2]`, chunk 1 left at `[0,1,3]` with its obligation still queued, and one
  `monotonic_counter.reconstruction_conflict` increment — the "raced another writer / superseded
  custodian" signal — with no racing writer in the store. **Verified identical on base `339da46`**
  (`Changed`, one repair, one conflict), so I am *not* claiming a regression. It is filed because
  the patch newly documents `prior_bytes` as "shared by every obligation inside the object"
  (`:121`) — which reads as a served case — and because the brief's C-1 convergence claim ("its work
  is bounded by the obligations it holds") is closed only on the *reading* axis: a multipart object
  that lost a failure domain still needs one pass per chunk and mis-attributes q−1 self-inflicted
  conflicts per pass, on the same surface #682 opens. Human call: accept as out of scope / track, or
  correct the comment here.

- **NEEDS-HUMAN [impl] — `crates/custodian/tests/segmented_map_reconstruction.rs:408-411`: leg 3's
  rule-E *ordering* oracle cannot fail for the property it names.** `repair` is the index of the
  first `"action":"repair"` row, i.e. `emit_repaired`, which `reconcile` emits in the metrics block
  at `crates/custodian/src/reconstruction.rs:302-304` — before the repair loop at `:316-322` and
  unconditionally. So `said(&audit, &named(key)) < repair` only proves the names precede a *metric*;
  an implementation that named objects anywhere inside `assess`'s per-obligation loop (violating rule
  E's "where the object is read, before the work loop", and rule D with it) would still pass. The
  line earned its base-red from the *absence* of the name, not from the ordering. A binding oracle
  would compare against the first `commit` the `MemMeta` double observed (it already sees every
  commit at `:70`). Narrow: leg 8 (`:456`) carries the binding "the pass ended and the name
  survived" half.

## Attempted and could not refute

- **C5's red is a timeout, not a survivor — do not spend a finding on it.** I re-ran the exact
  reported mutant (`!=` → `==` at `crates/custodian/src/reconstruction.rs:864`, the rule A guard) in
  a scratch clone: it is caught loudly — 14 of the 15 tests in the untouched
  `crates/custodian/tests/reconstruction.rs` fail, and leg 4 fails on `said(&audit, INCOMPLETE)`
  (with the guard bypassed the scanned segmented record takes the refusal branch, so no
  `withheld` row is emitted). `check-gates.json`'s `C5-mutants` `fail` row (`1 timeouts`, `0 missed`)
  is a host-load artifact under a 20 s auto-timeout, not a coverage hole.
- **Rule A firing spuriously on flat records / livelocking a hot object**: refuted —
  `resolve_snapshot` short-circuits `ChunkMap::Flat` to `Cow::Borrowed(record)`
  (`crates/core/src/metadata.rs:2585`), so `:864` can only ever fire on a segmented root; a
  frequently-rewritten flat object is untouched by the containment.
- **A drain escaping the incomplete-reading gate through the *second* `Drain` route** (`assess`'s
  `missing.is_empty()` at `crates/custodian/src/reconstruction.rs:496-498`, the iteration-2
  carry-forward): refuted — the gate is at the `reconcile` match arm (`:229-230`), so both routes are
  withheld, and leg 3 queues `C_IDLE`, a fully-healthy chunk in a *readable* flat record, asserting
  `queued == [C_IDLE, C_UNSEEN]`.
- **`Ok(None)` from the resolver silently draining an object that is only mid-overwrite**
  (`:833`): refuted — I built a segmented generation whose live root is `Pending`; the resolver
  answers a typed `ChunkMapError` ("seg:… is absent while the root still names this generation"), so
  the object is contained (`unresolvable-chunk-map`, `Blocked`, `obligations:1` withheld), not
  drained.
- **Rule C being non-discriminating**: refuted — re-deriving `metadata::inode_key(7)` from the key
  `inode:007` would make the `require` at `:686` conflict, so leg 3's
  `placement(F_WORK, 0) == [0,1,2]` goes red.
- **An unparsable `inode:` key silently skipped** (the base's behaviour): refuted — `inode:zz`
  beside healthy work yields `"action":"unparsable-inode-key"`, `Blocked`, and the healthy repair
  still lands. Fail-closed as rule C asks; no production writer produces such a key
  (`crates/core/src/metadata.rs:35` is the sole `inode:` constructor).
- **Rule B's orphan-marking of a fragment a hidden object still references** is
  recorded-rejected in `results/issue_681/review-rejected.md` and settled per the target rubric's
  *Deferrals are settled* protocol — not re-raised.
- **Budget** (informational): 201 production / 379 test semantic added lines against the brief's
  230 / 380 caps, 565 raw against 620, exactly 2 files; `crates/custodian/tests/reconstruction.rs`
  unmodified. The test file sits **one** semantic line under its cap, which constrains any follow-up
  assertion.
