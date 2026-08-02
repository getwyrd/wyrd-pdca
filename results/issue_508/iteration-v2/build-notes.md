# Build notes — issue 508 / multipart-upload (iteration 2)

Written against the target worktree `$PDCA_WORKTREE` at `ee801ec` (`pdca-integration/main`
= the folded 507+509 wave base). Every `path:line` below is that tree **with this patch
applied**, unless it says "base".

## What changed since iteration 1 (the carry-forward)

Iteration 1's feature was kept (it was accepted on C1/C3/C5/T1/T2/T3); this iteration
clears the two blocking reds and the four advisory findings, and closes the flagged open
question. Nothing was re-submitted unchanged.

### 1. C4-ci red — `typos`  (BLOCKING, fixed)

Four misspellings in changed text. Reworded rather than allow-listed (the allowlist stays
tight, per the xtask hint):

* `crates/core/src/multipart.rs:494` "silently mis-composing the ETag" → "silently
  composing a wrong ETag";
* `crates/core/src/multipart.rs:557` "a mis-composition" → "a wrong composition";
* the test's "mis-ordered assembly" → "out-of-order assembly"
  (`crates/server/tests/s3_multipart_upload.rs:312`);
* the `pn` binding (`typos` reads it as `on`) is gone with the classifier rewrite below.

`typos` now exits 0 over the whole tree, and `cargo xtask ci` (fmt + clippy `-D warnings` +
build + test + deny + conformance + typos + docs) is **green** — run through the project's
own runner, `./engine/xtask.sh ci` with `PDCA_WORKTREE` pointed at this tree.

### 2. C2/C4 red→green shape — the pre-fix leg must be an ASSERTION, not a compile error  (BLOCKING, fixed)

Iteration 1's test called the patch-added `Gateway::run_gc` (and needed a patch-added
`md-5` dev-dependency), so reverting the production change made the test **fail to
compile**. The brief requires the red to be a wire-level assertion. Two causes, both
removed:

* **`Gateway::run_gc` is deleted from the patch** (it no longer exists anywhere). The test
  now drives the **real custodian control point directly** —
  `wyrd_custodian::reconcile_step` with a `GcContext`
  (`crates/server/tests/s3_multipart_upload.rs:224-250`), wired exactly as the deployable
  role wires it (`crates/server/src/custodian.rs:332-351`, base). That is *more* faithful
  than `run_gc` was: in a deployment the sweep is the custodian role's, not the gateway's.
  Two seams were needed to reach the gateway's own state, and neither is a stand-in:
  - `SharedMeta(Arc<RedbMetadataStore>)` (`:96-116`) — a 3-method **forwarding handle** to
    the production redb store (`Gateway::new` takes its store by value and redb's in-memory
    backend cannot be reopened). No behaviour is re-implemented; the gateway and the
    custodian address one shared store, which is the deployment shape.
  - `DServerView` (`:120-180`) — one D server's view of the single-authority chunk root,
    filtering `list_fragments` to the fragments whose index is that server's id (the
    identity placement the write path uses). This mirrors the `Fleet` view the base peer
    test builds (`crates/server/tests/custodian_gc.rs:198-241`). **This is load-bearing,
    not cosmetic**: presenting one root under all nine ids shows every fragment eight times
    under a server that never held it, and the post-restore pass then (correctly) marks
    those as stranded — an artefact of the wiring, not of the code under test.
* **The `md-5` dev-dependency is dropped** (`crates/server/Cargo.toml` and `Cargo.lock` are
  no longer touched by the patch at all). `run-verify.sh`'s RED leg reverts *every* modified
  production file — including a manifest — so a test that needs a patch-added dev-dep is
  compile-red by construction. The independent MD5 oracle is now **known answers** pinned in
  the test (`:72-94`), computed off this codebase with CPython `hashlib` (the exact
  re-derivation one-liner is in the file). The oracle is still independent — the server's own
  `wyrd_core::md5` is never used to check the server — and it now costs the test nothing.

Result, through the project's own per-fix gate
(`engine/scripts/run-verify.sh`, `PDCA_VERIFY_BASE=origin/pdca-integration/main`):

```
run-verify.sh: GREEN — cargo test -p wyrd-server --test s3_multipart_upload (fix applied)
test result: ok. 7 passed; 0 failed
run-verify.sh: RED — … (production reverted, test kept)
test result: FAILED. 0 passed; 7 failed
run-verify.sh: PASS — red without the fix, green with it.
```

All seven fail by **assertion** on the base (`501 NotImplemented — the 'uploads' S3
subresource/operation is not supported` at the first wire call; the malformed-query leg
fails on `PUT /wyrd-bucket/victim?part%4Eumber=1 must be refused, got 200`). The RED leg
**compiled** — no missing symbol, no missing crate.

### 3. Destructive fall-through — a malformed multipart query reached the plain verb  (advisory, fixed)

The adversary's finding, and the worst of the four: after iteration 1's
`multipart_object_op` returned `None`, the request fell through to the plain-verb dispatch,
so `PUT /b/k?partNumber=1` (no `uploadId`), a non-numeric part number, `PUT /b/k?uploads`
and `DELETE /b/k?uploads` **overwrote or deleted the whole object** — each a safe `501` on
the base.

The classifier is now **three-way and fail-closed**
(`crates/gateway-s3/src/lib.rs:2161-2276`): `MultipartRoute::{Op, Refuse, Plain}`, where
`Plain` is returned **only** when none of `uploads` / `uploadId` / `partNumber` is present
(decoded — #491). Every other shape leaves as `Refuse`, and the dispatcher answers it
before the plain arms (`:1594-1597`). Refusals: `501 NotImplemented` for a structurally
unknown form (what the denylist answered), `400 InvalidArgument` for a bad part number.
The denylist doc-comment records the new invariant (`:341-346`).

Covered by the wire test `a_malformed_multipart_query_never_destroys_the_object`
(`crates/server/tests/s3_multipart_upload.rs:1039-1101`): nine attack forms over raw signed
HTTP against a stored object, asserting the refusal **and** that the object is still
present and byte-identical after each. Plus the unit table
`a_malformed_multipart_query_is_refused_never_plain` (`crates/gateway-s3/src/lib.rs:4249`),
20 forms.

### 4. UploadPart dropped the signed-payload integrity check  (advisory, fixed)

`ContentHash` now crosses the multipart seam:
`MultipartGateway::upload_part(..., expected: ContentHash)`
(`crates/gateway-core/src/lib.rs:236-262`). The wire layer passes `Expected(hex)` for a
single-shot signed body and `Unverified` for `aws-chunked` (per-chunk signatures already
verified) and unsigned — the *same* mapping the plain PUT arm uses
(`crates/gateway-s3/src/lib.rs:2341-2400`). The server checks it over the **same streamed
sha256** the object path checks, before the part record commits, by nesting the existing
wrappers: `HashingSource::new(Md5Source::new(source))` plus a `finalize_with_inner`
accessor so ONE pass yields both digests (`crates/server/src/lib.rs:605-628`, `:868-874`).
A mismatch is `PayloadMismatch` → `400 XAmzContentSHA256Mismatch`, and the staged chunks
stay leased garbage the GC reclaims — nothing is staged.
Bound by `upload_part_refuses_a_body_that_does_not_match_the_signed_hash`
(`crates/server/tests/s3_multipart_upload.rs:1103-1194`), which signs the digest of one
body and delivers another, then asserts `ListParts` is empty.

### 5. Part-number range unvalidated  (advisory, fixed)

`1..=10_000` enforced at the routing boundary (`crates/gateway-s3/src/lib.rs:2166-2167`,
`:2232-2242`); `partNumber=0`, `10001`, `-1`, `abc` and the empty value are all
`400 InvalidArgument`. This closes the orphan-until-abort footprint the adversary named
(a `partNumber=0` part could be staged but never completed, since `assemble` requires
strictly-ascending numbers `> 0`).

### 6. Pagination was stubbed (`IsTruncated=false` hard-coded)  (advisory + brief open question, fixed)

Both listings now paginate with 507's **materialize-sort-slice** split — the seam returns
the complete set, the wire layer orders and slices it:

* `ListParts`: `max-parts` (default/clamp 1000) + `part-number-marker`
  (`crates/gateway-s3/src/lib.rs:2497-2557`), rendering the real `IsTruncated`,
  `PartNumberMarker`, `MaxParts` and — only when truncated — `NextPartNumberMarker`
  (`:2688-2740`);
* `ListMultipartUploads`: `max-uploads` + `key-marker`/`upload-id-marker` over a
  `(key, upload-id)` ordering (`:2564-2617`, renderer `:2742-2790`).

A **zero-length page is never truncated** (`:2545`, `:2606`) — the same rule the object
listing applies to `max-keys=0` (`:562-567`) and for the same reason: a truncated page with
no resume marker loops a conforming paginator forever. Asserted over the wire (SDK
`max_parts(1)` → truncated + marker → second page resumes; `max_parts(0)` → empty and
NOT truncated; `max_uploads(1)` likewise) and in unit tests (`:4290-4368`).

### 7. Complete's conflict mapping (found while reviewing; not raised by Check)

Iteration 1 mapped every `Conflict` from the Complete batch to `NoSuchUpload`, which is
wrong for a dirent/inode race with an ordinary PUT (retryable, S3 `409`). It now re-reads
the session to distinguish "session gone" (`NoSuchUpload`) from "object write raced"
(`Conflict`) — the pattern `upload_part` already used (`crates/server/src/lib.rs:751-771`).
Nothing is published either way.

## Cost of the alternatives I rejected

* **Keeping `Gateway::run_gc` and having the test call it.** Exactly the rejected approach:
  1 new `pub` method (35 lines) whose *only* caller is the test, and it makes the RED leg a
  compile error — the C2/C4 finding. Deleting it removed 35 production lines and 6 imports
  (`reconcile_step`, `Custodian`, `ExpiredPendingPolicy`, `FencedZone`, `GcContext`,
  `Reconciled`, plus `BoxError`/`ChunkStore`/`DServerId`) from `crates/server/src/lib.rs`
  and cost the test 30 lines of fleet wiring that mirrors an existing peer
  (`custodian_gc.rs`). Strictly better on both axes.
* **Keeping the `md-5` dev-dependency and accepting a compile-red.** 8 lines in
  `Cargo.toml` + 1 in `Cargo.lock` — cheap in diff size, but it forfeits the brief's
  binding requirement ("fail by ASSERTION, not compile error"), which is not a cost-vs-
  minimalism trade at all. The KAT constants are 6 lines and are a *stronger* oracle
  (fixed, externally computed, no shared-implementation risk).
* **A guard AFTER the classifier** (`if multipart_key_present(query) { refuse }` bolted on
  the existing `Option`-returning function, ~8 lines) instead of making the classifier
  three-way (~50 lines with docs). Rejected because it is exactly the "guard the symptom"
  shape: the classifier could still return `None` for a multipart-keyed request, and the
  next reader adding a route would have two places to keep in sync. The three-way return
  makes "a multipart-keyed request can reach the plain verb" **unrepresentable** — the
  invariant the denylist used to hold is restored structurally, not patched over
  (`docs/principles.md` §1.2/§2: smallest change that restores the invariant, not smallest
  diff).
* **Eager fragment deletion on Abort** (instead of orphan + GC). Would have removed the
  whole GC leg from the test, but it breaks the reader-safe grace design (chunk deletion is
  not transactional with the metadata batch, so a crash mid-delete strands bytes with no
  ledger entry) and contradicts the brief's design. Rejected.

## Chosen scope boundary (stated per brief)

**GC of abandoned-but-unaborted sessions is deferred** (brief §Scope explicitly allows it):
a session whose `upload:{id}` record survives keeps its parts *referenced*, so they are
never reclaimed — correct, but an abandoned session leaks until a multipart-expiry sweep
reaps stale sessions. `SessionRecord.created_millis` is stored so that sweep has its input.
Also out of scope, unchanged: `UploadPartCopy`, per-part `Content-MD5`/checksum trailers,
part-ranged `GET ?partNumber=N` (refused `501`, as on the base).

## Refuting my own test (forced)

* **(a) Genuine red?** Yes, and by assertion. The project's own `run-verify.sh` RED leg
  (production reverted, test kept) compiles and fails **7/7** on wire 501s — output quoted
  above. I additionally refuted each *new* guard by reverting it alone in the working tree:
  - malformed-query guard → `PUT /wyrd-bucket/victim?partNumber=1 … got 200` (the object
    was overwritten): RED;
  - `ContentHash` on `upload_part` → the tampered part is accepted, `left: 200, right: 400`:
    RED;
  - the custodian's `uploadpart:` reference hook (`crates/custodian/src/gc.rs:293-323`) →
    the post-restore pass marks **54 of 54** staged fragments stranded (0 with the hook):
    RED. Each was restored and re-run green.
* **(b) Production path?** Yes. Everything is driven over the wire — a stock `aws-sdk-s3`
  client and raw SigV4-signed HTTP against the real `S3Gateway` listener over the shipping
  `Gateway<_, FsChunkStore, MemCoordination>` composition. The maintenance legs call the
  **production** `wyrd_custodian::reconcile_step` / `reconcile_after_restore` (base symbols,
  unmodified by this patch except the reference-set hook). The only test-local types are a
  forwarding store handle and a per-D-server view of the real store; neither implements any
  behaviour under test. The test imports **no** symbol this patch adds.
* **(c) Fixture includes the fault?** Yes. The abort leg stages real ≥5 MiB parts, asserts
  the on-disk `.frag` count rises, asserts the fragments are **still there** after abort
  (orphaned, not eagerly deleted), then runs GC and asserts the count returns to the
  pre-upload baseline — the failing element (the aborted upload's bytes) is in the fixture,
  not curated out. The live-upload leg runs the maintenance passes **while the upload is
  open** rather than around it. The malformed-query leg attacks a real stored object and
  re-reads it byte-for-byte after every attack. The wrong-part leg feeds a genuinely
  non-matching ETag and asserts `NoSuchKey` afterwards.

## Verification performed

* `./engine/xtask.sh ci` (the project's own gate: fmt, clippy `-D warnings`, build, test
  incl. DST, cargo-deny, conformance vectors, typos, docs) — **all checks passed**, run in
  `$PDCA_WORKTREE`. This is also the commit-hook readiness check: `cargo fmt --all` was run
  over every touched file and `cargo fmt --check` is clean.
* `./engine/scripts/run-verify.sh` (per-fix red→green, base `origin/pdca-integration/main`)
  — **PASS: red without the fix, green with it**. Its throwaway lane worktree was removed
  afterwards; all scratch lived under `$PDCA_SCRATCH/pdca-builder-508-*` and is deleted.

## Pre-declared deferred leg (off-Check, per brief §Verification posture)

The headline *"`aws s3 cp` of an 8+ GB file round-trips sha256-identical"* is observable
only off-Check against a deployed stack (registered doctor row "aws cli (S3 gateway
round-trip)"). The machinery it exercises IS built and exercised at Check by the SDK
integration test — same verbs, same wire forms, same streaming path, smaller bodies. No
external dependency was missing for the Check-level proof, so there is **no** NEEDS-HUMAN
external-dependency marker in this bundle; the large-object leg is the maintainer's
sign-off/post-merge confirmation to record in §9.

Also for §9 (carried from the brief's open questions): whether the record-prefix/GC design
should be captured as an ADR before merge. This proposal is written to be sufficient; the
prefixes are metadata-keyspace content, not on-disk format, so conformance vectors are
untouched.
