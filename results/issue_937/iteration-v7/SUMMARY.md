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
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 91.2% — 624 of 684 instrumentable changed lines executed (floor 80%); 684 of 1487 changed lines were instr
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 165 mutants tested in 8m: 1 missed, 134 caught, 30 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_937/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.51s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #937's pure, version-keyed S3 capability matrix: no new implementation defect was established; baseline verification and the explicitly reserved policy decisions still need human sign-off.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | NEEDS-HUMAN | Decide whether retention starts at a release-candidate tag or the first final release — `0.1.0-rc.1` selects mutable `pre-0.1`, while the proposal promises retention at the first tag; this determines whether later validators preserve that tag's expectations (`target/crates/validate/tests/capability_matrix.rs:357`, `target/docs/design/proposals/draft/0017-blackbox-validation-tool.md:284`, `brief.md:235`). |
| C2 Reproduction (red pre-fix) | NEEDS-HUMAN | Accept the declared new-API exception or require another discriminator — stashing production while retaining the submitted test produces an unresolved import, not an executable behavioral red; all ten specified fault injections do execute and fail (`brief.md:138`, `reviewer-red.log:261`, `reviewer-mutations.log:3`). |
| C3 Change | PASS | The changes remain within the pure matrix slice and its required proposal corrections; the previous contradictory version-header dependency paragraph is resolved, and production was restored exactly to the supplied patch (`target/crates/validate/src/lib.rs:24`, `target/docs/design/proposals/draft/0017-blackbox-validation-tool.md:873`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Require full-CI evidence on the intended refreshed baseline — 57 restored tests pass, but both independently rerun advisory scans still reject inherited ctutils 0.4.2 and tokio 1.52.3; this is a baseline/evidence caveat, not a matrix defect (`reviewer-final-green.log:66`, `reviewer-deny-rerun.log:9`, `reviewer-deny-all-features-rerun.log:91`, `target/Cargo.lock:1000`, `target/Cargo.lock:3953`, `brief.md:235`). |
| C5 Causal adequacy | PASS | The checks preserve decisive failures alongside incomplete evidence, and the required fault injections distinguish the intended verdicts; no capability probe masks a load-time cause (`target/crates/validate/src/matrix.rs:914`, `target/crates/validate/src/matrix.rs:1209`, `target/crates/validate/tests/capability_matrix.rs:838`, `reviewer-mutations.log:3`). |
| T1 Structure | PASS | The classifier depends on observations and the existing error-code value, with no I/O, clock, SDK error dependency, shared mutable global, or new production dependency (`target/crates/validate/src/matrix.rs:31`, `target/crates/validate/src/matrix.rs:617`). |
| T2 Shape | PASS | Distinct types preserve no-answer, incomplete-body, fatal, and unchecked states through the public interface; duplicate profile rows cannot silently hide an expectation (`target/crates/validate/src/matrix.rs:225`, `target/crates/validate/src/matrix.rs:684`, `target/crates/validate/src/matrix.rs:851`). |
| T3 Runtime | PASS | Real compilation and execution exercise production classification without a tool shim; the restored 57-test suite passes, including torn responses, contradictory probes, invalid grammar, and vacuous profiles (`reviewer-final-green.log:66`, `target/crates/validate/tests/capability_matrix.rs:765`, `target/crates/validate/tests/capability_matrix.rs:1088`). |
| T4 Contribution | N/A | Contribution drafts are intentionally absent during Check; their substantive audit must rerun at publish. The frozen batch review is clean, and independent affected-path history checking found no closed/rejected overlap (`gate-logs/T4-contribution.log:10`, `gate-logs/T4-batch-review.log:10`, `reviewer-prior-art.log:45`). |
| T5 Judgment | NEEDS-HUMAN | Accept or revise the reserved retry and empty-listing policies — one unanswered exercise defeats a successful retry, and a correctly empty listing remains INCONCLUSIVE; both are deliberately stricter than the original acceptance wording and affect usable run outcomes (`target/crates/validate/src/matrix.rs:1378`, `target/crates/validate/src/matrix.rs:1154`, `target/crates/validate/tests/capability_matrix.rs:1183`, `brief.md:235`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Approve the observation contract and the settled early-upload `protocol` classification for the smoke consumer — this review demonstrates pure decisions; request-to-observation mapping and deployment validation belong to #939 and are not established by these tests (`target/crates/validate/src/matrix.rs:719`, `target/crates/validate/src/matrix.rs:1046`, `brief.md:151`). |

The implementation evidence is positive, with a precise limit: the tests demonstrate the new classifier's behavior, but cannot execute against a base that has no classifier. The independent command was `cargo test --locked --offline -p wyrd-validate --test capability_matrix`. Production alone was stashed, the submitted test retained, and the resulting missing-module failure recorded; restoration passed all 57 tests. Each of the ten mutations required by the brief then produced an executed assertion failure. A final restored run again passed 57/57. Both added files match their patch contents exactly, and `git apply --reverse --check patch.diff` succeeds against the restored disposable target. No implementation changes remain from review.

The full-CI failure remains an inherited baseline problem. The frozen run reports only the two cargo-deny checks as failed (`gate-logs/C4-ci.log:9374`), with RUSTSEC-2026-0385 and RUSTSEC-2026-0386 identified at `gate-logs/C4-ci.log:8508` and `gate-logs/C4-ci.log:8590`. The supplied lockfile retains those flagged versions despite the brief's intended #950/#954 refresh. The initial independent scan could not lock the read-only advisory cache. Copying that database into this leaf's cwd and changing only its cache location allowed both real scans to execute: they reproduced the two advisory failures (`reviewer-deny-rerun.log:314`, `reviewer-deny-all-features-rerun.log:358`). These results do not establish a new dependency defect in this patch, which changes neither manifest nor lockfile.

The remaining frozen gates have sufficient evidence to adjudicate without inventing missing-oracle findings:

- **C4-verify:** UNVERIFIABLE, reproduced independently. Its green leg passes 57 tests and its red leg never executes a test (`gate-logs/C4-verify.log:10`, `gate-logs/C4-verify.log:15`). This is the C2 exception above.
- **C4-diff-cov:** PASS in the captured run: 624/684 instrumentable changed lines, 91.2%, above the 80% floor (`gate-logs/C4-diff-cov.log:163`). This measurement was read from the frozen log, not independently regenerated.
- **C5-mutants:** the captured scan tested 165 mutants: 134 caught, 30 unviable, one missed (`gate-logs/C5-mutants.log:14`). The missed `&&` to `||` change at `target/crates/validate/src/matrix.rs:1455` also survives the independent rerun (`reviewer-mutations.log:13`). It can add a misleading explanatory line when nothing passed, but cannot change the outcome or the fatal/unproved records. Rejected as a behavioral-defect finding; the ten mandated behavioral mutations are all caught.
- **T4-batch-review / host-tikv:** the captured batch reports zero blocking findings; both TiKV clippy invocations completed (`gate-logs/T4-batch-review.log:10`, `gate-logs/host-tikv.log:209`). Local clippy is unavailable (`reviewer-clippy.log:1`), so the broader compile/lint evidence remains the frozen run rather than a claimed independent reproduction.
- **T4-contribution:** N/A at Check, with the substantive audit explicitly deferred to publish (`gate-logs/T4-contribution.log:10`). It is not an outstanding human clearance item.

The brief's external tools were exercised successfully. Independent `typos`, documentation lint, documentation render/link audit, and `cargo fmt --all -- --check` all passed; the renderer produced 99 pages (`reviewer-docs-lint.log:2`, `reviewer-docs-render.log:3`, `reviewer-typos.log:1`, `reviewer-fmt.log:1`). There is no unmet `typos` or docs-renderer dependency. The pure fixtures are the specified scope of this slice, not substitutes for an unexercised network topology.

The prior-art check was independently repeated by affected path, not by title alone. GitHub history queries returned 11 commits for `crates/validate/` and seven for `docs/design/proposals/draft/0017-blackbox-validation-tool.md`; all 19 closed-unmerged PRs had their complete changed-file lists checked, with no overlap (`reviewer-prior-art.log:2`, `reviewer-prior-art.log:25`). The current target still denies multipart subresources, so its six unsupported multipart rows are grounded in this baseline (`target/crates/gateway-s3/src/lib.rs:410`, `target/crates/validate/src/matrix.rs:316`). The reported version/header and operation citations were checked against this target, not another checkout. The standing rubric's no-DCO-findings and settled-deferral rules were observed.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — #937 (743.1, the capability matrix)

Method: I copied `crates/validate/src/matrix.rs` byte-for-byte (diff-checked against
`$PDCA_TARGET`) and the shipped `crates/validate/tests/capability_matrix.rs` into a scratch
crate, with `ErrorCode` copied verbatim from `crates/validate/src/s3/error.rs:64-76,191-199`.
The shipped suite passes there 57/57, the same as `gate-logs/C4-verify.log`. I then ran my own
logic mutations and probe cases against that copy.

## Red→green evidence: attempted to refute, could not

- I made each of the brief's ten required reds (`brief.md:143-150`) as an edit to production
  `matrix.rs`. Every one turned the shipped suite red, and every test calls the production
  `wyrd_validate::matrix` functions, not a copy. (1) unsupported 2xx → pass, `:915`: 4 tests fail.
  (2) 2xx with a failed body → no answer: 2 fail. (3) ETag accepts any non-empty value, `:1052`:
  3 fail. (4) the non-matching conditional GET accepts 304, `:1101-1103`: 1 fails. (5) bulk delete
  passes on its document alone, `:1225`: 5 fail. (6) the selector ignores the version, `:601`:
  5 fail. (7) `+git.unknown` read as `0.0.0`, `:543`: 1 fails. (8) a not-checked row counted as
  unexercised, `:1412`: 2 fail. (9) the PUT ignores an early head, `:1046`: 2 fail. (10) a probe
  with no answer → `oracle`, `:1173`: 4 fail. C4-verify's UNVERIFIABLE (exit 77, the test cannot
  compile with production reverted) is the net-new exception the brief declared in advance
  (`brief.md:138-141`). I see no sign the tests pass for the wrong reason.
- I also tried to break the fix and could not, in these areas:
  - Version parsing against every shape `normalize_describe` emits
    (`crates/server/src/version/derivation.rs:40-68`), including `0.1.0+git.dirty` for a dirty
    build exactly on a tag, and `+git.unknown` on a fallback base other than `0.0.0`.
  - Pairing each bulk-delete probe with the key it asked about, retries, and probes of keys the
    delete did not request.
  - Torn bodies on every bodiless answer.
  - All 22 `pre-0.1` citations: each matches the line on this base (`crates/gateway-s3/src/lib.rs`
    :121, :410-422, :1766-1788, :1797-1815, :1819-1826, :1842-1849, :1865-1871, :2008-2028,
    :2713-2719, :2785-2803, :2293-2300, :3401).
  - The 0017 *Dependencies* contradiction raised in iteration 4, now fixed at
    `docs/design/proposals/draft/0017-blackbox-validation-tool.md:873`.

## Findings

- NEEDS-HUMAN [human] — **The gating C4-ci red comes from the base, not this patch, and
  rebuilding cannot clear it.** The only failing steps are the two `cargo deny` advisory checks:
  RUSTSEC-2026-0385 on ctutils 0.4.2 and RUSTSEC-2026-0386 on tokio 1.52.3
  (`gate-logs/C4-ci.log:3880`, `:3962`, `:5178-5179`). Both versions come from the base's
  `Cargo.lock:1000-1001` and `:3953-3954`, which the patch does not touch. Everything else passed:
  typos, `lint_docs`, the docs render, clippy, and the tests, `capability_matrix` included
  (`gate-logs/C4-ci.log:17-25`, `:3179`). The base (`93a6f37`) still lacks #950 and #954, although
  the iteration-3 carry-forward (`brief.md:235`) asked for a rebuild on `b552ca2`. This is the
  fourth round red for the same reason, and `Cargo.lock`/`deny*.toml` are out of this slice's
  scope (`brief.md:130-134`). Someone has to rebase the bundle onto a base that has #950/#954, or
  accept the red as inherited.

- NEEDS-HUMAN [human] — **The T4 "pass" is thin evidence.** `check-gates.json:108-116` records
  `T4-batch-review` as a gating pass: "0 blocking, 0 recorded-rejected, 0 noise-dropped" in
  **19.58 s**. That would be three review passes over 1,487 changed lines (about 100 KB of diff) in
  under 20 seconds, where earlier rounds of the same gate found 5 and then 2 blocking findings. I
  cannot see `review-batch.md`. Before treating T4 as the rubric's "one deep, multi-pass review",
  confirm the file holds real passes over this diff and not a cached or short-circuited result.

- NEEDS-HUMAN [human] — **The same 503 means FAIL on an unsupported row and INCONCLUSIVE on a
  required one, and the module's rustdoc says the opposite of the first.** In probe P4,
  `CopyObject` answered `503 SlowDown` gives `Fatal { Protocol }` (`crates/validate/src/matrix.rs:920-925`,
  pinned by `capability_matrix.rs:398`). In probe P5, `PutObject` answered `503 SlowDown` gives
  `Unsettled` ("availability", `matrix.rs:949-951`). The code follows the brief ("a 5xx included",
  `brief.md:34-35`) and 0017 §3 ("Not a 500", `0017:194`). But 0017 §7 puts every 5xx under
  budgeted `availability` (`0017:508`). The module's own invariant (`matrix.rs:26-27`) and the
  `RowResult::Unsettled` doc (`:863-864`) also list "a 5xx" as an answer that neither passes nor
  fails, which is false for every unsupported row. In practice, one transient 503 from a proxy
  during the unsupported sweep FAILs a long run. Someone has to decide which rule holds. If the
  brief's rule stays, the two rustdoc passages need a builder-sized correction.

- NEEDS-HUMAN [human] — **The conditional-GET negative control cannot be enforced by the
  matrix.** `Exercise::GetObjectIfNoneMatchMiss` (`matrix.rs:744-747`) and `...Hit` (`:741-743`)
  carry no record of the `If-None-Match` value sent or of the object's tag. In probe P6, a plain
  200 with the whole body passes the Miss row. So if 743.3's Miss request omits the header, or
  sends a tag the server ignores, a server that "answers 304 to any tag" passes both rows. That is
  the exact case brief (B′) calls the decided negative control (`brief.md:49-51`). Compare
  `GetObjectRanged`, which carries `first`/`last` and calls an impossible request unsettled
  (`matrix.rs:1075-1079`). A fix would add `sent`/`object_etag` fields: unsettled unless `sent` is
  a strong tag that differs from the object's tag (Miss) or equals it (Hit). That changes the type
  743.3 builds, so someone has to decide whether this guard belongs here or in 743.3's mapping.

- NEEDS-HUMAN [human] — **The whole-body observation type will not hold the size classes later
  scenarios need.** `Exercise::GetObject.object` and `GetObjectRanged.object` are `Vec<u8>`
  (`matrix.rs:730-740`), and `Body::Bytes` is `Vec<u8>` (`:687`). Judging the 5 GiB "must
  succeed" class (`0017:306`) means holding the expected object and the response body in memory,
  about 10 GiB per GET, while the brief says "every later scenario" needs this matrix
  (`brief.md:15-16`). This is fine for smoke. Someone should decide whether to record a
  digest-based body variant as a tracked follow-up now.

- NEEDS-HUMAN [impl] — **Three pass conditions have no test.** Production is correct on all
  three; a mutation that removes each one leaves the shipped suite 57/57 green.
  - (a) `matrix.rs:971` (`Body::Empty => Ok(&[])`) never runs (`gate-logs/C4-diff-cov.log:157`).
    Changing `:1013` to `if got == want || got.is_empty()` lets a GET of a 16-byte object answered
    `200` with no body pass. Production gives `Integrity` (P1). Add `get(just(200))` → `Integrity`
    beside `capability_matrix.rs:548-564`.
  - (b) Adding `got.dedup()` after `got.sort()` at `matrix.rs:1147` survives, so a listing that
    names `p/a` twice (a pagination overlap) would pass. Production gives `Oracle` (P2). Add a
    duplicate-key case to `capability_matrix.rs:625-637`. This is the rubric's "can pass while the
    property fails" class.
  - (c) Adding `deleted.dedup()` after `matrix.rs:1274` survives, so a `<Deleted>` naming `p/a`
    twice would pass. Production gives `Protocol` (P3). This one is lower priority.
  - Optional, and the sole reason C5 is red (`gate-logs/C5-mutants.log`): under the surviving
    `&&`→`||` mutant at `matrix.rs:1455`, the report for `verdict(&PRE_0_1, &[])` would claim
    "no required or unsupported row declared". Assert the opposite in `capability_matrix.rs:1176-1181`.
    It is text only; the outcome does not change.
  - Size caution: `patch.diff` is 101,896 bytes. That is under 100 KiB (102,400 bytes) but over
    100 kB, against the brief's "100 KB backstop" (`brief.md:166-167`), so keep these fixtures terse.

- Not re-raised: the three open design points the human already holds (`brief.md:235`): verdict
  strictness on retries, `0.1.0-rc.1` selecting the edited-in-place profile, and an empty listing
  being unsettled. The behavior matches what build-notes was told to flag. I found nothing new
  about them.

<!-- pdca:leaf-complete -->

### Advisory — code-review

- No introduced correctness defects found. Reviewed version selection, response classification, delete probes, and verdict folding against the target source (`crates/validate/src/matrix.rs:493`, `crates/validate/src/matrix.rs:891`, `crates/validate/src/matrix.rs:1197`, `crates/validate/src/matrix.rs:1384`) and the regression fixtures (`crates/validate/tests/capability_matrix.rs:303`).
- No actionable reuse, simplification, or efficiency findings. Common response, body, and probe checks already share helpers (`crates/validate/src/matrix.rs:946`, `crates/validate/src/matrix.rs:983`, `crates/validate/src/matrix.rs:1171`).

Validation evidence: the frozen logs report 57 passing matrix tests and 91.2% diff coverage. Full CI remains failed on dependency advisories outside this diff. No gates were rerun and the target was not modified.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C1 Spec — Decide whether retention starts at a release-candidate tag or the first final release — `0.1.0-rc.1` selects mutable `pre-0.1`, while the proposal promises retention at the first tag; this determines whether later validators preserve that tag's expectations (`target/crates/validate/tests/capability_matrix.rs:357`, `target/docs/design/proposals/draft/0017-blackbox-validation-tool.md:284`, `brief.md:235`).
- [x] C2 Reproduction (red pre-fix) — Accept the declared new-API exception or require another discriminator — stashing production while retaining the submitted test produces an unresolved import, not an executable behavioral red; all ten specified fault injections do execute and fail (`brief.md:138`, `reviewer-red.log:261`, `reviewer-mutations.log:3`).
- [x] C4 Verification (red→green) — Require full-CI evidence on the intended refreshed baseline — 57 restored tests pass, but both independently rerun advisory scans still reject inherited ctutils 0.4.2 and tokio 1.52.3; this is a baseline/evidence caveat, not a matrix defect (`reviewer-final-green.log:66`, `reviewer-deny-rerun.log:9`, `reviewer-deny-all-features-rerun.log:91`, `target/Cargo.lock:1000`, `target/Cargo.lock:3953`, `brief.md:235`).
- [x] T5 Judgment — Accept or revise the reserved retry and empty-listing policies — one unanswered exercise defeats a successful retry, and a correctly empty listing remains INCONCLUSIVE; both are deliberately stricter than the original acceptance wording and affect usable run outcomes (`target/crates/validate/src/matrix.rs:1378`, `target/crates/validate/src/matrix.rs:1154`, `target/crates/validate/tests/capability_matrix.rs:1183`, `brief.md:235`).
- [ ] Validation — fitness-to-purpose — Approve the observation contract and the settled early-upload `protocol` classification for the smoke consumer — this review demonstrates pure decisions; request-to-observation mapping and deployment validation belong to #939 and are not established by these tests (`target/crates/validate/src/matrix.rs:719`, `target/crates/validate/src/matrix.rs:1046`, `brief.md:151`).
- [x] **The gating C4-ci red comes from the base, not this patch, and rebuilding cannot clear it.** The only failing steps are the two `cargo deny` advisory checks: RUSTSEC-2026-0385 on ctutils 0.4.2 and RUSTSEC-2026-0386 on tokio 1.52.3 (`gate-logs/C4-ci.log:3880`, `:3962`, `:5178-5179`). Both versions come from the base's `Cargo.lock:1000-1001` and `:3953-3954`, which the patch does not touch. Everything else passed: typos, `lint_docs`, the docs render, clippy, and the tests, `capability_matrix` included (`gate-logs/C4-ci.log:17-25`, `:3179`). The base (`93a6f37`) still lacks #950 and #954, although the iteration-3 carry-forward (`brief.md:235`) asked for a rebuild on `b552ca2`. This is the fourth round red for the same reason, and `Cargo.lock`/`deny*.toml` are out of this slice's scope (`brief.md:130-134`). Someone has to rebase the bundle onto a base that has #950/#954, or accept the red as inherited.
- [ ] **The T4 "pass" is thin evidence.** `check-gates.json:108-116` records `T4-batch-review` as a gating pass: "0 blocking, 0 recorded-rejected, 0 noise-dropped" in **19.58 s**. That would be three review passes over 1,487 changed lines (about 100 KB of diff) in under 20 seconds, where earlier rounds of the same gate found 5 and then 2 blocking findings. I cannot see `review-batch.md`. Before treating T4 as the rubric's "one deep, multi-pass review", confirm the file holds real passes over this diff and not a cached or short-circuited result.
- [x] **The same 503 means FAIL on an unsupported row and INCONCLUSIVE on a required one, and the module's rustdoc says the opposite of the first.** In probe P4, `CopyObject` answered `503 SlowDown` gives `Fatal { Protocol }` (`crates/validate/src/matrix.rs:920-925`, pinned by `capability_matrix.rs:398`). In probe P5, `PutObject` answered `503 SlowDown` gives `Unsettled` ("availability", `matrix.rs:949-951`). The code follows the brief ("a 5xx included", `brief.md:34-35`) and 0017 §3 ("Not a 500", `0017:194`). But 0017 §7 puts every 5xx under budgeted `availability` (`0017:508`). The module's own invariant (`matrix.rs:26-27`) and the `RowResult::Unsettled` doc (`:863-864`) also list "a 5xx" as an answer that neither passes nor fails, which is false for every unsupported row. In practice, one transient 503 from a proxy during the unsupported sweep FAILs a long run. Someone has to decide which rule holds. If the brief's rule stays, the two rustdoc passages need a builder-sized correction.
- [ ] **The conditional-GET negative control cannot be enforced by the matrix.** `Exercise::GetObjectIfNoneMatchMiss` (`matrix.rs:744-747`) and `...Hit` (`:741-743`) carry no record of the `If-None-Match` value sent or of the object's tag. In probe P6, a plain 200 with the whole body passes the Miss row. So if 743.3's Miss request omits the header, or sends a tag the server ignores, a server that "answers 304 to any tag" passes both rows. That is the exact case brief (B′) calls the decided negative control (`brief.md:49-51`). Compare `GetObjectRanged`, which carries `first`/`last` and calls an impossible request unsettled (`matrix.rs:1075-1079`). A fix would add `sent`/`object_etag` fields: unsettled unless `sent` is a strong tag that differs from the object's tag (Miss) or equals it (Hit). That changes the type 743.3 builds, so someone has to decide whether this guard belongs here or in 743.3's mapping.
- [ ] **The whole-body observation type will not hold the size classes later scenarios need.** `Exercise::GetObject.object` and `GetObjectRanged.object` are `Vec<u8>` (`matrix.rs:730-740`), and `Body::Bytes` is `Vec<u8>` (`:687`). Judging the 5 GiB "must succeed" class (`0017:306`) means holding the expected object and the response body in memory, about 10 GiB per GET, while the brief says "every later scenario" needs this matrix (`brief.md:15-16`). This is fine for smoke. Someone should decide whether to record a digest-based body variant as a tracked follow-up now.
- [ ] **Three pass conditions have no test.** Production is correct on all three; a mutation that removes each one leaves the shipped suite 57/57 green.
- [x] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [x] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) —   - cargo deny --all-features --config deny-all-features.toml check advisories: exited with exit status: 1
- [x] external dependency: cargo deny all-features advisories — the run's stack base `93a6f37` (`pdca-integration/main-r42c5348f203d`) still pins `ctutils 0.4.2` and `tokio 1.52.3` (`Cargo.lock:1000-1001`, `:3953-3954`), so `cargo deny --all-features --config deny-all-features.toml check advisories` exits 1 on the unpatched base (RUSTSEC-2026-0385, RUSTSEC-2026-0386; confirmed this round before the patch was applied, `$PDCA_SCRATCH/pdca-builder-937-it7/deny.log`) and C4-ci cannot go green on it; #950/#954 fixed both on `main` but not on the integration line, and the standing doctor row runs in `../wyrd` (green), so it will not flag this. This patch touches no manifest, lockfile or deny config; refresh the integration line onto current `main` (or rebase this bundle on `main`) and re-run Check
- [x] builder-declared external dependency `cargo deny all-features advisories` was REFUTED deterministically — its detect cmd (`cd "${WYRD_REPO:-../wyrd}" && { cargo fetch --locked >/dev/null 2>&1 || exit 3; } && cargo deny --frozen --all-features --config deny-all-features.toml check advisories >/dev/null 2>&1`) exited 0, so the dependency is present and full Check ran (#341); weigh the builder's claim against the review before accept
- [ ] **The delete acceptance rule turns missing evidence into an oracle failure.** `brief.md:47-51` requires a successful delete followed by HEAD 404, “else `oracle`”; `brief.md:74-77` only explicitly makes rows exercised “only with no answer” INCONCLUSIVE. For DELETE 204 followed by a HEAD timeout, an answer did arrive, but there is no evidence that the deleted key remains readable. The target distinguishes no-response and timeout outcomes (`crates/validate/src/s3/error.rs:36-47`), and proposal 0017 distinguishes transport availability from oracle disagreement and requires unresolved probes to remain INCONCLUSIVE (`docs/design/proposals/draft/0017-blackbox-validation-tool.md:489-493`, `:520-524`). Revise B′/E to define incomplete multi-observation checks separately from contradictory answers. Require deterministic fixtures for successful single/bulk delete followed by an unavailable HEAD → INCONCLUSIVE, alongside HEAD 200 → fatal oracle.
- [ ] **The PUT criterion omits the existing early-acknowledgement invariant.** `brief.md:38-39` checks 200 plus a strong ETag; the observation contract at `brief.md:150-154` describes response headers and response-body outcomes, without requiring preservation of request-source completion. A premature 200 with a valid ETag and a complete empty response body meets those stated checks even while the upload is unfinished. The target explicitly says a PUT is a receipt only when its source had ended before the response head (`crates/validate/src/s3.rs:161-170`) and rejects premature success at `:235-243`; its regression test asserts this at `crates/validate/tests/s3_client_upload_peers.rs:1207-1217`. Preserve that distinction in the matrix-owned observation and add a pure early-ack fixture with a valid ETag that cannot pass. Deferring client-to-observation mapping to #939 does not specify how this fact survives the boundary.
- [x] **The claimed integration base is not the supplied target.** `brief.md:90-94` says this run's integration line “already carries #744 and #853,” but the target's HEAD equals local `main` at `4bac538`; both `git merge-base --is-ancestor refs/heads/enhancement/744-validate-payloads-and-size-classes HEAD` and the corresponding `enhancement/853-validate-s3-client-nonconforming-responses` check return 1. The source still defers #853's GET framing work (`crates/validate/src/s3.rs:264-268`). The repository and `main` target resolve, and no prerequisite is formally declared. Correct the ordering/base assertion to match the supplied source, or identify the integration revision that must be supplied before implementation relies on those changes; the current brief cannot substantiate that they are already present.
- [x] **The run verdict is stricter than brief (E), and this is a contract 743.3 will inherit.** (E) says INCONCLUSIVE for a row "exercised **only** with no answer". `verdict` (`matrix.rs:1341-1366`) and the test `a_row_passes_only_when_every_exercise_of_it_passed` (`capability_matrix.rs:1029`) make the row unproved if *any* exercise got no answer. Probe: `[PUT NoAnswer, PUT 200 + strong ETag]` → INCONCLUSIVE. This is safe for the "never claims more" invariant. But it means one timeout followed by a successful retry ends a smoke run as INCONCLUSIVE, which fits poorly with §7's "must not abort … because one request timed out" (`0017:499-500`). A human should decide whether that strictness is intended before 743.3 builds retries on it.
- [x] **A pre-release first tag gets no retained profile.** `PRE_0_1` covers everything `below 0.1.0` (`matrix.rs:416`), so `0.1.0-rc.1` and `0.1.0-rc.1+git.3.<sha>` select `pre-0.1` (shown by the test and by my probe). The 0017 amendment says "Retention starts at the first tag" (`0017:287`). If the first tag is `v0.1.0-rc.1`, that tag lands in the profile that is edited in place. Either the range should end at the first tag, or the amendment should say "the first `0.1.0` release".
- [x] external dependency.
- [x] **One timed-out retry makes a row INCONCLUSIVE, which the brief's (E) does not say.** The brief: INCONCLUSIVE when a row was "exercised **only** with no answer". The code marks a row unproved if *any* exercise of it got no answer or was unsettled (`matrix.rs:1400-1401`). The test `capability_matrix.rs:1252-1258` pins that stricter rule. Concrete case: a full passing run plus one extra `GetObject` that timed out → INCONCLUSIVE. By the brief's wording that run is PASS. The rustdoc contradicts itself on this exact input: `matrix.rs:1376-1377` says "exercised only with no answer", and `:1377-1378` says "proved only when … every exercise of it passed". The stricter rule never over-claims, and for a timed-out *mutation* §7 (`0017:528-539`) supports it. But for reads, it means one transient timeout in any run that feeds every attempt to `verdict` can never be PASS. That cuts against §7's budgeted `availability` class (`0017:508`, `0017:499-500`). Decide which rule `verdict` keeps (or whether 743.3 feeds it retries at all), then make the rustdoc say one thing.
- [x] **The diff-coverage gate measured nothing, and the base needs confirming before publishing.** `gate-logs/C4-diff-cov.log`: "patch.diff does not apply on origin/main". The target's base commit is `93a6f37`, which is neither base the brief names (`main` `4bac538`, integration tip `582da99`). This is not evidence against the fix: C5-mutants caught 127 of the 128 viable mutants, and the one survivor is equivalent (next bullet). But the PR's base is not settled. Whether `ErrorCode` (including `MissingInXml`, used at `capability_matrix.rs:337`) is identical on the base the PR finally targets is the brief's claim, and I could not check it here.
- [x] C1 Spec — Decide when a first prerelease tag freezes its expectations — `0.1.0-rc.1` selects the mutable `pre-0.1` profile although retention starts at the first tag; this affects repeatable validation of tagged builds (`target/crates/validate/tests/capability_matrix.rs:292`, `target/docs/design/proposals/draft/0017-blackbox-validation-tool.md:284`, `reviewer-probes.log:2`).
- [x] C5 Causal adequacy — Decide whether one unanswered retry must keep an otherwise proved row INCONCLUSIVE — the implementation requires every exercise to pass, stricter than criterion E's “exercised only with no answer”; direct execution confirms the distinction (`target/crates/validate/src/matrix.rs:1393`, `brief.md:238`, `reviewer-probes.log:1`).
- [x] T5 Judgment — Decide whether an exactly empty listing of an empty prefix proves the listing row — current behavior makes the run INCONCLUSIVE, adding a coverage requirement beyond equality with the expected keys; the brief explicitly reserves this policy decision (`target/crates/validate/src/matrix.rs:1170`, `brief.md:238`, `reviewer-probes.log:3`).
- [x] **The gating `C4-ci` red comes from a stale base, not from this patch.** The worktree is on `93a6f37`, not the `b552ca2` that the iteration-3 carry-forward told this round to rebuild on (`brief.md:235`). The target's lockfile still pins `ctutils 0.4.2` (`Cargo.lock:1000-1001`) and `tokio 1.52.3` (`Cargo.lock:3953-3954`). Those are exactly RUSTSEC-2026-0385 and -0386, the only failures (`gate-logs/C4-ci.log:3865-3872`, `:5168-5170`). Every other CI step passed both attempts: fmt, clippy, build, the whole test run with `capability_matrix` at 48/48 (`C4-ci.log:3179-3229`), typos, docs lint and render, the blackbox and unsafe guards. The gates also measured two different bases: `C4-diff-cov` compiled `ctutils v0.4.5` and `tokio v1.54.1` in another worktree (`C4-diff-cov.log:12-13`), while `C4-ci` ran on the old one. The patch adds files plus one `lib.rs` line and a draft-doc edit, so it does not depend on the base. But the gating row cannot go green until the driver rebases the worktree onto current `main` and re-runs it. That is a harness or human action, not build work.
- [ ] **(Low; the outcome is FAIL either way.) A 404 to the HEAD or GET of a key the run just wrote is classed `protocol`, not `oracle`.** Probe on production: `HeadObject` answered `404` gives `Fatal { Protocol, "answered 404; expected 200" }`, and the same holds for `GetObject`. The cause is `expect_status` (`crates/validate/src/matrix.rs:972-981`) running before the oracle checks (`:1124`, `:1081`). Brief (B′) says HeadObject is "200 with the object's size and the PUT's `ETag`, else `oracle`". §7 (`0017:505`) defines `oracle` as "model and server disagree", and a written key that reads as absent fits that. But §7's `protocol` row ("wrong status") also fits. Only the class label in the report changes, and later scenarios that key on the class (#746) will inherit whichever reading is chosen. A human should pick one.
- [x] external dependency: cargo deny all-features advisories — the run's stack base `93a6f37` (`pdca-integration/main-r42c5348f203d`) still pins `ctutils 0.4.2` and `tokio 1.52.3` (`Cargo.lock`), so `cargo deny check` and the all-features advisories check fail on the unpatched base (RUSTSEC-2026-0385, RUSTSEC-2026-0386) and C4-ci cannot go green on it; #950/#954 fixed both on `main` `b552ca2` but not on the integration line, and the standing doctor row checks `../wyrd` (`b552ca2`, green), so it will not flag this. This patch touches no manifest, lockfile or deny config; refresh the integration line onto `b552ca2` (or rebase this bundle on `main` with the one-line `lib.rs` change above) and re-run Check
- [x] **The gating C4-ci row is red for the same reason as iteration 4, and the base is still not the one the human asked for.** `gate-logs/C4-ci.log:3865-3872` and `:3947-3954` show cargo-deny failing on RUSTSEC-2026-0385 (ctutils) and RUSTSEC-2026-0386 (tokio). The target is built on `93a6f37` (`git log -1`: "pre-fix base 93a6f37…"), whose `Cargo.lock` still pins ctutils 0.4.2 (`target/Cargo.lock:1000-1001`) and tokio 1.52.3 (`:3953-3954`). The iteration-3 sign-off said to rebuild on `b552ca2`, which carries #950 and #954 (`brief.md:235`). Iteration 4 asked for "matching full-CI evidence on the intended refreshed base" (`brief.md:243`). Neither happened. The patch touches no Cargo file, so the failure is not the matrix's fault, but the full-CI evidence the human asked for still does not exist. The fix is choosing the base, which the builder cannot do.
- [x] external dependency: cargo deny all-features advisories — the run's stack base `93a6f37` (`pdca-integration/main-r42c5348f203d`) still pins `ctutils 0.4.2` and `tokio 1.52.3` (`Cargo.lock:1000-1001`, `:3953-3954`), so `cargo deny --all-features --config deny-all-features.toml check advisories` exits 1 on the unpatched base (RUSTSEC-2026-0385, RUSTSEC-2026-0386; confirmed this round before the patch was applied) and C4-ci cannot go green on it; #950/#954 fixed both on `main` `b552ca2` but not on the integration line, and the standing doctor row runs in `../wyrd` (`b552ca2`, green), so it will not flag this. This patch touches no manifest, lockfile or deny config; refresh the integration line onto `b552ca2` (or rebase this bundle on `main` with the one-line `lib.rs` hunk in iteration 4's notes) and re-run Check
- [x] T5 Judgment — Decide whether to retain strict retry accounting, inconclusive empty listings, and the prerelease retention boundary — these declared open choices change what a PASS means across attempts and releases (`brief.md:238`, `target/crates/validate/tests/capability_matrix.rs:1282`, `target/crates/validate/tests/capability_matrix.rs:907`, `target/crates/validate/tests/capability_matrix.rs:334`).
- [x] **The gating C4-ci red comes from the base, not from this patch, for the third round running.** `cargo deny` fails on ctutils 0.4.2 (RUSTSEC-2026-0385) and tokio 1.52.3 (RUSTSEC-2026-0386) (`gate-logs/C4-ci.log:3873`, `:3955`, `:9367-9368`). The target pins exactly those versions (`Cargo.lock:1000-1001`, `Cargo.lock:3953-3954`), and its base commit is `93a6f37`. The carry-forward at `brief.md:235` said to rebuild on `b552ca2`, which carries #950 and #954. The patch touches no dependency. Someone has to refresh the base, or waive the red at sign-off. Another Do round will not fix it.
- [x] **The two delete rows treat a retried HEAD differently, and the module's own doc contradicts one of them.** In one bulk exercise, follow-ups `[(a, 503), (a, 404)]` give `Pass` (`matrix.rs:1248-1251`, pinned at `capability_matrix.rs:856-862`). `DeleteObject` carries exactly one `follow_up: Answer` (`matrix.rs:768-772`), so the same retry has to be sent as a second exercise. The "every exercise must pass" rule (`matrix.rs:1393-1396`) then makes the run INCONCLUSIVE. I probed it: "not proved: DeleteObject: … answered 503". So the same evidence (one 503, then a 404 for the same key) proves the bulk row and not the single row. The bulk result also breaks the module invariant at `matrix.rs:26-28` ("a row with such a check and no fatal one is unproved"), and a strict reading of `brief.md:60-63` ("a 5xx → no verdict for the row"). This is open design point (1) in a new place (`brief.md:235`). Pick one retry rule, apply it to both rows, and make the rustdoc match.
- [x] **The brief's 5xx rule for unsupported rows conflicts with 0017 §7** (low priority; the code follows the brief). A `503 SlowDown` to `CopyObject` is fatal `protocol` (`matrix.rs:936-941`, pinned at `capability_matrix.rs:380`). The same 503 to `PutObject` only leaves the row unsettled (`matrix.rs:965-967`). 0017's taxonomy table budgets every 5xx as `availability` (`0017:508`), while §3 says an unsupported row must not answer "a 500" (`0017:194`). The brief settled it as "a 5xx included" (`brief.md:33-34`). The cost: one transient 503 on any of the twelve unsupported rows turns a smoke run FAIL instead of INCONCLUSIVE. Confirm that this is intended.
- [x] external dependency: cargo deny all-features advisories — the run's stack base `93a6f37` (`pdca-integration/main-r42c5348f203d`) still pins `ctutils 0.4.2` and `tokio 1.52.3` (`Cargo.lock:1000-1001`, `:3953-3954`), so `cargo deny --all-features --config deny-all-features.toml check advisories` exits 1 on the unpatched base (RUSTSEC-2026-0385, RUSTSEC-2026-0386; confirmed this round before the patch was applied) and C4-ci cannot go green on it; #950/#954 fixed both on `main` (`b552ca2` and later) but not on the integration line, and the standing doctor row runs in `../wyrd` (green), so it will not flag this. This patch touches no manifest, lockfile or deny config; refresh the integration line onto current `main` (or rebase this bundle on `main`) and re-run Check

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
- Iteration delta (if iterating): Revise the brief. The rule (the human's): the server must respond with the defined response; that is what the verify is for. Implementation errors are reported as FAIL. So any answer other than the defined one is a FAIL on every row, required as well as unsupported, including a 5xx (other than a declared 501). Today a 5xx on a required row is treated as "availability" (unsettled, INCONCLUSIVE). Settle the class in the brief, and amend 0017 §7's availability row to match; it currently budgets every 5xx. Correct the rustdoc that says a 5xx "neither passes nor fails" (matrix.rs:26-27, :863-864) to match the rule. Carry forward, already decided at this sign-off: keep the strict retry rule, the rc-tag profile selection and the empty-listing rule as built; the inherited cargo-deny red is not this bundle's to fix (base 93a6f37 lacks #950/#954); the new-API red->green exception is accepted. Also fix in the rebuild: tests for the three unguarded pass conditions (GET 200 with an empty body -> integrity; a listing naming a key twice -> oracle; <Deleted> naming a key twice -> protocol). Still open for Plan to settle (not decided at this sign-off): the conditional-GET negative control (matrix records no sent tag; here or #939), whole-body Vec<u8> vs large size classes, protocol vs oracle for a 404 on a just-written key, the observation contract and early-PUT protocol class. Keep the patch under the 100 KB backstop (now 101,896 bytes).
- By / date: Eduard Ralph / 2026-10-19

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
