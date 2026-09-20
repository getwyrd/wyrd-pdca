<!-- pdca:split-proposal v1 -->
# Split proposal — issue 663

**Intake-cap override (wyrd-pdca-P1).** Granted by the human (Eduard Ralph) in #663's re-plan
session, 2026-09-19, for this split only. `scripts/plan-cap --need 2` at the override:
`planned 23/6 (cap) — room for 0, need 2: Plan intake closed`. That 23 does not count #663 (its
brief was archived by `iterate-plan`, so it read UNPLANNED). Accepted 2026-09-19 as #813 and
#814. Readout right after acceptance: `planned 26/6` — the two children are PLANNED and #663 reads
BUILT on its split marker until the flow reaps it as split, after which the split's lasting effect
is +2 (25/6).

## Why this slice is oversized

v1–v3 each built all of #663 as one patch (v3: 248 KB, 20 files, three rounds). v3's protocol held
under adversarial review; what failed was size, and the findings it drew were the kind a too-big
diff produces. The slice has two outcomes that ship on their own:

- **child-1: find the damage and keep the record of it.** Scrub checks the fragments of committed
  parts and queues repair; reconstruction stops deleting a staged chunk's repair obligation. This
  removes the silent drop, which is the part of the defect that loses information. It also adds the
  two `ReconstructionContext` fields child-2 reads (its time source and write window).
- **child-2: rebuild.** The re-place protocol from 0016: pre-mark, deadline-bound write, and an
  adoption CAS fenced on the session, the part record, the pre-mark and the destination's drain key.

**Why the context fields go in child-1 and not child-2.** C4-verify proves red→green only with an
added `*/tests/*.rs` file (`engine/scripts/run-verify.sh:141-144`). On the red leg it rebuilds
that file against the base with the production change reverted. v3's new test set the two fields
v3 added, so its red leg did not compile and C4-verify read UNVERIFIABLE
(`iteration-v3/SUMMARY.md` §3, §5 C2/C4). With the fields on child-2's base, child-2's new test
compiles there and its whole re-place gets an automated red. The cost lands in child-1: its keep
legs build a `ReconstructionContext`, so they are appended to an existing test file (green-only
at C4-verify, demonstrated red by hand), and the two fields have no reader until child-2. Both
costs are declared in child-1's brief.

**Size, honestly.** child-1 is about a quarter of v3's bytes but touches about 19 files, because
the struct is built in 9 of them. child-2 is still roughly 190 KB, most of it tests. It is one
protocol whose safety rules cannot ship apart from each other, so it is not split further. The
convergence report reads "converged" (each child `watch`, score 6, against the parent's
`oversized`, 9), but only because the proposal cannot yet carry `Conflicts with: 777`. Once that
is added after acceptance each child scores 9, like the parent: the structural score does not see
the byte split, and the patch-size backstop at Check is the signal to watch for child-2.

## Wave sketch

child-2 `Depends on` child-1: it reads the two context fields child-1 adds and builds on child-1's
keep path (its red leg is "kept but not rebuilt"). With `wave_mode = "merge"` and
`auto_merge = true` (`pdca.toml:107`, `:125`) the driver merges child-1's PR before child-2
builds. Both edit `crates/custodian/src/reconstruction.rs`, `crates/dst/tests/custodian.rs` and
`docs/design/architecture/06-runtime-view.md:80`, which the dependency already orders.

Outside the pair (a proposal's ordering fields may name only siblings, `split.py:308-318`, so
these are edited in after `--accept`, as cap-exempt field edits):

- both children get `Conflicts with: 777` (#777 edits `reconstruction.rs` and the DST file);
- #508 and #625: `Depends on` 663 → both child ids;
- #800, #808, #809, #810: `Conflicts with` 663 → both child ids (a conflict naming a split parent
  is dropped by `waves.conflict_map`, so without this they could share a wave with a child).

<!-- pdca:child child-1 -->
# custodian: scrub checks committed staged fragments and reconstruction keeps their repair queued (663.1)

> Child 1 of 2 of #663's split (637.3). Do reads ONLY this file; keep the `- **Label:** value`
> lines. Citations are on `origin/main` @ `97fc2f9` (verified 2026-09-19). 0016 =
> `docs/design/proposals/draft/0016-multipart-commit-protocol.md`.

- **Slug:** staged-scrub-and-keep
- **Kind:** enhancement
- **Defect:** two custodian loops ignore a multipart upload's staged bytes. **Scrub** walks only
  the committed reference set (`crates/custodian/src/scrub.rs:88`, grouped `:130-135`, fetched
  `:137-203`), so a fragment named by a committed `part:` record is never fetched or checked. Rot
  or loss during a staging window that can last hours never becomes a repair obligation, though
  0016 says scrub must check it with the scheme the part record carries (`0016:824`).
  **Reconstruction** resolves an obligation only against committed inodes (`read_committed`,
  `crates/custodian/src/reconstruction.rs:468`). A staged chunk has no committed site, so `assess`
  returns `Drain` (`:613`), the chunk joins `drain_only` (`:218`) and its obligation is deleted
  (`:333-339`). The only record that the chunk is short a fragment is thrown away, and the pass
  can answer `Satisfied`.
- **Success criterion:** the NEW file `crates/custodian/tests/staged_scrub.rs` passes (legs A–C),
  legs D–F appended to the existing `crates/custodian/tests/staged_protection.rs` pass, and
  `cargo xtask ci` is green. All run over in-memory doubles. Records are seeded as raw JSON the
  base decoders accept (shapes as `crates/core/tests/multipart_session_records.rs:81-141`), each
  round-tripped through `decode_session_record` / `decode_part_record` / `decode_owned_entry`
  first. Legs:
  **(A) Scrub checks committed-part fragments.** An `Open` session has one committed `part:`
  record. Its chunk's fragment on one D server carries one flipped bit (`corrupt_fragment`,
  `crates/custodian/tests/scrub.rs:157-162`). One `reconcile_step` with a `ScrubContext` answers
  `Changed` and leaves that chunk in `wyrd_core::repair::queued_repairs`
  (`crates/core/src/repair.rs:151`). The same holds for a missing fragment, and for an intact
  fragment whose header names a different EC scheme from the part record's `ChunkRef` (this proves
  the part's scheme is the one checked). Control: with every fragment intact, nothing is queued
  and the pass answers `Satisfied`.
  **(B) Scrub leaves in-flight chunks alone.** A chunk named only by an owned `sidx:` entry (no
  `part:` record yet) with a fragment missing queues nothing: checking needs the committed scheme,
  which an in-flight chunk does not have yet (`0016:776-781`).
  **(C) Scrub fails closed on what it cannot read.** With one `part:` record whose value will not
  decode, the pass still checks every other fragment (A's corrupt chunk is still queued), names
  the record on the audit seam, and answers `Blocked` — scrub's rule for an unreadable committed
  map (`scrub.rs:99-116`, `:205-215`). A store fault while reading a session's `part:` range fails
  the pass with `Err`, as it fails GC (`docs/design/architecture/06-runtime-view.md:80`).
  **(D) Reconstruction keeps a staged chunk's obligation.** A committed part's fragment is lost
  and its chunk is enqueued (`enqueue_repair`). One `reconcile_step` with a
  `ReconstructionContext`: the obligation is still queued; the pass answers `Blocked`, as it does
  for a `seg:` repair it refuses (`reconstruction.rs:249-256`, `:341-358`); no D server received a
  write; the `part:` record is byte-identical. The same for an `sidx:`-only chunk. Control: an
  obligation for a chunk that no committed map and no staged record names still drains, and the
  pass answers `Satisfied`.
  **(E) Source before destination.** The pass reads the staged classes before the committed
  namespace, `sidx:` → `part:` → `inode:` (normative, `0016:782-800`; GC's order, `gc.rs:286`,
  `:301`). A store hook (`Meta::hook`, `staged_protection.rs:201`, as leg C uses it at
  `:1150-1468`) publishes the chunk — writes a committed inode naming it and deletes its `part:`
  record — right after the pass's first `inode:` read returns. The obligation must not drain. A
  pass that reads `inode:` first misses the chunk in both classes and drains it.
  **(F) An unreadable staged record holds back every drain.** With one `part:` record that will
  not decode, an obligation for a chunk that no class names is NOT drained, and the pass answers
  `Blocked`. This is the existing rule for an unreadable committed object
  (`reconstruction.rs:322-339`), applied to the staged read.
- **Falsifiability:** RED in-process on `origin/main` @ `97fc2f9`, no container. Scrub never reads
  `part:`, so A and C fail by assertion; B passes there by design (it guards against scrub
  over-reaching, it is not a red leg). Reconstruction deletes the obligation, so D, E and F fail
  by assertion when their red is run as Verification posture says.
- **Verification posture:** A–C are in the NEW file, so C4-verify proves their red→green. D–F
  are appended to `staged_protection.rs`, a modified file, which C4-verify reverts along with the
  production change, so there they are green-only (C4-ci runs them). They cannot go in the new
  file: they build a `ReconstructionContext`, whose two new fields (Scope item 3) do not exist on
  the red leg's base, and one added test that does not compile makes the whole C4-verify run
  UNVERIFIABLE (`engine/scripts/run-verify.sh:521-547`). So the new file must not build a
  `ReconstructionContext`. Do shows the D–F red by hand: base production code plus the appended
  legs with only the two field initialisers removed, each of D, E and F failing by assertion.
  Record the command and the pass/fail counts in `build-notes.md`.
- **Invariant to restore:** a fragment a staged record places is either present and intact or a
  durable repair obligation, and an obligation is removed only when no record, committed or
  staged, names its chunk. "I could not read a record" never counts as "no record names it".
  Sources: C-1, "a permanent or data-losing failure mode is never an acceptable cost"
  (`docs/principles.md` §5, and its §6 storage-lifecycle row; a dropped obligation leaves a chunk
  short for good); `0016:824-825`, the read order `:782-800`; scrub's own invariant
  (`scrub.rs:21-25`); proposal 0005's repair contract
  (`docs/design/proposals/accepted/0005-milestone-3-custodians.md:269-286`); ADR-0045. SELF-TEST: fixing scrub alone queues obligations that reconstruction then deletes
  (D goes red); fixing reconstruction alone leaves rot undetected (A goes red).
- **Repo + branch target:** getwyrd/wyrd @ main
- **Ordering note:** first of the pair; child-2 depends on this one. After acceptance, add `777`
  to `Conflicts with` (#777 edits `reconstruction.rs`). The briefs of #808, #809 and #810 are
  re-pointed to conflict with this child (#808 also rewrites leg F of `staged_protection.rs` and
  the same sentence of `06-runtime-view.md:80`).
- **Surfaces:** data
- **Difficulty:** high — it touches scrub, reconstruction's drain decision and the staged reader
  GC and restore share, and changes a struct that 9 files in 4 crates build.
- **Scope:** (1) scrub fetches and checks every fragment a committed `part:` record places, with
  the scheme that record carries, inside the loop it already runs (`scrub.rs:137-203`), and reads
  no `sidx:` entry. (2) Reconstruction reads the staged classes before the committed namespace and
  never drains an obligation for a chunk a staged record names: it keeps it, keeps it off the
  repairable-backlog gauge (`reconstruction.rs:175-199`) and names it on the audit seam, like the
  `seg:` refusal (`:1107`). An empty queue still reads nothing (`:161-169`). (3) The seam child-2
  reads: `ReconstructionContext` (`reconstruction.rs:72-95`) gains
  `clock: &'a (dyn wyrd_testkit::Clock + Sync)` (ADR-0024, `docs/design/adr/0024-clock-and-time-source-trust.md`;
  `crates/testkit/src/lib.rs:23`) and `staged_write_window_millis: u64`. Use these names; child-2's
  brief names them. The deployed loop passes the clock it already advances
  (`crates/server/src/custodian.rs:465-479`, context at `:502-509`) and a window value owned by
  `crates/server/src/cli.rs` as a `pub(crate) const` beside `LEASE_TTL_MILLIS` (`cli.rs:78`),
  passed down the way `GC_GRACE_WINDOW_MILLIS` feeds `GcContext::grace_window_millis`
  (`server/custodian.rs:114`, `gc.rs:200`). If a `W_write` constant already exists on the base
  (#800 names one), use it — never two definitions. Its doc comment states
  `G_orphan > W_repoint + W_write + δ_clock` (`0016:1348`) and that #800's late-write deadline
  must not be sized below it. `wyrd-testkit` moves from dev- to normal dependency where production
  code names `Clock` (as `crates/chunkstore-fs/Cargo.toml:17-19` has it). Update every existing
  construction site. Nothing in this slice reads either field. (4) Keep the prose true: leg F of
  `staged_protection.rs` (`:2035-2209`) now says scrub reads committed parts but no owned entry;
  the last sentence of `06-runtime-view.md:80` and the doc comments that say scrub and
  reconstruction read no staged record (`gc.rs:265`, `:873-876`; `staged_protection.rs:34`,
  `:2160`); narrow the `deferred: #663` marker at `gc.rs:893` to the rebuild, which child-2
  removes. Reusing GC's staged reader (`staged_fragments`,
  `gc.rs:1033`; `walk_staged_range`, `:1069`) is expected; what GC and restore conclude must not
  change, so `staged_protection.rs` legs A–E stay green unedited.
  / out of scope: rebuilding or re-placing anything (child-2); servers absent from the live fleet
  (scrub has only ever visited `ctx.fleet`, `scrub.rs:138`, and the deployed loop drops
  unreachable peers and reads around them, `server/custodian.rs:495-497` — the same for committed
  chunks today); drain status (#808); `crates/core/src/multipart.rs`; edits to 0016 or an ADR.
- **Repro instruction:** on `origin/main`, seed an `Open` session with one committed `part:`
  record, flip one bit in one of its fragments and run a scrub pass: nothing is queued. Enqueue the
  chunk by hand and run a reconstruction pass: the obligation is gone.
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/custodian/tests/staged_scrub.rs` — **NEW** (C4-verify's red comes only
  from an added `*/tests/*.rs`, `run-verify.sh:141-144`; `--classify` on a synthetic patch for this
  slice returns `ADDED_TEST crates/custodian/tests/staged_scrub.rs`). Legs D–F go in the existing
  `crates/custodian/tests/staged_protection.rs`, which already has the staged fixtures and the
  store hook.
- **Production reach:** A–F run through the production `reconcile_step`. The two new context
  fields are a seam ahead of their reader: (a) the deployed loop fills them, nothing reads them;
  (b) child-2, the next wave of the same run, reads them; (c) they sit here so child-2's new test
  compiles on its base and C4-verify can prove child-2's red.
- **Citations expected:** `path:line` on the target branch for every change. Peers Do MAY open:
  `scrub.rs:137-203` (the loop to extend); `gc.rs:286-301` and `:1033-1100` (read order and
  staged reader); `reconstruction.rs:249-256`, `:322-358`, `:1107` (the `seg:` refusal and the
  incomplete-reading rule to mirror); `staged_protection.rs:130-280` (the `Meta` double and hook).
  Prior art Do MAY read: v3's `scrub.rs` and `gc.rs` hunks in
  `results/issue_663/iteration-v3/patch.diff` (they passed review; v3 failed on size).
- **Prior-art check (triage cycles):** by path across merged history and open PRs: `scrub.rs` and
  `reconstruction.rs` have never read `part:` or `sidx:`; no open PR touches them (2026-09-19).
  Rejected prior art: #663 v1–v3 and #637 v1, all whole-slice builds.
- **Disposition hint:** likely-fix
<!-- pdca:end child-1 -->

<!-- pdca:child child-2 -->
# custodian: reconstruction rebuilds a staged chunk under the session fence (663.2)

> Child 2 of 2 of #663's split (637.3). Do reads ONLY this file; keep the `- **Label:** value`
> lines. Citations are on `origin/main` @ `97fc2f9` (verified 2026-09-19); this bundle's base
> adds child-1 (find its changes by symbol). 0016 =
> `docs/design/proposals/draft/0016-multipart-commit-protocol.md`.

- **Slug:** staged-replace
- **Kind:** enhancement
- **Defect:** after child-1, reconstruction recognises a committed part's chunk but can only keep
  its obligation. Nothing rebuilds the fragment, so the part stays a fragment short until it is
  published, or forever if the client never completes. 0016 requires the rebuild (`0016:825`),
  and its failure table names "scrub staged fragments but leave reconstruction committed-only" as
  a wrong implementation (`:889`).
- **Success criterion:** the NEW file `crates/custodian/tests/staged_repair.rs` passes, one
  seeded case appended to the existing `crates/dst/tests/custodian.rs` passes, and
  `cargo xtask ci` is green. The new test names only symbols on its base, including
  `ReconstructionContext::{clock, staged_write_window_millis}` from child-1, with time from a
  `wyrd_testkit::ManualClock` (`crates/testkit/src/lib.rs:49`). The D-server doubles — this
  file's and the DST's — **enforce** the deadline `put_fragment` carries, refusing at or after it
  through `WriteDeadlineExpired::if_elapsed` (`crates/traits/src/lib.rs:925`), as the real D
  server does since #638. v1–v2's doubles ignored it (`_deadline_millis`,
  `crates/custodian/tests/gc.rs:127`). Legs:
  **(A) The whole rebuild.** An `Open` session's committed part has one fragment lost and its
  obligation queued. One `reconcile_step` with a `ReconstructionContext` answers `Changed`, and:
  a new D server holds an intact fragment with the right scheme
  (`wyrd_core::repair::header_matches_identity`, `crates/core/src/repair.rs:58`); the `part:`
  record's `ChunkRef.placement` names that server; the destination's pre-mark `orphan:<P_new>` is
  gone; the vacated `P_old` carries an `orphan:` mark; the obligation has drained. A changed
  placement alone is not enough.
  **(B) Losing branches strand nothing (X29, `0016:888`).** A `put_fragment` hook moves the
  session from `Open@E` to `Aborting@E+1` after the destination write and before the adoption
  CAS. Then: no adoption, the `part:` record byte-identical, the pre-mark still there, the
  obligation queued. Repeat with the `part:` record rewritten instead of the session fenced.
  **(C) The pre-mark and deadline rules (`0016:1285-1358`), one case each:**
  (i) the pre-mark is durable before the destination write: the double checks `orphan:<P_new>`
  is present when `put_fragment` arrives;
  (ii) a destination position that already carries a mark from another event, or a legacy mark,
  is re-stamped fresh, never reused with its old stamp;
  (iii) a position with a `reclaiming` mark is never written, and ruling it out removes only that
  position, not its server: RS(2,2) with two lost fragments, two free domains and a stale
  `reclaiming` mark on one candidate position repairs both in ONE pass (v2's bug: excluding the
  whole server stalled this forever while every pass answered `Satisfied`);
  (iv) the write deadline is the time the context clock reads when the pre-mark commits, plus
  `staged_write_window_millis` — never the pass-start time. With the clock moved on between pass
  start and pre-mark, the deadline is still live (v1's stall). A write the double refuses as
  expired aborts the re-place: no adoption, pre-mark still there, obligation queued;
  (v) no destination write is authorized on a pre-mark older than `W_repoint`
  (`0016:1339-1349`). A hook moves the clock past it between pre-mark and write, and no write on
  the stale pre-mark is ever adopted. Enforcing this through the deadline the D server checks is
  accepted (v1 sign-off);
  (vi) a vacated `P_old` whose existing `orphan:` value decodes as none of the three shapes aborts
  the move before its CAS, keeps the obligation queued and names the fault; it is never
  overwritten (ADR-0045).
  **(D) The drain fence on the destination (`0016:885`).** A server with ANY
  `desired:dserver:<S>` record (`crates/custodian/src/desired_state.rs:36`), whatever its value —
  `maintenance` included — is never chosen, so selection and the CAS test the same fact (v1's
  repro: `maintenance` stalled four passes, all answering `Satisfied`). A drain recorded between
  selection and the adoption CAS makes the CAS lose (`require_absent(desired:dserver:<S_new>)`):
  not adopted, pre-mark still there.
  **(E) Kept, not rebuilt.** An `sidx:`-only chunk, and a chunk of a session that is not `Open`,
  keep their obligation and nothing is written (a repair blocked by a Complete is retried after
  publication, `0016:825`).
  **(F) Seeded DST for X29**, appended to the existing `crates/dst/tests/custodian.rs` (keep
  `#![cfg(madsim)]`, `:53`). Sweep the session fence across every point of the re-place: before
  the pre-mark, between pre-mark and write, between write and CAS, after the CAS. In every
  interleaving no fragment ends unreferenced and unevidenced, and the session never ends
  `Aborting` with a `part:` record naming a fragment that was not written. It runs under
  `cargo xtask ci` → `run_dst` (`xtask/src/main.rs:1578`, `--cfg madsim`). Record the seed count
  in `build-notes.md`.
- **Falsifiability:** RED in-process on this bundle's base, `origin/main` + child-1. With
  `wave_mode = "merge"` child-1 is merged first; a fold branch would arrive as
  `$PDCA_VERIFY_BASE`, which `engine/scripts/run-verify.sh:247-265` honours. On that base
  reconstruction keeps the obligation and writes nothing, so A, B, C(iv) and D fail by assertion.
  C(i)–(iii), (v) and (vi) fail there on the missing rebuild rather than on the rule; each rule's
  own branch must be reached in the green leg (C4-diff-cov reports it). The new test compiles on
  that base because child-1 added the two context fields. It must add no other symbol the test
  names. The DST case is in a modified file, so C4-ci is its gate, not C4-verify.
- **Invariant to restore:** a staged chunk degraded while its session is `Open` is rebuilt, and
  no outcome strands a fragment: every fragment the re-place writes is, at every instant, either
  named by a record or covered by an `orphan:` mark GC can act on, and the obligation is removed
  only in the commit that makes the repair durable. Sources: C-1 (`docs/principles.md` §5, and its
  §6 storage-lifecycle row: every durable byte is at every instant named by a record or evidenced
  for reclamation); `0016:825`, `:885`, `:888-889`; "no fragment is written after its evidence may
  have been reclaimed" (`:1356-1358`); ADR-0045.
  SELF-TEST: a rebuild that writes then CASes without a pre-mark passes A and fails B.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Ordering note:** builds on child-1 (its context fields and its keep path). After acceptance,
  add `777` to `Conflicts with` (#777 edits `reconstruction.rs` and the DST file). #800 is
  re-pointed to conflict with this child (both append to the DST file and edit
  `06-runtime-view.md`). The `W_write` constant #800 also names is child-1's to define or reuse.
- **Surfaces:** data
- **Difficulty:** high — a multi-step metadata protocol with four fences, plus a DST property.
- **Do model:** opus-max
- **Scope:** rebuild and re-place a committed part's chunk in an `Open` session: pre-mark before
  write; deadline from the context clock at pre-mark commit plus the window; the `W_repoint` gate;
  one adoption CAS pinned to the session state, the prior `part:` bytes, the pre-mark's bytes and
  `require_absent(desired:dserver:<S_new>)`. On any loss the obligation stays queued and the
  pre-mark stands. Time and window come only from child-1's fields; add no other. Extend the
  last sentence of `docs/design/architecture/06-runtime-view.md:80` with the rebuild, and drop
  `#663` from the `deferred:` marker at `gc.rs:893`.
  / out of scope: the committed repair path (`reconstruction.rs:829-955`, unchanged); one
  degraded chunk per part per pass — every plan in a part pins the same `part:` bytes, as the
  committed path pins one inode (`reconstruction.rs:155-159`, `:294-303`); accepted here, say so
  in a comment; a late write whose effect is `WriteEffect::Unknown` landing after GC reclaims its
  pre-mark (a gap in the deadline model shared by every writer, `crates/traits/src/lib.rs:862-902`);
  `EcScheme::None` answering Unrepairable (by design, as `reconstruction.rs:641`); servers absent
  from the live fleet; `seg:` repair (#777); drain status, rebalance, restore (#808–#810); the
  upload-side drain fence (#657); `crates/core/src/multipart.rs`; edits to 0016 or an ADR.
- **Repro instruction:** on the base (`origin/main` + child-1), seed an `Open` session with one
  committed `part:` record, delete one of its fragments, `enqueue_repair` its chunk and run a
  reconstruction pass: the obligation is still queued and no fragment was written.
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/custodian/tests/staged_repair.rs` — **NEW** (`--classify` on a synthetic
  patch for this slice returns `ADDED_TEST crates/custodian/tests/staged_repair.rs`; the DST file
  is modified, not added). Never add a new DST file: an added `#![cfg(madsim)]` file joins
  C4-verify's single cargo run and switches it to `--cfg madsim` (`run-verify.sh:155-200`).
- **Production reach:** the passes under test are the production `reconcile_step`. The test
  applies the session fence itself because Abort and Complete (#656, #658) do not exist yet. That
  is intended: the race is the re-place against any fence, and a fence is a CAS on the `mpu:`
  record whoever writes it.
- **Citations expected:** `path:line` on the target branch for every change. Peers Do MAY open:
  `reconstruction.rs:600-740` (`assess`) and `:829-955` (`repair_chunk`: rebuild, destination
  choice, CAS shape; it passes no deadline at `:934`, this path must);
  `crates/traits/src/lib.rs:837-970` (`WriteDeadlineExpired`, `if_elapsed`,
  `if_publication_unverified`); `crates/custodian/tests/reconstruction.rs` (repair-harness
  idioms). Prior art Do MAY read: v3's `crates/custodian/src/reconstruction/staged.rs` in
  `results/issue_663/iteration-v3/patch.diff`. Its protocol held under adversarial review
  (`iteration-v3/SUMMARY.md` §5), and it already carries the v1–v2 fixes in C(iii), C(iv) and D.
- **Prior-art check (triage cycles):** by path across merged history and open PRs: no staged
  re-place exists on `main`; no open PR touches `reconstruction.rs` (2026-09-19). Rejected prior
  art: #663 v1–v3 (size, not protocol) and #637 v1, whose review found the undecodable-source
  commit, the reused destination stamp and the never-exercised deadline (C(vi), C(ii), the
  enforcing doubles).
- **Disposition hint:** likely-fix
- **Depends on:** child-1
<!-- pdca:end child-2 -->
