# Build notes — #842 (809.4), iteration 7: restore fences resurrected `Completing` sessions

Base: `origin/pdca-integration/r-a834e98f67b952c61db7a03084c626e8/main` @ `022d76f` (origin/main +
child-1 #839, child-2 #840, child-3 #841; the bundle's `stack-base`). Line numbers below are on
that base **with `patch.diff` applied** (the worktree at `$PDCA_WORKTREE`).

**What I read besides `brief.md`.** The carry-forward findings cite lines of the previous
attempt's patch, so I applied `iteration-v6/patch.diff` to the clean worktree as the starting
point (it applied cleanly) and read `iteration-v6/build-notes.md` and `size-signal.json` (this
bundle's own earlier output). For the runner I read `pdca.toml`, `docs/INTEGRATION.md` (grep) and
the headers of `engine/scripts/run-verify.sh` and `engine/xtask.sh`. In the target I read two
lines of proposal 0016 the finding cites (`0016:2633`, X104; `0016:352-358`, the `retire:records:`
row) to check the comment I wrote about them. I read no reviewer file (`check-*`, `SUMMARY.md`,
`deferred-findings.json`) and no other bundle.

## Carry-forward: what each finding got

### 1. The re-run check accepted a records obligation that also owes a part set

`recheck_fenced` compared only `owed.segments()` with the session's own group. So
`retire:records:s:<id>:3` = `{"parts":[[1,2]],"seg":{<own group>}}` passed as the attempt's
deleter. No fence files that payload for an aborted session, and its drain would delete `part:`
records that the `{session, all}` bytes obligation still has to list to mark their bytes (X104,
`0016:2633`).

Fix, one line: the guard is now
`owed.segments() == Some(group) && owed.parts().is_none()`
(`crates/custodian/src/restore.rs:957`). Anything else that decodes is named `NotOfAttempt`
(`:960`), at the obligation's key.

Why that guard is complete. Under a session-wide `retire:records:` key only two components can
decode at all: an explicit part set and a `seg` group (`multipart.rs` `checked_against_key`:
`session`, `chunks`, `generation` and `"all"` are refused there by mode or token scope). So the
decodable payloads are `{parts}`, `{seg}` and `{parts, seg}`, and the guard now passes exactly
`{seg: <own group>}`, which is what `RetireObligation::attempt_segments`
(`crates/core/src/multipart.rs:3665`) mints.

Words that changed with it:

- `recheck_fenced`'s doc: "trusted only if it owes `group` and nothing else" (`restore.rs:930`).
  The code now does what the comment says, which was the reviewer's point.
- `SegmentFault::NotOfAttempt`'s doc (`:259-261`) and its operator text (`:275`): "it does not
  owe only the attempt's own segments". The old text ("does not owe the attempt's own segments")
  would have been false for this new case, which does owe them.
- Docs: "owes another range" became "owes anything but that range"
  (`docs/design/architecture/06-runtime-view.md:65`) and "owes anything but those records"
  (`docs/design/architecture/m4-first-deployment-blueprint.md:635-636`).

Test, in leg K (`crates/custodian/tests/restore_completing_fence.rs`): a fifth `Aborting@4`
session, `f1`, with clean segments and that payload built from its own group (`:605-606`, `:612`).
A new assertion pins that `f1` is the only one of the five whose obligation owes its own group
(`:627-629`), so the fixture cannot quietly turn into a second "foreign group" case. The existing
loop then requires both passes to name the session and the obligation key (`:647-648`), with one
audit event per pass (`:650`), and the third pass (obligation dropped) to name its first `seg:`
record (`:659-664`).

### 2. Stale rustdoc and a singular "that key"

- `reconcile_after_restore`'s summary line: "every session the image holds `Open` or
  `Completing` is fenced" (`restore.rs:397`).
- Its section heading: "Every session the image held `Open` or `Completing` is fenced — last"
  (`:421`). I searched the tree for links to the old heading's anchor; there are none.
- The paragraph I added last time now stands on its own instead of leaning on "also": "A
  `Completing@E` session is fenced the same way, its commit also installing its attempt's
  `retire:records:{seg}` (…), that key required absent too; the attempt's range is then judged
  on every run" (`:432-434`). The `Open@E` paragraph above it (`:423-430`) is unchanged.
- `06-runtime-view.md:65`: "no obligation already holds that key" → "…holds a key it installs".

What I tried first and dropped: rewriting the `Open@E` paragraph itself to cover both states. It
changed two more base lines and pulled eight more context lines into the hunk, about 535 bytes
more than the version above, for the same meaning.

### 3. C4 diff coverage "patch.diff does not apply on origin/main"

Unchanged, and not fixable in the patch. This bundle is stacked on child-1..3, so its patch
applies on the integration branch, not on `origin/main`. `run-verify.sh` gets that base through
`PDCA_BASE`; the diff-coverage gate does not seem to. That is gate wiring.

## Refuting my own test

- **(a) Genuine red?** Yes. `run-verify.sh` on the final `patch.diff` reverted every production
  file and kept the test: `0 passed; 7 failed`, all by assertion, none by a compile error.
  **7 of 7 ran red.** G, G-sparse, H and K panic at `:296` (`assert_aborted`: the session is
  still `Completing@3`); G-collision at `:465` (the base names `cause: Completing`, not "key
  taken"); G-atomic at `:420` and Order at `:711` (the base returns `Ok` where the fix returns the
  commit's `Err`). As the brief predicted, H(i) and H(vi) already hold on the base; leg H still
  goes red there through its other cases.

  For this round's finding I ran the reviewer's mutant (the full patch with
  ` && owed.parts().is_none()` removed) through `run-verify.sh` as a scratch bundle. Its GREEN leg
  failed: `6 passed; 1 failed`, leg K at `:648`, message `mpu:f1f1…` (the re-run did not name the
  session). With the guard in place all 7 pass.
- **(b) Production path?** Yes. Every leg calls `wyrd_custodian::reconcile_after_restore`
  (`restore_completing_fence.rs:175`), which runs the real `fence_session` (`restore.rs:801`) →
  `plan_fence` (`:886`) → `SessionRecord::completing_teardown`
  (`crates/core/src/multipart.rs:2291`) → `MetadataStore::commit`, then `check_attempt` (`:971`)
  and `part_chunks` (`:1019`), and on a re-run the real `recheck_fenced` (`:935`). The audit
  events are the ones the production `tracing::warn!` in `emit_segments_unaccounted` (`:1491`)
  emits, read through a `tracing` subscriber. Only the store and the disks are doubles.
- **(c) Fixture includes the fault?** Yes. Each named session really holds the bad record: an
  undecodable segment, a segment naming a chunk no part holds, a stray key, an obligation that
  will not decode / owes another group / owes only parts / owes its own group plus parts / is
  missing. The new case's payload is checked to decode and to owe the session's own group
  (`:626-629`). The store really refuses an oversized value: G-sparse first commits a
  `MAX_VALUE_BYTES + 1` probe and asserts the refusal. Every session's nonce differs from its id.

Mutants from earlier rounds (audit event, fault text, nonce-vs-id, missing-obligation branch,
foreign-group check, first-chunk-only, require-absent on one key only, single-page reads,
`{session, parts}` overflow, fencing without the `seg:` obligation) were not re-run. The
production code they target did not change this round, and the cases that killed them are still
in the file.

## Size budget (brief: at most 7 files, under 80 KB)

7 files. `patch.diff` is **81,816 bytes = 79.90 KB** at 1,024 bytes per KB (how every earlier
iteration measured it), 104 bytes under. In decimal it is 81.8 kB, as every earlier iteration was.

Iteration 6 had 174 bytes left. This round's fixes cost about 2.1 KB before cuts (the two rustdoc
fixes are each a new hunk: one changed line plus six lines of context, about 650 bytes apiece).
What I cut to pay for it. No leg, case or assertion was dropped.

| Cut | Saved (approx.) |
|---|---|
| The `{seg}` row of the retire rows table goes back to the base's wording (`crates/core/src/multipart.rs:3308`); that also shortens the hunk by four long table rows of context | 890 B |
| The Completing paragraph reworded in place instead of rewriting the `Open@E` paragraph (finding 2 above) | 460 B |
| "Base: …" clauses removed from the seven leg doc comments | 210 B |
| Six one-line doc comments on test helpers whose signature already says it (`upload`, `restore_pass`, `obligation`, `capture_audit`, `seed_segment`, `assert_open_fenced`) | 370 B |
| Tighter wording in the two docs and in two `restore.rs` doc comments | 120 B |

Trade-offs the human may want to weigh:

1. **The `{seg}` table row no longer names the restore fence.** It reads "a `Completing`
   rollback's dangling segments (`0016:663`, `:665`)", and `0016:665` is the
   `Completing → Aborting` fence row, so it is still correct. The `{session, all}` row
   (`multipart.rs:3301`), which the brief asked for, does name the restore fence and gives the
   reason. `completing_teardown`'s own doc (`:2280-2290`) names both obligations.
2. **The leg doc comments no longer say what the base does.** That was pre-fix history. It is
   recorded here instead, under (a).
3. **One comment is less exact than it could be**: `restore.rs:1559` (in `emit_summary`) says a
   fence cut short "may have left an `Open` session live". True, and now also true of a
   `Completing` one. Fixing it merges into the next hunk and costs about 475 bytes, which the
   budget does not have.
4. Carried from iteration 6, unchanged: child-3's helper comment at
   `crates/custodian/tests/restore_open_fence.rs:665` is less exact than the code below it; the
   blueprint's NOT FENCED bill lists child-3's `LostConflict` cause; the core unit test sits in
   `mod open_teardown` (`multipart.rs:5197`); the CLI paragraph does not say how to repair a
   `NoDeleter` session (I think #659's runbook is the right home).

If the human would rather have those comments back than hold the 80 KB line, restoring all of
1–3 costs about 1.6 KB.

## Gates and commit-readiness

- `cargo fmt --all`: applied; no file changed after the last run.
- `./engine/xtask.sh ci` (= `cargo xtask ci` in the worktree, on the final tree):
  `xtask ci: all checks passed`, exit 0. It ran `typos`, the docs renderer
  (`render_site --check`, 99 pages, link audit OK), fmt, clippy, the workspace tests, the
  ADR-0035 statics gate ("no DST-reachable shared mutable global state" — the test file's two
  statics are not DST-reachable), and `wyrd-dst` under `--cfg madsim`. Both external tools the
  brief names (`typos`, `docs-renderer`) were present and ran, so there is no NEEDS-HUMAN
  external dependency.
- `run-verify.sh` (`PDCA_BUNDLE=results/issue_842 PDCA_LANE=0
  PDCA_BASE=origin/pdca-integration/r-a834e98f67b952c61db7a03084c626e8/main`) on the final
  bundle: `PASS — red without the fix, green with it (7 test(s) ran red).`
- The tree and `patch.diff` are byte-identical (`git diff` with the new test file marked
  intent-to-add), and `git diff --check` reports no whitespace errors. I unstaged the
  intent-to-add afterwards, so the worktree's index is as the harness left it.
- Self-review against the `AGENTS.md` rubric the driver prompt carried: no new clock read; the
  pass still uses only `MetadataStore`; an undecodable record is left byte-identical and named
  (ADR-0045); absent or unexpected obligations are named, never skipped; every read is a bounded
  page; the docs for the changed behaviour are in the same patch. The DST deferral is marked
  `// deferred: #843` in the test file and at `restore.rs:785`.
- Nothing was pushed and no PR was opened.

## Known neighbours, not done here (carried from iteration 6)

- **An `Aborting` session with `part:` records but no `retire:bytes:` obligation** is not named.
  That is on the bytes key, which child-3's fence owns and the brief puts out of scope. Nothing
  is wrongly deleted; the records just have no deleter. I suggest #659.
- **Attempts older than `E'-1`.** The re-check reads only the session's range at `E'-1`. An
  earlier attempt's range belongs to the rollback that left it (`0016:663`), which is #656's door.
- 0016's reaper and operator-abort `Completing → Aborting` rows (`0016:665`, `:2193`) still
  specify `{session, parts}` and hit the same 100,000-byte ceiling. Flag to #656 and #659. No
  edit to 0016 or to any ADR here. #810 should depend on this child, not on #809.

## What the patch does (behaviour unchanged from iteration 6 except the guard above)

- **Core** (`crates/core/src/multipart.rs`): `SessionRecord::completing_teardown` (`:2291`) mints,
  from a decoded `Completing@E` record, the record at `Aborting@E+1`, `retire:bytes:s:<id>:<E>` =
  `{session, all}`, and `retire:records:s:<id>:<E>` = `{seg: (nonce, E)}` from the private
  `RetireObligation::attempt_segments` (`:3665`), which takes the token epoch from the group so
  key and payload always pass `checked_against_key`. `None` below `Completing` or at `u64::MAX`
  (`checked_add`): no wrap, no panic. Per the human's Plan decision (option a), `{session, all}`
  replaces `{session, parts}`; the retire rows table gives the reason (`:3301`).
- **Custodian** (`crates/custodian/src/restore.rs`): `plan_fence` (`:886`) sends `Open` → one
  obligation, `Completing` → both obligations plus the attempt to check, `Aborting@E'` → re-check
  of its own group at `E'-1`, `Completed` / `Aborting@0` → nothing, and no next epoch →
  `EpochExhausted`. `fence_session` (`:801`) builds ONE batch: `require(mpu, bytes read)`,
  `require_absent` on every obligation key, the session put, every obligation put. On `Conflict`
  it re-reads each obligation key and names the first one taken. After the commit lands,
  `check_attempt` (`:971`) pages the frozen `seg:<nonce>:<E>:` range and, only if it holds
  anything, the session's `part:` range (`part_chunks`, `:1019`). The first faulty record names
  the session in `RestoreReport::segments_unaccounted` and on the audit seam (`unaccounted`,
  `:1037`). `recheck_fenced` (`:935`) is the K re-check: obligation owing exactly the own group →
  check the range; owing anything else or undecodable → name the obligation; absent → the range
  must be empty.
- **CLI** (`crates/server/src/cli.rs`): the summary counts the new list (`:1282`), the NEEDS-HUMAN
  paragraph names each session with its first bad record and why (`:1402-1416`),
  `unsettled_causes` loses the `Completing` slot, and the agreement test covers the new report
  field.
- **Child-3's test** (`crates/custodian/tests/restore_open_fence.rs:675`): its "cannot be fenced"
  `Completing@3` fixture is now `Completing@u64::MAX`, since a plain `Completing@3` is fenced now.
- **Docs**: §6.5 of `docs/design/architecture/06-runtime-view.md:65` and step 7 of
  `docs/design/architecture/m4-first-deployment-blueprint.md:600-644`.

## Scratch

This run's logs and the one mutant patch are under `$PDCA_SCRATCH/pdca-builder-842-i7/` (under
1 MB; no build cache). The harness instructions say cleanup of its roots is the harness's and
that `rm` is not mine to run, so I did not delete it. The CI's docs check may also have left a
`wyrd-docs-build-*` directory under `$PDCA_SCRATCH`.
