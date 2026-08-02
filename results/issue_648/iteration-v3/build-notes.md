# Build notes — issue 648 (`chunkmap-flat-segmented-record-shape`), **iteration 3**

> Withheld from the reviewer by the driver; written for the human at sign-off.

Target branch: `getwyrd/wyrd @ main` (base `9120f7a`). All edits made in
`$PDCA_WORKTREE` = `/home/eddie/development/wyrd/wyrd.pdca-wt`; every `path:line`
below is that tree with the patch applied unless it says "pre-patch".

---

## 1. What this iteration had to fix (carry-forward from round 2)

Round 2's gates: **C4-ci pass**, **C4-verify pass** (red→green), **T4-contribution pass**,
reviewer C1/C2/C3/C4/C5/T1/T2/T3/T5 **all PASS**. Two gates were red:

| Gate | Finding | Disposition |
|---|---|---|
| **T4-batch-review (gating)** — 2 blocking | `crates/core/tests/segmented_map_record.rs:280` **TEST-GAP**: the assertion permits a 50–100 KB root though 0016 requires `MAX_ROOT_SEGMENTS` to keep the worst-case root within **half** the 100 KB ceiling (2× headroom) | **FIXED** — §2 |
| **T4-batch-review (gating)** | `crates/core/src/metadata.rs:1606` **CONVENTION**: the segmented-prior guard runs *after* `live_lease_guards`, so an absent/expired lease returns `Ok(Conflict)` instead of the explicit `SegmentedMapUnsupported` | **FIXED** — §3 |
| **C5-mutants (advisory)** | 3 missed of 129 | **unchanged and unkillable** — equivalent mutants on *pre-existing base lines*; see §4 |

The record shape, its codec, the decode invariants, the key helpers and the 43-file
fail-closed migration are **unchanged from round 2** — they were accepted by every
correctness and conformance cell. This iteration is exactly the two review fixes plus the
tests that bind them. Nothing rejected was re-submitted.

Two rows stay for the human at sign-off (they are not implementation work):
T5 — confirm PR #647 was closed for reviewability only; Validation — fitness of the
staged six-slice rollout.

---

## 2. Finding 1 — the capacity test measured the wrong ceiling

**The gap.** `0016:1467` states the budget rule for this constant explicitly:

> `MAX_ROOT_SEGMENTS` | `> 0`, and `max_segref_bytes × MAX_ROOT_SEGMENTS ≤ V / 2` |
> a segmented root inode value stays inside the ceiling (decision 7)

and `0016:2433` derives the settled value as `⌊50000 / b_segref⌋` — against **50 000**,
not 100 000. Round 2's test asserted only `encode(...).len() ≤ MAX_VALUE_BYTES`
(100 000), so a `MAX_ROOT_SEGMENTS` raised to ~1 000 would have sailed through at ~62 KB
while violating the rule the constant is derived from.

**Why the headroom is a durability property, not tidiness.** A root that only just fits
the backend's value limit is a root that the *next* field added to `InodeRecord` makes
un-writable. Its object is then permanently unrepairable: every placement repair
(reconstruction, rebalance, backfill) is a `require(inode, encode(prior)) + put(inode,
encode(next))` CAS, and if `encode(next)` exceeds the backend's value limit the put is
rejected — forever, for that object. That is the C-1 failure mode this slice exists to
prevent, reached from the capacity side.

**What shipped.**

* `crates/core/src/metadata.rs:326-338` — a new `MAX_ROOT_VALUE_BYTES = 50_000` budget
  constant carrying the rule and its rationale, plus
  `const _: () = assert!(MAX_ROOT_VALUE_BYTES * 2 <= MAX_VALUE_BYTES);` (`:338`) so the
  two numbers cannot drift apart at compile time. The `const _: () = assert!(…)` idiom is
  the repo's own (`crates/metadata-tikv/src/lib.rs:184`,
  `crates/metadata-fdb/src/lib.rs:394`, `crates/gateway-s3/src/lib.rs:453`).
* `crates/core/src/metadata.rs:300-320` — `MAX_ROOT_SEGMENTS`'s doc comment now states the
  budget rule it is derived from (`0016:1467`) and names the test that measures it, so
  raising the constant sends the reader to the measurement.
* `crates/core/tests/segmented_map_record.rs:250-256` reads `MAX_ROOT_VALUE_BYTES` out of
  the production source alongside the other two and first asserts the budget really is at
  most half the ceiling (so a later "fix" that inflates `MAX_ROOT_VALUE_BYTES` to silence
  the failure is caught here too); `:303-310` is the binding assertion
  (`encode(...).len() ≤ MAX_ROOT_VALUE_BYTES`). The brief's criterion-3 wording
  (`≤ 100 000`, `:292-297`) is kept and still asserted — the new bound is strictly
  stronger, not a replacement.
* `docs/design/architecture/08-crosscutting-concepts.md:85` — one clause added to the
  *same* record-shape paragraph the brief scopes (no new paragraph): the ceiling is sized
  with 2× headroom, and why.

**Demonstrated red (the fix binds).** With `MAX_ROOT_SEGMENTS` temporarily raised to 800
— a value the *old* assertion accepts — the new one fails:

```
$ cargo test -p wyrd-core --test segmented_map_record          # MAX_ROOT_SEGMENTS = 800
thread 'segmented_root_at_max_root_segments_stays_inside_the_value_ceiling' panicked at
crates/core/tests/segmented_map_record.rs:303:5:
a root holding MAX_ROOT_SEGMENTS (800) worst-case segments must stay inside the
50000-byte root budget (half the 100000-byte value ceiling, 0016:1467); got 61981 bytes
— lower MAX_ROOT_SEGMENTS rather than spending the headroom
test result: FAILED. 10 passed; 1 failed
```

61 981 bytes is exactly the "50–100 KB root" the reviewer said the old assertion permits.
The constant was reverted to 512 immediately (`crates/core/src/metadata.rs:320`), where
the worst case measures **39 691** bytes — 79 % of the 50 000 budget, so `MAX_ROOT_SEGMENTS`
has room to ~645 before the *budget* binds (and ~1 296 before the raw ceiling would).

---

## 3. Finding 2 — shape must outrank lease state

**The gap.** `commit_chunk_map_superseding_leased` read the pending leases first
(`live_lease_guards`) and judged the prior's shape second. With a segmented prior *and* an
absent or lapsed lease, the caller got `Ok(CommitOutcome::Conflict)`.

`Conflict` is the **retry** answer in this codebase — "a racing writer won the CAS;
re-read the prior and try again" (`crates/core/src/metadata.rs:1476-1479`). A caller
obeying it would re-read, get the same segmented root, and spin; the shape it must fail
closed on never surfaces. That is the rubric's *Absent or unsupported entries* class
("produce an explicit error … never silent success, silent skip") and the brief's third
C-1 clause ("a consumer that meets a shape it cannot resolve fails closed for that
object").

**What shipped.** `crates/core/src/metadata.rs:1616-1621` hoists the
`as_flat().ok_or(SegmentedMapUnsupported)?` above the lease read and binds it to
`prior_chunks`, reused by the orphan loop at `:1640` (so the shape is still resolved
exactly once — no second traversal, no duplicated rule). The doc comment states the
ordering and why (`:1598-1603`). Every sibling was already ordered this way:
`create`/`create_leased` call `checked_for_publication` before the lease read (`:1305`, `:1340`), `commit_chunk_map` refuses before building any batch (`:1493-1499`).

**Demonstrated red.** With the hoist reverted (guard back below `live_lease_guards`) and
the new test kept:

```
$ cargo test -p wyrd-core --lib metadata::segmented_shape_invariants
thread 'metadata::segmented_shape_invariants::a_segmented_prior_outranks_the_lease_state_it_is_committed_under'
panicked at crates/core/src/metadata.rs:1956:26:
the call must fail closed: Conflict
test result: FAILED. 11 passed; 1 failed
```

**The test** (`crates/core/src/metadata.rs:2286-2349`) drives the production function
against a real `RedbMetadataStore` over the three lease states — **absent** (swept),
**lapsed at exactly the sweep's reap boundary** `expiry == now`, and **live** — and
requires the same typed `SegmentedMapUnsupported { operation:
"commit_chunk_map_superseding_leased" }` from all three, then asserts the stored root is
byte-for-byte unchanged. It is co-located because the assertion is on the *typed variant*
(a `ChunkMapError` this patch adds); the acceptance target keeps its base-visible-only
import rule intact.

---

## 4. What I deliberately did **not** change

* **The shape, codec, invariants, key helpers and the 43-file ripple** — accepted by every
  correctness/conformance cell in round 2. Re-deriving them would be churn and would put
  settled ground back in front of a fresh review.
* **The 3 surviving mutants** (`crates/custodian/src/backfill.rs:132`,
  `rebalance.rs:300`, `reconstruction.rs:588`). Each deletes the explicit `size: X.size`
  field from an `InodeRecord { size: X.size, …, ..X.clone() }` literal, where `..X.clone()`
  supplies the identical value — the mutant *is* the same program, so no assertion can
  distinguish it. Round 2's reviewer reached the same conclusion (T5 PASS). Two things
  worth recording for sign-off: those `size:` lines are **pre-existing on `origin/main`**
  (`git show origin/main:crates/custodian/src/backfill.rs` :125-130), in-diff only because
  the neighbouring `chunk_map` line changed; and deleting them to make the gate quiet
  would be editing untouched production lines to game an advisory metric, which I am not
  doing. C5 is non-gating.

---

## 5. Alternatives considered this round, with their cost

| Alternative | Why rejected | Cost, measured |
|---|---|---|
| Leave the ceiling at `MAX_VALUE_BYTES` and just **lower `MAX_ROOT_SEGMENTS`** so the numbers happen to fit | Fixes today's value, not the rule. The next person raising the constant meets the same permissive assertion — the finding is that the *test* measures the wrong bound. | Would have been a 1-character diff (`512` → e.g. `400`) and left the 50–100 KB window open. The shipped fix is **+13 production lines** (`metadata.rs:326-338`) and **+12 test lines**. |
| Put the `/ 2` factor only in the test (no production constant) | The budget would then live in a test file while the constant it governs lives in production; a reader of `MAX_ROOT_SEGMENTS` sees no budget, and nothing catches a `MAX_VALUE_BYTES` edit. | Saves 13 lines; loses the compile-time `const _: () = assert!` tie-break and the doc link at the constant. |
| Enforce `MAX_ROOT_SEGMENTS`/`MAX_ROOT_VALUE_BYTES` **at decode** so an oversized root is an error | Explicitly out of scope and wrong per ADR-0045's liberal-on-read boundary: a capacity number a deployment picks must not make an already-durable object unreadable when it moves (`0016` X69). Capacity binds where the table becomes work (#649/#653). | n/a — rejected on correctness. |
| Fix the lease ordering by mapping `Conflict` → error at the **caller** | Guards the symptom at each of N call sites and leaves the function returning a wrong answer to every future caller. | The cause fix is **1 statement moved** (`metadata.rs:1616-1621`, +6/-6 lines). The caller-side version would have to classify `Conflict` at `crates/core/src/write.rs:319` (the only production caller today, `git grep -n commit_chunk_map_superseding_leased -- crates` → 1 production call site + 1 test) by re-reading the inode and re-deciding the shape — ~10 lines *and a second store read* per caller, repeated for every future caller, and still wrong for a caller that just retries. |
| Return a *new* `CommitOutcome::Unsupported` variant instead of `Err` | Changes a public enum every backend and test matches on, for a case that already has a typed error; the rubric's rule is an explicit **error**. | `git grep -o "CommitOutcome::" -- crates | wc -l` → **289** match sites to re-check, against 0 for the shipped fix. |

---

## 6. Refuting my own test (the three questions)

**(a) Genuine red?** Yes — three separate legs, all re-measured this round:

1. **Criteria (2)–(3), the acceptance target**: the project's own runner
   `./engine/scripts/run-verify.sh` reverts every modified production file, keeps the
   added `crates/core/tests/segmented_map_record.rs`, and re-runs it — see §7 for the run
   and its verdict. The file compiles on base (it names no patch-added symbol) and fails
   as **assertions**.
2. **The capacity finding's fix**: demonstrated red in §2 — `MAX_ROOT_SEGMENTS = 800`
   gives 61 981 bytes, which the shipped assertion rejects and the round-2 assertion
   accepted.
3. **The lease-ordering fix**: demonstrated red in §3 — with the guard back below
   `live_lease_guards` the new co-located test fails with `the call must fail closed:
   Conflict`.
4. **Criterion (1)** cannot flip on the base (byte-identity is trivially true there), so
   the brief requires a *demonstrated* red. Round 2 produced it and the codec is byte-
   identical to round 2's (`impl Serialize for ChunkMap`,
   `crates/core/src/metadata.rs:800-808`, unchanged in this iteration): temporarily
   serialising `ChunkMap` as serde's externally-tagged enum made
   `legacy_flat_record_round_trips_byte_identically` report
   `left: {"chunk_map":{"Flat":[…]}}` vs `right: {"chunk_map":[…]}` and
   `legacy_flat_record_cas_still_commits_against_the_original_bytes` report
   `left: Conflict, right: Committed` — the permanent-`Conflict` failure C-1 forbids.
   (Recorded in `iteration-v2/build-notes.md` §6 with the full output; not re-run here
   because the serializer is byte-for-byte the same code.)

**(b) Production path?** Yes. Every case drives shipped functions:
`wyrd_core::metadata::{decode, encode, create, create_leased, commit_chunk_map,
commit_chunk_map_superseding_leased, put_pending, sweep_pending}` and `InodeRecord`'s real
`Deserialize`. The write-path tests commit against a **real** metadata backend,
`wyrd_metadata_redb::RedbMetadataStore::in_memory()`
(`crates/core/src/metadata.rs:1964-1966`) — the M4 production adapter, not a fake — which
is what makes "the refused record never reached the store" mean anything. The capacity
test measures `encode(...)` of a record produced by the real decoder, not a hand-computed
size estimate.

**(c) Fixture includes the fault?** Yes. Every negative case is a hand-authored **raw
stored byte string** carrying the fault itself (duplicate index, gap, overlap, short
nonce, count mismatch, span/size disagreement, unaddressable index, non-summing chunk
lengths) decoded through `decode::<InodeRecord>` / `decode::<SegmentRecord>`. The new
lease test does **not** curate the failing state out: it exercises the two lease states
that used to short-circuit (**absent** and **lapsed at the reap boundary**) *and* the live
one, and the segmented prior is the real stored root, re-read from the store afterwards to
prove it is untouched. The capacity fixture holds the real `MAX_ROOT_SEGMENTS` at
worst-case decimal widths (`u64::MAX / n` spans, `u64::MAX` epoch), not a comfortable
subset.

---

## 7. Gate runs (local, through the project's own runners)

| Gate | Command | Result |
|---|---|---|
| C4-ci | `./engine/xtask.sh ci` (`PDCA_WORKTREE` set) | **`xtask ci: all checks passed`**, exit 0 (fmt `--check`, clippy `-D warnings`, build, whole-workspace tests incl. DST, `cargo deny`, conformance, statics, prose gates) |
| C4-verify | `PDCA_BUNDLE=… ./engine/scripts/run-verify.sh` | **`PASS — red without the fix, green with it.`**, exit 0 |
| formatter | `cargo fmt --all -- --check` | clean (exit 0) — commit-hook ready |
| targeted | `cargo test -p wyrd-core --lib --test segmented_map_record` | 38 + 11 tests, 0 failures |

The C4-verify legs, verbatim:

```
run-verify.sh: GREEN — cargo test -p wyrd-core --test segmented_map_record (fix applied)
test result: ok. 11 passed; 0 failed
run-verify.sh: RED — cargo test … (production reverted, test kept)
---- well_formed_segmented_root_decodes stdout ----
  … must decode; got Some(Error("invalid type: map, expected a sequence", line: 1, column: 23))
---- segmented_root_at_max_root_segments_stays_inside_the_value_ceiling stdout ----
  `pub const MAX_ROOT_SEGMENTS` is not declared in crates/core/src/metadata.rs — the
  segmented chunk-map shape this test binds is not present in the tree
test result: FAILED. 9 passed; 2 failed
run-verify.sh: PASS — red without the fix, green with it.
```

Both red failures are **assertions** (the file compiled on the reverted tree, as the
brief's falsifiability requires) and 11 tests ran on both legs — no vacuous `0 tests` leg.

External dependencies the brief declares (`typos`, `docs-renderer`) are installed on this
host, so `cargo xtask ci`'s prose gates ran for real rather than warn-and-skipping.

No NEEDS-HUMAN external dependency: the whole slice builds and is exercised with the base
Rust toolchain — no Docker, no protoc, no live backend, no new dev-dependency.

---

## 8. Change index (this iteration's delta over round 2)

| Where | What |
|---|---|
| `crates/core/src/metadata.rs:300-320` | `MAX_ROOT_SEGMENTS` doc states the `0016:1467` budget rule and names the measuring test |
| `crates/core/src/metadata.rs:326-338` | new `MAX_ROOT_VALUE_BYTES` budget + `const _: () = assert!` tying it to `MAX_VALUE_BYTES` |
| `crates/core/src/metadata.rs:1598-1603` | doc: "shape first, lease second", and why `Conflict` is the wrong answer |
| `crates/core/src/metadata.rs:1616-1621`, `:1634` | the hoisted segmented-prior guard, its result reused by the orphan loop |
| `crates/core/src/metadata.rs:2286-2349` | co-located test: a segmented prior outranks every lease state |
| `crates/core/tests/segmented_map_record.rs:19-24` | file doc: the third production constant it reads |
| `crates/core/tests/segmented_map_record.rs:245-256`, `:298-310` | the budget read + the binding `≤ MAX_ROOT_VALUE_BYTES` assertion |
| `docs/design/architecture/08-crosscutting-concepts.md:85` | one clause on the 2× headroom, inside the existing record-shape paragraph |
