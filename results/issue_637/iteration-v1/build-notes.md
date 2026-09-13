# Build notes — issue 637, staged-byte protection

Base: getwyrd/wyrd `main` @ `3969a3a` (the same commit as `origin/main` when this was built).
`patch.diff` applies cleanly to it (checked with `git apply --cached --check` against a scratch
index). Line numbers are on the patched tree unless marked `@3969a3a`. The patch touches 20
files: +5,773 / −207.

## What the patch does

Bytes a multipart upload has made durable but not yet published were invisible to every
maintenance pass. On the base, GC deletes them once any orphan mark's grace runs out
(`crates/custodian/src/gc.rs:214` @3969a3a). The post-restore pass marks them as stranded
(`restore.rs:383` @3969a3a). A drain calls a server that holds only staged bytes `Satisfied`
(`desired_state.rs:191-196` @3969a3a).

The reference set now has a second member, the **staged set**, kept separate from the committed
placements. Each maintenance pass makes proposal 0016's per-consumer decision from it
(`0016:820-871`). The set is built from bounded per-session ranges, never a global scan, and
read in the normative order `sidx:` → `part:` → committed inodes.

Three other pieces land with it:

- GC now walks the `orphan:` ledger in bounded pages that pick up where the last pass stopped.
  It commits its intent to reclaim before it deletes any bytes, and it reads all three mark
  shapes.
- A pending retirement protects its fragments through one keyed read. Fragment-less marks are
  swept, and a migration gate controls identity-keyed retirement.
- The post-restore pass fences every upload session the restored image brought back, and it
  writes the `restore:fence` generation record that a gateway reads.

## Where each decision lives

| Consumer / rule | What changed | Where |
|---|---|---|
| Staged set | `sidx:` then `part:` per session, from the bounded `mpu:` scan. A record that will not decode makes the set incomplete. A session record that will not decode keeps its fragments protected but is named. | `crates/custodian/src/staged.rs:110-204` (sidx `:135`, part `:162`) |
| Reference build | Staged set first, then inodes (source before destination) | `gc.rs:414` |
| GC protect | `protection()` adds `"staged"` and staged malformed placements; `definitely_references()` for the ledger | `gc.rs:336-358` |
| GC listing + lapsed leases | One listing per pass. A lapsed lease on a position with a mark is left to the ledger walk (one point `get` per candidate). The walk is called at `:248`. | `gc.rs:200-245` |
| GC ledger walk | 16 pages × 1,024 marks per pass, durable cursor, metadata-only decisions first, cursor durable before any delete | `ledger.rs:193-310` |
| Reclaim intent before destruction | CAS to `reclaiming` and commit, then `delete_fragment`, then delete the key; resume after a crash | `ledger.rs:467-515` (intent `:475`, bytes `:498`), `:519-540` |
| Keyed pending-retirement lookup | One `get` of `retire:bytes:<token>` per event, memoised per pass | `ledger.rs:388-403` |
| Fragment-less mark sweep | Only on this pass's own listing, past `W_repoint + W_write + δ` = 45 s | `ledger.rs:101-107`, `:416-420` |
| Orphan-identity migration gate | Marker `custodian:gc:orphan-identity`, written by a clean full sweep. Every mark protects until then. | `ledger.rs:117`, `:266-278`, `:301-307`, `:363-382` |
| Three mark shapes | Legacy decimal, `{orphaned_at_millis,event}`, `+reclaiming:true`; canonical decode; an undecodable value fails closed and is named | `crates/core/src/orphan.rs:149-289` |
| Drain / desired state | `genuinely_holds` also tests staged (both kinds); staged malformed placements join `PendingMalformed` | `desired_state.rs:201-206`, `:255-261` |
| Scrub | Walks committed-part fragments not already in `placed`, with the part's scheme; reports staged malformed placements | `scrub.rs:98-110`, `:139-164` |
| Reconstruction | Staged sites resolved before committed ones. Repair only while the session is `Open`; otherwise the obligation stays queued. Re-place under pre-mark → write with deadline → one CAS on session, part, pre-mark and `require_absent(desired:dserver)`. | `reconstruction.rs:178-207`, `:679-680`, `:1071-1222` (pre-mark `:1135`, write `:1174`, CAS `:1191`) |
| Rebalance | Doc only: staged bytes are not evacuated, and its answer stays separate from the drain's `Pending` | `rebalance.rs:60-67` |
| Restore | Fence phase first; staged skip counted; already-marked check by point `get` + decode | `restore.rs:357-381`, `:494-495`, `:530-545`, `:655-825` |
| Restore-fence record | `restore:fence` = `{generation, complete}`, advanced by CAS | `crates/core/src/multipart.rs:3714-3791` |
| Writer helpers in core | `SessionRecord::fenced_to_aborting`, `PartRecord::with_chunk_placement`, `RetirePayload::session_teardown` | `multipart.rs:2092`, `:2463`, `:3151` |
| CLI verdict | Counts for "left to a live upload" and "sessions fenced"; three new NEEDS-HUMAN paragraphs | `crates/server/src/cli.rs:1259-1260`, `:1303-1338`, test `:2924` |
| Docs | Architecture 05/06/08, plus the deployment runbook's post-restore section | `05-building-block-view.md:202`, `:206`; `06-runtime-view.md:31`, `:39-41`, `:74`; `08-crosscutting-concepts.md:45-51`; `m4-first-deployment-blueprint.md:585`, `:620-633`, `:695-702` |

`reconcile_step`'s signature is unchanged, and no `Cargo.toml` was touched.

## For sign-off: what I could not do, or chose, that you should weigh

These are the §6 items the brief says belong to Check. None is an external dependency.

1. **The `Completing` restore fence does not install `retire:records:{seg:<g>:<E>}`** (brief
   leg C, Open question 2). The session record from #716 carries no segment-group nonce:
   `PublishTarget` holds only `parent`, `name` and `epoch` (`multipart.rs:1850-1857`), so the
   records cannot be named. The session is still fenced to `Aborting@E+1`, with
   `retire:bytes:{session, parts}` in the same batch. That is enough to guarantee no
   resurrected session can Complete. A session whose attempt wrote segments
   (`segments_written > 0`) is named in `RestoreReport::segments_unretired`
   (`restore.rs:800`). That makes `needs_human()` true, prints its own NEEDS-HUMAN paragraph
   (`cli.rs:1316-1327`), and emits `action=segments-unretired`. The test asserts that the
   session is reported (`staged_protection.rs:889-900`). The segment records themselves are
   record residue with no deleter; no bytes are at risk. The real fix is for the session
   record (#716's shape) to carry the group nonce, and that is not this slice's to change.
2. **Leg F's "a committed segmented object IS evacuated" is not implemented.** Evacuating a
   committed segmented chunk means rewriting a `seg:` record under the pre-mark rule, which is
   `repoint_chunk`'s write path (#682, open). The test pins the current behaviour instead
   (`staged_protection.rs:1258-1313`): the pass refuses, moves nothing, returns
   `Reconciled::Blocked`, and the drain stays `Pending`. This is the one test that passes on
   the red leg, by design.
3. **The classification-sweep helper is local to the test file**
   (`staged_protection.rs:448-523`). #636 never shipped one, so the brief calls this a §6 item.
   All 20 tests run it at the end of their scenario. 18 use `assert_no_gaps`, which demands
   no gaps. The two leg-H tests call `unclassified` and demand that the only gaps are exactly
   the fragments under marks no pass can read (`:1509`, `:1559`).
4. **Open question 1:** `RestoreReport` gains five public fields: `staged_skipped`,
   `sessions_fenced`, `sessions_unfenced`, `segments_unretired`, `marks_unreadable`
   (`restore.rs:143-173`). The struct is not `#[non_exhaustive]`, and I did not add it. Adding
   the attribute later is itself a breaking change for anyone who builds the struct with a
   literal. Your call.
5. **Open question 3:** `W_write` = 20 s, `W_repoint` = 20 s and `δ_clock` = 5 s are local
   constants (`ledger.rs:91-107`), marked as #625's to reconcile. The 45 s deadline stays
   inside the 60 s deployed grace (checked at compile time, `ledger.rs:668-671`).
6. **Files outside the brief's listed scope.** The brief lists six custodian files. The patch
   also touches:
   - `crates/core`: a new `orphan.rs`, plus the `restore:fence` record and three writer
     helpers in `multipart.rs`;
   - two new custodian modules, `staged.rs` and `ledger.rs`;
   - `crates/server/src/cli.rs`;
   - the deployment runbook.

   Why core: see rejected alternative A1. Why the runbook: its list of reasons the
   post-restore command exits non-zero would otherwise be wrong (docs currency).
7. **`sidx:` entries are protected for sessions in every state**, not only `Open` as 0016's
   table words it (`staged.rs:19-24`). This errs toward keeping bytes: a drain waits for a
   teardown too, which W_session bounds. See A7.
8. **What the migration gate costs an operator.** After an upgrade, or after any post-restore
   pass (which deletes the marker, `restore.rs:362-368`), GC reclaims **nothing** on any mark
   until one clean full sweep of the ledger has completed. For a ledger under 16,384 marks
   that is the first GC pass (30 s at the default interval, `cli.rs:860`). An in-grace legacy
   mark on a still-referenced fragment delays it by up to one grace window (60 s). A
   1.78 M-mark ledger needs 109 passes (about 55 minutes). While the gate is closed, each
   held-back fragment is skipped with the reason `identity-migration`, and the gauges
   `gc_orphan_identity_gate_open` and `gc_orphan_oldest_waiting_millis` show it.
9. **Leg C's "a retried Complete is refused" is a raw batch.** No Complete verb exists until
   #508. The test commits the Complete fence's own precondition, `require(mpu == Open@E)` on
   the bytes the client read, and asserts `Conflict` (`staged_protection.rs:953-965`).
10. **How I ran the red leg.** I did not invoke `run-verify.sh` itself. It creates and resets
    its own worktree and branch next to the live repo (`engine/scripts/run-verify.sh:349-372`),
    which is outside the roots this beat may write to, and Check runs it anyway. Instead I
    applied its red-leg rule (`run-verify.sh:507-518`) inside the cycle worktree:
    - Kept the added test.
    - Reverted all 16 modified files to `3969a3a` with `git checkout HEAD --`. That covers the
      production files, the two modified test files and the docs.
    - Left the three added production files on disk. With both `lib.rs` files reverted they
      are no longer compiled, which has the same effect as the gate's `rm -f`.
    - Ran the gate's command, `cargo test -p wyrd-custodian --test staged_protection`, under
      `timeout 1800`.
    - Restored the fix from scratch copies and checked that `git diff HEAD` hashed identically
      to the patch saved before the revert (sha256 `466ef0cb…`).

    After that last red run I corrected one doc comment in `ledger.rs` (the page-budget
    arithmetic, `ledger.rs:79-86`). That gives the shipped `patch.diff` (sha256 `a8d6c8b5…`).
    The red leg reverts that file anyway, and the test file is byte-identical, so the red result
    carries over. The green leg and the full gate were re-run on the shipped tree.
11. **A gap I saw and did not fix, because it is out of scope.** The runbook's list of reasons
    the post-restore command exits non-zero never included the pending-ledger case #772 added
    (`RestoreReport::pending_unreadable`). I added the three reasons this patch creates and
    left that one for its own issue.

## Evidence

### Red leg and green leg (the C4-verify shape)

- **Red** (production reverted, final test file): `running 20 tests` → `FAILED. 1 passed; 19
  failed`, exit 101, no build error. All 19 failures are panics raised by assertions in the
  test file itself. Each one's message names the missing behaviour:

  | Test | Assertion line |
  |---|---|
  | `a_gc_keeps_staged_fragments…` | `staged_protection.rs:704` |
  | `b_…in_flight_owned…` | `:736` |
  | `b_…committed_part…` | `:760` |
  | `c_restore_marks_no_staged…` | `:839` |
  | `c_restore_fences…` | `:885` |
  | `c2_…` | `:991` |
  | `d_…` | `:1050` |
  | `e_reconstruction…` | `:1111` |
  | `e_a_staged_re_place…` | `:1179` |
  | `f_a_staged_only…` | `:1250` |
  | `g_…` | `:1365` |
  | `h_gc…` | `:1475` |
  | `h_restore…` | `:1539` |
  | `h2a` | `:1634` |
  | `h2b` | `:1660` |
  | `h2c` | `:1717` |
  | `h2d_identity…` | `:1787` |
  | `h2d_a_legacy…` | `:1831` |
  | `i_…` | `:1871` |

  G's red is its `.expect` on `Err(ScanCapExceeded { cap: 64, prefix: "orphan:" })`: the base's
  single ledger `scan` aborts `reconcile_step`. The one pass is item 2 above.
- **Green** (fix applied): `running 20 tests` → `ok. 20 passed; 0 failed`.

### Refuting my own test

- **(a) Does it go red with the fix reverted? Yes.** See above. I ran the red leg four times
  as the test file changed. Every run gave 19 of 20 red by assertion, the last one on the final
  file.
- **(b) Does it drive production code? Yes.** Every leg calls the production entry points:
  `reconcile_step` (seven arguments, unchanged), `reconcile_after_restore` and
  `reconciliation_status`. They run over in-memory `MetadataStore` and `ChunkStore` doubles,
  the same arrangement `crates/custodian/tests/gc.rs` uses. The staged records are real
  encoded bytes, each checked against the tree's own decoder for its namespace. No behaviour
  under test is mocked. The DST cases drive the same production functions over
  `SimTikvMetadataStore` under madsim.
- **(c) Does the fixture include the fault? Yes.**
  - Leg A seeds lapsed orphan marks on staged fragments. That is the evidence that makes the
    base delete them; without it the base keeps them anyway.
  - Leg C's image carries an `Open` session and a `Completing` session that has already
    written a `seg:` record.
  - Leg G seeds a ledger larger than a lowered scan cap of 64. Its head of in-grace marks is
    larger than one pass's budget, so a walk that always restarts at the first page never
    converges.
  - Leg I fails the intent commit. H2b crashes between the intent and the delete.
  - E's losing case fences the session from inside the D server, in the window after the
    rebuilt fragment lands and before the CAS.
  - The DST races interleave the real concurrent writer, and the coverage properties below
    prove each window is actually reached.

### DST: leg J, the I2 handoffs, and the paged walk under a concurrent delete

- Eight new `dst_campaign_test!` cases are appended to the existing
  `crates/dst/tests/custodian.rs:3433-3480`. The file keeps its `#![cfg(madsim)]`. There are
  four safety properties:
  - drain request versus intent (J(i), `:2644`);
  - both handoffs, source before destination (I2, `:2825`);
  - staged re-place versus session fence (J(ii), `:3030`);
  - the paged GC walk versus a concurrent unlink (G/I/H2b, `:3334`).

  Each has a coverage twin that asserts its window is reached:
  - `:2654`: both orders of drain versus intent;
  - `:2840`: the commit-between-reads window, the X67 window and the flip-between-reads window;
  - `:3036`: the lost-CAS-after-write window, and a win;
  - `:3340`: a delete landing between a pass's reference read and its ledger read.

  The coverage twins sweep every landing point (`STAGED_SPAN = 24`, `:2194`).
- **They ran.** The final `cargo xtask ci` runs `cargo test -p wyrd-dst (--cfg madsim)` with
  `MADSIM_TEST_NUM=50` (`xtask/src/main.rs:1573`, `:1607`). Output for `tests/custodian.rs`:
  `22 passed; 0 failed`, the eight new cases among them. So each new case ran for **50
  seeds**. The four safety properties also run on the eight committed regression seeds
  (`committed_regression_seeds_stay_green`, `:3500-3519`).
- **Mutation check** (run before the review edits; none of the mutated sites changed after it).
  I broke the production code five ways, ran the DST campaign, and restored each file (checked
  identical with `cmp`). Each broken version turned DST custodian tests red:

  | Mutation | Red tests (of 22) |
  |---|---|
  | M1: the drain ignores the staged set | 5 |
  | M2: `part:` read before `sidx:` | 3 |
  | M3: committed inodes read before the staged set | 2 |
  | M4: stale-mark drop without the "written before this pass" guard | 2 |
  | M5: staged re-place CAS without the session precondition | 3 |

- **One existing DST constant changed.** `RESTORE_NEMESIS_SPAN` went from 6 to 24 (`:1794`).
  The fence phase now runs before the restore pass's reference reads. That moved those reads
  later, so the existing coverage property `restore_two_readings_cover_the_divergence_window`
  stopped reaching its window within 6 ms. Widening where the nemesis may land restores
  coverage. The safety property next to it (`never_license_a_mark`) is unchanged.
- **One existing unit test changed.** In `crates/custodian/tests/segmented_map_restore.rs:643`,
  the `orphan:` half of a poisoned-scan test is gone, because restore no longer scans
  `orphan:` (it does point `get`s). The `pending:` half stays. The file carries a comment
  saying why.

### Full gate

`./engine/xtask.sh ci` (`cargo xtask ci`) on the final tree printed `xtask ci: all checks
passed` and exited 0. It ran 1,359 tests with 0 failures. The prose gates ran too: `typos`,
`lint_docs: OK`, and `render_site: wrote 99 page(s)` with the link audit OK. `cargo fmt
--check` and `clippy -D warnings` over all targets are part of the same run. New unit tests:
four in `orphan.rs`, four in `multipart.rs` (`writer_tests`), two in `ledger.rs`, plus the
extended CLI verdict test.

## Rejected alternatives, with their costs

- **A1. Putting the orphan-value codec in `wyrd-custodian`.** The custodian's `[dependencies]`
  are `wyrd-traits`, `wyrd-core`, `tracing` and `wyrd-telemetry`, with no serde. It would need
  two new manifest lines (`serde`, `serde_json`). A `Cargo.toml` edit is reverted on the red
  leg, so the crate under test would not build, and 19 assertion reds would become one build
  error. The brief forbids exactly that. Core already has `serde`, `serde_json` and `bytes`
  (`crates/core/Cargo.toml:18-22`), plus the retire-token grammar the `event` field has to
  reuse (`multipart::parse_retire_key`). In core the codec costs 0 manifest lines and no second
  copy of the token parser.
- **A2. Dropping a stale mark under its referencing record's CAS** (`require(inode == bytes
  read)`). The reference build would have to keep, for every placed fragment, the record that
  placed it and that record's bytes, through the whole ledger walk. Today it keeps a
  `HashSet<(DServerId, FragmentId)>` and drops the inode scan's values once the build finishes
  (`gc.rs:409-506`). Holding them means up to 1,048,576 committed records resident during the
  walk, instead of 16,384 marks, which is the unbounded footprint leg G exists to forbid.

  The chosen rule (drop a lapsed legacy mark written before the pass, `ledger.rs:374`) needs
  no extra precondition. The tree's legacy writers write the mark in the same batch that
  removes the reference (`crates/core/src/metadata.rs:1877-1899`, `:1986-2008`, `:2066-2079`).
  So if a reference disappears after the pass read it, the mark is rewritten with a stamp at
  least 60 s newer, and the drop's own `require(orphan == bytes read)` loses (`ledger.rs:428-455`).
- **A3. Re-stamping a stale mark instead of dropping it.** 0016 allows either. A re-stamp has
  to write an event identity, and a legacy mark over a still-referenced fragment has no event
  currently unreferencing it, so the identity would be made up. Dropping it is honest: the
  next real unreference writes its own mark.
- **A4. Reading the whole ledger each pass** (#508 attempt 7). A mark is about 67 B in the
  common case (a ~54 B key over a 13-digit legacy stamp) and at most 239 B (a 73 B key and a
  166 B value). So a 1.78 M-mark ledger is about 120 MB to 425 MB of key and value bytes per
  pass, before per-entry allocation overhead. The chosen budget is 16 pages × 1,024 marks =
  16,384 marks: about 1.1 MB, and at most 3.9 MB. One sweep of 1.78 M marks then takes 109
  passes, about 55 minutes at the default 30 s interval.
  - A one-page budget cuts the footprint to at most about 245 KB, but a sweep takes 1,739
    passes (about 14.5 hours). That slows reclamation and the migration gate 16-fold.
  - A 64-page budget is at most about 15.7 MB for a 28-pass sweep (about 14 minutes).

  Sixteen pages sits between the two, and the constants are named with their arithmetic
  (`ledger.rs:76-86`).
- **A5. Keeping the restore-fence state inside an existing record** such as `mpuctl`. Every
  admission CAS (every Create) would then carry the fence state and conflict with the restore
  pass's writes, and a gateway would have to decode the admission ledger to learn one boolean.
  A singleton key costs a gateway one `get` and touches nothing else.
- **A6. Fencing a `Completing` session by rolling it back to `Open`, then applying the `Open`
  fence.** 0016 rejects this (`0016:836-841`): it takes two batches, and a crash between them
  re-opens a session the restore has declared dead. It also does not solve the missing nonce
  (item 1).
- **A7. Protecting `sidx:` only for `Open` sessions**, as 0016's table words it. That needs a
  read of each session's state, which can be stale by the time its range is read. It would
  also hide a non-`Open` session's residue from the drain until the teardown walk marks it,
  so a drain could answer `Satisfied` over those bytes. The chosen version's cost is that a
  drain also waits for teardowns, which W_session bounds.
- **A8. Letting a marked position outrank the lapsed-lease input by reading the whole ledger
  into a map.** The base did this (`gc.rs:177` @3969a3a, and it fails past the scan cap). Now
  it is one point `get` per lapsed-lease candidate (`gc.rs:228-230`). Under the deployed
  `Defer` policy the candidate set is empty (`gc.rs:195-198`), so the deployed cost is zero
  reads.
- **A9. Restore's already-marked check with one ledger scan.** The base did this
  (`restore.rs:308` @3969a3a), and the scan fails past 1,048,576 marks. Now it is one point
  `get` per unreferenced fragment on disk (`restore.rs:530-545`): N reads for N strays.
- **A10. Merging the staged set into `placed`.** 0016 and the brief reject this, and leg F is
  the test. Merged, the drain status and the evacuation plan contradict each other on a
  staged-only server, and scrub would try to verify in-flight chunks against a scheme they do
  not have yet.

## Self-review against the target's review rubric (`AGENTS.md`, "Review rubric & protocol")

- **One clock per lifecycle.** Every orphan-lifecycle stamp and comparison uses the pass's
  `now_millis`, the deployment wall clock:
  - mark stamps, the grace test and the late-write deadline;
  - the pre-mark and the fragment write deadline;
  - the cleanup marker's value.

  The source is stated at `ledger.rs:59-64` and `reconstruction.rs:1174-1178`. The fence writes
  no timestamp.
- **Narrow seams.** No new trait or seam. The code uses only `MetadataStore` and `ChunkStore`,
  including the existing `put_fragment` deadline.
- **ADR-0045.** Every new value is validated when it is decoded (`decode_orphan_mark`,
  `decode_restore_fence`), with canonical-spelling round-trip tests. Values that cannot be
  decoded are never rewritten; they are named. This holds in the GC walk, in restore
  (`marks_unreadable`) and in the re-place (`emit_unreadable_mark`). Placement length is
  checked strictly on the maintenance path (`checked_fragments`).
- **No DST-reachable statics.** None were added; the statics gate passed inside `ci`. The
  test file's single `Once` is test-only, the #214 idiom.
- **Docs currency.** New persisted keys (`restore:fence`, `custodian:gc:orphan-walk`,
  `custodian:gc:orphan-identity`), the new mark shapes and the CLI's new NEEDS-HUMAN exits are
  documented. See the Docs row above.
- **Serialization identity.** Optional fields are omitted when absent (`event`,
  `reclaiming`). Decoding then re-encoding is byte-identical, and a test pins it
  (`orphan.rs:305-335`; `multipart.rs` `writer_tests`).
- **Absent entries.** Nothing is skipped silently:
  - an unreadable staged record makes the set incomplete, so nothing is reclaimed and nothing
    is certified;
  - an unreadable session record is named;
  - an unparsable ledger key is named;
  - a fence that could not complete leaves the generation incomplete and names the session.
- **Await discipline.** Every await is a `MetadataStore` or `ChunkStore` call, bounded by the
  implementation, as with every existing read in these passes. No task is spawned and no lock
  is held across an await.
- **Test fidelity.** These new destructive and concurrent paths have seeded Tier-0 DST
  coverage, with coverage properties showing each window is reached:
  - the paged walk and the reclaim intent, against a concurrent unlink;
  - the staged re-place, against a session fence;
  - the drain request, against a part intent;
  - both handoffs, against the reference build.

  The restore fence is not in DST: the runbook runs it with writers stopped. Its lost-commit
  path, a pass that dies before the fence lands, is covered in-process by `c2`
  (`staged_protection.rs:975-1003`).

No external dependency was missing: `typos` and the docs renderer are installed, and the
prose gates ran inside `ci`. Nothing was pushed and no PR was opened.
