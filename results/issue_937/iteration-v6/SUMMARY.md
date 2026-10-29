# Result — issue 937 / validate-capability-matrix

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: `wyrd-validate` has no capability matrix. Nothing declares, per S3 operation,
  whether a Wyrd release must support it or must refuse it with an exact status and code;
  nothing picks those expectations by the server's version; and nothing turns an observed
  answer into a pass or one of 0017 §7's failure classes. Smoke (743.3) and every later
  scenario need all three, and none of it needs the network.
- Success criterion: the NEW file `crates/validate/tests/capability_matrix.rs` passes,
  calling a pure `matrix` module (no I/O, no network, no SDK or client error types), and shows:
  - **(A) Today's profile.** The `pre-0.1` profile covers every version below `0.1.0` and
    declares exactly these rows, each citing the gateway line that gives today's answer:
    `required`: `PutObject`; `GetObject` whole, ranged (`Range: bytes=a-b`), conditional with
    a matching `If-None-Match`, and conditional with a non-matching one; `HeadObject`;
    `DeleteObject`; `DeleteObjects`; `ListObjectsV2`; `ListObjects` (v1).
    `unsupported`: `CopyObject` → `501 NotImplemented` (#766); `CreateMultipartUpload`,
    `UploadPart`, `CompleteMultipartUpload`, `AbortMultipartUpload`, `ListParts`,
    `ListMultipartUploads` → `501 NotImplemented` (#508); `GetBucketLocation` → `501
    NotImplemented` (#511); `CreateBucket`, `DeleteBucket`, `ListBuckets` → `400
    InvalidRequest` (#511); `HeadBucket` → `400` with no body (#511). No row is in the third
    state (D) today. **Unless #508 is on your base** (the gateway no longer refuses `uploads`,
    `uploadId`, `partNumber`: check the denylist at `crates/gateway-s3/src/lib.rs:410-422`): then
    the six multipart rows are `supported, not checked (#750)` instead, by the rule under *Design*.
  - **(B) Unsupported rows.** For an observed answer the classifier returns: pass for the
    declared status and code exactly; `protocol` (fatal) for any other status or code, a 5xx
    included; **`unexpected-support` (fatal) for any 2xx**, including a 2xx response head
    whose body then failed or timed out; and no verdict (cannot conclude) when no response
    head arrived at all.
  - **(B′) Required rows have checks of their own** (0017 §7 classes). Each passes only when:
    `PutObject` 200 with an `ETag` that is a strong entity-tag (RFC 9110 §8.8.3: `"`, etagc*,
    `"`), so `*`, an unquoted value, a weak `W/"…"` tag and an empty value are each `protocol`,
    **and** the request body had ended when the response head arrived. A success that arrives
    before that is never a receipt (the client's rule, `PutObject`'s doc at
    `crates/validate/src/s3.rs:161-172` on `main`; `BodyError::AcknowledgedEarly`,
    `crates/validate/src/s3/error.rs:97-108`), so it is `protocol` whatever its `ETag`
    *(revised 2026-10-22; Plan settled the class so Do does not invent one: a 2xx to a request
    the server had not finished receiving is a wrong status; the human may change it at
    sign-off)*;
    whole `GetObject` 200 whose body equals the expected bytes, else `integrity`; ranged
    `GetObject` **206** with `Content-Range: bytes a-b/<size>` and exactly those bytes;
    conditional `GetObject` with the matching tag **304** and no body; conditional `GetObject`
    with a valid strong tag that differs from the object's **200** with the whole body (the
    negative control that tells "compares tags" from "answers 304 to anything", **(decided)**);
    `HeadObject` 200 with the object's size and the PUT's `ETag`, else `oracle`;
    `ListObjectsV2` and v1 200 naming exactly the keys written under the listed prefix, with
    their sizes, else `oracle`; `DeleteObject` 204 and then a `HEAD` of the key answers 404;
    `DeleteObjects` 200 with every requested key under `<Deleted>` and no `<Error>` row (the
    gateway answers 200 even when keys failed, `crates/gateway-s3/src/lib.rs:2293-2300`),
    **and then** a `HEAD` of each of those keys answers 404: the document alone never proves a
    delete. **The follow-up `HEAD` is a probe, and a probe that cannot settle is not a
    disagreement** (revised 2026-10-22; 0017 §7, `0017:520-524`: where the probe cannot settle
    an outcome the run goes INCONCLUSIVE). For each follow-up `HEAD`: 404 → that key is proved
    deleted; a 2xx → `oracle` (a deleted key is readable, `0017:490`); **no response head (no
    answer, a timeout) or a 5xx → no verdict for the row**: the delete is unresolved, never
    `oracle`; any other status → `protocol`. A row with an unresolved follow-up and no fatal
    one is folded by (E) as not proved.
    Negative fixtures, each giving the named class and making the run FAIL: a 200 with the
    whole body to the ranged GET; a 206 with the wrong `Content-Range`; a 200 to the matching
    conditional GET; a 304 to the non-matching one; a `DeleteObjects` 200 with one `<Error>`
    row; a `DeleteObjects` 200 naming every key under `<Deleted>` followed by a `HEAD` that
    answers 200 (`oracle`); a `DeleteObject` 204 followed by a `HEAD` that answers 200
    (`oracle`); a GET body one byte off (`integrity`); a PUT `ETag` of `*`; an unquoted one; a
    PUT 200 with a valid strong `ETag` whose head arrived before the request body ended
    (`protocol`).
    Fixtures that must give **INCONCLUSIVE, not FAIL** (every other row passing): a
    `DeleteObject` 204 whose follow-up `HEAD` has no answer (timed out); the same with a `HEAD`
    that answers 503; a `DeleteObjects` 200 naming every key under `<Deleted>` where one
    key's `HEAD` has no answer and the rest answer 404. The verdict names the delete row.
  - **(C) Version keying.** Parsing a reported version gives a release (`MAJOR.MINOR.PATCH`
    with any pre-release; `+` build metadata ignored for selection), **unknown**, or a parse
    error. **Unknown** covers `unknown` (the gateway's default, `wyrd/unknown`,
    `crates/gateway-s3/src/lib.rs:121`) and any version whose build metadata is the build's
    "nothing was known" identity, `+git.unknown` or `+git.unknown.dirty`
    (`crates/server/src/version/derivation.rs:65`, `:121`); it is never read as release `0.0.0`
    **(decided)**. With a test-local table of two profiles covering different ranges, a version
    in each range selects its own profile and its own expectations (a row `unsupported` in one
    and `required` in the other flips the verdict for the same answer). `0.0.0+git.abc1234`
    selects `pre-0.1`. A version that parses but matches no profile selects none; unknown and
    unparseable select none.
  - **(D) The third row state, `supported, not checked (#NNN)` (decided):** an operation the
    server is expected to accept and the tool cannot check yet; `#NNN` is the issue that will
    add the check. With a test-local profile holding one such row: a run can be PASS without
    that row exercised; the verdict lists it as not covered, with its issue; an answer fed for
    it is never counted as a pass.
  - **(E) The run verdict.** Folding the row results: any fatal result ⇒ FAIL, whatever else
    passed, naming every fatal row and its class; else any `required` or `unsupported` row not
    exercised, exercised only with no answer, or whose check could not be settled (a follow-up
    probe with no answer, (B′)) ⇒ INCONCLUSIVE, naming those rows (0017's vacuity rule and
    §7's unresolved-outcome rule); else PASS.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: the `matrix` module: operations, rows in three states, immutable profiles with
  `pre-0.1`, version parsing and selection, the per-row classification in (A)–(B′), the run
  verdict (E); `pub mod matrix;` in `crates/validate/src/lib.rs` (that one line only); the new
  test file; and the 0017 §3 amendment under *Design*. / out of scope:
  - sending anything: the client verbs are 743.2; the runner, `--server-version`, reading the
    `Server` header, the occupied-prefix and cleanup rules are 743.3;
  - mapping the client's results to the matrix's observation type (743.3);
  - the rest of `lib.rs` (its module doc's "#743" sentence is 743.3's), `Cargo.toml`,
    `docs/design/architecture/05-building-block-view.md` (743.3 documents the feature);
  - the cross-sweep oracle, event stream and verdict artifact (#746, #747, #764); MinIO's
    reference profile (#751); authentication vectors (#745);
  - any gateway change; any `wyrd-*` crate in `[dependencies]`; `deny*.toml`.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): fail —   - cargo deny --all-features --config deny-all-features.toml check advisories: exited with exit status: 1
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: unverifiable —                why this slice has no isolable red (the cargo output is above).
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 88.6% — 601 of 678 instrumentable changed lines executed (floor 80%); 678 of 1480 changed lines were instr
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 158 mutants tested in 7m: 128 caught, 30 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_937/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.59s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #937: the pure, version-keyed S3 capability matrix has no confirmed implementation defect; verification exceptions and declared policy choices remain for human sign-off.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance cases make false support, false receipts, unresolved deletes and incorrect version selection falsifiable; the gateway still refuses multipart on this base (`brief.md:18`, `target/crates/gateway-s3/src/lib.rs:410`). |
| C2 Reproduction (red pre-fix) | NEEDS-HUMAN | Accept the declared new-API exception or require another discriminator — production stashed with the submitted test retained fails to compile, so no behavioral pre-fix red executes; all ten required fault injections are caught (`brief.md:138`, `reviewer-red.log:260`, `reviewer-mutations.log:2`, `reviewer-mutation-retries.log:2`). |
| C3 Change | PASS | The four-file change stays within the matrix, its export, tests and the amended draft; both version-header passages now agree with the gateway (`target/crates/validate/src/lib.rs:24`, `target/docs/design/proposals/draft/0017-blackbox-validation-tool.md:263`, `target/docs/design/proposals/draft/0017-blackbox-validation-tool.md:873`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Require full-CI evidence on the intended refreshed base — 53 restored tests pass, but both advisory scans still fail on unchanged ctutils/tokio versions despite the tracked baseline fixes; this is a baseline/evidence caveat, not a matrix defect (`reviewer-final-green.log:60`, `target/Cargo.lock:1000`, `target/Cargo.lock:3953`, `reviewer-deny.log:314`, `reviewer-deny-all-features.log:358`, `brief.md:235`). |
| C5 Causal adequacy | PASS | The checks preserve fatal evidence and withhold passes for incomplete answers; the earlier torn-body and bulk-probe defects are covered, and no load-time capability workaround was introduced (`target/crates/validate/src/matrix.rs:999`, `target/crates/validate/src/matrix.rs:1225`, `target/crates/validate/tests/capability_matrix.rs:788`, `target/crates/validate/tests/capability_matrix.rs:873`). |
| T1 Structure | PASS | The classifier uses its own observations and the explicitly permitted ErrorCode value, with no transport, clock or new dependency seam (`target/crates/validate/src/matrix.rs:30`, `target/crates/validate/src/matrix.rs:631`, `brief.md:120`). |
| T2 Shape | PASS | The patch is confined to four planned files, with operation-specific checks and shared parsing/folding helpers; formatting and whitespace checks pass (`patch.diff:1`, `target/crates/validate/src/matrix.rs:1013`, `reviewer-scanners-restored.log:10`). |
| T3 Runtime | PASS | The real Cargo target executes all 53 pure tests successfully; this slice adds no running service, external await or concurrent/destructive path (`reviewer-final-green.log:60`, `target/crates/validate/tests/capability_matrix.rs:1`, `brief.md:151`). |
| T4 Contribution | N/A | Contribution artifacts are absent by design and their substantive audit must rerun at publish; affected-path prior art was independently checked and the frozen batch-review summary reports no blocking findings (`gate-logs/T4-contribution.log:10`, `reviewer-prior-art-summary.log:1`, `gate-logs/T4-batch-review.log:10`). |
| T5 Judgment | NEEDS-HUMAN | Decide whether to retain strict retry accounting, inconclusive empty listings, and the prerelease retention boundary — these declared open choices change what a PASS means across attempts and releases (`brief.md:238`, `target/crates/validate/tests/capability_matrix.rs:1282`, `target/crates/validate/tests/capability_matrix.rs:907`, `target/crates/validate/tests/capability_matrix.rs:334`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept this pure component as the intended foundation for #939 — the classifications are exercised, but deployment fitness and the future client-to-observation mapping are outside this slice's evidence (`brief.md:129`, `brief.md:151`, `target/crates/validate/src/matrix.rs:3`). |

The implementation evidence is reproducible. I stashed production while retaining the submitted integration test, observed the missing `wyrd_validate::matrix` import, restored production, and ran the 53-test suite successfully. Each of the ten fault injections required by the brief then failed an executing test. Two initial injections encountered warnings-as-errors before execution; preserving the unused references made both compile and fail their assertions (`reviewer-mutations.log:1`, `reviewer-mutation-retries.log:1`). The final suite passes after restoration, and `git apply --reverse --check patch.diff` confirms the supplied patch is intact. No production fix was made during review.

The frozen gates support specific, limited conclusions. Coverage reports 601/678 instrumentable changed lines executed (88.6%, above the 80% floor), with 53 tests run (`gate-logs/C4-diff-cov.log:176`). Mutation testing reports 128 caught and 30 unviable out of 158, with no viable survivor (`gate-logs/C5-mutants.log:13`). The TiKV feature compilation finished successfully (`gate-logs/host-tikv.log:7`, `gate-logs/host-tikv.log:209`); the batch-review log supplies only its zero-blocking summary (`gate-logs/T4-batch-review.log:10`). These are frozen-log judgments, not claims that I reran those instance-scoped wrappers. The deferred contribution row is N/A, with its mandatory publish audit still owed.

Full CI remains failed on the supplied baseline. Its final summary names only the two cargo-deny checks (`gate-logs/C4-ci.log:9366`). I reran both advisory configurations using the real scanner, changing only the advisory-cache location to stay inside this sandbox: both reproduce RUSTSEC-2026-0385 for ctutils 0.4.2 and RUSTSEC-2026-0386 for tokio 1.52.3 (`reviewer-deny.log:3`, `reviewer-deny.log:85`, `reviewer-deny-all-features.log:3`, `reviewer-deny-all-features.log:85`). Those versions are in the unchanged target lockfile. The brief already names #950/#954 as their resolution; obtain evidence on that refreshed base rather than reopening those dependency fixes in this matrix patch. There is no target application or compilation failure to attribute to the patch.

The named external tools were exercised. Independent typos, documentation lint, documentation rendering/link audit and cargo fmt checks all exit zero; the renderer built 99 pages (`reviewer-scanners-restored.log:1`). The prior-art audit queried default-branch history by all four affected file paths and inspected the changed-file lists of all 19 closed-unmerged PRs, finding no affected-path match and no truncated file list (`reviewer-prior-art-summary.log:1`). The rejected earlier approach is recorded in the brief, and the current regressions cover its carried-forward implementation findings (`brief.md:173`, `target/crates/validate/tests/capability_matrix.rs:539`, `target/crates/validate/tests/capability_matrix.rs:678`, `target/crates/validate/tests/capability_matrix.rs:722`).

The T5 decisions are already identified as human choices in the brief; their observed consequences are:

- Retain or relax retry strictness: a passing PUT plus a no-answer PUT makes the run INCONCLUSIVE (`target/crates/validate/tests/capability_matrix.rs:1282`).
- Retain or relax empty-listing treatment: an empty expected prefix and empty listing make the row INCONCLUSIVE (`target/crates/validate/tests/capability_matrix.rs:907`).
- Define retention at a prerelease tag: `0.1.0-rc.1` currently selects the editable `pre-0.1` profile, while the amendment starts retention at the first tag (`target/crates/validate/tests/capability_matrix.rs:334`, `target/docs/design/proposals/draft/0017-blackbox-validation-tool.md:284`).

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — #937 (capability matrix), round 6

**How I checked.** I built a small scratch crate that compiles the target's own
`crates/validate/src/matrix.rs` through `#[path]` (the production file, not a copy), with a
stub `ErrorCode` that matches `crates/validate/src/s3/error.rs:69-76`. The shipped
`capability_matrix.rs` passes 53/53 against it, which matches `gate-logs/C4-verify.log` and
`gate-logs/C4-diff-cov.log`. Then I ran probe inputs and two hand-made mutants against the
same file.

**C4-verify is UNVERIFIABLE (exit 77).** The brief declares this up front (`brief.md:138-141`):
the module is new, so the test cannot compile with production reverted. That is not a
refutation.

## Findings

- NEEDS-HUMAN [human] — **The gating C4-ci red comes from the base, not from this patch, for
  the third round running.** `cargo deny` fails on ctutils 0.4.2 (RUSTSEC-2026-0385) and tokio
  1.52.3 (RUSTSEC-2026-0386) (`gate-logs/C4-ci.log:3873`, `:3955`, `:9367-9368`). The target
  pins exactly those versions (`Cargo.lock:1000-1001`, `Cargo.lock:3953-3954`), and its base
  commit is `93a6f37`. The carry-forward at `brief.md:235` said to rebuild on `b552ca2`, which
  carries #950 and #954. The patch touches no dependency. Someone has to refresh the base, or
  waive the red at sign-off. Another Do round will not fix it.

- NEEDS-HUMAN [impl] — **Two guards can be deleted with the suite still 53/53 green.** I ran
  both mutants. (a) `crates/validate/src/matrix.rs:908-914`, the check that the exercise matches
  the row. With it deleted, `classify(PRE_0_1.row(PutObject), &Exercise::GetObject { object,
  200 + the right bytes })` returns `Pass`, so a GET answer passes the PUT row. `verdict` cannot
  reach this, because it looks the row up from the exercise. But `classify` is `pub`, and 743.3
  may call it directly. Coverage agrees: lines 909 and 911 never ran
  (`gate-logs/C4-diff-cov.log`, the MISS list). (b) `matrix.rs:990-992`, which makes "a document
  where object bytes were expected" a `protocol` failure. Changed to `Ok(&[])`, a GET of an empty
  object answered `200` with a `Body::Listing` passes. For a non-empty object the failure class
  quietly changes from `protocol` to `integrity`. Line 991 is also on the MISS list. Fix: add one
  fixture for each, beside `capability_matrix.rs:538-546` and `:677-697`.

- NEEDS-HUMAN [human] — **The two delete rows treat a retried HEAD differently, and the
  module's own doc contradicts one of them.** In one bulk exercise, follow-ups
  `[(a, 503), (a, 404)]` give `Pass` (`matrix.rs:1248-1251`, pinned at
  `capability_matrix.rs:856-862`). `DeleteObject` carries exactly one `follow_up: Answer`
  (`matrix.rs:768-772`), so the same retry has to be sent as a second exercise. The
  "every exercise must pass" rule (`matrix.rs:1393-1396`) then makes the run INCONCLUSIVE. I
  probed it: "not proved: DeleteObject: … answered 503". So the same evidence (one 503, then a
  404 for the same key) proves the bulk row and not the single row. The bulk result also breaks
  the module invariant at `matrix.rs:26-28` ("a row with such a check and no fatal one is
  unproved"), and a strict reading of `brief.md:60-63` ("a 5xx → no verdict for the row"). This
  is open design point (1) in a new place (`brief.md:235`). Pick one retry rule, apply it to
  both rows, and make the rustdoc match.

- NEEDS-HUMAN [impl] — **A vacuous PASS is still possible at the profile level** (low
  priority: `PRE_0_1` cannot hit it today). `verdict` returns PASS whenever nothing is fatal and
  nothing is unproved (`matrix.rs:1436-1442`). A profile with no `required` or `unsupported`
  rows therefore passes an empty run. I probed it two ways: a profile holding only
  `not_checked(UploadPart)`, and a profile with no rows. Both give `verdict(&p, &[])` = PASS.
  Separately, `Profile::new` (`matrix.rs:250-252`) accepts a profile that declares the same
  operation twice, and `Profile::row` (`:267-269`) silently uses the first row. A profile
  declaring `PutObject` both `required` and `unsupported` passes a PUT 200. The no-duplicates
  check exists only for `PRE_0_1` (`capability_matrix.rs:314`). This is the rubric's
  "count-based assertion that can pass while the property fails" class. Fix: either make "zero
  proved rows" INCONCLUSIVE, or add a test over `PROFILES` requiring every profile to have at
  least one checked row and no duplicate operations.

- NEEDS-HUMAN [human] — **The brief's 5xx rule for unsupported rows conflicts with 0017 §7**
  (low priority; the code follows the brief). A `503 SlowDown` to `CopyObject` is fatal
  `protocol` (`matrix.rs:936-941`, pinned at `capability_matrix.rs:380`). The same 503 to
  `PutObject` only leaves the row unsettled (`matrix.rs:965-967`). 0017's taxonomy table budgets
  every 5xx as `availability` (`0017:508`), while §3 says an unsupported row must not answer "a
  500" (`0017:194`). The brief settled it as "a 5xx included" (`brief.md:33-34`). The cost: one
  transient 503 on any of the twelve unsupported rows turns a smoke run FAIL instead of
  INCONCLUSIVE. Confirm that this is intended.

- Note, no action needed: a GET or HEAD of a key the run just wrote that answers 404 is classed
  `protocol` (`matrix.rs:1081`, `:1124`). `brief.md:52` says HEAD "else `oracle`". Both classes
  are fatal and the run outcome is the same; only the label differs.

- Note, no action needed: `Content-Length` is not checked against the body on GET or 206 (a
  probe with `Content-Length: 16` and 4 correct range bytes passes, `matrix.rs:1079-1111`). On a
  real connection, hyper reports a short or long body as a transport error, so 743.3's mapping
  should already produce `Body::Failed`. This only matters if that mapping passes through a
  header the transport did not enforce.

## What I tried to refute and could not

- **The 10 required reds** (`brief.md:142-150`). The C5 gate shows 0 of 128 viable mutants
  missed (`gate-logs/C5-mutants.log`). Every named leg has a pinning test.
- **Unsupported rows.** Any 2xx is `unexpected-support`, including a 2xx whose body then
  failed. No response head gives no verdict. A declared refusal whose body was cut off is
  unsettled, not fatal.
- **PUT.** A strong `ETag` is required (`*`, unquoted, `W/` and empty are all refused). An
  early 200 is `protocol` whatever its `ETag`. A torn 200 is unsettled.
- **Ranged GET.** The `Content-Range` grammar refuses signs, a missing unit, `*/size` and
  other units. The unit match is case-insensitive.
- **Bulk delete.** Each follow-up is paired with its own key. A torn `<DeleteResult>` still
  lets a readable key's probe fail the row. Keys requested twice and unrequested probe keys are
  both handled.
- **Version parsing.** Leading zeros, `+` signs, empty identifiers, overflow and `_` are all
  refused. `+git.unknown[.dirty]` reads as Unknown. `+GIT.UNKNOWN` reads as release 0.0.0, but
  the version derivation only ever emits lowercase (`crates/server/src/version/derivation.rs:65`),
  so that is not a defect.
- **Every `PRE_0_1` citation** matches the gateway source at the cited line
  (`crates/gateway-s3/src/lib.rs:1766`, `:1797-1815`, `:1819-1826`, `:1842-1849`,
  `:1865-1871`, `:2008`, `:2017`, `:2025-2028`, `:2713-2719`, `:2785`, `:2995`, `:3401`).
- **#508 is not on the base.** `lib.rs:410-413` still lists `uploads`, `uploadId` and
  `partNumber`, so the six multipart rows are correctly `unsupported`.
- **The 0017 amendment's citations** check out (`lib.rs:1693-1699`, `crates/server/build.rs:117`,
  `xtask/src/dist.rs:546`). The old `*Dependencies*` contradiction is gone (`0017:873-880`).
<!-- pdca:leaf-complete -->

### Advisory — code-review

- No new correctness findings in this diff. Reviewed version selection (`crates/validate/src/matrix.rs:505`), unsupported and required classification (`crates/validate/src/matrix.rs:924`, `crates/validate/src/matrix.rs:1054`), bulk-delete evidence folding (`crates/validate/src/matrix.rs:1213`), and run verdicts (`crates/validate/src/matrix.rs:1397`), including their regression coverage in `crates/validate/tests/capability_matrix.rs:368`.
- No actionable reuse, simplification, or efficiency findings. The shared status, body, probe, and result-folding helpers cover the repeated checks (`crates/validate/src/matrix.rs:962`, `crates/validate/src/matrix.rs:999`, `crates/validate/src/matrix.rs:1013`, `crates/validate/src/matrix.rs:1187`).

Validation evidence: the frozen logs report 53 passing matrix tests, 88.6% diff coverage, and 128 caught / 30 unviable mutants with none surviving. Full CI remains failed on dependency advisories outside this diff; this advisory does not override that gate or the design questions carried forward in the brief. Target source was read only; no builds were rerun.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C2 Reproduction (red pre-fix) — Accept the declared new-API exception or require another discriminator — production stashed with the submitted test retained fails to compile, so no behavioral pre-fix red executes; all ten required fault injections are caught (`brief.md:138`, `reviewer-red.log:260`, `reviewer-mutations.log:2`, `reviewer-mutation-retries.log:2`).
- [ ] C4 Verification (red→green) — Require full-CI evidence on the intended refreshed base — 53 restored tests pass, but both advisory scans still fail on unchanged ctutils/tokio versions despite the tracked baseline fixes; this is a baseline/evidence caveat, not a matrix defect (`reviewer-final-green.log:60`, `target/Cargo.lock:1000`, `target/Cargo.lock:3953`, `reviewer-deny.log:314`, `reviewer-deny-all-features.log:358`, `brief.md:235`).
- [ ] T5 Judgment — Decide whether to retain strict retry accounting, inconclusive empty listings, and the prerelease retention boundary — these declared open choices change what a PASS means across attempts and releases (`brief.md:238`, `target/crates/validate/tests/capability_matrix.rs:1282`, `target/crates/validate/tests/capability_matrix.rs:907`, `target/crates/validate/tests/capability_matrix.rs:334`).
- [ ] Validation — fitness-to-purpose — Accept this pure component as the intended foundation for #939 — the classifications are exercised, but deployment fitness and the future client-to-observation mapping are outside this slice's evidence (`brief.md:129`, `brief.md:151`, `target/crates/validate/src/matrix.rs:3`).
- [ ] **The gating C4-ci red comes from the base, not from this patch, for the third round running.** `cargo deny` fails on ctutils 0.4.2 (RUSTSEC-2026-0385) and tokio 1.52.3 (RUSTSEC-2026-0386) (`gate-logs/C4-ci.log:3873`, `:3955`, `:9367-9368`). The target pins exactly those versions (`Cargo.lock:1000-1001`, `Cargo.lock:3953-3954`), and its base commit is `93a6f37`. The carry-forward at `brief.md:235` said to rebuild on `b552ca2`, which carries #950 and #954. The patch touches no dependency. Someone has to refresh the base, or waive the red at sign-off. Another Do round will not fix it.
- [ ] **Two guards can be deleted with the suite still 53/53 green.** I ran both mutants. (a) `crates/validate/src/matrix.rs:908-914`, the check that the exercise matches the row. With it deleted, `classify(PRE_0_1.row(PutObject), &Exercise::GetObject { object, 200 + the right bytes })` returns `Pass`, so a GET answer passes the PUT row. `verdict` cannot reach this, because it looks the row up from the exercise. But `classify` is `pub`, and 743.3 may call it directly. Coverage agrees: lines 909 and 911 never ran (`gate-logs/C4-diff-cov.log`, the MISS list). (b) `matrix.rs:990-992`, which makes "a document where object bytes were expected" a `protocol` failure. Changed to `Ok(&[])`, a GET of an empty object answered `200` with a `Body::Listing` passes. For a non-empty object the failure class quietly changes from `protocol` to `integrity`. Line 991 is also on the MISS list. Fix: add one fixture for each, beside `capability_matrix.rs:538-546` and `:677-697`.
- [ ] **The two delete rows treat a retried HEAD differently, and the module's own doc contradicts one of them.** In one bulk exercise, follow-ups `[(a, 503), (a, 404)]` give `Pass` (`matrix.rs:1248-1251`, pinned at `capability_matrix.rs:856-862`). `DeleteObject` carries exactly one `follow_up: Answer` (`matrix.rs:768-772`), so the same retry has to be sent as a second exercise. The "every exercise must pass" rule (`matrix.rs:1393-1396`) then makes the run INCONCLUSIVE. I probed it: "not proved: DeleteObject: … answered 503". So the same evidence (one 503, then a 404 for the same key) proves the bulk row and not the single row. The bulk result also breaks the module invariant at `matrix.rs:26-28` ("a row with such a check and no fatal one is unproved"), and a strict reading of `brief.md:60-63` ("a 5xx → no verdict for the row"). This is open design point (1) in a new place (`brief.md:235`). Pick one retry rule, apply it to both rows, and make the rustdoc match.
- [ ] **A vacuous PASS is still possible at the profile level** (low priority: `PRE_0_1` cannot hit it today). `verdict` returns PASS whenever nothing is fatal and nothing is unproved (`matrix.rs:1436-1442`). A profile with no `required` or `unsupported` rows therefore passes an empty run. I probed it two ways: a profile holding only `not_checked(UploadPart)`, and a profile with no rows. Both give `verdict(&p, &[])` = PASS. Separately, `Profile::new` (`matrix.rs:250-252`) accepts a profile that declares the same operation twice, and `Profile::row` (`:267-269`) silently uses the first row. A profile declaring `PutObject` both `required` and `unsupported` passes a PUT 200. The no-duplicates check exists only for `PRE_0_1` (`capability_matrix.rs:314`). This is the rubric's "count-based assertion that can pass while the property fails" class. Fix: either make "zero proved rows" INCONCLUSIVE, or add a test over `PROFILES` requiring every profile to have at least one checked row and no duplicate operations.
- [ ] **The brief's 5xx rule for unsupported rows conflicts with 0017 §7** (low priority; the code follows the brief). A `503 SlowDown` to `CopyObject` is fatal `protocol` (`matrix.rs:936-941`, pinned at `capability_matrix.rs:380`). The same 503 to `PutObject` only leaves the row unsettled (`matrix.rs:965-967`). 0017's taxonomy table budgets every 5xx as `availability` (`0017:508`), while §3 says an unsupported row must not answer "a 500" (`0017:194`). The brief settled it as "a 5xx included" (`brief.md:33-34`). The cost: one transient 503 on any of the twelve unsupported rows turns a smoke run FAIL instead of INCONCLUSIVE. Confirm that this is intended.
- [ ] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [ ] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) —   - cargo deny --all-features --config deny-all-features.toml check advisories: exited with exit status: 1
- [ ] external dependency: cargo deny all-features advisories — the run's stack base `93a6f37` (`pdca-integration/main-r42c5348f203d`) still pins `ctutils 0.4.2` and `tokio 1.52.3` (`Cargo.lock:1000-1001`, `:3953-3954`), so `cargo deny --all-features --config deny-all-features.toml check advisories` exits 1 on the unpatched base (RUSTSEC-2026-0385, RUSTSEC-2026-0386; confirmed this round before the patch was applied) and C4-ci cannot go green on it; #950/#954 fixed both on `main` (`b552ca2` and later) but not on the integration line, and the standing doctor row runs in `../wyrd` (green), so it will not flag this. This patch touches no manifest, lockfile or deny config; refresh the integration line onto current `main` (or rebase this bundle on `main`) and re-run Check
- [ ] builder-declared external dependency `cargo deny all-features advisories` was REFUTED deterministically — its detect cmd (`cd "${WYRD_REPO:-../wyrd}" && { cargo fetch --locked >/dev/null 2>&1 || exit 3; } && cargo deny --frozen --all-features --config deny-all-features.toml check advisories >/dev/null 2>&1`) exited 0, so the dependency is present and full Check ran (#341); weigh the builder's claim against the review before accept
- [ ] **The delete acceptance rule turns missing evidence into an oracle failure.** `brief.md:47-51` requires a successful delete followed by HEAD 404, “else `oracle`”; `brief.md:74-77` only explicitly makes rows exercised “only with no answer” INCONCLUSIVE. For DELETE 204 followed by a HEAD timeout, an answer did arrive, but there is no evidence that the deleted key remains readable. The target distinguishes no-response and timeout outcomes (`crates/validate/src/s3/error.rs:36-47`), and proposal 0017 distinguishes transport availability from oracle disagreement and requires unresolved probes to remain INCONCLUSIVE (`docs/design/proposals/draft/0017-blackbox-validation-tool.md:489-493`, `:520-524`). Revise B′/E to define incomplete multi-observation checks separately from contradictory answers. Require deterministic fixtures for successful single/bulk delete followed by an unavailable HEAD → INCONCLUSIVE, alongside HEAD 200 → fatal oracle.
- [ ] **The PUT criterion omits the existing early-acknowledgement invariant.** `brief.md:38-39` checks 200 plus a strong ETag; the observation contract at `brief.md:150-154` describes response headers and response-body outcomes, without requiring preservation of request-source completion. A premature 200 with a valid ETag and a complete empty response body meets those stated checks even while the upload is unfinished. The target explicitly says a PUT is a receipt only when its source had ended before the response head (`crates/validate/src/s3.rs:161-170`) and rejects premature success at `:235-243`; its regression test asserts this at `crates/validate/tests/s3_client_upload_peers.rs:1207-1217`. Preserve that distinction in the matrix-owned observation and add a pure early-ack fixture with a valid ETag that cannot pass. Deferring client-to-observation mapping to #939 does not specify how this fact survives the boundary.
- [ ] **The claimed integration base is not the supplied target.** `brief.md:90-94` says this run's integration line “already carries #744 and #853,” but the target's HEAD equals local `main` at `4bac538`; both `git merge-base --is-ancestor refs/heads/enhancement/744-validate-payloads-and-size-classes HEAD` and the corresponding `enhancement/853-validate-s3-client-nonconforming-responses` check return 1. The source still defers #853's GET framing work (`crates/validate/src/s3.rs:264-268`). The repository and `main` target resolve, and no prerequisite is formally declared. Correct the ordering/base assertion to match the supplied source, or identify the integration revision that must be supplied before implementation relies on those changes; the current brief cannot substantiate that they are already present.
- [ ] **The run verdict is stricter than brief (E), and this is a contract 743.3 will inherit.** (E) says INCONCLUSIVE for a row "exercised **only** with no answer". `verdict` (`matrix.rs:1341-1366`) and the test `a_row_passes_only_when_every_exercise_of_it_passed` (`capability_matrix.rs:1029`) make the row unproved if *any* exercise got no answer. Probe: `[PUT NoAnswer, PUT 200 + strong ETag]` → INCONCLUSIVE. This is safe for the "never claims more" invariant. But it means one timeout followed by a successful retry ends a smoke run as INCONCLUSIVE, which fits poorly with §7's "must not abort … because one request timed out" (`0017:499-500`). A human should decide whether that strictness is intended before 743.3 builds retries on it.
- [ ] **A pre-release first tag gets no retained profile.** `PRE_0_1` covers everything `below 0.1.0` (`matrix.rs:416`), so `0.1.0-rc.1` and `0.1.0-rc.1+git.3.<sha>` select `pre-0.1` (shown by the test and by my probe). The 0017 amendment says "Retention starts at the first tag" (`0017:287`). If the first tag is `v0.1.0-rc.1`, that tag lands in the profile that is edited in place. Either the range should end at the first tag, or the amendment should say "the first `0.1.0` release".
- [ ] external dependency.
- [ ] **One timed-out retry makes a row INCONCLUSIVE, which the brief's (E) does not say.** The brief: INCONCLUSIVE when a row was "exercised **only** with no answer". The code marks a row unproved if *any* exercise of it got no answer or was unsettled (`matrix.rs:1400-1401`). The test `capability_matrix.rs:1252-1258` pins that stricter rule. Concrete case: a full passing run plus one extra `GetObject` that timed out → INCONCLUSIVE. By the brief's wording that run is PASS. The rustdoc contradicts itself on this exact input: `matrix.rs:1376-1377` says "exercised only with no answer", and `:1377-1378` says "proved only when … every exercise of it passed". The stricter rule never over-claims, and for a timed-out *mutation* §7 (`0017:528-539`) supports it. But for reads, it means one transient timeout in any run that feeds every attempt to `verdict` can never be PASS. That cuts against §7's budgeted `availability` class (`0017:508`, `0017:499-500`). Decide which rule `verdict` keeps (or whether 743.3 feeds it retries at all), then make the rustdoc say one thing.
- [ ] **The diff-coverage gate measured nothing, and the base needs confirming before publishing.** `gate-logs/C4-diff-cov.log`: "patch.diff does not apply on origin/main". The target's base commit is `93a6f37`, which is neither base the brief names (`main` `4bac538`, integration tip `582da99`). This is not evidence against the fix: C5-mutants caught 127 of the 128 viable mutants, and the one survivor is equivalent (next bullet). But the PR's base is not settled. Whether `ErrorCode` (including `MissingInXml`, used at `capability_matrix.rs:337`) is identical on the base the PR finally targets is the brief's claim, and I could not check it here.
- [ ] C1 Spec — Decide when a first prerelease tag freezes its expectations — `0.1.0-rc.1` selects the mutable `pre-0.1` profile although retention starts at the first tag; this affects repeatable validation of tagged builds (`target/crates/validate/tests/capability_matrix.rs:292`, `target/docs/design/proposals/draft/0017-blackbox-validation-tool.md:284`, `reviewer-probes.log:2`).
- [ ] C5 Causal adequacy — Decide whether one unanswered retry must keep an otherwise proved row INCONCLUSIVE — the implementation requires every exercise to pass, stricter than criterion E's “exercised only with no answer”; direct execution confirms the distinction (`target/crates/validate/src/matrix.rs:1393`, `brief.md:238`, `reviewer-probes.log:1`).
- [ ] T5 Judgment — Decide whether an exactly empty listing of an empty prefix proves the listing row — current behavior makes the run INCONCLUSIVE, adding a coverage requirement beyond equality with the expected keys; the brief explicitly reserves this policy decision (`target/crates/validate/src/matrix.rs:1170`, `brief.md:238`, `reviewer-probes.log:3`).
- [ ] **The gating `C4-ci` red comes from a stale base, not from this patch.** The worktree is on `93a6f37`, not the `b552ca2` that the iteration-3 carry-forward told this round to rebuild on (`brief.md:235`). The target's lockfile still pins `ctutils 0.4.2` (`Cargo.lock:1000-1001`) and `tokio 1.52.3` (`Cargo.lock:3953-3954`). Those are exactly RUSTSEC-2026-0385 and -0386, the only failures (`gate-logs/C4-ci.log:3865-3872`, `:5168-5170`). Every other CI step passed both attempts: fmt, clippy, build, the whole test run with `capability_matrix` at 48/48 (`C4-ci.log:3179-3229`), typos, docs lint and render, the blackbox and unsafe guards. The gates also measured two different bases: `C4-diff-cov` compiled `ctutils v0.4.5` and `tokio v1.54.1` in another worktree (`C4-diff-cov.log:12-13`), while `C4-ci` ran on the old one. The patch adds files plus one `lib.rs` line and a draft-doc edit, so it does not depend on the base. But the gating row cannot go green until the driver rebases the worktree onto current `main` and re-runs it. That is a harness or human action, not build work.
- [ ] **(Low; the outcome is FAIL either way.) A 404 to the HEAD or GET of a key the run just wrote is classed `protocol`, not `oracle`.** Probe on production: `HeadObject` answered `404` gives `Fatal { Protocol, "answered 404; expected 200" }`, and the same holds for `GetObject`. The cause is `expect_status` (`crates/validate/src/matrix.rs:972-981`) running before the oracle checks (`:1124`, `:1081`). Brief (B′) says HeadObject is "200 with the object's size and the PUT's `ETag`, else `oracle`". §7 (`0017:505`) defines `oracle` as "model and server disagree", and a written key that reads as absent fits that. But §7's `protocol` row ("wrong status") also fits. Only the class label in the report changes, and later scenarios that key on the class (#746) will inherit whichever reading is chosen. A human should pick one.
- [ ] external dependency: cargo deny all-features advisories — the run's stack base `93a6f37` (`pdca-integration/main-r42c5348f203d`) still pins `ctutils 0.4.2` and `tokio 1.52.3` (`Cargo.lock`), so `cargo deny check` and the all-features advisories check fail on the unpatched base (RUSTSEC-2026-0385, RUSTSEC-2026-0386) and C4-ci cannot go green on it; #950/#954 fixed both on `main` `b552ca2` but not on the integration line, and the standing doctor row checks `../wyrd` (`b552ca2`, green), so it will not flag this. This patch touches no manifest, lockfile or deny config; refresh the integration line onto `b552ca2` (or rebase this bundle on `main` with the one-line `lib.rs` change above) and re-run Check
- [ ] **The gating C4-ci row is red for the same reason as iteration 4, and the base is still not the one the human asked for.** `gate-logs/C4-ci.log:3865-3872` and `:3947-3954` show cargo-deny failing on RUSTSEC-2026-0385 (ctutils) and RUSTSEC-2026-0386 (tokio). The target is built on `93a6f37` (`git log -1`: "pre-fix base 93a6f37…"), whose `Cargo.lock` still pins ctutils 0.4.2 (`target/Cargo.lock:1000-1001`) and tokio 1.52.3 (`:3953-3954`). The iteration-3 sign-off said to rebuild on `b552ca2`, which carries #950 and #954 (`brief.md:235`). Iteration 4 asked for "matching full-CI evidence on the intended refreshed base" (`brief.md:243`). Neither happened. The patch touches no Cargo file, so the failure is not the matrix's fault, but the full-CI evidence the human asked for still does not exist. The fix is choosing the base, which the builder cannot do.
- [ ] external dependency: cargo deny all-features advisories — the run's stack base `93a6f37` (`pdca-integration/main-r42c5348f203d`) still pins `ctutils 0.4.2` and `tokio 1.52.3` (`Cargo.lock:1000-1001`, `:3953-3954`), so `cargo deny --all-features --config deny-all-features.toml check advisories` exits 1 on the unpatched base (RUSTSEC-2026-0385, RUSTSEC-2026-0386; confirmed this round before the patch was applied) and C4-ci cannot go green on it; #950/#954 fixed both on `main` `b552ca2` but not on the integration line, and the standing doctor row runs in `../wyrd` (`b552ca2`, green), so it will not flag this. This patch touches no manifest, lockfile or deny config; refresh the integration line onto `b552ca2` (or rebase this bundle on `main` with the one-line `lib.rs` hunk in iteration 4's notes) and re-run Check

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Auto-iterate (round 5): rebuilding for the implementation-level findings — C2 Reproduction (red pre-fix) — Accept the declared new-API exception or require another discriminator — production stashed with the submitted test retained fails to compile, so no behavioral pre-fix red executes; all ten required fault injections are caught (`brief.md:138`, `reviewer-red.log:260`, `reviewer-mutations.log:2`, `reviewer-mutation-retries.log:2`).; C4 Verification (red→green) — Require full-CI evidence on the intended refreshed base — 53 restored tests pass, but both advisory scans still fail on unchanged ctutils/tokio versions despite the tracked baseline fixes; this is a baseline/evidence caveat, not a matrix defect (`reviewer-final-green.log:60`, `target/Cargo.lock:1000`, `target/Cargo.lock:3953`, `reviewer-deny.log:314`, `reviewer-deny-all-features.log:358`, `brief.md:235`).; **Two guards can be deleted with the suite still 53/53 green.** I ran both mutants. (a) `crates/validate/src/matrix.rs:908-914`, the check that the exercise matches the row. With it deleted, `classify(PRE_0_1.row(PutObject), &Exercise::GetObject { object, 200 + the right bytes })` returns `Pass`, so a GET answer passes the PUT row. `verdict` cannot reach this, because it looks the row up from the exercise. But `classify` is `pub`, and 743.3 may call it directly. Coverage agrees: lines 909 and 911 never ran (`gate-logs/C4-diff-cov.log`, the MISS list). (b) `matrix.rs:990-992`, which makes "a document where object bytes were expected" a `protocol` failure. Changed to `Ok(&[])`, a GET of an empty object answered `200` with a `Body::Listing` passes. For a non-empty object the failure class quietly changes from `protocol` to `integrity`. Line 991 is also on the MISS list. Fix: add one fixture for each, beside `capability_matrix.rs:538-546` and `:677-697`.; **A vacuous PASS is still possible at the profile level** (low priority: `PRE_0_1` cannot hit it today). `verdict` returns PASS whenever nothing is fatal and nothing is unproved (`matrix.rs:1436-1442`). A profile with no `required` or `unsupported` rows therefore passes an empty run. I probed it two ways: a profile holding only `not_checked(UploadPart)`, and a profile with no rows. Both give `verdict(&p, &[])` = PASS. Separately, `Profile::new` (`matrix.rs:250-252`) accepts a profile that declares the same operation twice, and `Profile::row` (`:267-269`) silently uses the first row. A profile declaring `PutObject` both `required` and `unsupported` passes a PUT 200. The no-duplicates check exists only for `PRE_0_1` (`capability_matrix.rs:314`). This is the rubric's "count-based assertion that can pass while the property fails" class. Fix: either make "zero proved rows" INCONCLUSIVE, or add a test over `PROFILES` requiring every profile to have at least one checked row and no duplicate operations.; C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — - cargo deny --all-features --config deny-all-features.toml check advisories: exited with exit status: 1. Deferred, not addressed here: 9 finding(s) that need human judgment, held in deferred-findings.json for the human at handover — not build work
- By / date: auto-iterate / 2026-10-19

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
