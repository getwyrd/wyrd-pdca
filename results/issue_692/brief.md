# Brief — issue 692 / multipart-record-family

> **This bundle ships no code.** #692 was decomposed at the re-plan of 2026-08-09 into **#715**,
> **#716** and **#717**, all three filed as GitHub **sub-issues** of #692 and materialised as their
> own bundles. Two have since **merged**; the third was itself split again. This brief is the
> parent's Plan artifact: it records the decomposition, maps every element of #692's original scope
> onto a child (or onto a recorded design withdrawal), and hands the human the one decision left —
> confirm the split, and choose what happens to the tracker issue.
>
> Authoritative detail lives in `split-proposal.md` (this bundle) and in the child briefs. This file
> does not restate them.
>
> **The 2026-08-17 `iterate-plan` that returned this bundle to Plan was answered on a stale
> picture.** Its carry-forward asks to "re-derive the split from scratch" over 8 adversary findings
> against `split-proposal.md` — but that split had already been accepted (2026-08-09) and executed:
> #715 merged 2026-08-10, #716 merged 2026-08-11. Several of the 8 findings were resolved *by the
> real children*, not by re-planning (see "What the 8 findings actually did" below). Re-deriving a
> split now would duplicate **#771** and **#772**, which are in flight today.
>
> **It also works around a harness bug.** Filed upstream as **eduralph/pdca-harness#481**: a split
> parent whose brief was archived by an earlier `iterate-plan` reads `UNPLANNED` despite its close
> marker, so it re-enters the interactive Plan leaf on every run and never freezes.
> `results/issue_717/brief.md` is the working precedent this file copies; `results/issue_711/brief.md`
> and `results/issue_681/brief.md` are the two earlier ones.

- **Slug:** multipart-record-family
- **Defect:** **The multipart key space had no value half.** The key grammar landed with #691
  (merged, `d986069`), but nothing could decode the records those keys name, so any store round trip
  that later read one (#656–#659) would have had to trust an unvalidated blob — an internally
  inconsistent `mpuctl` admission record lets a gateway admit sessions past the memory bound the
  reconcile pass is sized for. That defect was real; it is now **almost entirely fixed**, by
  children rather than by this bundle. `Budget`/`AdmissionRecord` (#715) and the
  session/slot/part lifecycle records (#716) are merged on `origin/main`; the two remaining
  namespaces — `retire:` and `sidx:`/`pending:` — are carried by **#771** and **#772**. Nothing is
  fixed by THIS bundle.
- **Success criterion:** the decomposition is **complete and correct**: every element of #692's
  original scope is either carried by a filed, briefed child, or is a design element **explicitly
  withdrawn** on the target with its withdrawal recorded in-tree. Re-verified at this revision pass
  (2026-08-18) against `origin/main` @ **`a801997`**. It is checked **row by row** against the
  mapping table below — every row has a command, and no row is asserted from prose. (`$PDCA_TARGET`
  is the target checkout the driver exports, AGENTS.md; the `../wyrd` fallback is INTEGRATION §2's
  sibling checkout. The earlier draft hardcoded `../wyrd`, which does not resolve for a reader whose
  sandbox mounts the target elsewhere — hence the parameterised form throughout.)
  1. **The children exist — and so do THEIR children.**
     `gh api graphql -f query='{repository(owner:"getwyrd",name:"wyrd"){issue(number:692){subIssues(first:10){nodes{number title state}}}}}'`
     → exactly #715 CLOSED, #716 CLOSED, #717 OPEN. That query sees **direct** sub-issues only, so it
     is not sufficient alone; the same query with **`number:717`** → #771 OPEN
     (`multipart-retire-obligation`), #772 OPEN (`multipart-owned-staging-entry`), and
     `cat results/issue_717/split-lineage.json` → `{"children":["771","772"]}`. Both were run at this
     revision.
  2. **Every child has a bundle with a brief.** `ls results/issue_{715,716,717,771,772}/brief.md`
     → five paths, no error. This is the check for "a child's bundle is missing".
  3. **Namespace ownership — positively and negatively, in ONE command.**
     `git -C "${PDCA_TARGET:-../wyrd}" grep -n 'pub struct \(Budget\|AdmissionRecord\|SessionRecord\|SlotRecord\|PartRecord\|PartSummary\|RetirePayload\|OwnedEntry\|StagedPlacement\)' origin/main -- crates/core/src/multipart.rs`
     → exactly **six** hits: `:1158` `Budget`, `:1413` `AdmissionRecord` (#715); `:1711`
     `SessionRecord`, `:1862` `SlotRecord`, `:2073` `PartRecord`, `:2172` `PartSummary` (#716). No
     `RetirePayload`, no `OwnedEntry`, no `StagedPlacement`. That single output carries both halves of
     the overlap check: each merged child's symbols are present **and disjoint**, and the two in-flight
     children's symbols are genuinely still unbuilt — so #771/#772 cannot be re-deriving work that is
     already on `main`. (A file *log* — the previous draft's second command — shows only that four
     commits touched the file; it proves neither ownership nor disjointness, so it is demoted to a
     provenance note: `git -C "${PDCA_TARGET:-../wyrd}" log --oneline origin/main -- crates/core/src/multipart.rs`
     → `d986069`, `5eeca16`, `778f1cf`, `a3b2bbe`.)
  4. **Tests, not just touches.**
     `git -C "${PDCA_TARGET:-../wyrd}" ls-tree --name-only origin/main crates/core/tests/ | grep multipart`
     → `multipart_budget_admission.rs` (#715), `multipart_keys.rs` (#691), `multipart_session_records.rs`
     (#716) — one shipped regression file per merged child. The two remaining files named under
     `Test file` are absent from that listing, which is the expected state for children still in flight.
  5. **The one genuinely unbuilt element.**
     `git -C "${PDCA_TARGET:-../wyrd}" show origin/main:crates/core/src/metadata.rs | grep -n 'struct PendingEntry' -A6`
     → `:1556`, holding `lease_expiry_millis` alone. This command proves **absence only** — that is its
     entire job: it is the negative half of the `PendingEntry` row, whose positive half is #772's brief
     (`results/issue_772/brief.md:172`, the two optional ownership fields).
  6. **The withdrawn envelope.**
     `git -C "${PDCA_TARGET:-../wyrd}" show origin/main:crates/core/src/multipart.rs | sed -n '15,21p'`
     → the withdrawal, stated in-tree.
  7. **The module header the merges left stale** (last row of the table):
     `sed -n '5,25p' results/issue_771/patch.diff` → the correcting hunk, owned by #771.
- **Falsifiability:** each named failure case now has a command that produces it, and each was run at
  this revision against `a801997`: a **child bundle or tracker issue missing** → rows 1–2 (a short
  sub-issue list, a missing `brief.md` path); **lineage missing** → row 1's `split-lineage.json`;
  **two children's namespaces overlapping**, or a child re-deriving merged work → row 3 (a symbol
  appearing that its owner has not merged, or a merged symbol claimed by an in-flight child);
  **a parent-scope element mapping to neither a child nor a recorded withdrawal** → the table below,
  each of whose rows terminates in one of rows 3–7. The one non-child mapping — the
  `encode_record`/`decode_record` envelope — is falsifiable in-tree and was checked:
  `crates/core/src/multipart.rs:15-21` states the forward reference "**is withdrawn**". **No gate
  can evaluate any of this** — a close-disposition bundle runs no gates (`gates.py:203-216`) — which
  is why it is a sign-off check, declared under `Verification posture`.
- **Invariant to restore:** none here — no code ships from this bundle. The invariant the WORK must
  restore (**ADR-0045, parse-don't-validate at decode: a stored record's fields may not disagree
  with each other or with the key that names them, and the disagreement must surface as an error,
  never as a value**) is carried unchanged by the children: already enforced on `origin/main` by
  #715/#716, and stated in both remaining child briefs. Note the parent's own earlier text cited
  `docs/principles.md:109` with a line number; **that file does not exist in the repo** (confirmed
  at `a801997`: `git -C ../wyrd ls-files | grep -i principle` returns nothing). The repo's
  convention for the same reference is sectional — `docs/principles.md` §5 C-1, the form
  `crates/core/src/multipart.rs:57` and `:1008` cite it in.
- **Repo + branch target:** getwyrd/wyrd @ main   (INTEGRATION §2; base verified `a801997`.
  **No PR opens from this bundle** — publish exits 0 with *"nothing to contribute; close the tracker
  item by hand"* (`publish.py:97`, `:173`), so the tracker action is the human's, at sign-off.)
- **Depends on:**
- **Conflicts with:**
- **Ordering note:** no `Depends on` / `Conflicts with` — deliberately. This bundle touches no file
  and builds nothing, so it can neither block nor collide with anything. The parent's former
  `Depends on: 691` is dropped: #691 is merged (PR #703) and the dependency is historical, and a
  bundle marked `split` can never go COMPLETE, so leaving live edges pointed at it would hold its
  dependents forever. That hazard was already handled downstream — **#693 was repointed off #692 on
  2026-08-09 and again on 2026-08-16, and now reads `Depends on: 772`**
  (`results/issue_693/brief.md:77`); #655 sits behind #693 and needed no change. Verified at this
  re-plan: no bundle declares `Conflicts with: 692`, and #693's is the only brief that still
  mentions #692 at all — as recorded history (`:86`), not as a live edge. The live ordering lives in
  the children: **#771 wave 0**, **#772 `Depends on: 771`** and terminal, with **#772 alone carrying
  the chain's external conflicts** (`Conflicts with: 721, 722`).
  **The tracker body's `Conflicts with: 682` is not dropped — it was transferred, twice, and is
  live today on #772.** `notes.json` (the pre-split filing body, lines 86-93) declares that #692
  shares `crates/core/src/metadata.rs` and `crates/dst/tests/custodian.rs` with **#682** and that
  "the two must never share a wave". Both ends of that edge then split, so the edge had to follow the
  ids — verified at this revision:
  **(a) the #682 end.** #682 was itself split (its bundle carries `close-disposition: split`), into
  #710 and #711 —
  `gh api graphql -f query='{repository(owner:"getwyrd",name:"wyrd"){issue(number:682){subIssues(first:10){nodes{number title state}}}}}'`
  → #710 CLOSED, #711 OPEN; #711 split again into #721 + #722 (same query with `number:711`). #682's
  own `split-proposal.md:105-111` names this repointing as a required post-accept step: "*repoint
  #692's `Conflicts with: 682` at the two new child ids*". The constraint has **not** ceased to
  apply — it moved.
  **(b) the #692 end.** The shared-file scope (`PendingEntry` + the `custodian.rs` ripple) is carried
  by **#772**, not by this parent and not by #771. #772's brief declares
  `Conflicts with: 721, 722` (`results/issue_772/brief.md:145`) over exactly those two files
  (`:154-157`, `:172`, `:186`) — the terminal descendants of #682. #771 touches neither file
  (`results/issue_771/patch.diff` = `multipart.rs`, its new test, and one docs paragraph), so it
  correctly carries no edge. **This parent declares none because it edits no file at all** — a
  `Conflicts with:` on a zero-file bundle schedules nothing, and on a bundle marked `split` (which
  can never go COMPLETE) a live edge would hold its counterpart forever.
- **Surfaces:** data
- **Difficulty:** low — zero files changed; the blast radius of a disposition record. The original
  slice's `high` (12 files, 6 crates) is carried by the children.
- **Scope:** record the decomposition of #692 — this brief's mapping table and its per-row checks.
  The two **operational** acts that follow are named separately here, because the earlier draft
  entangled them with the criterion and let it pass under mutually exclusive states:
  **(a) the control-flow marker** `results/issue_692/close-disposition` — **DONE**, restored
  2026-08-18 00:50 and reading `split` (`cat results/issue_692/close-disposition`); it is a
  file-state fact, not a judgement, and is no longer open (see "The mechanical item — settled").
  **(b) the tracker action for #692 itself** — the human's, at sign-off, and **deliberately NOT part
  of the success criterion**: the decomposition is complete-and-correct or it is not, and rows 1–7
  return the same answer whether #692 is closed today or kept open as an umbrella. Neither act can
  make a failing mapping pass, nor a passing one fail. / **out of scope:** any implementation of any
  record family (that is #715/#716,
  merged, and #771/#772, in flight — reopening this bundle to a fix path would rebuild the very
  11-file shape the split exists to abandon, and would duplicate two live bundles);
  **re-deriving the split**, which the stale carry-forward asks for and which events have overtaken;
  re-litigating the withdrawn `encode_record`/`decode_record` envelope, a design decision already
  settled and documented on the target.
- **Repro instruction:** confirm the split state rather than the defect — the defect's repro lives in
  the child briefs. On this bundle: `iteration-v1` / `v2` hold the two abandoned build attempts
  (v2 = 106 KB / 11 files / ~2,140 added lines, over the 100 KB size backstop); `iteration-v3` holds
  the re-plan that authored `split-proposal.md`, with `close-disposition` reading `split` and
  `check-advisory-adversary.md` holding the 8 findings; `iteration-v3/SUMMARY.md` §9 records the
  2026-08-17 decision (`iterate-plan`). On the tracker: #715, #716, #717 are sub-issues of #692.
- **External dependencies:** none
- **Test file:** none — no code ships from this bundle, so there is no test to flip. The regression
  tests for the work live in the children: `crates/core/tests/multipart_budget_admission.rs` (#715,
  merged), `crates/core/tests/multipart_session_records.rs` (#716, merged),
  `crates/core/tests/multipart_retire_obligation.rs` (#771, new) and
  `crates/core/tests/multipart_owned_staging.rs` (#772, new).
- **Verification posture:** declared, because the default does not hold. This is a close / no-fix
  disposition: there is no production change, so no flippable red→green exists and every gate
  element lands N/A without running (`gates.py:203-216`). Verification is the **human's confirmation
  at sign-off** against the mapping below. Nothing is deferred-but-unbuilt here: two thirds of the
  work is **merged**, and the rest is **reassigned** to two bundles that are filed, briefed and
  schedulable today.
- **Citations expected:** none of the code kind — nothing is built. The claims in this brief are
  cited to `origin/main` @ `a801997` (`crates/core/src/multipart.rs`,
  `crates/core/src/metadata.rs`, `docs/design/architecture/05-building-block-view.md`), the merge
  commits `5eeca16` / `778f1cf` / `a3b2bbe`, `split-proposal.md` and `iteration-v3/` (this bundle),
  `results/issue_717/split-lineage.json`, the child briefs, and the tracker. Added at this revision:
  `results/issue_771/patch.diff:5-25` (the module-header correction), `results/issue_772/brief.md:145`
  and `:154-157` (the transferred conflict edge), and `results/issue_682/split-proposal.md:105-111`
  (the repointing instruction that edge follows).
- **Prior-art check (triage cycles):** by affected file path — **none**, this bundle touches no
  file. By disposition: four sibling split parents in this instance. **#681** is the completed
  precedent — split, signed off `merged-wider` 2026-08-08, tracker issue CLOSED. **#711** and
  **#717** are the structural precedents this brief copies; #717 is this issue's own child.
  **#654** is this issue's own parent and sits in the unfixed-#481 state this brief exists to avoid
  (close marker `split`, no brief, permanently UNPLANNED).
- **Disposition hint:** likely-close
  — this bundle has three `iteration-v*` archives, so the hint alone does **not** fire the close
  fast path: `_close_class` returns `""` for a hinted brief once any archive exists
  (`driver.py:240`), and only an existing `close-disposition` marker wins outright
  (`driver.py:231-238`). The 2026-08-17 `iterate-plan` archived this bundle's marker into
  `iteration-v3/` (it is in `DOWNSTREAM_OF_BRIEF`); **it has since been restored** — 2026-08-18 00:50,
  reading `split`, so `_close_class` returns `"split"` on the marker branch and the builder/reviewer
  leaves are skipped. Verify with `cat results/issue_692/close-disposition`.

## What each child carries — #692's scope, fully mapped

| #692 scope element | Carried by | State |
|---|---|---|
| `Budget` (profile tuple + `U_ref` / `MAX_SESSIONS` derivations), `AdmissionRecord` (`mpuctl`) | **#715** | ✅ merged — PR #724, `5eeca16` |
| `SessionRecord` + `SessionState` / `PublishTarget` / `Completion` (`mpu:`), `SlotRecord` (`slot:`), `PartRecord` / `PartSummary` (`part:` / `psum:`) | **#716** | ✅ merged — PR #725, `778f1cf` (+ review `a3b2bbe`) |
| `PartNumberSet`, `RetirePayload`, `decode_retire_obligation(key, bytes)` (`retire:*`) | **#717 → #771** | built, in Check |
| `OwnedEntry` / `StagedPlacement`, `decode_owned_entry(key, bytes)` (`sidx:`) | **#717 → #772** | briefed, wave 1 |
| `PendingEntry` extension (`owner` / `staged`) + the mechanical 8-file ripple | **#717 → #772** | briefed — still absent on `main` (`metadata.rs:1556` holds `lease_expiry_millis` alone) |
| The `docs/design/architecture/05-building-block-view.md` paragraph | landed with #716's review | ✅ merged — `05-building-block-view.md:202` |
| `encode_record` / `decode_record` envelope | **withdrawn by design** | recorded at `crates/core/src/multipart.rs:15-21` |
| The module header's own forward reference — stale since #716 merged | **#771**, then **#772** | owned, in flight (below) |

**The stale module header is owned, not unmapped.** `crates/core/src/multipart.rs:8-13` on
`origin/main` @ `a801997` still reads "*The remaining record values (`mpu:`'s session shape,
`part:`'s chunk list) are the **next children's***" — false since #716 merged: `SessionRecord`,
`SlotRecord`, `PartRecord` and `PartSummary` are in that very file at `:1711`, `:1862`, `:2073`,
`:2172`. (`a3b2bbe`, "#725 review", corrected the *tense* of neighbouring prose but not this
sentence.) It is a documentation defect on the target, and this bundle ships no code — but that does
**not** leave it unowned: **#771's in-flight patch rewrites exactly this hunk**
(`results/issue_771/patch.diff:5-25`, `@@ -5,19 +5,24 @@`), replacing it with the merged record list
plus its own `RetirePayload`, and leaving a single correct forward reference — "*The owned staging
entry (`sidx:`'s `PendingEntry` with its ownership fields) is the next child's*" — which **#772**
then retires as the terminal child. So the header is correct again after #771 lands and
self-consistent after #772. Two consequences for sign-off: (i) the completeness claim is **narrowed**
to say the decomposition maps every scope element *and* that the one stale artefact the merges left
behind is carried by a named child, not that `main` reads perfectly today; (ii) if #771 were
abandoned rather than accepted, this hunk becomes genuinely unowned and must be re-mapped onto #772
— a check row (criterion row 7) exists precisely so that cannot pass silently.

Nothing was silently dropped. The single element that is not carried by a child was **withdrawn on
the record**, with its reasoning in-tree: `0016` §1 gives every value a *key-determined* shape and a
stored value carries no type tag, so a per-record dispatch arm would have nothing to dispatch on.
Each record type instead validates inside its own `Deserialize` over the store-wide
`metadata::encode` / `metadata::decode` codec.

## What the 8 findings actually did

The 2026-08-17 carry-forward lists 8 adversary findings as the reason to re-derive the split. Checked
against the executed children at `a801997`:

- **`MAX_SESSIONS` dropped the `SCAN_CAP/2` clamp** — *fixed in the shipped child.* Merged
  `Budget::max_sessions` implements `min( ⌊W_ref / U_ref⌋ , SCAN_CAP/2 )` (`multipart.rs:1247-1258`,
  `SCAN_HALF` at `:1137`), and the module documents the clamp as implementation-applied, not an
  operator range check.
- **`encode_record`/`decode_record` signature unpinned between children** — *dissolved.* The envelope
  was withdrawn outright (`multipart.rs:15-21`), so there is no signature to pin.
- **`docs/principles.md` cited but absent** — *still true, and corrected here* (see
  `Invariant to restore`): the file does not exist; the repo cites it sectionally.
- **#710/#711 conflict declarations unenforced** — *handled downstream.* The live conflict edge is
  declared from #772's side (`Conflicts with: 721, 722`).
- **File-count budget 12 vs 14, zero slack, three wrong `0016` citations, `metadata.rs` "nothing else
  changes" false** — *moot as stated.* All four were defects of the **proposal's arithmetic and
  citations for a three-way carve that no longer needs deriving**; the children were filed, rebriefed
  against their own bases, and two of them built and merged without hitting the backstop.

None of the eight describes a defect in the *shipped* decomposition. This is why the brief concludes
`likely-close` rather than proposing a new cut.

## The mechanical item — settled

`results/issue_692/close-disposition` was **absent** when this brief was first drafted: the
2026-08-17 `iterate-plan` had archived it into `iteration-v3/close-disposition` (it reads `split`),
because the marker is in `DOWNSTREAM_OF_BRIEF`. With a brief present and no marker, `_close_class`
returns `""` and the bundle takes the **normal Do path** — the builder dispatched against a slice
whose work is two-thirds merged and one-third owned by two live bundles.

**It has since been restored** (2026-08-18 00:50, content `split`), which reproduces the exact
on-disk shape of #711, #681 and #717 — brief + `close-disposition` + `iteration-v*` archives — under
which the marker branch of `_close_class` wins outright (`driver.py:231-238`), the driver skips the
builder and reviewer leaves (`driver.py:41`, `:72`, `:91`), and the bundle routes straight to
sign-off with an N/A gate matrix. This brief still does not *write* that marker — it is the bundle's
Do artifact and a control-flow decision — but the decision is no longer open, so sign-off inherits it
as a fact to confirm (`cat results/issue_692/close-disposition` → `split`) rather than an action to
take. Nothing else in this brief depends on it.

## Why the tracker record still reads like an implementation slice

The supplied `notes.json` is the issue **body as filed on 2026-08-09**, before the decomposition:
it defines the full record family and names `crates/core/tests/multipart_records.rs` as its success
criterion, and its `comments` array is **empty**. That is not stale scraping — the live issue is the
same today (verified at this revision: the `comments` node on #692 returns `[]`). So a reader given
only `brief.md` + `notes.json` sees this brief *assert* a disposition the tracker record nowhere
shows, which is a fair objection.

What makes the disposition observable **from the tracker alone** is the **sub-issue graph**, not the
body: criterion row 1's GraphQL query returns #715 CLOSED, #716 CLOSED, #717 OPEN as sub-issues **of
#692**, and #771/#772 as sub-issues of #717 — a structure GitHub only creates when issues are filed
as children, which is what `pdca split --accept` did. Two merge commits on the target
(`5eeca16`, `778f1cf`) close the loop: the parent's scope is *on `main`*, authored under the
children's PRs (#724, #725), which is not a state the un-split issue could have reached.

The gap the objection correctly identifies is that **#692's own thread says none of this in prose**.
That is fixable, and it is the human's act, not the brief's — see sign-off item 3.

## What sign-off decides

1. **Confirm the split** (or override it with iterate-to-Do, which re-enables the full Do+Check band
   on a slice whose scope is already merged or reassigned — it would duplicate #771/#772).
2. ~~Restore `close-disposition: split`~~ — **done** (2026-08-18 00:50); confirm, don't act.
3. **Choose the tracker action for #692 itself** — the driver does none of it (publish exits with
   *"close the tracker item by hand"*). Two defensible options: **close it**, as #681 was closed at
   its split sign-off, with a comment pointing at the sub-issues; or **keep it open as an umbrella**
   until #771 and #772 merge — #715 and #716 are already closed. The GitHub sub-issue links preserve
   lineage either way. **This is the human's call and is deliberately left open here** — no default
   is assumed. **Either option should carry the same comment on #692** — the decomposition, the two
   merge commits, and the two live children — because that is the one act that makes the disposition
   legible in the tracker record itself rather than only in its sub-issue graph. It is an operational
   step, outside the success criterion (see `Scope`), so the mapping stands or falls independently of
   whether or when it is posted.

## Harness note

Once the driver advances this bundle, `_do_close` **overwrites `build-notes.md`**
(`driver.py:253`) with generic close text. The child pointers are recorded in this brief for that
reason; #692 has no `split-lineage.json` of its own (its split predates that record — the lineage is
on the tracker as sub-issues, and in `split-proposal.md`).

## STOP discipline

Nothing to build, nothing to publish. Draft only until Check sign-off; the tracker action for #692
is the human's.

## Plan-review response (revision pass, 2026-08-18 — `plan-advisory-plan-reviewer.md`)

Five NEEDS-HUMAN findings. **Four revised the brief; one stands with a scope note.** Every claim
below was re-run against `origin/main` @ `a801997` and the live tracker at this pass.

- **F1 — "the no-code/close reframing is not supported by the supplied tracker record."**
  *Revised, and the finding is right about the record.* `notes.json` is the pre-split filing body
  with an empty `comments` array, and the **live** issue is identical (re-checked: `comments` → `[]`).
  Added "Why the tracker record still reads like an implementation slice": the disposition **is**
  observable from the tracker — through the sub-issue graph plus the two merge commits, not the body —
  and the missing prose record is now an explicit sign-off act (item 3). The brief does **not** retain
  the tracker's implementation scope: that scope is on `main` under the children's PRs, and rebuilding
  it here would duplicate #771/#772.
- **F2 — "'complete and correct' is not established by the three-command check."**
  *Revised; the criterion was genuinely under-specified.* Replaced with seven per-row checks. `../wyrd`
  → `"${PDCA_TARGET:-../wyrd}"` throughout; the grandchildren #771/#772 now have their own query plus
  `split-lineage.json`; child-bundle existence has a command; the file **log** (which proves touches
  only, as the finding says) is demoted to provenance and namespace ownership is proved instead by one
  symbol grep that is simultaneously the overlap check; test files are listed, not inferred; and the
  `PendingEntry` grep is labelled as the absence-only half it is.
- **F3 — "the scope contains separate, undecided outcomes."** *Revised.* `Scope` now separates the
  mapping (the criterion) from the two operational acts, and states that neither can flip the
  criterion. The marker repair is no longer undecided — the file exists and reads `split` — so the
  section is retitled "settled" and sign-off item 2 becomes a confirmation.
- **F4 — "the target contradicts the completeness claim — the module header is stale and unowned."**
  *Revised; the stale header is real* (`multipart.rs:8-13` vs the types at `:1711`/`:1862`/`:2073`/
  `:2172`). It is **not** unowned: #771's in-flight patch rewrites that exact hunk
  (`results/issue_771/patch.diff:5-25`) and #772 retires the residual forward reference. Added as a
  mapping-table row, with the ownership traced and criterion row 7 to catch it if #771 is abandoned.
  The completeness claim is narrowed accordingly — the decomposition maps every element and names the
  owner of the artefact the merges left behind; it does not claim `main` reads perfectly today.
- **F5 — "a load-bearing tracker conflict (#682) is dropped without resolution."**
  *Revised — the edge was transferred, not dropped, and the brief now shows the chain.* #682 is OPEN
  but **split** (`results/issue_682/close-disposition`) into #710 CLOSED + #711 OPEN, and #711 into
  #721 + #722; #682's own `split-proposal.md:105-111` requires exactly this repointing. The shared
  files (`core/src/metadata.rs`, `dst/tests/custodian.rs`) belong to **#772**, whose brief declares
  `Conflicts with: 721, 722`. #771 touches neither file. **This parent's fields stay empty** — it edits
  no file, and a live edge on a `split` bundle that can never reach COMPLETE would strand its
  counterpart. That part of the finding stands answered rather than actioned.
