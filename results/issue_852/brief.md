# Brief — validate-s3-client-core

- **Slug:** validate-s3-client-core
- **Track:** blackbox
- **Goal:** `wyrd-validate` gets the S3 client every later slice calls through, proven against
  a conforming endpoint. It is `aws-sdk-s3` on an arbitrary `--endpoint` (path-style, static
  SigV4 credentials, plain HTTP), with a typed error carrying status, `<Code>`, `<Message>`
  and `x-amz-request-id` as fields, bounded waits, and bodies that stream both ways.
- **Defect:** (the gap) On the base, `crates/validate` parses flags, resolves credentials,
  echoes them and exits (`crates/validate/src/lib.rs:5-8`). Its manifest says "Deliberately
  NO dependencies … the S3 client and its dependency audit arrive in their own slice (#741)".
- **Success criterion:** BINDING, shown by C4-verify in-process with no container and no
  network. The production client runs against a Wyrd S3 gateway served in-process on loopback
  from the crate's dev-dependencies:
  1. **Round trip.** PUT → GET → DELETE, the GET byte-identical. A GET after the DELETE
     reports the typed not-found error (404, `NoSuchKey`), checked field by field. An empty
     object also round-trips.
  2. **Typed error.** An error response surfaces as a value with the HTTP status, `<Code>`,
     `<Message>` and `x-amz-request-id`. The id must **equal** the header the relay saw the
     gateway send, not just have the right shape. No substring-matching of `Display`.
  3. **Streaming, both directions, bounded at every point.** Each direction has an oracle
     that holds for the WHOLE transfer, not at one moment, so a client that streams a prefix
     and then collects the rest fails it. The oracles sit on a loopback relay between the
     client and the gateway; it counts bytes per direction and paces the response. Do states
     the window `W` and the piece bound `K` below, with how each was derived (SDK re-chunking,
     hyper's buffers, loopback socket buffers). The payload must be at least `8 × W`; Do may
     shrink `W` by setting small socket buffers on the relay's own sockets, never by
     tuning the kernel.
     - **PUT, lag:** the generator is never materialised. At EVERY pull, bytes the source has
       produced minus bytes the relay has forwarded is at most `W`.
     - **PUT, retention:** every piece the source yields is a `Bytes::from_owner` value
       (`bytes 1.12.1` is on the lock; `from_owner` at `bytes-1.12.1/src/bytes.rs:254`) whose
       owner counts itself on drop. At every pull, at most `K` of the source's pieces are
       still alive. A client that keeps forwarded pieces fails this.
     - **GET, lag:** for the whole body, the relay writes at most `W` bytes past what the
       test has taken from the client's body stream. A client that waits for more than `W`
       before handing over a piece stalls, and the bounded wait turns that into a message,
       not a hang. Then every byte is compared.
     - **Reviewed, not tested:** a client that COPIES bytes it already forwarded or handed
       over into a buffer of its own and keeps the copy would pass the oracles above. In
       `build-notes.md`, Do states with `path:line` that neither body path has an
       accumulating buffer (no `collect`, `aggregate`, `into_bytes`, growing `Vec`/`BytesMut`),
       and Check's review checks it. A process-wide allocation counter cannot stand in: the
       in-process gateway allocates in the same process.
     - **Mutations.** Do runs five, records that each fires its assertion, reverts each, and
       reports them in `build-notes.md`: (i) buffer the whole PUT; (ii) forward the first
       MiBs of the PUT, then collect the rest; (iii) keep every forwarded PUT piece in a
       `Vec`; (iv) collect the whole GET; (v) hand over the first GET pieces, then collect
       the rest. Chunk counts or largest buffer seen are not acceptable for the GET leg:
       "collect, then re-chunk small" passes them.
  4. **Integrity and bounded waits.**
     - A PUT whose source ends short, runs long (including excess in a separate piece) or
       errors fails with the body error, and nothing is stored.
     - A GET whose connection the relay cuts mid-body fails with the body error, never as a
       shorter object.
     - Each deadline (connect, operation, body-idle) expires as a typed timeout naming its
       phase.
     - A request the SDK refuses to build (an empty key) is reported as exactly that.
- **Falsifiability:** RED is reachable on the plain harness: `cargo test -p wyrd-validate
  --test s3_client_roundtrip`, no Docker, no network. The pre-fix red is criterion-absence;
  criterion 3's five mutations are the demonstrated red.
  The base has `crates/validate` (from #774), so `run-verify.sh` skips `GREEN_ONLY`
  (`engine/scripts/run-verify.sh:410-412`). It reverts the production files, keeps the test,
  and the test fails to compile. That scores `UNVERIFIABLE` (§6 NEEDS-HUMAN, non-gating), the
  correct verdict for net-new API.
  **The base, by commit.** This bundle builds on the live run's integration branch
  `pdca-integration/r-a834e98f67b952c61db7a03084c626e8/main` (tip `022d76f` on 2026-10-02).
  It carries #774 at `a5a3fa3` and #775 at `343be83` (fold commit `fa94483`). `343be83` is
  the one commit that must be in your base: it contains #774's crate and #775's lint.
  **First, check the base:** `git merge-base --is-ancestor 343be83 HEAD`; `ls
  crates/validate`; `grep -n validate Cargo.toml`. If any check fails, STOP and report. Do
  not recreate #774's crate or #775's lint. PR #845's branch has since moved on to
  `bfe4128` (a non-UTF-8 argument fix) that is NOT on the integration branch; this slice
  does not need it and must not pull it in.
- **Invariant to restore:**
  - (a) *The validator never holds an object whole, in either direction: what it holds of a
    body is bounded independently of object size.* This is the "stream, don't buffer"
    invariant the server side already holds: `crates/gateway-s3/src/lib.rs:12-17` and
    `crates/core/src/write.rs:530-535` ("Peak resident bytes are one `chunk_size` piece …
    independent of object size", the `0015:789` OOM cliff). How it is shown, stated so no
    one reads more into it: criterion 3 TESTS that the client is never more than `W` bytes
    ahead of the peer or the consumer, and never keeps more than `K` of the caller's PUT
    pieces; REVIEW covers the one remaining way to break it, keeping a copy of bytes already
    passed on.
  - (b) *For a conforming response, every fact the typed error reports is the one the server
    sent, and no await on the endpoint is unbounded.* Sources: proposal 0017 §6-7; `AGENTS.md`
    "Await discipline" ("every await on external work is bounded … spawned helper tasks are
    aborted on drop").
  - Non-conforming responses are #853's and #854's invariant, not this one's.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on (merged):** 775
- **Ordering note:** added after the split was accepted (the proposal format only accepts
  sibling labels). #775 (the no-`wyrd-*` lint, stacked on #774's crate) is what puts
  `crates/validate` on the base. The run that adopted this split is still live:
  `pdca flow 839 840 841 842 843 810 682 736 711 721 738 774 775 741 742`, whose run key
  hashes to `a834e98f…` and so folds onto
  `pdca-integration/r-a834e98f67b952c61db7a03084c626e8/main`. #775 is a named member of that
  run and COMPLETE, so `_runnable` treats this in-batch dependency as met
  (`src/pdca_harness/flow.py:710-714`), and the fold has already merged #775's published
  commit `343be83` into that branch (`fa94483`). That commit is the base Do needs (see
  Falsifiability). In any later run outside that batch, the `(merged)` form holds this
  bundle until PR #849 has merged, so it never builds on a `main` without the crate
  (INTEGRATION §2). The plan review grounds on the brief's own base, `main` (`36f006d`),
  before any stack base is written, which is why it found no crate; that is not the tree Do
  builds on. #853 and #854 build on this bundle, and #742 is re-pointed at it because
  #742's image build must compile the aws SDK dependency set this slice adds.
- **Surfaces:** data
- **Difficulty:** high. The diff is moderate, but it moves the aws SDK's ~100 crates into the
  shipped dependency graph.
- **Do model:** opus
- **Scope:**
  - (a) A client built from `ResolvedConfig`, public in the **library** (`crates/validate/src/`)
    so the integration test can reach it: endpoint, region, static credentials, path-style,
    plain-HTTP connector, retries and stalled-stream protection **off**.
  - (b) The typed error in its **final shape**, with one variant per place a call can fail:
    an S3 error response; a response that cannot be read as S3; no response; a deadline,
    naming its phase; a request never built; an object body that failed or cannot be trusted.
    Its `<Code>` keeps "this code", "XML without a code" and "no body" apart. Children 2 and 3
    must be able to test against it without adding a variant.
  - (c) The request id read from the `x-amz-request-id` header itself. The SDK's generic
    accessor prefers `x-amzn-requestid` when both are present.
  - (d) Streaming PUT from a caller's source with a declared length, and a GET body read
    piece by piece.
  - (e) Connect, operation and body-idle deadlines, with defaults. Their tests need no
    invented bytes: a peer that accepts and never answers, a listener with a full accept
    queue, and the relay holding a real GET's tail.
  - (f) The dependency move (below).
  - (g) Update `docs/design/architecture/05-building-block-view.md:253` ("the S3 client
    arrives with #741").
  - (h) Test fixtures own every task they spawn and abort it on drop, unlike the detached
    spawn at `crates/server/tests/s3_http_wire.rs:88-90`.

  **/ out of scope:**
  - Every response a conforming S3 server does not send: empty-body errors, XML without a
    code, non-XML pages, broken or trailing-junk `<Error>` documents, unreadable 2xx, error
    bodies whose length disagrees with their framing, close-delimited or chunked GET framing,
    and the error-body byte budget (#853).
  - Every upload peer that acknowledges early or stops reading (#854).
  - Mark each site where those belong with an in-code `// deferred: #853` (responses) or
    `// deferred: #854` (uploads) marker, and answer a review finding on them with "Deferred —
    tracked in #853/#854". The rubric treats that as settled (`AGENTS.md`, "Deferrals are
    settled"). Implementing them here is how this slice grew to 103 KB.
  - Also out:
    - the 5 GiB property (#761);
    - TLS (`crates/gateway-s3/src/lib.rs:50-57`);
    - the capability matrix and `smoke` (#743), so the binary still echoes and exits;
    - any gateway change;
    - **any edit under `crates/server/`**.
- **Repro instruction:** on the base (the integration branch above, containing `343be83`),
  `grep -n aws crates/validate/Cargo.toml` returns nothing (a bare `grep -rn aws
  crates/validate/` matches only the AWS credential variable names in `access_keys.rs` and
  `tests/cli_surface.rs`), and `crates/server/Cargo.toml:131-135` holds the aws crates only under
  `[dev-dependencies]` (`:114`).
- **External dependencies:** none. Base toolchain only, and that is a scope rule: the gateway
  is served in-process from dev-dependencies. Reaching for a container, MinIO, a `wyrd s3`
  process or the network means the fixture is wrong. Stop and declare it.
- **Test file:** `crates/validate/tests/s3_client_roundtrip.rs` (NEW, self-contained: no
  `tests/common/` module, which the gate's classifier would treat as a test target).
- **Verification posture:** DECLARED (net-new API). Criteria 1, 2 and 4 are red by absence,
  so C4-verify reports `UNVERIFIABLE`. **Built and exercised at Check:** the whole client,
  end to end against the real `S3Gateway` over real TCP, with no mocks. **Demonstrated red:**
  criterion 3's five mutations. Reviewed, not tested: no copy kept of bytes already passed on (criterion 3). Nothing is deferred to another environment.
- **Production reach:** traversed at Check (real SDK, listener and gateway). Record two
  limits in `build-notes.md`: plain HTTP, where deployment puts TLS in front (proposal 0017
  §10); and a redb + mem + local-FS composition, not production's.
- **Citations expected:** `path:line` on the base for every change. Peers Do MAY open:
  - **SDK config to copy:** `crates/server/tests/s3_gateway_cluster.rs:100-115`
    (`sdk_client`: `force_path_style(true)`, retries and stalled-stream protection disabled,
    explicit `http_client`).
  - **In-process gateway:** `crates/server/tests/s3_http_wire.rs:56-92`. Use a 256 KiB chunk
    size, not its `with_chunk_size(8)`, and never call `serve_s3_role`.
  - **Request id:** `crates/gateway-s3/src/request_id.rs:40` and the stamp at
    `crates/gateway-s3/src/lib.rs:1552-1557`.
  - **Dependency wall:** `deny.toml:1-20`, `:77-86`, and `deny-all-features.toml:111`.
  - **Adoption-audit standard:** root `Cargo.toml:92-101` (`hmac`), `:134-142` (`roxmltree`).
  - **The lint:** `xtask/src/repo_guard.rs:34-42`, `:648-654`. `aws-sdk-s3` as a normal
    dependency is fine; Wyrd crates are allowed as dev-dependencies only (proposal 0017 §9).
- **Prior-art check (triage cycles):** by path. `git log origin/main -- crates/validate` →
  empty (the crate exists via open PRs #845 and #849, folded on this run's integration
  branch). `gh pr list --state all --search wyrd-validate` → #845, #849, #765 (proposal 0017),
  nothing else. The waiver came from #726 (closed), commit `28ff7b3`, on a dev-only ground.
  #741's three unpublished attempts are this split's history.
- **Disposition hint:** new-feature

## The dependency move

A shipped dependency is a human-only call (INTEGRATION §4). **It was taken:** promotion was
accepted in the 2026-08-17 Plan session, and the 2026-09-30 sign-off of #741 accepted the
floor and the waiver deletion below. Do writes the record; it does not re-open the decision.

**Where each decision is recorded** (paths in this harness repo, `results/issue_741/`; none
of it is in the target). Two of the three are session and sign-off records, not tracker
comments, and sign-off of THIS bundle should mirror one sentence onto #741 so the tracker
matches:

| Decision | Record |
|---|---|
| The question was posed | `notes.json`, the thread's only comment (eduralph, 2026-08-16T16:38:05Z): "`aws-sdk-s3` moves from dev-dependency into a shipped artifact — a NEEDS-HUMAN license item … this needs the audit **before** the crate lands". |
| Promotion accepted (2026-08-17) | `iteration-v3/brief.md:10-24`, the brief header: "promoting `aws-sdk-s3` to a shipped dependency is ACCEPTED", recorded as a session decision and stated there to be absent from the tracker. |
| Floor and waiver deletion accepted (2026-09-30) | `iteration-v3/SUMMARY.md:293-297`, §9 signed "Eduard Ralph / 2026-09-30", outcome `iterated-to-Plan`: "the waiver deletion and the >=1.144.0 floor are correct. The tracker note must say the advisory is not in the graph". |
| The split and its three children | same §9 line ("Split in re-plan (pdca split 741)"); `close-disposition` = `split`; `split-proposal.md`; each child's `split-lineage.json`. |
| The three earlier attempts | `iteration-v1/`, `iteration-v2/`, `iteration-v3/` (each with `patch.diff` and `SUMMARY.md`; §9 at v1 `:289-293` and v2 `:181-185`, both auto-iterate, and v3 above). |

- **Pin in `[workspace.dependencies]`.** `aws-sdk-s3` gets a **floor of `1.144.0`**: in the
  registry, `1.142.0` needs `lru ^0.16.3` and `1.144.0` needs `^0.18.2`. Set the smithy crates
  to floors the current `Cargo.lock` satisfies; it already resolves `aws-sdk-s3 1.148.0` and
  `lru 0.18.4`. The lockfile should gain only `wyrd-validate`'s edges.
- **Delete the RUSTSEC-2026-0253 waiver** (`deny.toml:77-86`, `deny-all-features.toml:111`).
  Its own removal trigger has already fired, and with this floor no unsound `lru` can enter
  the shipped graph. The advisory is not in the graph; say that, not that an exposure was
  accepted. `cargo deny check` must stay green on both configs.
- **Write the ADR-0003 §2 audit** in `build-notes.md`, plus a short adoption comment by the
  pins: licence, unsafe posture, maintenance, and the transitive surface from `cargo tree -p
  wyrd-validate -e normal`. Name what is new to the shipped graph. Round 1 got this wrong:
  `default-client` brings `rustls-native-certs`, `rustls-pki-types` and, on Unix,
  `openssl-probe`, though not `rustls` itself.

## Plan-review response (#301 revision pass, 2026-10-02)

Three findings; two revised the brief, one is answered with evidence and a tighter base check.

* **"The supplied base cannot support this slice."** The review was right about the tree it
  was given and wrong about the tree Do gets. The plan review is pinned to the brief's own
  base, `main` at `36f006d`, because no stack base exists before Do; `main` will not carry
  the crate until PRs #845 and #849 merge. Do builds on the live run's integration branch,
  which already has #775's commit `343be83` (and #774 under it) folded in. The brief now names
  that branch and commit, explains why the `(merged)` dependency is met in this run and held
  outside it, and makes `git merge-base --is-ancestor 343be83 HEAD` part of the STOP check.
* **"The streaming oracle proves early progress, not bounded memory."** Correct: one check at
  the last PUT pull and one held GET tail let "stream a prefix, collect the rest" pass, and
  kept copies were never observed. Criterion 3 now bounds lag at every point in both
  directions (window `W`, payload at least `8 × W`) and counts the caller's live PUT pieces
  (bound `K`) through a drop-counting `Bytes::from_owner`. Two more mutations (prefix then
  collect, for each direction) and a third (keep forwarded PUT pieces) must fire. The one
  hole left, keeping a private copy of bytes already passed on, is named as a review item,
  and invariant (a) now says which part is tested and which is reviewed.
* **"Tracker decisions have no evidence."** The decisions are real but live in session and
  sign-off records, not tracker comments. "The dependency move" now has a table pointing at
  the exact file and lines for each: the posing comment in #741's `notes.json`, the
  2026-08-17 acceptance in `iteration-v3/brief.md:10-24`, and the 2026-09-30 human sign-off
  in `iteration-v3/SUMMARY.md:293-297`. Sign-off of this bundle should mirror the outcome onto
  #741.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 1): rebuilding for the implementation-level findings — C4 Verification (red→green) — Accept or decline the declared absence-based verification for a new API — 11 tests pass after restoration, but the pre-fix discriminator never executes and the remote-base coverage gate measures nothing (`reviewer-restored-green.log:18`, `gate-logs/C4-verify.log:243`, `gate-logs/C4-diff-cov.log:10`).; C5 Causal adequacy — The bounded-wait contract remains false for a continuously ready empty-piece source: one poll can prevent both the SDK deadline and cancellation from running; resolve R1 before claiming bounded execution (`target/crates/validate/src/s3/body.rs:99`, `reviewer-public-probe.log:5`).; T5 Judgment — The PUT oracle cannot establish the stated payload window because framing bytes receive payload credit; resolve R2 so a real lag above W fails the assertion (`target/crates/validate/tests/s3_client_roundtrip.rs:214`, `target/crates/validate/tests/s3_client_roundtrip.rs:572`, `reviewer-lag-probe.log:4`).; `crates/validate/src/s3/body.rs:101` (`Some(Ok(piece)) if piece.is_empty() => continue`): this is the same bug T4-batch-review blocks on, and I measured what it does. Source used: `stream::repeat_with(|| Ok(Bytes::new()))`, declared length 10, a peer that reads and never answers, 1 s operation deadline. On a **multi-thread** runtime, `put_object` returns `Timeout { phase: Operation, limit: 1s }` after 1.001 s. But the hyper connection task keeps one worker spinning forever, and the test process never exits (killed by `timeout 60`, exit 124). That breaks the rubric's "spawned helper tasks are aborted on drop", because an abort cannot land on a task that never yields. On a **current-thread** runtime, the call hangs: neither the 1 s operation deadline nor an outer 5 s `tokio::time::timeout` fires (watchdog thread still waiting at 15 s). Passing empty frames through to the SDK does not fix this. `aws-runtime 1.10.0 src/content_encoding/body.rs:147-161` returns `Ready(Ok(true))` for an empty data frame, and the caller loop `continue`s, so the SDK spins the same way. The fix has to cap skipped empties per poll and then `cx.waker().wake_by_ref(); return Poll::Pending`. Add a current-thread test with exactly that source.; `crates/validate/src/s3.rs:212` (`DispatchFailure(failure) if failure.is_timeout()`): C5's surviving mutant (guard replaced with `true`) shows no test ever produces `S3Error::NoResponse`. Under that mutant, a GET to a refused port would come back instantly as `Timeout { phase: Connect, limit: 5s }`, a deadline that never ran, and all 11 tests stay green. The production code is correct today: in `aws-smithy-http-client 1.4.2 src/client.rs:628-660` only smithy's `TimedOutError` (the connect timeout, since the read timeout is disabled) maps to a timeout, and a refused connection maps to `ConnectorError::io`. But scope (b) makes "no response" a final-shape variant that children #853/#854 build on, and invariant (b) says every reported fact is real. Add a test: bind a listener, read its port, drop it, GET, and assert `NoResponse`.; `crates/validate/src/s3.rs:225` (`let request_id = request_id(raw);`): my mutation m6 swapped this for the SDK's generic `aws_sdk_s3::operation::RequestId::request_id(&err)`, which prefers `x-amzn-requestid`. **All 11 tests still pass.** Scope (c) exists only for the case where both headers are present, and nothing in the suite sends both. So `error_matches_wire` (`tests/s3_client_roundtrip.rs:769`) proves criterion 2's equality but not scope (c). The relay already splits and rewrites response heads in `pump_down`. Inject a decoy `x-amzn-requestid` there and assert the typed id still equals the captured `x-amz-request-id`. A proxy adding that header is not on the brief's #853 list of non-conforming responses.; `crates/validate/src/s3/body.rs:101`: An always-ready source yielding empty `Bytes` loops forever inside one `poll_frame` call. It never returns `Pending`, so cancellation cannot stop that task; on a single-worker runtime it also prevents the operation deadline from firing. Bound empty-piece processing per poll, wake the task and yield, and add a regression for an always-ready empty source. The frozen `T4-batch-review` log independently reports this defect.; `crates/validate/tests/s3_client_roundtrip.rs:215`: The PUT lag oracle subtracts encoded wire bytes from produced payload bytes. `pump_up` counts aws-chunked framing as forwarded data (`crates/validate/tests/s3_client_roundtrip.rs:572`). With cumulative framing overhead H, the assertion permits actual payload lag up to W + H; H grows with the transfer, so excess buffering can pass the claimed fixed W bound. Count forwarded payload bytes separately from framing and use that count in the lag assertion.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_852/review-b. 5 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 75 mutants tested in 3m: 11 missed, 20 caught, 44 unviable
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_852/review-b
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
