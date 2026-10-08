- **Slug:** validate-crate-skeleton-and-cli-surface
- **Track:** blackbox
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
- **Disposition hint:** new-feature

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 1): rebuilding for the implementation-level findings — C5 Causal adequacy — Protect exit-code honesty on output failure — the suite survives changing OR to AND in the write/flush check; add assertions for rejected writes and failed flushes (`crates/validate/src/lib.rs:100`, `reviewer-mutants.log:4`).; T5 Judgment — Repair coverage capture — clearing the child environment discards LLVM_PROFILE_FILE, so executed CLI paths are reported as missed; collecting the same run's profiles raises production line coverage from 70.62% to 92.89% (`crates/validate/tests/cli_surface.rs:55`, `reviewer-coverage-summary.log:6`, `reviewer-coverage-summary.log:12`).; **The half-pair refusal is tested in one direction only.** The `(None, Some(_))` arm at `crates/validate/src/access_keys.rs:144-149` (secret set, id missing) never runs in any test; diff-cov reports lines 145-148 as MISS. I replaced that arm with `(None, Some(_)) => continue`, and all 17 tests still passed. With that change, `AWS_SECRET_ACCESS_KEY` set with no `AWS_ACCESS_KEY_ID`, plus a Wyrd pair, silently signs as Wyrd. The test named `half_a_pair_is_refused_rather_than_skipped` (`tests/cli_surface.rs:219`) covers only the id-without-secret half. The empty-means-unset rule is also untested: deleting `.filter(|value| !value.is_empty())` at `access_keys.rs:127` still passes 17/17, so the documented promise at `access_keys.rs:124-125` (an empty `AWS_ACCESS_KEY_ID=` does not hide a complete Wyrd pair) is not checked by any test. Fix: add two `lib_run` cases, one for secret-only AWS and one for an empty AWS id plus a Wyrd pair.; **No test checks which secret gets resolved.** The C5 survivors at `crates/validate/src/access_keys.rs:67` (`secret_access_key` returning `""` or `"xyzzy"`) prove this. As a stronger check, I changed `resolve` (`access_keys.rs:130`) to pair the AWS id with `WYRD_S3_SECRET_KEY`, and all 17 tests passed. The tests prove the secret is never *printed*. They never prove the *right* secret is resolved, and signing in #741 depends on exactly that. Separately, the `Debug` redaction at `access_keys.rs:75-83` never runs (diff-cov MISS 76-82). If it were swapped for `#[derive(Debug)]`, `{:?}` of `ResolvedConfig` or `RunError` would print the secret and no test would fail. That leaves the claim at `access_keys.rs:52` ("no formatting path prints it") unchecked. Fix: in each credential direction, assert `secret_access_key()` equals the matching pair's secret, and assert `format!("{:?}", config)` does not contain it. Minor gaps, same cause: the C5 survivor at `lib.rs:100` (`||` changed to `&&`; the `EXIT_IO` return at `lib.rs:101` never runs, and a writer whose `flush` fails would cover it) and `args.rs:170` (the usage text is never asserted).; **Part of the C4-diff-cov failure (72.1%) comes from the test harness, not missing tests.** `bin()` calls `.env_clear()` (`crates/validate/tests/cli_surface.rs:55`), which also strips `LLVM_PROFILE_FILE`, the variable that tells the child binary where to write coverage data. So the coverage from the 14 binary-level tests is lost. Reproduced with `cargo llvm-cov --test cli_surface`: `main.rs:8-18` shows 0 hits even though the binary runs in 14 tests, and the run left 30 stray `default_*.profraw` files in `crates/validate/`, the child's working directory. `*.profraw` is not in `.gitignore`, so they show up as untracked files. That is why `main.rs`, the `ArgError` `Display` lines and `usage()` show as MISS. Fix: after `env_clear`, pass `LLVM_PROFILE_FILE` through when the test process has it set (and update the "never read" wording at `cli_surface.rs:9`). The real coverage gaps are the two bullets above.; `crates/validate/tests/cli_surface.rs:72` and `crates/validate/src/lib.rs:100`: The new stdout-failure exit contract lacks regression coverage: library tests always supply successful `Vec` writers. The frozen `C5-mutants` log confirms that replacing `||` with `&&` survives; that mutation returns success when writing fails but flushing succeeds, and skips flushing entirely when writing succeeds. Add injected-writer cases for write failure and flush-only failure, asserting `EXIT_IO` for both. The current production condition is correct; this finding concerns the tests.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_774/review-b. 2 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage 72.1% — 145 of 201 instrumentable changed lines executed (below the 80% floor); 201 of 464 changed lines w
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 39 mutants tested in 12s: 5 missed, 23 caught, 11 unviable
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_774/review-b
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
