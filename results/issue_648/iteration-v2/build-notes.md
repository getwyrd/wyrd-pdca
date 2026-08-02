# Build notes — issue 648 (`chunkmap-flat-segmented-record-shape`), **iteration 2**

> Withheld from the reviewer by the driver; written for the human at sign-off.

Target branch: `getwyrd/wyrd @ main` (base `9120f7a`). All edits made in
`$PDCA_WORKTREE` = `/home/eddie/development/wyrd/wyrd.pdca-wt`; every `path:line`
below is that tree with the patch applied unless it says "pre-patch".

---

## 1. What this iteration had to fix (carry-forward)

Iteration 1 was rejected with two red gates. Both were about the *evidence*, not the
shape — the shape/codec itself passed C1/C2/C3/C4/T1/T2/T3. This iteration keeps that
shape and fixes the eight blocking review findings plus the `[impl]` C5 finding, and
closes the mutation gaps.

| # | Finding (from `review-batch.md` / `check-review.md`) | Disposition | Where |
|---|---|---|---|
| 1,2,6 | `metadata.rs:1004`/`:1014` **BUG/CONVENTION** — derived serialization bypasses `checked_shape` while `size`/`chunk_map` are independently public, so `create`/`create_leased` can commit a *mismatched segmented* inode that every later decode rejects | **FIXED** | `InodeRecord::checked_for_publication` `crates/core/src/metadata.rs:1176-1209`, called at `create` `:1284` and `create_leased` `:1319` |
| 3 | `metadata.rs:1353` **BUG** — `commit_chunk_map` accepts a segmented prior and replaces it with a flat root, stranding that generation's `seg:` records | **FIXED** | `crates/core/src/metadata.rs:1474-1479` (typed refusal before any batch is built; rationale at `:1462-1470`) |
| 7,8 | `metadata.rs:624/625` **CONVENTION** — decode enforces no stable format maximum, so a root whose segments exceed the six-digit key space becomes a value nothing can address | **FIXED** | `MAX_SEGMENT_INDEX` `:271-284`, `checked_segment_index` `:972-986`, enforced in `SegmentedMap::new` `:606` and `seg_key` `:966` |
| 4,5 | `segmented_map_record.rs:200/206` **TEST-GAP** + C5 `[impl]` — the ceiling test hard-codes 512 / 100 000 and uses minimum-width spans | **FIXED** | `METADATA_SOURCE` / `production_constant` + the worst-case root, `crates/core/tests/segmented_map_record.rs:202-284` |
| — | C5 mutants: 24 missed of 120 | **FIXED for this patch's own code** (0 missed in `wyrd-core`); 3 remaining are *equivalent* mutants — see §5 | co-located tests `crates/core/src/metadata.rs:1898-2257` |
| T4/T5/Validation NEEDS-HUMAN rows | not implementation work (tool disposition, #647's closure rationale, rollout fitness) | left for sign-off | — |

Nothing from iteration 1 was re-submitted unchanged: the record shape is the same
(it was accepted), the *guards around it* and the *tests that bind it* are new.

---

## 2. Four review lines, one defect (findings 1, 2, 3, 6)

All four review lines describe the same C-1 hole from two sides:

* **write a record you cannot read** — `InodeRecord { size, chunk_map }` are two public
  fields with a cross-field invariant (`segmented map spans exactly size`) that decode
  enforces and `encode` did not. `create(&store, .., &record)` would happily persist
  `size: 99` over a table spanning 12 → those bytes are permanently undecodable. An
  object nobody can read is exactly "a permanent or data-losing failure mode".
* **drop a record you cannot resolve** — `commit_chunk_map` replaced a segmented `prior`
  with a flat root. The prior generation's `seg:` records (and every fragment they name)
  are then referenced by nothing, with no resolver (#649) to enumerate them and no
  retirement path (#653) to reclaim them.

**The fix is one rule stated at the write boundary**: a record this build cannot read
back, or cannot publish completely, never reaches a `WriteBatch`.
`InodeRecord::checked_for_publication` runs `checked_shape()` *then* refuses the
segmented shape as unsupported-by-this-build; `commit_chunk_map` refuses a segmented
`prior` the same way. The two steps raise **distinct** variants
(`SizeSpanMismatch` vs `SegmentedMapUnsupported`) so neither can be deleted without a
test noticing, and so #653 lifts exactly step 2 and leaves step 1 standing.

### Alternatives, with their cost

| Alternative | Why rejected | Measured cost |
|---|---|---|
| Make `size`/`chunk_map` **private** with a checked constructor (the pure parse-don't-validate answer) | It is the *right* long-run shape, but it is not this slice: struct-literal construction `InodeRecord { size, chunk_map, state, version, ..Default::default() }` is the idiom everywhere. | **68 construction sites** across 20 files (`git grep -n "InodeRecord {" -- crates \| wc -l` → 68) would each need a constructor call, on top of the 43-file mechanical ripple this patch already carries. #647 was closed *for reviewability*; doubling the ripple is how that repeats. |
| Hand-written `Serialize for InodeRecord` that errors on a mismatch | `encode` is infallible by contract (`crates/core/src/metadata.rs:1152-1154`, `.expect("metadata record serialization is infallible")`), so a serialize error becomes a **panic** in a storage server; making it fallible instead changes every caller. | `git grep -c "encode(" -- crates/*/src` → **11 non-test call sites** would become `?`-propagating, plus the panic risk. The guard costs **3 call sites** and no signature change. |
| Emit `size` from `map.span()` for segmented records (silently "repair" the mismatch) | Silently accepting corrupt input is the defect class the rubric names ("*Absent or unsupported entries*: produce an explicit error … never silent success"). It also hides a producer bug forever. | n/a — rejected on correctness, not cost. |

This is a case where the brief's **Invariant to restore** decides, not diff size: the
smallest change that restores "a stored record's meaning may not change under it" is a
guard at the durable-write boundary, and that is what shipped (**+40 lines** of
production code, 34 of them doc comment: `crates/core/src/metadata.rs:1176-1209`,
`:1284`, `:1319`, `:1462-1479`).

---

## 3. The format maximum (findings 7,8)

The reviewers were right that *some* maximum has to bind at decode, and iteration 1's
blanket "capacity is never a decode invariant" was too broad. The distinction that
resolves it:

* `MAX_ROOT_SEGMENTS` (512) is a **number this deployment picks**. Enforcing it at decode
  would make a durable object unreadable the day someone lowers it — so it stays out of
  decode (`crates/core/src/metadata.rs:302-312`).
* `MAX_SEGMENT_INDEX` (999 999) is the **stored format's own key space**: with
  `SEG_INDEX_WIDTH = 6` a segment past it has no canonical `seg:` key *at any setting*,
  so no reader, GC pass or repair could ever address its record. Admitting such a root
  would hand a consumer a map it can only half-resolve. It is therefore a decode
  invariant (`crates/core/src/metadata.rs:271-284`, enforced at `:606`).

Because indices are exactly `0..segment_count`, that per-index bound **is** the format's
maximum segment count — so no separate count check is needed (documented at
`SegmentedMap::from_wire`, `:663-667`). The check sits **before** the ordering rule, on
purpose: an unaddressable index is reported as what it is at any position, and the rule
is then reachable from a one-segment fixture instead of a million-entry table (which is
also why a bare `if segments.len() > …` guard was *not* used — it is untestable, and an
untestable branch is a permanently surviving mutant).

`seg_key` became fallible for the same reason: rendering index 1 000 000 yields seven
digits, which `parse_seg_key` then rejects — a key that writes but never reads back.
One private helper (`checked_segment_index`) is the single definition of "addressable",
used by both, so the key writer and the decoder cannot drift apart. `seg_key` has **no
production caller** in the tree yet (#649-#653), so the signature change ripples nowhere.

---

## 4. The capacity test, bound to the production constants (findings 4,5 + C5 `[impl]`)

The old test hard-coded `512` and `100_000` and tiled 512 one-byte segments — it would
have stayed green if `MAX_ROOT_SEGMENTS` moved to 4096, and it never exercised the
decimal widths that actually determine the encoded size.

The new test reads both constants out of the module's **source text**
(`include_str!("../src/metadata.rs")` → `production_constant`,
`crates/core/tests/segmented_map_record.rs:202-236`). This is deliberately not an
`use wyrd_core::metadata::MAX_ROOT_SEGMENTS`: the file must still **compile on
`origin/main`** (the RED leg keeps the test and reverts production), and those symbols do
not exist there. Reading the text keeps the test patch-aware anyway — on base the constant
is absent and the helper panics with "…is not declared in crates/core/src/metadata.rs",
which is the assertion red; on the patched tree it measures whatever the constants
actually say today.

The fixture is now the **worst case**, not a convenient one: `byte_len = u64::MAX / n`
(the widest span an `n`-segment tiling can carry before the total leaves `u64`), which
pushes trailing `byte_offset`s to 20 digits, under a `u64::MAX` epoch.

Measured: **39 691 bytes** for 512 worst-case segments against the 100 000-byte ceiling.
Iteration 1's minimum-width fixture measured **22 974 bytes** — it under-reported the
real worst case by 42%, i.e. it would have declared a `MAX_ROOT_SEGMENTS` of 2 000 "fits"
when the true encoding is ~154 KB, over the ceiling (its own fixture at that size
measures ~92 KB, comfortably "inside"). The current margin means the
constant could rise to ~1 296 before the ceiling binds — and if a later slice raises it
past that, this test fails *here* instead of in production.

---

## 5. Mutation testing

`scripts/mutants-in-diff` scoped to the package this patch's logic lives in:

```
$ cargo mutants --in-diff results/issue_648/patch.diff --no-shuffle -p wyrd-core
104 mutants tested in 2m: 41 caught, 63 unviable      # 0 missed
```

(Run against the production code exactly as shipped; the only edits after it were in the
test file — a corrected comment and one sanity assertion — and mutants mutate production
code only.)

(Iteration 1's whole-diff run: 24 missed of 120, of which **21 were in
`crates/core/src/metadata.rs`** — every one of those is now caught. The co-located
tests at `crates/core/src/metadata.rs:1898-2257` were written against that missed list:
accessors are asserted with real values, `parse_canonical_u64` is driven through
`"007"`/`"+7"`/`""`/`"0"`/multi-digit epochs, and `SegmentRecord::checked`'s `||`/`==`
each have a fixture that flips.)

The whole-workspace run of the same gate was **not** re-run to completion here: its
baseline leg spends >10 min in `crates/custodian`'s `deployed_role_*` GC tests before the
first mutant is built. The Check beat runs it; the delta to expect is the **3 remaining
misses**, which are *equivalent mutants* and cannot be killed by any test:

```
crates/custodian/src/backfill.rs:133       delete field size from struct InodeRecord expression in reconcile
crates/custodian/src/rebalance.rs:301      delete field size from struct InodeRecord expression in evacuate_chunk
crates/custodian/src/reconstruction.rs:589 delete field size from struct InodeRecord expression in repair_chunk
```

Each is `InodeRecord { size: X.size, …, ..X.clone() }` (e.g.
`crates/custodian/src/backfill.rs:132-141`): deleting the explicit `size` field leaves
`..X.clone()` supplying the identical value, so the mutated program is *the same
program*. They are in-diff only because the neighbouring `chunk_map` line changed. I did
not add assertions to chase them — no assertion can distinguish the two programs.

---

## 6. Refuting my own test (the three questions)

**(a) Genuine red?** Yes — measured twice, both legs recorded below.

* **Criteria (2) and (3)** — the project's own runner does this leg:
  `./engine/scripts/run-verify.sh` reverts every modified production file, keeps the
  added test, and re-runs it (see §7 for the run). Pre-fix result:
  `well_formed_segmented_root_decodes` and
  `segmented_root_at_max_root_segments_stays_inside_the_value_ceiling` fail as
  **assertions** (the file compiles — it names no patch-added symbol).
* **Criterion (1)** cannot flip on the base (byte-identity is trivially true there), so
  the brief requires a *demonstrated* red instead. Done, with the patch applied:
  `impl Serialize for ChunkMap` was temporarily changed to serde's externally-tagged
  encoding (`serialize_newtype_variant("ChunkMap", 0, "Flat", chunks)`) and the target
  re-run:

  ```
  ---- legacy_flat_record_round_trips_byte_identically ----
  assertion `left == right` failed: decode -> encode must be the identity …
    left:  {"size":11,"chunk_map":{"Flat":[…]},"state":"Committed","version":3}
    right: {"size":11,"chunk_map":[…],"state":"Committed","version":3}

  ---- legacy_flat_record_cas_still_commits_against_the_original_bytes ----
  assertion `left == right` failed: require(key, encode(prior)) must commit …
    left: Conflict     right: Committed

  test result: FAILED. 9 passed; 2 failed
  ```

  That `Conflict` **is** the C-1 failure mode the criterion exists to forbid: every
  overwrite/backfill/reconstruction of every pre-existing object, permanently. The
  temporary edit was reverted immediately (`git diff --stat` back to 1317 insertions) and
  the target re-run green (11/11).

**(b) Production path?** Yes. Every case goes through the shipped functions:
`wyrd_core::metadata::{decode, encode, create, create_leased, commit_chunk_map}` and
`InodeRecord`'s real `Deserialize`. The write-path tests commit against a **real**
metadata backend — `wyrd_metadata_redb::RedbMetadataStore::in_memory()`
(`crates/core/src/metadata.rs:1940-1945`), the same production adapter the M4 backend
uses, not a hand-rolled fake — which is what makes "the refused record never reached the
store" mean something.

**(c) Fixture includes the fault?** Yes. Every negative is a hand-authored **raw stored
byte string** containing the fault itself (duplicate index, gap, overlap, short nonce,
count mismatch, span/size disagreement, unaddressable index, non-summing chunk lengths),
decoded through `decode::<InodeRecord>` / `decode::<SegmentRecord>` — nothing is curated
out. The write-path tests seed the store with the *original* segmented root bytes and
assert they are still byte-for-byte unchanged after the refusal
(`crates/core/src/metadata.rs:2228-2256`). The capacity fixture holds the real
`MAX_ROOT_SEGMENTS` at worst-case widths rather than a comfortable subset.

---

## 7. Gate runs (local, through the project's own runners)

| Gate | Command | Result |
|---|---|---|
| C4-ci | `./engine/xtask.sh ci` (`PDCA_WORKTREE` set) | **`xtask ci: all checks passed`**, exit 0 |
| C4-verify | `./engine/scripts/run-verify.sh` | **`PASS — red without the fix, green with it.`** |
| C5-mutants (advisory) | `cargo mutants --in-diff … -p wyrd-core` | 41 caught / 63 unviable / **0 missed** |
| whole suite | `cargo test --workspace` | pass (0 failures) |
| formatter | `cargo fmt --all` | applied; tree clean under `--check` (inside `ci`) |
| lints | `cargo clippy --workspace --all-targets` (`-D warnings` via workspace lints) | clean |

Both external dependencies the brief declares **ran for real** (they were installed on
this host, so the prose gates did not warn-and-skip):

```
$ typos                                                   # (silent = clean)
$ python3 docs/publishing/tools/lint_docs.py              → lint_docs: OK
$ python3 docs/publishing/tools/render_site.py --check …  → wrote 98 page(s); link audit OK
$ xtask conformance                                       → 5 valid + 6 invalid vectors pass
$ xtask statics / deploy-guard / unsafe-guard / gitlink-guard → pass
```

No NEEDS-HUMAN external dependency was hit: nothing outside the brief's declared
`typos` / `docs-renderer` and the base Rust toolchain was required.

## 8. C4-verify evidence (the red→green, verbatim)

```
run-verify.sh: GREEN — cargo test -p wyrd-core --test segmented_map_record (fix applied)
test result: ok. 11 passed; 0 failed; …
run-verify.sh: RED — cargo test -p wyrd-core --test segmented_map_record (production reverted, test kept)
  well_formed_segmented_root_decodes --- FAILED
    panicked at crates/core/tests/segmented_map_record.rs:114:
    a well-formed segmented root … must decode; got Some(Error("invalid type: map,
    expected a sequence", line: 1, column: 23))
  segmented_root_at_max_root_segments_stays_inside_the_value_ceiling --- FAILED
    panicked at crates/core/tests/segmented_map_record.rs:222:
    `pub const MAX_ROOT_SEGMENTS` is not declared in crates/core/src/metadata.rs —
    the segmented chunk-map shape this test binds is not present in the tree
test result: FAILED. 9 passed; 2 failed; …
run-verify.sh: PASS — red without the fix, green with it.
```

Note what the red proves and what it does not: the file **compiled** on the reverted tree
(9 of 11 tests still ran and passed), so this is an assertion red, not a compile red — the
falsifiability the brief asks for. Iteration 1's capacity test also went red on this leg,
but for the *wrong reason* — its `decode(...).expect(...)` panicked, and its hard-coded
`512`/`100_000` could not have detected the thing the C5 finding was about (a later slice
moving `MAX_ROOT_SEGMENTS` past what fits). The new failure message names the missing
constant, and on the patched tree the assertion is against the real value.

---

## 9. Scope / budget

* Non-mechanical files: 11 — `crates/core/src/metadata.rs`, `read.rs`, `write.rs`,
  `crates/custodian/src/{gc,backfill,rebalance,reconstruction,restore}.rs`,
  `crates/server/src/lib.rs`, `crates/core/tests/segmented_map_record.rs`,
  `docs/design/architecture/08-crosscutting-concepts.md` (≤ 15 ✓).
* The other 31 files are the declared mechanical pattern — construction sites gaining
  `.into()` / `ChunkMap::Flat(..)`, read sites gaining `.as_flat()`.
* Semantic (non-blank, non-comment) added lines: ≈ 1 346 total, of which ≈ 540 are
  production code in `metadata.rs` and ≈ 450 are tests (co-located + acceptance);
  the rest is the ripple. Under the ≤ ~1 500 budget ✓.
* Out of scope and **not** touched: the resolver and its consumers (#649-#651), the
  chunk-id floor (#652), the staged-publication committer (#653), any ADR/spec/proposal,
  any conformance vector.

## 10. Self-review against the target's standing rubric (`AGENTS.md` §Review rubric)

Checked before shipping, since the reviewers apply the same list:

* *One clock per correctness lifecycle* — no clock read added or moved.
* *Metadata validation boundaries* (ADR-0045) — structural invariants at decode
  (`SegmentedMap::new`, `SegmentRecord::from_wire`, `TryFrom<InodeRecordWire>`), surfaced
  as errors; the *contextual* capacity ceiling stays liberal on read.
* *Serialization identity* — decode→encode byte-identical for the flat shape, with the
  round-trip test the rubric asks for, plus the CAS test that shows what the identity
  buys. `SegmentRecord` re-encodes identically too (`metadata.rs:2074`).
* *Absent or unsupported entries* — every `.chunk_map` site that cannot resolve a
  segmented map raises `SegmentedMapUnsupported`; no silent skip, no empty-list answer,
  and the new write-side guards close the same class on the way in.
* *Grammar strictness* — the `seg:` key parser validates digit width and rejects `+7` /
  `007` rather than trusting `u64::from_str`. (It is a *new* grammar, not an extension of
  `parse_orphan_key`'s: that one parses a different key space and stays as it is —
  changing its liberality is a behaviour change outside this slice.)
* *Transactions* — the guards return before any batch is built, so no live transaction is
  abandoned; a refused call touches the store not at all (asserted).
* *Docs currency* — the persisted-shape paragraph in
  `docs/design/architecture/08-crosscutting-concepts.md:85` is updated in the same patch,
  including the new decode-time key-space rule and the write-side refusals.
* *Test fidelity* — write-path tests run on the real redb backend, not a fake.
* Every crate root already carries `#![forbid(unsafe_code)]`; the new test file does too.
* No new dependency, no ADR/spec/proposal edit, no conformance-vector change.

## 11. Notes for the human

* The one judgement call worth a second opinion: `create`/`create_leased` now **refuse a
  well-formed segmented record outright** (`SegmentedMapUnsupported`), not just a
  malformed one. That is deliberate — this build has no committer that writes the `seg:`
  records, so a root published alone names segments that do not exist — and #653 lifts
  exactly that one branch. If the architecture board would rather have the shape
  publishable from day one, this is the line to change.
* No external dependency beyond the brief's declared `typos` / `docs-renderer` was
  needed: no Docker, no protoc, no live backend, no new dev-dependency (the co-located
  write-path tests use `wyrd-metadata-redb`, `pollster` — both already `crates/core`
  dev-deps, `crates/core/Cargo.toml:31`, `:44`).
* Scratch: everything transient lived under `$PDCA_SCRATCH` (`pdca-builder-648-*`) and
  was removed at the end of the run.
