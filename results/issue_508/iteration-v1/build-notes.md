# Build notes — issue 508 / multipart-upload

## What this delivers

The full S3 multipart verb set end-to-end, mapped onto the metadata keyspace and the
existing streaming write path:

- **`crates/core/src/md5.rs`** (new) — an in-crate RFC-1321 MD5 (part ETag + composite ETag).
  Kept in-tree deliberately: the brief's Impact section says *"No new dependency"*, and adding
  the `md-5` crate as a **production** dep would be a human-only ADR-0003/deny audit. MD5 is
  used **only** for the S3 ETag change-token, never integrity.
- **`crates/core/src/multipart.rs`** (new) — the multipart state machine over `MetadataStore`:
  disjoint `upload:{id}` / `uploadpart:{id}:{part}` records (ADR-0046 record pattern), keyed by
  the server-minted opaque upload-id (bucket/key stored *inside* the JSON, never in the key);
  create/commit-part/complete/abort/list, plus the pure `assemble` (validate + compose ETag).
- **`crates/gateway-core/src/lib.rs`** — the neutral `MultipartGateway` seam (no S3 vocabulary,
  ADR-0010) + `StagedPart`/`MultipartUpload`/`CompletePart`/`CompletedMultipart`, and four new
  `GatewayError` variants (`NoSuchUpload`/`InvalidPart`/`InvalidPartOrder`/`EntityTooSmall`).
- **`crates/server/src/lib.rs`** — `Gateway`'s `MultipartGateway` impl (UploadPart streams
  through `write::stream_write_data`, computing the part MD5 in an `Md5Source` wrapper as the
  bytes flow, never buffering the part); the composed ETag stamped onto the object inode; plus
  `Gateway::run_gc` (the co-located custodian sweep — see below).
- **`crates/gateway-s3/src/lib.rs`** — removes `uploads`/`uploadId`/`partNumber` from the
  subresource denylist and routes the six query forms (`multipart_object_op` for the object
  verbs, an intercept for `GET /b?uploads`), matching **decoded** query keys so #491's
  percent-encoding bypass class cannot recur on the new routes; parses the Complete body with
  the same `roxmltree` gate DeleteObjects uses; string-builds the four XML response documents.
- **`crates/custodian/src/gc.rs`** — the GC hook: `referenced_fragments` now also scans
  `uploadpart:` records, so a live upload's staged chunks are treated as referenced (never
  reclaimed) exactly like committed inode chunks.

## Key design decisions and why

**Denylist removal + routing land atomically.** The `:813` guard's own doc-comment names the
destructive fall-through (`PUT ?partNumber` = UploadPart silently overwriting the whole object).
Routing is classified BEFORE the guard runs, so the three entries are removed *with* their
routes — there is never a window where an UploadPart falls through to a plain PUT.

**Staged-fragment GC safety (the subtle part).** A part's chunks are RS-encoded + committed at
UploadPart, but referenced only by the `uploadpart:` record until Complete folds them into an
inode (one CAS commit) or Abort orphans them. So `commit_part` **releases** each freshly-staged
chunk's pending lease in the same atomic batch — the chunks stop being leased garbage the instant
the `uploadpart:` record (now in GC's reference set) is durable. Transitions:
- *Complete* → one batch: put inode+dirent, delete session + all part records, orphan any
  omitted part's chunks (and, on overwrite, the prior object's). Atomic reference hand-off; no gap.
- *Abort* → one batch: delete session + all part records, orphan every staged chunk. GC reclaims
  after the reader-safe window.
- *Re-upload a part* → replace the record + orphan the prior chunks.
- *Crash mid-stage (before the part record commits)* → the chunks keep their pending leases →
  reclaimed by the existing pending-lease sweep (leased garbage), nothing published.

**`Gateway::run_gc` (co-located custodian sweep) — why a new method.** The success criterion's
abort-hygiene leg is observed through *"the FsChunkStore tempdir the test owns (fragment-file
count returns to its baseline)"* after *"the custodian's GC pass"*. The gateway's metadata store
is `RedbMetadataStore::in_memory` moved into `Gateway::new` — it is **not** shareable (redb takes
a single-writer lock; the in-memory backend cannot be reopened), so a test cannot hold a
`&dyn MetadataStore` to feed `custodian::reconcile_step` directly. `run_gc` is the smallest real
surface that closes this: it drives the **real** fenced `reconcile_step` over the gateway's own
stores. It is genuine production capability, not test scaffolding — in a single-node co-located
`wyrd s3` deployment there is no separate custodian process, so without an in-process sweep an
aborted multipart upload's fragments would leak forever. A single-authority `FsChunkStore` is
presented under the identity D-servers `0..n` the streaming write path places on, so the
placement-aware reference check matches each fragment against its placed id and orphaned staged
fragments are reclaimed. `ExpiredPendingPolicy::Defer` — reclamation rides the orphan ledger,
never a bare expired lease, so an in-flight upload's leased bytes are never swept.

**Part-size floor enforced (not weakened for the test).** `MIN_PART_SIZE = 5 MiB`, validated at
Complete (`EntityTooSmall`) for every non-final part. The test uses a real ≥5 MiB non-final part.
The chunk size is 1 MiB (not the peer's `with_chunk_size(8)`): 8-byte chunks × a 5 MiB part would
be ~5.9M fragment files — infeasible. 1 MiB still makes each 5 MiB part span several chunks (the
multi-chunk streaming path), at ~45 fragment files/part.

**Independent ETag oracle.** The test checks the part ETag and the composite `md5(concat(part-
md5s))-N` against RustCrypto **`md-5`** (a **dev**-dependency), never the server's own
`wyrd_core::md5` — so a shared MD5 bug cannot pass silently. `md-5` is already in the graph via
`aws-smithy-checksums` (the same `aws-sdk-s3` dev-dep), so `cargo deny` is unaffected (verified:
`cargo deny check licenses bans` → ok); it mirrors how `sha2` is the oracle in
`s3_object_metadata.rs`. This is NOT a production dependency (the multipart feature computes MD5
in-crate).

## Alternatives ruled out

- **Dedicated staging store / buffer parts in memory** — rejected by the brief (breaks the
  stream-don't-buffer invariant; re-encode-at-Complete is hours for large uploads). UploadPart
  reuses the existing streaming write path instead.
- **Adding `md-5` as a production dependency** — would be a NEEDS-HUMAN (ADR-0003 + deny). In-crate
  MD5 (~150 lines, RFC-1321-vector-tested) avoids it, honoring the brief's "No new dependency".
- **Sharing the redb store into the test to run GC via `reconcile_step` directly** — impossible
  (redb single-writer lock; in-memory backend not reopenable), and a blanket `impl MetadataStore
  for Arc<T>` in the seam crate is a broader change than `run_gc` and still would not compile on
  the base. `run_gc` keeps the change inside the brief-named crates.

## Chosen scope boundary (stated per brief)

**GC of abandoned-but-unaborted sessions is deferred.** A session whose `upload:{id}` record
survives (client vanished without Abort) keeps its `uploadpart:` chunks *referenced* by the GC
hook, so they are never reclaimed — correct (a live upload must not be swept) but it means an
abandoned session leaks until a multipart-expiry/lifecycle sweep reaps stale sessions. That sweep
is explicit follow-up (brief §Scope out-of-scope: "GC of ABANDONED-but-unaborted sessions beyond a
custodian sweep hook is follow-up"). Abort + the custodian GC pass (the shipped, tested path) do
leave no unreferenced staged fragments.

## Refuting my own test (forced)

- **(a) Genuine red?** Yes — verified by ASSERTION, not compile error. With multipart routing
  disabled but the scaffolding present (a temporary `PDCA_RED_EXPERIMENT` gate that reinstates the
  denylist and bypasses `multipart_object_op`), all four tests fail at the first wire call with
  `501 NotImplemented — the 'uploads' S3 subresource/operation is not supported` — exactly the
  brief's stated per-leg red reason. The gate was removed and the tree restored; the clean tree
  is green. (On the *true* wave base the file also cannot reference `Gateway::run_gc`; the
  routing-disabled experiment is the faithful "behavior missing" red — the abort leg reds at its
  first `create` 501 long before `run_gc`, which is downstream of a successful abort.)
- **(b) Production path?** Yes — the test drives the **shipping** composition (`Gateway<Redb,
  FsChunkStore, Mem>` behind the real `S3Gateway` listener) over a stock `aws-sdk-s3` client and
  raw HTTP; it imports no multipart production symbol. The GC leg calls the production
  `Gateway::run_gc` → real `custodian::reconcile_step` → real `gc::reconcile` (with the new hook).
- **(c) Fixture includes the fault?** Yes — the abort leg stages real fragments (asserts the
  on-disk `.frag` count rises above baseline), aborts, asserts they are *still present* after
  abort (orphaned, not eagerly deleted), THEN runs GC and asserts the count returns to baseline.
  The wrong-part leg feeds a genuinely non-matching ETag and asserts `InvalidPart` + a `NoSuchKey`
  GET (nothing published). Out-of-order upload (part 2 before part 1) is exercised, so a
  request-order assembly bug would corrupt the round trip.

## Verification performed

`cargo fmt --check` clean; `cargo clippy --workspace --all-targets` clean (`-D warnings`);
`cargo deny check licenses bans` ok. Tests green: the new `s3_multipart_upload` (4/4), core
lib (md5 + multipart units), full `wyrd-custodian` suite (gc/scrub/etc. — the shared
`referenced_fragments` change), all `server` S3 wire tests (`s3_delete_objects`, `s3_list_objects`,
`s3_copy_object_guard`, `s3_object_metadata`, `s3_head_object`, `s3_http_wire`,
`s3_streaming_trailer`, `request_capacity_planes`), and the gateway-s3 lib tests.

## Pre-declared deferred leg (off-Check, per brief §Verification posture)

The headline *"`aws s3 cp` of an 8+ GB file round-trips sha256-identical"* is observable only off
Check against a real deploy stack. The multipart machinery it exercises IS built and exercised at
Check by the SDK integration test (same verbs, same wire forms, smaller bodies). The maintainer
confirms the large-object leg by hand at sign-off or post-merge (record in §9) — no external
dependency was needed for the Check-level proof.
