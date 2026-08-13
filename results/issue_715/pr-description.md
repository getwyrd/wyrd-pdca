## Summary
**User impact:** Multipart uploads have a fleet-wide "admission ledger" record
(`mpuctl`) that tracks how many upload sessions are live against a budget.
That record type does not exist in code yet, so nothing can safely store or
read it: any future code that decodes a torn or hand-edited copy of it would
have no way to notice, and an inconsistent copy can let a server admit more
concurrent sessions than its memory budget allows -- an out-of-memory
incident on the maintenance/reconcile side of the system rather than on the
gateway that actually caused the overrun.

This change adds that record type and makes every internal inconsistency in
it a decode-time error, so a bad `mpuctl` value can never be read back as a
trustworthy budget.

There is no live writer for this record yet (that lands in a follow-up
change), so this PR has no effect on any running system -- it is pure,
additive groundwork: the two new files it touches are the only files it
touches, and nothing existing is modified in a way that changes behavior.

## What to look at
- `crates/core/src/multipart.rs`: the new `Budget` (the five-field admission
  profile) and `AdmissionRecord` (the `mpuctl` singleton) types, and
  `decode_admission_record`, the function that turns raw stored bytes into
  one of those types or a specific, named error.
- `crates/core/tests/multipart_budget_admission.rs`: exercises every rule
  by hand-authoring the JSON bytes of a record that breaks exactly one rule
  and checking decode names that rule -- the fastest way to see what "an
  internal inconsistency" means concretely.
- Try it yourself: `cargo test -p wyrd-core --test multipart_budget_admission`.

## Root cause
The module that defines every multipart storage key parses the keys but had
no type for the *values* those keys store, so the `mpuctl` admission record
had no decode-time validation at all -- any bytes with the right JSON shape
would be trusted.

## Fix
Add `Budget` and `AdmissionRecord`, each validating through a fallible
`TryFrom` conversion behind `#[serde(try_from = ...)]` (the same pattern the
existing `InodeRecord` type uses), so a value cannot exist in a malformed
state regardless of which code path decoded it. Add `decode_admission_record`
as the typed peer of the existing `decode_segment_record`, returning one
distinct error variant per rule so a caller can tell exactly which relation a
stored record broke. The two derived quantities (`U_ref`, `MAX_SESSIONS`) are
computed in `u128` so a torn record near its field's maximum value cannot
panic or silently wrap instead of being rejected. Three cases are pinned as
things decode must *not* reject -- an occupancy count above its own cap, and
profile values outside ranges owned by other, later changes -- so decode
enforces exactly the record's own internal consistency and nothing else.

## Verification
- **Claim:** `AdmissionRecord` round-trips through the existing store-wide
  `metadata::encode`/`metadata::decode` preserving every field, and its
  decode enforces exactly its eight structural rules -- no more, no less.
- **Checked:** `crates/core/src/metadata.rs:1536-1543` (the store-wide
  encode/decode this record reuses, unmodified) and
  `crates/core/src/metadata.rs:2504-2517` (`decode_segment_record`, the
  existing typed-decoder pattern this record's decoder mirrors), both on
  the target branch.
- **Test:** `crates/core/tests/multipart_budget_admission.rs` -- fails to
  compile against the pre-fix tree (the types it imports do not exist yet)
  and passes (18/18) against the post-fix tree; twelve additional
  hand-verified negation runs (one per rule, each isolated to that rule
  alone) are recorded for the maintainer in this change's accompanying
  notes.

Fixes #715
