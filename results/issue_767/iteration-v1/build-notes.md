# Build notes — issue 767 / listing-last-modified-second-granularity

Target: getwyrd/wyrd `main` @ `7bf7eb9` (worktree `/home/eddie/wyrd/wyrd.pdca-wt-l1`).
Line numbers below are on the patched tree unless marked "base".

## The change

1. **`iso8601` renders whole seconds** — `crates/gateway-s3/src/lib.rs:633-648` (base
   `:624-638`). The `let millis = epoch_millis % 1_000` line is gone and the format string
   ends in a literal `.000Z`. That is the same `/ 1_000` truncation `http_date` already
   applies (`:3200-3203`), so for every stored `u64` the two formatters name the same
   second. The year-9999 bound is unchanged (`253_402_300_799_999` →
   `9999-12-31T23:59:59.000Z`).
2. **Doc comments state the rule on both formatters**: `iso8601` at `:620-627`, `http_date`
   at `:3192-3195`. Each one points at the other.
3. **Architecture doc**: one sentence in the object-metadata bullet,
   `docs/design/architecture/05-building-block-view.md:190-193`: the store keeps `modified`
   in epoch milliseconds, and every S3 surface reports it at whole seconds. This is the
   docs-currency requirement (AGENTS.md), since a listing response changes.
4. **In-file unit test** `listing_and_header_formatters_both_truncate_to_whole_seconds`
   (`crates/gateway-s3/src/lib.rs:4620-4650`): on-second, `.750`, `.999`, and the last
   renderable millisecond. Both formatters are checked against literal strings.
5. **New wire test** `crates/server/tests/s3_last_modified_agreement.rs` (the discriminator
   the brief names).

Why this is the rule fix and not a one-formatter patch: there is exactly one listing-side
renderer of `modified`. `grep iso8601` finds only `render_contents`
(`crates/gateway-s3/src/lib.rs:1033` patched), and the header side is the single `http_date`.
Both formatters now share the truncation and the stated rule, so any future surface
(ListParts' `LastModified`, #508) that reuses `iso8601` inherits whole seconds. The stored
record is not touched. ADR-0047's millisecond storage stays, and so does #768's ordering on
milliseconds.

## Alternatives ruled out

- **Add `.mmm` to the header instead**: impossible. IMF-fixdate has no fraction field
  (RFC 7231 §7.1.1.1).
- **Documentation-only answer**: the brief's live S3 sample (3,000/3,000 `.000Z`) picks the
  rendering fix.
- **Round to the nearest second instead of truncating**: that would disagree with `http_date`
  (which truncates) and with the conditional-request comparison
  (`crates/gateway-s3/src/lib.rs:2785`, `:2800` patched), which also truncates. Rejected.
- **Truncate at storage time**: out of scope per the brief, and it would break #768's
  millisecond ordering.

## Test

`crates/server/tests/s3_last_modified_agreement.rs`. It seeds the bucket marker as raw bytes
(as `s3_list_objects.rs:82-89` does) and two genuine flat objects through
`wyrd_core::write::{plan_write,intent,write_fragments,commit_create,release}` (as
`segmented_object_read.rs:55-80` does). Each object sets `plan.object_meta.modified`, the
field the gateway's own PUT stamps (`crates/server/src/lib.rs:251-255`), under the dirent name
`"{bucket}/{key}"` beneath ROOT (`crates/server/src/lib.rs:708-712`). All of this happens
before `Gateway::new`. It then sends raw SigV4-signed HTTP with the canonical query signed:

- `GET /stamps?list-type=2` (v2) and `GET /stamps` (v1; it also asserts there is no
  `<KeyCount>`, so it really is the v1 shape) → `<LastModified>` of the named row ==
  `2023-11-14T22:13:20.000Z`;
- `HEAD` and `GET /stamps/<key>` → `Last-Modified: Tue, 14 Nov 2023 22:13:20 GMT`;
- every value is parsed by independent in-test parsers (Hinnant `days_from_civil`, the
  inverse of the gateway's formatter), and all must give the same epoch millisecond.
  `the_independent_parsers_are_sound` checks the parsers recover `1_700_000_000_750` from
  `.750Z`, so the instant check really can tell the two apart.
- The off-second object (`1_700_000_000_750`) and the on-second one (`1_700_000_000_000`) are
  separate tests.

A missing `<Contents>` row, `<LastModified>` element, or header panics. Omitting the value
cannot pass as agreement.

### Red → green (run with `timeout 1200 cargo test -p wyrd-server --test s3_last_modified_agreement`)

- Fixed tree: 3 passed.
- Fix reverted (`git checkout crates/gateway-s3/src/lib.rs`, test file kept):
  `off_second_modified_lists_and_heads_as_the_same_instant ... FAILED` with
  `left: "2023-11-14T22:13:20.750Z"  right: "2023-11-14T22:13:20.000Z"`. The other two pass,
  as expected: the on-second object renders the same before and after, which is the brief's
  "renders the same as it does today" clause.
- The file uses only pre-existing API, so it compiles on base and fails by assertion.

Runner note: the project's runner is the whole `cargo xtask ci` gate (INTEGRATION.md §3). For
the quick red/green leg I ran the single test target under a `timeout` so it could not hang,
then ran the full `./engine/xtask.sh ci` once on the final tree (result below).

## Self-refutation

- **(a) Genuine red?** Yes. With only the `crates/gateway-s3/src/lib.rs` change reverted, the
  off-second test fails on the `.750Z` vs `.000Z` assertion (output above).
- **(b) Production path?** Yes. The test drives the real `S3Gateway::serve` over a loopback
  TCP listener, the real `Gateway` with redb + fs chunk store, and the production
  `render_contents` → `iso8601` and `http_date` paths. Nothing is mocked. Requests are signed
  with the gateway's own `sigv4::sign`.
- **(c) Fixture includes the fault?** Yes. The fixture stores `1_700_000_000_750`, a value
  off a second boundary, which is the exact case that failed. It is written as a real
  committed object, not a hand-built listing row.

## Commit-readiness

- `cargo fmt --all` run (it reflowed the new test file); `typos` on the three touched files:
  clean. The repo has no `.pre-commit-config.yaml` or `.githooks`.
- Full gate `./engine/xtask.sh ci`: see the result line below.

### Full-gate result (`./engine/xtask.sh ci`, final tree)

Two runs:

1. **Run 1**: `custodian_gc` (`crates/server/tests/custodian_gc.rs`) hung for 19 min at 0% CPU
   in three `deployed_role_*` tests. I killed that binary. Run alone afterwards
   (`timeout 300 cargo test -p wyrd-server --test custodian_gc`) it passed 10/10 in
   0.17 s. That file does not touch the gateway, listing, or formatter code, so this was a
   one-off hang, not this patch. It may be flaky under full-workspace parallel load; worth
   watching in Check's gate.
2. **Run 2**: these all passed: typos, `lint_docs`, `render_site --check` (the docs
   renderer; link audit OK), gitlink/unsafe/blackbox guards, `cargo fmt --check`,
   `cargo clippy --all-targets`, `cargo build`, `cargo test --workspace --exclude wyrd-dst`
   (197 test binaries ok, none failed), `cargo-machete`, and `cargo deny check`. The run then
   **failed at `cargo deny --all-features --config deny-all-features.toml check advisories`**
   with RUSTSEC-2026-0346 and RUSTSEC-2026-0349 against `rustls 0.21.12`
   (`Cargo.lock:302`). This patch changes no `Cargo.toml` or `Cargo.lock`, so this is a
   newly published advisory against a dependency already on base. It will fail every
   branch off `7bf7eb9` until the lockfile is bumped. It is out of scope here and should be
   tracked as a separate issue.
3. Because `ci` stops at the first failure, I ran the later stages by hand:
   `cargo xtask conformance` passed (5 valid + 6 invalid vectors), `cargo xtask statics`
   passed. `cargo xtask dst` was cut off by my tool's 20-minute background limit while
   still running. Every DST target that finished reported ok. `cargo tree -p wyrd-dst`
   has no `wyrd-gateway-s3` in it, so DST cannot reach the changed code.

Net: every gate that can see this change is green. The one red gate (rustls advisories)
is unrelated to it and would be red on base too.
