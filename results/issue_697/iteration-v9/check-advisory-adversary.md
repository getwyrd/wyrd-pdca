# Adversarial review — issue 697 (advisory, never gating)

Re-ran the asserted red→green myself in a throwaway copy of `$PDCA_TARGET` (scratch, since
removed): green = 6/6 pass with the patch; red (production `reconstruction.rs` reverted to
`339da46`, test kept) = **5 failed, 1 passed**, every failure behavioural
(`Store(SegmentedMapUnsupported { operation: "reconstruction::find_chunk" })`), not a compile
error. The whole `-p wyrd-custodian` suite is green on the patched tree, and
`crates/custodian/tests/reconstruction.rs` is untouched. The evidence is real; the legs drive
`reconcile_step`, not a helper. Two things survive the pass anyway.

- **NEEDS-HUMAN [impl] — `crates/custodian/src/reconstruction.rs:867` (the `?` on `repair_chunk`
  inside `repair_object`'s plan loop) discards *already-completed* repairs in the same object,
  turning a one-chunk write fault into permanent non-convergence for its neighbours.** Because
  the object's single repoint commit (`:896`) now happens only *after* every plan in the group
  has been rebuilt, a `put_fragment` error on a later chunk propagates out of `repair_object`
  and out of `reconcile` (`:318`) before the earlier chunks' successful rebuilds are ever
  committed. Concrete case, demonstrated (not argued): flat `inode:1` with two under-replicated
  chunks `A=0xA100`, `B=0xA200`, RS(1,1), survivor on d0, rebuild target d2; d2 accepts A's
  fragment and persistently rejects B's (`std::io::Error::other`, i.e. transient class, so it
  propagates by design per `:673-674`). On `origin/main` the pass returns `Err` **after** A is
  repaired — `inode:1` `version=2`, `placement=[[0,2],[0,1]]`, only B still queued. With this
  patch the pass returns the same `Err` but `version=1`, `placement=[[0,1],[0,1]]`, **both**
  still queued — and since the fault is deterministic every subsequent pass repeats it
  identically, so A is *never* repaired and each pass strands a fresh unreferenced rebuilt
  fragment on d2 whose `orphan:` mark rode the batch that never committed. That is a repair
  loop that stops converging for a reason inside the loop, i.e. exactly the C-1 permanence the
  brief invokes — introduced by this diff, not pre-existing. It is not a scope question: the
  brief only requires *one commit per object*, so committing the chunks already rebuilt before
  propagating the fault (or classifying a per-chunk store fault as that chunk's abort) satisfies
  it. No leg covers a chunk-store fault at all, which is why it went unseen.
- **NEEDS-HUMAN [human] — `crates/custodian/tests/segmented_map_reconstruction.rs:652-660` still
  cannot falsify the per-obligation whole-record clone, the exact regression the iteration-8
  sign-off named as the item that MUST be resolved.** I re-ran the sign-off's own injection —
  `let _ = std::hint::black_box(object.prior.clone());` at the head of `repair_object`'s plan
  loop (`reconstruction.rs:866`), i.e. a Q×N heap/CPU copy of the eight-entry map for each of the
  four obligations inside `inode:2` — and all six legs stayed **green**. The new `rewrites`
  counter binds the *encode/commit* half only (it charges bytes that cross the store seam), and
  an in-process clone crosses no seam. In fairness the production half **is** done — `RepairPlan`
  now carries `object: usize` (`:112-124`) and no per-obligation record copy remains anywhere on
  the path — so this is a regression-guard gap, not a live defect. The judgment a human owes:
  accept the seam-visible oracle as the achievable bound, or require a test-binary allocation
  probe. Routing it back to Do unqualified risks a fourth round chasing a property no black-box
  test over trait doubles can observe.
- `check-gates.json` C4-verify records *"red without the fix, green with it (6 test(s) ran red)"*.
  Measured: **5** red, 1 green — `an_empty_queue_reads_nothing_and_answers_satisfied` passes on
  the base, exactly as brief §Success-criterion leg 6 pre-declares. The verdict is right; the
  count in the row is not, and a reader taking it at face value would believe leg 6 is a
  behavioural red it is not. Not raised as NEEDS-HUMAN: harness phrasing, no bearing on the fix.
- The test file is **743 raw lines** against the brief's `≤ 460 raw` STOP threshold
  (`brief.md:315-320`); the iteration-8 sign-off accepted 678 explicitly ("do not spend the round
  shrinking the file") and it has grown 65 lines since. Recorded, not raised — the deferral is
  settled per the target rubric's *Deferrals are settled*.

## Refutations attempted that failed

- *Index aliasing between the resolver's answer and the scanned record.* `site.index` comes from
  `resolved.chunks` (`reconstruction.rs:524`) but `repair_object` indexes `object.prior`'s own
  list (`:871`). For a flat map `resolve_snapshot` returns `Cow::Borrowed(&record.chunk_map)`
  (`crates/core/src/metadata.rs:2585`) and cannot restart, so the two are one slice; a segmented
  snapshot — the only one that can be `Superseded` onto a different list (`:2629`) — is refused
  before an index is taken (`:509-516`). No mismatch, no panic path.
- *An unbounded `WriteBatch` per object* (the hazard `restore.rs:95-102` and `:414-424` bound with
  `MARK_BATCH = 1_000` for the same backend). Wrong here: a flat chunk map is one metadata value
  and is capped by `MAX_VALUE_BYTES = 100_000` (`crates/core/src/metadata.rs:327`) — which is why
  segmented maps exist — so the group is a couple of thousand chunks at worst and the batch stays
  a few hundred KB, well inside FDB's transaction limit.
- *First-match-wins drift from the base.* `read_committed`'s `reading.sites.contains_key` guard
  (`:528`) and the unparseable-key `continue` (`:514`) reproduce `find_chunk`'s choice on the base
  row-for-row, including the base's own "skip the record, let a later one claim the chunk" on an
  unparseable key. Duplicate-id behaviour is byte-for-byte the base's (#700, settled).
- *A hole in the drain rule.* Both drain paths — the missing-site miss (`:613`) and
  already-at-full-redundancy (`:707`) — flow into the one `drain_only` batch gated by
  `!reading.incomplete` (`:338`), so no site can drift; a refusal correctly does **not** gate it,
  since a read succeeded.
- *Containment wider or narrower than gc's rule.* `:477-501` is line-for-line `gc.rs:378-416`,
  including containing a decode failure before the `state` check and propagating a non-
  `ChunkMapError` downcast; leg 5 pins the propagation and the already-emitted name.
- *`inode:` prefix pollution* (rows under `inode:` that are not records, which would silently
  mark every reading incomplete): `metadata::inode_key` (`crates/core/src/metadata.rs:33-36`) is
  the only writer and there is no sub-namespace.
- *A refusal writing something, or refusal accounting being per chunk.* Leg 2 compares every
  non-`repair:` row byte-for-byte and asserts exactly one `refused-segmented` row for two
  obligations in one object; `reading.refused` is keyed by the store's own key bytes (`:549`).
- *Vocabulary drift.* `action = "unresolvable-chunk-map"` matches `gc.rs:567`, `restore.rs:830`,
  `scrub.rs:233`, `desired_state.rs:263` field-for-field including `fault`.
