# Build notes — issue 439 / fdb-dev-ci-harness (iteration 3)

## Scope of this iteration

Iterations 1 and 2 built the whole slice; both were rejected by the **adversarial**
reviewer's mutation pass (the primary reviewer PASSed). The iteration-2 carry-forward
names exactly **two** local defects to fix and lists what "survived and must NOT be
churned" (fixes #2/#4/#5 — the fdb/tikv toolchain coupling, the PR-leg server fdb
type-check, the doctor pure logic / planted-red / drift guards). I rebuilt on the
iteration-2 patch, changed **only** the two named seams, and left everything else byte-
for-byte. Base branch: `getwyrd/wyrd` @ `b1ccca3` (main, post-M4).

### Fix 1 — the workflow↔dispatch guard counted a *trailing* `#` comment as an execution

`run_script_lines` (`xtask/tests/fdb_harness.rs`) stripped only lines that *start* with
`#`. A surviving trailing comment — `run: echo DISABLED # cargo xtask fdb-conformance`,
inline or inside a `run: |` block — was then tokenised by `xtask_subcommands` as a real
command, so a step that executes **nothing** satisfied the dispatch contract. The
iteration-2 demonstrated-red exercised only *full-line* comments, so the hole was
uncovered.

Fix: a quote-aware `strip_shell_comment(line)` applied to every captured `run:` command
before it is stored. A `#` opens a comment only when it is **unquoted and at a word
boundary** (start of line or preceded by whitespace) — the POSIX shell rule — so
`echo foo#bar`, an URL fragment, and a `#` inside quotes all survive, while a dangling
`# cargo xtask …` note is removed. The demonstrated-red fixture now includes the trailing-
comment shape (both inline and in a block), plus a focused
`strip_shell_comment_honours_quotes_and_word_boundaries` unit test.

Why not "require the token be the command head, not any window" (the other option the
carry-forward offered)? The window scan in `xtask_subcommands` legitimately needs to find
`cargo xtask <sub>` mid-line (e.g. after a `&&` or `env FOO=bar cargo xtask …`), so
constraining to the head would produce false negatives on real compound commands. Stripping
the comment at the source is both narrower and more faithful to what the shell runs — the
whole premise of scraping `run:` is "what does this line actually execute". Cost of the
chosen fix: +25 lines (the stripper) vs. the head-constraint's rewrite of the tokeniser and
a weaker model of shell.

### Fix 2 — the impure `FDB_CLIENT_LIB_PATH` read was Check-unguarded

The pure `client_library_search_paths` was covered, but the impure *decision* in
`main.rs`'s `probe_client_library` (read `FDB_CLIENT_LIB_PATH`, stat candidates, fall back
to `ldconfig`) was not. Mutating the env read to `let configured = None;` reintroduced the
exact iteration-1 false-negative (a working custom-prefix build reported "missing" ⇒
false-green skip locally / hard-fail in CI) with **all** tests still green.

Fix: move the decision into the lib as `fdb_doctor::probe_client_library(env, path_exists,
ldconfig)` — the same injected-effects seam `run_gated_conformance`/`run_ci_steps` already
use. `main.rs`'s `probe_client_library` is now only the wiring that supplies the real
`std::env::var` / `Path::exists` / `ldconfig` effects. A new Check test,
`probe_client_library_finds_a_custom_prefix_build_via_fdb_client_lib_path`, drives the
production decision with a fake environment that sets `FDB_CLIENT_LIB_PATH` and a fake
filesystem in which the library exists at **only** that path — so dropping the env read in
production flips it red (demonstrated below). The test also covers the "nowhere ⇒ Failed,
naming the variable" and "ldconfig fallback" branches for non-vacuity.

Why move to the lib rather than shell out to `cargo xtask fdb-doctor` in a subprocess? The
subprocess approach would drive the real binary end-to-end but (a) spawns cargo/builds
inside a test — heavy and slow on a headless runner, (b) needs a writable fake prefix and
env manipulation of a live process. The injected-effects move keeps the harness load-light
and drives the *same* production decision function. Cost: the decision logic (~20 lines)
relocates from the binary to the lib; `main.rs`'s probe shrinks to a 10-line wiring call.

## Refutation (forced, recorded)

**(a) Genuine red?** Yes — verified by reverting each fix in place and re-running
`cargo test -p xtask --test fdb_harness`:
- Neutering `strip_shell_comment` (make the `#`-break unreachable) →
  `run_script_scraping_ignores_prose_and_is_red_on_a_workflow_that_runs_nothing` **and**
  `strip_shell_comment_honours_quotes_and_word_boundaries` FAILED (2 failed / 24 passed).
- Dropping the env read (`let configured: Option<String> = None;`) in
  `fdb_doctor::probe_client_library` →
  `probe_client_library_finds_a_custom_prefix_build_via_fdb_client_lib_path` FAILED
  (1 failed / 25 passed).
Both reverts restored → 26 passed.

**(b) Production path?** Yes. Fix 2's test calls `xtask::fdb_doctor::probe_client_library`
— the exact function `main.rs`'s production `probe_client_library` delegates to (verified
by running `cargo xtask fdb-doctor` on this host: it reported `client library (libfdb_c):
found at /usr/lib/libfdb_c.so`, i.e. the production wiring drives the lib decision). Fix 1's
test drives `run_script_lines` / `xtask_subcommands` — the same scraper the workflow↔dispatch
assertion (`the_fdb_conformance_workflow_executes_only_real_subcommands`) uses against the
real shipped `.github/workflows/fdb-conformance.yml`.

**(c) Fixture includes the fault?** Yes. Fix 1's `PROSE_ONLY` fixture *includes* the
failing element — a step whose `run:` executes only `echo DISABLED` with a trailing
`# cargo xtask fdb-conformance`, plus a block line `echo installed # cargo xtask ci`. The
assertion is that the scraper yields **no** xtask subcommands from it. Fix 2's fixture puts
the library at *only* the `FDB_CLIENT_LIB_PATH` prefix (no standard prefix, no ldconfig
entry), so a passing verdict is possible **only** if production consulted the variable.

## Supplementary live leg — `cargo xtask fdb-conformance` (RUN, as the brief requires)

Host prerequisites present: Docker OK; `/lib/libfdb_c.so` 7.3.77; `fdbcli` 7.3 (v7.3.77) —
byte-matching the compose image `foundationdb/foundationdb:7.3.77`.

Ran `cargo xtask fdb-conformance` from the worktree end-to-end: it brought up
`deploy/fdb-single-node/`, configured the database, wrote the host-side cluster file, and
drove all five `--features fdb` test legs — **all green**:
- shared `MetadataStore` conformance suite: passed
- contention properties (blind/conditional race, batch commit): 3 passed
- scan (paged prefix, cap-exceeded loud): 2 passed
- timeout (blind/conditional commit unknown-vs-conflict, unreachable-get): 3 passed
- stack torn down cleanly; final line: "FoundationDB passed the shared MetadataStore
  conformance suite and the contention properties".

Also ran `cargo xtask fdb-doctor`: client-library row OK (found `/usr/lib/libfdb_c.so`),
cluster-file + cluster-health rows FAIL with the actionable remediations (no cluster file
present outside a conformance run), exit 1 — the intended not-ready behaviour.

## Gate/hook readiness

- `cargo fmt -p xtask` applied; `cargo fmt --check -p xtask` clean.
- `cargo clippy -p xtask --all-targets -- -D warnings` clean.
- `cargo test -p xtask --test fdb_harness` → 26 passed (the brief's success criterion).
- Patch verified to apply cleanly onto a fresh worktree off `b1ccca3`.
- No write under `docs/design/` (grep of patch.diff: 0 hits); the audit-policy note is in
  `deny.toml`'s header comment, as the brief pins.

## Not touched (kept from iteration 2, per the carry-forward "do NOT churn")

The fdb/tikv independent toolchain gates (`feature_gated_checks(tikv, fdb)` in the lib +
`run_ci_steps`' env-lookup wiring + its unit tests), the PR-leg server `--features fdb`
type-check and its path filter, the doctor's pure verdict/remediation logic, the planted-
red, the version drift guards, and the workflow shape are unchanged from the iteration-2
patch.
