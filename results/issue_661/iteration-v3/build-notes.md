# Build notes — issue 661 / gc-orphan-ledger-paged-walk (Do, re-plan of 2026-09-13)

Target: getwyrd/wyrd `main` @ `605b33a` (still the tip of `origin/main` when I verified). A
`path:line` marked **base** is on `605b33a`; one marked **patched** is in the tree with
`patch.diff` applied.

Files changed: `crates/custodian/src/gc.rs`, `crates/custodian/src/restore.rs`, the new
`crates/custodian/tests/gc_ledger_walk.rs`, `crates/custodian/tests/segmented_map_restore.rs`,
`crates/dst/tests/custodian.rs`, `docs/design/architecture/06-runtime-view.md`. No `Cargo.toml`
change. No signature change to `reconcile_step` / `reconcile_after_restore`, no new field on
`GcContext`, any other context, or `RestoreReport`.

## What changed, and why

### 1. GC reads one bounded window of the `orphan:` ledger per pass (`gc.rs`)

- **Removed `orphan_leases`** (base `gc.rs:520-537`), the single `scan(ORPHAN_PREFIX)` that fails
  whole past `SCAN_CAP` (base `crates/traits/src/lib.rs:273-286`). Its two callers were GC (base
  `gc.rs:177`) and restore (base `restore.rs:308`); both are replaced.
- **`OrphanWindow`** (patched `gc.rs:643-751`; `read` at `:675`): each pass loads the persisted
  cursor, then calls `scan_page` asking for the *remaining* budget (`ORPHAN_WINDOW - read`) until it
  holds `ORPHAN_WINDOW` entries or a terminal page ends the ledger. It records the key range it
  covered, `(after, through]`, with `through = None` meaning "to the end of the prefix". GC uses it
  at patched `gc.rs:242-250`.
- **`ORPHAN_WINDOW` (B) = `SCAN_CAP / 16` = 65,536** (patched `gc.rs:64-79`), derivation in the doc
  comment: a pass already holds up to `SCAN_CAP` `inode:` and `SCAN_CAP` `pending:` entries, so the
  window adds a sixteenth of one scan's heap bound; the ~1.78 M-mark retirement (`0016:1392-1398`)
  drains in 28 passes. I took the brief's ceiling rather than something smaller. A smaller B makes
  each pass shorter (fewer sequential `delete_fragment` calls), but multiplies the full-fleet
  `list_fragments` passes a drain costs: B = 16,384 would take 109 passes for the same ledger, 4×
  the fleet listings. The test file runs in about 1 s even at 65,536, so test cost did not push me
  lower.
- **Persisted continuation `gc:orphan-cursor`** (patched `gc.rs:103-115`; written by
  `OrphanWindow::record_resume_point`, patched `gc.rs:720-731`). The value is the last key read
  (exclusive resume point), or empty for "start at the head" (an absent record means the same).
  It is written only when it moves, so a ledger smaller than B — every pre-existing test — never
  writes it. It has to live in the store: `GcContext` may not gain a field (brief Scope), and a
  global is ruled out (ADR-0035). Blind put; a stored value outside `orphan:` is treated as the head.
- **The cursor is written before the pass acts on its window** (patched `gc.rs:247-250`). I chose
  this over writing it last. Written last, a *persistent* fault on one fragment in a window (a D
  server that keeps failing one `delete_fragment`) would pin the walk to that window forever and
  stall GC for the rest of the ledger — the self-sealing shape this issue removes. Written first, a
  failed pass costs that window one lap of delay; its marks all stay in the ledger.
- **No wrap inside a pass.** A pass that reaches the end resets the cursor to the head and stops;
  the next pass starts from the head. Wrapping inside the pass would make the covered range a union
  of two intervals (`(c0, end] ∪ [head, x]`), which complicates the coverage test the expired-lease
  arm depends on, to save at most one pass per lap — which the brief's `⌈…/B⌉ + 1` bounds allow.
- **`ledger_page` refuses a page that does not advance** (patched `gc.rs:780-809`): an empty page
  that still carries a cursor, or a page whose last key is not past `after`. The trait promises
  progress (clauses 2-3), but a walk that trusted it would loop forever on a store that broke it
  (rubric: bounded awaits; protocol input never silently accepted).

### 2. What a pass may conclude from its window (legs D, E)

- **The expired-lease arm fires only for a fragment the window shows has no mark** (patched
  `gc.rs:283-303`): the window read no mark for it, its chunk is expired, AND the window covered
  the key position its own mark would occupy (`covers_mark_of`, patched `gc.rs:738-750`). A mark
  outside the window is unknown, not absent, and falls to the conservative arm. The safety gate
  (base `gc.rs:191`), the grace test (base `:196-203`) and the conservative arm (base `:207-211`)
  are unchanged; the lease arm (base `:204-206`) is the one that changed.
- **Why "covered and absent" is sound even though the covering page was read a moment ago.** Every
  in-tree mark writer that dereferences — unlink (base `crates/core/src/metadata.rs:1894`),
  supersede (`:2003`, `:2076`), repoint (base `reconstruction.rs:944`), evacuation (base
  `rebalance.rs:544`) — writes the mark in the same commit that dereferences the fragment, and the
  reference set is read before the window. Restore marks without dereferencing, but never marks a
  chunk that holds a `pending:` entry (base `restore.rs:421-424`). This is in the arm's comment.
  Future pre-marking (#723) writes marks *before* a fragment exists, so that slice has to recheck
  this argument.
- **An unreadable mark value counts as a mark** (`ReadMark::Unreadable`, patched `gc.rs:631-641`):
  kept on the conservative arm, never deleted, and named on the audit seam through its own callsite
  `emit_unreadable_mark` (action `unreadable-orphan-mark`, patched `gc.rs:959-972`), mirroring
  `emit_unreadable_pending` (base `gc.rs:600`). Decoding is unchanged: `from_utf8` + `u64` parse,
  as base `gc.rs:528-530`.
- **Only a fragment's own key is its mark** (`classify_ledger_entry`, patched `gc.rs:753-778`): a
  key counts only if `orphan_key(parse_orphan_key(key)) == key`. Anything else — does not parse, or
  a spelling like `orphan:5:01:0` — is skipped, never deleted, and named through a separate
  callsite, `emit_malformed_orphan_key` (action `malformed-orphan-key`, patched `gc.rs:973-986`).
  I also chose not to let such a key *protect* its position: no writer writes it, so it neither
  licenses nor blocks anything; it is only surfaced. (The brief requires only that it never
  licenses.)
- **Only consumed keys are deleted.** The base also deleted `orphan_key(dserver, frag)` after an
  expired-lease reclaim (base `gc.rs:216`) — a key it never read, which is exactly how the base
  deletes an unreadable mark. Now only the orphan arm deletes a key: the one it read and judged
  (patched `gc.rs:313-319`).

### 3. `pending:` entries retired only when nothing is left for them to account for (leg D(ii))

An entry is chunk-wide evidence, and with windows one pass can reclaim some of a chunk's fragments
while others wait for another window. The rule (patched `gc.rs:254-259`, `:320-342`): retire
`pending:<c>` when the chunk is expired, this pass reclaimed at least one of its fragments (either
arm), and no fragment of it that the reference set does not protect survived the pass. Compared
with the base (base `gc.rs:217-229`, "retire if any fragment went on the lease"):

- stricter where it matters: never leaves an unprotected survivor without its entry;
- also retires the entry when the chunk's *last* fragment went on the orphan arm (the base left
  such entries forever);
- still leaves an expired entry alone when its chunk has no fragment at all at the start of the
  pass, as the base does. Retiring those too would clean a pre-existing leak, but it changes
  behaviour for chunks the pass never touched, so I kept it out. One consequence to name: if a
  chunk's remaining fragment vanishes some other way (disk loss) between two windows, its entry now
  stays, where the base would have dropped it in its single pass. That is a small metadata record,
  not bytes.

### 4. Bounded cleanup commits (leg B)

`Cleanup` (patched `gc.rs:848-885`) queues blind deletes and commits at `CLEANUP_BATCH` (W) = 1,000
(patched `gc.rs:81-101`), plus the remainder at the end: `⌈n/W⌉` commits, never one sized by the
pass. W's doc comment gives the byte bound (keys ≤ 73 bytes, so < 73 KB per commit, far inside
10 MB) and says plainly that the 5-second operation half is not calibrated (0016's `B_ops`,
`0016:640-643`). No conditional writes anywhere: writes keep the base's shape.

### 5. Restore's "already marked" sees the whole ledger, a page at a time (leg F)

`reconcile_after_restore` now collects the candidates first (gate and displaced check unchanged,
patched `restore.rs:372-416`), then asks `gc::marked_among` (patched `gc.rs:811-846`) which of them
carry their own key — any value, readable or not — by walking the **whole** ledger in pages of
`ORPHAN_WINDOW`, holding one page plus the candidates' keys (patched `restore.rs:418-430`). Then
already / pending / mark run exactly as before. With no candidates (for example while a hole in the
reference set withholds every mark) it reads none of the ledger. `RestoreReport::already_marked`'s
doc now says "its own key, whatever its value holds" (patched `restore.rs:112-115`).

Rejected: a point `get(orphan_key(..))` per candidate. It is simpler (about 3 lines at base
`restore.rs:413`) and reads exactly the key the pass might overwrite, but costs one sequential round
trip per stranded fragment: a restore that strands 1 M fragments makes 1 M `get`s (~17 min at
1 ms), against ⌈ledger / 65,536⌉ page reads for the walk (28 for a 1.78 M ledger). The walk holds
one page of the ledger; the candidate set is at most the `on_disk` list the pass already holds
(base `restore.rs:351-358`).

### 6. Existing tests adjusted, as the brief allows

- `crates/custodian/tests/segmented_map_restore.rs` (base `:642-656`, patched `:646`): dropped the
  `orphan:` leg — restore no longer reads the ledger while a record is unreadable, because it may
  mark nothing then — and kept the `pending:` leg and the property. Doc comment updated.
- `crates/dst/tests/custodian.rs` restore campaign: the pass's two `inode:` readings are now two
  hops apart (`pending:` between them) instead of three. I traced simulated times: madsim's
  `sleep(0)` does not finish inside the tick it is called in (the writer started 1 ms after being
  spawned), so a zero-delay writer could only tie with the second reading, and lost the tie. Retune:
  a zero delay starts the writer at once, without sleeping (patched `:1977`), and the
  `RESTORE_NEMESIS_SPAN` comment describes the two-hop gap (patched `:1784-1793`). The span (6) and
  every invariant are unchanged; the coverage leg passes again.

### 7. Docs currency

`docs/design/architecture/06-runtime-view.md` §6.7 step 2 (base `:74`, new paragraph patched
`:76`): the windowed walk, the persisted cursor record and how it carries across a leader change,
what a pass may conclude, chunk-wide lease entries, bounded clean-up commits, restore's paged read.

## Leader change (Do's call, per the brief)

**The walk survives a leader change.** The cursor lives in the metadata store, so a new leader
resumes where the old one stopped. A deposed leader finishing its pass can still write its cursor
after the new leader's. That is harmless: a cursor is only a place to read from, so at worst a
region is read again or waits one more lap. It is never evidence and never licenses a delete.

## The test: `crates/custodian/tests/gc_ledger_walk.rs` (new)

Eight tests: legs A, B, C, D1, D2, D3, E, F. The double `LedgerMeta` is a `BTreeMap` store whose
`scan` fails with `ScanCapExceeded` past `CAP = 5,000` and whose `scan_page` is written directly
over the map with `page_limit` / `page_start` / `page_cursor`, pages capped at `CAP` (below B, and
not a divisor of it). Every pass builds a fresh `GcContext` and goes through `reconcile_step`, and
every pass is checked (`assert_bounded`, `:492`) for: no `scan` reaching `orphan:`, at most B
entries received, every commit at most W writes, and delete-carrying commits = `⌈n/W⌉`. B and W are
literals (65,536 and 1,000, `:76`, `:80`). It names only symbols present on `origin/main`: it
compiles and runs on the base (the red below). About 1 s for all eight in a debug build.

## Refuting my own test (forced)

**(a) Genuine red? Yes.** Through the project's C4-verify script (`engine/scripts/run-verify.sh`,
lane-scoped to a scratch worktree so it could not touch the Check gate's `../wyrd-verify`):

- GREEN, fix applied: `cargo test -p wyrd-custodian --test gc_ledger_walk` — 8 passed.
- RED, production reverted, test kept: **8 of 8 failed, each by the test's own assertion on the
  pass's `Result`** — the `panic!` in `Walk::pass` (`gc_ledger_walk.rs:474`) for legs A-E, and the
  one on `reconcile_after_restore`'s result (`:1144`) for leg F. Each message carries the base's
  error: `metadata scan exceeded the interim per-listing cap of 5000 keys for prefix "orphan:"`.
  No compile error on the red leg.
- Verdict: `run-verify.sh: PASS — red without the fix, green with it (8 test(s) ran red).`

Beyond the base's red, I checked that the legs bite on *wrong paged designs* (where rounds 1 and 2
were caught), by mutating my own production code one change at a time:

| Mutation of my fix | Went red |
|---|---|
| expired lease fires without the window covering the mark (`covers_mark_of` ignored) | D1, D2, D3 |
| expired lease beats a *stamped* mark still inside grace | DST property 12, both legs |
| unreadable mark treated as no mark (lease fires) | D3 |
| differently spelled key accepted as the mark | E |
| `pending:` entry retired chunk-wide after the first reclaim | D2 |
| window reads half its budget | B, C, D2, D3 |
| window stops after one page | A, B, C, D1, D2, D3, E |
| cursor never advances (restart at the head every pass) | C, D1, D2, D3, E |
| cursor never wraps (stays at the end) | C, D1, D3, E |
| cleanup batch flushed one write late (`>` for `>=`) | A, B, C |
| restore reads only the first page of the ledger | F |

One mutation was equivalent rather than missed: "loop while `read < B - 1`" still reads exactly B,
because the last page is sized from `B - read`. A real under-read (B / 2) is caught.

**(b) Production path? Yes.** The legs call the production `reconcile_step` (with a `GcContext`,
through the fence) and `reconcile_after_restore`. The only doubles are the metadata store and the D
servers, as in `crates/custodian/tests/gc.rs`. Leases are written with the production
`metadata::put_pending`, and a fixture check (`assert_seeding_is_mark_orphaned`, `:411`) pins the
seeded marks as byte-identical to what `mark_orphaned` writes. The double's `scan_page` is not the
testkit helper, which pages over `scan` and would inherit the cap.

**(c) Fixture includes the fault? Yes.** Every leg's ledger is past the lowered cap (the fault this
issue is about), and pages are capped below B, so a pass really pages. D1/D3 put the mark outside
the window being read in most passes; the test asserts it was *ahead* of the window in some passes
and *behind* it in others, and that D3 read the mark in two separate laps. D2 asserts, from the
recorded page spans, that the leased fragments' positions fell in different passes' windows. E
asserts the other spelling opens the ledger more than B keys before the mark. F asserts its
pre-marked keys sit past index max(B, CAP) and that one of them is the ledger's last key. In the
DST, the coverage leg asserts the concurrent unlink really lands between two pages with marks behind
and ahead of the walk's cursor.

## DST (leg G)

Property 12 is appended to `crates/dst/tests/custodian.rs` (patched `:2175-2560`; no new DST file;
`#![cfg(madsim)]` kept). Over `SimTikvMetadataStore::with_scan_cap(page_cap)` (the seed picks 3-6),
three GC passes with fresh contexts walk a ledger of 9 to ~35 actionable marks, plus a referenced
fragment with a stale mark and two within-grace marks, while a concurrent task runs the production
`metadata::unlink` at a seed-chosen instant (0-32 ms), writing marks that are the ledger's first and
last keys. The unlinked object's chunks also carry a stale expired lease. Asserted: referenced and
within-grace fragments survive, the unlink's marks survive, and every initially actionable fragment
is reclaimed. The coverage leg sweeps the landing over 0..=32 ms at page cap 3 and requires at
least one mid-walk landing with marks behind and ahead.

- **Seed count:** `cargo xtask ci` → `run_dst` runs every `dst_campaign_test!` over
  `MADSIM_TEST_NUM = 50` seeds (base `xtask/src/main.rs:1573`, `:1607`): 50 seeds for the campaign
  leg, 50 seeded runs of the coverage leg (33 landing points each), plus the 8 committed regression
  seeds, which now also replay property 12 (patched `:2684`).
- The ledger stays below B, as the brief says it must, so the DST exercises paging within a pass,
  not the cross-pass cursor. The in-process legs cover the cursor.

## Gates run here

- `cargo xtask ci` through the harness runner (`engine/xtask.sh ci`, in `$PDCA_WORKTREE`):
  **`xtask ci: all checks passed`**, exit 0 — fmt, clippy `-D warnings`, build, workspace tests,
  cargo-machete, cargo-deny (three runs), statics (ADR-0035), deploy-guard, and the madsim DST
  clippy + campaign (16 DST tests, including both new ones).
- C4-verify (`engine/scripts/run-verify.sh`): PASS, as above.
- Commit-readiness: `cargo fmt --all -- --check` clean; `typos` clean on every touched file;
  `docs/publishing/tools/lint_docs.py` OK; `render_site.py --check` rendered 99 pages, link audit
  OK. The target has no pre-commit hook config of its own (no `.pre-commit-config.yaml` or
  `.githooks`); these are the checks its CI runs.

## External dependencies

`typos` and `docs-renderer` (the brief's list) are both installed and were used as above. No
unlisted dependency was needed.
