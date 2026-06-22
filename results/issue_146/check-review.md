# Check review — issue 146 / `m3.8-dst-campaign`

> Advisory, artifact-only, decorrelated from the builder. Inputs held: `patch.diff`,
> `brief.md`, `check-gates.json`. **`build-notes.md` is withheld** — so anything the
> builder was told to record *there* (the per-property demonstrated assertion-level reds,
> the exact target file names) is not verifiable from my artifacts and routes to a human.
>
> **Grounding note:** `$PDCA_TARGET` was not readable in this environment (`env`/`printenv`
> were denied), so per protocol I ground every citation on `patch.diff` alone and did **not**
> search any other checkout on the machine. Citations of the form `path:NNN` are to the
> in-repo path the hunk creates/edits; `patch.diff:NNN` points at the raw diff line.

## Verdict table

| Item | Verdict | Basis |
|------|---------|-------|
| C1 — C1 Spec | PASS | Oracle is `brief.md`, which points at the immutable host artifact 0005 and enumerates all six binding Tier-0 properties with line refs (`brief.md:42-61`, `0005:378-403`) plus the invariant (`brief.md:62-71`); spec is concrete and category-gated, not re-decided here. |
| C2 — C2 Reproduction (red pre-fix) | NEEDS-HUMAN | Structural red is confirmable from the diff — `custodian.rs`, the testkit storage seam, and the xtask runners are all net-new (`patch.diff:40-42` `new file`, `patch.diff:1115-1212`, `patch.diff:1268-1272`). But this is born-at-tier infra (`brief.md:105-112`): the load-bearing red is the *demonstrated per-property negation* the builder was told to record in the withheld `build-notes.md`. No C2 gate was configured (`check-gates.json:14-22`). Human must confirm each property was shown to go red. |
| C3 — C3 Change | PASS | Change is coherent and scoped to the brief: Tier-0 suite `crates/dst/tests/custodian.rs` (`patch.diff:40-45`), testkit `StorageFault`/`SeededStorageFaults` seam (`crates/testkit/src/lib.rs`, `patch.diff:1130-1212`), deferred Tier-1/2 xtask runners (`xtask/src/faults.rs`, `patch.diff:1273+`) wired as standalone subcommands only (`xtask/src/main.rs`, `patch.diff:1504-1506`). No production custodian behaviour added — matches `brief.md:81-91` "adds no production logic". |
| C4 — C4 Verification (red→green) | NEEDS-HUMAN | Green side is established: gating `C4-ci` PASS — `./engine/xtask.sh ci` incl. the DST sweep "all checks passed" (`check-gates.json:32-40`), so the suite compiles under `--cfg madsim` and all six properties + regression seeds are green. **Red side is not**: non-gating `C4-verify` FAILED — "the test PASSES without the fix … no red" (`check-gates.json:42-49`). That is the *expected* artifact of a net-new infra slice (there is no production fix to revert; `brief.md:105-112`), so the per-property reds live in the withheld `build-notes.md`. Human must confirm those demonstrated reds make the green load-bearing. (Residual: I cannot confirm from the diff that `run_ci` invokes `run_dst` — the brief and the gate assert the sweep is included.) |
| C5 — C5 Causal adequacy | NEEDS-HUMAN | Oracle is reviewer + human sign-off (`check-gates.json:51-58`). Whether the six properties actually pin custodian correctness to the *real* `reconcile_step` fenced control point (Option A over trait seams) — vs. passing by construction of the in-memory stores (`CrashMeta` models a crash as a `Conflict` on any positive-precondition commit, `patch.diff:201-211`; `MemMeta`/`Fleet` are hand-rolled) — is a judgment call that depends on the withheld demonstrated reds. Human sign-off required. |
| T1 — T1 Structure | PASS | Files land where the brief prescribes: Tier-0 as `crates/dst/tests/custodian.rs` gated `#![cfg(madsim)]` (`patch.diff:87`), six named `#[madsim::test]` properties + a committed-regression-seed test (`patch.diff:1051-1110`), testkit seam in `crates/testkit/src/` with three unit tests (`patch.diff:1221-1266`), runners in `xtask/src/faults.rs` with five unit tests (`patch.diff:1424-1477`). Matches `brief.md:98-104`. |
| T2 — T2 Shape | PASS | Each property is arrange/act/assert through real control points: write via `write_new_object_placed` (`patch.diff:450`), inject seed-drawn faults via `SeededStorageFaults` (`patch.diff:544`, `:696`), drive the real `reconcile_step` (`patch.diff:567`, `:619`, `:709`, `:857`, `:934`), and assert via the real read path + `repair::fragment_intact` checksum verify (`patch.diff:511-516`, `:744-746`). Not mocked-away; faults flow from the run seed so each test is a pure function of its seed (`patch.diff:1047-1049`). |
| T3 — T3 Runtime | PASS | Gating `C4-ci` PASS (`check-gates.json:32-40`) means the madsim DST sweep ran the six properties + the regression-seed replay green, and the testkit/xtask unit tests passed. No gate is separately configured for T3 (`check-gates.json:78-85`); runtime evidence comes from the CI row. |
| T4 — T4 Contribution | PASS | Assertions are substantive and falsifiable, not tautological: version-increments-exactly-once (`patch.diff:578`, `:671`), placement on N distinct domains (`assert_full_redundancy`, `patch.diff:501-522`), checksum-intactness of rebuilt shards (`patch.diff:744-746`), GC selectivity across live/leased/old/new (`patch.diff:868-895`), fenced-leader rejection (`patch.diff:934-967`), exact metric values (`patch.diff:1001-1039`). Adds the consolidated campaign coverage that per-slice tests did not (`brief.md:28-40`). Per-assertion load-bearingness ultimately rests on the withheld demonstrated reds (see C2/C5). |
| T5 — T5 Judgment | NEEDS-HUMAN | Oracle is reviewer + human sign-off (`check-gates.json:96-102`). Taste-level calls — Option A (over trait seams, no deployed custodian process) as the campaign substrate, the `CrashMeta`-as-`Conflict` crash model, coupling property-6's time-to-repair assertion to the literal `now` arg (`reconcile_step(..,500)` → `vec![500]`, `patch.diff:1012-1015`), and the breadth of one slice spanning three crates + two deferred tiers — are for the human reviewer. |
| V — Validation — fitness-to-purpose | NEEDS-HUMAN | Always-human (`check-gates.json:104-112`). Whether this campaign actually discharges M3's graduation gate (`0005:500-502`) — that the six machine-checked properties + committed regression seeds + the deferred Tier-1/2 jobs together constitute "M3 is verified" — is the maintainer's fitness-to-purpose judgment at sign-off, including accepting the pre-declared deferred-posture green (Tier-1/2 observed off-Check, `brief.md:113-121`). |

## §6 — Items a human must clear

Each NEEDS-HUMAN row above is a sign-off item:

1. **C2 / C4 — demonstrated per-property reds (withheld evidence).** The automated red→green
   probe FAILED ("test passes without the fix", `check-gates.json:42-49`). For this net-new,
   born-at-tier suite that is expected, and the brief routes the real red-demonstration to
   `build-notes.md` (`brief.md:107-112`) — which I do not hold. **Action:** confirm each of the
   six properties was shown to go red under a temporary negation/stub (e.g. negate the fenced
   CAS so a deposed leader's update lands; negate the GC grace window so a referenced fragment
   is reclaimed; suppress the under-replicated decrement), so every assertion is load-bearing
   rather than resting green on file non-existence.

2. **C4 — confirm the Tier-0 sweep is inside the gating CI.** The diff does not show `run_ci`
   calling `run_dst`; the `dst` subcommand is pre-existing and unchanged. The brief and the
   `C4-ci` row assert the sweep is included ("incl. the DST sweep", `brief.md:121`;
   `check-gates.json:35-37`). **Action:** confirm `./engine/xtask.sh ci` actually exercises
   `crates/dst/tests/custodian.rs` under `--cfg madsim` (else the green covers only the
   testkit/xtask unit tests, not the six properties).

3. **C5 — causal adequacy of the Option-A substrate.** **Action:** confirm the in-memory
   `MemMeta`/`Fleet`/`CrashMeta` stores and the `CrashMeta`-as-`Conflict` crash model
   (`patch.diff:201-211`) are faithful enough that a real regression in the four custodian
   loops would be caught — i.e. the properties pin behaviour to the real `reconcile_step`, not
   to test-harness construction.

4. **T5 — test judgment.** **Action:** accept (or push back on) the taste-level choices noted
   in the table — Option A substrate, crash model, the time-to-repair assertion coupled to the
   literal time arg, and the three-crate / two-deferred-tier breadth of a single slice
   (the brief pre-justifies the breadth as one category-gated slice, `brief.md:62-71`,
   `brief.md:11-15`).

5. **V — validation / fitness-to-purpose.** **Action:** maintainer confirms the campaign
   discharges M3's graduation gate (`0005:500-502`) and accepts the pre-declared deferred
   posture for Tier-1 (dm-flakey/dm-error + Jepsen) and Tier-2 (single-node kill-reconstruct),
   whose green is observable only off-Check (`brief.md:113-121`; `xtask/src/faults.rs` runners
   are INERT/deferred by default, `patch.diff:1289-1292`, `:1429-1440`).

## Notes (non-blocking observations re-derived from the diff)

- **Scope adherence looks clean.** The new subcommands are added only to the `match` and usage
  string (`patch.diff:1504-1518`), **not** to `run_ci`, and `faults.rs` runners exit cleanly
  unless explicitly opted in (`WYRD_TIER1`/`WYRD_TIER2`, `patch.diff:1339-1341`,
  `:1429-1440`) — so they cannot destabilise the unprivileged gate, consistent with
  `brief.md:113-117`. No production custodian logic is touched (`brief.md:87-91`).
- **Determinism.** The campaign is seeded from the madsim run seed (`rand_seed`,
  `patch.diff:1047-1049`) and replays a fixed `REGRESSION_SEEDS` set independent of the sweep
  (`patch.diff:1088-1110`) — the ADR-0009 "bug-finding seed is a permanent regression" rule the
  brief requires (`brief.md:54-56`). The testkit `pick`/`kill` selection is unit-tested for
  seed-reproducibility and the `≤ max` survivor bound (`patch.diff:1222-1256`).
- The `MetricCapture` tracing layer pulls in no OpenTelemetry runtime
  (`patch.diff:330-374`), keeping property 6 deterministic under the simulator — a reasonable
  ILLUSTRATIVE in-process assertion of the BINDING telemetry surface.
