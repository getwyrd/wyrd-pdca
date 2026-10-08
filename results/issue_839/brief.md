# custodian: restore counts staged skips and names untrusted staged records (809.1)

> Child 1 of 5 of #809's split at its re-plan (2026-09-29); #809 is itself 664.2. Do reads ONLY
> this file. Keep the `- **Label:** value` lines. `path:line` citations are on `origin/main` @
> `243241e` (verified 2026-09-29). Background: 0016 =
> `docs/design/proposals/draft/0016-multipart-commit-protocol.md`; the restore row of its
> decision-2 table is `0016:823`.

- **Slug:** restore-staged-report
- **Kind:** enhancement
- **Defect:** the post-restore pass keeps staged multipart fragments without saying so. A
  fragment kept only because the staged class protects it is skipped **uncounted**
  (`crates/custodian/src/restore.rs:431-441`; the comment at `:433-434` reads "Staged counters
  are #664's"), while 0016 requires the report to carry `staged_skipped` beside `pending_skipped`
  (`0016:823`; `RestoreReport`, `restore.rs:114-183`). A staged record the pass **read but could
  not trust** holds its chunk and reaches only the audit seam (`emit_untrusted_staged`,
  `restore.rs:1000-1013`). The report does not name it, and `is_clean()` (`restore.rs:197`)
  certifies the run clean over it. `restore.rs:819-821` marks the question `deferred: #664`. The
  human decided it at #664's plan revision (2026-09-18, recorded in the harness repo at
  `results/issue_664/brief.md:152-165`): such a record needs **no** human, but the report names
  it and the run is not clean. The decision, its condition and its limits are quoted under leg
  H-iii.
- **Success criterion:** the NEW file `crates/custodian/tests/restore_staged_report.rs` passes
  over in-memory doubles, calling the production `reconcile_after_restore`. Legs:
  **(E) Staged skips are counted, and each kept fragment is counted once.** The counting rule:
  `staged_skipped` counts a fragment whose **first** matching protection, in the pass's existing
  order, is the staged class. That order is: the incomplete-set gate and both committed readings
  (`restore.rs:435-437`), then the staged class (`:438`), then the displaced check (`:454-462`), then the pending lease
  (`:489-491`). So a fragment the committed set protects is never counted as staged, and a
  fragment that the staged class **and** a later protection would both keep is counted as staged,
  not by the later counter. Every kept fragment lands in at most one counter. The store holds:
  two fragments that only the staged class protects (a committed `part:` record of an `Open`
  session places them, no committed inode references them, and no `pending:` lease holds their
  chunk); one fragment a staged record places whose chunk a `pending:` lease also holds; one
  fragment a staged record places that the displaced check would also keep (the committed map
  names its chunk's fragment on another D server, which does not hold it: the shape of
  `crates/custodian/tests/restore_reconcile.rs:685-715`, plus a `part:` record placing that
  fragment where it actually is); one ordinary unreferenced stray; and one fragment a committed
  inode references at this server **and** a staged record places. After the pass, the report's
  `Debug` rendering contains `staged_skipped: 4`, `pending_skipped: 0` and `displaced_kept: 0`,
  and `stranded_marked` is 1 (the stray alone).
  **(H-iii) An untrusted staged record is named, and needs no human.** An untrusted record is one
  the pass read but cannot trust about where its chunk's fragments are: a staged placement whose
  length is not its scheme's fragment count, or an owned `sidx:` value that will not decode under
  a key that still names its chunk (`crates/custodian/src/gc.rs:1318-1324`). Fixture: two `Open`
  sessions. Session 1 holds one `part:` record with a wrong-length placement; session 2 holds only
  trusted records. After the pass:
  (a) the report's `Debug` rendering contains `staged_untrusted` and session 1's record key as
  `object_name` renders it (`gc.rs:1788`; an ASCII key is unchanged), and none of session 2's
  keys appear in that list. This is the discriminating arm;
  (b) `needs_human()` is **false**, and `is_clean()` is **false**. The not-a-clean-bill predicate
  already counts findings that need no human (marks, under-replication, `restore.rs:186-199`);
  (c) no fragment of that chunk carries an `orphan:` mark, and the `untrusted-staged-record`
  audit line still fires (`restore.rs:1004-1013`).
  **(CLI)** `restore_verdict` (`crates/server/src/cli.rs:1256`) counts staged skips in its
  summary line. It names untrusted staged records on an **informational** line, not a
  `NEEDS-HUMAN` one, using `named_records` (`cli.rs:1389`) as the unreadable-records paragraph
  does (`cli.rs:1346-1358`). The line says this pass marked none of those chunks' fragments. It
  must not claim the bytes all survived the restore, and it must **not** promise automatic
  cleanup: whether the retire drain removes such a record is #659's call and not yet decided.
  Cover this in `cli.rs`'s own report tests (green-only). The exit status stays
  `report.needs_human()`, and the agreement test
  (`restore_needs_human_agrees_with_every_paragraph_it_prints`, from `cli.rs:2898`) must still
  hold: an informational line never reads as a NEEDS-HUMAN paragraph.
  **Why no human: the human's decision, quoted, with its condition.** From #664's plan revision
  (2026-09-18, `results/issue_664/brief.md:152-165` in the harness repo): "no, but the report
  names it … Why no human: once #809 fences the session its bytes are garbage whatever the record
  says, so what is left is cleanup, which is automatic work (#659), not a judgement. Why not
  silent: a damaged record points at a bug or corruption, and after #808 it blocks every drain in
  the cluster … so the operator should hear about it at restore time rather than when a drain
  stalls." The rule behind it (keep-on-doubt protects user data, not system residue) is #811.
  - *Its condition is the fence.* #809's fence now lands in child-3 (#841) and child-4 (#842).
    Until they do, the record's session is still `Open`. That window cannot be reached in
    production: no client can create a session until #508, which lands after the whole fence
    stack (#810 depends on #842).
  - *What this pass can claim, and no more:* it marks none of the chunk's fragments, so the pass
    itself destroys nothing the record might name. It does **not** claim the staged bytes all
    survived the restore: the pass lists the fragments that exist (`restore.rs:398-402`) and
    judges missing bytes for committed chunks only (Pass 3, `restore.rs:534-585`).
  - *It is an exception to `needs_human()`'s own rule,* which reads "the findings no loop
    resolves on its own" (`restore.rs:201-211`). No loop removes such a record yet (#659 has not
    decided), so the exception rests on the decision above, not on that rule. Say so in
    `needs_human()`'s doc comment, pointing at #811 and #659, so a later reader does not take the
    omission for an oversight.
  Why not silent, on the base: the record blocks every drain in the cluster
  (`docs/design/architecture/06-runtime-view.md:82`, "blocks every drain the same way").
  **(L) `cargo xtask ci` green.**
- **Falsifiability:** RED on `origin/main`, in-process, by assertion. The base `RestoreReport`
  has neither `staged_skipped` nor `staged_untrusted`, so neither string is in its `Debug`
  rendering. Its `is_clean()` is true over a store whose only finding is a held record. So E and
  H-iii (a)/(b) fail on the base. The test may name only base-visible symbols
  (`wyrd_custodian::{reconcile_after_restore, RestoreReport, GcContext, ExpiredPendingPolicy}`,
  `wyrd_core::multipart::{mpu_key, part_key, sidx_key, UploadId, PartNumber}`,
  `wyrd_core::metadata`, `wyrd_traits`) and no field this slice adds, which is why the report
  fields are asserted through `Debug`. A compile failure on the red leg reports UNVERIFIABLE
  (`engine/scripts/run-verify.sh:533-541`). Record in `build-notes.md` how many tests ran red.
- **Invariant to restore:** whatever the post-restore pass kept or held on staged grounds, its
  report says so: by count for kept fragments, by record key for held ones. A run that held a
  chunk over an untrusted record is never certified clean. Source: 0016 `:823` (restore reports
  `staged_skipped`); `docs/principles.md` §5 C-1 as `restore.rs:194-196` applies it ("clean" is a
  claim about a reading that finished); the human's decision at #664's plan revision
  (2026-09-18). SELF-TEST: counting skips without naming held records leaves an untrusted record
  certified clean.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Ordering note:** wave 1 of #809's split, in parallel with child-2 (no shared file). This child
  stays out of `multipart.rs`, the existing custodian test files, `crates/dst/` and
  `05-building-block-view.md`. child-3 builds on this child's report fields and CLI lines.
  Build every seeded session record in this test through **one** helper, without
  `segment_nonce` (this base's codec refuses it; this base's pass never decodes a session
  value). Child-2 puts that field on every session record, and child-3, the first to decode
  session values, adds it to that helper. So that helper must **not** check its own output
  through `decode_session_record`, as the `staged_protection.rs` helper you copy does
  (`:597-603`): once wave 1 folds child-2's codec under this test, that check would refuse every
  nonce-less record and fail the file before child-3 runs.
- **Surfaces:** data
- **Difficulty:** medium
- **Scope:** `staged_skipped` and `staged_untrusted` on `RestoreReport`, with `is_clean()`
  counting `staged_untrusted` and `needs_human()` unchanged in behaviour (its doc comment gains
  the exception, leg H-iii); the pass counting and naming them;
  replacing the `deferred: #664` marker (`restore.rs:819-821`) and the "Staged counters are
  #664's" comment (`:433-434`) with a pointer to this behaviour; `restore_verdict`'s summary and
  the informational line, with `cli.rs`'s report tests; the verdict list in the m4 blueprint's
  step 7 (`docs/design/architecture/m4-first-deployment-blueprint.md:599-625`, per
  `AGENTS.md:154-157` "Docs currency"). Size budget: four files, well under 50 KB of diff. A
  fifth file means the shape is wrong. / out of scope: reading or decoding any `mpu:` session
  value, and any session fence or `sessions_fenced` counter (child-3, child-4); `multipart.rs`;
  `gc.rs` (the staged reader already exposes each held record's key and fault, `gc.rs:1344`);
  every existing file under `crates/custodian/tests/` (their report assertions are
  field-by-field and should not break; if one does, say so in `build-notes.md` rather than
  editing around it); `06-runtime-view.md` (child-3 writes the restore paragraph);
  `crates/dst/`; any edit to 0016 or an ADR.
- **Repro instruction:** on `origin/main`, seed an `Open` session under `mpu:` with one `part:`
  record placing two fragments that are on disk, plus one `part:` record with a wrong-length
  placement, and run `reconcile_after_restore`. The report's `Debug` rendering has neither
  `staged_skipped` nor `staged_untrusted`, and `is_clean()` is true.
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/custodian/tests/restore_staged_report.rs`. This is a **NEW** file:
  C4-verify earns its red only from an added `*/tests/*.rs` (`run-verify.sh:141-144`). Keep the
  store and disk doubles inside this file. Any other file added under a `tests/` directory
  (a `common/mod.rs`) is itself classified as a test target by the gate and breaks it. No
  `Cargo.toml` change: the dev-dependencies the doubles need are already there
  (`crates/custodian/Cargo.toml:36-44`).
- **Production reach:** the pass under test is the production `reconcile_after_restore`. The
  store and fleet are in-memory doubles, and every staged record is seeded by the test, because
  no client can create one until #508.
- **Citations expected:** Do must cite `path:line` on the target branch for every change. Peer
  callsites Do MAY open and mirror:
  * `crates/custodian/src/restore.rs:431-441` (the staged gate), `:489-491` (`pending_skipped`,
    the counter to mirror), `:813-825` (`attribute_staged` and the marker), `:1000-1013`
    (`emit_untrusted_staged`), `:185-217` (`is_clean`, `needs_human`).
  * `crates/custodian/src/gc.rs:1337-1350` (`StagedSet`; `held` carries each record's key and
    fault).
  * Test doubles and fixtures to copy: `crates/custodian/tests/staged_protection.rs:154-470`
    (store and disk doubles), `:500` (`restore_pass`), `:577-660` (session, part and owned
    fixtures), `:710-770` (a healthy committed object), `:832-918` (audit capture),
    `:1830-1924` (the untrusted-record legs, including the wrong-length part at `:1911`).
  * `crates/server/src/cli.rs:1256-1369` (`restore_verdict`), `:1389` (`named_records`), and its
    report tests from `:2898`.
- **Prior-art check (triage cycles):** by path (`restore.rs`, `cli.rs`), 2026-09-29 on
  `243241e`. The last change to `restore.rs` is #803 (`14646e3`) and to `cli.rs` is #813
  (`f683dbe`). No open PR touches either file, and no closed-unmerged PR among the last 40 does.
  Rejected prior art: #809 iteration 1 (`results/issue_809/iteration-v1/`) built these legs
  inside one 179 KB patch with both fences. Its E and H-iii legs passed review; only the size and
  the fence around them were rejected. Do not build any fence here.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR MAY
happen during the cycle. The PR MUST NOT be marked ready before sign-off accepts.

Plan-review response (2026-09-29): both findings revised in place. (1) H-iii now quotes the
human's 2026-09-18 decision with its source path, states its condition (the fence, #841/#842;
unreachable in production before #508), replaces "nothing is lost" with the narrower claim the
pass can prove (it marks none of the chunk's fragments; it does not judge staged bytes survived),
and has Do record the exception in `needs_human()`'s doc comment. The CLI line was narrowed to
match. (2) E now defines `staged_skipped` as first-matching protection in the pass's existing
order, and adds staged∧pending and staged∧displaced fixtures with explicit expected counts
(`staged_skipped: 4`, `pending_skipped: 0`, `displaced_kept: 0`). Following #840's placement A
(the human, 2026-09-29): this test builds sessions through one helper, without the nonce and
without a decode self-check, so the wave-1 fold with #840 cannot break it; #841 adds the field.
