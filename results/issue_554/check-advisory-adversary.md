# check-advisory-adversary.md — issue 554 / deployed-custodian-runs-gc (iteration 5)

Adversarial pass. I applied `patch.diff` to the base (`dc503cd`) in an isolated clone and
re-ran the evidence independently; probes and verdicts below.

- NEEDS-HUMAN — **Evidence provenance: the target worktree does NOT contain the patch under
  review.** `$PDCA_TARGET` holds a stale (iteration-4-shaped) state: the duplicate
  `--endpoints`/`--ids` refusal still exists ONLY inside `--reconcile-after-restore`
  (`crates/server/src/cli.rs:944` in the worktree — the patch's hoisted every-path refusal
  block is absent), and `crates/server/tests/custodian_gc.rs` is 828 lines vs the patch's
  933 — the two `deployed_run_loop_refuses_duplicate_{endpoints,ids}` tests are missing
  (`cargo test --test custodian_gc` in the worktree runs 6 tests, the patch has 8).
  `check-gates.json` (15:55) records the gating C4-ci as "all checks passed" — if that gate
  ran against this worktree, the green covers iteration-4 code, not this patch; if it ran on
  a fresh application of `patch.diff` elsewhere, the worktree is merely out of sync and the
  reviewer's citations may have been grounded on the wrong tree. I could not determine
  which from the artifacts available. Mitigation: I applied `patch.diff` cleanly to base
  `dc503cd` and independently ran `custodian_gc` (8/8 green), `custodian_day_one` (15/15
  green) and `closed_write_path` (1/1 green) — the patch itself is sound; the question is
  what the recorded gate evidence attests to.

- **Red→green evidence attacked; refutation FAILED (verified independently).** The
  full-revert red leg is a COMPILE ERROR (E0061: `run_reconstruction_until` takes 7
  arguments but 8 were supplied), not an assertion failure — I reproduced this by reverting
  `src/custodian.rs` + `src/cli.rs` and keeping the test file. The C4-verify row ("PASS —
  red without the fix, green with it") does not qualify this; the test module doc does
  (custodian_gc.rs:56-63) and the brief pre-authorizes it if flagged in build-notes (which I
  cannot read here — the human should confirm the flag landed). I then closed the
  pass-for-the-wrong-reason gap myself with three behavior-binding probes on the applied
  patch: (a) neutering the GC gate to `if false` → 5 of 8 tests fail by ASSERTION (all
  reclaim + both defer tests); (b) regressing the gate to iteration-2's
  `unreachable.is_empty()` (custodian.rs:582) → exactly
  `deployed_role_defers_gc_when_the_operator_fleet_is_startup_partial` fails by assertion;
  (c) stripping only the hoisted cli.rs refusal block → both `refuses_duplicate_*` tests
  fail (the base path panics on the empty fleet at cli.rs:899, as the test doc predicts).
  The tests exercise the production wiring, not a mirror.

- NEEDS-HUMAN — **The hoisted fleet-identity refusal is string-equality only; endpoint
  aliasing walks straight past it into the deleting GC sweep** (patch.diff cli.rs hunk, the
  `unique_endpoints` HashSet<&String> check). Concrete failing case:
  `wyrd custodian --endpoints http://localhost:50051,http://127.0.0.1:50051 --ids 1,2` —
  two textual identities for one physical box pass both refusals, and the exact hazard the
  patch's own comment names ("a LIVE fragment protected as (A, frag) is unreferenced seen as
  (B, frag), so GC would DELETE IT") is live again; likewise trailing-slash or DNS-alias
  variants. The same weakness pre-existed in the restore one-shot, but this patch is what
  arms the ALWAYS-ON run loop with deletion behind that check, and its comment claims the
  refusal guards "EVERY path". Fully closing it needs server-side identity attestation
  (canonicalizing URLs cannot resolve DNS aliasing) — an architectural call, hence no
  [impl]: the human should decide whether operator-attested endpoint uniqueness is an
  accepted trust assumption (documented), or a follow-up issue.

- NEEDS-HUMAN — **A startup-dropped peer is never re-dialed, so one degraded boot pauses GC
  for the entire process lifetime — even after the peer recovers.** `connect_fleet` drops a
  boot-unreachable peer once and `configured` never grows (custodian.rs:190-204); the gate
  `fleet.len() == operator_fleet_size` (custodian.rs:582) can then never be satisfied
  in-process, so the run-loop doc's recovery claim — "a missing server's garbage is reaped
  on a later whole-fleet pass" / "recovered in full on the next whole-fleet pass"
  (custodian.rs:562-567) — is unattainable after a startup drop: recovery requires an
  operator RESTART of the custodian. Concrete case: a custodian restarted mid-incident (the
  exact start-degraded scenario connect_fleet exists for, custodian.rs:168-175) reclaims
  nothing, silently (one `gc pass deferred` stderr line per interval), until someone
  restarts it again after the fleet heals. The startup-partial test masks this by invoking
  the loop twice (two fresh "processes", custodian_gc.rs:726+). Data-safety direction is
  conservative (defer, never false-collect) — this is a fitness/ops extension of the
  pre-declared pause-under-outage §6 trade-off, for the maintainer to accept or route to a
  re-dial follow-up.

- **Attempted to refute, could not:** (a) GC racing the repair loop's placement rewrites —
  the three passes run strictly sequentially in one task and GC re-derives the committed
  reference set inside its own pass (`gc.rs:100`, `gc.rs:124`), so a just-re-placed fragment
  is protected; (b) partial-sweep evidence loss — a mid-sweep store fault aborts before the
  single cleanup commit (`gc.rs:152-169`), so pending/orphan evidence survives (the residual
  is a lingering orphan record for an already-deleted fragment: conservative direction,
  gc-library behavior the brief scopes out); (c) whole-fleet gate arithmetic — fleet ⊆
  configured ⊆ operator endpoints holds on the cli path, and duplicates that could inflate
  `fleet.len()` are refused upstream; (d) boundary semantics — the inclusive `>=` reclaim
  instant is now pinned exactly (`deployed_role_reclaims_at_the_exact_grace_boundary`,
  fails if regressed to `>`); (e) pre-resolved iteration-2 items (lease-liveness
  document-and-ship pending #490, 60 s grace floor) — carried, not re-litigated.
