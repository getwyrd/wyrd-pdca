# Build notes — issue #693 (multipart-state-machine-digests)

## What this lands

`crates/core/src/multipart.rs` gets:

- 5 new `RecordError` variants (`NoPartsNamed`, `DuplicatePart`, `PartsOutOfOrder`,
  `MultipartEtagMalformed`, `EtagPartCountOutOfRange`) plus their `Display` arms.
- `Digest::of` — the one call site in the crate that names `sha2` (`multipart.rs:1010-1016`
  on the patched tree), so the digest algorithm has exactly one place to look.
- `canonical_named_parts` — **refuses** a non-ascending or duplicate part list; never sorts.
- `MultipartEtag` (validated `<64-hex>-<N>` type), `multipart_etag`, `complete_fingerprint`.
- The typed outcome vocabulary: `InvalidPart`, `Backpressure`, `Refusal`, `Publication`,
  `CreateOutcome`, `ReserveOutcome`, `UploadPartOutcome`, `CompleteOutcome`, `AbortOutcome`.
- Decision 3's verb x state answer table: `Verb`, `UploadPartAnswer`, `CompleteAnswer`,
  `AbortAnswer`, `ListPartsAnswer`, `ListUploadsAnswer`, `Answer`, and the per-verb functions
  plus the total dispatcher `answer`.

`crates/core/Cargo.toml` adds `sha2.workspace = true` with a doc comment recording it is not
a new dependency decision (already a workspace dependency at `Cargo.toml:147`, used by
`gateway-s3`/`server`, on the `deny.toml` allowlist). `Cargo.lock` updates mechanically
(`wyrd-core` gains `sha2` in its dependency list — one line).

New test: `crates/core/tests/multipart_state_machine.rs`, 21 tests covering the five legs.

## Why this shape, what I ruled out

**`answer()`'s signature takes `Option<&SessionState>`, not `Option<&SessionRecord>`.** The
salvage patch (`results/issue_654/iteration-v2/patch.diff:2418-2422`) took the whole
`SessionRecord`. `SessionRecord` has no public constructor (`multipart.rs` doc at its
definition: "No writer-side constructor... the first writer is the store round trip"), so a
pure-function test would have had to go through JSON + `decode_session_record` just to get a
state to answer against. `SessionState` itself (and `Completion`/`PublishTarget`) are plain
public structs/enums with public fields, directly constructible — `multipart_session_records.rs`
already builds `SessionState` literals for assertions, so this isn't a new pattern. Taking the
state directly keeps `answer` a function of exactly what decision 3's table is over, and #508
/ #656-#659 can call it with `session.state()` (the existing accessor) without needing an
extra unwrap layer.

**One `complete_answer`, not two.** My first draft mirrored the salvage's shape: `answer()`
had its own inline copy of the `Open`/`Completing`/`Aborting`/absent table for the
no-fingerprint case, and delegated to `complete_answer` only when a fingerprint was
supplied. I caught this **only because I ran negation (a)** (see below) — negating
`complete_answer`'s `Completing` arm didn't fail `every_decision_3_cell_is_answered` at all,
because that test's non-`Completed` cells all pass `request_fingerprint = None` and so never
call `complete_answer` in the first place; they hit the inline duplicate instead. Two
independently-maintained copies of one table cell is exactly the defect class this child
exists to close (C-1: a cell answered wrong is worse than an error), so I widened
`complete_answer`'s signature to `Option<&Digest>` and made `answer()` delegate
unconditionally — one function, one place a fix (or a regression) can land. Re-ran the
negation after the fix; see below, it now fails correctly.

**`Publication::etag` is `Digest`, not `MultipartEtag`.** The salvage's `Publication.etag`
was `MultipartEtag`. But `Completion.etag` (the record `Publication::of` reads,
`multipart.rs:1863` pre-patch) is already typed `Digest` — a decision the prior child (#716)
made before this one existed. Publication mirrors what it's built from; changing
`Completion`'s field type is out of this child's scope (not in the bullet list of additions,
and the record's shape was already merged). I flagged this in-line
(`Publication.etag`'s doc comment) rather than silently reshaping it.

**`canonical_named_parts` refuses instead of sorting** — this is the pinned fix (leg 2/3, the
carried-forward v2 finding at the old `multipart.rs:1903`). No `BTreeMap`; a single linear
scan tracking `previous: Option<u32>`, erroring on `number == previous` (duplicate) or
`number < previous` (out of order). O(n), one pass, no auxiliary sort — cheaper than the
salvage's `BTreeMap` coalesce as well as correct.

**`MultipartEtag::parse` rejects a 0 count and a count past `MAX_PART_NUMBER`** (leg 4, the
other carried-forward finding, old `multipart.rs:1844`) — reusing `canonical_decimal` (already
in the file, used by every other variable-width decimal this module parses) rather than
writing a second ascii-digit/leading-zero check.

**Digest hashing goes through one call site, `Digest::of`.** The module's own pre-existing
doc comments (on `Digest` and on `hex_lower`) explicitly forward-referenced "the next child's
`Digest::of`" — I kept that promise literally rather than hand-rolling `Sha256::new()` /
`.update()` / `.finalize()` separately inside `multipart_etag` and `complete_fingerprint`.
`sha2::Digest` (the trait) is imported as `Digest as _` to never shadow this module's own
`Digest` value type — mirrors `crates/gateway-s3/src/crypto.rs:21`'s own alias, the cited peer
callsite, adjusted for the name collision that file doesn't have.

## Falsifiability — the four demonstrated negations (binding)

Each was applied to the patched tree, run through `cargo test -p wyrd-core --test
multipart_state_machine`, confirmed red, then reverted and confirmed green again.

### (a) Answer a `Completing` cell as if `Open`

Changed `complete_answer`'s `Completing` arm from
`CompleteAnswer::Refused(Refusal::OperationAborted)` to `CompleteAnswer::Fences`.

```
test every_decision_3_cell_is_answered ... FAILED
thread 'every_decision_3_cell_is_answered' panicked at crates/core/tests/multipart_state_machine.rs:185:13:
assertion `left == right` failed: CompleteMultipartUpload x Completing { fenced_at_millis: 1, segments_written: 0, publish_target: PublishTarget { parent: 7, name: "object", epoch: 1 } }
  left: Complete(Fences)
 right: Complete(Refused(OperationAborted))
test result: FAILED. 20 passed; 1 failed
```

(This negation is also what surfaced the duplicate-table defect above: my first cut of
`answer()` didn't call `complete_answer` for this cell at all, so the negation didn't fail
anything until I removed the duplicate. The transcript above is from the **fixed** shape.)

### (b) Concatenate hex text instead of raw digest bytes (leg 2)

In `multipart_etag`, changed `concatenated.extend_from_slice(digest.as_bytes())` to
`concatenated.extend_from_slice(digest.to_hex().as_bytes())`.

```
test multipart_etag_matches_the_oracle_for_one_part ... FAILED
  left: "129ce68373c9c9982f44e29880405332443aedb2676eb987f92b18fc53097679-1"
 right: "0c756341cd24e0d61603d4df8d4468e6282a7cd66c4ca76fd2cd3da384b38b32-1"

test multipart_etag_is_over_raw_bytes_not_hex_text ... FAILED
assertion `left != right` failed
  left: "fdbdbe6f19bb6f8a3dd5a08c3db8f6832773a2cf738e5a53e18bc670c40c8e40-2"
 right: "fdbdbe6f19bb6f8a3dd5a08c3db8f6832773a2cf738e5a53e18bc670c40c8e40-2"

test multipart_etag_refuses_a_non_ascending_list_rather_than_sorting_it ... FAILED
test result: FAILED. 18 passed; 3 failed
```

### (c) Ignore part numbers in the fingerprint (leg 3)

In `complete_fingerprint`, stopped writing `part_number.to_be_bytes()` into the hash (kept
only the digest bytes).

First attempt at this negation **did not fail any test** — my
`complete_fingerprint_disagrees_on_the_same_digests_under_different_numbering` test swapped
`d1`/`d2` between parts 1 and 2, which also reverses the *sorted digest order*, so the two
fingerprints still disagreed for a reason that had nothing to do with whether part numbers
are hashed. I rewrote the test to keep the digest order identical and change only the
literal numbering (`(1,d1),(2,d2)` vs `(1,d1),(5,d2)` — same relative order, different
absolute numbers), which is the case that can *only* disagree if numbers are actually hashed:

```
test complete_fingerprint_disagrees_on_the_same_digests_under_different_numbering ... FAILED
assertion `left != right` failed
  left: Digest([33, 126, 255, ...])
 right: Digest([33, 126, 255, ...])   (identical bytes)
test result: FAILED. 20 passed; 1 failed
```

### (d) Sort a non-ascending list instead of refusing it (legs 2/3)

Replaced `canonical_named_parts`'s refuse-on-out-of-order/duplicate body with the salvage's
original `BTreeMap`-based coalesce-and-sort.

```
test complete_fingerprint_refuses_the_same_malformed_lists_as_multipart_etag ... FAILED
called `Result::unwrap_err()` on an `Ok` value: MultipartEtag { ..., parts: 1 }
test multipart_etag_refuses_a_duplicate_part_number ... FAILED
called `Result::unwrap_err()` on an `Ok` value: MultipartEtag { ..., parts: 1 }
test multipart_etag_refuses_a_non_ascending_list_rather_than_sorting_it ... FAILED
called `Result::unwrap_err()` on an `Ok` value: MultipartEtag { ..., parts: 3 }
test result: FAILED. 18 passed; 3 failed
```

All four reverted; `cargo test -p wyrd-core --test multipart_state_machine` is green (21/21)
on the final tree, `cargo fmt -p wyrd-core -- --check` and `cargo clippy -p wyrd-core
--all-targets` are clean, `cargo deny check`/`cargo machete`/`typos` are clean.

## Refuting the test (the three required questions)

**(a) Genuine red?** Yes — verified directly, not inferred. With the module additions
reverted to base (`git checkout -- crates/core/src/multipart.rs crates/core/Cargo.toml`),
`cargo test -p wyrd-core --test multipart_state_machine` fails to **compile**: 7 errors,
`unresolved import sha2` plus 6 "not found in `multipart`" errors for the new symbols
(`answer`, `abort_answer`, `complete_answer`, `complete_fingerprint`, `multipart_etag`,
`upload_part_answer`, and the five `RecordError` variants). This matches the brief's
pre-declared posture exactly: "RED leg fails to compile → UNVERIFIABLE (exit 77), EXPECTED
and PRE-DECLARED." Restored the two files from a scratch-dir backup afterward and confirmed
21/21 green again.

**(b) Production path?** Yes. Every assertion calls the production functions directly
(`answer`, `complete_answer`, `abort_answer`, `list_parts_answer`, `list_uploads_answer`,
`multipart_etag`, `complete_fingerprint`, `MultipartEtag::parse`) — there is no test-side
reimplementation of the table or the digest composition. The one place the test *does* roll
its own logic is leg 2's oracle (`oracle_etag`, a direct `sha2::Sha256` call over a
hand-built concatenation) and leg 4's hex encoder — both deliberately independent of
production's `canonical_named_parts`/`Digest::of`/`hex_lower`, which is the point: they are
the *check*, not the thing under test.

**(c) Fixture includes the fault?** Yes, in the sense that matters here: this is a pure-value
test with no store, so there is no fixture to exclude a faulty node from. What corresponds to
"the failing element" for a table test is exhaustiveness — a fixture that only tried, say,
`Open` and skipped `Completing` would prove nothing about the `Completing` cells. Leg 1
enumerates the full `Verb::ALL x {Open, Completing, Aborting, Completed, absent}` product (25
cells, asserted via `assert_eq!(cells, 25)` so a shrunk state list would fail loudly) and the
four negations above each landed on a genuinely different code path, confirming the fixture
does reach the code the fix changes rather than curating around it.

## What I ruled out and why (cost, where relevant)

- **A `Digest::of` that streams instead of buffering**: `multipart_etag`/`complete_fingerprint`
  build a `Vec<u8>` and hash it in one shot rather than feeding a live `Sha256` hasher
  incrementally. Ruled the buffering fine: the input is a Complete's named-part list, bounded
  by `MAX_PART_NUMBER` (999,999) at (4 + 32) bytes/part ≈ 32 MB worst case, in memory for the
  duration of one request — not the "stream-don't-buffer" object-body concern ADR-0047's
  `HashingSource` exists for. Streaming would have meant either a second hasher type in this
  module or exposing `Sha256` itself as part of the public API; one `Digest::of(&[u8])` call
  site is smaller and keeps `sha2` naming confined to one function.
- **Reusing `Refusal::InvalidPart`'s `OutOfOrder` reason instead of a new `RecordError`
  variant for `canonical_named_parts`**: rejected — `InvalidPart` is the *protocol* answer
  vocabulary (`Refusal`, surfaced by #508's later validation against stored part records);
  `canonical_named_parts` is lower-level, called by both digests before any session/store
  context exists, and reuses the same `RecordError` family every other structural rule in this
  module already reports through.
- **Boxing `Publication` inside `CompleteOutcome`/`CompleteAnswer`** was kept (matching the
  salvage) rather than left unboxed: `Publication` is ~56 bytes; boxing keeps the enclosing
  enums from growing every match/return site by that much for a value that's usually a small
  `Refused(Refusal)`. Not load-bearing for correctness, just kept consistent with the
  already-reviewed v2 shape.

## External dependencies

None beyond the base toolchain. `sha2` is not new to the workspace (Plan's citation,
`Cargo.toml:147`); `cargo deny check` stays green with no license/ban change. `typos`,
`cargo-machete` clean. Did not run `cargo-mutants` (not part of a quick red/green check; the
brief lists it as a registered doctor id for Check's own gates, not something Do runs).

## Self-review against the target's standing rubric (AGENTS.md § Review rubric & protocol)

- **One clock per correctness lifecycle**: no clock reads added — this module stays pure
  (no `SystemTime::now()`, no lease/fence stamping); N/A.
- **Narrow trait seams / dependency direction**: no new traits or seams; N/A.
- **Metadata validation boundaries**: `canonical_named_parts`'s refusal and
  `MultipartEtag::parse`'s grammar check both validate at the point the value is constructed
  from untrusted input (a Complete request's part list, a stored/wire ETag string) — the
  ADR-0045 "validate at decode/construction, never as a silently-corrected value" rule leg
  2/3/4 exist to enforce.
- **No DST-reachable shared mutable global state**: no statics added.
- **`#![forbid(unsafe_code)]`**: crate root already has it (`crates/core/src/lib.rs:9`); the
  new test file states it too (`multipart_state_machine.rs:26`), matching its siblings.
- **Docs currency**: no port/API/RPC/CLI flag/persisted field changed — `Completion`'s shape
  is untouched, no wire mapping added (explicitly #508's). No living-architecture-doc edit is
  owed by this child.
- **Recurring defect classes**: grammar strictness (leg 4, reusing `canonical_decimal` rather
  than a new ad hoc parser) and serialization identity (`MultipartEtag`'s `Serialize`/
  `Deserialize` round-trips through the same validating `parse`/`Display` pair, leg 4's
  round-trip test) are the two classes this diff's surface touches; the others (protocol
  input framing, transactions, await discipline, probes, DST fidelity, workflow edits) don't
  apply to a pure, store-free, sync module.
- **Exhaustive enums (Plan-pinned)**: no `#[non_exhaustive]` on any new enum — leg 5 asserts
  this by matching every one without a wildcard arm.
