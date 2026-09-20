# Result — issue 637 / staged-byte-protection

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: durable-but-unpublished bytes — a client's uploaded parts — become a **first-class
  protection class** across the whole maintenance plane. Today nothing references them, so every
  destructive pass is entitled to reclaim them: GC deletes them given any evidence, restore
  **marks them stranded**, and a drain reports a server holding nothing but staged bytes as
  `Satisfied` so an operator wipes it under a live upload. `ReferenceSet`
  (`crates/custodian/src/gc.rs:228-247`) gains a second, **disjoint** member — the staged set,
  built from committed `part:` records **and** in-flight owned `sidx:` entries through **bounded
  per-session ranges, never a global scan** — and each of the seven consumers in 0016's table
  makes its own stated decision from it.
- Success criterion: **one NEW test file** plus seeded DST cases appended to an existing
  one (see `Test file` — the split is what protects the evidence). The new file is written to
  compile on this bundle's base so that **its first seven legs red by assertion, not by build
  failure**.
  `crates/custodian/tests/staged_protection.rs`, over in-memory trait stores, with a session's
  records seeded through #636's key helpers (present on this bundle's base) and fragments placed
  on in-memory D-server doubles:
  **(A) GC — protect, with the evidence present.** With an `Open` session holding a committed
  `part:` record and an in-flight owned `sidx:` entry, seed an `orphan:` record for one fragment
  of each **and advance past the grace window**, then run `reconcile_step`: **both fragments
  survive**. This is 0016's own oracle (`0016:877`, "leave the reference set committed-only")
  and the evidence is what makes it discriminating — without the `orphan:` record GC's
  conservative branch retains an unevidenced fragment anyway (`crates/custodian/src/gc.rs:183-187`),
  so the leg would pass vacuously on the base. **On the base, with the mark present, GC deletes
  them — that is the red.**
  **(B) Drain / desired-state — count both classes as held.** For a server holding **only** an
  in-flight owned (`sidx:`) fragment, with `desired:dserver:<S>` seeded,
  `reconciliation_status(S)` MUST answer **`Pending`** (`crates/custodian/src/desired_state.rs:150-170`).
  On the base it answers `Satisfied` — the F6 wipe trace, and the one row whose violation is an
  operator wiping live bytes. Assert the same for a committed-`part:` fragment, separately: an
  implementation that counts only `part:` passes one and fails the other (the iteration-3
  finding-4 hole).
  **(C) Restore — protect, then fence, and prove the data loss it prevents.**
  `reconcile_after_restore` over a store with a staged session MUST report
  `RestoreReport::stranded_marked == 0` (`crates/custodian/src/restore.rs:104-145`) and must have
  **skipped** those fragments as staged rather than merely not reached them. `staged_skipped` is a
  field this slice ADDS beside `pending_skipped` (`:256-263`), so naming it directly would break
  this file's base compile and destroy the assertion red — assert it in a **base-compiling** form
  instead (e.g. over the report's `Debug` rendering, which carries no such field on the base and
  does after the fix). If Do can find no base-compiling form that genuinely binds, drop the
  counter assertion from this file and say so in `build-notes.md`: `stranded_marked == 0` plus the
  survival assertion below is the binding pair, and it must not be traded away for a counter.
  On the base restore marks every staged fragment `orphan:`
  (`crates/custodian/src/restore.rs:217-300`) — so **also** assert the consequence: after that
  restore pass, advance past the grace window, run a GC pass, and assert **every staged fragment
  is still present**. On the base they are deleted. That is the leg that shows this slice
  prevents data loss rather than tidies a counter.
  **And "then fence" must be an OBSERVABLE, not prose.** D-B requires restore to fence **every**
  resurrected `Open`/`Completing` session to `Aborting`, because a records-only image cannot prove
  the staged bytes still exist (`0016:836-841`). Assert on durable state, for **both** shapes:
  an `Open` session is `Aborting` afterwards; a **`Completing`** session that had already written
  `seg:<g>:<E>:*` takes the dedicated **restore-fence transition** whose single batch installs
  `retire:bytes:{session, parts}` **and** `retire:records:{seg:<g>:<E>}` (X57 — fencing it as if it
  were `Open` leaves those `seg:` records with **no deleter in the whole design**); the
  `sessions_fenced` counter moved; and a Complete retried against a restored session is **rejected**
  rather than publishing. Without these, "protect, then fence" is satisfied by protect alone.
  **(C2) This slice OWNS the restore-fence generation record — decided 2026-07-26.** 0016 requires
  that "the restore fence generation MUST complete before any gateway serves multipart verbs on the
  restored image" (`0016:823`), and two independent plan reviews found that requirement owned by no
  slice: this brief fenced sessions but published nothing a gateway could read, and #508 turned the
  absence into a Check §6 item — passing the hole downstream. **It lands here.** Ship a single
  **durable, authoritative generation/completion record** that a gateway can read without inferring
  anything: `reconcile_after_restore` advances it, and it distinguishes **incomplete/stale** from
  **complete**. Assert all three arms on durable state: absent (no restore has run) ⇒ readable as
  not-complete; a restore pass **in progress / not yet fenced** ⇒ not-complete; after the fence
  generation completes ⇒ complete, and the value identifies **which** restore it belongs to (a
  monotonically advancing generation, so a *later* restore invalidates an earlier completion rather
  than being masked by it). #508 consumes exactly this record — it must not derive a second source
  of truth, and this slice must not leave it implicit.
  **(D) Scrub — verify AND enqueue.** A **corrupt** staged fragment (a bit-flip into a real v1
  fragment, the idiom `crates/custodian/tests/scrub.rs` already uses) MUST result in a durable
  `repair:` obligation. "Walks it" is not a passing answer: on the base scrub never sees it at
  all (it iterates `referenced.placed`, `crates/custodian/src/scrub.rs:75-110`), so assert the
  **positive** — the queued obligation exists and names that chunk.
  **(E) Reconstruction — resolve and repair, under the session fence.** A lost staged fragment
  MUST be rebuilt **and the `part:` record's `ChunkRef.placement` updated**, under the
  destination-pre-mark rule: pre-mark `orphan:<P_new>` **before** writing the destination
  fragment, then CAS the part record under `require(mpu == Open@E)` **and**
  `require(part:<id>:<n> == prior)`; **on win** adopt `P_new` (delete the pre-mark) and orphan
  `P_old`; **on loss** it is a no-op that **leaves the `P_new` pre-mark standing** so GC reclaims
  the pre-written destination (`0016:854-861`). On the base the obligation resolves to no
  committed map and is silently dropped (`Assessment::Drain`,
  `crates/custodian/src/reconstruction.rs:188-191`).
  **A changed placement is NOT sufficient — it proves metadata moved, not that a byte was
  rebuilt.** An implementation that repoints the `part:` record without writing a fragment passes a
  placement-only oracle and leaves the object one copy short. Assert the whole protocol: the
  **new** D server actually holds a fragment that is **intact and scheme-correct** for that chunk;
  the `part:` record names **that** holder; the destination pre-mark `orphan:<P_new>` has been
  **removed** on the win; `P_old` is **newly orphan-evidenced**; and the repair obligation drains
  **only** on the win. Keep the loss-branch assertions separately: on a lost CAS the `P_new`
  pre-mark **stands** and the obligation stays queued.
  **(F) Rebalance — disjoint, and consistent with (B).** For a draining server holding **only**
  staged fragments the evacuation plan MUST be **empty** while `reconciliation_status` is
  **`Pending`**. The pair is the assertion: a design that merged staged fragments into `placed`
  instead of keeping the set disjoint makes those two answers contradict each other
  (`0016:880`). A committed **segmented** object's fragments, by contrast, ARE evacuated —
  assert that too, via #635's `seg:` records.
  **(G) The ledger walk is bounded per pass, and it converges.** With a `MemMeta` whose `scan`
  enforces a **lowered** cap (the `crates/metadata-redb/tests/scan.rs:9-11` idiom, implemented in
  the test's own double — `ScanCapExceeded` is base-visible in `wyrd_traits`), seed an `orphan:`
  population **past** that cap and assert: `reconcile_step` **succeeds** (on the base
  `orphan_leases` returns `Err` from its single `scan`, `crates/custodian/src/gc.rs:322`, which
  `?`-propagates and aborts the whole reconcile step before GC, scrub, reconstruction and
  rebalance run, `crates/custodian/src/reconciliation.rs:78-85`); **no single pass materialises
  the whole ledger** (assert the per-pass page budget is respected — see
  `Design § the page-budget decision`); and **repeated passes converge**.
  **The convergence fixture must be shaped so a cursorless loop FAILS it.** A population in which
  *every* entry is actionable is passed by an implementation that restarts at the first page every
  time: it consumes that page, and the next invocation simply exposes the following one. So seed a
  **retention-safe head** — a run of marks that are **within** their grace window, larger than one
  pass's page budget — followed by an **actionable tail** beyond it. A first-page-only walk never
  reaches the tail and never converges; a cursored walk processes the tail within a bounded number
  of passes. Assert that bound explicitly, and assert the continuation survives whatever
  restart/context boundary the design chooses.
  **(H) The three `orphan:` value variants all decode, and none is ever rejected**: the legacy
  bare decimal (`crates/custodian/src/gc.rs:110-122` writes exactly that today),
  `{ orphaned_at_millis, event }`, and `{ …, reclaiming: true }`. Seed one of each and assert
  every consumer handles all three. A value that does **not** decode at all fails **closed** —
  the pass leaves it untouched, classifies it, and surfaces it (ADR-0045's metadata-validation
  boundary, `docs/design/adr/0045-metadata-validation-boundaries.md:42-65`: rewriting corrupt
  metadata is the one thing a maintenance loop may never do).
  **(H2) The GC protocols decision 2 names get binding legs of their own — "handles it" is not an
  assertion.** Each of these is a distinct normative rule with its own observable, and none is
  implied by legs A–H: **(a) keyed pending-retirement protection (X97)** — while a
  `retire:bytes:{generation}` obligation is pending, its fragments are protected by an **O(1) keyed
  lookup**, not by expanding the obligation's whole prefix; assert with a store double that
  **fails the test** if the pass ever scans the obligation prefix wholesale, and assert protection
  still holds under a backed-up drain. **(b) reclaim restart (X86)** — crash between the
  `reclaiming` CAS and `delete_fragment`, then re-run: the pass resumes and completes exactly once,
  with no double delete and no stranded `reclaiming` key. **(c) the fragment-less mark sweep
  (X87/X96)** — a position no `list_fragments()` reported, aged past
  `W_repoint + W_write + δ_clock` and observed **absent after that deadline**, is cleaned up; a
  **stale listing** must not cause a live fragment's mark to be swept. **(d) the orphan-identity
  migration gate (X92) — and note this is the OPPOSITE of what an earlier revision of this brief
  said.** 0016 requires that a mark carrying a **different** unreference-event identity **IS**
  re-stamped with fresh identity and grace (`0016:1222-1224`); the migration concern is not
  "never re-stamp a legacy value" but that **identity-keyed retirement stays DISABLED until a
  durable `orphan:`-identity cleanup has completed** (`0016:1259-1273`). So the leg is two-armed:
  **before** the durable cleanup-complete marker is observed, identity-keyed retirement is
  disabled / fail-safe; **after** a bounded cleanup writes that marker, it is enabled and a
  different-or-legacy event receives the fresh identity 0016 requires. Assert both arms and the
  marker itself.
  **(I) Reclamation intent precedes destruction.** The sweep CASes `orphan:<pos>` to `reclaiming`
  and **commits before** `delete_fragment`, deleting the key afterwards as today
  (`0016:2686-2688`). Assert with a store double that fails the commit: the fragment must still
  exist. This is the ordering the adoption CAS's precondition depends on.
  **(I2) The source-before-destination handoff order is normative, and it needs its own cases.**
  Decision 2 makes the read order `sidx:` → `part:` → committed inodes **normative**
  (`0016:782-800`), because a build that reads a destination class before its source can observe a
  chunk in **neither** — the F6 trace, and one handoff further out, **X67**'s
  `part:`-before-inode variant. Add seeded cases for **both** handoffs to the existing DST target,
  each interleaving the atomic move against the reference build. And run 0016's **classification
  sweep** (the at-least-one-safe-class helper this protocol earns, `0016:2906-2921`) **after every
  scenario in legs A–I** — asserting no gaps, never a partition, since the protocol deliberately
  overlaps protection across both handoffs. If #636 already shipped that helper, consume it;
  if not, that is a Check §6 item, not a reason to skip the sweep.
  **(J) Seeded DST for the two races this slice owns**, appended to the **existing**
  `crates/dst/tests/custodian.rs` (the Tier-0 custodian property campaign) — **NOT a new DST
  file**; see `Test file`, where the reason is mechanical and load-bearing for this slice's
  evidence: **(i)** the
  drain-request-versus-intent fence (X59, `0016:2588`): select a placement naming `S` from a
  pre-drain topology snapshot, interleave the drain request and a `reconciliation_status(S)` read
  **before** the `sidx:` intent commits — the status MAY be `Satisfied` at that instant, but the
  intent MUST then fail `require_absent(desired:dserver:S)` and **re-plan**, and no fragment of
  that part ever lands on `S`; **(ii)** the staged-replace-versus-session-fence race (X29):
  interleave a reconstruction re-place **after** it wrote `P_new` and **before** its CAS, then
  fence the session — the re-place MUST return `Conflict`, leave `P_new` covered by its pre-mark,
  and leave the obligation queued.
  **(K) `cargo xtask ci` green.**
- Repo + branch target: getwyrd/wyrd @ main
- Scope: decision 2 implemented per consumer across
  `crates/custodian/{gc,scrub,reconstruction,rebalance,desired_state,restore}.rs` and the shared
  `ReferenceSet` — the disjoint staged set, the per-consumer answers of 0016's table, the
  reclamation-intent ordering, the three `orphan:` value variants, the `scan_page` ledger walk
  with its bounded per-pass budget, and the seeded DST races. **Out of scope:** the reaper loop
  (#625); the S3 verbs (#508); the multipart records themselves (#636); **`reconcile_step`'s
  signature** (changing it destroys this slice's assertion red and collides with #625); any file
  under `docs/design/adr/` or `docs/design/specs/`, and any edit to `0016`.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (20 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 86.6% — 1043 of 1205 instrumentable changed lines executed (floor 80%); 1205 of 2586 changed lines were in
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 217 mutants tested in 5m: 27 missed, 90 caught, 100 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_637/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.10s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Issue #637's staged-byte-protection change is not ready: its shared protection core passes red→green and DST checks, but restore cannot complete the specified `Completing` fence and reconstruction can commit without valid vacated-source evidence.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | NEEDS-HUMAN | Architecture must choose a durable segment-group/frozen-part identity for `Completing` and the compatibility policy for the expanded public report — the atomic restore fence and downstream API contract otherwise remain unsettled (`crates/core/src/multipart.rs:1907`; `crates/custodian/src/restore.rs:130`). |
| C2 Reproduction (red pre-fix) | PASS | The independent base run compiled all 20 regressions and produced 19 assertion failures plus one unaffected pass, matching the frozen evidence (`gate-logs/C4-verify.log:15`). |
| C3 Change | FAIL | The required atomic `Completing` fence is not delivered: a session with written segments is fenced with only `retire:bytes`, reported as residue, and the generation is still completed, leaving `seg:` records with no deleter (`crates/custodian/src/restore.rs:369`; `crates/custodian/src/restore.rs:786`). |
| C4 Verification (red→green) | PASS | The independent run was 19 assertion-red/20 green; frozen CI also completed docs, deny, conformance, statics and DST, while the live full rerun's later Cargo advisory-lock failure was a read-only-host fault and separate statics/DST reruns passed (`gate-logs/C4-verify.log:10`; `gate-logs/C4-ci.log:2912`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Rebuild must abort a staged re-place when `P_old` carries an undecodable mark — log-and-commit leaves the vacated fragment without valid reclamation evidence and breaks the restored classification invariant (`crates/custodian/src/reconstruction.rs:1203`). |
| T1 Structure | PASS | The required one new base-compatible integration file and seeded cases appended to the existing DST target are present, and the affected-path merged/rejected prior-art audit is recorded (`crates/custodian/tests/staged_protection.rs:1`; `brief.md:350`). |
| T2 Shape | PASS | The regression target uses only base-visible seams and fails by runtime assertions rather than missing symbols, preserving the discriminating red leg (`crates/custodian/tests/staged_protection.rs:16`; `gate-logs/C4-verify.log:17`). |
| T3 Runtime | PASS | The patched integration suite ran 20/20 green and the independently rerun madsim tier ran all 22 DST tests under the configured 50-seed campaigns (`crates/dst/tests/custodian.rs:3351`; `xtask/src/main.rs:1572`). |
| T4 Contribution | FAIL | The required batched multi-pass review is red on two blocking safety defects; the separate contribution-artifact audit is N/A until its mandatory publish-time rerun, and the TiKV feature compile is green (`gate-logs/T4-batch-review.log:10`; `gate-logs/T4-contribution.log:10`; `gate-logs/host-tikv.log:207`). |
| T5 Judgment | NEEDS-HUMAN [impl] | Rebuild must add missing-part and undecodable-source counterexamples and replace the test's acceptance of unretired segments with the mandated atomic retirement assertion — current judgment blesses the known hole (`crates/custodian/tests/staged_protection.rs:889`; `crates/custodian/tests/staged_protection.rs:1128`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Human sign-off must decide whether the final repaired behavior is fit for destructive production maintenance — a false all-clear can reclaim upload bytes or leave unbounded metadata residue (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:820`). |

### Advisory — adversary

# Adversarial review — issue 637 (staged-byte protection)

What I checked: patch.diff against the target at `$PDCA_TARGET` (the patch is applied there). I re-read
the red/green log, ran three of the surviving C5 mutants against `cargo test -p wyrd-custodian` in a
scratch copy (all three survived), and wrote one probe test, in scratch only. All scratch work has been
deleted.

## Findings

- NEEDS-HUMAN [human] — **The stale-mark cleanup can leave a deleted object's bytes with no way to be
  reclaimed.** `crates/custodian/src/ledger.rs:363-381` drops any lapsed legacy mark on a fragment that
  `definitely_references` (`crates/custodian/src/gc.rs:356-358`) says is named. That check counts a
  `part:` record from a session in *any* state (`crates/custodian/src/staged.rs:19-24`). The failing case,
  reproduced with a scratch test on the patched tree:
  1. An upload publishes. Its `part:` record still exists, waiting for the records-mode
     `retire:records:{parts}` drain, which by design writes no marks.
  2. The object is deleted by `metadata::unlink`, which writes legacy marks
     (`crates/core/src/metadata.rs:1881-1899`).
  3. A GC pass after the grace window drops all three marks as "stale" (0 left).
  4. The part record then retires.

  After four more GC passes, all 3 fragments are still on disk with no mark and no reference. The test
  file's own classification sweep (`Rig::unclassified`, `crates/custodian/tests/staged_protection.rs:448`)
  flags all 3 as gaps. That breaks the brief's invariant ("garbage-with-a-sound-reclamation-path"). The
  base tree does reclaim these bytes. This can't happen until #508 publishes uploads, but the rule that
  causes it lands in this diff. A human needs to pick the fix. One option is to re-stamp instead of
  dropping when only a staged record names the fragment (0016:1254-1255 allows "re-stamps or drops").
  Another is to drop only against committed references.

- NEEDS-HUMAN [impl] — **No test covers the path where GC's reclaim-intent CAS loses.** At
  `crates/custodian/src/ledger.rs:483-497`, when the batched intent CAS conflicts, the per-mark
  fallback is the only code that decides *not* to delete a fragment whose mark changed under the pass.
  Changing `== CommitOutcome::Committed` to `!=` at `:492` deletes exactly those fragments. That mutant
  passes the whole custodian suite (I confirmed it; C5 lists it too). Leg I's store double fails commits
  with `Err`, never `Conflict` (`staged_protection.rs:180-184`), so the loop never runs, and no DST case
  rewrites a mark GC has already read. Add a case where one mark in the batch changes between the walk
  and the intent commit, and assert its fragment survives.

- NEEDS-HUMAN [impl] — **Two safety checks in the staged re-place have no tests.**
  1. `crates/custodian/src/reconstruction.rs:1148-1162` must re-stamp a destination mark that has a
     different identity. If the check at `:1152` is inverted, the old mark is reused as the pre-mark with
     its old stamp. That mutant passes every custodian test (I confirmed it). The failing case it hides:
     P_new has an old, empty mark. The mutant reuses it, and the custodian dies after issuing the put.
     The next leader's GC sweeps the empty mark at once, because it is already past the 45 s deadline. The
     delayed write (deadline now+40 s) then lands with no reference and no evidence. No test seeds any
     mark at the destination.
  2. The write deadline at `:1179-1188` is never checked, and it is what makes the 45 s sweep safe. Both
     D-server doubles ignore it (`staged_protection.rs:242-248`, `crates/dst/tests/custodian.rs:282-290`),
     so the `+`→`-`/`*` mutants at `:1179` survive and the `is_write_deadline_expired` → `Aborted` branch
     never runs.

- NEEDS-HUMAN [impl] — **No test checks that a drain can still reach `Satisfied` while uploads live on
  other servers.** At `crates/custodian/src/staged.rs:95-100`, changing the check to `*server != dserver`
  makes every drain `Pending` whenever any staged byte exists anywhere. All 20 legs still pass (I
  confirmed it). Legs B and F only ever assert `Pending` (`desired_state.rs:201-205`). Add a case with
  staged fragments on servers 0–2 and a drained server 3 that holds none, and assert `Satisfied`.

- NEEDS-HUMAN [impl] — **Legs G and H2(c) don't pin the numbers they claim to check.**
  - G asserts `read < ledger` plus a bound derived from the pass's own measured read
    (`staged_protection.rs:1379-1396`). Any per-pass budget under 3,020 marks passes, and the `<`→`<=`
    mutant at `ledger.rs:233` survives.
  - H2(c) only checks at 10 s and 50 s (`staged_protection.rs:1704-1724`), so any late-write deadline in
    (10 s, 50 s] passes. Dropping `δ_clock` (`ledger.rs:106-107`, 45 s → 35 s) survives.
  - The unit test at `ledger.rs:667-671` compares against a hard-coded 60 000, not the deployed
    `GC_GRACE_WINDOW_MILLIS` (`crates/server/src/custodian.rs:114`).

- NEEDS-HUMAN [human] — **19 tests went red, not 20, and the second half of leg F was replaced by a test
  that already passes on the base.** The C4-verify row in `check-gates.json` says "20 test(s) ran red",
  but `gate-logs/C4-verify.log` shows `1 passed; 19 failed`. The test that passes on the base is
  `f_a_committed_segmented_fragment_on_a_draining_server_is_held_and_never_dropped`
  (`staged_protection.rs:1263`). The brief asked for a committed segmented object's fragments to be
  **evacuated**. Rebalance still refuses them (`crates/custodian/src/rebalance.rs:642`, an existing
  `deferred: #682`), and the test asserts that refusal. If the human accepts the #682 deferral, this is
  settled, but the gate text still overstates the evidence.

- NEEDS-HUMAN [human] — **X57 isn't implemented: a fenced `Completing` session's `seg:` records get no
  deleter.** `crates/custodian/src/restore.rs:716-722` and `:798-801` install only
  `retire:bytes:{session, parts}`. The session is listed in `segments_unretired` because its record
  doesn't carry the segment-group nonce. The brief (leg C and Open question 2) requires
  `retire:records:{seg:<g>:<E>}` in the same batch. Leg C asserts the report line instead
  (`staged_protection.rs:889-900`). This is the §6 item the brief predicted.

- NEEDS-HUMAN [human] — **Three legs prove less than their names say.**
  - (a) "A retried Complete is refused" (`staged_protection.rs:953-965`) is a hand-built CAS on the
    session bytes from before the fence. Any write to that record would make it conflict, and no Complete
    path exists until #508. The real check is the `Aborting` state asserted above it.
  - (b) In J(i), the drain fence comes from the test's own batch (`crates/dst/tests/custodian.rs:2531-2560`,
    "emulated since the production writer is #657's"). Only `reconciliation_status` there is production
    code, so X59 isn't shown for any production writer.
  - (c) The I2 classification sweep is written twice, inside the tests (`staged_protection.rs:438-512`,
    `crates/dst/tests/custodian.rs:2424`), not as a shared helper. The brief calls that a §6 item.

## What I tried and could not break

- **Read order.** `staged.rs:134-201` reads `sidx:` before `part:` for each session. `gc.rs:414-415`
  builds the staged set before scanning `inode:`. Reconstruction builds the staged set before its
  committed reading (`reconstruction.rs:190-191`). I found no interleaving that hides a chunk from both
  classes.
- **Intent before delete, and resume.** The intent commits before `delete_fragment`
  (`ledger.rs:475-505`). A `reclaiming` mark is re-checked against the reference set before it resumes
  (`ledger.rs:352-360`). A destination holding a `reclaiming` mark aborts the re-place
  (`reconstruction.rs:1150`). The deployed passes run one after another (`crates/server/src/custodian.rs`,
  "the passes run sequentially"), so GC and a re-place in the same leader never overlap.
- **Restore marks the fence generation complete before its mark pass** (`restore.rs:370-381`, which runs
  before the reference build at `:397` and the fleet listing at `:455`). If a gateway starts accepting
  uploads the moment it reads "complete", this same pass could mark a new upload's fragments. GC's
  staged check (`gc.rs:215`, `ledger.rs:363`) still protects them, so I found no loss path, only a stale
  mark and an inflated `stranded_marked`.
- **Shape of the red leg.** All 19 reds are assertion panics, not build errors, and `reconcile_step` is
  still called with 7 arguments (`staged_protection.rs:358`). The DST cases ran in C4-ci
  (`gate-logs/C4-ci.log:3435-3450`).
- **Surviving C5 mutants in `crates/core`** (`orphan.rs:222`, `multipart.rs:3772`) look like a tool
  artifact: cargo-mutants runs only the owning crate's tests, and the callers of those methods are in
  `wyrd-custodian`. The same goes for `restore.rs:261` (`needs_human`), which the restore-verdict test in
  `crates/server/src/cli.rs:2988` covers.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C1 Spec — Architecture must choose a durable segment-group/frozen-part identity for `Completing` and the compatibility policy for the expanded public report — the atomic restore fence and downstream API contract otherwise remain unsettled (`crates/core/src/multipart.rs:1907`; `crates/custodian/src/restore.rs:130`).
- [ ] C5 Causal adequacy — Rebuild must abort a staged re-place when `P_old` carries an undecodable mark — log-and-commit leaves the vacated fragment without valid reclamation evidence and breaks the restored classification invariant (`crates/custodian/src/reconstruction.rs:1203`).
- [ ] T5 Judgment — Rebuild must add missing-part and undecodable-source counterexamples and replace the test's acceptance of unretired segments with the mandated atomic retirement assertion — current judgment blesses the known hole (`crates/custodian/tests/staged_protection.rs:889`; `crates/custodian/tests/staged_protection.rs:1128`).
- [ ] Validation — fitness-to-purpose — Human sign-off must decide whether the final repaired behavior is fit for destructive production maintenance — a false all-clear can reclaim upload bytes or leave unbounded metadata residue (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:820`).
- [ ] **The stale-mark cleanup can leave a deleted object's bytes with no way to be
- [ ] **No test covers the path where GC's reclaim-intent CAS loses.** At
- [ ] **Two safety checks in the staged re-place have no tests.**
- [ ] **No test checks that a drain can still reach `Satisfied` while uploads live on
- [ ] **Legs G and H2(c) don't pin the numbers they claim to check.**
- [ ] **19 tests went red, not 20, and the second half of leg F was replaced by a test
- [ ] **X57 isn't implemented: a fenced `Completing` session's `seg:` records get no
- [ ] **Three legs prove less than their names say.**
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 2 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_637/review-b
- [ ] size backstop — this slice is behaving oversized: patch is 334 KB (threshold 100 KB); patch touches 20 files (threshold 20). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Plan
- Iteration delta (if iterating): Too big for one slice: 334 KB patch across 20 files (both over the size backstop threshold), plus a genuine unresolved architecture decision buried inside the build (durable identity scheme for `Completing` sessions and the compatibility policy for the expanded restore report). The review and adversary pass also turned up multiple independent, unrelated safety gaps spread across different code paths (re-place safety, drain-with-live-uploads, numeric bounds on two legs, restore fence not installing the segment deleter) rather than one bug — evidence this is several coherent units of work glued together, not one that a rebuild can converge. Re-plan and split with `pdca split`; the architecture decision (identity/compat policy for `Completing`) needs to be settled as part of that split, not decided implicitly by a rebuild.
- By / date: Eduard Ralph / 2026-09-12

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
