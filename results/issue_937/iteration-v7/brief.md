# validate: the capability matrix — profiles, version selection, answer classification (743.1)

> Do reads ONLY this file. Plan of record: proposal 0017,
> `docs/design/proposals/draft/0017-blackbox-validation-tool.md`. Read §3 *The capability
> matrix* (`0017:188-275`) and §7 *Failure taxonomy* (`0017:482-524`) before writing code.
> First of three children of #743. This one is pure: it sends nothing.
> Siblings: 743.1 = #937 (the matrix), 743.2 = #938 (the client verbs), 743.3 = #939 (smoke).

- **Slug:** validate-capability-matrix
- **Track:** blackbox
- **Kind:** enhancement
- **Defect:** `wyrd-validate` has no capability matrix. Nothing declares, per S3 operation,
  whether a Wyrd release must support it or must refuse it with an exact status and code;
  nothing picks those expectations by the server's version; and nothing turns an observed
  answer into a pass or one of 0017 §7's failure classes. Smoke (743.3) and every later
  scenario need all three, and none of it needs the network.
- **Success criterion:** the NEW file `crates/validate/tests/capability_matrix.rs` passes,
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
- **Falsifiability:** RED is reachable on the plain `cargo test -p wyrd-validate --test
  capability_matrix` target that `cargo xtask ci` runs: every leg calls a pure function with a
  fixed input, so a classifier that lets a 2xx through, accepts `*` as an ETag, accepts a 304
  to the non-matching tag, passes `DeleteObjects` on its document alone, passes a PUT answered
  before its body ended, calls a delete `oracle` when its follow-up `HEAD` got no answer, reads
  `+git.unknown` as `0.0.0`, or ignores the version fails an assertion. No network, Docker or second process.
- **Invariant to restore:** the verdict never claims more than the answers prove. An
  `unsupported` operation that succeeds in any part is fatal (`0017:197-199`); a `required`
  operation passes only on an answer showing the operation was done (`0017:190-195`, §7
  `0017:482-524`); "version unknown" is never mistaken for a release (`0017:257-261`).
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:**
- **Conflicts with:**
- **Ordering note:** runs in parallel with 743.2 (no shared file); 743.3 depends on both.
  **Base (corrected 2026-10-22).** This slice declares no prerequisite, so its base is whatever
  the run that drives it gives it, and either one works. Adopted by the run that split #743, it
  builds on that run's line, `pdca-integration/main-r42c5348f203d` (tip `582da99`: #744 folded
  as `61e5341`, #853 as `582da99`, on `main` `4bac538`; #743 was built there,
  `results/issue_743/stack-base`). Driven by a later run that does not name #744 and #853, it
  builds on `main` `4bac538`, which has neither (PRs #931, #934 are open). It uses nothing of
  theirs: the matrix judges its own observation type, never the client's `S3Error` or
  `BodyError` (#853 changes those, `crates/validate/src/s3/error.rs`). It may use
  `s3::ErrorCode` (`crates/validate/src/s3.rs:41`), identical on both bases. Every
  `crates/gateway-s3/src/lib.rs` citation below holds on both (the line does not touch that
  crate). Do records in `build-notes.md` which base it built on.
- **Scope:** the `matrix` module: operations, rows in three states, immutable profiles with
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
- **Difficulty:** medium
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/validate/tests/capability_matrix.rs` (NEW)
- **Verification posture:** (a) net-new. The `matrix` module does not exist on the base, so
  with production reverted the test cannot compile, and C4-verify reports UNVERIFIABLE (exit 77;
  this instance's gate compiles every added `*/tests/*.rs` together,
  `engine/scripts/run-verify.sh:404-413`). A pre-declared sign-off item, not a defect. Built
  and exercised at Check: the whole module, through the pure test. **Demonstrated reds
  (required, recorded in `build-notes.md`):** make each temporary edit, show the named leg
  fail, restore it: (1) a 2xx to an unsupported row classified as a pass → (B); (2) a 2xx head
  whose body failed classified as no answer → (B); (3) the PUT ETag check accepting any
  non-empty value → (B′); (4) the non-matching conditional GET accepting a 304 → (B′); (5)
  `DeleteObjects` passing on its document alone → (B′); (6) the selector ignoring the version
  → (C); (7) `+git.unknown` parsed as release `0.0.0` → (C); (8) a `supported, not checked`
  row counted as unexercised → (D); (9) the PUT check ignoring when its head arrived → (B′);
  (10) a delete whose follow-up `HEAD` had no answer classified as `oracle` → (B′)/(E).
- **Production reach:** consumed by 743.3 (smoke) in this same run; until it lands nothing in
  the binary calls the module. Every leg calls the production functions, not a copy.
- **Citations expected:** Do must cite `path:line` on the target branch for every change.
  - Each `pre-0.1` row cites the gateway line giving today's answer
    (`crates/gateway-s3/src/lib.rs`): `CopyObject` `:1865-1871`; the object-path subresource
    denylist (`uploads`, `uploadId`, `partNumber`) `:1842-1849`, its list `:410-422`; bulk
    `POST ?delete` `:1766-1788`; the bucket-path denylist (`?uploads`, `?location`)
    `:1797-1804`; a bucket path with any other method `:1809-1815`; the root path `/`
    `:1819-1826`; ranged 206 `:2713-2719`; 304 `:2785-2803`; `DeleteObjects` 200 with errors
    `:2293-2300`.
  - The version string's shape: `normalize_describe`,
    `crates/server/src/version/derivation.rs:30-41`: `0.0.0+git.<sha>[.dirty]` untagged,
    `0.1.0` on a tag, `0.1.0+git.3.<sha>` past one; `+git.unknown[.dirty]` at `:65`.
  - Entity tags: RFC 9110 §8.8.3, under the grammar-strictness rule `AGENTS.md:165-169`
    (validate every token).
  - Size budget: the first attempt's matrix and its test were 87 KB; keep this patch under the
    100 KB backstop.
- **Prior-art check (triage cycles):** searched by path `crates/validate/` and
  `docs/design/proposals/draft/0017-blackbox-validation-tool.md` across merged, open and
  closed-unmerged PRs on getwyrd/wyrd (2026-10-22). Merged: #845, #849, #859, #860, #865 (crate,
  dependency guard, client, dist, early-answer fix), #886 (the `Server` header), #765 (0017).
  Open: #931 (#744), #934 (#853). Closed-unmerged: none. No matrix code exists on `main` or the
  integration line. The rejected first attempt's `matrix.rs` (`results/issue_743/iteration-v1/
  patch.diff`) had no review finding beyond the items now in (B′) and (C).
- **Disposition hint:** new-feature

Plan-review response (2026-10-22): all three findings are addressed in place. (1) A delete's
follow-up `HEAD` is now a probe: 404 proves the delete, a 2xx is `oracle`, no answer or a 5xx
leaves the row unresolved (INCONCLUSIVE, per `0017:520-524`), any other status is `protocol`;
(B′) gains INCONCLUSIVE fixtures beside the `oracle` ones, and (E) folds an unresolved check as
not proved. (2) The PUT check now requires the request body to have ended before the head
arrived, the observation type carries that fact, and an early 200 with a valid `ETag` is a
`protocol` fixture. (3) The base claim was imprecise rather than wrong:
`pdca-integration/main-r42c5348f203d` (`582da99`) does carry #744 and #853, but a run that does
not adopt this bundle builds it on `main`. The ordering note now says which base applies when,
and that both work. (The review could not see `notes.json`; nothing above depends on it.)

## Design

- **The observation type is the matrix's own.** What the classifier reads: no answer (and
  why); or a response head (status, the headers the checks read: `ETag`, `Content-Range`,
  `Content-Length`), the S3 error code or its absence, and the body: its bytes, a typed listing
  or delete result, or the fact that it failed after the head. For a PUT it also carries
  **whether the request body had ended when the response head arrived** (the fact the client
  reports as `BodyError::AcknowledgedEarly`), so an early success survives the mapping and
  cannot read as a receipt. A check that needs more than one request (a delete and its
  follow-up `HEAD`s) takes each follow-up as its own answer-or-no-answer. 743.3 maps the
  client's results onto it. This keeps the matrix independent of #853's types and of 743.2.
- **Version selection** compares the release part, pre-release included; build metadata never
  changes the selection, except that the `git.unknown` identity means unknown.
- **0017 §3 amendment** (a draft proposal, editable: `.github/workflows/docs-immutability.yml`
  freezes proposals only once accepted). Add the third state to the state list at
  `0017:193-195`. After `0017:263-275`, add: until the first tagged release, the `pre-0.1`
  profile describes `main` and is edited in place when a feature lands. No release exists for
  a retained profile to protect, and every untagged build reports `0.0.0+git.<sha>`, so a
  second pre-release profile could not be selected anyway. Retention starts at the first tag.
  A feature that lands before the tool can check it moves its rows to `supported, not checked
  (#NNN)`, naming the issue that adds the check (for multipart: the `cp` upload leg, #750).
  Also correct `0017:259-261`, which still says the gateway advertises no version: since #886
  every response carries `Server: wyrd/<version>` (`crates/gateway-s3/src/lib.rs:1697-1699`),
  so the version is read from the wire and `--server-version` is an override.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR MAY
happen during the cycle. The PR MUST NOT be marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 1): rebuilding for the implementation-level findings — C4 Verification (red→green) — Accept or require stronger evidence for the declared net-new API exception — 41 tests pass after restoration, but the pre-fix test cannot execute and the coverage gate used a mismatched base; `reviewer-restored-green.log:49`, `reviewer-red.log:3`, `gate-logs/C4-diff-cov.log:10`.; C5 Causal adequacy — Correct the two failed-body classifications — an unresolved PUT currently yields PASS and an unresolved matching conditional GET yields fatal protocol, violating the evidence limit; `target/crates/validate/src/matrix.rs:1046`, `target/crates/validate/src/matrix.rs:1092`, `reviewer-cases.log:6`, `reviewer-cases.log:13`.; T5 Judgment — Add regression coverage for failed PUT and conditional-GET response bodies — the 41-test suite passes while both verdict errors survive; `target/crates/validate/tests/capability_matrix.rs:438`, `target/crates/validate/tests/capability_matrix.rs:539`, `target/crates/validate/tests/capability_matrix.rs:682`, `reviewer-cases.log:28`.; **A torn response passes three required rows.** The module's own invariant (`crates/validate/src/matrix.rs:26-27`) says a body cut off in transport leaves the row "unproved, never passed and never failed". The rubric's *Protocol input* class says the same: torn input is never silently accepted. But the PUT arm (`matrix.rs:1034-1052`) never looks at `r.body`, and neither do HEAD (`:1100-1118`) or DELETE (`:1120-1124`). Probe results on production code: `PUT 200 + strong ETag + Body::Failed` → `Pass`; `HEAD 200 right size/ETag + Body::Failed` → `Pass`; `DELETE 204 + Body::Failed, HEAD 404` → `Pass`. So a run where those answers were torn reports PASS, not INCONCLUSIVE. The T4 batch review flagged the PUT case three times (gate-logs/T4-batch-review.log). The probe confirms it, and the same gap exists on HEAD and DELETE, which T4 did not name. No test feeds `Body::Failed` to these rows (`capability_matrix.rs:682` covers GET only).; **The opposite error on the matching conditional GET.** `matrix.rs:1091-1092`: `304 + Body::Failed` → `Fatal { Protocol, "answered 304 with a body" }`. §7 (`0017:508`) classes a transport failure as `availability` (budgeted), never fatal. The module doc at `:26-27` says "never failed" too. A reset after a 304 head would FAIL the run. It should be `Unsettled`, as the GET, listing and bulk-delete arms already do (`:1136`, `:1195`).; **A rubric grammar class has no test (surviving mutant `matrix.rs:1266`).** The production code is correct: `bytes +2-+5/+16` → `protocol`. But the test does not pin it. I changed `!s.is_empty() && …` to `||` in a copy, and `u64::from_str` then accepts the `+` signs: the ranged GET with `Content-Range: bytes +2-+5/+16` returned `Pass`, and the shipped suite stayed **41/41 green**. The rubric names this exact case ("no `+`/`-` signs via `from_str`"). Fix: add a signed value to the wrong-`Content-Range` list at `capability_matrix.rs:503-509`.; **The bulk-delete probe never checks which key a HEAD answer belongs to (surviving mutant `matrix.rs:1225`).** I changed `k == key` to `k != key` in a copy. Then `keys [a,b]`, a clean `<DeleteResult>`, and follow-ups only for an unrequested key (`[(c,404),(c,404)]`) returned `Pass`. Production correctly returns `Unsettled`. The shipped suite stayed 41/41 green, because every fixture gives both keys the same answer or a symmetric pair. Add a fixture where the probed key differs from the requested ones.; **Duplicate follow-ups for one key are silently dropped.** `matrix.rs:1225` takes the *first* follow-up per key and ignores the rest. Probe on production: follow-ups `[(a,404),(a,200),(b,404)]` → `Pass`, even though a later HEAD shows the deleted key readable (`oracle`, `0017:505`). The other order, `[(a,503),(a,404)]`, gives `Unsettled` even though the retry proved the delete. The type permits this input (`follow_ups: Vec<(String, Answer)>`, `:786`), and 743.3's mapping could produce it after a retry. Silently skipping it hits the rubric's *Absent entries* class. Fix: judge every follow-up for a key, or call duplicates or unrequested keys unsettled.; **More untested branches from C5's 16 missed mutants.** These are real gaps, not cosmetic. Deleting the `Body::Failed` arm for listings (`:1136`) or for bulk delete (`:1195`) turns a transport cut into fatal `protocol`, and the suite stays green. The pre-release character check (`:554`) is never exercised: under the `||`→`&&` mutant, `0.0.0-r_c` parses. Add it to `parse_refuses_malformed_versions` (`capability_matrix.rs:746`). The 304-with-`Bytes(empty)` guard (`:1091`) is untested, yet 743.3's client may well map a bodiless reply that way. The range guard (`:1065`) is never hit, so a `>`→`>=` regression would mark every single-byte range unsettled without failing a test. The Display impls and the unused `Ord for Release` are cosmetic; skip them.; `crates/validate/src/matrix.rs:1045`: A PUT with status 200, a strong ETag and `request_body_ended: true` returns `Pass` without inspecting its response body. Thus `Body::Failed` can produce a run-level PASS despite an incomplete response; nonempty response bytes also pass despite the client's empty-body contract. Check the body before returning success: transport failure should leave the row unsettled, and an unexpected body should be `protocol`. Add both fixtures while preserving the existing early-acknowledgement failure.; `crates/validate/src/matrix.rs:1092`: A matching conditional GET with status 304 and `Body::Failed` falls into “answered 304 with a body,” producing fatal `protocol`. A timeout/reset does not establish that forbidden bytes arrived, so this manufactures FAIL where the observation only supports INCONCLUSIVE. Handle `Body::Failed` as unsettled and retain the fatal result for actual nonempty bytes. A shared empty-body checker could cover this and the PUT case consistently.; `crates/validate/tests/capability_matrix.rs:653`: The bulk-delete fixtures do not distinguish correctly associated probes from probes of other keys. Frozen `gate-logs/C5-mutants.log` confirms that changing `k == key` to `k != key` at `crates/validate/src/matrix.rs:1225` survives. Add a three-key fixture with ordered HEAD results `a:404, b:404, c:200` and require fatal `oracle`: the surviving mutation checks b for a and a for both b and c, falsely passing without examining c's readable result.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_937/review-b. Deferred, not addressed here: 6 finding(s) that need human judgment, held in deferred-findings.json for the human at handover — not build work
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 152 mutants tested in 10m: 16 missed, 106 caught, 30 unviable
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_937/review-b
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 2): rebuilding for the implementation-level findings — C4 Verification (red→green) — Accept the declared new-API verification exception — restoration passes 46 tests and all ten required fault injections are caught, but no behavioral pre-fix red or diff-coverage measurement was established (`reviewer-restored-green.log:54`, `reviewer-mutations.log:2`, `gate-logs/C4-diff-cov.log:10`).; C5 Causal adequacy — Preserve fatal follow-up evidence after a truncated bulk-delete response — HEAD 200/403 currently yields INCONCLUSIVE instead of the required oracle/protocol failure (`target/crates/validate/src/matrix.rs:1225`, `reviewer-probes.log:16`).; T5 Judgment — Add mixed-failure regression coverage — the truncated bulk-delete fixture supplies only 404 probes, so all 46 submitted tests pass while fatal 200/403 probes are discarded (`target/crates/validate/tests/capability_matrix.rs:753`, `reviewer-probes.log:73`).; **The gating T4 failure is real and still open: a torn bulk-delete document hides a readable key.** `crates/validate/src/matrix.rs:1225` returns `Unsettled` as soon as the `<DeleteResult>` body failed, before any follow-up `HEAD` is judged. Concrete case (run on production code): `DeleteObjects` keys `[p/a, p/b]`, `200` + `Body::Failed`, then `HEAD p/a → 200`, `HEAD p/b → 404` gives `Unsettled`, so the run is INCONCLUSIVE, not FAIL. This breaks three things: the brief's per-HEAD rule ("a 2xx → `oracle`", no exception for a torn document); the module's own fold rule at `matrix.rs:1010-1011` ("none is skipped because another already failed"); and its handling of the single-key case. There, a torn `204` followed by `HEAD 200` is `oracle` (`matrix.rs:1144`, pinned by `capability_matrix.rs:800-806`). A small fix works: on `Body::Failed`, skip the document checks, push the `unsettled` result into `checks`, and still run the probe loop. With that edit my probe gives `oracle`, a torn body with all `404`s stays `Unsettled`, and all 46 shipped tests stay green. Add the torn-body + `HEAD 200` fixture beside `capability_matrix.rs:753`. (Counter-argument, for the record: 0017 §7's outcome-resolution rule, `0017:535-539`, could read a torn bulk delete as an unresolved mutation where `HEAD 200` just records "not deleted". But the brief settled the per-probe classes, and the module already rejects that reading for `DeleteObject`.); `crates/validate/src/matrix.rs:1225`: A failed bulk-delete response body skips every follow-up HEAD, hiding fatal evidence. Direct execution of the target source with requested keys `[a, b]`, `200 + Body::Failed`, and probes `a → 404, b → 200` returns `Unsettled` and a run verdict of INCONCLUSIVE; the brief requires fatal `oracle` for the readable key. The equivalent single-delete case correctly returns fatal `oracle` through `worst` at `crates/validate/src/matrix.rs:1144`. Evaluate the bulk document and probes independently and combine their results with that helper. Add the mixed failed-body/readable-probe regression: `crates/validate/tests/capability_matrix.rs:753` currently covers a failed bulk body only with both probes returning 404.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_937/review-b. Deferred, not addressed here: 5 finding(s) that need human judgment, held in deferred-findings.json for the human at handover — not build work
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 158 mutants tested in 8m: 1 missed, 127 caught, 30 unviable
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_937/review-b
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: Iteration 3 was not reviewed: Check was skipped on the confirmed external dependency (cargo deny advisories). That is now cleared on main: #950 (ctutils 0.4.5, RUSTSEC-2026-0385) and #954 (tokio 1.54.1, RUSTSEC-2026-0386); ../wyrd is at b552ca2 and both advisory checks pass. Rebuild on that base and carry iteration 3's patch forward unchanged in substance; the goal of this round is a full Do+Check with gates and review, not a new approach. Open design points the human has not decided yet (keep current behavior, keep the rustdoc consistent, and flag each again in build-notes for sign-off): (1) verdict strictness: a row is proved only when every exercise passed, so one timed-out retry makes the run INCONCLUSIVE, which is stricter than brief (E)'s "exercised only with no answer"; (2) pre-release first tag: 0.1.0-rc.1 selects the in-place-edited pre-0.1 profile while the 0017 amendment says retention starts at the first tag; (3) the self-added rule that listing a prefix with nothing written is unsettled rather than PASS.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Iteration 3 was not reviewed: Check was skipped on the confirmed external dependency (cargo deny advisories). That is now cleared on main: #950 (ctutils 0.4.5, RUSTSEC-2026-0385) and #954 (tokio 1.54.1, RUSTSEC-2026-0386); ../wyrd is at b552ca2 and both advisory checks pass. Rebuild on that base and carry iteration 3's patch forward unchanged in substance; the goal of this round is a full Do+Check with gates and review, not a new approach.
  Open design points the human has not decided yet (keep current behavior, keep the rustdoc consistent, and flag each again in build-notes for sign-off): (1) verdict strictness: a row is proved only when every exercise passed, so one timed-out retry makes the run INCONCLUSIVE, which is stricter than brief (E)'s "exercised only with no answer"; (2) pre-release first tag: 0.1.0-rc.1 selects the in-place-edited pre-0.1 profile while the 0017 amendment says retention starts at the first tag; (3) the self-added rule that listing a prefix with nothing written is unsettled rather than PASS.
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 4 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 3): rebuilding for the implementation-level findings — C2 Reproduction (red pre-fix) — Accept the declared new-API exception — stashing production while retaining the test produces an unresolved import, so there is no executable behavioral pre-fix red; all ten required fault injections do execute and fail (`brief.md:138`, `reviewer-red.log:3`, `reviewer-mutations.log:2`).; C4 Verification (red→green) — Require matching full-CI evidence on the intended refreshed base — restored production passes 48 tests, but frozen CI still fails on old ctutils/tokio versions despite the brief's #950/#954 carry-forward; this is a base/evidence caveat, not a matrix defect (`reviewer-restored-green.log:56`, `gate-logs/C4-ci.log:8484`, `gate-logs/C4-ci.log:8566`, `brief.md:235`).; C4-ci — The full captured run completed its other checks but failed both cargo-deny checks on ctutils 0.4.2 and tokio 1.52.3 (`gate-logs/C4-ci.log:8484`, `gate-logs/C4-ci.log:8566`, `gate-logs/C4-ci.log:9356`). The supplied target has those same versions (`target/Cargo.lock:1000`, `target/Cargo.lock:3953`); the patch does not change dependencies. The independent advisory rerun could not acquire the read-only advisory-cache lock (`reviewer-deny.log:1`), so it does not supersede the captured failure. Refresh/reconcile the baseline evidence named in C4 rather than treating this as a matrix implementation defect.; C4-verify — The log shows 48 green tests, then an unexecuted pre-fix test due to the missing matrix API, exit 77; the independent stash/restoration reproduces this exact limitation (`gate-logs/C4-verify.log:7`, `gate-logs/C4-verify.log:14`, `gate-logs/C4-verify.log:181`, `reviewer-red.log:3`). This is the C2 sign-off exception above.; **The 0017 amendment leaves the proposal contradicting itself.** Amended §3 now says every response carries `Server: wyrd/<version>`. It still points the reader to *Dependencies* (`docs/design/proposals/draft/0017-blackbox-validation-tool.md:263-265`). That section still says "**The gateway advertises no version.** No `Server:` header, nothing on the wire … Interim: `--server-version`" (`:873-878`). Before the patch the two passages agreed. Now the new sentence cites a paragraph that says the opposite. Fix: in *Dependencies*, record that #736 landed through #886, or drop the cross-reference. The brief named only `0017:259-261`. If the builder treats `:873-878` as out of scope, decline it with an issue reference rather than leaving it silent.; C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — - cargo deny --all-features --config deny-all-features.toml check advisories: exited with exit status: 1. Deferred, not addressed here: 10 finding(s) that need human judgment, held in deferred-findings.json for the human at handover — not build work
- Failing gate: C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) —   - cargo deny --all-features --config deny-all-features.toml check advisories: exited with exit status: 1
- Full previous attempt preserved in `iteration-v4/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 5 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 4): rebuilding for the implementation-level findings — C4 Verification (red→green) — Accept or require stronger evidence for the declared new-API exception, and assign resolution of the inherited advisory failures — restoration passes 48 tests and all ten injected faults are caught, but there is no executable pre-fix red and full CI remains failed; `reviewer-restored-green.log:56`, `reviewer-mutations.log:3`, `gate-logs/C4-ci.log:9356`.; **A required row can be passed without any check, and no test stops that regression.** `crates/validate/src/matrix.rs:1178-1180` makes `Exercise::Other` on a `required` row unsettled. That arm is the only thing stopping a generic exercise from passing a required row. I replaced it with `Exercise::Other { .. } => Ok(())`. The suite stayed 48/48 green, and `Other { operation: PutObject, answer: 200 + strong ETag }` then classified as `Pass` against `PRE_0_1` with no status, ETag, early-answer or body check. Under that change the arm never reads the answer, so any answer passes. 743.3 picks which `Exercise` variant to build, so a mapping slip there would quietly bypass every (B′) check. Fix: add a fixture asserting `classify(row(PutObject), &Exercise::Other { operation: PutObject, answer: <good 200> })` is `Unsettled`, plus a verdict-level case in which that run is INCONCLUSIVE.; **Two of the bulk-delete pass conditions have no test.** (1) `matrix.rs:1218-1220` (the empty-keys guard): with it removed, `DeleteObjects { keys: [], 200 + empty <DeleteResult>, follow_ups: [] }` returns `Pass`, which is a vacuous pass (the rubric's *Absent entries* class: "a count-based assertion that can pass while the property fails"). (2) `matrix.rs:1291-1295` (`<Deleted>` must name every requested key, as brief (B′) says at `brief.md:55`): with it removed, keys `[p/a, p/b]`, a `<Deleted>` naming only `p/a`, no `<Error>` row, and both `HEAD`s 404 returns `Pass`. In both cases the suite stays 48/48 green. Every bulk fixture either names all keys under `<Deleted>` or carries an `<Error>` row (`capability_matrix.rs:91-96`, `:110-123`, `:683-695`). Fix: add those two fixtures.; **Six more guards can be removed with the suite green. Each has a concrete case:** `matrix.rs:942-946` (unsupported row, declared status, then a torn body → unsettled): removed, `CopyObject` 501 + `Body::Failed` becomes fatal `protocol`. That is the "torn answer made fatal" class iteration 1 fixed for the 304 row, now unguarded on every unsupported row. `matrix.rs:1316-1318` (the unit must be `bytes`): removed, `Content-Range: items 2-5/16` passes. The rubric's *Grammar strictness* class names units, and `capability_matrix.rs:566-573` only checks that `Bytes` is accepted, never that another unit is refused. `matrix.rs:1106` (206 with no `Content-Range` → `protocol`): changed to `None => {}`, a 206 with the right bytes and no `Content-Range` passes. `matrix.rs:1159` (listing 200 without a listing document) and `:1274` (bulk-delete 200 without a `<DeleteResult>`): changed to `Ok`, both pass. `matrix.rs:1408` (an exercise the profile declares no row for → `Undeclared`): removed, the run goes from INCONCLUSIVE to PASS. Fix: one fixture per branch. I wrote all ten cases as a probe file. Production passes every one, so they are pure additions with no code change.; C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — - cargo deny --all-features --config deny-all-features.toml check advisories: exited with exit status: 1. Deferred, not addressed here: 6 finding(s) that need human judgment, held in deferred-findings.json for the human at handover — not build work
- Failing gate: C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) —   - cargo deny --all-features --config deny-all-features.toml check advisories: exited with exit status: 1
- Full previous attempt preserved in `iteration-v5/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 6 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 5): rebuilding for the implementation-level findings — C2 Reproduction (red pre-fix) — Accept the declared new-API exception or require another discriminator — production stashed with the submitted test retained fails to compile, so no behavioral pre-fix red executes; all ten required fault injections are caught (`brief.md:138`, `reviewer-red.log:260`, `reviewer-mutations.log:2`, `reviewer-mutation-retries.log:2`).; C4 Verification (red→green) — Require full-CI evidence on the intended refreshed base — 53 restored tests pass, but both advisory scans still fail on unchanged ctutils/tokio versions despite the tracked baseline fixes; this is a baseline/evidence caveat, not a matrix defect (`reviewer-final-green.log:60`, `target/Cargo.lock:1000`, `target/Cargo.lock:3953`, `reviewer-deny.log:314`, `reviewer-deny-all-features.log:358`, `brief.md:235`).; **Two guards can be deleted with the suite still 53/53 green.** I ran both mutants. (a) `crates/validate/src/matrix.rs:908-914`, the check that the exercise matches the row. With it deleted, `classify(PRE_0_1.row(PutObject), &Exercise::GetObject { object, 200 + the right bytes })` returns `Pass`, so a GET answer passes the PUT row. `verdict` cannot reach this, because it looks the row up from the exercise. But `classify` is `pub`, and 743.3 may call it directly. Coverage agrees: lines 909 and 911 never ran (`gate-logs/C4-diff-cov.log`, the MISS list). (b) `matrix.rs:990-992`, which makes "a document where object bytes were expected" a `protocol` failure. Changed to `Ok(&[])`, a GET of an empty object answered `200` with a `Body::Listing` passes. For a non-empty object the failure class quietly changes from `protocol` to `integrity`. Line 991 is also on the MISS list. Fix: add one fixture for each, beside `capability_matrix.rs:538-546` and `:677-697`.; **A vacuous PASS is still possible at the profile level** (low priority: `PRE_0_1` cannot hit it today). `verdict` returns PASS whenever nothing is fatal and nothing is unproved (`matrix.rs:1436-1442`). A profile with no `required` or `unsupported` rows therefore passes an empty run. I probed it two ways: a profile holding only `not_checked(UploadPart)`, and a profile with no rows. Both give `verdict(&p, &[])` = PASS. Separately, `Profile::new` (`matrix.rs:250-252`) accepts a profile that declares the same operation twice, and `Profile::row` (`:267-269`) silently uses the first row. A profile declaring `PutObject` both `required` and `unsupported` passes a PUT 200. The no-duplicates check exists only for `PRE_0_1` (`capability_matrix.rs:314`). This is the rubric's "count-based assertion that can pass while the property fails" class. Fix: either make "zero proved rows" INCONCLUSIVE, or add a test over `PROFILES` requiring every profile to have at least one checked row and no duplicate operations.; C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — - cargo deny --all-features --config deny-all-features.toml check advisories: exited with exit status: 1. Deferred, not addressed here: 9 finding(s) that need human judgment, held in deferred-findings.json for the human at handover — not build work
- Failing gate: C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) —   - cargo deny --all-features --config deny-all-features.toml check advisories: exited with exit status: 1
- Full previous attempt preserved in `iteration-v6/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 7 — carry-forward (from the previous attempt)
- Sign-off rationale: Revise the brief. The rule (the human's): the server must respond with the defined response; that is what the verify is for. Implementation errors are reported as FAIL. So any answer other than the defined one is a FAIL on every row, required as well as unsupported, including a 5xx (other than a declared 501). Today a 5xx on a required row is treated as "availability" (unsettled, INCONCLUSIVE). Settle the class in the brief, and amend 0017 §7's availability row to match; it currently budgets every 5xx. Correct the rustdoc that says a 5xx "neither passes nor fails" (matrix.rs:26-27, :863-864) to match the rule. Carry forward, already decided at this sign-off: keep the strict retry rule, the rc-tag profile selection and the empty-listing rule as built; the inherited cargo-deny red is not this bundle's to fix (base 93a6f37 lacks #950/#954); the new-API red->green exception is accepted. Also fix in the rebuild: tests for the three unguarded pass conditions (GET 200 with an empty body -> integrity; a listing naming a key twice -> oracle; <Deleted> naming a key twice -> protocol). Still open for Plan to settle (not decided at this sign-off): the conditional-GET negative control (matrix records no sent tag; here or #939), whole-body Vec<u8> vs large size classes, protocol vs oracle for a 404 on a just-written key, the observation contract and early-PUT protocol class. Keep the patch under the 100 KB backstop (now 101,896 bytes).
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Revise the brief. The rule (the human's): the server must respond with the defined response; that is what the verify is for. Implementation errors are reported as FAIL. So any answer other than the defined one is a FAIL on every row, required as well as unsupported, including a 5xx (other than a declared 501). Today a 5xx on a required row is treated as "availability" (unsettled, INCONCLUSIVE). Settle the class in the brief, and amend 0017 §7's availability row to match; it currently budgets every 5xx.
  Correct the rustdoc that says a 5xx "neither passes nor fails" (matrix.rs:26-27, :863-864) to match the rule.
  Carry forward, already decided at this sign-off: keep the strict retry rule, the rc-tag profile selection and the empty-listing rule as built; the inherited cargo-deny red is not this bundle's to fix (base 93a6f37 lacks #950/#954); the new-API red->green exception is accepted.
  Also fix in the rebuild: tests for the three unguarded pass conditions (GET 200 with an empty body -> integrity; a listing naming a key twice -> oracle; <Deleted> naming a key twice -> protocol).
  Still open for Plan to settle (not decided at this sign-off): the conditional-GET negative control (matrix records no sent tag; here or #939), whole-body Vec<u8> vs large size classes, protocol vs oracle for a 404 on a just-written key, the observation contract and early-PUT protocol class.
  Keep the patch under the 100 KB backstop (now 101,896 bytes).
- Failing gate: C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) —   - cargo deny --all-features --config deny-all-features.toml check advisories: exited with exit status: 1
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 165 mutants tested in 8m: 1 missed, 134 caught, 30 unviable
- Full previous attempt preserved in `iteration-v7/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
