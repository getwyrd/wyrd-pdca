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
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 91.3% — 617 of 676 instrumentable changed lines executed (floor 80%); 676 of 1376 changed lines were instr
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 174 mutants tested in 8m: 3 missed, 139 caught, 32 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_937/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.42s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #937: add the pure, version-selected S3 capability matrix and verdict classifier; one independently reproduced HeadBucket classification defect needs a builder iteration.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief defines falsifiable expectations and the draft taxonomy agrees that evidence of a wrong answer is fatal, including evidence received before a cut (`brief.md:47`; `docs/design/proposals/draft/0017-blackbox-validation-tool.md:508`). |
| C2 Reproduction (red pre-fix) | N/A | Net-new API exception already accepted: retaining the test while stashing production produces an unresolved `matrix` import, not a behavioral red (`brief.md:215`; `reviewer-red-green.log:8`). |
| C3 Change | PASS | The four-file patch stays within the authorized classifier, integration test, module export, and draft-proposal scope (`brief.md:189`; `crates/validate/src/lib.rs:24`; `patch.diff:2659`). |
| C4 Verification (red→green) | FAIL | The 39 supplied tests pass independently, but a separate compiled probe reproduces a wrong HeadBucket verdict; this failure is independent of the inherited CI advisory red (`reviewer-red-green.log:592`; `reviewer-headbucket.log:5`; `crates/validate/src/matrix.rs:840`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Enforce the declared absence of a body for unsupported HeadBucket responses — one received byte already requires fatal protocol, yet the generic cut branch discards that evidence and yields INCONCLUSIVE (`crates/validate/src/matrix.rs:839`; `reviewer-headbucket.log:5`). |
| T1 Structure | PASS | The decision layer preserves the blackbox dependency boundary and makes observations explicit; its only non-std import is the existing, authorized ErrorCode enum (`crates/validate/src/matrix.rs:19`; `crates/validate/src/matrix.rs:512`). |
| T2 Shape | PASS | The patch is 99,724 bytes against the explicit 100,000-byte budget, and formatting, clippy with warnings denied, and spelling checks pass (`brief.md:261`; `reviewer-scanners.log:1`; `reviewer-clippy.log:225`). |
| T3 Runtime | PASS | Classification uses finite synchronous work, with no clocks, spawned tasks, external awaits, or shared mutable state; whole-object memory is the accepted small-object scope (`crates/validate/src/matrix.rs:633`; `crates/validate/src/matrix.rs:997`; `brief.md:199`). |
| T4 Contribution | N/A | Contribution artifacts are intentionally drafted after Check; the substantive audit must rerun at publish, so there is no present contribution verdict to reproduce (`gate-logs/T4-contribution.log:10`). |
| T5 Judgment | PASS | Prior-art checks by affected path corroborate the brief; all 19 closed-unmerged PR file lists have no affected-path match, and the accepted base/red-leg exceptions and tracked scope deferrals remain settled (`reviewer-prior-art.json:13`; `brief.md:186`; `brief.md:264`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Approve the new cut-versus-unreadable observation contract for #939 — pure classifier tests establish local behavior, while the consumer must preserve those facts when mapping client outcomes (`brief.md:288`; `crates/validate/src/matrix.rs:573`). |

Source citations are relative to `$PDCA_TARGET`; review evidence and gate logs are relative to this report's directory. The target is readable and the patch is applied. Its synthetic base represents `93a6f37`, the older integration base explicitly covered by the brief. Reverse-apply validation succeeds after the stash/pop check; the reviewed source was restored.

**One defect prevents causal adequacy.** `classify_unsupported` handles every `Body::Cut` as unsettled before considering `DeclaredCode::NoBody` (`crates/validate/src/matrix.rs:840`). For HeadBucket's declared 400 with no body (`:291`), both `Arrived::Count(1)` and `Arrived::Bytes(b"x")`, with no error-code marker, therefore produce `Unsettled` and a run outcome of `Inconclusive`. They must produce fatal `Protocol` and `Fail`. The empty response passes and a zero-byte cut remains unsettled, as expected. The independently compiled probe links the actual Cargo-built `wyrd_validate` library; its inputs are in `reviewer-headbucket.rs:7` and results in `reviewer-headbucket.log:3`.

The same missing body validation also allows the public API's contradictory combination of `ErrorCode::NoBody` and a complete nonempty `Body::Bytes` to pass (`reviewer-headbucket.log:7`); the cut reproduction above does not rely on contradictory inputs. Apply the existing no-body validation to `DeclaredCode::NoBody` responses before the generic error-document cut handling, and add row/run regressions for empty, zero-byte-cut, and positive-byte-cut responses in both arrival representations. The existing HeadBucket test never supplies a body (`crates/validate/tests/capability_matrix.rs:462`), and the bodiless-response regression omits HeadBucket (`:903`). This is one defect: the three batch-review reports all identify it (`gate-logs/T4-batch-review.log:10`).

**Verification is substantial but does not establish correctness of the omitted case.** Independent runs passed `cargo test --offline -p wyrd-validate --test capability_matrix` (39/39), `cargo clippy --offline -p wyrd-validate --test capability_matrix -- -D warnings`, workspace formatting, spelling on all changed files, docs lint, and docs rendering/link audit (99 pages). Both named external tools, typos and docs-renderer, were actually exercised (`reviewer-scanners.log:5`, `:14`). Stashing production while keeping the integration test produced exit 101 before any test ran; restoring it produced exit 0. This confirms the accepted net-new limitation, not a behavioral red→green (`reviewer-red-green.log:526`, `:595`).

The remaining frozen gate evidence is adjudicated as follows; full CI, coverage, mutation, and TiKV campaigns were not independently repeated:

- **C4-ci: inherited FAIL, not a patch defect.** Both attempts end with the two cargo-deny failures. The log identifies RUSTSEC-2026-0385 and -0386 (`gate-logs/C4-ci.log:6732`, `:6814`, `:7598`). Cargo.lock and both deny configurations are unchanged. The brief already accepts precisely this base-state red (`brief.md:183`); it is not the basis of C4's FAIL above.
- **C4-verify: UNVERIFIABLE, accepted net-new exception.** The frozen run likewise passes 39 tests with the patch and fails compilation without the new module (`gate-logs/C4-verify.log:10`, `:15`).
- **C4-diff-cov: PASS from captured execution.** The instrumented run passes 39 tests and covers 617/676 instrumentable changed lines, 91.3%; 700 other changed lines are unscored (`gate-logs/C4-diff-cov.log:71`, `:144`).
- **C5-mutants: advisory FAIL.** Of 174 mutants, 139 are caught, 32 unviable, and three survive solely in `FailureClass::name`/Display formatting. Those survivors do not demonstrate a classification defect; the independent HeadBucket probe does (`gate-logs/C5-mutants.log:13`).
- **T4-batch-review: FAIL, corroborated and deduplicated.** Its three reports describe the single body-validation defect above (`gate-logs/T4-batch-review.log:10`).
- **T4-contribution: N/A.** Deferred by design to the mandatory publish audit (`gate-logs/T4-contribution.log:10`).
- **host-tikv: PASS from captured compilation.** The log records the actual crate/server clippy commands and successful completion (`gate-logs/host-tikv.log:2`, `:209`).

**Prior art and scope are accounted for.** Path-based closed-PR searches were followed by changed-file checks for the cited merged work and every returned closed-unmerged PR. Relevant merged antecedents include #845/#859 (library), #931/#934 (payload/client changes), and #765 (proposal); none supplies this matrix. None of the 19 closed-unmerged PRs touches any of the four affected paths (`reviewer-prior-art.json`). No capability-probe/load-time workaround is introduced: the guards validate exercise inputs and response evidence. Client mapping, large-object digests, and the binary runner remain in their already-tracked follow-ups. The validation decision above concerns the new observation contract, rather than reopening those scope decisions.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Advisory review — adversary — NOT COMPLETED

<!-- pdca:leaf-status human-empty -->

Failure class: **substantive — needs a human.** The leaf ran but did not yield a usable verdict; do not assume an infra blip. See `check-advisory-adversary.error.log` in this bundle for the captured error.

- NEEDS-HUMAN — advisory leaf 'adversary' did not produce findings (leaf failed: Command '['systemd-run', '--user', '--scope', '--quiet', '--collect', '--property', 'MemoryMax=8G', '--property', 'MemorySwapMax=0', '--property', 'ManagedOOMMemoryPressure=kill', '--', 'claude', '-p', '--agent', 'adversary', '--permission-mode', 'acceptEdits', '--allowedTools', 'Read,Bash,Grep,Glob', '--model', 'opus', '--effort', 'xhigh', '--output-format', 'stream-json', '--verbose']' returned non-zero exit status 1.); re-run it or adjudicate by hand.

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/validate/src/matrix.rs:840`: `classify_unsupported` treats every cut body as unsettled, including `HeadBucket`, whose declared 400 response must have no body. A 400 with `Body::Cut { arrived: Arrived::Count(1), .. }` therefore produces INCONCLUSIVE instead of fatal `protocol`, even though the received byte already proves the answer wrong. Route `DeclaredCode::NoBody` through the existing `expect_no_body` helper (`crates/validate/src/matrix.rs:945`) before the generic cut handling. Extend `head_bucket_passes_on_a_400_with_no_body_only` (`crates/validate/tests/capability_matrix.rs:462`) with both count and byte cut observations: nonempty must fail, zero received must remain unsettled. This consolidates the three duplicate findings in the frozen T4 batch log.

No additional correctness or reuse/simplification/efficiency findings.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Enforce the declared absence of a body for unsupported HeadBucket responses — one received byte already requires fatal protocol, yet the generic cut branch discards that evidence and yields INCONCLUSIVE (`crates/validate/src/matrix.rs:839`; `reviewer-headbucket.log:5`).
- [ ] Validation — fitness-to-purpose — Approve the new cut-versus-unreadable observation contract for #939 — pure classifier tests establish local behavior, while the consumer must preserve those facts when mapping client outcomes (`brief.md:288`; `crates/validate/src/matrix.rs:573`).
- [ ] leaf produced no usable verdict (needs a human) — advisory leaf 'adversary' did not produce findings (leaf failed: Command '['systemd-run', '--user', '--scope', '--quiet', '--collect', '--property', 'MemoryMax=8G', '--property', 'MemorySwapMax=0', '--property', 'ManagedOOMMemoryPressure=kill', '--', 'claude', '-p', '--agent', 'adversary', '--permission-mode', 'acceptEdits', '--allowedTools', 'Read,Bash,Grep,Glob', '--model', 'opus', '--effort', 'xhigh', '--output-format', 'stream-json', '--verbose']' returned non-zero exit status 1.); re-run it or adjudicate by hand.
- [ ] `crates/validate/src/matrix.rs:840`: `classify_unsupported` treats every cut body as unsettled, including `HeadBucket`, whose declared 400 response must have no body. A 400 with `Body::Cut { arrived: Arrived::Count(1), .. }` therefore produces INCONCLUSIVE instead of fatal `protocol`, even though the received byte already proves the answer wrong. Route `DeclaredCode::NoBody` through the existing `expect_no_body` helper (`crates/validate/src/matrix.rs:945`) before the generic cut handling. Extend `head_bucket_passes_on_a_400_with_no_body_only` (`crates/validate/tests/capability_matrix.rs:462`) with both count and byte cut observations: nonempty must fail, zero received must remain unsettled. This consolidates the three duplicate findings in the frozen T4 batch log.
- [ ] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [ ] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) —   - cargo deny --all-features --config deny-all-features.toml check advisories: exited with exit status: 1
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_937/review-b
- [ ] external dependency: cargo deny all-features advisories — the unpatched base `93a6f37` fails both cargo-deny advisory steps of C4-ci on RUSTSEC-2026-0385 / RUSTSEC-2026-0386; the gate's fmt, clippy, build, test (incl. this test), prose and conformance steps are green, so C4-ci cannot read green on this base until the advisory is recorded or cleared on `main`.
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
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Fix the one grading bug the reviewer, code-review leaf and all three batch-review passes found: in `classify_unsupported` (crates/validate/src/matrix.rs, ~:840/:857), a `DeclaredCode::NoBody` row (HeadBucket's declared 400 with no body) must run the existing `expect_no_body` check BEFORE the generic `Body::Cut` -> unsettled arm. A cut after one or more bytes (`Arrived::Count(n>0)` or `Arrived::Bytes(non-empty)`) is fatal `protocol` / run FAIL; a complete non-empty `Body::Bytes` on a NoBody row is also `protocol` (today it passes); an empty body passes; a cut after zero bytes stays unsettled. Add row and run tests for these cases in both arrival forms (extend `head_bucket_passes_on_a_400_with_no_body_only` and include HeadBucket in the bodiless-response test). Keep patch.diff under 100,000 bytes (it is at 99,724): make room by tightening rustdoc that restates the brief or by sharing test helpers, never by dropping a fixture or a check. Everything else stands: cargo-deny red (RUSTSEC-2026-0385/-0386) is inherited from main; the adversary leaf failed on a spend limit, not a finding; the carried-over findings are already handled.
- By / date: Eduard Ralph / 2026-10-19

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 2 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
