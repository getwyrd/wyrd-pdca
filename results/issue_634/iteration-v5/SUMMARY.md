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
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 77 mutants tested in 33s: 18 missed, 28 caught, 31 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Task under review: add a required, bounded, cursor-keyed `MetadataStore::scan_page` seam with identical pagination and cap-escape semantics across production backends, simulator models, and shared conformance tests.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | Proposal 0016 fixes raw-byte order, an exclusive cursor, correct termination, stable-key no-skip semantics, and cap escape as the acceptance context; violating any clause risks permanent retention or an unwalkable namespace (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:2646`). |
| C2 Reproduction (red pre-fix) | PASS | Both added targets independently exited 101 with zero tests on the base because the seam and clauses were absent; with the patch, deliberately violating stores go red while full- and legal short-page stores pass, establishing semantic non-vacuity (`crates/metadata-conformance/tests/scan_page_demonstrated_red.rs:15`). |
| C3 Change | PASS | Requiring a bodyless trait method removes the cap-inheriting default failure mode, while the single shared runners prevent backend clause lists from drifting (`crates/traits/src/lib.rs:1057`; `crates/metadata-conformance/src/lib.rs:1458`). |
| C4 Verification (red→green) | PASS | The two targets reproduced zero-test red→25+10 green, and all CI substance, both feature builds, TiKV unit tests, DST, and real FDB conformance passed; the direct CI launcher alone hit a host-only read-only advisory-lock, after which all three dependency-wall commands passed with a writable reviewer cache (`crates/metadata-redb/tests/scan_page.rs:97`). |
| C5 Causal adequacy | PASS | The patch removes the cause through required native range implementations rather than adding a capability probe or runtime fallback; the 18 reproduced mutation survivors are confined to optional FDB/TiKV bodies omitted by the default mutant build, and this experimental gate is explicitly non-blocking (`AGENTS.md:72`; `crates/metadata-fdb/src/lib.rs:1858`). |
| T1 Structure | PASS | The widening remains one narrow traits-layer seam, production backends implement it natively, and mechanical doubles share a test-seam helper without reversing backend dependency direction (`AGENTS.md:143`; `crates/testkit/src/lib.rs:759`). |
| T2 Shape | PASS | The exact page/result type, typed zero-bound refusal, limit clamp, cursor classification, and required signature make the caller-visible bounds and progress rules unambiguous (`crates/traits/src/lib.rs:326`; `crates/traits/src/lib.rs:387`; `crates/traits/src/lib.rs:471`). |
| T3 Runtime | PASS | Redb and all three DST stores passed the shared and cap-scoped suites, both optional backends type-checked, TiKV unit tests passed, and a real FDB cluster passed conformance/cap escape; the earlier live-TiKV backseat is a settled deferral and may not be re-raised (`crates/dst/tests/conformance.rs:31`; `AGENTS.md:200`). |
| T4 Contribution | NEEDS-HUMAN | A maintainer must decide whether the reported zero-blocking batched review is sufficient without its unavailable `scripts/review-branch` executable/report — this matters because definition of done requires one deep multi-pass review; the independent affected-path merged/closed-PR check found no prior implementation (`AGENTS.md:206`). |
| T5 Judgment | PASS | The 43-file breadth is mechanically forced by a required trait method, while the semantic surface stays confined to the seam, native backends, models/tests, and the required living-architecture update; no consumer or ADR/spec scope was pulled forward (`docs/design/architecture/05-building-block-view.md:204`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | A maintainer must decide whether this seam is fit to carry #636/#637's retirement and orphan drains before those consumers exist — automated contract/backend checks cannot yet expose workload-level pagination or reclamation mismatches, whose impact would be unbounded retention (`docs/design/architecture/05-building-block-view.md:204`). |

### Advisory — adversary

# Adversarial review — issue 634 / scan-page-seam (advisory, non-gating)

Attacked the evidence and the fix on the target at `$PDCA_TARGET`
(`/home/eddie/development/wyrd/wyrd.pdca-wt`, patch applied, base `22d71b4`). Ran the
default-gate targets, a differential probe of redb `scan_page` against a model of the
contract, and — because the two distributed backends are the declared blind spot — the
maintainer-run `cargo xtask fdb-conformance` leg against a live single-node FoundationDB
in a throwaway container. One attack landed.

## Findings

- **NEEDS-HUMAN [impl] — `crates/metadata-fdb/src/lib.rs:1460` (with `:1464` and
  `crates/traits/src/lib.rs:533`): the new `scan_page_once` chunk loop is exercised by
  nothing, at any tier, and the failure it guards against is a silent truncation of the
  walk.** `scan_page_once` is the only page body that must survive FDB returning a
  *partial* reply (`more() == true` with fewer than `limit` rows). Every population the
  suite drives it with is ≤ 25 keys of a few bytes, so the first `get_range` chunk always
  satisfies the whole page and the loop never iterates. Demonstrated, not inferred: I
  applied the exact mutant `cargo mutants` already reports as missed (`:1460`, `>=` → `<`)
  and ran the **whole** maintainer leg `cargo xtask fdb-conformance` against a live
  `foundationdb:7.3.77` cluster — it exited **0**, all five legs green (`--lib`,
  `conformance`, `contention`, `scan`, `timeout`), including
  `trait_contract_against_fdb`'s `run_all` + `run_all_cap_scoped`. With that same mutant a
  600-key × 512-byte fixture (the shape `crates/metadata-fdb/tests/scan.rs:95`,`:101`
  already uses) gives `scan_page(b"dir:", None, 5_000)` → **138 of 600 pairs with
  `next: None`**: `page_cursor` (`crates/traits/src/lib.rs:533`) turns the short page into
  "the prefix is exhausted", so a walk stops and 462 keys are never seen again — precisely
  the `retire:`/`orphan:` silent-skip this slice exists to prevent. The shipped code is
  correct (I re-ran unmutated: 600/600, and a limit-400 walk returns every key exactly
  once), so this is a **test** gap, not a live bug — but it is the one the repo has
  already written down as mandatory for this exact loop: `crates/metadata-fdb/tests/scan.rs:62`
  exists for `scan` with the stated rationale "the shared conformance clause only ever
  stores a handful of keys, so it fits in FDB's **first** page and never advances the
  cursor… This binary is what makes that truncation fail." The patch adds a second copy of
  that loop and no second copy of that binary. Fix: add the `scan_page` analogue beside it
  (multi-chunk fixture, grounded the same way, asserting completeness **and**
  `items.len() <= limit`); the same at-scale leg is absent for TiKV
  (`crates/metadata-tikv/tests/scan.rs` covers `scan` only).

- **NEEDS-HUMAN [impl] — `crates/metadata-tikv/src/lib.rs:1165-1166`: one `scan_page` call
  can make the client materialize `regions × limit` pairs, which is the heap bound the
  page bound is documented to enforce.** The page is read as a single
  `txn.scan(range, page_size)` with `page_size = min(limit, scan_cap)`, i.e. up to
  1,048,576 for the `usize::MAX` limit the seam explicitly invites
  (`crates/traits/src/lib.rs` "a `limit` above the store's cap is **clamped**, never an
  `Err`", and `contract_scan_page_limit_bounds_the_page` calls exactly that). tikv-client
  shards a `ScanRequest` per region and `apply_shard` rewrites only the key bounds — the
  `limit` field is carried **unchanged into every region's request**
  (`tikv-client-0.4.0/src/request/shard.rs:272-300`), the responses are `Collect`-merged
  and only then sorted and truncated (`.../src/transaction/buffer.rs:181`). Concrete case:
  the very population the brief cites — an `orphan:` ledger of ~1.78 M marks spread over N
  regions — answered with `scan_page(b"orphan:", None, usize::MAX)` pulls up to
  `N × 1,048,576` pairs into client memory to return one 1 M-pair page. `scan` avoids this
  deliberately by looping `PAGE_SIZE = 1024` reads (`crates/metadata-tikv/src/lib.rs:428`,
  `:1074`), which is the machinery the brief pointed Do at; re-using it here (loop
  `PAGE_SIZE`-bounded reads inside the same transaction until `limit` items or the range
  ends) keeps the "one page = one read timestamp" property the doc claims. Off-Check, so
  no gate can see it.

- **NEEDS-HUMAN — the Verification-posture claim that "the behavioural green for those two
  backends is the shared conformance suite run against real servers" is over-broad for
  what this slice actually adds.** The suite's populations never reach either backend's
  within-page paging path (finding 1), and all **18** surviving mutants in the C5 row sit
  in exactly those two bodies — including
  `crates/metadata-tikv/src/lib.rs:1135 → Ok(Default::default())`, i.e. "every page is
  empty and terminal", the whole-namespace silent skip. For FDB I discharged the
  equivalent myself: the same body-level mutation *is* caught by the live leg
  (`trait_contract_against_fdb` fails at `crates/metadata-conformance/src/lib.rs:496`), so
  the sign-off condition "TiKV backseat as long as redb/FDB stay green" is met for FDB on
  this host, today, with the caveat above. For TiKV it is **unverified anywhere**: the
  cluster on this host still fails at the first clause with
  `InvalidKeyMode { storage_api_version: V2 }` before any `scan_page` runs — the same
  environment fault recorded at iteration 2, so per issue #236 that is *not* scored as a
  refutation, only as a verdict that stays provisional for that backend.

- Not a refutation, recorded for the human: `run_all_cap_scoped`
  (`crates/metadata-conformance/src/lib.rs:1502`) is a **second** runner a driver must
  remember to call, so the cap-escape and zero-cap clauses are the one part of the suite a
  future backend can silently skip while `run_all` still passes. All four current drivers
  wire it (redb, DST×2, FDB, TiKV), and the brief authorised a "parameterised over a
  cap-lowering hook" runner, so this is residual drift risk, not a defect.

## Attacked and could not refute

- **redb's production body.** A differential probe (built in scratch, not in the target)
  compared `RedbMetadataStore::scan_page` against a straight model of the four clauses over
  a byte-nasty corpus — keys containing `0x00`/`0x7f`/`0x80`/`0xff`/multi-byte UTF-8, keys
  that are strict prefixes of others, `p` vs `p:` vs `p;`, a neighbouring `o:`/`q:` decoy —
  crossed with 7 prefixes (including `b""` and `b"p:\xff"`), ~30 cursors (including
  `None`, `b""`, below-prefix, past-prefix, non-existent between-keys) and limits
  `{1,2,3,7,usize::MAX}` at caps `{1,2,3,5,2^20}`: **exact match on every one of ~10k
  cases**, plus every walk returning every key exactly once and terminating. No edge input
  found.
- **The iteration-1..3 defects are genuinely closed, not papered over**: the zero effective
  cap now refuses with the seam type (`crates/traits/src/lib.rs:414` `page_limit`, driven
  through the real backend at `crates/metadata-redb/tests/scan_page.rs:213`); the clauses
  assert **values**, not just keys (`assert_pairs_eq`, and the `KeysOnlyStore` double proves
  it non-vacuous); the fixtures no longer assume a full first page
  (`assert_page_is_next_of`, and `ShortPagedStore` proves a conforming short-paging store
  still passes).
- **`page_start`'s three-arm claim.** Its "`PastPrefix` ⟺ `cursor >= upper_bound(prefix)`"
  equivalence holds on the boundary cases I could construct, the tikv-client panic it
  exists to avoid is real (`BTreeMap::range` on an inverted `BoundRange`,
  `.../src/transaction/buffer.rs:129-131`), and all five implementations plus the testkit
  helper match on the enum exhaustively.
- **The required-method rule holds mechanically**: every `impl MetadataStore for` in the
  workspace now carries `scan_page`, the three production backends implement it natively,
  and the ~34 doubles delegate through one identical line to `wyrd_testkit`.
- **Both feature-gated backends type-check** here (`cargo check -p wyrd-metadata-fdb
  --features fdb --tests` and `-p wyrd-metadata-tikv --features tikv --tests`, both clean),
  so the CLI-rot risk the brief flags did not materialise.
- **The C4-verify "red" is a build error, as the brief itself concedes** — I did not score
  that as a refutation: leg D's `#[should_panic]` doubles are the semantic red, and they are
  real (9 violating stores, each caught by the clause it violates and passing the others).

Scratch (`$PDCA_SCRATCH/pdca-adversary-151-scanpage`) and the throwaway FDB container were
removed; the target worktree was not modified.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T4 Contribution — A maintainer must decide whether the reported zero-blocking batched review is sufficient without its unavailable `scripts/review-branch` executable/report — this matters because definition of done requires one deep multi-pass review; the independent affected-path merged/closed-PR check found no prior implementation (`AGENTS.md:206`).
- [ ] Validation — fitness-to-purpose — A maintainer must decide whether this seam is fit to carry #636/#637's retirement and orphan drains before those consumers exist — automated contract/backend checks cannot yet expose workload-level pagination or reclamation mismatches, whose impact would be unbounded retention (`docs/design/architecture/05-building-block-view.md:204`).

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
- Iteration delta (if iterating): Auto-iterate (round 3): Check found implementation-level items only, no architectural judgment required — T4 Contribution — A maintainer must decide whether the reported zero-blocking batched review is sufficient without its unavailable `scripts/review-branch` executable/report — this matters because definition of done requires one deep multi-pass review; the independent affected-path merged/closed-PR check found no prior implementation (`AGENTS.md:206`).
- By / date: auto-iterate / 2026-07-26

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
