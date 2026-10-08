# Build notes — #842 (809.4), iteration 6: restore fences resurrected `Completing` sessions

Base: `pdca-integration/r-a834e98f67b952c61db7a03084c626e8/main` @ `022d76f` (origin/main +
child-1 #839, child-2 #840, child-3 #841; the bundle's `stack-base`). Line numbers below are on
that base **with `patch.diff` applied** (the worktree at `$PDCA_WORKTREE`).

**What I read besides `brief.md`.** The carry-forward findings cite lines of the previous
attempt's patch, so I applied `iteration-v5/patch.diff` to the clean worktree as the starting
point (it applied cleanly) and read `iteration-v5/build-notes.md`, `size-signal.json` and
`session-carry-forward` (this bundle's own earlier output). For the runner I read `pdca.toml` and
the headers of `engine/scripts/run-verify.sh` and `engine/xtask.sh`. I read no reviewer file
(`check-*`, `SUMMARY.md`, `deferred-findings.json`) and no other bundle.

**No production behaviour changed this round.** Both carry-forward findings are test gaps: the
reviewer confirmed the production code is right and asked for tests that pin it. The only
production edit is a shorter intra-doc link (`crates/custodian/src/restore.rs:433`). "What the
patch does" is at the end, unchanged from iteration 5.

## Carry-forward: what each finding got

### 1. The audit event and the fault text were pinned by no test

Mutants `emit_segments_unaccounted` → `()` (`restore.rs:1491`) and `Display for SegmentFault` →
empty (`restore.rs:266`) survived the whole custodian suite. That matters because the CLI names
only the first 20 sessions and sends the operator to the audit log for the rest.

Fix, in the test file (`crates/custodian/tests/restore_completing_fence.rs`):

- `capture_audit()` (`:363`) installs the audit subscriber once per binary through a
  `std::sync::Once`, so whichever leg runs first installs it. Leg K (`:598`) and the Order leg
  (`:678`) both call it.
- `audit_names(id, record)` (`:382`) counts this thread's `session-segments-unaccounted` events
  whose `session` is that session's `mpu:` key, whose `record` is that record's key, and whose
  `fault` is not empty.
- Leg K asserts the count is **exactly 2** after two passes for all eight named sessions (`:651`):
  one event from the first pass and one from the re-run. That covers both emit paths: the four
  `Completing` cases are named by `check_attempt` right after their fence lands on pass 1 and by
  `recheck_fenced` on pass 2; the four `Aborting` ones by `recheck_fenced` both times.
- After the third pass, each of the four `Aborting` sessions must have one more event at its
  first `seg:` record (`:663-664`; `fd` was already named there twice, so its count is 3).

I first put the same check in leg H too, then took it out: K runs H's fenced cases (ii, iv, v,
paged) through `seed_case` on both passes, so H's check pinned nothing K does not, and it cost
about 130 bytes the size budget does not have.

The fault check is "not empty", which is what the finding asked for and what kills the mutant.
It does not pin each variant's wording. Pinning words would make the test fail on a harmless
rewording of an operator message, so I left it.

### 2. Every fixture used the upload id as its segment nonce

So no test could tell `record.segment_nonce()` from the upload id.

Fix: `nonce(id)` (`:185`) is the upload id's 32 hex characters reversed (`a7a7…` → `7a7a…`), and
it asserts the result differs from the id (`:187`), so a future fixture such as `ff` cannot
quietly bring the gap back. `session()` writes it into every record (`:200`), `group()` builds
every segment group from it (`:224`), and H(i) strips it (`:516`). This is stronger than the
finding's minimum ("K's clean control and at least one damaged case"): **every** session in
every leg now has a nonce that is not its id.

One knock-on change: K's `Open` control moved from `f9` to `f8` (`:606`). Reversed, `f9` gives
`9f9f…`, which is the group the `FOREIGN` obligation (`:437`) names, and `FOREIGN`'s doc says no
session here has that group.

### 3. C4 diff coverage "patch.diff does not apply on origin/main"

Unchanged, and not fixable in the patch. This bundle is stacked on child-1..3, so its patch
applies on the integration branch, not on `origin/main`. `run-verify.sh` gets that base through
`PDCA_BASE`; the diff-coverage gate does not seem to. That is gate wiring.

## Refuting my own test

- **(a) Genuine red?** Yes. `run-verify.sh` on the final `patch.diff` reverted every production
  file and kept the test: `0 passed; 7 failed`, all by assertion, none by a compile error.
  **7 of 7 ran red.** G, G-sparse, H and K panic at `:300` (`assert_aborted`: the session is
  still `Completing@3`); G-collision at `:471` (the base names `cause: Completing`, not "key
  taken"); G-atomic at `:426` and Order at `:712` (the base returns `Ok` where the fix returns
  the commit's `Err`). As the brief predicted, H(i) and H(vi) already hold on the base; leg H
  still goes red there through its other cases.

  For this round's two findings, I ran each named mutant through `run-verify.sh` as a scratch
  bundle (the full patch with one production change). The GREEN leg failed each time, 6 passed /
  1 failed, the failure in leg K:

  | Mutant | Where K fails |
  |---|---|
  | `emit_segments_unaccounted` does nothing (`restore.rs:1491`) | `:651`, `f2f2…`: 0 events, 2 expected |
  | `Display for SegmentFault` writes nothing (`restore.rs:266`) | `:651`, `f2f2…`: 0 events with a non-empty fault, 2 expected |
  | `SegmentGroup::new(upload.as_str(), attempt)` in place of the session's nonce (`restore.rs:911`) | `:649`: on the re-run `seg:2f2f…:3:000001` is not named; the mutant names `retire:records:s:f2f2…:3` instead |

  The third is the exact false alarm the reviewer described: a correct obligation reported as
  "not of the attempt", and the real bad record hidden.
- **(b) Production path?** Yes. Every leg calls `wyrd_custodian::reconcile_after_restore`
  (`restore_completing_fence.rs:176`), which runs the real `fence_session` (`restore.rs:801`) →
  `plan_fence` (`:886`) → `SessionRecord::completing_teardown`
  (`crates/core/src/multipart.rs:2291`) → `MetadataStore::commit`, then `check_attempt` (`:971`)
  and `part_chunks` (`:1019`), and on a re-run the real `recheck_fenced` (`:935`). The audit
  events are the ones the production `tracing::warn!` at `restore.rs:1493` emits, read through a
  `tracing` subscriber. Only the store and the disks are doubles.
- **(c) Fixture includes the fault?** Yes. Each named session really holds the bad record: an
  undecodable segment, a segment naming a chunk no part holds, a stray key, an obligation that
  will not decode / owes another group / owes only parts / is missing. The store really refuses
  an oversized value: G-sparse first commits a `MAX_VALUE_BYTES + 1` probe and asserts the
  refusal. And now each session's nonce really differs from its id.

Mutants from earlier rounds (missing-obligation branch, foreign-group check, first-chunk-only,
require-absent on one key only, single-page reads, `{session, parts}` overflow, fencing without
the `seg:` obligation) were not re-run. The production code they target did not change, and the
cases that killed them are still in the file.

## Size budget (brief: at most 7 files, under 80 KB)

7 files. `patch.diff` is **81,746 bytes = 79.83 KB** at 1,024 bytes per KB (how the harness's
size signal divides), 174 bytes under. In decimal it is 81.7 kB, as every earlier iteration was.

Iteration 5 had 28 bytes left, and this round's test additions cost about 1.2 KB. What I cut to
pay for it. No leg, case or assertion from earlier rounds was dropped.

| Cut | Saved (approx.) |
|---|---|
| Child-3's test helper doc line goes back to the base's wording (`crates/custodian/tests/restore_open_fence.rs:665`), which removes one whole hunk from the diff | 560 B |
| Blueprint: three two-line edits reworded so each changes one line or adds none (`m4-first-deployment-blueprint.md:600`, `:629`, `:640-641`) | 320 B |
| H's audit check, redundant with K's (see finding 1) | 130 B |
| Audit capture stores `(ThreadId, fields)` tuples instead of a named struct (`restore_completing_fence.rs:335`) | 110 B |
| `Meta::applied()` accessor inlined at its two callers (`:316`, `:505`); one `JUNK` constant for the three "not a segment" values (`:31`); a shorter double name; a shorter doc link in `restore.rs:433` | 160 B |

Trade-offs the human may want to weigh:

1. **Child-3's helper comment is less exact.** `restore_open_fence.rs:665` again reads
   "(iii) `Completing`", while the code two lines below seeds `completing(u64::MAX)` (`:675`).
   The leg's own doc (`:681`) does say "an `Open` and a `Completing` one at `u64::MAX`". The
   comment is not wrong, only less specific. Restoring the exact wording costs 560 bytes, which
   puts the patch about 390 bytes over.
2. **The blueprint's NOT FENCED bill now lists a fifth cause**, "its commit lost an unexplained
   conflict" (`:629`). That cause (`SessionUnsettled::LostConflict`) is child-3's and was missing
   from the list. I added it because it let me replace the "or it is Completing (not supported
   yet)" clause on the same line instead of rewrapping two lines. It is accurate, but it is a
   doc line about child-3's behaviour.
3. **The core unit test still sits in `mod open_teardown`** (`multipart.rs:5197`), as in
   iteration 5. Its own module costs about 490 bytes.
4. **The CLI paragraph does not say how to repair a `NoDeleter` session** (carried from
   iteration 5; I think #659's runbook is the right home).

## Gates and commit-readiness

- `cargo fmt --all`: applied.
- `./engine/xtask.sh ci` (= `cargo xtask ci` in the worktree, on the final tree):
  `xtask ci: all checks passed`, exit 0. It ran `typos`, the docs renderer
  (`render_site --check`, 99 pages, link audit OK), fmt, clippy, the workspace tests, the
  ADR-0035 statics gate ("no DST-reachable shared mutable global state" — the test file's two
  statics are not DST-reachable), and `wyrd-dst` under `--cfg madsim`. Both external tools the
  brief names (`typos`, `docs-renderer`) were present and ran, so there is no NEEDS-HUMAN
  external dependency.
- `run-verify.sh` (`PDCA_BUNDLE=results/issue_842 PDCA_LANE=0
  PDCA_BASE=pdca-integration/r-a834e98f67b952c61db7a03084c626e8/main`) on the final bundle:
  `PASS — red without the fix, green with it (7 test(s) ran red).`
- The tree and `patch.diff` are byte-identical (`git diff` with the new test file marked
  intent-to-add), and `git diff --check` reports no whitespace errors. I unstaged the
  intent-to-add afterwards, so the worktree's index is as the harness left it.
- Nothing was pushed and no PR was opened.

## Known neighbours, not done here (carried from iteration 5)

- **An `Aborting` session with `part:` records but no `retire:bytes:` obligation** is not named.
  That is on the bytes key, which child-3's fence owns and the brief puts out of scope. Nothing
  is wrongly deleted; the records just have no deleter. I suggest #659.
- **Attempts older than `E'-1`.** The re-check reads only the session's range at `E'-1`. An
  earlier attempt's range belongs to the rollback that left it (`0016:663`), which is #656's door.
- 0016's reaper and operator-abort `Completing → Aborting` rows (`0016:665`, `:2193`) still
  specify `{session, parts}` and hit the same 100,000-byte ceiling. Flag to #656 and #659. No
  edit to 0016 or to any ADR here. #810 should depend on this child, not on #809.

## What the patch does (unchanged from iteration 5)

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
  the session in `RestoreReport::segments_unaccounted` and on the audit seam (`unaccounted`,
  `:1037`). `recheck_fenced` (`:935`) is the K re-check: obligation owing the own group → check
  the range; owing anything else or undecodable → name the obligation; absent → the range must
  be empty.
- **CLI** (`crates/server/src/cli.rs`): the summary counts the new list, the NEEDS-HUMAN paragraph
  names each session with its first bad record and why, `unsettled_causes` loses the `Completing`
  slot, and the agreement test covers the new report field.
- **Child-3's test** (`crates/custodian/tests/restore_open_fence.rs:675`): its "cannot be fenced"
  `Completing@3` fixture is now `Completing@u64::MAX`, since a plain `Completing@3` is fenced now.
- **Docs**: §6.5 of `docs/design/architecture/06-runtime-view.md:65` and step 7 of
  `docs/design/architecture/m4-first-deployment-blueprint.md:599-641`.

## Scratch

This run's logs and candidate patches are under `$PDCA_SCRATCH/pdca-builder-842-i6/` (about
640 KB: two candidate patches, three mutant patches, verify logs and the CI log; no build cache).
The harness instructions say cleanup of its roots is the harness's and that `rm` is not mine to
run, and the sandbox refused the `rm` last round, so I did not delete it. The CI's docs check
also left a `wyrd-docs-build-*` directory under `$PDCA_SCRATCH`.
