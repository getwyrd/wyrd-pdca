# Brief — issue 438 / metadata-fdb

> Plan artifact. Do reads ONLY this file (plus the peer callsites cited below).
> Design authority already exists: **ADR-0042** (Accepted) decides FoundationDB as the
> production distributed `MetadataStore` backend, superseding ADR-0008. This brief does not
> re-open that decision; it plans its first implementation slice. The FDB *semantics mapping*
> is deliberately implementation-first (ADR-0002) and is consolidated afterwards in #437 —
> so it is planned here, not in a host doc.

- **Slug:** metadata-fdb-store
- **Defect:** ADR-0042 chose FoundationDB as the production metadata backend, but no
  implementation exists: `crates/metadata-fdb` is absent from `origin/main`, so the
  decision is unexecuted and every downstream M4 item (CI harness, server selection,
  packaging, the #257 go/no-go battery) is blocked on it. The gap is a `MetadataStore`
  implementation over FDB that satisfies the same contract `redb` and `tikv` already do.
- **Success criterion:** With the `fdb` feature on and a live single-node `fdbserver`,
  `crates/metadata-fdb/tests/conformance.rs` drives the **shared** `run_all` suite green
  across all **7** contracts against `FdbMetadataStore`; and
  `crates/metadata-fdb/tests/contention.rs` shows, against two racing clients, that (a) a
  **conditional** batch that loses a race on a `require`d key returns `Ok(Conflict)` (FDB
  error 1020 `not_committed`), and (b) a **blind** batch (no preconditions) surfaces `Err`
  and **never** `Conflict`. Independently, `cargo xtask ci` with the `fdb` feature **off**
  stays green and links no `libfdb_c`.
- **Falsifiability:** RED is demonstrable, on the environment Do gets, by the house
  demonstrated-red pattern (`crates/metadata-conformance/tests/demonstrated_red.rs`, #419):
  against a live `fdbserver`, deliberately negate the 1020→`Conflict` classification (return
  `Err` instead) and `contract_rename_race_yields_conflict` **fails**; deliberately classify
  a blind batch's commit error as `Conflict` and the blind-batch clause of
  `tests/contention.rs` **fails**. Environment: single-node `fdbserver` from
  `foundationdb/foundationdb:7.3.77` (verified: image pulled; reports `FoundationDB 7.3
  (v7.3.77)`; carries `/usr/bin/fdbserver` **and** `/usr/lib/libfdb_c.so`) plus `libfdb_c`
  7.3.x on the **build** host and a matching `fdb-7_3` api-version feature. Docker daemon
  verified reachable.
  **Topology is adequate** (the check that fails the #257/#365 case): the forbidden failure
  here is a *lost conditional race* (1020 `not_committed`), which FDB's resolver detects
  from the read-conflict set on a **single** node — two racing clients suffice. It does not
  require a multi-replica partition, so unlike "exactly-one-winner under partition", a
  single-node stack **can** genuinely exhibit it.
  Note the RED is a *real failing assertion against a real server*, not criterion-absence —
  Do MUST capture it, not rest red on "the crate did not exist yet".
- **Invariant to restore:** **Every** `MetadataStore` implementation — not merely the FDB
  one — returns `Ok(CommitOutcome::Conflict)` **exactly when** a precondition of the batch
  did not hold, returns `Err` for a backend fault, and **never** reports `Conflict` for a
  batch carrying no preconditions (a blind write must not be silently swallowed by callers
  that use `?` and ignore `CommitOutcome`). Source: the trait's normative doc-contract,
  `crates/traits/src/lib.rs:346-350` (authoritative, internal Tier C per principles.md §5),
  enforced identically across backends by the single shared runner
  `crates/metadata-conformance/src/lib.rs:291` (`run_all`); corroborated by the TiKV
  driver's `conditional = !batch.preconditions.is_empty()` rule,
  `crates/metadata-tikv/src/lib.rs:542-546`.
  *Self-test:* a one-module fix cannot satisfy this — it is quantified over all
  implementations and is checked by the **shared, not forked** suite. **Forking or
  weakening the conformance suite to make FDB pass violates the invariant.** Any suite
  change must keep `redb` **and** `tikv` green in the same PR.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Ordering note:** #436 (the ADR-0042 decision) is CLOSED and already merged into `main`;
  it is not a wave dependency. #437 is an explicit **follow-on** consolidation, not a
  prerequisite. The issue body's "targets `main` after `feat/m4-production-metadata-backend`
  merges" is **satisfied**: that integration branch merged as PR #489 and is now `main`'s tip
  (`182ae4f`), so per INTEGRATION §2 the target is plain `main`, not the M4 integration branch.
- **Surfaces:** data
- **Difficulty:** high
- **Scope:** Add a new workspace crate `crates/metadata-fdb` implementing the unchanged
  `MetadataStore` trait (`crates/traits/src/lib.rs:338`) over the `foundationdb` crate,
  feature-gated `fdb` and **off by default**, so that the shared conformance suite and the
  driver-level race tests pass against a real `fdbserver`, and add the minimum throwaway
  harness needed to demonstrate that. Concretely in scope:
  (1) the crate + its `fdb` feature with all backend deps optional;
  (2) an `FdbMetadataStore` with **per-instance key-prefix isolation**, because `run_all`
  hands each of the 7 clauses a *fresh, isolated* store via `make_store(tag)`;
  (3) `tests/conformance.rs` and `tests/contention.rs`, both **cleanly skipping** when the
  cluster is not configured, so `cargo xtask ci` stays green on a machine with no FDB;
  (4) `deploy/fdb-single-node/docker-compose.yml` (throwaway, outside the Cargo workspace
  per ADR-0010) and an `xtask fdb-conformance` job that brings it up, runs the tests
  feature-on, and always tears it down;
  (5) workspace-member + dependency-wall bookkeeping (`Cargo.toml`, `Cargo.lock`,
  `deny.toml` if the graph introduces a license not already allowed).
  Cluster-file configuration (`WYRD_FDB_CLUSTER_FILE`, default `/etc/foundationdb/fdb.cluster`)
  is owned by **the driver's own constructor** in this slice.
  **Constraints (properties, not mechanisms — the shape is Do's to choose):**
  - The FDB client permits exactly **one network thread per process**, while `run_all`
    constructs **seven** stores in one test process; the driver must remain correct under that.
  - `1021 commit_unknown_result` MUST NOT be blind-retried (a `WriteBatch` is not guaranteed
    idempotent) — surface it as a distinguishable error. **Hazard:** the `foundationdb`
    crate's `db.run` closure-retry re-runs on 1021, so adopting it uncritically violates
    this. (Naming the hazard to avoid, not the mechanism to use.)
  - A **conditional** batch that loses a race returns `Ok(Conflict)` and lets the *caller*
    decide — e.g. `alloc_inode`'s budgeted backoff loop (`crates/server/src/cli.rs:1027-1049`),
    which retries on any non-`Committed` outcome. The driver must not silently retry a
    conditional batch on the caller's behalf. Bounded retry of retryable errors is
    permissible for read paths and blind batches.
  / out of scope: wiring FDB into backend **selection** in `crates/server/src/cli.rs` (a
  later, explicitly-blocked issue — do **not** add an `open_fdb_meta` selection arm); the
  GitHub Actions CI workflow; `libfdb_c` **packaging** / the `wyrd:fdb` image; the #257
  go/no-go fault+contention battery; the #437 contract-doc consolidation; the TiKV
  development stand-down; and **any change to the `MetadataStore` trait** (it is unchanged —
  this is a composition, not a refactor, per ADR-0010).
  Also out of scope: the **transaction-envelope check** (FDB's 5 s / 10 MB transaction and
  10 KB / 100 KB key/value limits). The issue leaves it "added here or at the #437
  consolidation"; **this brief resolves that ambiguity to #437**, to keep the slice bounded.
  Single `WriteBatch` commits sit far inside those limits and the trait admits no
  transaction spanning multiple `commit` calls, so nothing in this slice can breach them.
- **Repro instruction:** On `origin/main`, `git -C ../wyrd ls-tree origin/main crates/ | grep fdb`
  returns nothing — the crate does not exist, so no FDB backend can be constructed and no
  FDB conformance run exists. Baseline for comparison: `cargo xtask tikv-conformance`
  (`xtask/src/main.rs:181`) is the exact shape the FDB leg must mirror.
- **External dependencies:** (enumerated at Plan; seeded as `[[doctor.checks]]` rows in this
  harness's `pdca.toml` so they preflight rather than fail mid-cycle)
  1. **`libfdb_c` 7.3.x on the BUILD host** — the `foundationdb` crate links the FDB C client
     at **build** time, so `cargo build --features fdb` **cannot compile** without it.
     **Currently ABSENT on this host** (`ldconfig` shows no `libfdb_c`, no `fdbcli`). Install
     the official `foundationdb-clients` 7.3.x package, or extract `/usr/lib/libfdb_c.so`
     from the `foundationdb/foundationdb:7.3.77` image. The crate's `embedded-fdb-include`
     feature removes the *header* requirement, **not** the link requirement.
  2. **Docker daemon** (verified reachable) + the **`foundationdb/foundationdb:7.3.77`**
     image (verified pulled) — provides `fdbserver` for the live run.
  3. **A running single-node `fdbserver` + a cluster file**, reached via `WYRD_FDB_CLUSTER_FILE`.
  4. **Network access to crates.io** (verified) for the new `foundationdb` dependency tree.
  5. The api-version feature (`fdb-7_3`) **must match** the 7.3 server/client. Client and
     server versions must agree: a 7.3 `fdbserver` with a 7.1 `libfdb_c` fails to connect.
  **Do MUST NOT work around an unmet dependency** with a code-read, a stub, or a hand-rolled
  fake FDB in place of the real server. If `libfdb_c` is missing at Do time, **stop and
  declare it** — an unmet dependency is a Check §6 item, never a substitution.
- **Test file:** `crates/metadata-fdb/tests/conformance.rs` (primary — drives the shared
  `run_all`; must go red→green against a live `fdbserver` per Falsifiability above).
  Secondary: `crates/metadata-fdb/tests/contention.rs` (the 1020-classification and
  blind-batch-`Err` rules).
- **Verification posture:** **Deferred / off-Check for the live legs — but the code is BUILT
  and EXERCISED, not merely scaffolded.** By design (acceptance criterion 3, and the TiKV
  precedent `crates/metadata-tikv/Cargo.toml`), the `fdb` feature is **off** in the default
  build, so `cargo xtask ci` at Check neither links `libfdb_c` nor runs the FDB tests: both
  test binaries **skip cleanly** without a cluster file.
  - **Built AND exercised at Check:** `cargo xtask ci` green with the feature off; the
    feature-off skeleton and the `#[cfg(not(feature = "fdb"))]` skip arms compile; and
    `cargo deny check` (which `run_ci` invokes, `xtask/src/main.rs:1116`) **does** see the new
    `foundationdb` dependency tree, because an optional dep lands in `Cargo.lock` even with
    its feature off — verified: the optional, default-off `tikv-client` is present in
    `origin/main`'s `Cargo.lock` today. So a disallowed transitive licence fails **at Check**,
    feature-off. This is real Check signal, not scaffolding.
  - **Deferred:** the live `run_all` + contention runs. Confirmed by **Do**, which MUST
    (i) compile `--features fdb` for real, and (ii) run `cargo xtask fdb-conformance` against
    the docker `fdbserver`, pasting the passing output **and** the demonstrated-RED output
    into `build-notes.md`. Re-confirmed by the **human at sign-off** (Eduard Ralph).
  - **Forcing function:** deferred ≠ unbuilt. Code that was never compiled with `--features
    fdb` does not satisfy this brief. A driver that only *looks* right in a diff is a
    rejection, not a deferral.
- **Production reach:** This slice builds the backend **ahead of its production consumer**.
  Nothing in the live path constructs an `FdbMetadataStore`: `crates/server/src/cli.rs`
  still selects `redb` or `tikv` (`open_tikv_meta`, `cli.rs:133-140`), and the FDB selection
  arm is explicitly a later, blocked issue. So the seam is honoured **only by the two test
  binaries** — but load-bearingly: they drive the *real* driver against a *real* `fdbserver`
  over the *shared* contract suite, not a double. Callers such as `alloc_inode`
  (`crates/server/src/cli.rs:1027`), whose budgeted backoff loop retries on
  `CommitOutcome::Conflict`, will exercise it once selection lands. Pre-declared here so the
  "is a not-yet-wired backend causally sufficient?" C5/T5 question is a sign-off item, not a
  surprise NEEDS-HUMAN.
- **Citations expected:** Do must cite `path:line` on `origin/main` for every change. This is
  a **composition slice** — the codebase already applies this exact pattern for TiKV. Do MAY
  open these peer callsites and SHOULD mirror them:
  - `crates/metadata-tikv/tests/conformance.rs:51-59` — the canonical `run_all` driver:
    `make_store(tag)` returns a store scoped to a fresh per-`tag` namespace (with the pid, so
    concurrent CI runs don't collide). **Mirror this**; without per-tag isolation the 7
    clauses corrupt each other against one shared cluster.
  - `crates/metadata-tikv/tests/conformance.rs:11-34` — the clean-skip gate
    (`WYRD_TIKV_PD_ENDPOINTS` absent ⇒ `eprintln!` + `return`) **and** the
    `#[cfg(not(feature = "tikv"))] fn run(..)` arm at `:63-70`. Mirror both for
    `WYRD_FDB_CLUSTER_FILE`; this is what keeps `cargo xtask ci` green with no FDB.
  - `crates/metadata-tikv/src/lib.rs:403-414` (`conflict_or_err`) and `:542-546` (`let
    conditional = !batch.preconditions.is_empty();` + the comment explaining why a blind
    batch must stay `Err`) — the classification rule to reproduce for FDB error 1020.
  - `crates/metadata-tikv/Cargo.toml` — the feature-gating shape: `default = []`, all backend
    deps `optional = true`, `[package.metadata.cargo-machete] ignored = [...]` to keep the
    feature-off scan quiet.
  - `xtask/src/main.rs:181-213` (`run_tikv_conformance`) + `:215-268` (retry/backoff around a
    bootstrapping cluster) + `deploy/tikv-single-node/docker-compose.yml` — the compose-up →
    wait-for-port → run → **always tear down** job shape.
  - `crates/metadata-conformance/tests/demonstrated_red.rs` — the #419 demonstrated-red
    precedent for proving a property is load-bearing.
  - `crates/traits/src/lib.rs:338-351` (trait), `:363-383` (`Precondition`/`WriteBatch`),
    `:401` (`require_absent`).
- **Prior-art check (searched by file path):** `git -C ../wyrd log --all --oneline --
  crates/metadata-fdb` → **empty** (no merged, in-flight, or reverted attempt).
  `git ls-tree origin/main crates/` → no `metadata-fdb`. `gh pr list --state all --search
  "fdb in:title"` → **no PRs**, open or closed. `foundationdb` appears in **neither**
  root `Cargo.toml` nor `deny.toml` (new dependency). The single tree match for
  "FoundationDB" is `crates/server/tests/dst_commit.rs:234` — a comment citing the
  FoundationDB/TigerBeetle *DST seed* pattern (ADR-0009), unrelated to a metadata driver.
  Conclusion: genuinely net-new; nothing to supersede or resurrect.
- **Dependency-wall note (pre-declared NEEDS-HUMAN):** `foundationdb` is a **new
  dependency**, which INTEGRATION §4 lists as human-only by design (the ADR-0003 three-test
  audit + `deny.toml` allowlist review). The crate is `MIT/Apache-2.0`, both already in
  `deny.toml`'s allowlist (`deny.toml:25-38`), so the *licence* test should pass mechanically;
  the *judgment* tests remain the maintainer's. **Pin `foundationdb = "0.10"` with
  `default-features = false`** and the **`fdb-7_3`** api feature, matching the current
  FoundationDB **7.3.77** release. (Verified: 0.10.0 exposes `fdb-7_3` — and `fdb-7_4`, unused
  here. `0.11.0` is the current crate but adds `recipes` to its defaults, widening the audit
  surface for no gain in this slice; `0.10` is what the issue specifies. The issue body's
  "7.1" era references are superseded by 7.3.77.)
  Any transitive licence not on the allowlist is a **stop-and-declare**, not a `deny.toml`
  edit Do makes unilaterally.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: The fix is sound — not the reason for the iterate. The adversary reviewer ran four mutations against a live foundationdb/foundationdb:7.3.77 cluster and could not break the commit contract; the builder captured four demonstrated reds (RED-A/B/C/S) against a real fdbserver. The design is not in question. Three concrete gaps, all in-scope, no open design decisions: 1. deny.toml — MAINTAINER RULING AT SIGN-OFF: ISC is ALLOWED. Add "ISC" to deny.toml's `allow` list with an ADR-0003 §2 rationale comment, in the style of the existing BSD-3-Clause / Zlib entries (name libloading v0.8.9, reached transitively via foundationdb-sys -> bindgen -> clang-sys). This edit is in scope per brief.md:76-77 scope item (5) ("deny.toml if the graph introduces a license not already allowed"); the graph does, and patch.diff does not touch deny.toml. Do was right to stop-and-declare per brief.md:199-200 rather than edit unilaterally — the call is now made, so encode it. Do NOT re-ask this at the next sign-off. Note the hole is invisible to `cargo deny check` as run_ci invokes it (default features never resolve the foundationdb tree; verified `cargo deny list | grep -c foundationdb` -> 0), and that `cargo deny --all-features check licenses` already fails on pristine main via ring behind tikv-client. Fixing that pre-existing failure is NOT in this slice's scope; only the ISC allowlist entry is. 2. The 1021 no-blind-retry rule — brief.md:83-86's headline constraint — is defended by no test. The test named for it, `commit_unknown_result_is_never_conflict_and_never_retried` (contention.rs:104-107), never calls `commit_blind`: it asserts on the pure `classify_commit_error` and greps a Display substring, duplicating the unit tests at lib.rs:174-208. Changing the blind path's guard at lib.rs:956 from `err.is_retryable_not_committed()` to `err.is_retryable()` (1021 IS retryable) leaves conformance, contention and --lib all green. Add a test that exercises commit_blind's retry arm so that mutation goes RED. 3. scan's paging loop (lib.rs:473-485, scan_once) is dead code under test. Replacing the `next_range` match with `return Ok(out)` — a scan that silently returns only its first page — leaves conformance and contention green: every scan in the shared suite returns <=3 keys in one page, so `more()` is never true. Add a paged-scan completeness test mirroring the TiKV peer's crates/metadata-tikv/tests/scan.rs (the #254 binary named at xtask/src/main.rs:218-221) so first-page truncation goes RED. Already cleared — do not re-litigate: - C5 / T5 (a backend built ahead of its production consumer is causally sufficient for this slice): ACCEPTED. Pre-declared at brief.md:147-156. - T4 (adopt foundationdb 0.10, default-features = false, features = ["fdb-7_3"]): ACCEPTED. - The live-run NEEDS-HUMANs the codex reviewer raised (C2, C4, T3, fitness-to-purpose) are a sandbox artifact, not a finding: `codex exec --sandbox workspace-write` denies /var/run/docker.sock, so that reviewer could not reach the cluster. Docker, libfdb_c 7.3 and fdbcli are all present on this host, and both the builder and the adversary ran the live legs for real. Carry forward, so the rebuild is not misled: - brief.md:124-125 designates tests/conformance.rs the "primary — must go red->green" witness for the 1020->Conflict rule. It CANNOT be: contract_rename_race_yields_conflict (metadata-conformance/src/lib.rs:168-227) is strictly sequential, so the racer's precondition read observes the deleted key and returns Ok(Conflict) from the observed-miss branch (lib.rs:505); classify_commit_error is never invoked from conformance.rs and FDB error 1020 never arises there. Both the builder (build-notes.md §3) and the adversary found this independently. The 1020 rule is pinned by tests/contention.rs:175. This is a BRIEF defect, correctly reported by Do — not a shortfall to fix by reshaping the shared suite. Do NOT fork or weaken crates/metadata-conformance to satisfy the brief's sentence. - check-gates C4-verify was scored "pass" on the evidence string "no pre-patch state to isolate a RED against" — verbatim the ground brief.md:39-40 forbids. The real reds exist (build-notes.md §2). The row's evidence should cite them rather than rationalize absence. - xtask/src/main.rs:417 retries a failing test binary 5x, treating an assertion failure like a cluster-still-settling connection failure. Inherited from the TiKV precedent the brief told Do to mirror, so not invented here — but with (2) and (3) unfixed it can convert a future flake in the sole 1020-pinning test into a green. Worth a look while adding the tests above.
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Sign-off rationale (issue 438 / metadata-fdb, iteration 2 -> 3): The design is sound and is NOT the reason for the iterate. All three carry-forward gaps from iteration 1 are genuinely CLOSED, verified by the adversary against a live foundationdb/foundationdb:7.3.77 cluster: (1) the ISC allowlist entry is encoded and load-bearing; (2) the 1021 no-blind-retry rule is now bound by the production blind_commit_loop/blind_commit_step (mutation is_retryable_not_committed() -> is_retryable() goes RED on --lib); (3) tests/scan.rs kills first-page truncation and grounds its own page-boundary fixture against the live server. Gap 4 (xtask's 5x retry) is also fixed. Eight mutations against the conflict semantics; none survived undetected. Do NOT re-litigate the design, the seam, or the test shapes. Three concrete findings, all in-scope, no open design decisions: 1. `scan` has no SCAN_CAP. THE BLOCKER. crates/metadata-fdb/src/lib.rs:478-492 (`scan_once`) accumulates `out` with no ceiling. The sibling backend crates/metadata-tikv/src/lib.rs:136-145 documents this cap as "a **correctness constraint, not a tuning knob** (#262)" because "a silently truncated `inode:` scan corrupts GC's never-reclaim safety set (data loss)", and fails loud with `ScanCapExceeded` returning NO partial Vec. FDB instead grows the gateway heap unbounded or, past FDB's 5 s transaction limit, takes 1007 at lib.rs:726, restarts the WHOLE scan 5x, and returns the opaque "metadata scan exhausted 5 attempts" string. Mirror the TiKV peer: same 2^20 constant, same `ScanCapExceeded` error type, no partial results, plus a test. No design decision to make — the peer already made it. This is NOT the transaction-envelope item brief.md:98-102 deferred to #437 (that carve-out reasons only about `commit`); the #262 heap/GC bound is a separate constraint, undeclared in the brief, and this diff is the first thing to introduce an FDB `scan`. The shared run_all suite cannot see it, so no gate can. 2. The retry-exhaustion paths break the downcast contract this same diff promises. lib.rs:892-902 states a caller "must be able to `downcast_ref::<foundationdb::FdbError>()`" to tell a transient `transaction_too_old` from a permanent `value_too_large`. But lib.rs:617-619, lib.rs:701-703 and lib.rs:730-732 all exit as `BoxError::from(format!("... exhausted 5 attempts ..."))` — a String-backed error; downcast_ref returns None, destroying exactly the distinction the doc promises. Concrete case: a blind commit failing 5x with 1007 (retryable_not_committed = true, so every attempt takes the Retry arm). Same for `get` and `scan` under five 1007s. `CommitUnknownResult` (lib.rs:137) IS downcastable, so the crate contradicts itself, not merely a peer. Fix: a typed exhaustion error carrying the last FdbError as `source`. 3. The module doc overclaims what pins the invariant's third clause, and two mutants survive. lib.rs:31 says the `conditional` guard "is the whole of the invariant's third clause" and lib.rs:36 says it "is pinned by crates/metadata-fdb/tests/contention.rs". Neither holds on the production blind path: FdbError::from_code(1020) .is_retryable_not_committed() is TRUE, so blind_commit_step (lib.rs:579) always routes 1020 into BlindStep::Retry — `outcome_from_commit_error(err, false)` at lib.rs:613 can never be reached with 1020, and `classify_commit_error(1020, false)` is unreachable from production. Proof: flipping that literal `false` to `true` leaves all four legs green; forcing `let conditional = true` at lib.rs:746 is ALSO green on all four legs. What actually keeps a blind batch out of Conflict is the retry gate, not the guard. contention.rs:286-297 "pins" the guard only by calling the pure classify_commit_error directly — VERBATIM the vacuous shape the iteration-1 sign-off rejected for the 1021 rule (brief.md:210 item 2). The previous review named an instance; the identical pattern sits one function over. Required: (a) correct the doc claim at lib.rs:31,36 — say the guard is defence-in-depth and name what actually pins the clause; (b) add a test that makes the `let conditional = !batch.preconditions.is_empty()` rule at lib.rs:746 load-bearing. Concrete cost of that mutant surviving: it silently removes the bounded retry of 1007 transaction_too_old / 1009 future_version from every blind commit, and no test notices. The guard itself is harmless — keep it; it is the DOC CLAIM and the coverage that are refuted, not the code. Already cleared — do NOT re-ask: - C2 / C4 / T3 / Validation (fitness-to-purpose): CLEARED on the adversary's live-run evidence (Docker + libfdb_c 7.3 + fdbcli present; all four legs run for real, three cold container runs). The codex reviewer's NEEDS-HUMANs on these are a sandbox artifact (`codex exec --sandbox workspace-write` denies /var/run/docker.sock), not a finding. This is the SECOND cycle they have recurred verbatim. - T4 Contribution: CLEARED. foundationdb 0.10 (default-features = false, features = ["fdb-7_3"]) was accepted at iteration 1. The deny.toml ISC entry stands as written. - deny.toml:42-43 — the adversary is factually right that "No ISC-licensed code is linked into a shipped Wyrd binary" is false for `wyrd-server --features tikv` (ring, Apache-2.0 AND ISC, and untrusted, pure ISC, are NORMAL deps via tikv-client -> tonic -> tokio-rustls -> rustls; `grep -ci isc NOTICE` -> 0). MAINTAINER RULING: not a blocker, do not spend a cycle on it. The entry is load-bearing and the ruling is correctly encoded. Leave the comment alone. - C5 / T5 (a backend built ahead of its production consumer is causally sufficient for this slice): ACCEPTED at iteration 1, pre-declared at brief.md:147-156. Carry forward, so the rebuild is not misled: - brief.md:124-125 designates tests/conformance.rs the "primary — must go red->green" witness for the 1020->Conflict rule. It CANNOT be, and this is now confirmed by measurement twice over: under both the drop-the-guard and the snapshot-precondition-read mutations the conformance leg stays GREEN. tests/contention.rs is the only live witness. This is a BRIEF defect, correctly reported by Do. Do NOT fork or weaken crates/metadata-conformance to satisfy the brief's sentence. - check-gates C4-verify is scored "pass" on the evidence string "no pre-patch state to isolate a RED against; C4-ci gates the whole tree (#88)" — verbatim what brief.md:39-40 forbids, and flagged at the iteration-1 sign-off. It recurs unchanged. Worse, the claim that C4-ci covers it is false in a way that matters: run_ci builds with DEFAULT features and the entire `store` module is #[cfg(feature = "fdb")] (lib.rs:361), so the green C4-ci gate never compiled the driver, never linted it, and ran none of the four legs — including the four store::tests that are the sole witness for the 1021 rule. Real reds exist and are cheap to cite (build-notes.md §5). Cite them. - contention.rs:73 (blind_write_race_never_reports_conflict) is close to a tautology: a write-only FDB transaction has an empty read-conflict set, so no error is produced, the Ok(CommitOutcome::Conflict) panic arm at contention.rs:225 is unreachable, and any error that did occur is swallowed at contention.rs:230 (`Err(_) => {}`). Its only surviving assertion is `committed >= 1`. Do reported this honestly (build-notes.md §5). Keep it, but do not let it imply more than it proves — and consider reading the key back. - Low severity, fix while in there if cheap: lib.rs:608-616, :697, :726 call `on_error` (a real backoff sleep) after the FINAL failed attempt, charging the caller one full backoff it can never spend; `break` before the last reset. - conformance.rs:58 builds its isolation prefix as wyrd-fdb-conformance/{pid}/{tag}/ with no timestamp, while contention.rs:334 and scan.rs:197 (same diff) both stamp .../{tag}/{nanos}/. Internally inconsistent; masked today only by `compose down -v`.
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
