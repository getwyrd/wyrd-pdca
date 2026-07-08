# Brief — issue 405 / networked-client-observable

> The Plan artifact (docs 02 §PLAN). Human-authored. Do reads ONLY this file.
> Design already settled by ADR-0041 (the consistency-checker substrate decision);
> this is the implementation slice, so a normal brief — not a new design proposal.

- **Slug:** networked-client-observable
- **Defect:** The #329 consistency checker (ADR-0041) needs a **networked client it can
  drive storage operations through while recording exactly when each op started and
  finished** — there is none today. Wyrd's object API is in-process (`crates/server/src/lib.rs`
  composes the backends and exposes PUT/GET/DELETE as in-process methods); only the
  fragment-level gRPC `ChunkStore` is networked. The S3 HTTP wire surface (#364, merged as
  PR #448 — `crates/gateway-s3/src/lib.rs`) now exists, and `crates/server/tests/s3_http_wire.rs`
  drives it by hand over a `TcpStream`, but there is **no reusable observable client that
  records a client-observed real-time history** of the ops it drives. Without that observable,
  #329's register-model checker has no non-vacuous, real-time-ordered history to validate
  ADR-0015 guarantee 2 (a file's writes are linearizable at its home zone) against — this
  slice builds it (slice 2 of #329).
- **Success criterion:** A reusable networked S3 client observable drives a workload of
  **overwriting PUT / GET / DELETE** against the S3 HTTP wire surface over a real loopback
  listener and produces a **per-operation history** in which every operation carries a
  client-observed **start and end real-time timestamp** and its observed value/version, such
  that a sequence of overwrites `v1→v2→v3` with interleaved reads yields a **non-vacuous
  register history** a linearizability checker can consume (read-after-commit, no stale/torn
  reads, version monotone). Demonstrable by C4-verify in isolation: the shipped test drives an
  in-process loopback gateway through the observable and asserts the recorded history is
  well-formed and non-vacuous — **red before the observable exists, green after**. (The
  downstream checker verdict and the real-cluster nemesis run are off-Check — see
  `Verification posture` — and are NOT part of this criterion.)
- **Falsifiability:** RED is produced on the **in-process loopback S3 gateway** — the
  `s3_http_wire.rs::start_gateway` pattern (redb + fs + mem backends behind the HTTP listener),
  which fully exhibits the register: an overwriting PUT bumps the inode `version` under the
  commit-point CAS (`commit_overwrite`/`commit_chunk_map`) and a GET returns the current
  version. Before the observable exists there is no client type and no recorded history, so the
  test cannot record a history → RED; a broken observable (missing/​reversed timestamps,
  non-monotone version, a vacuous single-op history) also goes RED. **No real TiKV cluster or
  partition nemesis is needed to falsify this slice** — the client observable's own correctness
  is a register history over the single-node loopback gateway; the partition/split-brain
  consistency *verdict* (which needs #257's ≥3-replica cluster + `tc netem`/`iptables` nemesis
  + the off-Check checker) is a later #329 slice, not this criterion. RED is therefore
  producible on the environment Do is pointed at → no Plan-blocking gap.
- **Invariant to restore:** Net-new capability, not a structural/lifecycle defect — no
  §6 structural invariant applies. The governing DESIGN constraint from ADR-0041 decision 1
  binds the shape and MUST hold: the recorded history is a history over the **mutable metadata
  register the contract governs** (ADR-0015 guarantee 2) — modelled on overwriting PUT/GET of a
  shared object key whose inode `version` is bumped at the commit point — and **never** on the
  content-addressed, write-once fragment layer (which yields the vacuous history the #250
  iterations produced; ADR-0041 §Context, §Decision "MUST be modelled on the metadata register,
  never on the immutable fragment layer"). Source: `docs/design/adr/0041-consistency-checker-substrate.md`,
  `docs/design/adr/0015-consistency-contract.md`.
- **Repo + branch target:** getwyrd/wyrd @ feat/m4-production-metadata-backend   (M4
  integration branch per INTEGRATION §2; the S3 wire surface #448 is on it, base has PUT/GET/DELETE)
- **Depends on:**   none
- **Conflicts with:**   none
- **Ordering note:** Standalone — no in-flight (unmerged) prerequisites, so `Depends on` is
  empty and this runs in the single parallel wave with the rest of the set. Its prerequisites are
  already MERGED into the base `feat/m4-production-metadata-backend` and are therefore satisfied
  by the base tip, NOT wave/stacking edges: the S3 HTTP wire surface (#364, PR #448) and the
  cluster + partition nemesis (#257, PR #453) — putting either in `Depends on` would create a
  phantom wave dependency. Additive change: a new harness module plus its integration test,
  sharing no file with any other batch id, so `Conflicts with` is empty too. The other five batch
  ids (291, 262, 263, 264, 265) are research/decision issues left unplanned this run, so there is
  no cross-bundle ordering.
- **Surfaces:** data   (backend/test-harness; no frontend)
- **Difficulty:** medium   (net-new harness surface — a reusable observable client module + its
  integration test + likely a Cargo.toml dep line; mostly additive and self-contained, but it
  introduces new public API for the #329 harness that later slices build on, so rated up from low)
- **Scope:** Build a reusable, networked, **observable S3 client** — a type that drives a
  configurable workload of **overwriting PUT / GET / DELETE** against the S3 HTTP wire surface
  over a real listener and records, per operation, a client-observed history entry (op kind,
  object key, value/version observed, **start + end real-time timestamps**) in a form a
  register-model linearizability checker can consume. / out of scope: the **directory
  list-append model (list / rename)** — the S3 wire surface exposes only PUT/GET/DELETE
  (ListObjectsV2 and rename are deferred, `crates/gateway-s3/src/lib.rs:40`), so list/rename need
  wire-verb work first (a #364 follow-on / later #329 slice); the **linearizability checker /
  verdict engine** (Elle / JVM, off-Check per ADR-0041 §Decision); the **real ≥3-replica cluster
  + partition nemesis** (#257, off-Check); wiring the observable into a live nemesis run.
  **[SCOPE DECISION — human to confirm:** this briefs the ADR-0041 decision-1 *register* model
  only (PUT/GET/DELETE), deferring the decision-2 *directory* model (list/rename) because the
  wire verbs for it do not exist yet. If you want list/rename in this slice it must first build
  ListObjectsV2 + a rename verb (a materially larger slice). Recommended: keep this slice
  register-only.]**
- **Repro instruction:** On `feat/m4-production-metadata-backend`:
  `grep -rn "observable\|real-time order\|OpRecord\|history" crates/` finds no register-model
  client observable; `crates/server/tests/s3_http_wire.rs` drives PUT/GET/DELETE by hand over a
  `TcpStream` but records no reusable history and models no register. The new test starts a
  loopback gateway (as `s3_http_wire.rs` does), drives an overwrite/read workload through the new
  observable, and asserts the recorded history is non-vacuous and well-formed (every op has
  `start ≤ end` real-time stamps; observed versions are monotone; a GET after a committed PUT
  returns that PUT's value). It is RED today (no observable type to construct).
- **External dependencies:** **none** beyond the base Rust toolchain — the criterion goes
  red→green against an **in-process loopback S3 gateway** (redb + fs + mem backends, no external
  services), exactly the `s3_http_wire.rs::start_gateway` pattern. (The downstream off-Check
  checker's dependencies — JVM/Elle, a live ≥3-replica TiKV cluster, `tc netem`/`iptables`
  nemesis — are OUT of scope for this slice; they belong to the later #329 checker slice on
  #257's cluster.)
- **Test file:** `crates/server/tests/consistency_observable.rs`   (peer to
  `crates/server/tests/s3_http_wire.rs`; `server` already depends on `gateway-s3` + the backends
  so the loopback gateway is in reach. Do chooses the observable's module home — a small module
  in `crates/server` is the least-friction place, peer to the wire test; note `crates/testkit`
  is the DST-seam crate with no async/HTTP deps, so the observable does NOT belong there.)
- **Verification posture:** DEFAULT does not fully hold — declare the split. **Built AND
  exercised at Check:** the observable client type + its history recording, driven red→green
  against a live in-process loopback S3 gateway (the register model — overwriting PUT/GET/DELETE,
  real-time-ordered history). This is the whole deliverable of this slice and it is load-bearing,
  not inert scaffolding — the shipped test drives real ops through it and asserts a non-vacuous,
  well-formed history. **Deferred / off-Check (a SEPARATE later #329 slice, not deferred
  verification of THIS deliverable):** the linearizability checker verdict (Elle/JVM, privileged
  off-Check job per ADR-0041; MUST NOT enter `cargo xtask ci`, ADR-0016) and the real-cluster
  partition-nemesis run (#257). Ask Do to capture a **demonstrated red** proving the recording is
  load-bearing — e.g. temporarily drop the end-timestamp or the version read and show the
  well-formedness/non-vacuity assertion goes red, then restore.
- **Production reach:** The observable is the **real** client for the harness (not a test
  double) — it drives the real S3 wire surface, and at Check it traverses that seam live against
  the loopback gateway for the register model. What is deferred is the *composition* — driving a
  real ≥3-replica cluster under nemesis and feeding the recognized checker — which lands in the
  later #329 checker slice (needs #257's cluster + the off-Check job). So the live path DOES
  reach the seam at Check for the register model on loopback; only the cluster+nemesis+checker
  wiring is deferred. This is not a test-double-only seam.
- **Citations expected:** Do must cite path:line on `feat/m4-production-metadata-backend` for
  every change. **Peer callsite to mirror the driving composition** (Do MAY open this one file):
  drive the S3 wire surface as `crates/server/tests/s3_http_wire.rs` does — start the loopback
  gateway (`start_gateway_with_handle` / `start_gateway`, `s3_http_wire.rs:64-82`), sign each
  request with the production `wyrd_gateway_s3::sigv4::sign` + `format_amz_date(SystemTime::now())`
  (`signed_headers`, `s3_http_wire.rs:86-99`), and issue PUT/GET/DELETE over a `TcpStream`
  (`s3_http_wire.rs:120+`; `signed_put_get_delete_round_trip_is_byte_identical`, `s3_http_wire.rs:183`).
  Record per-op start/end with `SystemTime::now()` around each request. Model the register on the
  inode `version` bumped by `commit_overwrite` / `commit_chunk_map` (ADR-0041 decision 1 — an
  overwrite is a new inode version).
- **Prior-art check (triage cycles):** searched by file path / PR history (merged, open, closed)
  — the register-model networked client observable does **not** exist. PR #333 built the Tier-1
  Jepsen harness over the **repair path** (ADR-0039 Option-B, the immutable data path — a
  *different* layer, vacuous for a register). PR #403 landed ADR-0041 (the substrate decision,
  docs only). No merged/open/closed PR builds the register-model client observable → clean; this
  is #329's first code slice (the issue titles it slice 2 of #329, slice 1 being the ADR).
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.
