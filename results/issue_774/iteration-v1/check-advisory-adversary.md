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
