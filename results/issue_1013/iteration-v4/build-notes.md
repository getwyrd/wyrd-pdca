# Build notes — #1013 (multipart session terminal delete, 636.6b), iteration 4

Target: `getwyrd/wyrd` `main`, built on the stack base `b452551`
(`pdca-integration/main-r9dd123c9ed70` = `main` + #1007–#1011, #1027–#1030, #1061, #1062). All
`path:line` below are in the patched tree (base `b452551` + `patch.diff`).

## What this iteration is

The iteration-3 sign-off: "Keep iteration 3's design and patch … Fix only" three things. I applied
`iteration-v3/patch.diff` to the clean worktree (`git apply`, no fuzz) and changed only what the
three items need, plus the docs those changes touch.

| Carry-forward item | What I did | Where |
|---|---|---|
| 1. A held entry claims `first_left` with count 0, so a later wide entry's progress is never bookmarked (stalls at 232-233 of 256 while any earlier entry is held). Bookmark the first entry that actually advanced. Regression: held first entry + RS(128,128) second, `Aborting` and `Completed`/`Elapsed`. | The bookmark now goes to the first entry left that the step walked **without a hold** and covered a position of; a held entry with a count is only the fallback. The entry the bookmark names is walked **first** in each step. Three regression legs (two the sign-off named, one for the reorder), plus one for the fallback. | `crates/custodian/src/retirement/sessions.rs:562-572` (reorder), `:605-611`, `:638-651`, `:653`, `:697-704`, `:707-711` (choice); module doc `:38-54`; `docs/design/architecture/06-runtime-view.md:92`; tests `crates/custodian/tests/multipart_session_terminal.rs:2153` (helper), `:2261`, `:2273`, `:2282`, `:2388` |
| 2. Guard the deployed caller's new inputs (`custodian.rs:577` `operator_fleet_size`, `:580` `Tombstone::Retain`) with a server test through `run_reconstruction_until`, fleet short from startup. Mutating either input must turn it red. | New test in `crates/server/tests/custodian_gc.rs`, the file that already pins the same whole-fleet gate for GC. Both mutations turn it red, each on its own assertion. | `crates/server/tests/custodian_gc.rs:1380` (test), `:1343` (driver), `:183`/`:203`/`:234` (probe counter) |
| 3. Test lost-bookmark recovery (the `!moved` mutant at `sessions.rs:704` survived) and assert the literal stored key `custodian:owned-walk:<id>`. | New leg with every position already own-marked and no bookmark; it asserts the one bookmark write by literal key and exact value. The existing bookmark leg now uses the literal key too, and `owned_walk_key` is no longer imported by the test. A second new leg covers a bookmark naming another session's entry. | `crates/custodian/tests/multipart_session_terminal.rs:2295`, `:2348`, `:258-268` (`walk_bookmark`, `bookmark_value`), `:1899` |

The T4 batch review's three blocking findings: the BUG at `sessions.rs:678` is item 1 above (fixed);
the two TEST-GAP findings (no seeded Tier-0 DST for the session leg) are recorded-rejected in
`review-rejected.md`: the brief's scope says "add no DST scenario" and the iteration-3 sign-off
repeats it ("Add no DST scenario (#660 owns it)").

## Item 1: what was wrong, and the rule now

**The defect** (iteration 3, `sessions.rs:619/626/673-679` there): the bookmark went to the *first
entry the step did not delete*, and a held entry claimed that slot (with its count, 0 when the
bookmark did not name it). A wide entry behind it could never be bookmarked, so each step re-read it
from position 0, spent the 233-read budget on its own marks, and never got further. Reproduced:
with iteration 3's rule put back, all three new held-ahead legs fail ("the wide entry retires
within 3 passes").

**The rule now** (`sessions.rs:605-611`, `:697-711`):

1. *Advanced tier*: the first entry left, in the order walked, that the step walked without a hold
   and covered at least one named position of.
2. *Kept tier* (fallback): the first held entry left whose count is above zero — pre-walk held with
   the bookmark's count (`:640-642`), or held part-way through its positions (`:698`, a hold pushed
   while walking it).
3. `bookmark = advanced.or(kept)`; absent if neither.

A held entry therefore keeps its place while nothing else moves, but never takes the bookmark from
an entry that advanced.

**Why the reorder too** (`sessions.rs:562-572`: the bookmarked entry is rotated to the front of the
page). The two tiers alone fix the sign-off's case (an entry held *before* anything of it is read
costs no reads). They do not fix an entry held **part-way**, past one step's reads from its first
position. Trace, without the reorder: e1 = RS(128,128) with an unreadable mark at position 240;
e2 = RS(128,128).

- pass 1, step 1: e1 from 0 marks 233, budget spent → bookmark (e1, 233).
- pass 1, step 2: e1 from 233 marks 7, held at 240 (kept tier); e2 from 0 marks 225 (advanced) →
  bookmark (e2, 225).
- pass 2, step 1: e1 from 0 (the bookmark now names e2) re-reads 233 of its own marks, budget
  spent, not held → advanced → bookmark (e1, 233). e2 is never reached.
- pass 2, step 2: back to (e2, 225). Every pass repeats this: two commits a pass, forever, and e2
  never retires.

With e2 walked first in the step after it is bookmarked, it gets the whole budget and finishes in
pass 2. Reproduced: with the reorder removed, `d_a_wide_entry_behind_one_held_past_one_step_retires`
fails ("retires within 3 passes"); nothing else does.

**Why it converges.** The bookmarked entry is walked first with the full 233 reads. If it is not
held, the step reads further into it (its count rises), marks fragments the listing shows of it, or
deletes it. If it is held, the bookmark moves only to an entry that was walked without a hold and
covered positions, and that entry is walked first next step. So the bookmark changes only when its
entry is deleted, its count rises, or it is held while another entry advances; none of these can
cycle without the store changing under it (a fleet flipping between whole and short, a mark
repaired).

**Safety is unchanged.** Deletion still needs every named position covered before the step (by the
bookmark's count) or read as this walk's own mark in it, and for a by-chunk entry every listed
fragment read as own in this step (`sessions.rs:694`). The order entries are walked in does not
enter that test. The bookmark is still written only in the batch whose marks it counts, under
`require`/`require_absent` of the bytes read (`:720-728`).

### Alternatives ruled out (with the cost)

- **The sign-off's rule taken literally** ("the first entry that actually advanced (covered > 0)",
  where a held entry's count is its `from`): a pre-walk-held entry that the bookmark names has
  `from > 0`, so it would keep the bookmark and the wide entry behind it would stall again for as
  long as the hold stands (a short fleet after progress). The kept tier is that rule with the
  held case moved behind the advanced one. Same size: the two `Option`s replace one.
- **Held entries never keep the bookmark** (drop the kept tier, about 4 lines fewer): an entry held
  part-way past one step's reads then livelocks on its own: held at 240 → no candidate → bookmark
  deleted (a commit); next step from 0, 233 own reads → bookmark (e, 233) (a commit); next step held
  again → deleted. Two commits a pass for ever, and the session's pass never goes quiet.
- **One bookmark per entry** (a list of `(key, count)` in the value, or a key per entry): removes
  the need to choose and to reorder, but changes the persisted shape the sign-off said to keep, and
  needs per-entry cleanup in the delete batches. Sketch: `encode_bookmark` over a list (+15 lines),
  the resume lookup per entry (+5), every `step.delete` also removing its entry's count (+6), the
  terminal delete unchanged; about 30 lines and a persisted-format change, against 25 lines here.
  Not taken: "keep iteration 3's design … do not redesign".

### Residual (for the human)

- **Sixteen held entries at the head of one session's range** (`OWNED_PER_STEP`) fill the walk's
  page, so entries after them are never reached; their fragments stay protected by GC's staged
  class (kept, never lost), and the session is held anyway (it cannot end while a held entry
  stands). Unchanged from iteration 3, and not one of the three items; reaching past held entries
  would need a second cursor (where the page starts), which is a design change.

## Item 2: the deployed caller's inputs

`deployed_retirement_leg_waits_for_the_whole_operator_fleet_and_keeps_completed_tombstones`
(`crates/server/tests/custodian_gc.rs:1380`) drives the real `CustodianService::run_reconstruction_until`
built as `wyrd custodian` builds it (`with_logging_to`, log sink captured), over the file's in-memory
metadata store and D-server doubles:

- An **aborted** upload (production Create, then production Abort) with an untrusted staging entry
  (RS(4,2) placed on five servers, fragments on servers 0–5) and a trusted residue entry beside it.
- A **completed** upload (production Create and Complete) with trusted residue of a crashed part.
- The operator wired six servers; server 5 was left out at startup, so the loop gets five and
  `operator_fleet_size = 6`. Nothing is "unreachable": only the operator count says the fleet is
  short.

After six passes: the untrusted entry is byte-identical, nothing of its chunk is marked, all six of
its fragments stay, and the log sink has a `retire-session-held` line typed `partial-fleet` naming
the entry; both trusted residues were walked away and marked (so the leg ran over both uploads — the
test is not passing because nothing ran); the completed upload's record stays; the ledger still
counts two. Then six passes with all six servers: the untrusted entry retires (all six fragments
marked, server 5's included), the aborted upload ends, the count drops by one, and the completed
upload's record still stays.

**Pass count, not wall time.** The existing tests in that file stop the loop after 60 ms of wall
time. This one needs a fixed number of passes, so the D-server double now counts its reachability
probes (`custodian_gc.rs:183`, `:203`, `:234`): the loop probes every configured server once a
pass (`live_reconstruction_view`), so the shutdown future waits for server 0's count to rise by six
(`drive_passes`, `:1343`), inside a 120 s `tokio::time::timeout`.

**Why that file, not a new one.** It already pins the same whole-fleet gate for GC with the same
startup-partial shape (`deployed_role_defers_gc_when_the_operator_fleet_is_startup_partial`), and its
doubles are what the loop needs. A second new `tests/*.rs` file beside the brief's named one could
also confuse C4-verify, which treats an added test file as the per-fix discriminator.

## Item 3: the lost bookmark

- `d_a_lost_bookmark_costs_reads_not_progress` (`multipart_session_terminal.rs:2295`): every one of
  a wide entry's 256 positions already carries the walk's own mark; no bookmark. The first step
  re-reads 233 marks, marks nothing, deletes nothing, and commits only the bookmark — the `!moved`
  path. The test asserts exactly one committed put at the literal key `custodian:owned-walk:<id>`
  with value = entry key + `233u32` big-endian, before the entry's delete; no mark written; the
  session ends; the bookmark is gone. With `!moved` removed it fails ("ends within 3 passes").
- `d_a_bookmark_naming_no_entry_of_its_session_is_no_evidence` (`:2348`): the bookmark names another
  session's entry; the first pass audits `unusable-owned-walk-bookmark`, the walk reads from
  position 0 and every position is marked before the delete. With the range filter removed it fails
  (no audit event).
- The literal key: `walk_bookmark` (`:258`) spells `custodian:owned-walk:{id}`; the existing
  `d_owned_entries_past_one_steps_reads_resume_from_the_walks_bookmark` uses it (`:1899`).

Also added for the kept tier: `d_a_held_entry_keeps_its_place_in_the_walk` (`:2388`) — a
pre-walk-held entry with a count keeps the bookmark on a short-fleet pass that commits nothing, and
the walk resumes from it when the fleet is whole. And one assertion in
`d_the_terminal_delete_waits_for_an_empty_owned_range` (`:1252`): an entry held at its first
position leaves no bookmark.

## Gates run this iteration

- **`./engine/xtask.sh ci`** (the project runner, `pdca.toml` `[gates] runner`; it runs
  `cargo xtask ci` in `$PDCA_WORKTREE`) on the final tree: **rc 0, "xtask ci: all checks passed"**.
  Prose gates ran (`typos`, `lint_docs.py` OK, `render_site.py --check` 101 pages, link audit OK),
  then the gitlink/unsafe/blackbox guards, `cargo fmt --all -- --check`, clippy (workspace, all
  targets), build, the workspace test suite (233 `test result: ok`, none failed; the new file **27
  passed**, `custodian_gc` **11 passed**), `cargo-machete`, `cargo deny` (advisories, bans,
  licenses, sources; and the all-features advisories run), conformance vectors, the ADR-0035
  statics gate, deploy-guard, and `wyrd-dst` under `--cfg madsim` (clippy + tests). Extracts:
  `do-evidence/ci4-steps.log`, `do-evidence/ci4-tail.log`.
- **Two earlier runs, superseded.** The first stopped at clippy's `type_complexity` on my `wide()`
  test helper (fixed with a `Record` type alias, `multipart_session_terminal.rs:243-248`). The
  second failed one test in `wyrd-gateway-s3`,
  `tests::a_bodyless_response_is_recorded_complete_not_aborted` (empty log capture). Nothing that
  test links is touched by this patch: `git diff --stat b452551 -- crates/gateway-s3` is empty, and
  its only `wyrd-*` dependencies are `wyrd-gateway-core` and `wyrd-traits` (whose only `wyrd-*`
  dependency is `wyrd-traits`), neither touched. Run alone it
  passed 3 of 3, it passed in the final run, and it passed in iteration 1 and 3's gate logs:
  a flake (it reads a tracing capture, the callsite-interest race class `custodian_day_one.rs`
  documents). Because `cargo test --workspace` stops at the first failing binary, that run did not
  reach the server tests, so I re-ran the whole gate rather than count it. Extract:
  `do-evidence/ci4-flake-gateway-s3.log`.
- **Commit hooks** (target's `.pre-commit-config.yaml` on `origin/main` `57aefe0`): pass. As in
  iteration 3, `pre-commit` itself is not installed, so I ran each hook's own command, binary, args
  and config over the patch staged in a throwaway index in scratch (25 paths, the same 25 as
  `patch.diff`): `forbid-machine-local-files` (its `files:` regex) — no match; no `.gitleaksignore`
  in checkout or index; `gitleaks git --pre-commit --redact --staged --verbose
  --ignore-gitleaks-allow` with `GITLEAKS_CONFIG` = `[extend] useDefault = true` (gitleaks v8.30.1,
  the pinned rev) — "no leaks found", rc 0; `gitleaks-staged-blobs` (`git cat-file blob ":0:$f" |
  gitleaks stdin …` per path) — 25 scanned, none failed. Not vacuous: a fake `ghp_` token through
  the same command reports "leaks found: 1". Log: `do-evidence/hook-iter4.log`.
- `patch.diff` applies to the stack base: `git apply --cached --check` over an index read from
  `b452551` succeeds. It still does not apply to bare `origin/main` (`b452551` is not an ancestor
  of it: #1061/#1062 are unmerged), which is why C4 diff coverage cannot be measured; the
  iteration-3 sign-off says that is not a builder defect.
- Production code added by the whole patch: 76 KB (`sessions.rs` 42.8 KB, `retirement.rs` 15.2 KB,
  `marks.rs` 14.1 KB, the rest under 2 KB each), under the sign-off's 150 KB cap. Tests 127 KB,
  docs 16 KB. This iteration's own change to production code is about 25 lines in `sessions.rs`.

## Demonstrated reds

**Through the project runner:** B-iter3 — iteration 3's bookmark rule put back in
`sessions.rs` (the carry-forward's item 1), then `./engine/xtask.sh ci`: rc 1. Clippy, build, deny,
statics, deploy-guard and DST passed. In the workspace tests the failing binary is the new file,
with four legs failing — `d_a_wide_entry_behind_one_held_for_a_short_fleet_retires`,
`d_a_completed_sessions_wide_entry_behind_a_held_one_retires`,
`d_a_wide_entry_behind_one_held_past_one_step_retires` (each "the wide entry retires within 3
passes", `multipart_session_terminal.rs:2205`) and `d_a_held_entry_keeps_its_place_in_the_walk`
(the bookmark after the first pass, `:2419`) — "23 passed; 4 failed". The other failed check was
`cargo fmt --check`, because the mutation's inserted lines are not formatted. (`cargo test
--workspace` stops at the first failing binary, so the binaries after it did not run in this red
run; the green run ran them all.) Restored from a
backup; the regenerated diff of the worktree is byte-identical to `patch.diff`, the tree the green
run tested. Extract: `do-evidence/red-iter3-ci4.log`.

**All of them, through a scripted `cargo test` with a 900 s timeout per run** (32 mutations, each
an exact one-match replacement in production code, every production file restored from a backup
copy after each run — not from git, since the patch is uncommitted in the worktree — and checked
byte-identical at the end). Script `do-evidence/reds4.sh`; the brief's six and iteration 2–3's
mutations are `do-evidence/mutate.py` unchanged; this iteration's are
`do-evidence/mutate4-sessions.py`; results `do-evidence/reds4.log`. Every one is red.

New this iteration:

| Mutation | Red leg(s) |
|---|---|
| **B-iter3**: iteration 3's rule put back (held entries claim the bookmark, first left wins, no reorder) | the three held-ahead legs and `d_a_held_entry_keeps_its_place_in_the_walk` |
| **B-noreorder**: the bookmarked entry not walked first | `d_a_wide_entry_behind_one_held_past_one_step_retires`, `d_a_held_entry_keeps_its_place_in_the_walk` (the hold order) |
| **B-notmoved**: `&& !moved` dropped (`sessions.rs:735`) | `d_a_lost_bookmark_costs_reads_not_progress`, `d_a_wide_entry_behind_one_held_past_one_step_retires` |
| **B-nofilter**: a bookmark naming another session's entry taken as usable | `d_a_bookmark_naming_no_entry_of_its_session_is_no_evidence` |
| **B-keptlt / B-keptge**: a pre-walk-held entry never / always keeps a count (`:640`) | `d_a_held_entry_keeps_its_place_in_the_walk` / the short-fleet and `Completed` held-ahead legs (a zero-count bookmark is written) |
| **B-coveredge**: a zero count is kept (`:697`) | `d_the_terminal_delete_waits_for_an_empty_owned_range` |
| **B-heldge**: every entry counted as held (`:698`) | `d_a_wide_entry_behind_one_held_past_one_step_retires` |
| **B-keptfirst**: `kept.or(advanced)` (`:709`) | same |
| **S-fleet**: the deployed caller passes `operator_fleet_size: fleet.len()` (`crates/server/src/custodian.rs:577`) | `deployed_retirement_leg_waits_for_the_whole_operator_fleet_and_keeps_completed_tombstones`: "the untrusted entry is held … left: None" |
| **S-tomb**: the deployed caller passes `Tombstone::Elapsed` (`custodian.rs:580`) | same test: "the completed upload's record — its tombstone — stays" |

The brief's six, re-run on this tree (same mutations as iterations 2–3):

| Brief item | Mutation | Red leg(s) this run |
|---|---|---|
| `sidx:`-empty gate removed from the terminal delete | M1 | 3 legs, incl. `d_the_terminal_delete_waits_for_an_empty_owned_range` |
| `Completed` terminal-deleted without the tombstone input | M2 | 5 legs, incl. `d_a_completed_session_keeps_its_tombstone_until_the_window_passes` |
| `mpuctl` decrement not under its precondition / outside the batch | M3a / M3b | the race leg / the race, ledger-moved and many-sessions legs |
| untrusted record deleted without marking by chunk id | M4 | 12 legs |
| retirement by chunk id on a short fleet | M5 | 6 legs: all four fleet controls and two new ones |
| E2 marked only from the listing | M6 | 3 legs, incl. `e2_an_aborted_sessions_wrong_length_entry_is_retired_by_chunk_id` |

Also red as before: R1–R12 (iteration 1's two stalls, budget and cap mutants) and M0 (session leg
not dispatched: 25 of 27 fail; the 2 green are the `Open` and undecodable-`part:` safety controls,
which the base satisfies anyway).

On the real base the new test file does not compile (no `Tombstone`, no context fields), so
C4-verify reports it UNVERIFIABLE, as the brief pre-declares.

## Refuting my own test

- **(a) Genuine red?** Yes. Reverting each piece of this iteration's fix turns a named leg red:
  iteration 3's rule (B-iter3) fails all three held-ahead legs; dropping the reorder fails the
  part-way leg; dropping `!moved` fails the lost-bookmark leg; each deployed input mutated fails the
  server test on its own assertion. B-iter3 also went red through `./engine/xtask.sh ci` (above).
  The final green gate ran on the finished tree; after the runner red, the tree was restored and
  its regenerated diff checked byte-identical to `patch.diff`.
- **(b) Production path?** Yes. The custodian legs drive `wyrd_custodian::reconcile_step` with the
  context the deployed loop builds (`operator_fleet_size` = configured count, `Tombstone::Retain`
  unless a leg names `Elapsed`), over real redb, with sessions made by the production Create, Abort
  and Complete. The server test drives `CustodianService::run_reconstruction_until` itself, so the
  two inputs at `custodian.rs:577,580` are the ones under test, not copies.
- **(c) Fixture includes the fault?** Yes. The held entries are really held (a wrong-length
  placement on a short fleet; a `Completed` session's wrong-length entry; a mark of none of the
  three shapes at position 240 of a 256-position entry). The wide entries really exceed
  `MARKS_PER_STEP` (256 positions). The lost-bookmark leg really has every position own-marked and
  no bookmark. The server test's server 5 really holds a fragment of the untrusted chunk and is
  really absent from the loop's fleet, with nothing marked unreachable.

## Rubric self-review (AGENTS.md *Review rubric & protocol*)

- **One clock per lifecycle**: no new clock read. The server test's loop clock is one fixed closure
  (`LOOP_NOW`), as the other tests in that file use.
- **Narrow seams / dependency direction**: no new trait use; the change is inside `walk_step`.
- **Validation boundaries**: the bookmark stays custodian bookkeeping; one naming no entry of the
  session is audited and read as absent (now tested).
- **No DST-reachable shared mutable global state**: the only new `static` is a test-only
  `Once` in `crates/server/tests/custodian_gc.rs` (the pattern `custodian_day_one.rs` already uses);
  the statics gate passed.
- **`#![forbid(unsafe_code)]`**: no new crate; unsafe-guard passed.
- **Docs currency**: the persisted bookmark's meaning changed (which entry it names, and that it is
  walked first), so `docs/design/architecture/06-runtime-view.md:92` says so, and the module doc
  (`sessions.rs:38-54`) states the rule. No port, API, RPC, CLI flag or new persisted field.
- **Absent or unsupported entries**: unchanged; the one "nothing moved, nothing held" path is still
  counted and emitted.
- **Transactions**: unchanged; the bookmark write is still under `require`/`require_absent` of the
  bytes read.
- **Await discipline**: the server test bounds its loop with `tokio::time::timeout` (120 s); its
  shutdown future only polls a counter.
- **Test fidelity**: see (b) above. Seeded DST: none added, per the brief; the two review findings
  asking for it are recorded-rejected with the brief's words as the reason.

## Scratch

Everything throwaway is under `$PDCA_SCRATCH/pdca-builder-1013-redleg4` (logs, file backups,
five throwaway index files, a copy of iteration 3's `sessions.rs` for the line count; no build
cache). It sits inside the harness's per-issue scratch root, which
the harness reclaims, so I left it rather than run `rm`. The worktree holds exactly the patch,
uncommitted.
