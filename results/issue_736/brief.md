# Brief — issue 736 / s3-server-version-header

> Plan artifact (docs 02 §PLAN). **This brief is the PARENT of a split** — see
> "Split line" below. It is re-authored from scratch after four Do rounds; the previous
> 561-line brief and its four attempts are preserved under `iteration-v{1,2,3,4}/`.
>
> Plan of record: `docs/design/proposals/draft/0017-blackbox-validation-tool.md`
> §Dependencies ("The gateway advertises no version") and §3 ("The matrix must be keyed to
> the server version"). Read in place in the target checkout — never copied here.

- **Slug:** s3-server-version-header
- **Kind:** enhancement
- **Defect:** The S3 gateway advertises no version. Verified on `origin/main` at `a801997`:
  the only header `handle` stamps on every response is `x-amz-request-id`
  (`crates/gateway-s3/src/lib.rs:1552-1556`), and no build identifier exists anywhere in
  the binary — `crates/proto/build.rs` is the tree's ONLY build script, so nothing compiled
  into `wyrd` can report which checkout produced it. A client cannot tell last month's
  deployment from today's, a captured HTTP exchange does not identify the build that
  produced it, and `wyrd-validate`'s version-keyed capability matrix (proposal 0017 §3) has
  nothing to key on — its interim `--server-version` is an operator-supplied parameter that
  can silently be wrong.
- **Goal:** Every S3 response, success and error alike, carries `Server: wyrd/<version>`,
  where `<version>` is the build identity baked in at compile time, recorded in the role's
  startup log, and — **for the binary `cargo xtask dist` produces** — byte-equal to the
  string that same run stamps into the tarball's `VERSION` file. The equality is a claim
  about a DIST-BUILT artifact and the tarball around it: `dist` derives the version once
  (with `--dirty`, `xtask/src/dist.rs:355-363`), writes it to `VERSION` (`:566-571`) and
  hands that same binding to the image build, where the binary takes it VERBATIM. An
  ordinary `cargo build` derives the commit it was built from (no `--dirty`, Decision 1) and
  produces no tarball, so there is nothing there for it to be unequal to. See the
  plan-review response below.
- **Success criterion:** SPLIT — the binding criteria live in the children (#778, #779).
  The parent's own criterion, restated for the record so the children can be checked
  against it: driving the built `wyrd` binary as an `s3` role, a signed request that
  succeeds, an unsigned request refused 403, a **signed request answered in the 5xx
  server-error class** (`GET /<bucket>?acl` → `501 NotImplemented` — the subresource
  denylist, `crates/gateway-s3/src/lib.rs:342-360` and `:1655-1661`), and a streaming GET
  all return a `Server` header whose value is `wyrd/<v>`, where `<v>` is byte-equal to the
  `version` field of the child process's `role started` JSON log event and is a real
  git-derived build identity, not a default or a placeholder.
- **Falsifiability:** RED is producible on the ordinary developer harness Do is pointed at
  — `cargo test -p wyrd-server --test <name>`, no Docker, no cluster, no feature flag. On
  the pre-fix tree there is no `Server` header at all and the `role started` event carries
  no `version` field (re-verified on `origin/main` `65ca4fd`: `cli.rs:2199-2206` records
  `role`, `listen`, `region`,
  `dservers` and nothing else). Established over four rounds, not asserted: v4's C4-verify
  log shows the reverted tree failing on the absent `Server:` line and the adversary review
  re-adjudicated it as "real and on the production path".
- **Invariant to restore:** *Every response the S3 front door emits identifies the build
  that produced it.* Stated over the response CATEGORY — success, client error, server
  error and the streaming-GET head alike — not over one handler and not over one status
  code. SELF-TEST: this cannot be satisfied by touching a single handler; the gateway has
  exactly one place where the property is expressible for all of them (`handle`'s single
  stamp point, `crates/gateway-s3/src/lib.rs:1552-1556`, reached by every request because
  the router is `Router::new().fallback(handle::<G>)`, `:205`). Source: the peer invariant
  #529 established for `x-amz-request-id` — "mints one, returns it on every response,
  records it on every log line" (`crates/gateway-s3/src/request_id.rs:1-10`).
- **Repo + branch target:** getwyrd/wyrd @ main
- **Conflicts with:** 738, 742
- **Ordering note:** `Depends on: 773` is a GATE dependency, not a code one: `cargo deny
  check` fails on `main` (RUSTSEC-2026-0258, `h2 0.4.15`, patched `>= 0.4.16` — re-verified
  2026-08-19, `h2 0.4.15` still in `origin/main:Cargo.lock`) and `cargo_deny_check()` runs
  inside `run_ci`, this instance's one gating row. #773 is the one-line lockfile bump.
  `Conflicts with: 738` — #738 adds `--chunk-size` to `cmd_s3` and threads it through
  `serve_s3_role`/`serve_s3_dispatch`/`serve_s3`, the same functions this touches.
  `Conflicts with: 742` — #742 reworks `xtask/src/dist.rs`'s `obtain_binary` and
  `deploy/docker/wyrd/Dockerfile`, which this slice's packaging rung edits; #742's brief
  already expects `--build-arg WYRD_VERSION` to be present on its base. Both edges are
  inherited by the children (see Split line for which child carries which).
  #773 (the h2 RUSTSEC bump this gated on) landed on `main` via getwyrd/wyrd PR #790 on 2026-09-11, outside the cycle, so the edge was dropped on 2026-09-30: the gate it protected (`cargo deny`) is green on `main`.
- **Surfaces:** data
- **Difficulty:** high
- **Scope:** SPLIT — see below. As a whole: (a) a build script bakes the build identity
  into `wyrd`, single-sourcing its derivation with `cargo xtask dist`; (b) the packaging
  path passes that identity INTO the image build, because `.dockerignore:6` excludes
  `.git/` and `dist` builds the shipped binary inside the image; (c) the `role started`
  event records it; (d) `S3Config` carries it and `handle` stamps `Server: wyrd/<v>` on
  every response; (e) the release smoke step compares the wire value with the tarball's
  `VERSION`. / **out of scope:** any capability-negotiation protocol or capability list on
  the wire (the issue says so explicitly — one header, one string); a `wyrd --version`
  subcommand; the version on any other role's wire surface; changing the workspace
  `version = "0.0.0"` placeholder; the OCI image's `org.opencontainers.image.version`
  label, which `dist` already sets (`xtask/src/dist.rs:458`).
- **External dependencies:** none.
- **Test file:** SPLIT — one per child, each a NEW file under `crates/server/tests/`
  (confirmed against this instance's classifier: `_added_files` × `_is_test_file`,
  `engine/scripts/run-verify.sh:143-145`, makes an ADDED `*/tests/*.rs` the discriminator;
  appending to an existing suite degrades the gate to green-only).
- **Disposition hint:** split

## The two decisions four Do rounds could not make

Both axes below were left open by the previous brief, which is why iterate-do could not
converge: the builder re-litigated them each round and every round's reviewers reopened
them. They are decided **here**, at Plan, and the children inherit the decisions.

### Decision 1 — the identity names a COMMIT; `--dirty` is dropped

The v4 adversary review measured the trap precisely: the batch review's four blocking
findings all ask for a **wider** `build.rs` rerun-watch set, while the adversary measured
that the existing `.git/index` watch already "buys most of that cost back" — any
`git status` rewrites `.git/index`, and cargo recompiles a crate whose build script re-ran
even when its output is byte-identical, so `wyrd-server` plus its ~41 integration-test
binaries relink on the ordinary edit→status→test loop. *"The two cannot both be satisfied
by adding paths — pick a side."*

**The side picked: the baked identity is the commit the build was made FROM, and it makes
no claim about the working tree.** `git describe --tags --always`, no `--dirty`. That
changes only when `HEAD`/refs change, which a narrow watch set covers exactly, so the
T3 FAIL and all four batch-review blockers dissolve rather than being argued down: there is
no clean-vs-dirty claim left to go stale. A hand-built binary from an edited tree
advertises its base commit, and the child's module doc must say so plainly. Release builds
are unaffected — they come from `dist`, which injects the version explicitly.

This reverses the previous brief's design decision 2 ("a dirty build must advertise as
dirty"), deliberately and on the record.

### Decision 2 — the full identity stays on the wire, including for unauthenticated callers

v4's adversary raised it as `[human]`: the header is stamped on the pre-auth 403, so an
anonymous `curl` learns the deployment's exact commit, and
`docs/design/architecture/14-threat-model.md:89` ("Information disclosure") was never
revisited. Real S3 answers the coarse `Server: AmazonS3`.

**Accepted, and recorded rather than coarsened.** Coarsening defeats the version-keying
this issue exists for (proposal 0017 §3 keys a capability matrix to the exact build), and
an operator who needs it suppressed can rewrite `Server:` at a fronting proxy. Decision 1
already removes the dirty-tree half of the disclosure. The child that puts the value on the
wire adds the threat-model line; **no configuration knob** is in scope — that is a
follow-up issue if an operator ever asks for one.

## Split line

Four rounds of evidence say where this divides. The header half was never the problem:
v4's adversary could not refute its red→green, and it passed every rubric every round.
Every blocking finding in v3 and v4 landed on the **build-time derivation** — `git describe`
scoping, the reftable watch set, the rerun-watch set (T3 FAIL + 4/4 batch-review blockers),
`is_identity`'s missing Docker tag rule. So the split isolates the risk rather than merely
halving the byte count.

The previous brief rejected splitting at *"emit a header" | "give it a real version"* — that
child ships an inert `wyrd/unknown` whose test the second child rewrites. **Inverting the
order removes that objection**: derivation first, header second, neither child inert.

The split is **accepted and materialised** — these are filed tracker issues with their own
bundles, not placeholders (`split-lineage.json`: `children = ["778", "779"]`;
`results/issue_778/brief.md`, `results/issue_779/brief.md`):

1. **#778 (child 1) — the build identity, derived, injected and logged.** The build script,
   the shared normalizer, the packaging rung that carries the version into the image build,
   and the `role started` field. Binding criterion observes the built binary's startup log.
   Carries `Depends on: 773` and `Conflicts with: 738, 742`.
2. **#779 (child 2) — `Server: wyrd/<version>` on every S3 response** (`Depends on: 778`;
   #773 reaches it transitively through the wave order). The `S3Config` seam, the one stamp
   point, the composition root, the threat-model line, and the release-smoke comparison of
   the wire value against `VERSION`. Its criterion asserts the header's remainder is
   **byte-equal to #778's startup-log field**, which closes the "can go green on
   `wyrd/unknown`" hole by construction and needs no derivation logic in the test. Carries
   `Conflicts with: 738, 742`.

## Plan-review response (revision pass, issue #301)

Four NEEDS-HUMAN findings were raised against this brief *after* the split was accepted, so
the reviewer read the parent in isolation. Every factual claim in them was re-verified
against `origin/main` (`65ca4fd`) and all four hold as stated. Three are criterion-level and
therefore land where the criteria BIND — in the children, whose Do never reads this file;
this brief records the disposition, and the children's briefs carry the change.

1. **"The criterion does not falsify the server-error category."** *Accepted — real gap,
   fixed.* Verified: a 5xx *is* structurally covered (`dispatch`'s error responses return
   into `handle` and pass its single stamp point, `crates/gateway-s3/src/lib.rs:1541-1557`),
   but structure is not proof and the invariant names the category explicitly. A
   deterministic 5xx exists and needs no fault injection: a **signed** `GET /<bucket>?acl`
   is refused `501 NotImplemented` by the subresource denylist (`acl` is listed at
   `lib.rs:342-360`; the refusal is at `:1655-1661`), post-auth, without the bucket having
   to exist. Added as a fourth leg to **#779**'s binding criterion, with the signing gotcha
   named there (`sigv4::sign`'s 3rd argument is the raw query — the existing `signed_headers`
   helper passes `""`, `s3_http_wire.rs:94-105`, so it cannot be used unchanged). A true
   `500 InternalError` needs seam fault injection and stays out of scope; that is stated in
   #779 rather than left implied.
2. **"Equality with the tarball's `VERSION` has no PR-time red→green gate."** *Accepted;
   already declared, now with a named observation route.* Verified: `VERSION` is written
   only by `cargo xtask dist` (`xtask/src/dist.rs:566-571`), `dist` is deliberately outside
   `cargo xtask ci` (`:26-28`), and `.github/workflows/release.yml` triggers on `push: tags:
   ["v*"]` **or `workflow_dispatch`** (`:20-24`) — and no `v*` tag has ever been cut (the
   repo's only tag is `archive/backup-premerge-signoff`). The binding criteria were already
   narrowed to what a container-free gate can observe (#778 proves the *coupling*: one
   normalizer, and the build argument carrying the very string written to `VERSION`; #779
   writes the end-to-end comparison into the smoke step). What the finding correctly missed
   was a *route* to observe it before a tag: the `workflow_dispatch` trigger runs the same
   dist + smoke job on any branch, and `cargo xtask dist` + a `VERSION`-vs-startup-log
   comparison runs locally where Docker and a network are available. Both are recorded in
   the children's Verification posture as out-of-band checks — deliberately NOT added to
   `External dependencies` (which stays `none`), because neither is needed to build the
   slice or to make its binding criterion go red→green.
3. **"The identity contract forks on a dirty checkout."** *Accepted — the fork is real and
   the brief was silent about it; scoped, not removed.* Verified: `derive_version` runs
   `git describe --tags --always --dirty` and `normalize_describe` appends `.dirty`
   (`xtask/src/dist.rs:119-146`, `:355-363`), while Decision 1 drops `--dirty` from the
   binary's own derivation. There is no contradiction once the equality is scoped, because
   `dist` **hands its own derived string to the image build** and the binary takes it
   verbatim (rung 1): a dist build on a dirty tree bakes the same `.dirty` string it writes
   to `VERSION`, so byte equality holds there too. The divergence exists only for a local
   `cargo build`, which produces no tarball. The Goal above now says so, and **#778** gains a
   green-only assertion covering exactly this choice — rung 1 used verbatim including a
   `.dirty` value, rung 2 never emitting one — so the decision is tested, not merely written.
4. **"The split names a phantom base (`736a`)."** *Accepted — stale labels, corrected.* The
   `736a`/`736b` labels were the split proposal's internal names; the split has since been
   accepted and materialised as tracker issues **#778** and **#779** with bundles at
   `results/issue_778/` and `results/issue_779/` (`split-lineage.json`). `dependency-state.json`
   listed only `773` because it was written before the split. The Split line above now names
   the real ids and their edges: #778 `Depends on: 773`, #779 `Depends on: 778`, both
   `Conflicts with: 738, 742`.

This brief itself is a **closed split parent** (`close-disposition: split`) and will not be
built; it stands as the record of the two binding decisions the children inherit.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR
MAY happen during the cycle. The PR MUST NOT be marked ready before sign-off accepts.
