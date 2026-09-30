<!-- pdca:split-proposal v1 -->
# Split proposal — issue 809

> **Intake cap override (rule `wyrd-pdca-P1`, INTEGRATION §11).** `scripts/plan-cap --need 5`
> on 2026-09-29 printed: `planned 20/6 (cap) — room for 0, need 5: Plan intake closed`.
> The human (Eduard Ralph) gave an explicit override **for #809 only**, in this Plan session
> (2026-09-29): "Number 1 and create a child item for DST coverage" — split #809 over the cap,
> with seeded DST coverage as a child of its own. The override covers these five children and
> nothing else in the session.
>
> **Reading the convergence report `--accept` prints.** #809 has no `brief.md` right now (the
> re-plan archived it into `iteration-v1/`), so the report scores the parent 0 (`ok`) and calls
> the split NOT CONVERGED. That verdict is an artifact. Against the archived parent brief
> (`oversized`, score 12: difficulty high, 12.7 KB, 4 conflicts, 2 dependency tokens), children
> 1, 2 and 5 score 3 (`ok`) and children 3 and 4 score 6 (`watch`), so every child bands lower.

## Why this slice is oversized

Iteration 1 (`results/issue_809/iteration-v1/`) built everything in one patch: 179 KB across 15
files against the 100 KB size limit, and C5 mutation testing timed out after 7,200 s on 64
mutants with no verdict. The sign-off sent it back to Plan with "too big, split at re-plan", and
added four carry-forward items: the oversized `retire:bytes` value, the fence running before
Pass 3, the untested unreadable/stray `seg:` branch, and the missing DST coverage.

The patch was really five outcomes, each of which can ship on its own:

1. **Restore says what it kept on staged grounds.** `staged_skipped`, `staged_untrusted` and the
   #803 `deferred: #664` question. It reads no session record at all. (Legs E and H-iii of the
   old brief.)
2. **A `Completing` session record names its segment group.** A codec change plus fixture updates
   in six test files. It is the prerequisite for the `Completing` fence, and #658 needs it as well.
   (Leg J.)
3. **The restore fence for `Open` sessions.** The transition, the obligation, atomicity, the
   commit race, idempotency, the report fields and the CLI. The fence moves after Pass 3.
   This was the biggest block of iteration 1 (≈80 KB with its tests).
4. **The fence for `Completing` sessions.** The `seg:` deleter (X57), the H legs, the second-pass
   re-check, the sparse-value fix and tests for unreadable and stray `seg:` records.
5. **Seeded Tier-0 DST coverage** of both fence shapes, which the rubric requires
   (`AGENTS.md:188-190`) and which the old brief had ruled out of scope.

Fewer children do not fit under the limit. Folding the nonce into child 4 puts that child at
≈90 KB, and folding child 1 into child 3 at ≈115 KB. Folding DST into a fence child breaks its
gate. A DST test can only be green-only (see child 5), and it would pull the madsim cfg onto the
fence's own red leg.

**Design decided at this re-plan (2026-09-29, the human, option (a)):** the restore fence writes
`retire:bytes {session, all}` for a `Completing` session as well as an `Open` one, never
`{session, parts: <set>}`. That value has a fixed size, whereas a sparse part set of
10,000 alternating numbers encodes to 128,916 bytes against the 100,000-byte value ceiling.
Details are under child 4.

## Wave sketch

- **Wave 1: child-1 and child-2, in parallel.** They share no file. Child-1 stays in `restore.rs`,
  `cli.rs`, its new test and the m4 blueprint. Child-2 stays in `multipart.rs`, the core tests,
  the `Completing` fixtures of three custodian tests and one DST fixture line, plus doc 05.
- **Wave 2: child-3** (`Depends on: child-1, child-2`). It adds its report fields and CLI lines
  beside child-1's in the same struct and function. It decodes the session record shape child-2
  defines, and it edits `multipart.rs` and `staged_protection.rs`, which child-2 also edits.
- **Wave 3: child-4** (`Depends on: child-2, child-3`). It extends child-3's fence to `Completing`
  sessions, reads child-2's nonce, and extends child-3's doc paragraph.
- **Wave 4: child-5** (`Depends on: child-4`). It needs both fence shapes on its base.

**Outside this proposal, to do right after `--accept`** (field edits on existing briefs, which
the cap exempts). These are recorded here because the proposal format allows only sibling labels
in the ordering fields:

- **#810** (`restore-fence-generation`, PLANNED) says `Depends on: 809`. A split parent is closed
  as COMPLETE, so that dependency would count as met the moment #809 splits, and #810 would build
  with no fence on its base. Repoint it to **child-4**'s id.
- **child-5** needs `Conflicts with: 682, 722` added to its brief. Both edit
  `crates/dst/tests/custodian.rs` (#682 is AWAITING_SIGNOFF, #722 PLANNED).
- The old conflicts (808, 804, 813, 814) are all merged (PRs #822, #812, #824, #826), so no child
  inherits them.

<!-- pdca:child child-1 -->
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
  human decided it at #664's plan revision (2026-09-18): such a record needs **no** human, but
  the report names it and the run is not clean.
- **Success criterion:** the NEW file `crates/custodian/tests/restore_staged_report.rs` passes
  over in-memory doubles, calling the production `reconcile_after_restore`. Legs:
  **(E) Staged skips are counted apart from pending skips.** The store holds two fragments that
  only the staged class protects: a committed `part:` record of an `Open` session places them, no
  committed inode references them, and no `pending:` lease holds their chunk. It also holds one
  ordinary unreferenced stray, and one fragment that a committed inode references **and** a
  staged record places. After the pass, the report's `Debug` rendering contains
  `staged_skipped: 2` and `pending_skipped: 0`, and `stranded_marked` is 1 (the stray alone). So
  the counter means "kept because of the staged class alone", never a fragment the committed set
  already protects.
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
  does (`cli.rs:1346-1358`). The line says every fragment of those chunks was kept. It must
  **not** promise automatic cleanup: whether the retire drain removes such a record is #659's
  call and not yet decided. Cover this in `cli.rs`'s own report tests (green-only). The exit
  status stays `report.needs_human()`, and the agreement test
  (`restore_needs_human_agrees_with_every_paragraph_it_prints`, from `cli.rs:2898`) must still
  hold: an informational line never reads as a NEEDS-HUMAN paragraph.
  Why no human: nothing is lost, since every fragment of the chunk is kept, and what remains is
  cleanup. Why not silent: a damaged record points at a bug or corruption, and it blocks every
  drain in the cluster (`docs/design/architecture/06-runtime-view.md:82`, "blocks every drain the
  same way"), so the operator should hear about it at restore time rather than when a drain
  stalls.
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
- **Surfaces:** data
- **Difficulty:** medium
- **Scope:** `staged_skipped` and `staged_untrusted` on `RestoreReport`, with `is_clean()`
  counting `staged_untrusted` and `needs_human()` unchanged; the pass counting and naming them;
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
<!-- pdca:end child-1 -->

<!-- pdca:child child-2 -->
# core: a Completing session record names its segment group's nonce (809.2)

> Child 2 of 5 of #809's split at its re-plan (2026-09-29); #809 is itself 664.2. Do reads ONLY
> this file. Keep the `- **Label:** value` lines. `path:line` citations are on `origin/main` @
> `243241e` (verified 2026-09-29). Background: 0016 =
> `docs/design/proposals/draft/0016-multipart-commit-protocol.md`, X57 at `0016:880`.

- **Slug:** completing-session-nonce
- **Kind:** enhancement
- **Defect:** a `Completing` session record cannot name the segment records its attempt writes.
  `SessionState::Completing` carries `fenced_at_millis`, `segments_written` and `publish_target`
  (`crates/core/src/multipart.rs:2021-2030`). `PublishTarget` carries `parent`, `name` and the
  fence `epoch` (`:1954-1961`), but not the segment group's nonce. The nonce is deliberately
  independent of the upload id (`:3307-3311`), so nothing else on the record can derive it.
  Without it, no writer that ends a `Completing` attempt can install the
  `retire:records:{seg:<g>:<E>}` obligation 0016 requires in the same batch as the fence
  (`0016:665`, `:2193-2196`). That covers the restore fence (child-4 of this split), the reaper
  and operator abort (#656, #659) and Complete's own rollback (#658), and it leaves those `seg:`
  records with no deleter anywhere (X57, `0016:880`).
  **Design settled at Plan (2026-09-12, the human, option (i)):** the nonce lives on the session
  record, beside the fence epoch `PublishTarget` already carries. Rejected: deriving it from
  `(upload id, E)` as `0016:2333` says, because the code keeps the nonce independent of the
  upload id on purpose (`:3307`).
- **Success criterion:** the NEW file `crates/core/tests/multipart_segment_nonce.rs` passes. The
  wire spelling is fixed here so that this test and child-4's fixtures agree: a `Completing`
  record's `publish_target` carries `"segment_nonce"`, a string of exactly 32 lowercase hex
  characters (`SegmentNonce`, `crates/core/src/metadata.rs:963-1002`), immediately after
  `"epoch"`. For example:
  `{"kind":"Completing","fenced_at_millis":1,"segments_written":2,"publish_target":{"parent":1,"name":"n","epoch":3,"segment_nonce":"0123456789abcdef0123456789abcdef"}}`
  (wrap it in a full session record as the existing fixtures do). Legs:
  (a) such a record decodes through `decode_session_record` (`multipart.rs:2250`), and
  re-encoding the decoded value with `wyrd_core::metadata::encode` (`metadata.rs:1934`) gives
  back the input bytes exactly;
  (b) the same record **without** `segment_nonce` is refused;
  (c) a nonce that is not 32 lowercase hex characters (uppercase, 31 characters, one containing
  `:`) is refused;
  (d) a `segment_nonce` on any state other than `Completing` is refused. It can ride only inside
  `publish_target`, which only `Completing` carries.
  In `crates/core/tests/multipart_session_records.rs` (green-only): the decoded record exposes
  the attempt's segment group `(nonce, epoch)`, so a writer can mint the `seg:` range from it
  (`seg_range_prefix`, `metadata.rs:1505`) without re-parsing the nonce.
  **(L) `cargo xtask ci` green.** After this change every existing `Completing` fixture in the
  workspace carries the nonce; they are listed under Scope.
- **Falsifiability:** RED on `origin/main`, in-process, by assertion. The base `PublishTarget`
  is `deny_unknown_fields` (`multipart.rs:1953`), so (a) fails with `unknown field
  segment_nonce`. The base accepts the nonce-less record, so (b) fails too. The new file names
  only base-visible symbols (`decode_session_record`, `metadata::encode`, the existing
  `SessionState` and `PublishTarget` fields). The group accessor this slice adds is asserted only
  in the green-only file, because naming it in the new file would stop the red leg compiling
  (UNVERIFIABLE, `engine/scripts/run-verify.sh:533-541`). Record in `build-notes.md` how many
  tests ran red.
- **Invariant to restore:** every segment record a `Completing` attempt can write is nameable
  from that attempt's own session record, so the batch that ends the attempt can install their
  deleter. Source: 0016 X57 (`:880`), `:665`, `:2193-2196`; the human's option (i) decision
  (2026-09-12). SELF-TEST: a nonce derivable only from records that outlive the session (the
  `seg:` keys themselves) leaves the fence unable to name them in its own batch.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Ordering note:** wave 1 of #809's split, in parallel with child-1 (no shared file). child-3
  and child-4 build on it. The one-line fixture edit in `crates/dst/tests/custodian.rs:2915` sits
  in a region #682 and #722 do not edit. Downstream, not work here: #658 must write
  `segment_nonce` when its Complete fence moves a session into `Completing`.
- **Surfaces:** data
- **Difficulty:** medium
- **Scope:** the nonce on the `Completing` session record's wire shape and codec
  (`multipart.rs:1943-2039`, and the record's canonical-bytes decode, `:2121-2125`), with an
  accessor for the attempt's segment group. Every existing `Completing` fixture is updated to
  carry it: `crates/core/tests/multipart_session_records.rs`,
  `crates/core/tests/multipart_state_machine.rs`, `crates/custodian/tests/staged_protection.rs`
  (the `session` helper, `:577`), `crates/custodian/tests/staged_scrub.rs`,
  `crates/custodian/tests/staged_repair.rs`, and `crates/dst/tests/custodian.rs:2915` (that
  fixture only). Also the persisted-field sentence in
  `docs/design/architecture/05-building-block-view.md:202` (`AGENTS.md:154-157`, "Docs
  currency"). Size budget: under 40 KB of diff. / out of scope: any writer of the nonce (#658)
  and any reader of it (child-4); `restore.rs` and every custodian source file; the
  retire-obligation codec; `metadata.rs`. The nonce type exists already, and it deliberately has
  no `Deserialize` (`metadata.rs:979-982`), so the field must decode through its validating
  constructor. Also out: any edit to 0016 or an ADR.
- **Repro instruction:** on `origin/main`, pass the example record above to
  `decode_session_record`. It fails with `unknown field segment_nonce`. Drop the field and it
  decodes.
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/core/tests/multipart_segment_nonce.rs`. This is a **NEW** file:
  C4-verify's red comes only from an added `*/tests/*.rs` (`run-verify.sh:141-144`). The
  accessor leg goes in the existing `multipart_session_records.rs`, which the red leg reverts,
  so it runs green-only.
- **Citations expected:** Do must cite `path:line` on the target branch for every change. Peer
  callsites Do MAY open and mirror:
  * `crates/core/src/multipart.rs:1943-1961` (`PublishTarget`), `:2006-2039` (`SessionState`),
    `:2077-2135` (the session wire shape and its canonical decode), `:2250`
    (`decode_session_record`).
  * `crates/core/src/metadata.rs:983-1047` (`SegmentNonce`, `SegmentGroup::new`, the one
    validating path) and `:1048` (`SegmentGroup`'s own `Deserialize`, the peer for decoding a
    validated nonce off the wire).
  * Existing fixtures: `crates/core/tests/multipart_session_records.rs:101-109`, `:276`, `:320`.
- **Prior-art check (triage cycles):** by path (`multipart.rs`), 2026-09-29 on `243241e`. The
  last change is `4533549` (capacity knobs). No open PR touches the file, and no merged change
  adds a nonce to the session record. Rejected prior art: #809 iteration 1 added this nonce
  inside `PublishTarget` after `epoch` (the shape fixed above) inside a 179 KB patch, and its
  codec legs passed review. #637 iteration 1 fenced `Completing` sessions with no nonce at all;
  do not repeat that.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR MAY
happen during the cycle. The PR MUST NOT be marked ready before sign-off accepts.
<!-- pdca:end child-2 -->

<!-- pdca:child child-3 -->
# custodian: restore fences resurrected Open upload sessions (809.3)

> Child 3 of 5 of #809's split at its re-plan (2026-09-29); #809 is itself 664.2. Do reads ONLY
> this file. Keep the `- **Label:** value` lines. `path:line` citations are on `origin/main` @
> `243241e` (verified 2026-09-29). This bundle's base is `origin/main` **plus child-1's and
> child-2's accepted patches**: locate their additions (`staged_skipped`, `staged_untrusted`, the
> `segment_nonce` field and its accessor) by symbol. Background: 0016 =
> `docs/design/proposals/draft/0016-multipart-commit-protocol.md`; D-B and decision 1.4 at
> `0016:717-728`; the abort/restore fence row at `0016:664`; the restore row at `0016:823`; the
> D-B test row at `0016:879`.

- **Slug:** restore-fence-open
- **Kind:** enhancement
- **Defect:** the post-restore pass fences no upload session. A restored metadata image can
  resurrect a session that was torn down after the restore point, with its bytes already
  reclaimed, and nothing stops a retried Complete from publishing over them (D-B,
  `0016:717-728`; F13). `reconcile_after_restore` (`crates/custodian/src/restore.rs:312-589`)
  reads each session only by key, for staged protection (`crates/custodian/src/gc.rs:1505-1510`:
  "A session record's value is never decoded"), and never writes to it. 0016's restore fence
  moves every session open in the image to `Aborting@E+1`, in one batch with the retirement
  obligation that owes its records (`0016:664`, `:823`, `:717-728`), and the report counts it as
  `sessions_fenced` (`0016:823`, `:879`).
- **Success criterion:** the NEW file `crates/custodian/tests/restore_open_fence.rs` passes over
  in-memory doubles, with records seeded as raw JSON, calling the production
  `reconcile_after_restore`. Legs:
  **(F) An `Open` session is fenced, whole.** An `Open@E` session with committed parts ends as
  `Aborting@E+1`. The same commit installs `retire:bytes:s:<id>:<E>` owing the session's staged
  residue **and every part**: `{session, all}`, the one payload shape that owes both
  (`crates/core/src/multipart.rs:3141-3142`, `0016:2187`). `{session}` alone would leave the
  committed parts with no deleter. The obligation decodes through `decode_retire_obligation`
  (`multipart.rs:3455`) against the key it sits under. The `Debug` rendering contains
  `sessions_fenced: 1`. A session already `Aborting` or `Completed` in the image is left
  byte-identical and not counted.
  **(F-atomic) The fence lands whole or not at all.** Use a store double that fails the commit
  carrying the session's write in one run, and the commit carrying the obligation's write in
  another. After either run, **neither** write is present and the pass returns `Err`. A commit
  whose outcome is unknown is never read as `Conflict` (`crates/traits/src/lib.rs:205-212`,
  `AGENTS.md:178-180`).
  **(F-race) A session that changes under the pass is not fenced blind.** A write lands on the
  session record between the pass's read of it and the fence's commit: a Complete fence moving it
  to `Completing@E`. The fence then writes nothing: no `retire:` key names that session, and the
  concurrent write's bytes are intact. The session is named as needing a human, and the pass
  still fences every other session and returns `Ok`.
  **(H) What this pass cannot fence is named, never passed off as done.** Each of these is left
  byte-identical, has no `retire:` key written for it, and is named as needing a human
  (`needs_human()` true, `restore.rs:212`):
  (i) a session whose value will not decode (the staged class still protects its records exactly
  as on the base);
  (ii) an `Open` session at epoch `u64::MAX`, which has no `E+1`;
  (iii) a decodable `Completing` session. Its fence must also retire its segment records and is
  child-4's; until that lands, this pass must say it did not fence it.
  **(K) A second pass is idempotent.** Re-running over the fenced store leaves the whole store
  byte-identical to how the first pass left it (no second obligation, nothing fenced again), and
  every session H named is named again.
  **(P3) A fence fault never hides the pass's verdicts.** The store holds one dangling committed
  chunk and one `Open` session whose fence commit fails. The pass returns `Err`, **and** the
  chunk's `DANGLING` audit line (`emit_dangling`, `restore.rs:906`) was emitted before it. So
  the fence runs after Pass 3 (`restore.rs:534-585`), never before it. This is the "no report AT
  ALL" class #651 fixed (`restore.rs:324-329`).
  **(Paging)** Sessions listed across more than one page of the `mpu:` listing are all fenced. A
  scan cap on the double forces the paging, as `staged_protection.rs:1067` does.
  **(CLI)** `restore_verdict` (`crates/server/src/cli.rs:1256`) counts fenced sessions in its
  summary, and prints a `NEEDS-HUMAN` paragraph naming every session it could not fence, through
  `named_records` (`cli.rs:1389`). Cover this in `cli.rs`'s report tests (green-only), and extend
  the agreement test (from `cli.rs:2898`) to the new paragraph.
  **(L) `cargo xtask ci` green.** This includes the existing restore legs in
  `staged_protection.rs` (among them the three writes-during-restore legs, `:1511-1540`) and the
  restore DST properties in `crates/dst/tests/custodian.rs:1954-2200`, unchanged.
- **Falsifiability:** RED on its base (`origin/main` + child-1 + child-2), in-process, by
  assertion: the base pass decodes no session and writes no fence, so every leg but CLI fails
  there (session still `Open@E`, no `retire:` key, no `sessions_fenced` in `Debug`, `Ok` where P3
  wants `Err`, `needs_human()` false for H). The test names only symbols on its base, none this
  child adds; new report fields are asserted through `Debug`. A red-leg compile failure reports
  UNVERIFIABLE (`engine/scripts/run-verify.sh:533-541`). Record in `build-notes.md` how many ran
  red.
- **Invariant to restore:** after the post-restore pass, no session the restored image held
  `Open` can publish, and every record that session wrote has a named deleter, installed in the
  same commit as its fence. A session the pass could not fence is named, and the pass's other
  verdicts survive any fault in the fence. Source: 0016 D-B and decision 1.4 (`:717-728`),
  `:664`, `:823`, `:879`; ADR-0045 (what cannot be decoded is left byte-identical and named);
  `AGENTS.md:178-180` (an unknown commit outcome is never a clean conflict). SELF-TEST: a fence
  placed before Pass 3 satisfies F and fails P3, and a fence split across two commits satisfies F
  and fails F-atomic.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** child-1, child-2
- **Ordering note:** wave 2 of #809's split. child-1 is a prerequisite because this child's
  report fields and CLI lines sit beside child-1's in `RestoreReport` and `restore_verdict`.
  child-2 is one because this child's `Completing` arm (H-iii) decodes the record shape child-2
  defines, and both edit `multipart.rs` and `staged_protection.rs`. child-4 builds on this fence.
  Downstream, not work here: #656 should reuse this fence's batch shape for client Abort.
- **Surfaces:** data
- **Difficulty:** high
- **Do model:** opus
- **Scope:** the restore fence for `Open` sessions: one commit per session, installing the
  transition and its obligation together, holding only if the session record is still exactly
  what the pass read (the fence row's own precondition, `0016:664`). It runs after Pass 3, before
  `emit_summary` (`restore.rs:587`).
  `sessions_fenced` and `sessions_unsettled` go on `RestoreReport`. `needs_human()` counts the
  second, and `is_clean()` is false whenever a session was fenced, because that predicate counts
  the work the pass did (`restore.rs:189-192`). Both get audit lines.
  In `multipart.rs`, add the writer-side API the fence needs. This is the first writer of the
  session record and of a retire obligation, so update the "no writer-side constructor" notes
  (`multipart.rs:2127-2130`, `:3180-3182`) and the retire rows table (`:3141-3142`) to match what
  you add, and keep an obligation's key and payload unable to disagree.
  Update `restore_verdict` and its tests.
  One existing test changes by design: `staged_protection.rs:1742-1830` asserts the pass never
  names an undecodable session on the audit seam, which the fence makes false. Keep its
  protection assertions, and repoint its naming assertion at the unsettled line.
  Docs: a new fence paragraph in `docs/design/architecture/06-runtime-view.md` §6.5, after `:63`
  (not §6.7: its lines are 3–6 KB each). Update the m4 blueprint's step 7
  (`m4-first-deployment-blueprint.md:581-625`), including its claim that the pass never decodes
  an `mpu:` value (`:610-612`), and `cli.rs`'s matching comment (`:1339-1345`).
  `gc.rs` only to widen the visibility of its paging helpers (`walk_staged_range` `:1714`,
  `staged_page` `:1733`) if the fence reuses them.
  Size budget: at most 8 files and under 90 KB of diff.
  / out of scope: fencing `Completing` sessions, and anything about `seg:` records (child-4;
  here a `Completing` session is only named); DST coverage (child-5; leave
  `crates/dst/tests/custodian.rs` untouched unless an existing property stops passing, and then
  say so in `build-notes.md`); any durable record that the fence ran, and any "generation"
  (#810); the retire drain (#659); Abort and Complete (#656, #658); the gateway (#508);
  `scrub.rs`, `reconstruction.rs`; any edit to 0016 or an ADR.
- **Repro instruction:** on `origin/main`, seed an `Open@3` session under `mpu:` with one
  `part:` record and run `reconcile_after_restore`. The session is still `Open@3`, no `retire:`
  key exists, and the report's `Debug` rendering has no `sessions_fenced`.
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/custodian/tests/restore_open_fence.rs`, a **NEW** file (C4-verify's
  red comes only from an added `*/tests/*.rs`, `run-verify.sh:141-144`). Keep the doubles inside
  it: any other file added under `tests/` is taken for a test target by the gate.
- **Production reach:** the pass under test is the production `reconcile_after_restore`. Every
  session is seeded by the test, because no client can create one until #508.
- **Citations expected:** Do must cite `path:line` on the target branch for every change. Peer
  callsites Do MAY open and mirror:
  * `crates/custodian/src/restore.rs:312-589` (the pass: Pass 3 at `:534-585`, `emit_summary` at
    `:587`), `:494-529` (the mark commits: attribute only after a commit lands), `:185-217`.
  * `crates/core/src/multipart.rs:2006-2039` (`SessionState`), `:2133` (`SessionRecord`),
    `:2250` (`decode_session_record`), `:1212` (`mpu_key`), `:1339` / `:1465` (`RetireMode`,
    `retire_key`), `:3129-3150` (the retire rows table), `:3296-3303` (the `all` wildcard's
    rule), `:3455` (`decode_retire_obligation`), `:4236-4250` (no part commits outside `Open`).
  * `crates/custodian/src/gc.rs:1537-1560` (the session listing walk), `:1714`, `:1733`.
  * `crates/traits/src/lib.rs:205-212` (`CommitUnknownResult`), `:1480-1489` (`CommitOutcome`).
  * Test doubles: `crates/custodian/tests/staged_protection.rs:154-400` (the store double with
    hooks, the peer for F-race's write between read and commit), `:1067` (records across pages),
    `:1440-1540` (a write landing during the restore's reads).
  * `crates/server/src/cli.rs:1256-1369`, `:1389`, tests from `:2898`.
- **Prior-art check (triage cycles):** by path (`restore.rs`, `multipart.rs`, `cli.rs`,
  `staged_protection.rs`), 2026-09-29 on `243241e`: no merged change fences sessions; no open PR,
  and no closed-unmerged PR among the last 40, touches these paths. Rejected prior art: #809
  iteration 1 (`results/issue_809/iteration-v1/`) fenced both shapes in one 179 KB patch and ran
  the fence before Pass 3 (leg P3); #664 iteration 1 dropped an already-`Aborting` session's
  findings on a second pass (leg K). Do not repeat either unchanged.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR MAY
happen during the cycle. The PR MUST NOT be marked ready before sign-off accepts.
<!-- pdca:end child-3 -->

<!-- pdca:child child-4 -->
# custodian: restore fences resurrected Completing sessions with their segments' deleter (809.4)

> Child 4 of 5 of #809's split at its re-plan (2026-09-29); #809 is itself 664.2. Do reads ONLY
> this file. Keep the `- **Label:** value` lines. `path:line` citations are on `origin/main` @
> `243241e` (verified 2026-09-29). This bundle's base is `origin/main` **plus child-1's,
> child-2's and child-3's accepted patches**: locate the `Open` fence, `sessions_fenced`,
> `sessions_unsettled` and the `segment_nonce` accessor by symbol. Background: 0016 =
> `docs/design/proposals/draft/0016-multipart-commit-protocol.md`; the `Completing → Aborting`
> fence row at `0016:665`; X57's test row at `0016:880`.

- **Slug:** restore-fence-completing
- **Kind:** enhancement
- **Defect:** after child-3, restore names a resurrected `Completing` session but does not
  fence it. That session can still publish over reclaimed bytes (D-B, `0016:717-728`), and the
  `seg:<nonce>:<E>:*` records its attempt already wrote have no deleter anywhere (X57,
  `0016:880`). 0016's `Completing → Aborting` restore fence is one batch: the session goes to
  `Aborting@E+1`, with its bytes obligation **and** `retire:records:{seg:<g>:<E>}` for the
  segments that attempt wrote (`0016:665`, `:2193`).
  One payload 0016 names for that row cannot be written. `{session, parts: <set>}` for a sparse
  part set exceeds the store's value ceiling: 10,000 alternating part numbers (1, 3, …, 19,999)
  encode to 128,916 bytes against `MAX_VALUE_BYTES` = 100,000
  (`crates/core/src/metadata.rs:549`), though 0016 claims they fit (`0016:382-384`). #809
  iteration 1 measured it. FoundationDB refuses the whole batch permanently
  (`2103 value_too_large`), so every run fails and every session after it stays unfenced.
  **Decided at Plan (2026-09-29, the human, option (a)):** the restore fence's bytes obligation
  for a `Completing` session is `{session, all}`, as for `Open`. It owes the same records: no part
  can commit outside `Open` (`crates/core/src/multipart.rs:4236-4250`, `0016:1030`), so the part
  set is frozen from the Complete fence on. And its size does not grow with the part count.
  Rejected: (b) keeping `{session, parts}` and naming an oversized session instead of fencing it,
  which leaves that session able to publish.
- **Success criterion:** the NEW file `crates/custodian/tests/restore_completing_fence.rs`
  passes over in-memory doubles, calling the production `reconcile_after_restore`. Records are
  seeded as raw JSON; the `Completing` shape carries `publish_target.segment_nonce` (child-2's
  spelling). Legs:
  **(G) A `Completing` session is fenced with its segments' deleter.** A `Completing@E` session
  with `segments_written > 0`, its nonce on the record, and `seg:<nonce>:<E>:*` records present
  ends as `Aborting@E+1`. **One** commit installs `retire:bytes:s:<id>:<E>` `{session, all}` and
  `retire:records:s:<id>:<E>` naming exactly the group `(nonce, E)`. Both decode through
  `decode_retire_obligation` against their keys, and the records obligation's `segments()` names
  that group. The records obligation is installed even when `segments_written` is 0: 0016's row
  is "1 put" (`0016:665`), and a damaged cursor must not decide whether records get a deleter.
  Atomicity works as in child-3's F-atomic: a double failing the commit that carries any one of
  the three writes leaves none of them. Proving that draining empties the range is #659's job.
  **(G-sparse) The fence fits the store's value ceiling whatever the part count.** Use a store
  double that refuses any value larger than `MAX_VALUE_BYTES`, as FoundationDB does. A
  `Completing` session holding 10,000 parts numbered 1, 3, …, 19,999 is fenced as in G, every
  value the pass writes is at most `MAX_VALUE_BYTES`, and an `Open` session whose key sorts after
  it is fenced too.
  **(H) What cannot be fenced cleanly is never passed off as done.**
  (i) A `Completing` record with **no** nonce (the shape before child-2) fails decode. Restore
  leaves it byte-identical (ADR-0045) and names it as needing a human.
  (ii) A `Completing` session whose `seg:` records name a chunk that none of its `part:` records
  holds is still fenced, and still named as needing a human.
  (iv) A `Completing` session with one `seg:<nonce>:<E>:*` value that will not decode is still
  fenced and named as needing a human. The records obligation would delete that record without
  marking whatever chunks it named.
  (v) A key under the group's `seg:` range that is not a well-formed segment key of that group
  is named the same way.
  In every case `needs_human()` is true.
  **(K) A second pass is idempotent and still names what needs a human.** Re-running over the
  fenced store leaves it byte-identical, and a session named under H(ii), H(iv) or H(v) on the
  first pass is named again. "Already `Aborting`" never means "nothing to report"; #664
  iteration 1 got this wrong. A fact Do can rely on: for a session at `Aborting@E'`, only a
  `Completing → Aborting` fence files `retire:records:s:<id>:<E'-1>`. A rollback files at the
  epoch it leaves and lands in `Open@E+1` (`0016:2196`).
  **(Order)** child-3's P3 still holds with a `Completing` session in the store: the fence runs
  after Pass 3.
  **(L) `cargo xtask ci` green.**
- **Falsifiability:** RED on its base (`origin/main` + child-1..3), in-process, by assertion.
  Child-3's pass names a decodable `Completing` session and leaves it untouched, so G, G-sparse,
  H(ii), H(iv), H(v) and K all fail there (the session is not fenced). H(i) already passes on
  the base, since child-3 names an undecodable record, and stays as a regression leg. The test
  names only symbols on its base and none this child adds. A compile failure on the red leg
  reports UNVERIFIABLE (`engine/scripts/run-verify.sh:533-541`). Record in `build-notes.md` how
  many ran red.
- **Invariant to restore:** no session the restored image held `Open` or `Completing` can
  publish after the pass. Every record such a session wrote (its parts, its staged residue, its
  segment records) has a named deleter installed in the same commit as its fence. No value the
  fence writes can exceed the store's value ceiling. Source: 0016 D-B and 1.4 (`:717-728`),
  `:665`, X57 (`:880`), `:2193`; `metadata.rs:546-549` (the ceiling every backend inherits);
  ADR-0045. SELF-TEST: fencing `Completing` without the `seg:` obligation leaves X57 open, and
  fencing it with `{session, parts}` fails G-sparse.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** child-2, child-3
- **Ordering note:** wave 3 of #809's split. child-3 is a prerequisite because this child extends
  its fence, report fields and doc paragraph; child-2 because this child reads its nonce.
  child-5 builds on this child. **#810 must depend on this child**, not on #809, which the split
  closes. Downstream, not work here: 0016's reaper and operator-abort `Completing → Aborting`
  rows (`:665`, `:2193`) still specify `{session, parts}` and hit the same ceiling. Flag that to
  #656 and #659, which own those doors; do not edit 0016 here.
- **Surfaces:** data
- **Difficulty:** high
- **Do model:** opus
- **Scope:** the `Completing` arm of the restore fence: one commit per session installing the
  bytes and records obligations beside the transition. Reading the attempt's `seg:` range to name
  what cannot be fenced cleanly (H). The second-pass re-check (K). Update `multipart.rs`'s retire
  rows table (`:3141-3150`) so the restore `Completing` fence is listed under `{session, all}`,
  with the reason. Extend the fence paragraph child-3 wrote in `06-runtime-view.md` §6.5 and the
  m4 blueprint's step 7 to `Completing` sessions. A `deferred: #659` marker belongs wherever the
  re-check could misread a range the future drain has half-deleted. Size budget: at most 7 files
  and under 80 KB of diff. / out of scope: the `Open` fence, and report fields beyond what G/H
  need (child-3); DST (child-5; the same rule as child-3 for `crates/dst/tests/custodian.rs`); the
  `{session, parts}` rows of other writers; #810; #659; #656, #658; #508; any edit to 0016 or an
  ADR.
- **Repro instruction:** on its base, seed a `Completing@3` session with its nonce, two `part:`
  records and `seg:<nonce>:3:0`, and run `reconcile_after_restore`. The session is still
  `Completing@3`, no `retire:` key exists, and the session is named as needing a human.
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/custodian/tests/restore_completing_fence.rs`. This is a **NEW** file:
  C4-verify's red comes only from an added `*/tests/*.rs` (`run-verify.sh:141-144`). Keep the
  doubles inside it, including the value-ceiling double.
- **Production reach:** the pass under test is the production `reconcile_after_restore`. Every
  session is seeded by the test, because no client can create one until #508. A live
  FoundationDB would refuse an oversized value; here the double enforces the same ceiling.
- **Citations expected:** Do must cite `path:line` on the target branch for every change. Peer
  callsites Do MAY open and mirror:
  * Child-3's fence and its test doubles, by symbol on the base.
  * `crates/core/src/multipart.rs:2021-2030` (the `Completing` variant) and child-2's group
    accessor; `:3319-3355` (`checked_against_key`: a `seg` group's epoch must equal its token's
    epoch); `:3296-3303` (the `all` wildcard's rule).
  * `crates/core/src/metadata.rs:1343` / `:1462` (`SegmentRecord` and its decode), `:1480`
    (`seg_key`), `:1505` (`seg_range_prefix`), `:1527` (`parse_seg_key`), `:546-603`
    (`MAX_VALUE_BYTES`, `flat_value_ceiling_crossed`).
  * `crates/custodian/tests/staged_protection.rs:154-400` (the hooked store double).
- **Prior-art check (triage cycles):** by path (`restore.rs`, `multipart.rs`), 2026-09-29 on
  `243241e`. No merged change fences sessions, and no open PR touches these paths. Rejected prior
  art: #809 iteration 1 used `{session, parts}` (overflow, leg G-sparse) and left the
  unreadable/stray `seg:` branches untested (legs H(iv), H(v)). #637 iteration 1 fenced
  `Completing` sessions without the `seg:` deleter. #664 iteration 1 had the second-pass bug
  (leg K). Do not repeat any of them unchanged.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR MAY
happen during the cycle. The PR MUST NOT be marked ready before sign-off accepts.
<!-- pdca:end child-4 -->

<!-- pdca:child child-5 -->
# dst: seeded Tier-0 coverage for the restore session fence (809.5)

> Child 5 of 5 of #809's split at its re-plan (2026-09-29); #809 is itself 664.2. Do reads ONLY
> this file. Keep the `- **Label:** value` lines. `path:line` citations are on `origin/main` @
> `243241e` (verified 2026-09-29). This bundle's base is `origin/main` **plus children 1–4**:
> both fence shapes are on it. Locate them by symbol.

- **Slug:** restore-fence-dst
- **Kind:** enhancement
- **Defect:** the restore session fence (children 3 and 4) is a new path. It races concurrent
  writers on the session record and installs obligations that lead to deletes, yet it has no
  seeded Tier-0 DST coverage, only scripted in-process interleavings. The repo's rule is "a new
  destructive or concurrent path lands with seeded Tier-0 DST coverage" (`AGENTS.md:188-190`).
  The existing restore properties (`crates/dst/tests/custodian.rs:1954-2200`) run the pass
  against ordinary inode publication, never a multipart session. The staged-handoff driver
  (`:2617-2625`, `:2850`) has only GC and reconstruction arms. #809 iteration 1's review raised
  this three times as blocking.
- **Success criterion:** `crates/dst/tests/custodian.rs` gains seeded properties that run the
  production `reconcile_after_restore` fence, registered with `dst_campaign_test!` and swept over
  the run seed (50 seeds under `cargo xtask dst`):
  **(D1) The fence and a concurrent session writer never both win.** A resurrected `Open@E`
  session, and in a second arm a `Completing@E` session with `seg:` records, face a concurrent
  writer landing at a seed-drawn instant during the pass. For `Open` the writer is a Complete
  fence to `Completing@E`; for `Completing` it is the root flip to `Completed`. On every seed,
  exactly one of them lands for that epoch. Either the session is `Aborting@E+1`, every
  obligation children 3 and 4 install is present and decodable, and there is no publication; or
  the writer's transition stands, no `retire:` key names that epoch, and the pass named the
  session. Never an obligation without the transition, or the transition without its
  obligations.
  **(D2) An ambiguous fence commit is settled by the next pass.** The fence's commit is answered
  as an unknown outcome and is applied whole or not at all, by a seed-drawn fate. The pass returns
  `Err`, and the `DANGLING` line for a dangling chunk seeded in the same store was emitted before
  it. A second pass leaves the session fenced with exactly one set of obligations, whichever fate
  the store took.
  **(D3) The windows are reached.** A reachability leg walks the writer's landing span in one run
  and asserts that both D1 outcomes occurred (the fence wins; the writer wins), as
  `prop_restore_two_readings_cover_the_divergence_window` does (`:2165`). A property that never
  reaches its interleaving proves nothing.
  **(D4) Demonstrated falsifiability, recorded in `build-notes.md`.** Temporarily break the fence
  on the builder's machine: split its transition and obligations into two commits, or drop its
  precondition on the session bytes. At least one seed of D1 or D2 must then fail. Paste the seed
  and the failure into `build-notes.md`, then restore the fence. None of the breakage ships.
  **(L) `cargo xtask ci` green**, with every existing property in the file unchanged in what it
  asserts. A span constant may move if the longer pass needs it; say so if one does.
- **Falsifiability:** no gate can produce a RED for this child. It adds tests only, over a fence
  already on its base, and a test-only patch has no production change for C4-verify to revert.
  D4 is where the red is shown: on the builder's machine, by breaking the fence on purpose. Check
  can re-run it. The harness is madsim (`--cfg madsim`, ADR-0009), and the gate sets that cfg
  for `crates/dst` (`engine/scripts/run-verify.sh:21-27`, `:155-180`).
- **Verification posture:** (a) net-new coverage: "red" is the property's absence. C4-verify
  runs **green-only** for this patch. It modifies an existing test file and adds none, so the gate
  runs the whole `wyrd-dst` crate under `--cfg madsim` with 50 seeds and passes on green
  (confirmed with `run-verify.sh --classify` on a synthetic patch, 2026-09-29: `CRATE crates/dst`,
  no `ADDED_TEST`). Do NOT add a new file under `crates/dst/tests/`. The gate would keep it on the
  red leg, find no production change to revert, see it pass, and FAIL the bundle ("passes
  without the fix"). What is built and exercised at Check: the properties themselves, over the
  production fence. The demonstrated red is D4's.
- **Invariant to restore:** every schedule the seed can draw between the restore fence and a
  concurrent session writer, or a commit whose outcome is unknown, ends with the session either
  fenced with all its obligations or left to the writer and named, never half of either. Source:
  `AGENTS.md:188-190` (test fidelity), `:178-180` (unknown commit outcomes); ADR-0009; 0016
  `:665`, `:717-728`.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** child-4
- **Ordering note:** wave 4 of #809's split; it needs both fence shapes on its base. After
  acceptance, add `Conflicts with: 682, 722` to this brief. Both edit
  `crates/dst/tests/custodian.rs`, and the proposal format admits only sibling labels in that
  field.
- **Surfaces:** data
- **Difficulty:** medium
- **Do model:** opus
- **Scope:** new seeded properties and their harness in `crates/dst/tests/custodian.rs`,
  mirroring the restore nemesis harness and the staged-handoff driver. Nothing outside that file.
  / out of scope: any production change. If a property finds a real defect in the fence, stop,
  record the seed and the failure in `build-notes.md` and report it, rather than fixing
  production here. Also out: any other test file; docs; any edit to 0016 or an ADR.
- **Repro instruction:** n/a (new coverage). On its base, `cargo xtask dst` passes, and no
  property in `crates/dst/tests/custodian.rs` runs `reconcile_after_restore` over a store holding
  an `mpu:` session.
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/dst/tests/custodian.rs`, an EXISTING file, extended. See Verification
  posture for why it must not be a new one.
- **Production reach:** the fence under test is the production pass. The store is the DST
  crate's simulated metadata store (`MemMeta`, `:106`), where every read and commit spans a
  simulated network hop. Sessions are seeded because no client can create one until #508.
- **Citations expected:** Do must cite `path:line` on the target branch for every change. Peer
  callsites Do MAY open and mirror, all in `crates/dst/tests/custodian.rs`:
  * `:1819` (`RESTORE_NEMESIS_SPAN`), `:1954-2140` (`restore_under_a_concurrent_writer`),
    `:2144-2200` (the two restore properties).
  * `:2617-2625` (`Driver`), `:2760-2850` (handoff session fixtures and `staged_handoffs_under`).
  * `:3907-4270` (property 16, the ambiguous-commit store double `AmbiguousSweepMeta` at `:3974`).
  * `:4998-5010` (`rand_seed`, `dst_campaign_test!`), `:5060-5200` (registrations and the seed
    sweep list).
  * The fence itself, by symbol on the base.
- **Prior-art check (triage cycles):** by path (`crates/dst/tests/custodian.rs`), 2026-09-29 on
  `243241e`. The last changes are `5377850` (#814), `f683dbe` (#813) and `dd81029` (the
  ambiguous sweep). #682 (AWAITING_SIGNOFF) and #722 (PLANNED) edit this file for segmented
  repoint and evacuation properties, and no open PR touches it. #809 iteration 1 only edited a
  fixture here and never ran the fence under DST.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR MAY
happen during the cycle. The PR MUST NOT be marked ready before sign-off accepts.
<!-- pdca:end child-5 -->
