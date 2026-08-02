# Build notes — issue 649 (slice 2/6 of #635), iteration 9

Target branch: `getwyrd/wyrd @ main`; build/verify base = the wave fold
`origin/pdca-integration/main` = **6e7c255** (carries #648). Every `path:line` below is
against that base unless it names a file this patch adds (for those, `path:line` is the
line in the patched file).

---

## 0. What changed in iteration 9, and why only that

The iteration-8 sign-off cleared **every** §6 item except one, and rejected on it:

> C5 Causal adequacy is unresolved: mutation testing left two mutants alive at
> `crates/core/src/metadata.rs:2271` (`>`→`>=`) and `crates/core/src/metadata.rs:2290`
> (`||`→`&&`) […] Add targeted test coverage that kills both mutants (an exact-ceiling case
> for `MAX_ROOT_SEGMENTS`, and a one-component group-mismatch case), or — if on inspection
> either check turns out not to be load-bearing — remove the unsupported defense instead.

So this iteration is **the iteration-8 patch plus two test cases**. The production side is
byte-identical to iteration 8 — verified: `diff iteration-v8/patch.diff patch.diff` reports
changes in **`crates/core/tests/segmented_map_resolution.rs` only**. That is deliberate and
load-bearing beyond tidiness: `review-rejected.md` records 324 rows keyed to
`crates/core/src/metadata.rs:<line>`, and the `T4-batch-review` gate matches them by line.
Moving a single production line would silently invalidate rows and re-open findings the
human already settled, so the fix had to land where the gap actually was — in the tests.

### The two mutants, read off the machine rather than the paraphrase

`mutants.out/missed.txt` from the iteration-8 Check named them precisely:

```
crates/core/src/metadata.rs:2271:40: replace || with && in read_group_range
crates/core/src/metadata.rs:2290:28: replace >  with >= in read_group_range
```

Note the sign-off's paraphrase transposed the two and attributed the `>` mutant to
`MAX_ROOT_SEGMENTS`. It is not that ceiling: **every** mutant on
`if accounted > MAX_ROOT_SEGMENTS` (`crates/core/src/metadata.rs:2247`, `>`→`==`, `>`→`<`,
`>`→`>=`) was already **caught** by
`the_widest_admissible_root_still_sizes_no_page_and_reads_back_whole`, which reads a table
of exactly `MAX_ROOT_SEGMENTS` back whole. The surviving `>` is the **value** ceiling,
`if value.len() > MAX_VALUE_BYTES` (`:2290`). I killed the mutants the tool reported, and
flag the discrepancy here so the human is not looking for a `MAX_ROOT_SEGMENTS` case that
was already there.

### Both checks are load-bearing, so coverage — not removal — was the answer

* `:2290` (value ceiling). The brief's Success criterion 3 names it explicitly: a segment
  "over `MAX_VALUE_BYTES` […] ⇒ **fail closed** with a typed error". Removing it would drop
  a Check criterion.
* `:2271` (group pin). This is the check that stops a **retired generation's** segment from
  being spliced into a live map. `seg:<nonce>:<epoch>:` ranges of one group's epochs are
  byte-adjacent, and a retired generation's records outlive its root until the drain reaches
  them (`0016:2452-2462`) — so a leaky prefix answer hands the reader the *same object's*
  wrong bytes, at full length, with no error. That is the C-1 failure mode this slice exists
  to prevent; the defense stays.

### The two cases added (`crates/core/tests/segmented_map_resolution.rs`)

1. **`a_segment_value_at_exactly_the_ceiling_still_resolves`** (`:753-793`). The existing
   anomaly table already seeds a value at `MAX_VALUE_BYTES + 1` and requires a fail-closed
   (`:730`, "a segment's value is past the value ceiling"). This is the **same fixture one
   byte smaller** — the correct record padded with the JSON whitespace `decode` ignores
   (`crates/core/src/metadata.rs:1501-1503`) to exactly `MAX_VALUE_BYTES` — and it must read
   back **whole**. `>=` then refuses a legal record and the case goes red. Beyond the mutant:
   `MAX_VALUE_BYTES` is the largest value the tightest backend in play accepts, not the first
   it refuses (`crates/traits/src/lib.rs:995-999`), so refusing it would invent a permanent
   read failure out of a conforming record — the C-1 shape.
2. **`a_row_from_another_group_answered_inside_this_range_is_refused`** (`:795-841`). Two
   arms, each bending **exactly one** component of the group: same nonce at
   `EPOCH - 1`, and `OTHER_NONCE` at `EPOCH`. `||`→`&&` only refuses a row that differs in
   *both*, so it admits both arms and the read succeeds — red.

   The mechanism is one new field on the self-contained fake, `FakeStore::bleed`
   (`:239-246`, applied at `:397-399`): rows handed back on a `seg:` page **on top of**
   what the asked-for prefix matched — a backend whose prefix handling leaks a neighbouring
   range. Deliberate fault injection into a fake that already carries one
   (`reverse_pages`), and the only way to observe this check at all: the resolver pins the
   group *because* it does not trust the seam's filtering, so a fake that filters correctly
   can never exercise the pin.

   The bled row carries **the record the root's table names for that index**, byte-identical
   to the legitimate one. That is what makes the case bind: a resolver that admitted it
   resolves cleanly and answers the object's bytes, so nothing about the row's *contents*
   can produce the refusal — only its key's group can.

Measured effect (`scripts/mutants-in-diff`, the configured C5 gate): iteration 8 was
`57 mutants tested in 2m: 2 missed, 15 caught, 40 unviable`; this build is
**`57 mutants tested in 2m: 17 caught, 40 unviable`** — `mutants.out/missed.txt` is empty
and both former survivors are now in `caught.txt`:

```
crates/core/src/metadata.rs:2271:40: replace || with && in read_group_range
crates/core/src/metadata.rs:2290:28: replace >  with >= in read_group_range
```

And **each new case is the sole killer of its mutant** — not "the suite happens to be red
now". cargo-mutants' own per-mutant logs
(`mutants.out/log/crates__core__src__metadata.rs_line_{2271_col_40,2290_col_28_002}.log`)
name the single failing test under each mutation, with every other test in the run green:

```
line 2271 col 40 (|| -> &&):  FAILED. 11 passed; 1 failed
    a_row_from_another_group_answered_inside_this_range_is_refused
line 2290 col 28 (>  -> >=):  FAILED. 11 passed; 1 failed
    a_segment_value_at_exactly_the_ceiling_still_resolves
```

---

## 1. What the patch does, and why this shape

(Unchanged from iteration 8 — restated because these notes are what the human reads at
sign-off, not a delta.)

The defect: after #648 an `InodeRecord.chunk_map` can be `ChunkMap::Segmented`, and
**nothing can resolve one**. All three readers took the inline list off the record and
failed closed — `crates/core/src/read.rs:96-97`, `crates/server/src/lib.rs:364-365` and
`:459-460`. There was no shared resolution call anywhere in `crates/`.

The patch adds **one** resolver and routes the three in-slice readers through it.

### `crates/core/src/metadata.rs` (+270 semantic)

* **Two public entries, one result type** (the brief caps this at two; iteration 7 shipped
  three entries and three result types, a named cause of the review surface):
  - `resolve_chunk_map(store, root_key, &record)` — resolve a caller's own committed
    snapshot; if that generation was retired under the read, it **restarts** onto the live
    root itself (`crates/core/src/metadata.rs:2422-2434`).
  - `resolve_current_chunk_map(store, root_key)` — read the live root and resolve it,
    bounded by `MAX_RESOLVE_RESTARTS` (`:2446-2469`). This is the restart the first entry
    takes, and the entry #650/#651 will use from a stale scan snapshot.
  - Both answer `ResolvedChunkMap { record: Cow<InodeRecord>, chunks: Cow<[ChunkRef]> }`
    (`:2141-2152`). One type, not two, and both fields are `Cow` so the ordinary case — a
    flat map on a still-live snapshot — copies nothing and costs **zero** extra store
    calls. Carrying the *record* is not decoration: a restart answers chunks the caller's
    snapshot does not describe, so framing (`size`, `etag`, `modified`) must come from the
    generation the bytes came from.
* **Internals** (all private): `root_still_names` (`:2157-2167`), the one arbiter
  `retired_or` (`:2183-2198`), `read_group_range` (`:2242-2308`), `decode_segment_record`
  (`:2314-2325`), `read_segments` (`:2331-2379`), `resolve_snapshot` (`:2384-2404`).
* **Seven new `ChunkMapError` variants** (`:473-570`) — one per anomaly the criteria name:
  `TooManySegments`, `SegmentUnknown`, `SegmentAbsent`, `SegmentBoundsMismatch`,
  `SegmentValueOverCeiling`, `SegmentRecordUndecodable`, `MapResolutionUnstable`.

**Why every anomaly goes through one arbiter.** `read_group_range` *describes* an anomaly
(`GroupRange::Anomaly`) instead of raising it, and `read_segments` hands every shape to
`retired_or`, which re-reads the root. Deciding locally would answer "corrupt" for a
generation the store has already moved off — a hard read failure invented out of an
ordinary overwrite, the exact arm decision 7(h) exists to prevent (`0016:2452-2471`).

**The bounds, stated for what they are.** The reader bounds the **work**: a table past
`MAX_ROOT_SEGMENTS` is refused before its range is read at all (`:2247-2251`); the range is
the group's *and epoch's* own prefix (`:2252`); and each page asks for
`SEGMENT_PAGE_LIMIT = 128` (`:2111-2129`, `:2257`) — **this reader's constant, never the
root's claim**. The **bytes** are the seam's, assigned to getwyrd/wyrd#674 and said so in
the code (`:2076-2109`, `:523-541`).

### `crates/core/src/read.rs` (+28 semantic)

`read_object` resolves through `resolve_chunk_map` before assembling bytes (`:518-534`),
and the size framing comes from the resolved record. `read_path` inherits it.

**`read_object_from` keeps its signature** (`crates/core/src/read.rs:60` on the base) and
keeps failing closed on a segmented map: it takes no `MetadataStore`, so it *cannot*
resolve. `committed_inode` now returns `(InodeId, InodeRecord)` (`:577-591`) — the one
signature change, rippling over exactly **two** files.

### `crates/server/src/lib.rs` (+27 semantic)

`get_object_streaming` (`:344`, resolving at `:354`) and `get_object_range` (`:427`,
resolving at `:442`) resolve through the resolver instead of `as_flat()`, and frame the
response from `resolved.record`.

### `crates/dst/tests/custodian.rs` (+135 semantic)

Property 9, `segmented_resolve_never_tears`: a segmented root is retired in the window
between the reader's root read and its `seg:` page, and on half the seeds the drain has
already reclaimed one of the retired generation's records. Added to the *existing* file so
it cannot force `--cfg madsim` onto the whole `C4-verify` invocation.

### `docs/design/architecture/06-runtime-view.md` §6.2 step 2 (+1)

The resolver paragraph, and only that one; states the bound honestly and does **not** claim
universal consumer routing.

---

## 2. Budget: measured, and over — the same overage the human already weighed, plus 40

Counting added non-blank, non-comment, non-attribute lines out of `patch.diff`:

| file | iteration 8 | **iteration 9** | brief's expected shape |
|---|---|---|---|
| `crates/core/src/metadata.rs` | 270 | **270** | ~350 |
| `crates/core/src/read.rs` | 28 | **28** | ~30 |
| `crates/server/src/lib.rs` | 27 | **27** | ~30 |
| `docs/.../06-runtime-view.md` | 1 | **1** | ~10 |
| `crates/dst/tests/custodian.rs` | 135 | **135** | ~100 |
| `crates/core/tests/segmented_map_resolution.rs` (new) | 573 | **629** | ~350 |
| `crates/server/tests/segmented_object_read.rs` (new) | 194 | **194** | ~130 |
| **total** | 1,228 | **1,284 / 7 files** | ~1,000 / ≤10 files |

(The iteration-8 column is re-measured with this build's script, so the two columns are
comparable; iteration 8's own notes reported 1,249 / 589 with a slightly different
counter. The delta this iteration is **+56**.)

**The +56 is exactly the coverage the sign-off asked for** — two cases and one field on the
fake — and the production side is unchanged and under budget (326 vs ~410). The pre-existing
overage sits entirely in the two discriminator test files and was explicitly weighed and
accepted by the human at the iteration-8 sign-off (the ~1,925-line §6 bullet was cleared as
stale, and no budget item was left open). I am not re-litigating it; I am reporting that it
moved by 56 lines and why. The only way to *reduce* it here would be to drop a case that
kills a mutant, which is the thing this iteration exists to add.

---

## 3. Refuting my own test (the three forced questions)

**(a) Genuine red?** **Yes — measured on the two new cases specifically, not asserted.**
`./engine/scripts/run-verify.sh` (the configured `C4-verify` gate,
`PDCA_VERIFY_BASE=origin/pdca-integration/main`) applies the patch on a clean base worktree,
runs the shipped tests, then reverts the production change, keeps both test files and
re-runs:

```
run-verify.sh: GREEN — cargo test -p wyrd-core --test segmented_map_resolution -p wyrd-server --test segmented_object_read (fix applied)
test result: ok. 12 passed; 0 failed
test result: ok.  2 passed; 0 failed
run-verify.sh: RED — (production reverted, test kept)
test result: FAILED. 0 passed; 12 failed
run-verify.sh: PASS — red without the fix, green with it.
```

Both new cases are in the RED leg's failure list by name:

```
---- a_row_from_another_group_answered_inside_this_range_is_refused stdout ----
a row from another EPOCH of this very group: must be refused by the resolver's own typed
anomaly, not the base's blanket 'this build cannot yet resolve a segmented map'
---- a_segment_value_at_exactly_the_ceiling_still_resolves stdout ----
called `Result::unwrap()` on an `Err` value: SegmentedMapUnsupported { operation: "read_object_collecting" }
```

That is an *assertion* red, not a compile red: both files import only base-visible symbols
and name nothing this patch adds. And a second, sharper red exists for the two new cases —
the **mutation** red, which is the point of this iteration: with the production check
mutated (`||`→`&&`, `>`→`>=`) and *everything else intact*, the run goes
`11 passed; 1 failed` and the one failure is, in each case, the new test written for it
(§0). `mutants.out/missed.txt` is now empty (was two lines). So each new case is red for
two independent reasons — the whole resolver removed, and its one line flipped — and no
other test in the suite covers either flip.

**(b) Production path?** **Yes.** Every assertion is driven through
`wyrd_core::read::{read_object, read_path}` and `wyrd_gateway_core::ObjectGateway` on a real
`wyrd_server::Gateway`. The resolver is never called directly — not once in either file. The
new cases are no exception: both go through `read_object`, and the code they bind is the
shipped `read_group_range` in `crates/core/src/metadata.rs`, which is what `cargo mutants`
mutated to prove it. The only doubles are a `MetadataStore` (a seam production calls
*through*) and the real on-disk `FsChunkStore`; criterion 1 runs against the **real redb**
backend.

**(c) Fixture includes the fault?** **Yes — in both new cases the fault is what the fixture
is made of, and neither can pass vacuously:**
* the at-ceiling case seeds a value of **exactly** `MAX_VALUE_BYTES` and asserts
  `read_object` returns the payload; a guard fixture assertion
  (`encoded.len() < MAX_VALUE_BYTES`) makes it impossible for the padding to silently
  become a *truncation*, which would make the case pass for the wrong reason;
* the group-pin case injects a genuine foreign row **into the page the resolver reads**, and
  gives it the *correct* record bytes for that index — so the fixture deliberately removes
  every route to a refusal except the key's group. Both one-component arms are present; the
  case does not curate the failing element out, it *is* the failing element.

Carried forward from iteration 8 (unchanged fixtures): the over-ceiling root seeds
`MAX_ROOT_SEGMENTS + 1` and the boundary case seeds exactly `MAX_ROOT_SEGMENTS` and requires
a whole read; the retirement cases mutate the store *inside* the resolve window and delete a
retired record so no stale answer can succeed by accident; the ordering case hands every
`seg:` page back reversed; the DST property injects the retirement across 50 seeds.

---

## 4. Evidence run

| gate | result |
|---|---|
| `C5-mutants` (`scripts/mutants-in-diff`) | **PASS — 57 mutants: 17 caught, 40 unviable, 0 missed** (was 2 missed) |
| `C4-verify` (`./engine/scripts/run-verify.sh`) | **PASS** — red without the fix, green with it (12 + 2 cases) |
| `cargo xtask conformance` | **PASS** — 5 valid + 6 invalid vectors |
| `cargo xtask statics` (ADR-0035) | **PASS** — no DST-reachable shared mutable global state |
| `cargo xtask dst` (madsim, `MADSIM_TEST_NUM=50`) | **PASS** — incl. `segmented_resolve_never_tears ... ok` |
| `cargo fmt --all -- --check` | **clean** (the target's configured formatter; run over every touched file) |
| `cargo xtask ci` | **RED at `cargo deny check`** — the base condition, see below |

`cargo xtask ci` reaches typos ✅, docs lint ✅, docs render ✅, gitlink guard ✅,
unsafe-forbid guard ✅, fmt ✅, clippy ✅, build ✅, test ✅, machete ✅ and then fails at
`cargo deny check` on **RUSTSEC-2026-0221** (`event-listener` 5.4.1, unsound). This is a
**base-tree** condition, tracked as **getwyrd/wyrd#673**, explicitly waived at the
iteration-8 sign-off, and unchanged by this patch: **this patch touches no `Cargo.toml` and
no `Cargo.lock`**. `deny.toml` is a declared zero-tolerance wall (`deny.toml:19-24`) and the
brief forbids suppressing it here. Because deny runs *before* conformance/statics/DST
(`xtask/src/main.rs:1563-1567`), those three tiers are run individually above, as the brief
requires.

Both declared external dependencies were present (`typos`, `docs-renderer`), so the prose
gates ran rather than warn-skipping. **No NEEDS-HUMAN external dependency.** No new
dev-dependency: the `bleed` field uses `Vec`/`Bytes`, already in the file's imports.

`review-rejected.md`: every pre-existing row is carried forward **unchanged and still
line-accurate** — its 324 `crates/core/src/metadata.rs:<line>` rows (the #674
byte-materialisation class and the timeout/deadline class) all still point at the lines they
describe, because no production line moved this iteration. **30 rows were added** for the one
new surface that can attract the settled class: the at-ceiling fixture lines (`:774-786`),
which build a 100 000-byte value in the *test* process on purpose. Narrow phrases only
(`materializ`, `materialis`, `heap`, `over-materiali`, `scan_page`) — `value ceiling` is
deliberately **not** recorded there, so a genuine non-class finding about the new test is not
suppressed. Verified against the gate's own loader (`scripts/review-branch:load_rejected`):
356 rows parse; a simulated "this fixture materializes 100 KB on the heap" finding at `:780`
is suppressed, a simulated "the assertion is vacuous" finding at the same line is **not**.

---

## 5. Alternatives considered and rejected (with costs)

* **Delete the two checks instead of covering them** (the sign-off's second option).
  Rejected on invariant grounds, and it is the cheaper diff, so the reasoning is spelled
  out: removing `:2290` drops Success-criterion 3's "over `MAX_VALUE_BYTES` ⇒ fail closed"
  outright (−9 lines of production, −1 error variant, −1 Check criterion). Removing `:2271`
  (−5 lines) leaves the resolver trusting each backend's prefix handling for the one thing
  it cannot afford to be wrong about — a retired generation's segment silently entering a
  live map. `docs/principles.md` §1.2/§2: with an *Invariant to restore* named (C-1), the
  target is the smallest change that **restores the invariant**, not the smallest diff. +56
  test lines is the price of proving both.
* **Assert on the resolver directly** (call `read_group_range`/`resolve_chunk_map` in the
  test). Rejected: it is private, and the brief requires every assertion to be driven
  through base-visible entry points so the file compiles red on the base. A direct call
  would also stop being a *fix-discriminating* red.
* **Make the fake wrap a real backend and rely on its prefix filtering.** Rejected — it is
  the finding iteration 7 earned (`segmented_map_resolution.rs:553`, TEST-GAP): a delegating
  double reports what it forwarded, not what the resolver asked for, and it structurally
  cannot inject a leaked prefix at all.
* **A page-limit-violating bleed** (bleed enough rows to exceed `SEGMENT_PAGE_LIMIT`).
  Rejected as untargeted: it would bend two clauses at once, so a red would not say which
  check caught it. The shipped bleed keeps the page at 3 rows against a 128-row bound.
* Everything rejected at iteration 8 stands unchanged and is recorded in
  `review-rejected.md`: caller-side timeout/deadline over a `MetadataStore` await; a byte
  ceiling inside the resolver (**#674**); `accounted + 1` as the page limit; a
  `MapResolution` enum with the restart left to each caller; keeping `committed_inode`'s
  return type.

---

## 6. Self-review against the target's standing rubric (`AGENTS.md` §"Review rubric & protocol")

Re-run against **this iteration's delta** (the full-patch pass is in
`iteration-v8/build-notes.md` §8 and nothing it covers moved):

* *One clock per correctness lifecycle* — the delta reads no clock.
* *Narrow trait seams* — `FakeStore` implements only `MetadataStore`; the new field adds no
  dependency and no new trait surface.
* *Metadata validation boundaries (ADR-0045)* — unchanged; the delta adds no validation, it
  *proves* two existing contextual checks.
* *`#![forbid(unsafe_code)]`* — the test file carries it (`:24`); unchanged.
* *Docs currency* — no port/API/RPC/flag/persisted field changes in the delta.
* *Absent or unsupported entries* — the new cases assert **bytes** and typed refusals, never
  counts; `assert_fails_closed` still refuses `SegmentedMapUnsupported` as a pass, so
  neither case can be satisfied by the base's blanket refusal.
* *Test fidelity* — the fake's deviations from the seam contract are now two, both named in
  doc comments at the field that causes them (`reverse_pages`, `bleed`), and both are used
  only by the case that needs them; every other case runs the honest implementation.
* *Deferrals are settled* — nothing re-raised; the #674 rows are untouched and still parse.
