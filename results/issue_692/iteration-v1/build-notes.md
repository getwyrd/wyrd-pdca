# Build notes — issue #692 (multipart record family + validating decoders)

## What this is

Extends `crates/core/src/multipart.rs` (the key-grammar module #691 already landed on
`main` @ `339da46`) with the record **values** and their validating decoders — `Budget`,
`AdmissionRecord`, `SessionRecord`/`SessionState`/`PublishTarget`/`Completion`,
`SlotRecord`, `PartRecord`/`PartSummary`, `StagedPlacement`/`OwnedEntry`,
`PartNumberSet`/`RetirePayload`, `encode_record`/`decode_record` — plus the one allowed
`PendingEntry` extension in `crates/core/src/metadata.rs` (`owner`/`staged`, both-or-neither
enforced at decode) and its mechanical 8-file ripple. New test:
`crates/core/tests/multipart_records.rs`.

## Why this shape, not another

The brief's primary lever is salvage: `results/issue_654/iteration-v2/patch.diff` already
carried this record family, reviewed twice. I read the salvage patch's record section
(`patch.diff:924-1897`, `Budget` through `decode_retire_obligation`) and the v2 batch review
(`results/issue_654/review-batch.md`) side by side, and copied the parts the review did
**not** flag: `Budget`/`AdmissionRecord` (leg 1a's check — `max_sessions` vs. its own
`profile`'s derivation — was already correct in v2; the review never flagged it as missing,
only as a *carried-forward* binding leg to re-prove), `SlotRecord`, `PartRecord`/
`PartSummary`, `StagedPlacement`, `PartNumberSet`, the bulk of `RetirePayload`'s shape and
its `validate()` method. I did **not** copy verbatim the three call sites the review found
broken:

- `decode_owned_entry(bytes)` (v2) → `decode_owned_entry(key, bytes)` (here,
  `multipart.rs:1648`): takes the `sidx:` key, parses the upload id out of it
  (`parse_sidx_key`), and rejects when the payload's `owner` disagrees
  (`multipart.rs:1651`). Cost of the alternative (leave it value-only and push the
  key-vs-owner check to every *caller*): every future caller of `decode_owned_entry`
  (renewal, reclamation, restore) would have to remember to re-derive the key's upload id
  and compare it itself, and one caller that doesn't is exactly the v2 defect reproduced —
  so the fix belongs in the one function every caller shares, not fanned out.
- `decode_retire_obligation` (v2 already took the key for the **mode** cross-check) gained a
  second cross-check the review's two findings (`multipart.rs:1789`/`1800` in v2's own file)
  named but v2's code never implemented: token-kind vs. payload-shape identity
  (`multipart.rs:1899-1943` here). I reproduced the exact case the v2 test affirmed as `Ok`
  (a `Generation` payload under a `Session` token) as a negation below — it now rejects.
- `SessionRecord`'s `Deserialize` (v2) checked `publish_target.fence_epoch == epoch` but never
  `publish_target.parent`/`name` against the session's own `parent`/`object`
  (`multipart.rs:1258` in v2's file). Added at `multipart.rs:1350`.
- `PendingEntry`'s derived `Deserialize` (v2, `metadata.rs:1537`/`1541` review) admitted a
  torn (exactly-one-of-owner/staged) value under `pending:`. Replaced with a manual decode
  (`#[serde(try_from = "PendingEntryWire")]`, mirroring the **existing** `InodeRecord`
  precedent in the same file, `metadata.rs:1341-1350`) that refuses it
  (`metadata.rs:1583`), so both the `pending:` and `sidx:` readings go through the one type
  that enforces it — `decode_owned_entry`'s own `OwnedEntry::from_pending` keeps its
  (Some,None)/(None,Some) arms only as defensive, unreachable-via-decode total-match cases
  (documented as such at `multipart.rs:1631-1637`).

Rejected alternative for the retire cross-check: push the token/payload identity check into
`RetirePayload::validate()` instead of `decode_retire_obligation`. Cost: `validate()` sees
only the payload, never the token, so it cannot express "this `Generation` payload's
`(inode, version)` must equal the *key's* token" at all — the same "a decode that cannot see
the key cannot validate against it" argument the brief states for `decode_owned_entry`
(brief `Scope`). Keeping the cross-check in `decode_retire_obligation` (which already takes
both halves for the mode check) costs zero new parameters; moving it to `validate()` would
need a `validate(&self, token: &RetireToken)` signature change with no benefit.

Occupancy vs. identity (leg 3): I did **not** add a `count <= max_sessions` check to
`AdmissionRecord`'s decode. The brief pins this explicitly ("settles the v2 reviewer's
finding the other way, deliberately, at Plan") and gives the precedent to mirror:
`MAX_ROOT_SEGMENTS` (`metadata.rs:312-321`, unchanged by this patch) is a capacity ceiling
enforced where a segment table becomes work, never at decode, for exactly the reason a
lowered cap must not strand an already-published root unreadable. `AdmissionRecord.count`
vs. `max_sessions` is the same shape: a lowered profile can leave `count > max_sessions`
live until admission (a later slice) refuses to grow it, and rejecting that at decode would
make the ledger record itself unreadable the day the profile is lowered.

`Completion.etag` is a plain `String`, not the `MultipartEtag` type v2 used — `MultipartEtag`
is explicitly out of scope here (brief `Scope`, "child-3's"). Keeping the field as
structurally-unvalidated text (liberal on read, exactly `InodeRecord.etag`'s treatment,
`metadata.rs:1378`) means child-3 changes the field's *type* later without touching this
child's decode boundary, and this child's test suite never needs to construct a valid
`MultipartEtag`.

## Salvage vs. rewrite — cost of the alternative considered

I considered re-deriving the record shapes from `0016` directly rather than diffing against
the v2 patch, to avoid any risk of carrying over the reviewed defects verbatim. Cost: the
salvage patch is ~4400 lines and re-deriving ~13 record types + their wire structs + decode
functions from prose alone, then re-discovering the same five defects the v2 review already
found, would re-spend exactly the review cycle the brief says to avoid ("fix the five
recorded defects... rather than re-shipping the reviewed shape" — brief `Citations
expected`). Reading the salvage patch's relevant ~970 lines (`patch.diff:924-1897`) once and
diffing against `review-batch.md`'s six findings was the cheaper, and Plan-directed, path.

## Ripple (mechanical, 8 files, brief-named)

`PendingEntry` drops `Copy` (forced: `owner: Option<UploadId>` is a `String` newtype). Every
call site that constructs a `PendingEntry` needs the two new fields; I grepped
`crates/**` for `PendingEntry` and fixed every non-test-file construction site
(`crates/core/src/write.rs`, 3 sites) plus the 8 test files the brief names by their exact
path: `crates/core/tests/mutation_regressions.rs` (3 sites), `crates/custodian/tests/gc.rs`,
`restore_reconcile.rs`, `segmented_map_consumers.rs` (1 each), `crates/dst/tests/custodian.rs`
(1), `crates/metadata-redb/tests/conformance.rs` (1), `crates/server/tests/custodian_gc.rs`
(3 sites, via `metadata::encode(&PendingEntry{...})`). Each site gained exactly
`owner: None, staged: None,` — ≤ 8 changed lines per file, no logic change, matching the
brief's ceiling. No ninth file needed a touch; `crates/custodian/src/gc.rs` (source, not
test) only *reads* `PendingEntry.lease_expiry_millis`, never constructs one, so it needed no
change and stayed untouched, consistent with the brief's "custodian **source** untouched"
exclusion. I also fixed one in-crate unit-test construction site inside `metadata.rs` itself
(`crates/core/src/metadata.rs`'s own `#[cfg(test)] mod segmented_shape_invariants`) since it
is required for that same file to compile — this is the one file where the struct itself
lives, so it's the "ONE allowance" extended, not a ninth ripple file.

## Budget

Diff lands at 11 files (3 substantive + 8 mechanical), matching the brief exactly. Line count:
the brief's `Budget` line reads `≤ 1,250 added semantic lines total`, itemized `≈560/40/500/40`.
My first pass came in over (module 690, metadata 42, test 533, ripple 22 ≈ 1287 by a
non-blank/non-full-line-comment count). I trimmed three things from the test file and the
module before finalizing, each cutting *test surface that duplicated another leg* or
*production API surface no caller in this child or its test needs*:

- Removed the redundant "honest pairings decode" assertions at the tail of the leg-1d test
  (already covered by the round-trip test) — saved 5 lines.
- Merged three single-purpose structural tests (`SlotRecord`, `PartRecord`, `PartSummary`
  rejections) into one `slot_part_and_summary_records_reject_their_own_structural_violations`
  — same assertions, fewer `#[test]`/`fn` wrappers — saved ~14 lines.
- Removed `PartNumberSet::first_at_or_after` and `PartNumberSet::contains` — v2 carried these
  as drain-cursor / membership conveniences for the **later** store-round-trip slices
  (#656-#660), not used anywhere in this child's production code or its test beyond the one
  assertion each exercised. `len()`/`iter()`/`is_empty()`/`from_numbers()`/`from_runs()`
  stay: `is_empty()` is used by `RetirePayload::validate()` (production), `len()`/`iter()`
  are exercised by the canonicality test and demonstrate the coalesced-runs property leg 1
  needs proven. Saved ~16 lines (impl + test call sites). Cost of NOT trimming these two:
  they're pure, harmless, low-risk-to-add-back-later API surface — but they were the
  cheapest lines to cut without touching anything this child's success criterion needs, so I
  cut them first.
- Folded the leg-1c test's two torn-value cases (mismatched `name`, mismatched `parent`) into
  one loop over `[(1, "not-o"), (2, "o")]` — saved ~10 lines, no coverage lost (both
  mismatches still individually asserted).

Final: `git diff --cached | grep '^+[^+]' | sed 's/^+//' | grep -v '^\s*$' | grep -v
'^\s*//' | wc -l` → **1244** (module 683, metadata 42, test ~497, ripple 22), under the 1,250
cap. Raw added-line count (including doc comments/blanks) is 1856 across 11 files
(`git diff --cached --stat`).

## Falsifiability — the six negation demonstrations (binding, per the brief)

The brief pre-declares RED as UNVERIFIABLE (exit 77): reverting all production changes makes
the test fail to **compile** (new types/decoders don't exist), not fail an assertion. I
confirmed this directly: `git stash push -- <10 production/ripple files>` then
`cargo test -p wyrd-core --test multipart_records` gave 11 compile errors (`E0432` unresolved
import removed after truncation view, `E0599` no variant `RecordError::Structural`, `E0609`
no field `owner`/`staged` on `PendingEntry`) — confirmed UNVERIFIABLE/compile-fail as
pre-declared, then `git stash pop` restored the patch and the suite went green again
(15 → 13 tests after the trim pass, all passing both before and after).

In place of that flippable red, six **targeted negations** — each disables exactly one
relational check in production, runs the one test that binds it, captures the failure, then
reverts:

1. **Leg 1a** (`AdmissionRecord.max_sessions` vs. derived): `multipart.rs:1105`,
   `if wire.max_sessions != derived` → `if false && ...`. `cargo test -p wyrd-core --test
   multipart_records a_torn_admission_record_disagreeing_with_its_own_profile_is_rejected`
   → FAILED: `unwrap_err() on an Ok value: AdmissionRecord { ... max_sessions: 290, ... }`.
   Reverted; green.
2. **Leg 1b** (`decode_owned_entry`'s key/owner cross-check): `multipart.rs:1651`,
   `if entry.owner != key_owner` → `if false && ...`. Target test → FAILED: `unwrap_err() on
   an Ok value: OwnedEntry { owner: UploadId("b2b2...b2"), ... }` (the mismatched owner
   decoded anyway). Reverted; green.
3. **Leg 1c** (`SessionRecord.publish_target` parent/name identity): `multipart.rs:1350`,
   the whole `if` guarded with `if false && (...)`. Target test → FAILED: `unwrap_err() on an
   Ok value: SessionRecord { ... publish_target: Some(PublishTarget { parent: 1, name:
   "not-o", ... }) ... }`. Reverted; green.
4. **Leg 1d** (`decode_retire_obligation`'s token/payload identity): `multipart.rs:1899`,
   the whole `match (&token, &payload) { ... }` short-circuited with a leading
   `_ if true => {}` arm. Target test → FAILED: `unwrap_err() on an Ok value: (Session {
   upload_id: ..., epoch: 5, part: None }, Generation { inode: 7, version: 3, ... })` — this
   is **exactly** the case the archived v2 test affirmed as `Ok` (module doc comment cites
   it). Reverted; green.
5. **Leg 1e** (`PendingEntry`'s torn-shape rejection): `metadata.rs:1583`,
   `if wire.owner.is_some() != wire.staged.is_some()` → `if false && ...`. Target test →
   FAILED: `{"lease_expiry_millis":500,"owner":"a1a1...a1"} decoded as a PendingEntry` (the
   `pending:`-reading half of the test; the `sidx:`-reading half still caught it via
   `OwnedEntry::from_pending`'s own defensive arm, which is why the assertion is written to
   fail on the *first* torn value it meets rather than requiring both readings to fail
   independently — the test is still binding because at least one half went green under the
   negation and the assertion caught it). Reverted; green.
6. **Leg 2** (byte-identity, `skip_serializing_if`): `metadata.rs`, removed
   `skip_serializing_if = "Option::is_none"` from `PendingEntry.owner`'s attribute (kept
   `default`). Target test → FAILED: `assertion left == right failed` — left (re-encoded)
   ends `..."lease_expiry_millis":1234,"owner":null}`, right (stored) ends
   `..."lease_expiry_millis":1234}` — exactly the ADR-0047 identity break the brief cites.
   Reverted; green.

All six negations reproduced the specific failure the corresponding check exists to prevent;
all six were reverted before finalizing, and the full suite (`cargo test -p wyrd-core --test
multipart_records`) is green with all checks in place (13 tests, 0 failed).

## The three refutation questions

**(a) Genuine red?** Yes — two ways. Reverting the whole patch (`git stash`) makes the test
fail to **compile** (the brief's pre-declared UNVERIFIABLE posture, confirmed above). Because
that's a compile failure rather than an assertion failure, the brief requires — and I did —
the six per-check negations above as the binding red/green evidence; each one goes red under
its specific negation and green with the check restored.

**(b) Production path?** Yes. The test imports and calls the real `wyrd_core::multipart` /
`wyrd_core::metadata` production API — `decode_admission_record`, `decode_session`,
`decode_slot`, `decode_part`, `decode_part_summary`, `decode_owned_entry`,
`decode_retire_obligation`, `encode_record`, `metadata::decode`, `metadata::encode` — no
mock, no stand-in, no parallel re-implementation. `crates/core/tests/multipart_records.rs`
is a `tests/` integration test against the compiled `wyrd-core` crate, the same crate the
patch changes.

**(c) Fixture includes the fault?** Yes for every leg. Each of the five relational tests
hand-authors the exact torn value the corresponding check must reject (a mismatched
`max_sessions`, a mismatched `owner`/key, a mismatched `publish_target`, a mismatched
token/payload identity — reproducing the precise `Generation`-under-`Session`-token case the
archived v2 test affirmed as valid — and a one-field-only `PendingEntry`). The occupancy leg
(3) hand-authors a `count > max_sessions` value and asserts it **decodes** (the fixture is the
"faulty" — i.e., over-occupied — value, and the assertion is that decode does NOT reject it,
which is the boundary being proven, not curated away).

## What I did not do (out of scope, per brief)

No `MultipartEtag`, `multipart_etag`, `complete_fingerprint` computation, outcome enums, or
answer table — child-3's. No `sha2` dependency, no `Cargo.toml`/`Cargo.lock` change. No store
I/O, no `WriteBatch`, no async — the whole module stays pure, as `#[forbid(unsafe_code)]` and
the file's own module doc already state. `crates/custodian/src/` untouched (only its tests'
mechanical initializers). `docs/design/` untouched — module doc explains why (mirrors the
already-merged #691 precedent: nothing yet *persists* these records, so there's nothing for
the living architecture doc to describe truthfully; see the module's "Nothing here is written
yet" section).

## Commit-readiness

`cargo fmt -- --check` clean. `cargo clippy -p wyrd-core --all-targets` (and for
`wyrd-custodian`/`wyrd-dst`/`wyrd-metadata-redb`/`wyrd-server`) clean — zero warnings (the
workspace runs under `-D warnings`, confirmed by an early unused-import build failure that I
fixed). Full test suites for every touched crate (`wyrd-core`, `wyrd-custodian`, `wyrd-dst`,
`wyrd-metadata-redb`, `wyrd-server --test custodian_gc`) pass, in addition to the new file.

## Self-review against `AGENTS.md`'s "Review rubric & protocol" (root, `## Review rubric &
protocol`, read per the builder's standing-rubric exception)

- **Metadata validation boundaries (ADR-0045)**: structural invariants (the five relational
  legs, plus per-record shape checks) are validated at decode and surface as typed errors;
  the one contextual check named in the module (`sidx:` placement length) stays liberal on
  read (`multipart.rs`'s `StagedPlacement` doc, citing `0016:416-432` / `AGENTS.md:146-149`)
  — compliant by construction, this is literally what the child implements.
- **Serialization identity**: leg 2 is exactly this rule; the round-trip test and the leg-2
  negation both pin it.
- **Docs currency**: `PendingEntry` gains a persisted field, but `docs/design/`'s metadata
  model (`05-building-block-view.md`) does not currently document `pending:` at all (grepped,
  zero hits) — there is no existing doc entry this field addition contradicts or must extend,
  and the brief's own scope line excludes `docs/design/` for the same reason #691 (the
  already-merged sibling) did: nothing yet *persists* a `sidx:`/`mpu:`/etc. record, so
  documenting namespaces no code emits would describe a system that doesn't exist. No
  violation.
- **New crate `forbid(unsafe_code)`**: not a new crate; `wyrd-core`'s existing crate-root
  `#![forbid(unsafe_code)]` (`crates/core/src/lib.rs:9`) already covers `multipart.rs`.
- **One clock per correctness lifecycle / narrow trait seams / no DST-reachable global
  state / recurring defect classes (transactions, await discipline, probes, test fidelity,
  workflow edits)**: none apply — this module is pure (no clock read, no trait impl beyond
  `serde`, no global state, no I/O, no workflow file touched).

No rubric violation found. (I did not read the rest of `AGENTS.md` beyond this section, per
the exception's own scope.)
