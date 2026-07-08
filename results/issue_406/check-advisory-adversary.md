# Adversarial review — issue 406 (consistency-workload-history-and-elle-serialization)

Advisory only; I never gate. I re-derived the red→green by reading the target at
`$PDCA_TARGET` and building the suite (`cargo test -p wyrd-server --test consistency_workload
--list` → 9 tests compile clean; the fix is green). I did not add a test to the read-only
target, so the failing cases below are grounded on the code path, not a fresh run.

## Refutations (concrete)

- **NEEDS-HUMAN — INV-1 leak in an un-audited arm: the RYW *read* arm fabricates an `Absent`
  observation from a determinate non-404 failed read** —
  `crates/server/src/consistency_workload.rs:226`–`231`. The `None` branch is commented "A
  determinate absent read (404)" and returns `false` (own-write-lost) whenever a standing
  `AtLeast(_)` obligation exists. But `version` is `None` for **every** non-200 GET, not only
  404: `ObservableS3Client::get` sets `version = Some(..)` iff `status == 200`, else `None`
  (`crates/server/src/consistency_observable.rs:181`–`185`). So `is_indeterminate` (only
  `0`/`≥500`, `:57`–`58`) lets a determinate **4xx-non-404** GET (403, 400, 409, 412, 416…)
  fall into the `None` branch and be treated as a *definite absence*. Concrete failing case:
  `[PUT k v=1 @200 ; GET k version=None @403]` ⇒ `session_read_your_writes()` returns
  **false** — a fabricated RYW violation from a read that observed *nothing* about the
  register. This is exactly the INV-1 fault-class ("never convert an outcome-unknown into a
  definite claim") re-appearing in an arm the crafted reds never probe (the read-arm test uses
  only 500/200/404, `tests/consistency_workload.rs:1088`–`1096`). It is also **internally
  self-contradictory**: the module's own serializer labels a 403 GET `:fail` — "no value
  observed" (`register_completion_type`, `:503`, `:508`) — while the session check treats the
  same op as a determinate absence. The brief's success-criterion (c) claim that the session
  checks are "sound surface-wide (INV-1)" is therefore unwarranted for this arm, and this is
  precisely the point-vs-class scoping failure the re-plan was meant to close.

- **NEEDS-HUMAN — the PUT/DELETE "indeterminate → clear obligation" reds are weaker than
  claimed; deleting the guard leaves them green** — `crates/server/src/consistency_workload.rs:195`–`197`
  (PUT) and `:205`–`207` (DELETE). The tests' comments assert "RED if the PUT/DELETE arm stops
  guarding `is_indeterminate`" (`tests/consistency_workload.rs:1061`–`1083`), but the crafted
  histories carry **no prior obligation**, so the `Obligation::Unknown` clear is never
  observable. Trace: dropping line 197 entirely (keeping only the `else if is_success` arm)
  leaves `put_arm` green — a 500 PUT fails `is_success` too, so the obligation stays unset and
  the later `GET v=1 @200` still passes. Same for `delete_arm`: dropping line 207 leaves the
  standing `AtLeast(1)` from the prior determinate PUT, and reading `v=1` satisfies it → still
  green. The reds only flip against the *specific* historical v5 shape (an **unconditional**
  `AtLeast`/`Absent` insert), not against a general weakening of the indeterminate guard. A red
  that survives deletion of the very line it claims to pin is the weak-red pattern that let v4/v5
  through. A discriminating input the suite is missing:
  `[PUT k v=5 @200 ; PUT k v=2 @500 ; GET k v=1 @200]` — accept requires the indeterminate PUT to
  clear `AtLeast(5)`; without line 197 it stays `AtLeast(5)` and the read is (correctly, under
  that variant) rejected. (Note the flip side: whether *clearing* a determinate `AtLeast(5)`
  on a lower indeterminate write is itself desirable — it can mask a real own-write-lost of v5
  — is a soundness-direction judgement for the human; it errs toward accept, so it is within
  the "Elle owns the verdict" design, but it is untested either way.)

## Attempted and could not refute

- **INV-2 witness (cross-key / read↔read negatives).** I tried to find a vacuous overlap that
  still counts: `read_write_overlapping_pairs_across_processes` (`:149`–`165`) conjoins
  distinct-process ∧ same-key ∧ read↔write ∧ span-overlap; the cross-key and read↔read crafted
  negatives (`tests/consistency_workload.rs:1186`–`1204`) genuinely flip if any conjunct is
  dropped. Could not refute. (Sole nit: `spans_overlap` uses `<=`, so endpoint-touching spans
  count as overlapping — immaterial to the negatives.)

- **Serializer golden bytes.** The expected EDN is a hand-written literal, not recomputed with
  the serializer's own `join`, and the sort key `(time, process, phase)` reproduces the golden
  ordering exactly (`:438`–`447`); an indeterminate PUT maps to `:info` and a 404 GET to a
  definite `:ok` of `nil`. Genuine byte-exact, flippable, on the production `to_elle_edn` path.
  Could not refute the serializer-stability claim (and the brief already scopes it as
  stability-only, not Elle-parser acceptance).

- **Verdict-dispatch.** `consistency_verdict_dispatch` is a pure two-arm function with both
  arms representable and the default routing off-Check (`:744`–`757`); the red flips
  behaviourally if the default is re-pointed. Could not refute.

- **Monotonic-read arms.** `reads_are_monotone` (`:376`–`398`) correctly *skips* a `None`-version
  read via the `let Some(v) = … else continue`, so the 403-GET fabrication above does **not**
  reach the monotonicity checks — only the RYW read arm has it. Could not refute the monotonic arms.

## Scope note on the evidence gate

- The load-bearing socket-free reds are real and flippable (per above, with the two caveats).
  Leg (a)'s **wire-driven** green (`concurrent_workload_records_a_nonvacuous_genuinely_concurrent_history`,
  `tests/consistency_workload.rs:1294`) is timing/loopback-dependent and pre-declared deferred
  in the brief; I did not run it and make no claim about it — it is not the load-bearing red.
