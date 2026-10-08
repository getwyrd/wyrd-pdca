# Brief — issue 741 / validate-s3-client-layer

> Plan artifact (docs 02 §PLAN). Do reads ONLY this file (plus the peer callsites cited
> under **Citations expected**). The `- **Label:** value` lines are parsed by the driver.
>
> Plan of record: `docs/design/proposals/draft/0017-blackbox-validation-tool.md` §2
> (layering: `client.rs` is I/O only), §9 (the blackbox lint — normal deps, not dev) and
> §Dependencies. Read in place in the target checkout — never copied here.
>
> **The dependency decision this slice turned on was taken by the maintainer in the Plan
> session for this batch (2026-08-17): promoting `aws-sdk-s3` to a shipped dependency is
> ACCEPTED, including the RUSTSEC-2026-0253 exposure that comes with it.** Do proceeds; the
> audit and the corrected waiver rationale are deliverables of this slice, not preconditions
> for it. See Design § "The dependency move". Do must NOT re-open the decision, and must NOT
> stop on it.
>
> **Where that record lives, stated precisely because it matters.** The acceptance is a
> SESSION record — this brief is its artifact. It is NOT in the tracker thread: issue #741's
> only comment (eduralph, 2026-08-16) is the blocker that says the audit is needed "**before**
> the crate lands" and "Confirm before starting", and re-checking the live issue on
> 2026-08-17 shows no later comment. That comment is what was answered, by the person who
> wrote it, in session. Two consequences, neither of them Do's to resolve: (1) sign-off §9
> must confirm the acceptance and should mirror one sentence onto issue #741 so the tracker
> stops disagreeing with the plan; (2) if the maintainer does NOT confirm at sign-off, this
> is an unresolved human-only item (INTEGRATION §4) and the slice is rejected, not patched.

- **Slug:** validate-s3-client-layer
- **Track:** blackbox
- **Kind:** enhancement (design proposal)
- **Goal:** `wyrd-validate` gains the S3 client every later slice calls through: an
  `aws-sdk-s3` client pointed at an arbitrary `--endpoint` with path-style addressing and
  static SigV4 credentials, a typed error surface that names status, S3 `Code` and
  `x-amz-request-id` instead of a stringified blob, and object bodies that stream in both
  directions rather than being held whole.
- **Defect:** (framed as the gap) `crates/validate` after #740 parses arguments and echoes
  them; it cannot speak to an endpoint at all. The client layer was implicit in the original
  slicing and therefore in nobody's brief, which is why #741 exists as a named slice.
- **Success criterion:** BINDING (demonstrable by C4-verify at Check, in-process, no
  container, no live cluster): against a Wyrd S3 gateway served in-process over a loopback
  listener from the crate's own dev-dependencies —
  1. one object **round-trips** PUT → GET → DELETE through the client layer, byte-identical
     on the way back, and a subsequent GET reports the typed not-found error;
  2. an **error response surfaces as a typed value** carrying its HTTP status, its S3
     `<Code>`, and the `x-amz-request-id` the gateway stamps
     (`crates/gateway-s3/src/lib.rs:1552-1557`) — asserted field by field, never by
     substring-matching a `Display` string;
  3. a **multi-chunk object streams, in BOTH directions, each with its own
     aggregation-sensitive oracle** — not a process RSS measurement, and not an assertion
     that merely *permits* a buffering implementation. Route the fixture's traffic through a
     tiny in-process loopback **interposer** (a TCP relay between the SDK client and the
     gateway's listener that counts bytes per direction and can hold the tail back); it is
     dev-only test code, and it is what makes "the client did not aggregate" observable:
     * **PUT** — the payload is produced by a generator that is never fully materialised.
       When the generator is asked for its FINAL chunk, the interposer must ALREADY have
       forwarded bytes to the gateway (assert `> 0`, with a payload large enough — tens of
       MiB — that no socket or SDK window can account for the whole body). A client that
       buffers the body before sending forwards nothing until the generator is exhausted, so
       this fails.
     * **GET** — the interposer forwards a prefix of the response and then WITHHOLDS the
       tail. The client must yield at least one body chunk to the test while the tail is
       withheld; only then is the tail released and the read completed, asserting
       byte-identity with what was PUT. A client that collects the whole response before
       yielding produces nothing during the hold and fails (as a timeout with a stated
       message, not a hang — bound the wait).
     Do MUST demonstrate both mutations: temporarily buffer the PUT body, record that the
     PUT assertion fires; temporarily collect the GET body, record that the GET assertion
     fires; revert both, and report both in `build-notes.md`. If Do finds a cleaner oracle
     that is equally aggregation-sensitive in both directions, it may substitute it — and
     must say in `build-notes.md` what it is and why it is decisive. A weaker oracle (chunk
     counts, largest-buffer-seen alone) is NOT an acceptable substitute for the GET leg: the
     "collect, then re-chunk small" implementation passes it.
- **Falsifiability:** RED is producible on the ordinary developer harness Do is pointed at
  — `cargo test -p wyrd-validate --test <name>`, no Docker, no MinIO, no FDB. All three legs
  fail pre-fix by criterion-absence (the client does not exist), and criterion 3 is
  additionally *demonstrably* red rather than merely absent, in each direction separately:
  the interposer oracles above fail when the implementation is changed to buffer (PUT) or to
  collect (GET), which Do confirms by making each mutation in turn and recording that the
  corresponding assertion fires (report both in `build-notes.md`; ship neither).
  KNOWN GATE SHAPE, pre-declared: `C4-verify` classifies a discriminator on an ADDED
  `*/tests/*.rs` (`engine/scripts/run-verify.sh:141-144`) and takes its `GREEN_ONLY` branch
  when that test's crate has no pre-patch `Cargo.toml` (`run-verify.sh:412-414`). Which
  branch this bundle gets therefore depends on its BASE, and the base is `main` PLUS wave
  0's accepted work folded in — not `origin/main` as it stands today, where `crates/validate`
  does not exist (root `Cargo.toml`'s `[workspace] members` has no such entry, verified). If
  #740 was accepted and folded, the crate exists pre-patch, so the red leg will run,
  revert `crates/validate/src/**`, keep the test — and the test will fail to COMPILE, because
  every symbol it calls is one this patch introduces. That is scored `UNVERIFIABLE`
  (exit 77 → SUMMARY §6 NEEDS-HUMAN, non-gating — `run-verify.sh:201-215`), which is the
  correct verdict for net-new coverage and must not be mistaken for a defect. The gating
  evidence is `C4-ci` plus the three assertions above. If instead the crate is ABSENT from
  the base — #740 was not accepted, or its fold did not happen — the gate takes the
  `GREEN_ONLY` branch, which is not the failure: **the failure is building at all.** See the
  Ordering note's first instruction.
- **Invariant to restore:** *The validator never holds an object whole, in either
  direction.* Stated over the category — every transfer the client layer performs, PUT and
  GET alike — because the tool's entire reason for existing is objects larger than the
  memory of the machine driving them, and a buffered path that happens to work at test sizes
  is the bug that only appears at hour 40 on a real substrate. Source: Wyrd's own
  "stream, don't buffer" invariant, which the gateway side already holds and states —
  `crates/gateway-s3/src/lib.rs:12-17` and `crates/core/src/write.rs:526-529` ("Peak resident
  bytes are one `chunk_size` piece plus its fragments, independent of object size", closing
  the `0015:789` OOM cliff). The client must not be the end that reintroduces it.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** 775
- **Ordering note:** **RE-POINTED 2026-08-18 (maintainer-approved): this was `Depends on:
  740`; #740 was SPLIT and no longer builds anything.** #740 decomposed into #773 (the
  `h2`/RUSTSEC-2026-0258 lockfile bump that unblocks the gating `C4-ci` row for the whole
  batch) → #774 (creates `crates/validate`, its lib/bin split and the CLI surface) → #775
  (adds the no-`wyrd-*` dependency lint). A split parent never reaches COMPLETE, so a
  surviving `Depends on: 740` would have made `_runnable` (`flow.py:702`) skip this bundle
  outright. It now names **#775**, the last link in that chain, which transitively carries
  #774's crate and #773's green base.
  **FIRST, before anything else: verify the base.** `ls crates/validate`
  and `grep -n validate Cargo.toml` on the checkout you are handed. This bundle is scheduled
  after #775 by `Depends on: 775`, and the wave fold puts the ACCEPTED diffs of #773/#774/#775
  on the base this builds against — that is the mechanism, and it does not require a human
  merge. But
  it is contingent: on `origin/main` today `crates/validate` does not exist, and if #774 was
  rejected, iterated past this run, or otherwise not folded, the crate will not be there. In
  that case **STOP and report it** — do not create the crate, the manifest, the lib/bin split
  or the lint here. Those are #774's and #775's deliverables; re-creating them would duplicate
  a slice,
  break the batch's file-disjointness claim, and produce a patch nobody planned. A held
  dependent is a resolvable state (`pdca flow 741` once #775 lands); a silently widened one
  is not.
  #774 creates `crates/validate` and its lib/bin split, #775 adds the no-`wyrd-*`
  dependency lint; this slice adds modules to that crate and its first normal third-party
  dependency, so it must build on their accepted result — the wave after #775's.
  After the re-point this bundle no longer shares a wave with anything — the driver's own
  scheduler now puts the batch at `[736, 773] → [738, 774] → [775] → [741] → [742]`, so this
  is alone in its wave. The disjointness note below is kept because it still constrains a
  re-run with different membership (this touches
  `crates/validate/**`, root `Cargo.toml` `[workspace.dependencies]`, `deny.toml`; #738
  touches `crates/server/**`, `deploy/**`, `xtask/tests/dist_templates.rs`). One SEMANTIC
  overlap is avoided by construction rather than by a declared conflict: #738 changes
  `wyrd_server::cli::serve_s3_role`'s signature, so this slice's fixture is specified below
  to compose the gateway directly (`Gateway::new` + `S3Gateway::serve`) and MUST NOT call
  `serve_s3_role`. Honour that and the two cannot collide.
- **Surfaces:** data
- **Difficulty:** high
- **Sizing note:** the driver's structural sizer bands this `oversized` (score 6:
  `difficulty=high`, brief length, "structurally predicts a large patch"). Looked at. The
  slice HAS already been cut once, on the maintainer's own advice — the 5 GiB
  memory-property leg is out of scope above — and it is not cut again. The remaining
  candidate line is "client + typed errors" | "streaming", which would ship a buffered
  client first and rewrite it second, in direct contradiction of this brief's stated
  invariant; a temporarily-wrong shape in a foundation layer is what gets grandfathered.
  The `high` rating is real and is the routing signal it is meant to be: the diff is
  moderate but it moves ~100 crates into the shipped dependency graph, which is what a
  reviewer must hold in view. One slice.
- **Scope:** (a) `aws-sdk-s3` wiring against an arbitrary `--endpoint`: path-style
  addressing, explicit region and `s3` service scope, static credentials from #740's
  resolution, no anonymous path, retries and stalled-stream protection off by default so a
  failure is reported rather than papered over; (b) SigV4 header auth over plain HTTP (see
  Impact — the TLS limit is stated, not fixed here); (c) a typed error surface mapping S3
  XML error bodies onto values carrying status, `Code` and `x-amz-request-id`; (d) streaming
  in both directions — a PUT chunked as it is generated, a GET read incrementally; (e) the
  ADR-0003 §2 three-test dependency audit written into `build-notes.md`, and the
  RUSTSEC-2026-0253 waiver rationale in `deny.toml` corrected (see Design — it currently
  asserts something this slice makes false).
  **/ out of scope, and deliberately so:** the **5 GiB memory property** from the issue's
  definition of done — the maintainer's own review calls this slice oversized and names that
  leg as the one to split out (issue #741 comment, 2026-08-16), it is blocked on #635, and
  its home is #761, whose definition of done already says "Resident set stays flat
  across a 5 GiB transfer"; a TLS client (blocked on a rustls crypto-provider
  license decision, `crates/gateway-s3/src/lib.rs:50-57`); the capability matrix and its
  expected-failure assertions (#743); the oracle, payload derivation and size classes
  (#744, #746); workflows, pools, scenarios, verdict; MinIO and the corrupting proxy (#751,
  #758); any change to the Wyrd gateway itself; **any edit under `crates/server/`**,
  including deduplicating its inline `aws-sdk-s3` dev pin into `[workspace.dependencies]` —
  see Impact, that cleanup is a follow-up and touching it here would break this bundle's
  file-set disjointness from #738 in the same wave.
- **Repro instruction:** On `main` at `65ca4fd` with #740 applied: `crates/validate` builds
  and its binary echoes its configuration, and `grep -rn "aws" crates/validate/` returns
  nothing — the tool cannot reach an endpoint. `grep -n "aws-sdk-s3" crates/server/Cargo.toml`
  shows it at line 126 under `[dev-dependencies]` (`:108`), annotated at `:120-121`
  "Dev-only — dev-dependencies are not compiled into the production binary".
- **External dependencies:** none — base Rust toolchain only. This is deliberate and is a
  scope constraint on Do: the binding criterion is met against a Wyrd gateway served
  **in-process** from the crate's dev-dependencies, so no Docker, no MinIO container, no
  live `wyrd s3` process and no network are required to make it go red→green. If Do finds
  itself reaching for a container or an external service, the fixture design is wrong —
  stop and declare it rather than working around it with a curated fixture or a code-read.
- **Test file:** `crates/validate/tests/s3_client_roundtrip.rs` — a NEW file.
- **Verification posture:** DECLARED, not the default. This is net-new coverage: there is no
  prior failing assertion to flip, so "red" for criteria 1 and 2 is criterion-ABSENCE, and
  the `C4-verify` red leg will report `UNVERIFIABLE` for the compile reason recorded under
  Falsifiability. What IS built and exercised at Check: the whole client layer, driven
  end-to-end against a real Wyrd S3 wire surface in the same process — not a mock, not a
  recorded fixture, not a code-read. What is *demonstrated* rather than resting on
  non-existence: criterion 3's two interposer oracles, one per direction (Do buffers the PUT
  and records that the PUT assertion fires; collects the GET and records that the GET
  assertion fires; reverts both). Nothing in this slice is deferred to a later environment; what
  is deferred is out of SCOPE (the 5 GiB property), which is a different thing and is named
  as such above.
- **Production reach:** The live path traverses the seam at Check — the test drives the real
  `aws-sdk-s3` client over a real TCP listener into the real `S3Gateway`. Two honest limits
  to record in `build-notes.md` rather than leave for a reviewer to find: the transport is
  **plain HTTP**, which is not how the endpoint is deployed (§10 and blueprint §B.1 put an
  operator's TLS terminator in front of it), so nothing this slice verifies covers the TLS
  path; and the in-process gateway is the `redb` + `mem` + local-FS composition, not the
  production `fdb` + `etcd` one — adequate for a *client*-layer criterion, which is about
  the wire, but say so.
- **Citations expected:** Do must cite `path:line` on the target branch for every change.
  Peer callsites Do MAY open and should mirror:
  * **The SDK client configuration, already written for exactly this endpoint shape** —
    `crates/server/tests/s3_gateway_cluster.rs:99-114` (`sdk_client`: `behavior_version_latest`,
    `region`, `endpoint_url(format!("http://{addr}"))`, static `SdkCredentials`, an explicit
    `http_client`, `force_path_style(true)`, retries and stalled-stream protection
    disabled). This is the composition to copy. Do not re-derive it: a client that leaves
    retries on turns a real failure into a timeout, and one that omits `force_path_style`
    tries virtual-host addressing against an endpoint that has no DNS for it.
  * **The in-process fixture** — `crates/server/tests/s3_http_wire.rs:56-64`
    (`build_gateway`: `Gateway::new(RedbMetadataStore, FsChunkStore, MemCoordination)` over a
    tempdir, with a deliberately small chunk size so a modest object spans several chunks)
    and the `S3Gateway::new(gateway, config).serve(listener)` pattern it drives. Compose the
    gateway this way. **Do NOT call `wyrd_server::cli::serve_s3_role`** — its signature is
    changing in the same wave (#738) and this slice must not depend on it. One number NOT to
    copy: that fixture passes `with_chunk_size(8)` (eight BYTES) to force multi-chunk
    coverage on a tiny object. The streaming legs use a tens-of-MiB payload, where 8-byte
    chunks would mean millions of chunks and a test that never finishes — compose the
    streaming fixture with a realistic size (the 1 MiB default, or 256 KiB to keep the object
    multi-chunk at a smaller payload) and say which you chose. The interposer
    sits between the client's `endpoint_url` and this listener: bind a second loopback
    listener, `tokio::spawn` a relay per connection copying each direction with a counter
    (and, for the GET leg, a release gate on the server→client half), and point the client at
    the relay's address. It is ~40 lines of dev-only code and it is the whole reason
    criterion 3 can distinguish streaming from re-chunking.
  * **The error facts to surface** — `crates/gateway-s3/src/request_id.rs:37-39`
    (`pub const HEADER: &str = "x-amz-request-id"`) and the stamp point
    `crates/gateway-s3/src/lib.rs:1552-1557`. The typed error must carry that id from the
    header when present; the SDK exposes it, so do not parse it out of a message.
  * **The dependency-wall frame this slice moves inside** — `deny.toml:1-18` (the header:
    "THIS FILE GUARDS THE DEFAULT FEATURE GRAPH — the artifact we ship") and `deny.toml:77-86`
    (the RUSTSEC-2026-0253 waiver whose stated rationale this slice invalidates).
  * **How a new dependency is adopted in this repo** — the audit prose on `roxmltree`
    (root `Cargo.toml:131-139`) and on `hmac` (`:88-98`): licence, unsafe posture, transitive
    surface, why it is the right crate. Write the `aws-sdk-s3` audit to that standard.
  * **The lint this crate lives under** — #740's new invariant in `xtask/src/repo_guard.rs`.
    `aws-sdk-s3` is a **normal** dependency and is fine; `wyrd-*` crates may appear ONLY
    under `[dev-dependencies]`. Proposal 0017 §9: "Normal, not total: `cargo metadata` walks
    dev-dependencies too, and the §14 fixtures are dev-only."
- **Prior-art check (triage cycles):** searched by affected path on `main` at `65ca4fd`.
  `git log --oneline -- crates/validate` → empty. `gh pr list --state all --search
  "wyrd-validate"` → only PR #765, the merged proposal 0017 document. The `aws-sdk-s3`
  dependency itself has history worth knowing rather than prior art to reuse: issue **#726**
  (CLOSED) waived RUSTSEC-2026-0253 in `deny.toml` on the explicit ground that `lru` reaches
  the tree "only as a DEV-dependency … never in shipped code" — a rationale this slice makes
  false. No open or closed PR has attempted to promote `aws-sdk-s3` to a normal dependency.
- **Disposition hint:** new-feature

## Motivation

Every later slice — the capability matrix, the oracle, the workflows, every scenario —
calls S3 through this layer. It was implicit in the original slicing, which is exactly why
it needs to be explicit now: an implicit client layer gets written three times, differently,
inside the first three slices that need it.

The typed-error half is not polish. The tool's whole job is to say *what it expected and
what it got*; a matrix row that declares `unsupported(#508)` must assert a specific status
and a specific `<Code>`, and it cannot do that against a stringified blob. The
request-id half is what makes a client-side failure joinable to the server's own record of
it (#529's whole point).

## Design

### The client

One `aws_sdk_s3::Client`, built exactly as `crates/server/tests/s3_gateway_cluster.rs:99-114`
builds it: `endpoint_url` from `--endpoint`, `region` from `--region`, static credentials
from #740's resolution, `force_path_style(true)`, plain-HTTP `http_client`, retries and
stalled-stream protection **disabled**. The last two are a deliberate default for a
validator: an SDK that silently retries turns an availability failure into a latency number
and loses the event the tool exists to record. If a retry policy is wanted later it belongs
in the scenario layer, above this one, where it can be counted.

Per proposal 0017 §2, `client.rs` is in the **I/O only** column: no decision-shaped logic
lives here. Error *classification* (what a status and `Code` mean for a matrix row) belongs
in the pure layer that #743 adds; this slice's job is to hand that layer facts instead of
prose.

### The typed error

A single error type carrying: the HTTP status, the S3 `<Code>` when the body has one, the
`<Message>`, and the `x-amz-request-id`. Constructed from the SDK's structured error rather
than by re-parsing XML the SDK has already parsed. Where the SDK does not model an error
(a gateway response with a non-S3 body, a transport failure), the type must still say
*which* of those it was — an "unknown" variant that swallows the distinction between "the
server said something we did not understand" and "we never reached the server" is exactly
the blob this criterion rejects.

### Streaming, and how it is asserted

A PUT is fed from a body source that produces buffers as they are demanded; a GET is read
chunk by chunk from the response body. Neither path materialises the object.

The assertion matters as much as the implementation, and it has to be **aggregation
sensitive in each direction separately** — an oracle that a "collect it all, then hand it
over in small pieces" implementation passes is not evidence of streaming, it is evidence of
chunking. Process RSS is not a usable oracle in a test either (the allocator does not return
pages, other tests share the process, the number is noisy).

So criterion 3 is written against ORDERING, observed at the wire by a small in-process
interposer — a TCP relay the fixture puts between the SDK client and the gateway's listener,
counting bytes each way and able to hold the response tail back. Streaming and buffering
differ in *when* bytes cross that point, and that difference is decisive where a size or a
buffer-count is not:

* **PUT** — a streaming client has already put bytes on the wire while the generator is
  still producing; a buffering one has put none until the body is complete. The test records
  the interposer's client→server count at the moment the generator is asked for its last
  chunk and asserts it is non-zero, with a payload (tens of MiB) far larger than any socket
  or SDK window that could otherwise explain a zero.
* **GET** — the interposer forwards a prefix and withholds the tail; a streaming client
  yields a body chunk anyway, a collecting one cannot. Bound the wait so the failure is a
  message rather than a hang, then release the tail and finish the read, asserting the bytes
  came back identical.

State the chosen oracle in the test's module docs, and prove each one by mutation (buffer
the PUT, collect the GET) before shipping neither.

This is also where the slice was narrowed. The issue's "a 5 GiB payload streams without the
process resident set tracking object size" is the leg the maintainer's review names as the
one to split out; it is blocked on #635, it needs minutes of wall-clock and gigabytes of
scratch inside `cargo xtask ci`, and it measures RSS, which the paragraph above rejects as
an oracle. The *property* is kept and asserted at a size the gate can afford; the
large-object proof belongs to #761 with its own substrate.

### The dependency move — decided, and what it obliges

`aws-sdk-s3` is currently dev-only: `crates/server/Cargo.toml:126` under `[dev-dependencies]`
(`:108`), annotated at `:120-121` "dev-dependencies are not compiled into the production
binary". This slice makes it a **normal** dependency of a shipped workspace member, which
brings roughly a hundred crates inside the frame of a file whose own header says it "guards
the default feature graph — the artifact we ship" (`deny.toml:1-18`).

INTEGRATION §4 names "any new dependency or license (the ADR-0003 three-test audit +
`deny.toml` allowlist)" a human-only item, and **the maintainer has taken it: accepted,
2026-08-17, in the Plan session for this batch.** So the question Do faces is not *whether*
— it is to produce the record that a taken decision is supposed to leave behind. Three
deliverables, none of them optional on the grounds that the call is already made:

* write the ADR-0003 §2 three-test audit into `build-notes.md` — licence (Apache-2.0 across
  the aws crates), unsafe posture, transitive surface, maintenance — to the standard the
  `roxmltree` and `hmac` adoption notes set (root `Cargo.toml:131-139`, `:88-98`). The
  audit covers the aws crates specifically; `tokio` (which the SDK needs a runtime from)
  and `futures-util` are already vetted workspace dependencies and need no new audit, but
  say in the notes which of the crate's normal dependencies are new to the shipped graph
  and which merely gain a second consumer;
* pin the versions in `[workspace.dependencies]` (root `Cargo.toml`) rather than inline in
  `crates/validate/Cargo.toml`, matching how every other shared third-party crate is pinned.
  Pin the SAME versions `crates/server` already carries inline
  (`crates/server/Cargo.toml:126-129`: `aws-sdk-s3 1.137.0`, `aws-smithy-http-client 1.1.13`,
  `aws-smithy-runtime-api 1.12.3`, `aws-smithy-types 1.5.0`) so the graph resolves one copy —
  but do NOT edit that file to point at the workspace pin (out of scope, see Impact);
* **correct the RUSTSEC-2026-0253 waiver's rationale.** `deny.toml:77-86` waives it on the
  stated ground that `lru` "reaches us only as a DEV-dependency (`wyrd-server (dev) ->
  aws-sdk-s3 -> lru`), never in shipped code". After this slice that sentence is false, and
  a waiver whose justification is false is worse than no waiver. Mechanically nothing breaks
  — `cargo deny` ignores are keyed by advisory id and apply to the whole graph, so `cargo
  deny check` stays green either way (verified: the entry is already in the shipped-graph
  `deny.toml`, not only in `deny-all-features.toml`) — which is precisely why this must be
  corrected deliberately rather than discovered by a red gate. State the new exposure
  plainly: an unsound `LruCache::pop()` now sits in a shipped binary's dependency graph,
  still requiring a key type with a panicking `Drop` under `catch_unwind` to reach, still
  not how the SDK uses its cache, and still unfixable from here because `aws-sdk-s3` pins
  `lru ^0.16.3` (awslabs/aws-sdk-rust#1451). Record that this exposure was **accepted by the
  maintainer on 2026-08-17**, so a later reader sees a decision rather than an oversight —
  and leave the existing REMOVAL TRIGGER intact ("when a Dependabot `aws-sdk-s3` bump lifts
  `lru` past 0.18.2, this entry goes stale and cargo-deny warns `advisory-not-detected` —
  delete it then"), which is now the only thing that retires it.
  Write the rationale as it will read to someone who was not in the room: the old sentence
  is not merely edited, it is *replaced*, because "never in shipped code" was the whole
  justification and it no longer holds.

### The test fixture, and why a dev-dependency on Wyrd is allowed

`crates/validate` takes `wyrd-server`, `wyrd-gateway-s3`, `wyrd-metadata-redb`,
`wyrd-chunkstore-fs`, `wyrd-coordination-mem`, `tokio` and `tempfile` as
**dev-dependencies** and serves a gateway in-process. Proposal 0017 §9 permits exactly this
— "Normal, not total: `cargo metadata` walks dev-dependencies too, and the §14 fixtures are
dev-only. Nothing that ships in the binary may reach a workspace crate; what the test
harness links is unconstrained." #740's lint is written to that rule; this slice is its
first real exercise, and a run of `cargo xtask ci` that stays green with these dev-deps
present is a genuine check that the lint distinguishes the two kinds.

## Alternatives considered

**Hand-roll the S3 client (as the gateway's own tests do over a raw `TcpStream`).** It would
avoid the dependency decision entirely, and it is wrong for this tool: the tool's claim is
"a real client can drive Wyrd", and a bespoke client proves only that our client can drive
our server. Proposal 0017's alternatives section already rejected the reverse (Python +
boto3) on load-generation grounds; the Rust SDK is the compromise that keeps a real client
without an interpreter.

**Depend on `wyrd-gateway-s3::sigv4` for signing.** It is already written, tested against
AWS's published worked example, and in the tree. It is also a `wyrd-*` crate, so linking it
into the binary is exactly the edge #740's lint forbids and the blackbox property exists to
prevent.

**Test against a MinIO container instead of an in-process Wyrd gateway.** That is #751, and
it answers a different question ("is our reading of S3 right?"). It also needs Docker inside
`cargo xtask ci`, which this brief's External-dependencies constraint rules out for the
binding criterion.

**Spawn a real `wyrd s3` process for the round-trip.** Closer to the deployed shape, and it
buys nothing here that the in-process listener does not — the wire is identical, the
composition is the same, and a child process adds lifecycle flakiness and a dependency on
the `wyrd` binary's argument surface, which #738 is changing in the same wave.

**Keep the 5 GiB leg in scope.** Rejected above and by the maintainer's own review.

## Impact & compatibility

The shipped dependency graph grows by roughly a hundred crates, all of which are already IN
the graph `cargo deny` walks today (as dev-dependencies of `wyrd-server`), so no *new*
licence appears — what changes is ADR-0003's judgment, which is about *linked* crates. Build
times for a full workspace build are unchanged for the same reason; `crates/validate`'s own
build is not.

No existing crate's behaviour changes, and **no file under `crates/server/` is touched** —
which is a scope rule here, not an observation. `crates/server`'s dev-dependency on
`aws-sdk-s3` (pinned inline at `crates/server/Cargo.toml:126-129`) stays exactly as it is.
Deduplicating the two pins into `[workspace.dependencies]` would be tidy and is explicitly
NOT done here: it would edit `crates/server/Cargo.toml`, contradicting this bundle's stated
file set and its disjointness from #738's, for a cleanup the criterion does not need. This
slice pins its OWN aws versions in `[workspace.dependencies]` (root `Cargo.toml`) and lets
`crates/server` keep its inline dev pin; converging the two is a follow-up.

The transport limit is real and should be stated in the verdict wherever this layer's
capability is described: until the rustls provider decision is resolved
(`crates/gateway-s3/src/lib.rs:50-57`), the tool validates plain HTTP with static
credentials, which is not the configuration operators are told to deploy. Proposal 0017's
§Dependencies now says so; this slice does not change it.

## Plan-review response (#301 revision pass)

Four findings; three revised, one kept with its provenance corrected.

* **"The brief assumes a base that does not exist."** Half right, and the half that is right
  is now fixed. The mechanism is correct — `Depends on:` schedules this into a later wave and
  the wave fold puts the prerequisite's accepted diff on the base, which is precisely why the
  field exists and why waiting for a human merge is not required. *(Written when the field
  read `Depends on: 740`; #740 was split on 2026-08-18 and the field now reads
  `Depends on: 775` — see the Ordering note. The reasoning is unchanged, only the id.)*
  What was wrong is that the brief
  asserted the crate "DOES exist pre-patch" as a fact. It is contingent, and the contingency
  now leads the Ordering note: verify the base first; if `crates/validate` is absent, STOP
  and report — do not create #740's deliverables here. Falsifiability says the same about
  which `C4-verify` branch to expect.
* **"The dependency acceptance has no tracker evidence."** Correct and worth being exact
  about: re-checked the live issue on 2026-08-17 — the only comment is the 2026-08-16 blocker
  ("the audit **before** the crate lands", "Confirm before starting"), and nothing later. The
  acceptance is a SESSION record by the same maintainer, and the header now says so instead
  of implying a tracker decision, with two consequences named: sign-off §9 must confirm it
  and should mirror it onto the issue; if it is not confirmed, the slice is rejected rather
  than patched. The decision itself stands — it was taken by the person entitled to take it.
* **"Criterion 3 cannot falsify the GET direction."** Correct, and it was the weakest part of
  the brief: both permitted observables watched the PUT side, so "collect the response, then
  yield it in small pieces" passed. Criterion 3 now specifies a loopback interposer and one
  aggregation-sensitive oracle per direction (PUT: bytes on the wire before the generator
  finishes; GET: a chunk delivered while the tail is withheld), plus a required mutation
  demonstration for EACH direction.
* **"The optional dedup contradicts the file-set claim."** Correct — `crates/server`'s pins
  are inline (`crates/server/Cargo.toml:126-129`), so the cleanup necessarily edits the file
  set the brief called disjoint. Removed: `crates/server/` is now explicitly out of scope,
  this slice pins the same versions in `[workspace.dependencies]` for its own use, and
  converging the two is a follow-up.

## Open questions

1. ~~**Is the RUSTSEC-2026-0253 exposure acceptable in a shipped binary?**~~ **ANSWERED —
   yes, accepted (maintainer, 2026-08-17).** Kept here as the record of what was asked and
   settled: the alternatives were to vendor a patched `lru` or hand-roll the client, and
   both were declined in favour of proceeding. Nothing about this is left for sign-off
   except confirming Do wrote the audit and replaced the waiver rationale.
2. **Where the aws pins live** — `[workspace.dependencies]` (preferred; one pin, no drift
   with `crates/server`'s dev-dependency) or inline in `crates/validate/Cargo.toml`.
3. ~~**The 5 GiB leg's home.**~~ **ANSWERED — #761, which already owns it verbatim.**
   Checked rather than assumed: #761 ("validate: the large-object scenario") lists
   "Memory behaviour under streaming: resident set must not track object size" under *What
   it targets*, and its definition of done says "Resident set stays flat across a 5 GiB
   transfer". So the leg cut from here was never homeless — it was always #761's, and no
   new issue is needed. (#744 owns the neighbouring but distinct concern: that 5 GiB *is*
   one of the size classes, and that payloads at that size are derived rather than stored.)
   The property left this slice because it needs an environment `cargo xtask ci` does not
   have — RSS is not a usable oracle in a shared test process — and #761 runs against a real
   substrate where it is. **Tracker updated 2026-08-17:** #761 now declares
   `Depends on #759, #635`, the cross-milestone edge it was missing — a 5 GiB object needs
   segmented chunk maps to exist at all (at the 1 MiB default its root is ~13.5x the
   50,000-byte `MAX_ROOT_VALUE_BYTES` ceiling), and raising `--chunk-size` only clears it at
   >=16 MiB, which would measure a configuration nothing else uses. Arithmetic in the issue
   comment.
4. **Does the typed error need to distinguish "no `<Code>` in the body" from "no body"?**
   Probably yes for the matrix's `unexpected-support` class; cheap to model now, awkward to
   retrofit.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR
MAY happen during the cycle. The PR MUST NOT be marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 1): rebuilding for the implementation-level findings — C5 Causal adequacy — Correct and regression-test the SDK-to-wire error interpretation — treating all SDK service metadata as received S3 facts fabricates an absent code and loses unreadable-response diagnostics (`crates/validate/src/client.rs:241`, `crates/validate/src/client.rs:249`, `reviewer-probes.log:21`, `reviewer-probes.log:27`).; T5 Judgment — Add shipped deadline-expiry, truncated-GET and failing/length-mismatched PUT regressions — the new bounded-wait and integrity guarantees currently lack those tests, and relevant guard-removal mutants survive (`crates/validate/src/client.rs:224`, `crates/validate/src/client.rs:320`, `crates/validate/src/client.rs:393`, `gate-logs/C5-mutants.log:18`, `gate-logs/C5-mutants.log:20`).; C4-verify — The frozen log shows four green tests followed by compile-only pre-fix failure, exactly as independently reproduced; sign-off must accept the explicitly declared new-API proof posture (`gate-logs/C4-verify.log:10`, `gate-logs/C4-verify.log:277`, `brief.md:185`).; `crates/validate/src/client.rs:249-252`: an empty-body 404 is reported as a `<Code>` the server never sent. Reproduced: a loopback server answering `404, Content-Length: 0, x-amz-request-id: …` gives `Service{status: 404, code: Code("NotFound"), message: None}` for both GET and DELETE. The SDK makes this code up for any empty 404 (`aws-sdk-s3-1.148.0/src/protocol_serde.rs:24-28`), and `describe` trusts `meta.code()` before it checks `has_body`. That breaks the type's own rule (`crates/validate/src/error.rs:61`, "The body carried this `<Code>`") and the brief's Open Question 4 (keep "no body" apart). The result should be `ErrorCode::NoBody`. This confirms T4's finding with a reproduction.; `crates/validate/src/client.rs:241-262`: a success response the SDK cannot parse comes back as an S3 *error response*, and the only diagnostic is thrown away. Reproduced: `200 OK, Content-Length: 5, Last-Modified: not-a-date`, body `hello`, gives `Service{status: 200, code: NoBody, message: None}`. That is three wrong facts. It is a success status filed as an error response. It says "no body" when 5 bytes arrived (the SDK's deserializer had already swapped the body out, so `bytes()` returns `None`). And it drops the SDK's "Failed to parse LastModified from header" (`aws-sdk-s3-1.148.0/src/protocol_serde/shape_get_object.rs:136`), because `describe` reads only `meta()`. `error.rs:23-24` names this exact case ("a malformed header") as `Unreadable`, and `docs/design/architecture/05-building-block-view.md:253` claims the two are kept apart. This is the defect a validator most needs to name: a gateway emitting a timestamp that fails its grammar is a class the rubric lists. The same arm turns an HTML 500 into `Service{500, NoCodeInBody, request_id: None}` (T4's finding). Fix: route the operation's `Unhandled` service errors, and any 2xx, to `Unreadable`, and keep `DisplayErrorContext` as `detail`.; `crates/validate/src/client.rs:326-333`: a torn GET body with no declared length is silently accepted as a complete, shorter object. Reproduced: `200 OK` with no `Content-Length`, no chunked encoding, `Connection: close`, body `hel`, then close, gives `content_length() == None`, then `"hel"`, then `Ok(None)`. No error at any point. The rubric's protocol-input rule says torn or truncated input must be an error or indeterminate, never silently accepted. The doc at `client.rs:317-319` promises "never a shorter object" but only checks it when a length was declared. S3 GetObject always declares `Content-Length`, so a body framed only by connection close should be refused, or at least flagged so the layer above cannot mistake it for a clean read.; `crates/validate/tests/s3_client_roundtrip.rs:328-336` (and `:252`): criterion 2 asks for "the `x-amz-request-id` the gateway stamps", but the tests only check the format (32 lowercase hex characters), and the PUT receipt only checks `is_some()`. Mutation: replace `client.rs:258-261` with `request_id: Some("0123456789abcdef0123456789abcdef".to_string())`. All 4 tests still pass. So the "asserted field by field" claim is not backed for this field. Fix: the relay already sees the response bytes, so have it capture the `x-amz-request-id` header and assert the typed error's id equals it.; root `Cargo.toml:92` (audit prose for the shipped dependency): it says "no rustls", but the `default-client` feature chosen at `Cargo.toml:100` pulls `rustls-native-certs`, `rustls-pki-types` and, on Unix, `openssl-probe` into the shipped graph (`cargo tree -p wyrd-validate -e normal`). The `rustls` crate itself is not pulled in, and `cargo deny` passes, so this is only an accuracy fix to the transitive-surface line of the audit the brief asked for. Name these crates.; `crates/validate/src/client.rs:241`: An SDK `ServiceError` does not guarantee successfully parsed S3 XML. As recorded in the frozen T4 evidence, the SDK also wraps XML/header parsing failures in operation-level unhandled service errors. This unconditional conversion turns malformed error responses into `Service(NoCodeInBody)` and discards the parsing diagnostic. Preserve these as `Unreadable` with status, request ID, and cause; add a raw-response regression distinguishing malformed XML from valid XML without `<Code>`.; `crates/validate/src/client.rs:249`: Checking `meta.code()` before body presence invents a received S3 code for an empty HTTP 404: the SDK synthesizes `NotFound` in that case (also identified by the frozen T4 evidence). The result becomes `Code("NotFound")` rather than `NoBody`, breaking the distinction the public error type promises. Check body absence before accepting SDK metadata and cover an empty 404 with field-by-field assertions.; `crates/validate/src/client.rs:321` and `crates/validate/src/client.rs:407`: The four new integration tests never exercise deadline expiry, truncated GET responses, or short, oversized, and failing PUT sources. The frozen mutation run confirms that deleting the short-body checks survives. The GET test's outer timeout tests streaming progress, not the client's timeout classification. Add focused failure-path tests asserting `Timeout` phase and `Body` variants, including a PUT whose excess arrives in a separate chunk, so these new guarantees cannot silently regress.; `crates/validate/tests/s3_client_roundtrip.rs:175`: The relay discards its listener and both pump task handles; the gateway does the same at `crates/validate/tests/s3_client_roundtrip.rs:102`. Dropping the fixture leaves helpers and sockets alive until runtime teardown, and the gateway can outlive its temporary storage. This violates the standing abort-on-drop convention. Own the tasks in fixture guards, including child pumps, and abort them when the fixture drops.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 9 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_741/review-b. 6 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 89 mutants tested in 3m: 14 missed, 19 caught, 56 unviable
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 9 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_741/review-b
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 2): rebuilding for the implementation-level findings — C4 Verification (red→green) — Accept the declared new-API proof posture — no behavioral pre-fix test can run against the absent API; restored production passes all 16 integration tests and full CI, and both buffering mutations fail their intended oracles (`reviewer-red.log:10`, `reviewer-restored-green.log:25`, `reviewer-ci.log:3896`, `gate-logs/C4-verify.log:339`).; C5 Causal adequacy — Complete malformed-response discrimination — an unclosed XML document still becomes an authoritative S3 code, so the correction does not yet preserve the promised error facts (F1; `target/crates/validate/src/client.rs:345`, `reviewer-probes.log:8`).; T5 Judgment — Add regressions for complete-HTTP-but-truncated XML and early-success PUT responses — current malformed-response and bad-source tests miss both independently failing cases (`target/crates/validate/tests/s3_client_roundtrip.rs:681`, `target/crates/validate/tests/s3_client_roundtrip.rs:985`, `reviewer-probes.log:27`).; `crates/validate/src/client.rs:184-190`: **a PUT whose source fails is reported as a success** when the server answers before reading the whole body. The `fault` slot is checked once, right after `send()` resolves. hyper hands the response back as soon as the response head arrives and keeps polling `SizedBody` in the background, so a fault that comes later is never read. Reproduced: the server reads only the request head, then sends `200 OK, Content-Length: 0` and stops reading. (a) Declared 64 MiB, generator still producing: `Ok(Receipt{request_id: "early", e_tag: "\"x\""})`, with only 2.8 MiB of 64 MiB produced. (b) Declared 10 bytes, source yields `hello`, then `Err` after 500 ms: `Ok(Receipt{request_id: "early"})`. Case (b) breaks the method's own contract at `client.rs:148-150` ("a body that ends short … or yields an error fails the PUT with `S3Error::Body`"). It also breaks the claim at `docs/design/architecture/05-building-block-view.md:253` ("refused, never stored as a different object"). An early ack is exactly the gateway bug a validator exists to catch, and the rubric's *absent entries* rule forbids silent success. Fix: have `SizedBody` record a clean end (`Ready(None)` with `remaining == 0`). If the SDK returns `Ok` before that flag is set, return `S3Error::Body` with the response's request id. Add a scripted early-200 regression next to `a_put_source_that_is_short_long_or_failing_is_refused_and_nothing_is_stored` (`crates/validate/tests/s3_client_roundtrip.rs:985`). That test cannot see this case, because the Wyrd gateway always reads the whole body before it answers.; `crates/validate/src/client.rs:345-351`: **truncated or malformed error XML comes back as a clean S3 error with a code.** `is_error_document` checks only the root start tag, and the SDK's reader accepts a missing close tag. Reproduced on DELETE, 503, with a matching `Content-Length`: `<Error><Code>SlowDown</Code>` gives `Service{503, Code("SlowDown"), message: None}`. `<Error><Code>SlowDown</Code><Message>half` gives `Service{…, message: Some("half")}`, so a message cut off mid-text is reported as if it were complete. `<Error><Code>NoSuchKey</Code></Error><junk` gives `Service{Code("NoSuchKey")}`. This contradicts the patch's own rule at `client.rs:306-307` ("an `<Error>` document the SDK failed partway through reading — is `Unreadable`"), which its test enforces only for the bad-entity shape (`s3_client_roundtrip.rs:738`). It also breaks the rubric's *protocol input* rule (truncated input is never silently accepted). Fix: in `is_error_document`, read the whole document to the end, require the root to close, and allow nothing but whitespace or comments after it. Add these three bodies as `Unreadable` regressions.; `crates/validate/src/client.rs:349`: `is_error_document` checks only the root's opening tag. A response containing `<Error><Code>NoSuchKey</Code>` with a matching HTTP `Content-Length` passes this check; the SDK also tolerates the missing closing tag, so `service_error` reports `Service(Code("NoSuchKey"))` instead of `Unreadable`. The frozen T4 evidence identifies this case; the shipped truncation test only exercises an HTTP length mismatch. Validate the complete XML document before trusting SDK metadata, and add a regression for malformed XML inside a complete HTTP body. The workspace already provides a full XML validator, `roxmltree` (`Cargo.toml:171`).; `crates/validate/src/client.rs:190`: PUT returns a successful receipt whenever the SDK succeeds and the fault slot is empty, but that slot does not establish that the source finished. An endpoint returning HTTP 200 before consuming the upload can leave the source incomplete without any recorded fault, producing false success. `SizedBody` checks length only when polled and records no successful EOF (`crates/validate/src/client.rs:471`). Track clean EOF with the declared length satisfied and require it before returning a successful receipt. Add a scripted early-200 regression with a source whose tail remains pending; the existing PUT failure tests use a gateway that consumes the upload.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_741/review-b. 8 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_741/review-b
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: Slice is oversized and not converging: 3 builds, blocking findings 13 -> 8 -> 9, patch 101 KB (over the 100 KB backstop). Every round fixes one set of hostile-response edge cases and the review finds the next. Core is sound: PUT/GET/DELETE round-trip, typed errors, and both-direction streaming oracles hold (both buffering mutations go red). Split in re-plan (pdca split 741): (1) client core + typed errors + streaming, as proven; (2) hostile-response hardening — byte cap on error bodies before SDK aggregation (768 MiB error body -> 794 MiB peak), declared Content-Length enforced on PUT/DELETE error responses, upload cancellation that releases the source/connection under write backpressure (disputed repro: confirm on a large-frame peer that stops reading), and the chunked-GET refusal / x-amz-request-id precedence calls. Brief correction: the RUSTSEC-2026-0253 waiver is moot. The lockfile already resolves aws-sdk-s3 1.148.0 -> lru 0.18.4, so the waiver deletion and the >=1.144.0 floor are correct. The tracker note must say the advisory is not in the graph, not that an exposure was accepted.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Slice is oversized and not converging: 3 builds, blocking findings 13 -> 8 -> 9, patch 101 KB (over the 100 KB backstop). Every round fixes one set of hostile-response edge cases and the review finds the next.
  Core is sound: PUT/GET/DELETE round-trip, typed errors, and both-direction streaming oracles hold (both buffering mutations go red).
  Split in re-plan (pdca split 741): (1) client core + typed errors + streaming, as proven; (2) hostile-response hardening — byte cap on error bodies before SDK aggregation (768 MiB error body -> 794 MiB peak), declared Content-Length enforced on PUT/DELETE error responses, upload cancellation that releases the source/connection under write backpressure (disputed repro: confirm on a large-frame peer that stops reading), and the chunked-GET refusal / x-amz-request-id precedence calls.
  Brief correction: the RUSTSEC-2026-0253 waiver is moot. The lockfile already resolves aws-sdk-s3 1.148.0 -> lru 0.18.4, so the waiver deletion and the >=1.144.0 floor are correct. The tracker note must say the advisory is not in the graph, not that an exposure was accepted.
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 8 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_741/review-b
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
