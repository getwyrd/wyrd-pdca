# Adversarial review — issue 406 (elle-register-listappend-models-and-workload-recorder)

Advisory only; I never gate. Toolchain **was** available (cargo 1.96.0), so I re-ran the
red→green and attacked the models with crafted histories against a byte-for-byte copy of the
shipped `crates/testkit/src/consistency.rs` (pure-std; no target files modified).

## Evidence re-run — attempted to refute, could NOT

- **Green half holds.** `cargo test -p wyrd-testkit --test consistency_models` → 18/18 pass in
  the target.
- **The headline red is a genuine model-weakening red, not a compile-error red** (the exact
  concern iteration-1 raised). I built the mutant the test's own doc-comment names — dropping
  `&& c.ok` from the provenance loop at `crates/testkit/src/consistency.rs:330` only — and it
  **compiles and false-accepts** the failed-write-value history (`check_register` → `Ok(())`
  where `register_rejects_a_read_of_a_failed_writes_value` asserts `Err(TornRead)`). So the
  `check-gates.json` C4-verify claim ("red without the fix, green with it") is sound for that
  test, and the iteration-1 provenance fix (`consistency.rs:328-335`) genuinely closes the
  previously-rejected false-accept.
- **The workload test is not flaky:** 40/40 clean runs of
  `workload_against_the_in_process_gateway_yields_a_nonvacuous_checkable_history`. The
  real-time-proxy interval modeling (observe() sandwiched between the recorder's invoke/ok
  lock sections) is sound against spurious version-regression, and the barrier makes the
  non-vacuity asserts (`concurrency >= 2`, `version >= 2`) robust.

I could not refute the specific defect the previous cycle rejected, nor the two-winners /
rename / session crafted reds — those flips are all real behavioral reds on real inputs.

## The fix — concrete inputs that break it (new false-accepts)

The model documents itself (check_register doc, `consistency.rs:311` and the fn header) as
proving "no torn **or stale** read". The stale-read guarantee is delivered **only** by the
Pass-3 real-time version-regression loop (`consistency.rs:412-431`), and Pass-2 value
provenance (`consistency.rs:379`) keys a read's value to the **key**, never to its
`(key, version)`. That leaves detectable stale reads that the model **accepts** — the same
false-accept class that got iteration-1 rejected ("a checker whose whole value is its
correctness MUST detect this"). All three below were run through the shipped model and
returned `Ok(())`:

- **NEEDS-HUMAN — stale read on the contended path is invisible (`consistency.rs:412-419`).**
  `[w(k,10)→ok v1; w(k,20)→ok **version=None**; r(k,10)@v1]` with the read beginning strictly
  after the v2 write completed → **accepted**. The read returns the superseded value after a
  committed overwrite (a textbook stale read / non-linearizable), but because the winning
  write recorded `version=None` it is excluded from Pass 3's observation set
  (`if let Some(ver) = c.version`), so no regression fires. This is not academic: the
  production workload's `put_with_retry` returns `None` exactly under contention
  (`crates/testkit/tests/consistency_models.rs:454-456`), and the barriered 3-way HOT
  overwrite makes `version=None` write-oks the **common** case in the very history fed to
  `check_register`. So on the path criterion (d) most stresses, the model's stale-read
  detection is disabled — a buggy gateway that served a stale contended read would still be
  **passed**. That undercuts the load-bearing claim that "the register model then passes" is
  evidence of anything.

- **NEEDS-HUMAN — provenance is per-key, not per-(key,version) (`consistency.rs:379`).**
  `[w(k,10)→ok v1; w(k,20)→ok v2; r(k,10)@v3]` → **accepted**. Value 10 was superseded at v2
  and version 3 was never committed, but the read passes because 10 was written *somewhere*
  for k and no committed write pinned `(k,3)`. A crafted stale read the brief's criterion (a)
  says the register model must reject.

- **NEEDS-HUMAN — a vanished committed value (lost write) is not detected
  (`consistency.rs:377`).** `[w(k,10)→ok v1; r(k)→absent]` → **accepted**. The register model
  has no delete op, so a key that reads absent after a committed write is a lost write /
  stale read; Pass 2 treats an absent read as "observes nothing" and skips it, and Pass 3
  ignores versionless observations, so nothing flags the disappearance.

- **NEEDS-HUMAN — exactly-one-writer-wins is also disabled on the contended path
  (`consistency.rs:344-346`).** Pass 1 only counts a write when it carries `Some(ver)`.
  `[w(k,10)→ok None; w(k,20)→ok None; …]` (two winners, both `version=None`) → **accepted**.
  Same root cause as the stale-read gap: the exact `version=None` writes the workload emits
  under contention are the ones on which two-winners detection cannot fire, so the "real
  overwrites at the commit point" the workload advertises are the least-checked ops.

## The verdict — where the reviewer may have rationalized

- **NEEDS-HUMAN — the "no stale read" guarantee in the `check_register` doc
  (`consistency.rs:311` / fn header) is stronger than the implementation delivers.** The
  crafted red tests cover *one* torn read, *one* version regression, and two two-winners
  variants — a real green — but by the iteration-1 standard (one false-accept was sufficient
  to reject), the four false-accepts above are the same defect class. A human must decide
  whether they are acceptable incompleteness for a net-new conservative checker **or** a
  repeat of the rejected fault. The strongest of the four (contended-path stale read /
  two-winners) is not mere crafted-history incompleteness — it degrades detection on the
  production path the slice exists to exercise, so criterion (d)'s "the register model passes
  the produced history" is weak positive evidence: on the contended ops, passing is nearly
  guaranteed regardless of correctness.

## Scope note

All findings are on the code this diff adds (`crates/testkit/src/consistency.rs`,
`crates/testkit/tests/consistency_models.rs`). No pre-existing debt cited. The rename-branch,
session-teeth, and identical-value two-winners remarks from iteration-1 are genuinely
addressed by the patch (recorder rename API + tests at
`consistency_models.rs:1461`; occurrence-count two-winners at `consistency.rs:343-359`).
