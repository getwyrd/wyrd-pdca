# Build notes — issue 648 (`chunkmap-flat-segmented-record-shape`), **iteration 5**

Withheld from the reviewer; written for the human at sign-off.

Round 4 ended with **one** blocking review finding and the C4/T4-contribution gates green.
This round fixes that finding and changes **nothing else** in production: the delta over
`iteration-v4/patch.diff` is confined to `crates/core/tests/segmented_map_record.rs`
(criterion 3's capacity leg). Everything else in the patch — the shape, the codec, the
decode invariants, the `seg:`/`seggrp:` helpers, the fail-closed ripple, the docs
paragraph — is byte-identical to round 4, which the reviewer passed on C1/C3/C4/C5/T1/T2/T3
and which `cargo xtask ci` passed literally.

---

## 1. The carry-forward, and what it actually said

Round 4's single blocker (`review-batch.md:3`):

> `crates/core/tests/segmented_map_record.rs:329` **TEST-GAP**: Equal spans are not
> worst-case: concentrating the remaining `u64` range in the first segment keeps later
> offsets 20 digits, so 641 segments encode to 50,062 bytes while this generator measures
> 49,675 and falsely passes the 50,000-byte budget.

It is correct, and I reproduced it before touching anything — first with an independent
arithmetic model of the encoding (which reproduced the reviewer's 49 675 / 50 062 to the
byte), then against the real codec (§5.2). The old `worst_case_table_root` handed every
segment an equal span, which maximises the `byte_len` widths but lets the early
`byte_offset`s be short. A table
that gives segment 0 a `10^19`-byte span pushes **every** later offset to the full 20
digits while the remaining range still keeps the later `byte_len`s at 17 — strictly more
encoded bytes. At `MAX_ROOT_SEGMENTS = 512` both fit, so round 4 shipped a *true* claim;
but the test's guarantee was only ever "the table its author thought of fits", and at
641 segments that gap flips the verdict on the wrong side of the budget.

The deeper problem is that "the worst case" here is a **maximisation over all decodable
tables**, and I have now watched two rounds of a human/model pair try to win it by hand
(round 3 shipped equal spans; round 4's reviewer beat them; my own model beats *both* by
another ~37 digits with a mixed-width table). A test whose soundness depends on nobody
finding a cleverer table is a test that will keep failing review.

## 2. What shipped instead — a bound, not a guess

`crates/core/tests/segmented_map_record.rs:435-456` (`encoded_upper_bound`) computes a
**strict upper bound on `encode(...).len()` over every decodable segmented root at a given
segment count**, from real encoded bytes:

```
bound = encode(decode(concrete_root)).len()   // measured, criterion 3's "encode(...).len()"
      + digit_slack(concrete_root)            // widen every writer-chosen u64 to 20 digits
      + state_slack()                         // widen `state` to its widest spelling
```

The argument (in the doc comment, with citations a reviewer can check):

* For a fixed segment count the encoded skeleton is the **same in every decodable root** —
  each part of it is a decode invariant, not a convention: the nonce is exactly 32 hex
  characters (`crates/core/src/metadata.rs:574`), `segment_count` equals the number of
  segments (`:757`), every `index` is exactly its position (`:691`). The punctuation and
  field names are the codec's.
* What a writer can still vary is the **width** of `size`, `epoch`, `version` and each
  segment's `byte_offset` / `byte_len`. A `u64` renders in at most 20 decimal digits, and
  `serde_json` emits plain unsigned decimal — which the same test re-checks byte-for-byte
  (`re_encoded_len`, `:461-473`).
* Adding each field's shortfall from 20 digits therefore yields a length ≥ that of **any**
  table at that count — including tilings no legal table could realise.

Two consequences worth stating plainly:

* **A cleverer table cannot break it.** The round-4 counterexample is now *in the file* as
  `front_loaded_table` (`:395-418`) and measured, but it is not what the budgets are
  asserted against — the bound is. Both concrete tables are asserted `<= bound` (`:519-522`).
* **Looseness is safe in the only direction that matters.** A bound above the true maximum
  can make this test *fail* a shape that would have fitted; it can never *pass* one that
  would not. And the looseness is small: at 512 segments the bound is 41 571 against
  40 019 for the widest table I could construct, so the bound starts refusing a ceiling at
  617 segments where the best-known concrete table only breaks the budget at 640 — ~4 %
  conservative, in the safe direction.

A self-check pins the argument mechanically rather than in prose (`:511-518`): the bound
computed from the equal-span table must **equal** the bound computed from the front-loaded
one. Two structurally different tables at the same count can only differ in digit widths,
so if the two bounds ever disagree the encoding carries per-table bytes the bound does not
model — and it is not a bound. (They agree at 41 571.)

`state_slack` (`:420-433`) measures the two `InodeState` spellings through the **production
codec** rather than assuming `"Committed"` is the wider — it is, so the term is 0, but it
is measured, not asserted.

### What the numbers are

At `MAX_ROOT_SEGMENTS = 512`, `MAX_VALUE_BYTES = 100_000`, `MAX_ROOT_VALUE_BYTES = 50_000`
(all three read out of the production module's source text, §3):

| quantity | bytes |
|---|---|
| equal-span table, measured `encode(...).len()` | 39 710 |
| front-loaded table (round 4's counterexample), measured | 40 019 |
| **bound over every table at 512 segments** | **41 571** |
| root budget the bound is asserted against (`0016:1467`) | 50 000 |
| bound + the full 50 000-byte ADR-0047 reserve | 91 571 ≤ 100 000 |

## 3. Why the constants are still read out of the module's source text

Unchanged from rounds 2–4 and worth re-stating for sign-off, because it looks odd:
`production_constant` (`:303-323`) parses `pub const <NAME>` out of
`include_str!("../src/metadata.rs")`. The test may not `use` those constants — they are
symbols this patch **adds**, and the file must still compile on `origin/main` or the RED
leg becomes a compile error instead of the assertion red the brief's falsifiability
requires (brief §Falsifiability, §Test file). Reading the source text keeps the test
patch-aware anyway: it judges whatever `MAX_ROOT_SEGMENTS` says today (round 1's
carry-forward finding), and on a tree without the patch it fails as an assertion naming the
missing constant — which is exactly what the RED leg shows (§5).

## 4. What I deliberately did **not** change

* **Production.** Not one line differs from round 4. The blocker was a test-strength
  finding; changing the shape or the constants to answer it would be re-opening a reviewed
  design over a test defect. `MAX_ROOT_SEGMENTS` stays 512: the bound proves 512 fits with
  8 429 bytes of headroom, so there is nothing to lower.
* **The three surviving mutants** (advisory C5). I re-ran `scripts/mutants-in-diff` on this
  round's patch — `127 mutants tested in 3m: 3 missed, 46 caught, 78 unviable`, the same
  three as round 4 and none of them in the shape/codec this slice adds:

  ```
  MISSED crates/custodian/src/backfill.rs:133:13:      delete field size from struct InodeRecord expression in reconcile
  MISSED crates/custodian/src/rebalance.rs:301:9:      delete field size from struct InodeRecord expression in evacuate_chunk
  MISSED crates/custodian/src/reconstruction.rs:589:9: delete field size from struct InodeRecord expression in repair_chunk
  ```

  Each is an **equivalent mutant**: the same struct expression ends `..prior.clone()`, so
  deleting `size: prior.size` produces a byte-identical record and *no* test can
  distinguish it. Round 4's reviewer reached the same conclusion (T5 PASS). Killing them
  means deleting those three `size:` lines — 3 lines in 3 files, all **pre-existing base
  code** (`git show origin/main:crates/custodian/src/backfill.rs`, the `InodeRecord {`
  literal at `:131-142`); this patch only changed the `chunk_map:` line beside them. Two
  reasons not to: the target's reviewer protocol says a real finding outside the PR's
  stated scope gets a decline-with-issue-reference rather than an in-PR fix
  (`AGENTS.md:204-205`); and the redundancy is **defensive** — if the struct base ever
  changed from `..prior.clone()` to `..Default::default()`, the explicit `size:` is what
  keeps the record's size correct. Mutation testing cannot tell defensive redundancy from
  dead code, which is exactly why this row is advisory. Declined, recorded here rather than
  silently done.
* **`review-rejected.md`.** The two recorded rejections (0016's `status: draft`) stand
  unchanged; they were not re-raised in round 4.

## 5. Refuting my own test (the three questions)

**(a) Genuine red? — Yes, measured three ways this round, all through the project's own
runner (`./engine/scripts/run-verify.sh`, the configured C4-verify gate).**

1. **The gate's own RED leg** (production reverted, test kept), the one that matters for
   the brief's falsifiability claim:

   ```
   run-verify.sh: GREEN — cargo test -p wyrd-core --test segmented_map_record (fix applied)
   test result: ok. 14 passed; 0 failed
   run-verify.sh: RED — (production reverted, test kept)
   test result: FAILED. 10 passed; 4 failed
     well_formed_segmented_root_decodes                              — "invalid type: map, expected a sequence"
     segmented_root_round_trips_byte_identically                     — same
     segmented_root_cas_commits_against_the_stored_bytes             — same
     segmented_root_at_max_root_segments_stays_inside_the_value_ceiling
        — "`pub const MAX_ROOT_SEGMENTS` is not declared in crates/core/src/metadata.rs"
   run-verify.sh: PASS — red without the fix, green with it.
   ```

   All four are **assertion/panic** reds on a file that still compiles, as the brief
   requires — never a compile error.

2. **The carry-forward finding itself, as a regression probe.** I set
   `MAX_ROOT_SEGMENTS = 641` — the exact count round 4's reviewer used — regenerated the
   patch into a scratch bundle and ran the same gate. The capacity test **fails**:

   ```
   panicked at crates/core/tests/segmented_map_record.rs:532:5:   # `:534` in the shipped file
   EVERY root holding MAX_ROOT_SEGMENTS (641) segments must stay inside the 50000-byte root
   budget (half the 100000-byte value ceiling, 0016:1467); the worst one is at most 52020 bytes
   ```

   Round 4's test **passed** at that same ceiling — its equal-span table measured 49 675,
   under the 50 000 budget, which is exactly the false pass the review reported. That is
   the finding, closed and *proved* closed, not argued closed.

3. **Criterion 1's demonstrated red** — required by the brief's Verification posture,
   because byte-identity of the flat shape is trivially true on the base and so cannot flip
   on the C4 red leg. With the patch applied I made `ChunkMap::Flat` serialize as a tagged
   variant (one edit at `crates/core/src/metadata.rs:866`:
   `serializer.serialize_newtype_variant("ChunkMap", 0, "Flat", chunks)`) and ran the same
   gate:

   ```
   legacy_flat_record_round_trips_byte_identically           FAILED
     left:  {"size":11,"chunk_map":{"Flat":[…]},…}
     right: {"size":11,"chunk_map":[…],…}
   legacy_flat_record_cas_still_commits_against_the_original_bytes  FAILED
     assertion `left == right` failed: require(key, encode(prior)) must commit …
     left: Conflict     right: Committed
   ```

   The CAS over a pre-existing record returns **`Conflict`** — the permanent, nothing-exits
   state the C-1 invariant in the brief names. Reverted immediately afterwards; the shipped
   `metadata.rs` is byte-identical to round 4's (verified by re-applying the bundle patch
   for that one file).

**(b) Production path? — Yes.** Every assertion drives shipped code:
`wyrd_core::metadata::{encode, decode}` over raw stored bytes (`decode::<InodeRecord>` →
`InodeRecordWire` → `SegmentedMap::from_wire` → `SegmentedMap::new`), and the two CAS legs
commit through a **real `wyrd_metadata_redb::RedbMetadataStore`** on a temp dir, using the
same `require(key, encode(prior))` shape production uses
(`crates/core/src/metadata.rs:559`, `:605`, `:665`). The capacity leg measures
`encode(decode(bytes)).len()` — production output, not a hand-computed size — and asserts
the re-encoded bytes equal the input, so a codec that drifted would fail here rather than
be measured. No stand-in, no mock, no re-implementation of the codec: the *only* thing the
test computes for itself is the digit **slack**, which is arithmetic over the widths of the
numbers it wrote, and even that is cross-checked by the equal-vs-front bound equality.

**(c) Fixture includes the fault? — Yes.** Every negative case is a hand-authored raw JSON
byte string carrying the defect it names (`segment_count` 3 with 2 segments; two `index:0`;
indices 0,2; overlapping and gapped spans; a 31-character nonce; a span disagreeing with
`size`; a tiling that leaves `u64`). Nothing is curated out: the capacity fixture now
contains the very table the round-4 review said the old fixture excluded, and the bound
covers the tables neither of us thought of.

## 6. Gate runs (local, through the project's own runners)

| runner | result |
|---|---|
| `./engine/scripts/run-verify.sh` (C4-verify, bundle) | **PASS** — 14/14 green with the fix, 4 assertion reds without; re-run on the final artifact after the last doc-comment edit |
| `./engine/scripts/run-verify.sh --classify` | `ADDED_TEST crates/core/tests/segmented_map_record.rs` — the full red→green branch, not green-only |
| `./engine/xtask.sh ci` (C4-ci, repo, gating) | **`xtask ci: all checks passed`** — fmt, clippy `-D warnings`, build, the whole test suite incl. DST, cargo-deny, conformance vectors, statics and the prose gates |
| `cargo fmt --all` (the target's formatter) | no changes — the patch is fmt-clean, so the publish commit's own hooks have nothing to rewrite |
| `scripts/mutants-in-diff` (C5, advisory) | `127 tested: 3 missed, 46 caught, 78 unviable` — the same three equivalent survivors as round 4, none in this slice's code (§4) |

Scratch: probes ran in `$PDCA_SCRATCH/pdca-builder-648-probe` (a throwaway bundle holding
only `brief.md` + a probe `patch.diff`) and `$PDCA_SCRATCH/pdca-builder-648-calc`; both
removed at the end of the round. No `/tmp` paths were used.

## 7. Budget

Delta over round 4: `crates/core/tests/segmented_map_record.rs` only — +181 / −42 lines
(472 → 611), all in the capacity section and its doc comments; **no production change**
(verified: the two patches' hunks are identical for all 41 other files), no new file, no
new dependency. The patch as a whole stays inside the brief's ≤ ~1 500 semantic lines / ≤ 15
files for the shape+codec region, with the 43-file `.into()` / `.as_flat()` ripple counted
separately as the brief allows.

## 8. Open for the human at sign-off

* Nothing is unverifiable this round; no external dependency was missing. `typos` and
  `docs-renderer` are the brief's declared externals and `cargo xtask ci` covers them.
* The T4 review gate's own finding file is produced by `scripts/review-branch`, which lives
  in the PDCA project rather than the target — that is why the reviewer's rounds 2–4 tagged
  "cannot independently reproduce the scanner's verdict" as NEEDS-HUMAN. Nothing in this
  patch can change that; the finding it reported *is* reproduced and closed here (§5.2).
