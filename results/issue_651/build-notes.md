# Build notes — issue 651, iteration 14 (Do)

> Withheld from the reviewer by the driver; written for the human at sign-off.

## What this round is

A **narrow delta on the v13 tree**, not a rebuild. `iteration-v13/patch.diff` was applied to the
worktree at `origin/main` (`d50f0ca`) and edited. v13's gates were all green — C4-ci pass,
C4-verify red→green 5/5, C5 38 mutants with no survivor, C5 causal adequacy and T1 PASS — and the
only failing gate was **T4 batch review, 2 blocking findings**, both the same TEST-GAP:

> `crates/custodian/src/restore.rs:296` / `:585` — *the new concurrent two-scan marking path can
> authorize GC-visible orphan marks but has only Tokio/in-memory interleaving tests, violating the
> requirement that new destructive or concurrent paths include seeded Tier-0 DST coverage.*

The iteration-13 carry-forward directs exactly one piece of work: **add that DST leg**, exercising
"the interleaving the Tokio doubles only approximate", pinning "the brief's own one-reading rule".
It also directs **not** to chase the advisory reviewer's CLI findings (record-reject them) and
**not** to reintroduce the dropped ambiguity rule. That is what this round did, and nothing else:
the production semantics of v13 are unchanged, line for line.

## The delta, in three parts

### 1. The seeded Tier-0 DST leg — property 11 (`crates/dst/tests/custodian.rs:1727-2145`)

Two campaign tests, both declared through `dst_campaign_test!` (so the ADR-0035 determinism
barrier is unbypassable) and both swept over 50 seeds by `cargo xtask dst` inside `cargo xtask ci`:

- **`restore_two_readings_never_license_a_mark`** (`custodian.rs:2217-2221`) — the seed picks *which*
  disagreement a genuinely concurrent writer causes and *where* it lands.
- **`restore_two_readings_cover_the_divergence_window`** (`custodian.rs:2223-2227`) — walks the whole
  landing span in one run and asserts the window this property exists for is genuinely **reached**.
- The first is also replayed over the eight committed `REGRESSION_SEEDS`
  (`custodian.rs:2260`), so a bug-finding seed becomes permanent (ADR-0009).

**Why it is not another Tokio double with a nicer name.** The per-slice tests in
`crates/custodian/tests` drive the two-scan race through a metadata double that publishes at a
**hard-coded seam** — the instant the first `inode:` scan is answered
(`restore_reconcile.rs:44/58/89-95`). They pin the *decision*, but the test chooses the schedule. The
DST leg changes three things that matter:

- The store is the DST tier's **second `MetadataStore` implementation**, the simulated-TiKV model
  (`crates/dst/tests/support/mod.rs`), whose every read and commit spans a real madsim await
  boundary (`network_hop`). An in-memory `MemMeta` never yields, which is *why* a Tokio double can
  only approximate the race — there is no schedule for the scheduler to pick.
- The writer is a real concurrent task (`madsim::task::spawn`, `custodian.rs:1947`) whose landing
  point comes from the run seed, so 50 seeds sweep the window and ties at the same virtual instant
  are resolved by the seed, not by the fixture.
- The assertions are conditioned on **what the pass's own readings returned**, observed at the
  store seam by a recording tap (`RecordingMeta`, `custodian.rs:1779`), never on the fixture's
  intended timing. That is what makes them implementation-neutral in the brief's (2c) sense: a pass
  that read the namespace **once** satisfies every one of them, and the coverage leg's divergence
  clause is explicitly skipped when `readings <= 1` (`custodian.rs:2137`) rather than punishing the
  better implementation.

The six invariants asserted on every seed and every landing point (`custodian.rs:1974-2083`):

| # | Invariant | Why it is the C-1 one |
|---|---|---|
| 1 | the pass returns `Ok` | one damaged record may not blank the report for every object the pass could read |
| 2 | never both `stranded_marked > 0` and a record it could not read | the brief's (2c) conjunction — two readings, two conclusions |
| 3 | a fragment **either** reading protects is never marked | the late commit's only copy, else GC takes it after the grace window |
| 4 | either reading's hole withholds every mark, and the record is **named** | attribution, or the stall is a state nothing exits |
| 5 | the readable object's fragment is never marked, on any schedule | — |
| 6 | a reading that **finished** marks the genuine stray; one that did not, marks it after the named record is repaired | the positive observable, in both directions |

(6) is why "nothing was marked" can never pass vacuously: on a complete reading the pass **must**
mark the stray, and on an incomplete one the leg repairs the record the run named, re-runs the same
pass over the same store, and requires the withheld mark to appear (`custodian.rs:2059-2083`).

### 2. Two pointer comments in `restore.rs` (docs only, no semantics)

At the two sites the finding is filed against: the one-reading rule in `reconcile_after_restore`'s
docs (`crates/custodian/src/restore.rs:245-249`) and `committed_chunks`'s docs (`:610-614`) now
name the DST property that pins them. This is the cheap defence against the same finding being
re-filed from a site that gives the reviewer no way to see the coverage. `cargo fmt` clean; the
semantic-line count of `restore.rs` is unchanged (110 in both v13 and this patch — comments are not
counted, and nothing else moved).

### 3. Bundle hygiene — `review-rejected.md`

- The `restore.rs` rows were re-filed at this patch's line numbers (the gate binds a rejection to
  `file:line` + CLASS + MATCH, so a 12-line doc insertion above them reads as an unsuppressed
  finding): `277→283`, `287→293`, `288→294`, `293→299`, `604→616`, `613→625`, `632→644`,
  `815→827`. `desired_state.rs` rows are unmoved.
- Added section (iv): the advisory reviewer's round-13 CLI findings (the `dangling` / `misplaced`
  paragraphs printing a count and "See the audit log for each chunk id"), **declined** — the text
  is verbatim on the base (`git show origin/main:crates/server/src/cli.rs` `:1213-1215`,
  `:1221-1226`) and widening it is the report-schema churn `brief.md` § Scope declines. This is the
  iteration-13 carry-forward's own instruction ("Record-reject these rather than rebuilding the CLI
  output shape"), executed.

## The ninth file — flagged, not smuggled

`brief.md` § Budget names eight files and says *"A ninth file means the shape is wrong: STOP and
hand back."* This patch touches **nine**: the eight, plus `crates/dst/tests/custodian.rs`.

I did not stop, and the reason is that two instructions conflict and the later, bundle-specific one
is explicit. The iteration-13 carry-forward (the human's, after the brief) says: *"Add seeded Tier-0
DST coverage for that path… Note the brief currently states 'No DST leg' under External
dependencies. That line is now stale against the shipped design… Adding the DST leg is the correct
resolution… Expect the DST leg to add test bytes; that is not overage in the sense the backstop
means."* A seeded Tier-0 custodian property has exactly one home in this repo — the campaign file
that owns the seed sweep and the `REGRESSION_SEEDS` replay — and a **new** file there would be a
ninth file too, without the seed-regression replay. So the ninth file is unavoidable given the
directed work; the only choice was where.

**What it costs, measured** (crude counter: added lines that are non-blank and non-comment):

| | v13 | this patch |
|---|---|---|
| production (`restore.rs`, `desired_state.rs`, `cli.rs`) | 308 | **308 — unchanged** |
| tests | 640 | 932 |
| total | 948 | 1240 |
| files | 8 | 9 |
| patch bytes | 121 KB | 145 KB |

The whole +292 is the DST property and its fixture. Production semantics did not move by one line
this round.

## Alternatives I rejected, with their cost

**(a) A new DST file, `crates/dst/tests/restore_two_readings.rs`.** Same ninth-file count, and it
would have to re-declare the fixture the campaign file already owns: `MemDServer` (31 lines),
`servers()`/`fleet_of` (9), the `#[path = "support/mod.rs"]` include, plus its own
`dst_campaign_test!` wiring — ~45 duplicated lines — and it would sit **outside**
`committed_regression_seeds_stay_green`, so a bug-finding seed would not be replayable through the
campaign's own regression mechanism. Rejected on both counts, not on taste.

**(b) A hand-rolled paced double over `MemMeta`** (add `madsim::time::sleep` to a copy of the
in-memory store, ~25 lines) instead of `SimTikvMetadataStore`. Cheaper by roughly the 12 forwarding
lines of `RecordingMeta`'s trait impl, and **worse**: the rubric's test-fidelity rule is *"DST/sim
models mirror the production adapter's error and seam semantics"*, and the repo already has the
sanctioned model with a commit that awaits mid-flight, pessimistic prewrite locks and the blind-batch
`Err` rule. Inventing a third store to get an await boundary is how a sim model drifts from the
adapter it stands in for. `RecordingMeta` therefore adds **no** semantics — it forwards every call
and records one thing (each `inode:` scan's answer).

**(c) Asserting "nothing is ever marked" outright.** Two lines shorter than the conditioned form,
and it fails the single-reading implementation the brief calls *"the better one"* (§ Success
criterion 2c), and it fails legitimately whenever the writer lands after both readings. Rejected
by the brief, not by preference.

**(d) Driving the DST leg through `reconcile_step`** like properties 1–10. `reconcile_after_restore`
is not a loop step — it is an operator one-shot the CLI calls directly
(`crates/server/src/cli.rs`'s `restore_verdict` path), and `reconcile_step` has no restore arm. The
leg drives the production entry point; there is no fenced control point to route through.

## Refuting my own test (the three forced questions)

**(a) Genuine red?** Yes — three separate reverts, each re-run, each restored afterwards:

| revert | leg | result |
|---|---|---|
| drop `appeared.protects(..)` from the mark gate (v13's fix, `restore.rs:364`) | **through the project runner** `./engine/xtask.sh dst` | both new legs **FAILED**: *"reading(s) [1] of THIS pass returned the record that places FragmentId { chunk: 25874, index: 0 }, and the pass marked that fragment collectable anyway"* (at landings 0 ms and 2 ms) |
| drop `attribute_unresolvable(&committed.unresolvable, ..)` (v12's union-of-holes, `restore.rs:300`) | `cargo test -p wyrd-dst --test custodian` under `--cfg madsim` | both **FAILED**: *"reading(s) [1] met a record this pass could not read, and it marked 2 fragment(s) anyway … RestoreReport { stranded_marked: 2, …, unresolvable: [] }"* — note the report itself said nothing was unresolvable, so an assertion phrased only over the report would have missed it; the recording tap is what gives it teeth |
| whole `restore.rs` reverted to `origin/main` | — | **compile error** (`no field `unresolvable` on type `RestoreReport`), as expected and by design: the DST leg is CI-gated coverage, not the discriminator, so it may name symbols this patch introduces. The assertion-red-on-base constraint belongs to `crates/custodian/tests/segmented_map_restore.rs`, which C4-verify re-proved this round (below) |

And the discriminator itself, through the project's own gate:
`PDCA_BUNDLE=… ./engine/scripts/run-verify.sh` → **`PASS — red without the fix, green with it`**
(5/5 legs red on the base with `SegmentedMapUnsupported { operation: "restore::committed_chunks" }`
and a decode error).

**(b) Production path?** Yes. The DST leg calls `wyrd_custodian::reconcile_after_restore` — the
same function `wyrd custodian --reconcile-after-restore` calls — over a real `GcContext`, a real
fleet of `ChunkStore`s and the repo's own simulated-TiKV `MetadataStore`. Nothing about the pass is
re-implemented in the test; `RecordingMeta` forwards every trait call and adds no behaviour. The
marks are read back as the `orphan:` records the production code writes
(`metadata::orphan_key`, the same function `restore.rs` marks with), not as a report field.

**(c) Fixture includes the fault?** Yes, and the coverage leg **proves** it rather than asserting
it. The probe run (temporary, removed) classified every landing point:

```
readings=2
divergent=[(LateCommit,0),(LateCommit,1),(LateCommit,2),(Damage,0),(Damage,1),(Damage,2)]
past=[(LateCommit,3..6),(Damage,3..6)]
```

i.e. at landings 0–2 ms the writer genuinely lands **between** the pass's two readings — the exact
schedule the finding is about — and at 3–6 ms it lands past the pass. The permanent form of that
check is `restore_two_readings_cover_the_divergence_window` (`custodian.rs:2111-2145`), which reds
if a future change moves the readings so the span no longer covers the window: the failure mode is
"the window is no longer reached, re-tune", never a silent vacuum. The nemesis is really injected
(the writer's own commit is asserted to have landed, `custodian.rs:1958`), the damaged record
really stops decoding (the pass names it, invariant 4), and the marks asserted absent are asserted
present in the same run under invariant 6.

## Gates run here (the project's own runners, not hand-rolled)

- `./engine/xtask.sh dst` (the DST tier: clippy on `wyrd-dst` under `--cfg madsim`, then 50 seeds)
  → **green**; 14 campaign tests including the two new ones.
- `PDCA_BUNDLE=… ./engine/scripts/run-verify.sh` (C4-verify) → **`PASS — red without the fix, green
  with it`**. Classification re-checked with `--classify`: exactly one
  `ADDED_TEST crates/custodian/tests/segmented_map_restore.rs` (plus `CRATE crates/custodian`,
  `CRATE crates/dst`, `CRATE crates/server`), so the invocation is still
  `cargo test -p wyrd-custodian --test segmented_map_restore` and the new DST file — a *modified*
  test file — changes neither leg.
- `./engine/xtask.sh ci` (C4-ci, the whole gate) → **exit 0**, `xtask ci: all checks passed`. The
  prose gates really ran on this host (`typos`, `lint_docs: OK`, `render_site: link audit OK`), so
  this is CI parity rather than the warn-skip path.
- `cargo fmt --all --check` → clean, so the target's commit hooks have nothing to reject.

## Anything a reviewer might raise that I decided rather than missed

- **The leg does not cover "the writer landed before the first reading."** Unreachable by
  construction: the writer's own commit spans two network hops while the pass's first reading is
  one, so the earliest possible landing is after it. That regime is just "the object was already
  committed", which the per-slice tests and property 4 already own. The coverage leg therefore
  requires the *divergent* and *past-the-pass* schedules, and does not claim the third.
- **`RESTORE_NEMESIS_SPAN = 6` is a tuned constant.** Documented at the constant with the hop
  arithmetic it comes from, and the coverage leg fails loudly if the tuning goes stale.
- **The leg asserts on `orphan:` records, not on deletions.** Restore deletes nothing; the mark is
  the authorization. GC's own reclamation of a marked fragment is property 4's subject, and
  property 10's for the incomplete-set case.
- **`Interleaving` is returned but only the coverage leg reads it.** Deliberate: it is the seam
  that lets the coverage claim be *checked* rather than asserted in prose.

## Self-review against the target's standing rubric (`AGENTS.md` § Review rubric & protocol)

Read before emitting the patch, applied to it as the last step. Only the delta is re-examined here;
the v13 body was self-reviewed against the same rubric in `iteration-v13/build-notes.md`.

- **One clock per correctness lifecycle** — the DST leg reads no clock. It uses
  `madsim::time::sleep` (virtual time, the simulator's) for the writer's landing point and passes a
  fixed logical `now = 10_000` to the pass, the caller's own stamp. No `SystemTime::now`, so
  `clippy.toml`'s disallowed-method gate has nothing to review.
- **Narrow trait seams / dependency direction (ADR-0010)** — the leg lives in `crates/dst/tests`,
  adds no dependency to any production crate, and touches `wyrd-custodian` only through its public
  API (`reconcile_after_restore`, `GcContext`, `RestoreReport`).
- **Metadata validation boundaries (ADR-0045)** — the damaged record is bytes that fail *decode*,
  and the pass surfaces it as an error contained per record; nothing is identity-filled.
- **No DST-reachable shared mutable global state (ADR-0035)** — `RecordingMeta` is instance state
  only (a `Mutex` field, no `static`), and both new tests are declared through `dst_campaign_test!`
  so the barrier is installed. `cargo xtask ci`'s statics gate passed.
- **`#![forbid(unsafe_code)]`** — no new crate root; `custodian.rs` already carries it.
- **Docs currency** — no port/API/RPC/CLI-flag/persisted-field change in this delta (the v13 patch's
  are documented in both living architecture docs). The two `restore.rs` doc insertions point at
  the new coverage.
- **Absent or unsupported entries** — the leg's whole subject: an unreadable record is named,
  nothing is silently skipped, and the assertions read the `orphan:` records themselves rather than
  a count.
- **Await discipline** — the delta adds no production await. The test's awaits are the simulator's.
- **Test fidelity — the rule this round exists to satisfy** — the new concurrent path now lands
  with seeded Tier-0 DST coverage, driven over the sanctioned simulated-TiKV model rather than a
  bespoke double, and the coverage leg proves the interleaving is reached.
- **Reviewer protocol** — both round-13 findings are **fixed** (not silenced); the advisory CLI
  findings are declined with a recorded reason and a scope citation in `review-rejected.md`.

## Scratch hygiene

Three throwaway files under `$PDCA_SCRATCH` (`pdca-builder-651-restore.rs.keep`,
`pdca-builder-651-restore-final.rs`, `pdca-builder-651-ci.log`) — the revert backups and the CI log.
Removed at the end of the round; nothing was written to `/tmp`.
