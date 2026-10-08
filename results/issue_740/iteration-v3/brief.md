# Brief — issue 740 / validate-crate-skeleton-and-blackbox-lint

> Plan artifact (docs 02 §PLAN). Do reads ONLY this file (plus the peer callsites cited
> under **Citations expected**). The `- **Label:** value` lines are parsed by the driver.
>
> Plan of record: `docs/design/proposals/draft/0017-blackbox-validation-tool.md` §2
> (layering) and §9 (the blackbox property, as a lint). Read in place in the target
> checkout — never copied here.

- **Slug:** validate-crate-skeleton-and-blackbox-lint
- **Kind:** enhancement (design proposal)
- **Goal:** Wyrd has no place to put a blackbox validator, and no mechanism that keeps one
  blackbox. Create the workspace member `crates/validate` (package + binary
  `wyrd-validate`), give it the argument surface every later slice parses, and land — in
  the same change — the `xtask` invariant that its **normal** dependency closure contains
  no `wyrd-*` crate. The boundary must exist from commit one: a lint that lands after the
  code is a lint that lands after the first shortcut.
- **Defect:** (framed as the gap) There is no `crates/validate` and no dependency-closure
  guard. `xtask/src/repo_guard.rs` today carries exactly two invariants — the stray-gitlink
  scan (`scan_gitlinks`, `repo_guard.rs:238`) and the `#![forbid(unsafe_code)]` crate-root
  scan (`scan_roots`, `repo_guard.rs:500`) — verified on `main` at `65ca4fd`. Nothing
  prevents a future slice from linking `wyrd-core` into the validator and quietly
  destroying the only property that makes its verdict mean anything.
- **Success criterion:** BINDING (demonstrable by C4-verify at Check, every leg in
  `cargo xtask ci`):
  1. `cargo xtask ci` is green with `crates/validate` a workspace member, and the new guard
     RUNS INSIDE IT — asserted, not assumed. The guard's registration moves into the `xtask`
     **lib** target as data (the repo's own precedent: the feature-gated check list "lives
     in `xtask::feature_gated_checks` (the lib target) so `xtask/tests/fdb_harness.rs` can
     assert its content directly", `xtask/src/main.rs:1504-1505`), and the test asserts the
     blackbox guard is in the list `run_ci` executes. Without that, a guard that is defined,
     tested and never called passes every assertion in this brief — which is exactly the
     wiring hazard the repo already engineered against with `run_ci_steps`' injected `exec`
     (`main.rs:1486-1498`).
  2. The guard is **flippable, not vacuous**: fed a synthetic `cargo metadata` document in
     which `wyrd-validate` has a **normal** dependency on a `wyrd-*` crate, the pure scan
     function returns exactly one violation naming that crate; fed the same document with
     that edge marked `"kind": "dev"`, it returns none; fed one where the `wyrd-*` edge is a
     normal dependency that is **`optional` and off by default**, it returns the violation
     (see Design — this is the case a default-feature resolve hides); and run over the REAL
     workspace metadata it returns none.
  3. The CLI surface is bound flag by flag, not by sample: `wyrd-validate` invoked with ALL
     of `--endpoint --region --bucket --scenario --duration --workers --seed --out --run-id
     --driver-placement` exits 0 and echoes a resolved-configuration block in which EACH of
     the ten flags appears with the value it was given (assert per flag, so an implementation
     that parses three and ignores seven fails); a missing required argument exits non-zero
     with the offending flag named on stderr; and an UNRECOGNISED flag exits non-zero naming
     it (a deliberate departure from the peer parser — see Design and the Plan-review
     response).
  4. Credential resolution is bound in both directions, over the injected lookup so no
     process env is mutated: with `AWS_ACCESS_KEY_ID`/`AWS_SECRET_ACCESS_KEY` present the
     resolved id is the AWS one and the reported source says so; with those absent and
     `WYRD_S3_ACCESS_KEY`/`WYRD_S3_SECRET_KEY` present the resolved id is the Wyrd one and
     the source says so; with BOTH present AWS wins; with neither the run exits non-zero
     naming what to set. In every case the echoed block contains the access-key **id** and
     never the secret — asserted by searching the whole output for the secret's value.
- **Falsifiability:** Criterion 2 is where RED lives and it is producible on the ordinary
  developer harness Do is pointed at — no topology, no service. The three planted-metadata
  cases (normal edge → one violation; the same edge marked `dev` → none; an optional
  off-by-default normal edge → the violation) make the guard fail on demand, in the
  `xtask/tests/repo_hygiene_guards.rs` idiom: synthetic inputs, so demonstrating the red
  never requires committing the accident. Criteria 3 and 4 go red pre-fix trivially — there
  is no binary. Criterion 1 is what stops a *parser-only* green: a guard can pass every
  planted case and still never run, if `run_ci` does not call it or discards its violations,
  so the registration lives in the lib as data and the test asserts the guard is in the list
  `run_ci` executes — the same reason `feature_gated_checks` lives there
  (`xtask/src/main.rs:1504-1505`). The guard must additionally fail closed (see Design) when
  handed a metadata document in which the package is absent or the resolve graph is missing,
  mirroring `scan_roots`' "refusing to pass a workspace it cannot see"
  (`repo_guard.rs:505-510`).
  KNOWN GATE SHAPE, pre-declared so it is not a surprise at sign-off: this instance's
  `C4-verify` classifies a discriminator on an **added** `*/tests/*.rs` file
  (`engine/scripts/run-verify.sh:141-144`) and, because this patch CREATES the crate that
  one of the two test files lives in, the gate takes its `GREEN_ONLY` branch and records
  `PASS (green-only)` (`run-verify.sh:412-414`, `:499-503`). That is correct and expected
  for a net-new crate; the load-bearing red is criterion 2's planted violation, which runs
  inside `cargo xtask ci` (the one GATING row) every cycle thereafter.
- **Invariant to restore:** *Nothing that ships inside the `wyrd-validate` binary may reach
  a Wyrd workspace crate — and that must be enforced by the gate, not by a doc comment.*
  Stated over the category (the package's whole normal dependency closure, transitively AND
  under every feature — optional, off-by-default edges included), not over one edge: a guard
  that only checked direct dependencies would pass a validator reaching `wyrd-core` through
  one hop, and a guard that only resolved DEFAULT features would pass one reaching it behind
  an optional feature. Source: proposal 0017 §9 ("the **normal**
  dependency closure of its binary target must contain **no** `wyrd-*` crate … Normal, not
  total"), and the repo's own "found twice becomes a gate" rule recorded at
  `xtask/src/main.rs:1366-1372`.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Conflicts with:**
- **Ordering note:** Wave 0, alongside #736 (disjoint file sets: this touches
  `crates/validate/**`, root `Cargo.toml` `[workspace] members`, `xtask/src/repo_guard.rs`,
  `xtask/src/main.rs`, `xtask/src/lib.rs`; #736 touches `crates/gateway-s3`,
  `crates/server`, `xtask/src/dist.rs`, the Dockerfile). #741 declares `Depends on: 740`
  and therefore builds on this crate in the next wave. #742 (tarball packaging) IS in this
  batch after all — it declares `Depends on: 740, 741` and lands in wave 2 — so the batch
  schedule is `[736, 740] → [738, 741] → [742]`. (An earlier draft of this note said #742
  was excluded; corrected here.) Nothing in this slice changes as a result: #742 consumes
  the crate and the binary this one creates, and touches none of its files.
- **Surfaces:** data
- **Difficulty:** medium
- **Scope:** (a) new workspace member `crates/validate` — package `wyrd-validate`, a lib
  target holding the pure decisions and a thin `[[bin]] wyrd-validate` over it, both crate
  roots carrying `#![forbid(unsafe_code)]`; (b) the argument surface `--endpoint --region
  --bucket --scenario --duration --workers --seed --out --run-id --driver-placement`,
  parsed, validated for presence, and echoed as a resolved-configuration block; (c)
  credential resolution from `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY`, falling back to
  `WYRD_S3_ACCESS_KEY` / `WYRD_S3_SECRET_KEY`, over an injected lookup; (d) a third
  invariant in `xtask/src/repo_guard.rs` — the package's **normal** dependency closure,
  resolved with `--all-features`, plus its declared normal (incl. optional) dependencies,
  contain no `wyrd-*` crate — wired into `run_ci` through a lib-side registration a test can
  read; (e) its flippable test, including the optional-dependency case; (f) **strict
  unknown-flag rejection** — deliberately different from the peer parser, declared here as
  intentional scope rather than left as a Design aside, and asserted by criterion 3.
  **/ out of scope:** any S3 call whatsoever (that is #741 — this binary parses, echoes and
  exits); the capability matrix and `smoke` (#743); scenarios, oracle, pools, verdict; ANY
  new third-party crate (see Design — the arg parsing is hand-rolled precisely so this
  slice lands without an ADR-0003 dependency audit); tarball packaging (#742, wave 2 of this
  batch — it consumes this crate, it is not built here); extending the guard to any package
  other than `wyrd-validate`.
- **Repro instruction:** On `main` at `65ca4fd`: `ls crates/validate` → no such directory;
  `grep -n "wyrd-" xtask/src/repo_guard.rs` → the file never mentions a dependency closure;
  `cargo xtask ci` is green, i.e. today's gate is satisfied by a workspace that has no way
  to violate the property.
- **External dependencies:** none — the base Rust toolchain only. The new guard shells out
  to cargo's own metadata subcommand, exactly as the existing one already does
  (`xtask/src/main.rs:1451-1456`), so it needs nothing beyond the toolchain Do is handed.
  Do MUST NOT add a third-party crate to `crates/validate` in this slice; if one seems
  unavoidable, stop and declare it rather than adding it (it is an ADR-0003 §2 three-test
  audit + `deny.toml` allowlist decision, a human-only item per INTEGRATION §4).
- **Test file:** `xtask/tests/blackbox_dependency_guard.rs` and `crates/validate/tests/cli_surface.rs` — both NEW files.
- **Verification posture:** DECLARED, not the default. This is net-new coverage: there is no
  prior failing assertion to flip, so "red" for criteria 3 and 4 is criterion-ABSENCE, and
  criterion 1's wiring assertion is red pre-fix because the guard does not exist to be
  registered. What is nonetheless a *demonstrated* red rather than a red resting on
  non-existence: criterion
  2's planted-metadata cases, where the same pure function `cargo xtask ci` runs is driven
  over a document containing a real violation and MUST report it. Do must ship that case;
  a guard test that only asserts the real tree is clean is a vacuous assertion and will be
  rejected. Everything in this slice is BUILT and EXERCISED at Check — the crate compiles
  and its binary runs under `cargo xtask ci`, the guard runs inside `run_ci`, and both test
  files execute there. Nothing here is deferred to a later environment.
  On the `C4-verify` row specifically: expect `PASS (green-only)` for the reason recorded
  under Falsifiability. Do should not contort the patch to chase a red leg from that row.
- **Citations expected:** Do must cite `path:line` on `main` for every change. Four peer
  callsites Do MAY open and should mirror (a deliberate, narrow exception to reading this
  brief only):
  * **The guard's shape and its wiring** — `xtask/src/repo_guard.rs:387-408`
    (`target_src_paths`: a pure function over `cargo metadata` JSON, `Result<_, String>`,
    failing closed on a document it cannot parse) and `xtask/src/main.rs:1443-1481`
    (`run_unsafe_forbid_guard`: `print_step`, shell out to `cargo metadata`, hand the JSON
    to the pure function, join violations into one error). Add the new guard the same way
    and call it from `run_ci` beside `run_unsafe_forbid_guard` (`xtask/src/main.rs:1558`).
  * **How this repo makes a `run_ci` call site TEST-VISIBLE** — `xtask/src/main.rs:1486-1544`
    (`run_ci_steps`, whose `exec` and `toolchain` are injected "so the real wiring is
    exercised without spawning cargo", and whose row list "lives in
    `xtask::feature_gated_checks` (the lib target) so `xtask/tests/fdb_harness.rs` can assert
    its content directly"). Note the shape of the problem: `run_gitlink_guard` and
    `run_unsafe_forbid_guard` are `fn`s in the BINARY target, so no integration test can see
    whether `run_ci` calls them. Do not extend that. Register the hygiene guards as data in
    the lib target (name + callable), have `run_ci` iterate it, and assert the new guard's
    presence from the test. Criterion 1 is that assertion.
  * **The flippable-test idiom** — `xtask/tests/repo_hygiene_guards.rs:34-49`
    (`scan_gitlinks_is_red_on_an_undeclared_gitlink`: synthetic input, assert exactly one
    violation, assert it names the offender) and `:384-403`
    (`scan_crate_roots_is_green_over_the_real_workspace_crates`).
  * **Hand-rolled argument parsing** — `crates/server/src/cli.rs:2495-2532`
    (`ParsedArgs::parse` / `flag` / `positional`). Mirror the SHAPE; do **not** depend on
    `wyrd-server` to get it (that is the very edge the new lint forbids). The `--flag value`
    convention and the "a flag needs a value" error wording should match so the two
    binaries feel like one product.
  * **Injected environment lookup, so the credential fallback is unit-testable without
    mutating process env** — `xtask/src/main.rs:1519` (`run_ci_steps(&mut |name|
    std::env::var_os(name).is_some(), …)`), the repo's stated "pure decisions, injected
    I/O" convention (proposal 0017 §2 cites `xtask/src/consistency_run_runner.rs:8-12`).
- **Prior-art check (triage cycles):** searched by affected path on `main` at `65ca4fd`.
  `git log --oneline -- crates/validate` → empty (the directory has never existed).
  `git log --oneline -- xtask/src/repo_guard.rs` → six #616 commits, all on the gitlink and
  unsafe-forbid scans; none touches a dependency closure. `gh pr list --search
  "wyrd-validate"` and `--search "blackbox"` across all states → only PR #765 (the merged
  proposal 0017 document). No closed/rejected attempt at this work exists.
- **Disposition hint:** new-feature

## Motivation

The blackbox property is the premise of the whole tool. `wyrd-validate` exists to answer
"is Wyrd correct" from outside, the way a client does; the moment its binary links a
`wyrd-*` crate it is testing Wyrd's types against Wyrd's types and its verdict is
self-referential. Proposal 0017 §9 makes that a *mechanical* property rather than a
promise, and §2 says it is also why the tool must never become a `wyrd` subcommand — a
subcommand shares a dependency closure by construction.

Landing the crate and the lint together is the whole point of this slice. A lint added
later is added after the first shortcut has already been taken and grandfathered.

## Design

**The crate.** `crates/validate`, package `wyrd-validate`, added to `[workspace] members` in
the root `Cargo.toml`. Registration is not optional bookkeeping: `unregistered_manifests`
(`xtask/src/repo_guard.rs:421-455`) already fails the gate on a package under `crates/` that
is not a member, precisely because "nothing compiles, tests, or lints it — it looks live
while being dead".

Two targets: a **lib** holding the pure decisions (so integration tests can reach them, as
proposal 0017 §2's "pure, Check-tested" column requires) and a thin **bin** `wyrd-validate`
over it. Both crate roots carry `#![forbid(unsafe_code)]` — the existing guard scans every
target kind including bins and build scripts (`repo_guard.rs:380-386`), so a missing
attribute on either root fails `cargo xtask ci` immediately.

**No new dependency.** The argument surface is hand-rolled in the shape of
`ParsedArgs` (`crates/server/src/cli.rs:2495`). This is a deliberate scope decision, not
laziness: making `wyrd-validate` a *shipped* workspace member is already the change that
brings `aws-sdk-s3` and ~100 transitive crates inside `deny.toml`'s frame in the NEXT slice
(#741), and that is a declared human-only ADR-0003 audit. Keeping THIS slice at zero new
crates means the boundary and the lint land with no license decision attached, and #741
carries exactly one dependency question instead of two.

One difference from `ParsedArgs` worth making deliberately — and now IN SCOPE explicitly
(scope (f)) rather than a Design aside: `ParsedArgs::parse` silently accepts an unknown
`--flag` (it just records it in the map, `crates/server/src/cli.rs:2501-2522`). For a tool an
operator points at their own cluster, a typo'd flag that is silently ignored is a footgun —
`--duration 7d` misspelled means a two-minute run reported as a week — and unlike `wyrd`,
this binary is NEW, so a stricter policy breaks no existing invocation and grandfathers
nothing. `wyrd-validate` **refuses** an unrecognised flag, naming it, and criterion 3
asserts that. Mirror `ParsedArgs`' shape and error wording; do not mirror that one
behaviour. (If sign-off would rather the two binaries behave identically, this is one
branch to delete and one assertion to drop — say so at §9.)

**Credentials.** `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY` first (what every S3 client
and every CI runner already sets), falling back to the gateway's own `WYRD_S3_ACCESS_KEY` /
`WYRD_S3_SECRET_KEY` (`crates/server/src/cli.rs:2128,2135`) so a developer already running
a loopback gateway needs no second export. Resolution takes an **injected** lookup closure
rather than reading `std::env` directly, so the fallback ordering is unit-tested without
mutating process-global state (which is shared across parallel test threads and flakes).
The echoed configuration prints the access-key **id** and the *source* it came from; it
never prints the secret.

**The guard.** A third invariant in `xtask/src/repo_guard.rs`, discovered exactly as the
existing scans are — through `cargo metadata`, chosen there "so no manifest override or
unconventional layout can hide a root" (`repo_guard.rs:26-31`).

One difference in the invocation: the unsafe-forbid guard runs `cargo metadata --no-deps`,
which omits the resolve graph. This guard needs the graph, so it runs `cargo metadata
--format-version 1 --locked --all-features` **without** `--no-deps` and walks
`resolve.nodes`. `--locked` matters: without it a metadata call can rewrite `Cargo.lock`,
and a *guard* that mutates the tree it audits is its own defect. Verified on the real
workspace: each node's `deps[].dep_kinds[].kind` is `null` for a normal edge and `"dev"`
for a dev edge (checked against `wyrd-server` → `aws-sdk-s3`, which reports
`[{"kind": "dev", "target": null}]`). That distinction is the whole of "normal, not total":
transitively follow only `kind: null` edges from the `wyrd-validate` node, and report every
reached package whose name begins with `wyrd-`.

**`--all-features`, and a second check beside the graph — because a default resolve does not
see the whole closure.** The resolve graph is FEATURE-DEPENDENT, and this repo already uses
the pattern that hides an edge from it: `crates/metadata-tikv/Cargo.toml:11-30` declares
`tikv-client`, `tokio`, `async-trait`, `bytes` as **optional normal** dependencies activated
only by an off-by-default feature, and `xtask/src/lib.rs:40-47` records that the default
workspace commands do not cover non-default features at all. A `wyrd-validate` that declared
`wyrd-core = { workspace = true, optional = true }` behind a feature would therefore be
invisible to a default-feature resolve — and the invariant this brief states would be false
while the gate stayed green. Two things close it, and BOTH are required:

1. resolve with `--all-features`, so every feature-activated normal edge appears. Verified
   on this checkout rather than assumed: `cargo metadata --all-features --locked
   --format-version 1` exits 0 offline, and `wyrd-metadata-tikv`'s node then carries
   `tikv-client`, `tokio`, `async-trait` and `bytes` as `kind: null` edges — exactly the
   ones a default resolve omits;
2. additionally scan the package's **declared** dependency list — `packages[].dependencies`,
   which is feature-INDEPENDENT and carries `kind` (`null` / `"dev"` / `"build"`) and
   `optional` per entry (verified on the same document: tikv's four optional deps appear
   there as `kind: null, optional: true`) — and report any `wyrd-*` entry whose `kind` is
   `null`, optional or not. This is the belt to the graph's braces: it catches a declaration
   even if some future feature-resolution subtlety keeps it out of the graph, and it is what
   makes criterion 2's third planted case meaningful.

Report both classes as violations of the same invariant, with wording that says which one
fired.

Pure function in, violations out — `pub fn <name>(metadata_json: &str, package: &str) ->
Result<Vec<String>, String>` — so the same function `cargo xtask ci` runs is the one the
test drives over planted documents. **Fail closed**, in the `scan_roots` style: an
unparsable document, an absent `resolve` section, or a `package` that appears in no node is
`Err`, never a vacuously clean pass. A guard that cannot see the graph must say so.

Report the **path** to the offending crate, not just its name, when it is reachable in more
than one hop — the violation an operator reads should say how the edge got there.

## Alternatives considered

**Put the tool behind `wyrd validate`.** Rejected in proposal 0017 §2 and again in #742's
"Why not a `wyrd` subcommand": a subcommand shares the roles' dependency closure, so the
lint would have to be scoped away, and once scoped away it means nothing.

**Enforce the property by convention (a doc comment on the manifest).** That is the state
the `#![forbid(unsafe_code)]` convention was in before #616 — "held by habit until the two
newest crates shipped without the attribute" (`xtask/src/main.rs:1440-1442`). The repo has
already learned this one.

**A `clap`-based argument surface.** Rejected for this slice only: it is a new dependency,
and a new dependency is a human-only decision (INTEGRATION §4). The wyrd binary itself
hand-rolls, so the tool is consistent with its product. Revisit if the surface grows past
what hand-rolling reads well — as a separate, argued change.

**Check only direct dependencies.** Cheaper, and wrong: one hop through any crate that
depends on `wyrd-core` reintroduces exactly what the property forbids. The closure is
transitive or it is decorative.

**Scan the manifest text for `wyrd-` instead of the resolve graph.** Rejected for the same
reason `repo_guard.rs:26-31` gives for using `cargo metadata` at all: a manifest override,
a renamed dependency (`foo = { package = "wyrd-core" }`) or a patch section hides it.

## Impact & compatibility

Additive. No existing crate changes behaviour; `cargo xtask ci` gains one sub-second guard
and one more workspace member to compile. The guard's only subject is the `wyrd-validate`
package, so it cannot fail on any existing crate — which is also why it must be flippable
by construction: it will otherwise be green forever regardless of whether it works.

`cargo metadata --all-features` without `--no-deps` resolves the full graph, so the guard
reads `Cargo.lock` rather than merely the manifests. That is the correct input (it is the
graph that actually links) and needs no network in CI, which already builds from the
committed lockfile — verified offline on this checkout, exit 0, ~2.6 MB of JSON, no build.
`--all-features` widens the resolve only; it compiles nothing, so the guard stays
sub-second and needs neither the TiKV nor the FoundationDB toolchain that the corresponding
*builds* would.

Two behaviours to keep honest at review: the guard must not be satisfiable by removing
`crates/validate` from `[workspace] members` (that is caught separately by
`unregistered_manifests`, and the new guard's fail-closed branch must not turn a missing
package into a pass), and the crate's dev-dependencies are deliberately unconstrained —
#741 and §14's fixtures rely on that.

## Plan-review response (#301 revision pass)

Four findings, all accepted and all revised in place.

* **"A non-default feature hides an optional normal `wyrd-*` edge."** The strongest of the
  four, and the pattern is live in the target (`crates/metadata-tikv/Cargo.toml:11-30`;
  `xtask/src/lib.rs:40-47`). The guard now resolves with `--all-features` AND scans the
  feature-independent declared list (`packages[].dependencies`, `kind: null`, optional or
  not); criterion 2 gains a third planted case for the optional edge. Both mechanisms were
  checked against the real workspace before being written down, not assumed.
* **"The red exercises the parser, not the wiring."** Correct, and it is the classic way a
  guard ships dead. Criterion 1 now requires the guard's registration to live in the `xtask`
  lib as data with the test asserting `run_ci` runs it — the repo's own
  `feature_gated_checks` precedent (`xtask/src/main.rs:1504-1505`), noted alongside the fact
  that today's two guards are binary-target `fn`s no test can see.
* **"The CLI criterion binds three flags and no credential fallback."** Correct. Criterion 3
  now binds each of the flags with its resolved value (the tracker lists **ten**, not eleven
  — `--endpoint --region --bucket --scenario --duration --workers --seed --out --run-id
  --driver-placement`), and a new criterion 4 binds all four credential cases (AWS present,
  Wyrd fallback, both present → AWS wins, neither → refuse) plus "the secret never appears
  in the output", over the injected lookup.
* **"Unknown-flag rejection is undeclared extra scope."** Correct that it was buried in
  Design. It is KEPT — this binary is new, so a strict policy breaks nothing and
  grandfathers nothing, and a silently-ignored `--duration` typo misreports a run — but it
  is now scope item (f), asserted by criterion 3, with the note that it is one branch and one
  assertion to drop if sign-off prefers parity with `ParsedArgs`.

## Open questions

1. **Guard naming.** `run_blackbox_dependency_guard` / `scan_blackbox_closure` are
   placeholders; match whatever reads best beside `run_gitlink_guard` /
   `run_unsafe_forbid_guard`.
2. **Should the guard be generalised to a data table** (a list of `(package, forbidden
   prefix)` pairs, in the `UNSAFE_FORBID_ALLOWLIST` style) rather than hard-coding
   `wyrd-validate`? One entry today; the table shape is cheap and makes the second entry a
   one-line reviewed diff. Do's call — either is acceptable, the table is preferred if it
   costs nothing.
3. ~~**Milestone integration branch.**~~ **ANSWERED — `main`, confirmed (maintainer,
   2026-08-17).** Milestone 17's 28 open issues are the shape INTEGRATION §2 describes a
   shared `feat/*` integration branch for (the M4 pattern), and the maintainer considered
   it and chose `main` instead. So every bundle in this batch targets
   `getwyrd/wyrd @ main` by decision, not by default — which is also the base `C4-verify`
   resolves and the base publish opens each PR against. Kept here as the record for the
   remaining milestone-17 slices, which should follow it unless it is revisited.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR
MAY happen during the cycle. The PR MUST NOT be marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 1): rebuilding for the implementation-level findings — C5 Causal adequacy — Rebuild the strict-flag proof to cover a flag-shaped value token — the present test checks unknown flags only in flag position, so it misses the parser consuming `--totally-bogus` as another flag's value (`crates/validate/tests/cli_surface.rs:117`, `crates/validate/src/lib.rs:79`).; T5 Judgment — Route back to Do for the strict-parser regression and test above — accepting an unknown flag in a value slot contradicts the operator-safety decision in the brief and can silently misreport a run (`brief.md:217`, `crates/validate/src/lib.rs:79`).; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_740/review-b. 4 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — ERROR cargo test failed in an unmutated tree, so no mutants were tested
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_740/review-b
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 2): rebuilding for the implementation-level findings — C5 Causal adequacy — Rebuild the proof around the actual trust boundary: add successful multi-hop closure, malformed declared-`kind`, non-UTF-8 credential, and control-character output cases, because current planted tests exercise only direct normal/dev/optional edges and UTF-8 fixtures (`xtask/tests/blackbox_dependency_guard.rs:81`, `crates/validate/tests/cli_surface.rs:318`).; T5 Judgment — Rebuild must fail closed on unreadable metadata and credentials, emit an unambiguous configuration record, and prove transitive traversal; otherwise the operator-identity and blackbox-boundary judgments are not trustworthy (`crates/validate/src/main.rs:11`, `xtask/src/repo_guard.rs:757`, `crates/validate/src/lib.rs:302`).; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_740/review-b. 4 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — ERROR cargo test failed in an unmutated tree, so no mutants were tested
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_740/review-b
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: Size backstop is tripped on its own terms: patch is 114 KB against a 100 KB threshold, and 2 build/iterate rounds have already been spent — the bundle's own recommendation is to re-split rather than attempt a third Do round. That call is reinforced by the shape of the findings, which read as several separable concerns bundled into one slice rather than one coherent defect: - the dependency-closure guard's fail-closed gap on malformed/incomplete `cargo metadata` records (missing `id`/`name` silently skipped or opaque-ID-fallback at repo_guard.rs:729/730/751) — the guard's actual load-bearing purpose; - the guard's own red/wiring criterion tests only the pure scan function, not that `cargo xtask ci` actually calls it; - the CLI surface criterion covers 3 of 11 flags and doesn't bind the credential fallback per the tracker's own ask; - the strict-unknown-flag-rejection behavior is scope not present in the tracker and needs an explicit accept/drop call. Re-plan should weigh whether the crate skeleton + CLI surface + credential resolution + dependency guard truly belong in one slice, or split along those seams (e.g., guard correctness as its own child) before another Do attempt.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Size backstop is tripped on its own terms: patch is 114 KB against a 100 KB threshold, and 2 build/iterate rounds have already been spent — the bundle's own recommendation is to re-split rather than attempt a third Do round. That call is reinforced by the shape of the findings, which read as several separable concerns bundled into one slice rather than one coherent defect:
  - the dependency-closure guard's fail-closed gap on malformed/incomplete `cargo metadata` records (missing `id`/`name` silently skipped or opaque-ID-fallback at repo_guard.rs:729/730/751) — the guard's actual load-bearing purpose;
  - the guard's own red/wiring criterion tests only the pure scan function, not that `cargo xtask ci` actually calls it;
  - the CLI surface criterion covers 3 of 11 flags and doesn't bind the credential fallback per the tracker's own ask;
  - the strict-unknown-flag-rejection behavior is scope not present in the tracker and needs an explicit accept/drop call.
  Re-plan should weigh whether the crate skeleton + CLI surface + credential resolution + dependency guard truly belong in one slice, or split along those seams (e.g., guard correctness as its own child) before another Do attempt.
- Failing gate: C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) — xtask: `cargo deny check` failed with exit status: 1
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — ERROR cargo test failed in an unmutated tree, so no mutants were tested
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_740/review-b
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
