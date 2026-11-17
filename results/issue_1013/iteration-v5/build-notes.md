# Build notes — #1013 (multipart session terminal delete, 636.6b), iteration 5

Target: `getwyrd/wyrd` `main`, built on the stack base `b452551`
(`pdca-integration/main-r9dd123c9ed70` = `main` + #1007–#1011, #1027–#1030, #1061, #1062). All
`path:line` below are in the patched tree (base `b452551` + `patch.diff`).

## What this iteration is

The iteration-4 sign-off: "Keep iteration 4's design and patch … Fix only" two things. I applied
`iteration-v4/patch.diff` to the clean worktree (`git apply`, no fuzz) and changed only the owned
walk in `crates/custodian/src/retirement/sessions.rs`, its tests, and the docs those changes touch.
The terminal delete, the `mpuctl` decrement, the part cursor, the whole-fleet gate, the deployed
caller and its server test are byte-for-byte iteration 4's.

| Carry-forward item | What I did | Where |
|---|---|---|
| 1. The walk always reads the head of the range, so 16 held entries hide every later entry for good. Carry scan progress past held entries (resume from the last held key, wrap at the end), keep intra-entry progress and mark-before-delete. Fix the module doc. Regressions: 16 held + 1 trusted, `Aborting` on a short fleet and `Completed`/`Elapsed`. | A stored **scan position** per upload (`custodian:owned-scan:<id>`): where the next step's page starts. A step that marks and deletes nothing and holds something moves it past the held entries at the head of its page; once nothing is left after it, the walk is back at the head and the position is deleted. A bookmark naming an entry on another page is left alone unless an entry the walk can work on takes it. | `sessions.rs:60-75` (doc), `:172`, `:203` (key), `:577-594` (read, lap), `:619-625` (bookmark on another page), `:779-786`, `:810-830` (move), `:931-932` (terminal delete), `:1051-1072` (`move_scan`); docs `06-runtime-view.md:92`, `05-building-block-view.md:246`, `retirement.rs:123-126` |
| 2. Bookmark choice needs `covered > 0`, which never counts listed fragments, so an undecodable entry is never bookmarked and stays one fragment short behind two held entries. Count listed progress for scheduling, never as evidence. Regression: that three-entry case. | The bookmark now goes to the first entry left that the step walked **without a hold**, whatever it covered — at a count of zero if it names nothing countable. The next step walks it first with the whole read budget. The count is still only of named positions; listed fragments are re-read whole every step. | `sessions.rs:769-773`, doc `:40-58`; test `multipart_session_terminal.rs:2775` |

The out-of-scope adversary finding (marks left over positions that never landed, cf. #825) is not
touched, as the sign-off says. The two DST findings stay recorded-rejected in `review-rejected.md`
(anchors refreshed to this tree's lines); the brief and two sign-offs say "add no DST scenario".

## Item 1: the scan position

**The rule** (`sessions.rs:810-830`). After the step's entries are walked:

- `idle` = the step queued no mark and no delete.
- `passed` = how many entries at the head of the page, in range order, the step held.
- If `idle` and `passed > 0`, the position moves to the last of those held entries — provided an
  entry follows it. For a page held whole, a full page says only "more may follow"
  (`scan_page`'s `next` is `Some` on any full page, `crates/traits/src/lib.rs:562`), so one more
  one-row read says whether an entry does (`:820-825`). If none follows, the position is `None`:
  the head.
- The position is written only when it differs from the one the step read from (`:827`).

At the start of a step (`:577-594`): read the position; one that does not start with the upload's
`sidx:` range is audited (`unusable-owned-walk-position`) and read as the head; if the page after it
is empty the walk is back at the head and reads the first page instead; in both cases the stored
position is deleted there and then.

**Why "held entries at the head", not only "a page held whole".** The sign-off's wording is the
second. The first is the same rule on a whole page and also covers a case I found while tracing:
three entries held part-way at position 150 ahead of a trusted one. Re-reading two of them up to
their holds costs 302 reads, a step has 233, so on iteration 4's code the trusted entry is never
read, for good — the same defect class as the 16-entry one. With the head rule the walk passes the
two it held, the third and the trusted one become the page, and the trusted entry retires
(`d_a_trusted_entry_behind_three_held_part_way_retires`, `multipart_session_terminal.rs:2863`). The
cost against the whole-page rule is one `take_while` and one comparison (`sessions.rs:815`, `:820`).
The mutation that puts the whole-page rule back (`P-partial`) turns that leg red.

**Why it is safe.** The position only chooses which page a step reads. What a delete needs is
unchanged: every named position counted by the bookmark for that exact key or read as this walk's
own mark in this step, every listed fragment read so in this step, under `require(sidx == bytes
read)` (`sessions.rs:766`, `step.delete`). A stale or damaged position costs a lap, never a mark.

**Why it does not spend a step** (`sessions.rs:1051-1072`, doc `:71-75`). A step that moves the
position past held entries ends the visit (`:855-861`), and a session with more than a page of held
entries moves it every pass for as long as they are held. Charged to `SESSION_STEPS_PER_PASS` (16),
sixteen such sessions would spend every step of every pass and sessions listed after them would
never be visited. So it is a commit of its own, as the ledger walk's resume point is
(`retirement.rs:596-611`, `COMMITS_PER_PASS = STEPS_PER_PASS + 1` at `:280`). Bound: at most two per session a
pass — one delete when the walk is back at the head, one move past what is held there — because a
move ends the visit.

**Never outlives its session.** A move is put under `require(mpu:<id> == the record read)`
(`sessions.rs:1065-1067`), and the terminal delete's batch deletes the key (`:932`). So another
drainer's position either lands before the terminal delete and is removed by it, or after and is
refused. Both are tested with a wrapper that lands another writer's batch between two reads
(`d_no_scan_position_is_put_once_the_session_record_has_moved`, `:2744`;
`d_the_terminal_delete_removes_the_walks_own_records`, `:3162`).

**Intra-entry progress across pages** (`sessions.rs:619-625`, `:779-786`). The bookmark is one
record. When it names an entry outside the step's page (at or before the position, or after the
page's last key while more follows), it stays as it is unless an entry of this page that was walked
without a hold takes it. Three legs, the wide held entry being the first, the sixteenth (the very
key the position names) and the seventeenth of seventeen (`:3026`, `:3033`, `:3040`): the bookmark
still reads `(entry, 240)` after each of six lap passes, and after the repair the entry resumes
there (exactly 16 more marks).

### Alternatives ruled out (with the cost)

- **Move only past a page held whole** (the sign-off's literal wording). Two lines fewer
  (`sessions.rs:815`, `:820`). Leaves the three-held-part-way case above hidden for good. Not taken.
- **Carry the position in memory inside one pass, store nothing.** No new key, about 12 lines in
  `visit`. But a pass would then read a held session's whole range every pass: up to 2,528 entries
  at the shipped profile (`MAX_INFLIGHT_PARTS × MAX_PART_CHUNKS` = 16 × 158,
  `crates/core/src/multipart.rs:5843`) = 158 pages, each with up to 233 mark reads and one fleet
  listing. Capping the pages per pass brings the hidden-entry defect back at `cap × 16` entries.
- **Charge the position write to the step budget.** One line (`pass.steps_left -= 1`). Sixteen
  sessions with more than 16 held entries each would then starve every session listed after them.
- **Keep the position inside the bookmark record.** Saves the second key (about 12 lines: key
  function, terminal delete). Changes the bookmark's stored shape (`entry key + 4-byte count`),
  which the sign-off said to keep and tests assert literally; and a walk batch is already at the
  500-operation limit (`sessions.rs:163-164`), so the write would need its own commit anyway.
- **One count per entry instead of one bookmark** (a list in the value). Removes the residual
  below. About 30 lines and a stored-format change. Not taken: "do not redesign".

## Item 2: the bookmark rule

Iteration 4 (`sessions.rs:697` there): an entry gets the bookmark only if `covered > 0`, and
`covered` counts named positions. An undecodable entry names none, so it never got it.

Now (`sessions.rs:769-773`): of the entries the step leaves, the first walked without a hold takes
the bookmark at its named count, zero included; a held entry still needs a count above zero, and
still loses to any entry the walk can work on. The entry the bookmark names is walked first
(unchanged, `:637-644`), so it gets all 233 reads.

Trace of the sign-off's case (`d_an_undecodable_entry_listed_to_the_brim_behind_two_held_ones_retires`,
`:2775`): two entries held after one read each, then an undecodable entry listed with 232 fragments.
Pass 1 marks 231 and writes the bookmark `(entry, 0)` (asserted by literal key and value). Pass 2
walks it first, reads 231 own marks and writes the last. Pass 3 reads all 232 as its own and deletes
it. On iteration 4's code pass 2 onward reads 231 and stops, every pass.

Nothing about the listing is stored: the count is zero, and every step that reaches listed fragments
reads them all.

**Why wider than "count listed progress".** The literal fix (bookmark when named or listed progress
is above zero) needs a second counter threaded out of the listing branch, about 3 lines more than
this, and still leaves an entry the step's reads ran out on before reading anything of it without a
turn. "Walked without a hold" covers both. Its cost: when such an entry turns out to be held on its
next turn, the zero-count bookmark is written and then deleted, one extra small commit (seen and
asserted in the same test, `:2834-2842`).

## Tests added (13 legs; the file is now 40)

All through `reconcile_step` with the deployed context, over real redb, sessions made by the
production Create, Abort and Complete. `multipart_session_terminal.rs`:

| Leg | Line | What it pins |
|---|---|---|
| `d_a_trusted_entry_behind_a_page_held_for_a_short_fleet_retires` | 2718 | 16 wrong-length entries held `partial-fleet` + a trusted 17th. Pass 1 commits one batch: the position, literal key `custodian:owned-scan:<id>`, value the 16th entry's key, under the session-record precondition. Pass 2 marks and deletes the trusted entry, position deleted. Three more passes commit nothing (exactly one page: the one-row read says nothing follows). Held entries byte-identical, nothing of their chunks marked. Then the fleet is whole: all retire, the session ends. |
| `d_a_completed_sessions_trusted_entry_behind_a_page_of_held_ones_retires` | 2735 | The same for a `Completed` session with `Tombstone::Elapsed`, the 16 wrong-length and undecodable by turns. |
| `d_no_scan_position_is_put_once_the_session_record_has_moved` | 2744 | The put is refused once the session record is gone. |
| `d_an_undecodable_entry_listed_to_the_brim_behind_two_held_ones_retires` | 2775 | Item 2. |
| `d_a_trusted_entry_behind_three_held_part_way_retires` | 2863 | The position moves past a partly held page, to the second of three; the trusted entry retires. |
| three `…_survives_the_lap` legs | 3026–3040 | The bookmark of an entry on another page survives the lap. |
| `d_a_bookmark_naming_an_entry_that_is_gone_is_dropped` | 3049 | A bookmark naming a missing entry inside the page's span is dropped, not kept as "another page". |
| `d_a_scan_position_naming_nothing_of_its_range_is_read_as_the_head` | 3088 | Audited, deleted before any mark, walk proceeds. |
| `d_a_scan_position_left_over_an_empty_range_is_deleted` | 3131 | `Completed` + `Retain`: the position goes even though no terminal delete follows. |
| `d_the_terminal_delete_removes_the_walks_own_records` | 3162 | Position and bookmark written between the walk and the terminal batch are removed with the session. |
| `d_a_page_whose_first_entry_is_not_held_stays` | 3193 | An idle step with a hold but a workable entry at the head writes no position. |

Helper changes: a `meanwhile` hook on the recording store (`:329-331`, `:384-400`) that lands
another writer's batch right after the first read of a key; `walk_position` (`:275`) spells the
literal key.

## Gates run this iteration

- **`./engine/xtask.sh ci`** (the project runner) on the final tree: **rc 0, "xtask ci: all checks
  passed"**. `typos`, `lint_docs.py` OK, `render_site.py --check` 101 pages, link audit OK, the three
  guards, fmt, clippy, build, workspace tests (233 `test result: ok`, none failed; the new file
  **40 passed**, `custodian_gc` **11 passed**), cargo-machete, cargo-deny (all four runs),
  conformance vectors, statics, deploy-guard, `wyrd-dst` clippy and tests under `--cfg madsim`.
  Extracts: `do-evidence/ci5-steps.log`, `do-evidence/ci5-tail.log`.
- **The same runner, red:** iteration 4's `sessions.rs` put back (and the one export line it does
  not have): **rc 1**. The only failed check is `cargo test --workspace`, in the new file: "29
  passed; 11 failed". Restored from backups; `cmp` byte-identical to the tree the green run tested.
  Extract: `do-evidence/red-iter4-ci5.log`.
- **Commit hooks** (target's `.pre-commit-config.yaml` on `origin/main`): pass. `pre-commit` itself
  is not installed here, so, as in iterations 3 and 4, I ran each hook's own command over the patch
  staged in a throwaway index (25 paths, the 25 of `patch.diff`): `forbid-machine-local-files`
  regex — no match; no `.gitleaksignore` in checkout or index; `gitleaks git --pre-commit --redact
  --staged --verbose --ignore-gitleaks-allow` with the guard's config (`[extend] useDefault =
  true`), gitleaks 8.30.1 — "no leaks found", rc 0; the staged-blobs hook's loop — 25 scanned, none
  failed. Control: a fake `ghp_` token through the same command reports "leaks found: 1". Log:
  `do-evidence/hook-iter5.log`.
- `patch.diff` applies to the stack base (`git apply --cached --check` over an index read from
  `b452551`). It still does not apply to bare `origin/main` (#1061/#1062 unmerged), so C4 diff
  coverage stays unmeasurable; the sign-off says that is not a builder defect.
- Size: production code added by the whole patch 82.5 KB (`sessions.rs` 49.4 KB, `retirement.rs`
  15.3 KB, `marks.rs` 13.8 KB, the rest under 2 KB each), under the 150 KB cap. Tests 161 KB, docs
  17.7 KB. This iteration added about 6.6 KB to `sessions.rs`, half of it doc comment.

## Demonstrated reds

Scripted `cargo test -p wyrd-custodian --test multipart_session_terminal` under a 900 s timeout per
run; each mutation is an exact one-match replacement in production code, every file restored from a
backup copy afterwards and checked byte-identical. Every one is red on a named leg.

**The brief's six**, re-run on this tree (`do-evidence/reds5-carried.log`, mutations in
`do-evidence/mutate.py`):

| Brief item | Mutation | Result |
|---|---|---|
| `sidx:`-empty gate removed from the terminal delete | M1 | 4 legs red |
| `Completed` terminal-deleted without the tombstone input | M2 | 5 legs red |
| `mpuctl` decrement not under its precondition / outside the batch | M3a / M3b | 1 / 3 legs red |
| untrusted record deleted without marking by chunk id | M4 | 14 legs red |
| retirement by chunk id on a short fleet | M5 | 8 legs red |
| E2 marked only from the listing | M6 | 3 legs red |

Also red as before: R1–R12, M0 (session leg not dispatched: 38 of 40 fail), the bookmark mutations
of iteration 4 re-spelled for this code (`do-evidence/mutate5-carried.py`: iteration 3's rule, no
reorder, `!moved` dropped, range filter dropped, kept-tier bounds, held/unheld swapped, kept before
advanced), and the two deployed-input mutations against the server test (S-fleet, S-tomb).

**This iteration's 27** (`do-evidence/mutate5.py`, results `do-evidence/reds5.log`):

| Mutation | Red leg(s) |
|---|---|
| position never written | 9 legs |
| a position with entries after it dropped anyway / the lap never taken | the two 16-held legs and more |
| the head delete on every step / never | `d_sessions_past_one_page…` (more than 16 commits) and 5 more / the unusable and empty-range legs |
| a position outside the range used as it is | `d_a_scan_position_naming_nothing_of_its_range…` |
| position moved on a step that marked or deleted | `d_a_held_entry_keeps_its_place_in_the_walk`, the three-held leg |
| position never moved past held entries | 7 legs |
| a page whose first entry is not held passed too | `d_a_page_whose_first_entry_is_not_held_stays` |
| a held page always taken to have a follower / a full page taken as a follower / the one-row read inverted | the two 16-held legs (the quiet tail) and more |
| a partly held page not passed | the three-held leg |
| written when unchanged | 9 legs |
| put without the session precondition | `d_no_scan_position_is_put_once…`, the two 16-held legs |
| terminal delete keeps the position / the bookmark | `d_the_terminal_delete_removes_the_walks_own_records` |
| unusable position not audited | `d_a_scan_position_naming_nothing_of_its_range…` |
| "another page" test: `<=`→`<`, `<=`→`==`, `>`→`>=`, `>`→`<`, `&&`→`\|\|` | one lap leg each, or the gone-bookmark leg |
| a bookmark on another page dropped / overridden by a held entry here | the three lap legs |
| iteration 4's `covered > 0` rule put back (item 2) | `d_an_undecodable_entry_listed_to_the_brim…` |
| a held entry with nothing counted keeps the bookmark | 3 legs |

Three of the 27 did not compile on the first run (an unused binding under the workspace's denied
warnings), which proves nothing; I fixed their replacement text and re-ran them, and the log shows
the re-runs.

On the real base the new test file does not compile (no `Tombstone`, no context fields), so
C4-verify reports UNVERIFIABLE, as the brief pre-declares.

## Refuting my own test

- **(a) Genuine red?** Yes. Through the project runner, iteration 4's walk code turns 11 of the 13
  new legs red and nothing else. The two that stay green on it are controls for rules this
  iteration added (`d_a_bookmark_naming_an_entry_that_is_gone_is_dropped`,
  `d_a_page_whose_first_entry_is_not_held_stays`); each goes red under its own mutation above. One
  thing to know when reading the red log: on iteration 4's code the two 16-held legs fail at their
  first assertion (no position written in pass 1), before they reach "the trusted entry retires
  within 3 passes". The stall itself is what `P-nopass` shows on the new code, and the three-held
  leg fails on iteration 4's code at exactly "the trusted entry retires within 8 passes".
- **(b) Production path?** Yes. Every leg calls `wyrd_custodian::reconcile_step` with the context
  the deployed loop builds, over `RedbMetadataStore::in_memory()`. The keys are asserted as literal
  strings, not built with the crate's functions.
- **(c) Fixture includes the fault?** Yes. The 16 entries are really held (wrong-length placements
  on a fleet missing server 5; a `Completed` session's untrusted entries) and really fill one page;
  the 17th is really behind them in key order. The undecodable entry really has 232 listed
  fragments and two entries really cost a read each ahead of it (the test asserts the first step
  marks exactly `MARKS_PER_STEP - 2`). The three part-way holds are real marks of no known shape at
  position 150.

## Rubric self-review (AGENTS.md *Review rubric & protocol*)

- **One clock per lifecycle**: no new clock read.
- **Narrow seams / dependency direction**: only `MetadataStore` calls already used in this file.
- **Validation boundaries**: the position is custodian bookkeeping; one naming nothing of its range
  is audited and read as absent, never used.
- **No shared mutable global state**: none added; the statics gate passed.
- **`#![forbid(unsafe_code)]`**: no new crate.
- **Docs currency**: a new stored key, so `06-runtime-view.md:92` describes it and
  `05-building-block-view.md:246` names it; module docs `sessions.rs:19-94`, `retirement.rs:123-126`.
- **Absent or unsupported entries**: an unusable position is an explicit audit event; a held entry
  is still named every time its page is read.
- **Transactions**: no live transaction; each write is one `commit(batch)`. `move_scan` propagates
  a store fault (`?`), as the ledger walk's resume-point write does (`retirement.rs:609-610`); a
  refused put is ignored on purpose (the session moved).
- **Await discipline**: the new awaits are `MetadataStore` reads and commits, bounded by the
  backend as every other custodian read is.
- **Test fidelity**: see (b). No DST scenario, per the brief.

## Residuals (for the human)

1. **A session with more than 16 held entries writes its position once a pass** for as long as they
   are held (outside the step budget, at most two small commits a pass). With exactly 16 or fewer
   it writes nothing.
2. **Held entries are named once a lap, not every pass**, when a session has more than a page of
   entries: a pass that reads a page with no held entry is not `Blocked`. A lap takes one pass per
   page.
3. **Several entries held deep inside wide placements still contend for the one bookmark.** Two
   entries each held past position 231, or three whose hold depths sum past about 230 (on one page
   or on different pages), make the bookmark alternate between them: one small charged commit a
   pass for that session while the holds stand. This was already so in iteration 4. Nothing is
   hidden by it now (the three-held leg). Sixteen such sessions listed first would use up the 16
   steps and delay later sessions; that needs triple damage in each of sixteen uploads. A count per
   entry removes it and is a stored-format change, so I left it for you.
4. **Production reach (b), the cost**: unchanged from iteration 4 for listings
   (`retirement.rs:158-169`). The walk adds per step one read of the position, and at most one
   extra page read (the lap) or one one-row read (a page held whole).

## Scratch

Everything throwaway is under `$PDCA_SCRATCH/pdca-builder-1013-iter5` (logs, file backups,
throwaway index files, iteration 4's `sessions.rs` for the red run; no build cache). It sits inside
the harness's per-issue scratch root, which the harness reclaims, so I left it rather than run
`rm`. The worktree holds exactly the patch, uncommitted.
