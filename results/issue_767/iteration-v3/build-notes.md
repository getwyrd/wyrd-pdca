# Build notes — issue 767 / listing-last-modified-second-granularity (iteration 3)

Target: getwyrd/wyrd `main` @ `7bf7eb9` (worktree `/home/eddie/wyrd/wyrd.pdca-wt-l1`).
Upstream `main` is still at `7bf7eb9` as of 2026-10-15 (`gh api repos/getwyrd/wyrd/commits`).
Line numbers are on the patched tree unless marked "base".

## What changed from iteration 2

The production change and the architecture-doc sentence are **byte-identical** to round 2
(checked by hashing each file's hunk against `iteration-v2/patch.diff`). The round-2 review
passed every item on them, and the adversary could not break them. Only the test file
changed.

| Round-2 item | What this round does |
|---|---|
| Optional reuse: the test's hand-written calendar code and two date parsers duplicate `aws-smithy-types`, already a dev-dependency (`crates/server/Cargo.toml:134`) | Done. `days_from_civil`, `digits`, `check_civil`, `parse_iso_millis` and `parse_imf_millis` (about 110 lines) are gone. Two small wrappers replace them: `parse_listed` (`crates/server/tests/s3_last_modified_agreement.rs:483-486`) and `parse_header` (`:489-492`). They call `aws_smithy_types::DateTime::from_str` with the same formats `aws-sdk-s3` itself uses for these fields: `Format::DateTimeWithOffset` for a listing's `LastModified` (aws-sdk-s3 1.152.0, `src/protocol_serde/shape_object.rs:30-33`) and `Format::HttpDate` for the `Last-Modified` header (`src/protocol_serde/shape_head_object_output.rs:226`). So check (3) now reads both values exactly as a sync tool built on the AWS SDK reads them. The file went from 882 to 807 lines. The exact wire-string checks (1) and (2) stay as they were (`:766-777`). No new dependency: `aws-smithy-types` 1.8.1 is the only version in `Cargo.lock` (`:434-435`) and is what the SDK links. |
| (same item) keep the oracle independent of the gateway's formatter | The parsers are the SDK's, not the gateway's. The soundness test was rewritten as `the_sdk_date_parsers_tell_a_fraction_from_a_whole_second` (`:500-514`). It checks the SDK parsers recover the known instants, that the listing parser keeps `.750`, and that the exact pre-fix wire value `2023-11-14T22:13:20.750Z` parses to a different instant than `Tue, 14 Nov 2023 22:13:20 GMT`. Without that last point, check (3) could pass on the unfixed gateway. |
| C4-ci red on `cargo deny --all-features … check advisories` | Not build work, and not fixed in this patch. Reasons, severities, and a ready-to-paste fix for a **separate** PR are in the "NEEDS-HUMAN" section below. |

What the old parsers did that the SDK parser does not: check the weekday against the date.
The SDK's HTTP-date parser skips the weekday (`aws-smithy-types-1.8.1/src/date_time/format.rs:283-360`).
This loses no coverage, because check (2) compares the header to the exact string
`Tue, 14 Nov 2023 22:13:20 GMT`, weekday included, before any parsing.

## The change (production, unchanged from round 2)

1. **`iso8601` renders whole seconds**: `crates/gateway-s3/src/lib.rs:634-648` (base
   `:624-638`). The `let millis = epoch_millis % 1_000` line (base `:627`) is removed, and the
   format string ends in a literal `.000Z` (`:646`). `:637` is the same `/ 1_000` truncation
   `http_date` applies (`:3204`), so for every stored `u64` both formatters name the same
   second. The year-9999 bound is unchanged.
2. **Both doc comments state the rule**: `iso8601` at `:620-629`, `http_date` at `:3193-3197`.
   Each points at the other and names RFC 7231 §7.1.1.1 and ADR-0047 (storage stays in
   milliseconds).
3. **Architecture doc**, one sentence after the serde/CAS sentence
   (`docs/design/architecture/05-building-block-view.md:194-196`): the store keeps `modified`
   in epoch milliseconds, and every S3 surface reports it at whole seconds. This meets the
   docs-currency rule (`AGENTS.md:154-157`).
4. **In-file unit test** `listing_and_header_formatters_both_truncate_to_whole_seconds`
   (`crates/gateway-s3/src/lib.rs:4622-4656`): `.000`, `.750`, `.999`, and the last renderable
   millisecond.

This covers every surface that exists today. `iso8601` has one caller, `render_contents`
(`:1034`), which serves both v1 and v2 listings. `http_date` has two: `apply_object_headers`
(`:2499`, shared by GET, ranged GET and HEAD) and the 304 response (`:2607`). A grep for
`LastModified` / `last-modified` across `crates/` finds no other renderer (DeleteObjects'
`LastModifiedTime` at `:1970` is a refused input field, not an output). The conditional
request comparison already truncates to seconds (`:2786`, `:2801`). A future surface such as
ListParts (#508) gets the rule by reusing `iso8601`.

## Alternatives ruled out

- **Fractions in the header**: impossible. IMF-fixdate has no fraction field.
- **Documentation-only answer**: the brief's live S3 sample (3,000 of 3,000 on `.000Z`) rules it out.
- **Round to the nearest second**: would disagree with `http_date` and with the conditional
  comparison, which both truncate.
- **Truncate at storage time**: out of scope, and #768's ordering relies on milliseconds.
- **Keep the hand-written test parsers**: they work, but they are ~110 lines of calendar code
  the reviewer has to check, and the SDK parser is both shorter and closer to what clients do.
- **Use the full `aws-sdk-s3` client for the requests too**: rejected. The SDK would decode the
  timestamps before the test sees them, so checks (1) and (2) could no longer assert the exact
  wire strings. Raw signed HTTP plus the SDK's date parser gives both.

## Test: `crates/server/tests/s3_last_modified_agreement.rs` (new file)

Harness, following the cited peers: the bucket marker is seeded as raw bytes, as
`s3_list_objects.rs:82-88` does (`seed_bucket`, `:96-102`). Two flat objects are written
through `wyrd_core::write::{plan_write,intent,write_fragments,commit_create,release}`, as
`segmented_object_read.rs:55-82` does (`write_flat`, `:107-138`), with
`plan.object_meta.modified` set to `1_700_000_000_750` and `1_700_000_000_000`. That is the
field the gateway's PUT stamps (`crates/server/src/lib.rs:251-255`), under the dirent
`"{bucket}/{key}"` beneath `ROOT` (`crates/server/src/lib.rs:708-712`). All of it happens before
`Gateway::new` (`:155-201`). Requests are raw SigV4-signed HTTP with the canonical query signed
(`s3_list_objects.rs:34-37`; `send`, `:393-444`).

For each object: `GET /stamps?list-type=2`, `GET /stamps` (v1), `HEAD` and `GET` on the key
(`surfaces`, `:720-763`). Then `assert_agreement` (`:765-793`) checks:
(1) both listings render exactly `2023-11-14T22:13:20.000Z`;
(2) HEAD and GET carry exactly `Tue, 14 Nov 2023 22:13:20 GMT`;
(3) parsed with the SDK's parsers, both listing values and the GET header equal the HEAD
header's instant.

The round-2 fixes stay: every socket and store await is under a 30 s deadline (`DEADLINE`
`:67`, `bounded` `:88-92`); the serve task is aborted on drop (`:142-152`); the strict response
reader (`parse_response` `:247-338`, `dechunk` `:344-387`) refuses bad framing, proven by
`the_response_reader_rejects_incomplete_or_malformed_framing` (`:516-702`).

Clock: the one wall-clock read is the SigV4 `x-amz-date` in `amz_date_now` (`:203-211`), with a
per-site `#[allow(clippy::disallowed_methods)]` and a comment saying the wall clock owns the
request-signing lifecycle. The asserted `modified` values are constants.

### Red → green

There is no single-test subcommand in the project runner (`cargo xtask` subcommands,
`xtask/src/main.rs:119-150`), and `engine/scripts/run-verify.sh` writes to a sibling checkout
outside my roots and runs at Check anyway. So the quick legs used the same `cargo test` the
verify script runs (`run-verify.sh:441`), bounded by `timeout`:
`timeout 1200 cargo test --locked -p wyrd-server --test s3_last_modified_agreement`.
The full runner (`engine/xtask.sh ci`) was run too (below).

- **Fixed tree**: `4 passed; 0 failed`.
- **Fix reverted** (`git checkout -- crates/gateway-s3/src/lib.rs`; test and doc kept):
  `off_second_modified_lists_and_heads_as_the_same_instant ... FAILED` at `:767`,
  `ListObjectsV2 <LastModified> of off-second  left: "2023-11-14T22:13:20.750Z"  right:
  "2023-11-14T22:13:20.000Z"`; `3 passed; 1 failed`. The other three pass on both sides by
  design: the on-second object renders the same before and after (the brief's "renders the
  same as it does today"), and the two oracle tests do not depend on the fix.
- **Check (3) alone, fix reverted** (one-off: checks (1) and (2) skipped in a temporary copy,
  then the file restored byte-for-byte with `cmp`): fails at `:770` with
  `v2 listing (2023-11-14T22:13:20.750Z) and HEAD (Tue, 14 Nov 2023 22:13:20 GMT) disagree on
  off-second's instant  left: 2023-11-14T22:13:20.75Z  right: 2023-11-14T22:13:20Z`. So the
  brief's "(1) and (3) fail today" holds for both, not just for whichever runs first.
- **Fix re-applied**: `4 passed; 0 failed`.
- Every symbol the test uses predates #767 (`aws-smithy-types` included), so on base it
  compiles and fails on an assertion, not a build error.

## Self-refutation

- **(a) Genuine red?** Yes. With only `crates/gateway-s3/src/lib.rs` reverted, the off-second
  wire test fails on `.750Z` vs `.000Z`. Check (3) on its own also fails on the reverted tree
  (both outputs above). The soundness test shows the SDK parser cannot hide the gap: it
  parses the exact pre-fix value to a different instant than the header.
- **(b) Production path?** Yes. The test drives the real `S3Gateway::serve` on a loopback TCP
  listener and the real `wyrd_server::Gateway` over redb and the fs chunk store, so the
  production `render_contents` → `iso8601` and GET/HEAD → `http_date` paths run. Requests are
  signed with the gateway's own `sigv4::sign`. Nothing is mocked. The date parsers are the
  AWS SDK's, not a copy of the gateway's formatter.
- **(c) Fixture includes the fault?** Yes. The fixture stores `1_700_000_000_750`, the exact
  off-second value that disagreed, as a real committed object. The on-second control sits in
  the same bucket, so the listing reader has to pick the right row.

## NEEDS-HUMAN (not build work): the base-wide advisory red on C4-ci

`cargo xtask ci` on this tree fails at `cargo deny --all-features --config
deny-all-features.toml check advisories`, on **RUSTSEC-2026-0346** and **RUSTSEC-2026-0349**
against `rustls 0.21.12` (`Cargo.lock:3294-3295`). This is the same red as rounds 1 and 2. I did
not fix it in this patch. Facts for the decision:

- **Not caused by this patch.** The patch touches no `Cargo.toml`, `Cargo.lock` or `deny*.toml`.
  Every branch cut from `7bf7eb9` fails the same way. Both advisories were published on
  2026-10-14, the base's own date (`~/.cargo/advisory-dbs/.../crates/rustls/RUSTSEC-2026-0346.md`,
  `…-0349.md`, `date = "2026-10-14"`).
- **TiKV-only.** The chain is `tikv-client 0.4.0` → `tonic 0.10` → `tokio-rustls 0.24.1` →
  `rustls 0.21.12`, behind the off-by-default `tikv` feature. The default-graph check
  (`cargo deny check`) passes: `advisories ok, bans ok, licenses ok, sources ok`.
- **No fix to apply.** Both advisories say "patched >= 0.23.46". The 0.21 line has no fixed
  release, and `deny-all-features.toml:66-70` already records that `tikv-client` has no release
  carrying the TLS-stack upgrade.
- **Severity, from the CVSS vectors in the advisory files:** 0346 is
  `AV:L/AC:H/PR:H/UI:R/C:H/I:N/A:N` (about 4.0, medium); 0349 is `AV:N/AC:H/PR:N/UI:N/C:L/I:N/A:N`
  (about 3.7, low). Neither is high or critical, so the "immediately if a NEW high/critical
  advisory enters this tree" trigger (`deny-all-features.toml:83`) does not fire. My reading,
  which a human should confirm: 0346 only leaks through a custom `ClientSessionStore` that
  `Debug`-prints session values, and 0349 is about losing the post-quantum `X25519MLKEM768`
  group, which I believe rustls 0.21 does not offer. So both are probably not reachable from
  the TiKV leg.
- **Why not here.** Adding an `ignore` silences a security wall. The repo treats that as a
  deliberate, reviewed decision with review triggers (`deny-all-features.toml:50-90`). Putting
  it inside a timestamp-rendering PR would hide it from that review, and the rubric says an
  out-of-scope finding gets a decline with an issue reference, not an in-PR fix. It also
  blocks every open slice on this base, so one separate PR that all slices rebase onto is the
  cheaper fix: 2 entries, about 12 lines, in one file.

Proposed handling: a separate dependency PR (or a human decision to accept this slice with the
known baseline red). If the human goes with the PR, these entries follow the file's existing
#543 pattern (compare `RUSTSEC-2026-0258` at `deny-all-features.toml:146-152`) and go at the end
of the `ignore` list:

```toml
    # MEDIUM (4.0). The derived `Debug` of TLS client session values prints the TLS 1.2
    # master secret / TLS 1.3 resumption PSK; only a custom `ClientSessionStore` that formats
    # them leaks. rustls 0.21.12 <- tokio-rustls 0.24 / tonic 0.10 <- tikv-client 0.4.0.
    # Patched only in rustls >= 0.23.46; the 0.21 line has no fix.
    { id = "RUSTSEC-2026-0346", reason = "rustls 0.21.12 via tonic 0.10 <- tikv-client 0.4.0 behind the off-by-default `tikv` feature; the 0.21 line has no patched release (>= 0.23.46 only). Absent from the shipped graph. No fix reachable: tikv-client 0.4.0 is abandoned upstream (#543)." },

    # LOW (3.7). An unauthenticated ServerHello can pin later connections to a classical key
    # exchange group (loss of post-quantum confidentiality). Same rustls 0.21.12 chain.
    { id = "RUSTSEC-2026-0349", reason = "rustls 0.21.12, same tikv-client-only chain as RUSTSEC-2026-0346; no patched 0.21 release. Absent from the shipped graph (#543)." },
```

The file counts its tikv entries in two places ("The seven tikv entries below", `deny-all-features.toml:52`;
"All seven", `:105`); that PR should change both to nine. I did not open
an issue or PR for this; that is the human's call. A matching issue could be opened with
`gh issue create --repo getwyrd/wyrd --title "deps: RUSTSEC-2026-0346/0349 (rustls 0.21.12 via tikv-client) turn the all-features advisory wall red"`.

## Full gate (`engine/xtask.sh ci`, the project runner)

Run on the final tree (log in scratch, now removed):

- **Passed**: `typos`; `lint_docs.py`; `render_site.py --check` (docs renderer and link audit);
  gitlink, unsafe and blackbox guards; `cargo fmt --check`; `cargo clippy --workspace --exclude
  wyrd-dst --all-targets`; `cargo build`; `cargo test --workspace --exclude wyrd-dst` (197 test
  binaries, all `ok`, including this file's `4 passed`); `cargo-machete`; `cargo deny check`.
- **Failed, run stopped**: the all-features advisories check (section above).

Stages after the failing one, run by hand in `run_ci` order (`xtask/src/main.rs:1683-1687`):

- `cargo deny --all-features check licenses bans sources`: `bans ok, licenses ok, sources ok`.
- `cargo xtask conformance`: `5 valid + 6 invalid vectors pass`.
- `cargo xtask statics`: `no DST-reachable shared mutable global state (ADR-0035)`.
- Orchestrator (deploy) guard: no subcommand; it scans `crates/` for `kube::`, `k8s_openapi::`
  and similar (`xtask/src/deploy_guard.rs:28-35`). The new test has none of them.
- `cargo xtask dst` (`cargo clippy -p wyrd-dst` and `cargo test -p wyrd-dst`, both
  `--cfg madsim`): exit 0, `85 passed; 0 failed` (1 ignored) summed over all targets.

So every stage except the base-wide advisories check is green on this tree.

## Commit-readiness

- `cargo fmt --all -- --check`: clean. No line in the new file is over 100 characters.
- `cargo clippy --locked -p wyrd-gateway-s3 -p wyrd-server --all-targets -- -D warnings`
  (workspace lints): clean.
- `typos` on the three touched files: clean. `lint_docs.py`: `lint_docs: OK`.
- The repo has no `.pre-commit-config.yaml` or `.githooks`; the commit-time checks are the
  `cargo xtask ci` prose and fmt stages, all green above.
- `git apply --check -R patch.diff` on the worktree: applies cleanly, so the patch applies
  forward on `7bf7eb9`.

## Prior art

Unchanged since round 2, and the round-2 reviewer re-checked it independently (T4 PASS: all 381
closed PRs paged, changed-file lists of all 19 closed-unmerged PRs, path history of all three
touched paths). Re-checked today: upstream has no open PRs and `main` has not moved.

## Housekeeping

Scratch dirs, both under `$PDCA_SCRATCH`: `pdca-builder-767-redleg` (saved gateway diff and a
test-file backup for the one-off check-(3) run) and `pdca-builder-767-ci` (gate logs). Both are
removed. Six older files, `pdca-builder-767-{ci,ci2,conformance,dst,statics}.log` and
`pdca-builder-767-lib.rs.fixed`, dated 2026-10-14 (round 1), are still in `$PDCA_SCRATCH`; this
round did not create them, so I left them for the harness. The worktree keeps one `git add -N` (intent-to-add) entry for the new test file so `git diff`
includes it. Nothing is committed, pushed, or opened as a PR.
