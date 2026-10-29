# Build notes — #744 validate: derived payloads, size classes, tuning flags

Target: getwyrd/wyrd @ main, base `12dd082`. All `path:line` below are in the patched tree
(`$PDCA_WORKTREE`) unless marked "base".

## What changed and why

- **`crates/validate/src/payload.rs` (new).** `Payload::new(seed, key, size, generation)`
  (`payload.rs:120`) keys a SHA-256 state with an unambiguous, length-prefixed encoding of the
  four inputs under a version tag (`payload.rs:120-134`); block `i` is
  `SHA-256(prefix || u64be(i))` (`block`, `payload.rs:146-150`). Because block `i` depends on
  `i` alone, `fill(offset, buf)` (`payload.rs:156`) hashes only the blocks the range touches,
  and the state does not grow with `size` (the cloned prefix state plus two `u64`s). Streaming
  is `pieces()` / `range(a, b)` (`payload.rs:186`), yielding at most `PIECE_BYTES` = 64 KiB per
  item (`payload.rs:58`). `sha256()` (`payload.rs:212`) is the streaming digest #746 and the
  checkpoints will key on. The module docs (`payload.rs:1-43`) state the construction exactly
  enough to reimplement, and state the small-size limit the brief's (B) asks for (0 B always
  coincide; below 32 B coincidence probability is 2^-8n; the ETag does not help, it is derived
  from the body) for #746.
  - A range outside the payload is refused (`OutOfRange`), never truncated (rubric:
    absent/unsupported → explicit error).
- **`crates/validate/src/sizes.rs` (new).** `DEFAULT_CHUNK_SIZE`, `DEFAULT_MAX_ROOT_VALUE_BYTES`,
  `CHUNK_REF_BYTES` (`sizes.rs:24-32`) mirror `crates/server/src/lib.rs:59`,
  `crates/core/src/metadata.rs:574`, `crates/core/src/multipart.rs:4908` (base lines).
  `derived_segment_threshold` (`sizes.rs:123`) is `⌊budget / 315⌋ × C`, the rule
  `value_chunk_capacity` / `max_single_put_bytes` apply (base `multipart.rs:4940-4992`), with
  checked multiply and an explicit refusal when the budget holds no record (would derive T=0).
  `Tuning::resolve` (`sizes.rs:142`) gives each value a `Source` (given/default/derived) and
  refuses any tuning whose table cannot be built. `size_classes` (`sizes.rs:196`) builds the
  sixteen points of (D) with checked arithmetic, sorts, and merges coinciding sizes keeping
  every label (`Boundary` enum, derive `Ord` in table order so merged labels are deterministic).
- **`crates/validate/src/args.rs`.** `TUNING_FLAGS` is its own set (`args.rs:47`); `FLAGS`
  (`args.rs:31`) is untouched, so `Args::pairs`, the missing-flag check and the usage first
  line are unchanged. The parser looks up both sets (`args.rs:173`) and runs the existing
  checks (needs-value, flag-in-value-slot, empty, repeated) for the tuning flags too; the value
  parse `byte_count` (`args.rs:238`) is digits-only before `parse::<u64>` (rubric grammar
  strictness: `u64::from_str` accepts `+5`), then refuses 0 and overflow by name. Three new
  `ArgError` variants name the flag. Docs at `args.rs:1-20` and `FLAGS`' doc now state the
  required/optional distinction the brief asked for (replacing base `args.rs:15-18`'s "this
  slice has no defaults").
- **`crates/validate/src/lib.rs`.** `ResolvedConfig` gains `tuning`; `render` prints
  `  --flag = value (source)` after the ten flags (`lib.rs:73`); `resolve_config` resolves
  the tuning after args and before credentials (`lib.rs:99`), with `RunError::Tuning`
  (`lib.rs:113`) exiting `EXIT_USAGE` like an argument error.
- **`usage()`** (`args.rs:258`) keeps the base first line byte-for-byte (cli_surface.rs:185
  compares it line-exact) and adds a second `optional tuning:` line.
- **`crates/validate/Cargo.toml:37`**: `sha2.workspace = true` (workspace dep, `Cargo.toml:171`
  base; already in the graph via core/gateway-s3). `Cargo.lock` gains exactly the one
  `"sha2"` line under `wyrd-validate`.
- **Docs currency**: the `validate` paragraph at
  `docs/design/architecture/05-building-block-view.md:297`, only that paragraph — the three
  flags, their defaults, the echo sources, size classes computed from them, and payloads
  derived from seed/key/size/generation.

## What I ruled out

- **`f(seed, key, size)`** (the issue text): rejected per brief/0017 §4; (B)'s generation leg
  is red against it (below).
- **A stateful PRNG (ChaCha etc.) per object**: needs another dependency and seek support; the
  SHA-256 counter construction is in-tree already and gives random access for free.
- **Adding the tuning flags to `FLAGS`**: would make them required (`Missing` iterates
  `FLAGS`) and change the usage line `cli_surface.rs` pins.
- **Re-parsing the tuning value after the loop with `.ok()`**: first draft did that; replaced
  with a map filled while parsing, so no error path is folded into `None`.
- **Echoing the whole size-class table** in the resolved configuration: not asked for; the
  three tuning lines are enough to reconstruct it, and #743 decides what a run reports.

## Known answers — produced in a separate process, cross-checked independently

Computed by a temporary Rust test (then replaced by the real file), and recomputed by a
from-scratch Python (`hashlib`) implementation written from the module docs only. All six
digests and both 64-byte tails matched exactly, which also confirms the docs describe the
construction exactly.

## Demonstrated reds (each a temporary edit, run, then the file restored)

Run with `cargo test -p wyrd-validate --test payload_and_sizes -- <filter>` under `timeout 600`.

| Mutant | Leg that failed | Evidence |
|---|---|---|
| generation dropped from `Payload::new` (`payload.rs:128` removed) | (B) `changing_any_one_input_changes_the_bytes` | `changing the generation left 4096 bytes unchanged` (also (A) fails: `("424242", "kat/one", 1, 0)`) |
| `let c = DEFAULT_CHUNK_SIZE;` in `size_classes` | (D) `a_different_chunk_size_moves_the_chunk_classes_and_nothing_fixed` | panic at test `:468` |
| `DEFAULT_MAX_ROOT_VALUE_BYTES / CHUNK_REF_BYTES` in `derived_segment_threshold` | (D) `only_the_root_budget_changed_moves_only_the_derived_threshold` | panic at test `:487` |
| `fill_inside(offset + 1, buf)` in `fill` | (C) `any_range_produced_directly_equals_the_same_range_cut_from_the_whole` | panic at test `:223` |
| hash and discard every block before `offset` in `fill_inside` | (C′) `the_last_bytes_of_a_huge_payload_are_produced_directly` | `the tail of kat/10g+1 not produced within 5s (gave up after 5.00014573s)` |
| render without `({source})` | (E) `absent_tuning_flags_are_echoed_as_defaults_and_derived` | panic at test `:659` |

The (C′) timing check runs the work on its own thread with `recv_timeout(5s)`, so a
prefix-regenerating generator **fails** at 5 s rather than hanging the runner for hours.

Whole patch reverted (scratch worktree at base `12dd082` + only the new test file): the test
does not compile (`unresolved import wyrd_validate::sizes`, `sha2`, `Payload`, …), matching the
brief's pre-declared C4-verify UNVERIFIABLE (exit 77) posture.

## Refute-your-own-test

- **(a) Genuine red?** Yes. Six targeted reverts each turn a named leg red (table above); the
  full revert fails to compile.
- **(b) Production path?** Yes. The test calls the crate's public API — `Payload`,
  `Tuning::resolve`, `size_classes`, `wyrd_validate::run` (the same entry `main.rs` calls) —
  and one test runs the real `wyrd-validate` binary (`CARGO_BIN_EXE_wyrd-validate`). Nothing
  is re-implemented in the test except `sha2::Sha256::digest` over the produced bytes, which is
  the independent check, not a stand-in.
- **(c) Fixture includes the fault?** Yes. The fault cases are all in the fixture: same-size
  overwrite (generation 5 vs 6 at 4 KiB and 100,003 B), the non-default chunk size and root
  budget, ranges at 10 GiB + 1 and 1 TiB (where prefix regeneration is hours), and every bad
  flag value (0, `000`, `+5`, `-5`, whitespace, `1MiB`, `1.5`, `0x10`, `1_000`, 2^64, empty,
  `--typo`, repeated, trailing).

## Gates

- `cargo fmt -p wyrd-validate` applied; `typos` clean on `crates/validate` and the docs file.
- `crates/validate/tests/cli_surface.rs` unmodified: 21/21 pass.
- New test: 22/22 pass (0.21 s).
- Full gate `./engine/xtask.sh ci` (→ `cargo xtask ci` in the worktree): **exit 0,
  "xtask ci: all checks passed"**. `typos` and the doc renderer (`render_site.py --check`,
  link audit OK) both ran, not skipped; `xtask blackbox-guard` reported "wyrd-validate's normal
  dependency closure holds no wyrd-* crate". A comment-only edit to `args.rs` landed after
  the gate started; I re-ran `cargo fmt --check`, `cargo clippy -p wyrd-validate --all-targets
  -D warnings`, `cargo doc -p wyrd-validate` with `-D warnings`, and both test files on the
  final tree: all clean.
- `patch.diff` is `git diff 12dd082` with the new files included; `git apply --check`
  succeeds on a fresh base checkout.
- Scratch: the gate log `$PDCA_SCRATCH/pdca-builder-744-ci.log` is left for the harness to
  reclaim (no `rm` from this leaf); the two scratch worktrees were removed with
  `git worktree remove`.

## Runner note

The project's runner (`./engine/xtask.sh ci`) runs the whole gate only; it has no single-test
entry. For the per-mutant reds I used `cargo test -p wyrd-validate --test payload_and_sizes`
wrapped in `timeout 600` (the brief names that target as where RED is reachable), and ran the
full gate once through the wrapper.
