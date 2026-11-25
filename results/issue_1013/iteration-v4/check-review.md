Review of #1013’s multipart residue retirement and terminal session deletion: one confirmed pagination defect still strands otherwise eligible residue.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The exit, exactly-once admission release, tombstone retention, fleet completeness and late-write residual have explicit acceptance criteria; proposal 0016’s terminal rule grounds them (`brief.md:25`; `docs/design/proposals/draft/0016-multipart-commit-protocol.md:582`). |
| C2 Reproduction (red pre-fix) | N/A | This is declared new functionality; the pre-fix discriminator cannot compile against the old API, so its compiler errors are not a behavioral reproduction (`brief.md:115`; `review-red-green.log:7`). |
| C3 Change | PASS | The changes stay within the accepted session-drain slice and its context/test/doc updates; the existing retirement dispatch is retained, with production tombstones protected (`crates/custodian/src/retirement.rs:523`; `crates/server/src/custodian.rs:577`; `brief.md:334`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Accept the declared absence of an executable pre-fix behavioral red and the frozen dependency-audit evidence — 84 focused tests pass independently, but reverting removes required APIs and this disposable copy lacks the RustSec gitlink needed by full CI (`review-red-green.log:7`; `review-ci.log:3676`; `gate-logs/C4-ci.log:4979`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Restore progress beyond a full held page — both independent probes leave the trusted 17th entry present and unmarked after 20 passes because every walk restarts at the first 16 entries (`crates/custodian/src/retirement/sessions.rs:530`; `review-probes.log:8`; `review-probes.log:14`). |
| T1 Structure | PASS | Maintenance remains behind the existing metadata/chunk-store traits and reconstruction dispatch; no new concrete-backend dependency or correctness clock is introduced (`crates/custodian/src/retirement.rs:396`; `crates/custodian/src/retirement/sessions.rs:537`; `crates/custodian/src/reconciliation.rs:162`). |
| T2 Shape | PASS | The accepted single-slice size holds: approximately 38.6 KB of added non-comment production code; the persisted bookmark and runtime behavior are documented in the living architecture (`brief.md:334`; `docs/design/architecture/06-runtime-view.md:90`). |
| T3 Runtime | PASS | The fleet gate, tombstone input and batch bounds survive the focused runtime checks; the documented cost includes held sessions and does not claim GC-listing reuse (progress defect recorded separately in C5) (`crates/custodian/src/retirement.rs:155`; `crates/server/tests/custodian_gc.rs:1469`; `review-server.log:188`). |
| T4 Contribution | FAIL | The frozen batch review remains substantively red on one reproduced pagination defect; its three duplicate reports collapse to C5, and its DST objection is settled by #660; contribution-artifact checking is N/A until publish (`gate-logs/T4-batch-review.log:10`; `gate-logs/T4-contribution.log:10`; `brief.md:334`). |
| T5 Judgment | NEEDS-HUMAN | Confirm semantic supersession of the discarded #636 work — merged-history and closed/rejected searches ran by all 25 affected paths, but bounded history/search results and the old issue description cannot establish equivalence to the unavailable discarded patch (`review-prior-art.json:2`; `review-prior-art-rejected.json:2`; `brief.md:167`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Approve the operational tradeoffs after the pagination defect is fixed — completed sessions retain admission slots until #625, unlisted late writes can remain indefinitely, and the shipped profile permits up to 78 fleet listings per pass (`crates/server/src/custodian.rs:578`; `crates/custodian/src/retirement.rs:135`; `crates/custodian/src/retirement.rs:155`). |

All repository `path:line` citations above and below were read in `$PDCA_TARGET`, the disposable patched copy. Evidence-log paths refer to this review directory. The target source is usable: the patch’s reverse-apply check succeeds, and the stash/reapply exercise preserved the original target status. The synthetic Git index’s missing submodule is a separate target-state caveat.

**One implementation finding: [P2] a full page of held entries hides later eligible residue.** At `crates/custodian/src/retirement/sessions.rs:530`, `scan_page` always receives `after = None`, and its continuation is discarded. The bookmark only reorders entries already in that page (`:565`). If all 16 entries are held, the visit stops without reaching any later entry (`:735`). Subsequent passes repeat the same page. The earlier fix for one held entry ahead of a wide entry therefore does not cover a whole held page; the existing tests exercise the former (`crates/custodian/tests/multipart_session_terminal.rs:2258`, `:2273`).

Two executable probes independently reproduced this through `reconcile_step`, using the supplied real-redb fixture in a separate scratch test package:

- `Aborting`: 16 wrong-length entries are held on a five-of-six-server fleet; a trusted 17th entry on reachable servers remains present with zero orphan marks after 20 passes (`review-probes.log:8`).
- `Completed`, with `Tombstone::Elapsed`: the same 16 untrusted entries are held even on the whole fleet; the trusted 17th entry likewise remains present and unmarked after 20 passes (`review-probes.log:14`).

The impact is unnecessary permanent retention of eligible fragments while earlier entries remain held. The session correctly stays because held residue exists; this finding does not ask to delete it early. Carry bounded scan progress across held pages while preserving intra-entry mark progress, and add both page-boundary regressions. Reproduce the two failures from this directory with:

```sh
CARGO_TARGET_DIR="$PDCA_TARGET/target" cargo test --offline \
  --manifest-path pdca-reviewer-1013-probes/Cargo.toml \
  --test terminal_review reviewer_ -- --nocapture
```

The probe package copies the supplied test fixture and appends the two tests at `pdca-reviewer-1013-probes/terminal_review.rs:2469` and `:2496`; it changes no target source.

**Verification supports the exercised safety properties, with explicit limits.** Independent focused runs passed 27 terminal-session tests, 19 records-drain tests, 26 marks-drain tests, 11 server-loop tests and the admission-release unit test (`review-green.log`, `review-retirement.log:26`, `review-retirement.log:58`, `review-server.log:197`, `review-core.log`). The server-loop test exercises both new production inputs through `run_reconstruction_until`, including startup-partial fleet recovery and completed-tombstone retention. Both new reviewer probes fail on assertions, not compilation.

Stashing the tracked production changes while retaining the new discriminator produced the same four missing-API compiler errors as the frozen gate; after restoring the patch, all 27 discriminator tests passed (`review-red-green.log:7`, `:111`; `gate-logs/C4-verify.log:15`). This is the declared C4 sign-off exception, not a demonstrated behavioral red. The frozen mutation output reports 148 caught, 51 missed, 20 unviable and two timeouts; it does not identify every caught mutant, so it cannot corroborate all six particular hand-mutation witnesses requested by the brief (`gate-logs/C5-mutants.log:66`; `brief.md:126`). Surviving mutations were inspected against the source; their count alone is not a list of defects. The 73-minute sweep was not repeated.

The independent formatting, spelling, documentation lint, 101-page documentation render/link audit, whitespace check and redacted patch secret scan all passed (`review-scanners.log:1`). Both named external dependencies, `typos` and the docs renderer, were actually exercised. The secret scan is a direct scan of `patch.diff`; the historical project commit hook is not present in this target copy, so this does not assert that the hook itself ran.

**Frozen-gate adjudication preserves environment caveats and settled deferrals.**

- C4-ci: its frozen log records the full successful gate (`gate-logs/C4-ci.log:4979`). The independent full-CI run finished with two failures, both caused by the absent `.github/advisory-db` Git-index entry: the index test fails and the pinned dependency audit cannot run (`review-ci.log:3676`, `:3694`). Both `git ls-files --stage` and `git ls-tree HEAD` return no entry; `patch.diff` does not touch it. This is a disposable-copy limitation, not a patch defect.
- C4-verify: UNVERIFIABLE, independently reproduced as described above; no assertion-level pre-fix red was established.
- C4-diff-cov: unmeasured against `origin/main`, consistent with the explicitly accepted stacked base (`gate-logs/C4-diff-cov.log:10`; `brief.md:334`). This is neither measured coverage nor a compilation defect in the supplied patched target.
- C5-mutants: advisory failure with the exact outcomes above; no blanket correctness claim follows from either the caught or missed count.
- T4-batch-review: its three pagination reports describe the same independently reproduced defect. Its missing-DST report is rejected for this review because the current brief explicitly assigns that work to #660 and says to add no DST scenario (`brief.md:334`; `crates/custodian/src/retirement/sessions.rs:178`).
- T4-contribution: **N/A**. The artifacts do not exist at Check by design; the substantive audit must rerun at publish (`gate-logs/T4-contribution.log:10`).
- host-tikv: the frozen log records successful real feature compilations, including `tikv-client`, the metadata crate and server selection arms; this was not a code-read or shim assertion (`gate-logs/host-tikv.log:2`, `:205`, `:211`). The feature build was not independently repeated.

**Prior-art checking ran by affected path, with a semantic limit.** `review-prior-art.json` records the latest five merged-history commits and closed-PR text-search results for each of the 25 affected paths. The retirement modules and new terminal test return no main-branch history or closed-PR path matches, consistent with the stacked prerequisite. A separate closed-unmerged multipart search found PR #647, and its actual changed-file list was intersected with this patch’s surface; issue #636’s closed description also records the abandoned protocol and past convergence failures (`review-prior-art-rejected.json`). These searches do not recover the discarded #636 implementation or exhaust older unmerged diffs, so T5 retains the semantic prior-art decision. No scope expansion, capability-probe workaround, DCO finding, or re-raised #625/#660 deferral is requested.

<!-- pdca:leaf-complete -->
