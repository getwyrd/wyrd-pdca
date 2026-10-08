# Build notes — issue 740 (iteration 3), `validate-crate-skeleton-and-blackbox-lint`

> **Read first (C4-ci will be red, and not because of this patch).**
> `cargo xtask ci` passes every leg this patch touches — including the new guard running
> inside the gate — and then fails at `cargo deny check advisories` on **RUSTSEC-2026-0258
> (`h2 0.4.15`)**, a transitive dependency of `wyrd-server`/`tonic`/`axum` that is pinned in
> the **base** lockfile and that `wyrd-validate` (zero dependencies) cannot reach. The same
> leg was green in iteration 2's frozen log ~11 h earlier, so the advisory entered the
> RustSec database mid-cycle; `main` fails identically today. Evidence and the reason it is
> deliberately **not** fixed here are in §5 — please read that before ordering another
> iterate on the failing gate.

Target branch: `getwyrd/wyrd @ main`, built in `$PDCA_WORKTREE`
(`/home/eddie/wyrd/wyrd.pdca-wt-l0`, base `a801997`). Every `path:line` below is that tree
**after** the patch unless it says "base".

Plan of record read in place: `docs/design/proposals/draft/0017-blackbox-validation-tool.md`
§2 (layering — "pure, Check-tested" vs "I/O only", and why the tool must not be a `wyrd`
subcommand, `:159-164`) and §9 (the blackbox property as a lint, `:559-575`).

---

## 1. What this iteration changes relative to iteration 2

Iteration 2 was rejected on three **implementation** findings (all accepted here — none is
re-submitted unchanged) plus a causal-adequacy demand for four missing proof classes. The
architecture (new member crate + lib/bin split + guard registered as lib-side data) was
signed off as PASS on C1/C2/C3/C4/T1, so it is kept; everything the reviewer marked FAIL is
rebuilt.

| Carry-forward item | What was wrong | What this patch does |
|---|---|---|
| `crates/validate/src/main.rs:11` — `std::env::var(name).ok()` reports a non-UTF-8 credential as **absent** | a fully-set-but-unreadable `AWS_ACCESS_KEY_ID` silently fell through to the `WYRD_S3_*` identity, and the echoed block then named that identity as though the operator had chosen it | the lookup is no longer `Option<String>`. `EnvValue { Absent, Present(String), NotUnicode }` (`crates/validate/src/lib.rs:244`) makes the three answers distinct; `read_credential` (`:313`) turns `NotUnicode` into an error naming the variable, and only `Absent` falls through. `main` builds it from `std::env::var_os` (`crates/validate/src/main.rs:17-23`) |
| `xtask/src/repo_guard.rs:757` — a declared dependency with a missing / non-null / non-string `kind` was silently classified non-normal | malformed (or hand-edited, or future-cargo) metadata could hide a forbidden `wyrd-*` edge while the gate went green — a vacuous pass in a guard whose whole contract is failing closed | `dep_kind` (`xtask/src/repo_guard.rs:626`) accepts exactly `null` / `"dev"` / `"build"` and returns `Err` for anything else, **including a missing key**; it is the single classifier for both the declared list (`:858`) and the resolve-graph edges (`edge_is_normal`, `:655`, which also rejects a missing or empty `dep_kinds` array) |
| `crates/validate/src/lib.rs:302` — raw interpolation of flag values and the access-key id | a value carrying `\n` printed a line that reads exactly like another resolved flag, so the record misstated the run; a terminal control sequence could rewrite what the operator sees | one field per line, every operator-supplied value double-quoted and escaped by `quoted` (`crates/validate/src/lib.rs:397`, `str::escape_debug`), used by `render_config_block` (`:406`) **and** by every parse error that echoes a token (`:112-148`, `:427`) — nothing is dropped, the field boundary just becomes unforgeable |
| C5 — "add successful multi-hop closure, malformed declared-`kind`, non-UTF-8 credential, control-character output cases" | the planted cases covered only direct normal/dev/optional edges and UTF-8 fixtures | multi-hop violation with the reported path + a clean three-hop closure + dev/build pruning behind hop 1 + a dev-and-normal diamond + a cycle (`xtask/tests/blackbox_dependency_guard.rs:159, 186, 213, 242, 273`); malformed `kind` on both surfaces (`:352, 394`); non-UTF-8 credential over the real binary and over the injected lookup (`crates/validate/tests/cli_surface.rs:800, 654`); control-character/newline forging on values and on the id (`:340, 379, 402`) |
| T5 — "must fail closed on unreadable metadata **and credentials**, emit an unambiguous configuration record, and prove transitive traversal" | see the three rows above | all three, plus two hardenings the rebuild made obvious: a non-UTF-8 **argument** is refused by name instead of panicking in `std::env::args()` (`crates/validate/src/lib.rs:427`, `:449`), and `Credentials`' `Debug` is hand-written to redact the secret (`:300`) so a future `{:?}` cannot leak it |

Two further things this iteration adds that the review did not ask for but that the same
reasoning demands:

* **the metadata invocation is data, not an argv buried in the runner** —
  `BLACKBOX_METADATA_ARGS` (`xtask/src/repo_guard.rs:591`). Dropping `--all-features` was a
  silent hole: every planted case would stay green while the guard stopped seeing
  optional off-by-default edges. The test asserts the flags AND resolves the real
  workspace through the same argv production uses
  (`xtask/tests/blackbox_dependency_guard.rs:470`).
* **the wiring itself is exercised** — `run_hygiene_guards` (`xtask/src/repo_guard.rs:970`)
  is `run_ci`'s hygiene phase lifted into the lib, taking an injected runner exactly as
  `run_ci_steps` takes an injected `exec` (`xtask/src/main.rs:1486-1498`). The test drives
  it with a recording runner (`blackbox_dependency_guard.rs:523`) and with a failing one
  (`:545`), so "registered but never reached" and "reached but its `Err` discarded" are
  both red-able. `run_ci` is one line (`xtask/src/main.rs:1565`).

## 2. The change, file by file

* `Cargo.toml:30-33` — `crates/validate` joins `[workspace] members`. Registration is not
  bookkeeping: `unregistered_manifests` (base `xtask/src/repo_guard.rs:421-455`) fails the
  gate on a package under `crates/` that is not a member.
* `Cargo.lock` — one `[[package]] wyrd-validate` stanza, no dependency lines (the crate has
  none). Needed because the guard runs `cargo metadata --locked`.
* `crates/validate/Cargo.toml` — package + auto-discovered `[[bin]] wyrd-validate`, no
  `[dependencies]` table at all, workspace lints inherited.
* `crates/validate/src/lib.rs` — the pure decisions: `REQUIRED_FLAGS` (`:53`), `ParsedArgs`
  (`:96`, strict in both token positions), `Config`/`Config::entries` (`:175`, `:194`),
  `resolve_config` (`:213`), `EnvValue` (`:244`), `CredentialSource` (`:258`),
  `Credentials` + redacting `Debug` (`:290`, `:300`), `resolve_credentials` (`:343`),
  `quoted` (`:397`), `render_config_block` (`:406`), `utf8_args` (`:427`), `run` (`:449`).
* `crates/validate/src/main.rs` — 34 lines, the only place that touches real process state:
  `std::env::args_os()` + `std::env::var_os` mapped into `EnvValue`.
* `xtask/src/repo_guard.rs` — the third invariant (module docs `:32-41`), the guard
  (`:569-945`), and `run_hygiene_guards` (`:970-977`).
* `xtask/src/main.rs:1558-1565` — `run_ci` runs the phase after the two #616 guards;
  `:1932` now delegates `workspace_root` to the lib.
* `xtask/src/lib.rs:32` — `workspace_root` moved here (one derivation for lib, bin and
  tests) because a lib-side guard needs it and cannot call a binary-target `fn`.
* `docs/design/architecture/05-building-block-view.md:247`, `:253-278` — §5.4 and the
  cross-cutting-components row. This is a merge requirement under AGENTS.md §Docs currency
  (a new CLI surface), not a follow-up: the flag table, the credential precedence, the
  refusal rules, and the record's escaping are all operator-visible.

## 3. Forced refutation — the three required answers

**(a) Genuine red?** Yes. Each fix was individually reverted in the worktree and the suite
re-run; each produced exactly the failures the corresponding tests exist to catch, and
nothing else. All reverts were undone and the tree re-verified green before `patch.diff`
was generated (`grep -rn REFUTATION --include=*.rs` → empty). Counts in the table below are
from the moment of each revert (50 tests ship in total: 29 + 21).

| Reverted to | Result |
|---|---|
| iteration-2's lenient `kind` classification (`_ => Ok(DepKind::Dev)`, missing key → `Dev`) | `test result: FAILED. 18 passed; 2 failed` — `an_edge_whose_kind_cannot_be_classified_is_err_not_silently_non_normal`, `a_declared_dependency_whose_kind_cannot_be_classified_is_err` |
| direct-dependencies-only walk (never enqueue the next hop) | `FAILED. 16 passed; 4 failed` — `a_multi_hop_normal_path_reaches_the_violation_and_reports_the_path`, `a_dev_route_does_not_mask_a_normal_route_to_the_same_crate`, `a_cyclic_graph_terminates_and_still_reports`, `an_edge_reaching_a_package_with_no_resolve_node_is_err` |
| iteration-2's `var(name).ok()` collapse (`Err(_) => EnvValue::Absent` in `main.rs`) | `FAILED. 27 passed; 1 failed` — `a_non_utf8_credential_refuses_over_the_real_binary_instead_of_falling_back` |
| quoted-but-**unescaped** rendering (`format!("\"{value}\"")`) | `FAILED. 25 passed; 3 failed` — `a_newline_in_a_value_cannot_forge_a_configuration_line`, `a_terminal_control_sequence_in_a_value_is_escaped`, `a_newline_in_the_access_key_id_cannot_forge_a_credentials_line` |
| guard defined but **unregistered** (`HYGIENE_GUARDS = &[]`) | `FAILED. 17 passed; 3 failed` — the registration test and both `run_hygiene_guards` tests |
| guard reached but its `Err` **discarded** (`let _ = run(guard);`) | `FAILED. 19 passed; 1 failed` — `run_hygiene_guards_stops_at_the_first_failing_guard` |

Whole-patch red is criterion-ABSENCE, exactly as the brief pre-declares under *Verification
posture*: with the patch reverted there is no `crates/validate` and no
`xtask::repo_guard::scan_blackbox_dependency_closure`, so both test files fail to compile.
The load-bearing red is the planted-metadata table above, which runs inside `cargo xtask ci`
every cycle hereafter.

**(b) Production path?** Yes, on both halves.
* The guard test calls `xtask::repo_guard::scan_blackbox_dependency_closure` — the same
  function `run_blackbox_dependency_guard` calls (`xtask/src/repo_guard.rs:914`) — and the
  registration test *invokes the registered callable itself* (`(guard.run)()`), which
  shells the real `cargo metadata` over the real workspace. The gate log shows the guard
  running inside `cargo xtask ci` and printing its verdict (§5).
* The CLI tests spawn the real compiled binary via `env!("CARGO_BIN_EXE_wyrd-validate")`,
  so `crates/validate/src/main.rs`'s own `var_os`/`args_os` closures are the code under
  test — which is why the `Err(_) => EnvValue::Absent` revert turns the non-UTF-8 case red.
  The pure cases call the same lib functions `main` calls, over an injected lookup; no
  logic is re-implemented in the test.

**(c) Fixture includes the fault?** Yes. Every planted document *contains* the violating
edge (a normal `wyrd-core` edge at one, two and three hops; an optional off-by-default
declared edge; a `wyrd-core` reachable by both a dev and a normal route), and the
fail-closed documents contain a real forbidden edge whose `kind`/node/`pkg` is unreadable —
so a guard that skipped the unreadable part would report "clean" over a document that is
not. The credential fixtures set the *complete* fallback pair while the consulted pair is
half-stated or unreadable, so a silent fall-through would succeed rather than fail — the
test asserts the fallback identity never appears in stdout. The output-forging fixtures put
the forged text inside the value the operator supplied, not beside it.

## 4. Alternatives considered, with their cost

* **Reject control characters at parse time instead of escaping the record.** Cheaper by
  roughly the same 3 lines, but it refuses input that is legal for the OS (`--out` may
  legitimately name a path with an odd byte) while still leaving the *access-key id* —
  which comes from the environment, not the parser — unescaped, so the credentials line
  would remain forgeable. Escaping at the single render helper covers both surfaces and
  discards nothing.
* **`Option<Result<String, _>>` (or `Result<Option<String>, _>`) for the lookup instead of
  `EnvValue`.** Same information, but every call site then reads
  `lookup(name)?` with two error currencies mixed; `EnvValue` names the three states in the
  vocabulary the tool actually reasons in (`Absent` is the only one that may fall through).
  Cost of the rejected form: identical line count, worse call sites — this is a naming
  choice, not a capability one.
* **Leave `run_ci`'s guard loop inline (iteration 2's `for guard in HYGIENE_GUARDS`).**
  It is 3 lines shorter. It also leaves *both* wiring failure modes untestable: with the
  loop in the binary target, `HYGIENE_GUARDS = &[]` and `let _ = (guard.run)();` are each
  invisible to every integration test (verified: the two refutations in §3 that catch them
  are precisely the two `run_hygiene_guards` tests). The brief's criterion 1 exists because
  a guard that never runs passes everything else silently.
* **Generalise the guard beyond `wyrd-validate`** (open question 2): done, as far as it is
  free — `BLACKBOX_GUARD_TARGETS` (`xtask/src/repo_guard.rs:574`) is a
  `(package, forbidden prefix)` table with one row, and `run_blackbox_dependency_guard`
  iterates it, so a second blackbox tool is a one-line reviewed diff. Not extended to any
  other package (explicitly out of scope in the brief).
* **Add `clap` / any third-party crate.** Refused per the brief's *External dependencies*
  (an ADR-0003 §2 audit is a human-only decision). The parser is hand-rolled in
  `ParsedArgs`' shape (base `crates/server/src/cli.rs:2495-2532`) with matching "a flag
  needs a value" wording; the one deliberate divergence (strict unknown-flag rejection,
  in both token positions) is scope item (f).
* **`cargo metadata --no-deps` like the unsafe-forbid guard** (base
  `xtask/src/main.rs:1453`). Cheaper (no lockfile read), and wrong: `--no-deps` omits the
  `resolve` section entirely, so the transitive closure — the thing that actually links —
  would be invisible. `BLACKBOX_METADATA_ARGS`' doc records why each flag is load-bearing.

## 5. Gate evidence, and one thing the human must weigh

`./engine/xtask.sh ci` (the configured C4-ci gate; runs `cargo xtask ci` inside
`$PDCA_WORKTREE`) was run over the finished tree. Everything the patch touches is green:

* `xtask blackbox-dependency-guard (proposal 0017 §9 …)` runs **inside the gate**, between
  the two #616 guards and `cargo fmt`, and prints
  `every validated package's normal dependency closure stays blackbox` — criterion 1
  demonstrated end to end, not only asserted;
* `cargo fmt --all -- --check`, `cargo clippy --workspace --exclude wyrd-dst --all-targets`
  (workspace lints are `warnings = "deny"`, `clippy.all = "deny"`), `cargo build`,
  `cargo test --workspace --exclude wyrd-dst` (incl. `cli_surface` 29/29 and
  `blackbox_dependency_guard` 21/21), `cargo machete`, `typos`, `lint_docs.py`, statics,
  orchestrator guard and DST — all pass.

**`cargo deny check advisories` fails, and it is not this patch.** RUSTSEC-2026-0258
(`h2 0.4.15`, "unbounded empty DATA frames", low severity, patched in 0.4.16) is reported
against the **base** lockfile: `git show HEAD:Cargo.lock` already pins `h2 0.4.15`
(line 1535-1536), this patch's only lockfile change is the four-line `wyrd-validate`
stanza, and `wyrd-validate` has no dependencies at all — the advisory's own dependency tree
names `aws-smithy-http-client`/`hyper`/`axum`/`tonic` under `wyrd-server`. The previous
iteration's frozen gate log shows this leg green ~11 hours earlier
(`iteration-v2/gate-logs/C4-ci.log:2891` — `advisories ok`), so the advisory entered the
RustSec database between the two runs; `main` fails the same way today, with or without
this bundle. **Not fixed here on purpose**: bumping `h2` is an unrelated dependency change
outside this PR's stated scope (AGENTS.md §Reviewer protocol — "a real finding outside the
PR's stated scope gets a decline-with-issue-reference, not an in-PR fix"), and it would put
an unreviewed lockfile bump inside a crate-skeleton slice. Suggested disposition at
sign-off: file it as its own issue (`cargo update -p h2` → 0.4.16) and read this bundle's
C4-ci row against everything before the deny leg.

`C5-mutants` will again report `cargo test failed in an unmutated tree`. Cause (unchanged
from iterations 1–2, and independent of this patch): cargo-mutants copies the tree without
`.git`, and the pre-existing `xtask/tests/repo_hygiene_guards.rs:129`
`scan_gitlinks_is_green_over_the_real_index` shells `git ls-files -s -z`, which cannot run
there (`iteration-v2/gate-logs/C5-mutants.log:443-466`). Any patch touching `xtask` pulls
that suite into the mutants baseline. Neither new test file depends on the git index; the
guard's real-workspace case shells `cargo metadata`, which works in a copied tree.

## 6. Manual validation (if the human wants to see it by hand)

```sh
cd $PDCA_WORKTREE
cargo run -q -p wyrd-validate -- --endpoint http://127.0.0.1:9000 --region us-east-1 \
  --bucket b --scenario smoke --duration 30s --workers 4 --seed 7 --out /tmp/out \
  --run-id r1 --driver-placement external            # exits 1: names what to set
AWS_ACCESS_KEY_ID=AKIA AWS_SECRET_ACCESS_KEY=s3cr3t cargo run -q -p wyrd-validate -- …same…
                                                     # exits 0: ten quoted fields + the id, never the secret
AWS_ACCESS_KEY_ID=AKIA WYRD_S3_ACCESS_KEY=w WYRD_S3_SECRET_KEY=x cargo run -q -p wyrd-validate -- …
                                                     # exits 1: names AWS_SECRET_ACCESS_KEY, never uses `w`
# the boundary, live:  add `wyrd-core = { workspace = true }` to crates/validate/Cargo.toml
cargo xtask ci        # fails at blackbox-dependency-guard naming wyrd-validate -> wyrd-core
```

## 7. The brief's open questions, answered

1. **Guard naming.** Kept the placeholders, because they read correctly beside the two
   #616 guards: `run_blackbox_dependency_guard` sits with `run_gitlink_guard` /
   `run_unsafe_forbid_guard`, and the pure scan is `scan_blackbox_dependency_closure`
   beside `scan_gitlinks` / `scan_roots`. The gate step prints
   `xtask blackbox-dependency-guard`, matching `xtask gitlink-guard` / `xtask unsafe-guard`.
2. **Data table or hard-coded package?** Table — `BLACKBOX_GUARD_TARGETS`
   (`xtask/src/repo_guard.rs:574`), one row, iterated by the runner and by the
   real-workspace test. It cost the two lines the brief predicted.
3. **Strict unknown-flag rejection, if sign-off prefers parity with `wyrd`'s parser.**
   Delete the `REQUIRED_FLAGS.contains` branch (`crates/validate/src/lib.rs:119-125`) and
   drop `an_unrecognised_flag_exits_nonzero_naming_it` +
   `the_equals_spelling_is_refused_by_name_rather_than_half_parsed`
   (`crates/validate/tests/cli_surface.rs:169`, `:206`). Note that the *value-slot* refusal
   (`crates/validate/src/lib.rs:129-136`) is a separate branch and should stay either way —
   it is what stops
   `--endpoint --region us-east-1` resolving to "endpoint = `--region`".

## 8. Scratch / hygiene

Working files were kept under `$PDCA_SCRATCH`
(`/var/tmp/pdca/wyrd-pdca-9c587031/issue_740/pdca-builder-740-*`: the metadata sample used
to verify the document shape, the three gate logs, and the pre-refutation backups of the
three source files). Nothing was written outside `$PDCA_WORKTREE` and the bundle directory.
No `NEEDS-HUMAN external dependency` items: the base Rust toolchain was sufficient, exactly
as the brief predicted.
