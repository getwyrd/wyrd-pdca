# Adversarial review — issue 686 (streamed GET moves the flat chunk list)

Bottom line: I tried to break the red→green proof, the no-copy claim, and the new resolver's
equivalence with the old one, and could not. What's left is one doc-comment cleanup the
builder can do, and one gate red that this diff did not cause.

## Refutation attempts that failed (the fix holds)

- **The red→green proof is real, even though C4-verify says UNVERIFIABLE.** C4-verify exits 77
  because the test imports `resolve_chunk_map_owned`, which does not exist on the base. The
  brief predicted this. I re-ran the red myself on a scratch copy of the patched tree. The new
  test passes as shipped (3/3). Then I forced three regressions into
  `crates/core/src/metadata.rs`, one at a time:
  (a) `chunks: chunks.to_vec()` in the flat arm (`:3148`) → red at
  `crates/core/tests/streamed_read_chunk_map_ownership.rs:137` (outer pointer differs);
  (b) cloning only each `placement`, while an in-place `collect` keeps the outer buffer → red
  at `:142` (first placement pointer differs). The outer-pointer check alone would miss this
  one; the placement check catches it;
  (c) framing a restart from the stale snapshot instead of the live root (`:3168-3171`) → red
  at `:198` (size 16 vs 12).
  The test calls the same function the server calls (`crates/server/src/lib.rs:432`), not a
  copy of it. The fixture has 3 chunks, each with a non-empty placement, so a copied empty
  `Vec` cannot pass by accident.
- **No hidden copy of the record (brief check ii): could not refute.** `resolve_chunk_map_owned`
  takes the record apart field by field at `crates/core/src/metadata.rs:3129-3137` and moves
  the flat list out at `:3141-3149`. There is no `.clone()`, `to_owned()`, `into_owned()` or
  `to_vec()` on that path. The only `into_owned()` calls (`:3169-3170`) are on the restart arm,
  where both `Cow`s are always `Owned` (`:3213-3214`), so they copy nothing new. On the server
  side, `lib.rs:446-455` moves the framing fields and `resolved.chunks` out by value. The
  `async move` in `stream::once` (`:469`) runs once and captures the list by move. The reader
  loop only borrows it (`:475`). Build-notes were withheld, so I checked the source directly.
- **Same answers as the borrowing resolver: could not refute.** I compared it arm by arm with
  `resolve_chunk_map` (`metadata.rs:3047-3060`): flat (no read), segmented (the same
  `read_segments` and the extracted `assemble_segments`, `:3020-3026`), superseded (the same
  `resolve_current_chunk_map`), and gone (`Ok(None)`). The `InodeRecord` destructure has no
  `..`, so a future new field breaks the build instead of being dropped silently. The caller
  passes only `Committed` inodes (`crates/core/src/read.rs:588`), the same as before.
- **Server wiring is checked by reading the code, not by a test.** No test fails if
  `lib.rs:432` goes back to `resolve_chunk_map(&inode)` plus `into_owned()`. The brief's
  verification posture accepts this (there's no way to count allocations without `unsafe` or a
  new dependency), and the current code does call the owning entry. Not raised.
- **Gone arm not tested** (`metadata.rs:3174`, the one line diff-cov missed). This is low value.
  A wrong arm there would usually still return `Ok(None)` through `resolve_current_chunk_map`
  reading the absent root. Noted, not routed.
- **Out of scope, not raised:** on restart, a flat successor's list is still copied at
  `metadata.rs:3213`, and `framed_by` then drops the original (`:3099`). The brief's Scope
  explicitly excludes this.

## Findings

- NEEDS-HUMAN [impl] — Three nearby doc comments now say something the code no longer does.
  That is the same defect class as #686 itself, which was a comment claiming a move while the
  code made a copy.
  (1) `crates/core/src/metadata.rs:3028-3030`: `resolve_chunk_map` still calls itself "the
  entry for … the gateway's streaming and ranged reads". The streaming read now uses
  `resolve_chunk_map_owned` (`crates/server/src/lib.rs:432`).
  (2) `metadata.rs:1617-1620`: `InodeRecord::chunk_map` says `resolve_chunk_map` /
  `resolve_current_chunk_map` "is the ONE way" to resolve the map, and does not mention the
  new public entry.
  (3) `metadata.rs:1267-1270`: `ChunkMap::into_flat` names "a streamed read moving the list
  into its reader task" as its consumer. That read now destructures the record inside
  `resolve_chunk_map_owned` and never calls `into_flat`, which still has no caller.
  Fix: update the three comments, either deleting `into_flat` or dropping the consumer claim
  from its comment. Optionally, adjust "one call" in
  `docs/design/architecture/06-runtime-view.md:29`; the logic is still shared, but there are
  now two public entries.
- NEEDS-HUMAN [human] — The gating `C4-ci` red is not caused by this diff. It fails at
  `cargo deny --all-features --config deny-all-features.toml check advisories`, on
  RUSTSEC-2026-0346 and RUSTSEC-2026-0349 against `rustls 0.21.12`, which only
  `tikv-client 0.4.0` pulls in (`gate-logs/C4-ci.log:3470-3548`). The patch changes none of
  `Cargo.lock`, `Cargo.toml`, `deny.toml` or `deny-all-features.toml`. This is the same chain
  as the existing #543 ignore entries (`deny-all-features.toml:103-152`), so `main` fails the
  same way. Because `cargo xtask ci` stops at the first failure (`xtask/src/main.rs:1683-1687`),
  the conformance, statics, deploy-guard and DST steps never ran in this gate. I ran them on
  the patched tree: `xtask statics` passed, `xtask conformance` passed (5 valid + 6 invalid
  vectors), and `xtask dst` passed (exit 0; custodian 33/33, all other suites green). The
  deploy guard can't trip because the diff adds no imports. A human has to decide whether to
  waive this red for #686 or land the new advisory ignores on `main` under #543 first. Either
  way that is out of this diff's scope.

<!-- pdca:leaf-complete -->
