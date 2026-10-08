# Build notes — #842 (809.4), iteration 3: restore fences resurrected `Completing` sessions

Base: `pdca-integration/r-a834e98f67b952c61db7a03084c626e8/main` @ `022d76f` (origin/main + child-1
#839, child-2 #840, child-3 #841, plus unrelated integrated slices). Line numbers below are on that
base **with `patch.diff` applied** (the worktree at `$PDCA_WORKTREE`).

This iteration starts from iteration 2's patch (`iteration-v2/patch.diff`, which applied cleanly on
the same base) and addresses the four carry-forward findings. The design is unchanged; see
"What the patch does" below for the whole of it.

## Carry-forward: what each finding got

1. **`seg:` paging in `check_attempt` was never exercised** (`crates/custodian/src/restore.rs:981`).
   New H case `"paged"` (`crates/custodian/tests/restore_completing_fence.rs:549`): a `Completing@3`
   session with 513 `seg:` records, all naming a held chunk except index 512, which will not
   decode. `STAGED_PAGE` is 512 (`crates/custodian/src/gc.rs:307`), so the bad record is only on
   page two. The case runs in leg H (fenced, record named, `needs_human()`) and in leg K (named
   again on the second pass, which goes through `recheck_fenced` → `check_attempt`).
   Mutant `match (next.filter(|_| false), page.last())` → **H and K go red** (5 passed, 2 failed).

2. **`part:` paging in `part_chunks` was never exercised** (`restore.rs:1019`). G-sparse's segment 1
   now names the chunk of part 19,999 instead of part 3 (`restore_completing_fence.rs:498`), so a
   pass that read only the first `part:` page would call it `ChunkInNoPart` and name the clean
   session. Mutant "first `part:` page only" → **G-sparse goes red** (6 passed, 1 failed).

3. **The undecodable `retire:records:s:<id>:<E-1>` arm of `recheck_fenced` had no test**
   (`restore.rs:949-955`). Leg K now also seeds an `Aborting@4` session `fb…` beside
   `retire:records:s:<fb…>:3` = `not json` (`restore_completing_fence.rs:608-616`) and asserts both
   the session key and that obligation key are named on both passes (`:629-636`). It is not counted
   as fenced (`sessions_fenced` stays at 6 = 4 H cases + clean + `Open` control). Mutant
   `Err(_) => Ok(())` → **K goes red** (6 passed, 1 failed).
   The reviewer's second point here, about wording, is fixed too. The CLI NEEDS-HUMAN paragraph
   (`crates/server/src/cli.rs:1402-1417`) used to say every named record was a *segment record*.
   It now says the first record at fault is either a `seg:` record (naming a chunk no part holds,
   not decoding, or under a stray key) or the `retire:records:` obligation itself, not decoding.
   The same correction is in the audit message (`restore.rs:1486-1496`), the report field docs
   (`restore.rs:225-240`, `UnaccountedSegments::record` now says "a `seg:` record, or the
   `retire:records:` one"), the m4 blueprint's SEGMENTS bill
   (`docs/design/architecture/m4-first-deployment-blueprint.md:633-638`) and §6.5
   (`docs/design/architecture/06-runtime-view.md:65`).

4. **`completing_teardown` had no unit test in `wyrd-core`**, so C5 (which runs only the core
   suite for a core file) reported `multipart.rs:2300:17` (delete `state` field) as MISSED.
   New `mod completing_teardown` (`crates/core/src/multipart.rs:5200`), the twin of
   `mod open_teardown`: `Completing@3` → the record at `Aborting@4`; both obligations' keys equal
   `retire_key(mode, Session{id, 3})` and decode against their own payloads; bytes = `{session,
   all}`; records = exactly `{seg: (nonce, 3)}`; `None` for `Completing@u64::MAX`, `Open`,
   `Aborting` and `Completed`. It reuses `open_teardown::session`, made `pub(super)` (`:5147`).
   Mutant "delete the `state` field" → **the core test goes red**.

Also from the iteration-2 C5 run: the surviving `&&`→`||` mutant at the old `restore.rs:1001`.
Both reviewers called it equivalent. They were right: the scan prefix is the group's own
`seg:<nonce>:<E>:` (`crates/core/src/metadata.rs:1517-1519`), so any key there that
`parse_seg_key` accepts (`metadata.rs:1539-1567`) already has that nonce and epoch. I removed
the redundant guard. `segment_fault` (`restore.rs:991`) now only asks whether the key parses,
with a one-line comment saying why, and it no longer takes the `group` argument. That removes
the equivalent mutant at its source instead of leaving a permanent C5 survivor.

## What the patch does (unchanged from iteration 2 apart from the above)

- **Core** (`crates/core/src/multipart.rs`): `SessionRecord::completing_teardown` (`:2291`) mints,
  from a decoded `Completing@E` record, the record at `Aborting@E+1`, `retire:bytes:s:<id>:<E>` =
  `{session, all}` (child-3's `RetireObligation::session_teardown`), and `retire:records:s:<id>:<E>`
  = `{seg: (nonce, E)}` from the private `RetireObligation::attempt_segments` (`:3666`). That
  constructor takes the token epoch from the group, so key and payload always pass
  `checked_against_key`. It returns `None` below `Completing` or at `u64::MAX` (`checked_add`): no
  wrap, no panic. Per the human's Plan decision (option a), `{session, all}` replaces
  `{session, parts}`. The retire rows table (`:3301`) now gives the reason in full: no part commits
  outside `Open` (`upload_part_answer`, `0016:1030`), and 10,000 sparse part numbers encode to
  128,916 bytes, past `MAX_VALUE_BYTES`. The other docs that said "the `Open` teardown is the only
  writer route" are corrected.
- **Custodian** (`crates/custodian/src/restore.rs`): `plan_fence` (`:879`) sends `Open` → one
  obligation, `Completing` → both obligations plus the attempt to check, `Aborting` → re-check,
  `Completed` → nothing, and a missing next epoch → `EpochExhausted`. `fence_session` (`:811`)
  builds ONE batch: `require(mpu, bytes read)`, `require_absent` on every obligation key, the
  session put, every obligation put. On `Conflict`, it re-reads each obligation key and names the
  first one taken. After the commit lands, `check_attempt` (`:961`) pages the frozen
  `seg:<nonce>:<E>:` range and, only if that range holds anything, the session's `part:` range
  (`part_chunks`, `:1009`). The first faulty record names the session in
  `RestoreReport::segments_unaccounted` (`:227`). `recheck_fenced` (`:923`) does the K re-check off
  `retire:records:s:<id>:<E'-1>`, with a `// deferred: #659` marker at `:921`.
  `SessionUnsettled::Completing` is gone.
- **CLI** (`crates/server/src/cli.rs`): the summary counts the new list, the NEEDS-HUMAN paragraph
  is at `:1402`, `unsettled_causes` loses the `Completing` slot, and the agreement test covers the
  new report field.
- **Child-3's test** (`crates/custodian/tests/restore_open_fence.rs:675`): its "cannot be fenced"
  `Completing@3` fixture is now `Completing@u64::MAX`. A plain `Completing@3` is fenced now, so it
  could no longer stand for an unfenceable session.

## Size budget (brief: at most 7 files, under 80 KB)

7 files. `patch.diff` is **81,670 bytes = 79.76 KB** (bytes/1024, as `size_signal.py:356` counts
it). Adding the four fixes took iteration 2's 81,791-byte patch to 86,462 bytes, so I cut about
4.8 KB without dropping any leg or assertion:

- Shorter leg doc comments in the new test, the `Bytes::from_static` literal in G-collision, and
  one G-collision check folded into a stronger one. The old "no applied commit wrote a key
  containing `c1`" check is redundant: the session is still `Completing@3`, there is no bytes key,
  and the records key is byte-identical. Instead, the cause check now binds the session to its
  cause: `"mpu:c1…", cause: ObligationKeyTaken { key: "retire:records:s:c1…:3" }`.
- Shorter rustdoc in `restore.rs` and `multipart.rs`. The `{session, all}` reason now lives once,
  in the rows table, and `completing_teardown`'s doc points there.
- **Reverted iteration 2's rename** `fence_open_sessions` → `fence_live_sessions`. It cost two
  diff hunks (about 1.3 KB) for a private function whose doc already says "fence each session".
  The name now under-describes it (it also fences `Completing`). That was a deliberate trade for
  the budget; a reviewer may want the rename back if the budget can carry it.
- Also reverted two rustdoc lines on `reconcile_after_restore`: the heading "Every session the
  image held `Open` is fenced" and "Then, last, every session the image holds `Open` is fenced".
  Both are still true. The paragraph right under the heading now says `Completing` sessions are
  fenced too.

That leaves about 250 bytes of margin. **Any review round that adds code will cross 80 KB.** If
that happens, the cheapest further cut I know of is the `Completed` arm of the core unit test
(about 400 bytes). It adds no kill power, because `attempt_segment_group` returns `None` for
`Open`, `Aborting` and `Completed` from one match arm (`multipart.rs:2233`).

## Red → green, through the project's runner

`PDCA_BUNDLE=results/issue_842 PDCA_LANE=1 PDCA_BASE=pdca-integration/r-a834e98f…/main
./engine/scripts/run-verify.sh`:

- GREEN (fix applied): `running 7 tests … 7 passed`.
- RED (production reverted, test kept): `running 7 tests … 0 passed; 7 failed`. **7 of 7 ran
  red, all by assertion; none failed to compile.** Every `Completing` session is still
  `Completing@3` and named `cause: Completing`, and G-atomic and Order get `Ok` instead of `Err`.
- `run-verify.sh: PASS — red without the fix, green with it (7 test(s) ran red).`

## Refuting my own test

- **(a) Genuine red?** Yes. `run-verify.sh` reverted every production file and kept the test:
  7 of 7 failed on assertions (output above). Each new case was also checked against the mutant
  it exists for (table below), and each went red.
- **(b) Production path?** Yes. Every leg calls `wyrd_custodian::reconcile_after_restore`
  (`restore_completing_fence.rs:185`), which runs the real `fence_session` →
  `SessionRecord::completing_teardown` → `MetadataStore::commit`, and then the real
  `check_attempt` / `part_chunks` / `recheck_fenced`. Only the store and the disks are doubles.
  The core unit test calls `SessionRecord::completing_teardown` directly.
- **(c) Fixture includes the fault?** Yes. The paged case really puts its bad record on page two
  (index 512 of 513). G-sparse really holds 10,000 parts, with the checked chunk on the last
  `part:` page, and its store really refuses an oversized value (the leg first commits a
  `MAX_VALUE_BYTES + 1` probe and asserts the refusal). K's torn obligation is a real `not json`
  value under the exact key `recheck_fenced` reads. G-atomic really fails the commit for each of
  the three keys, and G-collision seeds a real, decodable foreign `{seg}`.

### Mutants (each applied to the worktree, test run, file restored; `git diff` re-checked byte-identical to `patch.diff` afterwards)

| Mutant (production) | Result |
|---|---|
| `check_attempt` stops after the first `seg:` page | H, K red |
| `part_chunks` reads only the first `part:` page | G-sparse red |
| `recheck_fenced`: undecodable records obligation → `Ok(())` | K red |
| `completing_teardown` drops `state: Aborting` | core `completing_teardown` test red |
| Blind put of the records obligation (`require_absent` on the bytes key only) — brief SELF-TEST | G-collision red |
| Fence `Completing` without the `seg:` obligation — brief SELF-TEST | 6 of 7 red |

The third SELF-TEST mutant (bytes obligation as `{session, parts}` over the 10,000 sparse parts)
was run in iteration 2 and turned G-sparse red (`value_too_large: Some(128916)`). Its code path and
the ceiling double did not change in this iteration, so I did not rebuild that mutant.

## Gates and commit-readiness

- `cargo fmt --all` applied. `cargo clippy -p wyrd-core -p wyrd-custodian -p wyrd-server
  --all-targets` is clean.
- `./engine/xtask.sh ci` (= `cargo xtask ci` in the worktree): **passed**, `xtask ci: all checks passed`, exit 0. That run includes `typos`, `lint_docs`, `render_site --check` (99 pages, link audit OK) and the ADR-0035 statics gate, so both external tools the brief names were present and actually ran.
- **C4 diff coverage could not be measured in iteration 2, and the cause is in the gate setup,
  not the patch.** `run-diff-cov.sh` takes its base from `run-verify.sh --print-base`
  (`engine/scripts/run-diff-cov.sh:685`). In the Check run, that resolved to `origin/main`, where
  a stacked child's patch cannot apply (`coverage/diff-cov.json`: "patch.diff does not apply on
  origin/main"). C4-verify got the integration branch, so it ran. The diff-cov row did not, so
  `$PDCA_VERIFY_BASE` (or `$PDCA_BASE`) apparently is not exported to it. That is a harness/config
  issue to raise with whoever owns the gate wiring. It is not fixable in this patch.

## Downstream (not done here, per the brief)

0016's reaper and operator-abort `Completing → Aborting` rows (`0016:665`, `:2193`) still specify
`{session, parts}` and hit the same 100,000-byte ceiling for a sparse session. Flag that to #656
and #659. No edit to 0016 or to any ADR. #810 should depend on this child, not on #809.

## Scratch

Logs and the mutant scripts are under `$PDCA_SCRATCH/pdca-builder-842-verify/`. I did not delete
them; the harness owns cleanup of that root.
