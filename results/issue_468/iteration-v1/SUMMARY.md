# Result — issue 468 / metadata-fdb-dst-story

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: Hold `metadata-fdb` — an FFI backend that can never run inside the
- Success criterion: `cargo xtask dst` is green with the new legs, and
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope: Give the FFI backend a DST story of the same strength as the TiKV backend's.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it.
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

Reviewing issue 468: add a deterministic simulated-FDB DST story for `metadata-fdb`, including 1021 commit-ambiguity coverage and a guard that keeps `libfdb_c` out of the simulator.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The owed behavior is explicit: build a simulated-FDB `MetadataStore`, exercise 1021 ambiguity, and keep FDB linkage out of DST, so acceptance can be judged against concrete legs (`brief.md:9`, `brief.md:16`, `brief.md:83`). |
| C2 Reproduction (red pre-fix) | PASS | Reversing the tracked model/conformance changes while keeping the added tests made the per-fix guard fail at `the_dst_support_module_declares_the_simulated_fdb_store`, proving the seam absence is observable (`crates/dst/tests/no_fdb_linkage.rs:181`). |
| C3 Change | PASS | The patch stays on the requested DST/test surface: model in support, shared conformance leg, ambiguity property, and linkage guard, with no production FDB edits (`crates/dst/tests/support/mod.rs:395`, `crates/dst/tests/conformance.rs:58`, `crates/dst/tests/commit_ambiguity.rs:222`, `crates/dst/tests/no_fdb_linkage.rs:165`). |
| C4 Verification (red→green) | PASS | I reran `cargo xtask ci` green and reproduced red→green directly; the named `engine/scripts/run-verify.sh` was not present in this target, but its structural red was independently observed and restored green (`xtask/src/main.rs:1324`, `crates/dst/tests/no_fdb_linkage.rs:190`). |
| C5 Causal adequacy | PASS | The change models the missing 1021 cause and requires settling re-read rather than adding a capability probe/runtime guard around an expected capability (`crates/dst/tests/support/mod.rs:490`, `crates/dst/tests/commit_ambiguity.rs:139`). |
| T1 Structure | PASS | The FDB store is a second parametrization beside the existing simulator support, and the shared contract gains a third implementation instead of a forked suite (`crates/dst/tests/support/mod.rs:384`, `crates/dst/tests/conformance.rs:47`). |
| T2 Shape | PASS | The behavioral ambiguity test is correctly madsim-gated while the linkage guard remains runnable under bare `cargo test -p wyrd-dst --test no_fdb_linkage` (`crates/dst/tests/commit_ambiguity.rs:34`, `crates/dst/tests/no_fdb_linkage.rs:33`). |
| T3 Runtime | PASS | The CI path really includes `run_dst()` with `--cfg madsim` and 50 seeds, and my rerun showed the new DST tests passing under that path (`xtask/src/main.rs:1331`, `xtask/src/main.rs:1342`, `crates/dst/tests/commit_ambiguity.rs:223`). |
| T4 Contribution | NEEDS-HUMAN | Local merged-history checks show only the prior DST skeleton and no FDB hits under `origin/main`, but no local PR refs were available to mechanically settle open/closed/rejected PR prior art, so the human must clear PR metadata coverage (`brief.md:151`). |
| T5 Judgment | NEEDS-HUMAN | The human must ratify the fidelity claim that optimistic conflict plus seeded 1021 over a plain map is the right DST abstraction, because that choice determines whether the proof covers the production risk (`crates/dst/tests/support/mod.rs:51`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Final sign-off must decide whether the simulator-plus-manifest-scan evidence is sufficient for the `metadata-fdb` DST story, since a live FDB topology cannot deterministically emit 1021 by design (`crates/metadata-fdb/src/lib.rs:67`, `crates/metadata-fdb/src/lib.rs:71`). |

### Advisory — adversary

# Adversarial review — issue 468 / metadata-fdb-dst-story

Reproduced the evidence first. `cargo test -p wyrd-dst --test no_fdb_linkage` → 5 passed.
`RUSTFLAGS="--cfg madsim" MADSIM_TEST_NUM=5 cargo test -p wyrd-dst --test commit_ambiguity`
→ 3 passed, and `assuming_an_ambiguous_commit_did_not_land_fails_the_sweep` genuinely panics
with the pinned message at `crates/dst/tests/commit_ambiguity.rs:171`. The behavioural
red→green is real. What follows attacks it anyway.

## Refutations

- **NEEDS-HUMAN — `crates/dst/tests/no_fdb_linkage.rs:58,77`: the purity guard is blind to the
  renamed-dependency form, which is this manifest's own house style.** `scan_line` takes the key
  as the text before the first `=`/`.`/space. Adding
  `fdb = { package = "foundationdb", version = "0.10", features = ["fdb-7_3"] }` to
  `crates/dst/[dev-dependencies]` links `libfdb_c` into every DST test binary and
  `the_dst_manifest_declares_no_fdb_dependency` (`:165`) stays **green**. I compiled `scan_line`
  verbatim and ran it: that line, `wyrd-fdb = { package = "wyrd-metadata-fdb" }`, and
  `[dependencies."foundationdb"]` all return `None`. This is not an exotic evasion —
  `crates/dst/Cargo.toml:56,66,68` already declares `tonic`, `etcd-client` and `tokio` in exactly
  that `package = ` rename form. The doc claim at `no_fdb_linkage.rs:10` ("there is exactly one
  way `libfdb_c` could enter this graph") and brief goal (c) ("mechanically guaranteeing that no
  `libfdb_c` symbol is ever reachable") are therefore unwarranted as stated.

- **`crates/dst/tests/no_fdb_linkage.rs:148`: the planted red is over-fitted to the scanner.**
  The fixture plants exactly the two shapes `scan_line` was written to recognise
  (`wyrd-metadata-fdb.workspace = true`, `foundationdb = { version = "0.9", … }`). It plants
  neither the rename form above nor a transitive edge, so it proves the scanner catches what the
  scanner catches. (Also: the fixture pins `version = "0.9"`/`fdb-7_1`; the workspace actually
  pins `foundationdb 0.10`/`fdb-7_3` at `Cargo.toml:108` — the fixture was not derived from the
  real dependency.) Separately the needle set is *over*-broad in the other direction:
  `crates/metadata-fdb/Cargo.toml:11-22` makes `foundationdb` an optional dep behind a
  default-off `fdb` feature, so a bare `wyrd-metadata-fdb` dev-dep would trip the guard without
  linking `libfdb_c` at all. The guard scans one manifest's text; the linkage condition is a
  feature-unified graph property.

- **NEEDS-HUMAN — `crates/dst/tests/support/mod.rs:494` (`&& conditional`) contradicts the
  production contract it cites.** `classify_commit_error`
  (`crates/metadata-fdb/src/lib.rs:213-215`, doc at `:194`) returns `UnknownResult` for 1021/1031
  **"for *every* batch, conditional or not."** The model gates the nemesis on `conditional`, so a
  blind batch can never be ambiguous in the simulator. The justification at `support/mod.rs:435-438`
  cites `metadata-fdb/src/lib.rs:53-56`, but that passage says 1021 is never *re-applied* by the
  blind retry gate — it does not say a blind batch is never ambiguous. Concrete consequence: make
  the model faithful (drop `&& conditional`) and `commit_ambiguity.rs:116`
  (`write::intent(…).await.unwrap()` → blind `put_pending`) panics on the first writer. The sweep
  is green *because* the model is narrower than the contract, and the four-phase write protocol's
  behaviour under an ambiguous pending-ledger put/delete is exercised by nothing.

- **NEEDS-HUMAN — `crates/metadata-fdb/src/lib.rs:165` vs `crates/dst/tests/support/mod.rs:398`:
  the model renders only the *strong* half of the ambiguity class.** Production maps 1021 **and**
  1031 to the same `CommitUnknownResult`, carrying `code` because "the two codes are not equally
  bad": *"Where 1021 promises the transaction is out of flight, 1031 promises nothing."*
  `SimCommitUnknownResult` is a unit struct with no code, and the whole settling re-read at
  `commit_ambiguity.rs:131` assumes the ambiguous txn is out of flight — precisely the guarantee
  1031 withholds. So "the 1021 ambiguity space, searched exhaustively" (brief §Falsifiability) is
  really "the out-of-flight half of it." The brief's Open Question 1 asks the human to ratify
  fidelity w.r.t. MVCC-vs-`BTreeMap`; it does **not** surface this narrowing or the blind-batch one
  above. Both belong in that ratification.

- **`crates/dst/tests/commit_ambiguity.rs:150` + `:184`: the torn-inode assertion is a tautology on
  exactly the path this file exists to test.** When the ambiguous commit *landed*, the nemesis
  struck the first accepted CAS, so every other writer conflicts and the sole winner is the `Err`
  writer — selected at `:150` by `settled.chunk_map == *chunk_map`. `:184` then asserts
  `settled.chunk_map == expected` where `expected` is that same `chunk_map`: `x == x`, unfalsifiable.
  It carries content only on the *not*-landed half (an `Ok(Committed)` winner), which
  `concurrency.rs:126` already covers for redb/sim-TiKV. And the model cannot produce a torn inode
  anyway — `apply()` runs inside the `Mutex` guard with no await (`support/mod.rs:470-508`). Brief
  criterion 2(iii) ("no torn/hybrid inode is ever observable") therefore has no demonstrated red,
  unlike 2(i)/2(ii). (`assert!(winners <= 1)` at `:163` is likewise unreachable-by-construction.)

- **NEEDS-HUMAN — `check-gates.json` row `C4-verify` ("run-verify.sh: PASS — red without the fix,
  green with it") is true but carries no information about the fix.** The discriminator is
  `no_fdb_linkage.rs:191`, a `String::contains("pub struct SimFdbMetadataStore")` over
  `support/mod.rs` — and `:185-186` states that this binary deliberately does *not* link the module.
  `run-verify.sh` runs `cargo test -p wyrd-dst` without `--cfg madsim`, so `support/mod.rs` is never
  compiled in either phase. The row would read identically if `SimFdbMetadataStore` were an empty
  `pub struct SimFdbMetadataStore;` with no `MetadataStore` impl, no nemesis, and no RNG. The brief
  is candid that this is "a *structural* red"; the gates row is not, and a reviewer scanning the
  table can reasonably misread it as per-fix evidence for criteria 1–3. The only real evidence for
  those is `C4-ci`.

## Attempted and could not refute

- **The both-halves sweep is not fragile.** I feared `AMBIGUITY_SWEEP_SEEDS = 64`
  (`commit_ambiguity.rs:55`) might straddle a lopsided parity. Replayed
  `ChaCha8Rng::seed_from_u64(seed).next_u64() % 2` for seeds `0..64`: **28 landed / 36 not**, first
  landed at seed 0. Both halves are amply covered and `assuming_…` panics on the first iteration.
- **The demonstrated red is not a tautology.** `#[should_panic(expected = "must equal the inode's
  version bump")]` pins the *load-bearing* assertion (`:171`), not any panic; the `AssumeNotCommitted`
  observer differs from the correct one on exactly the `Err` arm, and it passes on the not-landed
  half — so the red is caused by the store's version bump, as claimed.
- **The nemesis cannot be stolen by the fixture or by `intent`.** `metadata::put_pending` /
  `sweep_pending` build precondition-free batches (`crates/core/src/metadata.rs:499,508`) and
  `arm_commit_ambiguity` runs after the fixture (`commit_ambiguity.rs:106`), so exactly the
  four-writer `commit_overwrite` CAS is struck — `obs.ambiguous_commits == 1` holds by
  construction, not by luck.
- **`assert!(conditional)` in the Conflict arm (`support/mod.rs:481`) is unreachable, but not
  wrong**: `preconditions_hold` is vacuously true for an empty list, which is the FDB behaviour it
  claims to model.
- **No `Undeterminable` variant was added, `crates/metadata-conformance/` is untouched, no new Cargo
  dependency was introduced, and nothing under `crates/*/src/` changed** — the scope discipline the
  brief demanded holds.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T4 Contribution — Local merged-history checks show only the prior DST skeleton and no FDB hits under `origin/main`, but no local PR refs were available to mechanically settle open/closed/rejected PR prior art, so the human must clear PR metadata coverage (`brief.md:151`).
- [ ] T5 Judgment — The human must ratify the fidelity claim that optimistic conflict plus seeded 1021 over a plain map is the right DST abstraction, because that choice determines whether the proof covers the production risk (`crates/dst/tests/support/mod.rs:51`).
- [ ] Validation — fitness-to-purpose — Final sign-off must decide whether the simulator-plus-manifest-scan evidence is sufficient for the `metadata-fdb` DST story, since a live FDB topology cannot deterministically emit 1021 by design (`crates/metadata-fdb/src/lib.rs:67`, `crates/metadata-fdb/src/lib.rs:71`).

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
- Iteration delta (if iterating): The goals (a)(b)(c) and the brief stand; the DST machinery is real and worth keeping. What fails is fidelity: the simulated model provably diverges from the production contract it cites, in two ways the brief never surfaced, and the purity guard does not guarantee what goal (c) claims. Rebuild against the same brief — do NOT re-plan. Sign-off could not ratify §6 T5 ("the human must ratify the fidelity claim") because the claim as built is narrower than advertised. Fix the model, then re-present the ratification with the narrowings disclosed. WHAT IS REAL — keep it, do not churn - The behavioural red->green is genuine: `commit_ambiguity` panics with the pinned message under a wrong observer, and `#[should_panic(expected = "must equal the inode's version bump")]` pins the load-bearing assertion, not any panic. - The both-halves sweep is not fragile: replaying ChaCha8Rng for seeds 0..64 gives 28 landed / 36 not; first landed at seed 0. - The nemesis strikes the intended four-writer `commit_overwrite` CAS by construction (`put_pending`/`sweep_pending` build precondition-free batches; `arm_commit_ambiguity` runs after the fixture), so `obs.ambiguous_commits == 1` holds by construction. - `assert!(conditional)` in the Conflict arm (support/mod.rs:481) is unreachable but correct: `preconditions_hold` is vacuously true for an empty list, which is the FDB behaviour it models. - Scope discipline held: no `Undeterminable` variant, `crates/metadata-conformance/` untouched, no new Cargo dependency, nothing under `crates/*/src/` changed. WHAT TO FIX (four items) 1. The model contradicts the production contract it cites. Production `classify_commit_error` (crates/metadata-fdb/src/lib.rs:213-215, doc at :194) returns `UnknownResult` for 1021/1031 "for *every* batch, conditional or not" — the code/timeout check returns BEFORE the `conditional` check. The model gates the nemesis on `&& conditional` (crates/dst/tests/support/mod.rs:494), so a blind batch can never be ambiguous in the simulator, and the four-phase write protocol's behaviour under an ambiguous pending-ledger put/delete is exercised by nothing. The justification at support/mod.rs:435-438 cites lib.rs:53-56, but that passage says 1021 is never *re-applied* by the blind retry gate — it does not say a blind batch is never ambiguous. The sweep is green BECAUSE the model is narrower than the contract. FIX: drop `&& conditional` and make the blind-batch ambiguity path pass. Expect commit_ambiguity.rs:116 (`write::intent(..).await.unwrap()` -> blind `put_pending`) to panic on the first writer until the protocol is handled. 2. Only the strong half of the ambiguity class is modelled. Production maps 1021 AND 1031 to `CommitUnknownResult` carrying `code`, because (lib.rs:165) "Where 1021 promises the transaction is out of flight, 1031 promises nothing." `SimCommitUnknownResult` is a unit struct with no code, and the settling re-read at commit_ambiguity.rs:131 assumes the ambiguous txn is out of flight — exactly the guarantee 1031 withholds. "The 1021 ambiguity space, searched exhaustively" is really the out-of-flight half of it. FIX: carry the code on `SimCommitUnknownResult` and model 1031 distinctly from 1021. 3. Goal (c) is not met: the purity guard is blind to the renamed-dependency form, which is this manifest's own house style. `scan_line` (crates/dst/tests/no_fdb_linkage.rs:58,77) keys on the text before the first `=`/`.`/whitespace, so `fdb = { package = "foundationdb", version = "0.10", features = ["fdb-7_3"] }` links libfdb_c into every DST test binary and `the_dst_manifest_declares_no_fdb_dependency` (:165) stays green. So does `[dependencies."foundationdb"]` — the section branch never strips the quotes. This is not exotic: crates/dst/Cargo.toml:56,66,68 already declares tonic, etcd-client and tokio in exactly that rename form, in the very file the guard scans. The doc claim at no_fdb_linkage.rs:10 ("there is exactly one way libfdb_c could enter this graph") and brief goal (c) ("mechanically guaranteeing that no libfdb_c symbol is ever reachable") are unwarranted as stated. Also: the guard is over-broad the other way — `foundationdb` is optional behind a default-off `fdb` feature (crates/metadata-fdb/Cargo.toml:11-22), so a bare `wyrd-metadata-fdb` dev-dep trips it without linking libfdb_c at all. And the planted red (no_fdb_linkage.rs:148) is over-fitted: it plants exactly the two shapes scan_line recognises, neither the rename form nor a transitive edge, and pins `version = "0.9"`/`fdb-7_1` while the workspace pins foundationdb 0.10/fdb-7_3 (Cargo.toml:108) — the fixture was not derived from the real dependency. FIX: linkage is a feature-unified GRAPH property, not a manifest-text property. Resolve it from `cargo metadata` (or `cargo tree -e features`) rather than scanning one manifest's lines. Plant the rename form and a transitive edge in the red. 4. The torn-inode assertion is a tautology on exactly the path the file exists to test. When the ambiguous commit landed, the nemesis struck the first accepted CAS, so the sole winner is the `Err` writer — selected at commit_ambiguity.rs:150 by `settled.chunk_map == *chunk_map`. Line :184 then asserts `settled.chunk_map == expected` where `expected` is that same chunk_map: `x == x`, unfalsifiable. It carries content only on the not-landed half, which concurrency.rs:126 already covers for redb/sim-TiKV. The model cannot produce a torn inode anyway — `apply()` runs inside the Mutex guard with no await (support/mod.rs:470-508). `assert!(winners <= 1)` at :163 is likewise unreachable-by-construction. FIX: give brief criterion 2(iii) ("no torn/hybrid inode is ever observable") a real demonstrated red, or withdraw the claim. Do not leave it asserted by a tautology. DISCLOSE AT THE NEXT SIGN-OFF The brief's Open Question 1 asks the human to ratify fidelity w.r.t. MVCC-vs-BTreeMap. It does not surface the blind-batch narrowing (1) or the 1031 narrowing (2). Both belong in that ratification. Put them in build-notes.md so the next sign-off ratifies what was actually built. RECORDED AS A §10 ACT CANDIDATE — not this bundle's to fix The `C4-verify` gates row reads "red without the fix, green with it" while its discriminator here is a `String::contains("pub struct SimFdbMetadataStore")` over support/mod.rs — a file run-verify.sh never compiles in either phase (it runs without `--cfg madsim`). The row would read identically for an empty struct with no MetadataStore impl, no nemesis and no RNG. The brief is candid that this is "a *structural* red"; the gates table is not. Only C4-ci is real evidence for criteria 1-3. That is a harness-wide scanning hazard, not a defect of this patch.
- By / date: Eduard Ralph / 2026-07-10

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
- Gates-table hazard: the `C4-verify` row reads "red without the fix, green with it" even when the discriminator is merely *structural* (here, a `String::contains` over a file `run-verify.sh` never compiles). A reviewer scanning the table can reasonably misread it as per-fix behavioural evidence. Consider having run-verify.sh distinguish structural from behavioural reds in the row it emits.
