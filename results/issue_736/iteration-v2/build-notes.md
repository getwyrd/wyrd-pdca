# Build notes — issue 736 / s3-server-version-header (iteration 2)

Target branch: `getwyrd/wyrd @ main` (worktree `/home/eddie/wyrd/wyrd.pdca-wt-l0`, base `a801997`).
All `path:line` citations below are **post-patch** line numbers in that worktree unless the
text says "pre-patch".

---

## 1. What the patch does

One sentence per file, in the order a reviewer meets them:

| File | Change |
|---|---|
| `crates/gateway-s3/src/lib.rs:102,109,122,142` | `SERVER_PRODUCT` / `UNKNOWN_SERVER_VERSION` consts, `server_header_value` (RFC 9110 token validation + logged degrade), `is_tchar`. |
| `crates/gateway-s3/src/lib.rs:163,177` | `S3Config.server_version`, defaulted in `S3Config::new` (only constructor — source-compatible for every existing caller). |
| `crates/gateway-s3/src/lib.rs:197,268,1548,1641` | The header is built **once** in `router()`, carried in `AppState`, and stamped in `handle` beside the request id — the one point every request reaches (`Router::new().fallback(handle::<G>)`, `:276`). |
| `crates/server/src/version.rs` (new) | The single-sourced pure derivation: `VERSION_ENV:40`, `FALLBACK_VERSION:47`, `normalize_describe:60` (moved verbatim from `xtask/src/dist.rs`), `resolve_version:114`. |
| `crates/server/build.rs` (new) | The impure half: reads `WYRD_VERSION`, asks git, emits `cargo:rustc-env` + a **complete** rerun-watch set (`:81`). |
| `crates/server/src/lib.rs:22,36` | `pub mod version;` and `BUILD_VERSION = env!("WYRD_VERSION")`. |
| `crates/server/src/cli.rs:2208,2387` | The `role started` event records `version`; `serve_s3` sets `config.server_version` from the baked constant. |
| `xtask/src/dist.rs:128,130` | Compiles the product's `version.rs` by `#[path]` and re-exports `normalize_describe`, so `dist::normalize_describe` keeps its call site (`:422`) and its test (`xtask/tests/dist_templates.rs:301`) unchanged. |
| `xtask/src/dist.rs:163,218,493,509` | `image_build_args` / `version_file` extracted pure; the image build passes `--build-arg WYRD_VERSION`; the `--host` build passes the same value in its environment. |
| `deploy/docker/wyrd/Dockerfile:39,47,76` | `ARG WYRD_VERSION=""` (unset-safe), re-declared in the build stage, exported as `ENV` for the `cargo build`. |
| `xtask/tests/dist_templates.rs:337,376` | Two container-free coupling assertions (build-arg list ⇄ `VERSION` string; Dockerfile ARG/ENV declaration). |
| `.github/workflows/release.yml:79-99` | The end-to-end `Server:` ⇄ `VERSION` comparison inside the existing installer smoke step. |
| `crates/server/tests/s3_server_version_header.rs` (new) | The binding test: drives the built binary as an `s3` role. |

---

## 2. Iteration-1 carry-forward — what changed and why

The previous attempt was rejected for two defect classes (5 blocking review findings, plus the
C5 baseline). Both are fixed by **removing the cause**, not by guarding the symptom.

### (A) "The rerun directives leave the baked identity stale" — 3 of the 5 findings

> `crates/server/build.rs:48` — *"Restricting reruns to HEAD, its branch ref, and packed-refs
> leaves `git describe --dirty` stale after ordinary unstaged source changes and after loose
> tags are created"* … *"Emitting only these narrow `rerun-if-*` directives disables Cargo's
> package-wide default"*.

Both halves are correct. I measured the mechanism rather than argue about it (scratch crate
under `$PDCA_SCRATCH`, `pdca-builder-736-cargoprobe`):

1. **Any `rerun-if-*` opts out of the package-file default.** With only
   `cargo:rerun-if-env-changed=PROBE_VERSION` emitted, `touch src/main.rs` did **not** re-run
   the build script (run count stayed at 1). So "emit fewer directives" is not a fix — the
   watch set must be *complete*.
2. **A watched path that does not exist re-runs the script on every build, and the crate is
   recompiled with it**: `Dirty probe v0.1.0: the file 'nonexistent-sentinel' is missing` →
   `Compiling probe` on *every* `cargo build`, three builds in a row, with byte-identical
   script output. So "always re-run" is not free (see §5 for its cost here).
3. **Cargo scans a watched directory recursively.** Watching `watched/` and then creating
   `watched/sub/newtag` re-ran the script (6 → 7); a no-op build did not (6 → 6). That is what
   makes a *loose tag* (`refs/tags/…`, possibly nested like this repo's
   `refs/tags/archive/backup-premerge-signoff`) an input the watch set really covers.

The fix, therefore:

* **The working tree is no longer an input.** `crates/server/build.rs:45` asks
  `git describe --tags --always` — **no `--dirty`**. A build script's answer is *cached*, so
  it can only stay true for inputs cargo can invalidate on; the working tree is not one of
  those (see §5 for the two rejected ways to keep `--dirty`). What remains is a pure function
  of HEAD and the tag refs.
* **The watch set is complete for exactly that function** (`crates/server/build.rs:81-110`):
  `WYRD_VERSION` (env), `build.rs`, `src/version.rs`, and — via `git rev-parse --git-path`, so
  it is correct in a linked worktree — `HEAD`, `packed-refs`, `refs/heads` and `refs/tags`
  (directories, hence recursive). Verified on this worktree, `cargo build -vv`:

  ```
  [wyrd-server 0.0.0] cargo:rerun-if-env-changed=WYRD_VERSION
  [wyrd-server 0.0.0] cargo:rerun-if-changed=/home/eddie/wyrd/wyrd/.git/worktrees/wyrd.pdca-wt-l0/HEAD
  [wyrd-server 0.0.0] cargo:rerun-if-changed=/home/eddie/wyrd/wyrd/.git/packed-refs
  [wyrd-server 0.0.0] cargo:rerun-if-changed=/home/eddie/wyrd/wyrd/.git/refs/heads
  [wyrd-server 0.0.0] cargo:rerun-if-changed=/home/eddie/wyrd/wyrd/.git/refs/tags
  [wyrd-server 0.0.0] cargo:rustc-env=WYRD_VERSION=0.0.0+git.a801997
  ```

  (`HEAD` resolves per-worktree, the refs to the common dir — the previous attempt's
  `--git-path` idiom, kept and now with `refs/heads` + `refs/tags` added so a *new* loose ref
  is seen. HEAD is detached in gate worktrees, so `symbolic-ref` contributes nothing here; the
  branch-file case is still handled for developer checkouts.)
* **The `.dirty` half is not lost where it matters.** `cargo xtask dist` derives with
  `--tags --always --dirty` at packaging time — fresh, uncached, so it is always true — and now
  passes that string in on **both** of its paths: `--build-arg WYRD_VERSION` for the image
  build (`xtask/src/dist.rs:179`) and `.env(WYRD_VERSION, …)` for the `--host` build
  (`xtask/src/dist.rs:493`, new in this iteration). So every artifact `dist` produces
  advertises byte-exactly what its `VERSION` file says, dirty flag included. Only a plain
  developer `cargo build` derives its own, and it names the **commit** — a value that can never
  be false, rather than a `.dirty` flag that could be.

### (B) "The test rejects `0.0.0`, which the build script legitimately produces" — 2 findings + the C5 baseline

> *"The test rejects `0.0.0` even though the new build script explicitly uses that value for
> supported repository-less/source-tarball builds"* — and the same collision is why C5 reported
> `ERROR cargo test failed in an unmutated tree` (cargo-mutants copies the tree **without**
> `.git`, so the baked value was the bare `0.0.0` the test rejected;
> `iteration-v1/gate-logs/C5-mutants.log:1639`).

Two ways to reconcile: weaken the assertion (skip it when no repository is visible), or make
the repository-less answer distinguishable. I took the second, because it removes the
ambiguity instead of teaching the test to tolerate it:

* `resolve_version` has **no third arm** any more (`crates/server/src/version.rs:114-120`).
  "git could not be asked" is `describe = None`, and `normalize_describe` *already* spells that
  case `<fallback>+git.unknown` (`crates/server/src/version.rs:84`, pre-existing behaviour, unchanged). A repository-less
  build therefore advertises `wyrd/0.0.0+git.unknown` — which states *why* it cannot name a
  commit, instead of impersonating the workspace placeholder that every build ever made shares.
* The test's leg-2 assertion stays **unconditional** (`crates/server/tests/s3_server_version_header.rs:331-354`):
  no supported build path can produce the bare `0.0.0`, so the assertion cannot false-fail on a
  legitimate configuration — and it still fires against the wrong implementation it exists for
  (`CARGO_PKG_VERSION`, i.e. the workspace `version = "0.0.0"`).

Proof it is reconciled, on the real code (not by reasoning):

```
$ GIT_DIR=/nonexistent WYRD_VERSION= cargo build -vv -p wyrd-server --bin wyrd
[wyrd-server 0.0.0] cargo:rustc-env=WYRD_VERSION=0.0.0+git.unknown
$ GIT_DIR=/nonexistent WYRD_VERSION= cargo test -p wyrd-server --test s3_server_version_header
skipped: no git repository is visible from this checkout
test the_advertised_version_carries_the_commit_it_was_built_from ... ok
test every_s3_response_advertises_the_baked_build_version ... ok
test result: ok. 2 passed; 0 failed
```

`GIT_DIR=/nonexistent` reproduces exactly the state cargo-mutants' scratch copy is in (no
repository reachable from `CARGO_MANIFEST_DIR`), through the production code paths.

### (C) The test also got *stronger*, not just looser

Because cargo re-exports a build script's `cargo:rustc-env` into the environment of the test
process it spawns — **measured**, in the same scratch crate: with the script emitting
`PROBE_VERSION=emitted` and the ambient environment holding `PROBE_VERSION=ambient`, the test
process read `Ok("emitted")` — the test can compare the wire value against the string this
build actually baked:

```rust
if let Ok(baked) = std::env::var(VERSION_ENV) { assert_eq!(advertised, baked, …) }
```

(`crates/server/tests/s3_server_version_header.rs:355-365`.) The guard is not a hedge: on the
`run-verify.sh` RED leg the production change (and so the build script) is reverted, the
variable is absent, and legs 1/3/4 still bind. When it *is* present it pins the whole chain —
build script → `BUILD_VERSION` → `serve_s3` → `handle` — in one equality. §4 shows it firing.

---

## 3. Deviations from the brief, stated plainly

1. **Rung 3's output shape.** Design says rung 3 is "the `0.0.0` fallback `derive_version`
   already passes". The *fallback argument* is still `FALLBACK_VERSION = "0.0.0"`
   (`crates/server/src/version.rs:47`) and `dist`'s call site is unchanged in meaning
   (`xtask/src/dist.rs:422`); what changed is that a repository-less build routes it through the
   same normalizer, yielding `0.0.0+git.unknown`. Reason: §2(B) — it is the only way to keep the
   criterion's "not the bare `0.0.0`" *unconditional* without making a supported build
   configuration fail. `dist` itself never reaches this state (`derive_version` errors if
   `git describe` fails), so no artifact changes shape.
2. **`--dirty` dropped from the build script's own derivation** (kept everywhere else). Reason:
   §2(A) — three of five blocking findings were this staleness class, and it is unfixable while
   the working tree is an input to a cached script. Design decision 2 ("a dirty build advertises
   as dirty") still holds for every artifact `dist` builds, because `dist` derives with `--dirty`
   and passes the string in.
3. **`--host` builds now get `WYRD_VERSION` in their environment** (`xtask/src/dist.rs:493`) —
   not in the brief's scope list, but it is the same coupling scope (d) asks for on the image
   path, and without it the `--host` tarball would be the one artifact whose binary could
   disagree with its own `VERSION` file (the `.dirty` case).
4. **Open question 1** ("where the `wyrd/` prefix lives") — answered as the brief assumed: the
   wire crate owns the prefix (`SERVER_PRODUCT`, `crates/gateway-s3/src/lib.rs:102`), the
   composition root passes a bare version. Exactly one place spells `wyrd/`.
5. **Open question 3** — confirmed, no change needed: `.github/workflows/release.yml:36-39`
   already checks out with `fetch-depth: 0` + `fetch-tags: true`.
6. **Open question 2** (other roles' startup logs) — deliberately not done; out of scope, worth a
   follow-up issue as the brief says.
7. **Docs currency (rubric).** No living-architecture doc updated: a response header is not a
   port, API operation, RPC, CLI flag or persisted field, and the peer invariant (#529's
   `x-amz-request-id`) documents none either — `grep -rn "x-amz-request-id" docs/` returns
   nothing. `docs/design/proposals/draft/0017-blackbox-validation-tool.md:259,854` says "the
   gateway advertises no version today"; that is a *dependency register* entry that names #736
   as its tracking issue, and it is the consuming tool slice that flips it (together with
   `--server-version`'s interim status). Editing another slice's draft proposal here would be
   scope creep; flagged for the human instead.

---

## 4. Forced refutation — the three questions, with evidence

**(a) Genuine red?** Yes — with the production change fully reverted (every modified file
`git checkout`ed, both added production files moved out, only the added test kept: the exact
shape `engine/scripts/run-verify.sh:508-517` reconstructs), the test **compiles and fails**:

```
thread 'every_s3_response_advertises_the_baked_build_version' panicked at
  crates/server/tests/s3_server_version_header.rs:260:9:
the signed PUT response carries no `Server` header … Head:
HTTP/1.1 200 OK
…
x-amz-request-id: c1e67122c16a9e710000000000000000
test result: FAILED. 0 passed; 2 failed
```

and green again after restoring the patch (`test result: ok. 2 passed`). Compiling on the
reverted tree matters as much as failing: a test that named a net-new symbol would be scored
`UNVERIFIABLE` (exit 77) instead of RED — this one references no symbol the patch introduces.

**(b) Production path?** Yes. The test spawns `env!("CARGO_BIN_EXE_wyrd")` as a real `s3` role
over a loopback listener, parses the ephemeral port out of the child's own startup line, signs
with the production `wyrd_gateway_s3::sigv4::sign`, and reads raw response headers off a
`TcpStream`. Nothing is mocked: the composition root (`cli::serve_s3`), the real router, the
real `handle`, the real build script's constant. Three targeted mutations of the **production**
code, each rebuilt and re-run, confirm the test is bound to it and not to a coincidence:

| Mutation (production) | Result |
|---|---|
| delete `config.server_version = crate::BUILD_VERSION.to_string();` (`cli.rs:2387`) | **both tests FAIL** — `left: "unknown", right: "unknown"` (the "header shipped inert" hole the plan reviewer named) |
| `config.server_version = "1.2.3".to_string();` | **both tests FAIL** — `the wire must advertise the identity this build baked in, verbatim: left: "1.2.3", right: "0.0.0+git.a801997"`, and leg 4: `the advertised wyrd/1.2.3 must carry the commit it was built from (a801997)` |
| delete `version = crate::BUILD_VERSION,` from the `role started` event (`cli.rs:2208`) | **FAILS** — `the role started event carries no version field: {"dservers":0,"listen":…,"message":"role started","region":"us-east-1","role":"s3"}` |

**(c) Fixture includes the fault?** Yes. The failing element here is the *error plane* — the leg
a per-handler implementation would miss — and it is in the fixture, not curated out: the test
asserts the header on a **403** as well as a 200, and requires the two to be the same string.
Leg 4 asserts against the *real* HEAD sha of the checkout the binary was built from (`a801997`
in this run, printed by the test), not a fabricated one. The two skips (no repository / HEAD
exactly a tag) are printed, and are the two states in which no sha exists to look for; leg 2's
`unknown`/`0.0.0`/`WYRD_VERSION` assertions bind in **every** environment, including the
repository-less one (proof in §2(B)).

---

## 5. Alternatives rejected, with the costs shown

**Keep `--dirty` and re-run the build script on every build** (a nonexistent-path sentinel, or
`git ls-files` as the watch set). Measured cost, not adjectival: an always-stale watched path
makes cargo print `Dirty probe v0.1.0` and `Compiling` on *every* build, even with identical
script output (§2(A) experiment 2). Applied here that is a recompile **and relink of
`wyrd-server` on every `cargo build`/`cargo test` invocation** — the `wyrd` binary plus the
**40** integration-test binaries under `crates/server/tests/` (`ls crates/server/tests/*.rs | wc -l`
= 40), on a crate whose `cli.rs` alone is 2 997 lines. The `git ls-files` variant is worse in
kind: it would emit ~1 file per tracked path and rebuild the server whenever *any* workspace
file changes, including docs. Rejected for a `.dirty` suffix that `dist` supplies correctly on
every shipping path anyway.

**Weaken the test instead: skip the `0.0.0` assertion when no repository is visible.** Cost is
not lines but binding strength — the discriminator would go dark in exactly the environment the
C5 gate runs in (cargo-mutants' repository-less copy), which is where a mutation of
`resolve_version` most needs catching. §2(B)'s fix keeps the assertion live in every
environment for ~2 changed lines in `resolve_version` (the `fallback.to_string()` arm folded
into the existing `normalize_describe` call).

**`option_env!("WYRD_VERSION")` with no build script** (the brief's alternative): every build
today would advertise `unknown`, because no tag exists — the feature would be inert for the tool
that motivates it.

**Duplicate `normalize_describe` in the build script and pin the copies with a test**: the
repo's precedent (`xtask/tests/fdb_image.rs`) pins a *constant* across files, which a substring
assertion genuinely can do; it cannot pin two copies of an algorithm. Cost of the chosen
alternative is 2 lines (`#[path]` + `pub use`, `xtask/src/dist.rs:127-130`) and it deletes 35
lines of duplication rather than adding it.

**Extract `image_build_args` as a pure function vs. a file-read + substring assertion** (the
brief left the choice open, asking which and why): extracted — `xtask/src/dist.rs:163`. It cost
`obtain_binary` **47 removed lines / 4 added** (`@@ -436,57 +503,14 @@` in `patch.diff`) and gained a real assertion about the argument
list that actually runs (`xtask/tests/dist_templates.rs:337`), instead of grepping our own
source. `version_file` came out with it (`:218`) so one `version` provably reaches both the
build arg and the `VERSION` file. The Dockerfile half stays a file-read assertion (`:376`) in
the `fdb_image.rs` idiom, because that file is not ours to make pure.

---

## 6. Gates run here

| Command | Result |
|---|---|
| `cargo test -p wyrd-server --test s3_server_version_header` | 2 passed (green with fix, red without — §4a) |
| `./engine/xtask.sh ci` (the project's whole gate: typos, docs, fmt, clippy `-D warnings`, build, test incl. DST, machete, deny, conformance) | **`xtask ci: all checks passed`**, exit 0 |
| `cargo fmt --all -- --check` | clean (the patch is formatter-output, so the target's commit hook has nothing to rewrite) |
| `cargo clippy --workspace --exclude wyrd-dst --all-targets` | exit 0, no warnings |
| `GIT_DIR=/nonexistent WYRD_VERSION= cargo test …` (repository-less simulation) | 2 passed |

### C5 (`scripts/mutants-in-diff`) — my blocker is gone; the baseline now stops on a *pre-existing* one

Re-running the gate command verbatim (`PDCA_BUNDLE=… ./scripts/mutants-in-diff`) gets **past**
`s3_server_version_header` (it no longer appears in the failure list) and stops later, in an
untouched test:

```
thread 'scan_gitlinks_is_green_over_the_real_index' panicked at
  xtask/tests/repo_hygiene_guards.rs:137:5: git ls-files -s -z must succeed
ERROR cargo test failed in an unmutated tree, so no mutants were tested
```

That guard requires a **git repository**, and cargo-mutants copies the source tree without
`.git` — so it fails for any patch, independently of this one. Reproduced on this tree with the
patch applied *and* without touching that file (the diff does not modify
`xtask/tests/repo_hygiene_guards.rs`; it only cites it in a build-script comment):

```
$ GIT_DIR=/nonexistent cargo test -p xtask --test repo_hygiene_guards
test scan_gitlinks_is_green_over_the_real_index ... FAILED     (28 passed, 1 failed)
```

Previously this was *masked*: the run aborted earlier, at my test. So the iteration-1 C5 error
had two causes stacked; this patch removes the one that was mine. Fixing the other means either
teaching that guard to skip cleanly without a repository (it is a test-fidelity bug in an
unrelated file — out of this slice's scope; the rubric says decline-with-issue-reference rather
than an in-PR fix) or setting `copy_vcs` in `.cargo/mutants.toml`. **Recommend a follow-up
issue**; C5 is advisory by policy for exactly this "pre-existing suite debt adjacent to the
diff" reason (`pdca.toml`, C5 row).

Because that blocks the gate's own mutation evidence, §4's three hand-run mutations of the
production code are the causal-adequacy evidence the iteration-1 sign-off asked for — and §8
adds the tool's own verdict from a run with that one guard skipped: **baseline ok, 0 missed**.

---

## 7. Honest limits (for §6/§9 at sign-off)

1. **The `VERSION`-equality claim is WRITTEN and REVIEWABLE but UNOBSERVED.** The check now
   exists as executable shell inside the release smoke step
   (`.github/workflows/release.yml:79-99`): start the installed binary as an `s3` role, curl it
   unsigned (refused 403, header still present), compare the `Server:` remainder with the
   `version:` line of the untarred `VERSION`. It runs on a `v*` tag push, and **no `v*` tag has
   ever been cut** in this repo (the only tag is `archive/backup-premerge-signoff`;
   `.github/workflows/release.yml:20-23` triggers on `push: tags: ["v*"]`). So at this sign-off
   the equality is *not demonstrated* — I did not run it, and I did not attempt an image build
   (the brief forbids it and it needs Docker + network). What IS demonstrated at Check is the
   coupling that feeds it: the build-arg list carries the same string `VERSION` records
   (`xtask/tests/dist_templates.rs:337`), and the Dockerfile declares and exports the ARG
   (`:376`).
2. **The advertised value on an untagged checkout is `0.0.0+git.<sha>`** — a real build identity,
   not a release version. The release-shaped value (`0.1.0`, `0.1.0+git.3.<sha>`) only appears
   once a tag exists. This is the brief's own "production reach" caveat, and it is what the
   binding test observes today (`wyrd/0.0.0+git.a801997`).
3. **A dirty developer tree advertises its commit without a dirty marker** (§2(A), §3.2). Every
   artifact `cargo xtask dist` produces still carries `.dirty` when the packaging checkout was
   dirty, on both the image and `--host` paths.
4. **The `--host` and image paths of `dist` were not executed** (both need a toolchain/Docker
   this Check does not have). Their pure decisions are unit-tested; the shell that drives them is
   read-only reviewed.
5. No external dependency was missing: the build script needs only `git`, which the checkout and
   the packaging pipeline already require, and it falls back cleanly when git is absent (§2(B)).

---

## 8. Mutation cross-check (ran; result below)

With the one repo-dependent guard skipped, the gate's own tool runs clean on this bundle's
diff:

```
$ cargo mutants --in-diff results/issue_736/patch.diff --no-shuffle \
    -- --workspace --exclude wyrd-dst -- --skip scan_gitlinks_is_green_over_the_real_index
Found 30 mutants to test
ok       Unmutated baseline in 26s build + 48s test
30 mutants tested in 2m: 4 caught, 26 unviable          (exit 0 — 0 MISSED)
```

* **The unmutated baseline is green** — the iteration-1 blocker (`ERROR cargo test failed in an
  unmutated tree`) is gone; what remains between this and a green `scripts/mutants-in-diff` is
  only the pre-existing repo-dependent guard in §6.
* **Caught (4):** `server_header_value`'s `&&` → `||` and its `delete !`
  (`crates/gateway-s3/src/lib.rs:124`), `is_tchar`'s `||` → `&&` (`:143`), and
  `resolve_version`'s `delete !` (`crates/server/src/version.rs:115`).
* **Missed: 0.**
* **Unviable (26)** is an artifact of this workspace's `warnings = "deny"`
  (root `Cargo.toml:229-234`), not a test gap: cargo-mutants replaces a function body, the
  parameters go unused, and the mutant fails to compile — e.g.
  `crates/server/src/version.rs:115 → String::new()` logs
  `error: unused variable: 'explicit' … -D unused-variables implied by -D warnings`. Those
  mutants are evidence neither way, which is why §4's three hand-run mutations (which *do*
  compile, because they substitute a real value rather than delete the body) are the causal
  evidence to read.

## 9. Scratch

All throwaway work lived under `$PDCA_SCRATCH` as `pdca-builder-736-*` (the cargo-behaviour
probe crate, the red-leg backup of the touched files, the gate and mutants logs) and was
removed before handover; every measurement quoted above is reproduced inline rather than by
reference to a deleted file. Nothing was written outside `$PDCA_WORKTREE` and the bundle
directory.
