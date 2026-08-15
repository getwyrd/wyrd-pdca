# Build notes — issue #716 (iteration 3)

**Slug:** multipart-session-lifecycle-records
**Base:** `getwyrd/wyrd @ main`, worktree `$PDCA_WORKTREE` at `6151063` (merge of #724 = #715's
`5eeca16 multipart: land Budget/AdmissionRecord decode validation`). `patch.diff` was verified
to `git apply --check` cleanly against a pristine checkout of that commit.
**Citation frame:** every `path:line` is against that base, EXCEPT `crates/core/src/multipart.rs`
line numbers of code this patch adds or moves, which are patched-file lines (marked "patched").
That is the frame the review reads in, and the frame the previous round's findings were cited in.
**Files:** exactly three — `crates/core/src/multipart.rs`,
`crates/core/tests/multipart_session_records.rs` (new),
`docs/design/architecture/05-building-block-view.md`.

---

## 0. What changed relative to iteration 2, in one paragraph

Iteration 2's check-review was PASS on C1/C3/C5/T1/T2/T5 (T3 N/A — no runtime in scope); the
single failing gate was the
**T4 batched rubric review**, with four findings that are all one class: *a decoder that accepts
a JSON spelling whose re-encode is not the bytes it read, under a record that is CAS'd whole*.
They split in two, and the split is this iteration's whole decision:

* **the field-level half is a real defect and is FIXED at the cause.** `PartRecordWire.chunks`
  was `Vec<ChunkRef>` (v2 patched `multipart.rs:1882`), and `crate::metadata::ChunkRef` is
  deliberately tolerant: `#[serde(default)]` on `placement` (`metadata.rs:138`) and no
  `deny_unknown_fields` (`metadata.rs:128`). So a chunk with `placement` **omitted** decoded to
  `[]` and re-encoded as `"placement":[]`, and an unknown field inside a chunk decoded, vanished,
  and re-encoded gone. Both fabricate or destroy stored information on read — exactly the fault
  leg 1m closes one level up. `part:` chunks are now read through the module's own **closed**
  `ChunkRefWire` / `EcSchemeWire` (patched `multipart.rs:1914`, `:1939`);
* **the syntactic half — reordered / whitespace-bearing / escape-equivalent JSON — is
  recorded-rejected**, with the reasoning in `$PDCA_BUNDLE/review-rejected.md` (§2 below
  summarises it). It is JSON's own freedom, not a field the record rewrites; the peer record
  merged one child earlier states the same disclaimer in as many words; and it cannot be
  enforced where this module's rules live.

Everything else in the record set is iteration 2's, unchanged, and is not re-litigated here.

---

## 1. The fix, and why it is at the cause

**The change (patched `multipart.rs:1914-1955`, `:1958`, `:2040`):**

```rust
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ChunkRefWire { id: ChunkId, scheme: EcSchemeWire, len: u64, placement: Vec<DServerId> }

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
enum EcSchemeWire { None, ReedSolomon { k: u8, m: u8 } }
```

with `PartRecordWire.chunks: Vec<ChunkRefWire>` and one `map(ChunkRef::from)` in
`TryFrom<PartRecordWire>` (patched `:2040`). Three properties follow, and each is a witness:

1. a chunk whose `placement` is **omitted** is a typed decode rejection, not a value with an
   empty placement (`part_chunk_omitted_placement_is_rejected`, test:613);
2. an unknown field **inside a chunk** is a typed decode rejection
   (`leg_1m_unknown_field_in_nested_chunk_is_rejected`, test:574);
3. an unknown field **inside the chunk's `scheme`** likewise
   (`leg_1m_unknown_field_in_nested_scheme_is_rejected`, test:587) — without this the closure
   would stop one level short and leave `{"ReedSolomon":{…}}` open.

**Why at this seam rather than in `metadata.rs`.** Changing `ChunkRef` itself (dropping
`#[serde(default)]`, adding `deny_unknown_fields`) would be the "one definition" fix, and it is
wrong here for two independent reasons: `inode:` and `seg:` records have a **stored corpus** that
turns on that tolerance — the field is documented as "additive metadata on a never-yet-deployed
schema" (`metadata.rs:120-124`) — and this child's scope pins `metadata.rs` untouched (brief
**Scope**: "every file outside `multipart.rs` + the new test"). The mirror is also the target's
own idiom for exactly this: `SegmentRecordWire` (`metadata.rs:1220-1227`), `InodeRecordWire`
(`metadata.rs:1425-1437`), `BudgetWire`/`AdmissionRecordWire` (`multipart.rs:1198`, `:1305` on
the base).

**Cost, shown, of the alternative I rejected** (see §2 for the rejected *class*). Making the
per-record decoders verify byte-canonicality is **cheaper** than what I built, not dearer — it is
rejected on correctness, not cost. The whole of it, countable:

```rust
// +1 RecordError variant  (+2 lines: the variant and its field)
// +3 lines of `Display` arm
// +8 lines here, and one call in each of the four `decode_*` wrappers (+4)
fn checked_canonical<T: Serialize>(value: T, bytes: &[u8], namespace: &'static str)
    -> Result<T, RecordError> {
    if metadata::encode(&value).as_ref() != bytes {
        return Err(RecordError::NonCanonicalRecordValue { namespace });
    }
    Ok(value)
}
```

**≈ 17 lines against the 34 production lines the wire mirrors cost.** The four reasons in §2 are
why the cheaper change is still the wrong one — chiefly that it can only live in the wrappers,
so `metadata::decode::<T>` and `decode_*` would stop accepting the same set.

**Not enforced, deliberately** (unchanged from iteration 2): `placement`'s *length* — the
standing contextual check, liberal on read (ADR-0045:69-74, `AGENTS.md:146-149`,
`0016:416-429`), which is the brief's binding positive leg; `MAX_PART_CHUNKS` /
`MAX_COMPLETE_ATTEMPTS` and every other live knob (`0016:390-402`); any non-zero `len` rule
(withdrawn by the brief's plan review — `erasure.rs:79-83`'s `shard_size` `.max(1)` handles a
zero-length chunk).

---

## 2. The three findings I did NOT fix, and why (full text in `review-rejected.md`)

Findings `multipart.rs:1684`, `:1745` (BUG) and `:1746` (CONVENTION) all say: the decoder accepts
reordered / whitespace-bearing / escape-equivalent JSON that re-encodes differently, so a
whole-record CAS could `Conflict` forever or silently rewrite. Recorded-rejected, for four
reasons that stand together:

1. **Settled on this base by the peer record, in this module.** `AdmissionRecord`'s own doc:
   *"decode is not a canonicalisation check. A foreign spelling of the same value — fields
   reordered, whitespace inserted — still decodes, to the same value, and re-encodes in this
   codec's spelling rather than in its own; JSON, not this record, is what makes those spellings
   equal"* (`multipart.rs:1266-1272` on the base; patched `:1367-1373`). That text is #715's,
   merged as `5eeca16`. Ruling the other way for the four types beside it puts two canonicality
   policies in one codec.
2. **It cannot live where this child's rules live.** The brief requires each type to validate
   **inside its own `Deserialize`**, and a `Deserialize` impl never sees the input bytes — only
   the parsed token stream. A byte-canonicality check can therefore only sit in the per-record
   wrapper, which gives the module **two decoders with different accepted sets**
   (`metadata::decode::<SessionRecord>` liberal, `decode_session_record` strict). Two spellings
   of one decision is the fault this module refuses everywhere else (its C-1 argument,
   `multipart.rs:52-58`), and the suite asserts the two surfaces agree on every witness
   (test:167-174).
3. **The hazard has no producer.** Non-canonical bytes can only come from a writer that is not
   `metadata::encode` (`serde_json::to_vec`, compact, declaration order,
   `metadata.rs:1564-1566`) — and these records have no writer at all yet (#656–#659). The
   field-level half, which a legitimate writer *can* produce, is the half that was fixed.
4. **No record decoder in this repo does it**, including the peer these are modelled on:
   `decode_segment_record` is a bare `decode::<SegmentRecord>` with the error attributed
   (`metadata.rs:2536-2547`). If store-wide byte canonicality is wanted it is a `metadata::decode`
   decision for every namespace — an edit to a file this child may not touch.

And the rubric rule the finding cites prescribes a *different* remedy than the finding asks for:
"optional/legacy fields are omitted when absent, never emitted as defaults — decode→encode must
be byte-identical wherever a compare-and-swap or content hash depends on it (add the round-trip
test)" (`AGENTS.md:170-172`). Both halves are honoured: no field of these records is defaulted,
skipped-on-read or re-spelled (that is what §1 finished), and the round-trip test is asserted on
**every** accepted witness in the file (test:175-181), not test by test.

**Residue, stated so the human can overrule it in one place:** a value hand-written into the
store by a tool that bypasses `metadata::encode` would decode and re-encode in the codec's
spelling. Under `require(key, encode(prior))` that is a loud `Conflict`; under
`require(key, current)` the CAS matches the raw bytes read and the rewrite is to the canonical
spelling of the *same* value. Neither loses a field. If that is judged unacceptable, the right
shape is an issue against `metadata.rs`, not four canonicality checks in one module.

---

## 3. Forced refutation — 15 negations, all red

Method: revert **one** production rule at a time from the final tree, run the brief's own GREEN
leg (`cargo test -p wyrd-core --test multipart_session_records`), record the failure, restore.
Harness: `$PDCA_SCRATCH/pdca-builder-716-negations/negate.py` (removed at the end of the run;
the outputs below are the verbatim record). Green baseline, final tree:
**`test result: ok. 27 passed; 0 failed`**.

### The eight brief-required isolating negations

| # | Negation (one rule reverted) | Result | Test that went red |
|---|---|---|---|
| 1c | drop the `publish_target` key check | 26 passed; **1 failed** | `leg_1c_publish_target_key_mismatch_is_rejected` |
| 1c-epoch | drop the fence-epoch check | 26 passed; **1 failed** | `leg_1c_epoch_publish_target_epoch_mismatch_is_rejected` |
| 1i-ChunkRef-scheme | make `erasure::supported` non-binding | 26 passed; **1 failed** | `leg_1i_chunk_scheme_unsupported_is_rejected` |
| 1i-slot | drop the lapsed-lease check | 26 passed; **1 failed** | `leg_1i_slot_lease_already_lapsed_is_rejected` |
| 1j-forbidden-field | `Open {}` → unit `Open` | 26 passed; **1 failed** | `leg_1j_open_session_with_forbidden_completing_field_is_rejected` |
| 1j-missing-required | `#[serde(default)]` on `segments_written` | 26 passed; **1 failed** | `leg_1j_completing_session_missing_required_field_is_rejected` |
| 1k-len-mismatch | drop `total != wire.len` | 26 passed; **1 failed** | `leg_1k_part_length_mismatch_is_rejected` |
| 1m-unknown-field | drop `deny_unknown_fields` from `SlotRecordWire` | 26 passed; **1 failed** | `leg_1m_unknown_field_is_rejected` |

Verbatim, one per leg:

```
1c   left: Ok(SessionRecord { parent: 42, object: "key/one", … publish_target: PublishTarget { parent: 43, name: "key/one", epoch: 3 } } })
    right: Err(PublishTargetKeyMismatch { session_parent: 42, session_object: "key/one", target_parent: 43, target_name: "key/one" })

1c-epoch   left: Ok(SessionRecord { … publish_target: PublishTarget { parent: 42, name: "key/one", epoch: 4 } } })
          right: Err(PublishTargetEpochMismatch { session_epoch: 3, target_epoch: 4 })

1i-scheme  left: Ok(PartRecord { chunks: [ChunkRef { id: 7, scheme: ReedSolomon { k: 0, m: 1 }, len: 100, placement: [] }], … })
          right: Err(ChunkSchemeUnsupported { chunk_id: 7, k: 0, m: 1 })

1i-slot    left: Ok(SlotRecord { part_number: PartNumber(1), attempt_id: AttemptId("bbbb…"), reserved_at_millis: 1000, lease_expiry_millis: 1000 })
          right: Err(SlotLeaseAlreadyLapsed { reserved_at_millis: 1000, lease_expiry_millis: 1000 })

1j-forbidden   assertion failed: decode->encode is not byte-identical for SessionRecord { … state: Open }
              left: "{…,\"state\":{\"kind\":\"Open\"}}"
             right: "{…,\"state\":{\"kind\":\"Open\",\"fenced_at_millis\":9}}"

1j-missing     left: "…\"state\":{\"kind\":\"Completing\",\"fenced_at_millis\":1,\"segments_written\":0,\"publish_target\":…"
              right: "…\"state\":{\"kind\":\"Completing\",\"fenced_at_millis\":1,\"publish_target\":…"

1k         left: Ok(PartRecord { chunks: [ChunkRef { id: 1, scheme: None, len: 100, … }], len: 50, … })
          right: Err(PartLengthMismatch { declared: 50, chunks: 100 })

1m         assertion failed: decode->encode is not byte-identical for SlotRecord { … }
            left: "{…,\"lease_expiry_millis\":2}"
           right: "{…,\"lease_expiry_millis\":2,\"extra_field\":true}"
```

### The one positive leg, negated the other way

`positive-placement-made-structural` — a placement-length check added *after* the scheme check,
so only the positive leg moves → **26 passed; 1 failed**:

```
---- leg_1i_chunk_ref_wrong_placement_length_still_decodes stdout ----
a ChunkRef whose placement length disagrees with its scheme's fragment count still decodes
(ADR-0045: placement length is contextual, not structural): ChunkSchemeUnsupported { chunk_id: 1, k: 2, m: 1 }
```

**Nine demonstrations, as the brief's Falsifiability field requires.**

### The three negations of THIS iteration's fix (the review finding, proven load-bearing)

| Negation | Result | Tests that went red |
|---|---|---|
| `nested-chunk-open` — the whole fix reverted (`chunks: Vec<ChunkRef>` again) | 24 passed; **3 failed** | all three new witnesses |
| `nested-scheme-open` — only the scheme mirror reverted | 26 passed; **1 failed** | `leg_1m_unknown_field_in_nested_scheme_is_rejected` |
| `nested-placement-default` — only `#[serde(default)]` restored on `placement` | 26 passed; **1 failed** | `part_chunk_omitted_placement_is_rejected` |

Verbatim, the two that matter most — note the failure prints **the exact bytes the permissive
decoder invents or destroys**, which is the defect class itself:

```
---- part_chunk_omitted_placement_is_rejected ----
assertion failed: decode->encode is not byte-identical for PartRecord { chunks: [ChunkRef { id: 1, scheme: None, len: 100, placement: [] }], … }
  left: "{\"chunks\":[{\"id\":1,\"scheme\":\"None\",\"len\":100,\"placement\":[]}],…}"   (what re-encoding produced)
 right: "{\"chunks\":[{\"id\":1,\"scheme\":\"None\",\"len\":100}],…}"                    (what the store held)

---- leg_1m_unknown_field_in_nested_chunk_is_rejected ----
  left: "{\"chunks\":[{\"id\":1,\"scheme\":\"None\",\"len\":100,\"placement\":[]}],…}"
 right: "{\"chunks\":[{\"id\":1,\"scheme\":\"None\",\"len\":100,\"placement\":[],\"bogus\":true}],…}"
```

### Three more, carried from iteration 2 and re-run on this tree

| Negation | Result | Tests that went red |
|---|---|---|
| `identity-omitted` — drop `skip_serializing_if` (re-ship iteration 1's defect) | 22 passed; **5 failed** | every session witness with an absent `content_type` |
| `identity-null-spelling` — `de_content_type` accepts `Option<String>` | 26 passed; **1 failed** | `session_null_content_type_spelling_is_rejected` |
| `1c-name-clause` — drop only `\|\| publish_target.name != wire.object` | 26 passed; **1 failed** | `leg_1c_publish_target_key_mismatch_is_rejected` |

Fourteen of the fifteen isolate to exactly one failing test (`identity-omitted` fails five by
construction, and `nested-chunk-open` three, because each removes a guard several witnesses
depend on).

---

## 4. The three forced questions

**(a) Genuine red?** Yes — fifteen times, each with one rule reverted from an otherwise complete
tree and the failure pasted above. The honest limit the brief pre-declares: with the *whole*
production hunk reverted the test does not compile (born-at-tier, criterion-absence red), which
is why the binding evidence is per-rule negation, not a whole-patch revert. Each negation
reverts a **single** rule, which is the stronger claim.

**(b) Production path?** Yes. Every witness is bytes → the production decoders exported from
`wyrd_core::multipart` (`decode_session_record`, `decode_slot_record`, `decode_part_record`,
`decode_part_summary`) **and** the store-wide `wyrd_core::metadata::decode::<T>`, asserted to
agree; every re-encode goes through the production `wyrd_core::metadata::encode`. No type is
constructed in the test (none has a constructor), no decoder is re-implemented, no mock exists.
`decode_both` (test:160) is the single funnel, generic over all four decoded record types.

**(c) Fixture includes the fault?** Yes. Every negation's witness is the torn value itself, built
to violate **only** its own rule: the 1i-scheme witness sets `len` to exactly its chunk's length
so leg 1k holds; the 1c witness keeps `name` and `epoch` agreeing; the 1c-epoch witness keeps
`parent`/`name` agreeing; the nested-chunk witnesses carry a well-formed `len`, a supported
scheme and an agreeing record length, so only the unknown-field / omitted-field rule can reject
them. Nothing is curated out.

---

## 5. Gates run locally, on the final tree

| Gate | Command | Result |
|---|---|---|
| Focused test | `cargo test -p wyrd-core --test multipart_session_records` | **27 passed; 0 failed** |
| C4-ci | `./engine/xtask.sh ci` (the driver's own runner) | **`xtask ci: all checks passed`** (exit 0) — incl. this file's 27 tests, `typos`, `lint_docs`, `render_site --check`, `fmt --check`, `clippy --all-targets -D warnings`, `cargo deny`, `cargo-machete`, the statics/unsafe/deploy guards and the madsim DST leg |
| C5-mutants | `PDCA_BUNDLE=… scripts/mutants-in-diff` | **56 mutants tested in 2m: 34 caught, 22 unviable — 0 missed** |
| Formatter (commit hook) | `cargo fmt --all --check` | clean |
| Intra-doc links | `cargo doc -p wyrd-core --no-deps` | **0 diagnostics in `multipart.rs`** (the two `private_intra_doc_links` my first draft added were removed; the pre-existing errors in `metadata.rs`/`read.rs`/`write.rs`/`erasure.rs` are on the base and are why `cargo doc` is not in `xtask ci`) |
| Patch applies | `git apply --check` in a throwaway worktree at `6151063` | clean, 3 files |

`typos` and `docs-renderer` are the load-bearing external deps for leg D's architecture-doc edit
(brief **External dependencies**); both are registered doctor ids and ran inside `xtask ci` above
with no findings. **No NEEDS-HUMAN external dependency was hit** — nothing beyond the base Rust
toolchain was needed.

---

## 6. Budget accounting — an honest overrun, all of it in the test file

| File | Added semantic lines (non-blank, non-comment) | Brief's estimate |
|---|---|---|
| `crates/core/src/multipart.rs` | **385** | ≈ 415 ✔ under |
| `crates/core/tests/multipart_session_records.rs` | **451** | ≈ 340 ✗ over by 111 |
| `docs/design/architecture/05-building-block-view.md` | **1** | ≈ 15 ✔ under |
| **total** | **837** | ≤ 770 — **over by 67 (+8.7 %)** |

Measured exactly as the round-1 and round-2 reviewers measured (726, then 766):
`git diff | grep '^+' | drop blank | drop lines starting '//'`.

The delta from iteration 2's 766 is **+71**, and it is entirely this iteration's mandated work:
**+34 production** (the two wire mirrors and their conversions) and **+37 test** (three witnesses
for the review finding, plus the whole-`ChunkRef` equality assertion that keeps the conversion
mutation-covered), **−4** from folding `part_summary_round_trips` onto the shared `decode_both`
funnel. The file count — the brief's actual STOP condition — is **exactly 3**, and the production
module came in **30 lines under** its own sub-estimate. I did not trim the overrun out of the test
file: every candidate line is a witness or an assertion some negation above turns red, and
removing evidence to hit a size estimate is the wrong trade. Flagging it here so C3 can judge it
with the arithmetic in hand rather than discover it.

---

## 7. Scope discipline

Untouched, per the brief's out-of-scope list: `Budget`/`AdmissionRecord` (#715, merged beneath —
this patch adds no line inside them, only new variants at the end of the shared `RecordError`),
`OwnedEntry`/`StagedPlacement`/retirement types/`PendingEntry` (#717), the outcome enums and
answer table (#693), knob values (#655), store round trips (#656–#659). No `docs/` file except
the single `05-building-block-view.md` paragraph (leg D), which this iteration extends by one
sentence covering the nested chunk closure — the persisted-shape fact this change adds
(`AGENTS.md:154-157`). No fourth file. The seven landed public types are unchanged from
iteration 2: `SessionRecord`, `SessionState`, `PublishTarget`, `Completion`, `SlotRecord`,
`PartRecord`, `PartSummary`; `ChunkRefWire`/`EcSchemeWire` are **private** wire mirrors and are
not additional landed types (brief **Scope**).

---

## 8. What the human still has to decide at sign-off

1. **The pre-declared born-at-tier red** (C2/C4). Criterion absence is the red oracle: with
   production reverted the test cannot compile. The brief pre-declares this; the fifteen
   per-rule negations in §3 are the substitute evidence.
2. **The three recorded-rejected review findings** (§2, `review-rejected.md`). If the human
   wants byte-canonicality enforced anyway, the honest shape is a `metadata::decode`-level
   decision for the whole store — a new issue against `metadata.rs`, not this bundle.
3. **The +67-line budget overrun** (§6), all of it test evidence, with the production module
   under its own estimate.
