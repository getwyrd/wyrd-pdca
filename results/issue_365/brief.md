# Brief (pointer) — issue 365 / coordination-etcd-l5-backend

> A Plan artifact that is a **pointer**: the planning decision already lives in an
> accepted, governed proposal (0015, superseding 0007) — this file references it and
> carries the fields the driver parses. Do reads the **Planning artifact** as the
> authoritative plan; this brief does not restate it. This is 0015's *named deployment
> prerequisite*, not an M4 metadata slice — it is gated-upon work that M4's slice 5
> (#256) depends on.

- **Slug:** coordination-etcd-l5-backend

- **Planning artifact:** `docs/design/proposals/accepted/0015-milestone-4-production-metadata-backend-revised.md`
  — authoritative. Read specifically the **"Deployment prerequisite (scope boundary)"**
  note (`0015…-revised.md:443-463`) — it *names* this item: "(1) an **etcd-backed
  `Coordination`** implementation (a `coordination-etcd` crate behind the existing trait),
  and (2) **runnable gateway + custodian process roles**." Also read the coordination
  cross-references: the "Coordination (L5) and fencing" bullet (`:236-243` — the metadata
  swap touches neither the trait nor its consumers; only `coordination-mem` exists today),
  the deployment-tier section (`:428-441` — the etcd ensemble backs discovery / leader
  election / fencing per ADR-0006 and §7.1's "3-node etcd"), and slice 5's DoD
  (`:660-711` — "peers discovered through L5" is *gated on this prerequisite*). Ground it
  against the design corpus, read in place under `../wyrd` (never copied): **ADR-0006**
  (etcd for Coordination — the "pin the trait with two implementations" decision), **ADR-0004**
  / M2 (etcd discovery deferred to a composition swap), and the second-implementation-rigor
  argument in **#258** / **#264** (the `MetadataStore` mirror: a trait is pinned only by its
  second real implementation plus a shared contract suite).

- **Defect / goal:** the `Coordination` trait (`crates/traits/src/lib.rs:434`, methods
  `register`/`renew`/`revoke`/`discover`/`elect_leader`/`lock`/`unlock`/`set_config`/
  `get_config`/`config_revision`) has **exactly one implementation** — the process-local
  `coordination-mem` (`crates/coordination-mem/`, #70), whose registrations, elections and
  locks are visible only within a single process. Cross-process L5 discovery therefore does
  not exist, so a real multi-node cluster cannot discover peers, elect a single custodian
  leader, or fence stale holders across machines. Build the **etcd-backed second
  implementation** — a `coordination-etcd` crate implementing the existing trait
  (discovery, leader election, fencing, distributed locks, config) over etcd, selectable by
  config with **no caller edits** — the ADR-0006 REQUIRED second implementation, exactly as
  TiKV is `MetadataStore`'s.

- **Success criterion:** a **shared `Coordination` conformance / contract suite** — lifted
  from the trait-generic assertions that already live in
  `crates/coordination-mem/tests/conformance.rs` (helpers over `&impl Coordination`, e.g.
  `contract_register_then_discover:26`, `contract_leases_are_unique_and_rising:47`,
  `contract_election_is_always_granted_and_fenced:56`, `contract_locks_are_mutually_exclusive_and_fenced:65`,
  `contract_fencing_tokens_rise_across_locks_and_elections:91`) — passes **against both**
  `coordination-mem` (process-local) **and** `coordination-etcd` (against real etcd). The
  binding conditions: (a) `coordination-etcd` implements every trait method with etcd
  semantics — leased/expiring registration + `discover`, `elect_leader` yielding a rising
  fencing token honored across terms, mutually-exclusive fenced locks, and config with a
  monotonic revision; (b) the shared suite is green on both backends (real etcd via a
  Tier-2 compose target); (c) the backend is a `server`-composition selection — **`traits`,
  `core`, `custodian`, and every caller are byte-for-byte untouched** (the ADR-0008/0016
  discipline). Component identities (crate name `coordination-etcd`, the exact etcd client
  crate, whether the suite is a shared test crate vs a re-exported module) are ILLUSTRATIVE;
  the binding facts are "a second, networked `Coordination` implementation over etcd" and
  "one shared contract suite both impls pass." The **DST fidelity story** for an etcd backend
  (simulated-etcd vs contract harness — the mirror of #264/#258 for `MetadataStore`) is a
  named decision this work must resolve or explicitly hand off (see NEEDS-HUMAN).

- **Repo + branch target:** getwyrd/wyrd @ `feat/m4-production-metadata-backend`
  (the M4 integration branch — resolves to `remotes/origin/feat/m4-production-metadata-backend`;
  it already carries the M4 slices and #256's slice-5 brief targets the same base). This
  prerequisite's own slice branch is `feat/coordination-etcd-l5-backend`, PR'd **into** this
  integration base, not `main`. (Whether this prerequisite is sequenced as an explicit M4
  slice or a preceding coordination milestone is an *open scope decision* in 0015 `:461-463` /
  `:707-709` — see Ordering note; the branch base does not depend on that decision.)

- **Depends on:**
  - The existing `Coordination` trait surface (`crates/traits/src/lib.rs:434-478`) and the
    trait-generic conformance assertions in `crates/coordination-mem/tests/conformance.rs`
    (the shared-suite seed). No trait change is expected; if the etcd backend surfaces a
    genuine semantic gap (e.g. blocking-lock or push-watch, flagged provisional in the trait
    note `:428-433`), that is a NEEDS-HUMAN, not a silent trait edit.
  - A real etcd endpoint for the networked half of the suite — the 3-node etcd ensemble that
    #256's `deploy/` stack (slice 5) stands up, or a standalone Tier-2 compose etcd. **Note the
    circular framing in 0015:** slice 5's `deploy/` stack *hosts* etcd, but slice 5's "peers
    discovered through L5" DoD is *gated on this crate*. This crate depends only on a reachable
    etcd (a single-node/compose etcd suffices for the contract suite), not on slice 5 shipping.
  - **The process-role half of 0015's prerequisite is separate from this crate — and it is
    owned.** 0015 `:453-454` names TWO halves; this brief is scoped to half (1) only. Half (2)
    — **runnable gateway + custodian process roles** (the custodian binary deferred in M3/#0005;
    the gateway exists only as `put`/`get` client mode) — is **satisfied by #364** (the S3 HTTP
    wire makes the gateway a runnable networked server) **+ #366** (the observability floor makes
    the custodian runnable as its own deployable process). **Maintainer decision (2026-07-04):**
    #364 + #366 satisfy half (2); no separate process-roles issue is needed. This crate stays
    half (1) only; do not fold the roles into `coordination-etcd`.

- **Conflicts with:** #256 (M4.5 `deploy/` stack) is a **downstream consumer**, not a merge
  conflict — it touches `deploy/` + `xtask`, this touches `crates/`, so no file overlap, but
  the two are coupled by the "L5 discovery" DoD that moves onto this crate. Coordinate the
  `server` composition edit (backend selection) with slice 4 (#255, `fix/255-m4-4-server-backend-selection`),
  which edits `server` composition for `MetadataStore` selection — same crate, adjacent seam,
  potential textual overlap in `server`'s wiring.

- **Scope:**
  - A new `coordination-etcd` crate implementing the existing `Coordination` trait over etcd:
    leased/expiring D-server registration + `discover`; leader election with fencing tokens the
    custodian's guard honors (M3.3/#141 leader-elects through the trait); mutually-exclusive
    distributed locks with fencing; config set/get + a real (or polled) config revision.
  - A **shared `Coordination` conformance suite** both `coordination-mem` and `coordination-etcd`
    pass — lifting the trait-generic contract fns already in `coordination-mem/tests/conformance.rs`
    into a shared location, with the etcd run wired against real etcd (Tier-2 compose).
  - The **`server` composition change** that selects the backend by config, with **no caller
    edits** (ADR-0008/0016).
  - Workspace membership + `Cargo.toml` wiring for the new crate.

- **Out of scope:**
  - **Half (2) of the prerequisite — runnable gateway + custodian process roles** — **owned by
    #364** (gateway S3 server) **+ #366** (custodian process), per the 2026-07-04 maintainer
    decision. This crate provides the *seam*; the process binaries that dial
    `--coordination <L5-endpoint>` land in #364 / #366.
  - Any change to the `Coordination` **trait** surface, to `traits`, `core`, `custodian`, or any
    caller (all byte-for-byte untouched — 0015 `:236-240`, `:639`).
  - The `deploy/` bring-up stack, its `xtask` runner, and the orchestrator-coupling guard (#256 /
    slice 5).
  - `MetadataStore` / TiKV / redb backend code (slices 1–4); the on-disk format; the S3 HTTP wire
    (#364).
  - The blocking-lock and push-config-watch refinements the trait note (`:428-433`) marks as
    later work — surface them, don't build them.

- **Ordering note:** 0015 leaves it an **open scope decision** whether this prerequisite lands as
  an explicit M4 slice or a preceding coordination milestone (`:461-463`, `:707-709`) — but it must
  be **visible on the board** and must **precede slice 5's (#256) "peers discovered through L5"
  claim**. Until it lands, M4's Tier-1/Tier-2 clusters run on **static `--endpoints`**, which is
  sufficient to prove the metadata risk (`:459-461`). Sequence: this crate + shared suite first;
  in parallel #364 (gateway S3 server) + #366 (custodian process) — which own the process-role
  half; then slice 5 (#256) can claim L5 discovery.

- **Do model:** opus-xhigh
- **Difficulty:** **high** — a new distributed dependency (etcd) enters the workspace; the backend
  must satisfy real networked semantics for lease expiry, single-leader election, and fencing under
  a shared contract suite, and it is *the* gate for "peers discovered through L5." Blast radius is
  contained to the new crate + shared suite + one `server` composition line by the ADR-0008/0016
  design, but the correctness surface (distributed election/fencing) is the hard part, plus the
  #264-style DST-fidelity decision.

- **Test file:** the **shared `Coordination` conformance suite** run against `coordination-etcd`
  (path ILLUSTRATIVE — e.g. `crates/coordination-etcd/tests/conformance.rs` re-using the shared
  contract fns, or a shared `coordination-conformance` test crate that both backends drive). It is
  the flippable regression: the etcd backend must make the trait-generic contract assertions
  (register→discover, unique/rising leases, election-fenced, locks-mutually-exclusive-and-fenced,
  fencing-tokens-rise) go **GREEN against real etcd**, and they are RED against a non-implementing
  stub. The `coordination-mem` run of the same suite must stay green (no regression to impl #1).

- **Citations expected:** Do must cite `path:line` on the target branch AND the Planning artifact
  for every change — the trait (`crates/traits/src/lib.rs:434`), the existing impl and its
  conformance seed (`crates/coordination-mem/src/lib.rs:38`,
  `crates/coordination-mem/tests/conformance.rs`), the `server` composition site it edits, and the
  0015 Deployment-prerequisite note (`:443-463`). Where a code fact cannot be confirmed at Plan
  time (exact `server` wiring line, etcd client crate choice, shared-suite mechanics), mark it
  **"confirm at build"** rather than inventing it (standing lesson from #253).

- **Disposition hint:** likely-fix (net-new crate + shared suite; a real, exercisable second
  implementation). The prerequisite's process-role half is owned elsewhere (#364 + #366, per the
  2026-07-04 decision), so this crate is cleanly scoped to half (1).

## Invariants to hold
- The `Coordination` **trait** and all its consumers (`traits`, `core`, `custodian`, gateway
  callers) are **byte-for-byte unchanged**; selection is a `server`-composition swap only
  (ADR-0008/0016; 0015 `:236-240`).
- **One shared contract suite, two backends** — the same trait-generic assertions pass on
  `coordination-mem` *and* `coordination-etcd` (the ADR-0006 two-implementation pin; mirrors TiKV
  as `MetadataStore`'s second impl). No etcd-only fork of the contract.
- Fencing tokens are **monotonically rising** across elections and lock acquisitions (the custodian's
  single-active guard depends on this — M3.3/#141); leases **expire** on the real backend as they do
  under `coordination-mem`'s `ManualClock`.
- `coordination-mem` stays the process-local / dev backend; the etcd backend does not replace it.

## Known NEEDS-HUMAN
- **Companion process-role work (half 2 of the prerequisite) — RESOLVED (2026-07-04).** 0015
  `:453-454` names *runnable gateway + custodian process roles* alongside `coordination-etcd`. The
  maintainer decided **#364** (gateway S3 server) **+ #366** (custodian-as-a-process) satisfy it —
  no separate issue. Out of this crate's scope; slice 5 (#256) claims "peers discovered through L5"
  once this crate + #364 + #366 land. (Recorded so Do doesn't re-open the question.)
- **Sequencing decision (0015 `:461-463`, `:707-709`).** Explicit M4 slice vs a preceding
  coordination milestone is an open governance choice — either way it must be board-visible. A human
  resolves this; the branch base does not depend on it.
- **DST fidelity for an etcd backend** (the #264 / #258 mirror): simulated-etcd vs contract-harness.
  A human should confirm the chosen DST story; if it demands a trait change, that is a further
  NEEDS-HUMAN, not a silent edit.
- **etcd client dependency choice** (which crate, version, TLS/auth posture) — confirm at build;
  a new external/network dependency in the workspace warrants human review.
- Note: the proposal cites the Coordination seam as `traits/src/lib.rs:258-337`; on this tree the
  trait is at `crates/traits/src/lib.rs:434`. Cite the tree, not the proposal's stale line numbers —
  **confirm at build**.

## STOP discipline
Do reads **`brief.md` only** and produces `patch.diff`, the named test, and `build-notes.md`, citing
`path:line` on the target branch for every change (the named test fails against a non-implementing
stub, passes once `coordination-etcd` implements the trait; `coordination-mem`'s run stays green). Do
MAY push to a feature/draft branch and open a **draft** PR into `feat/m4-production-metadata-backend`;
Do MUST NOT `gh pr ready` or `gh pr merge` (mechanically blocked by the builder guard) — readiness and
merge are the human's Check sign-off. The sequencing and DST-fidelity decisions remain NEEDS-HUMAN:
surface them, do not silently absorb them. (The process-role half is resolved: #364 + #366 own it.)

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected: the adversary refuted the fix itself (not just the evidence) — a production L5 backend would ship split-brain. The gating C4-ci never compiles/runs store.rs (etcd feature OFF by default), so "green" only proves impl #1 wasn't regressed + the suite is non-vacuous, not that the etcd backend is correct. Rebuild to fix all the red correctness defects: 1. Split-brain / no lease keep-alive: `elect_leader` (store.rs:196) and `lock` (store.rs:231) grant a 30s lease and never renew it, and the seam returns a token-only `Leadership`/`LockGuard` (traits/src/lib.rs:490,498) so a caller cannot renew. A leader/lock silently lapses after 30s and a peer can win a second leadership while the original still believes it leads. Spawn/retain a keep-alive for the life of the hold (store.rs:33 comment describes code that does not exist), and/or expose a renewable handle through the trait seam (trait change stays a NEEDS-HUMAN, not a silent edit). 2. Unconditional unlock (store.rs:288): stale `unlock` deletes the lock key unconditionally, so it can release a NEWER holder's reacquired lock after the old lease expired. Gate the delete on still-holding the lease/mod-revision. 3. Re-fence path (store.rs:173-190): a repeat `elect_leader` proclaims on the cached LeaderKey; once the 30s lease has lapsed etcd deleted that key, so it returns Err instead of transparently re-campaigning. Re-acquire and return a fresh rising token (mem returns one). 4. Config path untested: `run_all` drives 5 clauses, none config, so store.rs:294-337 (set_config/get_config/config_revision) is exercised on neither backend — contradicting binding criterion (a) "config with a monotonic revision." Add a config clause to the shared suite. 5. Single-instance-only suite: every shared clause operates on one `&impl Coordination`, so the cross-process guarantees that justify the whole crate (single leader / mutual exclusion / peer discovery across processes) are asserted nowhere; mem passes precisely because it is single-process. Add clauses that stand up two `EtcdCoordination` instances and assert one blocks/loses. Also (contract fidelity): `contract_leases_are_unique_and_rising` (conformance lib.rs:47) asserts `second.id > first.id`, but etcd lease IDs are opaque (i64->u64) and only monotone within a session — can flake across restart/reseed; relax to what the trait/etcd actually promise. Not blocking the correctness rebuild but must be resolved before the real-etcd GREEN can be earned: there is no `xtask etcd-conformance` job (the promised automation does not exist), the real-etcd conformance test exits green when WYRD_ETCD_ENDPOINTS is set but built without --features etcd (a miswired Tier-2 job reports false green), and `etcd-client 0.14` needs system `protoc` against the repo's no-system-protoc posture. The mem-suite refactor lift + demonstrated_red non-vacuity are genuine and can be kept. </content>
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected: this is iteration 2 and the crate's load-bearing correctness is still UNPROVEN — the etcd store (store.rs, the crate's entire reason to exist) is compiled only under the OFF-by-default `etcd` feature, so no gate that ran ever compiled or exercised it; the split-brain paths iteration 1 was rejected on are verified by code reading only. The rebuild must make correctness provable AND address every adversary finding below. Correctness must be proven (not asserted): - The etcd backend must actually be compiled and exercised. Success criterion (b) — shared suite + cross-instance clauses GREEN on REAL etcd (single leader / fenced mutual exclusion / peers discovered via L5) — must be demonstrably earnable, not resting on a manual invocation nothing runs. Land the missing Tier-2 automation (an `xtask etcd-conformance` runner analogous to tikv-conformance) so the real-etcd GREEN has a reproducible home. Adversary items that must be addressed: - Orphaned-campaign lease leak (codex confirms): elect_leader (store.rs:196/283) spawns the detached keep-alive BEFORE campaign(...).await. When the cross-instance election test's 750ms timeout (conformance.rs:204/210) drops the future, etcd has already put B's candidate key bound to the lease and the detached keep-alive renews it forever (dropped future != aborted task; no LeaderHold recorded to clean it up). After drop(a), etcd promotes B's orphaned candidacy, the fresh campaign blocks behind an orphan nothing resigns -> the 40s timeout panics. The single-leader test that IS the crate's raison d'etre cannot pass as written. Fix: owned guard that aborts/revokes on cancellation, or detach the keep-alive only after campaign success. - Drop aborts keep-alives but never revokes leases (store.rs:130-141): a cleanly shut-down custodian holds leadership up to HOLD_TTL_SECS=30 after exit, widening the single-leader gap and making the failover test timing-fragile. Revoke on clean drop. - renew and revoke have zero contract coverage on any backend, yet renew is the registration-renewal path production wires (cli.rs:571). Add contract coverage; a renew defect would silently lapse D-server registrations in production. - Clean-build failure (codex confirmed): the `etcd` feature pulls etcd-client 0.14 whose build script needs system protoc; `cargo check -p wyrd-coordination-etcd --features etcd` fails with "Could not find protoc", conflicting with the repo's no-system-protoc posture. Resolve so the shipped feature builds in a clean env. - Config revision-monotonicity clause not shown to bite (RED panics earlier at the read-back assertion, never reaching r1 > r0); make it demonstrably non-vacuous. - "Rising lease id" coverage was dropped (relaxed to assert_ne!); confirm intended or restore. Human judgment items still owed at the next sign-off (decisions, not code): etcd-client dependency review (ADR-0003 three-test audit, deny.toml allowlist, TLS/auth, version), DST-fidelity decision (madsim-etcd-client vs contract harness), and the sequencing- governance call (explicit M4 slice vs preceding coordination milestone).
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected despite green gates: the crate's headline safety property is caught by no gated test, and criterion (b) (real-etcd green) is unearned. These are test-adequacy defects that need a rebuild, not a one-time confirmation. Direction for the rebuild: - Single-leader test can't catch split-brain (the exact class that got iterations 1 & 2 rejected). only_one_of_two_instances_leads_then_hands_off (dst/tests/coordination.rs:107) asserts only `lead_b.token > lead_a.token`, which two SEQUENTIAL campaigns satisfy even under a concurrent (split-brain) grant. Assert that B's campaign stays PENDING while A leads (b_task unresolved before drop(a)) so a store granting A and B concurrently fails. The single-custodian-leader property (elect_leader / campaign path) must be verified by a gated test. - Cross-instance clauses have no demonstrated-red: single leader, cross-instance mutual exclusion, cross-process discovery have no violating-stub counterpart proving a broken store fails them. Add demonstrated-red coverage so their non-vacuity doesn't rest solely on simulator fidelity. - Real etcd is exercised by no gate: criterion (b) is earnable only via manual `cargo xtask etcd-conformance` (docker+protoc, off-ci), and that job returns Ok(()) when docker/protoc are missing (local false-green, xtask/src/main.rs:286/301). Fix the false-green (missing tooling must not pass), and the real-etcd green must actually be produced before this backend enters the shipped graph. - config_revision semantics diverge across backends: mem returns a config-only counter; etcd returns the global cluster mvcc revision (bumped by every write). Shared clause asserts only r1>r0 so it can't catch it; on etcd a config watcher wakes on every unrelated coordination write. Either normalize the semantics or tighten the contract clause to pin config-only advancement. - Re-election treats every proclaim Err as lease-expiry (store.rs:264-270): a transient blip does stop_without_revoke + fresh campaign, self-inflicting a <=6s stall and a lease leak behind the instance's own orphaned candidacy. Distinguish transient RPC error from actual lease loss; add a test that errors the proclaim path (the simulator never errors today). Open sign-off items for the next Check (not the do itself): ADR-0003 three-test audit + deny.toml allowlist + TLS/auth review for etcd-client 0.14 before it enters the shipped graph; the sequencing-governance call. Credit retained: the iteration-2 "store never compiled by any gate" blocker is genuinely closed (store.rs compiled+driven under cfg(madsim) in the dst tier). §6 items: none ticked — driving the reject on the reported issues.
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 4 — carry-forward (from the previous attempt)
- Sign-off rationale: Binding criterion (b) is not satisfiable as the tests stand: the headline single-leader / split-brain property (`cross_instance_single_leader_is_exclusive`) runs ONLY on the madsim simulator and is NOT in the real-etcd conformance (`crates/coordination-etcd/tests/conformance.rs:82-108`; codex flags :72). The real-etcd job never runs a second `elect_leader` while the first instance holds leadership, so single-leader is never checked on a real cluster. Criterion (b) explicitly names "single leader ... on REAL etcd" — MUST be checked there, not only in the simulator. Add it to the real-etcd conformance so `cargo xtask etcd-conformance` earns (b). The correctness rebuild is otherwise materially complete vs iters 1-3 (store genuinely compiled + driven under madsim; split-brain guard gated demonstrated-red; lease-id-reuse, is-lost lag-window, and config-only-advancement refutations did not stick). C4-ci gate is green, but that green is the SIMULATOR's (madsim-etcd-client fidelity) — not real-etcd correctness. Also fix before re-accept: - keyspace prefix-collision bug (`crates/coordination-etcd/src/keyspace.rs:24`): registration keys are not escaped / length-delimited, so `reg/svc/` also matches a member keyed `svc/` (`reg/svc//...`) — violates per-key discovery isolation. Encode the logical segment or use a delimiter that cannot appear in the key. - transient-proclaim-error anti-churn path is verified by NO test: a proclaim RPC error while the lease is still live must return Err yet RETAIN the hold and its renewing lease (`store.rs:285-304`). Add a regression that errors the proclaim path so a dropped-hold regression cannot go undetected. Standing human calls to settle at next Check (not blocking the rebuild): DST fidelity acceptance (#264/#258 mirror), etcd-client dependency review (ADR-0003 audit + deny allowlist + the ships-no-TLS/auth `connect(endpoints, None)` posture), and sequencing governance.
- Full previous attempt preserved in `iteration-v4/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 5 — carry-forward (from the previous attempt)
- Sign-off rationale: Rejected (iter-5): the real-etcd conformance test does not compile under `--features etcd` — `crates/coordination-etcd/tests/conformance.rs:104` calls `b.elect_leader(...)` with no `use wyrd_traits::Coordination;` in scope (E0599). Criterion (b) is therefore unearned: the single-leader clause the iter-4 reject demanded exists only in a file no gate compiles (CI green is the madsim/cfg path; `--features etcd` is off-CI, needs protoc). This is exactly the recurring axis. Fixes for the rebuild: (1) Add the missing `use wyrd_traits::Coordination;` import so the real-etcd conformance compiles, and make `cargo xtask etcd-conformance` actually go GREEN against a live etcd. Verified at sign-off: docker present, protoc now installed on the sign-off host — the test still fails to BUILD, so this is a plain defect, not an environment gap. (2) The `xtask etcd-conformance` retry loop misreports a hard compile error as "etcd may still be bootstrapping" and retries it 5× — distinguish a build failure from a bootstrap flake so a non-compiling test can't masquerade as transient. (3) Adversary finding still open — `election_name` (keyspace.rs:58) formats the raw key with no `encode_segment` while `registration_member` does, reintroducing the iter-4 hierarchical-prefix-collision class for elections; caught by no test (all election clauses use flat keys). Decide election keys in-/out-of-contract and either encode+regression-test (mirror `discovery_prefix_isolates_hierarchical_keys`) or document+assert the restriction. Standing human calls (DST-fidelity acceptance, etcd-client 0.14 dependency review, sequencing governance) remain owed at the next Check. Env note going forward: protoc is now installed on the sign-off host, so `cargo xtask etcd-conformance` can be re-run at the next Check to confirm real-etcd green before any accept.
- Full previous attempt preserved in `iteration-v5/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
