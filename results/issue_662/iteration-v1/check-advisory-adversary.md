# Adversarial review — issue 662 (staged protection class + reclaim intent)

**Verdict: I could not refute the fix.** The red→green evidence holds, the tests drive the
production `reconcile_step` / `reconcile_after_restore`, and every targeted break I tried was
caught. Five follow-ups below. Two of them are the T4 gate's six blocking findings (they reduce
to two issues), with my read on each.

## What I tried and could not break

- **Re-ran the proof** on a scratch copy of `$PDCA_TARGET`: `staged_protection` is 13/13 red on the
  base (every one an assertion panic, no compile error) and 13/13 green with the fix — same as
  `gate-logs/C4-verify.log`.
- **Ran the four new DST properties (the seeded madsim simulation tests) against the base.**
  `gc_reclaim_reaches_both_adoption_outcomes` hits "the adoption published a placement naming a
  fragment GC deleted" at 1 half-ms, and both staged-build properties fail. So the DST legs can
  tell good code from bad; they don't pass by construction.
- **Hand-made probes the mutation gate cannot generate**, each caught by the leg built for it:
  `part:` read before `sidx:` → C(i) red; staged build moved after the `inode:` scan → C(ii) red;
  a batch `Conflict` treated as all-lost (no per-intent retry) → F(ii) red; `superseded` never set
  → F(ii) red; `reclaiming` marks put through the grace test again → F(iv) and G red; a global
  `scan("part:")` filtered down to the session → D red.
- **Stricter legacy parser** (rejects `+5`, `05`): every in-tree `orphan:` writer puts
  `u64::to_string()` (`crates/core/src/metadata.rs:2140`, `:2248`, `:2321`;
  `crates/custodian/src/gc.rs:209`; `restore.rs:466`; `rebalance.rs:544`; `reconstruction.rs:944`),
  so no real mark becomes unreadable.
- **Key/record edges:** `scan("mpu:")` cannot return the `mpuctl` singleton
  (`crates/core/src/multipart.rs:1128-1132`). An owned entry whose owner disagrees with its key
  fails `decode_owned_entry` (`multipart.rs:3748-3752`) but its key still names the chunk, so the
  chunk is held whole rather than skipped.
- **Evidence framing, not a defect:** leg D is red on the base because staged fragments get
  reclaimed (`crates/custodian/tests/staged_protection.rs:898`), not because of a global scan, so
  its place in "13 ran red" says nothing about the scan guard. The global-scan probe above shows
  that assertion does work on its own.

## Findings

- NEEDS-HUMAN [impl] — `crates/custodian/src/gc.rs:1196-1199`, `:1310-1319`: three of the six T4
  blocking findings are one issue. If a pass deletes a fragment and then dies (or hits any later
  `?`) before its `Cleanup` commit, the `reclaiming` mark is left over an absent fragment, and since
  the walk is driven by `list_fragments()` (`gc.rs:297`) no pass ever revisits it. That is the #800
  sweep, which the brief puts out of scope, and the base had the same window with a legacy mark.
  But the code has no `deferred: #800` marker, so the review gate keeps raising it. The patch also
  adds new early-return points after deletes have happened — the retirement `get` (`gc.rs:1426`)
  and the intent commits (`gc.rs:1351`, `:1358`) — and each one drops the queued cleanup deletes.
  Fix: put a `// deferred: #800` marker and one sentence naming the delete-then-die window in the
  `Reclaim` doc. Under the rubric's deferral rule, that settles it.

- NEEDS-HUMAN [human] — `crates/custodian/src/restore.rs:317-324`: the other three T4 blocking
  findings. Restore names `staged.unresolvable` but says nothing about `staged.malformed`, so a run
  over an untrusted staged placement can still report `is_clean()`. I think this finding is weak:
  the chunk is held (the safe direction), GC names it on every pass (`gc.rs:256-258`), and restore
  says nothing about committed `malformed` placements on the base either (`restore.rs` has no emit
  for `referenced.malformed`). The brief also forbids new `RestoreReport` fields and assigns
  restore's counters to child-4. A human should either record the rejection with that reason, or
  ask for an audit-log line in restore (no new field).

- NEEDS-HUMAN [impl] — `crates/custodian/src/gc.rs:1336`: the surviving C5 mutant (`>=` → `<`) is
  a real test gap. I ran the whole `wyrd-custodian` suite with that change and every test passed.
  The mutant commits each reclaim intent on its own. `assert_bounded` checks the ⌈n/W⌉ commit count
  only for commits that carry deletes (`crates/custodian/tests/gc_ledger_walk.rs:567-579`), and the
  patch's own tests never execute the batch-full branch at `gc.rs:1337`
  (`gate-logs/C4-diff-cov.log`). So a regression to one commit per mark — up to 65,536 commits per
  pass at `ORPHAN_WINDOW = SCAN_CAP / 16` — would pass every test. Fix: next to the delete count,
  count the commits that carry an `orphan:` precondition and assert ⌈intents / W⌉.

- NEEDS-HUMAN [impl] — the docs overstate two claims. `docs/design/architecture/06-runtime-view.md:78`
  says the fragments of a still-draining byte retirement "are never reclaimed or marked", but the
  retirement drain is what writes those marks; they are only protected from reclaim.
  `docs/design/architecture/08-crosscutting-concepts.md:91` opens with "No pass destroys a byte
  before the destruction is durable in metadata" with no scope, yet `Reclaim::expired_lease`
  (`gc.rs:1296-1302`) still deletes before recording anything under
  `ExpiredPendingPolicy::Reclaim`. Limit both sentences to the orphan-mark path. Low severity.

- NEEDS-HUMAN [human] — `crates/core/src/metadata.rs:121-122`, `crates/custodian/src/gc.rs:1310-1319`,
  `:1373-1379`: the `reclaiming` state is only safe if no writer ever overwrites it, and nothing
  says so. A concrete case with a future mover (child-3 / #659): GC's intent CAS (compare-and-set)
  lands on `orphan:P` → the mover blind-puts its pre-mark on `P` and writes its destination
  fragment at `P` → GC's `delete_fragment(P)` removes those bytes → the mover's adoption
  `require(orphan:P == pre-mark)` still passes, because GC deletes the key only later, in
  `Cleanup` → a placement now names deleted bytes (outcome (c)). `resume` has the same exposure,
  since it deletes based on the window's read with no CAS at all. 0016 (`:1285-1344`) does not
  state this writer-side rule either. No in-tree writer can hit it today (every current writer
  marks a fragment it has just dereferenced), so it is not a defect in this diff. Suggestion:
  write "a writer never overwrites a `reclaiming` mark" into the `OrphanMark` doc now, so child-3
  inherits the rule instead of rediscovering it.
