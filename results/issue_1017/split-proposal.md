<!-- pdca:split-proposal v1 -->
# Split proposal — issue 1017

> **Intake-cap override (wyrd-pdca-P1, INTEGRATION §11).** Granted by Eduard Ralph in the
> Plan session of 2026-11-23 for **#1017 only**: "override 1017; split it by outcome".
> Track `alpha` stood at **12/6** (room 0; need 2) when the split was drafted
> (`scripts/plan-cap --track alpha --need 2`). The override covers this one split and nothing
> else in the session.

## Why this slice is oversized

#1017 bundles two outcomes that share a reaper but not a defect.

- **A stuck `Completing` session gets out.** The reaper rolls back a session whose completer
  died holding the fence, and sends an over-age one straight to teardown. This part held up
  across three rounds: no reviewer or adversary could refute it.
- **A `Completed` session gets out after a full retry window.** The tombstone expires, and
  the time it expires from is stamped at publication. Every round-3 finding is here, and most
  of the patch growth (151 → 267 → 349 KB) came from here, because doing it right reaches back
  into the Complete path in `crates/core`.

The round-3 sign-off asked for this split. The human asked for it **by outcome**, so each
child is one exit:

- **child-1** is the `Completing` exit. It also carries the two budget items the old slice
  owned: the `G_orphan` bound and the `B_ops` calibration. Both are evidence for the budgets
  the reaper's new fences and teardowns run under. The calibration cannot stand as its own
  child: no Check gate runs FoundationDB or TiKV (`pdca.toml` `[gates]`), so a calibration-only
  child would have no criterion Check can make go red.
- **child-2** is the `Completed` exit, including the publication stamp.

## Wave sketch

The two children share no dependency: neither needs the other's code. They do edit the same
files: `ReaperPolicy` in `crates/custodian/src/reaper.rs`, `crates/dst/tests/custodian.rs`,
the multipart bullets of `08-crosscutting-concepts.md`, and `crates/server/src/custodian.rs`.
So child-2 declares `Conflicts with: child-1`. That keeps them in separate waves, and the
scheduler runs the lower id (child-1) first. That order suits both: child-1 is the settled
design and should land without waiting on child-2's open risks.

After acceptance the planner adds the edges that name non-siblings (`--accept` accepts
sibling labels only):

- both children: `Depends on: 1015` (the reaper pass they extend; COMPLETE, PR #1097);
- both children: `Conflicts with: 1016, 1018`. #1016 edits `crates/server/src/custodian.rs`,
  `custodian_gc.rs` and `06-runtime-view.md`. #1018 edits `crates/dst/tests/custodian.rs`,
  `08-crosscutting-concepts.md` and `06-runtime-view.md`, and builds the same
  `Completing → Aborting` edge for the operator;
- child-2: `Conflicts with: 670`. child-2 changes how Complete takes its publication stamp,
  and #670 is that API's first production caller;
- #1019: re-point `Depends on: 1017` to child-2's id. #1019 needs the per-session tombstone
  judgment and the foreign-`Completed` hold, which are both child-2's.

<!-- pdca:child child-1 -->
# custodian: a stuck `Completing` multipart session always exits — rollback at `W_completing`, over-age teardown, `G_orphan`, `B_ops` calibration (1017 part 1 of 2)

> Do reads ONLY this file. Split from #1017 (third slice of #625) on 2026-11-23, after three
> build rounds whose `Completing` design held. Plan of record: proposal 0016,
> `docs/design/proposals/draft/0016-multipart-commit-protocol.md`. Read before writing code:
> the exit table (`0016:569-579`), the `Completing` exits (`0016:1915-1932`), the reaper
> pseudocode (`0016:2188-2199`), the knob rows (`0016:1480-1481`) and the clock-lifecycle
> rows (`0016:2515-2520`). The sibling child (child-2) owns the `Completed` exit. Do not touch
> the tombstone.

- **Slug:** multipart-reaper-completing-exit
- **Track:** <leave as is — `pdca split --accept` writes the parent's track in>
- **Kind:** enhancement (design proposal)
- **Defect:** after #1015, the reaper judges `Open` sessions only. Every later state is skipped
  (`crates/custodian/src/reaper.rs:845-850`, "deferred: #1017").
  - **A dead completer's `Completing` session stays `Completing` forever.** It is never rolled
    back once its fence is older than `W_completing`. When it is over-age, it is never sent to
    `Aborting`. Its admission slot, and the `seg:` records that attempt wrote, are held for good.
  - **A foreign-clocked `Completing` session is skipped without being held.** A foreign-clocked
    `Open` session is held and named for an operator (`reaper.rs:826-843`). A `Completing`
    one only gets the generic foreign-clock event, and the pass reports nothing held.
  - **`G_orphan` has no owner.** The strict late-write bound is held at compile time against
    the server's private grace constant (`crates/server/src/custodian.rs:115`, `:124`). But 0016
    names the value as #625's knob (`0016:1480`). Nothing in `wyrd-custodian` owns it or
    documents it, and nothing pins the tick at which a fence's mark becomes reclaimable under
    GC's inclusive check (`crates/custodian/src/gc.rs:687-690`).
  - **`B_ops` rests on an assumed per-operation latency.** `SLOWEST_OP_MILLIS` is "assumed, not
    measured" (`crates/core/src/multipart.rs:5925-5931`), and 0016 assigns its replacement to
    #625.
- **Success criterion:** a NEW test file `crates/server/tests/multipart_reaper_completing.rs`
  drives the reaper through #1015's one-shot, `wyrd custodian --data-dir <dir>
  --reap-abandoned-uploads`, as a bounded subprocess. Every leg seeds raw records, drops the
  store, runs the one-shot, reopens the store, and asserts on durable state.
  **(D) `Completing` exits, both ways.**
  - A local-clock `Completing@E` session whose `fenced_at_millis` is older than `W_completing`,
    and which is **not** over-age, ends at `Open@E+1`. The same commit installs
    `retire:records:{seg:<g>:<E>}`.
  - A `Completing@E` session that **is** over-age (`now - created_at > W_session`) ends at
    `Aborting@E+1`, in **one** batch that installs both teardown obligations. It is never rolled
    back first (`0016:1924-1929`).
  - After the drain, that attempt's `seg:<g>:<E>:` range is empty, and a **later** epoch's
    segments are untouched.
  - A `Completing` session fenced inside `W_completing` is left exactly as seeded.

  **(E) The clock guard on `Completing`.** A foreign-clocked `Completing` session whose fence is
  far past `W_completing` keeps its state, epoch, `seg:` range and `retire:` records. It is
  **held and named**: one output line carries `reaper-session-held`, the `foreign-clock` reason
  and `mpu:<id>` together, as a foreign `Open` session's does.

  **(H) The profile guard.** With a mismatched `mpuctl.profile`, a stale `Completing` session is
  not rolled back.

  **(F) Teardown over the reaper's path is idempotent and waits for GC.**
  - Run the one-shot twice over a partly drained obligation of a session the reaper fenced. It
    converges to the same terminal state with **no double-decrement**.
  - An `Aborting` session's obligation has one position carrying a `reclaiming` mark. After the
    one-shot, the `mpu:` record, the count, the mark (byte for byte) and the obligation are all
    unchanged. Delete that key, as GC's blind delete would, and run the one-shot again. Then the
    position gets a fresh mark under `require_absent`, the obligation and the session are gone,
    and the count drops by exactly one.

  **`G_orphan` boundary** (C4-ci evidence, appended to the existing
  `crates/custodian/tests/gc.rs`): pin the tick at which a mark written by a fence's drain
  becomes reclaimable, and show `G_orphan > W_write + δ_clock` and
  `G_orphan > W_repoint + W_write + δ_clock` hold **strictly**. The deployed GC pass reads that
  same constant, so the test pins the deployed value, not a copy.
- **Falsifiability:** RED on child-1's base (#1015 folded on `main`). The `Completing` sessions
  are untouched, so D fails. The foreign `Completing` session is never named held, so E fails.
  The reaper never fences a `Completing` session, so F's fenced fixtures cannot reach teardown.
  H is vacuous on the base; evidence it with the negations below. The verify gate classifies on
  **added** test files (`engine/scripts/run-verify.sh:138-144`), so the new file is the
  discriminator. Round 3 earned a real red with this shape (6 of 8 legs failed without the
  fix). The file MUST compile against the base. It reaches production only through the CLI and
  raw seeded records, and imports no symbol this slice adds. A test calling a new symbol cannot
  build on the red run, which the gate reports as UNVERIFIABLE, not red. The one-shot runs
  in-process over redb on the base toolchain.
- **Invariant to restore:** every state in 0016's exit table (`0016:569-579`) has an actor that
  drives it out without a client. Nothing a teardown orphans is reclaimed before a write
  authorized ahead of the fence can land (`0016:1480`, `0016:2518`). After this child that holds
  for `Completing` as well as `Open`. C-1 applies.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:**
- **Conflicts with:**
- **Ordering note:** after acceptance the planner adds `Depends on: 1015` and
  `Conflicts with: 1016, 1018`. child-2 conflicts with this child and runs after it. This child
  owns:
  - the `Completing` arms in `crates/custodian/src/reaper.rs` and their unit tests;
  - `G_ORPHAN`'s definition in `crates/custodian/src/gc.rs`;
  - in `crates/server/src/custodian.rs`, **only** the GC grace constant (`:115`) and its
    compile-time checks (`:119-124`), so the deployed pass reads `G_orphan`. This exception
    was granted at the round-2 sign-off.
  - in `crates/core/src/multipart.rs`, the `pub` budget constants and `SLOWEST_OP_MILLIS`'s
    doc. Any `Completing`-rollback constructor goes beside `SessionRecord::completing_teardown`
    (`:2767`), and build-notes say why it is in core;
  - `crates/metadata-conformance/src/lib.rs`, the `conformance.rs` files of
    `crates/metadata-fdb` and `crates/metadata-tikv`, their dev-dependencies, and the
    mechanical `Cargo.lock` change;
  - `crates/dst/tests/custodian.rs`, `crates/custodian/tests/gc.rs`, and the new test file;
  - the `Completing` and `G_orphan` text in `docs/design/architecture/08-crosscutting-concepts.md`.
- **Surfaces:** data
- **Difficulty:** high
- **External dependencies:** `typos`, `docs-renderer`, `docker`, `libfdb_c loadable`, `fdb headers (bindgen)`, `fdb cluster healthy`, `openssl dev (pkg-config + libssl)`, `cmake (tikv-client build dep)`
  The binding criterion needs only the base toolchain. The backend tokens serve the off-Check
  calibration leg. The four `fdb` tokens are the FoundationDB conformance set. `openssl dev`,
  `cmake` and `docker` build and run the `tikv` feature.
- **Test file:** `crates/server/tests/multipart_reaper_completing.rs`, a **NEW** file, not
  `#![cfg]`-gated. Copy the one-shot driver and seeding helpers from
  `crates/server/tests/multipart_reaper.rs` into the new file: the store wrapper (`:89-130`),
  `reap_once_at` and its bounded runner (`:197-255`), the seeders (`:324-417`), and the output
  readers `reported`/`tally` (`:420-441`), which the held-line assertion needs. Do not share them: a shared `tests/` module would itself count
  as an added test file. Boundary ticks are unit tests beside the code. DST rows go into the
  EXISTING `crates/dst/tests/custodian.rs`. The calibration is appended to the existing backend
  `conformance.rs` files. Neither adds a new file, because an added `tests/*.rs` joins
  C4-verify's run.
- **Verification posture:** red→green at Check for D, E and F (see Falsifiability). Also:
  - **Boundary unit tests** (C4-ci): exact and neighbouring ticks for `W_session` on a
    `Completing` session and for `W_completing`.
  - **Negations** recorded in `build-notes.md`, each turning the named leg red:
    - (6′) skip the clock guard for `Completing` ⇒ E;
    - (15) roll back an over-age `Completing` session before aborting it ⇒ D;
    - (9) delete and decrement while a `retire:` obligation stands ⇒ F;
    - (8) on a scratch tree only, against #1062's code: treat `reclaiming` as a
      different-event mark ⇒ F's wait fixture;
    - drop the profile check for `Completing` ⇒ H.
  - **Seeded DST**, appended to `crates/dst/tests/custodian.rs`:
    - **A crash at each step** of the rollback and of the over-age edge. A crash stops ALL I/O
      at the crash point, not just one transient error. Assert the state at that point, then
      converge with the count at its true value.
    - **The Complete/rollback race, in both interleavings,** over a **multi-segment** upload,
      not a one-chunk flat one. The flip must land between the rollback's read and its commit,
      with no fallback publish. When the rollback wins, the attempt's staged segments drain.
      When the flip wins, they stay with the published root.
    - Not here: the drain-vs-GC race and drain crash rows (#1061/#1062); X56, two drainers
      (#660); any tombstone row (child-2).
  - **Off-Check: the `B_ops` calibration.**
    - **Fixture.** One `#[test]` per networked backend, appended to the existing
      `crates/metadata-fdb/tests/conformance.rs` and `crates/metadata-tikv/tests/conformance.rs`
      and gated exactly as the trait-contract test beside it. Make the batch builder and the
      timing loop one `pub` function in `crates/metadata-conformance/src/lib.rs`, outside
      `run_all`. Take `MAX_BATCH_OPS`, `MAX_BATCH_BYTES` and `SLOWEST_OP_MILLIS` from
      `wyrd-core` (made `pub`, values unchanged) as a dev-dependency. Never copy them as
      literals.
    - **Two batches.** (i) `MAX_BATCH_OPS` `require_absent` puts on distinct keys, with small
      values. (ii) The same count, with `MAX_BATCH_BYTES` of puts in total, and the
      preconditions split between `require_absent` and exact-value guards on pre-seeded keys.
    - **Per-run identity.** Each run gets its own identity in the TiKV namespace, as the FDB leg
      already has.
    - **Assertion.** Each batch commits, and its slowest wall time over at least five runs is at
      most `MAX_BATCH_OPS × SLOWEST_OP_MILLIS`. Print every measurement and the operations
      actually sent.
    - **Running it.** The legs are already wired: the TiKV loop is `xtask/src/main.rs:301`,
      with retries at `:342`, and the FDB loop is at `:578`. Do runs `cargo xtask
      fdb-conformance` and `cargo xtask tikv-conformance` where the host allows.
    - **Evidence retention (decided 2026-11-23).** Keep the **first** attempt's raw output
      verbatim in `build-notes.md`: the command, backend version, host, date, every run's time
      and the operations sent. If no cluster was available, say so plainly and quote no
      figures. Otherwise Eduard Ralph runs both before merge and records the raw output in
      SUMMARY §9.
    - **Results.** If the slowest batch is within the bound, replace only the "assumed, not
      measured" wording, citing the retained figures. If it is over the bound, change no
      constant and raise a §6 item: re-valuing is #655's value set and the human's call.
    - Check only compiles the TiKV test (`host-tikv`).
- **Production reach:** the new arms live inside #1015's reaper pass, which the deployed
  custodian runs every repair-loop pass (`crates/server/src/custodian.rs:474`, `:590`). The
  one-shot the test drives goes through the same `reconcile_step` dispatch. No new entry point.
  Do records the call path.
- **Scope:**
  - the `W_completing` rollback (`Completing@E → Open@E+1`) and the over-age edge
    (`Completing@E → Aborting@E+1`), each committing its teardown obligations in its fencing
    batch, exactly as 0016's pseudocode spells them (`0016:2190-2199`);
  - the clock guard and the profile guard applied to both;
  - a foreign-clocked `Completing` session held and named, as a foreign `Open` one is;
  - the knob `W_completing` (15 minutes, measured from `fenced_at_millis`, never from part
    progress, `0016:1005-1009`) in `ReaperPolicy`, cited at its definition;
  - `G_orphan` (60 s) owned by `wyrd-custodian`, both strict bounds held at compile time, and
    the deployed grace reading it;
  - the calibration experiment;
  - the matching text in `08-crosscutting-concepts.md`.

  **Not this child's to choose:** `W_ref`, `B_bytes`, `B_ops`, the profile tuple and
  `MAX_SESSIONS` are #655's single value set, consumed unchanged.

  **Out of scope:**
  - anything about `Completed` sessions: the tombstone window, its stamp, the retirement leg's
    tombstone input, and the Complete path in `crates/core/src/multipart/complete.rs` (child-2);
  - the operator abort (#1018) and the terminal-expiry verb and alarm (#1019);
  - #1016's deployed shapes, and any other line of `crates/server/src/*`;
  - the `orphan:` mark guard (#1061/#1062) and X56 (#660);
  - ADRs, specs, and any edit to 0016.
- **Repro instruction:** on the base (#1015 folded on `main`):
  1. Seed a local-clock `Completing@E` session whose `fenced_at_millis` is past
     `W_completing` and whose `created_at` is inside `W_session`. Add its
     `seg:<g>:<E>:` records and `mpuctl`.
  2. Seed a second one that is over-age.
  3. Drop the store, run `wyrd custodian --data-dir <dir> --reap-abandoned-uploads`, and reopen.

  Both sessions are exactly as seeded.
- **Citations expected:** `path:line` for every change. Mirror these peers:
  - **The over-age edge** is the restore fence's `Completing` arm:
    `SessionRecord::completing_teardown` (`crates/core/src/multipart.rs:2767`) committed
    through `wyrd_core::multipart::abort::fence_batch` (`crates/core/src/multipart/abort.rs:83`).
    Build it as `plan_fence` does (`crates/custodian/src/restore.rs:1123-1131`, committed at
    `:1056`). #1018's operator abort crosses the same edge; reuse the builder, don't fork it.
  - **The rollback** is Complete's own release, taken by another actor:
    `release_batch` (`crates/core/src/multipart/complete.rs:1309-1320`) with
    `RetireObligation::attempt_segments`.
  - **The foreign hold** is the `Open` arm at `reaper.rs:826-843`.
  - **The one-shot test mechanics** are `crates/server/tests/multipart_reaper.rs:197-255`.
  - **GC's inclusive grace check** is `gc.rs:687-690`, and the late-write deadline is
    `gc.rs:281`.
- **Prior-art check (triage cycles):** by path. On #1015's branch, `crates/custodian/src/reaper.rs`
  has one commit (427e5ec, #1015), which defers these arms to #1017. The peer for the
  `Completing → Aborting` edge is #856 (merged, restore fence). No closed or rejected PR covers a
  `Completing` reaper arm. The only earlier attempts are #1017's own three unmerged rounds
  (`results/issue_1017/iteration-v1..v3`), whose `Completing` design held. Re-run by path on
  your base.
- **Disposition hint:** likely-fix
<!-- pdca:end child-1 -->

<!-- pdca:child child-2 -->
# custodian: a `Completed` multipart session exits after a full retry window — `W_tombstone` expiry and the publication stamp (1017 part 2 of 2)

> Do reads ONLY this file. Split from #1017 (third slice of #625) on 2026-11-23. Every
> round-3 finding lives here; they are binding requirements below, not hints. Plan of record:
> proposal 0016, `docs/design/proposals/draft/0016-multipart-commit-protocol.md`. Read before
> writing code: the tombstone rules (`0016:899-910`, `:965-969`), the exit table row
> (`0016:574`), the terminal predicate (`0016:2204-2215`), the tombstone clock row
> (`0016:2520`), the foreign-clock rule (`0016:2277`) and X33 (`0016:2572`). The sibling
> (child-1) owns `Completing`. Do not touch its arms.

- **Slug:** multipart-tombstone-expiry
- **Track:** <leave as is — `pdca split --accept` writes the parent's track in>
- **Kind:** enhancement (design proposal)
- **Defect:** a `Completed` multipart session never leaves the store, and the window that should
  end it is measured from the wrong instant.
  - **No expiry.** The deployed loop hands the retirement leg `Tombstone::Retain`
    (`crates/custodian/src/reaper.rs:304-317`, passed at
    `crates/server/src/custodian.rs:628`). So every `Completed` session keeps its record and its
    admission slot forever (`crates/custodian/src/retirement/sessions.rs:924`). Admission counts
    tombstones (`0016:965-969`), so completed uploads alone eventually fill `MAX_SESSIONS`.
  - **The input cannot judge a session.** `Tombstone` is one value for the whole pass
    ("the window has passed for every `Completed` session this pass reads",
    `crates/custodian/src/retirement.rs:386-400`). It reads no session's stamp and no clock
    identity, so it cannot hold a foreign-clocked session while expiring a local one.
  - **The stamp is taken too early.** The caller supplies `completed_at_millis` before Complete
    fences and writes segments (`crates/core/src/multipart/complete.rs:1465-1475`, `:1516`,
    `:1522`), and it is persisted unchanged (`:1834`). A slow Complete therefore publishes a
    tombstone with part or all of its window already gone. An identical retry straight after
    gets `NoSuchUpload` instead of `AlreadyCompleted` (round-2 reviewer probe).
- **Success criterion:** a NEW test file `crates/server/tests/multipart_reaper_tombstone.rs`.
  Every leg asserts on durable store state.
  **(G) Tombstone expiry, through the one-shot** (`wyrd custodian --data-dir <dir>
  --reap-abandoned-uploads`, as a bounded subprocess):
  - A local-clock `Completed` session whose `completed_at_millis` is older than `W_tombstone`,
    with no session `retire:` obligation and an empty `sidx:` range, has its record deleted and
    `mpuctl.count` lowered by exactly one.
  - A `Completed` session inside `W_tombstone` keeps its record. Read it back and assert
    `wyrd_core::multipart::complete_answer` gives `AlreadyCompleted` for its recorded
    fingerprint (`crates/core/src/multipart.rs:5623-5640`).
  - A `Completed` session past the window that still owns `sidx:` residue has that residue
    walked first: each named position orphan-marked, and the entries deleted. Only then is its
    record deleted, with the count lowered by exactly one. Run the one-shot until it
    converges.

  **(E) The clock guard on `Completed`.** A foreign-clocked `Completed` session far past
  `W_tombstone` keeps its record, and is **held and named**: one output line carries
  `reaper-session-held`, the `foreign-clock` reason and `mpu:<id>` together. Its only exit is
  #1019's operator expiry.

  **(L) The deployed loop expires tombstones.** Drive the deployed run loop in-process, as
  `crates/server/tests/custodian_gc.rs` does (`drive_deployed_loop` `:458-470`,
  `drive_deployed_loop_operator` `:491-517` and the fleet helpers they use, **copied** into the
  new file). A `Completed` session past `W_tombstone` with
  nothing owed is deleted, and the count drops by one. With the deployed input swapped back to
  `Retain`, this leg MUST go red. Round 3 found that swap passed every test.
- **Falsifiability:** RED on child-2's base (child-1 and #1015 folded on `main`). The base
  retains every tombstone, so G's expiry legs and L fail. The base never names a foreign
  `Completed` session held, so E fails. The verify gate classifies on **added** test files
  (`engine/scripts/run-verify.sh:138-144`), so the new file is the discriminator. It MUST
  compile against the base: it reaches production through the CLI, the deployed run loop's
  existing public entry, raw seeded records and base-era functions such as `complete_answer`,
  and imports no symbol this slice adds. Otherwise the red run cannot build, and the gate
  reports UNVERIFIABLE, not red. The **stamp** cannot be red-tested from that file: the API it
  changes is not on the base. Its evidence is the appended regressions and the DST row in
  `Verification posture`, each with a recorded negation.
- **Invariant to restore:** a `Completed` session answers an identical Complete retry with
  `AlreadyCompleted` for a full `W_tombstone`, counted from the stamp its own publication
  commit carries. After that, and once nothing is owed for it, its record and admission slot are
  released (`0016:574`, `:965-969`, `:2211`). A session stamped under a clock the reaper does
  not read is never judged (`0016:2277`). C-1 applies.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:**
- **Conflicts with:** child-1
- **Ordering note:** `Conflicts with: child-1` because both edit `ReaperPolicy`
  (`crates/custodian/src/reaper.rs`), `crates/dst/tests/custodian.rs`, the multipart bullets of
  `08-crosscutting-concepts.md`, and `crates/server/src/custodian.rs`. They have no build-on
  dependency; the lower id, child-1, runs first. After acceptance the planner adds
  `Depends on: 1015` and `Conflicts with: 1016, 1018, 670`:
  - **#670** is the first production caller of the Complete API this child changes. With the
    edge they never build blind on one base. If #670 is on your base, update its call site.
    Otherwise #670 builds on the new API.
  - **#1019's** `Depends on: 1017` is re-pointed to this child.

  This child owns:
  - the tombstone judgment in `crates/custodian/src/reaper.rs`, `retirement.rs` and
    `retirement/sessions.rs`, with their unit tests;
  - the Complete path in `crates/core/src/multipart/complete.rs` (and
    `crates/core/src/staged_publication.rs` if the stamp needs it), plus the appended
    regressions in `crates/core/tests/multipart_complete.rs`;
  - the tombstone DST rows in `crates/dst/tests/custodian.rs`, and the new test file;
  - the tombstone text in `08-crosscutting-concepts.md`, `06-runtime-view.md` and
    `05-building-block-view.md`.

  **Narrow exceptions in #1016's files**, granted at the round-1 and round-2 sign-offs:
  - **`custodian_gc.rs`:** change ONLY the timestamp or age in
    `deployed_retirement_leg_waits_for_the_whole_operator_fleet_and_keeps_completed_tombstones`
    (`crates/server/tests/custodian_gc.rs:1406`), so its completed upload is inside the window
    when asserted. Keep every partial-fleet assertion, and keep the change base-neutral.
  - **`crates/server/src/custodian.rs`:** correct ONLY the stale comment above the tombstone
    input (`:624-628`), which says every `Completed` session keeps its record.

  Nothing else in `crates/server/*`. If more is needed, raise a §6 item.
- **Surfaces:** data
- **Difficulty:** high
- **Do model:** opus
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/server/tests/multipart_reaper_tombstone.rs`, a **NEW** file, not
  `#![cfg]`-gated. Copy the one-shot driver and seeders from
  `crates/server/tests/multipart_reaper.rs` (`:89-130`, `:197-255`, `:324-441`) and the
  deployed-loop driver from `crates/server/tests/custodian_gc.rs` (`:337-517`). Do not share
  them. The stamp regressions are appended to the EXISTING `crates/core/tests/multipart_complete.rs`,
  and the DST rows to the EXISTING `crates/dst/tests/custodian.rs`.
- **Verification posture:** red→green at Check for G, E and L. The stamp is evidenced at C4-ci
  instead, because its API is not on the base (see Falsifiability).
  - **When the window starts (decided 2026-11-23).** 0016 puts the stamp in the flip batch
    (`0016:2520`) and judges `now - completed_at > W_tombstone` (`0016:2211`). So the stamp is
    read on the session's clock **after the last segment write**, and immediately before the
    flip batch is built and sent: nothing else is awaited between the reading and the send. The
    flip commit's own latency comes off the client's window. It is bounded by the per-commit
    transaction envelope (0016's `B` row, 5 s, against a 120 s window); document it as an
    accepted cost in the `W_tombstone` doc. A recovery read that lands after the window is
    0016's registered X33 (`0016:2572`), not a defect here. The reaper adds no margin beyond
    0016's predicate.
  - **Appended stamp regressions** (C4-ci), in `crates/core/tests/multipart_complete.rs`:
    - a Complete whose segment-write phase takes longer than `W_tombstone` records a stamp no
      earlier than the clock reading after its last segment write, so an identical retry right
      after publication answers `AlreadyCompleted`;
    - a delayed **successful** flip: the stamp is the reading taken immediately before the
      flip's send, not one taken before the segment phase or before the flip was built.
  - **Publish to the store that was verified.** The publication commit goes to the same store
    whose segment records Complete verified. No public API lets a caller verify segments in one
    store and flip in another. If Do splits publication into phases, a regression shows a second
    store is refused. Keeping publication behind the one store-owning call is the simpler way to
    meet this.
  - **Per-session clock identity** (unit tests, C4-ci). The tombstone judgment reads the reaper
    context's clock and `clock_source`, never a fixed name in `ReaperPolicy::default()`. A
    logical-clock context holds a wall-stamped tombstone as foreign and expires one stamped
    under its own name. Also test exact and neighbouring ticks of `W_tombstone`, and a stamp
    ahead of the reaper's clock by more than `δ_clock` being held.
  - **Seeded DST**, appended to `crates/dst/tests/custodian.rs`:
    - **The Complete/reap race.** The publication stamp is driven from the reaper's own
      `ManualClock`, never frozen at fence time. The winning flip's tombstone survives every
      reap inside `W_tombstone`, an identical retry answers `AlreadyCompleted`, and the record
      is gone once the window has passed and nothing is owed.
    - **A crash at each step of the tombstone's terminal delete,** with no I/O after the crash
      point. The state at that point is asserted, and the run converges with the count at its
      true value.
  - **Negations** recorded in `build-notes.md`, each turning the named evidence red:
    - revert the stamp change ⇒ both appended regressions **and** the DST race row (round 3:
      all nine `reaper_*` rows stayed green);
    - swap the deployed input back to `Retain` ⇒ L;
    - (4) walk `sidx:` for `Aborting` only, not `Completed` ⇒ G's residue leg;
    - (5) fire the terminal delete before `sidx:` is observed empty ⇒ G;
    - skip the clock guard for `Completed` ⇒ E;
    - judge on a fixed clock name instead of the context's ⇒ the clock-identity unit test.
- **Production reach:**
  - **Expiry** runs in the deployed loop now (`crates/server/src/custodian.rs:628`) and in the
    one-shot, through the retirement leg's terminal delete.
  - **The stamp** is in core's Complete, which has no production caller yet
    (`crates/core/src/multipart/complete.rs:299`; the server seam body is a stub,
    `crates/server/src/lib.rs:842-854`). It is honoured by the core regressions and DST now.
    Production reaches it when #670 wires `CompleteMultipartUpload`.
- **Scope:**
  - the `W_tombstone` judgment, per session and on the reaper context's clock, replacing
    `Retain` as the deployed tombstone input;
  - the knob `W_tombstone`: **2 minutes**, decided at the round-1 sign-off, kept below `W_open`.
    Its doc says when the window starts and gives the throughput ceiling, corrected for the
    loop interval and the drain before the delete;
  - the publication stamp taken as decided above;
  - the store binding;
  - a foreign-clocked or ahead-stamped `Completed` session held and named;
  - the matching text in `05`, `06` and `08` of `docs/design/architecture/`.

  **Out of scope:**
  - the `Completing` arms, `G_orphan` and the calibration (child-1);
  - the operator expiry verb and the foreign-clock alarm (#1019);
  - #1016's deployed shapes, beyond the two narrow exceptions;
  - #670's S3 verb;
  - the `orphan:` mark guard (#1061/#1062) and X56, two drainers (#660);
  - ADRs, specs, and any edit to 0016.
- **Repro instruction:** on the base (child-1 and #1015 folded on `main`):
  1. Seed a local-clock `Completed` session whose `completed_at_millis` is far past
     `W_tombstone`, with no obligation, an empty `sidx:` range and `mpuctl` counting it.
  2. Run `wyrd custodian --data-dir <dir> --reap-abandoned-uploads`, and reopen.

  The record is still there and the count is unchanged.
- **Citations expected:** `path:line` for every change. Mirror these peers:
  - the terminal delete it feeds (`crates/custodian/src/retirement/sessions.rs:912-935`);
  - the reaper's clock guard and the `Open` foreign hold (`crates/custodian/src/reaper.rs:826-843`);
  - the tombstone answer table (`crates/core/src/multipart.rs:5623-5640`);
  - the stamp's current doc (`crates/core/src/multipart/complete.rs:278-297`);
  - the deployed loop's tombstone input (`crates/server/src/custodian.rs:590`, `:628`);
  - the test mechanics (`crates/server/tests/multipart_reaper.rs:197-255`,
    `crates/server/tests/custodian_gc.rs:458-470`).
- **Prior-art check (triage cycles):** by path. `retirement/sessions.rs` gained the terminal
  delete with the `Retain` input in f54db83 (#1013, PR #1082 open). `complete.rs`'s stamp
  arrived with 10f03fd and a0a6479 (flat and segmented publication) and 790214d (unknown-outcome
  recovery). No merged, open or closed PR measures `W_tombstone`. The only earlier attempts are
  #1017's own unmerged rounds (`results/issue_1017/iteration-v1..v3`). Their round-3 findings
  are the requirements above. Re-run by path on your base.
- **Disposition hint:** likely-fix
<!-- pdca:end child-2 -->
