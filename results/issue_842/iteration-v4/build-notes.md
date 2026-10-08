# Build notes — #842 (809.4), iteration 4: restore fences resurrected `Completing` sessions

Base: `pdca-integration/r-a834e98f67b952c61db7a03084c626e8/main` @ `022d76f` (origin/main + child-1
#839, child-2 #840, child-3 #841, plus unrelated integrated slices; the bundle's `stack-base`).
Line numbers below are on that base **with `patch.diff` applied** (the worktree at `$PDCA_WORKTREE`).

This iteration starts from iteration 3's patch (`iteration-v3/patch.diff`, which applies cleanly on
the same base) and addresses its carry-forward. The design of the fence itself is unchanged; see
"What the patch does" at the end.

## Carry-forward: what each finding got

### 1. The re-check trusted the obligation's group (T4 blocking; C5 / adversary / code review)

`recheck_fenced` used to read `retire:records:s:<id>:<E'-1>` and then check whatever segment group
**the obligation** named. A decodable obligation naming another group (A1) or no group at all (A2,
`{parts:[[1,1]]}`) made the re-run report clean while the session's own `seg:` range held a bad
record that nothing deletes.

Fix, as the reviewers proposed:

- `Plan::Fenced` now carries the session's **own** group for the attempt before its current epoch:
  `SegmentGroup::from_nonce(record.segment_nonce().clone(), E'-1)`
  (`crates/custodian/src/restore.rs:870`, built at `:904-909`). `from_nonce` is infallible because
  child-2's decode already validated the nonce (`crates/core/src/metadata.rs:1040-1046`).
  `Aborting@0` has no previous attempt and becomes `Plan::Settled` (`restore.rs:871-872`).
- `recheck_fenced` (`restore.rs:925-960`) checks the obligation only against that group: if
  `owed.segments() == Some(group)` it checks the session's own range (`check_attempt`); a
  decodable obligation owing anything else is named with a new `SegmentFault::NotOfAttempt`
  (`restore.rs:259-261`, Display `:273`); an undecodable one is named as before.
- The CLI NEEDS-HUMAN paragraph (`crates/server/src/cli.rs:1409-1413`), §6.5
  (`docs/design/architecture/06-runtime-view.md:65`) and the blueprint's SEGMENTS bill
  (`docs/design/architecture/m4-first-deployment-blueprint.md:633-637`) now say the obligation
  can also be "owing another range".

Regression (leg K, `crates/custodian/tests/restore_completing_fence.rs:592-646`): three sessions
already at `Aborting@4`, each beside a `retire:records:s:<id>:3`:

- `fa`: the decodable foreign `{seg}` (`FOREIGN`, `:435`), and its **own** `seg:<fa>:3:000000` is
  `not a segment` (`:623-624`) — the reviewers' A1 exactly.
- `fb`: `not json` (iteration 3's torn case, now folded into the same table).
- `fc`: `{"parts":[[1,1]]}`, which decodes (asserted, `:618-619`), with an empty own range — A2.

Each session key and each obligation key must be named on both passes (`:640-643`).

**A design choice the human may want to weigh: one entry per session, not two.** When the
obligation is not the attempt's deleter, I name the obligation and do **not** also scan the
session's own range. Reasons:

1. If the obligation does not owe the session's group, *every* record in that range has no
   deleter. The obligation is the cause, and naming it already makes the run need a human.
2. `retire:` sorts before `seg:`, so the obligation really is "the first record at fault, by key",
   which is what `UnaccountedSegments::record` promises (`restore.rs:237`).
3. The report and the CLI count **sessions** (`cli.rs:1409`: "{} fenced upload session(s)",
   from `segments_unaccounted.len()`). Naming both would add a second entry for one session and
   overcount. Keeping both would need a distinct-session count in the CLI (about 3 lines) and new
   wording in four places.
4. Once a human fixes the obligation, the next run checks the range and names a bad `seg:` record
   if there is one. The session stays named until both are fixed.

The reviewers' wording was "name the session … and check the session's own range". With this
choice the own range is the **only** range ever checked (the obligation's foreign group is never
read), but it is checked only once the obligation owes it. The `no_identity` mutant below shows
the difference matters: a version that skips the mismatch naming and just checks the own range
names `fa`'s bad seg record, but not `fc` at all, and leg K goes red.

**Declined: naming an `Aborting` session with no records obligation but a non-empty own range.**
The brief's K fact says only a `Completing → Aborting` fence files that key, so an `Aborting`
session without it was fenced from `Open` (its range is empty) or its obligation has drained
(#659's territory). Naming the leftover case would take a new fault variant plus one `seg:` page
read per `Aborting` session:

```rust
let Some(value) = meta.get(&key).await? else {
    let (page, _) = staged_page(meta, &seg_range_prefix(group), None).await?;
    if let Some((first, _)) = page.first() {
        unaccounted(report, session, object_name(first), SegmentFault::NoDeleter);
    }
    return Ok(());
};
```

That is about 12 production lines plus a variant, Display arm and test case: roughly 1 KB of
diff, against 428 bytes of budget left. It is also outside the brief's legs and invariant (the
invariant is about sessions the image held `Open`/`Completing`, and about deleters *this* fence
installs). I suggest raising it with #659, which owns the drain that creates the
"obligation gone" state.

### 2. `segment_fault` checked only the first chunk (adversary)

H(ii) now seeds segment 1 as a **two-chunk** segment, `[0xD21 (part 2's chunk), 0xD2F (held by
no part)]` (`restore_completing_fence.rs:525-532`). The mutant
`.chunks().iter().take(1).find(..)` now turns H and K red.

### 3. Nothing tested a `Completing` session whose **bytes** key was taken (adversary)

G-collision (`restore_completing_fence.rs:437-475`) now runs two collided sessions from one table:
`c1` with the foreign `{seg}` under its **records** key (the brief's case), and `c4` with
`{"session":true}` under its **bytes** key. For each: the taken value stays byte-identical, the
session stays `Completing@3`, it has no other `retire:` key, and the report binds the session to
`ObligationKeyTaken { key: <that key> }`. `c2` (uncollided `Completing`) and `c3` (`Open`) are
still fenced. Both "require-absent on one key only" mutants turn G-collision red.

### 4. C4 diff coverage "patch.diff does not apply on origin/main"

Unchanged cause, not fixable in the patch: `run-diff-cov.sh` resolves its base to `origin/main`,
but this bundle is stacked on child-1..3 (`stack-base`). `run-verify.sh` picks up the integration
base when `PDCA_BASE` is exported (it did here), so the diff-cov row apparently does not get the
same export. That is gate wiring for whoever owns `pdca.toml` / the harness.

## Size budget (brief: at most 7 files, under 80 KB)

7 files. `patch.diff` is **81,492 bytes = 79.58 KB** (bytes/1024, as `size_signal.py` counts it),
leaving 428 bytes.

The three fixes above added about 1.7 KB to iteration 3's 81,670 bytes. I cut it back without
dropping any leg, case or assertion:

| Cut | Saved (approx.) |
|---|---|
| Moved `enum Plan` / `struct Fence` from above `fence_session` to right above `plan_fence` (`restore.rs:865-880`), inside an already-changed region: one hunk fewer | 240 B |
| CLI agreement test names `wyrd_custodian::restore::{UnaccountedSegments, SegmentFault}` by full path (`cli.rs:3221-3225`) instead of a test-only `use` in its own hunk | 290 B |
| CLI unfenced-paragraph test: the two sessions iteration 3 moved off `Completing` now use `ChangedUnderPass` instead of `EpochExhausted` (`cli.rs:3329-3354`), so the original `!contains("last epoch")` check stays untouched: one hunk fewer | 300 B |
| Removed four decorative `// ---- … ----` separator lines from the new test | 430 B |
| H(i)'s nonce-less record is now the normal `Completing` record with its `segment_nonce` field removed (`restore_completing_fence.rs:517-520`), so `completing_state` folded into `completing` | 240 B |
| Shorter CLI NEEDS-HUMAN paragraph: each named session already carries its fault text (`SegmentFault`'s Display), so the paragraph no longer lists every fault kind | 150 B |
| Renamed the private helper `segments_unaccounted(…)` → `unaccounted(…)` (mirrors `unsettled(…)`), so its signature fits on one line | 100 B |
| Shorter wording in two core doc comments, the K doc, the blueprint and §6.5 | 250 B |

One trade to know about: after the CLI test change, no CLI test checks the **text** of the
`EpochExhausted` cause ("at the last epoch the record can spell"). The agreement test still uses
an `EpochExhausted` session and checks the paragraph names it. The core `Completed` fixture in
`mod completing_teardown` (`crates/core/src/multipart.rs:5241-5245`) now uses one hex string for
both `etag` and `complete_fingerprint`; I kept the `Completed` arm because iteration 2's review
asked for it.

If a review round adds code, the budget will be crossed. The next cheapest cut I know of: nest
`mod completing_teardown` inside `mod open_teardown` so `session` needs no `pub(super)`
(`multipart.rs:5146`): one hunk, about 450 bytes.

## Red → green, through the project's runner

`PDCA_BUNDLE=results/issue_842 PDCA_LANE=1 PDCA_BASE=pdca-integration/r-a834e98f67b952c61db7a03084c626e8/main ./engine/scripts/run-verify.sh`:

- GREEN (fix applied): `running 7 tests … 7 passed`.
- RED (production reverted, test kept): `running 7 tests … 0 passed; 7 failed`.
  **7 of 7 ran red, all by assertion; none failed to compile.** G, H, K and G-sparse fail because
  the session is still `Completing@3` (`assert_aborted`, `:297`); G-collision fails on the cause
  check (`:469`: base names `cause: Completing`, not `ObligationKeyTaken`); G-atomic and Order get
  `Ok` where the fix returns the commit's `Err` (`:424`, `:687`).
- `run-verify.sh: PASS — red without the fix, green with it (7 test(s) ran red).`

As the brief predicted, H(i) and H(vi) already hold on the base; leg H still goes red on the base
because its (ii)/(iv)/(v)/paged cases are not fenced there.

## Refuting my own test

- **(a) Genuine red?** Yes. `run-verify.sh` reverted every production file and kept the test:
  7 of 7 failed on assertions (above). Each new case was also checked against the mutant it was
  added for (table below); each went red.
- **(b) Production path?** Yes. Every leg calls `wyrd_custodian::reconcile_after_restore`
  (`restore_completing_fence.rs:183`), which runs the real `fence_session` → `plan_fence` →
  `SessionRecord::completing_teardown` → `MetadataStore::commit`, then the real `check_attempt` /
  `part_chunks`, and on a re-run the real `recheck_fenced`. Only the store and the disks are
  doubles. The core unit test calls `SessionRecord::completing_teardown` directly.
- **(c) Fixture includes the fault?** Yes. K's `fa` really has a decodable foreign `{seg}` under
  the exact key `recheck_fenced` reads **and** a bad record in its own range; `fc` really decodes
  (asserted) and owes no group. H(ii)'s segment really holds two chunks with the unheld one second.
  G-collision's `c4` really has a decodable `{session}` under its bytes key (asserted by decode).
  The paged case's bad record is really on page two (index 512 of 513); G-sparse really holds
  10,000 parts with the checked chunk on the last `part:` page, and its store really refuses an
  oversized value (the leg first commits a `MAX_VALUE_BYTES + 1` probe and asserts the refusal).

### Mutants (each applied to `restore.rs`, both fence test files run, file restored; `cmp` against a saved copy afterwards)

| Mutant (production, `crates/custodian/src/restore.rs`) | Caught by |
|---|---|
| `recheck_fenced`: a decodable obligation owing another group → `return Ok(())` (iteration 3's behaviour for A2) | K |
| `recheck_fenced`: skip the identity check, check the own range for any decodable obligation (`|| true` on the guard) | K (`fa`'s obligation key not named) |
| `recheck_fenced`: undecodable obligation → `return Ok(())` | K |
| `segment_fault` checks only the first chunk (`.iter().take(1).find`) | H, K |
| `require_absent` on the bytes key only (blind put of the records obligation) — brief SELF-TEST | G-collision |
| `require_absent` on the records key only (blind put of the bytes obligation) | G-collision |
| Fence `Completing` without the `seg:` obligation — brief SELF-TEST | 6 of 7 legs |
| `check_attempt` stops after the first `seg:` page | H, K |
| `part_chunks` reads only the first `part:` page | G-sparse |

The third brief SELF-TEST (`{session, parts}` over the 10,000 sparse parts) was run in iteration 2
and turned G-sparse red with `value_too_large: Some(128916)`. Neither that code path nor the
ceiling double changed since, so I did not rebuild it. Iteration 3 also showed the core
`mod completing_teardown` test kills "delete the `state` field" in `completing_teardown`; that
test only lost a duplicate hex string this round.

## Gates and commit-readiness

- `cargo fmt --all -- --check`: clean. `cargo clippy -p wyrd-core -p wyrd-custodian -p wyrd-server
  --all-targets -- -D warnings`: clean.
- Targeted runs: `restore_completing_fence` 7/7, child-3's `restore_open_fence` 9/9, core
  `teardown` tests 2/2, CLI `restore_verdict*` tests 5/5.
- `./engine/xtask.sh ci` (= `cargo xtask ci` in the worktree, run on the final tree): **passed**,
  `xtask ci: all checks passed`, exit 0. Every stage ran, none was skipped: `typos`, `lint_docs`,
  `render_site --check` (99 pages, link audit OK), the gitlink/unsafe/blackbox guards, fmt,
  clippy, build, workspace tests, `cargo-machete`, three `cargo deny` runs, the ADR-0035 statics
  gate, deploy-guard, and the `wyrd-dst` clippy + tests under `--cfg madsim`. So both external
  tools the brief names (`typos`, `docs-renderer`) were present and actually ran; no NEEDS-HUMAN
  external dependency.

## What the patch does (unchanged from iteration 3 apart from the above)

- **Core** (`crates/core/src/multipart.rs`): `SessionRecord::completing_teardown` (`:2291`) mints,
  from a decoded `Completing@E` record, the record at `Aborting@E+1`, `retire:bytes:s:<id>:<E>` =
  `{session, all}`, and `retire:records:s:<id>:<E>` = `{seg: (nonce, E)}` from the private
  `RetireObligation::attempt_segments` (`:3665`), which takes the token epoch from the group so key
  and payload always pass `checked_against_key`. `None` below `Completing` or at `u64::MAX`
  (`checked_add`): no wrap, no panic. Per the human's Plan decision (option a), `{session, all}`
  replaces `{session, parts}`; the retire rows table gives the reason. `mod completing_teardown`
  (`:5198`) is the unit-test twin of `mod open_teardown`.
- **Custodian** (`crates/custodian/src/restore.rs`): `plan_fence` (`:883`) sends `Open` → one
  obligation, `Completing` → both obligations plus the attempt to check, `Aborting@E'` → re-check
  of its own group at `E'-1`, `Completed`/`Aborting@0` → nothing, and no next epoch →
  `EpochExhausted`. `fence_session` (`:798`) builds ONE batch: `require(mpu, bytes read)`,
  `require_absent` on every obligation key, the session put, every obligation put. On `Conflict`
  it re-reads each obligation key and names the first one taken. After the commit lands,
  `check_attempt` (`:963`) pages the frozen `seg:<nonce>:<E>:` range and, only if it holds
  anything, the session's `part:` range (`part_chunks`, `:1011`). The first faulty record names
  the session in `RestoreReport::segments_unaccounted`. `recheck_fenced` (`:925`) is the K re-check,
  with the `// deferred: #659` marker at `:929`. `SessionUnsettled::Completing` is gone.
- **CLI** (`crates/server/src/cli.rs`): the summary counts the new list, the NEEDS-HUMAN paragraph
  is at `:1402`, `unsettled_causes` loses the `Completing` slot, and the agreement test covers the
  new report field.
- **Child-3's test** (`crates/custodian/tests/restore_open_fence.rs:675`): its "cannot be fenced"
  `Completing@3` fixture is now `Completing@u64::MAX`, since a plain `Completing@3` is fenced now.

## Downstream (not done here, per the brief)

0016's reaper and operator-abort `Completing → Aborting` rows (`0016:665`, `:2193`) still specify
`{session, parts}` and hit the same 100,000-byte ceiling for a sparse session. Flag that to #656
and #659. No edit to 0016 or to any ADR. #810 should depend on this child, not on #809. The
"`Aborting` with no records obligation but a non-empty own range" case above is a candidate for
#659.

## Scratch

Mutant logs, the verify log and the CI log are under `$PDCA_SCRATCH/pdca-builder-842-mutants/`.
I did not delete them; the harness owns cleanup of that root.
