# Result — issue 634 / scan-page-seam

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: `MetadataStore` gains a **bounded, cursor-keyed range scan** whose semantics are
- Success criterion: two **NEW** test files (see `Test file`), both compiled and run by the
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope: the `scan_page` trait method and its normative doc contract

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it.
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 89 mutants tested in 33s: 44 missed, 12 caught, 33 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 6 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Reviewing issue #634’s bounded, cursor-keyed `MetadataStore::scan_page` contract and native backend implementations for enumerating prefixes beyond `SCAN_CAP`.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance boundary is explicit: byte order, exclusive cursor, termination, stable-key no-skip, page clamping, and typed zero-limit rejection are all normative at `crates/traits/src/lib.rs:897`. |
| C2 Reproduction (red pre-fix) | PASS | A clean-base run with only the two added test targets failed to compile with 20 missing-seam errors and ran zero tests, while the patched violating-store target executed 10 tests whose four `#[should_panic]` cases demonstrate semantic RED at `crates/metadata-conformance/tests/scan_page_demonstrated_red.rs:265`. |
| C3 Change | FAIL | The change admits `with_scan_cap(0)` at `crates/metadata-redb/src/lib.rs:85`, and `page_limit` turns any nonzero request into an effective zero at `crates/traits/src/lib.rs:388`; an isolated patched-tree regression returned one item for a required maximum of zero. |
| C4 Verification (red→green) | FAIL | The declared added targets, shared redb suite, DST suite, feature clippy rows, and real FDB suite pass, but the tests never cover an effective zero cap and the reproduced `items.len() <= min(limit, cap)` failure contradicts the always-bound criterion at `crates/metadata-conformance/src/lib.rs:683`. |
| C5 Causal adequacy | FAIL | The mutation rerun left 44 of 89 mutants surviving, including whole-body empty-success replacements for FDB and TiKV `scan_page` at `crates/metadata-fdb/src/lib.rs:1876` and `crates/metadata-tikv/src/lib.rs:1078`, so the available default evidence cannot establish both production bodies as load-bearing. |
| T1 Structure | PASS | The seam is a required trait method with no `scan()` default at `crates/traits/src/lib.rs:942`, production backends use native range primitives, and the shared runner wires every ordinary clause at `crates/metadata-conformance/src/lib.rs:967`. |
| T2 Shape | FAIL | The public cap-lowering shape permits a zero effective page size even though the API promises non-progress is impossible; the shared resolver returns `0` rather than rejecting or flooring it at `crates/traits/src/lib.rs:388`. |
| T3 Runtime | FAIL | With a populated redb prefix and `with_scan_cap(0)`, `scan_page(..., 1)` returns one item because the loop checks `items.len() == limit` only after insertion at `crates/metadata-redb/src/lib.rs:177`, breaching the page bound in the real backend. |
| T4 Contribution | NEEDS-HUMAN | Decide whether the six recorded blocking batch-review findings were discharged — the `review-branch --bundle` harness is absent here and cannot be replayed; affected-path history found no competing implementation and only an unrelated overlap in rejected PR #336. |
| T5 Judgment | NEEDS-HUMAN | Decide whether sign-off requires clean reruns on a compatible host — `cargo xtask tikv-conformance` stopped in the pre-existing first commit clause with TiKV API-V2 `InvalidKeyMode`, and `cargo deny check` stopped on a read-only advisory-DB lock, so neither result verifies or indicts `scan_page`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the seam is fit for the future `retire:` and `orphan:` consumers before those callers and the 1.78 M-key scale case exist — lowered-cap redb, simulator, and real-FDB evidence passed, but production TiKV behavior remains unexercised; rerun `cargo xtask tikv-conformance` with a transactional-compatible TiKV topology and require the shared suite to pass. |

### Advisory — adversary

# Adversarial review — issue 634 / scan-page-seam

Attacked the evidence (re-ran leg D and probed the redb production path in a scratch
crate), the fix (boundary/edge inputs on all five implementations), and the verdict.
Three refutations landed; one architectural question is left for the human.

## Findings

- **NEEDS-HUMAN [impl]** — `crates/metadata-redb/src/lib.rs:178` returns an **unbounded
  page** whenever the store's effective cap is `0`, falsifying the patch's own normative
  claim. Confirmed empirically against the real backend (not a double):
  `RedbMetadataStore::in_memory().with_scan_cap(0)` seeded with 25 keys under `p:`, then
  `scan_page(b"p:", None, 5)` → **25 items**, `next: None`. `page_limit`
  (`crates/traits/src/lib.rs:388-394`) rejects only `limit == 0`, never a zero *cap*, so it
  returns `Ok(0)`; the break at `:178` is `items.len() == limit`, which can never fire once
  the first key is pushed, and the loop drains the whole prefix. This breaks the claim
  stated at `crates/traits/src/lib.rs:386` and `:935` ("`items.len() <= min(limit, effective
  cap)` … holds by construction") — and it breaks it in the specific direction the cap
  exists to stop (unbounded materialization). `with_scan_cap` applies no floor
  (`crates/metadata-redb/src/lib.rs:85-88`). redb is the **only** implementation that
  diverges: the sim stores, `FaithfulPagedStore` and `test_double_scan_page` use
  `take(limit)`/`truncate(limit)` and degrade to a safe empty page. The conformance clause
  cannot catch it because it excludes the input by construction (`assert!(cap >= 2)`,
  `crates/metadata-conformance/src/lib.rs:765`). Reachable today only through the public
  test knob, so it is not a live production data-loss path — but it is a public API on a
  production type and a one-character fix (`>=` at `:178`, or floor the cap in `page_limit`).

- **NEEDS-HUMAN [impl]** — the "never start the page before the prefix" guard at
  `crates/metadata-redb/src/lib.rs:166` is **untested in every backend**, and the clause
  that claims to cover it does not. `cargo mutants` records `replace match guard cursor >=
  prefix with true` as **missed** while `→ false` is caught; I read the mutant log and
  confirmed the full redb suite ran under it (`trait_contract`,
  `trait_contract_with_a_lowered_scan_cap`, and all 7 tests of the new `scan_page.rs`) and
  passed. The reason: `crates/metadata-conformance/src/lib.rs:518-524` labels
  `Some(b"p:0")` "a cursor below the whole range", but `b"p:0" > b"p:"` — the cursor is
  *inside* the prefix and exercises the same `Bound::Excluded` arm as every other case. No
  test anywhere passes an `after` lexicographically **below** the prefix, so four
  hand-written fallbacks (`metadata-redb:166`, `metadata-fdb:1891`, `metadata-tikv:1091`
  `.max(start)`, and `page_from_truth` in `crates/dst/tests/support/mod.rs`) are unverified.
  An implementation that dropped the fallback would answer `after = b"a"`, `prefix = b"p:"`
  with an **empty terminal page** — a false "prefix exhausted", i.e. exactly the silent skip
  clause (c) exists to prevent — and stay green on the whole gate. (I verified the shipped
  code is correct here: the probe returned the right 3 keys, and no neighbouring-prefix
  leak.) One added assertion in clause (b) closes it.

- **NEEDS-HUMAN [impl]** — `crates/traits/src/lib.rs:413-441`: the shared body behind
  **all 34 test doubles** this patch touched is never executed by any test. `.scan_page(`
  has **zero** invocation sites in the workspace outside the conformance clause bodies and
  the two new test files, so every delegating one-liner is dead at runtime, and the helper's
  documented contract at `:409-412` ("byte-lexicographic order, exclusive cursor, `next` =
  the last key returned on a full page and `None` on a short one") is asserted and never
  checked. The surviving mutants at `:429` (`>` → `>=`, `>` → `<`) and `:435` (`==` → `!=`)
  are consistent with this — though for `wyrd-traits` mutants cargo-mutants ran only that
  crate's own 9 unit tests, so the absence of call sites, not the mutant score, is the
  evidence. The failure this hides is precisely `InclusiveCursorStore` from leg D: flip `>`
  to `>=` and all 34 doubles silently acquire an inclusive cursor that would spin #636/#637's
  drain loops forever, with the gate green. I ran all five clause functions against a
  HashMap-backed double delegating to `test_double_scan_page`: **all pass**, so the helper is
  correct today and the fix is ~15 lines beside `FaithfulPagedStore` in
  `scan_page_demonstrated_red.rs`, green on the first run.

- **NEEDS-HUMAN** — `crates/traits/src/lib.rs:413` ships `test_double_scan_page` as an
  unconditional `pub` item of the **production** seam crate, which reduces the slice's
  central guarantee to a convention. The brief's required-method decision exists so "no
  production backend can inherit a `scan()`-based body" — the shape #508's 4th attempt was
  rejected for — but a future backend can now re-introduce that exact shape in one visible
  line, and the brief itself explains why the conformance suite can never detect it (the cap
  knob is a per-backend inherent method, unreachable through the trait). The brief sanctioned
  this location, so this is a fitness-to-purpose call, not a defect: the human should decide
  whether the helper stays as-is, moves behind a feature/dev-only crate, or is pinned by the
  mechanism the repo already uses for this purpose (`clippy.toml` `disallowed-methods`, today
  carrying only `SystemTime::now`).

## Attempted and could not refute

- **Leg D's non-vacuity proof is real.** I re-ran
  `crates/metadata-conformance/tests/scan_page_demonstrated_red.rs`: 10/10 pass, with all
  four `#[should_panic]` tests firing on the *matching* clause and each violating double
  still passing the four pre-existing sequential clauses. I traced each panic message to the
  assertion that raises it — no `should_panic` passes for the wrong reason, and
  `a_faithful_paged_store_passes_every_new_clause` rules out red-by-construction. This is
  genuine semantic red evidence, not a compile-shaped one.
- **Not a parallel re-implementation.** Leg B drives `RedbMetadataStore` itself, and the
  clauses reach redb + both DST sim stores through `run_all`/`run_all_cap_lowered` with no
  per-driver clause list; all four drivers wire the cap-lowered runner.
- **No `scan()`-shim smuggled in.** All five implementations read their own ordered range
  primitive; the cap-escape clause would fail against any of them if they did not.
- Probed and found correct: exact-multiple-of-`limit` termination, the empty-page-never-
  carries-a-cursor rule, `next` cursor monotonicity, cursor-past-the-end, cross-prefix leak
  on an out-of-range cursor, and TiKV's rollback discipline at
  `crates/metadata-tikv/src/lib.rs:1113` (identical to the pre-existing `scan` at `:1052`, so
  the rubric's *Transactions* class is honoured).
- **Docs currency does not fire**: the brief's conditional ("if a paragraph states the store
  offers only a whole-namespace scan") has no match in `docs/design/architecture/`.
- The C4-verify PASS is structurally degenerate here (on the base the new files do not
  compile, and a build error is scored as red) — but the brief pre-declared exactly that and
  nominated leg D as the binding demonstration, which I verified holds. Not a refutation.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T4 Contribution — Decide whether the six recorded blocking batch-review findings were discharged — the `review-branch --bundle` harness is absent here and cannot be replayed; affected-path history found no competing implementation and only an unrelated overlap in rejected PR #336.
- [ ] T5 Judgment — Decide whether sign-off requires clean reruns on a compatible host — `cargo xtask tikv-conformance` stopped in the pre-existing first commit clause with TiKV API-V2 `InvalidKeyMode`, and `cargo deny check` stopped on a read-only advisory-DB lock, so neither result verifies or indicts `scan_page`.
- [ ] Validation — fitness-to-purpose — Decide whether the seam is fit for the future `retire:` and `orphan:` consumers before those callers and the 1.78 M-key scale case exist — lowered-cap redb, simulator, and real-FDB evidence passed, but production TiKV behavior remains unexercised; rerun `cargo xtask tikv-conformance` with a transactional-compatible TiKV topology and require the shared suite to pass.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 6 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Rejected on the confirmed implementation defects, not just the review verdicts: - `crates/metadata-redb/src/lib.rs:178` (with `page_limit` at `crates/traits/src/lib.rs:388`): when a store's effective scan cap is 0, `scan_page` returns an UNBOUNDED page (empirically verified: 25 keys seeded, cap 0, limit 5 -> 25 items returned, next: None), directly violating the brief's own "items.len() <= min(limit, effective cap) always" contract. Fix the guard (`>=` not `==`, or floor the effective cap so 0 is rejected like `limit == 0` is) and add a conformance/unit case for a zero effective cap. - The T4 batched multi-pass review failed gating with 6 blocking findings; discharge or rebut each explicitly in the next attempt's build-notes.md. - Also correct, if practical in the same pass: the untested "cursor below prefix" fallback guard (`crates/metadata-redb/src/lib.rs:166` and siblings in fdb/tikv/dst) — add a clause with an `after` lexicographically below the prefix so clause (b) actually exercises it; and reduce the dead-code / visibility risk of `test_double_scan_page` being an unconditional `pub` item of the production `wyrd-traits` crate. - The TiKV real-cluster and `cargo deny` runs remain environmentally blocked here (off-Check per the brief's verification posture) and are not the reason for this rejection — re-attempt only needs to address the items above; the maintainer-run TiKV conformance and fitness-to-purpose question can still be settled at the next sign-off.
- By / date: Eduard Ralph / 2026-07-26

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
