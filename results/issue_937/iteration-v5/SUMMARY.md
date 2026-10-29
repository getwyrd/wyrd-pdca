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
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.88s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #937: add a pure, version-selected S3 capability matrix and evidence-limited row/run verdicts; no new implementation defect found, with verification and fitness decisions remaining below.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The scoped contract makes expected support, refusal, unknown versions, unresolved deletes, and fatal evidence distinguishable without a network dependency; `brief.md:17`, `brief.md:92`, `target/docs/design/proposals/draft/0017-blackbox-validation-tool.md:497`. |
| C2 Reproduction (red pre-fix) | N/A | This is a declared new API: stashing production while retaining the submitted test produces E0432 before any assertion executes, so it establishes API absence rather than a behavioral red; `brief.md:138`, `reviewer-red.log:260`. |
| C3 Change | PASS | The four-file patch stays within the matrix, its export/tests, and draft-proposal currency; gateway citations match the supplied target, including the still-denied multipart operations; `target/crates/validate/src/lib.rs:24`, `target/crates/gateway-s3/src/lib.rs:410`, `target/docs/design/proposals/draft/0017-blackbox-validation-tool.md:284`, `reviewer-checks.log:7`. |
| C4 Verification (red→green) | NEEDS-HUMAN | Accept or require stronger evidence for the declared new-API exception, and assign resolution of the inherited advisory failures — restoration passes 48 tests and all ten injected faults are caught, but there is no executable pre-fix red and full CI remains failed; `reviewer-restored-green.log:56`, `reviewer-mutations.log:3`, `gate-logs/C4-ci.log:9356`. |
| C5 Causal adequacy | PASS | Fatal evidence survives incomplete responses while unresolved observations cannot manufacture PASS; the previous bulk-delete failure is corrected at the check-combination boundary, with no capability-probe workaround; `target/crates/validate/src/matrix.rs:1225`, `target/crates/validate/src/matrix.rs:1258`, `target/crates/validate/tests/capability_matrix.rs:878`, `reviewer-mutations.log:3`. |
| T1 Structure | PASS | Scenarios can supply observations without coupling classification to client failures, runtime services, or another Wyrd backend; only the permitted existing ErrorCode value crosses the module boundary; `target/crates/validate/src/matrix.rs:30`, `target/crates/validate/src/matrix.rs:633`, `target/crates/validate/src/matrix.rs:737`. |
| T2 Shape | PASS | Version and range tokens are checked explicitly, unsupported checks fail closed, and the patch passes the repository formatter and focused Clippy with warnings denied; `target/crates/validate/src/matrix.rs:547`, `target/crates/validate/src/matrix.rs:1321`, `reviewer-checks.log:1`, `reviewer-checks.log:5`. |
| T3 Runtime | PASS | The changed execution path is synchronous and pure, introduces no clock or task lifecycle, and checks range bounds before indexing; the submitted cases execute successfully on the pinned toolchain; `target/crates/validate/src/matrix.rs:1091`, `target/crates/validate/src/matrix.rs:1109`, `reviewer-restored-green.log:56`. |
| T4 Contribution | N/A | The contribution-artifact audit is deferred by design because the PR description is drafted after Check; its substantive verdict is owed at publish, not to a human waiver here; `gate-logs/T4-contribution.log:10`. |
| T5 Judgment | PASS | The tests distinguish the ten specified incorrect implementations, cover prior review failures, and path-based history plus all 19 closed-unmerged PR file lists reveal no conflicting rejected work; `reviewer-mutations.log:2`, `target/crates/validate/tests/capability_matrix.rs:772`, `target/crates/validate/tests/capability_matrix.rs:844`, `reviewer-prior-art.log:1`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Approve the pre-0.1 expectations and early-PUT protocol classification as the contract for smoke #939 — this slice proves pure decisions, while live observations and binary integration belong to the subsequent slice; `brief.md:40`, `brief.md:151`, `target/docs/design/proposals/draft/0017-blackbox-validation-tool.md:284`. |

No builder-fixable finding remains from this review. The evidence supports the classifier's scoped behavior, the test suite detects the required faults, and the unresolved full-CI result belongs to unchanged dependencies. This advisory review does not override the deterministic gate's failure.

The independent check used the disposable target's real Cargo build. Production changes were stashed with the submitted integration test retained, then restored byte-for-byte: the pre-fix compilation failed on the absent matrix API, and the restored suite passed all 48 tests (`reviewer-red.log:260`, `reviewer-restored-green.log:56`). The ten additional fault injections used separate copies of the exact matrix source, the real compiled ErrorCode type, and the unchanged submitted tests; the copied baseline passed 48 tests and each injected case failed an executed assertion (`reviewer-mutations.log:1`). These fault injections supplement, rather than establish, a historical red→green.

The frozen gate evidence has these limits and outcomes:

| Gate | Adjudication |
|------|--------------|
| C4-ci | FAIL remains: both attempts end with the normal and all-features advisory audits failing. The log identifies ctutils 0.4.2 / RUSTSEC-2026-0385 and tokio 1.52.3 / RUSTSEC-2026-0386 (`gate-logs/C4-ci.log:8484`, `gate-logs/C4-ci.log:8566`, `gate-logs/C4-ci.log:9356`). Neither manifests nor lockfile change; the supplied target contains those versions at `target/Cargo.lock:1001` and `target/Cargo.lock:3954`. This is an inherited repository gate failure, not a newly introduced matrix defect. |
| C4-verify | UNVERIFIABLE is accurate: its green leg passes 48 tests, but the reverted production API cannot compile the test; independently reproduced (`gate-logs/C4-verify.log:10`, `gate-logs/C4-verify.log:180`, `reviewer-red.log:260`). |
| C4-diff-cov | The frozen run reports 585/678 instrumentable changed lines covered, 86.3%, above its 80% floor; 802 other changed lines are unscored, not evidence of execution (`gate-logs/C4-diff-cov.log:188`, `gate-logs/C4-diff-cov.log:187`). |
| C5-mutants | The frozen run reports 128 caught and 30 unviable out of 158, with no surviving viable mutant; it does not prove all possible bugs absent (`gate-logs/C5-mutants.log:10`). The independent ten required injections also failed their designated tests (`reviewer-mutations.log:3`). |
| T4-batch-review | The log reports zero blocking, rejected, or dropped findings; it contains no individual review text, so that tally is the extent of its evidence (`gate-logs/T4-batch-review.log:10`). |
| T4-contribution | N/A: substantive contribution checks re-run at publish when their artifacts exist (`gate-logs/T4-contribution.log:10`). |
| host-tikv | The frozen output shows successful Clippy compilation of the metadata adapter and server feature selections. It does not claim a live TiKV integration run (`gate-logs/host-tikv.log:2`, `gate-logs/host-tikv.log:209`). |

Independent formatting, focused Clippy, spelling, documentation lint, rendering of 99 pages, and link audit passed (`reviewer-checks.log:1`, `reviewer-docs-render.log:2`). Thus both named external dependencies, typos and the documentation renderer, were actually exercised. The local cargo-deny rerun could not acquire an exclusive lock on the read-only advisory database (`reviewer-deny.log:1`); that local host limitation does not explain away the substantive advisories in the frozen CI output. The instance-scoped gate wrappers were assessed from their supplied logs; no log was missing.

Prior art was checked by each affected path against default-branch history and by changed filename across all 19 closed-unmerged PRs: the new matrix and test have no prior merged history, and no closed-unmerged PR touches the affected paths (`reviewer-prior-art.log:2`, `reviewer-prior-art.log:18`, `reviewer-prior-art.log:38`). The target matches the submitted patch, with no stale-target caveat (`reviewer-checks.log:7`). No INTEGRATION.md was supplied or found in the target; the supplied standing rubric and target AGENTS.md were applied (`reviewer-checks.log:9`).

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — #937 (743.1, capability matrix)

Re-ran the proof on a scratch copy of `$PDCA_TARGET`: `cargo test -p wyrd-validate --test capability_matrix` → 48/48 green. The test imports the production `wyrd_validate::matrix` (`crates/validate/tests/capability_matrix.rs:7-13`), not a copy. The missing pre-fix red is the declared net-new exception (`brief.md:138-150`; `gate-logs/C4-verify.log` exit 77 on `E0432 unresolved import`), not a defect.

I then fed 10 edge cases to the production code. It gets all 10 right. But for 9 of them, deleting the guard that handles the case leaves the shipped suite **48/48 green**. C5's "158 mutants: 128 caught, 0 missed" (`check-gates.json:74`) does not cover these, because cargo-mutants never deletes an `if` block or an arm of a match that has no wildcard. The C4-diff-cov MISS list (`gate-logs/C4-diff-cov.log`: 943, 1106, 1159, 1178-1180, 1219, 1274, 1292-1294, 1317, 1408-1409) points at the same lines. None of this is wrong behavior today. It is a test suite that would not catch these guards being removed.

- NEEDS-HUMAN [impl] — **A required row can be passed without any check, and no test stops that regression.** `crates/validate/src/matrix.rs:1178-1180` makes `Exercise::Other` on a `required` row unsettled. That arm is the only thing stopping a generic exercise from passing a required row. I replaced it with `Exercise::Other { .. } => Ok(())`. The suite stayed 48/48 green, and `Other { operation: PutObject, answer: 200 + strong ETag }` then classified as `Pass` against `PRE_0_1` with no status, ETag, early-answer or body check. Under that change the arm never reads the answer, so any answer passes. 743.3 picks which `Exercise` variant to build, so a mapping slip there would quietly bypass every (B′) check. Fix: add a fixture asserting `classify(row(PutObject), &Exercise::Other { operation: PutObject, answer: <good 200> })` is `Unsettled`, plus a verdict-level case in which that run is INCONCLUSIVE.
- NEEDS-HUMAN [impl] — **Two of the bulk-delete pass conditions have no test.** (1) `matrix.rs:1218-1220` (the empty-keys guard): with it removed, `DeleteObjects { keys: [], 200 + empty <DeleteResult>, follow_ups: [] }` returns `Pass`, which is a vacuous pass (the rubric's *Absent entries* class: "a count-based assertion that can pass while the property fails"). (2) `matrix.rs:1291-1295` (`<Deleted>` must name every requested key, as brief (B′) says at `brief.md:55`): with it removed, keys `[p/a, p/b]`, a `<Deleted>` naming only `p/a`, no `<Error>` row, and both `HEAD`s 404 returns `Pass`. In both cases the suite stays 48/48 green. Every bulk fixture either names all keys under `<Deleted>` or carries an `<Error>` row (`capability_matrix.rs:91-96`, `:110-123`, `:683-695`). Fix: add those two fixtures.
- NEEDS-HUMAN [impl] — **Six more guards can be removed with the suite green. Each has a concrete case:** `matrix.rs:942-946` (unsupported row, declared status, then a torn body → unsettled): removed, `CopyObject` 501 + `Body::Failed` becomes fatal `protocol`. That is the "torn answer made fatal" class iteration 1 fixed for the 304 row, now unguarded on every unsupported row. `matrix.rs:1316-1318` (the unit must be `bytes`): removed, `Content-Range: items 2-5/16` passes. The rubric's *Grammar strictness* class names units, and `capability_matrix.rs:566-573` only checks that `Bytes` is accepted, never that another unit is refused. `matrix.rs:1106` (206 with no `Content-Range` → `protocol`): changed to `None => {}`, a 206 with the right bytes and no `Content-Range` passes. `matrix.rs:1159` (listing 200 without a listing document) and `:1274` (bulk-delete 200 without a `<DeleteResult>`): changed to `Ok`, both pass. `matrix.rs:1408` (an exercise the profile declares no row for → `Undeclared`): removed, the run goes from INCONCLUSIVE to PASS. Fix: one fixture per branch. I wrote all ten cases as a probe file. Production passes every one, so they are pure additions with no code change.
- NEEDS-HUMAN [human] — **The gating C4-ci row is red for the same reason as iteration 4, and the base is still not the one the human asked for.** `gate-logs/C4-ci.log:3865-3872` and `:3947-3954` show cargo-deny failing on RUSTSEC-2026-0385 (ctutils) and RUSTSEC-2026-0386 (tokio). The target is built on `93a6f37` (`git log -1`: "pre-fix base 93a6f37…"), whose `Cargo.lock` still pins ctutils 0.4.2 (`target/Cargo.lock:1000-1001`) and tokio 1.52.3 (`:3953-3954`). The iteration-3 sign-off said to rebuild on `b552ca2`, which carries #950 and #954 (`brief.md:235`). Iteration 4 asked for "matching full-CI evidence on the intended refreshed base" (`brief.md:243`). Neither happened. The patch touches no Cargo file, so the failure is not the matrix's fault, but the full-CI evidence the human asked for still does not exist. The fix is choosing the base, which the builder cannot do.
- `matrix.rs:23-28` (no human action needed). The module's invariant says a 5xx "neither passes nor fails" a check. But `classify_unsupported` (`matrix.rs:936-941`) makes any undeclared status fatal, so `CopyObject` answered `503 SlowDown` is `Fatal { Protocol, "answered 503; declared 501 NotImplemented" }` (my probe; the shipped test pins the same at `capability_matrix.rs:334`). That matches brief (B) ("a 5xx included", `brief.md:34`), and the gateway emits no 503 today (no `SlowDown` or `SERVICE_UNAVAILABLE` in `crates/gateway-s3/src` or `crates/server/src`), so this is not a refutation. The doc sentence should say the 5xx rule applies to required rows and probes only, so nobody later "fixes" the code to match the doc.
- Things I tried to refute and could not:
  - **Gateway citations** are correct on this base: `lib.rs:1766` (bulk `?delete`), `:1797` (bucket denylist), `:1806-1815` (bucket GET, else 400), `:1819-1826` (root path 400), `:1842` (object denylist), `:1865` (copy-source 501), `:2715`/`:2791` (206/304), `:3401-3411` (PUT 200, `content-length: 0`, empty body, so the `expect_no_body` on PUT matches the gateway).
  - **#508 has not landed**: `uploads`, `uploadId` and `partNumber` are still in the denylist at `:410-413`, so the six multipart rows correctly stay `unsupported`.
  - **Version parsing** handles every shape `normalize_describe` emits (`crates/server/src/version/derivation.rs:41-69`), including `0.1.0+git.dirty`, `0.1.0-rc.1+git.3.<sha>` and `0.0.0+git.unknown[.dirty]`.
  - **Bulk-delete probes**: key pairing, duplicate probes, retries, unrequested keys, and a torn document beside a readable key all classify correctly.
  - **Proposal citations** added in the 0017 *Dependencies* edit hold: `crates/server/build.rs:117`, `xtask/src/dist.rs:546`, `lib.rs:1693-1699`.
  - The three open design points from the iteration-3 sign-off (strict verdict, pre-release first tag, empty listing unsettled) are already with the human. I have not raised them again.
<!-- pdca:leaf-complete -->

### Advisory — code-review

No new actionable findings in either lens: introduced correctness bugs or reuse, simplification, and efficiency.

Reviewed the target's classification and verdict paths (`crates/validate/src/matrix.rs:907`, `crates/validate/src/matrix.rs:1213`, `crates/validate/src/matrix.rs:1397`), regression fixtures (`crates/validate/tests/capability_matrix.rs:772`, `crates/validate/tests/capability_matrix.rs:844`, `crates/validate/tests/capability_matrix.rs:878`), and proposal amendment (`docs/design/proposals/draft/0017-blackbox-validation-tool.md:873`). Previously deferred design decisions are not re-raised.

Validation evidence is the frozen gate output: 48 tests pass, diff coverage is 86.3%, and mutation testing reports 128 caught / 30 unviable / none missed. Full CI remains failed on dependency advisories outside this diff; this advisory does not supersede that gate.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Accept or require stronger evidence for the declared new-API exception, and assign resolution of the inherited advisory failures — restoration passes 48 tests and all ten injected faults are caught, but there is no executable pre-fix red and full CI remains failed; `reviewer-restored-green.log:56`, `reviewer-mutations.log:3`, `gate-logs/C4-ci.log:9356`.
- [ ] Validation — fitness-to-purpose — Approve the pre-0.1 expectations and early-PUT protocol classification as the contract for smoke #939 — this slice proves pure decisions, while live observations and binary integration belong to the subsequent slice; `brief.md:40`, `brief.md:151`, `target/docs/design/proposals/draft/0017-blackbox-validation-tool.md:284`.
- [ ] **A required row can be passed without any check, and no test stops that regression.** `crates/validate/src/matrix.rs:1178-1180` makes `Exercise::Other` on a `required` row unsettled. That arm is the only thing stopping a generic exercise from passing a required row. I replaced it with `Exercise::Other { .. } => Ok(())`. The suite stayed 48/48 green, and `Other { operation: PutObject, answer: 200 + strong ETag }` then classified as `Pass` against `PRE_0_1` with no status, ETag, early-answer or body check. Under that change the arm never reads the answer, so any answer passes. 743.3 picks which `Exercise` variant to build, so a mapping slip there would quietly bypass every (B′) check. Fix: add a fixture asserting `classify(row(PutObject), &Exercise::Other { operation: PutObject, answer: <good 200> })` is `Unsettled`, plus a verdict-level case in which that run is INCONCLUSIVE.
- [ ] **Two of the bulk-delete pass conditions have no test.** (1) `matrix.rs:1218-1220` (the empty-keys guard): with it removed, `DeleteObjects { keys: [], 200 + empty <DeleteResult>, follow_ups: [] }` returns `Pass`, which is a vacuous pass (the rubric's *Absent entries* class: "a count-based assertion that can pass while the property fails"). (2) `matrix.rs:1291-1295` (`<Deleted>` must name every requested key, as brief (B′) says at `brief.md:55`): with it removed, keys `[p/a, p/b]`, a `<Deleted>` naming only `p/a`, no `<Error>` row, and both `HEAD`s 404 returns `Pass`. In both cases the suite stays 48/48 green. Every bulk fixture either names all keys under `<Deleted>` or carries an `<Error>` row (`capability_matrix.rs:91-96`, `:110-123`, `:683-695`). Fix: add those two fixtures.
- [ ] **Six more guards can be removed with the suite green. Each has a concrete case:** `matrix.rs:942-946` (unsupported row, declared status, then a torn body → unsettled): removed, `CopyObject` 501 + `Body::Failed` becomes fatal `protocol`. That is the "torn answer made fatal" class iteration 1 fixed for the 304 row, now unguarded on every unsupported row. `matrix.rs:1316-1318` (the unit must be `bytes`): removed, `Content-Range: items 2-5/16` passes. The rubric's *Grammar strictness* class names units, and `capability_matrix.rs:566-573` only checks that `Bytes` is accepted, never that another unit is refused. `matrix.rs:1106` (206 with no `Content-Range` → `protocol`): changed to `None => {}`, a 206 with the right bytes and no `Content-Range` passes. `matrix.rs:1159` (listing 200 without a listing document) and `:1274` (bulk-delete 200 without a `<DeleteResult>`): changed to `Ok`, both pass. `matrix.rs:1408` (an exercise the profile declares no row for → `Undeclared`): removed, the run goes from INCONCLUSIVE to PASS. Fix: one fixture per branch. I wrote all ten cases as a probe file. Production passes every one, so they are pure additions with no code change.
- [ ] **The gating C4-ci row is red for the same reason as iteration 4, and the base is still not the one the human asked for.** `gate-logs/C4-ci.log:3865-3872` and `:3947-3954` show cargo-deny failing on RUSTSEC-2026-0385 (ctutils) and RUSTSEC-2026-0386 (tokio). The target is built on `93a6f37` (`git log -1`: "pre-fix base 93a6f37…"), whose `Cargo.lock` still pins ctutils 0.4.2 (`target/Cargo.lock:1000-1001`) and tokio 1.52.3 (`:3953-3954`). The iteration-3 sign-off said to rebuild on `b552ca2`, which carries #950 and #954 (`brief.md:235`). Iteration 4 asked for "matching full-CI evidence on the intended refreshed base" (`brief.md:243`). Neither happened. The patch touches no Cargo file, so the failure is not the matrix's fault, but the full-CI evidence the human asked for still does not exist. The fix is choosing the base, which the builder cannot do.
- [ ] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [ ] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) —   - cargo deny --all-features --config deny-all-features.toml check advisories: exited with exit status: 1
- [ ] external dependency: cargo deny all-features advisories — the run's stack base `93a6f37` (`pdca-integration/main-r42c5348f203d`) still pins `ctutils 0.4.2` and `tokio 1.52.3` (`Cargo.lock:1000-1001`, `:3953-3954`), so `cargo deny --all-features --config deny-all-features.toml check advisories` exits 1 on the unpatched base (RUSTSEC-2026-0385, RUSTSEC-2026-0386; confirmed this round before the patch was applied) and C4-ci cannot go green on it; #950/#954 fixed both on `main` `b552ca2` but not on the integration line, and the standing doctor row runs in `../wyrd` (`b552ca2`, green), so it will not flag this. This patch touches no manifest, lockfile or deny config; refresh the integration line onto `b552ca2` (or rebase this bundle on `main` with the one-line `lib.rs` hunk in iteration 4's notes) and re-run Check
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
- Iteration delta (if iterating): Auto-iterate (round 4): rebuilding for the implementation-level findings — C4 Verification (red→green) — Accept or require stronger evidence for the declared new-API exception, and assign resolution of the inherited advisory failures — restoration passes 48 tests and all ten injected faults are caught, but there is no executable pre-fix red and full CI remains failed; `reviewer-restored-green.log:56`, `reviewer-mutations.log:3`, `gate-logs/C4-ci.log:9356`.; **A required row can be passed without any check, and no test stops that regression.** `crates/validate/src/matrix.rs:1178-1180` makes `Exercise::Other` on a `required` row unsettled. That arm is the only thing stopping a generic exercise from passing a required row. I replaced it with `Exercise::Other { .. } => Ok(())`. The suite stayed 48/48 green, and `Other { operation: PutObject, answer: 200 + strong ETag }` then classified as `Pass` against `PRE_0_1` with no status, ETag, early-answer or body check. Under that change the arm never reads the answer, so any answer passes. 743.3 picks which `Exercise` variant to build, so a mapping slip there would quietly bypass every (B′) check. Fix: add a fixture asserting `classify(row(PutObject), &Exercise::Other { operation: PutObject, answer: <good 200> })` is `Unsettled`, plus a verdict-level case in which that run is INCONCLUSIVE.; **Two of the bulk-delete pass conditions have no test.** (1) `matrix.rs:1218-1220` (the empty-keys guard): with it removed, `DeleteObjects { keys: [], 200 + empty <DeleteResult>, follow_ups: [] }` returns `Pass`, which is a vacuous pass (the rubric's *Absent entries* class: "a count-based assertion that can pass while the property fails"). (2) `matrix.rs:1291-1295` (`<Deleted>` must name every requested key, as brief (B′) says at `brief.md:55`): with it removed, keys `[p/a, p/b]`, a `<Deleted>` naming only `p/a`, no `<Error>` row, and both `HEAD`s 404 returns `Pass`. In both cases the suite stays 48/48 green. Every bulk fixture either names all keys under `<Deleted>` or carries an `<Error>` row (`capability_matrix.rs:91-96`, `:110-123`, `:683-695`). Fix: add those two fixtures.; **Six more guards can be removed with the suite green. Each has a concrete case:** `matrix.rs:942-946` (unsupported row, declared status, then a torn body → unsettled): removed, `CopyObject` 501 + `Body::Failed` becomes fatal `protocol`. That is the "torn answer made fatal" class iteration 1 fixed for the 304 row, now unguarded on every unsupported row. `matrix.rs:1316-1318` (the unit must be `bytes`): removed, `Content-Range: items 2-5/16` passes. The rubric's *Grammar strictness* class names units, and `capability_matrix.rs:566-573` only checks that `Bytes` is accepted, never that another unit is refused. `matrix.rs:1106` (206 with no `Content-Range` → `protocol`): changed to `None => {}`, a 206 with the right bytes and no `Content-Range` passes. `matrix.rs:1159` (listing 200 without a listing document) and `:1274` (bulk-delete 200 without a `<DeleteResult>`): changed to `Ok`, both pass. `matrix.rs:1408` (an exercise the profile declares no row for → `Undeclared`): removed, the run goes from INCONCLUSIVE to PASS. Fix: one fixture per branch. I wrote all ten cases as a probe file. Production passes every one, so they are pure additions with no code change.; C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — - cargo deny --all-features --config deny-all-features.toml check advisories: exited with exit status: 1. Deferred, not addressed here: 6 finding(s) that need human judgment, held in deferred-findings.json for the human at handover — not build work
- By / date: auto-iterate / 2026-10-19

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
