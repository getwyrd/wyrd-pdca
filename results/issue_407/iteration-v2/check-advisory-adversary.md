# check-advisory-adversary.md — issue #407, iteration 2 (adversarial pass)

Evidence re-run at `$PDCA_TARGET`: both named Check tests are **green** (`cargo test -p
wyrd-metadata-fault-conformance --test nemesis_oracles` → 10/10; `cargo test -p xtask --test
nemesis_orchestration` → 4/4, after `cargo clean -p` — the first runs failed with `E0432
unresolved import ...::nemesis` from a **stale build cache**, which incidentally exhibits the
exact red the verify gate claims once the production modules are reverted, so the red→green
mechanism for these two files is genuine, not a tautology). Findings, strongest first:

- NEEDS-HUMAN [impl] — **The clock-skew leg deterministically fails on every live run: the probe
  container ID goes stale the moment `apply()` recreates the node.** The runner resolves the skew
  container ONCE, before any leg, as a container **ID** (`container_of` = `docker compose ps -q`,
  `xtask/src/fdb_faults.rs:81-106`, called at `xtask/src/fdb_faults.rs:389`, exported at
  `xtask/src/fdb_faults.rs:454`). The leg's `apply()` then runs `docker compose up -d
  --force-recreate fdb2` (`crates/metadata-fault-conformance/src/nemesis.rs:781,817`), which
  destroys that container and creates one with a NEW ID — after which every probe `docker exec
  <old-id> date +%s` (`nemesis.rs:768`) fails, `wait_execable` times out at 90s
  (`nemesis.rs:818,791-792`), `apply` errors, and the leg can never materialize. Concrete failing
  case: `WYRD_TIER1=1 cargo xtask metadata-nemesis` → skew leg fails with "target container never
  became exec-able" on **every** run. This falsifies the in-tree claim that the single runner
  resolution makes disagreement "structurally impossible"
  (`crates/metadata-fdb/tests/tier1_metadata_nemesis.rs:122-127`, `nemesis.rs:751-753`) and
  repeats the exact defect class (live skew leg cannot materialize on the default run) that got
  iteration 1 rejected as carry-forward item 2. Fix direction: probe by stable compose container
  *name* (or re-resolve `compose ps -q` after each recreate) instead of a pre-recreate ID.

- NEEDS-HUMAN [impl] — **Cross-leg staleness: the skew leg's recreates poison the netns map for
  the process-pause leg that runs after it.** `netns_map` is also resolved once, pre-campaign
  (`xtask/src/fdb_faults.rs:388`), and the campaign order is partition → clock-skew → pause
  (`xtask/src/nemesis.rs:80-86`). Both the skew `apply` and its `heal` force-recreate `fdb2`
  (`nemesis.rs:781,831` — heal runs even when apply fails, via `drive_leg`'s heal-on-failure), so
  by the pause leg `fdb2`'s mapped container ID is dead. If the post-restart master lands on
  `fdb2`, `resolve_role_holder` hands back the stale ID
  (`crates/metadata-fdb/tests/support/mod.rs:81-82`) and `docker pause <stale-id>`
  (`nemesis.rs:684`) fails — a probabilistic live failure of a leg whose logic is otherwise
  correct. Same root cause and same fix as the previous bullet.

- NEEDS-HUMAN — **The third added test file earns no red, and its live body is compiled by no
  gate anywhere in this cycle — the verify PASS covers only two of the three added tests.**
  Verified at target: `cargo test -p wyrd-metadata-fdb --test tier1_metadata_nemesis` under
  default features is green with `0 passed; 3 ignored` regardless of whether the fix is present
  (its imports of the new modules live only under `#[cfg(feature = "fdb")]`,
  `crates/metadata-fdb/tests/tier1_metadata_nemesis.rs:107,133,138`), so the brief's
  falsifiability sentence "the gate loops every added `*/tests/*.rs`, whose imports/assertions
  against the new modules then fail to compile/pass" is unwarranted for this file. I attempted to
  type-check the fdb-feature body (`cargo check -p wyrd-metadata-fdb --features fdb --test
  tier1_metadata_nemesis`) and CANNOT: `/usr/include/foundationdb/fdb.options` is absent —
  toolchain unavailable, NOT scored as a refutation (issue #236); verdict on that body is
  provisional. A manual API cross-check found no mismatch (`support::{processes,
  resolve_role_holder, survivor}` at `crates/metadata-fdb/tests/support/mod.rs:26,75,100`;
  `FdbMetadataStore::open`/`with_prefix` at `crates/metadata-fdb/src/lib.rs:1260,1275`;
  `WriteBatch` builder at `crates/traits/src/lib.rs:659-690`), but the brief's declared posture —
  "'compiled by ci' is claimed ONLY for the default-compiled surface" — means the maintainer's
  witnessed `WYRD_TIER1=1` run is the FIRST compile of this file's live body, and per the two
  [impl] findings above that run would fail today. The sign-off open question is not currently
  satisfiable.

- NEEDS-HUMAN [impl] — **`drive_leg` silently drops a heal failure when the workload panics,
  contradicting its own leak-free claim.** On the panic path, `resume_unwind` at
  `crates/metadata-fault-conformance/src/nemesis.rs:343` executes BEFORE `heal_result?` and the
  `heal_is_complete` check (`nemesis.rs:346-353`), so a failed `docker unpause`/recreate during a
  panicking workload leaks a paused container / skewed node with only the workload panic
  reported — while the module doc claims "no leg may leave a cut cluster, a paused container, or
  a skewed clock behind" (`nemesis.rs:50-51`). The guard test
  (`crates/metadata-fault-conformance/tests/nemesis_oracles.rs:383-401`) asserts only
  `heal_count >= 1`, so it cannot catch this. Concrete case: #408's checked workload panics on a
  violation AND the unpause errors → the leaked-fault error is unreported. Low severity inside
  `xtask metadata-nemesis` (the runner tears the whole stack down,
  `xtask/src/fdb_faults.rs:400-403`), but #408 imports `drive_leg` directly with no such
  teardown. Fix: log/append the heal failure before `resume_unwind`.

- Nit (no adjudication needed): `parse_tests_run`'s shape check is self-defeating —
  `tail.starts_with("test")` (`xtask/src/nemesis.rs:135`) subsumes the other two arms and accepts
  e.g. `running 3 testimonials`; it also takes the FIRST matching line of interleaved
  stdout+stderr. Harmless today (one test binary per leg), but the guard is weaker than its
  comment claims.

Refutations attempted and FAILED (signal, not filler): the oracle arithmetic boundary cases
(zero floor, |offset| == floor, crash-vs-partition `target_running_during`, single-probe pause)
are all pinned by discriminating assertions; the mock-`drive_leg` tests genuinely flip red if
the inconclusive bail (`nemesis.rs:320-329`) or the `heal_is_complete` check
(`nemesis.rs:347-353`) is deleted (they assert error text + workload-ran flags, not just
`is_err`); `PartitionLeg::heal` returns previously-removed rules too (`nemesis.rs:563-579`), so
the `applied ⊆ healed` completeness check holds across the double-heal path; the orchestration
tests assert independent expectations, not the returned literals. The Check-core half of this
patch withstands attack; the live-leg half does not.
