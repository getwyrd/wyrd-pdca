# Build notes — #803 staged protection class in the shared reference set (662.1), iteration 4

Target: `getwyrd/wyrd` @ `main` = `78f9859` (the worktree's HEAD; `main` has not moved).
Every `path:line` below is on the patched tree (`main` + `patch.diff`) unless marked "base".

## What this iteration changes

**Only the test file.** The production half of the patch is byte-identical to iteration 3 —
proved, not asserted: iteration 3's `patch.diff` was applied with `git apply --index`, so the
git index in the worktree *is* iteration 3, and `git diff` (working tree vs index) at the end of
this round lists exactly one path:

```
 crates/custodian/tests/staged_protection.rs | 221 ++++++++++++++++++++++++++--
 1 file changed, 210 insertions(+), 11 deletions(-)
```

`crates/custodian/src/gc.rs`, `src/restore.rs`, `crates/server/src/cli.rs`,
`crates/dst/tests/custodian.rs` and both docs are untouched this round.

That is the right shape for the carry-forward's item: production **already** propagates a store
fault (`gc.rs:789-807`, three `?`s at `:791`, `:799`, `:802`, with the rule stated at `:786-788`).
What was missing was a test that would notice if it stopped. The adversary demonstrated that by
replacing all three `?`s with `.unwrap_or_default()` and watching iteration 3's 11 tests stay
green — a transient backend read failure silently becoming "this session stages nothing", which
is the exact data-loss shape the slice exists to prevent (`docs/principles.md` §5 C-1).

## The new leg — E(iii), line by line

**The fault, at the store seam** (`crates/custodian/tests/staged_protection.rs`):

- `StoreFault(Vec<u8>)` (`:123-140`) — a test-local `std::error::Error` whose `Display` names the
  prefix that could not be read. This is the answer a real `MetadataStore` gives when it cannot
  reach its backend; the whole leg is the difference between it and an empty answer.
- `Meta.fault` (`:120`), `Meta::fail_scan` (`:187`), `Meta::heal` (`:192`) — at most one armed
  fault, matched on **exact** prefix equality, so a leg faults one staged read and leaves every
  other read of the pass healthy.
- The check inside `scan` (`:246-250`) sits **after** the read log push, so the faulted read is
  recorded as issued. A leg reads that log back (`:1393-1401`) to prove the read it faults is one
  the pass actually makes — without it, a leg could "pass" because the pass never got that far.

**The passes, with their answers handed back** (`:459-525`): `gc_pass` / `restore_pass` used to
unwrap inside the helper, so a failed pass could only ever be a panic. They are now thin wrappers
over `try_gc_pass` (`:464`) and `try_restore_pass` (`:497`), which return the step's own
`Result` with the error **rendered** (`map_err(|err| err.to_string())`). The eleven existing
tests call the unwrapping wrappers and are unchanged in behaviour.

**The leg** (`:1307-1433`):

- `faulted_world()` (`:1318`) — one `Open` session, one committed `part:` record (chunk `0xE8`,
  fragment on server 0) and one in-flight owned `sidx:` entry (chunk `0xE9`, fragment on server
  1), both fragments on disk. The smallest world in which a failed staged read has something to
  lose.
- `assert_a_store_fault_fails_both_passes(prefix_of)` (`:1346`), in order:
  1. arm the fault on the one prefix (`:1350`);
  2. **restore**, over a store with no mark in it: must return `Err` (`:1354`), the error must
     name the faulted prefix (`:1358`), and neither staged fragment may be marked (`:1363`);
  3. mark both fragments past grace — the state in which an empty staged set costs the upload its
     bytes — and run **GC**: must return `Err` (`:1375`), the error must name the prefix
     (`:1379`), both fragments must still be on disk and their marks unconsumed (`:1384-1391`);
  4. the faulted prefix must appear in the store's read log (`:1393`);
  5. **the control**: `heal()` the store and re-run the same GC pass over the same store, marks
     and all — it answers `Satisfied` and still keeps both fragments (`:1405-1414`). So the two
     failures above are the store's and not the fixture's.
- Three `#[tokio::test]`s, one per read the builder issues: `mpu:` (`:1418`), a session's
  `sidx:<id>:` (`:1424`), a session's `part:<id>:` (`:1430`).
- Module doc updated (`:48-52`) and the double's doc (`:111`).

## Evidence

### The carry-forward's mutant — caught, three ways

All three `?`s replaced with `.unwrap_or_default()` (`gc.rs:791`, `:799`, `:802`), run through
`cargo test -p wyrd-custodian --test staged_protection`:

| Test set | Under the mutant |
|---|---|
| iteration 3's 11 tests | **11/11 green** (the gap the adversary found) |
| this iteration's 14 | **11 green, 3 failed** — exactly the three E(iii) legs |

The failures say what the mutant costs, in the report's own numbers:

```
e3_…_the_mpu_listing…      RestoreReport { stranded_marked: 2, … }   (both staged fragments marked stranded)
e3_…_a_sessions_sidx_range RestoreReport { stranded_marked: 1, … }
e3_…_a_sessions_part_range RestoreReport { stranded_marked: 1, … }
```

**Each read is pinned by exactly one leg**, so a regression names the read that broke. One site
mutated at a time, whole file run each time:

| Mutant | Result |
|---|---|
| `:791` only (`mpu:`) | 13 passed, 1 failed — `e3_…_the_mpu_listing…` |
| `:799` only (`sidx:<id>:`) | 13 passed, 1 failed — `e3_…_a_sessions_sidx_range…` |
| `:802` only (`part:<id>:`) | 13 passed, 1 failed — `e3_…_a_sessions_part_range…` |

**The GC half is not carried by the restore half.** In the runs above the restore assertion
(step 2) fires first, so I re-ran the three-site mutant with steps 2's assertions spliced out of a
scratch copy of the test, to see the GC assertions speak for themselves:

```
the GC pass certified an answer while the store was failing every read of mpu: — …: Changed
the GC pass certified an answer while the store was failing every read of sidx:e8e8…: — …: Changed
the GC pass certified an answer while the store was failing every read of part:e8e8…: — …: Changed
```

`Changed`, not `Blocked` — under the mutant GC does not merely mis-certify, it reclaims. The
scratch copy was restored from a byte copy taken before the probe and `git diff` re-checked
afterwards (production identical to the index, test file identical to the shipped one).

### Red → green (the project's C4-verify runner)

`PDCA_BUNDLE=results/issue_803 PDCA_BRIEF_BASE=origin/main ./engine/scripts/run-verify.sh`, on the
shipped `patch.diff`, against its own clean `origin/main` = `78f9859` checkout:

- `GREEN — cargo test -p wyrd-custodian --test staged_protection (fix applied)`: `14 passed`.
- `RED — … (production reverted, test kept)`: `0 passed; 14 failed`.
- `run-verify.sh: PASS — red without the fix, green with it (14 test(s) ran red).`

**14 tests ran red, all by assertion.** Every red-leg panic is at a property-assertion site — no
unwrap of a fixture, no panic inside a helper, no compile error:

| Site | Property | Tests |
|---|---|---|
| `:835` | a staged fragment was reclaimed | A |
| `:863` | a staged fragment was marked stranded | B |
| `:923` | the moved fragment was reclaimed | C1, C2 |
| `:1052` | a staged fragment was reclaimed | D |
| `:1096` | a stray was marked while a record was unreadable | E(i) ×3 |
| `:1231` | a held fragment was marked | E(ii) ×3 |
| `:1354` | the restore pass **succeeded** over a failing store | E(iii) ×3 |

On base the three E(iii) legs report `stranded_marked: 2` — `main` marks **both** staged
fragments stranded, which is the brief's defect reproduced from a third direction.

### Full gate

`./engine/xtask.sh ci` (= `cargo xtask ci` in the worktree) on the final state:
**`xtask ci: all checks passed`**, exit 0. Includes `staged_protection`: 14 passed; the madsim DST
custodian suite: 18 passed, including `gc_staged_build_under_concurrent_handoffs` and
`gc_staged_build_reaches_every_landing` (leg F); typos, docs lint/render, fmt, clippy
`--workspace --all-targets`, deny, machete, statics, deploy-guard.

## Choices, and what I ruled out

- **Three tests, not one loop over the three prefixes.** A single test would stop at the first
  faulted read and would not say *which* of the three regressed; the per-site table above only
  exists because each read has its own test. Cost of the split: 12 lines (three 4-line
  `#[tokio::test]` wrappers) over a one-test loop of ~6.
- **Assert the error's *content*, not just `is_err()`.** An `is_err()` assertion passes on any
  failure — a fenced-out custodian, a fixture typo — and would have kept passing if the error
  turned out to come from somewhere else entirely. `err.contains(&name)` ties the failure to the
  injected fault. Cost: the two `assert!`s (8 lines) plus `map_err(|err| err.to_string())` in the
  two helpers (2 lines).
- **Render the error rather than return it typed.** Returning `Result<Reconciled, ReconcileError>`
  from `try_gc_pass` would put `ReconcileError` in the import list and need a `match` (or a
  `downcast`) per leg to reach the boxed store error underneath — about 6 more lines and one more
  imported symbol. `ReconcileError`'s own `Display` already carries the store's message
  (`reconciliation.rs:96`: `"reconciliation store access: {e}"`), so the rendered form is strictly
  more informative at the assertion site and keeps the file's "names no symbol this slice adds"
  property with nothing extra imported.
- **Exact-prefix fault, not `starts_with`.** With `starts_with`, arming `sidx:` would fail *every*
  session's range in a multi-session world, and the leg would no longer be about one read. Same
  one line either way; exact equality names exactly the read under test.
- **No fault on `scan_page`.** All three staged reads are `scan` (`gc.rs:791`, `:799`, `:802`), as
  is the committed `inode:` scan (`gc.rs:557`); `scan_page` serves the orphan-ledger walk, which
  is #661's and has its own tests (merged in PR #802). Adding it would be 4 lines that cover none
  of this slice's reads.
- **No DST property for store faults.** Leg F's DST legs exist because a handoff has a *timing*
  dimension — the property is about *when* the batch lands relative to the builder's reads, which
  is what seeds explore. A store fault has no such dimension: the fault either fires on a read or
  it does not, and the three unit legs cover all three reads deterministically. A DST version
  would need a metadata-store fault wrapper the custodian DST harness does not have today, and
  would explore a state space of one.
- **No production change.** The human accepted fail-closed retention as-is at iteration 3's
  sign-off, and the store-fault behaviour production already has (`?`) is the correct one. This
  round adds the test that holds it there. (Had I "fixed" anything here, the red→green would have
  been measuring my own new code rather than the gap the adversary found.)
- **The control (`heal()` + re-run) is deliberate, not padding.** Without it a reviewer cannot
  tell "both passes failed *because* of the fault" from "this fixture cannot run a pass at all".
  It costs 8 lines and it is the only assertion in the leg that runs a *successful* pass.

## The three refutation questions

- **(a) Genuine red?** Yes, at both scales. Whole patch: `run-verify.sh` reverted production, kept
  the test, and got `0 passed; 14 failed`, every one at a property assertion (table above). This
  iteration's specific gap: the carry-forward's mutant is 11/11 green against iteration 3's tests
  and 3-red against these, with each of the three mutation sites caught by its own leg.
- **(b) Production path?** Yes. The legs call `wyrd_custodian::reconcile_step` (the fenced control
  point) and `wyrd_custodian::reconcile_after_restore` — the real `gc::reconcile` → real
  `referenced_fragments` → real `staged_fragments`. The fault is injected at the `MetadataStore`
  trait seam, which is precisely where a real backend failure surfaces to this code; nothing about
  the pass is stubbed or re-implemented. Only the stores are doubles.
- **(c) Fixture includes the fault?** Yes. The faulted prefix is one the pass actually reads —
  asserted from the double's own read log (`:1393`), not assumed — and the session, its `part:`
  record, its `sidx:` entry and both fragments are in the store and on disk while the fault fires.
  Nothing is curated out: the same fixture, healed, runs a clean pass at the end of every leg.

## Commit-readiness

- `cargo fmt --all -- --check` clean; `cargo clippy -p wyrd-custodian --all-targets -- -D warnings`
  clean; the full `cargo xtask ci` above ran fmt, clippy, typos and the docs gates over the
  workspace and passed.
- The target repo configures no commit hooks (no `core.hooksPath`, no `.git/hooks` beyond samples,
  no pre-commit config), so CI's fmt/clippy/typos steps are the commit bar. DCO sign-off belongs to
  the publish step.
- `patch.diff` is `git diff HEAD` in the worktree at `78f9859`, with the new test file staged as an
  addition, and `run-verify.sh` applied it cleanly to a fresh `origin/main` checkout.

## Size

`patch.diff` is 131,244 bytes (iteration 3: 122,388). The +8,856 bytes are all in
`staged_protection.rs` (+210/−11 lines). The human overrode the size backstop for this bundle at
iteration 3's sign-off ("this is round 3 — ignore the size count"); this round adds only the test
the same sign-off required.

## What I read beyond `brief.md`

The carry-forward cites line numbers that exist only in the previous patch, so I read
`iteration-v3/patch.diff` and `iteration-v3/build-notes.md`. In the worktree I read the production
code the leg drives — `gc.rs`'s `staged_fragments` and `referenced_fragments`, `reconciliation.rs`'s
`reconcile_step` and `ReconcileError`, `restore.rs`'s `reconcile_after_restore` and `mark_orphaned`
— plus the whole of the test file I am editing, and the target's `## Review rubric & protocol`
section, which the patch was self-reviewed against (the only class it touches is *test fidelity*,
and this delta strengthens it: the double now models the production seam's error semantics, not
just its success ones).

## External dependencies

`typos` and `docs-renderer` (the brief's list) were present and exercised by `cargo xtask ci`.
Nothing outside that list was needed, so there is no NEEDS-HUMAN external-dependency item.
