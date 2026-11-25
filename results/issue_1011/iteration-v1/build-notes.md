# Build notes — #1011 core: Complete refuses part sets past the value ceiling (636.5e)

Base: the stacked integration tip `b74afe3` (`pdca-integration/main-r9dd123c9ed70`, child-2 #1008 and
child-4 #1010 already folded in). Every `path:line` below is on that base unless it says "patched".

## The brief's citations, re-resolved on `b74afe3`

| Brief (`4bac538`) | This base |
|---|---|
| `MAX_VALUE_BYTES`, `metadata.rs:549` | `crates/core/src/metadata.rs:577` |
| part-set range encoding, `multipart.rs:3184-3193` | `crates/core/src/multipart.rs:3339-3373` (`PartNumberSet`) |
| obligation payload, `multipart.rs:3619-3632` | `multipart.rs:3904-3918` (`RetirePayload`), rows `:4291-4325` |
| `RetireObligation`, `:3913` | `multipart.rs:4204-4373` |
| `SessionTooLarge`, `:4515-4526` | `multipart.rs:4929-4940` |
| `all` reserved for teardown, `:3174-3177` | `multipart.rs:3455-3460` (`PartScope::All`) |
| part number format bound, `:939` | `multipart.rs:1009` (`MAX_PART_NUMBER`) |
| ceiling wrapper, `restore_completing_fence.rs:473-486` | same lines; the refusal itself is `:115-118` |

## What the change does

Complete's flip writes each part set inside one obligation value: the published parts'
`retire:records:{parts}` and the unnamed parts' `retire:bytes:{parts}`
(`crates/core/src/multipart/complete.rs:1679-1695`, installed at `:1744-1761`; payload rows
`crates/core/src/multipart.rs:4291-4325`). A sparse set can encode past `MAX_VALUE_BYTES`
(`crates/core/src/metadata.rs:577`, 100,000). The fix measures both obligations inside the existing
`validate` step of the fence phase (`complete.rs:1072-1094`). That step already runs twice: before
the fence (`complete.rs:928`) and against the frozen set behind it (`complete.rs:978`). So one check
covers both places the brief's invariant needs:

- **Before the fence**: `Validation::Rejected(SessionTooLarge)` returns at once with zero commits
  (the existing `Ok(Validation::Rejected(refusal)) => return Ok(refused(refusal))` arm,
  `complete.rs:930`).
- **Against the frozen set**: the same rejection goes through the existing
  `ReleaseFor::Rejection` path (`complete.rs:989`), so the fence is released from cursor 0 with
  `attempts` given back (`Open@E+2`), exactly like an `InvalidPart` caught behind the fence.

Changes (patched line numbers):

- `crates/core/src/multipart.rs:4382` (patched) — new `pub(crate) fn RetireObligation::value()`: the payload
  through `metadata::encode`. The check measures this, and the flip now puts this
  (`complete.rs:1847-1849` patched), so the bytes measured are the bytes written by construction.
  The `{seg}` release row uses it too (`complete.rs:1311` patched); the bytes are unchanged
  (same `metadata::encode(payload)` expression).
- `crates/core/src/multipart/complete.rs:1154-1201` (patched) — new private `PartRetirement`:
  mints the two obligations from the named list and a part set through
  `RetireObligation::published_parts` / `unnamed_parts` (the code moved out of `FlipPlan::read`,
  base `complete.rs:1679-1695`), and `past_ceiling()` returns
  `Refusal::SessionTooLarge { bytes: value.len(), limit: MAX_VALUE_BYTES }` for the first one
  whose stored value is `> MAX_VALUE_BYTES`. Exactly at the ceiling fits.
- `complete.rs:1107-1148` (patched) — `validate` takes the fence epoch, and after the named parts
  are found builds the `PartRetirement` and refuses on `past_ceiling()`. `Validation::Valid` carries
  it (boxed, for clippy's `large_enum_variant`), and `HeldFence` keeps it
  (`complete.rs:417-419` patched). `FlipPlan` no longer mints its own copies; `flip()` installs
  `fence.retired` (`complete.rs:1836-1849` patched). So the obligations the flip writes are the
  very values validated behind the fence.
- Docs: the `SessionTooLarge` doc (`multipart.rs:4929-4940` base) now says Complete answers it and
  what `bytes` means there. The complete.rs module doc gains *The value ceiling* section and the
  table row; the `fence_complete` and `validate` docs say where the check runs.
- `docs/design/architecture/05-building-block-view.md:252` and `:254` — the living architecture
  doc. AGENTS.md's docs-currency rule makes an API answer change a merge requirement there. The
  brief's out-of-scope list names only 0016 and ADRs, so this does not cross it.
- Stale pointers fixed: `complete.rs:669` (base) `// deferred: #1011 — the value ceiling a flip's
  records are sized against.` is removed, because this patch is #1011. `complete.rs:266-267`,
  `multipart.rs:128-129` and `:3903` named #1011 as still to land.
- `multipart.rs:3339-3341` (base): the `PartNumberSet` doc said "the worst case — 10,000
  alternating part numbers — still fits inside one value". That is the false claim #842 found. It
  now gives the real numbers: 58,909 bytes for the widest set inside 1..=10,000, and 128,901 past
  it. I counted this as part of the fix's scope (it is the defect's own premise, in code), not as
  editing 0016.

## The numbers (all confirmed exactly by the test)

The real `RetirePayload` serializes nothing but `parts` for these rows. `session: false`, empty
`chunks`, `generation: None` and `seg: None` are all skipped (`multipart.rs:3904-3918`). So the brief's
pair did not need moving. Each `{"parts":…}` value is the bare set plus 10 bytes.

| Set | Bare set | `{parts}` value |
|---|---|---|
| odd 1..=9,999 (I1 published) | — | 58,901 |
| even 2..=10,000 (I1 unnamed) | — | 58,909 |
| odd 1..=19,999 (I2) | — | 128,901 |
| odd 1..=15,869 (I3 inside) | 99,981 | 99,991 |
| odd 1..=15,871 (I3 past) | 99,995 | 100,005 |
| at-ceiling set (added leg) | — | 100,000 |

Why 58,909 is the widest set inside S3's range: a canonical set's runs never touch, so 1..=10,000
holds at most 5,000 runs. A run `[a,b]` costs the same bytes as `[a,a]` when `a` and `b` have the
same digit count, so longer runs only lower the run count. With 5,000 singletons there is one
spare slot, so the set is "odds up to some point, then evens". Of those, all-evens has the most
digits (it trades the 1-digit `1` for the 5-digit `10000`). That gives 19,449 digits, i.e.
5,000 × 4 + 2 × 19,449 + 11 = 58,909.

**Added leg, not in the brief: (I3, at).** A value exactly at the ceiling must publish (`>` not
`>=`). Every all-singleton set has an odd value length (each `[n,n]` plus comma is even, the frame
adds 11), so 100,000 needs one run whose ends differ in width. The set: odd 1..=9,997, the run
`[9999,10000]`, even 10,002..=15,862, and 100,001, 100,003, 100,005, 100,007. It is
12 + 53,879 + 12 + 38,103 + 60 + 7,934 commas = 100,000. Without this leg a `>` → `>=` mutant
survives (mutation run below), and the repo runs `cargo mutants --in-diff` (C5-mutants).

## What the base actually does (the red)

The brief says the *store* refuses the flip (FoundationDB 2103). On this base the staged-publication
committer refuses it first, before any I/O for the flip: `prepare` → `check_flip_fence` →
`check_contribution` finds the put past the ceiling (`crates/core/src/staged_publication.rs:746-756`,
`PublicationError::ValueOverCeiling`). Complete then gives up with one release
(`complete.rs:1510-1531` → `give_up`). So the red is
`Err(Stopped { fence: Released { epoch: 1 }, cause: Publication(ValueOverCeiling { phase: Flip, key: "retire:records:s:…:1", bytes: 128901 }) })`
after two commits (fence, release). That is the brief's "child-2's J stopped error" all the same.
The test wrapper's 2103 refusal stays useful: it is the guarantee that nothing past the ceiling is
ever written, whichever layer would have caught it.

## Alternatives ruled out

- **Check only behind the fence (e.g. in `FlipPlan::read`).** Fails I2's "zero commits". Every
  oversize request would cost a fence + release (2 commits) and would cost any in-flight
  `UploadPart` its session bytes. Demonstrated: mutation (c) below turns I2 and I3-past red on
  "zero commits".
- **Check only before the fence.** Fails I4: a part committed between the two reads crosses the
  ceiling and the flip is refused. Demonstrated: mutation (b).
- **Measure the bare `PartNumberSet`, or a size formula, instead of the stored value.** Off by the
  10-byte `{"parts":…}` frame, so 99,995 / 100,005 slips through. Demonstrated: mutation (a). A
  formula would also drift from the codec; measuring `RetireObligation::value()` cannot.
- **Measure in `validate` but keep `FlipPlan::read` minting its own obligations.** About the same
  diff (−15 lines in `FlipPlan::read` either way), but then the measured and installed values are
  two separate constructions that only agree by convention. Carrying them in `HeldFence` (+3 lines
  for the field, +1 boxed field in `Validation::Valid`) makes them one value.
- **Split an oversize set across several obligation values, or use the `all` wildcard for the
  unnamed set.** The first changes the persisted format, which the brief puts out of scope (and it
  is 0016's design). The second is reserved for teardown: `multipart.rs:3457-3460` and
  `RecordError::RetireAllPartsWithoutSession` refuse it anywhere else.
- **Refuse the out-of-range part number at `UploadPart`.** This is the brief's open question for
  sign-off. It is out of scope here ("any change to `UploadPart` or the part-number range").

Not touched: the third flip obligation, `{generation}` for a superseded flat prior. Its chunk list is
the prior root's, capped at 158 refs × 315 bytes ≤ 50,000
(`multipart.rs:5367`), so it cannot cross 100,000. A segmented prior is named by its group, a few
bytes.

## Test — `crates/core/tests/multipart_complete_value_ceiling.rs` (new, 8 tests)

The test runs over `RedbMetadataStore::in_memory()` behind a `Ceiling` wrapper. The wrapper
refuses any batch putting a value `> MAX_VALUE_BYTES` with a definite error, as
`restore_completing_fence.rs:103-118` does. It logs every batch sent, refused or not, and can
apply one blind batch just before the next commit. Sessions come from Create (`create_session`,
#894). Parts are planted as canonical `part:`/`psum:` pairs, each checked by decode→re-encode.
Every stored obligation is compared byte for byte with an independent spelling of the range
encoding written in the test, and decoded through `decode_retire_obligation`.

- `the_wrapper_refuses_a_value_past_the_ceiling_and_takes_one_at_it`: the fixture is binding.
- **I1** `i1_inside_s3s_range_both_part_set_obligations_fit_and_it_publishes`.
- **I2** `i2_ten_thousand_sparse_parts_are_refused_with_zero_commits_and_stay_abortable`:
  `SessionTooLarge { bytes: 128901, limit: 100000 }`, zero commits, session bytes identical, then
  `abort_session` → `Fenced` (through the ceiling wrapper).
- **I3** `i3_a_value_just_inside_the_ceiling_publishes` (99,991 stored),
  `i3_a_value_past_the_ceiling_is_refused_though_its_bare_set_fits` (100,005, zero commits),
  `i3_a_value_exactly_at_the_ceiling_publishes` (100,000 stored).
- **I4** `i4_control_the_set_first_read_fits_and_publishes` (unnamed value 99,991 installed), and
  `i4_a_part_added_before_the_fence_is_refused_against_the_frozen_set_and_released`: the wrapper
  plants part 15,871 just before the fence commits. Result: `SessionTooLarge { bytes: 100005 }`;
  exactly 2 batches (the fence putting `Completing@1, attempts 1`, the release putting
  `Open@2, attempts 0`); the session ends `Open@2` with attempts 0; no obligation; no dirent.

The whole file runs in about 1 s.

## Red → green evidence

The command is the one C4-verify itself runs (`engine/scripts/run-verify.sh:441`), wrapped in
`timeout`: `timeout 1200 cargo test --quiet -p wyrd-core --test multipart_complete_value_ceiling`,
run in `$PDCA_WORKTREE`, with `CARGO_TARGET_DIR` under `$PDCA_SCRATCH`. I did **not** run
`run-verify.sh` itself. With `PDCA_LANE` unset it does `git worktree add/reset --hard/clean` on a
shared `../wyrd-verify` through the host's primary checkout (`run-verify.sh:129`, `:368-372`),
which is outside my roots and could clobber a concurrent lane's C4-verify. Check runs it.

- **Green (fix applied):** `test result: ok. 8 passed; 0 failed`.
- **Red (production reverted, test kept, base `b74afe3`):** `5 passed; 3 failed`. I2, I3-past and
  I4 fail with `expected Refused(SessionTooLarge { … }), got Err(Stopped(Stopped { …, fence:
  Released { epoch: 1 }, cause: Publication(ValueOverCeiling { phase: Flip, … }) }))`. I1, I3-inside,
  I3-at, I4-control and the wrapper test pass on the base, as they should: they publish there too
  (the committer's own check is `> MAX_VALUE_BYTES`, so 100,000 passes it).
- **The brief's demonstrated reds**, each a mutation of the fixed code, run against the final
  8-test file:
  - (a) measure the bare set (`metadata::encode(&obligation.payload().parts())`): `5 passed; 3
    failed` — I3-past fails (also I2, on `bytes`, and I4).
  - (b) check only before the fence (behind-fence `validate` ignoring the ceiling): `7 passed; 1
    failed` — I4, with the stopped error from the refused flip.
  - (c) check only behind the fence: `6 passed; 2 failed` — I2 and I3-past, on `zero commits`.
  - (extra) `>` → `>=`: `7 passed; 1 failed` — `i3_a_value_exactly_at_the_ceiling_publishes`,
    with `Refused(SessionTooLarge { bytes: 100000, … })`.

So the self-test the brief asks for holds: a guard on one side alone fails I4 or I2.

## Refuting my own test

- **(a) Genuine red? Yes.** With the production change reverted (`git checkout -- crates/core/src`
  on the base, test kept), I2, I3-past and I4 go red: 3 failed, 8 ran. Each of the three single-side
  or wrong-measure mutations also goes red, and so does `>=`.
- **(b) Production path? Yes.** Every leg calls the real `complete_upload` / `abort_session` /
  `create_session` against a real `RedbMetadataStore`. The wrapper only refuses oversize values
  and injects one blind write; it never re-implements Complete. The obligations checked are the
  ones the production flip stored, read back from the inner store.
- **(c) Fixture includes the fault? Yes.** The wrapper really enforces the ceiling: the wrapper
  test shows 100,001 bytes refused with nothing applied and 100,000 taken. The oversize sets are
  really planted (10,000 / 7,936 parts). The I4 plant really lands in the store before the fence
  commit: the refusal names 100,005 bytes, which only the frozen set including 15,871 produces.

## Commit-readiness

- `cargo fmt --all -- --check`: clean (the formatter rewrapped the new test file; production files
  were already clean).
- `cargo clippy -p wyrd-core --all-targets`: clean, after boxing `Validation::Valid::retired`
  (`large_enum_variant`) and moving two constant checks in the test to `const _: () = assert!(…)`.
- `typos` (1.48.0, the brief's one external dependency): clean.
- Full gate `./engine/xtask.sh ci` (→ `cargo xtask ci` in `$PDCA_WORKTREE`, under `timeout 5400`):
  **`xtask ci: all checks passed`**, exit 0. That covers typos, `lint_docs.py` and
  `render_site.py --check` (link audit OK), the repo guards, fmt, workspace clippy, build and test
  (the new file's 8 tests among them), cargo-deny (advisories, licenses, bans, sources), the
  conformance vectors, the statics and deploy guards, and the `wyrd-dst` clippy and test under
  `--cfg madsim`. One doc-comment edit landed after CI's first steps had run, so I re-ran fmt
  `--check`, `typos`, `lint_docs.py` and `cargo clippy -p wyrd-core --all-targets` on the final
  tree: all clean.
- `cargo doc -p wyrd-core --no-deps --document-private-items`: every link this patch adds
  resolves. The crate has pre-existing rustdoc errors (rustdoc is not a CI step); the only one in a
  file this patch touches is `max_part_bytes` → `one_value_bytes` at `multipart.rs:5397`, which
  is not from this change.

No external dependency beyond the brief's `typos` (1.48.0, present) was needed, so there is no
NEEDS-HUMAN item from Do.
