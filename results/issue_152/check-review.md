# Check review — issue 152 / docs-readme-dev-testing-section

> Advisory, artifact-only, decorrelated from the builder. Inputs: `patch.diff`,
> `brief.md`, `check-gates.json` (build-notes.md withheld). Citations re-derived
> against the target wyrd checkout (`xtask/src/main.rs`, `crates/server/src/cli.rs`,
> `crates/server/Cargo.toml`, `README.md`), read-only.

## Verdict table

| Item | Verdict | Basis |
|------|---------|-------|
| C1 — C1 Spec | PASS | `brief.md:16-25` gives a well-formed success criterion with four enumerated, inspectable deliverables (a–d) and an explicit verification method; scope and out-of-scope are stated (`brief.md:28-29`). |
| C2 — C2 Reproduction (red pre-fix) | PASS | The guard's first assert requires `## Development & testing` (`patch.diff:78-81`); the pre-fix README had only `cargo build`/`cargo test` at lines 77-82 (`README.md:77-82`), so the test is red on the unfixed tree. Inferred by inspection — no gate ran it red (`check-gates.json:15-21` C2 "none"). |
| C3 — C3 Change | PASS | `patch.diff:9-38` adds the section delivering all four brief items: xtask entry-point table, Docker/compose prereq, `WYRD_DSERVER_COUNT`, and the `wyrd demo` try-it line — every claim re-verified accurate against the target (see C5). |
| C4 — C4 Verification (red→green) | PASS | Gating gate green: `check-gates.json:33-39` records `xtask ci` pass; `run_ci` runs `cargo test --workspace` (`xtask/src/main.rs:344`), which covers the new `xtask/tests/readme_dev_section.rs`. |
| C5 — C5 Causal adequacy | PASS | Defect is a doc gap; fix fills it and each documented fact matches the target: subcommands `ci/conformance/dst/integration/bench` (`xtask/src/main.rs:29-34`), `ci` membership + `integration` exclusion (`main.rs:323-350`, `88-101`), `WYRD_DSERVER_COUNT` default 9/min 2 (`main.rs:103-107,75`), and `wyrd demo` via pkg `wyrd-server`/bin `wyrd` (`crates/server/src/cli.rs:59,280-308`; `crates/server/Cargo.toml:2,12`). Root cause uncontested. |
| T1 — T1 Structure | PASS | Standard Rust integration test resolving paths from `CARGO_MANIFEST_DIR` (`patch.diff:60-70`), one focused `#[test]` with descriptive assert messages (`patch.diff:72-133`). |
| T2 — T2 Shape | PASS | Asserts the right invariant: each README command is documented **and** backed by real dispatch (`patch.diff:85-93`), plus env knob and `-p/--bin/--demo` cross-checks — a drift guard, not a tautology over the README alone. |
| T3 — T3 Runtime | PASS | Every assertion resolves true on the fixed tree: literals `Some("ci")`…`Some("bench")` (`xtask/src/main.rs:29-34`), `Some("demo")` (`cli.rs:59`), `name = "wyrd-server"`/`name = "wyrd"` (`Cargo.toml:2,12`), `WYRD_DSERVER_COUNT` (`main.rs:103`); run green under the `ci` gate (`check-gates.json:33-39`). |
| T4 — T4 Contribution | PASS | Genuine coverage gain: the root `README.md` is outside `cargo xtask ci` (Rust-only) and the `docs/` linter, so this is the only thing pinning the on-ramp to the live command surface (`patch.diff:49-55`). |
| T5 — T5 Judgment | NEEDS-HUMAN | Ambiguous scope: brief scopes to "README additions only" (`brief.md:28`) and names "deterministic inspection" as the method (`brief.md:20-23`), but the patch also adds a Rust test `xtask/tests/readme_dev_section.rs` (`patch.diff:43-47`). Defensible discipline, but the scope expansion is a human call. |
| V — Validation — fitness-to-purpose | NEEDS-HUMAN | Whether the new section actually serves the newcomer on-ramp (tone, placement after "Repository layout", completeness of the dev story) is a fitness judgment reserved for human sign-off (`check-gates.json:95-102`). |

## §6 — Items the human must clear

1. **T5 / scope (ambiguous scope).** The brief constrains the work to "README
   additions only" and anticipates verification "by deterministic inspection,"
   yet the patch adds a Rust regression test at `xtask/tests/readme_dev_section.rs`.
   The test is accurate and valuable (it is the only guard over the root README),
   and CLAUDE.md asks a change to ship with the means to verify it — but adding a
   Rust file exceeds the brief's literal scope and contradicts its "touches no
   Rust" note (`brief.md:24-25`). Confirm the expanded scope is acceptable, or
   re-scope.
2. **V / validation fitness-to-purpose.** Confirm the section reads as an
   effective on-ramp for the intended newcomer audience (clarity, ordering,
   whether anything material in the dev workflow is still missing). Artifact
   inspection cannot settle fitness-to-purpose.

## Notes (non-gating)

- Factual accuracy of the documented surface is high — `WYRD_DSERVER_COUNT`
  default/min, `ci` membership of conformance+dst, `integration` exclusion, and
  the `demo` invocation all match the target source exactly.
- `[ADR-0016]` and `[\`cargo xtask\`](xtask)` references resolve against the
  existing link definition (`README.md:101`) and the `xtask` directory.
- Grounding caveat: `$PDCA_TARGET` could not be read in this sandbox (env access
  denied); citations were grounded on the wyrd checkout provided as an explicit
  working directory, treated as the target tree. If that is not the intended
  target, re-ground C2–C5/T3 before relying on the path:line basis.
