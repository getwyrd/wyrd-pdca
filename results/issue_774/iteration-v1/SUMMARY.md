# Result — issue 774 / validate-crate-skeleton-and-cli-surface

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: (framed as the gap) There is no `crates/validate`, so there is nowhere to put
  the blackbox validator proposal 0017 specifies — `ls crates/validate` on `main` @ `65ca4fd`
  → no such directory, and `git log --oneline -- crates/validate` is empty (it has never
  existed). Every later slice in milestone 17 (#741 the S3 client, #742 packaging, #743 the
  capability matrix) presumes the crate, its binary, and the argument surface an operator
  points at their own cluster.
- Success criterion: BINDING, every leg observable inside `cargo xtask ci`:
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
- Repo + branch target: getwyrd/wyrd @ main
- Scope: (a) new workspace member `crates/validate` — package `wyrd-validate`, a **lib**
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

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass —                no pre-patch state to isolate a RED against; C4-ci gates the whole tree (#88).
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage 72.1% — 145 of 201 instrumentable changed lines executed (below the 80% floor); 201 of 464 changed lines w
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 39 mutants tested in 12s: 5 missed, 23 caught, 11 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_774/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 14.23s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

The #774 `wyrd-validate` crate skeleton meets the scoped ten-flag and credential-resolution contract; two test-evidence repairs remain.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The four acceptance criteria are observable and bounded; strict rejection is settled, encoding cases excluded, and no external dependency is owed for this skeleton (`brief.md:10`, `brief.md:34`, `brief.md:94`). |
| C2 Reproduction (red pre-fix) | PASS | Stashing the patch removes the crate and makes its test command exit 101 with package-not-found, reproducing the declared criterion-absence red (`reviewer-red-green.log:6`, `brief.md:49`). |
| C3 Change | PASS | The patch stays within the authorized crate, workspace/lockfile registration, and living architecture update, with no new third-party dependency (`crates/validate/Cargo.toml:21`, `Cargo.toml:33`, `docs/design/architecture/05-building-block-view.md:253`). |
| C4 Verification (red→green) | PASS | Restoring the patch gives 17/17 passing tests; frozen full CI passes, while independent workspace CI passes build/tests before a read-only advisory-database lock stops it; coverage capture is adjudicated under T5 (`reviewer-red-green.log:50`, `gate-logs/C4-ci.log:3776`, `reviewer-ci.log:3147`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Protect exit-code honesty on output failure — the suite survives changing OR to AND in the write/flush check; add assertions for rejected writes and failed flushes (`crates/validate/src/lib.rs:100`, `reviewer-mutants.log:4`). |
| T1 Structure | PASS | The dependency-free library and thin binary preserve the proposed boundary, injected credentials avoid shared environment mutation, and both crate roots forbid unsafe code (`crates/validate/src/lib.rs:1`, `crates/validate/src/main.rs:1`, `crates/validate/src/access_keys.rs:126`). |
| T2 Shape | PASS | Each required flag has an independent value assertion, both strict-rejection cases are covered, and the living architecture describes the same operator surface (`crates/validate/tests/cli_surface.rs:86`, `crates/validate/tests/cli_surface.rs:260`, `docs/design/architecture/05-building-block-view.md:253`). |
| T3 Runtime | PASS | The exercised binary reports resolved configuration without making requests, preserves credential precedence, and omits secrets; independent failing-writer probes also return exit 1 (`crates/validate/src/lib.rs:98`, `crates/validate/tests/cli_surface.rs:163`, `reviewer-runtime-probes.log:1`). |
| T4 Contribution | N/A | Contribution artifacts are absent by design and receive their substantive audit at publish; the deferred row is neither a pass nor a missing-evidence finding (`gate-logs/T4-contribution.log:10`); batch-review disposition and affected-path prior art are recorded below. |
| T5 Judgment | NEEDS-HUMAN [impl] | Repair coverage capture — clearing the child environment discards LLVM_PROFILE_FILE, so executed CLI paths are reported as missed; collecting the same run's profiles raises production line coverage from 70.62% to 92.89% (`crates/validate/tests/cli_surface.rs:55`, `reviewer-coverage-summary.log:6`, `reviewer-coverage-summary.log:12`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept this parse-and-echo milestone as sufficient for #774 — exit 0 means configuration resolved, while actual cluster validation belongs to #741/#743; judge whether the explicit no-validation notice adequately communicates that boundary (`crates/validate/src/lib.rs:103`, `brief.md:80`). |

The two implementation annotations concern regression evidence, not a demonstrated failure of the scoped runtime behavior. Source citations resolve under `$PDCA_TARGET`; brief, gate, and reviewer-log citations resolve in this review directory. The disposable target matched the patch (`git apply --reverse --check ../patch.diff` passed), and the stash was restored. No target-source fix was made.

**Coverage capture needs repair.** The frozen diff gate reports 145/201 instrumentable changed lines covered, or 72.1%, below its 80% floor (`gate-logs/C4-diff-cov.log:106`). Independently running `cargo llvm-cov test --offline --locked -p wyrd-validate --test cli_surface --json` passed all 17 tests but reported `main.rs` at 0/11 and `args.rs` at 55/90. The binary helper's `env_clear()` caused 30 child profiles to land in `crates/validate/default_*.profraw`. Collecting those existing profiles into the coverage directory and regenerating the report, without editing source or rerunning tests, gave `main.rs` 11/11 and `args.rs` 90/90 (`reviewer-coverage-summary.log:2`). Preserve the coverage destination in the child environment while retaining explicit credential isolation. The 70.62%/92.89% figures are LLVM production-file totals, not a recalculation of the frozen wrapper's changed-line denominator; the frozen gate remains red until rerun.

**Output-failure behavior needs a regression assertion.** The independent mutation run reproduced exactly 39 mutants: 23 caught, 11 unviable, five missed (`reviewer-mutants.log:9`). Changing the write/flush OR to AND can return success after a failed write or skip flushing after a successful write. The current implementation correctly returns `EXIT_IO` for both failures in an independent injected-writer probe (`reviewer-runtime-probes.log:1`), but the submitted suite does not protect that behavior. The other four survivors replace the future signing-secret accessor or usage summary with constants; they do not establish a violation of the brief's required identity/source/secret-omission assertions. Mutation testing remains advisory under `AGENTS.md:72`. The C5 capability-probe smell-test does not fire: the parser validates inputs directly and introduces no load-time capability workaround.

**The frozen batch finding is outside this patch's settled scope.** Its sole allegation is that non-Unicode AWS credentials become absent through `std::env::var(name).ok()` (`gate-logs/T4-batch-review.log:10`, `crates/validate/src/main.rs:10`). An independent invocation with two non-Unicode AWS values and a valid Wyrd pair confirms exit 0 with the Wyrd identity (`reviewer-runtime-probes.log:5`). Reject that finding for #774 under the brief's explicit exclusion of encoding cases (`brief.md:45`); it is not a false observation, but it is not an authorized reason to expand this patch. This disposition does not rewrite the frozen batch gate's red result.

**The remaining evidence supports the scoped build and integration claims.** Independent `cargo xtask ci` completed typos, docs lint/render, repository guards, formatting, workspace clippy/build/tests, and dependency-usage checking before `cargo deny` failed to lock its database on a read-only path (`reviewer-ci.log:3147`). That is a reviewer-host limitation, not a patch defect. The frozen log shows the deny checks, conformance, statics/deployment guards, and DST checks completing successfully (`gate-logs/C4-ci.log:3155`, `gate-logs/C4-ci.log:3776`). The frozen TiKV feature log shows both requested clippy commands finishing (`gate-logs/host-tikv.log:110`, `gate-logs/host-tikv.log:209`); those feature builds were not independently rerun. No external service or substitute topology is needed for the brief's parse-only contract.

**Affected-path prior art was checked directly.** Remote merged-history queries returned no commits for `crates/validate` and inspected the touched workspace/architecture paths. A paginated enumeration found 19 closed, unmerged PRs; inspecting every changed-file list found no `crates/validate` implementation and no matching architecture edit. Shared manifest/lockfile hits were dependency updates (`reviewer-prior-art.log:1`, `reviewer-prior-art.log:27`, `reviewer-prior-art.log:47`). This supplements the brief's keyword search with actual affected-file evidence.

### Advisory — adversary

# Adversarial review — issue #774 (wyrd-validate crate skeleton + CLI surface)

Re-ran the suite from a scratch copy of the crate: 17/17 green. `gate-logs/C4-ci.log:2721` shows
the same 17 tests running inside `cargo xtask ci`. "Red" here means the crate does not exist yet,
as the brief declared ahead of time, so C4-verify's `PASS (green-only)` is honest. The tests
exercise the real code: the binary tests spawn `CARGO_BIN_EXE_wyrd-validate`, and the credential
tests call the production `run` with an injected lookup. The core fix holds. What I could break
is in the credential tests, which are weaker than their names claim, and in one input the gating
T4 review already found.

- NEEDS-HUMAN [human] — **The T4 blocking finding is real, and a ruling is needed on scope.**
  `crates/validate/src/main.rs:10` (`std::env::var(name).ok()`) treats a variable that is set but
  not valid UTF-8 as unset. Reproduced:
  `env -i AWS_ACCESS_KEY_ID=$'\xff' AWS_SECRET_ACCESS_KEY=$'\xfe' WYRD_S3_ACCESS_KEY=wid WYRD_S3_SECRET_KEY=wsec wyrd-validate <all ten flags>`
  exits 0 and reports `access-key-id = wid`, source Wyrd. That is the silent identity switch the
  patch's own doc says it prevents (`crates/validate/src/access_keys.rs:92`), and it matches the
  rubric's "never silent skip" class. A second case: a valid AWS id with a non-UTF-8 secret is
  refused, but the message says `AWS_SECRET_ACCESS_KEY is set but ... is not`, and the secret *is*
  set. The brief puts "the encoding cases" out of scope, but names only non-UTF-8 argv and control
  characters in output, not environment values. A human should decide: record-reject this under
  that scope decision (which clears the gating T4 row), or take a small fix at `main.rs:10` (for
  example, refuse `VarError::NotUnicode` by name). It is unlikely in practice, since real AWS keys
  are ASCII.

- NEEDS-HUMAN [impl] — **The half-pair refusal is tested in one direction only.** The
  `(None, Some(_))` arm at `crates/validate/src/access_keys.rs:144-149` (secret set, id missing)
  never runs in any test; diff-cov reports lines 145-148 as MISS. I replaced that arm with
  `(None, Some(_)) => continue`, and all 17 tests still passed. With that change,
  `AWS_SECRET_ACCESS_KEY` set with no `AWS_ACCESS_KEY_ID`, plus a Wyrd pair, silently signs as
  Wyrd. The test named `half_a_pair_is_refused_rather_than_skipped` (`tests/cli_surface.rs:219`)
  covers only the id-without-secret half. The empty-means-unset rule is also untested: deleting
  `.filter(|value| !value.is_empty())` at `access_keys.rs:127` still passes 17/17, so the
  documented promise at `access_keys.rs:124-125` (an empty `AWS_ACCESS_KEY_ID=` does not hide a
  complete Wyrd pair) is not checked by any test. Fix: add two `lib_run` cases, one for
  secret-only AWS and one for an empty AWS id plus a Wyrd pair.

- NEEDS-HUMAN [impl] — **No test checks which secret gets resolved.** The C5 survivors at
  `crates/validate/src/access_keys.rs:67` (`secret_access_key` returning `""` or `"xyzzy"`) prove
  this. As a stronger check, I changed `resolve` (`access_keys.rs:130`) to pair the AWS id with
  `WYRD_S3_SECRET_KEY`, and all 17 tests passed. The tests prove the secret is never *printed*.
  They never prove the *right* secret is resolved, and signing in #741 depends on exactly that.
  Separately, the `Debug` redaction at `access_keys.rs:75-83` never runs (diff-cov MISS 76-82). If
  it were swapped for `#[derive(Debug)]`, `{:?}` of `ResolvedConfig` or `RunError` would print the
  secret and no test would fail. That leaves the claim at `access_keys.rs:52` ("no formatting path
  prints it") unchecked. Fix: in each credential direction, assert `secret_access_key()` equals
  the matching pair's secret, and assert `format!("{:?}", config)` does not contain it. Minor
  gaps, same cause: the C5 survivor at `lib.rs:100` (`||` changed to `&&`; the `EXIT_IO` return
  at `lib.rs:101` never runs, and a writer whose `flush` fails would cover it) and `args.rs:170`
  (the usage text is never asserted).

- NEEDS-HUMAN [impl] — **Part of the C4-diff-cov failure (72.1%) comes from the test harness,
  not missing tests.** `bin()` calls `.env_clear()` (`crates/validate/tests/cli_surface.rs:55`),
  which also strips `LLVM_PROFILE_FILE`, the variable that tells the child binary where to write
  coverage data. So the coverage from the 14 binary-level tests is lost. Reproduced with
  `cargo llvm-cov --test cli_surface`: `main.rs:8-18` shows 0 hits even though the binary runs in
  14 tests, and the run left 30 stray `default_*.profraw` files in `crates/validate/`, the child's
  working directory. `*.profraw` is not in `.gitignore`, so they show up as untracked files. That
  is why `main.rs`, the `ArgError` `Display` lines and `usage()` show as MISS. Fix: after
  `env_clear`, pass `LLVM_PROFILE_FILE` through when the test process has it set (and update the
  "never read" wording at `cli_surface.rs:9`). The real coverage gaps are the two bullets above.

- NEEDS-HUMAN [human] — **An empty value counts as present.** `--run-id ""` (for example
  `--run-id "$RUN_ID"` with `RUN_ID` unset) exits 0 and echoes `  --run-id = `. The same happens
  for every flag; `crates/validate/src/args.rs:136` stores whatever it is given, and the
  `Missing` check at `args.rs:163` only catches flags that never appeared. The credential side
  treats empty as unset (`access_keys.rs:127`), so the two halves disagree. Proposal 0017 §5
  makes the run id the key prefix that keeps "delete only mine" safe, and an empty prefix covers
  the whole bucket. `args.rs:33` explicitly leaves value checks to later slices, so leaving this
  is defensible. Needs a call: refuse empty values now (a one-line check) or record it against
  #741/#743.

Tried to break these and could not:
- **Criterion 2.** The per-flag echo test uses ten distinct values and matches whole lines, so a
  dropped or swapped flag fails.
- **Criterion 4, both halves.** Each of these exits 2 and names the offending token:
  `--bucket --typo`, `--bucket --duration 2m`, `--bucket=foo`, a bare `--`, `--help`, `--BUCKET`,
  a repeated flag, and a stray positional. A single-dash `-typo` is accepted as a value. That is
  outside criterion 4's `--` wording, and values like negative seeds need it, so I did not raise
  it.
- **Out of scope, not raised.** Non-UTF-8 argv panics with exit 101 at `main.rs:9`; the brief
  rules the encoding cases out.
- **Citations.** The `path:line` references in the code comments (`crates/server/src/cli.rs:2166`,
  `:2168`, `:2173`, `:2533-2570`, `:2550-2553`, `xtask/src/main.rs:1559`) all match the base
  commit.
- **Docs currency.** Met: `docs/design/architecture/05-building-block-view.md` lists the ten
  flags and the credential order.
- **Dependencies.** The crate adds none; `Cargo.lock` gains only the bare package entry.

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/validate/tests/cli_surface.rs:72` and `crates/validate/src/lib.rs:100`: The new stdout-failure exit contract lacks regression coverage: library tests always supply successful `Vec` writers. The frozen `C5-mutants` log confirms that replacing `||` with `&&` survives; that mutation returns success when writing fails but flushing succeeds, and skips flushing entirely when writing succeeds. Add injected-writer cases for write failure and flush-only failure, asserting `EXIT_IO` for both. The current production condition is correct; this finding concerns the tests.

No in-scope production correctness bugs or actionable reuse, simplification, or efficiency findings identified. Review used the target source and frozen gate evidence; gates were not rerun. The non-Unicode credential finding in the frozen batch review was not re-raised, consistent with the brief's settled exclusion of encoding cases.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Protect exit-code honesty on output failure — the suite survives changing OR to AND in the write/flush check; add assertions for rejected writes and failed flushes (`crates/validate/src/lib.rs:100`, `reviewer-mutants.log:4`).
- [ ] T5 Judgment — Repair coverage capture — clearing the child environment discards LLVM_PROFILE_FILE, so executed CLI paths are reported as missed; collecting the same run's profiles raises production line coverage from 70.62% to 92.89% (`crates/validate/tests/cli_surface.rs:55`, `reviewer-coverage-summary.log:6`, `reviewer-coverage-summary.log:12`).
- [ ] Validation — fitness-to-purpose — Accept this parse-and-echo milestone as sufficient for #774 — exit 0 means configuration resolved, while actual cluster validation belongs to #741/#743; judge whether the explicit no-validation notice adequately communicates that boundary (`crates/validate/src/lib.rs:103`, `brief.md:80`).
- [ ] **The T4 blocking finding is real, and a ruling is needed on scope.** `crates/validate/src/main.rs:10` (`std::env::var(name).ok()`) treats a variable that is set but not valid UTF-8 as unset. Reproduced: `env -i AWS_ACCESS_KEY_ID=$'\xff' AWS_SECRET_ACCESS_KEY=$'\xfe' WYRD_S3_ACCESS_KEY=wid WYRD_S3_SECRET_KEY=wsec wyrd-validate <all ten flags>` exits 0 and reports `access-key-id = wid`, source Wyrd. That is the silent identity switch the patch's own doc says it prevents (`crates/validate/src/access_keys.rs:92`), and it matches the rubric's "never silent skip" class. A second case: a valid AWS id with a non-UTF-8 secret is refused, but the message says `AWS_SECRET_ACCESS_KEY is set but ... is not`, and the secret *is* set. The brief puts "the encoding cases" out of scope, but names only non-UTF-8 argv and control characters in output, not environment values. A human should decide: record-reject this under that scope decision (which clears the gating T4 row), or take a small fix at `main.rs:10` (for example, refuse `VarError::NotUnicode` by name). It is unlikely in practice, since real AWS keys are ASCII.
- [ ] **The half-pair refusal is tested in one direction only.** The `(None, Some(_))` arm at `crates/validate/src/access_keys.rs:144-149` (secret set, id missing) never runs in any test; diff-cov reports lines 145-148 as MISS. I replaced that arm with `(None, Some(_)) => continue`, and all 17 tests still passed. With that change, `AWS_SECRET_ACCESS_KEY` set with no `AWS_ACCESS_KEY_ID`, plus a Wyrd pair, silently signs as Wyrd. The test named `half_a_pair_is_refused_rather_than_skipped` (`tests/cli_surface.rs:219`) covers only the id-without-secret half. The empty-means-unset rule is also untested: deleting `.filter(|value| !value.is_empty())` at `access_keys.rs:127` still passes 17/17, so the documented promise at `access_keys.rs:124-125` (an empty `AWS_ACCESS_KEY_ID=` does not hide a complete Wyrd pair) is not checked by any test. Fix: add two `lib_run` cases, one for secret-only AWS and one for an empty AWS id plus a Wyrd pair.
- [ ] **No test checks which secret gets resolved.** The C5 survivors at `crates/validate/src/access_keys.rs:67` (`secret_access_key` returning `""` or `"xyzzy"`) prove this. As a stronger check, I changed `resolve` (`access_keys.rs:130`) to pair the AWS id with `WYRD_S3_SECRET_KEY`, and all 17 tests passed. The tests prove the secret is never *printed*. They never prove the *right* secret is resolved, and signing in #741 depends on exactly that. Separately, the `Debug` redaction at `access_keys.rs:75-83` never runs (diff-cov MISS 76-82). If it were swapped for `#[derive(Debug)]`, `{:?}` of `ResolvedConfig` or `RunError` would print the secret and no test would fail. That leaves the claim at `access_keys.rs:52` ("no formatting path prints it") unchecked. Fix: in each credential direction, assert `secret_access_key()` equals the matching pair's secret, and assert `format!("{:?}", config)` does not contain it. Minor gaps, same cause: the C5 survivor at `lib.rs:100` (`||` changed to `&&`; the `EXIT_IO` return at `lib.rs:101` never runs, and a writer whose `flush` fails would cover it) and `args.rs:170` (the usage text is never asserted).
- [ ] **Part of the C4-diff-cov failure (72.1%) comes from the test harness, not missing tests.** `bin()` calls `.env_clear()` (`crates/validate/tests/cli_surface.rs:55`), which also strips `LLVM_PROFILE_FILE`, the variable that tells the child binary where to write coverage data. So the coverage from the 14 binary-level tests is lost. Reproduced with `cargo llvm-cov --test cli_surface`: `main.rs:8-18` shows 0 hits even though the binary runs in 14 tests, and the run left 30 stray `default_*.profraw` files in `crates/validate/`, the child's working directory. `*.profraw` is not in `.gitignore`, so they show up as untracked files. That is why `main.rs`, the `ArgError` `Display` lines and `usage()` show as MISS. Fix: after `env_clear`, pass `LLVM_PROFILE_FILE` through when the test process has it set (and update the "never read" wording at `cli_surface.rs:9`). The real coverage gaps are the two bullets above.
- [ ] **An empty value counts as present.** `--run-id ""` (for example `--run-id "$RUN_ID"` with `RUN_ID` unset) exits 0 and echoes `  --run-id = `. The same happens for every flag; `crates/validate/src/args.rs:136` stores whatever it is given, and the `Missing` check at `args.rs:163` only catches flags that never appeared. The credential side treats empty as unset (`access_keys.rs:127`), so the two halves disagree. Proposal 0017 §5 makes the run id the key prefix that keeps "delete only mine" safe, and an empty prefix covers the whole bucket. `args.rs:33` explicitly leaves value checks to later slices, so leaving this is defensible. Needs a call: refuse empty values now (a one-line check) or record it against
- [ ] `crates/validate/tests/cli_surface.rs:72` and `crates/validate/src/lib.rs:100`: The new stdout-failure exit contract lacks regression coverage: library tests always supply successful `Vec` writers. The frozen `C5-mutants` log confirms that replacing `||` with `&&` survives; that mutation returns success when writing fails but flushing succeeds, and skips flushing entirely when writing succeeds. Add injected-writer cases for write failure and flush-only failure, asserting `EXIT_IO` for both. The current production condition is correct; this finding concerns the tests.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_774/review-b

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Auto-iterate (round 1): rebuilding for the implementation-level findings — C5 Causal adequacy — Protect exit-code honesty on output failure — the suite survives changing OR to AND in the write/flush check; add assertions for rejected writes and failed flushes (`crates/validate/src/lib.rs:100`, `reviewer-mutants.log:4`).; T5 Judgment — Repair coverage capture — clearing the child environment discards LLVM_PROFILE_FILE, so executed CLI paths are reported as missed; collecting the same run's profiles raises production line coverage from 70.62% to 92.89% (`crates/validate/tests/cli_surface.rs:55`, `reviewer-coverage-summary.log:6`, `reviewer-coverage-summary.log:12`).; **The half-pair refusal is tested in one direction only.** The `(None, Some(_))` arm at `crates/validate/src/access_keys.rs:144-149` (secret set, id missing) never runs in any test; diff-cov reports lines 145-148 as MISS. I replaced that arm with `(None, Some(_)) => continue`, and all 17 tests still passed. With that change, `AWS_SECRET_ACCESS_KEY` set with no `AWS_ACCESS_KEY_ID`, plus a Wyrd pair, silently signs as Wyrd. The test named `half_a_pair_is_refused_rather_than_skipped` (`tests/cli_surface.rs:219`) covers only the id-without-secret half. The empty-means-unset rule is also untested: deleting `.filter(|value| !value.is_empty())` at `access_keys.rs:127` still passes 17/17, so the documented promise at `access_keys.rs:124-125` (an empty `AWS_ACCESS_KEY_ID=` does not hide a complete Wyrd pair) is not checked by any test. Fix: add two `lib_run` cases, one for secret-only AWS and one for an empty AWS id plus a Wyrd pair.; **No test checks which secret gets resolved.** The C5 survivors at `crates/validate/src/access_keys.rs:67` (`secret_access_key` returning `""` or `"xyzzy"`) prove this. As a stronger check, I changed `resolve` (`access_keys.rs:130`) to pair the AWS id with `WYRD_S3_SECRET_KEY`, and all 17 tests passed. The tests prove the secret is never *printed*. They never prove the *right* secret is resolved, and signing in #741 depends on exactly that. Separately, the `Debug` redaction at `access_keys.rs:75-83` never runs (diff-cov MISS 76-82). If it were swapped for `#[derive(Debug)]`, `{:?}` of `ResolvedConfig` or `RunError` would print the secret and no test would fail. That leaves the claim at `access_keys.rs:52` ("no formatting path prints it") unchecked. Fix: in each credential direction, assert `secret_access_key()` equals the matching pair's secret, and assert `format!("{:?}", config)` does not contain it. Minor gaps, same cause: the C5 survivor at `lib.rs:100` (`||` changed to `&&`; the `EXIT_IO` return at `lib.rs:101` never runs, and a writer whose `flush` fails would cover it) and `args.rs:170` (the usage text is never asserted).; **Part of the C4-diff-cov failure (72.1%) comes from the test harness, not missing tests.** `bin()` calls `.env_clear()` (`crates/validate/tests/cli_surface.rs:55`), which also strips `LLVM_PROFILE_FILE`, the variable that tells the child binary where to write coverage data. So the coverage from the 14 binary-level tests is lost. Reproduced with `cargo llvm-cov --test cli_surface`: `main.rs:8-18` shows 0 hits even though the binary runs in 14 tests, and the run left 30 stray `default_*.profraw` files in `crates/validate/`, the child's working directory. `*.profraw` is not in `.gitignore`, so they show up as untracked files. That is why `main.rs`, the `ArgError` `Display` lines and `usage()` show as MISS. Fix: after `env_clear`, pass `LLVM_PROFILE_FILE` through when the test process has it set (and update the "never read" wording at `cli_surface.rs:9`). The real coverage gaps are the two bullets above.; `crates/validate/tests/cli_surface.rs:72` and `crates/validate/src/lib.rs:100`: The new stdout-failure exit contract lacks regression coverage: library tests always supply successful `Vec` writers. The frozen `C5-mutants` log confirms that replacing `||` with `&&` survives; that mutation returns success when writing fails but flushing succeeds, and skips flushing entirely when writing succeeds. Add injected-writer cases for write failure and flush-only failure, asserting `EXIT_IO` for both. The current production condition is correct; this finding concerns the tests.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_774/review-b. 2 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-09-30

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
