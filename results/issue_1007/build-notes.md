# Build notes — #1007 (636.5a), iteration 3: closing the test gaps and doc mismatches

Base: `getwyrd/wyrd` `main` at `376b106` (the worktree's HEAD). `patch.diff` applies to it. Line numbers are on the **patched** tree unless marked otherwise.

I started from `iteration-v2/patch.diff`, because the carry-forward cites its line numbers and says to rebuild it. I also read the v2 Check findings the carry-forward summarises, to get the exact edits it names.

**No production result changed.** The fence, validation and release decide exactly what they did in v2. The one code change outside comments is a new field on the in-doubt error (item 6), which adds to what the error reports.

## What the sign-off asked for, and where it is

| # | Item | Where |
|---|---|---|
| 1 | A release loses one `Conflict`, then lands; bound pinned to a written number; `RELEASE_ROUNDS = 1` fails | `a_release_that_loses_a_conflict_with_the_session_unchanged_tries_again_and_lands` (`multipart_complete.rs:1219`). `STATED_RELEASE_ROUNDS = 4` at `:91`; D4 now checks it (`:1150`). `RELEASE_ROUNDS` is no longer imported by the test. |
| 2 | "Another target" leg with a different parent | `e_another_target_answers_no_such_upload_in_every_state` (`:922`), by parent and by object, against `Open`, `Completing` and `Completed` |
| 3 | The fence loses to a later `Open` with different attempts, retries, is rejected | `a_retried_fence_that_is_rejected_restores_the_attempts_of_the_session_it_fenced` (`:1733`), with later attempts both above and below the first read. The held twin is at `:1775`. |
| 4 | Bucket first, first bad part reported | `the_first_bad_part_of_a_list_is_the_one_reported` (`:1805`), `a_deleted_bucket_answers_before_any_bad_part` (`:1855`), each before the fence and behind it. I kept the promise in the doc rather than dropping it. |
| 5 | "Whatever happens next" wording | `complete.rs:60-64`, `:538-540`, `:672`, the `ReleaseFor::Fault` doc, the `moved` doc; `05-building-block-view.md:248`. The word "whatever" is gone from all of them. The ownership paragraph (`complete.rs:44-47`) also now says a fence can be down because another writer moved it. |
| 6 | The in-doubt arm dropped the scan fault | Kept visible: `InDoubt::fault_behind_fence` (`complete.rs:293`), filled at `:914`, shown in the error's text at `:406`. Doc at `:75-80`. |
| 7 | `abort.rs` said invalid Completes move the session | `abort.rs:52-56`: only one that fenced and was then rejected or faulted behind its fence moves it; a list already invalid commits nothing. |

## Decisions left to me

**Item 6: I kept the fault rather than correcting the doc.** The sign-off allowed either. Correcting the doc costs 2 lines but leaves an operator with an in-doubt error that gives no reason a release was tried. Keeping it costs 26 lines of `complete.rs`, doc comments included: the field (5), the `Display` match (6), `into_fault` (9), the parameter and its use in `in_doubt` (2), and the two call sites (4). The source chain is unchanged, so `classify` still reads `Indeterminate`; `a_fault_behind_the_fence_with_an_unknown_release_is_in_doubt` (`:2319`) goes through a helper that asserts this.

**What still drops the fault.** An answer has nowhere to carry it, so a fault whose release finds a session with an answer of its own loses it. The docs now say that plainly (`complete.rs:75-80`, `05-building-block-view.md:248`). Unchanged from v2: when a fault's release also fails, the error keeps the fault and drops the release's own failure.

**Gaps the sign-off did not list, closed anyway.** The carry-forward's pattern was edits `cargo mutants` never tries: constants, a removed `||` half, reordered reads. So I went looking for more of that shape, and found these passing v2's tests:

- `FENCE_ROUNDS` was compared to itself, same as the release bound. Pinned to `STATED_FENCE_ROUNDS = 4` (`:93`), including the bound's last round succeeding (`:1466`).
- The cap check `>=` could become `==`: nothing tested `attempts` above the cap. Added to `:1407`.
- The doc promises an order of checks on an `Open` session (list order, clock, budget, epoch ceiling, then parts), and nothing tested it: `an_open_sessions_checks_run_in_the_documented_order` (`:1897`). The epoch ceiling was missing from that doc, so I added it (`complete.rs` `fence_complete` doc, "Before the fence").
- The held fence could carry the first read's part set instead of the one behind the fence: `the_held_fence_carries_the_set_read_behind_the_fence` (`:1970`).
- A fault's release passing no fingerprint would answer a tombstone wrongly: extra legs in `:2355` (a rival's fence, a tombstone by own list and by another list).
- The "Termination" doc said one validation per fence commit. There is one more when the rounds are spent. Doc corrected (`complete.rs:545`), code left alone.

**Not pinned on purpose.** No test says the held marker must be the bytes read behind the fence. That would pin behaviour on the deleted-and-recreated bucket question, which you said is still open.

## Demonstrated reds

Each is a temporary edit to `complete.rs` or `multipart.rs`, applied by script, run through `cargo test -p wyrd-core --test multipart_complete --no-fail-fast`, then restored and checked with `cmp`. **52 edits, all red.** On the first pass 9 did not compile, because the repo denies warnings. I do not count a build failure as a red, so I fixed those 9 edits so they compile and re-ran them (R3, R9, R13, R19, N3b, N4c, N6, X6, X7). These ran before the final citation-only comment change; nothing that can affect a result changed after.

**The brief's required list:**

| # | Temporary edit | Named leg that went red |
|---|---|---|
| R1 | Rejected request counted against `attempts` | **D1** `d_typos_…` (12 tests in all) |
| R2 | Pre-check kept, `attempts` not restored after the fence | **D2** `d_typos_…` (12 tests) |
| R3 | Session left `Completing` after a rejection | **D's Abort leg** `d_after_a_rejected_request_an_abort_fences` (15 tests) |
| R4 | Meeting another publisher's `Completing` releases it | **(E)** `e_another_publishers_…` (4 tests) |
| R5a | Definite release failure answered with its refusal | **D4** |
| R5b | The same, reported in doubt | **D4**, plus the fault-stands test |
| R6 | Unknown initial fence treated as success | **G0-fence** |

**The edits this sign-off named:**

| # | Temporary edit | Leg that went red |
|---|---|---|
| N1 | `RELEASE_ROUNDS = 1` (item 1) | `a_release_that_loses_a_conflict_…`, D4 |
| N1b, N1c | `RELEASE_ROUNDS = 3`, `= 5` | `…loses_a_conflict…` and D4; D4 |
| N1d | A rejection's release gets one round | `…loses_a_conflict…`, D4 |
| N2 | Dropping `session.parent() != request.parent` (item 2) | `e_another_target_…` |
| N3 | Restoring attempts from the first read (item 3) | `a_retried_fence_that_is_rejected_…` |
| N4a | List walked in reverse (item 4) | `the_first_bad_part_…` |
| N4b | Marker read after the part set (item 4) | `a_deleted_bucket_…`, "the parts are not read" |
| N4c | Bucket checked after the parts (item 4) | `a_deleted_bucket_…` |
| N5 | In-doubt error drops the fault (item 6) | `a_fault_…_with_an_unknown_release_…` |
| N5b | Error text omits the fault (item 6) | same test |

**The rest, from v2's list and from my own search.** All red:

- **From v2:** R7 `fence_released(0)`; R8 constant charge; R9 `attempts_before` always 0; R10 a typo fences; R11 one page only; R12 wrong cursor; R13 no release on a fault; R14, R15 wrong `FenceState`; R16 a fault's release retries; R17 a failed re-read claims `Stands`; R18 a fault's error reports the release's failure; R19 order not checked before the fence; R20 order before state.
- **New, target check:** N2b object half dropped; N2c target checked only on `Open`.
- **New, retries and bounds:** N3b `attempts_before` from the first read; N1e, N1f `FENCE_ROUNDS` 3 and 5; N7 cap `==`.
- **New, frozen set and order:** N6 first read's set handed over; N8a–d each order swap.
- **New, batches and walk:** X1, X2 no `require` on the fence or release; X3 moved session not noticed; X4, X5 release loses the fingerprint; X6, X7 walk accepts a foreign or unordered key; X8 moved `Open` reported `Released`; X9 page limit of one.

**Reverted base.** I removed `pub mod complete;` and ran the target. It does not build, and it stops in the library before it reaches the test: `error: methods complete_fence and fence_released are never used`. That was a partial revert. With the whole patch reverted, the test cannot find the module, which v2's gate recorded as E0432. Either way C4-verify will report **UNVERIFIABLE (exit 77)**, as the brief pre-declares.

## How I ran the checks

- `./engine/xtask.sh ci` (the full project gate, DST included): exit 0, `xtask ci: all checks passed`. Run before the citation fix below.
- `scripts/mutants-in-diff`, the C5 gate's own script: **72 mutants tested in 3m: 47 caught, 25 unviable, 0 missed.** Also run before the citation fix. A clean row here only means none of the edits `cargo mutants` tried survived: v2 also had zero missed and still had the gaps you named. The 52 hand edits above are the evidence for that class.
- `cargo fmt --all -- --check`, `cargo clippy -p wyrd-core --tests`, `typos` on every changed file, `python3 docs/publishing/tools/lint_docs.py`, `git diff --check`: all clean.
- Test target: **43 passed**, up from 35.
- After those runs I changed one thing: the citation `abort.rs:249-250` to `:249-251` in two comments, since my `abort.rs` edit moved those lines. I re-ran `cargo fmt --check` and the test target after it, but not the full gate or the mutation run. It touches comments only.

## Refuting my own test

- **(a) Genuine red?** Yes. Fully reverted the target cannot build, which proves little by itself. With each defect put back one at a time, the named leg fails at the assertion that names the property (52 edits above).
- **(b) Production path?** Yes. Every leg calls the production `fence_complete` over `RedbMetadataStore::in_memory()`, through a wrapper that only delegates, withholds or scripts a store's answers. Sessions come from the production `create_session`. Scripted `scan_page` answers stand in for a backend breaking its contract, the one thing redb will not do on request, and they feed the production `read_part_set`.
- **(c) Fixture includes the fault?** Yes. The new legs use a real conflict from a real session moved in redb (item 3), real replaced parts and a really deleted bucket (item 4), and a scripted `Conflict` with the session genuinely unchanged (item 1), which is the TiKV case the brief names and which redb will not produce on its own.

## For the human at sign-off

**Size.** `patch.diff` is about 176 KB, up from 154 KB, so the size backstop will fire again. You told me not to split. The test file went from 2,160 to 2,596 lines, `complete.rs` from 951 to 984.

**Still open from v2's Check, not touched here because the carry-forward did not ask and said to keep the logic:**

- The deleted-and-recreated bucket question, which you said is still with you.
- `complete.rs` reads only `psum:`, while the brief's Scope says "`psum:` / `part:` ranges". Deleting a `part:` record and leaving its `psum:` still returns `Held`. The module doc gives the reason (chunk lists can reach tens of megabytes), and a part commit writes both in one batch. Please confirm it; the next slice must error on a missing `part:` record.
- The stamp `fenced_at_millis` is fixed before the reads and retries, so a slow pre-check eats into `W_completing`. Fixing it needs a clock seam, which is a logic change.
- The fence loop runs one more full validation when its rounds are spent (79 extra page reads at 10,000 parts). Moving the check is a logic change: it could alter the answer when a list turns invalid at that moment. I corrected the doc and left the code.

**Open question from the brief (visibility).** Unchanged: `fence_complete`, `HeldFence` and the error types are `pub` so the test can drive them.

## Not done here (by the brief's scope)

Assembly and publication (#1008, #1009); the release's `retire:records:{seg}` obligation and the charging rule for fences that wrote segments (#1009); re-reads that settle an unknown outcome (#1010); the value ceiling (#1011); the reaper (#625), drain (#659), S3 verb (#670), DST (#660, marked in code). No edit to 0016, an ADR, `Cargo.lock` or `deny*.toml`.
