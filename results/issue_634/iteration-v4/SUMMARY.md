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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 84 mutants tested in 34s: 22 missed, 29 caught, 33 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Reviewing issue #634: add a bounded, cursor-keyed `MetadataStore::scan_page` with identical no-skip semantics and native implementations across all metadata backends.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The normative target is unambiguous: bounded pages, raw-byte order, an exclusive cursor, truthful termination, and no skip of stable keys are all stated at `crates/traits/src/lib.rs:980`. |
| C2 Reproduction (red pre-fix) | PASS | The pre-fix added target failed to compile with 69 missing-API/contract errors and ran 0 tests; with the patch, 22/22 semantic demonstrated-red cases passed, including the string-order violation at `crates/metadata-conformance/tests/scan_page_demonstrated_red.rs:453`. |
| C3 Change | PASS | The decision to require native cursor implementations is carried through redb, FDB, TiKV, and both simulator models, while test doubles are isolated behind the dev-only helper; representative production entries are `crates/metadata-redb/src/lib.rs:162`, `crates/metadata-fdb/src/lib.rs:1891`, and `crates/metadata-tikv/src/lib.rs:1084`. |
| C4 Verification (red→green) | PASS | Applied verification was green for the two new targets (22 + 10 tests), every default-gate component passed after isolating an unrelated health-test flake and read-only advisory-cache lock, DST passed, all four feature clippy rows passed, and live FDB conformance passed; the redb suite drives every shared clause at `crates/metadata-redb/tests/scan_page.rs:330`. |
| C5 Causal adequacy | PASS | The change removes the cap-bound cause with a required native range primitive rather than adding a capability probe or runtime guard, and the shared cap-escape clause demonstrably rejects a `scan()` shim at `crates/metadata-conformance/src/lib.rs:1157`; the 22 mutation misses were message-only/equivalent or feature-off distributed bodies, with live FDB independently green. |
| T1 Structure | PASS | The wide diff is the necessary source-breaking trait rollout, but decisions remain centralized in the trait helpers, backend adapters, shared runner, and dev-only double helper instead of being forked per test; the required seam is at `crates/traits/src/lib.rs:1055` and shared registration at `crates/metadata-conformance/src/lib.rs:1423`. |
| T2 Shape | PASS | The API and page-bound shape match the settled design, including typed zero-bound refusal and shared limit/start/cursor resolution, so callers cannot receive an unbounded or non-progressing page; see `crates/traits/src/lib.rs:387` and `crates/traits/src/lib.rs:426`. |
| T3 Runtime | PASS | Redb, all three DST stores, and a real FoundationDB cluster passed the shared and cap-scoped runtime contracts; TiKV again stopped before `scan_page` on the brief’s already-cleared V2 API-mode topology caveat at the driver entry `crates/metadata-tikv/tests/conformance.rs:53`, so it is not a new patch defect or open decision. |
| T4 Contribution | NEEDS-HUMAN | A maintainer must decide whether to rely on the reported zero-blocking batched review — `scripts/review-branch` and its finding report are absent from the allowed target/inputs, so that gate cannot be independently reproduced; affected-path history and closed-PR search otherwise found no prior implementation (only proposal PR #627), and current open PRs are dependency/workflow updates. |
| T5 Judgment | PASS | No source-grounded correctness or rubric defect remains after the independent code, test, mutation, backend, and prior-art checks; acceptability turns only on clearing the inaccessible T4 evidence, not on an identified implementation flaw. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | A maintainer must decide whether this seam is fit for the later `retire:` and `orphan:` consumers — this slice intentionally has no production caller, so primitive-level cap escape cannot establish the eventual maintenance loop’s operational page budget or end-to-end behavior; the architectural boundary is recorded at `docs/design/architecture/05-building-block-view.md:204`. |

### Advisory — adversary

# Adversarial review — issue 634 / scan-page-seam (advisory, never gating)

Scope: `patch.diff`, `brief.md`, `check-gates.json`; every anchor re-read on the target
source at `$PDCA_TARGET` (`/home/eddie/development/wyrd/wyrd.pdca-wt`, working tree =
patch applied on `22d71b4`). Evidence was re-run in a throwaway copy of that tree under
`$PDCA_SCRATCH` (removed before writing this): `cargo test -p wyrd-traits -p wyrd-testkit
-p wyrd-metadata-conformance -p wyrd-metadata-redb` (all green: 20 seam unit tests, 22
demonstrated-red targets, 10 redb `scan_page` targets, both conformance runners),
`cargo test --workspace --exclude wyrd-dst` (green except
`xtask/tests/repo_hygiene_guards.rs:137`, which fails only because my copy has no `.git`
— an artefact of my sandbox, not the patch), and the four feature-gated clippy rows the
brief demands (`-p wyrd-metadata-tikv --features tikv --tests`, `-p wyrd-metadata-fdb
--features fdb --tests`, `-p wyrd-server --features fdb,etcd --tests`) — all clean.

## Findings

- **NEEDS-HUMAN [impl] — the new clause set passes a store whose paged walk silently
  skips a key, which is the one invariant this slice exists to restore.**
  `crates/metadata-conformance/src/lib.rs:524` (clause (a)) and `:614` (clause (b)) never
  land a page boundary on a key that is a **strict prefix of its successor**, so a backend
  that resumes with "the cursor with its last byte incremented" instead of the immediate
  successor is accepted. Verified, not hypothesised: a `BadSuccessorStore` differing from
  the patch's own `FaithfulPagedStore` in exactly one line (`PageStart::After(c) =>
  Bound::Included(bad_successor(c))`) **passed all seven new clauses plus the four
  pre-existing ones** (`contract_scan_page_orders_by_raw_bytes`,
  `…_cursor_is_exclusive`, `…_walk_terminates_and_is_complete`,
  `…_no_skip_for_stable_keys`, `…_limit_bounds_the_page`, `…_escapes_the_scan_cap`,
  `…_refuses_a_zero_page_bound`), while a plain walk of `p:a`, `p:a0`, `p:b` at
  `limit = 1` returned `[p:a, p:b]` — `p:a0` was present throughout the walk and was
  silently omitted, exactly the failure `crates/traits/src/lib.rs:1012` and the brief's
  "Invariant to restore" forbid. The clause fixtures do contain the strict-prefix pair
  the brief asked for (`crates/metadata-conformance/src/lib.rs:525`: `b"a"`, `b"a0"`),
  but they are only ever read with `limit` 2 (`:576`) and 16 (`:564`), and at both the
  boundary falls elsewhere, so the pair proves *ordering inside a page* and nothing about
  *continuation across one*. Measured: at `limit = 1` the walk over that same fixture is
  incomplete; at `limit` 2 and 16 it is complete. **One added lap closes it** — e.g.
  `walk(store, b"p:", 1, LAP_BUDGET)` beside `:576`, or a clause-(b) case with
  `after = b"p:a"` while `p:a0` is stored. This is not a theoretical class: the one backend
  that resolves the cursor by explicit successor arithmetic is TiKV
  (`crates/metadata-tikv/src/lib.rs:435` `next_page_start`, correct today — it appends
  `0x00`), and it is also the only backend whose conformance run is off-Check, so the
  suite's blind spot sits precisely where nothing else is watching. The rubric's *Absent
  or unsupported entries* class names this shape directly: "never … a count-based
  assertion that can pass while the property fails".

- **NEEDS-HUMAN [impl] — the trait contract's structural claim about
  `test_double_scan_page` is false as written.** `crates/traits/src/lib.rs:1052-1054`
  states the helper "lives in the **dev-only** testkit crate so no production backend can
  reach it at all … it is a dev-dependency everywhere, never a dependency", and
  `crates/testkit/src/lib.rs:774-776` upgrades that to "a production `MetadataStore` body
  naming this function does not compile. What would otherwise be a convention … is a build
  error." `wyrd-testkit` is a **regular** dependency of `wyrd-coordination-mem`
  (`crates/coordination-mem/Cargo.toml:16`, under `[dependencies]` at `:11`) and of
  `wyrd-metadata-fault-conformance` (`crates/metadata-fault-conformance/Cargo.toml:20`),
  and `wyrd-coordination-mem` is itself a regular dependency of the production server
  (`crates/server/Cargo.toml:58`, `[dependencies]` at `:39`) — so the helper is linked into
  the shipped `wyrd` binary and *is* nameable from a non-dev position today. The narrow
  claim (the three metadata backends take testkit as a dev-dep only) is true and is the
  real backstop; the general one is not, and it is the sentence a future backend author
  will rely on. This was iteration 1's carry-forward item ("reduce the dead-code /
  visibility risk of `test_double_scan_page` being an unconditional `pub` item of the
  production `wyrd-traits` crate") — the move to testkit narrowed the exposure but the
  patch documents it as eliminated. Fix is a sentence (or `#[doc(hidden)]` plus the honest
  "convention with a partial build-error backstop", the same caveat the brief itself makes
  about the required-method rule).

- **NEEDS-HUMAN [impl] — a new seam error crosses the seam unclassified and unpinned.**
  `ZeroPageLimit` (`crates/traits/src/lib.rs:359`) is a new fault that a *production*
  backend now raises (`crates/metadata-redb/src/lib.rs:170` via `page_limit`), but it is
  absent from the classifier's normative table (`crates/traits/src/lib.rs:720-727`, which
  lists `ScanCapExceeded` explicitly) and has no classification test. The repo's own
  convention is explicit at `crates/metadata-redb/src/lib.rs:250-256`: "each drives a real
  fault this backend actually produces and asserts it classifies terminal, never transient
  (#591)", and `ScanCapExceeded` has exactly such a test at `:265`. The *behaviour* is
  right — `classify` (`:735`) falls through to `Terminal`, which is what a caller must see
  for a bound of zero — so this is a missing pin, not a wrong answer; but it is the pin
  that keeps a later refactor from turning a permanent refusal into a retry loop.

## Attempted and could not refute

- **The redb production path.** I probed the boundary inputs I could construct against the
  real `RedbMetadataStore::scan_page` (`crates/metadata-redb/src/lib.rs:162-206`): a key
  equal to the prefix; the empty prefix including the empty key; an all-`0xff` prefix;
  strict-prefix neighbours (`p:a`/`p:a0`/`p:a00`) at every `limit` 1..4; and a walk at
  `limit = 1` with an insert behind the cursor on every lap. All complete, exactly once,
  in byte order, terminating. No failing case found.
- **`page_start`'s three-arm rule** (`crates/traits/src/lib.rs:494-502`). The documented
  equivalence `c >= upper_bound(prefix) ⟺ c > prefix && !c.starts_with(prefix)` holds on
  every prefix/cursor pair I tried, including the cases the doc singles out (empty prefix,
  all-`0xff` prefix, `p\xff` vs `q`). The one surviving C5 mutant in this function
  — `crates/traits/src/lib.rs:499:32: replace > with >=`, the `cursor > prefix` guard — is an **equivalent** mutant, not a
  coverage gap: `cursor == prefix` is consumed by the `starts_with` arm above it, so no
  input can distinguish the two spellings.
- **Leg D's non-vacuity.** Every `#[should_panic]` target in
  `crates/metadata-conformance/tests/scan_page_demonstrated_red.rs` genuinely panics inside
  the matching clause's own assertion (expected-substring matches are unique to those
  assertions), and each violating double's sibling test genuinely passes the clauses it is
  meant to survive. The two *conforming* doubles (`FaithfulPagedStore`, `ShortPagedStore`)
  pass all seven clauses, so the suite does not reject a legal short-page store — the
  iteration-3 finding is discharged, and so is the values half (`assert_pairs_eq`,
  `crates/metadata-conformance/src/lib.rs:454`, caught `KeysOnlyStore` on both clause (a)
  and clause (d)).
- **The iteration-1 zero-cap defect.** `page_limit` refuses `min(limit, cap) == 0`
  (`crates/traits/src/lib.rs:414-423`), the redb store is driven at `with_scan_cap(0)` with
  a seeded population (`crates/metadata-redb/tests/scan_page.rs:213`), and the clause runs
  on redb + both sim stores + fdb + tikv through `run_all_cap_scoped`. I could not get any
  page out of a zero-cap store.
- **Runner drift.** All six metadata drivers call *both* `run_all` and
  `run_all_cap_scoped` (redb `tests/conformance.rs:34,45`; dst `tests/conformance.rs:35-76`
  ×3 stores; fdb `:136,155`; tikv `:53,69`), so no backend with a driver is left unproven.
- **Docs currency.** No stale "the store offers only a whole-namespace scan" paragraph
  survives; `docs/design/architecture/05-building-block-view.md:204` now states both
  operations and their contracts. Nothing under `docs/design/adr/`, `specs/` or `0016`
  itself was touched.

## Notes on the gate rows (not refutations)

- **C5 "22 missed" overstates the coverage gap.** Of the 22 in
  `mutants.out/missed.txt`, **18** are inside `#[cfg(feature = "fdb")]` / `#[cfg(feature =
  "tikv")]` bodies (11 in `crates/metadata-fdb/src/lib.rs`, 7 in
  `crates/metadata-tikv/src/lib.rs`) that the default gate does not compile at all — the
  mutation cannot change behaviour, so the suite passes by construction — 2 are in the
  message-only `escaped` helper
  (`crates/metadata-conformance/src/lib.rs:428`, documented as message-only at `:437`),
  and 1 is the equivalent `page_start` mutant above. The only one worth a line is
  `crates/metadata-conformance/src/lib.rs:941` (`>` → `>=` when picking the
  delete-ahead-of-the-cursor key): with `>=` the fixture may delete a key the walk has
  already returned, which weakens clause (d)'s mutation without failing it.
- **C4-verify's row wording** ("red without the fix, green with it") is, by the brief's own
  `Falsifiability` analysis, a **build-error** red over a run that executed nothing; the
  binding semantic red is leg D. The brief pre-declared this and prior sign-offs accepted
  it, so I raise it only so the row is not read as more than it is.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T4 Contribution — A maintainer must decide whether to rely on the reported zero-blocking batched review — `scripts/review-branch` and its finding report are absent from the allowed target/inputs, so that gate cannot be independently reproduced; affected-path history and closed-PR search otherwise found no prior implementation (only proposal PR #627), and current open PRs are dependency/workflow updates.
- [ ] Validation — fitness-to-purpose — A maintainer must decide whether this seam is fit for the later `retire:` and `orphan:` consumers — this slice intentionally has no production caller, so primitive-level cap escape cannot establish the eventual maintenance loop’s operational page budget or end-to-end behavior; the architectural boundary is recorded at `docs/design/architecture/05-building-block-view.md:204`.

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
- Iteration delta (if iterating): Auto-iterate (round 2): Check found implementation-level items only, no architectural judgment required — T4 Contribution — A maintainer must decide whether to rely on the reported zero-blocking batched review — `scripts/review-branch` and its finding report are absent from the allowed target/inputs, so that gate cannot be independently reproduced; affected-path history and closed-PR search otherwise found no prior implementation (only proposal PR #627), and current open PRs are dependency/workflow updates.
- By / date: auto-iterate / 2026-07-26

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
