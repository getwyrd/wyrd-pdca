# Brief — issue 290 / no-preallocate-from-untrusted-inode-size

> The Plan artifact (docs 02 §PLAN). Human-authored. Do reads ONLY this file.

- **Slug:** no-preallocate-from-untrusted-inode-size
- **Defect:** `read_object_collecting` allocates its output buffer with
  `Vec::with_capacity(inode.size as usize)` (`crates/core/src/read.rs:79`) *before* it has
  validated that the chunk map can produce that many bytes. The reassembled-vs-recorded size
  check happens only afterward (`read.rs:83-89`), and the metadata codec accepts arbitrary
  `u64` sizes from stored JSON. A corrupt/tampered inode with `size` set to a huge value (e.g.
  `u64::MAX`) turns an ordinary read into a capacity-overflow panic or an excessive/OOM
  allocation instead of a clean `ReadError::SizeMismatch` / validation error.
- **Success criterion:** Reading a committed inode with a wildly oversized `inode.size` (e.g.
  `InodeRecord { size: u64::MAX, chunk_map: vec![], state: Committed, .. }`) returns a clean
  typed read error without panicking and without attempting an allocation proportional to the
  untrusted size. Demonstrable by the named regression (red pre-fix — panics / attempts the huge
  allocation; green post-fix — clean `Err`), verifiable in isolation at C4-verify. The specific
  error variant is ILLUSTRATIVE; the BINDING condition is "no panic / no size-proportional
  allocation, and a clean error is returned".
- **Invariant to restore:** An untrusted metadata field (`inode.size`) must not size an
  allocation before it has been validated against the actually-available data — reassembly
  allocates from what the chunk map can commit, not from the recorded size. Grounded in the read
  path's own contract that a size mismatch is surfaced as a typed error, never a panic or short
  read ("**Never bad data**", `crates/core/src/read.rs:8-16`; `ReadError::SizeMismatch`,
  `read.rs:307-312`). No §6 category applies (`docs/principles.md` §5–§6 are scaffold-empty);
  this is the code's own typed-error contract, tier-C internal.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Conflicts with:** 285
- **Ordering note:** 290 and 285 both edit `crates/core/src/read.rs` (290 fixes the allocation
  in `read_object_collecting`; 285 adds EC-scheme validation in/around `read_chunk`). No
  build-on dependency — scheduled into different waves so neither builds blind on the other.
- **Surfaces:** data
- **Difficulty:** low
- **Scope:** The output-buffer allocation in `read_object_collecting` trusts `inode.size`
  before validating it against the chunk map. Remove that trust so a read of a corrupt oversized
  inode fails cleanly. / out of scope: the on-disk metadata format / codec schema (ADR-0002 —
  human-only); configurable object/read size limits as a new feature; the EC-scheme validation
  of #285; the broader M4 metadata validation-boundaries research (#291) beyond this defect.
- **Repro instruction:** On `main`, in `crates/core/src/read.rs`, build a committed
  `InodeRecord` with `size: u64::MAX` and an empty (or short) `chunk_map`, then call the read
  reassembly path — it panics on `Vec::with_capacity(u64::MAX as usize)` / attempts a giant
  allocation before the size-mismatch check at `read.rs:83`.
- **Test file:** crates/core/src/read.rs (tests module)
- **Citations expected:** Do must cite path:line on the target branch for every change.
- **Prior-art check (triage cycles):** searched by file path — `crates/core/src/read.rs` recent
  commits (2828f2f, 9d0af20, 5aece0e, …) are repair/placement/fragment work, none touching the
  `with_capacity(inode.size)` allocation; no open PR referencing 290. No prior or in-flight fix.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle. The PR MUST NOT be marked ready before sign-off.
