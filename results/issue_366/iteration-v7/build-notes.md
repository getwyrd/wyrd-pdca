# Build notes — issue 366 / obs-floor-observability (keystone, iteration 7)

Withheld from the reviewer. Rationale + what I ruled out. Line anchors are on the target
branch `feat/m4-production-metadata-backend` **with this patch applied**.

## Scope of this iteration

Observability-floor **keystone** (proposal 0010 items 1–2): the durability telemetry seam
extracted to `wyrd-telemetry`, and `wyrd custodian` wired as a runnable, deployable role in
`server`, proving the day-one signal — kill a D-server → the under-replicated count rises then
returns to zero, read back through the role's own `gather_prometheus` surface.

I did **not** rebuild from scratch. I took **iteration-6's patch as the base** (it applies cleanly
to the current target branch) and corrected the **three iteration-6 sign-off rejection items**,
plus their explicit test asks. The accepted deferrals the iteration-6 sign-off listed (C5
probe-and-drop membership → #365; tikv single-active fencing → #365; live Prometheus/OTLP scrape →
#367) are **not re-litigated** — they remain recorded human calls in build-notes, honestly stated
in the code, and land with their owning issues.

## The three iteration-6 rejection items, each addressed

### 1. BLOCKING — `Unrepairable` (below-`k`, data-lost) was buried in the backlog gauge and floored it

**Reject:** iteration-6 counted `Assessment::Unrepairable` on `reconstruction_under_replicated`
while giving `Malformed` its own dedicated metric + NEEDS-HUMAN line. That is backwards: an
`Unrepairable` chunk (survivors < `k`) is the storage system failing its *primary* responsibility —
data meant to be durable is actually **lost** — which is *more* severe than a metadata/placement
`Malformed`, not less. Worse, `Unrepairable` is never drained by this loop, so counting it on the
backlog gauge floored that gauge at ≥ 1 forever: the binding day-one "rise then return to **zero**"
signal returns to 1, never 0, on any store carrying one permanent loss.

**Fix:**
- New `emit_data_loss(chunk)` (`crates/custodian/src/reconstruction.rs:618`): a **dedicated,
  higher-severity** signal — a `reconstruction_data_loss` monotonic counter emitted at
  **`tracing::error!`** (vs `Malformed`'s `tracing::warn!`) plus an `error`-level NEEDS-HUMAN audit
  line. This is ≥ parity with `emit_needs_human` (`reconstruction.rs:588` region), as the reviewer
  required, and higher severity as the event warrants.
- The `Unrepairable` arm now calls `emit_data_loss(chunk)` and **no longer touches**
  `under_replicated` (`crates/custodian/src/reconstruction.rs:179`). The backlog gauge counts only
  the `Repairable` set (`emit_under_replicated(under_replicated)`, `reconstruction.rs:216`), so it
  is a true repairable-backlog *level* that returns to zero once the auto-repairable losses are
  repaired. Tally comment (`reconstruction.rs:135`) and the `emit_under_replicated` docstring
  updated to state the split (Repairable on the gauge; Unrepairable → data-loss; Malformed → its
  own metric).
- The obligation stays queued for both non-repairable classes, so the loss/corruption stays visible
  to a human / out-of-band recovery — it is *surfaced more loudly*, not dropped.

**Why a counter, not a second gauge:** the reviewer asked for "≥ parity with Malformed's
`emit_needs_human`", which is a monotonic counter + audit line, and a data-loss event is an alert
that *should* keep firing every pass while the loss persists (a gauge that floors at ≥ 1 is exactly
the poison we are removing from the backlog gauge — putting it on a *second gauge* re-introduces a
"never returns to zero" level for no benefit). A monotonic counter reads back cleanly via
`gather_prometheus` and keeps the alert live. Cost of the rejected second-gauge alternative:
identical emit-site line count, but it revives a floored level the reviewer flagged as the defect —
strictly worse, not smaller.

**Red→green (mechanical, in an EXISTING file the verify harness can revert):**
`crates/custodian/tests/reconstruction.rs:1563`
`under_replicated_gauge_excludes_unrepairable_data_loss_so_it_returns_to_zero` drives a **populated**
store with a repairable loss (chunk A) *and* a permanent data loss (chunk B, two of three fragments
gone → below `k`). Post-fix: backlog gauge reads **1** (pass 1) then **0** (pass 2), while
`reconstruction_data_loss` is raised throughout and chunk B stays queued. Reverting only the
production change (arm back to `under_replicated += 1`, `emit_data_loss` removed) makes it read
**2** then **1** — captured live: `left: Some(2.0), right: Some(1.0)`. FAILED pre-fix, PASS post-fix.

### 2. CLI help-text contract — `--ids`/`--failure-domains` advertised optional but required with `--endpoints`

**Reject:** the usage line listed `[--ids …] [--failure-domains …]` in independent brackets
(reads as optional), but `require_aligned_topology` rejects a run that supplies `--endpoints`
without them — an operator following the printed usage hits a startup error.

**Fix (`crates/server/src/cli.rs:170`, `:178–181`):** the usage line now groups the three flags
`[--endpoints … --ids … --failure-domains …]` so they read as one all-or-nothing unit, and an
explicit note states: when `--endpoints` is given, `--ids`/`--failure-domains` are **REQUIRED**,
one per endpoint (matching each D-server's own `--id`/`--failure-domain`); omit all three to run
the leader-elected role with no reconstruction plane. I fixed the **help text** rather than relaxing
the guard: deriving topology from the endpoint order is exactly the iteration-4 fabrication reject,
and real derivation from the registration record is the out-of-scope etcd discovery seam (#365) —
so the operator supplies the real topology, and the help now says so.

### 3. cmd_custodian end-to-end coverage (T5c) — driven through the real binary entry, not just its halves

**Reject:** iterations 5–6 asked for a test **through `cmd_custodian`**; only the factored halves
(`connect_fleet`, `run_reconstruction_over_backend`) were covered, so the glue iterations 3/4 were
rejected on (wrong backend / fabricated topology) could regress behind green gates.

**Fix:** `cmd_custodian` is now `pub` (`crates/server/src/cli.rs:525`) so a test drives the real
entry: arg parse → `resolve_backend` → `connect_fleet` (with `require_aligned_topology` + the
concrete `GrpcDServerConnector` dial) → the empty-fleet early return. Two plain `#[test]`s (not
`#[tokio::test]` — `cmd_custodian` builds its own runtime, so a nested one would panic):
- `crates/server/tests/custodian_day_one.rs:1221`
  `cmd_custodian_rejects_misaligned_topology_through_the_real_entry_point` — two endpoints, one
  `--ids` → `cmd_custodian` returns `Err` naming `--ids`/`--endpoints` (the iteration-4 fabrication
  reject, proven wired into the *binary path*, not just the helper).
- `crates/server/tests/custodian_day_one.rs:1256`
  `cmd_custodian_starts_degraded_on_an_unreachable_fleet_through_the_real_entry_point` — aligned
  topology, every D-server unreachable at startup (an ephemeral-then-dropped port → connection
  refused, no live D-server, no hang), asserts `cmd_custodian` comes up **degraded** and exits `Ok`
  rather than propagating the first dial `Err` (iteration-5 BLOCKING #3 through the binary entry).

These exercise the concrete `GrpcDServerConnector` dial (fast connection-refused, bounded by
`--connect-timeout-secs 1`) — headless, no network peer, no hang. The full reconstruction loop over
a live gRPC fleet remains the DEFERRED off-Check live-exporter evidence (#367); driving it here
would require a real D-server and is not headless-honest.

## Commit-readiness

- `cargo fmt --all -- --check` → clean (the usage-string edit was reflowed by `cargo fmt`).
- `cargo clippy -p wyrd-custodian -p wyrd-server --all-targets` (workspace `-D warnings`) → exit 0.
- `cargo test -p wyrd-custodian -p wyrd-server` → all green (incl. the 9 day-one tests and both
  gauge-exclusion tests).
- `cargo test --workspace --exclude wyrd-dst --no-run` and `cargo test -p wyrd-dst --no-run` →
  compile clean (the historically-failing gate `cargo test --workspace --exclude wyrd-dst`, exit
  101 in iters 3–5, builds and the affected suites pass).
- Patch round-trips: `git apply --check` against a freshly-reset target-branch tree passes, so it is
  committable to the target repo.

## Verification hygiene (iteration-5 note, carried)

The strongest mechanical red→green (item 1) is in an **existing** file
(`crates/custodian/tests/reconstruction.rs`), so `run-verify` can revert the production hunk and
observe the flip without the new-file `pathspec` artifact that dogged the `C4-verify` gate on the
new `wyrd-telemetry` crate. The day-one and cmd_custodian tests live in the new
`custodian_day_one.rs`; their behavioural asserts are the meaningful flips, verified here by hand
(the `Some(2.0)`/`Some(1.0)` pre-fix capture above). If `C4-verify` still reports a new-file
pathspec red, that is the harness's new-crate-rename handling (flagged since iteration-5), not a
defect in this patch — the code-derived reds are explicit.

## Carried-forward open items (human calls, recorded — not silently resolved)

- **Cross-process leader election / tikv single-active** — the `tikv` arm is honestly logged as
  NOT fenced (`cli.rs` startup log + `cmd_custodian` docstring); real fencing = etcd `Coordination`
  (#365, out of scope). Human may harden to a must-fix.
- **Reachability probe vs registration/lease membership** (`live_reconstruction_view`) — a stand-in
  for lease-driven fleet membership; accept for #367 or require the classification seam to treat
  unreachable-during-reconstruction as missing (#365).
- **Backlog gauge is a level over the repair QUEUE**, not the full chunk population — a
  lost-but-not-yet-enqueued chunk contributes 0 (unchanged; documented).
- **Milestone decomposition + typed-errors × #255 (item 6) sequencing + shared-`telemetry`
  extraction confirmation** — the pre-agreed §6 sign-off items.
- **Live-exporter evidence is off-Check** (DEFERRED, brief §Success): a Prometheus-scrape / OTLP run
  on a Tier-2 node against the day-one checklist. Pre-agreed sign-off item, not a surprise.
