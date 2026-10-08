# Adversarial review — #843 (restore session fence, seeded Tier-0 DST)

**Verdict: could not refute.** I re-ran the brief's falsifiability step (D4) myself instead of
trusting `build-notes.md` (withheld). In a scratch copy of `$PDCA_TARGET` I built `wyrd-dst`
under `--cfg madsim` (`MADSIM_TEST_NUM=50`), broke the production fence in
`crates/custodian/src/restore.rs` one way at a time, and ran the three new tests. Each break was
undone before the next. The gates C4-verify (green-only), C5-mutants ("No mutants to filter")
and C4-diff-cov (n/a) prove nothing about red here, because the patch changes no production
code. These manual runs are the real red→green evidence.

| Mutation of the production fence | D1 campaign | D2 ambiguous | D3 coverage |
|---|---|---|---|
| (a) drop `require(key, read)` at `restore.rs:824` | **FAIL** | ok | **FAIL** |
| (b1) split: session commit first, obligations second | ok | **FAIL** (`retire:` namespace empty after `Landed`) | ok |
| (b2) split: obligations first, session second | **FAIL** | **FAIL** | **FAIL** |
| iteration-1 finding: check obligation keys before the session re-read (`restore.rs:844-856`) | **FAIL** | ok | **FAIL** |
| read an unknown commit outcome as `Conflict` (`restore.rs:832`) | ok | **FAIL** | ok |
| unmodified fence, `MADSIM_TEST_NUM=1000`, plus `committed_regression_seeds_stay_green` | ok | ok | ok |

Attacks tried, and what came of each:

- `crates/dst/tests/custodian.rs:5585` (the "exactly one transition out of @E" assert): mutation
  (a) fails here at **one** landing point only: Open arm, writer at 5500 µs. In that schedule the
  writer's prewrite (6.5 ms) comes before the fence's `mpu:` read (7 ms), and its apply (7.5 ms)
  comes before the fence's prewrite (8 ms). When the writer decides *after* the read (6500 µs),
  SimTikv refuses the fence on the writer's lock (`crates/dst/tests/support/mod.rs:322-331`), not
  on the precondition. So DST coverage of the session precondition depends on a single 1 ms slot.
  That is fine because D3 asserts that slot is reached for each arm (`custodian.rs:5886`), so a
  longer pass that pushed it out of `RESTORE_FENCE_SPAN` (`:5046`) would fail loudly. Not a defect.
- `custodian.rs:5598` (`stale` classification): I checked it against the SimTikv model. A fence
  `Conflict` logged before the writer's answer can only be a lock refusal, because the writer
  answers at apply time and releases its locks then. `stale: true` therefore really does mean the
  writer had applied before the fence's prewrite. The D3 claim holds. In the Completing arm that
  conflict could also come from `require_absent(retire:records:…:E)`. The doc comment at
  `:5873-5876` says so, and the Open arm carries mutation (a), as the brief requires.
- `custodian.rs:5675` (narrowed to `ChangedUnderPass`): the iteration-1 carry-forward is fixed.
  Moving the obligation-key check ahead of the session re-read now fails D1 and D3 in the
  Completing arm (it reports `ObligationKeyTaken` on the flip's own key).
- Dropping the fence's `require_absent` on its obligation keys (`restore.rs:825-827`) **survives**
  all three new properties. In this race the session precondition covers it, since the writer
  always rewrites the session too. The existing per-pass test
  `crates/custodian/tests/restore_completing_fence.rs` (`neither_obligation_overwrites_one_already_there`)
  catches it, and the brief does not ask DST to. Not a finding.
- Flakiness: the pass and writer run on fixed 1 ms hops (`support/mod.rs:190-192`), `MemDServer`
  adds no hops, and the +500 µs offset rules out ties, so each (arm, delay) pair is one fixed
  schedule. It passed 1000 seeds.
- Observation, no action needed: for the same reason, the D1 seeded campaign
  (`custodian.rs:5747-5755`) replays only schedules that D3 already walks in full on every seed
  (`:5877`). The madsim seed only chooses which of the 15 slots to replay. The doc text "so 50
  seeds sweep the schedule space" (`:5747`) and the 1-in-a-million math for `RESTORE_FENCE_DRAWS`
  (`:5047-5051`) overstate what the seed adds. Nothing is wrong, because D3 is the deterministic
  guarantee. This matches the existing restore nemesis legs (`:2144`).

- NEEDS-HUMAN [human] — `crates/custodian/src/restore.rs:785` still says
  `// deferred: #843 — seeded Tier-0 DST coverage of this fence (809.5).` This patch is #843 and
  delivers that coverage, but the brief's scope ("Nothing outside that file … out of scope: any
  production change") keeps the marker in place. Under the rubric's "Deferrals are settled" rule,
  that marker tells later reviewers the fence's DST coverage is still deferred, which will be false
  once this merges. A human should decide whether to allow this one-line comment removal in this
  PR or file a follow-up. It is a scope call, not a build defect.
