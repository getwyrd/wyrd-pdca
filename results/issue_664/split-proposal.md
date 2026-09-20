<!-- pdca:split-proposal v1 -->
# Split proposal — issue 664

> **Intake-cap override (wyrd-pdca-P1).** Granted by the human in the Plan session on
> 2026-09-18 **for #664's split only** — it covers these three children and no other id.
> Also recorded in #664's brief (`Ordering note`).
>
> **Re-measured by the planner after the splitter drafted this** (the splitter itself could not
> run the script — the command needed an approval its headless session could not get):
>
> ```
> planned 23/6 (cap) — room for 0, need 3: Plan intake closed
>   PLANNED           18  508 625 633 663 664 722 738 741 742 773 774 775 776 777 778 779 800 804
>   BUILT              3  711 721 736
>   AWAITING_SIGNOFF   2  682 selftest
> ```
>
> 23, not the 22 the override was given against: #664's own re-planned brief moved it from
> UNPLANNED to PLANNED between the two readings. It drops back out the moment this proposal is
> accepted and #664 takes `close-disposition = split`, so accepting materialises three bundles
> and lands the count at **25**. The override stands on that arithmetic.
>
> **Three children, not two.** Merging child-2 and child-3 would rebuild the part of iteration 1
> that was both the largest and the one the review found unsound.

## Why this slice is oversized

#664 holds three outcomes that ship separately, and iteration 1 proved it: 211 KB across 14
files against a 100 KB backstop, with every blocking finding in one third of the diff. This is
the brief's own new scope, not size inherited from #637 — the sign-off asked for this split by
name, and the re-planned brief was written as its input.

The three seams, most independent first:

1. **Drain status and rebalance read the staged class (defects 1–2).** An operator is told a
   server may be wiped while a live upload's bytes sit on it. The fix lives in
   `crates/custodian/src/desired_state.rs` and reads a class #803 already built
   (`StagedSet`, `gc.rs:672-781`). It touches no restore code, no session record and no codec.
   It has its own red (`reconciliation_status` answers `Satisfied`), its own test file, and an
   operator can use it the day it merges.
2. **Restore fences resurrected sessions and reports what it skipped (defect 3).** This is the
   bulk of iteration 1: the nonce on the `Completing` session record
   (`crates/core/src/multipart.rs`), both fence shapes with their obligations in one batch each
   (`restore.rs`), the two new report counters, and the operator text in
   `crates/server/src/cli.rs`. It is useful alone — a fenced session cannot publish over lost
   bytes whether or not anything records that the fence ran.
3. **The durable fence-generation record, and residue that survives a re-fence (defect 4).**
   This is where iteration 1 broke (`restore.rs:916`: a second pass skipped an `Aborting`
   session and certified the generation over unrepaired residue). It is a claim *about* the
   fence, so it needs the fence to exist first, and it needs the repair-then-second-pass test
   the sign-off required before the marker can be trusted. Kept apart, the reviewer reads one
   question — "can this record ever say more than is true?" — on a small diff.

What stays whole: the nonce, the fence and the report counters are one child. The fence cannot
name `seg:<nonce>:<E>:` without the nonce, and the counters only count what the fence does, so
cutting there would give children that cannot show a red of their own.

## Wave sketch

Three waves, one child each. No two children can share a wave.

```
wave 1: child-1  — drain status + rebalance read the staged class
wave 2: child-2  — restore: staged accounting, session fence (both shapes), nonce
wave 3: child-3  — fence-generation record + residue across generations
```

- **child-3 depends on child-2.** The generation record certifies the fence child-2 builds, and
  its residue legs re-read sessions child-2's fence moved to `Aborting`. This is a real
  build-on dependency: child-3's base is `origin/main` plus child-2's accepted patch.
- **child-1 conflicts with child-2 and with child-3, but needs neither.** Its logic is disjoint
  from theirs. The files are not. `docs/design/architecture/06-runtime-view.md` keeps this whole
  topic in one paragraph at `:78`: child-1 rewrites that paragraph's last sentence ("Scrub and
  the drain-status query read committed references only"), child-2 adds its fence paragraphs
  directly under it, and child-3 adds the generation paragraph under those. Adjacent hunks do
  not fold cleanly. child-1 and child-2 also both edit `crates/custodian/src/gc.rs` — child-1
  the `StagedSet` doc comment and the `deferred: #663, #664` marker (`:669`), child-2 the
  visibility of the paging helpers below it.
- **Why child-1 goes first.** It is the smallest and the only `medium`, so it clears the shared
  doc paragraph early and the two restore children then stack on a settled base. Nothing breaks
  if the scheduler puts child-2 first instead; the fields allow either order.
- **The cost, stated plainly:** no parallel wave. That is the price of the shared doc paragraph
  and is cheaper than a fold conflict at integration.
- **Outside this proposal** (the ordering fields can name only siblings, so add these to the
  materialised briefs after acceptance): all three children edit `06-runtime-view.md`, so each
  inherits the parent's `Conflicts with: 663, 804`. #804 already declares `Conflicts with: 664`
  and should be re-pointed at the three new ids. Downstream: #658 must write child-2's nonce;
  #656 should reuse child-2's fence batch; #508 reads child-3's generation record, so #508's
  `Depends on` edge moves from 664 to child-3.

<!-- pdca:child child-1 -->
# Brief — staged-drain-status

> Child 1 of 3 of #664's split (itself 637.4). Do reads ONLY this file. Keep the
> `- **Label:** value` lines. `path:line` citations are on `origin/main` @ `f41e9c5`
> (re-verified 2026-09-18). Base is plain `origin/main`. Background: the drain and rebalance
> rows of 0016's decision-2 table
> (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:820-871`, `:826-827`) and the
> failure table (`:874-890`, `:881`, `:883`).

- **Slug:** staged-drain-status
- **Kind:** enhancement
- **Defect:** drain status tells an operator a server may be wiped while a live upload's bytes
  are on it. `reconciliation_status` answers `Satisfied` for a server holding only staged
  bytes, because its `genuinely_holds` test reads committed placements alone
  (`crates/custodian/src/desired_state.rs:181-196`) — the F6 trace. The sharper form is an
  in-flight part with no `part:` record yet (`0016:827`). The class that answers this already
  exists and is unread here: `StagedSet::protects` (`crates/custodian/src/gc.rs:705`). Second,
  0016 requires a rebalance pass over a draining server holding only staged fragments to plan
  no move and rewrite no `part:` record while drain status answers `Pending` for that same
  server (`0016:881`; `plan_evacuations`, `crates/custodian/src/rebalance.rs:257`). Nothing
  asserts that today. #803 left this to #664 by name: `deferred: #663, #664` at `gc.rs:669` and
  `crates/custodian/tests/staged_protection.rs:2160`.
- **Success criterion:** the NEW file `crates/custodian/tests/staged_drain_status.rs` passes
  over in-memory doubles, with records seeded as raw JSON the base decoders accept (the shapes
  in `crates/core/tests/multipart_session_records.rs:81-145`). Legs:
  **(A) Drain counts an in-flight part as held.** Server `S` holds **only** an owned `sidx:`
  fragment, and `desired:dserver:<S>` is set: `reconciliation_status(S)` is `Pending`. On the
  base it is `Satisfied` — the red.
  **(B) Drain counts a committed part as held**, as its own case: `S` holds only a committed
  `part:` fragment, and the answer is `Pending`. An implementation counting only one class
  passes one of A and B and fails the other (`0016:883`).
  **(C) Drain still finishes when the uploads live elsewhere.** Staged fragments sit on servers
  0–2, and server 3 is draining and holds none of them and no committed reference:
  `reconciliation_status(3)` is `Satisfied`. Iteration 1's `*server != dserver` mutant survived
  every other leg; this case kills it. It is green on the base too — a guard.
  **(D) Rebalance and drain agree, and rebalance leaves staged bytes alone (`0016:881`).** For a
  draining server holding **only** staged fragments, a rebalance pass writes no fragment
  anywhere and rewrites no `part:` record, **and** `reconciliation_status` is `Pending`. The red
  comes from the `Pending` half. State in `build-notes.md` which `Reconciled` the pass returns
  there, and why it does not tell an operator the drain is done.
  **(E) An unreadable or untrusted staged record never yields `Satisfied`.** A staged record the
  query cannot read blocks every drain; one it can read but not trust blocks them the way a
  committed map with an untrustworthy placement does (mirror `StagedSet::protection`,
  `gc.rs:690`).
  **(F) `cargo xtask ci` green.**
- **Falsifiability:** RED is produced in-process on plain `origin/main` @ `f41e9c5`, no
  container, by **assertion**: the base's drain status counts committed placements only, so A,
  B and D's `Pending` half fail there. C is a guard. The new test may name only base-visible
  symbols (`wyrd_custodian::{reconciliation_status, set_lifecycle, reconcile_step,
  RebalanceContext, ReconciliationStatus, DServerLifecycle}`,
  `wyrd_core::multipart::{mpu_key, part_key, sidx_key}`, `wyrd_traits`). A compile failure on
  the RED leg reports UNVERIFIABLE (`engine/scripts/run-verify.sh`). Record in `build-notes.md`
  how many tests ran red, all by assertion.
- **Invariant to restore:** a drain is `Satisfied` only when no byte that can still become
  referenced — committed, committed-part or in-flight — lives on that server. Source: 0016
  decision 2's drain and rebalance rows (`0016:826-827`, `:881`); the C-1 rule that a
  certification over an incomplete picture is a defect (`docs/principles.md` §5). SELF-TEST:
  counting only `part:` in the drain misses the in-flight `sidx:` case.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Repro instruction:** on `origin/main` @ `f41e9c5`, seed an owned `sidx:` entry whose fragment
  sits on server `S`, set `desired:dserver:<S>` with `set_lifecycle`, and call
  `reconciliation_status(S)`: it answers `Satisfied`.
- **Scope:** `reconciliation_status` reads the existing `StagedSet` and counts both staged
  classes as held; rebalance is confirmed disjoint from the staged set (expected: a test and a
  comment, no behaviour change — if `plan_evacuations` needs a real change, say why in
  `build-notes.md`); #664's half of the two `deferred:` markers is discharged, leaving #663's;
  the drain-status sentence in `docs/design/architecture/06-runtime-view.md:78` is corrected.
  / out of scope: anything in `restore.rs`, `crates/core/src/multipart.rs` or
  `crates/server/src/cli.rs` (child-2, child-3); rebuilding the staged class (#803, merged);
  `scrub.rs` and `reconstruction.rs` (#663); the mark codec (#804);
  `crates/dst/tests/custodian.rs`; evacuating committed segmented objects (#653/#722); any edit
  to 0016 or an ADR.
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/custodian/tests/staged_drain_status.rs` — a **NEW** file. The
  C4-verify gate earns its red only from an added `*/tests/*.rs`
  (`engine/scripts/run-verify.sh`), so do not append to `staged_protection.rs`. The existing
  dev-dependencies suffice; no `Cargo.toml` change.
- **Difficulty:** medium
- **Conflicts with:** child-2, child-3
- **Ordering note:** wave 1 of #664's split. Shares no logic with child-2 or child-3, but shares
  the paragraph at `06-runtime-view.md:78` with both and `gc.rs` with child-2, so it never
  shares a wave with them. After acceptance add `Conflicts with: 663, 804` (same doc).
- **Surfaces:** data
- **Do model:** opus-max
- **Production reach:** the passes under test are the production `reconciliation_status` and
  the rebalance loop. Every staged record is seeded by the test, because no client can create a
  session until #508.
- **Citations expected:** Do must cite `path:line` on the target branch for every change. Peer
  callsites Do MAY open and mirror:
  * `crates/custodian/src/gc.rs:672-781` — `StagedSet`, `protection` (`:690`), `protects`
    (`:705`), `staged_fragments` (`:809`). The class to read, not rebuild.
  * `crates/custodian/src/gc.rs:515` — `ReferenceSet::protects`, the committed twin.
  * `crates/custodian/src/desired_state.rs:181-247` — `reconciliation_status` and
    `genuinely_holds` (`:191`).
  * `crates/custodian/src/rebalance.rs:257` — `plan_evacuations`.
- **Prior-art check (triage cycles):** by path (`desired_state.rs`, `rebalance.rs`), re-run
  2026-09-18 on `f41e9c5`: #803 (PR #807, merged) explicitly excluded drain status and
  rebalance; no open PR touches them. #664 iteration 1
  (`results/issue_664/iteration-v1/patch.diff`) carried this work inside a 211 KB patch; its
  drain hunks drew no blocking finding, but the `*server != dserver` mutant survived — leg C.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR MAY
happen during the cycle. The PR MUST NOT be marked ready before sign-off accepts.
<!-- pdca:end child-1 -->

<!-- pdca:child child-2 -->
# Brief — restore-session-fence

> Child 2 of 3 of #664's split (itself 637.4). Do reads ONLY this file. Keep the
> `- **Label:** value` lines. `path:line` citations are on `origin/main` @ `f41e9c5`
> (re-verified 2026-09-18). This bundle's base is `origin/main` **plus child-1's accepted
> patch** if child-1 ran first: child-1 edits `gc.rs` comments and one doc paragraph only, so
> re-locate by symbol. Background: the restore rows of 0016's decision-2 table
> (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:820-871`, `:823`), the failure
> table (`:874-890`, X57 at `:880`), decision 1.4 / D-B (`:717-728`), and the fence rows of the
> batch table (`:660`, `:664-665`).
>
> **Design call settled at Plan (2026-09-12, the human, option (i)):** a `Completing` session
> stores its **segment-group nonce** on its session record, beside the fence epoch
> `PublishTarget` already carries (`crates/core/src/multipart.rs:1954-1961`).
> `SessionState::Completing` carries `fenced_at_millis`, `segments_written` and
> `publish_target` today and no nonce (`multipart.rs:2008-2038`). Rejected: deriving the nonce
> from `(upload id, E)` as `0016:2333` says, because the code keeps the nonce independent of the
> upload id on purpose (`multipart.rs:3307`); and leaving the records without a deleter.

- **Slug:** restore-session-fence
- **Kind:** enhancement
- **Defect:** restore fences no session, and cannot report what it skipped. A restored image
  can resurrect an `Open` or `Completing` session whose bytes are gone, and nothing stops it
  from completing over them (D-B, `0016:717-728`, F13). A `Completing` session that had already
  written segments needs its `seg:` records retired in the **same** batch as its fence, or they
  have no deleter anywhere in the design (X57, `0016:880`) — and today the session record
  cannot even name them (`multipart.rs:2008-2038`). Separately, restore skips staged fragments
  silently through #803's gate (`crates/custodian/src/restore.rs:438`); 0016 requires the
  report to say so — `staged_skipped` and `sessions_fenced` beside `pending_skipped`
  (`0016:823`; `RestoreReport`, `restore.rs:115-215`). #803 also left one question here, marked
  `// deferred: #664` at `restore.rs:819`: whether a held (untrusted) staged record sets
  `needs_human()`.
- **Success criterion:** the NEW file `crates/custodian/tests/restore_session_fence.rs` passes
  over in-memory doubles, with records seeded as raw JSON. A `Completing` fixture carries the
  new nonce field; base decoding rejects it (`#[serde(deny_unknown_fields)]`), which is harmless
  there because base restore never reads `mpu:`. Legs:
  **(E) Restore reports staged skips.** `reconcile_after_restore` over a store with two staged
  fragments reports them as staged-skipped, separately from `pending_skipped`. Assert through
  the report's `Debug` rendering, which must contain `staged_skipped: 2` (`0016:823`); the base
  rendering has no such counter.
  **(F) Restore fences a resurrected `Open` session (D-B).** An `Open@E` session ends as
  `Aborting@E+1`. In the same batch — assert atomicity with a double that fails that one
  commit, after which **none** of the writes are present — its byte-retirement obligation is
  installed. Round-trip every obligation the fence writes through `decode_retire_obligation`
  (`multipart.rs:3455`) against the key it sits under. The `Debug` rendering contains
  `sessions_fenced: 1`. A Complete retried against that session cannot fence it, since the
  Complete fence requires `Open@E` (`0016:660`); the client-visible `4xx` is #658's.
  **(G) Restore fences a resurrected `Completing` session with its segments' deleter (X57).** A
  `Completing@E` session with `segments_written > 0`, its nonce on the record, and
  `seg:<nonce>:<E>:*` records present ends as `Aborting@E+1`. One batch installs `retire:bytes`
  naming the session and its parts **and** `retire:records` naming exactly `seg:<nonce>:<E>`
  (`0016:665`). Both decode through `decode_retire_obligation`, and the records obligation's
  `segments()` names that group. That draining empties the range is #659's to prove.
  **(H) What cannot be fenced cleanly is never passed off as done.** (i) A `Completing` record
  with **no** nonce — the pre-decision shape — fails decode; restore leaves it byte-identical
  (ADR-0045) and names it as needing a human. (ii) A `Completing` session whose `seg:` records
  name a chunk none of its `part:` records holds is still fenced, and still named as needing a
  human. In both, `RestoreReport::needs_human()` is true (`restore.rs:212`).
  **(H-iii) The `restore.rs:819` question is answered**, either way, with a test and the reason
  in `build-notes.md`.
  **(J) The session record carries the nonce, and nothing else changes for it.** A `Completing`
  record with the nonce round-trips byte-identically through its codec, and one without is
  refused. Put this in the codec's own test module in `multipart.rs`; green-only, which is fine
  for a codec leg.
  **(K) A second pass is idempotent.** Re-running `reconcile_after_restore` over the fenced
  store installs no second obligation and fences nothing again, and a case H session is
  **still** named as needing a human on that second pass. This is the half of iteration 1's bug
  that lives in this child: never let "already `Aborting`" mean "nothing to report".
  **(L) `cargo xtask ci` green.**
- **Falsifiability:** RED is produced in-process on the base by **assertion**: base restore
  reads no `mpu:` record and writes no fence, so E, F, G, H and K fail there. J is green-only.
  The new test may name only base-visible symbols (`wyrd_custodian::{reconcile_after_restore,
  RestoreReport}`, `wyrd_core::multipart::{mpu_key, part_key, sidx_key, retire_key,
  decode_retire_obligation, decode_session_record, RetireMode, RetireToken}`,
  `wyrd_core::metadata::seg_key`, `wyrd_traits`) — no field or type this slice adds, hence the
  `Debug` assertions. A compile failure on the RED leg reports UNVERIFIABLE
  (`engine/scripts/run-verify.sh`). Record in `build-notes.md` how many tests ran red.
- **Invariant to restore:** a session a restore resurrected can no longer publish, and every
  record that session wrote has a named deleter, installed in the same commit as the fence.
  What restore skipped or could not fence, it says. Source: 0016 `:823`, D-B and decision 1.4
  (`:717-728`), X57 (`:880`), ADR-0045. SELF-TEST: fencing `Open` sessions alone leaves a
  `Completing` session's segment records with no deleter.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Repro instruction:** on the base, seed an `Open@E` session under `mpu:` with one `part:` record
  and run `reconcile_after_restore`: the session is still `Open@E`, no `retire:` key exists,
  and the report's `Debug` rendering has no `sessions_fenced`.
- **Scope:** the nonce on the `Completing` session record and its codec; the restore fence in
  both shapes, one batch each; `staged_skipped` and `sessions_fenced` on `RestoreReport`; the
  `restore.rs:819` answer; `restore_verdict` and the operator paragraphs in
  `crates/server/src/cli.rs:1230-1370`, with its report-literal tests (`:2905-2990`); the fence
  paragraphs in `06-runtime-view.md` and the nonce in `05-building-block-view.md:202` and the
  m4 blueprint's restore steps. `gc.rs` only to widen the paging helpers' visibility if the
  fence reuses them. / out of scope: **any durable record that the fence ran, and any
  "generation"** (child-3 — do not add `mpufence` or the like here); drain status and rebalance
  (child-1); `scrub.rs`, `reconstruction.rs` (#663); the mark codec (#804); the retire drain
  (#659); Abort and Complete (#656, #658); the gateway (#508); `crates/dst/tests/custodian.rs`
  unless an existing case stops passing, and then say so in `build-notes.md`; any edit to 0016
  or an ADR.
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/custodian/tests/restore_session_fence.rs` — a **NEW** file; the
  C4-verify gate earns its red only from an added `*/tests/*.rs`. No `Cargo.toml` change.
- **Difficulty:** high
- **Conflicts with:** child-1
- **Ordering note:** wave 2 of #664's split. Needs nothing from child-1, but both edit `gc.rs`
  and the paragraph at `06-runtime-view.md:78`. child-3 builds on this child. After acceptance
  add `Conflicts with: 663, 804` (same doc). Downstream, not work here: #658 must write this
  nonce when it fences a session into `Completing`; #656 should reuse this fence batch.
- **Surfaces:** data
- **Do model:** opus-max
- **Production reach:** the pass under test is the production `reconcile_after_restore`. Every
  session is seeded by the test, because no client can create one until #508.
- **Citations expected:** Do must cite `path:line` on the target branch for every change. Peer
  callsites Do MAY open and mirror:
  * `crates/custodian/src/restore.rs:111-215` (`MARK_BATCH`, `RestoreReport`, `pending_skipped`
    `:126`, `needs_human` `:212`), `:312` (`reconcile_after_restore`), `:438` (#803's staged
    gate), `:490`, `:511`, `:813-825` (`attribute_staged` and the `deferred: #664` marker).
  * `crates/core/src/multipart.rs:1954-1961` (`PublishTarget`), `:2008-2038` (`SessionState`),
    `:2250` (`decode_session_record`), `:3455` (`decode_retire_obligation`), `:3141` (the retire
    rows table), `:3307` (why the nonce is independent of the upload id).
  * `crates/core/src/metadata.rs:763` (`SegmentNonce`), `:798` (`SegmentGroup`), `:1258`
    (`seg_key`).
  * `crates/custodian/src/gc.rs:809` (`staged_fragments`) and the paging helpers under it.
  * `crates/server/src/cli.rs:1230-1370`, `:2905-2990`.
- **Prior-art check (triage cycles):** by path (`restore.rs`, `multipart.rs`), re-run
  2026-09-18 on `f41e9c5`: #803 (PR #807) is the only recent change to `restore.rs`; no merged
  change fences sessions; no open PR touches these paths. Rejected prior art: #637 iteration 1
  fenced `Completing` sessions without the `seg:` deleter; #664 iteration 1
  (`results/issue_664/iteration-v1/`) added the deleter but skipped an already-`Aborting`
  session on a second pass (`restore.rs:916` in that patch) — leg K. Do NOT re-attempt either
  unchanged.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR MAY
happen during the cycle. The PR MUST NOT be marked ready before sign-off accepts.
<!-- pdca:end child-2 -->

<!-- pdca:child child-3 -->
# Brief — restore-fence-generation

> Child 3 of 3 of #664's split (itself 637.4). Do reads ONLY this file. Keep the
> `- **Label:** value` lines. `path:line` citations are on `origin/main` @ `f41e9c5`
> (re-verified 2026-09-18). This bundle's base is `origin/main` **plus child-2's accepted
> patch** (the session fence, the nonce, `sessions_fenced` / `staged_skipped`): locate those by
> symbol on the base. Background: 0016 decision 1.4 / D-B
> (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:717-728`, `:723-728`), the
> deployment ordering (`:3017-3021`), X17b.

- **Slug:** restore-fence-generation
- **Kind:** enhancement
- **Defect:** no durable record tells a gateway the restore fence has run, and residue does not
  survive a re-fence. 0016 requires the restore-fence generation to complete before any gateway
  serves multipart verbs on the restored image (`0016:723-728`, `:3017-3021`, X17b). Iteration
  1's version was unsound: a session fenced to `Aborting` with unrepaired residue was skipped
  on every later pass, so the documented repair-and-rerun remedy cleared the residue finding
  and certified the generation complete with the obligations still unmet. A gateway trusting
  that marker would resume multipart verbs over an unfenced image.
- **Success criterion:** the NEW file `crates/custodian/tests/restore_fence_generation.rs`
  passes over in-memory doubles. The generation record is read by **raw key**, so the test
  compiles on the base. Legs:
  **(I) The generation record, three arms, each on durable state.** (i) Before any post-restore
  pass the record is absent. (ii) **During** a pass, read through a double hook at the first
  fence commit, it names the pass's generation and reads not-complete. (iii) After the pass it
  reads complete for that generation. A second pass **advances** the generation and reads
  not-complete until it finishes, so a later restore invalidates an earlier completion instead
  of being masked by it. "Complete" becomes observable only after every write the pass makes,
  the mark batches included.
  **(M) Needing a human blocks completion.** For each of child-2's unfenceable cases — a
  `Completing` record with no nonce, and a `Completing` session whose `seg:` records name a
  chunk no `part:` record holds — the pass ends with `needs_human()` true and the generation
  **not** complete.
  **(N) Residue survives a re-fence.** After (M), run the pass again with **nothing repaired**.
  The session is already `Aborting`. The second pass still names it as needing a human, and the
  new generation is **not** complete. This is the regression test for iteration 1's
  `restore.rs:916`; it must fail against that logic.
  **(O) Repair, then a second pass, and only then complete.** After (M), apply the repair the
  operator text documents (for the missing-part case, the `part:` record is put back), and run
  the pass again: the obligation the first pass could not write is now installed and decodes
  through `decode_retire_obligation`, and **only then** does the generation read complete. The
  counter-arm: a repair of one of two residue sessions leaves the generation not complete.
  **(P) The judgement comes from durable state.** The second pass runs in a fresh context that
  shares nothing in memory with the first. State in `build-notes.md` which durable fact carries
  the residue — re-deriving it from each `Aborting` session's records each pass, or a field on
  the generation record — and why a crash between passes cannot lose it.
  **(Q) A failed pass never reads complete.** A double that fails a commit mid-pass leaves the
  generation not complete.
  **(R) `cargo xtask ci` green.**
- **Falsifiability:** RED is produced in-process on the base (`origin/main` + child-2) by
  **assertion**: that base writes no generation record, so I(ii), I(iii) and O's final arm fail
  there; N and M's "not complete" arms are vacuously true on the base and earn their keep
  against mutants, so pair each with a positive arm in the same test. The test may name only
  symbols visible on its base. A compile failure on the RED leg reports UNVERIFIABLE
  (`engine/scripts/run-verify.sh`). Record in `build-notes.md` how many tests ran red.
- **Invariant to restore:** a restored image is declared fenced only when every session it
  resurrected can no longer publish, every record that session wrote has a named deleter, and
  no later pass can withdraw that judgement by forgetting what an earlier one could not repair.
  Source: 0016 D-B and decision 1.4 (`:717-728`), `:3017-3021`; the C-1 rule that a
  certification over an incomplete picture is a defect (`docs/principles.md` §5); ADR-0045.
  SELF-TEST: a pass that skips every already-`Aborting` session passes leg I and fails leg N.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Repro instruction:** on the base, run `reconcile_after_restore` over a store with one `Open`
  session, then scan the metadata store for any fence-generation key: there is none, before or
  after.
- **Scope:** one small durable record naming the post-restore pass's generation and whether it
  finished, written not-complete before the first fence and complete only after the last write
  and only with nothing needing a human; re-evaluation of already-fenced sessions on every
  pass; the record's key and shape documented for #508 in one place
  (`05-building-block-view.md:202`, plus a paragraph in `06-runtime-view.md` and the m4
  blueprint's restore steps); the operator text in `crates/server/src/cli.rs` saying whether
  the generation completed. Key name: use the one 0016 gives; if it gives none, `mpufence`,
  1-based so that absent is the only spelling of "no pass has run". / out of scope: the fence
  itself and the nonce (child-2 — change them only if leg N or O cannot pass otherwise, and say
  so in `build-notes.md`); the gateway's reading of the record (#508); drain status (child-1);
  `scrub.rs`, `reconstruction.rs` (#663); the mark codec (#804); the retire drain (#659);
  `crates/dst/tests/custodian.rs` unless an existing case stops passing; any edit to 0016 or an
  ADR.
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/custodian/tests/restore_fence_generation.rs` — a **NEW** file; the
  C4-verify gate earns its red only from an added `*/tests/*.rs`. No `Cargo.toml` change.
- **Difficulty:** high
- **Depends on:** child-2
- **Conflicts with:** child-1
- **Ordering note:** wave 3 of #664's split. Builds on child-2's fence. Conflicts with child-1
  only over the shared paragraph at `06-runtime-view.md:78`. After acceptance add
  `Conflicts with: 663, 804` (same doc), and re-point #508's `Depends on` from 664 to this id.
  **Settled at Plan, 2026-09-18:** leg P deliberately leaves *where* residue lives to Do. The
  brief binds the property — the judgement is re-derivable from durable state after a crash
  between passes — and naming the store for it here would seat the fix shape, which a brief must
  not do (`docs/principles.md` §3.1). Do picks, and records the choice and its crash argument in
  `build-notes.md`; leg P is what tests it either way.
- **Surfaces:** data
- **Do model:** opus-max
- **Production reach:** the pass under test is the production `reconcile_after_restore`. The
  generation record has **no reader** until #508's gateway gate. Until then the guarantee rests
  on the deployment ordering 0016 also allows: run the post-restore pass before re-enabling
  gateways (`0016:3017-3021`). Declared so sign-off weighs it rather than discovering it.
- **Citations expected:** Do must cite `path:line` on the target branch for every change. Peer
  callsites Do MAY open and mirror:
  * `crates/custodian/src/restore.rs:312` (`reconcile_after_restore`), `:111-215`
    (`MARK_BATCH`, `RestoreReport`, `needs_human` `:212`), and child-2's fence, by symbol.
  * `crates/core/src/multipart.rs:2250` (`decode_session_record`), `:3455`
    (`decode_retire_obligation`); the `mpuctl` singleton's codec as the model for a
    one-record singleton.
  * `crates/server/src/cli.rs:1230-1370`, `:2905-2990`.
- **Prior-art check (triage cycles):** re-run 2026-09-18 on `f41e9c5`: no merged change adds a
  fence-generation record; no open PR touches these paths. Rejected prior art: #664 iteration 1
  (`results/issue_664/iteration-v1/patch.diff`, `review-batch.md`) — three blocking findings,
  all the same defect at its `restore.rs:916`. Its generation arms (leg I) drew no finding and
  MAY be mirrored; its skip of `Aborting` sessions MUST NOT.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR MAY
happen during the cycle. The PR MUST NOT be marked ready before sign-off accepts.
<!-- pdca:end child-3 -->
