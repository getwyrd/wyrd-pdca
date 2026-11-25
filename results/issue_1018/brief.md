# mpu: operator abort of an `Open` / `Completing` session (FU-6 part 1 of 2, split from #633)

> Do reads ONLY this file. Plan of record: proposal **0016**,
> `docs/design/proposals/draft/0016-multipart-commit-protocol.md` as it stands in the checkout
> (merged by PR #627). **Read before writing code:** decision 3's `Completing → Aborting` edge
> (`0016:572`, batch rows `0016:664-665`), the exit table (`0016:567-579`), and implementation
> order point 3 (`0016:255-262`). This is the first half of FU-6 (`0016:2873`). The terminal
> expiry verb and the foreign-clock alarm are a **sibling child** that builds on this one. Do
> not build them here.
>
> **Interim-surface exception GRANTED, Eduard Ralph, 2026-07-25:** "deviation granted as we
> can't wait for 0008." The verbs land on the existing dev `wyrd` CLI, not on proposal 0008's
> management plane. The grant has three terms, and they are Scope obligations:
> - **(i) authority is host access:** no listener, no network surface, no new auth path;
> - **(ii) no bulk form:** exactly one `--upload-id` per invocation, never a wildcard;
> - **(iii) migration is owed to M8:** the architecture-doc entry records that these verbs
>   move onto the management API when it lands, with semantics and records unchanged.

- **Slug:** mpu-operator-abort
- **Track:** alpha
- **Kind:** enhancement (design proposal)
- **Defect:** #625's reaper deliberately **skips** a session whose `clock_source` it does not
  own (`0016:1889-1907`, `:1984-1998`). For a foreign-clocked `Open` or `Completing` session, no
  actor in the system can move it. It keeps its records, its staged bytes and its admission slot
  forever. `MAX_SESSIONS` of them is a permanent `503 SlowDown` on `CreateMultipartUpload` with
  no exit. 0016 names the operator as the exit (`0016:255-262`), but no operator verb exists.
  `crates/server/src/cli.rs` has no multipart or session-management subcommand
  (`:412-428`).
- **Success criterion:** one new test file (see `Test file`), every leg asserted on durable
  store state or on the subprocess's exit status and stderr.
  **(A) Operator abort of an `Open` session.** Seed an `Open@E` session with staged parts.
  - `wyrd mpu abort --upload-id <ID> --data-dir <DIR>` exits 0.
  - The session is left `Aborting@E+1` with `retire:bytes:{session}` installed (the Open
    door's payload, `0016:664`).
  - A following pass of #625's operator one-shot, invoked as argv, tears it down to zero:
    records gone, `orphan:` marks written, `mpuctl.count` decremented. This shows the verb hands
    off to the ordinary clock-free path instead of growing a second teardown.
  **(B) Abort of a `Completing` session ALSO retires that attempt's segments.** Seed a
  `Completing@E` session that has already written `seg:<g>:<E>:*` records, plus a decoy
  `seg:<g>:<E+2>:*` range.
  - The verb leaves the session `Aborting@E+1` **and** `retire:records:{seg:<g>:<E>}` present.
  - `retire:bytes:{session, parts}` is installed too (`0016:665`).
  - The decoy later-epoch range is **not** named by any obligation (F18).
  - A following one-shot pass drains the named range empty and leaves the decoy intact.
  **(C) Abort refuses where 0016 says it must, and the refusal is DISCRIMINATING.**
  - A `Completed` session (tombstone) and an absent session are **not** abortable.
  - An already-`Aborting` session is idempotent: exit 0, no second fence, no double obligation,
    epoch unchanged.
  - **"Store unchanged + non-zero" is not an oracle here.** On the base, an unknown command
    also leaves the store untouched and exits non-zero (**2**, `cli.rs:419-423`).
  - Each refusal therefore asserts the **handled** exit status **1** (`ExitCode::FAILURE`,
    `cli.rs:429-433`) **and** a stderr message naming the session's state. For `Completed`, the
    message says abort does not apply to a terminal session. (The pointer to the expiry verb is
    added and asserted by the sibling child.)
  **One-batch atomicity is REQUIRED but deliberately NOT in the binding criterion.**
  - Both doors MUST commit their state change and their obligation(s) in one batch
    (`0016:664-665`). For the `Completing` door, no interleaving may leave a fenced session whose
    segments have no reclaimer (the X57 hole).
  - An end-state scan cannot tell one batch from two, so Do owes atomicity outside C4, in two
    parts:
    - (i) seeded DST cases **appended to the existing** `crates/dst/tests/custodian.rs`, one per
      door, that interrupt between the two commits and show no reachable unreclaimed state;
    - (ii) the batch construction, stated in `build-notes.md` for the reviewer.
  - If Do cannot construct the interleaving, say so and record the property as a code-read
    obligation. Do not claim a test proves it.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Reproduction:** on this child's base (`origin/pdca-integration/main` carrying #625 and its
  prerequisites #693, #658, #659):
  1. Create a temp dir and open `RedbMetadataStore::open(dir.join("meta.redb"))`.
  2. Seed the sessions above as **raw JSON bytes**, or through the prerequisite slices' public
     helpers where they are `pub`, plus `mpuctl`. Then **drop the store**: redb holds an
     exclusive file lock.
  3. Run `Command::new(env!("CARGO_BIN_EXE_wyrd"))` with the verb's argv, wait with a bounded
     timeout, reopen the store and scan the prefixes.

  On the base the command is unknown: exit 2, stderr `unknown command`, every record exactly as
  seeded.
- **Falsifiability:** RED is produced in-process on the folded wave base, with no deploy stack.
  This is a stacked bundle, so the driver exports `$PDCA_VERIFY_BASE =
  origin/pdca-integration/main` (`src/pdca_harness/flow.py:459`, `gates.py:352-360`), and
  `run-verify.sh` honours it (`engine/scripts/run-verify.sh:186-192`). If that export is
  missing, the test fails to **compile** against `origin/main`. The RED leg's non-zero branch has
  **no zero-test guard** (`:415-434`), so it would print a false PASS. Do MUST record, from the
  RED leg, **how many tests ran and failed**. A zero-test run is a non-result.
  **The test may reference only symbols that exist on the base.** It drives the verb and the
  one-shot as **argv strings** through the real binary, never through a symbol this child adds.
- **Invariant to restore:** **no state of an upload session is absorbing.** For every
  non-terminal state, including one the reaper declines to judge, some actor can drive the
  session out. Where the clock cannot supply the judgment, the operator does, and the teardown
  that follows stays clock-free.
  - **Source:** 0016 invariant (3) and the exit table (`0016:567-579`), implementation order
    point 3 (`0016:255-262`).
  - **Provenance:** `docs/principles.md` §6 storage-lifecycle category, catalogue invariant
    **C-1**: a permanent or data-losing failure mode is never an acceptable cost (maintainer's
    standing rule, 2026-07-25; 0016 `:2802-2813`; `crates/custodian/src/gc.rs:22-25`).
  - Structural change, so §1.2 applies: the smallest change that restores the invariant.
- **Scope:**
  - **The `wyrd mpu` subcommand group** in `crates/server/src/cli.rs`: a dispatch arm, a
    `usage()` line, `ParsedArgs` / `VALUELESS_FLAGS` conventions (`:2244-2284`), and
    `cmd_custodian`'s stray-positional refusal and its "a value after a valueless flag does NOT
    disable it" rule (`:913-930`).
  - **The `abort` verb.** Exact argv, verbatim: `wyrd mpu abort --upload-id <ID> --data-dir
    <DIR>`, plus the role's standard `--metadata-backend <B>` and log flags. Flags only, exactly
    one `--upload-id`, no clock-source flag.
  - **The fence.** It fences `Open@E → Aborting@E+1` or `Completing@E → Aborting@E+1` in **one**
    batch preconditioned on the session record's exact bytes (`WriteBatch::require`,
    `crates/traits/src/lib.rs:825-843`). **The two doors carry different payloads and must not
    be conflated:**
    - `Open` gets 1 put + `retire:bytes:{session}` (`0016:664`);
    - `Completing` gets 1 put + `retire:bytes:{session, parts}` + `retire:records:{seg:<g>:<E>}`
      for **that fence epoch only** (`0016:665`).
  - **Reuse, don't rewrite.** The `Completing` door is "one shape for all three doors" with the
    restore fence and the `W_session` edge: **reuse** the path #658 landed. Do not write a third
    copy.
  - **Refusals.** Refuse `Completed` / absent sessions with the handled error. Be idempotent
    against an already-`Aborting` session.
  - **Tests and docs:**
    - the DST cases appended to `crates/dst/tests/custodian.rs`;
    - the living architecture docs (`docs/design/architecture/06-runtime-view.md`,
      `08-crosscutting-concepts.md`), updated for the `mpu` group and the abort verb, including
      grant term (iii). This is a merge requirement (`AGENTS.md` "Docs currency").

  **Out of scope:**
  - the `expire` verb, the session-record field, and **any** edit under `crates/custodian/src/`
    (the sibling child);
  - the alarm;
  - the reaper's windows, clock guard and teardown (#625);
  - the record classes and state machine (#693/#658/#659);
  - any S3 wire-visible verb (#508);
  - the 0008 management plane;
  - FU-2 / FU-3 / FU-4;
  - any file under `docs/design/adr/` or `docs/design/specs/`, and any edit to 0016.

  Anything else is a Check §6 item, not a silent edit to a layer below.
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/server/tests/multipart_operator_abort.rs`. This is a **NEW** file: the
  C4 gate discriminates on an added `crates/<c>/tests/<t>.rs` (`run-verify.sh:93`) and degrades to
  green-only otherwise (`:392-402`). It must not be `#![cfg(...)]`-gated (`:347-366`). The DST
  cases go in the EXISTING `crates/dst/tests/custodian.rs`. Every wall-clock read needs the
  annotation the workspace lint demands (`clippy.toml`, wyrd#619): state which clock owns it.
- **Verification posture:** flippable red→green at Check for legs A–C. Record these
  **negation runs** in `build-notes.md`, each with the leg that catches it:
  1. Fence `Completing` **without** `retire:records:{seg:<g>:<E>}`. Leg B catches it (X57).
  2. Install that obligation in a **second** batch. **Leg B cannot catch this.** The DST case
     catches it, or it becomes a code-read obligation.
  3. Name the whole `seg:<g>:` range instead of the fence epoch's. Leg B's decoy assertion
     catches it (F18).
  4. Let abort apply to `Completed`. Leg C catches it.
  5. Make the verb delete records itself. Leg A's "the one-shot did the teardown" ordering
     catches it.
- **Production reach:** the verb runs through the real CLI dispatch and commits real batches to
  a real store. Nothing is honoured only by a test double.
- **Surfaces:** data
- **Difficulty:** high
- **Size estimate:** ≈60–80 KB (CLI group + verb, fence reuse, subprocess suite, two DST cases,
  docs).
- **Depends on:** 1015, 1007, 1013
- **Conflicts with:**
- **Ordering note:** first wave of #633's split.
  - **External edges: after acceptance, add `Depends on: 625, 693, 658, 659`** to this line.
    These are the parent's 2026-09-11 edges, unchanged. They are not written into the field
    above because `pdca split --accept` refuses any ordering ref that is not a sibling label
    (`src/pdca_harness/split.py:396-408`). **Until they are added, this child is unblocked on
    bare `main` and must not be run.**
  - Re-point `625` / `658` / `659` to the child that lands the needed piece once those parallel
    splits are accepted. Whether `625` can be dropped is a human decision recorded in the split
    proposal.
  - **The sibling child** depends on this one: it extends the `mpu` group created here.
  - **Release:** ships in the same stack as #625 and with or before #508 (`0016:255-262`). The
    PR MUST NOT be marked ready before sign-off.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: The core fence holds (one-batch abort, exact-attempt F18 reclaim, X57 — the adversary could not refute them); keep the design. Fix the implementation-level findings, all small: - CLI parsing: refuse a valued flag whose value is missing or option-shaped (`--data-dir --bogus`) before any store access; add subprocess cases for an unknown flag (`--clock-source logical`) and for repeated non-id flags alongside a repeated `--upload-id` (kills the two C5 survivors at cli.rs:3240 / :3272), each asserting exit 1 and nothing written. - Operator output: the teardown hint must echo the effective `--data-dir` (quoted) and `--metadata-backend`; after a CommitUnknownResult, a later absence must not be reported as "nothing written" (report "possibly fenced and already torn down"), and do not attribute a rival's identical fence to this invocation. - Tests: pass `--metadata-backend redb` (or clear WYRD_METADATA_BACKEND) in both subprocess helpers; add the Completing-door case where only the records obligation key is already taken (ObligationKeyTaken naming it, nothing written); add a DST schedule where an already-submitted fence commit lands after the operator gives up, and check reclamation afterwards. - Doc one sentence on ABORT_ROUNDS being sized for the Open door (a live completer can exhaust it; fails closed). Decided at sign-off: `{session, all}` is ACCEPTED for the operator-abort Completing door (leg B is met with `PartScope::All`). Fix the RetirePayload table at `crates/core/src/multipart.rs` so it says all three Completing→Aborting doors (restore, reaper W_session ceiling, operator abort) use `{session, all}` — not that the reaper uses `{session, parts}`. Do not edit 0016 here; the proposal update is tracked separately.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  The core fence holds (one-batch abort, exact-attempt F18 reclaim, X57 — the adversary could not refute them); keep the design. Fix the implementation-level findings, all small:
  - CLI parsing: refuse a valued flag whose value is missing or option-shaped (`--data-dir --bogus`) before any store access; add subprocess cases for an unknown flag (`--clock-source logical`) and for repeated non-id flags alongside a repeated `--upload-id` (kills the two C5 survivors at cli.rs:3240 / :3272), each asserting exit 1 and nothing written.
  - Operator output: the teardown hint must echo the effective `--data-dir` (quoted) and `--metadata-backend`; after a CommitUnknownResult, a later absence must not be reported as "nothing written" (report "possibly fenced and already torn down"), and do not attribute a rival's identical fence to this invocation.
  - Tests: pass `--metadata-backend redb` (or clear WYRD_METADATA_BACKEND) in both subprocess helpers; add the Completing-door case where only the records obligation key is already taken (ObligationKeyTaken naming it, nothing written); add a DST schedule where an already-submitted fence commit lands after the operator gives up, and check reclamation afterwards.
  - Doc one sentence on ABORT_ROUNDS being sized for the Open door (a live completer can exhaust it; fails closed).
  Decided at sign-off: `{session, all}` is ACCEPTED for the operator-abort Completing door (leg B is met with `PartScope::All`). Fix the RetirePayload table at `crates/core/src/multipart.rs` so it says all three Completing→Aborting doors (restore, reaper W_session ceiling, operator abort) use `{session, all}` — not that the reaper uses `{session, parts}`. Do not edit 0016 here; the proposal update is tracked separately.
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 32 mutants tested in 12m: 2 missed, 18 caught, 11 unviable, 1 timeouts
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 15 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1018/review
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Keep the design: the one-batch fence, F18 exact-attempt reclaim and X57 closure held against the adversary again. Fix only the implementation findings: - Cleanup hint, store selection: resolve `--data-dir` to an absolute path (std::path::absolute) in both the hint and the `[store: …]` label; include the effective, shell-quoted network connection settings (WYRD_TIKV_PD_ENDPOINTS / WYRD_FDB_CLUSTER_FILE as resolved) in the printed command. Test: replay the hint after a working-directory change, and a command-scoped env override surviving into the hint. - Cleanup hint, stop condition: do not say "run until it reports satisfied". Give a per-upload stop condition (until `mpu:<id>` is gone; a BLOCKED pass naming other uploads does not mean this one stalled). Add a two-session subprocess case (abort X while Y stays held). - C5 survivors (cli.rs:3216/:3223): assert `"action":"operator-abort-fenced"` on leg A's stderr; for the unreachable-via-redb `FencedUnattributed` arm, either make it assertable from the DST unknown-outcome row or record in build-notes why that survivor is accepted. - `crates/core/src/multipart.rs:4157`: keep `{session, all}`, but mark the reaper W_session writer as "(#1017, not yet built)". - REQUIRED: add a seeded DST case racing `operator_abort` against a live Complete (interleave the completer's segment batches and flip with the abort). Every end state must be either `Completed` with no obligation naming `seg:<g>:<E>`, or `Aborting@E+1` with `seg:<g>:<E>` named and drained by the following passes. - Decided at sign-off: exit 2 for a valued flag with a missing trailing value is REJECTED as a finding — exit 2 is the usage-error code and the global pre-parser's behaviour is deliberate (#531). Do not change it; record the rejection in build-notes.md.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Keep the design: the one-batch fence, F18 exact-attempt reclaim and X57 closure held against the adversary again. Fix only the implementation findings:
  - Cleanup hint, store selection: resolve `--data-dir` to an absolute path (std::path::absolute) in both the hint and the `[store: …]` label; include the effective, shell-quoted network connection settings (WYRD_TIKV_PD_ENDPOINTS / WYRD_FDB_CLUSTER_FILE as resolved) in the printed command. Test: replay the hint after a working-directory change, and a command-scoped env override surviving into the hint.
  - Cleanup hint, stop condition: do not say "run until it reports satisfied". Give a per-upload stop condition (until `mpu:<id>` is gone; a BLOCKED pass naming other uploads does not mean this one stalled). Add a two-session subprocess case (abort X while Y stays held).
  - C5 survivors (cli.rs:3216/:3223): assert `"action":"operator-abort-fenced"` on leg A's stderr; for the unreachable-via-redb `FencedUnattributed` arm, either make it assertable from the DST unknown-outcome row or record in build-notes why that survivor is accepted.
  - `crates/core/src/multipart.rs:4157`: keep `{session, all}`, but mark the reaper W_session writer as "(#1017, not yet built)".
  - REQUIRED: add a seeded DST case racing `operator_abort` against a live Complete (interleave the completer's segment batches and flip with the abort). Every end state must be either `Completed` with no obligation naming `seg:<g>:<E>`, or `Aborting@E+1` with `seg:<g>:<E>` named and drained by the following passes.
  - Decided at sign-off: exit 2 for a valued flag with a missing trailing value is REJECTED as a finding — exit 2 is the usage-error code and the global pre-parser's behaviour is deliberate (#531). Do not change it; record the rejection in build-notes.md.
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 46 mutants tested in 12m: 2 missed, 33 caught, 11 unviable
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 9 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1018/review-
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: Keep the design: the one-batch fence, F18 exact-attempt reclaim and X57 closure held for a third round (adversary could not refute). Fix only these four items, nothing else, and do not grow the printed hint further: - F1 (crates/server/src/cli.rs:3273-3276): when the absolute FDB cluster-file path is not valid UTF-8, REFUSE before store access (same as the redb `absolute_data_dir` path does) instead of falling back to the relative path. Add the non-UTF-8-working-directory regression. - F2 (cli.rs:3268/3277): carry the effective WYRD_FDB_EXTERNAL_CLIENT_DIR, made absolute and shell-quoted, into both printed follow-up commands, alongside the cluster file. - F3 (cli.rs:3369-3374): the stop-condition hint must say what to do when a BLOCKED pass names THIS upload (e.g. a partial-fleet hold): stop repeating the metadata-only one-shot and run the custodian with `--endpoints` (whole fleet). CLI text + test only; no custodian change. - F4 (crates/server/tests/multipart_operator_abort.rs:257-278, :986): bound the whole subprocess lifecycle including stderr draining; use `exec` in the `sh -c` commands or kill+reap the process group on timeout. Optional, small: in the completed-race DST row (crates/dst/tests/custodian.rs:9307/:9427) keep the actual abort outcome and require OperatorAbortOutcome::Completed for that coverage row. Keep the patch from growing further — no new features, no extra hint detail beyond the above.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Keep the design: the one-batch fence, F18 exact-attempt reclaim and X57 closure held for a third round (adversary could not refute). Fix only these four items, nothing else, and do not grow the printed hint further:
  - F1 (crates/server/src/cli.rs:3273-3276): when the absolute FDB cluster-file path is not valid UTF-8, REFUSE before store access (same as the redb `absolute_data_dir` path does) instead of falling back to the relative path. Add the non-UTF-8-working-directory regression.
  - F2 (cli.rs:3268/3277): carry the effective WYRD_FDB_EXTERNAL_CLIENT_DIR, made absolute and shell-quoted, into both printed follow-up commands, alongside the cluster file.
  - F3 (cli.rs:3369-3374): the stop-condition hint must say what to do when a BLOCKED pass names THIS upload (e.g. a partial-fleet hold): stop repeating the metadata-only one-shot and run the custodian with `--endpoints` (whole fleet). CLI text + test only; no custodian change.
  - F4 (crates/server/tests/multipart_operator_abort.rs:257-278, :986): bound the whole subprocess lifecycle including stderr draining; use `exec` in the `sh -c` commands or kill+reap the process group on timeout.
  Optional, small: in the completed-race DST row (crates/dst/tests/custodian.rs:9307/:9427) keep the actual abort outcome and require OperatorAbortOutcome::Completed for that coverage row.
  Keep the patch from growing further — no new features, no extra hint detail beyond the above.
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 12 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1018/review
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
