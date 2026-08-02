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
> Citations re-verified against `origin/main` @ `22d71b4` on 2026-07-26.

- **Slug:** multipart-s3-verbs
- **Kind:** enhancement (design proposal)
- **Goal:** the S3 multipart verb set — `CreateMultipartUpload`, `UploadPart`,
  `CompleteMultipartUpload`, `AbortMultipartUpload`, `ListParts`, `ListMultipartUploads` — served
  over the protocol #636 landed, so `aws s3 cp` of a large file works with default settings.
  Today every multipart form is hard-refused **501 NotImplemented** by the subresource denylist
  (`crates/gateway-s3/src/lib.rs:343-345`, object-route guard `:1696-1709`, bucket-route guard
  `:1650-1662`), so objects >5 GB are impossible and `aws s3 cp` / boto3 — which auto-switch to
  multipart above ~8 MB — fail on ordinary large uploads. Plus the half of 0016 decision 7 that
  lands at the wire: an ordinary `PutObject` never segments, so it must stay inside the flat-map
  ceiling by **chunk-size selection**.
- **Success criterion:** two **NEW** test files, both driving an **in-process gateway over the
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
  `UploadPart`/`Complete`/`ListParts` after Abort → **404 `NoSuchUpload`**; a second Abort →
  **204** (idempotent); an identical Complete retry inside the tombstone window → **200** + the
  recorded ETag; a Complete reusing the upload id with a **different** part list → **404
  `NoSuchUpload`** (the `complete_fingerprint` rule); wrong part list → **400 `InvalidPart`** /
  `InvalidPartOrder` **without publishing**; non-final part < 5 MiB → **400 `EntityTooSmall`**;
  part number outside `1..=10000` → **400 `InvalidArgument`**; a part whose chunk count exceeds
  `MAX_PART_CHUNKS` → **400 `EntityTooLarge`** with the session still usable; `GET /bucket?uploads`
  on an absent bucket → **404 `NoSuchBucket`**; a Complete body that is not well-formed XML, or
  over the size cap → **400 `MalformedXML`**; a create refused by admission → **503 `SlowDown`**.
  **(B) Routing safety — the new surface must not be evadable** (same file, issue #491's
  interaction). Each ill-formed multipart form answers **400 `InvalidArgument`**, never a 2xx
  that overwrites or deletes the object: `PUT /b/k?partNumber=1` (no uploadId), a non-numeric
  `partNumber`, `PUT /b/k?uploadId=U` (no partNumber), `PUT /b/k?uploads`,
  `DELETE /b/k?uploadId=U`. And the percent-encoded form **`PUT /b/k?part%4Eumber=1`** must answer
  **400**, not a plain object PUT: `unsupported_subresource` matches **raw** keys (the residual is
  documented at `crates/gateway-s3/src/lib.rs:385-396`) while SigV4 canonicalisation
  decodes-then-re-encodes, so on the base that form reaches the plain PUT arm and answers
  **200, silently overwriting the object**. Removing `partNumber`/`uploadId`/`uploads` from the
  denylist must not leave that hole behind, and a multipart form carrying a **still-denylisted**
  subresource in its percent-encoded spelling
  (`PUT /b/k?partNumber=1&uploadId=U&t%61gging=1`, `GET /b?uploads&%61cl`) must still answer
  **501 `NotImplemented`** naming that subresource.
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
  **(D) Decision 7's `PutObject` carve-out — pin the SIZING, not a round-trip**
  (`crates/server/tests/s3_put_object_sizing.rs`). Segmentation is multipart-only, so an ordinary
  PUT stays flat by chunk-size selection. A byte-identical round-trip is already green on the base
  for both shapes, so a round-trip oracle proves nothing here. Assert instead: (i) a
  **declared-length** PUT whose size would need more than `MAX_MAP_CHUNKS` chunks at the default
  chunk size publishes a **flat** map whose chunk count is **≤ `MAX_MAP_CHUNKS`** — read the count
  off the published `inode:` record's raw bytes — which is false on the base, where the count grows
  with the object. **A `≤` bound alone does not pin the algorithm**: an implementation that always
  used `chunk_size_max` would satisfy it while wasting memory on every small object. So also assert
  the **selection itself** — for at least two declared lengths straddling the threshold, the
  observed chunk size equals
  `max(DEFAULT_CHUNK_SIZE, ⌈Content-Length / MAX_MAP_CHUNKS⌉)` **exactly**, and an object below the
  threshold still uses `DEFAULT_CHUNK_SIZE`; (ii) a **lengthless `aws-chunked`** PUT of the same body publishes a
  chunk count consistent with the **size-independent** selection `⌈5 GiB / MAX_MAP_CHUNKS⌉`
  (`x-amz-decoded-content-length` is optional by design,
  `crates/gateway-s3/src/sigv4.rs:579-584`; the base has a passing lengthless wire test at
  `crates/server/tests/s3_http_wire.rs:996-1036`); (iii) a declared-length PUT that cannot fit
  `MAX_MAP_CHUNKS` even at `chunk_size_max` is refused **400 `EntityTooLarge`** (the base answers
  200). **Declared non-C4:** the *configuration-load* refusal when
  `chunk_size_max < ⌈5 GiB / MAX_MAP_CHUNKS⌉` cannot be reached from a base-compiling test (the
  knob does not exist on the base), so it ships as a unit test beside the config code —
  `cargo xtask ci` evidence, not C4-verify evidence.
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
  Assert: that the metric and the audit event are emitted (read back in-process with a capturing
  subscriber / `gather_prometheus`, the idiom `crates/custodian/tests/gc_telemetry.rs` already uses
  — and in its **own test binary**, since `tracing` callsite caches are per-process, the #214
  discipline); that a second emission follows the first without a restart; and — **the negative
  case, without which the oracle is vacuous** — that a deployment which HAS declared the durability
  plane emits **no** such signal. An implementation that alarms unconditionally satisfies every
  positive assertion while telling the operator nothing, and a permanently-firing alarm is
  indistinguishable from no alarm at all.
  **(F) Multipart is refused on a restored image until the restore fence has run — CONDITIONAL on
  #637 having landed the observable.** 0016 decision 2 makes this the gateway's obligation: "the
  restore fence generation MUST complete before any gateway serves multipart verbs on the restored
  image" (`0016:836-841`) — otherwise a client's retried Complete can fence a still-resurrected
  `Open` session and publish over reclaimed bytes. Assert: with the generation marker absent/stale,
  `CreateMultipartUpload` and `CompleteMultipartUpload` are refused with a stated S3 error; once
  the generation is observed complete, they serve.
  **Ownership settled 2026-07-26: #637 ships the record, this slice CONSUMES it.** Its leg C2
  lands a single durable, authoritative restore-fence generation/completion record that
  distinguishes absent / in-progress / complete and identifies which restore it belongs to. This
  slice reads **that** record and nothing else — it MUST NOT write a generation counter of its own
  or infer completion from session states, which would be exactly the split-authority defect this
  stack keeps rejecting. If the record is somehow absent on this bundle's base, that is a Check §6
  item to raise, not a gap to fill locally.
  **(G) `cargo xtask ci` green**, and the off-Check large-object round trip — see
  `Verification posture`.
- **Falsifiability:** RED is producible **in-process on this bundle's own base** — the folded
  `origin/pdca-integration/main` carrying #634, #635, #636 and #637 — with **no deploy stack, no
  container and no cluster**, and it is an **assertion** red, not a build error. Leg A reds
  because every object-scoped multipart form is refused 501 by `unsupported_subresource`
  (`crates/gateway-s3/src/lib.rs:385-404`, guard `:1696-1709`, tokens `:343-345`) and
  bucket-scoped `GET /b?uploads` by `unsupported_subresource_decoded` (`:1650-1662`) — and because
  leg A asserts **exact status + code**, an already-501 form still reds (501 is neither the
  required 400 nor the required 404). Leg B's `?part%4Eumber=1` form reds the *other* way: on the
  base it answers **200 and overwrites the object**. Leg D reds on the chunk count. Legs C, E and
  F red on their own observables.
  **Corollary the test files MUST obey:** neither added file may reference **anything this slice
  adds** — not a Rust symbol, not a crate newly added to a `Cargo.toml`. `run-verify.sh`'s RED leg
  reverts modified production files and keeps the added tests, and its failure path has **no
  zero-test guard** (the `TESTS_RAN == 0` check at `engine/scripts/run-verify.sh:416-427` sits
  inside the cargo-*succeeded* branch; a non-zero exit falls through to the unconditional `PASS`
  at `:433`), so a compile error would be scored as a red over a run that executed nothing.
  Everything the tests need is present on this bundle's base: `aws-sdk-s3`, `aws-smithy-*`,
  `aws-credential-types` and `tempfile` are already dev-dependencies of `wyrd-server`
  (`crates/server/Cargo.toml`), `Gateway::new` / `with_chunk_size`
  (`crates/server/src/lib.rs:95`,`:129`), `RedbMetadataStore::in_memory`, `MetadataStore::scan`,
  `WriteBatch`, and — from the waves below — #636's key helpers and #637's custodian surface.
  Where an observation is not reachable with base-visible symbols, **seed or read raw record
  bytes** through the retained store handle exactly as `seed_bucket` does
  (`crates/server/tests/s3_list_objects.rs:78-88`).
  **Do MUST record in `build-notes.md`, from the RED leg, how many tests actually ran and
  failed**, and confirm the failures were assertions. A build-error red here is a defect in the
  test, not a pass.
  **Base resolution is a gate-evaluability precondition.** This is a **wave-4** bundle; the
  driver stamps its stack base and `pdca gates` exports `$PDCA_VERIFY_BASE =
  origin/pdca-integration/main` (`src/pdca_harness/flow.py:459`,
  `src/pdca_harness/gates.py:352-360`), honoured by `run-verify.sh` ahead of the brief's base
  (`:186-206`). Without it the gate resets to `origin/main`, where none of the four prerequisite
  slices exist and the test files cannot compile.
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
- **Scope:** the six S3 multipart verbs and their routing in `crates/gateway-s3`, the
  `MultipartGateway` companion trait in `crates/gateway-core`, its implementation at the
  `crates/server` composition root over #636's protocol, `PutObject` chunk-size selection
  (0016 decision 7's carve-out), the reaper-absent startup signal, the restore-fence gate, and
  the architecture-doc currency edit. **Out of scope:** the commit protocol, records, ETag
  *computation*, admission control and drain (#636); the staged-byte protection class and every
  custodian pass (#637) — do **not** re-assert its legs here; the reaper (#625); operator abort /
  terminal expiry (#633); `scan_page` (#634) and the segmented chunk map (#635); any file under
  `docs/design/adr/` or `docs/design/specs/`, and any edit to `0016`.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** 636, 637
- **Conflicts with:** 625, 633
- **Ordering note:** **wave 4, the top of the five-slice stack 634 → 635 → 636 → 637 → 508.**
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
- **Surfaces:** data
- **Difficulty:** high
- **Do model:** opus-max
- **External dependencies:** `aws cli (S3 gateway round-trip)`, `typos`, `docs-renderer`
  <br>*(All three on the field's own line — the driver reads only that line. The AWS CLI is for
  the DEFERRED `aws s3 cp <8+GB>` round-trip of `Verification posture`, not for any Check leg;
  `typos` + `docs-renderer` because this slice's docs-currency edit is gated by the prose gates,
  which `cargo xtask ci` warn-skips when absent so a locally-green docs change opens the PR red
  (INTEGRATION §3). All three are installed on this host.)*
- **Test file:** `crates/server/tests/s3_multipart_upload.rs`, `crates/server/tests/s3_put_object_sizing.rs`
  <br>*(Both paths on the field's own line — the driver parses only that line,
  `src/pdca_harness/brief.py:23-31`, `:101-113`.)* The first carries legs A, B, C, E and F; the
  second leg D. **Both NEW files** under a `tests/`
  directory: `run-verify.sh` classifies on an **added** `*/tests/*.rs`
  (`engine/scripts/run-verify.sh:92-94`, `:300-311`), so a case appended to
  `crates/server/tests/s3_http_wire.rs` would degrade the gate to green-only and throw away the
  assertion red this slice can produce. Two files, not one, so a `tracing` callsite-cache
  interaction in leg E cannot mask leg D (the #214 discipline).
  **Salvage — explicitly permitted, and preferred over re-deriving:** this bundle preserves the
  seventh attempt's own test files at `results/issue_508/s3_multipart_upload.rs` and
  `results/issue_508/s3_multipart_lifecycle.rs`. They already encode the base-visible-symbols
  discipline, the in-process gateway harness, the hand-signed raw-request helpers, and the exact
  ETag oracle. Do MAY read them and salvage from them. Do MUST **not** carry over their
  **lifecycle** legs (GC / restore / scrub / reconstruction / rebalance interaction) — those are
  now **#637**'s and duplicating them here re-creates the cross-plane diff this re-plan exists to
  avoid.
- **Verification posture:** mixed, stated per leg.
  * **Legs A, B, D, E, F — DEFAULT**: red pre-fix, green post-fix at Check, in-process.
  * **Leg C — DEFAULT with a deadline, not a sleep.** Teardown is deliberately **off** the request
    path, so the post-Complete / post-Abort store observations must **poll to a deadline**, never
    assert at response time (asserting `count == 0` at response time would reject the conforming
    implementation and push teardown back onto the HTTP path) and never `sleep` past a lease TTL.
  * **DEFERRED, off-Check:** the issue's own headline acceptance —
    `aws s3 cp <8+GB file> s3://b/k` (auto-multipart) round-trips byte-identical (`sha256` match)
    — needs a running gateway, real disk and the AWS CLI. It is **not** reachable from
    `cargo xtask ci` and must not be faked with a smaller object dressed up as one. What IS built
    and exercised at Check is the whole path it would traverse (legs A–D over the in-process
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
  * `crates/gateway-core/src/lib.rs:388-431` (`ContainerGateway`) — **the composition pattern to
    mirror exactly**. ADR-0046 decision 6 / ADR-0010: `ObjectGateway` stays object-only, and a new
    concern arrives as a **narrow companion trait** speaking its own vocabulary, implemented by
    `wyrd-server` at the composition root and projected by the S3 crate alone. Multipart is that
    shape: a `MultipartGateway` companion trait, **not** six methods bolted onto `ObjectGateway`.
  * `crates/server/src/lib.rs:580-600` — where `ContainerGateway` is implemented for `Gateway`,
    the composition root the multipart impl joins.
  * `crates/gateway-s3/src/lib.rs:160-175` — the `impl<G> S3Gateway<G> where G: ObjectGateway +
    ContainerGateway` bound the new trait joins, and `:196-205` (`router`).
  * `crates/gateway-s3/src/lib.rs:335-346` (the denylist and **why** it exists), `:375-404`
    (`unsupported_subresource`, the raw-match residual, issue #491) and `:1690-1712` (the
    object-route guard) — the three sites leg B is about.
  * `crates/gateway-s3/src/lib.rs:1640-1665` — the bucket-route guard and its comment naming
    508's `ListMultipartUploads` as the reason `?uploads` is listed.
  * `crates/server/tests/s3_list_objects.rs:78-88` (`seed_bucket`) — the raw-record seeding idiom;
    `crates/server/tests/s3_http_wire.rs:996-1036` — the lengthless `aws-chunked` wire test leg D
    must not break.
  * `crates/server/src/lib.rs:51` (`DEFAULT_CHUNK_SIZE`) and `:95`,`:129` (`Gateway::new`,
    `with_chunk_size`) — the sizing knobs of leg D. **Note the shape problem Do must solve rather
    than invent around:** `with_chunk_size` is a **per-Gateway** builder setting, while leg D needs
    a **per-request** chunk size derived from each PUT's declared `Content-Length`. Do MUST
    introduce that per-request seam explicitly and cite it, not thread a mutable knob through the
    shared gateway (which would make concurrent PUTs race on each other's chunk size). Likewise the
    configuration surface leg E(0) and leg D's `chunk_size_max` precondition need — a load-time
    validated config, reachable from `cmd_s3` — is **new** and must be designed here, not left to
    the builder to improvise: name the keys, state where they are validated, and state what a
    deployment that fails validation does (refuse at load, never mid-stream).
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

* **`crates/gateway-core`** — a **`MultipartGateway` companion trait**, mirroring `ContainerGateway`
  (`crates/gateway-core/src/lib.rs:388-431`): narrow, container/object-vocabulary, no HTTP and no
  concrete backend. ADR-0046 decision 6 and ADR-0010 make this the shape; six methods bolted onto
  `ObjectGateway` would widen a trait that is deliberately object-only. Every method that carries
  a body **streams** (the "stream, don't buffer" invariant at the seam,
  `crates/gateway-core/src/lib.rs:289-291`).
* **`crates/server`** — the implementation at the composition root, over #636's protocol
  functions, beside the existing `ObjectGateway` / `ContainerGateway` impls
  (`crates/server/src/lib.rs:270`, `:580`); the `PutObject` chunk-size selection of leg D; the
  reaper-absent startup signal of leg E; the restore-fence gate of leg F.
* **`crates/gateway-s3`** — remove `uploads` / `uploadId` / `partNumber` from
  `UNSUPPORTED_SUBRESOURCES` (`:343-345`) **and add routing that covers every spelling those three
  keys can arrive in**, the S3 XML request/response bodies, and the status + error-code mapping of
  0016's table. The remaining denylist entries and both guards stay exactly as they are.

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
settled ranges (`0016:1464-1474`). **This slice inherits them; it does not re-choose them.** What
it does own is `chunk_size` selection at the wire (leg D) and the configuration precondition
`chunk_size_max ≥ ⌈5 GiB / MAX_MAP_CHUNKS⌉` for accepting lengthless `aws-chunked` PUTs, checked
**at configuration load** with a header-time refusal — never a mid-stream failure
(`0016:2836-2860`, the lengthless row).

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

* **Client-visible, and all of it additive**: six new verbs, plus three refusals that are ordinary
  S3 answers — `503 SlowDown` (admission), `400 EntityTooSmall` (a non-final part under 5 MiB) and
  `400 EntityTooLarge` (a part or a single PUT past its ceiling).
* **One behaviour change on an existing verb**: an ordinary `PutObject` of a large object now
  selects a larger chunk size (leg D). Objects already stored are untouched; the change is in how
  *new* large single PUTs are chunked, and it is what keeps their flat map inside the value
  ceiling. A lengthless `aws-chunked` stream uses a fixed, size-independent chunk size — small
  lengthless PUTs therefore use larger chunks than they need (read amplification, not
  correctness), which is a registered accepted cost (`0016:2836-2860`).
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
   **The configuration input, settled here so Do does not invent it:** a single boolean-ish key on
   the `wyrd s3` role's existing configuration surface, **defaulting to NOT-declared** (fail
   closed — a deployment that has thought about nothing gets the safe answer), validated at
   configuration load beside `chunk_size_max`'s own precondition (leg D). Name it for what the
   operator is asserting — that a custodian running the reaper is deployed — not for what it
   switches off. It gates **only** the multipart verbs.
   **Scope note:** this is a `crates/server` configuration + `crates/gateway-s3` refusal, both
   inside this slice. It is *not* the "specify it with #625's liveness contract" deferral an
   earlier revision of this question proposed — that deferral was written under the old cost
   estimate and is withdrawn.

2. **How `ListMultipartUploads` paginates.** S3's `key-marker` / `upload-id-marker` pair over a
   `mpu:` namespace bounded by `MAX_SESSIONS` is small enough to materialise, but the wire layer
   already owns pagination for `ListObjects` over a single sorted view (ADR-0046 seam decision,
   `crates/gateway-core/src/lib.rs:394-431`). Follow that precedent unless there is a reason not
   to, and say which you did.
3. **Whether the restore-fence generation (leg F) is readable by the gateway on #637's shape.**
   The requirement is 0016's (`0016:836-841`) but the record is #637's. If #637 did not land an
   observable generation marker, leg F is a **§6 item for this bundle**, not something to invent
   here — say so rather than fabricating a second source of truth.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts. **Additionally, and specifically for this
slice:** 0016 requires #625 and #633 to land with or before it (`0016:234-268`), so
the human's ready-mark and merge are gated on those two, not only on this bundle's
own sign-off.
