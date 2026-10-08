# Build notes — issue 736 / s3-server-version-header (iteration 3)

Target branch: `getwyrd/wyrd @ main` (worktree `/home/eddie/wyrd/wyrd.pdca-wt-l1`, base
`a801997`). Citations are **post-patch** line numbers in that worktree unless the text says
"pre-patch".

This iteration keeps iteration 2's architecture (it was not rejected) and fixes the four
implementation findings the round-2 sign-off routed back, each by removing the cause. §2 is
the carry-forward answer; read that first.

---

## 1. What the patch does

| File | Change |
|---|---|
| `crates/gateway-s3/src/lib.rs:102,109,120,135,155` | `SERVER_PRODUCT` / `UNKNOWN_SERVER_VERSION` consts, **`pub fn is_advertisable_version`** (the wire's half of the identity contract), `server_header_value`, `is_tchar`. |
| `crates/gateway-s3/src/lib.rs:176,185` | `S3Config.server_version`, defaulted in `S3Config::new` (the only constructor — source-compatible for every existing caller). |
| `crates/gateway-s3/src/lib.rs:210,281,1561,1654` | The header is built **once** in `router()`, carried in `AppState`, stamped in `handle` beside the request id — the one point every request reaches (`Router::new().fallback(handle::<G>)`, `:289`). |
| `crates/server/src/version.rs` (new) | The single-sourced pure derivation: `VERSION_ENV:47`, `VERSION_ORIGIN_ENV:58`, `FALLBACK_VERSION:65`, `VersionOrigin:69`, `BuildIdentity:93`, `is_identity_byte:110`, `sanitize_identity:130`, `normalize_describe:150` (shape rules moved verbatim from `xtask/src/dist.rs`), `resolve_version:222`. |
| `crates/server/build.rs` (new) | The impure half: reads `WYRD_VERSION`, asks git, emits two `cargo:rustc-env`s + a complete rerun-watch set (`:98`). A malformed override **fails the build** (`:54`). |
| `crates/server/src/lib.rs:36,46` | `BUILD_VERSION` and `BUILD_VERSION_ORIGIN`; `pub mod version;` (`:22`). |
| `crates/server/src/lib.rs:813,838` | Two unit tests pinning the derivation against the gateway's grammar — the only place both crates are visible. |
| `crates/server/src/cli.rs:2208,2213,2392` | The `role started` event records `version` and `version_origin`; `serve_s3` sets `config.server_version` from the baked constant. |
| `xtask/src/dist.rs:128,130` | Compiles the product's `version.rs` by `#[path]` and re-exports `normalize_describe`, so `dist::normalize_describe` keeps its call site (`:422`) and its test (`xtask/tests/dist_templates.rs:301`) unchanged. |
| `xtask/src/dist.rs:163,218,493,509` | `image_build_args` / `version_file` extracted pure; the image build passes `--build-arg WYRD_VERSION`; the `--host` build passes the same value in its environment. |
| `deploy/docker/wyrd/Dockerfile:39,47,76` | `ARG WYRD_VERSION=""` (unset-safe), re-declared in the build stage, exported as `ENV` for the `cargo build`. |
| `xtask/tests/dist_templates.rs:337,376` | Two container-free coupling assertions (build-arg list ⇄ `VERSION` string; Dockerfile ARG/ENV declaration). |
| `docs/design/architecture/08-crosscutting-concepts.md:85` | Living-architecture §8.7 records the wire identity, the log field and the `VERSION` equality. |
| `.github/workflows/release.yml:79-117` | The end-to-end `Server:` ⇄ `VERSION` comparison inside the existing installer smoke step, with a failure path that reports itself. |
| `crates/server/tests/s3_server_version_header.rs` (new) | The binding test: all four legs against **one** built-binary child. |

---

## 2. Carry-forward from iteration 2 — the four findings, each fixed at the cause

### (A) T4 `[TEST-GAP]` + T5 — "the SHA leg fails for a supported `WYRD_VERSION` override"

> `crates/server/tests/s3_server_version_header.rs:435` — *"This assertion knowingly fails for
> the supported `WYRD_VERSION` override whenever it differs from HEAD"*; sign-off: *"add an
> override-origin signal or equivalent so the SHA leg skips only intentional overrides"*.

Correct, and iteration 2 knew it and asserted anyway, because it could not tell the two apart:
cargo re-exports the build script's own `cargo:rustc-env=WYRD_VERSION` into the test process,
so "the operator set it" and "the script emitted it" look identical from inside the test.

The fix is to stop guessing and **make the build say which rung answered**:

* `resolve_version` now returns `BuildIdentity { version, origin }`
  (`crates/server/src/version.rs:222`) — one function, one decision, so the origin cannot
  disagree with the value;
* `build.rs:65` emits `cargo:rustc-env=WYRD_VERSION_ORIGIN=explicit|git|unknown`;
* `crates/server/src/lib.rs:46` exposes it as `BUILD_VERSION_ORIGIN` and `cli.rs:2213` records
  it on the `role started` event beside `version`;
* leg 4 reads it **off the child's own log** (`tests/s3_server_version_header.rs:424`) and runs
  only when the child says `git`.

It is not a test hook: the origin is the answer to a question an operator has in production.
A verbatim override can say anything — that is exactly the issue's complaint about an
operator-supplied `--server-version` that "can silently be wrong" — so `wyrd/1.2.3` alone
cannot tell a derived identity from a typo'd build-arg, and now the log can.

Measured, on the real code:

```
$ WYRD_VERSION=1.2.3 cargo test -p wyrd-server --test s3_server_version_header -- --nocapture
leg 4 skipped: this build did not derive its identity from a checkout
               (version_origin: explicit); `wyrd/1.2.3` is used verbatim
test every_s3_response_advertises_the_baked_build_version ... ok
```

The suite passes for an override build, and leg 2's `advertised == baked` equality still binds
in that run (1.2.3 on both sides). **A missing `version_origin` is a FAILURE, not a skip**
(`:428`) — otherwise deleting the field would quietly make leg 4 vacuous; §4(a)'s mutation
table shows that firing.

### (B) Adversary `[impl]` — "a reachable tag containing `/` makes every response advertise `wyrd/unknown`"

Correct, and reproduced first-hand rather than reasoned about (scratch repo under
`$PDCA_SCRATCH`, `pdca-builder-736-slashtag`):

```
$ git tag release/1.2.0 && git commit --allow-empty -m two && git describe --tags --always
release/1.2.0-1-g72b7839
```

`/` is not an RFC 9110 `tchar`, so iteration 2 derived `0.0.0+git.release/1.2.0-1-g72b7839`,
the front door degraded it to `wyrd/unknown`, and the log and the tarball's `VERSION` carried
the real string — two spellings for one build, which is the drift this slice exists to prevent.
It also turned `cargo test` red for every developer on such a checkout.

Fixed **at the derivation**, not at the wire, so all three surfaces stay byte-identical:

* `sanitize_identity` (`crates/server/src/version.rs:130`) maps every byte outside
  `[A-Za-z0-9.+_-]` to `.`, and `normalize_describe:150` runs its result through it — one
  place, compiled by the build script *and* by `cargo xtask dist`, so `VERSION`, the log and
  the wire cannot disagree. `archive/backup-premerge-signoff-1-g2ece2f1` →
  `0.0.0+git.archive.backup-premerge-signoff-1-g2ece2f1`.
* The charset is deliberately **narrower** than `tchar`: the same string is also a docker tag
  (`image_tag_version`, `[A-Za-z0-9_.-]` after `+`→`-`) and part of a tarball filename, so a
  `tchar` like `` ` `` would be a legal header and an illegal tag.
* A *derived* value is repaired rather than rejected, deliberately — asymmetrically with the
  override below. The input is the checkout's own git state, which nobody chose for this
  build: refusing to compile because someone once pushed a `release/1.2.0` tag would fail
  every developer's build on that repository, and the identity is a *name*, not a decision
  anything depends on. The mapping is deterministic, single-sourced and unit-tested, so the
  three surfaces still cannot drift; what would be intolerable — and is what iteration 2 did —
  is repairing it at only ONE of them.
* The one input the crate does not compose itself — an explicit `WYRD_VERSION` — is
  **rejected, never repaired** (`version.rs:228`): sanitizing it would ship an identity the
  operator did not ask for and break the `VERSION` equality the release smoke checks, while
  accepting it would put a non-token on the wire. The build fails with the value in the
  message:

  ```
  $ WYRD_VERSION="1.2.3 (build 7)" cargo build -p wyrd-server --bin wyrd
  panicked at crates/server/build.rs:54:25:
  WYRD_VERSION="1.2.3 (build 7)" is not a usable build identity: … it may carry only ASCII
  letters, digits and `.`, `-`, `_`, `+`. Fix the value or unset it to derive one from git.
  ```

* The two contracts are now **pinned to each other**: `is_advertisable_version` is public on
  the gateway (`crates/gateway-s3/src/lib.rs:120`) and `crates/server/src/lib.rs:813,838`
  asserts (i) the identity **this build actually baked** is advertisable verbatim and (ii) so
  is every identity derivable from a corpus that includes the slash-tag shapes. That test
  lives in `lib.rs` and not in `version.rs` because `version.rs` is compiled by `build.rs` and
  by `xtask`, neither of which can see the gateway crate; it is not a new `tests/*.rs` file
  because an added test file that names net-new symbols would fail to compile on
  `run-verify.sh`'s RED leg and score `UNVERIFIABLE`.

### (C) Adversary `[impl]` — "the release smoke's own failure path cannot report itself"

The mechanism named in the finding does not bite as stated (the assignment reads a *pipeline*,
whose status is `sed`'s, and `sh -eu` has no `pipefail`) — but the substance is right: the
block had exactly one diagnostic path and several ways to reach the end without it, in a leg
that runs only on a `v*` tag inside `docker run --rm`, where the role's log dies with the
container. Rewritten (`.github/workflows/release.yml:85-117`) so every step captures its own
status and dumps `/tmp/s3.log` before exiting: an explicit `answered` flag for the ten-second
startup retry, `curl … || headers_rc=$?` for the header read, and the header dump as well as
the log on a mismatch. Verified by parsing the workflow YAML and running the reconstructed
inner script through `sh -n` (rc 0) — the escaping unwinds correctly and the shell is valid.

### (D) T4 `[CONVENTION]` ×2 — docs currency

Both rounds reported that changing every S3 response's contract needs a living-architecture
update (`AGENTS.md:154-157`). Iteration 2 argued the rule's enumeration ("a port, an API
operation, an RPC, a CLI flag, or a persisted field") does not name a response header. Even if
that reading is right, arguing it costs a round every time; and there *is* a living section
where this belongs. Added to **§8.7 Compatibility and version skew**
(`docs/design/architecture/08-crosscutting-concepts.md:85`): the header on every response, the
`role started` fields, the `VERSION` equality, the three origins, and the explicit note that it
is an identity rather than a capability negotiation. One paragraph, in the section whose
subject ("a half-upgraded fleet is the normal state") is exactly why the identity exists.

---

## 3. Deviations from the brief, stated plainly

1. **Leg 4's precondition.** The brief gates it on "`WYRD_VERSION` is unset in the test's
   environment". That predicate is unobservable (§2A), so it is replaced by the child's
   reported `version_origin == git`, which is the same intent and strictly stronger: it
   observes the *binary's* provenance rather than the test process's environment.
2. **The `role started` event gains a second field** (`version_origin`) beside the `version`
   scope (c) asks for. Justified in §2A; it is the mechanism leg 4's skip needs and it is
   operator-visible in its own right.
3. **Rung 3's output shape** (unchanged from iteration 2): a repository-less build yields
   `0.0.0+git.unknown`, not the bare `0.0.0`, so the criterion's "not the bare placeholder"
   assertion binds in *every* environment including cargo-mutants' repository-less copy.
   `FALLBACK_VERSION` is still `"0.0.0"` and `dist`'s call site is unchanged in meaning.
4. **`--dirty` is not asked by the build script** (unchanged from iteration 2, and routed to
   sign-off as `[human]` — "ratify or restore"). A build script's answer is cached, so it can
   only stay true for inputs cargo can invalidate on, and the working tree is not one; the
   iteration-1 review's three staleness findings were this class. Every artifact `cargo xtask
   dist` produces still carries `.dirty`, because `dist` derives with `--dirty` at packaging
   time and passes the string in on both paths (`xtask/src/dist.rs:493`,
   `deploy/docker/wyrd/Dockerfile:76`). Only a plain developer `cargo build` derives its own,
   and it names the **commit** — a value that cannot be false — rather than a dirty flag that
   could be.
5. **Open question 1** — answered as the brief assumed: the wire crate owns the `wyrd/` prefix
   (`SERVER_PRODUCT`), the composition root passes a bare version. Exactly one place spells
   `wyrd/`.
6. **Open question 3** — confirmed, no change needed: `.github/workflows/release.yml:36-39`
   already checks out with `fetch-depth: 0` + `fetch-tags: true`.
7. **Open question 2** (other roles' startup logs) — deliberately not done; out of scope, worth
   a follow-up issue as the brief says.

---

## 4. Forced refutation — the three questions, with evidence

**(a) Genuine red?** Yes. The RED leg was reconstructed exactly as
`engine/scripts/run-verify.sh:499-517` builds it (production reverted with `git checkout`, the
two added non-test files moved out, only the added test kept). The test **compiles** and fails:

```
thread 'every_s3_response_advertises_the_baked_build_version' panicked at
  crates/server/tests/s3_server_version_header.rs:274:9:
the signed PUT response carries no `Server` header — nothing on the wire says what build a
client is talking to. Head:
HTTP/1.1 200 OK
…
x-amz-request-id: f8bacda98ea56ba50000000000000000
test result: FAILED. 0 passed; 1 failed
```

Compiling on the reverted tree matters as much as failing: a test naming a net-new symbol
would score `UNVERIFIABLE` (exit 77) instead of RED. Green again after `git apply patch.diff`
(`test result: ok. 1 passed`), which also proves the bundle patch applies to the base.

Six targeted mutations of the **production** code, each rebuilt and re-run:

| Mutation (production) | Result |
|---|---|
| delete `config.server_version = crate::BUILD_VERSION.to_string();` (`cli.rs:2392`) | **FAILS** — `left: "unknown", right: "unknown"` (the "header shipped inert" hole) |
| `config.server_version = "1.2.3".to_string();` | **FAILS** — `left: "1.2.3", right: "0.0.0+git.a801997"` |
| delete `version = crate::BUILD_VERSION,` (`cli.rs:2208`) | **FAILS** — `the role started event carries no version field: {…,"version_origin":"git"}` |
| delete `version_origin = crate::BUILD_VERSION_ORIGIN,` (`cli.rs:2213`) | **FAILS** — `carries no version_origin field … neither an operator nor this test can tell an identity derived from the checkout from one handed in verbatim` |
| `sanitize_identity` body → `text.to_string()` | **FAILS** ×2 — `version::tests::a_tag_that_is_not_a_token…` and the cross-crate `tests::every_derivable_identity_is_advertisable` |
| `is_identity_byte` → `true` | **FAILS** ×3, incl. the cross-crate one — this is the mutation the *narrow* charset test cannot catch alone, which is why the gateway's predicate is the oracle |

(Deleting the `sanitize_identity` **call** rather than its body is unviable, not surviving: the
workspace's `warnings = "deny"` turns the now-dead function into a compile error.)

`cargo mutants --in-diff` over this bundle's `patch.diff`, with the one repository-dependent
guard skipped (§6): **46 mutants, 11 caught, 35 unviable, 0 missed**, unmutated baseline ok.

**(b) Production path?** Yes. The test spawns `env!("CARGO_BIN_EXE_wyrd")` as a real `s3` role
over a loopback listener, parses the ephemeral port out of the child's own startup line, signs
with the production `wyrd_gateway_s3::sigv4::sign`, and reads raw response headers off a
`TcpStream`. Nothing is mocked: the composition root (`cli::serve_s3`), the real router, the
real `handle`, the real build script's constant, and — new this round — the child's own
`role started` JSON as the provenance oracle. The in-process assertions in
`crates/gateway-s3` and `crates/server/src/lib.rs` are supplementary and call the same
production functions, not copies.

**(c) Fixture includes the fault?** Yes.
* The **error plane** — the leg a per-handler implementation would miss — is in the fixture,
  not curated out: the header is asserted on a **403** as well as a 200, and the two must be
  the same string.
* The **hostile tag shapes** that broke iteration 2 are in the corpus verbatim
  (`archive/backup-premerge-signoff-1-g2ece2f1`, `release/1.2.0-4-gabc12de`), and the coupling
  test also runs against the identity **this build actually baked**, so a checkout that really
  has such a tag reachable fails `cargo xtask ci` rather than shipping `wyrd/unknown`.
* Leg 4 asserts against the **real** HEAD sha of the checkout the binary was built from
  (`a801997` in this run, printed by the test), not a fabricated one. Its three skips are
  printed and are exactly the states with no sha to look for; the one state that used to be
  silently skippable (a missing origin field) is now a failure.

---

## 5. Alternatives rejected, with the costs shown

**Skip leg 4 whenever `WYRD_VERSION` is present in the test's environment** (the obvious
reading of the brief). Cost is not lines but binding strength: cargo re-exports the build
script's own `WYRD_VERSION` into the test process, so the variable is present in *every* run
that bakes a version — the leg would self-skip in 100% of normal runs and bind in none. Zero
lines saved, the whole leg lost. The origin signal costs 1 field on the event, 1
`cargo:rustc-env`, 1 const and an enum (≈30 lines including docs).

**Sanitize the explicit override too, instead of rejecting it.** ~4 lines cheaper (no error
arm, no `Result`), and silently wrong: the binary would advertise
`1.2.3..build.7.` while the tarball's `VERSION` — written by `dist` from the *unsanitized*
string it derived — said something else, breaking the one equality the release smoke exists to
check. The rubric's own rule ("never silently accepted") points the same way.

**Sanitize at the wire instead of at the derivation** (i.e. leave `normalize_describe` alone
and let the gateway repair the value). Same line count, wrong place: `dist` writes `VERSION`
from the *underived* string, so the wire would say `0.0.0+git.release.1.2.0…` and the tarball
`0.0.0+git.release/1.2.0…`. The whole point of compiling one derivation in both consumers is
that the repair must happen before either of them sees it.

**Put the cross-crate coupling test in a new `crates/server/tests/*.rs`.** Cost is a gate
regression, not lines: `run-verify.sh:141-144` keeps every ADDED `*/tests/*.rs` on the RED leg
and reverts production, so a second added test file naming `wyrd_server::version::…` would
fail to **compile** there, and `run-verify.sh:201-215` scores "non-zero exit, 0 tests ran" as
`UNVERIFIABLE` — turning a proven red into a NEEDS-HUMAN. In `lib.rs` (a *modified* file,
reverted whole on that leg) it costs nothing.

**Keep `--dirty` and re-run the build script on every build.** Measured in iteration 2 and
unchanged: an always-stale watched path makes cargo print `Dirty wyrd-server` and `Compiling`
on *every* build with byte-identical script output — a recompile and relink of `wyrd-server`,
the `wyrd` binary and the **40** integration-test binaries under `crates/server/tests/`
(`ls crates/server/tests/*.rs | wc -l` = 40), on a crate whose `cli.rs` alone is ~3 000 lines.
Rejected for a `.dirty` suffix that `dist` supplies correctly on every shipping path anyway.

**Fix the C5 baseline by adding `copy_vcs = true` to `.cargo/mutants.toml`.** One line, and it
would make the mutation gate measurable again for every bundle that touches `xtask` — but it
is a change to the repo's *gate configuration*, outside this slice's scope, and the round-2
adversary explicitly routed it to a human as a gate-scope decision. Cost of the alternative,
so the human can decide in one read: copying `.git` into each mutant scratch tree adds **30 MB**
per copy (`du -sh /home/eddie/wyrd/wyrd/.git`). Not taken here; see §6.

**Argue the docs-currency finding instead of writing the paragraph.** Iteration 2 did; it cost
a round and the finding came back unchanged. The paragraph is 1 line of markdown in the section
that already owns version skew. Written.

---

## 6. Gates run here

| Command | Result |
|---|---|
| `cargo test -p wyrd-server --test s3_server_version_header` | 1 passed (red without the fix — §4a) |
| `WYRD_VERSION=1.2.3 cargo test -p wyrd-server --test s3_server_version_header` | 1 passed (leg 4 skips with a printed reason — the round-2 TEST-GAP) |
| `cargo test -p wyrd-server --lib` | 50 passed (incl. the two new coupling tests and the 7 derivation tests) |
| `./engine/xtask.sh ci` (the project's whole gate) | **168 test groups green**, typos / docs / fmt / clippy `-D warnings` / build / DST / machete / conformance / the #616 guards all pass — **one failure, pre-existing and unrelated:** see below |
| `cargo fmt --all -- --check` | clean (the patch is formatter output, so the target's commit hook has nothing to rewrite) |
| `cargo mutants --in-diff patch.diff --no-shuffle -- --workspace --exclude wyrd-dst -- --skip scan_gitlinks_is_green_over_the_real_index` | 46 mutants: 11 caught, 35 unviable, **0 missed**; baseline ok |
| `sh -n` over the reconstructed release-smoke script | rc 0 (YAML parsed with `yaml.safe_load`, escaping unwound) |

### `cargo xtask ci` fails on a NEW advisory in a dependency this patch does not touch

```
error[vulnerability]: h2 unbounded empty DATA frames
  Cargo.lock:111  h2 0.4.15
  ID: RUSTSEC-2026-0258 · Low severity · Patched in v0.4.16
  Solution: Upgrade to >=0.4.16 (try `cargo update -p h2`)
advisories FAILED, bans ok, licenses ok, sources ok
```

`cargo deny`'s inputs are `Cargo.lock`, `deny.toml` and the fetched advisory DB. **This patch
modifies none of them** (`git status --short Cargo.lock deny.toml` → empty; neither appears in
`patch.diff`), and `h2` reaches the tree through `hyper`/`axum`/`tonic`, so the failure is
base-tree debt from an advisory published since iteration 2's `C4-ci` ran green
(`iteration-v2/gate-logs/C4-ci.log:3425`). Remediation is a lockfile bump
(`cargo update -p h2`) — a dependency change outside this slice's scope, so per the rubric's
"out of scope → decline-with-issue-reference" it is reported rather than folded in. **Expect
`C4-ci` red at Check for this reason**; every other check in the gate passed.

### C5 (`scripts/mutants-in-diff`) — still blocked by a pre-existing repository-dependent guard

The gate as configured stops before testing any mutant, for a reason that is not this bundle's:

```
$ GIT_DIR=/nonexistent cargo test -p xtask --test repo_hygiene_guards
test scan_gitlinks_is_green_over_the_real_index ... FAILED
  xtask/tests/repo_hygiene_guards.rs:137: git ls-files -s -z must succeed
  test result: FAILED. 28 passed; 1 failed
```

cargo-mutants copies the source tree **without** `.git`; that guard requires a real index. The
patch does not touch that file — it only pulls `xtask` into the mutant package set, because the
slice must edit `xtask/src/dist.rs` to single-source the derivation (removing that interaction
means abandoning the shared-module design the brief specifies). Two human-owned options, both
outside this slice: `copy_vcs = true` in `.cargo/mutants.toml` (§5 quantifies it: +30 MB per
mutant tree), or teaching that guard to skip cleanly with no repository (a test-fidelity fix in
an unrelated file → decline-with-issue-reference, follow-up issue). Meanwhile the evidence the
gate would have produced is in §4(a): the tool's own verdict with that one guard skipped
(**0 missed**) plus six hand-run production mutations.

---

## 7. Honest limits (for §6/§9 at sign-off)

1. **The `VERSION`-equality claim is WRITTEN and REVIEWABLE but UNOBSERVED.** The check exists
   as executable shell inside the release smoke step (`.github/workflows/release.yml:79-117`):
   start the installed binary as an `s3` role, curl it unsigned (refused 403, header still
   present), compare the `Server:` remainder with the `version:` line of the untarred `VERSION`.
   It runs on a `v*` tag push, and **no `v*` tag has ever been cut** in this repo (the only tag
   is `archive/backup-premerge-signoff`; `release.yml:20-23` triggers on `push: tags: ["v*"]`).
   So at this sign-off the equality is *not demonstrated* — I did not run it, and I did not
   attempt an image build (the brief forbids it; it needs Docker + network). What IS
   demonstrated at Check is the coupling that feeds it: the build-arg list carries the same
   string `VERSION` records (`xtask/tests/dist_templates.rs:337`), and the Dockerfile declares
   and exports the ARG (`:376`).
2. **The advertised value on an untagged checkout is `0.0.0+git.<sha>`** — a real build
   identity, not a release version. The release-shaped value (`0.1.0`, `0.1.0+git.3.<sha>`)
   appears only once a tag exists. This is the brief's own "production reach" caveat, and it is
   what the binding test observes today (`wyrd/0.0.0+git.a801997`).
3. **A dirty developer tree advertises its commit without a dirty marker** (§3.4) — the
   `[human]` "ratify or restore" item carried over from round 2, restated here so it is decided
   rather than inherited. Every artifact `cargo xtask dist` produces still carries `.dirty`.
4. **`cargo xtask ci` is red on RUSTSEC-2026-0258** (§6) — pre-existing, dependency-only,
   remediation named.
5. **The `--host` and image paths of `dist` were not executed** (both need a toolchain/Docker
   this Check does not have). Their pure decisions are unit-tested; the shell that drives them
   is read-only reviewed.
6. **A malformed `WYRD_VERSION` now fails the build** rather than degrading. That is a
   deliberate behaviour change for anyone who was setting a value with spaces or `/` in it —
   nobody can be today (the variable is introduced by this slice), but it is worth seeing at
   sign-off as a decision rather than an implementation detail.
7. No external dependency was missing: the build script needs only `git`, which the checkout
   and the packaging pipeline already require, and it falls back cleanly (`0.0.0+git.unknown`,
   origin `unknown`) when git is absent.

---

## 8. Scratch

All throwaway work lived under `$PDCA_SCRATCH` as `pdca-builder-736-*` (the slash-tag git
probe, the red-leg backups, the mutation backups, the gate logs) and was removed before
handover; every measurement quoted above is reproduced inline rather than by reference to a
deleted file. Nothing was written outside `$PDCA_WORKTREE` and the bundle directory.
