# Build notes — #744 validate: derived payloads, size classes, tuning flags (iteration 5)

Target: getwyrd/wyrd @ main, base **`4bac538`** (the rebase the round-4 sign-off asked for; the
round-4 patch applied to it without a conflict, including its one `Cargo.lock` hunk). Every
`path:line` below is in the patched tree (`$PDCA_WORKTREE`) unless marked "base". This round
starts from the round-4 patch (`iteration-v4/patch.diff`) and changes what round 4's
carry-forward names. The design, the construction and the known answers are v1's
(`iteration-v1/build-notes.md`); rounds 2 to 4 cover the iterator contract, the flag-naming
overflow error, the label pins, the every-test-is-bounded rule, and the behavioural
chunk-size drift check.

**What changed against round 4, in one paragraph.** `crates/validate/src/sizes.rs` refuses, by
flag name, a tuning no deployment can have: a chunk size outside the server's 1 MiB to 16 MiB
range, a given threshold below one chunk, and a given threshold that is not a whole number of
chunks (`Tuning::check`, `sizes.rs:277-296`; `check_chunk_size`, `:217-223`). The two range
bounds are new mirrored constants (`MIN_CHUNK_SIZE`, `MAX_CHUNK_SIZE`, `sizes.rs:40`, `:45`),
tied at compile time to the default and to the two sub-chunk sizes (`:64-65`), and checked
against the server behaviourally in (F) (`tests/payload_and_sizes.rs:1359-1415`). The now
unreachable `TuningError::Zero` variant is gone. `lib.rs` re-exports the two constants. The
test file gains the refusal and range tests and loses the 4 KiB-chunk coincidence case; the
architecture paragraph states the refusals and the range check. `Cargo.toml`, `Cargo.lock`,
`args.rs` and `payload.rs` are byte-identical to round 4.

## What round 4 asked for, and what changed

### 1. Rebase onto `4bac538`; show a green `cargo xtask ci` including the advisory step

Done: the worktree is at `4bac538` (`git log -1`), the round-4 patch applied cleanly on it
(`git apply --check`, then `git apply`), and the full gate ran through the project wrapper on
the final tree — result in **Gates** below.

### 2. Tuning sanity: refuse what no deployment can have, by flag name

`Tuning::check` (`sizes.rs:277-296`) runs at the top of `size_classes` (`:302`), so it holds
for a `Tuning` resolved from the flags and for one built by hand (the struct's fields are
public), and `resolve` additionally checks the chunk size *before* deriving the threshold from
it (`:241-243`), so `--chunk-size 4294967296` is refused as a chunk size no deployment has and
not as a threshold that does not fit. Three new `TuningError` variants (`:120-141`), each
naming its flag(s) in `Display` (`:151` on):

| refusal | condition | message names |
|---|---|---|
| `ChunkSizeOutOfRange` | `C ∉ MIN_CHUNK_SIZE..=MAX_CHUNK_SIZE` (1 MiB to 16 MiB, server `crates/server/src/lib.rs:77`, `:104`) | `--chunk-size`, both bounds |
| `ThresholdBelowChunk` | given `T < C` | `--segment-threshold-bytes`, `--chunk-size` |
| `ThresholdNotWholeChunks` | given `T mod C ≠ 0` | `--segment-threshold-bytes`, `--chunk-size` |

**Why `T` must be a whole number of chunks, not just `≥ C`.** The server's rule is
`T = MAX_MAP_CHUNKS × C` (base `crates/core/src/multipart.rs:4987-4992`): an `n`-byte object
has `⌈n / C⌉` chunks and is flat iff that is at most `MAX_MAP_CHUNKS`. A `T` that is not a
multiple of `C` puts `T` and `T + 1` in maps of the same chunk count, so the `T ± 1` classes
would straddle nothing. The derived `T` is always a multiple (`k × C`, `k ≥ 1`), so both
threshold checks are no-ops for it; they bite only on a given value or a hand-built `Tuning`.

**`TuningError::Zero` removed.** With the range check, every zero is already refused by name
through another variant: chunk size 0 → `ChunkSizeOutOfRange`, budget 0 → `NoChunkRefFits`,
threshold 0 → `ThresholdBelowChunk` (chunk size 0 and threshold 0 pinned,
`payload_and_sizes.rs:875`, `:886`; budget 0 takes the same path as the pinned 314, `:878-884`). The
argument parser still refuses `0` first (`ArgError::Zero`, unchanged), which is what the
brief's (E) requires; `TuningError::Zero` only ever covered a `TuningFlags` built by hand. Its
message pin from round 2 goes with it.

**What this does to the overflow refusals.** With `C ≤ 16 MiB`, `C − 1`, `C + 1` and `64 × C`
can no longer overflow (`64 × 16 MiB = 1 GiB`), so round 4's `u64::MAX` / `1 << 60` chunk
cases are now range refusals (`:870-874`). Two overflow paths stay reachable and are tested at
both levels (unit `:889-901`, CLI in the refusal table `:1128-1218`):

- the derived `T`: `--max-root-value-bytes 18446744073709551615` → `⌊u64::MAX / 315⌋ × 1 MiB`
  does not fit (`:890-893`);
- `T + 1`: a chunk size that **divides `u64::MAX`** and lies inside the range,
  `17 × 65,537 = 1,114,129` (`u64::MAX = 3 × 5 × 17 × 257 × 65,537 × 4,294,967,297`), with a
  threshold of exactly `u64::MAX` given (`:894-897`) or derived from the budget
  `16,557,098,929,935 × 315` (`:898-901`; `ODD_CHUNK`, `MAX_DERIVED_BUDGET`, `:840-841`,
  computed and checked with `python3` before pinning). The server accepts any integer in the
  range (`check_s3_chunk_size`, base `crates/server/src/cli.rs:2627-2650`, no power-of-two
  rule), so this is a legal if exotic tuning.

I kept checked arithmetic on the chunk classes too (`size_classes`, `:301-355`): after `check` they cannot
fail, but checked is what the brief's design asks for, the `ok_or` arguments are evaluated
eagerly so the lines run (no diff-coverage MISS), and a loosened bound later cannot make them
wrap.

**Refusal order at the command line** (the carry-forward's three examples, all in
`a_tuning_no_deployment_can_have_is_refused_at_the_command_line_by_flag_name`,
`payload_and_sizes.rs:1124-1218`): `--segment-threshold-bytes 158` → below one chunk, naming
both flags; `--chunk-size 4096` → outside the range, with both bounds; `--chunk-size 4294967296
--segment-threshold-bytes 1024` → the chunk size, refused first. Also `1048575`, `16777217`,
`1048577` (not whole chunks), and the three overflow cases. The first case of each kind carries
the **whole** message, so each kind's wording is pinned through the production path
(`run` → `RunError::Tuning` → stderr), not only the typed variant. The bounds and a one-chunk
threshold are accepted inclusively (`--chunk-size 1048576`, `16777216`,
`--segment-threshold-bytes 1048576`, and `16777216` at a 16 MiB chunk), folded into
`given_tuning_flags_are_used_and_echoed_as_given` (`:991-1061`).

### 3. Labels true for the tuning: "sub-chunk" is below `C` in every table

The carry-forward asked for the "sub-chunk" label to apply only when `size < C`. I did not add
a runtime condition; I made the condition impossible to violate and tied it at compile time:

```rust
// sizes.rs:64-65
const _: () = assert!(MIN_CHUNK_SIZE <= DEFAULT_CHUNK_SIZE && DEFAULT_CHUNK_SIZE <= MAX_CHUNK_SIZE);
const _: () = assert!(64 * KIB < MIN_CHUNK_SIZE);
```

Every table `size_classes` produces has passed `check`, so `C ≥ 1 MiB > 64 KiB > 4 KiB`:
both sub-chunk sizes are below `C` in every table, including one from a hand-built `Tuning`
(pinned: a 4 KiB chunk built by hand is refused by `size_classes`, `payload_and_sizes.rs:905-916`).
The round-4 test pin of the 4 KiB "sub-chunk" + "chunk size (C)" contradiction is gone; the
coincidence test (`:798-835`) now uses two tunings a deployment can have: `--chunk-size 8388608`
(8 MiB = the SDK multipart threshold) and `--segment-threshold-bytes 5368709120` (5 GiB: `T`
on S3's largest single PUT and `T + 1` on the first size past it).

**Why not the literal conditional.** `if 4 * KIB < c { push SubChunk }` after `check` is a
branch that is never false. cargo-mutants replaces such a condition with `true`/`false`; the
`true` mutant would survive every test (nothing can observe it), so the round would come back
with a C5 survivor on a line that exists only to satisfy a review note. The compile-time
assertion is the server's own idiom (base `crates/server/src/lib.rs:106`, `:113-114`,
`:119`): whoever lowers the floor past 64 KiB fails the build at `sizes.rs:65` and decides
what to call those sizes then. Cost of the rejected alternative: +2 lines and a surviving
mutant; cost of the chosen one: 2 `const` lines and a doc sentence on `Boundary::SubChunk`
(`:376-377`).

### 4. (F) kept, and extended to the range it now mirrors

`the_default_chunk_size_matches_the_servers` and its helper are unchanged
(`payload_and_sizes.rs:1300-1357`). The range check adds two mirrored constants, so (F) gains
`the_chunk_size_range_matches_the_servers` (`:1407-1415`) over `server_chunk_size_bounds`
(`:1359-1405`).

**How the server's range is read.** `MIN_CHUNK_SIZE` / `MAX_CHUNK_SIZE` are `pub(crate)`
(base `crates/server/src/lib.rs:77`, `:104`) and `check_s3_chunk_size` is private (base
`cli.rs:2627`). The one public path that runs that check is `wyrd_server::cli::serve_s3_role`
(base `cli.rs:2466-2478`), whose **first statement** is the check, before any backend is
opened or anything is served. The probe calls it twice with the redb/mem selectors, an empty
data dir, no endpoints, no credentials and a loopback listener that is never served on
(`127.0.0.1:0`, dropped after): a 1-byte chunk size is refused "too small: it must be at least
N bytes", `usize::MAX` "too large: it must be at most N bytes", and the two `N` are the bounds.
The whole thing runs in milliseconds and nothing is written. The probe is wording-sensitive
on purpose: a reworded refusal fails to parse and fails the test loudly, which is the right
failure for a drift check; the server's own tests match the same strings (base
`cli.rs:3189`, `:3219-3222`).

**Rejected ways to read the bounds**, with their cost:

- *Spawn the `wyrd` binary with `s3 --chunk-size 1`.* `CARGO_BIN_EXE_*` exists only for the
  test's own package, so the test would have to `cargo build -p wyrd-server --bin wyrd`
  itself (a nested cargo invocation inside `cargo test`, tens of seconds, and a lock the
  outer cargo may hold) or guess a path under `target/`. ~40 lines and a flaky dependency on
  the build layout against the 40 lines shipped.
- *Compare the tool's constants with literals in the test* (`assert_eq!(MIN_CHUNK_SIZE,
  1 << 20)`). Cannot fail when the server moves; exactly the round-3 finding on
  `DEFAULT_CHUNK_SIZE`.
- *Make the server constants `pub`.* A server change this blackbox slice must not carry
  (brief scope), and it would turn a mirror into an import for a test that then proves nothing
  about drift.

Demonstrated both ways (table below): the server's `MAX_CHUNK_SIZE` raised to 32 MiB fails the
new test with `left: (1048576, 16777216) / right: (1048576, 33554432)`; the tool's floor
lowered to 512 KiB fails it with `left: (524288, 16777216)`, and also fails the CLI refusal
test (`1048575` is then accepted).

### 5. The T4 batch review

Check's business, not Do's; nothing in the patch for it.

### 6. Size

`patch.diff` is 116,439 bytes (round 4: 103,027; the backstop is 100 KB and was already
passed in round 1). The 13 KB are the two constants and three refusals with their docs and
messages (`sizes.rs` +86 lines), the (F) range probe and test (+53), and the refusal /
acceptance tables (+~110 after rustfmt put each tuple field on its own line). To hold it down
I folded round 4's two overflow tests and this round's bounds-accepted test into the two
tables, dropped the unit-level message table (the CLI test pins each kind's whole message
instead), and trimmed redundant rows (`MIN − 1` / `MAX + 1` at the unit level are covered at
the CLI level). I did not shorten round 4's tests or the docs paragraph (one line, so its
whole old and new text is in the diff, ~13 KB of the total on its own).

### Test changes forced by the new refusals

- `a_given_threshold_is_used_exactly_whatever_the_root_budget`: `t = 123_456_789` (not a whole
  number of 1 MiB chunks) → `200 * MIB` (`:769`). Budget `200` with a given `T` is still
  accepted: the brief's "whatever the root budget".
- `given_tuning_flags_are_used_and_echoed_as_given`: `--segment-threshold-bytes 123456789` at a
  2 MiB chunk → `262144000` (125 chunks).
- `each_bad_tuning_value_is_refused_by_name`: `u64::MAX` is refused for a different reason per
  flag now (range / derived overflow / not whole chunks), each still naming the flag
  (`:1100-1105`); the repeated-flag value `4096` → `4194304` so the case is about repetition,
  not range (repetition is caught in the parser first either way).
- `the_ten_required_flags_stay_required_with_the_tuning_flags_given`: `4096` → `4194304`,
  same reason.
- `two_classes_that_coincide_appear_once_carrying_both_labels` and the overflow tests: as in
  items 2 and 3.

26 tests in the file (round 4: 26 — two removed, two added), every `#[test]` body opening
with `bounded(` (26 of 26 by grep). `cli_surface.rs` is not in the patch and passes 21/21.

### Not addressed: the deferred findings

`deferred-findings.json` holds the items "for the human at handover — not build work". Left
alone, as in round 4.

### Not changed: C4 verification posture

The test file still cannot compile on the base (base `crates/validate/src/lib.rs:20-22`
declares only `access_keys`, `args` and `s3`; base `Cargo.toml` has no `sha2`), so C4-verify
stays UNVERIFIABLE (exit 77), as the brief pre-declared.

## Demonstrated reds

Each a temporary text replacement applied by a driver (`$PDCA_SCRATCH/pdca-builder-744-v5/run_mutants.py`),
the new test file run with `timeout 300 cargo test -q -p wyrd-validate --test payload_and_sizes`,
the file restored from a snapshot and verified byte for byte (`filecmp`; afterwards `cmp`
against saved pristine copies for the three tool files and `git diff -- crates/server/` empty).
Per-mutant logs: `$PDCA_SCRATCH/pdca-builder-744-v5/mutant-NN.log`; table: `mutants.md` beside
them. (The driver's verdict column mislabels every run "COMPILE ERROR": its regex matches
cargo's `error: test failed` line. Each log's `test result: FAILED. N passed; M failed` line
and the absence of `could not compile` are what the table below reports.)

| # | Mutant | Failed tests | Evidence |
|---|---|---|---|
| 1 | brief (B): `keyed.update(generation…)` removed (`payload.rs:128`) | 3: (B) `changing_any_one_input_changes_the_bytes`, (A), (C′) | 23 passed, 3 failed |
| 2 | brief (D): `let c = DEFAULT_CHUNK_SIZE;` in `size_classes` (`sizes.rs:303`) | 2: `a_different_chunk_size_moves…`, `two_classes_that_coincide…` | 24/2 |
| 3 | brief (D): `DEFAULT_MAX_ROOT_VALUE_BYTES / CHUNK_REF_BYTES` in `derived_segment_threshold` (`:204`) | 5: `only_the_root_budget_changed…`, the two refusal tests, `each_bad_tuning_value…`, the binary echo | 21/5 |
| 4 | brief (C): `fill_inside(offset + 1, buf)` (`payload.rs:165`) | 3: `any_range_produced_directly…`, `split_pieces…`, (C′) | 23/3 |
| 5 | brief (C′): hash blocks `0..offset/32` at the top of `fill_inside` (`:170`) | 1: `the_last_bytes_of_a_huge_payload_are_produced_directly` | `did not finish within 5s`; whole run 6.0 s, no hang |
| 6 | brief (E): echo without `({source})` (`lib.rs:75-76`) | 3: `absent_tuning_flags…`, `given_tuning_flags…`, the binary echo | 23/3 |
| 7 | range check → `chunk_size > 0` (`sizes.rs:218`) | 3: both refusal tests, `each_bad_tuning_value…` | `--chunk-size 4096` accepted; `u64::MAX` refused as an overflow instead |
| 8 | `T < C` check → `if false` (`:281`) | 2: both refusal tests | `158` refused as `ThresholdNotWholeChunks` instead of `ThresholdBelowChunk` |
| 9 | whole-chunks check → `if false` (`:287`) | 3: both refusal tests, `each_bad_tuning_value…` | `1048577` accepted |
| 10 | tool `MIN_CHUNK_SIZE = 1 << 19` (`:40`) | 2: (F) `the_chunk_size_range_matches_the_servers`, the CLI refusal test | `left: (524288, 16777216)`, `right: (1048576, 16777216)` |
| 11 | tool `MAX_CHUNK_SIZE = 32 << 20` (`:45`) | 2: the same two | `16777217` accepted; `left: (1048576, 33554432)` |
| 12 | **server** `MAX_CHUNK_SIZE = 32 << 20` (base `crates/server/src/lib.rs:77`) | 1: `the_chunk_size_range_matches_the_servers` | `left: (1048576, 16777216)`, `right: (1048576, 33554432)` |

Round 4's server `DEFAULT_CHUNK_SIZE` mutants (2 MiB, 512 KiB) are not re-run this round;
`the_default_chunk_size_matches_the_servers` and its helper are byte-identical to round 4.

## Refute-your-own-test

- **(a) Genuine red?** Yes. Each of the twelve mutants above fails at least one named test and
  the run completes (no hang; the slowest, the prefix regeneration, ends at the 5 s bound).
  The whole patch reverted does not compile (net-new API; see C4 posture).
- **(b) Production path?** Yes. Every test calls the crate's public API (`Tuning::resolve`,
  `size_classes`, `run`, the real binary). The two (F) probes drive the production server:
  `Gateway::new` + `put_object` (round 4) and `wyrd_server::cli::serve_s3_role` (this round),
  the same function `wyrd s3` runs, whose first statement is the range check every deployment
  passes through. No stand-in, no copied constant, no re-implementation.
- **(c) Fixture includes the fault?** Yes. The reds inject the fault into production source on
  both sides — the tool's constants and checks, and the server's `MAX_CHUNK_SIZE` — and the
  probes compose the server with its own defaults and selectors, not a fixture that hides them.

## Rubric self-review (`AGENTS.md` § Review rubric & protocol)

- One clock: no clock read anywhere in the patch.
- Narrow seams / dependency direction: `wyrd-server` stays a dev-dependency; the binary links
  no `wyrd-*` crate (`cargo xtask blackbox-guard` in the gate).
- No shared mutable global state: only `const`s added.
- `#![forbid(unsafe_code)]`: no new crate root; the test file carries it.
- Docs currency: the three flags' refusals and the range mirror are in the `validate`
  paragraph (`docs/design/architecture/05-building-block-view.md:297`), same PR.
- Grammar strictness: the tuning values stay digits-only, no sign (`args.rs`, unchanged).
- Absent or unsupported entries: every impossible tuning is an explicit, named error; nothing
  is clamped or skipped.
- Await discipline: the one new await (`serve_s3_role`) returns before any I/O; the test body
  is under the file's 10 s guard like every other.

## Gates

- `cargo fmt -p wyrd-validate` applied, then `cargo fmt --all -- --check` clean;
  `cargo clippy -p wyrd-validate --all-targets -- -D warnings` clean (two findings fixed on
  the way: `manual_is_multiple_of` → `is_multiple_of`, MSRV 1.96; `type_complexity` on the
  test tables → the `Refusal` / `Echo` aliases); `typos crates/validate docs/…/05-building-block-view.md`
  clean; `git diff --check 4bac538` clean.
- `cargo test -p wyrd-validate --test payload_and_sizes --test cli_surface`: 26/26 and 21/21.
- `patch.diff` = `git diff 4bac538` with the three new files intent-to-add; it applies to a
  fresh index read from `4bac538` (`git apply --cached --check`). The brief's test file is
  also at `results/issue_744/crates/validate/tests/payload_and_sizes.rs`, byte-identical.
- Full gate through the wrapper, `./engine/xtask.sh ci` (→ `cargo xtask ci` in the worktree),
  on the final tree, log `$PDCA_SCRATCH/pdca-builder-744-v5/xtask-ci-final.log` (3,336
  lines): **`xtask ci: all checks passed`, exit 0.** Every step ran: `typos`, `lint_docs: OK`,
  `render_site … link audit OK` (covers the edited docs paragraph), gitlink-guard,
  unsafe-guard, blackbox-guard ("wyrd-validate's normal dependency closure holds no wyrd-*
  crate", log `:18`), fmt check, workspace clippy, build and test (both validate test files
  ran inside it, log `:2550`, `:2577`; no `FAILED` anywhere in the log), cargo-machete,
  `cargo deny check` ("advisories ok, bans ok, licenses ok, sources ok"), **`cargo deny
  --all-features --config deny-all-features.toml check advisories` → `advisories ok`**
  (log `:3144-3145`; the RUSTSEC-2026-0382 wall round 4 hit is recorded on this base, #929),
  `cargo deny --all-features check licenses bans sources`, conformance vectors, statics,
  deploy-guard, and the madsim DST clippy and test. After the gate, `git diff 4bac538`
  still equals `patch.diff` byte for byte (the gate left the tree untouched).
- The C5 gate's own command, `./scripts/mutants-in-diff` (→ `cargo mutants --in-diff
  patch.diff --no-shuffle --cap-lints=true` in the worktree, `PDCA_BUNDLE` set, `TMPDIR`
  under scratch), on the final `patch.diff`: "Found 106 mutants to test … Auto-set test
  timeout to 50s … **106 mutants tested in 6m: 95 caught, 11 unviable**" — 0 missed,
  0 timeouts (round 4: 99 mutants, 91 caught, 8 unviable; the seven new ones are the range and
  threshold checks and their messages). Log: `$PDCA_SCRATCH/pdca-builder-744-v5/cargo-mutants.log`.

## Runner note

The project's runner (`./engine/xtask.sh ci`) runs the whole gate only; it has no single-test
entry. For the per-mutant reds and the fast loop I ran `cargo test -p wyrd-validate --test
payload_and_sizes` under `timeout 300`/`600`, and the full gate through the wrapper.

## Scratch

This round's throwaway files are under `$PDCA_SCRATCH/pdca-builder-744-v5/`: pristine source
copies (`pristine/`), per-mutant snapshots (`snapshots/`), the red driver and its logs, the
gate log, the `cargo mutants` log and its `TMPDIR` (`mutants-tmp/`, the large one), and a
temporary index (`check.index`). Left for the harness to reclaim, per the leaf's filesystem
rules (no `rm` from this leaf).
