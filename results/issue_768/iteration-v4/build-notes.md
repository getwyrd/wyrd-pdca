# Build notes — issue #768, last-modified-never-decreases (re-planned brief of 2026-10-16)

Builder: Do leaf. This attempt builds the re-planned brief: the stored `+1 ms` rule with no cap
and no heal, a typed refusal of an unstamped overwrite of a stamped version, the far-ahead
warning, and the S3 shown-time cap at the response's `Date`. Reference patches
`iteration-v2/patch.diff` (the `+1 ms` rule) and `iteration-v3/patch.diff` (S3-wire harness,
warning) were read as the brief allows. v3's rule (whole-second step, one-day heal) and its
expected values were **not** reused: every value below was re-derived from this brief.

## Base and precondition

- Worktree `$PDCA_WORKTREE` = `/home/eddie/wyrd/wyrd.pdca-wt-l1`, detached at `8ebd1c0`, which is
  `origin/main` on 2026-10-18. It already contains #898 (#767), #899 (#671), #908 (#894) and
  #909 (#895), so the "Conflicts with" slices are merged underneath this patch, not beside it.
- **Waiver precondition met:** `git merge-base --is-ancestor 0674f4d HEAD` succeeds, and both
  ids are in `deny-all-features.toml:160` (RUSTSEC-2026-0346) and `:167` (RUSTSEC-2026-0349).
  `cargo deny --all-features --config deny-all-features.toml check advisories` printed
  `advisories ok` inside the gate run below. So no stop was needed.
- The brief's line numbers are against `7bf7eb9` / `0dd47cc`. They have shifted on this base
  (for example `commit_written` is at `crates/server/src/lib.rs:309`, not `:269`). Citations
  below are on the patched tree, with the base line in parentheses where it helps.

## What changed (`path:line` on the patched tree)

### Core: the stored rule (`crates/core/src/metadata.rs`)

- `:2373` `pub fn overwrite_modified(inode, prior, published) -> Result<Option<u64>,
  UnstampedOverwrite>`, the one pure function both commits call:
  - `prior == None` → `Ok(published)` (legacy record; also covers None/None → None);
  - `published == None` over `Some(prior)` → `Err(UnstampedOverwrite { inode, prior_modified })`;
  - else `Ok(Some(published.max(prior.saturating_add(1))))`, no cap, saturating;
  - when `prior - published > MODIFIED_LEAD_WARN_MILLIS`, a `tracing::warn!` (`:2389`,
    target `wyrd.metadata.modified`) naming `inode`, `prior`, `published`, `lead_millis`. The
    committed value is the same either way.
  - Its doc comment carries leg (B)'s stored-side text, the shown-side summary, the mirror
    exception, the clock statement (reads no clock; ADR-0009) and the "#658 must stamp and call
    this" rule.
- `:2287` `const MODIFIED_LEAD_WARN_MILLIS` (one day), documented as only triggering the log.
- `:2299` `pub struct UnstampedOverwrite { inode, prior_modified }` + `Display` + `Error`. It
  names the file by inode id, because the core commits only know the id, not the key.
- `:2250` `commit_chunk_map_superseding`: the segmented-prior check that used to run after the
  record and batch were built (base `:2214-2219`) is hoisted to the top, then the stamp
  (`overwrite_modified(...)?`), then the record. That is the order the brief asks for (new
  refusal after the segmented one). No store access happens between, so hoisting the shape
  check changes no outcome. The stamp was `modified: meta.modified` (base `:2208`).
- `:2451` `commit_chunk_map_superseding_leased`: segmented check → stamp → leases. The stamp
  was `meta.modified` (base `:2287`). The refusal comes **before** the lease read for the same
  reason the shape does (doc at `:2427-2432`): a lapsed lease answers `Ok(Conflict)`, the retry
  answer, and an unstamped publication cannot be fixed by retrying.
- Field docs: `ObjectMeta::modified` (`:1587-1612`, base `:1587-1588`) and
  `InodeRecord::modified` (`:1661-1683`, base `:1638-1641`).
- `crates/core/src/write.rs:63-71` (`WritePlan::object_meta`: a default plan is now refused on
  an overwrite of a stamped file) and `:314-320` (`commit_overwrite` doc).
- Unit tests, `metadata.rs:5558` `mod overwrite_modified_rule` (5 tests): the pure values;
  both commits return the typed error by **downcast** with the record byte-identical and no
  `orphan:` row; the segmented refusal outranks the stamp refusal; the leased commit refuses an
  unstamped overwrite before reading a reaped lease (and the same overwrite stamped still gets
  `Conflict`); the warning is logged past a day (with both readings and the lead, also at
  `u64::MAX`) and not at exactly a day.

### S3 gateway: the shown time (`crates/gateway-s3/src/lib.rs`)

- `:3051` `fn shown_modified(stored, now_secs) = stored.map(|m| m.min(now_secs * 1000))`. It
  stays in milliseconds so `http_date` / `iso8601` take it unchanged: `floor(min(s, n*1000) /
  1000) == min(floor(s/1000), n)`. `None` stays `None` (field still omitted). A stored time past
  year 9999 is now shown as the response time. Doc carries leg (B)'s shown-side text.
- `:3065` `fn date_header(now_secs)`: the response's `Date`, from the same reading (see the next
  section for why).
- `:2994-3013` `epoch_secs_now`: doc and wall-clock-exempt comment extended to name the second
  lifecycle it now serves (RFC 9110 §8.8.2.1's cap is defined against the origin server's own
  clock). No new clock read site was added.
- One clock read per response, used for the cap, the date preconditions and `Date`:
  `serve_get` ranged arm `:2435`, unranged arm `:2472`, `serve_head` `:2517`, and
  `list_objects` `:927-929`, which caps every row before `compute_page` / `render_contents`.
- Renderers take `now_secs` and set `Date`: `apply_object_headers` `:2573` / `:2596` (200 and
  206, GET and HEAD), `not_modified_response` `:2714` / `:2724` (the 304), listing `:958`.
  `precondition_response`, `full_object_response`, `partial_content_response` and
  `head_partial_content_response` just thread `now_secs` through.
- `evaluate_conditionals` (`:2895`): body unchanged. Its callers now hand it the shown time, so
  the comparisons at `:2910` (If-Unmodified-Since) and `:2925` (If-Modified-Since) judge the
  shown value. `parse_http_date` still accepts a future date.
- `render_contents` (`:1048`) is unchanged and documented as rendering what it is handed. So
  `a_contents_row_omits_unpresentable_recorded_metadata` and
  `a_contents_row_omits_last_modified_when_the_store_never_recorded_one` keep their
  expectations, and `http_date` / `iso8601` still return `None` past year 9999.
- Tests: new `the_shown_time_is_the_earlier_of_the_stored_time_and_the_clock` (`:4863`). The
  handler test `an_unrenderable_stored_modified_omits_last_modified_instead_of_malforming_it`
  (base `:5185`) changes expectation as the brief says and is renamed
  `a_stored_modified_past_year_9999_is_shown_as_the_response_time` (`:5350`): `Last-Modified`
  now equals that response's `Date` and is a valid IMF-fixdate. The `http_date` boundary
  assertions are kept.

### Docs

- `docs/design/architecture/05-building-block-view.md:205-238` extends the object-metadata
  paragraph (base `:201-204`): the stored rule, no cap / no correction and why, the warning,
  the refusal and #658's obligation, what S3 shows, same-second and skew effects, `ETag`, the
  future-time behaviour and the mirror exception with re-sync by `ETag`, and the ADR-0047
  sentence (architecture doc only). No ADR added or edited.

### Test

- `crates/server/tests/last_modified_monotonic.rs` (new, copied into the bundle). Legs
  (1)–(8) as the brief lists them, plus one sanity test of the SDK date helpers.

## Why the gateway sets `Date` itself (the brief's Citations got hyper's timing wrong)

The brief says hyper writes `Date` "when it encodes the response head, after the handler has
read its clock; that is why leg (7)'s window holds". I checked hyper 1.10.1 (the locked version)
and it is not quite so:

- `hyper-1.10.1/src/common/date.rs`: `Date` is a thread-local cache. `check()` re-renders only
  when `now > next_update` (the next second boundary), and `extend()` copies the cached bytes.
- `hyper-1.10.1/src/proto/h1/dispatch.rs:148`: `T::update_date()` runs at the top of
  `poll_inner`, **before** `poll_loop` polls the service future. Encoding (`role.rs:961-965`)
  then writes the cached bytes.

So hyper's `Date` is the second at which that poll **started**, and the handler's clock read
comes later in the same poll. If a second boundary falls between them, hyper sends `Date: S`
while the cap gives `Last-Modified: S+1`. That breaks RFC 9110 §8.8.2.1, and leg (7) would flake.
The window is the handler's work in its last poll (SigV4 check + metadata read), roughly
50–500 µs, so about 5e-5 to 5e-4 per request. Legs (7) and (8) send about 40 requests, which
comes to up to about 2% flaky runs.

Fix: the three renderers that show a `Last-Modified` set `Date` from the same `now_secs`, so
`Last-Modified ≤ Date` holds by construction. hyper writes no `Date` when the response already
has one (`role.rs:840-841` sets `wrote_date`, `:961` skips its own). HTTP/2 uses
`or_insert_with` (`proto/h2/server.rs:480`). Cost: 3 header sets (`:958`, `:2596`, `:2724`)
plus one `now_secs: u64` parameter threaded through 6 private functions. Responses with no
`Last-Modified` (412, 416, errors, PUT) keep hyper's `Date`. The brief did say "if it is
missing, sending it is part of this slice"; here it was present but could be one second early.

## Choices and alternatives ruled out

1. **Cap once in the handler, not inside each renderer.** Capping inside `render_contents`
   would add a `now_secs` argument to it, change its 5 unit-test call sites
   (`lib.rs:4667-4795` region on base), and flip `a_contents_row_omits_unpresentable_recorded_
   metadata`'s year-10000 expectation, which the brief asks to keep. For consistency, objects
   use the same pattern: the handler reads the clock, replaces `meta.modified` /
   `read.modified` with the shown value once, and every renderer and `evaluate_conditionals`
   use it. One conversion per response, at the layer that reads the clock.
2. **Reuse `epoch_secs_now()`** rather than adding a millisecond clock read. One reading then
   serves the RFC-850 pivot, the cap and `Date`, and no new `disallowed_methods` exemption is
   needed.
3. **Error type.** A new struct, not a `ChunkMapError` variant: that enum is about chunk-map
   shape, and maintenance passes classify a `ChunkMapError` as an object-local shape fault.
   Not `Conflict`, which callers retry (it would loop). The rule takes `inode` only to build the
   error and the warning; it reads no clock.
4. **No server change.** Both PUT paths already stamp (`crates/server/src/lib.rs:274-278`
   buffered, `:449-453` streaming). The only production overwrite caller is `commit_written`
   (`:309`, overwrite at `:315`). `wyrd put` only creates.
5. **No new DST campaign.** The rubric asks for seeded Tier-0 DST coverage on a *new
   destructive or concurrent path*. This adds none: the stamp is a pure function of the `prior`
   the existing CAS already pins (`require(key, encode(prior))`), so the concurrency semantics
   are unchanged. The existing DST overwrite campaigns (`crates/dst/tests/concurrency.rs:106`,
   `commit_ambiguity.rs:237/640/786/958`, `network.rs:532`,
   `tikv_await_commit_interleaving.rs:140`) overwrite unstamped-over-unstamped records, which
   still commit, and they run under `cargo xtask ci` below.
6. **Statics gate.** My first core log test installed a global `tracing` default (the
   `read_repair.rs` pattern). The ADR-0035 statics gate scans `crates/core/src`, including test
   modules, and forbids `set_global_default` there (`xtask/src/main.rs:1249-1254`), so it would
   have failed `cargo xtask ci`. I removed it. Installing the scoped dispatcher re-registers
   callsite interest, and I kept the over-a-day cases out of the other core unit test, so the
   log test is the only one in that binary that reaches the warning. The full core unit binary
   passed 3 runs in a row; `cargo xtask statics` passes.
7. **Typed refusal in the server test.** C4-verify compiles the new test against the reverted
   base, where `UnstampedOverwrite` does not exist. So the server test recognises the refusal
   by its `Debug` rendering (`starts_with("UnstampedOverwrite")`) plus `Err` (rules out
   `Ok(Conflict)`) plus byte-identical stored bytes and the same `modified` / `version`. The
   core unit tests downcast to the type by name.
8. **Pre-epoch host clock (not guarded, out of scope).** `epoch_secs_now()` falls back to 0,
   so on a host whose clock reads before 1970 every stored time would show as 1970 and `Date`
   would be 1970. hyper itself cannot run there: `httpdate-1.0.3/src/date.rs:49-51`
   `expect("all times should be after the epoch")` panics on its own `Date` render. The brief
   scopes the pre-epoch clock out.
9. **rustdoc.** Not gated. `wyrd-core` already fails `RUSTDOCFLAGS=-D warnings` with about 20
   pre-existing link errors. I turned my one new public-to-private link into plain code, so the
   patch adds none.

## Refuting my own test (forced)

- **(a) Genuine red? Yes.** `engine/scripts/run-verify.sh` (the C4-verify runner, lane 1, base
  `origin/main` = `8ebd1c0`), on the final `patch.diff`: GREEN 9/9; RED with
  `crates/core/src/metadata.rs`, `crates/core/src/write.rs`, `crates/gateway-s3/src/lib.rs`
  and the doc reverted and the test kept, 5 of 9 fail, `PASS — red without the fix, green with
  it`, exit 0. The red cases fail as the brief's Falsifiability predicts:
  - (1)/(2): `left: Some(1792325516116)` (host clock) vs `right: Some(1792329116501)` (seed+1);
  - (6): `Some(1792325516117)` vs `Some(1792498316501)`;
  - (5): 24 cases broke (12 per API): equal, ahead in the same second, ahead, both refusals
    ("must be refused with a typed error, not answer Committed (stored modified now None)"),
    two days ahead, `u64::MAX` prior, two of three saturation cases, and all three chains. The
    behind ×2, legacy, unstamped-over-unstamped and both-at-max cases pass on base by design;
  - (7): `before the overwrite: HEAD ahead: the shown time 1792329116 ... must lie between ...
    (1792325516) and the response's own Date (1792325516)`;
  - (3), (4), (8) pass on base by design (they pin that the ordinary case is untouched and that
    there is no visible lead).
  - The red is an assertion failure in tests that ran, not a build error: the file uses only
    pre-existing API.
- **(b) Production path? Yes.** Legs (1)–(4), (6) drive the real `wyrd_server::Gateway` over
  redb + the fs chunk store through `put_object`, `put_object_streaming` and
  `ObjectGateway::head_object`. Leg (5) calls the two public core commits directly. Legs
  (7)–(8) start the real `S3Gateway::serve` on a loopback `TcpListener` and send SigV4-signed
  raw HTTP/1.1 through hyper → axum → `serve_get` / `serve_head` / `list_objects`. No mock and no
  copy of the rule. Wire dates are parsed and formatted with `aws-smithy-types`, not the
  gateway's own formatter.
- **(c) Fixture includes the fault? Yes, with one honest limit.** The fast-clock fault is a
  stored stamp ahead of this host's clock, written through the ordinary write path. The store
  keeps no record of which gateway stamped it, so it is byte-identical to what a fast gateway
  leaves. Real two-host skew is not reproducible in-process (`now_millis` reads the host clock);
  the brief accepts this stand-in and says the registered `libfaketime` leg is not needed. The
  unstamped overwrite is the real core call with `modified: None`. The future-time render fault
  is a file stored an hour ahead, served through the real S3 surface.

## Gate results (in `$PDCA_WORKTREE`)

- `cargo fmt --all -- --check`: clean (formatter applied to every touched file).
- `cargo clippy -p wyrd-core -p wyrd-gateway-s3 -p wyrd-server --all-targets`: clean. It caught a
  doc line starting with `+` in the test's module doc, which is fixed.
- `typos` over the touched files: clean.
- `cargo xtask ci` (the gating C4-ci command) on the final tree: **`xtask ci: all checks
  passed`, exit 0.** That includes `typos`, `lint_docs.py` (OK), `render_site.py --check` (99
  pages, link audit OK, so the `docs-renderer` dependency was present and exercised), the
  gitlink/unsafe/blackbox guards, fmt, workspace clippy and build, `cargo test --workspace`
  (215 `test result: ok`, 0 failed across the run), cargo-machete, `cargo deny check`,
  `cargo deny --all-features --config deny-all-features.toml check advisories` (`advisories
  ok`), licenses/bans/sources, conformance vectors, `xtask statics`, `xtask deploy-guard`, and
  the madsim DST clippy + seeded tests. All three of the brief's External dependencies
  (`cargo deny all-features advisories`, `typos`, `docs-renderer`) were present, so there is no
  NEEDS-HUMAN external-dependency item.
- Raw logs (in the harness scratch dir `$PDCA_SCRATCH` =
  `/var/tmp/pdca/wyrd-pdca-9c587031/issue_768`): `pdca-builder-768-verify2.log` (final
  red→green run), `pdca-builder-768-verify.log` (first run, same verdict, before a doc-only
  clippy fix in the test), `pdca-builder-768-xtask-ci.log` (the full gate).
- `patch.diff` was regenerated after the last edit and byte-compared against the worktree's
  `git diff` (match). The bundle's `last_modified_monotonic.rs` is byte-identical to the
  worktree copy.

## Open for sign-off (not defects in this patch)

- Brief Open question 2 (a mirror that already holds a future time) is still the
  maintainer's call. Leg (7)'s "earlier than the seed" assertion pins that the exception is
  real, and the docs say to re-sync such a mirror by `ETag` or with a full copy.
- Intended wire changes: a stored time past year 9999 is now shown as the response time instead
  of being omitted; every 200/206/304/listing response now carries a handler-set `Date`.
