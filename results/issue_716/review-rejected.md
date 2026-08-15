# Recorded-rejected review findings — issue 716
#
# Format (ONE physical line per rejection, as `scripts/review-branch` parses it):
#   `<file:line>` | <CLASS> | <MATCH> | <reason>
# MATCH is a case-insensitive substring of the finding's own rationale, so a *different* defect
# landing at the same line still blocks. `loc` must match EXACTLY, so this round's rebuild
# records each finding at BOTH the line the previous round reported and the line the equivalent
# code now sits on.
#
# CITATION FRAME: every `crates/core/src/multipart.rs:NNNN` below is a line of the PATCHED file
# (the tree the review reads), matching the frame the findings themselves are cited in. Lines in
# other files (`metadata.rs`, `AGENTS.md`) are base lines — those files are untouched here,
# except `metadata.rs` which this patch does not edit at all.
#
# ---------------------------------------------------------------------------------------------
# Round 3 (this attempt). Round 2's `review-batch.md` carried FOUR findings, all of one class:
# "the decoder accepts a JSON spelling whose re-encode is not the bytes read, and the record is
# CAS'd whole". They split cleanly in two, and the split is the whole of this decision:
#
#   * `multipart.rs:1882` — the nested `ChunkRef`: a chunk with `placement` OMITTED decodes
#     (`#[serde(default)]`, `metadata.rs:138`) and re-encodes as `"placement":[]`. That is a
#     FIELD-level rewrite: information the stored bytes did not carry is fabricated on read.
#     It is FIXED in code, not rejected — `part:` chunks are now read through the module's own
#     closed `ChunkRefWire`/`EcSchemeWire` (`crates/core/src/multipart.rs:1914`, `:1939`), so an
#     omitted `placement` and an unknown field inside a chunk (or inside its `scheme`) are typed
#     decode rejections. Bound by three new witnesses at
#     `crates/core/tests/multipart_session_records.rs:576`, `:589`, `:615`, each shown red under
#     its own isolating negation in `build-notes.md` §3.
#
#   * `:1684`, `:1745`, `:1746` — reordered / whitespace-bearing / escape-equivalent JSON.
#     Recorded-rejected below. This is JSON's own syntactic freedom, not a field the record
#     rewrites: the decoded value is the same value, no field is added, dropped or re-spelled,
#     and the re-encode is that same value in this codec's canonical spelling.
# ---------------------------------------------------------------------------------------------
#
# The reason, once, for every line below (each row carries the short form):
#
# (1) It is settled on this base by the peer record, in this same module, merged one child
#     earlier. `AdmissionRecord`'s doc states it in as many words: "decode is not a
#     canonicalisation check. A foreign spelling of the same value — fields reordered,
#     whitespace inserted — still decodes, to the same value, and re-encodes in this codec's
#     spelling rather than in its own; JSON, not this record, is what makes those spellings
#     equal" (`crates/core/src/multipart.rs:1367-1373`, #715, merged as `5eeca16`). Ruling the
#     opposite way for the four record types beside it would put two canonicality policies in
#     one codec.
#
# (2) The check cannot live where this child's rules live. Every structural rule here is
#     enforced INSIDE `Deserialize` (the brief's binding shape, and `metadata.rs:1377`,
#     `:1240-1246`), and a `Deserialize` impl never sees the input bytes — only the parsed
#     token stream. A byte-canonicality check can therefore only sit in the per-record wrapper
#     (`decode_session_record` and its three peers), which would give the module TWO decoders
#     with DIFFERENT accepted sets: `metadata::decode::<SessionRecord>` liberal, the attributed
#     wrapper strict. Two spellings of one decision is the exact fault this module refuses
#     everywhere else (its own C-1 argument, `crates/core/src/multipart.rs:52-58`), and the
#     test asserts the two surfaces agree on every witness
#     (`crates/core/tests/multipart_session_records.rs:167-174`).
#
# (3) The hazard has no producer. Non-canonical bytes can only enter the store from a writer
#     that does not use `metadata::encode` — `serde_json::to_vec`, compact and in declaration
#     order (`crates/core/src/metadata.rs:1564-1566`) — and there is no such writer: these
#     records have no writer at all yet (#656–#659 land the first, through that same codec).
#     The FIELD-level half, which a legitimate writer CAN produce (an omitted `placement` from a
#     `ChunkRef` written elsewhere in the tree), is the half that was fixed.
#
# (4) No record decoder in this repo does it. `decode_segment_record` — the peer this module's
#     decoders are modelled on — is a bare `decode::<SegmentRecord>` with the error attributed
#     (`crates/core/src/metadata.rs:2536-2547`), and `InodeRecord`, `SegmentRecord`,
#     `PendingEntry` and `DirentRecord` are all decoded the same way, under the same two CAS
#     shapes. If store-wide byte canonicality is wanted it belongs at `metadata::decode`, one
#     decision for every namespace — an edit to `metadata.rs`, which this child's scope pins
#     untouched (brief **Scope**: "every file outside `multipart.rs` + the new test").
#
# (5) The rubric rule this class cites prescribes a different remedy than the finding asks for:
#     "optional/legacy fields are omitted when absent, never emitted as defaults — decode→encode
#     must be byte-identical wherever a compare-and-swap or content hash depends on it (add the
#     round-trip test)" (`AGENTS.md:170-172`). Both halves are honoured: no field of these
#     records is defaulted, skipped-on-read or re-spelled, and the round-trip test is asserted on
#     EVERY accepted witness in the file, not test by test
#     (`crates/core/tests/multipart_session_records.rs:175-181`).
#
# What this leaves standing, stated so the human can overrule it in one place: a `mpu:`/`slot:`/
# `part:`/`psum:` value hand-written into the store by an operator tool that does not use
# `metadata::encode` would decode and re-encode in the codec's spelling. Under
# `require(key, encode(prior))` that is a `Conflict` (loud, no data lost); under
# `require(key, current)` the CAS matches the raw bytes it read, so the rewrite is to the
# canonical spelling of the same value. Neither loses a field. If that residue is judged
# unacceptable it is a `metadata::decode`-level decision for the whole store, and the right
# shape is an issue against `metadata.rs`, not four canonicality checks in one module.

`crates/core/src/multipart.rs:1684` | BUG | whitespace | JSON syntactic freedom, not a field rewrite: the same value decodes and re-encodes in this codec's canonical spelling, no field added/dropped/re-spelled. Settled for the peer record one child earlier (`multipart.rs:1367-1373`); unenforceable inside `Deserialize` (which never sees the bytes) without giving the module two decoders with different accepted sets; no writer can produce non-canonical bytes (`metadata::encode` is the only encoder, `metadata.rs:1564-1566`); no record decoder in this repo checks it (`decode_segment_record`, `metadata.rs:2536-2547`). The field-level half of the same round's findings (the nested `ChunkRef`) was FIXED, not rejected. Full reasoning in the header above.

`crates/core/src/multipart.rs:1745` | BUG | whitespace | Same finding, recorded at the second line the previous round reported it on — see the entry for `multipart.rs:1684`.

`crates/core/src/multipart.rs:1746` | CONVENTION | whitespace | Same finding in its CONVENTION spelling (`AGENTS.md:170-172` serialization identity). Both halves of that rule are honoured: no field is defaulted, skipped-on-read or re-spelled, and the round-trip test is asserted on every accepted witness (`crates/core/tests/multipart_session_records.rs:175-181`). See the entry for `multipart.rs:1684`.

`crates/core/src/multipart.rs:1744` | BUG | whitespace | Same finding, recorded at the line `decode_session_record` occupies after this rebuild — see the entry for `multipart.rs:1684`.

`crates/core/src/multipart.rs:1746` | BUG | whitespace | Same finding, recorded at the line the session decode's `metadata::decode` call occupies after this rebuild — see the entry for `multipart.rs:1684`.

`crates/core/src/multipart.rs:1833` | BUG | whitespace | Same finding at `decode_slot_record`, the same shape one record over — see the entry for `multipart.rs:1684`.

`crates/core/src/multipart.rs:2063` | BUG | whitespace | Same finding at `decode_part_record` — see the entry for `multipart.rs:1684`.

`crates/core/src/multipart.rs:2094` | BUG | whitespace | Same finding at `decode_part_summary` — see the entry for `multipart.rs:1684`.
