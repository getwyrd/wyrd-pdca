# Result — issue 842 / restore-fence-completing

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: after child-3, restore names a resurrected `Completing` session but does not
  fence it. That session can still publish over reclaimed bytes (D-B, `0016:717-728`), and the
  `seg:<nonce>:<E>:*` records its attempt already wrote have no deleter anywhere (X57,
  `0016:880`). 0016's `Completing → Aborting` restore fence is one batch: the session goes to
  `Aborting@E+1`, with its bytes obligation **and** `retire:records:{seg:<g>:<E>}` for the
  segments that attempt wrote (`0016:665`, `:2193`).
  One payload 0016 names for that row cannot be written. `{session, parts: <set>}` for a sparse
  part set exceeds the store's value ceiling: 10,000 alternating part numbers (1, 3, …, 19,999)
  encode to 128,916 bytes against `MAX_VALUE_BYTES` = 100,000
  (`crates/core/src/metadata.rs:549`), though 0016 claims they fit (`0016:382-384`). #809
  iteration 1 measured it. FoundationDB refuses the whole batch permanently
  (`2103 value_too_large`), so every run fails and every session after it stays unfenced.
  **Decided at Plan (2026-09-29, the human, option (a)):** the restore fence's bytes obligation
  for a `Completing` session is `{session, all}`, as for `Open`. It owes the same records: no part
  can commit outside `Open` (`crates/core/src/multipart.rs:4236-4250`, `0016:1030`), so the part
  set is frozen from the Complete fence on. And its size does not grow with the part count.
  Rejected: (b) keeping `{session, parts}` and naming an oversized session instead of fencing it,
  which leaves that session able to publish.
- Success criterion: the NEW file `crates/custodian/tests/restore_completing_fence.rs`
  passes over in-memory doubles, calling the production `reconcile_after_restore`. Records are
  seeded as raw JSON, and every session record carries `segment_nonce` in child-2's spelling
  (immediately after `clock_source`, in every state). The attempt's group is `(that nonce, E)`,
  where `E` is `publish_target.epoch`. Legs:
  **(G) A `Completing` session is fenced with its segments' deleter.** A `Completing@E` session
  with `segments_written > 0`, its nonce on the record, and `seg:<nonce>:<E>:*` records present
  ends as `Aborting@E+1`. **One** commit installs `retire:bytes:s:<id>:<E>` `{session, all}` and
  `retire:records:s:<id>:<E>` naming exactly the group `(nonce, E)`. Both decode through
  `decode_retire_obligation` against their keys, and the records obligation's `segments()` names
  that group. The records obligation is installed even when `segments_written` is 0: 0016's row
  is "1 put" (`0016:665`), and a damaged cursor must not decide whether records get a deleter.
  Atomicity works as in child-3's F-atomic: a double failing the commit that carries any one of
  the three writes leaves none of them. Proving that draining empties the range is #659's job.
  **(G-collision) Neither obligation overwrites one already there.** Both obligation keys are
  installed under `require_absent` (`0016:369-373`, `:675-676`; `multipart.rs:1409-1414`), as
  child-3's F-collision already requires for the bytes key. Seed a `Completing@E` session beside a
  decodable `retire:records:s:<id>:<E>` naming a **different** segment group. After the pass,
  that obligation is byte-identical, none of the three fence writes landed (the session is still
  `Completing@E`, and no `retire:bytes:s:<id>:<E>` exists), the session is named as needing a
  human with child-3's "key taken" cause, and an `Open` session whose key sorts after it is
  still fenced.
  **(G-sparse) The fence fits the store's value ceiling whatever the part count.** Use a store
  double that refuses any value larger than `MAX_VALUE_BYTES`, as FoundationDB does. A
  `Completing` session holding 10,000 parts numbered 1, 3, …, 19,999 is fenced as in G, every
  value the pass writes is at most `MAX_VALUE_BYTES`, and an `Open` session whose key sorts after
  it is fenced too.
  **(H) What cannot be fenced cleanly is never passed off as done.**
  (i) A `Completing` record with **no** nonce (the shape before child-2) fails decode. Restore
  leaves it byte-identical (ADR-0045) and names it as needing a human.
  (ii) A `Completing` session whose `seg:` records name a chunk that none of its `part:` records
  holds is still fenced, and still named as needing a human.
  (iv) A `Completing` session with one `seg:<nonce>:<E>:*` value that will not decode is still
  fenced and named as needing a human. The records obligation would delete that record without
  marking whatever chunks it named.
  (v) A key under the group's `seg:` range that is not a well-formed segment key of that group
  is named the same way.
  (vi) A decodable `Completing` session at epoch `u64::MAX` (its `publish_target.epoch` equal,
  as decode requires, `multipart.rs:2224-2229`; decode does not refuse that epoch) has no
  `E+1`. Child-3's `u64::MAX` guard (its H(ii)) applies to `Completing` too: no transition, no
  `retire:` key for it, no wrap and no panic, the record byte-identical, the session named as
  needing a human, and an `Open` session whose key sorts after it still fenced.
  In every case `needs_human()` is true.
  **(K) A second pass is idempotent and still names what needs a human.** Re-running over the
  fenced store leaves it byte-identical, and a session named under H(ii), H(iv) or H(v) on the
  first pass is named again. "Already `Aborting`" never means "nothing to report"; #664
  iteration 1 got this wrong. A fact Do can rely on: for a session at `Aborting@E'`, only a
  `Completing → Aborting` fence files `retire:records:s:<id>:<E'-1>`. A rollback files at the
  epoch it leaves and lands in `Open@E+1` (`0016:2196`).
  **(Order)** child-3's P3 still holds with a `Completing` session in the store: the fence runs
  after Pass 3.
  **(L) `cargo xtask ci` green.**
- Repo + branch target: getwyrd/wyrd @ main
- Scope: the `Completing` arm of the restore fence: one commit per session installing the
  bytes and records obligations beside the transition. Reading the attempt's `seg:` range to name
  what cannot be fenced cleanly (H). The second-pass re-check (K). Update `multipart.rs`'s retire
  rows table (`:3141-3150`) so the restore `Completing` fence is listed under `{session, all}`,
  with the reason. Extend the fence paragraph child-3 wrote in `06-runtime-view.md` §6.5 and the
  m4 blueprint's step 7 to `Completing` sessions. A `deferred: #659` marker belongs wherever the
  re-check could misread a range the future drain has half-deleted. Size budget: at most 7 files
  and under 80 KB of diff. / out of scope: the `Open` fence, and report fields beyond what G/H
  need (child-3); DST (child-5; the same rule as child-3 for `crates/dst/tests/custodian.rs`); the
  `{session, parts}` rows of other writers; #810; #659; #656, #658; #508; any edit to 0016 or an
  ADR.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (7 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage not measured — patch.diff does not apply on origin/main
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 37 mutants tested in 4m: 9 caught, 28 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_842/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.13s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #842: the patch atomically fences restored `Completing` uploads with both cleanup obligations and preserves damage warnings on reruns; independent red→green checks support the implementation.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The brief defines observable fence, collision, size, damage, rerun and ordering outcomes, including the approved constant-size `{session, all}` decision (`brief.md:22`, `brief.md:30`, `brief.md:114`). |
| C2 Reproduction (red pre-fix) | PASS | With tracked changes stashed and the new regression file retained, all seven tests compiled and failed by assertion, establishing the missing fence on the actual base (`reviewer-evidence/red.log:143`, `reviewer-evidence/red.log:211`). |
| C3 Change | PASS | Publication is fenced only with both cleanup obligations in the same conditional commit; existing obligations cannot be overwritten, and damaged sessions remain named (`target/crates/custodian/src/restore.rs:824`, `target/crates/custodian/src/restore.rs:949`). |
| C4 Verification (red→green) | PASS | Restoring the patch passes all seven Completing and nine Open regressions; workspace checks and the frozen complete CI support correctness, with the local advisory-cache and coverage-baseline limitations recorded below (`reviewer-evidence/green.log:17`, `reviewer-evidence/green.log:32`, `gate-logs/C4-ci.log:3917`). |
| C5 Causal adequacy | PASS | The tests exercise the unsafe publication/cleanup gap directly, including sparse values, both collisions, later pages, distinct nonces and repeated damage warnings; no capability-probe symptom guard was added (`target/crates/custodian/tests/restore_completing_fence.rs:304`, `target/crates/custodian/tests/restore_completing_fence.rs:477`, `target/crates/custodian/tests/restore_completing_fence.rs:591`). |
| T1 Structure | PASS | Core owns validated key/payload construction and custodian stays on the metadata trait seam; contextual damage reporting preserves malformed stored bytes (`target/crates/core/src/multipart.rs:2291`, `target/crates/core/src/multipart.rs:3665`, `target/crates/custodian/src/restore.rs:886`). |
| T2 Shape | PASS | Seven files, 81,816 bytes (79.90 KiB), cover the specified slice and update the public API/operator documentation; the Open-test adjustment reflects its now-supported Completing case (`reviewer-evidence/grounding.log:19`, `target/docs/design/architecture/06-runtime-view.md:65`, `target/crates/custodian/tests/restore_open_fence.rs:675`). |
| T3 Runtime | PASS | Metadata waits inherit backend deadlines, scans page through both ranges, and fence errors retain an incomplete audit verdict; existing clock ownership is unchanged (`target/crates/traits/src/lib.rs:1334`, `target/crates/custodian/src/restore.rs:775`, `target/crates/custodian/src/restore.rs:991`, `target/crates/custodian/src/restore.rs:1023`). |
| T4 Contribution | N/A | Contribution drafts are intentionally produced after Check; their substantive audit must run at publish, so absence here is not a finding (`gate-logs/T4-contribution.log:10`). |
| T5 Judgment | NEEDS-HUMAN | Decide whether the recorded path-based prior-art check remains sufficient at sign-off — this target has only a synthetic base and no remote, so subsequent merged or closed/rejected work cannot be independently excluded (`brief.md:145`, `reviewer-evidence/grounding.log:14`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept the writers-stopped restore procedure and operator handling of damaged records before drains resume — regression tests establish the metadata behavior, not operational disaster-recovery fitness (`target/docs/design/architecture/m4-first-deployment-blueprint.md:581`, `target/crates/server/src/cli.rs:1409`). |

No implementation defect was found. The following evidence limits qualify that conclusion; this review remains advisory.

- **Independent execution:** stashing restored the pre-fix tree; popping restored the patch without conflict. Seven regression tests failed by assertion before the fix and all sixteen fence tests passed afterwards. `git apply --reverse --check ../patch.diff` and `git diff --check` passed after restoration (`reviewer-evidence/grounding.log:10`). The broader run passed typos, docs lint/render with link audit, repository guards, formatting, Clippy, workspace build/tests and cargo-machete. Separate conformance, statics, DST and both TiKV feature checks passed (`reviewer-evidence/ci.log:4`, `reviewer-evidence/conformance.log:3`, `reviewer-evidence/statics.log:5`, `reviewer-evidence/dst.log`, `reviewer-evidence/tikv-server.log:101`). The brief's two external dependencies were actually exercised, without aliases or replacement fixtures.
- **Host and baseline caveats:** the local full CI run stopped at `cargo deny` because its advisory database requires an exclusive lock on a read-only path (`reviewer-evidence/ci.log:3218`). The frozen full run did execute the dependency audits and finish green (`gate-logs/C4-ci.log:3296`, `gate-logs/C4-ci.log:3917`); this is not a patch failure. Diff coverage remains unmeasured: its wrapper rejected the patch against `origin/main` (`gate-logs/C4-diff-cov.log:10`). The brief explicitly requires the stacked prerequisites (`brief.md:4`), and the provided patched target compiled and reproduced red→green; the coverage baseline failure does not establish a compilation or application defect in this patch.
- **Mutation evidence:** the independent `cargo mutants --in-diff` run reproduced the frozen result exactly: 37 candidates, nine caught, 28 unviable, zero missed or timed out (`reviewer-evidence/mutants.log:6`, `gate-logs/C5-mutants.log:13`). The 28 unviable candidates did not exercise tests and are not counted as behavioral proof.
- **Other frozen gates:** the batched review reports zero blocking findings (`gate-logs/T4-batch-review.log:10`); that is the full extent of its supplied evidence. The TiKV log shows both feature compilations completing (`gate-logs/host-tikv.log:110`, `gate-logs/host-tikv.log:209`), also reproduced locally. Contribution validation is deferred, with its mandatory publish rerun explicit above.
- **Prior art and settled scope:** the brief records a check by `restore.rs` and `multipart.rs` paths, including merged history and rejected approaches #809, #637 and #664 (`brief.md:145`). Original history and closed/rejected tracker records are absent from this artifact-only target. The tracked DST deferral to #843 and drain interaction deferral to #659 are settled, not new findings (`target/crates/custodian/src/restore.rs:785`, `target/crates/custodian/src/restore.rs:932`). No dedicated Tier-1 disk-fault or Tier-2 kill/reconstruct run was performed; this metadata-only slice adds no fragment deletion, so those campaigns are follow-up observation rather than evidence for this fence. The sibling reaper/operator-abort size issue remains assigned to #656/#659 (`brief.md:108`). No `INTEGRATION.md` is present in the supplied target (`reviewer-evidence/grounding.log:20`).

### Advisory — adversary

# Adversarial review — #842 (809.4), iteration 7

I could not refute the fix itself. The production code did the right thing in every attack below.
One test gap is left (a surviving hand mutant that can wedge the pass), plus one cross-issue item
the brief asked to be flagged, which this diff does not record anywhere.

## Findings

- NEEDS-HUMAN [impl] — `crates/custodian/src/restore.rs:1017-1027` (`part_chunks`): the path that
  **skips an unreadable `part:` record** is never run by any test (llvm-cov: line 1027 = 0). The doc
  comment's claim ("skipped … it can only make a chunk look held by no part, naming the session") is
  unpinned. I tried a hand mutant that turns the skip into `?`
  (`parse_part_key(key).and_then(|_| decode_part_record(value)).map_err(..)?`). It **survives all
  7 tests here, all 9 in `restore_open_fence.rs`, and the rest of the `wyrd-custodian` suite**
  (`--no-fail-fast`). Failing case, which I ran: a `Completing@3` session `ab…` whose segment 1 names
  part 2's chunk, with `part:ab…:000002` = `not a part`, plus an `Open` control `fe…` that sorts
  after it. Production (I checked): fenced, named `ChunkInNoPart` at `seg:ba…:3:000001`, the part
  listed in `unresolvable`, the control fenced, and both passes `Ok`. Under the mutant: the pass
  returns `Err("malformed part: record value …")` and **the `Open` control is left `Open`**
  (unfenced, so it can still publish: D-B). Every re-run fails the same way, because `recheck_fenced`
  → `check_attempt` → `part_chunks` reads the same record, so the fence generation can never finish.
  Fix: add this case to leg H (and to K's store), asserting it is fenced and named, the control is
  fenced, and the pass is `Ok` on both runs. Low severity: the code is correct today; this pins it.
- NEEDS-HUMAN [human] — `crates/core/src/multipart.rs:3302`: the patch rewrites the
  `{session, parts: <set>}` row so it now names the reaper's and the operator abort's
  `Completing→Aborting` fence as that shape's writers. It says nothing about the overflow that the
  row just above it gives as the restore fence's reason to avoid that shape (10,000 sparse parts =
  128,916 bytes > `MAX_VALUE_BYTES`). The brief's ordering note says to "flag that to #656 and #659".
  Nothing in the diff records that it was done: no `deferred:` marker, no note in the row. A #656
  builder who reads this table as the spec will build a batch that FoundationDB refuses permanently
  (`2103`). A human should confirm the flag was filed on #656/#659, or decide whether the row should
  point at it.

## Minor (not routed; optional doc nits)

- `crates/core/src/multipart.rs:3456-3463` still says `all` is legal only in "the session teardown
  `{session, all}` the reaper's `Open` arm installs", and that "only a teardown fence makes that range
  immutable". For the new `Completing` row, it was the Complete fence that froze the range
  (`0016:704`; `upload_part_answer`, `multipart.rs:4464`). `crates/custodian/tests/restore_open_fence.rs:665` still
  calls case (iii) plain "`Completing`" after the patch moved it to `Completing@u64::MAX`.
- Reviewer evidence, not a defect in this diff: C5's "pass" (37 mutants: 9 caught, 28 unviable) says
  little on its own. The workspace has `warnings = "deny"` (`Cargo.toml` `[workspace.lints.rust]`),
  so most generated mutants never compile. I re-ran it with `--cap-lints=true`: `restore.rs` had 25
  mutants, 22 caught, 3 unviable, 0 missed; `multipart.rs` + `cli.rs` had 12, 6 caught, 6 unviable,
  0 missed. The conclusion holds, but only on my re-run. C4-diff-cov's "fail" comes from the stacked
  base (the patch is on top of child-1..3, which are not on `origin/main`). I measured coverage
  myself: every new line in `restore.rs:801-1050` ran except `:909` (`Aborting@0` → `Settled`,
  harmless) and `:1027` (above).

## Refutation attempts that failed

- **Red→green.** The frozen `C4-verify.log` shows 7 of 7 red on the base, each by assertion (cause
  `Completing`, session still `Completing@3`), not by a compile failure. All 7 + 9 pass on the patched
  tree (re-run here). Every leg drives the production `reconcile_after_restore`. I found no tautology:
  `names()` matches only quoted keys, and the bad `seg:` keys reach the report only through
  `segments_unaccounted`. K also checks the `session-segments-unaccounted` audit event per record.
- **G-atomic as a weak test.** A fence split across commits fails it in either order, and G's
  "ONE commit" check (`assert_fenced`) catches a split directly.
- **Records-obligation guard** (`restore.rs:957`, `segments()==group && parts().is_none()`).
  `session`, `chunks` and `generation` are bytes-mode only (`multipart.rs:3443-3448`), so a decodable
  `retire:records:` value can carry only `parts` and/or `seg`. The guard is complete.
- **False `NoDeleter`/`NotOfAttempt` on a consistent store.** The Complete fence bumps the epoch
  (`0016:704`), and so does every fence (`0016:364`), so an `Open`/`Aborting@E'` session never shares
  `(nonce, E'-1)` with a live attempt. A session that was rolled back and then aborted reads an empty
  range at `E'-1`. Half-drained ranges are under `deferred: #659` (`restore.rs:932-934`): settled.
- **Value ceiling, `u64::MAX`, collisions on either key, paging past 512 `seg:` records and past the
  first `part:` page, multi-chunk segments, a nonce that differs from the upload id.** Each one is
  pinned, and I found no input that gets past it.
- **`is_clean`** (`restore.rs:341-347`) goes through `needs_human()`, which now includes
  `segments_unaccounted`, so a re-run that only names segments is never reported clean.

### Advisory — code-review

No findings. No introduced correctness bugs or actionable reuse, simplification, or efficiency issues were identified in this diff.

Reviewed against the read-only target source and frozen gate evidence. CI and all seven restore-fence regression tests passed; mutation testing reported 9 caught and 28 unviable mutants. Diff coverage was not measured because the coverage runner could not apply the patch to `origin/main`. Tests were not re-run during this advisory review.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] T5 Judgment — Decide whether the recorded path-based prior-art check remains sufficient at sign-off — this target has only a synthetic base and no remote, so subsequent merged or closed/rejected work cannot be independently excluded (`brief.md:145`, `reviewer-evidence/grounding.log:14`).
- [x] Validation — fitness-to-purpose — Accept the writers-stopped restore procedure and operator handling of damaged records before drains resume — regression tests establish the metadata behavior, not operational disaster-recovery fitness (`target/docs/design/architecture/m4-first-deployment-blueprint.md:581`, `target/crates/server/src/cli.rs:1409`).
- [x] `crates/custodian/src/restore.rs:1017-1027` (`part_chunks`): the path that **skips an unreadable `part:` record** is never run by any test (llvm-cov: line 1027 = 0). The doc comment's claim ("skipped … it can only make a chunk look held by no part, naming the session") is unpinned. I tried a hand mutant that turns the skip into `?` (`parse_part_key(key).and_then(|_| decode_part_record(value)).map_err(..)?`). It **survives all 7 tests here, all 9 in `restore_open_fence.rs`, and the rest of the `wyrd-custodian` suite** (`--no-fail-fast`). Failing case, which I ran: a `Completing@3` session `ab…` whose segment 1 names part 2's chunk, with `part:ab…:000002` = `not a part`, plus an `Open` control `fe…` that sorts after it. Production (I checked): fenced, named `ChunkInNoPart` at `seg:ba…:3:000001`, the part listed in `unresolvable`, the control fenced, and both passes `Ok`. Under the mutant: the pass returns `Err("malformed part: record value …")` and **the `Open` control is left `Open`** (unfenced, so it can still publish: D-B). Every re-run fails the same way, because `recheck_fenced` → `check_attempt` → `part_chunks` reads the same record, so the fence generation can never finish. Fix: add this case to leg H (and to K's store), asserting it is fenced and named, the control is fenced, and the pass is `Ok` on both runs. Low severity: the code is correct today; this pins it.
- [x] `crates/core/src/multipart.rs:3302`: the patch rewrites the `{session, parts: <set>}` row so it now names the reaper's and the operator abort's `Completing→Aborting` fence as that shape's writers. It says nothing about the overflow that the row just above it gives as the restore fence's reason to avoid that shape (10,000 sparse parts = 128,916 bytes > `MAX_VALUE_BYTES`). The brief's ordering note says to "flag that to #656 and #659". Nothing in the diff records that it was done: no `deferred:` marker, no note in the row. A #656 builder who reads this table as the spec will build a batch that FoundationDB refuses permanently (`2103`). A human should confirm the flag was filed on #656/#659, or decide whether the row should point at it.
- [x] **Atomicity alone does not protect the new retirement obligation from overwrite.** G requires three writes in one commit and a failing-commit witness (`brief.md:35-43`), but never requires a collision witness for the newly added `retire:records:s:<id>:<E>` key. The target explicitly requires `require_absent` on obligation installation and classification of collisions, because overwriting a payload permanently loses reclamation evidence (`crates/core/src/multipart.rs:1409-1414`; `docs/design/proposals/draft/0016-multipart-commit-protocol.md:369-373`). A blind put of the records obligation can satisfy the listed fresh-store and atomicity cases. Revise G to require the absence guard and seed an existing, decodable same-epoch obligation naming a different segment group: its bytes must survive, none of the three fence writes may land, and the unresolved collision must be reported.
- [x] **The promised epoch transition has an accepted input with no successor.** G says `Completing@E` ends at `Aborting@E+1` (`brief.md:35-38`), and the invariant is unconditional (`brief.md:76-79`). On the supplied target, the session epoch is any `u64` (`crates/core/src/multipart.rs:2085`); Completing validation checks cursor bounds and matching target identity/epoch, but does not reject `u64::MAX` (`crates/core/src/multipart.rs:2204-2240`). Such a decodable record cannot take the required transition. Revise H to cover a Completing record at that boundary: checked increment, no wrap/panic or partial obligations, unchanged session reported as needing a human, and continued fencing of a later eligible session. Explicitly apply any inherited child-3 guard to Completing, and qualify the invariant for sessions left unresolved instead of promising universal fencing.
- [x] T5 Judgment — Confirm the path-based prior-art disposition against merged history and closed/rejected work — the brief records that check, but this one-commit snapshot has no remote or historical review evidence to corroborate it (`brief.md:145`, `pdca-reviewer-842-evidence/source-audit.log:10`).
- [x] T5 Judgment — Confirm merged-history and closed/rejected-work coverage by affected path on the publication base: the brief documents restore.rs/multipart.rs prior art, but this target exposes only one synthetic commit and no remote, so broader coverage cannot be independently settled (`brief.md:145`; `reviewer-evidence/target-grounding.txt:2`).
- [x] T5 Judgment — Confirm merged and closed/rejected prior art for all seven affected paths — the supplied single-commit target has no remote/history to establish whether equivalent work already exists (`reviewer-evidence/prior-art.log:1`, `brief.md:145`).
- [x] the C5 row ("36 mutants tested: 8 caught, 28 unviable", 0 missed) is not evidence of adequacy. The workspace sets `warnings = "deny"` (`Cargo.toml:230`), and `.cargo/mutants.toml` sets no `cap_lints`. So any mutant that stubs a body and leaves a parameter unused fails to compile, and is counted "unviable". Re-running the same 36 mutants with `--cap-lints=true` gives **25 caught, 2 missed, 9 unviable**; the 2 missed are the ones in bullet 1. Whether the gate (or `.cargo/mutants.toml`) should cap lints is a harness/repo decision outside this diff. The two survivors themselves are bullet 1's [impl] work.
- [x] leg (L), "`cargo xtask ci` green", is unverified, yet `check-gates.json` reports `overall: "pass"`. The gating C4-ci row is `unverifiable`: it hit the 7200 s timeout inside `crates/server/tests/custodian_day_one.rs`, after fmt, clippy, build and most tests had passed. It never ran machete, deny, conformance, statics, the orchestrator guard or DST. The hang is not this patch: here, with the patch applied, `custodian_day_one` passes 15/15 in 0.19 s. What I re-ran myself, all green: `cargo xtask statics`; `cargo clippy -p wyrd-dst --all-targets` under `--cfg madsim`; and the DST `restore*` tests. Still not run: machete, deny, conformance, the orchestrator guard, the full DST seed sweep, and the server tests after `custodian_day_one`. Provisional (toolchain/time), not a refutation.
- [x] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) unverifiable — gate exceeded its 7200s timeout
- [x] T5 Judgment — Confirm that the recorded path-based prior-art review covers merged and closed/rejected work for all affected paths — the brief names two paths and rejected iterations, but this snapshot has only one synthetic commit and no remote to substantiate the complete check (`brief.md:145`, `reviewer-evidence/scope-and-grounding.log:23`).
- [x] `crates/custodian/src/restore.rs:931`: the patch's own reasoning ("its absence proves nothing (a damaged or hand-repaired store)") applies just as much to the **bytes** obligation, which the re-run check never reads. Probe: `Aborting@4`, a correct `retire:records:s:<id>:3` `{seg}`, **no** `retire:bytes:s:<id>:3`, two live `part:` records. Result: clean, `needs_human() == false`, and those parts have no deleter. The brief limits leg K to the `seg:` side, and the `Open` fence (child-3) has the same gap, so this is outside this PR's scope. Per the rubric it needs a decline with a tracking-issue reference, not an in-PR fix. A human should decide whether to file that issue.

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: merged-wider
- Iteration delta (if iterating):
- By / date: Eduard Ralph / 2026-09-30

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 2 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
- #842 follow-up: add a test for `part_chunks` skipping an unreadable `part:` record (`restore.rs:1017-1027`) — session fenced + named, later `Open` control fenced, both passes `Ok`; a skip→`?` mutant survives today.
- #842 follow-up: restore re-check ignores a missing `retire:bytes` obligation on already-fenced sessions (both `Open` and `Completing` arms) — filed as getwyrd/wyrd#855.
