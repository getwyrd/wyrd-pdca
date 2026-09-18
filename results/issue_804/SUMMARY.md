# Result — issue 804 / gc-reclaim-intent-and-mark-shapes

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: three gaps in how GC reads and consumes `orphan:` marks.
  1. **Destroy first, record second.** GC calls `delete_fragment`
     (`crates/custodian/src/gc.rs:314`) before queueing the key delete (`:321`, committed
     `:346`), so an adoption CAS on a pre-mark's original bytes can land after the fragment is
     gone — a placement over deleted bytes (`0016:1293-1320`).
  2. **One shape decodes.** Only a bare decimal reads (`gc.rs:811-817`); 0016's structured and
     `reclaiming` shapes (`0016:1190-1211`, `:1321-1338`) read as unreadable and are kept
     forever.
  3. **A draining retirement protects nothing.** A mark naming a pending `retire:bytes:`
     obligation's event is reclaimed on its own stale grace (`0016:1226-1248`, X97 `:2626`).
- Success criterion: the NEW file `crates/custodian/tests/gc_reclaim_intent.rs` passes over
  in-memory doubles built as `crates/custodian/tests/gc_ledger_walk.rs` builds them, a fresh
  `GcContext` each pass; structured values are raw JSON. Every leg seeds a control the pass does
  reclaim. Legs:
  **(A) Three shapes decode; a fourth fails closed.** One mark each: the bare decimal
  `mark_orphaned` writes (`gc.rs:181-193`), `{"orphaned_at_millis":N,"event":"E"}`, and
  `{"orphaned_at_millis":N,"event":"E","reclaiming":true}` (event optional). GC honours each
  (the third per B(iv)). A value that is none of them: GC leaves it byte-identical, never
  reclaims its fragment, and names it on its audit seam (as `gc.rs:1010-1023` does today). Guard,
  green on the base: restore counts all four `already_marked` and leaves each byte-identical —
  existence is its whole judgement (`gc.rs:862-897`), and no reader rewrites a mark
  (`0016:1208-1211`). Base red: a structured mark past grace licenses nothing.
  **(B) Recorded before destroyed (`0016:1312-1338`).** (i) The double errors on the commit
  recording reclaim intent: the fragment is still present. (ii) The mark changes between GC's
  read and its intent commit: that intent loses, its fragment survives, the others in the pass
  are still reclaimed; and a pass whose only candidate loses does not answer `Satisfied`.
  (iii) The double's `delete_fragment` hook commits an adoption CAS `require(orphan:<pos> ==
  <bytes GC read>)` as GC deletes: it gets `Conflict`. (iv) A `reclaiming` mark over a present
  fragment, stamped recently, is finished next pass — fragment deleted once, then the key —
  with no grace test. (v) A store fault ends a pass after it deleted fragments: their keys are
  gone afterwards and the error still propagates. Base: (i)/(iii) delete first, (ii) reclaims
  on a changed mark, (iv) does not decode, (v) drops the queued deletes.
  **(C) Intents are batched.** With 1,001 reclaimable marks (`CLEANUP_BATCH` + 1, `gc.rs:101`,
  written as a number as `gc_ledger_walk.rs:88-90` writes `W`), count the commits that carry a
  precondition or a put on an `orphan:` key: exactly 2, and neither carries more than 1,000.
  Base red: no commit carries one. (v1's surviving mutant committed each intent alone and
  passed every test.)
  **(D) A draining retirement protects by keyed lookup.** A structured mark past grace whose
  `event` is a `RetireToken`'s canonical string (`crates/core/src/multipart.rs:1446-1462`):
  while `retire:bytes:<event>` (`retire_key`, `:1465`) exists the fragment survives; delete it
  and the next pass reclaims. The double records every `scan`/`scan_page`; none reads
  `retire:`. Base red: the second half.
  **(E) Seeded DST**, appended to the EXISTING `crates/dst/tests/custodian.rs` (no new DST
  file): a mover's adoption CAS on its pre-mark races GC's reclaim over a delete spanning a
  simulated hop, and never publishes a placement naming a deleted fragment (outcome (c)); a
  coverage property proves both outcomes are reached, as `:2139` does.
  **(F) `cargo xtask ci` green.**
- Repo + branch target: getwyrd/wyrd @ main
- Scope: the `orphan:` value's three shapes and their one codec, beside `orphan_key` in
  `crates/core/src/metadata.rs` (`:62-85`); decode accepts exactly what encode writes, so a
  legacy value round-trips byte for byte; `mark_orphaned`'s output is unchanged. GC: every
  shape decodes; the reclaim intent — an exact-value CAS of the mark to `reclaiming` — commits
  before `delete_fragment`, the key deleted after as today; intents batched at most
  `CLEANUP_BATCH` per commit, a lost precondition costing only its own; `reclaiming` resumed
  with no grace test; a pass that lost an intent and reclaimed nothing is not `Satisfied`; a
  fault after deletes still commits their queued key deletes before the error propagates (best
  effort). A crash can still leave `reclaiming` over a deleted fragment, which a
  `list_fragments()`-driven walk never revisits: #800's sweep — mark it `// deferred: #800`. A
  pending `retire:bytes:` obligation named by a mark's event protects through one keyed `get`
  per candidate, never a range read of `retire:`. Restore is untouched: `marked_among`
  (`gc.rs:862-897`) keeps existence as its whole judgement. The codec doc states the
  writer-side rule 0016 leaves implicit: no writer overwrites a `reclaiming` mark (#663 and
  #659 inherit it). Existing tests stay green, or `build-notes.md` says which expectation
  changed and why. No signature change to `reconcile_step`; no new field on a context struct.
  Docs (a persisted value changes, `AGENTS.md:154-157`): the shapes and their dual-format rule
  in `docs/design/architecture/08-crosscutting-concepts.md` §8.7 after `:85`;
  record-before-destroy in `06-runtime-view.md` §6.7 step 2, for marked fragments only, saying a
  draining retirement's fragments are never reclaimed (its drain is what marks them). / out of
  scope: the staged class (child-1); `restore.rs`; the orphan-identity migration gate (X92/X111,
  `0016:1249-1280`) and the three-arm mark write (`0016:1218-1224`), both #659's; the
  fragment-less sweep (#800); the expired-lease arm's order; `scrub.rs`, `reconstruction.rs`,
  `rebalance.rs`, `desired_state.rs`; 0016 and the ADRs.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (9 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 98.1% — 354 of 361 instrumentable changed lines executed (floor 80%); 361 of 804 changed lines were instru
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 58 mutants tested in 2m: 28 caught, 30 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_804/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.24s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Task under review: make orphan-mark reclamation durable before fragment deletion, decode all three persisted mark shapes, and protect marks owned by active retirement obligations.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The accepted scope defines observable A–F outcomes, the C-1 durability invariant, exclusions, and both external dependencies, so the implementation has a falsifiable contract (`brief.md:21`, `brief.md:71`, `brief.md:109`, `brief.md:138`). |
| C2 Reproduction (red pre-fix) | PASS | An independent stash replay compiled the retained regression suite and failed 8 of 9 tests by assertion—including the adoption-after-delete corruption—matching the frozen red evidence (`gate-logs/C4-verify.log:15`, `gate-logs/C4-verify.log:38`, `gate-logs/C4-verify.log:88`). |
| C3 Change | PASS | The change closes the data-loss window with a durable exact-value transition, preserves legacy mark bytes, fails closed on noncanonical values, and keeps the fragment-less sweep explicitly settled under #800 (`crates/custodian/src/gc.rs:433`, `crates/custodian/src/gc.rs:617`, `crates/core/src/metadata.rs:268`). |
| C4 Verification (red→green) | PASS | After restoring the patch, the focused suite passed 9/9 independently; frozen evidence also shows all 20 madsim custodian tests, `typos`, and the docs renderer passing, so the declared dependencies were exercised (`gate-logs/C4-verify.log:10`, `gate-logs/C4-ci.log:11`, `gate-logs/C4-ci.log:16`, `gate-logs/C4-ci.log:3520`). |
| C5 Causal adequacy | PASS | The fix removes the causal ordering defect rather than probing around it: reclamation is committed before deletion, a changed mark loses its own CAS, and restart consumes terminal intent without a second grace decision (`crates/custodian/src/gc.rs:577`, `crates/custodian/src/gc.rs:593`, `crates/custodian/src/gc.rs:486`). |
| T1 Structure | PASS | The shared persisted-value codec is colocated with the metadata key protocol, GC lifecycle logic is isolated in bounded helper types, and the required living architecture sections are current (`crates/core/src/metadata.rs:63`, `crates/custodian/src/gc.rs:400`, `docs/design/architecture/08-crosscutting-concepts.md:87`). |
| T2 Shape | PASS | Legacy, structured, and reclaiming encodings have one canonical byte image with omitted optional defaults, bounded event identity, strict decode errors, and explicit byte-round-trip tests (`crates/core/src/metadata.rs:223`, `crates/core/src/metadata.rs:255`, `crates/core/src/metadata.rs:301`, `crates/core/src/metadata.rs:4013`). |
| T3 Runtime | PASS | Work remains bounded by 1,000-intent commits and the ledger window, while retirement protection uses one bounded keyed read per candidate rather than an unbounded namespace scan (`crates/custodian/src/gc.rs:507`, `crates/custodian/src/gc.rs:513`, `crates/custodian/src/gc.rs:560`, `crates/custodian/src/gc.rs:573`). |
| T4 Contribution | N/A | Contribution artifacts are absent by design at Check, so their substantive audit is owed to the mandatory publish gate and this deferred result is not a finding (`gate-logs/T4-contribution.log:10`). |
| T5 Judgment | PASS | No unresolved rubric defect remains; the affected-path prior-art record covers merged, open, closed, and rejected work, and the frozen multi-pass review reports no blocking or rejected findings (`brief.md:151`, `gate-logs/T4-batch-review.log:10`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether the record-before-destroy protocol and dual-format rollout are fit for production durability operations—automated red→green and DST evidence establish the specified invariant but cannot authorize the operational tradeoff (`brief.md:71`, `docs/design/architecture/06-runtime-view.md:78`). |

### Advisory — adversary

# Adversarial review — #804 (GC reclaim intent + orphan-mark value shapes)

Verdict: **I could not break the core fix.** Recording the intent before the delete is sound in
every interleaving I tried. The red→green evidence holds, including the DST leg (the
simulated-time test), which no gate had checked on the base. What's left is three judgment calls
about contracts and cost. None of them is a failing case in today's tree.

## Evidence (re-checked)

- The unit legs fail by assertion on the base: `gate-logs/C4-verify.log` shows 8 of 9 red, each
  on a real assertion (`gc_reclaim_intent.rs:683,790,845,927,968,1024,1100,1233`). The 9th,
  `a_restore_counts_every_shape_already_marked_and_rewrites_none` (`gc_reclaim_intent.rs:732`),
  is the restore guard the brief designed to pass on the base. So the claim "9 test(s) ran red"
  in `check-gates.json` is off by one. This is a harness wording issue, not a patch defect.
- The DST leg (E) was never run against the base by any gate: C4-verify only runs the new
  `tests/*.rs` file, and C4-ci runs the patched tree. I re-ran it myself. I took a scratch copy,
  restored the base `metadata.rs`, `gc.rs` and `reconciliation.rs`, kept the patched
  `crates/dst/tests/custodian.rs`, and ran `RUSTFLAGS=--cfg madsim cargo test -p wyrd-dst --test
  custodian -- gc_reclaim_intent`. **Both tests fail on the base**, at `custodian.rs:3381`
  (base line numbering): `[Pass, PreMarkRead, Adoption(Committed), DeleteBegan(1, ..),
  Deleted(1, ..), ..]`. The adoption commits after the base GC read the pre-mark, and the base
  GC then deletes the fragment the new placement names. So E is genuinely red→green, and it runs
  the production `reconcile_step` over `SimTikvMetadataStore`.
- The unit legs call the production `reconcile_step` / `reconcile_after_restore`, not a copy.
  B(iii) commits its adoption from inside the double's `delete_fragment`
  (`gc_reclaim_intent.rs:405-420`), which is after GC's intent commit on the patched code and
  before any ledger write on the base. That is the right place to separate the two.

## Findings

- NEEDS-HUMAN [human] — **A stale `reclaiming` mark on a still-referenced fragment turns into a
  delete with no grace wait, once a writer obeys the new "never overwrite `reclaiming`" rule.**
  The codec doc makes the rule absolute: a writer that finds `reclaiming` "must not replace it …
  waits until the key is gone" (`crates/core/src/metadata.rs:126-137`). GC resumes any
  `reclaiming` mark with no grace test (`crates/custodian/src/gc.rs:486-493`), and the safety
  gate is the only thing in front of it (`gc.rs:475-482`).
  Concrete sequence:
  1. GC commits `reclaiming` for P and dies before `delete_fragment`.
  2. An in-tree mover re-places the chunk onto P without an adoption precondition
     (`reconstruction.rs:934`, `rebalance.rs:534`; that precondition only arrives with #663).
     P is now referenced and carries a `reclaiming` mark. GC skips it, correctly, because P is
     referenced.
  3. Later the object is retired by a writer that follows the rule, such as #659's drain. It
     either waits forever (GC never deletes a referenced key), or it drops the reference without
     re-marking. On the next pass GC sees P unreferenced and `reclaiming`, and deletes the bytes
     immediately. A reader still holding the prior version gets no grace.

  Today's dereferencing writers blind-put a fresh legacy mark, so this can't happen in the
  current tree. But the rule is written here for #659 and #663 to inherit. It also contradicts
  0016's cleanup pass, which "re-stamps or drops any mark found on a still-referenced fragment"
  (`0016:1255`). Someone needs to decide whether the rule needs a "referenced position"
  exception. A cheap step either way: GC could name a `reclaiming` mark it finds over a
  referenced fragment on the audit seam, instead of the generic `referenced` skip.
- NEEDS-HUMAN [human] — **One changed mark makes GC commit the whole batch one intent at a
  time.** When the batch commit returns `Conflict`, `record_intents` retries every intent alone
  (`gc.rs:600-613`). A batch of `W` = 1,000 intents with one re-stamped mark costs 1 + 1,000
  sequential commits instead of about 1 + 2·log₂(1000) ≈ 21 with bisection. Under steady
  contention (for example #659's drain re-stamping legacy marks "on contact",
  `0016:1203-1207`, while GC walks the same window), every conflicted batch behaves like the
  one-commit-per-intent mutant that leg C (`gc_reclaim_intent.rs:1076`) was written to kill. It
  is still correct: each lost intent costs only itself. This is cost only, so whether it matters
  before #659 lands is a judgment call.
- NEEDS-HUMAN [human] — **The docs overstate the draining-retirement guarantee.**
  `docs/design/architecture/06-runtime-view.md:78` says "A multipart retirement that is still
  draining never has its fragments reclaimed". The code only protects a fragment whose mark is
  structured and names that retirement (`gc.rs:569-575`, `metadata.rs:145-153`). A fragment of
  a draining retirement that still carries a legacy mark, or one naming an older event, is
  reclaimed on that mark's own grace. That is exactly the gap 0016 closes only with #659's
  migration gate (`0016:1249-1265`). The brief asked for this wording, and v1 was rejected for a
  similar overstatement ("never marked"). The sentence's last clause ("a fragment it marked is
  reclaimed only once …") is accurate. The opening "never" is not.

## What I tried that did not break it

- **Codec** (`metadata.rs:190-240`): leading zeros, `+`/`-`, whitespace, u64 overflow, duplicate
  or reordered fields, `null`, `"reclaiming":false`, JSON `\uXXXX` and `\/` escapes, trailing
  whitespace, and events with space, quote, backslash or non-ASCII. The re-encode check refuses
  every one of them, so `Intent::record`'s re-encoded precondition always equals the stored
  bytes. `retire_token()` goes through the strict `parse_retire_key`, and a non-canonical
  spelling returns `None`. Only `Bytes` mode ever marks (`multipart.rs:1339-1344`).
- **Fault paths**: an `Err` on the intent batch deletes nothing. If the batch actually landed,
  the next pass resumes it. A fallback commit that errors after earlier single commits landed
  and were destroyed still commits their key deletes through `finish_after_fault`
  (`gc.rs:1516-1520`). A cleanup batch whose own commit failed is not retried. That leaves
  `reclaiming` marks over deleted bytes, which is the documented `deferred: #800` case
  (`gc.rs:621-625`). The one untested line is the `emit_cleanup_lost` arm (diff-cov MISS
  `gc.rs:1518`). It is best-effort, so the risk is low.
- **Races**: two concurrent GC passes (the loser's CAS conflicts, and deletes are idempotent);
  an adoption before the read, between the read and the CAS, inside the delete, and after the
  pass (the DST covers all four); an obligation reappearing between the keyed `get` and the CAS
  (tokens are never reused, `multipart.rs:1409-1414`).
- **Protection order**: the referenced and staged gates run before every mark arm, `reclaiming`
  included. An incomplete reference set still withholds everything and answers `Blocked`. No
  other in-tree code reads `orphan:` values (checked all of `crates/*/src`), so GC's new JSON
  writes reach no bare-u64 parser.
- **Leg C** kills the per-intent, one-batch and per-D-server variants. It asserts exactly 2
  recording commits, each ≤ `W`, summing to the population.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] Validation — fitness-to-purpose — Decide whether the record-before-destroy protocol and dual-format rollout are fit for production durability operations—automated red→green and DST evidence establish the specified invariant but cannot authorize the operational tradeoff (`brief.md:71`, `docs/design/architecture/06-runtime-view.md:78`).
- [x] **A stale `reclaiming` mark on a still-referenced fragment turns into a
- [x] **One changed mark makes GC commit the whole batch one intent at a
- [x] **The docs overstate the draining-retirement guarantee.**
- [x] size backstop — this slice is behaving oversized: patch is 136 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat. Human overrode: accepted as-is.

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
- By / date: Eduard Ralph / 2026-09-15

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
- #804 review flagged a future conflict between the "never overwrite reclaiming" doc rule and no-grace-period resume — needs resolution when #659/#663 land.
- #804 review flagged per-intent retry fallback on batch conflict as slow under contention — no bisection retry strategy; cost-only, not correctness.
- #804 06-runtime-view.md §6.7 draining-retirement doc overstates guarantee ("never reclaimed") — only protects fragments with the new structured mark, not legacy marks; needs a docs fix.
