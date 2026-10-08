# Adversarial review — #842 (809.4), iteration 7

I could not refute the fix itself. The production code did the right thing in every attack below.
One test gap is left (a surviving hand mutant that can wedge the pass), plus one cross-issue item
the brief asked to be flagged, which this diff does not record anywhere.

## Findings

- NEEDS-HUMAN [impl] — `crates/custodian/src/restore.rs:1017-1027` (`part_chunks`): the path that
  **skips an unreadable `part:` record** is never run by any test (llvm-cov: line 1027 = 0). The doc
  comment's claim ("skipped … it can only make a chunk look held by no part, naming the session") is
  unpinned. I tried a hand mutant that turns the skip into `?`
  (`parse_part_key(key).and_then(|_| decode_part_record(value)).map_err(..)?`). It **survives all
  7 tests here, all 9 in `restore_open_fence.rs`, and the rest of the `wyrd-custodian` suite**
  (`--no-fail-fast`). Failing case, which I ran: a `Completing@3` session `ab…` whose segment 1 names
  part 2's chunk, with `part:ab…:000002` = `not a part`, plus an `Open` control `fe…` that sorts
  after it. Production (I checked): fenced, named `ChunkInNoPart` at `seg:ba…:3:000001`, the part
  listed in `unresolvable`, the control fenced, and both passes `Ok`. Under the mutant: the pass
  returns `Err("malformed part: record value …")` and **the `Open` control is left `Open`**
  (unfenced, so it can still publish: D-B). Every re-run fails the same way, because `recheck_fenced`
  → `check_attempt` → `part_chunks` reads the same record, so the fence generation can never finish.
  Fix: add this case to leg H (and to K's store), asserting it is fenced and named, the control is
  fenced, and the pass is `Ok` on both runs. Low severity: the code is correct today; this pins it.
- NEEDS-HUMAN [human] — `crates/core/src/multipart.rs:3302`: the patch rewrites the
  `{session, parts: <set>}` row so it now names the reaper's and the operator abort's
  `Completing→Aborting` fence as that shape's writers. It says nothing about the overflow that the
  row just above it gives as the restore fence's reason to avoid that shape (10,000 sparse parts =
  128,916 bytes > `MAX_VALUE_BYTES`). The brief's ordering note says to "flag that to #656 and #659".
  Nothing in the diff records that it was done: no `deferred:` marker, no note in the row. A #656
  builder who reads this table as the spec will build a batch that FoundationDB refuses permanently
  (`2103`). A human should confirm the flag was filed on #656/#659, or decide whether the row should
  point at it.

## Minor (not routed; optional doc nits)

- `crates/core/src/multipart.rs:3456-3463` still says `all` is legal only in "the session teardown
  `{session, all}` the reaper's `Open` arm installs", and that "only a teardown fence makes that range
  immutable". For the new `Completing` row, it was the Complete fence that froze the range
  (`0016:704`; `upload_part_answer`, `multipart.rs:4464`). `crates/custodian/tests/restore_open_fence.rs:665` still
  calls case (iii) plain "`Completing`" after the patch moved it to `Completing@u64::MAX`.
- Reviewer evidence, not a defect in this diff: C5's "pass" (37 mutants: 9 caught, 28 unviable) says
  little on its own. The workspace has `warnings = "deny"` (`Cargo.toml` `[workspace.lints.rust]`),
  so most generated mutants never compile. I re-ran it with `--cap-lints=true`: `restore.rs` had 25
  mutants, 22 caught, 3 unviable, 0 missed; `multipart.rs` + `cli.rs` had 12, 6 caught, 6 unviable,
  0 missed. The conclusion holds, but only on my re-run. C4-diff-cov's "fail" comes from the stacked
  base (the patch is on top of child-1..3, which are not on `origin/main`). I measured coverage
  myself: every new line in `restore.rs:801-1050` ran except `:909` (`Aborting@0` → `Settled`,
  harmless) and `:1027` (above).

## Refutation attempts that failed

- **Red→green.** The frozen `C4-verify.log` shows 7 of 7 red on the base, each by assertion (cause
  `Completing`, session still `Completing@3`), not by a compile failure. All 7 + 9 pass on the patched
  tree (re-run here). Every leg drives the production `reconcile_after_restore`. I found no tautology:
  `names()` matches only quoted keys, and the bad `seg:` keys reach the report only through
  `segments_unaccounted`. K also checks the `session-segments-unaccounted` audit event per record.
- **G-atomic as a weak test.** A fence split across commits fails it in either order, and G's
  "ONE commit" check (`assert_fenced`) catches a split directly.
- **Records-obligation guard** (`restore.rs:957`, `segments()==group && parts().is_none()`).
  `session`, `chunks` and `generation` are bytes-mode only (`multipart.rs:3443-3448`), so a decodable
  `retire:records:` value can carry only `parts` and/or `seg`. The guard is complete.
- **False `NoDeleter`/`NotOfAttempt` on a consistent store.** The Complete fence bumps the epoch
  (`0016:704`), and so does every fence (`0016:364`), so an `Open`/`Aborting@E'` session never shares
  `(nonce, E'-1)` with a live attempt. A session that was rolled back and then aborted reads an empty
  range at `E'-1`. Half-drained ranges are under `deferred: #659` (`restore.rs:932-934`): settled.
- **Value ceiling, `u64::MAX`, collisions on either key, paging past 512 `seg:` records and past the
  first `part:` page, multi-chunk segments, a nonce that differs from the upload id.** Each one is
  pinned, and I found no input that gets past it.
- **`is_clean`** (`restore.rs:341-347`) goes through `needs_human()`, which now includes
  `segments_unaccounted`, so a re-run that only names segments is never reported clean.
