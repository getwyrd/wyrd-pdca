# Build notes — issue 620 / s3-object-key-length-limit (iteration 3)

Target: `getwyrd/wyrd` @ `main` (`7bf7eb9`; `git ls-remote` on 2026-10-15 shows main has not
moved). Worktree: `/home/eddie/wyrd/wyrd.pdca-wt-l0`. Line numbers are on the patched tree
unless marked "base".

## Summary

The production change is the same as iteration 2. Neither review in round 2 found a defect in
it (`iteration-v2/check-review.md:5-14`, `check-advisory-adversary.md:3-13`). This round:

1. **Removed the two duplicate percent-encoders** (the code-review finding). Both tests now call
   the existing public `sigv4::uri_encode(key, true)` (`crates/gateway-s3/src/sigv4.rs:163`,
   exported through `pub mod sigv4` at `lib.rs:64`, both on the base, so the reverted base still
   compiles the test).
2. **Fixed a contradiction in the docs paragraph** (the adversary's non-blocking note).
3. **The cargo-deny failure.** It is still not in `patch.diff`. This time I wrote the fix as a
   separate, ready-to-apply diff (`base-advisory-waiver.diff` in this bundle). I applied it to
   the worktree only for one full `cargo xtask ci` run, then took it back out. Details and
   reasons are below.

## What changed this round

- `crates/gateway-s3/src/lib.rs`, test module: deleted `fn path_encode` (iteration 2,
  `lib.rs:4623-4635`). The two call sites now use `crate::sigv4::uri_encode(key, true)`
  (`lib.rs:4694`, `:4708`), with the comment that explained the helper moved to the loop
  (`lib.rs:4691-4692`).
- `crates/server/tests/s3_key_length_limit.rs`: deleted `fn encode` (iteration 2, `:134-146`).
  `object_path` now calls `uri_encode(key, true)` (`:203-207`, import at `:52`). I kept
  `encode_all` (`:134-138`), as the code reviewer said to: it escapes unreserved bytes on
  purpose, so leg 2 can send a 1,024-byte key as a 3,072-byte path. `uri_encode` cannot
  produce that spelling.
- `docs/design/architecture/05-building-block-view.md:137`: "The limit is enforced only where
  an object is created" became "Two operations enforce it: object `PUT`, where an object is
  created, and bulk `DeleteObjects`." The old sentence was contradicted two sentences later by
  the bulk-delete refusal. The code comment at `lib.rs:493` already said it correctly.

The file is now 396 lines instead of 408. Behaviour of both tests is unchanged:
`uri_encode(_, true)` has the same unreserved set (`A-Za-z0-9-_.~`), encodes `/`, and uses
upper-case hex (`sigv4.rs:163-186`), exactly like the deleted helpers.

## The production change (unchanged from iteration 2, for reference)

- `MAX_KEY_BYTES = 1024` (`lib.rs:497`), doc comment naming the AWS source and where the
  limit is and is not enforced (`:489-496`).
- `MAX_DELETE_BODY_BYTES` comment points at `MAX_KEY_BYTES` and says the parser refuses a
  longer key (`lib.rs:505-506`), so the "1000 × 1024-byte key" sizing is true by construction.
- PUT arm: decoded bucket and key bound once (`lib.rs:1776-1778`, base `:1765-1769`). After the
  CopyObject guard (base `:1800-1806`, the peer the brief names), two refusals before the body
  stream is touched: decoded bucket contains `/` → `400 InvalidBucketName` (`:1828-1835`,
  criterion 5); `key_name.len() > MAX_KEY_BYTES` → `400 KeyTooLongError` (`:1838-1840`,
  criteria 1-2).
- `DeleteRequestError::KeyTooLong` (`lib.rs:1983`), `key_too_long()` via the shared
  `error_response` (`:2023-2030`), mapped in `delete_objects` before any `delete_object` call
  (`:2114`).
- Parser bound right after the empty-key check (`lib.rs:2267-2272`, base `:2209-2211`),
  measured on the key as `char_data` yields it: entity-decoded once, not percent-decoded.
- GET, HEAD, single-object DELETE untouched (base `:1878-1891`), so criterion 4 holds.

## Iteration-2 carry-forward: what I did with each item

1. **Optional cleanup: reuse `sigv4::uri_encode`.** Done, see above.
2. **C4: "Resolve the existing TiKV advisory-policy failure and obtain a complete CI run."**
   Not resolved inside `patch.diff`. A ready fix is in `base-advisory-waiver.diff`, and a
   complete CI run with it applied is recorded under Verification. See the next section.
3. The findings in `deferred-findings.json` are held for the human and marked "not build
   work". I left them alone. That includes the bulk-delete design question (whole-request
   refusal vs. per-key `<Error>` rows vs. no bound): brief criterion 3 requires the
   whole-request refusal, so changing it is a brief decision.

## The cargo-deny failure

### What is failing

`cargo xtask ci`'s second deny step (`cargo deny --all-features --config
deny-all-features.toml check advisories`) reports RUSTSEC-2026-0346 and RUSTSEC-2026-0349 on
`rustls 0.21.12` (base `Cargo.lock:3294-3295`). Both advisories are dated 2026-10-14, the same
day as the base commit, so they are new to the advisory DB, not to the code. The only path to that rustls is `tikv-client 0.4.0 → tonic 0.10.2 /
tokio-rustls 0.24.1` behind the off-by-default `tikv` feature (`cargo tree -i rustls@0.21.12
--all-features`). The default graph has no rustls at all: `cargo tree -i rustls@0.21.12
--target all` and `cargo tree -i rustls@0.23.45 --target all` both answer "did not match any
packages". (`rustls 0.23.45` is in the lockfile but in no resolved graph, even with
`--all-features`.) This patch touches neither `Cargo.lock` nor either deny config, so the base
fails this step identically.

### New facts this round

- **There is no lockfile fix.** Both advisories list `patched = [">= 0.23.46"]` (local RustSec
  DB, `crates/rustls/RUSTSEC-2026-0346.md`, `…-0349.md`). No 0.21.x release carries the fix,
  and `tikv-client 0.4.0` pins `tonic 0.10` → `rustls 0.21` (`deny-all-features.toml:66-70`).
  So the h2 route used in #790 (`cargo update` for the default graph, waiver for the tikv one)
  has nothing to update here.
- **Severity is medium and low, not high/critical.** From the CVSS 3.1 vectors in the advisory
  files: 0346 is `AV:L/AC:H/PR:H/UI:R/C:H/I:N/A:N` = **4.0 (medium)**; 0349 is
  `AV:N/AC:H/PR:N/UI:N/C:L/I:N/A:N` = **3.7 (low)**. The file's "immediately" review trigger is
  only for "a NEW high/critical advisory" (`deny-all-features.toml:83`). In iteration 2 I
  treated the trigger as firing. It does not. Same-chain low/medium advisories have been
  waived before: RUSTSEC-2026-0098 and -0099 (low), RUSTSEC-2024-0437 (medium), -0258 (low).
- **Precedent for how such a waiver lands.** The last one went in as its own chore PR, #790
  (`55af870`, "deps: bump h2 … so the cargo-deny wall is green", closing issue #773). Its
  review then found that the quarantine-boundary test
  (`xtask/tests/fdb_harness.rs:1236-1270`) must list every tikv-only ID, which became a
  follow-up commit (`4323067`).

### What I did

I wrote the waiver in #790's final shape, as `base-advisory-waiver.diff` (66 lines, base
`7bf7eb9`, applies with `git apply`):

- `deny-all-features.toml`: two `ignore` entries after RUSTSEC-2026-0258, each with a severity
  and chain comment and a `reason` citing #543. The two "seven" counts (header `:52`, section
  `:105`) become "nine".
- `xtask/tests/fdb_harness.rs:1240-1257`: both IDs added to the list
  `the_shipped_artifact_wall_carries_no_off_by_default_backend_exception` checks, and the
  count comment updated, so neither ID can later be copied into `deny.toml` unnoticed.

I applied it in the worktree, ran the full gate once (results below), then reverted it with
`git apply -R`. Afterwards `git diff` was byte-identical to `patch.diff`.

### Why it is not in `patch.diff`

- The deferred findings already hold this for the human: "A dependency bump or a recorded
  advisory exception has to be decided separately" (`deferred-findings.json`, items 5-6). The
  driver marks those "not build work".
- The maintainer's own precedent puts a waiver in a separate PR with its own issue (#790/#773),
  not inside a feature PR.
- Rubric, Reviewer protocol: an out-of-scope finding gets a decline with an issue reference,
  not an in-PR fix.
- All seven bundles in this batch share the base and all fail this step. One small PR fixes
  every one of them. Seven copies of the same edit would collide in `deny-all-features.toml`
  if their wording differed at all.
- GitHub CI runs the same deny step, so this PR cannot merge before the base is fixed anyway.
  Putting the waiver in it would not save the human a step. It would only hide a policy change
  inside a key-length PR.

Cost, so this can be checked: putting it in `patch.diff` would add 22 lines and change 3, in
two files outside the brief's Scope (`git diff --stat` of the waiver: `deny-all-features.toml |
19 +++++++++++++++++--`, `xtask/tests/fdb_harness.rs | 6 +++++-`). The reason to keep it out is
who decides, not size.

NEEDS-HUMAN external dependency: RustSec advisory DB (RUSTSEC-2026-0346, RUSTSEC-2026-0349 on tikv-client's rustls 0.21.12, published 2026-10-14) — the gating C4-ci row (`cargo xtask ci`) fails at its second `cargo deny` step on the unpatched base `7bf7eb9`, so no bundle on this base can show a green C4-ci. A ready fix is in this bundle as `base-advisory-waiver.diff` (both are medium/low, below the file's high/critical review trigger; no rustls 0.21.x fix exists). Land it as its own PR (as #790 did for RUSTSEC-2026-0258), then re-run this bundle's C4-ci. A full `cargo xtask ci` with patch + waiver applied is recorded under Verification.

```toml
[[doctor.checks]]
id    = "base-offbydefault-advisories"   # the base's off-by-default (tikv/fdb) advisory wall is clean
cmd   = "cd ../wyrd && cargo deny --all-features --config deny-all-features.toml check advisories >/dev/null 2>&1"
hint  = "a new RustSec advisory landed on the base's tikv/fdb tree: every bundle's C4-ci fails at the second cargo-deny step until a separate deny-all-features.toml PR records or clears it (#543; precedent #790)"
level = "WARN"
```

## Verification

- **Red→green, through the project's runner** (`engine/scripts/run-verify.sh`, with
  `PDCA_BUNDLE` = this bundle, `PDCA_LANE=0`, `PDCA_BRIEF_BASE=origin/main`, under
  `timeout 3000`), on the final `patch.diff`:
  - GREEN (fix applied): `5 passed; 0 failed`.
  - RED (production reverted, test kept): `1 passed; 4 failed`, all on assertions. Details
    under self-refutation (a).
  - `run-verify.sh: PASS — red without the fix, green with it (5 test(s) ran red)`.
- **Formatter and lints**: `cargo fmt --all` then `cargo fmt --all -- --check` clean; `typos`
  clean on all three touched files; `cargo clippy -p wyrd-gateway-s3 -p wyrd-server
  --all-targets` clean (workspace lints are deny-level). The repo sets no git commit hooks
  (`core.hooksPath` unset, no `.pre-commit-config.yaml`), so these CI steps are what a commit
  has to pass. rustfmt had aligned my new loop comment under the trailing comment on the line
  above (`lib.rs:4689`); I moved it into its own block (`:4691-4692`) and re-ran fmt.
- **Full gate, one complete run, patch + `base-advisory-waiver.diff` applied**
  (`./engine/xtask.sh ci` with `PDCA_WORKTREE` = the worktree, under `timeout 7000`): exit 0,
  `xtask ci: all checks passed`. Log: `builder-evidence/ci-full-with-waiver.log`. Every stage
  ran, in order: typos, `lint_docs.py`, `render_site.py --check` (`link audit OK`), gitlink /
  unsafe / blackbox guards, fmt, clippy, build, `cargo test --workspace --exclude wyrd-dst`,
  cargo-machete, `cargo deny check` (`advisories ok, bans ok, licenses ok, sources ok`), the
  all-features advisory wall (`advisories ok`, `:2937`), the all-features licence/bans/sources
  check, `xtask conformance` (`5 valid + 6 invalid vectors pass`, `:2941`), statics (`:2944`),
  deploy-guard (`:2947`; it has no standalone subcommand, so this is the first time it ran on
  this patch), DST clippy and `cargo test -p wyrd-dst` under `--cfg madsim`. Summed over every
  `test result` line: 1,769 passed, 0 failed. That includes the five wire tests (`:2161-2170`),
  the three in-crate tests (`:1324`, `:1334`, `:1339`), and the quarantine-boundary test with
  the two new IDs (`:2715`).
  This is evidence that nothing else in the gate is red. It is not a claim that C4-ci will be
  green on `patch.diff` alone: without the waiver it still stops at the advisory step, exactly
  as on the base.
- **The waiver's own test, flip-checked**: with the waiver applied, appending the text
  `RUSTSEC-2026-0349` to `deny.toml` turns
  `the_shipped_artifact_wall_carries_no_off_by_default_backend_exception` red
  (`fdb_harness.rs:1259`: "RUSTSEC-2026-0349 reaches us only through tikv-client …"). I then
  restored `deny.toml` (`git diff --quiet -- deny.toml`).
- **Back to the patch only**: `git apply -R base-advisory-waiver.diff`, then
  `git diff | cmp - patch.diff` matched, and `git apply --check base-advisory-waiver.diff` still
  passes on top.
- **Mutants** (`scripts/mutants-in-diff`, the C5 script, `PDCA_BUNDLE` set, `TMPDIR` under
  `$PDCA_SCRATCH`): `12 mutants tested in 38s: 11 caught, 1 unviable`, none missed (iteration 2:
  same result).

## Forced self-refutation

- **(a) Genuine red? Yes.** `run-verify.sh` applied `patch.diff` to a clean `origin/main`
  worktree, reverted the production change and kept the test. Result: `1 passed; 4 failed`,
  each on an assertion (`left: 200`, expected 400), not a compile error. Leg 1 `PUT 1,025-byte
  key`, leg 2 `PUT 342 × €`, leg 3 `DeleteObjects with a 1,025-byte key` (the body shows both
  `ok` and the long key `<Deleted>`), leg 5 `PUT /b%2Fa/<1,023 bytes>`. Leg 4 passes on both
  sides, as the brief's Falsifiability section says. With the fix: `5 passed; 0 failed`.
  Verdict line: `run-verify.sh: PASS — red without the fix, green with it (5 test(s) ran red)`.
  The helper swap cannot change this: the test only calls API that exists on the base
  (`uri_encode`, `sign`, `format_amz_date`, `Gateway::put_object`, `MetadataStore::commit`).
- **(b) Production path? Yes.** The wire test runs the real `S3Gateway::serve` on a loopback
  `TcpListener` in front of the real `wyrd_server::Gateway` (redb in-memory metadata,
  `FsChunkStore` on a tempdir, `MemCoordination`), as the peer suites do, with real
  SigV4-signed HTTP/1.1. Nothing is mocked. The in-crate router tests swap only the storage
  seam for a recording double, and still go through the real `router()` → PUT arm /
  `delete_objects` → `parse_delete_request`; their point is to show the seam is never reached.
- **(c) Fixture includes the fault? Yes.** Leg 1 sends the 1,025-byte key, and a `control`
  object must appear in the listing, so an empty or broken listing cannot pass "not listed".
  Leg 2 sends the 1,026-byte `€` key and the fully encoded 1,024-byte key. Leg 3 sends both
  over-limit entries (ASCII and 342 × `€`) next to a stored `ok` and checks `ok` survives.
  Leg 4 seeds a real 1,025-byte object through the backend and requires HEAD 200 before the
  DELETE, so the 204 cannot come from an absent key. Leg 5 sends the real `%2F` path.

## Ruled out

- **Guarding only the bulk-delete parser.** PUT could still create a key bulk delete then
  refuses: the #612 finding, and the brief's SELF-TEST.
- **Checking the limit on GET / HEAD / single DELETE.** That would strand legacy objects
  (criterion 4, principle C-1).
- **Measuring `"{bucket}/{key}"` or the wire path.** Both contradict criterion 2.
- **Dropping `encode_all` along with the other helpers.** `uri_encode` never escapes an
  unreserved byte, so leg 2's "path far longer than 1,024" case would lose its only spelling.
- **Putting the advisory waiver in this patch.** See above.
- **Bumping `tikv-client`.** 0.4.0 is the latest release; the TLS-stack upgrade
  (client-rust#541) is unreleased (`deny-all-features.toml:66-70`).

## Conflicts / ordering

Conflicts with #671 (size refusal in the same PUT arm). My refusals sit between the CopyObject
guard and the `content-type` read (`lib.rs:1817-1840`). This is wave 0.
