# Build notes — issue 652 / startup-recovery-total-and-bounded (iteration 4)

*Withheld from the reviewer; written for the human at sign-off.*

Worktree: `/home/eddie/wyrd/wyrd.pdca-wt-l1` at `d50f0ca` (= `origin/main`, the brief's target
base). All line citations below are **post-patch** lines in that worktree unless marked
"pre-fix" (which means `origin/main`).

---

## 1. What the carry-forward asked for, and what this round changes

Round 3 was **not rejected on substance**: C4-ci, C4-verify and C5-mutants were all green and the
rubric review came back with no blocking implementation finding. One advisory finding was real
and is the whole reason for this round (brief, *Iteration 3 carry-forward*):

> `Gateway::recover()` calls the newly-fixed, bounded/total `high_water_marks` walk, then
> unconditionally calls `cli::seed_next_inode_floor`, which still does
> `std::str::from_utf8(bytes)?.parse()?` on the persisted `meta:next_inode` record. A single
> corrupted `meta:next_inode` value therefore still makes `recover()` return `Err` and takes the
> whole gateway down before it serves anything.

That is exactly right, and it was a real hole in the **Success criterion**, not a nicety:
criterion (1) says recovery must be total over damage *driven through `Gateway::recover()`*, and
`recover` is two steps — the walk (`crates/core/src/metadata.rs:2193`) **and** the seed
(`crates/server/src/cli.rs:1828`), run back to back at `crates/server/src/lib.rs:139-141`. A
property that holds for the first step and not the second is not the property. Verified on the
base before fixing: `git show origin/main:crates/server/src/cli.rs` line **1696** is
`Some(bytes) => std::str::from_utf8(bytes)?.parse()?`, inside `seed_next_inode_floor`.

Everything else from round 3 is **kept** as the carry-forward instructs ("Keep the rest of this
round's scope — only this one path needs closing"): the paged `scan_page` walk, the per-record
attribution, the deleted chunk-id half, the `alloc_inode` exhaustion fix, the co-located core
units. The delta is below.

## 2. The change, file by file

**`crates/core/src/metadata.rs`** — unchanged from round 3 except one line: `RECOVERY_AUDIT` is
now **`pub`** (`:2048`), with the reason in its doc. Recovery is two steps in two crates and an
operator's repair queue is one query; a second target for the second step would mean a filter
that selects one and misses the other. (Round 3's content, unchanged: `RECOVERY_PAGE` `:2066`,
`read_inode_value` `:2095`, `for_each_page` `:2118`, the paged and total `high_water_marks`
`:2193`, the removed chunk-id mark + `pending:`/`orphan:` walks, the deleted standing test with
its reasoning in place at `:3540-3560`, and `mod startup_recovery` `:3575-3734`.)

**`crates/server/src/cli.rs`** — the round's substance.

* `PersistedInodeCounter` + `read_persisted_inode_counter` (`:1650-1684`) — the persisted
  counter's **three** readings as a pure unit: `Absent`, `Readable(u64)`, `Damaged { detail }`.
  Damage becomes a *value*, so each caller can spend it as its own contract requires instead of
  every caller inheriting `?`.
* `ABSENT_COUNTER_NEXT_ID` + `counter_already_at_floor` (`:1686-1708`) — "the seed has nothing to
  do", per reading. `Damaged` is `false`: damage is **not** "already high enough". This is the
  one-way door — see §4(c).
* `alloc_inode` (`:1742-1757`) — same reading, **opposite answer**: fail *closed for that record*
  with a message that names the repair (a restart reseeds from the store's own `inode:` keys).
  An allocation cannot invent the id it hands out. This also replaces a raw `Utf8Error` with a
  sentence an operator can act on.
* `seed_next_inode_floor` (`:1828-1856`) — attributes a damaged counter on `RECOVERY_AUDIT`
  (`action = "next-inode-counter-undecodable"`, with `detail` and the `floor`) and **reseeds it**
  from the mark, CAS-guarded on the exact damaged bytes it read. `recover()` is `Ok(())`.

**`crates/server/src/lib.rs`** — `recover`'s doc now states the totality claim for *both* steps
(`:124-133`); the body is round 3's one-line change (`:140`, the discarded `_max_chunk` gone).
One pre-existing citation two lines above the hunk I was editing was stale and is corrected in
passing: `cli.rs:1415` → `cli.rs:2256`, the actual `gateway.recover()` callsite (`-1/+1`, no
semantic line).

**`crates/server/tests/gateway_recover_totality.rs`** — two tests added to the brief's named
target (now 5), plus `persisted_next_inode_raw` so a *damaged*-counter assertion fails at the
assertion instead of inside a parsing helper. See §4.

**`docs/design/architecture/08-crosscutting-concepts.md:85`** — unchanged from round 3: **one
clause deleted, nothing added.** The sentence listed "the id recovery that must not re-mint them"
among the chunk-map consumers that fail closed on an unresolvable shape; after this patch id
recovery reads no chunk map at all, so the clause is simply false. This is truth-maintenance of a
clause *this patch falsifies* (`-1/+1`, zero semantic lines), not the living-architecture
paragraph the brief puts out of scope. Round 3's reviewer routed the CLI+docs breadth to
NEEDS-HUMAN (C3) rather than calling it a defect, and the human's carry-forward then *required*
the `cli.rs` half — so the remaining judgment call for sign-off is this one clause. **If you
prefer it dropped, it is a clean one-line revert with no test consequence.**

## 3. The decision this round turns on: reseed vs. leave-damaged

A damaged `meta:next_inode` has exactly two answers that the brief's invariant permits ("*fail
closed for that record* or *contribute its true floor* — **never zero**"). I took different ones
in the two callers, deliberately:

| Caller | Answer | Why |
|---|---|---|
| `alloc_inode` (write path) | **fail closed for the record** | The id it hands out *is* the counter's value. It cannot prove a replacement, and inventing one would hand out an id another writer may still hold. Contained to the one PUT. |
| `seed_next_inode_floor` (startup) | **reseed from the proven floor** | It is the only step that *can* prove a replacement, and failing closed here fails closed for the whole gateway. |

**Why the reseed cannot re-mint a live id.** The floor is `max(inode: key) + 1`, taken from
**keys**, and `metadata::create` is `require_absent(inode_key(id))`
(`crates/core/src/metadata.rs:1560`) — so every *committed* object has an `inode:` key and the
floor is strictly above all of them. The ids a reseed re-opens are ids no committed record names.
This is not a new construction: it is exactly what the **absent**-counter path (issue #364
finding 1, `crates/server/tests/s3_http_wire.rs:645-700`) has done since it landed, applied to a
third state that today is a hard outage.

**Residual, stated honestly.** An id whose object was *deleted* but whose fragments are still in
the orphan ledger awaiting GC could be re-opened, and on the **cluster CLI path only** (whose
chunk ids derive from the inode, `cli.rs:1861-1868`) the new object would then mint chunk ids that
ledger already names. That residual is *identical* for the absent-counter path and is unchanged
by this patch — the gateway path mints `>= 2^127` from a random epoch and cannot collide at all.
Making it disappear would require the counter to be recoverable from something other than the
store's keys, i.e. a second persisted allocator record, which is a different issue.

**Alternatives rejected, with costs (not adjectives):**

* **Attribute and leave the damaged counter in place** (recovery returns `Ok(())`, writes nothing;
  −13 production lines vs. what I shipped — the reseed arm plus its unit). Rejected: `alloc_inode`
  then fails on *every* new-key PUT for as long as the record stays damaged, so the gateway starts
  and serves reads but is permanently write-dead until a human hand-writes a counter — and the
  number that human must invent is precisely the floor the store's own keys already prove. It also
  fails the brief's own criterion-(1) shape, which requires "a subsequent new-key PUT **commits**"
  after damage. Recorded in `review-rejected.md` at `cli.rs:1828`.
* **Read the damaged counter as the absent default (`1`) and let the existing `have >= floor`
  comparison decide** (−7 lines: no `counter_already_at_floor`, no third arm). Rejected as the
  *quiet under-approximation* the invariant names: over any store whose proven floor is `1` it
  answers "already high enough", returns `Ok(())` **without writing**, and leaves an allocator no
  PUT can read — while looking correct on every non-empty store. This is not theoretical: it is
  the exact mutant I ran (§4a), and it is why the fifth test exists.
* **Make `alloc_inode` total too** (reseed at allocation time, ≈10 lines). Rejected: an allocation
  that cannot read the counter has no floor to prove — it would have to run the `inode:` walk
  itself on the hot PUT path (an unbounded-in-practice cost per allocation) to earn the same
  guarantee recovery already earned once at startup.
* **A new `RECOVERY_AUDIT` constant in `wyrd-server`** instead of making core's `pub` (+1 line,
  −1 cross-crate item). Rejected: two literals for one operator filter is how a repair queue
  silently loses half its rows; the brief's own attribution requirement is "one filter selects the
  operator's repair queue".

**Why `counter_already_at_floor` is a separate function.** Not style — it is the mutation gate.
With the comparison inline in the loop, `cargo mutants` produced a `>= → <` mutant that no test
killed *quickly*: it made the seed a no-op, which hangs `crates/server/tests/custodian_gc.rs`
(seven tests "running for over 60 seconds") and scored **Timeout** rather than *caught*. Extracted
into a unit with a `wyrd-server` lib unit test, the same mutant now fails in the first test binary
`cargo test` runs and is scored **caught**. Before: 12 mutants, 1 timeout. After: 15 mutants, 6
caught, 9 unviable, **0 missed, 0 timeouts**.

## 4. Forced self-refutation (the three questions)

**(a) Genuine red?** Yes — and twice over, both through the project's own runner.

1. *Fix reverted, test kept* (`engine/scripts/run-verify.sh`, which resets `../wyrd-verify` to
   `origin/main`, applies `patch.diff`, then reverts the production files and keeps the added
   test):

   ```
   run-verify.sh: GREEN — cargo test -p wyrd-server --test gateway_recover_totality (fix applied)
     test result: ok. 5 passed
   run-verify.sh: RED — … (production reverted, test kept)
     recover_is_total_over_a_damaged_next_inode_counter_and_reseeds_it … FAILED
       recover() must be Ok(()) over a damaged `meta:next_inode` …: Utf8Error { valid_up_to: 0, … }
     a_damaged_counter_is_reseeded_even_when_the_proven_floor_is_the_allocator_default … FAILED
       recover() must be Ok(()) over a damaged allocator on an empty store: ParseIntError { … }
     recover_is_total_over_damaged_inode_records_and_attributes_each … FAILED
       …: Error("expected ident", line: 1, column: 2)
     recover_is_total_over_a_store_whose_scan_refuses_… … FAILED
       …: ScanCapExceeded { cap: 1048576, prefix: [105,110,111,100,101,58] }
     an_exhausted_inode_space_fails_… … FAILED
     test result: FAILED. 0 passed; 5 failed
   run-verify.sh: PASS — red without the fix, green with it.
   ```

   All five are **assertion** failures through the signature-stable `Gateway::recover()`, not
   compile errors — which is why the target sits at the `wyrd-server` level and never names
   `high_water_marks` (whose signature this patch changes). The two new ones fail on exactly the
   `Utf8Error` / `ParseIntError` the carry-forward predicted.

2. *Targeted mutation of the new decision*, run through the same gate against a scratch bundle
   (`$PDCA_SCRATCH/pdca-builder-652-mutantprobe`, since removed): with `Damaged` read as the
   absent default `1` — the single most plausible wrong implementation — the GREEN leg fails:

   ```
   a_damaged_counter_is_reseeded_even_when_the_proven_floor_is_the_allocator_default … FAILED
     the damaged counter must be replaced by the proven floor even when that floor equals the
     absent-counter default … left: Some([57,48,48,48,10,40,116,111,114,110,41])  right: Some([49])
   ```

   The four other tests stay green under that mutant — which is the point of the fifth: on any
   non-empty store the two implementations are observationally identical.

**(b) Production path?** Yes. The acceptance target composes the real `RedbMetadataStore` +
`FsChunkStore` + `MemCoordination` gateway and calls the production `Gateway::recover()`,
`put_object`, `get_object`. The counter it asserts is read back from the real persisted
`meta:next_inode` key with a helper that does no parsing. The co-located units call the production
`read_persisted_inode_counter` / `counter_already_at_floor` / `high_water_marks` /
`read_inode_value` directly. Nothing is re-implemented; the only double is `ScanRefusingStore`,
which overrides exactly one method (`scan`) and forwards the rest to redb — that is the *fault
injection*, not a stand-in for the unit under test.

**(c) Fixture includes the fault?** Yes, in every test, and the two new ones are built so a
curated fixture could not pass them:

* `recover_is_total_over_a_damaged_next_inode_counter_and_reseeds_it` corrupts the counter **in
  place over a store with two healthy committed objects** — the damage is the subject, so the test
  cannot pass on the strength of the walk's own totality. It asserts the repaired bytes exactly
  (`b"3"`, the floor the keys prove), the attribution on the audit target, both objects still
  byte-identical, and that the next PUT commits at 3 while object A is untouched.
* `a_damaged_counter_is_reseeded_even_when_the_proven_floor_is_the_allocator_default` is the
  *discriminating* fixture: an empty store, where "damaged" and "absent" compare identically
  unless damage is its own answer. Proven binding by the mutation run in (a2).
* Criterion 1's store still holds the healthy object **and** three unreadable `inode:` rows whose
  ids sit *above* it, so the asserted floor (61) can only have come from a damaged row's key.
* Criterion 2 asserts the double really refuses *before* the scenario runs, holds 258 rows across
  >1 page, and puts the numerically largest id on the last page while the byte-lexicographically
  last key (`inode:99`) is deliberately small — a one-page walk answers 227, a last-row-wins walk
  answers 100, only the correct walk answers 251.
* Mutation-checked as a whole: **15 mutants, 6 caught, 9 unviable, 0 missed, 0 timeouts.**

## 5. Gate evidence run locally (this iteration, final tree)

| Gate | Command | Result |
|---|---|---|
| C4-ci | `PDCA_WORKTREE=… ./engine/xtask.sh ci` | **pass** — `xtask ci: all checks passed` (fmt, clippy `-D warnings`, build, full `cargo test`, statics/unsafe/gitlink guards, DST, conformance vectors, cargo-deny all three legs, cargo-machete, typos) |
| C4-verify | `PDCA_BUNDLE=… ./engine/scripts/run-verify.sh` | **pass** — red without the fix, green with it (5/5 both ways) |
| C5-mutants | `PDCA_BUNDLE=… scripts/mutants-in-diff` | **pass** — 6 caught, 9 unviable, **0 missed**, **0 timeouts** |

Commit-readiness: `cargo fmt --all` is clean and `cargo xtask ci` — what the repo's own hooks and
CI run — is green on the final tree, so the publish commit will not be rejected by the target's
hooks. Every `path:line` citation in the touched files was re-checked against the **final** tree
after the last reflow (the anchors moved twice as the patch grew); `review-rejected.md` is
re-anchored to this attempt's lines, as the brief requires.

## 6. For the commit body (the brief requires the deletion's reasoning to travel with it)

> Deletes `high_water_marks_refuses_a_segmented_root_rather_than_re_mint_its_chunk_ids`. Its
> premise — a segmented root read as "owns no chunks" contributes nothing to `max_chunk`, so the
> next PUT could mint an id that object's fragments already occupy — expired with `fdd34f1`
> (#487, 2026-07-08), which replaced the sequential in-process chunk-id counter with a
> coordination-free scheme (`>= 2^127`) and, in the same commit, stopped `recover` reading the
> floor at all. The remaining minter is `>= 2^64`. No minter allocates in the `< 2^64` space the
> mark guarded, so the hazard is unreachable; the test guarded a number nobody read. Its live
> half — a walk must not silently under-count a record it cannot read — is superseded and
> strengthened by the totality requirement, bound by `mod startup_recovery` in the same file and
> end-to-end through `Gateway::recover()` by
> `crates/server/tests/gateway_recover_totality.rs`.

Also worth a line in the body: startup recovery is total over **three** record classes now —
damaged `inode:` values, a namespace larger than `SCAN_CAP`, and a damaged `meta:next_inode`
counter — because `recover` is two steps and a property that holds for one is not the property.

## 7. Budget

**748 semantic added lines** (≤ ~1,500), **5 files** (≤ 15). Of those, 421 are the acceptance
target and 190 are co-located units (119 in `metadata.rs`'s `mod startup_recovery`, 71 in
`cli.rs`'s `mod tests`); **production** semantic lines added are ~137 against 65 removed — the
slice still shrinks the walk it hardens, as the brief predicted. No mechanical migration: the
`high_water_marks` signature change has exactly one callsite (`lib.rs:140`) and it is in the
patch.

## 8. NEEDS-HUMAN

None from this build. No external dependency was missing: base Rust toolchain only, no new
dev-dependency, no Docker/protoc/live backend, and cargo-deny is at 0.20.2 on this host (the round-2
blocker), with `cargo xtask ci` green end to end including both deny legs.

Two judgment items for sign-off, neither blocking a gate:

1. The one-clause docs deletion (§2) — keep or revert; a clean `-1/+1`.
2. The reseed-vs-leave-damaged decision (§3) is a **behaviour** decision, not a test artefact: a
   gateway now silently (well — loudly, on the audit seam) rewrites a persisted allocator record it
   could not read. I believe it is right and it is what the carry-forward asked for, but it is the
   kind of call a maintainer should confirm rather than inherit.
