# Build notes — #744 validate: derived payloads, size classes, tuning flags (iteration 2)

Target: getwyrd/wyrd @ main, base `12dd082`. Every `path:line` below is in the patched tree
(`$PDCA_WORKTREE`) unless marked "base". This iteration starts from the previous attempt's
patch (`iteration-v1/patch.diff`, applied unchanged) and changes only what the carry-forward
names. The design, the construction, the known answers and the docs paragraph are the same as
v1; v1's build notes (`iteration-v1/build-notes.md`) still describe them.

## What the carry-forward asked for, and what changed

### 1. The (C′) prefix-regeneration red was a hang, not a failure

v1's `no_piece_exceeds_the_documented_bound_whatever_the_size` read the tails of the
10 GiB + 1 and 1 TiB payloads with no time limit, so the prefix-regenerating mutant spun for
hours there while only the timed test failed. The same shape caused the 12 `TIMEOUT` mutants
in `fill_inside` / `Pieces::next`: a stream that never advances or a fill that never ends
hangs any test that drives it, and `cargo test` (so `cargo xtask ci`) has no per-test timeout.

Fixed in the test, in three parts:

- **Every test that produces payload bytes runs under a wall-clock bound.** `within`
  (`crates/validate/tests/payload_and_sizes.rs:47`) runs the work on its own thread and fails
  at the limit. Unlike v1's version it re-raises a panic from the worker with its own message
  (v1 reported every worker panic as "not produced within 5s", because the panicking thread
  drops the sender and `recv_timeout` returns `Disconnected` at once). `bounded`
  (`:79`) wraps a whole test body in `GUARD` = 10 s (`:76`): honest work is milliseconds, and
  10 s is well below the per-mutant timeout cargo-mutants picks (47 s here, 53 s in v1's run),
  so a spinning mutant is scored caught, not TIMEOUT.
- **The huge-size piece checks moved into the timed test.** The first piece and the last
  `PIECE_BYTES + 3` bytes of both huge payloads are now read inside `within(DIRECT, …)` in
  `the_last_bytes_of_a_huge_payload_are_produced_directly` (`:480`), `DIRECT` = 5 s (`:477`).
  `no_piece_exceeds_the_documented_bound_whatever_the_size` (`:419`) keeps the small sizes
  (bounded) and, at the huge sizes, checks only `size_hint`, which hashes nothing.
- **Every stream is drained through `drain`** (`:88`), which stops at the first byte past
  the expected length, refuses an empty piece and a piece over `PIECE_BYTES`, and requires
  every piece but the last to be exactly `PIECE_BYTES`. So a stream that never ends fails at
  once and holds at most the expected length, rather than eating memory until the guard fires
  (v1's `whole()` was `flatten().collect()`, unbounded).

### 2. An overflow refusal that did not name its flag

`--segment-threshold-bytes 18446744073709551615` parsed and then failed with "the segmentation
threshold + 1 does not fit in 64 bits" (v1 `sizes.rs:230`). Fixed in production:
`TuningError::Overflow` is now `{ class, from }` (`crates/validate/src/sizes.rs:98`), and
every overflow names the flag(s) its value is computed from (`:105-110`):
`` `--chunk-size` `` for the C classes (`:215`), `` `--segment-threshold-bytes` `` for the
T classes when T was given, and `` `--max-root-value-bytes` and `--chunk-size` `` when T was
derived (`:219-222`, and the derived-T overflow itself at `:148`). The message is
"the {class} size class, computed from {from}, does not fit in 64 bits" (`:124`).

Tests: `an_overflowing_size_class_is_refused_naming_the_flags_it_came_from` (`:1016`) drives
both T + 1 cases through `run` (the entry `main.rs` calls); the derived one uses
C = 72,340,172,838,076,673 and budget 80,325, so ⌊80,325 / 315⌋ = 255 and
255 × C = `u64::MAX` exactly while every C class fits (`:777`). `each_bad_tuning_value_is_refused_by_name`
now also passes `u64::MAX` to each of the three flags and requires the flag named (`:996`).
`a_tuning_that_cannot_produce_a_table_is_refused_not_clamped` pins `class`, `from` and the
rendered message for all five overflow cases (`:842`). This also covers the `Overflow`
`Display` arm v1's diff-cov listed as MISS (v1 `sizes.rs:112`).

### 3. Mutation survivors on three public surfaces (14 missed)

- **`Pieces::size_hint`** (8 survivors). `drain` records `size_hint()` before every step and
  after the end and requires exactly the pieces still to come, `(n − k, Some(n − k))`,
  checked against the pieces actually yielded, not against a re-derived formula. At the
  huge sizes `no_piece_exceeds…` checks the hint without iterating (163,841 pieces at
  10 GiB + 1, 2^24 at 1 TiB; `(2, Some(2))` for a `PIECE_BYTES + 3` tail). In production the
  unreachable `Err(_) => (usize::MAX, None)` arm became one expression
  (`crates/validate/src/payload.rs:249-251`), same values, and the `Pieces` docs now state
  the exact piece shape the hint relies on (`:221-225`; `PIECE_BYTES` docs `:56-57`).
- **`Payload::generation()`** (2 survivors): `a_payload_reports_the_size_and_generation_it_was_derived_for`
  (`:215`) checks `size()`, `generation()` and the `Debug` form (which also covers the
  `Debug` mutant, previously MISSED).
- **`Boundary::label`** (1 survivor) and `Display for Boundary` (1): `LABELS` pins every one
  of the 15 labels, and `each_class_is_labelled_with_the_boundary_it_straddles` (`:665`)
  checks `label()` and `to_string()` against it. While pinning I reworded one label that
  read wrongly: `BeyondTenGib` was "past the > 10 GiB launch requirement (10 GiB + 1)", now
  "beyond 5 GiB: an object over 10 GiB, the launch requirement (10 GiB + 1)" (`sizes.rs:337`),
  closer to 0017's "> 5 GiB" row and the brief's "stands for the > 10 GiB launch requirement".
- **`Display for OutOfRange`** (1 survivor): `a_range_outside_the_payload_is_refused_not_truncated`
  now pins both messages and the exact `OutOfRange` values for all four refusals.

### Not changed: C4 verification posture

The carry-forward accepts the declared net-new-API exception; nothing to build. The new test
file still cannot compile on the reverted base (the modules do not exist there), so C4-verify
is UNVERIFIABLE (exit 77), as the brief pre-declared.

### Not addressed: the five deferred findings

`deferred-findings.json` holds five items "for the human at handover — not build work". I left
them alone: the vacuous `DEFAULT_CHUNK_SIZE == MIB` assertion (`:1110`, (F); the brief says
the server constant is private and not checkable, the reviewer shows its effect could be
observed through the in-process gateway, a scope call), tunings no server can have
(T < C, T not a multiple of C, C ≤ 64 KiB keeping the sub-chunk label), and three plan-review
items the revised brief already absorbed.

## What I ruled out

- **Restructuring `fill_inside` into a `for` over `chunks_mut`** so its mutants panic or give
  wrong bytes instead of spinning. It would remove the spin class from the production loop,
  but the loop is correct and already reviewed; with the test-side guard those mutants now
  fail in 10 s, and changing the construction code risks the bytes for no behavioural gain.
  The cost of the rewrite is small (about 12 lines replaced in `payload.rs:170-182`); I kept
  the reviewed code.
- **`ExactSizeIterator` for `Pieces`.** Its `len()` must be exact in a `usize`; on a 32-bit
  target a 64-bit range can have up to 2^48 pieces. `size_hint` already reports the exact
  count where it fits.
- **Putting the flag's value into the overflow message.** It would mean a `u64` per variant
  for a refusal whose flag is already on the operator's command line; naming the flag is what
  (E) requires.

## Demonstrated reds

Each a temporary edit to the patched tree, the test file run with
`timeout 300 cargo test -q -p wyrd-validate --test payload_and_sizes`, then the file
restored (byte-compared against a saved copy). Driver script and full log:
`$PDCA_SCRATCH/pdca-builder-744-mutants/{run_mutants.py,mutants.log}`.

### The brief's six required reds

| Mutant | Run | Failed leg (and others) | Evidence |
|---|---|---|---|
| generation dropped from `Payload::new` (`payload.rs:128` removed) | 0.5 s | (B) `changing_any_one_input_changes_the_bytes`; also (A), (C′) KATs | `changing the generation left 4096 bytes unchanged` |
| `let c = DEFAULT_CHUNK_SIZE;` in `size_classes` (`sizes.rs:213`) | 0.3 s | (D) `a_different_chunk_size_moves_the_chunk_classes_and_nothing_fixed`, `two_classes_that_coincide…`, `a_tuning_that_cannot…` | assertion failed |
| `DEFAULT_MAX_ROOT_VALUE_BYTES / CHUNK_REF_BYTES` in `derived_segment_threshold` (`sizes.rs:142`) | 0.3 s | (D) `only_the_root_budget_changed_moves_only_the_derived_threshold`; five others | assertion failed |
| `fill_inside(offset + 1, buf)` in `fill` (`payload.rs:165`) | 0.2 s | (C) `any_range_produced_directly_equals_the_same_range_cut_from_the_whole`, `split_pieces…`, (C′) | `fill [0, 1) of 1` |
| hash and discard blocks `0..offset/32` at the top of `fill_inside` (`payload.rs:170`) | **5.1 s** | (C′) `the_last_bytes_of_a_huge_payload_are_produced_directly`, **only** | `the tail of kat/10g+1, which must be computed directly, did not finish within 5s (gave up after 5.00015171s)`; the run exits 101 at 5.1 s (v1: still running at 150 s) |
| render without `({source})` (`lib.rs:73-77`) | 0.3 s | (E) `absent_tuning_flags_are_echoed_as_defaults_and_derived`, `given_…`, the binary test | `missing `  --chunk-size = 1048576 (default)`` |

### The carry-forward's surfaces

| Mutant | Run | Failed | Evidence |
|---|---|---|---|
| `size_hint`: `end / next` (divide by zero on first use) | 0.1 s | 9 tests | `attempt to divide by zero`; `size_hint against the pieces yielded` |
| `size_hint`: `end + next` (over-reports) | 0.1 s | 8 tests | `size_hint against the pieces yielded` |
| `size_hint` → `(0, None)` | 0.1 s | 9 tests | same |
| `generation()` → `0` | 0.3 s | `a_payload_reports_the_size_and_generation_it_was_derived_for` | assertion failed |
| `label()` → `"xyzzy"` | 0.3 s | `each_class_is_labelled_with_the_boundary_it_straddles` | `Empty` |
| overflow message without `{from}` (v1's behaviour) | 0.3 s | `an_overflowing_size_class…`, `each_bad_tuning_value…`, `a_tuning_that_cannot…` | `segment-threshold-bytes not named: wyrd-validate: the T + 1 size class does not fit in 64 bits` |

### v1's TIMEOUT class: each now fails, in bounded time

| Mutant | Run | Evidence |
|---|---|---|
| `fill_inside`: `while written <= buf.len()` (spins) | 10.1 s | `a_payload_digest_matches_its_known_answer did not finish within 10s`; 7 tests fail |
| `fill_inside`: `written *= take` (spins) | 10.1 s | same, 7 tests |
| `fill_inside`: `position + BLOCK_BYTES` for `%` | 10.1 s | `attempt to subtract with overflow` and guard failures, 7 tests |
| `Pieces::next` → `Some(vec![])` forever | 0.1 s | `an empty piece after 0 bytes`, 9 tests |
| `Pieces::next` → `Some(vec![0])` forever | 0.1 s | `the stream passed its 0 bytes`, 9 tests |
| `Pieces::next`: `self.next *= len` | 0.1 s | `the stream passed its 1 bytes`, 7 tests |
| `len_u64` → `0` | 0.1 s | `[99, 101) accepted`, divide by zero, 9 tests |
| `PIECE_BYTES = 64 / 1024` | 0.1 s | divide by zero, 9 tests |

No run took longer than 10.1 s.

### The C5 gate's own command, re-run on this patch

`cargo mutants --in-diff patch.diff --no-shuffle --cap-lints=true` (the command
`scripts/mutants-in-diff` runs), from the worktree, with `TMPDIR` under scratch:

```
Found 99 mutants to test
ok       Unmutated baseline in 24s build + 9s test
 INFO Auto-set test timeout to 47s
99 mutants tested in 6m: 91 caught, 8 unviable
```

**0 missed and 0 timeouts**, against v1's 14 missed and 12 timeouts (18 min). The 8
unviable are the same count as v1's.

Whole patch reverted: the test cannot compile on the base. Base `crates/validate/src/lib.rs:20-22`
declares only `access_keys`, `args` and `s3`, base `:28` exports no `TUNING_FLAGS`, and base
`crates/validate/Cargo.toml` has no `sha2`, so `use wyrd_validate::{Payload, Pieces, Tuning, …}`
and `use sha2::…` do not resolve. v1 compiled it on a scratch base worktree and recorded the
same `unresolved import` errors; I did not rebuild a second base tree for this iteration. This
matches the brief's pre-declared C4-verify UNVERIFIABLE (exit 77).

## Refute-your-own-test

- **(a) Genuine red?** Yes. Each of the six required reverts turns its named leg red (tables
  above), the prefix-regenerating one now as a 5 s failure instead of a hang; the full revert
  fails to compile.
- **(b) Production path?** Yes. The test calls the crate's public API: `Payload::{new, fill,
  range, pieces, sha256, size, generation}`, `Tuning::resolve`, `size_classes`,
  `Boundary::label`, and `wyrd_validate::run`, the entry `src/main.rs` calls; one test spawns
  the real binary (`CARGO_BIN_EXE_wyrd-validate`). `drain` and `within` are checkers around
  production calls, not stand-ins: `drain` only reads what `Pieces` yields and asks
  `size_hint`, and `sha2::Sha256::digest` over the produced bytes is the independent check
  of (A).
- **(c) Fixture includes the fault?** Yes. Same-size overwrites (generation 5 vs 6 at 4 KiB and
  100,003 B), the non-default chunk size and root budget, tails at 10 GiB + 1 and 1 TiB where
  prefix regeneration takes minutes to hours, `u64::MAX` for each flag and the exact
  derived-`u64::MAX` tuning, and every bad flag value (0, `000`, `+5`, `-5`, whitespace, `1MiB`,
  `1.5`, `0x10`, `1_000`, 2^64, empty, `--typo`, repeated, trailing).

## Gates

- `cargo fmt -p wyrd-validate` applied; `typos` clean on `crates/validate` and the docs file;
  `git diff --check` clean.
- `crates/validate/tests/cli_surface.rs` unmodified (`git diff --quiet` on it): 21/21 pass.
- New test: 25/25 pass (0.21 s).
- `cargo clippy -p wyrd-validate --all-targets -- -D warnings`: clean.
- Full gate through the wrapper, `./engine/xtask.sh ci` (→ `cargo xtask ci` in the worktree):
  **exit 0, "xtask ci: all checks passed"**. Steps that ran, not skipped: `typos`,
  `lint_docs.py`, `render_site.py --check` (99 pages, "link audit OK"), gitlink-guard,
  unsafe-guard, blackbox-guard ("wyrd-validate's normal dependency closure holds no wyrd-*
  crate"), `cargo fmt --all -- --check`, workspace clippy, build and test (where
  `cli_surface.rs` 21/21 and `payload_and_sizes.rs` 25/25), cargo-machete, the three
  cargo-deny runs (including the all-features advisories run), statics, deploy-guard, and the
  madsim DST clippy and test.
- `patch.diff` is `git diff 12dd082` with the new files included (intent-to-add);
  `git apply --cached --check` succeeds against a fresh index read from `12dd082`.
- The test file the brief names is also at
  `results/issue_744/crates/validate/tests/payload_and_sizes.rs`, byte-identical to the one in
  the patch.

## Runner note

The project's runner (`./engine/xtask.sh ci`, i.e. `cargo xtask ci` in the worktree) runs the
whole gate only; it has no single-test entry. For the per-mutant reds I ran
`cargo test -p wyrd-validate --test payload_and_sizes` under `timeout 300` (the brief names
that target as where RED is reachable), and the full gate once through the wrapper.

## Scratch

Everything throwaway is under `$PDCA_SCRATCH/pdca-builder-744-mutants/` (the mutant script,
its log, the saved source copies, and cargo-mutants' copy of the tree, with `TMPDIR` pointed
there so the copy did not land on `/tmp`). Left for the harness to reclaim, per the leaf's
filesystem rules (no `rm` from this leaf).
