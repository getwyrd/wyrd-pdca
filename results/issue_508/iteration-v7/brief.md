# Design proposal — issue 508 / multipart-upload

> Plan artifact (design-proposal form; enhancement). Do reads ONLY this file.
> The `- **Label:** value` lines are parsed by the driver — keep their shape.
>
> **Iteration 4.** Rev 3 (`iteration-v3/brief.md`) was BLOCKED because the publication +
> lifecycle half had no settled design. That design now exists and is **normative for this
> bundle**: proposal **0016 — the multipart commit protocol**,
> `docs/design/proposals/draft/0016-multipart-commit-protocol.md` **as it stands on
> `origin/main` @ `22d71b4`** (3,108 lines; content tip `c35d39d`, merged by PR #627, tracking
> issue #626, `status: draft`). **Read the file in the checkout, never a commit.** `97e2392` is
> only the *initial* 2,194-line draft; ten review commits followed it and they carry load-bearing
> rules (`B_ops` and its clamps, the independent segment-group reservation). A builder who reads
> `97e2392` reads a superseded protocol.
> **Do MUST read 0016 before writing code.** This brief does not restate its seven decisions;
> it cites them, fixes the S3 wire surface 0016 deliberately leaves open, and states what the
> tests pin. Where this brief and 0016 disagree on anything underneath `CompleteMultipartUpload`,
> **0016 wins** — report the conflict rather than choosing.
>
> All `path:line` citations below were re-verified against `origin/main` @ `22d71b4`
> (the merge of #627) on 2026-07-25. Rev 3's citations were pinned to `cd82a29` and had
> drifted; do not copy them from the archive without re-checking.

- **Slug:** multipart-upload
- **Kind:** enhancement (design proposal)
- **Goal:** the full S3 multipart-upload verb set — `CreateMultipartUpload`, `UploadPart`,
  `CompleteMultipartUpload`, `AbortMultipartUpload`, `ListParts`, `ListMultipartUploads` — over
  the commit protocol proposal 0016 settles, so objects >5 GB become possible and stock clients
  (aws-cli / boto3 auto-switch above ~8 MB) can upload large objects at all. Today every
  multipart form **in its raw spelling** is hard-refused **501** by the subresource denylist
  (`crates/gateway-s3/src/lib.rs:342-345`, object guard `:1702-1709`, bucket guard `:1655-1662`)
  — with one exception that is this slice's sharpest red: the denylist matches **raw** keys
  (`:387-404`), so a percent-encoded `?part%4Eumber=1` slips past it into the ordinary PUT arm
  and answers **200, overwriting the object** (`:1696-1712`).
- **Success criterion:** five legs, all over the wire against an in-process gateway. Legs A, B
  and E ship in `crates/server/tests/s3_multipart_upload.rs`, legs C and D in
  `crates/server/tests/s3_multipart_lifecycle.rs` (both NEW files — see `Test file`).
  **(A) Verb semantics, routing safety and the verb × state table.** `create_multipart_upload`
  → `upload_part` × N (submitted **out of order**, at least one non-final part that is **not a
  whole multiple of the chunk size**, e.g. 5 MiB + 7 B) → `complete_multipart_upload` succeeds,
  and a subsequent GET returns the object **byte-identical** to the parts concatenated in
  part-number order. `UploadPart`'s per-part ETag is that part's lowercase-hex SHA-256. The
  published multipart ETag must be a **pure function of the part records' recorded digests and
  their order** (0016 *Open questions*, "Multipart ETag composition", owner #508) — and because
  "a SHA-256-based pure function" admits many mutually incompatible spellings, this brief
  **settles the composition**, so the test's oracle is independent of the implementation's
  choice: **`etag = lowercase_hex( SHA-256( d_1 ‖ d_2 ‖ … ‖ d_N ) ) + "-" + N`**, where `d_i` is
  the **raw 32 binary digest bytes** (not their hex text, no separators, no part numbers mixed
  in) of the *i*-th part **in ascending part-number order over exactly the parts the client
  named**, and `N` is that count; the header value is quoted like every other S3 ETag.
  **Never MD5** (ADR-0047 closed the basis). The test asserts this against a known answer it
  computes itself from the part bodies. `list_parts` reflects the staged parts with genuinely computed pagination
  (`IsTruncated`). Every cell of 0016 decision 3's **verb × state table**
  (`0016:969-978`) that is reachable without the reaper is asserted with **exact status + S3
  code**: `UploadPart`/`Complete`/`ListParts` after `abort_multipart_upload` → **404
  `NoSuchUpload`**; a second `Abort` → **204** (idempotent); an identical retry of Complete
  inside the tombstone window → **200** + the recorded ETag; a Complete reusing the upload id
  with a **different** part list → **404 `NoSuchUpload`** (the `complete_fingerprint` rule,
  `0016:898-908`); `ListMultipartUploads` lists `Open` sessions and does **not** list
  `Completed`/`Aborting` ones. Error forms, each asserted as status **and** code: wrong part
  list → **400 `InvalidPart`** / `InvalidPartOrder` **without publishing**; non-final part
  < 5 MiB → **400 `EntityTooSmall`**; part number outside `1..=10000` → **400 `InvalidArgument`**;
  a part whose chunk count exceeds `MAX_PART_CHUNKS` → **400 `EntityTooLarge`** with the session
  still usable (0016 decision 4.4 / accepted-costs "Max part size"); `GET /bucket?uploads` on an
  absent bucket → **404 `NoSuchBucket`** (`crates/gateway-s3/src/lib.rs:834-844`); Complete body
  not well-formed XML, or over the size cap → **400 `MalformedXML`**. Routing: each ill-formed
  multipart form — `PUT /b/k?partNumber=1` (no uploadId), non-numeric partNumber,
  `PUT /b/k?uploadId=U` (no partNumber), `PUT /b/k?uploads`, `DELETE /b/k?uploadId=U`, and the
  percent-encoded `?part%4Eumber=1` — answers **400 `InvalidArgument`**, never a 2xx that
  overwrites or deletes the object; a multipart form carrying a still-denylisted subresource in
  its percent-encoded spelling (`PUT /b/k?partNumber=1&uploadId=U&t%61gging=1`,
  `GET /b?uploads&%61cl`) answers **501 `NotImplemented`** naming that subresource. **Never
  assert merely "an error"** — most of these forms already answer 501 on the base, so an
  "any error" oracle would be inert for them.
  **(B) Publication, accounting and the fence.** Same file, observed by scanning raw record
  prefixes through the retained store handle (base-visible `MetadataStore::scan`,
  `crates/traits/src/lib.rs:776`):
  (i) the staging class is the **disjoint owned one, observed while the part is IN FLIGHT** —
  not merely absent afterwards. Hold an `UploadPart` mid-body (a channel- or pipe-backed body the
  test releases on command) and, at that checkpoint, assert `scan("sidx:<upload-id>:")` is
  **non-empty** while `scan("pending:")` is **empty**; then release it and assert that after the
  commit **no `sidx:` entry and no `slot:` record remains for that part**. A post-hoc scan alone
  proves nothing here: an implementation that staged under `pending:` and deleted it during the
  commit passes it, while re-entering the global `pending:` scans and the #557 cross-clock expiry
  semantics for the whole life of the upload — exactly what 0016's disjoint class exists to
  prevent. This is also the binding form of "Complete does not sit on the 30 s lease TTL"
  (`crates/server/src/lib.rs:53`): if no lease-bearing entry survives a part commit, lease
  expiry cannot affect Complete by construction. Do NOT implement this as a >30 s sleep.
  (ii) after Complete **and its bounded drain** (poll to a deadline, see `Verification posture`):
  the session record is `Completed` (a tombstone — its deletion needs #625's `W_tombstone`, out
  of scope here), `part:`/`psum:` records for the **published** parts are gone, **no `orphan:`
  record exists for any published chunk** (0016 F2's observable), and a Complete over an
  existing object orphan-marks the **prior** generation's chunks so a GC pass past the grace
  window reclaims them.
  (iii) **storage accounting is exact under the two commonest client behaviours** — a part
  number uploaded **twice**, and a Complete naming only a **subset** of the staged parts (S3
  discards the rest). After teardown and a GC pass past the grace window, **every fragment still
  present is one the published object's chunk map references, and every fragment it references
  is present**. Do NOT phrase this as "equals a plain PUT of the same bytes": multipart chunking
  follows part boundaries, so a correct implementation legitimately yields a different chunk
  count.
  (iv) **admission is exact, and the decrement is off the request path.** `mpuctl` bootstraps on
  the first Create (`require_absent` + put, `{count: 1, max_sessions, profile}`, 0016 F12/X53);
  two open sessions read `count == 2`. For a create + abort round-trip assert **both** halves
  separately: the Abort **response** returns from the fence commit alone (the session reads
  `Aborting` immediately, `count` still 1 — teardown is NOT on the request path, 0016 F9), and
  `count` returns to **0 only after the bounded drain** (poll to a deadline). Asserting
  `count == 0` at response time would reject the conforming implementation and push teardown back
  onto the HTTP path.
  **(C) Staged-fragment lifecycle vs. the maintenance plane** (`s3_multipart_lifecycle.rs`):
  (i) **a live upload is never harmed by maintenance** — with a session `Open` and parts staged,
  a GC pass reclaims **none** of its fragments AND a restore pass marks **none** stranded
  (`RestoreReport::stranded_marked == 0`, `crates/custodian/src/restore.rs:104-108`). The
  restore half is not optional: a fix that filters inside `gc::reconcile` instead of inside the
  reference set passes the GC half while an operator restore pass strands a live upload's parts
  and the next GC pass deletes them.
  (ii) **teardown reclaims, and only what it should** — after `abort_multipart_upload` and its
  drain: no session, `part:`, `psum:`, `sidx:` or `slot:` record remains and a GC pass past the
  grace window returns the fragment count to its **pre-upload baseline**. After a **Complete**
  (stated separately — the arms differ): the **published object's fragments SURVIVE** and every
  fragment not in the published map is gone. Asserting the pre-upload baseline after a Complete
  would assert data loss.
  (iii) **the OTHER maintenance consumers see the staged class too, and DO something about it** —
  decision 2 is a per-consumer contract with a row per pass (`0016:765-892`, the table at
  `:820-890`), and GC + restore are only two rows. "Does not act" is not a passing answer for
  three of them, because doing nothing satisfies it trivially. With a session `Open` and parts
  staged on a server, assert: (a) **drain / desired-state** reports the draining server
  `Pending`, never `Satisfied`, while it still holds staged fragments (`genuinely_holds` counts an
  in-flight owned fragment as held — the F6 wipe trace, the one row whose violation is a *wipe*);
  (b) **scrub** over a **corrupt** staged fragment **enqueues a repair**, not merely "walks" it;
  (c) **reconstruction** of a lost staged fragment **updates that part record's placement** under
  the fenced repoint CAS (a committed-only implementation that does nothing here passes a
  "does not silently re-place" oracle — so assert the positive); and (d) **rebalance** leaves
  staged fragments unmoved and answers status disjointly from committed ones. Each is reachable
  in-process over the same trait stores the custodian suite already uses. A patch can satisfy legs
  A, B and C(i)–(ii) in full while failing any of these.
  **(D) Segmentation (decision 7, the >10 GiB launch requirement).** With a small
  `with_chunk_size` (see `Repro instruction`), an upload whose assembled map exceeds
  `MAX_MAP_CHUNKS` publishes a **segmented** root — `seg:<group-nonce>:<epoch>:<i>` records
  present, the inode root naming the group — and a GET returns the object byte-identical; a
  second Complete-sized object below the ceiling still publishes a **flat** map. Deleting or
  overwriting the segmented object installs one `retire:bytes:{generation}` obligation in a
  single O(1) batch (never an inline fan-out), and after the drain its `seg:` range and its
  fragments are gone.
  **(E) Decision 7 also changes ordinary `PutObject` — pin that too.** Segmentation is
  multipart-only, so a single PUT must stay flat by **chunk-size selection**
  (`chunk_size_effective = max(DEFAULT_CHUNK_SIZE, ⌈Content-Length / MAX_MAP_CHUNKS⌉)`), be
  **refused `400 EntityTooLarge`** when it cannot fit `MAX_MAP_CHUNKS` even at `chunk_size_max`,
  and — for a **lengthless `aws-chunked`** stream, which cannot evaluate the formula at all — use
  the size-independent `⌈5 GiB / MAX_MAP_CHUNKS⌉`, with the configuration precondition
  `chunk_size_max ≥ ⌈5 GiB / MAX_MAP_CHUNKS⌉` checked **at configuration load** and a
  header-time refusal (never a mid-stream failure) when a deployment does not meet it
  (`0016:2287-2312` and the two accepted-costs rows on lengthless / single-PUT sizing).
  **Assert the SIZING, not a round-trip** — a byte-identical round-trip is already green on the
  base for both shapes (the base has a passing lengthless `aws-chunked` wire test,
  `crates/server/tests/s3_http_wire.rs:996-1036`, and `x-amz-decoded-content-length` is optional
  by design, `crates/gateway-s3/src/sigv4.rs:579-584`), so a round-trip oracle proves nothing
  here: (i) a **declared-length** PUT whose size would need more than `MAX_MAP_CHUNKS` chunks at
  the default chunk size publishes a **flat** map whose **chunk count is ≤ `MAX_MAP_CHUNKS`** —
  read the count off the published inode record's raw bytes — which is false on the base, where
  the count grows with the object; (ii) a **lengthless `aws-chunked`** PUT of the same body
  publishes a chunk count consistent with the **size-independent** selection, likewise false on
  the base; (iii) a declared-length PUT that cannot fit `MAX_MAP_CHUNKS` even at `chunk_size_max`
  is refused **400 `EntityTooLarge`** (the base answers 200). **Declared non-C4:** the
  *configuration-load* refusal when `chunk_size_max < ⌈5 GiB / MAX_MAP_CHUNKS⌉` cannot be reached
  from a base-compiling test (the knob does not exist on the base), so it ships as a unit test
  beside the config code — `cargo xtask ci` evidence, not C4-verify evidence. These are
  **existing-verb** behaviours: without this leg, the slice can pass every multipart test while
  making a large ordinary `PutObject` unpublishable or silently breaking today's lengthless
  streaming.
- **Falsifiability:** RED is producible **in-process on `origin/main`** — no deploy stack, no
  container, no cluster. Leg A reds on the base because every object-scoped multipart form is
  refused 501 by `unsupported_subresource` (`crates/gateway-s3/src/lib.rs:398-404`, guard
  `:1702-1709`, tokens `:343-345`) and bucket-scoped `GET /b?uploads` by
  `unsupported_subresource_decoded` (`:1655-1662`, whose comment at `:1652-1653` names 508's
  ListMultipartUploads red) — and because leg A asserts **exact status + code**, an already-501
  form still reds (501 ≠ the required 400/404). The `?part%4Eumber=1` form reds the other way:
  `unsupported_subresource` matches **raw** keys (residual documented at `:392-396`, issue #491)
  while SigV4 canonicalisation decodes-then-re-encodes (`crates/gateway-s3/src/sigv4.rs`), so on
  the base it reaches the plain PUT arm and answers **200, overwriting the object**. Legs B, C
  and D red only trivially (their setup needs multipart, which 501s), so C4's aggregate red is
  earned by leg A and **cannot be attributed** to B/C/D: `run-verify.sh` runs every added test in
  ONE cargo invocation and declares RED on the exit status
  (`engine/scripts/run-verify.sh:301-311,415-434`).
  **Gate hazard Do MUST actively defend against.** The RED leg *does* guard the case where cargo
  exits 0 (`TESTS_RAN == 0` → UNVERIFIABLE, `:416-427`), but when `run_test` returns **non-zero**
  that block is skipped entirely and execution falls through to the unconditional
  `PASS — red without the fix` at `:433` (`:415-434`). So a **compile error** in an added test
  file is scored as a red over a build that ran nothing. Therefore **neither added test file
  may reference anything the patch adds** — not a Rust symbol, not a crate newly added to a
  `Cargo.toml`. Everything the tests need is base-visible: `aws-sdk-s3` and `tempfile` are
  already dev-dependencies of `wyrd-server` (`crates/server/Cargo.toml`), `Gateway::new` /
  `with_chunk_size` (`crates/server/src/lib.rs:95,129`), `RedbMetadataStore::in_memory`,
  `MetadataStore::scan`, `WriteBatch`, `reconcile_step` + `GcContext` (7-arg form,
  `crates/custodian/src/reconciliation.rs:65-73`) and `restore` — this slice must **not** change
  `reconcile_step`'s signature or `GcContext`'s fields (that is #625's change, and doing it here
  would break both files' compile in one leg or the other). Where an observation is not reachable
  with base-visible symbols, **seed or read raw record bytes** through the store handle exactly as
  `seed_bucket` does (`crates/server/tests/s3_list_objects.rs:78-88`). Do MUST record in
  `build-notes.md`, from the RED leg, how many tests actually ran and failed — proving the red is
  assertions, not a build error.
  *Why the lifecycle legs discriminate.* GC does **not** reclaim everything unreferenced: it
  reclaims only on explicit evidence — an `orphan:` record past the reader-safe window, or an
  expired `pending:` lease — and absent both it **conservatively retains**
  (`crates/custodian/src/gc.rs:158-190`). So "removing a fragment from the reference set" never
  reclaims it; every release path must WRITE the evidence (`mark_orphaned`, `gc.rs:110-122`).
  That is a property of the BASE, and it is what leg C(ii) pins.
- **Invariant to restore:** **every durable byte written by an assembled write is, at every
  instant, either protected by a record that names it or evidenced for reclamation — and no
  state of an upload session is absorbing.** Both directions bind, and neither may be traded for
  the other: retaining unconditionally leaks forever; unprotecting without evidence leaks
  forever *silently* (`gc.rs:158-190` retains an unevidenced fragment). **Source:** proposal
  0016's four invariants and its refutation standard (`0016:138-151`, outcomes (a)–(d) at
  `0016:2811-2813`), resting on the custodian's written safety rule "never reclaim a referenced
  fragment" (`crates/custodian/src/gc.rs:22-25`). Provenance (`docs/principles.md`): this brief falls in the **§6 storage-lifecycle /
  reclamation** category, so per principle 4.2 it states catalogue invariant **C-1** — *a
  permanent or data-losing failure mode is never an acceptable cost* — with its citation
  (maintainer's standing rule, 2026-07-25; corroborated by 0016's refutation standard
  `:2802-2813` and `crates/custodian/src/gc.rs:22-25`). It is a **structural /
  object-lifetime** change, so §1.2 applies: the target is the smallest change that **restores
  the invariant**, not the smallest diff.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:**
- **Conflicts with:**
- **Ordering note:** **wave 0 of a three-bundle stack: 508 → 625 → 633.** This bundle declares no
  prerequisite; #625 (the reaper) and #633 (the operator verbs) each `Depends on` it, so the
  driver folds this bundle's accepted patch onto `pdca-integration/main` and builds them on top.
  **0016's implementation order is normative (`0016:234-268`): #625 MUST land with or before
  #508, and #633 MUST ship with #625** — while the *build* order is necessarily the reverse,
  because every record class #625 reads is created here. So the wave order is forced by code
  dependency and the release requirement is met at **merge**, not at build. Three sequential
  bottom-up PR merges do **not** by themselves satisfy it: they leave a window in which `main`
  carries the verbs and no reaper. **Open question 4 is the maintainer's choice of closure** —
  merge the run's integration branch (all three patches) as one merge, or ship the verbs behind
  an opt-in until the stack completes. Do not release,
  or merge to a release branch, a tree that has this slice without #625 and #633 — a deployment
  exposing `CreateMultipartUpload` with no running reaper is misconfigured by construction
  (`0016:264-268`), which is why this slice also owes the operator-visible startup signal in
  Scope. All three sibling S3 slices this bundle builds beside are merged and closed on `main`:
  #507 (PR #609), #510 (PR #611), #509 (PR #612); ADR-0047 (#503) is merged. **Stale-base trap:**
  if anything re-creates `results/issue_508/stack-base` naming an old `pdca-integration/main`,
  delete it before gating — `gates.py:352-360` turns it into `$PDCA_VERIFY_BASE`, which outranks
  the brief's base (`engine/scripts/run-verify.sh:186-192`). `pdca flow` clears it
  (`_point_at_integration`, `flow.py:447-461`); a bare `pdca run` / `pdca gates` does not.
- **Surfaces:** data
- **Difficulty:** high
- **Do model:** opus-max
- **External dependencies:** `aws cli (S3 gateway round-trip)` — needed ONLY for the off-Check
  large-object leg (see `Verification posture`); it is the sole token on this line and is
  registered as a `[[doctor.checks]]` row of that exact id in `pdca.toml`. That off-Check leg
  also needs a running deploy stack and ≥8 GB free disk — a topology/environment shape with no
  detect command, exempt. **Everything BINDING at Check needs nothing beyond the base
  toolchain:** all legs run in-process over an in-memory redb store, an `FsChunkStore` tempdir
  and `MemCoordination`. **No new third-party crate may enter the production graph**: `roxmltree`
  is already a dependency of `wyrd-gateway-s3` for Complete's XML body, and the ETag reuses the
  SHA-256 the write path already streams (ADR-0047 rejected MD5). **Upload ids** are 128-bit
  random tokens in lowercase hex (`0016:493-497`), and that is a *seam* decision, not a helper
  choice: ADR-0035 requires every source of nondeterminism to be **injected through an explicit
  seam** — naming a seed-derived `ChaCha8Rng` as the established pattern — and it already lists
  "move the `Gateway` id allocation behind a seam, or document why it remains out of DST reach"
  as outstanding (`docs/design/adr/0035-no-dst-reachable-global-mutable-state.md:91-98,152-153`). So: mint the id from an **injectable,
  seedable** source hung off the gateway, and **do not** copy `random_chunk_epoch`
  (`crates/server/src/lib.rs:244-262`) — it returns a single `u64`, is a per-*process* epoch
  rather than per-request entropy, and rev 3 rejected `RandomState` for exactly this role.
  `rand` / `rand_chacha` are already workspace dependencies (root `Cargo.toml:127-128`) and are
  the sanctioned source; making one a direct dependency of `crates/server` adds no new
  third-party crate to the workspace graph and needs no ADR-0003 audit — but the **added test
  files must not import it** (the RED leg reverts `Cargo.toml`). Any genuinely new crate is an
  ADR-0003 three-test audit + `deny.toml` change and a Check §6 item — declare it, do not
  smuggle it.
- **Test file:** `crates/server/tests/s3_multipart_upload.rs` (legs A/B/E) and `crates/server/tests/s3_multipart_lifecycle.rs` (legs C/D)
  — both on this line deliberately, since the driver parses test paths off the label's own line.
  Both are **NEW**
  `crates/<c>/tests/<t>.rs` files, which this project's C4 gate requires: `run-verify.sh`
  discriminates on an **added** test file (`_is_test_file`, `engine/scripts/run-verify.sh:93`)
  and degrades to a green-only, proves-nothing branch for a co-located or appended test
  (`:392-402`). A `--classify` dry-run on a synthetic patch listing the expected file set returns
  `ADDED_TEST` for both. Do MUST NOT append these legs to an existing suite. They are separate
  binaries deliberately: the lifecycle file drives the custodian loops and must not share one
  process's `tracing` callsite cache with leg A (the issue-#214 rule the custodian suite already
  follows, `crates/custodian/tests/gc_telemetry.rs:6-13`). **Both added files must live in
  `crates/server` and must NOT be `#![cfg(...)]`-gated:** the gate reads the crate-level cfgs off
  the added test sources and applies the resulting `RUSTFLAGS` to the whole invocation
  (`run-verify.sh:347-366`), so adding a `#![cfg(madsim)]` file alongside them would compile the
  server tests under `--cfg madsim` (which swaps `chunkstore-fs` behaviour,
  `crates/chunkstore-fs/src/lib.rs:170`). **Any DST regression this slice owes therefore goes
  into an EXISTING `crates/dst/tests/*.rs` file** (a modified file, which the gate does not add
  to its invocation) — never a new one.
- **Verification posture:** mixed; stated per leg.
  **Flippable red→green at Check:** legs A and E — both red on `origin/main` (leg E's over-limit
  refusal and lengthless-stream behaviour do not exist there) and they are the binding red→green
  evidence.
  **Trivially-red at Check (their setup needs multipart, which 501s):** legs B, C, D. C4 cannot
  attribute its aggregate red to them, so their force must be evidenced by **negation runs Do
  performs and records in `build-notes.md`** — for each, the deliberately-broken variant and the
  leg that catches it: (1) staged `part:`/`sidx:` fragments NOT in the reference set ⇒ leg C(i)'s
  **restore** half fails (the GC half cannot discriminate this — `gc.rs:158-190` retains an
  unevidenced fragment either way — so do not claim it does); (2) a filter inside `gc::reconcile`
  instead of in the reference set ⇒ leg C(i) restore half fails; (3) release paths WITHOUT
  `mark_orphaned` evidence ⇒ legs C(ii) and B(iii) fail (this is iteration 2's shipped shape);
  (4) a part commit that WRITES the session record instead of merely `require`-ing it ⇒
  concurrent part uploads `Conflict` (drive ≥4 parts concurrently in the negation run; leg A is
  sequential and will NOT catch it — it is the headline `aws s3 cp` shape, so evidence it
  deliberately); (5) Complete WITHOUT the `Open@E → Completing@E+1` fence ⇒ a part committing
  after Complete read the part set is silently dropped; (6) `retire:records:{parts}` naming ALL
  parts rather than only the published ones ⇒ leg B(iii) leaks the unnamed parts' bytes
  unevidenced (0016 decision 3, iteration-14 finding 3); (7) the published version frozen at
  fence time rather than `prior.version + 1` from the re-read prior ⇒ a concurrent `PutObject`
  race records a stale version; (8) drain/desired-state NOT counting an in-flight owned fragment
  as held ⇒ leg C(iii)(a) reports `Satisfied` over a live upload's bytes — the F6 wipe trace.
  A negation that does NOT fail its leg means the leg is inert — say so rather than reporting a
  pass.
  **Seeded Tier-0 DST is owed** (`AGENTS.md`: a new destructive or concurrent path lands with
  seeded DST coverage; 0016's list at `:2878-2905`), **appended to EXISTING `crates/dst/tests/*.rs`
  files** so it stays out of the C4-verify invocation and inside `cargo xtask ci`. Minimum for this
  slice: the **publication crash points** (crash between segment writes and the root flip, and the
  same-epoch idempotent recovery, F5/X37/X40); the **restore fence over a `Completing` session that
  already wrote segments** (X57); the **inode-before-`part:` publication handoff** (X67, decision
  2's source-before-destination rule); the **drain-request-versus-intent fence race (X59)** and the
  **staged re-place losing to a session fence (X29)** (`0016:884-889`); and the **classification
  sweep** helper run after each
  scenario (`0016:2911-2924` — note its rule is *no gaps*, **not** a partition: overlap between
  "committed-referenced" and "staged" is deliberate and a disjointness assertion would fail correct
  executions).
  **Bounded-drain polling.** Abort and Complete answer from the fence/flip commit alone
  (0016 F9 — Abort's latency must not scale with session size), so their obligations drain
  **after** the response. Legs B(ii)/(iii) and C(ii) therefore poll to a bounded deadline (a few
  seconds, not a fixed sleep) for the terminal condition, and Do must make the gateway's own
  drain re-entrant and convergent without the reaper. The crash-safe backstop for a drain that
  never finishes is **#625**, not this slice.
  **One pre-declared deferred leg:** the issue's headline acceptance — `aws s3 cp` of an 8+ GB
  file round-trips `sha256`-identical — is observable only off-Check against a deployed stack.
  The machinery it exercises IS built and exercised at Check by legs A–D (same verbs, same wire
  forms, smaller bodies), so nothing is deferred-because-unbuilt. **Eduard Ralph confirms the
  large-object leg by hand at sign-off or post-merge and records it in SUMMARY §9.**
- **Production reach:** the live path traverses every seam this slice builds at Check (the tests
  drive the real gateway over HTTP and the real custodian loops in-process). The one seam whose
  production consumer is a later slice is the **reaper's** half of teardown: this slice's inline,
  best-effort drain is what runs at Check, and #625 supplies the crash-safe, out-of-band drain
  plus every window-driven exit. That is a declared, ordered hand-off, not a test double.
- **Scope:** the six multipart verbs, their records, their publication path and their staged
  lifecycle, implemented as 0016 decides — concretely: the `uploads` / `uploadId` / `partNumber`
  entries come off the object-path denylist and the forms are served as real operations
  classified over **decoded** query keys; a protocol-neutral multipart surface on the
  `gateway-core` seam (no S3 vocabulary crosses it); the record classes of `0016:346-356`
  (`mpuctl`, `mpu:`, `slot:`, `part:`, `psum:`, `sidx:`, `seg:`, `seggrp:`, `retire:`) with
  their key shapes, writers, deleters and scan visibility; the state machine and fenced
  transitions of `0016:528-601`; decision 1's fence-based publication proof; decision 2's staged
  protection class across **every** maintenance consumer (GC, restore, scrub, reconstruction,
  rebalance, desired-state); decision 3's verb × state answers and failure semantics; decision
  4's retirement ledger (including routing `DeleteObject`/`DeleteObjects` supersede/unlink
  through it) and its byte-budgeted **and** operation-budgeted batches; decision 5's owned
  `sidx:` staging entries, `slot:` in-flight key space and compensation paths, with the renewal
  loop rewriting its `slot:` record in the SAME batch as its `sidx:` leases; decision 7's
  flat-or-segmented chunk map; the **protocol-facing half of decision 6** — the serialized
  `mpuctl` admission reservation at Create (with the profile-mismatch refusal + alarm), the
  `clock_source` stamp and the rule that every writer of a session-scoped timestamp verifies its
  own source against it; the **one narrow trait seam** `MetadataStore::scan_page` with the
  ordering / exclusive-cursor / no-skip semantics of `0016:2646-2672`, implemented on
  `metadata-redb`, `metadata-fdb`, `metadata-tikv` and the DST sim store and asserted in
  `metadata-conformance`; a **bounded inline drain** driven by the gateway for its own
  obligations; **the whole budget-profile configuration surface and its defaults** — the knobs
  0016 lists as this slice's (`MAX_MAP_CHUNKS`, `MAX_SEG_CHUNKS`, `MAX_PART_CHUNKS`,
  `MAX_ROOT_SEGMENTS`, `MAX_STAGED_CHUNKS`, `MAX_INFLIGHT_PARTS`, `chunk_size`, `R_publish`,
  `MAX_COMPLETE_ATTEMPTS`) **and, because `Create` writes `mpuctl.profile` and that record stores
  `W_ref`, the `W_ref` configuration surface and its default too**, plus the `B_bytes`/`B_ops`
  budgets for the batches *this* slice commits. Each value inside the range 0016 settles, with
  the range cited at its definition, and `MAX_SESSIONS` **derived** (`⌊W_ref / U_ref⌋`, clamped by
  `SCAN_CAP/2`), never chosen. **This resolves a cycle the proposal leaves open, and the resolution
  is deliberate — read it before you touch a knob.** 0016's *Open questions* assigns `W_ref` and
  `B = min(B_bytes, B_ops)` to **#625**; but `mpuctl.profile` is written by `Create` — in *this*
  slice, in wave 0 — and *contains* `W_ref` and the caps; `MAX_PART_CHUNKS` and
  `MAX_INFLIGHT_PARTS` are legally clamped by `B_ops`; and #625's reaper must **fail closed** when
  its local profile disagrees with the ledger's. A wave-0 slice therefore cannot defer these to a
  wave-1 slice, and two independently chosen `B_ops` values would be actively unsafe: a session
  admitted under this slice's clamp could need more sequential operations than #625 permits in its
  **unsplittable** reap fence or terminal delete, which then times out forever. So: **there is ONE
  value set (`W_ref`, `B_bytes`, `B_ops`) and ONE configuration seam, landed here in wave 0**,
  sized to satisfy the constraints of **both** slices' batches (0016's worked example — a reconcile
  host at `W_ref = 4,000,000` chunk-refs — is the starting point, `0016:2126-2153`), and **#625
  consumes it unchanged and re-derives nothing**. This moves the *location* of the choice, not its
  substance. **SETTLED — Eduard Ralph, 2026-07-25**, on the standing rule that a permanent or
  data-losing failure mode is never an acceptable option in Wyrd: split budget authority admits
  exactly that class (a legally-admitted session whose unsplittable reap fence no longer fits, and
  so is never reaped and never reclaimed), so it is off the table, and strict location ownership is
  unbuildable in the confirmed wave order. The deviation from 0016's owner table is documentary
  only and rides the editorial correction already flagged in Open question 2. Do MUST make that seam public
  and stable enough for the custodian to read and compare; an **operator-visible startup signal** on the `wyrd s3` role
  naming that multipart requires a running custodian reaper (#625) — 0016 requires the
  misconfiguration be visible, and the mechanism is this slice's choice; and the **living
  architecture docs** (`docs/design/architecture/06-runtime-view.md`,
  `08-crosscutting-concepts.md`) updated for the new record classes, the segmented map and the
  new verbs — that is a **merge requirement** in this repo (`AGENTS.md` "Docs currency"), not a
  follow-up.
  / out of scope: **the reaper loop and every window-driven exit** (`W_open`, `W_session`,
  `W_completing`, `W_tombstone`, the cursor-keyed out-of-band drain, the reaper's clock guard and
  terminal delete) — that is **#625**, wave 1 of this stack; **the operator abort / terminal
  expiry verbs and the foreign-clock alarm** — **#633**, wave 2; **FU-1** (the segmented-map
  record-shape ADR — an ADR is architecture-board authority, never a model's to author) and
  **any** file under `docs/design/adr/` or `docs/design/specs/`, and any edit to 0016 itself;
  **FU-3** telemetry/alerting (#630); **FU-4** (surfacing the abandonment reason in the error
  text); **FU-5** (part-record segmentation); fixing **#491** for the *remaining* denylist
  entries (just do not extend the bug to the new routes); `x-amz-copy-source` UploadPartCopy;
  per-part `Content-MD5` / `x-amz-checksum-*` trailers beyond what the streaming path already
  verifies; and S3 bucket-lifecycle configuration as a client-facing API.
- **Repro instruction:** on `origin/main` @ `22d71b4`, start the in-process gateway exactly as
  `crates/server/tests/s3_list_objects.rs` does (`RedbMetadataStore::in_memory()`, an
  `FsChunkStore` tempdir, `MemCoordination`, `Gateway::new(...).with_chunk_size(N)`), seeding each
  bucket's `bucket:{name}` marker as **raw bytes before the store moves into `Gateway::new`** —
  nothing on `main` creates bucket records (#511), so every listing leg 404s otherwise. Build an
  `aws-sdk-s3` client with `sdk_client` (`crates/server/tests/s3_gateway_cluster.rs:96-110`).
  `create_multipart_upload` answers **501 NotImplemented** today; `PUT /b/k?part%4Eumber=1`
  answers **200 and overwrites the object**. For leg D pick `with_chunk_size` and part sizes so
  the assembled map crosses the `MAX_MAP_CHUNKS` Do chooses while each part stays under
  `MAX_PART_CHUNKS` and non-final parts stay ≥ 5 MiB (e.g. 64 KiB chunks, six 5 MiB parts ⇒ ~480
  chunks); Do records that arithmetic in `build-notes.md`.
- **Citations expected:** Do must cite `path:line` on `origin/main` for every change. Peer
  callsites Do MAY open and should mirror:
  **routing** — `foreign_subresource_on_delete` (`crates/gateway-s3/src/lib.rs:487`) with its
  interception site (`:1630`) is #509's solved form of this exact problem: intercept a route
  BEFORE the denylist without opening a hole in the fence for every other key. Decode keys before
  matching, exempt only the multipart markers, keep every other denylisted key refusing. The
  object-path guard and its destructive-fall-through rationale are `:1696-1709`; the "refuse a
  form we do not implement rather than mishandle it" precedent is `:1711-1732`.
  **payload dispatch** — `crates/gateway-s3/src/lib.rs:1747-1785` (streaming / signed / unsigned
  arms; a stock SDK sends parts `aws-chunked`, so all three must be handled).
  **bucket existence** — `list_objects` 404s `NoSuchBucket` off `list_container` returning
  `Ok(None)` (`:834-844`).
  **XML + body cap** — `roxmltree`, already a dependency of `wyrd-gateway-s3`; size Complete's
  body cap on `MAX_DELETE_BODY_BYTES`'s worked pattern including its compile-time assert
  (`crates/gateway-s3/src/lib.rs:439-457`).
  **publication** — `create` (`crates/core/src/metadata.rs:366`) and
  `commit_chunk_map_superseding` (`:582`) each build and commit a PRIVATE `WriteBatch` and accept
  no extra precondition; their lease-conditional siblings `create_leased` (`:398`) and
  `commit_chunk_map_superseding_leased` (`:638`) show the established "same function + extra
  guards" pattern; the guard being **replaced** by the fence is `live_lease_guards` (`:778`); the
  superseding-orphan obligation is `crates/core/src/write.rs:296-332`.
  **decode→encode identity** — `PendingEntry` (`crates/core/src/metadata.rs:346`) gains `owner`
  and `staged`, both `skip_serializing_if`, `Some` only on a `sidx:` value. **Get the rationale
  right:** today's `renew_pending` preconditions on the **raw stored bytes** it just read
  (`require(key, current)` with `current` the unparsed value, `crates/core/src/metadata.rs:748-760`),
  and `live_lease_guards` does the same (`:786-795`) — so the "emitting `null` breaks every
  renewal" mechanism that 0016 asserts at `:475-491` is **not** what this base code does, and Do
  must not repeat that claim. The requirement stands on the repo's own standing rule instead
  (`AGENTS.md` *Serialization identity*: optional/legacy fields omitted when absent, never emitted
  as defaults, decode→encode byte-identical wherever a CAS or content hash depends on it) and on
  the fact that **this slice's own** session/staging CAS paths will compare re-encoded values.
  Ship both round-trip tests (legacy `pending:` with both fields absent; owned `sidx:` with both
  present).
  **upload-id minting** — a **128-bit** token in lowercase hex from an **injectable, seedable**
  source (ADR-0035's seam rule; `rand`/`rand_chacha` are workspace deps). `require_absent(mpu:<id>)`
  turns a collision into a `Conflict`. `random_chunk_epoch` (`crates/server/src/lib.rs:244-262`) is
  the *shape* of a coordination-free mint but **not** the mechanism to copy: it is 64 bits and
  per-process.
  **clock reads** — every wall-clock read goes through the annotated single source
  (`crates/server/src/lib.rs:705-715`); bare `SystemTime::now()` is **denied by clippy**
  (`clippy.toml`, wyrd#619, commit `63d66b9`), so a new call site must state the clock that owns
  its lifecycle.
  **custodian** — reference set and safety gate `crates/custodian/src/gc.rs:128-190`;
  `mark_orphaned` `:110-122`; `ExpiredPendingPolicy`'s cross-clock reasoning `:77-104`; the
  restore pass and `RestoreReport` `crates/custodian/src/restore.rs:102-108`, and its
  **bounded-batch precedent** `MARK_BATCH = 1_000` with the FoundationDB transaction-limit
  rationale `:92-100`; the fenced control point `crates/custodian/src/reconciliation.rs:65-115`
  (do not add a parallel entry — the anti-#141 rule).
  **store envelope** — `crates/traits/src/lib.rs:744-758` (10 KB key / 100 KB value / 10 MB / 5 s),
  `SCAN_CAP` `:286`, `scan` `:776`, `WriteBatch::require` semantics `:825-843`.
  **test harness** — `sdk_client` (`crates/server/tests/s3_gateway_cluster.rs:96-110`);
  `seed_bucket` (`crates/server/tests/s3_list_objects.rs:78-88`). The retained iteration-2 draft
  `results/issue_508/s3_multipart_upload.rs` still has working `run_gc_pass` / `run_restore_pass`
  helpers over base-visible symbols — **its protocol is superseded by 0016, its harness plumbing
  is not**; reuse the plumbing, ignore its record design.
- **Prior-art check (triage cycles):** searched by affected file path across merged history and
  closed/rejected work, re-run on 2026-07-25 at `22d71b4`. `crates/gateway-s3/src/lib.rs` — last
  touched by #616 (`061bdaa`), #619 (`63d66b9`) and the #509 series (`9585604`, `19d3457`,
  `e99190e`, `adc1de8`), all merged; none implements a multipart verb and all deliberately
  PRESERVE the multipart denylist (the `:1652-1653` comment names 508).
  `crates/custodian/src/gc.rs` / `reconciliation.rs` — last touched by #554 / #430 / #551; no
  multipart-related change. `crates/core/src/metadata.rs`, `crates/core/src/write.rs`,
  `crates/traits/src/lib.rs` — no multipart-related change in history. No
  `crates/server/tests/s3_multipart_*.rs` on `main`. #491 (percent-encoded denylist bypass)
  remains OPEN and out of scope except for the new routes. **In-project prior art:** this
  bundle's own `iteration-v1` and `iteration-v2` were built and REJECTED, and `iteration-v3`'s
  brief was blocked — read `sources/01-prior-plan-reviews.md` and `sources/02-salvage-from-rev3.md`
  before starting; the reviewers' findings there are what a reviewer will attack again.
  Result: **no prior art on `main`; net-new.**
- **Disposition hint:** likely-fix

## Motivation

The single biggest gap in the S3 surface. `PutObject`'s 5 GB ceiling makes larger objects
impossible, and stock clients auto-switch to multipart above ~8 MB, so ordinary large uploads
fail today with a 501. Core to the 0.1-Alpha S3 completion epic (milestone 16). Two prior
implementation attempts were rejected because the *protocol underneath Complete* was never
written down; proposal 0016 now writes it down, and this slice implements it.

## Design

**The design is proposal 0016.** Do implements decisions 1–5 and 7 and the protocol-facing half
of decision 6 as that document specifies them, and does **not** re-derive them. The map from
decision to line, for orientation only:

| 0016 | Decision | What #508 owes |
|---|---|---|
| `:333-527` | The records (ADR-0046 shape rules) | every key class, its writer, deleter and scan visibility |
| `:528-601` | The state machine | fenced CAS transitions, epochs, "no state is absorbing" |
| `:603-689` | Batch inventory | every batch this slice commits, inside `E_tx/2` bytes **and** `B_ops` |
| `:693-764` | D1 — publication proof | fence, not lease liveness |
| `:765-892` | D2 — staged protection class | per-consumer visibility across the maintenance plane |
| `:894-1037` | D3 — lifecycle + verb × state | the wire contract this brief's leg A pins |
| `:1038-1504` | D4 — bounded work | the retirement ledger; supersede **and** unlink route through it |
| `:1505-1793` | D5 — reclamation evidence | owned `sidx:` entries, `slot:` key space, compensation |
| `:1796-1998` | D6 (protocol half) | `mpuctl` admission, `clock_source`, the writer-side clock check |
| `:2280-2496` | D7 — segmentation | flat-or-segmented map, staged publication, epoch-scoped `seg:` |

### What 0016 leaves to this slice — the S3 wire surface

Settled here, and stable across four rounds of adversarial review on rev 3:

1. **Routing.** The multipart markers come off the object-path denylist
   (`UNSUPPORTED_SUBRESOURCES`, `crates/gateway-s3/src/lib.rs:342-385`) and are dispatched as
   real operations, classified over **percent-decoded** query keys — mirroring #509's
   `foreign_subresource_on_delete` interception, which shows how to route a form *before* the
   denylist without opening a hole for every other key. A multipart form that does not classify
   into a valid operation is **refused (400 `InvalidArgument`)**, never served by a plain object
   verb.
2. **The percent-encoding fence.** `?part%4Eumber=1` must not reach the plain PUT arm. This is
   the one form that answers **200 and overwrites the object** on the base, and it is the
   sharpest single red in leg A. #491 stays open for the *remaining* denylist entries; this slice
   must simply not extend the bug to the new routes.
3. **Status and error codes.** The exact matrix in `Success criterion` leg A, plus 0016 decision
   3's verb × state table (`0016:969-978`) — which is normative for the *answers* while the XML
   and status encoding are this slice's.
4. **ETag composition.** ADR-0047 closed the basis (lowercase-hex SHA-256, MD5 rejected). The
   composition is this slice's, constrained to one property: the published ETag is a **pure
   function of the part records' recorded digests and their order**, so a retried Complete
   re-derives the identical value. Reuse the SHA-256 the write path already streams.
5. **Knob values**, listed in Scope — each inside the range 0016 settles, each citing that range
   at its definition site so a later reader can check it.

### Precedence rulings — where 0016 contradicts ITSELF (read this before decision 7)

"0016 wins" cannot arbitrate a disagreement *inside* 0016. Cross-vendor plan review found one,
and it is load-bearing, so this brief rules on it and the ruling is a **NEEDS-HUMAN item for the
maintainer to correct in the proposal** (a proposal edit is architecture-board authority, never
Do's):

- **Segment-group keying.** §1 (`0016:499-526`, iteration-14 finding 2 / iteration-15 finding 1)
  states that segment records must **not** be keyed by the upload id, because they outlive the
  `mpu:` tombstone that would be their only reuse guard; it mints an **independent segment-group
  nonce**, reserves it in the Create batch under `require_absent(seggrp:<group-nonce>)` (the batch
  inventory's Create row, `:656`), and defines the marker's two-armed deletion. Decision 7
  (`:2314-2320`) still says `Segmented { group: (<upload-id>, <epoch>) }` and calls the nonce "the
  minting upload-id", and the implementation summary repeats `group = (upload-id, epoch)`
  (`:2678`). **The §1 rule supersedes** — it is the later correction, it carries the defect
  analysis (an id reused after its tombstone expires would overwrite a live object's segments),
  and it is the shape the Create batch already reserves. Implement the independent nonce +
  `seggrp:` marker; where decision 7 says "upload-id", read "group-nonce".

If Do meets any **other** internal contradiction in 0016, it stops and reports it as a §6 item —
it does not choose.

### Deliberately NOT decided here

The publication proof, the protection class, the terminal states, the reclamation evidence and
the segmentation shape. Rev 3 tried to specify these inside the brief and was blocked for it.
Cite 0016; do not re-derive.

## Alternatives considered

Recorded in 0016 *Alternatives considered* (`0016:2730-2803`) and **not reopened**: lease-liveness
publication, a per-session mutation counter as the publication proof, scan-then-create admission,
owned residue under `pending:`, per-part exact-value preconditions at Complete, staging inside
`pending:` with a longer TTL, inline orphan expansion, synchronous Abort teardown, a smaller
reference-set scan bound, `W_open` alone without `W_session`, deferring segmentation, and leaving
staged bytes unscrubbed. Two wire-surface alternatives are this brief's own: a true S3
`md5-of-part-md5s` ETag (rejected — ADR-0047 rejected MD5 and it would add a dependency), and
buffering parts in memory (rejected — it violates the streaming/OOM-cliff invariant the gateway
already holds, `crates/server/src/lib.rs:276-286`).

## Impact & compatibility

- **Additive record prefixes** (`mpuctl`, `mpu:`, `slot:`, `part:`, `psum:`, `sidx:`, `seg:`,
  `seggrp:`, `retire:`), all disjoint from `inode:` / `dirent:` / `pending:` / `bucket:` /
  `orphan:` / `desired:dserver:`. A store written by an older build reads unchanged.
- **One non-additive change**, spelled out in 0016 *Backward compatibility*: the `orphan:`
  **value** gains structured variants. All legacy forms must still decode — never reject.
- **`PendingEntry` gains two optional fields**, both `skip_serializing_if`, `Some` only on an
  owned `sidx:` value. The decode→encode identity is load-bearing for every CAS and renewal.
- **`InodeRecord.chunk_map` becomes flat-or-segmented**, which touches every `.chunk_map`
  consumer (~19 sites). This is the change 0016 recommends for ADR graduation (FU-1) — the ADR
  itself is out of scope and human-authored.
- **One trait seam widens** (`MetadataStore::scan_page`) across four implementations plus the
  conformance suite.
- **Client-visible**: three refusals that are ordinary S3 answers — `503 SlowDown` (too many
  in-flight parts or open sessions), `404 NoSuchUpload`, and `400 EntityTooSmall`/`EntityTooLarge`.

## Open questions

1. **Slice size.** This is the largest single bundle this project has attempted, and the two
   earlier attempts at a *simpler* protocol were rejected at 123 KB and 169 KB of diff. The
   maintainer's 2026-07-24 scope call (`sources/00-design-authority-0016.md`) is "plan the full
   slice"; this brief honours it. If the resulting patch is not reviewable in one pass, the
   natural seams to split at — each of which would need its own tracker issue — are: (i) the
   `scan_page` trait seam + backends + conformance; (ii) decision 7 segmentation and the
   `chunk_map` shape change; (iii) the custodian-side reference-set / GC changes of decision 2;
   (iv) the gateway verbs and the wire surface. **Do does not split — Do reports.** Splitting is
   the human's call at sign-off.
2. **ADR graduation (FU-1).** Settled: **no ADR in this patch.** An ADR/spec/proposal change is
   architecture-board authority (INTEGRATION §4) and always a NEEDS-HUMAN item. The same applies
   to the **proposal correction** the precedence ruling above records: 0016 needs an editorial fix
   so decision 7 and the implementation summary stop naming the upload id as the segment-group
   nonce. **NEEDS-HUMAN.**
3. **The startup signal for a missing reaper** is a warn-level operator signal in this slice. If
   the maintainer wants a hard startup refusal instead, say so at sign-off — it is a one-line
   posture change, not a redesign.
4. **Release atomicity — SETTLED as (a) by Eduard Ralph, 2026-07-25 ("we stay with your suggested
   order"); recorded here so Do has one contract and review does not re-open it.** 0016's
   implementation order says #625 must land *with or before* #508, while the build order is the
   reverse (this slice's records are #625's prerequisite), so three sequential PR merges would
   leave a window in which `main` exposes multipart with no reaper. **The settled closure is (a):
   merge the run's integration branch (`pdca-integration/main`, carrying all three accepted
   patches) into `main` as ONE merge**, treating the three stacked PRs as review units rather than
   merge units — which is exactly what the harness's wave fold already produces, and which asks
   **nothing of Do**. So: **this slice exposes the verbs normally and ships NO opt-in switch.**
   The rejected alternative (b) — verbs refusing behind an explicit opt-in until the stack
   completes — would add a public configuration knob for a transient condition; if the maintainer
   prefers it, say so at sign-off *before* Do runs, because it changes what is built.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR MAY
happen during the cycle (useful for CI feedback). The PR MUST NOT be marked ready before sign-off
accepts, and this slice MUST NOT be merged to a release ahead of #625 and #633.

## Iteration 4 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected on the strength of the T4 rubric review (58 blocking, gating fail) plus the adversarial review's four concrete, cited defects — fix all of them in the next Do pass: 1. Segmented objects are invisible to the maintenance plane: GC (`crates/custodian/src/gc.rs:305`) and restore (`crates/custodian/src/restore.rs:397`) iterate `record.chunk_map` directly instead of resolving through `resolve_chunk_map`/reading `seg:` records, so a restore pass strands and a later GC pass deletes a live, committed segmented object's fragments. Route every maintenance consumer through the shared segmented-map resolver, not just the gateway read path. 2. `drain_records`'s paged branch (`crates/core/src/multipart.rs:2043-2067`) never converges past `B_OPS` (1,000) keys — it recomputes and re-deletes the same first 1,000 keys forever with no cursor, so any Complete naming ≥501 parts (i.e. the brief's own headline `aws s3 cp` 8GB case at default chunk size) never fully drains. Needs a cursor/rewritten-payload progress mechanism like the sibling arms (`Segments`, `UnnamedParts`) already have. 3. `MetadataStore::scan_page` ships as a default-only shim over `scan()` on all four backends, so it still inherits `SCAN_CAP` and does not escape the bound it exists to escape; no `metadata-conformance` assertions were added either. Needs native cursored implementations (or an honest scope-cut agreed with Plan) plus the conformance tests the brief's Scope names. 4. Legs C(iii)(b) and (c) — scrub and reconstruction over staged fragments — are unimplemented and untested: scrub only reads `referenced.placed`, never `referenced.staged`, so a corrupt staged fragment is never repaired; reconstruction has no staged/`sidx:`/`part:` handling at all. Brief explicitly named these as required positives, not "does nothing" passes. Also carry forward for the rebuild: the zero seeded Tier-0 DST coverage gap (T4 "For the human" item) should be addressed in the same pass rather than deferred again, given it covers the same new destructive/concurrent paths this iteration touches.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 589 mutants tested in 39m: 216 missed, 85 caught, 286 unviable, 2 timeouts
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 58 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue
- Full previous attempt preserved in `iteration-v4/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 5 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected on the strength of the reviewer findings: T4 batched rubric review gates fail (33 blocking) and the adversarial review found three concrete, verified defects that must be fixed in the next Do pass: 1. Silent permanent part loss above ~4,000 parts in a single Complete — `drain_records`' paged branch (`crates/core/src/multipart.rs:2377`/`:2454`) truncates derivation at 4,000 units but marks the obligation fully drained regardless, deleting the tombstone while parts past the 4,000th are never reclaimed. The regression test at `:2723` uses only 700 parts, which is below the threshold and does not catch this — raise it past 4,001 and fix the truncation/exhausted-flag interaction. 2. Permanent multipart-admission exhaustion — `max_sessions()` derivation plus `Completed` sessions never releasing their admission slot (`terminal_delete_batch` / `teardown_sessions`, `crates/core/src/multipart.rs:182,1239,2498`) means only ~70 lifetime successful uploads are possible before the store refuses all further `CreateMultipartUpload` calls until #625 lands. Needs a decision + fix (or an explicit, documented interim posture) before this ships. 3. Ordinary-PUT size-limit bypass via omitted Content-Length — `chunk_size_for_lengthless` (`crates/core/src/multipart.rs:2595`, `crates/server/src/lib.rs:528-530`) never checks streamed byte count against `SINGLE_PUT_MAX_BYTES`, so an oversized lengthless `aws-chunked` PUT is fully staged and can fail only after streaming, past `MAX_MAP_CHUNKS`. Needs a byte-count guard that fails before commit. Also address C3 (fail-closed handling of malformed `x-amz-decoded-content-length`), T2 (double percent-decoding of multipart query keys), and T3 (UploadPart latency coupled to unrelated global drain backlog) from the advisory review. Do not re-attempt the current approach unchanged — fix all of the above in the same pass.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 735 mutants tested in 39m: 244 missed, 129 caught, 360 unviable, 2 timeouts
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 33 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue
- Full previous attempt preserved in `iteration-v5/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 6 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected on: (1) T4 batched rubric review gates FAIL — 37 blocking findings; (2) the adversarial review's four concrete, cited defects in this iteration's new code, three tagged [impl]: - UploadPartCopy (x-amz-copy-source) is silently served as a 0-byte UploadPart instead of being refused — can publish an empty object where a copy was expected (crates/gateway-s3/src/lib.rs:1938, guard should sit before the UploadPart arm at :2335). - Any operator storage-server drain turns every UploadPart into 404 NoSuchUpload fleet-wide (crates/core/src/multipart.rs:1489-1491, crates/server/src/multipart.rs:453-456) — 0016 specifies a re-plan against the fresh topology instead; this one is a design call for the next Do pass to scope or implement, not a one-line fix. - A same-part-number retry racing the original wrongly answers 404 NoSuchUpload instead of 200 last-writer-wins (crates/core/src/multipart.rs:1565, crates/server/src/multipart.rs:570-572). - The "second Abort is 204" test has a race (crates/server/tests/s3_multipart_upload.rs:646-666) — poll for the sidx: entry before the first Abort, matching the sibling lifecycle test's pattern. Fix all four (plus the stale MAX_STAGED_CHUNKS comment at crates/core/src/multipart.rs:124) in the next Do pass. Do not re-attempt the current approach unchanged. Note: overall finding volume is down from prior iterations, but the bundle remains very large (6th attempt); the human has an upstream fix in progress to break down future packages into smaller slices — no bundle-specific Act-candidate note needed here.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 821 mutants tested in 45m: 270 missed, 174 caught, 376 unviable, 1 timeouts
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 37 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue
- Full previous attempt preserved in `iteration-v6/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 7 — carry-forward (from the previous attempt)
- Sign-off rationale: Re-enter Plan: this 7th attempt is still one 44-file / 14,117-line cross-plane slice (T1 Structure, §6). Split the re-plan along the seams the brief's own Open question 1 already names, not per S3 operation: (i) the `scan_page` trait seam + backends + conformance suite; (ii) decision-7 segmentation and the `chunk_map` shape change; (iii) the custodian-side reference-set / GC changes of decision 2; (iv) the gateway verbs and wire surface. Each seam becomes its own reviewable tracker slice/issue rather than one monolithic patch. Carry forward into the new slices' briefs, so the next Do pass isn't blind: - Adversary-found, not yet in any brief: CreateMultipartUpload answers false 503 SlowDown under ordinary client concurrency (aws-cli default 10 concurrent) on an empty store — the CAS-contention retry needs separating from the id-collision retry with a real bound (crates/server/src/multipart.rs:46,297,320-322). - The operator-visible startup signal (brief Scope item) is wired to `Gateway::with_reaper`, which has zero callers anywhere in the tree — functionally dead in every real deployment (crates/server/src/lib.rs:201; crates/server/src/cli.rs:1588). - GC's orphan-ledger walk lost its SCAN_CAP heap bound exactly where this patch's own arithmetic makes the population large — needs a bounded page walk, a design call for whichever slice owns GC (crates/custodian/src/gc.rs:720-738). - Routing regression: GET/HEAD ?partNumber=N (a valid S3 part-ranged GetObject) now wrongly answers 400 instead of the prior 501 (crates/gateway-s3/src/lib.rs:629,1955). - C4 xtask ci gate fail looks like a stale `target/` cache artifact (mtimes predate sources), not a real break — re-run `cargo clean && xtask ci` before treating any future C4 red here as genuine.
- Failing gate: C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) — xtask: `cargo build --workspace --exclude wyrd-dst --all-targets` failed with exit status: 101
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 925 mutants tested in 58m: 312 missed, 213 caught, 395 unviable, 5 timeouts
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 32 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue
- Full previous attempt preserved in `iteration-v7/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
