# build-notes — #937 validate-capability-matrix (stopped at the base check, second time)

**Supplied `HEAD`: `93a6f3715e8785a22fd1d1ab40206bcf51b3eef4`** (`pdca-integrate: issue_932`,
the tip of `pdca-integration/main-r42c5348f203d`). **Base below `9b1a3e2`.**

No code was written. No `patch.diff`, no test file. Built on: nothing.

**This is the second Do attempt in a row to stop here.** The previous `build-notes.md` in this
bundle (written 12:47 today, same wave) recorded the same `HEAD` and the same failed check;
`loop-telemetry.json` shows two attempts numbered 10. Nothing about the worktree changed between
them. A further iterate-do without a base fix will stop the same way, so the auto-iterate budget
(`auto-iterate.json`: `{"count": 5}`) should not be spent on this; the base must be fixed first.

## Why I stopped

The brief makes this Do's first step, before any code (brief.md:192-195):

> in the worktree, run `git merge-base --is-ancestor 9b1a3e2 HEAD`. If it fails, stop: write
> no code, put the supplied `HEAD` and "base below `9b1a3e2`" at the top of `build-notes.md`,
> and do not rebase, merge `main`, or repair `Cargo.lock` / `deny*.toml` to get past it.

It failed. What I ran, in `$PDCA_WORKTREE` = `/home/eddie/wyrd/wyrd.pdca-wt-l0`:

```
$ git rev-parse HEAD
93a6f3715e8785a22fd1d1ab40206bcf51b3eef4
$ git merge-base --is-ancestor 9b1a3e2 HEAD; echo $?
1
$ git cat-file -t 9b1a3e2
commit                                         # the commit is present locally
$ git rev-parse origin/main
9b1a3e202f2cc89a113b7790a3b9cb3033654a9e      # Merge pull request #947 …
$ git merge-base --is-ancestor HEAD 9b1a3e2; echo $?
1                                              # HEAD is not below main either; the lines diverged
$ git merge-base HEAD 9b1a3e2
6a85eaaf2f80dee5c2b29a28126c470d4ca03e4d
$ git log --oneline 6a85eaa..9b1a3e2 | wc -l
34
```

So `9b1a3e2` is present and is `origin/main`, but the worktree was cut from the old integration
line, which forked from `main` at `6a85eaa`. `main` has 34 commits past that fork, among them
exactly the ones the brief requires (brief.md:180-184): `955b9cb` (PR #934, #853), `a7802b8`
(PR #942, #932), `6a0b369` (RUSTSEC-2026-0385), `6c79af3` (RUSTSEC-2026-0386), and PR #931
(#744) via the earlier merges. The integration line `93a6f37` is a fold of #744, #853 and #932
onto the older `main`; every one of those is now merged on `main`, so the line is fully
superseded. It is also the line on which iteration 9's two all-features advisory steps of C4-ci
were red; building here would repeat that red for a reason unrelated to the matrix.

I did not rebase, merge, or touch `Cargo.lock` / `deny*.toml`, as instructed.

## What went wrong in the harness (facts, not a diagnosis)

- The bundle still holds `stack-base` (`pdca-integration/main-r42c5348f203d`) and
  `stack-base-tip` (`93a6f37…`), both dated Oct 23 02:10. The brief (brief.md:185-192) says
  that stale file is what pinned `93a6f37`, and expected the driver to delete it at the top of
  the wave (`flow.py:2204-2209`, `:784-790`) unless this run had already folded onto
  `getwyrd/wyrd main`. It was not deleted before either Do attempt today.
- The whole wave is on the stale line, not just this worktree: `git worktree list` shows
  `wyrd.pdca-wt-l0`, `-l1`, `-l2`, `pdca-verify-l0/l1/l2` and `wyrd.pdca-integ-main` all at
  `93a6f37`. The `pdca-cov-l0` worktree alone sits at `9b1a3e2`. So this is a bundle/driver-level
  base problem; fixing one worktree by hand would not fix Check's verify worktrees.

If the stale `stack-base` was kept because the run *had* folded onto `main`, then the fold
itself is on the wrong tip (the integration branch tip is `93a6f37`, below `9b1a3e2`). Either
way the human, not Do, decides how the base is refreshed. Per the project's standing rule, if
this turns out to be a driver defect it goes upstream to `eduralph/pdca-harness`.

## NEEDS-HUMAN

NEEDS-HUMAN external dependency: base at or after 9b1a3e2 — the Do worktree was cut at 93a6f37 (stale `pdca-integration/main-r42c5348f203d`), so the brief's stop rule (brief.md:192-195) forbade writing any code; no patch, no test, no red→green evidence exists for (A)–(H). Second consecutive attempt to stop here; the base must be refreshed before another iterate-do.

```toml
[[doctor.checks]]
id    = "base at or after 9b1a3e2"
cmd   = "git -C \"${PDCA_WORKTREE:?}\" merge-base --is-ancestor 9b1a3e2 HEAD"
hint  = "The Do worktree must be cut from origin/main (9b1a3e2) or a line built on it. Remove the stale results/issue_937/stack-base and stack-base-tip (pdca-integration/main-r42c5348f203d, tip 93a6f37) so the next fold starts from origin/main; do not rebase or merge inside the worktree by hand."
level = "MISSING"
```

If `doctor` runs before the worktree exists, `$PDCA_WORKTREE` is unset and the `:?` makes the
check fail loudly instead of passing by accident; the same test then belongs where the worktree
is cut (`worktree.py:93-98` per the brief).

## What the human can do to let the next iterate-do build

1. Drop the stale `results/issue_937/stack-base` and `stack-base-tip` (the brief says they are
   stale: `pdca-integration/main-r42c5348f203d`, tip `93a6f37`) by whatever `pdca` command owns
   them, so the next wave folds from `origin/main` (`integrate.py:298-300` per the brief).
2. Re-run Do (`iterate-do`). The next attempt re-runs the ancestry check first; with `HEAD` on
   or above `9b1a3e2` it proceeds to the build.
3. If the driver re-creates the stale base on its own, that is a harness defect; file it
   upstream rather than working around it in the worktree.

## Housekeeping the human should know about

- `results/issue_937/capability_matrix.rs` at the bundle's top level (43 172 bytes, Oct 24
  19:07) is **iteration 9's** test, not this attempt's. This attempt shipped no test. Nothing at
  the top level is mine except this file.
- I created no scratch directories and wrote nothing inside the worktree.

## Refutation record (a)–(c)

Not applicable: no test and no patch were produced, so there is nothing to revert, nothing
driving production code, and no fixture. Recorded here so the sign-off does not mistake the
absence for an omission.

## Which child this attempt was for

The brief is the re-planned parent with the split (child-1 core / child-2 required-row checks,
brief.md:305-322) and I was invoked on the parent bundle `issue_937`. I did not have to resolve
which child to build because the base check stopped the attempt first. When the base is fixed,
the next Do should still check whether it is being run on the parent or a child bundle: one patch
for both children was ruled out by the brief (iteration 9 was 99.6 KB against the 100 KB
backstop).
