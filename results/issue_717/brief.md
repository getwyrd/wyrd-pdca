# Brief — issue 717 / multipart-staging-retire-pending

> **This bundle ships no code.** #717 was decomposed at the re-plan of 2026-08-14 into **#771**
> and **#772**, both filed as GitHub **sub-issues** of #717 and materialised as their own bundles.
> This brief is the parent's Plan artifact: it records the decomposition, maps every element of
> #717's original scope onto a child, and hands the human the one decision left — confirm the
> split, and choose what happens to the tracker issue.
> `close-disposition: split` is already on disk, so the driver skips the builder and reviewer
> leaves and routes straight to sign-off (`driver.py:210-217`); the gate matrix lands N/A by
> construction, with no gate command executed (`gates.py:153-171`).
>
> Authoritative detail lives in `split-proposal.md` (this bundle) and in the two child briefs.
> This file does not restate them.
>
> **It exists only to work around a harness bug.** Filed upstream as
> **eduralph/pdca-harness#481**: a split parent whose brief was archived by an earlier
> `iterate-plan` reads `UNPLANNED` despite its close marker, so it re-enters the interactive Plan
> leaf on every run and never freezes. Confirmed still unfixed at `origin/main` @ `b38c68a` —
> nothing in the v0.57.0→#230 range touches `state.py`, `split.py` or `driver.py`.
> `results/issue_711/brief.md` is the working precedent this file copies.

- **Slug:** multipart-staging-retire-pending
- **Defect:** **Two disjoint record namespaces have no value half.** The base carries the *key*
  half of both — `RetireToken` (`multipart.rs:1022`), `retire_key` (`:1071`), `parse_retire_key`
  (`:1092`), `sidx_key` (`:907`), `parse_sidx_key` (`:925`) — and nothing that can read the
  values those keys name. Nothing decodes a retirement obligation, nothing decodes an owned
  staging entry, and the `pending:` ledger cannot tell an owned entry from an ordinary lease.
  That defect is real and unfixed — it is now carried by **#771** (the `retire:` obligation
  value) and **#772** (`sidx:` + the `PendingEntry` extension). Nothing is fixed by THIS bundle.
- **Success criterion:** the decomposition is **complete and correct**: every element of #717's
  original scope is carried by a filed, briefed child; both children exist as tracker sub-issues
  of #717 and as `PLANNED` bundles; and the two children's namespaces do not overlap. Verified at
  the re-plan (2026-08-14) and re-checkable at sign-off with two commands —
  `gh api graphql -f query='{repository(owner:"getwyrd",name:"wyrd"){issue(number:717){subIssues(first:10){nodes{number title state}}}}}'`
  (returns #771 and #772 OPEN) and `cat results/issue_717/split-lineage.json` (`children:
  ["771","772"]`) — read against the scope mapping below.
- **Falsifiability:** the criterion goes RED if a parent-scope element maps to neither child, if
  a child's bundle or tracker issue is missing, or if the two children's *namespaces* overlap
  (which would re-create the entanglement the split exists to remove). All three were checked:
  the sub-issue query returns #771 and #772; #771 owns `retire:` only, #772 owns `sidx:` +
  `pending:` only. Note the children deliberately **do** share two files
  (`crates/core/src/multipart.rs` and one sentence of
  `docs/design/architecture/05-building-block-view.md:202`) — that is why `#772 Depends on: 771`
  wave-serialises them, and it is not a falsifier. **No gate can evaluate this** — a
  close-disposition bundle runs no gates — which is why it is a sign-off check, declared under
  `Verification posture`.
- **Invariant to restore:** none here — no code ships from this bundle. The invariant the WORK
  must restore (**ADR-0045 decision 1, parse-don't-validate at decode: a record that decodes is
  a record every downstream reader may trust without re-checking it**) is carried unchanged by
  both children and stated in both child briefs.
- **Repo + branch target:** getwyrd/wyrd @ main   (inherited from the original slice and carried
  unchanged by both children. **No PR opens from this bundle** — publish exits 0 with *"nothing to
  contribute; close the tracker item by hand"* (`publish.py:161-166`), so the tracker action is the
  human's, at sign-off.)
- **Ordering note:** no `Depends on` / `Conflicts with` — deliberately. This bundle touches no file
  and builds nothing, so it can neither block nor collide with anything. The ordering lives in the
  children: **#771 is wave 0** (no prerequisite, touches only `crates/core/src/multipart.rs`);
  **#772 `Depends on: 771`** and is wave 1, terminal. **#772 alone carries the chain's external
  conflicts** — `Conflicts with: 721, 722`, because it edits both `crates/core/src/metadata.rs`
  (#721's file) and `crates/dst/tests/custodian.rs` (#722's). #771 touches neither and needs none.
- **Surfaces:** data
- **Difficulty:** low — zero files changed; the blast radius of a disposition record. The original
  slice's `high` is carried by both children, each rated `high` in its own right.
- **Scope:** record the decomposition of #717 and close the parent — this brief, plus the human's
  sign-off decision. / **out of scope:** any implementation of either record family (that is #771
  and #772, and reopening this bundle to a fix path would rebuild the very 12-file shape the split
  exists to abandon); re-litigating the split itself; the three record-format decisions, each of
  which now belongs to a named child.
- **Repro instruction:** confirm the split state rather than the defect — the defect's repro lives
  in the child briefs. On this bundle: `close-disposition` reads `split`; `split-lineage.json`
  carries `["771","772"]`; `split-proposal.md` carries both children; `iteration-v1` … `v3` hold
  the abandoned attempts, and `iteration-v3/size-signal.json` records **123,675 bytes / 12 files /
  2 rounds** against a ≤960-line budget and the driver's 100 KB backstop. `iteration-v3/SUMMARY.md`
  §9 records the decision (`Outcome: iterated-to-Plan`, Eduard Ralph / 2026-08-12). On the tracker:
  #771 and #772 are OPEN sub-issues of #717.
- **External dependencies:** none
- **Test file:** none — no code ships from this bundle, so there is no test to flip. The regression
  tests for the work live in the children: `crates/core/tests/multipart_retire_obligation.rs`
  (#771, new) and #772's own new test files.
- **Verification posture:** declared, because the default does not hold. This is a close / no-fix
  disposition: there is no production change, so no flippable red→green exists and every gate
  element lands N/A without running (`gates.py:155-165`). Verification is the **human's
  confirmation at sign-off** against the mapping below. Nothing is deferred-but-unbuilt here: the
  work is not deferred, it is **reassigned** to two bundles that are filed, briefed and schedulable
  today.
- **Citations expected:** none of the code kind — nothing is built. The claims in this brief are
  cited to `split-proposal.md` and `split-lineage.json` (this bundle), `iteration-v3/SUMMARY.md` §9,
  `iteration-v3/size-signal.json`, the two child briefs, and the tracker.
- **Prior-art check (triage cycles):** by affected file path — **none**, this bundle touches no
  file. By disposition: three sibling split parents in this instance. **#681** is the completed
  precedent — split, signed off `merged-wider` 2026-08-08, tracker issue CLOSED. **#711** is the
  structural precedent this brief copies, and **#692** is this issue's own parent (split
  2026-08-09 into #715 → #716 → #717). **#654** sits in the unfixed-#481 state this brief exists to
  avoid: close marker `split`, no brief, permanently UNPLANNED.
- **Disposition hint:** likely-close
  — the `close-disposition` marker already on disk reads `split` and outranks this hint outright
  (`driver.py:210-217`). `split` is deliberately **not** in `[driver].close_dispositions`, which is
  exactly why the parent never freezes on the marker alone; the configured token is written here so
  the close fast path fires and the bundle can reach sign-off.

## Why this was split

Three Do rounds converged on the same oversized shape. `iteration-v3/patch.diff` reached **123 KB
/ 1,780 added lines across 12 files** against a ≤960-line budget — the size backstop fired at 121 KB
against its 100 KB threshold — and the gating `T4-batch-review` came back **red in all three
rounds**.

The surviving findings were never implementation slips. They are three **record-format decisions**
that Do kept being asked to make inside a slice too large to hold them:

1. **The retirement payload cannot express `{session, parts}`.** v2/v3 modelled `Session {}` and
   `Parts { parts }` as mutually exclusive enum arms, while 0016 names `{session, parts}` as **one**
   obligation in four places (`:355`, `:665`, `:823`, `:2193`). One obligation gets one key under
   `require_absent(retire:<mode>:<token>)` (`0016:369-373`), so the halves cannot be installed
   separately.
2. **The retirement token has no canonical epoch.** `checked_against_token` accepted a segment
   generation under both `E` and `E+1`, so one obligation had **two valid keys** — installable
   twice, drainable twice (batch-review round 3, blocking). 0016 never pins it.
3. **`PendingEntry` accepts an owned-shaped value under a `pending:` key.** Deferred in v3 as "a
   shape check, not a key check" and re-raised by the batch reviewer in **every** round, because the
   deferral named an expectation rather than a filed issue (the target's `AGENTS.md:200-203` settles
   only a deferral tracked in a real `#N`).

The two namespaces do not need each other: the base already carries the **key** halves of both, so
each child adds only its own **value** half. Split by namespace, each child is ~1,000–1,250 lines
and carries exactly one cluster of the open decisions — **child-1 takes decisions 1 and 2, child-2
takes 3.**

## What each child carries — #717's scope, fully mapped

| #717 scope element | Carried by | Note |
|---|---|---|
| `retire:bytes:` / `retire:records:` obligation payload, range-encoded part-number set | **#771** | with decisions 1 and 2 |
| `decode_retire_obligation(key, bytes)` — the module's first **key-taking** decoder | **#771** | an obligation's identity lives partly in its token |
| `sidx:<upload-id>:<part-number>:<chunk-id>` owned staging entry + `StagedPlacement` | **#772** | |
| `decode_owned_entry(key, bytes)` | **#772** | mirrors #771's key-taking shape rather than inventing a second |
| `PendingEntry` extension — additive `owner` / `staged` (`0016:442-457`) | **#772** | forces `Copy` off; `UploadId` is a `String` newtype |
| Namespace separation as a **decode-time** property, not a convention (decision 3) | **#772** | the part three Do rounds deferred |
| The shared `RecordError` enum growth (`multipart.rs:96`) | **both** | wave-serialised by `#772 Depends on: 771` |

Nothing was withdrawn: every element of the parent's scope is carried by a child. This is a clean
two-way split, unlike #711's, which dropped one element deliberately.

## Post-`--accept` work — done, recorded here so sign-off can check it

`split --accept` could not declare these; both are now applied:

- **`- **Conflicts with:** 721, 722` added to #772's brief** (`results/issue_772/brief.md:145`).
  A proposal's ordering fields may only name sibling labels, so this had to be added by hand.
- **#693 repointed from `Depends on: 717` to `Depends on: 772`**
  (`results/issue_693/brief.md:77`). #717 is now a bundle marked `split`, which can never go
  COMPLETE, and `_runnable` gates every `Depends on` on `merged.is_merged` — left alone, #693
  would have been held forever. #655 sits behind #693 and needed no change.

## What sign-off decides

1. **Confirm the split** (or override it with iterate-to-Do, which archives the close marker and
   re-enables the full Do+Check band on a slice three attempts already failed to fit).
2. **Choose the tracker action for #717 itself** — the driver does none of it (publish exits with
   *"close the tracker item by hand"*). Two defensible options: **close it**, as #681 was closed at
   its split sign-off, with a comment pointing at the sub-issues; or **keep it open as an umbrella**
   until #771 and #772 merge. The GitHub sub-issue links preserve lineage either way. **This is the
   human's call and is deliberately left open here** — no default is assumed.
3. **Milestone the children.** #717 is on `0.1 Alpha`; **#771 and #772 were filed with no
   milestone**. Not a blocker for the driver, which never reads it, but the alpha is tracked by
   milestone and two of its slices are currently invisible to that view.

## Harness note

Once the driver advances this bundle, `_do_close` **overwrites `build-notes.md`**
(`driver.py:232`) with generic close text, destroying the `The work lives in the child bundles:
issue_771, issue_772` breadcrumb `split --accept` wrote. The child pointers are recorded in this
brief and in `split-lineage.json` for that reason.

## STOP discipline

Nothing to build, nothing to publish. Draft only until Check sign-off; the tracker action for #717
is the human's.
