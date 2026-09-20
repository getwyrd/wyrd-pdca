# Build notes — #803 staged protection class in the shared reference set (662.1), iteration 2

Target: `getwyrd/wyrd` @ `main` = `78f9859` (re-checked: `origin/main` has not moved). Every
`path:line` below is on the patched tree (`main` + `patch.diff`) unless marked "base".

## What this iteration changes, and why

The sign-off sent the previous attempt back for one thing: every test fixture seeded its upload
session as `Open`. The adversarial reviewer added a filter that skips any session whose value is
not `Open`, and all 11 unit tests and both DST properties still passed. That filter drops the parts
that matter most, because the protocol publishes only from `Completing` (the root flip requires
`mpu == Completing@E` and flips the session to `Completed`, `0016:662`), and a `Completed` session
keeps its `part:` records until its drain runs (`0016:573`).

The production code was already right (it reads every session listed under `mpu:`, whatever its
state, `crates/custodian/src/gc.rs:789-806`), so **this iteration changes only the tests**. The
production files (`gc.rs`, `restore.rs`, `cli.rs`, the two docs) are byte-identical to iteration 1;
I checked the `gc.rs` diff against `iteration-v1/patch.diff` after every mutation run.

What the tests now do, as the carry-forward asked:

- **Legs A and B** (`crates/custodian/tests/staged_protection.rs:661`, tests `:729`, `:763`) seed
  one session in each state: `Open` (as before), plus `Completing`, `Aborting` and `Completed`, each
  with its own committed `part:` record and owned `sidx:` entry on its own chunk
  (`staged_protection.rs:688-712`). The `Completed` session's part is one its Complete did not
  name, so no inode names its chunk and the record alone protects it. That is a real protocol
  state: the flip hands unnamed parts to a `retire:bytes:` drain that marks then deletes them
  (`0016:662`). Every fragment carries an `orphan:` mark past grace in A; in B the restore pass must
  mark none of them. Failure messages name the session state and record kind.
- **Session records** come from one helper, `session_value(phase)`
  (`staged_protection.rs:501`). Each is decoded by `decode_session_record`, re-encoded
  byte-identically, and checked to decode *in the claimed state*. Epochs follow the state machine
  (`0016:535-552`): `Open@3`, fenced to `Completing@4` / `Aborting@4`, flipped to `Completed@5`.
  The `Completing` value's `publish_target` matches the session's parent, name and epoch, which
  the decoder enforces (`crates/core/src/multipart.rs:2205-2230`).
- **Leg C(ii)** (`staged_protection.rs:877`) now commits the Complete fence
  (`require(mpu == Open@3)`, put `Completing@4`, `:886`) before arming the publication. The
  publication batch requires `mpu == Completing@4` and the inode absent, puts `Completed@5`,
  writes the inode and removes the `part:` record, all in one batch.
- **Leg C(i)** (`:854`): the part commit now carries `require(mpu == Open@3)` and
  `require_absent(part:…)`, as `0016:659` specifies.
- **The handoff double** now checks the batch's preconditions at the instant it lands
  (`staged_protection.rs:173-180`) and panics if they fail. So a fixture can no longer land a
  handoff the protocol could not commit there (for example, a publication from `Open`).
- **Leg D** (`:934`): its five sessions now cycle through all four states. That is cheap, and it
  makes the "every session's fragments survive" assertion catch a state filter as well.
- **Leg E** is unchanged: its failure classes don't depend on session state.
- **DST property 13** (`crates/dst/tests/custodian.rs:2563-2989`): the writer now does the part
  commit under `Open@3` (with preconditions), then the Complete fence `Open@3 → Completing@4` as its
  own batch (`:2841`), then after `publish_after` ms the publication, which requires
  `Completing@4`, puts `Completed@5`, writes the inode and deletes the `part:` record in one batch
  (`:2852`). All three session values go through `staged_session` (`:2728`), which checks the
  decode and byte-identical re-encode. The recording tap now also records which state the build's
  `mpu:` read listed the session in (`:2642`), without adding a simulated hop, so the old timing
  is kept.
  - `STAGED_PUBLISH_SPAN` goes from 4 to 8 (`:2607`). Reason: for the publication to land after
    the build's `inode:` scan while the build listed the session as `Completing`, the fence must
    land before the `mpu:` read (5 ms) and the publication after the `inode:` read (10 ms). The
    publication is issued `publish_after` ms after the fence lands and takes 2 ms, so this needs
    `publish_after ≥ 5`. With the old span of 4 that landing was unreachable.
  - The coverage walk (`prop_gc_staged_build_reaches_every_landing`, `:2942`) now walks
    `publish_after ∈ {0, 2, 6}` and asserts a fifth landing (`:2970`): the build listed the session
    as `Completing` and found the chunk in its `part:` record alone. That is exactly the landing a
    "skip non-`Open` sessions" build gets wrong.
  - Timing model (SimTikv: a read is 1 hop, a commit applies 2 hops after it is issued; the pass
    starts at 4 ms; the build reads `mpu:` at 5, `sidx:` at 6, `part:` at 8, `inode:` at 10): part
    commit applies at `c+2`, fence at `c+4`, publication at `c+6+p`. So: both handoffs before the
    build at (0,0) and (1,0); part commit between `sidx:` and `part:` at `c = 5`; publication
    between `part:` and `inode:` at (3,0) and (1,2); both after at `c ≥ 7`; the new
    `Completing`-only landing at (0,6). This is my analysis of the model; the coverage property is
    what proves each landing is actually reached, and it passes.

## The fix itself (unchanged from iteration 1, restated for the sign-off)

- `crates/custodian/src/gc.rs:662`: new `StagedSet { placed, held, unresolvable }`, the staged
  member. `placed` is a placement read and trusted; `held` is a chunk named but not trusted (the
  whole chunk is protected); `unresolvable` is a record that can't be read at all (the set is
  incomplete).
- `gc.rs:451`: `ReferenceSet.staged`. Kept out of `placed`, `malformed` and `unresolvable`,
  because scrub and drain status read exactly those three and keep their answers (brief scope;
  merging is 0016's own listed failure mode, `0016:881`).
- `gc.rs:468` `protection()`: two new reasons, `staged` and `staged-malformed`, after the
  committed reasons and before `incomplete-reference-set`. `gc.rs:494` `is_incomplete()` covers
  both halves and drives GC's `Blocked` answer (`gc.rs:378`).
- `gc.rs:552`: the staged class is read first, then the `inode:` scan. `gc.rs:789`
  `staged_fragments`: one `scan("mpu:")`, then per session `scan(sidx:<id>:)` and then
  `scan(part:<id>:)`. That is the `sidx:` → `part:` → `inode:` source-before-destination order
  (`0016:782-800`, X67), with bounded per-session ranges and no global scan (`0016:801-805`,
  `:890`).
- `gc.rs:696` `add_owned`: the chunk comes from the key; a key naming no chunk is `unresolvable`,
  and an undecodable value under a key that names its chunk holds that chunk. `gc.rs:723`
  `add_part`: an undecodable value is `unresolvable`. `gc.rs:740` `add_chunk`: the placement
  length must equal `fragment_count()`, otherwise the chunk is held.
- GC names held and unresolvable staged records on `wyrd.custodian.gc.audit` (`gc.rs:243`,
  `:261`; emitters `:1234`, `:1251`).
- `crates/custodian/src/restore.rs:323`: unreadable staged records join the same `unreadable`
  union as unreadable committed maps, so they land in `RestoreReport::unresolvable`, nothing is
  marked, and each is named on `wyrd.custodian.restore.audit` (`restore.rs:780`, `:962`).
  `restore.rs:328`: held records are named on the audit seam (`restore.rs:795`, `:978`), with the
  required `// deferred: #664` marker at `restore.rs:326`. The mark gate at `restore.rs:408` is
  unchanged code; it covers staged fragments because `ReferenceSet::protects` now does.
- Docs: `docs/design/architecture/06-runtime-view.md:78` (§6.7 step 2), as the brief asks.
- Two edits outside the brief's named files, kept from iteration 1: `crates/server/src/cli.rs:1263`,
  `:1331-1340`, `:1360`, test `:3009`, and `docs/design/architecture/m4-first-deployment-blueprint.md:610-611`.
  Restore now puts staged record keys into `RestoreReport::unresolvable`, and the CLI used to call
  every entry there a "committed object … whose chunk map is missing segments", which would be
  false for a `part:`/`sidx:`/`mpu:` key. The wording now says "record(s)" and names both audit
  actions. If the human judges these out of scope, drop both files; nothing else depends on them.

## Evidence

### Red → green (the project's C4-verify runner)

`PDCA_BUNDLE=… PDCA_BRIEF_BASE=origin/main ./engine/scripts/run-verify.sh`, on its own clean
`origin/main` (`78f9859`) worktree:

- `GREEN — cargo test -p wyrd-custodian --test staged_protection (fix applied)`: `11 passed`.
- `RED — … (production reverted, test kept)`: `0 passed; 11 failed`.
- `run-verify.sh: PASS — red without the fix, green with it (11 test(s) ran red).`

**11 tests ran red, all by assertion.** The failing lines on the red leg are all property
assertions (a fragment still on disk, or not marked): A `:742`, B `:770`, C1 and C2 `:830`,
D `:959`, E(i) ×3 `:997`, E(ii) ×3 `:1099`. None is an unwrap, a fixture panic or a compile error.
On base, C2's publication batch passed its new `Completing` preconditions and landed after the
`inode:` scan, so its red is the real reclaim.

### The reviewer's mutation is now caught

Each run: production `gc.rs` mutated in the worktree, tests unchanged, then `gc.rs` restored from
a scratch copy and checked identical to iteration 1.

| Mutation of `staged_fragments` (`gc.rs:789`) | Unit tests (11) | DST (`./engine/xtask.sh dst`) |
|---|---|---|
| skip any session whose value lacks `"kind":"Open"` (the reviewer's) | A, B, C2, D red | both property-13 tests red |
| skip `Completing` sessions only | A, B, C2, D red | not run |
| skip `Aborting` sessions only | A, B, D red | not run |
| skip `Completed` sessions only | A, B, D red | not run |
| read `sidx:` only for `Open` sessions (0016's minimum) | A, B, D red | not run |

The DST failure under the reviewer's mutation, for example: "GC reclaimed a fragment a staged
record or the published inode named throughout (part commit at 0 ms, publication 0 ms after the
fence; the build listed the session as Some("Completing") and saw the owned entry: false, the part
record: false, the published inode: false)". The coverage walk and the 50-seed campaign both went
red; the committed regression seeds happened not to draw that region and stayed green, which is
why the coverage walk exists.

The last row is a deliberate choice, not an accident. 0016 requires every listed session's parts
but only an `Open` session's owned entries (`0016:770-772`). The brief's scope reads both record
kinds of every listed session ("covering every listed session whatever its state is fine"), and
the production doc says so. The test pins that choice, with a comment saying so
(`staged_protection.rs:685-687`). A later slice that narrows to 0016's exact set would have to
change that expectation on purpose.

Iteration 1's other mutation evidence still holds, since production is unchanged (part-before-sidx
→ C1 red; staged-after-inode → C2 red; global `part:` scan → D red; held treated as unresolvable →
the E(ii) tests red; identity-fill of an empty staged placement → the empty-placement E(ii) test red).

### DST

`./engine/xtask.sh dst` (madsim clippy, then `cargo test -p wyrd-dst` at 50 seeds) with the fix:
green; custodian suite `18 passed`, including `gc_staged_build_under_concurrent_handoffs` and
`gc_staged_build_reaches_every_landing` (so all five landings, including the new `Completing`-only
one, were reached).

## Alternatives considered and rejected

- **One separate test per session state** (for example `a_gc_keeps_a_completing_sessions_parts`).
  It would give finer failure names, but the shared world already labels every fragment with its
  state and record kind in the assertion message, and splitting would add about 8 test functions
  (roughly 4 KB of patch) to a patch already over the size threshold. Rejected.
- **Seed the non-`Open` sessions with `part:` records only** (0016's minimum) and no owned entries.
  That would leave the brief-scoped behaviour (owned entries of every listed session) untested,
  and the mutation table's last row would pass. Rejected; both record kinds are real protocol
  states in every phase (`0016:573-576`: owned residue is walked on every path out of `Open`).
- **Keep the `part:` record through the flip and delete it in a later drain step** (what 0016
  actually does, `0016:794-796`) in C(ii) and the DST. The brief defines C(ii)'s publication as one
  batch that writes the inode and removes the `part:` record, which is the harder case for the
  build (the source disappears the instant the destination appears). I kept the brief's batch and
  added the session flip to it, as the carry-forward asks; the comment at `staged_protection.rs:891-894`
  says so.
- **A separate seed-chosen delay for the fence** in the DST. With the pass at 4 ms the fence must
  land before 5 ms, so only `commit_at = 0` reaches the `Completing` listing whatever the fence
  delay; an extra knob would add draws without adding reachable landings. Rejected in favour of
  the fence right after the part commit and a wider publication span.
- **Moving the pass start later** so more `commit_at` values reach the `Completing` listing. It
  shifts every other landing and needs a wider `commit_at` span too, for no gain the coverage walk
  doesn't already give. Rejected.

## Size

`patch.diff` is 119,119 bytes (iteration 1: 108,731; the human waived the 100 KB threshold at
106 KB as a minor overage). The whole +10 KB is the test additions the carry-forward asked for:
+6.1 KB in `staged_protection.rs`, +4.3 KB in the DST file. Production files are unchanged.

## The three refutation questions

- **(a) Genuine red?** Yes. `run-verify.sh` reverted every production file the patch changes, kept
  the new test, and got `0 passed; 11 failed`, each on a property assertion (lines above). For the
  carry-forward's gap specifically: with the reviewer's "skip non-`Open`" mutation, A, B, C2 and D
  go red and both DST property-13 tests go red; with iteration 1's tests the same mutation was green.
- **(b) Production path?** Yes. The tests call the public production entry points
  (`wyrd_custodian::reconcile_step` with `GcContext`, `wyrd_custodian::reconcile_after_restore`),
  which run the real `referenced_fragments` → `staged_fragments` build and the real restore mark
  gate. Only the stores are doubles. Staged records are real bytes accepted by the production
  decoders, in the state they claim, and marks are written by production `mark_orphaned`.
- **(c) Fixture includes the fault?** Yes. Every staged fragment carries an `orphan:` mark past
  grace, so only protection stops the reclaim. The sessions are now in the states the protocol
  actually publishes and drains from, so a state filter meets the records it would drop. The
  handoffs land mid-build under the session state the protocol requires (the double enforces the
  batch preconditions), and the DST writer lands the part commit, the fence and the publication
  concurrently at seed-chosen instants. Controls prove each pass still acts.

## Commit-readiness

- `cargo fmt --all` applied; `cargo fmt --all -- --check` clean. It changed only my two test files.
- `cargo clippy -p wyrd-custodian --all-targets -- -D warnings` clean; the DST crate's madsim clippy
  ran clean inside `./engine/xtask.sh dst`.
- Full `cargo xtask ci` through `./engine/xtask.sh ci`, after the final edits:
  **`xtask ci: all checks passed`** (exit 0). It ran typos, docs lint and render, the gitlink and
  unsafe guards, `cargo fmt --check`, clippy `--workspace --all-targets`, build, `cargo test
  --workspace` (`staged_protection`: 11 passed; `cli::tests::restore_verdict_names_the_blocking_records…`
  ok), machete, deny (its `license-not-encountered` warning is pre-existing and not a failure),
  statics, deploy-guard, and the madsim DST clippy + test at 50 seeds (custodian suite 18 passed,
  both property-13 tests ok).
- `patch.diff` was produced from the worktree whose HEAD is `78f9859`, and `run-verify.sh` applied
  it cleanly to a fresh `origin/main` checkout.

## External dependencies

`typos` and `docs-renderer` (the brief's list) were present and exercised by `cargo xtask ci`. No
dependency outside the brief's list was needed, so there is no NEEDS-HUMAN external-dependency item.
