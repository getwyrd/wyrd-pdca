# Result — issue 1061 / multipart-retirement-drain-records

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: Every multipart slice installs `retire:` obligations and nothing on `main` drains
  them. The `retire:records:` ones, the root flip's published parts (`{parts}`, #1008) and a
  rolled-back attempt's segments (`{seg}`, #1009 and the restore fence), leave `part:`, `psum:` and
  `seg:` records for ever. #1012's first attempt built a drain, and Check found three defects in it:
  (1) a pass's budget counted only commits, so obligations that could not progress cost nothing,
  one pass walked the whole ledger (a probe read 4,096 held obligations in 65 pages in one pass),
  and the per-pass bounds the docs promised had no test; (2) writing the advanced cursor back could
  push an obligation past `MAX_VALUE_BYTES`, the store refuses that for good (FoundationDB 2103),
  and the error ended the whole leg on every pass, so every later obligation starved; (3) finding
  the next member of a sparse part set rescanned the runs from the first one each time (O(N²)).
- Success criterion: A NEW test file `crates/custodian/tests/multipart_retirement_drain.rs`
  (the core-level parts, (K) and the Complete refusal in (W), MAY go inline in
  `crates/core/src/multipart.rs` or in a NEW `crates/core/tests/*.rs`), over `RedbMetadataStore::in_memory()`, driving the drain **only
  through `reconcile_step`** with the contexts the deployed loop passes
  (`crates/server/src/custodian.rs:397`), never a test-only entry. It shows all of:
  **(B-rec) Complete, then drain.** After a production subset Complete (#1008) and reconcile
  passes: no `part:` or `psum:` record of a published part remains; the drain wrote **no**
  `orphan:` record at all (`0016:1503`); the published object reads back byte-identical; the
  unnamed parts' records and their `retire:bytes:` obligation are byte-identical to before (that
  mode is child-2's).
  **(C) Convergence at scale.** A `retire:records:` `{parts}` obligation over **at least 4,001
  part numbers, no two adjacent** (one run per part; planted canonical `part:`/`psum:` records are
  fine), and a `{seg}` obligation over more `seg:` records than `MAX_BATCH_OPS`, drain to empty
  across steps. Every commit stays within `MAX_BATCH_BYTES` and `MAX_BATCH_OPS`, preconditions
  counted. The obligation's stored value changes every step (its cursor advanced); a drain resumed
  from fresh contexts (nothing in memory) continues from the stored cursor; the obligation is
  deleted only in the batch that deletes its last record; a second full drain is a no-op.
  **Combined payload:** a `retire:records:` `{parts, seg}` obligation (the row the decoder accepts,
  `crates/core/src/multipart.rs:3866`; no production writer installs it yet, so the fixture plants
  its canonical encoding), sized so its drain crosses from one component to the other **between
  two commits**. A drain resumed from fresh contexts exactly at that boundary continues into the
  second component; after the first component's last record is gone the obligation is still
  present, its cursor in the second component; it is deleted only with the second component's
  last record.
  **(L) Per-pass bounds, no starvation.** With a recording store wrapper counting reads and
  commits per pass, over a ledger at least three times the per-pass visit bound Do chooses:
  (L1) a productive backlog: every pass stays within fixed bounds on obligations visited, store
  reads and commits, whatever the ledger size; (L2) a held prefix: obligations that cannot progress
  (values that will not decode) sorting before one productive obligation: every pass stays within
  the same bounds, each held obligation stays byte-identical with a typed outcome and an audit
  event, and the productive obligation drains within a number of passes the test states from the
  bounds.
  **(W) Value ceiling.** Under a store wrapper that refuses any put over `MAX_VALUE_BYTES`, as
  FoundationDB does: an obligation installed within the installer ceiling (which now leaves the
  cursor's room) drains to empty; one planted just under `MAX_VALUE_BYTES` (e.g. 6,249 non-adjacent
  six-digit part numbers, 99,995 bytes) is held byte-identical with a typed outcome and an audit
  event, its over-ceiling put is never submitted, no pass returns `Err`, and a second obligation
  later in key order drains in the same passes. A Complete whose obligation would leave less than
  the cursor's room under `MAX_VALUE_BYTES` is refused where #1011 refuses one over the ceiling.
  **(G-rec) Restore between records-drain steps.** An `Aborting@E+1` session fenced from
  `Completing@E` with segments written (installed by the production restore fence, as
  `crates/custodian/tests/restore_completing_fence.rs` does) has its `retire:records:` `{seg}`
  obligation drained **one step at a time**. After every step the production
  `reconcile_after_restore` (`crates/custodian/src/restore.rs:597`) does not name the session (in
  particular no `NoDeleter`, `restore.rs:1176`) until that obligation is gone.
  **(K) Cursor codec.** An obligation without a cursor encodes byte-identical to today (no
  installer's bytes change); a cursor round-trips exactly. Decode refuses, with a typed error,
  every cursor it can judge from the key and value it is given: one on a component the payload
  does not carry; one past the end of work the payload itself lists (a part number past the
  `{parts}` set's last member; an index past a `{chunks}` list or a flat `{generation}` list); a
  segment index past the key grammar's bound `MAX_SEGMENT_INDEX`
  (`crates/core/src/metadata.rs:536`). A `{seg}` payload names only the group's nonce and epoch
  (`SegmentGroup`, `metadata.rs:1052-1055`), so decode **cannot** know where the group's `seg:`
  range ends, and (K) does not ask it to. That end is the drain's to find, and (C) tests it: a
  `{seg}` component is finished only when a read of the group's **whole** `seg:` range, from its
  start, finds it empty. A planted `{seg}` cursor past the last record, with records still below
  it, is a typed hold (obligation byte-identical, audit event), never a finish; this half of (K)
  is asserted through `reconcile_step`, in the custodian test file.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: the optional `cursor` component inside `RetirePayload`, decoded against its payload,
  with its maximum encoded size reserved under `MAX_VALUE_BYTES` in the ceiling installed
  obligations are held to; the retirement leg dispatched from `reconcile_step` in the deployed
  custodian; the `retire:` ledger walk with per-pass bounds on obligations visited, reads and
  commits, resuming past obligations that cannot progress; the drain step for `retire:records:`
  (`{parts}`: each member's `part:` and `psum:`; `{seg}`: the group's `seg:` records; both
  together), delete only, never mark; typed holds with audit events for an obligation that will not
  decode and one whose advanced value would cross the ceiling; next-member lookup over a part set
  that does not rescan from the first run; the `wyrd-metadata-redb` dev-dependency of
  `wyrd-custodian` and its `Cargo.lock` line (approved 2026-11-11); the docs. / out of scope:
  - every `retire:bytes:` obligation, in any shape: this child leaves each byte-identical. Marking,
    the mark guard and its `reclaiming` check, the stamp, validated placements and their hold, the
    owned-residue hold, segments-before-parts, the DST row and the `deferred: #659` marker at
    `restore.rs:1156` are child-2's;
  - the owned `sidx:` walk and the terminal session delete (#1013); the orphan-identity gate and
    unlink/supersede routing (#1014); the reaper (#625); the operator abort (#633); concurrent
    drainers (#660); the S3 verbs (#668–#670);
  - any edit to proposal 0016 or an ADR; `deny*.toml`; any `Cargo.lock` change beyond the
    dev-dependency line.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: unverifiable —                why this slice has no isolable red (the cargo output is above).
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 93.0% — 372 of 400 instrumentable changed lines executed (floor 80%); 400 of 1045 changed lines were instr
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 173 mutants tested in 37m: 7 missed, 136 caught, 28 unviable, 2 timeouts

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1061/review-
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.56s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Advisory review of #1061’s bounded, resumable, delete-only multipart retirement drain: no confirmed correctness defect; behavioral red→green and the unpublished prior-art disposition remain human decisions.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The record-reclamation boundary and observable success conditions are explicit, including held-prefix fairness and combined-component completion; `brief.md:28`, `docs/design/proposals/draft/0016-multipart-commit-protocol.md:613`, `docs/design/proposals/draft/0016-multipart-commit-protocol.md:1081`. |
| C2 Reproduction (red pre-fix) | NEEDS-HUMAN | Accept the declared net-new baseline exception — independently stashing the fix produced 15 compilation errors and no executed discriminator, so no behavioral red was established; `brief.md:151`, `review-red.log:3`, `review-red.log:122`. |
| C3 Change | PASS | The patch stays within the approved records-mode slice and preserves bytes-mode obligations and published content in the exercised production path; `crates/custodian/tests/multipart_retirement_drain.rs:703`, `crates/custodian/tests/multipart_retirement_drain.rs:732`, `crates/custodian/src/reconciliation.rs:166`. |
| C4 Verification (red→green) | NEEDS-HUMAN | Decide whether the declared verification exception is discharged by 35 independently passing tests and frozen gate evidence — green is reproduced, but the behavioral baseline and all nine specifically requested fault-injection reds are not demonstrated by the supplied logs; `review-restored-green.log:18`, `review-restored-green.log:30`, `review-restored-green.log:55`, `brief.md:155`. |
| C5 Causal adequacy | PASS | The missing durable worker and the prior convergence failures are addressed directly; restart, short-page, held-prefix, and false-certification regressions pass, with no capability-probe workaround; `crates/custodian/tests/multipart_retirement_drain.rs:916`, `crates/custodian/tests/multipart_retirement_drain.rs:988`, `crates/custodian/tests/multipart_retirement_drain.rs:1296`, `crates/custodian/tests/multipart_retirement_drain.rs:1372`. |
| T1 Structure | PASS | Production remains behind the metadata trait and existing reconstruction context; shared page validation avoids a divergent scanner, and the concrete backend dependency is test-only; `crates/custodian/src/retirement.rs:95`, `crates/custodian/src/gc.rs:2067`, `crates/custodian/Cargo.toml:61`. |
| T2 Shape | PASS | The 13-file change remains the accepted fix-round scope; 2,471 of 3,525 added lines are tests, and the three carry-forward defects have focused regressions; `brief.md:281`, `crates/custodian/tests/multipart_retirement_drain.rs:988`, `crates/custodian/tests/multipart_retirement_drain.rs:1270`, `crates/custodian/tests/multipart_retirement_drain.rs:1412`. |
| T3 Runtime | PASS | Fixed visit/read/commit budgets and persisted progress withstand productive and held backlogs; backend-owned deadlines discharge the alleged indefinite metadata wait; `crates/custodian/src/retirement.rs:243`, `crates/custodian/tests/multipart_retirement_drain.rs:1098`, `crates/traits/src/lib.rs:1334`, `crates/metadata-tikv/src/lib.rs:1363`, `crates/metadata-fdb/src/lib.rs:1380`. |
| T4 Contribution | NEEDS-HUMAN | Confirm that the brief fully captures unpublished #1012’s rejected work — affected-path merged-history and closed-work searches ran, but cannot independently recover that unpublished attempt; living docs are current and contribution-artifact checking is deferred to publish; `brief.md:210`, `review-path-history.json:1`, `review-closed-paths.json:1`, `gate-logs/T4-contribution.log:10`. |
| T5 Judgment | PASS | The accepted split and tracked deferrals remain intact; the new decode checks preserve canonical bytes, and records retirement neither adds a clock lifecycle nor takes on marking/concurrent-drainer scope; `crates/core/tests/multipart_retire_cursor.rs:148`, `crates/core/tests/multipart_retire_cursor.rs:428`, `crates/custodian/src/retirement.rs:203`. |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether this delete-only release meets rollout needs while bytes-mode work remains queued and downgrade restore reports become noisy — automated convergence checks cannot decide that operational trade; `docs/design/architecture/06-runtime-view.md:88`. |

Source citations above are relative to `$PDCA_TARGET` (`target/`); brief, log, and investigation citations are relative to this report. The target contained the required Complete/restore/value-ceiling prerequisites. After the stash/pop comparison, `git apply --reverse --check ../patch.diff` succeeded and the supplied source changes were restored.

**The timeout finding is unsupported by the actual metadata seam.** The frozen `T4-batch-review` log reports an indefinite wait at `crates/custodian/src/retirement.rs:351` (`gate-logs/T4-batch-review.log:10`). However, `MetadataStore` explicitly makes bounded waiting the backend’s responsibility (`crates/traits/src/lib.rs:1334`). TiKV bounds reads and scans with `under_deadline` and commits with `tokio::time::timeout` (`crates/metadata-tikv/src/lib.rs:1156`, `crates/metadata-tikv/src/lib.rs:1307`, `crates/metadata-tikv/src/lib.rs:1363`). FoundationDB sets a transaction timeout in the shared transaction constructor, including the transactions used by paged reads and commits (`crates/metadata-fdb/src/lib.rs:1378`, `crates/metadata-fdb/src/lib.rs:1921`, `crates/metadata-fdb/src/lib.rs:1482`). The embedded backend’s termination model is documented at `crates/metadata-redb/src/lib.rs:23`. A permanently pending test double would violate this seam. Requiring another timeout in the drain is therefore not substantiated as a patch defect. This records a rejection reason; it does not change the frozen gate’s failure status.

**Independent execution confirms green behavior, with two distinct verification limits.** I ran `cargo test --offline --locked -p wyrd-core --test multipart_retire_cursor --test multipart_complete_value_ceiling -p wyrd-custodian --test multipart_retirement_drain`, stashed the tracked fix, ran the two new discriminator files against the base, popped the stash, and reran the green command. The restored run passed 10 ceiling tests, 6 codec tests, and 19 drain tests (`review-restored-green.log:18`, `:30`, `:55`). The base run failed compilation before any test executed (`review-red.log:122`), matching `gate-logs/C4-verify.log:140`; that is not a behavioral red. The mutation log establishes 136 caught mutations and two timeouts, but does not identify the caught mutations sufficiently to certify every deliberate red listed in `brief.md:155`.

The independent `cargo xtask ci` run completed with two failures caused by the disposable target’s absent `.github/advisory-db` gitlink: the existing index assertion and advisory-database preparation (`review-ci.log:3708`, `review-ci.log:4353`). `git ls-files -s -- .github/advisory-db` returned no entry. This is a target-state limitation, not a patch defect; neither path is changed by the patch. Formatting, clippy, build, conformance, repository guards, and DST completed, but this rerun is not an all-green CI result. The frozen CI log explicitly shows the pinned advisory checks succeeding and ends with all checks passed (`gate-logs/C4-ci.log:4265`, `gate-logs/C4-ci.log:4276`, `gate-logs/C4-ci.log:4909`). Spellcheck and docs lint/render were also rerun separately; rendering produced 101 pages and passed the link audit (`review-docs-lint.log:1`, `review-docs-render.log:2`). The named `typos` and renderer dependencies were exercised.

**The seven mutation survivors do not establish a correctness failure in this slice.** The frozen campaign lists five byte-budget arithmetic survivors, one segment-page comparison survivor, and omission of successful-step telemetry (`gate-logs/C5-mutants.log:15`). The arithmetic changes leave the operation cap dominant for these fixed-width delete keys and values bounded by 100,000 bytes; a maximal records step remains far below the 5,000,000-byte ceiling (`crates/custodian/src/retirement.rs:447`, `crates/custodian/src/retirement.rs:478`, `crates/core/src/multipart.rs:5874`). Changing `records.len() > take` to `>= take` can defer one last segment to another step but still preserves it under an advanced cursor (`crates/custodian/src/retirement.rs:544`). Removing `emit_step` exposes unasserted success counters, not a missing reclamation assertion (`crates/custodian/src/retirement.rs:619`). These are recorded dispositions, not a claim that the mutation gate passed. The independent source/test review also confirms the earlier short-page, resumed-pass, conflict-exhaustion, and literal hold-reason corrections.

**The remaining frozen evidence is usable within its stated scope.** `C4-diff-cov` reports 372/400 instrumentable changed lines executed (93.0%), with 645 changed lines unscored (`gate-logs/C4-diff-cov.log:109`). `host-tikv` compiled both the backend and server feature selections (`gate-logs/host-tikv.log:2`, `gate-logs/host-tikv.log:211`); it is not a live-cluster exercise. `T4-contribution` is **N/A at Check**: `pr-description.md` is intentionally absent and the substantive audit must rerun at publish (`gate-logs/T4-contribution.log:10`). The instance-scoped coverage, mutation, and batch-review wrappers were adjudicated from their frozen logs; their absence from the target is not a finding.

**Prior-art searches covered every affected path, with an explicit unpublished-work limit.** Main-branch path histories and closed issue/PR searches are saved in `review-path-history.json` and `review-closed-paths.json`. All three new source/test paths have no main history or closed-path search matches. The closed, not-planned [#636](https://github.com/getwyrd/wyrd/issues/636) records the repeated-first-batch and premature-completion failures; the current convergence tests exercise those cases. [#1012](https://github.com/getwyrd/wyrd/issues/1012) and the brief describe the prior attempt, but its unpublished artifact is unavailable here; the T4 decision asks the human to confirm that inventory, not to repeat the searches. The tracked DST/marking and concurrent-drainer deferrals to #1062/#660 are settled. Tier-1/Tier-2 backend-fault follow-up remains useful for deployed custodian durability, without reopening those accepted deferrals.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — #1061 retirement drain (records mode), iteration 2

**Bottom line:** I could not break the drain. I re-ran the new tests and three mutations (small deliberate code changes used to check that a test catches them) against the fixes from last round, and each mutation turned a named test red. One weak test is left, for the rescan fix (defect 3). The gating T4 failure looks like a false positive under the repo's own store contract.

Toolchain was available. I built a scratch copy of `$PDCA_TARGET` and ran `cargo test -p wyrd-custodian --test multipart_retirement_drain` (19/19 pass) and `-p wyrd-core --test multipart_retire_cursor` (6/6 pass). C4-verify is UNVERIFIABLE, which the human accepted in advance. The mutations below act as a partial red leg in its place.

## Findings

- NEEDS-HUMAN [impl] — `crates/core/src/multipart.rs:3518`: no test catches a return of defect 3 (the O(N²) rescan of the part set). I replaced the binary search with a linear scan from the first run: `self.0.iter().position(|&(_, hi)| hi >= from)`. That is exactly the first attempt's shape. All 6 core tests and all 19 drain tests still passed. `members_from_starts_at_the_first_member_at_or_after_its_position` (`crates/core/tests/multipart_retire_cursor.rs:494-523`) only checks results. Its 500,000-run set does 6 lookups, which costs microseconds either way. A cheap, deterministic fix: look up every position of that 500,000-run set (or drain a `{parts}` obligation over it through `reconcile_step`). With the binary search that takes about 10 M comparisons. A linear scan takes about 10¹¹ and would never finish within the test timeout. Low severity: the production code is correct today, and the brief listed no demonstrated red for this defect.

- NEEDS-HUMAN [human] — The gating T4 blocker (`gate-logs/T4-batch-review.log`, citing `crates/custodian/src/retirement.rs:351`, "awaits … without a timeout") conflicts with the store seam's own contract. `crates/traits/src/lib.rs:1333-1342` makes termination the backend's job: "every operation terminates — a backend must bound its own waiting". FoundationDB puts a deadline on every transaction (`crates/metadata-fdb/src/lib.rs:78-89`), and redb has no network to wait on. No other custodian leg wraps store calls in a timeout (`gc.rs`, `reconstruction.rs`, `scrub.rs`: none). Adding one here would be the odd one out. It would also turn an undetermined commit into an extra failure mode that the step's `require(prior)` already covers. Recommend the human record a rejection with that reason (rubric, *Definition of done*), not send it back to Do.

## Attempted, could not refute

- **Short pages from the store (last round's defect 1).** `retirement.rs:544-559` now reads one row past a page the store cut short, and never treats `more == Some` as the end. Mutation: I disabled the read-ahead branch at `:546`, and `c_a_seg_obligation_drains_to_empty_on_a_store_that_cuts_pages_short` went red. I also walked these cases by hand: an empty group after `{parts}`, a page of exactly `take` rows with `next = None`, `entry > 0` over an empty range, and a stray non-canonical key. Each finishes, advances or holds correctly. A `seg:` record can't be written behind the drain, because segment batches require `mpu == Completing@E` (`crates/core/src/staged_publication.rs:20`) and the fence has already moved the session.
- **False `Satisfied` (last round's defect 2).** Mutation `saw_whole_ledger = reached_end` (dropping `from_head`, `retirement.rs:293`): `s_a_pass_that_resumed_part_way_does_not_certify_the_ledger` and `l1_a_productive_backlog…` went red. Mutation "`owed` never set" (`:329`): `s_a_pass_whose_every_step_lost_its_precondition_does_not_certify` went red. A contended obligation spends at most one pass's steps per lap, and the walk resumes after it (`:257-264`), so it can't starve the rest.
- **The value ceiling and the installers.** I suspected the Complete's `{generation}` obligation (`crates/core/src/multipart/complete.rs:1904-1911`) could land between 99,931 and 100,000 bytes, because `past_ceiling` (`complete.rs:1200`) measures only the two part-set obligations. That turned out to be wrong: a flat map is capped at `MAX_MAP_CHUNKS` = 158 refs, at most V/2 (`multipart.rs:5759-5764`, where V/2 means half of `MAX_VALUE_BYTES`), so the "at most half" claim at `multipart.rs:4077-4078` holds. Small doc nit only: its cite `metadata.rs:329-352` points at orphan-mark decoding, not the V/2 rule. The same stale cite is already at `multipart.rs:5738` and `:5865`.
- **Batch bounds.** `{parts}`: 1 precondition + 249×2 deletes + 1 put = 500 ops. `{seg}`: 1 + 498 + 1 = 500 ops. `MAX_BATCH_OPS` is 500, so the ops limit always binds first. The C5 survivors at `retirement.rs:447`, `:456` and `:477` only touch the byte budget, which never binds (about 50 KB of keys against 5 MB), so they make no observable difference. The survivor at `:544` (`>` → `>=`) costs one extra step at a page boundary and stays correct. The one at `:619` means the `emit_step` counters have no test. None of these is a correctness gap.

## Notes (no action needed)

- `Satisfied` is only possible when the whole ledger fits in one pass (`retirement.rs:239`, `:293`). Once the `retire:` ledger holds **64 or more** entries of any mode, including the `retire:bytes:` ones this child never drains, every reconstruction pass reports `Partial` for good. At exactly 64 it also rewrites `custodian:retire-cursor` every pass. This is the rule the human picked last round, and the deployed loop ignores the outcome (`crates/server/src/custodian.rs:616`). Worth remembering for any harness that drives "until `Satisfied`" once #508 creates sessions.
- Tier-0 DST coverage for this destructive path is deferred in code (`retirement.rs:203-207`, `// deferred: #1062`, `#660`). Per the rubric that is settled, so I did not raise it.

<!-- pdca:leaf-complete -->

### Advisory — code-review

No findings: the diff is clean on both advisory lenses—patch-introduced correctness bugs and actionable reuse, simplification, or efficiency issues.

The frozen T4 timeout finding against `crates/custodian/src/retirement.rs:351` is not substantiated. `MetadataStore` explicitly assigns bounded waiting to its implementations (`crates/traits/src/lib.rs:1333`). FoundationDB sets a transaction timeout (`crates/metadata-fdb/src/lib.rs:1380`); TiKV bounds reads, paged scans, and commits (`crates/metadata-tikv/src/lib.rs:1156`, `crates/metadata-tikv/src/lib.rs:1307`, `crates/metadata-tikv/src/lib.rs:1363`). The embedded redb adapter satisfies the same contract without network waiting (`crates/metadata-redb/src/lib.rs:23`). These calls do not require a second timeout at the drain to satisfy that contract.

Reviewed the supplied patch, target source, and frozen gate evidence, including surviving mutations. Checks were not rerun; the target was not modified.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C2 Reproduction (red pre-fix) — Accept the declared net-new baseline exception — independently stashing the fix produced 15 compilation errors and no executed discriminator, so no behavioral red was established; `brief.md:151`, `review-red.log:3`, `review-red.log:122`.
- [x] C4 Verification (red→green) — Decide whether the declared verification exception is discharged by 35 independently passing tests and frozen gate evidence — green is reproduced, but the behavioral baseline and all nine specifically requested fault-injection reds are not demonstrated by the supplied logs; `review-restored-green.log:18`, `review-restored-green.log:30`, `review-restored-green.log:55`, `brief.md:155`.
- [x] T4 Contribution — Confirm that the brief fully captures unpublished #1012’s rejected work — affected-path merged-history and closed-work searches ran, but cannot independently recover that unpublished attempt; living docs are current and contribution-artifact checking is deferred to publish; `brief.md:210`, `review-path-history.json:1`, `review-closed-paths.json:1`, `gate-logs/T4-contribution.log:10`.
- [x] Validation — fitness-to-purpose — Decide whether this delete-only release meets rollout needs while bytes-mode work remains queued and downgrade restore reports become noisy — automated convergence checks cannot decide that operational trade; `docs/design/architecture/06-runtime-view.md:88`.
- [x] `crates/core/src/multipart.rs:3518`: no test catches a return of defect 3 (the O(N²) rescan of the part set). I replaced the binary search with a linear scan from the first run: `self.0.iter().position(|&(_, hi)| hi >= from)`. That is exactly the first attempt's shape. All 6 core tests and all 19 drain tests still passed. `members_from_starts_at_the_first_member_at_or_after_its_position` (`crates/core/tests/multipart_retire_cursor.rs:494-523`) only checks results. Its 500,000-run set does 6 lookups, which costs microseconds either way. A cheap, deterministic fix: look up every position of that 500,000-run set (or drain a `{parts}` obligation over it through `reconcile_step`). With the binary search that takes about 10 M comparisons. A linear scan takes about 10¹¹ and would never finish within the test timeout. Low severity: the production code is correct today, and the brief listed no demonstrated red for this defect.
- [x] The gating T4 blocker (`gate-logs/T4-batch-review.log`, citing `crates/custodian/src/retirement.rs:351`, "awaits … without a timeout") conflicts with the store seam's own contract. `crates/traits/src/lib.rs:1333-1342` makes termination the backend's job: "every operation terminates — a backend must bound its own waiting". FoundationDB puts a deadline on every transaction (`crates/metadata-fdb/src/lib.rs:78-89`), and redb has no network to wait on. No other custodian leg wraps store calls in a timeout (`gc.rs`, `reconstruction.rs`, `scrub.rs`: none). Adding one here would be the odd one out. It would also turn an undetermined commit into an extra failure mode that the step's `require(prior)` already covers. Recommend the human record a rejection with that reason (rubric, *Definition of done*), not send it back to Do.
- [x] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [x] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_1061/review-
- [x] **(K) requires an end-of-payload check that the segment payload cannot support.** `brief.md:66` and `brief.md:222` require `decode_retire_obligation` to reject a cursor past its payload's end; `brief.md:212` makes the segment cursor the next segment index. But `RetirePayload.seg` holds only a `SegmentGroup` (`crates/core/src/multipart.rs:3822`), whose fields are nonce and epoch, with no count or last index (`crates/core/src/metadata.rs:1052`). Decode receives only key/value bytes (`crates/core/src/multipart.rs:4081`). Identical payload/cursor bytes cannot distinguish an index beyond the actual segment range from one within it. Revise (K) to distinguish payload-checkable bounds from segment key-space bounds (`metadata.rs:536`), and require actual range exhaustion to be checked by the drain. Otherwise Do must weaken the criterion silently or add an unstated persisted extent field.
- [x] **The criteria miss the combined obligation's component transition.** Scope expressly includes parts and segments “both together” (`brief.md:109`); the target codec likewise permits one `{parts, seg}` obligation (`crates/core/src/multipart.rs:3774`). Yet (C) specifies separate `{parts}` and `{seg}` obligations (`brief.md:38`), while (B-rec) and (G-rec) also exercise single-component obligations. A drain that deletes the obligation upon finishing its first component can satisfy these fixtures while stranding the second component's records. Add a combined-payload fixture that crosses the component boundary across commits, resumes with fresh contexts there, and proves the obligation survives until both components are empty; require the premature-deletion mutation to fail this case.
- [x] size backstop — this slice is behaving oversized: patch is 209 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: merged-wider
- Iteration delta (if iterating):
- By / date: Eduard Ralph / 2026-11-10

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 2 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
- Add a timing-based regression test for #1061 defect 3 (part-set next-member lookup): look up every position of a large sparse part set (or drain a `{parts}` obligation over it via reconcile_step) so a linear rescan from the first run times out; today no test catches a return of the O(N²) shape (`crates/core/src/multipart.rs:3518`).
