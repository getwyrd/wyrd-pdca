# validate: the S3 client verbs smoke sends — HEAD, ranged and conditional GET, listings, bulk delete, unsupported-operation requests (743.2)

> Do reads ONLY this file. Second of three children of #743; it extends the client #853
> hardened and decides nothing about whether an answer is right (743.1 judges, 743.3 runs).
> Siblings: 743.1 = #937 (the matrix), 743.2 = #938 (the client verbs), 743.3 = #939 (smoke).
>
> **Re-planned 2026-10-23 to be split, not rebuilt.** Sign-off of iteration 3 (iterate-to-Plan):
> "the slice is too big, not wrong". The patch grew 131 → 192 → 208 KB over three rounds against a
> 100 KB limit, and no reviewer found a defect in it. This brief folds every carry-forward of
> those rounds into one criterion, so the split's children start from the whole, settled contract
> and do not grow it round by round. The split itself is in the ordering note.

- **Slug:** validate-s3-client-smoke-verbs
- **Track:** blackbox
- **Kind:** enhancement
- **Defect:** the client can only PUT, GET and DELETE one object
  (`crates/validate/src/s3.rs:229`, `:281`, `:325` on `main` 955b9cb). Smoke (743.3) also needs
  HEAD; a ranged GET; a GET with a given `If-None-Match`; ListObjectsV2 and v1 under a prefix;
  bulk DeleteObjects; and one request for each operation the matrix may declare unsupported.
  Each must report what the server actually sent (status, the headers checks read, the S3 error
  code, the body) with #853's bounded, typed reading. A 2xx head must never be lost: when a body
  fails after a 2xx head, the report still carries the 2xx, or a half-working unsupported
  operation reads as "no answer" instead of `unexpected-support`. And a probe must reach the
  endpoint it was given with the bucket name it was given: the SDK's own endpoint rules reroute
  or refuse some names (S3 Express `--x-s3` / `--xa-s3`, Outposts-alias `--op-s3`, `arn:`)
  before anything is sent, which is exactly what 743.3's bucket-safety rule cannot tolerate.
- **Success criterion:** the NEW test file(s) under `crates/validate/tests/` pass, against an
  in-process Wyrd gateway and, for the fault legs, a scripted loopback endpoint or relay (peers
  under *Citations*):
  - **(A) The verbs against the real gateway.** HEAD of a written key → 200 with its size and
    `ETag`; of a missing key → 404 with no body (`ErrorCode::NoBody`). Ranged GET `bytes=a-b` →
    206, the `Content-Range` header as sent, exactly those bytes. GET with `If-None-Match: <its
    ETag>` → 304, no body; with a different valid tag → 200 and the whole body. ListObjectsV2
    and v1 under a prefix → every key with its size, across more than one page (the client
    follows the continuation). DeleteObjects of N existing keys → every key under `<Deleted>`,
    no `<Error>` row; the keys are then gone (HEAD 404). Error rows are (F)'s, because the
    gateway cannot be made to fail one key on demand.
  - **(B) Every unsupported operation reaches the gateway** and is reported with status and S3
    code: `CopyObject` 501 `NotImplemented`; `CreateMultipartUpload`, `UploadPart`,
    `CompleteMultipartUpload`, `AbortMultipartUpload`, `ListParts`, `ListMultipartUploads` 501;
    `GetBucketLocation` 501; `CreateBucket`, `DeleteBucket`, `ListBuckets` 400 `InvalidRequest`;
    `HeadBucket` 400 with no body. **Bucket names reach the endpoint as given**, on an IP
    endpoint (`http://127.0.0.1:{port}`) AND a host-name endpoint (`http://localhost:{port}`):
    `CreateBucket` / `DeleteBucket` naming a bucket S3's rules forbid (one ending in `-`), a
    host-safe and a non-host-safe `--x-s3` name, an `--xa-s3` name, an Outposts-alias-shaped
    `--op-s3` name and an `arn:` name each arrive as ONE request, `PUT|DELETE /<name>/` with the
    name as one percent-encoded path segment after the endpoint's own path, `Host:` the
    configured endpoint, signed for `s3` in the configured region. `ListBuckets` addresses the
    endpoint itself. A name that cannot be sent at all is reported `RequestNotBuilt` (nothing
    sent), never `NoResponse`; a request that reached the connector and got no answer stays
    `NoResponse`. The remaining unsendable names are named in the code's docs and tested.
  - **(C) A 2xx head survives its body.** A scripted endpoint answers a request that carries a
    body with a `200` head and then (i) stalls the body past the body-idle deadline, (ii)
    trickles it past the whole-body deadline, (iii) cuts the connection mid-body, (iv) sends a
    body that is not the operation's success document. Short deadlines via
    `S3Client::with_deadlines`. In each case the report carries status 200 and the body failure
    beside it, never only a timeout or a transport error. A `200` whose body is an S3 `<Error>`
    document (S3 does this for `CopyObject` / `CompleteMultipartUpload`) keeps the 200 AND
    reports that document's `<Code>` and `<Message>` as typed fields.
  - **(D) Unreadable documents are refused for the new verbs as #853 refuses them for the old.**
    A listing page or `<DeleteResult>` that is cut short, followed by junk or a second document,
    followed by bytes past its `Content-Length`, or past `BUFFERED_BODY_BUDGET` (with a
    `Content-Length` and chunked) is a typed refusal that keeps the 200, never a partial listing
    or a short delete result. A later page that fails discards the earlier good pages (no
    partial listing). A document of exactly the budget is read. A `<Size>` that is not plain
    decimal digits (`+7`, a space, a fraction, past `u64`) is refused.
  - **(E)** `s3_client_roundtrip.rs`, `s3_client_upload_peers.rs` and
    `s3_client_nonconforming_responses.rs` pass unmodified (the existing verbs keep their
    behaviour).
  - **(F) A mixed `<DeleteResult>` is reported row by row.** The gateway answers `200` even
    when some keys failed (`crates/gateway-s3/src/lib.rs:2293-2296`), so error rows are the only
    evidence of a failed key. A scripted endpoint answers `POST ?delete` with `200` and two
    `<Deleted>` rows plus one `<Error>` row (`<Key>`, `<Code>AccessDenied</Code>`,
    `<Message>`). The result lists exactly the two deleted keys and exactly one error row with
    that key, code and message as sent, in document order.
  - **(G) The success and continuation paths are pinned, not only the failures.** Against the
    scripted endpoint: a `200` with each operation's whole, correct success document (or no
    body where its success has none) is a success; `ListObjects` (v1) follows a `<NextMarker>`
    that differs from the last key, and the last key where a truncated page names none; a
    truncated page that names nothing to continue from is refused; a listing still truncated
    after the page cap stops there with a typed result; a header sent twice comes back as both
    lines.
  - **(H) The living architecture doc matches the code.**
    `docs/design/architecture/05-building-block-view.md:299` on `main` (the `validate`
    paragraph; `:301` on the integration line, where #932 adds two lines at `:243`) describes the new verbs, the 2xx head kept beside a body failure, listing continuation and
    its page cap, bulk-delete rows, the buffered budget's reach (listings, delete results,
    probes, non-2xx ranged/conditional GETs), and the client's own endpoint resolution with its
    named exceptions. No sentence in it or in the code's docs claims more than the tests show.
- **Falsifiability:** RED is reachable on the plain `cargo test -p wyrd-validate --test <file>`
  target, which `cargo xtask ci` also runs: a real gateway on loopback for (A)/(B), a scripted
  endpoint or relay with short deadlines for (B)-routing/(C)/(D)/(F)/(G). A client that drops
  the 2xx head on a body failure, lets the SDK's endpoint rules reroute or refuse a bucket name,
  reports an unsent request as `NoResponse`, stops listing after one page, accepts a cut-off
  document, or drops a `<DeleteResult>`'s error rows fails an assertion. No Docker, no network
  beyond `127.0.0.1` / `localhost`. Every leg was shown red and green on iteration 3's code
  (`results/issue_938/iteration-v3/build-notes.md`; adversary
  `results/issue_938/iteration-v3/check-advisory-adversary.md`).
- **Invariant to restore:** what the client reports is what the server sent, from the request
  the caller asked for. No answer is upgraded, none downgraded, nothing filled in on the
  server's behalf (`crates/validate/src/s3/error.rs:1-7`); a failure after a 2xx head never
  erases the 2xx; and no request is rewritten, rerouted or silently dropped on the way out, so
  "the server refused it" and "it was never sent" are never confused (proposal 0017 §7, the
  failure taxonomy the typed errors serve).
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:**
- **Conflicts with:**
- **Ordering note:** **Base (2026-10-23):** #853 (PR #934) and #744 (PR #931) are merged; `main`
  955b9cb has `s3/response.rs`, `s3/wire.rs`, `s3_client_nonconforming_responses.rs`, `roxmltree`
  under `[dependencies]` (`crates/validate/Cargo.toml:44`, section at `:24`), and the advisory fixes #950
  (ctutils 0.4.5) and #954 (tokio 1.54.1). So no `Depends on` is needed. Every `crates/validate/`
  citation here was re-checked on 955b9cb; the lines are identical on the run's integration line.
  In the live run the children build on `pdca-integration/main-r42c5348f203d`; its next fold
  re-merges #853's branch at its current tip d9d50d1, which contains b552ca2, so the line takes
  the advisory fixes in before they build. Iteration 3's `patch.diff` applies cleanly on
  955b9cb (`git apply --check`, 2026-10-23).
  **Siblings:** runs in parallel with 743.1 (#937) and does not depend on it; must not touch
  `crates/validate/src/lib.rs` (743.1 adds a line there, 743.3 rewrites it); new types are
  reachable as `wyrd_validate::s3::…`. 743.3 (#939) consumes all of it.
  **The split (human, sign-off 2026-10-23: "reads and listing vs. deletes and bucket probes";
  prefer two children).** child-1: the reply-capture core (the 2xx head kept beside a bounded
  body read, the typed results) plus HEAD, ranged GET, conditional GET and both listings, with
  (A)'s reads and listings, (C) shown on a listing page, (D) for listings, (G)'s listing and
  header legs. child-2: bulk delete, every probe, and the endpoint resolution, with (A)'s delete,
  (B), (C) on probes including the `<Error>`-in-200 case, (D) for `<DeleteResult>`, (F), (G)'s
  success-document legs, and (H). child-2 depends on child-1 (it reports through child-1's
  core). Each takes its own files' hunks from the salvage below; neither re-derives the design.
  **Salvage — the starting point, not a hint.** `results/issue_938/iteration-v3/patch.diff`
  (production: `s3.rs` endpoint resolver and module wiring, `s3/answer.rs` capture and answer
  types, `s3/document.rs` listing and delete-result reading, `s3/verbs.rs` the verbs and
  `Probe`, `s3/response.rs` shared document checks; test file
  `crates/validate/tests/s3_client_smoke_verbs.rs`, 23 tests). Adversarial review could not
  refute it and the code review found nothing (iteration-v3 `SUMMARY.md` §5). Keep the design;
  move its hunks into the child that owns them.
- **Scope:** the new verbs and their typed results in `crates/validate/src/s3.rs` and
  `crates/validate/src/s3/*`; the client's endpoint resolution (so a bucket name reaches the
  configured endpoint as given); `wyrd-traits` (or what the bucket-seeding peer needs) under
  `[dev-dependencies]` in `crates/validate/Cargo.toml`; the new test file(s); the `validate`
  paragraph of `docs/design/architecture/05-building-block-view.md`. / out of scope:
  - judging answers (743.1); the runner, ledger, version source and arguments (743.3);
    `lib.rs`;
  - the multipart success path (#508, #750); changing the existing verbs' behaviour beyond
    what the shared endpoint resolution implies (an endpoint path joined with one `/`; the
    region signed as given), each of which must be named in the docs;
  - any `wyrd-*` crate in `[dependencies]`; any new production dependency; `deny*.toml`.
- **Difficulty:** medium
- **Do model:** opus
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/validate/tests/s3_client_smoke_verbs.rs` (NEW)
- **Verification posture:** (a) net-new. The verbs do not exist on the base, so with production
  reverted the test cannot compile and C4-verify reports UNVERIFIABLE (exit 77): pre-declared.
  Built and exercised at Check: every verb against the real gateway, every fault and routing
  path against the scripted endpoint or relay. **Demonstrated reds (required, in
  `build-notes.md`, per child for the legs it owns):** (1) return only the timeout when a body
  stalls after a 2xx head → (C); (2) accept a listing document cut short → (D); (3) put the
  SDK's default endpoint resolution back → (B) routing; (4) stop listing after the first page →
  (A); (5) drop the `<Error>` rows of a `<DeleteResult>` → (F); (6) report a request refused
  before sending as `NoResponse` → (B).
- **Production reach:** consumed by 743.3 (#939) in this same run; until then only the tests
  call the new verbs.
- **Citations expected:** Do must cite `path:line` on the target branch for every change.
  - **Peer, response reading:** the existing verbs' `.customize()` hooks
    (`crates/validate/src/s3.rs:244`, `:288`, `:330`) and `S3Client::classify` (`:339`); the
    strict document reader `error_document` (`crates/validate/src/s3/response.rs:363`) and the
    `ResponseGuard` interceptor (`:209`, `impl Intercept` `:231`). Read listings and delete
    results by the same rules (UTF-8, one well-formed document under the expected root, bounded
    depth and size).
  - **Peer, in-process gateway:** `start_gateway_with_chunk`,
    `crates/validate/tests/s3_client_roundtrip.rs:323`. A listing needs the bucket's marker
    seeded first (ADR-0046 decision 1): do it as `seed_bucket` does,
    `crates/server/tests/s3_list_objects.rs:82-88`, before the store goes into `Gateway::new`.
  - **Peer, faults:** the relay's `Downstream` modes (`HoldAfter`, `CutAfter`),
    `crates/validate/tests/s3_client_roundtrip.rs:465-474`; the scripted endpoint
    `crates/validate/tests/s3_client_nonconforming_responses.rs:354` (`serve` `:489`, `head()`
    `:247`, `error_doc()` `:220`).
  - **Deadlines:** `Deadlines` `crates/validate/src/s3.rs:81-96`, defaults `:98-107` (body idle
    60 s, whole body 15 min): use `S3Client::with_deadlines` for (C).
  - **Salvage:** `results/issue_938/iteration-v3/patch.diff` and its rationale
    `results/issue_938/iteration-v3/build-notes.md` (the resolver's SDK citations, the two
    names that still cannot be sent, the rejected alternatives).
- **Prior-art check (triage cycles):** searched by path `crates/validate/src/s3.rs`,
  `crates/validate/src/s3/` (2026-10-23): merged #859 (the client), #865 (early answers), #934
  (#853, response hardening); open: #942 shows these paths only because it was stacked on the
  integration line (its own commit 6a85eaa touches none of them); closed-unmerged: none. Rejected
  attempts are this bundle's own: iteration-v1 (findings: docs currency, success-path legs,
  bucket-name claim, 2xx `<Error>` document), iteration-v2 (finding: the SDK rules rerouted
  `--x-s3` / Outposts names; fixed by the resolver), iteration-v3 (no defect; split for size).
  All are in (B), (C), (G), (H) above. The bucket-safety rule (B) serves is #743's: a
  `DeleteBucket` probe could delete a bucket it did not own (`results/issue_743/review-batch.md:3`,
  `:5`, `:7`); probes naming a bucket S3's rules forbid were decided in the 2026-10-22 Plan
  session (`results/issue_743/split-proposal.md:495`, `:387`) and are 743.3's to choose.
- **Disposition hint:** new-feature

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR MAY
happen during the cycle. The PR MUST NOT be marked ready before sign-off accepts.
