# Brief — issue 398 / unrepairable-loss-signal-already-shipped

> Triage outcome: the defect as filed is ALREADY FIXED on `main`. This brief routes the
> close-disposition fast path (builder + reviewer skipped; the human confirms the close
> at sign-off and closes the tracker issue).

- **Slug:** unrepairable-loss-signal-already-shipped
- **Defect:** As filed: a beyond-tolerance chunk (`Assessment::Unrepairable`) was dropped
  from the reconstruction plan set and never counted in the under-replicated durability
  metric, while the code comment claimed it would be "surfaced as under-replicated" — the
  one loss class an operator most needs to see read as healthy (`under_replicated == 0`).
- **Success criterion:** The human verifies on `getwyrd/wyrd@main` that beyond-tolerance
  loss is surfaced on a dedicated, higher-severity signal (the issue's own preferred
  "distinct signal" option) and closes #398 with a comment citing the fixing PR — no code
  change is shipped by this bundle.
- **Falsifiability:** N/A for a close disposition — no Do runs. The confirming evidence is
  re-checkable in place: `crates/custodian/src/reconstruction.rs:212`
  (`Assessment::Unrepairable => emit_data_loss(chunk)`), `reconstruction.rs:727-728`
  (`monotonic_counter.reconstruction_data_loss` + `tracing::error!` NEEDS-HUMAN audit
  line), and the shipped red→green regression
  `a_loss_beyond_tolerance_raises_data_loss_and_the_backlog_gauge_returns_to_zero` in
  `crates/server/tests/custodian_day_one.rs`.
- **Invariant to restore:** (already restored) Beyond-tolerance loss is never silent on the
  durability plane — it is raised on its own data-loss signal, deliberately excluded from
  the repairable-backlog gauge so that gauge's return-to-zero stays observable
  (`reconstruction.rs:148-171` documents the split; proposal 0005 §durability signals,
  ADR-0011).
- **Repo + branch target:** getwyrd/wyrd @ main
- **Difficulty:** low
- **Scope:** confirm-and-close only / out of scope: any code change; the M7.2 (#481)
  failover-drill work the maintainer folded this into (comment of 2026-07-08, tracked
  under #486) stays where it is.
- **Repro instruction:** `git -C ../wyrd show e65cf69 --stat` — "Add a deployable
  data-repair custodian with durability metrics (#450)", merged 2026-07-06, is the commit
  that replaced the empty `Unrepairable => {}` arm with `emit_data_loss` and corrected the
  stale comment the issue quotes.
- **External dependencies:** none
- **Test file:** none (close disposition — the regression already ships at
  `crates/server/tests/custodian_day_one.rs`)
- **Citations expected:** n/a (no patch). Sign-off should cite `reconstruction.rs:212`,
  `:727-728`, and commit `e65cf69` in the closing tracker comment.
- **Prior-art check (triage cycles):** searched by file path — `git -C ../wyrd log --
  crates/custodian/src/reconstruction.rs` shows `e65cf69` ("… durability metrics (#450)",
  merged 2026-07-06) landed the exact "distinct signal" fix the issue's Suggested-fix
  section prefers, with a deployed-role regression test. The issue's cited evidence
  (comment says "surface as under-replicated", arm body empty) no longer exists on `main`.
- **Disposition hint:** likely-close

## Ordering / batch note

Independent of the other bundles in this batch (no code change; no shared files).

## STOP discipline

Draft only until Check sign-off. No PR is expected from this bundle; the human closes
#398 on the tracker (noting the #486 / M7.2 fold) after confirming the evidence above.
