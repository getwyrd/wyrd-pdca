# Build notes — issue 652 / startup-recovery-total-and-bounded (iteration 5)

*Withheld from the reviewer; written for the human at sign-off.*

Worktree: `/home/eddie/wyrd/wyrd.pdca-wt-l1` at `d50f0ca` (= `origin/main`, the brief's target
base). Line citations are **post-patch** lines in that worktree unless marked "pre-fix" / "on
base" (which means `origin/main`).

---

## 1. What round 5 changes, item by carry-forward item

Round 4's *substance* was accepted at sign-off ("The cli.rs allocator-counter change and the
one-line docs deletion are ACCEPTED as in-scope … Do not revert these in the next round"), so
everything from round 4 is kept unchanged. What was still open was the T4 rubric review's four
blocking findings plus two questions the human asked to be answered rather than deferred:

| Carry-forward item | Disposition this round |
|---|---|
| T4 BUG `cli.rs:1678` — `u64::from_str` accepts a leading `+`, so `+1` reads as a valid counter | **FIXED.** The counter is read through the tree's own canonical-decimal grammar (`crates/core/src/metadata.rs:1321`, now `pub`), so `+1`, `01`, `007`, `7\n` are **damage**, not `1`. §2. |
| T5 Judgment `[impl]` — same defect, plus "the test matrix at `cli.rs:2789` omits `+1` and `01`" | **FIXED.** Both rows added, with four more near-misses (`crates/server/src/cli.rs:2822-2833`), and the end-to-end test that now uses `+1` as its damaged counter (`crates/server/tests/gateway_recover_totality.rs:771`). |
| T4 TEST-GAP `cli.rs:1848` — the CAS repair path has no seeded Tier-0 DST coverage for races with allocation or another recovery | **Coverage FIXED, DST *location* rejected with the cost shown.** Two new tests: a deterministic injected interleaving that binds the compare-and-set guard on every run, and a real-threads contention test. §4. |
| T4 CONVENTION ×2 `metadata.rs:2129/2135` — the `scan_page` awaits carry no timeout | **REJECTED**, on the tree's own already-merged position for this exact question, now also written at the loop. §3. |
| Validation / fitness-to-purpose — "page-bounded memory bounds memory but not time … demonstrate this is acceptable or note what bound is needed" | **ANSWERED, with measurements** (§5) and in the code (`crates/core/src/metadata.rs:2201-2222`). |
| C4 — the reviewer's `cargo deny check` stopped on a read-only advisory-db lock; confirm nothing real is masked | **CONFIRMED CLEAN.** A full local `cargo xtask ci` passes here, all three deny legs included (`advisories ok, bans ok, licenses ok, sources ok`, then the `--all-features` advisories leg and the licences/bans/sources leg). §7. |

## 2. The counter's grammar (the BUG + the T5 judgment)

`read_persisted_inode_counter` did `text.parse::<u64>()`. `u64::from_str` accepts `+1` and `01`,
so a stored `+1` read as **`Readable(1)`** — a spelling no writer of this key produces
(`alloc_inode`/`seed_next_inode_floor` both write `u64::to_string`). The consequence is the
exact hazard the whole slice is about, and it is worse than the one the round fixed: over a
store whose `inode:` keys prove a floor of 5 000, `Readable(1)` means "the allocator is at 1",
`counter_already_at_floor` answers *false* so recovery does not even repair it — and
`alloc_inode` then hands out 1, 2, 3 …, ids that are live. A **lenient counter read is a
re-mint**, whereas the fail-closed/reseed pair this slice built is safe.

The fix is not a new parser: the tree already has one, and its doc already names this defect —
`parse_canonical_u64` (`crates/core/src/metadata.rs:1310-1321` on base, private), used by
`parse_seg_key` with the comment "`u64::from_str` accepts `+7` and `007`, so a segment could be
addressed by keys that differ in bytes but agree in value". The repo rubric says to extend a
shared parser rather than write a new one, so it is made `pub` (one added item, doc at `:1311-1320`)
and `cli::read_persisted_inode_counter` calls it (`crates/server/src/cli.rs:1696`). Rejected
alternative: copy the eight lines into `cli.rs` (**+8 lines, two grammars for one record class**)
— that is precisely the duplication the rubric names, and the two copies would drift the first
time one is tightened.

The asymmetry is stated at the callsite (`cli.rs:1680-1683`) because it is the reason strictness
matters *here* and not on the `inode:` keys: a lenient **key** reading would *raise* the mark
(untidy, safe); a lenient **counter** reading *lowers* the floor. `parse_inode_key`
(`metadata.rs:2025`, untouched base code) is therefore deliberately left as it is — tightening it
would convert `inode:007` from "contributes 7" to "contributes nothing", which is the
under-approximation direction the invariant forbids.

Canonical `0` stays `Readable(0)` (asserted, `cli.rs:2844-2848`): it is a number, and the floor
comparison raises it — inventing a fourth reading for it would add an arm no caller needs.

## 3. The two await-timeout findings — rejected, and why that is not laziness

Both say the new `scan_page` await must carry a timeout. The tree has already answered this
question, in this module, in merged code that predates the patch —
`git show origin/main:crates/core/src/metadata.rs` lines **2142-2149**:

> The TIME an await may take is the BACKEND's: "a backend must bound its own waiting rather than
> block a caller forever on an unreachable cluster" … which is why each networked driver imposes
> its own (`metadata-fdb/src/lib.rs:78-89`, `metadata-tikv/src/lib.rs:143-172`) and the embedded
> one needs none. `wyrd-core` holds no runtime dependency to spend a caller-side deadline from
> (ADR-0009 …), and no other metadata call in this module wraps one.

The cost of "just adding a timeout" is therefore not a wrapper line: `wyrd-core` has **no tokio
dependency at all** (`crates/core/Cargo.toml`), so it means taking a runtime dependency in the
crate ADR-0009 keeps executor-free, in order to bound one of the module's ~20 store awaits while
`get`, `commit` and the resolver's own `scan_page` stay unwrapped — a bound stated where the seam
does not provide one. It is also the brief's standing *Do-not-re-earn* (i) ("the implementation
owns the network bound, not the caller", rejected 3× across #508/#636).

What I *did* do, so the next reviewer does not have to find the answer a screen away: the
position is now written at the loop itself (`crates/core/src/metadata.rs:2130-2139`), citing the
trait clause and the two drivers. And on "a stalled backend hangs startup recovery": the pre-#652
walk had the identical exposure through three unbounded `scan` awaits
(`origin/main:crates/core/src/metadata.rs:2077`, `:2094`, `:2105`); this patch leaves **one**.
Both findings are recorded in `review-rejected.md` at the lines they were reported at.

## 4. The DST test-gap — coverage added; the *location* rejected, with the cost

The finding is right that a compare-and-set repair racing an allocator is concurrent correctness
logic that a unit test does not bind. Two tests now bind it, both driving production code over a
real redb store:

* **`a_damaged_counter_repair_never_rewinds_an_allocator_that_won_the_race`**
  (`crates/server/tests/gateway_recover_totality.rs:771`) — the decisive interleaving is
  *injected*, not raced for: `RacingCounterStore` (`:706`) hands the seed the damaged bytes that
  were there when it read, and — before returning them — commits the write a peer would have made
  (repair to 6, then ids 6..=10 spent, counter at 11, with an `inode:` key for each). The seed's
  guarded write therefore lands on a counter that has moved past it. `recover()` must still be
  `Ok(())`, the counter must still read 11, and the next `alloc_inode` must return 11 — not 6,
  which is live. Deterministic on every run.
* **`concurrent_recoveries_and_allocations_over_a_damaged_counter_never_reuse_a_live_id`**
  (`:858`) — three `Gateway::recover()` calls on their own threads against five `alloc_inode`
  contenders over one shared store (`SharedRedb`, `:152`, a forwarder to one real redb handle —
  the same shape as the existing `ScanRefusingStore`). Every recovery `Ok`, every id distinct and
  above every committed inode, and the counter ends above every id handed out.

**Why not `crates/dst` (the rubric's "seeded Tier-0")**, with the cost rather than an adjective:
`wyrd-dst` compiles only under `--cfg madsim` and would need `wyrd-server` as a dev-dependency.
`crates/server/Cargo.toml` has **no** `[target.'cfg(madsim)']` section: it depends on real `tonic`
while `wyrd-chunkstore-grpc` aliases `tonic` → `madsim-tonic` under that cfg, so the two disagree
on the transport types `server` hands between them; and the other half of the race the finding
names, `alloc_inode`, sleeps on real `tokio::time` (`crates/server/src/cli.rs:1806`), which under
the simulator must be madsim's timer. Making this code DST-reachable means **moving the persisted
allocator protocol out of `wyrd-server` into `wyrd-core`** — `PersistedInodeCounter`,
`read_persisted_inode_counter`, `counter_already_at_floor`, `seed_next_inode_floor`, plus a
core-side single-attempt CAS for `alloc_inode` to keep its tokio budget in `server`: ≈250 lines
moved across a crate boundary, a public-API change, their unit tests re-homed, and a new DST
target on top. That is a restructure Plan did not scope, in a slice whose size backstop has
already fired. The tree's own precedent for this allocator's concurrency is a **server-level**
contention test, not a DST leg: `crates/server/tests/gateway_multi_writer.rs:265-295` contends
the production `cli::alloc_inode` from 16 tasks for exactly this reason. And the CAS loop itself
is not new — `origin/main:crates/server/src/cli.rs:1685-1712` already read-then-CAS-then-retried;
this patch changes which value it writes when the reading is damage.

If the maintainer wants the DST leg anyway, it is a clean follow-up issue ("move the persisted
inode-allocator protocol into `wyrd-core` and add a seeded DST race"), not a change this bundle
should carry.

## 5. Fitness-to-purpose: what the walk costs in TIME (the human's open question)

Measured on this host, redb in-memory, release build, through the production
`high_water_marks` (throwaway target, removed after the run):

| rows under `inode:` | pages (`RECOVERY_PAGE` = 128) | walk | per row |
|---|---|---|---|
| 10 000 | 80 | 2.5 ms | 253 ns |
| 100 000 | 783 | 27.2 ms | 271 ns |
| 400 000 | 3 126 | 111.5 ms | 278 ns |

Linear, as expected. Extrapolated: **10 M objects ≈ 2.8 s** of startup on the embedded backend.
On a **networked** backend the round trips dominate rather than the CPU: `ceil(rows/128) + 1`
requests, so 1 M objects ≈ 7 800 round trips (≈ 8 s at 1 ms) and 10 M ≈ 78 000 (≈ 78 s). That is
the honest number: at ten million objects on TiKV/FDB this walk is a minute-plus of startup.

Three things make that acceptable *for this patch*, and the third is the answer to "what bound is
actually needed":

1. **The baseline is not "fast startup", it is "no startup".** `scan` fails loud at `SCAN_CAP`
   = 2^20 **rows** (`crates/traits/src/lib.rs:286`), so on `origin/main` a store past ~1.05 M
   inode records makes `recover()` return `Err(ScanCapExceeded)` and the gateway **never serves
   at all**. This patch turns "will not start" into "starts in seconds to a minute" — the
   availability floor C-1 is about.
2. **It also strictly *reduces* startup work.** The pre-#652 walk read three namespaces
   (`inode:`, `pending:`, `orphan:` — `origin/main:crates/core/src/metadata.rs:2077`, `:2094`,
   `:2105`) to feed a chunk-id mark nobody read. This one reads one.
3. **The bound that would actually be needed is a floor recovery does not have to derive.** Every
   row must be visited because the mark is a *maximum* over ids whose keys are decimal text —
   `inode:99` sorts after `inode:100`, so key order is not id order and there is no bounded
   "read the largest key". Stopping early answers with the largest id *seen*, i.e. the quiet
   under-approximation the invariant forbids. The store already holds an O(1) floor —
   `meta:next_inode` — and this walk exists for the two states in which that number cannot be
   trusted (**absent**: the legacy store of #364 finding 1; **damaged**: this issue). So the
   available bound is "skip the walk when the counter reads", and that is a **behaviour trade,
   not an optimisation**: it gives up today's silent repair of a counter that reads but is stale
   (an inconsistent partial restore, where the counter is older than the `inode:` keys), whose
   symptom is a spurious `Conflict` on every new-key PUT. I did not take that trade in this
   bundle — it is a separate decision with its own test surface, and the brief scopes this slice
   to totality + boundedness. It is written up in the code so the next reader inherits the
   analysis rather than re-deriving it (`crates/core/src/metadata.rs:2201-2222`).

## 6. Forced self-refutation (the three questions)

**(a) Genuine red?** Yes — through the project's own runner, and twice more by targeted mutation.

1. `PDCA_BUNDLE=… ./engine/scripts/run-verify.sh` (resets `../wyrd-verify` to `origin/main`,
   applies `patch.diff`, reverts the production files, keeps the added test):

   ```
   run-verify.sh: GREEN — cargo test -p wyrd-server --test gateway_recover_totality (fix applied)
     test result: ok. 7 passed
   run-verify.sh: RED — … (production reverted, test kept)
     a_damaged_counter_repair_never_rewinds_an_allocator_that_won_the_race … FAILED
     concurrent_recoveries_and_allocations_over_a_damaged_counter_never_reuse_a_live_id … FAILED
     a_damaged_counter_is_reseeded_even_when_the_proven_floor_is_the_allocator_default … FAILED
     an_exhausted_inode_space_fails_… … FAILED
     recover_is_total_over_a_damaged_next_inode_counter_and_reseeds_it … FAILED
     recover_is_total_over_a_store_whose_scan_refuses_… … FAILED
     recover_is_total_over_damaged_inode_records_and_attributes_each … FAILED
     test result: FAILED. 0 passed; 7 failed
   run-verify.sh: PASS — red without the fix, green with it.
   ```

   All seven are **assertion** panics inside the test file (lines 310, 408, 485, 586, 649, 810,
   932), not compile errors — the target names only `Gateway::recover()`, `put_object`,
   `get_object` and `cli::alloc_inode`, all signature-stable across this patch.
2. *Mutation: the lenient parse* (`parse_canonical_u64(text)` → `text.parse::<u64>().ok()`, i.e.
   round 4's code). `cli::tests::the_persisted_inode_counter_has_three_distinct_readings` fails
   (`"+1" is not a canonical decimal u64 and must read as damage, got Readable(1)`) **and** the
   new end-to-end test fails on its attribution assertion (a `+1` counter is silently "read", so
   nothing is attributed). Restored and re-run green.
3. *Mutation: the compare-and-set guard removed* (`WriteBatch::new().put(…)` instead of
   `require(exact bytes).put(…)` in `seed_next_inode_floor`).
   `a_damaged_counter_repair_never_rewinds_an_allocator_that_won_the_race` fails —
   "the repair must lose the race cleanly … rewinding it to the stale floor would re-mint ids
   6..=10, which are committed". The six other tests stay green, which is why that test exists.
   Restored and re-run green.

**(b) Production path?** Yes. The acceptance target composes the real `RedbMetadataStore` +
`FsChunkStore` + `MemCoordination` and calls the production `Gateway::recover()`, `put_object`,
`get_object`, `cli::alloc_inode`. The new counter grammar is the production
`wyrd_core::metadata::parse_canonical_u64` called from the production
`cli::read_persisted_inode_counter`. Nothing is re-implemented: the only doubles are
`ScanRefusingStore` (overrides `scan` only), `SharedRedb` (pure forwarder, so several owners can
hold one real store) and `RacingCounterStore` (a forwarder whose `get` additionally *commits a
real peer write* — fault injection on the production path, not a stand-in for it).

**(c) Fixture includes the fault?** Yes, in every test, and the two new ones are built so a
curated fixture could not pass them:

* the injected-race fixture keeps the **live** ids in the store — the peer commits `inode:6`…
  `inode:10` before the seed's write lands, so an implementation that rewinds hands out an id
  that is *demonstrably* occupied, not one that merely might be;
* its damaged counter is `+1` — the value the *previous* round's implementation read as a valid
  `1`, so the fixture contains the defect that round 4 shipped;
* the contention fixture's `inode:` values are undecodable too, so the floor can only come from
  the keys, and the counter is not UTF-8 at all;
* the earlier four fixtures are unchanged from round 4 (damaged + over-ceiling + unparsable-key
  rows above the healthy object; a scan-refusing store with the largest id on the last page and
  the byte-lexicographically last key deliberately small; an empty store where "damaged" and
  "absent" are otherwise indistinguishable; a `u64::MAX` record for the wrap case).

## 7. Gate evidence run locally (this iteration, final tree)

| Gate | Command | Result |
|---|---|---|
| C4-ci | `PDCA_WORKTREE=… ./engine/xtask.sh ci` | **pass** — `xtask ci: all checks passed` (fmt, clippy `-D warnings`, build, full `cargo test`, statics/unsafe/gitlink guards, madsim DST at 50 seeds, conformance vectors, **all three cargo-deny legs**, cargo-machete, typos) |
| C4-verify | `PDCA_BUNDLE=… ./engine/scripts/run-verify.sh` | **pass** — 7/7 green with the fix, 7/7 assertion-red without it |
| C5-mutants | `PDCA_BUNDLE=… scripts/mutants-in-diff` | **pass** — `15 mutants tested in 2m: 6 caught, 9 unviable` → **0 missed, 0 timeouts** (identical to round 4; this round adds tests, not production branches) |

The deny result is the direct answer to the C4 carry-forward: the reviewer's failure was a
host-local read-only advisory-database lock, not a dependency wall — the legs print
`advisories ok, bans ok, licenses ok, sources ok` here.

Commit-readiness: `cargo fmt --all` clean, `cargo clippy --workspace --all-targets` clean, and
`cargo xtask ci` (what the repo's own hooks and CI run) green on the final tree.

## 8. Budget

**995 semantic added lines** (≤ ~1 500) across **5 files** (≤ 15). Of those, 656 are the
acceptance target (`crates/server/tests/gateway_recover_totality.rs`, up from 421 — the two new
concurrency tests and their two store doubles) and ~210 are co-located unit tests
(`metadata.rs`'s `mod startup_recovery`, `cli.rs`'s `mod tests`). **Production** semantic lines
added are ~130 against 65 removed — the slice still shrinks the walk it hardens. No mechanical
migration: `high_water_marks`'s signature change has one callsite (`crates/server/src/lib.rs:140`)
and it is in the patch.

## 9. For the commit body (unchanged from round 4, still required by the brief)

> Deletes `high_water_marks_refuses_a_segmented_root_rather_than_re_mint_its_chunk_ids`. Its
> premise — a segmented root read as "owns no chunks" contributes nothing to `max_chunk`, so the
> next PUT could mint an id that object's fragments already occupy — expired with `fdd34f1`
> (#487, 2026-07-08), which replaced the sequential in-process chunk-id counter with a
> coordination-free scheme (`>= 2^127`) and, in the same commit, stopped `recover` reading the
> floor at all. The remaining minter is `>= 2^64`. No minter allocates in the `< 2^64` space the
> mark guarded, so the hazard is unreachable; the test guarded a number nobody read. Its live
> half — a walk must not silently under-count a record it cannot read — is superseded and
> strengthened by the totality requirement, bound by `mod startup_recovery` in the same file and
> end-to-end through `Gateway::recover()` by `crates/server/tests/gateway_recover_totality.rs`.

Also worth a line: startup recovery is total over **three** record classes (damaged `inode:`
values, a namespace larger than `SCAN_CAP`, a damaged `meta:next_inode` counter), and the counter
is read under the store's canonical decimal grammar, so `+1`/`01` are damage rather than a
silently low floor.

## 10. NEEDS-HUMAN

None from this build. No external dependency was missing (base Rust toolchain only; cargo-deny
0.20.2 and cargo-mutants are present and green on this host).

Two judgment items for sign-off, neither blocking a gate — both carried from round 4 and both
already accepted once:

1. The one-clause docs deletion (`docs/design/architecture/08-crosscutting-concepts.md:85`) — a
   clause this patch falsifies ("the id recovery that must not re-mint them"). Accepted at round
   4's §6; still a clean one-line revert if you change your mind.
2. The reseed-vs-leave-damaged behaviour decision (round 4 §3): a gateway now rewrites a
   persisted allocator record it could not read, loudly on the audit seam. Unchanged this round.

And one new judgment call, recorded so it is a decision rather than an omission: **the DST
location** (§4). I fixed the coverage where the tree covers this allocator today and rejected the
`crates/dst` placement on a stated cost (≈250 lines moved across a crate boundary plus a public
API change, because `wyrd-server` cannot compile under `--cfg madsim`). If you would rather have
the DST leg, the clean form is a follow-up issue that moves the persisted-allocator protocol into
`wyrd-core` — I would not bolt it onto this bundle.
