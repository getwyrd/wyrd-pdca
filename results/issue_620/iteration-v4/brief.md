# Brief — issue 620 / s3-object-key-length-limit

> The Plan artifact (docs 02 §PLAN). Do reads ONLY this file. The `- **Label:** value`
> lines are parsed by the driver — keep their shape.
>
> Citations verified against `getwyrd/wyrd` `origin/main` @ `7bf7eb9` on 2026-10-14.

- **Slug:** s3-object-key-length-limit
- **Track:** alpha
- **Defect:** The S3 gateway enforces S3's 1,024-byte object-key limit nowhere. A `PUT
  /{bucket}/{key}` stores a key of any length: the decoded key is composed at
  `crates/gateway-s3/src/lib.rs:1765-1769` and streamed straight into `put_object_streaming`
  (`:1829-1868`). `parse_delete_request` (`:2171-2245`) accepts any non-empty `<Key>`
  (`:2209-2212`) up to the whole 8 MiB body cap. Yet `MAX_DELETE_BODY_BYTES` (`:488-507`) is sized
  on "1000 keys × a 1024-byte key", so the cap and the parser disagree about what one entry can
  be (the #612 review follow-up in this issue's thread). The limit cannot land in bulk delete
  alone. PUT would still create a 2,000-byte key, and bulk delete would then refuse to delete
  it, which is the "undeletable object" this issue was deferred to avoid.
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
  (3) **Bulk delete's per-entry bound matches.** A `DeleteObjects` request naming a 1,025-byte key
  next to an existing ordinary key is refused as a whole with `400` `KeyTooLongError`, and no key
  is touched: the ordinary object still exists afterwards. A request whose keys are all ≤ 1,024
  bytes, including one of exactly 1,024 bytes, behaves as it does today.
  (4) **Deletion never strands an object.** An over-limit object that already exists stays
  removable by single-object `DELETE`. The test seeds `b/<'a' × 1025>` through the existing
  backend API, `Gateway::put_object` (`crates/server/src/lib.rs:235`), on an `Arc<Gateway>` clone
  it keeps before handing the gateway to `S3Gateway::new` (`crates/gateway-s3/src/lib.rs:232`
  takes `Arc<G>`). Then: signed HEAD of that key is `200`; signed `DELETE` is `204`; HEAD again
  is `404`. The HEAD-before and HEAD-after legs are required, because DELETE answers `204` for
  an absent key too (`crates/gateway-s3/src/lib.rs:1887-1889`), so `204` alone proves nothing.
  This is the way out for any over-limit object stored before this fix or written outside the S3
  surface (the data-plane `wyrd put --key`, `crates/server/src/cli.rs:562-568`).
  (5) **No bucket-separator alias around the limit.** The gateway splits the path on the first
  literal `/` before decoding (`crates/gateway-s3/src/lib.rs:567-572`) and joins the decoded
  bucket and key into one flat key (`:1765-1769`). So `PUT /b%2Fa/<'x' × 1023>` has a 1,023-byte
  request key yet stores the same object as `/b/a/<'x' × 1023>`, whose key under bucket `b` is
  1,025 bytes. A PUT whose decoded bucket segment contains `/` must be refused `400`
  `InvalidBucketName` (S3 bucket names cannot contain `/`) before the body is stored: afterwards
  HEAD of `/b/a/<'x' × 1023>` is `404`. Send this one as signed raw HTTP (the SDK will not put
  `%2F` in a bucket name).
- **Falsifiability:** RED on `origin/main`, in-process, no special environment: (1) answers
  `200` and stores the object, (2)'s `€` key answers `200`, (3) answers `200` and deletes the
  ordinary key, and (5) answers `200` and stores `b/a/<'x' × 1023>`, so each fails on an
  assertion. The test uses only pre-existing API (the wire, plus `Gateway::put_object` for (4)'s
  fixture, which `crates/server/tests/e2e.rs:38` already calls), so C4-verify's reverted base
  compiles it. Leg (4) is green before and after; it pins the decision and is not the red.
- **Invariant to restore:** No object whose S3 key exceeds 1,024 UTF-8 bytes can be **created**
  through the S3 surface, under any spelling of the request path (including a `%2F` that moves
  part of the key into the bucket segment), and every object that exists stays **deletable**
  through it. The
  refusal therefore sits on the creating path, and no refusal on a delete path can strand an
  object. **Sources:** AWS S3 User Guide, "Naming Amazon S3 objects": the key name is a sequence
  of Unicode characters whose UTF-8 encoding is at most 1,024 bytes long (authoritative for S3
  behaviour); `KeyTooLongError` is S3's error code for it (S3 API Reference, Error responses).
  **C-1** (`docs/principles.md` §5): a state nothing can exit is never an acceptable cost
  (internal, Tier C). SELF-TEST: guarding the bulk-delete parser alone breaks the second half of
  the invariant; that is exactly the #612 finding.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:**
- **Conflicts with:** 671
- **Ordering note:** `Conflicts with 671`: both add a header-time refusal to the same S3 PUT arm
  (`crates/gateway-s3/src/lib.rs:1765-1830`, before the body streams). #620 refuses the key and
  #671 refuses the size, so they must be built in different waves. This one is wave 0. #767 and
  #666 also edit `crates/gateway-s3/src/lib.rs`, in regions that do not overlap these (the
  timestamp formatter; the generic bounds and test doubles). **Intake-cap override
  (wyrd-pdca-P1):** the human allowed all seven ids of this batch (656, 666, 671, 767, 768, 620,
  686) in over the alpha cap in the 2026-10-14 Plan session. Count at that moment: alpha 4/6
  planned (508, 625, 633, 654), room 2, need 7.
- **Surfaces:** data
- **Difficulty:** low
- **Scope:** S3 key-length enforcement where an object is created (PUT), the refusal of a PUT
  whose decoded bucket segment contains `/` (criterion 5), and the per-entry bound
  in the bulk-delete parser, with the `MAX_DELETE_BODY_BYTES` sizing comment kept true / out of
  scope: CopyObject, which answers `501` today (`crates/gateway-s3/src/lib.rs:1800-1806`; when
  #504 implements it, the same rule applies to its destination key); single-object GET, HEAD and
  DELETE (unchanged, see criterion 4); listing parameters (`prefix`, `start-after`,
  `continuation-token`); the data-plane CLI `wyrd put --key`, which is not an S3 surface; bucket
  naming rules, except the one criterion (5) needs (no `/` in a decoded bucket, on PUT only; GET,
  HEAD and DELETE on such a path stay as they are, since they create nothing); the multipart verbs (#508's slices apply the same rule to
  CreateMultipartUpload's key when they land); the 8 MiB cap value itself.
- **Repro instruction:** On `origin/main`, with the in-process gateway (`start_gateway` as in
  `crates/server/tests/s3_list_objects.rs:82-114`, a bucket marker seeded):
  `PUT /b/<'a' × 1025>` answers `200` and HEAD finds it. A `DeleteObjects` body naming that key
  plus an existing `b/ok` answers `200` and reports both deleted, and `b/ok` is gone.
- **External dependencies:** `typos`
- **Test file:** `crates/server/tests/s3_key_length_limit.rs` — a **NEW** file (C4-verify treats
  only an added `crates/*/tests/*.rs` as the per-fix discriminator; an appended or inline test
  degrades to green-only).
- **Citations expected:** Do must cite `path:line` on the target branch for every change.
  **Peer callsites:** a header-time refusal on the PUT arm before any body byte is read is
  already done for CopyObject at `crates/gateway-s3/src/lib.rs:1800-1806`; mirror its placement
  and `error_response` use. The bulk-delete parser's existing whole-request semantic bounds (the
  empty-key check at `:2209-2211`, the `>1000` bound at `:2241`) are the pattern for the new per-entry bound.
  Bulk-delete keys are XML-entity-decoded once and **not** percent-decoded
  (`crates/server/tests/s3_delete_objects.rs:24-28`); measure the key as the parser yields it.
  Path keys are measured after percent-decoding (`percent_decode_utf8`, `:1765-1769`). The wire
  harness for bulk delete is `crates/server/tests/s3_delete_objects.rs`.
- **Prior-art check (triage cycles):** searched by path `crates/gateway-s3/src/lib.rs` (PUT arm,
  `parse_delete_request`, `MAX_DELETE_BODY_BYTES`). Merged: PR #612 (bulk DeleteObjects, #509),
  which deferred this rule. No key-length check has ever existed in `gateway-s3`, `gateway-core`
  or `server`. Open PRs on getwyrd/wyrd: none. Closed or rejected PRs on #620: none.
- **Disposition hint:** likely-fix

Plan-review response: both findings revised in place. Finding 1 (bucket `%2F` alias): checked
at `crates/gateway-s3/src/lib.rs:567-572` and `:1765-1769`, it holds; added criterion (5) and
widened the invariant rather than qualifying it, with the separator rule kept to PUT. Finding 2
(DELETE `204` is also the absent-key answer, `:1887-1889`): criterion (4) now seeds a real
over-limit object through `Gateway::put_object` and requires HEAD `200` → DELETE `204` → HEAD `404`.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 1): rebuilding for the implementation-level findings — C4 Verification (red→green) — Resolve the unchanged TiKV dependency-policy failure and complete the halted CI stages — independent red→green and 27 wire tests pass, but the frozen full gate stops on two rustls advisories outside this patch (`reviewer-evidence/pinned-green.log:30`, `reviewer-evidence/pinned-green.log:41`; `gate-logs/C4-ci.log:3473`; `Cargo.lock:3294`).; T5 Judgment — Make the regression harness bounded and cancel its owned server on drop — a stalled response must fail promptly, and fixture teardown must end its helper’s lifetime (`crates/server/tests/s3_key_length_limit.rs:61`, `crates/server/tests/s3_key_length_limit.rs:92`, `crates/server/tests/s3_key_length_limit.rs:153`; `AGENTS.md:181`).; **The bulk-delete leg doesn't check that the parser counts bytes rather than characters.** Leg 3 uses only ASCII keys (`crates/server/tests/s3_key_length_limit.rs:266`, `:290`). I changed `crates/gateway-s3/src/lib.rs:2266` from `key.len()` to `key.chars().count()`, and all 5 tests still pass. Under that change, a 342 × `€` entry (1,026 bytes) would be accepted. The current code does refuse it: I probed it, and it returns `400 KeyTooLongError` with the ordinary key untouched. Fix: add a multi-byte over-limit entry to leg 3. This is moot if the human drops the bulk bound per the finding above.; `crates/server/tests/s3_key_length_limit.rs:161`: The new HTTP helper awaits connect, writes, and `read_to_end` without a deadline. A server regression that stalls a response or leaves the connection open hangs the test instead of producing a bounded failure. Wrap the complete socket exchange in a timeout, as required by the await-discipline rubric.; `crates/server/tests/s3_key_length_limit.rs:92`: The spawned server's `JoinHandle` is discarded, so dropping `Harness` removes its temporary directory without cancelling the server that still owns the backend. Runtime teardown eventually cancels it, but the new harness does not satisfy the explicit abort-on-drop rule. Retain the handle in `Harness` and abort it on drop, including assertion-failure paths.; `crates/gateway-s3/src/lib.rs:1835` and `crates/gateway-s3/src/lib.rs:2110`: PUT and DeleteObjects gain new rejection behavior, but the diff contains no living architecture documentation update. Document the decoded-byte limit, PUT's bucket-separator rejection, whole-request bulk refusal, and continued single-object access/deletion of legacy keys, as required by the docs-currency rubric.; C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo deny --all-features --config deny-all-features.toml check advisories` failed with exit status: 1; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 7 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_620/review-b. Deferred, not addressed here: 3 finding(s) that need human judgment, held in deferred-findings.json for the human at handover — not build work
- Failing gate: C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) — xtask: `cargo deny --all-features --config deny-all-features.toml check advisories` failed with exit status: 1
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 12 mutants tested in 44s: 8 missed, 3 caught, 1 unviable
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 7 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_620/review-b
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 2): rebuilding for the implementation-level findings — C4 Verification (red→green) — Resolve the existing TiKV advisory-policy failure and obtain a complete CI run — focused red→green passes, but frozen CI rejects unchanged dependencies and the independent run hits a read-only advisory-cache lock (`gate-logs/C4-ci.log:6480`, `gate-logs/C4-ci.log:6510`; `reviewer-evidence/ci.log:3267`; `Cargo.lock:3294`).; `crates/gateway-s3/src/lib.rs:4625` and `crates/server/tests/s3_key_length_limit.rs:136`: The new `path_encode` and `encode` helpers both duplicate the existing public `sigv4::uri_encode(key, true)` (`crates/gateway-s3/src/sigv4.rs:163`), including its unreserved set, slash escaping, and uppercase UTF-8 byte escapes. Reuse that helper to remove both copies and avoid their temporary `String` allocation per input byte. Keep `encode_all`, whose deliberate escaping of unreserved bytes exercises a different case. This is an optional test-code cleanup, not a correctness defect.; C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo deny --all-features --config deny-all-features.toml check advisories` failed with exit status: 1. Deferred, not addressed here: 5 finding(s) that need human judgment, held in deferred-findings.json for the human at handover — not build work
- Failing gate: C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) — xtask: `cargo deny --all-features --config deny-all-features.toml check advisories` failed with exit status: 1
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 3): rebuilding for the implementation-level findings — C4 Verification (red→green) — Decide disposition of the unchanged TiKV dependency-policy failure under #543 — independent red→green passes, but both audits reject RUSTSEC-2026-0346/0349, so the full gate remains red; the supplementary DST sweep is incomplete (`reviewer-evidence/green.log:15`; `reviewer-evidence/deny-all-advisories.log:10`, `reviewer-evidence/deny-all-advisories.log:40`; `deny-all-features.toml:80`).; C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo deny --all-features --config deny-all-features.toml check advisories` failed with exit status: 1. Deferred, not addressed here: 5 finding(s) that need human judgment, held in deferred-findings.json for the human at handover — not build work
- Failing gate: C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) — xtask: `cargo deny --all-features --config deny-all-features.toml check advisories` failed with exit status: 1
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 4 — carry-forward (from the previous attempt)
- Sign-off rationale: Bulk-delete design must change; the PUT-side limit (incl. %2F bucket refusal) is right and should carry over unchanged. - Criterion (3)'s whole-request refusal breaks bucket-emptying tools: one legacy over-limit key fails every 1,000-key DeleteObjects batch it sits in (aws s3 rm --recursive, rclone purge), leaving the other 999 keys (§6 items 6, 9). - It also strands keys above the request-line ceiling (~64 KiB, ~21 KiB percent-encoded): HEAD/DELETE answer 414 there, and bulk delete was their only exit (§6 item 5). - Re-plan bulk delete: per-key <Error><Code>KeyTooLongError</Code> rows (other keys still deleted), or no per-entry bound (body cap only). Check what real S3 does for an over-limit key in DeleteObjects before choosing. - Decide whether `wyrd put --key` gets the same 1,024-byte cap (closes the source of new over-limit keys) — in this slice or a follow-up.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Bulk-delete design must change; the PUT-side limit (incl. %2F bucket refusal) is right and should carry over unchanged.
  - Criterion (3)'s whole-request refusal breaks bucket-emptying tools: one legacy over-limit key fails every 1,000-key DeleteObjects batch it sits in (aws s3 rm --recursive, rclone purge), leaving the other 999 keys (§6 items 6, 9).
  - It also strands keys above the request-line ceiling (~64 KiB, ~21 KiB percent-encoded): HEAD/DELETE answer 414 there, and bulk delete was their only exit (§6 item 5).
  - Re-plan bulk delete: per-key <Error><Code>KeyTooLongError</Code> rows (other keys still deleted), or no per-entry bound (body cap only). Check what real S3 does for an over-limit key in DeleteObjects before choosing.
  - Decide whether `wyrd put --key` gets the same 1,024-byte cap (closes the source of new over-limit keys) — in this slice or a follow-up.
- Full previous attempt preserved in `iteration-v4/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
