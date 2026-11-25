The UploadPart commit slice (#927)—atomic part publication, outcome classification, compensation, re-upload and ceilings—needs one implementation correction: malformed session records can erase doubt about a batch that still lands.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief defines observable outcomes, read ordering, lease boundaries and settled scope decisions; the requested surface is specific and testable (brief.md:31, brief.md:269, brief.md:640). |
| C2 Reproduction (red pre-fix) | N/A | This is new functionality with no baseline commit API; the independent stash run fails to import the module before executing any test, so it establishes no behavioral red (brief.md:321; reviewer-red-green.log:6). The acceptance decision is recorded under C4. |
| C3 Change | PASS | The five-file patch stays within the approved core-only slice and updates its living architecture; it adds no production caller or protocol-schema change (patch.diff:1; crates/core/src/multipart/commit.rs:151; docs/design/architecture/05-building-block-view.md:248). The correctness defect is recorded under C5. |
| C4 Verification (red→green) | NEEDS-HUMAN | Accept or reject the predeclared lack of an executable baseline red—44/44 tests pass after restoration, but the reverted API cannot compile; frozen full CI passed and independent CI has the target-copy caveat below (brief.md:328; reviewer-red-green.log:6; reviewer-red-green.log:83; gate-logs/C4-ci.log:4540). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Validate session observations before changing outstanding doubt—three independent probes show a definite stop or false may_still_commit despite a later successful landing (crates/core/src/multipart/commit.rs:878, :896, :1136; reviewer-probe.log:69). |
| T1 Structure | PASS | The core module uses the existing metadata/chunk-store traits and staging hand-off without a concrete backend or runtime dependency in production (crates/core/src/multipart/commit.rs:164, :168, :1202). |
| T2 Shape | PASS | Writer constructors and derived summaries preserve record identity; the test wrapper checks decoding and slot preconditions on every submitted batch (crates/core/src/multipart.rs:3029, :3058, :4102; crates/core/tests/multipart_upload_part_commit.rs:464, :570). |
| T3 Runtime | PASS | Retry budgets, dispatch-margin checks and one caller-owned clock bound the exercised paths; external operations inherit the backend termination contract (crates/core/src/multipart/commit.rs:193, :200, :817; crates/traits/src/lib.rs:1334). Outcome-classification correctness is assessed under C5. |
| T4 Contribution | FAIL | The frozen multi-pass review's session-validation finding remains reproducible; the independent affected-path history/closed-PR check found no conflicting prior art, and contribution artifacts are N/A until their mandatory publish audit (gate-logs/T4-batch-review.log:10; reviewer-prior-art.json:10; gate-logs/T4-contribution.log:10). |
| T5 Judgment | PASS | Ratified size, dispatch-only lease semantics and live-session refusal choices are respected; seeded DST work remains the settled #660 deferral, and no load-time capability probe was added (brief.md:641; crates/core/src/multipart/commit.rs:30, :157). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the core-only UploadPart API and evidence meet slice #927's intended use after the confirmed defect is corrected; production S3 wiring remains the already-agreed later slice (brief.md:300, brief.md:389; docs/design/architecture/05-building-block-view.md:248). |

Source citations are relative to `$PDCA_TARGET`; `brief.md`, `patch.diff`, logs and reviewer probes are evidence in this review directory. The target matches the patch (`git apply --reverse --check ../patch.diff` succeeds), and the stashed production changes were restored. No production source was edited by this review.

**F1 — Preserve uncertainty when a session record cannot be decoded.** The first session observation reaches `Doubt::session_seen` without validation; that function treats differing bytes as irreversible movement and clears `may_land` (`crates/core/src/multipart/commit.rs:496`). A subsequent held-slot read then removes the doubt entirely (`:890`). The final commit-session read (`:896`) and compensation-session read (`:1136`) likewise update doubt before decoding, incorrectly changing its live flag even when decoding fails. This violates the metadata-validation boundary and the requirement to retain an unknown outcome; it is the same underlying defect as the frozen batch-review finding, with two additional affected sites.

Three probes against unchanged production code establish the impact:

- First session read corrupted: returned `Stopped { cause: Record, slot_released: false }`; repairing the session and releasing the held commit returned `Committed` (reviewer-probe.log:82).
- Last commit-session read corrupted: returned `InDoubt { may_still_commit: false }`; the held commit still committed after repair (reviewer-probe.log:76).
- Compensation-session read corrupted: returned `InDoubt { may_still_commit: false }`; the held compensation still committed after repair (reviewer-probe.log:69).

The existing damaged-session cases inject definite conflicts, so they cannot detect losing outstanding doubt (`crates/core/tests/multipart_upload_part_commit.rs:3212, :3230, :3239`). Decode each session observation before using it as evidence of irreversible movement, preserve unresolved doubt on decode failure, and add these held-batch cases. The isolated probe crate copies the supplied harness and appends three tests; rerun from this directory with:

```sh
CARGO_TARGET_DIR="$PDCA_TARGET/target" cargo test --offline   --manifest-path pdca-reviewer-927-probe/Cargo.toml reviewer_ -- --nocapture
```

**Gate evidence distinguishes passing checks from unverified claims.** All seven frozen logs were readable; instance-scoped wrappers were adjudicated from their captured output, with the regression and full CI commands independently rerun.

| Gate | Assessment | Evidence |
|------|------------|----------|
| C4-ci | PASS in frozen evidence; independent target-copy caveat below | Full frozen CI reaches dependency audits, conformance and DST, then reports all checks passed (gate-logs/C4-ci.log:3888, :3900, :3914, :4540). |
| C4-verify | NEEDS-HUMAN | Both frozen and independent runs have green tests but a compile-only reverted failure; no executed pre-fix red (gate-logs/C4-verify.log:15; reviewer-red-green.log:6, :83). |
| C4-diff-cov | PASS in frozen evidence | 619/633 instrumentable changed lines executed, 97.8%; 773 unscored changed lines are explicitly excluded (gate-logs/C4-diff-cov.log:70, :98). |
| C5-mutants | PASS in frozen evidence | 78 tested: 63 caught, 15 unviable; no reported survivors. This does not cover the three newly demonstrated cases (gate-logs/C5-mutants.log:13). |
| T4-batch-review | FAIL, independently confirmed | F1 reproduces and also affects the two other session observations (gate-logs/T4-batch-review.log:10; reviewer-probe.log:96). |
| T4-contribution | N/A | PR/commit artifacts are absent by design; their substantive audit must rerun at publish (gate-logs/T4-contribution.log:10). |
| host-tikv | PASS in frozen evidence | Both real feature-enabled clippy builds finish; this establishes compilation, not live-cluster behavior (gate-logs/host-tikv.log:112, :211). |

**Prior art is checked by affected path.** Independent GitHub queries examined merged history at the brief's base and all 19 closed, unmerged PRs' changed files. None of those PRs touched the multipart source/test surfaces or architecture page; merged history shows the existing Create, Abort and staging work, and no commit-test file (reviewer-prior-art.json:10, :55, :89). The rejected internal attempts and their disposition are recorded in the brief (brief.md:423, :640); their ratified decisions were not reopened.

**The independent CI failure is a target-copy caveat, not a patch defect.** `cargo xtask ci` completed with exit 1 and two failures caused by the same absent `.github/advisory-db` gitlink: its index test and the advisory-pin prerequisite (reviewer-ci.log:3355, :4001). The harness copy's `git ls-files --stage .github/advisory-db` is empty, while the actual base records submodule SHA `40f5be6fb12ed93fc591bf4ab23b7d793b3f2239` (reviewer-base-pin.json:1); neither this pin nor its test is changed by the patch. Thus the local run cannot establish a full CI green or completed dependency audits; the frozen full-run log supplies that evidence. Locally, `custodian_gc` passed all 10 tests in 0.17 seconds and the 33-test DST custodian suite passed in 167.03 seconds (reviewer-ci.log:2596, :3921). The brief's external tools, `typos` and the docs renderer, actually ran successfully (reviewer-ci.log:2, :7); no shim or code-read substitutes for them.

<!-- pdca:leaf-complete -->
