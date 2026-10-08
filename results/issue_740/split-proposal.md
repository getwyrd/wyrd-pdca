<!-- pdca:split-proposal v1 -->
# Split proposal — issue 740

## Why this slice is oversized

Three Do rounds, each patch larger than the last — 47 KB → 72 KB → **117 KB** against a
100 KB threshold (`iteration-v{1,2,3}/size-signal.json`) — and sign-off returned the bundle
to Plan on the size backstop rather than spending a fourth round.

The v3 patch divides in half along a **zero-overlap file boundary**:

| half | files | added lines |
|---|---|---|
| the crate | `crates/validate/{Cargo.toml,src/lib.rs,src/main.rs,tests/cli_surface.rs}`, root `Cargo.toml`, `Cargo.lock` | ~1376 |
| the guard | `xtask/src/repo_guard.rs`, `xtask/src/lib.rs`, `xtask/src/main.rs`, `xtask/tests/blackbox_dependency_guard.rs` | ~1003 |

The blocking findings **alternated between the halves** round by round: v1 was a CLI parser
edge case (an unknown flag consumed in a *value* slot); v2 was guard fail-closed coverage
*and* CLI encoding cases; v3 was guard metadata fail-closed — all three T4 findings one
defect at `repo_guard.rs:729/730/751`. Each round fixed one surface and the next review pass
found the other. That is the signature of a slice too wide for one review pass, not of a
stubborn defect.

The split also buys a **RED the combined slice could not earn**. Dry-running this instance's
own classifier (`engine/scripts/run-verify.sh --classify`) on a synthetic patch per child:

- a patch that CREATES `crates/validate` classifies `CRATE crates/validate`, absent on base,
  so `GREEN_ONLY=1` (`run-verify.sh:412`) and the row records `PASS (green-only)` (`:497`);
- a **guard-only** patch classifies `CRATE xtask`, which exists on base, so the gate reverts
  the production change, keeps the added test, and demands a genuine red→green.

Separating them makes the load-bearing half falsifiable.

**A third child is prepended, and it is not a decomposition of #740's work — it is the
blocker in front of it.** `cargo deny check advisories` FAILS on a clean `main` @ `65ca4fd`:
RUSTSEC-2026-0258 (`h2 0.4.15`, patched `>= 0.4.16`, advisory published 2026-08-17).
`cargo_deny_check()` runs inside `run_ci` (`xtask/src/main.rs:1563`) and `C4-ci` is the one
**gating** row, so the base is red before any patch applies. This is demonstrably new rather
than a standing repo defect: iteration-v1 (Aug 17 22:47) and v2 (Aug 18 02:24) both logged
`advisories ok` → `xtask ci: all checks passed`; only v3 (Aug 18 13:13) failed, after the
local advisory-db refreshed. It is filed here as child-1 so this run drives it instead of
leaving it to hand-work — every other child (and #736/#738/#741/#742) is blocked until it
lands.

## Wave sketch

A strict chain, one child per wave — three waves, each cheap:

```
child-1 (h2 bump)  ──▶  child-2 (the crate)  ──▶  child-3 (the guard)  ──▶  #741 ──▶ #742
```

- **child-2 `Depends on` child-1** — not a code dependency but a *gate* dependency. `C4-ci`
  is the only gating row and it is red on the base until the lockfile bump lands; a child-2
  built alongside child-1 would fail its gate for a reason that has nothing to do with its
  own patch. Under `wave_mode = "merge"` + `auto_merge = true` the driver merges each
  non-final wave, so child-2 builds on a genuinely green `main`.
- **child-3 `Depends on` child-2** — a real build-on dependency. The guard's subject is the
  `wyrd-validate` package, and its own fail-closed requirement turns an absent package into
  an `Err`, so child-3 on a base without child-2 is red by construction.
- **No `Conflicts with` edges anywhere** — the three file sets are disjoint: `Cargo.lock`
  only; `crates/validate/**` + root `Cargo.toml`; `xtask/**`. (child-1 touches `Cargo.lock`
  and child-2 also regenerates it by adding a workspace member, but they are in different
  waves already, so the ordering is settled by `Depends on`.)
- **Outside this proposal, but part of the same schedule:** #741 declares `Depends on: 740`
  and #742 declares `Depends on: 740, 741`. A split parent never reaches COMPLETE, so
  `_runnable` (`flow.py:702`) would skip both. Both must be re-pointed at **child-3**
  (maintainer approved, 2026-08-18).

Order **child-3 before #741** preserves the tracker's stated intent — "the boundary must
exist from commit one" — because #741 is the first slice that adds *any* dependency to
`crates/validate`, so no shortcut can be taken in the gap.

<!-- pdca:child child-1 -->
- **Slug:** deps-bump-h2-rustsec-2026-0258
- **Kind:** dependency / security bump
- **Defect:** `cargo deny check` fails on a clean `main` @ `65ca4fd`:
  `error[vulnerability]: h2 unbounded empty DATA frames` — RUSTSEC-2026-0258
  (GHSA-q83h-524g-xf6h), `h2 0.4.15`, `patched = [">= 0.4.16"]`, advisory published
  2026-08-17. `cargo_deny_check()` runs inside `run_ci` (`xtask/src/main.rs:1563`), and
  `C4-ci` is this instance's one **gating** gate, so the entire repo's gate is red before any
  patch applies. The exposure is real and on normal (shipped) edges, not dev-only:
  `cargo tree -e normal -i h2@0.4.15` reaches it from `wyrd-server` three ways — via `axum`
  (the S3 gateway's HTTP surface), via `tonic` (`wyrd-proto`, `wyrd-chunkstore-grpc`,
  `tonic-health`), and via `opentelemetry-otlp` (`wyrd-telemetry`, `wyrd-custodian`). The
  flaw queues empty HTTP/2 DATA frames without limit: unbounded memory growth, or a panic on
  length overflow. Low severity, unauthenticated, on the request-serving path.
- **Success criterion:** BINDING, and the gate itself is the oracle. On the patched tree:
  1. `cargo deny check` exits 0 and reports `advisories ok, bans ok, licenses ok, sources
     ok` (the pre-existing `license-not-encountered` warning for `"ISC"` at `deny.toml:121`
     is expected and unchanged — it is present on base too);
  2. `Cargo.lock` names `h2 0.4.16` and no `h2 0.4.15` remains;
  3. `cargo xtask ci` exits 0 — the whole-tree gate, which is what proves the transitive
     re-unification below actually compiles;
  4. **no manifest is edited** — `git diff --name-only` against the base lists `Cargo.lock`
     and nothing else. In particular: no `deny.toml` ignore/waiver entry. Waiving the
     advisory instead of fixing it satisfies the gate and not the defect, and would be
     rejected at review.
- **Falsifiability:** The RED is the base itself and needs no construction: on `main` @
  `65ca4fd`, clean tree, `cargo deny check advisories` exits non-zero naming
  RUSTSEC-2026-0258. Verified today, twice — once via `cargo deny --manifest-path
  ../wyrd/Cargo.toml check advisories` against the live checkout, and once in a throwaway
  `git archive` copy where applying the bump flipped it to `advisories ok, bans ok, licenses
  ok, sources ok`. Red and green are both demonstrated, on the ordinary developer harness Do
  is pointed at — no topology, no service.
- **Invariant to restore:** *The dependency wall is green on `main`: no crate in the shipped
  (normal) dependency closure carries an unwaived RUSTSEC advisory.* Stated over the
  category, not over `h2`: the remedy is upgrading the affected crate, never adding an
  `[advisories] ignore` entry — `deny.toml:59-70` records the project's own reasoning for
  auditing `unsound = "all"` across the whole graph rather than just the workspace ("the
  anyhow bump to the patched 1.0.103 lands with this, so the wall is green on a real fix, not
  a waiver", #543). That precedent is binding here.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Reproduction:** On `main` @ `65ca4fd` with a clean tree:
  `cargo deny check advisories` → `error[vulnerability]: h2 unbounded empty DATA frames /
  ID: RUSTSEC-2026-0258 / Solution: Upgrade to >=0.4.16`, exit non-zero. Equivalently
  `cargo xtask ci` fails at the `cargo deny check` step.
- **Scope:** exactly one generated file — `Cargo.lock` — produced by
  `cargo update -p h2@0.4.15 --precise 0.4.16` run at the workspace root. **Bare
  `cargo update -p h2` is ambiguous and will error**: the tree also carries `h2 0.3.27` (via
  `tonic 0.12.3` under `madsim-etcd-client`), which cargo-deny does *not* flag. Do must run
  the disambiguated form and must not touch `h2 0.3.27`.
  **/ out of scope:** any `Cargo.toml` edit (no Wyrd manifest names `h2` — verified across
  all 23 workspace manifests; it is purely transitive through `axum 0.8` / `tonic 0.14` /
  `hyper`); any `deny.toml` change, especially an `[advisories] ignore` entry; any other
  advisory, dependency or code change; any attempt to also fix the pre-existing
  `license-not-encountered` `"ISC"` warning.
- **EXPECTED, pre-declared so it is not a surprise at review — the diff is 13 insertions /
  13 deletions, not 2.** Beyond `h2` (version, checksum, and 5 dependent references), the
  resolver re-unifies four platform edges: `errno`, `rustix` and `tempfile` re-point
  `windows-sys` 0.61.2 → 0.52.0; `nu-ansi-term` → 0.59.0; `hyper-util` re-points `socket2`
  0.6.4 → 0.5.10. This was investigated rather than assumed:
  * **every one of those versions is already in the lockfile** — no new crate enters the
    tree, nothing new to license-audit, and `cargo deny` reports `bans ok` (no new
    duplicate-version violation);
  * it is **not** caused by `h2`: 0.4.15 and 0.4.16 have byte-identical dependency
    requirements and the same `rust_version = 1.63` (checked against the crates.io index);
  * it is **not** ambient churn: a no-op `cargo update -p equivalent@1.0.2 --precise 1.0.2`
    on the same tree changes zero bytes;
  * `--precise 0.4.16` and a plain `cargo update -p h2@0.4.15` produce the identical 13/13
    delta, so there is no more surgical form.
  The cause is the MSRV-aware resolver (workspace `rust-version = "1.96"`, cargo 1.96.1)
  normalising those edges the moment the lock is rewritten. Note `socket2` is
  cross-platform, not Windows-only, so criterion 3 (`cargo xtask ci`) is what actually
  clears it — this child was verified at the dependency-wall level, not at the compiler.
  Do must NOT hand-edit `Cargo.lock` to suppress the re-unification.
- **External dependencies:** `cargo-deny`
- **Test file:** none, deliberately — see **Verification posture**. Do MUST NOT invent one:
  an xtask test asserting `h2 >= 0.4.16` would pin a version number in two places and rot at
  the next bump, and it would assert the patch rather than the property.
- **Verification posture:** DECLARED, not the default. There is no regression test and there
  should not be: the **dependency wall is the oracle**, it already runs inside the gating
  `C4-ci` row, and it is genuinely red on the base and green with the patch — a real
  red→green, just owned by `cargo deny` rather than by `cargo test`. Nothing is deferred:
  both legs are observable at Check on the ordinary harness.
  Pre-declared gate shapes, so neither reads as a defect at sign-off:
  * **`C4-verify` → PASS with `patch touches no Wyrd crate (docs/CI only) — nothing to
    verify per-fix; the C4-ci gate covers it.`** `_crate_dir("Cargo.lock")` is empty, so no
    package is selected and the script early-exits 0 at `run-verify.sh:425-428` before it
    even needs a toolchain. Correct and expected.
  * **`C4-diff-cov` → nothing to score.** The patch contains no `.rs` lines, so there are no
    instrumentable changed lines. Advisory row.
- **Citations expected:** Do must cite `path:line` on `main`. The three that matter:
  `xtask/src/main.rs:1563` (`cargo_deny_check()` inside `run_ci` — why this is gating),
  `deny.toml:59-70` (the project's own "green on a real fix, not a waiver" precedent), and
  the advisory itself (RUSTSEC-2026-0258 /
  https://github.com/hyperium/hyper/security/advisories/GHSA-q83h-524g-xf6h).
- **Prior-art check (triage cycles):** `gh pr list --repo getwyrd/wyrd --state open` → empty
  (no dependabot PR pending for this). `git log --oneline -- Cargo.lock` shows the usual
  dependabot bump cadence (`aws-smithy-types-1.6.2`, `async-trait-0.1.92` at `da9892d` /
  `6450101`), so a lockfile-only PR is the established shape for this change. No closed or
  rejected attempt exists.
- **Surfaces:** data
- **Difficulty:** low
- **Disposition hint:** likely-fix
<!-- pdca:end child-1 -->

<!-- pdca:child child-2 -->
- **Slug:** validate-crate-skeleton-and-cli-surface
- **Kind:** enhancement
- **Defect:** (framed as the gap) There is no `crates/validate`, so there is nowhere to put
  the blackbox validator proposal 0017 specifies — `ls crates/validate` on `main` @ `65ca4fd`
  → no such directory, and `git log --oneline -- crates/validate` is empty (it has never
  existed). Every later slice in milestone 17 (#741 the S3 client, #742 packaging, #743 the
  capability matrix) presumes the crate, its binary, and the argument surface an operator
  points at their own cluster.
- **Success criterion:** BINDING, every leg observable inside `cargo xtask ci`:
  1. `crates/validate` is a member of the root `[workspace] members`, the workspace builds,
     and `cargo xtask ci` exits 0. Registration is not bookkeeping:
     `unregistered_manifests` (`xtask/src/repo_guard.rs:421`) already fails the gate on a
     package under `crates/` that is not a member.
  2. **The CLI surface is bound flag by flag, not by sample.** `wyrd-validate` invoked with
     ALL TEN of `--endpoint --region --bucket --scenario --duration --workers --seed --out
     --run-id --driver-placement` exits 0 and echoes a resolved-configuration block in which
     **each** flag appears with the value it was given — asserted per flag, so an
     implementation that parses three and ignores seven FAILS. A missing required argument
     exits non-zero naming the offending flag on stderr.
  3. **Credential resolution is bound in all four directions**, over an injected lookup so no
     process env is mutated (process env is shared across parallel test threads and flakes):
     AWS pair present → the resolved id is the AWS one and the reported source says so;
     AWS absent and `WYRD_S3_ACCESS_KEY`/`WYRD_S3_SECRET_KEY` present → the Wyrd one, source
     says so; BOTH present → AWS wins; NEITHER → exit non-zero naming what to set. In every
     case the echoed block contains the access-key **id** and never the secret — asserted by
     searching the whole output for the secret's value.
  4. **Strict argument rejection, both halves** (see the DECISION note below): an
     unrecognised `--flag` exits non-zero naming it, AND a `--`-prefixed token appearing in a
     **value slot** exits non-zero naming both flags. The second half is explicit because it
     is exactly what the peer parser gets wrong and what cost iteration v1: `ParsedArgs::parse`
     takes `args[i+1]` as the value verbatim (`crates/server/src/cli.rs:2512-2515`), so
     `--bucket --typo` silently sets bucket to `"--typo"`.
- **SCOPE DECISION — settled by the maintainer, 2026-08-18. Not Do's to revisit.** Criterion
  4 is scope the tracker text does not name; the Plan reviewer flagged it as such, and it was
  put to the maintainer explicitly. **Decision: STRICT — criterion 4 is IN.** The reasoning
  on record: this binary is NEW, so a strict policy breaks no existing invocation and
  grandfathers nothing, whereas making `wyrd` itself strict later WOULD break existing
  scripts — so the two parsers will not converge anyway, and the new one should be the
  correct one. The failure mode decides it: a mistyped `--duration` on a run whose 7-day
  endurance leg gates the 0.1 Alpha tag does not crash, it returns a believable green over a
  two-minute run. Do MUST implement BOTH halves — the unrecognised-flag case and the
  flag-shaped-token-in-a-value-slot case. The second is called out because it is precisely
  what iteration v1 shipped without and a review pass then rejected.
  **Also settled: the encoding cases are OUT.** Non-UTF-8 arguments and
  control-characters-in-output were added mid-iteration on reviewer speculation, found no
  defect, and were a large share of what inflated v3's `cli_surface.rs` to 863 lines. Do MUST
  NOT add them back; if they ever matter they are their own ticket.
- **Falsifiability:** Criteria 2–4 go red pre-fix trivially — there is no binary. This is
  net-new coverage, so the honest red is criterion-ABSENCE; see **Verification posture**.
  Producible on the ordinary developer harness — no topology, no service, no network.
  KNOWN GATE SHAPE, pre-declared: this instance's `C4-verify` classifies on an **added**
  `*/tests/*.rs` file, and because this patch CREATES the crate that file lives in, the gate
  takes its `GREEN_ONLY` branch and records `PASS (green-only)`
  (`run-verify.sh:412`, `:497-498`). **Confirmed, not assumed** — dry-run of
  `run-verify.sh --classify` on a synthetic patch of this child's file set returns
  `ADDED_TEST crates/validate/tests/cli_surface.rs` / `CRATE crates/validate`. That is
  correct for a net-new crate; the binding gate is `C4-ci`. Do should not contort the patch
  chasing a red leg from that row.
- **Invariant to restore:** *An operator-facing binary never silently discards an argument it
  was given.* Stated over the category, not over one flag: every declared flag is either
  resolved into the echoed configuration or refused by name, and every token the parser does
  not understand is refused rather than absorbed. Source: the credential half is the repo's
  own established contract — `crates/server/src/cli.rs:2128-2136` refuses rather than
  defaulting ("`--access-key` (or `WYRD_S3_ACCESS_KEY`) is required; there is no anonymous
  access"). Proposal 0017 §2 supplies the layering the criterion rests on: decisions pure and
  Check-tested, the runner owning "only the I/O".
- **Repo + branch target:** getwyrd/wyrd @ main
- **Reproduction:** n/a (new functionality). On `main` @ `65ca4fd`: `ls crates/validate` →
  no such directory; `grep -n validate Cargo.toml` → no member entry.
- **Scope:** (a) new workspace member `crates/validate` — package `wyrd-validate`, a **lib**
  target holding the pure decisions (so integration tests can reach them) and a thin
  `[[bin]] wyrd-validate` over it, **both** crate roots carrying `#![forbid(unsafe_code)]`
  (the existing guard scans every target kind including bins, `repo_guard.rs:380-386`, so a
  missing attribute on either root fails `cargo xtask ci` immediately); (b) the ten-flag
  argument surface, parsed, validated for presence, and echoed as a resolved-configuration
  block; (c) credential resolution AWS → `WYRD_S3_*` → refuse, over an **injected** lookup
  closure rather than reading `std::env` directly; (d) registration in the root
  `[workspace] members`; (e) strict argument rejection per criterion 4.
  **/ out of scope:** the dependency-closure guard (that is child-3 — do not touch
  `xtask/**`); any S3 call whatsoever, and any `aws-sdk-s3` dependency (#741 — this binary
  parses, echoes and exits); the capability matrix and `smoke` (#743); scenarios, oracle,
  pools, verdict; tarball packaging (#742); **ANY new third-party crate.**
- **On the no-new-dependency rule:** the argument surface is hand-rolled in the shape of
  `ParsedArgs` (`crates/server/src/cli.rs:2495-2532`). This is a deliberate scope decision,
  not laziness: making `wyrd-validate` a shipped workspace member is already the change that
  brings `aws-sdk-s3` and its transitive tree inside `deny.toml`'s frame in the NEXT slice
  (#741), and that is a declared human-only ADR-0003 §2 three-test audit + allowlist
  decision. Keeping THIS child at zero new crates means the crate lands with no license
  decision attached, and #741 carries exactly one dependency question instead of two. If a
  crate seems unavoidable, Do must STOP and declare it rather than adding it (a human-only
  item per INTEGRATION §4). `clap` in particular is rejected for this child on those grounds
  — revisit as a separate, argued change if the surface outgrows hand-rolling.
- **External dependencies:** none
- **Test file:** `crates/validate/tests/cli_surface.rs` — a NEW file.
- **Verification posture:** DECLARED, not the default. This is net-new coverage: there is no
  prior failing assertion to flip, so "red" is criterion-ABSENCE and `C4-verify` records
  `PASS (green-only)` for the pre-declared reason under **Falsifiability**. What is
  nonetheless BUILT and EXERCISED at Check: the crate compiles, its binary runs, and every
  criterion above executes as a real test under `cargo xtask ci` — the one gating row.
  Nothing is deferred to a later environment.
- **Citations expected:** Do must cite `path:line` on `main` for every change. Peer callsites
  Do MAY open (a narrow, deliberate exception to reading the brief only), re-verified on
  `65ca4fd`:
  * **Hand-rolled argument parsing** — `crates/server/src/cli.rs:2495-2532`
    (`ParsedArgs` / `parse` / `flag` / `positional`). Mirror the SHAPE and the "a flag needs
    a value" error wording so the two binaries feel like one product. Do **not** depend on
    `wyrd-server` to get it — that is the very edge child-3's lint forbids. Do **not** mirror
    its value-slot behaviour (`:2512-2515`); criterion 4 deliberately departs from it.
  * **The credential fallback pair and the refuse-don't-default posture** —
    `crates/server/src/cli.rs:2128` and `:2135` (`WYRD_S3_ACCESS_KEY` /
    `WYRD_S3_SECRET_KEY`, each behind an `.or_else`, each with a named error).
  * **Injected environment lookup, so the fallback is unit-testable without mutating process
    env** — `xtask/src/main.rs:1519` (`run_ci_steps(&mut |name|
    std::env::var_os(name).is_some(), …)`), the repo's stated "pure decisions, injected I/O"
    convention (proposal 0017 §2 cites `xtask/src/consistency_run_runner.rs:8-12`).
  * **Why workspace registration is load-bearing** — `xtask/src/repo_guard.rs:421`
    (`unregistered_manifests`).
- **Prior-art check (triage cycles):** searched by affected path on `main` @ `65ca4fd`.
  `git log --oneline -- crates/validate` → empty (never existed). `gh pr list --search
  "wyrd-validate"` and `--search "blackbox"` across all states → only PR #765, the merged
  proposal 0017 document. No closed/rejected attempt at this work exists.
- **Surfaces:** data
- **Difficulty:** medium
- **Depends on:** child-1
- **Disposition hint:** new-feature
<!-- pdca:end child-2 -->

<!-- pdca:child child-3 -->
- **Slug:** xtask-blackbox-dependency-closure-guard
- **Kind:** enhancement
- **Defect:** (framed as the gap) Nothing mechanically prevents `wyrd-validate` from linking
  a Wyrd workspace crate. `xtask/src/repo_guard.rs` carries exactly two invariants on `main`
  @ `65ca4fd` — the stray-gitlink scan (`scan_gitlinks`, `:238`) and the
  `#![forbid(unsafe_code)]` crate-root scan (`scan_roots`, `:500`); `grep -n "wyrd-"
  xtask/src/repo_guard.rs` shows the file never mentions a dependency closure. The moment the
  validator's binary links a `wyrd-*` crate it is testing Wyrd's types against Wyrd's types
  and its verdict is self-referential — the property the whole tool rests on, held by nothing
  but habit.
- **Success criterion:** BINDING, every leg inside `cargo xtask ci`:
  1. **The guard RUNS INSIDE the gate — asserted, not assumed.** Its registration lives in
     the `xtask` **lib** target as data, and a test asserts the blackbox guard is in the list
     `run_ci` executes. Without this, a guard that is defined, tested and never called passes
     every other criterion here. The repo has already engineered against exactly this hazard:
     the feature-gated check list "lives in `xtask::feature_gated_checks` (the lib target) so
     `xtask/tests/fdb_harness.rs` can assert its content directly"
     (`xtask/src/main.rs:1504-1505`), and `run_ci_steps` injects `exec` "so the real wiring is
     exercised without spawning cargo" (`:1486-1498`). Note the shape of the problem:
     `run_gitlink_guard` (`main.rs:1373`) and `run_unsafe_forbid_guard` (`main.rs:1443`) are
     `fn`s in the BINARY target, so no integration test can see whether `run_ci` calls them.
     Do not extend that.
  2. **The guard is flippable, not vacuous.** Fed a synthetic `cargo metadata` document, the
     pure scan function returns: exactly one violation naming the crate when `wyrd-validate`
     has a **normal** dependency on a `wyrd-*` crate; **none** when that same edge is marked
     `"kind": "dev"`; the violation when the `wyrd-*` edge is a normal dependency that is
     **`optional` and off by default**; and the violation when the `wyrd-*` crate is reached
     **transitively**, through at least one intermediate hop, with the violation naming the
     path rather than only the crate. Run over the REAL workspace metadata it returns none.
  3. **The guard fails closed.** An unparsable document, an absent `resolve` section, a
     `package` that appears in no node, or a reached package record whose identity cannot be
     decoded (missing `id` **or** missing `name`) is an `Err` — never a vacuously clean pass,
     and never an opaque-package-id fallback that lets the `wyrd-` prefix check silently miss.
     This is the exact defect three independent review passes found in iteration v3
     (`repo_guard.rs:729`, `:730`, `:751`) and it is promoted to a binding criterion here so
     it is built rather than rediscovered. Mirror `scan_roots`' stated posture of "refusing to
     pass a workspace it cannot see" (`repo_guard.rs:505-510`).
- **Falsifiability:** This child earns a **genuine per-fix red→green**, unlike its sibling —
  confirmed by dry-running `run-verify.sh --classify` on a synthetic patch of this file set,
  which returns `ADDED_TEST xtask/tests/blackbox_dependency_guard.rs` / `CRATE xtask`. `xtask`
  exists on the base, so `GREEN_ONLY` is NOT set: the gate reverts the production change,
  keeps the added test file, and the test must fail. Every red in criteria 2 and 3 is a
  planted input on the ordinary developer harness — synthetic metadata documents, so
  demonstrating the red never requires committing the accident. No topology, no service.
- **Invariant to restore:** *Nothing that ships inside the `wyrd-validate` binary may reach a
  Wyrd workspace crate — and that must be enforced by the gate, not by a doc comment.* Stated
  over the category (the package's whole **normal** dependency closure, transitively AND
  under every feature, optional off-by-default edges included), not over one edge: a guard
  that checked only direct dependencies would pass a validator reaching `wyrd-core` through
  one hop, and a guard that resolved only DEFAULT features would pass one reaching it behind
  an optional feature. Source: proposal 0017 §9
  (`docs/design/proposals/draft/0017-blackbox-validation-tool.md:559-575` — "the **normal**
  dependency closure of its binary target must contain **no** `wyrd-*` crate … Normal, not
  total: `cargo metadata` walks dev-dependencies too, and the §14 fixtures are dev-only"),
  and the repo's own "held by habit until it was load-bearing" rule at
  `xtask/src/main.rs:1438-1442`.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Reproduction:** On `main` @ `65ca4fd`: `grep -n "wyrd-" xtask/src/repo_guard.rs` → the
  file never mentions a dependency closure; `cargo xtask ci` is green, i.e. today's gate is
  satisfied by a workspace that has no way to violate the property.
- **Scope:** a third invariant in `xtask/src/repo_guard.rs` — the `wyrd-validate` package's
  normal dependency closure, plus its declared normal (including optional) dependencies,
  contain no `wyrd-*` crate — wired into `run_ci` through a lib-side registration a test can
  read, with its flippable test.
  **/ out of scope:** anything under `crates/validate/**` (that is child-2 — this child adds
  no crate code and no test there); extending the guard to any package other than
  `wyrd-validate`; changing the two existing invariants' behaviour (refactoring their
  registration into the shared lib-side list is in scope and expected, but their verdicts must
  not change); constraining dev-dependencies, which are deliberately unconstrained — #741 and
  proposal 0017 §14's fixtures rely on that.
- **Design constraints Do must honour** (why the obvious implementation is wrong):
  * **Resolve with the graph, and with `--all-features`.** The existing unsafe-forbid guard
    runs `cargo metadata --no-deps`, which omits the resolve graph. This guard needs the
    graph, so it runs `cargo metadata --format-version 1 --locked --all-features` WITHOUT
    `--no-deps` and walks `resolve.nodes`. `--locked` matters: a metadata call without it can
    rewrite `Cargo.lock`, and a guard that mutates the tree it audits is its own defect.
    `--all-features` matters because the resolve graph is FEATURE-DEPENDENT and this repo
    already uses the pattern that hides an edge from it —
    `crates/metadata-tikv/Cargo.toml:11-27` declares optional normal dependencies activated
    only by an off-by-default feature, and `xtask/src/lib.rs:40-47` records that the default
    workspace commands do not cover non-default features at all.
  * **Normal, not total.** Each node's `deps[].dep_kinds[].kind` is `null` for a normal edge
    and `"dev"` for a dev edge. Follow only `kind: null` edges transitively from the
    `wyrd-validate` node, and report every reached package whose name begins with `wyrd-`.
  * **Belt AND braces.** Additionally scan the package's **declared** dependency list
    (`packages[].dependencies`), which is feature-INDEPENDENT and carries `kind` and
    `optional` per entry, and report any `wyrd-*` entry whose `kind` is `null`, optional or
    not. This catches a declaration even if some future feature-resolution subtlety keeps it
    out of the graph, and it is what makes criterion 2's optional case meaningful. Report both
    classes as violations of the same invariant, with wording that says which one fired.
  * **Pure function in, violations out** — the same function `cargo xtask ci` runs is the one
    the test drives over planted documents.
  * `--all-features` widens the resolve only; it compiles nothing, so the guard stays
    sub-second and needs neither the TiKV nor the FoundationDB toolchain the corresponding
    *builds* would (verified offline against this checkout: exit 0, no build).
- **External dependencies:** none
- **Test file:** `xtask/tests/blackbox_dependency_guard.rs` — a NEW file.
- **Citations expected:** Do must cite `path:line` on `main` for every change. Peer callsites
  Do MAY open (a narrow, deliberate exception to reading the brief only), re-verified on
  `65ca4fd`:
  * **The guard's shape** — `xtask/src/repo_guard.rs:387` (`target_src_paths`: a pure
    function over `cargo metadata` JSON returning `Result<_, String>`, failing closed on a
    document it cannot parse) and `xtask/src/main.rs:1443-1484` (`run_unsafe_forbid_guard`:
    `print_step`, shell out to `cargo metadata`, hand the JSON to the pure function, join
    violations into one error). Call the new guard from `run_ci` beside it, `:1557-1558`.
  * **How this repo makes a `run_ci` call site TEST-VISIBLE** — `xtask/src/main.rs:1486-1544`
    and its doc comment at `:1504-1505`; the list itself is `xtask/src/lib.rs:81`
    (`feature_gated_checks`). This is the model for criterion 1.
  * **The flippable-test idiom** — `xtask/tests/repo_hygiene_guards.rs:35`
    (`scan_gitlinks_is_red_on_an_undeclared_gitlink`: synthetic input, assert exactly one
    violation, assert it names the offender) and `:385`
    (`scan_crate_roots_is_green_over_the_real_workspace_crates`).
  * **Why the guard's subject must already exist** — `xtask/src/repo_guard.rs:421`
    (`unregistered_manifests`): a package under `crates/` that is not a workspace member never
    reaches metadata. The fail-closed branch of criterion 3 must not turn a missing package
    into a pass, and the guard must not become satisfiable by removing `crates/validate` from
    `[workspace] members`.
- **Prior-art check (triage cycles):** searched by affected path on `main` @ `65ca4fd`.
  `git log --oneline -- xtask/src/repo_guard.rs` → six #616 commits, all on the gitlink and
  unsafe-forbid scans; none touches a dependency closure. `gh pr list --search "blackbox"`
  across all states → only PR #765, the merged proposal 0017 document. No closed/rejected
  attempt exists.
- **Surfaces:** data
- **Difficulty:** medium
- **Depends on:** child-2
- **Disposition hint:** new-feature
<!-- pdca:end child-3 -->
