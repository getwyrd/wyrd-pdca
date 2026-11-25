# Result — issue 937 / validate-capability-matrix

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: `wyrd-validate` has no capability matrix. Nothing declares, per S3 operation,
  whether a Wyrd release must support it or must refuse it with an exact status and code;
  nothing picks those expectations by the server's version; and nothing turns an observed
  answer into a pass or one of 0017 §7's failure classes. Smoke (743.3) and every later
  scenario need all three, and none of it needs the network.
- Success criterion: the matrix module exists, is pure (no I/O, network, clock, SDK or
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
- Repo + branch target: getwyrd/wyrd @ main
- Scope: the `matrix` module: operations, rows in three states, immutable profiles with
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

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — N/A — close disposition (no patch to verify)
- C3 Change: none — patch.diff
- C4 Verification (red→green): none — N/A — close disposition (no patch to verify)
- C5 Causal adequacy: none — reviewer + human sign-off

## 4. Conformance (Check — stack)
- T1 Structure: none — N/A — close disposition (no patch to verify)
- T2 Shape: none — N/A — close disposition (no patch to verify)
- T3 Runtime: none — N/A — close disposition (no patch to verify)
- T4 Contribution: none — N/A — close disposition (no patch to verify)
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

# Advisory review — SKIPPED (close disposition)

The reviewer leaf was skipped: this bundle's Plan concluded a close / no-fix disposition (split), so there is no patch to review.

- NEEDS-HUMAN — Confirm the close disposition 'split' (no patch was built). Override to a fix path (iterate-to-Do) if the close is wrong.


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] Confirm the close disposition 'split' (no patch was built). Override to a fix path (iterate-to-Do) if the close is wrong.
- [x] **The delete rules can report an oracle failure for a correct server.** `brief.md:65-66` makes a follow-up HEAD returning 2xx fatal after an acknowledged delete; `brief.md:85` makes a probe of an unrequested key unsettled, but the precedence at `brief.md:50-56` lets the fatal rule win. Counterexample: delete `a` → 204, then mistakenly probe still-existing `b` → 200. The target deletes only the named key (`$PDCA_TARGET/crates/gateway-s3/src/lib.rs:2026-2028`) and correctly returns 200 for an existing key (`:2583-2584`, `:2612-2617`). Nothing here proves a server/model disagreement. Restrict the deleted-key oracle and expected-404 checks to probes of requested keys; specify paired fixtures for unrelated-key 200 → unsettled and deleted-key 200 → oracle, while retaining checks independent of key identity.
- [x] **The profile amendment silently postpones retention past tagged prereleases.** The tracker says “Retention starts at the first tag” (`notes.json.body`, §3 amendment). Instead, `brief.md:260-261` allows in-place edits until the first `0.1.0` release, while `brief.md:30-31,103` explicitly puts `0.1.0-rc.1` in that mutable profile. The target preserves prerelease tag identities (`$PDCA_TARGET/crates/server/src/version/derivation.rs:48-62`), so those releases can be distinguished. If multipart lands after rc.1, changing its rows to “supported, not checked” also changes rc.1's expectations; its correct 501 then becomes fatal under `brief.md:91-92`. This recreates the older-release false failure warned about in `$PDCA_TARGET/docs/design/proposals/draft/0017-blackbox-validation-tool.md:263-272`. Restore retention at the first tag, including prereleases, or explicitly resolve this policy change and add a retained-prerelease selection fixture.
- [x] **The supplied target violates the brief's base requirement.** `brief.md:161,168-172` requires `getwyrd/wyrd @ main` at or after `9b1a3e2`, explicitly identifying `93a6f37` as the obsolete integration base and forbidding lockfile/advisory-config repairs. Yet `git -C "$PDCA_TARGET" rev-parse HEAD` returns `93a6f3715e8785a22fd1d1ab40206bcf51b3eef4`; ancestry checks for `9b1a3e2` and both named advisory fixes (`6a0b369`, `6c79af3`) each return 1. The repository origin is correct and `origin/main` resolves to `9b1a3e2`, but that is not the supplied checkout. Revise the handoff to include the concrete target re-resolution/rebase step; recording the old base in build notes cannot satisfy the stated requirement.
- [x] **The delete acceptance rule turns missing evidence into an oracle failure.** `brief.md:47-51` requires a successful delete followed by HEAD 404, “else `oracle`”; `brief.md:74-77` only explicitly makes rows exercised “only with no answer” INCONCLUSIVE. For DELETE 204 followed by a HEAD timeout, an answer did arrive, but there is no evidence that the deleted key remains readable. The target distinguishes no-response and timeout outcomes (`crates/validate/src/s3/error.rs:36-47`), and proposal 0017 distinguishes transport availability from oracle disagreement and requires unresolved probes to remain INCONCLUSIVE (`docs/design/proposals/draft/0017-blackbox-validation-tool.md:489-493`, `:520-524`). Revise B′/E to define incomplete multi-observation checks separately from contradictory answers. Require deterministic fixtures for successful single/bulk delete followed by an unavailable HEAD → INCONCLUSIVE, alongside HEAD 200 → fatal oracle.
- [x] **The PUT criterion omits the existing early-acknowledgement invariant.** `brief.md:38-39` checks 200 plus a strong ETag; the observation contract at `brief.md:150-154` describes response headers and response-body outcomes, without requiring preservation of request-source completion. A premature 200 with a valid ETag and a complete empty response body meets those stated checks even while the upload is unfinished. The target explicitly says a PUT is a receipt only when its source had ended before the response head (`crates/validate/src/s3.rs:161-170`) and rejects premature success at `:235-243`; its regression test asserts this at `crates/validate/tests/s3_client_upload_peers.rs:1207-1217`. Preserve that distinction in the matrix-owned observation and add a pure early-ack fixture with a valid ETag that cannot pass. Deferring client-to-observation mapping to #939 does not specify how this fact survives the boundary.
- [x] **(Low; the outcome is FAIL either way.) A 404 to the HEAD or GET of a key the run just wrote is classed `protocol`, not `oracle`.** Probe on production: `HeadObject` answered `404` gives `Fatal { Protocol, "answered 404; expected 200" }`, and the same holds for `GetObject`. The cause is `expect_status` (`crates/validate/src/matrix.rs:972-981`) running before the oracle checks (`:1124`, `:1081`). Brief (B′) says HeadObject is "200 with the object's size and the PUT's `ETag`, else `oracle`". §7 (`0017:505`) defines `oracle` as "model and server disagree", and a written key that reads as absent fits that. But §7's `protocol` row ("wrong status") also fits. Only the class label in the report changes, and later scenarios that key on the class (#746) will inherit whichever reading is chosen. A human should pick one.

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
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
