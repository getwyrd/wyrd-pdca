<!-- pdca:split-proposal v1 -->
# Split proposal — issue 937

> The seam the Plan session of 2026-10-25 already drew into the parent brief (*Split*), written
> out as two complete child briefs. Every point marked **(decided 2026-10-25)** is the human's
> decision from that session, carried over unchanged. Citations are the parent's, verified
> against `getwyrd/wyrd` `main` @ `9b1a3e2` on 2026-10-25.
>
> Intake cap (wyrd-pdca-P1): blackbox **4/6** when written — PLANNED #937, #939, #955, #956;
> every other blackbox bundle is COMPLETE — room 2, need 2: open, with nothing to spare.
> Counted by hand from `results/` by `scripts/plan-cap`'s own rule (states from the files,
> tracks from each brief's `Track` field); the script itself could not be run in this headless
> session. Re-run `scripts/plan-cap --track blackbox --need 2` before `--accept`. On accept the
> parent leaves the count the way #743 and #938 did (close-disposition, COMPLETE).

## Why this slice is oversized

The parent is one module but two shippable outcomes, and the second cannot fit in the same
patch as the first: iteration 9 was 99.6 KB against the 100 KB backstop (`iteration-v9/
size-signal.json`), and the re-plan adds code — the one-ladder precedence of (B), the per-class
rules of (C), the closed evidence table (F) and the unfit-ask rules (G) replace early returns
with checks that all run. The brief's own new scope, not the inherited sibling fields, is what
tips it: the parent's `Depends on:` and `Conflicts with:` are empty, so nothing in the 22.7 KB is
ordering metadata.

- **The seam is where the code already divides.** Iteration 9's `classify` (`matrix.rs:812-827`
  in `iteration-v9/patch.diff`) dispatches on the row's state: `SupportedNotChecked` and
  `Unsupported` are judged there and in `check_unsupported` (`:833-876`); every `required` row
  goes to `check_required` (`:1001-1135`) and its helpers (`expect_bytes`, `expect_no_body`,
  `whole_document`, `probe_deleted`, `check_delete_objects`, `check_delete_result`,
  `parse_content_range`, `:907-1270`). The first half plus the profiles, version keying,
  observation type and verdict fold is child-1; the second half is child-2. Child-1 ships with
  `check_required` reduced to the 5xx fact and "unsettled"; child-2 fills it in.
- **Each half has its own defect, test and pass condition.** Child-1: nothing declares what a
  release must refuse, picks it by version, or folds answers into a run verdict — its test shows
  (A), (D), (E), (H) and the ladder on `unsupported` rows. Child-2: after child-1 a `required`
  operation can never pass and a wrong byte, header or document from one is never fatal — its
  test shows (B), (C), (F), (G) on every `required` row and the full passing `pre-0.1` run.
  Each is a PR on its own; child-1 is useful to #939 even before child-2 lands (it already
  turns an accepted multipart upload into `unexpected-support`).
- **The cut leaves no public type for child-2 to change** (decided 2026-10-25). The whole
  observation type of the parent's *Design* — including the fields only child-2 reads, such as
  the matching conditional form's object bytes — is child-1's. So child-2's test compiles
  against child-1's module, and child-1's test file is never edited: child-1's tests assert
  nothing about a `required` row's answer beyond the 5xx fact and "otherwise unsettled".
- **Not three.** The two 0017 amendments (≈4 KB of hunks) and the version keying could each be
  their own child; neither has a separate test or pass condition worth a full cycle, and both
  ride in child-1 under the 100 KB backstop (≈60 KB, mostly iteration 9's code carried over).

## Wave sketch

- **Wave 1: child-1 alone.** It creates `crates/validate/src/matrix.rs`, adds one line to
  `crates/validate/src/lib.rs`, creates `crates/validate/tests/capability_matrix.rs`, and
  amends 0017 §3 and §7. Nothing else in the track touches those files: #955 and #956 edit
  `crates/validate/src/s3.rs` and `s3/*`, so child-1 can share a wave with them and needs no
  `Conflicts with`.
- **Wave 2: child-2 stacks on child-1** (`Depends on: child-1`). It edits the `matrix.rs`
  child-1 created and adds `crates/validate/tests/capability_matrix_required.rs`. It needs
  child-1's accepted result as its base — the observation type, the rows, the verdict fold —
  not a parallel copy, so the dependency edge, not a conflict edge, is the right one;
  `compute_waves` folds child-1 onto the integration branch before child-2 builds.
- **Outside the split: #939 (743.3) must depend on both children.** Its brief says
  `Depends on: 937, 955, 956, 744, 853` (`results/issue_939/brief.md:154`). The driver levels a
  dependent of a split parent by its own edges and does not move them onto the children
  (`flow.py:1616-1620` in this harness), so when this split is accepted, `937` in that line is
  replaced by the two new child ids. #939 then lands in wave 3 (after child-2) or later.
- **Base for both:** `main` at or after `9b1a3e2`, which carries #744 (PR #931), #853 (PR #934)
  and the cargo-deny fixes (`6a0b369`, `6c79af3`). Neither child names #744 or #853 in
  `Depends on:` — both are COMPLETE and merged — but neither may be built on the batch's old
  integration line `93a6f37`, where both all-features advisory steps of C4-ci were red. Each
  child's Do starts with an ancestry check against `9b1a3e2` and stops if it fails (#937's
  plan review was pinned to `93a6f37` by iteration 9's stale `stack-base` file, which the
  driver deletes before Do, `flow.py:2204-2209`).

<!-- pdca:child child-1 -->
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
- **Track:** <leave as is — `pdca split --accept` writes the parent's track in>
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
<!-- pdca:end child-1 -->

<!-- pdca:child child-2 -->
# validate: capability matrix — the required-row checks, the 404 fact, the full `pre-0.1` run (743.1b)

> Do reads ONLY this file. Plan of record: proposal 0017,
> `docs/design/proposals/draft/0017-blackbox-validation-tool.md`. Read §3 *The capability
> matrix* (`0017:188-275`) and §7 *Failure taxonomy* (`0017:482-524`, as amended by child-1)
> before writing code. Second of two children of #937 (itself 743.1); builds on child-1's
> accepted module and changes **no public type**. Pure; it sends nothing. Kin: 743.2 = #938
> (client verbs, split into #955, #956), 743.3 = #939 (smoke), which consumes both children.
>
> **Why this shape:** #937's iterations 7–9 each fixed the last edge case in the `required`-row
> grading and review found the next one in the same code. Both iteration-9 defects had one
> shape: a check that can only make an exercise unsettled ran before a check that would have
> found it fatal (`matrix.rs:889` there ignored a declared `Content-Length` once a cut arrived;
> `:1186` returned "unsettled" on a duplicate key before the `<Error>` and document-shape
> checks). This brief states each outcome as one rule over every row, under the one ladder
> child-1 built, with a closed table of what each row reads. Fixtures are chosen per class and
> per precedence pair, not per case. The decisions of 2026-10-25 (the human's, that Plan
> session) are marked **(decided 2026-10-25)**.

- **Slug:** validate-capability-matrix-required-rows
- **Track:** <leave as is — `pdca split --accept` writes the parent's track in>
- **Kind:** enhancement
- **Defect:** after child-1, the matrix judges a `required` row by the 5xx fact alone and
  otherwise reports it unsettled. So no run against `pre-0.1` can PASS, and a `required`
  operation that returns the wrong bytes, a wrong header, a wrong document, a 404 for a key the
  run wrote, or a bulk delete that did not delete is never fatal. Smoke (#939) needs the
  `required` half of the verdict before its PASS means anything.
- **Success criterion:** with child-1's module as the base and no public type changed, the
  NEW file `crates/validate/tests/capability_matrix_required.rs` shows (B), (C), (F), (G) for
  every `required` row of `pre-0.1`, the 404 fact, and (H)'s full passing run; and child-1's
  `crates/validate/tests/capability_matrix.rs` still passes unedited.
  - **(B) The ladder, now with the `required` rows' checks as its data.** An exercise carries
    its evidence in message order: what it asked, the response head (status, then headers),
    the body (whole, or cut with what arrived before the cut), and any follow-up request, each
    its own answer. Judged in child-1's order: (1) another row's exercise is unsettled; (2) no
    response head is no verdict, follow-ups not judged; (3) **fatal** — every check (F) names
    for the row runs on everything that arrived, and the result is the class of the **first
    failing check in message order**: request-side facts, status, headers in (F)'s order,
    body, follow-ups in their order **(decided 2026-10-25)**; nothing that only leaves the
    exercise unproved — a cut, an ask that cannot test the row, a follow-up with no answer, an
    unprobed key, an empty listing — stops a later check from running; (4) **unsettled** only
    when step 3 found nothing and something left the row unproved; (5) **pass** when every
    check passed and nothing was left unproved. The tests show at least one precedence pair
    per class: a fixture where the unproving fact arrives before the fatal one, asserting the
    fatal class.
  - **(C) The classes, each one rule over every `required` row** (0017 §7 as amended).
    - **`oracle`** — the server's account of what the store holds disagrees with the model: a
      404 to any `GetObject` form or to `HeadObject` of a key the run wrote; a `HeadObject` 200
      whose `Content-Length` is not the object's size or whose `ETag` is not the one its PUT
      answered (absent counts as different); a listing 200 whose keys and sizes are not exactly
      those written under the prefix (one missing, extra, listed twice, another size); a
      follow-up `HEAD` **of a key the delete requested**, after an acknowledged delete (one
      that answered its defined status), answering 2xx (`0017:490`). A follow-up of any other
      key proves nothing about this delete: the gateway deletes only the named key
      (`crates/gateway-s3/src/lib.rs:2026-2028`) and rightly answers 200 for a key that still
      exists (`:2583-2584`, `:2612-2617`).
    - **`integrity`** — the payload a read delivered is not the object's: on an answer that
      carries object bytes (whole `GetObject` 200, ranged 206, a conditional `GetObject` 200),
      a declared `Content-Length` other than the expected count (the object's size, or the
      range's length); bytes that arrived whole and differ from the expected ones; bytes before
      a cut that are not a prefix of them; or a cut reporting more bytes than expected.
    - **`protocol`** — any other way the answer is not the defined one: a status other than
      the row's defined one (a 5xx included); a header the table requires, absent or malformed;
      a byte (whole, or before a cut) on an answer that carries no body; a non-zero declared
      `Content-Length` on a `PutObject` 200 or a `DeleteObject` 204, which declares a body the
      answer must not have, whatever arrived after it **(decided 2026-10-25: the matrix owns
      the length-framed case)**; a body that arrived whole and is not the document its status
      calls for (0017 §7, "XML shape"); a `PutObject` success whose head arrived before the
      request body had ended; a `<DeleteResult>` with any `<Error>` row, or whose `<Deleted>`
      does not name each requested key exactly once and nothing else.
    - **Unsettled** (step 4): a body cut off where the check needs the rest (after a correct
      prefix, after no byte, or a document cut before it could be read); an ask that cannot
      test the row ((G)); a follow-up with no head or a cut-off body; a requested key no
      follow-up proved gone; a follow-up of a key the delete did not request, whatever it
      answered — only the checks that do not depend on the key run on it (a 5xx is `protocol`;
      a byte on its no-body answer is `protocol`), never the follow-up's 404 status check or the
      deleted-key `oracle`; a listing of a prefix with nothing written; a `required` row whose
      exercise carries no check of its own.
    - **No verdict** (step 2): the exercise's own request got no response head.
    - **The two facts for every exercise that got a head:** a 5xx is `protocol` unless the row
      declares that status (child-1 shows it; it must still hold here); **a 404 to a typed read
      of a written key is `oracle`**, whatever was asked — shown here on every `GetObject` form
      and `HeadObject`, including an unfit ask. A typed read is a `GetObject` form's or
      `HeadObject`'s own request, never a delete's follow-up `HEAD`.
  - **(F) The evidence table — closed: a row reads these facts and no others.**

    | Row | Defined status | Headers read (in this order) | Body |
    |---|---|---|---|
    | `PutObject` | 200 | `ETag` strong (RFC 9110 §8.8.3), else `protocol`; `Content-Length` absent or 0, else `protocol` | none; request body had ended before the head, else `protocol` |
    | `GetObject` whole | 200 | `Content-Length` absent or the object's size, else `integrity` | the object's bytes |
    | ranged | 206 | `Content-Range` exactly `bytes a-b/size` (unit `bytes`, digits only, no signs), else `protocol`; `Content-Length` absent or `b-a+1`, else `integrity` | bytes `a..=b` |
    | conditional, matching | 304 | none | none |
    | conditional, non-matching | 200 | as whole `GetObject` | the object's bytes |
    | `HeadObject` | 200 | `Content-Length` = size, else `oracle`; `ETag` = the PUT's, else `oracle` | none |
    | `DeleteObject` | 204 | `Content-Length` absent or 0, else `protocol` | none; then the follow-up `HEAD` |
    | `DeleteObjects` | 200 | none | a `<DeleteResult>` ((C)); then a follow-up `HEAD` per requested key |
    | follow-up `HEAD` of a requested key | 404 | none | none |
    | `ListObjectsV2`, v1 | 200 | none | the listing, every page together ((C)) |

    A follow-up `HEAD` of a key the delete did not request has no defined status: only the
    key-independent checks run on it, then it is unsettled ((C)).

    Not read by this slice, and stated so a reviewer reads a request for one as scope, not a
    defect: a GET's `ETag` or `Last-Modified`; a 304's `Content-Length` (RFC 9110 §8.6 lets it
    describe the 200's length; the gateway sends none, `crates/gateway-s3/src/lib.rs:2785-2803`);
    `Content-Length` on a `HEAD` 404, a listing or a `<DeleteResult>`; a 204's
    `Content-Length: 0`. The `unsupported` rows' line of the table is child-1's and is not
    changed here.
  - **(G) Asks that cannot test their row.** The exercise records what it asked. An unfit ask
    never passes; it is judged by every check that does not depend on the ask, then is
    unsettled:
    - conditional GET whose sent value is not a strong tag equal to the object's (matching
      form) or not a strong tag different from it (non-matching form): 200 and 304 are both
      acceptable statuses, any other is judged by (C); a 200 is held to the object-bytes checks
      and a 304 to the no-body check. Each conditional exercise already records the sent value,
      the object's tag and the object's bytes (child-1's type).
    - ranged GET with `a > b` or `b ≥ size`, and `DeleteObjects` of no keys: only the two
      facts that hold for every exercise ((C), last bullet).
    - `DeleteObjects` naming a key twice: every check runs except the `<Deleted>` exact match.
    - a listing of a prefix with nothing written: every check runs (a listed key is `oracle`).
  - **(H) The run verdict, now reachable.** A full run against `pre-0.1` — every `required`
    row exercised with the defined answer, every `unsupported` row with its declared refusal —
    is PASS. Replacing any one exercise with a fatal one is FAIL naming that row and its class;
    replacing it with an unsettled or unanswered one is INCONCLUSIVE naming that row; a row with
    one passing and one unsettled exercise is not proved. The fold itself is child-1's and is
    not changed here.
- **Falsifiability:** RED is reachable on the plain `cargo test -p wyrd-validate --test
  capability_matrix_required` target that `cargo xtask ci` (gate C4-ci, gating) runs. It is a
  plain integration test with no `cfg` or feature gate, and every leg calls a pure production
  function with a fixed input. With production reverted to child-1's module the file compiles
  (no public type changes) and fails: every `required`-row leg expects a class or a pass where
  child-1 answers "unsettled", so the file is red on the base and green after. A classifier
  that lets a cut, an unfit ask or a lost follow-up hide a fatal fact; ignores a declared
  `Content-Length`; passes a weak or missing `ETag` on a PUT; passes a `<DeleteResult>` that
  omits, repeats or errors a key; passes a listing with an extra key; reads a 200 to a
  follow-up of a key the delete did not request as fatal, or a 200 to one of a requested key as
  anything but `oracle`; or reads a 404 to a written key as anything but `oracle` fails an
  assertion. No network, Docker or second process.
- **Invariant to restore:** the verdict never claims more than the answers prove, and never
  less. A `required` operation passes only on an answer showing it was done (`0017:190-195`);
  an answer the server gave that is not the defined one is a FAIL, never budgeted (the human's
  rule, 2026-10-24; §7 amended by child-1 to match); a fact that already proves the answer
  wrong is never hidden by anything that only leaves the exercise unproved (a cut never undoes
  a wrong byte, `0017:484-485`).
- **Repo + branch target:** getwyrd/wyrd @ main
- **Reproduction:** on child-1's module, `classify` of any `required` row with the defined
  answer returns unsettled, and `verdict(pre_0_1, <full defined run>)` is INCONCLUSIVE naming
  every `required` row. The new test file's first leg asserts PASS for both and is red.
- **Depends on:** child-1
- **Conflicts with:**
- **Ordering note:** second of two children of #937; builds on child-1's accepted result (the
  module, the observation type, the ladder, the verdict fold) and edits `matrix.rs` in place.
  #939 (743.3) must depend on BOTH children, not on #937 (`flow.py:1616-1620` in this harness
  levels a dependent by its own edges); its `Depends on:` is changed when the split is accepted.
  **Base:** child-1 folded onto `main` at or after `9b1a3e2` (#744, #853 and the cargo-deny
  fixes `6a0b369`, `6c79af3` present). Do must not touch `Cargo.lock` or `deny*.toml`, and
  records in `build-notes.md` which base it built on. **Do's first step, before any code:** in
  the worktree, run `git merge-base --is-ancestor 9b1a3e2 HEAD` and check that child-1's
  `crates/validate/src/matrix.rs` exists. If either fails, stop: write no code, put the
  supplied `HEAD` and what is missing at the top of `build-notes.md`, and do not rebase, merge
  `main`, or repair `Cargo.lock` / `deny*.toml` to get past it.
- **Scope:** in `crates/validate/src/matrix.rs`: the `required` rows' entries of the check
  table — (F)'s header, body, document, follow-up and listing checks, (G)'s unfit-ask rules,
  the 404 fact — and the private grammar helpers they need (`is_strong_etag` if child-1 did not
  keep it, `parse_content_range`); the new test file with its own fixtures. No new public type,
  variant, field or function; a private helper is fine. / out of scope:
  - child-1's test file (`crates/validate/tests/capability_matrix.rs`) — not edited; the
    profiles, version keying, observation type, class and result types, the `unsupported` and
    `supported, not checked` checks, the ladder's order, and the verdict fold — not changed
    (a defect found in them is filed as a finding against child-1, not fixed here);
  - `crates/validate/src/lib.rs`; 0017 and every other doc; `Cargo.toml`;
  - sending anything: the client verbs are #955/#956; the runner, `--server-version`, reading
    the `Server` header, occupied-prefix and cleanup rules are #939;
  - mapping the client's results onto the observation type (#939), including telling the base
    client's one `S3Error::Unreadable` apart for a chunked PUT/DELETE 2xx;
  - a digest-based body for large objects (#761 adds one when it needs it); smoke sends small
    sizes only;
  - any fact (F) does not list; the cross-sweep oracle, event stream and verdict artifact
    (#746, #747, #764); MinIO's profile (#751); authentication vectors (#745); the availability
    budget itself;
  - any gateway change; any `wyrd-*` crate in `[dependencies]`; `Cargo.lock`; `deny*.toml`.
- **Difficulty:** medium
- **Do model:** opus
- **External dependencies:** `typos`
- **Test file:** `crates/validate/tests/capability_matrix_required.rs` (NEW)
- **Verification posture:** default: a flippable red→green on C4-verify. The new test
  compiles against child-1's public surface, which this child does not change, so with
  production reverted it compiles and fails (see Falsifiability). If Do finds it must add a
  public item after all, that is a scope question for the human at sign-off, not a reason to
  switch to UNVERIFIABLE silently — say so in `build-notes.md`.
- **Production reach:** consumed by #939 in this same run; until then nothing in the binary
  calls the module. Every test leg calls the production functions, not a copy.
- **Citations expected:** Do must cite `path:line` on the target branch for every change.
  - **Peer to start from:** #937 iteration 9's `check_required` and helpers,
    `results/issue_937/iteration-v9/patch.diff` (`matrix.rs` hunk lines `:907-1270`:
    `expect_bytes`, `expect_no_body`, `whole_document`, `worst`, `check_required`,
    `probe_deleted`, `check_delete_objects`, `check_delete_result`, `is_strong_etag`,
    `parse_content_range`), and its `required`-row tests (`capability_matrix.rs` hunk,
    `put_passes_only_on_200_with_a_strong_etag_after_the_body_ended` through
    `a_lost_or_unacknowledged_delete_result_never_hides_the_probes`, and
    `a_full_passing_run_against_pre_0_1_passes`). Review found no defect in the two grammars.
    The classification is restated by (B)–(G), not patched: the early returns
    (`check_delete_result`'s duplicate guard, `matrix.rs:1187-1193` there; `expect_bytes`,
    `:889-934`, which never reads `content_length`) are what (B) step 3 removes. Every check
    runs; the first failure in message order names the class.
  - The (F) facts against the gateway (`crates/gateway-s3/src/lib.rs` on `9b1a3e2`): PUT 200
    declares `content-length: 0` (`put_object_response`, `:3401-3404`); a 204 declares none
    (`empty_response`, `:3384`); ranged 206 declares `content-length` = the span and
    `Content-Range` (`:2704-2725`); 304 carries no body and no length
    (`not_modified_response`, `:2785-2803`); `DeleteObject` 204 `:2028`; `DeleteObjects` 200
    with errors `:2293-2300`. The client exposes a GET's declared length
    (`ObjectBody::content_length`, `crates/validate/src/s3/body.rs:431-433`).
  - The PUT receipt rule: `PutObject`'s doc, `crates/validate/src/s3.rs:169-178`;
    `BodyError::AcknowledgedEarly`, `crates/validate/src/s3/error.rs:101-114`.
  - Grammars: RFC 9110 §8.8.3 (entity tags) and §14.4 (`Content-Range`), under the
    grammar-strictness rule `AGENTS.md:165-169`.
- **Prior-art check (triage cycles):** searched by path `crates/validate/` and
  `docs/design/proposals/draft/0017-blackbox-validation-tool.md` across merged, open and
  closed-unmerged PRs on getwyrd/wyrd (re-run 2026-10-24; `main` unchanged at `9b1a3e2` on
  2026-10-25). Merged: #845, #849, #859, #860, #865, #886, #765, #931 (#744), #934 (#853).
  Open: none on these paths besides child-1's own draft PR. Closed-unmerged: none overlapping
  (iteration 9's reviewer checked all 19). The only prior `required`-row grading is #937's
  nine attempts (`results/issue_937/iteration-v1/` … `iteration-v9/`).
- **Disposition hint:** new-feature

## Design

- **Fixtures per class and per precedence pair, not per case.** For each class in (C), one
  fixture per rule that produces it; for each precedence pair the re-plan was written for, one
  fixture where the unproving fact comes first in message order and the fatal fact later, e.g.
  a whole GET whose `Content-Length` declares the wrong count and whose body is then cut after
  a correct prefix (`integrity`, not unsettled); a `DeleteObjects` naming a key twice whose
  `<DeleteResult>` carries an `<Error>` row (`protocol`, not unsettled); a bulk delete whose
  result document was cut off but whose follow-up `HEAD` answered 200 (`oracle`, not
  unsettled); a ranged GET with `b ≥ size` answered 404 (`oracle`, not unsettled); a matching
  conditional GET whose sent tag is not the object's (an unfit ask) answered 304 with one body
  byte (`protocol`, not unsettled). Where two fatal facts of different classes arrive, one
  fixture shows the first in message order winning: a ranged 206 with a wrong `Content-Range`
  and wrong bytes is `protocol` (the header comes before the body).
- **The follow-up key pair.** A follow-up's key decides which checks run, so these fixtures
  come in sets: `DeleteObject` of `a` → 204, then a follow-up `HEAD` of `b` (not requested)
  → 200 is unsettled, not fatal; of `a` → 200 is `oracle`; of `b` → 500 is `protocol` (a check
  that does not depend on the key still runs). The same three on `DeleteObjects` requesting
  `a` only.
- **The observation type is used, not changed.** Every fact (F) reads is already a field of
  child-1's type (status, `ETag`, `Content-Range`, `Content-Length`, error code; object bytes,
  listing, `<DeleteResult>`, unreadable-document, cut-with-arrived; the PUT's body-ended flag;
  the ask: range, sent conditional value, the object's tag and bytes for both conditional
  forms, size and tag for `HEAD`, requested keys, keys and sizes under the prefix; follow-ups
  keyed by the key asked about). If a fact is missing from the type, stop and report it in
  `build-notes.md` — it is a child-1 defect — rather than add a field here.
- **Version selection, the 0017 amendments and the verdict fold are child-1's** and are not
  touched. A run verdict's wording for a fatal `required` row (row and class) comes from the
  fold as child-1 wrote it; this child only makes the fatal results reachable.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR MAY
happen during the cycle. The PR MUST NOT be marked ready before sign-off accepts.
<!-- pdca:end child-2 -->
