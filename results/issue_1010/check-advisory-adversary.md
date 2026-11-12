# Adversarial review — #1010 (Complete: recovery from unknown commit outcomes)

**Verdict:** I could not break the production logic. Every attack on fence ownership, the
held/unsettled bookkeeping, resume and release failed. One real mismatch is left: in the fence
phase, the in-doubt error can say a fence "may still be applied" right after a read has shown it
can't land. That is the safe direction (it over-warns, never under-warns), but it contradicts the
documented contract of a public field.

## Evidence re-run (scratch copy of `$PDCA_TARGET`, `cargo test -p wyrd-core --test multipart_complete`)

- **Green:** 85/85 pass on the patched tree.
- **Red:** with the patched test file over the pre-fix `crates/core/src/multipart/complete.rs`,
  17 of the 23 new legs fail: G1, G1-open, G2, G2-first, G2-refused, G2-settled, G3-fence, G4,
  G4-landed, G4-repin, G6, G6-spent, both G6-held legs that clear or refuse, G6-held no-check, and
  both G6-rolled-back legs. The 6 that pass pre-fix (G3 flip, G5, both G3-torn, G2-unread,
  G4-unbuilt) all expect the in-doubt error, which pre-fix code returned for every unknown
  outcome. They guard against recovering too eagerly, so passing pre-fix is expected. The C4-verify
  row is green-only (its log shows only the GREEN run), as the brief's posture (b) says. The red
  side above is now reproduced independently.
- **G5's red rests on mutations, so I ran two by hand** at `complete.rs:963-965`. (a) Treating the
  matching fence bytes as ownership (`break (opened, placed)`). (b) Dropping the check. Both turn
  G5 and the G0 applied-fence leg red.
- **The wrapper is faithful to a real store.** `Hold`/`Land`/`release_held` apply the held batch
  through the inner redb store's `commit` (`crates/core/tests/multipart_complete.rs:572-575`,
  `:620-627`). So a late batch re-checks its `require`s, and every "can no longer land" assertion
  is a real CAS (compare-and-swap) result, not a copy of production logic.

## Findings

- NEEDS-HUMAN [impl] — `crates/core/src/multipart/complete.rs:963-965` returns the in-doubt error
  before `doubt.settle` runs. So a held first fence is reported with `may_still_commit: true` even
  when the read just found the session off the bytes that fence requires. Concrete case, with the
  test file's wrapper: script `[OnCommit::Hold, OnCommit::Unknown { applied: true,
  may_still_commit: false }]`, then `up.complete(&named(&PARTS))`. It returns
  `InDoubt { commit: Fence { epoch: 1 }, may_still_commit: true }`, and the text says "its fence at
  Completing@1 may still be applied". `up.store.release_held()` afterwards answers `Conflict`: the
  held fence requires `Open@0`, which never comes back. The same happens with
  `[Hold, OnCommit::land(OnCommit::Pass)]`: the held fence has already landed before the request
  returns, yet the error says it may still be applied after. This contradicts three places that
  say a held doubt lasts only "until a read finds the session off the bytes ... requires":
  - the field doc at `complete.rs:527-530`;
  - the module doc at `complete.rs:189-194`;
  - the living doc at `docs/design/architecture/05-building-block-view.md:252`.

  Abort keeps the rule exactly (`abort.rs:259-264`); Complete's publication phase also settles
  before it ends (`complete.rs:1560`). Safety is not affected: the G5 rule (never claim ownership)
  still returns the in-doubt error. But #670's same-gateway recovery reads this flag. Fix
  options:
  - report `false` when the read finds the fence's own bytes (any held fence's required `Open`
    bytes are then gone for good);
  - or narrow the three doc sentences to say a held outcome stays reported when the read finds the
    fence's own bytes.

  Either way, add the probe above as a regression.

## Attempted to refute; could not

- **Stale re-read after `may_still_commit: false`** (the fence landed, the read shows `Open@E`, so
  the check before the fence answers and leaves the fence standing). The store contract rules this
  out: one re-read settles an out-of-flight outcome (`crates/traits/src/lib.rs:240-243`).
- **Unknown outcome lost inside the committer.** `staged_publication.rs:457` and `:464` pass the
  store's error through `?` unchanged, so the downcast at `complete.rs:1500` sees
  `CommitUnknownResult`. It is never read as a definite failure that releases.
- **The pin rule for publication-round holds** (`complete.rs:1507`, `:1560`, `:1878-1894`). Within
  a round the committer stops at the first unknown outcome, so the held batch requires the bytes
  the last committed batch left. The read after it finds those bytes, or the bytes after them if
  it landed. Re-pinning on a second hold (`:1855-1859`) and clearing once a later round's batch
  moved the cursor both hold. Every case is either exact or conservative.
- **`unknown_flips` matching an older flip** (`complete.rs:1545-1550`). An earlier round's flip
  pins the target it read. If the target moved, that flip can't land. If it didn't, both rounds'
  `Completed` bytes are identical. So keeping every flip is harmless, and only flips of this owned
  fence can match.
- **`open_and_charged_since`** (`complete.rs:1007-1012`): reaper rollback; rollback followed by a
  rival's give-back; a rival's charged release; a torn or missing record. Each one answers
  `OperationAborted`, loops into the in-doubt error, or returns `NoSuchUpload`. A re-fence with the
  original stamp happens only after a give-back, which the human approved in iteration 2.
- **Answers given while a held fence is kept** (`complete.rs:897-933`). A held doubt survives
  `settle` only when the read is exactly the bytes it required. Those bytes passed every check in
  the round that sent it, and the optimization skips itself (`:927`).
- **Termination.** Each fence commit, unknown ones included, spends a round (`:934-937`). Each
  publication round, unknown ones included, counts against `R_PUBLISH` (`:1572`). The release stays
  bounded.
- **Citations:** `multipart.rs:2276-2285` (the `Completing` variant) and `abort.rs:259-273`
  resolve correctly on the patched tree.

## Note (no action asked)

- `complete.rs:1897-1905` (`Doubt::in_doubt`) always passes `fault_behind_fence: None`. So when a
  held doubt ends publication on a read fault, the fault is dropped from the error. Probe:
  `[Pass, arm(FailGets(bucket_key(BUCKET)), Hold)]` gives `InDoubt { Flip, true }` with no mention
  of the marker-read fault. The field's doc limits it to a fault that led to a release, so this is
  consistent. Carrying the cause would touch the error type, which belongs to child-1. I'm
  recording it, not raising it.

<!-- pdca:leaf-complete -->
