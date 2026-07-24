# Serve bulk DeleteObjects (POST /bucket?delete)

## Summary
**User impact:** Deleting many objects at once did not work against the gateway. A
bulk-delete request came back as "not implemented," so the everyday cleanup commands
`aws s3 rm --recursive` and `aws s3 sync --delete` either slowed to one delete per
object or failed outright — there was no way to remove a set of objects in a single
call.

This PR implements bulk delete: the gateway now removes every requested object in one
call and reports the result per key (deleted, or an error for that key), while
matching S3's rule that deleting an already-absent key still counts as deleted.

Reported in #509.

## What to look at
The change is confined to the S3 gateway: a new handler for `POST /bucket?delete` and
the parsing of its XML request body. To try it, store a few objects and then run
`aws s3 rm s3://bucket --recursive` (or `delete_objects()` from any S3 SDK) against the
gateway — the objects should be gone and the response should list each key. To exercise
the safety path, send a malformed delete body and confirm it is rejected **without**
removing anything.

## Root cause
`POST /bucket?delete` fell through to the bucket-route subresource denylist, which lists
`delete`, so the request answered `501 NotImplemented` before any bulk-delete logic could
run — there was no DeleteObjects handler at all. The harder part is that this is a
destructive path: the request body has to be validated as well-formed XML before any
object is removed, and doing that validation by hand repeatedly let a malformed body slip
through and authorise a deletion it should have refused.

## Fix
`POST /bucket?delete` is now intercepted on the bucket route *ahead* of the subresource
denylist and routed to a new `delete_objects` handler; `delete` stays denied on object
paths, so `DELETE /b/k?delete` is still `501`. The signed body is buffered under a 2 MiB
cap and its digest verified exactly as the object PUT path does, then parsed with the
vetted `roxmltree` XML DOM parser: **any** body it does not accept as a well-formed
`<Delete>` document is `400 MalformedXML` and touches no key, so well-formedness is
established by construction rather than by a hand-maintained list of checks. Each `<Key>`
is read fail-closed — a comment, processing instruction, or child element inside it is
rejected rather than silently truncated — and used as the literal, already-entity-decoded
object name (never percent-decoded). Deletion fans out over the existing idempotent
single-object delete, and the `<DeleteResult>` is string-built to mirror the existing
listing responses, honouring `Quiet`.

## Verification
- **Claim:** A `delete_objects()` naming several keys — some present, at least one absent
  — returns `200` with each key reported once as `<Deleted>`, and the objects are then
  gone; `Quiet=true` omits the `<Deleted>` entries but still deletes.
- **Checked:** `crates/gateway-s3/src/lib.rs:1496` (the `POST` + `?delete` arm routes to
  the handler ahead of the denylist), deletion begins at `crates/gateway-s3/src/lib.rs:1882`.
- **Test:** `crates/server/tests/s3_delete_objects.rs:261`
  (`delete_objects_removes_present_and_absent_keys_idempotently`) and `:305`
  (`delete_objects_quiet_omits_deleted_entries_but_still_deletes`).

- **Claim:** A body that is not a well-formed `<Delete>` document is `400 MalformedXML`
  and deletes nothing — no malformed request authorises a deletion.
- **Checked:** the whole body is parsed before any effect at
  `crates/gateway-s3/src/lib.rs:1914`; a rejection returns before the first delete call at
  `crates/gateway-s3/src/lib.rs:1871`; a `<Key>` whose content is not pure character data
  (a comment, PI, or child element) fails closed at `crates/gateway-s3/src/lib.rs:1974`.
- **Test:** `crates/server/tests/s3_delete_objects.rs:428`
  (`delete_objects_comment_split_key_is_rejected_and_deletes_nothing`, the wrong-key
  discriminator) plus the malformed-body cases that each store a victim object and assert
  it survives — second root (`:473`), trailing content, junk after a tag name, duplicate
  attribute, `<`/bare-`&` in an attribute value, and a malformed processing instruction
  (`:536`).

- **Claim:** A `<Key>` is the literal object name — entity-decoded exactly once, never
  percent-decoded.
- **Checked:** the extracted value is used verbatim; percent-decoding is applied only to
  the URL path/query, not to a body key.
- **Test:** `crates/server/tests/s3_delete_objects.rs:362`
  (`delete_objects_literal_percent_key_is_not_percent_decoded` — `a%2Fb` deletes `a%2Fb`,
  the `a/b` decoy survives) and `:397`
  (`delete_objects_nested_entity_key_is_decoded_exactly_once` — `a&amp;amp;b` deletes the
  literal `a&amp;b`, the `a&b` decoy survives).

- **Claim:** Over-limit requests are refused before doing damage: more than 1000 keys, or
  a body past the buffered-size cap.
- **Checked:** the byte cap is enforced during buffering at
  `crates/gateway-s3/src/lib.rs:420`; the 1–1000 key bound at
  `crates/gateway-s3/src/lib.rs:1953`; exactly one `<Key>` per `<Object>` at
  `crates/gateway-s3/src/lib.rs:1925`.
- **Test:** `crates/server/tests/s3_delete_objects.rs:551`
  (`delete_objects_more_than_1000_keys_is_refused`) and `:575`
  (`delete_objects_oversized_body_is_refused_before_it_is_resident`).

- **Claim:** The new coverage is a genuine regression, not a green-only test.
- **Checked:** the whole suite (15 tests) is red on the current gateway — every
  `POST /bucket?delete` answers `501 NotImplemented`, so every assertion fails — and green
  with this change. `cargo xtask ci` (fmt, clippy `-D warnings`, build, test, `cargo deny
  check`, conformance) passes; the added dependency is `MIT OR Apache-2.0` (already on the
  `deny.toml` allowlist) and pulls in no new transitive crate.
- **Test:** `crates/server/tests/s3_delete_objects.rs` — the full wire suite, driven over a
  real loopback listener with a stock `aws-sdk-s3` client and raw SigV4-signed requests.

Fixes #509
