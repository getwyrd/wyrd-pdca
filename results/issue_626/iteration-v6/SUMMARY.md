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
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of issue #626’s docs-only settlement of the multipart commit, reclamation, reaper, and segmented-map protocol needed before #508 can be implemented.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The human has an implementable settlement bar covering seven decisions, F1–F18, computed bounds, sequencing, and exactly two documentation paths; the proposal identifies that complete decision surface at `docs/design/proposals/draft/0016-multipart-commit-protocol.md:17`. |
| C2 Reproduction (red pre-fix) | PASS | At base `cd82a29` the numbered proposal is absent, so #508 has no protocol artifact to consume; the base prose gate is green, confirming that the relevant red is criterion absence/judgment rather than a mechanical docs failure (`docs/design/proposals/README.md:22`). |
| C3 Change | PASS | The human must decide whether the requested design artifact is the whole change—inspection found only the new draft and its index row, with draft metadata grounded at `docs/design/proposals/draft/0016-multipart-commit-protocol.md:1`. |
| C4 Verification (red→green) | NEEDS-HUMAN | Accept the mechanical evidence despite the host caveat—the patched `typos`, docs lint, and 98-page render/link audit passed, but full `cargo xtask ci` stopped at `cargo deny` because its advisory DB lock path is read-only, and no per-fix test exists for this docs-only design (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:1697`). |
| C5 Causal adequacy | NEEDS-HUMAN | Decide whether guaranteed residue bounds justify per-session part-boundary serialization—the proposal explicitly leaves that D-C/D-D cost contested, and weakening it can reopen the unbounded-owned-residue cause at `docs/design/proposals/draft/0016-multipart-commit-protocol.md:1817`. |
| T1 Structure | PASS | The human can ratify one editable proposal rather than a split register: the document keeps the required draft frontmatter and consolidates the seven-decision protocol in the requested vehicle (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:1`). |
| T2 Shape | NEEDS-HUMAN | Decide whether a protocol is settled while its enforcement mechanism remains an open sign-off choice—the alternatives at `docs/design/proposals/draft/0016-multipart-commit-protocol.md:1831` materially change the strength or cost of the F11a bound. |
| T3 Runtime | N/A | No runtime is changed; implementation and seeded DST obligations are explicitly assigned to #625/#508 at `docs/design/proposals/draft/0016-multipart-commit-protocol.md:1697`. |
| T4 Contribution | NEEDS-HUMAN | Triage the four blocking fresh-review findings before relying on contribution quality—`check-gates.json` records the batch-review red, but its finding artifact/command output is not among the three reviewer inputs and could not be independently reproduced here. |
| T5 Judgment | NEEDS-HUMAN | The architecture/founding-maintainer must accept both the protocol’s operational trade-offs and its explicitly flagged serialization cost before this draft can become the implementation authority (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:41`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether this design is fit to unblock #508—the mechanical prose checks establish document integrity, not that every crash/race/maintenance execution is adequately settled (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:102`). |

### Advisory — adversary

# Advisory review — adversary — NOT COMPLETED

<!-- pdca:leaf-status human-empty -->

Failure class: **substantive — needs a human.** The leaf ran but did not yield a usable verdict; do not assume an infra blip. See `check-advisory-adversary.error.log` in this bundle for the captured error.

- NEEDS-HUMAN — advisory leaf 'adversary' did not produce findings (leaf failed: Command '['claude', '-p', '--agent', 'adversary', '--permission-mode', 'acceptEdits', '--allowedTools', 'Read,Bash,Grep,Glob', '--add-dir', '/home/eddie/development/wyrd/wyrd.pdca-wt', '--output-format', 'stream-json', '--verbose']' returned non-zero exit status 1.); re-run it or adjudicate by hand.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Accept the mechanical evidence despite the host caveat—the patched `typos`, docs lint, and 98-page render/link audit passed, but full `cargo xtask ci` stopped at `cargo deny` because its advisory DB lock path is read-only, and no per-fix test exists for this docs-only design (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:1697`).
- [ ] C5 Causal adequacy — Decide whether guaranteed residue bounds justify per-session part-boundary serialization—the proposal explicitly leaves that D-C/D-D cost contested, and weakening it can reopen the unbounded-owned-residue cause at `docs/design/proposals/draft/0016-multipart-commit-protocol.md:1817`.
- [ ] T2 Shape — Decide whether a protocol is settled while its enforcement mechanism remains an open sign-off choice—the alternatives at `docs/design/proposals/draft/0016-multipart-commit-protocol.md:1831` materially change the strength or cost of the F11a bound.
- [ ] T4 Contribution — Triage the four blocking fresh-review findings before relying on contribution quality—`check-gates.json` records the batch-review red, but its finding artifact/command output is not among the three reviewer inputs and could not be independently reproduced here.
- [ ] T5 Judgment — The architecture/founding-maintainer must accept both the protocol’s operational trade-offs and its explicitly flagged serialization cost before this draft can become the implementation authority (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:41`).
- [ ] Validation — fitness-to-purpose — Decide whether this design is fit to unblock #508—the mechanical prose checks establish document integrity, not that every crash/race/maintenance execution is adequately settled (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:102`).
- [ ] leaf produced no usable verdict (needs a human) — advisory leaf 'adversary' did not produce findings (leaf failed: Command '['claude', '-p', '--agent', 'adversary', '--permission-mode', 'acceptEdits', '--allowedTools', 'Read,Bash,Grep,Glob', '--add-dir', '/home/eddie/development/wyrd/wyrd.pdca-wt', '--output-format', 'stream-json', '--verbose']' returned non-zero exit status 1.); re-run it or adjudicate by hand.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_

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
- Iteration delta (if iterating): Rejected at sign-off (issue_626): gating T4-batch-review red — 4 blocking, 0 recorded-rejected. A fresh adversary pass (re-run on opus[1m]; the driver's leaf had failed to produce a verdict) corroborated the two load-bearing findings as builder-fixable [impl] and could NOT refute the design's core. Rework the document — do NOT restart. PRESERVE (adversary attempted and could not refute — do not redesign): the fence/epoch state machine; restore fence-then-serve (X17/F13); per-attempt epoch-scoped seg: keys (X37/X40/F18); the committed-object repoint-vs-supersede armor (X47); the exactly-once terminal decrement and counter-only-collision handling (X42/X52); the segmented-GET resolve-retry rule (X51); byte-budgeted batch inventory; retire:/reference-build bounded-cost dispositions (X39/X48). FIX (every T4 finding must be fixed so it leaves the next run, or recorded-rejected in review-rejected.md — the gate blocks while any is unchecked): 1. (T4 #1 / adversary, 0016:523) Staged-part reconstruction re-place strands the rebuilt destination fragment on a lost CAS — outcome (a), permanent under Defer/GC (gc.rs:183-187): the fragment is written to P_new before the CAS, and a Complete/Abort/reap fence in that window fails the CAS, leaving P_new referenced by nothing and evidenced by nothing. Extend the committed branch's destination pre-mark rule (X47) to the STAGED re-place path, and correct the X29 register row that mischaracterizes this as safe. 2. (T4 #2 / adversary, 0016:217/:388) mpuctl:count has no bootstrap — first CreateMultipartUpload on a fresh/upgraded store cannot satisfy require(mpuctl:count == c). Define the absent-as-zero read (or a one-time init batch) and add its round-trip/first-create observable. 3. (T4 #3, 0016:780) G_orphan == W_write boundary race: tighten to G_orphan strictly > W_write with clock-resolution margin, OR record-reject with the adversary's t_mark > t_authorize reasoning (deferable to #625's knob choice) in review-rejected.md. 4. (T4 #4, 0016:1532) Summary line wrongly attributes the late-write bound to lease-renewal refusal; the bound is the fail-closed W_write timeout (Decision 5). Correct the doc-consistency slip. ADJUDICATE (adversary note, not scored): state normatively whether a bulk DeleteObjects (#509) must byte-budget its obligation-installation across transactions (1,000 large generations x ~V ≈ 100 MB, over-envelope), or explicitly assign that contract to #509. Do not re-attempt the rejected approach unchanged.
- By / date: Eduard Ralph / 2026-07-23

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
- issue_626: pin `--model` on the `pdca.toml` adversary leaf (and other `model: inherit` leaves) — the account's Fable promo/overage default silently displaced `opus[1m]`, so the headless adversary leaf failed twice (`Execution error`/hang) and only ran once `--model "opus[1m]"` was pinned; mirror the builder leaf's `--model sonnet`.
