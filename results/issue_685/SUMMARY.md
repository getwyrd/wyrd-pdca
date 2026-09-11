# Result — issue 685 / dependabot-advisories-tikv-boundary

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: #685 recorded five open Dependabot alerts on `main` and asserted that *"the dependency
  wall does not see any of them"* — a HIGH-severity advisory invisible to the gate that exists to
  catch exactly that. **The second half of that assertion is no longer true**, and the first half is
  accounted for. Every one of the five reaches the graph through exactly one crate, `tikv-client`
  0.4.0, which is pulled in only by the **off-by-default `tikv` feature**; the shipped default
  artifact contains none of the vulnerable versions. `cargo xtask ci` audits that off-by-default
  tree on every PR through a second `cargo deny` invocation over `deny-all-features.toml`
  (`xtask/src/lib.rs:140-172`), where all five are recorded under their RUSTSEC identifiers with a
  stated exposure boundary, a no-fix-available rationale, and a dated review trigger. Nothing is
  unfixed here that a fix could reach; what remains is a wording question, stated below.
- Success criterion: the close is **complete and correct** — every element of #685's stated
  acceptance is either satisfied on `origin/main @ 65ca4fd` or explicitly dispositioned in the
  mapping below, with no element left unassigned. Re-checkable at sign-off with three commands:
  `gh api repos/getwyrd/wyrd/dependabot/alerts -q '.[]|"\(.number) \(.state) \(.dismissed_reason)"'`
  (expect five `dismissed` / `tolerable_risk`), `grep -c '^    { id = ' deny-all-features.toml`
  (expect 8 — two inherited from `deny.toml` plus the six tikv entries), and
  `cargo deny --all-features --config deny-all-features.toml check advisories` (expect green).
- Repo + branch target: getwyrd/wyrd @ main   (**No PR opens from this bundle** — publish exits
  0 with *"nothing to contribute; close the tracker item by hand"* (`publish.py:161-166`), so the
  tracker action is the human's, at sign-off.)
- Scope: record the accounting for #685's five advisories and the corpus-gap question, and close
  the tracker item. / **out of scope:** bumping any dependency (no reachable fix exists — see the
  mapping); adding a second advisory source or a Dependabot-severity CI step (that is the residual
  decision below, and if taken it is its own item, not this one); re-opening #543's or #547's
  settled two-config split; any change to `deny.toml`, `deny-all-features.toml` or `xtask`.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-close
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — N/A — close disposition (no patch to verify)
- C3 Change: none — patch.diff
- C4 Verification (red→green): none — N/A — close disposition (no patch to verify)
- C5 Causal adequacy: none — reviewer + human sign-off

## 4. Conformance (Check — stack)
- T1 Structure: none — N/A — close disposition (no patch to verify)
- T2 Shape: none — N/A — close disposition (no patch to verify)
- T3 Runtime: none — N/A — close disposition (no patch to verify)
- T4 Contribution: none — N/A — close disposition (no patch to verify)
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

# Advisory review — SKIPPED (close disposition)

The reviewer leaf was skipped: this bundle's Plan concluded a close / no-fix disposition (likely-close), so there is no patch to review.

- NEEDS-HUMAN — Confirm the close disposition 'likely-close' (no patch was built). Override to a fix path (iterate-to-Do) if the close is wrong.


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] Confirm the close disposition 'likely-close' (no patch was built). Override to a fix path (iterate-to-Do) if the close is wrong.

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
- By / date: Eduard Ralph / 2026-08-17

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
