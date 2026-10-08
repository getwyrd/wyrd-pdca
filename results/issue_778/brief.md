# Brief — issue 778 / wyrd-build-identity-derived-and-logged

> Plan artifact (docs 02 §PLAN). Do reads ONLY this file (plus the peer callsites cited
> under **Citations expected**). The `- **Label:** value` lines are parsed by the driver.
>
> **Child 1 of 2** from the split of #736 (`results/issue_736/split-proposal.md`). #779
> puts this identity on the S3 wire and depends on this slice. Everything about the
> `Server:` header belongs to #779 — do not anticipate it here.
>
> Plan of record: `docs/design/proposals/draft/0017-blackbox-validation-tool.md`
> §Dependencies and §3. Read in place in the target checkout — never copied here.

- **Slug:** wyrd-build-identity-derived-and-logged
- **Kind:** enhancement
- **Defect:** Nothing compiled into the `wyrd` binary can report which checkout produced
  it. Verified on `origin/main` at `a801997`: `crates/proto/build.rs` is the tree's ONLY
  build script (`git ls-tree -r --name-only origin/main | grep build.rs`), and the `s3`
  role's `role started` event records `role`, `listen`, `region`, `dservers` and nothing
  else (`crates/server/src/cli.rs:2199-2206`). The packaging pipeline DOES derive a version
  — `derive_version` → `normalize_describe` (`xtask/src/dist.rs:356-364`, `:127-160`) — and
  writes it to the tarball's `VERSION` file (`:566-571`), but that string exists only
  outside the binary. Worse, it cannot be re-derived where the shipped binary is compiled:
  `.dockerignore:6` excludes `.git/`, and `dist` builds the binary inside the image so that
  "the tarball's `bin/wyrd` is bit-identical to the image's `/usr/local/bin/wyrd`"
  (`xtask/src/dist.rs:5-7`). So the artifact's `VERSION` and the binary's self-knowledge
  have no common source and no way to acquire one without an explicit hand-off.
- **Goal:** The `wyrd` binary knows the build identity of the checkout it was compiled
  from, that identity is the SAME string `cargo xtask dist` writes to the tarball's
  `VERSION` **for the binary that same `dist` run produced**, and every `wyrd s3` process
  records it in its startup log. The equality is scoped to a DIST-BUILT artifact and its own
  tarball, and that scoping is a decision, not a hedge — see "What equality with `VERSION`
  does and does not claim" below.
- **Success criterion:** BINDING (demonstrable by C4-verify at Check — no container, no
  cluster, no feature flag): a new integration test spawns the **built `wyrd` binary** as
  an `s3` role (`--s3-listen 127.0.0.1:0 --data-dir <tmp> --access-key … --secret-key …
  --log-format json`, killed on every exit path), reads its stderr, and asserts against that
  one child process — the credential flags are not optional decoration: `cmd_s3` refuses to
  start without them (`crates/server/src/cli.rs:2126-2136`, "there is no anonymous access"),
  and `--data-dir` must be a temp dir because it otherwise defaults to a shared location
  (`cli.rs:2120`):
  1. **the identity is recorded** — the `role started` JSON event carries a **`version`**
     field whose value is non-empty, is not `unknown`, and is not EQUAL to the bare
     workspace placeholder `0.0.0` (equality, not a prefix test: the legitimate derived
     value on an untagged checkout is `0.0.0+git.<sha>`, which starts with `0.0.0` and is
     correct);
  2. **it came from git, not from a constant** — the test independently runs
     `git rev-parse --short HEAD` and `git describe --tags --always` from the workspace
     root. When the independent probe SUCCEEDS and `describe` does not resolve exactly onto
     a tag, the recorded value MUST CONTAIN that short sha. Containment rather than string
     equality: it binds the value to the real derivation while staying immune to tag shape,
     and it must not be weakened to a skip. **`unknown`/`0.0.0` while `git rev-parse`
     succeeds is a hard FAILURE, not a skipped leg** — v3's round found a self-minted skip
     predicate that silently disabled the only provenance check while staying green
     (`iteration-v3/` carry-forward, item 2). Skipping is permitted only when the
     independent probe itself fails (no repository visible at all), and then the leg must
     print why.
  The test MUST NOT call the production normalizer or reference any symbol this patch
  introduces — see Falsifiability.
  ALSO REQUIRED, gated by `cargo xtask ci` rather than by the discriminator (see
  Verification posture): (3) the derivation is single-sourced — the existing
  `normalize_describe_covers_all_three_shapes` test (`xtask/tests/dist_templates.rs:301`)
  keeps passing **unmodified**, which pins `dist`'s consumer to the one shared definition;
  and (4) the packaging hand-off is asserted container-free — the image build's argument
  vector carries `WYRD_VERSION=<the same string `dist` writes to `VERSION`>`, and the
  Dockerfile declares `ARG WYRD_VERSION` in the build stage; and (5) **the dirty-checkout
  contract is pinned by a pure-function test** — the rung resolution is extracted as a pure
  function (env value, describe output, fallback ⇒ identity) so the build script is a thin
  caller of it, and its unit tests assert BOTH halves of the scoping decision: a rung-1 value
  is used **verbatim, including a `.dirty` suffix** (this is what makes a dist build on a
  dirty tree byte-equal to its own `VERSION`), and a rung-2 derivation **never** produces
  one (no `--dirty` is passed and no `.dirty` reaches the identity, Decision 1). The same
  pure function is where the validator's rules (leading `-`/`.`/`+`, length cap, fail-closed
  to the sha form) get their tests. These are green-only assertions run by `cargo xtask ci`,
  in the production crate and reverted with it — they are NOT part of the discriminator test
  file and do not weaken its red leg. If the pure function must be shared between `build.rs`
  and the crate, an `include!`d module is the cheap idiom; say what you chose in
  `build-notes.md`.
- **Falsifiability:** RED is producible on the ordinary developer harness Do is pointed at
  — `cargo test -p wyrd-server --test build_identity_startup_log`, no Docker, no cluster,
  no feature flag. On the reverted tree the `role started` event has no `version` field at
  all (verified: `cli.rs:2199-2206` records four fields, none of them a version), so leg 1
  fails on a missing field. This instance's `C4-verify` **reverts the production change and
  keeps the added test** (`engine/scripts/run-verify.sh:508-517`) — an ADDED `build.rs` is
  `rm`ed, a modified `cli.rs` is `git checkout`ed — so the RED leg compiles only if the
  test names NO symbol this patch introduces. Drive `env!("CARGO_BIN_EXE_wyrd")` (the
  `cli_roundtrip.rs:11` idiom) and observe stderr only; do not `use` the new module, the
  new constant, or the new normalizer from the test file. A test that calls net-new API
  fails to COMPILE on the red leg, which the gate correctly scores `UNVERIFIABLE`
  (exit 77 → SUMMARY §6, `run-verify.sh:527-540`) rather than as a proven red.
  The child process MUST be killed on every exit path — the `s3` role blocks forever. Read
  its stderr until the `wyrd s3: serving S3-compatible HTTP on <addr>` line
  (`cli.rs:2183-2186`, which reports `listener.local_addr()`) so the ephemeral port is
  observed rather than guessed and two tests can run concurrently. A test that lets the
  pre-fix binary run unbounded turns a red into a hang, which is worse than either verdict.
  Environment fact checked so leg 2 cannot false-fail: every tree the gates build in is a
  git worktree, and on `main` today `git describe --tags --always` prints `65ca4fd`-style
  bare short shas (the repo's only tag is `archive/backup-premerge-signoff` and it is not
  an ancestor), which normalizes to `0.0.0+git.<sha>` — non-empty, not `unknown`, not equal
  to the placeholder, and containing the sha. If Do finds a gate environment where no
  repository is visible at all, say so in `build-notes.md` rather than quietly weakening
  leg 2 into a skip.
- **Invariant to restore:** *A `wyrd` binary can name the source revision it was built
  from, and that name is the same one the artifact around it carries.* Stated over the
  build/packaging category, not over one call site: it is violated equally by a binary that
  knows nothing, by a binary that derives its own answer differently from `dist`, and by a
  shipped binary whose build stage cannot see the repository. SELF-TEST: this cannot be
  satisfied by guarding a single module — it requires one derivation with two compiled
  consumers plus an explicit hand-off across the container boundary. Source: the repo's own
  single-source-of-truth rule for exactly this class of cross-file coupling — "`FDB_VERSION`
  is the SINGLE SOURCE OF TRUTH" (`deploy/docker/wyrd/Dockerfile:17-26`), pinned across
  three files by `xtask/tests/fdb_image.rs` and read back by
  `dist::dockerfile_fdb_version` (`xtask/src/dist.rs:165-178`).
- **Repo + branch target:** getwyrd/wyrd @ main
- **Conflicts with:** 738, 742
- **Ordering note:** `Depends on: 773` is a GATE dependency, not a code one: `cargo deny
  check` fails on `main` (RUSTSEC-2026-0258, `h2 0.4.15`, patched `>= 0.4.16`) and
  `cargo_deny_check()` runs inside `run_ci` — this instance's one gating row — so this
  bundle would fail `C4-ci` for a reason with nothing to do with its own patch. Re-verified
  2026-08-19: `h2 0.4.15` is still in `origin/main:Cargo.lock`, so the edge is live. #773 is
  the one-line lockfile bump. `Conflicts with: 738` — #738 adds `--chunk-size` parsing to
  `cmd_s3`, the same function whose `role started` event this slice extends
  (`cli.rs:2199-2206`); same function, overlapping hunks. `Conflicts with: 742` — #742
  reworks `obtain_binary` in `xtask/src/dist.rs` and the `build` stage of
  `deploy/docker/wyrd/Dockerfile`, both of which this slice's packaging rung edits; #742's
  own brief already states its base "will ALREADY carry #736's `--build-arg WYRD_VERSION`
  inside `obtain_binary`", i.e. it expects this slice merged first, and it is last in the
  batch. **#779 depends on this bundle** and reads the constant this slice exposes.
  #773 (the h2 RUSTSEC bump this gated on) landed on `main` via getwyrd/wyrd PR #790 on 2026-09-11, outside the cycle, so the edge was dropped on 2026-09-30: the gate it protected (`cargo deny`) is green on `main`.
- **Surfaces:** data
- **Difficulty:** high
- **Sizing note:** expect the sizer to band this `watch`/`oversized` (`difficulty=high`,
  two declared conflicts). It is the deliberately-isolated hard half of the #736 split and
  is not to be split again: the build script, the shared normalizer and the container
  hand-off are one decision (a single identity with two compiled consumers), and any
  further cut leaves a child with no independently observable criterion — a build script
  whose value nothing reads, or a build-arg carrying a version nothing derives.
- **Scope:** (a) a build script for `crates/server` bakes the build identity into the
  binary and exposes it as a public crate-level constant, named EXACTLY
  `wyrd_server::version::BUILD_IDENTITY` (a `&str`) — the path and the name are fixed here,
  not left to taste, because #779 reads that constant and its Do never sees this brief;
  (b) the identity's derivation is single-sourced with `cargo xtask dist`'s
  — one definition of the normalizer, compiled by both consumers, with the dependency
  direction tooling→product and never the reverse; (c) `cargo xtask dist` passes the version
  it already derived INTO the image build as a build argument, and the Dockerfile declares
  it in the build stage and exports it to the `cargo build`, so the shipped binary carries a
  real identity rather than the fallback; (d) the `s3` role's `role started` event gains a
  **`version`** field (that field name is fixed here too — #779 asserts on it) carrying the
  same string. **/ out of scope:** any wire surface — no `Server:` header, no `S3Config`
  change, nothing in `crates/gateway-s3` (that is #779, and a patch touching it here will be
  rejected); `--dirty` or any working-tree claim (Decision 1); a `wyrd --version`
  subcommand; the `role started` events of the `d-server` and `custodian` roles
  (`cli.rs:1039`, `:1664`) — uniform and cheap, but a separate issue, not scope creep here;
  changing the workspace `version = "0.0.0"` placeholder (root `Cargo.toml`); the OCI
  `org.opencontainers.image.version` label, which `dist` already sets
  (`xtask/src/dist.rs:458`).
- **Repro instruction:** On `origin/main` at `a801997`, in the target checkout:
  `git ls-tree -r --name-only origin/main | grep 'build\.rs'` → `crates/proto/build.rs` and
  nothing else. `git -C ../wyrd show origin/main:crates/server/src/cli.rs | sed -n
  '2199,2206p'` → the `role started` event's four fields, no version. Start
  `wyrd s3 --s3-listen 127.0.0.1:0 --data-dir <tmp> --access-key k --secret-key s
  --log-format json` and read stderr: the emitted `role started` object has no `version`
  key. Equivalently `grep -n "VERSION" xtask/src/dist.rs` shows the version exists only
  inside the packaging pipeline.
- **External dependencies:** none.
- **Test file:** `crates/server/tests/build_identity_startup_log.rs` — a NEW file, and the
  patch's **ONLY** added `*/tests/*.rs`. Confirmed by DRY-RUNNING this instance's own
  classifier on a synthetic patch carrying exactly this slice's expected file set
  (`./engine/scripts/run-verify.sh --classify`), which returned
  `ADDED_TEST crates/server/tests/build_identity_startup_log.rs` + `CRATE crates/server` +
  `CRATE xtask`: `_added_files` × `_is_test_file` (`engine/scripts/run-verify.sh:143-145`)
  makes an ADDED `*/tests/*.rs` the discriminator, both crate dirs exist on base so
  `GREEN_ONLY` stays 0 (`:412`), and the gate runs
  `-p wyrd-server --test build_identity_startup_log` (`:408`). Appending to an existing
  suite would silently degrade the gate to green-only and prove nothing.
  **Do MUST NOT create a new file under `xtask/tests/`.** Dry-run on the same synthetic
  patch plus an added `xtask/tests/dist_version_arg.rs` returns TWO `ADDED_TEST` rows, so
  that file becomes a second discriminator — and legs 3-4 necessarily name API this patch
  introduces, so with production reverted the RED leg fails to COMPILE, runs zero tests, and
  `_red_verdict` (`:227-232`) scores the WHOLE row `UNVERIFIABLE` (exit 77 → SUMMARY §6)
  instead of a proven red. The leg 3-4 assertions therefore go into the EXISTING
  `xtask/tests/dist_templates.rs`, which is modified rather than added and is reverted with
  the rest of the production change. **Leg 5 obeys the same rule by a different route:** its
  pure-function assertions are `#[cfg(test)]` unit tests INSIDE the production module they
  test (`crates/server/src/version.rs` or whatever file the `include!`d resolver lands in) —
  a source file, not a `*/tests/*.rs`, so the classifier is untouched and the assertions are
  reverted with production like legs 3-4. Do MUST NOT add
  `crates/server/tests/<anything>.rs` for it; that would be a second `ADDED_TEST` row and
  the same `UNVERIFIABLE` trap.
- **Verification posture:** The BINDING criterion (legs 1-2) is the default posture — a
  flippable test, red pre-fix and green post-fix at Check. Two things are declared here so
  they land as pre-declared sign-off items rather than surprises:
  * **Legs 3-5 are green-only by nature and run under `C4-ci`, not the discriminator.**
    Legs 3-4 are net-new assertions in `xtask/tests/` and leg 5 is a unit test inside the
    production module (the discriminator only compiles
    `-p wyrd-server --test build_identity_startup_log`), so their "red" is criterion
    absence. Make leg 4 a real assertion rather than a grep of our own source: extract the
    image build's argument vector from `obtain_binary` as a **pure function** and assert on
    its output — `xtask/src/dist.rs` already keeps `image_tag_version` (`:161-163`) and
    `dockerfile_fdb_version` (`:165-178`) pure and unit-tested in `ci`, and the module says
    so ("the pure decisions … are unit-tested inside `ci`", `:26-28`). If extraction reaches
    further than expected, a file-read + substring assertion in the
    `the_installer_pin_shares_the_dockerfile_source_of_truth` style
    (`xtask/tests/dist_templates.rs:280-298`) is an acceptable fallback; say which you chose
    and why in `build-notes.md`.
  * **The end-to-end equality with the tarball's `VERSION` is NOT observable in this slice
    and is not claimed.** `cargo xtask dist` needs Docker and a network and is deliberately
    outside `ci` (`xtask/src/dist.rs:26-28`). What this slice proves is the *coupling* —
    one normalizer, and `dist` passing the very string it writes to `VERSION` into the build.
    The end-to-end comparison is written into the release smoke step by **#779**, and is
    itself unobserved until a `v*` tag exists (none has ever been cut). Do must state that
    plainly in `build-notes.md` and must not claim it as demonstrated, and must NOT attempt
    an image build. Two out-of-band routes DO exist for a human who wants the pair observed
    before a tag — the release workflow's `workflow_dispatch` trigger
    (`.github/workflows/release.yml:20-24`; the job has no tag guard) and a local `cargo
    xtask dist` followed by comparing `target/dist/wyrd-*/VERSION`'s `version:` line with the
    spawned role's logged `version`. Both need Docker and a network, both are the human's
    call at sign-off, and **neither is needed to build this slice or to make the binding
    criterion go red→green** — which is why `External dependencies` stays `none`.
  * **Known environmental gate noise, not a defect of this patch:** the advisory
    `C5-mutants` row fails its own unmutated baseline on this repo because cargo-mutants'
    scratch copy carries no `.git` and `xtask/tests/repo_hygiene_guards.rs:137` requires
    `git ls-files` to succeed. It has failed that way for every #736 round. Do should not
    contort the design around it.
- **Production reach:** Not applicable in the seam sense — production traverses the change
  at Check: the test drives the real binary, whose real build script produced the value the
  real composition root logs. One honest limit to record in `build-notes.md`: on a checkout
  with no tag and no override the advertised string is the `0.0.0+git.<sha>` shape, which is
  a real build identity but not a release version; the release-shaped value appears once a
  tag exists.
- **Citations expected:** Do must cite `path:line` on `main` for every change. Peer
  callsites Do MAY open and should mirror:
  * **The derivation to single-source** — `xtask/src/dist.rs:127-160` (`normalize_describe`,
    the three documented shapes) and `:356-364` (`derive_version`, which runs `git describe`
    and applies it). Its behaviour is already pinned by
    `xtask/tests/dist_templates.rs:301-322`, which must keep passing UNCHANGED.
  * **Where that version is written to the artifact** — `xtask/src/dist.rs:566-571` (the
    `VERSION` file). The build argument must carry the SAME `version` binding, not a
    re-derivation.
  * **The image build's argument vector to extend** — `xtask/src/dist.rs:450-470`
    (`--build-arg FEATURES=…`, the `org.opencontainers.image.version` label, the `-t` tags
    built through `image_tag_version`). Add the version argument the same way.
  * **The Dockerfile's existing build-arg convention** — `deploy/docker/wyrd/Dockerfile:31`
    (`ARG FEATURES=""` — a global arg with an empty default), `:37` (re-declared inside
    the `build` stage) and `:66` (`RUN cargo build --release --locked --bin wyrd …`). Mirror
    it exactly: an unset argument must still build and fall through to the next rung, never
    bake an empty identity.
  * **The role-started event to extend** — `crates/server/src/cli.rs:2199-2206` (today
    `role`, `listen`, `region`, `dservers`; add `version` beside them). It reaches stderr
    through the process-global subscriber (`crates/server/src/logging.rs:301-329` — "writing
    to **stderr**") and `--log-format json` (`cli.rs:496`) makes it machine-readable, which
    is how leg 1 asserts on it.
  * **Driving the built binary from an integration test** —
    `crates/server/tests/cli_roundtrip.rs:11` (`const WYRD: &str =
    env!("CARGO_BIN_EXE_wyrd");`). That is the idiom; this test differs only in that its
    child is a long-running server, so it is spawned, read from, and killed rather than
    `output()`-ed.
  * **A build script is a crate root for the #616 unsafe guard** —
    `xtask/tests/repo_hygiene_guards.rs:257-294`
    (`scan_crate_roots_covers_build_scripts_benches_and_examples`): `build.rs` is scanned, so
    it needs `#![forbid(unsafe_code)]` or `cargo xtask ci` goes red.
- **Prior-art check (triage cycles):** searched by affected path on `origin/main` at
  `a801997`. `git log --oneline -- xtask/src/dist.rs` → one commit, `f5d4575` ("dist(570):
  one pipeline ships the operator tarball and the OCI image"), which introduced
  `normalize_describe`; nothing since. `git log --oneline -- crates/server/src/cli.rs` →
  role/telemetry and lint-sweep work, no version plumbing. No `build.rs` has ever existed
  under `crates/server`. `gh pr list --state all --search "WYRD_VERSION"` → nothing. The
  only prior attempt at this work is #736's own four rounds, preserved under
  `results/issue_736/iteration-v{1,2,3,4}/`; this brief supersedes them and the two
  decisions below are what they lacked.
- **Disposition hint:** new-feature

## The decision four Do rounds could not make (parent Decision 1 — binding)

Rounds v3 and v4 of #736 both blocked here, and the v4 adversary review named the trap
precisely: the batch review's four blocking findings all asked for a **wider** `build.rs`
rerun-watch set, while the adversary measured that the existing `.git/index` watch already
buys most of the cost back — any `git status` rewrites `.git/index`, and cargo recompiles a
crate whose build script re-ran even when its output is byte-identical, so `wyrd-server`
plus its ~41 integration-test binaries relink on the ordinary edit→status→test loop. *"The
two cannot both be satisfied by adding paths — the builder should pick a side explicitly."*

**The side, decided at Plan and not open to re-litigation in Do: the baked identity names
the COMMIT the build was made from, and makes no claim about the working tree.**

* `git describe --tags --always` — **no `--dirty`**, and no `.dirty` suffix reaching the
  identity from any rung.
* The answer therefore changes only when `HEAD`/refs change, so the rerun-watch set covers
  exactly that and nothing wider. Do not watch the git index; do not watch source files.
* The module doc MUST state the limit plainly: this names the commit the build was made
  from, not an attestation that the tree was unmodified. A binary hand-built from an edited
  tree advertises its base commit — that is the honest reading, and release builds are
  unaffected because `dist` injects the version explicitly.

This reverses #736's previous design decision 2 ("a dirty build must advertise as dirty"),
deliberately and on the record. It dissolves the T3 FAIL and all four batch-review blockers
rather than arguing them down: there is no clean-vs-dirty claim left to go stale.

## The identity must be usable where it is consumed

The v4 adversary landed a second finding worth carrying into the design rather than
rediscovering: the version reaches `docker buildx build -t wyrd:<tag>-<flavor>` through
`image_tag_version` (`xtask/src/dist.rs:161-163`, which maps `+`→`-`), and a Docker tag is
`[A-Za-z0-9_][A-Za-z0-9_.-]{0,127}` — the **first** character may not be `-`, `.` or `+`.
An identity that violates that passes an over-permissive validator and dies minutes later
inside the image build with `invalid reference format`.

So the resolved identity must be **validated at derivation time**, where the error is
cheap and legible: reject a leading `-`/`.`/`+`, cap the length, and reject bytes that are
not tag-legal after the `+`→`-` mapping. And when a derived value fails validation — an
exotic tag name, say — **fail closed to the sha-derived form with a `cargo:warning`, never
by collapsing forbidden bytes into a shared replacement character**: v3 found that
collapsing lets two distinct tags normalize to one identity, which is precisely the "can
silently be wrong" failure this work exists to remove. Distinct inputs must not become one
identity.

## What equality with `VERSION` does and does not claim (plan review, #301)

The tracker asks for the version to come from "the same source `cargo xtask dist` already
uses" — `normalize_describe` over `git describe --tags --always --dirty` — and to match the
tarball's `VERSION`. Decision 1 drops `--dirty` from the binary's OWN derivation, and the
plan review correctly asked whether those two can both be true. They can, and the resolution
is the rung order below rather than a compromise on either:

* **A dist-built binary never derives anything.** `dist` runs `derive_version` once — WITH
  `--dirty` (`xtask/src/dist.rs:355-363`) — writes that string to `VERSION`
  (`:566-571`) and hands the SAME binding to the image build, where rung 1 takes it
  verbatim. So on a dirty tree the shipped binary advertises exactly the `.dirty` string its
  own tarball carries: byte equality holds, dirty included, and `dist` remains the single
  authority on what a released artifact is called.
* **A locally-built binary derives rung 2 and names its base commit.** No `--dirty`, no
  working-tree claim (Decision 1), and no tarball exists around it — so there is nothing
  there for it to be unequal to. The equality claim is about an artifact PAIR, and a local
  `cargo build` produces only one half of the pair.
* **The fork is therefore in the derivation, not in the contract**, and it is asserted, not
  asserted-about: criterion leg 5 pins both halves as pure-function tests. The module doc
  must state the same thing in one sentence — a hand-built binary names its base commit;
  a released one carries `dist`'s word.

## Rungs, in resolution order

1. `WYRD_VERSION` from the build environment, used verbatim (after validation) — this is
   how the packaging pipeline hands in the value it has already derived, and it is the rung
   that exists because `.dockerignore:6` hides `.git/` from the image build;
2. otherwise `git describe --tags --always` in the workspace, normalized by the ONE shared
   normalizer — scoped to the workspace's own repository, so a checkout nested inside an
   unrelated git repo cannot bake a foreign commit's sha (v3's most severe finding);
3. otherwise the `0.0.0` fallback `derive_version` already passes (`xtask/src/dist.rs:363`).

An unset or empty build argument must fall THROUGH to rung 2, never bake an empty identity —
the `ARG FEATURES=""` convention at `deploy/docker/wyrd/Dockerfile:31` is the precedent.

## Plan-review response (revision pass, issue #301 — inherited from #736)

The antagonistic plan review ran against the parent #736 brief after the split; two of its
four findings bind HERE and both were accepted after re-verification against `origin/main`
(`65ca4fd`):

* **"The identity contract forks on a dirty checkout."** Real, and the brief had left it
  implicit. Fixed by SCOPING the equality (Goal, and the new section above) and by TESTING
  the choice (criterion leg 5: rung 1 verbatim including `.dirty`, rung 2 never emitting
  one). Decision 1 is unchanged — the fork was always intended; what was missing was saying
  so and pinning it.
* **"Equality with `VERSION` has no PR-time red→green gate."** Already declared under
  Verification posture as a coupling proof rather than an end-to-end one; what is added is
  the two out-of-band routes that observe the artifact pair without waiting for a tag, so
  sign-off can choose to spend one. The binding criterion was deliberately NOT widened to
  need Docker.

The remaining two findings landed on #779 (the missing server-error leg) and on the parent's
stale `736a`/`736b` labels; see `results/issue_736/brief.md`.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR
MAY happen during the cycle. The PR MUST NOT be marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 1): rebuilding for the implementation-level findings — T5 Judgment — Correct the independent probe’s repository scope — inherited `GIT_DIR` makes the test demand a foreign SHA even though production correctly reports this workspace, yielding false regression failures; `crates/server/tests/build_identity_startup_log.rs:46`, `crates/server/build.rs:77`, `reviewer-evidence/foreign-git.log:12`.; `crates/server/build.rs:119-127` (`watch_paths`) leaves the baked identity **stale on a reftable repository**. Reftable is git's newer ref storage format (`git init --ref-format=reftable`; Git 3.0 plans to make it the default). In such a repo, commits and tags only update `<common_dir>/reftable/`: `HEAD` stays the stub `ref: refs/heads/.invalid`, `refs/` holds only a stub `heads` file, and there is no `packed-refs`. None of the three watched paths ever changes, so the build script never re-runs. I reproduced this with git 2.53, using the patch's own `build.rs` and `derivation.rs` in a probe crate nested two levels deep:; `crates/server/src/version/derivation.rs:161`: rung 2 can still produce a **`.dirty` identity**, which the doc at `:138` says it never does ("this rung never produces `.dirty`"). The guard only checks for the `-dirty` suffix that `git describe --dirty` would add. A tag whose own name carries the marker passes straight through as `Rung::Describe`, with no warning. I compiled the shared file on its own and called `resolve(None, Some(d), Some("abc12de"), "0.0.0")`:; `crates/server/tests/build_identity_startup_log.rs:162` and `:169`: **the SKIP reason is invisible.** The brief allows leg 2 to skip only when the probe fails, "and then the leg must print why". The test prints with `eprintln!`, and libtest captures and discards that output when a test passes. I checked with a one-line `#[test]` that calls `eprintln!`: the output is just `test t ... ok`. So wherever the probe can't see a repo, leg 2 is skipped and the run shows a plain `ok`. Examples: cargo-mutants' scratch copy (no `.git`), an unpacked source tree, or a container where git refuses the checkout over ownership. That is the "silently green with the only provenance check off" pattern the brief's v3 carry-forward warns about. This does not affect the C4-verify evidence: the gate tree is a git worktree and the probe succeeded there. Fix: write the reason with `writeln!(std::io::stderr(), …)`. I verified that this bypasses capture and shows on a passing run. Alternatively, fail unless an explicit opt-out env var is set.; `crates/server/build.rs:120`: The rerun inputs omit Git’s `shallow` file. Deepening a shallow checkout can make an existing tag reachable without changing HEAD or refs, leaving the baked identity stale. Reproduced with this build script: `git describe` changed from a bare SHA to `v1.2.3-1-g<SHA>`, but a second Cargo build retained `0.0.0+git.<SHA>`. Track shallow-history changes and cover incremental rebuilds.; `crates/server/tests/build_identity_startup_log.rs:181`: The unconditional SHA-containment assertion rejects a supported build override. On an untagged commit, `WYRD_VERSION=1.2.3 cargo test -p wyrd-server --test build_identity_startup_log` fails even though the binary correctly reports `1.2.3`. Assert the compile-time override when supplied, while retaining the independent Git assertion for ordinary builds.; `crates/server/tests/build_identity_startup_log.rs:46`: The independent Git probe inherits repository overrides and searches enclosing repositories, unlike the build script. An unpacked source tree inside another repository correctly builds as `0.0.0+git.unknown`, but this probe finds the outer HEAD and fails the assertion at line 173; reproduced in a nested scratch fixture. Independently restrict discovery to the workspace’s own repository and clear repository-selection overrides.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 9 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_778/review-b. 3 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — ERROR cargo test failed in an unmutated tree, so no mutants were tested
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 9 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_778/review-b
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 2): rebuilding for the implementation-level findings — C5 Causal adequacy — Make full→shallow transitions refresh the identity — filtering absent watch inputs leaves an incremental build advertising an obsolete tag-derived value after tag reachability changes; independently reproduced at `crates/server/build.rs:43`, `reviewer-evidence/shallow-probe.log:44`.; `crates/server/build.rs:52` (with `:29`): **once a build lands on rung 3 it stays there.** When no repository is found, the script emits only `rerun-if-env-changed=WYRD_VERSION`, so nothing triggers a re-probe later. Repro with the probe crate: built with no `.git` → `0.0.0+git.unknown`; then `git init && git commit` → rebuild → still `0.0.0+git.unknown` while `git describe` prints `43076f1`. The same happens if `git` was missing from `PATH` on the first build, or if git refused the checkout ("dubious ownership") and the user later fixed `safe.directory`. On such a machine the binary keeps a wrong identity, and the patch's own test fails at `crates/server/tests/build_identity_startup_log.rs:245` ("the identity did not come from the repository") until someone runs `cargo clean -p wyrd-server`. Possible fix: on rung 3, emit a trigger that makes cargo re-probe, e.g. `rerun-if-changed=<root>/.git` (a missing path makes cargo re-run the script on every build — harmless in the one-shot image build, and only paid by builds that already warn). At minimum, put the `cargo clean -p wyrd-server` remedy in the rung-3 warning.; `gate-logs/C4-diff-cov.log:10`: **the bundle does not apply on `origin/main`** ("the bundle is stale; rebase Do"). This is the second round in a row. C4-ci and C4-verify ran against the pre-fix base `df68932f` (`f492a28` in the target), so the green evidence is for that base, not for the merge result. The declared conflicts (#738 `--chunk-size` in `cmd_s3`, #742 two-binary Dockerfile and dist) are already in the base, so something newer has moved. Rebase and re-run the gates.; `crates/server/build.rs:43`: Filtering out an absent `shallow` file leaves a full checkout unable to detect a newly introduced shallow boundary. Reproduced with the target's unchanged build script and resolver in a small Cargo fixture: after `git fetch --depth=1 --no-tags origin HEAD`, every watched path retained its mtime, while `git describe --tags --always` changed from `v1.2.3-1-g7e56f1b` to `7e56f1b`. The next incremental build still baked `1.2.3+git.1.7e56f1b` instead of `0.0.0+git.7e56f1b`. Detect creation of the shallow boundary as well as changes/deletion, and add an incremental-build regression covering this transition; the pure watch-list assertions do not exercise the existence filter. This confirms the duplicated finding in the frozen T4 log as one defect.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_778/review-b. 4 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — ERROR cargo test failed in an unmutated tree, so no mutants were tested
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_778/review-b
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
