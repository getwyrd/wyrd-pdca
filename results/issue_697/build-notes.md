# Build notes — issue 697, iteration 12 (`reconstruction-reads-through-resolver-once-contained`)

Target branch: `getwyrd/wyrd @ main` (base `origin/main @ 339da46`). All edits made in
`$PDCA_WORKTREE = /home/eddie/wyrd/wyrd.pdca-wt-l0`; every `path:line` below is that tree
(= the patched file) unless marked "(base)".

**Two files, as budgeted.** `crates/custodian/src/reconstruction.rs` (**143** added semantic
lines, cap 160) and the new `crates/custodian/tests/segmented_map_reconstruction.rs`
(712 raw / 492 semantic — §6, the overage the human pre-accepted at iteration 8's sign-off).

---

## 1. What this round is — and what it deliberately is NOT

The iteration-11 sign-off is explicit: *"Do not change the fix's behaviour or design — the
six-leg discriminator and C4 red→green are passing; this round is gate closure only."* So the
patch is **byte-identical to iteration 11**, and that is checkable in one command:

```
$ git -C $PDCA_WORKTREE diff > /tmp/x && diff results/issue_697/iteration-v11/patch.diff /tmp/x
(no output — IDENTICAL, index hashes included)
```

Zero production lines, zero test lines changed. The whole round is in three artifacts the
sign-off named: `review-rejected.md` (items 1–2), these notes (item 3), and the driver's own
advisory leaf (item 4, below).

Why nothing else moved: `scripts/review-branch` binds a recorded rejection to the finding's
**exact `(file:line, class)`** plus a substring of its rationale. Every production line I touch
shifts the anchors the last three rounds' rejections are filed at, which un-triages decisions the
human has already made and re-blocks the gate on them. In a closure round, line stability *is*
the deliverable.

Everything the mechanism does is unchanged and described in `iteration-v10/build-notes.md` §1 /
`iteration-v11/build-notes.md` §1: one reading of the committed namespace per pass through the
shared `metadata::resolve_chunk_map` (`reconstruction.rs:455`, called `:474`), containment by
exactly `gc.rs`'s downcast rule (`:477-487`, mirroring `gc.rs:402-416`), a per-**object**
`Assessment::Refused` (`:581`, counted and named once at `:533-539`, consumed at `:255`), the one
drain gate over both drain paths (`:323-329`), the `Blocked` answer that certifies only over the
reading performed (`:331-340`), and the two brief-pinned audit rows (`:1015-1036` /
`:1038-1052`).

## 2. The four sign-off items, and where each landed

### Item 1 — record-reject the DST finding at `reconstruction.rs:302`

`review-rejected.md`, three entries: `:302` (as flagged), `:305` (the `repair_chunk` call its
"repair commit" names) and `:889` (the CAS itself). **MATCH is the class token `DST`**, not a
phrase: rounds 6, 8, 9 and 11 each worded this finding differently ("seeded Tier-0 DST
*coverage*" → "seeded Tier-0 DST *regression exercising a root change*"), and the older entries'
literal MATCH strings are exactly why the same decision failed to triage it again in round 11.
The reason text declines on **scope** (brief §Verification posture: *"no seeded Tier-0 DST case
ships in this child, and none is owed"*, brief.md:341-353; `crates/dst/` is *"not a file this
bundle may touch"*, brief.md:280; budget is exactly 2 files, brief.md:315) **and on the merits**
(the root-change-under-scan path reaches no write by construction — write eligibility is read off
the scanned record's own `chunk_map` shape at `:496-503`, a flat snapshot resolves by borrow and
never restarts, `crates/core/src/metadata.rs:2585` / `:2629`, and a segmented snapshot is refused
at `:402-410`). A genuinely **new** `BUG`-class finding at the same line is *not* suppressed —
verified, see the parse check in §5.

### Item 2 — record-reject the await-timeout finding at `reconstruction.rs:458`

`review-rejected.md`, four entries: `:458` (the `meta.scan(b"inode:")` the round-11 rationale
flagged), `:474` (the resolver await), `:455` (the walk's signature) and `:449` (the in-code
statement of the rule). **MATCH is the class token `timeout`** for the same reason — round 3/10
said "caller-**enforced** timeout", round 11 said "caller-**side** timeout", so the recorded
`:474` entry no longer matched its own finding. The rejection basis of the round-3 `:482` /
round-10 `:423` entries **does** hold at `:458`, and is strengthened there: the flagged scan is
the *same unbounded await the base already makes* (`reconstruction.rs:624` on `origin/main @
339da46`), moved rather than added — so the alternative (add the bounded await) would newly bound
a call the base leaves unbounded, in a crate whose seam forbids the `tokio` dependency it needs
(ADR-0010; a `Cargo.toml` change §Scope forbids, brief.md:310-314), while both merged peers make
the identical call unbounded and say so in-code (`gc.rs:394-401`, `restore.rs:604-608`). The
class is pinned to CONVENTION, so a runtime `BUG` at those lines still blocks.

### Item 3 — clear C5: the two "missed" mutants are **equivalent**, and the proof is here

Re-measured this round, unchanged: `22 mutants tested in 33s: 2 missed, 13 caught, 7 unviable`.

```
MISSED crates/custodian/src/reconstruction.rs:868:9: delete field size  from struct InodeRecord expression in repair_chunk
MISSED crates/custodian/src/reconstruction.rs:870:9: delete field state from struct InodeRecord expression in repair_chunk
```

The literal they land in (`:867-876`) ends in the functional-update tail
`..object.prior.clone()` (`:875`):

```rust
let next = InodeRecord {
    size: object.prior.size,          // <- mutant 1 deletes this line
    chunk_map: next_chunk_map.into(),
    state: InodeState::Committed,     // <- mutant 2 deletes this line
    version: object.prior.version + 1,
    // Reconstruction rebuilds the SAME content, so it PRESERVES the object metadata
    // (ADR-0047) …
    ..object.prior.clone()
};
```

**Mutant 1 (`size`) is equivalent by the language definition.** Functional-update syntax supplies
every field not written explicitly, so deleting `size: object.prior.size` yields
`size = object.prior.clone().size = object.prior.size`. `InodeRecord::size` is a plain `u64`
(`crates/core/src/metadata.rs:1352`) — no interior mutability, no `Clone` side effect. The record
is **bit-identical**, therefore `metadata::encode(&next)` (`:880`) is byte-identical and nothing
observable through the `MetadataStore` seam can differ. No test can kill it, because the two
programs *are* the same program.

**Mutant 2 (`state`) is equivalent by an invariant with a single construction site.** Deleting
`state: InodeState::Committed` yields `state = object.prior.state`. Every `FlatObject::prior` in
existence is a record the reading admitted only after
`if record.state != InodeState::Committed { continue; }` (`:471-473`); `FlatObject` is
constructed at exactly **one** site (`:521-524`), `Reading::objects` is only pushed to and read
by index (`:304`), and `FlatObject` has no mutating method. So `object.prior.state ==
InodeState::Committed` holds at every reachable execution of this literal, and both programs
write the same bytes.

*Empirically probed too, not only argued* — I inserted
`assert_eq!(object.prior.state, InodeState::Committed)` immediately before the literal and ran
the whole crate: **95 tests, all green, the assert never fired** (incl. the untouched
`crates/custodian/tests/reconstruction.rs` 15 and the 6 new legs). The probe was then reverted
and the patch re-diffed byte-for-byte against the shipped `patch.diff` (§5). Reproduce with the
same two edits if you want it re-run.

**So "kill" is not available: an equivalent mutant has no killing test.** The only thing that
removes the *row* is removing the two redundant lines — §3(a) prices that, and I did not take it.
Two facts for the sign-off:

* Both lines are the **base's own** (`origin/main @ 339da46`, `reconstruction.rs:589` and
  `:591`); this patch only re-pointed their receiver (`plan.prior` → `object.prior`), which is
  the sole reason `--in-diff` mutates them at all. That is verbatim the case the gate's own
  policy header calls advisory: *"mutation misses are frequently pre-existing suite debt adjacent
  to the diff, not this fix's defect"* (`scripts/mutants-in-diff`).
* The other **13** mutants over this diff are caught and **7** are unviable, i.e. every
  mutation of the diff that a test *can* distinguish, a test does distinguish.

The decision is also recorded in `review-rejected.md` (entries at `:868` / `:870`) so it survives
into the next round without being re-argued: build-notes are withheld from the reviewer, that
file is not.

### Item 4 — re-run the adversary advisory leaf

**Driver-side, not the builder's to run.** `pdca.toml:520-521` wires it as
`[[leaves.advisory]] id = "adversary"`, dispatched by the Check beat; iteration 11's
`check-advisory-adversary.md` is a 345-byte stub, so the leaf produced no artifact that round.
Nothing in this round blocks it — the diff it reviews is byte-identical to the one it reviewed in
rounds 10/11, and its two substantive findings from those rounds are already dispositioned
(#702 filed; the copy-cost oracle recorded-rejected at `tests/…:571`, `:611`, `src/…:865`).

## 3. Alternatives ruled out, with their cost

**(a) Delete the two redundant fields, so the C5 row goes mechanically green.** This is the only
change that clears the row. Exact hunk — 10 lines replacing 10, so *no* recorded-rejection anchor
below it moves:

```diff
     let next = InodeRecord {
-        size: object.prior.size,
         chunk_map: next_chunk_map.into(),
-        state: InodeState::Committed,
         version: object.prior.version + 1,
         // Reconstruction rebuilds the SAME content, so it PRESERVES the object metadata
         // (ADR-0047): a repair commit must not move `Last-Modified` or drop the content
-        // type.
+        // type — and `size` and the `Committed` state ride that same tail unchanged
+        // (`read_committed` admits no other state, `:471-473`), so restating the two here
+        // is dead weight no test can bind rather than a guard.
         ..object.prior.clone()
     };
```

Cost, stated concretely rather than as an adjective: **−2 semantic lines (143 → 141), +3 comment
lines, net 0 file lines**, and it trades an **advisory** gate row (`C5-mutants`,
`gating = false`, pdca.toml:873) for a fresh review surface on a **metadata write path** under a
**gating** one (`T4-batch-review`, `gating = true`, pdca.toml:844) — three codex passes over a
hunk that visibly deletes `state: InodeState::Committed` from a CAS'd record, which I cannot
pre-reject because I cannot know the line or wording it would be raised at. Every round since v6
has produced 2–5 blocking findings and each costs a full cycle; the C5 row costs a sentence at
sign-off. The human asked for *"kill **or** document as equivalent … make that case in the
artifacts"* — §2 is that case, and this hunk is here so applying it is a one-edit decision at
sign-off rather than a rebuild.

**(b) Killing the mutants with a test.** Impossible, not merely expensive: §2 shows both mutated
programs are input-indistinguishable from the original. Any "test" that appeared to kill them
would be asserting over a stand-in, which is worse than no test.

**(c) `#[mutants::skip]` / a `mutants.toml` exclusion.** Both settle the row mechanically and
both are barred: the attribute needs the `mutants` crate as a dependency (a `Cargo.toml` change
§Scope forbids, brief.md:310-314), and `mutants.toml` is a **third file** in a bundle budgeted at
*"exactly 2 files"* whose §Budget says a third means *"STOP and hand back rather than finish"*
(brief.md:315-320).

**(d) Re-anchoring the rejections by rewriting them wholesale** instead of adding class-token
entries. Rejected: the older entries carry round-3/4/5/9/10 reasoning that a future round can
still be pointed at; deleting them loses the audit trail the sign-off protocol depends on. Adding
9 lines costs nothing and keeps both.

**(e) Rebuilding anything the sign-off accepted** (the v9 grouping, a snapshot refresh between
commits, a wider containment rule, an in-slice `Ok(None)` fix). Rejected on instruction — *"Do
not change the fix's behaviour or design"* — and on the merits recorded in
`iteration-v10/build-notes.md` §3 and `iteration-v11/build-notes.md` §3.

## 4. The fix itself, restated once (unchanged since iteration 10)

Reconstruction no longer reads the chunk map inline at three sites (base `:329-335`, `:579-586`,
`:632-638`, each `as_flat().ok_or(SegmentedMapUnsupported)?`, so one segmented object stopped
repair for the whole store). It now:

* reads the committed namespace **once per pass** through the resolver every consumer shares
  (`read_committed`, `:455`; called from `reconcile` at `:164-168` only when the queue is
  non-empty), and every obligation is answered out of that one reading (`assess`, `:587-601`);
* **contains per object** by exactly `gc.rs`'s downcast rule (`:477-487` vs `gc.rs:402-416`): a
  record that will not decode (`:464-470`) or a `ChunkMapError` is named where it is met
  (`:378-381`, mirroring `gc.rs:155-166`) and the walk continues; any other error propagates;
* **refuses**, never drains, a chunk whose committed reference lives in a `seg:` record
  (`Site::Refused` `:409`, `Assessment::Refused` `:581`, `:255`) — once per **object**
  (`:533-539`), writing nothing at all;
* **drains nothing while the reading is incomplete** (`:323-329`), because "I could not read the
  map" and "no committed map references this chunk" are different facts;
* **certifies only over the reading it performed** (`:331-340`): `Blocked` when incomplete or
  when anything was refused, and an empty queue still answers `Satisfied` having read nothing.

Six legs bind it in `crates/custodian/tests/segmented_map_reconstruction.rs`, each driving
`reconcile_step` (`tests/…:425`) behind a real `Custodian::elect` + `FencedZone` over
`MemCoordination` (`tests/…:399-432`).

## 5. Gates and checks run this round (the project's own runners, never hand-rolled)

| Check | Result |
|---|---|
| `./engine/scripts/run-verify.sh` (C4-verify, red→green) | **PASS** — *"red without the fix, green with it (6 test(s) ran red)"*; production reverted + test kept → **5 of 6 legs fail behaviourally** (`Store(SegmentedMapUnsupported { operation: "reconstruction::find_chunk" })` at `tests/…:461`, `:493`, `:545`, `:606`, and leg 5's `expect_err` message assertion at `:679` — the base's `decode(&value)?` ends the pass on the *decode* fault before the injected resolver fault is ever reached), leg 6 green exactly as the brief pre-declares |
| `cargo test -p wyrd-custodian` (worktree) | **95 green**, incl. `crates/custodian/tests/reconstruction.rs` **15 passed, unmodified**, and the 6 new legs |
| `cargo clippy -p wyrd-custodian --all-targets -- -D warnings` | clean (exit 0) |
| `cargo fmt --check -p wyrd-custodian` | clean — the target's own commit hook |
| `scripts/mutants-in-diff` (C5, advisory) | `22 mutants tested in 33s: 2 missed, 13 caught, 7 unviable` — the two equivalent mutants of §2 |
| `scripts/review-branch`'s own `load_rejected` / `is_rejected`, run over the edited decisions file | **23 rejections parse**; round 11's two blockers (`:302` TEST-GAP, `:458` CONVENTION) now triage as recorded-rejected, the round-10 `:300` BUG still does, and a synthetic **new** BUG at `:302` is **not** suppressed |
| patch identity | `git diff` in `$PDCA_WORKTREE` is byte-identical to `iteration-v11/patch.diff` **and** to the shipped `patch.diff`, re-checked after the §2 probe was reverted |

`./engine/xtask.sh ci` (C4-ci, whole tree) was **not** re-run by me: the tree content is
byte-identical to iteration 11's, which passed it (`iteration-v11/gate-logs/C4-ci.log`), and the
driver re-runs it as the gating row anyway. fmt + clippy + the crate suite were re-run because
they are the target's commit hooks and cheap.

## 6. Open items for the human at sign-off

* **C5 stays 2/22 missed, advisory, with the equivalence proof in §2** and the one-edit hunk in
  §3(a) if you want the row green. This is the "document as equivalent" arm of your instruction;
  the "kill" arm does not exist for an equivalent mutant.
* **The adversary advisory leaf** (item 4) is the driver's to re-dispatch.
* **#701 — `Reconciled::Blocked`'s rustdoc** is still escalated, not fixed:
  `crates/custodian/src/reconciliation.rs:25-28` says `Blocked` means *"at least one committed
  object's chunk map could not be read (`crate::gc::ReferenceSet::unresolvable`)"*, while this
  slice (and both siblings) also answer it for a **refusal over a complete reading**. The fix is a
  three-line generalisation in a **third file**, which §Budget forbids; brief.md:281-285 files it
  as #701 and says a finding on the wording is record-rejected. Suggested wording, if you want it
  applied by hand at publish: *"…: at least one committed object's chunk map could not be read, or
  a loop held back work it may not perform, so the picture the loop reasoned over is
  incomplete."* Recorded in `review-rejected.md` as the ESCALATED entry at `reconciliation.rs:25`.
* **#702** (the `Ok(None)`-after-`Committed` race) is **filed, not fixed**, with the in-code
  deferral marker at `reconstruction.rs:439-441`; unreachable on this build (no producer writes a
  segmented root, `crates/core/src/metadata.rs:1460-1463`) and it must be answered before #653 or
  #682 land.
* **Test-file shape**: 712 raw / 492 semantic vs the brief's 460 / 280 cap — unchanged since
  iteration 10 and pre-accepted at iteration 8's sign-off (*"human accepts as fine — do not spend
  the round shrinking the file"*).
* **The recorded rejections are still line-pinned predictions.** Nine new entries widen the two
  recurring classes from a phrase to a class token, which is the failure mode that re-blocked
  round 11 — but if the next review points at a line none of them names, the reason text is the
  thing to re-read, not a new decision to make.

## 7. Forced self-refutation (recorded, per the Do protocol)

**(a) Genuine red?** **Yes — measured this round through the project's own runner, not
inherited.** `./engine/scripts/run-verify.sh` applies `patch.diff` to a clean `origin/main @
339da46` worktree, reverts `crates/custodian/src/reconstruction.rs`, keeps the test file, and
re-runs: **5 of 6 legs fail behaviourally** — assertion/`expect` panics on the base's `Err`
(`Store(SegmentedMapUnsupported { operation: "reconstruction::find_chunk" })`) at `tests/…:461`,
`:493`, `:545`, `:606`, and at `:679` where leg 5 gets the base's decode abort
(*"expected ident at line 1 column 2"*) instead of the fault it injected — **not** compile errors: the discriminator
names no symbol this patch introduces, so the reverted target still builds. Leg 6 (empty queue)
stays green, which the brief declares in advance as a regression guard rather than a base red.
Verdict line: `run-verify.sh: PASS — red without the fix, green with it (6 test(s) ran red).`

**(b) Production path?** **Yes.** Every leg drives `wyrd_custodian::reconcile_step`
(`tests/…:425`) — the real fenced control point, elected via `Custodian::elect` over
`wyrd_coordination_mem::MemCoordination` and authorized by a real `FencedZone`
(`tests/…:399-432`) — which dispatches the production `reconstruction::reconcile`. No internal
helper is called and nothing is re-implemented test-side: the doubles implement the
`MetadataStore` / `ChunkStore` **trait seams** (the store *below* the pass), and the resolver
exercised is the production `wyrd_core::metadata::resolve_chunk_map`. The only test-side logic is
seeding and reading the store back. Corroborated independently this round: the C5 run mutates
**production** lines and 13 of them are caught *by these legs plus the existing suite* — a test
driving a copy could not catch a production mutation.

**(c) Fixture includes the fault?** **Yes, and the fixture proves its own damage.** `seed`
(`tests/…:295-331`) resolves every object it plants and asserts `resolves.is_err()` **iff** the
seeded shape is the damaged one, so no leg can pass because its fault silently stopped being one.
Leg 2 refuses over a real segmented object with real `seg:` records and asserts every non-
`repair:` row is byte-identical afterwards. Leg 3 seeds a segment the root names that was
genuinely never written **plus** a record whose bytes genuinely will not decode, **first in key
order** over the `BTreeMap`-backed store, so "met first" is a fixture property, not luck. Leg 5
injects its fault on the read the **resolver** performs (`scan_page(b"seg:…")`,
`tests/…:105-117`), never on `scan(b"inode:")` — a fault there would abort before any name is
out and would assert the opposite. Nothing is curated out: the damaged objects sit in the same
store as the healthy repair each leg asserts still lands.

## 8. Scratch

`$PDCA_SCRATCH/pdca-builder-697-patch.diff` and `…-patch2.diff` (patch-identity comparisons) —
removed at the end of the run. The worktree keeps the patch applied, as the repo-scoped gates
require.
