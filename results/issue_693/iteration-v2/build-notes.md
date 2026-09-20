# Build notes — issue #693 (multipart-state-machine-digests), iteration 2

Base: `getwyrd/wyrd` `main` at `3969a3a` (the #772 merge, so the grammar and record children
are in). All `path:line` below are on that base unless marked **patched** (the tree after
`patch.diff`). The patch applies cleanly to a fresh checkout of the base and reproduces the
tested files byte for byte (checked with `git apply` + `cmp` on all five files).

## What iteration 1 got wrong, and what this round does about it

The carry-forward named three things. Each is fixed at its cause, and each has a negation below
that goes red without the fix.

1. **The retry answered the wrong ETag** (T4 review, 4 blocking findings, all the same bug;
   C3 FAIL). A `Completed` tombstone must answer an identical retry with the ETag the first
   Complete returned, `<hex>-N`. Iteration 1's `Publication.etag` was a bare `Digest` because
   the record it copies from, `Completion.etag`, was typed `Digest` by the previous child
   (`crates/core/src/multipart.rs:1863`) before the composition existed. A `Digest` has no room
   for `-N`, so no code could return the right token.
   **Fix: the record stores the whole ETag.** `Completion.etag` is now a `MultipartEtag`
   (**patched** `multipart.rs:1957`), and `Publication.etag` too (**patched** `:4009`);
   `Publication::of` copies it verbatim (**patched** `:4018`). This is what 0016 says the
   record holds: "the value is in any case recorded in the `Completed` session record"
   (`0016:3068-3069`), and the salvage patch had it this way
   (`results/issue_654/iteration-v2/patch.diff:1201`). A stored record with a bare digest now
   fails to decode, because it could only ever answer a retry wrongly.

2. **Counts past `u32::MAX` got the wrong error** (T2 FAIL; review finding at old
   `multipart.rs:3769`). Iteration 1 parsed the count as `u32` first, so a canonical count like
   `4294967296` came back "malformed" while the range error carried a `u64`. Switching to `u64`
   only moves the same problem up one width. **Fix:** the parse now checks the grammar first
   (canonical decimal, whatever its size), then the range, and the range error carries the
   count **as read, as text** (`EtagPartCountOutOfRange { count: String }`, **patched**
   `multipart.rs:545-550`, parse at `:3791-3809`). Every canonical count outside
   `[1, MAX_PART_NUMBER]`, including one wider than any integer type, gets the range error.
   To do this without writing a second decimal parser I split the grammar half out of the
   existing shared helper: `is_canonical_decimal` (**patched** `:1175-1179`), with
   `canonical_decimal` (base `:1087-1095`) now calling it. Behaviour of the existing helper is
   unchanged; `multipart_keys.rs` (21 tests) still passes, and the mutants on those lines are
   caught.

3. **Two surviving mutants and a digest-only test oracle** (C5 FAIL; C5 NEEDS-HUMAN [impl]).
   - `replace > with >= in MultipartEtag::parse`: nothing tested a count of exactly
     `MAX_PART_NUMBER`. Now `multipart_etag_parse_accepts_the_whole_count_range`
     (**patched** test `:649`) accepts 1, 2, 10 000 and 999 999 and round-trips each through
     `Display` and serde; negation (f) below shows it catching the off-by-one.
   - `replace < with <= in canonical_named_parts`: an equivalent mutant (the `==` case was
     handled first, so `<` and `<=` behaved the same). I rewrote the check as one
     `match current.cmp(&previous)` with three arms (**patched** `:3745-3765`), which leaves no
     redundant comparison to mutate.
   - The leg-1 oracle now expects the **full** recorded ETag, built by an independent oracle
     with `N = 3` (neither 1 nor the highest part number, 4), and a new end-to-end test decodes
     a tombstone from stored bytes and checks the retry gets `...-3` verbatim
     (**patched** test `:422`).

   Re-run of the Check gate's own script (`scripts/mutants-in-diff`) on the final patch:
   **35 mutants, 11 caught, 24 unviable, 0 missed** (iteration 1: 2 missed). Most "unviable"
   mutants are an artefact of the workspace's `warnings = "deny"`: replacing a body with a
   constant leaves a parameter unused, which fails the build. The one semantic guard mutant that
   did build (`==` → `!=` in the retry-fingerprint guard, **patched** `:4243`) was caught.

## Why fix the record instead of rebuilding `-N` from the request

The alternative I rejected: keep `Completion.etag: Digest` and have the tombstone rebuild the
ETag as `<recorded digest>-<N from the request>`. After a fingerprint match the request lists
the same parts as the winning Complete, so the count would come out right. Cost, concretely:

```rust
// complete_answer would need the count as well as the fingerprint (every caller changes):
pub fn complete_answer(state: Option<&SessionState>, request: Option<(&Digest, u32)>) -> CompleteAnswer
// and Publication would need a second constructor that mixes record and request:
pub fn retried(completion: &Completion, request_parts: u32) -> Self { /* etag: MultipartEtag { composed: completion.etag, parts: request_parts } */ }
```

That is about 8 changed lines against this fix's 1-line type change plus 4 changed lines in
the old test's fixtures — not cheaper. Its only saving is not touching a fifth file. And it
treats the symptom: the stored record would still not hold the ETag 0016 says it holds, and the
answer would be half recorded, half re-derived, relying on SHA-256 collision resistance to be
right. Fixing the type removes the cause, which is what the brief's Invariant to restore (C-1:
"the convenient answer is never a silently wrong one") asks for.

Also rejected: adding a separate `parts: u32` field to `Completion` next to the digest. Same
file count, and it splits one value across two fields that must agree.

## Deviations from the brief — for the human at sign-off

- **Five files, not four.** The record fix forces a fixture change in the previous child's
  test, `crates/core/tests/multipart_session_records.rs` (base `:294`, `:309`, `:337`, `:342`:
  the hand-written stored ETag gains its `-N` suffix and two assertions compare the rendered
  token). 6 lines added, 4 removed. The brief's file list was set before the review found that
  the record type could not hold the answer.
- **Line budget is about 8% over.** Brief: ≤ 950 added semantic lines (module ≈ 350,
  test ≈ 550). Measured on `patch.diff`, counting every non-blank line that is not a comment:
  module 335, new test 683, other files 6 — **1,024 total**. The overage is test code, mostly
  what the review asked for (whole-ETag retry, stored-bytes end-to-end, count boundaries) plus
  rustfmt spreading nested literals vertically. I already compacted the 25-cell table to one
  row per verb and merged three refusal tests into one; going further would drop evidence.
- **Docs currency — your call.** The rubric requires updating the living architecture doc when
  a persisted field changes, and `Completion.etag` changes spelling from a bare digest to
  `<hex>-N`. The brief pins `docs/design/` untouched, and I kept to that. The doc describes
  `mpu:<id>` only as "a session's target, lifecycle state and epoch"
  (`docs/design/architecture/05-building-block-view.md:202`), so no sentence there becomes
  false, and nothing writes these records yet. If you want the doc to name the recorded ETag
  anyway, one clause after "lifecycle state and epoch" would do: "— once `Completed`, the
  multipart ETag it published (`<lowercase-hex SHA-256 of the parts' raw digests in part
  order>-<part count>`) and the fingerprint of the part list that won, so only an identical
  retry is answered from the tombstone". A reviewer may raise this as a CONVENTION finding;
  if so it needs your decision, not a rebuild. The inode ETag wording at `:189` ("the
  lowercase-hex SHA-256") stays true until a multipart object is actually published, which is
  #508/#658's change.

## The rest of the design, briefly

- **Refuse, never sort** (`canonical_named_parts`, **patched** `:3745`): returns the same slice
  after checking it is non-empty and strictly ascending. No copy, no sort. Both digests call it,
  so they refuse the same lists with the same errors.
- **Fingerprint format** (**patched** `:3874-3901`): SHA-256 over `be32(n_i) ‖ d_i` per named
  part, in request order. Every record is 36 bytes, so different lists never share a preimage;
  iteration 1's 8-byte length prefix added nothing and is gone. The test pins the format to an
  oracle (`complete_fingerprint_is_the_oracle`) because it is stored and compared across
  retries.
- **ETag format** (**patched** `:3841-3872`): `lowercase_hex(SHA-256(d_1 ‖ … ‖ d_N)) + "-" + N`
  over raw digest bytes, `N` the named count. Both digests hash through one private helper,
  `Digest::sha256(feed)` (**patched** `:1018-1026`), which feeds the hasher piece by piece
  instead of first copying up to 36 MB (999 999 parts × 36 bytes) into a buffer as iteration 1
  did. I did not add the public `Digest::of` the previous child's doc comments promised
  (base `:938-941`, `:1016-1017`): nothing in scope needs a one-shot hash, and the brief does
  not list it. Those two doc comments now say where the hashing is instead.
- **Outcome enums** (**patched** `:3903-4102`): taken from the salvage with iteration 1's
  vocabulary, but `Publication` and `Budget` are no longer boxed inside the enums. The largest
  payload, `Publication`, is 64 bytes, far below clippy's `large_enum_variant` threshold, and
  unboxed values are simpler for every consumer. No `#[non_exhaustive]` anywhere (leg 5).
- **Answer table** (**patched** `:4104-4313`): unchanged in behaviour from iteration 1, one
  function per verb plus the total `answer` dispatcher; `complete_answer` answers the tombstone
  with a match guard on the fingerprint. I also corrected two doc slips: iteration 1 said a
  `Completing` session answers "every client verb" with 409, which is false (UploadPart gets
  NoSuchUpload, ListParts gets the frozen set; 0016:980 means every client *Complete*), and I
  removed HTTP status numbers from the docs, since the status mapping is #508's.

## Demonstrated red — the brief's four named negations (binding) plus four more

All run through `cargo test -p wyrd-core --test multipart_state_machine` on the **final** tree,
one negation at a time, then restored (scripted, so each is exactly the stated edit). Line
numbers are the final test file's.

### (a) Answer one `Completing` cell as if `Open`
`complete_answer`: `Completing => CompleteAnswer::Fences`.
```
test a_non_identical_complete_is_refused_by_the_tombstone ... FAILED
test every_decision_3_cell_is_answered_for_an_identical_retry ... FAILED
panicked at crates/core/tests/multipart_state_machine.rs:321:13:
assertion `left == right` failed: CompleteMultipartUpload × Completing
  left: Complete(Fences)
 right: Complete(Refused(OperationAborted))
test result: FAILED. 20 passed; 2 failed
```

### (b) Concatenate hex text instead of raw digest bytes (leg 2)
`multipart_etag`: `hasher.update(digest.to_hex().as_bytes())`.
```
test multipart_etag_is_over_raw_digest_bytes_not_hex_text ... FAILED
test multipart_etag_of_the_winning_list_is_the_recorded_etag ... FAILED
test multipart_etag_suffix_is_the_named_count ... FAILED
test multipart_etag_of_one_part_is_the_oracle ... FAILED
test multipart_etag_of_a_strict_subset_differs_from_the_full_set ... FAILED
panicked at crates/core/tests/multipart_state_machine.rs:510:5:
assertion `left != right` failed
  left: "5e17af24ddd878d6164cd74162ed5d64b2091e9c7eac946c45fb4bdd536893b3-3"
 right: "5e17af24ddd878d6164cd74162ed5d64b2091e9c7eac946c45fb4bdd536893b3-3"
panicked at crates/core/tests/multipart_state_machine.rs:479:5:
  left: "f2f065ea94adfa26205f510752dbd9271589de9f600e782f615d8fdb2f688a2e-1"
 right: "0018e0e3babbc9f34cfaadf921b6e92dea1318e245a364dd929ed1257a40fa0c-1"
test result: FAILED. 17 passed; 5 failed
```

### (c) Ignore part numbers in the fingerprint (leg 3)
`complete_fingerprint`: stop hashing `part_number.to_be_bytes()`.
```
test complete_fingerprint_is_the_oracle ... FAILED
test a_stored_tombstone_answers_an_identical_retry_with_the_whole_recorded_etag ... FAILED
test complete_fingerprint_disagrees_on_the_same_digests_under_different_numbers ... FAILED
test every_decision_3_cell_is_answered_for_an_identical_retry ... FAILED
panicked at crates/core/tests/multipart_state_machine.rs:593:5:
assertion `left != right` failed
  left: Digest([75, 188, 100, 47, 163, 93, 14, 169, ...])
 right: Digest([75, 188, 100, 47, 163, 93, 14, 169, ...])   (identical)
test result: FAILED. 18 passed; 4 failed
```
Side note: under this negation the fingerprint equals the ETag's digest half
(`[75, 188, ...]` in both), which is exactly why the two must be separate compositions.

### (d) Sort a non-ascending list instead of refusing it (legs 2 and 3)
`canonical_named_parts` replaced by the salvage's `BTreeMap` coalesce-and-sort.
```
test complete_fingerprint_refuses_what_multipart_etag_refuses ... FAILED
test multipart_etag_refuses_a_non_ascending_duplicate_or_empty_list ... FAILED
panicked at crates/core/tests/multipart_state_machine.rs:629:9:
assertion `left == right` failed: [(2, ...), (1, ...), (4, ...)]
  left: Ok(Digest([155, 190, 175, 250, ...]))
 right: Err(PartsOutOfOrder { part_number: 1, previous: 2 })
panicked at crates/core/tests/multipart_state_machine.rs:549:9:
  left: Ok(MultipartEtag { composed: Digest([75, 188, ...]), parts: 3 })
 right: Err(PartsOutOfOrder { part_number: 2, previous: 4 })
test result: FAILED. 20 passed; 2 failed
```
The `[155, 190, ...]` value is the winning list's fingerprint: with sorting, a reordered
(invalid) request would have matched the tombstone and been told it succeeded. In iteration 2's
first cut the leg-3 test only failed indirectly here (at its `multipart_etag` call); I rewrote
it to check `complete_fingerprint` against the exact expected error on its own.

### (e) The tombstone keeps the digest but loses the recorded count (the review's bug class)
`Publication::of`: `etag: MultipartEtag { composed: completion.etag.composed(), parts: 1 }`.
```
test a_stored_tombstone_answers_an_identical_retry_with_the_whole_recorded_etag ... FAILED
test every_decision_3_cell_is_answered_for_an_identical_retry ... FAILED
panicked at crates/core/tests/multipart_state_machine.rs:433:5:
  left: "4bbc642fa35d0ea92af584ca4dfd035cecd6f82eb32dc60876ded2cc8d79dcfe-1"
 right: "4bbc642fa35d0ea92af584ca4dfd035cecd6f82eb32dc60876ded2cc8d79dcfe-3"
test result: FAILED. 20 passed; 2 failed
```

### (e2) Iteration 1's shape: record and answer hold only a `Digest`
Both `etag` fields retyped to `Digest`. The test does not compile:
```
error[E0308]: mismatched types  --> crates/core/tests/multipart_state_machine.rs:138:15
error[E0308]: mismatched types  --> crates/core/tests/multipart_state_machine.rs:202:27
error[E0599]: no method named `parts` found for struct `wyrd_core::multipart::Digest`
              --> crates/core/tests/multipart_state_machine.rs:435:33
```

### (f) Exclude exactly `MAX_PART_NUMBER` (iteration 1's surviving mutant)
`(1..=MAX_PART_NUMBER)` → `(1..MAX_PART_NUMBER)`.
```
test multipart_etag_parse_accepts_the_whole_count_range ... FAILED
panicked at crates/core/tests/multipart_state_machine.rs:652:48:
a count in range parses: EtagPartCountOutOfRange { count: "999999" }
test result: FAILED. 21 passed; 1 failed
```

### (g) Iteration 1's parse: count read as `u32` first
```
test multipart_etag_parse_refuses_a_count_outside_the_range ... FAILED
panicked at crates/core/tests/multipart_state_machine.rs:683:9:
assertion `left == right` failed: count 4294967296
  left: Err(MultipartEtagMalformed { etag: "88d98b...28ab-4294967296" })
 right: Err(EtagPartCountOutOfRange { count: "4294967296" })
test result: FAILED. 21 passed; 1 failed
```

## Refuting my own test

**(a) Genuine red?** Yes, in two ways. With the production files reverted to the base
(`git checkout -- crates/core/src/multipart.rs crates/core/Cargo.toml Cargo.lock`, tests kept):
- the new test fails to **compile** (`unresolved import sha2`, unresolved `answer`,
  `multipart_etag`, `MultipartEtag`, … and missing `RecordError` variants; cargo exit 101).
  That is the red the brief pre-declared: born-at-tier, so C4-verify will report UNVERIFIABLE
  (exit 77) and the eight negations above stand in for a behavioural red;
- the edited `multipart_session_records.rs` **compiles and fails at runtime** against the base,
  which is a real behavioural red for the record fix:
  ```
  test completion_round_trips_standalone ... FAILED
  test session_completed_round_trips ... FAILED
  a Completed session decodes: MalformedRecordValue { namespace: "mpu:", detail:
    "\"abab...abab-3\" is not 64 lowercase-hex characters (a SHA-256 digest) at line 1 column 361" }
  test result: FAILED. 32 passed; 2 failed
  ```
  C4-verify only runs the added test file, so it will not show this; it is here for you.

**(b) Production path?** Yes. Every assertion calls the production functions (`answer` and the
five per-verb functions, `multipart_etag`, `complete_fingerprint`, `MultipartEtag::parse`,
serde through `metadata::encode`/`decode`, and `decode_session_record` on stored bytes). The
only code the test writes itself is the oracles: SHA-256 straight from `sha2` and hex through
`format!("{b:02x}")`, deliberately not the production hashing or `hex_lower`. They touch
production only through `Digest::from_bytes`/`as_bytes`.

**(c) Fixture includes the fault?** Yes. The table test runs all 25 cells, with three request
inputs (matching fingerprint, a different assembly's, none), and fails on any verb without a
row; each row is an array of exactly `STATES` answers, and `Column::of` matches `SessionState`
with no wildcard, so a new state cannot slip past. The retry fixture is the case the bug lives
in: a tombstone decoded from stored bytes with `N = 3`, answered after its part records would be
gone. The count tests include the exact boundaries the review named (999 999 accepted,
1 000 000 and 4 294 967 296 refused as out of range).

## Gates I ran (fast pass; Check re-runs the real ones)

- `cargo test -p wyrd-core --test multipart_state_machine`: 22 passed (the brief's GREEN leg,
  wrapped in `timeout`). Neighbours: `multipart_session_records` 34 passed,
  `multipart_keys` 21 passed.
- `./engine/xtask.sh ci` (the C4-ci gate command): **`xtask ci: all checks passed`**, exit 0,
  with no step skipped: typos, docs lint and render, gitlink and unsafe guards, fmt, clippy,
  build, the whole workspace test suite, cargo-machete, cargo-deny (three runs), conformance,
  statics, deploy-guard, DST clippy and DST tests. It ran before my last test-only reshaping
  (table layout, merged refusal test, leg-5 assertions); after that I re-ran `cargo fmt --all
  -- --check`, `cargo clippy -p wyrd-core --all-targets`, `typos` on every touched file and the
  test itself, all clean.
- `scripts/mutants-in-diff`: 35 mutants, 11 caught, 24 unviable, 0 missed.
- No commit hooks are configured in the target (`core.hooksPath` unset, no active
  `.git/hooks`, no pre-commit config); the formatter is rustfmt via `cargo fmt`, which is clean.

No external dependency was missing: `typos`, the docs renderer, `cargo-deny`, `cargo-machete`
and `cargo-mutants` all ran. No NEEDS-HUMAN external-dependency item.

## Self-review against the target's rubric (`AGENTS.md` § Review rubric & protocol)

- One clock per lifecycle: no clock read added. N/A.
- Trait seams / dependency direction: no trait or seam added. `sha2` was already a workspace
  dependency (`Cargo.toml:147`) used by `gateway-s3` and `server`; `cargo deny` and
  `cargo machete` are green.
- Metadata validation boundaries (ADR-0045): `Completion.etag` is validated at decode through
  `MultipartEtag`'s `Deserialize`; the count bound is the key space's format bound
  (`MAX_PART_NUMBER`), not a live knob, so strict at decode is right.
- Shared mutable global state: none (statics gate green).
- `#![forbid(unsafe_code)]`: the new test crate root has it (`multipart_state_machine.rs:35`);
  unsafe-guard green.
- Docs currency: see the sign-off item above.
- Grammar strictness: the count is checked as canonical decimal (no sign, no leading zero,
  ASCII digits only) before any `from_str`, and the shared decimal rule was extended rather than
  copied.
- Serialization identity: `MultipartEtag` serializes as its one canonical text and parses only
  that, so decode→encode is byte-identical; tested directly (serde round trip for every count
  boundary) and through `decode_session_record`'s canonical-bytes gate on a stored tombstone.
- Absent or unsupported entries: every refusal is a typed error or a typed `Refusal`; nothing is
  skipped or defaulted.
- Test fidelity: pure functions, no DST model involved; not a destructive or concurrent path.

## Leftovers

The worktree keeps the applied change (the harness owns it). My scratch files are under
`$PDCA_SCRATCH/pdca-builder-693-negations` (file copies, the negation script and transcripts,
under 1 MB) and `$PDCA_SCRATCH/pdca-builder-693-ci` (the ci log). The workspace test run inside
`xtask ci` also left about 150 small `.tmp*` dirs (2.4 MB) there. The 977 MB
`pdca-reviewer-693-rerun` dir in the same scratch root is the previous round's reviewer, not
mine. I left all of it for the harness to reclaim.
