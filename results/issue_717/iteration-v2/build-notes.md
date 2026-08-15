# Build notes — #717 `multipart-staging-retire` (iteration 2)

Target branch `getwyrd/wyrd @ main`, built in `$PDCA_WORKTREE`
(`/home/eddie/wyrd/wyrd.pdca-wt`) at base **`c824243`** (`Merge pull request #725 from
getwyrd/enhancement/716-multipart-session-lifecycle-records`) — #715 **and** #716 merged, as
the brief's execution precondition requires. Every `path:line` below is against the patched
tree at that base.

**This iteration is not a rebuild.** Iteration 1's shape (two key-taking decoders, no
envelope, rules with one home each) was not what failed: the gate that failed was
`T4-batch-review` with **two blocking BUG findings**, and sign-off deferred four judgment
items. §1 is those, one by one; §2–§4 are the unchanged rationale; §5 is the (now fourteen)
negations, re-run against the final tree; §6 the forced self-refutation.

---

## 1. The carry-forward, item by item

### 1.1 BLOCKING #1 — `Records{parts: Some(empty), segments: Some(..)}` was rejected

> `crates/core/src/multipart.rs:2885` **BUG**: A `Records` payload with `parts: Some(empty)`
> and valid `segments` still owes segment cleanup, but this check rejects it as owing nothing
> and can strand that retirement obligation. (`review-batch.md`)

Confirmed by reading it: the arm applied the emptiness rule **per field** — `parts` present
and empty was `RetireObligationOwesNothing` regardless of `segments`. The finding's
consequence is the serious half: a `retire:records:` obligation is installed *atomically with
the batch that superseded those records* (`0016:356`, `:662-665`), so a value the drain cannot
decode is work that can neither be done nor cleared — the dangling `seg:` generation is owed
forever under a key nothing can read back. Refusing a payload is only safe when the payload
owes **nothing**; here it owed a generation.

**Fix** (`crates/core/src/multipart.rs:2894-2900`): judge emptiness over the payload as a
whole.

```rust
Self::Records { parts, segments } => {
    let owes_parts = parts.as_ref().is_some_and(|parts| !parts.is_empty());
    if owes_parts || segments.is_some() { Ok(()) } else { Err(owes_nothing()) }
}
```

The rule that survives is the one that was actually meant, and it is the rule the other arms
already implement: `Parts{[]}`, `Chunks{[]}`, `Records{}` and `Records{parts: []}` still
refuse (they owe nothing at all — `crates/core/tests/multipart_staging_retire.rs:713-734`),
and `Generation` has always been judged over both of its sources together. The doc bullet
(`multipart.rs:2858-2862`) and the `RetireObligationOwesNothing` variant doc
(`multipart.rs:376-387`) were corrected to say "as a whole, never per field".

Alternative considered and rejected — **normalise `Some(empty)` → `None` at decode**: it
would re-encode to bytes that are not the bytes read, so `require_canonical`
(`multipart.rs:1706`) would then reject the same value one line later as
`NoncanonicalRecordValue`. Same stranding, worse error. 0 lines saved, so it is not a cost
trade — it is simply wrong.

Regression test: `a_records_obligation_owing_only_segments_decodes`
(`crates/core/tests/multipart_staging_retire.rs:745`) decodes
`{"Records":{"parts":[],"segments":{…}}}` through the production key-taking decoder and
asserts byte-identical re-encode. Negation §5.13 restores the old two-`if` arm and this test —
and only this test — goes red.

### 1.2 BLOCKING #2 — `PendingEntryWire` accepted unknown fields

> `crates/core/src/metadata.rs:1594` **BUG**: `PendingEntryWire` accepts unknown fields, so
> live `pending:` readers can decode and renewal can silently drop those fields when
> re-encoding the CAS value, violating the required byte identity. (`review-batch.md`)

Verified against the code, and the mechanism is exactly this patch's own leg-2 argument turned
around. `renew_pending` (`crates/core/src/metadata.rs:2068-2091`) preconditions on the **raw
bytes it read** (`batch.require(key.clone(), current)`, `:2090`) and puts `encode(entry)` — a
freshly built entry. An unknown field in the stored value therefore does **not** make the CAS
fail: the precondition still matches the bytes, the commit succeeds, and the field is deleted
from a durable record with no error anywhere. That is the same silent-rewrite failure mode the
brief names for `"owner":null`, reached by the other door. (On the `inode:` path the identical
drop is *loud* — `require(key, encode(prior))` can never match again — which is why
`InodeRecordWire` can stay open and this one cannot.)

**Fix** (`crates/core/src/metadata.rs:1618`): `#[serde(deny_unknown_fields)]` on
`PendingEntryWire`, with the asymmetry recorded on `PendingEntry`'s own doc
(`:1584-1590`). `#[serde(default)]` on both new fields is untouched, so **missing** fields
still decode — every legacy record stays readable; only a field this build does not know is
now an error rather than a value.

I also closed `OwnedEntryWire` (`crates/core/src/multipart.rs:2518`), whose stated reason for
being open ("the shared record is open") no longer exists. An unknown field on a `sidx:` value
is now `MalformedRecordValue{namespace: "sidx:"}` (attributed to the field) instead of
`NoncanonicalRecordValue` (attributed to the spelling), which is what every other decoder in
the module does — `AdmissionRecordWire` (`multipart.rs:1525`), `SessionRecordWire`
(`:1846`), `PartRecordWire` (`:2221`), `ChunkRefWire` (`:2239`).

**Scope note for sign-off, stated plainly:** the brief's `metadata.rs` allowance is the two
fields, the dropped `Copy` and the torn-shape rejection. `deny_unknown_fields` is one more
line of strictness on a live-path record, inside that same hunk — a *behaviour* change for a
value nothing writes today (nothing in the tree or its history emits a `pending:` value with
an extra field; the conformance vectors carry none). It is the fix the gating review asked
for, it serves the brief's own leg 2, and the alternative was to record-reject a correct
finding. Flagging it rather than burying it.

Alternatives, with their cost:

* **Record-reject the finding** ("pre-existing on the base; `InodeRecord` has the same shape").
  0 lines. Rejected: the two paths are not equivalent (loud vs silent, above), and this is the
  patch that gives `pending:` values fields worth preserving.
* **Preserve unknown fields through the renewal instead.** Concretely: add
  `#[serde(flatten)] extra: serde_json::Map<String, Value>` to `PendingEntry` (+2 lines), then
  change `renew_pending` to decode each chunk's current value, graft the caller's expiry onto
  it and re-encode per chunk instead of `encode(entry)` once — `metadata.rs:2079-2090`, ~8
  changed lines, plus the same treatment in every caller that builds a `PendingEntry`
  (`write.rs:207`, `:435`, `:498`) because a single `&PendingEntry` applied to a slice of
  chunks can no longer carry per-chunk state. ≈15 production lines *and* it makes
  decode→encode identity depend on `serde_json` map ordering, and it changes live readers —
  which the brief rules out twice ("do not smuggle in changes to live readers"). Rejected on
  scope and on risk, not on line count alone.

Regression test: the `PendingEntry` half of `an_unknown_field_is_refused`
(`crates/core/tests/multipart_staging_retire.rs:807-810`) — `{"lease_expiry_millis":9000,
"future":1}` must not decode. Negation §5.14 removes the attribute; that assertion, and only
it, goes red.

### 1.3 T5 [impl] — the GC-quarantine claim (sign-off: "remove or substantiate")

Removed, in both places it appeared. `StagedPlacement`'s doc
(`crates/core/src/multipart.rs:2442-2452`) now says placement length is contextual and that
0016 puts its treatment in the maintenance passes that *will* read these entries
(`0016:416-429`), explicitly adding "Nothing reads a `sidx:` key in this tree yet, so that
treatment is the design's, not a property this module can claim today". The test's leg-3 doc
(`crates/core/tests/multipart_staging_retire.rs:462-474`) says the same and names what it can
observe — the decode boundary — citing `crates/custodian/src/gc.rs:482-496` as the reason it
cannot observe more. The brief itself withdrew the quarantine claim; iteration 1's prose had
not followed.

### 1.4 Adversary [impl] — the `skip_serializing_if` justification cited a line that refutes it

`renew_pending` cannot reach a `sidx:` key (it builds `pending_key(chunk)`, `metadata.rs:2079`)
and writes the **caller's** entry, not the decoded prior — so "an owned lease is renewed in
flight by re-encoding the entry it read" was wrong. Reworded at
`crates/core/src/metadata.rs:1573-1582` (the legacy half, which is correct, kept; the owned
half restated as "what a renewal stores is what was read", with the wiring attributed to #657)
and at `crates/core/src/multipart.rs:2617-2621` and the test's canonical-bytes doc
(`crates/core/tests/multipart_staging_retire.rs:857-861`). The brief's own warning — 0016's
`:475-485` paragraph mis-describes this code — is why none of that framing was re-imported.

### 1.5 Adversary [impl] — "the accepted sets are pinned equal by the S1/S2 helper" was false

It was: `decode_owned_both` runs on accepted witnesses only, and the seams differed by the
canonical-bytes gate. Rewritten at `crates/core/src/multipart.rs:2504-2517` to state what is
true — the *rules* are shared (one definition each, called from both seams), both shapes are
now closed, and the one remaining difference is that the `sidx:` decoder additionally demands
the canonical spelling. `a_foreign_spelling_of_an_accepted_value_is_refused`
(`crates/core/tests/multipart_staging_retire.rs:863`) is the witness for that difference.

### 1.6 Adversary [human] — decode-only enforcement (public constructible shapes)

Not closed by a type redesign (Rust enum variant fields cannot be made private, and
`PendingEntry`'s public literal is what the briefed 8-file ripple is made of — that is the
adversary's own reason for routing it to a human). Partially mitigated instead, at a cost of
one word plus a test: `RetirePayload::checked_shape` is now **public**
(`crates/core/src/multipart.rs:2870`, doc at `:2866-2869`), so a writer can refuse its own
malformed payload before making it durable — the shape `InodeRecord::checked_for_publication`
already has on the commit path. `a_writer_can_check_its_own_payload_before_storing_it`
(`crates/core/tests/multipart_staging_retire.rs:771`) exercises it. **Still open for the
human:** nothing *forces* a future writer to call it; the standing decision remains "decode is
the boundary", and #656–#659 are where the writers appear.

### 1.7 C2 / C4 (born-at-tier red; full-gate reproduction)

Unchanged in kind — the brief pre-declares the UNVERIFIABLE C4-verify — but better evidenced
this round: the full gate `./engine/xtask.sh ci` (`cargo xtask ci` in `$PDCA_WORKTREE`) ran to
completion **twice** — once on the near-final tree and once on the exact tree `patch.diff` was
cut from — both **`xtask ci: all checks passed`, exit 0**. No test stalled; the two
`cargo deny` advisory legs that could not lock the host database last round both ran
(`advisories ok`, twice); the prose gates ran for real (`typos`, `lint_docs: OK`,
`render_site: 98 page(s) … link audit OK`), so leg 4's docs paragraph is gate-verified. And
the demonstrated red is now **fourteen** isolating negations (§5), each failing exactly one
test.

### 1.8 T4 — closed/rejected prior art, mechanically

Run on the merged base, recorded so sign-off does not have to take it on trust:

* `git log --all -S<sym> -- crates/` for `OwnedEntry`, `StagedPlacement`, `PartNumberSet`,
  `RetirePayload`, `decode_owned_entry`, `decode_retire_obligation` → **0 commits** in any of
  the 13 remote refs + 10 local branches.
* Open PRs on `getwyrd/wyrd` today: **#712, #713, #714 — all dependabot CI bumps**; none
  touches `crates/core/src/metadata.rs` or `crates/core/src/multipart.rs`. (#710/#721/#722 of
  the brief's conflict list are not open PRs yet, so no live conflict exists at this moment.)
* Closed-**unmerged** PRs, all time: **4** — #647 (`enhancement/635-segmented-chunk-map`,
  superseded by the merged segmented-map work), #550, #526, #525 (dependabot). None is prior
  art for these record types.

---

## 2. The three shaping decisions (unchanged from iteration 1)

**(a) No envelope; the two decoders take the key.** `decode_owned_entry(key, bytes)`
(`multipart.rs:2623`) and `decode_retire_obligation(key, value)` (`:3016`) are their own entry
points over `metadata::encode`/`decode`, in the shape `decode_segment_record`
(`metadata.rs:2614`) and `decode_admission_record` (`multipart.rs:1678`) already use — one
extra parameter aside. Nothing dispatches; #715's third-attempt envelope was neither salvaged
nor rebuilt.

**(b) Each rule has exactly one home, even where two seams apply it.**
`checked_ownership_pairing` (`multipart.rs:2411`) is called by `TryFrom<PendingEntryWire>`
(`metadata.rs:1627`), by `OwnedEntry::from_pending` and by `decode_owned_entry`;
`checked_staged_scheme` (`multipart.rs:2487`) by `StagedPlacement::new` and by its `TryFrom`;
leg 1p reuses #716's `checked_chunk_scheme` (`multipart.rs:2128`) rather than a second copy.
`decode_owned_both` (`tests:158`) decodes each `sidx:` witness through **both** seams and
asserts they agree, so a future divergence fails a test.

**(c) One wire mirror, deliberately, and its cost.** `decode_owned_entry` reads
`OwnedEntryWire` (`multipart.rs:2519`, 3 fields) rather than decoding `PendingEntry`: **9
lines** of duplicated *shape* (no duplicated rules — see (b)), bought so the rejections arrive
as `TornOwnedEntry` / `NotAnOwnedEntry` / `StagedSchemeUnsupported` instead of a
`MalformedRecordValue` whose `detail` is a serde string. That is the same reason
`decode_admission_record` reaches `AdmissionRecordWire` (`multipart.rs:1671-1677`: serde's
`Error::custom` funnel stringifies a domain error before a `downcast` could recover it). The
rejected alternative — decode the shared record and map every failure to
`MalformedRecordValue{namespace: "sidx:"}` — saves those 9 lines and costs legs 1i and 1e
their typed variants, leaving the test asserting on substrings of a serde message.

## 3. The salvaged defects, and what they are now

| Recorded defect (#692 v2 review) | Fix here |
|---|---|
| the token **suffix ignored** (v2 `multipart.rs:2024`: `(RetireToken::Session { .. }, _) => {}` accepted **every** session-scoped payload) | `checked_against_token` (`multipart.rs:2921`) matches payload shape against `part.is_some()` in **four explicit arms**, so each of leg 1h's three demonstrations negates in isolation |
| `StagedPlacement` deriving an **unchecked** `EcScheme` (v2 `:1657`) | `#[serde(try_from = "StagedPlacementWire")]` over #716's closed `EcSchemeWire`, with `checked_staged_scheme` applying `erasure::supported` (`erasure.rs:120`) at the bare type, through the shared record, and through the key-taking decoder |
| decoders that could not see the key (v2 `:1555`/`:1789`/`:1800`) | both decoders take the key; the three key relations (owner, generation identity/scope, part scope) are typed variants |
| `structural(&'static str, String)` stringly errors | 12 typed variants (`multipart.rs:300-411`), each naming **one** rule |

`RetirePayload` **must** stay externally tagged: an internally tagged enum buffers content
through serde's `Content`, which cannot carry a 128-bit `ChunkId`, so `{"kind":"Chunks",…}`
would fail on any real chunk list. Recorded on `RetirePayloadWire`'s doc (`multipart.rs:2730`).

## 4. Scope lines I did not cross

* **1e is a shape check, not a namespace check.** A both-present value under a `pending:` key
  still decodes (`tests:409-443`); making it reject means teaching `renew_pending` /
  `live_lease_guards` to be key-aware — live readers, out of this slice. The brief defers it
  and marks the owner provisional; nothing here assumes #657 owns it.
* **Leg 1e's observable** is `metadata::decode::<PendingEntry>` returning `Err`, never an
  ADR-0045 `MetadataValidationError` out of the live readers.
* **Placement length stays contextual** (`tests:475`), and the length-mismatch witness is the
  only short placement in the file — without that, the leg-3 negation failed 7 tests instead
  of 1.
* `crates/custodian/src/` untouched; no ADR/proposal/spec edited; no `Cargo.toml` /
  `Cargo.lock` change; outcome enums, `multipart_etag`, knob values and store round trips
  untouched.
* **Citation drift I deliberately did not chase:** this patch inserts ~80 lines into
  `metadata.rs` above `renew_pending`, so pre-existing `metadata.rs:NNNN` citations *in
  #715/#716's own doc comments* (e.g. `multipart.rs:1588`, `:1600`, `:1697`, `:1886` citing
  `metadata.rs:2012`) now point ~80 lines early. Renumbering them means editing #716's lines —
  a rebase surface the brief tells me twice to keep small, and #710/#721 will move them again.
  Every citation **inside my own hunks** was refreshed against the patched tree instead
  (`:2085-2090` for the renewal CAS, `:2121`/`:2125` for the lease guards).

## 5. The fourteen demonstrations — production negated, test run, reverted

Each negation removes **one** guard and nothing else, then `cargo test -p wyrd-core --test
multipart_staging_retire` runs. Every one failed **exactly one** test (`25 passed; 1 failed`),
which is what makes each leg load-bearing rather than incidentally green. Driver + full logs:
`$PDCA_SCRATCH/pdca-builder-717-redleg/negate.py` + `*.log` (removed at the end of the run;
the failure text below is verbatim). The tree was restored and re-verified green after each.

1. **1b — owner vs key** (drop the `owner != key_owner` guard):
   `owned_entry_owner_must_agree_with_its_key` FAILED — *expected OwnedEntryOwnerMismatch, got
   Ok(OwnedEntry { owner: UploadId("1a1a…"), lease_expiry_millis: 9000, staged: StagedPlacement
   { scheme: ReedSolomon { k: 2, m: 1 }, placement: [7, 8, 9] } })*
2. **1d — generation identity** (drop the `(inode, version)` comparison):
   `retire_payload_scope_must_agree_with_its_token` FAILED — *expected
   RetireGenerationIdentityMismatch, got Ok((Generation { inode: 42, version: 6 }, Generation {
   inode: 42, version: 5, … }))*
3. **1d — scope, the other half** (both cross-scope arms return `Ok`): same test FAILED —
   *expected RetireTokenScopeMismatch, got Ok((Session { … part: None }, Generation { … }))*
4. **1h — a whole-session obligation under a per-part token** (delete
   `(Self::Session {}, Some(_))`): `whole_session_obligations_are_rejected_under_a_per_part_token`
   FAILED — *expected RetireTokenSuffixMismatch for session, got Ok((Session { … part:
   Some((PartNumber(4), AttemptId("3c3c…"))) }, Session))*
5. **1h — chunks under a session-wide token** (delete `(Self::Chunks { .. }, None)`):
   `per_part_obligation_is_rejected_under_a_session_wide_token` FAILED — *expected
   RetireTokenSuffixMismatch, got Ok((Session { … part: None }, Chunks { … }))*
6. **1h — records under a per-part token** (delete `(Self::Records { .. }, Some(_))`):
   `records_obligation_is_rejected_under_a_per_part_token` FAILED — *expected
   RetireTokenSuffixMismatch, got Ok((Session { … part: Some(…) }, Records { parts:
   Some(PartNumberSet([(3, 3)])), segments: None }))*
7. **1i — `StagedPlacement`'s `EcScheme`** (`checked_staged_scheme` returns `Ok`):
   `staged_placement_scheme_must_be_supported` FAILED — *assertion failed:
   metadata::decode::<StagedPlacement>(unsupported.as_bytes()).is_err()*
8. **1e — the torn pairing** (`checked_ownership_pairing` returns `Ok`):
   `torn_pending_entry_is_rejected_under_both_readings` FAILED — *the pending: reading must
   refuse the torn shape {"lease_expiry_millis":9000,"owner":"1a1a…"}*
9. **1n — a generation naming both sources** (drop the `RetireGenerationSourcesConflict`
   guard): `generation_obligation_names_exactly_one_source` FAILED — *assertion failed:
   matches!(decode_retire_obligation(&key, both.as_bytes()),
   Err(RecordError::RetireGenerationSourcesConflict { chunks: 1 }))*.
   **Which guard covers which branch:** *both* and *neither* are **separate** guards
   (`RetireGenerationSourcesConflict` and `RetireObligationOwesNothing{payload: "generation"}`),
   so the neither-branch does **not** ride this negation; it is asserted in the same test and
   again in `an_obligation_that_owes_nothing_is_refused`.
10. **1p — nested `ChunkRef`** (drop the `checked_chunk_scheme` fold in both arms):
    `retire_payload_nested_chunk_scheme_must_be_supported` FAILED — *assertion failed:
    matches!(…, Err(RecordError::ChunkSchemeUnsupported { k: 0, m: 1, .. }))*
11. **Leg 2 — remove ONE `skip_serializing_if`** (on `PendingEntry::owner`):
    `legacy_pending_entry_re_encodes_byte_identically` FAILED — *left:
    "{\"lease_expiry_millis\":9000,\"owner\":null}"* vs *right:
    "{\"lease_expiry_millis\":9000}"*
12. **Leg-3 corollary negated the other way** (make `StagedPlacement::new` reject a
    length-mismatched placement): `owned_entry_with_length_mismatched_placement_decodes` FAILED
    — *a length-mismatched placement is not a decode error: StagedSchemeUnsupported { k: 0, m: 0 }*
13. **NEW — the `Records` emptiness rule** (restore iteration 1's per-field two-`if` arm,
    §1.1): `a_records_obligation_owing_only_segments_decodes` FAILED — *a records obligation
    that owes its segments decodes: RetireObligationOwesNothing { payload: "records" }*
14. **NEW — the closed `PendingEntry` shape** (remove `deny_unknown_fields`, §1.2):
    `an_unknown_field_is_refused` FAILED — *an unknown field on a pending: value must not
    decode: renewal would delete it*

## 6. Forced self-refutation

* **(a) Genuine red?** **Yes** — fourteen times over, §5, each reverting *the fix* (the guard)
  and each printing the production value that should have been refused, or the byte string
  that should not have been rewritten. The whole-file RED (revert everything → the test fails
  to **compile**, since it names types that do not exist on the base) is the brief's
  pre-declared UNVERIFIABLE, unchanged.
* **(b) Production path?** **Yes.** Every witness goes through the production codec
  (`wyrd_core::metadata::encode`/`decode`), the production key builders (`sidx_key`,
  `retire_key` — never a hand-spelled key string) and the production decoders under test. No
  mock, no re-implementation, no test-only copy of a rule; the two `_both` helpers exist only
  to run the *same* bytes through two production seams and compare. The two new assertions are
  no exception: §1.1's runs through `decode_retire_obligation`, §1.2's through
  `metadata::decode::<PendingEntry>` — the very call `renew_pending` (`metadata.rs:2085`) and
  GC's sweep (`crates/custodian/src/gc.rs:489`) make.
* **(c) Fixture includes the fault?** **Yes.** The fault is hand-authored into the witness
  every time: the key names another session (1b), the token names another generation or the
  wrong scope (1d), the token carries — or omits — the `:<part>:<attempt>` suffix (1h ×3), the
  stored scheme is `rs(0,1)` (1i, 1p), the value carries exactly one ownership field (1e), the
  generation names two sources (1n), the legacy value carries neither field (leg 2), the
  `Records` payload carries the empty part set *and* the segments (13), the `pending:` value
  carries the unknown field (14). Nothing is curated out: the positive control sits *beside*
  each negative — the same bytes under their honest key decode — which is what proves the
  rejection is about the key/value relation and not about the value alone.

## 7. Gates run here

* `cargo test -p wyrd-core --test multipart_staging_retire` → **26 passed**.
* `cargo fmt --all -- --check` → clean (the target's own commit hook runs rustfmt).
* `cargo clippy --workspace --all-targets` → clean (`-D warnings` via the workspace lints).
* `typos` over every touched file → clean.
* `./engine/xtask.sh ci` on the exact tree `patch.diff` was cut from → **`xtask ci: all checks
  passed`, exit 0** (and again on the near-final tree, same result). Includes: prose gates (`typos`, `lint_docs: OK`, `render_site … 98
  page(s) … link audit OK`), fmt, clippy, the **whole workspace test suite** (nothing
  stalled), `cargo deny check` + both `--all-features` legs (`advisories ok`), `cargo-machete`,
  `xtask conformance` (5 valid + 6 invalid vectors), the ADR-0035 statics gate, the ADR-0010
  deploy guard, and the madsim DST leg.

## 8. Budget, honestly (the C3 item)

Measured on the final tree, added lines only:

| | raw | comment | blank | **code** |
|---|---:|---:|---:|---:|
| `crates/core/src/multipart.rs` | 862 | 341 | 36 | **485** |
| `crates/core/tests/multipart_staging_retire.rs` (new) | 879 | 213 | 49 | **617** |
| `crates/core/src/metadata.rs` | 81 | 50 | 3 | **28** |
| 8 mechanical ripple files | 28 | 0 | 0 | **28** |
| `docs/design/architecture/05-building-block-view.md` | 2 | 0 | 1 | **1** |
| **total** | **1852** | **604** | **89** | **1159** |

Against the brief's ≈960 semantic estimate that is **+199 (+21 %)**, and it is *not* spread
evenly: production is 513 against the briefed 465, and the whole overage is the test file (617
against 440). That file is where the criterion lives — 14 negations, one per binding rule,
plus a round trip per landed type, plus the two regressions this round's blocking findings
demanded. `assert!(matches!(…))` witnesses render across 4–6 lines each under rustfmt. I did
not trim tests to hit the number: every test in the file defends a rule this patch implements,
and deleting one leaves landed code unexercised — the worse trade, and the same judgment
iteration 1 recorded. Sign-off may still call it a re-plan; the number is above so the call
can be made on data.

## 9. What a reviewer should look at hardest

1. **`deny_unknown_fields` on the shared `PendingEntry`** (§1.2) — the one live-path
   *behaviour* change in the patch, and the one place I widened the brief's letter to close a
   gating finding. Trade-off stated: a future field written by a newer binary now makes an
   older binary fail its `pending:` decode (loud: renewal errors, the GC sweep aborts) instead
   of silently deleting that field on the next renewal. Fail-closed on a deletion path is the
   repo's rule (`AGENTS.md:161-177`, ADR-0045), but it is a judgment call and it belongs in
   §9 of the sign-off, not buried.
2. **The `Records` emptiness rule** (§1.1) — read it as "the obligation must owe *something*",
   not "each field must be non-empty". `Generation` was always judged that way; `Records` now
   is too.
3. **`RetirePayload` must stay externally tagged** (serde's `Content` cannot carry a 128-bit
   `ChunkId`). Recorded on the wire type's doc.
4. **The four-arm suffix rule** (`checked_against_token`, `multipart.rs:2921`) — four explicit
   arms rather than a table lookup, *so that each of leg 1h's three demonstrations negates in
   isolation*; a table would have made one negation kill all three.
5. **What I deliberately did NOT do:** no `pending:` reader was made key-aware (1e stays a
   shape rule), no custodian source was touched, no ADR/proposal/spec was edited, and
   #715/#716's own citations were not renumbered (§4).

## 10. Open items for the human at sign-off

* **The 1e deferral's owner is still provisional** — the brief says so; nothing here verifies
  that #657 (or whichever slice first writes a `sidx:` record) carries the obligation to make
  the `pending:` readers key-aware. Unowned deferral → forgotten gap.
* **Decode-only enforcement** (§1.6) — mitigated with a public `checked_shape`, not closed. A
  writer can still mint a `RetirePayload` its own decoder will refuse; the decision "decode is
  the boundary" is the standing one and #656–#659 is where it gets tested by a real writer.
* **Scope of §1.2** — one line beyond the brief's literal `metadata.rs` allowance, taken to
  clear a gating review finding. Confirm or send it back.
