# Build notes — #1013 (multipart session terminal delete, 636.6b), iteration 6

Target: `getwyrd/wyrd` `main`, built on the stack base `b452551`
(`pdca-integration/main-r9dd123c9ed70` = `main` + #1007–#1011, #1027–#1030, #1061, #1062). All
`path:line` below are in the patched tree (base `b452551` + `patch.diff`).

## What this iteration is

The iteration-5 sign-off: "Keep iteration 5's design and patch … Fix only the two new bookmark
bugs." I applied `iteration-v5/patch.diff` to the clean worktree (`git apply`, no fuzz) and changed
only the tail of the owned walk's step in `crates/custodian/src/retirement/sessions.rs`
(101 changed lines, most of them doc comment), its tests, and the doc sentences those changes touch.
The terminal delete, the `mpuctl` decrement, the part cursor, the scan position, the whole-fleet
gate, the deployed caller and its server test are byte-for-byte iteration 5's.

| Carry-forward item | What I did | Where |
|---|---|---|
| 1. Starvation: held wide entries move the bookmark every pass and each commit costs a step; 16 such sessions starve a 17th. "Do not charge a step for a bookmark-only move while the step holds something, **or** do not let a zero-progress re-read displace a held entry's count." Add a steady-state-quiet assertion to the three-held test and a 17-session regression. | Both, each a little differently from the wording, for reasons below. **(A)** A step whose batch marks nothing and deletes nothing is not charged, whether or not it holds something, and ends the session's visit. **(B)** A step that moves the scan position on does not move the bookmark to the entry it only re-read. | `sessions.rs:825` (`passing`), `:833-835` (B), `:887-889` (A, the charge), `:908` (A, the visit ends); docs `:51-58`, `:108-115`, `:166-171`; `retirement.rs:161-170`; `06-runtime-view.md:92` |
| 2. Recreated bookmark: a bookmark-only commit guarded only by `require_absent(bookmark)` can recreate `custodian:owned-walk:<id>` after another drainer's terminal delete. Add the exact session-record precondition to every bookmark-producing commit; add a wrapper test. | Every batch that **puts** the bookmark now also requires the session record's exact bytes as read, as `move_scan` does (`sessions.rs:1117`). There is one such place in the code. | `sessions.rs:850-854`; reservation `:181-190`, `:686-694`; doc `:60-66` |

Nothing else of the patch moved. Two bundle files outside the patch also changed; see *The DST
finding and the decisions file* at the end, which needs your eye.

## Item 1: why two rules, and why not the literal wording

**The defect.** One bookmark per session, but two or three entries can each want it. In the
reported case (three entries each held at position 150 of 256) a step reads the bookmarked entry
(1 read, held), the next (151 reads, held) and runs out of reads 81 positions into the third. That
third entry was "walked without a hold", so iteration 5 gave it the bookmark, and the next pass gave
it back. Every such move was a commit, and every commit was charged a step.

**Rule A — a step that marks and deletes nothing is free** (`sessions.rs:887-889`, `:908`). The
step budget exists to bound what a pass *changes*. A batch with no mark and no delete can only move
the bookmark. So it is not charged, and it ends the visit, exactly as a step that commits nothing
already did. Bound per session per pass: one such step, so at most one bookmark-only commit beside
the scan position's two (three small commits; asserted as `<= 3`, `multipart_session_terminal.rs:3611`).

Why wider than "while the step holds something" (the sign-off's first option): there is a case where
the bookmark-only step holds nothing. Two wide entries on two different pages of one session, each
held deeper than one step reads (positions 240 and 250), with a page of held entries between them.
The bookmark can count only one, so every lap the walk re-reads the other from position 0, spends
all 233 reads on marks it already wrote, and stops before it reaches the hold. That step holds
nothing and only moves the bookmark. I built that case (`Churn::PastOneStep`,
`multipart_session_terminal.rs:3454-3460`) and ran the literal option against it: mutation
`A-literal` is **red** on `d_sixteen_sessions_held_past_one_step_spend_no_step`. So is `C-unless-held`
(charge when nothing is held) and `C-if-held` (charge when something is held): the test needs both
kinds of step to be free.

**Rule B — a step that moves on leaves the bookmark alone** (`sessions.rs:833-835`). When a step
marks nothing, deletes nothing and holds the entries at the head of its page, it moves the scan
position past them (iteration 5's rule). The entry its reads ran out on is then the first entry of
the next step's page, where it gets the reads the held ones took. It does not need the bookmark, so
it no longer takes it. In the reported case the bookmark now comes to rest on a held entry and
stays there: no bookmark write at all in the steady state.

Why not the sign-off's second option as worded ("do not let a zero-progress re-read displace a held
entry's count", always): an entry whose marks are all written but whose count was lost *must* take
the bookmark from a held entry, or it is never finished. That is the existing test
`d_a_page_whose_first_entry_is_not_held_stays`. Mutation `Y-when-idle` (the literal reading) is red
on it and on three more. Rule B applies only when the scan position moves, which is exactly when the
entry gets a better turn next step.

One line of the rule looked needed and is not: I first wrote "…unless the bookmark already names
that entry". That branch cannot run. The bookmarked entry is walked first, so if the step's reads
ran out on it, nothing else was read and nothing was held, and the step is not one that moves on. I
removed the branch rather than ship a line no test can reach (`sessions.rs:56-58` says why).

**Why both rules.** Either alone fixes the reported case; neither alone is enough.

| With only | Reported case (three held at 150) | Two held past one step, two pages |
|---|---|---|
| Rule A | not starved, but the bookmark is rewritten every pass, forever | not starved |
| Rule B | bookmark at rest, nothing charged | **starved** (mutation `C-always` is red) |

A alone leaves a write per session per pass that B removes for free. B alone leaves the deep-hold
case charging a step every pass, because with one bookmark and two entries held deeper than a step
reads, the bookmark has to move: that is what a single bookmark means. A makes that move free.

### Alternatives ruled out (with the cost)

- **The literal option (a)**: one condition different (`if !idle || !any_held`). Leaves the two-page
  case charged every pass. Red, as above.
- **The literal option (b)**: `.filter(|_| !idle)` instead of `!passing`, same size. Stalls an entry
  whose count was lost. Red, as above.
- **Let a free step continue the visit** instead of ending it: one token fewer (`idle ||` at `:908`).
  Then the number of free steps in one visit is bounded only by an entry's width over 233, and a
  damaged placement can name far more positions than a scheme's 510, so a pass's reads would depend
  on stored data. Ending the visit makes the bound "one per session per pass". Its price: a lost
  bookmark is caught up one step's reads per pass (a 256-wide entry: one pass later than before),
  and a hold deeper than one step is named one pass later. Mutation `S-idle-goes-on` is red.
- **One count per entry** (a list in the bookmark's value): removes the contention at its root, about
  30 lines and a stored-format change. Not taken: "do not redesign", three sign-offs running.
- **The session precondition on every walk batch**, not only those that put the bookmark: the same
  line, moved up four lines. It would also refuse a stale drainer's *mark* batch after the session
  is gone. Not taken: the sign-off scoped the fix to "every bookmark-producing commit" and put
  stale marks (cf. #825) out of the builder's scope. If you want it, it is that one move.
- **Re-read the session record after a refused commit and end the visit if it moved**: about 6
  lines. A record in `Aborting` or `Completed` changes only by the terminal delete, which needs the
  range empty; the next step then reads the range empty and the terminal batch finds the record
  gone. So the branch would be unreachable short of an operator's edit. Not added.

## Item 2: the bookmark is put only under the session record

`sessions.rs:850-854`. The batch that puts `custodian:owned-walk:<id>` requires `mpu:<id>` to hold
the exact bytes the pass listed. A batch that *deletes* the bookmark needs no such condition: a
delete cannot leave a record behind.

The batch budget counts the extra precondition: `BOOKMARK_OPS` is 3 (`:183`), and the session key
and record are in the reserved bytes (`:693-694`). The compile-time check had to be restated, since
`3 + 2×233 + 2×16 = 501`. The honest worst case is 499: a step that marks anything leaves the entry
it marked (an entry is deleted only in a later step than the one that marked it), so it deletes at
most 15 of its 16 entries (`:185-190`). The run-time budget (`OwnedStep::run`, `OwnedStep::delete`)
still enforces the limit whatever the page holds.

## Tests (the file is now 43 legs; 3 new, 2 extended)

All through `reconcile_step` with the deployed context, over real redb, sessions made by the
production Create, Abort and Complete. `crates/custodian/tests/multipart_session_terminal.rs`:

| Leg | Line | What it pins |
|---|---|---|
| `d_a_trusted_entry_behind_three_held_part_way_retires` (extended: the steady-state-quiet assertion, no trusted entry left) | 2934, block at 2981 | After the trusted entry retires: over 8 passes the bookmark's stored bytes never change (the literal key, value = third entry's key + 150), each pass commits exactly one session-leg batch and it touches only `custodian:owned-scan:<id>`, no `orphan:` record changes, every pass is `Blocked` with `unreadable-mark` holds. |
| `d_sixteen_sessions_held_part_way_spend_no_step` | 3681 | The adversary's case, sixteen times, then eight more sessions per round. See below. |
| `d_sixteen_sessions_held_past_one_step_spend_no_step` | 3689 | The two-page deep-hold case, sixteen times. The one that separates Rule A from the literal option. |
| `d_no_bookmark_is_put_once_another_drainer_has_ended_the_session` | 3373 | The wrapper holds drainer A's bookmark batch back; drainer B runs real passes until the session is gone (walks the entry, drains the obligation, commits the terminal delete); then A's batch reaches the store. Asserts: one batch putting the bookmark was sent, the store answered `Conflict`, it carries the precondition on `mpu:<id>` with the bytes read, nothing is under `custodian:owned-walk:` or `custodian:owned-scan:`, the count went down by exactly one, A holds nothing. |
| `d_a_lost_bookmark_costs_reads_not_progress` (extended) | 2377, block at 2396 | The first pass commits one session-leg batch, the bookmark alone, under the session precondition, and the entry is still there: a step that marks and deletes nothing ends the visit. |

**The starvation legs** (`held_sessions_spend_no_step`, `:3491`). Sixteen held sessions, as many as
a pass has steps. Listed after them, the seventeenth onward: four rounds of eight sessions, each
with one residue entry, `Open` until its round. The sixteen are first run to a steady state (four
passes in a row that mark and delete nothing). Then each round aborts eight sessions and runs
**one** pass. Eight sessions need two steps each (mark, then delete): all sixteen steps. So the
assertion "all eight residue entries are gone after this one pass" (`:3586`) fails if even one held
session spent a step. Four rounds cover every phase of the four-pass lap. Per round it also asserts:
exactly 16 batches that mark or delete, each of the sixteen commits at most 3 batches and only of
its own two records, and for the part-way case no bookmark write at all. For the deep-hold case it
asserts the fault is really there: every one of the sixteen writes its bookmark in every round, in
at least one round where no hold names it and at least one where a hold does (`:3627-3646`). Every
one of the sixteen is named in a hold within the four rounds (`:3640`). Then all 32 sessions end and the count
is down by exactly 32.

Why eight sessions a round and not one "seventeenth": with one, a mutant that charges only half the
held sessions in a given pass still leaves two steps over, and passes. Needing all sixteen steps
makes the test say what the rule says: a held session costs the pass no step.

Helper changes: an `Overtaken` hook on the recording store (`:331-335`, `:454-475`) that runs a
second drainer to a session's end just before the first batch putting a given key reaches the
store; `walks_own_records` (`:3464`).

## Gates run this iteration

- **`./engine/xtask.sh ci`** (the project runner) on the final tree: **rc 0, "xtask ci: all checks
  passed"**. `typos`, `lint_docs.py` OK, `render_site.py --check` 101 pages, link audit OK, the three
  guards, fmt, clippy, build, workspace tests (233 `test result: ok`, none failed; the new file
  **43 passed**, `custodian_gc` **11 passed**), cargo-machete, cargo-deny, conformance vectors,
  statics, deploy-guard, `wyrd-dst` under `--cfg madsim`. The tree was checked byte-identical before
  and after the run. Extracts: `do-evidence/ci6-steps.log`, `do-evidence/ci6-tail.log`.
- **The same runner, red:** iteration 5's `sessions.rs` put back, everything else final: **rc 1**.
  The only failed check is `cargo test --workspace`, in the new file: "38 passed; 5 failed" (the
  three new legs and the two extended ones). Restored and compared byte-identical.
  Extract: `do-evidence/red-iter5-ci6.log`; where each leg fails: `do-evidence/red-iter5-code-detail6.log`.
- **Commit hooks** (the target's `.pre-commit-config.yaml` on `origin/main`): pass. `pre-commit`
  itself is not installed here, so, as in iterations 3 to 5, I ran each hook's own command over the
  patch staged in a throwaway index (25 paths, the 25 of `patch.diff`; the real index untouched):
  the `forbid-machine-local-files` regex, no match; no `.gitleaksignore` in checkout or index;
  `gitleaks git --pre-commit --redact --staged --verbose --ignore-gitleaks-allow` with the guard's
  config (`[extend] useDefault = true`), gitleaks 8.30.1, "no leaks found", rc 0; the staged-blobs
  hook's loop, 25 scanned, none failed. Control: a made-up `ghp_` token through the same command
  reports "leaks found: 1". Log: `do-evidence/hook-iter6.log`.
- `cargo fmt --all` leaves the tree unchanged (it is also a step of the gate above).
- `patch.diff` applies to the stack base (`git apply --cached --check` over an index read from
  `b452551`). It still does not apply to bare `origin/main` (#1061/#1062 unmerged), so C4 diff
  coverage stays unmeasurable; the sign-off says that is not a builder defect.
- Size: production code added by the whole patch 86.1 KB (`sessions.rs` 53.0 KB, `retirement.rs`
  15.4 KB, `marks.rs` 13.8 KB, the rest under 2 KB each), under the 150 KB cap. Tests 181.8 KB,
  docs 18.2 KB. This iteration added about 3.5 KB to `sessions.rs`, most of it doc comment.

## Demonstrated reds

Scripted `cargo test -p wyrd-custodian --test multipart_session_terminal` under a 900 s timeout per
run; each mutation is an exact one-match replacement in production code, every file restored from a
backup copy afterwards and compared byte-identical. Every one is red.

**The brief's six**, re-run on this tree (`do-evidence/reds6-carried.log`, mutations in
`do-evidence/mutate.py`):

| Brief item | Mutation | Result |
|---|---|---|
| `sidx:`-empty gate removed from the terminal delete | M1 | 4 legs red |
| `Completed` terminal-deleted without the tombstone input | M2 | 5 legs red |
| `mpuctl` decrement not under its precondition / outside the batch | M3a / M3b | 1 / 3 legs red |
| untrusted record deleted without marking by chunk id | M4 | 14 legs red |
| retirement by chunk id on a short fleet | M5 | 9 legs red |
| E2 marked only from the listing | M6 | 3 legs red |

**Carried from iterations 1 to 5**, same log, 58 runs in all, every one red: R1–R12, M0 (session leg
not dispatched: 41 of 43 fail), iteration 4's bookmark mutations, iteration 5's 27, and the two
deployed-input mutations against the server test (S-fleet, S-tomb). Seven of them no longer matched
the text this iteration changed; I re-spelled those for the new code rather than drop them
(`do-evidence/mutate6-carried.py`: R5, `iter3`, `keptfirst`, `P-notidle`, `P-nopass`, `P-passed-ge`,
`B-kept-over-elsewhere`).

**This iteration's 12** (`do-evidence/mutate6.py`, results `do-evidence/reds6.log`):

| Mutation | Red leg(s) |
|---|---|
| `C-always`: every committed step charged (Rule A off, Rule B on) | the deep-hold starvation leg |
| `C-unless-held`: charged when the step holds nothing | the deep-hold starvation leg |
| `C-if-held`: charged when the step holds something | the deep-hold starvation leg |
| `C-idle-only`: the charge inverted | both starvation legs, `d_sessions_past_one_page…` |
| `S-idle-goes-on`: a free step does not end the visit | the lost-bookmark leg, the deep-hold leg |
| `A-literal`: the sign-off's option (a) as worded | the lost-bookmark leg, the deep-hold leg |
| `Y-never`: Rule B off (Rule A on) | the three-held steady state, the part-way starvation leg |
| `Y-when-idle`: Rule B on every step that marks nothing | `d_a_page_whose_first_entry_is_not_held_stays` and 3 more |
| `Y-inverted`, `Y-passing-any` | 15 and 11 legs |
| `P-no-session`: the bookmark put without the session precondition | the overtaken-commit leg, the lost-bookmark leg |
| `P-session-absent`: the wrong condition on the session record | 27 legs |

On the real base the new test file does not compile (no `Tombstone`, no context fields), so
C4-verify reports UNVERIFIABLE, as the brief pre-declares.

## Refuting my own test

- **(a) Genuine red?** Yes. Through the project runner, iteration 5's walk code turns all five
  changed legs red and nothing else. Each fails at the assertion that states the defect: the
  starvation legs at "…'s residue is walked away in this one pass" in round 0 (`:3586`); the
  overtaken-commit leg at "and it was refused" with `Committed` (`:3420`); the three-held leg at
  "steady pass 1" with two batches (`:3003`); the lost-bookmark leg at "one batch" with two (`:2402`).
- **(b) Production path?** Yes. Every leg calls `wyrd_custodian::reconcile_step` with the context the
  deployed loop builds, over `RedbMetadataStore::in_memory()`. The second drainer in the
  overtaken-commit leg is the same `reconcile_step`, run to the session's real terminal delete; the
  wrapper only orders the two. Keys are asserted as literal strings.
- **(c) Fixture includes the fault?** Yes, and the tests check it rather than assume it. The deep-hold
  leg asserts that every held session's bookmark really moves in every round, both in a pass that
  names it in no hold and in one that does. The part-way sessions are really held (a mark of no
  known shape at position 150 of each of three 256-wide entries, the first 150 marked by the walk
  itself during settling, not planted). The sixteen really fill the step budget's worth of
  sessions, and the eight really need every step. The rival in the overtaken-commit leg really ends
  the session (the hook asserts the record is gone before it lets the batch through).

## Rubric self-review (AGENTS.md *Review rubric & protocol*)

- **One clock per lifecycle**: no new clock read.
- **Narrow seams / dependency direction**: only `MetadataStore` calls already used in this file.
- **Validation boundaries**: untouched.
- **No shared mutable global state**: none added; the statics gate passed.
- **`#![forbid(unsafe_code)]`**: no new crate.
- **Docs currency**: no port, operation, RPC, flag or stored field changes. The runtime view
  describes this walk in detail, so `06-runtime-view.md:92` now says the bookmark is put only while
  the upload's record is unchanged, that a batch which only moves it is outside the pass's budget,
  and that a batch which moves on leaves it on a held entry. Module docs `sessions.rs:10-17`,
  `:51-66`, `:108-115`; `retirement.rs:161-170`.
- **Absent or unsupported entries**: unchanged; a held entry is still named every time its page is
  read, and within a lap at the latest (asserted for all sixteen sessions in both starvation legs).
- **Transactions**: no live transaction; each write is one `commit(batch)`. A refused bookmark batch
  is the store's answer to a moved session record or bookmark, and the walk reads again.
- **Await discipline**: no new await beyond `MetadataStore` commits already made here.
- **Test fidelity**: see (b). No DST scenario, per the brief; see the next section.

## The DST finding and the decisions file (please check)

Iteration 5's T4 gate reported "2 blocking". From `review-batch.md` in the bundle, both were the
same finding you have rejected three times: no seeded DST coverage for the session leg. They
blocked because of how the gate matches a recorded rejection: each line of `review-rejected.md`
binds to the **one** finding nearest its recorded line, and the three review passes anchored the
finding at three different lines of `sessions.rs` (243, 586, 841). One decision bound; two did not.

I did two things about it, both outside "fix the two bookmark bugs", so you should confirm them:

1. **`review-rejected.md` rewritten**: one decision ("no DST scenario lands with #1013; #660 owns
   it", quoting the brief and the sign-offs) recorded at 18 anchors, one per commit site of the
   session leg, the by-chunk retirement and the new test file. Class `TEST-GAP` and the word "DST"
   only, in those four files only. A finding of another class, or one that does not say "DST", is
   not cleared by any line. If you would rather triage these yourself each round, delete the file's
   lines; nothing in the patch depends on them.
2. **The in-code deferral marker reworded** (`sessions.rs:249-254`). It said "two drainers of one
   session", and the reviewers read that as covering only two-drainer interleavings. It now says
   what the brief and sign-offs say: seeded Tier-0 DST coverage of the session leg is deferred to
   #660, and no DST scenario lands with #1013. If #660's scope is narrower than that, this comment
   overstates it and the right fix is a tracking issue, not this wording. That is your call.

## Residuals (for the human)

1. **A held session still writes, for free.** Up to three small commits per pass for as long as its
   holds stand: the scan position (iteration 5's residual 1) and, only when two entries are each
   held deeper than one step reads, the bookmark. With holds a step can read up to (the reported
   case) the bookmark is at rest. A count per entry would remove the rest; it is a stored-format
   change.
2. **A pass can answer `Changed` over a held session.** In the deep-hold case, the step that
   re-reads an entry from position 0 runs out of reads before it reaches the hold, holds nothing,
   and ends the visit; the hold is named on the next pass. Before, the same pass went on and found
   it. Every hold is still named within a lap (asserted). The deployed loop does not branch on the
   answer (`crates/server/src/custodian.rs` matches no `Reconciled::` variant).
3. **A lost bookmark is caught up one step's reads per pass**, not several steps in one pass.
4. **Two lines the tests cannot bind.** The bytes reserved for the session precondition
   (`sessions.rs:693-694`) and `BOOKMARK_OPS = 3` (`:183`): a walk batch is at most about 2 MB and
   499 operations against budgets of 5 MB and 500, so neither limit is ever the one that stops a
   step. A mutant on those lines will survive; the compile-time check at `:190` is what covers them.
5. **Production reach (b), the cost**: unchanged for listings (`retirement.rs:158-170`). A pass now
   reads, per held session, at most one free step (233 mark reads, one page, at most one listing)
   beyond its 16 charged steps; before, such a step was charged whenever it moved the bookmark.
6. Residuals 2 and 3 of iteration 5 (holds named once a lap; several deep holds contend for one
   bookmark) stand, except that the contention no longer costs steps.

## Scratch

Everything throwaway is under `$PDCA_SCRATCH/pdca-builder-1013-iter6` (2.7 MB: logs, file backups,
throwaway index files, iteration 5's `sessions.rs` for the red runs; no build cache). It sits inside
the harness's per-issue scratch root, which the harness reclaims, so I left it rather than run
`rm`. The worktree holds exactly the patch, uncommitted.
