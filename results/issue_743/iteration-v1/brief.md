# validate: capability matrix + the smoke scenario — issue 743

> The Plan artifact for #743. Do reads ONLY this file. The `- **Label:** value` lines are parsed
> by the driver; keep their shape. The plan of record is proposal 0017,
> `docs/design/proposals/draft/0017-blackbox-validation-tool.md`: read **§3 The capability
> matrix** (`0017:188-275`), **§7 Failure taxonomy** (`0017:482-524`), the `smoke` row and cleanup
> rule of **§8** (`0017:526-561`), and **§5**'s run-id rule (`0017:386-389`) before writing code.
> Citations verified against `getwyrd/wyrd` `origin/main` @ `12dd082` on 2026-10-19.
>
> **Two things changed since the issue and 0017 were written.** #736 landed: the gateway now
> answers every request with `Server: wyrd/<version>` (`crates/gateway-s3/src/lib.rs:1699`, value
> built at `:126-145`), so the version can be read from the wire and `--server-version` becomes an
> override. And `CopyObject`'s row points at #766 (the 2026-08-16 tracker comment), not the closed
> #504.

- **Slug:** validate-capability-matrix-smoke
- **Track:** blackbox
- **Kind:** enhancement (design proposal)
- **Goal:** `wyrd-validate` resolves its configuration and exits INCONCLUSIVE; no scenario runs.
  This slice adds the first one. **The capability matrix** is a pure, reviewed table: every S3
  operation the tool knows is declared `required`, or `unsupported(#NNN)` with the exact status and
  S3 error code it must answer. It is kept as immutable per-release profiles, one of which is
  selected by the server's version. **The `smoke` scenario** runs every `required` operation once
  and asserts every `unsupported` one fails exactly as declared, against a live endpoint, and
  ends PASS, FAIL or INCONCLUSIVE. An `unsupported` operation that **succeeds** is a fatal
  `unexpected-support` failure: #504's silent-overwrite class, generalised, as a standing check.
- **Success criterion:** Two NEW test files.
  **`crates/validate/tests/capability_matrix.rs`** (pure, no network):
  - **(A) The table.** The current profile declares exactly these rows, each with a citation:
    `required`: `PutObject`, `GetObject` (plus a `Range` and a conditional form), `HeadObject`,
    `DeleteObject`, `DeleteObjects`, `ListObjectsV2` (plus the v1 form);
    `unsupported`: `CopyObject` → `501 NotImplemented` (#766); `CreateMultipartUpload`,
    `UploadPart`, `CompleteMultipartUpload`, `AbortMultipartUpload`, `ListParts`,
    `ListMultipartUploads` → `501 NotImplemented` (#508); `GetBucketLocation` → `501
    NotImplemented` (#511); `CreateBucket`, `DeleteBucket`, `ListBuckets` → `400 InvalidRequest`
    (#511); `HeadBucket` → `400` with no body (#511).
  - **(B) Classification.** Fed an observed answer, the pure classifier returns: pass for a
    declared failure answered exactly; `protocol` (fatal) for a wrong status or code; and
    **`unexpected-support` (fatal) for any 2xx to an `unsupported` row**. A run with one such
    result is FAIL whatever else passed.
  - **(B′) Required rows have checks of their own, not "any success".** Fed an observed answer,
    the classifier passes each required row only when (0017 §7 classes):
    `PutObject` 200 with an ETag; whole `GetObject` 200 whose body is the derived payload
    (else `integrity`); ranged `GetObject` (`Range: bytes=a-b`) **206** with `Content-Range: bytes
    a-b/<size>` and exactly those bytes (`crates/gateway-s3/src/lib.rs:2713-2719`); conditional
    `GetObject` (`If-None-Match: <its ETag>`) **304** with no body (`:2785-2803`); `HeadObject` 200
    with the size and the PUT's ETag; `ListObjectsV2` and v1 200, naming exactly the keys this run
    wrote under its prefix, with their sizes (else `oracle`); `DeleteObject` 204, after which a
    `HEAD` of the key answers 404; `DeleteObjects` 200 with every requested key under `<Deleted>`
    and **no `<Error>` row** (the gateway answers 200 even when keys failed,
    `crates/gateway-s3/src/lib.rs:2293-2300`). Negative fixtures, each a `protocol` (or the named
    class) result and so a FAIL run: a 200 with the whole body to the ranged GET; a 206 with the
    wrong `Content-Range`; a 200 to the conditional GET; a `DeleteObjects` 200 carrying one
    `<Error>` row; a GET body one byte off (`integrity`). This is smaller than #746's oracle: it
    checks each answer against what this sweep just did, and keeps no model across sweeps.
  - **(C) Version keying.** With a test-local table of two profiles covering different version
    ranges, a version in each range selects its own profile and its own expectation set (a row
    `unsupported` in one and `required` in the other flips its verdict). Build metadata (`+git.…`)
    does not change the selection. A version that parses but matches no profile, an unparseable
    one, and `unknown` each select nothing.
  **`crates/validate/tests/smoke_loopback.rs`** (against an in-process Wyrd gateway, the
  composition `crates/validate/tests/s3_client_roundtrip.rs:318-350` uses), through `run` or the
  real binary, with `--workers 1` and a short `--duration` (e.g. `1s`) unless the leg says
  otherwise:
  - **(D) PASS against loopback.** With the gateway's `S3Config::server_version` set to a
    `0.0.0+git.<sha>` value, `--scenario smoke` exits with the PASS status. Every required
    operation ran and passed (B′)'s checks against the real answers; every unsupported operation
    was sent and answered as declared, `CopyObject` included.
  - **(E) A wrongly succeeding unsupported operation fails the run.** The same gateway with a
    test-local profile that declares `PutObject` `unsupported` exits with the FAIL status and
    names `unexpected-support` for `PutObject` (no MinIO needed: the profile mismatch is the
    fixture, as 0017 §14 says, `0017:753-758`).
  - **(F) The version source.** Header present → used, and the echo says `(from Server header)`.
    `--server-version` given → it wins and the echo says `(given)`. Header absent or
    `wyrd/unknown` and no flag → INCONCLUSIVE, naming `--server-version`, before any mutating
    request is sent.
  - **(G) Only its own keys.** Every key smoke writes is under the `--run-id` prefix; on PASS none
    is left; on FAIL the keys involved are left in place (0017 §8 cleanup). **An occupied prefix is
    refused before any write** (`0017:791-794`): with one object planted under `<run-id>/` before
    the run, smoke exits with the usage status (2), names the occupied prefix and `--run-id`, and
    sends no mutating request; afterwards the planted object is byte-identical and is still the
    only key under the prefix. **Deletes are by ownership, not by name** (`0017:811-816`): cleanup
    deletes only keys this run wrote and recorded, never keys found by listing the prefix.
  - **(H) Scale arguments** (`0017:528-531`, `:555-561`). Smoke repeats its sweep until
    `--duration` has elapsed (at least one full sweep; a sweep running at the deadline finishes).
    With `--duration 2s` against loopback: PASS, the summary states the number of sweeps and the
    elapsed time, and the elapsed time is at least 2 s. With `--workers` other than `1`, smoke
    ends INCONCLUSIVE (exit 3) naming `--workers`, after the configuration echo and before any
    request is sent (the test's gateway sees no request). A `--duration` that does not parse
    (e.g. `7x`) is refused by name with the usage status. `crates/validate/tests/cli_surface.rs`
    passes **unmodified**: its `--workers 7` (`cli_surface.rs:30`) now ends INCONCLUSIVE for this
    reason, so those tests stay off the network instead of reaching for `127.0.0.1:9000`.
- **Falsifiability:** RED is reachable on the plain `cargo test -p wyrd-validate` target: (A)–(C)
  are pure; (D)–(G) run a real Wyrd gateway in-process on loopback, so a wrong declared code or a
  classifier that lets a 2xx through goes red against the real answers, with no Docker or network
  beyond `127.0.0.1`. (E) is the flippable case the issue asks for: the run's verdict flips with
  one row of the profile. (B′)'s negative fixtures are pure answers fed to the classifier, so a
  runner that accepts any success on a required row goes red there; (G)'s planted object and
  (H)'s short duration are set up by the test, not left to timing.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** 741, 744, 853
- **Conflicts with:**
- **Ordering note:** `Depends on 744`: smoke sends #744's derived payloads, and both slices edit
  `crates/validate/src/args.rs`, `src/lib.rs` and the `validate` paragraph of
  `docs/design/architecture/05-building-block-view.md:297`. `Depends on 853`: #853 is rewriting how
  the client reads every response (`crates/validate/src/s3.rs`, `src/s3/*`), and this slice adds
  most of the client's verbs to that same file, so it must build on #853's result. **#853 runs in
  the same batch** (the human's decision in the 2026-10-19 Plan session: the run's ids include 853,
  so its pending rebuild and sign-off happen in this run). With 853 in the batch the driver levels
  it as: wave 0 653, 657, 744; wave 1 658, 853; wave 2 659, 743. This bundle builds on the folded
  result of #744 and #853. Without 853 among the run's ids the driver refuses the whole run
  (`waves.check_dep_graph`: a dependency neither in the batch nor COMPLETE).
  `Depends on 741`: the issue's own prerequisite (split parent, merged). Intake cap
  (wyrd-pdca-P1): blackbox 2/6 when written, room 4. **No `Conflicts with` 658, on purpose:** it
  edits the multipart paragraphs of the same docs file (`:244-248`), about fifty lines from this
  slice's `:297`; the edits do not overlap and a fold merges them cleanly. Keep this slice's docs
  edit to the `validate` paragraph.
- **Scope:** the matrix module (profiles, version selection, classification), the `smoke` scenario
  and the code that runs it, the client verbs smoke needs, `--server-version` and reading the
  `Server` header, the occupied-prefix refusal and ownership-based cleanup, typing `--duration`
  and `--workers` for smoke (repeat until the duration; one worker), a FAIL exit status, the docs
  paragraph, and the two new test files. / out of scope:
  - `--allow-non-empty` and `--dry-run` (0017 §15's other guards): a later slice;
  - the negative authentication vectors and #491's known-open handling (#745), although 0017 says
    `smoke` turns them on: that is #745's to add to this scenario;
  - the oracle, the event stream and the verdict artifact (#746, #747): smoke's verdict here is its
    exit status and stderr summary;
  - MinIO and the reference profile (#751); the other scenarios (`endurance`, `churn`, …);
  - size classes and payload generation (#744; this slice only uses them, at a small size);
  - any gateway change (a `400 InvalidRequest` for bucket verbs is recorded as today's answer, not
    fixed here);
  - any `wyrd-*` crate in `[dependencies]`; `deny*.toml`.
- **Difficulty:** high
- **Do model:** opus
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/validate/tests/smoke_loopback.rs`, a **NEW** file (C4-verify treats an
  added `crates/*/tests/*.rs` as the per-fix discriminator); `crates/validate/tests/capability_matrix.rs`
  is also NEW.
- **Verification posture:** (a) net-new. The matrix and smoke do not exist on the base, so the
  tests cannot compile on C4-verify's reverted base and C4-verify reports **UNVERIFIABLE** (exit
  77), a pre-declared sign-off item.
  **Built and exercised at Check:** the matrix (pure tests) and smoke end to end against a real
  in-process gateway.
  **Demonstrated reds (required, recorded in `build-notes.md`).** Do makes each temporary edit,
  shows the named leg fail, then restores the file:
  - a 2xx to an `unsupported` row classified as a pass (or a warning) → (B) and (E) fail;
  - `CopyObject`'s declared status changed to `400` → (D) fails;
  - the selector ignoring the version (always the newest profile) → (C) fails;
  - a key written outside the run-id prefix → (G) fails;
  - the ranged GET accepted on any 2xx, and `DeleteObjects` accepted on its status alone (two
    edits) → (B′) fails;
  - the occupied-prefix check removed → (G)'s planted-object case fails;
  - `--duration` ignored (one sweep, then PASS) → (H) fails.
- **Production reach:** the tool is the product here; the binary runs smoke for real. No Wyrd
  server code changes.
- **Citations expected:** Do must cite `path:line` on the target branch for every change.
  - **Today's answers the profile records** (each row cites its line): `CopyObject`
    `crates/gateway-s3/src/lib.rs:1865-1871`; the object-path subresource denylist (`uploads`,
    `uploadId`, `partNumber`) `:1842-1849` with the list at `:410-422`; the bucket-path denylist
    (`?uploads`, `?location`) `:1797-1804`; a bucket path with any method but `GET` (and `POST
    ?delete`) `:1810-1815`; the root path `/` `:1819-1826`. Smoke checks every row against the real
    gateway, so a wrong citation shows up as a red (D).
  - **Peer for the in-process gateway:** `start_gateway_with_chunk`,
    `crates/validate/tests/s3_client_roundtrip.rs:323-350`; set `server_version` on its
    `S3Config` (`crates/gateway-s3/src/lib.rs:111-116`).
  - **Peer for the client's reading of responses:** whatever #853 lands in
    `crates/validate/src/s3.rs` and `src/s3/`; every new verb goes through the same bounded,
    classified reading, and its errors through `S3Error` (`src/s3/error.rs:14`).
  - **The version string's shape:** `normalize_describe`
    (`crates/server/src/version/derivation.rs:30-41`, the function at `:41`): `0.0.0+git.<sha>[.dirty]` untagged,
    `0.1.0` on a tag, `0.1.0+git.3.<sha>` past one.
  - **Exit statuses:** `crates/validate/src/lib.rs:35-44` (0, 1, 2, 3 taken); FAIL needs its own.
- **Prior-art check (triage cycles):** searched by path `crates/validate/` and
  `docs/design/architecture/05-building-block-view.md` on `origin/main`. Merged: the crate and
  argument surface (PR #845), the S3 client (PR #859, #854), the version header (#736). In flight:
  #853, #744 (this batch). No matrix or scenario code exists. Neither new test file exists. Open
  PRs on getwyrd/wyrd: none.
- **Disposition hint:** new-feature

**Sources for claims taken from the tracker** (for the sign-off reader): the 2026-08-16 comment on
version keying, the `CopyObject` row and sizing,
https://github.com/getwyrd/wyrd/issues/743#issuecomment-5308492891; copy in
`results/issue_743/notes.json`.

## Motivation

#504 was live data loss from an operation nobody had implemented: `CopyObject` ignored
`x-amz-copy-source` and overwrote the destination with an empty body. The fix rejects it, and the
repo already has a regression test for that: `crates/server/tests/s3_copy_object_guard.rs:175`
asserts the `501 NotImplemented` and that the destination's bytes survive. What does not exist is
a check an **operator** can run against a **deployed** endpoint, keyed to the version it reports:
one that catches a build where an operation the release does not support starts half-working,
and that covers every unsupported operation, not only the one that already bit. The matrix is that
check, and `smoke` is the first thing the tool can actually run.

## Design

**Matrix module** (pure). A row is an operation, its state, and a citation; an `unsupported` row
carries the issue that will implement it, the status, and the S3 error code (or "no body" for a
`HEAD`). A **profile** is an immutable table for one version range; profiles are retained, never
edited in place, once a release ships (0017 `:263-272`). Selection compares the version's release
part (`MAJOR.MINOR.PATCH`, with any pre-release), ignoring `+` build metadata. Today there is one
profile, covering every version below `0.1.0` (all builds so far). The classifier maps
`(row, observed answer)` to pass, `protocol` or `unexpected-support`, using the client's typed
`S3Error` and never a message string.

**First request: the occupied-prefix check, which also reads the version.** One `ListObjectsV2`
with `prefix=<run-id>/` and `max-keys=1` (`0017:791-794`). Anything listed ⇒ exit with the usage
status (2) before any write, naming the prefix and `--run-id`. `--allow-non-empty` is not part of
this slice, so an occupied prefix is always refused. The same response's `Server` header gives the
version: `wyrd/<version>` is used; `--server-version` overrides it; neither usable means
INCONCLUSIVE before any mutating request. The echo states which source was used.

**Smoke, one sweep.** Under `<run-id>/sweep-<n>/`: PUT objects (#744 payloads at a small size),
GET one whole, as a range and conditionally (`If-None-Match` with its ETag), HEAD it, list the
sweep's prefix with v2 and v1, delete one key singly and others with `DeleteObjects`; then send each
`unsupported` operation once and classify its answer. Each answer goes through (B′)'s checks.
Requests that could write (`CopyObject`, `UploadPart`, `CreateBucket`) target keys and buckets
under the run prefix only. The run keeps an in-memory set of every key it wrote; cleanup deletes
from that set only, never from a listing (`0017:811-816`). On a clean sweep, delete what it wrote;
on FAIL, keep it and print the keys.

**Scale.** 0017 makes scale orthogonal to the scenario and requires wall-clock within tolerance of
`--duration` before PASS (`0017:528-531`, `:555-561`). So smoke repeats its sweep until
`--duration` has elapsed (at least one sweep; the sweep running at the deadline finishes), and a
run that stops early for any reason but a FAIL is INCONCLUSIVE. `--duration` gets a small grammar
here (`<n>s`, `<n>m`, `<n>h`, `<n>d`), refused by name if it does not parse. Smoke runs one
worker in this slice: concurrent sweeps belong to the scenarios with worker pools. A `--workers`
other than `1` is a scale smoke cannot honour, so under the vacuity rule the run is INCONCLUSIVE,
named, before any request; it is never ignored and never a PASS. An availability failure on the
first request ends the run INCONCLUSIVE at once rather than retrying until `--duration`. Verdict:
PASS (exit 0), FAIL (a new, distinct non-zero status) or INCONCLUSIVE (exit 3).

**Client verbs.** HEAD, ranged and conditional GET, ListObjectsV2 and v1, DeleteObjects, and the
unsupported operations, through the SDK, with the same deadlines and the same response reading as
the existing verbs.

**Arguments.** `--server-version` joins #744's optional set (the ten required flags stay as they
are). `--scenario` stays a required flag; `smoke` runs, and any other value is refused by name for
now, listing the scenarios that exist. `--duration` and `--workers` stay required and are now
typed as described under Scale; their parsing lives in `args.rs` beside the existing strict
parser, and `crates/validate/tests/cli_surface.rs` passes unmodified (see (H)).

**Docs currency** (`AGENTS.md:154-157`): extend the `validate` paragraph at
`docs/design/architecture/05-building-block-view.md:297`: the matrix and its version keying, the
`smoke` scenario, and the exit statuses.

## Alternatives considered

- **Flip rows in place as features land.** Discards the expectations for every release where the
  operation was unsupported, so a newer tool fails an older server (0017 `:263-272`). Rejected.
- **Treat `unexpected-support` as a warning.** The day an unimplemented operation half-works is the
  day to hear about it (0017 `:197-199`). Rejected; fatal.
- **Require `--server-version` always.** #736 makes the wire the better source. Rejected; it is an
  override.
- **Declare `501 NotImplemented` for the bucket verbs, as S3 would.** Smoke would be red against
  every current build for a reason #511 owns. Rejected; the profile records what the gateway
  answers today, and #511's slice changes the row when it changes the answer.

## Impact & compatibility

The binary now runs a scenario instead of always exiting INCONCLUSIVE, and gains one optional flag
and one exit status. No Wyrd server code changes.

## Open questions

- **Size, and a split that also unblocks it.** The 2026-08-16 comment already called this
  oversized. A natural split: **(1)** the matrix module, version selection and `--server-version`
  (pure; touches no client code, so it need not wait for #853); **(2)** smoke, the client verbs and
  the binary wiring, depending on (1), #744 and #853. The human's call. The 2026-10-20 plan-review
  revision added (B′), the occupied-prefix check and (H); under the split, (B′)'s pure checks go to
  (1) and the rest to (2).
- **Scale for smoke (decided at that revision, for sign-off to confirm).** 0017's smoke row says
  "once, no time axis" while §8 makes `--duration` and `--workers` apply to every scenario and
  gates PASS on wall-clock. This brief follows §8 for `--duration` (repeat the sweep until it
  elapses) and ends INCONCLUSIVE for `--workers` other than `1`. The other reading (one sweep,
  `--duration` as a ceiling) would need 0017's vacuity rule amended first.
- **Untagged builds.** Every build before a tag reports `0.0.0+git.<sha>`, so profiles can tell
  releases apart but not two builds of `main`. When #508 lands, `main` builds will support multipart
  while still reporting `0.0.0+…`. Whether the pre-0.1 profile is then edited in place (it describes
  no release yet) or keyed some other way needs deciding before #508 lands.
- **The bucket verbs' `400 InvalidRequest`.** Recorded as today's answer. Whether it should be
  `501 NotImplemented` until #511 lands is a gateway question for #511, not this tool.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

Plan-review response (2026-10-20, revision pass): all four findings accepted and the brief
revised. (1) Motivation now cites the existing `CopyObject` regression test
(`crates/server/tests/s3_copy_object_guard.rs:175`) and states the real gap: operator-run,
version-keyed validation of a deployed endpoint. (2) New (B′) gives each required row its own
status and result checks (206 + `Content-Range`, 304, `DeleteObjects` with no `<Error>` row, bytes
equal to the payload) with negative fixtures, and (D) runs them against real answers. (3) New (H)
and a Scale section: smoke repeats until `--duration`, and `--workers` other than `1` ends
INCONCLUSIVE before any request, which also keeps `cli_surface.rs` passing unmodified; flagged
for sign-off in Open questions. (4) (G) adds the occupied-prefix refusal (planted object, no
mutation, bytes unchanged) and ownership-based cleanup.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Oversized (250 KB, ~3,100 lines of production code) and never split. Split along brief.md:267-271: (1) the capability matrix, version selection and --server-version: pure, no network, no wait on #853; (2) smoke, the client verbs and the binary wiring, depending on (1), #744 and #853. The three reproduced defects all belong to (2) and must be required reds there: - the DeleteBucket probe can delete a bucket the run does not own; refuse a scratch name equal to --bucket, and never send a destructive probe without ownership (pre-existing-bucket fixture); - a bulk delete acknowledged but not done still gives PASS and a false "nothing left" summary; verify absence before dropping keys from the ledger (acknowledging-relay fixture); - any non-empty ETag (including * and unquoted tags) is accepted and reused as If-None-Match; validate entity-tag grammar (wildcard and malformed fixtures). Also for (2): keep a 2xx head across a body timeout so it is unexpected-support, not INCONCLUSIVE; test the --scenario refusal (endurance -> exit 2, no connection), the --server-version parse refusal, and the h/d duration units. Open for Plan to settle with the human (sign-off leaned, not decided): a 0.0.0+git.unknown server version should end INCONCLUSIVE naming --server-version (the brief's "unknown selects nothing" rule); conditional GET should get a negative control (a non-matching tag must answer 200 with the bytes). Reframe the CopyObject premise: crates/server/tests/s3_copy_object_guard.rs already checks the 501; this slice is about operator-run, version-aware validation of deployed endpoints.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Oversized (250 KB, ~3,100 lines of production code) and never split. Split along brief.md:267-271:
  (1) the capability matrix, version selection and --server-version: pure, no network, no wait on #853;
  (2) smoke, the client verbs and the binary wiring, depending on (1), #744 and #853.
  The three reproduced defects all belong to (2) and must be required reds there:
  - the DeleteBucket probe can delete a bucket the run does not own; refuse a scratch name equal to --bucket, and never send a destructive probe without ownership (pre-existing-bucket fixture);
  - a bulk delete acknowledged but not done still gives PASS and a false "nothing left" summary; verify absence before dropping keys from the ledger (acknowledging-relay fixture);
  - any non-empty ETag (including * and unquoted tags) is accepted and reused as If-None-Match; validate entity-tag grammar (wildcard and malformed fixtures).
  Also for (2): keep a 2xx head across a body timeout so it is unexpected-support, not INCONCLUSIVE; test the --scenario refusal (endurance -> exit 2, no connection), the --server-version parse refusal, and the h/d duration units.
  Open for Plan to settle with the human (sign-off leaned, not decided): a 0.0.0+git.unknown server version should end INCONCLUSIVE naming --server-version (the brief's "unknown selects nothing" rule); conditional GET should get a negative control (a non-matching tag must answer 200 with the bytes).
  Reframe the CopyObject premise: crates/server/tests/s3_copy_object_guard.rs already checks the 501; this slice is about operator-run, version-aware validation of deployed endpoints.
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 379 mutants tested in 41m: 54 missed, 225 caught, 99 unviable, 1 timeouts
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 8 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_743/review-b
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
