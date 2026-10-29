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
    state (D) today. (#508 is open and the denylist at `crates/gateway-s3/src/lib.rs:410-422`
    still refuses `uploads`, `uploadId`, `partNumber` on `main` `9b1a3e2`; #942 staged part
    bytes in core only. If a later base no longer refuses them, the six multipart rows become
    `supported, not checked (#750)` by the rule under *Design*.)
  - **(B) The one rule for every answer (decided 2026-10-24, the human's).** The server must
    give the defined answer; an implementation error is a FAIL. So on **every** row, required
    and unsupported alike, a status the server sent that is not the row's defined one is
    fatal, **a 5xx included** (except where a row declares it, as the `501` rows do). Only
    an exercise that leaves nothing to judge stays open: **no response head** (refused, reset,
    timed out before the head) gives no verdict for that exercise, and **a body cut off after
    its head**, where the check needs that body or its absence, leaves the exercise unsettled,
    unless what arrived before the cut already shows the answer wrong (next paragraph).
    Neither passes nor fails; (E) folds both as not proved. The class of a wrong answer is
    `protocol` unless a check below names another.
    **What arrived before a cut is evidence (plan review 2026-10-24, applying the rule above).**
    A cut never undoes a wrong byte ("It must abort instantly when a byte comes back wrong",
    `0017:484-485`). Where a check compares object bytes, a body cut off after bytes that are
    not a prefix of the expected bytes (a wrong byte, or more bytes than expected) is
    `integrity`. Where the answer carries no body (a PUT's 200, a 304, a `HEAD`, a 204, the
    follow-up `HEAD`'s 404), a body cut off after one byte or more is `protocol`. A cut after a
    correct prefix, or after no byte, stays unsettled. **A body that arrived whole but is not
    the document its status calls for** (an error body that is not one S3 `<Error>` document;
    a 200 whose body is not the listing or `<DeleteResult>` its operation answers with) is an
    answer, not a cut: `protocol` (§7, "XML shape").
  - **(B1) Unsupported rows.** Pass for the declared status and code exactly; `protocol` for
    any other status or code, a 5xx included; **`unexpected-support` for any 2xx**, including
    a 2xx head whose body then failed or timed out; no verdict when no head arrived; unsettled
    when the declared status arrived and its error body was cut off before its code was read;
    `protocol` when the declared status arrived with an error body that arrived whole but is
    not one S3 `<Error>` document.
  - **(B2) Required rows have checks of their own** (0017 §7 classes). Each passes only when:
    - `PutObject`: 200, an `ETag` that is a strong entity-tag (RFC 9110 §8.8.3: `"`, etagc*,
      `"`), so `*`, an unquoted value, a weak `W/"…"` tag and an empty value are each
      `protocol`; the request body had ended when the response head arrived; and no response
      body. A success that arrives before the request body ended is never a receipt, so it is
      `protocol` whatever its `ETag` (the client's rule: `PutObject`'s doc,
      `crates/validate/src/s3.rs:172-176`; `BodyError::AcknowledgedEarly`,
      `crates/validate/src/s3/error.rs:101-114`) **(decided 2026-10-24)**.
    - whole `GetObject`: 200 whose body equals the expected bytes, else `integrity`.
    - ranged `GetObject`: **206** with `Content-Range: bytes a-b/<size>` (unit `bytes`, no
      signs, digits only) and exactly those bytes.
    - **conditional `GetObject` (decided 2026-10-24: the control lives here, not in #939).**
      Each conditional exercise records the `If-None-Match` value it sent and the object's tag
      (the `ETag` its PUT answered). Matching form: settles nothing (unsettled) unless the sent
      value is a strong entity-tag equal to the object's tag; then **304** with no body.
      Non-matching form: settles nothing unless the sent value is a strong entity-tag that
      differs from the object's tag; then **200** with the whole body. This is the negative
      control that tells "compares tags" from "answers 304 to anything": an exercise that did
      not send the right tag cannot pass the row, so a mapping slip in 743.3 reads as
      INCONCLUSIVE, never PASS.
    - `HeadObject`: 200 with the object's size and the PUT's `ETag`, else `oracle`.
    - **A 404 to any `GetObject` form or to `HeadObject` of a key the run wrote is `oracle`,
      not `protocol` (decided 2026-10-24):** the mirror of "a deleted key readable", which §7
      names `oracle` (`0017:490`). Every other wrong status on those rows stays `protocol`.
    - `ListObjectsV2` and v1: 200 naming exactly the keys written under the listed prefix, each
      once, with their sizes, else `oracle`. A listing of a prefix with nothing written is
      unsettled (decided at iteration 7's sign-off, as built).
    - `DeleteObject`: 204 with no body, then a follow-up `HEAD` of the key.
    - `DeleteObjects`: 200 with a `<DeleteResult>` naming every requested key under
      `<Deleted>` exactly once and no `<Error>` row, else `protocol` (the gateway answers 200
      even when keys failed, `crates/gateway-s3/src/lib.rs:2293-2300`); **and then** a
      follow-up `HEAD` of each requested key. The document alone never proves a delete.
    - **The follow-up `HEAD` after a delete:** 404 proves that key deleted; a 2xx is `oracle`
      (a deleted key readable, `0017:490`); **any other status, a 5xx included, is `protocol`
      (decided 2026-10-24; iteration 7 left a 5xx here unsettled)**; no head or a cut-off body
      is unsettled. Every follow-up is judged, a fatal one fails the row even when another
      probe of the same key answered 404, and a probe of a key the delete did not request
      settles nothing.
  - **(B3) Fixtures.** Each giving the named class and making the run FAIL: a 200 with the
    whole body to the ranged GET; a 206 with the wrong `Content-Range`; a 200 to the matching
    conditional GET; a 304 to the non-matching one; a GET body one byte off (`integrity`); **a
    GET 200 with an empty body for a non-empty object (`integrity`)**; a PUT `ETag` of `*`; an
    unquoted one; a PUT 200 with a valid strong `ETag` whose head arrived before the request
    body ended (`protocol`); **a PUT answered `503` (`protocol`)**; **a whole GET answered `500`
    (`protocol`)**; **a `HeadObject` and a whole `GetObject` of a written key answered `404`
    (`oracle`)**; **a listing naming a written key twice (`oracle`)**; a `DeleteObjects` 200
    with one `<Error>` row; **a `<Deleted>` naming a key twice (`protocol`)**; a `DeleteObjects`
    200 naming every key under `<Deleted>` followed by a `HEAD` that answers 200 (`oracle`); a
    `DeleteObject` 204 followed by a `HEAD` that answers 200 (`oracle`); **a `DeleteObject` 204
    followed by a `HEAD` that answers 503 (`protocol`; iteration 7 had this as INCONCLUSIVE)**;
    **a whole GET 200 cut off after bytes that differ from the object's (`integrity`)**; **a
    304 to the matching conditional GET (the right tag sent) cut off after one body byte
    (`protocol`)**; **a `CopyObject` answered its declared `501` with an error body that
    arrived whole but is not an `<Error>` document (`protocol`)**; **a `ListObjectsV2` 200
    whose body arrived whole but is not a listing (`protocol`)**.
    Each giving **INCONCLUSIVE, not FAIL** (every other row passing): a `DeleteObject` 204 whose
    follow-up `HEAD` had no answer; a `DeleteObjects` 200 naming every key under `<Deleted>`
    where one key's `HEAD` had no answer and the rest answered 404; **a non-matching
    conditional GET answered 200 with the whole body whose sent tag equals the object's tag**;
    **a non-matching one whose sent value is `*`**; **a matching conditional GET answered 304
    whose sent tag differs from the object's**; **a whole GET 200 cut off after a correct but
    incomplete prefix** (the pair of the `integrity` fixture above); **a `CopyObject` answered
    `501` whose error body was cut off before its code was read** (the pair of the `protocol`
    one). The verdict names the row in each.
  - **(C) Version keying.** Parsing a reported version gives a release (`MAJOR.MINOR.PATCH`
    with any pre-release; `+` build metadata ignored for selection), **unknown**, or a parse
    error. **Unknown** covers `unknown` (the gateway's default, `wyrd/unknown`,
    `crates/gateway-s3/src/lib.rs:121`) and any version whose build metadata is the build's
    "nothing was known" identity, `+git.unknown` or `+git.unknown.dirty`
    (`crates/server/src/version/derivation.rs:65`, `:121`); it is never read as release `0.0.0`
    **(decided)**. With a test-local table of two profiles covering different ranges, a version
    in each range selects its own profile and its own expectations (a row `unsupported` in one
    and `required` in the other flips the verdict for the same answer). `0.0.0+git.abc1234`
    selects `pre-0.1`; so does `0.1.0-rc.1` (decided at iteration 7's sign-off, as built:
    `pre-0.1` covers every version below `0.1.0`, pre-releases included). A version that parses
    but matches no profile selects none; unknown and unparseable select none.
  - **(D) The third row state, `supported, not checked (#NNN)` (decided):** an operation the
    server is expected to accept and the tool cannot check yet; `#NNN` is the issue that will
    add the check. With a test-local profile holding one such row: a run can be PASS without
    that row exercised; the verdict lists it as not covered, with its issue; an answer fed for
    it is never counted as a pass.
  - **(E) The run verdict.** Folding the row results: any fatal result ⇒ FAIL, whatever else
    passed, naming every fatal row and its class. Else INCONCLUSIVE, naming the rows, when any
    `required` or `unsupported` row was not exercised, or **any** exercise of it had no answer
    or was unsettled (the strict retry rule: a row is proved only when every exercise of it
    passed; decided at iteration 7's sign-off, as built), or an exercise names an operation
    the profile has no row for, or no row was proved at all. Else PASS. A profile that declares
    one operation twice is refused at construction (as built).
- Repo + branch target: getwyrd/wyrd @ main
- Scope: the `matrix` module: operations, rows in three states, immutable profiles with
  `pre-0.1`, version parsing and selection, its own observation type, the per-row
  classification in (A)–(B3), the run verdict (E); `pub mod matrix;` in
  `crates/validate/src/lib.rs` (that one line only); the new test file; and the 0017 §3 and
  §7 amendments under *Design*. The module's rustdoc states (B)'s rule as built: iteration 7's
  module invariant (`matrix.rs:23-29` there, the 5xx phrase at `:26-27`) and the
  `RowResult::Unsettled` doc (`:863-864`) said a 5xx "neither passes nor fails", which the
  rule makes false. / out of scope:
  - sending anything: the client verbs are 743.2; the runner, `--server-version`, reading the
    `Server` header, the occupied-prefix and cleanup rules are 743.3;
  - mapping the client's results to the matrix's observation type (743.3);
  - **a digest-based body for large objects (decided 2026-10-24):** object bodies stay whole
    bytes in this slice. Smoke sends only small sizes (at most one chunk; #939 puts every size
    class out of its scope), and large objects are the large-object scenario's, #761, which
    adds a digest form when it needs one;
  - the rest of `lib.rs` (its module doc's "#743" sentence, `lib.rs:10-11`, is 743.3's),
    `Cargo.toml`, `docs/design/architecture/05-building-block-view.md` (743.3 documents the
    feature);
  - the cross-sweep oracle, event stream and verdict artifact (#746, #747, #764); MinIO's
    reference profile (#751); authentication vectors (#745); the availability budget itself
    (counting and thresholds belong to the scenarios that run long);
  - any gateway change; any `wyrd-*` crate in `[dependencies]`; `Cargo.lock`; `deny*.toml`.

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
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 91.7% — 620 of 676 instrumentable changed lines executed (floor 80%); 676 of 1361 changed lines were instr
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 171 mutants tested in 8m: 1 missed, 140 caught, 30 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_937/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.67s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of #937’s pure, versioned S3 capability matrix: one reproduced GET-size classification defect needs implementation follow-up.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The pure scope, three row states, version selection and fatal-answer policy are explicit and testable; the remaining downstream contract decision is recorded under Validation (brief.md:26; brief.md:189; docs/design/proposals/draft/0017-blackbox-validation-tool.md:505). |
| C2 Reproduction (red pre-fix) | N/A | The accepted net-new exception applies: stashing production while retaining the test produces an unresolved matrix import, not an executed behavioral red (brief.md:215; review-red-green.log:7). |
| C3 Change | PASS | The four changed files stay inside the approved scope and the 99,636-byte patch meets the 100,000-byte limit; the HeadBucket carry-forward is addressed with row and run assertions (crates/validate/src/matrix.rs:827; crates/validate/tests/capability_matrix.rs:468; brief.md:261). |
| C4 Verification (red→green) | PASS | The agreed net-new verification posture is met: restoration passes all 39 tests and targeted checks; full CI remains red solely on the already-accepted inherited advisories, so this is not an all-gates-green claim (review-red-green.log:601; brief.md:186; gate-logs/C4-ci.log:9338). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Make a declared GET size disagreement fatal before treating a cut as unsettled — a known-wrong Content-Length currently yields INCONCLUSIVE for whole, ranged and conditional-miss GETs (crates/validate/src/matrix.rs:889; review-probes.log:6). |
| T1 Structure | PASS | The classifier stays within the permitted pure library seam, with immutable profiles and no new dependency edge; the existing ErrorCode reuse is explicitly authorized (crates/validate/src/matrix.rs:18; crates/validate/src/matrix.rs:182; brief.md:179). |
| T2 Shape | PASS | The plain integration target exercises production functions, strict tag/range grammar and version boundaries; formatting, spelling and draft-doc checks pass (crates/validate/tests/capability_matrix.rs:7; crates/validate/tests/capability_matrix.rs:706; review-docs-render.log:3). |
| T3 Runtime | PASS | No external waits, clocks, shared mutable globals or destructive execution are introduced; actual compilation and execution succeed, with grading correctness qualified by C5 (crates/validate/src/matrix.rs:983; crates/validate/src/matrix.rs:1297; review-green.log:303). |
| T4 Contribution | N/A | Commit/PR artifacts are absent by design and their substantive audit is owed at publish; prior art was independently checked by affected path, and the batch-review defect is consolidated under C5 (gate-logs/T4-contribution.log:10; review-prior-art.json:2; gate-logs/T4-batch-review.log:10). |
| T5 Judgment | PASS | No capability-probe symptom guard or scope expansion was found; the sole surviving mutant removes failure-class display text and does not alter classification, so it remains advisory (crates/validate/src/matrix.rs:739; gate-logs/C5-mutants.log:13). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Confirm the observation contract is fit for #939’s smoke integration, including ownership of the recorded PUT/DELETE unreadable-versus-cut mapping decision — those facts determine FAIL versus INCONCLUSIVE (brief.md:288; brief.md:303; crates/validate/src/s3.rs:399). |

The implementation follow-up is one defect, independently reproduced in four cases. Source citations above and below resolve under `$PDCA_TARGET`; evidence paths resolve in this review directory. The target is the disposable snapshot of base `93a6f37` with the patch applied. All four patch-file checksums were identical after the stash/pop cycle (`review-red-green.log:553`).

**Known size disagreement is lost after a body cut.** `expect_bytes` reads only the body and expected bytes, ignoring `Response::content_length`. A correct prefix therefore takes the unsettled return at `crates/validate/src/matrix.rs:918` even when the response head already contradicts the expected representation size. Whole GET, ranged GET and conditional-miss GET all call this helper (`crates/validate/src/matrix.rs:1009`, `:1037`, `:1062`). The taxonomy makes size disagreement fatal integrity (`docs/design/proposals/draft/0017-blackbox-validation-tool.md:505`). The real streaming client exposes this header and reports premature closure separately (`crates/validate/src/s3/body.rs:431`, `:494`), so these are representable observations.

The standalone `review-probes.rs` links the actual Cargo-built library; it does not copy the classifier. With the repository’s pinned Rust 1.96.0, these assertions fail:

| Exercise | Declared length | Correct bytes before cut | Actual row / run |
|----------|-----------------|--------------------------|------------------|
| Whole GET of 16 bytes | 15 | First 8 bytes | Unsettled / INCONCLUSIVE |
| Whole GET of 16 bytes | 17 | All 16 bytes | Unsettled / INCONCLUSIVE |
| Range 2–5 of 16 bytes | 5 | All 4 requested bytes | Unsettled / INCONCLUSIVE |
| Conditional-miss GET of 16 bytes | 17 | First 8 bytes | Unsettled / INCONCLUSIVE |

Each should be fatal integrity / FAIL. The control with declared length 16 and an 8-byte correct prefix stays unsettled and passes its assertion. Results: four failures, one passing control (`review-probes.log:38`). Reproduce with the compile command at `review-probes.log:1`, then `./review-probes --nocapture --test-threads=1` from this directory. Compare a present declared length against the requested representation length before accepting the cut as unsettled; for a range that is the slice length. Add row/run regressions while preserving the legitimate correct-length and absent-length cut cases. The two reports in `gate-logs/T4-batch-review.log:10` describe this same omission, not two separate defects.

**The available verification supports the rest of the scoped change, with explicit limits.** I independently ran `cargo test --offline --locked -p wyrd-validate --test capability_matrix` before and after stashing production, `cargo clippy --offline --locked -p wyrd-validate --all-targets -- -D warnings`, `cargo fmt --all -- --check`, `git diff --check`, targeted `typos`, docs lint, and the real docs renderer. The restored test target passes 39/39; clippy and the scanners pass; rendering produces 99 pages with a successful link audit (`review-red-green.log:601`, `review-clippy.log:222`, `review-docs-lint.log:1`, `review-docs-render.log:2`). Both external dependencies named in `brief.md:213` were exercised. The stashed leg exits 101 at compilation; it is not asserted as behavioral red→green.

The remaining gate results are adjudicated from their frozen logs, not represented as independent reruns:

- C4-ci: FAIL, with only the two cargo-deny advisory checks failing on RUSTSEC-2026-0385/-0386; this is the inherited base condition already accepted in the brief (`gate-logs/C4-ci.log:8473`, `:8555`, `:9338`; `brief.md:183`). It is not a patch defect.
- C4-verify: UNVERIFIABLE, because the reverted module’s API is absent; its green leg ran 39 tests, matching my rerun (`gate-logs/C4-verify.log:14`, `:544`).
- C4-diff-cov: PASS, 620/676 instrumentable changed lines executed, 91.7%, against an 80% floor (`gate-logs/C4-diff-cov.log:141`).
- C5-mutants: advisory FAIL, 140 caught, 30 unviable, one missed display-only mutant; the log supplies aggregate mutation evidence, not separate traces for all fifteen brief-prescribed edits (`gate-logs/C5-mutants.log:13`).
- T4-batch-review: FAIL; the reported Content-Length omission is reproduced and consolidated above (`gate-logs/T4-batch-review.log:10`).
- T4-contribution: N/A/deferred; substantive checking runs at publish (`gate-logs/T4-contribution.log:10`).
- host-tikv: PASS in the captured log, which shows successful completion of both requested feature compilations (`gate-logs/host-tikv.log:111`, `:209`).

**Prior art was checked mechanically by path.** GitHub main history was queried for each of the four affected files and for `crates/validate/`; neither new matrix file has prior merged history. I fetched changed-file lists for all 19 closed, unmerged PRs returned by the repository query and compared them with both affected path surfaces; none overlap. The queries, commit URLs and complete file lists are retained in `review-prior-art.json`. Existing client/payload work and the draft proposal are the relevant predecessors. Gateway/profile citations were also checked against the supplied target, including the still-active multipart denylist at `crates/gateway-s3/src/lib.rs:410`.

Fitness sign-off remains human-owned. The two new body states distinguish whole malformed documents from interrupted transport; the existing #939 handoff must preserve that distinction. The brief’s recorded PUT/DELETE ambiguity remains assigned to that downstream decision (`brief.md:303`); this review does not request a client change inside #937.
<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — #937 (743.1, the capability matrix), iteration 9

I re-ran the evidence and could not break the core logic. I found two concrete gaps where the
verdict says less than the answer proves. Both are small fixes inside `matrix.rs`.

How I tested: a scratch crate holding a byte-identical copy of `crates/validate/src/matrix.rs`
(checked with `cmp`), a copy of the test file, and a stub `ErrorCode` with the same shape as
`crates/validate/src/s3/error.rs:68-76`. That way I avoided building `wyrd-server`. The suite
passes 39/39 there, matching `gate-logs/C4-ci.log:3179-3181`.

## Findings

- NEEDS-HUMAN [impl] — **The declared `Content-Length` is ignored on every object read, so a
  head that already shows the wrong size reads as "unsettled", not `integrity`.**
  `expect_bytes` (`crates/validate/src/matrix.rs:889-934`, called at `:1009`, `:1037` and
  `:1062`) never looks at `response.content_length`. The field is read in one place only:
  `HeadObject` at `:1067`. Concrete cases I ran against the production code:
  (a) whole GET, `Content-Length: 17` for the 16-byte object, all 16 correct bytes arrive, then
  a cut → `Unsettled { "cut off after 16 correct bytes of 16" }`;
  (b) `Content-Length: 15`, cut after 10 correct bytes → `Unsettled`;
  (c) ranged 206 `bytes 2-5/16` with `Content-Length: 10`, cut after 2 correct bytes →
  `Unsettled`;
  (d) a whole 16-byte correct body under `Content-Length: 15` → `Pass`.
  This is the same defect the gating T4 review found (`gate-logs/T4-batch-review.log:10`). I
  confirm it, and it applies to the ranged and non-matching GETs too, not only the whole GET.
  It breaks the brief's invariant ("never less") and the rubric's *Protocol input* rule
  ("Enforce declared `Content-Length`"). §7 already defines `integrity` as "bytes, digest, or
  size differ". The reviewer might lean on (B)'s line "a cut after a correct prefix … stays
  unsettled", but that line is about the bytes. The brief's *Design* lists `Content-Length`
  among "the headers the checks read", so leaving the header out of every read check does not
  follow from the brief.
  Fix: in `expect_bytes`, if `content_length` is `Some(n)` and `n != want.len()`, return
  `integrity` before the cut arm. Add tests for both arrival forms next to
  `crates/validate/tests/capability_matrix.rs:691-697`; none of those tests sets a
  `Content-Length` today. The patch is at 99,636 of its 100,000-byte budget, so the builder will
  need to trim rustdoc to make room.

- NEEDS-HUMAN [impl] — **If the exercise requests a key twice, `DeleteObjects` hides an `<Error>`
  row or an unreadable document as "unsettled".** `check_delete_result`
  (`crates/validate/src/matrix.rs:1186-1193`) returns early on a duplicate key. That check runs
  before `whole_document` (`:1194`) and the `<Error>` check (`:1199`), and neither of those
  needs to match keys one by one. Concrete cases: keys `["p/a","p/a"]`, a 200 whose
  `<DeleteResult>` has an `<Error>` row for `p/a`, follow-up `HEAD` 404 →
  `Unsettled { "the exercise requested key \"p/a\" twice" }`, where (B3) makes an `<Error>` row
  `protocol`. Same keys with a 200 `Body::Unreadable` → `Unsettled`, where (B) says `protocol`
  ("XML shape"). It takes a mapping slip in 743.3 to reach this, and the result is INCONCLUSIVE,
  never PASS, so the severity is low. Still, a fatal fact gets hidden. Fix: run the document
  and `<Error>` checks first, and keep the duplicate guard only in front of the `<Deleted>`
  key-by-key match. Add a case beside `crates/validate/tests/capability_matrix.rs:986`.

- NEEDS-HUMAN [human] — **The open 743.3 case in the brief can be partly settled by the matrix
  today.** The brief leaves "a PUT … 2xx … a body framed and then cut before its first byte"
  for a human to assign to 743.3. The matrix already has the fact that decides it when the head
  carries a length: PUT 200, strong `ETag`, `Content-Length: 5`, cut after 0 bytes →
  `Unsettled` (`crates/validate/src/matrix.rs:943-944`, reached from `:1004`). A success that
  declares a body has a body, which is `protocol`. Whether the matrix should judge this from
  the header (and so close the length-framed half of the open case), or leave it all to 743.3,
  is a scope call. It does nothing for chunked framing, which declares no length.

- `FailureClass`'s `Display` (`crates/validate/src/matrix.rs:737-746`) is the one surviving
  mutant (`gate-logs/C5-mutants.log:13`). No test pins the §7 class names (`integrity`,
  `oracle`, `unexpected-support`, `protocol`) that 743.3's report will print. One
  `assert_eq!(Protocol.to_string(), "protocol")`-style line per class would kill the mutant.
  This is a nit and not blocking; I checked by hand that the strings are correct today.

## What I tried and could not refute

- **The evidence.** C4-verify is UNVERIFIABLE (`gate-logs/C4-verify.log`), but that is the
  net-new exception the brief accepts. To stand in for it, I made 23 hand-written changes to my
  copy of the module: all 15 reds the brief requires, plus the iteration-8 HeadBucket ordering
  bug (moving `expect_no_body` back after the cut arm at `matrix.rs:827-835`), a 5xx on the
  follow-up `HEAD` read as unsettled, the hit-form tag guard removed, a cut-off listing called
  `protocol`, an empty listing passing, the strict-retry rule removed, and the vacuity rule
  removed. **Every one turned the suite red**, each in the test named for that leg. The tests
  call `wyrd_validate::matrix` directly, not a copy, and none of them passes for the wrong
  reason that I could find.
- **The rules.** I attacked the 5xx-is-`protocol` rule on every row (`:867-874`, `:1133-1135`),
  404 → `oracle` on reads only (`:878-885`), the conditional-GET tag guards (`:1040`, `:1055`),
  the early PUT acknowledgement (`:991-993`, which fires before the ETag check), the NoBody
  fix (`:827-829`), and the `<Deleted>`/probe folding (`:1142-1183`, including retries and
  probes of keys that were never requested). None of them broke.
- **The grammars.** `parse_version` handles `+git.unknown[.dirty]` after a pre-release, signs,
  leading zeros, empty identifiers and u64 overflow. `0.1.0-rc.1` selects `pre-0.1`.
  `parse_content_range` refuses signs, `*` and a missing size. `is_strong_etag` refuses `*`,
  `W/`, unquoted values, inner quotes and DEL. All of these hold.
- **The citations.** Every `pre-0.1` citation matches the gateway source in the target tree
  (`crates/gateway-s3/src/lib.rs:121`, `:410-422`, `:1693-1699`, `:1766-1788`, `:1797-1815`,
  `:1819-1826`, `:1842-1849`, `:1865-1871`, `:2008-2028`, `:2293-2300`, `:2713-2719`,
  `:2785-2803`, `:2995`, `:3401`).
- **The C4-ci red.** It is only the inherited `cargo deny` advisories (RUSTSEC-2026-0385/-0386,
  `gate-logs/C4-ci.log:3856-3945`), which the human accepted. The matrix test is green in the
  same run.
<!-- pdca:leaf-complete -->

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/validate/src/matrix.rs:889`: `expect_bytes` ignores `response.content_length`. For a known 16-byte object, a GET declaring 17 bytes and then cutting off after the 16 correct bytes returns `Unsettled`; declaring 15 bytes and cutting off after eight correct bytes does too. The head already establishes a size mismatch, so these should be fatal `integrity`, making the run FAIL instead of INCONCLUSIVE. Whole, ranged, and non-matching conditional GETs share this omission. Check any present length against `want.len()` before handling the cut, preserving `None` for chunked responses, and add row/run regressions for both shorter and longer declarations. This consolidates the two equivalent frozen batch-review findings.

No additional correctness, reuse, simplification, or efficiency findings.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Make a declared GET size disagreement fatal before treating a cut as unsettled — a known-wrong Content-Length currently yields INCONCLUSIVE for whole, ranged and conditional-miss GETs (crates/validate/src/matrix.rs:889; review-probes.log:6).
- [ ] Validation — fitness-to-purpose — Confirm the observation contract is fit for #939’s smoke integration, including ownership of the recorded PUT/DELETE unreadable-versus-cut mapping decision — those facts determine FAIL versus INCONCLUSIVE (brief.md:288; brief.md:303; crates/validate/src/s3.rs:399).
- [ ] **The declared `Content-Length` is ignored on every object read, so a head that already shows the wrong size reads as "unsettled", not `integrity`.** `expect_bytes` (`crates/validate/src/matrix.rs:889-934`, called at `:1009`, `:1037` and `:1062`) never looks at `response.content_length`. The field is read in one place only: `HeadObject` at `:1067`. Concrete cases I ran against the production code: (a) whole GET, `Content-Length: 17` for the 16-byte object, all 16 correct bytes arrive, then a cut → `Unsettled { "cut off after 16 correct bytes of 16" }`; (b) `Content-Length: 15`, cut after 10 correct bytes → `Unsettled`; (c) ranged 206 `bytes 2-5/16` with `Content-Length: 10`, cut after 2 correct bytes → `Unsettled`; (d) a whole 16-byte correct body under `Content-Length: 15` → `Pass`. This is the same defect the gating T4 review found (`gate-logs/T4-batch-review.log:10`). I confirm it, and it applies to the ranged and non-matching GETs too, not only the whole GET. It breaks the brief's invariant ("never less") and the rubric's *Protocol input* rule ("Enforce declared `Content-Length`"). §7 already defines `integrity` as "bytes, digest, or size differ". The reviewer might lean on (B)'s line "a cut after a correct prefix … stays unsettled", but that line is about the bytes. The brief's *Design* lists `Content-Length` among "the headers the checks read", so leaving the header out of every read check does not follow from the brief. Fix: in `expect_bytes`, if `content_length` is `Some(n)` and `n != want.len()`, return `integrity` before the cut arm. Add tests for both arrival forms next to `crates/validate/tests/capability_matrix.rs:691-697`; none of those tests sets a `Content-Length` today. The patch is at 99,636 of its 100,000-byte budget, so the builder will need to trim rustdoc to make room.
- [ ] **If the exercise requests a key twice, `DeleteObjects` hides an `<Error>` row or an unreadable document as "unsettled".** `check_delete_result` (`crates/validate/src/matrix.rs:1186-1193`) returns early on a duplicate key. That check runs before `whole_document` (`:1194`) and the `<Error>` check (`:1199`), and neither of those needs to match keys one by one. Concrete cases: keys `["p/a","p/a"]`, a 200 whose `<DeleteResult>` has an `<Error>` row for `p/a`, follow-up `HEAD` 404 → `Unsettled { "the exercise requested key \"p/a\" twice" }`, where (B3) makes an `<Error>` row `protocol`. Same keys with a 200 `Body::Unreadable` → `Unsettled`, where (B) says `protocol` ("XML shape"). It takes a mapping slip in 743.3 to reach this, and the result is INCONCLUSIVE, never PASS, so the severity is low. Still, a fatal fact gets hidden. Fix: run the document and `<Error>` checks first, and keep the duplicate guard only in front of the `<Deleted>` key-by-key match. Add a case beside `crates/validate/tests/capability_matrix.rs:986`.
- [ ] **The open 743.3 case in the brief can be partly settled by the matrix today.** The brief leaves "a PUT … 2xx … a body framed and then cut before its first byte" for a human to assign to 743.3. The matrix already has the fact that decides it when the head carries a length: PUT 200, strong `ETag`, `Content-Length: 5`, cut after 0 bytes → `Unsettled` (`crates/validate/src/matrix.rs:943-944`, reached from `:1004`). A success that declares a body has a body, which is `protocol`. Whether the matrix should judge this from the header (and so close the length-framed half of the open case), or leave it all to 743.3, is a scope call. It does nothing for chunked framing, which declares no length.
- [ ] `crates/validate/src/matrix.rs:889`: `expect_bytes` ignores `response.content_length`. For a known 16-byte object, a GET declaring 17 bytes and then cutting off after the 16 correct bytes returns `Unsettled`; declaring 15 bytes and cutting off after eight correct bytes does too. The head already establishes a size mismatch, so these should be fatal `integrity`, making the run FAIL instead of INCONCLUSIVE. Whole, ranged, and non-matching conditional GETs share this omission. Check any present length against `want.len()` before handling the cut, preserving `None` for chunked responses, and add row/run regressions for both shorter and longer declarations. This consolidates the two equivalent frozen batch-review findings.
- [ ] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [ ] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) —   - cargo deny --all-features --config deny-all-features.toml check advisories: exited with exit status: 1
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_937/review-b
- [ ] external dependency: cargo deny all-features advisories — the unpatched base `93a6f37` fails both cargo-deny advisory steps of C4-ci on RUSTSEC-2026-0385 / RUSTSEC-2026-0386; the gate's fmt, clippy, build, test (incl. this test's 39 legs), prose and conformance steps are green, so C4-ci cannot read green on this base until the advisory is recorded or cleared on `main`.
- [ ] builder-declared external dependency `cargo deny all-features advisories` was REFUTED deterministically — its detect cmd (`cd "${WYRD_REPO:-../wyrd}" && { cargo fetch --locked >/dev/null 2>&1 || exit 3; } && cargo deny --frozen --all-features --config deny-all-features.toml check advisories >/dev/null 2>&1`) exited 0, so the dependency is present and full Check ran (#341); weigh the builder's claim against the review before accept
- [ ] **An interrupted body can already prove corruption.** `brief.md:46-48` says a cut-off body leaves a body-dependent exercise unsettled, and `brief.md:247-248` describes body bytes *or* the fact that the body failed. Consider a GET expecting `abc`: its 200 response delivers `x`, then the connection resets. The missing tail cannot undo the demonstrated wrong byte, yet the specified incomplete-body outcome is INCONCLUSIVE. The target proposal explicitly says “It must abort instantly when a byte comes back wrong” (`$PDCA_TARGET/docs/design/proposals/draft/0017-blackbox-validation-tool.md:484-485`), and the client exposes pieces before a subsequent transport error (`$PDCA_TARGET/crates/validate/src/s3/body.rs:477-498`). Revise the observation contract to retain partial evidence and make an established fatal mismatch dominate interruption. Require paired fixtures: wrong prefix then reset → `integrity`/FAIL; correct but incomplete prefix then reset → INCONCLUSIVE. The current fixtures can pass without distinguishing these cases.
- [ ] **The approved observation contract leaves a necessary client distinction unassigned.** `brief.md:245-255` delegates mapping client results to #939 while calling the contract independent of #938; `brief.md:267-273` requires malformed XML to be fatal but transport interruption to remain budgetable. On the supplied target, both a fully received malformed error document and a body that fails mid-read become `S3Error::Unreadable` (`$PDCA_TARGET/crates/validate/src/s3.rs:380-403`). Its typed fields contain only status, request ID and diagnostic text (`$PDCA_TARGET/crates/validate/src/s3/error.rs:26-38`), despite the stated requirement to classify by matching variants, “never by reading a message” (`:3-4`). Thus #939 cannot distinguish a malformed complete 501 from an interrupted 501 using the current typed facts; the first must be `protocol`, the second unsettled. Specify distinct pure observation states and identify the sibling client change that will supply that distinction before declaring the mapping contract settled. Add one fixture for each case; do not silently make diagnostic-string parsing or a client API change #939's hidden task.
- [ ] **The delete acceptance rule turns missing evidence into an oracle failure.** `brief.md:47-51` requires a successful delete followed by HEAD 404, “else `oracle`”; `brief.md:74-77` only explicitly makes rows exercised “only with no answer” INCONCLUSIVE. For DELETE 204 followed by a HEAD timeout, an answer did arrive, but there is no evidence that the deleted key remains readable. The target distinguishes no-response and timeout outcomes (`crates/validate/src/s3/error.rs:36-47`), and proposal 0017 distinguishes transport availability from oracle disagreement and requires unresolved probes to remain INCONCLUSIVE (`docs/design/proposals/draft/0017-blackbox-validation-tool.md:489-493`, `:520-524`). Revise B′/E to define incomplete multi-observation checks separately from contradictory answers. Require deterministic fixtures for successful single/bulk delete followed by an unavailable HEAD → INCONCLUSIVE, alongside HEAD 200 → fatal oracle.
- [ ] **The PUT criterion omits the existing early-acknowledgement invariant.** `brief.md:38-39` checks 200 plus a strong ETag; the observation contract at `brief.md:150-154` describes response headers and response-body outcomes, without requiring preservation of request-source completion. A premature 200 with a valid ETag and a complete empty response body meets those stated checks even while the upload is unfinished. The target explicitly says a PUT is a receipt only when its source had ended before the response head (`crates/validate/src/s3.rs:161-170`) and rejects premature success at `:235-243`; its regression test asserts this at `crates/validate/tests/s3_client_upload_peers.rs:1207-1217`. Preserve that distinction in the matrix-owned observation and add a pure early-ack fixture with a valid ETag that cannot pass. Deferring client-to-observation mapping to #939 does not specify how this fact survives the boundary.
- [ ] **(Low; the outcome is FAIL either way.) A 404 to the HEAD or GET of a key the run just wrote is classed `protocol`, not `oracle`.** Probe on production: `HeadObject` answered `404` gives `Fatal { Protocol, "answered 404; expected 200" }`, and the same holds for `GetObject`. The cause is `expect_status` (`crates/validate/src/matrix.rs:972-981`) running before the oracle checks (`:1124`, `:1081`). Brief (B′) says HeadObject is "200 with the object's size and the PUT's `ETag`, else `oracle`". §7 (`0017:505`) defines `oracle` as "model and server disagree", and a written key that reads as absent fits that. But §7's `protocol` row ("wrong status") also fits. Only the class label in the report changes, and later scenarios that key on the class (#746) will inherit whichever reading is chosen. A human should pick one.

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
- Iteration delta (if iterating): Re-plan around the 0017 §7 failure classes, not individual cases. Nine attempts each fixed the last edge case and review found the next one in the same grading code (iteration 9: declared Content-Length ignored on GET reads before a cut, matrix.rs:889; DeleteObjects duplicate-key guard runs before the <Error> and document-shape checks, matrix.rs:1186). The patch is at 99,636 of 100,000 bytes, so case-by-case patching cannot converge. Next brief: state each class (integrity, protocol, oracle, unexpected-support, unsettled / no verdict) as one rule that applies to every row: what evidence triggers it, and the order of precedence (a fatal fact already shown in the head or the bytes that arrived beats a cut, and beats any guard that only makes the result unsettled). Then check each row against those class rules, and pick fixtures per class, not per case. Consider splitting the slice (e.g. profiles + version selection + unsupported rows + verdict fold / required-row checks) if the class rules still don't fit the budget. Open scope call to settle in Plan: whether the matrix judges a PUT 2xx that declares a Content-Length and is cut off (length-framed case), or 743.3 owns it. Inherited cargo-deny red (RUSTSEC-2026-0385/-0386) is not a patch defect; rebasing onto current main may clear it.
- By / date: Eduard Ralph / 2026-10-19

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 2 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
