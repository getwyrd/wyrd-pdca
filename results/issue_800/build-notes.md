# Build notes — issue 800 / gc-fragment-less-mark-sweep

Base: `main` @ `97fc2f9` (the merge of #812, i.e. `origin/main` + #661 + #803 + #804), which is
the worktree the driver gave me. `#661`, `#803` and `#804` are all merged there, so `main` itself
is this bundle's base, as the brief's Falsifiability section anticipates. Line citations marked
"base" are on `97fc2f9`; the rest are on the patched tree.

## What changed and why

**The sweep** — `crates/custodian/src/gc.rs`.

- `Sweep::run` (base `gc.rs:451-458`, now `:583-597`) records, for every listed fragment that has a
  mark in the window, its position in a new `listed` set (`:590`), before judging the fragment, and
  after the walk and `record_intents` calls the new `sweep_fragment_less_marks` (`:596`). The set is
  keyed by `(DServerId, FragmentId)` — the position — never by a raw ledger key, and it is a union
  over every fleet entry, so a fleet naming one server twice cannot un-list a position (B(v)). It is
  bounded by the window (only positions with a mark in the window are recorded), not by the fleet's
  fragment count.
- `sweep_fragment_less_marks` (`:815-861`) walks the window's marks in position order
  (`OrphanWindow::marks_in_position_order`, `:1613-1622`) and keeps a mark when: its position is
  listed (the walk's); its D server is not in this pass's fleet (`server-not-in-fleet`); the
  reference set or the staged class protects the position (the reclaim's own gate, base
  `gc.rs:475-482`, same `protection().or_else()` call); its value is none of the three shapes
  (`ReadMark::Unreadable`, already named by the window); or it is younger than `D`
  (`within-late-write-deadline`). Everything else is swept.
- `commit_sweep` (`:882-915`) deletes in batches of at most `CLEANUP_BATCH`, each delete conditioned
  on the exact bytes read (`Sweepable::delete`, `:573-577`, re-encoding the decoded mark as
  `Intent::record` does). On `Conflict` it retries each delete alone (#804's shape,
  base `gc.rs:585-615`); a delete that loses again is judged on a **fresh `get`** of its key:
  absent → `mark-gone` (nothing claimed); present with other bytes → `mark-changed`; present with
  the same bytes → `mark-unchanged`; the last two set `lost_sweep` so the pass answers `Partial`.
  A sweep is audited and counted (`claim_sweep`, `:918-921`, `emit_mark_swept`, `:1845-1859`) only
  after its own commit returned `Committed`; an `Err` propagates with nothing of that commit
  claimed. That is the restore rule the brief cites (`restore.rs:340-346` on `605b33a`; on the base
  it sits at `restore.rs:385-390`, "Evidence is claimed only once it is durable").
- `ReadMark::Reclaiming` now carries its `OrphanMark` (base `gc.rs:1202`, now `:1486`;
  `classify_ledger_entry` base `:1374`, now `:1671`). The walk's judgement of a `reclaiming` mark
  is unchanged (`Some(ReadMark::Reclaiming(_))`, base `:486`); the sweep needs the stamp to age a
  fragment-less `reclaiming` mark and the value to condition its delete. This is the one reshape of
  #804's types, and it changes no judgement.
- The pass's answer (base `gc.rs:387-396`, now `:486-492`): a landed sweep makes it `Changed`; a
  lost sweep to a mark still present makes a pass that changed nothing `Partial`.
- Docs in the module header (`:46-59`), on `CLEANUP_BATCH` (`:155-158`, the sweep's commit size),
  `reconcile` (the outcomes), `destroy` (the `deferred: #800` marker at base `:621-625` is resolved
  and removed), `Cleanup::finish_after_fault` (`:1806-1810`) and `emit_cleanup_lost`
  (`:1966-1971`). New emitters `emit_mark_swept` (counter `gc_orphan_marks_swept`, audit action
  `sweep-mark`) and `emit_mark_skip` (counter `gc_orphan_marks_skipped`, audit action `skip-mark`,
  `:2028-2045`), mirroring `emit_reclaim` / `emit_skip`.

**`D` and its parts** — `gc.rs:161-233`, each a `pub const` with its derivation in its doc comment:

| Name | Value | Derivation (short) |
|---|---|---|
| `W_WRITE_MILLIS` (`:176`) | 30 000 | the D server's own per-request ceiling, `DEFAULT_REQUEST_TIMEOUT`, `crates/server/src/dserver.rs:73` |
| `W_REPOINT_MILLIS` (`:192`) | 10 000 | two 5 s metadata transaction envelopes (`E_TX_MILLIS`, `crates/core/src/multipart.rs:4509`): the pre-mark's commit, then one more read/commit before authorizing |
| `DELTA_CLOCK_MILLIS` (`:205`) | 1 000 | a stated skew budget (ADR-0024 is Proposed and sets no value); ≫ the 1 ms tick the strict margin needs |
| `LATE_WRITE_DEADLINE_MILLIS` = `D` (`:233`) | 41 000 | `W_repoint + W_write + δ_clock` (`0016:1381-1391`) |

`D`'s doc comment names the writers' obligation (`0016:1339-1349`, `:1551-1576`) and the slices
that inherit it (#814 — the staged re-place, split from #663 —, #723, multipart teardown), and
says why the sweep is sound against today's tree (the brief's ordering note).

**Leg C** — `D` strictly inside the deployed grace — is a **compile-time assertion beside the
deployed constant**: `crates/server/src/custodian.rs:116-123`,
`const _: () = assert!(wyrd_custodian::gc::LATE_WRITE_DEADLINE_MILLIS < GC_GRACE_WINDOW_MILLIS);`
It names `GC_GRACE_WINDOW_MILLIS` itself (base `custodian.rs:114`, `= crate::cli::LEASE_TTL_MILLIS`
= 60 000, `cli.rs:78`), not a copy of its value, so retuning either side past the other fails the
build. It adds no `*/tests/*.rs` file, so C4-verify's invocation is untouched. `gc.rs` already uses
this pattern (`const _: () = assert!(STAGED_PAGE ...)`, base `gc.rs:173`).

**Docs currency** — `docs/design/architecture/06-runtime-view.md` §6.7 step 2: a new paragraph
(`:80`) describing the sweep, and the reclaim-intent paragraph's sentence that said a `reclaiming`
record over deleted bytes "waits for the sweep" now says the sweep removes it (`:78`). No other
living doc lists GC's metrics or audit actions (checked: no `.md` names `gc_fragments_reclaimed`).

**`Reconciled::Partial` doc** — `crates/custodian/src/reconciliation.rs:46-52` gains the sweep's
reason for `Partial`.

**An existing fixture had to change** — `crates/custodian/tests/gc_ledger_walk.rs`.
`seed_filler` (base `:947-957`) and leg D(ii)'s own fillers (base `:1135-1141`) seed fragment-less
marks stamped at 0 and document them as marks "which no pass reclaims and none consumes, so the
ledger stays several windows long". That is precisely the behaviour this slice removes, so
`d1_a_mark_on_the_resume_cursor_outranks_an_expired_lease` (its "a window ends on the mark" fixture
check) and `d2_an_expired_leases_entry_outlives_every_fragment_it_accounts_for` (its
`assert_bounded` "`⌈n / W⌉` delete-carrying commits" check, now joined by the sweep's own commits)
failed. I stamped the fillers at `NOW` instead (`:952-958`, `:1139-1148`): every pass those legs
run is inside the fillers' 41 s late-write deadline, so the sweep keeps them and the legs test
exactly what they tested before. No assertion was changed. The verify gate reverts modified test
files on its red leg (`engine/scripts/run-verify.sh`, the RED loop), so this does not touch
C4-verify.

**Cost of the per-mark skip audit.** `gc_ledger_walk` now takes ~9.8 s in debug against 1.4 s on
the base. Measured: with `emit_mark_skip` for kept marks switched off it takes 1.75 s, so the time
is the audit of ~65 000 young filler marks per pass across ~20 passes. The walk already audits
every listed fragment it keeps (`emit_skip`, `within-grace`) at the same per-entry cost; I kept the
sweep consistent with that rather than make its declines silent. A reviewer may prefer not to audit
`within-late-write-deadline` (the common, transient case); that is a one-line change and would
cost the B(i)/B(vii)/E tests their positive observable for the deadline rule.

## Test: `crates/custodian/tests/gc_mark_sweep.rs` (NEW; a copy is in this bundle)

16 tests over in-memory doubles built as `gc_ledger_walk.rs` builds them (an ordered-map metadata
double with a lowered cap of 4 and its own `scan_page`; D-server doubles whose `list_fragments()`
the test controls), every pass through the production `reconcile_step` with a fresh `GcContext`
at the deployed grace (60 000, a literal). `D` is the literal `41_000`. The file names only symbols
present on the base (compiled and ran there — see RED below). The metadata double injects a
"racer" that lands on a key the moment the pass's conditional delete of it arrives (rewrite;
rewrite-then-delete; hold = answer `Conflict` without changing anything), and a commit fault; it
logs commits and `get`s in order. The audit trail is captured as JSON off a scoped subscriber.
Each leg that asserts survival also seeds a control that must be swept, so no leg can pass on a
pass that swept nothing.

Legs → tests: A `a_…`; B(i) `b1_…`; B(ii) `b2_…`; B(iii) `b3_…` (also checks that past grace the
walk reclaims it, as a reclaim); B(iv) `b4_…` (passes at `D − 1`, then a fragment lands, then passes
at `D` and at `GRACE − 1`); B(v) `b5_…` (both orders of holder/empty store under one id); B(vi)
`b6_an_incomplete_…` (answers `Blocked`, then sweeps once the record is repaired) and
`b6_a_referenced_…`; B(vii) `b7_…` (four honoured shapes old and young, five non-shapes); D(i)
`d1_a_mark_restamped_…` and `d1_a_pass_whose_only_sweep_lost_…` (`Partial`); D(ii)
`d2_a_fault_across_a_batch_boundary_…` (W + 5 marks, the second batch fails) and
`d2_a_fault_inside_the_retry_…`; D(iii) `d3_a_mark_rewritten_then_deleted_…` (asserts a `get` of the
key after the lost commit, reason `mark-gone`, no sweep/no `mark-changed`, answer `Satisfied`) and
`d3_a_delete_that_loses_to_an_unchanged_mark_…`; E `e_…` (both cases; also asserts no commit ever
names the alias). D(ii) asserts "claimed == landed" (keys actually gone), not an order, so it does
not depend on the batching order.

### RED → GREEN (run by hand, time-bounded, on this worktree)

- GREEN: `cargo test -p wyrd-custodian --test gc_mark_sweep` — **16 passed**.
- RED (base `gc.rs` restored, test kept): **16 of 16 ran and failed, all by assertion** (the binary
  compiled against the base and every test reached a failing `assert`; none panicked outside an
  assertion). Leg A fails on its first assertion — the mark survives (`gc_mark_sweep.rs:664-669`).
  The B/E legs that are pure survival guards (B(ii)–B(v), E) fail on their *control*
  (`assert_control_swept`, `:620`: no sweep exists on the base, so the control survives); B(i), B(vi),
  B(vii) fail on the sweep's own audit reasons or on the swept marks; D(i)/D(iii) fail on their
  fixture check that the pass ever tried to delete the mark; D(ii) fails because the base pass
  never meets the fault and returns `Ok`. So on the base, only leg A's red is the defect itself;
  the others are red because each of them is a guard that is only meaningful over a pass that does
  sweep — which the brief anticipates ("may already be green on the base").

### Refute-your-own-test (all three: yes)

- **(a) Genuine red?** Yes. Reverted `crates/custodian/src/gc.rs` to the base with the test kept:
  16/16 red by assertion (above). Also mutation-tested the patched code, each mutant restored
  after: a **blind delete** (round 1's defect) → A, D(i)×2, D(ii)-retry, D(iii)×2 red, and both DST
  tests red at the "refreshed mark survives" assertion; an **age-only sweep** (ignores this pass's
  listing — the brief's SELF-TEST) → B(iii), B(iv), B(v) red; **no fresh read after a lost
  precondition** (the `Conflict` taken as proof a mark exists — round 2's defect) → D(iii)×2 red;
  **claiming a sweep before its commit lands** → D(i)×2, D(ii)×2, D(iii)×2 red.
- **(b) Production path?** Yes. Every leg drives the production `wyrd_custodian::reconcile_step`
  → `gc::reconcile` → `Sweep::run` → `sweep_fragment_less_marks` / `commit_sweep`; the doubles are
  only the store seams (`MetadataStore`, `ChunkStore`), as in every other custodian GC test. The DST
  leg drives the same production pass over `SimTikvMetadataStore`.
- **(c) Fixture includes the fault?** Yes. The racing writer really rewrites (or rewrites and
  deletes, or holds) the mark between the pass's read and its delete — each D test first asserts
  the racer fired (the pass did try to delete it), so none passes on a pass that never met the
  race; D(ii) asserts the fault struck partway ("some landed, some did not"); B(iv) really lands a
  fragment after pass 1's listing; B(v) really lists the position from one store and not the other
  under the same id; B(vi) really seeds an unreadable committed record and a real committed map.

## Leg F — seeded DST (`crates/dst/tests/custodian.rs`, property 15, `:3465-3762`)

Appended to the existing file (still `#![cfg(madsim)]`; no new DST file). `sweep_under_a_concurrent_restamp`
(`:3614`) pages a ledger of fragment-less marks over `SimTikvMetadataStore::with_scan_cap(page_cap)`
while a `madsim::task::spawn`ed writer re-stamps one target (a blind put, retried on the model's
lock-race `Err`) at a seed-chosen delay; three passes, each with a fresh `GcContext`, at a logical
now past every target's deadline and inside the re-stamp's. Property: the re-stamped key holds
exactly the refreshed value at the end, and every other target is swept. The seed picks page cap
(2–5), target count, which target, the re-stamp's shape (legacy / structured) and the delay
(0–36 ms). Coverage leg `prop_gc_fragment_less_sweep_reaches_between_the_read_and_the_delete`
(`:3741`) walks every delay for both shapes and asserts some landing fell between a pass's read of
the target and that pass's delete of it, and some did not (the shape of
`prop_restore_two_readings_cover_the_divergence_window`). Both are registered as
`dst_campaign_test!`s, and the campaign leg is appended to `committed_regression_seeds_stay_green`.

**Seed count:** `cargo xtask ci` → `run_dst` runs the campaign with `MADSIM_TEST_NUM = 50`
(`xtask/src/main.rs:1573`, `DST_SEEDS = "50"`), so the campaign leg runs **50 seeds** plus the **8
committed regression seeds**; the coverage leg runs 2 shapes × 37 delays = 74 runs per seed. Run by
hand with `MADSIM_TEST_NUM=50`: both green in 0.7 s; with the blind-delete mutant both red. Of the
coverage leg's 74 landings, **10 fall between the read and the delete** (delays 4–8 ms, both
shapes) and 64 outside it (measured with a temporary print, since removed).

## Gate

`./engine/xtask.sh ci` (the project's wrapper for `cargo xtask ci`, run with
`PDCA_WORKTREE` = this worktree, bounded by `timeout 7000`) → **`xtask ci: all checks passed`,
exit 0**, on the final tree (the one `patch.diff` was cut from). Every step ran, none warn-skipped:
`typos`, `lint_docs.py`, `render_site.py --check` (link audit OK), gitlink and unsafe guards,
`cargo fmt --check`, `cargo clippy --workspace --exclude wyrd-dst --all-targets`, build, `cargo
test --workspace --exclude wyrd-dst` (including `gc_mark_sweep`: 16 passed, and `gc_ledger_walk`:
12 passed), `cargo-machete`, the three `cargo deny` runs, statics, deploy-guard, and `run_dst`
(DST clippy, then `cargo test -p wyrd-dst` at 50 seeds: both new properties and
`committed_regression_seeds_stay_green` passed). `patch.diff` applies cleanly to `97fc2f9`
(= `origin/main`), checked with `git apply --cached --check` against a throwaway index, and
`run-verify.sh --classify` on it reports exactly one `ADDED_TEST`, `crates/custodian/tests/gc_mark_sweep.rs`.

**How the red leg was run.** By hand in this worktree: `crates/custodian/src/gc.rs` restored to the
base, the new test kept, then the command C4-verify itself runs,
`cargo test -p wyrd-custodian --test gc_mark_sweep`, under `timeout 900`; then my `gc.rs` put back
(its diff checked unchanged). I did not run `engine/scripts/run-verify.sh` itself: it creates a git
worktree and a `pdca-verify*` branch in the primary checkout, outside the roots I may write to.
Check's C4-verify runs it.

## Alternatives I ruled out, with their cost

- **A fresh targeted existence check at delete time** (0016's other allowed observation,
  `0016:1375-1377`) instead of relying on this pass's listing: one `get_fragment` per candidate —
  and `get_fragment` returns the fragment's **bytes**, so up to `ORPHAN_WINDOW` = 65 536 fragment
  reads per pass. The listing the pass already takes is an equally valid observation once
  `now ≥ orphaned_at + D`, because the pass's clock is read before any listing
  (`crates/server/src/custodian.rs:600-611`, `clock()` evaluated as the argument; the clock is
  `wall_clock_millis`, `cli.rs:1589-1593`). Adds cost, no safety.
- **Folding the sweep's deletes into `Cleanup`'s batch** (base `gc.rs:1476-1528`): `Cleanup` is
  blind by design (it can never `Conflict`). Adding a precondition would make every cleanup commit
  conditional, so one re-stamped fragment-less mark would cost the blind key deletes of fragments
  already destroyed; keeping it blind would be round 1's defect. A separate conditional batch is
  ~35 lines (`commit_sweep`).
- **Keeping raw bytes in the window** instead of changing `ReadMark::Reclaiming` to carry its mark:
  the codec guarantees re-encoding reproduces the stored bytes exactly
  (`crates/core/src/metadata.rs:301-305`), which is what #804's own precondition relies on; storing
  bytes too would duplicate every mark in the window's memory.
- **Sorting by ledger key** (building a key per mark): my first version did; I switched to sorting by
  position, which is equally deterministic and allocates a key only for a mark actually swept.
- **Putting `D` and its parts in `wyrd-core`** so a gateway-side writer can name them: the brief
  scopes the constants to GC (`gc.rs`) and the server can name them through `wyrd_custodian::gc`.
  If #723 or multipart teardown lands outside the custodian crate, moving the three parts to `core`
  (and re-exporting) is a mechanical follow-up; I did not pre-empt it.
- **A draining-retirement check in the sweep** (the reclaim path's `retirement_draining`, base
  `gc.rs:569-575`): that rule protects *bytes* while a drain refreshes stale marks
  (`0016:1226-1247`); a fragment-less position has no bytes to protect, and a drain that finds the
  mark gone simply writes a fresh one. Not in the brief's scope list either.

## For the human at sign-off

- The three part values (`W_write` 30 s, `W_repoint` 10 s, `δ_clock` 1 s) are named and derived as
  the brief asks, but `δ_clock` in particular is a stated budget, not a measurement (ADR-0024 has
  no value yet). They are the writers' obligations for #814/#723/teardown; worth a deliberate look.
- A mark on a D server that is not in the pass's fleet is never swept (B(ii), as the brief
  requires) and is named every pass as `server-not-in-fleet`. The deployed loop runs GC only over
  the whole operator fleet, so such a mark belongs to a server no longer configured; nothing exits
  that state today except a human. Worth an issue if decommissioning is expected to leave marks.
- No external dependency beyond the brief's list was needed (`typos` and the docs renderer are
  installed here and ran inside the gate).
