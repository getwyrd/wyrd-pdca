# Build notes — #744 validate: derived payloads, size classes, tuning flags (iteration 6)

Target: getwyrd/wyrd @ main, base **`4bac538`** (unchanged from round 5). Every `path:line` below
is in the patched tree (`$PDCA_WORKTREE`) unless marked "base". This round starts from the
round-5 patch (`iteration-v5/patch.diff`, which the sign-off accepted "as is" apart from three
items) and changes exactly what the round-5 carry-forward names. The design, the construction
and the known answers are v1's (`iteration-v1/build-notes.md`); rounds 2 to 5 are summarised
in `iteration-v5/build-notes.md`.

**What changed against round 5, in one paragraph.** `crates/validate/src/sizes.rs` gains one
mirrored constant, `MAX_ROOT_VALUE_BYTES_CAP = 50_000` (`sizes.rs:52-58`), one refusal,
`TuningError::RootBudgetOutOfRange` (`:141-143`, message `:180-187`), and one check,
`check_root_budget` (`:252-261`), run in `Tuning::resolve` before the threshold is derived
(`:284-286`) and in `Tuning::check` (`:321`). A root budget of zero or above the cap is now
refused by flag name whether or not a threshold is given, for flags built by hand too; a positive
budget too small to derive from is still accepted beside a given threshold. `lib.rs` re-exports
the constant (`lib.rs:37-41`). The test file pins the new refusals at both levels, adds the cap
to the (F) drift check, and re-homes the two derived-overflow cases the cap made unreachable. The
architecture paragraph (`docs/design/architecture/05-building-block-view.md:297`) states the cap,
conditions the one-chunk-record refusal on a derived threshold, and justifies the 10 GiB + 1
ceiling from the cap instead of asserting it. `Cargo.toml`, `Cargo.lock`, `args.rs` and
`payload.rs` are byte-identical to round 5.

## What round 5 asked for, and what changed

### 1. Zero root budget: refused by flag name even when a threshold is given

Round 5 refused a zero budget only through `derived_segment_threshold` (`NoChunkRefFits`),
which runs only when no threshold is given; `TuningFlags { max_root_value_bytes: Some(0),
segment_threshold_bytes: Some(..) }` resolved. Now:

- `check_root_budget` (`sizes.rs:252-261`) accepts `1..=MAX_ROOT_VALUE_BYTES_CAP` and refuses
  everything else as `RootBudgetOutOfRange { max_root_value_bytes }`.
- `resolve` calls it right after the chunk-size check and **before** the threshold is derived
  (`:284-286`), so a zero or over-cap budget is refused as a value no deployment has, not as one
  that derives nothing. That changes one answer for a hand-built zero with no threshold:
  `(None, Some(0), None)` is now `RootBudgetOutOfRange { 0 }` where round 5 said
  `NoChunkRefFits { 0 }` (the command line never reaches either: the parser refuses `0`
  first, `args.rs`, unchanged; pinned at `payload_and_sizes.rs:1118-1119`).
- `check` calls it too (`:321`), so a `Tuning` built by hand with a zero budget beside a given
  threshold is refused by `size_classes` (pinned `payload_and_sizes.rs:931-936`), the contract
  the `TuningFlags` docs state (`sizes.rs:111-113`).
- The positive-but-small budget stays accepted with a threshold given:
  `a_given_threshold_is_used_exactly_whatever_the_root_budget` now runs budgets
  `[None, 31_500, 200, 1, MAX_ROOT_VALUE_BYTES_CAP]` (`payload_and_sizes.rs:772-778`), which
  also pins both ends of the range inclusive.
- Public-API rows for the refusal: zero and cap + 1, each with and without a threshold, and
  `u64::MAX` (`:885-898`).

**One variant for both ends, not two.** `RootBudgetOutOfRange` covers zero and above-cap the
way `ChunkSizeOutOfRange` covers both chunk-size bounds; its message names the flag, the value
and the range ("1 to 50000 bytes, inclusive"), so a zero reads as below the floor. A separate
`ZeroRootBudget` variant would cost one more variant (3 lines), one more `Display` arm (~8
lines) and one more test closure, and would say nothing the range message does not. Round 2
removed a `TuningError::Zero` for the same reason.

### 2. The architecture paragraph (`05-building-block-view.md:297`)

Two sentences changed, nothing else in the paragraph:

- The refusal list now reads "... a root budget of zero or above 50,000 (half the 100,000-byte
  value limit every backend inherits from FoundationDB; a bigger budget needs that limit
  raised), a given threshold below one chunk or not a whole number of chunks, a budget too small
  to hold one chunk record **when the threshold is derived from it** (with the threshold given,
  any budget from 1 to the cap is accepted), or a size the table needs that does not fit in 64
  bits." Round 5 stated the one-chunk-record refusal without the condition.
- The ceiling sentence, round 5: "At any accepted chunk size with a derived threshold they run
  from 0 B to 10 GiB + 1; only a given `--segment-threshold-bytes` above 10 GiB moves the `T`
  classes past that." That was false with an unbounded budget (`--max-root-value-bytes
  18446744073709551` derived a `T` far past 10 GiB). Now: "With the budget capped at 50,000 and
  the chunk size at most 16 MiB, a derived threshold is at most 158 × 16 MiB (under 2.5 GiB),
  so at any accepted tuning with a derived threshold the table runs from 0 B to 10 GiB + 1;
  only a given `--segment-threshold-bytes` above 10 GiB moves the `T` classes past that." The
  arithmetic: ⌊50,000 / 315⌋ = 158; 158 × 16 MiB = 2,528 MiB ≈ 2.47 GiB, below the fixed 5 GiB
  class, so `T + 1` is below 5 GiB and the top of the table is the fixed 10 GiB + 1 class. The
  derived `T` cannot exceed that bound because `resolve` refuses any budget above the cap and any
  chunk size above 16 MiB before deriving.

### 3. Cap `--max-root-value-bytes` at 50,000; refuse above it by name, never clamp

- `MAX_ROOT_VALUE_BYTES_CAP: u64 = 50_000` (`sizes.rs:52-58`), mirroring
  `wyrd_core::metadata::MAX_ROOT_VALUE_BYTES` (base `crates/core/src/metadata.rs:574`), which is
  half the FoundationDB value limit `MAX_VALUE_BYTES = 100_000` (base `:549`) by the core's own
  `const` assertion (base `:576`). A bigger budget needs that limit raised, so no deployment has
  one. Tied at compile time (`sizes.rs:76-81`): `CHUNK_REF_BYTES ≤ DEFAULT_MAX_ROOT_VALUE_BYTES
  ≤ MAX_ROOT_VALUE_BYTES_CAP`, so the no-flags tuning is always one `resolve` accepts.
- Refusal, never a clamp: `check_root_budget` returns `Err`; nothing in the module rewrites a
  value.
- (F): `assert_eq!(MAX_ROOT_VALUE_BYTES_CAP, wyrd_core::metadata::MAX_ROOT_VALUE_BYTES as u64)`
  (`payload_and_sizes.rs:1313-1317`), beside the existing default check.
- CLI: `--max-root-value-bytes 50001` with the whole message (`:1178-1184`), and `50001` beside
  `--segment-threshold-bytes 1048576` (`:1185-1192`), so the cap is shown to hold with a
  threshold given; `18446744073709551615` as a budget is now a range refusal (`:1235-1239`,
  and `each_bad_tuning_value_is_refused_by_name`, `:1137`).

**A second constant, not the default reused as the cap.** Both mirror the same core constant
today, but they are two roles: the value an absent flag takes, and the largest value a given
flag may take. #739 makes the budget deployment configuration on the server side; when a
deployment runs below the ceiling the tool's default may follow it while the cap stays at the
backend limit. Reusing `DEFAULT_MAX_ROOT_VALUE_BYTES` as the cap costs 0 new lines but makes
(F) check one number for two meanings and ties the default to the ceiling by name; the second
constant costs 7 lines of doc + 1 line, 1 re-export, 1 (F) assertion.

**What the cap does to the overflow paths.** With budget ≤ 50,000 and `C ≤ 16 MiB`, the
derived `T` is at most 158 × 16 MiB, so `TuningError::Overflow` is no longer reachable through
`resolve` for a derived threshold. `derived_segment_threshold` is `pub` and keeps its checked
multiply, and `size_classes` keeps the derived `t_from` branch; both are now pinned through the
public API directly (`derived_segment_threshold(MIB, u64::MAX)` → `Overflow { "T (...)",
derived }`; a `Tuning` built by hand with `segment_threshold_bytes: u64::MAX, Source::Derived`
at `ODD_CHUNK` → `Overflow { "T + 1", derived }`; `payload_and_sizes.rs:937-949`) instead of
through `resolve`, so no diff-coverage line is lost. `MAX_DERIVED_BUDGET` is gone (its only use
was the CLI row the cap turned into a range refusal; the `50001`-with-threshold row takes its
place, the table stays at 11 rows: `:1168`). The one overflow `resolve` can still reach — a
given threshold of `u64::MAX` at a chunk size that divides it — is unchanged (`:914-917`,
`:1240-1248`).

### Test changes, all forced by the three items

| test | change |
|---|---|
| `a_given_threshold_is_used_exactly_whatever_the_root_budget` (`:764`) | budgets `1` and the cap added |
| `a_tuning_no_deployment_can_have_is_refused_not_clamped` (`:851`) | `budget` closure; five range rows; `u64::MAX` budget → range; by-hand zero `Tuning`; the two derived-overflow cases re-homed |
| `each_bad_tuning_value_is_refused_by_name` (`:1110`) | `u64::MAX` budget → "is outside the range a deployment can have" |
| `a_tuning_no_deployment_can_have_is_refused_at_the_command_line_by_flag_name` (`:1163`) | `50001` × 2 added, `MAX_DERIVED_BUDGET` row removed, `u64::MAX` budget row → range |
| `the_mirrored_defaults_match_the_core_constants` (`:1305`) | cap vs core constant |

26 tests in the file (round 5: 26), every `#[test]` body opening with `bounded(`.
`cli_surface.rs` is not in the patch and passes 21/21.

### Size

`patch.diff` is 121,798 bytes (round 5: 116,439; +5,359). The growth is the three items:
`sizes.rs` +42 lines (constant and its doc, variant, message, check, two calls, docs),
`lib.rs` +1, the test file +47 lines, the docs line (one line, so its whole old and new text
is in the diff: 10.6 KB of the total, unchanged in size). The sign-off said "fix exactly these
three things, nothing else", so I did not trim other tests to compensate.

### Not addressed: the deferred findings

`deferred-findings.json` holds the items "for the human at handover — not build work". Left
alone, as in rounds 4 and 5.

### Not changed: C4 verification posture

The test file still cannot compile on the base (base `crates/validate/src/lib.rs:20-22`
declares only `access_keys`, `args` and `s3`; base `Cargo.toml` has no `sha2`), so C4-verify
stays UNVERIFIABLE (exit 77), as the brief pre-declared.

## Demonstrated reds

Each a temporary text replacement applied by a driver
(`$PDCA_SCRATCH/pdca-builder-744-v6/run_mutants.py`), the new test file run with `timeout 600
cargo test -q -p wyrd-validate --test payload_and_sizes`, the file restored from a snapshot and
verified byte for byte (`filecmp`; afterwards `cmp` against pristine copies of the four tool
files, `git diff --quiet -- crates/core/ crates/server/` empty, and `git diff 4bac538` equal to
`patch.diff`). Per-mutant logs: `$PDCA_SCRATCH/pdca-builder-744-v6/mutant-NN.log`; table:
`mutants.md` beside them. Rows 1 to 6 are the brief's six required reds, re-run on this round's
tree (the lines they mutate are byte-identical to round 5); 7 to 11 are this round's.

| # | Mutant | Failed tests | Evidence |
|---|---|---|---|
| 1 | brief (B): `keyed.update(generation…)` removed (`payload.rs:128`) | 3: (B) `changing_any_one_input_changes_the_bytes`, (A), (C′) | 23 passed, 3 failed, 1.4 s |
| 2 | brief (D): `let c = DEFAULT_CHUNK_SIZE;` in `size_classes` (`sizes.rs:345`) | 2: `a_different_chunk_size_moves…`, `two_classes_that_coincide…` | 24/2 |
| 3 | brief (D): `DEFAULT_MAX_ROOT_VALUE_BYTES / CHUNK_REF_BYTES` in `derived_segment_threshold` (`:231`) | 4: `only_the_root_budget_changed…`, both refusal tests, the binary echo | 22/4 |
| 4 | brief (C): `fill_inside(offset + 1, buf)` (`payload.rs:165`) | 3: `any_range_produced_directly…`, `split_pieces…`, (C′) | 23/3 |
| 5 | brief (C′): hash blocks `0..offset/32` at the top of `fill_inside` (`:170`) | 1: `the_last_bytes_of_a_huge_payload_are_produced_directly` | `did not finish within 5s`; whole run 6.0 s, no hang |
| 6 | brief (E): echo without `({source})` (`lib.rs:75-76`) | 3: `absent_tuning_flags…`, `given_tuning_flags…`, the binary echo | 23/3 |
| 7 | item 1/3: `check_root_budget` call removed from `resolve` (`sizes.rs:286`) | 3: both refusal tests, `each_bad_tuning_value…` | first row: `(None, Some(0), None)` answers `NoChunkRefFits` not `RootBudgetOutOfRange`; CLI `50001` with a threshold accepted (`left: 3, right: 2` = INCONCLUSIVE, not USAGE) |
| 8 | item 1: `check_root_budget` call removed from `check` (`:321`) | 1: `a_tuning_no_deployment_can_have_is_refused_not_clamped` | the by-hand zero `Tuning` returns `Ok([SizeClass …])` (`:936`) |
| 9 | item 1: floor dropped, `max_root_value_bytes <= CAP` (`:254`) | 1: the same | `(None, Some(0), None)` → `NoChunkRefFits` (zero slipped past the range check into derivation) |
| 10 | item 3: tool cap raised to 100,000 (`:58`) | 2: (F) `the_mirrored_defaults_match_the_core_constants`, the CLI refusal test | `50001` accepted (`left: 3, right: 2`); cap ≠ core constant |
| 11 | item 3 / brief (F): **core** `MAX_ROOT_VALUE_BYTES` lowered to 40,000 (base `crates/core/src/metadata.rs:574`) | 2: (F) `the_mirrored_defaults…` (`left: 50000, right: 40000`), `the_default_chunk_size_matches_the_servers` | the tool's default and cap both drift from the core at once; the default gateway's single-PUT limit moves with `MAX_MAP_CHUNKS` |

Round 5's chunk-size range mutants (7 to 12 there) are not re-run: the lines they mutate are
byte-identical to round 5 and the tests that caught them are unchanged.

## Refute-your-own-test

- **(a) Genuine red?** Yes. Each of the eleven mutants above fails at least one named test and
  the run completes (no hang; the slowest, the prefix regeneration, ends at the 5 s bound). The
  three items' own reds are 7 to 11: removing either call to the new check, dropping the floor,
  moving the tool's cap, or moving the core constant each fails a named test. The whole patch
  reverted does not compile (net-new API; see C4 posture).
- **(b) Production path?** Yes. Every test calls the crate's public API (`Tuning::resolve`,
  `size_classes`, `derived_segment_threshold`, `run`, the real binary). The (F) probes drive
  the production server: `Gateway::new` + `put_object` (round 4) and
  `wyrd_server::cli::serve_s3_role` (round 5), unchanged; the cap is compared with the core
  constant itself. No stand-in, no copied constant, no re-implementation.
- **(c) Fixture includes the fault?** Yes. The reds inject the fault into production source on
  both sides — the tool's check, floor and cap, and the core's `MAX_ROOT_VALUE_BYTES` — and the
  refusal rows include the exact case round 5 found (zero beside a given threshold), at the
  public API and, for the cap, at the command line.

## Rubric self-review (`AGENTS.md` § Review rubric & protocol)

- One clock: no clock read anywhere in the patch.
- Narrow seams / dependency direction: `wyrd-core` and `wyrd-server` stay dev-dependencies;
  the binary links no `wyrd-*` crate (`cargo xtask blackbox-guard` in the gate).
- No shared mutable global state: only `const`s added.
- `#![forbid(unsafe_code)]`: no new crate root; the test file carries it.
- Docs currency: the cap, its reason and the conditioned refusal are in the `validate`
  paragraph (`05-building-block-view.md:297`), same patch; a CLI flag's accepted range changed,
  so this is the merge requirement, not a follow-up.
- Grammar strictness: the tuning values stay digits-only, no sign (`args.rs`, unchanged).
- Absent or unsupported entries: zero and over-cap budgets are explicit, named errors; nothing
  is clamped or skipped; the refusal runs whether or not a threshold is given.
- Test fidelity: the new refusals are pinned at the typed level and at the command line, with
  the whole message once per kind, through `run` → `RunError::Tuning` → stderr.
- Await discipline: no new await.

## Gates

- `cargo fmt -p wyrd-validate` applied before the first run; `cargo clippy -p wyrd-validate
  --all-targets -- -D warnings` clean; `typos crates/validate
  docs/design/architecture/05-building-block-view.md` clean; `git diff --check 4bac538` clean.
- `cargo test -p wyrd-validate --test payload_and_sizes --test cli_surface` (under
  `timeout 600`): 26/26 and 21/21.
- `patch.diff` = `git diff 4bac538` with the three new files intent-to-add; it applies to a
  fresh index read from `4bac538` (`git apply --cached --check`, index under scratch). The
  brief's test file is also at `results/issue_744/crates/validate/tests/payload_and_sizes.rs`,
  byte-identical to the worktree's.
- Full gate through the wrapper, `./engine/xtask.sh ci` (→ `cargo xtask ci` in the worktree),
  on the final tree, log `$PDCA_SCRATCH/pdca-builder-744-v6/xtask-ci-final.log` (3,344
  lines): **`xtask ci: all checks passed`, exit 0** (log `:3344`). Every step ran: `typos`
  (`:1`, no output), `lint_docs: OK` (`:5`), `render_site … link audit OK` (`:9`, covers the
  edited docs paragraph), gitlink-guard, unsafe-guard (`:15`), blackbox-guard ("wyrd-validate's
  normal dependency closure holds no wyrd-* crate", `:18`), `cargo fmt --all -- --check`
  (`:20`, no output), workspace clippy, build and test — both validate test files ran inside it
  (`cli_surface` 21 passed, `:2578`; `payload_and_sizes` 26 passed, `:2610`), no `FAILED` line
  anywhere in the log — cargo-machete (`:3141`), `cargo deny check` ("advisories ok, bans ok,
  licenses ok, sources ok", `:3145`), **`cargo deny --all-features --config
  deny-all-features.toml check advisories` → `advisories ok`** (`:3148`), `cargo deny
  --all-features check licenses bans sources` (`:3151`), conformance vectors, statics,
  deploy-guard, and the madsim DST clippy and test. After the gate, `git diff 4bac538` still
  equals `patch.diff` byte for byte, and `git status` shows only the eight files in the patch.
- The C5 gate's own command, `./scripts/mutants-in-diff` (→ `cargo mutants --in-diff
  patch.diff --no-shuffle --cap-lints=true` in the worktree, `PDCA_BUNDLE` set, `TMPDIR`
  under scratch), on the final `patch.diff`: "Found 107 mutants to test … Unmutated baseline
  in 24s build + 9s test … Auto-set test timeout to 48s … **107 mutants tested in 6m: 96
  caught, 11 unviable**" — 0 missed, 0 timeouts (round 5: 106 mutants, 95 caught, 11
  unviable; the one new viable mutant is `check_root_budget`'s body, caught). Log:
  `$PDCA_SCRATCH/pdca-builder-744-v6/cargo-mutants.log`. After it, `git diff 4bac538` still
  equals `patch.diff` (cargo-mutants works on a copy under `TMPDIR`).

## Runner note

The project's runner (`./engine/xtask.sh ci`) runs the whole gate only; it has no single-test
entry. For the per-mutant reds and the fast loop I ran `cargo test -p wyrd-validate --test
payload_and_sizes --test cli_surface` under `timeout 600` in the worktree, and the full gate
through the wrapper.

## Scratch

This round's throwaway files are under `$PDCA_SCRATCH/pdca-builder-744-v6/`: pristine source
copies (`pristine/`), per-mutant snapshots (`snapshots/`), the red driver and its logs, the
gate log, and a temporary index (`check.index`). Left for the harness to reclaim, per the
leaf's filesystem rules (no `rm` from this leaf).
