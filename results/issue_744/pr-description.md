## Summary
**User impact:** `wyrd-validate` is meant to run against a Wyrd deployment for
weeks or months and prove that what it wrote comes back intact. Until now it had
no way to decide what data to write or at which object sizes. Without this, a
long run would either have to keep a copy of every object it uploaded, or reuse
the same contents when it overwrites an object, in which case reading back the
*old* version would look correct. And a list of test sizes copied from today's
settings would quietly stop testing the right limits once those settings
changed. Nothing runs validation scenarios yet, so no report has been wrong; the
scenarios that come next need this in place first.

This PR makes every object's contents reproducible from a few recorded inputs
(seed, key, size, and which write of that key it is), and computes the list of
object sizes to test from the deployment's own settings, given as three new
optional flags.

**Order of merging:** it builds on the merged `wyrd-validate` crate and S3
client. #853 adds a dependency line at the same spot in
`crates/validate/Cargo.toml`, so whichever of the two lands second needs a
trivial rebase. #743 builds on this one.

## What to look at
- **How object contents are made.** The module doc at the top of
  `crates/validate/src/payload.rs` states the exact construction, so it can be
  reimplemented, and the one limit: at 0 bytes, and for very small objects,
  two versions of the same object can have the same contents, so a read-back
  check cannot tell them apart by contents alone.
- **Which sizes get tested.** `crates/validate/src/sizes.rs` turns the three
  settings into the size list, each size labelled with the limit it sits on.
- **The new flags.** `--chunk-size`, `--max-root-value-bytes` and
  `--segment-threshold-bytes`. All optional; when left out each takes today's
  server value, and the startup echo says whether each value was given,
  defaulted or derived. Settings no deployment can have are refused by flag
  name rather than silently adjusted.
- **Docs.** The `validate` paragraph of
  `docs/design/architecture/05-building-block-view.md` is updated to match.

To try it: `cargo test -p wyrd-validate --test payload_and_sizes` (26 tests,
in-process, no network or containers). Or run the binary with the ten required
flags, add or leave out e.g. `--chunk-size 2097152`, and read the echoed
configuration.

## Root cause
The crate had an argument surface and an S3 client but no payload or size
logic. Proposal 0017 §4 requires both to be derived: bytes from inputs the run
records, so nothing is stored and an overwrite always sends new bytes; sizes
from the deployment's tuning, because Wyrd's own limits (the chunk size `C` and
the chunk-map segmentation threshold `T`) depend on values a blackbox tool must
not import.

## Fix
- **Payloads** (`crates/validate/src/payload.rs`, new): `Payload::new(seed, key,
  size, generation)` (`:120`). Block `i` is `SHA-256(prefix || u64(i))`, where
  the prefix length-prefixes every input (`:11-31`), so any range `[a, b)` costs
  about `(b − a) / 32` hashes wherever it sits, and the state held is one
  SHA-256 midstate. `fill` (`:156`), `range` (`:186`) and `pieces` (`:202`)
  stream in pieces of at most `PIECE_BYTES` = 64 KiB (`:58`); `sha256` (`:212`)
  gives the streaming digest the oracle and checkpoints will key on. The
  small-size limit is documented at `:33-43`.
- **Size classes** (`crates/validate/src/sizes.rs`, new): `Tuning::resolve`
  (`:267`) turns the flags into values tagged with their source;
  `derived_segment_threshold` (`:227`) computes `T = ⌊budget / 315⌋ × C`;
  `size_classes` (`:343`) builds the ordered list and merges sizes that
  coincide into one entry with both labels. All arithmetic is checked. Refused,
  never clamped (`TuningError`, `:137`): a chunk size outside 1–16 MiB (the
  range the server's `s3` role accepts), a root budget of 0 or above 50,000
  (`check_root_budget`, `:253-261`; 50,000 is half of FoundationDB's
  100,000-byte value limit), a given threshold below one chunk or not a whole
  number of chunks, a budget too small to derive from when no threshold is
  given, and any size that overflows 64 bits.
- **Flags and echo**: `TUNING_FLAGS` (`crates/validate/src/args.rs:47-51`) is a
  separate optional set, so `FLAGS` stays the ten required flags; parsing is as
  strict as for the existing flags. `ResolvedConfig::render`
  (`crates/validate/src/lib.rs:69-80`) prints `--flag = value (given|default|derived)`.
- **Dependency**: `sha2` (already a workspace dependency used by `wyrd-core` and
  the S3 gateway) added to `crates/validate/Cargo.toml:37`. No new crate in the
  graph; `wyrd-core` and `wyrd-server` stay dev-dependencies only.

## Verification
All `path:line` below are on this branch (base `main` @ `4bac538`).

- **Claim:** the same inputs give the same bytes in every process.
  **Checked:** `crates/validate/tests/payload_and_sizes.rs:204` compares digests
  with hex constants computed by a separate process; a generator that read any
  process state would fail it.
- **Claim:** changing seed, key, size or generation changes the bytes,
  including two writes of one key at the same size; and the 0-byte case is
  stated, not hidden.
  **Checked:** `:245` (4 KiB and 100,003 B vectors), `:281` (0 B).
- **Claim:** exact length, and any range produced directly equals the same
  slice of the whole.
  **Checked:** `:295`, `:307`, `:328`; out-of-range requests are refused, not
  truncated (`:374`).
- **Claim:** state stays bounded and ranges are computed directly, even at huge
  sizes. **Checked:** `:435` (every piece ≤ 64 KiB), `:497` (the last 64 bytes
  of a 10 GiB + 1 and a 1 TiB payload, each within 5 s and pinned to a known
  digest). Every test body runs under a time bound (`:90`), so a regression
  fails by name instead of hanging the suite.
- **Claim:** the size list comes from the tuning, not a fixed table.
  **Checked:** `:614` (default table), `:689` (labels), `:711` (a different
  chunk size moves only the `C`, `64×C` and `T` sizes), `:735` (budget 31,500
  moves `T` from 158 MiB to 100 MiB and nothing else), `:764` (a given threshold
  is used exactly), `:806` (coinciding sizes appear once with both labels).
- **Claim:** bad settings are refused by flag name, never clamped.
  **Checked:** `:851` (library API), `:1110` and `:1163` (command line,
  including `--max-root-value-bytes 50001` and a zero budget beside a given
  threshold).
- **Claim:** the flags are optional, echoed with their source, and the ten
  existing flags are unchanged. **Checked:** `:1002`, `:1029`, `:1262`, `:1274`
  (the real binary); `crates/validate/tests/cli_surface.rs` passes unmodified
  (21/21).
- **Claim:** the defaults cannot drift from the server's values.
  **Checked:** `:1305` (root budget, its cap, and the 315-byte record width
  against `wyrd_core` constants), `:1379` (chunk size, read off a default
  in-process gateway's single-PUT limit), `:1454` (chunk-size range, read off
  the server's own refusal). Lowering `MAX_ROOT_VALUE_BYTES` in `wyrd-core`
  turns these red.
- **Test:** `crates/validate/tests/payload_and_sizes.rs` (new). This adds new
  API, so the test cannot compile without the change; instead, each guarantee
  was shown to fail under a deliberate fault: dropping the generation from the
  hash, hardcoding the chunk size or the root budget, shifting a range by one
  byte, regenerating the prefix before a range (fails on the 5 s bound), and
  dropping the source from the echo. `cargo mutants` on the diff: 107 mutants,
  96 caught, 11 unviable, none missed. `cargo xtask ci` passes, including
  `blackbox-guard` (the binary still links no `wyrd-*` crate).

Fixes #744
