# Adversarial review — #1015 (625.1: multipart reaper, `Open` half)

**Bottom line:** I could not break the binding red→green. Two claims around it do fail, and I
proved both by running probes on a scratch copy: (1) no test notices if the deployed loop stops
passing the reaper; (2) a listing fault that keeps recurring leaves the uploads already read
unjudged forever. Toolchain was available; both probes ran.

## Red→green evidence — tried to refute it, could not

- `gate-logs/C4-verify.log`: all 10 tests fail on the base, every one at argument parsing
  (`flag --reap-abandoned-uploads needs a value`), and all 10 pass with the fix. The test runs the
  real binary as a subprocess against redb and checks what the store holds
  (`crates/server/tests/multipart_reaper.rs:155-165`). Leg A's fixture can only be reaped by the
  idle arm (created now−14d, progress now−10d, leases now−9d; `:495-510`). Leg B is not vacuous
  after the fix, because it also asserts a successful exit (`:611-616`). Leg H's red on the base
  comes only from its signal assertion (`:781-784`), as the brief predicted. I found no
  tautology and no mocked-away defect.
- The X60/X62 DST rows (seeded deterministic simulation tests) drive the production `UploadPart`
  against the real reaper (`crates/dst/tests/custodian.rs:7468-7641`). X60 keeps a live slot
  lease, so reading `psum:` first would see the new part in neither range and fence it, which
  means the row does discriminate. The slot-cycle gap is an accepted deferral (`:8535-8539`), so
  I did not re-raise it.

## Findings

- NEEDS-HUMAN [impl] — **No test covers the reaper's wiring into the deployed loop.**
  `crates/server/src/custodian.rs:680` passes `Some(&reaper)` to the reconstruction pass. In a
  scratch copy I changed it to `None` and ran every suite that drives `run_reconstruction_until`:
  `custodian_gc` (11), `custodian_day_one` (15) and `closed_write_path` (1). All 27 still pass.
  The only "deployed" coverage is a hand copy of the loop's shape that calls `reconcile_step`
  directly (`crates/custodian/tests/gc.rs:1484-1530`, `deployed_step`), and the DST rows do the
  same (`crates/dst/tests/custodian.rs:7387-7401`). So nothing tests the brief's Production-reach
  claim ("a healthy fleet therefore reaps every interval"). A merge that drops the argument would
  ship with every gate green. Fix: add one case to `crates/server/tests/custodian_gc.rs` that
  seeds an idle `Open` session, runs `drive_passes` (`:1341`), and asserts the session is fenced
  and torn down.

- NEEDS-HUMAN [impl] — **A listing fault that keeps recurring starves the uploads before it,
  forever.** On a page fault, `crates/custodian/src/reaper.rs:654-658` stops the walk, and
  `:692-695` saves the cursor at the last key read. Each later pass resumes there, hits the same
  fault, and never returns to the head. Under the shipped profile this needs only 17 sessions
  (`SESSION_PAGE` = 16, `:238`). Probe (scratch `reaper::tests`): 5 sessions kept at T, with a
  fault on the page after the 3rd. At T+8d all are idle past `W_open`, yet five more passes each
  report `held: 1, kept: 0, fenced_idle: 0`, the cursor stays at `mpu:0202…`, and sessions 0–2
  stay `Open`. The hold names the cursor key, not the starved sessions. The existing test at
  `crates/custodian/src/reaper/tests.rs:1003-1033` asserts this behaviour as intended ("the next
  pass resumes at the fault"). Fix: keep the hold, but start the next pass from the head after a
  listing fault. The one surviving C5 mutant (`reaper.rs:679`, `&&`→`||`) only changes the cursor
  on this same post-fault path, so a test for the fix would also kill it. This confirms T4's
  `reaper.rs:693` / `:697` items.

- NEEDS-HUMAN [impl] — **Fences committed earlier in a pass are dropped from the counters when
  the pass's last ledger read fails or refuses.** `reaper.rs:476` runs
  `check_admission_profile(..).await?` after `walk` has already committed fences. A single
  transient read fault there (no damage needed) makes `pass` return `Err`, so `reconcile`
  (`:448-456`) never calls `emit_pass`, and `reaper_sessions_fenced_*` never counts those fences.
  A refusal at that read (`:477-479`, `:487-493`) also skips `emit_pass`, and its message says the
  reaper "fences nothing" (`:1187-1189`). The per-fence info line (`:1239-1247`) still records
  each fence, so this affects counters and log accuracy, not data. Fix: emit the tallies on every
  exit after `walk`, and say in the refusal text that fences may already have committed.

- NEEDS-HUMAN [human] — **T4 triage: three of the 11 T4 blockers can only be reached through
  outside damage today; record them as rejected or deferred, with a reason.**
  (a) "The fence has no admission-ledger precondition" (`reaper.rs:1123`, raised 3×) requires
  `mpuctl` (the admission ledger) to disappear or change profile while a session is `Open`. Only
  two writers touch it: Create (`crates/core/src/multipart/create.rs:684-695`, which refuses a
  profile change at `:427`) and the terminal delete
  (`crates/custodian/src/retirement/sessions.rs:975-985`). Both require the prior bytes, and
  neither deletes the ledger or changes its profile. Pinning `mpuctl` in the fence would also make
  every fence lose to any concurrent Create or terminal delete, since both rewrite `count`.
  (b) The stale-clean-lap items (`reaper.rs:712`/`:714`, 3×) need more than 64 sessions listed.
  The only profile this custodian accepts admits at most 46 (`reaper.rs:205`; Create's cap at
  `create.rs:433-440`). They become reachable once #508 plumbs in a larger profile, so a deferral
  pointing there fits.
  (c) The `reconciliation.rs:187` convention item is real, but only at the API level: the server
  passes one store and one clock into both contexts (`custodian.rs:611-639`).
  Recording these decisions stops T4 from going red on them every round.

- NEEDS-HUMAN [human] — **Low priority: the pass reads every open session's full `psum:` range
  every 30 s.** `visit` reads the whole `psum:` range (up to 10,016 records) even when a live slot
  has already decided the session will be kept (`reaper.rs:794-807`). It does this for every
  `Open` session inside `W_session`, on every `--interval-secs` (default 30, `cli.rs:896`). At the
  shipped cap the worst case is 46 × 10,016 ≈ 460k record reads per interval, against windows
  measured in days. The doc at `reaper.rs:253-257` calls this "a few bounded ranges". This slice
  owns the cadence, so decide before #668 creates real sessions whether to run the pass less often
  or skip the `psum:` read when a slot is live (keeping the session is the safe direction).

I also tried these and could not break them: the drain and the reaper reading different clocks
(both shapes hand the same `LoopClock` to both, `custodian.rs:470-476`, `:634-639`); the one-shot
failing to finish on normal crash residue (that residue decodes as trusted, and leg A finishes
within 6 runs); and leg H passing on the base for the wrong reason (it fails there on the
missing signal).

<!-- pdca:leaf-complete -->
