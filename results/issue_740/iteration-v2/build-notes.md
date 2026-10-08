# Build notes — issue 740 (iteration 2)

Target: `getwyrd/wyrd @ main`, base `a801997` in the cycle worktree
`/home/eddie/wyrd/wyrd.pdca-wt-l1`. Every `path:line` below is that tree with `patch.diff`
applied.

## What this iteration changes, and why

Iteration 1 shipped the crate, the CLI surface, the credential fallback and the
dependency-closure guard; Check kept it back on two things (brief `## Iteration 1 —
carry-forward`):

1. **C5 / T5 — the strict parser was strict in only one token position.** `--endpoint
   --totally-bogus` was consumed as *the endpoint's value*: exit 0, the typo echoed back as
   resolved configuration. That is precisely the misreported run the strict-flag decision
   exists to prevent (brief Design, scope item (f)), reached one slot to the left.
2. **T4 batch review — 3 blocking findings, all one class: docs currency.** Ten new CLI
   flags landed with no living-architecture update, which the repo's rubric makes a merge
   requirement, not a follow-up (`AGENTS.md:154-157`).

Everything else from iteration 1 is carried forward unchanged (the reviewer passed C1–C4,
T1, T2); the diff below is iteration 1 plus the two fixes plus the strictness holes that
sit in the same class as (1).

### 1. Strictness, closed in both token positions and at both input surfaces

`crates/validate/src/lib.rs`:

* `FLAG_PREFIX` (`:93`) is now **one constant used in both positions** — it is what makes a
  token a flag (`:99`) *and* what a value slot refuses (`:115`). The finding was a hole
  *between* two spellings of the same rule, so a single constant is the fix's shape, not
  just its content.
* A `--`-prefixed token in a value slot is an error naming **both** the flag left without a
  value and the offending token (`:115-122`).
* Two siblings of the same defect class, closed in the same pass rather than left for a
  round 3: an **empty** value (`:123-125`) and a **repeated** flag (`:126-131`). Each is
  "an operator's input silently becoming configuration that is not what they asked for".
* `resolve_credentials` (`:249-289`): **half a pair is an error**, not a fall-through to
  the other identity (`:269-282`), and an exported-but-empty name is an error naming it
  (`:259-262`). Same class at the other input surface: an operator who exported
  `AWS_ACCESS_KEY_ID` and misspelled the secret's name would otherwise have their run
  signed with the *gateway's* identity and reported as if it were the AWS one.
* Deliberately **not** tightened: `-` (single dash) still leads a legal value, because a
  negative `--seed -12345` is legitimate (`:75-78`, asserted at
  `crates/validate/tests/cli_surface.rs:234`). Tightening to `starts_with('-')` would look
  equally "strict" and would reject a legal invocation — the test exists so the next
  person cannot make that trade silently.
* Deliberately **not** added: per-flag *value vocabulary* validation (`--workers` as a
  count, `--driver-placement` as `internal|external`). The boundary is written down at
  `crates/validate/src/lib.rs:132-139`: the parser refuses tokens that are **not values at
  all**; what a value *means* is validated by the slice that consumes it (#741/#743),
  where the accepted set is defined. Cost of the alternative, concretely: `--duration`,
  `--workers`, `--seed` and `--out` have no consumer in this slice, so their accepted sets
  would be invented here and re-decided there — two definitions of the same vocabulary,
  and a `Config` that stops being "the strings the operator typed" (brief scope (b), which
  says *parsed, validated for presence, and echoed*).

### 2. Docs currency — the living architecture doc

`docs/design/architecture/05-building-block-view.md` (`status: living`):

* `:247` — a row in §5.3's cross-cutting table: blackbox validation, its own binary, links
  no `wyrd-*` crate (gate-enforced).
* `:253-276` — a new **§5.4**: why a separate binary and not a `wyrd` subcommand, the
  mechanical guard that keeps it that way, what exists today (parses/echoes, no S3 call
  yet), **the ten flags in a table with their meanings**, the strictness policy and why it
  differs from `wyrd`'s own parser, and the credential precedence including "half a pair is
  an error" and "the secret is never echoed".

Placement rationale: the rubric's docs-currency rule points at "the living architecture
doc" (`AGENTS.md:154-157`, `docs/design/README.md`'s class map via `AGENTS.md:88-99`).
`05-building-block-view.md` is the file that enumerates the crates and what each is for, so
a new binary + its flags belong there. The other candidates were rejected on fit:
`07-deployment-view.md` is about profiles/topology and the tarball question is #742's (the
tool is not shipped by this slice); proposal 0017 is a **draft design doc** and, per
`AGENTS.md:100-106`, implementation facts do not get back-patched into it. The proposal is
untouched by this patch, so `docs-immutability` has nothing to object to (its scope is
`docs/design/adr|proposals|specs`, `.github/workflows/docs-immutability.yml:253-256`).

Both docs legs ran for real on this patch, not warn-skipped: `lint_docs: OK` and
`render_site: link audit OK` (green run, ci log lines 4-9).

### 3. The guard now fails closed on every part of the closure it cannot read

Self-review against the rubric's *absent-or-unsupported → never a silent skip* class found
one hole in the guard I carried from iteration 1: the BFS did `let Some(node) = … else {
continue; }`. A package **reached by a normal edge** but carrying no resolve node was
skipped — and a skip one hop out hides every `wyrd-*` crate behind it
(`wyrd-validate -> middle -> wyrd-core` would have reported clean), which to the gate is
indistinguishable from a clean closure. That is the same failure mode the brief's
"fail closed … never a vacuously clean pass" exists to forbid, one hop from where it was
already enforced.

Now `Err` with the offending package and the path that reached it
(`xtask/src/repo_guard.rs:700-726`): a reached package with no node, a node with no `deps`
array, an edge naming no `pkg`, a `package` whose declared dependency list is missing
(`:747-754`), and a declared entry with no `name`. The same argument applies one field
further in, so `edge_is_normal` now returns `Result<bool, String>` (`:586-597`): an edge
with no readable `dep_kinds` array is `Err`, because "I cannot tell what kind this edge is"
is not "this edge is dev-only", and silently choosing the second drops the edge and
everything behind it. Three new planted cases bind these
(`xtask/tests/blackbox_dependency_guard.rs:181`, `:204`, `:227`) and go red without the
change — see (a) below. Doc comment updated to state the widened rule (`:626-631`).

That the stricter classification is safe on the *real* graph is not assumed: the
real-workspace case (`:118`) scans the whole ~700-package resolve under `--all-features`
and stays green, so every edge cargo actually emits carries a readable `dep_kinds`.

## Unchanged from iteration 1 (carried, not re-litigated)

* `crates/validate` as a workspace member (`Cargo.toml:30`), package `wyrd-validate` with
  **no `[dependencies]` table at all** (`crates/validate/Cargo.toml:11-14`) — zero new
  third-party crates, so no ADR-0003 audit rides along (brief External dependencies).
* Lib holding the pure decisions + a thin bin (`crates/validate/src/main.rs:9-20`); both
  roots `#![forbid(unsafe_code)]`.
* The guard: `scan_blackbox_dependency_closure` (`xtask/src/repo_guard.rs:632`) — pure
  function, `Result<Vec<String>, String>`; walks only normal (`kind: null`) edges
  transitively (`:586` `edge_is_normal`) **and** scans the feature-independent declared
  list so an optional off-by-default `wyrd-*` edge cannot hide. Invoked over
  `cargo metadata --format-version 1 --locked --all-features` (`:805-844`).
* Registration as **data in the lib target** (`HYGIENE_GUARDS`,
  `xtask/src/repo_guard.rs:870`) with `run_ci` iterating it (`xtask/src/main.rs:1559-1565`)
  — criterion 1's wiring proof, asserted at
  `xtask/tests/blackbox_dependency_guard.rs:239-252`. The `feature_gated_checks` precedent
  (`xtask/src/main.rs:1504-1505`) is what this mirrors.
* `BLACKBOX_GUARD_TARGETS` as a `(package, forbidden prefix)` table
  (`xtask/src/repo_guard.rs:574`) — brief open question 2, answered "table" because it cost
  one line.

## Refuting my own test (forced, recorded)

Every run below is the project's own runner, `./engine/xtask.sh ci` → `cargo xtask ci` in
`$PDCA_WORKTREE` (`pdca.toml [gates] runner`), never a hand-rolled invocation.

**(a) Genuine red?** Yes — demonstrated twice, by reverting production and keeping the
tests:

* *Parser strictness reverted* (the three branches at `lib.rs:115-131` replaced by the
  plain `flags.insert(...)` of iteration 1): `cargo xtask ci` **exit 1**, `test result:
  FAILED. 14 passed; 4 failed` — `a_flag_shaped_token_in_a_value_slot_is_refused_for_every_flag`,
  `a_flag_left_without_its_value_does_not_consume_the_next_flag`,
  `a_repeated_flag_is_refused_naming_it`, `an_empty_value_is_refused_naming_the_flag`. The
  first failure printed the exact defect the reviewer reported:
  `` `--endpoint --totally-bogus` must exit non-zero … stdout: resolved configuration:
  --endpoint --totally-bogus ``.
* *Credential handling reverted as well* (earlier red run, `resolve_credentials` back to
  fall-through): 6 failures, including
  `half_an_aws_pair_refuses_over_the_real_binary_instead_of_using_the_wyrd_identity`, whose
  output showed `credentials: access-key-id=wyrd-gateway-id-2b7` — the silent identity
  substitution, caught.
* *Guard verdict neutered* (`scan_blackbox_dependency_closure`'s `Ok(violations)` →
  `Ok(Vec::new())`, fail-closed paths left intact): `test result: FAILED. 7 passed; 2
  failed` — `a_normal_wyrd_dependency_is_exactly_one_violation_naming_it` and
  `an_optional_off_by_default_normal_edge_is_still_the_violation`. Criterion 2's planted
  cases bind the scan, not merely its existence.
* *Fail-closed widening reverted* (the `ok_or_else(...)` arms put back to `continue`):
  `test result: FAILED. 9 passed; 2 failed` —
  `an_edge_reaching_a_package_with_no_resolve_node_is_err` and
  `a_package_entry_without_a_declared_dependency_list_is_err`. The silent-skip form is
  therefore observably different from the fail-closed one, not a cosmetic tightening.
* *`edge_is_normal` reverted to the boolean form* (an unreadable `dep_kinds` classified
  non-normal instead of `Err`): `test result: FAILED. 11 passed; 1 failed` —
  `an_edge_with_no_dep_kinds_is_err_not_silently_non_normal`.
* Post-revert restore, full green: `xtask ci: all checks passed`, `cli_surface` 18/18,
  `blackbox_dependency_guard` 12/12, and the guard itself running inside `run_ci` (ci log:
  `$ xtask blackbox-dependency-guard …` / `every validated package's normal dependency
  closure stays blackbox`).

**(b) Production path?** Yes. `crates/validate/tests/cli_surface.rs` spawns the **real
compiled binary** via `env!("CARGO_BIN_EXE_wyrd-validate")` (`:28-30`) for every CLI and
credential case; the pure-function cases call `wyrd_validate::resolve_credentials` — the
same function `main` calls (`crates/validate/src/main.rs:11`) — over an injected lookup.
`xtask/tests/blackbox_dependency_guard.rs` drives
`xtask::repo_guard::scan_blackbox_dependency_closure`, the same function
`run_blackbox_dependency_guard` hands `cargo metadata`'s output to inside `cargo xtask ci`,
and additionally *invokes the registered callable itself* (`:251`), so a stub
registered under the right name would fail.

**(c) Fixture includes the fault?** Yes. The planted metadata documents **contain** the
violating edge (`wyrd-validate` → `wyrd-core`, `kind: null`), including the
optional/off-by-default variant that a default-feature resolve omits; the dev-marked
variant is the negative control. The CLI fixtures **contain** the offending token in every
one of the ten value slots, not a curated single case — a check written into one arm of the
parser fails the test. The credential fixture keeps a *complete* fallback pair available
while the first pair is half-stated, so "refuses" cannot pass by there being nothing to
fall back to.

## Gate observations for sign-off

* **A pre-existing flaky test, not this patch.** One `cargo xtask ci` run failed at
  `crates/gateway-s3/src/lib.rs:4259`
  (`tests::a_bodyless_response_is_recorded_complete_not_aborted`) — a crate this patch does
  not touch. Re-run immediately: green (`test tests::a_bodyless_response_is_recorded_complete_not_aborted
  ... ok`). Four other full-ci runs of this patch passed it. This is the class
  `pdca.toml`'s `confirm_gating_fail = true` exists for.
* **C5-mutants will error again, for a pre-existing reason.** `cargo mutants` copies the
  tree without `.git`, and `xtask/tests/repo_hygiene_guards.rs:137`
  (`scan_gitlinks_is_green_over_the_real_index`, a #616 test that predates this bundle)
  panics with `git ls-files -s -z must succeed`, so the row reports "cargo test failed in
  an unmutated tree". Iteration 1's reviewer classified it as harness topology; nothing in
  this patch changes it, and the new tests do not depend on a real git index.
* **C4-verify will record `PASS (green-only)`**, as the brief pre-declared: one of the two
  test files lives in a crate this patch *creates*, so `run-verify.sh` takes the
  `GREEN_ONLY` branch (`engine/scripts/run-verify.sh:405-412`). The load-bearing red is the
  one recorded under (a), inside `cargo xtask ci`.
* No external dependency beyond the base toolchain was needed; nothing is deferred to
  another environment. Everything in this slice compiles and runs under `cargo xtask ci`.

## Two decisions the human may want to overturn at §9

Both are one branch + one assertion to drop, in the style the brief itself offers for the
unknown-flag policy (brief:226):

1. **Duplicate-flag and empty-value refusal** (`lib.rs:123-131`, tests at
   `cli_surface.rs:264`, `:288`). These extend scope item (f) from "unknown flag" to "input
   that cannot be what the operator meant". I judged them the same defect the carry-forward
   routed back — the reviewer's finding was that strictness applied in one position only —
   and cheaper to close now than to meet again in a later round.
2. **Half a credential pair is an error** (`lib.rs:269-282`, tests at `cli_surface.rs:460`,
   `:493`). The brief binds four credential cases and is silent on the half-set one;
   falling through silently swaps the identity a run is attributed to, which the rubric's
   "never silent skip" class covers.
