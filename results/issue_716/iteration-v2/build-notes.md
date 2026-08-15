# Build notes — issue #716 (iteration 2)

**Slug:** multipart-session-lifecycle-records
**Base:** `getwyrd/wyrd @ main`, worktree `$PDCA_WORKTREE` at `6151063` (merge of #724 =
#715's `5eeca16 multipart: land Budget/AdmissionRecord decode validation`). Every `path:line`
below is against that base unless it says "patched file".
**Files:** exactly three — `crates/core/src/multipart.rs`,
`crates/core/tests/multipart_session_records.rs` (new),
`docs/design/architecture/05-building-block-view.md`.
**Size:** 766 added semantic lines (non-blank, non-comment), against the brief's ≤ 770.
Measured the same way the round-1 reviewer measured 726: `git diff | grep '^+' | drop blank |
drop lines starting '//'`. Per file: production 351, test 414, docs 1.

---

## 1. What changed relative to iteration 1, and why

Iteration 1 was accepted on C1/C3/T1 and **failed** T2 Shape, C5-mutants and T4-batch-review.
All four findings were one defect plus its evidence gap. This rebuild is iteration 1's patch
with that defect removed at the cause and the evidence made load-bearing; nothing else in the
record set was re-litigated (the reviewer passed C3 "the change stays on the declared codec
seam").

### 1a. The T2 FAIL / all three batch-review findings: `content_type` serialization identity

The finding (three independent review passes, `review-batch.md`; check-review T2):

> `#[serde(default)]` accepts an omitted `content_type` at `multipart.rs:1559` (v1 patched
> file), then serialization inserts `"content_type":null`, contradicting the byte-preservation
> contract.

That is a real, durable bug and it is the repo's own named defect class —
`AGENTS.md:170-172`: *"optional/legacy fields are omitted when absent, never emitted as
defaults — decode→encode must be byte-identical wherever a compare-and-swap or content hash
depends on it (add the round-trip test)"*. A session transition CASes the session record whole
on its **exact current bytes** (`0016:555-558`), so a decode→encode that inserts a field is
durable either way: `require(key, encode(prior))` (`metadata.rs:1794`, `:1919`) becomes a
permanent `Conflict`; `require(key, current)` on the raw bytes read (`metadata.rs:2012`) wins
the CAS and silently rewrites the record.

**Fix — both spellings, not one.** The hole has two sides and closing one leaves the other:

* absent is spelled by **omitting** the field:
  `#[serde(skip_serializing_if = "Option::is_none")]` on `SessionRecord::content_type`
  (patched `multipart.rs:1641`). This is the target's own convention, stated at length for
  `InodeRecord`'s optional trio at `metadata.rs:1394-1419` (`etag`, `content_type`, `modified`
  — the identical field, the identical reason);
* and `null` — `Option<String>`'s *other* spelling of the same `None` — is refused at decode
  (`de_content_type`, patched `multipart.rs:1567`). Without this, `{"content_type":null}`
  decodes and re-encodes **omitted**: the same rewrite-under-CAS, mirrored. The negation
  `identity-null-spelling` in §3 shows exactly those bytes changing.

The property the type now states and the test now enforces is total over the accepted set:
**`encode(decode(bytes)) == bytes` for every value this record accepts** — not merely for the
shapes the encoder happens to write. The doc says so and says what it does *not* claim (decode
is not a canonicalisation check — a reordered/whitespaced foreign spelling still decodes and
re-encodes in this codec's spelling), mirroring `AdmissionRecord`'s own caveat (patched `multipart.rs:1367-1373`, #715's text).

Why strictness costs nothing **here** but does for `InodeRecord`: `InodeRecord` must keep
decoding records written before those fields existed, so it can only skip on write. This record
class has **no** stored corpus — its first writer is #656–#659 — so refusing the spelling the
encoder can never emit forecloses the hazard instead of documenting it.

*Rejected alternative — require `content_type` always present (no `default`), always
serialized.* It also makes decode→encode total, and costs the same ~3 lines. Rejected because
it violates the standing repo rule verbatim (`AGENTS.md:170-172`: "omitted when absent, never
emitted as defaults") and because `0016:350` lists `content_type` among the session's fields
without making it mandatory: a `CreateMultipartUpload` with no `Content-Type` header has none,
and spelling that as a stored `null` is exactly the "emitted as a default" this repo already
decided against for the identical field on `InodeRecord`.

*Rejected alternative — `skip_serializing_if` only (accept `null`, re-emit omitted).* One line
cheaper (drops `de_content_type`'s 5 lines + its `deserialize_with` attribute). Rejected because
it leaves precisely the mirror image of the shipped bug: an accepted value whose re-encode is
not the bytes read. The `identity-null-spelling` negation in §3 is that alternative, run: it
turns `{"…","content_type":null,…}` into `{"…",…}` on re-encode.

### 1b. The C5 finding: three surviving mutants

Round 1: `51 mutants tested: 3 missed` —
`SessionRecord::content_type -> None`, `SessionRecord::attempts -> 0`,
`PartRecord::is_empty -> false`. Each survived because every witness in the suite observed only
that field's *default* value (`None`, `0`, `false`), so a decoder that stopped reading the field
passed.

Fixed by giving each a non-default witness rather than by adding assertions to the same one:

* `content_type` → `session_with_content_type_round_trips` decodes a record carrying
  `"content_type":"text/plain"` and asserts `Some("text/plain")` (test:229);
* `attempts` → every session witness now carries `ATTEMPTS = 2`, deliberately neither `0` nor
  `1` so both default-return mutants die (test:48);
* `is_empty` → `part_with_zero_length_chunk_is_empty` decodes a part whose lone chunk has
  `len: 0` and asserts `is_empty()` **true**, beside `part_round_trips`' `false` (test:372).
  That witness doubles as the standing proof that the brief's **withdrawn** "logical `len` must
  be non-zero" rule is not enforced (`erasure.rs:79-83`, `shard_size`'s `.max(1)`).

Result on the final patch: **`54 mutants tested in 2m: 34 caught, 20 unviable`, 0 missed**
(`scripts/mutants-in-diff`, run against this bundle's `patch.diff` in `$PDCA_WORKTREE`).

### 1c. The T5 finding: an omitted-`content_type` witness

Round 1's round-trip helper always supplied explicit `null`, so no witness ever exercised the
omitted spelling — the suite could stay green while an accepted value changed bytes. Now the
*default* witness omits the field (`session()` → `session_with(None, …)`, test:76), and
`session_absent_content_type_re_encodes_omitted` (test:245) asserts the re-encoded bytes contain
no `content_type` key at all.

### 1d. Two evidence changes that were not asked for but close the same class

* **Byte identity is asserted centrally, not per test.** `decode_both` (test:160) re-encodes
  whatever it decoded and compares byte-for-byte with the input, so *every* accepted witness in
  the file is an identity witness — a new test cannot silently omit the check. This is what
  makes the `identity-*` negations in §3 fail loudly across five tests instead of one.
* **Leg 1c now perturbs both halves of its rule.** Round 1 only moved `publish_target.parent`,
  so the `|| publish_target.name != wire.object` clause was unbound. A second witness renames
  the object with the parent agreeing (test:416); the `1c-name-clause` negation in §3 confirms
  the clause is load-bearing.

---

## 2. The seven landed types and where each rule lives

| Type | Wire | Rule enforced at decode | Source |
|---|---|---|---|
| `SessionRecord` | `SessionRecordWire`, `deny_unknown_fields` | 1c `publish_target.parent`/`name` == session's `parent`/`object`; 1c-epoch `publish_target.epoch` == session's `epoch` | `0016:350`, `:555-563` |
| `SessionState` | derived, `tag = "kind"`, `deny_unknown_fields` | 1j state-forbidden / state-required fields | `0016:403-415` |
| `PublishTarget` | derived, `deny_unknown_fields` | none of its own (a plain value) | `0016:350` |
| `Completion` | derived, `deny_unknown_fields` | none of its own | `0016:350` |
| `SlotRecord` | `SlotRecordWire`, `deny_unknown_fields` | 1i `lease_expiry_millis > reserved_at_millis` | `0016:349` |
| `PartRecord` | `PartRecordWire`, `deny_unknown_fields` | 1i every `ChunkRef`'s `EcScheme` passes `erasure::supported`; 1k `len` == overflow-checked chunk sum | ADR-0045:69-74; `0016:351` |
| `PartSummary` | derived, `deny_unknown_fields` | none of its own | `0016:352` |

Codec seam, exactly as the brief's **Citations expected** prescribes and with **no** envelope:
each type validates inside its own `Deserialize` (`try_from = "…Wire"`, the `InodeRecord` shape
at `metadata.rs:1376-1377`), rides the store-wide `metadata::encode`/`decode`
(`metadata.rs:1564`, `:1569` — the brief's `:1536-1543` is stale on this base), and gets a
per-record attributed wrapper (`decode_session_record`, `decode_slot_record`,
`decode_part_record`, `decode_part_summary`) peer to `decode_segment_record`
(`metadata.rs:2536-2547`) so a store round trip can tell "this record is torn" from "the store
is failing".

**Deliberately NOT enforced at decode** (each would be an invented or knob-dependent bound —
`0016:390-402`): `MAX_PART_CHUNKS` on the chunk list, `MAX_COMPLETE_ATTEMPTS` on `attempts`
(doc'd on the accessor), any non-zero `len`, and `ChunkRef.placement` length — the last is the
brief's binding positive leg (ADR-0045:69-74, `AGENTS.md:146-149`, `0016:416-429`).

---

## 3. Forced refutation — the nine required demonstrations, plus three more

Method: revert **one** production rule at a time from the final tree, run the project's test
command for this file (`cargo test -p wyrd-core --test multipart_session_records`, the brief's
GREEN leg), record the failure, restore. Harness + full logs:
`$PDCA_SCRATCH/pdca-builder-716-negations/{negate.py,negations-final.log}` (removed at the end
of the run; the outputs below are the verbatim record).

Green baseline, final tree: `test result: ok. 24 passed; 0 failed`.

### The eight isolating negations

**1c — drop the `publish_target` key check** → `23 passed; 1 failed`

```
---- leg_1c_publish_target_key_mismatch_is_rejected stdout ----
assertion `left == right` failed
  left: Ok(SessionRecord { parent: 42, object: "key/one", … publish_target: PublishTarget { parent: 43, name: "key/one", epoch: 3 } } })
 right: Err(PublishTargetKeyMismatch { session_parent: 42, session_object: "key/one", target_parent: 43, target_name: "key/one" })
```

**1c-epoch — drop the fence-epoch check** → `23 passed; 1 failed`

```
---- leg_1c_epoch_publish_target_epoch_mismatch_is_rejected stdout ----
  left: Ok(SessionRecord { … publish_target: PublishTarget { parent: 42, name: "key/one", epoch: 4 } } })
 right: Err(PublishTargetEpochMismatch { session_epoch: 3, target_epoch: 4 })
```

**1i (ChunkRef scheme) — make `erasure::supported` non-binding** → `23 passed; 1 failed`

```
---- leg_1i_chunk_scheme_unsupported_is_rejected stdout ----
  left: Ok(PartRecord { chunks: [ChunkRef { id: 7, scheme: ReedSolomon { k: 0, m: 1 }, len: 100, placement: [] }], … })
 right: Err(ChunkSchemeUnsupported { chunk_id: 7, k: 0, m: 1 })
```

**1i (slot) — drop the lapsed-lease check** → `23 passed; 1 failed`

```
---- leg_1i_slot_lease_already_lapsed_is_rejected stdout ----
  left: Ok(SlotRecord { part_number: PartNumber(1), attempt_id: AttemptId("bbbb…"), reserved_at_millis: 1000, lease_expiry_millis: 1000 })
 right: Err(SlotLeaseAlreadyLapsed { reserved_at_millis: 1000, lease_expiry_millis: 1000 })
```

**1j (forbidden field) — `Open {}` → unit `Open`** (the one-token way to drop the per-variant
field check; the gap the type's own doc records) → `23 passed; 1 failed`

```
---- leg_1j_open_session_with_forbidden_completing_field_is_rejected stdout ----
assertion `left == right` failed: decode->encode is not byte-identical for SessionRecord { … state: Open }
  left: "{…,\"state\":{\"kind\":\"Open\"}}"
 right: "{…,\"state\":{\"kind\":\"Open\",\"fenced_at_millis\":9}}"
```

(The permissive decoder silently *drops* the forbidden field — the failure prints the dropped
byte range, which is the F-class this leg exists for.)

**1j (missing required) — `#[serde(default)]` on `segments_written`** → `23 passed; 1 failed`

```
---- leg_1j_completing_session_missing_required_field_is_rejected stdout ----
  left: "…\"state\":{\"kind\":\"Completing\",\"fenced_at_millis\":1,\"segments_written\":0,\"publish_target\":…"
 right: "…\"state\":{\"kind\":\"Completing\",\"fenced_at_millis\":1,\"publish_target\":…"
```

**1k — drop `total != wire.len`** → `23 passed; 1 failed`

```
---- leg_1k_part_length_mismatch_is_rejected stdout ----
  left: Ok(PartRecord { chunks: [ChunkRef { id: 1, scheme: None, len: 100, … }], len: 50, … })
 right: Err(PartLengthMismatch { declared: 50, chunks: 100 })
```

**1m — drop `deny_unknown_fields` from `SlotRecordWire`** → `23 passed; 1 failed`

```
---- leg_1m_unknown_field_is_rejected stdout ----
assertion `left == right` failed: decode->encode is not byte-identical for SlotRecord { … }
  left: "{…,\"lease_expiry_millis\":2}"
 right: "{…,\"lease_expiry_millis\":2,\"extra_field\":true}"
```

### The one positive leg, negated the other way

**placement length made structural** (a length check added *after* the scheme check, so the
scheme leg still attributes correctly and only the positive leg moves) → `23 passed; 1 failed`

```
---- leg_1i_chunk_ref_wrong_placement_length_still_decodes stdout ----
a ChunkRef whose placement length disagrees with its scheme's fragment count still decodes
(ADR-0045: placement length is contextual, not structural): ChunkSchemeUnsupported { chunk_id: 1, k: 2, m: 1 }
```

### Three more, for the rules this rebuild adds

**identity-omitted — drop `skip_serializing_if`** (i.e. re-ship iteration 1's defect) →
`19 passed; 5 failed`; every session witness whose `content_type` is absent goes red:

```
---- session_absent_content_type_re_encodes_omitted stdout ----
assertion `left == right` failed: decode->encode is not byte-identical for SessionRecord { … content_type: None … }
  left: "{\"parent\":42,\"object\":\"key/one\",\"content_type\":null,\"created_at_millis\":1000,…}"
 right: "{\"parent\":42,\"object\":\"key/one\",\"created_at_millis\":1000,…}"
```

**identity-null-spelling — `de_content_type` accepts `Option<String>`** (the cheaper rejected
alternative of §1a) → `23 passed; 1 failed`:

```
---- session_null_content_type_spelling_is_rejected stdout ----
  left: "{\"parent\":42,\"object\":\"key/one\",\"created_at_millis\":1000,…}"
 right: "{\"parent\":42,\"object\":\"key/one\",\"content_type\":null,\"created_at_millis\":1000,…}"
```

**1c-name-clause — drop only `|| publish_target.name != wire.object`** → `23 passed; 1 failed`:

```
---- leg_1c_publish_target_key_mismatch_is_rejected stdout ----
  left: Ok(SessionRecord { parent: 42, object: "key/one", … publish_target: PublishTarget { parent: 42, name: "key/other", epoch: 3 } } })
 right: Err(PublishTargetKeyMismatch { … target_parent: 42, target_name: "key/other" })
```

Eleven of the twelve isolate to exactly one failing test; `identity-omitted` deliberately fails
five, because the guard it removes is the one every session witness in the class depends on.

---

## 4. The three forced questions

**(a) Genuine red?** Yes — twelve times, each with the fix reverted and the failure pasted
above. Note the honest limit the brief pre-declares: with the *whole* production hunk reverted
the test does not compile (born-at-tier, criterion-absence red), which is why the binding
evidence is the per-rule negations, not a whole-patch revert. Each negation above reverts a
**single** rule from an otherwise complete tree, which is the stronger claim.

**(b) Production path?** Yes. Every witness is bytes → the production decoders exported from
`wyrd_core::multipart` (`decode_session_record`, `decode_slot_record`, `decode_part_record`,
`decode_part_summary`) **and** the store-wide `wyrd_core::metadata::decode::<T>`, asserted to
agree; every re-encode goes through the production `wyrd_core::metadata::encode`. No type is
constructed in the test (none of them has a constructor), no decoder is re-implemented, no mock
exists. `decode_both` (test:160) is the single funnel, generic over the three record types, so
no record can be given a weaker check than its siblings.

**(c) Fixture includes the fault?** Yes. Every negation's witness is the torn value itself, and
each is built to violate **only** its own rule: the 1i scheme witness sets `len` to exactly its
chunk's length so leg 1k holds; the 1c witness keeps `name` and `epoch` agreeing; the 1c-epoch
witness keeps `parent`/`name` agreeing; the 1m witness is a slot whose four real fields are all
well-formed. Nothing is curated out — the failing element is in the fixture in every case.

---

## 5. Gates run locally

| Gate | Command | Result |
|---|---|---|
| Focused test | `cargo test -p wyrd-core --test multipart_session_records` | `24 passed; 0 failed` |
| C5-mutants | `scripts/mutants-in-diff` (bundle `patch.diff`) | `54 mutants tested in 2m: 34 caught, 20 unviable` — **0 missed** (was 3) |
| C4-ci | `./engine/xtask.sh ci` | `xtask ci: all checks passed` (exit 0), incl. this file's 24 tests, `typos`, `lint_docs`, `render_site --check` (98 pages), `fmt --check`, `clippy --all-targets`, `cargo deny`, `cargo-machete`, the statics/unsafe/deploy guards and the madsim DST leg |
| Formatter (commit hook) | `cargo fmt --all --check` | clean |
| Intra-doc links | `cargo doc -p wyrd-core --no-deps` | no error in `multipart.rs` (the workspace denies `broken_intra_doc_links`; the pre-existing private-link errors in `write.rs`/`metadata.rs`/`read.rs`/`erasure.rs` are on the base and are why `cargo doc` is not part of `xtask ci`) |

**One environment note, recorded because it cost wall-clock and a human may see it again.** An
intermediate `xtask ci` run wedged for >1 h inside `tests/custodian_gc.rs` while a second
worktree (`wyrd.pdca-wt-l0`) was running its own `cargo test --workspace` on the same host —
the known target-side test-deadlock class `pdca.toml` records as getwyrd/wyrd#646. It was
killed and re-run; the re-run passed end to end in the normal ~2 min (v1's C4-ci gate log
records 110 s for the same command). Nothing in this patch reaches `wyrd-custodian` — the diff
is `crates/core/src/multipart.rs`, one new `crates/core/tests/` file and one docs paragraph —
and `custodian_gc` passes in the green run above.

`typos` / `docs-renderer` are the load-bearing external deps for leg D's architecture-doc edit
(brief **External dependencies**); both are registered doctor ids and run inside `cargo xtask
ci`'s prose gates, which reported no findings in the run above. Nothing outside the base Rust
toolchain was needed, and no NEEDS-HUMAN external dependency was hit.

## 6. Scope discipline

Untouched, per the brief's out-of-scope list: `Budget`/`AdmissionRecord` (#715, merged
beneath), `OwnedEntry`/`StagedPlacement`/retirement types/`PendingEntry` (#717), the outcome
enums and answer table (#693), knob values (#655), store round trips (#656–#659). No `docs/`
file except the single `05-building-block-view.md` paragraph (leg D). No fourth file. The only
edits to lines #715 authored are none — the new code appends sections 6–8 after
`decode_admission_record` and adds six variants + six `Display` arms to the shared
`RecordError`, which is exactly the shared-enum coupling the brief's ordering note predicted.

Leg D note: the brief expected #715 to have landed a multipart paragraph to extend. It did not
(`git show --stat 5eeca16` = `multipart.rs` + its test only), so — as the brief instructs
("Read what #715 actually landed on the merged base") — this child writes the paragraph,
covering `mpuctl` alongside its own four record classes, plus the cross-field-disagreement rule
and the omitted-when-absent identity that are this change's persisted-field facts.
