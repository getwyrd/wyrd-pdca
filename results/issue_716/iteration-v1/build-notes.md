# Build notes — issue #716 (multipart-session-lifecycle-records)

## What I read

`brief.md` only, plus the two narrow exceptions the brief itself licenses:

- The peer callsites named under **Citations expected**: `crates/core/src/erasure.rs:120`
  (`supported(k, m)`), `crates/core/src/metadata.rs:129-140` (`ChunkRef`'s shape),
  `metadata.rs:1195-1246` (`SegmentRecord`'s hand-written `Deserialize` + `from_wire`,
  `checked_chunk_bytes`), `metadata.rs:1376-1524` (`InodeRecord`'s `#[serde(try_from =
  ...)]` + `checked_shape` pattern), `metadata.rs:1564-1571` (`encode`/`decode`),
  `metadata.rs:2513-2547` (`decode_root_record`/`decode_segment_record`, the per-record
  attributed-decode pattern).
- ADR-0045 (`docs/design/adr/0045-metadata-validation-boundaries.md`), the brief's
  **Invariant to restore**.
- Proposal 0016 §1 (`:333-527`) and §2 (`:528-602`), the brief's cited sources for the
  record shapes and the state machine.
- `AGENTS.md`'s `## Review rubric & protocol` section (the target's standing rubric,
  the second narrow exception) — read and self-reviewed against below.
- The one archived salvage source the brief names:
  `results/issue_692/iteration-v2/patch.diff` and `results/issue_692/iteration-v2/check-review.md`
  (harness-root-relative, resolved as such — my cwd as a claude builder is the harness root).

I did **not** read prior multipart cycles beyond that one archived patch, the conformance
ruleset, or project context outside what the brief cites.

## What I built

`crates/core/src/multipart.rs` gains the seven landed types the brief's Scope pins —
`SessionRecord`, `SessionState`, `PublishTarget`, `Completion`, `SlotRecord`, `PartRecord`,
`PartSummary` — each decoding through the base's `metadata::encode`/`metadata::decode`
(no second generic codec), with structural validation inside `Deserialize`
(`#[serde(try_from = "...")]` + a wire struct for the three that have a cross-field
invariant, mirroring `Budget`/`AdmissionRecord`; a plain closed struct for the four that
don't). New `RecordError` variants: `PublishTargetKeyMismatch`, `PublishTargetEpochMismatch`,
`ChunkSchemeUnsupported`, `SlotLeaseAlreadyLapsed`, `PartLengthMismatch`,
`PartLengthOverflow`. Three new attributed decode wrappers — `decode_session_record`,
`decode_slot_record`, `decode_part_record` — for the same reason `decode_admission_record`
exists: `#[serde(try_from = ...)]` funnels a `TryFrom` error through serde's
`Error::custom`, stringifying it before a `downcast` could recover the variant, so a
caller that wants the typed `RecordError` back must decode through the wire struct
directly. `decode_part_summary` is a thin wrapper for the same typed-error reason, though
`PartSummary` has no `TryFrom` of its own.

`docs/design/architecture/05-building-block-view.md` § "The metadata model" gains one new
paragraph (leg D).

`crates/core/tests/multipart_session_records.rs` is new: 20 tests — a round trip per
landed type (covering every `SessionState` variant) plus the nine Falsifiability
demonstrations.

## Design decisions and why

**`SessionState` field placement — literal, not maximal.** Proposal `0016:350`'s table
states the extra fields land "on `Completing` also: `fenced_at_millis`, `segments_written`,
`publish_target`" and separately "on `Completed` the published `{inode, version, etag,
completed_at_millis}` and `complete_fingerprint`" — it does **not** say these persist onto
`Aborting`, nor that `publish_target`/the fence stamps survive onto `Completed`. I took the
literal reading: only `Completing` carries the fence stamps; `Aborting` carries nothing
extra; `Completed` carries only `completion`. This is a real design choice I had latitude
over — "Production reach: N/A by design... nothing on an existing path changes" — and it is
the one that makes leg 1j's normative example ("a `Completing`-only `fenced_at_millis` on an
`Open` session") a **type-level** impossibility for every other state too, not just `Open`.
The alternative (carrying the fence stamps through `Aborting`/`Completed` as well, to cover
the direct `Open→Aborting` reaper-fence transition the state diagram also shows) is a real
possibility for #656–#659's writer to pick later; nothing here forecloses it — a wider
`SessionState` shape is compatible with every leg and round-trip test in this file, since
they are all authored against the *decoder*, not against an assumption about which fields a
future writer stamps on which transition.

**Nested, not flattened, `SessionState`.** I first tried `#[serde(flatten)]` so the wire
JSON would read as one flat object (closer to `0016:350`'s "also… segments_written,
publish_target" wording). Serde refuses `deny_unknown_fields` combined with `flatten` (a
compile-time restriction, not a design taste) — and leg 1m demands `deny_unknown_fields` on
every landed type including `SessionRecord`. So `state` is a genuinely nested value:
`{"state": {"kind": "Completing", ...}}`. This is also what the Defect field's "each
validating inside its own `Deserialize`" reads most naturally as: `SessionState` decodes as
an independent unit (`session_state_round_trips_standalone` exercises it bare, with no
`SessionRecord` wrapping it), and `SessionRecord`'s own `TryFrom` does only the **cross**
check (publish_target vs the session's own parent/object/epoch) that a value nested inside
`SessionState` cannot see on its own.

**`Open`/`Aborting` are empty struct variants (`Open {}`), not unit variants — a real
correctness finding, verified before shipping, not assumed.** I first wrote them as plain
unit variants (`Open`, `Aborting`) under `#[serde(tag = "kind", deny_unknown_fields)]` on
the enum. A throwaway probe (`cargo run --example probe`, removed before the final patch)
showed `{"kind":"Open","fenced_at_millis":9}` decoded **successfully** — serde's generated
`Deserialize` for an internally-tagged *unit* variant does not consult the rest of the map
once the tag matches, so `deny_unknown_fields` at the enum level has no effect on a unit
variant. Confirmed against this workspace's exact serde version with a second minimal probe
(`enum X { Open {}, ... }` vs `enum X { Open, ... }` — the empty-struct-variant form
correctly rejects `unknown field "a"`, the unit form silently accepts it). This is exactly
the class of "locally reasonable but wrong" mistake the exercise's cost-transparency rule
warns about — the cost of getting it wrong here is leg 1j's forbidden-field demonstration
being **vacuously green** (passing whether or not the check exists), which is precisely
what the binding refutation protocol below exists to catch. I did catch it, before it
shipped, by probing rather than assuming.

**Digest reuse for `etag`/`complete_fingerprint`/part `digest`.** The module already has a
validated `Digest` type (64-lowercase-hex, ADR-0047 — never MD5) for exactly this SHA-256
change-token shape. Reusing it rather than a raw `String` costs nothing (it's already in
scope) and gets ADR-0047's shape validation "for free" on every one of these fields — no
new type, so it doesn't count against the pinned seven.

**No writer-side constructor for `SessionRecord`/`SlotRecord`/`PartRecord`.** Mirrors
`Budget`/`AdmissionRecord`'s own choice in this same module, for the same reason: "the
first writers are the store round trips (#656–#659)," so a constructor here could not be
relied on to hold the identities `decode_*` enforces. `SessionState`, `PublishTarget`,
`Completion`, `PartSummary` have plain `pub` fields (no invariant of their own to bypass,
matching `ChunkRef`'s precedent); the three with a cross-field invariant keep private fields
+ getters (matching `Budget`/`AdmissionRecord`'s precedent), so there is no struct-literal
path that skips `TryFrom`.

**"Logical `len` must be non-zero" is NOT reinstated.** The brief withdrew this explicitly
(plan-review finding) as an invention with no source; ADR-0045's own invariant-table row for
`ChunkRef` says "logical length consistent with the scheme," and the brief assigns that
consistency check to leg 1k — `PartRecord.len` vs. the **summed** chunk lengths — not to a
per-`ChunkRef` non-zero bound. `checked_chunk_len`/`PartLengthMismatch` is that leg;
`checked_chunk_scheme`/`ChunkSchemeUnsupported` is leg 1i's `ChunkRef` half (scheme validity
only). Neither checks a chunk's `len` against zero.

**Placement length is deliberately never checked** in `checked_chunk_scheme` or anywhere in
`PartRecord`'s `TryFrom` — ADR-0045's own table says placement length is the standing
*contextual* check, liberal on read, and the brief's positive leg (leg 1i, the "still
decodes" case) is exactly this. Verified live under `AGENTS.md:146-149` and `0016:416-429`.

**Leg D — docs currency, a base discrepancy caught, not assumed.** The brief states "#715
lands beneath this child and already adds a multipart paragraph to
`docs/design/architecture/05-building-block-view.md` § 'The metadata model'." I read
`05-building-block-view.md` on the actual merged base (`git log -- <path>`; the last commit
touching it is `18180a2`, well before `5eeca16` which is #715's own commit and touches only
`crates/core/src/multipart.rs`) and found **no** multipart paragraph there at all — the
brief's premise does not hold on this base. Per the brief's own instruction ("Read what
#715 actually landed on the merged base before writing — do not assume this brief's wording
of it"), I added one paragraph documenting the full `mpu:`/`slot:`/`part:`/`psum:` +
`mpuctl` namespace (not just "extending" a paragraph that does not exist), since the
underlying `AGENTS.md:154-157` policy ("a change that ... alters a persisted field updates
the living architecture doc in the same PR") binds regardless of what #715 did or did not
do, and this child alone introduces four new persisted record shapes. Kept to one paragraph,
2 added lines (well under the ≈15-line docs budget). Verified: `typos
docs/design/architecture/05-building-block-view.md` (exit 0) and `python3
docs/publishing/tools/render_site.py --check` (`render_site: link audit OK`).

## Alternatives considered and ruled out

- **`SessionState` as `#[serde(flatten)]`** — ruled out on a compile error (serde forbids
  `flatten` + `deny_unknown_fields`), not a style preference. Cost if forced: none, it does
  not compile at all, so there is no diff to compare.
- **A shared `CompletingFence { fenced_at_millis, segments_written, publish_target }` struct**
  reused across `Completing`/`Aborting`/`Completed` — ruled out because it would be an
  **eighth** public landed type where the brief pins "exactly these SEVEN... corrected
  2026-08-09 from 'five'... a fixed review surface." Cost: the alternative saves roughly
  15–20 lines of field repetition (there is only one variant that needs the three fields, so
  the "repetition" is in fact zero — the struct would be pure overhead, not a savings).
- **Reusing `crate::metadata::ChunkMapError`'s variants instead of new `RecordError`
  ones** — ruled out because the brief's Scope explicitly assigns "the typed error variants
  those rejections need" to `RecordError` (this module's own error type, which #715 already
  widened for this purpose), and `ChunkMapError` is `metadata.rs`'s own type for a different
  record family (`InodeRecord`/`SegmentRecord`), not this module's.
- **Unit-variant `Open`/`Aborting`** — ruled out on the probed serde behavior above (a
  correctness finding, not a style call): kept as empty struct variants at essentially zero
  cost (`{}` after the variant name; JSON shape unchanged, since a variant with zero fields
  serializes the same as a unit variant under an internally-tagged enum).

## Falsifiability — the nine demonstrations (brief's binding requirement)

RED is criterion-ABSENCE, born-at-tier (posture (a), pre-declared, matching child-1): with
production reverted, `cargo test -p wyrd-core --test multipart_session_records` fails to
**compile** (confirmed: 7 `E0432`/`E0599` errors — the test imports `SessionState`,
`PartRecord`, `Completion`, etc. that do not exist on `origin/main`). That is the expected,
pre-declared UNVERIFIABLE-red leg; the binding evidence is the nine isolating
demonstrations below — each: drop exactly one check, run the one corresponding test, paste
the failing output, revert (verified back to the saved-good copy + a full green re-run
after every single one).

1. **1c** (`PublishTargetKeyMismatch`) — removed the parent/name comparison in
   `SessionRecord::try_from`:
   ```
   test leg_1c_publish_target_key_mismatch_is_rejected ... FAILED
   assertion `left == right` failed
     left: Ok(SessionRecord { ... state: Completing { ... publish_target: PublishTarget { parent: 43, name: "key/one", epoch: 3 } } })
    right: Err(PublishTargetKeyMismatch { session_parent: 42, session_object: "key/one", target_parent: 43, target_name: "key/one" })
   ```
2. **1c-epoch** (`PublishTargetEpochMismatch`) — removed the epoch comparison:
   ```
   test leg_1c_epoch_publish_target_epoch_mismatch_is_rejected ... FAILED
     left: Ok(SessionRecord { ... publish_target: PublishTarget { parent: 42, name: "key/one", epoch: 4 } } })
    right: Err(PublishTargetEpochMismatch { session_epoch: 3, target_epoch: 4 })
   ```
3. **1i-ChunkRef-scheme** (`ChunkSchemeUnsupported`) — neutered `checked_chunk_scheme` to
   always `Ok(())` (kept `EcScheme`/`erasure` referenced so the negation isolates the rule,
   not an unrelated unused-import error under this crate's `-D warnings`):
   ```
   test leg_1i_chunk_scheme_unsupported_is_rejected ... FAILED
     left: Ok(PartRecord { chunks: [ChunkRef { id: 7, scheme: ReedSolomon { k: 0, m: 1 }, ... }], ... })
    right: Err(ChunkSchemeUnsupported { chunk_id: 7, k: 0, m: 1 })
   ```
4. **1i-slot** (`SlotLeaseAlreadyLapsed`) — removed the lease-vs-reservation comparison in
   `SlotRecord::try_from`:
   ```
   test leg_1i_slot_lease_already_lapsed_is_rejected ... FAILED
     left: Ok(SlotRecord { ..., reserved_at_millis: 1000, lease_expiry_millis: 1000 })
    right: Err(SlotLeaseAlreadyLapsed { reserved_at_millis: 1000, lease_expiry_millis: 1000 })
   ```
5. **1j-forbidden-field** — dropped `deny_unknown_fields` from the `SessionState` enum:
   ```
   test leg_1j_open_session_with_forbidden_completing_field_is_rejected ... FAILED
   expected MalformedRecordValue, got Ok(SessionRecord { ... state: Open })
   ```
6. **1j-missing-required** — added `#[serde(default)]` to `Completing`'s `segments_written`:
   ```
   test leg_1j_completing_session_missing_required_field_is_rejected ... FAILED
   expected MalformedRecordValue, got Ok(SessionRecord { ... state: Completing { fenced_at_millis: 1, segments_written: 0, ... } })
   ```
7. **1k-len-mismatch** (`PartLengthMismatch`) — dropped the `total != wire.len` comparison in
   `PartRecord::try_from` (kept `checked_chunk_len`'s call, `let _total = ...`, so this
   negation isolates the *comparison*, not the overflow-checked summation leg 1k also pins):
   ```
   test leg_1k_part_length_mismatch_is_rejected ... FAILED
     left: Ok(PartRecord { chunks: [...], len: 50, ... })
    right: Err(PartLengthMismatch { declared: 50, chunks: 100 })
   ```
8. **1m-unknown-field** — dropped `deny_unknown_fields` from `SlotRecordWire` (a *different*
   record class than leg 1j's `SessionState`, so the two negations cannot be satisfied by one
   shared code path):
   ```
   test leg_1m_unknown_field_is_rejected ... FAILED
   expected MalformedRecordValue, got Ok(SlotRecord { part_number: PartNumber(1), attempt_id: AttemptId("cccc..."), reserved_at_millis: 1, lease_expiry_millis: 2 })
   ```
9. **Positive leg, negated the other way** — added a temporary over-strict placement-length
   check to `checked_chunk_scheme` (exactly the ADR-0045-forbidden rule: reject when
   `placement` is non-empty and its length disagrees with `fragment_count()`):
   ```
   test leg_1i_chunk_ref_wrong_placement_length_still_decodes ... FAILED
   ... still decodes (ADR-0045: placement length is contextual, not structural): ChunkSchemeUnsupported { chunk_id: 1, k: 0, m: 0 }
   ```

After each of the nine, the file was restored from a saved-good copy
(`/var/tmp/pdca/.../multipart.rs.good`, byte-compared equal after every restore) and
`cargo test -p wyrd-core --test multipart_session_records` re-confirmed **20 passed; 0
failed** before moving to the next negation. Final state: full green, `cargo clippy -p
wyrd-core --all-targets -- -D warnings` clean, `cargo fmt -p wyrd-core -- --check` clean,
and the rest of the crate (`cargo test -p wyrd-core --lib` and `--tests`) unaffected (43 +
every other test file's suite still green).

## The three refutation questions (forced, recorded)

**(a) Genuine red?** Yes. Reverting `crates/core/src/multipart.rs` to the merged base
(`git stash` of just that file) and running the named test produces 7 compile errors
(`E0432`/`E0599`: no `SessionState`/`PartRecord`/`Completion`/etc. in `multipart`, no
`PublishTargetKeyMismatch`/etc. variant) — the pre-declared UNVERIFIABLE red, matching
`Verification posture: born-at-tier (posture (a))`. Beyond that born-at-tier red, every one
of the nine Falsifiability demonstrations above is independently, freshly reproduced red
under its own isolating negation (not merely asserted) — see the pasted output above.

**(b) Production path?** Yes. Every test decodes through the actual production entry
points: `decode_session_record`/`decode_slot_record`/`decode_part_record`/
`decode_part_summary` (S2) and `metadata::decode::<T>` (S1) — the same store-wide codec the
real store round trips (#656–#659) will use — asserted to agree on every witness
(`decode_session_both`/`decode_slot_both`/`decode_part_both`). No mock, no stand-in, no
parallel reimplementation: the nine negations edit the **production** functions directly
(`SessionRecord::try_from`, `SlotRecord::try_from`, `PartRecord::try_from`,
`checked_chunk_scheme`, the `SessionState`/`SlotRecordWire` `deny_unknown_fields`
attributes) and prove the test notices.

**(c) Fixture includes the fault?** Yes. Every witness is a hand-authored JSON byte string
carrying exactly the torn shape the leg names — the wrong `publish_target.parent`, the
lapsed lease, the unsupported `(k, m)`, the forbidden/missing field, the length mismatch,
the extra field — decoded through the real production decoder, never curated to exclude the
faulty component. The positive leg's fixture (`placement: [1]` against a 3-fragment scheme)
likewise carries the exact malformed-length placement ADR-0045 says must still decode.

## Commit-readiness

`cargo fmt -p wyrd-core` applied (two small reflow diffs it made: one `write!` arm, one
import line, one function signature — all cosmetic, already reflected in `patch.diff`).
`cargo clippy -p wyrd-core --all-targets -- -D warnings` clean. Patch verified to `git apply
--check` cleanly against a stashed-clean copy of the exact base commit
(`6151063`/`5eeca16`) it targets.

## Scope discipline

Touches exactly 3 files (`crates/core/src/multipart.rs`, the new test, one docs paragraph) —
matches the brief's "a fourth changed file means the seam is wrong" constraint. Added
"semantic" (non-comment, non-blank) lines: module 343 + test 382 + docs 2 = 727, under the
≤770 budget. Out-of-scope items named in the brief (`Budget`/`AdmissionRecord` internals,
`OwnedEntry`/`StagedPlacement`, retirement types, `PendingEntry`, the outcome enums/answer
table, knob values, store round trips, every other `docs/` file) are untouched.

## Rubric self-review (AGENTS.md § "Review rubric & protocol")

- **Metadata validation boundaries** (hard convention): every structural invariant is
  validated at decode via `Deserialize`/`TryFrom`, surfacing as a typed `RecordError`, never
  admitted as a value; the one contextual check named (`ChunkRef.placement` length) is
  deliberately left liberal-on-read, matching the rule verbatim.
- **Docs currency** (hard convention, "in the same PR"): addressed above (leg D).
- **Serialization identity** (recurring defect class): every landed type's round-trip test
  asserts `metadata::encode(&record).as_ref() == bytes` (byte-identical re-encode), not just
  value equality, for the three `TryFrom`-validated types plus a value-equality check for the
  four plain ones.
- **Absent or unsupported entries** (recurring defect class): every rejection returns an
  explicit typed `RecordError` — no silent default, no silent skip, no count-based assertion.
- **One clock per correctness lifecycle**: not applicable — this slice adds no clock read
  (timestamps are plain `u64` millis fields on records with no writer yet; #656–#659 owns
  which clock stamps them).
- **`#![forbid(unsafe_code)]`**: the crate root already carries it; the new test file states
  its own copy too (mirroring `multipart_budget_admission.rs`).
- No DST-reachable shared mutable global state, no new crate, no narrow-trait-seam surface
  touched — this slice is pure functions over `Vec<u8>`, consistent with the module's
  existing framing.
