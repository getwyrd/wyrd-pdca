# Build notes — #1013 (multipart session terminal delete, 636.6b), iteration 3

Target: `getwyrd/wyrd` `main`, built on the stack base `b452551`
(`pdca-integration/main-r9dd123c9ed70` = `main` + #1007–#1011, #1027–#1030, #1061, #1062). All
`path:line` below are in the patched tree (base `b452551` + `patch.diff`).

## What this iteration is

A resume, as the iteration-2 sign-off asked: "Keep iteration 2's patch and design … do not
redesign. Run the target's commit hook (forbidden-file check + gitleaks) over the patch and record
the result in build-notes.md, then let Check run the full review."

So:

- **`patch.diff` is byte-identical to `iteration-v2/patch.diff`.** I applied the iteration-2 patch
  to the worktree (clean at `b452551`, `git apply` with no fuzz), regenerated the diff from the
  applied tree, and `cmp` reported no difference. 24 files, 3,789 insertions, 97 deletions.
- **The test file is byte-identical** to `iteration-v2/multipart_session_terminal.rs`
  (`cmp` clean), copied to `crates/custodian/tests/multipart_session_terminal.rs` in this bundle.
- No production or test line changed. The design, rationale, alternatives and red table of
  iteration 2 (kept below, unchanged except where marked) still describe this patch exactly.
- Iteration 2's declared missing dependency (gitleaks) is **resolved**: gitleaks v8.30.1 is now at
  `~/.local/bin/gitleaks`, and the hook ran (next section). No external dependency is missing this
  time, so these notes declare none.

## Commit hook (target's `.pre-commit-config.yaml` on `origin/main` `57aefe0`) — PASS

The stack base has no hook config; `origin/main` adds it (#1067/#1070/#1072) with three hooks:
`forbid-machine-local-files`, `gitleaks` (the gitleaks repo's own hook, rev `83d9cd6…` = v8.30.1,
arg `--ignore-gitleaks-allow`) and `gitleaks-staged-blobs`. `.secret-guard/activate.sh` runs them
with `GITLEAKS_CONFIG` pointing at a file holding only `[extend] useDefault = true` (gitleaks'
built-in rules; a checked-out `.gitleaks.toml` is ignored) and refuses while a `.gitleaksignore`
exists.

`pre-commit` itself is not installed here (`which pre-commit`: nothing), and installing it
(`pipx install pre-commit==4.7.0`) would change the host, so I ran each hook's command directly,
with the same binary, args and config, over the patch staged into a **throwaway index** in scratch
(`GIT_INDEX_FILE=<scratch>/index; git read-tree HEAD; git add -A`), so the worktree's own index
was not touched. 24 paths staged, the same 24 as `patch.diff`.

| Hook | Command run | Result |
|---|---|---|
| `forbid-machine-local-files` | its `files:` regex `(^|/)(\.claude/settings\.local\.json[^/]*\|CLAUDE\.local\.md\|\.credentials\.json\|\.env)$` over the 24 staged paths | no match — **pass** |
| guard pre-check | `.gitleaksignore` in checkout or index? | absent — **pass** |
| `gitleaks` | `GITLEAKS_CONFIG=<useDefault toml> gitleaks git --pre-commit --redact --staged --verbose --ignore-gitleaks-allow` | "scanned ~182432 bytes … no leaks found", rc 0 — **pass** |
| `gitleaks-staged-blobs` | for each staged path: `git cat-file blob ":0:$f" \| gitleaks stdin --no-banner --redact --ignore-gitleaks-allow` | all 24 `ok`, rc 0 each — **pass** |

The scan is not vacuous: piping a line holding a fake `ghp_` + 36-character token through the same
`gitleaks stdin` command and config gives `leaks found: 1`, rc 1.

What this does not cover: the exact `pre-commit` 4.7.0 wrapper (it only selects files and passes
them on; `types: [file]` admits all 24 here) and the guard's own hook-path checks, which concern
the clone's setup, not the patch content.

## Gates run this iteration

- **`./engine/xtask.sh ci`** (the project runner, `pdca.toml` `[gates] runner`; it `cd`s into
  `$PDCA_WORKTREE` and runs `cargo xtask ci`) on the patched worktree: **rc 0, "xtask ci: all checks
  passed"**. The prose gates ran, not warn-skipped: `typos`, `lint_docs.py` ("OK"),
  `render_site.py --check` (101 pages, "link audit OK"). Then gitlink/unsafe/blackbox guards,
  `cargo fmt --all -- --check`, clippy (workspace, all targets), build, the whole workspace test
  suite (233 `test result: ok` lines, none failed; the new file: **21 passed, 0 failed**),
  `cargo-machete`, `cargo deny` (advisories, bans, licenses, sources; plus the all-features
  advisories run), conformance vectors, the ADR-0035 statics gate, deploy-guard, and `wyrd-dst`
  under `--cfg madsim` (clippy + tests). Extracts: `do-evidence/ci3-steps.log`,
  `do-evidence/ci3-tail.log`.
- `cargo fmt --all -- --check` is part of that run and was clean, so the patch is formatted as the
  target requires.
- Commit hook: pass (previous section); full output in `do-evidence/hook-iter3.log`.
- One red re-run through the same runner (below, "Demonstrated reds").
- Scratch: `pdca-builder-1013-{hook,ci,redleg}` under `$PDCA_SCRATCH` (logs and two throwaway
  index files, about 1 MB; no build cache). They sit inside the harness's per-issue scratch root,
  which the harness reclaims, so I left them rather than run `rm`. The worktree holds exactly
  `patch.diff`, uncommitted.

## Why the patch still does not apply to bare `origin/main` (C4-diff-cov, iteration 1's failure)

Unchanged from iteration 2, re-checked today. `origin/main` (`57aefe0`) still has no
`crates/custodian/src/retirement.rs` (`git cat-file -e origin/main:crates/custodian/src/retirement.rs`
fails): child-1 (#1061, #1062) has not merged, and this child is built on that drain (`Depends on:
1062`). The 16 commits `origin/main` has beyond the stack base touch only `.github/workflows/*`
(5 files), `.gitignore`, `.pre-commit-config.yaml`, `.secret-guard/activate.sh` and `Cargo.lock`;
none is a file this patch touches (`comm -12` of the two sorted file lists is empty). So the patch
applies to the stack base and would apply unchanged on the stack base merged with `origin/main`.
A gate that measures against bare `origin/main` cannot apply any stacked bundle until its
prerequisites merge; that is a base-selection matter for the gate, not a patch defect.

---

# Iteration 2's notes (still accurate for this identical patch)

## Kept from iteration 1 (the sign-off said keep it)

The session leg (`crates/custodian/src/retirement/sessions.rs`, run first by
`retirement::reconcile`, `retirement.rs:527`); the terminal delete with its one batch under the
session record's exact bytes and the `mpuctl` CAS (`sessions.rs:739-808`,
`AdmissionRecord::release_one` `crates/core/src/multipart.rs:2091`); `ledger_unusable` (a lost
race is never a false "needs a human" hold); `Tombstone` and `operator_fleet_size` on
`ReconstructionContext` (`crates/custodian/src/reconstruction.rs:149,155`), the deployed caller
passing its fleet size and `Tombstone::Retain` (`crates/server/src/custodian.rs:577,580`); the
`{session}` obligation waiting (`owned-residue-first`) rather than holding while the walk drains
the residue; the context-literal ripples across 13 test files; the two changed child-1 tests
(`multipart_retirement_marks.rs`, `staged_protection.rs`). Rationale and alternatives for those
are in `iteration-v1/build-notes.md` and still hold.

## The iteration-1 carry-forward, point by point

| Carry-forward item | What I did | Where |
|---|---|---|
| 1. `marks.rs:520` — a damaged part restarts at position 0, ignoring the cursor; parts past ~232 positions stall as `Partial` | The part now resumes from its cursor. Its items are its trusted positions in record order, then its untrusted chunks in chunk-id order, **one item per chunk**. A chunk is counted only when every fragment of it the listing shows was read in that one step. A chunk listed with more fragments than one step reads is held, typed (`listing-past-step`). | `crates/custodian/src/retirement/marks.rs:540` (`part` → `by_chunk_part`), `:548-621`; module doc `:82-97` |
| 2. `sessions.rs:443` — the owned walk restarts each step at the range head and each entry's first position; entries past 233 positions stall with no hold | The walk keeps a per-session bookmark, `custodian:owned-walk:<upload-id>` = (first unfinished entry's key, how many of its named positions are marked). It is written in the batch whose marks it counts, under a precondition on the bytes read; removed with the entry or by the terminal delete. Listed fragments (not named by the entry) are never counted in it; every step that reaches them reads them all. | `sessions.rs:131-171` (key, codec), `:386-506` (`OwnedStep`), `:513-735` (`walk_step`), `:803` (terminal delete removes it), `:909` (unusable-bookmark audit); module doc `:18-66` |
| Regressions crossing `MARKS_PER_STEP`, `SESSION_STEPS_PER_PASS`, `SESSION_PAGE` | Four new legs (below), plus exact per-step mark counts. | `tests/multipart_session_terminal.rs:1813`, `:1888`, `:1961`, `:2026` |
| `Completed` (`Tombstone::Elapsed`) control holding a wrong-length and an undecodable `sidx:` entry | New leg: both held (`untrusted-placement`, `record-unreadable`), records byte-identical, nothing marked, session kept, fragments kept past GC grace. | `tests/multipart_session_terminal.rs:1710` |
| Cost claim at `retirement.rs:145-146` | Corrected to the real bound, and the reason listings are not charged a step. | `crates/custodian/src/retirement.rs:155-166` |
| Rebase onto current `origin/main` | Not possible as asked: see "Why the patch still does not apply to bare `origin/main`" above. | — |

## Design of the two fixes, and why

**Fixed orders are resumable; listings are not.** The positions a record names itself (a part's
trusted chunks, an `sidx:` entry's placement) are a fixed order from its bytes, so a count into
them is safe across steps. A fleet listing is not: if a fragment drops out of it between steps (a
disk fault, a server replaced at the same fleet size), every later fragment shifts down one, and a
count into it would step over one never marked. Example: listing `[A, B, C, D]`, cursor at 2 (A, B
marked); B vanishes; next listing `[A, C, D]`, cursor 2 claims A and C marked, so C is skipped.
Once the record is deleted, C has no mark and no record and GC keeps it for good (`gc.rs:738` on
base). So:

- **Parts** (`marks.rs:548-621`): trusted positions resume from `item` as for any entry; then
  `item = trusted + j` counts whole untrusted chunks. A chunk is counted only when all of its
  listed fragments were covered (read as this obligation's, or marked) in the same step. A covered
  chunk stays covered: nothing adds a fragment to a part's chunk once the session left `Open`
  (part commit follows every write's `Ok`; reconstruction's re-place runs only for `Open` uploads
  and pre-marks its target, brief Design), and a fragment leaving is harmless. Deletion still
  needs every item covered before the step or read as own in it (`marks.rs:616`).
- **Owned entries** (`sessions.rs:591-674`): named positions resume from the bookmark; listed
  fragments not named are read whole in each step that reaches them, so the deleting step's
  listing is the last one before the delete (the brief's late-write residual is unchanged:
  `sidx:` chunks *can* gain a fragment between steps, which is why these are never counted).

**The bookmark rule** (`sessions.rs:676-698`): it names the first entry of the page the step did
not delete, at the count of that entry's named positions covered once the batch commits (zero →
absent). Entries before it were deleted in that step, so next step it is the range head and gets
the full read budget. A step either marks, deletes, or moves the bookmark forward, so it always
progresses unless something is held. If nothing is held and nothing moved (should not happen), it
counts and emits a `no-progress` wait instead of a silent stop (`sessions.rs:704-712`).

**Entries held before walking** (not `Aborting`, short fleet, key names no chunk) keep their
bookmark count (`first_left.get_or_insert((key, from))`, `sessions.rs:619`, `:626`), so a
short-fleet pass does not throw away progress.

**The listing cap** `LISTED_PER_CHUNK = MARKS_PER_STEP - 1 = 232` (`retirement.rs:261`): a step
resuming at a part's untrusted chunk has 233 reads less the session-record read. Past it the
record is held as `Hold::ListingPastStep` (`retirement.rs:340`, detail says "needs a human",
`retirement.rs:485`). Only a chunk with more than 232 listed fragments meets it: a scheme with
`k + m > 232` (RS(128,128) is supported) whose placement is also damaged, or many duplicate
copies.

**The whole-fleet gate is checked before anything of the record is marked**
(`ByChunk::whole`, `retirement.rs:412`, used at `marks.rs:555` and `sessions.rs:612`), with no
I/O. The listing itself is lazy: taken only when a step reaches the listed fragments
(`marks.rs:590`, `sessions.rs:634-640`). A listing that fails is still a `partial-fleet` hold where
it stands; marks already queued for the record's own positions then commit, which is harmless
progress.

### Cost per pass (Production reach (b))

- Fleet listings: zero unless an untrusted record of an `Aborting` session is met. At most one per
  step. The ledger walk spends a step per obligation it plans, so ≤ `STEPS_PER_PASS` = 16. The
  session leg spends a step only on a commit; a walk step that commits nothing ends its session's
  visit, so ≤ 16 for committing steps + one per `Aborting` session held after its listing (≤
  `max_sessions`, 46 at the shipped profile). Worst case 16 + 16 + 46 = 78 listings of the fleet
  per pass, each one `list_fragments` per configured server, repeated each pass while such holds
  stand (`retirement.rs:155-166`).
- A non-committing listing is not charged to the step budget: with more than 16 held sessions at
  the head of the `mpu:` listing, every pass's 16 steps would go to them and the sessions after
  them would never be visited.
- Per walk step: 1 bookmark read + 1 page read + ≤ 233 mark reads + ≤ 1 listing; batch ≤ 500 ops
  (`2 + 2·233 + 2·16`, static assert `sessions.rs:135-136`).
- No reuse of GC's listing (GC is its own fenced pass after this one; unchanged from iteration 1).

## Alternatives ruled out (with the cost)

- **Position-granular cursor over the listing** (the reviewer's "listed ones in BTreeMap order"):
  unsafe when a fragment drops out, shown above. Rejected on safety, not cost.
- **Fragment-index-granular items** (`item = trusted + j·2^16 + index`): safe and would remove the
  `listing-past-step` hold, but needs a non-obvious encoding of `item` (~15 more lines in
  `by_chunk_part` plus doc) to help only chunks listed past 232 fragments. Not taken; the hold is
  typed and audited. The human may prefer it.
- **A bigger read budget for the owned walk instead of a bookmark** (separate "verify" reads from
  mark writes): needs a cap at least the longest entry, but a damaged placement's length is bounded
  only by the value ceiling (tens of thousands of entries), so it still needs a hold, and each step
  re-reads the whole entry: a 510-position entry costs 233 + 466 + 510 + 510 = 1,719 reads against
  ~510 with the bookmark. Rejected.
- **Derive the resume point from the marks (binary search for the first non-own mark)**: needs own
  marks to form a prefix. Another obligation's arm (b) re-stamp can turn one of ours into another
  event's mark inside the prefix, and the search could then land past an unmarked position.
  Rejected on safety.
- **One global bookmark for the whole session leg**: two sessions each stopped inside an oversized
  entry overwrite each other; with a held one at the head, the other's progress is lost every pass.
  The per-session key costs one blind delete in the terminal batch (`sessions.rs:803`).
- **Store the walk's progress in the `{session}` obligation's cursor**: the walk is the session
  leg's, a session need not have such an obligation left, and it would put two drains on one
  record. Rejected.

## Tests

`crates/custodian/tests/multipart_session_terminal.rs` — 21 tests, all through `reconcile_step`
with the deployed contexts, `RedbMetadataStore::in_memory()` behind a recording wrapper, a
placement-aware D-server double.

Added in iteration 2:

- `d_owned_entries_past_one_steps_reads_resume_from_the_walks_bookmark` (`:1813`): trusted
  RS(128,128) entry (256 positions) + wrong-length RS(4,2) entry naming 300 positions + a listed
  copy it does not name. Ends in **2** passes; first step marks exactly `MARKS_PER_STEP`; none
  more; bookmark written then gone; each entry marked over ≥ 2 steps before its delete.
- `e1_a_part_past_one_steps_reads_resumes_from_its_cursor` (`:1888`): part 1 = exactly 233
  trusted positions (so step 1 meets part 2 with zero reads left); part 2 = 39 trusted + 40
  untrusted RS(4,2) chunks (234 positions + 240 listed fragments). Step sizes 233 then 232, none
  above 233, ≥ 4 steps; everything marked before its part's delete; gone past grace; a committed
  object's fragments kept.
- `e_a_chunk_listed_past_one_steps_reads_is_held` (`:1961`): a part and an undecodable entry each
  with 233 listed fragments → `listing-past-step` every pass, nothing marked, records kept; a part
  listed with exactly 232 retires (boundary).
- `d_sessions_past_one_page_and_one_pass_of_steps_all_end` (`:2026`): 16 `Open` sessions fill the
  first listing page (they never leave it), then 20 `Aborting` ones; ≤ 16 session-leg commits per
  pass, the first pass cut at exactly 16; all 20 end.
- `e_a_completed_sessions_untrusted_entries_are_held_past_its_window` (`:1710`): the carry-forward's
  `Completed` control.
- `d_a_terminal_delete_whose_ledger_moved_is_judged_again_in_the_same_pass` (`:1305`): a Create
  moves `mpuctl` between the drainer's ledger read and its terminal batch → `Conflict`, re-judged,
  `Committed` in the same pass; count unchanged (+1 −1).

Strengthened in iteration 2: (D) checks the `obligations-first` wait and the three telemetry
counters (`:1001`); the owned-entry short-fleet controls gained a trusted residue entry that the
walk drains beside the held ones, and still hold each record exactly once a pass (`short_fleet`).

Also in the patch: `crates/core/src/multipart.rs:6625` `admission_release` unit test for
`release_one` (cargo-mutants runs only `wyrd-core`'s own tests against core mutants).

Test-only fixture change: the test profile admits 64 sessions (`w_ref` 128) so the many-sessions
leg fits.

### Demonstrated reds

Iteration 2 ran every mutation below against this same patch on this same base (`b452551`); the
patch and test are byte-identical now, so those results stand. Script and logs:
`do-evidence/{mutate.py,reds.sh,reds-final.log,reds-r3.log}`. Each mutation was an exact
one-match replacement in production code, undone with `git checkout -- crates/custodian/src
crates/core/src`.

**Re-run this iteration, through the project runner.** Iteration 2's script called
`timeout 900 cargo test -p wyrd-custodian --test multipart_session_terminal` directly. This time I
re-ran one mutation through `./engine/xtask.sh ci` instead, so the red comes from the same runner
Check uses. I picked **M3a** (the brief's "`mpuctl` decrement … not under its precondition"): only
the new file can catch it, so a red cannot come from some other test, and it leaves no unused
variable (`key` and `prior` are still read at `sessions.rs:778-796`), so clippy passes and the run
reaches the tests.

- Mutation: `crates/custodian/src/retirement/sessions.rs:772`
  `WriteBatch::new().require(key.to_vec(), prior.to_vec())` → `WriteBatch::new()`; backup of the
  one file kept in scratch.
- Result: rc 1. fmt, clippy and build passed; in `cargo test --workspace` the only failure is
  `d_two_drainers_racing_the_terminal_delete_release_the_slot_once`
  (`multipart_session_terminal.rs:1263`): "released once — left: 0, right: 1", i.e. both drainers
  decremented `mpuctl`. "test result: FAILED. 20 passed; 1 failed". Same leg and same assertion as
  iteration 2's M3a. Log extract: `do-evidence/red-m3a-ci3.log`.
- Restored from the backup; the regenerated diff of the worktree is byte-identical to
  `patch.diff` (`cmp` clean), i.e. the tree the green run above tested.

The rest of the table is iteration 2's run (script-driven `cargo test` with a 900 s timeout).

| Mutation (production code) | Red leg(s) | First failing assertion |
|---|---|---|
| **M1** the `sidx:`-empty gate removed (`Walked::Stop` falls through to the terminal delete) | `d_the_terminal_delete_waits_for_an_empty_owned_range`, `e_a_completed_sessions_untrusted_entries_are_held_past_its_window` | "round 1": session deleted with residue left |
| **M2** a `Completed` session deleted without the tombstone input | `d_a_completed_session_keeps_its_tombstone_…` + 4 E legs (their `Completed` control sessions got deleted) | "the tombstone stays" |
| **M3a** decrement not under the session precondition | `d_two_drainers_racing_the_terminal_delete_release_the_slot_once` | "released once": left 0, right 1 (two decrements) |
| **M3b** decrement committed in its own batch before the delete | race leg, ledger-moved leg, many-sessions leg | two decrements (count 0) |
| **M4** untrusted records deleted without marking by chunk id | 10 legs: E1, E2, E3, both past-one-step legs, listing-cap leg, all four fleet controls | "FragmentId { chunk: 42007, index: 0 } on 0 survived" |
| **M5** retirement by chunk id on a short fleet (gate always open) | all four fleet controls | round 0 `Changed`, expected `Blocked` |
| **M6** an E2 entry's named positions not marked (listing only) | `e2_…`, `d_owned_entries_past_one_steps_…` | "(3, FragmentId { chunk: 22007, index: 3 }) is unmarked" |
| **R1** iteration 1's part stall: the damaged part walked from item 0 | `e1_a_part_past_one_steps_reads_resumes_from_its_cursor` | "… ends within 12 passes" (never ends) |
| **R2** iteration 1's owned-walk stall: the bookmark ignored | `d_owned_entries_past_one_steps_reads_resume_from_the_walks_bookmark` | "… ends within 2 passes" |
| **R3** the `Aborting`-only gate of the owned walk bypassed | `e_a_completed_sessions_untrusted_entries_are_held_past_its_window` | round 0 `Changed`, expected `Blocked` |
| **R4** the session listing stops after its first page | `d_sessions_past_one_page_and_one_pass_of_steps_all_end` | "the first pass is cut short" |
| **R5** a walk step's commit spends no step | same | "pass 0: 40 commits" (≤ 16) |
| **R6a** listing cap off by one (`>=`) | `e_a_chunk_listed_past_one_steps_reads_is_held` | "the part that fits retired" |
| **R6b** listing cap removed | same | round 0 `Changed`, expected `Blocked` |
| **R7** a step that held something goes on (`any_held` inverted) | both owned-entry fleet controls, `d_owned_entries_past_one_steps_…` | holds per round 6, expected 2 |
| **R8** a terminal batch refused on a moved ledger is not judged again | `d_a_terminal_delete_whose_ledger_moved_is_judged_again_in_the_same_pass` | `Partial`, expected `Changed` |
| **R9** a damaged part met with no reads left reads anyway | `e1_a_part_past_one_steps_…` | "attempt to subtract with overflow" |
| **R10** the damaged part's session-record read not charged | `e1_a_part_past_one_steps_…` | "part 2's first step": 233, expected 232 |
| **R11** the owned walk's mark reads not charged | `d_owned_entries_past_one_steps_…` | "the first step reads its whole budget": 249, expected 233 |
| **R12** `AdmissionRecord::release_one` → `None` | 18 legs (every terminal delete holds `admission-ledger`) | pass 1 `Blocked` |
| **M0** the session leg not dispatched at all | 19 of 21 legs | (the 2 green are the `Open` and undecodable-`part:` safety controls, which the base already satisfies) |

On the real base the new file does not compile (no `Tombstone`, no context fields, no
`owned_walk_key`), which is why C4-verify reports this net-new file UNVERIFIABLE (the brief's
pre-declared posture). M0 is the runtime stand-in for "base behaviour".

### Refuting my own test

- **(a) Genuine red?** Yes. Every mutation in the table, each reverting one piece of the fix, turns
  at least one named leg red; R1 and R2 put back exactly iteration 1's two stalls and turn the two
  past-one-step legs red. M0 (session leg not dispatched) fails 19 of 21. This iteration re-ran
  M3a through the project's own runner (above). The tree was restored after each run.
- **(b) Production path?** Yes. Every leg drives `wyrd_custodian::reconcile_step` with a
  `ReconstructionContext` shaped as `crates/server/src/custodian.rs` builds it (fleet, configured
  `operator_fleet_size`, `Tombstone::Retain` except where a leg names `Elapsed`), and GC through its
  own `reconcile_step` pass. Sessions, aborts, completes and the race's Create are the production
  `wyrd_core` functions; the metadata store is real redb. Only the D servers are a double, as the
  brief allows.
- **(c) Fixture includes the fault?** Yes. The oversized legs plant entries and parts that really
  exceed `MARKS_PER_STEP` (256, 300, 233 + 474 positions); the listing-cap leg plants 233 real
  fragments of one chunk on the servers; the many-sessions leg has 16 sessions that never leave the
  first page; the race leg runs a real Create between the read and the commit; the fleet controls
  keep server 5 out of the fleet while it holds fragments.

## Residuals (for the human)

- **`listing-past-step` needs a human**: an `Aborting` session's untrusted chunk whose listing
  shows more than 232 fragments is held, keeping its record, its fragments and the session's
  admission slot. The index-granular alternative above would remove this.
- **A bookmark is trusted like the obligation's own cursor**: it is written only in the batch whose
  marks it counts; a planted or corrupted one naming more positions than are marked would be
  believed. One that names no entry of the session is read as absent and audited
  (`unusable-owned-walk-bookmark`). One that names an entry but counts more positions than the
  entry names is read as zero **without** an audit event (`sessions.rs:601-606`): safe, since the
  walk re-reads from the first position, but silent; only a store edit makes one.
- Unchanged from iteration 1: fragment-less walk marks stay in the `orphan:` ledger; the
  late-write residual for an untrusted `sidx:` entry; `Completed` sessions never end in production
  until #625.
- **Mutants expected to survive, and why** (equivalent at the `reconcile_step` boundary):
  `sessions.rs` `reconcile`'s `|| !read_whole_listing` (the listing is cut short only once the
  steps are spent, which needs a commit, so the leg answers `Changed` anyway; and a session's
  `Partial` is always masked by the ledger walk's verdict on the same obligation); `read_entry`'s
  `owner == *upload_id` guard (the range prefix `sidx:<id>:` already pins the owner);
  `ledger_unusable`'s `now == prior` guard (no writer changes an ended session's record other than
  deleting it); the ops/bytes checks and cost arithmetic of both walks (reads bind first at the
  shipped `MAX_BATCH_OPS`/`MAX_BATCH_BYTES`; the static assert shows 500 ops exactly);
  `MARK_MAX_BYTES`' `+` (same reason). The `emit_*` telemetry is pinned by counters in (D).

## Review finding recorded-rejected

The iteration-1 batch review's TEST-GAP (no new seeded Tier-0 DST) is out of scope by the brief
("add no DST scenario"; #660 owns the session leg's interleavings), recorded in
`review-rejected.md`, marker `// deferred: #660` at `sessions.rs:173`.

## Rubric self-review (AGENTS.md *Review rubric & protocol*)

Re-read against the rubric given with this run; nothing changed since iteration 2, and nothing in
the patch is flagged.

- **One clock per lifecycle**: no new clock read. The walk's stamp is read from `ctx.clock` once
  the page is in hand (`sessions.rs:531`), the same source the drain's other marks use.
- **Narrow seams**: only `MetadataStore` / `ChunkStore`. New `pub` items: `owned_walk_key`,
  `OWNED_WALK_PREFIX`, `LISTED_PER_CHUNK`, `Hold::ListingPastStep`.
- **Validation boundaries**: the bookmark is the custodian's own bookkeeping (like
  `LEDGER_CURSOR_KEY`); a value that does not name an entry of the session is read as absent,
  audited, and rewritten under a precondition on its exact bytes. Placement length stays a
  maintenance-path check.
- **No shared mutable global state**: none added (the `xtask statics` gate passed).
- **`#![forbid(unsafe_code)]`**: no new crate; `xtask unsafe-guard` passed.
- **Docs currency**: the new persisted key and the resume rules are in
  `docs/design/architecture/06-runtime-view.md:92` and `05-building-block-view.md:246`. The two
  new `ReconstructionContext` inputs are described there in prose, not by field name: the
  tombstone input ("an input the deployed custodian sets to keep it until `W_tombstone`…") and the
  whole-fleet condition ("only on a pass that sees every server the operator configured").
- **Absent or unsupported entries**: every case the walks cannot act on is a typed, audited hold;
  the one "no progress, nothing held" path is counted and emitted, not silent.
- **Transactions**: each step is one commit; the bookmark write is under `require`/`require_absent`
  of what was read, so two drainers cannot both move it; `Err` is `commit-failed`. The terminal
  delete's `mpuctl` decrement is in the same batch, under the session record's exact bytes.
- **Await discipline**: no new kinds of await; listings are the same `list_fragments` call as
  iteration 1.
- **Test fidelity / DST**: see the recorded rejection above.
