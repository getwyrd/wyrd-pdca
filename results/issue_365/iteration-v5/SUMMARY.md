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

**Task under review:** build the ADR-0006 REQUIRED *second* `Coordination` implementation — a networked, etcd-backed `coordination-etcd` crate behind the byte-for-byte-unchanged L5 trait — plus **one shared conformance suite** that both `coordination-mem` and `coordination-etcd` pass, selectable by `server` composition with no caller edits. This is iteration 5; iterations 1–4 were rejected. The iter-4 blockers were: (1) the headline single-leader/split-brain property ran only on the madsim simulator and was NOT in the real-etcd conformance; (2) a keyspace prefix-collision bug (`reg/a/` matched a member of `a/b`); (3) the transient-proclaim-error anti-churn path had no test.

**Grounding caveat:** `PDCA_TARGET` is not reachable in this environment (no target worktree, no cargo/docker/protoc), so per protocol every citation is grounded on `patch.diff` and I could **not** independently re-run the gates or exercise etcd. Gate status is taken from `check-gates.json` (C4-ci = pass), and the residual "real-etcd green is not produced by any CI gate" is routed to Validation/T5 as a human item — it is an environment/fidelity decision, not a patch defect, so it is not raised as a blocking C4 FAIL.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Scope matches the brief's binding facts: net-new `coordination-etcd` (`store.rs`, `keyspace.rs`, `fencing.rs`, `hold.rs`), one shared `wyrd-coordination-conformance` suite both backends drive (`coordination-conformance/src/lib.rs:1293` `run_all`), and a config-selected `server` composition (`server/src/cli.rs:3455`) — trait/core/custodian untouched (grep for `crates/(traits|core|custodian)/src` in patch: no matches). Half-2 (process roles) correctly excluded per the 2026-07-04 decision. |
| C2 Reproduction (red pre-fix) | PASS | No pre-existing runtime bug (net-new feature); the flippable RED is codified as demonstrated-red — every shared clause goes RED against a deliberately-violating stub (`coordination-conformance/tests/demonstrated_red.rs:1312`) and the cross-instance clauses go RED against two process-local mem instances (`dst/tests/coordination.rs:3185-3210`, `#[should_panic]`). Non-vacuity is pinned, not asserted. |
| C3 Change | PASS | Coherent and contained: real etcd lease/keep-alive lifecycle, conditional-by-lease unlock (`store.rs:2518`), fresh-campaign-on-loss election (`store.rs:2405`), one code path served two ways (real `etcd-client` under `--features etcd`; `madsim-etcd-client` under `--cfg madsim`, `coordination-etcd/Cargo.toml:1816`). Blast radius = new crates + one composition site. |
| C4 Verification (red→green) | PASS (gate green; scope-limited) | `check-gates.json` C4-ci = pass; C4-verify N/A ("no pre-patch state to isolate a RED"). The gated green is the **madsim simulator's** (`dst/tests/coordination.rs`, in `ci`), which drives the same store source. I could not re-run gates (no toolchain). The **real-etcd** green (criterion (b)) is earnable only via off-CI `cargo xtask etcd-conformance` (docker+protoc) and is not produced by any gate here — routed to Validation, not failed here (would fabricate an ordering blocker). |
| C5 Causal adequacy | PASS | Root cause (trait pinned by only one impl) is removed by building a genuine networked second impl, not guarded around. The iter-4 churn root cause is addressed at source: loss is concluded from the keep-alive's authoritative `is_lost()` signal, never inferred from a proclaim RPC error (`store.rs:2417-2431`). No capability-probe / runtime-guard-over-optional-capability smell — `#[cfg(feature="etcd")]` gates are compile-time composition, not a load-time-side-effect paper-over. |
| T1 Structure | PASS | Pure logic (`keyspace`, `fencing`, `hold`) carries no etcd dep and is unit-tested on every build incl. feature-off `ci` (`keyspace.rs:2006`, `fencing.rs:1866`, `hold.rs:1907`); shared suite depends on `traits` only (ADR-0016). Correct workspace/feature wiring (`Cargo.toml:876`, `server/Cargo.toml:3428`). |
| T2 Shape | PASS | Single `run_all` runner so a new clause is picked up by both backends without a per-driver list to drift (`lib.rs:1293`); config-only-advancement and renew/revoke clauses now in the shared suite (`lib.rs:1130,1043`) — the previously-uncovered paths. Assertions target the property, with demonstrated-red counterparts. |
| T3 Runtime | PASS (what runs here) | mem suite on pollster, etcd store on madsim in `ci` (gate green per check-gates.json). iter-4 blockers now runtime-exercised: single-leader across two etcd instances (`dst/tests/coordination.rs:3140`; and on **real** etcd at `coordination-etcd/tests/conformance.rs:2683`), keyspace hierarchical-isolation regression (`keyspace.rs:2038`), transient-proclaim retains hold+lease (`dst/tests/coordination.rs:3362`). Real-etcd runtime not exercisable here (no docker/protoc). |
| T4 Contribution | PASS | Adds real, exercisable capability (cross-process discovery / single-leader / fencing) and its regression floor; the false-green is closed — `run_etcd_conformance` now returns `Err` when docker/protoc are missing rather than `Ok` (`xtask/src/main.rs:3790-3809`). |
| T5 Judgment | NEEDS-HUMAN | Standing decisions owed at sign-off, none code-fixable: (a) DST-fidelity acceptance — is the `madsim-etcd-client` simulator an adequate stand-in for real-etcd correctness (#264/#258 mirror)? (b) `etcd-client 0.14` dependency review — ADR-0003 three-test audit + `deny.toml` allowlist + the ships-no-TLS/auth `Client::connect(endpoints, None)` posture (`store.rs:2331`); (c) sequencing governance (explicit M4 slice vs preceding milestone). Impact: these gate whether a distributed L5 dependency may enter the shipped graph. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | **Decision owed:** does criterion (b) — "shared suite + single-leader GREEN on **REAL** etcd" — count as satisfied? The single-leader clause is now present in the real-etcd conformance (`coordination-etcd/tests/conformance.rs:2683`) and the false-green is fixed, so the job is now *earnable*; but no CI gate produces that green (off-CI, needs docker+protoc). The human must either **run `cargo xtask etcd-conformance`** against a live etcd and confirm it passes, or explicitly accept the madsim-simulator proof as the shipping bar — and clear the T5 dependency/fidelity/sequencing calls — before this backend enters the graph. This is the recurring axis of the prior rejections; the concrete iter-4 defects are resolved, leaving this a genuine human/environment gate. |

## Notes for the human clearing §6
- The three concrete iter-4 blockers are **resolved in the diff**: single-leader is now in the real-etcd conformance; the keyspace prefix-collision is fixed via injective `encode_segment` with a hierarchical-isolation regression test; the transient-proclaim path has a load-bearing gated test.
- Prior-art / rejected-work check: iterations v1–v4 are preserved and their carry-forward findings were each traced to a corresponding fix in this diff (verified by reading `store.rs`, `keyspace.rs`, the conformance + dst tests).
- I could not execute the stash→red / unstash→green re-run or `cargo xtask ci` myself (no reachable target checkout or toolchain in this environment); the C4 verdict rests on `check-gates.json` plus static grounding of the cited paths on `patch.diff`.

### Advisory — adversary

# Adversarial review — issue 365 / coordination-etcd-l5-backend (iteration 5)

Skeptic's pass. Grounded on the target tree at `/home/eddie/wyrd/wyrd.pdca-wt-l0`.
Advisory only — nothing here gates. The prior four rejections drove the crate to a
genuinely stronger place (store compiled+driven under madsim; split-brain guard gated
demonstrated-red; single-leader now checked in the real-etcd conformance too). Below is
what I could still break and what I tried but could not.

## Refutations (the fix)

- **NEEDS-HUMAN — `election_name` reintroduces exactly the hierarchical-prefix-collision
  bug that iteration 4 forced the builder to fix for *registration* — but only registration
  got the fix.** `crates/coordination-etcd/src/keyspace.rs:58` (`election_name`) formats the
  raw caller key: `format!("{ns}{ELECT}{key}")`, with **no** `encode_segment`, while
  `registration_member`/`registration_prefix` (keyspace.rs:43,48) *do* encode it precisely to
  stop `/` in a logical key forging an etcd key-prefix nesting. etcd's election API (used at
  `store.rs:324-329`) determines leadership by the **min-create-revision under the prefix
  `<name>/`** and its candidate keys live under that prefix — this diff's own test confirms it
  (`crates/dst/tests/coordination.rs:399-404` reads candidate keys with
  `get("lapse/elect/custodian", with_prefix())`). Concrete failing case: an instance holding
  `elect_leader("custodian")` (candidate under `…/elect/custodian/`) and any instance calling
  `elect_leader("custodian/shard-a")` (candidate `…/elect/custodian/shard-a/<hex>`, which
  *starts_with* `…/elect/custodian/`). The child's candidate now falls inside the parent
  election's leadership range; if it holds the lower create-revision, the parent election
  `waitDeletes` behind a key that will never resign for it — the parent's `elect_leader`
  blocks or resolves to the wrong holder. The trait's `elect_leader(&self, key: &str)` places
  no restriction on keys, and the registration path was defended for exactly this arbitrary-
  hierarchical-key case; elections were not. A human must decide whether hierarchical election
  keys are in-contract (then this is a defect: encode the segment as registration does, adding
  the mirror of `discovery_prefix_isolates_hierarchical_keys` for elections) or explicitly
  out-of-contract (then document + assert the restriction). It is caught by **no** test on
  either backend — every election clause uses flat keys (`"custodian"`, `"y"`).

- **The gated green never runs the code path this bug lives on — criterion (b) rests on the
  simulator alone.** `check-gates.json:33-48`: the only gating row is `C4-ci`, and `C4-verify`
  is non-gating with `path_line` admitting *"no pre-patch state to isolate a RED against."*
  The real-etcd conformance (`crates/coordination-etcd/tests/conformance.rs`) is skipped unless
  `WYRD_ETCD_ENDPOINTS` is set (:35-41), and `cargo xtask etcd-conformance` hard-requires
  docker **and** system `protoc` (`xtask/src/main.rs:293-312`) — neither present in a PDCA
  worktree, so it was not run here. Brief criterion (b) is *"the shared suite is green on both
  backends (real etcd via a Tier-2 compose target)."* No artifact in `check-gates.json` shows a
  real-etcd run; the green is the **madsim simulator's** (dst tier). Any reviewer claim that (b)
  is *satisfied* (as opposed to *earnable*) is unwarranted on this evidence, and the election-
  prefix defect above is precisely the class of real-etcd behaviour a simulator can mask.

## Attempted but could not refute

- **Real-etcd single-leader (the iteration-4 reject).** Now genuinely present at
  `crates/coordination-etcd/tests/conformance.rs:102-109`, and it is robust: a `b.elect_leader`
  that returned `Err` would `unwrap()`-panic (line 107 `.map(|r| r.unwrap())`), and a wrong
  concurrent `Ok` fails `is_none()` — the only pass path is B genuinely blocking. Could not
  turn it into a false green.
- **Orphaned cancelled campaign / detached keep-alive leak** (iterations 1–2 reject): the guard
  now lives on the campaign future's stack and `Drop`/`stop_without_revoke` revoke on
  cancellation (`store.rs:95-104,309-342`), gated by `a_cancelled_campaign_leaks_no_orphan`
  (dst/tests/coordination.rs:332-365). Held.
- **Transient-proclaim-error self-churn** (iterations 3–4 reject): loss is read only from the
  keep-alive's authoritative `is_lost()` (`store.rs:76,285-304`), and
  `a_transient_proclaim_error_keeps_the_hold_and_its_lease` (dst:448-501) pins that a proclaim
  error retains the live lease. Held.
- **`config_revision` config-only advancement** (iteration 3): etcd path takes `max(mod_revision)`
  over the config prefix (`store.rs:418-441`), and `contract_config_is_revisioned:219-228`
  asserts an unrelated `register` does *not* move it — non-vacuous and correct for the etcd
  keyspace layout.
- **`unlock` releasing a newer holder / registration prefix bleed** (iterations 1,4): `unlock`
  revokes only its own lease (`store.rs:386-398`) and `registration_member` is `/`-and-`%`
  encoded (keyspace.rs:43-50, tests :94-127). Held. Note `lock_key`/`config_key` are also
  unencoded but are used with **exact-key** ops (no prefix range), so unlike elections they do
  not collide — benign, mentioned only for completeness.

### Advisory — codex

- NEEDS-HUMAN — `crates/coordination-etcd/Cargo.toml:23` still makes the real etcd backend depend on `etcd-client`, whose build script requires a system `protoc`; `CARGO_TARGET_DIR=/tmp/pdca-advisory-parldrau/target-etcd cargo check -p wyrd-coordination-etcd --features etcd` fails here with “Could not find `protoc`”. This leaves the feature-on production backend non-buildable in a clean no-system-protoc environment unless humans explicitly accept that new toolchain prerequisite or require vendoring/prost setup.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T5 Judgment — Standing decisions owed at sign-off, none code-fixable: (a) DST-fidelity acceptance — is the `madsim-etcd-client` simulator an adequate stand-in for real-etcd correctness (#264/#258 mirror)? (b) `etcd-client 0.14` dependency review — ADR-0003 three-test audit + `deny.toml` allowlist + the ships-no-TLS/auth `Client::connect(endpoints, None)` posture (`store.rs:2331`); (c) sequencing governance (explicit M4 slice vs preceding milestone). Impact: these gate whether a distributed L5 dependency may enter the shipped graph.
- [ ] Validation — fitness-to-purpose — **Decision owed:** does criterion (b) — "shared suite + single-leader GREEN on **REAL** etcd" — count as satisfied? The single-leader clause is now present in the real-etcd conformance (`coordination-etcd/tests/conformance.rs:2683`) and the false-green is fixed, so the job is now *earnable*; but no CI gate produces that green (off-CI, needs docker+protoc). The human must either **run `cargo xtask etcd-conformance`** against a live etcd and confirm it passes, or explicitly accept the madsim-simulator proof as the shipping bar — and clear the T5 dependency/fidelity/sequencing calls — before this backend enters the graph. This is the recurring axis of the prior rejections; the concrete iter-4 defects are resolved, leaving this a genuine human/environment gate.
- [ ] `crates/coordination-etcd/Cargo.toml:23` still makes the real etcd backend depend on `etcd-client`, whose build script requires a system `protoc`; `CARGO_TARGET_DIR=/tmp/pdca-advisory-parldrau/target-etcd cargo check -p wyrd-coordination-etcd --features etcd` fails here with “Could not find `protoc`”. This leaves the feature-on production backend non-buildable in a clean no-system-protoc environment unless humans explicitly accept that new toolchain prerequisite or require vendoring/prost setup.

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
- Iteration delta (if iterating): Rejected (iter-5): the real-etcd conformance test does not compile under `--features etcd` — `crates/coordination-etcd/tests/conformance.rs:104` calls `b.elect_leader(...)` with no `use wyrd_traits::Coordination;` in scope (E0599). Criterion (b) is therefore unearned: the single-leader clause the iter-4 reject demanded exists only in a file no gate compiles (CI green is the madsim/cfg path; `--features etcd` is off-CI, needs protoc). This is exactly the recurring axis. Fixes for the rebuild: (1) Add the missing `use wyrd_traits::Coordination;` import so the real-etcd conformance compiles, and make `cargo xtask etcd-conformance` actually go GREEN against a live etcd. Verified at sign-off: docker present, protoc now installed on the sign-off host — the test still fails to BUILD, so this is a plain defect, not an environment gap. (2) The `xtask etcd-conformance` retry loop misreports a hard compile error as "etcd may still be bootstrapping" and retries it 5× — distinguish a build failure from a bootstrap flake so a non-compiling test can't masquerade as transient. (3) Adversary finding still open — `election_name` (keyspace.rs:58) formats the raw key with no `encode_segment` while `registration_member` does, reintroducing the iter-4 hierarchical-prefix-collision class for elections; caught by no test (all election clauses use flat keys). Decide election keys in-/out-of-contract and either encode+regression-test (mirror `discovery_prefix_isolates_hierarchical_keys`) or document+assert the restriction. Standing human calls (DST-fidelity acceptance, etcd-client 0.14 dependency review, sequencing governance) remain owed at the next Check. Env note going forward: protoc is now installed on the sign-off host, so `cargo xtask etcd-conformance` can be re-run at the next Check to confirm real-etcd green before any accept.
- By / date: Eduard Ralph / 2026-07-04

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
