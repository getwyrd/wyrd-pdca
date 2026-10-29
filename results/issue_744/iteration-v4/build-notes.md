# Build notes — #744 validate: derived payloads, size classes, tuning flags (iteration 4)

Target: getwyrd/wyrd @ main, base `12dd082`. Every `path:line` below is in the patched tree
(`$PDCA_WORKTREE`) unless marked "base". This round starts from the round-3 patch
(`iteration-v3/patch.diff`, applied unchanged) and changes only what round 3's carry-forward
names. The design, the construction and the known answers are v1's
(`iteration-v1/build-notes.md`); rounds 2 and 3 (`iteration-v2/`, `iteration-v3/build-notes.md`)
cover the iterator contract, the flag-naming overflow error, the label pins and the
every-test-is-bounded rule.

**What changed against round 3:** no production logic. `crates/validate/src/sizes.rs` changed
in three doc comments only; the test file gained one test and lost one always-true assertion;
the architecture doc changed one sentence. `Cargo.toml` and `Cargo.lock` are byte-identical to
round 3 (only the `sha2` line the brief allows).

## What round 3 asked for, and what changed

### 1. (F) did not check the chunk-size default, though it can

Round 3's (F) test compared the tool's `DEFAULT_CHUNK_SIZE` with the test's own literal `MIB`
(v3 `payload_and_sizes.rs:1210`), so it could never fail. The brief said the server's value is
private and "not checkable from a test" (brief.md:129-130); the reviewer showed it is, through
the refusal a default gateway gives an oversize single PUT.

The fix, `the_default_chunk_size_matches_the_servers` (`payload_and_sizes.rs:1259`) and its
helper `default_gateway_single_put` (`:1227`):

- composes a gateway with the server's defaults, `wyrd_server::Gateway::new(..)` with no
  `with_chunk_size` (server base `crates/server/src/lib.rs:163-174`, which sets
  `chunk_size: DEFAULT_CHUNK_SIZE`, base `:59`), over the same backends the crate's other tests
  use (redb in memory, `FsChunkStore` in a temp dir, `MemCoordination`);
- asks it to store `T + 1` bytes, where `T` is the tool's derived default threshold
  (`derived_segment_threshold(DEFAULT_CHUNK_SIZE, DEFAULT_MAX_ROOT_VALUE_BYTES)`, 158 MiB);
- requires the answer to be `GatewayError::EntityTooLarge { declared: T + 1, limit: T }`.
  `limit` is `max_single_put_bytes(self.chunk_size)` (base `lib.rs:283-296`), so it is 158 ×
  the server's own default chunk size.

That pins the server's limit to exactly `T` from both sides: a smaller server default is
refused with a smaller `limit`; a larger one admits the PUT (and the test says so). The
always-true `assert_eq!(DEFAULT_CHUNK_SIZE, MIB)` is gone; the message of the
`max_single_put_bytes` assertion no longer says "1 MiB" (`:1213`).

**Why the inherent `Gateway::put_object`, not the reviewer's `put_object_streaming`.** The
streaming PUT is a method of the `ObjectGateway` trait, which lives in `wyrd-gateway-core`; it
is not re-exported by `wyrd-server` (only `GatewayError` is, base `lib.rs:44`) or by
`wyrd-gateway-s3`. Calling it needs a new dev-dependency:

```diff
 # crates/validate/Cargo.toml [dev-dependencies]
 wyrd-core.workspace = true
+wyrd-gateway-core.workspace = true
 # Cargo.lock, the wyrd-validate entry
  "wyrd-core",
+ "wyrd-gateway-core",
```

That is a `Cargo.lock` change beyond the `sha2` line, which the brief puts out of scope
(brief.md:105-106). The inherent `put_object(&self, key, data: &[u8])` (base `lib.rs:257`) runs
the same admission first (`self.admit_single_put(data.len() as u64)?`, base `:258`), so it
reports the same `limit` with no new dependency. Its cost is the body: a `&[u8]` of
`T + 1` bytes. I build it with `vec![0u8; n]`, which is a zeroed allocation the refusal never
reads, so on the passing path it costs nothing measurable (below).

The trade-off, measured:

| | passing path | server default raised to 2 MiB (red) | lowered to 512 KiB (red) |
|---|---|---|---|
| inherent `put_object(T + 1)` (shipped) | 0.00 s, peak RSS 94 MB (same as a trivial test) | 5.8 s, peak RSS 435 MB: the gateway really stores 158 MiB | 0.2 s |
| trait `put_object_streaming(u64::MAX)` (rejected) | instant | instant | instant |

So the only price of staying inside the brief's scope is a slower red when the server's default
grows. It still fails by name with a clear message (below), and if a slow host pushes it past
the test's 10 s bound, the bound fails the same test. On that path the stored body sits in a
temp dir until the test finishes, or is left behind if the bound fires first (the worker thread
is still writing when the process exits). I judged that acceptable for a failure that only
happens on drift. If the human prefers the instant red, the dev-dependency above is the
whole change, plus about 10 lines in the helper.

I also ruled out a probe body large enough to be refused by any server build (the server pins
`max_single_put_bytes(MAX_CHUNK_SIZE) <= 5 GiB`, base `lib.rs:113`): a 5 GiB zeroed allocation
is virtual only, but under strict overcommit (`vm.overcommit_memory = 2`) it fails, and a failed
allocation aborts the whole test process, not just this test.

### 2. The doc for `T` was off by one

`crates/validate/src/sizes.rs:86-87` now reads "`T`: the largest object size whose chunk map is
still flat, in bytes; `T + 1` is the first that is not (it segments, or a single PUT of it is
refused)". That matches the server: at `T = ⌊budget / 315⌋ × C` the map holds exactly
`MAX_MAP_CHUNKS` chunks and is flat (base `crates/core/src/multipart.rs:4954-4958`); `T + 1` is
the first size that segments or is refused (`:4986-4992`), and the architecture doc already
said so ("the size past which a chunk map no longer fits one value",
`05-building-block-view.md:297`). I checked the rest of the patch for the same slip: the
`derived_segment_threshold` doc (`sizes.rs:139-142`, "a flat chunk map holds … chunks, and a
larger one segments") and the three labels (`sizes.rs:334-338`, `T - 1`, `T`, `T + 1`) were
already right.

### Docs that described the old check

- `sizes.rs:14-17` (module docs) and `:24-28` (`DEFAULT_CHUNK_SIZE`) now say the chunk size is
  checked against what a default server gateway refuses with, and why it is read that way
  (the server constant is private).
- `05-building-block-view.md:297`: "A test compares the defaults with what they mirror: the
  root budget and the 315-byte chunk-record width with the `wyrd-core` constants, and the chunk
  size with the single-PUT limit a default in-process server gateway refuses with."
- Test file header (`payload_and_sizes.rs:9-12`) and the (F) section heading (`:1187`).

### Not addressed: the deferred findings

`deferred-findings.json` holds the items "for the human at handover — not build work"
(tunings no server can have, the T4 batch-review evidence question, and plan-review items the
brief already absorbed). I left them alone. The round-3 deferral "whether to observe the
server's private default through the in-process gateway" is now done, as item 1, because round 3's
carry-forward asked for it.

### Not changed: C4 verification posture

The test file still cannot compile on the base (the modules do not exist there: base
`crates/validate/src/lib.rs:20-22` declares only `access_keys`, `args` and `s3`, and base
`crates/validate/Cargo.toml` has no `sha2`), so C4-verify stays UNVERIFIABLE (exit 77), as the
brief pre-declared.

## Demonstrated reds

Each a temporary edit to the patched tree, the test file run with
`timeout 300 cargo test -q -p wyrd-validate --test payload_and_sizes`, then the file restored
from a saved copy and checked with `cmp` (server: `git diff --stat -- crates/server/` empty).
Logs: `$PDCA_SCRATCH/pdca-builder-744-v4/*.log`.

### This round's check

| Edit | Run | Failed | Evidence |
|---|---|---|---|
| **the reviewer's mutant**: server `DEFAULT_CHUNK_SIZE = 2 << 20` (base `crates/server/src/lib.rs:59`) | 5.8 s | `the_default_chunk_size_matches_the_servers` only | `a default server gateway stored a 165675009-byte single PUT, so its default chunk size is above the tool's 1048576 B` |
| the same mutant, **round 3's test file** (the fix reverted) | 0.2 s | none: 25 passed | the reviewer's finding, reproduced |
| server `DEFAULT_CHUNK_SIZE = 1 << 19` | 0.2 s | `the_default_chunk_size_matches_the_servers` only | `single-PUT limit is 82837504 B, 158 chunks of 524288 B, but the tool's default chunk size is 1048576 B` (`left: 82837504, right: 165675008`) |
| the tool's `DEFAULT_CHUNK_SIZE = 2 << 20` alone (`sizes.rs:28`) | 0.2 s | the new test and four (D)/(E) tests | `single-PUT limit is 165675008 B, 158 chunks of 1048576 B, but the tool's default chunk size is 2097152 B` |

### The brief's six required reds

Re-run on this round's final tree with round 3's driver, unchanged
(`$PDCA_SCRATCH/pdca-builder-744-v4/run_mutants.py`, log `mutants.log` beside it). After the
run all three source files `cmp` equal to saved copies, and `git diff 12dd082` equals
`patch.diff` byte for byte.

| Mutant | Run | Failed leg (and others) | Evidence |
|---|---|---|---|
| generation dropped from `Payload::new` (`keyed.update(generation…)` removed, `payload.rs`) | 0.5 s | (B) `changing_any_one_input_changes_the_bytes`; also (A), (C′) KATs | `changing the generation left 4096 bytes unchanged` |
| `let c = DEFAULT_CHUNK_SIZE;` in `size_classes` (`sizes.rs`) | 0.3 s | (D) `a_different_chunk_size_moves_the_chunk_classes_and_nothing_fixed`, `two_classes_that_coincide…`, `a_tuning_that_cannot…` | assertion failed |
| `DEFAULT_MAX_ROOT_VALUE_BYTES / CHUNK_REF_BYTES` in `derived_segment_threshold` (`sizes.rs:147`) | 0.3 s | (D) `only_the_root_budget_changed_moves_only_the_derived_threshold`; five others | assertion failed |
| `fill_inside(offset + 1, buf)` in `fill` (`payload.rs`) | 0.2 s | (C) `any_range_produced_directly_equals_the_same_range_cut_from_the_whole`, `split_pieces…`, (C′) | `fill [0, 1) of 1` |
| hash and discard blocks `0..offset/32` at the top of `fill_inside` | 5.1 s | (C′) `the_last_bytes_of_a_huge_payload_are_produced_directly`, only | `the tail of kat/10g+1, which must be computed directly, did not finish within 5s` |
| render without `({source})` (`lib.rs`) | 0.3 s | (E) `absent_tuning_flags_are_echoed_as_defaults_and_derived`, `given_…`, the binary test | `missing `  --chunk-size = 1048576 (default)`` |

### Rounds 1-3's surfaces, re-run (all still caught)

The same driver's other 17 edits all fail, none takes longer than 10.1 s: `size_hint` (`/`,
`+`, `(0, None)`: 9, 8, 9 tests), `generation()` → 0 (1 test), `label()` → `"xyzzy"` (1),
the overflow message without its flag (3), the round-2 reviewer's prefix loop in the range
constructor (2 tests, at 5 s and 10 s), `Zero`'s message (1), a spin at the top of
`size_classes` (13 tests at 10 s), the three `fill_inside` spins (7 tests each at 10 s), the
three `Pieces::next` faults (9, 9, 7), `len_u64` → 0 (9) and `PIECE_BYTES = 64 / 1024` (9).

## Refute-your-own-test

- **(a) Genuine red?** Yes. For this round's check: with round 3's test file (the fix
  reverted) the reviewer's server mutant passes all 25 tests; with this round's file it fails
  `the_default_chunk_size_matches_the_servers`, in both directions (2 MiB and 512 KiB) and when
  only the tool's side moves. The whole patch reverted does not compile (see C4).
- **(b) Production path?** Yes. The new test drives the production server gateway
  (`wyrd_server::Gateway::new` and its public `put_object`), whose refusal reads the same
  `single_put_limit` every single-PUT path reads (base `lib.rs:283-296`), and compares it with
  the tool's production `derived_segment_threshold` and `DEFAULT_CHUNK_SIZE`. No stand-in, no
  copied constant. The rest of the file calls the crate's public API (round 3's answer).
- **(c) Fixture includes the fault?** Yes. The gateway is composed with the server's defaults
  (no `with_chunk_size`), so a moved server default is exactly what it sees; the reds above
  inject that fault in both directions, in the server's source.

## Gates

- `cargo fmt -p wyrd-validate` applied, then `cargo fmt --all -- --check` clean; `typos` clean
  on `crates/validate` and the docs file; `git diff --check 12dd082` clean;
  `cargo clippy -p wyrd-validate --all-targets -- -D warnings` clean.
- Every `#[test]` in the new file still opens its body with `bounded(` (26 of 26, by grep). The
  new test first had its explanation as a `//` comment before `bounded(`, which broke that
  one-line check; I moved it to a `///` doc comment above `#[test]`.
- `crates/validate/tests/cli_surface.rs` unmodified (not in `patch.diff`): 21/21 pass. New
  test file: 26/26 pass (0.21 s).
- `patch.diff` is `git diff 12dd082` with the new files marked intent-to-add;
  `git apply --cached --check` succeeds against a fresh index read from `12dd082`. The test
  file the brief names is also at `results/issue_744/crates/validate/tests/payload_and_sizes.rs`,
  byte-identical to the one in the patch.
- Full gate through the wrapper, `./engine/xtask.sh ci` (→ `cargo xtask ci` in the worktree),
  run on the final tree (log `$PDCA_SCRATCH/pdca-builder-744-v4/xtask-ci-final.log`): **exit 1,
  from one step only**, `cargo deny --all-features --config deny-all-features.toml check
  advisories`, on RUSTSEC-2026-0382 (`rustls-webpki 0.101.7`, "Certificate verification may
  accept distrusted intermediates", via `tikv-client 0.4.0` → `tonic 0.10.2` → `rustls 0.21.12`;
  `Cargo.lock:305`, a line this patch does not touch). Every other step ran and passed:
  `typos`, `lint_docs.py` ("OK"), `render_site.py --check` ("link audit OK"; covers the edited
  docs paragraph), gitlink-guard, unsafe-guard, blackbox-guard ("wyrd-validate's normal
  dependency closure holds no wyrd-* crate"), fmt check, workspace clippy, build and test
  (no `FAILED` result anywhere; `cli_surface.rs` 21/21, `payload_and_sizes.rs` 26/26),
  cargo-machete ("didn't find any unused dependencies"), `cargo deny check` (advisories,
  bans, licenses, sources ok), `cargo deny --all-features check licenses bans sources`,
  conformance vectors, statics, deploy-guard, and the madsim DST clippy and test.
- **The advisory fails on the unpatched base too.** I ran the same command in a clean
  `git archive 12dd082` copy (`$PDCA_SCRATCH/pdca-builder-744-v4/base-12dd082`, log
  `base-deny-advisories.log`): exit 1, the same `RUSTSEC-2026-0382`. The patch changes neither
  `deny-all-features.toml` nor that lock entry (its only `Cargo.lock` change is the `sha2` line
  in `wyrd-validate`'s dependency list). Round 3's gate passed this step on the same base, so
  the advisory most likely entered the RustSec database between the two rounds. Fixing it (`cargo update -p rustls-webpki` cannot move a
  0.101 pin that `rustls 0.21` requires; it needs a `tikv-client` bump or an ignore entry) is
  out of this slice's scope (brief.md:105-106, `deny*.toml` and `Cargo.lock`).

NEEDS-HUMAN external dependency: cargo deny all-features advisories — the unpatched base `12dd082` fails `cargo deny --all-features --config deny-all-features.toml check advisories` on RUSTSEC-2026-0382 (rustls-webpki 0.101.7 via tikv-client), so `cargo xtask ci` exits 1 for every bundle on this base; this patch's own steps all pass, but a green full gate cannot be shown until the base is fixed or the advisory is triaged.

- The C5 gate's own command, `./scripts/mutants-in-diff` (→ `cargo mutants --in-diff
  patch.diff --no-shuffle --cap-lints=true` in the worktree), with `TMPDIR` under scratch:
  "Found 99 mutants to test … Auto-set test timeout to 48s … 99 mutants tested in 6m: 91
  caught, 8 unviable". 0 missed, 0 timeouts, the same as round 3 (this round's production
  changes are doc comments, so the mutant set is the same). Log:
  `$PDCA_SCRATCH/pdca-builder-744-v4/cargo-mutants.log`.

## Runner note

The project's runner (`./engine/xtask.sh ci`, i.e. `cargo xtask ci` in the worktree) runs the
whole gate only; it has no single-test entry. For the per-mutant reds I ran
`cargo test -p wyrd-validate --test payload_and_sizes` under `timeout 300`, and the full gate
through the wrapper.

## Scratch

This round's throwaway files are under `$PDCA_SCRATCH/pdca-builder-744-v4/`: saved source
copies, the red driver and its logs, both gate logs, the `cargo mutants` log and its `TMPDIR`
(`mutants-tmp/`), the `git archive` copy of the base used for the advisory check
(`base-12dd082/`), a temporary index (`check.index`), and an empty `v3tree/` directory I made
by mistake. Left for the harness to reclaim, per the leaf's filesystem rules (no `rm` from this
leaf).
