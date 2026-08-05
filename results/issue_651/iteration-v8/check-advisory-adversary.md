# Adversarial review — issue 651 (advisory; never gates)

Method: re-ran the asserted red→green in a scratch copy of `$PDCA_TARGET`
(`pdca-adversary-651-collide`, since removed) with `CARGO_TARGET_DIR` in scratch, and probed the
fix with three new scenarios driven through the **production** entry point
(`wyrd_custodian::reconcile_after_restore`). Toolchain was available; nothing below is a
"could not reproduce".

**The evidence holds.** With the patch, all 7 legs of `crates/custodian/tests/segmented_map_restore.rs`
pass; with `restore.rs` / `desired_state.rs` / `cli.rs` reverted to `origin/main` and the test file
kept, **6 of 7 fail on assertions** (not on a missing symbol — the file compiles against the base),
and (4c) passes both ways as the intended non-regression guard. `C4-verify`'s "red without the fix,
green with it" is real and the discriminator exercises production, not a mirror.

## Findings

- **NEEDS-HUMAN [impl] — `crates/custodian/src/restore.rs:665-675` (and `:303-306`, `:339-342`): the
  ambiguity test keys on *how many D servers are named*, not on *how many committed references exist*,
  so a colliding chunk id whose two objects share a placement defeats criterion (4) entirely — both
  legs, including the data-losing one.** `referenced.placed` is a `HashSet` (`crates/custodian/src/gc.rs:267`),
  so two committed references naming the same server collapse to `canonical[frag] == [d]`; then
  `holders.iter().all(...)` at `:340-341` is identical to the base's `any`, and `attributable`'s
  `holders.iter().all(|&d| d == dserver)` at `:674` returns `true`. Concrete failing store, **run on the
  patched tree**: `inode:2` and `inode:3` both committed with `ChunkRef { id: C, scheme: None, placement: vec![0] }`;
  D server 0 holds `(C,0)`; D server 8 — named by no placement — holds object 3's only copy.
  Observed: `RestoreReport { stranded_marked: 1, .. }` **and an `orphan:` record written for `(8, (C,0))`** —
  i.e. GC will delete the second object's only copy on the next grace window, which is criterion (4b)
  verbatim ("no `orphan:` record is written for it"). Second run, same two objects with server 0 empty
  and the only bytes on server 8: `misplaced: [C, C]`, `dangling: []` — the "the bytes are one hop away,
  restage them" verdict criterion (4a) forbids, emitted for **both** references. This is not the exotic
  shape: the M0–M2 identity route places fragment `index` on D server `index`
  (`crates/core/src/write.rs:80`, `crates/core/src/placement.rs:5`) and an empty `placement` vector decodes
  to that same identity fallback (`crates/custodian/src/gc.rs:418-426`), so two objects sharing an id
  normally name the **same** servers; the divergent placement both fixtures seed
  (`crates/custodian/tests/segmented_map_restore.rs:701-702`, `crates/custodian/tests/restore_reconcile.rs:934-935`)
  is the *narrower* case. The brief's own scope sentence is broader than the code — "a chunk id that more
  than one committed object references is ambiguous, and authorizes neither a recoverability verdict …
  nor a reclamation mark on **any** copy of it" — and the information needed is already in hand:
  `committed_chunks` (`restore.rs:545`) returns **one entry per reference**, so `>1` reference under one
  chunk id is derivable without a second read.

- **NEEDS-HUMAN [human] — the premise the whole of criterion (4) rests on is not supported by the code it
  cites, and the patch pays a real false-`LOST` price for it (`crates/custodian/src/restore.rs:231-240`).**
  The doc comment (and the brief) say ids "are minted from the inode counter the restore rewound … so two
  committed objects can carry the same id". But `chunk_id_minter` packs the **inode id** into the high 64
  bits (`crates/server/src/cli.rs:1788-1797`), and the inode id *is* the record key
  (`crates/core/src/metadata.rs:34`) — so two *live committed* records cannot share an id by that route;
  the gateway path mints per-process epochs ≥ 2^127 (`crates/server/src/lib.rs:229-241`), the in-process
  path resumes above every committed id (`crates/core/src/metadata.rs:2073-2093`), and
  `seed_next_inode_floor` (`crates/server/src/cli.rs:1758-1765`) raises the counter past every committed
  inode at gateway start. The fixtures create the shape by writing two raw `inode:` records
  (`segmented_map_restore.rs:701-702`), which does not demonstrate reachability. This is a squeeze, not a
  quibble: **either** the collision is reachable by a path nobody has named — and then finding 1 is a live
  data-loss gap on the commonest form of it — **or** it is not, and the new conservatism is inert code that
  still bills the operator. Measured cost, run on the patched tree: two committed objects sharing id `C`
  with *divergent* placements and the bytes genuinely displaced to an unnamed server (`displaced_kept: 1`,
  bytes on disk) now reports `dangling: [C, C]`, where the base reported `misplaced` — the CLI prints
  "**2 chunk(s) are LOST** … no reconstruction can rebuild them" for one chunk id whose bytes it just
  counted as kept. A human should name the reachable path (or re-scope criterion 4 to #652) rather than let
  the round-7 reviewer's "criterion (4) settled" stand on the brief's assertion alone.

- **NEEDS-HUMAN [impl] — `crates/server/src/cli.rs:1284-1290`: the operator-facing DANGLING paragraph now
  states a cause that the same patch concedes is false.** It still says, unconditionally, "Restoring past a
  delete resurrects the map after GC took the bytes; no reconstruction can rebuild them" — but the new
  ambiguity-induced `dangling` (`restore.rs:462-473`) fires precisely when the bytes **do** exist, and
  restaging them is actively harmful. This patch already hedged the *runbook*
  (`docs/design/architecture/m4-first-deployment-blueprint.md`: "Usually you restored past a delete …
  Do not restage those"), so the diff concedes the story changed; the operator who reads only stderr — the
  surface this slice exists to fix — gets the old story, and the only place the difference is stated is the
  audit log (`emit_ambiguous_evidence`). This is not the new CLI *cell* the brief declines out of scope;
  it is the accuracy of a sentence the patch moved and reprinted. One hedging clause fixes it.

- **NEEDS-HUMAN [impl] — `crates/custodian/tests/segmented_map_restore.rs:554-559`: criterion (2a)'s binding
  assertion is justified by a coupling that does not exist.** The leg reads "`is_clean` is what the operator
  command exits on"; it is not — `cmd_custodian` exits on `restore_verdict(&report).needs_human`
  (`crates/server/src/cli.rs:1203`, `:1329-1333`), an independent predicate that deliberately ignores
  `stranded_marked` / `under_replicated`. `RestoreReport::is_clean` (`crates/custodian/src/restore.rs:173-180`)
  has **no production caller at all** (grep: only tests), so criterion (2a) is pinned on a predicate nothing
  reads, while `restore_verdict`'s own doc (`cli.rs:1250-1257`) claims the findings "are judged by the same
  predicate that prints them". Net effect: a future report field added to `is_clean()` will never reach the
  exit code. Fix the claim, or make one predicate load-bearing.

## Attempted and could not refute

- Tried to make the mark half delete *more* than the base: `all` implies `any` whenever
  `canonical.get(&frag)` is `Some` (non-empty by construction, `restore.rs:303-306`), so the mark leg is
  strictly more conservative; no new deletion path exists.
- Tried to underflow `by_id_alone - anywhere` (`restore.rs:472`): every element counted in `anywhere` is
  counted in `by_id_alone` (`present ⊆ present_anywhere`, `restore.rs:408`), so the subtraction cannot wrap.
- Tried to make `committed_chunks` (`restore.rs:545-600`) diverge from `gc::referenced_fragments`
  (`gc.rs:360-455`) so that `canonical` would lack an entry `attributable` needs (turning a healthy chunk
  `dangling`): the two walk the same prefix with the same decode-contain / resolve-contain /
  `checked_fragments` treatment, so `(dserver, frag) ∈ placed` for every `Expected::frags` entry.
- Tried to make the containment swallow a genuine store fault (the standing rejection (ii)):
  `restore.rs:578-590` downcasts to `ChunkMapError` and re-raises everything else, mirroring `gc.rs:405-415`.
- Tried an exhaustiveness/silent-fallthrough break from the new public `ReconciliationStatus::PendingUnresolvable`
  (`desired_state.rs:119-124`): no `match` on the enum exists outside tests, and `reconciliation_status` has
  no production caller today, so nothing degrades it to a `_` arm.
- `assert_attributes_blocker` (`segmented_map_restore.rs:244-258`) proves target, action and inode with three
  independent `contains` over the whole capture, so it would also pass if the three came from three different
  events; in these fixtures they do coincide on the one `unresolvable-chunk-map` line, so it is a durability
  nit rather than a false green — not raised as a finding.
- `check-gates.json` records `C4-ci` as `attempts: ["fail","pass"], flaky: true` with a truncated reason
  (`cargo test --workspace --exclude wyrd-dst` failed on the first run). Not refutable from the inputs here
  and not counted against the patch — but worth noting that the first failure is unattributed, and this
  bundle adds tracing-capture-dependent tests, a known flake class in this repo (issue #214, cited at
  `segmented_map_restore.rs:208-222`).
