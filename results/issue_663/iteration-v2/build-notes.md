# Build notes — #663 staged scrub and repair (iteration 2, rebuild)

Base: `origin/main` @ `97fc2f9` (#803 and #804 merged — the brief's "base + #661 + #662").
All `path:line` below are on the patched tree unless marked "base".

## What the patch does, in one paragraph

Scrub now verifies the fragments a multipart upload's **committed parts** name, against the
scheme each part record carries, and queues the ordinary repair obligation on rot or loss.
Reconstruction reads the staged records **before** the committed namespace and resolves an
obligation for a staged chunk instead of dropping it: a committed part's chunk is rebuilt and
re-placed under 0016's pre-mark rule (choose a usable destination → pre-mark → write with a
deadline → one adoption CAS pinned to the session being `Open@E`, the part record's exact bytes,
the pre-mark's exact bytes and the destination's drain key); a still-streaming or fenced chunk
keeps its obligation; a record the pass cannot read withholds drains and certification. Every
losing branch leaves any written fragment under its pre-mark and the obligation queued. The
committed repair path (`repair_chunk`) keeps its behaviour.

The design is iteration 1's (its structure and shape passed review, T1/T2). This rebuild applies
the four carry-forward fixes (§1), kills the C5 survivors and adds the tests the adversary asked
for (§2).

## 1. The carry-forward fixes

### Fix 1 — destination choice and the drain fence now test one fact

v1 excluded draining servers with `draining_servers` (base `crates/custodian/src/desired_state.rs:152-167`),
which silently skips a `desired:dserver:<S>` whose value is not `draining`/`decommissioning`,
while the adoption required the key to be absent whatever its value. So
`desired:dserver:3 = "maintenance"` was chosen, written and refused on every pass.

Now the choice reads the key itself: `destination()`
(`crates/custodian/src/reconstruction/staged.rs:411`) passes a server over if `desired_key(dserver)`
is present at all (`staged.rs:423`). The pre-mark batch pins the same key (`staged.rs:502`) and so
does the adoption (`staged.rs:557`), so choice, pre-mark and adoption test one fact. The destination
is also now chosen at **assessment** time (`choose_destinations`, `staged.rs:372`): a chunk no usable
destination can take is `Assessment::Blocked` (`staged.rs:347`) — off the repairable-backlog gauge,
like a committed chunk with no free domain — instead of being counted as repairable and aborting
every pass. v1's per-pass `draining_servers` scan and its `owes_repairs` gate are gone (that gate
was a C5 survivor).

Regression test: `e_any_desired_state_record_fences_the_destination` seeds the adversary's case
exactly and asserts one pass repairs onto server 4, nothing is pre-marked or written on server 3,
and a second pass has nothing left to do. **Refuted**: with v1's check put back
(`draining_servers(ctx.meta).await?.contains_key(&dserver)`), this test — and only this one —
fails (37 passed, 1 failed).

Two more drain cases are pinned: `e_a_drain_recorded_after_the_choice_loses_the_pre_mark` (the drain
lands right after the choice read the destination's mark → the pre-mark loses, nothing is written,
the next pass picks another server) and `e_a_drain_recorded_after_the_write_loses_the_adoption`
(brief leg E: the adoption loses, the pre-mark stands).

### Fix 2 — the pre-mark stamp and the write deadline are read when the pre-mark is written

v1 stamped the pre-mark and fixed the deadline from `now_millis`, read once when the pass began,
so a pass that spent more than `W_write` reading and assessing sent every staged write already
expired.

The constraint: the brief forbids changing `reconcile_step`'s signature or adding a context field
(and a new field would break the red leg — see fix 4). So I added `StepClock`
(`crates/custodian/src/reconciliation.rs:126`): `reconcile_step` starts it at entry with the
caller's `now_millis` (`reconciliation.rs:183`) and hands it to reconstruction
(`reconciliation.rs:204`). `StepClock::now_millis()` (`reconciliation.rs:144`) is the caller's
reading plus the monotonic time the step has spent (`std::time::Instant`). The re-place reads it as
it builds the pre-mark (`staged.rs:489`) and fixes the deadline from that stamp (`staged.rs:513`);
the vacated source's mark is stamped as the adoption is built (`staged.rs:546`). The committed path
still stamps with the step's start reading (`clock.started_at_millis`, `reconstruction.rs:200`),
exactly as before.

Why this is one clock, not two (rubric MUST 1): `Instant` supplies only a *duration*; the epoch is
the caller's. In the deployed loop the caller's clock is the wall clock (`wall_clock_millis`,
`crates/server/src/cli.rs`), so the stamp is wall time at the pre-mark; GC ages the mark and the D
server judges the deadline on that same clock. In DST, madsim virtualises `Instant` (it overrides
`clock_gettime`; madsim-0.2.34 `src/sim/time/system_time.rs:40-88`), so runs stay
seed-deterministic. In in-process tests the logical clock advances by the few real milliseconds a
pass takes — harmless, and the tests allow for it (`PASS_SLACK`). `Instant::now` is not on
`clippy.toml`'s disallowed list (only `SystemTime::now` is). Stamp accuracy matters only to
liveness: grace and deadline are both measured from the one stamp, so an early or late stamp moves
both together; the `StepClock` doc says so.

Regression tests: in-process `d_the_pre_mark_is_stamped_when_it_is_written_not_when_the_pass_began`
stalls each fragment read of the assessment by 60 ms (real time) and asserts the stamp is at least
`NOW + 180`; DST `staged_replace_reaches_every_window` runs a pass whose three fragment reads take
10 s each (simulated), longer than the whole 20 s write window, and asserts the repair is adopted in
one pass. **Refuted**: with the pre-mark stamped at `clock.started_at_millis`, both fail
(`the pre-mark is stamped 1000000, less than the 180 ms the pass spent reading ...` and
`a pass slower than a write window authorized its write already expired`). The DST campaign leg
stays green in that experiment, correctly: pass-start stamping is a liveness defect, not a safety
one.

Ruled out:
- `SystemTime::now()` at the pre-mark: a second epoch inside the lifecycle in every in-process test
  (logical `now` ≈ 1e6 vs wall ≈ 1.7e12) — the #557/#565 class. Rejected.
- A clock field on `ReconstructionContext`: forbidden by the brief, and breaks the red leg's compile.
- Anchoring the clock inside `reconstruction::reconcile` instead of `reconcile_step`: in a step that
  also runs GC and scrub first it would miss their time and stamp early. Anchoring at the step's
  entry costs one argument on a `pub(crate)` function.

### Fix 3 — the DST D-server double enforces the deadline

`ReplaceDServer` (`crates/dst/tests/custodian.rs`, property 15) no longer hands the deadline to
`MemDServer` (which ignores it). It judges the deadline on its own simulated clock where the real D
server does (the `ChunkStore::put_fragment` contract, base `crates/traits/src/lib.rs:1075-1151`):
before it publishes (`WriteDeadlineExpired::if_elapsed` — refused, nothing lands) and after the
publication returns (`if_publication_unverified` — `Unknown`, the bytes stay). The seed now also
draws a late write (queued past the deadline, or publishing past it) and slow reads, so refusals
actually happen in the campaign. The coverage leg asserts both refusal effects are reached and never
adopted. **Refuted**: with the double's two checks disabled, the coverage leg fails
(`a write queued past its deadline was not refused`).

The in-process double (`Disk` in `staged_repair.rs`) enforces both halves too;
`d_iv_a_write_whose_landing_is_unknown_is_never_adopted` covers the `Unknown` effect (bytes landed,
not adopted, covered by the pre-mark, reclaimed by GC after grace). Production treats both effects
as an abort, and names which one on the audit seam (`staged.rs:526-536`).

### Fix 4 — `W_write` promoted to a constant in `crates/server/src/cli.rs`

`STAGED_REPAIR_WRITE_WINDOW_MILLIS` is now `crates/server/src/cli.rs:126`, beside
`LEASE_TTL_MILLIS`, with a doc comment (`cli.rs:101-125`) stating both inequalities —
`G_orphan > W_repoint + W_write + δ_clock` (the re-place's `W_repoint` is 0) and #800's
`D ≥ W_repoint + W_write + δ_clock` for the fragment-less-mark sweep — and what breaks if `D` is
sized shorter. Two compile-time checks make a contradiction visible:
- `crates/server/src/custodian.rs:121-124`: `GC_GRACE_WINDOW_MILLIS > STAGED_REPAIR_WRITE_WINDOW_MILLIS`;
- `crates/server/src/cli.rs:130-133`: the cli value equals the library's.

**Decision for the human: "CLI-configurable" is only partly achievable inside this brief.** The
re-place runs in the custodian library, which may not depend on the server crate (ADR-0010), and
the only ways to hand it a runtime value are a new `ReconstructionContext` field or a
`reconcile_step` parameter. The brief forbids both, and a new field would also make the new test
fail to compile on the red leg (it must build `ReconstructionContext` from base-visible fields
only, so `run-verify.sh` would report UNVERIFIABLE). Cost of the field, counted on base: 43
`ReconstructionContext { .. }` literals in 9 files (`crates/chunkstore-grpc/tests` 10,
`crates/server` 8, `crates/dst/tests` 7, `crates/custodian/tests` 18). So the library restates the
value (`crates/custodian/src/reconstruction.rs:95`, exported at `crates/custodian/src/lib.rs:43`,
following the `SIZING_SCHEME` precedent, base `crates/core/src/multipart.rs:4366-4372`) and the
server asserts the two are equal: changing the cli.rs value alone fails the build with a message
naming the library constant. A true runtime knob needs a follow-up that is allowed to add the
context field. Also: the brief lists `crates/server/src/cli.rs` as out of scope; I edited it only
because carry-forward item 4 names it.

## 2. Other changes relative to v1

- **The C5 survivors are gone.** The two in `Gathered::settle` (`+` → `-`/`*` on
  `survivors + transient_missing >= k`, now `reconstruction.rs:892`) are killed by a new test,
  `a_staged_chunk_short_only_of_an_unreachable_server_is_not_data_loss` (one server unreachable, one
  fragment lost → `Unreachable`, no `data-loss` event; either mutant emits one). `owes_repairs` no
  longer exists (fix 1). `read_part` holds its part lazily on its first vacant site
  (`staged.rs:226-251`, the committed reading's `get_or_insert_with` pattern), so there is no "hold
  only if owed" predicate left to mutate without effect.
- **`read_owned` parses the key only** (`staged.rs:211-218`): reconstruction needs only which chunk
  a still-streaming part stages, and v1 already fell back to the key when the value would not decode,
  so decoding the value added nothing.
- **More tests**: a fence landing before the pre-mark (nothing written); a repointed part over the
  value ceiling (refused, nothing written, pass `Blocked`); leg B checks the adoption changed only the
  placement (byte comparison), and that GC keeps the vacated source inside its grace and reclaims it
  after; the in-place case checks the record comes back byte-identical.
- **Test GC grace is now the deployed 60 s.** v1 used 50 ms against a 20 s write window — the
  pairing 0016 forbids (adversary finding).

## 3. Resolution rules (unchanged from v1)

- Read order staged (`sidx:` then `part:`) → committed (`inode:`) (`reconstruction.rs:224-232`).
- A committed site wins over a staged one.
- `Open` session → repair; other state → `Deferred("session-fenced")`; `sidx:`-only →
  `Deferred("in-flight")`; an undecodable session record or vacated-source mark → `Withheld`
  (NEEDS-HUMAN audit, pass `Blocked`).
- Drains are withheld while either reading is incomplete (`reconstruction.rs:421`).
- `PartRecord` has no constructor (base `crates/core/src/multipart.rs:2492`); `repointed_part`
  (`staged.rs:593`) splices the re-encoded chunk list into the stored bytes and accepts the result
  only if the canonical decoder reads it back.
- Scrub reads the parts of every listed session whatever its state; an obligation for a fenced
  session's part is deferred until the chunk is published (committed path) or retired (drain).

## 4. Evidence

- **C4-verify** (`engine/scripts/run-verify.sh`, patch applied to a clean `origin/main` checkout),
  final patch: GREEN `38 passed`; RED `2 passed; 36 failed`; `PASS — red without the fix, green with
  it`. The two that pass on the base are guards that should: `a_staged_chunk_already_whole_drains_its_obligation`
  (the base drains that one too, correctly) and `a_scrub_verifies_a_chunk_named_by_both_classes_once`
  (on the base the committed map alone names the chunk). Every red is an assertion failure, not a
  compile error — the test names only base symbols.
- **C4-diff-cov** (`engine/scripts/run-diff-cov.sh`): **98.0% (492/502)**. Misses: `gc.rs:1044-1046`
  (GC's unnamed-session arm, moved code exercised by `staged_protection.rs`, which this gate does not
  measure); `staged.rs:250` (the implicit branch for a chunk two parts both name — first in key order
  wins, the committed reading's rule; not worth a test that pins an anomaly); `staged.rs:608-612`
  (`repointed_part`'s "does not spell its own chunk list" error — unreachable, since the decoder
  accepts only canonical bytes); `scrub.rs:339` (the `Owned` arm a parts-only walk never meets).
- **C5-mutants** (`scripts/mutants-in-diff`): **79 mutants, 34 caught, 45 unviable, 0 missed**
  (v1: 4 missed).
- **C4-ci** (`./engine/xtask.sh ci` = `cargo xtask ci`) on the final tree (identical to
  `patch.diff`, checked with `cmp`): `xtask ci: all checks passed`, exit 0 — typos, docs lint,
  guards, fmt, clippy (warnings denied), workspace build + test, conformance, statics gate,
  orchestrator guard, madsim clippy + DST at 50 seeds (`tests/custodian.rs`: 22 passed, including
  both new properties and `committed_regression_seeds_stay_green`).
- **host-tikv** (`WYRD_TIKV_TOOLCHAIN=1 cargo clippy -p wyrd-metadata-tikv --features tikv --tests
  && cargo clippy -p wyrd-server --features tikv,etcd --tests`): exit 0.
- **DST (brief leg F)**: `staged_replace_never_strands` (campaign: the seed picks the fence instant
  in 0..=20 ms or no fence, slow reads or not, and a normal, queued-late or publishing-late write) and
  `staged_replace_reaches_every_window` (coverage: walks all 21 fence instants over a fast pass and
  asserts the fence landed before the pre-mark, between pre-mark and write, between write and
  adoption, and after the adoption; then both refusal effects; then the slow pass repairs). **Seed
  count: 50** each (`MADSIM_TEST_NUM=50`, `xtask/src/main.rs:1573`), so the coverage leg makes
  50 × 24 = 1,200 runs; plus the campaign leg inside `committed_regression_seeds_stay_green` over its
  8 fixed seeds.
- **Commit hooks**: the target configures no git hooks; `cargo fmt --all -- --check` and `typos` on
  the changed files are the commit-time checks, both clean.

### A hang seen once in CI, on a path this patch does not execute

The first full CI run (on an intermediate tree) stalled in `crates/server/tests/custodian_gc.rs`:
7 of its 10 tests sat asleep at 0 s CPU for 13 minutes, among them
`deployed_run_loop_refuses_duplicate_endpoints`, which returns its refusal from argument checks
inside `cmd_custodian` before any reconciliation step runs. I killed that run. The same binary (same
hash, workspace-feature build) then passed 60 of 60 runs, and the per-package build 40 of 40, each
under a 30 s timeout. It looks like a pre-existing intermittent deadlock in that test binary's
global setup (telemetry/logging init across parallel tests), not this change; if it recurs, it is
worth an issue of its own. The final CI run's result is above.

## 5. Refute-your-own-test (forced)

- **(a) Genuine red?** Yes. `run-verify.sh` reverted the production files on a clean base and the
  new test went red 36/38 on assertions; the two that stay green are the guards named above. Each
  carry-forward fix was also reverted on its own and its regression test went red (fix 1: the
  maintenance test; fix 2: the in-process stamp test and the DST slow-pass assertion; fix 3: the DST
  refusal assertions). All experimental edits were restored and checked byte-identical (`cmp`).
- **(b) Production path?** Yes. Every leg calls the production `reconcile_step` with real
  `ScrubContext` / `ReconstructionContext` / `GcContext`; nothing is re-implemented. Only the stores
  are doubles, and both D-server doubles judge deadlines through the seam's own
  `WriteDeadlineExpired::if_elapsed` / `if_publication_unverified`.
- **(c) Fixture includes the fault?** Yes. The lost fragment is really deleted or bit-flipped on its
  server; the fence really rewrites the `mpu:` record (after the write, or right after the
  destination choice); the drain is a real `desired:dserver:` record, including one with an unknown
  value; stale and `reclaiming` marks are real records in the store; the refusals come from the
  doubles' own deadline checks against clocks the test moves; the slow pass really spends the time
  (real 60 ms reads in-process, 10 s simulated reads in DST); the DST fence is a concurrent task
  over the simulated-TiKV model at seed-chosen instants, and the coverage leg proves all four windows
  are hit.

## 6. Open items for sign-off (not blockers I could resolve inside the brief)

- **Docs currency.** `docs/design/architecture/06-runtime-view.md:80` ends "Scrub and the
  drain-status query read committed references only" — now false for scrub, and reconstruction now
  resolves staged obligations. `docs/design/architecture/05-building-block-view.md:202` says
  "Nothing writes or consumes these records in production yet" — reconstruction now rewrites a
  `part:` record's placement (already stale on base for GC's consumption since #803). The brief puts
  `docs/` out of scope, and none of the rubric's docs-currency triggers (port, API operation, RPC, CLI
  flag, persisted field) is hit, so I left them. Needs a human call: here, or in #664.
- **`W_write` configurability** — see fix 4.
- **Size.** The patch is ~215 KB (v1: 177 KB); the new test file is ~2,000 lines and the DST
  property ~650. The size backstop will fire again; last round it was overridden.
- The caller-side await on `put_fragment` is bounded by the `ChunkStore` implementation, the rule
  every custodian await follows (#508/#636); the write's *effect* is bounded by the D-server
  deadline.
- The committed repair path still does not exclude draining servers (unchanged by design, brief).
- Reconstruction reads every listed session's `sidx:`/`part:` ranges on each pass with a non-empty
  queue — bounded pages, the same cost class GC already pays every pass.
