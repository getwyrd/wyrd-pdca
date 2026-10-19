# Brief — issue 620 / s3-object-key-length-limit

> The Plan artifact (docs 02 §PLAN). Do reads ONLY this file. The `- **Label:** value`
> lines are parsed by the driver — keep their shape.
>
> **Re-plan (iteration 5), 2026-10-15.** The iteration-4 sign-off sent this back to Plan: the
> PUT side was right, the bulk-delete design was not. This brief keeps the PUT side and replaces
> criterion (3). Citations re-verified against `getwyrd/wyrd` `origin/main` @ `7bf7eb9`
> (unchanged since iteration 1) on 2026-10-15. Plan-review revision (same day): criterion (3)
> and the Invariant narrowed to the existing request limits; see the Plan-review response at the end.

- **Slug:** s3-object-key-length-limit
- **Track:** alpha
- **Defect:** The S3 gateway enforces S3's 1,024-byte object-key limit nowhere. A `PUT
  /{bucket}/{key}` stores a key of any length: the decoded key is composed at
  `crates/gateway-s3/src/lib.rs:1765-1769` and streamed straight into the backend write. The
  bulk-delete body cap's doc comment (`MAX_DELETE_BODY_BYTES`, `:489-507`) sizes its floor on
  "1000 keys × a 1024-byte key" while `parse_delete_request` (`:2171-2245`) accepts any non-empty
  `<Key>` (`:2208-2211`) up to the whole 8 MiB cap, and nothing says which of the two is meant
  (the #612 review follow-up in this issue's thread). The limit must not land as a refusal in
  bulk delete: over-limit objects can already exist (stored before this fix, or written by the
  data-plane `wyrd put --key`, `crates/server/src/cli.rs:567`), and some of them can only be
  removed by bulk delete (see criterion 3).
- **Success criterion:** A NEW wire-level test over the in-process gateway (signed raw HTTP and/or
  the stock SDK, as the peer suites do) shows:
  (1) **Creation refuses an over-limit key.** A PUT whose decoded key is 1,025 UTF-8 bytes answers
  `400` with S3 error code `KeyTooLongError`, before the body is stored: afterwards HEAD of that
  key is `404` and a listing of the bucket does not show it.
  (2) **The limit counts UTF-8 bytes of the decoded key, not characters or wire bytes.** A key of
  512 × `é` (1,024 bytes, 512 characters) is accepted and round-trips (HEAD `200`). A key of
  342 × `€` (1,026 bytes, 342 characters) is refused `KeyTooLongError`. A 1,024-byte key sent
  percent-encoded on the wire (so its request path is far longer than 1,024) is accepted. The
  limit applies to the key alone, not to `"{bucket}/{key}"`.
  (3) **Bulk delete adds no key-length refusal: a key the existing request limits can carry is
  deleted, however long.** (Those limits are unchanged: the 8 MiB body cap and the request-target
  ceiling below. This leg does not claim to reach a key too large for the body cap; see the
  Invariant.) Seed `b/<'a' × 1025>` and
  `b/<'c' × 70000>` through `Gateway::put_object` on the `Arc<Gateway>` clone (as in (4)), and PUT
  `b/ok` over the wire. One signed `DeleteObjects` naming all three keys answers `200` with a
  `<Deleted>` row for each key and **no** `<Error>` row. Afterwards all three are gone: HEAD of
  `b/ok` and of `b/<'a' × 1025>` is `404`, and the 70,000-byte key is absent when read through the
  `Arc<Gateway>` clone (`Gateway::get_object`, `crates/server/src/lib.rs:298`, returns `None`, as `crates/server/tests/e2e.rs:40` reads it).
  Its HEAD cannot show this, because hyper answers `414` for a request target over 65,534 bytes
  before the gateway runs (`http` 1.4.2, `Uri` `MAX_LEN = u16::MAX - 1`). That `414` is why bulk
  delete is the only S3 way to remove such a key, and why it must not refuse it.
  (4) **Single-object DELETE still removes an over-limit object.** Seed `b/<'a' × 1025>` through
  the existing backend API, `Gateway::put_object` (`crates/server/src/lib.rs:235`), on an
  `Arc<Gateway>` clone the test keeps before handing the gateway to `S3Gateway::new`
  (`crates/gateway-s3/src/lib.rs:232` takes `Arc<G>`). Then: signed HEAD of that key is `200`;
  signed `DELETE` is `204`; HEAD again is `404`. The HEAD-before and HEAD-after legs are required,
  because DELETE answers `204` for an absent key too (`crates/gateway-s3/src/lib.rs:1887-1890`),
  so `204` alone proves nothing.
  (5) **No bucket-separator alias around the limit.** The gateway splits the path on the first
  literal `/` before decoding (`crates/gateway-s3/src/lib.rs:567-573`) and joins the decoded
  bucket and key into one flat key (`:1765-1769`). So `PUT /b%2Fa/<'x' × 1023>` has a 1,023-byte
  request key yet stores the same object as `/b/a/<'x' × 1023>`, whose key under bucket `b` is
  1,025 bytes. A PUT whose decoded bucket segment contains `/` must be refused `400`
  `InvalidBucketName` (S3 bucket names cannot contain `/`) before the body is stored: afterwards
  HEAD of `/b/a/<'x' × 1023>` is `404`. Send this one as signed raw HTTP (the SDK will not put
  `%2F` in a bucket name).
- **Falsifiability:** RED on `origin/main`, in-process, no special environment: (1) answers
  `200` and stores the object, (2)'s `€` key answers `200`, and (5) answers `200` and stores
  `b/a/<'x' × 1023>`, so each fails on an assertion. (3) and (4) are green before and after: they
  pin that no delete path refuses a stored key for its length. They do not (and need not) test a
  key above the 8 MiB body cap, which this fix leaves as it is. (3) is the leg that goes red
  against the rejected iteration-4 design (a whole-request `400 KeyTooLongError` for an
  over-limit `<Key>`), so it catches a regression to it. The test uses only pre-existing API (the wire, plus
  `Gateway::put_object` / `get_object` for the fixtures, which `crates/server/tests/e2e.rs:38-40`
  already calls), so C4-verify's reverted base compiles it. `run-verify.sh --classify` on a patch
  of this shape returns `ADDED_TEST crates/server/tests/s3_key_length_limit.rs`, and one red leg
  is enough for its RED verdict (iteration 3: "PASS — red without the fix, green with it").
- **Invariant to restore:** No object whose S3 key exceeds 1,024 UTF-8 bytes can be **created**
  through the S3 surface, under any spelling of the request path (including a `%2F` that moves
  part of the key into the bucket segment), and **no delete path gains a length refusal**: every
  stored key stays exactly as deletable through S3 as it is today, bounded only by the existing,
  unchanged request limits (the 8 MiB bulk-delete body cap, `crates/gateway-s3/src/lib.rs:507`,
  refused as `MalformedXML` at `:2007-2009`; the 65,534-byte request target for single-object
  DELETE). So the refusal sits only on the creating path. In practice that covers every key a
  shipped write surface can produce: S3 PUT is held under the request-target ceiling, and
  `wyrd put --key` takes the key from argv (`crates/server/src/cli.rs:567`), which Linux caps at
  128 KiB per argument (`MAX_ARG_STRLEN`, with 4 KiB pages). **Known residual, not closed here:**
  a key longer than about 8 MiB (8,388,563 plain ASCII bytes fill a one-key body; fewer if the key
  needs XML escaping) can be written only through the in-process Rust API (`Gateway::put_object`,
  `crates/server/src/lib.rs:235`, which has no length check), and it has no S3 exit before or
  after this fix: bulk delete refuses its body, single DELETE answers `414`, and the CLI has no
  `delete`. #893's native key limit stops new ones; recovering an existing one is not tracked by
  any issue (see the Plan-review response). **Sources:** AWS S3 User Guide, "Naming Amazon S3
  objects": the key name is a sequence of Unicode characters whose UTF-8 encoding is at most
  1,024 bytes long (authoritative for S3 behaviour); `KeyTooLongError` is S3's error code for it
  (S3 API Reference, Error responses). AWS S3 API Reference, DeleteObjects: "For each key, Amazon
  S3 performs a delete operation and returns the result of that delete, success or failure"; it
  documents no key-length refusal for bulk delete. MinIO (corroborating, not canon): the 1,024-byte
  check runs on PUT and object args (`checkPutObjectArgs` / `checkObjectArgs`), while bulk delete's
  `checkDelObjArgs` has no length check. **C-1** (`docs/principles.md` §5): a state nothing can
  exit is never an acceptable cost (internal, Tier C): this fix must create no new such state; the
  pre-existing one above is named, not widened. SELF-TEST: a length bound in the bulk-delete
  parser, whole-request or per-key, breaks the second half for keys between the `414` ceiling and
  the body cap; that is exactly the iteration-4 sign-off finding.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:**
- **Conflicts with:** 671
- **Ordering note:** `Conflicts with 671`: both add a header-time refusal to the same S3 PUT arm
  (`crates/gateway-s3/src/lib.rs:1765-1830`, before the body streams). #620 refuses the key and
  #671 refuses the size, so they must be built in different waves. #767 and #666 also edit
  `crates/gateway-s3/src/lib.rs`, in regions that do not overlap (the timestamp formatter; the
  generic bounds and test doubles). This re-plan no longer touches the bulk-delete parser, so it
  adds no conflict. **Intake cap (wyrd-pdca-P1):** this is a repair of an ITERATE_PLAN brief
  already in the batch, which the cap allows. Count on 2026-10-15: alpha 6/6 (planned 508 625 633
  671 768, built 654), room 0. The original over-cap admission is the human's 2026-10-14 override
  for the seven-id batch (656, 666, 671, 767, 768, 620, 686). **Held before Do** by the
  `cargo deny all-features advisories` row until the base's advisory wall is clean (see External
  dependencies).
- **Surfaces:** data
- **Difficulty:** low
- **Scope:** S3 key-length enforcement where an object is created (PUT); the refusal of a PUT
  whose decoded bucket segment contains `/` (criterion 5); the `MAX_DELETE_BODY_BYTES` doc
  comment (`crates/gateway-s3/src/lib.rs:489-507`) made true about what bounds one bulk-delete
  entry (the body cap alone, deliberately: no length refusal is added, so any key the cap can
  carry stays deletable; the comment must not claim more than that); and the
  architecture doc's account of the limit (`docs/design/architecture/05-building-block-view.md`,
  next to the L1 access-layer table at `:126-135`): where it applies, that bulk delete has no
  per-key bound and why, and the residual limit (a key too large for the body cap, writable only
  through the in-process API, has no S3 exit) / out of scope: recovering a key too large for the
  8 MiB body cap (a pre-existing gap this fix does not widen; see Invariant); any key-length bound in bulk delete, whole-request or
  per-key `<Error>` row (rejected at the iteration-4 sign-off); CopyObject, which answers `501`
  today (`crates/gateway-s3/src/lib.rs:1800-1806`; when #504 implements it, the same rule applies
  to its destination key); single-object GET, HEAD and DELETE (unchanged, see criterion 4);
  listing parameters (`prefix`, `start-after`, `continuation-token`); `wyrd put --key` and any
  native key limit below the S3 gateway (filed as #893); bucket naming rules except the one
  criterion (5) needs (no `/` in a decoded bucket, on PUT only; GET, HEAD and DELETE on such a
  path stay as they are, since they create nothing); the multipart verbs (#508's slices apply the
  same rule to CreateMultipartUpload's key when they land); the 8 MiB cap value itself; the `414`
  hyper returns for request targets over 65,534 bytes.
- **Repro instruction:** On `origin/main`, with the in-process gateway (`start_gateway` as in
  `crates/server/tests/s3_list_objects.rs:92-117`, a bucket marker seeded): `PUT /b/<'a' × 1025>`
  answers `200` and HEAD finds it. `PUT /b%2Fa/<'x' × 1023>` answers `200` and HEAD
  `/b/a/<'x' × 1023>` finds it.
- **External dependencies:** `cargo deny all-features advisories` `typos` `docs-renderer`
- **Test file:** `crates/server/tests/s3_key_length_limit.rs` — a **NEW** file (C4-verify treats
  only an added `crates/*/tests/*.rs` as the per-fix discriminator; an appended or inline test
  degrades to green-only).
- **Citations expected:** Do must cite `path:line` on the target branch for every change.
  **Starting point:** `iteration-v4/patch.diff` in this bundle. Its PUT side (the `MAX_KEY_BYTES`
  constant, the two PUT refusals, legs 1, 2, 4, 5 of its test, the bounded socket exchange and the
  abort-on-drop harness) was accepted at sign-off; keep it. Remove its bulk-delete bound
  (`DeleteRequestError::KeyTooLong`, `key_too_long()`, the parser check after the empty-key check,
  its mapping in `delete_objects`, and the unit tests that pin it), rewrite leg 3 to criterion (3)
  above, and correct the `MAX_DELETE_BODY_BYTES` comment and the architecture paragraph, which
  both describe the rejected whole-request refusal. **Peer callsites:** a header-time refusal on
  the PUT arm before any body byte is read is already done for CopyObject at
  `crates/gateway-s3/src/lib.rs:1800-1806`; mirror its placement and `error_response` use.
  Path keys are measured after percent-decoding (`percent_decode_utf8`, `:1765-1769`). For
  percent-encoding in the test, reuse `sigv4::uri_encode` (`crates/gateway-s3/src/sigv4.rs:163`)
  rather than a copy. The wire harness for bulk delete is `crates/server/tests/s3_delete_objects.rs`.
- **Prior-art check (triage cycles):** searched by path `crates/gateway-s3/src/lib.rs` (PUT arm,
  `parse_delete_request`, `MAX_DELETE_BODY_BYTES`). Merged: PR #612 (bulk DeleteObjects, #509),
  which deferred this rule. No key-length check has ever existed in `gateway-s3`, `gateway-core`
  or `server`. Open PRs on getwyrd/wyrd: none. Closed or rejected PRs on #620: none. Rejected
  within this bundle: iterations 1 to 4 built the same PUT side plus a whole-request bulk-delete
  refusal; iterations 1 to 3 were sent back only for the base's red C4-ci (the advisory wall), and
  iteration 4's sign-off rejected the bulk refusal (`iteration-v4/SUMMARY.md` §9). Follow-up filed:
  #893 (native key limit, independent of S3's).
- **Disposition hint:** likely-fix

## Why bulk delete has no per-key bound (decided at the iteration-4 sign-off, 2026-10-15)

- A whole-request refusal makes list-then-delete tools (`aws s3 rm --recursive`, `rclone purge`,
  `mc rm -r`) fail every 1,000-key batch that holds one legacy over-limit key, leaving the other
  999 keys in place.
- Any refusal, whole-request or per-key, strands keys above the request-target ceiling (about
  64 KiB, or about 21,800 key bytes when every byte needs percent-encoding): their single-object
  HEAD and DELETE answer `414`, so bulk delete is their only S3 exit. Such keys can be written
  today by `wyrd put --key` (up to its 128 KiB argv limit), and the CLI has no `delete`
  subcommand.
- That exit ends at the 8 MiB body cap. A key too large for it (only the in-process Rust API can
  write one) has no S3 exit today, and this fix does not change that. Closing it would mean new
  scope (a larger cap, a streaming parser, or a native delete), so it stays out of this PUT fix.
- Dropping the bound matches the documented S3 contract and MinIO (see Invariant). The body cap
  still bounds the request, which is what the issue thread's "not an unbounded vector" relied on.

**Plan-review response:** (plan review, 2026-10-15) Finding accepted; the brief is revised, not
defended. Criterion (3), the Invariant, Falsifiability, the Scope's doc-comment and architecture
items, and the section above now promise only that no delete path gains a length refusal, within
the unchanged 8 MiB body cap and request-target ceiling, and they name the residual (a key over
the body cap, writable only through `Gateway::put_object`, has no S3 exit). Re-verified on
`main` @ `7bf7eb9`: cap `crates/gateway-s3/src/lib.rs:507`, over-cap refusal `:1920-1921` and
`:2007-2009`, no length check in `crates/server/src/lib.rs:235-256`. The test is unchanged; its
70,000-byte fixture sits below the cap, which is all the narrowed criterion asks. For the human:
#893's text repeats the old claim ("no stored key is stranded whatever its length", and "keys
already stored ... stay readable and deletable"), so you may want to correct it; and recovering an
over-cap key is tracked nowhere. This pass files nothing (alpha is at 6/6, room 0).

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Iteration 5 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 4): rebuilding for the implementation-level findings — **Leg 3 only protects keys up to 70,000 bytes.** The brief's Invariant and the patch's own doc (`crates/gateway-s3/src/lib.rs:498-500`, `:518-525`) promise that bulk delete has **no** length bound below the 8 MiB body cap. The tests only check that promise up to the 70,000-byte fixture (`crates/server/tests/s3_key_length_limit.rs:328`, `crates/gateway-s3/src/lib.rs:4787`). Concrete failing case: in a scratch copy I added `if key.len() > 100_000 { return Err(DeleteRequestError::Malformed); }` right after the empty-key check in `parse_delete_request` (`crates/gateway-s3/src/lib.rs:2331-2333`). That mutant passed every test: `s3_key_length_limit` (5/5), all 82 `wyrd-gateway-s3` lib tests, and `s3_delete_objects` (22/22). Yet it strands every key of 100,001 to 131,072 bytes that `wyrd put --key` can write. Those keys are past the `414` limit, so bulk delete is their only S3 exit, which is exactly the iteration-4 sign-off defect. The fix is cheap. Over the real wire, bulk delete of an 8,388,000-byte key seeded through `Gateway::put_object` works today (200, `<Deleted>`, no `<Error>`, gone afterwards), and three such deletes (131 KB, 3 MB, 8.4 MB) took about 1 s in total. So leg 3, or the in-crate test where `RecordingGateway` makes it nearly free, can add a key near the cap (about 8,388,000 bytes, or at least one over 131,072). The brief did accept 70,000 bytes as enough for criterion (3), so declining with that recorded reason is also a valid answer.. Deferred, not addressed here: 1 finding(s) that need human judgment, held in deferred-findings.json for the human at handover — not build work
- Full previous attempt preserved in `iteration-v5/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 6 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 5): rebuilding for the implementation-level findings — **The tests only send signed single-shot PUTs, so they don't pin the limit for the PUT form the stock SDK actually uses.** Every new PUT leg goes through `signed()` (`crates/server/tests/s3_key_length_limit.rs:162`, `:175`) or `signed_key_limit_request` (`crates/gateway-s3/src/lib.rs:4666`, `:4683`). Both compute a signed body hash, so every request lands in the `PayloadHash::Signed` arm. Concrete failing case: in a scratch copy I moved the `key_name.len() > MAX_KEY_BYTES` check (`crates/gateway-s3/src/lib.rs:1862-1869`) inside the `sigv4::PayloadHash::Signed(hex) =>` arm (`:1944`). With that change, all 6 wire tests and all 3 in-crate #620 tests still pass. Yet under that change an `aws-chunked` PUT (`STREAMING-AWS4-HMAC-SHA256-PAYLOAD`, the stock SDK default, `:1929`) or an `UNSIGNED-PAYLOAD` PUT (`:1957`) stores a 1,025-byte key. That breaks the Invariant ("no object whose S3 key exceeds 1,024 UTF-8 bytes can be created through the S3 surface"). The same gap covers the `InvalidBucketName` check (`:1851`). The **shipped code is correct**: it runs both checks before the payload `match`. I confirmed this with a scratch probe: aws-chunked 1,025-byte PUT → `400 KeyTooLongError`, HEAD `404`; UNSIGNED-PAYLOAD 1,025-byte PUT → `400 KeyTooLongError`, HEAD `404`; aws-chunked `PUT /b%2Fa/<'x'×1023>` → `400 InvalidBucketName`. So this is only a missing regression guard. It matters because the PUT arm keeps getting header-time refusals added to it (#671 already landed there). The fix is cheap: add one aws-chunked leg (or one UNSIGNED-PAYLOAD leg) for the 1,025-byte key, reusing the signer and framer helpers already in `crates/server/tests/s3_put_object_ceiling.rs:156-272`. Declining it with a recorded reason is also fine, given the production code is right.. Deferred, not addressed here: 1 finding(s) that need human judgment, held in deferred-findings.json for the human at handover — not build work
- Full previous attempt preserved in `iteration-v6/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
