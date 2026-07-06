# Adversarial review — issue 364 / s3-http-wire-surface (iteration 5)

Advisory only; I never gate. Every `path:line` is grounded on the target source at
`$PDCA_TARGET` (= `/home/eddie/wyrd/wyrd.pdca-wt-l1`, patch applied). Scope: this diff.

## Findings

- **NEEDS-HUMAN — PUT overwrite leaks the prior object's fragments *permanently* — the very
  leak the DELETE path was rebuilt to close, now wide open on the more common verb.**
  `crates/server/src/lib.rs:182-202` (`commit_written`) routes an overwrite of an existing key
  to `write::commit_overwrite` (`crates/core/src/write.rs:265-272`), which only CAS-swaps the
  new chunk map onto the inode (`metadata::commit_chunk_map`). Unlike `delete_object`
  (`lib.rs:232-294`), the overwrite path **never** calls `reclaim_fragments` and **never**
  writes an orphan grace record — orphan records are written *only* in `metadata::unlink`
  (`crates/core/src/metadata.rs:394-411`). The old inode's committed fragments therefore carry
  no `pending:` lease and no `orphan:` record, so the custodian GC (which scans only those two
  key spaces) can never reclaim them. Concrete case: `PUT /b/k` (object A) then `PUT /b/k`
  (object B) — after the second commit, A's fragment bytes are stranded on every D-server
  forever, on the happy path, no crash required. This is *worse* than the crash-only DELETE
  leak the last three iterations fixed, and there is **no test** for it (the suite tests DELETE
  reclaim exhaustively — `crates/server/tests/s3_http_wire.rs:372,816` — but no overwrite-reclaim
  test exists). The reviewer's "does not leak fragment bytes" narrative (lib.rs:220-231) is
  scoped to DELETE and silently untrue for overwrite.

- **NEEDS-HUMAN — GET-during-DELETE truncates a streaming read; the docstring's "reader-safe
  grace window" is not honored on the happy path.** `delete_object`'s eager reclaim
  (`crates/server/src/lib.rs:245`, `reclaim_fragments` → `delete_fragment_at` at
  `lib.rs:285`) deletes the object's fragments **immediately**, not after any grace window.
  A concurrent `get_object_streaming` (`lib.rs:312-334`) resolves the chunk map up front and
  then reads fragments lazily on a spawned task (`lib.rs:324-332`); if the DELETE lands
  mid-stream, `read::read_chunk_verified` raises `MissingFragment`
  (`crates/core/src/read.rs:145,172`) and the reader task breaks, sending the client a
  truncated body. The GET response sets **no `Content-Length`** (`s3/mod.rs:255-259`) and has
  already emitted `200 OK`, so the client cannot cleanly distinguish truncation from success.
  For a single-chunk object the window truncates to zero bytes. This directly undercuts the
  binding "byte-identical round-trip" criterion under concurrent access. The docstrings claim
  a "reader-safe grace window" (`lib.rs:230-231`, `metadata.rs:356-358`) but that window is
  honored only by the crash/GC backstop — the happy-path reclaim ignores it. This is the exact
  concern iteration 4 left as an open "decide" item; it appears unaddressed.

- **NEEDS-HUMAN — "Real-SDK / stock-SDK interop" is still asserted, not demonstrated
  end-to-end.** The over-the-wire round-trip and streaming tests sign with the gateway's *own*
  `sigv4::sign` / `sign_with_payload_hash` and frame with the gateway's own helper
  (`crates/server/tests/s3_http_wire.rs:598,633`, comment at `:21`) — no real boto3/aws-sdk
  process ever hits the listener. The only independent oracles are unit KATs
  (`s3/sigv4.rs:629,659`; `s3/streaming.rs:278`), which pin the signing *math* but not the wire
  framing/header set a live SDK emits. So "a stock SDK upload round-trips instead of 501-ing"
  (mod.rs:19-23, streaming.rs:1-5) is verified only against the gateway's model of an SDK, not
  an SDK. This is the recurring carry-forward (iterations 2-4); it should be ratified as scoped
  or backed by a genuine SDK path, not accepted as "proven."

- Minor (fail-closed, not a hole): `verify` trims but does not collapse sequential internal
  whitespace in signed header values (`s3/sigv4.rs:403`), and re-sorts `SignedHeaders`
  (`sigv4.rs:384-385,405`). A real client that signs a header carrying doubled internal spaces,
  or sends `SignedHeaders` in a non-sorted order, would 403 — a spurious reject that further
  erodes the "real SDK compatibility" claim, though it never *weakens* auth.

## Attempted but could not refute

- **Percent-decode off-by-one** (`s3/mod.rs:298`, `s3/sigv4.rs:171`): the `i + 2 < len` guard
  is correct — a valid `%XX` at the very end of the segment still decodes. No boundary bug.
- **SigV4 canonicalization / signature math**: attacked query sorting, URI-encoding, and the
  signing-key ladder; the AWS `get-vanilla`, docs query-sort, and published streaming KATs
  (`sigv4.rs:629,659`; `streaming.rs:278`) are genuine independent oracles. Could not break the
  signing chain.
- **XML error injection** (`s3/mod.rs:317-344`): `xml_escape` covers all five predefined
  entities and is applied to both `<Code>` and `<Message>`. Could not inject markup.
- **DELETE idempotency under the CAS race** (`lib.rs:232-262`): the retry + re-resolve loop
  makes two concurrent DELETEs both succeed and bounds a pathological overwrite storm. Could
  not force a non-idempotent 409.
- **Auth-before-body / pre-auth amplification** (`s3/mod.rs:187-217`): `sigv4::verify` runs and
  must succeed before `body.into_data_stream()` is touched. Could not force a body allocation on
  an unsigned request.
