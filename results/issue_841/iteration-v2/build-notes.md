# Build notes — #841 (809.3) restore fence for `Open` upload sessions

Worktree: `/home/eddie/wyrd/wyrd.pdca-wt-l0`, base `4bda59c` (origin/main + #839 + #840, the
`pdca-integration/…/main` stack base). All `path:line` below are on that tree with the patch
applied.

## What the patch does

The post-restore pass (`reconcile_after_restore`) now fences every upload session the restored
image holds `Open`, as its last step, after Pass 3:

- For each session listed under `mpu:` (re-listed in bounded pages through `gc::staged_page`,
  `crates/custodian/src/restore.rs:718-733`), `open_teardown` (`restore.rs:788-809`) classifies
  it: `Open` → fence; `Aborting`/`Completed` → leave alone, uncounted; anything else → a named
  cause.
- The fence is ONE `WriteBatch` (`restore.rs:751-758`): `require(mpu:<id>, bytes as read)` +
  `require_absent(retire:bytes:s:<id>:<E>)` + put the session as `Aborting@E+1` + put the
  obligation `{"session":true,"parts":"all"}`.
- `Committed` → counted in `sessions_fenced` and audited `action=session-fenced`
  (`restore.rs:760-764`).
- `Conflict` → a fresh read of the session and then of the obligation key classifies it once as
  `ChangedUnderPass`, `ObligationKeyTaken{key}` or `LostConflict`; never retried
  (`restore.rs:765-778`).
- `Err` (including `CommitUnknownResult`) → audited `action=session-fence-failed`, then
  propagated unchanged; never read as a conflict (`restore.rs:779-783`).
- The summary is emitted before the error propagates, and reads INCOMPLETE when the fence did not
  finish (`restore.rs:710-714`, `emit_summary` at `restore.rs:1284-1325`).
- Report: `sessions_fenced: usize` and `sessions_unsettled: Vec<UnsettledSession>`
  (`restore.rs:214-218`, types `:221-266`). `is_clean()` is false when any session was fenced
  (`restore.rs:286`); `needs_human()` counts unsettled sessions (`restore.rs:321`).
- `Completing` sessions are named with a `// deferred: #842` marker (`restore.rs:804-805`).

Writer-side API in `crates/core/src/multipart.rs`:

- `SessionRecord::open_teardown(&self, &UploadId) -> Option<OpenTeardown>`
  (`multipart.rs:2256-2278`): `None` unless `Open` below `u64::MAX`. Clones the decoded record and
  changes only `epoch` (+1) and `state` (`Aborting {}`), so every other field is one decode
  already vouched for.
- `OpenTeardown { session, obligation }` (`multipart.rs:2280-2297`).
- `RetireObligation { mode, token, payload }` (`multipart.rs:3579-3629`). Private fields; the only
  constructor is the private `session_teardown`, which mints `retire:bytes:s:<id>:<E>` and
  `{session, all}` together, and debug-asserts the pair passes the key-taking decode's own rules
  (`:3609-3617`). Accessors: `key()`, `payload()`. So key and payload cannot disagree.
- Doc notes updated to match:
  - module header (`multipart.rs:91-95`);
  - `SessionRecord`'s "no writer-side constructor" note (`:2163-2166`);
  - the retire rows table's `{session, all}` row (`:3250`);
  - `RetirePayload`'s note (`:3288-3291`);
  - the `all` wildcard notes (`:382`, `:3172-3173`);
  - the `RetiredMap` sentence that claimed no type in the module had a writer-side constructor
    (`:2988-2989`).

Other files:

- `crates/custodian/src/gc.rs:1734`: `staged_page` widened to `pub(crate)` (and one doc line).
  Nothing else in `gc.rs`.
- `crates/server/src/cli.rs`:
  - summary line counts fenced and unfenced sessions (`:1273-1274`);
  - new NEEDS-HUMAN paragraph names unfenced sessions through `named_records` unchanged, and counts
    them by cause via `unsettled_causes` (`:1374-1392`, `:1451-1483`);
  - comment fix for the "never decodes a session value" claim (`:1351-1357`);
  - tests: agreement test extended (`:3026-3038` plus its loop);
    `restore_verdict_counts_fenced_sessions_and_names_the_ones_it_could_not_fence` (`:3123`);
    the 21-session bound test (`:3183`).
- `crates/custodian/tests/staged_protection.rs:1725-1795`: the one existing leg the brief says
  changes by design. Its protection assertions are kept. Its naming assertion now requires the
  session to be named exactly once on the restore audit seam, as `session-unsettled`, and never
  as `unresolvable-staged-record`. Doc lines adjusted (`:34`, `:1729`, `:1740`).
- `crates/custodian/tests/restore_staged_report.rs:228-240`: `segment_nonce` added to child-1's
  `open_session` builder. I also rewrote that builder's doc comment: it said "No segment
  nonce … the slice that first decodes one here adds the nonce", which would be false once the
  nonce is there. Nothing else in that file changed, and both of its legs pass unchanged.
  **Flag:** the brief said "and nothing else in it". The doc comment is part of the builder, but
  if you read the instruction strictly, that doc edit goes beyond it.
- Docs:
  - `docs/design/architecture/06-runtime-view.md:65`: new fence paragraph in §6.5.
  - `docs/design/architecture/m4-first-deployment-blueprint.md:599-639`: step 7. "Four bills"
    now, the UNREADABLE bill's "never decodes their values" claim corrected, a new NOT FENCED
    bill, and a fence paragraph.

## Decisions and what I ruled out

- **A fresh `mpu:` listing for the fence, not the staged reader's.**
  - `staged_fragments` keeps no session values, and `staged_fragments_observing`
    (`gc.rs:1537`) reports only part reads, never sessions.
  - Reusing that walk would mean widening `StagedSet` or its observer in `gc.rs` to carry session
    bytes. The brief limits `gc.rs` to a visibility change.
  - A later read is also the right precondition: the fence pins the bytes as close to its commit
    as the pass can read them.
- **Fence before Pass 3 ruled out** (leg P3; #809 iteration 1's defect).
- **Classify the `Conflict` by re-reading, rather than pre-reading the obligation key.** A pre-read
  would still need the post-conflict classification for F-race, and would add one `get` per
  `Open` session for no extra safety. `require_absent` in the batch is what actually enforces the
  rule.
- **`Err` from the fence commit is propagated, not settled by a re-read.**
  - GC has a settle path for an out-of-flight `CommitUnknownResult` (`gc.rs:1022-1060`). Copying
    it here would be about 25 lines plus a test leg.
  - It would not change the invariant: the fence is idempotent, so the re-run the operator must
    make anyway settles it. A commit that landed leaves an `Aborting` session, which the re-run
    passes over. One that did not land gets fenced then.
  - The honest cost of this choice: a fence that landed under an unknown result is never counted
    in `sessions_fenced` by either run. It is named on the audit seam (`session-fence-failed`).
  - I declined the settle path mainly for the 90 KB budget (see "Size" below).
- **`sessions_unsettled` holds a struct `{session, cause}`, not a `Vec<String>`.** F-collision
  needs the cause ("key taken", not "changed") on the report and the audit line. The CLI maps to
  names so `named_records` stays unchanged.
- **An `mpu:` key that parses to no upload is named unsettled too** (`KeyNamesNoUpload`). The
  staged read already names it in `unresolvable`, but "a session the pass could not fence is
  named" applies to it as well. So it appears in two CLI paragraphs, each answering a different
  question.
- **An undecodable session value stays out of `unresolvable`.** `unresolvable` means "the
  protection reading is partial" and withholds every mark. An undecodable session value does not
  make that reading partial (the staged class still reads the session by key). So it is named only
  as unsettled, which keeps the base's protection and mark behaviour unchanged (brief H(i)).
- **`open_teardown` returns `Option`, not a typed error.** Its only failures are "not Open" and
  "no next epoch". The restore caller matches the state first, so `None` there means exhausted.
  A new error enum would be API surface for a distinction nobody needs.

## Tests: red → green

New file: `crates/custodian/tests/restore_open_fence.rs` (copy at the bundle root). It has 8
`#[tokio::test]` functions; F-atomic's two runs are one test with two cases. It names only
symbols that exist on the base, and reads the new report fields through `Debug`.

| Leg | Test (`restore_open_fence.rs`) |
|---|---|
| F | `an_open_session_is_fenced_whole` (:498) |
| F-atomic (both runs) | `a_failed_fence_commit_leaves_neither_write` (:541) |
| F-race | `a_session_that_changes_under_the_pass_is_not_fenced_blind` (:587) |
| F-collision | `the_fence_never_overwrites_an_obligation` (:638) |
| H (i)(ii)(iii) | `what_the_pass_cannot_fence_is_named_never_passed_off_as_done` (:720) |
| K | `a_second_pass_is_idempotent` (:770) |
| P3 | `a_fence_fault_never_hides_the_pass_verdicts` (:812) |
| Paging | `sessions_listed_across_pages_are_all_fenced` (:857) |
| CLI | `cli.rs` tests above (green-only, co-located, as the brief says) |

**Red on the base, 8 of 8.** I reverted `crates/{core,custodian,server}/src` to base `4bda59c`
and kept the test. The file compiled, and all 8 tests failed by assertion (the gate's
UNVERIFIABLE compile-failure path is not reached). The first failure of each:

- F, K, Paging: `assert_fenced` finds the session still `Open@3` (expected `Aborting@4`).
- F-atomic: `expect_err` gets `Ok(RestoreReport …)`; the base pass writes no fence.
- F-race: "the race was not exercised". The concurrent batch never fired, because the base never
  commits a fence.
- F-collision: `needs_human()` is false.
- H: the undecodable session is not named in `sessions_unsettled` (the base has no such field).
- P3: the pass returned `Ok`, with the dangling chunk in the report.

With the patch: 8 of 8 pass. The whole custodian suite passes, including `staged_protection.rs`
(36 tests) and `restore_staged_report.rs` (2), and so do the `cli.rs` `restore_` tests (7).

**How I ran it.** I did not run `engine/scripts/run-verify.sh` myself. It creates its own verify
worktree beside the primary Wyrd checkout, which is outside the roots I may write to; Check's
C4-verify gate runs it. I ran `cargo test -p wyrd-custodian --test restore_open_fence` inside the
per-cycle worktree, wrapped in `timeout`. That is the same `cargo test -p <pkg> --test <name>`
invocation `run-verify.sh` makes (`engine/scripts/run-verify.sh:441`). I also ran the project's
full gate `./engine/xtask.sh ci` (= `cargo xtask ci`) on the final tree: `xtask ci: all checks passed`, exit 0 (typos, docs lint/render, fmt, clippy, build, whole-workspace tests, cargo-machete, cargo-deny, statics, deploy-guard, DST clippy and DST tests).

**Self-test from the brief's Invariant.** I mutated the fixed `restore.rs` and re-ran the file
(9 tests at the time, before the two F-atomic tests were merged into one):

- Fence moved before Pass 3: only `a_fence_fault_never_hides_the_pass_verdicts` failed (P3).
- Fence split into two commits: F-atomic's obligation-write run failed, as the brief predicts.
  F also failed, because my F asserts that ONE applied commit carries both writes, which is
  stricter than the brief's self-test note ("satisfies F"). F-race, F-collision, H, K and Paging
  failed too, because they call the same `assert_fenced`.
- Blind put of the obligation (no `require_absent`): only
  `the_fence_never_overwrites_an_obligation` failed (F-collision).

## Refute-your-own-test

- **(a) Genuine red?** Yes. With `crates/{core,custodian,server}/src` reverted to the base and the
  test kept, the file compiles and every test fails by assertion (numbers above). I reverted the
  sources in the worktree and re-applied them from a saved diff; I did not merely reason about it.
- **(b) Production path?** Yes. Every leg calls the production `wyrd_custodian::
  reconcile_after_restore` through a `GcContext`. The fence code under test is the production
  `fence_open_sessions` / `fence_session` / `SessionRecord::open_teardown`. The doubles are only
  the `MetadataStore` / `ChunkStore` seams, as in `staged_protection.rs`. Obligations are checked
  through the production `decode_retire_obligation`, and sessions through the production
  `decode_session_record`.
- **(c) Fixture includes the fault?** Yes.
  - F-atomic injects a real `CommitUnknownResult` on the fence's own commit (both halves, one run
    each).
  - F-race lands a real concurrent `Open@3 → Completing@4` batch between read and commit. The
    test asserts that batch committed; otherwise the race was not exercised.
  - F-collision seeds a decodable foreign `{session}` obligation under the fence's key.
  - H seeds an undecodable value, `u64::MAX`, and a `Completing` session.
  - P3 seeds a dangling chunk, an under-replicated chunk and a failing fence commit together.
  - Paging lowers the scan cap to 2, and the fixture asserts one `scan` of `mpu:` fails.

## Scope and budget

- **Files:** 9, the brief's maximum: `multipart.rs`, `restore.rs`, `gc.rs`, `cli.rs`,
  `staged_protection.rs`, `restore_staged_report.rs`, `restore_open_fence.rs`,
  `06-runtime-view.md`, `m4-first-deployment-blueprint.md`.
- **Size:** `git diff` of the patch is 89,236 bytes (the brief's budget is under 90 KB).
  My first complete version was 118 KB. I cut it by shortening doc comments and messages, making
  the test double more compact, merging the F-atomic runs into one test, and dropping two unused
  `RetireObligation` accessors (`mode()`, `token()`). No leg and no binding assertion was
  dropped. While trimming I briefly deleted two existing `cli.rs` tests by accident; they are
  restored byte-identical from the base, and the patch removes no existing test.
- **`crates/dst/tests/custodian.rs`:** untouched. The restore DST properties pass under
  `cargo xtask ci`.

## For the human at sign-off

- **Stale sentence outside the file budget.**
  - `docs/design/architecture/05-building-block-view.md:202` says "Nothing writes or consumes these
    records in production yet". After this patch, the restore fence writes a session transition
    and a `retire:bytes:` obligation.
  - The brief's doc list and 9-file budget leave `05` out, so I did not edit it.
  - `multipart.rs:91`'s header still says the living-architecture doc "says exactly that", which
    leans on the same sentence.
  - Either allow a 10th file or open a follow-up issue.
- **DST coverage (rubric: a new concurrent write path lands with seeded Tier-0 DST coverage).**
  The brief assigns this to child-5. I don't know child-5's issue number, so there is no
  `// deferred: #N` marker in the code. If a reviewer raises it, answer "Deferred — tracked in
  #<child-5>", or add the marker once the number is known.
- **`LostConflict` has no test.** It needs a store that reports a spurious conflict, and the brief
  lists no leg for it. It is the fall-through arm after both re-reads find nothing.
- **External dependencies** (`typos`, `docs-renderer`): both are present locally, and `cargo xtask
  ci`'s typos and docs lint/render steps passed. No undeclared dependency was needed.
