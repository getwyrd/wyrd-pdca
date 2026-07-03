# Result — issue 252 / metadata-tikv-skeleton-conformance

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: 
- Success criterion: the shared `MetadataStore` trait-contract suite (the assertions currently at `crates/metadata-redb/tests/conformance.rs:20-111`, lifted to one shared home — NOT copy-forked) passes against a real TiKV for the `get`/`scan`/`commit` basics; redb still passes the same shared suite; CI can reach a TiKV via a throwaway single-node in `deploy/`; `cargo xtask ci` exits 0 with the pinned `tikv-client` dependency tree (`cargo deny check` green, ADR-0003) — including on a machine with **no** TiKV, where the TiKV-backed test skips cleanly instead of failing.
- Repo + branch target: getwyrd/wyrd @ main (feature branch `feat/m4.1-metadata-tikv-skeleton`; commit subject `feat(metadata-tikv): … (M4.1, #252)` per proposal 0007 §"Suggested PR sequence")
- Scope (one logical fix) / out of scope: the one logical change is the M4.1 slice as proposal 0007 item 1 defines it: (a) new crate `crates/metadata-tikv` with `[dependencies]` = `wyrd-traits` + `tikv-client` (pinned version) + `tokio` **only** — never `core` or another concrete backend (ADR-0016; dev-deps for the test harness may follow the `metadata-redb/Cargo.toml` dev-dep precedent); (b) `impl MetadataStore for TikvMetadataStore` covering the basic `get` / `scan` / `commit` shapes; (c) lift the generic trait-contract functions out of `crates/metadata-redb/tests/conformance.rs` into a shared home both backends' test targets drive (the file's own header says they were written to lift; redb's model/property tests stay where they are); (d) a throwaway **single-node** TiKV under `deploy/` (new, outside the Cargo workspace — proposal 0007 §"Crate touch-points") + the CI wiring so the TiKV run actually executes there, while the test **skips cleanly when no TiKV endpoint is configured** (the `cargo xtask ci` gate must stay green on a laptop/worktree with no TiKV); (e) pin `tikv-client` in the workspace `Cargo.toml`, confirm its futures are `Send + Sync` for the object-safe trait, and keep `cargo-deny` green. / **Out of scope:** the rigorous atomic-commit conflict semantics — `get_for_update` locking discipline, write-conflict → `Ok(Conflict)` classification, version-CAS-under-contention properties (M4.2, #253); native paged prefix scan + read-consistency doc (M4.3, #254 — a whole-range-filter shortcut is acceptable in the skeleton); the `server` backend selector (M4.4, #255); the production `deploy/` tier — TiKV/PD cluster + etcd ensemble (M4.5, #256); Jepsen/Tier-1/Tier-2 (M4.6, #257); the DST simulated-TiKV / contract harness (M4.7, #258); **any change to the `MetadataStore` trait itself** (the milestone's premise — a trait edit is a failure of M4's thesis, proposal 0007 §"Crate touch-points").

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix (implement — accepted-plan feature slice behind already-Accepted ADR-0008; no new ADR is minted, proposal 0007 §"Alternatives considered")
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): fail — xtask: `cargo test --workspace --exclude wyrd-dst` failed with exit status: 101
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: fail — ./engine/scripts/run-verify.sh
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

# Check review — issue 252 / metadata-tikv-skeleton-conformance

**Task under review:** M4.1 (getwyrd/wyrd #252) — stand up the `metadata-tikv` crate skeleton (basic `get`/`scan`/`commit` over TiKV's transactional API), lift redb's trait-contract conformance assertions into one **shared** suite both backends drive, add a throwaway single-node TiKV under `deploy/` plus the CI wiring to run it, while `cargo xtask ci` stays green on a machine with no TiKV (proposal 0007 §"Suggested PR sequence" item 1, per draft 0015 where they differ).

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Brief pointer grounds on the target: proposal 0007 exists (`docs/design/proposals/accepted/0007-milestone-4-production-metadata-backend.md`), ADR-0008/0016/0003 present under `docs/design/adr/`; the success criterion is concrete and testable (shared suite green on both backends, clean skip with no TiKV, deny green). Nothing turns on a human here. |
| C2 Reproduction (red pre-fix) | PASS | Structural red re-verified on the target base: `crates/` has no `metadata-tikv` and no `deploy/` exists (verified by listing `/home/eddie/wyrd/wyrd.pdca-wt/crates`), so the brief's named test target `crates/metadata-tikv/tests/conformance.rs` cannot even build pre-patch — red by construction. |
| C3 Change | FAIL | Scope item (d) is half-delivered: the patch adds `deploy/tikv-single-node/docker-compose.yml` and the `xtask tikv-conformance` runner (patch.diff `xtask/src/main.rs` hunks), but **no `.github/workflows/*` invokes it** — the repo's precedent for exactly this tier is a workflow (`.github/workflows/integration-nightly.yml:50-51` → `cargo xtask integration`). As landed, no CI job ever exercises TiKV, so "CI can reach a TiKV via a throwaway single-node in `deploy/`" is unmet. Decision owed: require the nightly-style workflow in this slice, or explicitly accept its deferral. Everything else in scope grounds clean: `traits/src/lib.rs` untouched by the diff (brief invariant), deps mirror the redb precedent incl. `async-trait`/`bytes` (`crates/metadata-redb/Cargo.toml:11-15`), suite lifted verbatim, not forked. |
| C4 Verification (red→green) | FAIL | Both gates are red (C4-ci: `cargo test --workspace --exclude wyrd-dst` exit 101 after fmt/clippy/build passed; C4-verify: no detail recorded). I could **not** re-run them — this review sandbox denies cargo/git/docker execution — and the target worktree holds base only (no `wyrd_metadata_tikv`/shared-suite artifacts in `target/debug/deps`), so the red ran in another checkout I cannot read. Static re-derivation finds no in-patch cause: every trait usage matches `crates/traits/src/lib.rs:338-419`, imports/feature gates check out, the lifted assertions are byte-identical, and no existing package version changes in the lock (additions + duplicate-major entries only). Decision owed: pull the actual `cargo test` log and attribute the exit-101 to a named failing test (patch defect) vs the gate runner's environment before any accept — the gate row names no failing test, which is itself a gap. |
| C5 Causal adequacy | NEEDS-HUMAN | The symptom-guard smell-test fires: the OFF-by-default `tikv` feature (`crates/metadata-tikv/Cargo.toml` `[features]`) means `cargo xtask ci` never compiles **or deny-audits** the pre-1.0 `tikv-client` tree — `cargo deny check` runs bare (`xtask/src/main.rs:647-652`), cargo-deny's default graph excludes feature-off optional deps, and `deny.toml` is untouched by the patch, so the success criterion's "ci exits 0 **with the pinned tikv-client dependency tree** (cargo deny check green)" is satisfied only by keeping the tree out of the audited graph. Decision owed: is feature-gated audit deferral an acceptable reading of ADR-0003 for M4.1, or must the tree enter ci's audited graph (deny.toml allowlist work) now? (The endpoint probe `WYRD_TIKV_PD_ENDPOINTS` → clean skip is spec-mandated by the brief, not a paper-over.) |
| T1 Structure | PASS | New crates mirror repo norms: `metadata-tikv`/`metadata-conformance` follow the `metadata-redb` Cargo.toml shape incl. `[lints] workspace = true`; `deploy/` sits outside the workspace members (root `Cargo.toml:9-23` + patch adds only the two crates); the shared suite is a trait-only sibling crate (ADR-0016); `run_tikv_conformance` is modeled on the existing `run_integration` (`xtask/src/main.rs:103-114`). |
| T2 Shape | PASS | Implementation shapes match the frozen trait exactly (`get`/`scan`/`commit` vs `crates/traits/src/lib.rs:338-351`); `WriteBatch`/`Precondition` fields used as declared (`:365-383`); precondition failure returns `Ok(Conflict)`, not `Err`, per `:348-350`; the `keyspace` module is dependency-free and unit-tested incl. the `0xff` upper-bound edge cases; whole-range shortcut is explicitly allowed for the skeleton (brief, M4.3 deferral). |
| T3 Runtime | NEEDS-HUMAN | Neither runtime leg could be exercised here (sandbox denies execution; C4's red also blocks inference). Owed observations, concrete steps: (1) no-TiKV leg — on a machine without TiKV run `cargo xtask ci`, expect green with the skip line "WYRD_TIKV_PD_ENDPOINTS not set — skipping"; (2) real leg — with Docker, run `cargo xtask tikv-conformance`, expect "TiKV passed the shared MetadataStore conformance suite" and the compose project `wyrd-tikv-m41` torn down afterward (`docker ps` clean). |
| T4 Contribution | NEEDS-HUMAN | The change is additive and self-contained (no drive-by edits outside scope), but the prior-art check by affected path could not be mechanically settled — git execution is denied in this sandbox. Owed: `git log --oneline -- crates/metadata-redb/tests/conformance.rs xtask/src/main.rs` plus a closed/rejected-PR search for `metadata-tikv`/`conformance` to confirm no prior attempt was rejected on grounds this patch re-trips. |
| T5 Judgment | PASS | Where checkable, the judgment calls are sound and documented in-code: verbatim (not rewritten) suite lift; per-clause namespace isolation against one shared cluster with pid-scoped keys; compose teardown on all paths incl. failure; bounded PD readiness wait; the two genuinely contested calls are already carried as the C3 and C5 rows rather than buried. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Two calls only the human can make: (1) the ADR-0003 three-test audit + `deny.toml` allowlist decision on `tikv-client 0.4.0` (pre-1.0; ~2.3k new lock lines incl. duplicate majors such as `indexmap 1.9.3` and `itertools 0.12.1` alongside the existing 2.x/0.13 — INTEGRATION §4, research issue #260); (2) whether the skeleton as built — shared suite + on-demand-only TiKV run, rigorous conflict semantics deferred to M4.2 — proves ADR-0008's applicability well enough to found M4.2–M4.7 on it. |

## Notes

- **Target-state caveat (not a patch defect):** `$PDCA_TARGET` resolves to `/home/eddie/wyrd/wyrd.pdca-wt`, which is a clean base checkout — the patch is not applied there and its `target/debug/deps` contains only base-state test binaries, so the failing C4 gates ran in a different checkout this reviewer cannot read. Citations above ground on that base target plus `patch.diff`.
- **Reviewer execution limits:** this session's sandbox denied all process execution (cargo, git, docker, even `printenv`), so gate re-runs and the red→green replay were impossible; every verdict above is derived from read-only grounding. The C4 FAIL rows mirror the deterministic gates and are annotated with what the static trace could and could not establish — the missing failing-test name in the gate output is the first thing to recover.
- **§6 items expected from this review:** C3 (CI wiring: require workflow vs accept deferral), C4 (attribute the exit-101 red), C5 (feature-gated deny-audit deferral vs ADR-0003), T3 (two runtime observations, steps above), T4 (prior-art by path), V (tikv-client audit + ADR-0008 applicability).

### Advisory — codex

- NEEDS-HUMAN — `Cargo.toml:86` adds `tikv-client = "0.4"` and the submitted lockfile includes its native TLS/OpenSSL transitive tree; this needs the ADR-0003 dependency/license/advisory adjudication called out in the brief, and the feature-on compile path in my temp-applied check failed before Rust code on `openssl-sys` because `pkg-config`/OpenSSL discovery was unavailable.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C5 Causal adequacy — The symptom-guard smell-test fires: the OFF-by-default `tikv` feature (`crates/metadata-tikv/Cargo.toml` `[features]`) means `cargo xtask ci` never compiles **or deny-audits** the pre-1.0 `tikv-client` tree — `cargo deny check` runs bare (`xtask/src/main.rs:647-652`), cargo-deny's default graph excludes feature-off optional deps, and `deny.toml` is untouched by the patch, so the success criterion's "ci exits 0 **with the pinned tikv-client dependency tree** (cargo deny check green)" is satisfied only by keeping the tree out of the audited graph. Decision owed: is feature-gated audit deferral an acceptable reading of ADR-0003 for M4.1, or must the tree enter ci's audited graph (deny.toml allowlist work) now? (The endpoint probe `WYRD_TIKV_PD_ENDPOINTS` → clean skip is spec-mandated by the brief, not a paper-over.)
- [x] T3 Runtime — Neither runtime leg could be exercised here (sandbox denies execution; C4's red also blocks inference). Owed observations, concrete steps: (1) no-TiKV leg — on a machine without TiKV run `cargo xtask ci`, expect green with the skip line "WYRD_TIKV_PD_ENDPOINTS not set — skipping"; (2) real leg — with Docker, run `cargo xtask tikv-conformance`, expect "TiKV passed the shared MetadataStore conformance suite" and the compose project `wyrd-tikv-m41` torn down afterward (`docker ps` clean).
- [x] T4 Contribution — The change is additive and self-contained (no drive-by edits outside scope), but the prior-art check by affected path could not be mechanically settled — git execution is denied in this sandbox. Owed: `git log --oneline -- crates/metadata-redb/tests/conformance.rs xtask/src/main.rs` plus a closed/rejected-PR search for `metadata-tikv`/`conformance` to confirm no prior attempt was rejected on grounds this patch re-trips.
- [x] Validation — fitness-to-purpose — Two calls only the human can make: (1) the ADR-0003 three-test audit + `deny.toml` allowlist decision on `tikv-client 0.4.0` (pre-1.0; ~2.3k new lock lines incl. duplicate majors such as `indexmap 1.9.3` and `itertools 0.12.1` alongside the existing 2.x/0.13 — INTEGRATION §4, research issue #260); (2) whether the skeleton as built — shared suite + on-demand-only TiKV run, rigorous conflict semantics deferred to M4.2 — proves ADR-0008's applicability well enough to found M4.2–M4.7 on it.
- [x] `Cargo.toml:86` adds `tikv-client = "0.4"` and the submitted lockfile includes its native TLS/OpenSSL transitive tree; this needs the ADR-0003 dependency/license/advisory adjudication called out in the brief, and the feature-on compile path in my temp-applied check failed before Rust code on `openssl-sys` because `pkg-config`/OpenSSL discovery was unavailable.
- [x] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo test --workspace --exclude wyrd-dst` failed with exit status: 101

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: merged-wider
- Iteration delta (if iterating):
- By / date: Eduard Ralph / 2026-07-03

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
- issue_252: worktree isolation silently fell back to in-place — Do + C4 gates ran in the human's primary checkout (`../wyrd`), violating the never-mutate guarantee; make the fallback loud (or fatal) and record which tree each gate ran in.
- issue_252: C4 gate retains no test stdout/log — the exit-101 red could not be attributed post-hoc (no failing test name); persist the gate's test output into the bundle.
- issue_252: lazy worktree cleanup (reset only at next Do) leaves merged bundles' edits in shared lanes — `wyrd-verify-l1` currently holds an orphaned `dserver_image.rs` test that fails at exit 101, a trap for the next verify run; sweep worktrees after Check or on publish.
- issue_252: root cause of the in-place fallback — `publish._clean_ref` prefers a backticked span over the plain first token, so a brief base of "main (feature branch `feat/…`)" resolves base=the feature branch; nonexistent ref → `ensure()` reset fails → silent in-place fallback (and the short-circuit skips `git clean`, leaving prior strays). Fix the parser to take the token before the parenthetical, and make the fallback fail-closed.
- issue_252: host build deps `pkg-config` + `libssl-dev` are required by the `tikv` feature-on path (`tikv-client 0.4.0 → prometheus 0.13.4 → reqwest → native-tls → openssl-sys`) — absent on this host, so `cargo xtask tikv-conformance` fails before compiling any Rust. Add both to `pdca doctor` (`[[doctor.checks]]`, alongside the docker check) and to the CI workflow that will run the TiKV leg; also note the runner retries a deterministic build failure 5× as "TiKV may still be bootstrapping" (misattribution).
