# Result — issue 768 / last-modified-never-decreases

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: A file's `Last-Modified` never goes backwards across an overwrite, whichever gateway
  of an active/active fleet performs it. Today each gateway stamps `modified` from its own clock
  (`now_millis`, `crates/server/src/lib.rs:813-823`) at both PUT paths (`:251-255` buffered,
  `:402-406` streaming). `commit_written` reads the prior record (`:269-277`) and
  `write::commit_overwrite` (`crates/core/src/write.rs:314-335`) passes `plan.object_meta`
  through unchanged. No code compares the new stamp with the one it replaces, and
  `InodeRecord::checked_shape` validates size and span, not time. The fleet sits behind a
  round-robin balancer with no affinity (`docs/design/architecture/m4-first-deployment-blueprint.md`
  §B.1; three gateways in `deploy/small-multi-node-fdb/docker-compose.yml`; active/active per
  #477). There, a gateway whose clock runs 2 s fast followed by an accurate one makes
  `Last-Modified` go backwards while the content moves forwards. `aws s3 sync` then skips the
  newer file when the bytes have the same size: silent divergence.
- Success criterion: **(A) The rule holds.** A NEW test drives the real `wyrd_server::Gateway`
  through the pre-existing `ObjectGateway` API:
  (1) seed a file whose committed `modified` is **one hour ahead** of the test's wall clock (the
  record a fast-clocked gateway leaves behind), then overwrite it through `put_object_streaming`
  with **different bytes of the same size**: afterwards `head_object` reports **exactly**
  `modified == seeded + 1` (not merely `>=`; an implementation that jumps to `u64::MAX` or any
  other value must fail) and the new ETag;
  (2) the same through the buffered `Gateway::put_object`, also **exactly** `seeded + 1`;
  (3) an overwrite of a file whose prior stamp is in the **past** records the current clock
  (between readings taken just before and just after the call), not `prior + 1`;
  (4) a **create** records the current clock, as today;
  (5) **the rule's exact values, on both core overwrite APIs.** The server legs cannot choose the
  publisher's clock reading, so this leg calls core directly, with an explicit publisher stamp
  `t`, through **both** public overwrite commits: `metadata::commit_chunk_map_superseding`
  (`crates/core/src/metadata.rs:2199`) and `metadata::commit_chunk_map_superseding_leased`
  (`:2268`, the one `write::commit_overwrite` calls; an empty pending set needs no leases,
  `:2426-2444`). Peer: `crates/core/tests/mutation_regressions.rs:446-460` overwrites a seeded
  record this way. For each API, the committed `modified` is:
  - prior `Some(t)`, publisher `Some(t)` (**equal**: two writes in one clock millisecond) →
    `Some(t + 1)`;
  - prior `Some(t + 5_000)` → `Some(t + 5_001)`;
  - prior `Some(t - 5_000)` → `Some(t)`;
  - prior `None` (a legacy record) → `Some(t)`;
  - publisher `None` → `None`, whatever the prior;
  - prior `Some(u64::MAX)` → `Some(u64::MAX)`: saturates, no panic, no wrap to 0;
  - three overwrites in a row, all with publisher `Some(t)`, over prior `Some(t)` →
    `t + 1`, `t + 2`, `t + 3` (the accumulated lead described under Impact, pinned as intended);
  - **the chain resets across an unstamped publication, pinned as intended:** over prior
    `Some(t + 5_000)`, an overwrite with publisher `None` commits `None`; a further overwrite
    with publisher `Some(t)` over that commits `Some(t)`, lower than the `t + 5_000` two
    generations back. This is the documented boundary of the rule (see Invariant and Design),
    not a defect. An implementation that carried `t + 5_000` forward through the `None`
    generation, or stamped that generation itself, fails this case.
  **(B) The rule is stated where readers look.** It is in the living architecture doc's
  object-metadata paragraph (`docs/design/architecture/05-building-block-view.md:197-200`) and in
  the doc comments of the `modified` fields (`crates/core/src/metadata.rs:1598-1600` on
  `ObjectMeta`, `:1648-1653` on `InodeRecord`). It says:
  - across overwrites that each record a time, a file's `modified` never decreases: such an
    overwrite records `max(publishing clock, prior modified + 1 ms)`;
  - a generation published with no time records none, and the overwrite after it starts from
    its own clock. Only core's API can publish without a time; both gateway PUT paths always
    stamp one;
  - repairs still preserve it unchanged;
  - `ETag` is the content-identity token;
  - **in the architecture doc only:** this rule is within ADR-0047's file-level meaning of
    `modified` (the file's last-modified time, `docs/design/adr/0047-object-metadata-model.md:36`).
    The `+1 ms` adjustment for a skewed clock is an implementation detail below that decision,
    not a change to it. This sentence is there so a reader comparing ADR-0047's wording
    ("content-publication time") with the code finds the reasoning without a new ADR.

  No ADR is added or edited.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: making every overwrite publication keep a file's `Last-Modified` from going
  backwards, and stating that rule in the architecture doc and the `modified` field docs / out of
  scope: any new or edited ADR (the rule is within ADR-0047's file-level intent); a clock-skew
  bound or NTP requirement (option 2); the pre-epoch clock that stamps `modified: Some(0)` on a
  **create** (`now_millis`'s `.unwrap_or(0)`, `crates/server/src/lib.rs:819-822`; see Open
  questions); the listing's rendering granularity (#767); the multipart Complete flip (#658 builds
  it and must call the shared rule this slice adds; it does not inherit it); lease stamps (the
  same `now_millis` feeds them, and they keep their meaning).

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): fail — xtask: `cargo deny --all-features --config deny-all-features.toml check advisories` failed with exit status: 1
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (5 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: unverifiable — the -p wyrd-server --test last_modified_monotonic target executed 0 tests, so diff coverage was not measured
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 5 mutants tested in 23s: 3 caught, 2 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_768/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.24s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of issue #768: prevent a file's Last-Modified from decreasing across consecutive stamped overwrites; no implementation defect found, with full CI disposition and fitness-to-purpose still owed.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The agreed contract specifies exact timestamps, saturation, legacy records, and the unstamped reset, with both gateway paths and both core APIs covered (`brief.md:31`; `crates/server/tests/last_modified_monotonic.rs:297`). |
| C2 Reproduction (red pre-fix) | PASS | Independently stashing the production fix while retaining the new test produced the equal-stamp failure and both backwards-time gateway failures: 3 failed, 2 passed (`review-red.log:279`; `review-red.log:305`). |
| C3 Change | PASS | The patch stays within the agreed publication-ordering scope and updates the living field contract without changing record encoding or ADRs (`crates/core/src/metadata.rs:2218`; `crates/core/src/metadata.rs:2330`; `docs/design/architecture/05-building-block-view.md:205`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Arrange baseline dependency remediation and a complete CI rerun — the frozen gate fails on rustls advisories, while this sandbox cannot lock the advisory database; independent regression and coverage checks pass, but the supplemental DST campaign is incomplete (`review-dst.log:504`; `gate-logs/C4-ci.log:6596`; `gate-logs/C4-ci.log:6626`; `review-ci.log:3321`; `review-green.log:16`). |
| C5 Causal adequacy | PASS | Ordering against the CAS-protected prior closes the actual overwrite gap on both APIs; exact-value tests and 3 caught mutants support the rule, with no capability probe or symptom guard (`crates/core/src/metadata.rs:2222`; `crates/core/src/metadata.rs:2334`; `review-mutants-final.log:4`). |
| T1 Structure | PASS | Publication policy stays in core with caller-supplied time, preserving trait direction and the lease clock lifecycle; no new clock read, shared global, or backend dependency is introduced (`crates/core/src/metadata.rs:2269`; `crates/core/src/write.rs:321`; `review-statics.log:3`). |
| T2 Shape | PASS | One small helper expresses the complete rule; field comments and the living architecture explain its boundary and ETag's identity role, and formatting/clippy/docs checks pass (`crates/core/src/metadata.rs:1587`; `crates/core/src/metadata.rs:1643`; `review-ci.log:10`; `review-ci.log:279`). |
| T3 Runtime | PASS | The change adds constant-time arithmetic without I/O or new concurrency; saturation prevents wraparound and the unchanged atomic CAS rejects stale priors (`crates/core/src/metadata.rs:2272`; `crates/core/src/metadata.rs:2334`; `crates/server/tests/last_modified_monotonic.rs:306`). |
| T4 Contribution | PASS | Path-based merged history and all 19 closed/unmerged PRs disclose no prior fix or rejection of timestamp ordering; contribution-artifact audit is N/A until mandatory publish-time rerun (`review-prior-art.log:2`; `review-prior-art.log:7`; `gate-logs/T4-contribution.log:10`). |
| T5 Judgment | PASS | The implementation respects the accepted bounded rule and scope; multipart adoption remains the already tracked #658 obligation, and no new implementation concern requires iteration (`crates/core/src/metadata.rs:2255`; `crates/core/src/metadata.rs:2263`; `brief.md:131`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Confirm the bounded timestamp contract meets the sync use case — an unstamped generation resets order, rapid writes accumulate clock lead, and whole-second wire timestamps can tie (`crates/server/tests/last_modified_monotonic.rs:319`; `crates/server/tests/last_modified_monotonic.rs:330`; `docs/design/architecture/05-building-block-view.md:197`). |

The independent checks support the patch's stated behavior. `git stash push` removed the tracked fix, leaving the new regression file; `cargo test --locked --offline -p wyrd-server --test last_modified_monotonic` ran red with three assertion failures. After `git stash pop`, the identical command passed all five tests. These exercise the real Gateway, redb, and filesystem chunk store; the future-stamped fixture demonstrably exhibits the forbidden failure before the fix. Creates and past-stamped overwrites remain clock-based, and both core APIs pass the exact-value and chained cases (`review-red.log:268`; `review-green.log:9`; `crates/server/tests/last_modified_monotonic.rs:297`).

| Gate / obligation | Verdict | Re-derived evidence |
|-------------------|---------|---------------------|
| C4-ci | NEEDS-HUMAN | Local rerun passed typos, docs lint/render/link audit, repository guards, fmt, clippy, build, 1,715 tests with 14 ignored, and machete, then stopped at the read-only advisory-cache lock (`review-checks.log:1`; `review-ci.log:3321`). Frozen CI instead reaches two rustls 0.21.12 advisory failures (`gate-logs/C4-ci.log:6596`; `gate-logs/C4-ci.log:6626`). Neither establishes a defect introduced by this patch; Cargo.lock is unchanged (`Cargo.lock:3294`). |
| C4-verify | PASS | Independent red→green agrees with the captured gate: three assertion failures before, five passing tests after (`review-red.log:305`; `review-green.log:16`; `gate-logs/C4-verify.log:52`). |
| C4-diff-cov | PASS | Independent instrumented run passed all five tests and hit all 11 added lines carrying LLVM line records, including both call sites and every helper branch (`review-coverage-summary.log:2`). The frozen coverage attempt itself measured nothing; it failed to compile against an older four-argument streaming API (`gate-logs/C4-diff-cov.log:36`). |
| C5-mutants | PASS | Independent diff-filtered mutation run matches the frozen result: 3 caught, 2 unviable, none surviving (`review-mutants-final.log:4`; `gate-logs/C5-mutants.log:13`). Mutation-only lint capping lets generated unused arguments compile; normal CI retained warnings-denied checks (`review-checks.log:6`). |
| T4-batch-review | PASS | Captured reviewer summary reports zero blocking, rejected, or dropped findings; the log contains the summary, not the underlying review narratives (`gate-logs/T4-batch-review.log:10`). This report supplies the independent source review. |
| T4-contribution | N/A | PR description is absent by design at Check; the substantive audit must rerun at publish (`gate-logs/T4-contribution.log:10`). |
| host-tikv | PASS | Frozen log records both requested clippy invocations completing; this establishes compilation, not live-service behavior (`gate-logs/host-tikv.log:2`; `gate-logs/host-tikv.log:211`). |
| Declared external dependencies | PASS | Both `typos` and the real docs renderer ran locally; 99 pages rendered and the internal-link audit passed (`brief.md:140`; `review-ci.log:2`; `review-ci.log:9`). |

The supplied target is usable and matches the patch. Its streaming API already accepts `declared_len` (`crates/gateway-core/src/lib.rs:329`), so the frozen coverage mismatch is an older coverage-checkout caveat, not a patch compilation failure. All source citations above resolve under `$PDCA_TARGET`; bundle and reviewer-log citations resolve in this directory. The target's original patch was restored, and both `git apply --reverse --check ../patch.diff` and `git diff --check` pass (`review-checks.log:7`).

Prior-art checking covered each affected file, not just the issue title: 23 metadata-file commits, no history for the new regression file, and 30 architecture-file commits. All 19 closed/unmerged PR file lists were inspected without pagination gaps. The only matching closed PR, [#647](https://github.com/getwyrd/wyrd/pull/647), was rejected for the size of its segmented-map change, not timestamp ordering; the existing metadata introduction is commit `68403eb3f61e` (`review-prior-art.log:2`).

Supplemental conformance passed all 11 vectors and the shared-state scanner passed (`review-conformance.log:1`; `review-statics.log:3`).

The supplemental `cargo xtask dst` run passed its clippy stage and earlier test suites, then was interrupted during the long-running existing custodian permanent-seed campaign. It is **incomplete**, not a green verification result (`review-dst.log:504`). This adds no demonstrated patch defect; a complete CI result remains owed under C4.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — issue #768 (last-modified-never-decreases)

The red→green holds on the production path. The patch implements the brief's rule exactly. The main refutation is about whether the fix does its job, not about the code. On this base every S3 surface shows `Last-Modified` in whole seconds, so the `+1 ms` bump almost never reaches the wire. In the issue's own skew scenario, the value clients see goes from "one second older" (before) to "identical" (after). It never shows as newer.

- NEEDS-HUMAN [human] — **The fix does not reach the wire, and for some sync rules it makes the issue's scenario worse.** `crates/core/src/metadata.rs:2272` commits `max(t, p + 1 ms)`. Both renderers truncate to seconds: `crates/gateway-s3/src/lib.rs:637` (listing, always `.000Z`) and `:3264` (header). The base already documents this at `docs/design/architecture/05-building-block-view.md:195-198`: #767 has landed. So the brief's statement "a listing on `main` renders milliseconds … `.751Z` after `.750Z`" is out of date here, and the bump is visible on **no** S3 surface unless `p` ends in `.999`.

  **Reproduced** in a scratch copy, on the real `S3Gateway` over signed HTTP:
  1. Seed `stamps/doc` with `p` = 2 s ahead of the wall clock, at `.500`.
  2. Read HEAD and ListObjectsV2.
  3. Overwrite through `Gateway::put_object` with different bytes of the same size.
  4. Read both surfaces again.

  | | HEAD and ListObjectsV2 | GET with `If-Modified-Since: <old header>` |
  |---|---|---|
  | With the fix | `08:31:29 GMT` / `…08:31:29.000Z` before **and** after | `304 Not Modified` |
  | Fix reverted | `08:31:40` before → `08:31:39` after | `304 Not Modified` |

  What this means:
  - Comparators that need "newer": the `If-Modified-Since` check at `gateway-s3/src/lib.rs:2862`, HTTP caches, and a "source newer than destination" rule. They miss the new content both before and after the fix, so the fix changes nothing for them.
  - Comparators that react to "different": aws-cli's documented download rule ("ignore same-sized items unless the local version is newer than the S3 version"; with `--exact-timestamps`, unless the timestamps differ). Before the fix they saw the local copy at `:40` as newer than S3's `:39` and downloaded again. After the fix they see `:40 == :40` and skip silently. That is the exact failure the Goal names.

  The tests cannot catch this. They read raw milliseconds through `head_object` (`crates/server/tests/last_modified_monotonic.rs:142-147`), and the brief says so on purpose. So they pass while the property users rely on fails. The Plan-review claim "the reported gateway defect is fully fixed" does not hold on this base.

  Fixing it means changing the maintainer-decided rule, for example bumping to the next whole second (`max(t, (p / 1000 + 1) * 1000)`). That rule has its own cost: a burst of same-second overwrites would build a lead of seconds, not milliseconds. Or the human accepts "never older on the wire, but often equal" and records that limit. Either way it is a design call.

- NEEDS-HUMAN [human] — **One bad clock reading now sticks to a file permanently. Before the fix the next good write healed it.** The rule has no cap (`crates/core/src/metadata.rs:2272`). The patch's own leg (1) shows a prior an hour ahead is carried forward exactly (`last_modified_monotonic.rs:136-151`).

  Concrete case: a gateway boots with its clock stepped to 2036 (a VM restored before NTP syncs, or a bad RTC) and serves one PUT of key `k`. From then on, every PUT of `k` from any accurate gateway records 2036 + n ms. A GET with `If-Unmodified-Since: <now>` returns `412` for that key until 2036 (`crates/gateway-s3/src/lib.rs:2847`). Before the fix, the next accurate overwrite healed the stamp. Now only delete + recreate clears it.

  The brief says the lead is "bounded by the largest clock skew in the fleet". That is not a bound: the brief itself says nothing limits skew on this path (ADR-0024 sets no limit here, and option 2 was rejected). Whether to cap the carry-forward (for example, ignore a prior that is more than X ahead of the publisher's clock, and log it) is a design decision.

- NEEDS-HUMAN [human] — **The red C4-ci gate is not caused by this patch.** It fails only at `cargo deny … check advisories`: RUSTSEC-2026-0346 and RUSTSEC-2026-0349 on `rustls 0.21.12` (`Cargo.lock:302`, pulled in through `tikv-client 0.4.0`; see `gate-logs/C4-ci.log:3530-3608`). The patch does not touch `Cargo.lock`. fmt, clippy, build and the full test run passed, including all five new tests (`C4-ci.log:2574-2581`, `:5633-5640`). This needs a waiver, or a dependency fix on `main`. It is not a builder iteration.

- **C4-diff-cov "executed 0 tests" is a harness misdiagnosis, not a property of the test.** The coverage worktree (`/home/eddie/wyrd/wyrd-cov`) failed to compile the test: `E0061`, it expects a 4-argument `put_object_streaming` (`gate-logs/C4-diff-cov.log`). This target has the 5-argument signature (`crates/gateway-core/src/lib.rs:330-337`), so that worktree is on an older base. The script then reported "compiled out". Coverage of the diff is not in doubt: the core leg calls both committers directly, and C5 caught 3 of 3 viable mutants.

- **Attempts that did not refute the fix:**
  - **Red→green, re-run locally.** On a scratch copy of `$PDCA_TARGET` with `metadata.rs` reverted: 3 fail (A1, A2, A5) and 2 pass (A3, A4 are guards). With the fix: 5 of 5 pass. Same panics and lines as `gate-logs/C4-verify.log`.
  - **Production path.** Both legs go through `Gateway` → `commit_written` (`crates/server/src/lib.rs:309-316`) → `write::commit_overwrite` → the leased committer (`metadata.rs:2330`). This is not a copy of the logic.
  - **Concurrency.** The stamp comes from the same `prior` the batch CAS-requires (compare-and-swap on the record's exact bytes), so a racing overwrite cannot order against a stale generation.
  - **Paths that could skip the helper.** Every S3 PUT enters `put_object_streaming` (`gateway-s3/src/lib.rs:1881-1908`). The CLI put is create-only (`cli.rs:2267`). Repairs keep the stamp through `..prior.clone()` (`metadata.rs:2164-2172`, `custodian/src/backfill.rs:239-246`).
  - **Edge cases.** `None` and saturation are pinned. The assertions are exact (`==`), not `>=`.
  - **Delete-then-recreate.** It starts again from the publisher's clock (`crates/core/src/write.rs:284`). That is outside the brief's overwrite scope, so I did not raise it.
<!-- pdca:leaf-complete -->

### Advisory — code-review

No findings on either advisory lens: introduced correctness bugs, or actionable reuse, simplification, and efficiency changes.

- Correctness: `crates/core/src/metadata.rs:2218` and `crates/core/src/metadata.rs:2330` derive the timestamp from the same prior record their respective CAS guards protect. The shared helper at `crates/core/src/metadata.rs:2269` preserves absent publisher timestamps and uses saturating arithmetic. Creation, repair metadata, and lease clocks remain unchanged.
- Reuse and test fidelity: both overwrite APIs share one constant-time helper. `crates/server/tests/last_modified_monotonic.rs:129` and `crates/server/tests/last_modified_monotonic.rs:155` exercise real gateway overwrites; `crates/server/tests/last_modified_monotonic.rs:297` checks exact values through both core APIs, including saturation, repeated writes, and the documented unstamped reset. No worthwhile simplification identified.

Frozen evidence: C4-verify reports five passing tests with the fix and three assertion failures without it; C5 reports three caught mutants and two unviable, with none surviving. C4-ci fails on rustls dependency advisories outside this diff. Diff coverage remains unmeasured: its log shows a streaming API argument-count compilation error, whereas the supplied target's signature at `crates/gateway-core/src/lib.rs:330` accepts the test's arguments. This evidence discrepancy is not a demonstrated defect in this patch. No builds were rerun and no target files were modified.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Arrange baseline dependency remediation and a complete CI rerun — the frozen gate fails on rustls advisories, while this sandbox cannot lock the advisory database; independent regression and coverage checks pass, but the supplemental DST campaign is incomplete (`review-dst.log:504`; `gate-logs/C4-ci.log:6596`; `gate-logs/C4-ci.log:6626`; `review-ci.log:3321`; `review-green.log:16`).
- [ ] Validation — fitness-to-purpose — Confirm the bounded timestamp contract meets the sync use case — an unstamped generation resets order, rapid writes accumulate clock lead, and whole-second wire timestamps can tie (`crates/server/tests/last_modified_monotonic.rs:319`; `crates/server/tests/last_modified_monotonic.rs:330`; `docs/design/architecture/05-building-block-view.md:197`).
- [ ] C4-ci — Local rerun passed typos, docs lint/render/link audit, repository guards, fmt, clippy, build, 1,715 tests with 14 ignored, and machete, then stopped at the read-only advisory-cache lock (`review-checks.log:1`; `review-ci.log:3321`). Frozen CI instead reaches two rustls 0.21.12 advisory failures (`gate-logs/C4-ci.log:6596`; `gate-logs/C4-ci.log:6626`). Neither establishes a defect introduced by this patch; Cargo.lock is unchanged (`Cargo.lock:3294`).
- [ ] **The fix does not reach the wire, and for some sync rules it makes the issue's scenario worse.** `crates/core/src/metadata.rs:2272` commits `max(t, p + 1 ms)`. Both renderers truncate to seconds: `crates/gateway-s3/src/lib.rs:637` (listing, always `.000Z`) and `:3264` (header). The base already documents this at `docs/design/architecture/05-building-block-view.md:195-198`: #767 has landed. So the brief's statement "a listing on `main` renders milliseconds … `.751Z` after `.750Z`" is out of date here, and the bump is visible on **no** S3 surface unless `p` ends in `.999`.
- [ ] **One bad clock reading now sticks to a file permanently. Before the fix the next good write healed it.** The rule has no cap (`crates/core/src/metadata.rs:2272`). The patch's own leg (1) shows a prior an hour ahead is carried forward exactly (`last_modified_monotonic.rs:136-151`).
- [ ] **The red C4-ci gate is not caused by this patch.** It fails only at `cargo deny … check advisories`: RUSTSEC-2026-0346 and RUSTSEC-2026-0349 on `rustls 0.21.12` (`Cargo.lock:302`, pulled in through `tikv-client 0.4.0`; see `gate-logs/C4-ci.log:3530-3608`). The patch does not touch `Cargo.lock`. fmt, clippy, build and the full test run passed, including all five new tests (`C4-ci.log:2574-2581`, `:5633-5640`). This needs a waiver, or a dependency fix on `main`. It is not a builder iteration.
- [ ] C4 diff coverage: changed lines executed by the patch's tests unverifiable — the -p wyrd-server --test last_modified_monotonic target executed 0 tests, so diff coverage was not measured
- [ ] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo deny --all-features --config deny-all-features.toml check advisories` failed with exit status: 1
- [ ] The unconditional ordering contract is contradicted by the planned handling of absent metadata. `brief.md:82-89` promises the invariant for every committed generation on both public overwrite APIs, and `brief.md:61-62` / `brief.md:211-212` require unqualified “never decreases across overwrites” documentation. But `brief.md:52-53` and `brief.md:165-168` require a publisher with `None` to clear the stamp and a prior with `None` to accept the next publisher's stamp unchanged. Counterexample under those rules: `Some(2000)` → an unstamped overwrite → `None` → an overwrite publishing `Some(1000)` → `Some(1000)`. The same file loses its ordering history and later exposes a lower timestamp. This is an admitted core API input, not malformed metadata: target `crates/core/src/write.rs:63-68` explicitly permits plans with no metadata, and `crates/core/src/metadata.rs:2218` / `:2297` store that optional value. Current gateway PUTs supply `Some` (`crates/server/src/lib.rs:254`, `:405`), so the chosen implementation can still fix the reported gateway defect. Revise the invariant and required docs to limit the guarantee to uninterrupted timestamp-bearing overwrites, explicitly documenting the reset across an unstamped publication; add the chained case to A5 to pin that boundary. If the unconditional core guarantee is intended instead, the `None` policy needs a design revision.

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
- Iteration delta (if iterating): Auto-iterate (round 1): rebuilding for the implementation-level findings — C4 Verification (red→green) — Arrange baseline dependency remediation and a complete CI rerun — the frozen gate fails on rustls advisories, while this sandbox cannot lock the advisory database; independent regression and coverage checks pass, but the supplemental DST campaign is incomplete (`review-dst.log:504`; `gate-logs/C4-ci.log:6596`; `gate-logs/C4-ci.log:6626`; `review-ci.log:3321`; `review-green.log:16`).; C4-ci — Local rerun passed typos, docs lint/render/link audit, repository guards, fmt, clippy, build, 1,715 tests with 14 ignored, and machete, then stopped at the read-only advisory-cache lock (`review-checks.log:1`; `review-ci.log:3321`). Frozen CI instead reaches two rustls 0.21.12 advisory failures (`gate-logs/C4-ci.log:6596`; `gate-logs/C4-ci.log:6626`). Neither establishes a defect introduced by this patch; Cargo.lock is unchanged (`Cargo.lock:3294`).; C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo deny --all-features --config deny-all-features.toml check advisories` failed with exit status: 1. Deferred, not addressed here: 4 finding(s) that need human judgment, held in deferred-findings.json for the human at handover — not build work
- By / date: auto-iterate / 2026-10-14

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 1 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
