# Result — issue 743 / validate-capability-matrix-smoke

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: `wyrd-validate` resolves its configuration and exits INCONCLUSIVE; no scenario runs.
  This slice adds the first one. **The capability matrix** is a pure, reviewed table: every S3
  operation the tool knows is declared `required`, or `unsupported(#NNN)` with the exact status and
  S3 error code it must answer. It is kept as immutable per-release profiles, one of which is
  selected by the server's version. **The `smoke` scenario** runs every `required` operation once
  and asserts every `unsupported` one fails exactly as declared, against a live endpoint, and
  ends PASS, FAIL or INCONCLUSIVE. An `unsupported` operation that **succeeds** is a fatal
  `unexpected-support` failure: #504's silent-overwrite class, generalised, as a standing check.
- Success criterion: Two NEW test files.
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
- Repo + branch target: getwyrd/wyrd @ main
- Scope: the matrix module (profiles, version selection, classification), the `smoke` scenario
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

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: unverifiable —                why this slice has no isolable red (the cargo output is above).
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage not measured — patch.diff does not apply on origin/main
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 379 mutants tested in 41m: 54 missed, 225 caught, 99 unviable, 1 timeouts

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 8 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_743/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.62s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of #743’s version-keyed S3 capability matrix and smoke scenario: three implementation defects remain despite passing tests and an independently demonstrated binary red→green.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief gives falsifiable operation, version, isolation and scale requirements, with explicit exclusions; the remaining slice/scale decisions are retained under Validation (brief.md:27, brief.md:118, brief.md:267). |
| C2 Reproduction (red pre-fix) | PASS | The existing CLI supplies an executable discriminator: the compiled real-binary loopback test fails against a freshly built stashed-base executable because it returns 3 instead of 0 (crates/validate/tests/smoke_loopback.rs:736; reviewer-binary-red.log:4). |
| C3 Change | PASS | The diff stays within the approved validator/client/test surface and updates the living architecture description; Wyrd dependencies remain development-only (crates/validate/Cargo.toml:24, crates/validate/Cargo.toml:55, docs/design/architecture/05-building-block-view.md:297). |
| C4 Verification (red→green) | PASS | Restoring and rebuilding the executable makes the identical loopback test pass; all 139 package tests also passed independently. Frozen test-compilation and diff-coverage limitations remain explicitly bounded below (reviewer-binary-green.log:5; reviewer-verification.log:1; gate-logs/C4-diff-cov.log:10). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Correct ownership and verdict fidelity before treating PASS as validation: fault endpoints expose unowned bucket deletion, 57 undeleted objects reported as cleaned, and malformed ETags accepted (crates/validate/src/smoke.rs:629, crates/validate/src/smoke.rs:536, crates/validate/src/matrix.rs:1100; reviewer-probes.log:5). |
| T1 Structure | PASS | Blackbox dependency direction remains intact and response handling uses the bounded client seam; the independent blackbox guard passed (crates/validate/Cargo.toml:24, crates/validate/src/s3.rs:225; reviewer-blackbox.log:3). |
| T2 Shape | PASS | Typed status/header/body observations preserve the evidence needed by the pure classifier; formatting and warning-denying package clippy passed (crates/validate/src/s3/capture.rs:28, crates/validate/src/matrix.rs:793; reviewer-clippy.log:259). |
| T3 Runtime | FAIL | An unexpected DeleteBucket success can destroy a pre-existing bucket before the validator reports FAIL; its delete is sent after the refused create has removed the bucket from the ledger (crates/validate/src/smoke.rs:610, crates/validate/src/smoke.rs:629, crates/validate/src/smoke.rs:641; reviewer-probes.log:9). |
| T4 Contribution | NEEDS-HUMAN | Confirm affected-path prior art across closed/rejected work as well as merged history — the supplied check names merged/open work only, and the disposable target has one synthetic commit and no remote, so rejected precedent is unresolved (brief.md:173; reviewer-history.log:1). |
| T5 Judgment | NEEDS-HUMAN [impl] | Add regressions that distinguish ownership from a derived name and verify failure behavior — the current name-based assertion accepts the unowned bucket probe, while the passing suite misses both false-PASS cases (crates/validate/tests/smoke_loopback.rs:633, crates/validate/src/matrix.rs:1100, crates/validate/src/smoke.rs:536; reviewer-probes.log:5). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Approve the combined slice and repeat-to-duration, single-worker smoke semantics for operator use after the defects are resolved — the brief leaves the split and scale interpretation for sign-off, and loopback success does not establish deployed-environment fitness (brief.md:267, brief.md:273; crates/validate/src/smoke.rs:88). |

Source citations without `target/` are relative to `$PDCA_TARGET`; brief, patch and evidence citations are relative to this reviewer directory. Findings were checked against the supplied target, whose applied patch passes `git apply --reverse --check`. No implementation edits remain, and both reviewer stashes were restored.

Three findings warrant builder iteration:

1. **P1 — Prove ownership before probing bucket deletion.** `scratch_bucket` depends only on the run id and cannot establish either ownership or inequality with `--bucket` (crates/validate/src/smoke.rs:80). After CreateBucket returns its expected refusal, the ledger entry is removed; DeleteBucket is nevertheless sent directly (crates/validate/src/smoke.rs:610, crates/validate/src/smoke.rs:629, crates/validate/src/smoke.rs:641). In the independent wire fixture, the derived bucket already exists and CreateBucket refuses with `400 InvalidRequest`; DeleteBucket removes that bucket and answers 204. The binary reports `unexpected-support` and exits 4 only after the destructive request. Prevent deletes of unowned buckets and cover a pre-existing derived name, including equality with the configured bucket. Evidence: reviewer-probes.log:9 and reviewer-bucket-delete.log. This contradicts the ownership guarantee documented at docs/design/architecture/05-building-block-view.md:297.

2. **P1 — Verify bulk-deleted objects are absent before forgetting them.** Matching `<Deleted>` rows immediately remove every requested key from the ledger (crates/validate/src/smoke.rs:530, crates/validate/src/smoke.rs:536). No subsequent HEAD or owned-key reconciliation checks those keys. A fixture that acknowledges every bulk delete while preserving the objects completed 19 sweeps, left 57 objects, exited 0 and printed “every key this run wrote is deleted” (reviewer-probes.log:5). The otherwise identical control completed 19 sweeps and left zero objects (reviewer-probes.log:1). Verify absence for each owned key, report an oracle failure when one remains, and retain/report its ledger entry. This is the brief’s PASS-cleanup promise, not the deferred cross-sweep oracle: brief.md:77 and crates/validate/src/smoke.rs:747.

3. **P2 — Validate an ETag’s grammar before accepting or reusing it.** `check_put` accepts every nonempty string (crates/validate/src/matrix.rs:1096); HEAD only checks equality and the same string becomes `If-None-Match` (crates/validate/src/smoke.rs:484). A fixture emitting the unquoted `bare-invalid-etag`, echoing it on HEAD and honoring it conditionally, completed 20 sweeps and exited 0 (reviewer-probes.log:13). Validate entity-tag syntax and add malformed-tag fixtures. This is grounded in the standing grammar and ETag rules at AGENTS.md:165 and AGENTS.md:173.

The independent verification establishes the new behavior while exposing the negative-case gaps. `cargo test -p wyrd-validate --locked` passed 139 tests, including 19 matrix and 11 real-gateway smoke tests; the unmodified CLI tests passed. Stashing production changes while retaining the new test sources reproduced the frozen compilation failure (reviewer-red.log:3). To obtain a behavioral red, I retained the already compiled `smoke_loopback` test executable, stashed production changes, built the pre-fix `wyrd-validate` executable, and ran only `d_the_real_binary_runs_smoke_to_a_pass`. Its real gateway ran and its exit assertion failed, 3 versus 0. After stash restoration and rebuilding the product executable, the identical test passed (reviewer-binary-red.log:4; reviewer-binary-green.log:3). This independently discharges the behavioral-red question without claiming that the frozen source-recompilation gate executed a test.

The fault probes are reproducible with `python3 reviewer-probes.py` after the patched binary is built in `reviewer-cargo`; they drive the production binary over loopback and preserve summaries, wire requests and remaining keys in `reviewer-*.log`. They demonstrate the defects on controlled faulty endpoints. The separate package tests exercise the actual Wyrd gateway and backend.

Independent checks also passed `cargo fmt --all -- --check`, package clippy with `-D warnings`, `cargo xtask blackbox-guard`, `typos`, docs lint, and docs rendering/link audit of 99 pages. Thus the brief’s named external dependencies, `typos` and `docs-renderer`, were actually exercised (brief.md:135; reviewer-docs-lint.log:1; reviewer-docs-render.log:3). A direct `cargo xtask unsafe-guard` attempt showed that it is not a public subcommand; the internal CI scanner’s frozen output explicitly reports success (reviewer-unsafe.log:1; gate-logs/C4-ci.log:24). That invocation limitation is not a source defect.

The frozen gates have the following bounded dispositions:

| Gate | Disposition | Evidence |
|------|-------------|----------|
| C4-ci | PASS, frozen evidence | Full workspace CI reports all checks passed; package tests, lint, format and docs checks were independently repeated (gate-logs/C4-ci.log:4418). |
| C4-verify | Frozen UNVERIFIABLE; independent behavioral red→green established | Its green leg passes 19 + 11 tests, but its reverted-source leg fails compilation before execution (gate-logs/C4-verify.log:10, gate-logs/C4-verify.log:250); the separate binary experiment above executes both legs. |
| C4-diff-cov | Coverage unavailable; base-state caveat | The wrapper cannot apply the stacked patch to `origin/main` and measures no coverage (gate-logs/C4-diff-cov.log:10). The brief identifies folded dependencies #744/#853 (brief.md:110); the supplied target applies and builds. This is not a patch compilation defect. |
| C5-mutants | Inspect surviving mutants; no blanket defect inference | The captured run has 54 missed, 225 caught, 99 unviable and one timeout out of 379; the ETag acceptance survivor is relevant to finding 3 (gate-logs/C5-mutants.log:30, gate-logs/C5-mutants.log:68). The 41-minute mutation campaign was not repeated. |
| T4-batch-review | FAIL, independently grounded | Its eight entries consolidate to the same three distinct findings above; no recorded rejection or tracked deferral is supplied (gate-logs/T4-batch-review.log:10). |
| T4-contribution | N/A | Contribution artifacts are intentionally absent at Check; their substantive audit reruns at publish (gate-logs/T4-contribution.log:10). No human clearance is requested for this deferral. |
| host-tikv | PASS, frozen evidence | Captured output shows successful compilation/clippy for the TiKV crate and server feature selection; this unchanged backend check was not independently repeated (gate-logs/host-tikv.log:209). |

The C5 symptom-guard smell-test does not independently fire: these capability probes are the requested external S3 checks, not guards around an eager load-time side effect. No DCO findings or already tracked scope deferrals are raised. The artifact-only prior-art investigation found one synthetic target commit and no remote; the supplied brief documents searches by both affected paths but supplies no closed/rejected-work result (reviewer-history.log:1; brief.md:173). No `INTEGRATION.md` is present in the supplied target (reviewer-history.log:3).

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — #743 (capability matrix + `smoke`)

I built the patched tree in a scratch copy and added two probe tests to a copy of
`smoke_loopback.rs`, using its own in-process gateway and relay. Both probes went red against the
production `run_with_profiles`, so findings 1 and 2 below are reproduced, not inferred. The
scratch copy has been removed.

- NEEDS-HUMAN [human] — **A build that does not know its version is judged as a known pre-0.1 build.** I ran the gateway with `server_version = "0.0.0+git.unknown"`. Smoke exited 0 (`smoke PASS`), echoing `server-version = 0.0.0+git.unknown (from Server header)` and `profile = pre-0.1`. That string is the "nothing was known" identity (`crates/server/src/version/derivation.rs:65`, `:121`). `docs/design/architecture/07-deployment-view.md:42` says every image built without `WYRD_VERSION` records it, "such as the compose fixtures'". `crates/validate/src/smoke.rs:138-145` treats only the literal `wyrd/unknown` as unusable, and `crates/validate/src/matrix.rs:551-563` drops the build metadata, so `+git.unknown` reads as release `0.0.0`. This is harmless today, because every build is pre-0.1. Once a `0.1.0` profile ships, a compose-fixture image built from 0.1.x source will be judged by the pre-0.1 table, and smoke will FAIL it with `unexpected-support`. Brief (C) says "`unknown` … selects nothing" and also says "build metadata does not change the selection". Those two rules conflict on exactly this string, so a human has to pick: fail closed to INCONCLUSIVE naming `--server-version`, or accept it as written.

- NEEDS-HUMAN [impl] — **A bulk delete that is acknowledged but not done still gives PASS, and the summary says nothing was left.** In my probe, the relay answered `POST ?delete` with a well-formed `<DeleteResult>` naming every requested key under `<Deleted>`, and never forwarded the request. Smoke exited 0 with `DeleteObjects: required; 11 passed` and `left in place: nothing; every key this run wrote is deleted`. The backend still held `adv-bulk/sweep-0/obj-1`, `obj-4096` and `obj-65536`: 3 keys per sweep, 33 over the run. The cause: `crates/validate/src/smoke.rs:534-538` takes keys out of the ledger on the response alone, `crates/validate/src/matrix.rs:1226-1269` checks only the document, and `smoke.rs:746-747` then prints the false claim. This breaks brief (G), "on PASS none is left". It is also the `oracle` class (the server disagrees with what the run did) that the single delete already checks with a HEAD afterwards (`smoke.rs:521-523`). Fix: HEAD each bulk-deleted key and require 404, or list the sweep prefix and require it empty, before removing keys from the ledger. Add a relay fixture like this one as the red. This agrees with T4's blocking findings at `smoke.rs:530/536/540` and adds the reproduction.

- NEEDS-HUMAN [impl] — **The `--scenario` refusal is untested, and it is the only guard against a false green.** C5 shows the mutant that replaces the guard at `crates/validate/src/args.rs:265` with `false` survives. `crates/validate/src/lib.rs:210` runs smoke whatever `--scenario` says, so that regression would turn `--scenario endurance` into a smoke run that can exit 0. That is a PASS status for a scenario that never ran, which is exactly what the vacuity rule exists to stop. Brief: "any other value is refused by name". Add a test: `--scenario endurance` exits 2, names `smoke`, and opens no connection. Two smaller gaps of the same kind, both from C5. First, deleting the `server-version` arm at `args.rs:268` survives, so `--server-version banana` would reach the network and end INCONCLUSIVE instead of exiting 2. Second, the `h` and `d` units at `args.rs:341-342` are never exercised: deleting `'h'`, or computing a day as `24 + 60 * 60` seconds, survives, so `--duration 7d` could run about 7 hours and still PASS. `parse_duration` is pure and public, so `2h` and `1d` can be checked directly.

- NEEDS-HUMAN [impl] — **"Never `--bucket`" is stated but not enforced.** The comment at `crates/validate/src/smoke.rs:76-79` and the new docs paragraph both say the scratch bucket is never `--bucket`, but nothing compares the two. Run with `--bucket wyrd-validate-<first 16 hex digits of sha256(run-id)>` and the CreateBucket and DeleteBucket probes (`smoke.rs:602-615`) target the bucket under test. DeleteBucket is sent with no ledger entry (`Creates` is `None`). C5 also shows that replacing `scratch_bucket` with a constant (`smoke.rs:81`) survives. A one-line refusal with exit 2 before the first request would make the claim true. This agrees with T4's `smoke.rs:82/593/598` findings.

- NEEDS-HUMAN [human] — **The conditional GET check cannot tell a matching ETag from any ETag.** The `GetObjectConditional` exchange carries no ETag (`crates/validate/src/matrix.rs:810`), `check_conditional` (`matrix.rs:1146-1166`) passes any 304 with no body, and smoke only ever sends the matching tag (`smoke.rs:483-488`). A server that answers 304 to every `If-None-Match` (the comparison ignored, say) passes this row. That is the rubric's "can pass while the property fails" class. The fix is a negative control: a non-matching tag must get 200 with the bytes. Brief (B′) asks only for the 304, so adding that control widens scope and is a human call.

- Reviewer and gate claims: `C4-verify` UNVERIFIABLE is the pre-declared net-new posture, and `C4-diff-cov`'s "does not apply on origin/main" comes from the stacked bundle (#744 and #853 in the same batch). Neither is evidence against the fix. Most of the 54 surviving mutants only change message text (`Display` impls, `same_bytes`' "differing from byte" index at `matrix.rs:1030`) or have no observable effect. The ones that matter are listed above.

- I tried to refute the following and could not. The tests drive the production `run_with_profiles` and the real binary against a real `S3Gateway`, not a copy. Any 2xx to an unsupported row, including a body error after a success head, is `unexpected-support` (`matrix.rs:916`, `:941-948`). The `Content-Range` check is strict (`matrix.rs:1054-1062`). The occupied-prefix refusal sends no write (`smoke.rs:187-195`). The `--workers` preflight happens before any connection (`smoke.rs:88-97`). The duration loop always finishes at least one sweep and never stops before the deadline (`smoke.rs:265-279`). And `cli_surface.rs` is unmodified.

<!-- pdca:leaf-complete -->

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/validate/src/smoke.rs:610`: The DeleteBucket probe bypasses ownership checks. After CreateBucket returns its declared refusal, the scratch bucket is removed from the ledger, yet DeleteBucket is sent unconditionally. An endpoint that rejects creation but unexpectedly supports deletion can therefore delete a pre-existing empty bucket with the derived name. The derivation also never excludes `--bucket`. Refuse collisions and establish that the bucket is safe to target before sending the destructive probe; add a pre-existing-bucket fixture.

- NEEDS-HUMAN [impl] — `crates/validate/src/smoke.rs:534`: Bulk-deleted keys leave the ownership ledger solely because the response contains matching `<Deleted>` rows. A server that acknowledges deletion but retains the objects passes every sweep: subsequent sweeps use different prefixes, and final cleanup sees an empty ledger. Smoke then reports PASS and “every key this run wrote is deleted” while leaving objects behind. Verify absence before removing these keys from the ledger, and test an acknowledged-but-ineffective bulk delete.

- NEEDS-HUMAN [impl] — `crates/validate/src/matrix.rs:1100`: Any nonempty ETag passes, including `*` and malformed unquoted tags. In particular, `*` is subsequently sent as `If-None-Match` (`crates/validate/src/smoke.rs:484`), turning the conditional test into an existence check. If PUT and HEAD both return `*`, smoke can PASS without validating a usable entity tag or tag comparison. Validate entity-tag grammar before accepting or reusing the value, with negative fixtures for wildcard and malformed tags.

- NEEDS-HUMAN [impl] — `crates/validate/src/s3.rs:610`: Probe errors discard the captured response head. If an unsupported operation returns a 200 head and then stalls its declared/chunked body until the operation deadline, `classify` returns `S3Error::Timeout`; `crates/validate/src/matrix.rs:917` treats it as no answer, producing INCONCLUSIVE instead of the required fatal `unexpected-support`. Preserve a received success status across body timeouts, and exercise this path with a short-deadline loopback fixture.

No additional material reuse, simplification, or efficiency findings. Reviewed the target source and frozen gate evidence; no builds were rerun.
<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Correct ownership and verdict fidelity before treating PASS as validation: fault endpoints expose unowned bucket deletion, 57 undeleted objects reported as cleaned, and malformed ETags accepted (crates/validate/src/smoke.rs:629, crates/validate/src/smoke.rs:536, crates/validate/src/matrix.rs:1100; reviewer-probes.log:5).
- [ ] T4 Contribution — Confirm affected-path prior art across closed/rejected work as well as merged history — the supplied check names merged/open work only, and the disposable target has one synthetic commit and no remote, so rejected precedent is unresolved (brief.md:173; reviewer-history.log:1).
- [ ] T5 Judgment — Add regressions that distinguish ownership from a derived name and verify failure behavior — the current name-based assertion accepts the unowned bucket probe, while the passing suite misses both false-PASS cases (crates/validate/tests/smoke_loopback.rs:633, crates/validate/src/matrix.rs:1100, crates/validate/src/smoke.rs:536; reviewer-probes.log:5).
- [ ] Validation — fitness-to-purpose — Approve the combined slice and repeat-to-duration, single-worker smoke semantics for operator use after the defects are resolved — the brief leaves the split and scale interpretation for sign-off, and loopback success does not establish deployed-environment fitness (brief.md:267, brief.md:273; crates/validate/src/smoke.rs:88).
- [ ] **A build that does not know its version is judged as a known pre-0.1 build.** I ran the gateway with `server_version = "0.0.0+git.unknown"`. Smoke exited 0 (`smoke PASS`), echoing `server-version = 0.0.0+git.unknown (from Server header)` and `profile = pre-0.1`. That string is the "nothing was known" identity (`crates/server/src/version/derivation.rs:65`, `:121`). `docs/design/architecture/07-deployment-view.md:42` says every image built without `WYRD_VERSION` records it, "such as the compose fixtures'". `crates/validate/src/smoke.rs:138-145` treats only the literal `wyrd/unknown` as unusable, and `crates/validate/src/matrix.rs:551-563` drops the build metadata, so `+git.unknown` reads as release `0.0.0`. This is harmless today, because every build is pre-0.1. Once a `0.1.0` profile ships, a compose-fixture image built from 0.1.x source will be judged by the pre-0.1 table, and smoke will FAIL it with `unexpected-support`. Brief (C) says "`unknown` … selects nothing" and also says "build metadata does not change the selection". Those two rules conflict on exactly this string, so a human has to pick: fail closed to INCONCLUSIVE naming `--server-version`, or accept it as written.
- [ ] **A bulk delete that is acknowledged but not done still gives PASS, and the summary says nothing was left.** In my probe, the relay answered `POST ?delete` with a well-formed `<DeleteResult>` naming every requested key under `<Deleted>`, and never forwarded the request. Smoke exited 0 with `DeleteObjects: required; 11 passed` and `left in place: nothing; every key this run wrote is deleted`. The backend still held `adv-bulk/sweep-0/obj-1`, `obj-4096` and `obj-65536`: 3 keys per sweep, 33 over the run. The cause: `crates/validate/src/smoke.rs:534-538` takes keys out of the ledger on the response alone, `crates/validate/src/matrix.rs:1226-1269` checks only the document, and `smoke.rs:746-747` then prints the false claim. This breaks brief (G), "on PASS none is left". It is also the `oracle` class (the server disagrees with what the run did) that the single delete already checks with a HEAD afterwards (`smoke.rs:521-523`). Fix: HEAD each bulk-deleted key and require 404, or list the sweep prefix and require it empty, before removing keys from the ledger. Add a relay fixture like this one as the red. This agrees with T4's blocking findings at `smoke.rs:530/536/540` and adds the reproduction.
- [ ] **The `--scenario` refusal is untested, and it is the only guard against a false green.** C5 shows the mutant that replaces the guard at `crates/validate/src/args.rs:265` with `false` survives. `crates/validate/src/lib.rs:210` runs smoke whatever `--scenario` says, so that regression would turn `--scenario endurance` into a smoke run that can exit 0. That is a PASS status for a scenario that never ran, which is exactly what the vacuity rule exists to stop. Brief: "any other value is refused by name". Add a test: `--scenario endurance` exits 2, names `smoke`, and opens no connection. Two smaller gaps of the same kind, both from C5. First, deleting the `server-version` arm at `args.rs:268` survives, so `--server-version banana` would reach the network and end INCONCLUSIVE instead of exiting 2. Second, the `h` and `d` units at `args.rs:341-342` are never exercised: deleting `'h'`, or computing a day as `24 + 60 * 60` seconds, survives, so `--duration 7d` could run about 7 hours and still PASS. `parse_duration` is pure and public, so `2h` and `1d` can be checked directly.
- [ ] **"Never `--bucket`" is stated but not enforced.** The comment at `crates/validate/src/smoke.rs:76-79` and the new docs paragraph both say the scratch bucket is never `--bucket`, but nothing compares the two. Run with `--bucket wyrd-validate-<first 16 hex digits of sha256(run-id)>` and the CreateBucket and DeleteBucket probes (`smoke.rs:602-615`) target the bucket under test. DeleteBucket is sent with no ledger entry (`Creates` is `None`). C5 also shows that replacing `scratch_bucket` with a constant (`smoke.rs:81`) survives. A one-line refusal with exit 2 before the first request would make the claim true. This agrees with T4's `smoke.rs:82/593/598` findings.
- [ ] **The conditional GET check cannot tell a matching ETag from any ETag.** The `GetObjectConditional` exchange carries no ETag (`crates/validate/src/matrix.rs:810`), `check_conditional` (`matrix.rs:1146-1166`) passes any 304 with no body, and smoke only ever sends the matching tag (`smoke.rs:483-488`). A server that answers 304 to every `If-None-Match` (the comparison ignored, say) passes this row. That is the rubric's "can pass while the property fails" class. The fix is a negative control: a non-matching tag must get 200 with the bytes. Brief (B′) asks only for the 304, so adding that control widens scope and is a human call.
- [ ] `crates/validate/src/smoke.rs:610`: The DeleteBucket probe bypasses ownership checks. After CreateBucket returns its declared refusal, the scratch bucket is removed from the ledger, yet DeleteBucket is sent unconditionally. An endpoint that rejects creation but unexpectedly supports deletion can therefore delete a pre-existing empty bucket with the derived name. The derivation also never excludes `--bucket`. Refuse collisions and establish that the bucket is safe to target before sending the destructive probe; add a pre-existing-bucket fixture.
- [ ] `crates/validate/src/smoke.rs:534`: Bulk-deleted keys leave the ownership ledger solely because the response contains matching `<Deleted>` rows. A server that acknowledges deletion but retains the objects passes every sweep: subsequent sweeps use different prefixes, and final cleanup sees an empty ledger. Smoke then reports PASS and “every key this run wrote is deleted” while leaving objects behind. Verify absence before removing these keys from the ledger, and test an acknowledged-but-ineffective bulk delete.
- [ ] `crates/validate/src/matrix.rs:1100`: Any nonempty ETag passes, including `*` and malformed unquoted tags. In particular, `*` is subsequently sent as `If-None-Match` (`crates/validate/src/smoke.rs:484`), turning the conditional test into an existence check. If PUT and HEAD both return `*`, smoke can PASS without validating a usable entity tag or tag comparison. Validate entity-tag grammar before accepting or reusing the value, with negative fixtures for wildcard and malformed tags.
- [ ] `crates/validate/src/s3.rs:610`: Probe errors discard the captured response head. If an unsupported operation returns a 200 head and then stalls its declared/chunked body until the operation deadline, `classify` returns `S3Error::Timeout`; `crates/validate/src/matrix.rs:917` treats it as no answer, producing INCONCLUSIVE instead of the required fatal `unexpected-support`. Preserve a received success status across body timeouts, and exercise this path with a short-deadline loopback fixture.
- [ ] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 8 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_743/review-b
- [ ] **The claimed absence of a CopyObject regression check is false on the target.** `brief.md:149-152` says “nothing checks that it stays rejected,” repeating the issue body's stale premise. At `$PDCA_TARGET`, `crates/server/tests/s3_copy_object_guard.rs:175` already runs `copy_source_put_is_refused_and_destination_survives`: lines 199-207 assert `501 NotImplemented`, and lines 210-222 verify the destination's original bytes survive. Reframe the missing capability as operator-run, version-aware validation of deployed endpoints, and acknowledge the existing regression coverage; absence of a rejection test is not the problem this slice fixes.
- [ ] **The required-operation half has no falsifiable correctness criterion.** `brief.md:49-52` requires only that each required operation “ran once”; the classification tests at `:37-40` exercise unsupported rows. The distinction matters: `crates/gateway-s3/src/lib.rs:2293-2300` explicitly returns HTTP 200 even when bulk deletion contains per-key errors; a correct ranged GET returns 206 with `Content-Range` (`:2713-2719`), and the planned matching `If-None-Match` request returns 304 (`:2785-2803`). A runner accepting any SDK success could meet the stated happy-path tests while missing a partial delete failure or an ignored Range. Specify the required-operation status/result checks and a negative fixture that proves they affect the run verdict. This can remain smaller than the explicitly deferred #746 oracle.
- [ ] **One-shot PASS silently changes the duration contract.** `brief.md:170-176` finishes after one sweep, while `:182-184` retains all ten required flags without defining what smoke does with `--duration` or `--workers`. The target parser leaves these as untyped strings for consuming slices (`crates/validate/src/args.rs:32-45`); it explicitly ties duration to the proposal's verdict rule (`:15-18`). The plan of record says scale applies to every scenario and requires wall-clock within tolerance of the requested duration before PASS (`docs/design/proposals/draft/0017-blackbox-validation-tool.md:528-531`, `:555-561`). As written, `--scenario smoke --duration 7d` can report PASS after a short sweep. Resolve this scope change explicitly: enforce a defined floor, or define/refuse unsupported scale arguments for one-shot smoke, with an observable acceptance case.
- [ ] **Criterion G does not establish ownership before the newly live write/delete path.** `brief.md:61-62` checks prefix membership and cleanup, and `:165-175` proceeds from version discovery to PUT without an occupied-prefix check. The cited plan of record explicitly requires refusing an occupied `<run-id>/` before writing (`docs/design/proposals/draft/0017-blackbox-validation-tool.md:791-794`) and explains why prefix shape does not prove ownership (`:811-816`); it also identifies overwriting a pre-existing generated key and then deleting it as the concrete failure (`:822-825`). Reusing a run id containing retained failure evidence can satisfy G while destroying that evidence. Add the occupied-prefix refusal and deletion-ownership rule to the brief, and require a loopback case with a pre-existing object under that prefix that observes no mutations and unchanged bytes.
- [ ] size backstop — this slice is behaving oversized: patch is 250 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Plan
- Iteration delta (if iterating): Oversized (250 KB, ~3,100 lines of production code) and never split. Split along brief.md:267-271: (1) the capability matrix, version selection and --server-version: pure, no network, no wait on #853; (2) smoke, the client verbs and the binary wiring, depending on (1), #744 and #853. The three reproduced defects all belong to (2) and must be required reds there: - the DeleteBucket probe can delete a bucket the run does not own; refuse a scratch name equal to --bucket, and never send a destructive probe without ownership (pre-existing-bucket fixture); - a bulk delete acknowledged but not done still gives PASS and a false "nothing left" summary; verify absence before dropping keys from the ledger (acknowledging-relay fixture); - any non-empty ETag (including * and unquoted tags) is accepted and reused as If-None-Match; validate entity-tag grammar (wildcard and malformed fixtures). Also for (2): keep a 2xx head across a body timeout so it is unexpected-support, not INCONCLUSIVE; test the --scenario refusal (endurance -> exit 2, no connection), the --server-version parse refusal, and the h/d duration units. Open for Plan to settle with the human (sign-off leaned, not decided): a 0.0.0+git.unknown server version should end INCONCLUSIVE naming --server-version (the brief's "unknown selects nothing" rule); conditional GET should get a negative control (a non-matching tag must answer 200 with the bytes). Reframe the CopyObject premise: crates/server/tests/s3_copy_object_guard.rs already checks the 501; this slice is about operator-run, version-aware validation of deployed endpoints.
- By / date: Eduard Ralph / 2026-10-19

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 4 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
