# Build notes — issue 661 / gc-orphan-ledger-paged-walk (Do, iteration 4)

Target: getwyrd/wyrd `main` @ `605b33a` (still the tip of `origin/main` when I checked, via
`git ls-remote`). A `path:line` marked **base** is on `605b33a`; one marked **patched** is in the
tree with `patch.diff` applied.

Files changed (same six as iteration 3): `crates/custodian/src/gc.rs`,
`crates/custodian/src/restore.rs`, the new `crates/custodian/tests/gc_ledger_walk.rs`,
`crates/custodian/tests/segmented_map_restore.rs`, `crates/dst/tests/custodian.rs`,
`docs/design/architecture/06-runtime-view.md`. No `Cargo.toml` change. No signature change to
`reconcile_step` / `reconcile_after_restore`, no new field on `GcContext`, any other context, or
`RestoreReport`.

## What this iteration changes (the carry-forward)

The sign-off asked for one thing: close the C5 gap. Iteration 3's mutation gate
(`iteration-v3/gate-logs/C5-mutants.log`) reported 47 mutants: **2 missed, 1 timeout**, 26
caught, 18 unviable. I rebuilt on iteration 3's patch (it applies cleanly on `605b33a`) and changed
only what closes that gap. I read the iteration-3 artifacts the carry-forward block points at
(`patch.diff`, `build-notes.md`, `SUMMARY.md`, `check-review.md`, the gate logs), because the
mutant positions it cites (`gc.rs:745`, `:795`) are line numbers in iteration 3's patched file,
and I read the harness's `scripts/mutants-in-diff` to re-run the gate exactly as Check does.

### The three mutants, and what now catches each

| Iteration-3 mutant | Why it survived | Now |
|---|---|---|
| `gc.rs:745:48` `>` → `>=` in `covers_mark_of` (patched now `:746`) | No test put a fragment's own mark exactly on the cursor a pass resumed from. With `>=`, the window claims to cover the one key its range `(cursor, last]` leaves out, finds no mark there (the previous pass read it), and the expired-lease arm reclaims a fragment whose mark is inside grace. A destructive bug, as the sign-off said. | Caught by the new `d1_a_mark_on_the_resume_cursor_outranks_an_expired_lease` (test `:869-959`): it fails at pass 2 with "the fragment was reclaimed on its expired lease while its own mark — the cursor this pass resumed from, which it did not read — is inside its grace window". |
| `gc.rs:795:59` `>` → `>=` in `ledger_page` (patched now `:806`) | Every test store kept the exclusive-cursor clause, so no page ever ended on its own cursor. | Caught by the new `guard_a_page_outside_the_scan_page_contract_is_refused_not_walked` (test `:1314-1383`): a store that repeats its cursor must be refused; under the mutant the GC pass accepts the page and the test fails with "the GC pass walked a page outside the contract instead of refusing it". |
| `gc.rs:694:36` `<=` → `>` in `OrphanWindow::read` — **TIMEOUT** | The line was `let whole = page.len() <= want;`, which only matters for a page longer than asked for. Under the mutant `whole` is never true, so the loop never ends at the end of a ledger shorter than one window, and the test binary hangs. No test can turn a production infinite loop into a caught result, only a timeout, and cargo-mutants exits 3 for a timeout (`scripts/mutants-in-diff` fails on any non-zero exit). | The line is gone. I rewrote the loop (patched `gc.rs:685-705`) and moved the over-long case into `ledger_page` as an error (patched `:797-804`). Details below. |

### Production change 1: the window's read loop (patched `gc.rs:685-705`)

Iteration 3 tolerated a page longer than the walk asked for: it cut the page at the budget
(`take(want)`) and used `whole` so it would not conclude "end of ledger" from a cut page. The
rewrite asks each page for exactly what is left of the budget, adds the page length to `read`, and
stops on one of two plain conditions: `next.is_none()` (end of the ledger, `through = None`) or
`read == ORPHAN_WINDOW` (budget spent, `through = last key`). For every store that keeps the page
bound, which is every in-tree backend (they resolve the bound through `page_limit`, base
`crates/traits/src/lib.rs:407-409`: "`items.len() <= min(limit, cap)` holds by construction"),
the behaviour is identical to iteration 3.

I checked every operator mutation cargo-mutants makes on the new loop for a hang, and the re-run
confirms none: `- → +` over-reads to the end of the ledger (caught by the per-pass bound),
`- → /` divides by zero, `+= → -=` underflows, `+= → *=` keeps `read` at 0 and reads to the end
(caught by the bound), `== → !=` stops after one page (caught by leg B's exact count).

Rejected: keep `take(want)` + `whole` and add a test with an over-long store. That does not help:
the `<=` → `>` mutant hangs in every test whose ledger ends inside a window, whatever else the
file tests, so it stays a timeout.

### Production change 2: `ledger_page` refuses a page longer than it asked for (patched `gc.rs:781-820`)

A page over `limit` is now an `Err` ("returned N entries for a limit of L — refused rather than
read past the walk's budget"), next to the existing refusal of a page that does not advance. The
`scan_page` contract already forbids such a page ("The page bound", base
`crates/traits/src/lib.rs:1400-1407`), and the rubric's protocol-input rule says oversize input
is an error, never silently accepted. Cutting it (iteration 3) was not silent data loss, but it
kept a branch no honest store reaches and that could only hang under mutation. Both GC's window
and restore's `marked_among` go through `ledger_page`, so both refuse. Cost: 8 lines.

### Test additions (`crates/custodian/tests/gc_ledger_walk.rs`)

- **Boundary regression** `d1_a_mark_on_the_resume_cursor_outranks_an_expired_lease`
  (`:869-959`), the one the sign-off asked for. Under `Reclaim`, the fragment's own mark (stamped
  `NOW`, inside grace) is seeded as the ledger's `B`-th key (`B - 1` filler keys before it, `B / 2`
  after, 98,304 keys in all, past the cap) and its chunk carries an expired lease. Each lap's first
  pass ends its window on the mark, so the persisted cursor is the mark itself; the second pass
  resumes from it without reading it. Four passes (two laps): the fragment, the mark bytes and the
  `pending:` entry all survive every pass. Fixture checks, from the double's own record: in two
  passes a window ended on the mark, and in two a pass resumed from it (the first page was asked
  for strictly after the mark's key) without receiving it. Then the grace window elapses and the
  fragment is reclaimed within one lap, the mark consumed and the entry retired, so the leg cannot
  pass on a walk that never reclaims.
- **Guard test** `guard_a_page_outside_the_scan_page_contract_is_refused_not_walked`
  (`:1314-1383`). The double gains a `PageFault` knob (`:172-184`, default `Honest`), applied only
  to `orphan:` listings (`:283-302`): `RepeatsCursor` starts a page AT its cursor with one key a
  page, so its second page is exactly the cursor key; `OverLong` answers one entry more than the
  caller's `limit`. For each, over a ledger of `B + 2` marks and one stray fragment, the GC pass and
  the post-restore pass must both return `Err` naming the refusal. The restore half matters most:
  `marked_among` walks to the end with no budget, so without the progress check a cursor-repeating
  store would hang it forever. The GC half comes first, so under the progress mutant the test fails
  there, before restore could hang.
- The tap's `Page` records the `after` each page was asked for (`:117-124`, helper
  `Tap::resumed_from`, `:165-169`). Module docs updated (`:11-45`).

### Mutation re-run (the C5 gate, reproduced)

`cargo mutants --in-diff patch.diff --no-shuffle` (cargo-mutants 27.1.0, output directed to
scratch), in the worktree, on the final `patch.diff` (the same result as a first run before I
renamed the guard test):

    Found 46 mutants to test
    ok       Unmutated baseline in 10s build + 1s test
    46 mutants tested in 69s: 28 caught, 18 unviable      [exit 0]

**0 missed, 0 timeouts.** All three `covers_mark_of:746` mutants (`==`, `<`, `>=`), all three at
`ledger_page:797` (the new over-long check) and all three at `ledger_page:806` are caught; the
per-mutant logs show `>=` at `:746` fails only the boundary test, and `>=` at `:806` fails only the
guard test. The 18 unviable are "replace the function body" mutants that do not compile under the
workspace's lints (unused parameters), the same 18 shapes as iteration 3.

### Considered and not changed this round

- **The advisory review's T2 note** (iteration 3 `check-review.md`: `marked_among` holds a `wanted`
  map and a `marked` set sized by the candidates). It was not in §6 or in the sign-off's delta, so I
  left it. My reading, for the human: the brief's invariant defines a pass's footprint as "the
  entries it holds, the writes in each commit", bounded by a constant "not by the ledger";
  `marked_among` holds one page of ledger entries at a time, plus a map over the candidates, and
  the candidates are a subset of the `on_disk` list the pass already held on the base (base
  `restore.rs:351-358`). Making the candidate side constant too would take a merge-join over
  candidates sorted by key spelling or a point `get` per candidate; I rejected the point `get` in
  iteration 3 on round trips (1 M stranded fragments ≈ 1 M sequential gets ≈ 17 min at 1 ms, against
  28 page reads for a 1.78 M ledger).
- The other §6 items the sign-off listed as tracker/scope/process questions: untouched.
- The oversize flag (patch now 117 KB) is waived by the human's instruction.

## The design, carried over from iteration 3 (unchanged; line numbers refreshed)

### 1. GC reads one bounded window of the `orphan:` ledger per pass (`gc.rs`)

- **Removed `orphan_leases`** (base `gc.rs:520-537`), the single `scan(ORPHAN_PREFIX)` that fails
  whole past `SCAN_CAP` (base `crates/traits/src/lib.rs:273-286`). Its two callers were GC (base
  `gc.rs:177`) and restore (base `restore.rs:308`); both are replaced.
- **`OrphanWindow`** (patched `gc.rs:643-752`; `read` at `:675`): loads the persisted cursor, then
  pages until it holds `ORPHAN_WINDOW` entries or the ledger ends, recording the key range it
  covered, `(after, through]`, with `through = None` meaning "to the end of the prefix". GC uses it
  at patched `gc.rs:242-250`.
- **`ORPHAN_WINDOW` (B) = `SCAN_CAP / 16` = 65,536** (patched `gc.rs:64-79`), derivation in the doc
  comment: a pass already holds up to `SCAN_CAP` `inode:` and `SCAN_CAP` `pending:` entries, so the
  window adds a sixteenth of one scan's heap bound; the ~1.78 M-mark retirement (`0016:1392-1398`)
  drains in 28 passes. A smaller B shortens each pass but multiplies the full-fleet
  `list_fragments` calls a drain costs (B = 16,384 would take 109 passes, 4× the listings).
- **Persisted continuation `gc:orphan-cursor`** (patched `gc.rs:103-115`; written by
  `record_resume_point`, `:714-732`). Value: the last key read (exclusive resume point), or empty
  for "start at the head" (an absent record means the same). Written only when it moves, so a
  ledger smaller than B, which is every pre-existing test, never writes it. It has to live in the
  store: `GcContext` may not gain a field (brief Scope), and a global is ruled out (ADR-0035). A
  stored value outside `orphan:` is treated as the head.
- **The cursor is written before the pass acts on its window** (patched `gc.rs:247-250`). Written
  last, a persistent fault on one fragment in a window would pin the walk there forever and stall
  GC for the rest of the ledger. Written first, a failed pass costs that window one lap of delay.
- **No wrap inside a pass.** A pass that reaches the end resets the cursor to the head and stops.
  Wrapping inside the pass would make the covered range a union of two intervals, which complicates
  the coverage test the expired-lease arm depends on, to save at most one pass per lap.

### 2. What a pass may conclude from its window (legs D, E)

- **The expired-lease arm fires only for a fragment the window shows has no mark** (patched
  `gc.rs:283-303`): no mark read, chunk expired, AND the window covered the key position its own
  mark would occupy (`covers_mark_of`, `:739-751`). A mark outside the window, including one
  exactly on the cursor, is unknown, not absent, and falls to the conservative arm. The safety gate
  (base `gc.rs:191`), the grace test (base `:196-203`) and the conservative arm (base `:207-211`)
  are unchanged; the lease arm (base `:204-206`) is the one that changed.
- **Why "covered and absent" is sound.** Every in-tree mark writer that dereferences — unlink (base
  `crates/core/src/metadata.rs:1894`), supersede (`:2003`, `:2076`), repoint (base
  `reconstruction.rs:944`), evacuation (base `rebalance.rs:544`) — writes the mark in the same
  commit that dereferences the fragment, and the reference set is read before the window. Restore
  marks without dereferencing, but never marks a chunk that holds a `pending:` entry (base
  `restore.rs:421-424`). Future pre-marking (#723) writes marks before a fragment exists, so that
  slice has to recheck this argument.
- **An unreadable mark value counts as a mark** (`ReadMark::Unreadable`, patched `gc.rs:631-641`):
  kept, never deleted, named on the audit seam by `emit_unreadable_mark` (`:970-983`), mirroring
  `emit_unreadable_pending` (base `gc.rs:600`). Decoding is unchanged (base `gc.rs:528-530`).
- **Only a fragment's own key is its mark** (`classify_ledger_entry`, patched `gc.rs:754-779`):
  `orphan_key(parse_orphan_key(key)) == key`. Anything else is skipped, never deleted, and named by
  `emit_malformed_orphan_key` (`:985-997`).
- **Only consumed keys are deleted.** The base also deleted the mark key after an expired-lease
  reclaim (base `gc.rs:216`), a key it never read. Now only the orphan arm deletes a key, the one it
  read and judged (patched `gc.rs:313-319`).

### 3. `pending:` entries retired only when nothing is left for them to account for (leg D(ii))

Patched `gc.rs:254-259`, `:320-342`: retire `pending:<c>` when the chunk is expired, this pass
reclaimed at least one of its fragments (either arm), and no unprotected fragment of it survived the
pass. Stricter than the base (base `gc.rs:217-229`) where it matters; it still leaves an expired
entry alone when its chunk has no fragment at all at the start of the pass, as the base does.

### 4. Bounded cleanup commits (leg B)

`Cleanup` (patched `gc.rs:859-896`) queues blind deletes and commits at `CLEANUP_BATCH` (W) = 1,000
(`:81-101`), plus the remainder at the end: `⌈n/W⌉` commits. W's doc comment gives the byte bound
(keys ≤ 73 bytes, so < 73 KB per commit, far inside 10 MB) and says plainly that the 5-second
operation half is not calibrated (0016's `B_ops`, `0016:640-643`). No conditional writes anywhere.

### 5. Restore's "already marked" sees the whole ledger, a page at a time (leg F)

`reconcile_after_restore` collects the candidates first (patched `restore.rs:372-416`, gate and
displaced check unchanged), then `gc::marked_among` (patched `gc.rs:822-857`) walks the whole ledger
in pages of `ORPHAN_WINDOW` and reports which candidates carry their own key, any value (patched
`restore.rs:418-430`). With no candidates it reads none of the ledger. `already_marked`'s doc says
"its own key, whatever its value holds" (patched `restore.rs:112-115`).

### 6. Existing tests adjusted, as the brief allows

- `crates/custodian/tests/segmented_map_restore.rs` (base `:642-656`): dropped the `orphan:` leg —
  restore no longer reads the ledger while a record is unreadable, because it may mark nothing
  then — and kept the `pending:` leg and the property.
- `crates/dst/tests/custodian.rs` restore campaign: the two `inode:` readings are now two hops
  apart instead of three. Retune only: a zero delay starts the writer at once (patched `:1976-1979`;
  madsim's `sleep(0)` finishes a tick late), and the `RESTORE_NEMESIS_SPAN` comment describes the
  two-hop gap (`:1786-1792`). Span and invariants unchanged.

### 7. Docs currency

`docs/design/architecture/06-runtime-view.md` §6.7 step 2 (base `:74`, new paragraph patched `:76`):
the windowed walk, the persisted cursor record and how it carries across a leader change, what a
pass may conclude, chunk-wide lease entries, bounded clean-up commits, restore's paged read. This
iteration adds no port, operation, flag or persisted field, so the paragraph is unchanged.

## Leader change (Do's call, per the brief)

**The walk survives a leader change.** The cursor lives in the metadata store, so a new leader
resumes where the old one stopped. A deposed leader finishing its pass can still write its cursor
after the new leader's. That is harmless: a cursor is only a place to read from, so at worst a
region is read again or waits one more lap. It is never evidence and never licenses a delete.

## The test file: `crates/custodian/tests/gc_ledger_walk.rs` (new)

Ten tests: legs A, B, C, D(i), D(i) on the cursor, D(ii), D(iii), E, F, and the guard test. The
double `LedgerMeta` is a `BTreeMap` store whose `scan` fails with `ScanCapExceeded` past
`CAP = 5,000` and whose `scan_page` is written directly over the map with `page_limit` /
`page_start` / `page_cursor`, pages capped at `CAP` (below B, not a divisor of it). Every GC pass
builds a fresh `GcContext` and goes through `reconcile_step`, and every `Walk::pass` is checked
(`assert_bounded`, `:531`) for: no `scan` reaching `orphan:`, at most B entries received, every
commit at most W writes, and delete-carrying commits = `⌈n/W⌉`. B and W are literals (65,536 and
1,000, `:81`, `:85`). It names only symbols present on `origin/main`. About 1 s for all ten.

## Refuting my own test (forced)

**(a) Genuine red? Yes.** Through the project's C4-verify script (`engine/scripts/run-verify.sh`,
pointed at a scratch checkout and branch so it could not touch the Check gate's `../wyrd-verify*`):

- GREEN, fix applied: `cargo test -p wyrd-custodian --test gc_ledger_walk` — 10 passed.
- RED, production reverted, test kept: **10 of 10 failed, each by the test's own assertion** —
  the `panic!` in `Walk::pass` (`:518`) for legs A-E and the boundary test, the one on
  `reconcile_after_restore`'s result for leg F (`:1280`), and the guard test's refusal-text
  assertion (`:1365`), which sees the base's `ScanCapExceeded` error instead of a refusal. No
  compile error on the red leg.
- Verdict: `run-verify.sh: PASS — red without the fix, green with it (10 test(s) ran red).`
- Beyond the base, each new test goes red under the exact mutant it exists for (the mutation
  re-run above): `>=` at `gc.rs:746` fails only the boundary test, `>=` at `gc.rs:806` fails only
  the guard test. Iteration 3's hand-mutation table (wrong paged designs, all caught) still holds.

**(b) Production path? Yes.** Every leg calls the production `reconcile_step` (with a `GcContext`,
through the fence) and/or `reconcile_after_restore`. The only doubles are the metadata store and
the D servers, as in `crates/custodian/tests/gc.rs`. Leases are written with the production
`metadata::put_pending`; `assert_seeding_is_mark_orphaned` (`:455`) pins seeded marks as
byte-identical to what `mark_orphaned` writes. The guard test's broken stores break only the
`orphan:` listing; the passes under test are the production ones.

**(c) Fixture includes the fault? Yes.** Every leg's ledger is past the lowered cap, and pages are
capped below B, so a pass really pages. The boundary test asserts, from the double's own record,
that the mark really was the last key of a window in two passes and the resume cursor of the next
pass in two — so the fixture contains the exact boundary the mutant needs, not a nearby one. The
guard test's stores really break the contract: the cursor-repeating store's second page is the
cursor key, and the over-long store answers `limit + 1` over a ledger long enough to do so. The
other legs' fixture checks are as in iteration 3 (mark ahead of and behind the window, laps
completed, positions in different windows, the other spelling more than B keys early, pre-marked
keys past index max(B, CAP)).

## DST (leg G) — unchanged from iteration 3

Property 12 in `crates/dst/tests/custodian.rs` (patched `:2175-2557`; no new DST file;
`#![cfg(madsim)]` kept): over `SimTikvMetadataStore::with_scan_cap(page_cap)` (seed picks 3-6),
three GC passes with fresh contexts walk 9 to ~35 actionable marks plus a referenced fragment with a
stale mark and two within-grace marks, while a concurrent task runs the production
`metadata::unlink` at a seed-chosen instant (0-32 ms), writing marks that are the ledger's first and
last keys; the unlinked chunks carry a stale expired lease. The coverage leg sweeps the landing over
0..=32 ms and requires a mid-walk landing with marks behind and ahead of the cursor.

- **Seed count:** `cargo xtask ci` → `run_dst` runs every `dst_campaign_test!` over
  `MADSIM_TEST_NUM = 50` seeds (base `xtask/src/main.rs:1573`, `:1607`): 50 seeds for the campaign
  leg, 50 seeded runs of the coverage leg (33 landing points each), plus the 8 committed regression
  seeds, which also replay property 12 (patched `:2684`).
- The ledger stays below B, as the brief says, so the DST exercises paging within a pass, not the
  cross-pass cursor; the in-process legs cover the cursor.

## Gates run here

- `cargo xtask ci` through the harness runner (`engine/xtask.sh ci`, in `$PDCA_WORKTREE`, on the
  final tree): **`xtask ci: all checks passed`**, exit 0 — fmt, clippy `-D warnings`, build,
  workspace tests (the new file's 10 tests among them), cargo-machete, cargo-deny, statics
  (ADR-0035), deploy-guard, and the madsim DST clippy + campaign, including
  `gc_orphan_walk_under_a_concurrent_unlink` and `gc_orphan_walk_reaches_the_mid_walk_landing`.
- C4-verify (`engine/scripts/run-verify.sh`, on the final `patch.diff`): PASS, 10 red / 10 green,
  as above.
- C5 (`cargo mutants --in-diff patch.diff --no-shuffle`): 46 mutants, 0 missed, 0 timeouts.
- Commit-readiness: `cargo fmt --all -- --check` clean; clippy `-D warnings` clean on
  `wyrd-custodian --tests`; `typos` (1.48.0) clean on all six touched files;
  `docs/publishing/tools/lint_docs.py` OK; `render_site.py --check` rendered 99 pages, link audit
  OK. The target has no pre-commit hook config of its own (no `.pre-commit-config.yaml` or
  `.githooks`); these are the checks its CI runs.
- Scratch: the verify run's git worktree and its local branch `pdca-verify-lb661` are removed
  (`git worktree remove`, `git branch -D`). Left for the harness to reclaim:
  `$PDCA_SCRATCH/pdca-builder-661-mutants{,-final}` (the two mutation runs' per-mutant logs) and
  `$PDCA_SCRATCH/pdca-builder-661-ci.log`.

## External dependencies

`typos` and `docs-renderer` (the brief's list) are installed. No unlisted dependency was needed.
