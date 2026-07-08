# Adversarial review — issue #406 (consistency workload + Elle serialization)

Skeptic's pass. I re-ran the asserted proof at `$PDCA_TARGET`: `cargo test -p wyrd-server
--test consistency_workload` → **11/11 green** (incl. the 2 wire-driven — loopback bind is
permitted in this sandbox). The per-fix "red" is a whole-file **compile** failure (the
`consistency_workload` module is absent pre-fix), not a per-assertion red; the golden /
rejection assertions themselves are non-tautological and genuinely flippable. I could not run
the full `cargo xtask ci` (C4-ci) — that gating pass I take provisionally. I attacked the two
iteration-4 tightenings the sign-off demanded and the reviewer marked closed. Two land.

## Refutations

- **NEEDS-HUMAN — Iteration-4 tightening 2 is only half-applied: `session_read_your_writes`
  manufactures a *definite* obligation from an *indeterminate* write/delete, false-REJECTING
  valid histories.** `crates/server/src/consistency_workload.rs:480-487` — the `Put` arm
  inserts `AtLeast(v)` and the `Delete` arm inserts `Absent` **without** checking
  `is_indeterminate(op.status)`, even though the `Get` arm guards it (`:490`). A 5xx/timeout
  write "might or might not have committed" (the module's own words, `:31-40`), yet the check
  treats it as a committed obligation. Concrete valid histories that get **rejected**:
  `[PUT k v=2 status=500; GET k v=1 status=200]` and `[PUT k v=1 status=200; DELETE k
  status=500; GET k v=1 status=200]`. I transcribed the exact algorithm and ran it: **both
  return `false`** (reject) — but both are legitimate, since the indeterminate mutation may
  never have landed. This is the *same* "bakes an indeterminate outcome into a definite one"
  fault the sign-off flagged, merely relocated from the serializer's `:fail` (which was fixed)
  and the *read* side (which was guarded) to the **obligation-establishing** side (which was
  not). It directly violates success-criterion (c) "ACCEPT a valid one" and the module's own
  invariant at `:38` (which conspicuously scopes the promise to "an indeterminate *read*"). It
  is not merely crafted: `ObservableS3Client::put` records `version: Some(version)` on **every**
  PUT regardless of wire status (`crates/server/src/consistency_observable.rs:173-180`), and a
  5xx is a real wire outcome, so a real workload history containing a 500 write feeds this bug.
  The reviewer's "tightening 2 addressed" claim in the brief is unwarranted for the session
  check.

- **NEEDS-HUMAN — Iteration-4 tightening 1 is over-broad the other way: the "genuine
  concurrency" witness ignores the object key, so a *cross-key* read↔write overlap counts —
  as vacuous for register linearizability as the read↔read overlap it was meant to exclude.**
  `crates/server/src/consistency_workload.rs:158-171` (`read_write_overlapping_pairs_across_processes`),
  `:236-238` (`is_read_write_pair`), `:224-226` (`spans_overlap`) test only `process`, span
  overlap, and op-kind — **never** `a.record.key == b.record.key`. The doc claims this is "the
  only overlap non-vacuous for register linearizability" (`:155-157`, `:173-176`), but a read
  of key `"a"` overlapping a write of key `"b"` places no ordering constraint on *any single
  register* — exactly the vacuity the tightening targeted. Concrete case:
  `MultiProcessHistory::from_process_ops([P1 GET "a" [0,10], P0 PUT "b" [5,15]])
  .is_genuinely_concurrent()` returns **`true`**. The wire test happens to drive a single key
  (`register-object`), so its green is sound *there*, but the exported witness over-accepts and
  the guarantee its docstring advertises does not hold in general. Whether cross-key overlap
  should count for the Elle register model is a judgment call → for human adjudication.

## Weaker points (noted, not scored as refutations)

- **Asymmetric indeterminate handling between the two session checks.**
  `session_read_your_writes` guards `is_indeterminate` on reads (`:490`) but
  `session_monotonic_reads` (`:514-529`) does not — it relies solely on `version == None`. For
  *real* client histories the two coincide (the client sets `version = None` on any non-200
  GET, `consistency_observable.rs:191-195`), but the two **public** checks would treat a crafted
  indeterminate GET that carries a version differently. Low impact; latent inconsistency in the
  exported surface.

- **`version_climbs_for_key` is tautological for the shipped workload.**
  `crates/server/src/consistency_workload.rs:182-196` derives "the version climbs" from the
  PUT's *written* (caller-supplied) version, i.e. the writer's loop counter `1..=overwrites`,
  never a backend-observed value — so for `run_concurrent_register_workload` it is `true` by
  construction and can never fail whatever the backend does. It adds no falsifiable signal to
  criterion (a); the real observation is `per_process_reads_monotone` (which does read observed
  GET versions). Not wrong, but weaker evidence than the brief implies.

## Attempted and could not refute

- The register and directory **byte-exact golden serializers** (`to_elle_edn` /
  `directory_to_elle_edn`): the expected EDN is written independently of the algorithm, the
  `:info` mappings for 5xx are correct, event ordering `(time, process, seq, phase)` keeps
  per-process invoke→completion nesting deterministic (no HashMap in the output path). I could
  not make it pass for the wrong reason.
- The **verdict-dispatch** seam (`verdict_dispatch`): default routes off-Check, the in-gate
  shell-out arm is representable but non-default; the test uses independent expectations and a
  panic arm. Non-tautological; could not refute.
- The **delete-then-read RYW** case the sign-off asked for is present and correct for
  *determinate* deletes (`:485-487`, tests at `tests/consistency_workload.rs`) — my refutation
  above is narrowly about the *indeterminate* delete/write, a distinct path.
