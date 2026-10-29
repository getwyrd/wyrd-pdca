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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 158 mutants tested in 8m: 1 missed, 127 caught, 30 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_937/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 15.57s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

One reproducible classification defect remains in #937's pure, version-keyed S3 capability matrix: a truncated bulk-delete response hides fatal follow-up evidence.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The operation expectations, version selection, and unresolved-outcome rules are explicit and testable; multipart remains unsupported on this target (`brief.md:18`, `target/crates/gateway-s3/src/lib.rs:410`). |
| C2 Reproduction (red pre-fix) | N/A | This is a new API with no executable pre-fix discriminator: independently stashing production while retaining the test fails compilation on the missing matrix import, as declared (`brief.md:138`, `reviewer-red.log:259`). |
| C3 Change | PASS | The four changed paths stay within the authorized matrix/test/proposal scope, including only the module export in lib.rs; the 96,265-byte patch is below the 100 KB cap (`brief.md:123`, `target/crates/validate/src/lib.rs:24`, `reviewer-patch-state.log:1`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Accept the declared new-API verification exception — restoration passes 46 tests and all ten required fault injections are caught, but no behavioral pre-fix red or diff-coverage measurement was established (`reviewer-restored-green.log:54`, `reviewer-mutations.log:2`, `gate-logs/C4-diff-cov.log:10`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Preserve fatal follow-up evidence after a truncated bulk-delete response — HEAD 200/403 currently yields INCONCLUSIVE instead of the required oracle/protocol failure (`target/crates/validate/src/matrix.rs:1225`, `reviewer-probes.log:16`). |
| T1 Structure | PASS | The classifier owns pure observation types and depends on the existing ErrorCode value, without coupling to client transport errors or adding dependencies (`target/crates/validate/src/matrix.rs:33`, `target/crates/validate/src/matrix.rs:630`). |
| T2 Shape | PASS | Explicit row states and profile ranges fit the agreed boundary between classification and later client/runner integration; no CLI, wire operation, or persisted field is added (`target/crates/validate/src/matrix.rs:139`, `target/crates/validate/src/matrix.rs:224`, `brief.md:127`). |
| T3 Runtime | PASS | The new path is synchronous, uses no external work, clocks, or shared mutable state, and validates range bounds before slicing; runtime fault classification is addressed by C5 (`target/crates/validate/src/matrix.rs:1089`, `target/crates/validate/tests/capability_matrix.rs:876`). |
| T4 Contribution | FAIL | Resolve the one reproduced defect represented by two batch-review entries to satisfy recorded review completion; contribution-artifact audit is separately N/A until publish (`gate-logs/T4-batch-review.log:10`, `target/AGENTS.md:206`, `gate-logs/T4-contribution.log:10`). |
| T5 Judgment | NEEDS-HUMAN [impl] | Add mixed-failure regression coverage — the truncated bulk-delete fixture supplies only 404 probes, so all 46 submitted tests pass while fatal 200/403 probes are discarded (`target/crates/validate/tests/capability_matrix.rs:753`, `reviewer-probes.log:73`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Approve these capability expectations and the early-PUT-acknowledgment protocol classification for the intended validator — pure-function evidence cannot decide operator fitness, and client/runner integration is explicitly a later slice (`brief.md:37`, `brief.md:127`, `target/docs/design/proposals/draft/0017-blackbox-validation-tool.md:193`). |

The implementation finding has one root cause. `check_delete_objects` returns at the failed-body arm before examining any supplied HEAD probes (`target/crates/validate/src/matrix.rs:1225`), even though its probe classifier makes 2xx fatal oracle and other non-404/non-5xx statuses fatal protocol (`target/crates/validate/src/matrix.rs:1187`). The brief requires fatal evidence to outrank an unresolved check. The single-delete arm already combines body and probe results (`target/crates/validate/src/matrix.rs:1144`). The bulk path needs the same preservation of independent evidence, with regression coverage for both fatal classes.

I compiled two additional tests against the actual compiled `wyrd-validate` library. With every other matrix row passing, `DeleteObjects` 200 + `Body::Failed`, followed by `p/a:404, p/b:200`, produces INCONCLUSIVE with no fatal entry. Replacing the last status with 403 produces the same incorrect outcome. The original 46 tests pass alongside these two failures (`reviewer-probes.rs:1261`, `reviewer-probes.rs:1269`, `reviewer-probes.log:73`). Reproduce with `./pdca-reviewer-937-probes --test-threads=1 reviewer_truncated_bulk_delete`. This confirms and deduplicates the batch review's two entries; it is not a second independent implementation defect.

Verification establishes substantial behavior, with the declared limits:

- **Independent PASS:** restored `cargo test --offline --locked -p wyrd-validate --test capability_matrix` under pinned Rust 1.96.0: 46/46. Stash and pop both succeeded, the patch still passes a reverse-application check, and the target retains its original four changed paths (`reviewer-red-green.log:1`, `reviewer-restored-green.log:54`, `reviewer-patch-state.log:1`). The stashed leg failed compilation rather than an assertion; it is not claimed as a behavioral red.
- **Independent PASS:** all ten brief-required mutations compile and fail the submitted tests. The scratch harness uses the actual module source and the real crate's S3 types; its unmodified baseline passes 46 tests (`reviewer-mutations.log:1`). These controlled faults demonstrate discrimination, not a pre-existing baseline API.
- **Independent PASS:** formatting, spelling, documentation lint, and documentation rendering/link audit. Both declared external dependencies, `typos` and the renderer, were exercised successfully; the renderer produced 99 pages (`reviewer-validation-summary.log:2`, `reviewer-docs-render.log:1`). No unmet external dependency is inferred for this pure slice.
- **Frozen C4-ci PASS; host-tikv PASS:** the full CI log records successful lint/build/test/dependency/static checks and ends with all checks passed; the TiKV log records both requested clippy builds finishing. These broad runs are adjudicated from their captured logs, not claimed as independent reruns (`gate-logs/C4-ci.log:4490`, `gate-logs/host-tikv.log:111`, `gate-logs/host-tikv.log:210`). The instance-scoped wrapper scripts are absent from this target as expected.
- **Frozen C4-verify unverifiable; C4-diff-cov unmeasured:** the former matches the independently reproduced missing-API compilation failure; the latter never measured coverage because its origin/main base rejected the patch (`gate-logs/C4-verify.log:180`, `gate-logs/C4-diff-cov.log:10`). The supplied target accepts the patch and compiles. The coverage result is a base-state caveat, not evidence that this patch cannot compile or apply.
- **Frozen C5-mutants FAIL, survivor rejected as non-actionable:** the log shows 127 caught, 30 unviable, and one surviving `parse_numeric` mutation (`gate-logs/C5-mutants.log:13`). I reproduced that survivor (`reviewer-mutations.log:12`). Its changed empty/non-digit precheck is redundant for reachable inputs: empty and nonnumeric strings still fail `u64` parsing; `+` is split off before core parsing, and prerelease numeric tokens are explicitly digit-checked (`target/crates/validate/src/matrix.rs:513`, `target/crates/validate/src/matrix.rs:546`, `target/crates/validate/src/matrix.rs:572`). It does not justify another test-gap finding.
- **Frozen T4-batch-review FAIL:** its two entries describe the single confirmed defect above. **Frozen T4-contribution N/A:** the PR description is absent by design, and the substantive contribution audit must rerun at publish (`gate-logs/T4-contribution.log:10`); no human clearance is requested for that deferred row.

Prior art was checked independently by each affected file path across all 407 returned PRs, including merged and closed-unmerged work; complete file pagination was checked for the two large PRs. No PR touches either new matrix path, and no closed-unmerged PR matches the affected paths (`reviewer-prior-art.log:1`). The earlier rejected attempt and its review findings are recorded in the brief; the repaired failed-body, range-grammar, and probe-association cases are present in this revision and were exercised, rather than re-raised (`brief.md:218`, `target/crates/validate/tests/capability_matrix.rs:546`, `target/crates/validate/tests/capability_matrix.rs:772`, `target/crates/validate/tests/capability_matrix.rs:844`). The C5 capability-probe smell-test does not trigger: the new guards validate observations and bounds, with no capability probe masking a load-time side effect.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — #937 (validate: capability matrix)

How I checked: I compiled an unmodified copy of `crates/validate/src/matrix.rs` and the
shipped `crates/validate/tests/capability_matrix.rs` in a scratch crate (the only outside
symbol, `s3::ErrorCode`, was copied from `crates/validate/src/s3/error.rs:69-76`). Result:
46/46 pass. I then made each of the brief's ten required "demonstrated reds" as a temporary
edit. **All ten turn the shipped suite red**: (1) 2xx to an unsupported row passes → 4 tests
fail; (2) torn 2xx read as no answer → 1; (3) any non-empty ETag accepted → 3; (4) 304 to the
non-matching tag accepted → 1; (5) bulk delete passes on its document alone → 4; (6) selection
ignores the version → 5; (7) `+git.unknown` read as `0.0.0` → 1; (8) a not-checked row counted
as unexercised → 1; (9) an early PUT 200 accepted → 2; (10) a probe with no answer called
`oracle` → 3. The tests call the real module, not a copy of its logic. So the red→green
evidence holds even though C4-verify could not run it (the net-new exception the brief
declared in advance).

## Findings

- NEEDS-HUMAN [impl] — **The gating T4 failure is real and still open: a torn bulk-delete
  document hides a readable key.** `crates/validate/src/matrix.rs:1225` returns `Unsettled` as
  soon as the `<DeleteResult>` body failed, before any follow-up `HEAD` is judged. Concrete
  case (run on production code): `DeleteObjects` keys `[p/a, p/b]`, `200` + `Body::Failed`,
  then `HEAD p/a → 200`, `HEAD p/b → 404` gives `Unsettled`, so the run is INCONCLUSIVE, not
  FAIL. This breaks three things: the brief's per-HEAD rule ("a 2xx → `oracle`", no exception
  for a torn document); the module's own fold rule at `matrix.rs:1010-1011` ("none is skipped
  because another already failed"); and its handling of the single-key case. There, a torn
  `204` followed by `HEAD 200` is `oracle` (`matrix.rs:1144`, pinned by
  `capability_matrix.rs:800-806`). A small fix works: on `Body::Failed`, skip the document
  checks, push the `unsettled` result into `checks`, and still run the probe loop. With that
  edit my probe gives `oracle`, a torn body with all `404`s stays `Unsettled`, and all 46
  shipped tests stay green. Add the torn-body + `HEAD 200` fixture beside
  `capability_matrix.rs:753`. (Counter-argument, for the record: 0017 §7's outcome-resolution
  rule, `0017:535-539`, could read a torn bulk delete as an unresolved mutation where `HEAD 200`
  just records "not deleted". But the brief settled the per-probe classes, and the module
  already rejects that reading for `DeleteObject`.)

- NEEDS-HUMAN [human] — **One timed-out retry makes a row INCONCLUSIVE, which the brief's (E)
  does not say.** The brief: INCONCLUSIVE when a row was "exercised **only** with no answer".
  The code marks a row unproved if *any* exercise of it got no answer or was unsettled
  (`matrix.rs:1400-1401`). The test `capability_matrix.rs:1252-1258` pins that stricter rule.
  Concrete case: a full passing run plus one extra `GetObject` that timed out → INCONCLUSIVE.
  By the brief's wording that run is PASS. The rustdoc contradicts itself on this exact input:
  `matrix.rs:1376-1377` says "exercised only with no answer", and `:1377-1378` says "proved
  only when … every exercise of it passed". The stricter rule never over-claims, and for a
  timed-out *mutation* §7 (`0017:528-539`) supports it. But for reads, it means one transient
  timeout in any run that feeds every attempt to `verdict` can never be PASS. That cuts
  against §7's budgeted `availability` class (`0017:508`, `0017:499-500`). Decide which rule
  `verdict` keeps (or whether 743.3 feeds it retries at all), then make the rustdoc say one
  thing.

- NEEDS-HUMAN [human] — **The diff-coverage gate measured nothing, and the base needs
  confirming before publishing.** `gate-logs/C4-diff-cov.log`: "patch.diff does not apply on
  origin/main". The target's base commit is `93a6f37`, which is neither base the brief names
  (`main` `4bac538`, integration tip `582da99`). This is not evidence against the fix:
  C5-mutants caught 127 of the 128 viable mutants, and the one survivor is equivalent (next
  bullet). But the PR's base is not settled. Whether `ErrorCode` (including `MissingInXml`,
  used at `capability_matrix.rs:337`) is identical on the base the PR finally targets is the
  brief's claim, and I could not check it here.

- C5's one surviving mutant is equivalent, not a test gap. `matrix.rs:572`, first `||` → `&&`
  turns the guard into "leading zero only". Then a non-digit string reaches `u64::from_str`,
  which would accept a leading `+`. But `parse_numeric` only ever sees text cut before the
  first `+` and the first `-` (`matrix.rs:513-528`), or text already checked to be all digits
  (`:546`). So no input can tell the two apart. No action needed.

- Class choice only, the outcome is unaffected: a `GET` or `HEAD` answering `404` for a key the
  run just wrote is classed `protocol` ("answered 404; expected 200",
  `matrix.rs:1080`, `:1123`). The brief says `HeadObject` "… else `oracle`", and a missing
  written key is the "model and server disagree" case of `0017:505`. Both classes are fatal,
  so the run still FAILs. Only the report's label differs. Worth a one-line decision, not a
  rebuild on its own.

## Attempted and could not refute

- **ETag grammar** (`matrix.rs:1284-1292`): `*`, unquoted, `W/"…"`, empty, a lone `"`, an
  inner `"`, DEL (`0x7F`), spaces and leading whitespace are all refused; `""` and obs-text
  are accepted, which matches RFC 9110 §8.8.3.
- **Content-Range** (`:1296-1311`): `+`/`-` signs, `*/size`, a missing size, extra spaces are
  all `protocol`; unit is case-insensitive; leading zeros are accepted (allowed by `1*DIGIT`).
- **Versions** (`:505-577`): `+git.unknown[.dirty]` → unknown; `+git.unknownx` → a release;
  the derivation's other real shapes (`0.1.0+git.dirty`, `0.1.0-rc.1+git.3.<sha>`, checked
  against `crates/server/src/version/derivation.rs:40-67`) select correctly; leading zeros,
  empty identifiers, `v`/`wyrd/` prefixes are refused.
- **Unsupported rows**: any 2xx, including a torn one, is `unexpected-support`; a 5xx other
  than the declared status is `protocol` (as the brief decided); no head → no verdict.
- **Bulk-delete probes**: wrong-key pairing, a later `HEAD 200` after a `404`, a `503` then
  `404` retry, an unrequested probed key, duplicate requested keys: all behave as the brief
  and the prior round asked.
- **Fold**: two fatal exercises of one row are both listed; a fatal row outranks an unproved
  one; a not-checked row is listed and never passes.
- **Citations**: every `pre-0.1` row's `crates/gateway-s3/src/lib.rs` line matches today's
  code (spot-checked `:1797-1826`, `:1842-1849`, `:1865-1871`, `:2008`, `:2017`, `:2025-2028`,
  `:2293-2300`, `:2713-2719`, `:2785-2803`, `:2995`, `:823`, `:855`, `:868`).
- Patch size 96,265 bytes, under the 100 KB backstop.

<!-- pdca:leaf-complete -->

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/validate/src/matrix.rs:1225`: A failed bulk-delete response body skips every follow-up HEAD, hiding fatal evidence. Direct execution of the target source with requested keys `[a, b]`, `200 + Body::Failed`, and probes `a → 404, b → 200` returns `Unsettled` and a run verdict of INCONCLUSIVE; the brief requires fatal `oracle` for the readable key. The equivalent single-delete case correctly returns fatal `oracle` through `worst` at `crates/validate/src/matrix.rs:1144`. Evaluate the bulk document and probes independently and combine their results with that helper. Add the mixed failed-body/readable-probe regression: `crates/validate/tests/capability_matrix.rs:753` currently covers a failed bulk body only with both probes returning 404.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Accept the declared new-API verification exception — restoration passes 46 tests and all ten required fault injections are caught, but no behavioral pre-fix red or diff-coverage measurement was established (`reviewer-restored-green.log:54`, `reviewer-mutations.log:2`, `gate-logs/C4-diff-cov.log:10`).
- [ ] C5 Causal adequacy — Preserve fatal follow-up evidence after a truncated bulk-delete response — HEAD 200/403 currently yields INCONCLUSIVE instead of the required oracle/protocol failure (`target/crates/validate/src/matrix.rs:1225`, `reviewer-probes.log:16`).
- [ ] T5 Judgment — Add mixed-failure regression coverage — the truncated bulk-delete fixture supplies only 404 probes, so all 46 submitted tests pass while fatal 200/403 probes are discarded (`target/crates/validate/tests/capability_matrix.rs:753`, `reviewer-probes.log:73`).
- [ ] Validation — fitness-to-purpose — Approve these capability expectations and the early-PUT-acknowledgment protocol classification for the intended validator — pure-function evidence cannot decide operator fitness, and client/runner integration is explicitly a later slice (`brief.md:37`, `brief.md:127`, `target/docs/design/proposals/draft/0017-blackbox-validation-tool.md:193`).
- [ ] **The gating T4 failure is real and still open: a torn bulk-delete document hides a readable key.** `crates/validate/src/matrix.rs:1225` returns `Unsettled` as soon as the `<DeleteResult>` body failed, before any follow-up `HEAD` is judged. Concrete case (run on production code): `DeleteObjects` keys `[p/a, p/b]`, `200` + `Body::Failed`, then `HEAD p/a → 200`, `HEAD p/b → 404` gives `Unsettled`, so the run is INCONCLUSIVE, not FAIL. This breaks three things: the brief's per-HEAD rule ("a 2xx → `oracle`", no exception for a torn document); the module's own fold rule at `matrix.rs:1010-1011` ("none is skipped because another already failed"); and its handling of the single-key case. There, a torn `204` followed by `HEAD 200` is `oracle` (`matrix.rs:1144`, pinned by `capability_matrix.rs:800-806`). A small fix works: on `Body::Failed`, skip the document checks, push the `unsettled` result into `checks`, and still run the probe loop. With that edit my probe gives `oracle`, a torn body with all `404`s stays `Unsettled`, and all 46 shipped tests stay green. Add the torn-body + `HEAD 200` fixture beside `capability_matrix.rs:753`. (Counter-argument, for the record: 0017 §7's outcome-resolution rule, `0017:535-539`, could read a torn bulk delete as an unresolved mutation where `HEAD 200` just records "not deleted". But the brief settled the per-probe classes, and the module already rejects that reading for `DeleteObject`.)
- [ ] **One timed-out retry makes a row INCONCLUSIVE, which the brief's (E) does not say.** The brief: INCONCLUSIVE when a row was "exercised **only** with no answer". The code marks a row unproved if *any* exercise of it got no answer or was unsettled (`matrix.rs:1400-1401`). The test `capability_matrix.rs:1252-1258` pins that stricter rule. Concrete case: a full passing run plus one extra `GetObject` that timed out → INCONCLUSIVE. By the brief's wording that run is PASS. The rustdoc contradicts itself on this exact input: `matrix.rs:1376-1377` says "exercised only with no answer", and `:1377-1378` says "proved only when … every exercise of it passed". The stricter rule never over-claims, and for a timed-out *mutation* §7 (`0017:528-539`) supports it. But for reads, it means one transient timeout in any run that feeds every attempt to `verdict` can never be PASS. That cuts against §7's budgeted `availability` class (`0017:508`, `0017:499-500`). Decide which rule `verdict` keeps (or whether 743.3 feeds it retries at all), then make the rustdoc say one thing.
- [ ] **The diff-coverage gate measured nothing, and the base needs confirming before publishing.** `gate-logs/C4-diff-cov.log`: "patch.diff does not apply on origin/main". The target's base commit is `93a6f37`, which is neither base the brief names (`main` `4bac538`, integration tip `582da99`). This is not evidence against the fix: C5-mutants caught 127 of the 128 viable mutants, and the one survivor is equivalent (next bullet). But the PR's base is not settled. Whether `ErrorCode` (including `MissingInXml`, used at `capability_matrix.rs:337`) is identical on the base the PR finally targets is the brief's claim, and I could not check it here.
- [ ] `crates/validate/src/matrix.rs:1225`: A failed bulk-delete response body skips every follow-up HEAD, hiding fatal evidence. Direct execution of the target source with requested keys `[a, b]`, `200 + Body::Failed`, and probes `a → 404, b → 200` returns `Unsettled` and a run verdict of INCONCLUSIVE; the brief requires fatal `oracle` for the readable key. The equivalent single-delete case correctly returns fatal `oracle` through `worst` at `crates/validate/src/matrix.rs:1144`. Evaluate the bulk document and probes independently and combine their results with that helper. Add the mixed failed-body/readable-probe regression: `crates/validate/tests/capability_matrix.rs:753` currently covers a failed bulk body only with both probes returning 404.
- [ ] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_937/review-b
- [ ] **The delete acceptance rule turns missing evidence into an oracle failure.** `brief.md:47-51` requires a successful delete followed by HEAD 404, “else `oracle`”; `brief.md:74-77` only explicitly makes rows exercised “only with no answer” INCONCLUSIVE. For DELETE 204 followed by a HEAD timeout, an answer did arrive, but there is no evidence that the deleted key remains readable. The target distinguishes no-response and timeout outcomes (`crates/validate/src/s3/error.rs:36-47`), and proposal 0017 distinguishes transport availability from oracle disagreement and requires unresolved probes to remain INCONCLUSIVE (`docs/design/proposals/draft/0017-blackbox-validation-tool.md:489-493`, `:520-524`). Revise B′/E to define incomplete multi-observation checks separately from contradictory answers. Require deterministic fixtures for successful single/bulk delete followed by an unavailable HEAD → INCONCLUSIVE, alongside HEAD 200 → fatal oracle.
- [ ] **The PUT criterion omits the existing early-acknowledgement invariant.** `brief.md:38-39` checks 200 plus a strong ETag; the observation contract at `brief.md:150-154` describes response headers and response-body outcomes, without requiring preservation of request-source completion. A premature 200 with a valid ETag and a complete empty response body meets those stated checks even while the upload is unfinished. The target explicitly says a PUT is a receipt only when its source had ended before the response head (`crates/validate/src/s3.rs:161-170`) and rejects premature success at `:235-243`; its regression test asserts this at `crates/validate/tests/s3_client_upload_peers.rs:1207-1217`. Preserve that distinction in the matrix-owned observation and add a pure early-ack fixture with a valid ETag that cannot pass. Deferring client-to-observation mapping to #939 does not specify how this fact survives the boundary.
- [ ] **The claimed integration base is not the supplied target.** `brief.md:90-94` says this run's integration line “already carries #744 and #853,” but the target's HEAD equals local `main` at `4bac538`; both `git merge-base --is-ancestor refs/heads/enhancement/744-validate-payloads-and-size-classes HEAD` and the corresponding `enhancement/853-validate-s3-client-nonconforming-responses` check return 1. The source still defers #853's GET framing work (`crates/validate/src/s3.rs:264-268`). The repository and `main` target resolve, and no prerequisite is formally declared. Correct the ordering/base assertion to match the supplied source, or identify the integration revision that must be supplied before implementation relies on those changes; the current brief cannot substantiate that they are already present.
- [ ] **The run verdict is stricter than brief (E), and this is a contract 743.3 will inherit.** (E) says INCONCLUSIVE for a row "exercised **only** with no answer". `verdict` (`matrix.rs:1341-1366`) and the test `a_row_passes_only_when_every_exercise_of_it_passed` (`capability_matrix.rs:1029`) make the row unproved if *any* exercise got no answer. Probe: `[PUT NoAnswer, PUT 200 + strong ETag]` → INCONCLUSIVE. This is safe for the "never claims more" invariant. But it means one timeout followed by a successful retry ends a smoke run as INCONCLUSIVE, which fits poorly with §7's "must not abort … because one request timed out" (`0017:499-500`). A human should decide whether that strictness is intended before 743.3 builds retries on it.
- [ ] **A pre-release first tag gets no retained profile.** `PRE_0_1` covers everything `below 0.1.0` (`matrix.rs:416`), so `0.1.0-rc.1` and `0.1.0-rc.1+git.3.<sha>` select `pre-0.1` (shown by the test and by my probe). The 0017 amendment says "Retention starts at the first tag" (`0017:287`). If the first tag is `v0.1.0-rc.1`, that tag lands in the profile that is edited in place. Either the range should end at the first tag, or the amendment should say "the first `0.1.0` release".
- [ ] external dependency.

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
- Iteration delta (if iterating): Auto-iterate (round 2): rebuilding for the implementation-level findings — C4 Verification (red→green) — Accept the declared new-API verification exception — restoration passes 46 tests and all ten required fault injections are caught, but no behavioral pre-fix red or diff-coverage measurement was established (`reviewer-restored-green.log:54`, `reviewer-mutations.log:2`, `gate-logs/C4-diff-cov.log:10`).; C5 Causal adequacy — Preserve fatal follow-up evidence after a truncated bulk-delete response — HEAD 200/403 currently yields INCONCLUSIVE instead of the required oracle/protocol failure (`target/crates/validate/src/matrix.rs:1225`, `reviewer-probes.log:16`).; T5 Judgment — Add mixed-failure regression coverage — the truncated bulk-delete fixture supplies only 404 probes, so all 46 submitted tests pass while fatal 200/403 probes are discarded (`target/crates/validate/tests/capability_matrix.rs:753`, `reviewer-probes.log:73`).; **The gating T4 failure is real and still open: a torn bulk-delete document hides a readable key.** `crates/validate/src/matrix.rs:1225` returns `Unsettled` as soon as the `<DeleteResult>` body failed, before any follow-up `HEAD` is judged. Concrete case (run on production code): `DeleteObjects` keys `[p/a, p/b]`, `200` + `Body::Failed`, then `HEAD p/a → 200`, `HEAD p/b → 404` gives `Unsettled`, so the run is INCONCLUSIVE, not FAIL. This breaks three things: the brief's per-HEAD rule ("a 2xx → `oracle`", no exception for a torn document); the module's own fold rule at `matrix.rs:1010-1011` ("none is skipped because another already failed"); and its handling of the single-key case. There, a torn `204` followed by `HEAD 200` is `oracle` (`matrix.rs:1144`, pinned by `capability_matrix.rs:800-806`). A small fix works: on `Body::Failed`, skip the document checks, push the `unsettled` result into `checks`, and still run the probe loop. With that edit my probe gives `oracle`, a torn body with all `404`s stays `Unsettled`, and all 46 shipped tests stay green. Add the torn-body + `HEAD 200` fixture beside `capability_matrix.rs:753`. (Counter-argument, for the record: 0017 §7's outcome-resolution rule, `0017:535-539`, could read a torn bulk delete as an unresolved mutation where `HEAD 200` just records "not deleted". But the brief settled the per-probe classes, and the module already rejects that reading for `DeleteObject`.); `crates/validate/src/matrix.rs:1225`: A failed bulk-delete response body skips every follow-up HEAD, hiding fatal evidence. Direct execution of the target source with requested keys `[a, b]`, `200 + Body::Failed`, and probes `a → 404, b → 200` returns `Unsettled` and a run verdict of INCONCLUSIVE; the brief requires fatal `oracle` for the readable key. The equivalent single-delete case correctly returns fatal `oracle` through `worst` at `crates/validate/src/matrix.rs:1144`. Evaluate the bulk document and probes independently and combine their results with that helper. Add the mixed failed-body/readable-probe regression: `crates/validate/tests/capability_matrix.rs:753` currently covers a failed bulk body only with both probes returning 404.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_937/review-b. Deferred, not addressed here: 5 finding(s) that need human judgment, held in deferred-findings.json for the human at handover — not build work
- By / date: auto-iterate / 2026-10-19

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
