# Build notes — #1011 core: Complete refuses part sets past the value ceiling (636.5e)

Iteration 3. Base: the stacked integration tip `b74afe3` (`pdca-integration/main-r9dd123c9ed70`,
with #1008, #1009, #1010, #1028, #1029 and #1030 folded in). Every `path:line` below is on that
base unless it says "patched".

## What this iteration addresses

Round 2 raised no code finding. The reviewer passed C1–C5, T1–T3 and T5; the adversary "could not
refute the fix" and routed nothing back to Do; the code-review lens had no findings. The two items
that sent it round again are not build work, so the production code and the test are **unchanged
from iteration 2** (`patch.diff` is byte-identical to `iteration-v2/patch.diff`, checked with
`cmp`). Changing working code just to look different would make things worse. What I did instead:

1. **The T4 prior-art audit** the reviewer could not run from its snapshot ("one synthetic base
   commit and no remotes"). Done below, against GitHub and the real history. Result: nothing on
   `getwyrd/wyrd`, merged, open or closed-unmerged, already does this or was turned down.
2. **The failing C4 diff-coverage row.** I traced why it can never pass on a stacked bundle, and
   measured the diff coverage myself on the right base: **54 of 55** instrumentable changed lines
   from the new test alone (98.2%), **55 of 55** together with the existing `multipart_complete`
   suite.
3. Re-ran red→green, four mutants, fmt, clippy, `typos` and the docs linter on this base.

### 1. Prior-art audit (for the T4 NEEDS-HUMAN item)

Read-only `gh` queries on `getwyrd/wyrd`, run 2026-11-12:

- **Origin of the finding: PR #856** (merged 2026-10-07, `Fixes #842`). Its body records that a
  `{session, parts}` payload for 10,000 alternating part numbers is 128,916 bytes against
  `MAX_VALUE_BYTES`. It fixed only the **restore fence**, by switching that writer to the `all`
  wildcard, and adds the ceiling-refusing store double that the brief tells this test to copy
  (`crates/custodian/tests/restore_completing_fence.rs:473-486`). Its "Not covered" list hands the
  other writers on: the reaper and operator-abort `Completing → Aborting` rows go to #656/#659. It
  does **not** touch Complete's flip, so it is not a duplicate of this slice.
- **Peer: PR #908** (merged 2026-10-18), Create's `SessionTooLarge` refusal (#894). It is the only
  PR that matches `SessionTooLarge`. This patch reuses that variant; it adds none (brief scope).
- **Parents, all still open:** #1050 (#1008, flat publication), #1059 (#1009, segmented), #1063
  (#1010, unknown outcomes), #1064 (#1030). #1032 (#1007, the fence) merged 2026-11-02. No PR exists
  for `head:enhancement/1011*`.
- **Closed without merging:** the only non-dependency-bump one is #647 ("segmented chunk maps
  beyond one value", closed 2026-07-30). It touches chunk maps in `metadata.rs`/`read.rs`/`write.rs`
  and custodian, not multipart part sets. Nothing on this topic was rejected.
- **The alternative in the brief's open question:** issue #669 (open) already plans to refuse a
  part number above 10,000 in the S3 `UploadPart` verb. #927 (open, the core `UploadPart` commit)
  does not mention a part-number bound or this ceiling.
- **History of the edited files:** `git log b74afe3 -- crates/core/src/multipart/complete.rs` is
  four commits: `b546557` (#1007), `10f03fd` (#1008), `a0a6479` (#1009), `790214d` (#1010). None
  sizes the part sets. The base still carries `// deferred: #1011 — the value ceiling a flip's
  records are sized against.` (`crates/core/src/multipart/complete.rs:669`), which this patch
  resolves and removes.
- Search terms used: `"value ceiling"`, `SessionTooLarge`, `"MAX_VALUE_BYTES" multipart`,
  `"part set" ceiling`, `value_too_large`, `Complete in:title`, plus the full closed-PR list.

### 2. Why C4-diff-cov keeps failing, and the numbers it would have shown

The row reports "patch.diff does not apply on origin/main" (`iteration-v2/gate-logs/C4-diff-cov.log:10`).
That is true and cannot be fixed from Do:

- This patch edits code the open parents add (`HeldFence`, `FlipPlan`, the fence release). Remote
  `main` is still `d04e507` (`git ls-remote origin refs/heads/main`), the merge base, and
  `b74afe3` is 18 commits ahead of it. So it cannot apply to `origin/main` until the parents merge.
- C4-verify gets the stacked base and passes; C4-diff-cov does not. In `pdca.toml`, C4-verify has
  `verifies_base = true` (`pdca.toml:1503`) and C4-diff-cov has `verifies_base = false`
  (`pdca.toml:1540`). The comment at `pdca.toml:1104-1111` says the base ladder
  (`$PDCA_BASE` / `$PDCA_VERIFY_BASE` / `$PDCA_BRIEF_BASE`) goes **only** to a row with
  `verifies_base = true`. `run-diff-cov.sh` still asks for that base (`engine/scripts/run-diff-cov.sh:685`,
  via `run-verify.sh --print-base`), gets none of it, and ends up at `origin/main`
  (`engine/scripts/run-verify.sh:263-268`). That is my reading of the config; the human should
  confirm it. Setting `verifies_base = true` on the diff-cov row, or fixing it in the harness
  upstream, would be an Act / harness change. It is not the target patch's job, and another Do
  iteration will hit the same failure.

Measured by hand on the right base (the patched `$PDCA_WORKTREE` at `b74afe3`), with
`cargo llvm-cov -p wyrd-core --test <name> --lcov` (instrumented build under `$PDCA_SCRATCH`), then
checking each added line of `patch.diff` against the lcov `DA` entries. Only lines lcov reports
(code, not comments) count, as the gate counts them. 169 changed lines, which matches the gate's
`changed_lines: 169` (`coverage/diff-cov.json`).

| Test target | `multipart.rs` | `complete.rs` | Total |
|---|---|---|---|
| `multipart_complete_value_ceiling` (this patch's test, what the gate runs) | 3/3 | 51/52 | **54/55 = 98.2%** |
| `multipart_complete` (existing 85 tests, all pass with the patch) | 3/3 | 48/52 | 51/55 |
| both | 3/3 | 52/52 | **55/55** |

The one line the new test does not reach is `complete.rs:1311` (patched), `.put(segments.key(),
segments.value())` in `OwnFence::release_batch`: the release after a segment batch landed (#1009),
switched from `metadata::encode(segments.payload())` to the same bytes through `value()`. The
existing suite covers it. The four lines that suite misses (`:1141`, `:1197-1199`, the refusal
itself) are what the new test covers.

## The brief's citations, re-resolved on `b74afe3`

| Brief (`4bac538`) | This base |
|---|---|
| `MAX_VALUE_BYTES`, `metadata.rs:549` | `crates/core/src/metadata.rs:577` |
| part-set range encoding, `multipart.rs:3184-3193` | `crates/core/src/multipart.rs:3339-3373` (`PartNumberSet` doc; struct `:3378` patched) |
| obligation payload, `multipart.rs:3619-3632` | `multipart.rs:3904-3918` (`RetirePayload`; `:3910` patched), part-set rows `:4301-4316` patched |
| `RetireObligation`, `:3913` | `multipart.rs:4205` (struct `:4210`, `impl` `:4216` patched; new `value()` `:4382` patched) |
| `SessionTooLarge`, `:4515-4526` | `multipart.rs:4929-4940` (now `:4941-4964` patched) |
| `all` reserved for teardown, `:3174-3177` | `multipart.rs:3448-3455` (`PartScope` doc and enum; `RecordError::RetireAllPartsWithoutSession` `:462`) |
| part-number format bound, `:939` | `multipart.rs:1009` (`MAX_PART_NUMBER`, `:1010` patched) |
| ceiling wrapper, `restore_completing_fence.rs:473-486` | same lines |

## What the change does

Complete's flip writes each part set inside one obligation value: the published parts'
`retire:records:{parts}` and the unnamed parts' `retire:bytes:{parts}` (base
`crates/core/src/multipart/complete.rs:1679-1695`, installed at `:1744-1761`). A sparse set can
encode past `MAX_VALUE_BYTES` (`crates/core/src/metadata.rs:577`, 100,000). The fix measures both
obligations inside the existing `validate` step of the fence phase (base `complete.rs:1072-1094`,
patched `:1116-1148`). That step already runs twice: before the fence (patched `:961`) and against
the frozen set behind it (patched `:1011`). One check covers both places the invariant needs:

- **Before the fence:** `Validation::Rejected(SessionTooLarge)` returns at once with zero commits,
  through the existing arm (patched `:963`).
- **Against the frozen set:** the same rejection goes through the existing `ReleaseFor::Rejection`
  path (patched `:1027`). The fence is released from cursor 0 with `attempts` given back
  (`Open@E+2`), like an `InvalidPart` caught behind the fence.

Changes (patched line numbers):

- `crates/core/src/multipart.rs:4382` — new `pub(crate) fn RetireObligation::value()`: the payload
  through `metadata::encode`. The check measures it and the flip puts it (`complete.rs:1848`), so
  the bytes measured are the bytes written. The `{seg}` release row uses it too (`complete.rs:1311`);
  same bytes as before.
- `crates/core/src/multipart/complete.rs:1150-1201` — new private `PartRetirement`. It mints the
  two obligations through `RetireObligation::published_parts` / `unnamed_parts` (code moved out of
  `FlipPlan::read`, base `:1679-1695`). Its `past_ceiling()` (`:1192`) returns
  `Refusal::SessionTooLarge { bytes: value.len(), limit: MAX_VALUE_BYTES }` for the first one whose
  stored value is `> MAX_VALUE_BYTES`. A value exactly at the ceiling fits.
- `complete.rs:1116-1148` — `validate` takes the fence epoch, builds the `PartRetirement` after the
  named parts are found, and refuses on `past_ceiling()` (`:1138-1142`). `Validation::Valid` carries
  it (boxed for clippy's `large_enum_variant`) and `HeldFence` keeps it (`:419`). `FlipPlan` no
  longer mints its own copies; `flip()` installs `fence.retired` (`:1836-1837`). So the flip writes
  exactly the values validated behind the fence.
- Docs: the `SessionTooLarge` doc (`multipart.rs:4941-4964`) says Complete answers it and what
  `bytes` means there. The `complete.rs` module doc gains *The value ceiling* (`:52`) and a table
  row (`:22`); the `fence_complete` and `validate` docs say where the check runs.
- `docs/design/architecture/05-building-block-view.md:252,254` — the living architecture doc. The
  rubric's docs-currency rule makes an API answer change a merge requirement there. The brief's
  out-of-scope list names only 0016 and ADRs.
- Stale pointers removed: base `complete.rs:669` (`// deferred: #1011 …`), base
  `complete.rs:266-267`, `multipart.rs:128-129` and `:3903`, which named #1011 as still to land.
- Base `multipart.rs:3339-3341`: the `PartNumberSet` doc said the worst case, 10,000 alternating
  part numbers, "still fits inside one value". That is the false claim #842 found. It now gives the
  real numbers. This is the defect's premise in code, not an edit to 0016.

## The numbers (each confirmed exactly by the test)

The real `RetirePayload` writes nothing but `parts` for these rows (`session: false`, empty
`chunks`, `generation: None`, `seg: None` are all skipped, `multipart.rs:3904-3918`). So the brief's
pair did not need moving. Each `{"parts":…}` value is the bare set plus 10 bytes.

| Set | Bare set | `{parts}` value |
|---|---|---|
| odd 1..=9,999 (I1 published) | — | 58,901 |
| even 2..=10,000 (I1 unnamed) | — | 58,909 |
| odd 1..=19,999 (I2) | — | 128,901 |
| odd 1..=15,869 (I3 inside; I4 unnamed at first read) | 99,981 | 99,991 |
| odd 1..=15,871 (I3 past; I4 frozen; I4-already-past unnamed) | 99,995 | 100,005 |
| {2} (I4 published) | — | 17 |
| `at_ceiling_parts()` (I3 at) | — | 100,000 |

58,909 is the widest set inside S3's range: runs in a canonical set never touch, so 1..=10,000 holds
at most 5,000 runs, and all-evens has the most digits of the "odds then evens" layouts that reach
5,000 singletons: 5,000 × 4 + 2 × 19,449 + 11 = 58,909.

## Test — `crates/core/tests/multipart_complete_value_ceiling.rs` (new, 9 tests)

Runs over `RedbMetadataStore::in_memory()` behind a `Ceiling` wrapper (`:257`) that refuses any
batch putting a value `> MAX_VALUE_BYTES` with a definite error, as
`restore_completing_fence.rs:473-486` does. It logs every batch sent and can apply one blind batch
just before the next commit. Sessions come from Create (`create_session`, #894). Parts are planted
as canonical `part:`/`psum:` pairs, each checked by decode → re-encode. Each stored obligation is
compared byte for byte with a spelling of the range encoding written in the test, and decoded
through `decode_retire_obligation`.

- `:466` the wrapper refuses 100,001 bytes with nothing applied and takes 100,000.
- `:482` **I1** publishes; obligations stored at 58,901 and 58,909.
- `:503` **I2** `SessionTooLarge { bytes: 128901 }`, zero commits (`:513`), session bytes unchanged,
  then `abort_session` → `Fenced`.
- `:530` **I3 inside** (99,991 stored), `:562` **I3 at** (100,000 stored), `:580` **I3 past**
  (100,005, zero commits `:591`).
- `:611` **I4 control** publishes; `:630` **I4** the wrapper plants part 15,871 just before the
  fence commits: `SessionTooLarge { bytes: 100005 }`, exactly 2 batches (fence `Completing@1`,
  attempts 1; release `Open@2`, attempts 0), no obligation, no dirent.
- `:676` **I4 already past** (added in iteration 2 for the round-1 finding): the unnamed set is past
  the ceiling at the first read; zero commits (`:691`), session bytes unchanged.

## Red → green, this iteration

`$PDCA_WORKTREE` at `b74afe3`, each run as `timeout 1500 cargo test --quiet -p wyrd-core --test
multipart_complete_value_ceiling` (the same command C4-verify runs). I did not call
`run-verify.sh`: it adds a git worktree and a `pdca-verify` branch through the host's primary
checkout (`engine/scripts/run-verify.sh:128-130`, `:369`), outside the roots I may write. Check
runs it.

- **Green (fix applied):** `9 passed; 0 failed`.
- **Red (`git checkout -- crates/core/src docs`, test kept):** `5 passed; 4 failed`: I2, I3-past,
  I4, I4-already-past. Each says `expected Refused(SessionTooLarge { … }), got
  Err(Stopped(… fence: Released { epoch: 1 }, cause: Publication(ValueOverCeiling { phase: Flip, key:
  "retire:…:1", bytes: … })))`. That is the brief's stopped error from a refused flip. On this base
  the staged-publication committer refuses the put before the store sees it
  (`crates/core/src/staged_publication.rs:746-756`). The five that pass are controls that publish
  with or without the fix.
- **Restored:** `git apply` of the production part; `git diff` byte-identical to `patch.diff`;
  `9 passed`.

Mutants of the fix, run this iteration against the final test file (script under `$PDCA_SCRATCH`;
the tree was restored and checked against `patch.diff` afterwards):

| Mutant | Result | Fails |
|---|---|---|
| `>` → `>=` at `complete.rs:1195` | 8 passed; 1 failed | I3-at |
| measure the bare set (value minus the `{"parts":}` frame) | 5 passed; 4 failed | I2, I3-past, I4, I4-already-past |
| check only before the fence | 8 passed; 1 failed | I4 |
| check only behind the fence | 6 passed; 3 failed | I2, I3-past, I4-already-past |

These are the brief's three demonstrated reds (check removed → I2; bare set → I3; before the fence
only → I4), plus the boundary. The round-1 mutant (before the fence, published set only) was run in
iteration 2 on this same code and base: 8 passed, 1 failed (I4-already-past). C5-mutants on
iteration 2's identical diff: 16 tried, 9 caught, 7 unviable, none surviving
(`iteration-v2/gate-logs/C5-mutants.log`).

## Refuting my own test

- **(a) Genuine red? Yes.** With the fix reverted on `b74afe3` and the test kept, 4 of 9 fail, each
  with the refused-flip stopped error. Every one-sided, wrong-measure and boundary mutant above also
  goes red.
- **(b) Production path? Yes.** Every leg calls the real `complete_upload`, `abort_session` and
  `create_session` against a real `RedbMetadataStore`. The wrapper only refuses oversize values and
  injects one blind write; it does not re-implement Complete. The obligations checked are the ones
  the production flip stored, read back from the inner store.
- **(c) Fixture includes the fault? Yes.** The wrapper really enforces the ceiling (its own test:
  100,001 refused with nothing applied, 100,000 taken). The oversize sets are really planted (10,000,
  7,936 and 7,937 parts). The I4 plant really lands before the fence commit: the refusal names
  100,005 bytes, which only a frozen set including 15,871 gives. In I4-already-past the oversize set
  is the unnamed one and the published one is 17 bytes (asserted).

## Alternatives ruled out (unchanged from iteration 2)

- **Check only behind the fence.** Every oversize request would cost a fence and a release
  (2 commits) and would cost any in-flight `UploadPart` its session bytes. Fails I2, I3-past and
  I4-already-past (mutant table).
- **Check only before the fence.** A part committed between the two reads crosses the ceiling and
  the flip is refused. Fails I4.
- **Measure the bare set, or a size formula.** Off by the 10-byte frame, so 99,995 / 100,005 slips
  through. A formula would also drift from the codec; measuring `RetireObligation::value()` cannot.
- **Measure in `validate` but keep `FlipPlan::read` minting its own obligations.** About the same
  size (the −15 lines in `FlipPlan::read` happen either way), but the measured and installed values
  would be two constructions that agree only by convention. Carrying them in `HeldFence` costs
  +3 lines for the field and +1 boxed field in `Validation::Valid`, and makes them one value.
- **Split an oversize set across several values, or use `all` for the unnamed set.** The first
  changes the persisted format (0016's design, out of scope). `all` is reserved for teardown
  (`multipart.rs:3448-3455`; `RecordError::RetireAllPartsWithoutSession`, `:462`, refuses it
  elsewhere).
- **Refuse the out-of-range part number at `UploadPart`.** The brief's open question for sign-off,
  and out of scope here.

Not touched: the third flip obligation, `{generation}` for a superseded flat prior. Its chunk list
is the prior root's, at most 158 refs × 315 bytes ≤ 50,000, so it cannot cross 100,000. A segmented
prior is named by its group, a few bytes.

## Commit-readiness

On the final tree in `$PDCA_WORKTREE`, this iteration:

- `cargo fmt --all -- --check`: clean.
- `cargo clippy -p wyrd-core --all-targets -- -D warnings`: exit 0.
- `typos` (1.48.0, whole repo, the brief's one external dependency): clean.
- `python3 docs/publishing/tools/lint_docs.py`: `lint_docs: OK`.
- `git diff --check`: clean. No git hooks are configured (`core.hooksPath` unset, no pre-commit
  config).
- The existing `multipart_complete` suite: `85 passed` with the patch (run under llvm-cov above).
- I did not re-run the full `cargo xtask ci` this time. The patch is byte-identical to iteration 2's
  on the same base, and that run passed C4-ci (`iteration-v2/gate-logs/C4-ci.log`, last line
  `xtask ci: all checks passed`). Check re-runs it.

No external dependency beyond `typos` (present) was needed, so Do raises no NEEDS-HUMAN external
dependency item. The two human items that remain are the brief's open policy question (refuse at
Complete or at `UploadPart`) and the diff-coverage row's base wiring above.
