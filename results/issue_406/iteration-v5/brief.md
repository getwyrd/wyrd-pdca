# Brief — issue 406 / consistency-workload-history-and-elle-serialization

> The Plan artifact (docs 02 §PLAN). Human-authored. Do reads ONLY this file.
> The field labels below are parsed by the driver — keep the `- **Label:** value`
> shape. The success criterion is the load-bearing field: it is the sentence
> Check tests "did this work" against.
>
> NET-NEW functionality (not a bug fix): the "Defect" field states the GAP/need, and
> the minimalism maxim does not govern (principle 1.3); there is no invariant-to-restore.
> This slice implements the **accepted** ADR-0041 (acceptance issue #410, CLOSED) — the
> Plan artifact of record — so no new design proposal is needed.
>
> **RE-PLAN (supersedes iterations v1–v3 — read this).** Iterations 1, 2, and 3 all built
> a **hand-rolled, in-gate register-linearizability verdict** in pure Rust and were rejected
> each time for the SAME class of fault: the checker FALSE-ACCEPTED an inconsistent history
> (v3: it accepted the stale read `[w k=10@v1; w k=20@v2; r k=10@v3]`). The iteration-3
> sign-off directed a plan-level re-scope, not a rebuild. **This brief drops the in-gate
> hand-rolled verdict entirely.** The recognized checker (Elle) owns the register/namespace
> linearizability verdict, and per ADR-0041/ADR-0016 it runs **only off-Check**. The Rust
> slice's job is exactly what ADR-0041 grants it: **produce a genuinely concurrent,
> non-vacuous history over the real wire and serialize it in the recognized checker's
> format** — plus only the *sound, local* checks that are cheap to get right. **Do MUST NOT
> re-implement a global register-linearizability decision procedure in `cargo xtask ci`** —
> that is the rejected vehicle. The sequencing question that dogged v1–v3 is resolved: #405
> (the networked observable) has **merged** (`8af8e97`, "Fixes #405") and is the substrate
> this slice builds on.

- **Slug:** consistency-workload-history-and-elle-serialization
- **Defect:** (gap / need) ADR-0041 §Decision names three deliverables for #329's
  consistency artifact: (1) the read-write **register** model, (2) the **directory
  list-append/set** model, (3) **session** read-your-writes + monotonic-read checks — plus a
  **non-vacuous, recognized-checker-consumable history** and the **verdict** step, with the
  recognized checker (Elle) and any JVM/Clojure kept **off-Check**. Slice 2 (#405, merged
  `8af8e97`) shipped the **networked observable client** — `ObservableS3Client` drives real
  signed PUT/GET/DELETE over the S3 HTTP wire against a real gateway and records a real-time
  `History`/`OpRecord` (`crates/server/src/consistency_observable.rs`), with one sound local
  check already (`versions_monotone_per_key`, `:105`). What does NOT yet exist, and is this
  slice's gap: (a) a **concurrent** workload — #405's test drives a *single* client
  sequentially, so its history has no overlapping real-time spans and cannot exercise
  concurrency at all; (b) a **checker-compatible serialization** — the recorded history is a
  Rust struct, not the recognized checker's input format, so nothing can be fed to Elle; (c)
  the **session** RYW + monotonic-read checks; (d) the **directory-as-set** namespace history
  (create/delete/membership); (e) the **verdict-dispatch seam** that routes the Elle verdict
  to the privileged off-Check job. Without a concurrent workload the history is vacuous
  (nothing to linearize); without the serializer the recognized checker cannot consume it.
- **Success criterion:** In `cargo xtask ci` (the pure-Rust, container-free, JVM-free gate):
  (a) a **concurrent workload** driving **≥2 concurrent `ObservableS3Client`s** over the real
  in-process S3 HTTP wire + gateway produces a **non-vacuous, well-formed, genuinely
  concurrent** merged multi-process history — **≥2 real-time-overlapping** operation spans
  across **distinct process ids**, the register version **climbing** across overwrites of a
  shared key, and per-key reads observing a **monotone** version sequence — each op tagged
  with its `:process`; (b) the **serializer** emits that history in the recognized checker's
  (**Elle**) EDN operation-history format (`:process` / `:type` ∈ `{:invoke,:ok,:fail}` /
  `:f` / `:value` / `:time`), asserted **byte-exact** against a crafted golden history. (Scope
  note: this in-gate golden assertion proves the serializer is **stable and well-shaped**; it
  does NOT by itself prove real-**Elle-parser** acceptance — that is confirmed only when the
  off-Check verdict job parses the EDN, since Elle is JVM/Clojure and stays off-Check. So (b)
  is a serializer-stability regression, and Elle-schema conformance is part of the deferred
  verdict leg — do not over-read the golden as an Elle-compatibility proof.) (c)
  the **session** read-your-writes and monotonic-read checks **REJECT** a crafted violating
  single-session history and **ACCEPT** a valid one, and the reused per-key monotonicity check
  **REJECTS** a crafted version-regression history; (d) a **directory-as-set** history
  (create=PUT / delete=DELETE / membership=GET-probe under a prefix — **no rename, no wire
  LIST**; see Scope) is recorded and serializes to the checker's list-append/set op form; and
  (e) a **verdict-dispatch** value routes the Elle linearizability verdict to the **privileged
  off-Check job** (representable + unit-tested, never shelling JVM/Clojure into `ci`),
  mirroring `metadata_faults.rs`. **The in-gate slice does NOT itself return a register- or
  namespace-linearizability verdict** — that is Elle's, off-Check, over the SAME serialized
  history. Each of (b), (c), (e) — and the serialization half of (d) — is a **socket-free,
  flippable, module-weakening red→green** assertion in the named test file; (a)'s
  non-vacuity/concurrency witnesses are asserted on the produced history (green confirmed in
  real CI where loopback bind is permitted; see Falsifiability).
- **Falsifiability:** RED is producible **on the plain `cargo xtask ci` environment** — no
  cluster, and no live socket for the load-bearing reds. The **core reds are socket-free**:
  (b) feed the serializer a crafted history and assert the exact EDN bytes — a wrong field or
  op-kind mapping makes it RED, and it stays RED whenever the serializer is weakened; (c) feed
  the session RYW/monotonic-read checks a crafted *violating* history and assert rejection
  (RED whenever the check is weakened, module still compiles), and feed the reused
  `versions_monotone_per_key` a regressing history and assert rejection; (e) assert the
  verdict-dispatch value routes to the off-Check leg for the default inputs (RED if re-pointed
  at an in-gate JVM shell-out), exactly the `metadata_faults.rs` shape. These give a
  **demonstrated red on real inputs**, not a red resting on non-existence. Leg (a) — the
  **wire-driven concurrent workload** — binds `127.0.0.1:0` and drives real HTTP, exactly as
  #405's merged test `crates/server/tests/consistency_observable.rs` and
  `crates/server/tests/s3_http_wire.rs` already do; its green is confirmed by the full
  `cargo xtask ci`. The gateway serves connections **concurrently** (`S3Gateway::serve`
  → `axum::serve`, `crates/gateway-s3/src/lib.rs:158`, per-connection tasks), so ≥2
  concurrent clients genuinely produce **overlapping** real-time spans — the "≥2 overlapping
  spans" witness is reachable, not aspirational. **Plan note on the Check sandbox:** the iteration-2 Check observed that the
  Check *sandbox* can deny loopback `bind` (an unrelated gRPC test failed on it). This slice is
  built so that does **not** block RED: the (b)/(c)/(d-serialize)/(e) reds need **no socket**,
  so a flippable RED is always producible even under a bind-restricted sandbox, and leg (a)'s
  green is the real-CI observation (Verification posture). There is therefore **no
  Plan-blocking falsifiability gap**.
- **Invariant to restore:** N/A — net-new functionality (principle 1.3): there is no prior
  correct behaviour to restore. The governing rule instead is ADR-0041's **division of
  labour**: the recognized checker (Elle) owns the linearizability *verdict* (off-Check); the
  Rust slice owns *history production, recording, and serialization* plus only *sound, local*
  invariants. Do MUST keep to this division — re-deriving a global register-linearizability
  decision in-gate is the rejected v1–v3 vehicle. Source: ADR-0041 §Decision (verdict engine
  SHOULD be Elle/recognized; workload driver MAY be Rust emitting checker-compatible history;
  checker + JVM/Clojure MUST run only off-Check) and ADR-0016 (privileged tiers stay out of
  `cargo xtask ci`).
- **Repo + branch target:** getwyrd/wyrd @ feat/m4-production-metadata-backend
  (M4 slice — stacks on the integration branch per INTEGRATION §2, not `main`; #405 already
  landed there at `8af8e97`, tip `a7c7408`)
- **Depends on:** (none in-batch — see Ordering note)
- **Ordering note:** The build-on prereq **#405 has MERGED** into the target branch
  (`8af8e97` "Fixes #405"), so there is no wave dependency to set: this slice builds directly
  on the landed `crates/server/src/consistency_observable.rs`. #329's slice order is #405
  (networked observable, slice 2, **MERGED**) → **#406 (this, slice 3)** → #407 (partition/
  skew/pause nemesis over the M4 cluster, slice 4, **OPEN**, out of scope). The live Elle
  verdict and the real-cluster nemesis run consume this slice's serialized history downstream.
- **Surfaces:** data
- **Difficulty:** high — a net-new consistency-checker deliverable spanning a concurrent
  multi-client workload, a multi-process history merge + `:process` tagging, an Elle-EDN
  serializer, session checks, a directory-as-set history, and an off-Check verdict seam across
  `crates/server/` (+ tests) and `xtask/`. Ripples little into existing call sites, but a
  reviewer must hold the whole checker-substrate design and the in-gate/off-Check boundary in
  view — and this is a re-plan of a bundle that already burned three iterations, so route to
  the strongest Do backend and deepest review (rated up per the safe-default rule).
- **Scope:** Build, on top of the merged #405 observable, (1) a **concurrent workload driver**
  that spins **≥2 `ObservableS3Client`s** as concurrent tasks driving overwriting PUT + GET on
  a small shared key set over the real in-process S3 HTTP wire + gateway, producing genuinely
  overlapping real-time spans; (2) a **multi-process history** that merges the per-client
  `History`s into one real-time-ordered log with a `:process` id per client; (3) an
  **Elle-EDN serializer** (`History` → the recognized checker's operation-history format); (4)
  the **session** read-your-writes + monotonic-read checks over the register (and, where the
  workload observes it, `meta:version`), as *sound, local* invariants; (5) a **directory-as-set**
  history — create=PUT / delete=DELETE / membership=GET-probe (200 present / 404 absent) under
  a prefix — recorded and serialized to the checker's list-append/set op form; (6) the
  **verdict-dispatch seam** (a pure routing value, unit-tested, that sends the Elle verdict to
  the privileged off-Check job), mirroring `xtask/src/metadata_faults.rs`. Model on the
  **mutable metadata register** the wire exposes (an object key's overwritten value/version;
  namespace membership under a prefix), per ADR-0041 — **never** the immutable chunk/fragment
  path (the vacuous-history mistake ADR-0041 rejects). / out of scope: **any in-gate
  register/namespace linearizability verdict** (Elle owns it, off-Check — this is the rejected
  v1–v3 vehicle; do not rebuild it); **directory rename and a wire `LIST` verb** (the S3 wire
  floor is PUT/GET/DELETE only, `crates/gateway-s3/src/lib.rs:40` — model the directory as a
  set via PUT/DELETE/GET-probe; rename and LIST are a later slice once the wire grows them);
  the **execution** of the live Elle/JVM verdict inside CI (off-Check per ADR-0016/ADR-0041 —
  no JVM/Clojure in `cargo xtask ci`); the **real-cluster partition/skew/pause nemesis** run
  (#407, reusing #257); any change to the **gateway, `MetadataStore`, or the S3 wire surface**
  (this slice is a *consumer* of the merged #405 client and the existing wire); any cross-zone
  / `meta:version` failover-fence strengthening (M10/M11).
- **Repro instruction:** On `feat/m4-production-metadata-backend`, read
  `docs/design/adr/0041-consistency-checker-substrate.md` (targeting + the off-Check verdict
  constraint), `docs/design/adr/0015-consistency-contract.md:22-25` (the three guarantees),
  the merged #405 substrate `crates/server/src/consistency_observable.rs` (the client +
  `History`/`OpRecord` this slice extends) and its single-client sequential test
  `crates/server/tests/consistency_observable.rs`, and the concurrent-wire driving pattern in
  `crates/server/tests/s3_http_wire.rs` (`start_gateway:63`, `send:104`, `signed_headers:86`,
  `parse_response:133`). There is no prior concurrent workload / serializer / session-checks /
  verdict-seam to run — this slice creates them; "reproduction" is the absence of a concurrent,
  non-vacuous, Elle-serializable history and of the session/serializer/verdict machinery.
- **External dependencies:** **none for the Check-exercised core** — the workload, history
  merge, Elle-EDN serializer, session checks, directory-as-set history, and verdict-dispatch
  value are pure Rust + a loopback TCP listener + the in-process gateway (redb in-memory + fs
  temp + mem coordination), all under the base toolchain in `cargo xtask ci`, exactly as
  #405's merged test already runs. The **Elle verdict** step needs a recognized checker (Elle
  → JVM/Clojure, or an equivalent) which per ADR-0041/ADR-0016 runs **only in a privileged
  off-Check job and MUST NOT enter `cargo xtask ci`** — so it is **not** a build/verify
  dependency of this bundle at Check. Do MUST NOT pull a JVM/Clojure dependency into the merge
  gate; serialize to Elle's EDN input schema in Rust and defer the verdict execution off-Check.
  (Sandbox caveat, not a dependency: loopback `bind` may be denied in the Check sandbox — the
  core reds are socket-free so this does not block RED; see Falsifiability.)
- **Test file:** `crates/server/tests/consistency_workload.rs` (new) — the load-bearing,
  C4-verify-flippable artifact, run by `cargo xtask ci`. It carries BOTH the **socket-free
  crafted-history reds** (serializer golden-bytes; session RYW/monotonic-read + monotonicity
  rejection; directory-as-set serialization; verdict-dispatch routing) AND the **wire-driven
  concurrent-workload** non-vacuity/concurrency assertions. Do MAY additionally house pure
  functions (serializer, session checks, verdict-dispatch) with `#[cfg(test)]` unit tests in
  their module, but the named integration test is the regression of record.
- **Verification posture:** DEFERRED / net-new — declared so C2/C4 land as a pre-declared
  sign-off item, not a surprise NEEDS-HUMAN. **What IS built AND exercised at Check** (not
  inert scaffolding): the concurrent workload + multi-process history merge (exercised by
  producing and asserting a non-vacuous, genuinely-concurrent history), the Elle-EDN serializer
  (byte-exact golden red→green), the session RYW/monotonic-read + per-key monotonicity checks
  (crafted-violation rejection reds), the directory-as-set history + its serialization, and the
  verdict-dispatch routing value (unit red→green). "Red" here is NOT rested on non-existence —
  each socket-free assertion is a genuine, flippable, module-weakening red on real inputs.
  **What is DEFERRED off-Check:** (i) the **live Elle/recognized-checker verdict** over the
  serialized history — its green is observable only in the privileged off-Check job
  (ADR-0041/ADR-0016), confirmed by the maintainer / nightly job, over the SAME history the
  Check-exercised serializer produces (not inert scaffolding); (ii) the **wire-workload leg's
  green** MAY be limited if the Check *sandbox* denies loopback `bind` — in that case the
  socket-free reds still provide the flippable RED and the workload green is confirmed by the
  real `cargo xtask ci` / CI (which permits loopback bind, as it does for #405's merged test).
  This slice is not mere dispatch plumbing: the workload, serializer, session checks, and
  directory history are functionally implemented and exercised here; only the *recognized-checker
  verdict execution* — which ADR-0041 mandates be off-Check — is deferred.
- **Production reach:** This slice produces and serializes the history that the recognized
  checker consumes; it builds the checker seam ahead of the live verdict. **What honours the
  seam now:** the concurrent workload drives the **real production S3 HTTP wire +
  `wyrd_server::Gateway` commit path** via the merged #405 `ObservableS3Client` (real signed
  HTTP, real overwriting commits that bump the register version, real reads) — load-bearing,
  not a mock or a re-implementation. **What the full artifact still does downstream:** the
  **live Elle verdict** on the serialized history (off-Check job) and the **fault-injected run
  on the real M4 cluster** (#407, reusing #257's partition/skew/pause nemesis) are not present
  here; they consume this slice's serialized history. A recognized-checker verdict job must
  exist to run Elle over the EDN, and #257's cluster + nemesis before the fault run — both are
  the named later slices.
- **Citations expected:** Do must cite path:line on `feat/m4-production-metadata-backend` for
  every change, and cite ADR-0041 for the register/list-append targeting + the off-Check verdict
  constraint. **Peer callsites Do SHOULD mirror** (composition slice): the merged #405 substrate
  — extend/consume `ObservableS3Client` + `History`/`OpRecord` +
  `versions_monotone_per_key` in `crates/server/src/consistency_observable.rs`
  (`OpRecord:55`, `History:84`, `versions_monotone_per_key:105`, `ObservableS3Client:127`,
  `put:157`, `get:176`, `delete:199`); drive concurrent signed HTTP over the wire as
  `crates/server/tests/s3_http_wire.rs` does (`start_gateway:63`, `send:104`,
  `signed_headers:86`, `parse_response:133`) and as the single-client
  `crates/server/tests/consistency_observable.rs` does; and mirror the **"deferred ≠ unbuilt"
  pure-routing-value + off-Check-leg** seam in `xtask/src/metadata_faults.rs` (the
  `MetadataTierDispatch` enum `:40` + `metadata_tier_dispatch` pure fn `:53`) for the
  verdict-dispatch value. Do MAY open these cited callsites to copy the composition.
- **Prior-art check (triage cycles):** Searched `feat/m4-production-metadata-backend` for
  `jepsen|elle|checker|register|list_append|history|consistency`. **#405 has merged** (`8af8e97`,
  "Fixes #405"), shipping `crates/server/src/consistency_observable.rs` (+ its test) — the
  observable client + history recorder this slice builds on; it does NOT provide the concurrent
  workload, the Elle serializer, session checks, the directory-as-set history, or the verdict
  seam. The rejected iteration-3 testkit module `crates/testkit/src/consistency.rs` was **never
  merged** (only `crates/testkit/{Cargo.toml,src/lib.rs}` exist on the branch) — clean slate, no
  hand-rolled verdict to inherit or resurrect. The ADR-0039 in-repo scenario
  (`.github/workflows/tier1-jepsen.yml`, `crates/chunkstore-grpc/tests/tier1_jepsen_consistency.rs`,
  `crates/metadata-tikv/tests/tier1_metadata_consistency.rs`) watches the **immutable repair
  path** on purpose (ADR-0041 §Consequences) — a different layer, not this mutable-register
  history; no duplication. Parent #329 is CLOSED (re-sliced into #405/#406/#407); ADR-0041
  accepted via #410 (CLOSED). Not a duplicate; a genuine build-on of #405.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Iteration 4 — carry-forward (from the previous attempt)
- Sign-off rationale: Fitness-to-purpose not yet met. Two tightenings required before accept: (1) The concurrency witness is over-broad — it counts read↔read overlaps as "genuine concurrency," so it can pass with zero read/write overlap. Require at least one read span overlapping a write span (the only overlap non-vacuous for register linearizability). (2) The serializer bakes indeterminate outcomes into definite ones — 5xx/timeout writes map to :fail and failed membership probes to definitely-absent. Map these to Elle's :info (unknown) instead, or the false-accept class (the exact fault that sank v1–v3) is merely relocated downstream into the off-Check Elle job. Also add a delete-then-read (RYW) crafted case, since session_read_your_writes currently clears its obligation on DELETE and that branch is untested. C4 Verification and T3 Runtime loopback caveats already cleared at sign-off (xtask ci passed; adversary ran all 8 tests green including the 2 wire-driven).
- Full previous attempt preserved in `iteration-v4/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 5 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected on the adversary's two refutations (both verified against patch.diff). Not fitness-to-purpose yet. The two iteration-4 tightenings were each fixed only at the exact spot flagged, then the SAME fault-class reappeared in an adjacent path. Re-scope both as surface-wide invariants, not point fixes, so the next Do closes the class rather than the instance: 1. Indeterminate-never-yields-a-definite-obligation must hold EVERYWHERE, not just in the serializer. `session_read_your_writes` establishes definite obligations from indeterminate mutations: the Put arm (AtLeast(v)) and Delete arm (Absent) do NOT guard is_indeterminate(op.status); only the Get arm does. Because ObservableS3Client::put records version: Some(_) on every PUT regardless of wire status, a 5xx write followed by a valid older read is FALSE-REJECTED — the same "bake indeterminate into definite" fault that sank v1-v3, relocated from the serializer :fail (fixed at iter-4) to the obligation-establishing side (missed). Plan the invariant across the whole check surface, and add a crafted [PUT k v=2 status=500; GET k v=1 status=200] (and the delete analogue) that must be ACCEPTED. 2. The genuine-concurrency witness must be per-register (same key). `read_write_overlapping_pairs_across_processes` tests process + span-overlap + read/write kind but NOT a.record.key == b.record.key, so a cross-key read↔write overlap counts as "genuinely concurrent" — vacuous for any single register, the same vacuity class the iter-4 read↔read tightening targeted. Require same-key overlap and add a crafted cross-key-only history that must NOT pass as concurrent. (The shipped single-key workload masks this, so a socket-free crafted red is needed.) C4/T3 loopback and T4 tracker items were NOT the basis for rejection (gate green; adversary re-ran 11/11 incl. both wire tests). Do NOT re-attempt the iter-4 approach unchanged — generalise the two tightenings.
- Full previous attempt preserved in `iteration-v5/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
