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
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: unverifiable —                why this slice has no isolable red (the cargo output is above).
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage not measured — patch.diff does not apply on origin/main
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 152 mutants tested in 10m: 16 missed, 106 caught, 30 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_937/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 15.62s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Two classifier corrections are needed in #937’s new pure capability matrix, version selection, and run-verdict functionality.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance contract distinguishes fatal answers from unresolved observations and bounds this slice to pure classification; `brief.md:20`, `brief.md:54`, `brief.md:121`. |
| C2 Reproduction (red pre-fix) | N/A | This is a declared new API: stashing production while retaining the test reproduces a missing-module compile error, not an executed behavioral red; `brief.md:138`, `reviewer-red.log:3`. |
| C3 Change | PASS | The four-file, 85,859-byte patch stays within the authorized matrix, tests, module export, and draft-proposal amendment; `target/crates/validate/src/lib.rs:24`, `target/docs/design/proposals/draft/0017-blackbox-validation-tool.md:196`, `reviewer-checks.log:8`. |
| C4 Verification (red→green) | NEEDS-HUMAN | Accept or require stronger evidence for the declared net-new API exception — 41 tests pass after restoration, but the pre-fix test cannot execute and the coverage gate used a mismatched base; `reviewer-restored-green.log:49`, `reviewer-red.log:3`, `gate-logs/C4-diff-cov.log:10`. |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Correct the two failed-body classifications — an unresolved PUT currently yields PASS and an unresolved matching conditional GET yields fatal protocol, violating the evidence limit; `target/crates/validate/src/matrix.rs:1046`, `target/crates/validate/src/matrix.rs:1092`, `reviewer-cases.log:6`, `reviewer-cases.log:13`. |
| T1 Structure | PASS | The scenario-to-classifier boundary remains pure and independent of client error types, with only the explicitly permitted ErrorCode reuse; `target/crates/validate/src/matrix.rs:33`, `target/crates/validate/src/matrix.rs:643`, `target/crates/validate/src/matrix.rs:747`. |
| T2 Shape | PASS | The patch meets the size backstop, pinned formatting, spelling, and documentation checks; `reviewer-checks.log:1`, `reviewer-checks.log:8`. |
| T3 Runtime | PASS | The slice preserves in-memory execution: classification introduces no external-work waits, clock lifecycle, or mutable global state; `target/crates/validate/src/matrix.rs:916`, `target/crates/validate/src/matrix.rs:1032`, `target/crates/validate/src/matrix.rs:1341`. |
| T4 Contribution | FAIL | The frozen batch review remains unresolved: its five entries reduce to the same two independently reproduced defects; affected-path prior art reveals no competing closed-unmerged implementation; `gate-logs/T4-batch-review.log:10`, `reviewer-cases.log:28`, `reviewer-prior-art-path-audit.log:1`. |
| T5 Judgment | NEEDS-HUMAN [impl] | Add regression coverage for failed PUT and conditional-GET response bodies — the 41-test suite passes while both verdict errors survive; `target/crates/validate/tests/capability_matrix.rs:438`, `target/crates/validate/tests/capability_matrix.rs:539`, `target/crates/validate/tests/capability_matrix.rs:682`, `reviewer-cases.log:28`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Approve the classification contract and editable pre-0.1 profile policy for downstream smoke use, including the proposed early-PUT protocol class — pure classifier tests cannot decide those product semantics; `brief.md:40`, `target/docs/design/proposals/draft/0017-blackbox-validation-tool.md:284`. |

All source citations resolve in the supplied `$PDCA_TARGET` (`target/`); other citations name evidence in this review directory. This review is advisory. Production files were restored byte-for-byte after the stash cycle; no implementation fix was made.

Two independently reproduced defects explain the implementation verdicts:

1. **An incomplete PUT response can falsely certify success.** At `target/crates/validate/src/matrix.rs:1045`, a 200 response with a strong ETag and `request_body_ended = true` returns success without accounting for `Body::Failed`. A one-row required-PUT profile consequently returns `PASS` after a response-body connection reset (`reviewer-cases.rs:14`, `reviewer-cases.log:6`). The observation explicitly represents transport failure (`target/crates/validate/src/matrix.rs:716`), and the module promises that a cut-off body leaves the row unproved (`target/crates/validate/src/matrix.rs:26`); proposal §7 also classifies transport failure separately from correctness failure (`target/docs/design/proposals/draft/0017-blackbox-validation-tool.md:508`). Preserve that uncertainty as `Unsettled`, folding to `INCONCLUSIVE`, while retaining the established fatal checks for an early PUT or invalid ETag.
2. **A failed conditional-GET body manufactures a protocol failure.** At `target/crates/validate/src/matrix.rs:1092`, the wildcard arm treats `Body::Failed` as evidence that a forbidden body arrived. A matching conditional GET with status 304 and a connection reset instead produces `FAIL: protocol: answered 304 with a body` (`reviewer-cases.rs:24`, `reviewer-cases.log:13`). Failure to finish observing a body does not establish nonempty content. Handle the failed observation as unresolved; keep actually observed nonempty bytes fatal. The successful PUT and empty-304 controls both pass (`reviewer-cases.log:18`).

The supplied tests pass, but their failed-body coverage misses both defects. The reviewer probes compile against the actual cargo-built `wyrd_validate` library, without replacing its implementation or dependencies (`reviewer-cases.log:1`). Their two failures are executable counterexamples, not deductions from the batch-review labels. The batch log repeats the PUT concern three times and the conditional-GET concern twice; these are two distinct findings. Its additional nonempty-PUT-body claim is not needed to establish either finding.

Verification and frozen-gate evidence have the following limits:

- **C4-ci: PASS from the frozen full-suite log.** It records real fmt, clippy, build, tests, dependency checks, and DST checks, ending in success (`gate-logs/C4-ci.log:30`, `gate-logs/C4-ci.log:4491`). Independent reruns covered the requested capability test, pinned formatting, spelling, docs lint/render, and diff whitespace. The initially selected default toolchain lacked rustfmt; explicitly selecting the repository’s pinned 1.96.0 toolchain passed. Neither external dependency named in the brief remains undischarged: typos ran, and the renderer built 99 pages with a passing link audit (`reviewer-checks.log:2`, `reviewer-checks.log:6`).
- **C4-verify: the declared net-new exception is reproduced.** With production stashed and the new test retained, Cargo exits 101 on the absent matrix module; after `git stash pop`, all 41 tests pass (`reviewer-stash-cycle.log:2`, `reviewer-restored-green.log:49`). This agrees with the frozen exit-77 explanation (`gate-logs/C4-verify.log:181`). It does not establish an executed pre-fix behavioral failure. The frozen mutation summary also does not individually document all ten custom mutation demonstrations requested by `brief.md:143`; those demonstrations are not affirmed here.
- **C4-diff-cov: coverage is unmeasured, not a compile defect.** The log stops because the patch does not apply to its `origin/main` base (`gate-logs/C4-diff-cov.log:10`). The supplied patched target compiles and its source citations ground correctly. Treat the coverage failure as a base-state caveat; it supplies no coverage percentage or patch defect.
- **C5-mutants: the frozen run reports 16 missed, 106 caught, and 30 unviable mutations** (`gate-logs/C5-mutants.log:29`). The missed listing/delete failed-body branches show additional test gaps (`gate-logs/C5-mutants.log:25`). A surviving formatting or equivalent-parser mutation is not automatically a defect. The two findings above have separate executable evidence. The full mutation campaign and instance-root wrappers were not independently rerun; their supplied logs are the evidence for those runs.
- **T4-batch-review: FAIL**, with its substantive findings independently consolidated and reproduced above (`gate-logs/T4-batch-review.log:10`).
- **T4-contribution: N/A.** The contribution artifacts are absent by design at Check; the mandatory publish audit owns their substantive verdict (`gate-logs/T4-contribution.log:10`). No human clearance is requested for this deferred row.
- **host-tikv: PASS from the frozen compile evidence.** Both metadata-tikv and server feature clippy commands finish successfully (`gate-logs/host-tikv.log:111`, `gate-logs/host-tikv.log:209`); this establishes feature compilation, not a live TiKV integration run.

The standing rubric and prior-art checks do not add another defect. The target gateway still denies `uploads`, `uploadId`, and `partNumber`, so the six unsupported multipart rows match this base (`target/crates/gateway-s3/src/lib.rs:410`). The cited gateway response paths and version-derivation identities ground in the target. The patch introduces input-validation checks, not an optional-capability probe masking a load-time cause. Its draft-proposal amendment follows the expressly scoped documentation requirement; no new wire operation, CLI flag, or persisted field is introduced.

The path-based prior-art check was independently repeated against GitHub’s default-branch history for `crates/validate/` and `docs/design/proposals/draft/0017-blackbox-validation-tool.md` (`reviewer-prior-art-validate.log:1`, `reviewer-prior-art-proposal.log:1`). All closed-unmerged PRs were enumerated with pagination, and every file path in all 19 such PRs was checked: none matched either affected surface (`reviewer-prior-art-path-audit.log:1`). The earlier rejected local attempt is described by the supplied brief (`brief.md:173`); it was not supplied for independent inspection, and this review does not re-raise any tracked deferral.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — #937 (743.1, capability matrix)

**Evidence re-run.** I built the production `crates/validate/src/matrix.rs` and the shipped `crates/validate/tests/capability_matrix.rs`, both copied unchanged, in a small standalone crate under `$PDCA_SCRATCH`. The only stand-in was `s3::ErrorCode`, copied from `crates/validate/src/s3/error.rs:69-76,191-199`. Result: 41/41 green. The test calls the production functions, not a copy. C4-verify cannot show a red here (the module is new, so the test does not compile without it; pre-declared in the brief). So I re-made four of the brief's required reds by hand. Each one turns the named leg red: (6) selector ignores the version → 4 tests fail; (7) `+git.unknown` read as `0.0.0` → `unknown_is_never_read_as_release_0_0_0` fails; (8) a not-checked row counted as unexercised → fails; (10) a probe with no answer called `oracle` → 3 tests fail. The red→green evidence holds where I checked it. The gaps below are cases the suite does not pin down.

## Findings

- NEEDS-HUMAN [impl] — **A torn response passes three required rows.** The module's own invariant (`crates/validate/src/matrix.rs:26-27`) says a body cut off in transport leaves the row "unproved, never passed and never failed". The rubric's *Protocol input* class says the same: torn input is never silently accepted. But the PUT arm (`matrix.rs:1034-1052`) never looks at `r.body`, and neither do HEAD (`:1100-1118`) or DELETE (`:1120-1124`). Probe results on production code: `PUT 200 + strong ETag + Body::Failed` → `Pass`; `HEAD 200 right size/ETag + Body::Failed` → `Pass`; `DELETE 204 + Body::Failed, HEAD 404` → `Pass`. So a run where those answers were torn reports PASS, not INCONCLUSIVE. The T4 batch review flagged the PUT case three times (gate-logs/T4-batch-review.log). The probe confirms it, and the same gap exists on HEAD and DELETE, which T4 did not name. No test feeds `Body::Failed` to these rows (`capability_matrix.rs:682` covers GET only).
- NEEDS-HUMAN [impl] — **The opposite error on the matching conditional GET.** `matrix.rs:1091-1092`: `304 + Body::Failed` → `Fatal { Protocol, "answered 304 with a body" }`. §7 (`0017:508`) classes a transport failure as `availability` (budgeted), never fatal. The module doc at `:26-27` says "never failed" too. A reset after a 304 head would FAIL the run. It should be `Unsettled`, as the GET, listing and bulk-delete arms already do (`:1136`, `:1195`).
- NEEDS-HUMAN [impl] — **A rubric grammar class has no test (surviving mutant `matrix.rs:1266`).** The production code is correct: `bytes +2-+5/+16` → `protocol`. But the test does not pin it. I changed `!s.is_empty() && …` to `||` in a copy, and `u64::from_str` then accepts the `+` signs: the ranged GET with `Content-Range: bytes +2-+5/+16` returned `Pass`, and the shipped suite stayed **41/41 green**. The rubric names this exact case ("no `+`/`-` signs via `from_str`"). Fix: add a signed value to the wrong-`Content-Range` list at `capability_matrix.rs:503-509`.
- NEEDS-HUMAN [impl] — **The bulk-delete probe never checks which key a HEAD answer belongs to (surviving mutant `matrix.rs:1225`).** I changed `k == key` to `k != key` in a copy. Then `keys [a,b]`, a clean `<DeleteResult>`, and follow-ups only for an unrequested key (`[(c,404),(c,404)]`) returned `Pass`. Production correctly returns `Unsettled`. The shipped suite stayed 41/41 green, because every fixture gives both keys the same answer or a symmetric pair. Add a fixture where the probed key differs from the requested ones.
- NEEDS-HUMAN [impl] — **Duplicate follow-ups for one key are silently dropped.** `matrix.rs:1225` takes the *first* follow-up per key and ignores the rest. Probe on production: follow-ups `[(a,404),(a,200),(b,404)]` → `Pass`, even though a later HEAD shows the deleted key readable (`oracle`, `0017:505`). The other order, `[(a,503),(a,404)]`, gives `Unsettled` even though the retry proved the delete. The type permits this input (`follow_ups: Vec<(String, Answer)>`, `:786`), and 743.3's mapping could produce it after a retry. Silently skipping it hits the rubric's *Absent entries* class. Fix: judge every follow-up for a key, or call duplicates or unrequested keys unsettled.
- NEEDS-HUMAN [impl] — **More untested branches from C5's 16 missed mutants.** These are real gaps, not cosmetic. Deleting the `Body::Failed` arm for listings (`:1136`) or for bulk delete (`:1195`) turns a transport cut into fatal `protocol`, and the suite stays green. The pre-release character check (`:554`) is never exercised: under the `||`→`&&` mutant, `0.0.0-r_c` parses. Add it to `parse_refuses_malformed_versions` (`capability_matrix.rs:746`). The 304-with-`Bytes(empty)` guard (`:1091`) is untested, yet 743.3's client may well map a bodiless reply that way. The range guard (`:1065`) is never hit, so a `>`→`>=` regression would mark every single-byte range unsettled without failing a test. The Display impls and the unused `Ord for Release` are cosmetic; skip them.
- NEEDS-HUMAN [human] — **The run verdict is stricter than brief (E), and this is a contract 743.3 will inherit.** (E) says INCONCLUSIVE for a row "exercised **only** with no answer". `verdict` (`matrix.rs:1341-1366`) and the test `a_row_passes_only_when_every_exercise_of_it_passed` (`capability_matrix.rs:1029`) make the row unproved if *any* exercise got no answer. Probe: `[PUT NoAnswer, PUT 200 + strong ETag]` → INCONCLUSIVE. This is safe for the "never claims more" invariant. But it means one timeout followed by a successful retry ends a smoke run as INCONCLUSIVE, which fits poorly with §7's "must not abort … because one request timed out" (`0017:499-500`). A human should decide whether that strictness is intended before 743.3 builds retries on it.
- NEEDS-HUMAN [human] — **A pre-release first tag gets no retained profile.** `PRE_0_1` covers everything `below 0.1.0` (`matrix.rs:416`), so `0.1.0-rc.1` and `0.1.0-rc.1+git.3.<sha>` select `pre-0.1` (shown by the test and by my probe). The 0017 amendment says "Retention starts at the first tag" (`0017:287`). If the first tag is `v0.1.0-rc.1`, that tag lands in the profile that is edited in place. Either the range should end at the first tag, or the amendment should say "the first `0.1.0` release".
- Minor doc point, no tag: `0017:263` says "every gateway response carries `Server: wyrd/<version>`". But `crates/gateway-s3/src/lib.rs:1696-1698` says responses hyper produces before the service runs (a malformed request line, oversized headers) carry none. "Every response the service produces" would be accurate. The brief supplied this wording, so it is not the builder's slip.
- Gate evidence, no tag: C4-diff-cov measured nothing ("patch.diff does not apply on origin/main", gate-logs/C4-diff-cov.log). The only coverage-style evidence is C5 (106 caught / 16 missed), and the misses above are where it shows. The bundle may need a refresh onto current `main` before publish.

## Attempted and could not refute

- **Version parsing:** `unknown`, `+git.unknown[.dirty]` with or without a pre-release, `+git.unknownx`, `+git.UNKNOWN`, u64 overflow, leading zeros, `_` in build metadata, an empty pre-release or build, `0.1.0+git.dirty` (the dirty-tag shape at `crates/server/src/version/derivation.rs:58-60`). Each gives the right release, unknown, or error. Selection edges (`1.0.0-rc.1` vs a `1.0.0` bound) and the overlap check are correct.
- **Unsupported rows:** any 2xx (including a cut-off body) → `unexpected-support`; 3xx, 4xx and 5xx mismatches → `protocol`; no head → no verdict; the right status with a cut-off body → unsettled.
- **PUT and ranged GET:** an early answer to a PUT is `protocol` whatever its ETag. `*`, unquoted, `W/` and empty ETags are refused, and DEL/space/quote inside the tag are refused. A ranged exercise with first > last is unsettled, not a panic.
- **Gateway citations:** every `pre-0.1` row cites a line that gives today's answer on this base (`lib.rs:1766-1826`, `:1842-1849`, `:1865-1871`, `:2008-2028`, `:2713-2719`, `:2785-2803`, `:2293-2300`; `GET /` → `bucket_scoped_path` returns `None` → 400 at `:1819-1826`). The test at `capability_matrix.rs:252` checks only the path prefix, so citation drift will not be caught. I checked them by hand.
- The composition root does set the advertised version (`crates/server/src/cli.rs:2681`), so `pre-0.1` can be selected from the wire.
<!-- pdca:leaf-complete -->

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/validate/src/matrix.rs:1045`: A PUT with status 200, a strong ETag and `request_body_ended: true` returns `Pass` without inspecting its response body. Thus `Body::Failed` can produce a run-level PASS despite an incomplete response; nonempty response bytes also pass despite the client's empty-body contract. Check the body before returning success: transport failure should leave the row unsettled, and an unexpected body should be `protocol`. Add both fixtures while preserving the existing early-acknowledgement failure.

- NEEDS-HUMAN [impl] — `crates/validate/src/matrix.rs:1092`: A matching conditional GET with status 304 and `Body::Failed` falls into “answered 304 with a body,” producing fatal `protocol`. A timeout/reset does not establish that forbidden bytes arrived, so this manufactures FAIL where the observation only supports INCONCLUSIVE. Handle `Body::Failed` as unsettled and retain the fatal result for actual nonempty bytes. A shared empty-body checker could cover this and the PUT case consistently.

- NEEDS-HUMAN [impl] — `crates/validate/tests/capability_matrix.rs:653`: The bulk-delete fixtures do not distinguish correctly associated probes from probes of other keys. Frozen `gate-logs/C5-mutants.log` confirms that changing `k == key` to `k != key` at `crates/validate/src/matrix.rs:1225` survives. Add a three-key fixture with ordered HEAD results `a:404, b:404, c:200` and require fatal `oracle`: the surviving mutation checks b for a and a for both b and c, falsely passing without examining c's readable result.

No separate efficiency findings. Findings are advisory; no gate was rerun.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Accept or require stronger evidence for the declared net-new API exception — 41 tests pass after restoration, but the pre-fix test cannot execute and the coverage gate used a mismatched base; `reviewer-restored-green.log:49`, `reviewer-red.log:3`, `gate-logs/C4-diff-cov.log:10`.
- [ ] C5 Causal adequacy — Correct the two failed-body classifications — an unresolved PUT currently yields PASS and an unresolved matching conditional GET yields fatal protocol, violating the evidence limit; `target/crates/validate/src/matrix.rs:1046`, `target/crates/validate/src/matrix.rs:1092`, `reviewer-cases.log:6`, `reviewer-cases.log:13`.
- [ ] T5 Judgment — Add regression coverage for failed PUT and conditional-GET response bodies — the 41-test suite passes while both verdict errors survive; `target/crates/validate/tests/capability_matrix.rs:438`, `target/crates/validate/tests/capability_matrix.rs:539`, `target/crates/validate/tests/capability_matrix.rs:682`, `reviewer-cases.log:28`.
- [ ] Validation — fitness-to-purpose — Approve the classification contract and editable pre-0.1 profile policy for downstream smoke use, including the proposed early-PUT protocol class — pure classifier tests cannot decide those product semantics; `brief.md:40`, `target/docs/design/proposals/draft/0017-blackbox-validation-tool.md:284`.
- [ ] **A torn response passes three required rows.** The module's own invariant (`crates/validate/src/matrix.rs:26-27`) says a body cut off in transport leaves the row "unproved, never passed and never failed". The rubric's *Protocol input* class says the same: torn input is never silently accepted. But the PUT arm (`matrix.rs:1034-1052`) never looks at `r.body`, and neither do HEAD (`:1100-1118`) or DELETE (`:1120-1124`). Probe results on production code: `PUT 200 + strong ETag + Body::Failed` → `Pass`; `HEAD 200 right size/ETag + Body::Failed` → `Pass`; `DELETE 204 + Body::Failed, HEAD 404` → `Pass`. So a run where those answers were torn reports PASS, not INCONCLUSIVE. The T4 batch review flagged the PUT case three times (gate-logs/T4-batch-review.log). The probe confirms it, and the same gap exists on HEAD and DELETE, which T4 did not name. No test feeds `Body::Failed` to these rows (`capability_matrix.rs:682` covers GET only).
- [ ] **The opposite error on the matching conditional GET.** `matrix.rs:1091-1092`: `304 + Body::Failed` → `Fatal { Protocol, "answered 304 with a body" }`. §7 (`0017:508`) classes a transport failure as `availability` (budgeted), never fatal. The module doc at `:26-27` says "never failed" too. A reset after a 304 head would FAIL the run. It should be `Unsettled`, as the GET, listing and bulk-delete arms already do (`:1136`, `:1195`).
- [ ] **A rubric grammar class has no test (surviving mutant `matrix.rs:1266`).** The production code is correct: `bytes +2-+5/+16` → `protocol`. But the test does not pin it. I changed `!s.is_empty() && …` to `||` in a copy, and `u64::from_str` then accepts the `+` signs: the ranged GET with `Content-Range: bytes +2-+5/+16` returned `Pass`, and the shipped suite stayed **41/41 green**. The rubric names this exact case ("no `+`/`-` signs via `from_str`"). Fix: add a signed value to the wrong-`Content-Range` list at `capability_matrix.rs:503-509`.
- [ ] **The bulk-delete probe never checks which key a HEAD answer belongs to (surviving mutant `matrix.rs:1225`).** I changed `k == key` to `k != key` in a copy. Then `keys [a,b]`, a clean `<DeleteResult>`, and follow-ups only for an unrequested key (`[(c,404),(c,404)]`) returned `Pass`. Production correctly returns `Unsettled`. The shipped suite stayed 41/41 green, because every fixture gives both keys the same answer or a symmetric pair. Add a fixture where the probed key differs from the requested ones.
- [ ] **Duplicate follow-ups for one key are silently dropped.** `matrix.rs:1225` takes the *first* follow-up per key and ignores the rest. Probe on production: follow-ups `[(a,404),(a,200),(b,404)]` → `Pass`, even though a later HEAD shows the deleted key readable (`oracle`, `0017:505`). The other order, `[(a,503),(a,404)]`, gives `Unsettled` even though the retry proved the delete. The type permits this input (`follow_ups: Vec<(String, Answer)>`, `:786`), and 743.3's mapping could produce it after a retry. Silently skipping it hits the rubric's *Absent entries* class. Fix: judge every follow-up for a key, or call duplicates or unrequested keys unsettled.
- [ ] **More untested branches from C5's 16 missed mutants.** These are real gaps, not cosmetic. Deleting the `Body::Failed` arm for listings (`:1136`) or for bulk delete (`:1195`) turns a transport cut into fatal `protocol`, and the suite stays green. The pre-release character check (`:554`) is never exercised: under the `||`→`&&` mutant, `0.0.0-r_c` parses. Add it to `parse_refuses_malformed_versions` (`capability_matrix.rs:746`). The 304-with-`Bytes(empty)` guard (`:1091`) is untested, yet 743.3's client may well map a bodiless reply that way. The range guard (`:1065`) is never hit, so a `>`→`>=` regression would mark every single-byte range unsettled without failing a test. The Display impls and the unused `Ord for Release` are cosmetic; skip them.
- [ ] **The run verdict is stricter than brief (E), and this is a contract 743.3 will inherit.** (E) says INCONCLUSIVE for a row "exercised **only** with no answer". `verdict` (`matrix.rs:1341-1366`) and the test `a_row_passes_only_when_every_exercise_of_it_passed` (`capability_matrix.rs:1029`) make the row unproved if *any* exercise got no answer. Probe: `[PUT NoAnswer, PUT 200 + strong ETag]` → INCONCLUSIVE. This is safe for the "never claims more" invariant. But it means one timeout followed by a successful retry ends a smoke run as INCONCLUSIVE, which fits poorly with §7's "must not abort … because one request timed out" (`0017:499-500`). A human should decide whether that strictness is intended before 743.3 builds retries on it.
- [ ] **A pre-release first tag gets no retained profile.** `PRE_0_1` covers everything `below 0.1.0` (`matrix.rs:416`), so `0.1.0-rc.1` and `0.1.0-rc.1+git.3.<sha>` select `pre-0.1` (shown by the test and by my probe). The 0017 amendment says "Retention starts at the first tag" (`0017:287`). If the first tag is `v0.1.0-rc.1`, that tag lands in the profile that is edited in place. Either the range should end at the first tag, or the amendment should say "the first `0.1.0` release".
- [ ] `crates/validate/src/matrix.rs:1045`: A PUT with status 200, a strong ETag and `request_body_ended: true` returns `Pass` without inspecting its response body. Thus `Body::Failed` can produce a run-level PASS despite an incomplete response; nonempty response bytes also pass despite the client's empty-body contract. Check the body before returning success: transport failure should leave the row unsettled, and an unexpected body should be `protocol`. Add both fixtures while preserving the existing early-acknowledgement failure.
- [ ] `crates/validate/src/matrix.rs:1092`: A matching conditional GET with status 304 and `Body::Failed` falls into “answered 304 with a body,” producing fatal `protocol`. A timeout/reset does not establish that forbidden bytes arrived, so this manufactures FAIL where the observation only supports INCONCLUSIVE. Handle `Body::Failed` as unsettled and retain the fatal result for actual nonempty bytes. A shared empty-body checker could cover this and the PUT case consistently.
- [ ] `crates/validate/tests/capability_matrix.rs:653`: The bulk-delete fixtures do not distinguish correctly associated probes from probes of other keys. Frozen `gate-logs/C5-mutants.log` confirms that changing `k == key` to `k != key` at `crates/validate/src/matrix.rs:1225` survives. Add a three-key fixture with ordered HEAD results `a:404, b:404, c:200` and require fatal `oracle`: the surviving mutation checks b for a and a for both b and c, falsely passing without examining c's readable result.
- [ ] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_937/review-b
- [ ] external dependency.
- [ ] **The delete acceptance rule turns missing evidence into an oracle failure.** `brief.md:47-51` requires a successful delete followed by HEAD 404, “else `oracle`”; `brief.md:74-77` only explicitly makes rows exercised “only with no answer” INCONCLUSIVE. For DELETE 204 followed by a HEAD timeout, an answer did arrive, but there is no evidence that the deleted key remains readable. The target distinguishes no-response and timeout outcomes (`crates/validate/src/s3/error.rs:36-47`), and proposal 0017 distinguishes transport availability from oracle disagreement and requires unresolved probes to remain INCONCLUSIVE (`docs/design/proposals/draft/0017-blackbox-validation-tool.md:489-493`, `:520-524`). Revise B′/E to define incomplete multi-observation checks separately from contradictory answers. Require deterministic fixtures for successful single/bulk delete followed by an unavailable HEAD → INCONCLUSIVE, alongside HEAD 200 → fatal oracle.
- [ ] **The PUT criterion omits the existing early-acknowledgement invariant.** `brief.md:38-39` checks 200 plus a strong ETag; the observation contract at `brief.md:150-154` describes response headers and response-body outcomes, without requiring preservation of request-source completion. A premature 200 with a valid ETag and a complete empty response body meets those stated checks even while the upload is unfinished. The target explicitly says a PUT is a receipt only when its source had ended before the response head (`crates/validate/src/s3.rs:161-170`) and rejects premature success at `:235-243`; its regression test asserts this at `crates/validate/tests/s3_client_upload_peers.rs:1207-1217`. Preserve that distinction in the matrix-owned observation and add a pure early-ack fixture with a valid ETag that cannot pass. Deferring client-to-observation mapping to #939 does not specify how this fact survives the boundary.
- [ ] **The claimed integration base is not the supplied target.** `brief.md:90-94` says this run's integration line “already carries #744 and #853,” but the target's HEAD equals local `main` at `4bac538`; both `git merge-base --is-ancestor refs/heads/enhancement/744-validate-payloads-and-size-classes HEAD` and the corresponding `enhancement/853-validate-s3-client-nonconforming-responses` check return 1. The source still defers #853's GET framing work (`crates/validate/src/s3.rs:264-268`). The repository and `main` target resolve, and no prerequisite is formally declared. Correct the ordering/base assertion to match the supplied source, or identify the integration revision that must be supplied before implementation relies on those changes; the current brief cannot substantiate that they are already present.

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
- Iteration delta (if iterating): Auto-iterate (round 1): rebuilding for the implementation-level findings — C4 Verification (red→green) — Accept or require stronger evidence for the declared net-new API exception — 41 tests pass after restoration, but the pre-fix test cannot execute and the coverage gate used a mismatched base; `reviewer-restored-green.log:49`, `reviewer-red.log:3`, `gate-logs/C4-diff-cov.log:10`.; C5 Causal adequacy — Correct the two failed-body classifications — an unresolved PUT currently yields PASS and an unresolved matching conditional GET yields fatal protocol, violating the evidence limit; `target/crates/validate/src/matrix.rs:1046`, `target/crates/validate/src/matrix.rs:1092`, `reviewer-cases.log:6`, `reviewer-cases.log:13`.; T5 Judgment — Add regression coverage for failed PUT and conditional-GET response bodies — the 41-test suite passes while both verdict errors survive; `target/crates/validate/tests/capability_matrix.rs:438`, `target/crates/validate/tests/capability_matrix.rs:539`, `target/crates/validate/tests/capability_matrix.rs:682`, `reviewer-cases.log:28`.; **A torn response passes three required rows.** The module's own invariant (`crates/validate/src/matrix.rs:26-27`) says a body cut off in transport leaves the row "unproved, never passed and never failed". The rubric's *Protocol input* class says the same: torn input is never silently accepted. But the PUT arm (`matrix.rs:1034-1052`) never looks at `r.body`, and neither do HEAD (`:1100-1118`) or DELETE (`:1120-1124`). Probe results on production code: `PUT 200 + strong ETag + Body::Failed` → `Pass`; `HEAD 200 right size/ETag + Body::Failed` → `Pass`; `DELETE 204 + Body::Failed, HEAD 404` → `Pass`. So a run where those answers were torn reports PASS, not INCONCLUSIVE. The T4 batch review flagged the PUT case three times (gate-logs/T4-batch-review.log). The probe confirms it, and the same gap exists on HEAD and DELETE, which T4 did not name. No test feeds `Body::Failed` to these rows (`capability_matrix.rs:682` covers GET only).; **The opposite error on the matching conditional GET.** `matrix.rs:1091-1092`: `304 + Body::Failed` → `Fatal { Protocol, "answered 304 with a body" }`. §7 (`0017:508`) classes a transport failure as `availability` (budgeted), never fatal. The module doc at `:26-27` says "never failed" too. A reset after a 304 head would FAIL the run. It should be `Unsettled`, as the GET, listing and bulk-delete arms already do (`:1136`, `:1195`).; **A rubric grammar class has no test (surviving mutant `matrix.rs:1266`).** The production code is correct: `bytes +2-+5/+16` → `protocol`. But the test does not pin it. I changed `!s.is_empty() && …` to `||` in a copy, and `u64::from_str` then accepts the `+` signs: the ranged GET with `Content-Range: bytes +2-+5/+16` returned `Pass`, and the shipped suite stayed **41/41 green**. The rubric names this exact case ("no `+`/`-` signs via `from_str`"). Fix: add a signed value to the wrong-`Content-Range` list at `capability_matrix.rs:503-509`.; **The bulk-delete probe never checks which key a HEAD answer belongs to (surviving mutant `matrix.rs:1225`).** I changed `k == key` to `k != key` in a copy. Then `keys [a,b]`, a clean `<DeleteResult>`, and follow-ups only for an unrequested key (`[(c,404),(c,404)]`) returned `Pass`. Production correctly returns `Unsettled`. The shipped suite stayed 41/41 green, because every fixture gives both keys the same answer or a symmetric pair. Add a fixture where the probed key differs from the requested ones.; **Duplicate follow-ups for one key are silently dropped.** `matrix.rs:1225` takes the *first* follow-up per key and ignores the rest. Probe on production: follow-ups `[(a,404),(a,200),(b,404)]` → `Pass`, even though a later HEAD shows the deleted key readable (`oracle`, `0017:505`). The other order, `[(a,503),(a,404)]`, gives `Unsettled` even though the retry proved the delete. The type permits this input (`follow_ups: Vec<(String, Answer)>`, `:786`), and 743.3's mapping could produce it after a retry. Silently skipping it hits the rubric's *Absent entries* class. Fix: judge every follow-up for a key, or call duplicates or unrequested keys unsettled.; **More untested branches from C5's 16 missed mutants.** These are real gaps, not cosmetic. Deleting the `Body::Failed` arm for listings (`:1136`) or for bulk delete (`:1195`) turns a transport cut into fatal `protocol`, and the suite stays green. The pre-release character check (`:554`) is never exercised: under the `||`→`&&` mutant, `0.0.0-r_c` parses. Add it to `parse_refuses_malformed_versions` (`capability_matrix.rs:746`). The 304-with-`Bytes(empty)` guard (`:1091`) is untested, yet 743.3's client may well map a bodiless reply that way. The range guard (`:1065`) is never hit, so a `>`→`>=` regression would mark every single-byte range unsettled without failing a test. The Display impls and the unused `Ord for Release` are cosmetic; skip them.; `crates/validate/src/matrix.rs:1045`: A PUT with status 200, a strong ETag and `request_body_ended: true` returns `Pass` without inspecting its response body. Thus `Body::Failed` can produce a run-level PASS despite an incomplete response; nonempty response bytes also pass despite the client's empty-body contract. Check the body before returning success: transport failure should leave the row unsettled, and an unexpected body should be `protocol`. Add both fixtures while preserving the existing early-acknowledgement failure.; `crates/validate/src/matrix.rs:1092`: A matching conditional GET with status 304 and `Body::Failed` falls into “answered 304 with a body,” producing fatal `protocol`. A timeout/reset does not establish that forbidden bytes arrived, so this manufactures FAIL where the observation only supports INCONCLUSIVE. Handle `Body::Failed` as unsettled and retain the fatal result for actual nonempty bytes. A shared empty-body checker could cover this and the PUT case consistently.; `crates/validate/tests/capability_matrix.rs:653`: The bulk-delete fixtures do not distinguish correctly associated probes from probes of other keys. Frozen `gate-logs/C5-mutants.log` confirms that changing `k == key` to `k != key` at `crates/validate/src/matrix.rs:1225` survives. Add a three-key fixture with ordered HEAD results `a:404, b:404, c:200` and require fatal `oracle`: the surviving mutation checks b for a and a for both b and c, falsely passing without examining c's readable result.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_937/review-b. Deferred, not addressed here: 6 finding(s) that need human judgment, held in deferred-findings.json for the human at handover — not build work
- By / date: auto-iterate / 2026-10-19

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
