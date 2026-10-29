# Build notes — #743, validate: capability matrix + the smoke scenario

Target: `getwyrd/wyrd` `main`, built on the folded result of #744 and #853 (worktree base
`582da99`, `pdca-integrate: issue_853`). Every `path:line` below is on the patched tree unless it
says "base". The gateway lines the profile cites are unchanged between `12dd082` (the brief's
pin) and this base (`git diff --stat 12dd082 HEAD -- crates/gateway-s3` is empty).

## What the patch does

| Brief item | Where |
|---|---|
| Matrix module: operations, rows, profiles, version selection, judge, tally | `crates/validate/src/matrix.rs` (new) |
| The current profile, every row cited | `matrix.rs:317` (`PRE_0_1_ROWS`), `:488` (`PRE_0_1`), `:498` (`PROFILES`) |
| Version grammar and semver precedence, build metadata ignored | `matrix.rs:549` (`Version::parse`), `:612` (`Ord`) |
| `select`: one profile, or why none (`NotAVersion`, `Uncovered`, `Ambiguous`) | `matrix.rs:707` |
| Unsupported rows: declared refusal passes, any 2xx `unexpected-support`, else `protocol` | `matrix.rs:935` (`judge_refusal`), `:895` (`seen`) |
| Required rows' own checks (B′) | `matrix.rs:1065` (`judge_required`), `:1096`–`:1340` (`check_*`) |
| Run verdict: FAIL on any fatal finding, else INCONCLUSIVE unless every row passed | `matrix.rs:1358` (`Tally`), `:1417` (`verdict`) |
| The `smoke` scenario | `crates/validate/src/smoke.rs` (new) |
| First request: occupied-prefix refusal + `Server` header | `smoke.rs:154-245`, `:309` (`occupant`), `:108` (`server_version`) |
| Sweep, probes, ownership ledger, cleanup, summary | `smoke.rs:408`, `:544`, `:342`, `:669`, `:692`, `:702` |
| `--workers` ≠ 1 INCONCLUSIVE before any request | `smoke.rs:88` (`preflight`), called at `lib.rs:190` |
| Repeat until `--duration` | `smoke.rs:263-279` |
| New client verbs (HEAD, ranged/conditional GET, list v2/v1, DeleteObjects, probes) | `s3.rs:325`, `:396`, `:461`, `:485`, `:507`, `:557`, `:598` |
| Response head capture (status, `Server`, `ETag`, `Content-Length`, `Content-Range`) | `s3/capture.rs` (new) |
| Strict listing / delete-result readers | `s3/document.rs` (new), `:63`, `:106` |
| 1 MiB document budget, `ResponseGuard::document` | `s3/response.rs:65`, `:247` |
| `--server-version`, typed `--duration` / `--workers`, `--scenario` vocabulary | `args.rs:68`, `:94-101`, `:219-317` (`parse`; typing at `:254-275`), `:333` (`parse_duration`), `:361`, `:400` |
| FAIL exit status `4` | `lib.rs:65` |
| `run_with_profiles` (test-local profiles for (E)) | `lib.rs:162` |
| Docs currency | `docs/design/architecture/05-building-block-view.md:297` (the `validate` paragraph only) |
| Tests | `crates/validate/tests/capability_matrix.rs`, `crates/validate/tests/smoke_loopback.rs` (both new) |

Base lines touched (from `git diff -U0` hunk headers): `crates/validate/src/lib.rs:8-54,67-79,130-166`
(module docs, exit statuses, `render`, `run`), `src/args.rs:1-24,53-74,118-270` (surface),
`src/s3.rs:36-63,115,197-201,229-415` (verbs refactored onto answer-returning forms; `put_object`,
`get_object`, `delete_object` keep their signatures and behaviour), `src/s3/response.rs:13-18,48,
172-299,442,453` (budget, guard modes, `pub(super)` on the depth check),
`crates/validate/Cargo.toml:3,62` (description; `wyrd-traits` dev-dependency at new `:63-65`),
`docs/design/architecture/05-building-block-view.md:297`.

## Decisions, and why

**Rows for the GET forms and v1.** The brief lists "GetObject (plus a Range and a conditional
form)" and "ListObjectsV2 (plus the v1 form)". I made each form its own `Operation`
(`GetObjectRange`, `GetObjectConditional`, `ListObjects`), so each has its own row, its own check and
its own pass count, and (A) can assert them. 9 required + 12 unsupported = 21 rows.

**The judge reads typed errors only.** `seen` (`matrix.rs:895`) maps `S3Error` to status + code;
nothing looks at a message. A body error after a 2xx head (`AcknowledgedEarly`, a cut GET body) is
read as "a success arrived", so a probe that half-works through the PUT path is still
`unexpected-support`.

**5xx classing.** Required rows: a `5xx` other than `501` is `availability` (0017 §7 lists 5xx
there); `501` on a required row is `protocol`, since it is the server saying it does not implement a
required operation (`matrix.rs:969`). Unsupported rows: the brief's (B) says any wrong status or
code is `protocol`, and 0017 §3 says "not a 500", so a `500` to a probe is `protocol`, not
availability. Flagged for sign-off below.

**Smoke's availability budget is zero.** Any non-pass stops the run (`smoke.rs` `judge`). A
timed-out mutation needs 0017 §7's outcome resolution, which is out of this slice; stopping
INCONCLUSIVE with keys kept is the honest alternative to continuing on a guess.

**First request is also judged.** The occupancy ListObjectsV2 is judged as a `ListObjectsV2`
exchange with zero keys written (`smoke.rs:257-261`). This turned out to be a second line of
defence: with the occupancy refusal removed (red 6), the run still stopped before any write, as an
`oracle` FAIL.

**Probes before listings.** A probe that writes a key while answering the declared refusal (the
"refused but mishandled" shape of #504) shows up as a key nobody wrote in the sweep's listing.

**Probe targets.** CopyObject, UploadPart, CompleteMultipartUpload write under
`<run-id>/sweep-<n>/`. CreateBucket and DeleteBucket name `wyrd-validate-<16 hex of sha256(run
id)>` (`smoke.rs:80`), never `--bucket`: a DeleteBucket that started working must not be able to
remove the operator's bucket. Read-only bucket probes (HeadBucket, GetBucketLocation,
ListMultipartUploads) use `--bucket`.

**Ownership ledger.** Keys (and the scratch bucket) are recorded before the request that could
create them, removed when the server confirms deletion or refuses as declared
(`smoke.rs:425-453`, `:619-645`). `delete_owned` / `delete_objects_owned` refuse any key not in
the ledger (`smoke.rs:669`, `:680`). Cleanup on PASS deletes from the ledger only; the sweep's own
deletes normally leave it empty.

**Strict document reading.** The brief asks every new verb to go "through the same bounded,
classified reading" as #853. #853 replaced the SDK's lax error-document reader; the SDK's success
readers are just as lax, so listings and delete results are re-read with `roxmltree` under the same
rules (`s3/document.rs`). Budget sized for the worst-case encoded page at 100 keys (comment at
`s3/response.rs:52-65`), with `max-keys` and bulk-delete size capped at `LIST_MAX_KEYS = 100`
(`s3.rs:167`) so the budget holds.

**`Content-Range` parsed, not string-compared.** RFC 9110 §14.1 makes the unit case-insensitive;
an exact string compare would have failed a conforming `BYTES 10-19/36`. `byte_content_range`
(`matrix.rs:1054`) checks every token. Found in self-review against the rubric's grammar
strictness rule.

**`--server-version` refused at parse when malformed** (`args.rs:268-273`), rather than INCONCLUSIVE
later: it is operator input, so a usage error is the right answer.

**Clock.** One source for the run's duration: `tokio::time::Instant`, read at the start of
`smoke::run` (`smoke.rs:162-165`) for the start, the deadline check and the summary's elapsed
time. No `SystemTime::now()`.

## Alternatives ruled out

- **Leave the SDK to parse listings.** Fewer lines (the two readers are ~150 lines plus ~110 of
  unit tests in `s3/document.rs`), but the SDK accepts a cut-off document, which is exactly what
  #853 removed for errors; a truncated listing read as complete would hide keys from the `oracle`
  check.
- **One `Answered`-less verb per form** (e.g. `get_object_range`, `get_object_if_none_match`).
  Rejected for `GetOptions` on one verb: same reading path, one place for the range validation.
- **`PutOutcome` gains a `status` field.** Would break `crates/validate/tests/s3_client_upload_peers.rs:1364`
  (`PutOutcome { etag: None }`, base). Added `put_object_answer` returning `Answered<PutOutcome>`
  instead; `put_object` is a 3-line wrapper (`s3.rs:317-321`).
- **Plant the mid-run object with a hook in production code.** Rejected: the test's relay plants it
  server-side on the first mutating request, so no test seam enters the product.

## Test environment

- In-process gateway: the composition of `s3_client_roundtrip.rs:323-350`, plus
  `S3Config::server_version` (`crates/gateway-s3/src/lib.rs:111-116`) and a seeded bucket record.
  The seeding was needed: listing an empty, never-created bucket answers `404 NoSuchBucket`
  (`crates/server/src/lib.rs:746-758`, ADR-0046 decision 4), and CreateBucket is #511's. I mirrored
  `crates/server/tests/s3_list_objects.rs:82-89`, which needs `MetadataStore::commit`, so
  `wyrd-traits` became a **dev**-dependency of `wyrd-validate` (`crates/validate/Cargo.toml:63-65`).
  `cargo xtask blackbox-guard` still passes (the normal closure holds no `wyrd-*`).
- A loopback relay in the test records every request head (and each POST body), counts
  connections, can strip `Server`, and can plant an object through the backend. What a run left
  behind is read from the backend (`Gateway::get_object`), not through the tool's client.
- Every run in the test is bounded: `run_with_profiles` on its own thread with a 120 s
  `recv_timeout`, the binary polled and killed past 120 s.

## Demonstrated reds (each a temporary edit, the named leg run, the file restored)

Runner: `timeout 900 cargo test -p wyrd-validate --test <file> [filter]`, the invocation
`engine/scripts/run-verify.sh:441` uses (there is no single-test `cargo xtask` subcommand), always
under `timeout`. After the eight edits the production sources were compared file by file against
a diff checkpoint taken before them: all identical.

1. **A 2xx to an unsupported row judged a pass** (`judge_refusal`'s 2xx arm → `Verdict::Pass`):
   `capability_matrix` 3 red — `b_any_success_to_an_unsupported_row_is_unexpected_support_and_fatal`,
   `b_one_fatal_result_makes_the_run_fail_whatever_else_passed`,
   `c_each_version_range_selects_its_own_profile_and_its_own_verdict`; and
   `e_a_profile_declaring_put_object_unsupported_fails_the_run_as_unexpected_support` red
   ("exit 3", expected 4).
2. **CopyObject declared `400`** (`PRE_0_1_ROWS`): both (D) tests red, each naming
   `protocol: CopyObject answered 501 with code NotImplemented; it is declared unsupported (#766):
   it must answer 400 NotImplemented` (and `g_a_pass_…` red the same way).
3. **Selector ignores the version** (`select` returns `profiles.last()`): all three (C) tests red
   (`0.1.5` selected `late`; `0.3.0` selected something).
4. **Sweep keys outside the run prefix** (`format!("sweep-{n}/")`): `g_a_pass_…` red
   ("PUT /validate/sweep-0/obj-0 writes outside `smoke-g-own/`"), and (E)'s key-prefix assert too.
5. **Ranged GET passed on any 2xx, DeleteObjects on its status alone** (two early returns):
   `b_prime_a_ranged_get_…` and `b_prime_a_bulk_delete_…` red ("expected a failure, got Pass").
6. **Occupied-prefix refusal removed** (`occupant(..).filter(|_| false)`):
   `g_an_occupied_run_prefix_is_refused_before_any_write` red ("exit 4", expected 2). No write was
   sent even so (see "First request is also judged").
7. **`--duration` ignored** (break after the first sweep): `h_smoke_repeats_…` red ("elapsed
   0.165 s", one sweep against `--duration 2s`).
8. **Extra, not in the brief's list — cleanup by listing** (`clean_up` deletes every key a listing
   of `<run-id>/` shows): `g_a_pass_…` red, the object planted mid-run under `<run-id>/` deleted.

## Refuting my own tests

- **(a) Genuine red?** Yes, per the eight reverts above, each run and seen failing on the named
  leg. On the plain base the two files do not compile (`wyrd_validate::matrix`,
  `run_with_profiles`, `smoke` do not exist), so C4-verify will report UNVERIFIABLE (exit 77), as
  the brief's verification posture pre-declares.
- **(b) Production path?** Yes. `capability_matrix.rs` calls the production `matrix::judge`,
  `Profile::judge`, `select`, `Tally::verdict` on hand-built answers; it re-implements nothing.
  `smoke_loopback.rs` runs the production `run_with_profiles` (and, in
  `d_the_real_binary_runs_smoke_to_a_pass`, the real `wyrd-validate` binary) with the production
  `S3Client` over real TCP into the real `S3Gateway` + `wyrd_server::Gateway` on redb and a local
  `FsChunkStore`. The relay only forwards bytes (and strips `Server` in one leg). The test-local
  profile in (E) is the fixture the brief names (`0017:753-758`), not a stand-in for the gateway.
- **(c) Fixture includes the fault?** Yes. (D)/(G)/(H) run against the real gateway's real answers
  for all 21 operations, so a wrong declared row goes red (red 2). (E)'s fault is the profile
  mismatch itself. (F)'s "header absent" is produced by actually stripping the header on the wire,
  and `wyrd/unknown` is the real `S3Config::new` default. (G)'s planted objects are real objects in
  the backend, one before the run, one written mid-run between the occupancy check and the first
  PUT. (H)'s `--workers 2` leg counts relay connections (zero).

## Commit-readiness

- `cargo fmt --all -- --check`: clean.
- `cargo clippy -p wyrd-validate --all-targets` (workspace lints, warnings denied): clean.
- `cargo test -p wyrd-validate`: all targets green — lib 7, `capability_matrix` 19, `cli_surface`
  21 (unmodified), `payload_and_sizes` 26, `s3_client_nonconforming_responses` 28,
  `s3_client_roundtrip` 15, `s3_client_upload_peers` 12, `smoke_loopback` 11.
- `typos crates/validate docs/design/architecture/05-building-block-view.md`: clean (renamed a
  `NoProfile` variant to `NotAVersion` after typos refused "Unparseable").
- `python3 docs/publishing/tools/lint_docs.py`: OK; `render_site.py --check` (out dir under
  `$PDCA_SCRATCH`): link audit OK.
- `cargo xtask blackbox-guard`: passes.
- `cargo doc -p wyrd-validate --no-deps` fails on a **pre-existing** private intra-doc link in
  `crates/validate/src/sizes.rs:340` (`[Tuning::check]`, from #744, untouched here). `cargo xtask
  ci` does not run rustdoc, so no gate sees it; this patch adds no rustdoc warning of its own (it is
  the only one reported). Out of scope; worth a one-line follow-up.
- Full gate `/home/eddie/wyrd/wyrd-pdca/engine/xtask.sh ci` (= `cargo xtask ci` in the worktree,
  under `timeout 5400`), on the final tree: **`xtask ci: all checks passed`, exit 0** — guards
  (gitlink, unsafe, blackbox), typos, `lint_docs.py`, `render_site.py --check`, fmt, clippy, build,
  `cargo test --workspace --exclude wyrd-dst` (in it: `capability_matrix` 19 ok,
  `smoke_loopback` 11 ok), cargo-machete (no unused dependencies), the three `cargo deny` runs
  (advisories ok on the all-features wall too), statics, deploy-guard, and the `wyrd-dst`
  clippy + test tier.
- `patch.diff` is `git diff HEAD` on the worktree; `git apply --cached --check` against a
  fresh index of `582da99` passes.

External dependencies the brief names (`typos`, `docs-renderer`): both installed and run. None
missing.

Scratch: everything I wrote outside the worktree and this bundle is under `$PDCA_SCRATCH`
(`/var/tmp/pdca/wyrd-pdca-9c587031/issue_743/pdca-builder-743-*`: the diff checkpoints, a
throwaway git index, the docs render, the gate log). Left for the harness to reclaim with the
directory it created. The worktree's index carries `git add -N` entries for the six new files, so
`git diff` shows them.

## For sign-off (not NEEDS-HUMAN blockers; decisions to confirm)

- **Scale reading** (brief's Open question): followed — sweeps repeat until `--duration`;
  `--workers` ≠ 1 is INCONCLUSIVE before any request.
- **5xx to an unsupported row is `protocol`**, a 5xx (not 501) to a required row is
  `availability` (above). A different reading changes one arm of `judge_refusal`.
- **Zero availability budget for smoke** (above).
- **`wyrd-traits` dev-dependency** for seeding the bucket record in tests.
- **A deployment's bucket must exist** (have its record) for smoke to start: against a bucket with
  no record the first ListObjectsV2 answers `NoSuchBucket` and smoke ends INCONCLUSIVE at once,
  naming the error. Correct behaviour, but an operator will hit it until #511 gives them
  CreateBucket.
- **Untagged builds** (brief's Open question): unchanged — every `0.0.0+git.<sha>` build selects
  `pre-0.1`; the decision about editing it when #508 lands is still open.
