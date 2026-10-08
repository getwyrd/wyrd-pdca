# Build notes — #721 · segmented-repair-completes-through-repoint (iteration 3)

*(Withheld from the reviewer; written for the human at sign-off.)*

Everything below is against the cycle worktree `$PDCA_WORKTREE`
(`/home/eddie/wyrd/wyrd.pdca-wt-l0`, `origin/main` @ `a801997`; the brief was authored at
`92e1b4b` and every citation it makes still resolves).

---

## 0. What iteration 3 changed, and why (the carry-forward, item by item)

Iteration 2 passed C1–C5, T1 and T2 and was rebuilt for **two runtime findings** (T3/T5 and the
gating T4 batch) plus one finding this child's brief puts in another slice. This iteration keeps
that patch's design — no reviewer faulted the shape — and closes exactly those findings.

| carry-forward / batch finding | what iteration 3 does |
| --- | --- |
| **T5 + T4 BUG** `metadata.rs:2844` — "a flat repoint on an inode whose version is `u64::MAX` panics in debug or wraps to 0 in release, violating generation monotonicity instead of refusing the move" | `crates/core/src/metadata.rs:2844` — `generation.version.checked_add(1)`, and exhaustion is a **typed** `ChunkMapError::VersionExhausted { version }` (`:647`, Display at `:766`). Not a `Conflict` (that would re-plan forever) and not a saturate (same fault standing still). Named negation **(a)** in §3, demonstrated. |
| **T5 + T4 CONVENTION** `metadata.rs:2882` — "a freshly read segment that fails structural decoding is collapsed into `Repoint::Conflict`, violating the requirement that structural invariants surface as errors and causing persistent corruption to be misreported as a transient race" | `crates/core/src/metadata.rs:2892` — `decode_segment_record(segment.index, &bytes)?`: the typed `SegmentRecordUndecodable` propagates. The caller **contains it to the object** (`crates/custodian/src/reconstruction.rs:311-327`) by the same downcast rule `read_committed` already uses (`:490-500` on the base): named on the durability seam, obligation kept, nothing drained, pass answers `Blocked`. Named negation **(b)** and **(g)** in §3, demonstrated. |
| **T4 TEST-GAP** `metadata.rs:2779` — "lacks the seeded Tier-0 DST regression required for new concurrent paths" | Recorded-rejected again in `results/issue_721/review-rejected.md`, now also at the `crates/core/src/metadata.rs` locations the class was reported at this round (`is_rejected` matches an exact `file:line`, `scripts/review-branch:352`, and the file's lines move every iteration). Substance unchanged and it is **the brief's own carve-out**, not a builder opinion: #722 owns the repoint-versus-supersede DST property and this child may not touch `crates/dst/tests/custodian.rs`. **That file is yours to overrule** — delete the rows. |

Nothing from iteration 2 was re-submitted unchanged: the primitive gained a checked generation
advance and a typed corruption fault it did not have, the caller gained a containment arm it did
not have, and the discriminator gained a **seventh leg** that only that containment can pass.

---

## 1. The change

Four files, exactly the brief's set.

| file | what |
| --- | --- |
| `crates/core/src/metadata.rs` | the placement move: `ChunkMapError::VersionExhausted` (`:647`), `segment_value_ceiling_crossed` (`:397`), `Repoint` (`:2758`), `repoint_chunk` (`:2822`), `covers` (`:2928`), `chunk_at` (`:2938`), plus in-crate `#[cfg(test)]` tests for the addressing helpers, both ceilings, the batch the move hands back, and **both typed faults** |
| `crates/custodian/src/reconstruction.rs` | the repair pass completes through it: `Site`/`Object` lose the flat-only shape, `Assessment::Refused` / `Reading::refused` / `emit_refused` are gone, `repair_chunk` (`:822`) prepares the move (`:889`), and `reconcile` contains the move's typed faults (`:311-327`) |
| `crates/custodian/tests/segmented_map_repoint.rs` | **new**, the discriminator — seven legs |
| `crates/custodian/tests/segmented_map_reconstruction.rs` | the forced edit: that file's own leg 2 asserted the refusal this child removes |

**Budget.** 183 added semantic lines of non-test production code (`metadata.rs` 114 +
`reconstruction.rs` 69; non-blank, non-comment, outside `#[cfg(test)]`) against a cap of 250.
`patch.diff` is **97 231 bytes = 94.9 KB**, inside the brief's 95 KB cap — see §6, which is this
iteration's one budget judgement and is worth your eye.

### The two fixes, in full

```rust
// crates/core/src/metadata.rs:2844 — the flat arm
let Some(version) = generation.version.checked_add(1) else {
    return Err(ChunkMapError::VersionExhausted { version: generation.version }.into());
};
```
The wrap is the worse half of the finding, not the panic: this CAS pins a record's **exact
bytes**, so a version that wrapped to 0 re-mints a byte string an older writer may still be
holding as its precondition — an ABA that lands a plan built from a long-retired generation.
Negation (a) shows exactly that batch being prepared once the check is removed.

```rust
// crates/core/src/metadata.rs:2892 — the segmented arm
let record = decode_segment_record(segment.index, &bytes)?;
```
An **absent** row stays a `Conflict` (that is the retirement race — records are deleted after the
root moves, `0016:2452-2462`). A **present** row that will not decode is not that race: a
retirement deletes records, it never scribbles over them. `SegmentRecordUndecodable`'s own doc
already states the contract this now honours — "a maintenance pass classifies by that distinction
— an object-local fault contains to the object, a store fault ends the pass" (`metadata.rs:612`).

The caller therefore *contains* rather than propagates, reusing `Reading::contain` so the rule and
its audit row have one implementation:

```rust
// crates/custodian/src/reconstruction.rs:313
Err(err) => match err.downcast::<ChunkMapError>() {
    Ok(fault) => {
        reading.contain(&metadata::inode_key(inode), &fault.to_string());
        RepairOutcome::Aborted          // offsets the up-front `emit_repaired`; see below
    }
    Err(err) => return Err(err),
},
```

Consequences, all asserted by leg 7: the object is named on the same `unresolvable-chunk-map`
audit row every other maintenance loop publishes (`gc.rs:564-567`, `restore.rs:827-830`,
`scrub.rs:230-233`), the obligation stays queued, **nothing is drained** (the pass's reading now
has a hole in it), and the pass answers `Blocked`. Without it the pass answers **`Satisfied`** —
negation (g) — i.e. it tells an operator redundancy is restored while a corrupt record blocks the
repair forever.

### The metric identity the containment had to keep

`reconcile` emits `reconstruction_repaired` **up front**, once per dispatched plan, and every
non-success offsets it so that `repaired − conflict − aborted − ceiling_refused` still counts true
successes (`crates/custodian/src/reconstruction.rs:265-286`, ADR-0011 §2 names this file as the
source of truth for that emission). A contained fault is a dispatched repair that did not
complete, so leaving it un-offset would have inflated the success count by exactly the number of
damaged objects — a count-based signal passing while the property fails, which is the rubric's own
*absent-or-unsupported-entries* class. I caught this while self-reviewing the arm: it now answers
`RepairOutcome::Aborted`, so the existing arm emits the offset, and `RepairOutcome::Aborted`'s doc
(`:786`) records that one of its two causes is **not** transient and is named by
`Reading::contain` where it is met. The severity signal stays separate and warn-level
(`reconstruction_unresolvable_records` + the `unresolvable-chunk-map` audit row).

---

## 2. Reaching the read→prepare window (unchanged from iterations 1–2, and still the trick)

The two reads are on **different `MetadataStore` methods**: the resolver reads the group's range
with `scan_page` (`read_group_range`, `metadata.rs:2466`) while the move's `get` is the only `get`
anyone performs on a `seg:` key. The double fires the racing batch **after answering the `seg:`
page** — after the plan's read, before the move's. Counting `get`s would land the race after the
move captured its CAS bytes and would quietly invert leg 2. Leg 4's root flip fires on the way
**into** `commit`. Legs 2, 3, 6 and 7 carry `meta.raced()` as a fixture self-check.

---

## 3. Named negations — demonstrated, not asserted

Each run edits the patched tree, runs the suite through the project's own runner, and restores the
file from a scratch copy (`$PDCA_SCRATCH/pdca-builder-721-negations/`). All seven were actually
run in this session; (a), (b) and (g) are this iteration's new pins.

**(a) NEW — `checked_add(1)` → `wrapping_add(1)` (`metadata.rs:2844`) → the in-crate test goes RED**,
and the failure prints the defect itself:

```
test metadata::tests::a_generation_with_no_successor_is_refused_never_wrapped ... FAILED
the move must fail closed on this object's own fault: Prepared(WriteBatch { preconditions: [ …
  expected: Some(b"…,\"version\":18446744073709551615}") ], puts: [ … b"…,\"version\":0}" ] })
```

**(b) NEW — the typed decode fault → `Repoint::Conflict` (the iteration-2 code) → RED twice.**

```
test metadata::tests::a_segment_record_that_will_not_decode_is_a_fault_not_a_race ... FAILED
the move must fail closed on this object's own fault: Conflict

test a_segment_record_torn_under_the_move_is_contained_never_re_planned ... FAILED
assertion `left == right` failed: a pass that met a record it could not read certifies nothing
  left: Satisfied     <- the pass certifies over a record it could not read
 right: Blocked
```

**(g) NEW — the caller counts the fault but never contains it** (delete the `reading.contain(…)`
call at `reconstruction.rs:322`, keep the `RepairOutcome::Aborted`) → leg 7 RED, `Satisfied`
where `Blocked` is required. Re-run in this shape after the counter fix below; the first attempt
did not compile — with the call gone `reading` need not be `mut` and the workspace denies
warnings — so it was re-run with the `mut` dropped, which is the shape `cargo mutants` produces.

**(c) Delete the root/record extent comparison (`metadata.rs:2897`) → leg 6 + its in-crate test RED.**

```
test a_racing_rewrite_that_leaves_the_roots_segment_table_behind_is_a_conflict ... FAILED
assertion `left == right` failed: the segment record still holds EXACTLY the bytes it held before
test metadata::tests::a_segment_record_that_no_longer_matches_the_roots_table_is_never_rewritten … FAILED
```

**(d) Delete the `chunk == prior` equality from `chunk_at` → leg 3 + its in-crate test RED**
(re-run with `let _ = prior;`, the mutation cargo-mutants generates, because the bare deletion
leaves `prior` unused and the workspace denies that).

**(e) Drop the root-generation `require` from the segmented arm → leg 4 RED.**

**(f) Widen the segment budget from `MAX_ROOT_VALUE_BYTES` to `MAX_VALUE_BYTES` → leg 5 + its
in-crate boundary test RED.** (Deleting the *call* instead does not compile: the private helper
becomes dead code and the workspace denies warnings — recorded because a compile failure is not a
red test.)

---

## 4. Refutation of my own test (the three forced questions)

**(a) Genuine red?** Yes — through the project's own gate, not by hand:

```
$ PDCA_BUNDLE=results/issue_721 ./engine/scripts/run-verify.sh
run-verify.sh: GREEN — cargo test -p wyrd-custodian --test segmented_map_repoint (fix applied)
  test result: ok. 7 passed; 0 failed
run-verify.sh: RED — (production reverted, test kept)
  a_segmented_objects_under_replicated_chunk_is_repaired_in_its_own_record --- FAILED (Blocked, want Changed)
  a_racing_move_of_a_sibling_chunk_in_the_same_segment_record_is_merged   --- FAILED (Blocked, want Changed)
  test result: FAILED. 5 passed; 2 failed
run-verify.sh: PASS — red without the fix, green with it (7 test(s) ran red).
```

Read the count as a count, as the brief asks: the gate reports how many tests **ran** in the red
leg (7), not how many failed (2). Legs 3–7 pass on the base by construction — pre-fix the pass
refuses, so it also writes nothing — and are bound by the mutation oracle in §3 instead.

**(b) Production path?** Yes. Every leg drives `wyrd_custodian::reconcile_step` — the real fenced
control point, with a real `Custodian` leader and `FencedZone` — over the production
`MetadataStore` / `ChunkStore` seams, and asserts by **reading the store back**. Nothing is mocked
but the two backends; the fragments are real EC shards through the production encoder, so the
loop's checksum/identity verification is genuine. The in-crate tests drive `repoint_chunk` over a
**real** redb store. No assertion names a symbol this patch adds (required: the RED leg reverts
production).

**(c) Fixture includes the fault?** Yes. The under-replicated chunk really is missing fragment 1
(never written; its D server is in neither the fleet nor the topology); the racing writers really
land inside the window (`meta.raced()` asserted in legs 2, 3, 6 and 7 — a leg cannot pass because
the race silently never happened); leg 5's record is seeded **on** the V/2 boundary with measured,
self-checked padding; leg 7's torn bytes are asserted undecodable *before* the pass runs, so the
leg cannot pass against bytes that happened to parse.

---

## 5. Three phrasings that are deliberate, and worth your eye at sign-off

**Legs 3, 4 and 6 assert `outcome != Reconciled::Changed`, not "answers `Blocked`".** Post-fix a
lost race answers `Satisfied`: `hole` is `reading.incomplete || ceiling_refused`, and a lost CAS is
neither — the **base's own, unchanged** semantics for a flat lost CAS. Pre-fix the same legs answer
`Blocked`. The assertion binding in both worlds is "it never reports a change it did not make".
Legs 5 and 7, where the pass does open a hole, assert `Blocked` outright.

**Leg 4 is vacuous on the base** and cannot be made otherwise: pre-fix the pass never commits, so
the armed root flip never fires and `meta.raced()` cannot be asserted there. Post-fix the leg
passes *because* the flip fired, and negation (e) shows it is binding.

**A contained fault suppresses this pass's drain.** `Reading::contain` sets `incomplete`, and the
drain of no-op obligations is gated on it — so a pass that met a damaged record drains nothing,
exactly as when the *reading* met one. That is the conservative direction (an obligation is never
discarded on doubt) and it keeps `emit_unresolvable`'s standing operator message true ("this pass
drains NOTHING and certifies NOTHING until that record is repaired"). The audit row carries
`fault=<the typed message>`, so the `VersionExhausted` case is named precisely even though it
shares the `unresolvable-chunk-map` action string with the undecodable one.

---

## 6. The size cap — this iteration's one budget judgement (please read)

The brief caps `patch.diff` at **95 KB**; the driver measures `patch_bytes / 1024`
(`src/pdca_harness/size_signal.py:363`), so the cap is 97 280 bytes and the backstop 102 400.
Iteration 2 shipped **97 226** — 54 bytes under. The carry-forward then *required* ~140 more lines
(two production fixes, two in-crate tests for the new typed faults, and leg 7 with its containment
evidence), which cost **+10.6 KB**: the naive iteration-3 patch measured **107 814** at `-U1`.

What I did, and the cost of each option so you can overrule me:

1. **Prose compression (chosen, ~10.8 KB).** The added *comment* volume was cut from **522 lines
   / 41.9 KB** to **~390 lines / ~31 KB** — restatements, anticipatory defences and per-leg banner
   comments duplicating the test names. Every distinct claim survives, said once. The patch is
   still ~28 % comment lines.
2. **Deduplicating the three conflict legs (chosen, ~1.8 KB).** Legs 3, 6 and 7 shared ten lines
   of arm-run-assert boilerplate each; they now share `raced_and_lost(…)`
   (`segmented_map_repoint.rs:373`) and keep their own names, docs and extra assertions. Better
   code, not just smaller.
3. **`-U0` instead of `-U1` (rejected, would have saved 2.9 KB).** `scripts/review-branch --bundle`
   feeds **patch.diff itself** to the reviewers (`scripts/review-branch:194`), so context lines are
   the reviewers' only surrounding code. Buying 2.9 KB by making every changed line context-free
   trades review accuracy for a byte count; and it would not have been enough on its own
   (`-U0` measured 101 342 when `-U1` measured 104 224).
4. **Dropping evidence to fit (rejected, with the number).** The in-crate segmented tests
   (`metadata.rs:3273`+, 85 lines) or leg 7 (35 lines incl. `torn`) would have paid for the
   overshoot in one cut — and each is precisely what a carry-forward demanded. Leg 7 is also the
   *only* coverage of the new containment arm: negation (g) shows the arm is otherwise unkillable,
   so dropping it would hand C5 a surviving mutant and the reviewer an untested error path.
5. **Shipping over the cap (rejected).** ~99–100 KB stays under the driver's backstop, but the
   T2 reviewer measured the byte count against the brief's cap explicitly last round
   ("The patch is 97,226 bytes…"), so an overshoot buys a guaranteed review round.

Final: **97 231 bytes = 94.9 KB**, 4 files, no content removed that any finding asked for.

---

## 7. Rubric self-review (the target's own `## Review rubric & protocol`)

* **One clock per lifecycle (ADR-0009)** — this patch adds **no clock read**. `repair_chunk` still
  stamps orphan marks from the pass's `now_millis`, unchanged.
* **Narrow trait seams / dependency direction (ADR-0010, ADR-0016)** — the primitive lives in
  `core` beside the record shapes it rewrites and takes `&dyn MetadataStore`; `custodian` gains no
  backend or on-disk-format knowledge, and recovers the primitive's verdict through the same
  `downcast::<ChunkMapError>()` seam it already uses. No new dependency anywhere.
* **Metadata validation boundaries (ADR-0045)** — this iteration's decode fix *is* that rule:
  structural invariants stay at decode (`SegmentRecord::from_wire`) and now **surface as an error,
  never as a value**; the root/record extent agreement is *contextual* and therefore sits at the
  operation boundary, strict because the boundary is a maintenance write.
* **No DST-reachable shared mutable global state (ADR-0035)** — none added; the statics gate in
  `cargo xtask ci` is green.
* **`#![forbid(unsafe_code)]`** — no new crate root; the new test file carries it.
* **Docs currency** — no port, API operation, RPC, CLI flag or persisted field changes: a repoint
  writes the same `inode:` / `seg:` shapes the base already stores. The two closest peers (#697
  `1f871ce`, #710 `d2609b2`, which likewise added a public helper to `core::metadata`) touched no
  docs either. Proposal 0016 is a draft and the brief forbids touching it.
* **Serialization identity** — the flat arm rebuilds via `..generation.clone()`, so the ADR-0047
  `skip_serializing_if` fields survive decode→encode and legacy records stay CAS-able (asserted
  in-crate). The segmented CAS pins the **stored bytes** it read, not a re-encode.
* **Absent or unsupported entries** — every anomaly is an explicit `Repoint::Conflict`, a
  `Repoint::Refused`, or a **typed error**; nothing is silently skipped and no obligation is
  drained on doubt.
* **Transactions** — one batch, prepared before any fragment write; no early return over a live
  transaction (the store seam has none to roll back).
* **Await discipline** — one added `await` (`store.get`), bounded by the `MetadataStore` contract;
  no spawned task, no unbounded stream.
* **Probes and readiness / protocol input / grammar strictness** — surfaces untouched, except that
  a **torn** stored value is now an explicit error rather than a value (the *protocol input* class,
  covered by leg 7).
* **Test fidelity** — conformance contracts untouched and green; the DST item is the recorded
  rejection in `review-rejected.md` (#722 owns it), and `cargo xtask ci` (which includes the
  conformance suites) is green on the patched tree.
* **Deferrals are settled / out of scope** — the stranded destination fragment on a lost CAS is the
  pre-existing, tracked **getwyrd/wyrd#723**; the brief forbids re-arguing it and forbids editing
  the two "collectable garbage" comments #723 owns. Neither was touched.

---

## 8. Alternatives ruled out (with their cost)

* **Answer the undecodable record with `Repoint::Conflict` and just log it** (+3 lines). That is
  the iteration-2 code the batch review rejected, and negation (b) shows what it costs: the pass
  answers `Satisfied` over an object whose repair can never complete. Rejected on the finding.
* **Propagate the typed fault out of `reconcile` instead of containing it** (−12 lines). One
  damaged object would end the pass for *every* object, which is exactly the containment
  `read_committed:498-508` exists to avoid, and `SegmentRecordUndecodable`'s own doc names that
  distinction as the reason it is typed. Rejected on blast radius, not size.
* **A dedicated `Repoint` variant / `RepairOutcome` arm / metric for each new fault** (+~35 lines
  and a new counter each). Both faults are the same operator class — "this object's records cannot
  be used by maintenance; a human must look" — and reuse of `emit_unresolvable` puts them on the
  action string gc, restore, scrub and desired-state already publish for it. Rejected as new
  surface for no new information; the typed message is emitted in the `fault` field.
* **Saturating or wrapping the version** (0 lines). Both keep a CAS that can match twice; see §1.
* **Fixing the same `prior.version + 1` in `commit_chunk_map`** (`metadata.rs:1769-1797`, +6
  lines). Pre-existing and explicitly out of scope — the brief says "`commit_chunk_map` is not
  what this child changes". Worth a follow-up issue at sign-off if you want the class closed.
* **Validating the extent at decode instead** (`SegmentRecord::from_wire`). Not possible there:
  the record does not know its root, so the check needs the caller's `SegmentRef` — which is
  precisely ADR-0045's reason for contextual checks at the operation boundary.
* **Pinning the whole resolved segment record's bytes.** Not implementable — `ResolvedChunkMap`
  has no per-segment bytes — and faked by re-reading at resolve time it makes leg 2 red.
* **Reusing `flat_value_ceiling_crossed` for the segment arm** (0 lines). It writes records in the
  50 001..100 000 band no publication could produce; the named helper is 3 lines over the existing
  constant (a second *name*, not a second *value*).

---

## 9. Scope held / deliberately not done

* **No DST leg, no rebalance, no `crates/dst/tests/custodian.rs`** — #722's, per the brief.
* **No pre-mark, no drain fence** (0016 X47): out of scope, and the stranded-fragment leak they
  close is the pre-existing, tracked getwyrd/wyrd#723. Not re-argued; the two comments #723 owns
  were left alone. One consequence worth noting: the primitive prepares its batch **before** the
  fragment writes, so a conflict or a contained fault here writes no fragment either — this
  slice's failure paths are strictly quieter than the flat one's, never louder.
* `commit_chunk_map` and its segmented refusal untouched; the flat arm reuses its CAS idiom.
* Four files, no fifth; no new dependency; no ADR/proposal/conformance-vector change.

---

## 10. Gates run here (advisory to your sign-off; Check re-runs them)

* `./engine/scripts/run-verify.sh` — **PASS** (2 of 7 red without the fix, 7/7 green with it).
* `scripts/mutants-in-diff` (C5) — **41 mutants tested in 80 s: 25 caught, 16 unviable, 0 missed.**
* `./engine/scripts/run-diff-cov.sh` — **PASS, 96.4 %** (376 of 390 instrumentable changed lines,
  floor 80 %). The misses are two `panic!` arms of in-crate test helpers, two `matches!` expansions
  inside passing asserts, the store-fault propagation arm of the new containment match
  (`reconstruction.rs:319` — no double injects a store fault under the write; its peer at `:509`
  is untested on the base for the same reason), and two pre-existing arms.
* `cargo xtask ci` — fmt, clippy (`-D warnings`), build, **168 test suites**, statics gate and the
  conformance suites all green on the **exact shipped tree**; **`cargo deny check` FAILS on one
  advisory**, see below. Every row in this list was run against that same tree.
* `cargo xtask dst` — green (10 suites, 0 failures) on the patched tree; nothing there asserted
  the removed refusal, and the new containment arm breaks no simulation.
* `cargo fmt --all` — clean (run after every edit; the patch is formatter-stable, so the target's
  own commit hooks will not reject it).

### The one red: `cargo deny check advisories` — pre-existing, not this patch's

`RUSTSEC-2026-0258` ("h2 unbounded empty DATA frames", `h2 0.4.15` via `hyper`/`tonic`). This patch
touches **no `Cargo.toml` and no `Cargo.lock`** — four `.rs` files only — and the same check fails
on the **clean base checkout**:

```
$ cd /home/eddie/wyrd/wyrd-verify   # a801997, pristine
$ cargo deny check advisories
… advisories FAILED
```

So it is `origin/main`'s state, freshly published, and it will fail for every bundle until the
dependency moves. Worth an issue at sign-off; nothing in this slice can close it.

---

## 11. One convention worth stating: which line numbers the new comments cite

Every in-file `:NNNN` citation inside the added code is a **base** (`origin/main` @ `a801997`)
line number — `commit_chunk_map`'s CAS idiom `:1769-1797`, `ResolvedChunkMap` `:2294-2300`, the
resolver's read-side refusal `:2493`, `read_segments`' extent check `:2582-2589`,
`read_committed`'s downcast `:490-500`. That is the brief's own numbering and the numbering the
*adjacent, unmodified* comments in the same doc blocks already use (e.g.
`crates/core/src/metadata.rs:378` cites `:2493` for the same target), so the file stays internally
consistent; a patch that inserts ~230 lines cannot renumber the base comments it does not touch.
Iteration 2 shipped one stale citation from its own line numbering (`:2603-2610`, which matched
neither the base nor the merged file) — corrected here, and every remaining citation was checked
against `git show HEAD:<file>` in this session.

## 12. Nothing is deferred off-Check

No external dependency was missing: no Docker host, no live cluster, no env var, no protoc. The
whole discriminator runs headless over in-memory doubles plus a real in-memory redb store, in
milliseconds. Every gate row above was run in this session, in the cycle worktree.
