# Build notes — issue 767 / listing-last-modified-second-granularity (iteration 4)

Target: getwyrd/wyrd `main` @ `7bf7eb9` (worktree `/home/eddie/wyrd/wyrd.pdca-wt-l0`).
Upstream `main` is still `7bf7eb9` on 2026-10-15 (`gh api repos/getwyrd/wyrd/commits?sha=main`).
Line numbers are on the patched tree unless marked "base".

## Short version

The patch and the test are **byte-identical to round 3** (SHA-256 of `patch.diff` and of the
test file match `iteration-v3/`, prefix `5d11b52a…` and `a7276bc4…`). That is deliberate, not a
re-submission of a rejected approach: round 3's review found "No builder-fixable defect"
(`iteration-v3/check-review.md`, every C/T row PASS or NEEDS-HUMAN), and the only gating red
in all three earlier rounds is `cargo deny --all-features … check advisories` failing on two
rustls advisories that come from the base, not from this diff.

What this round changes is how that blocker is reported. Rounds 1–3 described it in prose, so
the auto-iterate loop kept treating a failing gate as build work and sent the bundle back to
Do three times. This round declares it with the exact `NEEDS-HUMAN external dependency:`
marker and a `[[doctor.checks]]` row (below). With `dependency_halt = true` in `pdca.toml`,
the driver can check that claim itself and hand the bundle to the human at sign-off instead
of looping. I also ran the full gate once with the two advisories ignored (in the worktree
only, then reverted), to give the complete CI result past the deny step that reviewers asked
for in rounds 2 and 3.

## NEEDS-HUMAN: the all-features advisory baseline

NEEDS-HUMAN external dependency: all-features-advisory-baseline — the C4-ci gate (`cargo xtask ci`) stops at `cargo deny --all-features --config deny-all-features.toml check advisories` on RUSTSEC-2026-0346 and RUSTSEC-2026-0349 against `rustls 0.21.12` (`Cargo.lock:3294-3295`, via `tikv-client 0.4.0` → `tonic 0.10.2` → `tokio-rustls 0.24.1`). This patch touches no `Cargo.toml`, `Cargo.lock` or `deny*.toml`, so every branch cut from `7bf7eb9` fails the same way. A green C4-ci needs a disposition on the base (an `ignore` entry with a reason, or a TiKV client stack that drops rustls 0.21), which is a human security decision outside this slice. I could not produce a green C4-ci without it.

```toml
[[doctor.checks]]
group = "engine"
id    = "all-features-advisory-baseline"
cmd   = "timeout 600 bash -c '. ./engine/lib/ensure-cargo.sh && ensure_cargo && cd \"${PDCA_WORKTREE:-${WYRD_REPO:-../wyrd}}\" && cargo deny --locked --all-features --config deny-all-features.toml check advisories'"
hint  = "the target base fails the all-features advisory wall before any patch is applied; land an advisory disposition on main (a reviewed `ignore` in deny-all-features.toml, or a TiKV stack bump) so C4-ci can go green, then answer iterate-do"
level = "MISSING"
```

The row runs the same command the gate runs (`xtask/src/main.rs:1683`, `cargo_deny_check`),
plus `--locked` so it can never rewrite `Cargo.lock`, and a 600 s `timeout` because
`doctor.probe` runs it with none. It finds the Wyrd checkout in the same order as
`engine/xtask.sh` (`$PDCA_WORKTREE`, then `$WYRD_REPO`, then `../wyrd`, which is on `main` @
`7bf7eb9` too). It fetches the advisory database as the gate does, so a network failure would
also exit non-zero; the human sees the probe's verdict at sign-off either way. I checked that
it tells the two states apart, from the project root against this worktree:

- As shipped: exit **1**, `error[vulnerability]` for RUSTSEC-2026-0346 and RUSTSEC-2026-0349,
  `advisories FAILED`.
- With the two ids added to the worktree's `deny-all-features.toml` `ignore` list: exit **0**,
  `advisories ok`. The file was then restored and compared byte-for-byte with a saved copy.

So the row confirms the blocker today, and it stops firing as soon as a disposition lands on
the base.

Facts for the decision (re-checked today, unchanged from round 3):

- **No upstream fix exists to pull in.** crates.io lists no rustls release after `0.21.12` on
  the 0.21 line (the advisories say "patched >= 0.23.46"), and `tikv-client`'s newest release
  is still `0.4.0` (2026-02-07). `deny-all-features.toml:50-90` already records that this TiKV
  stack has no release carrying the TLS upgrade (#543).
- **TiKV-only.** The default-graph `cargo deny check` passes: `advisories ok, bans ok,
  licenses ok, sources ok` (this round's CI log, line 2944). The chain sits behind the
  off-by-default `tikv` feature.
- **Severity** from the advisory files' CVSS vectors: 0346 is
  `AV:L/AC:H/PR:H/UI:R/C:H/I:N/A:N` (about 4.0, medium); 0349 is
  `AV:N/AC:H/PR:N/UI:N/C:L/I:N/A:N` (about 3.7, low). Neither is high or critical, so the
  file's "immediately if a NEW high/critical advisory enters this tree" trigger
  (`deny-all-features.toml:83`) does not apply.
- **Ready-to-paste entries** for a separate PR, in the file's existing #543 style (compare
  `RUSTSEC-2026-0258`, `deny-all-features.toml:146-152`). They go at the end of the `ignore`
  list, and the two counts of tikv entries (`:52` "seven", `:105` "All seven") become nine.
  (Plain fence on purpose: this is a fragment of the `ignore = [ … ]` array, not a
  standalone TOML document, and the driver parses every `toml` fence in this file.)

```
    # MEDIUM (4.0). The derived `Debug` of TLS client session values prints the TLS 1.2
    # master secret / TLS 1.3 resumption PSK; only a custom `ClientSessionStore` that formats
    # them leaks. rustls 0.21.12 <- tokio-rustls 0.24 / tonic 0.10 <- tikv-client 0.4.0.
    # Patched only in rustls >= 0.23.46; the 0.21 line has no fix.
    { id = "RUSTSEC-2026-0346", reason = "rustls 0.21.12 via tonic 0.10 <- tikv-client 0.4.0 behind the off-by-default `tikv` feature; the 0.21 line has no patched release (>= 0.23.46 only). Absent from the shipped graph. No fix reachable: tikv-client 0.4.0 is abandoned upstream (#543)." },

    # LOW (3.7). An unauthenticated ServerHello can pin later connections to a classical key
    # exchange group (loss of post-quantum confidentiality). Same rustls 0.21.12 chain.
    { id = "RUSTSEC-2026-0349", reason = "rustls 0.21.12, same tikv-client-only chain as RUSTSEC-2026-0346; no patched 0.21 release. Absent from the shipped graph (#543)." },
```

  A matching issue: `gh issue create --repo getwyrd/wyrd --title "deps: RUSTSEC-2026-0346/0349
  (rustls 0.21.12 via tikv-client) turn the all-features advisory wall red"`. I did not open
  it or any PR; that is the human's call.

**Why not put those entries in this patch.** It is small (2 entries, 9 lines with comments,
plus 2 one-word count edits, all in `deny-all-features.toml`), and it would turn C4-ci green.
I still ruled it out on substance, not size. The repo treats each `ignore` as a reviewed
security decision with review triggers (`deny-all-features.toml:50-90`). Hiding one inside a
timestamp-rendering PR would bypass that review. The rubric also says a real finding outside
the PR's scope gets a decline with an issue reference, not an in-PR fix. And the blocker hits
every slice cut from this base, so one separate PR that every slice rebases onto is cheaper
than repeating it in each.

## The change (unchanged since round 2)

1. **`iso8601` renders whole seconds**: `crates/gateway-s3/src/lib.rs:634-648` (base
   `:624-638`). The `let millis = epoch_millis % 1_000` line (base `:627`) is gone, and the
   format string ends in a literal `.000Z` (`:646`). `:637` keeps the `/ 1_000` truncation,
   the same one `http_date` applies, so for every stored `u64` both formatters name the same
   second. The year-9999 bound is unchanged.
2. **Both doc comments state the rule**: `iso8601` at `:620-629`, `http_date` at `:3193-3197`.
   Each points at the other and names RFC 7231 §7.1.1.1 and ADR-0047 (storage stays in
   milliseconds).
3. **Architecture doc**, one sentence after the serde/CAS sentence
   (`docs/design/architecture/05-building-block-view.md:194-196`): the store keeps `modified`
   in epoch milliseconds, and every S3 surface reports it at whole seconds. This meets the
   docs-currency rule (`AGENTS.md:154-157`).
4. **In-file unit test** `listing_and_header_formatters_both_truncate_to_whole_seconds`
   (`crates/gateway-s3/src/lib.rs:4622-4656`): `.000`, `.750`, `.999`, and the last
   renderable millisecond.

Every surface that exists today is covered. `iso8601` has one caller, `render_contents`
(`:1034`), which serves both v1 and v2 listings. `http_date` has two: `apply_object_headers`
(`:2499`, shared by GET, ranged GET and HEAD) and the 304 response (`:2607`). The
conditional-request comparison already works in whole seconds (`:2786`, `:2801`). Other
mentions of `Last-Modified` / `<LastModified>` in `crates/` are comments that say the value is
stored in epoch milliseconds and rendered on the wire (e.g.
`crates/gateway-core/src/lib.rs:230-231`, `crates/core/src/metadata.rs:1648`); they stay true.
A future surface such as ListParts (#508) gets the rule by reusing `iso8601`.

Out of scope, declined with a reference: the draft proposal
`docs/design/proposals/draft/0017-blackbox-validation-tool.md:461-463` still says a listing
"keeps milliseconds". The brief rules out any `docs/` edit other than the one
architecture-doc sentence, and the round-3 adversary already put this in front of the human
(deferred findings). It belongs to a follow-up on #769 or a human call to amend the draft.

## Alternatives ruled out

- **Fractions in the header**: impossible. IMF-fixdate has no fraction field.
- **Documentation-only answer**: the brief's live S3 sample (3,000 of 3,000 values on `.000Z`)
  rules it out.
- **Round to the nearest second**: would disagree with `http_date` and with the conditional
  comparison, which both truncate.
- **Truncate at storage time**: out of scope, and #768's ordering relies on milliseconds.
- **Ship the advisory `ignore` in this patch**: see the NEEDS-HUMAN section.
- **`cargo update -p rustls@0.21.12`**: there is no newer 0.21 release to update to.

## Test: `crates/server/tests/s3_last_modified_agreement.rs` (new file, unchanged since round 3)

Harness, following the cited peers: the bucket marker is seeded as raw bytes, as
`s3_list_objects.rs:82-88` does (`seed_bucket`, `:96-102`). Two flat objects are written
through `wyrd_core::write::{plan_write,intent,write_fragments,commit_create,release}`, as
`segmented_object_read.rs:55-82` does (`write_flat`, `:107-138`), with
`plan.object_meta.modified` set to `1_700_000_000_750` and `1_700_000_000_000`. That is the
field the gateway's PUT stamps (`crates/server/src/lib.rs:251-255`), under the dirent
`"{bucket}/{key}"` beneath `ROOT` (`crates/server/src/lib.rs:708-712`). All of it happens
before `Gateway::new` (`:155-201`). Requests are raw SigV4-signed HTTP with the canonical query
signed (`s3_list_objects.rs:34-37`; `send`, `:393-444`).

For each object: `GET /stamps?list-type=2`, `GET /stamps` (v1), `HEAD` and `GET` on the key
(`surfaces`, `:720-763`). Then `assert_agreement` (`:765-793`) checks:
(1) both listings render exactly `2023-11-14T22:13:20.000Z`;
(2) HEAD and GET carry exactly `Tue, 14 Nov 2023 22:13:20 GMT`;
(3) parsed with the AWS SDK's own parsers (`aws_smithy_types::DateTime::from_str` with
`Format::DateTimeWithOffset` and `Format::HttpDate`, `:483-492`), both listing values and the
GET header name the same instant as the HEAD header.

Carried forward from earlier rounds: every socket and store await is under a 30 s deadline
(`DEADLINE` `:67`, `bounded` `:88-92`); the serve task is aborted on drop (`:142-152`); the
strict response reader (`parse_response` `:247-338`, `dechunk` `:344-387`) refuses bad
framing, proven by `the_response_reader_rejects_incomplete_or_malformed_framing`
(`:516-702`); `the_sdk_date_parsers_tell_a_fraction_from_a_whole_second` (`:500-514`) shows
the parser keeps `.750`, so check (3) cannot pass on the unfixed gateway.

Clock: the one wall-clock read is the SigV4 `x-amz-date` in `amz_date_now` (`:203-211`), with
a per-site `#[allow(clippy::disallowed_methods)]` and a comment saying the wall clock owns the
request-signing lifecycle. The asserted `modified` values are constants.

### Red → green (this round, re-run)

`cargo xtask` has no single-test task (`xtask/src/main.rs:117-150`), and
`engine/scripts/run-verify.sh` creates its own worktree beside the live repo, outside my
roots, and runs at Check anyway. So the quick legs used the same `cargo test` line that script
runs, bounded by `timeout`:
`timeout 1500 cargo test --locked -p wyrd-server --test s3_last_modified_agreement`.

- **Fixed tree**: `4 passed; 0 failed`.
- **Fix reverted** (`git checkout -- crates/gateway-s3/src/lib.rs`; test and doc kept):
  `off_second_modified_lists_and_heads_as_the_same_instant ... FAILED` at `:767`,
  `ListObjectsV2 <LastModified> of off-second  left: "2023-11-14T22:13:20.750Z"  right:
  "2023-11-14T22:13:20.000Z"`; `3 passed; 1 failed`. The other three pass on both sides by
  design: the on-second object renders the same before and after (the brief's "renders the
  same as it does today"), and the two oracle tests do not depend on the fix.
- **Fix re-applied** (from the saved gateway diff): `4 passed; 0 failed`; the in-file unit test
  `listing_and_header_formatters_both_truncate_to_whole_seconds` also passes.
- Every symbol the test uses predates #767, so on base it compiles and fails on an assertion,
  not a build error. Round 3 also showed check (3) alone fails on the reverted tree
  (`v2 listing (…20.750Z) and HEAD (… 22:13:20 GMT) disagree`); the test is unchanged, so that
  still holds.

## Self-refutation

- **(a) Genuine red?** Yes. With only `crates/gateway-s3/src/lib.rs` reverted, the off-second
  wire test fails on `.750Z` vs `.000Z` (output above). The soundness test shows the SDK parser
  cannot hide the gap: it parses the exact pre-fix value to a different instant than the
  header.
- **(b) Production path?** Yes. The test drives the real `S3Gateway::serve` on a loopback TCP
  listener and the real `wyrd_server::Gateway` over redb and the fs chunk store, so the
  production `render_contents` → `iso8601` and GET/HEAD → `http_date` paths run. Requests are
  signed with the gateway's own `sigv4::sign`. Nothing is mocked. The date parsers are the AWS
  SDK's, not a copy of the gateway's formatter.
- **(c) Fixture includes the fault?** Yes. The fixture stores `1_700_000_000_750`, the exact
  off-second value that disagreed, as a real committed object. The on-second control sits in
  the same bucket, so the listing reader has to pick the right row.

## Full gate (`engine/xtask.sh ci`, the project runner), two runs this round

**Run 1, the tree as shipped** (`PDCA_WORKTREE=… ./engine/xtask.sh ci`): exit 1, same profile
as rounds 1–3.

- Passed: `typos`; `lint_docs.py`; `render_site.py --check` (the docs renderer and link
  audit, so both of the brief's external dependencies were exercised); the gitlink, unsafe and
  blackbox guards; `cargo fmt --check`; `cargo clippy --workspace --exclude wyrd-dst
  --all-targets`; `cargo build`; `cargo test --workspace --exclude wyrd-dst` (197 test
  binaries, 1,681 passed, 0 failed, 14 ignored, including this file's `4 passed`);
  `cargo-machete`; `cargo deny check` (`advisories ok, bans ok, licenses ok, sources ok`).
- Failed, run stopped: the all-features advisories check, on RUSTSEC-2026-0346 and -0349 only.

**Run 2, with the two advisories ignored in the worktree only** (to see every later stage;
`deny-all-features.toml` restored afterwards and checked byte-for-byte against a saved copy,
and it is not in `patch.diff`):

exit 0, `xtask ci: all checks passed`. Every stage of `run_ci` (`xtask/src/main.rs:1671-1690`)
ran, in order:

- Everything in run 1 again, with the same counts (197 binaries, 1,681 passed, 0 failed).
- `cargo deny --all-features --config deny-all-features.toml check advisories`: `advisories ok`.
- `cargo deny --all-features check licenses bans sources`: `bans ok, licenses ok, sources ok`.
- `xtask conformance`: `5 valid + 6 invalid vectors pass`.
- `xtask statics`: `no DST-reachable shared mutable global state (ADR-0035)`.
- `xtask deploy-guard`: `no workspace crate imports an orchestrator API (ADR-0010)`.
- `cargo clippy -p wyrd-dst` and `cargo test -p wyrd-dst` (both `--cfg madsim`): clean;
  10 test binaries, 85 passed, 0 failed, 1 ignored.

So the advisory disposition is the only thing between this patch and a green C4-ci.

## Commit-readiness

- `cargo fmt --all -- --check`: clean.
- `cargo clippy --locked -p wyrd-gateway-s3 -p wyrd-server --all-targets -- -D warnings`:
  clean.
- `typos` on the three touched files: clean. `lint_docs.py`: `lint_docs: OK`.
- The repo has no `.pre-commit-config.yaml` or `.githooks`; the commit-time checks are the
  `cargo xtask ci` prose and fmt stages, all green above.
- `git apply --check -R patch.diff` on the worktree succeeds, so the patch applies forward on
  `7bf7eb9`.

## Prior art

Unchanged since round 2. The round-3 reviewer re-checked it on its own (T4 PASS: all 381
closed PRs paged, the file lists of all 19 closed-unmerged PRs, and the path history of all
three touched paths). Re-checked today: upstream has no new commits on `main` and no open PR
on #767.

## Housekeeping

Scratch dirs, both under `$PDCA_SCRATCH`: `pdca-builder-767-redleg` (the saved gateway diff and
the `deny-all-features.toml` backup) and `pdca-builder-767-ci` (gate logs and the doctor-probe
log). Both are removed. Left in place, because I did not choose their names: six round-1 files
`pdca-builder-767-{ci,ci2,conformance,dst,statics}.log` and `pdca-builder-767-lib.rs.fixed`
(dated 2026-10-14), and the `wyrd-repo-guard-fixture-*` dirs that the target's own xtask tests
write to `$TMPDIR` during `cargo xtask ci` (some from this round's two runs). The harness
reclaims `$PDCA_SCRATCH`. The worktree's `deny-all-features.toml` is back to base (`git
checkout --`, then `cmp` against the saved copy). The worktree keeps one `git add -N` (intent-to-add) entry for the new
test file so `git diff` includes it. Nothing is committed, pushed, or opened as a PR.
