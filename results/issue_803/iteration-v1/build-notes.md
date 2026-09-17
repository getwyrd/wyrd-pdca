# Build notes — #803 staged protection class in the shared reference set (662.1)

Target: `getwyrd/wyrd` @ `main` = `78f9859`. Every `path:line` below is on the patched tree
(`main` + `patch.diff`) unless marked "base".

## What changed, and why

The defect (brief): `ReferenceSet` held committed placements only, built from the `inode:` scan
alone (base `crates/custodian/src/gc.rs:383-413`, `:478-573`). A committed part's fragments and an
in-flight owned `sidx:` fragment were in no protected set, so GC reclaimed them once they carried
an `orphan:` mark past grace, and the post-restore pass marked a live upload's fragments stranded.

The fix gives the shared set a second, disjoint member and routes both destructive passes through
it via the one predicate they already share.

- `crates/custodian/src/gc.rs:662` — new `StagedSet { placed, held, unresolvable }`, the staged
  member. It mirrors the committed trio: a placement read and trusted (`placed`), a chunk named
  but not trusted (`held`, the whole chunk is protected), and a record that cannot be read at all
  (`unresolvable`, the set is incomplete).
- `gc.rs:451` — `ReferenceSet.staged: StagedSet`. Kept out of `placed`, `malformed` and
  `unresolvable`, because scrub (`scrub.rs:88`, `:95`, `:114`, `:205`) and drain status
  (`desired_state.rs:188`, `:225-244`) read exactly those three. They get the same answers as
  before, as the brief requires. Merging would also be 0016's own listed failure mode
  (`0016:881`).
- `gc.rs:468` `protection()` — two new reasons, `staged` (a staged record places the fragment) and
  `staged-malformed` (a staged record names the chunk but can't be trusted), ordered after the
  committed reasons and before `incomplete-reference-set`, so the audit trail names the rule that
  actually held.
- `gc.rs:494` `is_incomplete()` — committed `unresolvable` OR staged `unresolvable`. Used by
  `protection()` and by GC's `Blocked` outcome (`gc.rs:378`). Scrub and drain keep reading the
  committed `unresolvable` field directly, so an unreadable staged record blocks GC and restore
  only (brief scope).
- `gc.rs:552` — `referenced_fragments` reads the staged class **first**, then the `inode:` scan.
  `gc.rs:789` `staged_fragments`: one `scan("mpu:")`, then per session `scan(sidx:<id>:)` and then
  `scan(part:<id>:)`. That is the `sidx:` → `part:` → `inode:` source-before-destination order
  (`0016:782-800`, X67 `0016:2596`), with bounded per-session ranges and no global `part:`/`sidx:`
  scan (`0016:801-805`, `:890`).
- `gc.rs:696` `add_owned`: the chunk comes from the **key** (`parse_sidx_key`). A key naming no
  chunk goes to `unresolvable` (E(i)). A key that names its chunk but has a value that won't decode
  becomes a hold on that chunk (E(ii)). `gc.rs:723` `add_part`: the chunks come from the
  **value**, so a value that won't decode goes to `unresolvable`; the key is never parsed.
  `gc.rs:740` `add_chunk`: placement length must equal `fragment_count()` exactly, otherwise the
  chunk is held.
- GC emits every held and unresolvable staged record on `wyrd.custodian.gc.audit`, from the loop
  and not the shared builder (`gc.rs:241`, `:260`, emitters `:1234`, `:1251`), with new counters
  `gc_unresolvable_staged_records` and `gc_staged_malformed_records`.
- `crates/custodian/src/restore.rs:323` — staged unresolvable records join the same `unreadable`
  union as unreadable committed maps. That puts them in `RestoreReport::unresolvable` (E(i)), sets
  `incomplete`, so nothing is marked, and names each one on `wyrd.custodian.restore.audit`
  (`restore.rs:780`, `:962`). `restore.rs:328` — held records are named on the audit seam
  (`restore.rs:795`, `:978`), with the required `// deferred: #664` marker at `restore.rs:326`: no
  report field changes, and whether a held record sets `needs_human()` is #664's call. The mark
  gate at `restore.rs:408` is unchanged code; it now covers staged fragments because
  `ReferenceSet::protects` does.
- Docs: one paragraph in `docs/design/architecture/06-runtime-view.md:78` (§6.7 step 2), as the
  brief asks.

### Two edits outside the brief's named files, and why

- `crates/server/src/cli.rs:1263`, `:1331-1340`, `:1360`, `:1369`, test `:3009`, plus
  `docs/design/architecture/m4-first-deployment-blueprint.md:610-611`. E(i) makes restore put
  staged record keys into `RestoreReport::unresolvable`. The restore CLI prints that list as
  "{n} committed object(s) could not be READ … Their chunk maps are missing segments or will not
  decode", which would be false for a `part:`/`sidx:`/`mpu:` key. I changed the wording to
  "record(s)" and named both audit actions. That is 15 changed lines in `cli.rs`, including one
  unit-test string, and 2 in the blueprint runbook. The operator's view of this exact change should
  be true. If the human judges this out of scope, drop those two files: nothing else depends on
  them. It will touch the same `cli.rs` lines #664 edits for staged counters (#664 builds after
  this, so it just rebases).

## Alternatives considered and rejected (with costs)

- **Filter staged fragments inside GC only.** Passes leg A and fails leg B: restore would still
  mark them, and the next GC pass would delete them. This is the brief's self-test. Rejected.
- **Fold staged into `placed` / `malformed` / `unresolvable`.** A 0-line struct change, but scrub
  would then walk staged fragments (it verifies every `placed` pair against `schemes`, which staged
  owned entries lack), drain status would count staged bytes as held (that's #664's decision), and
  an unreadable staged record would blank scrub and drain status. Contradicts the brief's scope
  and 0016:881. Rejected.
- **A second builder for GC and restore only, leaving scrub/drain on a committed-only builder.**
  This would spare scrub and drain status the extra reads. The chosen design costs them `1 + 2×S`
  more metadata reads per call (S = sessions listed under `mpu:`). Today S = 0 (no client creates
  sessions before #508), so that is exactly one extra `scan("mpu:")` per scrub pass or drain-status
  query. The alternative is about 15 lines (a wrapper plus an `Option<StagedSet>` or a second
  struct), but it produces two definitions of "the reference set", and #663/#664 are planned to
  read the staged member of the shared set. The brief's invariant is "protected by the SHARED
  reference set every destructive pass reads", and its Difficulty line counts scrub and drain as
  readers of the shared builder. Rejected in favour of one builder.
- **Identity-fill an empty staged placement** (the committed rule, `ChunkRef::placement_is_valid`).
  An empty vector is a pre-M3 committed spelling; staged records are born full-length
  (`0016:828`), so here it can only be damage. Identity-fill would protect `(i, i)` pairs only and
  leave a copy elsewhere open to reclaim. Holding the whole chunk is the fail-safe direction
  (ADR-0045 decision 3). Chosen: strict length equality, documented at `gc.rs:675-678`. Pinned by
  `e2_an_owned_placement_of_the_wrong_length…` (empty placement).
- **Decode the `mpu:` value and read `sidx:` only for `Open` sessions** (0016's exact set). The
  brief allows covering every listed session ("it only keeps more"). Not decoding the value
  removes a failure class (an undecodable session value) at the cost of keeping an
  Aborting/Completed session's owned fragments until the teardown deletes the records, which is
  bounded by `W_session`.
- **Parse the `part:` key.** Protection needs only the value's chunk list, so a misspelled
  `part:<id>:…` key whose value decodes still protects its chunks. That is the fail-safe
  direction; nothing is skipped silently.

## Known limits (by design, not gaps in this slice)

- A `part:`/`sidx:` record whose session has no `mpu:` record is not read (per-session ranges
  only; 0016's "sessions that still exist"). By protocol a session is deleted only after its
  retirement drains its records; residue is the reaper's (0016 decision 6).
- Scrub and drain status don't read the staged class yet (#663, #664). The runtime doc says so.

## Tests

### `crates/custodian/tests/staged_protection.rs` (NEW, 11 tests)

It drives production `reconcile_step` + `GcContext` and `reconcile_after_restore` over in-memory
doubles:

- `Meta` (`:102`): an ordered map with a `scan` cap (`ScanCapExceeded` past it), `scan_page` built
  on the seam's page helpers, a log of read prefixes, and one armed `Handoff` (`:115`) that lands
  atomically right after the first read that could observe it (`:160`).
- `MemDServer`/`Fleet`: four D servers.

Every seeded valid record goes through `decode_session_record` / `decode_part_record` /
`decode_owned_entry` and re-encodes byte-identically first (`:465`, `:487`, `:513`). Deliberately
bad records are asserted to be rejected by the decoder or key parser first. Every protection leg
has an unprotected control that the pass does reclaim or mark. The file names no symbol this
slice adds: audit seams are checked by the record key appearing on the right `target`
(`audit_names`, `:383`).

- A `:622` — committed part (RS(2,1) over d0/d1/d2) + owned entry (d3), all marked past grace →
  GC keeps all four, marks intact. Controls reclaimed: a copy of fragment 0 on d3, which the
  placement doesn't name, and an unrelated fragment.
- B `:654` — restore marks none of the four and marks both controls (`stranded_marked == 2`); a GC
  pass past grace then keeps the four and reclaims the controls.
- C1 `:744` / C2 `:762` — part-commit and publication handoffs land right after the first read of
  either class; the marked fragment survives, the control is reclaimed, and the handoff is proven
  to have landed (`fired_after` is set, source gone, destination present).
- D `:798` — cap 8; 5 sessions × (2 parts + 2 owned) = 10 records under each of `part:` and
  `sidx:`. The pass succeeds, all 20 staged fragments survive, the control is reclaimed, and every
  read under `part:`/`sidx:` is exactly one session's range.
- E(i) `:902`, `:915`, `:931` — undecodable `part:` value, `sidx:<id>:000001:not-a-chunk`,
  `mpu:not-an-upload-id`: restore marks nothing (even a genuine stray), names the record in
  `unresolvable`, `needs_human()`, and puts it on the restore audit seam; GC then reclaims nothing
  (a stray marked past grace survives), answers `Blocked`, and puts it on the GC audit seam.
- E(ii) `:1010`, `:1022`, `:1034` — wrong-length part placement (2 servers for RS(2,1)), empty
  owned placement, undecodable owned value under a key naming its chunk: restore marks none of the
  chunk's four fragments (including a copy on d3) but marks an unrelated stray; `unresolvable`
  stays empty; the record is on the restore audit seam. GC past grace keeps all four, reclaims the
  stray, answers `Changed`, and the record is on the GC audit seam.

**Red → green (the project's own C4-verify runner, `engine/scripts/run-verify.sh`, on a clean
`origin/main` = `78f9859` worktree):** `GREEN … 11 passed`; `RED … 0 passed; 11 failed`;
`run-verify.sh: PASS — red without the fix, green with it (11 test(s) ran red).` **11 tests ran
red, all by assertion.** Failing lines on the red leg: A `:632`, B `:661`, C1/C2 `:720`, D `:831`,
E(i) ×3 `:869`, E(ii) ×3 `:971`. Each is an `assert!` on the property: fragment still present, or
fragment not marked. None is an unwrap or a compile error.

**Mutation evidence that the legs bind the specific claims** (each run with the test unchanged,
production mutated, then restored):

| Mutation of the fix | Result |
|---|---|
| read `part:` before `sidx:` (per session) | only C1 red |
| read the staged class after the `inode:` scan | only C2 red |
| global `scan("part:")` filtered by session | only D red (`ScanCapExceeded { cap: 8, prefix: "part:" }` fails the pass) |
| global paged `scan_page("part:")` walk | only D red (the read-range assertion names `"part:"`) |
| a held record treated as unresolvable | the 3 E(ii) red (the unrelated stray is no longer judged) |
| identity-fill an empty staged placement | only `e2_an_owned_placement_of_the_wrong_length…` red (the d3 copy is marked) |

### `crates/dst/tests/custodian.rs` — property 13 (appended; no new DST file)

- `:2562-2911`: `StagedMeta` tap over `SimTikvMetadataStore` records what each `sidx:` / `part:` /
  `inode:` read answered and adds one simulated hop after the store answers (the response trip
  back). That spaces the build's three observations 2 ms apart (at 6, 8 and 10 ms after the writer
  starts, with the pass starting at 4 ms). So a landing strictly between two reads is reachable
  whatever order the scheduler picks for a tie. Without it the three reads are 1 ms apart and every
  in-between landing would be a scheduler tie (the restore property gets its gap from the
  `pending:` scan between its two readings).
- `gc_pass_across_staged_handoffs` (`:2705`): a genuinely concurrent writer does the part commit
  at `commit_at` ms, then the publication `publish_after` ms later, each as one batch, while one GC
  pass builds its set. It asserts the fragment is kept with its mark intact, the stray is reclaimed
  (`Changed`), the build read all three classes, and at least one read saw the chunk.
- `prop_gc_staged_build_under_concurrent_handoffs`: the seed picks `commit_at ∈ 0..=12`,
  `publish_after ∈ 0..=4`. Registered with `dst_campaign_test!` (`:3001`) and added to
  `committed_regression_seeds_stay_green`.
- `prop_gc_staged_build_reaches_every_landing` (`:3007`): walks `commit_at` 0..=12 ×
  `publish_after` {0, 2} and asserts all four schedules occur: part commit between the `sidx:` and
  `part:` reads, publication between the `part:` read and the `inode:` scan, both handoffs before
  the build, and both after it. Timeline analysis: before = commit_at ≤ 1 with publish_after 0;
  commit-between = commit_at 5; publication-between = (5, 0) or (3, 2); after = commit_at ≥ 9.
  Each has at least one landing with no tie.
- Results through `engine/xtask.sh dst` (50 seeds): **green with the fix** (custodian suite
  `18 passed`). **Red on base**: the campaign leg and the regression-seed leg fail with "GC
  reclaimed a fragment a staged record or the published inode named throughout (part commit at
  7 ms … / 8 ms …)", and the coverage walk fails at "the build did not read all three classes".
  **Red with `part:` read before `sidx:`**: "the build's reads saw the chunk in NO class"
  (landings at 4/5/6 ms). All by assertion.

## The three refutation questions

- **(a) Genuine red?** Yes. `run-verify.sh` reverts every file the patch changes except the added
  test (`engine/scripts/run-verify.sh:510-516`), keeps the new test, and gets `0 passed; 11
  failed`, all on property assertions (lines above). The DST property also fails on base: the
  chunk is actually reclaimed for seed-drawn landings.
- **(b) Production path?** Yes. The tests call the public production entry points
  (`wyrd_custodian::reconcile_step` with `GcContext`, `wyrd_custodian::reconcile_after_restore`),
  which run the real `gc::reconcile` → `referenced_fragments` → `staged_fragments` and the real
  restore mark gate. Only the stores are doubles. Staged records are real bytes accepted by the
  production decoders, and marks are written by production `mark_orphaned`. No logic is copied
  into the test.
- **(c) Fixture includes the fault?** Yes. Every staged fragment carries an `orphan:` mark past
  grace, so the reclaim path is live and only protection stops it. The handoff is injected
  mid-build: the unit double lands it after the first read that could see it, and the DST writer
  lands it concurrently with the source-first order tested at every seed-chosen instant. The
  unreadable and untrusted records are in the store the passes read. Controls prove each pass
  still acts.

## Commit-readiness

- `cargo fmt --all` applied and `--check` clean; `typos` clean on every changed file;
  `docs/publishing/tools/lint_docs.py` clean.
- **Full `cargo xtask ci` through `engine/xtask.sh ci`: `xtask ci: all checks passed`.** That run
  covered typos, docs lint and render, gitlink and unsafe guards, fmt, clippy `--workspace
  --all-targets`, build, `cargo test --workspace` (including the 11 new tests and the updated
  `cli::tests::restore_verdict_names_the_blocking_records…`), machete, deny, statics, deploy-guard,
  and DST clippy + test at 50 seeds with property 13. The one change made after that run is the
  wording of one DST doc comment (`STAGED_COMMIT_SPAN`); fmt and typos were re-checked after it.
  (An earlier run of the same gate stopped at `typos` on a test name spelled `…owned_entrys…`,
  which I renamed before the passing run.)
- `patch.diff` reverse-applies cleanly on the worktree whose HEAD is `78f9859`, so it applies to
  the base as-is.

## External dependencies

`typos` and `docs-renderer` (the brief's list) were both present and exercised by the gate. No
dependency outside the brief's list was needed, so there is no NEEDS-HUMAN external-dependency item.
