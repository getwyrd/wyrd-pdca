# Adversarial review — #840 (809.2): every session record carries its segment-group nonce

**Verdict: could not refute.** I attacked the evidence, the fix, and the gate claims. None of
them broke. No NEEDS-HUMAN items.

## The evidence (red→green re-run myself, in a scratch copy of `$PDCA_TARGET`)

- Attempted to refute the red leg: reverted `crates/core/src/{multipart,metadata}.rs` to the
  pre-fix base and kept `crates/core/tests/multipart_segment_nonce.rs`. **17 of 17 failed**, by
  assertion, with `unknown field segment_nonce`. With the fix, **17 of 17 passed**. The
  `C4-verify` claim holds. Every red failure comes from leg (a)'s positive arm
  (`crates/core/tests/multipart_segment_nonce.rs:113`), so (b)–(d) never ran their negative
  arms on the base. The brief says so up front (Falsifiability), so this is not a hidden gap.
- Attempted to show the test copies production instead of calling it: it doesn't. Every leg
  calls the real `decode_session_record` (`crates/core/src/multipart.rs:2315`) and
  `metadata::encode`, and also checks that the store-wide `metadata::decode::<SessionRecord>`
  agrees.
- Attempted to show leg (c) is a tautology, using a hand mutant: `de_segment_nonce`
  (`crates/core/src/multipart.rs:2085`) lowercases the input before validating it, so an
  uppercase nonce gets through as a second spelling. **Caught** in all four states
  (`multipart_segment_nonce.rs:144`, `leg_c`): S2 answers `NoncanonicalRecordValue` where the
  test expects `MalformedRecordValue`, and S1 accepts where the test expects an error.
- Attempted to find an old negative test that now passes for the wrong reason, i.e. refused
  for the missing nonce instead of its intended fault. There are none. Every hand-built session
  record in the workspace (grep on `"object"` and `clock_source` across `crates/`) now carries
  the nonce. The negative legs that reuse those builders pin their own needle or variant: 1j at
  `multipart_session_records.rs:499`, the `null` content type at `:647`, and field order at
  `:713`. Every custodian and DST builder asserts that its record decodes
  (`crates/custodian/tests/staged_drain_status.rs:283`, `crates/dst/tests/custodian.rs:2767`),
  so a missed fixture would fail loudly, not quietly.

## The fix

- Attempted to break serialization identity (rubric: *Serialization identity*). The field is
  required, not optional, so there is no absent-or-default spelling. `SegmentNonce` serializes
  `transparent`, and its struct position (`multipart.rs:2179`) matches its position on the wire
  (`:2104`). A `\u`-escaped nonce decodes, then fails the canonical re-encode
  (`multipart.rs:1933`), so it is refused and never stored in a second spelling. A present
  `content_type` combined with the nonce round-trips (`multipart_session_records.rs:242`).
- Attempted to make `SegmentGroup::from_nonce` (`crates/core/src/metadata.rs:1044`) create an
  invalid group. It can't: `SegmentNonce`'s field is private (`metadata.rs:989`), and its only
  constructor validates (`:996-1006`). `SegmentGroup::new` already accepted any epoch, so no
  new state becomes possible.
- Attempted to make `attempt_segment_group` (`multipart.rs:2225`) name the wrong epoch. A
  mutant that uses `self.epoch` instead of `publish_target.epoch` would survive the tests, but
  it behaves identically: `try_from` refuses any record where the two differ
  (`multipart.rs:2288`). Not a defect.
- Attempted a production-reader regression. The one production decoder,
  `crates/custodian/src/reconstruction/staged.rs:274`, sends a decode fault to `Withheld`.
  `SessionRecord` has no constructor and nothing writes `mpu:` values yet, so there are no old
  stored records to break.
- Checked the design against 0016's lifecycle (`0016:508-520`, the fence rows in the batch
  table, and the state diagram at `:536-553`). `(nonce, publish_target.epoch)` is exactly the
  `seg:<g>:<E>` that the fence-release and `Completing → Aborting` rows retire. Returning `None`
  for `Completed` is correct: the terminal delete only needs the whole-group prefix
  (`seggrp:`/`seg:<nonce>:`), and the record carries that in every state.

## Gate claims the reviewer might have over-read (advisory; no action needed)

- `C4-diff-cov` "fail, 30%": this reflects how the tool ran, not a real gap. It ran only the new
  test file. The lines it reports as missed (`multipart.rs:2215-2235`, `metadata.rs:1044-1046`)
  are exercised by the tests at `multipart_session_records.rs:860`, `:876` and `:890`. I ran
  that file myself: 37 of 37 green.
- `C5-mutants` "pass": this is weak evidence. It made 6 mutants, 5 of which did not compile and
  1 was caught, so it says almost nothing about legs (c) and (d). The hand mutant above covers
  (c). For (d), the "beside" arm (`multipart_segment_nonce.rs:201-208`) requires
  `unknown field` and so blocks a placement-B mutant. The "instead" arm (`:209-215`) only checks
  that the error names `segment_nonce`, so it would pass whether the error is "unknown" or
  "missing". That is a loose assertion, but the "beside" arm already covers it. Not worth a
  rebuild.
