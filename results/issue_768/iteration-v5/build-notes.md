# Build notes — issue #768, last-modified-never-decreases (iteration 5)

Builder: Do leaf, attempt 5. The carry-forward said the iteration-4 fix is right and the tests
are too weak, and listed four items. This attempt starts from `iteration-v4/patch.diff` (applied
cleanly to the base, `git apply --check`), addresses the four items, and adds unit tests that
kill the other three surviving mutants the carry-forward's gate reported. The brief's behaviour
and scope are otherwise unchanged: the stored `+1 ms` rule with no cap and no heal, the typed
refusal of an unstamped overwrite of a stamped version, the far-ahead warning, and the S3
shown-time cap at the response's `Date`.

## Base and precondition

- Worktree `$PDCA_WORKTREE` = `/home/eddie/wyrd/wyrd.pdca-wt-l1`, detached at `8ebd1c0`. That is
  still `origin/main` (re-fetched 2026-10-18 before the verify run: both resolve to
  `8ebd1c02b3da79db86a89fa54fdea8a838f0448a`). It contains #898 (#767), #899 (#671), #908 (#894)
  and #909 (#895), so every "Conflicts with" slice is merged underneath this patch.
- **Waiver precondition met:** `git merge-base --is-ancestor 0674f4d HEAD` succeeds and both
  ids are in `deny-all-features.toml:160` (RUSTSEC-2026-0346) and `:167` (RUSTSEC-2026-0349).
  No stop needed.
- Citations below are `path:line` on the patched tree.

## The four carry-forward items

### 1. Warning threshold under test with literals (C5: 6 survivors on the constant, 1 on `Display`)

- `crates/core/src/metadata.rs:5567`: the test module's `DAY` is now the literal `86_400_000`,
  with a doc saying it is deliberately not derived from `MODIFIED_LEAD_WARN_MILLIS`.
- `:5850-5884` `a_prior_more_than_a_day_ahead_is_still_ordered_after_and_is_logged`: a lead of
  `86_400_001` ms is logged (`:5853`, the log line is asserted to carry
  `"lead_millis":86400001`), leads of `86_400_000` and `86_399_999` are not (`:5877`). Any
  mutation of `24 * 60 * 60 * 1_000` (`:2287`) moves the threshold away from 86_400_000 and one
  of the two sides catches it: a lower threshold logs the 86_400_000 case, a higher one stops
  logging the 86_400_001 case.
- `:5646-5652`: the exact `Display` text of `UnstampedOverwrite` is asserted (inode 7, prior
  `1700000005250`), so the `fmt -> Ok(Default::default())` mutant is caught.
- Result: `scripts/mutants-in-diff` on the final `patch.diff`: **39 mutants tested in 2m: 35
  caught, 4 unviable, 0 missed** (iteration 4: 29 tested, 10 missed, 16 caught, 3 unviable). Log:
  `$PDCA_SCRATCH/pdca-builder-768-mutants-v5.log`.

### 2. Wire window also bounded by the test's clock after the response

- `crates/server/tests/last_modified_monotonic.rs:905-937`: `Around { before, after }` carries
  the test's two readings; `assert_in_window` now asserts `before ≤ shown ≤ Date ≤ after`:
  `date <= after` (`:926`) and `shown <= after` (`:932`), with a comment saying why: the gateway
  reads the same host clock, before the response arrives, so a later `Date` or shown value is a
  gateway clock reading that ran ahead of the host's, and `shown ≤ Date` alone would still hold
  for it (the handler writes `Date` from that same reading). `shown_in_window` (`:941`), `list`
  (`:963`) and `listed_in_window` (`:972`) read `after` right after the response; leg (8) reads
  `after_head` / `after_listing` (`:1139`, `:1158`). The module doc (`:36-41`) states the window
  the same way.
- **Checked for real, not by reasoning.** With `epoch_secs_now()` temporarily returning
  `as_secs() + 600` (then reverted; the gateway file is byte-identical to the gated version,
  checked by diffing the two patches), `cargo test -p wyrd-server --test last_modified_monotonic
  wire_`:
  - with only `shown ≤ after` added (the carry-forward's literal wording): leg (7) red
    (`HEAD ahead: the shown time 1792343415 ... must not be later than this host's clock just
    after the response (1792342815)`), leg (8) **green** — in (8) the stored time is the real
    clock, so the cap never bites and the shown time is honest; only the handler-set `Date` was
    wrong, and nothing asserted on it;
  - with `Date ≤ after` added too: both legs red (`the response's Date 1792343466 ... must not
    be later than this host's clock just after the response (1792342866)`, in (7) at the first
    HEAD and in (8) at the create's HEAD).
  So the `Date ≤ after` bound is what makes leg (8) catch an ahead clock, which is why it is in
  the window and not only `shown ≤ after`. It is as robust as the existing `before ≤ shown`:
  both rest on one host clock read in order.

### 3. A clock that cannot be read caps nothing and writes no `Date`

- `crates/gateway-s3/src/lib.rs:3031` `epoch_secs_now() -> Option<u64>`: `None` when
  `duration_since(UNIX_EPOCH)` fails (a host clock before 1970). The `.unwrap_or(0)` is gone.
- `:3084` `shown_modified(stored, now_secs: Option<u64>)`: `None` clock → the stored time is
  returned unchanged (never 1970). `:3099` `date_header(now_secs: Option<u64>)`: `None` clock →
  no `Date` written; hyper's own stands (RFC 9110 §6.6.1).
- The one `now_secs` parameter threaded through the renderers becomes `Option<u64>`:
  `apply_object_headers` `:2590`, `full_object_response` `:2623`, `partial_content_response`
  `:2650`, `precondition_response` `:2681`, `head_partial_content_response` `:2699`,
  `not_modified_response` `:2731`. The three handler clock reads are unchanged in shape
  (`:2451`, `:2488`, `:2533`).
- `evaluate_conditionals` (`:2919`) takes `Option<u64>` and does `unwrap_or(0)` once, inside
  (`:2925`), for the RFC-850 two-digit-year pivot only. That is the pre-#768 behaviour of that
  parser (#619): the pivot is a parsing default and is never shown. I chose to put the fallback
  inside `evaluate_conditionals` rather than at its three call sites so the reason is stated
  once; `evaluate_conditionals` and `parse_http_date` have no direct unit-test callers, so the
  signature change costs nothing elsewhere.
- Unit test `:5002` `a_clock_that_cannot_be_read_caps_nothing_and_writes_no_date`: the pure
  helpers, the `200`/`206` renderer (`apply_object_headers`), the `304` renderer, a listing row
  (stored shown as stored; `u64::MAX` omitted, and `1970` nowhere in the output) and the date
  preconditions with an IMF-fixdate, all with `None` for the clock.
- Not handler-level: the handlers read the real clock, which on this host is after 1970, so a
  handler-level test of the failed read would need a clock seam the brief did not ask for. The
  renderers the handlers call are tested with `None` directly.

### 4. Listing cost: cap only the emitted rows, one clock reading into the renderer

- `crates/gateway-s3/src/lib.rs:932-938`: the per-object capping loop over the whole bucket is
  gone. `now_secs` is read once, after `compute_page`, and passed into `render_list_v2`
  (`:1103`, new last parameter) and `render_list_v1` (`:1170`), which hand it to
  `render_contents` (`:1055`) per emitted row. `render_contents` applies
  `shown_modified(obj.modified, now_secs)` itself; its doc says so and says why (a listing pays
  for the rows it returns, not for every object in the bucket). The listing's `Date` is
  rendered from the same reading (`:961-966`).
- The `render_contents` unit tests pass the new argument. `a_contents_row_omits_unpresentable_
  recorded_metadata` keeps its year-10000 expectation by passing `None` (no clock to cap at,
  the renderer still omits rather than emits a five-digit year); the brief's condition for
  keeping it ("as long as `render_contents` is handed the value it renders") no longer applies
  literally since the renderer now caps, so the new test `:4871`
  `a_contents_row_shows_a_future_recorded_time_as_the_clock` pins what a clock does to the same
  row: `+1 ms`, `+1 h`, year 10000 and `u64::MAX` all show the clock's second.

## The other three surviving mutants (gateway `-> Default::default()`)

The carry-forward's gate also listed `list_objects`, `serve_head` and
`head_partial_content_response` replaced by `Default::default()` as missed. They survived
because no `wyrd-gateway-s3` unit test drove HEAD or a bucket listing through the router (the
server crate's tests do, but `cargo mutants` runs the mutated crate's own tests). Two
handler-level tests now drive the real router (`S3Gateway::new(..).router()` +
`tower::ServiceExt::oneshot`, as the existing malformed-metadata tests do):

- `:5369` `head_shows_a_future_stored_time_as_its_own_date`: a signed unranged HEAD (`200`) and a
  ranged HEAD (`206`, `Content-Range: bytes 0-3/11`) of an object stored at `u64::MAX - 1`;
  `Last-Modified == Date`, a well-formed IMF-fixdate, between the test's clock before and
  after the request (`assert_shown_as_the_clock`, `:5350`).
- `:5413` `a_listing_shows_a_future_stored_time_as_its_own_date_and_a_past_one_as_stored`: v2
  and the v1 shim; the `ahead` row lists the listing's own `Date` second, the `behind` row its
  stored second.
- To support them, the `StoredMetaGateway` double gains `listed: Vec<ListedObject>` (`:5179`),
  a `list_container` that returns it (`:5196`), and `head_object` now returns the double's own
  `modified` (`:5236`) instead of a hard-coded value (no existing test drove HEAD through it, so
  nothing else changes). `signed_through_router` delegates to a general
  `signed_request_through_router(gateway, method, path, query, extra, body)` (`:5269`) rather
  than a fourth copy of the signing code.

## Everything carried from iteration 4 (unchanged in substance)

- Core rule `overwrite_modified` (`crates/core/src/metadata.rs:2373`), `MODIFIED_LEAD_WARN_MILLIS`
  (`:2287`), `UnstampedOverwrite` (`:2299`, `Display` `:2306`), both overwrite commits calling it
  after the segmented-prior refusal and before the leases, the field docs on `ObjectMeta::modified`
  and `InodeRecord::modified`, `write.rs` docs, the architecture-doc paragraph
  (`docs/design/architecture/05-building-block-view.md:205-238`), the S3 shown-time cap and the
  handler-set `Date` (see iteration-v4/build-notes.md for why the gateway sets `Date` itself:
  hyper's cached `Date` can be one second earlier than the handler's clock read).
- No ADR added or edited. No server code change. No new dependency.

## Refuting my own test (forced)

- **(a) Genuine red? Yes.** `engine/scripts/run-verify.sh` (the C4-verify runner; lane 1, base
  `origin/main` = `8ebd1c0`, `PDCA_BRIEF_BASE=origin/main`) on the final `patch.diff`:
  GREEN 9/9 with the fix; RED with the production change reverted and the test kept: 5 of 9
  fail, `PASS — red without the fix, green with it (9 test(s) ran red)`, exit 0. Red failures
  are assertions, not build errors (the test uses only pre-existing API):
  - (1)/(2)/(6): `the stored time must step exactly 1 ms past the prior, never back to this
    host's clock` (seeds `1792345982500` an hour ahead, `1792515182500` two days ahead);
  - (5): both refusals and the no-reset chain on both APIs (`must be refused with a typed
    error, not answer Committed (stored modified now None)`) plus the equal/ahead/far-ahead
    cases;
  - (7): `HEAD ahead: the shown time 1792345982 (Sun, 18 Oct 2026 17:53:02 GMT) must lie
    between the clock just before the request (1792342382) and the response's own Date
    (1792342382)`;
  - (3), (4), (8) pass on the base by design.
  Log: `$PDCA_SCRATCH/pdca-builder-768-verify-v5.log`.
- **(b) Production path? Yes.** Legs (1)–(4), (6) drive the real `wyrd_server::Gateway` through
  `put_object`, `put_object_streaming` and `ObjectGateway::head_object`; leg (5) calls the two
  public core commits; legs (7)–(8) start the real `S3Gateway::serve` on a loopback listener and
  send SigV4-signed raw HTTP/1.1 through hyper → axum → `serve_get` / `serve_head` /
  `list_objects`. The new gateway unit tests drive the real router with `oneshot`. The only
  double is `StoredMetaGateway`, which stands in for the store, not for the code under test.
- **(c) Fixture includes the fault? Yes, with the same honest limit as before.** The fast-clock
  fault is a stored stamp ahead of this host's clock, written through the ordinary write path;
  the store keeps no record of which gateway stamped it, so it is byte-identical to what a fast
  gateway leaves. Real two-host skew is not reproducible in-process (`now_millis` reads the host
  clock); the brief accepts this stand-in. The pre-1970 clock cannot be produced on this host, so
  item 3 is tested at the renderer/helper level with `None`, not through a handler.

## Gate results (in `$PDCA_WORKTREE`)

- `cargo fmt --all`: applied; the tree is formatted.
- `cargo clippy -p wyrd-core -p wyrd-gateway-s3 -p wyrd-server --all-targets`: clean.
- `cargo test -p wyrd-core --lib overwrite_modified_rule`: 5 passed.
- `cargo test -p wyrd-gateway-s3 --lib`: 85 passed (3 new tests).
- `typos` over the five touched files: clean.
- `scripts/mutants-in-diff`: 39 tested, 35 caught, 4 unviable, 0 missed.
- `engine/scripts/run-verify.sh`: PASS (above), run twice: on the gated patch and on the final
  one (`GREEN` 9 passed; `RED` 4 passed, 5 failed; `PASS — red without the fix, green with it
  (9 test(s) ran red)`, exit 0; log `$PDCA_SCRATCH/pdca-builder-768-verify-v5-final.log`).
- `engine/xtask.sh ci` (the gating C4-ci): **`xtask ci: all checks passed`, exit 0**, on the
  tree as it was before the last test-only change (the `Date ≤ after` assertion and its
  comment / module-doc line in `last_modified_monotonic.rs`; `diff` of the two patches shows
  only that file's hunk changed, by 10 lines). The run covered `typos`, `lint_docs.py` (OK),
  `render_site.py --check` (99 pages, link audit OK, so the `docs-renderer` dependency was
  present), the gitlink/unsafe/blackbox guards, fmt, workspace clippy and build, `cargo test
  --workspace` (215 `test result: ok`, 0 failed), cargo-machete, `cargo deny check`
  (`advisories ok, bans ok, licenses ok, sources ok`), `cargo deny --all-features --config
  deny-all-features.toml check advisories` (`advisories ok`), conformance vectors (5 valid + 6
  invalid), `xtask statics`, `xtask deploy-guard`, and the madsim DST seeds. After the test-only
  change I re-ran what that change can affect: `cargo fmt --all -- --check` (clean),
  `cargo clippy -p wyrd-core -p wyrd-gateway-s3 -p wyrd-server --all-targets` (clean), `typos`
  on the test file (clean), and the test itself through `run-verify.sh` (the final run above).
  All three of the brief's External dependencies (`cargo deny all-features advisories`, `typos`,
  `docs-renderer`) were present, so there is no NEEDS-HUMAN external-dependency item.
  Log: `$PDCA_SCRATCH/pdca-builder-768-xtask-ci-v5.log`.
- `patch.diff` is byte-identical to `git diff` in the worktree (intent-to-add on the new test
  file), and the bundle's `last_modified_monotonic.rs` is byte-identical to the worktree's.
- Scratch: only log files under `$PDCA_SCRATCH` (`pdca-builder-768-*-v5.log`); the verify
  worktree `../wyrd-verify-l1` is the runner's own. No files outside the harness roots.

## Open for sign-off (not defects in this patch)

- Brief Open question 2 (a mirror that already holds a future time) is still the maintainer's
  call; leg (7)'s "earlier than the seed" assertion pins that the exception is real.
- Intended wire changes, unchanged from iteration 4: a stored time past year 9999 is shown as
  the response time instead of being omitted; every 200/206/304/listing response that shows a
  `Last-Modified` carries a handler-set `Date`. New in this iteration: on a host whose clock
  reads before 1970 the gateway caps nothing and leaves `Date` to hyper (which, per
  iteration-4's notes, cannot itself run on such a host).
