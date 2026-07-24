# Result — issue 626 / multipart-commit-protocol

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: The multipart **commit protocol** — what happens *underneath* a
- Success criterion: two legs, both evaluated at Check on the patched tree.
- Repo + branch target: getwyrd/wyrd @ main
- Scope (one logical fix) / out of scope: ONE logical change: REWORK draft proposal 0016 (starting from

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: patch touches no Wyrd crate (docs/CI only) — nothing to verify per-fix; the C4-ci gate covers it.
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — INFO Diff changes no Rust source files

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 7 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of issue #626’s docs-only proposal to settle the multipart commit, protection, reclamation, reaper, and chunk-map segmentation protocol required before #508.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance decision is sufficiently falsifiable: it fixes seven decision areas, F1–F18, computed bounds, implementation ordering, and exactly two documentation paths (`brief.md:72`; `brief.md:81`; `brief.md:106`). |
| C2 Reproduction (red pre-fix) | NEEDS-HUMAN | Decide whether the withheld iteration artifacts establish the content-level red — removing this patch leaves `typos`, docs lint, and link audit green, and the supplied gate manifest has no C2 gate (`check-gates.json:12`). |
| C3 Change | FAIL | The proposal is not ready to settle #626 while the asserted fresh batch review still has seven untriaged blocking findings; their text was not included in the permitted artifacts, so the specific repairs cannot be independently adjudicated (`check-gates.json:91`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Decide whether CI evidence may be accepted despite the independent rerun stopping at a read-only Cargo advisory-database lock — `typos`, docs lint, render, clippy, build, tests, and machete passed, but full green and a content red→green were not reproduced (`check-gates.json:38`). |
| C5 Causal adequacy | NEEDS-HUMAN | Decide whether per-session `sinf:` CAS plus per-chunk session preconditions is the intended guaranteed-residue-bound mechanism — it preserves the stated bound but imposes the explicitly contested serialization cost (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:1855`). |
| T1 Structure | PASS | The human can assess one editable draft plus its authoritative index row; frontmatter is complete and the change is confined to the two required paths (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:1`; `docs/design/proposals/README.md:32`). |
| T2 Shape | FAIL | The settlement shape cannot be accepted while seven blocking multi-pass findings remain untriaged, even though the document visibly includes the required execution register, accepted-cost register, and F1–F18 disposition table (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:1465`). |
| T3 Runtime | N/A | This cycle changes a draft design and index only; no runtime implementation exists to exercise (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:29`). |
| T4 Contribution | FAIL | The contribution does not meet the repository definition of done because its deterministic deep multi-pass review is red with seven blocking findings and no recorded rejection (`check-gates.json:91`). |
| T5 Judgment | NEEDS-HUMAN | Decide whether the protocol’s safety/capacity trade-offs are acceptable for implementation, especially roughly one maximal-part session per 4,000,000-reference budget and the explicit serialization-cost question (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:1710`; `docs/design/proposals/draft/0016-multipart-commit-protocol.md:1869`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether this draft is fit to unblock #508 and bind #625 — mechanical prose checks cannot validate the protocol’s refutation standard or architecture-governance acceptability (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:126`). |

### Advisory — adversary

# Adversarial review — issue 626 (iteration 7), advisory

Lens: refute the settlement (leg B) and the reviewer's verdict; find the execution that
strands/absorbs/publishes-over-reclaimed/over-envelope and is **absent from the register**.
Target read-only at `$PDCA_TARGET`
(`docs/design/proposals/draft/0016-multipart-commit-protocol.md`, 1919 lines).

## The evidence (red→green)

- Docs-only deliverable, verification posture (a). There is **no code red→green to re-run**:
  `C4-verify` and `C5-mutants` are vacuous on a no-Rust diff (the brief says so, and the gate
  rows record it — I did **not** read either as green). Mechanical leg A1 (`C4-ci`) is green,
  and I confirmed the two things it actually checks: the README index row mirrors 0014's
  draft-row shape with a resolving relative link (`docs/design/proposals/README.md:32`), and
  every `[iNNN]` link reference is defined (`[i625]` at `:1919`) — no dangling link, no missing
  row. So leg A1's green is real but shallow, exactly as the brief bounds it.
- **The gating judgment leg is already RED, not green.** `check-gates.json` records
  `T4-batch-review = fail` (7 blocking, 0 recorded-rejected) and `overall = fail`. There is no
  confirmatory "green" verdict for me to overturn here — the deterministic gate already blocks
  publish. My job below is only to add grounded, concrete findings on top of that.

## Findings

- **NEEDS-HUMAN [impl] — `0016:522`, `:1816`, `:396`, `:1492`: restore-fencing a *Completing*
  session that has already written segments is an undefined transition, and the resulting stray
  `seg:<id>:E:*` records have no reclamation path — outcome (a), and absent from the register.**
  D-B requires the restore pass to "fence every restored **`Open`/`Completing`** session to
  `Aborting`" (`:522`, `:1816`). But the state machine gives a `Completing` session only two
  exits — `→ Completed` (flip) and `→ Open` (reaper rollback) — and the batch inventory defines
  **no** `Completing → Aborting` batch: the abort/reap-fence row is `require(mpu == Open@E)`,
  with `Completing@E` named only "for the **rollback**" to `Open` (`:396`). So "fence a
  Completing session to Aborting" is a transition the rest of the document never defines. The
  concrete trace: snapshot a session in `Completing@E` **after** its segment-write phase
  (`seg:<id>:E:*` durable) but **before** the root flip → restore → D-B fences it to `Aborting`.
  The `seg:` deleters are only `retire:records:{seg}` drain or supersede/delete of the owning
  *committed* inode (`:222` deleter column); neither fires here (no flip ⇒ no committed inode; and
  the register/backward-compat text never says the restore-fence installs
  `retire:records:{seg:<id>:E}`). The `Aborting` exit reclaims `retire:bytes:` + `sidx:` only
  (`:340`), not `seg:`. Result: the segment **records** dangle unreclaimed (their fragments are
  orphan-marked via `retire:bytes:{session}`, so after grace the records point at deleted
  fragments and just accumulate in `seg:`). The register's restore rows (X17 `:1492`, X17b `:1493`)
  both stipulate an **`Open`** session — the `Completing`-with-segments case is not enumerated.
  Builder fix is document-local: either route restore's `Completing` sessions through the
  rollback-to-`Open` path first (which *does* install `retire:records:{seg:<id>:E}`, §7c/`:1306`)
  then fence `Open → Aborting`, or define a real `Completing → Aborting` restore transition whose
  batch installs the epoch-scoped seg cleanup — and add the matching X-row + §2 edge.

- **NEEDS-HUMAN — `0016:1120-1127`, `:1710`: the derived `MAX_SESSIONS` collapses to ≈ 1 for the
  large-part deployments the launch scope targets — a fitness-to-purpose call the human should
  confirm, not a defect the builder can close.** The arithmetic is honest and computed (leg B(iv)
  is satisfied), so this is **not** a refutation: at `MAX_PART_CHUNKS = 381`,
  `U_ref ≈ 3.82 M` ⇒ `MAX_SESSIONS = ⌊W_ref/U_ref⌋ ≈ 1` at `W_ref = 4 M`, and reaching even 32
  concurrent large-part uploads needs `W_ref ≈ 122 M` chunk-refs ("tens of GB" of reconcile-host
  RAM). The document is upfront about this. But "the release that ships multipart MUST support
  objects over 10 GiB" (§9 scope change) plus "a single fleet may run ~1 concurrent large-object
  upload unless the operator provisions tens of GB of reconcile RAM" is a launch-capacity trade
  the maintainer should explicitly bless — flagging it here so it reaches §6 rather than being
  read as settled by silence. No `[impl]` tag: the builder cannot fix it by iterating; only a
  human can decide the trade is acceptable (or send it back to re-scope `W_ref`/`MAX_PART_CHUNKS`).

## Refutations attempted that I could NOT land (the load-bearing core held)

I attacked each of these with a concrete interleaving and the register/mechanism answered:

- **Late-landing fragment after teardown (X49, `:861-887`).** Tried to strand a fragment
  authorized-before-fence that lands after the reaper deleted its `sidx:` entry. Defeated by
  full-`staged`-placement pre-marking + the **strict** `G_orphan > W_write + δ_clock` coupling
  (I verified GC's grace test is the inclusive `≥` the doc claims, `gc.rs:171-176`), and I could
  not construct a landing time past the position's grace given `t_auth < t_fence ≤ t_mark`.
- **Rollback → re-Complete → publish while the prior `retire:records:{seg}` still pends (X40,
  `:1312-1328`).** Per-attempt (epoch-scoped) `seg:<id>:E:*` vs `seg:<id>:E':*` are disjoint
  ranges; draining epoch-E deletes nothing epoch-E' published. Held.
- **Repoint vs supersede/delete of a committed segmented `seg:` fragment (X47, `:1523`).**
  Destination pre-mark + dual `require(seg==prior) && require(inode==prior)` + drain re-reading
  current placement closes both CAS branches. Held on both interleavings.
- **Exactly-once `mpuctl:count` decrement under gateway-inline vs reaper teardown (X42, `:403`,
  `:1518`).** The `require(mpu:<id> == prior)` session precondition makes the terminal batch
  single-winner; I could not force a double-decrement or a low-drift.
- **Unbounded owned `sidx:` residue from crash-looped parts (F11a, X41, `:940-974`).** `sinf:`
  counts crashed slots (never released while `Open`), so `503` fires at the cap before the
  per-session teardown scan can approach `SCAN_CAP`. Held.
- **Terminal delete racing a re-created owned entry (X43, `:1519`).** Every `sidx:` intent
  carries `require(mpu == Open@E)`; nothing refills the walked-empty range post-fence. Held.
- **Segmented-GET tear when `seg:` records are deleted mid-resolution (X51, `:1400-1422`).**
  Root-flip-before-`seg:`-delete ordering + resolve-retry (absent segment ⇒ re-read root;
  unchanged-root-with-absent-segment ⇒ fail-closed) closes it clock-free. Held.

I also probed the two namespaces the design admits are *not* cardinality-bounded — `retire:`
(paginated `scan_page` + oldest-obligation-age alarm) and the committed reference build (X48,
`W_ref_committed` telemetry). Both are disclosed as capacity/operational costs with a bounded
reclamation path (a backlogged drain is not a *stranding*), so neither is a Refutation-standard
outcome; I note them only to show they were attacked, not as findings.

The ⚑ per-session part-boundary serialization cost (`:1855`) is explicitly kept as a flagged
NEEDS-HUMAN sign-off question per the iteration-5 direction — settled as the human's to rule on,
so I did not spend a refutation on it.

## Net

One concrete, builder-fixable enumeration gap (restore × `Completing`-with-segments) and one
human fitness call (`MAX_SESSIONS ≈ 1` at launch part sizes). The fence/epoch machine,
records-as-proof, restore fence, epoch-scoped segments, exactly-once decrement, and the
byte-budgeted batch inventory survived every interleaving I could build — consistent with the
brief's instruction to preserve them. The gating `T4-batch-review` is already red independently.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C2 Reproduction (red pre-fix) — Decide whether the withheld iteration artifacts establish the content-level red — removing this patch leaves `typos`, docs lint, and link audit green, and the supplied gate manifest has no C2 gate (`check-gates.json:12`).
- [ ] C4 Verification (red→green) — Decide whether CI evidence may be accepted despite the independent rerun stopping at a read-only Cargo advisory-database lock — `typos`, docs lint, render, clippy, build, tests, and machete passed, but full green and a content red→green were not reproduced (`check-gates.json:38`).
- [ ] C5 Causal adequacy — Decide whether per-session `sinf:` CAS plus per-chunk session preconditions is the intended guaranteed-residue-bound mechanism — it preserves the stated bound but imposes the explicitly contested serialization cost (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:1855`).
- [ ] T5 Judgment — Decide whether the protocol’s safety/capacity trade-offs are acceptable for implementation, especially roughly one maximal-part session per 4,000,000-reference budget and the explicit serialization-cost question (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:1710`; `docs/design/proposals/draft/0016-multipart-commit-protocol.md:1869`).
- [ ] Validation — fitness-to-purpose — Decide whether this draft is fit to unblock #508 and bind #625 — mechanical prose checks cannot validate the protocol’s refutation standard or architecture-governance acceptability (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:126`).
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 7 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: discontinued
- Iteration delta (if iterating): Pulled out of the automated PDCA loop after 7 iterations that all die on the same gating T4 gate (this run: 7 blocking, 0 recorded-rejected; overall = fail). The design core is sound — the adversary could not refute the fence/epoch state machine, the O(1) session-precondition publication proof, the restore fence, epoch-scoped segments, the exactly-once terminal decrement, or the byte-budgeted batch inventory. The remaining defects are document-local and cluster on one mechanism (the `sinf:` in-flight-part counter: retry-bound / slot-before-lease / drain-CAS / X52 compensation-vs-reaper) plus the adversary's restore × `Completing`-with-segments enumeration gap. Rather than run an 8th headless iteration on the same finding class, the work moves to interactive, by-hand refinement of the draft. Where the work goes: continue editing `docs/design/proposals/draft/0016-multipart-commit-protocol.md` interactively (stays at status: draft; not shipped via this cycle). The six open findings are recorded in §10 of SUMMARY.md as the punch list. Two standing human-only calls to adjudicate during that hand-refinement: the `MAX_SESSIONS ≈ 1` launch-capacity trade (bless, or re-scope `W_ref`/`MAX_PART_CHUNKS`) and the per-session part-boundary serialization cost. #508 stays blocked on this document; #625 stays its reaper implementing slice.
- By / date: Eduard Ralph / 2026-07-23

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
- issue_626 — T4: `sinf:` retry bound false under continuous churn; claimed ≤`MAX_INFLIGHT_PARTS` termination can livelock/starve a valid part commit (0016:908, :218).
- issue_626 — T4: `UploadPart` reserves its slot before its owned `sidx:` lease exists, so a slow pre-first-chunk request can be falsely reaped after `W_open` (0016:1004).
- issue_626 — T4: owned-`sidx:` drain has no CAS precondition; concurrent gateway+reaper drainers double-orphan-mark and re-stamp grace, breaking grace-preserving idempotency (0016:400).
- issue_626 — T4: X52 "session-left-`Open` ⇒ compensation" contradicts Decision 5 / X7 reaper-owned cleanup and permits a double-release of `sinf:` (0016:1528).
- issue_626 — adversary [impl]: restore-fencing a `Completing` session that already wrote segments is an undefined transition; stray `seg:<id>:E:*` records have no reclamation path — outcome (a), absent from the register (0016:522/:1816/:396/:1492).
- issue_626 — adversary human-fitness call: derived `MAX_SESSIONS` ≈ 1 at launch part sizes; 32 concurrent large uploads needs ~122M chunk-refs / tens of GB reconcile RAM (0016:1120-1127/:1710).
