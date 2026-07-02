## Summary

If a committed chunk's placement record ever became truncated or corrupted (a
non-empty list of the wrong length), the background durability loops treated it
as if it were normal — which could silently reclaim a real fragment of that
chunk (data loss), rewrite the corrupt record over an invented placement, and
let the corruption pass unnoticed by operators. This change makes the four
maintenance loops **reject** a wrong-length placement instead of guessing at the
missing entries: they fail safe and raise an operator signal rather than acting
on fabricated data.

## What to look at

- **The classifier** — `crates/core/src/metadata.rs`: `checked_fragments()` /
  `placement_is_valid()`. A committed placement is valid only if it is empty
  (legacy records, resolved by the identity fallback) or exactly
  `fragment_count()` long; any other non-empty length is malformed. This is the
  strict counterpart to the existing liberal `fragments()` expansion, which the
  read path keeps using untouched.
- **The four call sites that now route through it** — `gc.rs`, `scrub.rs`,
  `reconstruction.rs`, `rebalance.rs`. Each classifies the committed placement
  *before* expanding it.
- **Exercise it:** `cargo test -p wyrd-core -p wyrd-custodian`. To see the old
  behaviour go red, revert any loop's expansion back to `chunk.fragments()`.

## Root cause

All four loops expanded a committed chunk's placement through the liberal
`ChunkRef::fragments()` helper, which applies the identity fallback
unconditionally and never checks the vector's length. A malformed (non-empty,
wrong-length) placement — which today can only mean truncation or corruption, as
no writer emits a short non-empty vector — was therefore identity-filled for its
missing tail rather than rejected, so every loop acted on invented placement.

## Fix

Add a single shared classifier in `wyrd-core` (`checked_fragments()`,
`placement_is_valid()`, `MalformedPlacement`) and route every maintenance loop
through it before expansion:

- **GC / scrub fail safe:** a malformed chunk is treated as fully referenced —
  none of its fragments is ever reclaimed and scrub enqueues no phantom repair —
  and each is surfaced as an audit event on the durability seam.
- **Reconstruction / rebalance skip + flag for a human:** the chunk is left
  exactly as committed (never repointed over a fabricated placement) and raised
  as needs-human.
- **Drain status is attributed:** when a malformed chunk blocks a
  drain/decommission cluster-wide, the status now names the blocking chunk ids
  instead of an unexplained "pending".

The read path is deliberately left liberal and unchanged, so a malformed-placement
chunk still reads via the per-index identity fallback (availability first).

## Verification

- **Claim:** a wrong-length committed placement is classified as malformed
  before any expansion, in one shared place.
  - **Checked:** `crates/core/src/metadata.rs:159-199` — `placement_is_valid()` /
    `checked_fragments()` / `MalformedPlacement`.
  - **Test:** `crates/core/src/metadata.rs:378` (`RS{4,2}`, length-2 vector →
    malformed, rejected before expansion).

- **Claim:** GC and scrub never reclaim a fragment of a malformed chunk and emit
  an operator signal.
  - **Checked:** `crates/custodian/src/gc.rs:202-219` (the shared `ReferenceSet`
    with `protects()`) and `gc.rs:313` (audit emitter); `scrub.rs:81` (fail-safe
    branch) and `scrub.rs:208` (audit emitter).
  - **Test:** `crates/custodian/tests/gc.rs:642` (the chunk's real fragment
    survives) and `crates/custodian/tests/scrub.rs:761` (no phantom repair
    enqueued). Both fail pre-fix, pass post-fix.

- **Claim:** reconstruction and rebalance skip a malformed chunk and never
  rewrite the committed record.
  - **Checked:** `crates/custodian/src/reconstruction.rs:240-242` and `:525`;
    `crates/custodian/src/rebalance.rs:167` and `:346`.
  - **Test:** `crates/custodian/tests/reconstruction.rs:609` (record untouched,
    obligation stays queued) and `crates/custodian/tests/rebalance.rs:1249`
    (placement unchanged, fragment not evacuated). Both fail pre-fix, pass
    post-fix.

- **Claim:** a drain stalled by malformed placement reports the blocking chunk
  ids rather than a bare "pending".
  - **Checked:** `crates/custodian/src/desired_state.rs:150-179`
    (`PendingMalformed { chunks }`, ids sorted).
  - **Test:** `crates/custodian/tests/rebalance.rs:1325`.

- **Claim:** the read path is unchanged and still resolves a malformed-placement
  chunk.
  - **Checked:** `crates/core/src/read.rs:104` still resolves via
    `placed_dserver` (this change does not touch `read.rs`).
  - **Test:** `crates/core/src/metadata.rs:393`
    (`read_path_fragments_stays_liberal_for_malformed_placement`).

Whole-tree gate: `cargo xtask ci` (fmt, clippy `-D warnings`, build, test,
conformance) is green on the target branch with the change applied.

Fixes [#348](https://github.com/getwyrd/wyrd/issues/348)
