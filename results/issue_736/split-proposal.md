<!-- pdca:split-proposal v1 -->
# Split proposal — issue 736

<!-- Each child is a COMPLETE brief, not a sketch. A filled Slug alone makes `state.state`
     classify the materialised file as PLANNED, so `pdca flow` skips Plan and sends it
     straight to Do — a child missing `Repo + branch target` then builds successfully and
     has nowhere to publish. Fill every field. -->

<!-- Delimiters are HTML comments, not headings, DELIBERATELY: each child body is a full
     draft brief and may contain arbitrary headings and fenced code blocks, so anything
     that could also appear inside a child cannot be its boundary. `pdca split --accept`
     parses these markers; keep them exactly as written. -->

## Why this slice is oversized

The parent brief bundles two outcomes that four Do rounds have already proved separable —
by where the blocking findings landed, not by byte count.

**Outcome one is a build identity that exists at all.** Today nothing compiled into `wyrd`
can report which checkout produced it: `crates/proto/build.rs` is the tree's only build
script, the `role started` event (`cli.rs:2205-2212`) records `role`, `listen`, `region`,
`dservers` and no version, and the packaging path cannot even see the git metadata
(`.dockerignore:6` excludes `.git/` while `dist` builds the shipped binary inside the
image). This half is where **every** blocking finding of rounds v3 and v4 landed:
`git describe` scoping, the reftable watch set, the rerun-watch set (a T3 FAIL plus 4/4
batch-review blockers), `is_identity`'s missing Docker tag rule. It is a build-system and
packaging problem, adjudicated by Decision 1 of the parent brief (identity names a COMMIT,
`--dirty` dropped, narrow watch set).

**Outcome two is that identity on the S3 wire.** `handle` stamps exactly one header today
(`x-amz-request-id`, `crates/gateway-s3/src/lib.rs:1552-1556`) and has exactly one stamp
point reached by every response (`Router::new().fallback(handle::<G>)`, `:205`). This half
was never the problem: v4's adversary could not refute its red→green and it passed every
rubric every round. Its remaining obligations are the `S3Config` seam, the threat-model
line (Decision 2: full identity stays on the wire, recorded, no knob), and the
release-smoke wire-vs-`VERSION` comparison.

The previous brief rejected splitting at *"emit a header" | "give it a real version"*
because the header child would ship an inert `wyrd/unknown` whose test the second child
rewrites. Inverting the order removes that objection: **derivation first, header second,
neither child inert.** Each child has its own defect, its own observable criterion, its
own new test file, and ships as its own PR. Two children, not more: the five scope rungs
(a)–(e) partition cleanly across this one seam, and any finer cut (e.g. packaging as its
own child) would produce a slice with no independently observable success criterion.

## Wave sketch

**Wave 1: child-1 alone. Wave 2: child-2 alone.** The children stack strictly — this is a
two-wave, one-lane split, and that is deliberate.

- `child-2` **depends on** `child-1` because its success criterion is defined *against*
  child-1's artifact: the header's remainder must be byte-equal to the `version` field of
  the `role started` log event that child-1 introduces. Built on a base without child-1
  there is no field to compare against and the header could only go green on a
  placeholder — the exact `wyrd/unknown` hole this ordering exists to close. No
  `Conflicts with:` edge between the children is needed; the dependency already keeps
  them out of a shared wave.
- Both children inherit the parent's **external** tracker edges, and they divide by which
  rungs each child carries: `child-1` conflicts with **#742** (it edits the packaging rung
  — `xtask/src/dist.rs` / `deploy/docker/wyrd/Dockerfile` — that #742 reworks; #742's
  brief already expects `--build-arg WYRD_VERSION` on its base, i.e. it expects child-1
  merged first) and with **#738** (child-1's `role started` field touches the
  `cli.rs`/serve path that #738 threads `--chunk-size` through). `child-2` conflicts with
  **#738** (its composition root threads the version through the same
  `serve_s3_role`/`serve_s3_dispatch`/`serve_s3` chain) and with **#742** (its
  release-smoke comparison lands in the `dist` flow #742 reworks). Neither child should
  share a wave with #738 or #742 in any batch that includes them.
- `child-1` inherits the parent's `Depends on: 773` — a GATE dependency, not a code one:
  `cargo deny check` fails on `main` (RUSTSEC-2026-0258, `h2 0.4.15`) and runs inside
  `run_ci`, so no child can go green until the #773 lockfile bump lands. `child-2` gets
  #773 transitively through child-1.

<!-- pdca:child child-1 -->
- **Slug:** wyrd-build-identity-derived-and-logged
- **Defect:** No build identifier exists anywhere in the `wyrd` binary — `crates/proto/build.rs` is the tree's ONLY build script, so nothing compiled in can report which checkout produced it. The `role started` JSON log event records `role`, `listen`, `region`, `dservers` and nothing else (`crates/server/src/cli.rs:2205-2212`). The packaging path cannot derive an identity inside the image build because `.dockerignore:6` excludes `.git/` and `cargo xtask dist` builds the shipped binary inside the image. The tarball's `VERSION` file (stamped by `dist`) and the binary's self-knowledge have no common source. Per parent Decision 1 (binding, decided at Plan after four Do rounds): the baked identity names the COMMIT the build was made from — `git describe --tags --always`, **no `--dirty`**, no claim about the working tree — so the build-script rerun-watch set stays narrow (`HEAD`/refs only) and the edit→status→test relink storm the v4 adversary measured does not occur. The module doc must state the no-working-tree-claim plainly. Release builds inject the version explicitly via `dist`.
- **Success criterion:** Driving the built `wyrd` binary as an `s3` role, the `role started` JSON log event carries a `version` field whose value is a real git-derived build identity — not empty, not a default, not a placeholder — and for the same checkout that string is byte-equal to what `cargo xtask dist` stamps into the tarball's `VERSION` file (single-sourced normalizer, one derivation). The image-build path receives the identity via `--build-arg WYRD_VERSION` rather than deriving it inside the container.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Reproduction:** On `origin/main` at `a801997`, start an `s3` role and observe the `role started` event: no `version` field is present (`crates/server/src/cli.rs:2205-2212` records four fields, none of them a version), and no build script exists outside `crates/proto` to have baked one.
- **Scope:** One logical fix: (a) a build script bakes the commit-derived identity into `wyrd`, with the derivation/normalization single-sourced with `cargo xtask dist`'s `VERSION` stamping; (b) the packaging rung passes the identity INTO the image build (`--build-arg WYRD_VERSION`, `xtask/src/dist.rs` + `deploy/docker/wyrd/Dockerfile`); (c) the `role started` event records it. Rerun-watch set covers exactly the refs that change the answer (`HEAD`/refs), nothing wider — Decision 1 dissolves the v3/v4 watch-set blockers rather than arguing them down. / **out of scope:** any wire surface (no `Server:` header — that is child-2); `S3Config`; a `wyrd --version` subcommand; `--dirty` or any working-tree claim; changing the workspace `version = "0.0.0"` placeholder; the OCI `org.opencontainers.image.version` label (`dist` already sets it, `xtask/src/dist.rs:458`).
- **External dependencies:** none
- **Test file:** crates/server/tests/build_identity_startup_log.rs
- **Difficulty:** high
<!-- Inherited external tracker edges — `Depends on: 773`, `Conflicts with: 738, 742` — are
     restored on the materialised brief; proposal ordering fields take sibling labels only. -->
<!-- pdca:end child-1 -->

<!-- pdca:child child-2 -->
- **Slug:** s3-server-version-header-stamp
- **Defect:** The S3 gateway advertises no version on the wire. The only header `handle` stamps on every response is `x-amz-request-id` (`crates/gateway-s3/src/lib.rs:1552-1556`); there is no `Server:` header at all, on any response category. A client cannot tell last month's deployment from today's, a captured HTTP exchange does not identify the build that produced it, and `wyrd-validate`'s version-keyed capability matrix (proposal 0017 §3) has nothing to key on. Invariant to restore: *every response the S3 front door emits identifies the build that produced it* — stated over the response CATEGORY (success, client error, server error, streaming-GET head), not over one handler; the gateway has exactly one place where the property is expressible for all of them (`handle`'s single stamp point, reached by every request because the router is `Router::new().fallback(handle::<G>)`, `lib.rs:205`; peer invariant: #529's `x-amz-request-id`, `crates/gateway-s3/src/request_id.rs:1-10`). Per parent Decision 2 (binding): the full identity stays on the wire, including on the pre-auth 403 to unauthenticated callers — recorded in the threat model, not coarsened, and with **no configuration knob** (a fronting proxy can rewrite `Server:` if an operator needs suppression).
- **Success criterion:** Driving the built `wyrd` binary as an `s3` role: a signed request that succeeds, an unsigned request refused 403, and a streaming GET all return a `Server` header whose value is `wyrd/<v>`, where `<v>` is byte-equal to the `version` field of the child process's `role started` JSON log event (child-1's artifact — this closes the "green on `wyrd/unknown`" hole by construction and needs no derivation logic in the test). Additionally: `docs/design/architecture/14-threat-model.md` §Information disclosure records the exact-commit disclosure decision, and the release smoke step compares the wire value against the tarball's `VERSION`.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Reproduction:** On the pre-fix tree there is no `Server` header on any S3 response — established over four rounds, not asserted: v4's C4-verify log shows the reverted tree failing on the absent `Server:` line, re-adjudicated by the adversary review as "real and on the production path".
- **Scope:** One logical fix: (d) `S3Config` carries the version, threaded from the composition root (`serve_s3_role`/`serve_s3_dispatch`/`serve_s3`), and `handle` stamps `Server: wyrd/<v>` at its single stamp point (`crates/gateway-s3/src/lib.rs:1552-1556`) so every response category carries it; the threat-model line per Decision 2; (e) the release smoke step comparing the wire value with the tarball's `VERSION`. / **out of scope:** any derivation logic (child-1 owns the identity); any capability-negotiation protocol or capability list on the wire (one header, one string); a suppression/config knob (follow-up issue if ever asked for); the version on any other role's wire surface; coarsening the value.
- **External dependencies:** none
- **Test file:** crates/server/tests/s3_server_version_header.rs
- **Difficulty:** medium
- **Depends on:** child-1
<!-- Inherited external tracker edges — `Conflicts with: 738, 742` (and #773 transitively via
     child-1) — are restored on the materialised brief. -->
<!-- pdca:end child-2 -->
