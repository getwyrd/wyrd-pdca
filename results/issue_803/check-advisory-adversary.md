# Adversarial review — issue #803 (staged protection class for GC and restore)

Verdict: **could not refute the fix.** Two doc-accuracy defects found, both `[impl]`-shaped.
The protection logic itself survived every attack I could build.

## Findings

- NEEDS-HUMAN [impl] — `docs/design/architecture/m4-first-deployment-blueprint.md:610-612` tells the
  operator that for an upload session (`mpu:`) **or an in-flight staging entry (`sidx:`)** "the pass
  never decodes their values, so a damaged one is not what you are looking at". That is true of
  `mpu:` and **false of `sidx:`**: `StagedSet::read_owned_entry` (`crates/custodian/src/gc.rs:711-712`)
  decodes every owned value through `decode_owned_entry`, which also enforces byte-canonicality
  (`crates/core/src/multipart.rs:3735-3755`). Concrete case: an `sidx:` value corrupted in the store
  under a key that still parses. The runbook says there is nothing to look at; in fact both passes
  hold that chunk's fragments unmarked and emit `action=untrusted-staged-record`
  (`crates/custodian/src/gc.rs:1359`, `crates/custodian/src/restore.rs:1004`) — a signal this runbook
  entry never mentions, while the audit-action list two lines below names only the two
  `unresolvable-*` actions. An operator following this text concludes a damaged staging-entry value
  produces no signal and no held chunk. The CLI comment gets it right by scoping the same claim to
  "to build this class" (`crates/server/src/cli.rs:1339-1344`); the runbook dropped the scope. One
  sentence to reword; it does not touch any string leg I pins.

- NEEDS-HUMAN [impl] — `crates/custodian/src/gc.rs:906-909`: `object_name`'s contract still reads
  "How a blocker is named to an operator: the `inode:` key as the store spells it", but this patch
  routes three new key namespaces and one key *range* through it — `mpu:` / `part:` / `sidx:` keys at
  `gc.rs:262-268` and `restore.rs:814-824`, and `StagedReadFault.range` at `gc.rs:873`, which is a
  prefix, not a record key. The doc is now wrong about its own callers, in the one helper the report
  names, the CLI paragraph and the new fault text all depend on. Docs-currency nit, one line.

## What I attacked and could not break

- **Red→green, re-run independently.** `cargo test -p wyrd-custodian --test staged_protection` in a
  scratch copy: 26/26 green with the patch. The frozen `gate-logs/C4-verify.log` shows 24 of those
  26 failing on base by assertion, with the guards (D, F) green on base as the brief designed.
- **Leg I's red, which no gate can show** (a modified file earns no C4-verify red). I rebuilt
  `crates/server/src/cli.rs` as base production code + the new test only, and the test fails on base:
  `the verdict does not say "4 record(s) UNREADABLE" … 4 committed object(s) UNREADABLE`. The brief's
  claim that leg I is red-by-assertion on base holds.
- **Twelve hand-built mutants on the production path, all caught** (each compiled, each run against
  the new suite): allow an empty staged placement through `place` (`gc.rs:756`) → 2 fail; read
  `part:` before `sidx:` (`gc.rs:828-835`) → 4 fail; drop the `held` arm and drop the
  `incomplete-staged-set` arm of `StagedSet::protection` (`gc.rs:693`, `:695`) → 6 and 4 fail; skip
  `parse_part_key` in `read_part` (`gc.rs:740`) → 1 fails; stop `walk_staged_range` after one page
  (`gc.rs:856-859`) → 2 fail; drop `staged.unresolvable` from GC's `Blocked` answer (`gc.rs:411`) → 4
  fail; drop the staged arm from GC's reclaim gate (`gc.rs:331-334`) → 17 fail; drop it from restore's
  mark gate (`restore.rs:435-441`) → 9 fail; classify an undecodable owned value as unresolvable
  instead of held (`gc.rs:726-731`) → 1 fails; stop `attribute_staged` from naming the record in the
  report (`restore.rs:813-817`) → 4 fail. No tautology and no parallel re-implementation: every leg
  drives production `reconcile_step` / `reconcile_after_restore`.
- **The source-before-destination claim, all three handoffs.** `sidx:` → `part:` → `inode:` holds in
  both passes (`gc.rs:258` before `:273`; `restore.rs:340` before `:350`/`:360`), and the C(ii)
  fixture really lands the flip and the retirement drain as two batches
  (`staged_protection.rs:1274-1301` asserts the store state after each).
- **Scrub / drain-status isolation (leg F), including the path leg F cannot see.** `reconcile_step`
  runs GC first and short-circuits on `?` (`reconciliation.rs:136-148`), so a GC staged-read fault
  *would* suppress scrub in a combined call — but the deployed loop drives scrub, reconstruction and
  GC in three separate `reconcile_pass` calls (`crates/server/src/custodian.rs:519`, `:533`, `:610`)
  and swallows a GC `Store` error at `:619-623`. No regression reaches scrub or drain status.
- **`STAGED_PAGE` = 512 against real backends.** I expected a fleet-wide read failure on a backend
  whose scan cap is below 512; `page_limit` clamps from above (`crates/traits/src/lib.rs:414-424`),
  so a small cap only shortens the page. `checked_page` also rejects an empty page carrying a
  continuation token (`gc.rs:1190-1203`), closing the silent-truncation hole I looked for in
  `staged_fragments`' two loops.
- **Key-namespace cross-talk.** A `part:<id>:` / `sidx:<id>:` range cannot capture another upload's
  records — `UploadId` is 32 hex characters (`multipart.rs:846`), so no id can extend another. And
  `slot:` / `psum:` records name no chunk (`multipart.rs:2284-2289`), so no staged byte falls outside
  the two classes the patch reads.
- **The expired-pending interaction.** A staged-protected fragment now `continue`s before the
  `still_held` arm (`gc.rs:331-337` vs `:389-393`), so a chunk with both an expired `pending:` lease
  and a staged record could have its lease entry retired while protected fragments survive. It is
  unreachable today: owned staging entries are disjoint from `pending:` by construction
  (`multipart.rs:1139-1142`). Worth remembering if a later slice ever gives a staged chunk a
  `pending:` lease; not a defect in this diff.

## On the verdict

I found nothing unwarranted in `check-gates.json`. The one loose phrase — C4-verify's
"26 test(s) ran red", where 24 ran red and 2 are guards green on base — is the gate's own wording and
was already settled at the iteration-5 sign-off; I am noting it only so it is not re-litigated as a
finding. I did not re-run the madsim DST leg myself; `gate-logs/C4-ci.log:3506` and `:3517` show both
new properties green, and the leg is registered in the campaign and in `REGRESSION_SEEDS`
(`crates/dst/tests/custodian.rs:3138-3148`, `:3183`).
