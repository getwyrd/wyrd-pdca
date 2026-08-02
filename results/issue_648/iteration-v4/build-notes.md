# Build notes — issue 648 (`chunkmap-flat-segmented-record-shape`), **iteration 4**

> Withheld from the reviewer by the driver; written for the human at sign-off.

Target branch: `getwyrd/wyrd @ main` (base `9120f7a`). All edits made in
`$PDCA_WORKTREE` = `/home/eddie/development/wyrd/wyrd.pdca-wt`; every `path:line`
below is that tree with the patch applied unless it says "pre-patch".

---

## 1. What this iteration had to fix (carry-forward from round 3)

Round 3's gates: **C4-ci pass**, **C4-verify pass**, **T4-contribution pass**, reviewer
C1/C2/C3/C4/C5/T1/T2/T3/T5 **all PASS**. The gating red was **T4-batch-review: 5
blocking, 0 recorded-rejected** — and the sign-off note says the human could not triage
them because the finding text was not in front of them. It is now: the five findings are
in `review-batch.md`, each is either **fixed here** or **recorded-rejected** in
`review-rejected.md` (the file the triage rule names, which no previous round wrote).

| # | Finding (round 3) | Disposition |
|---|---|---|
| 1 | `metadata.rs:1016` **BUG** — an unvalidated nonce lets `seg_group_prefix("<nonce>:<epoch>")` alias another group's epoch range, so cleanup could delete a live group's segments | **FIXED** — §2 |
| 2 | `tests/segmented_map_record.rs:113` **TEST-GAP** — the segmented shape is only tested for successful decode; inode CASes re-encode the prior, so it needs an exact segmented decode→encode byte-identity assertion | **FIXED** — §3 |
| 3 | `tests/segmented_map_record.rs:282` **TEST-GAP** — the "worst-case root" omits the optional persisted fields, especially caller-controlled `content_type` | **FIXED** — §4 |
| 4 | `metadata.rs:647` **TEST-GAP** — nothing exceeds `u64::MAX` on segment spans, record extents or chunk lengths: three checked-overflow paths unverified | **FIXED** — §5 |
| 5 | `metadata.rs:244` **CONVENTION** — implements 7(a) while proposal 0016 is `status: draft` (ADR-0037 lifecycle) | **RECORDED-REJECTED** — §6 |
| — | **C5-mutants (advisory)** — 3 missed of 129 | unchanged and unkillable — §7 (re-measured: **3 missed of 127**, the same three) |

The record shape, its codec, the decode invariants, the write-side guards and the 43-file
fail-closed migration are otherwise **unchanged from round 3** — every correctness and
conformance cell accepted them. Nothing rejected was re-submitted.

---

## 2. Finding 1 — a key range minted from an unvalidated string (the BUG)

**The gap.** Pre-patch this round, `seg_group_prefix(nonce: &str)` and
`seggrp_key(nonce: &str)` took bare strings. `seg_group_prefix("<valid-nonce>:7")`
renders `seg:<valid-nonce>:7:` — **byte-for-byte** what `seg_range_prefix` mints for that
group's epoch-7 generation. A `seg:` prefix is what a cleanup pass *deletes*, so a caller
holding a nonce it had not validated (one parsed out of a key, read from a marker, or
concatenated by mistake) could aim "retire every epoch of this group" at a live
generation's segment records — and every fragment those records name goes unreferenced.
That is the C-1 failure mode this slice exists to prevent, reached from the key side
rather than the value side.

**What shipped.** A validated newtype, so the bad prefix is *unrepresentable* rather than
merely unlikely (ADR-0045 parse-don't-validate, an AGENTS.md hard convention):

* `crates/core/src/metadata.rs:543-594` — `SegmentNonce`, its validating constructor
  (the single home of the nonce rule) and `Display`. `#[serde(transparent)]` keeps the
  wire form a plain JSON string, so **no stored byte changes** (the byte-identity rule
  at `:277-286` still holds — verified by the round-trip tests, §3).
* `:601`, `:609-614`, `:616-620` — `SegmentGroup` now *holds* a `SegmentNonce`;
  `SegmentGroup::new` delegates the rule instead of restating it, and `nonce()` hands
  back the validated value, so a consumer can pass it straight to a key helper.
* `:1084`, `:1089` — `seg_group_prefix` / `seggrp_key` take `&SegmentNonce`.
* `:1096`, `:1110-1113` — `parse_seg_key` returns a `SegmentNonce`: what a caller derives
  from a *stored* key is validated by construction. This also removed the throwaway
  `SegmentGroup::new(nonce, 0).is_err()` hack the old parser used to borrow the rule.

**Demonstrated red (the fix binds).** With the two helpers reverted to `&str` and the
rule expressed against that pre-fix API:

```
$ cargo test -p wyrd-core --lib metadata::segmented_shape_invariants::a_group_prefix
thread '…::a_group_prefix_can_never_alias_another_generations_epoch_range' panicked at
crates/core/src/metadata.rs:2121:9:
assertion `left != right` failed: a group prefix must never equal an epoch range prefix
  left:  [115, 101, 103, 58, … 58, 55, 58]      # "seg:0123…cdef:7:"
 right:  [115, 101, 103, 58, … 58, 55, 58]      # "seg:0123…cdef:7:"
test result: FAILED. 0 passed; 1 failed
```

Byte-for-byte identical — exactly the aliasing the reviewer described. Restored
immediately.

**The test** (`crates/core/src/metadata.rs:2122-2203`) is co-located because it asserts on
patch-added symbols. It covers both halves of the rule: (a) the aliasing spelling and five
neighbours (short, long, uppercase, non-hex, a separator smuggled mid-nonce) are refused
by the constructor; (b) a prefix built from a *validated* nonce addresses that group and
nothing else — a group prefix carries two colons and an epoch range three, so the two can
never be equal, and no other group's key lies under my group prefix at any epoch
(`0`, `7`, `u64::MAX`).

**Alternatives, with their cost.**

| Alternative | Why rejected | Cost, measured |
|---|---|---|
| Make the two helpers **fallible** (`-> Result<Vec<u8>, ChunkMapError>`) and validate inside | Validates per *call* instead of per *value*: every future caller re-checks or `unwrap`s, and `parse_seg_key` would still hand back a bare `String` that can be fed to the wrong helper. The rule would live in three places. | ~10 added lines vs the newtype's **20 code lines** (`:543-594`, of which 38 are doc) plus 6 mechanical call-site edits — a 10-line saving for a rule that stops being a type. |
| Have `seg_group_prefix` take `&SegmentGroup` and ignore the epoch | A caller that only knows the nonce (a `seggrp:` marker, a parsed key) would have to invent an epoch to name a group. | 0 lines saved; forces a fabricated field at every future call site. |
| Leave it and document the precondition | A comment does not stop a delete. This is the same class as the round-2 lease-ordering finding: the answer must be structural. | n/a — rejected on correctness. |

---

## 3. Finding 2 — the segmented root's byte-identity was never asserted

**The gap.** Criterion 1 pinned decode→encode identity for a *legacy flat* record; the
segmented root was only tested for "decodes". But a segmented root is re-encoded by
exactly the paths that already exist: every placement repair is
`require(inode, encode(prior)) + put(inode, encode(next))`. If the segmented half of the
codec re-ordered a field, dropped `segment_count` or spelled the group differently on the
way out, every commit against an already-published segmented object would return
`Conflict` **forever** — the permanently-unrepairable object C-1 forbids, and the rubric's
*Serialization identity* class ("add the round-trip test").

**What shipped**, both through base-visible API on raw bytes so they stay part of the C4
red leg:

* `crates/core/tests/segmented_map_record.rs:135-144` —
  `encode(decode(SEGMENTED_ROOT_OK)) == SEGMENTED_ROOT_OK`, byte-for-byte. This pins the
  segmented wire form's **field order and spelling**, not just its acceptance.
* `:147-176` — the same rule end-to-end against a **real** metadata backend: seed
  `RedbMetadataStore` with the segmented root's bytes, then `require(key, encode(prior))`
  must **commit**. That is the actual CAS shape from `metadata.rs:277-286`, `:559`.

Both are genuinely red on base (§9): the root does not decode there at all.

---

## 4. Finding 3 — the "worst case" omitted the fields a caller controls

**The gap.** The capacity fixture built a root with `size`, `chunk_map`, `state`,
`version` only. A real `InodeRecord` also persists the ADR-0047 object metadata — `etag`,
`modified`, and `content_type`, which is the client's request header round-tripped
verbatim (`crates/gateway-s3/src/lib.rs:1737-1741`), so its width is the caller's choice
and not the record shape's. Measuring a root without them measures bytes no production
write ever emits.

**What shipped.**

* `crates/core/src/metadata.rs:327-350` — `MAX_ROOT_VALUE_BYTES`'s doc now states the
  **split** rather than implying the table is the whole record: half the ceiling is the
  segment table's budget, and *the other half is a reserve* for the ADR-0047 metadata and
  for a later field addition. `:302-308` (`MAX_ROOT_SEGMENTS`) matches.
* `crates/core/tests/segmented_map_record.rs:365-472` — one test, two legs, both on
  `encode(...).len()` of a record produced by the real decoder:
  * **Leg 1** (`:388-407`) — the worst-case segment table root: **39 691** bytes ≤
    `MAX_ROOT_VALUE_BYTES` (50 000) and ≤ `MAX_VALUE_BYTES`. Unchanged in substance from
    round 3; this is the `0016:1467` budget rule.
  * **Leg 2** (`:409-472`) — the *same* root carrying object metadata that spends the
    **whole** reserve: `etag` at its structural worst case (64-hex SHA-256), `modified` at
    full `u64` width, `content_type` taking every remaining byte (49 876 of them). Total
    **89 691** bytes ≤ `MAX_VALUE_BYTES` (100 000), and the block is required to be
    exactly the reserve so it cannot quietly shrink into a convenient case.
* `docs/design/architecture/08-crosscutting-concepts.md:85` — the 2× headroom clause now
  says *table* half + reserve half, and why.

**Honesty about what leg 2 does and does not prove** (this is the part a reviewer should
weigh, so it is stated rather than dressed up): because the reserve is defined as
`MAX_VALUE_BYTES − MAX_ROOT_VALUE_BYTES`, leg 2's inequality is *implied* by leg 1's. What
it adds is **fixture fidelity and encoding evidence**: the measured object is now the whole
record, decoded and re-encoded byte-identically at ~90 KB, so a codec that escaped or
expanded a long `content_type`, a field order that did not match the declaration order, or
a decoder that balked at a 90 KB value would fail here while leg 1 passed. The residual —
a root whose caller metadata exceeds the reserve — is stated in both the constant's doc
and the test: it is refused by the tightest backend **when it is published** (a clean
create failure, the same one an equally large *flat* record already meets today on
`origin/main`), and it is not a durability hazard, because a root that was published fits
and every repair re-encodes the same fields. Bounding a caller-supplied header is the
protocol gateway's job.

**Alternative considered and rejected: enforce a value-size guard at the durable-write
gate.** `InodeRecord::checked_for_publication` could refuse any record whose
`encode(...)` exceeds `MAX_VALUE_BYTES`. Rejected on scope and blast radius, both
measurable: it is a **behaviour flip for flat records** (the brief's Scope says this slice
lands none — a PUT with a 200 KB `content-type` that today fails at the backend would
start failing with a different, earlier error), it needs a second full `encode` per write
on a path that already encodes (`git grep -o "encode(&" -- crates | wc -l` → **75** encode
sites), and it changes the error contract of the **42** `create`/`create_leased` call
sites. The finding was about a curated fixture; the fix is an honest fixture.

---

## 5. Finding 4 — three checked-overflow paths with no test

**The gap.** `SegmentedMap::new`'s running offset, `SegmentRecord::checked`'s extent and
`checked_chunk_bytes`'s sum are all `checked_add`, and the failure mode they guard is not
a panic: an unchecked sum **wraps** in a release build to a small total that the very next
equality check would then confirm. A record whose chunk lengths wrap to its declared
`byte_len`, or a table whose tiling wraps back to its declared `size`, would decode as a
**value** — a map that under-reports the bytes its object owns. Nothing exercised any of
the three.

**What shipped.**

* `crates/core/src/metadata.rs:2331-2436` — one co-located test over all three boundaries:
  the root tiling (`u64::MAX` + 1 → `SegmentSpanOverflow`), the record extent
  (`byte_offset = u64::MAX - 1`, `byte_len = 2` → `SegmentSpanUnrepresentable`), and the
  chunk-length sum (`u64::MAX` + 1 → `SegmentLengthOverflow`, asserted on `from_wire` too,
  because the sum must be rejected **before** it is compared with the declared
  `byte_len`). Each case is asserted through the typed variant *and* through
  `decode::<…>` on raw stored bytes, and each is paired with the largest input that still
  **succeeds**, so the assertion pins a boundary rather than a blanket refusal.
* `crates/core/tests/segmented_map_record.rs:254-272` — the root case as base-visible raw
  bytes, under the **forged `size`** a wrapping (or saturating) implementation would
  confirm (`u64::MAX`, which is what `span()` reports once the tiling has wrapped). That
  detail is what makes it bind: with any other `size` — the `1` my first draft carried —
  the case is still `Err` on a wrapping build, but for the *wrong* reason (size-vs-span),
  so it would prove nothing about the overflow check. I caught this only by running the
  wrapping mutation, which is why the demonstrated red below covers both files.

**Demonstrated red.** Replacing all three `checked_add`s with wrapping equivalents:

```
$ cargo test -p wyrd-core --lib …::every_span_arithmetic_that_leaves_u64_is_refused_not_wrapped
assertion `left == right` failed
  left:  Ok(SegmentedMap { … byte_offset: 18446744073709551615, byte_len: 1 })
 right:  Err(SegmentSpanOverflow { index: 1 })
test result: FAILED. 0 passed; 1 failed

$ cargo test -p wyrd-core --test segmented_map_record a_segment_table_whose_tiling
panicked at crates/core/tests/segmented_map_record.rs:268:5:
a segment table whose tiling overflows u64 must be Err, never a wrapped span
test result: FAILED. 0 passed; 1 failed
```

The production file was restored from the scratch copy immediately after each run.

---

## 6. Finding 5 — recorded-rejected, not silenced

`metadata.rs:244` **CONVENTION**: implementing decision 7(a) while proposal 0016 is
`status: draft`. Declined, with the reason recorded at both lines the citation lands on,
in `results/issue_648/review-rejected.md` (the file the triage rule names). In short: the
brief's Scope explicitly excludes "any new/edited ADR / spec / proposal (0016 §(a) names
this an ADR-graduation candidate — that is the architecture board's, INTEGRATION §2/§4)",
and the target's own reviewer protocol says an out-of-scope finding gets a
decline-with-issue-reference rather than an in-PR fix. A code slice editing the proposal's
status to license itself would be the very lifecycle inversion ADR-0037 forbids. Nothing
in the patch depends on the status — no producer, no resolver, byte-identical legacy
records, every consumer of the new variant fails closed — so it stays reversible if 0016
changes before it is accepted. **This is a judgment the human may overturn at sign-off**;
if they want the graduation first, the whole six-slice chain waits on the board, which is
why it is recorded rather than quietly fixed.

---

## 7. What I deliberately did **not** change

* **The shape, codec, write-side guards, key grammar and the 43-file ripple** — accepted
  by every correctness/conformance cell in rounds 2 and 3. Re-deriving settled ground
  would put it back in front of a fresh reviewer for nothing.
* **The 3 surviving mutants** (`crates/custodian/src/backfill.rs:133`,
  `rebalance.rs:301`, `reconstruction.rs:589`). Each deletes the explicit `size: X.size`
  field from an `InodeRecord { size: X.size, …, ..X.clone() }` literal where `..X.clone()`
  supplies the identical value — the mutant *is* the same program, so no assertion can
  distinguish it. Those `size:` lines are **pre-existing on `origin/main`**
  (`git show origin/main:crates/custodian/src/backfill.rs` :125-130); they are in-diff only
  because the neighbouring `chunk_map` line changed. Deleting untouched production lines to
  quiet an advisory metric is gaming it, so I did not. C5 is non-gating.

  Re-measured on this round's patch: **127 mutants, 3 missed, 46 caught, 78 unviable** —
  the same three, and **every** mutant generated in `crates/core/src/metadata.rs` was
  caught, including this round's new code (`SegmentNonce::as_str` replaced with `""` /
  `"xyzzy"`, `checked_segment_index`'s `>` flipped to `==`/`<`/`>=`,
  `SegmentRecord::checked`'s `||`→`&&` and `==`→`!=`, `SegmentedMap::from_wire`'s `!=`→`==`,
  `parse_seg_key`'s `||`→`&&`, `ChunkMap::as_flat`→`None`, …). The new tests are killing
  mutants, not just passing.

---

## 8. Refuting my own test (the three questions)

**(a) Genuine red?** Yes — four separate legs, all measured this round:

1. **The acceptance target** (criteria 2–3), through the project's own runner
   `./engine/scripts/run-verify.sh`: production reverted, test kept →
   `test result: FAILED. 10 passed; 4 failed` with **assertion** failures (the file
   compiles on base — it names nothing this patch adds) →
   `run-verify.sh: PASS — red without the fix, green with it.` Full output in §9. Two of
   the four failures are this round's **new** cases (`segmented_root_round_trips_byte_
   identically`, `segmented_root_cas_commits_against_the_stored_bytes`), so the round-3
   fix is itself covered by the red leg, not just the round-2 shape.
2. **The nonce fix**: demonstrated red in §2 — the pre-fix `&str` helper produces a prefix
   byte-identical to a live generation's epoch range.
3. **The overflow tests**: demonstrated red in §5 — wrapping arithmetic admits the
   overflowing table as a value, and the forged-`size` raw-byte case fails too.
4. **Criterion 1** cannot flip on base (byte-identity is trivially true there), so the
   brief requires a *demonstrated* red. The serializer is byte-for-byte the same code as
   round 2's (`impl Serialize for ChunkMap`, `crates/core/src/metadata.rs:863-871`;
   `SegmentNonce` is `#[serde(transparent)]`, so the wire form did not move — the two
   round-trip tests in §3 prove it on real bytes). Round 2's demonstration, unchanged:
   temporarily serialising `ChunkMap` as serde's externally-tagged enum made
   `legacy_flat_record_round_trips_byte_identically` report
   `left: {"chunk_map":{"Flat":[…]}}` vs `right: {"chunk_map":[…]}` and
   `legacy_flat_record_cas_still_commits_against_the_original_bytes` report
   `left: Conflict, right: Committed` — the permanent-`Conflict` C-1 forbids.

**(b) Production path?** Yes. Every case drives shipped functions —
`wyrd_core::metadata::{decode, encode, create, create_leased, commit_chunk_map,
commit_chunk_map_superseding_leased, put_pending, sweep_pending, seg_key, parse_seg_key,
seg_group_prefix, seggrp_key}`, `SegmentNonce::new`, `SegmentedMap::new`,
`SegmentRecord::{new, from_wire}` and `InodeRecord`'s real `Deserialize`. The store-backed
cases commit against a **real** metadata backend, `wyrd_metadata_redb::RedbMetadataStore`
(`crates/core/src/metadata.rs:2033-2035`, and `::open` on a real file in the acceptance
target) — the M4 production adapter, not a fake. The capacity test measures `encode(...)`
of a record produced by the real decoder, not a computed size estimate.

**(c) Fixture includes the fault?** Yes. Every negative case is a hand-authored **raw
stored byte string** carrying the fault itself (duplicate index, gap, overlap, short
nonce, count mismatch, span/size disagreement, unaddressable index, non-summing chunk
lengths, and now all three overflow boundaries) decoded through `decode::<InodeRecord>` /
`decode::<SegmentRecord>`. This round's additions keep the fault *in* the fixture where it
would have been easiest to curate out: the overflow root carries the **forged `size`** a
wrapping build would confirm (not the honest one, which would have been caught by a
different rule); the capacity fixture carries the caller-controlled `content_type` at the
full reserve rather than omitting it; the aliasing test uses the exact
`"<valid-nonce>:<epoch>"` spelling from the finding, not a merely-invalid string.

---

## 9. Gate runs (local, through the project's own runners)

| Gate | Command | Result |
|---|---|---|
| C4-ci | `./engine/xtask.sh ci` (`PDCA_WORKTREE` set) | **`xtask ci: all checks passed`**, exit 0 (fmt `--check`, clippy `-D warnings`, build, whole-workspace tests incl. DST, `cargo deny`, conformance, statics, prose gates) |
| C4-verify | `PDCA_BUNDLE=… ./engine/scripts/run-verify.sh` | **`PASS — red without the fix, green with it.`**, exit 0 |
| C5-mutants (advisory) | `PDCA_BUNDLE=… ./scripts/mutants-in-diff` | see below |
| formatter | `cargo fmt --all` then `cargo xtask ci`'s `fmt --check` | clean — commit-hook ready |
| targeted | `cargo test -p wyrd-core --lib metadata:: --test segmented_map_record` | 18 + 14 tests, 0 failures |

The C4-verify red leg, verbatim:

```
run-verify.sh: RED — cargo test … (production reverted, test kept)
---- segmented_root_round_trips_byte_identically stdout ----
  a well-formed segmented root decodes: Error("invalid type: map, expected a sequence", …)
---- well_formed_segmented_root_decodes stdout ----
  … must decode; got Some(Error("invalid type: map, expected a sequence", …))
---- segmented_root_at_max_root_segments_stays_inside_the_value_ceiling stdout ----
  `pub const MAX_ROOT_SEGMENTS` is not declared in crates/core/src/metadata.rs — the
  segmented chunk-map shape this test binds is not present in the tree
---- segmented_root_cas_commits_against_the_stored_bytes stdout ----
  the stored root decodes: Error("invalid type: map, expected a sequence", …)
test result: FAILED. 10 passed; 4 failed
run-verify.sh: PASS — red without the fix, green with it.
```

All four are **assertions** on a file that compiled against reverted production, and 14
tests ran on both legs — no vacuous `0 tests` branch.

External dependencies the brief declares (`typos`, `docs-renderer`) are installed on this
host, so `cargo xtask ci`'s prose gates ran for real rather than warn-and-skipping. No
NEEDS-HUMAN external dependency: the whole slice builds and is exercised with the base
Rust toolchain — no Docker, no protoc, no live backend, no new dev-dependency.

---

## 10. Budget

42 files, +2 537 / −202 lines; ~1 105 of the added lines in `crates/core/src/metadata.rs`
are non-comment, of which the mechanical migration (construction sites gaining
`.into()` / `ChunkMap::Flat(..)`, read sites gaining `.as_flat()`) is counted separately
per the brief. Well inside the ≤ ~1 500 semantic-line / ≤ 15-file budget once the
declared mechanical pattern is excluded (40 of the 42 files are that pattern, one line
per site).

---

## 11. Change index (this iteration's delta over round 3)

| Where | What |
|---|---|
| `crates/core/src/metadata.rs:543-594` | `SegmentNonce` — validated newtype, single home of the nonce rule, `#[serde(transparent)]` so no stored byte moves |
| `:601`, `:609-614`, `:616-620` | `SegmentGroup` holds and returns the validated nonce |
| `:1079-1089` | `seg_group_prefix` / `seggrp_key` take `&SegmentNonce`, with the aliasing rationale |
| `:1096`, `:1097-1100`, `:1110-1113` | `parse_seg_key` returns a validated `SegmentNonce` |
| `:302-308`, `:327-350` | the budget doc states the table half + reserve half split |
| `:2122-2203` | co-located test: a group prefix can never alias another generation's epoch range |
| `:2321-2436` | co-located test: every span arithmetic that leaves `u64` is refused, not wrapped (3 boundaries, each with its passing neighbour) |
| `:2361-2370` | the raw-byte overflow root now carries the **forged** `size` |
| `crates/core/tests/segmented_map_record.rs:122-176` | segmented decode→encode byte identity + a real-backend CAS over a stored segmented root |
| `:254-272` | base-visible raw-byte case: a tiling that leaves `u64` |
| `:315-362`, `:365-472` | the capacity test refactored into `worst_case_table_root` / `re_encoded_len` plus leg 1 (table budget) and leg 2 (whole record at the full metadata reserve) |
| `docs/design/architecture/08-crosscutting-concepts.md:85` | same paragraph: table-half/reserve-half wording, the `u64` tiling invariant, and the validated-nonce clause |
| `results/issue_648/review-rejected.md` | finding 5 declined with a recorded reason, at both lines it lands on |
