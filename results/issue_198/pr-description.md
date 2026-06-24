# PR description

## Summary

A read could silently return the wrong data. If the storage layer handed back a
fragment that was internally intact (its checksum verified) but actually belonged
to a *different* chunk — a misrouted or placement-confused fragment — the read path
accepted it on the checksum alone. For an unencoded chunk it returned that foreign
fragment's bytes as the answer; for an erasure-coded chunk it fed the foreign shard
into the decoder, producing silently corrupt reconstructed data. In both cases the
caller got no error. This change makes the read path verify that an accepted
fragment actually belongs to the chunk being read, so a misplaced fragment is
rejected instead of trusted.

## What to look at

- `crates/core/src/read.rs`, function `read_chunk` — the two places that turn a
  decoded fragment into output: the `EcScheme::None` (single-fragment) arm and the
  `EcScheme::ReedSolomon` survivor loop. Each previously accepted any fragment that
  decoded; each now additionally requires `decoded.header.chunk_id == chunk.id`.
- For comparison, `crates/core/src/repair.rs` — `fragment_intact` and `intact_shard`
  are the shared verify used by scrub and reconstruction; the read path now applies
  the same predicate they do.
- To exercise it: store a valid fragment whose header names a different chunk at an
  index the read will fetch, then read the target chunk. See the two regression
  tests in `crates/core/tests/read_repair.rs`.

## Root cause

Both fragment-acceptance sites in `read_chunk` accepted a decoded fragment on a bare
`Ok` from `decode(...)`, checking only the self-describing checksum and never the
`header.chunk_id`. Scrub (`fragment_intact`) and reconstruction (`intact_shard`)
gate on *decodes-cleanly AND header names the requested chunk*; the read path was the
sole consumer that omitted the second half of that gate, so a misplaced-but-intact
fragment passed through.

## Fix

Add the `decoded.header.chunk_id == chunk.id` guard to the accepting arm at both read
sites:

- `EcScheme::None`: the fragment is returned only when its header names the chunk; a
  misplaced-but-intact fragment is treated as no usable fragment present — the chunk
  is recorded for repair (existing plumbing) and the read surfaces a missing-fragment
  error rather than returning foreign bytes.
- `EcScheme::ReedSolomon`: a survivor is admitted to the decoder only when its header
  names the chunk; a misplaced one is excluded and read around exactly as a missing
  or checksum-failing fragment is, and the chunk is enqueued for repair. With fewer
  than `k` genuine survivors the read fails on insufficient fragments instead of
  decoding from a foreign shard.

The on-disk fragment format, the shared `repair` verify, and the corrupt-fragment
error path are all left unchanged.

## Verification

- **Claim:** A read backed by a misplaced-but-intact fragment (valid checksum, wrong
  `chunk_id`) does not admit that fragment — unencoded reads do not return the foreign
  payload, and erasure reads treat the foreign fragment as absent rather than feeding
  it to the decoder.
- **Checked:** `crates/core/src/read.rs:139` (`EcScheme::None` acceptance) and
  `crates/core/src/read.rs:176` (ReedSolomon survivor acceptance) on `main` — both now
  require `decoded.header.chunk_id == chunk.id`, the same predicate as
  `crates/core/src/repair.rs:53-55` (`fragment_intact`) and
  `crates/core/src/repair.rs:66-71` (`intact_shard`), making the read path consistent
  with the verify whose doc comment (`crates/core/src/repair.rs:50-51`) already claims
  the read path decodes "for the same effect inline."
- **Test:** `crates/core/tests/read_repair.rs` — two added tests fail before the fix,
  pass after:
  - `none_read_rejects_a_misplaced_but_intact_fragment` — pre-fix returns the foreign
    payload (`Ok(Some("another chunk's bytes!"))`); post-fix the read errors.
  - `ec_read_treats_a_misplaced_but_intact_fragment_as_absent` — RS(2,1) with one
    genuine survivor and one misplaced shard; pre-fix the decoder runs on the foreign
    shard and returns corrupt bytes (correct prefix, then `0xFF` garbage), post-fix the
    misplaced shard is excluded, leaving fewer than `k` survivors so the read errors.

Fixes #198
