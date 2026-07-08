# Adversarial review — issue 406 / consistency-checker models + workload + recorder

Lens: refute the red→green and the reviewer's verdict; find the input that breaks the fix.
I compiled the **actual** `crates/testkit/src/consistency.rs` module verbatim (pure `std`)
and drove it with crafted histories to test the claims empirically.

## Findings

- **NEEDS-HUMAN — the register model still FALSE-ACCEPTS a genuine stale/torn read — the same
  false-accept class that sank iterations 1 and 2, only *partially* closed.**
  `crates/testkit/src/consistency.rs:381-385` builds value provenance **per-key**
  (`written: BTreeMap<key, Set<value>>`), and `:432-434` admits any read whose value appears
  *anywhere* in that per-key set; `:419-426` seed `version_value` only from committed writes,
  and `:444-458` reject a read only when its version *already maps to a different value*.
  So a read reporting a **superseded value at a version strictly higher than any committed
  write's version** slips through every pass. Concrete case, **empirically ACCEPTED**:
  `[w k=10@v1 (idx0-1); w k=20@v2 (idx2-3); r k=10@v3 (idx4-5)]` — the read begins (idx 4) in
  real time strictly after the overwrite to 20@v2 completed (idx 3), so a linearizable register
  must return 20 or newer; returning the superseded 10 is a stale read. It also violates the
  model's **own** `TornRead` contract at `:233-235` ("a read returned a `(value, version)` the
  commit point never produced") — no committed write ever produced `(10, v3)`. Pass 0
  (`:427`, `UnresolvedWrite`) closed only the `version=None` sub-case the iteration-2 sign-off
  named; Pass 3 (`:493-521`) catches a stale read only at the value's *real* (lower) version
  (verified: the identical read at `@v1` is correctly `VersionRegression`-rejected; at a phantom
  version *below* the newest committed version, also rejected). The uncovered window is exactly
  a **torn commit** — inode `version` bumped under the CAS while `size`/value is stale — which is
  the canonical metadata corruption this checker exists to catch, and which the workload's own
  `size == value` observe design (`crates/testkit/tests/consistency_models.rs` `observe`/`payload`)
  would surface as `read_ok(old_value, new_version)`. None of the shipped crafted reds exercise
  this shape (the torn-read test uses a never-written value → provenance; the regression test
  uses the value's real version → Pass 3). Brief Success criterion (a) — "the rw-register model
  **rejects** a hand-crafted **inconsistent** history (a stale/torn read …)" — is therefore not
  fully met, and the deferred-Elle "verdict over the SAME history" claim is undercut: a real
  rw-register checker would flag this history, so the Rust model is not a faithful pre-filter.

- **The produced-history leg (criterion (d)) cannot exercise the checker's teeth — it is a
  serialized log dressed as concurrent, so its pass is checker-correctness-blind.**
  `crates/testkit/tests/consistency_models.rs:635,648` call `versioned_put(…, Some(&hot_lock), …)`,
  and `versioned_put` (`:559`) holds `hot_lock` across the **entire** commit+observe of the
  shared HOT key, so every hot-key commit is fully serialized; per-process keys are single-writer.
  Consequence: the produced history contains **zero** CAS conflicts, **zero** `Fail` events,
  **zero** `version=None`, and no torn/stale/two-winner conditions — the `is_conflict` retry loop
  never fires. The barrier (`:634`) overlaps only the *invoke* records, which is enough to satisfy
  `max_register_concurrency >= 2` (`:770`) while the actual mutations run strictly serially. So
  "the register model then passes" (`:757`) is near-guaranteed regardless of the checker's
  correctness — precisely the structural objection the iteration-2 sign-off raised, now relocated:
  all detection teeth live **entirely** in crafted histories, and finding 1 shows a crafted
  stale/torn read the teeth miss. (This is by-brief-design for non-vacuity; flagged so the human
  weighs that criterion (d)'s green proves non-vacuity, not correctness.)

- **The session checks have no `UnresolvedWrite`-equivalent guard — a version-less own-write is
  silently skipped, not rejected.** `crates/testkit/src/consistency.rs:554-558`
  (`check_read_your_writes`) advances the RYW floor only `if let Some(ver) = op.version`, and
  `check_monotonic_reads` (`:585` onward) likewise skips version-less observations. Unlike
  `check_register`'s Pass 0, a committed own-write with `version=None` does not raise the floor,
  so `[w k=50@None; r k=50@v1]` is **ACCEPTED** (empirically confirmed) — a contended-session RYW
  history the register model would reject as `UnresolvedWrite`. Minor, because the produced
  workload records a version on every write; raised for crafted-history coverage parity with the
  register model (a skeptic's note, not a produced-path defect).

- **NEEDS-HUMAN — I cannot confirm the C4-verify "red" is a *model-weakening* red rather than a
  whole-patch *compile-error* red.** `check-gates.json` C4-verify asserts "red without the fix,
  green with it," but `run-verify.sh` / the engine are harness-side and absent from
  `$PDCA_TARGET`, so I cannot inspect what it reverted. The brief's iteration-1 carry-forward
  explicitly requires a model-weakening red (flip a guard, keep compiling), not the
  new-module-revert that merely fails to compile. The test file's flip comments (e.g.
  `consistency_models.rs`: "delete Pass 0 … `consistency.rs` still compiles") describe the correct
  shape, but I could not verify the gate actually exercised a weakening flip. (Toolchain-absent →
  provisional; not scored as a refutation.)

## Attempted but could not refute

- The **namespace** model (`check_list_append`): probed a name present-in-list backed only by a
  *failed* create (correctly `ResurrectedDelete`), a valid rename, and unrelated-remove masking of
  a lost create — all held.
- Two-winners counting (`consistency.rs:459-477`) correctly flags same-value and different-value
  double-commits; the four contended-path crafted reds (`UnresolvedWrite`/`LostWrite`) reject as
  claimed; the real-version stale read is caught by Pass 3.
