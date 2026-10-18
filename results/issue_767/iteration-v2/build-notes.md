# Build notes — issue 767 / listing-last-modified-second-granularity (iteration 2)

Target: getwyrd/wyrd `main` @ `7bf7eb9` (worktree `/home/eddie/wyrd/wyrd.pdca-wt-l1`).
Line numbers are on the patched tree unless marked "base".

## What changed from iteration 1

The production fix is the same as round 1. Nobody objected to it, and the brief's evidence
(3,000 of 3,000 live S3 listing values on `.000Z`) still picks it. This round fixes what the
review and sign-off flagged:

| Round-1 finding | What this round does |
|---|---|
| Arch-doc sentence sat between the field list and "Each is optional…", so "Each" read as if it meant the two wire surfaces | The sentence now comes **after** the serde/CAS sentence, at the end of the `inode:` bullet (`docs/design/architecture/05-building-block-view.md:194-196`). "Each" again follows the three-field list directly, and the stub line is gone. |
| No deadline on connect / write / `read_to_end` (await discipline) | Every await on the socket and the store goes through `bounded()` (`crates/server/tests/s3_last_modified_agreement.rs:85-89`, `tokio::time::timeout`, 30 s `DEADLINE` at `:64`). The full exchange in `send` (connect, write, flush, read to close) runs as one bounded future (`:423-433`). Seeding (`:156-178`) and the listener bind are bounded too. The serve task is held in `Server` and aborted on drop (`:139-149`), so no spawned task outlives the test. |
| Response reader accepted bad framing: invalid chunk size treated as end, missing terminal chunk accepted, chunk CRLF not checked, `Content-Length` ignored, no HEAD rule | New `parse_response` (`:244-335`) and `dechunk` (`:341-384`) return `Err` and never a body. They reject: no header terminator; a status line that is not `HTTP/1.1` + three digits; obs-fold, a header line without a colon, bare CR/LF; a non-decimal, signed or repeated `Content-Length`; `Content-Length` together with `Transfer-Encoding`; neither framing on a non-HEAD response; a body shorter or longer than `Content-Length`; **any** body bytes on a HEAD response; a chunk size that is empty, signed or not hex; chunk data shorter than its size or not followed by CRLF; EOF before the terminal chunk; a trailer section with no final CRLF, or a trailer field without a colon; a chunk size that overflows; bytes after the terminal chunk. A new test, `the_response_reader_rejects_incomplete_or_malformed_framing` (`:591-777`), runs 24 malformed inputs and 4 well-formed ones. Each malformed one must be `Err` **for its stated reason** (the error text is matched), so no case can pass on an earlier, unrelated refusal. Each well-formed one must decode to exactly its body. That includes the reviewer's example: one complete data chunk followed by EOF with no `0\r\n\r\n`. |
| (same family) other ways the oracle could pass vacuously | `Response::header` fails on a repeated field. `listed_last_modified` needs exactly one `<Contents>` row for the key and exactly one `<LastModified>` in it, and bounds the row at `</Contents>`. Each listing must be a whole `<?xml … </ListBucketResult>` document. v2 must carry `<KeyCount>` and v1 must not. The GET body must equal the stored bytes. The IMF parser checks the weekday against the date, and every numeric field's width and range. |
| C4-ci red on `cargo deny … advisories` | Not a patch problem. Evidence and proposed handling are below. |
| T4 Contribution: prior art not checked by every touched path | Done this round. Results below. |

## The change (production)

1. **`iso8601` renders whole seconds**: `crates/gateway-s3/src/lib.rs:634-648` (base
   `:624-638`). The `let millis = epoch_millis % 1_000` line (base `:627`) is gone, and the
   format string ends in a literal `.000Z` (`:646`). `:637` is the same `/ 1_000` truncation
   `http_date` applies (`:3204`), so for every stored `u64` the two formatters name the same
   second. The year-9999 bound is unchanged.
2. **Both doc comments state the rule**: `iso8601` at `:620-629`, `http_date` at
   `:3193-3197`. Each points at the other, and names RFC 7231 §7.1.1.1 and ADR-0047
   (storage stays milliseconds).
3. **Architecture doc**, one sentence (`docs/design/architecture/05-building-block-view.md:194-196`):
   the store keeps `modified` in epoch milliseconds, and every S3 surface reports it at
   whole seconds. This meets the docs-currency rule (`AGENTS.md:154-157`), since a listing
   response changes.
4. **In-file unit test** `listing_and_header_formatters_both_truncate_to_whole_seconds`
   (`crates/gateway-s3/src/lib.rs:4622-4656`) covers `.000`, `.750`, `.999`, and the last
   renderable millisecond (`253_402_300_799_999` → `9999-12-31T23:59:59.000Z` /
   `Fri, 31 Dec 9999 23:59:59 GMT`).

This fixes the rule, not just one formatter. `iso8601` has exactly one caller,
`render_contents` (`:1034`), which serves both v1 and v2 listings. The header side is the
single `http_date`, used by `apply_object_headers` (`:2500`, shared by GET, ranged GET and
HEAD) and by the 304 `not_modified_response` (`:2608`). The
conditional-request comparison already truncates to seconds (`:2786`, `:2801`). So every S3
surface that reports `modified` now reports the same second, and a future surface such as
ListParts (#508) inherits that by reusing `iso8601`. The stored record is not touched,
which keeps ADR-0047's millisecond storage and #768's ordering.

## Alternatives ruled out

- **Put `.mmm` in the header instead**: impossible. IMF-fixdate has no fraction field.
- **Documentation-only answer**: the brief's live S3 sample rules it out.
- **Round to the nearest second**: that would disagree with `http_date` and with the
  conditional-request comparison, which both truncate.
- **Truncate at storage time**: out of scope, and it breaks #768's ordering on milliseconds.
- **Bump the vulnerable dependency in this patch**: see the C4-ci section. It needs a
  `tikv-client` upgrade, not a lockfile tweak, and it is out of this brief's scope.

## Test: `crates/server/tests/s3_last_modified_agreement.rs` (new file)

Harness, following the cited peers: the bucket marker is seeded as raw bytes, as
`s3_list_objects.rs:82-88` does. Two genuine flat objects are written through
`wyrd_core::write::{plan_write,intent,write_fragments,commit_create,release}`, as
`segmented_object_read.rs:55-82` does. `plan.object_meta.modified` is set to
`1_700_000_000_750` and `1_700_000_000_000`. That is the field the gateway's PUT stamps
(`crates/server/src/lib.rs:251-255`), under the dirent `"{bucket}/{key}"` beneath `ROOT`
(`crates/server/src/lib.rs:708-712`). All of it happens before `Gateway::new`. Requests are
raw SigV4-signed HTTP with the canonical query signed (`s3_list_objects.rs:34-37`).

Each object goes through `GET /stamps?list-type=2`, `GET /stamps` (v1), `HEAD` and `GET`
on the key. The test checks (1) both listings render exactly `2023-11-14T22:13:20.000Z`,
(2) HEAD and GET carry exactly `Tue, 14 Nov 2023 22:13:20 GMT`, and (3) the in-test
parsers (Hinnant `days_from_civil`, written apart from the gateway's formatter) give the
same epoch millisecond for the listing values and the header values.
`the_independent_parsers_are_sound` proves the ISO parser keeps `.750`, so check (3) can
tell `.750` from `.000`.

What the live responses use (seen once with a throwaway header print, since removed): all
four use `Content-Length` (listing 529/524 bytes, HEAD and GET 15 bytes) with
`connection: close`. So the wire tests exercise the exact-length path and the
HEAD-no-body path. The chunked decoder runs only in the reader's unit test. It is there so
a future switch to a streamed (chunked) response is still read strictly.

Clock: the one wall-clock read, the SigV4 `x-amz-date`, sits in `amz_date_now`
(`:200-208`) with a per-site `#[allow(clippy::disallowed_methods)]` and a comment saying
the wall clock owns the request-signing lifecycle. The asserted `modified` values are
constants. Round 1 used a file-wide allow; this round narrows it to the one site, as
`clippy.toml` asks.

### Red → green

Quick leg: `timeout 1200 cargo test -p wyrd-server --test s3_last_modified_agreement` in the
worktree (bounded by `timeout`). The project runner has no single-test subcommand;
`./engine/xtask.sh ci` is the whole gate, and it was run too (below).

- **Fixed tree**: 4 passed.
- **Fix reverted** (`git checkout -- crates/gateway-s3/src/lib.rs`, test and doc kept):
  `off_second_modified_lists_and_heads_as_the_same_instant ... FAILED` with
  `ListObjectsV2 <LastModified> of off-second  left: "2023-11-14T22:13:20.750Z"  right:
  "2023-11-14T22:13:20.000Z"`. The other three pass. That is expected: the on-second
  object renders the same before and after (the brief's "renders the same as it does
  today" clause), and the two oracle tests do not depend on the fix.
- **Fix re-applied** (`git apply` of the saved diff): green again.
- The file uses only API that predates #767, so on base it compiles and fails on an
  assertion, not a build error.

## Self-refutation

- **(a) Genuine red?** Yes. With only `crates/gateway-s3/src/lib.rs` reverted, the
  off-second wire test fails on `.750Z` vs `.000Z` (output above). The new oracle tests
  also show the reader cannot go green on bad bytes: 24 malformed responses, each `Err`
  for its stated reason.
- **(b) Production path?** Yes. The test drives the real `S3Gateway::serve` on a loopback
  TCP listener, the real `wyrd_server::Gateway` over redb + the fs chunk store, and the
  production `render_contents` → `iso8601` and GET/HEAD → `http_date` paths. Requests are
  signed with the gateway's own `sigv4::sign`. Nothing is mocked or re-implemented. The
  in-test parsers are the oracle, not a copy of the formatter.
- **(c) Fixture includes the fault?** Yes. The fixture stores `1_700_000_000_750`, the
  exact off-second value that disagreed, as a real committed object, not a hand-built
  listing row. The on-second control object sits beside it in the same bucket, so the
  listing parser must pick the right row.

## Prior art by every touched path (round-1 T4 finding)

Searched 2026-10-15 against getwyrd/wyrd:

- **Open PRs**: there are none (`gh pr list --state open` → 0). So none touches
  `crates/gateway-s3/src/lib.rs`, `docs/design/architecture/05-building-block-view.md`,
  or any `crates/server/tests/s3_*`.
- **Closed, unmerged PRs**: 13 among the last 200 closed (back to #525). None touches any
  of those three paths.
- **PR search** on `s3_last_modified_agreement`, `LastModified`, `iso8601`, `767`,
  `last-modified granularity`: the only relevant hit is #609 (merged), the listing PR that
  added `iso8601`.
- **History**: `git log -S'millis:03'` → only `a27f907` (#609, where the `.mmm` fraction
  came in). `git log -S'.000Z'` → `a27f907` and `9585604` (#509, a DeleteObjects
  `LastModifiedTime` fixture, unrelated). The new test path has no history; it is a new
  file. Recent changes to the arch doc (`b2d3922`, multipart/restore-fence) are in other
  paragraphs. #768 (the overwrite rule, `:197-200` base) is the known conflict the brief
  already orders into a different wave.

## Commit-readiness

- `cargo fmt --all -- --check`: clean. rustfmt had silently left two lines over 100
  characters in the malformed-response table; they are now shortened with named consts,
  so rustfmt lays the table out fully and no line in the file is over 100 characters.
- `typos` on all three touched files: clean.
- `cargo clippy -p wyrd-gateway-s3 -p wyrd-server --all-targets` with workspace lints: clean.
- The repo has no `.pre-commit-config.yaml` or `.githooks`. The commit-time checks are the
  `cargo xtask ci` prose and fmt stages, all green below.

## Full gate (`./engine/xtask.sh ci`, the project runner) and the advisories red

Run on this tree. **Passed**: `typos`, `lint_docs.py`, `render_site.py --check` (docs
renderer and link audit), gitlink / unsafe / blackbox guards, `cargo fmt --check`,
`cargo clippy --workspace --all-targets`, `cargo build`, `cargo test --workspace --exclude
wyrd-dst` (197 test binaries, none failed, including this file's 4 tests),
`cargo-machete`, `cargo deny check`.

**Failed**, and the run stopped there:
`cargo deny --all-features --config deny-all-features.toml check advisories`, on
RUSTSEC-2026-0346 and RUSTSEC-2026-0349 against `rustls 0.21.12`.

Why this is not this patch's red, and why I did not fix it here:

- `cargo tree -i rustls@0.21.12 --all-features` shows the only path:
  `tikv-client 0.4.0` → `tonic 0.10.2` → `tokio-rustls 0.24.1` → `rustls 0.21.12`, reached
  only through `wyrd-metadata-tikv` under the off-by-default `tikv` feature. The default
  build's advisories check (`cargo deny check advisories`) passes.
- Both advisories say "Upgrade to >=0.23.46". The 0.21 line has no fixed release, so a
  `cargo update` cannot clear it. The fix is a `tikv-client` upgrade (a new `tonic` major
  and a new rustls major, through a pre-1.0 crate), or a time-limited `ignore` entry in
  `deny-all-features.toml` with a reason. Each is a dependency-policy decision (ADR-0003
  audit, `deny.toml`), which the repo treats as human-only. Each also changes manifests
  every other open slice rebases over. This brief's scope is the listing rendering.
- This patch touches no `Cargo.toml`, no `Cargo.lock`, and no deny config. The same red
  fails every branch off `7bf7eb9`.
- Proposed handling, for the human: open a dependency issue ("tikv feature pulls
  rustls 0.21.12, RUSTSEC-2026-0346/0349"), and decide whether a base-wide advisory on an
  off-by-default feature blocks unrelated PRs. Round 1 already parked this in
  `deferred-findings.json`.

**To get a complete result anyway**, I ran every stage after the failing one by hand, in
`cargo xtask ci` order:
- `cargo deny --all-features check licenses bans sources` (deny step 3): `bans ok,
  licenses ok, sources ok`.
- `cargo xtask conformance`: `5 valid + 6 invalid vectors pass`.
- `cargo xtask statics`: passed.
- `cargo xtask dst` (`clippy -p wyrd-dst` + `test -p wyrd-dst`, both `--cfg madsim`): exit 0,
  85 tests passed, 0 failed. It took about 25 minutes, mostly the `custodian` seed sweep
  (busy at ~120% CPU, not hung).
- The orchestrator (deploy) guard has no separate subcommand. It scans `crates/` for
  orchestrator imports, and this patch adds none (the only new file is a test that imports
  tokio, tempfile and `wyrd_*` crates).

So every stage other than the base-wide advisories check is green on this tree.

Small note: the background `ci` run compiled an earlier version of the new test file. After
it I made test-only edits: shortened two over-long lines, added two rejection cases (trailer
without a colon, overflowing chunk size), made every rejection case check its error text,
and used `checked_add` in the chunk-length check. No production or doc line changed after
the `ci` run. I re-ran fmt, clippy, typos, the docs lint and the red/green legs on the final
file (below).

## Final re-check on the shipped file

On the shipped tree (the one `patch.diff` was cut from):

- `cargo fmt --all -- --check`: clean. No line in the new file is over 100 characters.
- `cargo clippy -p wyrd-gateway-s3 -p wyrd-server --all-targets` (workspace lints, deny): clean.
- `typos` on the three touched files: clean. `python3 docs/publishing/tools/lint_docs.py`: clean.
- `timeout 1200 cargo test -p wyrd-server --test s3_last_modified_agreement`:
  - fix applied: `4 passed; 0 failed`;
  - fix reverted (`git checkout -- crates/gateway-s3/src/lib.rs`):
    `off_second_modified_lists_and_heads_as_the_same_instant ... FAILED`,
    `left: "2023-11-14T22:13:20.750Z"  right: "2023-11-14T22:13:20.000Z"`, `3 passed; 1 failed`;
  - fix re-applied: `4 passed; 0 failed`.
- `git apply --check -R patch.diff` against the worktree: applies cleanly.

Scratch dirs used, both under `$PDCA_SCRATCH`: `pdca-builder-767-redleg` (saved gateway diff
and a test-file backup for the one-off framing print) and `pdca-builder-767-ci` (gate logs).
Both removed at the end. The worktree keeps one `git add -N` (intent-to-add) entry for the
new test file, used to include it in `git diff`; nothing is committed.
