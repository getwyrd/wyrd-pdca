## Summary
**User impact:** none yet that a client can see — no production code writes multipart
upload sessions today. The risk this closes is for when they do: an upload that is
aborted, rolled back, or cleaned up after a restore would leave its staged data records
and its reservation marker in the metadata store forever, because nothing on the upload's
own record would say which records were its to delete. That is leaked metadata that no
cleanup pass could find.

This PR makes every multipart session record store the ID of the group its staged data
lives under, in every state from creation to the final delete, so later cleanup code can
always find and remove what the upload left behind.

## What to look at
- The session record gains one field, `segment_nonce`, and two read methods for it. The
  wire format is fixed here so follow-up work (#841, #842, #843, #810) can build on it:
  ```json
  {"parent":1,"object":"n","created_at_millis":1000,"clock_source":"wall","segment_nonce":"0123456789abcdef0123456789abcdef","epoch":3,"attempts":1,"state":{"kind":"Open"}}
  ```
- To see the change: on `main`, decode the record above with `decode_session_record`. It
  fails with `unknown field segment_nonce`. On this branch it decodes and re-encodes to the
  same bytes. Run `cargo test -p wyrd-core --test multipart_segment_nonce`.
- Most of the diff by line count is test fixtures (one field added to each existing
  session builder) and one long paragraph in the architecture doc.

## Root cause
The segment-group nonce is deliberately independent of the upload id, and the session
record had no field for it (`SessionRecordWire`, `crates/core/src/multipart.rs:2078-2088`
on `main`), so no reader of a session could name the `seg:` records or the `seggrp:`
marker the session owns. Proposal 0016 needs it at Create (`0016:508-518`, `:656`), while
`Completing` (`:665`, X57 at `:880`), across a rollback (`:538-552`) and at the terminal
delete from any state (`:673`).

## Fix
- `SessionRecordWire` and `SessionRecord` gain `segment_nonce: SegmentNonce` between
  `clock_source` and `epoch` (`crates/core/src/multipart.rs:2103-2104`, `:2179`). It
  decodes through the new `de_segment_nonce` (`multipart.rs:2082-2086`), which calls
  `SegmentNonce::new`, the same route `SegmentGroup`'s own `Deserialize` takes
  (`crates/core/src/metadata.rs:1060-1071`). `SegmentNonce` still has no `Deserialize`.
- The existing canonical-bytes check in `decode_session_record` (`multipart.rs:2321`)
  refuses the field in any other position, with no new code.
- Accessors: `SessionRecord::segment_nonce()` in every state (`multipart.rs:2215`), and
  `SessionRecord::attempt_segment_group()` (`multipart.rs:2225`), which returns
  `(nonce, publish_target.epoch)` for `Completing` and `None` otherwise. It is built with
  the new infallible `SegmentGroup::from_nonce` (`metadata.rs:1044`), so the nonce is
  never parsed twice.
- `publish_target` is unchanged. Doc comments that said its epoch "makes the nonce
  deterministic" now say it is the attempt's epoch within the session's group.
- Every full session fixture in the workspace gains the field (8 builders across
  `crates/core`, `crates/custodian` and `crates/dst` tests). The persisted-field sentence in
  `docs/design/architecture/05-building-block-view.md:202` names the field and notes that
  0016's `mpu:` row does not list it yet.

Design choice: the nonce is stored on the record, not derived from `(upload id, epoch)`,
because the code keeps it independent of the upload id on purpose. It sits on the record
itself rather than inside `publish_target`, because `Open` and `Aborting` records must
also name it, and a rollback drops `publish_target`.

## Verification
- **Claim:** a record carrying a valid nonce decodes, and re-encoding gives back the exact
  input bytes, in all four states (`Open`, `Completing`, `Aborting`, `Completed`).
  **Checked:** `crates/core/src/multipart.rs:2103-2104`, `:2179`, `:2301` on this branch.
  **Test:** `crates/core/tests/multipart_segment_nonce.rs`, leg (a).
- **Claim:** a record without the nonce is refused. **Test:** leg (b).
- **Claim:** a nonce that is not 32 lowercase hex characters (uppercase, 31 characters,
  containing `:`) is refused, and the error names the hex rule. **Checked:**
  `multipart.rs:2082-2086` (`de_segment_nonce`). **Test:** leg (c).
- **Claim:** the record names its nonce exactly once: every other field position, a
  repeated key, and a copy inside `publish_target` are all refused. **Checked:**
  `multipart.rs:2321` (canonical-bytes check). **Test:** leg (d).
- **Claim:** the two example records in this PR are the shared wire spelling, byte for
  byte. **Test:** `fixture_is_the_shared_wire_spelling`.
- **Claim:** every state exposes its nonce and can build its `seggrp:` key; only
  `Completing` has an attempt group, and it yields the `seg:<nonce>:<epoch>:` prefix.
  **Checked:** `multipart.rs:2215`, `:2225`; `metadata.rs:1044`. **Test:**
  `crates/core/tests/multipart_session_records.rs:841-900` (three tests).
- **Red/green:** the new file has 17 tests. With the production change reverted, all 17
  fail by assertion (`unknown field segment_nonce`). With it applied, all 17 pass.
- **Whole tree:** `cargo xtask ci` passes, including `typos`, the docs render check,
  clippy under `--cfg madsim`, and the DST custodian tests with the two edited helpers.

Fixes #840
