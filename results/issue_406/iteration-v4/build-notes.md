# Build notes — issue 406 / consistency-workload-history-and-elle-serialization

*Withheld from the reviewer. Rationale for the human at sign-off.*

## What the brief asked for (net-new, ADR-0041 slice 3)

Build, on the merged #405 observable, the five deliverables ADR-0041 §Decision grants the
Rust slice — **and nothing that re-derives a global linearizability verdict in-gate** (the
rejected v1–v3 vehicle). The Rust slice's job is *history production + serialization + sound,
local checks*; Elle owns the register/namespace linearizability *verdict*, off-Check.

## Where the change lives

Everything is one cohesive module in the **production** crate `wyrd-server`, alongside the
merged #405 `consistency_observable.rs` (same established pattern: #405 put the observable
client in `crates/server/src/`, not a test-only crate). Cited on
`feat/m4-production-metadata-backend` (worktree tip `a7c7408`, #405 landed `8af8e97`):

- `crates/server/src/consistency_workload.rs` (**new**) — the whole slice:
  - `MultiProcessHistory` (merge + `:process` tag + concurrency/climb witnesses),
  - `to_elle_edn` (register EDN serializer),
  - `session_read_your_writes` / `session_monotonic_reads` (ADR-0015 guarantee 3),
  - `DirectoryHistory` + `directory_to_elle_edn` (ADR-0041 decision 2, set op form),
  - `VerdictDispatch` / `verdict_dispatch` (the off-Check routing seam),
  - `run_concurrent_register_workload` / `run_directory_workload` (wire drivers).
- `crates/server/src/consistency_observable.rs:88-97` — added `History::from_ops`, a minimal
  sound construction seam so the merge and the **socket-free** crafted-history reds can feed
  the *sound, local* checks (e.g. the reused `versions_monotone_per_key`) with no listener.
  The #405 client only grows a history over the wire; there was no way to inject a crafted op.
- `crates/server/src/lib.rs:18` — `pub mod consistency_workload;`.
- `crates/server/tests/consistency_workload.rs` (**new**) — the named, load-bearing regression.

### Why the verdict-dispatch is in `wyrd-server`, not physically in `xtask`

The brief says to *mirror* `xtask/src/metadata_faults.rs` (the `MetadataTierDispatch` enum +
`metadata_tier_dispatch` pure fn) — i.e. mirror the **shape** ("deferred ≠ unbuilt" pure
routing value with both alternatives representable). It does **not** require the value to live
in `xtask`. The named test file is `crates/server/tests/consistency_workload.rs`, and an
integration test in `crates/server` cannot reach an `xtask` lib symbol. Physically splitting
the verdict test into `xtask/tests/` would contradict the brief's "[the named test] carries …
verdict-dispatch routing". A parallel copy in `xtask` would be dead code (nothing in `ci`
routes an Elle verdict yet — that job is the deferred off-Check slice). So the routing value
lives with the history it routes, in `wyrd-server`, exercised by the named test. Shape mirrors
`metadata_faults.rs` exactly: `OffCheckJob` (default) vs `InGateShellOut` (representable, never
selected for default inputs, a hard error downstream).

## The one non-obvious design decision (and why it matters)

**Monotonicity is checked PER-PROCESS (per session), not across the merged history.** My first
cut asserted `versions_monotone_per_key()` on the *merged, start-sorted* concurrent history and
it went **red** in the wire leg — correctly. A PUT's version becomes visible only at its
**commit** (span *end*), not its *start*; ordering the merged history by start puts a writer's
higher version *before* a concurrent reader that legitimately read the older (pre-commit) value,
so a start-ordered global monotonicity check produces false "regressions". More importantly, a
**global cross-process register-monotonicity decision IS the linearizability verdict ADR-0041
reserves for Elle, off-Check** — asserting it in-gate is precisely the rejected v1–v3 vehicle.

So I removed the merged-history global check and assert the **sound, local** invariant instead:
`per_process_reads_monotone()` — each session's own reads are monotone (a single client's ops
are sequential, so its read order *is* real-time order, and with a single climbing writer every
reader observes a non-decreasing value). This is green *by construction*, not by luck, and it
keeps the slice strictly on the Rust side of ADR-0041's division of labour. The global verdict
is Elle's, over the SAME serialized history, off-Check. (`consistency_workload.rs:150-172`.)

The reused #405 `versions_monotone_per_key` is still exercised — but where it is **sound**: on a
single crafted (single-session) history in the socket-free red
(`reused_per_key_monotonicity_rejects_a_version_regression`).

## Serializer shape (register + set)

Register EDN emits exactly the brief's fields, in order:
`{:process P, :type :invoke|:ok|:fail, :f :read|:write|:delete, :value V, :time N}`, one op
split into an invoke (at start) + a completion (at end), the whole stream ordered by real time
(`:time` is a **relative** nanosecond offset from the history origin — the Jepsen convention).
`:fail` is the mapping for a non-delivered op (a 5xx write; a GET 404 is a legitimate *absent*
read → `:ok` with `:value nil`). The workload uses a **single shared key** (criterion (a)), so
`:value` is the scalar register version and the 5-field schema is complete (no `:key` needed).
Directory uses the same machinery with `:f :add|:remove|:contains?` and a `[member present?]`
read value — the "set op form".

**Scope honesty:** the golden asserts the serializer is *stable and well-shaped* (byte-exact),
NOT that real Elle parses it — Elle is JVM/Clojure and stays off-Check, so real-parser
acceptance is part of the deferred verdict leg (the brief's Scope note; do not over-read the
golden as an Elle-compat proof).

## Concurrency is real, not simulated

`run_concurrent_register_workload` spawns 1 writer (process 0) + N readers (processes 1..=N) as
**separate tokio tasks on a multi-thread runtime**, each an `ObservableS3Client` opening its own
TCP connection to the real in-process `S3Gateway` (`axum::serve`, per-connection tasks,
`crates/gateway-s3/src/lib.rs:158`). Readers spin GETs until the writer signals `done`, so their
read spans overlap the writer's overwrite spans in real time. The writer overwrites with
strictly-climbing versions committing sequentially, so the register only climbs (sound
per-session monotonicity + a climbing `version_climbs_for_key` witness). A saturating reader cap
prevents a runaway loop if the writer errs. Verified non-flaky: the wire leg passed 8/8 repeats.

## Verification — red→green, proven (the three forced questions)

Runner: the project's `cargo` (the same one `./engine/xtask.sh ci` wraps), scoped to the named
test; plus fmt `--check` and `clippy --all-targets` (default features — the exact flags
`cargo xtask ci` uses, `xtask/src/main.rs:1082`). Full `cargo xtask ci` also run as the
authoritative gate.

- **(a) Genuine red?** YES, two independent demonstrations:
  1. *Red of record* — with the module absent (pre-fix), the test fails to compile:
     `error[E0432]: unresolved import wyrd_server::consistency_workload`.
  2. *Module-weakening reds* (the brief's flippable bar) — I weakened five things and each
     produced exactly its targeted red, the two un-weakened tests staying green:
     `:write`→`:put` reds the register golden; `:add`→`:insert` reds the directory golden +
     the wire directory leg; neutering the RYW violation branch reds the RYW reject;
     flipping the dispatch default reds both dispatch tests; `spans_overlap`→`false` reds the
     concurrent-overlap witness. Restored → 9/9 green.
- **(b) Production path?** YES. The wire legs drive the **production** `ObservableS3Client`
  (real signed SigV4 HTTP, real overwriting commits that bump the register, real reads) against
  the real `wyrd_server::Gateway` + `S3Gateway` HTTP wire — no mock, no re-implementation. The
  socket-free legs feed the **production** serializer / session checks / dispatch (the same
  functions the wire path uses) with crafted inputs — not a copy.
- **(c) Fixture includes the fault?** YES. The concurrency fixture is the real gateway with
  genuinely concurrent clients producing actually-overlapping spans (asserted, not curated in).
  The crafted-history reds each *include the violating element* (the stale read, the version
  regression, the in-gate-shellout input), so the check's rejection is a real observation.

Result: `9 passed; 0 failed` (named test); full `wyrd-server` suite green (incl. the existing
#405 `consistency_observable` and `s3_http_wire` tests); fmt clean; clippy clean.

## Sandbox caveat (pre-declared, not a surprise)

The load-bearing reds — serializer goldens, session RYW/monotonic-read rejection, per-key
monotonicity rejection, verdict-dispatch routing, directory serialization — are **socket-free**,
so a flippable RED is producible even where the Check sandbox denies loopback `bind`. Leg (a)/(d)
wire greens need loopback bind; confirmed here and by the full `cargo xtask ci` (which permits
it, as it does for #405's merged wire test). This matches the brief's DEFERRED/net-new posture.

## What is deferred off-Check (unchanged from the brief)

The live Elle/JVM verdict over the serialized EDN, and the real-cluster partition nemesis (#407),
are later slices that *consume* this history. `verdict_dispatch` is the built seam that routes the
verdict there; this in-gate slice returns no register/namespace linearizability verdict.

No external dependency beyond what #405's merged wire test already uses (loopback TCP + in-process
gateway) — no NEEDS-HUMAN external-dependency blocker.
