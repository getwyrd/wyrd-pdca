# Build notes — issue 408, iteration v4

Target branch: `getwyrd/wyrd @ main` (base `e0e39c1`). Worktree: `$PDCA_WORKTREE`
(`/home/eddie/development/wyrd/wyrd.pdca-wt`). All `path:line` citations below are against that
worktree at the patched state unless marked "(pre-fix)".

## What this iteration is

The Iteration-3 carry-forward accepted the pure xtask core ("the parser, gate, report renderer,
fixtures held under adversarial re-verification and stays") and named **four defects in the live
scenario/report path** to fix before the witnessed run is attempted. It also says, explicitly:

> The privileged witnessed run (`WYRD_TIER1=1 cargo xtask consistency-run` on the live FDB
> topology) and the committed report under `docs/design/reviews/` — the brief's acceptance
> artifact — will be produced in the NEXT iteration, after these fixes land; **do not attempt the
> live leg before they do**.

So this iteration deliberately ships **no witnessed run and no committed report**. That is not an
omission I am glossing: the brief's Success criterion is not fully met until that report exists,
and §6/sign-off still gates the ready-mark on it. I started from `iteration-v3/patch.diff` (applied
cleanly onto `origin/main`) and fixed the four items.

## The four carry-forward items

### 1 + 2 + 3 have ONE root cause — fixed at the source, not at the callsites

The carry-forward lists three separate scenario defects:

- **(2) Stale-status fabrication** — `consistency_run_fdb.rs:437-439,478-479` (pre-fix) read the
  status of the op just driven off `c.history().ops().last() ... unwrap_or(0)`.
- **(3) Dropped errored register ops** — `consistency_run_fdb.rs:360,371` (pre-fix)
  `let _ = c.put(...)` / `let _ = c.get(...)` silently omitted indeterminate ops.

Both are symptoms of the **production client**: `ObservableS3Client::put/get/delete`
(`crates/server/src/consistency_observable.rs`, pre-fix `:157,:176,:199` at base `e0e39c1`) used
`?` on `send()`, so
a transport error returned `Err` and **pushed nothing into the history**. Consequences:

- the op vanishes (defect 3) — and it is precisely the ops that *raced the nemesis* that vanish, so
  the checker is handed a history that reads like an unremarkable clean run;
- with the op missing, `ops().last()` returns the **previous** op — so the directory create /
  final-read probe inherits a neighbour's determinate `200` (defect 2), which serializes an
  indeterminate op as a definite `:ok`. That is INV-1's exact prohibition.

The module's own INV-1 doc already names a **"synthetic-0 status the client stamps on a timeout"**
(`crates/server/src/consistency_workload.rs:58-64`, `is_indeterminate(0) == true`) — a convention
the client never actually implemented. So the invariant to restore ("no fabricated certainty", and
its dual: every invoked op is in the history) was already written down; the client just didn't hold
it.

**Fix (at the source):**

- `crates/server/src/consistency_observable.rs:62` — `pub const INDETERMINATE_STATUS: u16 = 0`,
  the convention `consistency_workload` already assumes.
- `:249,:285,:330` — `put`/`get`/`delete` now record **every** invoked op, stamping
  `INDETERMINATE_STATUS` when the round trip produced no status, and return the recorded
  `OpRecord`.
- `:74` — the error type is now `OpFailed { record, cause }`: the record travels **in the error**,
  so a driver that treats a transport failure as data gets the very record that was recorded,
  without re-reading the history's tail, and a caller on a healthy wire still gets a loud failure.
  This is why the scenario callsites are now `.unwrap_or_else(OpFailed::into_record)`
  (`crates/server/tests/consistency_run_fdb.rs:562,604`) with **no `.last()` anywhere** — the
  flagged shape is gone structurally, not merely made sound.
- A **torn read** (200 whose body doesn't decode) also records as indeterminate rather than as a
  definite 200-of-`nil`: `nil` would claim the register was *unwritten*, a fabrication.
- An indeterminate PUT keeps `version: Some(v)` — the version it *attempted*. That is what the
  `:invoke` micro-op states (`[:w key v]`), and `register_write_microop`
  (`crates/server/src/consistency_workload.rs:643`) **panics** on a `None` version (a nil-write is
  checker-rejected). Recording `None` would therefore panic the serializer on the register pool
  during a partition — i.e. `Some(v)` is forced, not preferred.

**Soundness consequence I had to handle (and did not hide):** because indeterminate PUTs now appear
in the history carrying `Some(v)`, `History::versions_monotone_per_key`
(`crates/server/src/consistency_observable.rs:180`) would count a *may-never-have-committed* write
as an observed version — so a later, entirely correct read of the earlier version would read as a
regression: a **fabricated violation**. INV-1 forbids deriving a definite claim from an
indeterminate op in *either* direction, so I added the skip guard
(`crates/server/src/consistency_observable.rs:184-186`).

Scope note: the brief puts "the #406 *checks*" out of scope (`is_genuinely_concurrent`, the session
/monotonicity checks in `consistency_workload.rs`). `versions_monotone_per_key` is **#405's**, in
`consistency_observable.rs`, and is not one of the named-untouched functions. I judged the guard
in-scope because *my* change is what makes indeterminate ops reachable by it — shipping a check
that my change newly makes unsound would be worse than the 3-line guard. It only ever makes the
check *more conservative* (fewer violations), and the unit test pins that it is scoped to
indeterminacy: a **determinate** regression is still a violation
(`crates/server/src/consistency_observable.rs:537-582`). Its one consumer is a #405 test
(`crates/server/tests/consistency_observable.rs:190`), not the #408 pipeline, so no live behaviour
changes.

Adjacent issue observed, **deliberately not fixed**: `versions_monotone_per_key` has a
*pre-existing* hole for 5xx ops (a 500 PUT is already recorded with `Some(version)` and is already
counted) — it predates this patch (`send()` returns `Ok((500, _))`; a 5xx is a successful round
trip), is #405's, and no #408 code path calls it. Widening scope to re-decide a landed check on a
path this issue doesn't use is exactly the drift the brief's scope line guards against. Flagging it
here rather than silently fixing or silently ignoring it.

### 1. The missing Wyrd-checked DELETE pool

`drive_pools` drove only the overwrite and directory-create pools. Added the third pool Design §2
names:

- `crates/server/tests/consistency_run_fdb.rs:476` — `spawn_delete_pool_process`: 2 processes
  driving PUT → GET → DELETE → GET on `DELETE_POOL_KEY` (`:469`), **disjoint** from the Elle-fed
  `REGISTER_KEY` (`:462`). Disjointness is what makes the exclusion sound (Elle partitions per key,
  so a whole key never serialized fabricates no order; filtering ops out of a *serialized* key's
  history would). Each process writes from its own version band so a read names its writer.
- `crates/server/tests/consistency_run_fdb.rs:226-230` — judged by the landed #406 checks:
  `session_read_your_writes` (which carries the resurrection / lost-write logic),
  `session_monotonic_reads`, `reads_monotone_per_key`. Counts + verdicts go into the summary
  (`:269-273`).
- It is never serialized into the register EDN (`to_elle_edn` is only called on `register`,
  `:234`), which is the whole reason the pool can carry deletes at all.

**A pool that is driven but not acted on is decorative**, so the verdict is wired to a run outcome:
`wyrd_check_violations` (`xtask/src/consistency_run.rs:323`) names every violated check, and the
runner **fails the run** on it (`xtask/src/consistency_run_runner.rs:537-545`) exactly as it does
on Elle's `false`. It is deliberately *orthogonal* to the vacuity gate: a violated Wyrd check means
the run learned something definite and **bad** (a FAILURE), not that it learned nothing
(inconclusive) — pinned at `xtask/tests/consistency_run_orchestration.rs`, `the_delete_pools_violation_is_independent_of_the_vacuity_gate`.

### 4. Report conformance — elle-cli version + the member-id map

**Version.** I did **not** guess the version flag: I ran the real jar on this host.
`java -jar elle-cli-0.1.9-standalone.jar --version` prints `Unknown option: "--version"` **and
exits 0** — elle-cli 0.1.9 has no version flag (its `--help` confirms: only `-m/-f/-v/-h/-c/-a/-s`).
Keying the report on it would have recorded an *error string* as the checker's version — the same
"exit 0 lies" shape as the `:unknown` verdict in Design §3. So the version is read from the jar's
own metadata (`META-INF/maven/elle-cli/elle-cli/pom.properties`), which the real jar carries:

```
version=0.1.9
revision=6d4afc4c5f794e8cb038bb33de465f66cb21f3a4
```

That is strictly better than a filename (a rename would lie) and gives the **upstream source
revision** too. `xtask/src/consistency_run.rs:372` (`ELLE_VERSION_JAR_ENTRY`), `:405`
(`elle_version_extraction`), `:421` (`parse_elle_version`), `:390` (`CheckerIdentity::describe`).
A jar with no `version` key is a **hard error**, not a `"version: unknown"` placeholder — a report
naming an unidentifiable checker is not a credibility artifact. The unit test parses the **real
captured bytes** (`xtask/tests/consistency_run_orchestration.rs`, `the_real_jars_pom_properties_parses_to_its_version_and_revision`).

**Member-id map.** The scenario always emitted it (`consistency_run_fdb.rs:274`); `RunSummary` had
no such field, so **serde silently discarded it** and the report could never show it. Added
`member_id_map: Vec<MemberId>` + `composed_final_read` (`xtask/src/consistency_run.rs:177,239,241`)
and `ReportInputs::member_id_map_field` (`:659`). Without it the `set` history's integer elements
resolve to nothing and the artifact can't be tied back to the objects the run created.

**And the class of bug, not just the instance:** `#[serde(deny_unknown_fields)]` on `RunSummary`
(`xtask/src/consistency_run.rs:228`). Serde's default — ignore unknown fields — *is* the silent
data loss that hid this for three iterations: the scenario emits, the runner drops, nothing fails.
The seam now breaks loudly when the two sides drift (`consistency_run_orchestration.rs`, `a_field_the_scenario_emits_but_the_seam_does_not_name_is_a_hard_error`).

The report grew two fields beyond the issue's five (which all remain):
`- **Checker:**` and `- **Member-id map:**` (`xtask/src/consistency_run.rs`, `render_report`). `unzip` is
added to the preflight (`Environment`, `:559`) so an opted-in run learns it can't identify the
checker **before** standing up a cluster and burning a nemesis window, not after
(`consistency_run_runner.rs:484,491`).

## NEEDS-HUMAN external dependency

```
NEEDS-HUMAN external dependency: unzip — the off-Check runner reads the elle-cli version out of the jar (META-INF/maven/elle-cli/elle-cli/pom.properties) via `unzip -p`, because elle-cli 0.1.9 has NO --version flag (verified on this host: it prints `Unknown option: "--version"` and exits 0). Plan's External-dependencies list names docker/libfdb_c/fdb-headers/java/elle-cli but not unzip. It IS present on this host (/usr/bin/unzip), so nothing was blocked and no evidence is missing — but the witnessed live run now hard-fails at preflight without it, so it belongs in the registered set rather than being discovered by a maintainer mid-run.
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

I considered avoiding the dependency: a Rust `zip` crate would add a dependency to `xtask` (which
today has exactly three: `wyrd-chunk-format`, `serde`, `serde_json`) plus a `cargo deny` review, to
read one 142-byte properties file in an off-Check code path — worse than reusing a tool the runner's
host already needs alongside `docker`/`java`, and the runner already shells out to `sha256sum`,
`docker`, `cargo` and `java`. Deriving the version from the jar's *filename* needs no tool but is
not evidence: a renamed or rebuilt jar would report a version it isn't.

## The three forced questions

**(a) Genuine red?** Yes — behavioural (assertion) reds, not compile errors. Each fix was reverted
*in its behaviour only*, keeping the API, and the test re-run through the project's toolchain:

| Fix reverted | Test | Result |
|---|---|---|
| record-on-error (build the record, don't push it) | `a_transport_failure_records_the_op_as_indeterminate_rather_than_omitting_it` | **FAILED**: `left: 1, right: 4` — the dump shows the lone survivor is the determinate `status: 200` op, i.e. exactly the stale status a `.last()` read would have inherited |
| the `versions_monotone_per_key` indeterminacy guard | `an_indeterminate_write_is_not_counted_as_an_observed_version` | **FAILED** (`consistency_observable.rs:560`) |
| `#[serde(deny_unknown_fields)]` on `RunSummary` | `a_field_the_scenario_emits_but_the_seam_does_not_name_is_a_hard_error` | **FAILED** (`consistency_run_orchestration.rs:346`) |

Restored → all green (`xtask ci: all checks passed`).

The member-id-map / checker-version / delete-pool tests are red pre-fix only at **compile** level
(the fields/functions don't exist in v3), which is inherent to a fix that adds a seam field — I note
it rather than dress it up as a behavioural red.

**One test I wrote failed this question and I deleted it.** My first
`an_indeterminate_write_never_fabricates_a_stale_read_violation` (integration, over the killed
loopback gateway) **passed with the guard reverted** — it drove put(v1)→put(v2, indeterminate), and
1→2 never regresses, so it exercised nothing. Exhibiting the regression needs a *determinate read
after* an indeterminate write, which a dead-peer loopback cannot stage (once the peer is gone, no
read completes determinately). Rather than fake a peer that resurrects, I moved it to a unit test
that constructs the history and drives the same production predicate
(`crates/server/src/consistency_observable.rs:537`), and left a comment at the integration site
saying why (`crates/server/tests/consistency_observable.rs:282-289`).

**(b) Production path?** Yes. `a_transport_failure_...` drives the real `ObservableS3Client` — the
same type, same methods the live scenario calls — over a **real** loopback S3 gateway (redb + fs +
mem behind the production `S3Gateway`, the composition `s3_http_wire.rs` uses), with real signed
HTTP round trips. No mock client, no re-implementation. The xtask tests import `xtask::consistency_run`
— the very module the runner calls (`consistency_run_runner.rs`), not a copy. The version parser is
fed the **real jar's real bytes**.

**(c) Fixture includes the fault?** Yes — the fault is *injected*, not curated out. The gateway is
genuinely killed (`stop_gateway`, `crates/server/tests/consistency_observable.rs:73`, which aborts the serving task and
**polls until the port actually refuses connections** rather than assuming), and the test asserts
the ops genuinely failed (`put_err.is_err() && get_err.is_err() && delete_err.is_err()`,
`crates/server/tests/consistency_observable.rs:226`) — so it cannot pass by never exercising the indeterminate path at all. The history it
then asserts over is the one *containing* the broken ops; the pre-fix run proves the fixture
excludes nothing (it fails at `1 != 4`).

Caveat, stated plainly: a killed loopback listener is not an `iptables -j DROP` on a live FDB
coordinator. It is the same *client-side* fact (peer gone → no status → indeterminate), which is
what the client's recording contract turns on, and that contract is what the four defects were
about. The **live** leg — real cluster, real partition, real Elle verdicts — remains deferred to
the next iteration by the carry-forward's own instruction.

## What is NOT done (honest status)

- **No witnessed run, no committed report under `docs/design/reviews/`.** Per the carry-forward.
  The brief's Success criterion (e) and §6 are therefore not yet satisfied, and the PR must not be
  marked ready. The environment is present on this host (docker, java 25, the 0.1.9 jar, unzip), so
  the next iteration can run it.
- The live scenario's own code is compiled (`cargo check -p wyrd-server --features fdb
  --all-targets` passes) but not executed — it is `#[ignore]`d + env-gated + `fdb`-gated by design.
  Its decision logic is exercised at Check through the summary seam, which is what the seam is for.

## Gates

`./engine/xtask.sh ci` (the project's own single-sourced gate: fmt, clippy `-D warnings`, build,
test, `cargo deny`, conformance) — **green**. `cargo fmt --all` was run over every touched file, so
the target's own commit hooks should pass; the gate's `cargo fmt --check` leg caught two spots
before this note was written, which is exactly the class of thing that fails mid-publish otherwise.
