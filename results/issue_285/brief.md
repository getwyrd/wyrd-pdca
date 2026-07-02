# Brief — issue 285 / validate-ec-scheme-at-read-boundary

> The Plan artifact (docs 02 §PLAN). Human-authored. Do reads ONLY this file.

- **Slug:** validate-ec-scheme-at-read-boundary
- **Defect:** A corrupted or malformed inode record carrying `EcScheme::ReedSolomon { k: 0, .. }`
  (or an otherwise unsupported `k`/`m`) is trusted by the lower read/reconstruct layers. The
  CLI rejects `rs(0,m)` when parsing `--durability` (`crates/server/src/cli.rs:110`), but the
  stored EC scheme read back from metadata is never re-validated. `read_chunk` casts `k`/`m`
  straight to `usize` (`crates/core/src/read.rs:176`), and for `k == 0` the `shards.len() < k`
  guard (`read.rs:243`) is `len < 0` which never trips, so `erasure::reconstruct(0, m, …)` runs;
  its own `available.len() < k` check (`crates/core/src/erasure.rs:95`) also never trips, and
  `available[0]` (`erasure.rs:101`) then panics on an empty shard list. Corrupt/tampered
  metadata becomes a process panic instead of a clean read error.
- **Success criterion:** Calling `erasure::reconstruct` with `k == 0` (and, at the read path, a
  committed inode whose stored chunk scheme is `ReedSolomon { k: 0, m }`) returns a typed error
  rather than panicking or indexing an empty slice. Demonstrable by the named regression test
  (red pre-fix — panics/indexes; green post-fix — returns the typed error), verifiable in
  isolation at C4-verify. The typed-error *variant name* is ILLUSTRATIVE; the BINDING condition
  is "invalid EC parameters from stored metadata yield a clean `Err`, never a panic".
- **Invariant to restore:** EC-scheme parameters that originate from stored (untrusted) metadata
  must be validated at the erasure/read API boundary before they index shard buffers or drive
  fan-out — a malformed scheme fails as a typed error, never as a panic or out-of-bounds index.
  Grounded in the read path's own documented contract: "**Never bad data** … below `k` survivors
  the read fails with a typed error" (`crates/core/src/read.rs:8-16` module doc) and the CLI's
  existing `k >= 1` rule (`cli.rs:110`), which the lower layers must not silently undercut. No
  §6 category applies (the catalogue is scaffold-empty, `docs/principles.md` §5–§6); this is the
  code's own stated typed-error contract, tier-C internal.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Conflicts with:** 290
- **Ordering note:** 285 and 290 both edit `crates/core/src/read.rs` (285 adds read-path EC
  validation in/around `read_chunk`; 290 fixes the allocation in `read_object_collecting`).
  No build-on dependency between them — scheduled into different waves so neither builds blind
  on the other's base.
- **Surfaces:** data
- **Difficulty:** medium
- **Scope:** Malformed EC-scheme parameters read back from inode metadata reach shard
  reconstruction / fan-out unvalidated (the `k == 0` panic being the concrete symptom, plus
  unsupported `k + m`). Restore validation at the erasure/read boundary so such records fail as
  a typed error. / out of scope: changing the CLI parse rule (already rejects `rs(0,m)`); the
  on-disk metadata format / codec schema (ADR-0002 — human-only); the broader M4 metadata
  validation-boundaries research (#291) beyond this specific defect.
- **Repro instruction:** On `main`, construct a `ReedSolomon { k: 0, m }` scheme and drive
  reconstruction: `erasure::reconstruct(0, 1, 0, &[])` panics (indexing `available[0]`). At the
  read path, a committed `InodeRecord` whose chunk `scheme` is `ReedSolomon { k: 0, m: 1 }`
  reaches the same panic through `read_chunk`.
- **Test file:** crates/core/src/erasure.rs (tests module — the `reconstruct(0, …)` boundary
  regression; Do adds a companion read-path assertion in `crates/core/src/read.rs` tests for the
  stored-scheme case)
- **Citations expected:** Do must cite path:line on the target branch for every change.
- **Prior-art check (triage cycles):** searched by file path — `crates/core/src/erasure.rs`
  (last touched d732843 create, e03da3f mutant tests), `crates/core/src/read.rs` (recent repair/
  placement work, none validating stored EC params), no open PR referencing 285 / EC validation.
  No prior or in-flight fix for this defect.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle. The PR MUST NOT be marked ready before sign-off.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Rebuild must address the codex finding: read-boundary validation (read.rs:184) currently rejects only k == 0. Extend it to reject ALL invalid/unsupported stored EC schemes — notably m == 0 and unsupported k+m — using the same supported-scheme predicate the erasure coder uses, so a tampered rs(k,0) inode can no longer drive read fan-out and return bytes without rejection (path: read.rs:267). The builder previously (and defensibly) scoped this to k == 0 per the brief's narrow success criterion, so the rebuild must NOT repeat the narrow fix: the k==0-only reading is explicitly rejected here. Note: the gating C4-ci failure (cargo test --workspace exit 101) was base-drift / transient, NOT attributable to this patch — re-running the identical command on the applied patch (base now at #402 merge) is green, exit 0, zero failures. It is not a reason to iterate; the codex scope gap is the sole reason.
- Failing gate: C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) — xtask: `cargo test --workspace --exclude wyrd-dst` failed with exit status: 101
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
