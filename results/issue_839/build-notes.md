# Build notes — #839 (809.1) restore-staged-report

Target: getwyrd/wyrd @ main. Worktree HEAD `36f006d` (origin/main; one merge past the brief's
`243241e` — #844 touches only `crates/core/src/metadata.rs`, so every `restore.rs` / `cli.rs`
line the brief cites is unchanged). "base" lines are on `36f006d`; "new" lines are in the
patched tree.

Size: 4 files, `patch.diff` ≈ 46.7 KB. The first cut was 53.9 KB, over the brief's "well under
50 KB". I trimmed it by shortening doc comments and assertion messages and by not rewrapping the
whole CLI summary string. No assertion or behaviour was dropped.

## What changed, and why

### `crates/custodian/src/restore.rs`

- **`RestoreReport::staged_skipped: usize`** (new `:136`, right after `pending_skipped`, base
  `:126`). 0016 `:823` asks for it beside `pending_skipped`. Its doc states the counting rule:
  first matching protection, in the pass's own order.
- **`RestoreReport::staged_untrusted: Vec<String>`** (new `:158`). The staged records the pass
  read but could not trust (`StagedSet::held`, `gc.rs:1344`). Named via `object_name`
  (`gc.rs:1788`), once per record, in raw-key order.
- **`is_clean()`** (base `:197`, new `:220`) also requires `staged_untrusted` to be empty.
  `needs_human()` (base `:212-217`, new `:250`) is unchanged in behaviour. Its doc comment gains
  the exception paragraph (new `:239-249`), as leg H-iii asks. That paragraph:
  - rests the exception on the human's 2026-09-18 decision (#664 plan revision), not on the
    predicate's own rule;
  - states the condition (the fence, #841/#842; unreachable before #508);
  - points at #811 (keep-on-doubt protects user data, not residue) and #659 (drain not decided).
- **The mark gate** (base `:435-441`) is split in two:
  - the committed half (`incomplete || referenced || appeared`, new `:471`) still skips
    uncounted;
  - the staged half follows and counts (new `:480-483`).

  Resulting order: incomplete/committed → staged → displaced (base `:454-462`) → pending (base
  `:489-491`). Every kept fragment lands in at most one counter. The "Staged counters are #664's"
  comment (base `:433-434`) is replaced with the rule.
- **`attribute_staged`** (base `:813-827`, new `:856-872`) now returns the untrusted record names,
  deduplicated by raw key in a `BTreeSet<&[u8]>`. It still emits the same per-chunk audit line
  (`emit_untrusted_staged`, base `:1004-1013`, unchanged). The `deferred: #664` marker (base
  `:819-821`) is replaced with a pointer to `needs_human()`'s exception. The caller keeps the
  result (new `:389`) and puts it on the report (new `:417`). It is attributed at the same point
  as before: right after the staged read, before any later store read.
- **`emit_summary`** gains `staged_skipped` and `staged_untrusted` (new `:1073`, `:1075`).
- The module doc (base `:71-77`) and the `reconcile_after_restore` doc (base `:241-244`) now say
  the untrusted record is named in the report and what the class keeps is counted.

### `crates/server/src/cli.rs`

- **Summary line** (base `:1257-1284`, new `:1260-1295`) adds two counts: "N kept for multipart
  uploads' staged records" and "N staged multipart record(s) untrusted".
- **Informational line** (new `:1370-1386`), only when `staged_untrusted` is non-empty. It names
  the records through `named_records` (base `:1389`), as the unreadable paragraph does (base
  `:1346-1358`). It says:
  - the pass held those chunks and marked none of their fragments;
  - it did not check that the staged bytes survived the restore;
  - a damaged record points at a bug or corruption and blocks every drain
    (`06-runtime-view.md:82`);
  - the run is not a clean bill;
  - it does not change the exit status.

  It never says "NEEDS-HUMAN" and promises no cleanup (#659 has not decided).
- The exit status is still `report.needs_human()`. The `RestoreVerdict.lines` doc (new `:1233`)
  and `restore_verdict`'s doc (new `:1257-1259`) say where the informational line goes and why.
- **Tests** (green-only, per the brief):
  - The agreement test (`restore_needs_human_agrees_with_every_paragraph_it_prints`, new `:2930`)
    gains an `untrusted` case with human = false. It still requires "NEEDS-HUMAN appears iff the
    status is set", so the informational line is pinned as not a NEEDS-HUMAN paragraph. It also
    asserts that such a report is never clean and that its records are named. `routine` gains
    `staged_skipped: 2`, which pins that kept fragments are not a finding.
  - The new test
    `restore_verdict_counts_staged_skips_and_names_untrusted_staged_records_as_information`
    (new `:3039`) pins:
    - the two summary counts and "complete";
    - the note's required phrases, including both record names;
    - no "NEEDS-HUMAN" and no cleanup wording;
    - no note, and `is_clean()`, for a report with only `staged_skipped`.

### `docs/design/architecture/m4-first-deployment-blueprint.md`

Step 7 (base `:599-624`) gains a paragraph (new `:626-633`). The summary counts staged-kept
fragments. An untrusted staged record is not an exit-status bill; it gets its own named line, and
the run is not reported clean. This follows "Docs currency" (`AGENTS.md` rubric): the CLI's
output changed. `06-runtime-view.md` is left to child-3, per the brief.

### `crates/custodian/tests/restore_staged_report.rs` (NEW)

It has in-file `Meta` / `Disk` doubles (trimmed from `staged_protection.rs:164-433`: no hooks, no
fault injection, same scan-cap and paging behaviour) and the audit capture (from
`staged_protection.rs:832-897`). It names only base-visible symbols, and it reads the new fields
through `Debug` (`debug_field`, `:294`).

- Sessions: one helper, `open_session` (`:234`), with no `segment_nonce` and no
  `decode_session_record` self-check, as the ordering note asks.
- Part records: `part()` (`:255`) spells the canonical encoding
  (`staged_protection.rs:619-640`). It is not round-tripped through `decode_part_record`, which is
  outside the brief's symbol list. Instead, each leg asserts `report.unresolvable.is_empty()`, so
  a record that did not decode fails by name.
- **E** (`:389`). One `Open` session with four part records:
  - staged-only (0xE1, 0xE2);
  - staged ∧ pending (0xE3 plus a live lease);
  - staged ∧ displaced (0xE4). This is the `restore_reconcile.rs:685-715` shape made RS(2,1): the
    committed map places fragment 0 on server 0, which lacks it, and the part record places it on
    server 3, where it is. Fragments 1 and 2 are where both say, so the chunk is under-replicated,
    not misplaced, and the fixture has no NEEDS-HUMAN finding;
  - committed ∧ staged at the same server (0xE6).

  Plus one stray (0xE5). Asserts `staged_skipped: 4`, `pending_skipped: 0`, `displaced_kept: 0`
  and `stranded_marked == 1`. It checks the store too: the stray carries an `orphan:` mark, and
  no kept fragment does.
- **H-iii** (`:498`).
  - Session 1: one part record naming two chunks, each with a wrong-length placement (1 and 2
    servers for RS(2,1)). The reader holds per chunk (`gc.rs:1436-1451`), so this record appears
    twice in `held`. The exact list `["part:a1…:000001"]` therefore also binds "named once, by
    record".
  - Session 2: two trusted part records.
  - A premise assert pins that nothing else is found, so `!is_clean()` discriminates.
  - (a) exact list, plus an explicit check that no session-2 key is in it. (b) `!needs_human()`
    and `!is_clean()`. (c) no `orphan:` mark on any held fragment, and the
    `untrusted-staged-record` audit event names the record.
  - Also `staged_skipped: 8` (6 held + 2 trusted). Held-chunk protection is part of the staged
    class (`StagedSet::protection`, `gc.rs:1355-1365`), so the brief's rule counts it there.

## What I ruled out

- **Folding untrusted records into `unresolvable`.** One line, but it would withhold every mark
  in the fleet (`incomplete`, base `:383`), flip the summary to INCOMPLETE and set
  `needs_human()`. That contradicts the human's decision and H-iii (b).
- **A separate counter for held-chunk fragments.** The brief defines one `staged_skipped` over the
  staged class. The record-key list already tells the held case apart.
- **Building `staged_untrusted` at report construction from `staged.held`.** Same size (~5 lines
  either way). I kept it in `attribute_staged` so the untrusted names are decided in one place,
  beside the audit emit for the same records.
- **Counting inside `StagedSet` / changing `gc.rs`.** Out of scope per the brief, and not needed:
  `held` already carries each record's key.

## Red → green (project runner: `engine/xtask.sh` → `cargo xtask ci`)

Logs are in `$PDCA_SCRATCH/pdca-builder-839-logs/`.

- **RED** (`red-ci-final.log`; an earlier red run with the pre-trim test text gave the same
  result). The three production files were set back to base `36f006d` and the final test file was
  added. `cargo xtask ci` passed typos, docs, guards, fmt, clippy and build, so the test file is
  lint-clean on base. It then stopped at `cargo test --workspace`.
  - `restore_staged_report`: **2 tests ran, 2 failed, both by assertion** (no compile failure).
  - E: `staged_skipped` was absent (`None` vs `Some("4")`). The base report was
    `RestoreReport { stranded_marked: 1, already_marked: 0, pending_skipped: 0, pending_unreadable: [], displaced_kept: 0, dangling: [], misplaced: [], under_replicated: [228], unresolvable: [] }`.
    So the fixture's premise holds on base: the four staged fragments were skipped uncounted, the
    stray alone was marked, and chunk 0xE4 is under-replicated, not misplaced.
  - H-iii: "no untrusted staged record named". The base report was all zero/empty, so base
    `is_clean()` is **true** (`restore.rs:197-199`: stranded 0, under_replicated empty,
    needs_human false). Arm (b) is red on base too; the test panics at (a) first.
- **GREEN** (`green-ci-final.log`). The full `cargo xtask ci` on the final tree printed
  **`xtask ci: all checks passed`**, exit 0. That run covers typos, docs lint/render, guards,
  fmt, clippy `-D warnings`, build, workspace tests, machete, deny, conformance, statics, the
  orchestrator guard and DST. This is leg (L).
  - `restore_staged_report`: 2/2 pass.
  - `cli::tests::restore_needs_human_agrees_with_every_paragraph_it_prints` and
    `restore_verdict_counts_staged_skips_and_names_untrusted_staged_records_as_information` pass,
    along with the other two `restore_verdict_*` tests.
  - No existing custodian test file needed a change: `restore_reconcile.rs` 17/17 and
    `staged_protection.rs` 36/36 pass as they are. The brief's "field-by-field assertions should
    not break" holds.
- Formatter: `cargo fmt --all` was run over the tree after the last edit, and the gate's fmt step
  is green. No other commit hooks are configured in the target beyond what `xtask ci` runs.

## Refute-your-own-test (forced)

- **(a) Genuine red?** Yes. The red runs are the fix reverted: the three production files were at
  base, and only the new test file was present. Both tests in it ran and failed by assertion, not
  by a compile error.
- **(b) Production path?** Yes. Both legs call the production
  `wyrd_custodian::reconcile_after_restore`. That runs the production `staged_fragments`,
  `referenced_fragments`, `committed_chunks` and the mark gate. Only the store and the D servers
  are in-memory doubles, which the brief allows. The CLI tests call the production
  `restore_verdict`.
- **(c) Fixture includes the fault?** Yes.
  - E seeds each overlap the counting rule is about: staged∧pending, staged∧displaced (server 0
    really lacks the fragment), committed∧staged, and a real stray. Moving the staged gate after
    the displaced or pending check would shift counts into `displaced_kept` / `pending_skipped`,
    and the test asserts both stay 0.
  - H-iii seeds a genuinely untrusted record that the production reader holds (`gc.rs:1436`). Its
    held fragments are on disk, including on server 3, which neither the truncated placement nor
    the identity fallback names. None of them is curated out.
