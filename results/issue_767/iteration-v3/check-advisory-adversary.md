# Adversarial review — issue 767 (listing `<LastModified>` at whole seconds)

**Bottom line:** I tried to refute the fix and its red→green evidence and could not. The two
open gate problems are a baseline dependency advisory and a question about how the review
gate was run. Neither is a defect in this diff, and the builder can't fix either by
rebuilding.

## Evidence (re-run independently)

- `crates/server/tests/s3_last_modified_agreement.rs:767` — I re-ran red→green in a scratch
  copy of `$PDCA_TARGET`. **Green:** 4/4 integration tests, and the 3 related unit tests in
  `crates/gateway-s3/src/lib.rs` pass. **Red** (only `crates/gateway-s3/src/lib.rs` reverted,
  test kept): `off_second_modified_lists_and_heads_as_the_same_instant` fails at `:767` with
  `"2023-11-14T22:13:20.750Z"` vs `"…20.000Z"`. This matches `gate-logs/C4-verify.log:25-29`.
  Could not refute.
- `crates/server/tests/s3_last_modified_agreement.rs:782` — the red run stops at check (1),
  so check (3) never runs before the fix. I turned check (1) off in the scratch copy and ran
  the pre-fix base again. Check (3) fails on its own (`20.75Z` vs `20Z`, parsed by
  `aws-smithy-types`), so the parsed-instant check really tells the two apart. It is not a
  tautology. Could not refute.
- `crates/gateway-s3/src/lib.rs:1130`, `:1187` — the test runs the production path: it seeds
  through `wyrd_core::write`, then sends real signed requests over TCP to the real `Gateway` +
  `S3Gateway`. The v1 and v2 listings both render through `render_contents` (`:1030`), and the
  test asserts both. No mirrored copy of the production code is involved.
- `gate-logs/C4-verify.log:38` (wording only, no action) — the gate's summary says "4 test(s)
  ran red", but only 1 of the 4 failed before the fix (`:35`, "3 passed; 1 failed"). The
  parser and response-reader tests check the test harness itself, and the on-second test is a
  regression guard. All three are green on both sides, by design. Read this row as one red
  proof, not four.

## The fix (tried to break it)

- `crates/gateway-s3/src/lib.rs:637`, `:3204` — both formatters derive the instant from
  `epoch_millis / 1_000`, so they agree to the second for every `u64`. They also share the
  year-9999 cut-off (`:642`, `:3211`): `253_402_300_800_000` and `u64::MAX` give `None` from
  both, so no value can render on one surface and not the other. Rounding instead of
  truncating would be caught: 750 ms rounds up to `:21`, the test expects `:20`, and the unit
  test has a 999 ms row (`:4640`). Could not refute.
- `crates/gateway-s3/src/lib.rs:2499`, `:2607`, `:2787`, `:2802` — every header surface
  (200, 206, ranged HEAD, 304) goes through `http_date`, and the conditional-request
  comparisons already truncate to seconds. The only readers of `modified`
  (`crates/server/src/lib.rs:445`, `:531`, `:626`, `:739`) feed these paths. I found no other
  surface that renders the stored milliseconds. Could not refute.
- `crates/server/tests/s3_last_modified_agreement.rs:247`, `:344` — I looked for a
  false-success case in the strict response reader. What leniency is left (a status line with
  no space before an empty reason phrase; header names checked as `ascii_graphic` instead of
  `tchar`, the RFC's allowed token characters) only accepts slightly malformed responses, and
  the timestamp checks are exact string matches. A framing leak can't make the property pass
  falsely. Not a finding.
- A trade-off, not a refutation: truncating means two same-size overwrites within one second
  now list with the same `LastModified`, so a sync tool that compares only size and time
  (`aws s3 sync` by default) skips the second one. Real S3 behaves the same way, the HEAD
  header already did, and draft 0017 already notes it
  (`docs/design/proposals/draft/0017-blackbox-validation-tool.md:369-370`). No action.

## The verdict (claims in check-gates.json)

- NEEDS-HUMAN [human] — `gate-logs/C4-ci.log:3472-3550` (the second attempt repeats at
  `:6474-6551`): the failing C4-ci gate is the baseline advisory RUSTSEC-2026-0346/0349 in
  `rustls 0.21.12`, pulled in through `tikv-client` (`Cargo.lock:3294-3295`). It is not caused by
  this diff: the patch doesn't touch `Cargo.lock`, the default `cargo deny check` passes
  (`:3469-3470`), and typos, docs lint and render, fmt, clippy, build, the full test run and
  cargo-machete all passed before the stop. The gate never reached the steps after deny, so I
  ran `cargo xtask statics` and `cargo xtask conformance` on the patched tree in scratch, and
  both pass. I did not run DST, but `crates/dst/Cargo.toml` depends on neither `gateway-s3`
  nor `server`. This failure has already triggered two auto-iterate rebuilds
  (`brief.md:133-141`) that the builder cannot fix, because bumping the dependency or
  ignoring the advisory is a human-only call (ADR-0003 / `deny.toml`). Decide the advisory
  disposition; don't send this back to Do again.
- NEEDS-HUMAN [human] — `gate-logs/T4-batch-review.log:6`, `:10`: the gating rubric review
  reports "0 blocking, 0 recorded-rejected, 0 noise-dropped" after 22.28 s for "3x codex".
  Last round the same gate reported 6 blocking. Three model passes over an ~800-line diff in
  22 s is fast enough that a cached or empty result is possible. If so, the zero count passes
  while no review happened (the rubric's "count-based assertion that can pass while the
  property fails" class). I can't see `results/issue_767/review-batch.md`. Before counting
  this row as green, confirm it holds three real passes over this patch.
- NEEDS-HUMAN [human] — `docs/design/proposals/draft/0017-blackbox-validation-tool.md:461-463`:
  this draft proposal still says a listing "keeps milliseconds" and that the two surfaces
  differ by up to 999 ms. After this patch that is false. The brief scopes out any other docs
  edit (`brief.md:91-93`), and the rubric says an out-of-scope finding gets a decline with an
  issue reference. Decide whether to amend the draft here (drafts aren't frozen) or file a
  follow-up against #769. The living architecture doc is correct
  (`docs/design/architecture/05-building-block-view.md:194-196`), and the iteration-1
  placement problem is fixed.

<!-- pdca:leaf-complete -->
