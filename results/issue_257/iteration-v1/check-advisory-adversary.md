# Adversarial review — issue 257 / m4.6-tier1-jepsen-tier2

Skeptic's pass. Grounded on target source at `$PDCA_TARGET`
(`/home/eddie/wyrd/wyrd.pdca-wt-l0`, patch applied). Advisory only — I do not gate.

## Attacks that landed

- **NEEDS-HUMAN — The asserted Check-observable red→green did not reproduce.** The brief
  makes the "pure dispatch/seam unit tests" the *load-bearing flippable* — "RED when
  negated, GREEN on the tree" (brief.md:52-56, 97-100). But `check-gates.json` records
  **C4-verify = fail**: `run-verify.sh: FAIL — the test PASSES without the fix, so it does
  not catch the bug (no red)`. The one deterministic proof the brief promised at Check is
  the one the gate says is missing. Overall still shows `pass` only because that row was
  marked `gating:false`. This is exactly the reviewer rationalization to distrust: the
  DEFERRED/privileged tier-green (legitimately off-Check) is being conflated with the
  on-Check dispatch/seam red→green the brief *required* to be demonstrable here.

- **`meta_dispatch` test is a tautological change-detector, not a defect regression**
  (`xtask/src/faults.rs:599` fn, `:917` test). `meta_dispatch` returns hardcoded string
  literals (`"wyrd-metadata-tikv"`, `"tier1_metadata_integration"`, …); the test at :929/:938
  asserts those *same* literals. The entire `MetaTier`/`MetaDispatch`/`meta_dispatch` surface
  is net-new in this patch — there is **no pre-existing production defect** it protects.
  "Red" is only reachable by hand-editing `meta_dispatch` to point at the chunkstore crate,
  which the brief itself concedes ("Do SHOULD supply a temporary negation"). Concrete gap:
  the test never checks that the routed `package`/`--test` target actually *resolves* — a
  matching typo in both the literal and the expectation passes green while
  `cargo test -p … --test …` would fail off-Check. It asserts a string equals itself; it
  gives false assurance that routing is correct.

- **NEEDS-HUMAN — The committed "promoted regression" does not model what it claims and can
  never go red** (`crates/dst/tests/tikv_surfaced_regressions.rs:76-99`). `AwaitingStore::commit`
  yields **before** delegating to the inner store (`:80`), and the inner `RedbMetadataStore::commit`
  (`crates/metadata-redb/src/lib.rs:72-98`) contains **no `.await`**: precondition read, puts,
  deletes and `txn.commit()` all execute inside a single `begin_write()` transaction. Under
  madsim's single-threaded deterministic executor, once a racer enters `inner.commit()` it runs
  to completion before any other task resumes — so **no racer can ever be scheduled "between
  this writer's precondition read and its write"** (contradicting the comment at
  `tikv_surfaced_regressions.rs:77`). Concrete failing-to-refute case: delete the `AwaitingStore`
  decorator and drive `RedbMetadataStore` directly — `winners == 1` still holds for every seed,
  because it is guaranteed by redb's serialized write txn, not by anything this test adds. The
  test therefore promotes *nothing the redb fake did not already model* and re-proves Tier-0
  atomicity — which the invariants explicitly forbid ("a real environment is never used to test
  correctness the simulation already covers"). Whether this satisfies the mandatory
  compounding-loop DoD bullet is a human call (brief.md:174-177).

- **`PROMOTED_SEED = 17` is decorative** (`tikv_surfaced_regressions.rs:52`, used only at `:91`).
  The seed is never asserted on — it merely triggers an `eprintln!`. The test asserts the same
  two invariants identically across all 50 seeds; nothing is pinned to seed 17 and, per the
  point above, no "mid-commit interleaving" is surfaced by it. The doc claim (`:31-32`, "the
  seed that first surfaced the mid-commit interleaving … recorded as the committed regression
  fixture") describes a fixture that does not exist.

## Attacks that did NOT land (attempted, could not refute)

- **`quorum_safe_max` / `SeededMetaFaults::minority` seam tests** (`crates/testkit/src/lib.rs:397`,
  tests at `:620`). Unlike the dispatch test, these carry an **independent oracle** —
  `survivors * 2 > n` (`:637`) — that is not just a restatement of the implementation. A
  regression from `⌊(n-1)/2⌋` to `n/2` genuinely flips them red (n=4 → faults 2, leaves 2, not a
  majority). Determinism (`same seed → same faulted nodes`) and the minority bound are real
  properties. I could not construct a passing-for-the-wrong-reason case here.
- **Trait untouched / clean-skip gating.** `MetadataStore` (`crates/traits/src/lib.rs:337`) is
  unmodified; the tier tests are `#[ignore]` + env-gated and cfg-out their TiKV bodies without
  `--features tikv`, so `cargo xtask ci` compiles but never runs them. The C4-ci green is honest.
  Package names (`wyrd-metadata-tikv`) and `TikvMetadataStore::connect`/`with_namespace`
  (`crates/metadata-tikv/src/lib.rs:435,446`) exist as referenced. No refutation.

## Bottom line

The two *genuine* red→green artifacts (the testkit quorum seam tests) survive scrutiny, but the
two the brief leans on as the milestone's headline evidence do not: the `meta_dispatch` test is a
self-referential tautology, and the "compounding-loop" DST regression is a no-op decorator that
re-proves redb's own atomicity and models none of the TiKV await-inside-commit behavior it claims.
The deterministic C4-verify gate independently agrees (no red). Human adjudication needed on the
seeded-regression DoD and on the overall `pass` verdict resting on a non-reproduced flippable.
