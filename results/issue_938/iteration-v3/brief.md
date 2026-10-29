# validate: the S3 client verbs smoke sends — HEAD, ranged and conditional GET, listings, bulk delete, unsupported-operation requests (743.2)

> Do reads ONLY this file. Second of three children of #743. It extends the client #853
> hardened; it decides nothing about whether an answer is right (743.1 judges, 743.3 runs).
> Siblings: 743.1 = #937 (the matrix), 743.2 = #938 (the client verbs), 743.3 = #939 (smoke).

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
    page (the client follows the continuation). DeleteObjects of N existing keys → every key
    reported under `<Deleted>`, no `<Error>` row; the keys are then gone (HEAD 404). This is
    the success case only; error rows are (F)'s, because the gateway cannot be made to fail one
    key on demand.
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
    `s3_client_nonconforming_responses.rs` pass unmodified (the third is #853's; it exists on
    the base this slice must build on, see the ordering note).
  - **(F) A mixed `<DeleteResult>` is reported row by row** (added 2026-10-22). The gateway
    answers `200` even when some keys failed (`crates/gateway-s3/src/lib.rs:2293-2300`), so
    the error rows are the only evidence of a failed key. A scripted endpoint answers
    `POST ?delete` with `200` and a `<DeleteResult>` holding two `<Deleted>` rows and one
    `<Error>` row (`<Key>`, `<Code>AccessDenied</Code>`, `<Message>`). The result lists
    exactly the two deleted keys and exactly one error row carrying that key, that code and
    that message as sent, in document order. A client that drops or empties the error list
    fails this leg.
- **Falsifiability:** RED is reachable on the plain `cargo test -p wyrd-validate --test
  s3_client_smoke_verbs` target: a real gateway on loopback for (A)/(B), a scripted endpoint
  with short deadlines for (C)/(D). A client that drops the 2xx head on a body failure, lets the
  SDK refuse a forbidden bucket name, stops listing after one page, accepts a cut-off
  document, or drops a `<DeleteResult>`'s error rows fails an assertion. No Docker, no network beyond `127.0.0.1`.
- **Invariant to restore:** what the client reports is what the server sent. No answer is
  upgraded, none downgraded, nothing filled in on the server's behalf
  (`crates/validate/src/s3/error.rs:1-7`); a failure after a 2xx head never erases the 2xx.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** 853
- **Conflicts with:**
- **Ordering note:** builds on #853's reading of responses (`s3/response.rs`, `s3/wire.rs`, the
  typed errors). Runs in parallel with 743.1: it must not touch `crates/validate/src/lib.rs`
  (743.1 adds a line there, 743.3 rewrites it); new types are reachable as `wyrd_validate::s3::…`.
  **Base (corrected 2026-10-22): #853's code is NOT on `main`.** #853 is COMPLETE, but its PR
  #934 is open, so `main` (`4bac538`) has no `s3/response.rs`, no `s3/wire.rs`, no
  `s3_client_nonconforming_responses.rs`, and `roxmltree` is still only a dev-dependency there
  (`crates/validate/Cargo.toml:42-43`). Its code is on the integration line of the run that
  split #743, `pdca-integration/main-r42c5348f203d` (tip `582da99`: #744 folded as `61e5341`,
  #853 as `582da99`, on `main` `4bac538`; `results/issue_853/stack-base`). Every `crates/validate/`
  citation in this brief is on that line (re-checked 2026-10-22 at `582da99`), and there
  `roxmltree` is a production dependency (`crates/validate/Cargo.toml:44`, under
  `[dependencies]` at `:24`). The driver never builds this bundle on a base without #853: it
  builds it on that line when the run that split #743 adopts it, or on a later run's line when
  853 is named in the same `pdca flow` command, and otherwise holds it until PR #934 merges
  (`src/pdca_harness/flow.py:755-760`). **Do MUST confirm its base has #853's code** (the three
  files above exist) and record the base commit in `build-notes.md`. On a base without it,
  stop and report: do not recreate #853's work here.
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
  → (B); (4) stop listing after the first page → (A); (5) drop the `<Error>` rows of a
  `<DeleteResult>` → (F).
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
  body timeout, now (C). **Where that is recorded** (these are harness records, not tracker
  comments; #743's `notes.json` holds one comment and covers neither): the finding is
  `results/issue_743/iteration-v1/check-advisory-code-review.md:7` (repeated in that
  iteration's `SUMMARY.md:203`, `:221`); the later batched review,
  `results/issue_743/review-batch.md`, names only `smoke.rs` and `matrix.rs`. The bucket-safety
  rule (B) serves comes from the first attempt's sign-off findings that a `DeleteBucket` probe
  could delete a bucket it did not own (`results/issue_743/review-batch.md:3`, `:5`, `:7`).
  The human decided the fix, probes that name a bucket S3's naming rules forbid, in the
  2026-10-22 Plan session (`results/issue_743/split-proposal.md:5-6`, `:385`, `:495`). It is
  written into 743.3's brief (`results/issue_939/brief.md`, (D) *Bucket probes* and *Design*).
- **Disposition hint:** new-feature

Plan-review response (2026-10-22): all three findings are addressed in place. (1) New leg (F):
a scripted mixed `<DeleteResult>` (two `<Deleted>`, one `<Error>`) must come back row by row
with exact key, code and message, with a demonstrated red for dropped error rows; (A)'s "keys
are then gone" is now the real-gateway success case only. (2) The base: the reviewer is right
that `main` lacks #853. Its code (and `roxmltree` as a production dependency) is on
`pdca-integration/main-r42c5348f203d` at `582da99`, and the driver never builds this bundle on
a base without it. The ordering note now says so, and Do must confirm the base or stop. (3) The
prior-art entry now cites the harness records behind "one finding" and the bucket-safety
decision.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR MAY
happen during the cycle. The PR MUST NOT be marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Core behaviour held under adversarial review (2xx head survives stalled/cut/unreadable bodies, strict listing/DeleteResult refusals, mixed DeleteResult rows); keep that design. Close these gaps: - Docs currency (gating T4): update docs/design/architecture/05-building-block-view.md:299 for the new verbs, retained 2xx heads beside body failures, listing continuation and the 100-page cap, bulk-delete rows, and the widened 64 KiB buffered budget (listings, delete results, probes, non-2xx ranged/conditional GETs). This doc edit is in scope for this slice. - Add scripted-endpoint legs: 200 + whole correct success document for each Document root (verbs.rs:104-114) -> Outcome::Success; <Size>+7</Size> refused (document.rs:251-256); a NextMarker that differs from the last key is followed (document.rs:180-183); ListingError::TooManyPages reached; Head::header_lines / Head::fields return repeated headers (answer.rs:99-111). - Bucket names: the "sent exactly as given" claim (verbs.rs:35-38, test name at s3_client_smoke_verbs.rs:753) overstates it — the SDK reroutes `--x-s3` names to a CreateSession request and refuses ARN names before sending. Either disable S3 Express session auth on this client or narrow the claim and name those cases; in both cases report a name refused before sending as RequestNotBuilt, not NoResponse. - A 2xx carrying an <Error> document (S3 does this for CopyObject / CompleteMultipartUpload): keep the 2xx status, and also expose the S3 code and message as typed fields instead of only Outcome::Unreadable (answer.rs:336-337), matching the brief's "report the S3 error code".
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Core behaviour held under adversarial review (2xx head survives stalled/cut/unreadable bodies, strict listing/DeleteResult refusals, mixed DeleteResult rows); keep that design. Close these gaps:
  - Docs currency (gating T4): update docs/design/architecture/05-building-block-view.md:299 for the new verbs, retained 2xx heads beside body failures, listing continuation and the 100-page cap, bulk-delete rows, and the widened 64 KiB buffered budget (listings, delete results, probes, non-2xx ranged/conditional GETs). This doc edit is in scope for this slice.
  - Add scripted-endpoint legs: 200 + whole correct success document for each Document root (verbs.rs:104-114) -> Outcome::Success; <Size>+7</Size> refused (document.rs:251-256); a NextMarker that differs from the last key is followed (document.rs:180-183); ListingError::TooManyPages reached; Head::header_lines / Head::fields return repeated headers (answer.rs:99-111).
  - Bucket names: the "sent exactly as given" claim (verbs.rs:35-38, test name at s3_client_smoke_verbs.rs:753) overstates it — the SDK reroutes `--x-s3` names to a CreateSession request and refuses ARN names before sending. Either disable S3 Express session auth on this client or narrow the claim and name those cases; in both cases report a name refused before sending as RequestNotBuilt, not NoResponse.
  - A 2xx carrying an <Error> document (S3 does this for CopyObject / CompleteMultipartUpload): keep the 2xx status, and also expose the S3 code and message as typed fields instead of only Outcome::Unreadable (answer.rs:336-337), matching the brief's "report the S3 error code".
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 111 mutants tested in 19m: 17 missed, 54 caught, 40 unviable
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_938/review-b
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Keep the attempt-2 design unchanged: retained 2xx heads, strict listing/DeleteResult reading, mixed DeleteResult rows, and the tests that already pass all held under adversarial review. Fix only the bucket-name routing bug and the claims that overstate it: - Bug: WithoutS3Express (crates/validate/src/s3.rs:453-494) keeps the SDK endpoint rules' URL. On a hostname endpoint (http://localhost:PORT), `--x-s3`/`--xa-s3` names go virtual-host style (`PUT /` with host `<bucket>.localhost`), and non-host-safe ones (e.g. `Wyrd_Validate--x-s3`) are refused as RequestNotBuilt. Outposts-alias names (`...--op-s3`) resolve to an amazonaws.com https URL even on an IP endpoint and are reported as NoResponse, though nothing was sent. Fix: the client always has `--endpoint` and always forces path-style, so use a resolver that always builds `{endpoint}{path}/{uri-encoded bucket}` (no bucket segment for ListBuckets). Any name refused before sending must report RequestNotBuilt, never NoResponse. - Tests: add legs on `http://localhost:{port}` with one host-safe and one non-host-safe `--x-s3` name, plus an Outposts-alias-shaped name. Each must reach the endpoint as `PUT /<name>/`. - Code comments and docs: make crates/validate/src/s3/verbs.rs:35-43, crates/validate/src/s3.rs:4-6, crates/validate/src/s3/answer.rs:426 ("handed to the connector" = sent) and docs/design/architecture/05-building-block-view.md:299 match the fixed behaviour exactly. Name any remaining exception (e.g. `arn:`) explicitly. Do not split: the size backstop was weighed and declined. This is already a split child of #743, and the findings are narrow.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Keep the attempt-2 design unchanged: retained 2xx heads, strict listing/DeleteResult reading, mixed DeleteResult rows, and the tests that already pass all held under adversarial review. Fix only the bucket-name routing bug and the claims that overstate it:
  - Bug: WithoutS3Express (crates/validate/src/s3.rs:453-494) keeps the SDK endpoint rules' URL. On a hostname endpoint (http://localhost:PORT), `--x-s3`/`--xa-s3` names go virtual-host style (`PUT /` with host `<bucket>.localhost`), and non-host-safe ones (e.g. `Wyrd_Validate--x-s3`) are refused as RequestNotBuilt. Outposts-alias names (`...--op-s3`) resolve to an amazonaws.com https URL even on an IP endpoint and are reported as NoResponse, though nothing was sent. Fix: the client always has `--endpoint` and always forces path-style, so use a resolver that always builds `{endpoint}{path}/{uri-encoded bucket}` (no bucket segment for ListBuckets). Any name refused before sending must report RequestNotBuilt, never NoResponse.
  - Tests: add legs on `http://localhost:{port}` with one host-safe and one non-host-safe `--x-s3` name, plus an Outposts-alias-shaped name. Each must reach the endpoint as `PUT /<name>/`.
  - Code comments and docs: make crates/validate/src/s3/verbs.rs:35-43, crates/validate/src/s3.rs:4-6, crates/validate/src/s3/answer.rs:426 ("handed to the connector" = sent) and docs/design/architecture/05-building-block-view.md:299 match the fixed behaviour exactly. Name any remaining exception (e.g. `arn:`) explicitly.
  Do not split: the size backstop was weighed and declined. This is already a split child of #743, and the findings are narrow.
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 124 mutants tested in 21m: 2 missed, 80 caught, 42 unviable
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: The slice is too big, not wrong: the patch grew 131 KB -> 192 KB -> 208 KB over three rounds against a 100 KB limit (about 1,650 production lines across answer.rs, verbs.rs, document.rs, s3.rs plus about 2,500 test lines). The reviewer found no defect in the patch, and the code is worth keeping as the starting point for the children. Split 743.2 in Plan (pdca split) into smaller verb slices, for example reads and listing vs. deletes and bucket probes; the exact split is decided in Plan. Each child keeps the brief's existing constraints: it runs in parallel with #937 (743.1) and does not depend on it, it does not touch crates/validate/src/lib.rs, and #939 (743.3) consumes all of them. The C4-ci red was only the base advisory wall, now cleared on main by #950 (ctutils) and #954 (tokio); ../wyrd is at b552ca2 and both advisory checks pass. Base the children on that.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  The slice is too big, not wrong: the patch grew 131 KB -> 192 KB -> 208 KB over three rounds against a 100 KB limit (about 1,650 production lines across answer.rs, verbs.rs, document.rs, s3.rs plus about 2,500 test lines). The reviewer found no defect in the patch, and the code is worth keeping as the starting point for the children. Split 743.2 in Plan (pdca split) into smaller verb slices, for example reads and listing vs. deletes and bucket probes; the exact split is decided in Plan. Each child keeps the brief's existing constraints: it runs in parallel with #937 (743.1) and does not depend on it, it does not touch crates/validate/src/lib.rs, and #939 (743.3) consumes all of them.
  The C4-ci red was only the base advisory wall, now cleared on main by #950 (ctutils) and #954 (tokio); ../wyrd is at b552ca2 and both advisory checks pass. Base the children on that.
- Failing gate: C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) —   - cargo deny --all-features --config deny-all-features.toml check advisories: exited with exit status: 1
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 130 mutants tested in 23m: 2 missed, 83 caught, 42 unviable, 3 timeouts
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
