# Build notes — #840 (809.2): every session record carries its segment-group nonce

Target: `getwyrd/wyrd @ main`. Worktree base `36f006d` (= `origin/main` at build time; the
brief cites `243241e`, and the one commit between them, `842f48e`, touches only
`crates/core/src/metadata.rs`, which moved the `SegmentNonce` lines by a few). Every `path:line`
below is on the **patched** tree unless it says "base".

## What changed

**Production (`crates/core/src/multipart.rs`)**, placement A as the brief settles it:

- `SessionRecordWire` gains `segment_nonce: SegmentNonce` between `clock_source` and `epoch`
  (`multipart.rs:2103-2104`). It decodes through the new `de_segment_nonce`
  (`multipart.rs:2082-2086`): `String::deserialize`, then `SegmentNonce::new`, each failure
  mapped to `DeError::custom("segment_nonce: …")`. That is the same route `SegmentGroup`'s own
  `Deserialize` takes (`metadata.rs:1060-1071`), and it mirrors `de_content_type`'s field-prefixed
  message (`multipart.rs:2063-2069` base). `SegmentNonce` still has no `Deserialize`.
- `SessionRecord` gains the same field in the same position (`multipart.rs:2179`). Because
  `Serialize` is derived and `SegmentNonce` serializes `transparent`, the encoder writes it
  right after `clock_source`. `decode_session_record`'s existing canonical-bytes check
  (`multipart.rs:2321`, `require_canonical`) therefore refuses it in any other position with
  no new code. `TryFrom` moves it across (`multipart.rs:2301`).
- Accessors: `segment_nonce() -> &SegmentNonce` in every state (`multipart.rs:2215`), and
  `attempt_segment_group() -> Option<SegmentGroup>` (`multipart.rs:2225`): `Some((nonce,
  publish_target.epoch))` only for `Completing`, `None` for `Open`/`Aborting`/`Completed`. The
  match lists every state, so a new state has to choose.
- Docs: field list and the new "# The segment-group nonce" section (`multipart.rs:2110-2125`);
  "Eight of the nine fields" (`multipart.rs:2135`); `PublishTarget`'s note (`multipart.rs:1946`)
  now says its epoch is the attempt's epoch within the session's group, not something that
  "makes the nonce deterministic".

**`crates/core/src/metadata.rs`**, only the constructor the brief allows:

- `SegmentGroup::from_nonce(SegmentNonce, u64) -> Self` (`metadata.rs:1044`), infallible, so
  `attempt_segment_group` never re-parses the nonce.

**Docs I changed beyond the brief's list, because they would be false otherwise** (one to three
lines each):

- `RecordError::PublishTargetEpochMismatch` (`multipart.rs:283-287`) had the same "makes the
  nonce deterministic" wording as `PublishTarget`.
- `SegmentNonce`'s doc said "the only decode path is `SegmentGroup`'s" (`metadata.rs:984`). Now
  it names both paths.
- `SegmentGroup::new` said it was "the **only** way to obtain a `SegmentGroup`"
  (`metadata.rs:1030`). Now it names `from_nonce` too.
- `leg_1c_epoch`'s doc comment in `multipart_session_records.rs:443` repeated the old wording.

**Architecture doc** (`docs/design/architecture/05-building-block-view.md:202`, the persisted-field
sentence, per "Docs currency"): the `mpu:<id>` parenthetical now names the nonce, why it is kept
in every state, and that 0016's `mpu:` row does not list it yet.

**Fixtures**: every full session record gains
`"segment_nonce":"0123456789abcdef0123456789abcdef"` after `clock_source`, one builder each:
`multipart_session_records.rs:99` (`session_with`), `:651`, `:717` (the two inline records);
`multipart_state_machine.rs:405`; `staged_protection.rs:594`; `staged_scrub.rs:430`;
`staged_repair.rs:418`; `staged_drain_status.rs:278`; `crates/dst/tests/custodian.rs:2763`
(`handoff_session`) and `:4535` (`replace_session`). Each is one line split into two to stay
under 100 columns. State-only and `publish_target` fixtures are untouched. `grep clock_source`
over the whole tree finds no other session record.

## Tests

- **New file, red→green:** `crates/core/tests/multipart_segment_nonce.rs`, 17 tests: legs (a),
  (b), (c), (d), each once per state (4 × 4, through a small `per_state!` macro so a red run
  names every state), plus `fixture_is_the_shared_wire_spelling`, which pins the brief's two
  example records byte for byte. It uses only base-visible symbols
  (`decode_session_record`, `metadata::{encode, decode, SEG_NONCE_HEX_LEN}`, `RecordError`,
  `SessionRecord`). (b)–(d) each open with (a)'s positive arm, as the brief asks. (c) also
  asserts the refusal names the hex rule (`not 32 lowercase hex`), so on the base it cannot
  pass on the `unknown field` message alone. (d) checks all seven wrong positions against
  `NoncanonicalRecordValue` and confirms the store-wide decode accepts the same bytes, so the
  refusal is the position and nothing else. It also covers the key repeated
  (`duplicate field`), and for `Completing` a copy inside `publish_target`, both beside the
  record's own nonce and instead of it (placement B).
- **Green-only, in `multipart_session_records.rs:841-900`:** three tests for the accessors.
  Every state exposes its nonce and mints `seggrp:<nonce>` from it. A `Completing` record at a
  non-default epoch gives `SegmentGroup::new(NONCE, fence)` and mints `seg:<nonce>:<fence>:`.
  Only `Completing` has an attempt group.

### Red→green, through the project's own gate script

`engine/scripts/run-verify.sh` (the C4-verify row), run with `PDCA_LANE=2`,
`PDCA_BUNDLE=results/issue_840`, base `origin/main`:

```
run-verify.sh: GREEN — cargo test -p wyrd-core --test multipart_segment_nonce (fix applied)
test result: ok. 17 passed; 0 failed
run-verify.sh: RED — ... (production reverted, test kept)
test result: FAILED. 0 passed; 17 failed
run-verify.sh: PASS — red without the fix, green with it (17 test(s) ran red).
```

**17 of 17 tests ran red, all by assertion** (the file compiles on the base). Each failed at
(a)'s positive arm with `unknown field segment_nonce, expected one of parent, object,
content_type, created_at_millis, clock_source, epoch, attempts, state`. That run used the
patch before one comment-only edit (a line-number citation in `de_segment_nonce`'s doc,
`995-1005` → `996-1006`), which changes no behaviour.

### Full gate

`./engine/xtask.sh ci` (= `cargo xtask ci`) on the final worktree: **all checks passed**, exit 0.
That covers `typos`, `render_site.py --check` (link audit OK), `cargo fmt --check`, clippy on the
workspace and on `wyrd-dst` under `--cfg madsim`, every test including the DST custodian file
with the two edited helpers, `cargo-deny` and `cargo-machete`. `cargo fmt --all -- --check` was
clean again after the last edit.

## Refuting my own test

- **(a) Genuine red? Yes.** `run-verify.sh` reverted the production change and kept the test:
  17/17 failed with the unknown-field error above, and 17/17 passed with the fix.
- **(b) Production path? Yes.** Every leg calls the production `decode_session_record`
  (`multipart.rs:2315`) and the production `metadata::encode`/`metadata::decode`. No copy, no
  mock. The accessor tests call the production `SessionRecord` methods and the production key
  helpers `seggrp_key` and `seg_range_prefix`.
- **(c) Fixture includes the fault? Yes.** The fault is "the record cannot carry or name its
  nonce". Every state is exercised, including the two the rejected placement B could not
  serve (`Open`, `Aborting`), and placement B's own shape (the nonce inside `publish_target`)
  is a refused witness in leg (d).

I did not run hand-made mutants (the rule is the project's runner only; the C5-mutants row runs
at Check). By reasoning: a decoder that swapped in a constant nonce fails (a)'s byte identity; a
record that declared the field elsewhere fails (a) and (d); a decoder that skipped the hex rule
cannot compile, because `SegmentNonce` has no other constructor. The rule itself is covered by
`metadata.rs`'s own unit tests.

## Alternatives I ruled out

- **Validate in `TryFrom` with a typed `RecordError` variant** (keep `segment_nonce: String` on
  the wire). This would give a typed error instead of `MalformedRecordValue`. Cost: a new public
  variant with a doc (~5 lines), a `Display` arm (~4 lines), and the check in `TryFrom` (~4
  lines), about 13 lines and new public API. I chose the serde route because it is exactly
  what the peer `SegmentGroup` decode does (`metadata.rs:1060-1071`), and the brief says "decodes
  through `SegmentNonce`'s validating constructor". Both routes refuse the same set. If the
  reviewer wants the typed variant, it is a small follow-up.
- **`impl Deserialize for SegmentNonce`** (a manual impl through `new`). This would open a
  general decode path for the type in `metadata.rs`, which the brief limits to the
  constructor. The type's doc also says on purpose that it has none. Rejected on scope.
- **`attempt_segment_group` via `SegmentGroup::new(self.segment_nonce.as_str(), epoch)`**. It
  re-parses the nonce and returns a `Result` the caller must unwrap, and the brief says neither
  accessor re-parses. `from_nonce` is 3 lines.
- **Returning `Option<&SegmentGroup>`**. That would need the group stored on the record, which is
  placement B again. The accessor builds it on demand (one `String` clone).

## Size

`patch.diff` is 44,873 bytes, under the 45 KB budget. About 10.7 KB of that is the
architecture doc, because the persisted-field paragraph at `05-building-block-view.md:202` is a
single ~5 KB line and the diff carries it twice. The added clause itself is ~390 characters. I
kept the edit in the `mpu:` parenthetical where it belongs rather than adding a separate line,
and trimmed the test file and doc comments to fit.

## Notes for downstream (child-3, child-4, #841–#843, #810)

- Accessor names: `SessionRecord::segment_nonce()` and `SessionRecord::attempt_segment_group()`.
  Constructor: `SegmentGroup::from_nonce`.
- Wire spelling as the brief fixes it. The fixture nonce used everywhere is
  `0123456789abcdef0123456789abcdef`.
- There are no stored `mpu:` records to migrate: the record has no production writer yet
  (#656–#659, #508), as the existing `SessionRecord` doc already states.

## External dependencies

`typos` (1.48.0) and `docs-renderer` (`markdown_it`, `yaml`) are both present, and both ran
inside `cargo xtask ci`. No NEEDS-HUMAN item.
