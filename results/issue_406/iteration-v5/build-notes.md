# Build notes — issue 406 / consistency-workload-history-and-elle-serialization (iteration 5)

*Withheld from the reviewer. Rationale for the human at sign-off.*

## What this iteration changes vs. v4

v4 built the whole slice and cleared C4/T3, but sign-off withheld accept on **fitness-to-
purpose**: three tightenings. This iteration is a **re-do of the whole patch** (the worktree
is pristine at `a7c7408` — the v4 patch was never committed) with those three fixes folded
in. Nothing else about the v4 approach was rejected, so the structure (module in `wyrd-server`
alongside the merged #405 `consistency_observable.rs`; named integration test
`crates/server/tests/consistency_workload.rs`) is carried forward unchanged. Citations are on
`feat/m4-production-metadata-backend`, worktree tip `a7c7408` (#405 landed `8af8e97`).

### Fix 1 — the concurrency witness must require a read↔write overlap (carry-forward §1)

**Was:** `is_genuinely_concurrent()` = `overlapping_pairs_across_processes() >= 1`, which
counts *any* cross-process span overlap — including read↔read. Two concurrent reads impose no
ordering constraint on each other, so a read↔read-only history is vacuous for register
linearizability yet passed as "genuinely concurrent."

**Now** (`consistency_workload.rs:165-190`): added
`read_write_overlapping_pairs_across_processes()` — counts only pairs where one span is a GET
and the other a mutation (PUT/DELETE), across distinct processes — and
`is_genuinely_concurrent()` now requires `>= 1` of *those*. The raw
`overlapping_pairs_across_processes()` is kept (documented as the raw counter) so the
regression can prove the distinction: a read↔read-only history has `overlapping_pairs >= 1`
but `read_write_overlapping_pairs == 0` and `is_genuinely_concurrent() == false`. The wire
leg now asserts `read_write_overlapping_pairs_across_processes() >= 1`, not the raw count. The
workload genuinely produces read↔write overlaps: the writer (process 0) issues real-time PUT
spans throughout, while readers (1,2) spin GETs continuously — a reader GET overlapping a
writer PUT is the non-vacuous overlap, and it is reachable (the gateway serves connections
concurrently, `crates/gateway-s3/src/lib.rs:158`).

### Fix 2 — indeterminate outcomes map to Elle's `:info`, never a definite outcome (carry-forward §2)

This is the load-bearing one — the sign-off flagged it as *the exact false-accept class that
sank v1–v3, merely relocated downstream into the off-Check Elle job*. A 5xx/timeout **write**
may or may not have committed; a failed **membership probe** observed neither present nor
absent. Baking either into a definite `:fail` / definitely-absent fabricates certainty the
wire never gave.

**Now:**
- `is_indeterminate(status)` (`consistency_workload.rs:60-68`) = `status == 0 || status >= 500`
  (5xx, or a synthetic 0 standing for a timeout / dropped connection before any response).
- `register_completion_type` (`:352-367`): an indeterminate status is `:info` (early return),
  *before* the `:ok`/`:fail` decision. A determinate 2xx is `:ok`; a GET's 404 stays `:ok`
  (a determinate *absent* read — the server successfully said "not found"); a 4xx the server
  rejected before any state change is a determinate `:fail`.
- `DirOp::present()` (`:590-606`): `200 → Some(true)`, `404 → Some(false)` (**determinate**
  absent), everything else → `None` (**indeterminate** — no boolean may be claimed). This
  replaced the v4 `present: Option<bool>` *field* (which `from_register_history` filled as
  `Some(status == 200)`, i.e. a 5xx → `Some(false)` = definitely-absent). `DirOp` now carries
  the raw `status` and derives membership, so a single source of truth can't disagree with
  itself.
- `directory_completion_type` (`:657-671`) + `directory_to_elle_edn` (`:679-733`): an
  indeterminate probe → `:info` with just the `member` (no fabricated `[member false]` pair);
  a determinate probe → `:ok` with `[member present?]`; an indeterminate add/remove → `:info`.

The Elle `:type` set is therefore `{:invoke, :ok, :fail, :info}`. The brief's criterion (b)
enumerated only `{:invoke, :ok, :fail}`, but the carry-forward *explicitly* directs "Map these
to Elle's :info (unknown) instead" — `:info` is standard Jepsen/Elle semantics for an
indeterminate op, and adding it is exactly the demanded tightening (the brief's enumeration
was illustrative of the definite-outcome cases). Elle then treats an `:info` op as "may or may
not have happened," so the false-accept can no longer be manufactured *or* relocated.

### Fix 3 — a delete establishes a read-your-own-delete obligation (carry-forward §3)

**Was:** `session_read_your_writes` cleared its obligation on DELETE, so a read that
resurrected a deleted value was silently accepted, and that branch was untested.

**Now** (`consistency_workload.rs:450-505`): the per-key obligation is an enum
`RywObligation { AtLeast(u64), Absent }`. A write sets `AtLeast(w)` (a later read must observe
`>= w`); a **delete sets `Absent`** (a later read must observe the key *absent* — read your own
delete). A determinate read that resurrects a value after the session's own delete is now
**rejected**. Indeterminate reads (5xx/timeout) are skipped in the check — an indeterminate
read proves neither compliance nor violation, so counting it a violation would be an unsound
false-reject (same discipline as fix 2, applied to the local checks). Two new crafted cases
cover the branch: `session_read_your_writes_rejects_a_read_after_the_sessions_own_delete`
(integration) and `session_ryw_rejects_a_read_after_own_delete` (module unit).

## Why guard-the-symptom is not what happened here (no cheaper cause-removal was skipped)

This is net-new functionality (brief principle 1.3, no invariant-to-restore), so the axis is
"smallest change that delivers the end result," not "smallest diff." All three fixes *remove a
cause* rather than guard a symptom: fix 1 changes the witness definition (a read↔read overlap
is no longer *called* concurrency); fix 2 changes the outcome mapping at the source (an
indeterminate op is never *turned into* a definite one); fix 3 changes the obligation model (a
delete now *creates* the correct obligation). None adds a probe around a wrong value. The only
data-model change with a footprint — replacing `DirOp.present: Option<bool>` with `status: u16`
+ a `present()` method — is 1 field swapped for 1 field + a 5-line method, and it is what makes
the "definitely-absent" bug *unrepresentable* rather than merely avoided (cost: the golden and
wire tests move from `op.present` to `op.present()`, ~4 call sites in the test).

## The v4 design decision this iteration keeps (per-session monotonicity, not global)

Monotonicity is still checked **per-process** (`per_process_reads_monotone`,
`consistency_workload.rs:207-224`), not across the merged history. A global cross-process
register-monotonicity decision *is* the linearizability verdict ADR-0041 reserves for Elle,
off-Check — asserting it in-gate is the rejected v1–v3 vehicle. The in-gate slice asserts only
the sound, local per-session invariant; Elle owns the global verdict over the SAME serialized
history. This division is unchanged and is why the slice does not re-derive a verdict.

## The three forced refutation questions (each actually run, evidence recorded)

**(a) Genuine red?** YES — each of the three carry-forward fixes was independently weakened
back to its v4-rejected behaviour and the targeted assertion went red, then restored:

1. `is_genuinely_concurrent` → `overlapping_pairs_across_processes() >= 1` (count read↔read):
   `concurrency_witness_requires_a_read_write_overlap_not_read_read` **FAILED** —
   "a read↔read-only overlap must NOT count as genuine concurrency."
2. drop the `:info` early return in `register_completion_type` **and** `DirOp::present() → _
   => Some(false)` **and** drop it in `directory_completion_type`: both goldens **FAILED** —
   the register golden emitted `:fail :write value 2` where the fix emits `:info`, and the
   directory golden emitted `:fail :contains? ["b" false]` where the fix emits `:info … "b"`.
   (Diffs captured in the run log — the failing bytes are literally the relocated false-accept.)
3. `Some(RywObligation::Absent) if false => …` (never enforce read-your-own-delete):
   `session_read_your_writes_rejects_a_read_after_the_sessions_own_delete` (integration) and
   `session_ryw_rejects_a_read_after_own_delete` (unit) **FAILED** — "a read that resurrects a
   value after the session's own delete must be rejected." (I first tried deleting the `Absent`
   *insert*, but that makes the variant dead code and reds as a `-D dead-code` compile error,
   not behaviourally; the `if false` guard keeps the enum live so the red is a genuine
   *behavioural* one.)

   All restored → 11/11 integration + 5/5 module unit green.

**(b) Production path?** YES. The wire legs
(`concurrent_workload_produces_a_nonvacuous_genuinely_concurrent_history`,
`directory_workload_records_and_serializes_a_set_history`) drive the **production**
`ObservableS3Client` (real signed SigV4 HTTP, real overwriting commits that bump the register,
real reads / 404s) against the real `wyrd_server::Gateway` + `S3Gateway` HTTP wire — no mock.
The socket-free legs feed the **production** serializer / session checks / dispatch / witness
(the same functions the wire path calls) with crafted inputs — not a copy. The three fixed
functions are the very ones the wire path exercises.

**(c) Fixture includes the fault?** YES. The concurrency fixture is the real gateway with
genuinely concurrent clients producing actually-overlapping read↔write spans (asserted, not
curated). The crafted-history reds each *include the fault element*: the read↔read-only
history (with a non-overlapping write present, so the fix must specifically reject the vacuous
overlap), the 5xx write and the 5xx probe (the indeterminate ops), and the resurrect-after-
delete read.

## Verification runner

Named test red→green run scoped via `cargo test -p wyrd-server --test consistency_workload`
(and `--lib consistency_workload` for the module unit tests), bounded by the tool timeout. The
**authoritative** gate is `./engine/xtask.sh ci` (= `cargo xtask ci` in `$PDCA_WORKTREE`,
INTEGRATION §3) — run in full: fmt `--check`, clippy `-D warnings`, build, the whole test
suite (incl. this test), `cargo deny`, conformance, statics, orchestrator-guard, DST — **exit
0, "xtask ci: all checks passed."** Commit-ready: `cargo fmt -p wyrd-server -- --check` clean
and `cargo clippy -p wyrd-server --all-targets` clean (a `collapsible_match` lint on the RYW
match guard was fixed).

## Scope honesty / deferred (unchanged from the brief)

The golden asserts the serializer is **stable and well-shaped** (byte-exact), NOT that real
Elle parses it — Elle is JVM/Clojure and stays off-Check (ADR-0041/ADR-0016), so real-parser
acceptance is part of the deferred verdict leg. `verdict_dispatch` is the built seam that
routes the verdict there; this in-gate slice returns no register/namespace linearizability
verdict. The live Elle verdict and the #407 real-cluster nemesis consume this slice's
serialized history downstream. No external dependency beyond what #405's merged wire test
already uses (loopback TCP + in-process gateway) — no NEEDS-HUMAN external-dependency blocker.
