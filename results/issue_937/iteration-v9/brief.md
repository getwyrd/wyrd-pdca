# validate: the capability matrix — profiles, version selection, answer classification (743.1)

> Do reads ONLY this file. Plan of record: proposal 0017,
> `docs/design/proposals/draft/0017-blackbox-validation-tool.md`. Read §3 *The capability
> matrix* (`0017:188-275`) and §7 *Failure taxonomy* (`0017:482-524`) before writing code.
> First of three children of #743. This one is pure: it sends nothing.
> Siblings: 743.1 = #937 (the matrix), 743.2 = #938 (the client verbs), 743.3 = #939 (smoke).
>
> **Re-planned 2026-10-24 after seven builds** (iteration 7's sign-off: iterate-plan). The
> module is close to done: iteration 7's `matrix.rs` passed review with no correctness defect,
> and its patch still applies cleanly on `main` `9b1a3e2`. What changed is policy, settled
> below with the human and marked **(decided 2026-10-24)**: an answer the server sends that is
> not the defined one is a FAIL on every row, a 5xx included; the conditional GETs record the
> tag they sent; a 404 to a key the run wrote is `oracle`. Earlier attempts are in
> `iteration-v1/` … `iteration-v7/`; the rest of this brief is the whole spec.

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
- **Falsifiability:** RED is reachable on the plain `cargo test -p wyrd-validate --test
  capability_matrix` target that `cargo xtask ci` (gate C4-ci, gating) runs: it is a plain
  integration test with no `cfg` or feature gate, and every leg calls a pure function with a
  fixed input. So a classifier that lets a 2xx through, treats a 5xx on a required row as
  unsettled, accepts `*` as an ETag, passes a PUT answered before its body ended, passes a
  non-matching conditional GET whose sent tag equals the object's, calls a 404 to a written
  key `protocol`, passes `DeleteObjects` on its document alone, calls a delete `oracle` when
  its follow-up `HEAD` got no answer, lets a wrong byte that arrived before a cut read as
  unsettled, reads a malformed error document that arrived whole as a cut-off one, reads
  `+git.unknown` as `0.0.0`, or ignores the version fails an assertion. No network, Docker or
  second process.
- **Invariant to restore:** the verdict never claims more than the answers prove, and never
  less. An `unsupported` operation that succeeds in any part is fatal (`0017:197-199`); a
  `required` operation passes only on an answer showing the operation was done (`0017:190-195`,
  §7 `0017:482-524`); an answer the server gave that is not the defined one is a FAIL, never
  budgeted (the human's rule, 2026-10-24; §7 amended to match, *Design*); only an exercise
  that leaves nothing to judge stays INCONCLUSIVE, and a cut never hides a wrong byte that
  arrived before it (`0017:484-485`); "version unknown" is never mistaken for a release
  (`0017:257-261`).
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:**
- **Conflicts with:**
- **Ordering note:** runs in parallel with 743.2 (no shared file); 743.3 depends on both.
  **Base.** This slice declares no prerequisite, and the patch does not depend on the base:
  it adds files, one `lib.rs` line and a draft-doc edit, and uses only `s3::ErrorCode`
  (`crates/validate/src/s3/error.rs:68-76`, re-exported at `crates/validate/src/s3.rs:60` and
  `crates/validate/src/lib.rs:34-36`). The intended base is `main` at or after `9b1a3e2`, which
  carries #744 (PR #931), #853 (PR #934), and the cargo-deny fixes #950 and #954; iteration 7's
  patch applies there cleanly. If the run hands Do the batch's old integration line
  (`pdca-integration/main-r42c5348f203d`, tip `93a6f37`, recorded in this bundle's
  `stack-base`), the two `cargo deny` advisory steps of C4-ci fail on that base's `Cargo.lock`
  (RUSTSEC-2026-0385, -0386). The human accepted that red as inherited at iteration 7's
  sign-off; it is not this slice's to fix and Do must not touch `Cargo.lock` or `deny*.toml`.
  Do records in `build-notes.md` which base it built on.
- **Scope:** the `matrix` module: operations, rows in three states, immutable profiles with
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
- **Difficulty:** medium
- **Do model:** opus
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/validate/tests/capability_matrix.rs` (NEW)
- **Verification posture:** (a) net-new. The `matrix` module does not exist on the base, so
  with production reverted the test cannot compile, and C4-verify reports UNVERIFIABLE (exit 77;
  this instance's gate compiles every added `*/tests/*.rs` together,
  `engine/scripts/run-verify.sh:404-413`). The human accepted this exception at iteration 7's
  sign-off. Built and exercised at Check: the whole module, through the pure test.
  **Demonstrated reds (required, recorded in `build-notes.md`):** make each temporary edit,
  show the named leg fail, restore it: (1) a 2xx to an unsupported row classified as a pass →
  (B1); (2) a 2xx head whose body failed classified as no answer → (B1); (3) the PUT ETag check
  accepting any non-empty value → (B2); (4) the non-matching conditional GET accepting a 304 →
  (B2); (5) `DeleteObjects` passing on its document alone → (B2); (6) the selector ignoring the
  version → (C); (7) `+git.unknown` parsed as release `0.0.0` → (C); (8) a `supported, not
  checked` row counted as unexercised → (D); (9) the PUT check ignoring when its head arrived →
  (B2); (10) a delete whose follow-up `HEAD` had no answer classified as `oracle` → (B2)/(E);
  (11) a 5xx on a required row classified as unsettled → (B); (12) the non-matching
  conditional check ignoring the sent tag → (B2); (13) a 404 to a written key's `HEAD`
  classified as `protocol` → (B2); (14) a cut-off body classified as unsettled without looking
  at the bytes that arrived before the cut → (B); (15) an error body that arrived whole but is
  not an `<Error>` document classified like a cut-off one → (B1).
- **Production reach:** consumed by 743.3 (smoke) in this same run; until it lands nothing in
  the binary calls the module. Every leg calls the production functions, not a copy.
- **Citations expected:** Do must cite `path:line` on the target branch for every change.
  - **Peer to start from:** iteration 7's module and test, `results/issue_937/iteration-v7/
    patch.diff` (`matrix.rs`, 1,486 lines; `capability_matrix.rs`). Review found no correctness
    defect in it. Rebuild from it with the deltas above; do not restart from scratch. Its
    `required_response` (`matrix.rs:946-954` there) is the "5xx is availability" branch (B)
    removes; its `probe_deleted` (`:1171-1191`) carries the follow-up 5xx arm (B2) changes; its
    `GetObjectRanged` exercise (`:735-740`, guard `:1074-1079`) is the pattern the conditional
    exercises copy: the exercise says what was asked, and an impossible ask is unsettled. Its
    `Body::Failed` (`:692-695`) carries no bytes and has no "arrived whole but unreadable"
    sibling; its `body_bytes` and `expect_no_body` (`:967-993`), the unsupported row's cut arm
    (`:926-930`) and the listing and `<DeleteResult>` arms (`:1140-1143`, `:1255-1258`) are
    where (B)'s cut rule changes. Their rustdoc ("a body cut off in transport settles nothing",
    `:967`, `:981`) changes with them.
  - Each `pre-0.1` row cites the gateway line giving today's answer
    (`crates/gateway-s3/src/lib.rs`, unchanged between `4bac538` and `9b1a3e2`): `CopyObject`
    `:1865-1871`; the object-path subresource denylist (`uploads`, `uploadId`, `partNumber`)
    `:1842-1849`, its list `:410-422`; bulk `POST ?delete` `:1766-1788`; the bucket-path
    denylist (`?uploads`, `?location`) `:1797-1804`; a bucket path with any other method
    `:1809-1815`; the root path `/` `:1819-1826`; ranged 206 `:2713-2719`; 304 `:2785-2803`;
    `DeleteObjects` 200 with errors `:2293-2300`; the `Server` header `:1694-1699`.
  - The version string's shape: `normalize_describe`,
    `crates/server/src/version/derivation.rs:33-68`: `0.0.0+git.<sha>[.dirty]` untagged,
    `0.1.0` on a tag, `0.1.0+git.dirty` dirty on a tag (`:60`), `0.1.0+git.3.<sha>` past one;
    `+git.unknown[.dirty]` at `:65`.
  - Entity tags and `Content-Range`: RFC 9110 §8.8.3 and §14.4, under the grammar-strictness
    rule `AGENTS.md:165-169` (validate every token; no `+`/`-` signs via `from_str`).
  - **Size budget:** keep `patch.diff` under **100,000 bytes** (iteration 7: 101,896, over the
    backstop). The new legs add code, so make room by tightening rustdoc that restates this
    brief and by sharing test helpers, never by dropping a fixture or a check named above.
- **Prior-art check (triage cycles):** searched by path `crates/validate/` and
  `docs/design/proposals/draft/0017-blackbox-validation-tool.md` across merged, open and
  closed-unmerged PRs on getwyrd/wyrd (re-run 2026-10-24). Merged: #845, #849, #859, #860, #865
  (crate, dependency guard, client, dist, early-answer fix), #886 (the `Server` header), #765
  (0017), and since the last brief #931 (#744, payloads and size classes) and #934 (#853,
  non-conforming responses), both 2026-10-23. Open: none touching these paths.
  Closed-unmerged: none. No matrix code exists on `main`. This bundle's own seven attempts are
  the only prior matrix code (`iteration-v1/` … `iteration-v7/`).
- **Disposition hint:** new-feature

## Design

- **The observation type is the matrix's own.** What the classifier reads: no answer (and
  why); or a response head (status, the headers the checks read: `ETag`, `Content-Range`,
  `Content-Length`), the S3 error code or its absence, and the body: its bytes; a typed listing
  or delete result; **arrived whole but not the document its status calls for** (and why);
  or **cut off after the head**, carrying the bytes that arrived before the cut for an object
  body, or only how many arrived for a body the client buffers. For a PUT it also carries
  whether the request body had ended when the response head arrived (the fact the client
  reports as `BodyError::AcknowledgedEarly`), so an early success survives the mapping and
  cannot read as a receipt. Each conditional GET carries the `If-None-Match` value it sent and
  the object's tag. A check that needs more than one request (a delete and its follow-up
  `HEAD`s) takes each follow-up as its own answer-or-no-answer. 743.3 maps the client's results
  onto it. This keeps the matrix independent of the client's types and of 743.2.
  **Approved as the contract 743.3 builds against (2026-10-24); the two body states in bold
  were added in the plan-review pass the same day, for the human to confirm at sign-off.**
- **Where 743.3 gets each fact (a note for 743.3; nothing here is built in this slice).** The
  new verbs and probes report the two states apart, typed: #955's `Outcome::Unreadable { body,
  detail }` (read whole, not what its status calls for) against `Outcome::BodyFailed(..)`,
  whose `BodyFailure::Transport` / `Timeout` carry `received` (salvage
  `results/issue_938/iteration-v3/patch.diff:471-509`). #956's `probe` (every `unsupported`
  row) and #955's `head_object` return that `Answer` (`:1757`, `:1580`). #939 already depends
  on both. Object bodies stream: the whole GET through the base's `ObjectBody::next_piece`
  (`crates/validate/src/s3/body.rs:437-449`, `:477-498`), the ranged and conditional 200s
  through #955's `Fetched::Object` (`:581-592`). Each piece reaches the caller before a later
  failure, so 743.3 holds the bytes that came before a cut. The base's old verbs report both
  states as one `S3Error::Unreadable` with a text `detail` (`crates/validate/src/s3.rs:380-384`,
  `:399-404`). They serve only `required` rows whose defined answer is a 2xx, so a non-2xx is
  fatal on its status alone and its body never decides. **One case is left, and it is not this
  slice's (NEEDS-HUMAN):** a PUT or DELETE 2xx reported as `Unreadable` may be a byte where the
  success has none (`Refusal::BodyOnSuccess`, `crates/validate/src/s3/response.rs:175-177`) or
  a body framed and then cut before its first byte. The base client does not tell these apart,
  and 743.3 must not read `detail` to do it. Assign it (a client change, or a rule 743.3's
  brief states) before 743.3's mapping is final. The matrix's states hold either answer.
- **Version selection** compares the release part, pre-release included; build metadata never
  changes the selection, except that the `git.unknown` identity means unknown.
- **0017 §3 amendment** (a draft proposal, editable: `.github/workflows/docs-immutability.yml`
  freezes proposals only once accepted). Carry iteration 7's §3 and *Dependencies* hunks
  (its `patch.diff`, the 0017 file): the third state in the state list at `0017:193-195`;
  `0017:259-261` corrected (since #886 every response carries `Server: wyrd/<version>`, so the
  version is read from the wire and `--server-version` is an override); the *Dependencies*
  paragraph at `0017:858-863` marked landed; and after `0017:274-275` the paragraph that
  `pre-0.1` describes `main` and is edited in place until the first tagged release. One
  correction to that paragraph: retention starts at the first **`0.1.0` release**, not "the
  first tag", since a release candidate (`0.1.0-rc.1`) still selects `pre-0.1` by (C).
- **0017 §7 amendment (decided 2026-10-24).** The taxonomy table at `0017:487-493`: the
  `protocol` row reads "wrong status (a 5xx included), error code, or XML shape"; the
  `availability` row loses "5xx" and covers only failures that leave no answer to judge:
  transport failure, connection reset, a timeout before the response head, a body cut off after
  it (unless what arrived before the cut is already wrong, which is `integrity` or
  `protocol`). Add one sentence after the `protocol` paragraph (`0017:495-500`): a 5xx is an
  answer, and an answer other than the defined one is an implementation error, fatal like any
  wrong status; only the transport-shaped failures are budgeted. The opening line (`0017:484-485`, "must not
  abort at hour three because one request timed out") stays true and stays.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR MAY
happen during the cycle. The PR MUST NOT be marked ready before sign-off accepts.

Plan-review response: both findings revised in place (2026-10-24). (1) Bytes that arrived
before a cut now count: a wrong prefix is `integrity`, a byte on a bodiless answer is
`protocol`, and a correct prefix stays unsettled (criterion (B), paired fixtures in (B3), red
14). (2) The observation type now keeps "arrived whole but unreadable" (`protocol`) apart from
"cut off" (unsettled) ((B), (B1), fixtures in (B3), red 15). *Design* names where 743.3 gets
each fact: typed from #955/#956 for every `unsupported` row, `HEAD`, listing and bulk delete.
The one case the base client does not split, a PUT or DELETE 2xx reported as
`S3Error::Unreadable`, is flagged NEEDS-HUMAN for 743.3 and not built here.

## Iteration 8 — carry-forward (from the previous attempt)
- Sign-off rationale: Fix the one grading bug the reviewer, code-review leaf and all three batch-review passes found: in `classify_unsupported` (crates/validate/src/matrix.rs, ~:840/:857), a `DeclaredCode::NoBody` row (HeadBucket's declared 400 with no body) must run the existing `expect_no_body` check BEFORE the generic `Body::Cut` -> unsettled arm. A cut after one or more bytes (`Arrived::Count(n>0)` or `Arrived::Bytes(non-empty)`) is fatal `protocol` / run FAIL; a complete non-empty `Body::Bytes` on a NoBody row is also `protocol` (today it passes); an empty body passes; a cut after zero bytes stays unsettled. Add row and run tests for these cases in both arrival forms (extend `head_bucket_passes_on_a_400_with_no_body_only` and include HeadBucket in the bodiless-response test). Keep patch.diff under 100,000 bytes (it is at 99,724): make room by tightening rustdoc that restates the brief or by sharing test helpers, never by dropping a fixture or a check. Everything else stands: cargo-deny red (RUSTSEC-2026-0385/-0386) is inherited from main; the adversary leaf failed on a spend limit, not a finding; the carried-over findings are already handled.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Fix the one grading bug the reviewer, code-review leaf and all three batch-review passes found:
  in `classify_unsupported` (crates/validate/src/matrix.rs, ~:840/:857), a `DeclaredCode::NoBody`
  row (HeadBucket's declared 400 with no body) must run the existing `expect_no_body` check BEFORE
  the generic `Body::Cut` -> unsettled arm. A cut after one or more bytes (`Arrived::Count(n>0)` or
  `Arrived::Bytes(non-empty)`) is fatal `protocol` / run FAIL; a complete non-empty `Body::Bytes` on a
  NoBody row is also `protocol` (today it passes); an empty body passes; a cut after zero bytes stays
  unsettled. Add row and run tests for these cases in both arrival forms (extend
  `head_bucket_passes_on_a_400_with_no_body_only` and include HeadBucket in the bodiless-response test).
  Keep patch.diff under 100,000 bytes (it is at 99,724): make room by tightening rustdoc that restates
  the brief or by sharing test helpers, never by dropping a fixture or a check.
  Everything else stands: cargo-deny red (RUSTSEC-2026-0385/-0386) is inherited from main; the
  adversary leaf failed on a spend limit, not a finding; the carried-over findings are already handled.
- Failing gate: C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) —   - cargo deny --all-features --config deny-all-features.toml check advisories: exited with exit status: 1
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 174 mutants tested in 8m: 3 missed, 139 caught, 32 unviable
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_937/review-b
- Full previous attempt preserved in `iteration-v8/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 9 — carry-forward (from the previous attempt)
- Sign-off rationale: Re-plan around the 0017 §7 failure classes, not individual cases. Nine attempts each fixed the last edge case and review found the next one in the same grading code (iteration 9: declared Content-Length ignored on GET reads before a cut, matrix.rs:889; DeleteObjects duplicate-key guard runs before the <Error> and document-shape checks, matrix.rs:1186). The patch is at 99,636 of 100,000 bytes, so case-by-case patching cannot converge. Next brief: state each class (integrity, protocol, oracle, unexpected-support, unsettled / no verdict) as one rule that applies to every row: what evidence triggers it, and the order of precedence (a fatal fact already shown in the head or the bytes that arrived beats a cut, and beats any guard that only makes the result unsettled). Then check each row against those class rules, and pick fixtures per class, not per case. Consider splitting the slice (e.g. profiles + version selection + unsupported rows + verdict fold / required-row checks) if the class rules still don't fit the budget. Open scope call to settle in Plan: whether the matrix judges a PUT 2xx that declares a Content-Length and is cut off (length-framed case), or 743.3 owns it. Inherited cargo-deny red (RUSTSEC-2026-0385/-0386) is not a patch defect; rebasing onto current main may clear it.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Re-plan around the 0017 §7 failure classes, not individual cases. Nine attempts each fixed
  the last edge case and review found the next one in the same grading code (iteration 9:
  declared Content-Length ignored on GET reads before a cut, matrix.rs:889; DeleteObjects
  duplicate-key guard runs before the <Error> and document-shape checks, matrix.rs:1186).
  The patch is at 99,636 of 100,000 bytes, so case-by-case patching cannot converge.
  Next brief: state each class (integrity, protocol, oracle, unexpected-support, unsettled /
  no verdict) as one rule that applies to every row: what evidence triggers it, and the order
  of precedence (a fatal fact already shown in the head or the bytes that arrived beats a cut,
  and beats any guard that only makes the result unsettled). Then check each row against those
  class rules, and pick fixtures per class, not per case. Consider splitting the slice
  (e.g. profiles + version selection + unsupported rows + verdict fold / required-row checks)
  if the class rules still don't fit the budget. Open scope call to settle in Plan: whether
  the matrix judges a PUT 2xx that declares a Content-Length and is cut off (length-framed
  case), or 743.3 owns it.
  Inherited cargo-deny red (RUSTSEC-2026-0385/-0386) is not a patch defect; rebasing onto
  current main may clear it.
- Failing gate: C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) —   - cargo deny --all-features --config deny-all-features.toml check advisories: exited with exit status: 1
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 171 mutants tested in 8m: 1 missed, 140 caught, 30 unviable
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_937/review-b
- Full previous attempt preserved in `iteration-v9/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
