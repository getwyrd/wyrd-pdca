# Build notes — issue 620 / s3-object-key-length-limit (iteration 2)

Target: `getwyrd/wyrd` @ `main` (`7bf7eb9`). Worktree: `/home/eddie/wyrd/wyrd.pdca-wt-l0`.
Line numbers below are on the patched tree unless marked "base".

## What changed

**Production (`crates/gateway-s3/src/lib.rs`)**, same design as iteration 1. The review did not
reject the design; its findings were about the test harness, docs, and test coverage.

- `MAX_KEY_BYTES = 1024` (`lib.rs:497`), with a doc comment naming the source (AWS "Naming
  Amazon S3 objects") and stating where it is and is not enforced.
- `MAX_DELETE_BODY_BYTES` comment (`lib.rs:505-507`) now points at `MAX_KEY_BYTES` and says the
  parser refuses a longer key, so the "1000 keys × a 1024-byte key" sizing is true by
  construction (brief Scope: "the `MAX_DELETE_BODY_BYTES` sizing comment kept true").
- PUT arm: the decoded bucket and key are bound once (`lib.rs:1776-1778`, base `:1765-1769`).
  After the CopyObject guard (base `:1800-1806`, the peer callsite the brief names), two
  header-time refusals, both before the body stream is touched:
  - decoded bucket contains `/` → `400 InvalidBucketName` (`lib.rs:1828-1835`), criterion 5;
  - `key_name.len() > MAX_KEY_BYTES` → `400 KeyTooLongError` (`lib.rs:1838-1840`), criteria 1-2.
  `str::len` is the UTF-8 byte count of the percent-decoded key alone.
- `DeleteRequestError::KeyTooLong` (`lib.rs:1983`); `key_too_long()` builds the response via the
  shared `error_response` (`lib.rs:2023-2030`); `delete_objects` maps the variant before any
  `delete_object` call (`lib.rs:2114`).
- Parser per-entry bound right after the empty-key check (`lib.rs:2270-2272`, base `:2209-2211`,
  the pattern the brief names). It measures the key as `char_data` yields it: entity-decoded
  once, never percent-decoded (brief Citations).
- GET, HEAD and single-object DELETE are untouched (base `:1878-1891`), so criterion 4 holds.

**Docs**: `docs/design/architecture/05-building-block-view.md:137`, a paragraph under the L1
access-layer table covering the decoded-byte limit, the PUT bucket-separator refusal, the
whole-request bulk refusal, and that single-object GET/HEAD/DELETE stay open for legacy keys.
I chose this file because it is the living architecture doc that owns the S3 gateway row
(`:132`). PR #612 (bulk delete) added no doc, so no S3 operations page existed to extend.

**Tests**
- `crates/server/tests/s3_key_length_limit.rs` (NEW, the brief's named discriminator): five
  wire tests, one per criterion.
- `crates/gateway-s3/src/lib.rs:4521-4773` (in the existing `mod tests`): a pure parser test and
  two router tests (`oneshot`) with a recording double. Why they exist is explained under C5 below.

## Iteration-1 carry-forward: what I did with each item

1. **T4 / T5: socket awaits had no deadline** (`s3_key_length_limit.rs:161` in v1). The
   whole exchange (connect, write head, write body, flush, `read_to_end`) now runs inside
   `bounded()` with `IO_BUDGET = 30s` (`s3_key_length_limit.rs:66`, `:88-93`, `:186-196`).
   The fixture's backend awaits (bucket-marker commit, `:102-107`; leg 4's `put_object` seed,
   `:371-376`) are bounded too. A healthy exchange takes milliseconds (the whole file ran in
   0.06 s), so 30 s will not flake under a loaded `cargo test`.
2. **T4 / T5: discarded server `JoinHandle`** (`:92` in v1). `Harness` now owns
   `server: JoinHandle<()>` and `impl Drop for Harness` aborts it (`:72-85`). Drop runs on
   every exit path, including assertion-failure unwinding. `Drop::drop` runs before the fields
   drop, so the abort is issued before the tempdir is removed.
3. **Leg 3 did not prove the parser counts bytes, not characters.** The reviewer showed that
   `key.len()` → `key.chars().count()` passed all five tests. Leg 3 now loops over two
   over-limit entries: 1,025 ASCII bytes, and `"€".repeat(342)` (342 chars, 1,026 bytes)
   (`s3_key_length_limit.rs:324-344`). Under the reviewer's mutation, 342 characters is not
   more than 1,024, so the request deletes `ok` and answers 200, and `assert_refused` fails.
   The in-crate parser test checks the same case (`lib.rs:4564-4577`).
4. **T4: docs currency** (three passes flagged `lib.rs:1812/1836/1840`). Fixed with the
   `05-building-block-view.md:137` paragraph above.
5. **C5 mutants (advisory): 8 missed.** Cause: `cargo mutants --in-diff` runs only the tests
   of the package that contains the mutant (`wyrd-gateway-s3`). The v1 discriminator lives in
   `wyrd-server`'s `tests/`, so no test that cargo-mutants ran could see any of these mutants.
   Fix: in-crate tests that pin each missed mutant:
   - `lib.rs:1838` `>` → `==` / `>=` (PUT): `an_object_put_refuses_an_over_limit_key_before_the_write_seam`
     (`lib.rs:4699`) expects 200 at exactly 1,024 bytes (kills `>=`) and 400 at 1,025 (kills `==`);
   - `key_too_long` → `Default::default()`: same test, asserts `400` + `<Code>KeyTooLongError</Code>`;
   - `delete_objects` → `Default::default()`: `a_bulk_delete_naming_an_over_limit_key_deletes_nothing`
     (`lib.rs:4749`) expects 400 and then a 200 that reaches the delete seam;
   - "delete match arm `Object`" and `lib.rs:2270` `>` → `==` / `<` / `>=` (parser):
     `the_delete_parser_bounds_each_key_at_1024_utf8_bytes` (`lib.rs:4545`) expects
     `Some([ok, 1024-byte, 512×é])` (kills arm deletion, `<`, `>=`) and `KeyTooLong` for 1,025
     bytes and 342×€ (kills `==`).
   The mutants re-run result is recorded under "Verification" below.
6. **C4: `cargo deny … deny-all-features.toml check advisories` failed.** Not caused by this
   patch, and deliberately not fixed in it. See the NEEDS-HUMAN item below.

I did not touch the 3 findings held in `deferred-findings.json` (the driver marked them as
needing human judgement, not build work). The bulk-delete bound stays: brief criterion 3
requires it, and the carry-forward's "moot if the human drops the bulk bound" is that human's
call, not mine.

## The cargo-deny failure is on the base, not the patch

The failing invocation reads only `Cargo.lock` and `deny-all-features.toml`. This patch touches
neither (`git diff --stat`: `crates/gateway-s3/src/lib.rs`, `crates/server/tests/s3_key_length_limit.rs`,
`docs/design/architecture/05-building-block-view.md`). The two advisories are
RUSTSEC-2026-0346 (rustls `Debug` impls expose session secrets) and RUSTSEC-2026-0349 (an
unauthenticated ServerHello can downgrade key-share prediction). Both are on `rustls 0.21.12`
(base `Cargo.lock:3294`), reached only through `tikv-client 0.4.0 → tonic 0.10.2`, behind the
off-by-default `tikv` feature. The default-graph `cargo deny check` passes. A base checkout fails
this step identically, so every bundle on this base fails C4-ci, whatever it contains.

Why I did not add the two `ignore` entries here:
- `deny-all-features.toml:80-84` sets a review trigger: "immediately if a NEW high/critical
  advisory enters this tree … grounds to reopen the carrying-cost-vs-option-value question that
  #443 leaves open." Whether these two are waived or prompt that review is a maintainer policy
  decision about the TiKV fallback. It is not part of a key-length fix.
- The rubric's Reviewer protocol: "a real finding outside the PR's stated scope gets a
  decline-with-issue-reference, not an in-PR fix."
- All seven bundles in this batch share the base. If each one carried the same two entries,
  they would conflict with each other in `deny-all-features.toml`. One separate PR fixes all
  seven.
- `gh search issues --repo getwyrd/wyrd RUSTSEC-2026-0346` and `… 0349` both return `[]`, so
  there is no issue to cite yet, and opening one on the target is not a builder action.

The separate PR is about 6 lines, in the file's existing style. It belongs after the
`RUSTSEC-2026-0258` entry in `deny-all-features.toml`'s `ignore` list:

```toml
    # rustls 0.21.12 <- tokio-rustls 0.24 / tonic 0.10 <- tikv-client 0.4.0. Same abandoned
    # chain as RUSTSEC-2026-0104; the shipped graph's rustls is unaffected (`cargo deny check` green).
    { id = "RUSTSEC-2026-0346", reason = "rustls 0.21.12 Debug impls expose session secrets; tikv-client-only chain, absent from the shipped graph (#543)." },
    { id = "RUSTSEC-2026-0349", reason = "rustls 0.21.12 ServerHello key-share downgrade; tikv-client-only chain, absent from the shipped graph (#543)." },
```

NEEDS-HUMAN external dependency: RustSec advisory DB (RUSTSEC-2026-0346, RUSTSEC-2026-0349 on tikv-client's rustls 0.21.12) — the gating C4-ci row (`cargo xtask ci`) fails at its second `cargo deny` step on the unpatched base `7bf7eb9`, so no bundle on this base can show a green C4-ci; a maintainer must waive or otherwise resolve the two advisories in a separate PR against `deny-all-features.toml` (sketch above, subject to the file's #543 review trigger), after which this bundle's C4-ci can be re-run.

```toml
[[doctor.checks]]
id    = "base-offbydefault-advisories"   # the base's off-by-default (tikv/fdb) advisory wall is clean
cmd   = "cd ../wyrd && cargo deny --all-features --config deny-all-features.toml check advisories >/dev/null 2>&1"
hint  = "a new RustSec advisory landed on the base's tikv/fdb tree: every bundle's C4-ci will fail at the second cargo-deny step until a separate deny-all-features.toml PR records or clears it (#543 review trigger)"
level = "WARN"
```

The CI stages that this failure stopped from running last round (the third `cargo deny`
invocation, conformance, statics, orchestrator guard, DST) I ran separately. Results are under
"Verification".

## Verification

- **Red→green, through the project's own runner** (`engine/scripts/run-verify.sh`, with
  `PDCA_BUNDLE` set to this bundle, `PDCA_LANE=0`, `PDCA_BRIEF_BASE=origin/main`, wrapped in
  `timeout 3000`):
  - GREEN (fix applied): `5 passed; 0 failed`.
  - RED (production reverted, test kept): `1 passed; 4 failed`. The failures are leg 1 (`PUT
    1,025-byte key: … left: 200 right: 400`), leg 2 (`PUT 342 × €: … left: 200`), leg 3
    (`DeleteObjects with a 1,025-byte key: … left: 200`, and the body shows both keys
    `<Deleted>`), and leg 5 (`PUT /b%2Fa/<1,023 bytes>: … left: 200`). Leg 4 passes on both
    sides, as the brief's Falsifiability section says it should.
  - Verdict line: `run-verify.sh: PASS — red without the fix, green with it`.
- **Formatter / lints**: I ran `cargo fmt --all` before writing `patch.diff`, and the CI run's
  `cargo fmt --all -- --check` is clean. `cargo clippy -p wyrd-gateway-s3 -p wyrd-server --all-targets`
  is clean (workspace lints are deny-level).
- **Full gate** (`./engine/xtask.sh ci` with `PDCA_WORKTREE` set to the worktree, under
  `timeout 7000`): typos, `lint_docs.py` (`OK`), `render_site.py --check` (`link audit OK`), the
  gitlink / unsafe / blackbox guards, fmt, clippy, build and `cargo test --workspace --exclude
  wyrd-dst` all pass. That includes the three new in-crate tests
  (`tests::the_delete_parser_bounds_each_key_at_1024_utf8_bytes`,
  `tests::an_object_put_refuses_an_over_limit_key_before_the_write_seam`,
  `tests::a_bulk_delete_naming_an_over_limit_key_deletes_nothing`) and the five wire tests.
  `cargo-machete` and `cargo deny check` (default graph: `advisories ok, bans ok, licenses ok,
  sources ok`) pass. The gate then stops at the second `cargo deny` step on exactly
  RUSTSEC-2026-0346 and RUSTSEC-2026-0349 (see above).
- **Stages after that step, run one by one**: `cargo deny --all-features check licenses bans
  sources` exit 0; `./engine/xtask.sh conformance` exit 0; `./engine/xtask.sh statics` exit 0;
  `./engine/xtask.sh dst` exit 0 (clippy under `--cfg madsim` clean; the DST test binaries
  report 85 passed, 0 failed; the longest binary ran 33 tests in 1,273 s). The orchestrator guard (`deploy-guard`) has no standalone
  subcommand, so I could not run it on its own. The patch adds no dependency and no `use` beyond
  `std`, `tokio` and crates the files already import, and `grep` finds no
  kube/k8s/orchestrator text in `patch.diff`.
- **Mutants** (`scripts/mutants-in-diff`, the C5 script, with `PDCA_BUNDLE` and `PDCA_WORKTREE`
  set): `12 mutants tested in 43s: 11 caught, 1 unviable`, none missed. Iteration 1 had 8 missed.

## Forced self-refutation

- **(a) Genuine red? Yes.** `run-verify.sh` reverted the production change in a clean
  `origin/main` worktree and kept the test. Four of five tests failed on assertions (`left:
  200, right: 400`), not on compile errors: the test uses only pre-existing API (the wire,
  `Gateway::put_object`, `MetadataStore::commit`). The fifth (leg 4) is designed to pass on both
  sides. For the leg-3 gap the reviewer found: with `chars().count()`, the 342 × `€` entry
  counts as 342, so it is accepted, `ok` is deleted, the response is 200, and
  `assert_refused` fails at `s3_key_length_limit.rs:333-337`.
- **(b) Production path? Yes.** The wire test starts the real `S3Gateway::serve` over a
  loopback `TcpListener` in front of the real `wyrd_server::Gateway` (redb in-memory metadata,
  `FsChunkStore` on a tempdir, `MemCoordination`), the stack the peer suites use. Requests are
  real SigV4-signed HTTP/1.1. Nothing is mocked. The in-crate router tests use a recording
  `ObjectGateway` double, but they go through the real `router()` → `dispatch` → PUT arm /
  `delete_objects` → `parse_delete_request`. Only the storage seam is replaced, and the point
  of those tests is to show the seam is never reached.
- **(c) Fixture includes the fault? Yes.** Leg 1 sends the 1,025-byte key, and its listing check
  is made binding by a `control` object that must appear in the listing (`:272`), so an empty
  or broken listing cannot satisfy "not listed". Leg 2 sends the 1,026-byte `€` key and the
  fully encoded 1,024-byte key. Leg 3 sends both over-limit entries next to a real stored `ok`
  and checks `ok` survives. Leg 4 seeds a real 1,025-byte object through the backend
  (`:371-376`) and requires HEAD 200 before the DELETE, so the 204 cannot come from an absent
  key. Leg 5 sends the real `%2F` path.

## Ruled out

- **Guarding only the bulk-delete parser.** This breaks the invariant (PUT could still create
  a 2,000-byte key that bulk delete then refuses). It is exactly the #612 finding, and the
  brief's SELF-TEST rules it out.
- **Checking the limit on GET / HEAD / single DELETE.** That would strand legacy over-limit
  objects (criterion 4, principle C-1).
- **Measuring `"{bucket}/{key}"` or the wire path.** Both contradict criterion 2.
- **A separate test-module file** (`#[cfg(test)] mod key_limit_tests;`), to keep clear of
  #666's test-double edits. No crate in the repo does this (`grep` found no file-backed
  `#[cfg(test)] mod x;`), so I followed the convention and added the tests inline. They sit
  after `op_label_attributes_bulk_delete_to_the_delete_series`, away from the shared doubles
  (`NoGateway`, `StoredMetaGateway`, `DefaultRangeSeamGateway`) that #666 edits. The new double
  (`RecordingGateway`) is a separate type, so #666 changing the existing doubles does not touch it.
- **Adding the deny ignores in this PR.** See above.

## Conflicts / ordering

The brief says this conflicts with #671 (size refusal in the same PUT arm). My refusals sit
between the CopyObject guard and the `content-type` read (`lib.rs:1817-1840`). #671 will want
the same spot, which is why the two are in different waves. This is wave 0.
