# Adversarial review — issue #767 (listing `<LastModified>` at whole seconds)

Verdict: **could not refute the fix.** I re-ran the red→green proof, tried to break it with a
mutant, and looked for a timestamp surface the patch misses. Nothing broke. What follows is
one small doc wording slip, one gate red that this diff did not cause, and notes.

## Evidence (attempted to refute; could not)

- `crates/server/tests/s3_last_modified_agreement.rs:384` — re-ran it myself in a scratch copy
  of `$PDCA_TARGET`. With the fix: 3/3 pass (plus the in-file unit test
  `crates/gateway-s3/src/lib.rs:4621`). With the pre-fix `lib.rs` from `HEAD`: fails at `:384`,
  `left: "2023-11-14T22:13:20.750Z"`, `right: "…20.000Z"`. This is the same result as
  `gate-logs/C4-verify.log`. The test goes through the real path: seeded store →
  `Gateway::new` → `S3Gateway::serve` → raw signed HTTP. No mock or copy of the formatter is
  involved.
- Mutant probe: I changed `iso8601` to round (`(epoch_millis + 500) / 1_000`) instead of
  truncating. The test catches it at `:384` (`…22:13:21.000Z` vs `…20.000Z`). A cross-check
  that only compared shapes, or allowed ±1 s, would have missed this. This one doesn't.
- False-pass paths I checked and ruled out: a missing `<Contents>` row, a missing or unclosed
  `<LastModified>`, or a missing `Last-Modified` header all panic (`:362-376`, `:470-477`).
  An omitted element cannot pass as "agreement". The v1 leg really is v1: it checks that
  `<KeyCount>` is absent (`:455`), and dispatch is at `crates/gateway-s3/src/lib.rs:832-844`.
- Minor: `check-gates.json` C4-verify says "3 test(s) ran red". Only 1 test went red. The
  other 2 (parser self-check, on-second object) pass on base by design. This wording comes
  from the harness, not from the patch.

## The fix (attempted to break; could not)

- Missed surfaces: `iso8601` has exactly one production caller, `render_contents`
  (`crates/gateway-s3/src/lib.rs:1033`), and v1 and v2 share it. The GET/HEAD header goes
  through `http_date` (`:2499`, `:2607`). If-(Un)Modified-Since already compares `m / 1_000`
  (`:2786`, `:2801`), so a client that echoes a listed time back as a conditional gets a
  consistent answer. There is no CopyObject result (`:1809` refuses it), no ListBuckets
  `CreationDate`, and no ListParts yet. `gateway-core` only carries the raw `u64`.
  `crates/dst`, `crates/validate` and `crates/testkit` have no timestamp assertions that the
  change could silently break. (The C4-ci gate excludes `wyrd-dst`, so I grepped it.)
- Boundaries: both formatters compute `secs = epoch_millis / 1_000`, call the same
  `civil_from_days`, and return `None` on the same `year > 9_999` check (`:641`, `:3209`). So
  for every `u64` they either both render the same second or both omit. The last renderable
  millisecond `253_402_300_799_999` is pinned (`:4642`). I found no input where the two
  surfaces disagree.

## Findings

- NEEDS-HUMAN [impl] — `docs/design/architecture/05-building-block-view.md:190-194`: the new
  sentence went in between the list of the three fields and "Each is optional with a serde
  default *and* skipped when absent". "Each" now follows a sentence whose subjects are "the
  `Last-Modified` header and a listing's `<LastModified>`", so it reads as if the two wire
  surfaces are "optional with a serde default". Fix: move the new sentence after the
  serde/CAS sentence (after `:197`), or write "Each of the three fields is optional…". Also
  re-wrap the stub line `:194`.
- NEEDS-HUMAN [human] — `gate-logs/C4-ci.log:3471-3549`: the only C4-ci red is
  `cargo deny … check advisories`, on RUSTSEC-2026-0346 and RUSTSEC-2026-0349 against
  `rustls 0.21.12` (via `tokio-rustls 0.24.1`). That dependency comes from the base. This diff
  does not touch `Cargo.toml` or `Cargo.lock`, and every test step passed in both attempts. The
  red is real and gating, but it is not evidence against this fix. It needs its own dependency
  bump issue, or a decision on whether a base-wide advisory blocks unrelated PRs.

## Notes (no action needed for this diff)

- T4-batch-review's 6 blockers (`gate-logs/T4-batch-review.log`) are all about the test
  harness: lenient `dechunk`, `Content-Length` ignored, and no socket timeout. None of them
  shows a way for this test to pass falsely. A truncated body loses the asserted element and
  panics. The no-timeout point fits the rubric's await-discipline rule, but the peer harness
  the brief told the builder to copy has the same unbounded read
  (`crates/server/tests/s3_list_objects.rs:206`). Wrapping connect and read in
  `tokio::time::timeout` is cheap. The gate already sends this back, so I have not tagged it.
- Visible one-time change on upgrade: a client that cached `…20.750Z` from a pre-fix gateway
  will see `…20.000Z` on its first post-upgrade listing. A tool that compares listing to
  cache (rather than listing to HEAD) will then treat each such object as changed once. This
  is inherent to the fix, and the fix is still correct (it matches S3 and the header). It is
  worth one sentence in the PR description when it is drafted (T4-contribution is deferred).
- Out of scope per the brief: the draft proposal
  `docs/design/proposals/draft/0017-blackbox-validation-tool.md:461-468` still says a listing
  "keep[s] milliseconds" and that the oracle "never compares one [surface] to the other". After
  this fix the oracle could compare them. Per the rubric's out-of-scope rule, this calls for a
  follow-up issue, not an in-PR edit.
<!-- pdca:leaf-complete -->
