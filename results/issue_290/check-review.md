# Check review — issue 290 / no-preallocate-from-untrusted-inode-size

**Task under review:** `read_object_collecting` sized its output buffer with
`Vec::with_capacity(inode.size as usize)` *before* validating that the chunk map could
back that many bytes, so a corrupt/committed inode with `size: u64::MAX` turned an
ordinary read into a capacity-overflow panic (or an OOM-scale allocation) instead of the
typed `ReadError::SizeMismatch` the read path's "Never bad data" contract requires. Fix:
grow the buffer from bytes actually read (`Vec::new()`), letting the existing size check
surface the mismatch cleanly.

**Grounding note:** `$PDCA_TARGET` = `/home/eddie/wyrd/wyrd.pdca-wt` is present, readable,
and current — the patch is applied there and `crates/core/src/read.rs:86,90-96,364-438`
match `patch.diff` line-for-line, so citations ground on the target. Direct `cargo`
execution was blocked by sandbox approval this session; C2/C4 red→green is re-derived
statically and corroborated by the recorded gate results (`C4-ci` pass, `C4-verify` pass
in check-gates.json).

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Brief's binding criterion is precise and testable — "no panic / no size-proportional allocation, clean typed error" for an oversized `inode.size`; scope (allocation trust only) and out-of-scope (codec/format, size limits, #285) are delimited. `crates/core/src/read.rs:8-16` (contract) and `:313-319` (SizeMismatch) exist as cited. |
| C2 Reproduction (red pre-fix) | PASS | Pre-fix `Vec::with_capacity(u64::MAX as usize)` = `with_capacity(usize::MAX)`, which panics on 64-bit (request exceeds `isize::MAX`) *before* the size check at read.rs:90 — so the regression at read.rs:412 panics pre-fix. Gate `C4-verify` recorded pass (red→green demonstrated); re-derived statically here. |
| C3 Change | PASS | Single behavioural line changed: `crates/core/src/read.rs:86` now `let mut bytes = Vec::new();`, buffer grows only from checksum-verified chunk bytes (read.rs:87-88). Minimal, on-target, matches the diff; only other `with_capacity` (read.rs:206, shard vec bounded by `k`) is untouched and unrelated. |
| C4 Verification (red→green) | PASS | Post-fix: empty `chunk_map` ⇒ `bytes.len()==0 != u64::MAX` ⇒ `Err(SizeMismatch{expected:u64::MAX, found:0})` (read.rs:90-96); test assert at read.rs:427 (`"18446744073709551615" \|\| "0"`) holds. Gates `C4-ci` and `C4-verify` both pass in check-gates.json. |
| C5 Causal adequacy | PASS | Root cause = allocation sized from untrusted metadata ahead of validation; fix *removes* that trust (read.rs:86) rather than guarding a symptom. No capability probe / runtime guard added, so the C5 symptom-guard rule does not fire. Note (advisory, non-blocking): `Vec::new()` drops preallocation for legitimate large reads (dynamic regrowth); a bounded `with_capacity(min(size, cap))` was out of scope per brief — human may weigh perf. |
| T1 Structure | PASS | Regression lives in the `#[cfg(test)] mod tests` of the target file `crates/core/src/read.rs:364-438`, exactly the brief's named test location; `UnreachableStore` fixture is local and self-contained. |
| T2 Shape | PASS | Test name states the property (`oversized_inode_size_with_empty_chunk_map_errors_cleanly_not_panics`, read.rs:412); asserts a typed `Err` and fails loudly on `Ok`; `UnreachableStore` `unreachable!()` arms (read.rs:379-386) prove no fragment fetch is reached — pinning "no size-proportional allocation". |
| T3 Runtime | PASS | `#[tokio::test]` async test; compiles and runs under the recorded `C4-ci` pass. `InodeRecord` literal fields (size/chunk_map/state/version, read.rs:413-417) match the struct at `crates/core/src/metadata.rs:202-211`. |
| T4 Contribution | PASS | Test is a genuine red→green witness for this defect (panic pre-fix → clean `Err` post-fix), not a tautology; gate `C4-verify` (per-fix red→green) recorded pass. |
| T5 Judgment | PASS | Assertion binds the brief's stated invariant (typed error, no panic) rather than a specific variant, matching the brief's "variant is illustrative" guidance; the `unreachable!` store is a sound way to assert the allocation never scales. Advisory only: consider a comment/follow-up on the lost preallocation for the hot path. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Human must confirm this restores the intended read-path contract for untrusted metadata: (a) accept the perf trade-off of dropping preallocation on the legitimate large-read path vs. a bounded-capacity alternative (explicitly out of scope per brief §Scope) — decision owed on whether that follow-up is wanted; (b) confirm the prior-art check by file path (brief §Prior-art: read.rs commits 2828f2f/9d0af20/5aece0e, no open PR for 290) is still accurate and that #285's concurrent edits to read.rs won't collide at merge (brief §Conflicts/§Ordering). |

## Notes for the human (§6 seeds)
- **Validation / fitness-to-purpose (NEEDS-HUMAN):** sign off that `Vec::new()` is the
  desired fix and that no bounded-preallocation follow-up is required; re-confirm the
  prior-art/no-in-flight-fix finding and the 290↔285 merge ordering on `read.rs`.
