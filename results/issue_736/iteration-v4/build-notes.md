# Build notes — issue 736 / s3-server-version-header (iteration 4)

Target branch: `getwyrd/wyrd @ main`, built in `$PDCA_WORKTREE`
(`/home/eddie/wyrd/wyrd.pdca-wt-l1`, base `a801997`, which contains the brief's `65ca4fd`).
Every `path:line` below is against that patched worktree unless it says "base".

This is a **rebuild on iteration 3's patch**, not a fresh design: the shape the brief asks for
(one stamp point, a config field defaulted in `S3Config::new`, a build script, one shared
derivation, the packaging plumbing, the release-smoke equality) survived review and is kept.
What changed is every point the sign-off and the T4 batch named. §2 maps them one by one.

---

## 1. What the patch does (scope (a)–(e))

**(a) One stamp point, every response.** `crates/gateway-s3/src/lib.rs:1660` inserts
`Server: wyrd/<version>` immediately after the `x-amz-request-id` stamp (`:1655-1657`), at the
end of `handle` — the one place every request reaches, because the router is a bare
`Router::new().fallback(handle::<G>)` (`:205` on base, `:280` patched). The value is built and
validated **once** at composition time (`server_header_value`, `:142`, called from
`S3Gateway::router`, `:287`) and carried in `AppState.server` (`:216`), so the per-request path
is an infallible `insert` of a cheap `HeaderValue` clone — an invariant stated over *every*
response cannot depend on a per-response fallible conversion.
`S3Config` gains `server_version` (`:182`), defaulted by `S3Config::new` to
`UNKNOWN_SERVER_VERSION` (`:196`), so every existing caller keeps compiling and a library
caller still emits a well-formed header.

**(b) The composition root supplies the real value.** `crates/server/build.rs` bakes the build
identity; `crates/server/src/lib.rs:36` exposes it as `BUILD_VERSION`; `cli::serve_s3` sets it
(`crates/server/src/cli.rs:2392`). ADR-0010: the root knows the build identity, the wire crate
does not.

**(c) The startup log names the same build.** `crates/server/src/cli.rs:2208` adds `version`
to the `role started` event, beside `version_origin` (`:2213`) — see §3, item 7 for the
scope decision on that second field.

**(d) One derivation, single-sourced.** `crates/server/src/version.rs` holds the pure
decision (`resolve_version`, `:242`; `normalize_describe`, `:149`). `crates/server/build.rs`
compiles it with `#[path = "src/version.rs"] mod version;` (`:26-27`) and `xtask/src/dist.rs`
with `#[path = "../../crates/server/src/version.rs"] pub mod version;` (`:127-128`),
re-exporting `normalize_describe` (`:130`) so `dist::normalize_describe` keeps its existing
public name and its existing test. Tooling reaches into the product, never the reverse.
The packaging path passes the version in (`image_build_args`, `xtask/src/dist.rs:163`;
`--host` build env, `:516`; `ARG`/`ENV WYRD_VERSION` in `deploy/docker/wyrd/Dockerfile`),
because `.dockerignore:6` excludes `.git/` and the default `dist` path compiles the binary
*inside* the image build.

**(e) The release smoke checks the equality.** `.github/workflows/release.yml` starts the
installed binary as an `s3` role inside the existing bookworm container, reads the `Server:`
header off an unsigned (403) request, and compares its `wyrd/` remainder with the `version:`
line of the untarred `VERSION`. Unobserved until a `v*` tag exists — see §5.

---

## 2. Iteration 3's carry-forward, item by item

**1. `git describe` was not scoped to the source tree (most severe).**
`crates/server/build.rs:49-53` now gates *both* git answers on
`git ls-files --error-unmatch Cargo.toml`, run in `CARGO_MANIFEST_DIR`; when it fails, the
describe and the sha are dropped (`:64-80`) and the derivation falls to `0.0.0+git.unknown` /
origin `unknown`. Demonstrated concretely (scratch repo, since removed):

```
$ cd <outer-repo>/vendor/wyrd/crates/server      # untracked in the outer repo
$ git rev-parse --short HEAD            => 0329068          # a FOREIGN commit
$ git describe --tags --always --dirty  => 0329068          # would have been baked
$ git ls-files --error-unmatch Cargo.toml
error: pathspec 'Cargo.toml' did not match any file(s) known to git   # guard trips
```

`emit_rerun_directives` is also gated on it (`:142-150`): a foreign repository's refs are not
this build's inputs and must not be watched either.

**2. Leg 4's skip predicate was self-minted from the artifact.**
The test now carries an **independent oracle** and makes `unknown` a hard failure:
`crates/server/tests/s3_server_version_header.rs:430-507`. It asks git itself — `rev-parse
--short HEAD` *and* `ls-files --error-unmatch Cargo.toml`, from `CARGO_MANIFEST_DIR`, the same
directory the build script asks from — and if a repository that tracks this source answers,
`version_origin` **must** be `git` (`:489-496`). `explicit` still skips (iteration 2's
requirement: a handed-in version is a supported distribution input and carries no sha).
Refuted, not asserted: with `tracked_here` forced to `false` in `build.rs` (a one-token slip,
exactly the modelled defect) the test FAILS:

```
assertion `left == right` failed: `git rev-parse --short HEAD` answers a801997 for a
repository that tracks this crate, yet the running build reports `version_origin: unknown`
and advertises `wyrd/0.0.0+git.unknown` — the version derivation did not run where it
plainly could
  left: "unknown"   right: "git"
```

**3. reftable ref backend / incomplete watch set.** `crates/server/build.rs:154-193` watches
both ref layouts: the `files` backend's `packed-refs`, `refs/heads`, `refs/tags`, and the
`reftable` backend's `reftable/` directory — the latter twice over, because measurement in
this worktree shows `--git-path` does *not* map an unknown name to the common dir:

```
rev-parse --git-path reftable    => /home/eddie/wyrd/wyrd/.git/worktrees/wyrd.pdca-wt-l1/reftable
rev-parse --git-common-dir       => /home/eddie/wyrd/wyrd/.git
```

so `<common-dir>/reftable` is resolved explicitly (`:191-193`) — otherwise a linked worktree
on reftable would watch only its own per-worktree stack while `git describe` reads the tags in
the common one. Emissions are de-duplicated (`:168-174`) and a non-existent path is skipped
(cargo treats a missing `rerun-if-changed` target as always-changed).

**4. `--dirty` was silently reversed against the brief.** Honored, not re-argued:
`crates/server/build.rs:64-67` derives with `git describe --tags --always --dirty`, so this
build advertises `wyrd/0.0.0+git.a801997.dirty` (test output below). `index` joins the watch
set (`:163`) as the closest thing cargo can invalidate on for the working tree.
Residual, stated in the code (`:129-141`): an *unstaged* edit git has not yet noticed can
leave `.dirty` unset until the next index write. That is strictly better than iteration 3's
"no `--dirty` at all", which produced the clean spelling **always**; and it never names a
commit the build did not come from.
Cost of the alternative (watch the whole tree): `git ls-files` is **463 files** today, so the
honest form is 463 `rerun-if-changed` lines covering `docs/**` and `deploy/**` as well — every
prose commit then re-runs the script. Measured price of one such re-run in this worktree:
`touch $(git rev-parse --git-path index) && cargo build -p wyrd-server --bin wyrd` = **11 s**
(warm debug cache) for the bin alone; `cargo test -p wyrd-server` additionally re-links the
**41** integration-test binaries under `crates/server/tests/`. Watching `index` pays that
price only when git actually writes the index; watching 463 files pays it for every edit to
any tracked file in the workspace.

**5. The streaming-GET head was never asserted on the wire.**
`crates/server/tests/s3_server_version_header.rs:338-357` adds a **signed GET** leg between
the signed PUT and the unsigned 403, and asserts its `Server` value equals the other two. Its
head is built and returned before the body streams (`gateway-s3`'s `Body::from_stream`
wrapper), which is why the invariant text names it separately.

**6. Non-injective normalization + whitespace trimming (also the five T4 findings).**
* The rewrite-forbidden-bytes-to-`.` pass is **gone**. `resolve_version` now *refuses* a
  describe that does not normalize to an advertisable identity and names the **commit**
  instead (`crates/server/src/version.rs:242-294`, decision documented at `:204-241`).
  Regression: `tag_normalization_never_collapses_two_builds_onto_one_identity` (`:406-421`)
  drives the exact collision the finding named — `v1/2` and `v1.2` — and asserts they cannot
  land on one identity. `a_tag_that_is_not_an_identity_is_refused_and_the_commit_named_instead`
  (`:344-403`) pins the whole mapping, dirt included.
* `normalize_describe` (`:149-176`) is back to the base's exact shape rules, so
  `xtask/tests/dist_templates.rs:300-321` passes **unchanged** (verified: that test is green
  and untouched).
* Trimming is removed where it was a silent repair and kept where it is parsing, with the
  difference documented (`:225-241`): `explicit` is verbatim-or-rejected — `" 1.2.3 "` now
  fails the build (`an_override_padded_with_whitespace_is_rejected_not_trimmed`, `:482-497`);
  `describe`/`head_sha` are a subprocess's stdout and are trimmed.
* Gateway side: `server_header_value` (`crates/gateway-s3/src/lib.rs:142`) no longer trims;
  `" 0.1.0 "` degrades to `wyrd/unknown` **and logs why** rather than being quietly rewritten
  (`a_malformed_identity_degrades_instead_of_dropping_the_header`, `:5563`).
* `dist::derive_version` (`xtask/src/dist.rs:426-446`) now calls the shared `resolve_version`
  rather than `normalize_describe` directly — otherwise the refusal would exist only on the
  binary side and a `release/1.2.0`-style tag would put one string in `VERSION` and another on
  the wire, i.e. re-open the drift this module exists to close.

**7. Scope of `version_origin` (C3) — decision: KEEP, recorded here.**
It is one `&'static str` constant (`crates/server/src/lib.rs:46`) and one log field
(`cli.rs:2213`). It stays because iteration 2's carry-forward *required* an override-origin
signal ("so the SHA leg skips only intentional overrides"), and because rung 1 is used
verbatim: `wyrd/1.2.3` alone cannot distinguish a version this checkout derived from one
somebody typed into a build-arg — which is the same "an operator-supplied parameter that can
silently be wrong" the header exists to replace. Note the fix for item 2 means the test no
longer *trusts* the field: git is the oracle, `version_origin` is the claim being checked.
Trimming it would cost the `explicit` skip its only signal and leave leg 4 either vacuous on
dist builds or false-failing on them.

---

## 3. Forced self-refutation (the three questions)

**(a) Genuine red?** Yes — actually reverted and re-run. Production changes stashed
(`crates/gateway-s3/src/lib.rs`, `crates/server/src/{cli,lib}.rs`, `xtask/**`, Dockerfile,
release.yml, docs) and the two new production files moved out of the tree, keeping only the
test:

```
$ cargo test -p wyrd-server --test s3_server_version_header
   Compiling wyrd-gateway-s3 ... Compiling wyrd-server ...        # it COMPILES on the revert
thread '...' panicked at crates/server/tests/s3_server_version_header.rs:279:9:
the signed PUT response carries no `Server` header — nothing on the wire says what build a
client is talking to. Head:
HTTP/1.1 200 OK / content-length: 0 / etag: "..." / x-amz-request-id: ... / connection: close
test result: FAILED. 0 passed; 1 failed
```

It compiles on the reverted tree because it names **no** symbol this patch introduces (only
`wyrd_gateway_s3::sigv4`, std, serde_json, tempfile and `env!("CARGO_BIN_EXE_wyrd")`) — which
is what keeps `C4-verify` scoring a proven red rather than `UNVERIFIABLE`.

**(b) Production path?** Yes. The test spawns the **built `wyrd` binary** as an `s3` role
(`env!("CARGO_BIN_EXE_wyrd")`, `s3_server_version_header.rs:64`), parses the ephemeral port
out of the child's own startup line, and drives real HTTP/1.1 over TCP. Nothing in-process is
composed; the only way to satisfy leg 2 is through `cli::serve_s3` + the build script.

**(c) Fixture includes the fault?** Yes.
* The **error plane** is in the fixture, not curated out: the unsigned 403 leg is asserted to
  carry the same value as the success leg (`:359-373`) — that is the leg a per-handler
  implementation misses.
* The **streaming-GET head** is in it too (item 5 above).
* Leg 4's oracle is **git itself**, not the artifact: the "failing element" it exists to catch
  (a derivation that silently did not run) was injected and observed failing (§2 item 2).
* Post-fix, green with the real derived value:
  `advertised build identity: wyrd/0.0.0+git.a801997.dirty (HEAD a801997)`.

---

## 4. Alternatives considered (and their measured cost)

* **Rewrite an unrepresentable tag into the identity grammar** (iteration 3's `sanitize_identity`,
  4 lines). Rejected on correctness, not cost: not injective — `v1/2` and `v1.2` both become
  `1.2`. The replacement is *smaller* (the function is deleted; the decision moves into the
  branch `resolve_version` already needed for the no-describe case).
* **Hard-error the build on such a tag.** Rejected: `release/1.2.0` is a legitimate convention,
  and a tag nobody controls must not be able to break `cargo build`.
* **Drop `--dirty`** (iteration 3). Rejected: it reverses a brief decision without ratification,
  and it is strictly less informative than the stale-tolerant `--dirty` (see §2 item 4 for the
  463-file / 11 s numbers behind the watch-set choice).
* **Watch every tracked file so `.dirty` is exact.** Rejected on measured cost: 463 files
  today; 11 s to relink the bin after *any* of them changes, plus 41 test binaries under
  `cargo test -p wyrd-server`. Cheaper and honest: watch `index`, document the residual.
* **A file-read + substring assertion for the build-arg coupling** (the `:279-295` fallback the
  brief allows). Not taken: `image_build_args` was extracted as a pure function
  (`xtask/src/dist.rs:163`) and `xtask/tests/dist_templates.rs` asserts on its **output**, which
  is an assertion about what runs rather than a grep of our own source. The Dockerfile half
  *does* use the file-read idiom, because a `Dockerfile` has no other reviewable surface.

---

## 5. Honest limits — for §6 / sign-off

1. **The `VERSION`-equality half is written but UNOBSERVED.** `cargo xtask dist` needs Docker
   and a network and is deliberately outside `ci` (`xtask/src/dist.rs:26-28`). The check now
   exists as executable shell in `.github/workflows/release.yml`, but that workflow triggers on
   `push: tags: ["v*"]` (`:20-22`) and **no `v*` tag has ever been cut** (the repo's only tag is
   `archive/backup-premerge-signoff`). So at this sign-off the equality is reviewable and
   unrun. I did not attempt an image build (the brief forbids it).
2. **This checkout advertises `0.0.0+git.<sha>[.dirty]`**, a real build identity but not a
   release version; the release-shaped value appears only once a tag exists. Recorded because
   the brief asks for it under Production reach.
3. **`C4-ci` will be RED on the base I was handed, for a reason outside this patch.** The
   worktree base `a801997` still carries `h2 0.4.15`, so `cargo deny check` fails
   (`error[vulnerability]: h2 unbounded empty DATA frames / RUSTSEC-2026-0258`) — the brief's
   `Depends on: 773` edge, whose one-line lockfile bump is not in this tree. Everything else in
   `cargo xtask ci` passes with this patch applied (see §6). Nothing in this patch touches it.
4. **`C5-mutants` will report an unmutated-baseline ERROR again, and it is environmental.**
   `cargo mutants` copies the source tree **without** `.git` (`--copy-vcs` defaults to false —
   `cargo-mutants mutants --help`), and this patch touches `xtask/`, which pulls `xtask`'s own
   test binaries into the mutants baseline. One of them,
   `xtask/tests/repo_hygiene_guards.rs:137` (`scan_gitlinks_is_green_over_the_real_index`),
   asserts `git ls-files -s -z must succeed` and therefore fails in any VCS-less copy — which
   is what iteration 3's `gate-logs/C5-mutants.log:1639` recorded. It is a pre-existing
   interaction between that guard and the mutants runner, not a defect this patch introduces;
   fixing it means either `--copy-vcs=true` in the harness's `scripts/mutants-in-diff` (PDCA
   side, not the target) or weakening a target guard (out of scope, and the rubric says an
   out-of-scope finding gets a decline-with-issue-reference, not an in-PR fix).
5. **`.dirty` freshness residual** (§2 item 4) — documented in `crates/server/build.rs:129-141`
   rather than hidden.

No NEEDS-HUMAN external dependency was hit: git is present, and nothing here needed Docker.

---

## 6. What I ran (project runner)

* `./engine/xtask.sh ci` (the gating `C4-ci` command, via the wrapper): typos ✓, docs lint +
  render ✓, gitlink guard ✓, unsafe-forbid guard ✓, `cargo fmt --check` ✓, `cargo clippy
  --workspace --all-targets` ✓ (0 warnings; `warnings = "deny"` workspace-wide), `cargo build
  --all-targets` ✓, `cargo test --workspace` ✓ (**0** `test result: FAILED` lines), `cargo
  machete` ✓ → stops at `cargo deny check` (limit 3 above).
* The steps `deny` aborted, run separately: `./engine/xtask.sh conformance` ✓ (5 valid + 6
  invalid vectors), `./engine/xtask.sh statics` ✓ (ADR-0035), `./engine/xtask.sh dst` ✓
  (50-seed sweep, exit 0).
* Red→green on the named test: `cargo test -p wyrd-server --test s3_server_version_header`
  (the brief's own falsifiability command) — red on the reverted tree, green on the patched
  one, both bounded by the tool's timeout.
* Docs gates re-run after the last `08-crosscutting-concepts.md` edit: `lint_docs: OK`,
  `render_site: link audit OK`, `typos` exit 0.
* Formatter run over every touched file (`cargo fmt --all`) — the patch is commit-ready for the
  target's hooks.

Scratch: everything throwaway lived under `$PDCA_SCRATCH/pdca-builder-736-*` and was removed.
No PR was pushed, opened, readied or merged.

---

## 7. The brief's open questions, answered

1. **Where the `wyrd/` prefix lives** — in `gateway-s3`, and exactly one production site spells
   it: `SERVER_PRODUCT` (`crates/gateway-s3/src/lib.rs:102`), interpolated once in
   `server_header_value` (`:156`). The composition root passes the bare version; the wire crate
   owns the whole header shape. The other `wyrd/` literals in the tree are test oracles and the
   release-smoke shell, which must spell the expectation independently or they would assert
   nothing.
2. **Other roles' startup logs** — untouched, per the brief (`cmd_d_server` / `cmd_custodian`
   keep their own `role started` events). Worth a follow-up issue; not scope creep here.
3. **Does `release.yml` need a checkout change?** Confirmed **no**, as the brief predicted:
   `.github/workflows/release.yml:34-39` already does `fetch-depth: 0` + `fetch-tags: true`
   ("Full history + tags so `cargo xtask dist`'s `git describe` versioning works"). Nothing was
   changed there beyond the smoke step scope (e) adds.
