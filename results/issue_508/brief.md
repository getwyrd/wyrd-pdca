# Design proposal — issue 508 / multipart-s3-verbs

> Plan artifact (design-proposal form; enhancement). Do reads ONLY this file.
> The `- **Label:** value` lines are parsed by the driver — keep their shape.
>
> **This is a RE-PLAN.** Seven implementation attempts of the monolithic #508 were rejected; the
> seventh at sign-off on reviewability (one 44-file / 14,117-line cross-plane patch, T1
> Structure). #508 is now **seam (v) of five** — the S3 wire surface only — over a protocol,
> a record shape, a store primitive and a maintenance plane that land first
> (634 → 635 → 636 → 637 → **508**). Everything below the wire has moved out of this brief.
>
> **The design is already settled and is normative:** proposal **0016 — the multipart commit
> protocol**, `docs/design/proposals/draft/0016-multipart-commit-protocol.md` on `origin/main` @
> `22d71b4`. For this slice read: the **verb × state answer table** `0016:969-978` and decision 3
> around it `0016:894-1037`; the **`PutObject` chunk-size carve-out** `0016:2287-2312`; the
> denylist rows `0016:2718-2721`; the accepted-costs rows on single-PUT and lengthless sizing
> `0016:2836-2860`; the open questions this slice owns `0016:3064-3080`.
> **Do MUST read the verb × state table and the `PutObject` carve-out before writing code.**
>
> Citations re-verified against `origin/main` @ `22d71b4` on 2026-07-26. The passages revised on
> 2026-10-15 (legs A's Abort cells, B's `DELETE` form, D, E's layout, F, `Falsifiability`, `Test
> file`) were verified at `7bf7eb9`. The passages revised on 2026-10-20 (this hold, leg B's
> wrong-target case, leg E's key and test entry, leg H, `Scope`, `Citations expected`, `Design §
> In scope`, `Open questions` 1) were verified at `12dd082`. **Line numbers in
> `crates/gateway-s3/src/lib.rs` have moved since `7bf7eb9`:** at `12dd082` the denylist is
> `:410-413`, `unsupported_subresource` `:466`, `unsupported_subresource_decoded` `:556`,
> `router` `:261`. Re-locate every gateway-s3 citation below on the resolved base.
>
> **⚑ HOLD — NEEDS-HUMAN before this bundle is ever scheduled: as written it duplicates four
> open child issues.** Checked 2026-10-20 at `12dd082` and on the tracker: getwyrd/wyrd records
> **#666–#671 as sub-issues of #508** (the 2026-07-31 re-slicing,
> `docs/2026-07-31-alpha-reslicing-proposal.md:7`, `:67-72`).
> - **Closed and landed:** #666, the `MultipartGateway` seam (`b879e7b`): the trait
>   (`crates/gateway-core/src/lib.rs:742`), the S3 bound (`crates/gateway-s3/src/lib.rs:227-229`)
>   and the composition-root impl whose six bodies answer `NotImplemented` and say which issue
>   replaces each — #668, #669 or #670 (`crates/server/src/lib.rs:799-883`). #671 (single-PUT
>   sizing, leg D) is closed too.
> - **Open, no bundle, and together they own every remaining leg:** leg B → **#667** (routing
>   hardening); leg A's Create / Abort / ListParts / ListMultipartUploads cells and leg H →
>   **#668**; leg A's UploadPart cells → **#669**; leg A's Complete cells, leg E's reaper-absent
>   signal and leg F's restore gate → **#670**. Legs C and G go with whichever of them they
>   observe. Nothing is left that no child owns.
> - **Plan recommends** that #508 become the coordinating parent: no Do on this brief, and it
>   closes when #667–#670 do (`duplicate` at that point). The requirements this brief adds beyond
>   those issues' text travel to the owning child's brief at its Plan: leg B's wrong-target case
>   (#667/#668), leg E's refusal posture and repeating alarm (#670; its issue text still asks for
>   a once-at-startup signal, which the human's 2026-07-26 decision in `Open questions` 1
>   replaced), and leg H (#668, which Create's own doc names as the owner of its deadline wiring).
>   The alternative is to keep #508's scope and close #667–#670 as duplicates.
> **This is the human's call, and this revision does not make it.** `Depends on` keeps the bundle
> unschedulable until then (926, 927, 658 and 659 are not COMPLETE). Until it is made, read the
> rest of this brief as the consolidated requirement set for the S3 multipart surface, not as a
> build order.

- **Slug:** multipart-s3-verbs
- **Track:** alpha
- **Kind:** enhancement (design proposal)
- **Goal:** the S3 multipart verb set — `CreateMultipartUpload`, `UploadPart`,
  `CompleteMultipartUpload`, `AbortMultipartUpload`, `ListParts`, `ListMultipartUploads` — served
  over the protocol #636 landed, so `aws s3 cp` of a large file works with default settings.
  Today every multipart form is hard-refused **501 NotImplemented** by the subresource denylist
  (`crates/gateway-s3/src/lib.rs:343-345`, object-route guard `:1696-1709`, bucket-route guard
  `:1650-1662`), so objects >5 GB are impossible and `aws s3 cp` / boto3 — which auto-switch to
  multipart above ~8 MB — fail on ordinary large uploads. (0016 decision 7's single-`PutObject`
  carve-out is **not** this slice's: #671 owns it, re-scoped with the human on 2026-10-14; see
  leg D.)
- **Success criterion:** two **NEW** test files (legs A, B, C, F in one; leg E alone in the
  other), both driving an **in-process gateway over the
  wire** — a real `aws-sdk-s3` client plus hand-signed raw requests for the forms an SDK cannot
  spell. Nothing is stubbed: the gateway, the S3 wire layer, the commit protocol and the chunk
  store are all production code; only the metadata *backend* is a test double, exactly as
  `RedbMetadataStore::in_memory` already is in `crates/server/tests/s3_list_objects.rs`.
  **(A) Verb semantics and the answer table** (`crates/server/tests/s3_multipart_upload.rs`).
  `create_multipart_upload` → `upload_part` × N submitted **out of order**, at least one non-final
  part that is **not** a whole multiple of the chunk size (e.g. 5 MiB + 7 B) →
  `complete_multipart_upload` succeeds, and a subsequent GET returns the object **byte-identical**
  to the parts concatenated in part-number order. `UploadPart`'s per-part ETag is that part's
  lowercase-hex SHA-256; the published multipart ETag is the composition #636 settled —
  `lowercase_hex( SHA-256( d₁ ‖ d₂ ‖ … ‖ d_N ) ) + "-" + N` over the **raw 32 binary digest
  bytes** in ascending part-number order over exactly the parts the client named, quoted like
  every other S3 ETag, **never MD5** (ADR-0047 closed the basis). The test computes the expected
  value itself from the part bodies. `list_parts` reflects the staged parts with genuinely
  computed pagination (`IsTruncated`); `ListMultipartUploads` lists `Open` sessions and does
  **not** list `Completed`/`Aborting` ones.
  Every cell of the decision-3 table reachable without a reaper, asserted as **exact HTTP status
  + exact S3 error code** — never "an error", because most of these forms already answer *some*
  error (501) on the base and an "any error" oracle would be inert:
  `UploadPart`/`Complete`/`ListParts` after Abort → **404 `NoSuchUpload`** (the same answer
  whether the session is still `Aborting` or already deleted, so no race); **Abort of an
  `Aborting` session → 204** (idempotent), asserted on a session **planted `Aborting`** as
  canonical bytes through the retained store handle, so no drain started by an earlier request
  can delete it under the assertion (a second Abort racing the first one's teardown could
  legitimately meet either state); **Abort after the session's drain and terminal delete have
  finished** (leg C's deadline poll) → **404 `NoSuchUpload`**: the record is absent, and
  `abort_answer` answers absence that way (`crates/core/src/multipart.rs:4805-4812` at
  `4bac538`, `0016:977`);
  an identical Complete retry inside the tombstone window → **200** + the
  recorded ETag; a Complete reusing the upload id with a **different** part list → **404
  `NoSuchUpload`** (the `complete_fingerprint` rule); wrong part list → **400 `InvalidPart`** /
  `InvalidPartOrder` **without publishing**; non-final part < 5 MiB → **400 `EntityTooSmall`**;
  part number outside `1..=10000` → **400 `InvalidArgument`**; a part whose chunk count exceeds
  `MAX_PART_CHUNKS` → **400 `EntityTooLarge`** with the session still usable; `GET /bucket?uploads`
  on an absent bucket → **404 `NoSuchBucket`**; a Complete body that is not well-formed XML, or
  over the size cap → **400 `MalformedXML`**; a create refused by admission → **503 `SlowDown`**;
  a create whose session record could not reach its later states inside the value ceiling
  (core's `Refusal::SessionTooLarge`, returned before any store read,
  `crates/core/src/multipart/create.rs:342-350`, doc `:232-235`) → **400 `MetadataTooLarge`**,
  `<Message>` naming the measured bytes and the limit. Reach it with a `Create` on a short key
  (≤ 1024 bytes, so no key-length refusal fires first) carrying a `Content-Type` of 100,000
  ASCII bytes — the record stores it, so its encoding crosses `MAX_VALUE_BYTES` = 100,000
  (`crates/core/src/metadata.rs:549`). Assert the exact status and code, and that nothing was
  written: no `mpu:` record, and `mpuctl.count` unchanged (or the ledger still absent).
  *(Added 2026-10-22. `MetadataTooLarge` is S3's own 400 code for request metadata too large to
  store; a Content-Type is system metadata. Plan settled the code so Do does not invent one; the
  human may change it at sign-off.)*
  **(B) Routing safety — the new surface must not be evadable** (same file, issue #491's
  interaction). Each ill-formed multipart form answers **400 `InvalidArgument`**, never a 2xx
  that overwrites or deletes the object: `PUT /b/k?partNumber=1` (no uploadId), a non-numeric
  `partNumber`, `PUT /b/k?uploadId=U` (no partNumber), `PUT /b/k?uploads`. **`DELETE
  /b/k?uploadId=…` is the Abort request shape, not an ill-formed one**, so it is pinned by the
  id it carries, and after each the object `k` must still answer HEAD `200`: a live upload's
  id → `204` (leg A); a well-formed id (32 lowercase-hex characters,
  `crates/core/src/multipart.rs:814-848`) that names no session → **404 `NoSuchUpload`**; an id
  that is **not** a well-formed token (e.g. `uploadId=zz`) → **404 `NoSuchUpload`** too, the
  answer S3 gives for an upload id it cannot resolve. The point of all three is that a DELETE
  carrying `uploadId` never reaches the object DELETE.
  **An upload id acts only on its own target.** Every verb that carries an upload id
  (UploadPart, Complete, Abort, ListParts) checks the request's bucket and key against the
  session's stored target before it acts: `SessionRecord::parent()` / `object()`
  (`crates/core/src/multipart.rs:2464-2472`), spelled exactly as Create spells them for a
  `PutObject` to the same key (`crates/core/src/multipart/create.rs:50-60`). A mismatch answers
  **404 `NoSuchUpload`** and changes nothing. The check is the adapter's: core's Abort takes only
  the store and the upload id (`crates/core/src/multipart/abort.rs:282-285`), so an adapter that
  passes the id straight through lets `DELETE /other/key?uploadId=U` abort `U`'s real upload.
  (The target never changes over a session's life, so a read before the core call is enough.)
  Test it with a live upload `U` on `/b/k` and a second existing bucket `b2`: `/b/other` and
  `/b2/k`, each with UploadPart, Complete, Abort and ListParts naming `U`, answer exact
  `404 NoSuchUpload`. Afterwards `U` is still `Open`, ListParts on `/b/k` lists its parts
  unchanged, and a Complete on `/b/k` publishes.
  **The target is checked only after the bucket is validated** (added 2026-10-22). The target
  is the root plus the flat name `<bucket>/<key>` (`crates/core/src/multipart/create.rs:50-60`),
  and the path is split on its first literal `/` before decoding
  (`crates/gateway-s3/src/lib.rs:1832-1834`), so `/b%2Fa/k` and `/b/a/k` produce the **same**
  parent and name. The base refuses a decoded bucket containing `/` with **400
  `InvalidBucketName`**, but only inside the ordinary PUT arm (`:1882-1893`). A multipart route
  dispatched outside that arm would pass the flat-name comparison above and act on the upload
  through the alias. So every multipart route, object- and bucket-scoped, refuses a decoded
  bucket containing `/` with that same **400 `InvalidBucketName`** before it reads a session,
  matches a target or calls core. Test it with a live upload `U` on `/b/a/k` (bucket `b`, key
  `a/k`): `POST /b%2Fa/k?uploads`, and UploadPart, Complete, Abort (`DELETE /b%2Fa/k?uploadId=U`)
  and ListParts on `/b%2Fa/k` naming `U`, each answer exact `400 InvalidBucketName` (hand-signed
  raw requests; an SDK will not spell the path). Afterwards `U` is still `Open`, ListParts on
  `/b/a/k` lists its parts unchanged, no second `mpu:` record exists, and a Complete on `/b/a/k`
  publishes. On the base each alias form answers `501`, so the leg reds on its exact status.
  And the percent-encoded form **`PUT /b/k?part%4Eumber=1`** must answer
  **400**, not a plain object PUT: `unsupported_subresource` matches **raw** keys (the residual is
  documented at `crates/gateway-s3/src/lib.rs:385-396`) while SigV4 canonicalisation
  decodes-then-re-encodes, so on the base that form reaches the plain PUT arm and answers
  **200, silently overwriting the object**. Removing `partNumber`/`uploadId`/`uploads` from the
  denylist must not leave that hole behind, and a multipart form carrying a **still-denylisted**
  subresource in its percent-encoded spelling
  (`PUT /b/k?partNumber=1&uploadId=U&t%61gging=1`, `GET /b?uploads&%61cl`) must still answer
  **501 `NotImplemented`** naming that subresource.
  **Three refusals the base gives only on a route this slice bypasses must survive** (added
  2026-10-22; at `4bac538`, line numbers in `crates/gateway-s3/src/lib.rs`). Each is a regression
  guard: the base already answers it, so it is green on both trees, and it must stay green.
  - **Bulk delete keeps refusing a multipart selector.** `POST /b?delete` is intercepted before
    the denylist (`:1766`) and refuses every other denylisted key through
    `foreign_subresource_on_delete`, which reads `UNSUPPORTED_SUBRESOURCES` (`:578-584`, list
    `:410`). Removing `uploads`, `uploadId` and `partNumber` from that list would let a valid
    `<Delete>` body through with them attached. So the bulk-delete route keeps refusing all
    three, decoded, with the answer it gives today: exact **`501 NotImplemented`** naming the
    key. Test with a well-formed `<Delete>` body naming two existing objects: `POST
    /b?delete&uploads`, `POST /b?delete&uploadId=U` and `POST /b?delete&part%4Eumber=1`
    (hand-signed raw requests) each answer exact `501 NotImplemented`, and both objects still
    answer HEAD `200` with their bytes unchanged.
  - **Create keeps the object-key limit.** The 1,024-byte limit on the decoded key's UTF-8
    (`MAX_KEY_BYTES`, `:501`, `400 KeyTooLongError`) is enforced only in the object `PUT` arm
    (`:1898-1905`), and core's Create does not check it (its preflight is the value ceiling,
    `crates/core/src/multipart/create.rs:342-350`). Create is a `POST`, so it must refuse a
    decoded key over 1,024 bytes with exact **`400 KeyTooLongError`** before admission. Test
    with multi-byte keys, so a character count would give the wrong answer: 512 × `é` (1,024
    bytes) creates a session; 512 × `é` + `a` (1,025 bytes) answers exact `400
    KeyTooLongError`, with no new `mpu:` record and `mpuctl.count` unchanged. (On the base the
    1,025-byte form answers `501`, so it reds.) The other upload-id verbs need no length check:
    no session can exist on a longer key, so the target check above answers them `404
    NoSuchUpload`.
  - **UploadPart refuses a copy request.** `x-amz-copy-source` on a `PUT` is refused before any
    body byte is read, because streaming a copy request's empty body as data overwrites the
    target (`:1853-1872`), but only inside the object `PUT` arm. `PUT
    /b/k?partNumber=1&uploadId=U` carrying that header (S3's UploadPartCopy, not one of the
    six verbs) must answer exact **`501 NotImplemented`** before anything is staged. Test with
    a live upload `U` whose part 1 is staged: the copy request for part 1 answers `501
    NotImplemented`; ListParts still shows part 1 with its original ETag and size; Complete
    publishes and GET returns the original part bytes.
  **Carried forward from the seventh attempt's own wire tests — these were demonstrated failures,
  and losing them is a regression in the plan, not a simplification.** Keep a named assertion for
  each: `UploadPart`'s per-part ETag is that part's lowercase-hex SHA-256 (not the composed
  multipart form); `ListParts` pagination is genuinely computed rather than a constant
  `IsTruncated=false`; `ListMultipartUploads` on an **absent bucket** answers `404 NoSuchBucket`
  rather than an empty list; a Complete body over the size cap answers `400 MalformedXML` rather
  than being read unbounded (the *Protocol input* rule, `../wyrd/AGENTS.md:159-165`); and the
  salvaged file at `results/issue_508/s3_multipart_upload.rs` is the reference for the exact
  request shapes that reached these paths.
  **(C) Publication and accounting, observed at the wire and in the store** (same file, through
  the retained store handle using base-visible `MetadataStore::scan`): after Complete **and its
  bounded drain** (poll to a deadline — see `Verification posture`), the session record is
  `Completed` (a tombstone; its deletion needs #625's `W_tombstone` and is out of scope),
  `part:`/`psum:` records for the **published** parts are gone, and **no `orphan:` record exists
  for any published chunk**. A Complete over an existing object orphan-marks the **prior**
  generation's chunks. For a create + abort round trip assert both halves separately: the abort
  **response** returns from the fence commit alone (teardown is not on the request path), and the
  admission count returns to its prior value **only after** the drain **and the terminal delete**.
  **Do NOT assert that a Complete releases admission capacity.** `mpuctl.count` counts every
  `mpu:` record in any state, tombstones included, and is decremented only in the terminal delete
  — after `W_tombstone`, which is **#625's** (`0016:348`, `:966-968`, `:2029-2031`). #636's brief
  was corrected on 2026-07-26 to match; the tombstone this slice's leg A depends on for the
  identical-Complete retry is the *same* record that holds the slot. A test here that expected
  capacity to return on Complete would contradict the prerequisite and could only be satisfied by
  deleting the tombstone early, breaking retry idempotence.
  **(D) REMOVED 2026-10-15 — the single-`PutObject` carve-out is #671's, and it could not run
  as written.** The leg asked for per-PUT chunk-size selection up to `⌈5 GiB / MAX_MAP_CHUNKS⌉`.
  `MAX_MAP_CHUNKS` is **158** (`crates/core/src/multipart.rs:4748-4759`), so that is
  **33,979,172 bytes (~32.4 MiB)** per chunk. Chunks are capped at **16 MiB**
  (`MAX_CHUNK_SIZE`, `crates/server/src/lib.rs:55-72`, enforced at `crates/server/src/cli.rs:2619-2626`)
  because each RS(6,3) fragment crosses the wire in one gRPC message under tonic's default 4 MiB
  receive limit; a 32.4 MiB chunk gives ~5.4 MiB fragments, and its networked PUT fails while a
  local-FS test passes. #671 (in this batch) was re-scoped with the human on 2026-10-14 to
  match: the chunk size stays the startup setting, a single PUT is capped at 158 × that size and
  refused `400 EntityTooLarge` from its headers, and a PUT must declare its length
  (`results/issue_671/brief.md:6-25`). **This slice therefore adds no chunk-size selection, no
  `chunk_size_max` knob and no lengthless-PUT sizing, and changes no single-PUT behaviour.**
  `crates/server/tests/s3_http_wire.rs:996-1036` (the lengthless wire test) must stay green
  unmodified.
  **(E) Multipart is REFUSED unless the deployment declares the durability plane — plus an
  alarmable, repeating signal.** *(Posture changed 2026-07-26, after the plan review corrected the
  fuse; see `Open questions` 1 for the full reasoning.)* 0016 makes it normative that a deployment exposing `CreateMultipartUpload` without a
  running reaper is **misconfigured**, and that the implementing slices must make that state
  visible — explicitly leaving the mechanism open, "a startup refusal **or** an operator-visible
  alarm" (`0016:264-268`). This slice ships **both** arms, because the plan review corrected the
  fuse: `mpuctl.count` counts **every** `mpu:` record including `Completed` tombstones, and the
  decrement happens only in the terminal delete after `W_tombstone` — which is **#625's**
  (`0016:348`, `:966-968`, `:2029-2031`). So with no reaper running, **every successful upload
  permanently consumes a slot**, and `MAX_SESSIONS` is derived and small (≈19–79,
  `0016:2836-2860`): a reaper-less deployment bricks multipart after a few dozen *successful*
  uploads, in ordinary use — not after a run of abandoned ones. Three properties are binding:
  (0) **The verbs are refused unless the deployment explicitly declares the durability plane is
  deployed** — a **local configuration acknowledgement**, evaluated at configuration load, **not** a
  liveness probe of a running custodian. Local config is the only reliable signal available: there
  is no custodian-liveness observable to read (`crates/custodian/src/leadership.rs:22`, `:46`
  publish a fencing token, not a heartbeat), and a probe that guessed wrong would refuse uploads
  while the reaper was in fact running, turning a maintenance-plane gap into a data-plane outage.
  Undeclared ⇒ every multipart verb answers a stated S3 error naming the configuration key;
  declared ⇒ the verbs serve. Assert **both** arms, and assert ordinary `PutObject` / GET / DELETE
  are **unaffected** while undeclared — this gate covers multipart only, never the object path.
  **The key, named here (2026-10-20):** the environment variable
  **`WYRD_S3_MULTIPART_REAPER_DEPLOYED`**, values `true` / `false`; absent means `false` (fail
  closed). Any other value is refused at load, never mid-stream: `cmd_s3` checks it before it
  binds, beside `parse_s3_chunk_size` (`crates/server/src/cli.rs:2348`), and `serve_s3`
  (`:2658`) reads and checks it again before it serves, as `serve_s3_role` re-checks the chunk
  size (`:2477-2479`). It is read in `serve_s3`, the one serve path every composition runs
  (`:2652-2657`), so the composition root owns it (ADR-0010); wherever it is carried below that,
  its default is not-declared. `serve_s3`, `serve_s3_role`, `S3Gateway::new` and `Gateway::new`
  keep their signatures. It is an environment variable and not a flag because a flag would have
  to reach `serve_s3` as a new argument, which no test file can call on the reverted base (see
  `Falsifiability`); the S3 role already reads its credentials this way (`WYRD_S3_ACCESS_KEY`,
  `cli.rs:2335`). A flag spelling can follow in a later slice. The refusal's `<Message>` names
  the variable.
  **How the tests reach it on both trees:** every gateway in both test files is served through
  `wyrd_server::cli::serve_s3(Arc<Gateway<…>>, credentials, region, listener, metrics)`
  (`cli.rs:2658-2664`; `pub` on `main` in the public `cli` module, whose sibling `serve_s3_role`
  `crates/server/tests/s3_gateway_cluster.rs:41` already imports and drives in-process), over a `Gateway` the test builds so it keeps its
  own store handle. The first file sets the variable to `true` once per process
  (`std::env::set_var`, safe on edition 2021; precedent `crates/server/tests/log_env_directive.rs:27`)
  and never changes it. Leg E's file sets and clears it inside its one sequential test function.
  On the reverted base the variable is ignored and every multipart form answers 501, so each arm
  reds on its exact-status assertion, not on a build error.
  **The exact response contract for BOTH new refusal gates** (this one and leg F's restored-image
  gate), settled here so Do does not invent status codes: each answers **`503 ServiceUnavailable`**
  with S3 code **`ServiceUnavailable`** and a `Retry-After`-less body whose `<Message>` names the
  condition — not `501 NotImplemented` (the verb *is* implemented), not `500` (nothing failed), and
  not `403` (nothing is unauthorized). `503` is the S3-conforming "temporarily cannot serve this"
  and is what a well-behaved SDK already retries/backs off on. Assert **status and code** for every
  one of the six verbs under each gate, exactly as leg A does — an "any error" oracle is inert here
  because the base already answers `501`.
  (i) **It is emitted on the durability-plane telemetry seam, not only as `tracing::warn!`** — a
  metric the `DurabilityTelemetry` `tracing`→OTel bridge counts **plus** an append-only audit
  event, exactly the shape every other operator signal in this workspace uses (`emit_malformed`,
  `crates/custodian/src/gc.rs:355-365`; `emit_reclaim`, `:338-349`; the seam itself,
  `crates/telemetry/src/lib.rs:79`, `:158`). A bare log line is not alarmable, and "operator-visible"
  then means nominally rather than actually.
  (ii) **It repeats for as long as the condition holds** — on an interval, not once at role entry.
  A startup line scrolls out of a log buffer within hours; a gateway that has been up for a month
  under this misconfiguration would otherwise show an operator nothing at all, which is precisely
  when the condition matters most.
  **Where leg E runs: its own NEW test binary, `crates/server/tests/s3_multipart_durability_gate.rs`,
  holding every leg-E assertion (both refusal arms and the telemetry) and nothing else.** `tracing`
  caches each callsite's interest in process-global state; a test in the same process that drives
  the same callsites with no capturing subscriber can cache them disabled and silently drop this
  leg's counters (the #214 race, `crates/custodian/tests/gc_telemetry.rs:6-13`). The other file's
  gateways run the same gateway code, so sharing a binary with them is exactly that race. Inside
  this binary: install **one** capturing subscriber **process-wide** (a global default, set once,
  before any gateway starts) — not a per-future `with_subscriber`, which does not reach the tasks
  the gateway spawns — and pass the same dispatch as `serve_s3`'s `metrics` argument, which is
  where the role's own signals go (`cli.rs:2683-2688`); run the leg's cases **in one test
  function, in sequence** (the
  gc_telemetry precedent has one, a `#[tokio::test]`, `:197-198`). The negative case reads the counter before and
  after the declared gateway runs for more than two intervals and asserts no change.
  Assert: that the metric and the audit event are emitted (read back in-process with the capturing
  subscriber / `gather_prometheus`, the idiom `crates/custodian/tests/gc_telemetry.rs` already
  uses); that a second emission follows the first without a restart; and — **the negative
  case, without which the oracle is vacuous** — that a deployment which HAS declared the durability
  plane emits **no** such signal. An implementation that alarms unconditionally satisfies every
  positive assertion while telling the operator nothing, and a permanently-firing alarm is
  indistinguishable from no alarm at all.
  **(F) A restored image: the guarantee is the deployment order, and the gateway's gate is defence
  in depth only.** *(Revised 2026-10-15; the earlier text made the gateway gate the guarantee,
  which the landed record cannot support.)* 0016 requires "the restore fence generation MUST
  complete before any gateway serves multipart verbs on the restored image", and allows two ways
  to meet it: the deployment runs the restore-fence pass **before re-enabling the gateways**, or
  the gateways refuse multipart until they observe the generation complete (`0016:3017-3021`;
  `:836-841`). Otherwise a client's retried Complete can fence a still-resurrected `Open` session
  and publish over reclaimed bytes.
  **The second way cannot be built on what landed.** #810 landed the record (`FenceGeneration`
  under `MPUFENCE_KEY`, `crates/core/src/multipart.rs:1925-1990`, key `:1141`; `39bf35a`), and its own doc says
  why it is not enough (`:1937-1945`): a restore rewinds the whole store, this record included, so
  an image captured after an earlier pass completed comes back reading `complete` until the next
  pass starts. Read alone, it cannot tell this restore's completion from one the image carried, so
  a gate on it serves multipart on exactly the image it exists to protect. The same holds for an
  image with no record (no pass had ever run when it was captured).
  **So this slice takes the first way, as the landed code says to "until then"** (`:1943-1945`):
  - **the guarantee** is the deployment order — the post-restore pass runs before any gateway is
    re-enabled. State it in the living architecture doc beside the new verbs (part of the
    docs-currency edit already in scope), with the reason the gate below cannot replace it;
  - **the gate, defence in depth:** while the record is present and **not** complete — a pass
    has started and not finished, or finished leaving a session for a human — every multipart
    verb is refused with the 503 contract below. Record absent, or `complete: true`: the verbs
    serve. Assert all three cases with exact status and code for each of the six verbs, and that
    ordinary `PutObject` / GET / DELETE are unaffected while refused;
  - this slice reads that record and nothing else. It MUST NOT write a generation counter of its
    own, add a restore-scoped signal, or infer completion from session states (the split-authority
    defect this stack keeps rejecting).
  ⚑ **Human:** making the gateway gate authoritative needs a restore-scoped signal beside the
  record (the code names this as #508's need, `:1943`). That is custodian and core work outside
  this wire slice, and no issue exists for it. File one if the deployment order is not an
  acceptable long-term guarantee.
  **(G) `cargo xtask ci` green**, and the off-Check large-object round trip — see
  `Verification posture`.
  **(H) A dropped request never drops a commit that may have been sent, and an in-doubt outcome
  is never answered as a result** (added 2026-10-20). Core's Create and Abort are documented **not
  cancel-safe once a commit is sent**: bound them through the store's deadlines, never by
  dropping the future (`crates/core/src/multipart/create.rs:328-335`, which names #668 as the
  owner of the caller's deadline wiring; `crates/core/src/multipart/abort.rs:277-281`). A handler
  future is dropped when its connection closes, and a timeout layer drops it too. Required:
  (i) every adapter call into a core multipart function that can send a commit (create, the
  part commit, complete, abort) runs to its end even when the request that started it is gone —
  for example, spawned on the role's runtime with the handler awaiting its handle; (ii) no
  timeout wraps a core multipart call anywhere between the router and core; (iii) a core in-doubt
  error (`CreateError::InDoubt`, `AbortError::InDoubt`, Complete's in-doubt error) — and
  Complete's stopped error, a fence it holds and could not release (#658's brief, revised
  2026-10-22) — answers **`500 InternalError`**, never a protocol answer, and is logged with the
  upload id when it carries one. **Test at the adapter boundary, base-visibly:** build the `Gateway` over a
  test-local store wrapper that logs every store call and can hold a chosen commit until
  released; call `MultipartGateway::create_multipart_upload` on it (a trait method on `main`,
  `crates/gateway-core/src/lib.rs:742-756`); once the wrapper reports the admission commit held,
  drop the future; release the commit.
  **How the held commit is released matters (revised 2026-10-22).** A commit released as plain
  `Committed` is a bad oracle: core returns straight away with no further store call — Create
  answers `Created` (`crates/core/src/multipart/create.rs:462`) and Abort answers `Fenced`
  (`crates/core/src/multipart/abort.rs:327`) — so a correctly detached adapter would show
  nothing after the release. So the wrapper releases the held commit by **applying its batch to
  the inner store and then returning `CommitUnknownResult { may_still_commit: false }`** (out of
  flight, and in fact applied). Core must then read to settle it: Create backs off and reads its
  own `mpu:<id>` again, finds its own bytes and answers `Created` (`create.rs:448-455`,
  `:369-372`); Abort reads `mpu:<id>` and answers `Fenced` (`abort.rs:345-346`, `:295-296`).
  The wrapper applies the batch only at release, inside the held commit's own future, so a
  dropped future never applies it. Assert, within a deadline: (a) the wrapper logs **a read of
  `mpu:<id>` after the release** (core's settling read; a dropped future makes none); (b) the
  held commit future ran to its return and was never dropped (a drop guard on the wrapper's
  commit future records an early drop); (c) the batch took effect. For Create, read the store:
  an `Open` `mpu:` record and `mpuctl.count` one higher (nothing deletes them afterwards). **For
  Abort, do not read the session state from the store after the fact** (revised 2026-10-22):
  `Aborting` is transient. Once core answers `Fenced`, the drain that leg C requires runs and
  the terminal delete removes the record (`AbortOutcome::Fenced` "is the response — byte
  reclamation is the drain's, asynchronously", `crates/core/src/multipart.rs:4637-4642`;
  `Aborting → drain → absent`, `0016:550`), and with no parts that can finish before the test
  looks. Instead the wrapper **records the bytes its settling read of `mpu:<id>` returned**, at
  the moment of that read, and the test asserts they decode to `Aborting`. The store afterwards
  shows the session **not `Open`** (either `Aborting` or already absent; either is correct).
  The same for `abort_multipart_upload`, holding its fence commit. Over the wire: with the wrapper answering every commit
  `CommitUnknownResult { may_still_commit: true }` unapplied, Create and Abort each answer exact
  `500 InternalError`. On the reverted base the verbs answer `NotImplemented` without a commit,
  so the wait for the held commit times out and the leg reds on that assertion.
- **Falsifiability:** RED is an **assertion** red, in-process, with **no deploy stack, no
  container and no cluster** — **but only on a base that carries every prerequisite, and no such
  base exists yet** (see *Base resolution* at the end of this field; nothing here is evidence that
  the unlanded prerequisites will deliver what it assumes). On that base: Leg A reds
  because every object-scoped multipart form is refused 501 by `unsupported_subresource`
  (`crates/gateway-s3/src/lib.rs:385-404`, guard `:1696-1709`, tokens `:343-345`) and
  bucket-scoped `GET /b?uploads` by `unsupported_subresource_decoded` (`:1650-1662`) — and because
  leg A asserts **exact status + code**, an already-501 form still reds (501 is neither the
  required 400 nor the required 404). Leg B's `?part%4Eumber=1` form reds the *other* way: on the
  base it answers **200 and overwrites the object**. (Leg D is removed.) Legs C, E and F red on
  their own observables; leg E's declared arm reds because the base ignores
  `WYRD_S3_MULTIPART_REAPER_DEPLOYED` and still answers 501; leg H reds because the base sends no
  commit for the wrapper to hold.
  **Corollary the test files MUST obey:** neither added file may reference **anything this slice
  adds** — not a Rust symbol, not a crate newly added to a `Cargo.toml`. `run-verify.sh`'s RED leg
  reverts modified production files and keeps the added tests; a test that does not compile there
  runs zero tests and is reported **UNVERIFIABLE (exit 77)** — no red at all, a §6 item
  (`engine/scripts/run-verify.sh:40-56`, `:207-229`, `:521-541`).
  So the declaration of leg E is an environment variable, never a new setter or argument (see
  leg E). On `main` today: `aws-sdk-s3`, `aws-smithy-*`, `aws-credential-types` and `tempfile` are
  dev-dependencies of `wyrd-server` (`crates/server/Cargo.toml`); `cli::serve_s3`, the
  `MultipartGateway` trait and its methods, `Gateway::new` /
  `with_chunk_size`, `RedbMetadataStore::in_memory`, `MetadataStore::scan`, `WriteBatch`, and the
  multipart key and record helpers (`mpu_key`, `decode_session_record`, `MPUFENCE_KEY`,
  `FenceGeneration`, `crates/core/src/multipart.rs`) exist. The protocol functions the verbs call
  come from the unlanded prerequisites below, and Do must check each symbol it uses against the
  resolved base, not against this list.
  Where an observation is not reachable with base-visible symbols, **seed or read raw record
  bytes** through the retained store handle exactly as `seed_bucket` does
  (`crates/server/tests/s3_list_objects.rs:78-88`).
  **Do MUST record in `build-notes.md`, from the RED leg, how many tests actually ran and
  failed**, and confirm the failures were assertions. A build-error red here is a defect in the
  test, not a pass.
  **Base resolution — a precondition, not met on 2026-10-15** (checked at `7bf7eb9` and with
  `gh issue view`; `origin/pdca-integration/main` does not exist). What each `Depends on` id
  supplies, and where it stands:
  - **on `main` already:** #693 (state machine, verb × state answer table, digests; `605b33a`),
    #808 (drain status reads the staged class; `cf6b420`), #810 (the restore-fence generation
    record leg F reads; `39bf35a`), #813 and #814 (scrub and reconstruction of staged chunks;
    `feb1e30`, `ab35f6a`);
  - **merged since (checked 2026-10-20 at `12dd082`):** #894 (Create and admission, PR #908,
    `f9b70e4`) and #895 (Abort's fence, PR #909, `6e26f1f`);
  - **planned, not built (2026-10-20):** #926 and #927 (UploadPart staging and the part commit,
    #657's split), #658 (Complete, ETag, tombstone answers), #659 (retirement routing, drain and
    terminal delete).
  The driver holds this bundle unschedulable until all eleven are COMPLETE
  (`src/pdca_harness/waves.py:65-120`, `:286-320`). COMPLETE alone does not put a prerequisite's
  code in the base: an out-of-batch prerequisite is folded into nothing (`waves.py:197-200`). The
  base carries it only if it runs **in the same `pdca flow` batch** as this bundle (folded onto
  the run's integration branch, `pdca-integration/main…`, `src/pdca_harness/integrate.py:73-85`,
  exported as `$PDCA_VERIFY_BASE`, `src/pdca_harness/gates.py:544-580`, honoured by
  `engine/scripts/run-verify.sh:247-265`) **or has merged to `main`** first. RED is runnable only
  once one of those holds for all four unlanded prerequisites. **Do MUST record in
  `build-notes.md` the base commit it built on and confirm that base contains each
  prerequisite's code before claiming any red.** The dependencies stay declared as they are.
- **Invariant to restore:** **a request form this gateway does not implement is refused, never
  silently mishandled — and a form it *does* implement is reachable by exactly one route,
  whatever spelling the client uses.** Both halves bind: the first is why the denylist exists at
  all (`crates/gateway-s3/src/lib.rs:335-342`: without the guard a `PUT /b/k?partNumber=1&uploadId=…`
  would silently **overwrite** the whole object and a `DELETE /b/k?tagging` would delete it, both
  answering 2xx); the second is what removing three denylist entries puts at risk, because the
  raw-key match that guarded them is not the canonicalisation SigV4 performs (issue #491).
  **Source:** the routing decision recorded at `crates/gateway-s3/src/lib.rs:335-342` and
  `:375-396` (issue #507 adversary, issue #491), and 0016's requirement that the denylist "loses
  its multipart entries in that slice" (`0016:2718-2721`) — i.e. loses them *to routing*, not to
  a fall-through. SELF-TEST: this cannot be satisfied by guarding one handler — it is a property
  of the dispatch table as a whole, which is why leg B enumerates the ill-formed forms rather
  than testing the happy path.
- **Scope:** the six S3 multipart verbs and their routing in `crates/gateway-s3`, **over the
  landed `MultipartGateway` seam** (#666, `b879e7b`): replace the six `NotImplemented` bodies of
  the composition-root impl (`crates/server/src/lib.rs:799-883`) with calls into the protocol
  slices below (#693, #894, #895, #926, #927, #658, #659), with the target-identity check and
  the three carried refusals (bulk-delete selector, Create key length, UploadPartCopy; leg B)
  and the cancellation ownership and in-doubt answer (leg H); the durability-plane declaration
  gate and alarm (leg E); the restore-image gate and the deployment-order statement (leg F); and
  the architecture-doc currency edit. **Do not add, move or reshape the trait or its bounds** —
  the trait (`crates/gateway-core/src/lib.rs:742`) and the S3 bound
  (`crates/gateway-s3/src/lib.rs:227-229`) are on `main`. Two seam changes are allowed, both
  arms of `MultipartRefusal` (`crates/gateway-core/src/lib.rs:519-565`) and nothing else:
  (1) **required:** a `SessionTooLarge { bytes: u64, limit: u64 }` arm mirroring core's
  `Refusal::SessionTooLarge` (`crates/core/src/multipart.rs:4515-4526`), which #894 added after
  the seam landed. The enum's doc says it mirrors core arm for arm (`gateway-core/src/lib.rs:513-517`)
  and protocol refusals must be answer values, never `Err` (`:719-723`), so without the arm the
  adapter can only map the refusal lossily — `EntityTooLarge` carries chunk counts, not bytes —
  or return it as an error. Keep the doc's arm-for-arm claim true. The same core arm is also
  Complete's value-ceiling refusal (#658's brief, leg I); the S3 part-number range (`1..=10000`,
  leg A) makes that one unreachable over the wire, so the arm has one mapping. Neither added test file may
  name the new arm (`Falsifiability`, *Corollary*): leg A asserts it only at the wire, as `400
  MetadataTooLarge`. (2) **optional:** how the
  503 gates of legs E and F reach the wire: either the S3 layer refuses before it calls the seam,
  or `MultipartRefusal` gains one arm for them; say which. ⚑ See the HOLD at the top: every item here is also owned by one of #667–#670.
  **The capability matrix (note added 2026-10-22, from #743's split into #937, #938, #939).** If
  `crates/validate/src/matrix.rs` exists on your base (#937 and #939 landed), its `pre-0.1`
  profile declares the six multipart operations `unsupported(#508)` → `501 NotImplemented`, and
  `crates/validate/tests/smoke_loopback.rs` runs the `smoke` scenario against this tree's
  gateway. Once this slice routes the verbs, smoke reports `unexpected-support` for them and that
  test goes red in `cargo xtask ci`. That red is the matrix doing its job: do **not** weaken the
  classifier, drop the rows, or delete or loosen a test. Resolve it by the pre-release rule #937
  writes into 0017 §3: in `pre-0.1`, edited in place (no new profile before the first tag), move
  the six rows to `supported, not checked (#750)`, and update the exact-row assertion in
  `crates/validate/tests/capability_matrix.rs` to match. Those two edits are in scope; nothing
  else under `crates/validate/`. If `matrix.rs` is not on your base, do nothing there: #937 and
  #939 record whatever the gateway on their base answers. If #508 and #939 are ever scheduled in
  one batch, declare `Conflicts with` between them first: built side by side on one base, each
  passes alone and the folded tree fails.
  **Out of scope:**
  every single-`PutObject` sizing change — chunk-size selection, a `chunk_size_max` knob,
  lengthless-PUT sizing, the single-PUT `EntityTooLarge` (#671); the commit protocol, records, ETag
  *computation*, admission control and drain (#636's successors); the staged-byte protection class and every
  custodian pass (#637) — do **not** re-assert its legs here; the reaper (#625); operator abort /
  terminal expiry (#633); `scan_page` (#634) and the segmented chunk map (#635); any file under
  `docs/design/adr/` or `docs/design/specs/`, and any edit to `0016`.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** 813, 814, 808, 810, 693, 894, 895, 932, 933, 927, 658, 659
- **Conflicts with:** 625, 633, 671
- **Ordering note:** **RE-POINTED 2026-10-22 — `926` → `932, 933`.** #926 (657.1) was split at its third re-plan
  into #932 (657.1a: slot reserve, owned intent, drain fence, write and body deadlines) and #933
  (657.1b: in-flight renewal and `StagedAttempt::renew`, depends on #932) —
  `results/issue_926/split-proposal.md`. A split parent ends COMPLETE, so `926` would be met at
  once. 
  **RE-POINTED 2026-10-20 — `657` → `926, 927`.** #657 was split at its re-plan
  into #926 (UploadPart staging) and #927 (the part commit and the composed `UploadPart`, depends
  on #926) — `results/issue_657/split-proposal.md`. A split parent ends COMPLETE, so `657` would
  be met at once; the S3 `UploadPart` verb calls #927's function. **`Conflicts with: 671` added 2026-10-15:** #671 (in this batch) edits the
  single-PUT path in `crates/server/src/lib.rs` and the `aws-chunked` length handling in
  `crates/gateway-s3/src/`; this slice adds the multipart impl to the same `crates/server/src/lib.rs`
  and the routing to `crates/gateway-s3/src/lib.rs`. No build-on edge: this slice no longer
  touches single-PUT sizing (leg D).
  **RE-POINTED 2026-09-18 — `664` → `808, 810`.** #664 was split at its
  re-plan into #808 (drain status + rebalance read the staged class), #809 (the restore session
  fence + the nonce) and #810 (the durable fence-generation record + residue across generations)
  — `results/issue_664/split-proposal.md`. Both new edges are needed and neither is redundant:
  leg F reads the **generation record**, which is #810's, and #810 `Depends on` #809 so the fence
  itself comes transitively; #808 is independent of that chain (it only *conflicts* with it), so
  it must be named separately. The previous re-point stands below.
  **RE-POINTED 2026-09-12 — `637` → `663, 664`.** #637 was split into #661 → #662 → {#663, #664} (`results/issue_637/split-proposal.md`). A split parent reaches COMPLETE at its own sign-off, before any child lands (`src/pdca_harness/cli.py:1060-1073`), so the edge now names the last wave's two children, which carry #661 and #662 transitively. **wave 4, the top of the five-slice stack 634 → 635 → 636 → 637 → 508.**
  Both edges are genuine build-on dependencies: the verbs are served over **#636**'s protocol
  functions, and **#637** must be in the base or this slice's own tests would pass on a tree where
  a running custodian deletes the staged bytes the verbs create (and leg F reads the restore-fence
  generation #637 lands). #634 and #635 are transitive through #636.
  **A merge-order requirement that is NOT a batch dependency and must not be encoded as one:**
  0016 makes it **normative** that **#625 (the reaper) and #633 (the operator session-abort /
  terminal-expiry / foreign-clock alarm) land WITH OR BEFORE this slice** (`0016:234-268`) — the
  protocol has states whose only exit is the reaper, and the reaper's clock guard deliberately
  skips a foreign-clocked session, for which the operator verb is the *only* exit in the whole
  design. Neither is in this batch, so neither may appear in `Depends on` (the driver rejects a
  declared dependency that is neither in-batch nor COMPLETE, `src/pdca_harness/waves.py:57-83`).
  **`Conflicts with: 625, 633`** covers the *build* side of that: all three edit
  `crates/server/src/cli.rs` (this slice for `cmd_s3`'s new configuration surface, #625 for the
  custodian role's reaper wiring and one-shot, #633 for the operator verbs), so they must never
  share a wave. A conflict naming an out-of-batch bundle is simply dropped
  (`src/pdca_harness/waves.py:104-120`), so the edge is harmless in the five-id batch and load-bearing
  if all seven ever run together. Note the orientation the scheduler picks — name-lower first, so
  **#508 builds before #625/#633** — is correct for *building* (this slice's base is #636 + #637 and
  it drives no reaper) and says nothing about merging.
  **⚑ Human sign-off obligation: do not mark this slice's PR ready — and do not merge it — before
  #625 and #633 are merged.** Leg E's warn-level signal is the in-product mitigation, not a
  substitute for the ordering.
  ⚑ **And the two existing briefs are stale:** `results/issue_625/brief.md` declares
  `Depends on: 508` and `results/issue_633/brief.md` declares `Depends on: 508, 625`. Those were
  written against the pre-split plan, where #508 *was* the protocol. Under this split their real
  prerequisite is **#636**, and the #508 edge is backwards with respect to 0016's normative
  order. Re-point both before scheduling them.
  **RE-POINTED 2026-09-11 — `636` removed from `Depends on`.** #636 was DISCONTINUED at sign-off (2026-07-31) and re-sliced into #654–#660 (`docs/2026-07-31-alpha-reslicing-proposal.md`); #654 was then split into #691 (merged), #692 → #715/#716 (merged) + #717 → #771/#772, and #693. A `Depends on` id must reach COMPLETE before the driver schedules the dependent (`src/pdca_harness/waves.py:57-83`, `cli.py:_blocked_by`), and a DISCONTINUED bundle never does — so `636` was a permanent block, not a wait. Replaced by the protocol slices the verbs expose: #693, #656, #657, #658, #659 (all of 636.3–636.6 plus the answer table); #660 (seeded DST races) is scaffolding, not a prerequisite. #656–#659 have no bundle yet, so `pdca status` reads blocked-by them — that is the true state. **Re-pointed 2026-09-19:** #663 was split at its re-plan into #813 (scrub checks committed staged fragments; reconstruction keeps their repair queued) and #814 (reconstruction rebuilds a staged chunk); the field above names both in place of `663`.
  **Re-pointed 2026-10-15:** #656 was split at its re-plan into #894 (admission + Create, 636.3a) and #895 (Abort fence + shared fence batch, 636.3b, which depends on #894); the field above names both in place of `656`.
- **Surfaces:** data
- **Difficulty:** high
- **Do model:** opus-max
- **External dependencies:** `aws cli (S3 gateway round-trip)`, `typos`, `docs-renderer`
  <br>*(All three on the field's own line — the driver reads only that line. The AWS CLI is for
  the DEFERRED `aws s3 cp <8+GB>` round-trip of `Verification posture`, not for any Check leg;
  `typos` + `docs-renderer` because this slice's docs-currency edit is gated by the prose gates,
  which `cargo xtask ci` warn-skips when absent so a locally-green docs change opens the PR red
  (INTEGRATION §3). All three are installed on this host.)*
- **Test file:** `crates/server/tests/s3_multipart_upload.rs`, `crates/server/tests/s3_multipart_durability_gate.rs`
  <br>*(Both paths on the field's own line — the driver pulls every path token from that line,
  `src/pdca_harness/brief.py:200-212`.)* The first carries legs A, B, C and F; the second carries
  leg E and nothing else, in one test function under one process-wide capturing subscriber
  (see leg E). Leg E is alone because its telemetry assertions share gateway callsites with every
  other leg, and `tracing`'s process-global callsite cache would let those legs drop its counters
  (the #214 race). **Both NEW files** under a `tests/`
  directory: `run-verify.sh` classifies on an **added** `*/tests/*.rs`
  (`engine/scripts/run-verify.sh:144`, collected at `:392`), so a case appended to
  `crates/server/tests/s3_http_wire.rs` would degrade the gate to green-only and throw away the
  assertion red this slice can produce. (The former second file, `s3_put_object_sizing.rs`, went
  with leg D to #671.)
  **Salvage — explicitly permitted, and preferred over re-deriving:** this bundle preserves the
  seventh attempt's own test files at `results/issue_508/s3_multipart_upload.rs` and
  `results/issue_508/s3_multipart_lifecycle.rs`. They already encode the base-visible-symbols
  discipline, the in-process gateway harness, the hand-signed raw-request helpers, and the exact
  ETag oracle. Do MAY read them and salvage from them. Do MUST **not** carry over their
  **lifecycle** legs (GC / restore / scrub / reconstruction / rebalance interaction) — those are
  now **#637**'s and duplicating them here re-creates the cross-plane diff this re-plan exists to
  avoid.
- **Verification posture:** mixed, stated per leg.
  * **Legs A, B, E, F, H — DEFAULT**: red pre-fix, green post-fix at Check, in-process, on the
    resolved base (`Falsifiability` § *Base resolution*).
  * **Leg C — DEFAULT with a deadline, not a sleep.** Teardown is deliberately **off** the request
    path, so the post-Complete / post-Abort store observations must **poll to a deadline**, never
    assert at response time (asserting `count == 0` at response time would reject the conforming
    implementation and push teardown back onto the HTTP path) and never `sleep` past a lease TTL.
  * **DEFERRED, off-Check:** the issue's own headline acceptance —
    `aws s3 cp <8+GB file> s3://b/k` (auto-multipart) round-trips byte-identical (`sha256` match)
    — needs a running gateway, real disk and the AWS CLI. It is **not** reachable from
    `cargo xtask ci` and must not be faked with a smaller object dressed up as one. What IS built
    and exercised at Check is the whole path it would traverse (legs A–C over the in-process
    gateway, with a lowered `with_chunk_size` so the segmented and multi-part shapes are reached
    at test-sized objects); what is deferred is only the *scale*. **Named confirmer: Eduard Ralph
    at sign-off**, running `aws s3 cp` against a brought-up deploy stack on this host (aws-cli
    2.36.1 is installed; the doctor row is `aws cli (S3 gateway round-trip)`). Record the command
    and the `sha256` comparison in the sign-off notes.
- **Production reach:** the live path traverses every seam this slice builds, at Check — the
  tests drive real HTTP through the real router, the real SigV4 verifier, the real gateway and the
  real commit protocol. Nothing here is a seam ahead of its consumer; this **is** the consumer.
  The one thing the deployed system still lacks when this merges is the **reaper** (#625) and the
  **operator exit** (#633), which is why the merge-order obligation above is stated as a human
  gate rather than a code dependency.
- **Citations expected:** Do must cite `path:line` on the target branch for every change. **Peer
  callsites Do SHOULD open and mirror** (a deliberate, narrow exception to reading `brief.md`
  only):
  * `crates/gateway-core/src/lib.rs:710-834` — the **landed** `MultipartGateway` trait (#666,
    `b879e7b`) and its contract: answers are values, arguments arrive unvalidated, the part-number
    range check runs first, listings are complete and the wire layer paginates. Consume it; do not
    edit it (see `Scope` for the one allowed change).
  * `crates/server/src/lib.rs:799-883` — the composition-root impl whose six bodies this slice
    replaces (each says which issue replaces it: #668, #669, #670).
  * `crates/gateway-s3/src/lib.rs:227-229` — the `S3Gateway<G>` bound, which already includes
    `MultipartGateway`, and `:261` (`router`).
  * `crates/server/src/cli.rs:2658-2690` (`serve_s3`) — where leg E's variable is read and where
    the role's metrics dispatch is attached; `crates/server/tests/s3_gateway_cluster.rs:41` imports
    its sibling `serve_s3_role` from the same public module.
  * `crates/gateway-s3/src/lib.rs:335-346` (the denylist and **why** it exists), `:375-404`
    (`unsupported_subresource`, the raw-match residual, issue #491) and `:1690-1712` (the
    object-route guard) — the three sites leg B is about.
  * `crates/gateway-s3/src/lib.rs:1640-1665` — the bucket-route guard and its comment naming
    508's `ListMultipartUploads` as the reason `?uploads` is listed.
  * `crates/server/tests/s3_list_objects.rs:78-88` (`seed_bucket`) — the raw-record seeding idiom;
    `crates/server/tests/s3_http_wire.rs:996-1036` — the lengthless `aws-chunked` wire test this
    slice must leave green and unmodified.
  * The configuration surface leg E(0) needs is designed in leg E: `WYRD_S3_MULTIPART_REAPER_DEPLOYED`,
    checked in `cmd_s3` before it binds and in `serve_s3` before it serves, refused at load on
    any value but `true` / `false`. Mirror the existing chunk-size load-time check
    (`parse_s3_chunk_size`, `crates/server/src/cli.rs:2348`; the re-check in `serve_s3_role`,
    `:2477-2479`). It adds no chunk-size knob (leg D).
- **Prior-art check (triage cycles):** searched by affected file path across merged history and
  all PRs. `crates/gateway-s3/src/lib.rs` has never carried a multipart route: its history is the
  S3 wire surface (#364, PR #448 MERGED), the object-metadata model (#503, PR #594 MERGED), and
  the subresource-denylist hardening (#507). `git log -S"CreateMultipartUpload" --all` matches only
  the proposal. **No open PRs.** The rejected prior art is this bundle's own seven attempts,
  preserved at `results/issue_508/iteration-v1..v7/`; `results/issue_508/review-rejected.md` and
  `review-batch.md` record what was rejected **with reason** in v7 and must not be re-earned:
  the missing-reaper startup posture (a plan decision — leg E keeps it), a second timeout inside
  `core`'s fragment fan-out (the `ChunkStore` seam already bounds it,
  `crates/chunkstore-grpc/src/client.rs:169-190`), and a wall-clock bound inside the drain
  (`clippy.toml` denies a bare `SystemTime::now()`; bound the **work**, not the time).
  Interacting open issue: **#491** (percent-encoded subresource bypass) — still open, and leg B is
  the part of it this slice must not make worse.
- **Disposition hint:** likely-fix

Plan-review response (2026-10-15): all five findings hold at `7bf7eb9` and are revised in place.
(1) Restore gate: the landed record cannot tell this restore from an old image
(`multipart.rs:1937-1945`), so leg F now rests on 0016's deployment-order arm (`0016:3017-3021`),
keeps the gate as defence in depth (refuse while present and not complete), and flags the
missing restore-scoped signal for the human. (2) Abort oracle: the 204 is pinned on a planted
`Aborting` session, Abort after terminal delete answers 404, and `DELETE /b/k?uploadId=…` is
treated as the Abort shape with live, unknown and malformed ids pinned separately. (3) Leg E gets
its own binary (`s3_multipart_durability_gate.rs`), one test function, one process-wide
subscriber. (4) Leg D is removed: #671 owns single-PUT sizing and the human re-scoped it on
2026-10-14, because ~32.4 MiB chunks break the 4 MiB gRPC limit (`server/src/lib.rs:55-72`).
`Conflicts with` gains 671. (5) `Falsifiability` replaces the hypothetical integration base with
the ten successor ids, their state, the three with no bundle (#657, #658, #659), and the
condition under which RED becomes runnable; dependencies unchanged. Stale `run-verify.sh` claims
corrected. ⚑ Separately, the header flags that #666–#671 re-sliced this issue's whole scope.

Plan-review response (2026-10-20): all four findings hold at `12dd082` and are revised in place;
the scope question stays the human's. (1) Declaration key: named
`WYRD_S3_MULTIPART_REAPER_DEPLOYED`, read and checked in `serve_s3` (and in `cmd_s3` before it
binds), and both test files serve their gateways through `cli::serve_s3`, which is `pub` on
`main` (the cluster test already imports its sibling `serve_s3_role`), setting the variable with `std::env::set_var`, so they compile on the reverted base and
red on 501. No signature changes. (2) Leg B now requires every upload-id verb to check the
session's stored target and tests a live id on another key and another bucket: exact `404
NoSuchUpload`, session still usable. (3) Scope, `Design § In scope` and `Citations expected` now
use the landed seam (#666, `b879e7b`) instead of adding it. The header becomes a HOLD that maps
every leg to #667–#670 (all open sub-issues of #508; #666 and #671 closed) and recommends #508 as
the coordinating parent. That decision is left to the human. (4) New leg H: core multipart calls
that can send a commit run to their end after the request is dropped, no timeout wraps them, and
in-doubt errors answer `500 InternalError`. It is tested by dropping the future while a held
commit is pending, then checking the settling read happens after release. The prerequisite list
is updated: #894 and #895 have merged.

Plan-review response (2026-10-22): all three findings hold at `4bac538` and are revised in
place. (1) Leg H: a commit released as `Committed` makes core return with no further read
(`create.rs:462`, `abort.rs:327`), so the held commit is now released by applying its batch and
answering `CommitUnknownResult { may_still_commit: false }`, which forces core's settling read;
the oracle is a store call after release, plus a drop guard and the batch's store effect.
(2) Scope now requires a `MultipartRefusal::SessionTooLarge { bytes, limit }` arm (core's
refusal has no seam arm, and `EntityTooLarge` counts chunks), mapped to `400 MetadataTooLarge`,
and leg A asserts it with a 100,000-byte `Content-Type` and nothing written. (3) Leg B: every
multipart route refuses a decoded bucket containing `/` with `400 InvalidBucketName` before any
target match, tested with the `/b%2Fa/k` alias of a live upload on `/b/a/k`.

Plan-review response (2026-10-22, second pass): all four findings hold at `4bac538` and are
revised in place. (1) Leg H: Abort's `Aborting` state is transient once the drain runs, so the
wrapper now records the bytes its settling read of `mpu:<id>` returned and the test checks those
decode to `Aborting`; the store afterwards only has to show the session not `Open`. (2)-(4) Leg B
gains three regression guards for refusals the base gives only on routes this slice bypasses:
bulk delete keeps answering `501` to `uploads` / `uploadId` / `partNumber` in its query (it reads
the same list, `lib.rs:578-584`); Create refuses a key over 1,024 bytes with `400
KeyTooLongError` (the check is only in the `PUT` arm, `:1898-1905`), tested with multi-byte keys;
UploadPart with `x-amz-copy-source` answers `501` and leaves the staged part unchanged
(`:1853-1872`). Scope, Design and Impact updated to match. The HOLD stays the human's call.

## Motivation

S3's single `PutObject` ceiling is 5 GB, so objects above it are impossible without multipart —
and `aws s3 cp` and boto3 auto-switch to multipart above ~8 MB, so *default* clients fail on
ordinary large uploads today. Multipart is the biggest missing piece of the 0.1-Alpha S3 surface
(epic #513).

The reason this took eight planning passes is that multipart is not a wire feature: a client's
uploaded parts are durable bytes that no record class described, and every attempt that shipped
the verbs together with the machinery underneath produced a diff no one could review. The four
slices below this one exist so that **this** slice is what it should have been all along — a wire
surface over a settled protocol, whose diff is routing, XML, status codes and one companion trait.

## Design

Read 0016's verb × state table and the `PutObject` carve-out. What follows is only the scoping and
the composition decisions.

### In scope

* **`crates/gateway-core`** — **nothing to add.** The `MultipartGateway` companion trait landed
  with #666 (`crates/gateway-core/src/lib.rs:742`), in the ADR-0046 decision 6 / ADR-0010 shape
  this design asked for, and its streaming `upload_part` already keeps the "stream, don't
  buffer" invariant at the seam. The only changes allowed here are the two `Scope` names: the
  required `SessionTooLarge` arm and, optionally, an arm for the 503 gates.
* **`crates/server`** — replace the six `NotImplemented` bodies at the composition root
  (`crates/server/src/lib.rs:799-883`) with calls into the protocol functions, adding the
  target-identity check (leg B) and the cancellation ownership (leg H); the durability-plane
  declaration gate and alarm of leg E, read in `serve_s3`; the restore-image gate of leg F. No
  single-PUT change (leg D, #671).
* **`crates/gateway-s3`** — remove `uploads` / `uploadId` / `partNumber` from
  `UNSUPPORTED_SUBRESOURCES` (`:343-345`) **and add routing that covers every spelling those three
  keys can arrive in**, the S3 XML request/response bodies, and the status + error-code mapping of
  0016's table. The remaining denylist entries and both guards stay exactly as they are, and
  the bulk-delete route keeps refusing all three keys (leg B). The multipart routes also carry
  the `PUT` arm's key-length and copy-source refusals where leg B says.

### Out of scope — do not touch

* The commit protocol, the records, the ETag *computation*, admission control, the drain —
  **#636**. This slice calls them.
* The staged-byte protection class and every custodian pass — **#637**. Do **not** re-assert its
  legs here; the seventh attempt's `s3_multipart_lifecycle.rs` is exactly that duplication.
* The reaper loop and its windows — **#625**. The operator abort / terminal expiry /
  foreign-clock alarm — **#633**.
* `MetadataStore::scan_page` (#634) and the segmented chunk map (#635).
* Any file under `docs/design/adr/` or `docs/design/specs/`, and any edit to `0016`.

### The knob values

#636 chose `MAX_MAP_CHUNKS`, `MAX_SEG_CHUNKS`, `MAX_PART_CHUNKS`, `MAX_ROOT_SEGMENTS`,
`MAX_STAGED_CHUNKS`, `MAX_INFLIGHT_PARTS`, `R_publish` and `MAX_COMPLETE_ATTEMPTS` inside 0016's
settled ranges (`0016:1464-1474`); on `main` they are #655's (`4533549`). **This slice inherits
them; it does not re-choose them.** It owns no chunk-size knob: the single-PUT sizing 0016 assigned
to #508 (`0016:2836-2860`) is #671's, re-scoped on 2026-10-14 because its numbers cannot run on the
16 MiB transport-bounded chunk (leg D).

### Why the routing legs are enumerated rather than sampled

Removing three keys from a denylist changes the *default* for every form carrying them: what was
one refusal becomes a dispatch decision per shape. The base already demonstrates the failure mode
this creates — `?part%4Eumber=1` reaches the plain PUT arm and overwrites the object (issue #491's
residual, documented at `crates/gateway-s3/src/lib.rs:385-396` as acceptable *only because* the
raw key was denylisted). Once `partNumber` is a routed parameter that argument no longer holds, so
leg B enumerates each ill-formed spelling and pins its answer. This is the "*Protocol input*"
recurring defect class (`../wyrd/AGENTS.md:159-165`): torn, truncated or oversize input is an
error, never silently accepted.

## Alternatives considered

* **Six methods on `ObjectGateway`** — rejected: ADR-0046 decision 6 keeps that trait object-only
  and bucket-free, and `ContainerGateway` is the recorded precedent for a companion trait.
* **Buffering a part in memory to compute its digest before staging** — rejected by the seam's own
  streaming invariant (`crates/gateway-core/src/lib.rs:289-291`) and by the 0015 OOM cliff. Parts
  are up to `max_part_bytes` (165–381 MiB at the default chunk size); the digest is computed as
  the bytes stream.
* **Implementing the verbs together with the protocol** — attempts 1–7. Rejected at sign-off on
  reviewability; the split is the remedy.
* **A hard startup refusal when no reaper is configured** — see `Open questions` 1. The maintainer
  settled warn-level at the seventh attempt's sign-off; it is a one-line change if that has
  changed.

## Impact & compatibility

* **Client-visible, and all of it additive**: six new verbs, plus refusals that are ordinary
  S3 answers — `503 SlowDown` (admission), `400 EntityTooSmall` (a non-final part under 5 MiB),
  `400 EntityTooLarge` (a part past its ceiling), `400 MetadataTooLarge` (a Create whose session
  record would cross the value ceiling), `400 InvalidBucketName` (a multipart path whose decoded
  bucket contains `/`), `400 KeyTooLongError` (a Create on a key over 1,024 bytes), `501
  NotImplemented` (UploadPartCopy), and `503 ServiceUnavailable` from the two gates of legs E and F.
* **No behaviour change on an existing verb.** Single `PutObject`, GET and DELETE behave as on
  the base (single-PUT sizing is #671's), and so does bulk DeleteObjects: it still refuses a
  multipart selector in its query with `501` (leg B).
* **One operational requirement, stated in the docs:** after a metadata restore, the post-restore
  pass runs before any gateway is re-enabled (leg F).
* **Docs currency** (`../wyrd/AGENTS.md:154-157`): this slice adds **API operations**, so
  `docs/design/architecture/06-runtime-view.md` (and `03-context-and-scope.md` where the S3
  surface is enumerated, if it lists verbs) gain them **in this PR**. Three waves below already
  edited `06`/`08`; extend what they wrote.
* **`require-issue` / DCO / `adr-immutability`** are host-side and unchanged; the PR carries
  `Fixes #508`.

## Open questions

1. **The reaper-absent posture — SETTLED 2026-07-26 (Eduard Ralph): REFUSAL, gated on an explicit
   deployment declaration, plus the alarm.** Recorded in full so it is not re-litigated; this
   **supersedes** the warn-only posture of the seventh attempt's sign-off
   (`results/issue_508/review-rejected.md:8`), which was taken against a materially wrong estimate
   of the cost.
   **What changed:** the plan review established that `mpuctl.count` counts **every** `mpu:` record
   including `Completed` tombstones, decremented only in the terminal delete after `W_tombstone`
   (#625's) — `0016:348`, `:966-968`, `:2029-2031`. So with no reaper, **every successful upload
   permanently consumes a slot**, and `MAX_SESSIONS` is derived and small (≈19–79,
   `0016:2836-2860`). A reaper-less deployment therefore bricks multipart after a few dozen
   *successful* uploads, in ordinary use. The earlier reasoning assumed the fuse required
   *abandoned* uploads and that the retained population was bounded waste; both were wrong.
   **Why a configuration acknowledgement rather than a liveness probe:** there is no
   custodian-liveness observable to read — `crates/custodian/src/leadership.rs:22`,`:46` publish a
   fencing token, not a heartbeat — so a probe would have to be invented here, and a false positive
   would refuse uploads while the reaper was in fact running, converting a maintenance-plane gap
   into a data-plane outage. Local configuration is the one signal that is reliable.
   **The configuration input, settled here so Do does not invent it:** the environment variable
   **`WYRD_S3_MULTIPART_REAPER_DEPLOYED`** (`true` / `false`), **defaulting to NOT-declared** (fail
   closed — a deployment that has thought about nothing gets the safe answer), validated at
   configuration load beside the chunk-size check (leg E gives where and why it is a variable,
   not a flag). It is named for what the operator is asserting — that a custodian running the
   reaper is deployed — not for what it switches off. It gates **only** the multipart verbs.
   **Scope note:** this is a `crates/server` configuration + `crates/gateway-s3` refusal, both
   inside this slice. It is *not* the "specify it with #625's liveness contract" deferral an
   earlier revision of this question proposed — that deferral was written under the old cost
   estimate and is withdrawn.

2. **How `ListMultipartUploads` paginates.** S3's `key-marker` / `upload-id-marker` pair over a
   `mpu:` namespace bounded by `MAX_SESSIONS` is small enough to materialise, but the wire layer
   already owns pagination for `ListObjects` over a single sorted view (ADR-0046 seam decision,
   `crates/gateway-core/src/lib.rs:394-431`). Follow that precedent unless there is a reason not
   to, and say which you did.
3. **Whether the restore-fence generation (leg F) is readable by the gateway — ANSWERED
   2026-10-15.** It is readable (#810, `crates/core/src/multipart.rs:1925-1990`), but read alone
   it cannot tell this restore's completion from one the image carried (`:1937-1945`). Leg F
   therefore rests on the deployment order and keeps the gate as defence in depth only. Making the
   gate authoritative needs a restore-scoped signal that no issue covers yet (⚑ in leg F).

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts. **Additionally, and specifically for this
slice:** 0016 requires #625 and #633 to land with or before it (`0016:234-268`), so
the human's ready-mark and merge are gated on those two, not only on this bundle's
own sign-off.
