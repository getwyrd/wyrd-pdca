# Build notes — #717 `multipart-staging-retire`

Target branch `getwyrd/wyrd @ main`, built in `$PDCA_WORKTREE`
(`/home/eddie/wyrd/wyrd.pdca-wt`) at base **`c824243`** (`Merge pull request #725 from
getwyrd/enhancement/716-multipart-session-lifecycle-records`) — i.e. #715 **and** #716 are
merged into the base, as the brief's execution precondition requires. Every `path:line`
below is against that tree.

## 1. What landed, and where

| File | What |
|---|---|
| `crates/core/src/multipart.rs` | §9 the owned staging entry (`StagedPlacement`, `OwnedEntry`, `checked_ownership_pairing`, `decode_owned_entry`), §10 the retirement obligation (`PartNumberSet`, `RetirePayload`, `decode_retire_obligation`), 12 new `RecordError` variants + `Display` arms, 3 header-doc corrections |
| `crates/core/src/metadata.rs` | `PendingEntry` gains `owner`/`staged` + `PendingEntryWire` + `TryFrom` (`:1588`, `:1608`, `:1616`), and the in-file test-module construction site (`:3489`) |
| `crates/core/src/write.rs` | 3 mechanical `owner: None, staged: None` initializers (`:209`, `:437`, `:500`) |
| 7 test files in core/custodian/dst/metadata-redb/server | the same mechanical initializer, 2–6 lines each |
| `docs/design/architecture/05-building-block-view.md:204` | leg 4 — one paragraph extending #715/#716's |
| `crates/core/tests/multipart_staging_retire.rs` | **new**, 24 tests |

12 files, exactly the set the brief names (4 substantive + 8 mechanical). No ninth ripple
file appeared; the in-file `metadata.rs` site the brief pre-declared (`&PendingEntry {
lease_expiry_millis }`) is the only extra construction site, as it predicted.

## 2. The three shaping decisions

**(a) No envelope; the two decoders take the key.** `decode_owned_entry(key, bytes)` and
`decode_retire_obligation(key, value)` are their own entry points over
`metadata::encode`/`decode`, in the shape `decode_segment_record` (`metadata.rs:2603`) and
`decode_admission_record` (`multipart.rs:1678`) already use — one extra parameter aside.
Nothing dispatches. I did not build (and did not salvage) #715's third-attempt envelope.

**(b) Each rule has exactly one home, even where two seams apply it.** The salvage's
`PendingEntry` hunk and its `OwnedEntry::from_pending` both restated the both-or-neither
rule. Here it is `multipart::checked_ownership_pairing` (`multipart.rs:2411`), called by
`TryFrom<PendingEntryWire> for PendingEntry` (`metadata.rs:1616`), by
`OwnedEntry::from_pending` and by `decode_owned_entry`. Likewise `checked_staged_scheme`
(`multipart.rs:2484`) is called by `StagedPlacement::new` and by its `TryFrom` conversion, and
the nested-`ChunkRef` rule of leg 1p reuses #716's existing `checked_chunk_scheme`
(`multipart.rs:2128`) rather than a second copy. The test pins the seams to each other:
`decode_owned_both` decodes every `sidx:` witness through **both** `decode_owned_entry` (S2)
and `metadata::decode::<PendingEntry>` + `OwnedEntry::from_pending` (S1) and asserts they
agree, so a future divergence between the shared record and this module's wire mirror fails a
test rather than shipping.

**(c) One wire mirror, deliberately, and its cost.** `decode_owned_entry` reaches its own
`OwnedEntryWire` (3 fields, `multipart.rs:2515`) instead of decoding `PendingEntry` directly.
Cost: **9 lines** of duplicated *shape* (no duplicated rules — see (b)). Benefit: the
rejections arrive as `RecordError::TornOwnedEntry` / `NotAnOwnedEntry` /
`StagedSchemeUnsupported`, not as a `MalformedRecordValue` whose `detail` is a serde string —
the exact reason `decode_admission_record` reaches `AdmissionRecordWire` instead of decoding
`AdmissionRecord` (`multipart.rs:1671-1677`: serde's `Error::custom` funnel stringifies a
domain error before a `downcast` could recover it). The alternative I rejected — decoding the
shared `PendingEntry` and mapping every failure to `MalformedRecordValue { namespace: "sidx:",
detail }` — would have saved those 9 lines and cost leg 1i and leg 1e their typed variants,
leaving the test asserting on substrings of a serde message.

`OwnedEntryWire` is deliberately **open** (no `deny_unknown_fields`) while
`StagedPlacementWire` and `RetirePayloadWire` are closed: the shared record must keep decoding
`pending:` entries written before these fields existed, so closing the outer shape would make
S1 and S2 disagree. An unknown field on a `sidx:` value is refused one check later by the
canonical-bytes gate (`NoncanonicalRecordValue`), which `an_unknown_field_is_refused` asserts.

## 3. The defects the salvage carried, and what they are now

| Recorded defect (v2 review) | Fix here |
|---|---|
| the token **suffix ignored** (`multipart.rs:2024` in the v2 patched file — its `(RetireToken::Session { .. }, _) => {}` arm accepted **every** session-scoped payload) | `RetirePayload::checked_against_token` (`multipart.rs:2909`) matches the payload shape against `part.is_some()` in **four explicit arms**, one per case, so each of leg 1h's three demonstrations negates in isolation |
| `StagedPlacement` deriving an **unchecked** `EcScheme` (v2 `:1657`) | `StagedPlacement` is `#[serde(try_from = "StagedPlacementWire")]` over #716's closed `EcSchemeWire`, and `checked_staged_scheme` (`multipart.rs:2484`) applies `erasure::supported` (`erasure.rs:120`) — refused at the bare type, through the shared record, and through the key-taking decoder |
| decoders that could not see the key (v2 `:1555`/`:1789`/`:1800`) | both decoders take the key; the three key relations (owner, generation identity/scope, part scope) are typed variants |
| `structural(&'static str, String)` stringly errors throughout the salvage | 12 typed variants, each naming **one** rule, per the module's stated convention |

Also adapted to the base as it now is (the salvage predates #715/#716): `metadata::encode`/
`decode` instead of the salvage's `encode_record`/`decode_record`; `require_canonical`
(`multipart.rs:1706`) closing both new decoders, as every decoder landed since #725 does;
`RetirePayloadWire` reading its chunk lists through #716's `ChunkRefWire`.

One thing I found while writing it and would flag to a reviewer: `RetirePayload` **must** stay
externally tagged. An internally tagged enum (the shape `SessionState` uses) buffers content
through serde's `Content`, which cannot carry a 128-bit `ChunkId` — so `{"kind":"Chunks",…}`
would fail to deserialize any real chunk list. That is recorded on `RetirePayloadWire`'s doc
so nobody "harmonises" the two enums later.

## 4. Scope lines I did not cross

* **1e is a shape check, not a namespace check.** A both-present (owned-shaped) value under a
  `pending:` key still decodes; `torn_pending_entry_is_rejected_under_both_readings` asserts
  exactly that. Making it a rejection means teaching `renew_pending`/`live_lease_guards` to be
  key-aware — live readers, out of this slice. (The brief defers this and marks the owner
  provisional; nothing here assumes #657 owns it.)
* **Leg 1e's observable** is `metadata::decode::<PendingEntry>` returning `Err`, not an
  ADR-0045 `MetadataValidationError` out of the live readers. The test asserts the decode
  boundary and never the reader's error type.
* **Placement length stays contextual** (ADR-0045 `:69-74`, `0016:416-429`) — asserted by
  `owned_entry_with_length_mismatched_placement_decodes`, and I fixed my own first witness,
  which accidentally carried a short placement (`rs(2,1)` with 2 positions), so that the
  length-mismatch leg is the *only* witness in the file exercising a mismatch. Without that
  fix the leg-3 negation was not isolating (it failed 7 tests); with it, exactly 1.
* `crates/custodian/src/` untouched; no ADR/proposal/spec edited; no `Cargo.toml`/`Cargo.lock`
  change; the outcome enums, `multipart_etag`, knob values and store round trips are untouched.

## 5. The eleven (twelve) demonstrations — production negated, test run, reverted

Each negation removes **one** guard and nothing else, then `cargo test -p wyrd-core --test
multipart_staging_retire` runs. Every one failed **exactly one** test (`22 passed; 1 failed`),
which is what makes each leg load-bearing rather than incidentally green. Script + full logs:
`$PDCA_SCRATCH/pdca-builder-717-redleg/` (removed at the end of the run; the failure text
below is verbatim). The tree was restored after each and re-verified green.

1. **1b — owner vs key** (drop the `owner != key_owner` guard):
   `owned_entry_owner_must_agree_with_its_key` FAILED — `expected OwnedEntryOwnerMismatch, got
   Ok(OwnedEntry { owner: UploadId("1a1a…"), lease_expiry_millis: 9000, staged: … })`
2. **1d — generation identity** (drop the `(inode, version)` comparison):
   `retire_payload_scope_must_agree_with_its_token` FAILED — `expected
   RetireGenerationIdentityMismatch, got Ok((Generation { inode: 42, version: 6 }, Generation {
   inode: 42, version: 5, … }))`
3. **1d — scope, the other half** (make the two cross-scope arms `Ok`): same test FAILED —
   `expected RetireTokenScopeMismatch, got Ok((Session { … part: None }, Generation { … }))`
4. **1h — session/parts under a per-part token** (delete `(Self::Session {}, Some(_))`):
   `whole_session_obligations_are_rejected_under_a_per_part_token` FAILED — `expected
   RetireTokenSuffixMismatch for session, got Ok((Session { … part: Some((PartNumber(4),
   AttemptId("3c3c…"))) }, Session))`
5. **1h — chunks under a session-wide token** (delete `(Self::Chunks { .. }, None)`):
   `per_part_obligation_is_rejected_under_a_session_wide_token` FAILED — `expected
   RetireTokenSuffixMismatch, got Ok((Session { … part: None }, Chunks { … }))`
6. **1h — records under a per-part token** (delete `(Self::Records { .. }, Some(_))`):
   `records_obligation_is_rejected_under_a_per_part_token` FAILED — `expected
   RetireTokenSuffixMismatch, got Ok((Session { … part: Some(…) }, Records { parts:
   Some(PartNumberSet([(3, 3)])), segments: None }))`
7. **1i — `StagedPlacement`'s `EcScheme`** (make `checked_staged_scheme` return `Ok`):
   `staged_placement_scheme_must_be_supported` FAILED — `assertion failed:
   metadata::decode::<StagedPlacement>(unsupported.as_bytes()).is_err()`
8. **1e — the torn pairing** (make `checked_ownership_pairing` return `Ok`):
   `torn_pending_entry_is_rejected_under_both_readings` FAILED — `the pending: reading must
   refuse the torn shape {"lease_expiry_millis":9000,"owner":"1a1a…"}`
9. **1n — a generation naming both sources** (drop the `RetireGenerationSourcesConflict`
   guard): `generation_obligation_names_exactly_one_source` FAILED — `assertion failed:
   matches!(decode_retire_obligation(&key, both.as_bytes()),
   Err(RecordError::RetireGenerationSourcesConflict { chunks: 1 }))`.
   **Which guard covers which branch:** *both* and *neither* are **separate** guards
   (`RetireGenerationSourcesConflict` and `RetireObligationOwesNothing { payload:
   "generation" }`), so the neither-branch does **not** ride this negation; it is asserted in
   the same test and additionally covered by the `an_obligation_that_owes_nothing_is_refused`
   witnesses.
10. **1p — nested `ChunkRef`** (drop the `checked_chunk_scheme` fold in both arms):
    `retire_payload_nested_chunk_scheme_must_be_supported` FAILED — `assertion failed:
    matches!(…, Err(RecordError::ChunkSchemeUnsupported { k: 0, m: 1, .. }))`
11. **Leg 2 — remove ONE `skip_serializing_if`** (on `PendingEntry::owner`):
    `legacy_pending_entry_re_encodes_byte_identically` FAILED — `left:
    "{\"lease_expiry_millis\":9000,\"owner\":null}"` vs `right:
    "{\"lease_expiry_millis\":9000}"`
12. **Leg-3 corollary negated the other way** (make a length-mismatched placement reject in
    `StagedPlacement::new`): `owned_entry_with_length_mismatched_placement_decodes` FAILED —
    `a length-mismatched placement is not a decode error: StagedSchemeUnsupported { k: 0, m: 0 }`

## 6. Forced self-refutation

* **(a) Genuine red?** **Yes** — twelve times over, §5. Each negation reverts *the fix* (the
  guard) and the corresponding leg goes red with the production value it should have refused
  printed in the failure. The whole-file RED (revert everything → the test fails to
  **compile**) is the brief's pre-declared UNVERIFIABLE, unchanged: the test names types that
  do not exist on the base.
* **(b) Production path?** **Yes.** Every witness goes through the production codec
  (`wyrd_core::metadata::encode`/`decode`), the production key builders (`sidx_key`,
  `retire_key` — never a hand-spelled key string), and the production decoders under test.
  There is no mock, no re-implementation, and no test-only copy of a rule; the two `_both`
  helpers exist only to run the *same* bytes through two production seams and compare.
* **(c) Fixture includes the fault?** **Yes.** The fault is hand-authored into the witness in
  every negation leg: the key names another session (leg 1b), the token names another
  generation or the wrong scope (1d), the token carries — or omits — the `:<part>:<attempt>`
  suffix (1h ×3), the stored scheme is `rs(0,1)` (1i, 1p), the value carries exactly one
  ownership field (1e), the generation names two sources (1n), the legacy value carries
  neither field (leg 2). Nothing is curated out: the positive control sits *beside* each
  negative (the same bytes under their honest key decode), which is what proves the rejection
  is about the key/value relation and not about the value alone.

## 7. Gates run here

* `cargo test -p wyrd-core --test multipart_staging_retire` → **24 passed**.
* `cargo fmt --all -- --check` → clean (the target's own commit hook runs rustfmt).
* `cargo clippy --workspace --all-targets` → clean (`-D warnings` via the workspace lints).
* `cargo test --workspace --no-run` → the whole workspace, including all 8 ripple files,
  compiles.
* `./engine/xtask.sh ci` (the project's gate runner — `cargo xtask ci` inside
  `$PDCA_WORKTREE`) → **`xtask ci: all checks passed`, exit 0**, run three times: once
  mid-build, once on the near-final tree, and once on the exact tree `patch.diff` was cut
  from. The prose gates ran for real on this host rather than warn-skipping (`typos`,
  `lint_docs: OK`, `render_site: … 98 page(s) … link audit OK`), so leg 4's paragraph is
  gate-verified, not merely written. `cargo deny`, `cargo-machete`, the conformance vectors
  (5 valid + 6 invalid), the ADR-0035 statics gate and the madsim DST leg all passed.

## 8. Budget, honestly

Physical lines added: **1 129 code** + 563 doc/comment + 88 blank = 1 780 diff lines across
12 files (production 536 code, test 591 code, docs 2), against the brief's ≈960 *semantic*
estimate. The overage is ~17 %
and it is concentrated in two places, both of which I would rather a reviewer see than have me
delete:

* the 12 typed `RecordError` variants and their `Display` arms — ~190 physical lines for what
  is arithmetically ~24 statements, and the direct cost of the module's "one variant names one
  rule" convention plus the three legs (1h-records, 1n, 1p) the plan review **added** after the
  original 420-line estimate was written;
* `assert!(matches!(…))` witnesses, which rustfmt renders across 4–6 lines each.

I did not trim tests to hit the number: every extra test in the file (mode mismatch,
owes-nothing, part-number canonicality, unknown-field closure, malformed key) defends a rule
this patch actually implements, and deleting one would leave landed code unexercised — the
worse trade.

## 9. What a reviewer should look at hardest

1. **The `OwnedEntryWire` mirror** (§2c). It is the one place where a *shape* is stated
   twice. The rules are not, and `decode_owned_both` asserts the two seams accept and reject
   the same bytes on every witness — but if a reviewer prefers the cheaper shape, the
   trade-off is explicitly "9 lines of duplicated shape" against "leg 1i and leg 1e lose their
   typed variants".
2. **The four-arm suffix rule** (`checked_against_token`, `multipart.rs:2909`). Written as
   four explicit arms rather than one table lookup *so that each of leg 1h's three
   demonstrations negates in isolation*; a table would have made one negation kill all three.
3. **`RetirePayload` must stay externally tagged** (serde's `Content` cannot carry a 128-bit
   `ChunkId`). Recorded on the wire type's doc.
4. **The two docs that were stale on the base and are now corrected** — `multipart.rs`'s
   module header claimed the remaining record values were "the next children's" (untrue since
   #716) and that the living architecture doc would gain these namespaces later (untrue since
   #715/#716 wrote that paragraph). Both were rewritten to describe the tree as it is; that is
   the only part of my diff that is not new material or a mechanical initializer.
5. **What I deliberately did NOT do**: no `pending:` reader was made key-aware (1e stays a
   shape rule), no custodian source was touched, no ADR/proposal/spec was edited, and no
   pre-existing `metadata.rs:NNNN` citation elsewhere in the repo was renumbered for my
   +67-line insertion — only the citations in the four files I own were refreshed (the repo
   already carries drifted ones by convention, e.g. `gateway_recover_totality.rs:11`'s
   explicit "pre-fix" markers).
