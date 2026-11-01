# Result — issue 1005 / validate-capability-matrix-core

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: `wyrd-validate` has no capability matrix. Nothing declares, per S3 operation,
  whether a Wyrd release must support it or must refuse it with an exact status and code;
  nothing picks those expectations by the server's version; nothing defines what an observed
  answer looks like to a judge; and nothing folds a set of judged exercises into a run verdict.
  Smoke (743.3) and every later scenario need all of it, and none of it needs the network.
- Success criterion: the `matrix` module exists, is pure (no I/O, network, clock, SDK or
  client error types), and the NEW file `crates/validate/tests/capability_matrix.rs` shows
  (A)–(E) and (H) below, with (B)–(D) applied to `unsupported` and `supported, not checked`
  rows and to the two facts that hold for every row. A `required` row's full checks are
  child-2's: HERE, a `required` row's exercise that got a response head is `protocol` on a 5xx
  (unless the row declares that status) and is otherwise **unsettled**; one with no head has
  no verdict. This file's tests assert nothing else about a `required` row's answer, so child-2
  never edits this file.
  - **(A) Today's profile.** `pre-0.1` covers every version below `0.1.0`, pre-releases
    included (**decided 2026-10-19**, see the §3 amendment under *Design*), and declares exactly these rows, each citing the gateway line that gives today's
    answer. `required`: `PutObject`; `GetObject` whole, ranged (`Range: bytes=a-b`),
    conditional with a matching `If-None-Match`, conditional with a non-matching one;
    `HeadObject`; `DeleteObject`; `DeleteObjects`; `ListObjectsV2`; `ListObjects` (v1).
    `unsupported`: `CopyObject` → `501 NotImplemented` (#766); `CreateMultipartUpload`,
    `UploadPart`, `CompleteMultipartUpload`, `AbortMultipartUpload`, `ListParts`,
    `ListMultipartUploads` → `501 NotImplemented` (#508); `GetBucketLocation` → `501
    NotImplemented` (#511); `CreateBucket`, `DeleteBucket`, `ListBuckets` → `400
    InvalidRequest` (#511); `HeadBucket` → `400` with no body (#511). No row is in the third
    state today. (#508 is open; the denylist `crates/gateway-s3/src/lib.rs:410-422` still
    refuses `uploads`, `uploadId`, `partNumber` on `main` `9b1a3e2`. If a later base no longer
    refuses them, the six multipart rows become `supported, not checked (#750)` by the rule
    under *Design*.) A profile declaring one operation twice is refused at construction.
  - **(B) The order of precedence — one ladder for every exercise of every row.** An exercise
    carries its evidence in message order: what it asked (as recorded), the response head
    (status, then headers), the body (whole, or cut with what arrived before the cut), and any
    follow-up request, each its own answer. It is judged in this order:
    1. **Another row's exercise** shows nothing about this row: unsettled.
    2. **No response head** on the exercise's own request: no verdict. Follow-ups are not judged.
    3. **Fatal.** Every check the row's evidence table names runs on everything that arrived.
       If any fails, the result is fatal, with the class of the **first failing check in
       message order**: request-side facts, status, headers in table order, body, follow-ups
       in their order **(decided 2026-10-25)**. Nothing that only leaves the exercise unproved
       (a cut, an ask that cannot test the row, a follow-up with no answer) stops a later check
       from running.
    4. **Unsettled**, only when step 3 found nothing, and something left the row unproved.
    5. **Pass**, when every check passed and nothing was left unproved.

    The order is written once and every row goes through it, so child-2 adds the `required`
    rows' checks without re-deciding the order (how the checks are represented is Do's). In
    THIS child the `unsupported` rows (D), `HeadBucket` and the `supported, not checked` rows
    are judged in full; a `required` row is judged by the 5xx fact and is otherwise unsettled
    ("required-row checks not yet defined").
  - **(C) The classes, each one rule over every row** (0017 §7, as amended under *Design*).
    Declared in full here — `unexpected-support`, `oracle`, `integrity`, `protocol`, unsettled,
    no verdict — as the public `FailureClass` / row-result types, so child-2 adds no variant.
    Shown by this child's tests on the rows it judges:
    - **`unexpected-support`:** an `unsupported` row answered any 2xx, whatever its body did
      after (a cut, an unreadable document or a clean `<Error>` after a 2xx head changes nothing).
    - **`protocol`:** on an `unsupported` row, a status other than the declared one (a 5xx
      included); a declared error code other than the row's, or none; a body that arrived
      whole and is not one S3 `<Error>` document; on `HeadBucket`, any byte (whole, or before a
      cut) on its `400`.
    - **Unsettled** (step 4): an `<Error>` body cut off before its code could be read, on the
      declared status; a `required` row's exercise that got a head and is not a 5xx (this
      child); another row's exercise.
    - **No verdict** (step 2): the exercise's own request got no response head.
    - **Two facts hold for every exercise that got a head, whatever was asked:** a 5xx is
      `protocol` unless the row declares that status — shown here on a `required`, an
      `unsupported` and a `supported, not checked` row; a 404 to a typed read of a written key
      is `oracle` (a typed read is a `GetObject` form's or `HeadObject`'s own request, never a
      delete's follow-up `HEAD`) — child-2's to build and show, but the `oracle` and `integrity` variants
      exist here. **On a `supported, not checked` row: a 5xx is `protocol`; anything else
      there is "not checked", never a pass (decided 2026-10-25).**
    - The four fatal classes print exactly as §7 names them (`integrity`, `oracle`,
      `unexpected-support`, `protocol`), asserted here: #939's report prints them, and
      iteration 9's one surviving mutant was that text.
  - **(D) Unsupported rows.** Pass only on the declared status and the declared code exactly,
    or, for `HeadBucket`, the declared status with no body. A cut-off error body on the declared
    status is unsettled. Everything else follows (B) and (C). The evidence an `unsupported` row
    reads, and no other: the status; the body as one S3 `<Error>` with the declared code
    (`HeadBucket`: no body). Not read: any header; `Content-Length` on an error response.
  - **(E) Version keying.** Parsing a reported version gives a release (`MAJOR.MINOR.PATCH`
    with any pre-release; `+` build metadata ignored for selection), **unknown**, or a parse
    error. **Unknown** covers `unknown` (`crates/gateway-s3/src/lib.rs:121`) and build metadata
    `git.unknown` or `git.unknown.dirty` (`crates/server/src/version/derivation.rs:65`,
    `:121`); it is never read as release `0.0.0`. With a test-local table of two profiles over
    different ranges, a version in each range selects its own profile, and a row `unsupported`
    in one and `required` in the other flips the verdict for the same answer: the declared
    `501 NotImplemented` passes the `unsupported` row and is `protocol` on the `required` one
    (a 5xx that row does not declare). That flip holds before and after child-2; a 2xx would
    not (it is unsettled on a `required` row here and judged in full by child-2), so the test
    does not use one. `0.0.0+git.abc1234`, `0.1.0-rc.1` and `0.1.0-rc.1+git.3.abc1234` (a
    build past an rc tag) select `pre-0.1`. A version matching no profile, unknown, and unparseable each select
    none, and say which.
  - **(H) The run verdict.** Any fatal result ⇒ FAIL, whatever else passed, naming every
    fatal row and its class. Else INCONCLUSIVE, naming the rows, when any `required` or
    `unsupported` row was not exercised, or **any** exercise of it had no verdict or was
    unsettled (a row is proved only when every exercise of it passed), or an exercise names an
    operation the profile has no row for, or no row was proved. Else PASS. A `supported, not
    checked (#NNN)` row is listed as not covered with its issue, and a run can PASS without it.
    In this child a run against `pre-0.1` is INCONCLUSIVE at best; this file does not assert
    that, because child-2 makes it PASS. The PASS leg of the fold is shown against a test-local
    profile of `unsupported` rows and one `supported, not checked` row. Where a test needs an
    unproved `required` row, it uses an exercise that stays unproved after child-2: one with no
    response head, or one that carries no check of its own. The full passing `pre-0.1` run is
    child-2's.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: the `matrix` module: operations, rows in three states, immutable profiles with
  `pre-0.1`, version parsing and selection, the whole observation type of *Design* (every
  field, including the ones only child-2 reads), the public class and result types, the
  ladder (B) with the `unsupported` / `HeadBucket` / `supported, not checked` checks and the
  5xx fact, the run verdict (H); `pub mod matrix;` in `crates/validate/src/lib.rs` (that one
  line only); the test file; the 0017 §3 and §7 amendments under *Design*. / out of scope:
  - every check of a `required` row beyond the 5xx fact — the evidence table, the object-bytes,
    header, document, follow-up and listing checks, the unfit-ask rules and the 404 fact: all
    child-2 (`crates/validate/tests/capability_matrix_required.rs`); this child's `required`
    entry says "unsettled" and leaves a seam child-2 fills without changing a public type;
  - sending anything: the client verbs are #955/#956; the runner, `--server-version`, reading
    the `Server` header, occupied-prefix and cleanup rules are #939;
  - mapping the client's results onto the observation type (#939), including telling the base
    client's one `S3Error::Unreadable` apart for a chunked PUT/DELETE 2xx;
  - a digest-based body for large objects (#761 adds one when it needs it);
  - the rest of `lib.rs` (its "#743" sentence, `lib.rs:10-11`, is #939's); `Cargo.toml`;
    `docs/design/architecture/05-building-block-view.md`;
  - the cross-sweep oracle, event stream and verdict artifact (#746, #747, #764); MinIO's
    profile (#751); authentication vectors (#745); the availability budget itself;
  - any gateway change; any `wyrd-*` crate in `[dependencies]`; `Cargo.lock`; `deny*.toml`.

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
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 89.4% — 390 of 436 instrumentable changed lines executed (floor 80%); 436 of 1141 changed lines were instr
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 116 mutants tested in 10m: 89 caught, 27 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1005/review-
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 31.99s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of #1005: add the pure `wyrd-validate` capability-matrix core, version selection, observation contract and run verdict; no confirmed patch defect was found.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance rules distinguish fatal evidence from incomplete evidence and explicitly reserve required-row checks for child-2 (`brief.md:47`, `brief.md:154`). |
| C2 Reproduction (red pre-fix) | N/A | This is new functionality: independently stashing production while retaining the test produces an unresolved module import, not a behavioral red, as already accepted in the brief (`reviewer-red.log:259`, `brief.md:182`). |
| C3 Change | PASS | The four changed files stay within the agreed module/test/draft-document scope; required-row checking remains deferred without falsely passing an exercise (`crates/validate/src/lib.rs:24`, `crates/validate/src/matrix.rs:998`). |
| C4 Verification (red→green) | PASS | The restored patch independently passes all 30 tests, and all four stipulated rule-removal cases produce assertion failures; baseline compilation failure is not counted as behavioral red (`reviewer-final-green.log:39`, evidence below). |
| C5 Causal adequacy | PASS | Incomplete evidence cannot hide a fatal fact: settlement checks fatal findings across message stages before accepting an unproved result; no capability-probe workaround was introduced (`crates/validate/src/matrix.rs:863`, `crates/validate/tests/capability_matrix.rs:475`). |
| T1 Structure | PASS | The pure observation boundary admits only the existing `ErrorCode` client type and contains the fields child-2 needs, avoiding a dependency on client execution (`crates/validate/src/matrix.rs:27`, `crates/validate/src/matrix.rs:550`, `crates/validate/src/matrix.rs:663`). |
| T2 Shape | PASS | Version and entity-tag parsing reject malformed tokens, profiles reject duplicate operations, and formatting/spelling checks pass (`crates/validate/src/matrix.rs:221`, `crates/validate/tests/capability_matrix.rs:862`, `crates/validate/tests/capability_matrix.rs:1010`). |
| T3 Runtime | PASS | Classification and folding need no external runtime state; independent tests cover wrong statuses, cut bodies, absent answers and repeated exercises without claiming unsupported coverage (`crates/validate/tests/capability_matrix.rs:530`, `crates/validate/tests/capability_matrix.rs:1163`). |
| T4 Contribution | PASS | Path-based prior art is confirmed: all 19 closed-unmerged PRs and 24 merges since the brief's cutoff have no overlap; earlier cited merged work was checked by filenames, and no PR is open (`reviewer-prior-art.json:2`); contribution artifacts remain N/A until publish. |
| T5 Judgment | PASS | The agreed pre-0.1 retention trade-off is stated with its false-failure cost, and the second child's checks remain explicitly deferred; neither decision needs reopening (`docs/design/proposals/draft/0017-blackbox-validation-tool.md:284`, `crates/validate/src/matrix.rs:998`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept the observation contract as a useful foundation for child-2 and #939 — fixed-observation tests establish this slice's logic, while suitability for the eventual scenario runner remains the human sign-off decision (`brief.md:188`, `crates/validate/src/matrix.rs:660`). |

Source citations beginning `crates/` or `docs/` were checked in `$PDCA_TARGET`; other citations name evidence in this bundle. The target was readable and matched the patch. Both new files were restored byte-for-byte, and reverse-application checking succeeds (`reviewer-restoration.log:5`).

Independent falsifiability evidence covers every case required by `brief.md:185`:

- Suppressing success recognition fails the unexpected-support assertion at `crates/validate/tests/capability_matrix.rs:710` (`reviewer-mutants.log:3`).
- Accepting every error code fails the wrong-code assertion at `crates/validate/tests/capability_matrix.rs:614` (`reviewer-code-mutant.log:3`). This mutant was initially unviable under denied unused-variable warnings; rerunning with cargo-mutants' lint cap produced an actual assertion failure.
- Disabling the unknown-build branch makes `0.0.0+git.unknown` parse as a release and fails at `crates/validate/tests/capability_matrix.rs:854` (`reviewer-unknown-mutant.log:14`).
- Weakening the inconclusive condition incorrectly produces PASS with an unsettled required row and fails at `crates/validate/tests/capability_matrix.rs:1115` (`reviewer-mutants.log:4`).

The final unmutated suite passes under Rust 1.96.0. Independent rustfmt, typos, documentation lint and rendering also pass; rendering produced 101 pages with a successful link audit (`reviewer-docs-render.log:2`). Thus both named external dependencies were actually exercised, with no shim or missing-tool substitution.

The instance-scoped gate wrappers were adjudicated from their frozen captured output; full workspace CI, coverage and the complete mutation campaign are not claimed as independent reruns.

| Gate | Verdict | Basis |
|------|---------|-------|
| C4-ci | PASS | Captured CI runs the matrix's 30 tests and finishes with all checks passed (`gate-logs/C4-ci.log:3236`, `gate-logs/C4-ci.log:4526`). |
| C4-verify | N/A | Exit 77 reflects the agreed new-API limitation: green executes, while the pre-fix test cannot import the module; independently reproduced (`gate-logs/C4-verify.log:14`, `gate-logs/C4-verify.log:16`). |
| C4-diff-cov | PASS | Captured instrumentation reports 390/436 changed executable lines covered, 89.4%, above the 80% floor (`gate-logs/C4-diff-cov.log:162`). |
| C5-mutants | PASS | Captured campaign reports 89 caught and 27 unviable mutants, with no survivors; independent targeted assertion failures supplement it (`gate-logs/C5-mutants.log:13`). |
| T4-batch-review | PASS | Captured review reports zero blocking findings; this advisory review separately examined the patch against the standing rubric (`gate-logs/T4-batch-review.log:10`). |
| T4-contribution | N/A | Contribution artifacts are absent by design at Check; the substantive audit must rerun at publish (`gate-logs/T4-contribution.log:10`). |
| host-tikv | PASS | Both requested feature compilation commands finish successfully in the captured log (`gate-logs/host-tikv.log:112`, `gate-logs/host-tikv.log:211`). |

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — issue 1005 (capability matrix core, 743.1a)

**Bottom line: I tried to refute the fix and could not.** No NEEDS-HUMAN findings. Below are
the attacks I ran and the two small leftovers I found; neither is a defect in today's code.

## The evidence (red→green)

- `C4-verify` reports UNVERIFIABLE (`gate-logs/C4-verify.log`: the test does not compile
  without the new module). The brief expects this and accepts it. Since `build-notes.md` is
  withheld from me, I re-ran the reds myself. In a scratch copy of `$PDCA_TARGET` I made 14
  targeted mutations to the production `crates/validate/src/matrix.rs` and ran the real
  `cargo test -p wyrd-validate --test capability_matrix` against each one. 13 turned the test
  red, including all four the brief names:
  - removing `unexpected-support` (`matrix.rs:943`): 4 tests red;
  - letting any code match (`matrix.rs:75`): 2 red;
  - reading `+git.unknown` as a release (`matrix.rs:472`): 2 red;
  - dropping the "required-row checks not yet defined" finding (`matrix.rs:1002-1004`), so an
    unsettled `required` row would fold into PASS: 3 red.

  The other nine also went red: `unread` ignoring a code that was read (`:986`), `settle` not
  returning on the first fatal (`:875`), the 5xx fact exempting every `unsupported` row
  (`:928`), the vacuity check removed (`:1128`), undeclared operations ignored (`:1078`),
  pre-release ignored in `cmp_bound` (`:374`), a `HeadBucket` cut after 0 bytes treated as
  `protocol` (`:962`), body judged before status (`:864-871`), and a `not checked` row
  passing (`:915`). Every test calls `wyrd_validate::matrix::{classify, verdict,
  select_reported, parse_version}` directly. Nothing re-implements the code under test.
- The one mutation that survived is a test gap, not a production bug. If the code check is
  skipped whenever the body is `Body::Bytes` (`matrix.rs:986`, e.g.
  `unread = … || matches!(response.body, Body::Bytes(_))`), all 30 tests still pass. The input
  nothing pins: `CopyObject` answered `501`, `error_code: Code("InvalidRequest")`,
  `body: Body::Bytes(b"<Error>…")`. Production calls that `protocol`, but a broken version
  would pass it. The only `Bytes` leg (`capability_matrix.rs:604`) uses the right code. I
  did **not** raise this as NEEDS-HUMAN: the real client cannot produce this shape, because
  `S3Error::Service` carries the code but no raw body
  (`crates/validate/src/s3/error.rs:16-25`), so #939 will map a readable error to
  `Body::Empty` plus the code, which is the form the tests do pin. Adding one leg would be
  cheap hardening, nothing more.

## The fix — inputs I tried that did not break it

I ran a throwaway probe test against the production functions:
- A cut error body on the declared `501` whose bytes before the cut already hold a wrong
  `<Code>InvalidRequest</Code>`, with no code read by the client: **unsettled**
  (`matrix.rs:974-976`, `:986`). This matches brief (D) ("a cut-off error body on the
  declared status is unsettled"). The client buffers error bodies whole and never reads a
  code from a partial one. Not a refutation. #939 should keep mapping a cut error body to
  `Arrived::Count`, as `Arrived` (`matrix.rs:624-630`) says.
- A `204` delete whose follow-up `HEAD` got a `503`: unsettled. Follow-up checks belong to
  child-2 under the brief. Not this diff's job.
- A 5xx to an operation the profile has no row for: INCONCLUSIVE, not FAIL
  (`matrix.rs:1077-1079`). Brief (H) asks for exactly this, and `pre-0.1` cannot reach it,
  since all 22 `Operation` variants have rows (`matrix.rs:284-337`).
- Odd statuses (0, 99, 600, 999): `protocol` on an `unsupported` row and unsettled on a
  `required` row. Sound.
- Contradictory observations (`501` + `ErrorCode::NoBody` + cut; `400` `HeadBucket` +
  `Unreadable`): `protocol`, which is fail-closed.
- Version grammar: `0.0.0-rc.1+git.unknown` gives Unknown. `x+git.unknown` gives a parse
  error. `0.1.0--rc`, `0.0.0+git.0123abc` and `1.0.0-0` parse as valid SemVer, and select
  `pre-0.1` / `pre-0.1` / no match. `0.1.0-rc.1+git.dirty` selects `pre-0.1`. Leading
  zeros, `+` signs, empty identifiers and whitespace are all refused (`matrix.rs:426-493`),
  which meets the rubric's grammar-strictness rule.
- Citations: I checked every `pre-0.1` row's cited line in
  `crates/gateway-s3/src/lib.rs` on the target (`:410-422`, `:1548-1553`, `:1766-1788`,
  `:1797-1826`, `:1842-1849`, `:1865-1871`, `:2008-2028`, `:2293-2300`, `:2704-2725`,
  `:2785-2803`, `:2995`, `:3401-3404`). Each one gives the answer its row declares.
  `HeadBucket` reaches the bucket-path `400` and loses its body at `:1552-1553`, so the
  client sees `ErrorCode::NoBody`.

## The verdict — claims I checked

- `check-gates.json`: C4-ci ran the 30 new tests green, plus typos and docs-check
  (`gate-logs/C4-ci.log:11-19`, `:3202-3236`). C5-mutants reports 89 caught and 0 missed;
  that is honest, but it does not mean every mutation was tried (see the surviving one
  above). Nothing a gate claims is wrong.
- Diff coverage misses `matrix.rs:1093-1098`. That arm (`RowResult::NotChecked` on a row
  that must be proved) cannot run: only a `supported, not checked` row produces
  `NotChecked`, and the `!must_prove()` arm at `:1088` catches it first. It is harmless
  defensive code, not a gap in the tests.
- Scope: `lib.rs` changes by one line. The proposal is `status: draft`, so editing it is
  allowed. No `Cargo.lock`, `deny*.toml` or gateway change.

<!-- pdca:leaf-complete -->

### Advisory — code-review

- No introduced correctness bugs found in version selection (`crates/validate/src/matrix.rs:426`), classification and fatal-evidence precedence (`crates/validate/src/matrix.rs:897`), or run verdict folding (`crates/validate/src/matrix.rs:1067`). The scoped regression tests exercise these production paths (`crates/validate/tests/capability_matrix.rs:375`).
- No actionable reuse, simplification, or efficiency findings. The shared classification ladder (`crates/validate/src/matrix.rs:863`) and refusal checks (`crates/validate/src/matrix.rs:941`) keep the scoped rules centralized without duplicating the client's XML parser.

Validation used the frozen gate evidence: CI passed, all 30 matrix tests passed, and mutation testing reported 89 caught and 27 unviable mutants, with none surviving. Checks were not rerun; the target remained read-only.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] Validation — fitness-to-purpose — Accept the observation contract as a useful foundation for child-2 and #939 — fixed-observation tests establish this slice's logic, while suitability for the eventual scenario runner remains the human sign-off decision (`brief.md:188`, `crates/validate/src/matrix.rs:660`).
- [x] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: merged-wider
- Iteration delta (if iterating):
- By / date: Eduard Ralph / 2026-10-30

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
