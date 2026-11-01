# validate: capability matrix core — profiles, version keying, the observation type, unsupported-row classification, the run verdict (743.1a)

> Do reads ONLY this file. Plan of record: proposal 0017,
> `docs/design/proposals/draft/0017-blackbox-validation-tool.md`. Read §3 *The capability
> matrix* (`0017:188-275`) and §7 *Failure taxonomy* (`0017:482-524`) before writing code.
> First of two children of #937 (itself 743.1, first child of #743); pure, it sends nothing.
> Sibling: child-2 (the `required`-row checks) builds on this one and changes no public type,
> so every field the observation type will ever need is declared HERE, even the ones only
> child-2 reads. Other kin: 743.2 = #938 (client verbs, split into #955, #956), 743.3 = #939
> (smoke), which consumes both children of #937.
>
> **Re-planned 2026-10-25 after nine builds of #937** (iteration 9's sign-off: iterate-plan,
> "re-plan around the 0017 §7 failure classes, not individual cases"). Iterations 7–9 each fixed
> the last edge case, and review found the next one in the same grading code. Both iteration-9
> defects had one shape: a check that can only make an exercise unsettled ran before a check
> that would have found it fatal. This brief therefore states each outcome as one rule over
> every row and one order of precedence. The decisions of 2026-10-25 (the human's, that Plan
> session) are marked **(decided 2026-10-25)**.

- **Slug:** validate-capability-matrix-core
- **Track:** blackbox
- **Kind:** enhancement
- **Defect:** `wyrd-validate` has no capability matrix. Nothing declares, per S3 operation,
  whether a Wyrd release must support it or must refuse it with an exact status and code;
  nothing picks those expectations by the server's version; nothing defines what an observed
  answer looks like to a judge; and nothing folds a set of judged exercises into a run verdict.
  Smoke (743.3) and every later scenario need all of it, and none of it needs the network.
- **Success criterion:** the `matrix` module exists, is pure (no I/O, network, clock, SDK or
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
- **Falsifiability:** RED is reachable on the plain `cargo test -p wyrd-validate --test
  capability_matrix` target that `cargo xtask ci` (gate C4-ci, gating) runs. It is a plain
  integration test with no `cfg` or feature gate, and every leg calls a pure production
  function with a fixed input. A classifier that lets a 2xx pass an `unsupported` row; passes
  an `<Error>` with the wrong code; lets a body cut after a 2xx head hide the 2xx; reads
  `+git.unknown` as `0.0.0`; ignores the version; passes a `supported, not checked` row; or
  folds an unsettled `required` row into a PASS fails an assertion. No network, Docker or
  second process.
- **Invariant to restore:** the verdict never claims more than the answers prove, and never
  less. An `unsupported` operation that succeeds in any part is fatal (`0017:197-199`); an
  answer the server gave that is not the defined one is a FAIL, never budgeted (the human's
  rule, 2026-10-24; §7 amended to match); a fact that already proves the answer wrong is never
  hidden by anything that only leaves the exercise unproved (`0017:484-485`); "version
  unknown" is never mistaken for a release (`0017:257-261`); a `required` row this child
  cannot yet judge is reported unproved, never passed.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Reproduction:** n/a — new functionality. No matrix code exists on `main`.
- **Depends on:**
- **Conflicts with:**
- **Ordering note:** first of two children of #937; child-2 depends on this one. #939 (743.3)
  must depend on BOTH children, not on #937: the driver levels a dependent of a split parent
  by its own edges and does not move them onto the children (`flow.py:1616-1620` in this
  harness), so #939's `Depends on:` is changed to name them when the split is accepted.
  **Base:** `main` at or after `9b1a3e2`, which carries #744 (PR #931), #853 (PR #934) and the
  cargo-deny fixes (`6a0b369` RUSTSEC-2026-0385, `6c79af3` RUSTSEC-2026-0386). Iteration 9 of
  #937 was built on the batch's old integration line (`93a6f37`), which lacks them, so both
  all-features advisory steps of C4-ci were red there. Do must not touch `Cargo.lock` or
  `deny*.toml`, and records in `build-notes.md` which base it built on. The driver cuts Do's
  worktree from `origin/main` or a run line freshly built on it (`src/pdca_harness/
  worktree.py:93-98`, `integrate.py:298-300`, in this harness). **Do's first step, before any
  code:** in the worktree, run `git merge-base --is-ancestor 9b1a3e2 HEAD`. If it fails, stop:
  write no code, put the supplied `HEAD` and "base below `9b1a3e2`" at the top of
  `build-notes.md`, and do not rebase, merge `main`, or repair `Cargo.lock` / `deny*.toml` to
  get past it.
- **Scope:** the `matrix` module: operations, rows in three states, immutable profiles with
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
- **Difficulty:** medium
- **Do model:** opus
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/validate/tests/capability_matrix.rs` (NEW)
- **Verification posture:** net-new. With production reverted the test cannot compile, so
  C4-verify reports UNVERIFIABLE (exit 77; this instance's gate compiles each added
  `*/tests/*.rs`, `engine/scripts/run-verify.sh:403-413`), accepted as at #937 iteration 7's
  sign-off, with demonstrated reds instead: `build-notes.md` shows, for at least the
  `unexpected-support`, wrong-code, `+git.unknown` and unsettled-`required`-folds-to-
  INCONCLUSIVE legs, the assertion that fails when the rule is removed.
- **Production reach:** consumed by child-2 and then #939 in this same run; until then
  nothing in the binary calls the module. Every test leg calls the production functions, not a
  copy.
- **Citations expected:** Do must cite `path:line` on the target branch for every change.
  - **Peer to start from:** #937 iteration 9's module and test,
    `results/issue_937/iteration-v9/patch.diff` (`crates/validate/src/matrix.rs`,
    `crates/validate/tests/capability_matrix.rs`, and the 0017 hunks). Review found no defect
    in its profiles (`:44-330` of the diff's `matrix.rs` hunk), version keying (`:332-514`),
    observation type (`:516-741`), class and result types (`:743-810`), `check_unsupported`
    (`:833-876`), verdict fold (`:1271-1360`) or grammars (`is_strong_etag`, `parse_version`).
    Carry those over. Its `check_required` and helpers (`:907-1270`) are NOT carried: in this
    child a `required` row is the 5xx fact and "unsettled"; child-2 writes the checks to
    (B)–(G). `is_strong_etag` stays public and is carried here with its grammar test
    (`strong_entity_tag_grammar`), because child-2 may add no public function and its tests
    call it; `parse_content_range` is private and child-2's. Of iteration 9's tests, those on `unsupported` rows, `HeadBucket`,
    the 5xx fact, version keying, selection and the verdict fold are this child's starting
    point; `a_full_passing_run_against_pre_0_1_passes` and every `required`-row test move to
    child-2.
  - Each `pre-0.1` row cites the gateway line giving today's answer
    (`crates/gateway-s3/src/lib.rs` on `9b1a3e2`): `CopyObject` `:1865-1871`; the object-path
    subresource denylist `:1842-1849`, its list `:410-422`; bulk `POST ?delete` `:1766-1788`;
    the bucket-path denylist `:1797-1804`; a bucket path with any other method `:1809-1815`;
    the root path `/` `:1819-1826`; `DeleteObject` 204 `:2028`; `DeleteObjects` 200 with errors
    `:2293-2300`; the `Server` header `:1694-1699`.
  - The version string's shape: `normalize_describe`,
    `crates/server/src/version/derivation.rs:33-68`: `0.0.0+git.<sha>[.dirty]` untagged,
    `0.1.0` on a tag, `0.1.0+git.dirty` dirty on a tag (`:60`), `0.1.0+git.3.<sha>` past one;
    `+git.unknown[.dirty]` at `:65`.
  - The only client type the matrix uses: `ErrorCode`, `crates/validate/src/s3/error.rs:68-76`,
    re-exported at `crates/validate/src/s3.rs:60` and `crates/validate/src/lib.rs:34-36`.
  - The observation type's fields against the gateway and client, so child-2 finds them in
    place: PUT 200 declares `content-length: 0` (`put_object_response`, `:3401-3404`); a 204
    declares none (`empty_response`, `:3384`); ranged 206 declares `content-length` = the span
    and `Content-Range` (`:2704-2725`); 304 carries no body and no length
    (`not_modified_response`, `:2785-2803`); the client exposes a GET's declared length
    (`ObjectBody::content_length`, `crates/validate/src/s3/body.rs:431-433`); the PUT receipt
    rule, `crates/validate/src/s3.rs:169-178` and `BodyError::AcknowledgedEarly`,
    `crates/validate/src/s3/error.rs:101-114`.
- **Prior-art check (triage cycles):** searched by path `crates/validate/` and
  `docs/design/proposals/draft/0017-blackbox-validation-tool.md` across merged, open and
  closed-unmerged PRs on getwyrd/wyrd (re-run 2026-10-24; `main` unchanged at `9b1a3e2` on
  2026-10-25). Merged: #845, #849, #859, #860, #865, #886, #765, #931 (#744), #934 (#853).
  Open: none on these paths. Closed-unmerged: none overlapping (iteration 9's reviewer checked
  all 19). No matrix code exists on `main`; #937's nine attempts are the only prior matrix
  code (`results/issue_937/iteration-v1/` … `iteration-v9/`).
- **Disposition hint:** new-feature

## Design

- **The observation type is the matrix's own**, the contract #939 maps onto (approved
  2026-10-24), and it is complete in THIS child so child-2 changes no public type. No answer
  (and why); or a response head: status, `ETag`, `Content-Range`, `Content-Length`, the S3
  error code or its absence; and the body: object bytes; a typed listing (key and size, every
  page together) or `<DeleteResult>` (`<Deleted>` keys, `<Error>` rows); arrived whole but not
  the document its status calls for (and why); or cut off after the head, carrying the bytes
  that arrived for an object body or only their count for a buffered one. A PUT records
  whether its request body had ended when the head arrived. Each exercise records what it
  asked: the range; for both conditional forms the sent value, the object's tag and the
  object's bytes (the matching form's bytes are new against iteration 9 — child-2 needs them
  for an unfit matching ask that is answered 200); the object's size and tag for `HEAD`; the
  requested keys; the keys and sizes written under a listed prefix. A delete carries each
  follow-up `HEAD` as its own answer-or-no-answer, with the key it asked about. This child
  constructs all of it in tests (to show the type is whole) but judges only what (B)–(D) above
  read.
- **One ladder, written once.** Every row is judged through the order of (B), and the
  `required` rows' place in it holds the 5xx fact until child-2 fills it in without changing
  the order or a public type. How the checks are represented is Do's. Early returns that skip a later check — iteration 9's
  `check_delete_result` duplicate guard (`matrix.rs:1187-1193` there) and `expect_bytes`
  (`:889-934`), which never read `content_length` — are the shape (B) step 3 forbids; do not
  carry that shape into the `unsupported` path either (a 2xx head is fatal before the body is
  looked at; a cut `<Error>` body is unsettled only after the status and the no-body check ran).
- **Version selection** compares the release part, pre-release included; build metadata never
  changes the selection, except that the `git.unknown` identity means unknown.
- **0017 §3 amendment** (a draft proposal, editable: `.github/workflows/docs-immutability.yml`
  freezes proposals only once accepted). Carry iteration 9's §3 and *Dependencies* hunks: the
  third state `supported, not checked (#NNN)` in the state list at `0017:193-195`;
  `0017:259-261` corrected (since #886 every response carries `Server: wyrd/<version>`, so the
  version is read from the wire and `--server-version` is an override); the *Dependencies*
  paragraph at `0017:858-863` marked landed; and after `0017:274-275` the paragraph that
  `pre-0.1` describes `main` and is edited in place until the first `0.1.0` release, a
  feature that lands before the tool can check it moving its rows to `supported, not checked`.
  **Retention starts at `0.1.0`, not at the first tag (decided 2026-10-19,** #937 iteration 7
  sign-off, the human: "keep … the rc-tag profile selection … as built"**).** This replaces
  #937's tracker body's older "Retention starts at the first tag"; it is the human's call, not
  Do's or the reviewer's to reopen. The paragraph must also state its cost plainly: a tag
  before `0.1.0` (such as `v0.1.0-rc.1`) gets no retained profile, so a newer tool run against
  that rc after a later edit can fail on a row the rc predates (its correct `501` on a
  multipart row moved to `supported, not checked` is a 5xx, so `protocol`). Freezing at the rc
  tag would not remove that false failure, only move it: a build past a tag reports
  `0.1.0-rc.1+git.<n>.<sha>` (`crates/server/src/version/derivation.rs:55`) and selects what
  the tag selects, so `main` past the rc would hit `unexpected-support` on the frozen rows
  instead.
- **0017 §7 amendment.** The taxonomy table at `0017:487-493`: `protocol` reads "wrong status
  (a 5xx included), header, error code, or XML shape"; `availability` loses "5xx" and covers
  only failures that leave no answer to judge (transport failure, reset, a timeout before the
  head, a body cut off after it). After the `protocol` paragraph (`0017:495-500`) add: a 5xx
  is an answer, and an answer other than the defined one is an implementation error, fatal like
  any wrong status; and a fatal fact outranks an unproved exercise — a cut, a lost follow-up
  or an ask that cannot test its row never hides a wrong status, header or byte that already
  arrived. The opening line (`0017:484-485`) stays.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR MAY
happen during the cycle. The PR MUST NOT be marked ready before sign-off accepts.
