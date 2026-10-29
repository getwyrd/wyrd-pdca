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
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 86.3% — 585 of 678 instrumentable changed lines executed (floor 80%); 678 of 1480 changed lines were instr
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 158 mutants tested in 7m: 128 caught, 30 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_937/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.40s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #937 adds a pure S3 capability matrix, version selection, and answer/run classification: no new implementation defect was confirmed; verification and the three declared policy decisions still need sign-off.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | NEEDS-HUMAN | Decide when a first prerelease tag freezes its expectations — `0.1.0-rc.1` selects the mutable `pre-0.1` profile although retention starts at the first tag; this affects repeatable validation of tagged builds (`target/crates/validate/tests/capability_matrix.rs:292`, `target/docs/design/proposals/draft/0017-blackbox-validation-tool.md:284`, `reviewer-probes.log:2`). |
| C2 Reproduction (red pre-fix) | NEEDS-HUMAN | Accept the declared new-API exception — stashing production while retaining the test produces an unresolved import, so there is no executable behavioral pre-fix red; all ten required fault injections do execute and fail (`brief.md:138`, `reviewer-red.log:3`, `reviewer-mutations.log:2`). |
| C3 Change | PASS | The four-file change stays within the authorized pure-module slice and proposal amendment; no manifest, gateway, client, or runner behavior is changed (`patch.diff:1`, `target/crates/validate/src/lib.rs:24`, `target/docs/design/proposals/draft/0017-blackbox-validation-tool.md:196`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Require matching full-CI evidence on the intended refreshed base — restored production passes 48 tests, but frozen CI still fails on old ctutils/tokio versions despite the brief's #950/#954 carry-forward; this is a base/evidence caveat, not a matrix defect (`reviewer-restored-green.log:56`, `gate-logs/C4-ci.log:8484`, `gate-logs/C4-ci.log:8566`, `brief.md:235`). |
| C5 Causal adequacy | NEEDS-HUMAN | Decide whether one unanswered retry must keep an otherwise proved row INCONCLUSIVE — the implementation requires every exercise to pass, stricter than criterion E's “exercised only with no answer”; direct execution confirms the distinction (`target/crates/validate/src/matrix.rs:1393`, `brief.md:238`, `reviewer-probes.log:1`). |
| T1 Structure | PASS | The classifier remains independent of transport execution and client error variants, using its own observations plus the explicitly permitted ErrorCode; no clock, mutable global, or backend dependency is introduced (`target/crates/validate/src/matrix.rs:30`, `target/crates/validate/src/matrix.rs:631`, `target/crates/validate/src/lib.rs:1`). |
| T2 Shape | PASS | The patch is 99,441 bytes, below the 100 KB backstop; formatting, spelling, documentation lint/render, and crate Clippy with warnings denied pass independently (`brief.md:166`, `reviewer-scanners.log:1`, `reviewer-clippy.log:222`). |
| T3 Runtime | PASS | Executed production code preserves fatal response-head evidence and leaves torn required answers unproved; all 1,200 unsupported-operation/2xx combinations remain fatal after body failure, with no network or timing dependency (`target/crates/validate/src/matrix.rs:929`, `target/crates/validate/src/matrix.rs:1225`, `reviewer-probes.log:4`). |
| T4 Contribution | PASS | Prior-art checking was confirmed by affected paths, including all 19 closed-unmerged PR file lists; previous torn-body findings are covered, and the frozen batch review reports no blockers; contribution-artifact auditing is separately N/A until publish (`reviewer-prior-art.log:1`, `target/crates/validate/tests/capability_matrix.rs:878`, `gate-logs/T4-batch-review.log:10`, `gate-logs/T4-contribution.log:10`). |
| T5 Judgment | NEEDS-HUMAN | Decide whether an exactly empty listing of an empty prefix proves the listing row — current behavior makes the run INCONCLUSIVE, adding a coverage requirement beyond equality with the expected keys; the brief explicitly reserves this policy decision (`target/crates/validate/src/matrix.rs:1170`, `brief.md:238`, `reviewer-probes.log:3`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept this pure classifier as the foundation for smoke, including the settled early-PUT protocol classification — the evidence exercises production classification, while client-to-observation mapping and a live server scenario remain the explicitly separate #939 slice (`brief.md:125`, `brief.md:151`, `target/crates/validate/src/matrix.rs:1062`). |

The revised implementation addresses the prior implementation findings. Independent probes return `Unsettled` for torn PUT, HEAD, DELETE, and matching conditional-GET answers; a torn bulk-delete document followed by HEAD 200 yields fatal `oracle`, and HEAD 403 yields fatal `protocol` (`reviewer-probes.log:4`, `reviewer-probes.log:8`). The submitted regressions exercise those combinations through the production module (`target/crates/validate/tests/capability_matrix.rs:772`, `target/crates/validate/tests/capability_matrix.rs:878`). The C5 symptom-guard rule does not fire: the new checks judge observations and preserve independent evidence; they do not probe an optional capability to hide a load-time cause.

The independent verification is executable and restored. Production files were stashed with the added test retained, producing compiler exit 101 before any test ran; popping the stash restored all four patch files byte-for-byte (`reviewer-stash.log:16`). Each of the ten fault injections required by the brief was caught by an executing assertion, and the restored complete suite passed 48/48 again (`reviewer-mutations.log:2`, `reviewer-post-mutations-green.log:55`). The extra probe program links the compiled production library rather than a copied classifier. Both declared external dependencies were exercised: real `typos` passed, and the docs renderer fetched Mermaid, rendered 99 pages, and passed its link audit (`reviewer-typos.log:3`, `reviewer-docs-render.log:2`).

The frozen gates support the following conclusions; log-based evidence is distinguished from independent reruns.

| Gate | Disposition | Evidence |
|------|-------------|----------|
| C4-ci | NEEDS-HUMAN | The full captured run completed its other checks but failed both cargo-deny checks on ctutils 0.4.2 and tokio 1.52.3 (`gate-logs/C4-ci.log:8484`, `gate-logs/C4-ci.log:8566`, `gate-logs/C4-ci.log:9356`). The supplied target has those same versions (`target/Cargo.lock:1000`, `target/Cargo.lock:3953`); the patch does not change dependencies. The independent advisory rerun could not acquire the read-only advisory-cache lock (`reviewer-deny.log:1`), so it does not supersede the captured failure. Refresh/reconcile the baseline evidence named in C4 rather than treating this as a matrix implementation defect. |
| C4-verify | NEEDS-HUMAN | The log shows 48 green tests, then an unexecuted pre-fix test due to the missing matrix API, exit 77; the independent stash/restoration reproduces this exact limitation (`gate-logs/C4-verify.log:7`, `gate-logs/C4-verify.log:14`, `gate-logs/C4-verify.log:181`, `reviewer-red.log:3`). This is the C2 sign-off exception above. |
| C4-diff-cov | PASS | Captured instrumentation ran 48 tests and measured 585/678 changed instrumentable lines, 86.3%, above its 80% floor; 802 changed lines were unscored (`gate-logs/C4-diff-cov.log:224`). This measurement is log-based, not an independent coverage rerun. |
| C5-mutants | PASS | The captured campaign reports 158 tested: 128 caught, 30 unviable, none surviving (`gate-logs/C5-mutants.log:10`). The independent ten targeted fault injections supplement, rather than reproduce, that full campaign (`reviewer-mutations.log:1`). |
| T4-batch-review | PASS | Its captured result reports zero blocking, rejected, or noise-dropped findings (`gate-logs/T4-batch-review.log:10`); the separate implementation review above found no additional confirmed defect. |
| T4-contribution | N/A | The PR description is intentionally drafted after Check; the substantive contribution audit must rerun at publish (`gate-logs/T4-contribution.log:10`). There is no missing human approval for this deferred artifact. |
| host-tikv | PASS | The captured command compiles both the TiKV crate and server feature-selection arms and exits successfully; this is compile evidence, not live TiKV operation evidence (`gate-logs/host-tikv.log:2`, `gate-logs/host-tikv.log:7`, `gate-logs/host-tikv.log:209`). No TiKV execution is required by this pure matrix slice. |

The target grounds the patch, with a dependency-baseline caveat. Reverse application checking succeeds, and the cited gateway responses and version derivation exist in the supplied target. In particular, `uploads`, `uploadId`, and `partNumber` remain denied, so today's six multipart rows correctly remain unsupported (`target/crates/gateway-s3/src/lib.rs:410`, `target/crates/gateway-s3/src/lib.rs:1842`). The frozen CI's old dependency versions conflict with the refreshed-base instruction in `brief.md:235`; no patch-applicability or compilation defect is inferred from that discrepancy.

Prior art was checked by file rather than title alone. Live GitHub changed-file lists corroborate the brief's merged-history entries and show #931/#934 now merged; none of the 19 closed-unmerged PRs touches `crates/validate/` or the proposal path. The inspected earlier PRs contain neither new matrix file (`reviewer-prior-art.log:1`). Earlier local rejections are disclosed in the supplied brief, and their torn-body counterexamples now execute successfully. No builder notes, other local checkouts, or commit-signoff inspection were used.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — #937 (743.1, capability matrix)

Short version: I tried hard to break the classifier and could not. Its 48 tests are real
discriminators. I re-ran them green on a scratch copy of `$PDCA_TARGET`. Then I made four
temporary edits to production code, matching the brief's reds (7), (10), (2) and (8). Each one
turned the suite red, and every leg calls `wyrd_validate::matrix` directly, not a copy. C5
reports 0 surviving mutants out of 128 caught. The one gating red is a stale-base environment
fault, not the fix. Two smaller items follow.

## Findings

- NEEDS-HUMAN [human] — **The gating `C4-ci` red comes from a stale base, not from this
  patch.** The worktree is on `93a6f37`, not the `b552ca2` that the iteration-3 carry-forward
  told this round to rebuild on (`brief.md:235`). The target's lockfile still pins
  `ctutils 0.4.2` (`Cargo.lock:1000-1001`) and `tokio 1.52.3` (`Cargo.lock:3953-3954`). Those
  are exactly RUSTSEC-2026-0385 and -0386, the only failures (`gate-logs/C4-ci.log:3865-3872`,
  `:5168-5170`). Every other CI step passed both attempts: fmt, clippy, build, the whole test
  run with `capability_matrix` at 48/48 (`C4-ci.log:3179-3229`), typos, docs lint and render,
  the blackbox and unsafe guards. The gates also measured two different bases: `C4-diff-cov`
  compiled `ctutils v0.4.5` and `tokio v1.54.1` in another worktree (`C4-diff-cov.log:12-13`),
  while `C4-ci` ran on the old one. The patch adds files plus one `lib.rs` line and a draft-doc
  edit, so it does not depend on the base. But the gating row cannot go green until the driver
  rebases the worktree onto current `main` and re-runs it. That is a harness or human action,
  not build work.

- NEEDS-HUMAN [impl] — **The 0017 amendment leaves the proposal contradicting itself.** Amended
  §3 now says every response carries `Server: wyrd/<version>`. It still points the reader to
  *Dependencies* (`docs/design/proposals/draft/0017-blackbox-validation-tool.md:263-265`). That
  section still says "**The gateway advertises no version.** No `Server:` header, nothing on
  the wire … Interim: `--server-version`" (`:873-878`). Before the patch the two passages
  agreed. Now the new sentence cites a paragraph that says the opposite. Fix: in *Dependencies*,
  record that #736 landed through #886, or drop the cross-reference. The brief named only
  `0017:259-261`. If the builder treats `:873-878` as out of scope, decline it with an issue
  reference rather than leaving it silent.

- NEEDS-HUMAN [human] — **(Low; the outcome is FAIL either way.) A 404 to the HEAD or GET of a
  key the run just wrote is classed `protocol`, not `oracle`.** Probe on production:
  `HeadObject` answered `404` gives `Fatal { Protocol, "answered 404; expected 200" }`, and the
  same holds for `GetObject`. The cause is `expect_status` (`crates/validate/src/matrix.rs:972-981`)
  running before the oracle checks (`:1124`, `:1081`). Brief (B′) says HeadObject is "200 with
  the object's size and the PUT's `ETag`, else `oracle`". §7 (`0017:505`) defines `oracle` as
  "model and server disagree", and a written key that reads as absent fits that. But §7's
  `protocol` row ("wrong status") also fits. Only the class label in the report changes, and
  later scenarios that key on the class (#746) will inherit whichever reading is chosen. A
  human should pick one.

## Attempted, could not refute

- **The evidence.** I re-ran `cargo test -p wyrd-validate --test capability_matrix` on a copy:
  48 passed. My own reds, each restored afterwards: removing the `+git.unknown` check
  (`matrix.rs:555-557`) fails `unknown_is_never_read_as_release_0_0_0`. Making a no-answer
  probe `oracle` (`:1189-1191`) fails 4 tests. Treating a 2xx head with a failed body as no
  answer (`:929-935`) fails 1 test. Counting a `supported, not checked` row as unexercised
  (`:1425-1428`) fails 1 test. `C4-verify` UNVERIFIABLE is the brief's pre-declared new-API
  exception (`brief.md:138-150`), not a gap.
- **Content-Range grammar** (`matrix.rs:1314-1329`). These all give `protocol`: a double
  space, `bytes=2-5/16`, a tab before or after, `2 -5`, and every `+` sign form. Leading zeros
  (`02-05`) pass, which RFC 9110's `1*DIGIT` allows.
- **Strong ETag** (`:1302-1310`). DEL (0x7F), a tab, `W/""` and a list `"a", "b"` are refused.
  obs-text (`"é"`) is accepted, as the RFC allows.
- **Versions** (`:505-564`). `0.1.0+git.unknown`, `1.2.3-rc.1+git.unknown` and
  `0.0.0-alpha+git.unknown` are all Unknown. `0.1.0+git.dirty` (a dirty-tag build, see
  `crates/server/src/version/derivation.rs:59-61`) is 0.1.0, which no profile covers. `0.0.0+git.unknown+x` is a parse
  error. `0.0.0+GIT.UNKNOWN` reads as a release, but the derivation only ever emits lowercase.
- **Bulk delete.** A Quiet-mode empty `<DeleteResult>` gives `protocol`. A 404 probe that
  carries a body gives `protocol`. I tried mis-paired, duplicate, unrequested and torn-document
  probes; all are handled (`:1213-1259`).
- **A 2xx carrying an S3 error code passes** PUT, HEAD, GET and DELETE (`:962-970` never looks
  at `error_code` on a success). Not raised: today's client never produces that observation.
  For any 2xx it returns `S3Error::Unreadable` before reading an error document
  (`crates/validate/src/s3.rs:363-368`). 743.3's mapping should keep it that way.
- **A 501 to a delete's follow-up HEAD is unsettled** (`:1198`), while a 501 to a required row
  is `protocol` (`:965`). This matches the brief's "a 5xx → no verdict for the row" rule
  (`brief.md:62`).
- **An exercise the profile does not declare** makes the run INCONCLUSIVE, even a 2xx
  (`:1407-1409`). This cannot happen with `pre-0.1`, which declares all 22 operations. Worth
  remembering when MinIO's profile (#751) declares fewer.
- **`pre-0.1` citations.** I checked each one against `crates/gateway-s3/src/lib.rs` on this
  base: the copy-source guard at `:1865-1871`, the object-path denylist at `:1842-1849`, the
  bucket-path denylist at `:1797-1804`, the `_ =>` 400 arm at `:1809-1815`, the root-path 400
  at `:1819-1826`, the 204 delete at `:2026-2028`, and `UNKNOWN_SERVER_VERSION` at `:121`.
  #508 is not on this base (`uploads`, `uploadId` and `partNumber` are still at `:411-413`), so
  the six multipart rows are rightly `unsupported`.
- I did not re-raise the three open design points that sign-off left for the human
  (`brief.md:235`): strictness on retries, the `0.1.0-rc.1` profile, and an empty-prefix
  listing.

<!-- pdca:leaf-complete -->

### Advisory — code-review

- No new correctness findings in this diff. Response-body checks and bulk-delete probe aggregation preserve fatal evidence while leaving incomplete observations unproved (`crates/validate/src/matrix.rs:999`, `crates/validate/src/matrix.rs:1213`), with regressions covering the previously reported failures (`crates/validate/tests/capability_matrix.rs:772`, `crates/validate/tests/capability_matrix.rs:878`).
- No actionable reuse, simplification, or efficiency findings. The shared body checker and result-folding helper cover the repeated checks consistently (`crates/validate/src/matrix.rs:999`, `crates/validate/src/matrix.rs:1013`).

Frozen gate evidence reports 48 passing matrix tests, 86.3% diff coverage, and 128 caught / 30 unviable mutants with none surviving. Overall CI still fails dependency advisory checks; the declared new-API pre-fix verification remains unverifiable. These are existing gate results, not new diff findings. No target files were changed or tests rerun.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C1 Spec — Decide when a first prerelease tag freezes its expectations — `0.1.0-rc.1` selects the mutable `pre-0.1` profile although retention starts at the first tag; this affects repeatable validation of tagged builds (`target/crates/validate/tests/capability_matrix.rs:292`, `target/docs/design/proposals/draft/0017-blackbox-validation-tool.md:284`, `reviewer-probes.log:2`).
- [ ] C2 Reproduction (red pre-fix) — Accept the declared new-API exception — stashing production while retaining the test produces an unresolved import, so there is no executable behavioral pre-fix red; all ten required fault injections do execute and fail (`brief.md:138`, `reviewer-red.log:3`, `reviewer-mutations.log:2`).
- [ ] C4 Verification (red→green) — Require matching full-CI evidence on the intended refreshed base — restored production passes 48 tests, but frozen CI still fails on old ctutils/tokio versions despite the brief's #950/#954 carry-forward; this is a base/evidence caveat, not a matrix defect (`reviewer-restored-green.log:56`, `gate-logs/C4-ci.log:8484`, `gate-logs/C4-ci.log:8566`, `brief.md:235`).
- [ ] C5 Causal adequacy — Decide whether one unanswered retry must keep an otherwise proved row INCONCLUSIVE — the implementation requires every exercise to pass, stricter than criterion E's “exercised only with no answer”; direct execution confirms the distinction (`target/crates/validate/src/matrix.rs:1393`, `brief.md:238`, `reviewer-probes.log:1`).
- [ ] T5 Judgment — Decide whether an exactly empty listing of an empty prefix proves the listing row — current behavior makes the run INCONCLUSIVE, adding a coverage requirement beyond equality with the expected keys; the brief explicitly reserves this policy decision (`target/crates/validate/src/matrix.rs:1170`, `brief.md:238`, `reviewer-probes.log:3`).
- [ ] Validation — fitness-to-purpose — Accept this pure classifier as the foundation for smoke, including the settled early-PUT protocol classification — the evidence exercises production classification, while client-to-observation mapping and a live server scenario remain the explicitly separate #939 slice (`brief.md:125`, `brief.md:151`, `target/crates/validate/src/matrix.rs:1062`).
- [ ] C4-ci — The full captured run completed its other checks but failed both cargo-deny checks on ctutils 0.4.2 and tokio 1.52.3 (`gate-logs/C4-ci.log:8484`, `gate-logs/C4-ci.log:8566`, `gate-logs/C4-ci.log:9356`). The supplied target has those same versions (`target/Cargo.lock:1000`, `target/Cargo.lock:3953`); the patch does not change dependencies. The independent advisory rerun could not acquire the read-only advisory-cache lock (`reviewer-deny.log:1`), so it does not supersede the captured failure. Refresh/reconcile the baseline evidence named in C4 rather than treating this as a matrix implementation defect.
- [ ] C4-verify — The log shows 48 green tests, then an unexecuted pre-fix test due to the missing matrix API, exit 77; the independent stash/restoration reproduces this exact limitation (`gate-logs/C4-verify.log:7`, `gate-logs/C4-verify.log:14`, `gate-logs/C4-verify.log:181`, `reviewer-red.log:3`). This is the C2 sign-off exception above.
- [ ] **The gating `C4-ci` red comes from a stale base, not from this patch.** The worktree is on `93a6f37`, not the `b552ca2` that the iteration-3 carry-forward told this round to rebuild on (`brief.md:235`). The target's lockfile still pins `ctutils 0.4.2` (`Cargo.lock:1000-1001`) and `tokio 1.52.3` (`Cargo.lock:3953-3954`). Those are exactly RUSTSEC-2026-0385 and -0386, the only failures (`gate-logs/C4-ci.log:3865-3872`, `:5168-5170`). Every other CI step passed both attempts: fmt, clippy, build, the whole test run with `capability_matrix` at 48/48 (`C4-ci.log:3179-3229`), typos, docs lint and render, the blackbox and unsafe guards. The gates also measured two different bases: `C4-diff-cov` compiled `ctutils v0.4.5` and `tokio v1.54.1` in another worktree (`C4-diff-cov.log:12-13`), while `C4-ci` ran on the old one. The patch adds files plus one `lib.rs` line and a draft-doc edit, so it does not depend on the base. But the gating row cannot go green until the driver rebases the worktree onto current `main` and re-runs it. That is a harness or human action, not build work.
- [ ] **The 0017 amendment leaves the proposal contradicting itself.** Amended §3 now says every response carries `Server: wyrd/<version>`. It still points the reader to *Dependencies* (`docs/design/proposals/draft/0017-blackbox-validation-tool.md:263-265`). That section still says "**The gateway advertises no version.** No `Server:` header, nothing on the wire … Interim: `--server-version`" (`:873-878`). Before the patch the two passages agreed. Now the new sentence cites a paragraph that says the opposite. Fix: in *Dependencies*, record that #736 landed through #886, or drop the cross-reference. The brief named only `0017:259-261`. If the builder treats `:873-878` as out of scope, decline it with an issue reference rather than leaving it silent.
- [ ] **(Low; the outcome is FAIL either way.) A 404 to the HEAD or GET of a key the run just wrote is classed `protocol`, not `oracle`.** Probe on production: `HeadObject` answered `404` gives `Fatal { Protocol, "answered 404; expected 200" }`, and the same holds for `GetObject`. The cause is `expect_status` (`crates/validate/src/matrix.rs:972-981`) running before the oracle checks (`:1124`, `:1081`). Brief (B′) says HeadObject is "200 with the object's size and the PUT's `ETag`, else `oracle`". §7 (`0017:505`) defines `oracle` as "model and server disagree", and a written key that reads as absent fits that. But §7's `protocol` row ("wrong status") also fits. Only the class label in the report changes, and later scenarios that key on the class (#746) will inherit whichever reading is chosen. A human should pick one.
- [ ] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [ ] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) —   - cargo deny --all-features --config deny-all-features.toml check advisories: exited with exit status: 1
- [ ] external dependency: cargo deny all-features advisories — the run's stack base `93a6f37` (`pdca-integration/main-r42c5348f203d`) still pins `ctutils 0.4.2` and `tokio 1.52.3` (`Cargo.lock`), so `cargo deny check` and the all-features advisories check fail on the unpatched base (RUSTSEC-2026-0385, RUSTSEC-2026-0386) and C4-ci cannot go green on it; #950/#954 fixed both on `main` `b552ca2` but not on the integration line, and the standing doctor row checks `../wyrd` (`b552ca2`, green), so it will not flag this. This patch touches no manifest, lockfile or deny config; refresh the integration line onto `b552ca2` (or rebase this bundle on `main` with the one-line `lib.rs` change above) and re-run Check
- [ ] builder-declared external dependency `cargo deny all-features advisories` was REFUTED deterministically — its detect cmd (`cd "${WYRD_REPO:-../wyrd}" && { cargo fetch --locked >/dev/null 2>&1 || exit 3; } && cargo deny --frozen --all-features --config deny-all-features.toml check advisories >/dev/null 2>&1`) exited 0, so the dependency is present and full Check ran (#341); weigh the builder's claim against the review before accept
- [ ] **The delete acceptance rule turns missing evidence into an oracle failure.** `brief.md:47-51` requires a successful delete followed by HEAD 404, “else `oracle`”; `brief.md:74-77` only explicitly makes rows exercised “only with no answer” INCONCLUSIVE. For DELETE 204 followed by a HEAD timeout, an answer did arrive, but there is no evidence that the deleted key remains readable. The target distinguishes no-response and timeout outcomes (`crates/validate/src/s3/error.rs:36-47`), and proposal 0017 distinguishes transport availability from oracle disagreement and requires unresolved probes to remain INCONCLUSIVE (`docs/design/proposals/draft/0017-blackbox-validation-tool.md:489-493`, `:520-524`). Revise B′/E to define incomplete multi-observation checks separately from contradictory answers. Require deterministic fixtures for successful single/bulk delete followed by an unavailable HEAD → INCONCLUSIVE, alongside HEAD 200 → fatal oracle.
- [ ] **The PUT criterion omits the existing early-acknowledgement invariant.** `brief.md:38-39` checks 200 plus a strong ETag; the observation contract at `brief.md:150-154` describes response headers and response-body outcomes, without requiring preservation of request-source completion. A premature 200 with a valid ETag and a complete empty response body meets those stated checks even while the upload is unfinished. The target explicitly says a PUT is a receipt only when its source had ended before the response head (`crates/validate/src/s3.rs:161-170`) and rejects premature success at `:235-243`; its regression test asserts this at `crates/validate/tests/s3_client_upload_peers.rs:1207-1217`. Preserve that distinction in the matrix-owned observation and add a pure early-ack fixture with a valid ETag that cannot pass. Deferring client-to-observation mapping to #939 does not specify how this fact survives the boundary.
- [ ] **The claimed integration base is not the supplied target.** `brief.md:90-94` says this run's integration line “already carries #744 and #853,” but the target's HEAD equals local `main` at `4bac538`; both `git merge-base --is-ancestor refs/heads/enhancement/744-validate-payloads-and-size-classes HEAD` and the corresponding `enhancement/853-validate-s3-client-nonconforming-responses` check return 1. The source still defers #853's GET framing work (`crates/validate/src/s3.rs:264-268`). The repository and `main` target resolve, and no prerequisite is formally declared. Correct the ordering/base assertion to match the supplied source, or identify the integration revision that must be supplied before implementation relies on those changes; the current brief cannot substantiate that they are already present.
- [ ] **The run verdict is stricter than brief (E), and this is a contract 743.3 will inherit.** (E) says INCONCLUSIVE for a row "exercised **only** with no answer". `verdict` (`matrix.rs:1341-1366`) and the test `a_row_passes_only_when_every_exercise_of_it_passed` (`capability_matrix.rs:1029`) make the row unproved if *any* exercise got no answer. Probe: `[PUT NoAnswer, PUT 200 + strong ETag]` → INCONCLUSIVE. This is safe for the "never claims more" invariant. But it means one timeout followed by a successful retry ends a smoke run as INCONCLUSIVE, which fits poorly with §7's "must not abort … because one request timed out" (`0017:499-500`). A human should decide whether that strictness is intended before 743.3 builds retries on it.
- [ ] **A pre-release first tag gets no retained profile.** `PRE_0_1` covers everything `below 0.1.0` (`matrix.rs:416`), so `0.1.0-rc.1` and `0.1.0-rc.1+git.3.<sha>` select `pre-0.1` (shown by the test and by my probe). The 0017 amendment says "Retention starts at the first tag" (`0017:287`). If the first tag is `v0.1.0-rc.1`, that tag lands in the profile that is edited in place. Either the range should end at the first tag, or the amendment should say "the first `0.1.0` release".
- [ ] external dependency.
- [ ] **One timed-out retry makes a row INCONCLUSIVE, which the brief's (E) does not say.** The brief: INCONCLUSIVE when a row was "exercised **only** with no answer". The code marks a row unproved if *any* exercise of it got no answer or was unsettled (`matrix.rs:1400-1401`). The test `capability_matrix.rs:1252-1258` pins that stricter rule. Concrete case: a full passing run plus one extra `GetObject` that timed out → INCONCLUSIVE. By the brief's wording that run is PASS. The rustdoc contradicts itself on this exact input: `matrix.rs:1376-1377` says "exercised only with no answer", and `:1377-1378` says "proved only when … every exercise of it passed". The stricter rule never over-claims, and for a timed-out *mutation* §7 (`0017:528-539`) supports it. But for reads, it means one transient timeout in any run that feeds every attempt to `verdict` can never be PASS. That cuts against §7's budgeted `availability` class (`0017:508`, `0017:499-500`). Decide which rule `verdict` keeps (or whether 743.3 feeds it retries at all), then make the rustdoc say one thing.
- [ ] **The diff-coverage gate measured nothing, and the base needs confirming before publishing.** `gate-logs/C4-diff-cov.log`: "patch.diff does not apply on origin/main". The target's base commit is `93a6f37`, which is neither base the brief names (`main` `4bac538`, integration tip `582da99`). This is not evidence against the fix: C5-mutants caught 127 of the 128 viable mutants, and the one survivor is equivalent (next bullet). But the PR's base is not settled. Whether `ErrorCode` (including `MissingInXml`, used at `capability_matrix.rs:337`) is identical on the base the PR finally targets is the brief's claim, and I could not check it here.

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
- Iteration delta (if iterating): Auto-iterate (round 3): rebuilding for the implementation-level findings — C2 Reproduction (red pre-fix) — Accept the declared new-API exception — stashing production while retaining the test produces an unresolved import, so there is no executable behavioral pre-fix red; all ten required fault injections do execute and fail (`brief.md:138`, `reviewer-red.log:3`, `reviewer-mutations.log:2`).; C4 Verification (red→green) — Require matching full-CI evidence on the intended refreshed base — restored production passes 48 tests, but frozen CI still fails on old ctutils/tokio versions despite the brief's #950/#954 carry-forward; this is a base/evidence caveat, not a matrix defect (`reviewer-restored-green.log:56`, `gate-logs/C4-ci.log:8484`, `gate-logs/C4-ci.log:8566`, `brief.md:235`).; C4-ci — The full captured run completed its other checks but failed both cargo-deny checks on ctutils 0.4.2 and tokio 1.52.3 (`gate-logs/C4-ci.log:8484`, `gate-logs/C4-ci.log:8566`, `gate-logs/C4-ci.log:9356`). The supplied target has those same versions (`target/Cargo.lock:1000`, `target/Cargo.lock:3953`); the patch does not change dependencies. The independent advisory rerun could not acquire the read-only advisory-cache lock (`reviewer-deny.log:1`), so it does not supersede the captured failure. Refresh/reconcile the baseline evidence named in C4 rather than treating this as a matrix implementation defect.; C4-verify — The log shows 48 green tests, then an unexecuted pre-fix test due to the missing matrix API, exit 77; the independent stash/restoration reproduces this exact limitation (`gate-logs/C4-verify.log:7`, `gate-logs/C4-verify.log:14`, `gate-logs/C4-verify.log:181`, `reviewer-red.log:3`). This is the C2 sign-off exception above.; **The 0017 amendment leaves the proposal contradicting itself.** Amended §3 now says every response carries `Server: wyrd/<version>`. It still points the reader to *Dependencies* (`docs/design/proposals/draft/0017-blackbox-validation-tool.md:263-265`). That section still says "**The gateway advertises no version.** No `Server:` header, nothing on the wire … Interim: `--server-version`" (`:873-878`). Before the patch the two passages agreed. Now the new sentence cites a paragraph that says the opposite. Fix: in *Dependencies*, record that #736 landed through #886, or drop the cross-reference. The brief named only `0017:259-261`. If the builder treats `:873-878` as out of scope, decline it with an issue reference rather than leaving it silent.; C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — - cargo deny --all-features --config deny-all-features.toml check advisories: exited with exit status: 1. Deferred, not addressed here: 10 finding(s) that need human judgment, held in deferred-findings.json for the human at handover — not build work
- By / date: auto-iterate / 2026-10-19

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
