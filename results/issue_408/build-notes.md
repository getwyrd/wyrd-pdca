# Build notes — issue 408, iteration v5

Target branch: `getwyrd/wyrd @ main` (base `e0e39c1`). Worktree: `$PDCA_WORKTREE`
(`/home/eddie/development/wyrd/wyrd.pdca-wt`). All `path:line` citations are against that worktree
at the patched state unless marked "(pre-fix)".

## What this iteration is

The Iteration-4 carry-forward kept the v4 core explicitly ("the pure core, serializer vocabulary,
and golden fixtures are confirmed against the real elle-cli and **should be kept as-is**") and named
**five defects**, four of them in the live scenario file — the one file no Check gate exercises. It
also said: "**Do NOT attempt the witnessed live run until these are fixed**".

So: started from `iteration-v4/patch.diff` (applies cleanly to `origin/main`), fixed the five, and
**then** — the environment preflighting green — produced the witnessed run and committed its report.
That report is the brief's acceptance artifact (§6, Success criterion (e)), and it exists now:
`docs/design/reviews/m4-checked-consistency-run.md`. **Elle returned real `true` verdicts for both
models** over a genuinely concurrent history recorded under a materialized partition of a live
3-node FDB cluster.

I did not take the adversary's two execution-verified findings on trust. I re-derived both from
production code and re-verified both against the real jar before changing anything (below).

## The five carry-forward items

### 1. The delete pool fabricated violations on a correct system

**Confirmed independently, two ways.** (a) By reading production: `ObservableS3Client::put(key,
version)` takes the version as an **argument** (`crates/server/src/consistency_observable.rs:249`)
and `get` decodes the tag the writer wrote — so a version tag orders by **writer**, not by
**commit**. All three #406 checks compare raw tags on a key (`reads_are_monotone`,
`crates/server/src/consistency_workload.rs:511`). Two writers on one key ⇒ the premise is false. (b)
By execution: the trap is now a committed test that **passes**, i.e. it reproduces the fabrication —
`a_shared_delete_pool_key_with_version_bands_would_fabricate_a_violation`
(`crates/server/tests/consistency_workload.rs`), which feeds the v4 banded shape through the real
production checks and asserts all of them wrongly report `false` on a linearizable history.

**Fixed at the premise, not the symptom.** `delete_pool_key(process)`
(`crates/server/src/consistency_workload.rs:115`) gives each process its **own** key — single writer
per key ⇒ tag order = commit order ⇒ the landed checks are sound **untouched** (they stay out of
scope, per the brief). The alternative — loosening the checks to tolerate multi-writer tags — would
have edited #406's checks (explicitly out of scope) and weakened the only judge the delete traffic
has, to accommodate a pool shape I chose. The scenario consumes it at
`crates/server/tests/consistency_run_fdb.rs:529`.

Cost of the rejected alternative, concretely: waiving cross-process tag comparisons means deleting
the `Obligation::AtLeast(w, _) if v < w` arm (`consistency_workload.rs:260`) and the whole
`reads_are_monotone` cross-process comparison (`:511-529`) — ~20 lines that are the entire substance
of two of the three checks. The pool would then be "judged" by checks that can no longer fail. My
fix is a 3-line key function.

**Why the keys live in the library, not the scenario.** The scenario compiles only under
`--features fdb`, so anything decided there is judged by nothing at Check — which is exactly how
this defect survived four iterations of green gates. The key assignment is now production
(default-compiled) and pinned by
`delete_pool_keys_are_single_writer_per_key_so_version_tags_track_commit_order`, which builds the
pool's real traffic **using the production key function** and drives the **production checks**. Make
`delete_pool_key` shared again and it goes red (verified below).

**Live confirmation:** the witnessed run's delete pool reported all three checks `true` over 480 ops.
v4's shape would very likely have reported a false violation and failed the run.

### 2. The composed final read silently omitted Unknown-probed members

**Re-verified against the real jar before fixing** (the fix's whole design depends on what elle-cli
actually does with an `:info` read, and that was not something to assume):

| history fed to `--model set` | real elle-cli 0.1.9 says | exit |
|---|---|---|
| add-only, **no** final read | `:unknown` | 0 |
| adds + `:info` read, `:value nil` | `:unknown` | 0 |
| adds + `:info` read, partial set value | `:unknown` | 0 |
| adds + `:ok` read missing an added element (the omission shape) | **`false`** | 1 |

So the v4 behaviour (drop the unknown member from a definite `:ok` read) really does manufacture a
`false` from an unanswered probe — and, critically, degrading to `:info` is **safe**: the checker
refuses to give a verdict rather than granting a vacuous pass. The honest outcome is enforced by the
*checker itself*, not merely by our gate. That is the property that made me choose degrade-to-`:info`
over "abort the run": it fails safe even if our gate were removed.

**The fix, in three places:**
- `DirFinalRead::unresolved` + `is_determinate()` (`crates/server/src/consistency_workload.rs:813,
  824`) — an unknown member is *recorded*, not dropped.
- `compose_final_read(process, probes, start, end)` (`:845`) — the classification decision moved
  **into the library** for the same reason as item 1: in the scenario it would be un-testable at
  Check. `Present`→set, `Absent`→genuinely excluded (a real lost element must stay visible to Elle —
  pinned by `a_fully_resolved_sweep_composes_a_definite_read_that_still_exposes_a_lost_element`),
  `Unknown`→unresolved.
- the serializer emits `:info`/`nil` for an indeterminate sweep (`:970`).

The scenario keeps only the I/O: `sweep_final_read` (`crates/server/tests/consistency_run_fdb.rs:632`)
re-probes an unknown member up to `FINAL_READ_PROBE_ATTEMPTS = 5` times (`:614`) with a 500ms
backoff, then hands the outcomes to the production composer. The sign-off allowed "re-probe, abort,
or degrade"; I did re-probe **then** degrade — abort alone would throw away a run that is still
perfectly informative about the register model.

**Quiesce added** (`:98`, `:229`): `QUIESCE_AFTER_HEAL = 10s` between `drive_leg` returning and the
sweep. Stated honestly in its doc comment: this buys a *conclusive* run, not a *sound* one —
soundness is the sweep's, whatever the wait is set to. A timing constant that silently carried the
correctness would be the same class of bug as the one it is next to.

**Beyond the fix — the degrade path is now a committed, checker-verified artifact**, not a claim in
a comment: `xtask/tests/fixtures/consistency-run/directory-history-indeterminate-final-read.edn` +
`checker-output-indeterminate-final-read.txt` are the real history and the **real jar's real
answer**. `SelfCheckExpectation::Inconclusive` (`xtask/src/consistency_run.rs:562`) adds it to the
off-Check self-check, so every live run re-confirms that *this* checker build refuses the degraded
shape before trusting any verdict from it. (Design §5's standard: the v2 gap was a partial
self-check; a new fixture outside it would repeat that.) The witnessed run's self-check exercised it
and passed.

**Gate + report.** `InconclusiveReason::FinalReadIndeterminate` (`:300`) and
`composed_final_read_determinate` on the seam (`:265`). I want to be precise about what this gate is
worth: it is **not** what makes the run safe — the checker already refuses. It buys the *diagnosis*
(which members), and the report says so rather than leaving an operator to reverse-engineer a bare
`:unknown` (`composed_final_read_field`, `:737`). I've said this in the code comments too, because a
gate that quietly takes credit for the checker's safety is how the next person over-trusts it.

### 3. `deny_unknown_fields` on the nested seam objects

`NemesisEvidence` (`xtask/src/consistency_run.rs:143`) and `OutcomeCounts` (`:170`). Serde applies
the attribute **per struct** — it is not inherited — so v4's claim that "the seam fails loudly" was
true only of the top level. Pinned by `an_unknown_field_nested_inside_the_seam_is_a_hard_error_too`
at three depths: inside `nemesis`, inside `register_outcomes`, and inside `delete_pool.outcomes`
(nested three deep). The test also asserts each JSON substitution *actually modified* the fixture, so
it cannot silently degrade into testing nothing if the anchor text drifts.

The revert run for this one is worth reading (recorded below): the summary parses **fine** with
`confirmed_at` swallowed — the defect demonstrating itself.

### 4. `unzip` in the external-dependencies list

Still **not registered**: `pdca.toml` has no `unzip` row, and the brief's `External dependencies`
line still names only docker/libfdb_c/fdb-headers/java/elle-cli. So the marker carries forward
(below). It is present on this host, so nothing was blocked and no evidence is missing.

### 5. Check-time test for the banded shape, and the ~2x `directory_ops` overstatement

The banded-shape test is in item 1. The overstatement: `directory_ops = creates.len() +
universe.len()` (pre-fix) counted every sweep probe as a history op, though the whole sweep enters
the EDN as **one** composed `:read`. Now `DirectoryHistory::op_count()`
(`crates/server/src/consistency_workload.rs:1002`) = creates + 1, consumed at
`crates/server/tests/consistency_run_fdb.rs:242`, pinned by
`directory_op_count_counts_the_composed_read_once_not_every_probe`. The witnessed report accordingly
says `directory (set): 121 ops` for 120 creates + 1 read; v4 would have claimed 240. The report also
now names what the probes *are* ("the sweep's per-member probes are that read's raw material, not
history ops") so the number can't be misread the other way.

## The witnessed run (the acceptance artifact)

Environment preflighted green (docker 29.6.1, libfdb_c loadable, fdb headers, java 25, the 0.1.9
jar, unzip), so per Design §6 this was mine to produce rather than defer.

```
WYRD_TIER1=1 WYRD_ELLE_CLI_JAR=… ./engine/xtask.sh consistency-run     → exit 0
```

Real results, from `target/consistency-run/`:
- **register (`--model rw-register`): `true`** — the real jar, on the real live history (240 EDN
  entries, 120 ops).
- **directory (`--model set`): `true`** — 120 creates + one determinate composed read of all 120.
- nemesis: `partition` on fdb0, `materialized: true`, typed evidence `peers_saw_target before=true
  during=false, target_running_during=true`.
- `genuinely_concurrent: true`; delete pool 480 ops, all three #406 checks held.
- fixtures self-check passed: both models, both polarities, **plus** the degraded shape.

Report committed at `docs/design/reviews/m4-checked-consistency-run.md` (precedent:
`m4-fdb-go-no-go.md`), with the emitted artifact embedded **byte-for-byte**.

**The finding I want the reviewer to look at hardest.** The run recorded `info: 0, fail: 0` across
all 720 ops — *the client never saw the fault*. That is not a bug and not a vacuous run (the
partition provably bit; the history is provably concurrent), it is FDB doing its job: partitioning
1 of 3 nodes leaves a 2/3 coordinator quorum and `double` replication, so the workload sailed
through. But it would be easy to read "PASS under partition" as more than it is, so the committed
report's **first** carve-out says exactly this in as many words, and the report keeps the per-op-kind
counts that reveal it. Design §4 asks for those counts so a *degenerate* workload is visible; here
they reveal the opposite, and it matters just as much. If the reviewer thinks the artifact still
overstates, that carve-out is where to push.

I considered forcing observable disruption (partition the majority / isolate the client). I did not:
it changes what the run *is*, the brief pins the partition leg and topology, and a quorum-killing
partition is a different experiment (#442's battery covers disruptive-fault classification). Better
to ship the honest artifact with the limit named than a louder one that answers a question nobody
asked.

## NEEDS-HUMAN external dependency

```
NEEDS-HUMAN external dependency: unzip — the off-Check runner reads the elle-cli version out of the jar (META-INF/maven/elle-cli/elle-cli/pom.properties) via `unzip -p`, because elle-cli 0.1.9 has NO --version flag (verified on this host: it prints `Unknown option: "--version"` and exits 0). Plan's External-dependencies list names docker/libfdb_c/fdb-headers/java/elle-cli but not unzip, and pdca.toml still has no row for it (the v4 carry-forward asked for one; it has not landed). It IS present on this host, so nothing was blocked and no evidence is missing — the witnessed run completed and read the checker's version from the jar — but the run hard-fails at preflight without it, so it belongs in the registered set rather than being discovered by a maintainer mid-run.
```

```toml
[[doctor.checks]]
group = "engine"
id    = "unzip"
cmd   = "unzip -v"
hint  = "apt-get install unzip — the #408 off-Check runner reads the elle-cli version/revision out of the jar's own META-INF metadata (elle-cli has no --version flag), so the run report can name the checker that produced the verdict; not needed for `cargo xtask ci`"
level = "WARN"
```

`WARN`, not `MISSING`: like `java`/`elle-cli` it is only needed for the opt-in privileged run;
Check-core red→green needs only the base Rust toolchain.

## The three forced questions

**(a) Genuine red?** Yes — five behavioural reverts (behaviour only, API kept), each re-run through
the project's own toolchain. Every one went red, and each red was *targeted*: the other tests in the
same file stayed green, so these are not blanket compile failures.

| Fix reverted (to the v4 defect shape) | Test | Result |
|---|---|---|
| `delete_pool_key` → one shared key | `delete_pool_keys_are_single_writer_per_key_so_version_tags_track_commit_order` | **FAILED** — `left: "checked-delete-register", right: "checked-delete-register"` |
| `compose_final_read` → drop Unknown probes | `an_unresolved_probe_degrades_the_composed_read_to_info_rather_than_omitting_the_member` | **FAILED** — `left: [], right: [2]` (the member vanished, exactly as in v4) |
| nested `deny_unknown_fields` removed | `an_unknown_field_nested_inside_the_seam_is_a_hard_error_too` | **FAILED** — `unwrap_err()` on an **`Ok`**: the summary parsed happily with `confirmed_at` silently swallowed |
| `op_count` → count every probe | `directory_op_count_counts_the_composed_read_once_not_every_probe` | **FAILED** — `left: 4, right: 3` (the 2x overstatement, in miniature) |
| final-read gate removed from `evaluate_summary` | `a_summary_whose_composed_final_read_is_indeterminate_is_inconclusive_and_names_the_members` | **FAILED** — `left: Ok(()), right: Err(FinalReadIndeterminate)` |

Restored → `xtask ci: all checks passed`; `cargo check -p wyrd-server --features fdb --all-targets`
clean.

The **C4-verify** red is also confirmed structurally, not assumed: `xtask::consistency_run` is
**absent** on `origin/main` (`git cat-file -e origin/main:xtask/src/consistency_run.rs` → absent), and
I simulated the gate in a throwaway `origin/main` worktree — patch applied, production module
removed, named test kept → `error[E0432]: unresolved import xtask::consistency_run`. The kept test
cannot pass pre-fix.

**(b) Production path?** Yes, at every level that matters here.
- The server-side tests drive the **real** production functions the live scenario calls —
  `delete_pool_key`, `compose_final_read`, `DirectoryHistory::to_elle_edn`, `op_count`, and the
  landed #406 checks themselves. Not copies: the scenario imports the same symbols
  (`consistency_run_fdb.rs:529, 632, 242`). This is the deliberate structural change of this
  iteration — the two defects lived in decisions the scenario made *privately*, which is why four
  green gates never saw them. The decisions now live in default-compiled production code, so the
  gate reaches them.
- The xtask tests import `xtask::consistency_run` — the module the runner itself calls.
- The verdict parser is fed **real captured checker output**; the version parser real jar bytes.
- And the strongest form: the whole pipeline **actually ran** against a live 3-node FDB cluster with
  a real iptables partition, and a real JVM Elle judged the real histories.

**(c) Fixture includes the fault?** Yes.
- The delete-pool test's fixture *is* the failing shape: I did not curate the banded history out — I
  committed it (`a_shared_delete_pool_key_with_version_bands_would_fabricate_a_violation`) and it
  passes by **exhibiting** the fabrication through the production checks.
- The composed-read test's fixture **contains** the 503-probed member (and a 404 one, so the test
  can't pass by treating everything as unknown); the pre-fix run proves nothing was curated out —
  it fails at `[] != [2]`.
- The degraded-EDN fixture is checked by the **real jar**, and its committed answer is the jar's, not
  mine.
- The witnessed run's fault was genuinely injected (`iptables` DROP on a real container) and its
  materialization is attested by #407's own oracle sampling the *actual* reachability flip — not by
  the scenario asserting `drive_leg`'s contract.

One honest limit on (c), stated rather than buried: the witnessed run's fixture includes the fault
but the **client** didn't feel it (see the carve-out above). The nemesis is real; its blast radius
did not reach the workload.

## Scope discipline

- #406's checks (`is_genuinely_concurrent`, the session/monotonicity checks) are **untouched** — item
  1 was fixed by changing the pool's keys, which is what let them stay untouched.
- #407's nemesis seam consumed as-is; no lifecycle logic reopened.
- The only production behaviour that changed is the #408 pipeline's own; no runtime crate's behaviour
  changes. `cargo xtask ci` stays unprivileged, container-free, JVM-free.
- `docs/design/reviews/` is a review document, not an ADR/spec — no immutability concern.

## Gates

`./engine/xtask.sh ci` (the project's single-sourced gate: fmt, clippy `-D warnings`, build, test,
`cargo deny`, conformance) — **green**, including with the committed report present.
`cargo check -p wyrd-server --features fdb --all-targets` — clean. `cargo fmt --all` run over every
touched file; the gate's `cargo fmt --check` leg caught two spots before this note was written, which
is precisely the class of thing that fails mid-publish otherwise.

## STOP

Nothing pushed; no PR opened or marked ready. The witnessed report §6 gates the ready-mark on is
committed in the patch, so the human's sign-off is unblocked on that count — but the ready-mark
remains theirs.
