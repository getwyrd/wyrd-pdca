# Brief — issue 664 / staged-drain-and-restore-fence

> Child 4 of 4 of #637's split (637.4). Do reads ONLY this file. Keep the `- **Label:** value`
> lines. `path:line` citations are on `origin/main` @ `3969a3a` (re-verified 2026-09-12). This
> bundle's base is `origin/main` **plus #661 and #662** (wave 3). #662 already makes restore's
> mark gate skip staged fragments through the shared predicate, so what is left here is
> restore's *accounting* and its *fence*. Background: the restore, rebalance and drain rows of
> 0016's decision-2 table (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:820-871`),
> the failure table `:874-890`, decision 1.4 (`:717-728`), and the fence rows of the batch table
> (`:664-665`).
>
> **Design call settled at Plan (2026-09-12, the human, option (i)):** a `Completing` session
> stores its **segment-group nonce** on its session record, alongside the fence epoch
> `PublishTarget` already carries (`crates/core/src/multipart.rs:1842-1849`). That lets the
> restore fence name the attempt's `seg:<nonce>:<E>:` records. Rejected: deriving the nonce from
> `(upload id, E)` as `0016:2333` says, because the code keeps the nonce independent of the
> upload id on purpose (`multipart.rs:3185-3190`, citing `0016:499-509`); and leaving the
> records without a deleter (v1's outcome).

- **Slug:** staged-drain-and-restore-fence
- **Kind:** enhancement
- **Defect:** four gaps, all on the operator-facing and post-restore side.
  1. **Drain status ignores staged bytes.** `reconciliation_status` answers `Satisfied` for a
     server holding only staged bytes, because `genuinely_holds` reads committed placements
     alone (`crates/custodian/src/desired_state.rs:188-196`). An operator is then told the
     server may be wiped under a live upload — the F6 trace. Its sharper form is an in-flight
     part with no `part:` record yet (`0016:827`).
  2. **Restore's report cannot tell staged skips apart.** Once #662 lands, restore skips staged
     fragments silently, through the shared gate (`restore.rs:383`). 0016 requires the report to
     say so: `staged_skipped` and `sessions_fenced` beside `pending_skipped` (`0016:823`;
     `RestoreReport`, `restore.rs:107-169`).
  3. **Restore fences no session.** A restored image can resurrect an `Open` or `Completing`
     session whose bytes are gone, and nothing stops it from completing over them (D-B,
     `0016:717-728`, F13). A `Completing` session that had already written segments needs its
     `seg:` records retired in the **same** batch as its fence, or they have no deleter anywhere
     in the design (X57, `0016:880`). Today the session record cannot even name them
     (`PublishTarget`, `multipart.rs:1842-1849`).
  4. **No durable record tells a gateway the fence has run.** 0016 requires the restore-fence
     generation to complete before any gateway serves multipart verbs on the restored image
     (`0016:723-728`, `:3017-3021`, X17b).
- **Success criterion:** the NEW file `crates/custodian/tests/staged_drain_restore.rs` passes
  over in-memory doubles, with records seeded as raw JSON the base decoders accept (the shapes in
  `crates/core/tests/multipart_session_records.rs:81-145`). A `Completing` fixture carries the
  new nonce field. Base decoding rejects it (`#[serde(deny_unknown_fields)]`), which is harmless
  there: base restore never reads `mpu:`. Legs:
  **(A) Drain counts an in-flight part as held.** Server `S` holds **only** an owned `sidx:`
  fragment, and `desired:dserver:<S>` is set: `reconciliation_status(S)` is `Pending`. On the
  base it is `Satisfied` — the red.
  **(B) Drain counts a committed part as held**, as its own case: `S` holds only a committed
  `part:` fragment, and the answer is `Pending`. An implementation counting only one class passes
  one of A and B and fails the other (`0016:883`).
  **(C) Drain still finishes when the uploads live elsewhere.** Staged fragments sit on servers
  0–2, and server 3 is draining and holds none of them and no committed reference:
  `reconciliation_status(3)` is `Satisfied`. v1's `*server != dserver` mutant survived every
  leg; this case kills it. It is green on the base as well — a guard.
  **(D) Rebalance and drain agree, and rebalance leaves staged bytes alone (`0016:881`).** For
  a draining server holding **only** staged fragments, a rebalance pass writes no fragment
  anywhere and rewrites no `part:` record, **and** `reconciliation_status` is `Pending`. The
  red comes from the `Pending` half. State in `build-notes.md` which `Reconciled` the pass
  returns there, and why it does not tell an operator the drain is done.
  **(E) Restore reports staged skips.** `reconcile_after_restore` over a store with two staged
  fragments reports them as staged-skipped, separately from `pending_skipped`. The test cannot
  name a field this slice adds, or it would not compile on the base, so assert it through the
  report's `Debug` rendering. The rendering must contain `staged_skipped: 2` (0016's name for
  the counter, `0016:823`); the base rendering has no such counter.
  **(F) Restore fences a resurrected `Open` session (D-B).** An `Open@E` session in the store
  ends as `Aborting@E+1`. In the same batch — assert atomicity with a double that fails that one
  commit, after which **none** of the writes are present — its byte-retirement obligation is
  installed. Round-trip every obligation the fence writes through `decode_retire_obligation`
  (`crates/core/src/multipart.rs:3333`) against the key it sits under, and assert it decodes.
  The fenced-session counter moves: the `Debug` rendering contains `sessions_fenced: 1` (0016's
  name, `0016:823`). A Complete retried against
  that session cannot fence it, since the Complete fence requires `Open@E` (`0016:660`). The
  client-visible `4xx` is #658's to answer.
  **(G) Restore fences a resurrected `Completing` session with its segments' deleter (X57).** A
  `Completing@E` session with `segments_written > 0`, its nonce on the record, and
  `seg:<nonce>:<E>:*` records present ends as `Aborting@E+1`. One batch installs
  `retire:bytes` naming the session and its parts, **and** `retire:records` naming exactly
  `seg:<nonce>:<E>` (`0016:665`, the "one shape for all three doors" row). Both decode through
  `decode_retire_obligation`, and the records obligation's `segments()` names that group. v1
  installed only the first and reported the records as residue. That draining empties the range
  is #659's drain to prove (it stays on #665).
  **(H) What cannot be fenced cleanly is never passed off as done.** Two cases.
  (i) A `Completing` record with **no** nonce — the pre-decision shape — fails decode. Restore
  leaves it byte-identical (ADR-0045) and names it as needing a human.
  (ii) A `Completing` session whose `seg:` records name a chunk that none of its `part:` records
  holds — a part record missing from the restored image — is still fenced, and still named as
  needing a human. v1 silently built the teardown from whatever `part:` keys were present
  (`results/issue_637/iteration-v1/review-batch.md`).
  In both cases `RestoreReport::needs_human()` is true (`restore.rs:197`), and the fence
  generation (leg I) is **not** marked complete.
  **(I) The restore-fence generation record.** Three arms, each on durable state: (i) before any
  post-restore pass, the record is absent; (ii) **during** a pass, read through a double hook at
  the first fence commit, it names the pass's generation and reads not-complete; (iii) after the
  pass, it reads complete for that generation, and only if leg H found nothing. A second pass
  **advances** the generation and reads not-complete until it finishes, so a later restore
  invalidates an earlier completion instead of being masked by it. "Complete" becomes
  observable only after every write the pass makes, the mark batches included. How a gateway
  acts on the record is #508's: document the record's key and shape for it, and keep one source
  of truth.
  **(J) The session record carries the nonce, and nothing else changes for it.** A `Completing`
  session record with the nonce round-trips byte-identically through its codec, and one without
  it is refused. Put this in the codec's own test module in `crates/core/src/multipart.rs`; it is
  green-only by nature, which is fine for a codec leg.
  **(K) `cargo xtask ci` green.**
- **Falsifiability:** RED is produced in-process on this bundle's base — `origin/main` + #661 +
  #662. When the waves run in one flow that fold is exported as `$PDCA_VERIFY_BASE`, which
  `engine/scripts/run-verify.sh` honours (`:247-265`); once both have merged, `main` itself is
  that base. Legs A, B, D, E, F, G, H and I fail by **assertion** there: the base's drain
  status counts committed placements only, and its restore reads no `mpu:` record and writes no
  fence or generation. C is a guard, and J is green-only. The new test may name only base-visible
  symbols: `wyrd_custodian::{reconcile_after_restore, reconciliation_status, set_lifecycle,
  reconcile_step, RebalanceContext, GcContext, RestoreReport, ReconciliationStatus,
  DServerLifecycle}`; `wyrd_core::multipart::{mpu_key, part_key, sidx_key, retire_key,
  decode_retire_obligation, decode_session_record, RetireMode, RetireToken, ...}`;
  `wyrd_core::metadata::seg_key`; `wyrd_traits`. It names no field or type this slice adds —
  hence the `Debug` assertions in E and F, and raw keys for the generation record. A compile
  failure on the RED leg reports UNVERIFIABLE (`run-verify.sh:521-547`). Record in
  `build-notes.md` how many tests ran red, all by assertion.
- **Invariant to restore:** no answer the custodian gives about a server or a restored image
  claims more than is true. A drain is `Satisfied` only when no byte that can still become
  referenced — committed, committed-part or in-flight — lives on that server. A restored image
  is declared fenced only when every session it resurrected can no longer publish, and every
  record that session wrote has a named deleter. Source: 0016 decision 2's drain, rebalance and
  restore rows (`0016:823`, `:826-827`), D-B and decision 1.4 (`:717-728`), X57 (`:880`,
  `:2587`); the C-1 rule that a certification over an incomplete picture is a defect
  (`docs/principles.md` §5); ADR-0045. SELF-TEST: fencing `Open` sessions alone passes F and
  leaves G's segment records with no deleter. Counting only `part:` in the drain passes B and
  fails A.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** 803
- **Conflicts with:** 693
- **Ordering note:** **Re-pointed 2026-09-15:** #662 was split at its re-plan into #803 (the
  staged reference set) and #804 (reclaim intent before deletion, and the `orphan:` value
  shapes). This slice needs the staged class only, not the mark codec, so `Depends on` names
  #803 in place of `662`. #804 does not touch `restore.rs`, but both slices edit
  `docs/design/architecture/06-runtime-view.md`, so #804 declares `Conflicts with: 664` (added
  2026-09-15 at the human's direction) and builds after this one.
  #803 leaves one question to this slice, marked `// deferred: #664` in `restore.rs`: whether a
  staged record restore HOLDS as untrusted — a wrong-length staged placement, or an undecodable
  owned value under an `sidx:` key that names its chunk — should set `needs_human()`. #803
  names such a record on the audit seam only, as restore treats a malformed committed placement
  today; decide it with defect 2's report work, and if it should count, carry it in the fields
  this slice introduces. Wave 2 of the run, alone: #803 · this slice · #804 · #663 · #800.
  **Outside the
  proposal**, added after acceptance: this slice edits `crates/core/src/multipart.rs`, which
  **#693** (ITERATE_DO) also edits. Decided at acceptance (2026-09-12): `Conflicts with: 693`, so the two never build blind on
  one base; no completion dependency. #658 (Complete) must write the nonce this slice adds when it
  fences a session into `Completing`, and #656 (Abort) should reuse the fence batch this slice
  builds rather than write a second one. Both are notes for those issues, not work here.
- **Surfaces:** data
- **Difficulty:** high
- **Do model:** opus-max
- **Scope:** four things. (1) Drain status counting both staged classes as held. (2) Rebalance
  confirmed disjoint from the staged set: a code change only if it currently moves or rewrites
  staged records. (3) Restore's staged accounting and its session fence, both shapes, with the
  obligations 0016's rows name in one batch each. Any writer-side construction added for those
  obligations must produce only values `decode_retire_obligation` accepts against their key,
  since the module withholds a writer on purpose (`multipart.rs:3036-3060`). (4) The durable
  restore-fence generation record, the nonce on the `Completing` session record (option (i)),
  and the post-restore command's report of fenced and unfenceable sessions
  (`crates/server/src/cli.rs`, whose tests build `RestoreReport` literals at `:2885-2990`).
  `RestoreReport` gains its fields plainly, with no `#[non_exhaustive]` (decided at Plan: it
  derives `Default` and is only built inside the workspace). Record the nonce decision in the new
  field's doc comment, citing the `0016:354` / `:2333` disagreement it settles. Docs currency
  (`AGENTS.md:154-157`: new persisted fields and a new persisted record): describe the fence,
  the generation record and the nonce in `docs/design/architecture/06-runtime-view.md`, and the
  post-restore exit reasons in `docs/design/architecture/m4-first-deployment-blueprint.md`.
  Must NOT change the signatures of `reconcile_after_restore`, `reconciliation_status` or
  `reconcile_step`, and must NOT add a field to any context struct. / out of scope: scrub and
  reconstruction (child-3 — do not touch `scrub.rs` or `reconstruction.rs`); the staged set
  itself and the mark codec (child-2); the retire drain that empties the obligations (#659); the
  gateway's reading of the generation record (#508); evacuating committed segmented objects
  (#653/#722); client Abort and Complete (#656, #658); any edit to 0016 or an ADR.
- **Repro instruction:** on `origin/main`, seed an owned `sidx:` entry whose fragment sits on
  server `S`, set `desired:dserver:<S>` with `set_lifecycle`, and call
  `reconciliation_status(S)`: it answers `Satisfied`.
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/custodian/tests/staged_drain_restore.rs` — a **NEW** file. The C4-verify
  gate earns its red only from an added `*/tests/*.rs` (`engine/scripts/run-verify.sh:141-144`).
  The existing dev-dependencies suffice; make no `Cargo.toml` change.
- **Production reach:** the passes under test are the production `reconciliation_status`, the
  rebalance loop and `reconcile_after_restore`. Every session is seeded by the test, because no
  client can create one until #508. The generation record has no reader until #508's gateway
  gate. Until then the guarantee rests on the deployment ordering 0016 also allows: run the
  post-restore pass before re-enabling gateways (`0016:3017-3021`). Declared so sign-off weighs
  it rather than discovering it.
- **Citations expected:** Do must cite `path:line` on the target branch for every change. Peer
  callsites Do MAY open and mirror:
  * `crates/custodian/src/desired_state.rs:181-247` — `reconciliation_status` and
    `genuinely_holds`, to test the union with the staged set.
  * `crates/custodian/src/restore.rs:280-460` — the pass, its gate (`:383`), its `pending_skipped`
    accounting (`:420-424`) and its bounded `MARK_BATCH` commit (`:103`).
  * `crates/core/src/multipart.rs:1842-1849` (`PublishTarget`), `:1886-1917` (`SessionState`),
    `:3015-3032` (the retire rows: `{session}` for the abort fence, `{session, parts}` plus
    `{seg}` for the `Completing` fence), `:2965-2979` (the `seg` component).
  * `crates/core/src/metadata.rs:763-830` — `SegmentNonce` and `SegmentGroup`.
  * `crates/server/src/cli.rs:1300-1330` and `:2880-2990` — the post-restore verdict and its
    report tests.
- **Prior-art check (triage cycles):** by path (`crates/custodian/src/desired_state.rs`,
  `restore.rs`, `crates/core/src/multipart.rs`) across merged history and open PRs: no merged
  change fences sessions or adds a fence-generation record, and no open PR touches these paths.
  Rejected prior art: #637 v1 (`results/issue_637/iteration-v1/`) fenced `Completing` sessions
  without the `seg:` deleter, because the nonce was missing. That is the design call settled
  above, and legs G and H exist for its review findings.
- **Disposition hint:** likely-fix

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Slice is oversized (206 KB patch vs 100 KB threshold) and the advisory review (both rubric and adversary passes) converged on a real correctness bug: crates/custodian/src/restore.rs:916 skips already-`Aborting` sessions on a second fence pass, so a repair-and-rerun (the documented operator remedy) can certify the restore-fence generation complete while a `Completing` session's orphaned seg: residue still has no deleter — a gateway trusting that marker would resume multipart verbs over an unfenced image. Given the size flag, treat this as a slicing problem, not an implementation bug to patch in place: split at re-plan (drain/rebalance staged-accounting vs. restore fencing + generation record vs. residue-across-generations handling) rather than iterate-do, so the rebuild doesn't keep producing implementation-shaped findings on an oversized diff. Carry into the split: residue must survive re-fencing across generations, and a repair-then-second-pass regression test is required before the fence-complete marker can be trusted.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 74 mutants tested in 3m: 9 missed, 25 caught, 40 unviable
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_664/review-b
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
