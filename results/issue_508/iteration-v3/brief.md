# Design proposal — issue 508 / multipart-upload

> # ⛔ BLOCKED — DO NOT IMPLEMENT
> **This brief is NOT ready to build.** Four rounds of adversarial plan review concluded that
> the publication + lifecycle half of this slice needs a settled design first, filed as
> **getwyrd/wyrd#626** (draft proposal per ADR-0037). Known-unresolved defects in the design
> below include a terminal-state deadlock reachable with no crash (a concurrent PUT to the same
> key makes Complete's publish CAS lose, stranding the session in `Completing` with no verb able
> to leave it), undisposed published part records, and `pending:` residue that deployed GC will
> never reclaim.
> **If you are the builder: stop and report this line rather than implementing.** The design
> sections are retained deliberately as input to #626, not as an approved plan.
> The S3 **wire surface** half (routing, denylist removal, percent-encoding fence, exact
> status/error codes) IS sound and stable — that part survived all four rounds unrefuted.
>
> Plan artifact (design-proposal form; enhancement). Do reads ONLY this file.
> Field labels are parsed by the driver — keep the `- **Label:** value` shape.
> Iteration 3. Citations grounded on **origin/main @ cd82a29**. This brief survived two
> rounds of independent plan-stage review (`results/plan-review-{codex,adversary}-508*.md`);
> those reports record what was wrong with the earlier drafts and are worth reading before
> implementing.
> **Scope was SPLIT after round 2:** the abandoned-upload reaper is a FOLLOW-UP bundle, not
> this one — see Scope / out of scope and Open question 2.

- **Slug:** multipart-upload
- **Kind:** enhancement (design proposal)
- **Goal:** the full S3 multipart-upload verb set — CreateMultipartUpload, UploadPart,
  CompleteMultipartUpload, AbortMultipartUpload, ListParts, ListMultipartUploads — so objects
  >5 GB become possible and default clients (aws-cli/boto3 auto-switch above ~8 MB) can upload
  large objects at all. Today every multipart form is hard-refused 501 by the subresource
  denylist (`crates/gateway-s3/src/lib.rs:342-345`, guard at `:1702-1709`).
- **Success criterion:** three legs.
  **(A) Verb semantics and routing safety** — `crates/server/tests/s3_multipart_upload.rs`,
  stock `aws-sdk-s3` plus raw signed HTTP against the in-process loopback gateway. This file
  MUST import **zero** patch-added symbols (see Verification posture — it is the only leg that
  can earn a real C4 red). `create_multipart_upload` → `upload_part` × N (submitted OUT OF
  ORDER) → `complete_multipart_upload` succeeds, and a subsequent GET returns the object
  **byte-identical** to the parts concatenated in part-number order — with at least one
  non-final part that is **not a whole multiple of the chunk size** (e.g. 5 MiB + 7 B), so an
  assembler that assumes chunk alignment fails. UploadPart's per-part ETag is the part's
  lowercase-hex SHA-256; Complete's ETag is the lowercase-hex SHA-256 of the concatenated raw
  part digests suffixed **`-N`** (SHA-256, NOT MD5 — ADR-0047, see §Dependencies), asserted
  against independently-computed known answers. `list_parts` reflects staged parts with REAL
  pagination markers (`IsTruncated` computed). `abort_multipart_upload` ends the session, so
  subsequent UploadPart/Complete/ListParts answer **404 `NoSuchUpload`**; UploadPart against an
  unknown upload-id answers **404 `NoSuchUpload`** (this is what makes the session-existence
  precondition load-bearing). Every error clause asserts **exact status + S3 code**, never
  merely "an error" — otherwise the six forms already refused 501 on the base would be inert:
  wrong part list → **400 `InvalidPart`**/`InvalidPartOrder` without publishing; non-final part
  <5 MiB → **400 `EntityTooSmall`**; part number outside `1..=10000` → **400 `InvalidArgument`**;
  UploadPart body not matching the signed `x-amz-content-sha256` → the same refusal plain
  PutObject gives (`crates/gateway-s3/src/lib.rs:1768-1778`); `GET /bucket?uploads` on a
  non-existent bucket → **404 `NoSuchBucket`** (`:834-844`); Complete body not well-formed XML
  → **400 `MalformedXML`**; Complete body over the size cap → **400 `MalformedXML`**. Routing
  safety — each form below is pinned to an exact status+code (pick per form and assert it
  literally; never a 2xx that overwrites or deletes the object): an ill-formed multipart
  combination — `PUT /b/k?partNumber=1` (no uploadId), non-numeric partNumber,
  `PUT /b/k?uploadId=U` (no partNumber), `PUT /b/k?uploads`, `DELETE /b/k?uploadId=U`, and the
  percent-encoded `?part%4Eumber=1` — answers **400 `InvalidArgument`**; a multipart form
  carrying a still-denylisted subresource, in its **percent-encoded** spelling
  (`PUT /b/k?partNumber=1&uploadId=U&t%61gging=1`, `GET /b?uploads&%61cl`), answers **501
  `NotImplemented`** naming that subresource, exactly as `foreign_subresource_on_delete` does
  for `?delete` (`crates/gateway-s3/src/lib.rs:487-494,1630-1637`). Every one of these must be
  asserted as status **and** code: on the base most already answer 501, so an "any error" oracle
  would be inert for them (only `?part%4Eumber=1`, which answers 200 today, would red).
  **(B) Publication and storage accounting** — same file, over the wire:
  (i) **the lease hand-off completes** — after a part commits, **no `pending:` entry remains
  for that part's chunks** (scan the `pending:` prefix through the retained store handle; raw
  bytes, base-visible symbols only). This is the binding form of "Complete does not sit on the
  30 s lease TTL" (`crates/server/src/lib.rs:53`): if no pending entry survives a part commit,
  lease expiry cannot affect Complete **by construction**. Do NOT implement this as a >30 s
  sleep — that would need a `with_lease_ttl` seam, i.e. a patch-added symbol in leg A's file.
  (ii) **Complete over an existing object** publishes the new version AND orphans the prior
  version's chunks, so a pass past the grace window reclaims them (`write.rs:296-300`).
  (iii) **storage accounting is exact under the two commonest client behaviours** — a part
  number uploaded **twice**, and a Complete listing only a **subset** of the staged parts (S3
  discards the rest). After teardown and a pass past the grace window, **every fragment still
  present is one the published object's chunk map references, and every fragment it references
  is present**. Do NOT phrase this oracle as "equals a plain PUT of the same bytes": multipart
  chunking follows part boundaries, so a correct implementation with two misaligned parts
  legitimately yields a different chunk count than a single streamed PUT, and that oracle would
  fail a correct fix. Both behaviours are routine in `aws s3 cp` (it retries parts), and without
  this leg a builder can pass everything else while leaking every superseded and every unlisted
  part forever.
  (iv) **ListMultipartUploads actually lists** — with two uploads open on a bucket, the listing
  returns both (bucket, key and upload-id per entry), and after one is aborted it returns only
  the other. Without this, an always-empty listing satisfies every other clause — and this
  slice's mitigation for abandoned uploads (operator sees them and aborts them; see Scope) rests
  entirely on the listing being real.
  **(C) Staged-fragment lifecycle** — `crates/server/tests/s3_multipart_lifecycle.rs` (new; in
  the SERVER crate so it drives both the real wire and the real `reconcile_step`, as the
  retained draft does — the custodian crate has no dev-dependency on `wyrd-server` and adding
  one would invert ADR-0010). This file must also compile on the base (see Verification
  posture):
  (i) **a live upload is never harmed by maintenance** — with an upload open and parts staged,
  a GC pass reclaims none of its fragments AND a restore pass marks none stranded
  (`RestoreReport::stranded_marked == 0`, `crates/custodian/src/restore.rs:104-108`). The
  restore half is not optional: a fix that filters inside `gc::reconcile` instead of inside
  `referenced_fragments` passes the GC half while an operator restore pass strands a live
  upload's parts and the next GC pass deletes them — Complete then publishes an inode over
  missing bytes, the silent corruption `gc.rs:22-25` names.
  (ii) **teardown reclaims, and only what it should** — after `abort_multipart_upload`: no
  session or part record remains and a pass past the grace window returns the fragment count to
  its **pre-upload baseline**. After a **Complete** (stated separately — the arms differ): the
  **published object's fragments SURVIVE**, and every fragment not in the published chunk map
  is gone. Asserting the pre-upload baseline after a Complete would assert data loss.
  (iii) **a session-less part record is fail-safe** — seed/produce an `uploadpart:` record whose
  `upload:` session is absent (the abort-races-an-in-flight-part outcome); a custodian pass
  marks it stranded and a pass past the grace window reclaims its fragments **and deletes the
  `uploadpart:` record itself** — assert both, or an implementation can drop the fragments while
  retaining the record forever, leaving an unbounded metadata leak plus a dangling reference to
  missing chunks. Without this rule the staged set of §Publication makes such a record protect
  its chunks **forever** — iteration 2's leak in a new place. (The crash-mid-teardown case is
  handled by the resumable terminal-state path instead — §Publication item 4.)
- **Falsifiability:** RED is producible in-process on `origin/main` (cd82a29); no deploy stack,
  cluster or container needed.
  *Leg A reds on the base.* Every object-scoped multipart form is refused 501 by
  `unsupported_subresource` (`crates/gateway-s3/src/lib.rs:1702-1709`; tokens at `:343-345`)
  and bucket-scoped `GET /b?uploads` by `unsupported_subresource_decoded` (`:1655-1662`, whose
  comment at `:1652-1653` names 508's ListMultipartUploads red). Because leg A asserts **exact
  status + code**, the already-501 forms still red (501 ≠ the required 400/404). The
  `?part%4Eumber=1` form reds the other way: `unsupported_subresource` matches **raw** keys
  (`:398-404`, residual `:392-396`, issue #491) and SigV4 canonicalisation decodes-then-
  re-encodes (`crates/gateway-s3/src/sigv4.rs:245-264`), so on the base it reaches the plain PUT
  arm and answers **200, overwriting the object** — verified from source in both review rounds.
  *Legs B and C red only trivially* (their setup needs multipart, which 501s), so C4's aggregate
  red is earned by leg A and **cannot attribute** a red to B or C: `run-verify.sh` runs every
  added test in ONE cargo invocation and declares RED on the exit status
  (`engine/scripts/run-verify.sh:301-311,416-434`). Do not claim otherwise.
  **Gate hazard Do MUST actively defend against.** That failure branch has **no zero-test
  guard** (the `TESTS_RAN == 0` check exists only on the success branch, `:420-427`), so a
  **compile error** in either added test file makes cargo exit non-zero and the gate print
  "PASS — red without the fix" over a build that ran nothing. Therefore neither test file may
  depend on **anything the patch adds** — not just a patch-added Rust symbol but also any
  crate this patch adds to a `Cargo.toml`. Concretely: **the tests must not use `rand`**, which
  this slice adds to `crates/server/Cargo.toml` (§Dependencies); the red leg reverts that
  manifest, so a test importing it compiles to nothing and the gate reports a false PASS. Use
  fixed seeds/inputs instead. Do MUST record in build-notes.md, from the RED leg, the number of
  tests that actually ran and failed (proving the red is assertions, not a build error). If a
  needed observation is not reachable with base-visible symbols, seed raw bytes through the
  store handle as `seed_bucket` does (`crates/server/tests/s3_list_objects.rs:78-88`) rather
  than adding a seam. `MetadataStore::scan` is public on the base
  (`crates/traits/src/lib.rs:767-776`), which is what makes leg B(i)'s `pending:` scan
  observable without one.
  *Why the lifecycle legs discriminate.* GC does NOT reclaim everything unreferenced.
  `gc.rs:157-196` reclaims only on explicit **evidence**: an `orphan:` record past the
  reader-safe window, or an expired `pending:` lease; absent both it **conservatively retains**
  (`:183-186`). So on the base a committed staged part's chunks would be retained forever —
  unreferenced yet never reclaimed. That is the iteration-2 leak, and it is a property of the
  BASE. Consequence: **removing a fragment from the reference set does not reclaim it** —
  every release path must WRITE the evidence (`mark_orphaned`, `gc.rs:105-121`, documented as
  the extension point for "later slices").
- **Invariant to restore:** a staged multipart fragment is retained **if and only if it is
  reachable from a live upload session**. Both directions bind: (1) while its session is live, a
  staged fragment is never reclaimed and never marked stranded, by GC or any maintenance pass;
  (2) once it is no longer reachable — the part was superseded by a re-upload, not listed in a
  Complete, aborted, or its session vanished — the fragment is **actually reclaimed within a
  bounded window**, not merely left unprotected. Direction (2) is NOT satisfied by removing
  protection: on this base an unprotected, unreferenced fragment is retained forever for want of
  reclaim evidence (`crates/custodian/src/gc.rs:183-186`). A staged-part record must never
  confer retention independently of its session. Neither direction may be traded for the other —
  retaining unconditionally leaks forever; reclaiming eagerly destroys a live upload.
  **Source (in-tree, authoritative):** the custodian's written safety invariant "never reclaim a
  referenced fragment" (`gc.rs:23,129-132,159-166`) with its reclaim mandate at `:15`; the
  reference set is defined over committed records only (`:217-249`). Provenance
  (`docs/principles.md` §7): this project's catalogue (§5) and category map (§6) are **unfilled
  scaffolds**, so this is not a §6-mapped category — sourced directly from the target repo's
  written rule per §4.1/4.3. It is a **structural / object-lifetime** change, so §1.2 applies:
  the smallest change that RESTORES the invariant, not the smallest diff.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:**
- **Conflicts with:**
- **Ordering note:** SINGLE bundle, no wave, no dependency edges — a deliberate change from
  iteration 2, which declared `Depends on: 507, 509, 510` and was built on a base lacking 510
  (rejection item 8). All three are **merged and closed on `main`**: 507 via PR #609, 510 via
  PR #611, 509 via PR #612 (iteration-2's sign-off said "PR #610" for 510; the merge commit in
  `main` is #611 — corrected deliberately, not silently). #503 (ADR-0047) is merged.
  `pdca-integration/main` (fea4119) is **35 commits BEHIND** `origin/main` and MUST NOT be a
  base. **A stale-base trap was found and REMOVED during planning:** the bundle carried a
  `stack-base` marker naming `pdca-integration/main`; `gates.py:358-360` turns it into
  `PDCA_VERIFY_BASE`, which OUTRANKS the brief's base (`engine/scripts/run-verify.sh:182-192`)
  — it would have verified against the 35-commits-behind branch and reproduced rejection item 8
  exactly. `pdca flow` would have cleared it (`_point_at_integration`, `flow.py:448-461`, called
  at `:637`, #187) but a bare `pdca run`/`pdca gates` would not, so the file was deleted. If
  anything re-creates `results/issue_508/stack-base`, delete it before gating.
- **Surfaces:** data
- **Difficulty:** high
- **Scope:** the six multipart verbs, their state, their publication path and their
  staged-fragment lifecycle, end-to-end — take `uploads`/`uploadId`/`partNumber` off the
  object-path denylist and serve the query forms (`POST /b/k?uploads`,
  `PUT /b/k?partNumber=N&uploadId=U`, `POST /b/k?uploadId=U`, `DELETE /b/k?uploadId=U`,
  `GET /b/k?uploadId=U`, `GET /b?uploads`) as real operations classified over DECODED query
  keys; a protocol-neutral multipart surface on the gateway-core seam (no S3 vocabulary crosses
  it); upload-session and staged-part state in the `MetadataStore` under new disjoint prefixes;
  UploadPart staged over the existing streaming write path, preconditioned on a live session,
  wired to the FULL payload dispatch (a stock SDK sends parts `aws-chunked`, so the streaming
  and signed and unsigned arms must all be handled, mirroring `crates/gateway-s3/src/lib.rs:1747-1785`);
  a publication path for Complete that does not sit on the pending-lease TTL (Design
  §Publication); Abort; ListParts and ListMultipartUploads with real pagination; and the
  staged-fragment lifecycle stated in `Invariant to restore`, including the custodian's part in
  it. A multipart form that does not classify into a valid operation is refused, never served
  by a plain object verb.
  / out of scope: **the abandoned-upload reaper** — detecting an upload the client silently
  discontinued is **issue #625**, a follow-up bundle (Open question 2); this slice reclaims on
  the explicit paths (abort, complete, supersede) and ships ListMultipartUploads +
  AbortMultipartUpload so an operator can see and clear the rest, which is S3's own default
  posture (S3 auto-aborts nothing without a lifecycle rule). Also out: fixing #491 for the REMAINING denylist entries (separate
  open issue — just do not extend the bug to the new routes); `x-amz-copy-source` UploadPartCopy;
  per-part `Content-MD5` / `x-amz-checksum-*` trailers beyond what the streaming path verifies;
  S3 bucket lifecycle configuration as a client-facing API; and **any ADR or spec file**
  (`docs/design/adr/`, `docs/design/specs/`) — Open question 1.
- **External dependencies:** `aws cli (S3 gateway round-trip)`
  (the only token on this line; registered as a doctor.checks row of that exact id in
  pdca.toml) — needed ONLY for the off-Check large-object leg, see Verification posture. That
  leg also needs a running deploy stack and ≥8 GB of free disk (topology/environment shape, no
  detect command — exempt). Everything BINDING at Check needs nothing beyond the base
  toolchain: all legs run in-process over an in-memory redb store, an FsChunkStore tempdir and
  MemCoordination. **No new third-party crate enters the production graph** — see Design
  §Dependencies: roxmltree is already wired, the ETag reuses the already-streamed SHA-256
  (ADR-0047 rejected MD5), and rand is an existing workspace crate becoming a direct dependency
  of crates/server. So no ADR-0003 audit and no dependency sign-off item.
- **Test file:** `crates/server/tests/s3_multipart_upload.rs` (legs A and B) **and**
  `crates/server/tests/s3_multipart_lifecycle.rs` (leg C). Both are **NEW**
  `crates/<c>/tests/<t>.rs` files, which this project's C4 gate requires: `run-verify.sh`
  discriminates on an **added** test file (`_is_test_file`, `engine/scripts/run-verify.sh:90-93`)
  and degrades to a green-only, proves-nothing branch for a co-located test (`:397`). A
  `--classify` dry-run on a synthetic patch of the expected file set returns `ADDED_TEST` for
  both. Do MUST NOT append these legs to an existing suite, and **both files must compile
  against the base with production reverted** (see the gate hazard in Falsifiability). They are
  separate binaries deliberately: leg C drives the custodian loops and must not share one
  process's `tracing` callsite cache with leg A (the issue-#214 rule the custodian suite already
  follows, `crates/custodian/tests/gc.rs:20-23`).
- **Verification posture:** mixed; stated per leg.
  **Flippable red→green at Check:** leg A — it reds on `origin/main` and is the binding
  red→green evidence.
  **Trivially-red at Check (setup needs multipart):** legs B and C. C4 cannot attribute its
  aggregate red to them, so their force must be evidenced by **negation runs Do performs and
  records in build-notes.md** — for each, the deliberately-broken variant and the leg that
  caught it: (1) the staged set NOT consulted in `protects` ⇒ leg C(i)'s **restore** half fails
  (note: the GC half cannot discriminate this — `gc.rs:183-186` retains an unevidenced fragment
  either way — so do not claim it does); (2) a filter inside `gc::reconcile` instead of the
  `protects` gate ⇒ leg C(i) restore half fails; (3) release paths WITHOUT `mark_orphaned` ⇒
  legs C(ii) and B(iii) fail (this is iteration 2's shipped shape); (4) a session-less part
  record treated as protected ⇒ leg C(iii) fails; (5) part commit that WRITES the session
  instead of merely requiring it ⇒ concurrent part uploads `Conflict` (drive ≥4 parts
  concurrently in the negation run; leg A is sequential and will NOT catch this — it is the
  headline `aws s3 cp` shape, so evidence it deliberately); (6) publication through the
  lease-conditional primitive ⇒ leg B(i) fails; (7) Complete WITHOUT the phase-1 fence ⇒ a part
  committing after Complete read the part set is silently dropped. A
  negation that does NOT fail its leg means the leg is inert — say so rather than reporting a
  pass. Also record the RED-leg test counts (Falsifiability, gate hazard).
  One pre-declared **deferred** leg: the issue's headline acceptance "`aws s3 cp` of an 8+ GB
  file round-trips sha256-identical" is observable only off-Check against a deployed stack. The
  machinery it exercises IS built and exercised at Check by legs A–C (same verbs, same wire
  forms, smaller bodies) — nothing is deferred-because-unbuilt. **Eduard Ralph confirms the
  large-object leg by hand at sign-off or post-merge and records it in SUMMARY §9.**
- **Citations expected:** Do must cite `path:line` on `origin/main` for every change. Peer
  callsites Do MAY open and should mirror:
  **routing** — `foreign_subresource_on_delete` (`crates/gateway-s3/src/lib.rs:487-494`) with
  its interception site (`:1630-1646`) is 509's solved form of this exact problem: intercept a
  route BEFORE the denylist without opening a hole in the fence for every other key. Decode keys
  before matching, exempt only the multipart markers, keep every other denylisted key refusing.
  Object-path guard and its destructive-fall-through rationale `:1696-1709`; the "refuse a form
  we do not implement rather than mishandle it" precedent `:1725-1732`.
  **payload dispatch** — `crates/gateway-s3/src/lib.rs:1747-1785` (streaming / signed /
  unsigned arms; `PayloadHash::Signed(hex)` → `ContentHash::Expected(hex)`,
  `crates/gateway-core/src/lib.rs:245-251`).
  **bucket existence** — `list_objects` 404s `NoSuchBucket` off `list_container` returning
  `Ok(None)` (`:834-844`; seam `crates/gateway-core/src/lib.rs:424`, impl
  `crates/server/src/lib.rs:590`).
  **streaming write + the digest to reuse** — seam `crates/gateway-core/src/lib.rs:300`, impl
  `crates/server/src/lib.rs:287`, stream-don't-buffer invariant `:276-286`, and the SHA-256 the
  path already streams via `HashingSource` `:297-317,649-663`.
  **publication (read Design §Publication FIRST — these are not composable as-is)** —
  `create` (`crates/core/src/metadata.rs:366`) and `commit_chunk_map_superseding` (`:582`) each
  build and commit a PRIVATE `WriteBatch` (`:373-382`, `:604-619`) and accept no extra
  precondition; their lease-conditional siblings `create_leased` (`:398`) and
  `commit_chunk_map_superseding_leased` (`:638`) show the established "same function + extra
  guards" pattern; the guard being replaced is `live_lease_guards` (`:763-793`); the
  superseding-orphan obligation is `crates/core/src/write.rs:296-300`.
  **XML** — parse Complete's body with `roxmltree`, ALREADY a dependency
  (`crates/gateway-s3/Cargo.toml:38`, workspace `Cargo.toml:139`); size the body cap on
  `MAX_DELETE_BODY_BYTES`'s worked pattern incl. its compile-time assert
  (`crates/gateway-s3/src/lib.rs:421-457`).
  **custodian** — reference set + safety gate `crates/custodian/src/gc.rs:128-196,217-255`;
  `mark_orphaned` `:105-121`; `ExpiredPendingPolicy`'s cross-clock reasoning `:78-105`; restore
  pass `crates/custodian/src/restore.rs:104-108,179` and its **bounded-batch precedent**
  `MARK_BATCH = 1_000` with the FoundationDB transaction-limit rationale `:92-100`; harness +
  the `Flippable:` convention `crates/custodian/tests/gc.rs:14-23,37,137,255`.
  **test harness** — `sdk_client` (`crates/server/tests/s3_gateway_cluster.rs:96-110`);
  **`seed_bucket`** (`crates/server/tests/s3_list_objects.rs:78-88`) — a `bucket:` marker must
  be seeded as raw bytes BEFORE the store moves into `Gateway::new`, or every listing leg 404s;
  nothing on `main` creates bucket records (#511). The retained iteration-2 draft
  `results/issue_508/s3_multipart_upload.rs` has working `run_gc_pass` / `run_restore_pass`
  helpers over base-visible symbols.
  **record design** — ADR-0046 §Decision 1-2 (`docs/design/adr/0046-…:63-77`); ADR-0047 for the
  ETag/inode model incl. the `skip_serializing_if` decode→encode identity rule that keeps
  full-value CAS working.
  **seam neutrality** — `crates/gateway-core/src/lib.rs:1-22`.
- **Prior-art check (triage cycles):** searched by affected file path across merged history and
  closed/rejected work. `crates/gateway-s3/src/lib.rs` — last touched by #509 (PR #612), #510
  (PR #611), #507 (PR #609), all merged; none implements a multipart verb and all three
  deliberately PRESERVE the multipart denylist (the `:1652-1653` comment names 508).
  `crates/custodian/src/gc.rs`, `restore.rs`, `crates/core/src/metadata.rs` — no
  multipart-related change in history. No `crates/core/src/multipart.rs`, no
  `crates/server/tests/s3_multipart_upload.rs`, no `crates/server/tests/s3_multipart_lifecycle.rs`
  on `main`. No open or closed PR implements multipart upload. #491 (percent-encoded denylist
  bypass) remains OPEN, out of scope except for the new routes. Result: **no prior art; net-new.**
- **Disposition hint:** likely-fix

## Motivation

The single biggest gap in the S3 surface. `PutObject`'s 5 GB ceiling makes larger objects
impossible, and stock clients auto-switch to multipart above ~8 MB, so ordinary large uploads
fail today with a 501. Core to the 0.1-Alpha S3 completion epic (milestone 16).

## Design

### Record format

Multipart state lives in the **metadata keyspace** (ADR-0046's record pattern): prefixes
disjoint from `inode:`/`dirent:`/`pending:`/`orphan:`/`bucket:`, JSON-encoded like existing
records. Keys are collision-safe by construction — bucket and key hold arbitrary bytes including
any delimiter, so the server-minted upload-id is the sole scan/lookup component:

- `upload:{upload-id}` — the session: bucket, key, declared content-type, and a **lifecycle
  state** (§Publication item 2). It carries **no per-part counter**: part commits deliberately
  do not write this record, so that concurrent part uploads do not serialise (§Publication
  item 2). The only writers are the lifecycle transitions. Bucket/key live INSIDE the JSON,
  never in the key.
- `uploadpart:{upload-id}:{part-number}` — a staged part's chunk map, **raw SHA-256 digest**
  (the part ETag's basis — not MD5) and size, part number zero-padded to fixed width so scan
  order is numeric.

The upload-id is server-minted and MUST match a strict grammar (32 lowercase hex). That is
load-bearing: a client-supplied id containing `:` would make one session's `uploadpart:{id}:`
scan prefix a prefix of another's. Every part-key path first resolves a live session, and
UploadPart preconditions on the session record existing (what leg A's `NoSuchUpload` clause
pins).

### Publication — variant (b), decided

**The problem.** Every existing publication path is lease-conditional (#490): `live_lease_guards`
(`crates/core/src/metadata.rs:763-793`) refuses if any chunk's `pending:` lease is absent or
lapsed, and the TTL is **30 s** (`crates/server/src/lib.rs:53`), renewed only within one
`stream_write_data` call. Multipart is inherently long-lived, so publishing through those
primitives makes Complete `Conflict` for any upload older than 30 s — and **always** for the
8 GB case. Hand-rolling a batch instead would silently drop both the #490 guard and the
superseding-orphan step.

**The decision.** A *committed* staged part is durable, referenced state — not the "leased
garbage a crashed write leaves" that `pending:` models (`gc.rs:11-13`). Therefore:

1. **Protection moves from lease to a SEPARATE staged set — do NOT widen the committed
   reference set.** `referenced_fragments` (`gc.rs:251-255`) has four consumers, not one:
   GC's safety gate, scrub, reconstruction, and drain/rebalance's desired-state check
   (`crates/custodian/src/desired_state.rs:157-163` vs `rebalance.rs:147`, which are
   inode-only). Widening it would make a drain **never** reach `Satisfied` — unbounded, with no
   reaper in this slice to end it — and re-open the scrub→reconstruction `Drain` hole
   (`reconstruction.rs:186-191,320-323`). Instead build a **separate staged set** from
   `uploadpart:` records and consult it **only** in GC's `protects` safety gate
   (`gc.rs:162,244`), leaving scrub/reconstruction/drain semantics untouched. State in
   build-notes which consumers you audited.
2. **Session state machine — this is what makes concurrent parts safe AND Complete stable.**
   The session record carries a state: `Open` → `Completing` | `Aborting`.
   **Part commit is ONE batch, and it does NOT write the session:**
   `require(upload:{id}, exact Open-state bytes)` + `require(pending entries still live)` +
   `put(uploadpart record)` + `delete(pending keys)` + `put(orphan:…)` for every fragment of the
   part record this one supersedes (a re-upload of the same number).
   The session term is a **compare-only precondition**, never a mutation — that is
   load-bearing: stock clients upload parts **concurrently** (`aws s3 cp` and boto3 default to
   ~10 in flight), and a design that made every part commit CAS-and-write the session would
   serialise them and `Conflict` all but one. Leg A is sequential and would NOT catch that; only
   the deferred 8 GB leg would. Concurrent batches that merely `require` the same unchanged
   session bytes do not conflict with each other. Omitting the orphan terms leaks every retried
   part forever, since removing a fragment from the protected set does not reclaim it
   (`gc.rs:183-186`). The pending→staged hand-off must be atomic in this batch, or there is a
   window where neither lease nor staged set protects the chunks.
3. **Complete is phased; Abort mirrors it.**
   *Phase 1 — fence.* CAS the session `Open` → `Completing` in one small batch. This is what
   stops further part commits (their `require(Open-state bytes)` now fails) and gives Complete a
   stable view of the part set. Complete reads the part records **after** this fence.
   *Phase 2 — publish, atomically.* One batch: inode create/CAS + the prior object version's
   orphan records + `require(upload:{id}, Completing-state bytes)`. The superseding primitive's
   orphan semantics are exactly right (`commit_chunk_map_superseding` CASes the prior inode,
   publishes the new map and orphans all prior fragments in one batch, `metadata.rs:604-619`),
   but it is **not composable as it stands**: it and `create` build and commit a private
   `WriteBatch` and accept no extra precondition (`:373-382`, `:604-619`). This slice must make
   them composable — refactor each into a batch-*building* helper the caller commits (keeping
   the existing function as a thin wrapper), or add a guarded variant taking extra
   preconditions, following the established `create_leased` /
   `commit_chunk_map_superseding_leased` pattern (`:398`, `:638`). Do MUST NOT call the helper
   and then separately CAS the session — that ordering gap is the race this design closes.
   *Phase 3 — bounded cleanup.* Orphan and delete the discarded part records (superseded, and
   any staged part **not** in the published list — S3 discards those) in bounded batches, each
   preconditioned on the session still being in its terminal state, each batch pairing a part's
   orphan records with that part record's deletion. **Delete the session LAST.** Bound the
   batches per the `MARK_BATCH = 1_000` precedent and its FoundationDB transaction-limit
   rationale (`crates/custodian/src/restore.rs:92-100`).
   This preserves #490's *obligation* (never publish over fragments GC may reclaim) by
   substituting a non-expiring protection and an equivalent fail-closed precondition; a reviewer
   will read "unconditional primitive" as "unguarded", so say this explicitly in build-notes.
4. **A terminal-state session is resumable; a session-less part record is the backstop.** An
   interrupted teardown leaves the session **present** in `Completing`/`Aborting` with some
   parts still to clean — a custodian pass resumes it (that is why the session is deleted last,
   and why UploadPart requires `Open` specifically, so it can never revive a terminal upload).
   Independently, any `uploadpart:` record whose `upload:` session is **absent** is marked and
   reclaimed, **record included** — the fail-safe for an abort racing an in-flight part. Both
   paths are needed: the first handles crash-mid-teardown, the second handles the race. Leg
   C(iii) binds the second; state in build-notes how the first is driven.
5. **Bound the fan-out this slice ADDS.** A single 5 GiB part can carry tens of thousands of
   fragments, so the superseded-part orphan writes in item 2 must themselves be bounded — if a
   supersede would exceed the batch bound, stage it through the same terminal-state cleanup path
   rather than one oversized transaction. *Out of scope:* the **pre-existing** prior-object
   orphan fan-out inside `commit_chunk_map_superseding` (one put per prior fragment,
   `metadata.rs:604-619`) is how every overwrite already behaves on `main`; this slice must not
   make it worse, but redesigning it is a separate issue — note it in build-notes as a
   follow-up candidate rather than expanding this bundle.

The only clock on this path is the custodian's own orphan-grace window, which is already
single-owner (`gc.rs:172-179`); no gateway-stamped time is ever compared against it.

### Dependencies

- **`roxmltree` for Complete's XML body — already wired.** A dependency of `wyrd-gateway-s3`
  (`crates/gateway-s3/Cargo.toml:38`, workspace `Cargo.toml:139`), adopted through the ADR-0003
  §2 three-test audit for #509 with the explicit rationale that "the gateway writes NO
  hand-rolled well-formedness pass on that destructive path". Do MUST NOT hand-roll a parser —
  that reverses a just-settled decision and re-opens the class #509 was rejected five times
  over. Pair it with a `MAX_COMPLETE_BODY_BYTES` cap enforced **as the body is read** (never by
  trusting `Content-Length`), sized on the worst-case *encoded* 10 000-part list and carrying a
  compile-time assert, mirroring `MAX_DELETE_BODY_BYTES` (`crates/gateway-s3/src/lib.rs:421-457`).
  Without a cap a 1 GiB Complete body OOMs the gateway — the `0015:789` cliff.
- **NO MD5 — the composite ETag is SHA-256-based, per ADR-0047.** Not the planner's call:
  ADR-0047 (**Accepted**, therefore immutable — ADR-0001, INTEGRATION §2) decided the ETag basis
  and explicitly rejected MD5 — "requires a new MD5 dependency (ADR-0003 audit + `deny.toml`)
  and a second digest over every streamed byte; rejected — SHA-256-as-opaque-token is spec-legal
  and already streamed" — and deferred only the **composition** to this slice. So the per-part
  ETag is the part's lowercase-hex SHA-256, taken from the digest the write path ALREADY streams
  (`HashingSource`, `crates/server/src/lib.rs:297-317,649-663`) with no second pass; Complete's
  is the lowercase-hex SHA-256 of the concatenated raw part digests suffixed **`-N`** — the
  suffix is what carries S3 semantics, since it is how clients detect a multipart object.
  **Interop cost is zero:** a client reconstructing `md5-of-md5s` already mismatches this gateway
  on every simple PUT, and S3 documents ETag as not-guaranteed-MD5 *specifically for multipart
  objects*. Do MUST NOT add `md-5` or hand-roll MD5; that would reverse an Accepted ADR and needs
  a superseding ADR (out of scope). If Do believes MD5 is necessary, STOP and raise it.
  **One production doc-contract must widen:** `ObjectMeta::etag` is documented as "the
  lowercase-hex SHA-256 of the **object bytes**" (`crates/core/src/metadata.rs:252-253`); a
  multipart composite is not that. Update that doc-comment to admit the multipart composite form
  — a deliberate, reviewable change, not a silent redefinition.
- **`rand` for upload-ids.** `rand = "0.10"` is a workspace dependency (`Cargo.toml:127`) used by
  `crates/dst`/`crates/testkit`, so no new package/version/licence enters the lockfile — but
  `wyrd-server` does not currently depend on it, so this adds `rand.workspace = true` to
  `crates/server/Cargo.toml`, a new **direct** dependency for that crate. Say so.

### Upload ids

ONE binding contract: unguessable and collision-free across concurrent sessions and process
restarts, with a fresh cryptographic draw **per id**, and `require_absent` at mint so a collision
loses rather than cross-wiring two uploads. Documenting a weaker guarantee does not satisfy it.
`std::collections::hash_map::RandomState` does not meet it at per-request rates: it draws OS
entropy once per thread and derives later constructions from a per-thread counter, so successive
ids on one worker are a deterministic SipHash stream from one seed — why iteration 2's "128 bits
of OS entropy" claim was rejected. `random_chunk_epoch` (`crates/server/src/lib.rs:244-263`) uses
`RandomState` legitimately (once per gateway, for range disjointness) — a precedent for honest
scoping, not a source to copy. DST: ADR-0035 already records the `Gateway` id allocators as a
**latent hazard**, "deterministic today only because `Gateway` is not yet exercised under DST"
(`0035:124-127`, remedy `:152`); a new per-request entropy source extends that recorded hazard —
note it in build-notes under the ADR-0035 entry rather than adding a second unregistered one.

### Streaming, assembly and the part-size floor

UploadPart streams through the existing write path (RS-encode as bytes arrive — never buffer a
part, `crates/server/src/lib.rs:276-286`), taking the part's SHA-256 from the digest that path
already streams, and carries the signed-payload integrity check plain PutObject enforces
(`crates/gateway-s3/src/lib.rs:1768-1778`). Complete validates the part list (numbers ascending
and present, ETags matching, every non-final part ≥5 MiB → `EntityTooSmall`) and assembles in
part-number order. **Parts are not chunk-aligned in general**: the read path walks each
`ChunkRef`'s own `len` (`crates/core/src/read.rs:92-95`), so the assembled chunk map must carry
true per-chunk lengths — never `size = chunk_count × chunk_size`. Leg A's non-multiple part
catches exactly that.

### Harness sizing

`DEFAULT_CHUNK_SIZE` is **already 1 MiB** (`crates/server/src/lib.rs:51`), so no explicit
`with_chunk_size` call is needed — a 5 MiB part spans 5 chunks over the streaming path and a
two-part ~10 MiB upload stays fast on in-memory redb + an fs tempdir. Include one non-final part
that is not a whole multiple (e.g. 5 MiB + 7 B). Do NOT use the 8-byte chunk size the
small-object harnesses use — it would explode a 5 MiB part into ~650k chunks.

## Alternatives considered

- **Publish through the lease-conditional primitives (variant a)** — re-lease every part chunk
  immediately before publishing. Unless the re-lease is atomic with the publish it reopens the
  TOCTOU window it closes, and at 10 000-part scale it is a large operation gated on a 30 s
  clock. Rejected: keeps a timer on an inherently long-lived operation.
- **Raise the lease TTL for multipart** — does not scale to an 8 GB upload and makes the TTL a
  correctness parameter. Rejected.
- **A true S3 `md5-of-part-md5s` ETag** (what the issue text asks for) — needs an MD5 dependency
  or a hand-rolled digest plus a second pass over every streamed byte, and reverses ADR-0047's
  accepted decision, requiring a superseding ADR. Rejected: buys compatibility only with clients
  that violate ETag opacity, and those are already broken against this gateway's simple PUTs.
  Recorded so the divergence from the issue text is deliberate and visible at sign-off.
- **A dedicated staging store** (spool raw parts, RS-encode at Complete) — Complete would re-read
  and re-encode the whole object (hours for a large upload), needs its own crash-recovery and GC,
  and breaks stream-don't-buffer at the seam. Rejected.
- **Buffering parts in memory** — violates the `0015:789` OOM-cliff invariant. Rejected.
- **`uploadpart:` records unconditionally referenced with no teardown evidence** (iteration 2's
  shipped hook) — satisfies "never destroy a live upload" and violates "never leak". Rejected by
  the invariant.

## Impact & compatibility

Three denylist entries become live routes; the guard's protective purpose is preserved by real
routing plus fail-closed refusal of unclassifiable forms. New record prefixes in the metadata
keyspace — no on-disk chunk-format change, conformance vectors untouched. New trait surface on
the gateway-core seam (in-tree implementers updated in-change). **Two changes deserve explicit
reviewer attention, and must be called out in build-notes and the PR description:** (1) a new
**staged protection set** is consulted in GC's `protects` gate (`gc.rs:162,244`) — deliberately
NOT widening `referenced_fragments`, whose other consumers (scrub, reconstruction, and
drain/rebalance's desired-state check, `desired_state.rs:157-163` vs `rebalance.rs:147`) would
change semantics if it grew; name the consumers you audited and why each is unaffected.
(2) the publication primitives become composable (batch-builders or guarded variants), touching
every existing caller — a compiler-driven refactor whose behaviour must not change. **No new production dependency**: the ETag reuses the already-streamed SHA-256,
`roxmltree` is wired, `rand` is an existing workspace crate becoming a direct dependency of
`crates/server`. Objects written before this change are unaffected; a client that assumed
`ETag == MD5` is already incompatible with this gateway's simple PUTs, so multipart adds no new
incompatibility.

## Open questions

- **ADR: settled — no ADR in this patch.** Maintainer decision (option a): this proposal is the
  design record for the record prefixes, the publication change and the lifecycle; capturing it
  as an ADR is a **follow-up issue**. Do MUST NOT add or edit anything under `docs/design/adr/`.
  The reviewer's recurring T5 NEEDS-HUMAN on this is pre-answered — record it in §9 and open the
  follow-up.
- **The reaper is a follow-up bundle — FILED as getwyrd/wyrd#625** (enhancement, milestone
  0.1 Alpha; it carries the design constraints below so they are not re-derived). Detecting a
  silently discontinued upload was split out of this slice after round-2 review. The reason is the C4
  shape: in any natural design it adds custodian configuration surface (a `GcContext` field or a
  `reconcile_step` parameter — `gc.rs:65-75`, `reconciliation.rs:65-73`), which makes its test
  file **compile-red** on the base; and `run-verify.sh`'s red leg has no zero-test guard on its
  failure branch (`:416-434`), so that compile error would make C4 print PASS over a build that
  ran nothing, masking leg A. (A fixed const or raw-metadata config could avoid new API in
  principle — so "impossible" would overstate it; the point is that the natural shapes all carry
  this risk, and the reaper also brings its own hard sub-problems, below.) It therefore needs its
  own bundle where the C4 shape can be designed for it deliberately.
  The follow-up must cover: progress-based (not age-based) detection — comparing a
  gateway-stamped timestamp against the custodian's clock reproduces the #557/#565 cross-clock
  class (`gc.rs:78-105`, AGENTS.md "one clock per correctness lifecycle"); the case a single
  in-flight `UploadPart` streams longer than the window (session version advances only at part
  commit, so naive progress-detection can reap a live upload mid-stream — a conservative
  "skip any session with a live `pending:` entry" presence check avoids a clock comparison);
  watch-record lifecycle and deletion; and crash-safe bounded teardown. Until it lands, silently
  abandoned uploads pin storage — visible via ListMultipartUploads and clearable via
  AbortMultipartUpload, which is S3's own default posture.
- **ETag basis is NOT open** — ADR-0047 decided it (SHA-256, MD5 rejected); this slice supplies
  only the composition. Recorded because the issue text asks for `md5-of-part-md5s`: that
  phrasing predates ADR-0047 and is superseded by it. True S3 MD5 ETags would be a superseding
  ADR plus a dependency audit — a separate work item, not a change Do can make.

## Prior iterations — what was rejected, and what must not be repeated

Attempts 1 and 2 are preserved in `iteration-v1/` and `iteration-v2/`. Iteration 2 passed
C1/C3/C5 and T1–T3 and its adversary could not refute its red→green; it was rejected on §6
items 4-8. This brief closes each:

1. **Stale base** (item 8) — closed by retargeting: 507/509/510 are merged on `main`, no
   dependency edges, and the stale `stack-base` marker was deleted.
2. **GC design not implemented** (item 5) — the sweep was prescribed in prose and nothing bound
   it, so the shipped hook swept nothing. Closed by the `Invariant to restore` field, legs
   B(iii)/C, the required negation runs, and by naming the actual mechanism gap: removing a
   fragment from the reference set does not reclaim it; every release path must WRITE
   `mark_orphaned` evidence.
3. **Overstated entropy claim** (item 6) — closed by Design §Upload ids (one binding contract).
4. **`GET /bucket?uploads` on an unknown bucket answered 200** (item 7) — closed by the 404
   `NoSuchBucket` clause; the mixed-subresource case is specified as an ambiguous-combination
   rule (NOT a denylist case — `partNumber` is removed from the denylist by this slice) and
   tested in its **percent-encoded** form so a raw-key check cannot satisfy it.
5. **Provisional C4** (item 1) — the full `cargo xtask ci`, including `cargo deny` against a
   **writable** advisory database, reruns at the next Check; the provisional pass is NOT carried.
   If the host advisory DB is read-only again that is a Check-environment item for the human (and
   an Act candidate for a `[[doctor.checks]]` row), not a pass.
6. **Large-object validation** (item 4) — remains the human's off-Check leg, pre-declared in
   Verification posture and recorded in §9.

Iteration 1's rejections stay closed: no `typos` misspellings in changed text; leg A fails by
ASSERTION and **neither** test file references a patch-added symbol; malformed multipart queries
never fall through destructively; UploadPart enforces the signed-payload hash; part numbers
validated `1..=10000`.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.
