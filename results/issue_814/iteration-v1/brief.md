# custodian: reconstruction rebuilds a staged chunk under the session fence (663.2)

> Child 2 of 2 of #663's split (637.3). Do reads ONLY this file; keep the `- **Label:** value`
> lines. Citations are on `origin/main` @ `97fc2f9` (verified 2026-09-19); this bundle's base
> adds #813 (find its changes by symbol). 0016 =
> `docs/design/proposals/draft/0016-multipart-commit-protocol.md`.

- **Slug:** staged-replace
- **Kind:** enhancement
- **Defect:** after #813, reconstruction recognises a committed part's chunk but can only keep
  its obligation. Nothing rebuilds the fragment, so the part stays a fragment short until it is
  published, or forever if the client never completes. 0016 requires the rebuild (`0016:825`),
  and its failure table names "scrub staged fragments but leave reconstruction committed-only" as
  a wrong implementation (`:889`).
- **Success criterion:** the NEW file `crates/custodian/tests/staged_repair.rs` passes, one
  seeded case appended to the existing `crates/dst/tests/custodian.rs` passes, and
  `cargo xtask ci` is green. The new test names only symbols on its base, including
  `ReconstructionContext::{clock, staged_write_window_millis}` from #813, with time from a
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
- **Falsifiability:** RED in-process on this bundle's base, `origin/main` + #813. With
  `wave_mode = "merge"` #813 is merged first; a fold branch would arrive as
  `$PDCA_VERIFY_BASE`, which `engine/scripts/run-verify.sh:247-265` honours. On that base
  reconstruction keeps the obligation and writes nothing, so A, B, C(iv) and D fail by assertion.
  C(i)–(iii), (v) and (vi) fail there on the missing rebuild rather than on the rule; each rule's
  own branch must be reached in the green leg (C4-diff-cov reports it). The new test compiles on
  that base because #813 added the two context fields. It must add no other symbol the test
  names. The DST case is in a modified file, so C4-ci is its gate, not C4-verify.
- **Invariant to restore:** a staged chunk degraded while its session is `Open` is rebuilt, and
  no outcome strands a fragment: every fragment the re-place writes is, at every instant, either
  named by a record or covered by an `orphan:` mark GC can act on, and the obligation is removed
  only in the commit that makes the repair durable. Sources: C-1 (the harness catalogue
  `wyrd-pdca/docs/principles.md` §5 and its §6 storage-lifecycle row, not a target file: every
  durable byte is at every instant named by a record or evidenced for reclamation; its target
  sources are 0016's refutation standard, `0016:2802-2813`, and GC's "never reclaim a referenced
  fragment", `crates/custodian/src/gc.rs:47`); `0016:825`, `:885`, `:888-889`; "no fragment is written after its evidence may
  have been reclaimed" (`:1356-1358`); ADR-0045.
  SELF-TEST: a rebuild that writes then CASes without a pre-mark passes A and fails B.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Conflicts with:** 777
- **Ordering note:** builds on #813 (its context fields and its keep path), per #663's split
  (accepted 2026-09-19 under the human's intake-cap override, `results/issue_663/split-proposal.md`).
  `Conflicts with: 777`: #777 edits `reconstruction.rs` and the DST file. #800, #808, #809 and
  #810 name this id in their own `Conflicts with` (#800: both append to the DST file and edit
  `06-runtime-view.md`); #508 and #625 depend on it. The `W_write` constant #800 also names is
  #813's to define or reuse.
- **Surfaces:** data
- **Difficulty:** high — a multi-step metadata protocol with four fences, plus a DST property.
- **Do model:** opus-max
- **Scope:** rebuild and re-place a committed part's chunk in an `Open` session: pre-mark before
  write; deadline from the context clock at pre-mark commit plus the window; the `W_repoint` gate;
  one adoption CAS pinned to the session state, the prior `part:` bytes, the pre-mark's bytes and
  `require_absent(desired:dserver:<S_new>)`. On any loss the obligation stays queued and the
  pre-mark stands. Time and window come only from #813's fields; add no other. Extend the
  last sentence of `docs/design/architecture/06-runtime-view.md:80` with the rebuild, and drop
  `#663` from the `deferred:` marker at `gc.rs:893`.
  A committed part's chunk in an `Open` session that is found at full redundancy resolves as the
  committed path resolves one: its obligation drains as a duplicate finding
  (`reconstruction.rs:216-218`, `Assessment::Drain`); "kept" is for a chunk that needs a repair
  this pass cannot make. Keep #813's legs D–F in `crates/custodian/tests/staged_protection.rs`
  passing: D's committed-part case asserts "kept, no write" for a committed part in an `Open`
  session, which this slice makes false — retarget it to a session that is not `Open` (leg E's
  rule here), or remove it, since `staged_repair.rs` leg A owns the rebuild assertion; D's
  `sidx:`-only case and control stay. E's chunk must be degraded (a fragment lost), so the pass
  reaches this slice's re-place and loses its CAS on the published `part:` record, rather than
  draining the chunk as one at full redundancy.
  / out of scope: the committed repair path (`reconstruction.rs:829-955`, unchanged); one
  degraded chunk per part per pass — every plan in a part pins the same `part:` bytes, as the
  committed path pins one inode (`reconstruction.rs:155-159`, `:294-303`); accepted here, say so
  in a comment; a late write whose effect is `WriteEffect::Unknown` landing after GC reclaims its
  pre-mark (a gap in the deadline model shared by every writer, `crates/traits/src/lib.rs:862-902`);
  `EcScheme::None` answering Unrepairable (by design, as `reconstruction.rs:641`); servers absent
  from the live fleet; `seg:` repair (#777); drain status, rebalance, restore (#808–#810); the
  upload-side drain fence (#657); `crates/core/src/multipart.rs`; edits to 0016 or an ADR.
- **Repro instruction:** on the base (`origin/main` + #813), seed an `Open` session with one
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
- **Depends on:** 813

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Size backstop: patch is 197 KB, nearly double the 100 KB threshold. This is an oversized slice — split it in re-plan (`pdca split`) rather than patching in place. The reproduced slow-write stall (shared pre-mark timestamp starving the second write's authorization window in multi-fragment moves) is real and should be addressed as part of the split, but the driving reason for iterate-plan over iterate-do is the size backstop itself, not an attempt to scope the bug fix.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 83 mutants tested in 12m: 8 missed, 26 caught, 48 unviable, 1 timeouts
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_814/review-b
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
