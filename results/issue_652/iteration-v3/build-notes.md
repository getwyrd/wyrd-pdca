# Build notes — issue 652 / startup-recovery-total-and-bounded (iteration 3)

*Withheld from the reviewer; written for the human at sign-off.*

Worktree: `/home/eddie/wyrd/wyrd.pdca-wt-l1` at `d50f0ca` (= `origin/main`, the brief's target
base). All line citations below are **post-patch** lines in that worktree unless marked
"pre-fix".

---

## 1. What the carry-forward asked for, and what changed because of it

Round 2's patch was **not rejected on substance**: T4 (3× codex rubric passes) came back
`0 blocking`, and C4-verify was PASS. Two gates were red:

| Carry-forward gate | Cause | What I did |
|---|---|---|
| **C4-ci** — `cargo deny --all-features --config deny-all-features.toml check advisories` exit 2 | **Environmental, not the patch.** cargo-deny 0.20.0 moved `--config` into the ROOT; the host still had **0.19.9** after the home migration, which rejects the root form at argument-parse time — before any tree content is read. `pdca.toml:716-736` already records this and registers the probe. | Verified the host is now on **cargo-deny 0.20.2** (`cargo deny --version`) and the registered doctor probe `cargo deny --config /nonexistent/deny.toml check --help` exits **0**. Then ran the gate itself: `./engine/xtask.sh ci` → **`xtask ci: all checks passed`** (twice, before and after the final reflow). Nothing in the patch had to change for this; I checked rather than assumed. |
| **C5-mutants** — 3 missed mutants, all `crates/core/src/metadata.rs:2167:24` (`replace > with ==` / `<` / `>=`) | The `value.len() > MAX_VALUE_BYTES` ceiling check lived **inside the walk's closure**, and its only observable effect is *which* attribution an operator sees. The acceptance target asserts those labels — but `cargo mutants` defaults to `--test-workspace=false`, i.e. **only the mutated package's tests run**, so a `wyrd-server` integration test can never kill a `wyrd-core` mutant. | Extracted the decision into a pure unit, `read_inode_value` (`metadata.rs:2062-2099`), and added co-located `wyrd-core` tests (`mod startup_recovery`, `metadata.rs:3557-3728`) that stand on the boundary: **at** `MAX_VALUE_BYTES` → `Undecodable`, **one past** → `OverCeiling`, small damaged → `Undecodable`, healthy → `Readable`. Re-ran the gate: **9 mutants, 4 caught, 5 unviable, 0 missed** (was 3 missed). |

The approach itself (page the walk, attribute damage, delete the dead chunk-id half) is the one
Plan settled and the reviewers already passed; it is *not* re-submitted unchanged — the
mutation-binding extraction plus the core-level tests are the delta, and they are exactly what
the red gate asked for.

## 2. The change, file by file

**`crates/core/src/metadata.rs`**

* `RECOVERY_AUDIT` (`:2033-2042`) — the audit target damaged rows are attributed on, named like
  `wyrd.read.audit` (`read.rs:488`) and `wyrd.custodian.gc.audit` (`gc.rs:524`) so one filter
  selects the repair queue. The rubric's *absent-or-unsupported-entries* class forbids a silent
  skip; this is the tree's existing answer shape.
* `RECOVERY_PAGE = 128` (`:2044-2060`) — same width as the resolver's own `SEGMENT_PAGE_LIMIT`
  (`:2287`, 128) over rows of the same class. The doc states what a page bounds (**rows**, and
  this walk's residency: it retains one `u64`) and what it does not (**bytes**, which the seam
  owns — `crates/traits/src/lib.rs:995-999`, tracked as #674), matching the position
  `read_group_range` already takes one screen below (`:2252-2259`).
* `InodeValueReading` + `read_inode_value` (`:2062-2099`) — the ceiling-then-decode reading of a
  row's value, as a **pure unit**. The ceiling is `>` (a value *at* `MAX_VALUE_BYTES` is the
  largest a conforming writer may store, so it is decoded), mirroring
  `read_group_range`'s `value.len() > MAX_VALUE_BYTES` at `:2503`.
* `for_each_page` (`:2101-2135`) — salvaged from `sources/salvage.diff:4559-4591` with its
  rationale, cursor loop mirroring the merged #634 peer (`crates/traits/src/lib.rs:1044-1057`,
  `:1086-1087`).
* `high_water_marks` (`:2137-2231`) — now `-> Result<InodeId>`. Walks `inode:` in bounded pages;
  takes the mark from the **key before** the value is looked at; attributes an unparsable key, an
  over-ceiling value and an undecodable value, and keeps going. The `pending:` and `orphan:`
  walks are gone with the chunk-id mark (two fewer unbounded startup `scan`s), and
  `parse_pending_chunk_key` with them (it had no other caller).
* Standing test `high_water_marks_refuses_a_segmented_root_rather_than_re_mint_its_chunk_ids`
  **deleted** (`:3534-3553` carries the reasoning in place, and it goes in the commit body too —
  see §6): its hazard needs a minter allocating below `2^64`, and `fdd34f1` (#487, 2026-07-08)
  removed the last one. Its live half is superseded by the totality requirement and is bound in
  both new test layers.
* `mod startup_recovery` (`:3557-3728`) — the walk's own units: the ceiling boundary, totality
  over damage (mark from keys, 70 over a store whose rows 50/60/`not-a-number` are unreadable),
  and paging over a `scan`-refusing double with 258 rows.

**`crates/server/src/lib.rs`** — `recover()` (`:133-136`) drops the discarded second tuple
element; its doc says recovery does not refuse over store contents and why the saturating step is
safe. Two doc references to the removed chunk-id mark corrected (`:236-238`, `:249-252`).

**`crates/server/src/cli.rs`** — `alloc_inode` fails **closed** at an exhausted id space
(`:1670-1676`) instead of `id + 1`. This is caused by *this* patch: before it, a damaged
`inode:18446744073709551615` record made `recover()` `Err`; after it, that key legitimately seeds
the floor at `u64::MAX`, and `id + 1` would panic in a checked build or, in **release**, roll the
persisted counter to 0 and re-mint every live id — the exact "floor an allocator trusts turned
into re-mint" the brief's invariant forbids. Fixing the cause (refuse the allocation) rather than
guarding the symptom (clamp the floor, which would under-approximate) is the smaller restoration
of the invariant.

**`docs/design/architecture/08-crosscutting-concepts.md:85`** — **one clause deleted**, nothing
added: the sentence listed "the id recovery that must not re-mint them" among the chunk-map
consumers that fail closed on an unresolvable shape. After this patch id recovery reads **no**
chunk map at all, so that clause is simply false. The brief puts "any docs paragraph" out of
scope, and I have deliberately stayed inside that: this is truth-maintenance of a clause **this
patch falsifies** (a 1-line, `-1/+1`, zero-semantic-line edit), not the living-architecture
paragraph #648/#649-#651/#653 own. Round 2 instead *added* a new sentence about startup recovery;
I dropped that — writing new docs prose here would be the scope violation.

## 3. Alternatives considered and rejected (with costs, not adjectives)

* **Wire the chunk-id floor to a caller instead of deleting it.** Settled at Plan and accepted by
  the maintainer; also unbuildable without inventing a consumer — both minters are `>= 2^64`
  (`cli.rs:1735-1742`) or `>= 2^127` (`lib.rs:249-252`). Cost of the rejected path is the #647
  apparatus the brief names: ~282 semantic lines of `RecoveredIds`/`ClassIds`/byte-scavenging,
  carrying its own silently-low-floor defect. Recorded in `review-rejected.md`.
* **Leave the ceiling check inline in the closure and kill the mutants from the server test.**
  Impossible, not merely costly: `cargo mutants` runs only the mutated package's tests by default
  (`cargo mutants --help`, `--test-workspace … If false, only the tests in the mutated package are
  run`), and the mutants are in `wyrd-core`. Any binding test has to live in `wyrd-core`.
* **Drop the over-ceiling check altogether** (fewer branches, no mutants to kill: −9 production
  lines). Rejected: it re-earns the round-1 memory finding that `review-rejected.md` answers as
  *partly fixed*, and it would make this walk the only paged reader in the file that decodes a
  value the tightest backend would have refused — `read_group_range:2503` does exactly this check
  one screen below.
* **Clamp the recovered floor below `u64::MAX` so `alloc_inode` never sees the ceiling** (≈2 lines
  instead of 8). Rejected: that is a *quiet under-approximation* of the floor — the brief's
  invariant names it explicitly ("never zero", "≥ every id whose bytes still exist"). The
  allocator would then re-mint the id the damaged record already holds.
* **A `MetadataStore` fake in the core tests instead of in-memory redb.** Rejected: the paging
  contract is a property of an implementation; a fake that pages the way the walk expects asserts
  nothing. The double only overrides `scan` (to refuse) and forwards `scan_page`/`get`/`commit` to
  a real `RedbMetadataStore`.

## 4. Forced self-refutation (the three questions)

**(a) Genuine red?** Yes — proven by the project's own runner, not by reasoning.
`PDCA_BUNDLE=… ./engine/scripts/run-verify.sh` applies `patch.diff` to a clean `../wyrd-verify`
worktree off `origin/main`, then **reverts the production files and keeps the test**:

```
run-verify.sh: GREEN — cargo test -p wyrd-server --test gateway_recover_totality (fix applied)
  test result: ok. 3 passed
run-verify.sh: RED — … (production reverted, test kept)
  recover_is_total_over_damaged_inode_records_and_attributes_each … FAILED
    recover() must be Ok(()) over a store holding records it cannot read …: Error("expected ident", line: 1, column: 2)
  recover_is_total_over_a_store_whose_scan_refuses_and_pages_the_whole_namespace … FAILED
    …: ScanCapExceeded { cap: 1048576, prefix: [105,110,111,100,101,58] }
  an_exhausted_inode_space_fails_… … FAILED
  test result: FAILED. 0 passed; 3 failed
run-verify.sh: PASS — red without the fix, green with it.
```

All three are **assertion** failures through the signature-stable `Gateway::recover()`, not
compile errors — which is why the target is at the `wyrd-server` level and never names
`high_water_marks` (whose signature this patch changes). The co-located core tests are red on the
base for the harder reason that the symbols do not exist there; `C4-ci` covers them, and the
discriminator stays the single added `*/tests/*.rs` (`run-verify.sh --classify` →
`ADDED_TEST crates/server/tests/gateway_recover_totality.rs`).

**(b) Production path?** Yes. The acceptance target composes the real
`RedbMetadataStore` + `FsChunkStore` + `MemCoordination` gateway and calls the production
`Gateway::recover()`, `put_object`, `get_object`; the floor it asserts is read back from the real
persisted `meta:next_inode` key. The core tests call the production `high_water_marks` and
`read_inode_value` over an in-memory **redb** store. Nothing is re-implemented; the only double is
`ScanRefusingStore`, which overrides exactly one method (`scan`) and forwards the rest to redb —
that is the *fault injection*, not a stand-in for the unit under test.

**(c) Fixture includes the fault?** Yes.
* Criterion 1's store holds the healthy object **and** the three unreadable rows at once
  (`inode:50` undecodable, `inode:60` past the ceiling, `inode:not-a-number`), with the damaged
  ids **above** the healthy one — so the asserted floor (`61`) can only have come from a damaged
  row's key. A fixture that curated the damage out would assert `2`.
* Criterion 2 asserts the double really refuses *before* the scenario runs
  (`MetadataStore::scan(&sanity, b"inode:")` must be `Err`), holds 258 rows across >1 page, and
  puts the numerically largest id on the last page while the byte-lexicographically last key
  (`inode:99`) is deliberately small — a one-page walk answers 227, a last-row-wins walk answers
  100, and only the correct walk answers 251.
* Mutation-checked as well: `scripts/mutants-in-diff` → 9 mutants, **4 caught, 5 unviable, 0
  missed**.

## 5. Gate evidence run locally (this iteration)

| Gate | Command | Result |
|---|---|---|
| C4-ci | `PDCA_WORKTREE=… ./engine/xtask.sh ci` | **pass** — `xtask ci: all checks passed` (fmt, clippy `-D warnings`, build, full `cargo test`, statics, DST, conformance vectors, cargo-deny **all three legs**, cargo-machete, typos) |
| C4-verify | `PDCA_BUNDLE=… ./engine/scripts/run-verify.sh` | **pass** — red without the fix, green with it |
| C5-mutants | `PDCA_BUNDLE=… scripts/mutants-in-diff` | **pass** — 4 caught, 5 unviable, **0 missed** |

Commit-readiness: `cargo fmt --all` is clean and `cargo xtask ci` (which the repo's own hooks and
CI run) is green on the final tree, so the publish commit will not be rejected by the target's
hooks.

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
> end-to-end by `crates/server/tests/gateway_recover_totality.rs`.

## 7. Budget

502 semantic added lines (≤ ~1,500), 5 files (≤ 15). Of those, 311 are the acceptance target and
~65 the co-located core tests; **production** semantic lines added are ~126 against 57 removed —
the slice shrinks the walk it hardens, as the brief predicted.

## 8. NEEDS-HUMAN

None from this build. The round-2 cargo-deny item is **answered**: the host now carries
cargo-deny 0.20.2, the registered `[[doctor.checks]]` probe (`pdca.toml:731-736`) exits 0, and a
full local `cargo xtask ci` passes end to end, including the two deny legs that failed last round.
