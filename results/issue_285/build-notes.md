# Build notes — issue 285 / validate-ec-scheme-at-read-boundary (iteration 2)

## What changed, and why (path:line on the target worktree, `wyrd.pdca-wt-l0`, base `b91401a` / `origin/main`)

- `crates/core/src/erasure.rs:109-122` — new `pub fn supported(k: usize, m: usize) -> bool`,
  a thin wrapper over `reed_solomon_simd::ReedSolomonDecoder::supports(k, m)`. This IS the
  "supported-scheme predicate the erasure coder uses" the carry-forward calls for — I did not
  invent a parallel rule; I named the one `reed_solomon_simd::encode`/`decode` already apply
  internally (`ReedSolomonEncoder::supports` and `ReedSolomonDecoder::supports` both delegate to
  the same `DefaultRate::<E>::supports`, verified by reading
  `~/.cargo/registry/.../reed-solomon-simd-3.1.0/src/reed_solomon.rs:82-84,180-182` and
  `.../src/rate/rate_default.rs:76-78` — so using the decoder's is equivalent to the encoder's).
  `use_high_rate` (`.../src/rate/rate_default.rs:15-34`) rejects `original_count == 0`,
  `recovery_count == 0`, counts over `GF_ORDER`, and any pair whose `smaller_pow2 + larger >
  GF_ORDER` — i.e. `k == 0`, `m == 0`, and "unsupported k+m" generically, exactly the three
  things the brief's Defect bullet and the carry-forward's codex finding name.
- `crates/core/src/erasure.rs:144-146` (`reconstruct`) — replaced the iteration-1 `if k == 0`
  guard with `if !supported(k, m)`. Still runs before `available.len() < k`
  (`erasure.rs:147-152`) and before `available[0]` (`erasure.rs:153`), so the panic vector from
  the brief's repro (`reconstruct(0, 1, 0, &[])`) is still closed — but now `m == 0` is rejected
  too, even in the case iteration 1 missed: a `rs(k, 0)` scheme with a full `k`-of-`k`
  `available` set, which never needs `reed_solomon_simd::decode` at all (the
  `data.iter().filter(is_some).count() < k` branch at `erasure.rs:170` is false when all `k` are
  present) and so would otherwise return bytes for a scheme `encode()` itself could never have
  produced.
- `crates/core/src/erasure.rs:20-47` (`ErasureError`) — `InvalidScheme { k, m }` variant kept
  from iteration 1, doc comment broadened from "`k` must be at least 1" to name the `m == 0` /
  unsupported-`k+m` cases too; `Display` message likewise broadened
  (`erasure.rs:58-64`: "unsupported by the erasure coder" instead of the `k`-only phrasing).
- `crates/core/src/read.rs:175-199` (`read_chunk`'s `EcScheme::ReedSolomon` arm) — replaced the
  iteration-1 `if k == 0` guard with `if !erasure::supported(k, m)`, still before the fragment
  fan-out (`read.rs:200+`) fires a single fetch. This is the exact line the carry-forward and the
  codex advisory named (`read.rs:184` in the reviewed PR state ≈ this arm's guard here; the
  advisory's second citation, `read.rs:267` in that state, is this arm's `erasure::reconstruct`
  call, now unreachable with an unsupported scheme for the same reason).
- `crates/core/src/read.rs:336-361,388-395` (`ReadError::InvalidEcScheme`) — doc/Display broadened
  the same way as the erasure-layer variant.
- Tests (co-located, per the brief's `Test file:` line, which names `erasure.rs`'s tests module
  plus "a companion read-path assertion" in `read.rs`'s tests):
  - `erasure.rs` tests (`erasure.rs:290-345` area): kept both iteration-1 `k == 0` regressions
    unchanged (still pass, still exercise the empty/non-empty `available` panic sites), added
    `reconstruct_with_m_zero_is_a_typed_error_even_with_all_k_shards_present` — builds `k`
    same-size shards by hand (never through `encode`, since `encode(k, 0, ..)` is itself not a
    legal call) to isolate that the *scheme* is rejected, not shard availability — and
    `supported_accepts_the_schemes_this_module_round_trips`, a guard-rail that the broadened
    predicate doesn't regress any `(k, m)` pair the module's own round-trip tests already rely on.
  - `read.rs` tests (new `#[cfg(test)] mod tests` at the file's end, `read.rs:402+`): kept
    iteration-1's `read_chunk_rejects_a_stored_k_zero_scheme`, added
    `read_chunk_rejects_a_stored_m_zero_scheme` — a `ChunkRef { scheme: ReedSolomon { k: 3, m: 0
    }, .. }` against an `EmptyChunks` store, asserting `ReadError::InvalidEcScheme { k: 3, m: 0
    }` and that `corrupt` stays empty (a validation rejection, not a corruption finding).

## Why this shape, not something else

**Rejected: keep iteration 1's narrow `k == 0` check.** This is exactly what iteration 1 shipped
and what Check's codex advisory flagged as incomplete
(`results/issue_285/iteration-v1/check-advisory-codex.md:1`): "other unsupported stored EC
schemes such as `rs(k,0)` still drive read fan-out and can reach `erasure::reconstruct`... where
all data shards present can return bytes without the Reed-Solomon coder ever rejecting the
unsupported `m == 0` scheme." I reproduced that gap manually (see "Red/green" below) before
writing the fix, and the carry-forward explicitly says not to resubmit it unchanged.

**Rejected: hand-list every unsupported case (`k == 0 || m == 0 || k + m > SOME_LIMIT`).** This
would silently drift from `reed_solomon_simd`'s actual rate limits (`GF_ORDER`, the
`smaller_pow2 + larger` bound) the moment the dependency's internal limits change, and it
duplicates logic the coder crate already owns. The carry-forward's own phrasing — "using the same
supported-scheme predicate the erasure coder uses" — points at reusing the coder's own
`supports()`, not re-deriving its rules. Concretely, hand-listing would mean tracking
`rate_default.rs`'s three reject conditions (`count > GF_ORDER`, `count == 0`, `smaller_pow2 +
larger > GF_ORDER`) by hand in two places (`erasure.rs` and `read.rs`) instead of one function
call in each — more code, and a second copy that can silently fall out of sync with the coder's
actual behavior (e.g. a future `reed-solomon-simd` bump changing `GF_ORDER` or the rate-selection
threshold). `erasure::supported` is 3 lines and is the one place that predicate lives.

**Rejected: validate only in `read_chunk`, not `reconstruct`.** The erasure API is also reachable
from custodian reconstruction (scrub/repair), not just the client read path — the brief's
Invariant to restore names "the erasure/read API boundary", plural. Iteration 1's review (T5)
already endorsed this defense-in-depth shape; I kept it rather than collapsing to one call site,
since removing the `erasure.rs` guard would leave any *other* future caller of `reconstruct`
(besides `read_chunk`) exposed to the same untrusted-`k`/`m` panic the brief opens with.

**Rejected: reject at CLI/encode time instead of read time.** Explicitly out of scope per the
brief ("out of scope: changing the CLI parse rule (already rejects `rs(0,m)`)"); `encode()`
already can't produce an `m == 0` chunk (`reed_solomon_simd::encode`'s own `supports()` check
rejects it before any shard is built, `erasure.rs:104`), so there is no write-time gap to close —
the gap was purely stored-metadata trust at read time, which is what this patch closes.

## Red/green, and how it was checked (project's own runner)

Ran `PDCA_BUNDLE=results/issue_285 PDCA_LANE=0 ./engine/scripts/run-verify.sh` from
`/home/eddie/wyrd/wyrd-pdca` (the driver's own C4-verify gate). Result: **GREEN** — `cargo test -p
wyrd-core` (24/24) passes with the patch applied to a fresh `origin/main` worktree. Because the
regression tests are co-located inside the two modified production files (per the brief's `Test
file:` line — no separate `*/tests/*.rs` is added), the script reports "PASS (green-only)": it
cannot mechanically isolate a per-fix RED for a co-located test and says so explicitly, deferring
the whole-tree red→green discriminator to `C4-ci`.

To still prove red→green honestly (not just trust the green-only note), I manually reverted *only*
the two guard blocks (`erasure.rs`'s `if !supported(k, m) { return Err(...) }` at what is now
`erasure.rs:144-146`, and `read.rs`'s equivalent at what is now `read.rs:192-199`) while keeping
every test, via `cargo test -p wyrd-core --lib` in the same worktree (not a hand-rolled runner —
this is the identical command `run-verify.sh` itself invokes, run directly to isolate what the
wrapper's green-only note said it couldn't). Result: **5 of the 6 new/changed regressions failed**
— two as literal panics (`index out of bounds: the len is 0 but the index is 0` at the pre-fix
`erasure.rs:150`, matching the brief's repro), one as a wrong-`Ok`-value `unwrap_err()` panic (the
`m == 0`-with-full-`k` silent-success case the carry-forward flagged), and the read-path `m == 0`
case surfaced the *wrong* error (`InsufficientFragments` instead of `InvalidEcScheme`, since
without the guard the unguarded fan-out against `EmptyChunks` just starves normally) — i.e. every
test this iteration is graded on is a genuine regression, not a vacuous assertion. Restored the
guards and re-ran: all 24 pass again (see the full pass listing above under "the fix applied").
Then ran the **gating** `cargo xtask ci` (fmt/clippy -D warnings/build/test/deny/conformance) —
the exact command whose failure blocked iteration 1's sign-off (per the carry-forward: "cargo
test --workspace --exclude wyrd-dst failed with exit status: 101") — end to end: **all checks
passed**, including the `wyrd-dst` DST suite (`concurrency`, `custodian`, `network` integration
tests, 16 tests, all green) that the carry-forward's rationale said was base-drift, not
attributable to the patch; running on the now-current base confirms that.

`cargo fmt --check` (workspace-wide) is clean; `cargo fmt -p wyrd-core` was run before finalizing
(reformatted one `write!` call in `erasure.rs`'s `Display` impl to the project's line-wrap style —
included in `patch.diff`). `cargo clippy -p wyrd-core --all-targets -- -D warnings` is clean.

## Scope discipline

Touches only `crates/core/src/erasure.rs` and `crates/core/src/read.rs` — the two files the brief
names (`Test file:` line) and the two files 285 is scheduled to touch per the ordering note
("285 and 290 both edit `crates/core/src/read.rs`... scheduled into different waves"). No change
to `crates/server/src/cli.rs` (CLI parse rule, explicitly out of scope), no change to the on-disk
metadata format/codec (ADR-0002, out of scope), no change to #291's broader validation-boundaries
research.
