# Build notes — #938 (743.2): the S3 client verbs smoke sends

## Base

- Built in `$PDCA_WORKTREE` = `/home/eddie/wyrd/wyrd.pdca-wt-l2`, HEAD **`93a6f3715e8785a22fd1d1ab40206bcf51b3eef4`**
  (`pdca-integrate: issue_932`), the bundle's `stack-base-tip` on `pdca-integration/main-r42c5348f203d`.
- #853's code is on it, as the brief requires: `582da99 pdca-integrate: issue_853` is an ancestor;
  `crates/validate/src/s3/response.rs`, `crates/validate/src/s3/wire.rs` and
  `crates/validate/tests/s3_client_nonconforming_responses.rs` exist; `roxmltree.workspace = true` is under
  `[dependencies]` (`crates/validate/Cargo.toml:44`). Nothing of #853 was recreated.
- `patch.diff` is `git diff HEAD` on that commit (10 files; new files added intent-to-add, index restored
  afterwards). `git apply -R --check patch.diff` passes on the built tree.
- `crates/validate/src/lib.rs` is untouched (743.1 / 743.3 own it). The new types are reachable as
  `wyrd_validate::s3::…` (`crates/validate/src/s3.rs:70-77`).

## What changed (line numbers are in the patched tree)

| File | Change |
|---|---|
| `crates/validate/src/s3/answer.rs` (new) | `Head` (`:54`), `Answer` (`:121`), `Outcome` (`:128`), `BodyFailure` (`:151`), `Fetched` (`:249`), `NoResult` (`:283`); the `Capture` interceptor (`:376`, hooks `:426`, `:437`); `read_whole` (`:461`), the client's own budgeted, deadline-bounded body read; `settle` (`:330`) / `answer` (`:346`), which read a body against its status. |
| `crates/validate/src/s3/document.rs` (new) | `Listing`, `ListEntry`, `ListingError`, `DeleteResult`, `DeleteRow`, `KeyError`, `MAX_LISTING_PAGES = 100` (`:22`); strict readers `listing_page` (`:143`) and `delete_rows` (`:196`). |
| `crates/validate/src/s3/verbs.rs` (new) | `Probe` (`:40`, 12 operations); `S3Client::head_object` (`:127`), `get_object_range` (`:145`), `get_object_if_none_match` (`:168`), `list_objects_v2` (`:196`), `list_objects_v1` (`:230`), `delete_objects` (`:269`), `probe` (`:303`); helpers `captured` (`:376`), `read` (`:404`), `fetched` (`:429`), `page` (`:459`). |
| `crates/validate/src/s3/response.rs` | `error_document` (`:370`) now goes through a shared strict reader `whole_document` (`:463`) + `NotADocument` (`:424`) + `text_only` (`:497`) + `named_code` (`:413`). Every message it produced before is produced byte-identically (each `NotADocument` arm maps back to the old text). `MAX_ERROR_DOCUMENT_DEPTH` → `MAX_DOCUMENT_DEPTH` (`:509`, same value 16, private). Doc of `BUFFERED_BODY_BUDGET` extended to the bodies the client now reads itself. |
| `crates/validate/src/s3.rs` | modules + re-exports (`:46-77`); module docs gain the eighth item; `Deadlines::body_idle`/`body` docs extended to "a body the client reads itself"; `get_object`'s framing→length match moved verbatim into `object_length` (`:415`), called at `:316` (pure refactor: the same five arms, the same errors). |
| `crates/validate/src/s3/body.rs` | `source_of`, `error_chain` made `pub(super)` (used by `read_whole`). |
| `crates/validate/src/s3/error.rs` | `Phase::BodyIdle` / `Phase::Body` docs only. |
| `crates/validate/Cargo.toml`, `Cargo.lock` | `wyrd-traits` under `[dev-dependencies]` only (bucket-marker seeding, as the brief scopes). `blackbox-guard` green. |
| `crates/validate/tests/s3_client_smoke_verbs.rs` (new) | the brief's test, 12 tests. |

## The design, and why

**The invariant:** what the client reports is what the server sent; a failure after a 2xx head never erases
the 2xx (`crates/validate/src/s3/error.rs:1-7`, brief).

**Chosen: take the response from the SDK at the head and read the body ourselves.** `Capture`
(`answer.rs:376`) records status + every header field line + framing at `read_after_transmit`, and takes the
body at `modify_before_deserialization`, leaving the SDK an empty body. The SDK's deserialization result is
never consulted. The body is then read by `read_whole` under `BUFFERED_BODY_BUDGET` and the `body_idle` /
`body` deadlines, the same tokio clock as every other deadline of the request. The 2xx head therefore exists
before any body wait starts, so no body failure can replace it: a stall is `BodyFailure::Timeout` beside
`head.status == 200`, a cut is `Transport`, junk past the framing is `Overrun`, too much is `OverBudget`, and
a whole body that is not the operation's success body is `Outcome::Unreadable`.

**Why two hooks, not one.** The SDK's own `S3ExpiresInterceptor` (HEAD/GET object) runs at
`modify_before_deserialization` *before* a customize-level interceptor and rewrites headers (drops an
unparseable `Expires`, adds a synthetic `ExpiresString`) — `aws-sdk-s3` 1.152.0
`s3_expires_interceptor.rs:30-56`. Recording the head there would report a header the server never sent.
`read_after_transmit` runs for every interceptor before any `modify_before_deserialization`
(`aws-smithy-runtime` 1.16.0 `client/orchestrator.rs:510-514`), so the head is pristine there.

**Rejected: let the SDK read the body (the existing verbs' `ResponseGuard` path) and recover the head
afterwards.** This is what lost the 2xx at the first attempt: the body is read inside `send()` under the
*operation* deadline, and `SdkError::TimeoutError` carries no raw response. Fixing it inside that path needs
(sketch, counted against the code above): a deadline-carrying wrapper body with a pinned `tokio::time::Sleep`
(≈60 lines, replacing nothing), a head-recording interceptor (≈25, the same as `Capture`'s first hook), a
`read_after_deserialization` hook to recover the raw bytes of a 2xx the SDK *accepted* (≈20; the SDK's XML
reader accepts cut-off listings, so its `Ok` cannot be trusted and the bytes must be re-read strictly
anyway), and a merge of the recorded head into `TimeoutError` (≈30). That is ≈135 lines against
`Capture` + `read_whole` ≈ 140, so it is not cheaper — and it leaves **two** deadlines (the SDK's operation
deadline and the wrapper's) racing over one body, with the outcome depending on which fires first. The chosen
path has one owner for every body wait. It also needs no special case for GET: a 2xx GET body is handed to
the existing `ObjectBody` unread (`verbs.rs:429`).

**Rejected: new variants on `S3Error` / `BodyError`** (e.g. a `Timeout` that carries a status). Both are pub
enums matched by the existing tests and, likely, by 743.1 running in parallel; a new variant breaks any
exhaustive match there. New types (`Answer`, `Outcome`, `BodyFailure`) carry the new facts instead, and
`S3Error` is returned only for "no head arrived", exactly its existing meaning (via the unchanged
`S3Client::classify`).

**Rejected: buffered bodies for the ranged / conditional GET 2xx.** It would cap a GET at 64 KiB and break
the existing rule "a GET object is outside the budget: it streams". `Fetched::Object` hands the body to
`ObjectBody` (boxed only to satisfy `clippy::large_enum_variant`), so the 206/200 head is in the caller's
hands before any body read. A non-2xx GET (304, 404) is `Fetched::Other(Answer)`.

**Other decisions.**
- `Head::header` returns a value only when exactly one field line of that name exists: a repeated `ETag` is
  never read as its first line; `header_lines` gives all of them.
- Listing documents: `<IsTruncated>` required and exactly `true`/`false`; `<Contents>` needs exactly one
  `<Key>` and one decimal `<Size>`; a truncated page that names nothing to continue from is refused (V2 needs
  `<NextContinuationToken>`; V1 uses `<NextMarker>` or the last key, S3's v1 rule). Unknown elements are passed
  over. No ordering/count judgement: that is 743.1's.
- `MAX_LISTING_PAGES = 100`: the "every await is bounded" rubric item — an endpoint that always answers with
  another continuation would otherwise hold the call and grow memory without end. Each page is bounded by the
  budget, so a listing holds at most 100 × 64 KiB of bodies.
- `delete_objects` sends not-quiet, so the result names every deleted key. Rows keep document order
  (`DeleteResult::rows`), with `deleted()` / `errors()` views. A row's empty/absent `<Code>` is
  `ErrorCode::MissingInXml`, the same rule as an error document (`named_code`).
- Probes: bucket probes send the name given, no client-side check. The SDK sends a bucket-level request as
  `/{bucket}/` (trailing slash, path style); the test asserts that exact path.
- `captured()` has one `unreachable!` (`verbs.rs:399`): an SDK `Ok` without a captured head is impossible
  because `read_after_transmit` precedes every deserialization. Precedent: `get_object`'s `.expect(...)` for
  the same class of invariant (`s3.rs:312-315`).

**Known limits (for the human, and for 743.3).**
- Listing pages and delete results are read under the 64 KiB `BUFFERED_BODY_BUDGET`, as the brief's (D)
  requires. A conforming page of many entries (≈250 bytes of markup each plus the key) can exceed it: about
  200 short keys a page fit, far fewer with long keys. This is a typed `OverBudget` refusal, never a silent
  partial read, and is documented on `list_objects_v2` / `delete_objects`. 743.3 must pick a small
  `page_size` (it needs several pages anyway).
- Docs currency: I did not edit an architecture doc. The new operations are client verbs of the validator
  tool's library, not a Wyrd port, API operation, RPC, CLI flag or persisted field, and the brief's scope
  names only `s3.rs`, `src/s3/*`, the dev-dependency and the test. If the human reads the rule as covering the
  validator's client surface (proposal 0017 §6-7), that doc needs a line; flagging it rather than editing
  outside scope.

## Red → green

Verification posture (a) net-new, as pre-declared: with production reverted the test cannot build, so
C4-verify reports UNVERIFIABLE (77). Demonstrated directly: reverting `s3.rs`, `s3/body.rs`, `s3/error.rs`,
`s3/response.rs` to HEAD (test kept) → `cargo test -p wyrd-validate --test s3_client_smoke_verbs` exit 101,
20 `error` lines (`unresolved imports wyrd_validate::s3::Answer …`, `no method named list_objects_v2 …`), no
`running N tests` line. Files restored afterwards (checked byte-for-byte).

Green: `cargo test -p wyrd-validate --test s3_client_smoke_verbs` → `12 passed; 0 failed` in ≈1.7 s; five
consecutive runs green; three more runs green while `s3_client_roundtrip` ran beside it for CPU load. Every
run was bounded by `timeout 900` and every call inside the test by a 30 s `tokio::time::timeout`.

**(E):** `cargo test -p wyrd-validate` (whole crate) green, the three named files unmodified:
`s3_client_roundtrip` 15 passed, `s3_client_upload_peers` 12 passed, `s3_client_nonconforming_responses` 28
passed (plus `cli_surface` 21, `payload_and_sizes` 26, lib unit 4).

**Demonstrated reds (required).** Each was one mutation of production, applied, the test run, and reversed in
the same command (reversal checked by grep):

1. *Return only the timeout when a body stalls after a 2xx head* — in `probe`, turn
   `Outcome::BodyFailed(Timeout)` into `Err(S3Error::Timeout)`. → (C) RED: `22 case(s) lost or misreported
   the 200`, e.g. `CopyObject Stall: the 200 head was lost: Timeout { phase: BodyIdle, limit: 300ms }`
   (all 11 Stall and 11 Trickle cases; run with the earlier 300 ms idle deadline, since widened to 500 ms).
2. *Accept a listing document cut short* — in `listing_page`, a document that fails `whole_document` becomes an
   empty, final page. → (D) RED: `ListV2 cut-short: not refused as Unreadable: Listed([])` (6 cases across V2
   and V1), and the later-page test RED: `a cut second page must fail the whole listing: Ok(Listing { entries:
   [list/a, list/b …] })` — the partial listing the brief forbids.
3. *Let bucket-name handling refuse or rewrite the forbidden name* — two forms: rewrite
   (`trim '-'` + lowercase) → (B) RED `left: ("PUT", "/wyrd-validate-probe/")` in both the probe-matrix test
   and the bucket-name test; refuse (client-side S3-naming check → `RequestNotBuilt`) → RED `CreateBucket
   "wyrd-validate-probe-": refused or unanswered: the request was not built, nothing was sent`.
4. *Stop listing after the first page* — return the listing after page 1 in both listing loops. → (A) RED
   `left: [("list/a", 0), ("list/b", 1)]` against the real gateway (expected five keys over three pages).
5. *Drop the `<Error>` rows of a `<DeleteResult>`* — `delete_rows` discards `Error` rows. → (F) RED
   `left: [Deleted { key: "mixed/a" }, Deleted { key: "mixed/c" }]`.

## Refuting my own test

- **(a) Genuine red? Yes.** Fully reverted production: the test does not build (above), which is the
  pre-declared UNVERIFIABLE for a net-new slice. Partially reverted (each of the five defect classes put back):
  every one goes RED with the specific assertion named above, so the test binds each objective, not just
  "the symbols exist".
- **(b) Production path? Yes.** Every leg drives the production `S3Client`, built through the production
  `resolve_config` and `S3Client::with_deadlines`, on the real `aws-sdk-s3` and the client's own `WireClient`
  connector, over real loopback TCP. (A) and (B) hit the real `S3Gateway` + `wyrd_server::Gateway` (redb,
  `MemCoordination`, `FsChunkStore`), with the bucket marker seeded as `crates/server/tests/s3_list_objects.rs`
  does. No mock of the client, the SDK or the parser. The relay in (B) only forwards bytes and records the
  request head; it changes nothing.
- **(c) Fixture includes the fault? Yes.** The faults are injected on the wire, not curated out: (C) the
  scripted endpoint really sends a `200` head and then holds the socket open (stall), writes a byte every
  50 ms (trickle), closes mid-body (cut), or sends a cut-off document — for every probe that can carry a body
  (HeadBucket excluded: hyper reads no body for a HEAD, so there is nothing to stall). (D) the documents are
  really cut short, trailed by junk or a second document, trailed past `Content-Length`, or larger than the
  budget with a length and chunked; plus the multi-page case where page 1 is good and page 2 is cut. (F) the
  endpoint really sends the mixed `<DeleteResult>`. (B) uses six names S3's rules forbid (trailing `-`,
  upper case + `_`, too short, IP form, `..`, leading `-`) and shows each on the wire exactly as given, with
  the gateway's own 400 `InvalidRequest` and its 32-hex request id. The gateway's answers matched the brief's
  table exactly: 501 `NotImplemented` for copy / multipart ×6 / GetBucketLocation, 400 `InvalidRequest` for
  Create/Delete/ListBuckets, 400 with no body for HeadBucket.

## Commit-readiness

- `cargo fmt --all -- --check`: clean (rustfmt applied to every touched file).
- `cargo clippy -p wyrd-validate --all-targets` (workspace lints, warnings denied): clean, after boxing
  `Fetched::Object`'s body.
- `typos crates/validate/`: clean (typos-cli 1.48.0).
- `./engine/xtask.sh ci-dry-run` (all repo guards: gitlink, unsafe, blackbox): pass.
  `./engine/xtask.sh blackbox-guard`: pass (`wyrd-traits` is a dev-dependency only).
  `./engine/xtask.sh statics`: pass.
- `cargo doc -p wyrd-validate` (lints capped): no warning from these changes. With the workspace's
  `warnings = "deny"` it fails on base already, at `crates/validate/src/sizes.rs:340` (a public doc linking the
  private `Tuning::check`) — pre-existing, untouched here, and not part of `cargo xtask ci`.
- I did not run the full `cargo xtask ci` (workspace build/test incl. DST); Check's C4-ci runs it.

## External dependencies

`typos` (present, run) and `docs-renderer` (`markdown_it` + `yaml` import, present) as the brief lists. No
Markdown doc was changed, so the docs check has nothing new to render. No unregistered dependency was needed:
no Docker, no network beyond `127.0.0.1`. No NEEDS-HUMAN external-dependency item.
