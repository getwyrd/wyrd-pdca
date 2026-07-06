# Adversarial review — issue 366 (obs-floor keystone: counter→gauge + `CustodianRole`)

Ran the asserted evidence at `$PDCA_TARGET`: the diff is applied to the worktree;
`cargo test -p wyrd-custodian --test reconstruction_telemetry` → **green** (1 passed).
Confirmed the gauge mechanism is real: `tracing-opentelemetry 0.33` maps the `gauge.`
prefix to a sync `Gauge::record` (metrics.rs:24,176,205,226), and `opentelemetry-prometheus
0.32` appends `_total` only to *monotonic* sums (lib.rs:122-124,196). So the fix is
mechanically sound. Findings below are the places it is thinner than the verdict implies.

## Findings

- **NEEDS-HUMAN — the binding "day-one" gauge reads `0` during the *worst* durability loss.**
  `crates/custodian/src/reconstruction.rs:170` emits `emit_under_replicated(plans.len())`,
  and `plans` holds only `Assessment::Repairable` chunks; a chunk that has lost more than
  `m` fragments is `Assessment::Unrepairable` and the arm is a no-op
  (`reconstruction.rs:145` `=> {}`) that emits nothing. **Concrete failing case:** kill 2 of
  the 3 fragments of the test's RS(2,1) chunk (survivors = 1 < k = 2). The chunk is on the
  brink of *permanent* loss, yet `emit_under_replicated(0)` fires and
  `reconstruction_under_replicated` reads **0** off `gather_prometheus`. This diff is what
  *promotes* this metric to "the day-one durability signal", so it now owns the semantics
  "gauge = 0 ⇒ zone healthy" — which is false for below-k chunks. The single test only
  exercises the recoverable (1-of-3-lost) case, so this inversion is never seen. Whether the
  floor's binding signal may silently read healthy while data is being lost is a human call.

- **The red→green discriminates for a *different* reason than the patch documents.**
  The code comment (`reconstruction.rs:510-513`) and the test docstring
  (`crates/custodian/tests/reconstruction_telemetry.rs:198-203`) claim a monotonic counter
  "reads back 1 then stays pinned at 1", making the pass-2 *returns-to-zero* assertion RED.
  That is not what happens: a counter is exported as `reconstruction_under_replicated_total`
  (opentelemetry-prometheus lib.rs:196), and `gauge_value()` queries the un-suffixed name, so
  a counter is simply **absent** → the *pass-1* `Some(1.0)` assertion
  (`reconstruction_telemetry.rs:536`) fails first; the test never reaches the pass-2 leg the
  narrative rests on. The fix works and the emit-line-level red is genuine, but the reviewer
  should not credit the elaborate "accumulating counter" causal story — it describes a path
  the assertion never walks.

- **The DST change is cosmetic and carries zero independent evidence.**
  `crates/dst/tests/custodian.rs:1023,1046` only renames the capture key
  `monotonic_counter.` → `gauge.`. `MetricCapture.values()` reads the *raw per-event tracing
  field value* (`count as u64` = 1 then 0), which is identical for a counter or a gauge — it
  is exactly the "bespoke per-event capture layer" the new test's own docstring disparages
  (`reconstruction_telemetry.rs:182-189`). It would stay green under either instrument type
  as long as the key matches. So the entire red→green rests on the *single* new test; there
  is no second, independent oracle for the gauge behaviour.

- **NEEDS-HUMAN — "the wired, runnable custodian role (not the library alone)" is met by
  relabeling, not by a runnable process.** `CustodianRole` is constructed **nowhere** but the
  test (`grep`: only `crates/custodian/src/lib.rs:44` re-exports it); the `server` crate still
  has no `custodian` dependency and `custodian` remains a `dst`-only dep
  (`crates/dst/Cargo.toml:44`, unchanged). `CustodianRole` (`crates/custodian/src/role.rs`)
  is a library struct whose sole added behaviour over calling `reconcile_step` by hand is
  owning a `Dispatch` and wrapping one pass in `.with_subscriber(...)` (`role.rs:158-168`);
  there is no run loop, leadership lifecycle, or binary. This is consistent with the brief's
  "keystone slice, deployment half deferred" disposition, but the brief's binding phrasing
  "not the library alone" is satisfied nominally — the human should confirm the keystone is
  accepted on that basis and that item 2's deployment half is booked as a follow-on slice.

## Attempted refutations that FAILED (fix survives)

- *Scoped subscriber loses emissions from spawned children.* `with_subscriber` only covers
  the wrapped future, so a `tokio::spawn` inside a loop would emit into the global no-op
  dispatcher. Grepped `crates/custodian/src` — **no** `spawn`/`join!`/`block_in_place`; every
  loop is sequential `.await`. Holds for this diff (but is a latent trap for the deferred
  continuous run loop / any future loop that spawns — worth a comment there).
- *Gauge doesn't actually return to zero through the pull exporter.* Verified green: the
  `Dispatch` (and its `Instruments`/gauge) is built once in `role.rs:120-121` and shared, so
  pass-2's `record(0)` overwrites the single (label-less) series; `flush()` + `gather()` reads
  0. Confirmed empirically.
- *Per-chunk label leaves a stale `1` series after repair.* `emit_under_replicated` carries no
  attributes (`reconstruction.rs:515-516`), so both passes hit the same series — no staleness.

## Caveat on the red→green gate

`run-verify.sh` and `build-notes.md` are withheld here, so I could not see *which* lines the
harness reverted to establish "red". If it reverted the whole diff, the red is a
*compile* failure (missing `CustodianRole`/`gauge.`), which proves the API is new, not that
the assertion catches the defect. I independently established the stronger claim — reverting
*only* the `emit_under_replicated` line to `monotonic_counter.` yields an *assertion* red
(the `_total`-suffix name miss → pass-1 `Some(1.0)` fails). A human confirming the gate should
ensure the red was taken at that emit-line level, not at compile time.
