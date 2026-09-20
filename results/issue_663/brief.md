# Brief — issue 663 / staged-scrub-and-repair

> **Split parent.** Re-planned 2026-09-19 after three builds of the whole slice (`iteration-v1..v3/`)
> on the human's `iterated-to-Plan` sign-off of v3 (Eduard Ralph, 2026-09-15: "split into smaller
> briefs"). The work is divided in `split-proposal.md` into **663.1** (scrub checks committed staged
> fragments; reconstruction keeps their repair queued; the reconstruction time source and write
> window) and **663.2** (reconstruction rebuilds a staged chunk under the session fence). This file
> states the whole outcome; the children carry the leg-level criteria Do builds against.
> Citations are on `origin/main` @ `97fc2f9` (verified 2026-09-19). #803 and #804, this slice's
> former prerequisites, are merged there. 0016 = `docs/design/proposals/draft/0016-multipart-commit-protocol.md`.

- **Slug:** staged-scrub-and-repair
- **Kind:** enhancement
- **Defect:** staged redundancy decays untended. **Scrub** walks only committed placements
  (`crates/custodian/src/scrub.rs:88`, `:130-203`), so a committed part's fragment can rot or
  vanish for the hours a session stays open and nothing notices. **Reconstruction** resolves an
  obligation only against committed inodes (`read_committed`,
  `crates/custodian/src/reconstruction.rs:468`). A staged chunk finds no committed site, is assessed
  `Drain` (`:613`), joins `drain_only` (`:218`) and its obligation is deleted (`:333-339`), so the
  part stays a fragment short until it is published, or forever if the client never completes.
- **Success criterion:** on `main` after both children, four separate outcomes hold, each
  asserted by a named test, and `cargo xtask ci` is green.
  **(1) Found.** One scrub pass queues a repair for a corrupt or missing fragment of a committed
  `part:` record (663.1, NEW `crates/custodian/tests/staged_scrub.rs`).
  **(2) Kept while it cannot be repaired.** When reconstruction cannot rebuild a staged chunk —
  it is named only by an `sidx:` entry, its session is not `Open`, or the re-place loses to a
  session fence, a rewritten `part:` record, an expired write deadline or a drain — the obligation
  is still queued after the pass and nothing is adopted. It is never drained (663.1 legs D–F in
  `crates/custodian/tests/staged_protection.rs`; 663.2 legs B–E).
  **(3) Rebuilt when it can be.** For a committed part in an `Open` session, one reconstruction
  pass rebuilds the fragment on a new D server, repoints the `part:` record's
  `ChunkRef.placement`, and deletes the obligation in that same commit (663.2 leg A, NEW
  `crates/custodian/tests/staged_repair.rs`). This is the tracker's "the queued repair updates
  the part placement" and 0016's "resolved, not drained" (`0016:889`).
  **(4) The race, seeded.** A seeded DST (deterministic simulation test) case appended to
  `crates/dst/tests/custodian.rs` (663.2 leg F) sweeps the session fence across every point of
  the re-place (0016 X29, `:888`): in every interleaving no fragment ends unreferenced and
  unevidenced, and a losing re-place leaves its pre-mark and the obligation queued. It runs under
  `cargo xtask ci` → `run_dst` (`xtask/src/main.rs:1578`, `--cfg madsim`), as `AGENTS.md:189-190`
  requires of a new destructive or concurrent path.
  **Handover between the children:** 663.1's leg D asserts "kept" for a committed part's chunk as
  well as an `sidx:`-only one. For a committed part in an `Open` session that is the interim state
  on 663.1's own base; 663.2 turns it into outcome (3) and must retarget or remove that case of
  leg D. The `sidx:`-only case keeps. A committed part's chunk in an `Open` session found at full
  redundancy is not in outcome (2): after 663.2 its obligation drains as a duplicate finding, as a
  committed chunk's does (`reconstruction.rs:216-218`), so 663.1's leg E must seed a degraded
  chunk or it goes red after 663.2.
- **Falsifiability:** RED in-process on `origin/main`, no container: today scrub never reads a
  `part:` record and reconstruction deletes a staged chunk's obligation, so outcomes (1)–(3) fail
  by assertion. See each child for how its RED leg compiles. Outcome (4) lives in a modified file,
  so C4-verify runs it green-only and C4-ci (`cargo xtask ci` → `run_dst`) is its gate.
- **Invariant to restore:** a staged chunk's redundancy is kept the way a committed chunk's is:
  checked, and repaired when degraded. A staged chunk's obligation is removed only in the commit
  that makes its repair durable; "I could not read a record" never counts as "no record names
  it". The **staged re-place this slice adds** strands nothing: every fragment it writes is, at
  every instant, either named by a record or covered by an `orphan:` mark GC can act on. This
  binds the new staged path only. The committed repair path writes its destination before its
  CAS (`crates/custodian/src/reconstruction.rs:934`) and leaves it unmarked when the CAS loses
  (`:949-953`); that is the tracked leak #723 ("reconstruction/rebalance strand an unreclaimable
  fragment when the placement CAS loses"), not changed here. Sources: C-1 from the harness's
  principle catalogue (`wyrd-pdca/docs/principles.md` §5 and its §6 storage-lifecycle row — a
  harness file, not in the target tree), which rests on two target sources: 0016's refutation
  standard, "only availability, latency, capacity and operational costs are acceptable
  trade-offs" and no outcome (a)–(d) disposed of as an accepted cost (`0016:2802-2813`), and GC's
  invariant "never reclaim a referenced fragment" (`crates/custodian/src/gc.rs:47`). Also 0016's
  scrub and reconstruction rows (`0016:824-825`), failure rows `:888-889`, "no fragment is written
  after its evidence may have been reclaimed" (`:1356-1358`); proposal 0005's repair contract
  (`docs/design/proposals/accepted/0005-milestone-3-custodians.md:269-286`); ADR-0045.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Conflicts with:** 777
- **Ordering note:** **Intake-cap override (wyrd-pdca-P1):** granted by the human (Eduard Ralph)
  in this re-plan session, 2026-09-19, for this split of #663 only. `scripts/plan-cap --need 2`
  at the override: `planned 23/6 (cap) — room for 0, need 2: Plan intake closed` (that count
  does not include #663, which reads UNPLANNED after `iterate-plan`). `Depends on 803, 804` is
  dropped: both merged (`97fc2f9`). #777 edits `reconstruction.rs` and
  `crates/dst/tests/custodian.rs`. After the split is accepted, the briefs that name `663` are
  re-pointed at the children: #508 and #625 (`Depends on`), #800, #808, #809 and #810
  (`Conflicts with`).
- **Surfaces:** data
- **Difficulty:** high
- **Scope:** scrub over committed staged fragments; reconstruction's resolution of a staged
  chunk's obligation (keep it while it cannot be repaired, rebuild it when it can) under 0016's
  re-place rules; the time source and write window that re-place needs. The committed repair path
  (`reconstruction.rs:829-955`) keeps its behaviour. / out of scope: the committed path's
  lost-CAS leak (#723); `seg:`-resident repair (#777, #682); drain status, rebalance and restore
  (#808, #809, #810); the upload-side drain fence (#657); the GC sweep of fragment-less marks
  (#800); edits to 0016 or any ADR.
- **Repro instruction:** on `origin/main`, seed an `Open` session with one committed `part:`
  record, delete one of its fragments from its D server, `enqueue_repair` its chunk, and run one
  reconstruction pass: the obligation is gone and no fragment was rebuilt. Run one scrub pass on
  the same store before enqueueing: nothing is queued.
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** see the children: `crates/custodian/tests/staged_scrub.rs` (663.1) and
  `crates/custodian/tests/staged_repair.rs` (663.2), both NEW; plus legs appended to the existing
  `crates/custodian/tests/staged_protection.rs` (663.1 D–F) and the seeded DST case appended to
  the existing `crates/dst/tests/custodian.rs` (663.2 F, `#![cfg(madsim)]`, `:53`).
- **Citations expected:** Do must cite `path:line` on the target branch for every change.
- **Prior-art check (triage cycles):** by path (`crates/custodian/src/scrub.rs`,
  `reconstruction.rs`) across merged history and open PRs: neither file has ever read `part:` or
  `sidx:`, and no open PR touches them (2026-09-19: only dependency bumps are open). Rejected
  prior art: this issue's own v1–v3 (`results/issue_663/iteration-v1..v3/`) and #637 v1. v3's
  protocol held under adversarial review; it failed on size (248 KB, 20 files).
- **Disposition hint:** likely-fix

Plan-review response (2026-09-19, all four findings accepted and revised in place): (1) the
criterion is now four separate outcomes — found, kept while unrepairable, rebuilt-and-drained in
one commit when repairable, seeded race — and names the 663.1 → 663.2 handover of leg D;
(2) the invariant binds only the new staged re-place, and the committed path's lost-CAS leak is
named out of scope as #723; (3) the seeded DST case (663.2 leg F, `crates/dst/tests/custodian.rs`,
`run_dst`) is in the criterion and Test file — 0016's `:887` `W_repoint` row is covered
in-process by 663.2 leg C(v), not by a DST seed, per the v1 sign-off, and the human (Eduard
Ralph, 2026-09-19) confirmed that is enough; (4) C-1 is cited as the harness file it is, plus the
two target sources it rests on (`0016:2802-2813`, `gc.rs:47`). On the human's word the same
session: #814's Scope now says to rewrite #813's leg D (the handover above), and #813 and #814
carry the same corrected C-1 citation. Second review pass, same day: #814 now states the
full-redundancy rule for a staged chunk (drain as duplicate, `reconstruction.rs:216-218`) and
that #813's leg E must seed a degraded chunk — neither child said what 663.2 does with an
intact staged chunk, and #813's leg E does not say its chunk is degraded, so leg E could have
gone red after 663.2 for a reason unrelated to the read order. Every cited line, the sibling
re-pointing (#508, #625, #800, #808–#810) and the `--classify` result for both children were
re-checked on `97fc2f9`.
