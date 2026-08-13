# Build notes — issue 681 (iteration 5), `passes-read-through-resolver-contained`

Withheld from the reviewer; written for the human at sign-off.

Base: `origin/main` @ `339da46`. Worktree: `$PDCA_WORKTREE` =
`/home/eddie/wyrd/wyrd.pdca-wt-l0`. Four files, exactly as budgeted:
`crates/custodian/src/{reconstruction,backfill,rebalance}.rs` + the NEW
`crates/custodian/tests/segmented_map_passes.rs`. No `Cargo.toml`, no docs, no ADR.

---

## 1. What this round changed, and why — the round-4 carry-forward

The brief names `iteration-v2/patch.diff` as **salvage**. v4 is that salvage plus three
earlier rounds of corrections; it applied cleanly to `339da46`, so this round started from
v4 and fixed the seven blocking findings in `review-batch.md` plus the two truncated
sign-off lines. **The rejected approach is not re-submitted**: the CAS framing that produced
five of the seven findings is gone, replaced by a different rule.

### 1.1 The one substantive design change: *a pass writes only the generation it read*

v4 decided both the refusal and the CAS precondition off **`resolved.record`** — the
generation the *resolver* answered with — and synthesised the precondition by
**re-encoding** it (`iteration-v4/patch.diff`: `reconstruction.rs` `Arc::from(metadata::encode(resolved.record.as_ref()).as_ref())`,
`rebalance.rs` `fn encoded(record) -> Arc<[u8]>`, `backfill.rs` `.require(key.clone(), metadata::encode(&record))`).
That is what five findings hit (`review-batch.md` lines 3, 4, 5, 6, 8): a `decode → encode`
round trip is only byte-identical while every field re-serialises exactly as the *stored*
spelling had it, and the store compares the precondition **byte-for-byte**. A record written
by another build — an optional field, a different key order, whitespace — loses that CAS on
every pass, forever, and the loss is reported as an ordinary lost race. That is the repo
rubric's *Serialization identity* class ("decode→encode must be byte-identical wherever a
compare-and-swap or content hash depends on it"), in the one place it decides a CAS.

The correction is a single rule, applied identically in all three passes:

> **The generation this pass may write is the one its own scan read, in the shape this
> slice writes. The CAS precondition is that row's own stored bytes.**

- the write/refusal branch now reads the **snapshot's** shape, not the resolved record's —
  `reconstruction.rs:834`, `backfill.rs:166`, `rebalance.rs:248`;
- the precondition is the scan's `value` verbatim — `reconstruction.rs:846` +
  `:665`, `backfill.rs:195`, `rebalance.rs:304` + `:418`.

This is strictly *better* than the base, which also re-encoded (`origin/main`
`backfill.rs:144`, `rebalance.rs:312`, `reconstruction.rs:600`), and it makes the brief's
**pinned decision 4** true *by construction* rather than by assumption: a flat snapshot
resolves to `Answer(Cow::Borrowed(chunks))` with no store read and no supersede check
(`crates/core/src/metadata.rs:2584-2586`), so for every generation this slice writes,
`resolved.record` **is** the scan snapshot and `value` **is** its stored bytes. Only a
*segmented* snapshot can restart onto a live root (7(h)) — and a segmented snapshot is
refused before any write.

### 1.2 The brief invited a falsification of decision 4 — here it is

The brief says: *"If Do finds a commit path that CAN be reached through a restarted resolve,
that falsifies this reasoning: say so in `build-notes.md` and leave it for sign-off."*

**It is reachable — in v4's shape, and it was live in the patch the last Check ran.** The
path: a scan snapshot that is **segmented**, whose root is overwritten with a **flat**
generation between the scan and the resolver's settle read (`metadata.rs:2563-2573`).
`resolve_chunk_map` then returns `Superseded → resolve_current_chunk_map`, whose
`ResolvedChunkMap.record` is the *live flat* root. v4 branched on that record, found it not
segmented, and **filled and CAS'd a generation the pass had never read** — framing the
commit by headers it did not read, and (because the precondition was a re-encoding) racing
on bytes it did not hold. This is the deferred finding recorded in `deferred-findings.json`
("The brief's decision-4 'unreachable by construction' claim is false"): it was false *of
v4*. It is true of this patch, and the reason is now written where the branch is taken.

No new leg was spent on it; the property is bound as a **sub-assertion of leg 2**
(`segmented_map_passes.rs:590-613`) using a one-shot root swap on the metadata double
(`:62`, `:71`). Verified adversarially: re-introducing only v4's branch
(`resolved.record.chunk_map.is_segmented()`) in `backfill.rs` makes that sub-assertion fail
(`assertion left == right failed: it read less than the store`) — the leg discriminates the
exact defect the reviewer named, not a proxy for it.

### 1.3 The other two findings

- **`rebalance.rs:294` (`refused_chunks` counts chunks, not fragments).** Fixed: the counter
  is `refused_fragments += evac.len()` (`rebalance.rs:287`) and the audit field is renamed
  `fragments` (`rebalance.rs:508`). The number an operator is waiting on is *fragments still
  on the machine*; one chunk with three fragments there is three of them. To make the two
  numbers **distinguishable**, the fixture's segmented object now carries THREE draining
  fragments across TWO chunks (`draining_segments()`, `segmented_map_passes.rs:277`) and leg
  2 asserts the single refusal line reports `"fragments":3`. Under v4's counting it would
  read 2 — with the old fixture (2 chunks × 1 fragment) both spellings printed the same
  number and no assertion could have caught it.
- **`segmented_map_passes.rs:493` TEST-GAP (no test forces the resolver-restart race).**
  Answered by 1.2 above — and answered in the stronger direction: rather than proving the
  pass "mutates only the resolver-returned generation", the leg proves it mutates **nothing**
  when handed a generation it did not read.

### 1.4 Two smaller corrections that fell out

- **The CAS precondition is now bound by the fixture, everywhere.** Every root the
  discriminator seeds is stored in a *valid but non-canonical* spelling (one space after the
  opening brace — `stored()`, `segmented_map_passes.rs:304`). Any pass whose precondition is
  a re-encoding therefore loses its CAS on **every** object in the fixture and none of its
  positive work lands. Measured, not asserted: re-introducing the re-encoding in all three
  files turns 4 of the 6 legs red. This costs **zero** extra assertions — it makes the legs
  the brief already required into the binding for the finding.
- **`InodeRecord { size, chunk_map, state, version, ..prior.clone() }` → clone-and-mutate.**
  v4's four surviving mutants were all `delete field size|state from struct InodeRecord
  expression` — *equivalent* mutants: `size: prior.size` and `state: InodeState::Committed`
  are exactly what `..prior.clone()` already supplies for a record the scan proved
  `Committed`. Unkillable by any test. Writing `let mut next = prior.clone(); next.chunk_map
  = …; next.version += 1;` removes the redundancy instead of arguing about it
  (`reconstruction.rs:660`, `backfill.rs:185`, `rebalance.rs:408`), is 6 lines shorter, and
  preserves ADR-0047's "a maintenance commit does not move `Last-Modified`" *by
  construction*. **C5 result: 67 mutants, 31 caught, 36 unviable, 0 missed** (was 4 missed).

---

## 2. Forced self-refutation (recorded per the Do protocol)

**(a) Genuine red?** Yes — measured with the final code. Reverting the three production
files to `origin/main` and keeping the new test (`git checkout -- crates/custodian/src/…`)
fails **all six** legs on assertions/behaviour, and the target still compiles (no symbol this
patch introduces is named in the test), so the red is behavioural, not a missing symbol:

```
test a_duplicate_committed_chunk_id_is_repaired_by_neither_reference ... FAILED
test a_segmented_object_ends_no_pass_and_the_flat_work_still_happens ... FAILED
test an_unreadable_object_is_named_and_the_walk_continues ... FAILED
test a_fault_that_is_not_one_objects_map_still_ends_the_pass ... FAILED
test work_in_a_segmented_record_is_refused_never_discarded ... FAILED
test reconstruction_reads_the_namespace_once_per_pass ... FAILED
test result: FAILED. 0 passed; 6 failed
```

Restoring the three files: `6 passed; 0 failed`, and the whole `wyrd-custodian` suite is
green with the per-pass suites (`tests/{reconstruction,backfill,rebalance}.rs`) **unmodified**
— `git status` lists exactly 3 modified + 1 added file.

Note: the brief predicted leg 6 would *not* be base-red ("it passes before and after"). It
**is** base-red, for an incidental reason: on the base an undecodable committed record ends
the pass with its own decode error, so the leg's assertion that the returned error carries
the *injected* `STORE_FAULT` text fails there. That is more discrimination than predicted,
not less; the leg's actual purpose (guarding against over-containment) is unchanged and it
passes post-fix.

Beyond the whole-patch red, three **targeted** refutations were run (each re-introduces one
specific defect and confirms a specific leg goes red):

| defect re-introduced | legs that go red |
|---|---|
| CAS precondition re-encoded in all three passes (v4's shape) | 4 of 6 |
| `backfill` branches on `resolved.record` instead of the snapshot (v4's shape) | leg 2 |
| — (whole fix reverted) | 6 of 6 |

**(b) Production path?** Yes. Every leg drives the **real** production entry points and no
copy of them: `wyrd_custodian::reconcile_step` (the fenced control point, elected through
`Custodian::elect` over `wyrd_coordination_mem::MemCoordination` and a real `FencedZone`) for
reconstruction and rebalance, and the public `wyrd_custodian::backfill::reconcile` for
backfill — the same three functions this patch edits. The doubles are only the
`MetadataStore` / `ChunkStore` **seams** (`MemMeta`, `MemDServer`), which is the shape every
sibling suite in this family uses (`segmented_map_consumers.rs:78-133`). The fragment bytes
are real erasure-coded shards from `wyrd_core::erasure::encode` +
`wyrd_core::write::encode_ec_fragment`, so `repair::intact_shard` /
`repair::fragment_intact` actually verify rather than trivially accept. The resolver under
test is the shared `wyrd_core::metadata::resolve_chunk_map`, not a stand-in.

**(c) Fixture includes the fault?** Yes, and the fixture **asserts its own faults are real**
rather than assuming them (`Store::seed`, `segmented_map_passes.rs:375`):

- the segmented object is seeded as raw `seg:` records + a segmented root (never a
  committer), and the fixture asserts `resolve_chunk_map` **succeeds** on it — so leg 2's
  refusal is a refusal of readable work, not a side effect of damage;
- the dangling-`seg:` object is seeded with the root's table naming a segment that was never
  written, and the fixture asserts `resolve_chunk_map` really **errors** on it;
- the undecodable object's bytes are asserted to really fail `metadata::decode`;
- the damaged objects sit **first in key order** over a `BTreeMap`-backed store
  (`inode:1`, `inode:2` before `inode:20`/`inode:3x`), so "the healthy object was still
  handled" cannot pass on a walk that abandoned at the first blocker;
- `stored()` asserts both halves of its own premise (the bytes are *not* canonical, and they
  decode back to the same record) — so the CAS binding cannot silently stop binding;
- nothing is curated out: the healthy flat work sits in the **same store** as the damage in
  legs 1–3, and the assertions are positive (a placement actually moved, a fill actually
  materialised, a fragment actually on server 2), never "no error was raised".

---

## 3. Alternatives considered and rejected — with the cost, measured

**(i) Teach the resolver to return the generation's stored bytes** (so a restarted resolve
could be written safely). Rejected on scope, not taste: it needs a new field on
`ResolvedChunkMap` (`crates/core/src/metadata.rs:2265-2272`) and edits at its three
construction sites (`:2625`, `:2668`, and the `Gone` arm), i.e. a **fifth file** in
`crates/core` — which the brief forbids outright ("A **fifth** file … means the shape is
wrong: STOP"), and which changes a type four other consumers already read (`gc.rs:402`,
`scrub`, `restore.rs:616-688`, the read path). Cost: ~15 added lines in `core` plus a
re-verification of four consumers, against **0 lines** for refusing a generation the pass did
not read. And the refusal is not a workaround: it is the same C-1 rule the brief pins —
"an object whose shape changed under the scan is refused on this pass and re-assessed on the
next, because the obligation stays queued".

**(ii) Keep v4's `resolved.record` branch and merely fix the bytes.** Rejected: it leaves
the pass *writing a generation it never read* (the finding of 1.2). Fixing only the bytes
would have required the resolver change of (i) to be correct at all.

**(iii) Detect the restart by comparing `*resolved.record != record` and refuse.** This is
behaviourally identical to what shipped (a restart is only possible from a segmented
snapshot, which is already refused) but costs an extra `InodeRecord` equality compare per
object per pass — O(chunks) per committed object on the hot walk — to re-derive a fact the
snapshot's shape already gives in O(1). Rejected as strictly more work for the same answer;
the reasoning is written at each branch so a future reader does not have to re-derive it.

**(iv) A seeded Tier-0 DST case.** Pre-declared recorded-rejected at Plan; the reason is
already in `review-rejected.md` and it still holds after this round — every write this slice
performs is on a FLAT object, on the scan snapshot's own bytes, and the segmented side writes
nothing at all. This round makes the claim *stronger*, not weaker: the one concurrent
interleaving that was actually reachable (1.2) is now closed by construction and bound by a
test.

---

## 4. Budget

| file | added raw | added semantic | brief's cap |
|---|---|---|---|
| `src/reconstruction.rs` | 323 | 180 | ≤ 210 semantic |
| `src/backfill.rs` | 171 | 82 | ≤ 100 semantic |
| `src/rebalance.rs` | 183 | 90 | ≤ 100 semantic |
| `tests/segmented_map_passes.rs` (new) | **788** | 560 | ≤ 780 raw |
| **total** | **1,465** | 912 | ≤ 1,520 raw / ≤ 880 semantic |
| `patch.diff` | **99,748 bytes**, 4 files | | < 100 KB backstop, exactly 4 files |

Three of the four caps are met; **the test file is 788 raw lines, 8 over the brief's 780**
(+1.0%), and the semantic total is 912 against 880 (the overage is all in the test file's
560 vs 470). Flagged rather than hidden. The cause is attributable: the round-4 review
required two bindings the brief's cap predates — the non-canonical stored-bytes fixture
(§1.4, ~16 lines) and the superseded-snapshot sub-assertion (§1.2, ~20 lines incl. the
double's one-shot swap) — and the fragment-count fixture change (§1.3). Everything else went
the other way: the file came down from 831 to 788 in four compression passes (leg banners
deleted, every added comment block re-wrapped to 100 columns, doc prose cut), and the
production files from 766 to 677 added lines. Cutting the last 8 lines would have to come
out of an assertion or a fixture self-check, which is the wrong trade against a 1% overshoot;
`patch.diff` is under the 100 KB signal that actually raises a §6 item, with 252 bytes to
spare.

---

## 5. Verification run (the project's own runners, in `$PDCA_WORKTREE`)

- `cargo test -p wyrd-custodian --test segmented_map_passes` — red 0/6 pre-fix, green 6/6
  post-fix (§2a). This is the exact green leg the brief's `--classify` dry-run named.
- `cargo test -p wyrd-custodian` — whole crate green (16 targets, 0 failures), per-pass
  suites unmodified.
- `cargo fmt --all -- --check` — clean (the target's commit hook formatter).
- `cargo clippy -p wyrd-custodian --all-targets -- -D warnings` — clean.
- `typos` over the four touched files — clean.
- `scripts/mutants-in-diff` (C5, advisory) — **67 mutants, 31 caught, 36 unviable, 0 missed**.
- `./engine/xtask.sh ci` (C4, whole-workspace) was **not** run to completion here: it exceeds
  the 600 s tool timeout in this environment. Its components that this patch can affect were
  run directly (fmt, clippy, typos, the crate's tests). The driver runs C4-ci in full at
  Check; nothing in this patch leaves `crates/custodian`, and no public API, port, RPC, CLI
  flag or persisted field changes, so the blast radius is that crate's own gate rows.

No external dependency beyond the base Rust toolchain was needed; `typos` and
`cargo-mutants` (both registered `[[doctor.checks]]`) were present and were used. **No
NEEDS-HUMAN external dependency.**

---

## 6. For the human at sign-off

1. **The one judgement call in this round** is §1.1's rule — "a pass writes only the
   generation it read". It refuses a fill/repair/evacuation on an object whose segmented root
   was superseded by a flat one *during the pass*, where a more elaborate patch could have
   performed it. The work is not lost: the obligation stays queued, the object is named, the
   pass answers `Blocked`, and the next pass reads the flat root directly and does it. That is
   the bounded-and-re-assessed shape the brief pins as decision 4's corollary.
2. **`deferred-findings.json` item 2** ("the brief's decision-4 'unreachable by construction'
   claim is false") is answered in §1.2: it was false of v4's patch, and the patch — not the
   claim — is what changed.
3. **The test file is 8 lines over the brief's cap** (§4). Everything else is inside budget.
4. Scratch: `${PDCA_SCRATCH}/pdca-builder-681-redleg` was used for the revert/restore cycles
   and is removed.
