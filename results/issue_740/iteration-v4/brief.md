# Brief — issue 740 / validate-crate-skeleton-and-blackbox-lint

> Plan artifact (docs 02 §PLAN). Do reads ONLY this file (plus the peer callsites cited
> under **Citations expected**). The `- **Label:** value` lines are parsed by the driver.
>
> Plan of record: `docs/design/proposals/draft/0017-blackbox-validation-tool.md` §2
> (layering) and §9 (the blackbox property, as a lint). Read in place in the target
> checkout — never copied here.
>
> **RE-PLAN (attempt 4). SPLIT AUTHORED AND ACCEPTED.** Three Do rounds produced
> 47 KB → 72 KB → 117 KB patches against a 100 KB threshold; sign-off returned the bundle
> to Plan on the size backstop. The re-plan's finding is that this slice is separable along
> a zero-overlap file seam — see **Sizing note**. The split was accepted on 2026-08-18:
> `close-disposition = split`, and `split-lineage.json` records the children as
> **#773 (h2 lockfile bump) → #774 (the crate) → #775 (the guard)**, each with its own
> materialised bundle and brief. **The children carry the binding criteria; this parent
> brief is the record of the decomposition and never itself reaches Do.**
>
> **Base re-anchored (plan-review finding 6).** Every claim below was re-verified against
> the resolved execution base **`origin/main` = `a801997`** (was `65ca4fd`, now an
> ancestor — 4 commits behind). Of the paths this brief cites, only `Cargo.lock` changed,
> by one line (a `wyrd-testkit` dev-dependency added to a chunkstore crate); every cited
> `path:line` in `xtask/**` and `crates/server/src/cli.rs` is byte-identical at both
> revisions, and `h2 0.4.15` is still locked. Citations below are stated at `a801997`.

- **Slug:** validate-crate-skeleton-and-blackbox-lint
- **Kind:** enhancement
- **Goal:** Wyrd has no place to put a blackbox validator, and no mechanism that keeps one
  blackbox. Create the workspace member `crates/validate` (package + binary
  `wyrd-validate`), give it the argument surface every later slice parses, and land the
  `xtask` invariant that its **normal** dependency closure contains no `wyrd-*` crate.
- **Defect:** (framed as the gap) There is no `crates/validate`, and no dependency-closure
  guard. `xtask/src/repo_guard.rs` carries exactly two invariants today — the stray-gitlink
  scan (`scan_gitlinks`, `repo_guard.rs:238`) and the `#![forbid(unsafe_code)]` crate-root
  scan (`scan_roots`, `repo_guard.rs:500`) — re-verified on `main` at `a801997`. Nothing
  prevents a later slice from linking `wyrd-core` into the validator and destroying the only
  property that makes its verdict mean anything.
- **Success criterion:** NOT binding here — carried in full by the three accepted children,
  which are where Check evaluates it. This parent's own criterion is met by the split having
  been authored and accepted. In outline, and each verified present in the child's brief:
  1. **#773** — `cargo deny check advisories` passes on a `main` that today fails it
     (RUSTSEC-2026-0258), by a `Cargo.lock`-only bump; the gate is its own oracle.
  2. **#774** — `crates/validate` is a workspace member, `cargo xtask ci` is green,
     `wyrd-validate` binds all ten flags *flag by flag, not by sample*, credential
     resolution is bound in all four directions, **and argument rejection is strict in both
     halves** — unknown `--flag`, and a `--`-prefixed token in a value slot (`brief.md`
     criterion 4, with the maintainer's STRICT scope decision recorded on it).
  3. **#775** — a third `repo_guard` invariant **registered as data in the `xtask` lib** and
     asserted to be in the list `run_ci` executes; flippable over planted `cargo metadata`
     documents across **normal-red, dev-only-GREEN, optional-off-by-default-red and
     transitive-red**; and fail-closed on a document it cannot fully decode.
- **Falsifiability:** Split-dependent, and the split is what makes half of it possible.
  Verified against this instance's own gate by dry-running its classifier
  (`engine/scripts/run-verify.sh --classify`) on a synthetic patch per child:
  * **#774** — a patch that CREATES `crates/validate` classifies `CRATE crates/validate`,
    which does not exist on base, so `run-verify.sh` sets `GREEN_ONLY=1`
    (`run-verify.sh:412-414`) and records `PASS (green-only)` (`:499-503`). The crate child
    can never earn a per-fix RED from that row; its red is criterion-absence, and `C4-ci` is
    the gate that binds it.
  * **#775** — a guard-only patch classifies `ADDED_TEST
    xtask/tests/blackbox_dependency_guard.rs` / `CRATE xtask`, which DOES exist on base, so
    the gate reverts the production change, keeps the added test, and demands a genuine
    red→green. **The guard child is falsifiable in a way the combined slice was not** — that
    is an argument for the seam, not just a consequence of it.
  * **#773** — needs no constructed red: the base itself is red (`cargo deny check
    advisories` fails on clean `main`), and the bump turns it green.
  No topology, no service, no network: every red is a planted input on the ordinary
  developer harness Do is pointed at.
- **Invariant to restore:** *Nothing that ships inside the `wyrd-validate` binary may reach
  a Wyrd workspace crate — and that must be enforced by the gate, not by a doc comment.*
  Stated over the category (the package's whole normal dependency closure, transitively AND
  under every feature — optional, off-by-default edges included), not over one edge: a guard
  that checked only direct dependencies would pass a validator reaching `wyrd-core` through
  one hop, and a guard that resolved only DEFAULT features would pass one reaching it behind
  an optional feature. Source: proposal 0017 §9 (`0017-blackbox-validation-tool.md:559-575`
  — "the **normal** dependency closure of its binary target must contain **no** `wyrd-*`
  crate … Normal, not total"), and the repo's own "held by habit until it was load-bearing"
  rule recorded at `xtask/src/main.rs:1438-1442`.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Conflicts with:**
- **Ordering note:** A strict chain, one child per wave: **#773 → #774 → #775 → #741 → #742**.
  **The downstream re-point is DONE, not pending** (plan-review finding 2): #741 now reads
  `Depends on: 775` and #742 reads `Depends on: 775, 741`, each annotated "RE-POINTED
  2026-08-18 (maintainer-approved)" in its own brief. This closes the strand risk — a split
  parent never reaches COMPLETE, so the surviving `Depends on: 740` would have made
  `_runnable` (`flow.py:702`) skip both with "prerequisite(s) not ready (740)". No
  `Conflicts with` edges among the children: their file sets are disjoint (`Cargo.lock`;
  `crates/validate/**` + root `Cargo.toml`; `xtask/**`), and #773/#774 both touch
  `Cargo.lock` but sit in different waves, so ordering is settled by `Depends on`.
- **Surfaces:** data
- **Difficulty:** high
- **Scope:** (a) new workspace member `crates/validate` — package `wyrd-validate`, a lib
  target holding the pure decisions and a thin `[[bin]] wyrd-validate` over it, both crate
  roots carrying `#![forbid(unsafe_code)]`; (b) the argument surface `--endpoint --region
  --bucket --scenario --duration --workers --seed --out --run-id --driver-placement`,
  parsed, validated for presence, and echoed as a resolved-configuration block; (c)
  credential resolution from `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY`, falling back to
  `WYRD_S3_ACCESS_KEY` / `WYRD_S3_SECRET_KEY`, over an injected lookup; (d) a third
  invariant in `xtask/src/repo_guard.rs` — the package's normal dependency closure contains
  no `wyrd-*` crate — wired into `run_ci` through a lib-side registration a test can read;
  (e) its flippable test.
  **/ out of scope:** any S3 call whatsoever (#741); the capability matrix and `smoke`
  (#743); scenarios, oracle, pools, verdict; ANY new third-party crate (the arg parsing is
  hand-rolled precisely so this lands without an ADR-0003 dependency audit); tarball
  packaging (#742); extending the guard to any package other than `wyrd-validate`;
  **the `h2` / RUSTSEC-2026-0258 lockfile bump** (see External dependencies — base-owned,
  its own slice).
- **Repro instruction:** Re-run on `main` at `a801997` (finding 6), both legs confirmed:
  `ls crates/validate` → no such directory (`git ls-tree -d a801997 -- crates/validate` is
  empty; the path has never existed); `grep -n "wyrd-" xtask/src/repo_guard.rs` → no match,
  the file never mentions a dependency closure.
- **External dependencies:** none — the base Rust toolchain only. The guard shells out to
  cargo's own metadata subcommand, exactly as the existing one already does
  (`xtask/src/main.rs:1451-1456`). Do MUST NOT add a third-party crate to `crates/validate`
  in this slice; if one seems unavoidable, stop and declare it (ADR-0003 §2 three-test audit
  + `deny.toml` allowlist — a human-only item per INTEGRATION §4).
  **BASE-RED, AND NOW OWNED BY A DECLARED PREREQUISITE — issue #773** (plan-review
  finding 3). `cargo deny check advisories` FAILS on a clean `main` — RUSTSEC-2026-0258
  (`h2 0.4.15`, patched `>= 0.4.16`, advisory dated 2026-08-17). Re-verified at `a801997`:
  `Cargo.lock` still pins `h2 0.4.15` (`Cargo.lock:1535-1537`), and `cargo_deny_check()`
  still runs unconditionally inside `run_ci` (`xtask/src/main.rs:1563`, called from
  `run_ci` at `:1548`). `C4-ci` is the one **gating** row, so the base is red before any
  patch applies. The earlier version of this brief named that blocker without assigning it,
  which left the criterion knowingly un-evaluable; the split fixes it by making the bump
  **child #773**, and **#774 declares `Depends on: 773`** so the crate child builds on a
  genuinely green `main` (under `wave_mode = "merge"` + `auto_merge = true` the driver
  folds each non-final wave). The bump stays out of #774/#775's patches — carrying it there
  would make `Cargo.lock` a conflict point across the batch.
- **Test file:** carried by the children — `crates/validate/tests/cli_surface.rs` (**#774**)
  and `xtask/tests/blackbox_dependency_guard.rs` (**#775**), both NEW files. **#773 ships no
  test file, deliberately** — it is a lockfile-only bump whose oracle is `cargo deny` itself;
  its brief instructs Do not to invent one. Classification of each against this instance's C4
  gate is recorded under **Falsifiability**.
- **Verification posture:** DECLARED, not the default, and split-dependent. **#774** is
  net-new coverage in a crate the patch creates, so its C4-verify row is `PASS (green-only)`
  by construction and `C4-ci` is what binds it. **#775** earns a real per-fix red→green.
  **#773** has no test by design; `cargo deny` is its oracle and the base supplies the red.
  Everything in all three children is BUILT and EXERCISED at Check — the crate compiles and
  its binary runs under `cargo xtask ci`, the guard runs inside `run_ci`, and both test files
  execute there. Nothing is deferred to a later environment.
- **Citations expected:** Do must cite `path:line` on `main` for every change. Peer
  callsites, **re-verified on `a801997`** (a deliberate, narrow exception to reading this
  brief only) — every line number below was re-read at that revision and is unchanged from
  `65ca4fd`. The children restate the ones each needs:
  * **The guard's shape** — `xtask/src/repo_guard.rs:387` (`target_src_paths`: a pure
    function over `cargo metadata` JSON returning `Result<_, String>`, failing closed on a
    document it cannot parse) and `xtask/src/main.rs:1443-1484` (`run_unsafe_forbid_guard`:
    `print_step`, shell out to `cargo metadata`, hand the JSON to the pure function, join
    violations into one error). Call the new guard from `run_ci` beside it
    (`xtask/src/main.rs:1557-1558`).
  * **How this repo makes a `run_ci` call site TEST-VISIBLE** — `xtask/src/main.rs:1486-1544`
    (`run_ci_steps`, whose `exec` and `toolchain` are injected "so the real wiring is
    exercised without spawning cargo", and whose row list "lives in
    `xtask::feature_gated_checks` (the lib target) so `xtask/tests/fdb_harness.rs` can assert
    its content directly", `:1504-1505`; the function itself is `xtask/src/lib.rs:81`). Note
    the shape of the problem: `run_gitlink_guard` (`main.rs:1373`) and
    `run_unsafe_forbid_guard` (`main.rs:1443`) are `fn`s in the BINARY target, so no
    integration test can see whether `run_ci` calls them. Do not extend that.
  * **The flippable-test idiom** — `xtask/tests/repo_hygiene_guards.rs:35`
    (`scan_gitlinks_is_red_on_an_undeclared_gitlink`: synthetic input, assert exactly one
    violation, assert it names the offender) and `:385`
    (`scan_crate_roots_is_green_over_the_real_workspace_crates`).
  * **Hand-rolled argument parsing** — `crates/server/src/cli.rs:2495-2532`
    (`ParsedArgs::parse` / `flag` / `positional`). Mirror the SHAPE; do **not** depend on
    `wyrd-server` to get it (that is the very edge the new lint forbids).
  * **The credential fallback pair** — `crates/server/src/cli.rs:2128` / `:2135`
    (`WYRD_S3_ACCESS_KEY` / `WYRD_S3_SECRET_KEY`, each behind an `.or_else`).
- **Prior-art check (triage cycles):** searched by affected path, re-run on `main` at
  `a801997` (finding 6) — same results as at `65ca4fd`; none of the searched paths moved.
  `git log --oneline -- crates/validate` → empty (the directory has never existed).
  `git log --oneline -- xtask/src/repo_guard.rs` → six #616 commits, all on the gitlink and
  unsafe-forbid scans; none touches a dependency closure. `gh pr list --search
  "wyrd-validate"` / `--search "blackbox"` across all states → only PR #765 (the merged
  proposal 0017 document). `gh pr list --state open` → empty. No closed/rejected attempt at
  this work exists.
- **Disposition hint:** new-feature

## Sizing note — why this is split, and where

Three Do rounds, each larger than the last (47 KB → 72 KB → 117 KB against a 100 KB
threshold; `iteration-v{1,2,3}/size-signal.json`). The v3 patch divides in half along a
**zero-overlap file boundary**:

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

Three children, not more (each costs a full cycle) — as accepted, with real tracker ids:

* **#773 — the h2 bump**: `Cargo.lock` only. Difficulty low. Not a code dependency of the
  others but a *gate* dependency: `C4-ci` is red on the base until it lands, so a sibling
  built alongside it would fail its gate for a reason unrelated to its own patch.
* **#774 — the crate**: `crates/validate/**` + root manifest. Difficulty medium.
  `Depends on: 773`.
* **#775 — the guard**: `xtask/**`. Difficulty medium. `Depends on: 774` — the guard's
  subject package must exist, and its own fail-closed requirement turns an absent package
  into an `Err`, so #775 on a base without #774 is red by construction.

### The tracker's atomicity constraint — WAIVED with mitigation, not preserved

The tracker asks for the opposite of this split: "Two outcomes, deliberately kept together
(`watch`, not split) … landing it with the crate is what makes the boundary exist from
commit one." **An earlier draft of this brief claimed the ordering "preserves the tracker's
stated intent". That claim was wrong and is withdrawn** (plan-review finding 1). Splitting
#774 from #775 does exactly what the tracker warned against: for the window between them,
validator code exists without the mechanical boundary. This is a deliberate **waiver**, and
it is honest to call it one.

What makes the waiver acceptable — the gap is *inert*, not merely short:

1. **Nothing can violate the boundary in the gap.** #774's scope forbids **ANY new
   third-party crate** (the arg parsing is hand-rolled precisely so this lands without an
   ADR-0003 dependency audit), and it obviously adds no `wyrd-*` edge. The crate that exists
   in the gap has an empty dependency set, so there is no shortcut available to take.
2. **The first slice that adds any dependency is #741, and it is scheduled after #775.**
   The chain #774 → #775 → #741 means the guard is in place before the first dependency
   edge ever appears — which is the *substance* the tracker was protecting, even though it
   is not the *mechanism* the tracker named.
3. **The gap is one wave inside a single flow run**, not a release window: #775 declares
   `Depends on: 774` and the wave fold lands them consecutively in the same run.
4. **The split bought a falsifiability the atomic slice did not have.** Combined, the guard's
   red→green was unreachable — a patch creating `crates/validate` classifies as a new
   `CRATE`, forcing `GREEN_ONLY=1` for the whole bundle. Separated, #775 earns a genuine
   per-fix red. Keeping them together would have kept the boundary lint permanently
   unfalsifiable by the gate, which is a worse outcome for the very property the tracker
   cared about.

**Status:** the split was put to the maintainer and accepted on 2026-08-18 — recorded in
this bundle as `close-disposition = split` plus `split-lineage.json`, and corroborated by
the maintainer-approved re-point annotations carried in #741/#742. **The approval is
recorded in the bundle, not in the tracker thread** (`notes.json` has `"comments": []`, and
the issue body still reads `watch`, not split). Updating the tracker issue to record the
decomposition is an open housekeeping item for the maintainer.

## Motivation

The blackbox property is the premise of the whole tool. `wyrd-validate` exists to answer
"is Wyrd correct" from outside, the way a client does; the moment its binary links a
`wyrd-*` crate it is testing Wyrd's types against Wyrd's types and its verdict is
self-referential. Proposal 0017 §9 makes that a *mechanical* property rather than a
promise, and §2 says it is also why the tool must never become a `wyrd` subcommand — a
subcommand shares a dependency closure by construction.

## Plan-review response (revision pass, 2026-08-18)

Six NEEDS-HUMAN findings in `plan-advisory-plan-reviewer.md`. The reviewer read this parent
brief, which had gone **stale against the split that was already accepted** — it still
described two prose-labelled children (A/B) when three real ones (#773/#774/#775) had been
materialised. Most findings were therefore accurate against the text and already answered by
the artifacts; the text is now corrected to match. Disposition per finding:

- **Plan-review response (1 — atomicity waived, not preserved):** REVISED, finding upheld.
  The claim that the ordering "preserves the tracker's stated intent" was false and is
  withdrawn. The Sizing note now states the waiver plainly and gives the four grounds that
  make the gap inert (empty dependency set in the gap; #741 scheduled after #775; one wave
  inside one run; the split is what makes the guard falsifiable at all). Approval status is
  recorded precisely — accepted in-session, **not** on the tracker thread. *Remains the
  maintainer's call; see the closing note.*
- **Plan-review response (2 — unresolved child ids / strand risk):** REVISED, finding
  answered by artifacts that existed but were not cited. Children resolve to **#773, #774,
  #775** (`split-lineage.json`), each with a materialised bundle and a real `Depends on:`
  chain. The downstream re-point is **done, not pending**: #741 reads `Depends on: 775`,
  #742 reads `Depends on: 775, 741`, both annotated maintainer-approved. Strand risk closed.
- **Plan-review response (3 — binding criterion impossible on a red base):** REVISED,
  finding upheld and fixed by the split. Re-verified at `a801997`: `h2 0.4.15` still locked
  (`Cargo.lock:1535-1537`), `cargo_deny_check()` still unconditional in `run_ci`
  (`main.rs:1563`, `run_ci` at `:1548`). The bump is now **child #773**, and #774 declares
  `Depends on: 773`, so the blocker is a declared prerequisite with an owner rather than an
  admitted un-evaluable base.
- **Plan-review response (4 — guard red case under-binds the invariant):** NO CHANGE
  NEEDED; already bound in the child. #775's criterion 2 requires all four legs the reviewer
  asked for — normal-edge RED, `"kind": "dev"` **GREEN**, optional-off-by-default RED, and
  transitive RED naming the path — plus green over real workspace metadata, and criterion 3
  binds fail-closed decoding. The dev-only-green requirement is confirmed in the plan of
  record at `0017-blackbox-validation-tool.md:565-567` ("what the test harness links is
  unconstrained"). The outline in this parent has been corrected to reflect it.
- **Plan-review response (5 — CLI criterion misses the prior blocking defect):** NO CHANGE
  NEEDED; already bound in the child. #774's criterion 4 binds **both** halves — unknown
  `--flag`, and a `--`-prefixed token in a value slot — and carries the maintainer's STRICT
  scope decision. The reviewer's diagnosis of the peer parser is correct and independently
  re-verified: `ParsedArgs::parse` accepts any `--name` and takes `args[i+1]` verbatim
  (`crates/server/src/cli.rs:2505-2516`), so `--bucket --typo` sets bucket to `"--typo"`;
  that is why the child cites the shape as a hazard to fix, not a pattern to copy.
- **Plan-review response (6 — stale source grounding):** REVISED, finding upheld. The base
  had moved: `origin/main` = **`a801997`**, with `65ca4fd` 4 commits behind. Every cited
  anchor was re-read at `a801997` — `repo_guard.rs:238/387/500`, `main.rs:1373/1438-1442/
  1443/1504-1505/1548/1557-1558/1563`, `lib.rs:81`, `repo_hygiene_guards.rs:35/385`,
  `cli.rs:2128/2135/2495-2532`, proposal `0017:559-575` — **all unchanged**. Of the cited
  paths only `Cargo.lock` differs, by one unrelated line. Repro and prior-art re-run at
  `a801997` with identical results. *Carry-over:* the three child briefs still cite
  `65ca4fd` in their own text; the anchors are identical at both revisions so no claim is
  wrong, but re-anchoring their wording is a loose end this pass did not touch (out of its
  named scope).

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR
MAY happen during the cycle. The PR MUST NOT be marked ready before sign-off accepts.

## Iteration 4 — carry-forward (from the previous attempt)
- Sign-off rationale: Rationale: this parent closed as a split (children #741/#742 re-pointed, #773/#774/#775 materialized), but the plan-advisory review left 9 substantive NEEDS-HUMAN items that read as defects in the split itself, not routine confirmations. Send back to re-author the split rather than let every child inherit these: - Atomicity: the tracker wants the crate skeleton and its dependency-boundary guard landed together ("landing it with the crate is what makes the boundary exist from commit one"); the split orders crate-only child A before guard child B, leaving the validator crate unguarded in between. - Dangling dependency IDs: child B's "Depends on: A" is a prose label, not a resolvable bundle ID; #741/#742's re-point target is unnamed; no tracker record shows the re-point was approved. - Child A's binding success criterion (cargo xtask ci green) is already unreachable on this base (h2/RUSTSEC-2026-0258, the same pre-existing advisory as #771/#721) and the brief excludes fixing it and labels external dependencies "none" — success can't be distinguished from base failure as written. - Guard child's promised scope (transitive + optional/feature-gated closures) exceeds the tracker's stated definition of done (direct wyrd-core dependency only) without fixtures for the wider claim, and doesn't test the dev-only-stays-green inverse. - CLI child criterion doesn't bind the previously-recorded "unknown flag consumed in a value slot" defect, covers only 3 of 11 flags plus unspecified credential handling, and checks only the parser function rather than the cargo xtask ci wiring. - Source grounding is stale relative to the resolved target's actual origin/main HEAD. - Strict unknown-flag rejection is added scope not present in the tracker, and contradicts the peer CLI parser's existing permissive convention. - The dependency-closure guard's design won't see optional/off-by-default normal dependencies unless all features are enabled; a concrete counterexample already exists in the tree (crates/metadata-tikv). Human directive: iterate-plan for 740 — return to Plan to re-author the split.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Rationale: this parent closed as a split (children #741/#742 re-pointed, #773/#774/#775
  materialized), but the plan-advisory review left 9 substantive NEEDS-HUMAN items that read as
  defects in the split itself, not routine confirmations. Send back to re-author the split
  rather than let every child inherit these:
  - Atomicity: the tracker wants the crate skeleton and its dependency-boundary guard landed
    together ("landing it with the crate is what makes the boundary exist from commit one");
    the split orders crate-only child A before guard child B, leaving the validator crate
    unguarded in between.
  - Dangling dependency IDs: child B's "Depends on: A" is a prose label, not a resolvable
    bundle ID; #741/#742's re-point target is unnamed; no tracker record shows the re-point
    was approved.
  - Child A's binding success criterion (cargo xtask ci green) is already unreachable on this
    base (h2/RUSTSEC-2026-0258, the same pre-existing advisory as #771/#721) and the brief
    excludes fixing it and labels external dependencies "none" — success can't be
    distinguished from base failure as written.
  - Guard child's promised scope (transitive + optional/feature-gated closures) exceeds the
    tracker's stated definition of done (direct wyrd-core dependency only) without fixtures for
    the wider claim, and doesn't test the dev-only-stays-green inverse.
  - CLI child criterion doesn't bind the previously-recorded "unknown flag consumed in a value
    slot" defect, covers only 3 of 11 flags plus unspecified credential handling, and checks
    only the parser function rather than the cargo xtask ci wiring.
  - Source grounding is stale relative to the resolved target's actual origin/main HEAD.
  - Strict unknown-flag rejection is added scope not present in the tracker, and contradicts
    the peer CLI parser's existing permissive convention.
  - The dependency-closure guard's design won't see optional/off-by-default normal
    dependencies unless all features are enabled; a concrete counterexample already exists in
    the tree (crates/metadata-tikv).

  Human directive: iterate-plan for 740 — return to Plan to re-author the split.
- Full previous attempt preserved in `iteration-v4/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
