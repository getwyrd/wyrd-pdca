# Adversarial review — #842 (809.4), restore fence for `Completing` sessions

Bottom line: I could not break the main path. The red→green proof holds up, the fence is one atomic
batch, and the mutants that matter are killed. I found one small logic gap in the re-run check and
one stale doc block, both cheap to fix, plus one scope question.

## Findings

- NEEDS-HUMAN [impl] — `crates/custodian/src/restore.rs:957`: the re-run check accepts a records
  obligation that owes the session's own segment group **plus** a part set. It compares only
  `owed.segments()`. Concrete case (my scratch probe, production code unchanged): an `Aborting@4`
  session, `retire:bytes:s:<id>:3` = `{"session":true,"parts":"all"}`, and
  `retire:records:s:<id>:3` = `{"parts":[[1,2]],"seg":{"nonce":"<own nonce>","epoch":3}}`, which
  decodes. With clean segments, the pass returns `segments_unaccounted: []` and
  `needs_human() == false`. The same test file names `{parts}` alone as damage (`fc`,
  `crates/custodian/tests/restore_completing_fence.rs:614`, `NotOfAttempt`). No writer files a
  `{parts}` records obligation for an aborted session: 0016:356 says only the publication batch
  does. Its drain would delete the `part:` records that `{session, all}` has to list at drain
  time, leaving part bytes unmarked. That is the X104 outcome (0016:2633). The doc comment just
  above the check (`restore.rs:928-930`) says "trusted only if it owes `group`: one owing anything
  else is the first record at fault", and the code does not do that. Fix: also require
  `owed.parts().is_none()` in the guard, and add this payload as a fifth `Aborting@4` case in leg
  K. Low severity: it needs a damaged store.

- NEEDS-HUMAN [impl] — `crates/custodian/src/restore.rs:397` and `:421-427`: the rustdoc of the
  public `reconcile_after_restore` still says "every session the image holds `Open` is fenced" and
  describes only the `Open@E` / `{session, all}` commit. The patch added a paragraph at `:432-434`
  but left this heading and summary contradicting it. Related wording in
  `docs/design/architecture/06-runtime-view.md:65`: "no obligation already holds that key" is
  singular, but a `Completing` fence now requires two keys absent ("either key"). Doc-only nit.

- NEEDS-HUMAN [human] — `crates/custodian/src/restore.rs:931`: the patch's own reasoning ("its
  absence proves nothing (a damaged or hand-repaired store)") applies just as much to the **bytes**
  obligation, which the re-run check never reads. Probe: `Aborting@4`, a correct
  `retire:records:s:<id>:3` `{seg}`, **no** `retire:bytes:s:<id>:3`, two live `part:` records.
  Result: clean, `needs_human() == false`, and those parts have no deleter. The brief limits leg K
  to the `seg:` side, and the `Open` fence (child-3) has the same gap, so this is outside this
  PR's scope. Per the rubric it needs a decline with a tracking-issue reference, not an in-PR fix.
  A human should decide whether to file that issue.

## Evidence I re-checked (no refutation)

- Red→green, re-run by me: with base `multipart.rs`, `restore.rs` and `cli.rs` and the new test
  kept, 7 of 7 tests fail by assertion (the build succeeds; base names the session
  `cause: Completing`). With the patch, 7 of 7 pass. Every leg calls the production
  `reconcile_after_restore` (`restore_completing_fence.rs`, `restore_pass`). Nothing in the test
  re-implements production code.
- The C5 gate row ("36 mutants: 8 caught, 28 unviable") is weak evidence by itself, because most
  of the "unviable" mutants only failed to compile under the workspace's deny-lints. I re-ran
  cargo-mutants on the diff with `--cap-lints=true` (lints downgraded, so those mutants compile).
  `restore.rs`: 21 caught, 3 unviable, **0 missed**. `multipart.rs`: 3 caught, 5 unviable
  (no `Default` impl), **0 missed**. I also ran a manual mutant that stops the `seg:` read after
  its first page (`restore.rs:991`, `next.filter(|_| false)`). The `paged` case kills it
  (`restore_completing_fence.rs:545`; H and K both fail).
- The G-sparse premise holds: `{"session":true,"parts":[[1,1],[3,3],…,[19999,19999]]}` encodes to
  exactly 128,916 bytes, more than `MAX_VALUE_BYTES` (100,000). So the test's ceiling double would
  refuse a `{session, parts}` regression, and the pass would return `Err`.
- The C4 diff-coverage "fail" happens because the patch is stacked on child-1..3 and does not
  apply to bare `origin/main`. That is a harness limit, not evidence against the fix.

## Attacks tried, could not refute

- Atomicity: the session, bytes and records writes go in one `WriteBatch`
  (`restore.rs:824-831`), with `require` on the session and `require_absent` on both obligation
  keys. G-atomic fails each of the three keys in turn, and a split-commit mutant would leave a
  partial write that the test catches.
- Collision: when both keys are taken, the bytes key is named first (my probe). Each key is
  pinned on its own by G-collision (`c1`, `c4`).
- False alarms on correct stores: a rollback leaves `Completing@E` for `Open@E+1` and files at `E`
  (0016:2196). So an `Open`-fenced `Aborting@E'` session's range at `E'-1` is always empty, and the
  re-check stays quiet (control `fe`). The reaper's `Completing → Aborting` records shape
  (`{seg:<g>:E}`, 0016:665) matches what the re-check expects.
- Key-grammar edges: `seg:<n>:3:` cannot prefix-match `seg:<n>:30:`, and a 7-digit or suffixed
  index fails `parse_seg_key` and is named `KeyNotOfGroup` (`restore.rs:1002`).
- `u64::MAX`: `completing_teardown` returns `None` through `checked_add`, which leads to
  `EpochExhausted`. No wrap, no panic, and the later `Open` session is still fenced (H(vi)).
- Settled deferrals I did not re-raise: #659 (half-drained ranges, `restore.rs:932-934`) and #843
  (Tier-0 DST, test file header and `restore.rs:785`).
