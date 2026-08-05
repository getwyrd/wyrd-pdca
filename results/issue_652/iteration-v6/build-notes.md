# Build notes — issue 652 / startup-recovery-total-and-bounded (iteration 6)

*Withheld from the reviewer; written for the human at sign-off.*

Worktree: `/home/eddie/wyrd/wyrd.pdca-wt-l1` at `d50f0ca` (= `origin/main`, the brief's target
base). Line citations are **post-patch** lines in that worktree unless marked "pre-fix" / "on
base" (which means `origin/main`).

---

## 1. Round 6, item by carry-forward item

Round 5's carry-forward named three concrete bugs and said explicitly: *fix these, do not
re-litigate scope*. All three are **fixed** (none rejected), and one of them required reversing
a rejection this bundle recorded in round 4 — flagged in §3 because it is a behaviour decision
the human should see, not a detail.

| Carry-forward item | Disposition |
|---|---|
| **1. Unbounded retry / hang.** `seed_next_inode_floor` retried `CommitOutcome::Conflict` forever, so `Gateway::recover` could fail to reach a result (T3 Runtime FAIL; suspected cause of the C4 `cargo xtask ci` timeout). | **FIXED.** The loop is bounded by `SEED_FLOOR_ATTEMPTS` (`crates/server/src/cli.rs:1727`, used at `:1857`) and fails closed with a named error (`:1894-1899`). Bound end-to-end by a new test that *fails* without it (§4). The red leg measured the old loop spinning **4 129 099** conflicts in 15 s — the hang is not hypothetical. |
| **2. Unsafe damaged-counter reseed.** Recomputing the counter from committed `inode:` keys can reuse an id whose fragments are still live under an `orphan:` grace record, or rewind past an id already handed to a concurrent in-flight allocator. *"Needs a reseed strategy that cannot regress below any live or in-flight id."* | **FIXED, by removing the reseed.** A damaged counter is attributed on the recovery audit seam and **left exactly as stored**; `alloc_inode` fails closed on it (`cli.rs:1856-1878`, `:1766-1776`). Nothing is written, so nothing can regress. §3 has the argument, the cost, and the rejected alternative with its line count. |
| **3. Lenient `parse_inode_key`.** `.parse()` accepts `inode:+7`, `inode:007`, `inode:+18446744073709551615`, so damaged keys are treated as valid and can "falsely exhaust the allocator"; the acceptance test covered only `inode:not-a-number`. | **FIXED.** The key grammar is the store's own canonical-decimal reader (`crates/core/src/metadata.rs:2036-2060`, `parse_canonical_u64`), a refused key is attributed rather than skipped (`:2258-2273`), and the coverage is extended in both places — unit (`metadata.rs:3698-3758`) and end-to-end (`crates/server/tests/gateway_recover_totality.rs:320-323`, `:380-388`). §5. |

Everything else from round 5 that the gates and sign-off accepted is kept: the paged
`for_each_page` walk, the removal of the dead chunk-id mark with its two `scan`s and its
standing test, the `alloc_inode` canonical-grammar read and `checked_add`, the one-clause docs
deletion. The patch is **~40 production lines smaller** than round 5's because the
damaged-counter *repair* apparatus (its CAS-race semantics, its two concurrency tests and their
two store doubles) is gone rather than defended.

## 2. What the patch does, in one paragraph per file

* **`crates/core/src/metadata.rs`** — `high_water_marks` returns one mark (`InodeId`), reads
  `inode:` in bounded pages through `for_each_page` (`:2158`) instead of `scan`, takes each
  row's id from its **key** before looking at the value, and attributes every row it cannot
  account for on `RECOVERY_AUDIT` (`:2077`). `parse_inode_key` is the exact inverse of
  `inode_key` (`:2058`). The `pending:`/`orphan:` walks and `parse_pending_chunk_key` are
  deleted with the chunk mark they fed; so is the standing test that guarded it, with the
  reason left in place of the test (`:3604-3623`). New unit module `startup_recovery`
  (`:3639`).
* **`crates/server/src/cli.rs`** — one shared reading of the persisted counter
  (`PersistedInodeCounter` / `read_persisted_inode_counter`, `:1645`, `:1679`) under the same
  canonical grammar; `alloc_inode` fails closed on damage and on an exhausted id space
  (`:1766-1786`); `seed_next_inode_floor` is total over a damaged counter, leaves it alone, and
  is bounded (`:1856-1900`).
* **`crates/server/src/lib.rs`** — `recover()` consumes the single mark (`:145-148`) and its doc
  states both steps' totality and boundedness; two stale doc references to the removed mark
  fixed (`:258`, `:273`).
* **`docs/design/architecture/08-crosscutting-concepts.md:85`** — one clause deleted ("the id
  recovery that must not re-mint them"), which this patch falsifies. Accepted at round 4's
  sign-off; unchanged since.
* **`crates/server/tests/gateway_recover_totality.rs`** — the acceptance target, five tests.

## 3. The reseed decision — the round's one behaviour change, and a reversal

**What changed.** Rounds 4–5 *repaired* a damaged `meta:next_inode` by writing the walk's floor
over it. This round attributes it and leaves it; the gateway starts, reads/overwrites/deletes
keep working, and a **new-key** PUT fails closed until an operator repairs the record.

**Why the repair is not safe.** The floor is `max(inode: key) + 1`. `metadata::create` is
`require_absent` on the inode key, so that is provably above every id a **committed** record
holds — and that is *all* it is above. The counter's true value also stands above every id that
was handed out and is **not** committed under an `inode:` key:

* an id a concurrent gateway is mid-PUT with (M4 runs gateways active-active over one store,
  `crates/server/src/lib.rs:70-73`); and
* an id whose object was deleted while its fragments are still deadlined under an `orphan:`
  grace record (`crates/custodian/src/gc.rs:197-201`).

The cluster path derives an object's chunk ids from its inode id — `(inode << 64) | seq`,
`cli::chunk_id_minter` (`crates/server/src/cli.rs:1904-1911`) — so re-minting either kind of id
writes a *live* object's chunk ids: fragments clobbered on the shared chunk store, or a
still-armed grace record deadlining the new object's bytes. That is the C-1 failure class
(data loss), reached by a step that exists to prevent it.

**Why no derived value fixes it.** A damaged counter's true value is unknowable — corruption is
arbitrary and the tree keeps no second copy — so no number computed from the store can be
*proven* at or above it. "Cannot regress below any live or in-flight id" therefore has exactly
one implementation: do not write.

**The alternative I rejected, with its cost rather than an adjective.** A floor that *witnesses*
the two record classes above is constructible: `pending:<chunk>` and `orphan:<dserver>:<chunk>:<index>`
keys carry cluster-path chunk ids, and `chunk >> 64` recovers the inode id from each. Cost:

* two more paged walks in `high_water_marks` — the `pending:` and `orphan:` walks this slice's
  Plan decision **deliberately removes** ("two fewer unbounded `scan` calls at startup, which
  serves this slice's own 'total and bounded' goal"), re-added under a different justification;
* concretely ~55 production lines (`parse_pending_chunk_key` restored, 8; an orphan-key
  projection, ~10; two `for_each_page` calls with their `>> 64` filters, ~20; doc, ~15) plus
  two more unit tests and a fixture per class;
* startup cost goes from `ceil(N_inode/128) + 1` round trips to
  `ceil(N_inode/128) + ceil(N_pending/128) + ceil(N_orphan/128) + 3` — and the orphan ledger is
  precisely the namespace #634 was merged for ("one maximum segmented-object retirement
  installs ~1.78 M marks", `crates/traits/src/lib.rs:1032-1035`), i.e. potentially the largest
  of the three;
* and it **still** would not witness a peer gateway's in-flight id, because on the gateway path
  `alloc_inode` runs after the intent phase and the pending ledger holds `>= 2^127` epoch ids
  that encode no inode. So the extra 55 lines buy a floor that is *more* often right and never
  *provably* right — which is the same class of "safe by construction" claim that failed
  review this round.

**The cost of what I did ship**, stated plainly so sign-off can weigh it: after a damaged
counter, every **new-key** PUT fails with an error naming `meta:next_inode`, until an operator
writes a counter. Reads, overwrites and deletes of existing objects are unaffected (none of
them allocate — `Gateway::commit_written`, `crates/server/src/lib.rs:218-243`), and the
attribution carries the proven floor as the operator's lower bound (`cli.rs:1864-1876`). This
is the same answer the tree's allocator already gives for this record on `origin/main`
(`alloc_inode` propagates `from_utf8(bytes)?.parse()?`), so the patch makes recovery
*consistent* with the allocator instead of inventing a repair the allocator would not trust.

**This reverses a rejection this bundle recorded in round 4** ("startup recovery should leave a
damaged `meta:next_inode` in place … rejected on the invariant"). It is recorded as a reversal
with its reason in `review-rejected.md` (iteration-6 header) rather than quietly dropped, and
the counter-rejection — *"repair it automatically"* — is now recorded at `cli.rs:1856` so the
next reviewer gets the argument rather than the round-4 text. If the maintainer prefers the
automatic repair despite the clobbering hazard, that is a legitimate call to make at sign-off;
it is a ~20-line change back (restore the write plus its CAS guard) and I would want the
`pending:`/`orphan:` witnesses with it, which is Plan's decision to reopen, not Do's.

## 4. The bound — a count, not a timer, and why

`seed_next_inode_floor` now runs at most `SEED_FLOOR_ATTEMPTS` (8) read/CAS attempts and then
returns `Err` (`cli.rs:1727`, `:1857`, `:1892`). Three notes:

* **No `tokio::time` backoff**, deliberately. `alloc_inode` spends a wall-clock budget with
  capped exponential backoff because it must eventually *win* for a user-facing PUT. The seed
  runs inside `Gateway::recover`, i.e. before the gateway serves anything, on whatever executor
  the composition root is on — and this repo's own recovery tests drive it under
  `pollster::block_on` (`crates/server/tests/s3_http_wire.rs`, and the new target), where a
  `tokio::time::sleep` **panics** for want of a reactor. A count needs no timer. It is also the
  right shape: every writer of this key only ever *raises* it, so a lost race is progress and
  re-reading, not waiting, is how the loop converges.
* **Failing closed on exhaustion is the safe direction.** Returning `Ok(())` with the counter
  still below the floor would let `alloc_inode` hand out an id a committed object holds — the
  #364 re-mint. An `Err` is a transient startup refusal a supervisor retries; nothing is
  written and nothing is rewound (asserted).
* **It is bound by a test that fails without it.**
  `recovery_reaches_a_result_over_a_permanently_conflicting_counter`
  (`crates/server/tests/gateway_recover_totality.rs:654`) runs `recover()` over a store whose
  every commit answers `Conflict`, on its own thread with a 15 s wall-clock guard — so the
  unbounded loop is a **test failure**, not a hung suite (which is how this defect would
  otherwise have taken the whole gate down again). On the red leg it failed after serving
  4 129 099 conflicts; on the green leg it returns an `Err` after 8.

## 5. The key grammar, and why strict is the safe direction here

`parse_inode_key` used `str::parse`, which accepts `+7`, `007` and `+18446744073709551615`.
Round 5's build notes argued for keeping it lenient ("a lenient key reading would *raise* the
mark — untidy, safe"). That is wrong in one direction and the review caught it: a key whose
digits spell `u64::MAX` seeds the persisted allocator at the ceiling of the id space, after
which `alloc_inode` fails **every** new-key PUT for want of a free id. One stray row would cost
the store its writes — a certain availability loss, traded against a speculative one.

Refusing the row costs nothing that is provably real: `inode_key` formats with `u64::to_string`
and `create` is `require_absent` on those exact bytes, so every *committed* object's id is
spelled canonically and still contributes. A non-canonical key is a row no writer in this tree
produced; it is attributed on the audit seam (`action = "inode-key-unparsable"`), which is the
repair obligation, not a silent skip. The residual — key bytes corrupted from a canonical
spelling into a parseable non-canonical one, e.g. `inode:70` → `inode:007` — loses that id from
the mark, but such a record is already unreachable to the resolver (its key is not the one any
reader derives) and the attribution names it. I preferred that to the certain outage above.

The counter (`meta:next_inode`) is read under the same grammar for the opposite reason — there a
lenient reading *lowers* the floor (`+1` → `1`) and hands out live ids. One parser, two
directions, both stated at their callsites (`cli.rs:1668-1678`, `metadata.rs:2040-2057`);
`AGENTS.md` *Grammar strictness* asks for exactly this (extend the shared parser, no `+`/`-` or
free digit widths via `from_str`).

## 6. Forced self-refutation (the three questions)

**(a) Genuine red?** Yes — through the project's own runner, and again under four mutation
experiments (one mechanical sweep, three targeted).

1. `PDCA_BUNDLE=results/issue_652 ./engine/scripts/run-verify.sh` (resets `../wyrd-verify` to
   `origin/main`, applies `patch.diff`, reverts the three production files + the docs line,
   keeps the added test):

   ```
   run-verify.sh: GREEN — cargo test -p wyrd-server --test gateway_recover_totality (fix applied)
     test result: ok. 5 passed
   run-verify.sh: RED — … (production reverted, test kept)
     an_exhausted_inode_space_fails_the_next_put_closed_… — recover() … Error("expected ident")
     recover_is_total_over_a_damaged_next_inode_counter_… — recover() … Utf8Error { … }
     recover_is_total_over_a_store_whose_scan_refuses_…   — recover() … ScanCapExceeded { cap: 1048576 }
     recover_is_total_over_damaged_inode_records_…        — recover() … Error("missing field `size`")
     recovery_reaches_a_result_over_a_permanently_conflicting_counter — must reach a result
       within 15s … Conflicts served so far: 4129099
     test result: FAILED. 0 passed; 5 failed
   run-verify.sh: PASS — red without the fix, green with it.
   ```

   All five are **assertion** panics inside the test file (the `recover()` expectations at `:344`, `:454`, `:603`, `:766` and the contention guard at `:702`), not
   compile errors: the target names only `Gateway::{recover, put_object, get_object}`,
   `inode_key`, `MAX_VALUE_BYTES` and `logging::{dispatch, LogConfig}`, all signature-stable
   across this patch. `--classify` reports the single discriminator
   `ADDED_TEST crates/server/tests/gateway_recover_totality.rs`.
2. `cargo mutants --in-diff` over this patch: **13 mutants, 4 caught, 9 unviable, 0 missed**
   (§7a). Nothing on the diff survives a mutation the suite cannot see.
3. Three **targeted** mutations, each run through the same `run-verify.sh` (mutation applied in
   the worktree, patch regenerated into a scratch bundle, gate re-run; the tree was restored
   byte-identically after each — verified by `diff` against `patch.diff`):

   | Mutation | Result |
   |---|---|
   | `parse_inode_key` back to `.parse().ok()` (round 5's code) | **1 failed, 4 passed** — `recover_is_total_over_damaged_inode_records_and_attributes_each` at the floor assertion (`gateway_recover_totality.rs:363-369`): `left: Some(18446744073709551615)`, `right: Some(61)`. The `inode:+18446744073709551615` row seeds the allocator at the ceiling of the id space, exactly the "falsely exhaust the allocator" hazard the carry-forward named. (The two new unit tests in `metadata.rs` assert the refused spellings directly, so they bind it at the unit too.) |
   | `for _ in 0..SEED_FLOOR_ATTEMPTS` → `loop` (round 5's unbounded retry) | **1 failed, 4 passed** — `recovery_reaches_a_result_over_a_permanently_conflicting_counter` at its wall-clock guard (`:701-709`), after serving **4 048 506** conflicts inside the 15 s guard. Only that test moves, which is why it exists. |
   | the damaged-counter arm made a write again (round 5's reseed) | **1 failed, 4 passed** — `recover_is_total_over_a_damaged_next_inode_counter_and_leaves_it_for_repair` at the byte-identity assertion (`:471-478`): `left: Some([51])` (the ASCII `3` the reseed wrote) vs `right:` the damaged bytes. The rewrite the round-5 review found unsafe is now a test failure. |

**(b) Production path?** Yes. The acceptance target composes the real `RedbMetadataStore` +
`FsChunkStore` + `MemCoordination` and calls the production `Gateway::recover()`, `put_object`
and `get_object`; the unit module drives the production `high_water_marks`,
`parse_inode_key` and `read_inode_value` over a real in-memory redb store; `cli::tests` drives
the production `read_persisted_inode_counter`. Nothing is re-implemented. The only doubles are
`ScanRefusingStore` (overrides `scan` only — everything else, `scan_page` included, forwards to
a real redb) and `ConflictingCounterStore` (forwards every read to a real redb and answers
`Conflict` to commits): fault injection *on* the production path, not a stand-in for it.

**(c) Fixture includes the fault?** Yes, in every test, and each fixture is built so a curated
one could not pass it:

* the damage fixture keeps the damaged rows **above** the healthy object's id (50, 60, and a key
  spelling `u64::MAX`), so the asserted floor can only come from a damaged row's key — and the
  `inode:+18446744073709551615` row is the exact input the *previous* round's code read as a
  valid id;
* the size fixture's largest id is on the **last** page and its byte-lexicographically last key
  (`inode:99`) is deliberately small, so "stopped after one page" and "kept the last row seen"
  both answer wrong;
* the damaged-counter fixture asserts the stored bytes are **unchanged** (not merely that
  `recover` returned `Ok`), and asserts the refusal that follows names the record — a fixture
  that swallowed the damage would pass neither;
* the contention fixture's store refuses **every** commit, so an implementation that "usually
  wins" cannot pass it, and the assertion is on the observed number of refused commits
  (2..=64), not merely on the call returning.

## 7. Gate evidence run locally (this iteration, final tree)

| Gate | Command | Result |
|---|---|---|
| C4-ci | `PDCA_WORKTREE=… ./engine/xtask.sh ci` | **pass** — `xtask ci: all checks passed` (fmt, clippy `-D warnings`, build, full workspace `cargo test`, statics/unsafe/gitlink guards, madsim DST, conformance vectors, **all three cargo-deny legs**: `advisories ok, bans ok, licenses ok, sources ok`, cargo-machete, typos). The round-5 timeout did not recur; the new target runs in 0.09 s inside it. |
| C4-verify | `PDCA_BUNDLE=… ./engine/scripts/run-verify.sh` | **pass** — 5/5 green with the fix, 5/5 assertion-red without it |
| C5-mutants | `PDCA_WORKTREE=… PDCA_BUNDLE=… scripts/mutants-in-diff` | see §7a |

Commit-readiness: `cargo fmt --all` clean, `cargo clippy --workspace --all-targets
--all-features` clean, and `cargo xtask ci` — what the repo's own hooks and CI run — green on
the final tree.

### 7a. Mutants

`13 mutants tested in 2m: 4 caught, 9 unviable` → **0 missed, 0 timeouts**. The four caught are
the three `>`/`==`/`<`/`>=` variants of `read_inode_value`'s ceiling check
(`crates/core/src/metadata.rs:2125`) and `Gateway::recover -> Ok(())`
(`crates/server/src/lib.rs:146`); the nine unviable are `Default::default()`-style replacements
for types with no `Default` (e.g. `parse_inode_key -> None`, `alloc_inode -> Ok(0)`), which do
not compile. Advisory by policy; the driver re-runs it at Check.

## 8. Budget

**879 semantic added lines** (≤ ~1 500) across **5 files** (≤ 15). Of those, 514 are the
acceptance target and ~230 are co-located unit tests (`metadata.rs`'s `mod startup_recovery`,
`cli.rs`'s `mod tests`). **Production** semantic lines added are ~135 against 81 removed — the
slice still shrinks the walk it hardens. No mechanical migration: `high_water_marks`'s signature
change has one callsite (`crates/server/src/lib.rs:146`) and it is in the patch.

## 9. For the commit body (required by the brief's *Scope*)

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

Also worth a line in the body: startup recovery is now total over **three** record classes
(damaged `inode:` values, a namespace larger than `SCAN_CAP`, a damaged `meta:next_inode`), it
**reaches a result** under sustained contention rather than retrying forever, and both the
`inode:` keys and the counter are read under the store's canonical decimal grammar — so
`inode:+18446744073709551615` cannot invent a mark and `+1` cannot lower a floor.

## 10. NEEDS-HUMAN / judgment at sign-off

No missing external dependency: base Rust toolchain only; cargo-deny, cargo-mutants and typos
are present and green on this host. Nothing is unverifiable for environmental reasons.

Two judgment calls, both stated rather than buried:

1. **The damaged-counter answer changed** (§3): recovery no longer repairs a `meta:next_inode`
   it cannot read; it attributes it and new-key PUTs fail closed until an operator repairs it.
   This is the round-5 carry-forward's instruction taken to its only provable conclusion, and it
   reverses a round-4 rejection recorded in this bundle. If you would rather keep the automatic
   repair, say so at sign-off — but the reseed can clobber a committed object's fragments
   through the cluster path's `(inode << 64) | seq` chunk ids, and the witness-based version
   that would fix that costs ~55 production lines and re-adds the two startup walks this slice
   removes.
2. **The one-clause docs deletion** (`docs/design/architecture/08-crosscutting-concepts.md:85`)
   — a clause this patch falsifies. Accepted at round 4's sign-off; still a clean one-line
   revert if you change your mind.
