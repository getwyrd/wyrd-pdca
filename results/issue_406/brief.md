# Brief — issue 406 / consistency-workload-history-and-elle-serialization

> The Plan artifact (docs 02 §PLAN). Human-authored. Do reads ONLY this file.
> The field labels below are parsed by the driver — keep the `- **Label:** value`
> shape. The success criterion is the load-bearing field: it is the sentence
> Check tests "did this work" against.
>
> NET-NEW functionality (not a bug fix): the "Defect" field states the GAP/need, and
> the minimalism-of-diff maxim does not govern (principle 1.3). But this slice DOES carry
> two governing **soundness invariants** (see Invariant field) — the recurring failure
> across every prior attempt was these being enforced point-wise, not surface-wide. This
> slice implements the **accepted** ADR-0041 (acceptance issue #410, CLOSED — accepted on
> `origin/main`; the `feat/m4-*` branch carries a stale `status: Proposed` frontmatter
> because the branch was cut before the acceptance commit landed, but the Decision text is
> the same), so no new design proposal is needed.
>
> **RE-PLAN — iteration 6 (supersedes iterations v1–v5 — read this).** History:
> v1–v3 built a **hand-rolled in-gate register-linearizability verdict** in pure Rust and
> were rejected each time for FALSE-ACCEPTING an inconsistent history — v3 dropped that
> vehicle entirely (Elle owns the verdict, off-Check). v4 built the full substrate and was
> rejected on fitness tightenings. v5 fixed the two load-bearing tightenings **only at the
> exact spot flagged**, and the **SAME two fault-classes reappeared in adjacent arms** of the
> surface — so v5 was rejected again. The human sign-off directs: **re-scope both tightenings
> as surface-wide invariants, not point fixes, so this Do closes the CLASS, not the instance.**
> Those two invariants are now stated as `INV-1` / `INV-2` (Invariant field) and are the
> load-bearing addition of this iteration. Everything else about the v4/v5 structure was
> sound and is carried forward: a `wyrd-server` module beside the merged #405
> `consistency_observable.rs`; named integration test `crates/server/tests/consistency_workload.rs`;
> Elle owns the global verdict off-Check; **Do MUST NOT re-implement a global register/namespace
> linearizability decision in `cargo xtask ci`** (the rejected v1–v3 vehicle).

- **Slug:** consistency-workload-history-and-elle-serialization
- **Defect:** (gap / need) ADR-0041 §Decision names three deliverables for #329's
  consistency artifact: (1) the read-write **register** model, (2) the **directory
  list-append/set** model, (3) **session** read-your-writes + monotonic-read checks — plus a
  **non-vacuous, recognized-checker-consumable history** and the **verdict** step, with the
  recognized checker (Elle) and any JVM/Clojure kept **off-Check**. Slice 2 (#405, PR #466,
  landed on the branch at `8af8e97`) shipped the **networked observable client** —
  `ObservableS3Client` drives real signed PUT/GET/DELETE over the S3 HTTP wire against a real
  gateway and records a real-time `History`/`OpRecord` (`crates/server/src/consistency_observable.rs`),
  with one sound local check already (`versions_monotone_per_key`, `:105`). (Issue #405 is
  still OPEN on the tracker only because M4 slices merge into the integration branch, not
  `main`, so GitHub's auto-close has not fired — the substrate code IS present on the branch
  this slice builds on.) What does NOT yet exist, and is this slice's gap: (a) a **concurrent**
  workload — #405's test drives a *single* client sequentially, so its history has no
  overlapping real-time spans and cannot exercise concurrency at all; (b) a
  **checker-compatible serialization** — the recorded history is a Rust struct, not the
  recognized checker's input format, so nothing can be fed to Elle; (c) the **session** RYW +
  monotonic-read checks; (d) the **directory-as-set** namespace history (create/delete/
  membership); (e) the **verdict-dispatch seam** that routes the Elle verdict to the
  privileged off-Check job. Without a concurrent workload the history is vacuous (nothing to
  linearize); without the serializer the recognized checker cannot consume it. And — the
  reason this is a re-plan — every prior attempt's local checks/serializer/witness leaked one
  of two soundness faults (INV-1/INV-2 below) into an un-audited arm.
- **Success criterion:** In `cargo xtask ci` (the pure-Rust, container-free, JVM-free gate):
  (a) a **concurrent workload** driving **≥2 concurrent `ObservableS3Client`s** over the real
  in-process S3 HTTP wire + gateway produces a **non-vacuous, well-formed, genuinely
  concurrent** merged multi-process history — **≥1 real-time-overlapping SAME-KEY read↔write
  span pair across distinct process ids** (the only overlap non-vacuous for a single register,
  INV-2), the register version **climbing** across overwrites of a shared key, and per-key
  reads observing a **monotone** version sequence — each op tagged with its `:process`; AND
  two **socket-free crafted negative** witnesses hold: a **cross-key-only** history
  (e.g. `[P1 GET "a" @ [0,10]; P0 PUT "b" @ [5,15]]`) is **NOT** reported genuinely concurrent,
  and a **read↔read-only** history is **NOT** either. (b) the **serializer** emits that history
  in the recognized checker's (**Elle**) EDN operation-history format
  (`:process` / `:type` ∈ `{:invoke,:ok,:fail,:info}` / `:f` / `:value` / `:time`), with every
  **indeterminate** wire outcome mapped to `:info` (never a definite `:ok`/`:fail`), asserted
  **byte-exact** against a crafted golden history. (Scope note: this in-gate golden proves the
  serializer is **stable and well-shaped**; it does NOT by itself prove real-**Elle-parser**
  acceptance — that is confirmed only when the off-Check verdict job parses the EDN, since Elle
  is JVM/Clojure and stays off-Check. So (b) is a serializer-stability regression, and
  Elle-schema conformance is part of the deferred verdict leg — do not over-read the golden as
  an Elle-compatibility proof.) (c) the **session** read-your-writes and monotonic-read checks
  and the reused per-key monotonicity check are **sound surface-wide** (INV-1): they **REJECT**
  crafted *determinate* violating histories — a determinate resurrect-after-own-delete, a
  determinate version regression — AND **ACCEPT** crafted valid histories where the only
  ordering-relevant mutation is **indeterminate**: specifically `[PUT k v=2 status=500; GET k
  v=1 status=200]` (an indeterminate write must not create a definite `AtLeast(2)` obligation)
  and `[PUT k v=1 status=200; DELETE k status=500; GET k v=1 status=200]` (an indeterminate
  delete must not create a definite `Absent` obligation), and an indeterminate GET must not be
  counted as a monotonicity/RYW *violation*. (d) a **directory-as-set** history (create=PUT /
  delete=DELETE / membership=GET-probe under a prefix — **no rename, no wire LIST**; see Scope)
  is recorded and serializes to the checker's list-append/set op form, with indeterminate
  probes mapped to `:info` (no fabricated `[member false]`). (e) a **verdict-dispatch** value
  routes the Elle linearizability verdict to the **privileged off-Check job** (representable +
  unit-tested, never shelling JVM/Clojure into `ci`), mirroring `metadata_faults.rs`. **The
  in-gate slice does NOT itself return a register- or namespace-linearizability verdict** —
  that is Elle's, off-Check, over the SAME serialized history. Each of (b), (c), (e), the
  negative witnesses of (a), and the serialization half of (d) is a **socket-free, flippable,
  module-weakening red→green** assertion in the named test file; (a)'s positive
  non-vacuity/concurrency witnesses are asserted on the produced history (green confirmed in
  real CI where loopback bind is permitted; see Falsifiability).
- **Falsifiability:** RED is producible **on the plain `cargo xtask ci` environment** — no
  cluster, and no live socket for the load-bearing reds. The **core reds are socket-free**:
  (a-negative) feed `is_genuinely_concurrent` a crafted **cross-key-only** and a
  **read↔read-only** history and assert it returns `false` (RED whenever the witness stops
  requiring same-key read↔write — the exact v4/v5 leak); (b) feed the serializer a crafted
  history with an indeterminate op and assert the exact EDN bytes carry `:info`, not a definite
  outcome (RED on any wrong field / op-kind / indeterminate-baked-to-definite mapping); (c)
  feed the session checks the crafted **valid-but-indeterminate** histories above and assert
  **ACCEPT** (RED whenever an obligation-establishing arm — PUT→`AtLeast`, DELETE→`Absent`, or
  the read/monotonic side — fails to guard the indeterminate status), and feed the determinate
  resurrect-after-delete / version-regression histories and assert **REJECT**; (e) assert the
  verdict-dispatch value routes to the off-Check leg for the default inputs (RED if re-pointed
  at an in-gate JVM shell-out), exactly the `metadata_faults.rs` shape. These give a
  **demonstrated red on real inputs**, not a red resting on non-existence. Leg (a)'s positive
  witness — the **wire-driven concurrent workload** — binds `127.0.0.1:0` and drives real HTTP,
  exactly as #405's landed test `crates/server/tests/consistency_observable.rs` and
  `crates/server/tests/s3_http_wire.rs` already do; its green is confirmed by the full
  `cargo xtask ci`. The gateway serves connections **concurrently** (`S3Gateway::serve`
  → `axum::serve`, `crates/gateway-s3/src/lib.rs:157`, per-connection tasks), so ≥2 concurrent
  clients on a **shared key** genuinely produce **overlapping same-key read↔write** spans — the
  witness is reachable, not aspirational. **Plan note on the Check sandbox:** the iteration-2
  Check observed the Check *sandbox* can deny loopback `bind` (an unrelated gRPC test failed on
  it). This slice is built so that does **not** block RED: the (a-negative)/(b)/(c)/(d-serialize)/
  (e) reds need **no socket**, so a flippable RED is always producible even under a
  bind-restricted sandbox, and leg (a)'s positive green is the real-CI observation (Verification
  posture). There is therefore **no Plan-blocking falsifiability gap**.
- **Invariant to restore:** N/A as *restoration* — net-new functionality (principle 1.3), no
  prior correct behaviour to restore. But TWO **soundness invariants** govern the whole checker
  surface and are the operative rules this fix must make hold EVERYWHERE (the recurring
  class-failure across v1–v5 is precisely these enforced point-wise). SELF-TEST: neither can be
  satisfied by guarding a single arm — Do MUST audit every arm named below.
  - **INV-1 (no fabricated certainty — soundness):** No function on the checker surface may
    convert an **indeterminate** wire outcome (5xx / timeout / synthetic-0 status / any outcome
    where the effect is unknown) into a **definite** obligation, outcome, or membership claim.
    An indeterminate op is `:info` ("may or may not have happened") in Jepsen/Elle semantics; a
    local check must **SKIP** it (an indeterminate op neither proves nor refutes an obligation).
    This holds across the ENTIRE surface, not one arm: (i) the serializer completion-type
    mapping (register + directory); (ii) the RYW **obligation-establishing** arms — PUT→
    `AtLeast(v)` and DELETE→`Absent` **must** guard `is_indeterminate(status)`, not only the Get
    arm; (iii) the RYW read side; (iv) `session_monotonic_reads` (guard indeterminate the same
    way, not only via `version == None`); (v) directory membership derivation
    (`200→present`, `404→absent`, everything else→**unknown**, never definitely-absent).
    Source: Jepsen/Elle `:info` semantics; ADR-0041 — this false-accept is *the exact fault*
    the whole substrate exists to prevent (it sank the earlier Clojure attempt and v1–v3, and
    v4/v5 kept relocating it to an un-audited arm).
  - **INV-2 (non-vacuity — a witness must constrain a single register):** A concurrency/overlap
    witness may count an overlap ONLY when it imposes an ordering constraint on **one register**:
    **same-key** (`a.record.key == b.record.key`), **read↔write**, across **distinct processes**.
    Read↔read overlaps and cross-key overlaps place no constraint on any single register and are
    **vacuous** — they must NOT count as genuine concurrency. Source: register-linearizability
    semantics; ADR-0041's non-vacuity requirement (the vacuous-history failure it rejects).
  - **Division of labour (ADR-0041, unchanged):** the recognized checker (Elle) owns the
    linearizability *verdict* (off-Check); the Rust slice owns *history production, recording,
    serialization* plus only *sound, local* invariants. Do MUST keep to this division —
    re-deriving a global register/namespace-linearizability decision in-gate is the rejected
    v1–v3 vehicle. Source: ADR-0041 §Decision + ADR-0016 (privileged tiers stay out of `cargo
    xtask ci`).
- **Repo + branch target:** getwyrd/wyrd @ feat/m4-production-metadata-backend
  (M4 slice — stacks on the integration branch per INTEGRATION §2, not `main`; #405's code
  landed there at `8af8e97`, tip `a7c7408`)
- **Depends on:** (none in-batch — see Ordering note)
- **Ordering note:** The build-on prereq **#405's code has landed** on the target branch
  (`8af8e97`, PR #466), so there is no wave dependency to set: this slice builds directly on
  the landed `crates/server/src/consistency_observable.rs`. (The #405 *issue* is still OPEN on
  the tracker — an integration-branch artifact, not a missing prereq; the code is present.)
  #329's slice order is #405 (networked observable, slice 2, **landed on branch**) → **#406
  (this, slice 3)** → #407 (partition/skew/pause nemesis over the M4 cluster, slice 4, **OPEN**,
  out of scope). The live Elle verdict and the real-cluster nemesis consume this slice's
  serialized history downstream.
- **Surfaces:** data
- **Difficulty:** high — a net-new consistency-checker deliverable spanning a concurrent
  multi-client workload, a multi-process history merge + `:process` tagging, an Elle-EDN
  serializer, session checks, a directory-as-set history, and an off-Check verdict seam across
  `crates/server/` (+ tests) and `xtask/`. Ripples little into existing call sites, but a
  reviewer must hold the whole checker-substrate design AND the two surface-wide soundness
  invariants (INV-1/INV-2, which must be audited across every arm) and the in-gate/off-Check
  boundary in view — and this is a re-plan of a bundle that already burned FIVE iterations on
  exactly a point-vs-class scoping failure, so route to the strongest Do backend and deepest
  review (rated up per the safe-default rule).
- **Scope:** Build, on top of the landed #405 observable, (1) a **concurrent workload driver**
  that spins **≥2 `ObservableS3Client`s** as concurrent tasks driving overwriting PUT + GET on
  a small shared key set over the real in-process S3 HTTP wire + gateway, producing genuinely
  overlapping **same-key read↔write** real-time spans; (2) a **multi-process history** that
  merges the per-client `History`s into one real-time-ordered log with a `:process` id per
  client; (3) an **Elle-EDN serializer** (`History` → the recognized checker's operation-history
  format); (4) the **session** read-your-writes + monotonic-read checks over the register (and,
  where the workload observes it, `meta:version`), as *sound, local* invariants; (5) a
  **directory-as-set** history — create=PUT / delete=DELETE / membership=GET-probe (200 present
  / 404 absent) under a prefix — recorded and serialized to the checker's list-append/set op
  form; (6) the **verdict-dispatch seam** (a pure routing value, unit-tested, that sends the
  Elle verdict to the privileged off-Check job), mirroring `xtask/src/metadata_faults.rs`.
  Model on the **mutable metadata register** the wire exposes (an object key's overwritten
  value/version; namespace membership under a prefix), per ADR-0041 — **never** the immutable
  chunk/fragment path (the vacuous-history mistake ADR-0041 rejects).
  **Enforce INV-1 and INV-2 SURFACE-WIDE, not at the two spots the prior sign-offs named.**
  Before writing crafted reds, ENUMERATE every function on the checker surface that (α)
  establishes an obligation or maps a wire outcome to a completion-type/membership, and audit
  each against INV-1 — at minimum: the register serializer completion-type, the directory
  serializer completion-type + membership derivation, the RYW PUT-arm and DELETE-arm obligation
  establishment, the RYW read side, and `session_monotonic_reads`; and (β) counts an
  overlap/concurrency witness, and audit each against INV-2 — at minimum
  `read_write_overlapping_pairs_across_processes` / `is_genuinely_concurrent`. Ship one crafted
  socket-free red per audited arm so the CLASS goes red if it reappears anywhere, not just at
  the historical spot. / out of scope: **any in-gate register/namespace linearizability
  verdict** (Elle owns it, off-Check — the rejected v1–v3 vehicle; do not rebuild it);
  **directory rename and a wire `LIST` verb** (the S3 wire floor is PUT/GET/DELETE only,
  `crates/gateway-s3/src/lib.rs:40`,`:347` "only object PUT, GET, and DELETE are supported" —
  model the directory as a set via PUT/DELETE/GET-probe; rename and LIST are a later slice once
  the wire grows them); the **execution** of the live Elle/JVM verdict inside CI (off-Check per
  ADR-0016/ADR-0041 — no JVM/Clojure in `cargo xtask ci`); the **real-cluster partition/skew/
  pause nemesis** run (#407, reusing #257); any change to the **gateway, `MetadataStore`, or the
  S3 wire surface** (this slice is a *consumer* of the landed #405 client and the existing wire);
  any cross-zone / `meta:version` failover-fence strengthening (M10/M11).
  **DECISION POINT for the human (not yet fixed):** the v5 adversary noted `version_climbs_for_key`
  is *tautological* for the shipped single-key workload (it derives "climbs" from the writer's
  caller-supplied loop counter, never a backend-observed value) — a "weaker-evidence" note, NOT
  a refutation. Default here is **out of scope** (it was not a rejection basis). If the human
  wants it strengthened to read a backend-observed GET version, say so and it moves in-scope.
- **Repro instruction:** On `feat/m4-production-metadata-backend`, read
  `docs/design/adr/0041-consistency-checker-substrate.md` (targeting + the off-Check verdict
  constraint; note the branch copy reads `status: Proposed` but the ADR is Accepted on `main`
  via #410 — same Decision), `docs/design/adr/0015-consistency-contract.md:22-24` (the three
  guarantees), the landed #405 substrate `crates/server/src/consistency_observable.rs` (the
  client + `History`/`OpRecord` this slice extends; `version: Some(_)` recorded on EVERY put
  regardless of wire status, `:166` — the reason INV-1 must guard the obligation side) and its
  single-client sequential test `crates/server/tests/consistency_observable.rs`, and the
  concurrent-wire driving pattern in `crates/server/tests/s3_http_wire.rs` (`start_gateway:63`,
  `send:104`, `signed_headers:86`, `parse_response:133`). There is no prior concurrent workload
  / serializer / session-checks / verdict-seam to run — this slice creates them; "reproduction"
  is the absence of a concurrent, non-vacuous, Elle-serializable history and of the session/
  serializer/verdict machinery, PLUS the crafted socket-free reds that pin INV-1/INV-2.
- **External dependencies:** **none for the Check-exercised core** — the workload, history
  merge, Elle-EDN serializer, session checks, directory-as-set history, and verdict-dispatch
  value are pure Rust + a loopback TCP listener + the in-process gateway (redb in-memory + fs
  temp + mem coordination), all under the base toolchain in `cargo xtask ci`, exactly as
  #405's landed test already runs. The **Elle verdict** step needs a recognized checker (Elle
  → JVM/Clojure, or an equivalent) which per ADR-0041/ADR-0016 runs **only in a privileged
  off-Check job and MUST NOT enter `cargo xtask ci`** — so it is **not** a build/verify
  dependency of this bundle at Check. Do MUST NOT pull a JVM/Clojure dependency into the merge
  gate; serialize to Elle's EDN input schema in Rust and defer the verdict execution off-Check.
  (Sandbox caveat, not a dependency: loopback `bind` may be denied in the Check sandbox — the
  core reds are socket-free so this does not block RED; see Falsifiability.)
- **Test file:** `crates/server/tests/consistency_workload.rs` (new) — the load-bearing,
  C4-verify-flippable artifact, run by `cargo xtask ci`. It carries BOTH the **socket-free
  crafted-history reds** (serializer golden-bytes incl. `:info`; session RYW/monotonic-read
  ACCEPT-valid-indeterminate + REJECT-determinate-violation; per-key monotonicity rejection;
  directory-as-set serialization; concurrency witness cross-key/read↔read negatives;
  verdict-dispatch routing) AND the **wire-driven concurrent-workload** non-vacuity/same-key-
  concurrency assertions. Do MAY additionally house pure functions (serializer, session checks,
  witness, verdict-dispatch) with `#[cfg(test)]` unit tests in their module, but the named
  integration test is the regression of record.
- **Verification posture:** DEFERRED / net-new — declared so C2/C4 land as a pre-declared
  sign-off item, not a surprise NEEDS-HUMAN. **What IS built AND exercised at Check** (not inert
  scaffolding): the concurrent workload + multi-process history merge (exercised by producing
  and asserting a non-vacuous, genuinely-concurrent same-key history), the Elle-EDN serializer
  (byte-exact golden red→green, incl. `:info`), the session RYW/monotonic-read + per-key
  monotonicity checks (crafted ACCEPT-valid-indeterminate and REJECT-determinate-violation
  reds), the directory-as-set history + its serialization, the concurrency-witness negatives,
  and the verdict-dispatch routing value (unit red→green). "Red" here is NOT rested on
  non-existence — each socket-free assertion is a genuine, flippable, module-weakening red on
  real inputs. **What is DEFERRED off-Check:** (i) the **live Elle/recognized-checker verdict**
  over the serialized history — its green is observable only in the privileged off-Check job
  (ADR-0041/ADR-0016), confirmed by the maintainer / nightly job, over the SAME history the
  Check-exercised serializer produces (not inert scaffolding); (ii) the **wire-workload leg's
  green** MAY be limited if the Check *sandbox* denies loopback `bind` — in that case the
  socket-free reds still provide the flippable RED and the workload green is confirmed by the
  real `cargo xtask ci` / CI (which permits loopback bind, as it does for #405's landed test).
  This slice is not mere dispatch plumbing: the workload, serializer, session checks, directory
  history, and witness are functionally implemented and exercised here; only the
  *recognized-checker verdict execution* — which ADR-0041 mandates be off-Check — is deferred.
- **Production reach:** This slice produces and serializes the history that the recognized
  checker consumes; it builds the checker seam ahead of the live verdict. **What honours the
  seam now:** the concurrent workload drives the **real production S3 HTTP wire +
  `wyrd_server::Gateway` commit path** via the landed #405 `ObservableS3Client` (real signed
  HTTP, real overwriting commits that bump the register version, real reads) — load-bearing,
  not a mock or a re-implementation. **What the full artifact still does downstream:** the
  **live Elle verdict** on the serialized history (off-Check job) and the **fault-injected run
  on the real M4 cluster** (#407, reusing #257's partition/skew/pause nemesis) are not present
  here; they consume this slice's serialized history. A recognized-checker verdict job must
  exist to run Elle over the EDN, and #257's cluster + nemesis before the fault run — both are
  the named later slices.
- **Citations expected:** Do must cite path:line on `feat/m4-production-metadata-backend` for
  every change, and cite ADR-0041 for the register/list-append targeting + the off-Check verdict
  constraint. **Peer callsites Do SHOULD mirror** (composition slice): the landed #405 substrate
  — extend/consume `ObservableS3Client` + `History`/`OpRecord` + `versions_monotone_per_key` in
  `crates/server/src/consistency_observable.rs` (`OpKind:40`, `OpRecord:55`, `History:84`,
  `versions_monotone_per_key:105`, `ObservableS3Client:127`, `put:157` recording
  `version: Some(_):166`, `get:176`, `delete:199`); drive concurrent signed HTTP over the wire
  as `crates/server/tests/s3_http_wire.rs` does (`start_gateway:63`, `send:104`,
  `signed_headers:86`, `parse_response:133`, `TcpListener::bind("127.0.0.1:0"):77`) and as the
  single-client `crates/server/tests/consistency_observable.rs` does; and mirror the
  **"deferred ≠ unbuilt" pure-routing-value + off-Check-leg** seam in `xtask/src/metadata_faults.rs`
  (the `MetadataTierDispatch` enum `:40` + `metadata_tier_dispatch` pure fn `:53`) for the
  verdict-dispatch value. Do MAY open these cited callsites to copy the composition.
- **Prior-art check (triage cycles):** Searched `feat/m4-production-metadata-backend` for
  `jepsen|elle|checker|register|list_append|history|consistency`. **#405's code has landed**
  (`8af8e97`, PR #466), shipping `crates/server/src/consistency_observable.rs` (+ its test) —
  the observable client + history recorder this slice builds on; it does NOT provide the
  concurrent workload, the Elle serializer, session checks, the directory-as-set history, or the
  verdict seam. `crates/server/src/consistency_workload.rs` and
  `crates/server/tests/consistency_workload.rs` do **not** exist on the branch (verified via
  `git -C ../wyrd ls-tree`) — the v4/v5 patches were never committed; **clean slate**, no
  hand-rolled verdict to inherit. The rejected iteration-3 testkit module
  `crates/testkit/src/consistency.rs` was **never merged** (only `crates/testkit/src/lib.rs`
  exists). The ADR-0039 in-repo scenario (`.github/workflows/tier1-jepsen.yml`,
  `crates/chunkstore-grpc/tests/tier1_jepsen_consistency.rs`,
  `crates/metadata-tikv/tests/tier1_metadata_consistency.rs`) watches the **immutable repair
  path** on purpose (ADR-0041 §Consequences) — a different layer, not this mutable-register
  history; no duplication. Parent #329 is CLOSED (re-sliced into #405/#406/#407); ADR-0041
  accepted via #410 (CLOSED). Not a duplicate; a genuine build-on of #405.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Prior-iterations carry-forward (why this is a re-plan — do NOT re-attempt unchanged)

- **v1–v3 (rejected):** hand-rolled in-gate register-linearizability verdict; FALSE-ACCEPTED an
  inconsistent history. Vehicle dropped at v3 — Elle owns the verdict, off-Check. **Do MUST NOT
  rebuild an in-gate global linearizability decision.**
- **v4 (rejected — fitness):** built the full substrate; sign-off required three tightenings —
  (1) concurrency witness must require a read↔write overlap (not read↔read); (2) the serializer
  must map indeterminate outcomes to `:info`, not a definite `:fail`/absent; (3) a delete must
  establish a read-your-own-delete obligation.
- **v5 (rejected — the reason for THIS re-plan):** the v4 tightenings were fixed **only at the
  exact spot flagged**, and the SAME two fault-classes reappeared in adjacent arms:
  **(1) INV-1 leak** — `session_read_your_writes` PUT-arm (`AtLeast`) and DELETE-arm (`Absent`)
  established a *definite* obligation from an *indeterminate* mutation (only the Get arm guarded
  `is_indeterminate`), FALSE-REJECTING `[PUT k v=2 status=500; GET k v=1 status=200]` and the
  delete analogue; `session_monotonic_reads` had the same asymmetry. **(2) INV-2 leak** — the
  concurrency witness tested process + span-overlap + read/write kind but NOT
  `a.record.key == b.record.key`, so a cross-key read↔write overlap counted as genuine
  concurrency (vacuous for a single register). This iteration states both as **surface-wide
  invariants (INV-1/INV-2)** and requires a crafted socket-free red per audited arm. Do NOT
  re-fix only the two named spots — **audit and pin the whole surface.**
