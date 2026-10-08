# Build notes — #842 (809.4), iteration 5: restore fences resurrected `Completing` sessions

Base: `pdca-integration/r-a834e98f67b952c61db7a03084c626e8/main` @ `022d76f` (origin/main +
child-1 #839, child-2 #840, child-3 #841; the bundle's `stack-base`). Line numbers below are on
that base **with `patch.diff` applied** (the worktree at `$PDCA_WORKTREE`).

**What I read besides `brief.md`.** The carry-forward findings cite lines of the previous attempt's
patch (`restore.rs:945`, `tests/restore_completing_fence.rs:608`, …), so I applied
`iteration-v4/patch.diff` to the clean worktree as the starting point and read
`iteration-v4/build-notes.md` and `session-carry-forward` (this bundle's own earlier output). I
read no reviewer file (`check-*`, `SUMMARY.md`) and no other bundle.

The fence itself is unchanged from iteration 4. This iteration fixes the one blocking finding and
pays for it inside the size budget. "What the patch does" is at the end.

## Carry-forward: what each finding got

### 1. A missing records obligation made a re-run report clean (T4 blocking, C5, adversary, code review — all one line)

`recheck_fenced` returned `Ok(())` as soon as `retire:records:s:<id>:<E'-1>` was absent, without
looking at the session's own `seg:<nonce>:<E'-1>:` range. So an `Aborting@4` session with leftover
segment records and no deleter (X57 open again) got a clean verdict, and so did a session whose
bad obligation an operator had deleted after the first run named it.

Fix (`crates/custodian/src/restore.rs:949-955`): on an absent key, read one `staged_page` of
`seg_range_prefix(group)`. If it holds anything, name the session at its **first** record with the
new `SegmentFault::NoDeleter` (`restore.rs:262-263`, Display `:276`: "no `retire:records:`
obligation is there to delete it"). The missing deleter is reported whether or not the records
decode, as the code review asked. Seven production lines plus the variant and its Display arm.

No false alarms that I can find:
- A session fenced from `Open@E` never wrote a segment at `E` (a segment write requires
  `Completing@E`), so its range at `E` is empty. Test controls: `f9` (fenced from `Open` by the
  first pass) and `fe` (seeded `Aborting@4` with nothing else) stay unnamed on all three passes.
- A #659 drain that empties the range before it drops the key stays unnamed. One that drops the
  key first is named; the `// deferred: #659` marker now says so (`restore.rs:932-934`).

**This reverses a decision from iteration 4**, which declined this case as outside the brief and
too costly for the budget ("roughly 1 KB of diff, against 428 bytes left"). The reviewers were
right that the brief's K fact says who files the key, not that a missing key means no attempt.
The doc comment that argued the converse is rewritten (`restore.rs:928-931`: "Its absence proves
nothing (a damaged or hand-repaired store): the range must be empty").

Docs, as asked: §6.5 (`docs/design/architecture/06-runtime-view.md:65`) and the blueprint's
SEGMENTS bill (`docs/design/architecture/m4-first-deployment-blueprint.md:633-638`) now list a
missing obligation with records remaining. `UnaccountedSegments`' doc says it too
(`restore.rs:230-232`).

Regression, in leg K (`crates/custodian/tests/restore_completing_fence.rs:573-651`):

- **A1** — `fd` (`:597`): `Aborting@4`, two parts, `seg:<fd>:3:000000` valid, `:000001` =
  `not a segment`, **no** obligation. Its session key and its **first** segment key must be named
  on pass 1 and pass 2 (`:628-633`).
- **A2** — the operator flow. `fa`, `fb`, `fc` now each have a real attempt (two parts, two
  segments; `fb`'s and `fc`'s ranges are fully clean, `fa`'s first segment is bad). Passes 1 and 2
  name each bad obligation as before. The test then deletes all the named obligations (`:635-640`)
  and runs a **third** pass (`:641`): all four sessions must be named at their first `seg:` record
  (`:643-647`).
- **Clean control** — `fe` (`Aborting@4`, no obligation, no segment), `f9` (fenced from `Open`)
  and `f7` (cleanly fenced `Completing`) are unnamed on every pass, and every pass still
  `needs_human()` (`:648-651`).

### 2. C4 diff coverage "patch.diff does not apply on origin/main"

Unchanged, and not fixable in the patch: `run-diff-cov.sh` resolves its base to `origin/main`, but
this bundle is stacked on child-1..3. `run-verify.sh` got the integration base through `PDCA_BASE`;
the diff-cov row does not seem to get the same export. That is gate wiring.

## Size budget (brief: at most 7 files, under 80 KB)

7 files. `patch.diff` is **81,892 bytes = 79.97 KB** at 1,024 bytes per KB (how the harness's size
signal divides). That leaves 28 bytes. In decimal it is 81.9 kB, as iterations 2–4 also were.

The fix, its three test cases and the doc lines added about 2.5 KB to iteration 4's 81,492 bytes.
What I cut to pay for it (no leg, case or assertion from earlier rounds was dropped):

| Cut | Saved (approx.) |
|---|---|
| Core unit test moved from its own `mod completing_teardown` into the existing `mod open_teardown` as a second `#[test]` (`crates/core/src/multipart.rs:5194-5242`): no new module header, no `pub(super)` on `session`, one hunk fewer | 490 B |
| The store double enforces `MAX_VALUE_BYTES` on **every** commit, not only in G-sparse (`restore_completing_fence.rs:118-121`): the `ceiling` field and G-sparse's special construction are gone. Every leg now runs against the real ceiling, which is closer to FoundationDB | 230 B |
| Audit capture: a plain `static AUDIT` (`:336`) instead of a `OnceLock` behind a function, and the subscriber installed inline in the one leg that uses it (`:659-660`) | 420 B |
| K's two tables now share one shape `(UploadId, record to name)`, so one loop checks both (`:628-633`); a shared `aborting()` helper (`:207`) | 250 B |
| Child-3's test doc line reworded to change one line instead of two (`crates/custodian/tests/restore_open_fence.rs:665`) | 170 B |
| Shorter wording: K's doc, `names`' doc, one CLI test comment, one core doc clause ("pinned by …") | 330 B |

Trade-offs the human may want to weigh:

1. **The core test's home.** `only_a_completing_session_has_one_and_both_obligations_match_their_keys`
   now sits in a module called `open_teardown`. It has its own doc comment, and cargo-mutants
   does not care where it lives, but the module name is a little off. Giving it its own module
   again costs about 490 bytes, which the budget does not have.
2. **The CLI paragraph does not say how to get out of `NoDeleter`.** It still reads "inspect the
   named record before one runs, then re-run this pass" (`crates/server/src/cli.rs:1409-1413`).
   Each named session carries its fault text, so the operator sees "no `retire:records:`
   obligation is there to delete it". What it does not say is the repair: put back an obligation
   owing that range, or remove the segment records by hand once their chunks are accounted for.
   I wrote a sentence for it and took it out again for size (about 130 bytes). I think the right
   home is #659's runbook, since the drain is what makes that obligation matter. If the human
   wants it here, it is one sentence in the CLI text and one in the blueprint.
3. **The third pass does not re-check that the store is unchanged.** `recheck_fenced` has no write
   path, and K still proves the second pass writes nothing (`:627`).

## Red → green, through the project's runner

`PDCA_BUNDLE=results/issue_842 PDCA_LANE=1 PDCA_BASE=pdca-integration/r-a834e98f67b952c61db7a03084c626e8/main ./engine/scripts/run-verify.sh`
on the final `patch.diff`:

- GREEN (fix applied): `test result: ok. 7 passed; 0 failed`.
- RED (production reverted, test kept): `test result: FAILED. 0 passed; 7 failed`.
  **7 of 7 ran red, all by assertion; none failed to compile.** G, G-sparse, H and K panic at
  `:296` (`assert_aborted`: the session is still `Completing@3`); G-collision at `:455` (the base
  names `cause: Completing`, not "key taken"); G-atomic at `:410` and Order at `:694` (the base
  returns `Ok` where the fix returns the commit's `Err`).
- `run-verify.sh: PASS — red without the fix, green with it (7 test(s) ran red).`

As the brief predicted, H(i) and H(vi) already hold on the base; leg H still goes red there
because its other cases are not fenced.

## Refuting my own test

- **(a) Genuine red?** Yes. `run-verify.sh` reverted every production file and kept the test: 7 of
  7 failed on assertions. For the new branch specifically, three mutants were each run through
  `run-verify.sh` as a scratch bundle (the full patch with one change in `recheck_fenced`); the
  script's GREEN leg failed each time, 6 passed / 1 failed, the failure in leg K:

  | Mutant of the absent-obligation branch (`restore.rs:949-955`) | Where K fails |
  |---|---|
  | Never name (`left.first().filter(\|_\| false)`) — iteration 4's behaviour | `:631`, `mpu:fdfd…` not named |
  | Fall back to `check_attempt` instead of naming the missing deleter | `:631`, `seg:fdfd…:3:000000` not named (the mutant names the bad second record instead) |
  | Name the missing deleter only when some record in the range is also faulty | `:646`, `fbfb…` not named on the third pass (its range is clean) |

  The third mutant passes the first two passes and dies only in the third, so the third pass is
  not decoration.
- **(b) Production path?** Yes. Every leg calls `wyrd_custodian::reconcile_after_restore`
  (`restore_completing_fence.rs:179`), which runs the real `fence_session` → `plan_fence` →
  `SessionRecord::completing_teardown` → `MetadataStore::commit`, then `check_attempt` /
  `part_chunks`, and on a re-run the real `recheck_fenced`. Only the store and the disks are
  doubles. The core unit test calls `SessionRecord::completing_teardown` directly.
- **(c) Fixture includes the fault?** Yes. `fd` really has no `retire:records:` key and really has
  segment records under its own range. After the test deletes the obligations, `fb` and `fc`
  really hold two valid segment records each, over parts that hold their chunks, and nothing owes
  them. The control `fe` really has neither. The store really refuses an oversized value: G-sparse
  first commits a `MAX_VALUE_BYTES + 1` probe and asserts the refusal (`:469-471`).

Mutants from earlier rounds (the foreign-group check, first-chunk-only, require-absent on one key
only, single-page reads, `{session, parts}` overflow, fencing without the `seg:` obligation) were
not re-run. The production code they target did not change this round, and the test cases that
killed them are still there: `fa`/`fb`/`fc` on passes 1–2, H(ii)'s two-chunk segment, both
G-collision arms, the "paged" case, and G-sparse's 10,000 parts.

## Gates and commit-readiness

- `cargo fmt --all`: applied; `cargo fmt --all -- --check` clean inside CI.
- `cargo clippy -p wyrd-core -p wyrd-custodian -p wyrd-server --all-targets -- -D warnings`: clean.
- `./engine/xtask.sh ci` (= `cargo xtask ci` in the worktree, on the final tree):
  `xtask ci: all checks passed`, exit 0. Every stage ran: `typos`, `lint_docs`, `render_site
  --check` (99 pages, link audit OK), the gitlink / unsafe / blackbox guards, fmt, clippy, build,
  workspace tests, `cargo-machete`, three `cargo deny` runs, the ADR-0035 statics gate,
  deploy-guard, and `wyrd-dst` clippy + tests under `--cfg madsim`. Both external tools the brief
  names (`typos`, `docs-renderer`) were present and ran, so there is no NEEDS-HUMAN external
  dependency.
- The tree and `patch.diff` are byte-identical (`git diff | cmp - patch.diff`), and
  `git diff --check` reports no whitespace errors.

## Known neighbours, not done here

- **An `Aborting` session with `part:` records but no `retire:bytes:` obligation** is not named.
  It is the same shape of gap as finding 1, but on the bytes key, which child-3's fence owns and
  the brief puts out of scope ("report fields beyond what G/H need"). Nothing is wrongly deleted
  (the staged records still protect the chunks); the records just have no deleter. A reviewer may
  raise it. I suggest #659.
- **Attempts older than `E'-1`.** The re-check reads only the session's range at `E'-1`. An
  earlier attempt's range belongs to the rollback that left it (`0016:663`), which is #656's door.
- 0016's reaper and operator-abort `Completing → Aborting` rows (`0016:665`, `:2193`) still
  specify `{session, parts}` and hit the same 100,000-byte ceiling. Flag to #656 and #659. No
  edit to 0016 or to any ADR here. #810 should depend on this child, not on #809.

## What the patch does (unchanged from iteration 4 apart from the above)

- **Core** (`crates/core/src/multipart.rs`): `SessionRecord::completing_teardown` (`:2291`) mints,
  from a decoded `Completing@E` record, the record at `Aborting@E+1`, `retire:bytes:s:<id>:<E>` =
  `{session, all}`, and `retire:records:s:<id>:<E>` = `{seg: (nonce, E)}` from the private
  `RetireObligation::attempt_segments` (`:3665`), which takes the token epoch from the group so
  key and payload always pass `checked_against_key`. `None` below `Completing` or at `u64::MAX`
  (`checked_add`): no wrap, no panic. Per the human's Plan decision (option a), `{session, all}`
  replaces `{session, parts}`; the retire rows table gives the reason.
- **Custodian** (`crates/custodian/src/restore.rs`): `plan_fence` (`:886`) sends `Open` → one
  obligation, `Completing` → both obligations plus the attempt to check, `Aborting@E'` → re-check
  of its own group at `E'-1`, `Completed` / `Aborting@0` → nothing, and no next epoch →
  `EpochExhausted`. `fence_session` (`:801`) builds ONE batch: `require(mpu, bytes read)`,
  `require_absent` on every obligation key, the session put, every obligation put. On `Conflict`
  it re-reads each obligation key and names the first one taken. After the commit lands,
  `check_attempt` (`:971`) pages the frozen `seg:<nonce>:<E>:` range and, only if it holds
  anything, the session's `part:` range (`part_chunks`, `:1019`). The first faulty record names
  the session in `RestoreReport::segments_unaccounted`. `recheck_fenced` (`:935`) is the K
  re-check: obligation owing the own group → check the range; owing anything else or undecodable
  → name the obligation; absent → the range must be empty.
- **CLI** (`crates/server/src/cli.rs`): the summary counts the new list, the NEEDS-HUMAN paragraph
  is at `:1402-1417`, `unsettled_causes` loses the `Completing` slot, and the agreement test covers
  the new report field.
- **Child-3's test** (`crates/custodian/tests/restore_open_fence.rs:675`): its "cannot be fenced"
  `Completing@3` fixture is now `Completing@u64::MAX`, since a plain `Completing@3` is fenced now.

## Scratch

This run's verify, mutant and CI logs are under `$PDCA_SCRATCH/pdca-builder-842-i5/` (about
550 KB: logs, three mutant patches and one saved copy of `restore.rs`; no build cache). The lines
that matter are quoted above. I tried to delete that directory at the end, as the scratch rule
asks; the sandbox refused the `rm`, and I did not try another way. It is still there for the
harness to reclaim. Earlier iterations left `$PDCA_SCRATCH/pdca-builder-842-*` files of about
1 MB in total; I did not create them in this run and left them alone.
