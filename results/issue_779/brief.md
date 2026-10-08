# Brief — issue 779 / s3-server-version-header-stamp

> Plan artifact (docs 02 §PLAN). Do reads ONLY this file (plus the peer callsites cited
> under **Citations expected**). The `- **Label:** value` lines are parsed by the driver.
>
> **Child 2 of 2** from the split of #736 (`results/issue_736/split-proposal.md`). #778 is
> already on your base: it bakes the build identity into the binary, exposes it as
> `wyrd_server::version::BUILD_IDENTITY`, and records it as the `version` field of the `s3`
> role's `role started` log event. This slice puts that identity on the wire. It derives
> nothing.
>
> Plan of record: `docs/design/proposals/draft/0017-blackbox-validation-tool.md`
> §Dependencies ("The gateway advertises no version") and §3 ("The matrix must be keyed to
> the server version"). Read in place in the target checkout — never copied here.

- **Slug:** s3-server-version-header-stamp
- **Kind:** enhancement
- **Defect:** The S3 gateway advertises no version on the wire. Verified on `origin/main`
  at `a801997`: `grep -rn "header::SERVER" crates/gateway-s3/src/` returns nothing, and the
  only header `handle` stamps on every response is `x-amz-request-id`
  (`crates/gateway-s3/src/lib.rs:1552-1556`). Real S3 sends `Server: AmazonS3`; sending
  nothing is the unusual choice. A client cannot tell last month's deployment from today's,
  a captured HTTP exchange does not identify the build that produced it, and
  `wyrd-validate`'s version-keyed capability matrix (proposal 0017 §3) has nothing to key
  on — its interim `--server-version` is an operator-supplied parameter that can silently
  be wrong, which for a *validation* tool is the worst kind of input. After #778 the binary
  knows its identity and logs it; nothing puts it on the wire.
- **Goal:** Every response the S3 front door emits carries `Server: wyrd/<version>`, where
  `<version>` is the identity #778 baked in — success, client error, server error and the
  streaming-GET head alike.
- **Success criterion:** BINDING (demonstrable by C4-verify at Check — no container, no
  cluster, no feature flag): a new integration test spawns the **built `wyrd` binary** as
  an `s3` role (`--s3-listen 127.0.0.1:0 --data-dir <tmp> --access-key … --secret-key …
  --log-format json`, killed on every exit path) and asserts, against that one child
  process:
  1. **the success plane carries it** — a signed `PUT` that succeeds returns a `Server`
     header;
  2. **the error plane carries it** — an UNSIGNED request refused `403` returns a `Server`
     header. This is the load-bearing leg: an error response is the one most likely to end
     up in a bug report, and it is the leg a per-handler implementation would miss;
  3. **the streaming-GET head carries it** — a signed `GET` of the object just written
     returns a `Server` header. Named in the invariant, so asserted rather than assumed;
  4. **the SERVER-ERROR class carries it** — a **signed** `GET /wyrd-bucket?acl` is refused
     `501 NotImplemented` by the subresource denylist (`acl` is listed at
     `crates/gateway-s3/src/lib.rs:342-360`; the refusal is `error_response(...
     NOT_IMPLEMENTED ...)` at `:1655-1661`), and that response returns a `Server` header.
     This leg exists because the invariant names four response CATEGORIES and legs 1-3 only
     exercised three: without it a check can go green while the 5xx category is untested.
     It is deterministic, needs no fault injection, runs post-auth (a different return path
     from leg 2's pre-auth refusal) and does not require the bucket to exist — the denylist
     is consulted before any handler. **Signing gotcha, do not trip on it:** the request
     carries a query, so it must be signed WITH that query — `sigv4::sign`'s third argument
     is the raw query string (`crates/gateway-s3/src/sigv4.rs:619-629`; `sign` applies
     `canonical_query` itself, and `canonical_query("acl") == "acl="`, `:868`). The existing
     `signed_headers` helper hardcodes `""` there (`s3_http_wire.rs:94-105`), so copy its
     shape with the query threaded through rather than reusing it as-is; a request signed
     over an empty query is refused `403` and would silently turn this leg into a second
     copy of leg 2;
  5. **the value is the real build identity, not a placeholder** — each of those four
     header values is EXACTLY `wyrd/` + V, where V is the `version` field of the same child
     process's `role started` JSON event (#778's observable, read from the same stderr
     stream). Byte equality against an independently-observed value, not a shape test: this
     is what makes the criterion unfakeable, and it is why this slice depends on #778.
     A `wyrd/unknown`, a hardcoded constant, or a per-handler stamp that misses the 403 all
     fail it.

  **What leg 4 does NOT claim.** A genuine `500 InternalError` (`gateway_error_response`,
  `crates/gateway-s3/src/lib.rs:3324`, through `classify`'s `InternalError` arm at
  `:3380-3395`) is not deterministically
  producible from a black-box test without injecting a backend fault, and inventing one is
  out of scope here. `501` is the deterministic representative of the server-error class;
  what carries the rest of the category is structural and is the point of the invariant —
  every response `dispatch` returns, `500` included, flows through `handle`'s ONE stamp
  point (`:1541-1557`). Say exactly that in `build-notes.md`; do not write prose claiming
  the `500` path was exercised.
- **Falsifiability:** RED is producible on the ordinary developer harness Do is pointed at
  — `cargo test -p wyrd-server --test s3_server_version_header`, no Docker, no cluster, no
  feature flag. On the reverted tree there is no `Server` header on any response at all
  (verified by grep above; axum/hyper add none of their own), so legs 1-4 fail on an absent
  header — including the new `501` leg, whose refusal exists on the base today (the denylist
  is pre-fix code) and answers with no `Server` header at all. This is not a prediction: #736's v4 `C4-verify` log shows exactly this red, and
  its adversarial review re-adjudicated the red→green as "real and on the production path".
  This instance's `C4-verify` **reverts the production change and keeps the added test**
  (`engine/scripts/run-verify.sh:508-517`), so the test must still COMPILE against the
  reverted tree: drive `env!("CARGO_BIN_EXE_wyrd")` (the `cli_roundtrip.rs:11` idiom), sign
  with the existing public `wyrd_gateway_s3::sigv4::{sign, format_amz_date, Credentials}`
  (`s3_http_wire.rs:45`, `:94-108`), and observe only response headers, status and stderr.
  Do MUST NOT reference any symbol this patch introduces — not the new `S3Config` field, not
  a new constant — from the test file; a test that calls net-new API fails to compile on the
  red leg, which the gate correctly scores `UNVERIFIABLE` (exit 77 → SUMMARY §6) rather than
  as a proven red.
  **If leg 5 cannot go green because the base lacks #778's `version` log field, STOP and say
  so in `build-notes.md` — do NOT weaken leg 5 into "starts with `wyrd/`".** That degradation
  is precisely the hole this ordering exists to close; a missing prerequisite is a
  declarable finding, not a licence to soften the criterion.
  The child process MUST be killed on every exit path — the `s3` role blocks forever. Read
  its stderr until the `wyrd s3: serving S3-compatible HTTP on <addr>` line
  (`cli.rs:2183-2186`, which reports `listener.local_addr()`) so the ephemeral port is
  observed rather than guessed and two tests can run concurrently. A test that lets the
  pre-fix binary run unbounded turns a red into a hang, which is worse than either verdict.
- **Invariant to restore:** *Every response the S3 front door emits identifies the build
  that produced it.* Stated over the response CATEGORY — success, client error, server
  error and the streaming-GET head alike — not over one handler and not over one status
  code. SELF-TEST: this cannot be satisfied by touching a single handler, which is the
  point; the gateway already has exactly one place where the property is expressible for
  all of them (`handle`'s single stamp point, `crates/gateway-s3/src/lib.rs:1552-1556`,
  reached by every request because the router is `Router::new().fallback(handle::<G>)`,
  `:205`). Source: the peer invariant #529 established for `x-amz-request-id` — "mints one,
  returns it on every response, records it on every log line"
  (`crates/gateway-s3/src/request_id.rs:1-10`). Honest boundary, and the reason the
  invariant says *the S3 front door emits*: responses hyper generates before the service is
  entered (a malformed request line, an unparseable `Content-Length`, an oversized header
  section → bare `400`/`431`) never reach `handle` and carry no header. That is the same
  fact the RED leg depends on, so it cuts both ways; do not claim otherwise in any doc
  prose you add.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** 778
- **Conflicts with:** 738, 742
- **Ordering note:** `Depends on: 778` is a genuine build-on dependency, not just
  sequencing: leg 5 asserts byte-equality against the `version` field #778 adds to the
  `role started` event, and the value stamped on the wire is the constant #778 bakes
  (`wyrd_server::version::BUILD_IDENTITY`). Built on a base without #778 this slice cannot
  go green. #778 also carries `Depends on: 773` (the `h2`/RUSTSEC-2026-0258 lockfile bump
  that `cargo deny` — and therefore the one gating `C4-ci` row — needs), which reaches this
  bundle transitively through the wave order. `Conflicts with: 738` — #738 threads
  `--chunk-size` through `cmd_s3` → `serve_s3_role` → `serve_s3_dispatch` → `serve_s3`, and
  this slice edits `serve_s3`'s body (`cli.rs:2377`). `Conflicts with: 742` — #742's scope
  (f) rewrites the `release.yml` smoke step this slice extends, and it is last in the batch,
  so the overlap is already scheduled apart.
- **Surfaces:** data
- **Difficulty:** medium
- **Scope:** (a) `S3Config` gains a server-identity field, defaulted in `S3Config::new` so
  every existing caller keeps compiling and a library caller that never sets it still emits
  a well-formed header; (b) `handle` stamps `Server: wyrd/<v>` at its single stamp point,
  beside the request id; (c) the composition root supplies the real value — `serve_s3` sets
  the field from `wyrd_server::version::BUILD_IDENTITY`, which #778 put in this same crate,
  so **no parameter threading through `serve_s3_role`/`serve_s3_dispatch` is needed or
  wanted**; (d) `docs/design/architecture/14-threat-model.md` records the disclosure
  decision (see Decision 2); (e) `.github/workflows/release.yml`'s existing installer smoke
  step gains the ONE end-to-end check that closes the tracker's "matches `VERSION`" clause —
  see Verification posture. It is a few lines of shell inside the `docker run` block that
  already installs the tarball; it cannot run at Check and is not part of the binding
  criterion. **/ out of scope:** any derivation, build script, normalizer or packaging
  change — #778 owns the identity and it is already on your base (a patch touching
  `crates/server/build.rs`, `crates/server/src/version.rs`, `xtask/src/dist.rs` or the
  Dockerfile will be rejected); any capability-negotiation protocol or capability list on
  the wire (the issue says so explicitly — one header carrying one string); a configuration
  knob to suppress or coarsen the header (Decision 2 — a follow-up issue if an operator ever
  asks); coarsening the value; the version on any other role's wire surface; a `wyrd
  --version` subcommand.
- **Repro instruction:** On `origin/main` at `a801997`, in the target checkout:
  `grep -rn "header::SERVER\|\"server\"" crates/gateway-s3/src/` → nothing. `cargo test -p
  wyrd-server --test s3_http_wire` passes today and asserts nothing about a `Server` header;
  add an assertion to any of its round-trips that the response carries one and it fails.
- **External dependencies:** none.
- **Test file:** `crates/server/tests/s3_server_version_header.rs` — a NEW file, and the
  patch's **ONLY** added `*/tests/*.rs`. Confirmed by DRY-RUNNING this instance's own
  classifier on a synthetic patch carrying exactly this slice's expected file set
  (`./engine/scripts/run-verify.sh --classify`), which returned
  `ADDED_TEST crates/server/tests/s3_server_version_header.rs` + `CRATE crates/gateway-s3` +
  `CRATE crates/server`: an ADDED `*/tests/*.rs` is the discriminator
  (`engine/scripts/run-verify.sh:143-145`), both crate dirs exist on base so `GREEN_ONLY`
  stays 0 (`:412`), and the gate runs `-p wyrd-server --test s3_server_version_header`
  (`:408`). The `.github/` and `docs/` files map to no crate and are correctly ignored by
  the classifier. Appending to the existing `s3_http_wire.rs` would silently degrade the
  gate to green-only and prove nothing; adding a SECOND `*/tests/*.rs` would make it a
  second discriminator that must also go red with production reverted, so do not.
- **Verification posture:** The BINDING criterion above is the default posture — a flippable
  test, red pre-fix and green post-fix at Check. One half IS deferred and is declared here
  so it lands as a pre-declared sign-off item rather than a surprise NEEDS-HUMAN: the
  tracker's "the wire version matches what `cargo xtask dist` stamps into the tarball's
  `VERSION`" clause cannot be observed inside `cargo xtask ci`, because `dist` needs Docker
  and a network and is deliberately not part of `ci` (`xtask/src/dist.rs:26-28`).
  * BUILT AND EXERCISED AT CHECK: the header on all three response planes and its equality
    with the startup-log identity, against the real composition root through the real
    router (the child-process test above).
  * WRITTEN HERE, OBSERVED AT RELEASE: scope (e). Inside the smoke step's existing
    `docker run` block, after `./install.sh` and the `libfdb_c` install it already does
    (`.github/workflows/release.yml:69-75`), start
    `/usr/local/bin/wyrd s3 --s3-listen 127.0.0.1:18080 --data-dir /tmp/vsmoke --access-key
    k --secret-key s` in the background, `curl -sS -D-` the endpoint — an UNSIGNED request,
    refused `403` and still carrying the header, so no signing is needed in shell — extract
    the `Server:` value, and compare its `wyrd/` remainder with the `version:` line of the
    untarred `VERSION` file; kill the role. The step already installs `curl` (`:66`). Make
    its own failure path report itself (`set -eu` is in force; an unreachable role must fail
    the step loudly, not silently skip the comparison).
  * WHEN IT IS ACTUALLY OBSERVED, plainly: on the next `v*` tag — and **no `v*` tag has ever
    been cut** (the repo's only tag is `archive/backup-premerge-signoff`; the workflow
    triggers on `push: tags: ["v*"]`). So at this sign-off the equality is written and
    reviewable but UNOBSERVED. Do must say exactly that in `build-notes.md` and must not
    claim it as demonstrated, and must NOT attempt an image build.
  * TWO ROUTES OBSERVE IT WITHOUT WAITING FOR A TAG, and the human decides at sign-off
    whether to spend one (recorded here so it is a choice, not a discovery): the release
    workflow ALSO triggers on **`workflow_dispatch`** (`.github/workflows/release.yml:20-24`)
    and its job carries no tag guard, so a manual run on the branch executes `cargo xtask
    dist --oci-archive` and the smoke step this slice extends; and the same pair is readable
    locally — `cargo xtask dist`, then compare the `version:` line of
    `target/dist/wyrd-*/VERSION` with the `Server:` value of a spawned role. Both need Docker
    and a network, both are out-of-band, and **neither is required to build this slice or to
    make the binding criterion go red→green** — which is why `External dependencies` stays
    `none` and Do must attempt neither.
  * The deferred half is a *verification* gap, not an unbuilt deliverable: the check IS
    written, and the coupling it would observe end-to-end is already asserted
    container-free by #778.
- **Production reach:** Not applicable in the seam sense — the production path traverses the
  change at Check: the test drives the real `handle` through the real router in the real
  binary, whose composition root sets the field.
- **Citations expected:** Do must cite `path:line` on the base for every change. Peer
  callsites Do MAY open and should mirror:
  * **The one place a header reaches every response** —
    `crates/gateway-s3/src/lib.rs:1540-1557` (the `x-amz-request-id` stamp at the end of
    `handle`, after `finish_response`). Stamp the `Server` header there, the same way, for
    the same reason. Every request reaches it: `Router::new().fallback(handle::<G>)`,
    `crates/gateway-s3/src/lib.rs:205`.
  * **The config seam and its default** — `crates/gateway-s3/src/lib.rs:101-121` (`S3Config`
    and `S3Config::new`). Only `new` constructs it — verified: no struct literal exists
    anywhere in the tree — so adding a field is source-compatible. The gateway crate owns
    the header NAME and the `wyrd/` shape (there must be exactly ONE place that spells
    `wyrd/`); it does not own the version.
  * **The composition root that sets config on the way in** — `crates/server/src/cli.rs:2377`
    (`serve_s3` building `S3Config` and setting `config.region` before `S3Gateway::new`).
    Set the new field there, from `wyrd_server::version::BUILD_IDENTITY` — ADR-0010: the
    composition root is what knows the build identity, the wire crate does not.
  * **Driving the built binary from an integration test** —
    `crates/server/tests/cli_roundtrip.rs:11` (`const WYRD: &str =
    env!("CARGO_BIN_EXE_wyrd");`). This test's child is a long-running server, so it is
    spawned, read from, and killed rather than `output()`-ed.
  * **Signing a request by hand with the production SigV4** —
    `crates/server/tests/s3_http_wire.rs:94-108` (`signed_headers`, built on the public
    `wyrd_gateway_s3::sigv4::{sign, format_amz_date, Credentials}` imported at `:45`).
  * **A signed PUT round-trip needs no bucket to be created first** —
    `crates/server/tests/s3_http_wire.rs:191-215`
    (`signed_put_get_delete_round_trip_is_byte_identical` PUTs straight to
    `/wyrd-bucket/round-trip-object` and asserts `200`, then GETs it back). Legs 1 and 3 are
    that same two-request shape against the spawned binary — don't invent a `CreateBucket`
    step.
  * **The deterministic server-error-class refusal leg 4 drives** —
    `crates/gateway-s3/src/lib.rs:342-360` (`UNSUPPORTED_SUBRESOURCES`, which lists `acl` and
    whose doc comment names `GET /bucket?acl` explicitly), `:1655-1661`
    (`unsupported_subresource_decoded` → `error_response(request_id,
    StatusCode::NOT_IMPLEMENTED, "NotImplemented", …)`, on the bucket route, before any
    handler runs) and `:465-472` (the matcher — a VALUELESS `?acl` matches, since the key is
    taken whole when there is no `=`). Read them to confirm the refusal needs no bucket and
    no object — then drive it as a signed request, per the signing gotcha in leg 4.
  * **Reading a response's HEADER block over a raw socket** —
    `crates/server/tests/s3_http_wire.rs:1286-1300` (`send_with_headers`, which exists
    precisely because `send` at `:112` discards headers). Both are private to that test
    file, so copy the shape; do not try to import them.
  * **The release smoke step to extend** — `.github/workflows/release.yml:59-88` (the
    bookworm container that untars, `./install.sh`s, installs `libfdb_c` and uses the `curl`
    it already has, then proves idempotence and uninstalls). Add the role-start +
    `Server`-vs-`VERSION` comparison inside that same `docker run`, and leave every existing
    assertion untouched.
  * **The threat-model table to extend** —
    `docs/design/architecture/14-threat-model.md:89` (the STRIDE row for **Information
    disclosure**). Record the decision; do not invent a new section shape. Editing it is
    legal and checked: the file's frontmatter is `status: living`, and
    `.github/workflows/docs-immutability.yml:253-256` scopes the append-only regime to
    `docs/design/adr/`, `docs/design/proposals/{draft,accepted}/` and `docs/design/specs/**`
    — `docs/design/architecture/` is outside it.
- **Prior-art check (triage cycles):** searched by affected path on `origin/main` at
  `a801997`. `git log --oneline -- crates/gateway-s3/src/lib.rs` → recent history is the
  #504/#506/#509/#510 wire-surface work and the #616/#619 lint sweeps; none adds a response
  header beyond #529's request id. `git log --oneline -- .github/workflows/release.yml` →
  the #570 dist pipeline work, no version assertion. `gh pr list --state all --search
  "Server header"` → no PR, open or closed, has attempted one. The only prior attempt is
  #736's own four rounds (`results/issue_736/iteration-v{1,2,3,4}/`), whose header half was
  never refuted; this brief is that half, re-scoped.
- **Disposition hint:** new-feature

## Decision 2 (parent, binding): the full identity stays on the wire

#736's v4 adversarial review raised this as a `[human]` item and it is decided here rather
than left for Do or for sign-off to discover: the header is deliberately stamped on the
**pre-auth 403**, so an anonymous `curl -sSD- http://<gw>/` learns the deployment's exact
build. Real S3 answers the coarse `Server: AmazonS3` for that reason, and
`docs/design/architecture/14-threat-model.md:89` ("Information disclosure") had never been
revisited.

**Accepted, and recorded rather than coarsened.** Coarsening defeats the version-keying this
work exists for — proposal 0017 §3 keys a capability matrix to the exact build — and an
operator who needs it suppressed can rewrite `Server:` at a fronting proxy. #778's Decision 1
already removed the dirty-tree half of the disclosure: the identity names a commit and makes
no claim about the working tree.

So scope (d) is not optional and not a doc-polish afterthought: the threat model must record
that the S3 front door discloses its exact build identity to unauthenticated callers, that
this is intended, and what an operator does if they need otherwise. **No configuration knob
is in scope** — that is a follow-up issue if an operator ever asks for one.

If you add living-doc prose elsewhere about the header, scope it to *responses the gateway
service produces*: hyper answers a malformed request line or an unparseable
`Content-Length` itself, before `handle` runs, and those carry no `Server`. Claiming "every
response" in operator-facing prose would be false — and it is the very fact the RED leg
relies on.

## Plan-review response (revision pass, issue #301 — inherited from #736)

The antagonistic plan review ran against the parent #736 brief after the split, and two of
its four findings are criterion-level and bind HERE. Both were re-verified against
`origin/main` (`65ca4fd`) and both were accepted:

* **The criterion did not falsify the server-error category** it claims. The invariant names
  four response categories; legs 1-3 exercised three. **Leg 4 above is new** — a signed
  `GET /wyrd-bucket?acl` refused `501` — and the old leg 4 (byte equality) is now leg 5.
  Nothing else changed: no leg was weakened, and the honest limit about a real `500` is
  stated rather than papered over.
* **Equality with the tarball's `VERSION` has no PR-time gate.** True, and already declared
  under Verification posture; what was missing is that it need not wait for a tag. The
  `workflow_dispatch` route and the local `cargo xtask dist` comparison are now named there
  as sign-off options. The binding criterion was NOT widened to include them — a criterion
  no gate can evaluate is worse than a declared deferral.

The other two findings landed on #778 (the dirty-checkout contract) and on the parent's own
stale `736a`/`736b` labels (now #778/#779); see `results/issue_736/brief.md`.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR
MAY happen during the cycle. The PR MUST NOT be marked ready before sign-off accepts.
