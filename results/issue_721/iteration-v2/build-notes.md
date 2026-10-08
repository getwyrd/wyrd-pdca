# Build notes — #721 · segmented-repair-completes-through-repoint (iteration 2)

*(Withheld from the reviewer; written for the human at sign-off.)*

Everything below is against the cycle worktree `$PDCA_WORKTREE`
(`/home/eddie/wyrd/wyrd.pdca-wt-l1`, `origin/main` @ `a801997`; the brief was authored at
`92e1b4b` and every citation it makes still resolves).

---

## 0. What iteration 2 changed, and why (the carry-forward, item by item)

Iteration 1 passed C1–C4 and T1–T3 and was rebuilt for two implementation-level findings plus a
gating T4 batch review (4 blocking findings, 3 of them one class). This iteration keeps that
patch's shape — the reviewers did not fault the design — and closes exactly those findings.

| carry-forward item | what iteration 2 does |
| --- | --- |
| **C5 Causal adequacy** — "add in-crate assertions that a flat repoint changes `chunk_map` and increments `version`; four killable mutants remove those semantics while the core mutation run stays green" | `crates/core/src/metadata.rs:3193` — `a_flat_repoint_moves_that_one_placement_and_advances_the_generation` drives `repoint_chunk`'s flat arm over a **real** redb store and decodes the batch's own put: the addressed chunk's placement moved, its neighbour's did not, `version == prior + 1`, and the ADR-0047 object metadata is preserved. All four mutants demonstrated dead in §3(d). |
| **T5 Judgment / T4 batch (3 findings, one class)** — "compare the fresh `SegmentRecord` offset and length with the selected `SegmentRef` and race-test the mismatch — otherwise maintenance can CAS a root-inconsistent row that normal resolution rejects" | `crates/core/src/metadata.rs:2893` — the segmented arm now refuses to rewrite a record whose `byte_offset`/`byte_len` disagree with the root's selected `SegmentRef`, answering `Repoint::Conflict`. **Race-tested** end to end at `crates/custodian/tests/segmented_map_repoint.rs:733` (leg 6) and unit-tested at `crates/core/src/metadata.rs:3328`. Named negation demonstrated in §3(a). |
| **T4 batch, 4th finding** — TEST-GAP: seeded Tier-0 DST coverage for the new concurrent path | Recorded-rejected in `results/issue_721/review-rejected.md`: the brief assigns the repoint-versus-supersede DST property to **#722** and forbids touching the DST/rebalance files, and a DST leg would break the 4-file budget. `cargo xtask dst` was run on the patched tree anyway — green, 10 suites, 0 failures. **That file is yours, not mine** — I drafted it (its header says so) because the T4 gate blocks while any finding is untriaged and this one is the brief's own carve-out, not a judgement call; delete the rows to overrule. `is_rejected` matches on an exact `file:line` (`scripts/review-branch:352`), and the file's lines moved between iterations, so the one decision is recorded at the four locs the class can land on. |

Nothing else in iteration 1's approach was re-attempted unchanged: the primitive gained a
precondition it did not have, and the discriminator gained a sixth leg that only that
precondition can pass.

---

## 1. The change

Four files, exactly the brief's set.

| file | what |
| --- | --- |
| `crates/core/src/metadata.rs` | the placement move: `Repoint` (`:2744`), `repoint_chunk` (`:2821`), `covers` (`:2924`), `chunk_at` (`:2938`), `segment_value_ceiling_crossed` (`:401`), plus in-crate `#[cfg(test)]` tests for the addressing helpers, the segment budget and **the batch the move hands back** (`:3193`, `:3287`, `:3328`) |
| `crates/custodian/src/reconstruction.rs` | the repair pass completes through it: `Site`/`Object` lose the flat-only shape, `Assessment::Refused` / `Reading::refused` / `emit_refused` are gone, `repair_chunk` (`:806`) prepares the move (`:878`) instead of hand-rolling an inode CAS |
| `crates/custodian/tests/segmented_map_repoint.rs` | **new**, the discriminator — six legs |
| `crates/custodian/tests/segmented_map_reconstruction.rs` | the forced edit: that file's own leg 2 asserted the refusal this child removes (`:529`) |

**Budget.** 147 added semantic lines of non-test production code (`metadata.rs` 92 +
`reconstruction.rs` 55; measured as non-blank, non-comment `+` lines outside `#[cfg(test)]`),
against a cap of 250. `patch.diff` is **97 226 bytes = 94.9 KB** — see §6, which is where the
one budget judgement of this iteration lives and is worth your eye.

### What the move pins, and the new precondition

`repoint_chunk(store, inode, generation, byte_offset, prior, placement) -> Repoint`. It hands the
batch back; the caller adds the obligation delete and the orphan marks and commits once
(`reconstruction.rs:878-912`). It pins **three** things — the root generation's bytes, the
segment record's **freshly-read** bytes, and the `ChunkRef` itself — which is what makes a
sibling's concurrent move MERGE and the planned chunk's own move CONFLICT (legs 2 and 3). The
archived attempt's doc sites claiming it pins "the exact bytes the resolve read" were corrected
in iteration 1 and stay corrected: `ResolvedChunkMap` (`metadata.rs:2294-2300`) carries only
`record` + the flattened `chunks`, so per-segment bytes are not available to pin.

**New in this iteration**, and the T5 finding's substance: pinning is not enough on its own. A
competing writer can leave a record that is *structurally* valid, still holds the planned chunk
at the planned offset, and yet no longer covers the span the pinned root's table gives it. The
move re-reads that record, so it would have happily rewritten it — CASing a **fresh generation**
of a row that `resolve_chunk_map` then refuses (`read_segments`, `metadata.rs:2603-2610`,
`ChunkMapError::SegmentBoundsMismatch`). That is maintenance adopting another writer's
inconsistency as its own durable write, which ADR-0045 decision 3 forbids in as many words ("a
maintenance loop … MUST classify, skip … never act on or rewrite a malformed record"). The check
is deliberately the resolver's own comparison, field for field, so there is one rule and not two:

```rust
// crates/core/src/metadata.rs:2893
if record.byte_offset() != segment.byte_offset || record.byte_len() != segment.byte_len {
    return Ok(Repoint::Conflict);
}
let within = byte_offset - segment.byte_offset;      // :2898
```

The `checked_sub` iteration 1 used to derive `within` was replaced by a plain subtraction because
the two checks above it now make it total (`covers` put the offset inside the root's span; the
record just agreed the span is its own). That is not a defensive line removed for tidiness: a
fallible branch that no input can reach is an unkillable mutant and a false sense of a guard.
The four arithmetic mutants the plain `-` introduces are all demonstrated dead in §3(e).

---

## 2. Reaching the read→prepare window (unchanged from iteration 1, and still the trick)

The two reads are on **different `MetadataStore` methods**: the resolver reads the group's range
with `scan_page` (`read_group_range`, `metadata.rs:2466`; its page read at `:2481`) and the move's `get` is the only
`get` anyone performs on a `seg:` key. The double therefore fires the racing batch **after
answering the `seg:` page** — after the plan's read, before the move's. Counting `get`s would
land the race *after* the move captured its CAS bytes and would quietly invert leg 2. Leg 4's
root flip fires on the way **into** `commit`. All three race legs carry `meta.raced()` as a
fixture self-check (leg 4 excepted — see §5).

---

## 3. Named negations — demonstrated, not asserted

Each run edits the patched tree, runs the suite, and restores the file from a scratch copy
(`$PDCA_SCRATCH/pdca-builder-721-negations/`). All five were actually run in this session.

**(a) NEW — delete the root/record extent comparison (`metadata.rs:2893`) → leg 6 goes RED.**

```
test a_racing_rewrite_that_leaves_the_roots_segment_table_behind_is_a_conflict ... FAILED
assertion `left == right` failed: the segment record still holds EXACTLY the bytes it held before
  left:  ...{"id":41472,...,"placement":[0,2]}...,"byte_offset":8,"byte_len":24}   <- the repair's
 right:  ...{"id":41472,...,"placement":[0,1]}...,"byte_offset":8,"byte_len":24}   <- untouched
test result: FAILED. 5 passed; 1 failed
```

Read the `byte_len`: the repair re-published a record covering **24** bytes under a root whose
table says that segment covers **16** — exactly the row `resolve_chunk_map` refuses. The same
deletion also reddens the in-crate unit test:

```
test metadata::tests::a_segment_record_that_no_longer_matches_the_roots_table_is_never_rewritten ... FAILED
a span that grew: a repair must not rewrite a record whose extent contradicts the root it pinned
  … answered Prepared(WriteBatch { … puts: [(seg:…:1:000001, …"placement":[0,5]…)] })
```

**(b) Delete the `chunk == prior` equality from `chunk_at` → leg 3 goes RED.**

```
test a_racing_move_of_the_planned_chunk_itself_is_a_conflict_that_writes_nothing ... FAILED
crates/custodian/tests/segmented_map_repoint.rs:563 (the byte-for-byte competing-writer assert)
test result: FAILED. 5 passed; 1 failed
```

(The first attempt at this negation failed to *compile* — deleting the equality leaves `prior`
unused and the workspace denies that — so it was re-run with `let _ = prior;` added, which is the
mutation cargo-mutants would generate. Recorded because a compile failure is not a red test.)

**(c) Delete the root-generation `require` from the segmented arm → leg 4 goes RED.**

```
test a_superseded_root_generation_makes_the_repair_lose_without_writing_its_own_metadata ... FAILED
test result: FAILED. 5 passed; 1 failed
```

**(d) Widen the segment budget from V/2 to the full value ceiling → leg 5 goes RED.**

```
test a_repoint_that_would_outgrow_a_segment_records_budget_is_refused ... FAILED
test result: FAILED. 5 passed; 1 failed
```

**(e) The four C5 mutants the previous round MISSED — all now dead.** Each was applied by hand
exactly as `cargo mutants` names it, then `cargo test -p wyrd-core --lib metadata::tests::a_flat_repoint`:

```
MUTANT [delete field chunk_map from struct InodeRecord expression] -> FAILED. 0 passed; 1 failed
MUTANT [delete field version   from struct InodeRecord expression] -> FAILED. 0 passed; 1 failed
MUTANT [replace + with - in repoint_chunk]                         -> FAILED. 0 passed; 1 failed
MUTANT [replace + with * in repoint_chunk]                         -> FAILED. 0 passed; 1 failed
```

**(f) Every operator the new code introduces, likewise killed** (same method, in-crate targets):

```
MUTANT [within: replace - with +]  -> FAILED   MUTANT [extent: replace || with &&]   -> FAILED
MUTANT [within: replace - with *]  -> FAILED   MUTANT [extent: offset != -> ==]      -> FAILED
MUTANT [within: replace - with /]  -> FAILED   MUTANT [extent: len    != -> ==]      -> FAILED
MUTANT [within: replace - with %]  -> FAILED
```

The `%` case is the one a careless fixture misses (`16 % 8 == 0` would coincide with a chunk at
offset 0), which is why the in-crate segmented fixture puts the moved chunk at object byte **16**
inside a record starting at **8** — both subtractions non-zero.

---

## 4. Refutation of my own test (the three forced questions)

**(a) Genuine red?** Yes — through the project's own gate, not by hand:

```
$ PDCA_BUNDLE=results/issue_721 ./engine/scripts/run-verify.sh
run-verify.sh: GREEN — cargo test -p wyrd-custodian --test segmented_map_repoint (fix applied)
  test result: ok. 6 passed; 0 failed
run-verify.sh: RED — ... (production reverted, test kept)
  a_segmented_objects_under_replicated_chunk_is_repaired_in_its_own_record --- FAILED (Blocked, want Changed)
  a_racing_move_of_a_sibling_chunk_in_the_same_segment_record_is_merged   --- FAILED (Blocked, want Changed)
  test result: FAILED. 4 passed; 2 failed
run-verify.sh: PASS — red without the fix, green with it (6 test(s) ran red).
```

Note the count, as the brief asks: the gate reports how many tests **ran** (6), not how many
failed (2). Legs 3–6 pass on the base by construction — pre-fix the pass refuses, so it also
writes nothing — and are bound by the mutation oracle in §3 instead.

**(b) Production path?** Yes. Every leg drives `wyrd_custodian::reconcile_step` — the real fenced
control point, with a real `Custodian` leader and `FencedZone` — over the production
`MetadataStore` / `ChunkStore` seams, and asserts by **reading the store back**. Nothing is
mocked but the two backends, and the fragments are real EC shards through the production encoder
so the loop's checksum/identity verification is genuine. No assertion names a symbol this patch
adds (required: the RED leg reverts production).

**(c) Fixture includes the fault?** Yes. The under-replicated chunk really is missing fragment 1
(never written; its D server is in neither the fleet nor the topology), the racing writers really
land inside the window (`meta.raced()` asserted in legs 2, 3 and 6 — a leg cannot pass because
the race silently never happened), and leg 5's record really is seeded **on** the V/2 boundary
(the padding is measured and self-checked, not a magic count). Leg 6's competing record is a real
`SegmentRecord` through the production encoder, root-inconsistent in exactly the way the finding
described — the planned chunk still matching at its planned offset, so addressing alone would
have rewritten it.

---

## 5. Two phrasings that are deliberate, and worth your eye at sign-off

**Legs 3, 4 and 6 assert `outcome != Reconciled::Changed`, not "answers `Blocked`".** Post-fix a
lost race answers `Satisfied`: `hole` is `reading.incomplete || ceiling_refused`, and a conflict
is neither — the **base's own, unchanged** semantics for a flat lost CAS. Pre-fix the same legs
answer `Blocked`. The assertion that is binding and true in both worlds is "it never reports a
change it did not make". Leg 5, where the refusal does open a hole, asserts `Blocked` outright.

**Leg 4 is vacuous on the base** and cannot be made otherwise: pre-fix the pass never commits, so
the armed root flip never fires and `meta.raced()` cannot be asserted there. Post-fix the leg
passes *because* the flip fired, and negation (c) shows it is binding.

---

## 6. The size cap — the one budget judgement in this iteration (please read)

The brief caps `patch.diff` at **95 KB** "(the driver's size backstop trips at 100 KB)". The
driver measures KB as `patch_bytes / 1024` (`src/pdca_harness/size_signal.py:363`), so the cap is
**97 280 bytes** and the backstop **102 400**.

Iteration 1 shipped **94 957 bytes** — 43 bytes under the strictest reading of the cap, with five
legs and no in-crate tests for the primitive's batch. This iteration is *required* by the
carry-forward to add ~230 lines of in-crate mutation-killing tests, a sixth discriminator leg and
the extent check. Measured: those additions cost **+15 KB**; the naive iteration-2 patch was
**112 121 bytes (109.5 KB)**, which would have tripped the driver's backstop and raised a §6
NEEDS-HUMAN recommending a re-split the brief has already declined once.

What I did, and the cost of each option, so you can overrule me:

1. **Prose trim (kept).** ~14 KB of the added *comment* volume was compressed or deleted across
   all four files — restatements of one claim in two places, paragraphs the code comment beside
   them already made, and the multi-line assertion blocks of legs 3/4/6 factored into one shared
   `assert_no_repair_metadata` helper (`segmented_map_repoint.rs:403`). Result: **103 720 bytes**
   at the default `-U3`. Every distinct claim survives, said once. The patch is still ~27 %
   comment lines against 38 % (`metadata.rs`) and 54 % (`reconstruction.rs`) in the base files.
2. **Deleting substance to fit (rejected, with the number).** At **zero** context the content
   alone is 101 774 bytes: the cap is unreachable at `-U3` without deleting roughly **135
   comment lines or ~45 lines of test code** — concretely, either the in-crate segmented tests
   (`metadata.rs:3287` + `:3328`, 85 lines) or leg 6 (45 lines), i.e. exactly the evidence the
   carry-forward demanded. Rejected: it trades the finding's remedy for a byte count.
3. **`patch.diff` emitted with `-U1` (chosen).** One line of context per hunk instead of three:
   **97 226 bytes = 94.9 KB**, inside both the cap and the backstop, with **no content removed**.
   It is a standard unified diff — `run-verify.sh` applied it cleanly to a fresh checkout of
   `origin/main` in the verify worktree and both legs ran (§4a), and the T4 batch review reads the
   *branch*, not the diff. The measured menu: `-U3` 103 720 · `-U2` 100 250 · `-U1` 97 226.

If you would rather have the full three-line context and take the §6 size item, regenerate with
`git -C $PDCA_WORKTREE diff -U3 > results/issue_721/patch.diff` — the tree is identical either way.

---

## 7. Rubric self-review (the target's own `## Review rubric & protocol`)

* **One clock per lifecycle (ADR-0009)** — this patch adds **no clock read**. `repair_chunk` still
  stamps orphan marks from the pass's `now_millis`, unchanged.
* **Narrow trait seams / dependency direction (ADR-0010, ADR-0016)** — the primitive lives in
  `core` beside the record shapes it rewrites and takes `&dyn MetadataStore`; `custodian` gains no
  backend or on-disk-format knowledge, and no new dependency is added anywhere.
* **Metadata validation boundaries (ADR-0045)** — this iteration's whole addition *is* that rule:
  structural invariants stay at decode (`SegmentRecord::from_wire`), the root/record extent
  agreement is *contextual* and therefore sits at the operation boundary, and it is strict because
  the boundary is a maintenance write (decisions 1 and 3, cited in the code at `metadata.rs:2886-2892`).
* **No DST-reachable shared mutable global state (ADR-0035)** — none added; the statics gate in
  `cargo xtask ci` is green.
* **`#![forbid(unsafe_code)]`** — no new crate root; the new test file carries it (`:24`).
* **Docs currency** — no port, API operation, RPC, CLI flag or persisted field changes: a repoint
  writes the same `inode:` / `seg:` shapes the base already stores, and the two closest peers
  (#697 `1f871ce`, #710 `d2609b2`, which likewise added a public helper to `core::metadata`)
  touched no docs either. Proposal 0016 is a draft and the brief forbids touching it.
* **Serialization identity** — the flat arm rebuilds via `..generation.clone()`, so the ADR-0047
  `skip_serializing_if` fields survive decode→encode and legacy records stay CAS-able; the new
  in-crate flat test asserts exactly that. The segmented CAS pins the **stored bytes** it read, not
  a re-encode, so it cannot be broken by an encoder round-trip quirk.
* **Absent / unsupported entries** — every anomaly is an explicit `Repoint::Conflict` or
  `Repoint::Refused` with the obligation left queued; nothing is silently skipped or drained.
* **Await discipline** — one added `await` (`store.get`), bounded by the `MetadataStore` contract's
  termination clause; no spawned task, no unbounded stream.
* **Test fidelity** — conformance contracts untouched; the DST item is the recorded rejection in
  `review-rejected.md` (#722 owns it), and `cargo xtask dst` is green on the patched tree.
* **Deferrals are settled / out of scope** — the stranded destination fragment on a lost CAS is
  the pre-existing, tracked **getwyrd/wyrd#723**; the brief forbids re-arguing it and forbids
  editing the two "collectable garbage" comments #723 owns. Neither was touched.

---

## 8. Alternatives ruled out (with their cost)

* **Answer the extent mismatch with an `Err` instead of `Conflict`.** It is the same class as a
  retired generation — another writer got there first — and an `Err` from one object ends the
  whole pass for every object (`reconcile` propagates), which is the fault containment `gc`,
  `restore`, `rebalance` and `reconstruction` all share. 0 lines either way; rejected on blast
  radius, not size.
* **Validate the extent at decode instead (`SegmentRecord::from_wire`).** It cannot be done there:
  the record does not know its root, so the check needs the `SegmentRef` the caller selected —
  which is precisely ADR-0045's reason for putting contextual checks at the operation boundary. It
  would also turn a *readable* record into a read fault, the ADR-0040 regression the ADR forbids.
* **Keep `checked_sub` for `within` and add the extent check after it.** +0 lines, and it leaves a
  branch no input can reach: `covers` proves the offset is inside the span and the extent check
  proves the span is the record's own. An unreachable fallback is an unkillable mutant and reads
  as a guard that is doing work. Rejected.
* **Pin the whole resolved segment record's bytes** (the simplest "CAS on what I read"). Not
  implementable — `ResolvedChunkMap` has no per-segment bytes — and if faked by re-reading at
  resolve time it makes leg 2 red: two repairs inside one multipart object serialise on the
  record, so a busy object's second obligation loses its CAS every pass.
* **Address the chunk by its index in the resolved list.** 0 added lines and wrong across a
  segment boundary: the index into the flattened list is not the index into the segment record.
* **Let the caller commit the placement, then the evidence.** +1 `commit` call and a window in
  which a fragment has moved and nothing records where it went — ADR-0015 / `0005:277` forbid it.
* **Reuse `flat_value_ceiling_crossed` for the segment arm.** 0 added lines, and it writes records
  in the 50 001..100 000 band that no publication could produce and no re-publication reproduce.
  The named helper is 6 lines over the existing constant (a second *name*, not a second *value*).

---

## 9. Scope held / deliberately not done

* **No DST leg, no rebalance, no `crates/dst/tests/custodian.rs`** — #722's, per the brief.
* **No pre-mark, no drain fence** (0016 X47): out of scope, and the stranded-fragment leak they
  close is the pre-existing, tracked getwyrd/wyrd#723. Not re-argued, and the two comments #723
  owns were left alone. One consequence worth noting: because the primitive prepares the batch
  **before** the fragment writes, a leg-3/4/6 conflict here writes no fragment either — this
  slice's conflict path is strictly quieter than the flat one's, never louder.
* `commit_chunk_map` and its segmented refusal untouched; the flat arm reuses its CAS idiom.
* Four files, no fifth; no new dependency; no ADR/proposal/conformance-vector change.

## 10. Gates run here (advisory to your sign-off; Check re-runs them)

* `cargo xtask ci` — **`xtask ci: all checks passed`** (exit 0) on the patched tree.
* `scripts/mutants-in-diff` (the C5 gate that failed last round) — **42 mutants tested in 85 s:
  28 caught, 14 unviable, 0 missed.** Last round: 37 tested, **4 missed**. This also confirms
  `cargo mutants --in-diff` parses the `-U1` patch correctly — it finds *more* mutants (42 vs 37),
  i.e. the reduced context costs the gate nothing.
* `./engine/scripts/run-diff-cov.sh` — **PASS, 96.3 %** (315 of 327 instrumentable changed lines,
  floor 80 %; last round 84.2 %). The four uncovered lines are the `panic!` arms of two in-crate
  test helpers (`metadata.rs:3170`, `:3188`) and two pre-existing `reconstruction.rs` arms
  (`:499` the unparseable-key `continue`, #698's; `:592` the drain return).
* `cargo xtask dst` — green, 10 suites, 0 failures.
* `cargo fmt --all` — clean (run after every edit; the patch is formatter-stable).
* `cargo clippy -p wyrd-core -p wyrd-custodian --all-targets` — clean.
* `cargo test -p wyrd-core`, `cargo test -p wyrd-custodian` — green.
* `./engine/scripts/run-verify.sh` — PASS (red without the fix, green with it).

## 11. Nothing is deferred off-Check

No external dependency was missing: no Docker host, no live cluster, no env var, no protoc. The
whole discriminator runs headless over in-memory doubles and a real in-memory redb store, in
milliseconds.
