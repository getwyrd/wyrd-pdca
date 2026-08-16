# Batch run plan — 0.1 Alpha ∥ blackbox-tester (2026-08-16)

Commands to run by hand, in order. One `pdca flow` process at a time — two concurrent flows
would collide in the `$PDCA_LANE` worktrees. `lanes = 2` with `wave_mode = "merge"` pairs one
alpha bundle with one blackbox bundle per wave on its own; the two streams touch disjoint code.

Working directory is **`~/wyrd/wyrd-pdca`** (there is no `~/development/wyrd/wyrd-pdca`).
The driver is `./scripts/pdca` — there is no `pdca` on `PATH` in a non-login shell.

---

## Step 0 — done: the checkout is current

The primary worktree was on `fix/209-review-input-ceiling`, **already merged** (HEAD was an
ancestor of `origin/main`) and **24 commits behind**. It is now on
**`chore/batch1-housekeeping`**, branched off `origin/main` @ `b38c68a`, which carries this
housekeeping commit.

`main` itself was already current — it is checked out in the **`wyrd-pdca-integration`**
worktree (`git worktree list`), which is why the primary directory cannot hold it and why a
plain `git checkout main` here fails with *"already used by worktree"*. That is the layout, not
a fault: work in this directory happens on a branch, and every commit on `main` arrives through
a PR merge — bundle records included (`chore/settle-651-652-bundles` → PR #210).

Three of the 24 pulled-in commits change how this exact batch runs:

| Commit | What changes |
|---|---|
| `365d2ac` + `a890981` | **`auto_merge` is back `true`** (2026-08-16). The driver merges each non-final wave's PRs itself, so a dependent chain runs to completion in **one** invocation instead of stopping at every wave boundary for a hand-merge. |
| `1179b8d` / `048dae6` | New `merge_sync_base = true` — a PR behind its base is brought up to date *before* the rollup gate, so the checks describe the tree the PR actually merges into. Plus `merge_wait_secs = 1800`, letting the rollup settle before `merge_requires = "all"` reads it. |
| `2be1e85` … `3c7d200` | New advisory gate **`C4-diff-cov`** (diff coverage). Host prerequisites verified present here: `cargo-llvm-cov 0.8.7` and `llvm-tools` on the pinned 1.96.0 toolchain. Advisory, never gating. |

Run stale and you would get the old serialised behaviour and no diff-cov signal. Worth one
confirmation before the first batch:

```sh
./scripts/pdca doctor
```

**Note on the house rule.** "Never `gh pr merge` yourself outside what the driver's `wave_mode`
does" now covers more ground than it did last week: the driver merges non-final waves again. The
final wave's PRs still wait for you.

---

## Housekeeping

| # | Item | Status |
|---|---|---|
| 1 | Publish #638 | **Yours to run** — verified ready, command below |
| 2 | Audit pre-written briefs for base / `Stacks on:` | **Done — clean, no changes needed** |
| 3 | Reconcile stale bundle state (651/652, selftest) | **Nothing to do — already reconciled** |
| 4 | #685 close-disposition brief | **Done — written, ready to run** |

### 1. Publish #638 — yours

```sh
./scripts/pdca publish 638
```

Verified ready: §6 has all twelve NEEDS-HUMAN items ticked, §9 records `Outcome: merged-wider`
(the harness's token for *accepted*, `signoff.py:22`) by Eduard Ralph / 2026-07-26. It is
genuinely unpublished, not already-landed — `origin/main`'s `chunk.proto` carries no deadline
field and `crates/traits/src/lib.rs` has no `put_fragment` deadline, so this opens real work,
not a duplicate PR. It gates #661 (shared custodian test files).

The publisher leaf is `interactive = true`, so this needs a real terminal.

One thing to decide while you are there: **#638 has no milestone** on GitHub. Every sibling in
this stream is on `0.1 Alpha`. Worth setting before the PR opens.

### 2. Brief audit — clean

Every brief for a slice that has not yet built names **`getwyrd/wyrd @ main`**, and **not one
carries a `Stacks on:` value**. Audited: #508, #625, #633, #637, #655, #682, #692, #693, #711,
#721, #722. Nothing to fix.

Three `Stacks on:` hits exist in the tree — `issue_195`, `issue_196`, `issue_250` — all are
**empty template fields** in bundles that are COMPLETE or DISCONTINUED. The non-`main` bases in
`issue_253`–`258`, `365`, `406`, `419`, `477` are the M4-era slices that deliberately targeted
`feat/m4-production-metadata-backend`; all COMPLETE, all historical. Neither set is a leftover.

**#653 has no brief and no bundle at all** — it was never written, so there was nothing to audit.
It needs a fresh Plan when batch 3 runs.

One thing the audit *did* surface, flagged rather than edited: several briefs (#721, #722, #655,
#682) carry prose explaining that `auto_merge = false` means the driver stops at the wave boundary
for a hand-merge. After step 0 that prose is stale. It is free text, not a machine-parsed field, so
it changes no scheduling — but it will mislead a reader, and the builder reads the brief.

### 3. Stale bundle state — nothing to reconcile

Both reported items had already been settled:

- **#651 / #652** read **COMPLETE**, not CHECKED, with PRs
  [#688](https://github.com/getwyrd/wyrd/pull/688) and
  [#689](https://github.com/getwyrd/wyrd/pull/689) both **MERGED** 2026-08-05 and both tracker
  issues CLOSED. Commit `b3da404` (*chore(bundles): settle 651 and 652 at COMPLETE*) did this.
  All three harness checkouts — `wyrd-pdca`, `wyrd-pdca-197`, `wyrd-pdca-integration` — agree.
- **`issue_selftest` does not exist**, in any of the three checkouts. There is no
  AWAITING_SIGNOFF bundle anywhere; all 120 bundle directories are `issue_<number>`. The four
  bundles that still carry unticked §6 items — #153, #250, #626, #636 — are all DISCONTINUED,
  which is the expected end state for an abandoned bundle.

### 4. #685 — brief written, yours to run

```sh
./scripts/pdca flow 685
```

`results/issue_685/brief.md` is written and the bundle now reads **PLANNED**.
`Disposition hint: likely-close` resolves through `close_class()` (verified), so the builder and
reviewer leaves are skipped and it routes straight to sign-off. No PR opens; the tracker close is
yours by hand.

The close case is stronger than "the alerts were dismissed". All five trace to `tikv-client`
0.4.0, which only the **off-by-default `tikv` feature** pulls, so none is in the shipped artifact —
and all five are *already* caught and waived with reasons in `deny-all-features.toml`, under their
RUSTSEC identifiers, by a `cargo deny` invocation that runs on every PR:

| Alert | Package | Already waived as |
|---|---|---|
| GHSA-82j2-j2ch-gfr8 (HIGH) | `rustls-webpki` 0.101.7 | RUSTSEC-2026-0104 |
| GHSA-xgp8-3hg3-c2mh (LOW) | `rustls-webpki` 0.101.7 | RUSTSEC-2026-0098 |
| GHSA-965h-392x-2mh5 (LOW) | `rustls-webpki` 0.101.7 | RUSTSEC-2026-0099 |
| GHSA-2gh3-rmm4-6rq5 (MED) | `protobuf` 2.28.0 | RUSTSEC-2024-0437 |
| GHSA-cq8v-f236-94qc | `rand` 0.7.3 | RUSTSEC-2026-0097 |

**Read this before you accept.** #685 has two halves and the brief closes them unevenly. The
five alerts are fully accounted for. The second half — *"`deny.toml` either checks the GitHub
corpus or documents that it does not"* — is only **partly** met, and the issue itself calls it the
more important one. `deny.toml`'s header names RUSTSEC as its corpus five times and sends the
reader to `deny-all-features.toml`, but never states the actual blind spot in those words: a GHSA
advisory carrying no RUSTSEC identifier is not matched by ID, and Dependabot triage is the process
guarantee covering it. I did not decide that for you — the brief puts three options at sign-off
(close as-is / close plus a follow-up documentation item / reopen to a fix path for a second
advisory source). Nothing in it argues that a partially-met acceptance is fully met.

Worth knowing, because it cuts against the issue's framing: RustSec catches `RUSTSEC-2025-0134`
(`rustls-pemfile`, same chain) which Dependabot never reported. The corpora differ in both
directions.

---

## Batch 1

Your line was `pdca flow 721 722 717 692 693 740 741 742 736 738`. **#717 cannot go in as an id** —
see below. Run the split first, then the batch.

### 1a. Accept #717's split — it is proposed but not accepted

```sh
./scripts/pdca split 717 --accept
```

`results/issue_717/split-proposal.md` exists (authored 2026-08-14, after you answered
`iterate-plan` on iteration-v3) but is **untracked and unaccepted**: #717 has no brief and **no
`close-disposition` marker**, so it reads UNPLANNED. Feeding `717` to `flow` would drop you into
an interactive Plan for a slice three Do rounds already failed to fit — 123 KB / 1,780 lines
across 12 files against a ≤960-line budget, with `T4-batch-review` red in all three rounds.

The proposal splits it by record namespace into two children, `child-2 Depends on: child-1`:

- **child-1** `multipart-retire-obligation` — the `retire:` obligation value, ~3 files / ~1,000 lines
- **child-2** `multipart-owned-staging-entry` — `sidx:` + the `PendingEntry` extension, ~13 files / ~1,250 lines

`--accept` files both as tracker sub-issues of #717 and materialises their bundles. **Three things
it cannot do for you**, two named in the proposal and one that follows from a known harness bug:

1. Add `- **Conflicts with:** 721, 722` to **child-2's** `brief.md`. Child-2 shares
   `crates/core/src/metadata.rs` with #721 and `crates/dst/tests/custodian.rs` with #722 and must
   never share a wave with either. A proposal's ordering fields may only name sibling labels, so
   this could not be declared there. Child-1 touches neither file and needs nothing.
2. Repoint **#693** — its brief reads `Depends on: 717` (`results/issue_693/brief.md:77`) — at
   **child-2**, the terminal child. #655 sits behind #693 and follows automatically.
3. Hand-write a parent `brief.md` for #717 with `Disposition hint: likely-close`, or it never
   freezes. This is upstream bug **eduralph/pdca-harness#481**: a split parent whose brief was
   archived by an earlier `iterate-plan` reads UNPLANNED *despite* its close marker and re-enters
   interactive Plan on every run. `split` is deliberately not in `[driver].close_dispositions`.
   `results/issue_711/brief.md` is the working precedent — copy its shape. Confirmed still
   unfixed: none of the 24 pulled commits touches `state.py`, `split.py` or `driver.py`.
   **`results/issue_654` is stuck in exactly this state too** (close marker `split`, no brief,
   UNPLANNED). It is on your do-not-feed list so it harms nothing, but it will keep showing up as
   UNPLANNED noise until it gets the same treatment.

### 1b. Run the batch

```sh
./scripts/pdca flow 721 722 <child-1> <child-2> 692 693 740 741 742 736 738
```

Substitute the two issue numbers `split --accept` prints. Notes on what is already in flight:

- **#692 is CHECKED**, not unstarted — it has been built and reviewed, so `flow` resumes it at
  sign-off rather than rebuilding. Nothing to do differently; do not expect a Do pass.
- **#721 / #722 / #693 / #692 have briefs**; **#740, #741, #742, #736, #738 have none** and will
  each open an interactive Plan.
- Ordering is already declared and correct: `#722 Depends on: 721`; `#721` and `#722` both carry
  `Conflicts with: 717`; `#692 Depends on: 691` (COMPLETE, PR #703 merged). With `auto_merge` back
  on, the whole chain runs in one invocation.

---

## Follow-on batches

One flow invocation each, alpha ∥ blackbox. Only **#655**, **#625** and **#633** already have
bundles — every other id below opens a fresh interactive Plan.

```sh
# batch 2
./scripts/pdca flow 655 656 657 743 744 745 746 747 748

# batch 3   — see "chained pairs" below before running 653→658
./scripts/pdca flow 653 658 659 660 749 750 751 752

# batch 4   — stand up #737 (Hetzner) during this one
./scripts/pdca flow 661 662 663 664 665 753 754 755 756 757 758

# batch 5   — BLOCKED, see below
./scripts/pdca flow 625 633 759

# batch 6   — 666→671 are serial, all edit gateway-s3
./scripts/pdca flow 666 667 668 669 670 671 760 761 762 763 764
```

### Clear before batch 5 — a confirmed blocker

**#625 and #633 will both be skipped if you run them as-is.** Both declare `Depends on: 636`
(`issue_625/brief.md:165`, `issue_633/brief.md:145`) and **#636 is DISCONTINUED**. `flow.py:661`
is explicit: *"a prereq DISCONTINUED earlier never gets there, and its dependent is skipped
loudly."* #636's work was re-sliced, so the dependency is stale rather than real — repoint or drop
it in both briefs before the batch. **#508 and #637 carry the same stale `Depends on: 636`**; they
are on your do-not-feed list, so they bite only if that changes.

### Chained pairs — the addendum's open question, still open

The reslicing addendum flags **#653→#658** and **#625→#633** as pairs that merge mode cannot hold
together: the consumer builds on the merged producer, so the producer lands on `main` a full
build-cycle earlier. `auto_merge = true` does **not** resolve this — the objection is about merge
*atomicity*, not about who does the merging. The addendum's two options stand, and it asks you to
decide per pair at its Plan pass: fold each pair into a single bundle, or pull the pair out of the
driver and hand-stack the two PRs. There is no per-bundle mode override.

---

## Standing notes

**Do not feed to flow** — seam trackers: `#508 #513 #635 #636 #637 #654 #682 #711`. #625 goes in
only as the real issue in batch 5. **#739 is HELD** pending the segmented-root-ceiling architecture
question — do not brief it.

**Fillers for idle lanes:** `560 585 596 674 684 686 687 690 766 767 768`; `511`/`512` after 508.5.

**Expect NEEDS-HUMAN at:**
- **#741 sign-off** — `aws-sdk-s3` becomes a shipped dependency, so ADR-0003's licence wall
  applies to a graph it has not judged before. Related and worth having to hand: `deny.toml`
  already waives `RUSTSEC-2026-0253` (`lru` via `aws-sdk-s3`) as **dev-only**. Once the SDK ships,
  that "never in shipped code" justification stops holding and the waiver needs re-arguing.
- **#759** — the TLS-provider question.

**House rules in force:** no AI attribution anywhere; DCO `-s` on every commit; every PR links an
issue; scratch is `/var/tmp/pdca`, never `/tmp` (12G target dirs against a 31G tmpfs).

**Committed with this doc** on `chore/batch1-housekeeping`: `results/issue_685/brief.md` (new)
and `results/issue_717/split-proposal.md` (authored 2026-08-14, untracked until now). Not pushed —
open the PR when you are ready.
