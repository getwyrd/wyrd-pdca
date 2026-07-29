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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 69 mutants tested in 28s: 25 missed, 17 caught, 27 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 8 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Task under review: add a bounded, cursor-keyed `MetadataStore::scan_page` that escapes `SCAN_CAP` with identical ordering, cursor, termination, and stable-key semantics on every backend.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance decision is unambiguous: the seam states the bounded-page, exclusive-cursor, cap-escape, stable-key, and zero-bound obligations together at `crates/traits/src/lib.rs:922` and `crates/traits/src/lib.rs:952`. |
| C2 Reproduction (red pre-fix) | PASS | The base checkout failed both added targets at compile time because the seam was absent, while the patched semantic oracle ran 16/16 and demonstrably rejects an inclusive cursor at `crates/metadata-conformance/tests/scan_page_demonstrated_red.rs:329`; the red evidence is therefore non-vacuous despite zero base tests executing. |
| C3 Change | FAIL | The living architecture must be corrected before acceptance: it calls the store read surface “two operations” at `docs/design/architecture/05-building-block-view.md:204`, but the public seam exposes `get`, `scan`, and `scan_page` at `crates/traits/src/lib.rs:914`, `crates/traits/src/lib.rs:920`, and `crates/traits/src/lib.rs:994`, so readers receive a contradictory API inventory. |
| C4 Verification (red→green) | PASS | Independent scratch runs reproduced the base compile-red and patched green (16 conformance plus 9 redb tests), and `cargo xtask ci`, DST, feature clippy, and live FoundationDB conformance all passed; the shared backend runner actually invokes every page clause at `crates/metadata-conformance/src/lib.rs:1121`. |
| C5 Causal adequacy | PASS | No capability probe or runtime guard papers over the failure: the trait deliberately requires a native method at `crates/traits/src/lib.rs:980`, and redb reads its ordered range directly at `crates/metadata-redb/src/lib.rs:162`; the reproduced 25 surviving mutants are fixture or feature-gated coverage gaps, not evidence of a symptom guard. |
| T1 Structure | PASS | The seam decision remains narrow and dependency-safe: production backends implement the required trait method, while only the dev-only test seam may page over `scan` as documented at `crates/traits/src/lib.rs:989`. |
| T2 Shape | PASS | Callers receive one backend-independent error and bound shape—zero effective limits are refused and positive limits are clamped at `crates/traits/src/lib.rs:961`—so no backend-specific classification leaks through the API. |
| T3 Runtime | NEEDS-HUMAN | A maintainer must supply a TiKV topology compatible with the client’s API mode and run `cargo xtask tikv-conformance` until both runners at `crates/metadata-tikv/tests/conformance.rs:53` and `crates/metadata-tikv/tests/conformance.rs:69` pass—the available cluster failed every attempt with `InvalidKeyMode { storage_api_version: V2 }` during the first commit, before exercising `scan_page`, so live TiKV parity remains unverified. |
| T4 Contribution | NEEDS-HUMAN | A maintainer must inspect or rerun the reported eight batched-review findings before relying on that gate—`scripts/review-branch --bundle` and its finding artifact were absent from the allowed inputs and target, so its red summary is provisional rather than a confirmed patch defect. |
| T5 Judgment | PASS | The affected-path search across merged history plus changed-file inspection of every closed/rejected GitHub PR found no prior `scan_page` implementation, so the required seam at `crates/traits/src/lib.rs:994` has no unresolved duplicate prior art. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | The maintainer must decide whether this contract is operationally fit for the later retirement and GC consumers—the automated evidence proves enumeration beyond `SCAN_CAP`, but not production workload adequacy for the namespaces identified at `crates/traits/src/lib.rs:926`. |

### Advisory — adversary

# Adversarial review — issue 634 / scan-page-seam (advisory, non-gating)

Re-ran the asserted evidence in a scratch clone of `$PDCA_TARGET` (removed afterwards):
`wyrd-traits --lib` (18/18), `metadata-redb --test scan_page` (9/9),
`metadata-conformance --test scan_page_demonstrated_red` (16/16, incl. 7
`#[should_panic(expected = …)]`), `wyrd-testkit --lib` (34/34), and the gating
in-simulator row `RUSTFLAGS=--cfg madsim cargo test -p wyrd-dst --test conformance`
(3/3). Also compile-checked both backends the default gate never builds:
`cargo clippy -p wyrd-metadata-{tikv,fdb} --features {tikv,fdb} --tests` — both clean.

## Findings

- **NEEDS-HUMAN [impl] — `crates/metadata-tikv/src/lib.rs:1107`** — the new `scan_page`
  builds `(cursor..upper)` from a **caller-supplied** cursor, and **panics** whenever that
  cursor sorts at or above the prefix's upper bound. Concrete case:
  `store.scan_page(b"orphan:", Some(b"retire:0001"), 64)` (minimal: `scan_page(b"p:", Some(b"q"), 8)`).
  `page_lower_bound` returns `Some(after)` because `after >= prefix`
  (`crates/traits/src/lib.rs:444`), so `cursor = next_page_start(physical(after))`
  (`:435`, appends `0x00`) while `upper = prefix_upper_bound(physical(prefix))` (`:80`,
  increments the last byte) — giving `cursor > upper`, an **inverted** `BoundRange`.
  `txn.scan` → `Transaction::scan_inner` → `Buffer::scan_and_fetch` →
  `self.entry_map.range(range)` (tikv-client 0.4.0 `src/transaction/buffer.rs:129`, a
  `BTreeMap<Key, BufferEntry>`) → `panicked at …btree/search.rs: range start is greater
  than range end in BTreeMap`. I reproduced that panic standalone against tikv-client
  0.4.0 with the exact key shapes above. redb answers the identical call `(vec![], None)`
  — verified in-process; it iterates and breaks on the first non-prefix key
  (`crates/metadata-redb/src/lib.rs:182,186`), as do both sim models
  (`crates/dst/tests/support/mod.rs:270`) and `test_double_scan_page`. So this is not a
  hypothetical "don't do that": one backend aborts the task where the other three return
  the terminal empty page the contract wants. The patch has already conceded that a
  foreign cursor is in-contract input — `page_lower_bound` exists precisely for "the
  drain's persisted cursor after a namespace rename, … a shared cursor column across
  namespaces" — but it floors the cursor at the prefix without ceiling it at the prefix's
  upper bound, so it covers exactly half of the input space it was introduced for. Fix
  shape: when the resolved cursor is `>= prefix_upper_bound(prefix)`, short-circuit to
  `(vec![], None)` (or clamp the range), decided once in the seam alongside the floor.

- **NEEDS-HUMAN [impl] — `crates/traits/src/lib.rs:1438`** — the seam's own unit test
  *blesses* the input above as safe: `assert_eq!(page_lower_bound(b"p:", Some(b"q")), Some(&b"q"[..]))`
  under the comment "the page is then exhausted, which is a terminal answer rather than a
  wrong one". That is a claim about **every backend's** behaviour asserted by a test that
  only exercises a two-line pure function — and it is false for `metadata-tikv` (above).
  The conformance suite cannot catch the divergence either: clause (b) drives four
  *below*-the-prefix cursors (`crates/metadata-conformance/src/lib.rs:557`) and one
  past-the-last-key cursor that is still **under** the prefix (`:545`, `Some(b"p:99")`),
  but no clause ever passes an `after` at or above `prefix_upper_bound(prefix)`. So
  neither `cargo xtask ci` nor the maintainer-run `xtask tikv-conformance` /
  `fdb-conformance` legs can observe it — the brief's "normative and identical on every
  backend" holds only on the inputs the suite happens to drive. The mirror case belongs
  in `contract_scan_page_cursor_is_exclusive` beside case (iv), and it would have gone
  red on TiKV before this shipped.

## Attempted and could **not** refute

- **The C4-verify PASS is compile-shaped, but the substantive red is real.**
  `check-gates.json:46` claims "red without the fix, green with it"; on `origin/main`
  the added files cannot compile at all, and `brief.md:124-131` pre-declares that
  `run-verify.sh` scores that as red over a run that executed zero tests. I therefore
  went at leg D directly and could not break it: all seven violating doubles carry
  `#[should_panic(expected = "…")]` with clause-specific messages (not bare
  `should_panic`), each has a sibling test proving the *same* double still passes the
  four pre-existing sequential clauses, `FaithfulPagedStore` proves the clauses are not
  red-by-construction, and `ScanBackedStore` proves the rejected `scan()`-shim shape
  actually fails leg B. That is genuine discriminating power, not a tautology.
- **The cap-0 regression from iteration 1 is genuinely fixed, not papered over.**
  I drove `RedbMetadataStore::with_scan_cap(0)` with 25 seeded keys at limits
  `1 / 5 / usize::MAX`: all three refuse with a typed `ZeroPageLimit` rather than the
  previous unbounded 25-item page. The `>= limit` guard at
  `crates/metadata-redb/src/lib.rs:193` and the refusal at `crates/traits/src/lib.rs:416`
  are both killed by mutants (`mutants.out/caught.txt`).
- **Other edge inputs I expected to break redb, and did not:** cursor above the prefix
  range; empty prefix `b""`; a stored key exactly equal to the prefix (walk still returns
  it once and terminates); an all-`0xff` prefix where `prefix_upper_bound` is `None`;
  a below-prefix cursor over an *empty* prefix range; and the cap-clamp and cursor-floor
  interacting (`with_scan_cap(3)` + `after = Some(b"")` + `limit = usize::MAX`).
- **The 25 surviving C5 mutants are not hiding a defect.** I read `mutants.out/missed.txt`:
  they are (a) fdb/tikv bodies the default gate never runs — expected, and the brief's
  declared off-Check posture — and (b) arithmetic in the conformance *fixtures*
  (`:592`, `:593`, `:854`). I checked each of (b): mutating `LIMIT * 3` → `LIMIT + 3` or
  `cap * 3 + 1` → `cap * 3 - 1` still satisfies the fixtures' own asserted invariants
  (exact-multiple-ness, `count > cap`), so the clause still tests what it claims.
- **The ~34 delegating doubles are uniform and none is a decorator.** All delegate to
  `wyrd_testkit::test_double_scan_page`; no `impl MetadataStore` in the workspace wraps
  another store, so no fault-injection seam is bypassed by the delegation. `wyrd-testkit`
  is a `[dev-dependencies]` entry in all three backend crates, so the "a production
  backend naming this helper does not compile" claim holds; the one non-dev consumer
  (`crates/metadata-fault-conformance`) is itself dev-only downstream and declares no
  `MetadataStore` impl.
- **`run_all_cap_scoped` is a second runner rather than a `run_all` extension** (a
  deviation from `brief.md:36-43`'s "no per-driver list to edit"), but every metadata
  driver was in fact wired: redb, both DST sim stores, fdb and tikv. The brief itself
  asks for "a shared conformance clause parameterised over a cap-lowering hook", which
  this is. Not a refutation.

*Context, not a finding: `check-gates.json` is `overall: fail` with the gating T4 row at
**8** blocking findings, up from 6 in iteration 1.*

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T3 Runtime — A maintainer must supply a TiKV topology compatible with the client’s API mode and run `cargo xtask tikv-conformance` until both runners at `crates/metadata-tikv/tests/conformance.rs:53` and `crates/metadata-tikv/tests/conformance.rs:69` pass—the available cluster failed every attempt with `InvalidKeyMode { storage_api_version: V2 }` during the first commit, before exercising `scan_page`, so live TiKV parity remains unverified.
- [ ] T4 Contribution — A maintainer must inspect or rerun the reported eight batched-review findings before relying on that gate—`scripts/review-branch --bundle` and its finding artifact were absent from the allowed inputs and target, so its red summary is provisional rather than a confirmed patch defect.
- [ ] Validation — fitness-to-purpose — The maintainer must decide whether this contract is operationally fit for the later retirement and GC consumers—the automated evidence proves enumeration beyond `SCAN_CAP`, but not production workload adequacy for the namespaces identified at `crates/traits/src/lib.rs:926`.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 8 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_

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
- Iteration delta (if iterating): Auto-iterate (round 1): Check found implementation-level items only, no architectural judgment required — T3 Runtime — A maintainer must supply a TiKV topology compatible with the client’s API mode and run `cargo xtask tikv-conformance` until both runners at `crates/metadata-tikv/tests/conformance.rs:53` and `crates/metadata-tikv/tests/conformance.rs:69` pass—the available cluster failed every attempt with `InvalidKeyMode { storage_api_version: V2 }` during the first commit, before exercising `scan_page`, so live TiKV parity remains unverified.; T4 Contribution — A maintainer must inspect or rerun the reported eight batched-review findings before relying on that gate—`scripts/review-branch --bundle` and its finding artifact were absent from the allowed inputs and target, so its red summary is provisional rather than a confirmed patch defect.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 8 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- By / date: auto-iterate / 2026-07-26

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
