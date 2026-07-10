# Build notes — issue 439 / fdb-dev-ci-harness (iteration 4)

## Scope of this iteration

The iteration-3 design **survived the primary reviewer every time**; the adversarial pass
reopened the bundle on exactly two seams, which the re-plan (brief §"Re-plan resolution")
pins by their binding property. I therefore **kept the iteration-3 patch verbatim** for
everything that survived attack (brief §"What SURVIVED adversarial attack") and rebuilt
only the two named seams. Concretely, versus `iteration-v3/patch.diff` the *only* deltas
are inside `xtask/tests/fdb_harness.rs`:

- **Seam 1 (assertion 3 — workflow ↔ dispatch).** Removed the `tokens.windows(3)` scan
  (`iteration-v3` `xtask_subcommands`, the adversary's finding at `check-advisory-adversary.md:9`)
  and its `strip_shell_comment` machinery. Replaced with:
  - `xtask_head_subcommand` — binds on the **command HEAD** only (first shell-word
    sequence: `cargo`, then `xtask`, then the sub). No window scan.
  - `dispatched_subcommands` — scrapes the real `Some("<sub>") =>` arms of `main.rs`'s
    dispatch table (the `readme_dev_section.rs:41` pattern) and cross-checks every workflow
    head against that compiled set.
  - `the_workflow_head_scrape_is_red_when_the_conformance_step_is_evaded` — the
    demonstrated-red enumerating all four evasion shapes (i full-line comment, ii trailing
    inline comment, iii mention-as-argument, iv no-op builtin prefix) as explicit cases.
- **Seam 2 (assertion 5 — impure-adapter totality).** Added two structural body-assertions
  (the `function_body` idiom the brief sanctions) pinning the two named mutations:
  - `the_client_library_adapter_supplies_the_real_environment_read` — asserts
    `probe_client_library`'s body carries `std::env::var` + `Path::new(...).exists()`.
  - `the_conformance_preflight_is_handed_the_real_measured_probes` — asserts
    `run_fdb_conformance`'s body passes `docker_available()`, `is_ci()`,
    `probe_client_library()` and contains **no** hardcoded `Outcome::…`.

No source-behaviour change was needed: the iteration-3 `main.rs` adapters were already
total pass-throughs (`main.rs:391-403`, `:309-318`); the gap was purely that nothing at
Check *pinned* that totality. The re-plan's guidance ("the guard is cheap because the
adapter is genuinely trivial … pinning its totality is sufficient") is exactly what these
two assertions do — they do not re-unit-test `std::env::var`.

The artifact constraint (brief Scope (b): every `cargo xtask` call a bare single-command
`run:` step) was already satisfied by the iteration-3 workflow — `run: cargo xtask
fdb-conformance` (`:141`) and `run: cargo xtask ci` (`:152`). No `&&`, no `bash -c`, no
`run: |` wrapping any xtask call. The two `cargo check` PR-leg rows sit in a `run: |` block,
which is allowed (they are not xtask invocations).

## Why command-HEAD, not a cleverer comment-stripper

Iterations 1–3 each reached for a narrower parser of unconstrained shell (comment
stripping, quote handling) and each left a residual hole. The head approach is
categorically different: it does not try to *understand* the whole line, it only reads the
**first token**. Every evasion the adversary named puts a non-`cargo` word first
(`echo …`, `: …`, `# …`), so none can ever be a head. Cost of the discarded alternative is
not "heavier" hand-waving — it is a whole class of parser bugs (the three prior iterations),
whereas the head predicate is 8 lines and has no line-interior logic to get wrong.

## Forced refutation (brief §"refute your own test") — recorded evidence

**(a) Genuine red?** Yes — proven for each seam by reverting/mutating and re-running:
- Mutation A (`&|name| std::env::var(name).ok()` → `&|_| None`, main.rs:393) →
  `the_client_library_adapter_supplies_the_real_environment_read` **FAILED**.
- Mutation B (preflight arg `probe_client_library()` → `fdb_doctor::Outcome::ok("forced")`,
  main.rs:315) → `the_conformance_preflight_is_handed_the_real_measured_probes` **FAILED**
  (panic showed the mutated body).
- Evasion iii (workflow `run: cargo xtask fdb-conformance` → `run: echo would run cargo
  xtask fdb-conformance`) → `the_fdb_conformance_workflow_runs_only_real_dispatched_subcommands`
  **FAILED** ("Executed heads: [\"ci\"]"). Under the old `windows(3)` scan this stayed green
  — that is the regression now caught.
- Whole patch reverted → the test file does not compile (no `fdb_doctor` module) → red, per
  the brief's falsifiability note. All four evasion shapes are additionally asserted red at
  the unit level (`a_bare_xtask_step_is_a_head_but_every_evasion_shape_is_not`,
  `the_workflow_head_scrape_is_red_when_the_conformance_step_is_evaded`).

**(b) Production path?** Yes. The tests drive the *production* code: `xtask::fdb_doctor`'s
real functions, `xtask::feature_gated_checks`, and the actual `main.rs` adapter bodies read
off disk via `function_body(&read("xtask/src/main.rs"), …)`. The dispatch cross-check reads
the real `main.rs` table and the real `.github/workflows/fdb-conformance.yml`. No copy,
mock, or re-implementation.

**(c) Fixture includes the fault?** Yes. The demonstrated-reds construct workflows that
*contain* the evasion (the failing element), not curated-clean ones; the planted-probe test
injects a real `Outcome::failed` for each row; the mutation legs above physically edited the
production files. Nothing is curated out.

## Supplementary live leg (brief requires it; host has the deps)

Host verified: `docker info` OK; `/lib/libfdb_c.so` = 7.3.77 (23 MB); `fdbcli` = v7.3.77 —
byte-matching the compose pin `foundationdb/foundationdb:7.3.77`.

Ran `./engine/xtask.sh fdb-conformance` (i.e. `cargo xtask fdb-conformance`) end-to-end in
`$PDCA_WORKTREE`. It built the `fdb` feature (linking the real `libfdb_c`), brought up
`deploy/fdb-single-node`, ran `configure new single memory`, wrote the host cluster file,
and drove all legs green:
- conformance suite: pass
- contention (3 tests): pass
- scan (2 tests): pass
- timeout (3 tests): pass
- teardown: `wyrd-fdb-m4-fdb-1` removed; final line
  "xtask fdb-conformance: FoundationDB passed the shared MetadataStore conformance suite and
  the contention properties".

This exercises the real production preflight (`run_gated_conformance` → `probe_client_library`
→ container stack) that the container-free harness test can only drive with fake effects.

## Gate / commit-readiness

- `cargo test -p xtask --test fdb_harness`: **28 passed** on the plain worktree (no Docker,
  no libfdb_c needed for the binding criterion).
- `cargo test -p xtask --lib --bins`: 19 passed (includes the `run_ci_steps` toolchain-
  independence wiring test that survived the adversary).
- `cargo fmt -p xtask -- --check`: clean (rustfmt applied to the new test file).
- `cargo clippy -p xtask --all-targets -- -D warnings`: clean.
- Full `cargo xtask fdb-conformance` live leg: green (above).

## Out of scope, carried forward unchanged

- `configure_fdb_database`'s readiness poll gating on `status.success()` (Act candidate,
  #438-origin) — not touched here per brief carry-forward.
- No new Cargo dependency; workflow asserted by plain-text/substring + head checks. Audit
  note lives in `deny.toml:1-6` header only; no `docs/design/` write (441 owns that in-wave).
</content>
</invoke>
