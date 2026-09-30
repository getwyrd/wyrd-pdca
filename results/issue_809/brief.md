# Brief — issue 809 / restore-session-fence (split parent)

> The Plan artifact (docs 02 §PLAN), written by `split --accept` (issue #481):
> this bundle had no brief.md when its split was accepted — an iterate-to-Plan
> had archived it to `iteration-v1/brief.md`.

- **Slug:** restore-session-fence
- **Defect:** decomposed instead of built as one cycle: the slice was judged to be more
  than one shippable outcome. The seams are set out in `split-proposal.md`; the
  original defect and scope are in `iteration-v1/brief.md`.
- **Success criterion:** the slice is decomposed, not built here — the child bundles
  issue_839, issue_840, issue_841, issue_842, issue_843 each carry their own brief, and together they cover
  the goal of `iteration-v1/brief.md`. No patch lands in this bundle; each child is verified
  by its own cycle.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Scope:** decomposition only: no patch, test or gate run belongs to this bundle. / out
  of scope: building any part of the original slice here — the child bundles
  carry that work.
- **External dependencies:** none
- **Disposition hint:** split
