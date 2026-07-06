# Result — issue 365 / coordination-etcd-l5-backend

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: 
- Success criterion: a **shared `Coordination` conformance / contract suite** — lifted
- Repo + branch target: getwyrd/wyrd @ `feat/m4-production-metadata-backend`
- Scope (one logical fix) / out of scope: 

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix (net-new crate + shared suite; a real, exercisable second
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass —                no pre-patch state to isolate a RED against; C4-ci gates the whole tree (#88).
- C5 Causal adequacy: none — reviewer + human sign-off

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 Contribution: none — (no gate configured)
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

# Check review — issue 365 / coordination-etcd-l5-backend

**Task under review:** The `Coordination` trait (`crates/traits/src/lib.rs:434`) has exactly one
implementation, the process-local `coordination-mem`, so cross-process L5 discovery/election/fencing
does not exist. Build the ADR-0006 REQUIRED second implementation — a networked `coordination-etcd`
crate over etcd — plus one **shared** conformance suite both backends pass, selectable in `server`
composition with no caller edits.

> Re-run note: `cargo`, `git`, and `printenv` were all blocked by the sandbox approval guard in this
> session, so I could not independently re-execute `xtask ci` or the headless tests. Verdicts are
> grounded on (a) the target source at `/home/eddie/wyrd/wyrd.pdca-wt-l1` (patch is applied there;
> read-only) and (b) `check-gates.json` (C4-ci gating=pass). Every real-etcd (`--features etcd`)
> claim is therefore un-re-run by me AND un-exercised by the gate — see the Validation row.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Brief pins a crisp, binding spec: second etcd `Coordination` impl + one shared suite both backends pass + `server`-only selection, traits/core/custodian byte-for-byte untouched. Grounds on trait at `crates/traits/src/lib.rs:434` and 0015 `:443-463`. |
| C2 Reproduction (red pre-fix) | PASS | Net-new capability, not a bug — no pre-patch RED to isolate (gate C4-verify: "no pre-patch state"). The meaningful non-vacuity RED is demonstrated headlessly: `coordination-conformance/tests/demonstrated_red.rs:671` drives the shared clauses against a `BrokenCoordination` stub and asserts each panics. The real-etcd GREEN half is deferred (Validation). |
| C3 Change | PASS | Change is exactly the scoped surface: new `coordination-etcd` (`src/lib.rs`, `src/store.rs`), shared `coordination-conformance`, mem test lifted to `run_all`, `server` selection in `cli.rs:1706`+, Cargo wiring. No stray edits; traits/core/custodian absent from the diff. |
| C4 Verification (red→green) | PASS | Deterministic gate C4-ci = pass (check-gates.json:33) on the DEFAULT build. But that build has the `etcd` feature OFF, so `store.rs` (the entire real etcd impl) is never compiled and the networked suite `tests/conformance.rs:1446` skips (no `WYRD_ETCD_ENDPOINTS`). Green-here = load-light `keyspace`/`fencing` unit tests + mem `run_all` + demonstrated_red only. The binding success criterion (b) "suite green on real etcd" is un-verified by any gate — carried to Validation, not a fabricated FAIL. |
| C5 Causal adequacy | PASS | Fix removes the root cause (only one impl) by shipping a real second implementation, not a guard. Smell-test applied: the `#[cfg(feature = "etcd")]` gating (`lib.rs:1062`, `cli.rs:1702`) and the endpoint-gated test skip are standard optional-backend compile/CI gating (mirror of `metadata-tikv`), not a capability probe papering over a load-time side effect — C5 trigger does not fire. |
| T1 Structure | PASS | Two new workspace crates correctly placed and wired (`Cargo.toml:18-19,47-48`); optional `etcd-client` dep isolated behind an OFF-by-default feature; `server` gains an optional dep. Layering respects ADR-0016 (backend depends on traits + client only). |
| T2 Shape | PASS | `EtcdCoordination` implements every trait method with etcd semantics (leased put/prefix-get, campaign/proclaim, compare-and-put txn lock, mod-revision config); `CoordinationBackend` selection mirrors the `MetadataBackend` shape. Load-light `keyspace`/`fencing` split mirrors `metadata-tikv`. |
| T3 Runtime | PASS | Default/unit runtime is covered (keyspace aliasing, NUL-delimiter isolation, revision→token monotonicity unit-tested in `store` siblings, gate green). The networked runtime is unexercised here; its concrete assumptions (see Validation) are the sign-off's to confirm. |
| T4 Contribution | PASS | Trait-generic clauses lifted out of `coordination-mem/tests/conformance.rs` into the shared suite and both backends now drive `run_all`, so the lift is proven non-regressive for impl #1; mem-specific ManualClock/revision tests retained; `server` selection test added (`cli.rs:1891`). |
| T5 Judgment | NEEDS-HUMAN | Oracle is "reviewer + human sign-off" (check-gates.json:98). Judgment calls left open by design and NOT silently absorbed: (1) DST-fidelity story for etcd (`madsim-etcd-client` vs contract harness, #264/#258 mirror), (2) whether any etcd semantic gap should force a trait change (must stay a NEEDS-HUMAN, not a silent edit). Decide before merge whether the deferred-DST posture is acceptable for a production L5 backend. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | The core deliverable — "shared suite GREEN against real etcd" (success criterion b) — is verified by NO gate: it needs a live etcd via `xtask etcd-conformance` + `--features etcd`, which no headless runner in this session can stand up. Human must, at sign-off: (a) run the etcd conformance job and confirm it passes AND that `store.rs` even compiles under `--features etcd` (its own header flags the `etcd-client` API entry points as "reconfirmed at build time"); (b) sanity-check two networked assumptions the contract leans on — that etcd `lease_grant` IDs satisfy `second.id > first.id` (`contract_leases_are_unique_and_rising`) and that campaign/lock revisions stay globally monotonic across kinds; (c) clear the new external gRPC dependency `etcd-client = "0.14"` (Cargo.toml) through the ADR-0003 three-test audit + `deny.toml` allowlist + TLS/auth posture (INTEGRATION §4); (d) record the board-visibility / sequencing governance decision (0015 `:461-463`, `:707-709`). Runnable check: `WYRD_ETCD_ENDPOINTS=<host:2379> cargo test -p wyrd-coordination-etcd --features etcd` against a throwaway single-node etcd, and expect `trait_contract_against_etcd` to run (not skip) and pass. |

### Advisory — adversary

# Adversarial review — issue 365 (coordination-etcd-l5-backend)

Lens: refute the red→green evidence and the reviewer's verdict; find the input that
breaks the etcd backend. Grounded on `$PDCA_TARGET` (branch head 5d87cc4).

## Attack on the evidence — what the gate actually proves

- **NEEDS-HUMAN — The deterministic gate never compiles or runs the production etcd
  implementation.** `crates/coordination-etcd/src/lib.rs` gates the whole store behind
  `#[cfg(feature = "etcd")] mod store;` and the feature is OFF by default
  (`crates/coordination-etcd/Cargo.toml:26` `default = []`). `crates/coordination-etcd/tests/conformance.rs:29`
  returns early with a clean-skip when `WYRD_ETCD_ENDPOINTS` is unset — which it is in
  CI. So `cargo xtask ci` (the only gating row, C4-ci) never touches the 338-line
  `store.rs`. The check-gates C4-verify row concedes this: *"no pre-patch state to
  isolate a RED against."* **Green CI ⇏ the etcd backend works.** Any reviewer claim that
  the red→green demonstrates the etcd path is correct is unwarranted: the etcd path is
  never executed by any automated check.

- **NEEDS-HUMAN — The promised automation to close that gap does not exist.**
  `crates/coordination-etcd/Cargo.toml:20` and `Cargo.toml` (root, `etcd-client` note)
  both point to *"a dedicated `xtask etcd-conformance` job (companion) [that] turns the
  feature on and drives the SHARED suite against a real etcd."* There is **no such job**:
  `grep etcd xtask/` finds only the `deploy`/`tikv` machinery; `xtask/src/main.rs:167`
  has a `tikv-conformance` target but no etcd analogue. The GREEN half therefore depends
  entirely on a human manually running `--features etcd` with an endpoint. This is worse
  than "deferred to sign-off" — there is no runnable target to defer *to*.

- **The RED half is demonstrated against a hand-written stub, never against
  `coordination-etcd`.** `crates/coordination-conformance/tests/demonstrated_red.rs`
  drives the five shared clauses against `BrokenCoordination` and asserts each panics. I
  tried to refute the non-vacuity and could not — each clause genuinely bites (constant
  lease id fails `second.id > first.id`; empty `discover` fails the `assert_eq!`; constant
  token fails every monotonicity check). But this proves *the suite* bites, **not** that
  `coordination-etcd` was ever red-then-green. The RED artifact (stub) and the GREEN
  artifact (real etcd) never meet on the production path inside any gate.

## Attack on the fix — concrete failing cases in `store.rs`

- **NEEDS-HUMAN — Won leaderships and held locks silently lapse after 30 s: no
  keep-alive is ever spawned.** `store.rs:196` (`elect_leader`) and `store.rs:231`
  (`lock`) each `lease_grant(HOLD_TTL_SECS=30, …)` and never renew it. The only
  `keep_alive` in the file is inside `renew()` (`store.rs:129`), the trait method for
  *registration* leases, invoked externally with a `Lease` handle. But `elect_leader`
  returns `Leadership { token }` and `lock` returns `LockGuard { token }`
  (`crates/traits/src/lib.rs:490,498`) — **token only, no lease handle** — so a caller has
  no way to renew leadership or a lock through the seam. Concrete failing case: a custodian
  wins leadership, does no coordination for 30 s, its lease lapses in etcd, and a peer can
  win a *second* leadership — while the original still believes it leads. The comment at
  `store.rs:33` ("kept renewed for the life of the hold in a real deployment") describes
  code that does not exist. The fast conformance suite completes in ≪30 s, so it can never
  observe this. This is a mem-vs-etcd semantic divergence (`coordination-mem` leaderships
  never lapse) that the shared suite is structurally unable to detect.

- **NEEDS-HUMAN — The re-fence path errors instead of re-campaigning once the hold
  lease has lapsed.** `store.rs:173-190`: a repeat `elect_leader` proclaims on the
  `LeaderKey` cached in `state.leaders`. If the 30 s lease from the previous bullet has
  lapsed, etcd has already deleted that leader key, so `proclaim(...).await?` returns an
  error and `elect_leader` yields `Err` rather than transparently re-acquiring. Concrete
  failing case: any caller that re-elects after an idle gap > 30 s gets a spurious error
  where `coordination-mem` returns a fresh rising token.

- **The etcd config path (`set_config`/`get_config`/`config_revision`) has zero test
  coverage even at sign-off.** `run_all` (`crates/coordination-conformance/src/lib.rs:136-147`)
  drives exactly five clauses — register/discover, leases, election, locks, fencing —
  **none of them config.** So `store.rs:294-337` is never exercised by the shared suite on
  *either* backend, and the etcd `config_revision` (max mod-revision over the config
  prefix) is untested against real etcd. This directly contradicts the brief's binding
  condition (a), which names *"config with a monotonic revision"* as a success criterion.
  A reviewer who reads "shared suite green on both backends" as covering config is
  mistaken.

- **The shared suite proves only single-instance semantics; the cross-process
  guarantees that are the crate's entire reason to exist are untested.** Every clause in
  `crates/coordination-conformance/src/lib.rs` operates on **one** `&impl Coordination`.
  `contract_locks_are_mutually_exclusive_and_fenced` (:75) acquires and contends on the
  *same* instance; `contract_election_is_always_granted_and_fenced` (:56) elects twice on
  the *same* instance (exercising the local-state proclaim short-circuit, not a real second
  campaigner). Nothing ever stands up two `EtcdCoordination` instances and checks that one
  process blocks/loses. `coordination-mem` passes the identical suite precisely because it
  is single-process — so the suite cannot distinguish "correct distributed backend" from
  "in-memory backend." The property that justifies the whole crate (cross-process single
  leader / mutual exclusion / peer discovery) is asserted nowhere.

- **`contract_leases_are_unique_and_rising` bakes in an etcd implementation detail as a
  contract.** The clause asserts `second.id > first.id` (`lib.rs:47`), and `store.rs:120`
  returns `lease_id as u64` straight from etcd's `lease_grant`. etcd lease IDs are
  documented as **opaque**; they are only monotone by virtue of the server's internal
  `idGen` within one session, and are `i64` cast to `u64` (a high-bit-set id becomes a
  huge u64). Concrete risk: across an etcd restart, id-generator reseed, or a version whose
  allocation differs, `second.id` can be ≤ `first.id` and this clause flakes/fails — a
  contract asserting more than the trait (opaque id) or etcd's API promises.

## Verdict — where the reviewer likely rationalized

- The check-gates "overall: pass" rests on C4-ci, which — per the two evidence findings
  above — exercises none of the etcd code. Reading C4-verify's "no pre-patch state" as
  benign is the rationalization: it is *because* the flippable regression's green half was
  moved entirely off the automated path. The mem-suite green + `demonstrated_red` green
  prove the trait was refactored without regressing impl #1 and that the suite is
  non-vacuous — real, but a strictly weaker claim than "the etcd second implementation is
  correct," which is what the brief's success criterion actually demands.

## Attempted refutations that held

- Tried to break the shared clauses on `coordination-mem` and the `keyspace`/`fencing`
  unit tests (NUL-delimiter aliasing, class-collision, `prefix_range_end` carry,
  `token_from_revision` non-positive guard): all sound; could not refute.
- Tried to make `demonstrated_red.rs` pass vacuously (a stub that slips through): each of
  the five clauses genuinely panics on `BrokenCoordination`; could not refute.
- Within a single instance and a fast run, the etcd register→discover, try-acquire lock,
  and proclaim-based re-fence logic are internally consistent; the defects above surface
  on the time axis (30 s lapse), the cross-process axis, and the untested-config axis —
  none of which the gate or the suite can reach.

### Advisory — codex

- `crates/coordination-etcd/src/store.rs:231` — Lock and leadership leases are granted with a fixed 30s TTL but no keep-alive is started or retained, so a live holder can silently lose a lock/leadership term while still holding the returned `LockGuard`/`Leadership`; worse, stale `unlock` later deletes the lock key unconditionally at `crates/coordination-etcd/src/store.rs:288`, which can release a newer holder's reacquired lock after the old lease expired.
- NEEDS-HUMAN — `Cargo.toml:101` — The new `etcd-client = "0.14"` dependency requires a system `protoc` in its build script; `CARGO_TARGET_DIR=/tmp/pdca-advisory-03mbuhet/cargo-target cargo check -p wyrd-coordination-etcd --features etcd` fails here before checking this crate's Rust code with "Could not find `protoc`", while this repo's own protobuf path documents a no-system-`protoc` posture elsewhere. Decide whether to vendor/provision `protoc` or document it as a Tier-2 prerequisite.
- NEEDS-HUMAN — `crates/server/src/cli.rs:297` — The gateway/client cluster paths still require static `--endpoints` and never resolve D-server endpoints through the selected `Coordination` backend; only `d-server` parses `--coordination-backend` at `crates/server/src/cli.rs:490`. If this issue is meant to satisfy "peers discovered through L5" rather than only the backend crate half, the consumer side is still missing.
- NEEDS-HUMAN — `crates/coordination-etcd/tests/conformance.rs:68` — The real-etcd conformance test exits successfully when `WYRD_ETCD_ENDPOINTS` is set but the crate was built without `--features etcd`, and there is no `xtask etcd-conformance` command present in the visible tree despite the test message. A miswired Tier-2 job can therefore report green without exercising the new backend.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T5 Judgment — Oracle is "reviewer + human sign-off" (check-gates.json:98). Judgment calls left open by design and NOT silently absorbed: (1) DST-fidelity story for etcd (`madsim-etcd-client` vs contract harness, #264/#258 mirror), (2) whether any etcd semantic gap should force a trait change (must stay a NEEDS-HUMAN, not a silent edit). Decide before merge whether the deferred-DST posture is acceptable for a production L5 backend.
- [ ] Validation — fitness-to-purpose — The core deliverable — "shared suite GREEN against real etcd" (success criterion b) — is verified by NO gate: it needs a live etcd via `xtask etcd-conformance` + `--features etcd`, which no headless runner in this session can stand up. Human must, at sign-off: (a) run the etcd conformance job and confirm it passes AND that `store.rs` even compiles under `--features etcd` (its own header flags the `etcd-client` API entry points as "reconfirmed at build time"); (b) sanity-check two networked assumptions the contract leans on — that etcd `lease_grant` IDs satisfy `second.id > first.id` (`contract_leases_are_unique_and_rising`) and that campaign/lock revisions stay globally monotonic across kinds; (c) clear the new external gRPC dependency `etcd-client = "0.14"` (Cargo.toml) through the ADR-0003 three-test audit + `deny.toml` allowlist + TLS/auth posture (INTEGRATION §4); (d) record the board-visibility / sequencing governance decision (0015 `:461-463`, `:707-709`). Runnable check: `WYRD_ETCD_ENDPOINTS=<host:2379> cargo test -p wyrd-coordination-etcd --features etcd` against a throwaway single-node etcd, and expect `trait_contract_against_etcd` to run (not skip) and pass.
- [ ] `Cargo.toml:101` — The new `etcd-client = "0.14"` dependency requires a system `protoc` in its build script; `CARGO_TARGET_DIR=/tmp/pdca-advisory-03mbuhet/cargo-target cargo check -p wyrd-coordination-etcd --features etcd` fails here before checking this crate's Rust code with "Could not find `protoc`", while this repo's own protobuf path documents a no-system-`protoc` posture elsewhere. Decide whether to vendor/provision `protoc` or document it as a Tier-2 prerequisite.
- [ ] `crates/server/src/cli.rs:297` — The gateway/client cluster paths still require static `--endpoints` and never resolve D-server endpoints through the selected `Coordination` backend; only `d-server` parses `--coordination-backend` at `crates/server/src/cli.rs:490`. If this issue is meant to satisfy "peers discovered through L5" rather than only the backend crate half, the consumer side is still missing.
- [ ] `crates/coordination-etcd/tests/conformance.rs:68` — The real-etcd conformance test exits successfully when `WYRD_ETCD_ENDPOINTS` is set but the crate was built without `--features etcd`, and there is no `xtask etcd-conformance` command present in the visible tree despite the test message. A miswired Tier-2 job can therefore report green without exercising the new backend.

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Rejected: the adversary refuted the fix itself (not just the evidence) — a production L5 backend would ship split-brain. The gating C4-ci never compiles/runs store.rs (etcd feature OFF by default), so "green" only proves impl #1 wasn't regressed + the suite is non-vacuous, not that the etcd backend is correct. Rebuild to fix all the red correctness defects: 1. Split-brain / no lease keep-alive: `elect_leader` (store.rs:196) and `lock` (store.rs:231) grant a 30s lease and never renew it, and the seam returns a token-only `Leadership`/`LockGuard` (traits/src/lib.rs:490,498) so a caller cannot renew. A leader/lock silently lapses after 30s and a peer can win a second leadership while the original still believes it leads. Spawn/retain a keep-alive for the life of the hold (store.rs:33 comment describes code that does not exist), and/or expose a renewable handle through the trait seam (trait change stays a NEEDS-HUMAN, not a silent edit). 2. Unconditional unlock (store.rs:288): stale `unlock` deletes the lock key unconditionally, so it can release a NEWER holder's reacquired lock after the old lease expired. Gate the delete on still-holding the lease/mod-revision. 3. Re-fence path (store.rs:173-190): a repeat `elect_leader` proclaims on the cached LeaderKey; once the 30s lease has lapsed etcd deleted that key, so it returns Err instead of transparently re-campaigning. Re-acquire and return a fresh rising token (mem returns one). 4. Config path untested: `run_all` drives 5 clauses, none config, so store.rs:294-337 (set_config/get_config/config_revision) is exercised on neither backend — contradicting binding criterion (a) "config with a monotonic revision." Add a config clause to the shared suite. 5. Single-instance-only suite: every shared clause operates on one `&impl Coordination`, so the cross-process guarantees that justify the whole crate (single leader / mutual exclusion / peer discovery across processes) are asserted nowhere; mem passes precisely because it is single-process. Add clauses that stand up two `EtcdCoordination` instances and assert one blocks/loses. Also (contract fidelity): `contract_leases_are_unique_and_rising` (conformance lib.rs:47) asserts `second.id > first.id`, but etcd lease IDs are opaque (i64->u64) and only monotone within a session — can flake across restart/reseed; relax to what the trait/etcd actually promise. Not blocking the correctness rebuild but must be resolved before the real-etcd GREEN can be earned: there is no `xtask etcd-conformance` job (the promised automation does not exist), the real-etcd conformance test exits green when WYRD_ETCD_ENDPOINTS is set but built without --features etcd (a miswired Tier-2 job reports false green), and `etcd-client 0.14` needs system `protoc` against the repo's no-system-protoc posture. The mem-suite refactor lift + demonstrated_red non-vacuity are genuine and can be kept. </content>
- By / date: Eduard Ralph / 2026-07-04

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
