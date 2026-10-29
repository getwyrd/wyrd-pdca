# Build notes — #744 validate: derived payloads, size classes, tuning flags (iteration 3)

Target: getwyrd/wyrd @ main, base `12dd082`. Every `path:line` below is in the patched tree
(`$PDCA_WORKTREE`) unless marked "base". This iteration starts from the round-2 patch
(`iteration-v2/patch.diff`, applied unchanged) and changes only what round 2's carry-forward
names. **Production code is byte-identical to round 2**: comparing the two patches file by
file, `Cargo.lock`, `crates/validate/Cargo.toml`, `src/args.rs`, `src/lib.rs`,
`src/payload.rs` and `src/sizes.rs` are the same; only
`crates/validate/tests/payload_and_sizes.rs` and the docs paragraph
(`docs/design/architecture/05-building-block-view.md:297`) changed. The design, the
construction and the known answers are v1's (`iteration-v1/build-notes.md`); round 2's notes
(`iteration-v2/build-notes.md`) cover the iterator contract, the flag-naming overflow error and
the label pins.

## What round 2 asked for, and what changed

### 1. A range constructor that regenerates the prefix still hung the suite (C5)

Round 2 bounded the tests that *read* huge ranges, but
`no_piece_exceeds_the_documented_bound_whatever_the_size` still built two huge ranges and asked
their `size_hint` *after* its `bounded(...)` call returned (v2 `payload_and_sizes.rs:437-455`).
The reviewer put a prefix loop (`for block_index in 0..start / BLOCK_BYTES { black_box(self.block(block_index)) }`)
before `Ok(Pieces { .. })` in `Payload::range` (`crates/validate/src/payload.rs:194`), and that
test then ran with no deadline.

I fixed the class, not the instance: **every test's whole body is now one `bounded(...)` call**,
so no call into the library is made outside a time bound. 25 of 25 `#[test]` functions start
with `bounded(` (checked by grep: 25 `#[test]`, 25 bodies opening with `bounded(`).

- `no_piece_exceeds_the_documented_bound_whatever_the_size` (`payload_and_sizes.rs:426`): the
  huge-size loop, with the `Payload::range` construction and both `size_hint` checks, moved
  inside the bound (`:442-461`).
- `the_last_bytes_of_a_huge_payload_are_produced_directly` (`:488`): `Payload::new` and the
  loop now sit inside `bounded` too. The four inner `within(DIRECT, …)` checks (5 s, `:485`)
  stay, so the brief's (C′) 5 s bound is unchanged; the outer 10 s `GUARD` (`:78`) only covers
  what lies between them.
- `a_payload_reports_the_size_and_generation_it_was_derived_for` (`:217`) and the 15 (D)/(E)/(F)
  tests (`:605` to `:1185`) were also outside any bound; each is now wrapped. Nothing in them
  spins today, but "no library call outside a bound" is a rule a reviewer can check by reading
  one line per test, where "every *payload* call is bounded" needs a judgement per call.
- The file's header (`:13-19`) and the `GUARD` / `bounded` docs (`:75-83`) now say this.

Nested bounds behave: when an inner `within(DIRECT)` times out it panics on the outer worker
thread, the outer `within` sees `Disconnected`, joins, and re-raises the inner message. So the
test reports "did not finish within 5s", not the outer 10 s message (mutant log below).

### 2. The architecture doc overstated the generation guarantee (docs currency)

`05-building-block-view.md:297` said, with no condition, "an overwrite at the same size still
sends different bytes", and that the classes run "from 0 B to 10 GiB + 1". Both were wrong for
some inputs. Now:

- "an overwrite at the same size sends different bytes except at the smallest sizes. Two
  `n`-byte bodies match with probability 2^(-8n), negligible from a few bytes up but not below:
  at 0 B every generation is the same empty body, and at 1 B two generations match one time in
  256. At those sizes neither the bytes nor the `ETag` (which the gateway derives from the body)
  can tell a stale read from a current one, which the oracle (#746) has to account for
  (`crates/validate/src/payload.rs`)." This matches the module docs (`payload.rs:34-44`) and the
  ETag source the brief cites (base `crates/server/src/lib.rs:274-275`).
- The class list now names the fixed sizes and the tuning-dependent ones separately and says
  "At the defaults they run from 0 B to 10 GiB + 1; a larger `--chunk-size` moves the `C` and
  `T` classes past that." (The reviewer's case: `--chunk-size 1099511627776` derives a
  158 TiB `T`.)

Whether such a tuning should be *accepted* is the deferred "tunings no server can have" item
(below), not this one. Here I only made the doc true for what the code does.

### 3. `TuningError::Zero`'s message was never run (diff-cov MISS `sizes.rs:115`)

`a_tuning_that_cannot_produce_a_table_is_refused_not_clamped` (`:821`) checked only the variant,
for `--chunk-size` alone. It now builds a zero for each of the three flags and checks both the
variant and the rendered message, "flag `--{flag}` must be a positive integer, not 0"
(`:894-925`). That also exercises the other two arms of the zero loop in `Tuning::resolve`
(`sizes.rs:159-167`), which only the parser's own refusal covered before.

### Not changed: C4 verification posture

Round 2's sign-off accepted the declared net-new-API exception. The new test file still cannot
compile on the base (the modules do not exist there: base `crates/validate/src/lib.rs:20-22`
declares only `access_keys`, `args` and `s3`, and base `crates/validate/Cargo.toml` has no
`sha2`), so C4-verify stays UNVERIFIABLE (exit 77), as the brief pre-declared.

### Not addressed: the deferred findings

`deferred-findings.json` holds the items "for the human at handover — not build work": the
vacuous `DEFAULT_CHUNK_SIZE == MIB` assertion (`:1210`; whether to observe the server's private
default through the in-process gateway is a scope call), tunings no server can have (T < C, T
not a multiple of C, C ≤ 64 KiB keeping the sub-chunk label; refuse or warn is a scope call),
the T4 batch-review evidence question, and the plan-review items the revised brief already
absorbed. I left them alone.

## What I ruled out

- **Bounding only the two huge-range calls in `no_piece_exceeds…`** (the reviewer's literal
  fix). About 6 lines: move the loop inside the existing `bounded`. It fixes this instance but
  leaves 16 other test bodies, and the `Payload::new` call in `the_last_bytes…`, outside any
  bound, so the next hand-written spin in, say, `size_classes` hangs the suite again. Wrapping
  every body costs one `bounded("name", || { … });` per test: round 2 had 8 such calls, this
  round has 25, so 17 more, about 3 lines each plus re-indentation (the test file grew from
  1,115 to 1,216 lines, which also counts the `Zero` loop of item 3). The
  `size-classes-spin` mutant below shows the difference: under round 2's file it would have hung
  13 tests; now they fail at 10 s.
- **A `macro_rules!` that defines a bounded test**, so a bare `#[test]` could not be written by
  mistake. rustfmt does not format the body of a `{}`-delimited macro call, so the whole file
  would drift from `cargo fmt`, and the macro hides the test function from a reader. The explicit
  wrapper plus the grep check is cheaper.
- **Taking the test name from `std::thread::current().name()`** instead of passing it. That
  relies on how libtest names its threads, which I did not want to depend on for every mode the
  suite runs in (plain `cargo test`, `cargo mutants`, the coverage run). A repeated string is
  explicit and costs nothing.
- **Changing production code** (a deadline inside `Payload::range`, say). Nothing in production
  is wrong; the round-2 reviewer says so ("the unmodified production implementation passes"),
  and a change there would reopen the reviewed construction for a test-harness defect.

## Demonstrated reds

Each a temporary edit to the patched tree, the test file run with
`timeout 300 cargo test -q -p wyrd-validate --test payload_and_sizes` (build first, then the
timed run), then the file restored and byte-compared against a saved copy (all three restored).
Driver and log: `$PDCA_SCRATCH/pdca-builder-744-v3/{run_mutants.py,mutants.log}`.

### The brief's six required reds

| Mutant | Run | Failed leg (and others) | Evidence |
|---|---|---|---|
| generation dropped from `Payload::new` (`payload.rs:128` removed) | 0.5 s | (B) `changing_any_one_input_changes_the_bytes`; also (A), (C′) KATs | `changing the generation left 4096 bytes unchanged` |
| `let c = DEFAULT_CHUNK_SIZE;` in `size_classes` (`sizes.rs:213`) | 0.3 s | (D) `a_different_chunk_size_moves_the_chunk_classes_and_nothing_fixed`, `two_classes_that_coincide…`, `a_tuning_that_cannot…` | assertion failed |
| `DEFAULT_MAX_ROOT_VALUE_BYTES / CHUNK_REF_BYTES` in `derived_segment_threshold` (`sizes.rs:142`) | 0.3 s | (D) `only_the_root_budget_changed_moves_only_the_derived_threshold`; five others | assertion failed |
| `fill_inside(offset + 1, buf)` in `fill` (`payload.rs:165`) | 0.2 s | (C) `any_range_produced_directly_equals_the_same_range_cut_from_the_whole`, `split_pieces…`, (C′) | `fill [0, 1) of 1` |
| hash and discard blocks `0..offset/32` at the top of `fill_inside` (`payload.rs:170`) | 5.1 s | (C′) `the_last_bytes_of_a_huge_payload_are_produced_directly`, only | `the tail of kat/10g+1, which must be computed directly, did not finish within 5s` |
| render without `({source})` (`lib.rs:73-77`) | 0.3 s | (E) `absent_tuning_flags_are_echoed_as_defaults_and_derived`, `given_…`, the binary test | `missing `  --chunk-size = 1048576 (default)`` |

### Round 2's findings

| Mutant | Run | Failed | Evidence |
|---|---|---|---|
| **the reviewer's mutant**: prefix loop before `Ok(Pieces { .. })` in `Payload::range` (`payload.rs:194`) | **10.1 s** | `the_last_bytes_of_a_huge_payload_are_produced_directly` (at 5 s), `no_piece_exceeds_the_documented_bound_whatever_the_size` (at 10 s) | `the streamed tail of kat/10g+1, which must start directly, did not finish within 5s`; `no_piece_exceeds_… did not finish within 10s`. Suite exits 101: 23 passed, 2 failed. Round 2: still running when killed at 25 s |
| `Zero`'s message changed to "flag `--{flag}` is zero" (`sizes.rs:115`) | 0.3 s | `a_tuning_that_cannot_produce_a_table_is_refused_not_clamped` | assertion failed (was a survivor: nothing ran that line) |
| `while black_box(true) {}` at the top of `size_classes` (`sizes.rs:213`) | 10.1 s | 13 (D)/(E)/(F) tests | `… did not finish within 10s`; round 2's file had these 13 tests outside any bound |

### Round 2's surfaces, re-run (all still caught)

| Mutant | Run | Failed |
|---|---|---|
| `size_hint`: `end / next`, `end + next`, `(0, None)` | 0.1 s each | 9, 8, 9 tests (`size_hint against the pieces yielded`, divide by zero) |
| `generation()` → `0` | 0.3 s | `a_payload_reports_the_size_and_generation_it_was_derived_for` |
| `label()` → `"xyzzy"` | 0.3 s | `each_class_is_labelled_with_the_boundary_it_straddles` |
| overflow message without `{from}` | 0.3 s | 3 tests (`segment-threshold-bytes not named`) |
| `fill_inside`: `<=`, `written *= take`, `position + BLOCK_BYTES` (spin) | 10.1 s each | 7 tests each |
| `Pieces::next`: `Some(vec![])` / `Some(vec![0])` forever, `next *= len` | 0.1 s each | 9, 9, 7 tests |
| `len_u64` → `0`; `PIECE_BYTES = 64 / 1024` | 0.1 s each | 9 tests each |

No mutant run took longer than 10.1 s.

Whole patch reverted: the test file does not compile on the base (see C4 above). That is the
brief's pre-declared C4-verify UNVERIFIABLE.

## Refute-your-own-test

- **(a) Genuine red?** Yes. Each of the six required edits turns its named leg red (table
  above), the prefix one as a 5 s failure. The reviewer's constructor mutant, which hung round
  2's file, now fails two named tests and the run exits at 10.1 s. The `Zero` message edit, which
  round 2's file let through, now fails. Reverting the whole patch fails to compile.
- **(b) Production path?** Yes. The tests call the crate's public API: `Payload::{new, fill,
  range, pieces, sha256, size, generation}`, `Tuning::resolve`, `size_classes`,
  `Boundary::label`, `derived_segment_threshold`, and `wyrd_validate::run` (the function
  `src/main.rs` calls); one test spawns the real binary (`CARGO_BIN_EXE_wyrd-validate`).
  `bounded`, `within` and `drain` only run and check production calls; they compute no bytes of
  their own. (A) checks against `sha2::Sha256::digest` of the produced bytes and pinned hex.
- **(c) Fixture includes the fault?** Yes. Same-size overwrites (generation 5 vs 6 at 4 KiB and
  100,003 B), a non-default chunk size and root budget, tails at 10 GiB + 1 and 1 TiB (where
  prefix regeneration takes minutes to hours), huge-range construction itself (the round-2
  gap), a zero for each flag built by hand, `u64::MAX` for each flag, the exact derived
  `u64::MAX` tuning, and every bad flag value (0, `000`, `+5`, `-5`, whitespace, `1MiB`, `1.5`,
  `0x10`, `1_000`, 2^64, empty, `--typo`, repeated, trailing).

## Gates

- `cargo fmt -p wyrd-validate` applied; `typos` clean on `crates/validate` and the docs file;
  `git diff --check` clean; `cargo clippy -p wyrd-validate --all-targets -- -D warnings` clean.
- `crates/validate/tests/cli_surface.rs` unmodified: 21/21 pass. New test: 25/25 pass (0.21 s).
- `patch.diff` is `git diff 12dd082` with the new files marked intent-to-add;
  `git apply --cached --check` succeeds against a fresh index read from `12dd082`.
- The test file the brief names is also at
  `results/issue_744/crates/validate/tests/payload_and_sizes.rs`, byte-identical to the one in
  the patch.
- Full gate through the wrapper, `./engine/xtask.sh ci` (→ `cargo xtask ci` in the worktree):
  **exit 0, "xtask ci: all checks passed"** (log: `$PDCA_SCRATCH/pdca-builder-744-v3/xtask-ci.log`).
  Steps that ran, not skipped: `typos`, `lint_docs.py` ("OK"), `render_site.py --check`
  (99 pages, "link audit OK"; this covers the edited docs paragraph), gitlink-guard,
  unsafe-guard, blackbox-guard ("wyrd-validate's normal dependency closure holds no wyrd-*
  crate"), fmt check, workspace clippy, build and test (`cli_surface.rs` 21/21,
  `payload_and_sizes.rs` 25/25), cargo-machete, the three cargo-deny runs (including the
  all-features advisories run), conformance vectors, statics, deploy-guard, and the madsim DST
  clippy and test.

- The C5 gate's own command, `./scripts/mutants-in-diff` (→ `cargo mutants --in-diff
  patch.diff --no-shuffle --cap-lints=true` in the worktree), with `TMPDIR` under scratch:
  "Found 99 mutants to test … Auto-set test timeout to 45s … 99 mutants tested in 6m: 91
  caught, 8 unviable". 0 missed, 0 timeouts, the same as round 2 (production code is the same,
  so the mutant set is too). Log: `$PDCA_SCRATCH/pdca-builder-744-v3/cargo-mutants.log`.

## Runner note

The project's runner (`./engine/xtask.sh ci`, i.e. `cargo xtask ci` in the worktree) runs the
whole gate only; it has no single-test entry. For the per-mutant reds I ran
`cargo test -p wyrd-validate --test payload_and_sizes` under `timeout 300` (the brief names
that target as where RED is reachable), and the full gate once through the wrapper.

## Scratch

This round's throwaway files are under `$PDCA_SCRATCH/pdca-builder-744-v3/` (the extended mutant
driver, its log, the saved source copies, the wrap script, a temporary index, and the gate log).
Left for the harness to reclaim, per the leaf's filesystem rules (no `rm` from this leaf).
