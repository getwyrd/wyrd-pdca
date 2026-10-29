<!-- pdca:split-proposal v1 -->
# Split proposal — issue 743

> The iterate-to-Plan of #743. Sign-off (Eduard Ralph, 2026-10-19) returned the first attempt
> as oversized (256 KB, never split) and asked for a split. Authored with the human in the Plan
> session of 2026-10-22; every point marked **(decided)** is the human's decision from that
> session. Citations verified on 2026-10-22 against `getwyrd/wyrd` `origin/main` @ `4bac538`
> and the batch's integration line `origin/pdca-integration/main-r42c5348f203d` @ `582da99`
> (main plus #653, #744, #853 folded; `crates/gateway-s3/src/lib.rs` is the same blob on both).
> Intake cap (wyrd-pdca-P1): blackbox 0/6 when written, room 6, need 3.

## Why this slice is oversized

#743 is three outcomes, each with its own test file and its own pass/fail condition, and the
first attempt shipped them as one 256 KB patch. Split three ways **(decided)**, not the two the
sign-off sketched, because the two-way cut leaves the second child about 170 KB, over the
100 KB backstop again.

- **The measured split.** By file, the first attempt (`iteration-v1/patch.diff`) was: matrix
  module + its test 87 KB (`matrix.rs` 52.6, `capability_matrix.rs` 34.4); client verbs 49 KB
  (`s3.rs` 21.8, `s3/document.rs` 12.4, `s3/response.rs` 8.7, `s3/capture.rs` 6.0); smoke runner,
  arguments and wiring with its loopback test 95 KB (`smoke.rs` 31.0, `args.rs` 14.4, `lib.rs`
  9.8, `smoke_loopback.rs` 40.0); docs 23.6 KB, almost all of it from rewriting the 8.9 KB line
  `05-building-block-view.md:297` (a new paragraph costs a fraction). The fault fixtures the
  sign-off now requires add to the client and smoke halves.
- **The seams.** (1) The **matrix** is pure: what each operation must answer, chosen by server
  version, and how an answer is judged. (2) The **client verbs** report what the server sent.
  (3) **Smoke** sends them, judges the answers with the matrix, and owns everything that touches
  someone else's storage.
- **Each sign-off finding lands in one child.** ETag grammar, the conditional-GET negative
  control, `+git.unknown` and "a 2xx head is unexpected-support even if the body failed" are
  judgements (743.1). Keeping the 2xx head when the body fails is the client's (743.2). Unowned
  bucket deletion, the acknowledged-but-not-done bulk delete, reusing a malformed ETag, the
  `--scenario` refusal, the `--server-version` refusal and the `h`/`d` units are the runner's
  (743.3), as required reds.

## Wave sketch

- **743.1 and 743.2 are independent: same wave, in parallel.** 743.1 touches `matrix.rs` (new),
  one line of `lib.rs`, its test and 0017 §3; 743.2 touches `s3.rs`, `s3/*`, `Cargo.toml`'s
  dev-dependencies, its test. No shared file, so no `Conflicts with`.
- **743.3 stacks on both**, and on #744 (payloads, `args.rs`) and #853 (response reading). #744
  and #853 are COMPLETE in this run's batch and already folded onto the integration line, so
  the fold carries them. Run outside this batch, 743.2 and 743.3 are held, not built on a base
  missing them, until PR #934 (#853) and #931 (#744) merge into `main` or are named in the same
  `pdca flow` command (`src/pdca_harness/flow.py:681-767`).
- **Outside the split: #508.** Once 743.1 and 743.3 land, a gateway that accepts multipart
  turns the in-tree smoke test red with `unexpected-support`. That is the matrix doing its job.
  #508's brief carries the note on what to change (the `pre-0.1` rule 743.1 writes into 0017 §3).

<!-- pdca:child child-1 -->
# validate: the capability matrix — profiles, version selection, answer classification (743.1)

> Do reads ONLY this file. Plan of record: proposal 0017,
> `docs/design/proposals/draft/0017-blackbox-validation-tool.md`. Read §3 *The capability
> matrix* (`0017:188-275`) and §7 *Failure taxonomy* (`0017:482-524`) before writing code.
> First of three children of #743. This one is pure: it sends nothing.

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
    state (D) today.
  - **(B) Unsupported rows.** For an observed answer the classifier returns: pass for the
    declared status and code exactly; `protocol` (fatal) for any other status or code, a 5xx
    included; **`unexpected-support` (fatal) for any 2xx**, including a 2xx response head
    whose body then failed or timed out; and no verdict (cannot conclude) when no response
    head arrived at all.
  - **(B′) Required rows have checks of their own** (0017 §7 classes). Each passes only when:
    `PutObject` 200 with an `ETag` that is a strong entity-tag (RFC 9110 §8.8.3: `"`, etagc*,
    `"`), so `*`, an unquoted value, a weak `W/"…"` tag and an empty value are each `protocol`;
    whole `GetObject` 200 whose body equals the expected bytes, else `integrity`; ranged
    `GetObject` **206** with `Content-Range: bytes a-b/<size>` and exactly those bytes;
    conditional `GetObject` with the matching tag **304** and no body; conditional `GetObject`
    with a valid strong tag that differs from the object's **200** with the whole body (the
    negative control that tells "compares tags" from "answers 304 to anything", **(decided)**);
    `HeadObject` 200 with the object's size and the PUT's `ETag`, else `oracle`;
    `ListObjectsV2` and v1 200 naming exactly the keys written under the listed prefix, with
    their sizes, else `oracle`; `DeleteObject` 204 and then a `HEAD` of the key answers 404,
    else `oracle`; `DeleteObjects` 200 with every requested key under `<Deleted>` and no
    `<Error>` row (the gateway answers 200 even when keys failed,
    `crates/gateway-s3/src/lib.rs:2293-2300`), **and then** a `HEAD` of each of those keys
    answers 404, else `oracle`: the document alone never proves a delete.
    Negative fixtures, each giving the named class and making the run FAIL: a 200 with the
    whole body to the ranged GET; a 206 with the wrong `Content-Range`; a 200 to the matching
    conditional GET; a 304 to the non-matching one; a `DeleteObjects` 200 with one `<Error>`
    row; a `DeleteObjects` 200 naming every key under `<Deleted>` followed by a `HEAD` that
    answers 200 (`oracle`); a GET body one byte off (`integrity`); a PUT `ETag` of `*`; an
    unquoted one.
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
    exercised, or exercised only with no answer ⇒ INCONCLUSIVE, naming those rows (0017's
    vacuity rule); else PASS.
- **Falsifiability:** RED is reachable on the plain `cargo test -p wyrd-validate --test
  capability_matrix` target that `cargo xtask ci` runs: every leg calls a pure function with a
  fixed input, so a classifier that lets a 2xx through, accepts `*` as an ETag, accepts a 304
  to the non-matching tag, passes `DeleteObjects` on its document alone, reads `+git.unknown`
  as `0.0.0`, or ignores the version fails an assertion. No network, Docker or second process.
- **Invariant to restore:** the verdict never claims more than the answers prove. An
  `unsupported` operation that succeeds in any part is fatal (`0017:197-199`); a `required`
  operation passes only on an answer showing the operation was done (`0017:190-195`, §7
  `0017:482-524`); "version unknown" is never mistaken for a release (`0017:257-261`).
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:**
- **Conflicts with:**
- **Ordering note:** runs in parallel with 743.2 (no shared file); 743.3 depends on both. In
  this run it is built on the batch's integration line, which already carries #744 and #853,
  but it uses nothing of theirs: the matrix judges its own observation type, never the client's
  `S3Error` or `BodyError` (#853 changes those, `crates/validate/src/s3/error.rs`). It may use
  `s3::ErrorCode`, which #853 leaves unchanged.
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
  row counted as unexercised → (D).
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

## Design

- **The observation type is the matrix's own.** What the classifier reads: no answer (and
  why); or a response head (status, the headers the checks read: `ETag`, `Content-Range`,
  `Content-Length`), the S3 error code or its absence, and the body: its bytes, a typed listing
  or delete result, or the fact that it failed after the head. 743.3 maps the client's results
  onto it. This keeps the matrix independent of #853's types and of 743.2.
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
<!-- pdca:end child-1 -->

<!-- pdca:child child-2 -->
# validate: the S3 client verbs smoke sends — HEAD, ranged and conditional GET, listings, bulk delete, unsupported-operation requests (743.2)

> Do reads ONLY this file. Second of three children of #743. It extends the client #853
> hardened; it decides nothing about whether an answer is right (743.1 judges, 743.3 runs).

- **Slug:** validate-s3-client-smoke-verbs
- **Track:** blackbox
- **Kind:** enhancement
- **Defect:** the client can only PUT, GET and DELETE one object
  (`crates/validate/src/s3.rs:229`, `:281`, `:325` on the integration line). Smoke (743.3) also
  needs HEAD; a ranged GET; a GET with a given `If-None-Match`; ListObjectsV2 and v1 under a
  prefix; bulk DeleteObjects; and one request for each operation the matrix declares
  unsupported. Each must report what the server actually sent (status, the headers checks
  read, the S3 error code, the body) with #853's bounded, typed reading. And a 2xx head must
  never be lost: when a body fails after a 2xx head, the report still carries the 2xx, or a
  half-working unsupported operation reads as "no answer" instead of `unexpected-support` (the
  Check finding at the first attempt's `s3.rs:610`).
- **Success criterion:** the NEW file `crates/validate/tests/s3_client_smoke_verbs.rs` passes,
  against an in-process Wyrd gateway and, for the fault legs, a scripted endpoint or relay
  (peers under *Citations*):
  - **(A) The verbs against the real gateway.** HEAD of a written key → 200 with its size and
    `ETag`; of a missing key → 404 with no body (`ErrorCode::NoBody`). Ranged GET
    `bytes=a-b` → 206, the `Content-Range` header as sent, exactly those bytes. GET with
    `If-None-Match: <its ETag>` → 304, no body; with a different valid tag → 200 and the whole
    body. ListObjectsV2 and v1 under a prefix → every key with its size, across more than one
    page (the client follows the continuation). DeleteObjects of N keys → the `<Deleted>` and
    `<Error>` rows exactly as the document carried them; the keys are then gone.
  - **(B) Every unsupported operation reaches the gateway** and is reported with status and
    S3 code: `CopyObject` 501 `NotImplemented`; `CreateMultipartUpload`, `UploadPart`,
    `CompleteMultipartUpload`, `AbortMultipartUpload`, `ListParts`, `ListMultipartUploads`
    501; `GetBucketLocation` 501; `CreateBucket`, `DeleteBucket`, `ListBuckets` 400
    `InvalidRequest`; `HeadBucket` 400 with no body. `CreateBucket` and `DeleteBucket` naming
    a bucket S3's naming rules forbid (for example one ending in `-`) are **sent and reach the
    gateway**, never refused by the client: 743.3's bucket-safety rule depends on it.
  - **(C) A 2xx head survives its body.** A scripted endpoint answers an unsupported operation
    with a `200` head and then (i) stalls the body past a short body deadline
    (`S3Client::with_deadlines`), (ii) cuts the connection mid-body, (iii) sends a body that is
    not a readable document. In each case the report carries status 200 and the body failure,
    never only a timeout or transport error.
  - **(D) Unreadable answers are refused for the new verbs as #853 refuses them for the old.**
    A listing or `<DeleteResult>` document cut short, followed by junk, or past the buffered
    budget (`BUFFERED_BODY_BUDGET`) is a typed refusal, never a partial listing or a short
    delete result.
  - **(E)** `s3_client_roundtrip.rs`, `s3_client_upload_peers.rs` and
    `s3_client_nonconforming_responses.rs` pass unmodified.
- **Falsifiability:** RED is reachable on the plain `cargo test -p wyrd-validate --test
  s3_client_smoke_verbs` target: a real gateway on loopback for (A)/(B), a scripted endpoint
  with short deadlines for (C)/(D). A client that drops the 2xx head on a body failure, lets the
  SDK refuse a forbidden bucket name, stops listing after one page, or accepts a cut-off
  document fails an assertion. No Docker, no network beyond `127.0.0.1`.
- **Invariant to restore:** what the client reports is what the server sent. No answer is
  upgraded, none downgraded, nothing filled in on the server's behalf
  (`crates/validate/src/s3/error.rs:1-7`); a failure after a 2xx head never erases the 2xx.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:**
- **Conflicts with:**
- **Ordering note:** builds on #853's reading of responses (`s3/response.rs`, `s3/wire.rs`, the
  typed errors). #853 is COMPLETE in this run's batch and on the integration line. Runs in
  parallel with 743.1: it must not touch `crates/validate/src/lib.rs` (743.1 adds a line there,
  743.3 rewrites it); new types are reachable as `wyrd_validate::s3::…`.
- **Scope:** the new verbs and their typed results in `crates/validate/src/s3.rs` and
  `src/s3/*`; `wyrd-traits` (or whatever the bucket-seeding peer needs) under
  `[dev-dependencies]` in `crates/validate/Cargo.toml`; the new test file. / out of scope:
  - judging answers (743.1); the runner, ledger, version source and arguments (743.3);
    `lib.rs`;
  - the multipart success path (#508, #750); changing the existing verbs' behaviour;
  - any `wyrd-*` crate in `[dependencies]`; any new production dependency (`roxmltree` is
    already one); `deny*.toml`.
- **Difficulty:** medium
- **Do model:** opus
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/validate/tests/s3_client_smoke_verbs.rs` (NEW)
- **Verification posture:** (a) net-new. The verbs do not exist on the base, so with production
  reverted the test cannot compile and C4-verify reports UNVERIFIABLE (exit 77): pre-declared.
  Built and exercised at Check: every verb against the real gateway, every fault path against
  the scripted endpoint. **Demonstrated reds (required, in `build-notes.md`):** (1) return
  only the timeout when a body stalls after a 2xx head → (C)(i); (2) accept a listing document
  cut short → (D); (3) let the SDK's bucket-name handling refuse or rewrite the forbidden name
  → (B); (4) stop listing after the first page → (A).
- **Production reach:** consumed by 743.3 in this same run; until then only the test calls the
  new verbs.
- **Citations expected:** Do must cite `path:line` on the target branch for every change.
  - **Peer, response reading:** the existing verbs' `.customize()` hooks
    (`crates/validate/src/s3.rs:244`, `:288`, `:330`) and `S3Client::classify` (`:339`); the
    strict document reader `error_document` (`crates/validate/src/s3/response.rs:363`) and the
    `ResponseGuard` interceptor (`:209-236`). Read listings and delete results by the same
    rules (UTF-8, one well-formed document under the expected root, bounded depth and size).
  - **Peer, in-process gateway:** `start_gateway_with_chunk`,
    `crates/validate/tests/s3_client_roundtrip.rs:323-350`. A listing needs the bucket's
    marker seeded first (ADR-0046 decision 1): do it as `seed_bucket` does,
    `crates/server/tests/s3_list_objects.rs:82-88`, before the store goes into `Gateway::new`.
  - **Peer, faults:** the relay's `Downstream` modes (`HoldAfter`, `CutAfter`),
    `crates/validate/tests/s3_client_roundtrip.rs:465-477`; the scripted endpoint
    `crates/validate/tests/s3_client_nonconforming_responses.rs:354` (`serve` `:489`, `head()`
    `:247`, `error_doc()` `:220`).
  - Deadlines: `Deadlines` defaults `crates/validate/src/s3.rs:98-107` (body idle 60 s): use
    `S3Client::with_deadlines` for (C).
- **Prior-art check (triage cycles):** searched by path `crates/validate/src/s3.rs`,
  `crates/validate/src/s3/` across merged, open and closed-unmerged PRs (2026-10-22): merged
  #859 (the client), #865 (early answers); open #934 (#853); closed-unmerged: none. The rejected
  first attempt's `s3.rs`, `capture.rs`, `document.rs` had one finding: the 2xx head lost on a
  body timeout, now (C).
- **Disposition hint:** new-feature

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR MAY
happen during the cycle. The PR MUST NOT be marked ready before sign-off accepts.
<!-- pdca:end child-2 -->

<!-- pdca:child child-3 -->
# validate: the smoke scenario — the matrix run against a live endpoint, touching only what it wrote (743.3)

> Do reads ONLY this file. Plan of record: proposal 0017,
> `docs/design/proposals/draft/0017-blackbox-validation-tool.md`: §3 (`0017:188-275`), §7
> (`0017:482-524`), §8's `smoke` row and scale rules (`0017:526-561`), §15 *Safety*
> (from `0017:782`; ownership `:791-837`). Third child of #743; builds on 743.1 (the matrix) and 743.2 (the verbs).

- **Slug:** validate-smoke-scenario
- **Track:** blackbox
- **Kind:** enhancement
- **Defect:** `wyrd-validate` resolves its configuration and always exits INCONCLUSIVE
  (`crates/validate/src/lib.rs:132-160`): no scenario runs. The in-tree test
  `crates/server/tests/s3_copy_object_guard.rs:175` already proves this tree's gateway refuses
  `CopyObject`. What is missing is a check an operator runs against a **deployed** endpoint,
  keyed to the version it reports, covering every unsupported operation and every required
  one. That is `smoke`. It is also the first part of the tool that writes and deletes on
  someone else's storage, so it must never delete, overwrite or change anything it did not
  write in this run. The first attempt broke that three ways at Check: it could delete a
  bucket it did not own, it reported PASS and "nothing left" after a bulk delete that was
  acknowledged but not done, and it accepted and reused a malformed `ETag`.
- **Success criterion:** the NEW file `crates/validate/tests/smoke_loopback.rs` passes. Every
  leg drives the **real binary** (`env!("CARGO_BIN_EXE_wyrd-validate")`) or the existing
  `wyrd_validate::run` (`lib.rs:132`, signature unchanged) against an in-process Wyrd gateway
  with the test's buckets seeded, through a recording relay where a leg must see or change the
  wire; `--workers 1` and a short `--duration` (e.g. `1s`) unless the leg says otherwise.
  - **(A) PASS against loopback.** Gateway `server_version` `0.0.0+git.<sha>`: exit 0; the
    summary names every `required` and `unsupported` row of `pre-0.1` with its passes; the relay
    saw every unsupported operation's request, `CopyObject` included, answered as declared;
    afterwards nothing is under `<run-id>/` and no key outside it was written or changed.
  - **(B) A wrong success fails the run.** (i) The relay answers `CopyObject` itself with a 200
    and a `<CopyObjectResult>`: exit with the FAIL status (new, distinct from 0–3), naming
    `unexpected-support` for `CopyObject`. (ii) The relay answers `CreateMultipartUpload` with a
    200 head and cuts the connection mid-body: still FAIL, `unexpected-support`, never
    INCONCLUSIVE.
  - **(C) The version source (decided).** A `Server: wyrd/0.0.0+git.<sha>` header is used and
    the echo says `(from Server header)`. `--server-version` wins over it and the echo says
    `(given)`. With `wyrd/unknown`, `wyrd/0.0.0+git.unknown`, `wyrd/0.0.0+git.unknown.dirty`, or
    no `Server` header, and no flag: INCONCLUSIVE (exit 3) naming `--server-version`, no
    mutating request sent. `--server-version banana`: usage status (2) naming the flag, no
    connection opened. `--server-version 9.0.0` (matches no profile): INCONCLUSIVE naming the
    version, no mutating request.
  - **(D) It touches only what it wrote in this run** (0017 §15):
    - **Occupied prefix** (`0017:791-794`): with one object planted under `<run-id>/`, exit 2
      naming the prefix and `--run-id`; no mutating request; the planted object is
      byte-identical and still the only key there.
    - **Acknowledged, not done** (sign-off red): the relay answers `POST ?delete` with a
      well-formed `<DeleteResult>` listing every requested key under `<Deleted>` and does not
      forward it. FAIL naming `oracle` for `DeleteObjects` and the keys still present; the
      report lists them as left in place and never says nothing was left; they are in fact
      still there.
    - **Bucket probes (decided; sign-off red):** the relay answers `CreateBucket` and
      `DeleteBucket` with 2xx, as a server implementing them would, and records the names. FAIL
      (`unexpected-support`); every recorded name breaks S3's bucket-naming rules, so no
      conforming server can hold a bucket by it; none equals `--bucket`; and the test's buckets
      (`--bucket` and a second, pre-existing one holding an object) still exist with their
      objects byte-identical. A run whose `--bucket` equals the name its bucket probes would use
      exits 2 before any request.
    - **Malformed ETag (sign-off red):** the relay rewrites the `ETag` on PUT and HEAD answers
      to `*` (one run) and to the unquoted `bare-etag` (another): FAIL naming `protocol` for
      `PutObject`; the relay saw no `If-None-Match` carrying `*` or the malformed value.
    - **Conditional negative control (decided):** the relay answers every `If-None-Match` GET
      with 304: FAIL naming `protocol` for the non-matching conditional GET.
    - On any FAIL, every key the run wrote and did not prove deleted is listed and left in place.
  - **(E) Scale and arguments** (`0017:528-531`, `:555-561`). Smoke repeats its sweep until
    `--duration` has elapsed (at least one full sweep; a sweep running at the deadline
    finishes). `--duration 2s`: PASS, the summary states sweeps and elapsed time, elapsed ≥ 2 s.
    `--workers` other than `1`: INCONCLUSIVE (exit 3) naming `--workers`, after the
    configuration echo and before any request (the relay sees none). `--duration 7x`: usage
    status naming the flag. The units `s`, `m`, `h`, `d` each parse to the right number of
    seconds, shown in the echo, checked with `--workers 7` so nothing is sent (`1d` is 86400,
    `2h` is 7200). `--scenario endurance`: usage status naming the flag and the scenarios that
    exist (`smoke`), no connection. `crates/validate/tests/cli_surface.rs` passes
    **unmodified**: its `--workers 7` (`cli_surface.rs:30`) now ends INCONCLUSIVE before any
    request, keeping those tests off the network.
- **Falsifiability:** RED is reachable on C4-verify itself, not only by code-reading. The test
  file uses only the binary, the existing `run`, test-local relays and the gateway, so it
  compiles on this slice's base (the integration line plus 743.1 and 743.2). With production
  reverted, the base binary exits 3 for every run, so (A)'s exit-0 and every FAIL-status
  assertion go red: a real red→green. Every fault is a relay on `127.0.0.1`; no Docker.
- **Invariant to restore:** the tool never removes, overwrites or changes a byte or a bucket it
  did not create in this run, and never reports a result its answers do not prove. Sources:
  0017 §15, "Ownership may under-approximate. It must never over-approximate." (`0017:831`),
  membership not name shape (`0017:811-816`), occupied prefix (`0017:791-794`); #754's invariant
  ("never removes a byte it did not write"); the vacuity rule (`0017:555-561`).
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** child-1, child-2
- **Conflicts with:**
- **Ordering note:** 743.1 (the matrix) and 743.2 (the verbs) are its inputs. #744 gives the
  payloads it sends and is the last slice to edit `args.rs` and `lib.rs`; #853 gives the
  response reading. Both are COMPLETE in this run's batch and on the integration line. Outside
  this batch it is held until PRs #931 and #934 merge into `main`, or until they are named in the
  same `pdca flow` command.
- **Scope:** the `smoke` scenario and its runner; mapping the client's results onto the
  matrix's observations; `--server-version` (optional, joining #744's optional flags) and
  reading the `Server` header; typing `--duration`, `--workers` and `--scenario` for smoke; the
  FAIL exit status; the first-writer ownership minimum (refuse an occupied run prefix; delete
  only what this run wrote and recorded, and forget a key only once its absence is proved;
  bucket probes that cannot touch an existing bucket or `--bucket`); `lib.rs` and `main.rs`
  wiring and the module doc's "#743" sentence (`lib.rs:4-11`); the `Cargo.toml` description;
  the docs; the test file. / out of scope:
  - `--allow-non-empty`, forced cleanup-off, orphan sets, delete-time verification beyond this
    run's own record (#754); `--dry-run` (#755); worker pools (#749);
  - authentication vectors and #491's known-open handling (#745), although 0017 turns them on
    for smoke;
  - the cross-sweep oracle, event stream and verdict artifact (#746, #747, #764); MinIO (#751);
    other scenarios;
  - **every size class** (`0017:535` asks for it): smoke sends #744's payloads at a few small
    sizes (at most one chunk), carried from the 2026-10-20 brief. #744's classes reach the
    single-PUT limit (about 158 MiB at default tuning) and large objects are #761's;
  - any gateway change; any `wyrd-*` crate in `[dependencies]`; `deny*.toml`; a
    `[dev-dependencies]` entry the test needs that 743.2 did not add (the red leg reverts
    `Cargo.toml`, so the test would not compile).
- **Difficulty:** high
- **Do model:** opus
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/validate/tests/smoke_loopback.rs` (NEW)
- **Verification posture:** default: a flippable red→green on C4-verify (see Falsifiability).
  Do confirms it before handing off: revert production, keep the test, show it fails by
  assertion (exit 3 against an expected 0 or FAIL status), not by a compile error.
  **Demonstrated reds (required, in `build-notes.md`):** (1) bulk-deleted keys dropped from the
  run's record on the document alone → (D) acknowledged-not-done; (2) bucket probes naming a
  valid derived name → (D) bucket probes; (3) a `*` ETag accepted and reused → (D) malformed
  ETag; (4) the 2xx head dropped on a cut body → (B)(ii); (5) the `--scenario` check removed →
  (E); (6) a day computed as `24 + 60 * 60` seconds → (E); (7) `+git.unknown` accepted as a
  version → (C); (8) the occupied-prefix check removed → (D); (9) `--duration` ignored (one
  sweep, then PASS) → (E).
- **Production reach:** the binary runs smoke for real; the test drives that binary. No Wyrd
  server code changes.
- **Citations expected:** Do must cite `path:line` on the target branch for every change.
  - **Peer, gateway:** `start_gateway_with_chunk`,
    `crates/validate/tests/s3_client_roundtrip.rs:323-350`; set `server_version` on its
    `S3Config` (`crates/gateway-s3/src/lib.rs:111-116`); seed bucket markers as `seed_bucket`
    does, `crates/server/tests/s3_list_objects.rs:82-88`.
  - **Peer, relay:** `Downstream` and `relay_connection`,
    `crates/validate/tests/s3_client_roundtrip.rs:465-477`, `:699`.
  - **The `Server` header:** value built at `crates/gateway-s3/src/lib.rs:126-145`, sent at
    `:1697-1699`; default `wyrd/unknown` at `:121`.
  - **Exit statuses** `crates/validate/src/lib.rs:43-54` (0–3 taken: FAIL needs its own);
    `run` `:132-160`; flags `crates/validate/src/args.rs:31-50`, `parse` `:162`.
  - **Docs currency** (`AGENTS.md:154-157`, a new CLI flag): add a **new paragraph** after the
    `validate` paragraph, `docs/design/architecture/05-building-block-view.md:297`, covering the
    matrix and its version keying, `smoke`, the ownership rules and the exit statuses. Do not
    rewrite line 297 itself: it is one 8.9 KB line, and rewriting it costs about 20 KB of diff.
- **Prior-art check (triage cycles):** searched by path `crates/validate/`,
  `docs/design/architecture/05-building-block-view.md` across merged, open and closed-unmerged
  PRs (2026-10-22): merged #845, #849, #859, #860, #865, #886; open #931, #934; closed-unmerged:
  none. No scenario code exists. The rejected first attempt's `smoke.rs` is the source of the
  three sign-off defects named in (D).
- **Disposition hint:** new-feature

## Design

- **Order of a run.** Parse and echo the configuration (the echo also shows the bucket-probe
  name and the parsed duration). Refuse what can be refused offline: `--workers` ≠ 1, an
  unknown `--scenario`, an unparseable `--duration` or `--server-version`, a `--bucket` equal to
  the probe name. Then one `ListObjectsV2` with `prefix=<run-id>/` and `max-keys=1`, which both
  checks the prefix is empty and reads the `Server` header. Only then send any write.
- **The run's record.** A key is recorded before the request that could create it, and is
  forgotten only once a `HEAD` answers 404. Cleanup deletes from that record only, never from a
  listing (`0017:811-816`). On FAIL, everything still recorded is listed and left in place.
- **Bucket probes.** `CreateBucket` and `DeleteBucket` name a bucket S3's naming rules forbid,
  never `--bucket`. Reason: no run can prove it owns a bucket from a 2xx (S3's us-east-1 answers
  200 to `CreateBucket` on a bucket the caller already owns), and the endpoint today offers no
  way to check that a name is free (`HeadBucket`, `ListBuckets` are unsupported). So the probe
  names a bucket that cannot exist. A server that implements the verb answers an error that
  differs from the declared row, and the run FAILs without anything being deleted.
- **`HeadBucket`, `GetBucketLocation`, `ListBuckets`** are read-only and may name `--bucket`.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR MAY
happen during the cycle. The PR MUST NOT be marked ready before sign-off accepts.
<!-- pdca:end child-3 -->
