# Adversarial review — issue_681 (advisory, non-gating)

Bundle ships **no code** (disposition `split`), so there is no red→green to re-run: the artefact
under attack is the accepted split and the claim that closing #681 leaves nothing behind. Every
citation below is grounded on `$PDCA_TARGET` (`/home/eddie/wyrd/wyrd.pdca-wt-l0`, worktree at the
Plan base `339da46`; post-child state read via `git show origin/main:` — `origin/main` is now
`9dbcd72`, i.e. the three children have already merged).

## Refutations

- **NEEDS-HUMAN [human] — the parent's "settled, no round relitigates them" rule set did not
  survive the split, and the close comment claims otherwise.** `brief.md:113` pins Rules A–E as
  *"settled here; no Do round and no review round relitigates them"* and `brief.md:128` makes Rule A
  a MUST-bind for **every** child. In fact **Rule A and Rule C were removed from all three children**
  at their own re-plans and re-tracked as **#698** (OPEN — *"backfill reads a record at one inode key
  and CASes it at another"*) and **#699** (OPEN). This is verifiable in the shipped code, not just in
  paperwork: the Rule C construct the brief called a defect at `crates/custodian/src/backfill.rs:142`
  (base) is **byte-for-byte still there** after #695 — `parse_inode_key` still accepts a
  non-canonical spelling (`git show origin/main:crates/custodian/src/backfill.rs`, `:70-76`;
  `"007".parse()` and `"+3".parse()` both succeed) and the CAS is still re-derived at `:249`
  (`let inode_key = metadata::inode_key(inode_id);` → `.require(inode_key.clone(), encode(&record))`
  at `:251`). Concrete failing case, unchanged from the base: two committed records under
  `inode:007` and `inode:7` whose bytes are equal → the pass reads `inode:007`, CASes `inode:7`,
  emits `backfilled(inode = 7)`, decrements `remaining` for a fill that never landed on the record it
  read. The rubric's *"deferrals are settled"* covers the deferral itself, so this is **not** a
  request to re-open #698/#699 — it is that #681's closing comment states *"Nothing of the original
  scope sits outside those three seams, so there is no remaining work here"*, which is unwarranted:
  #698, #699 and #707 (*"backfill silently skips a committed record whose inode key will not
  parse — after #695 it lands on neither gauge and the pass still certifies"*) are all open and all
  descend from this scope. The human should decide whether the close text is amended or the residue
  is explicitly accepted; the split itself is not thereby refuted.

- **NEEDS-HUMAN [human] — the split created one seam that no issue owns, and shipped an
  operator-facing message that misroutes it.** `brief.md:172-174` sends *"the repair/evacuation write
  path for a `seg:`-resident chunk"* to #682, and the merged backfill child accordingly carries four
  `#682` markers — `git show origin/main:crates/custodian/src/backfill.rs` `:112`, `:217`, `:347`,
  and the **audit string** at `:367` (*"the segmented write path is #682"*). But #682 explicitly
  disowns it: `results/issue_682/brief.md:234-237` — *"`backfill.rs` (#695 — this slice does not
  touch it … a backfill fill that would cross the ceiling is a real gap, but it is a different write
  path and belongs to whichever slice owns it next)"* — and its budget makes editing `backfill.rs` a
  STOP; its two open children #710/#711 are rebalance/reconstruction only. So the concrete state is:
  a committed object whose chunks live in `seg:` records **and** carries an empty placement makes
  `backfill::reconcile` answer `Reconciled::Blocked` on **every** pass forever
  (`origin/main:crates/custodian/src/backfill.rs:216-222` → `incomplete += 1` → `:275-289`), and the
  operator is told to wait for #682, which will never do it. Honest weighting: this is **latent**
  today — no producer of segmented maps exists yet (#653 owns the committer), so the two conditions
  cannot co-occur in a live store — and it is a decline, not a loss, so C-1's data-loss arm is not
  breached. It is nonetheless a permanent state with no owner, and the rubric's reviewer protocol
  (`AGENTS.md:200-203`) says to raise the tracking issue when the deferral itself looks wrong. That
  is a scope/ownership decision, not something a Do round on #681 can fix.

- **The gate row that says nothing.** `check-gates.json:3` records `"overall": "pass"` while all
  eleven rows are `"none"` with `"N/A — close disposition (no patch to verify)"`, and
  `check-review.md` records the reviewer leaf as SKIPPED. Nothing in the Check tested this bundle's
  *actual* success criterion (`brief.md:56-58`). I tested it by hand and it **holds** — so this is a
  note, not a refutation: GitHub `subIssues` of #681 = {695, 696, 697} (all CLOSED/merged);
  `results/issue_{695,696,697}/brief.md` all exist; the target worktree is clean at `339da46` and the
  bundle carries no `patch.diff`, so "ships no code" is true. The `pass` is vacuous evidence, but the
  criterion behind it is satisfied.

## Attempted and could not refute

- **The seven-site table (`brief.md:32-38`) is exact and complete.** On the base, `SegmentedMapUnsupported`
  occurs in `crates/custodian/src/` at precisely `backfill.rs:99`, `:181`, `rebalance.rs:162`, `:259`,
  `reconstruction.rs:332`, `:583`, `:636` — no eighth site was missed, and no other `chunk_map` read
  in those three files bypasses `as_flat()`. On `origin/main` every one of them now goes through
  `metadata::resolve_chunk_map` (backfill `:156`, rebalance `:256`, reconstruction `:474`) with the
  `Ok(None)` / downcast arms mirroring `gc.rs:402-416`. The stated defect really is gone.
- **The disjointness / one-parallel-wave claim (`brief.md:83-85`).** `git diff --stat
  339da46..origin/main` over the three children touches exactly `src/{backfill,rebalance,reconstruction}.rs`
  plus three **new** `tests/segmented_map_*.rs` — no shared module, no `crates/custodian/src/lib.rs`,
  no `crates/core/src/metadata.rs`, no `Cargo.toml`. I looked specifically for a forced shared edit
  (the `Reconciled::Blocked` fold in `reconciliation.rs:44`, `emit_remaining`'s second namespace walk
  at `backfill.rs:156`/`:171` needing a caller change) — both are containable inside one file.
- **The fixture-duplication justification (`brief.md:110-111`).** `struct MemMeta` really is defined
  in **twelve** independent files under `crates/custodian/tests/` on the base. The number is not
  inflated.
- **A sibling in flight colliding with Rule C's surface.** #691 (`d986069`, merged between the base
  and the children) adds validated identity types and fail-closed key parsing — exactly the Rule C
  surface — but it is additive and *"nothing in this module is consumed by production code yet"*, so
  it neither conflicts with nor silently satisfies the children.
- **The #682 repointing the brief demanded (`brief.md:18`).** `results/issue_682/brief.md:159` now
  reads `Depends on: 696, 697`; the drop of #695 is argued at `:7-9` rather than overlooked. (The
  parent's "#682 depends on all three" was over-broad, and the downstream correction is what exposed
  the ownership gap in the second finding above.)
