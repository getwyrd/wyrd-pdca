Review of #894’s multipart admission and Create implementation: one reproduced ownership defect warrants a builder iteration.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The scope and success conditions distinguish admission contention, identity collisions, and ambiguous commits; the excluded case requires both identical upload ID and nonce (`brief.md:61`; `crates/core/src/multipart/create.rs:256`). |
| C2 Reproduction (red pre-fix) | NEEDS-HUMAN | Accept the declared new-API verification exception — the independent stash run fails at the missing import before any assertion, so it establishes no behavioral pre-fix red (`brief.md:196`; `review-red-green.log:3`). |
| C3 Change | FAIL | An overlapping creator can win the upload ID with a different nonce and object, yet recovery reports that upload as this caller’s successful Create (`crates/core/src/multipart/create.rs:305`; `review-collision.log:53`). |
| C4 Verification (red→green) | FAIL | The reviewer’s compiled ownership counterexample fails on the patched target, despite independent greens for all 19 supplied core tests and six DST tests (`review-collision.log:56`; `review-red-green.log:53`; `review-dst-test.log:248`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Distinguish another creator’s session before adopting it and add the overlapping-collision regression — absence plus an unknown outcome does not establish ownership when the stored nonce differs (`crates/core/src/multipart/create.rs:305`; `crates/core/src/multipart/create.rs:498`). |
| T1 Structure | PASS | Core retains the narrow MetadataStore seam and caller-supplied runtime; no concrete production backend or runtime dependency is introduced (`crates/core/src/multipart/create.rs:72`; `crates/core/src/multipart/create.rs:272`). |
| T2 Shape | PASS | Canonical creation routes preserve decoder boundaries, and the living architecture documents the new writer and its production reach (`crates/core/src/multipart.rs:1895`; `crates/core/src/multipart.rs:2349`; `docs/design/architecture/05-building-block-view.md:206`). |
| T3 Runtime | PASS | Retry and wait bounds follow the active profile, store awaits inherit the terminating-operation contract, and the caller supplies the lifecycle clock (`crates/core/src/multipart/create.rs:263`; `crates/core/src/multipart/create.rs:371`; `crates/traits/src/lib.rs:1326`). |
| T4 Contribution | N/A | Contribution artifacts are absent by design; their substantive audit must re-run at publish (`gate-logs/T4-contribution.log:10`). |
| T5 Judgment | PASS | The change stays within the core-only slice, preserves the tracked bucket-fence deferral, and affected-path prior-art queries found no competing rejected PR implementation (`crates/core/src/multipart/create.rs:569`; `review-prior-art-paths.json:1`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether this core-only slice meets #894 after the ownership defect is resolved — end-user multipart behavior is intentionally deferred to #668 and is not established here (`brief.md:221`; `crates/core/src/multipart/create.rs:21`). |

Source citations are relative to `$PDCA_TARGET`; evidence files are relative to this review sandbox. The target matches the supplied patch: reverse-apply checking succeeds, and the stashed changes were restored without modifying the reviewed implementation.

**The ownership defect is reproducible and distinguishable from the documented exception.** The first Create reads upload ID X and nonce A absent. Before its unapplied commit returns `CommitUnknownResult { may_still_commit: false }`, an overlapping real Create installs X with nonce B for another object. Recovery tests only whether X exists and returns `Created(X)`. The caller requested `photos/requested-object` with nonce 100 but receives the ID for `photos/other-object` with nonce 200; only one identity was minted (`review-collision.log:53`). The same unchecked adoption exists in `finish` (`crates/core/src/multipart/create.rs:498`).

The rustdoc excludes concurrent reuse of **both** ID and nonce (`crates/core/src/multipart/create.rs:256`); this counterexample reuses only the ID. Retain the invocation-history requirement and additionally check that the found session belongs to the intended nonce/request. A mismatching record must not become `Created`; collision handling must also retain any unresolved late-commit doubt. The existing repeated-identity test plants its record before the initial read, so it does not cover this interleaving (`crates/core/tests/multipart_create.rs:1242`).

The reviewer-only reproducer uses the unchanged target crates and a real in-memory redb store. Run from this sandbox:

```sh
CARGO_TARGET_DIR="$PWD/review-build-collision" TMPDIR="$PWD/pdca-reviewer-894-tmp" cargo test --offline --manifest-path pdca-reviewer-894-collision/Cargo.toml -- --nocapture
```

**The supplied success paths and repository checks have substantial passing evidence, but do not cover that collision.**

- Independent execution: 19 core tests pass after stash restoration; all six DST tests pass with `RUSTFLAGS="--cfg madsim" MADSIM_TEST_NUM=50`, including the mixed-outcome witnesses and two expected-panic negative controls. Conformance, statics, formatting, typos, documentation lint and the 99-page render/link audit all pass (`review-red-green.log:53`; `review-dst-test.log:240`; `review-scanners.log:1`). Both external dependencies named in the brief were exercised.
- Frozen gate adjudication: full CI actually reaches deny, conformance, statics, deployment guard, and DST, ending successfully; the brief’s anticipated deny failure is not present in this round (`gate-logs/C4-ci.log:3537`; `gate-logs/C4-ci.log:4149`). Diff coverage is 265/301 instrumentable changed lines, 88%; TiKV and server feature compilation succeed (`gate-logs/C4-diff-cov.log:138`; `gate-logs/host-tikv.log:211`). The batched-review log reports zero findings but contains no individual review reasoning (`gate-logs/T4-batch-review.log:10`); it does not refute the counterexample.
- Mutation results remain advisory: the frozen run reports 39 caught, 12 unviable and three surviving diagnostic mutations (two Display implementations and Error::source; `gate-logs/C5-mutants.log:13`). My independent 54-mutant run reports 33 caught, 20 unviable and one survivor (`review-mutants.log:5`); for example, its Display mutations are unviable because unused formatter parameters trigger denied warnings, not because tests reject them. These results do not establish ownership under overlapping ID collisions.

**Prior art and scope were checked by affected file path.** GitHub commit queries covered all seven changed paths and confirmed the merged nonce, capacity-knob and answer-table prerequisites (`2f41e73531b3`, `4533549cb131`, `14911ec56693`). File lists for all 19 returned closed, unmerged PRs had no overlap with those paths; the rejected-attempt history in #508, #636 and #656 was also read (`review-prior-art-paths.json:1`; `review-issue-508.json:1`; `review-issue-636.json:1`; `review-issue-656.json:1`). Production-callsite searching found no caller outside the new tests. The #511 deferral is settled, and the diff introduces no capability probe that triggers the symptom-guard rule.

<!-- pdca:leaf-complete -->
