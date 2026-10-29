# validate: the capability matrix — profiles, version selection, answer classification (743.1)

> Do reads ONLY this file. Plan of record: proposal 0017,
> `docs/design/proposals/draft/0017-blackbox-validation-tool.md`. Read §3 *The capability
> matrix* (`0017:188-275`) and §7 *Failure taxonomy* (`0017:482-524`) before writing code.
> First of three children of #743; pure, it sends nothing. Siblings: 743.2 = #938 (client
> verbs, itself split into #955, #956), 743.3 = #939 (smoke).
>
> **Re-planned 2026-10-25 after nine builds, and split in this Plan session** (iteration 9's
> sign-off: iterate-plan, "re-plan around the 0017 §7 failure classes, not individual cases").
> Iterations 7–9 each fixed the last edge case, and review found the next one in the same
> grading code. Both iteration-9 defects had one shape: a check that can only make an exercise
> unsettled ran before a check that would have found it fatal (`matrix.rs:889` there ignored a
> declared `Content-Length` once a cut arrived; `:1186` returned "unsettled" on a duplicate key
> before the `<Error>` and document-shape checks). This brief therefore states each outcome as
> one rule over every row, one order of precedence, and a closed table of what each row reads.
> It is split in two children (*Split* below): the core, and the required-row checks. The
> decisions of 2026-10-25 (the human's, this session) are marked **(decided 2026-10-25)**.

- **Slug:** validate-capability-matrix
- **Track:** blackbox
- **Kind:** enhancement
- **Defect:** `wyrd-validate` has no capability matrix. Nothing declares, per S3 operation,
  whether a Wyrd release must support it or must refuse it with an exact status and code;
  nothing picks those expectations by the server's version; and nothing turns an observed
  answer into a pass or one of 0017 §7's failure classes. Smoke (743.3) and every later
  scenario need all three, and none of it needs the network.
- **Success criterion:** the matrix module exists, is pure (no I/O, network, clock, SDK or
  client error types), and its tests show (A)–(H) below. The split assigns each part to a child.
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
    3. **Fatal.** Every check the evidence table (F) names for the row runs on everything that
       arrived. If any fails, the result is fatal, with the class of the **first failing check
       in message order**: request-side facts, status, headers in (F)'s order, body,
       follow-ups in their order **(decided 2026-10-25)**. Nothing that only leaves the
       exercise unproved — a cut, an ask that cannot test the row, a follow-up with no answer,
       an unprobed key, an empty listing — stops a later check from running.
    4. **Unsettled**, only when step 3 found nothing, and something left the row unproved.
    5. **Pass**, when every check passed and nothing was left unproved.
  - **(C) The classes, each one rule over every row** (0017 §7, as amended under *Design*).
    - **`unexpected-support`:** an `unsupported` row answered any 2xx, whatever its body did after.
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
      the row's defined one (a 5xx included, unless the row declares that status); a header the
      table requires, absent or malformed; a declared error code other than the row's; a byte
      (whole, or before a cut) on an answer that carries no body; a non-zero declared
      `Content-Length` on a `PutObject` 200 or a `DeleteObject` 204, which declares a body the
      answer must not have, whatever arrived after it **(decided 2026-10-25: the matrix owns
      the length-framed case)**; a body that arrived whole and is not the document its status
      calls for (0017 §7, "XML shape"); a `PutObject` success whose head arrived before the
      request body had ended; a `<DeleteResult>` with any `<Error>` row, or whose `<Deleted>`
      does not name each requested key exactly once and nothing else.
    - **Unsettled** (step 4): a body cut off where the check needs the rest (after a correct
      prefix, after no byte, or a document or error body cut before it could be read); an ask
      that cannot test the row ((G)); a follow-up with no head or a cut-off body; a requested
      key no follow-up proved gone; a follow-up of a key the delete did not request, whatever it
      answered — only the checks that do not depend on the key run on it (a 5xx is `protocol`;
      a byte on its no-body answer is `protocol`), never the follow-up's 404 status check or
      the deleted-key `oracle`; a listing of a prefix with nothing written; a `required` row
      whose exercise carries no check of its own.
    - **No verdict** (step 2): the exercise's own request got no response head.
    - **Two facts hold for every exercise that got a head, whatever was asked:** a 5xx is
      `protocol` unless the row declares that status; a 404 to a typed read of a written key
      is `oracle` (a typed read is a `GetObject` form's or `HeadObject`'s own request, never a
      delete's follow-up `HEAD`). **Also on a `supported, not checked` row: a 5xx is `protocol`; anything
      else there is "not checked", never a pass (decided 2026-10-25).**
  - **(D) Unsupported rows.** Pass only on the declared status and the declared code exactly,
    or, for `HeadBucket`, the declared status with no body. A cut-off error body on the declared
    status is unsettled. Everything else follows (B) and (C).
  - **(E) Version keying.** Parsing a reported version gives a release (`MAJOR.MINOR.PATCH`
    with any pre-release; `+` build metadata ignored for selection), **unknown**, or a parse
    error. **Unknown** covers `unknown` (`crates/gateway-s3/src/lib.rs:121`) and build metadata
    `git.unknown` or `git.unknown.dirty` (`crates/server/src/version/derivation.rs:65`,
    `:121`); it is never read as release `0.0.0`. With a test-local table of two profiles over
    different ranges, a version in each range selects its own profile, and a row `unsupported`
    in one and `required` in the other flips the verdict for the same answer.
    `0.0.0+git.abc1234`, `0.1.0-rc.1` and `0.1.0-rc.1+git.3.abc1234` (a build past an rc tag)
    select `pre-0.1`. A version matching no profile, unknown, and unparseable each select
    none, and say which.
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
    | `unsupported` | declared | none | one S3 `<Error>` with the declared code; `HeadBucket`: none |

    A follow-up `HEAD` of a key the delete did not request has no defined status: only the
    key-independent checks run on it, then it is unsettled ((C)).

    Not read by this slice, and stated so a reviewer reads a request for one as scope, not a
    defect: a GET's `ETag` or `Last-Modified`; a 304's `Content-Length` (RFC 9110 §8.6 lets it
    describe the 200's length; the gateway sends none, `crates/gateway-s3/src/lib.rs:2785-2803`);
    `Content-Length` on a `HEAD` 404, an error response, a listing or a `<DeleteResult>`;
    a 204's `Content-Length: 0`.
  - **(G) Asks that cannot test their row.** The exercise records what it asked. An unfit ask
    never passes; it is judged by every check that does not depend on the ask, then is
    unsettled:
    - conditional GET whose sent value is not a strong tag equal to the object's (matching
      form) or not a strong tag different from it (non-matching form): 200 and 304 are both
      acceptable statuses, any other is judged by (C); a 200 is held to the object-bytes checks
      and a 304 to the no-body check. So each conditional exercise records the sent value, the
      object's tag and the object's bytes.
    - ranged GET with `a > b` or `b ≥ size`, and `DeleteObjects` of no keys: only the two
      facts that hold for every exercise ((C), last bullet).
    - `DeleteObjects` naming a key twice: every check runs except the `<Deleted>` exact match.
    - a listing of a prefix with nothing written: every check runs (a listed key is `oracle`).
  - **(H) The run verdict.** Any fatal result ⇒ FAIL, whatever else passed, naming every
    fatal row and its class. Else INCONCLUSIVE, naming the rows, when any `required` or
    `unsupported` row was not exercised, or **any** exercise of it had no verdict or was
    unsettled (a row is proved only when every exercise of it passed), or an exercise names an
    operation the profile has no row for, or no row was proved. Else PASS. A `supported, not
    checked (#NNN)` row is listed as not covered with its issue, and a run can PASS without it.
    The four fatal classes print exactly as §7 names them (`integrity`, `oracle`,
    `unexpected-support`, `protocol`): 743.3's report prints them (iteration 9's one surviving
    mutant was that text).
- **Falsifiability:** RED is reachable on the plain `cargo test -p wyrd-validate --test <file>`
  targets that `cargo xtask ci` (gate C4-ci, gating) runs. They are plain integration tests
  with no `cfg` or feature gate, and every leg calls a pure production function with a fixed
  input. A classifier that lets a cut, an unfit ask or a lost follow-up hide a fatal fact;
  ignores a declared `Content-Length`; lets a 2xx pass an `unsupported` row; reads a 200 to a
  follow-up of a key the delete did not request as fatal, or a 200 to one of a requested key as
  anything but `oracle`; reads `+git.unknown` as `0.0.0`; or ignores the version fails an
  assertion. No network, Docker or
  second process.
- **Invariant to restore:** the verdict never claims more than the answers prove, and never
  less. An `unsupported` operation that succeeds in any part is fatal (`0017:197-199`); a
  `required` one passes only on an answer showing it was done (`0017:190-195`); an answer the
  server gave that is not the defined one is a FAIL, never budgeted (the human's rule,
  2026-10-24; §7 amended to match); a fact that already proves the answer wrong is never hidden
  by anything that only leaves the exercise unproved (a cut never undoes a wrong byte,
  `0017:484-485`); "version unknown" is never mistaken for a release (`0017:257-261`).
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:**
- **Conflicts with:**
- **Ordering note:** split into two children (*Split*); child-2 depends on child-1. #939
  (743.3) must depend on BOTH children, not on this parent: the driver levels a dependent of a
  split parent by its own edges and does not move them onto the children (`flow.py:1616-1620`
  in this harness), so #939's `Depends on:` is changed to name them when the split is accepted.
  **Base:** `main` at or after `9b1a3e2`, which carries #744 (PR #931), #853 (PR #934) and the
  cargo-deny fixes (`6a0b369` RUSTSEC-2026-0385, `6c79af3` RUSTSEC-2026-0386). Iteration 9 was
  built on the batch's old integration line (`93a6f37`), which lacks them, so both
  all-features advisory steps of C4-ci were red there. Do must not touch `Cargo.lock` or
  `deny*.toml`, and records in `build-notes.md` which base it built on.
  **How the base resolves (harness, this repo):** this bundle still holds iteration 9's
  `stack-base` file (`pdca-integration/main-r42c5348f203d`, tip `93a6f37`); that stale file is
  why the plan review's pinned target was `93a6f37`. The driver deletes a stale stack base at
  the top of the bundle's wave unless this run has already folded onto `getwyrd/wyrd main`
  (`src/pdca_harness/flow.py:2204-2209`, `:784-790`; `publish.py:663-670`), and a run's first
  fold of a target starts fresh from `origin/main` (`integrate.py:298-300`). Either way Do's
  worktree is cut from `origin/main` or a line built on it (`worktree.py:93-98`), today
  `9b1a3e2`. **Do's first step, before any code:** in the worktree, run
  `git merge-base --is-ancestor 9b1a3e2 HEAD`. If it fails, stop: write no code, put the
  supplied `HEAD` and "base below `9b1a3e2`" at the top of `build-notes.md`, and do not
  rebase, merge `main`, or repair `Cargo.lock` / `deny*.toml` to get past it.
- **Scope:** the `matrix` module: operations, rows in three states, immutable profiles with
  `pre-0.1`, version parsing and selection, its own observation type, the classification
  (B)–(G), the run verdict (H); `pub mod matrix;` in `crates/validate/src/lib.rs` (that one
  line only); the tests; the 0017 §3 and §7 amendments under *Design*. / out of scope:
  - sending anything: the client verbs are 743.2; the runner, `--server-version`, reading the
    `Server` header, occupied-prefix and cleanup rules are 743.3;
  - mapping the client's results onto the observation type (743.3), including telling the base
    client's one `S3Error::Unreadable` apart for a chunked PUT/DELETE 2xx;
  - a digest-based body for large objects (#761 adds one when it needs it); smoke sends small
    sizes only;
  - any fact (F) does not list; the rest of `lib.rs` (its "#743" sentence, `lib.rs:10-11`, is
    743.3's); `Cargo.toml`; `docs/design/architecture/05-building-block-view.md`;
  - the cross-sweep oracle, event stream and verdict artifact (#746, #747, #764); MinIO's
    profile (#751); authentication vectors (#745); the availability budget itself;
  - any gateway change; any `wyrd-*` crate in `[dependencies]`; `Cargo.lock`; `deny*.toml`.
- **Difficulty:** medium
- **Do model:** opus
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/validate/tests/capability_matrix.rs` (NEW, child-1);
  `crates/validate/tests/capability_matrix_required.rs` (NEW, child-2)
- **Verification posture:** per child (*Split*). Child-1 is net-new: with production reverted
  its test cannot compile, so C4-verify reports UNVERIFIABLE (exit 77; this instance's gate
  compiles each added `*/tests/*.rs`, `engine/scripts/run-verify.sh:403-413`), accepted as at
  iteration 7's sign-off, with demonstrated reds instead. Child-2 changes no public type, so
  its new test compiles against child-1's module and earns a real red→green.
- **Production reach:** consumed by 743.3 (#939) in this same run; until then nothing in the
  binary calls the module. Every test leg calls the production functions, not a copy.
- **Citations expected:** Do must cite `path:line` on the target branch for every change.
  - **Peer to start from:** iteration 9's module and test, `results/issue_937/iteration-v9/
    patch.diff` (`crates/validate/src/matrix.rs`, `crates/validate/tests/capability_matrix.rs`,
    and the 0017 hunks). Review found no defect in its profiles, version keying, verdict fold
    or grammars (`is_strong_etag`, `parse_content_range`, `parse_version`). Its classification
    is restated by (B)–(G), not patched: its early returns (`check_delete_result`'s duplicate
    guard, `matrix.rs:1187-1193` there; `expect_bytes`, `:889-934`, which never reads
    `content_length`) are what (B) step 3 removes.
  - Each `pre-0.1` row cites the gateway line giving today's answer
    (`crates/gateway-s3/src/lib.rs` on `9b1a3e2`): `CopyObject` `:1865-1871`; the object-path
    subresource denylist `:1842-1849`, its list `:410-422`; bulk `POST ?delete` `:1766-1788`;
    the bucket-path denylist `:1797-1804`; a bucket path with any other method `:1809-1815`;
    the root path `/` `:1819-1826`; `DeleteObject` 204 `:2028`; `DeleteObjects` 200 with errors
    `:2293-2300`; the `Server` header `:1694-1699`.
  - The (F) facts against the gateway: PUT 200 declares `content-length: 0`
    (`put_object_response`, `:3401-3404`); a 204 declares none (`empty_response`, `:3384`);
    ranged 206 declares `content-length` = the span and `Content-Range` (`:2704-2725`); 304
    carries no body and no length (`not_modified_response`, `:2785-2803`). The client exposes
    a GET's declared length (`ObjectBody::content_length`, `crates/validate/src/s3/body.rs:431-433`).
  - The version string's shape: `normalize_describe`,
    `crates/server/src/version/derivation.rs:33-68`: `0.0.0+git.<sha>[.dirty]` untagged,
    `0.1.0` on a tag, `0.1.0+git.dirty` dirty on a tag (`:60`), `0.1.0+git.3.<sha>` past one;
    `+git.unknown[.dirty]` at `:65`.
  - The PUT receipt rule: `PutObject`'s doc, `crates/validate/src/s3.rs:169-178`;
    `BodyError::AcknowledgedEarly`, `crates/validate/src/s3/error.rs:101-114`. The only client
    type the matrix uses: `ErrorCode`, `crates/validate/src/s3/error.rs:68-76`, re-exported at
    `crates/validate/src/s3.rs:60` and `crates/validate/src/lib.rs:34-36`.
  - Grammars: RFC 9110 §8.8.3 (entity tags) and §14.4 (`Content-Range`), under the
    grammar-strictness rule `AGENTS.md:165-169`.
- **Prior-art check (triage cycles):** searched by path `crates/validate/` and
  `docs/design/proposals/draft/0017-blackbox-validation-tool.md` across merged, open and
  closed-unmerged PRs on getwyrd/wyrd (re-run 2026-10-24; `main` unchanged at `9b1a3e2` on
  2026-10-25). Merged: #845, #849, #859, #860, #865, #886, #765, #931 (#744), #934 (#853).
  Open: none on these paths. Closed-unmerged: none overlapping (iteration 9's reviewer checked
  all 19). No matrix code exists on `main`; this bundle's nine attempts are the only prior
  matrix code (`iteration-v1/` … `iteration-v9/`).
- **Disposition hint:** new-feature

## Design

- **The observation type is the matrix's own**, the contract 743.3 maps onto (approved
  2026-10-24). No answer (and why); or a response head: status, `ETag`, `Content-Range`,
  `Content-Length`, the S3 error code or its absence; and the body: object bytes; a typed
  listing (key and size, every page together) or `<DeleteResult>` (`<Deleted>` keys,
  `<Error>` rows); arrived whole but not the document its status calls for (and why); or cut
  off after the head, carrying the bytes that arrived for an object body or only their count
  for a buffered one. A PUT records whether its request body had ended when the head arrived.
  Each exercise records what it asked: the range; for both conditional forms the sent value,
  the object's tag and the object's bytes (the matching form's bytes are new against
  iteration 9, for (G)); the object's size and tag for `HEAD`; the requested keys; the keys
  and sizes written under a listed prefix. A delete carries each follow-up `HEAD` as its own
  answer-or-no-answer, with the key it asked about.
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
  the tracker body's older "Retention starts at the first tag"; it is the human's call, not
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

## Split (decided in this Plan session, 2026-10-25)

Two children; child-2 depends on child-1. Iteration 9 was 99.6 KB against the 100 KB backstop;
the class rules add code, so one patch would land over it. The seam is the one the sign-off
named, and it falls where iteration 9's code already divides:

- **child-1 — the core** (≈60 KB, mostly carried over from iteration 9): (A), (B) and (C) as
  they apply to `unsupported` and `supported, not checked` rows, (D), (E), (H); the whole
  observation type of *Design*, so child-2 changes no public type; the 5xx fact on every row;
  the two 0017 amendments. A `required` row is judged by the 5xx fact and is otherwise
  unsettled until child-2. Its tests assert nothing else about a `required` row's answer, so
  child-2 never edits them. Test file `crates/validate/tests/capability_matrix.rs` (NEW).
- **child-2 — the required-row checks** (≈50 KB): (B), (C), (F), (G) for every `required`
  row, the 404 fact, and the full passing run against `pre-0.1`. No public type changes. Test
  file `crates/validate/tests/capability_matrix_required.rs` (NEW), fixtures chosen per class
  and per precedence pair, not per case — including the follow-up key pair: delete `a` → 204,
  follow-up `HEAD` of `b` (not requested) → 200 is unsettled; of `a` → 200 is `oracle`; of
  `b` → 500 is `protocol`; the same three on `DeleteObjects`.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR MAY
happen during the cycle. The PR MUST NOT be marked ready before sign-off accepts.

Plan-review response (2026-10-25): two findings revised in place, one stands on a recorded
human decision. (1) Holds: the deleted-key `oracle` and the follow-up's 404 status check now
apply only to a follow-up of a key the delete requested; a follow-up of any other key gets only
the key-independent checks (5xx, a byte on a no-body answer) and is unsettled; "typed read" no
longer covers a follow-up; child-2 carries the paired fixtures (unrequested 200 → unsettled,
requested 200 → `oracle`, unrequested 500 → `protocol`). (2) Stands: rc tags select `pre-0.1`
and retention starts at `0.1.0` by the human's iteration-7 sign-off (2026-10-19), which
replaces the tracker body's older wording. The brief now marks it as decided, cites it, and
has the 0017 paragraph state the cost the reviewer found (an rc's own expectations are not
kept). Freezing at the rc would only move the false failure onto `main` builds past the rc,
because a past-tag build selects the tag's profile (`derivation.rs:55`). (E) also pins
`0.1.0-rc.1+git.3.<sha>`. (3) Holds as a fact, not as a brief defect: the `93a6f37` target came
from iteration 9's stale `stack-base` file, which the driver deletes before Do
(`flow.py:2204-2209`); the brief now says so and makes Do's first step an ancestry check
against `9b1a3e2` that stops the build, without any rebase or lockfile repair, if it fails.
`split-proposal.md`'s two child briefs carry the same edits.
