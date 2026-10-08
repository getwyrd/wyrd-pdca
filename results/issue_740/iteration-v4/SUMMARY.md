# Result — issue 740 / validate-crate-skeleton-and-blackbox-lint

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: (framed as the gap) There is no `crates/validate`, and no dependency-closure
  guard. `xtask/src/repo_guard.rs` carries exactly two invariants today — the stray-gitlink
  scan (`scan_gitlinks`, `repo_guard.rs:238`) and the `#![forbid(unsafe_code)]` crate-root
  scan (`scan_roots`, `repo_guard.rs:500`) — re-verified on `main` at `a801997`. Nothing
  prevents a later slice from linking `wyrd-core` into the validator and destroying the only
  property that makes its verdict mean anything.
- Success criterion: NOT binding here — carried in full by the three accepted children,
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
- Repo + branch target: getwyrd/wyrd @ main
- Scope: (a) new workspace member `crates/validate` — package `wyrd-validate`, a lib
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

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — N/A — close disposition (no patch to verify)
- C3 Change: none — patch.diff
- C4 Verification (red→green): none — N/A — close disposition (no patch to verify)
- C5 Causal adequacy: none — reviewer + human sign-off

## 4. Conformance (Check — stack)
- T1 Structure: none — N/A — close disposition (no patch to verify)
- T2 Shape: none — N/A — close disposition (no patch to verify)
- T3 Runtime: none — N/A — close disposition (no patch to verify)
- T4 Contribution: none — N/A — close disposition (no patch to verify)
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

# Advisory review — SKIPPED (close disposition)

The reviewer leaf was skipped: this bundle's Plan concluded a close / no-fix disposition (split), so there is no patch to review.

- NEEDS-HUMAN — Confirm the close disposition 'split' (no patch was built). Override to a fix path (iterate-to-Do) if the close is wrong.


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] Confirm the close disposition 'split' (no patch was built). Override to a fix path (iterate-to-Do) if the close is wrong.
- [ ] The split discards the tracker's load-bearing atomicity constraint. The tracker says both “Establish the crate and its boundary before there is code to violate it” and “Two outcomes, deliberately kept together (`watch`, not split) … landing it with the crate is what makes the boundary exist from commit one” (`notes.json`, `body`). The brief instead says it is “authored to be SPLIT” (`brief.md:10-14`) and orders crate-only child A before guard child B (`brief.md:160-169`). After A lands, validator code exists without the mechanical boundary, so the claimed preservation of the tracker's intent is false; the brief needs either an atomic landing strategy or explicit human approval to waive that constraint.
- [ ] The proposed dependency rewrite does not resolve to actionable bundle IDs, and the claimed approval is absent from the supplied tracker record. Child B declares `Depends on: A`, where `A` is only a prose label (`brief.md:160-165`), while #741/#742 are to be re-pointed at unspecified “children” (`brief.md:60-64`). `notes.json` has `"comments":[]` and still says `watch`, not split, so it does not support “Maintainer approved the re-point.” With no `dependency-state.json` or source record resolving A/B, the split can strand B, #741, and #742 exactly as the brief warns.
- [ ] The only binding success command is knowingly impossible on the selected base, but the required fix is neither in scope nor a declared prerequisite. Child A requires `cargo xtask ci` green (`brief.md:28-31`), then the brief says that gate is already red, excludes the needed lockfile update, and labels external dependencies “none” (`brief.md:85-97`). The target confirms `run_ci` unconditionally calls `cargo_deny_check()` (`xtask/src/main.rs:1546-1563`) and still locks `h2` 0.4.15 (`Cargo.lock:1534-1538`). Until a concrete prerequisite ID or advanced base is declared, C4 cannot distinguish either child's result from the admitted base failure.
- [ ] The guard's proposed red case does not falsify the invariant the brief promises. The tracker only asks that a deliberately added direct `wyrd-core` dependency turn red (`notes.json`, “Definition of done”), while the brief expands the promise to transitive and optional/off-by-default normal edges (`brief.md:48-56`) without naming separate fixtures for those cases. It also must prove the inverse: dev-only dependencies remain allowed, because the plan of record says “what the test harness links is unconstrained” (`docs/design/proposals/draft/0017-blackbox-validation-tool.md:565-567`). A direct-edge fixture can pass while a default-feature-only or total-closure implementation is wrong; bind transitive-red, optional-feature-red, and dev-only-green cases (plus the exact metadata invocation) in the child criterion.
- [ ] The crate-child criterion fails to bind the prior CLI defect that the brief itself calls blocking. The only stated check is that all ten happy-path flags are parsed and echoed (`brief.md:28-31`), even though the sizing history records “an unknown flag consumed in a value slot” (`brief.md:153-155`). The cited parser shape unconditionally accepts any `--name` and consumes the next token as its value (`crates/server/src/cli.rs:2505-2516`), so mirroring it can reproduce that failure while satisfying the happy-path criterion. Add deterministic negative cases for unknown flags and a value flag followed by another flag.
- [ ] The source grounding is stale relative to the supplied target. The brief repeatedly says its source claims were re-verified on `main` at `65ca4fd` (`brief.md:22-25`, `brief.md:107-109`), but the read-only target resolves `origin/main` to `a801997233f471ae5f8ed3415678a1e088c6da65`. Even where the cited line numbers currently happen to match, the plan does not establish that its repro, prior-art result, and base-red diagnosis apply to the actual execution base; re-run and record those checks against the resolved target revision.
- [ ] The proposed guard does not enforce the brief's claimed *whole* normal dependency closure across feature sets. The design runs `cargo metadata --format-version 1 --locked` and follows that resolve graph (`brief.md:189-198`), but never enables all features; a non-default feature can therefore hide an optional normal `wyrd-*` dependency from both the real-workspace check and the planted normal/dev fixture. This is a concrete pattern in the target: `crates/metadata-tikv/Cargo.toml:11-27` declares off-by-default optional normal dependencies, and `xtask/src/lib.rs:40-47` explicitly records that default workspace commands do not cover non-default features. Revise the guard/criterion to inspect every feature-enabled normal edge (or explicitly forbid optional `wyrd-*` declarations), otherwise the invariant promised at `brief.md:52-58` is false.
- [ ] The tracker requires that “the lint is green, and a deliberately added `wyrd-core` dependency turns it **red**” (`notes.json:1`), but the brief substitutes a unit test that feeds synthetic JSON directly to the pure scan function (`brief.md:28-31`, `brief.md:37-51`). That can pass while the production path fails to invoke the scanner, supplies different metadata, or discards its violations. The target calls out exactly this wiring hazard and uses injected execution so a wrong `run_ci` call site is test-visible (`xtask/src/main.rs:1486-1498`). Make the red criterion exercise the guard/`cargo xtask ci` wiring, not only its parser.
- [ ] The binding CLI criterion does not verify the CLI surface promised by the tracker. The issue requires eleven flags to be “parsed and echoed” plus AWS credentials falling back to the `WYRD_S3_*` pair (`notes.json:1`; repeated at `brief.md:70-76`), while the only stated behavior check supplies just `--endpoint`, `--bucket`, and `--scenario` with unspecified credentials “in the environment” (`brief.md:33-36`). An implementation that ignores the other eight flags and never implements the fallback can satisfy the criterion. Bind the criterion to each flag's resolved output and to both precedence/fallback credential cases.
- [ ] The brief adds an observable argument-contract change not present in the tracker: rejecting every unknown flag (`brief.md:170-174`). The tracker asks for the named surface to be parsed and echoed (`notes.json:1`), and the cited peer parser deliberately accepts arbitrary `--flag value` pairs (`crates/server/src/cli.rs:2501-2522`). Strict unknown-flag rejection therefore adds a separate compatibility policy and tests to this crate-plus-lint slice; remove it from this brief or have the human explicitly accept that extra scope.

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Plan
- Iteration delta (if iterating): Rationale: this parent closed as a split (children #741/#742 re-pointed, #773/#774/#775 materialized), but the plan-advisory review left 9 substantive NEEDS-HUMAN items that read as defects in the split itself, not routine confirmations. Send back to re-author the split rather than let every child inherit these: - Atomicity: the tracker wants the crate skeleton and its dependency-boundary guard landed together ("landing it with the crate is what makes the boundary exist from commit one"); the split orders crate-only child A before guard child B, leaving the validator crate unguarded in between. - Dangling dependency IDs: child B's "Depends on: A" is a prose label, not a resolvable bundle ID; #741/#742's re-point target is unnamed; no tracker record shows the re-point was approved. - Child A's binding success criterion (cargo xtask ci green) is already unreachable on this base (h2/RUSTSEC-2026-0258, the same pre-existing advisory as #771/#721) and the brief excludes fixing it and labels external dependencies "none" — success can't be distinguished from base failure as written. - Guard child's promised scope (transitive + optional/feature-gated closures) exceeds the tracker's stated definition of done (direct wyrd-core dependency only) without fixtures for the wider claim, and doesn't test the dev-only-stays-green inverse. - CLI child criterion doesn't bind the previously-recorded "unknown flag consumed in a value slot" defect, covers only 3 of 11 flags plus unspecified credential handling, and checks only the parser function rather than the cargo xtask ci wiring. - Source grounding is stale relative to the resolved target's actual origin/main HEAD. - Strict unknown-flag rejection is added scope not present in the tracker, and contradicts the peer CLI parser's existing permissive convention. - The dependency-closure guard's design won't see optional/off-by-default normal dependencies unless all features are enabled; a concrete counterexample already exists in the tree (crates/metadata-tikv). Human directive: iterate-plan for 740 — return to Plan to re-author the split.
- By / date: Eduard Ralph / 2026-08-17

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 6 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
