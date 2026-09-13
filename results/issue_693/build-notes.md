# Build notes — issue #693 (multipart-state-machine-digests), iteration 3

Base: `getwyrd/wyrd` `main` at `3969a3a` (the #772 merge, so the grammar and record children
are in). Every `path:line` below is on the tree **after** `patch.diff` unless marked *base*.
`patch.diff` applies cleanly to a fresh copy of the base and reproduces all six files byte for
byte (checked with `git archive HEAD` + `git apply` + `cmp`).

## What blocked iteration 2, and what this round changes

Iteration 2 failed one gate: the T4 batched review, with 3 blocking findings that are one
finding seen by each of the 3 passes (`review-batch.md`): changing the persisted
`Completion.etag` from a bare digest to the composed `<digest>-<N>` needs a same-PR update of the
living architecture doc (the rubric's "Docs currency" rule), and there was none.

The finding is right. The rubric makes a persisted-field change a merge requirement for the
doc, and this module's own header already says the doc records these record types
(*base* `crates/core/src/multipart.rs:82-95`). Iteration 2 left it out only because the brief
says `docs/design/` untouched.

**The fix is the doc, not a code change:**

1. `docs/design/architecture/05-building-block-view.md:204` — one new paragraph, right after the
   multipart record paragraph at `:202`. It says: how a multipart object's ETag is composed and
   its count range; that a `Completed` session record's `etag` holds it whole and decode refuses
   any other spelling, and why (the part records may be retired before a retry arrives); what
   `complete_fingerprint` is and what it is for; that both digests refuse, never sort, a list
   that is empty or not strictly ascending; and that decision 3's table is landed as pure
   functions in a typed vocabulary with no HTTP status (#508 owns the wire mapping).
2. `crates/core/src/multipart.rs:94-99` — the module header's pointer to the doc now cites
   `:202-204` and says the doc also records the answer table and the stored ETag/fingerprint.

Everything else is iteration 2's code, which the Check reviewer passed on C3, C5, T1, T2, T3 and
T5, plus five doc-comment fixes I made while re-reading it (no behaviour change):

- `crates/core/src/multipart.rs:3774-3776` — `MultipartEtag` doc had a garbled sentence ("a
  record carrying it one a whole-record CAS … could never match").
- `crates/core/src/multipart.rs:3966-3968` — `Refusal::InvalidPart` said only "does not match the
  frozen part set"; it also covers an out-of-order list (`0016:993-994`).
- `crates/core/src/multipart.rs:4187-4189` — `ListUploadsAnswer::NotListed` said "tearing down or
  torn down", wrong for a `Completed` session; each variant now names its states.
- `crates/core/src/multipart.rs:4127-4130` — `Verb::ALL`'s doc implied the compiler checks the
  array is complete. It does not (Rust cannot list an enum's variants); the compiler checks
  `answer`'s exhaustive match. Same pattern, and same limit, as *base* `RetireMode::ALL`
  (`crates/core/src/multipart.rs:1339`).
- `crates/core/Cargo.toml:25-27` — `deny.toml`'s allowlist is a **licence** allowlist
  (`deny.toml:89-95`), not a crate list; the comment now says so.

## Why document the record rather than keep it unchanged

Two ways clear the finding.

**(A) Chosen:** keep `Completion.etag: MultipartEtag` (`crates/core/src/multipart.rs:1958`) and
document it. Cost: the doc paragraph (+2 lines incl. the blank) and the header pointer
(+5/−4 lines of comment).

**(B) Rejected:** put `Completion.etag` back to `Digest` (no persisted change, so no doc rule
applies) and build the retry's ETag from the record's digest plus the request's part count,
which equals the winner's count whenever the fingerprints match. Concretely:

```rust
// the request's count has to reach the tombstone cell, so both entry points change:
pub fn complete_answer(state: Option<&SessionState>, request: Option<(&Digest, u32)>) -> CompleteAnswer
pub fn answer(verb: Verb, state: Option<&SessionState>, request: Option<(&Digest, u32)>) -> Answer
// Publication::of can no longer copy the record; it needs the count from outside it:
pub fn retried(completion: &Completion, parts: u32) -> Result<Self, RecordError>
// and MultipartEtag needs a public digest+count constructor with its own range check
pub fn new(composed: Digest, parts: u32) -> Result<Self, RecordError>
```

About 15 changed production lines, plus 13 test call sites (`answer`, `complete_answer`,
`assert_every_cell_answered`) and the two `Completion` fixtures, against 2 doc lines. Cost is
not the main reason, though. (B) contradicts 0016: the root flip writes
`Completed{inode, version, etag, completed_at_millis}` (`0016:941-942`), "the value is in any
case recorded in the `Completed` session record" (`0016:3068-3069`), and the tombstone answers
"the recorded ETag" (`0016:898-908`, brief leg 1). Under (B) half the answer would come from
the request. Iteration 1 was sent back (4 blocking findings) for a retry answer that was not the
whole recorded ETag, so (B) walks back toward that.

**Also not done:** writing a `review-rejected.md` line for the finding. It is correct under the
rubric, so I have no honest reason to reject it.

## Deviations from the brief — for you at sign-off

- **Six files, not four.** `crates/core/tests/multipart_session_records.rs` (the record fix's
  fixture, *base* `:294`, `:309`, `:337`, `:342`: the stored ETag gains its `-N`, two assertions
  compare the rendered token; +6/−4 lines incl. comments) and
  `docs/design/architecture/05-building-block-view.md` (the docs-currency fix). The brief says
  `docs/design/` untouched; the rubric says a persisted-field change updates the living doc in
  the same PR, as a merge requirement. The brief's scope was set before iteration 1 found that
  the record type could not hold the answer, so I followed the rubric. This is the C1 item from
  iteration 2's §6 ("widen Plan to the existing record fixture and living architecture"), and
  it stays your decision. If you would rather keep the brief's four files, (B) above is the
  only route I see, and I think it is the worse one.
- **Line budget is over.** Brief: ≤ 950 added semantic lines (module ≈ 350, test ≈ 550).
  Measured on `patch.diff` by script, counting added lines that are not blank and not a
  comment (`//`, `///`, `//!`, TOML `#`; attributes counted): module **352**, new test **707**,
  other files **7** — **1,066** total, about 12 % over. The module is on estimate; the overage is
  test evidence the iteration-1 and iteration-2 reviews asked for (whole-ETag retry from stored
  bytes, the count boundaries, the past-`u32::MAX` count) plus rustfmt spreading nested
  literals. I did not cut evidence to hit the number. (Iteration 2's notes said 1,024 with a
  different counting rule; the code is the same.)
- **Something the doc paragraph does not change:** the inode line
  `05-building-block-view.md:189` ("the lowercase-hex SHA-256") stays true until a multipart
  object is actually published, which is #508/#658's change and the place to update it.

## The design, briefly (unchanged from iteration 2)

- **Refuse, never sort** — `canonical_named_parts` (`crates/core/src/multipart.rs:3746-3766`)
  checks non-empty and strictly ascending and hands back the same slice. No copy, no sort. Both
  digests call it, so they refuse the same lists with the same errors (`NoPartsNamed`,
  `DuplicatePart`, `PartsOutOfOrder`, `:514-533`).
- **ETag** — `multipart_etag` (`:3864-3874`): `lowercase_hex(SHA-256(d_1 ‖ … ‖ d_N)) + "-" + N`
  over raw digest bytes, `N` the named count. Both digests hash through one private helper,
  `Digest::sha256` (`:1023-1027`), fed piece by piece.
- **Fingerprint** — `complete_fingerprint` (`:3895-3903`): SHA-256 over `be32(n_i) ‖ d_i` per
  named part, in request order. Every record is 36 bytes, so different lists never share a
  preimage.
- **`MultipartEtag`** (`:3778-3841`): private fields; obtained only by composing or by
  `parse` (`:3793-3812`), which checks the hex half, then the count's grammar
  (`is_canonical_decimal`, `:1176-1180`, split out of the existing `canonical_decimal`,
  `:1164-1169`, so one rule serves both), then its range `[1, MAX_PART_NUMBER]`. The range
  error carries the count as text, so a count wider than any integer is "out of range", not
  "malformed". Serde goes through `parse`/`Display`, so decode→encode is byte-identical.
- **Record** — `Completion.etag: MultipartEtag` (`:1958`), and its doc (`:1941-1946`) says why
  it is stored whole.
- **Outcome vocabulary** (`:3906-4104`) and **answer table** (`:4106-4317`): no
  `#[non_exhaustive]` anywhere (leg 5, and the comment at `:3910-3913`); one function per verb
  plus the total `answer` dispatcher (`:4303`); `complete_answer` (`:4239-4255`) answers the
  tombstone through a match guard on the fingerprint, and `Publication::of` (`:4021-4029`)
  copies the recorded ETag verbatim.
- **Reading of "the two conditional cells"** (brief leg 1): 0016's table has one conditional
  cell, `CompleteMultipartUpload` × `Completed`. I read "two" as its two branches (match,
  mismatch) and test both, plus a third input (no fingerprint at all, i.e. a list
  `complete_fingerprint` refused).

## Demonstrated red — the brief's four named negations, plus seven more

Each run is one exact edit to `crates/core/src/multipart.rs`, applied by script, then
`timeout 580 cargo test -p wyrd-core --test multipart_state_machine` (the brief's GREEN-leg
command), then the file restored and its SHA-256 re-checked. Baseline on the final tree:
`test result: ok. 22 passed; 0 failed`. Test-file line numbers are the final file's.

### (a) Answer the `Completing` cell as if `Open` — brief negation (a), leg 1
Edit: in `complete_answer`, `Some(SessionState::Completing { .. }) => CompleteAnswer::Fences`.
```
test a_non_identical_complete_is_refused_by_the_tombstone ... FAILED
test every_decision_3_cell_is_answered_for_an_identical_retry ... FAILED
panicked at crates/core/tests/multipart_state_machine.rs:321:13:
assertion `left == right` failed: CompleteMultipartUpload × Completing
  left: Complete(Fences)
 right: Complete(Refused(OperationAborted))
test result: FAILED. 20 passed; 2 failed
```

### (b) Concatenate hex text instead of raw digest bytes — brief negation (b), leg 2
Edit: in `multipart_etag`, `hasher.update(digest.to_hex().as_bytes())`.
```
test multipart_etag_of_one_part_is_the_oracle ... FAILED
test multipart_etag_is_over_raw_digest_bytes_not_hex_text ... FAILED
test multipart_etag_of_the_winning_list_is_the_recorded_etag ... FAILED
test multipart_etag_of_a_strict_subset_differs_from_the_full_set ... FAILED
test multipart_etag_suffix_is_the_named_count ... FAILED
panicked at crates/core/tests/multipart_state_machine.rs:479:5:
  left: "f2f065ea94adfa26205f510752dbd9271589de9f600e782f615d8fdb2f688a2e-1"
 right: "0018e0e3babbc9f34cfaadf921b6e92dea1318e245a364dd929ed1257a40fa0c-1"
panicked at crates/core/tests/multipart_state_machine.rs:510:5:
assertion `left != right` failed
  left: "5e17af24ddd878d6164cd74162ed5d64b2091e9c7eac946c45fb4bdd536893b3-3"
 right: "5e17af24ddd878d6164cd74162ed5d64b2091e9c7eac946c45fb4bdd536893b3-3"
test result: FAILED. 17 passed; 5 failed
```

### (c) Ignore part numbers in the fingerprint — brief negation (c), leg 3
Edit: in `complete_fingerprint`, drop `hasher.update(part_number.get().to_be_bytes())`.
```
test a_stored_tombstone_answers_an_identical_retry_with_the_whole_recorded_etag ... FAILED
test complete_fingerprint_disagrees_on_the_same_digests_under_different_numbers ... FAILED
test complete_fingerprint_is_the_oracle ... FAILED
test every_decision_3_cell_is_answered_for_an_identical_retry ... FAILED
panicked at crates/core/tests/multipart_state_machine.rs:593:5:
assertion `left != right` failed
  left: Digest([75, 188, 100, 47, 163, 93, 14, 169, ...])
 right: Digest([75, 188, 100, 47, 163, 93, 14, 169, ...])   (identical)
panicked at crates/core/tests/multipart_state_machine.rs:321:13:
assertion `left == right` failed: CompleteMultipartUpload × Completed
  left: Complete(Refused(NoSuchUpload))
 right: Complete(AlreadyCompleted(Publication { inode: 9, version: 4, etag: MultipartEtag { .. parts: 3 }, .. }))
test result: FAILED. 18 passed; 4 failed
```
Under this negation the fingerprint equals the ETag's digest half (`[75, 188, …]` in both),
which is why the two must be separate compositions.

### (d) Sort a non-ascending list instead of refusing it — brief negation (d), legs 2 and 3
Edit: `canonical_named_parts` copies, sorts by part number, de-duplicates, and returns the sorted
copy (leaked, so the signature and both callers stay exactly as they are).
```
test multipart_etag_refuses_a_non_ascending_duplicate_or_empty_list ... FAILED
test complete_fingerprint_refuses_what_multipart_etag_refuses ... FAILED
panicked at crates/core/tests/multipart_state_machine.rs:549:9:
  left: Ok(MultipartEtag { composed: Digest([75, 188, ...]), parts: 3 })
 right: Err(PartsOutOfOrder { part_number: 2, previous: 4 })
panicked at crates/core/tests/multipart_state_machine.rs:629:9:
  left: Ok(Digest([155, 190, 175, 250, ...]))
 right: Err(PartsOutOfOrder { part_number: 1, previous: 2 })
test result: FAILED. 20 passed; 2 failed
```
`[155, 190, …]` is the winning list's fingerprint: with sorting, a reordered (invalid) request
would have matched the tombstone and been told it succeeded.

### (e) The retry keeps the digest but loses the recorded count (iteration 1's bug class)
Edit: `Publication::of` sets `etag: MultipartEtag { composed: completion.etag.composed(), parts: 1 }`.
```
test every_decision_3_cell_is_answered_for_an_identical_retry ... FAILED
test a_stored_tombstone_answers_an_identical_retry_with_the_whole_recorded_etag ... FAILED
panicked at crates/core/tests/multipart_state_machine.rs:433:5:
  left: "4bbc642fa35d0ea92af584ca4dfd035cecd6f82eb32dc60876ded2cc8d79dcfe-1"
 right: "4bbc642fa35d0ea92af584ca4dfd035cecd6f82eb32dc60876ded2cc8d79dcfe-3"
test result: FAILED. 20 passed; 2 failed
```

### (e2) Iteration 1's shape: record and answer hold a bare `Digest`
Edit: both `pub etag: MultipartEtag` fields retyped to `Digest`. The test does not compile:
```
error[E0308]: mismatched types  --> crates/core/tests/multipart_state_machine.rs:138:15
error[E0308]: mismatched types  --> crates/core/tests/multipart_state_machine.rs:202:27
error[E0599]: no method named `parts` found for struct `wyrd_core::multipart::Digest`
              --> crates/core/tests/multipart_state_machine.rs:435:33
```

### (f) Exclude exactly `MAX_PART_NUMBER` (iteration 1's surviving mutant), leg 4
Edit: `(1..=MAX_PART_NUMBER)` → `(1..MAX_PART_NUMBER)` in `MultipartEtag::parse`.
```
test multipart_etag_parse_accepts_the_whole_count_range ... FAILED
panicked at crates/core/tests/multipart_state_machine.rs:652:48:
a count in range parses: EtagPartCountOutOfRange { count: "999999" }
test result: FAILED. 21 passed; 1 failed
```

### (g) Iteration 1's parse: count read as `u32` first, leg 4
Edit: `let parts: u32 = canonical_decimal(count).ok_or_else(malformed)?;` then the range check.
```
test multipart_etag_parse_refuses_a_count_outside_the_range ... FAILED
panicked at crates/core/tests/multipart_state_machine.rs:683:9:
assertion `left == right` failed: count 4294967296
  left: Err(MultipartEtagMalformed { etag: "88d98b...28ab-4294967296" })
 right: Err(EtagPartCountOutOfRange { count: "4294967296" })
test result: FAILED. 21 passed; 1 failed
```

### (h) Accept a count of zero, leg 4
Edit: `(1..=MAX_PART_NUMBER)` → `(0..=MAX_PART_NUMBER)`.
```
test multipart_etag_parse_refuses_a_count_outside_the_range ... FAILED
assertion `left == right` failed: count 0
  left: Ok(MultipartEtag { composed: Digest([136, 217, ...]), parts: 0 })
 right: Err(EtagPartCountOutOfRange { count: "0" })
test result: FAILED. 21 passed; 1 failed
```

### (j) The tombstone answers any retry — the silent wrong answer 0016 forbids
Edit: the guard becomes `if request_fingerprint.is_none() || request_fingerprint.is_some()`.
```
test a_non_identical_complete_is_refused_by_the_tombstone ... FAILED
test a_stored_tombstone_answers_an_identical_retry_with_the_whole_recorded_etag ... FAILED
panicked at crates/core/tests/multipart_state_machine.rs:321:13:
assertion `left == right` failed: CompleteMultipartUpload × Completed
  left: Complete(AlreadyCompleted(Publication { inode: 9, version: 4, .. parts: 3 .. }))
 right: Complete(Refused(NoSuchUpload))
test result: FAILED. 20 passed; 2 failed
```

### (k) The tombstone answers no retry
Edit: the guard becomes `if request_fingerprint.is_none() && request_fingerprint.is_some()`.
```
test a_stored_tombstone_answers_an_identical_retry_with_the_whole_recorded_etag ... FAILED
test every_decision_3_cell_is_answered_for_an_identical_retry ... FAILED
assertion `left == right` failed: CompleteMultipartUpload × Completed
  left: Complete(Refused(NoSuchUpload))
 right: Complete(AlreadyCompleted(Publication { inode: 9, version: 4, .. parts: 3 .. }))
test result: FAILED. 20 passed; 2 failed
```
(j) and (k) cover by hand the two mutants `cargo mutants` could not build (guard → `true` /
`false` leave a binding unused or an arm unreachable, which `warnings = "deny"` refuses).

## Refuting my own test

**(a) Genuine red?** Yes, in two ways. With the three production files put back to the base
(`crates/core/src/multipart.rs`, `crates/core/Cargo.toml`, `Cargo.lock` from `HEAD`; tests
kept; restored afterwards and hash-checked):
- the new test fails to **compile** — `unresolved import sha2`, unresolved `answer`,
  `multipart_etag`, `MultipartEtag`, … and missing `RecordError` variants (17 errors, cargo exit
  101). That is the red the brief pre-declared: born-at-tier, so C4-verify will report
  UNVERIFIABLE (exit 77) again, and the eleven negations above stand in for a behavioural red;
- the edited `multipart_session_records.rs` **compiles and fails at runtime** against the base,
  a real behavioural red for the record fix:
  ```
  test completion_round_trips_standalone ... FAILED
  test session_completed_round_trips ... FAILED
  a Completed session decodes: MalformedRecordValue { namespace: "mpu:", detail:
    "\"abab...abab-3\" is not 64 lowercase-hex characters (a SHA-256 digest) at line 1 column 361" }
  test result: FAILED. 32 passed; 2 failed
  ```
  C4-verify runs only the added test file, so it will not show this.

**(b) Production path?** Yes. Every assertion calls the production functions — `answer` and the
five per-verb functions, `multipart_etag`, `complete_fingerprint`, `MultipartEtag::parse`, serde
through `metadata::encode`/`decode`, and `decode_session_record` on stored bytes. The only code
the test writes itself is the oracles: SHA-256 straight from `sha2`, hex through
`format!("{b:02x}")`, deliberately not the production hashing or `hex_lower`. They touch
production only through `Digest::from_bytes`/`as_bytes`.

**(c) Fixture includes the fault?** Yes. The table test runs all 25 cells with three request
inputs (matching fingerprint, a different assembly's, none) and fails on any verb in
`Verb::ALL` without a row; each row is an array of exactly `STATES` answers, and `Column::of`
matches `SessionState` with no wildcard. The retry fixture is the case the bug lives in: a
tombstone decoded from stored bytes with `N = 3` (neither 1 nor the highest part number, 4),
answered with no part records in reach. The count tests hit the exact boundaries (1 and
999,999 accepted; 0, 1,000,000, 4,294,967,295, 4,294,967,296 and a 40-digit count refused as
out of range). One honest limit: nothing in Rust can prove `Verb::ALL` lists every variant; a
new verb fails to compile in `answer` and in the test's `exhaust_verb` until someone handles
it, which is where they would add it to the array.

## Gates I ran (fast pass; Check re-runs the real ones)

- `timeout 580 cargo test -p wyrd-core --test multipart_state_machine` (the brief's GREEN leg):
  22 passed. Neighbours: `multipart_session_records` 34 passed, `multipart_keys` 21 passed.
- `./engine/xtask.sh ci` (the C4-ci gate command) on the final tree: **`xtask ci: all checks
  passed`**, exit 0, no step skipped — typos, docs lint, docs render with link audit, gitlink
  and unsafe guards, fmt, clippy, build, the whole workspace test suite, cargo-machete,
  cargo-deny (three runs), conformance, statics, deploy-guard, DST clippy and DST tests. No test
  failed anywhere in the log.
- `PDCA_BUNDLE=… scripts/mutants-in-diff` (the C5 gate command) on this `patch.diff`:
  **35 mutants, 11 caught, 24 unviable, 0 missed.** Every unviable one is a `Default::default()`
  swap on a type without `Default`, or an edit that leaves a parameter unused, which the
  workspace's `warnings = "deny"` refuses to build; (j)/(k) above cover the two semantic ones.
- `cargo fmt --all -- --check` clean; `cargo clippy -p wyrd-core --all-targets` clean; `typos`
  clean on all six files; `lint_docs.py` OK; `render_site.py --check` link audit OK;
  `cargo doc -p wyrd-core` reports nothing in `multipart.rs` (other modules' existing rustdoc
  warnings are not gated and not mine).
- **Not run by me:** `engine/scripts/run-verify.sh` (C4-verify) and `run-diff-cov.sh`
  (C4-diff-cov). Both work in shared `../wyrd-verify` / `../wyrd-cov` worktrees scoped by
  `$PDCA_LANE`, which is unset in my environment, so a run could have trampled another lane's
  Check. I did the red leg by hand instead (above). Check runs both.
- No commit hooks are configured in the target (`core.hooksPath` unset, no active hooks, no
  pre-commit config); the formatter is rustfmt, which is clean.

No external dependency was missing: `typos`, the docs renderer, `cargo-deny`, `cargo-machete`
and `cargo-mutants` all ran. No NEEDS-HUMAN external-dependency item.

## Self-review against the target's rubric (`AGENTS.md` § Review rubric & protocol)

- **One clock per lifecycle:** no clock read added. N/A.
- **Trait seams / dependency direction:** no trait or seam added. `sha2` was already a workspace
  dependency (`Cargo.toml:147`) used by `gateway-s3` and `server`; `cargo deny` and
  `cargo machete` are green.
- **Metadata validation boundaries (ADR-0045):** `Completion.etag` is validated at decode through
  `MultipartEtag`'s `Deserialize`; the count bound is the key space's format bound
  (`MAX_PART_NUMBER`), not a live knob, so strict at decode is right; a count above the
  `max_parts_per_session` knob but inside the format bound still decodes.
- **Shared mutable global state:** none (statics gate green).
- **`#![forbid(unsafe_code)]`:** the new test crate root has it
  (`crates/core/tests/multipart_state_machine.rs:35`); unsafe-guard green.
- **Docs currency:** this round's fix — the persisted `Completion.etag` form and the stored
  `complete_fingerprint`'s composition are now in the living doc
  (`05-building-block-view.md:204`), in the same PR.
- **Grammar strictness:** the ETag count is checked as canonical decimal (no sign, no leading
  zero, ASCII digits only) before any `from_str`, and the shared decimal rule was split rather
  than copied; the hex half goes through the existing `Digest::from_hex`.
- **Serialization identity:** `MultipartEtag` serializes as its one canonical text and parses only
  that; tested directly (serde round trip at every count boundary) and through
  `decode_session_record`'s canonical-bytes gate on a stored tombstone.
- **Absent or unsupported entries:** every refusal is a typed error or a typed `Refusal`; nothing
  is skipped or defaulted. The table test's cell count (`== 25`) sits beside per-cell value
  assertions, never instead of them.
- **Test fidelity:** pure functions, no DST model involved; no destructive or concurrent path.

## Considered and declined

- **A `Refusal` for an empty named-part list.** The digests refuse an empty list
  (`RecordError::NoPartsNamed`), but 0016 defines no protocol answer for one (no match for
  "EntityTooSmall", "MalformedXML", "at least one part" anywhere in it); for S3 it is a
  malformed request body, which #508's request parsing answers before the protocol sees it. I
  did not invent a variant 0016 does not name. If #658 ends up needing one, adding it breaks
  every exhaustive match at compile time, which is the designed way to extend this vocabulary.
- **A compile-time check that `Verb::ALL` is complete.** Not possible without a macro or a new
  dependency; see (c) above. The base's `RetireMode::ALL` has the same shape.

## Carry-forward items that are not mine to fix

- **C4 red→green (NEEDS-HUMAN):** pre-declared born-at-tier; C4-verify will again report
  UNVERIFIABLE (exit 77) because the test cannot compile without the symbols the patch adds.
  The eleven negations are the behavioural evidence.
- **C1 scope, Validation fitness-to-purpose, T5 prior-art (all NEEDS-HUMAN):** your calls at
  sign-off; the first is discussed under "Deviations".

## Leftovers

The worktree keeps the applied change (the harness owns it), with the new test file untracked as
the harness created it; `mutants.out/` there is git-ignored. My scratch is under
`$PDCA_SCRATCH`: `pdca-builder-693-negations` (the negation script, per-negation diffs and
logs), `pdca-builder-693-redleg` (base-production run logs and file copies),
`pdca-builder-693-ci` (the xtask ci and mutants logs), `pdca-builder-693-applycheck` (the clean
apply check) and `pdca-builder-693-docs-render` (the rendered site). All small; left for the
harness to reclaim.
