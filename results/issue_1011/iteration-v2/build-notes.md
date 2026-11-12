# Build notes — #1011 core: Complete refuses part sets past the value ceiling (636.5e)

Iteration 2. Base: the stacked integration tip `b74afe3` (`pdca-integration/main-r9dd123c9ed70`,
child-2 #1008 and child-4 #1010 already folded in). Every `path:line` below is on that base unless
it says "patched".

## What changed since iteration 1

The carry-forward finding: no test pinned the zero-commit refusal for the **unnamed** set. Every
zero-commit leg overflowed the published `retire:records:` set; the only leg overflowing the
unnamed `retire:bytes:` set (I4) did so behind the fence. A mutant whose check before the fence
measures only the published set passed all 8 tests.

- **New leg** `i4_an_unnamed_set_already_past_the_ceiling_is_refused_with_zero_commits`
  (`crates/core/tests/multipart_complete_value_ceiling.rs:676`, patched). Exactly the reviewer's
  case: parts {2} ∪ {1, 3, …, 15,871}, a Complete naming only [2], no plant. It asserts
  `Refused(SessionTooLarge { bytes: 100_005, limit: 100_000 })`, `take_sent().is_empty()` (zero
  commits), `Open@0` with `attempts` 0, session bytes byte-identical, and neither obligation
  stored. The module doc's (I4) bullet says so too (`:24-30`, patched).
- **Production code: unchanged from iteration 1.** The fix already measured both obligations before
  the fence (`crates/core/src/multipart/complete.rs:1138-1142` patched, run from both call sites
  `:961` and `:1011`). The finding was a test gap, not a code defect, so the rejected approach
  ("the check before the fence measures only the published set") was never the shipped code; the
  new leg now proves it isn't.
- The reviewer's mutant, rebuilt and run against the final file: **8 passed; 1 failed**, the new
  leg, on `zero commits` (`:691`). It is the only leg that catches it.

### Tried and dropped: a doubt-path leg

The matrix is {before the fence, behind it} × {published, unnamed}. I tried to pin the fourth
cell, "behind the fence, published set", with a leg whose first fence commit answers
`CommitUnknownResult { may_still_commit: true }`, since `fence_complete` skips the check before the
fence while a fence of the call is in doubt (`complete.rs:960`). It sent **zero** commits: on the
first round nothing is in doubt yet (`Doubt::default()`, `complete.rs:914`), so the check before
the fence always runs once, and the published obligation's value depends only on the named list
(`PartRetirement::of`, `complete.rs:1165-1181` patched), never on what the store holds. So that
cell can never refuse anything the first-round check didn't already refuse: a mutant dropping the
published set from only the check behind the fence is equivalent (same behaviour on every input).
The leg tested nothing I2 doesn't, so I removed it and the wrapper's unknown-outcome mode with it.

## The brief's citations, re-resolved on `b74afe3`

| Brief (`4bac538`) | This base |
|---|---|
| `MAX_VALUE_BYTES`, `metadata.rs:549` | `crates/core/src/metadata.rs:577` |
| part-set range encoding, `multipart.rs:3184-3193` | `crates/core/src/multipart.rs:3339-3373` (`PartNumberSet`; `:3378` patched) |
| obligation payload, `multipart.rs:3619-3632` | `multipart.rs:3904-3918` (`RetirePayload`; `:3910` patched), rows `:4291-4325` |
| `RetireObligation`, `:3913` | `multipart.rs:4204-4373` (`impl` at `:4216` patched) |
| `SessionTooLarge`, `:4515-4526` | `multipart.rs:4929-4940` (`:4941-4964` patched) |
| `all` reserved for teardown, `:3174-3177` | `multipart.rs:3455-3460` (`PartScope::All`) |
| part number format bound, `:939` | `multipart.rs:1009` (`MAX_PART_NUMBER`; `:1010` patched) |
| ceiling wrapper, `restore_completing_fence.rs:473-486` | same lines; the refusal itself is `:115-118` |

## What the change does

Complete's flip writes each part set inside one obligation value: the published parts'
`retire:records:{parts}` and the unnamed parts' `retire:bytes:{parts}`
(`crates/core/src/multipart/complete.rs:1679-1695`, installed at `:1744-1761`; payload rows
`crates/core/src/multipart.rs:4291-4325`). A sparse set can encode past `MAX_VALUE_BYTES`
(`crates/core/src/metadata.rs:577`, 100,000). The fix measures both obligations inside the existing
`validate` step of the fence phase (`complete.rs:1072-1094` base; `:1116-1148` patched). That step
already runs twice: before the fence (`complete.rs:928` base, `:961` patched) and against the frozen
set behind it (`complete.rs:978` base, `:1011` patched). So one check covers both places the
brief's invariant needs:

- **Before the fence**: `Validation::Rejected(SessionTooLarge)` returns at once with zero commits
  (the existing `Ok(Validation::Rejected(refusal)) => return Ok(refused(refusal))` arm,
  `complete.rs:963` patched).
- **Against the frozen set**: the same rejection goes through the existing
  `ReleaseFor::Rejection` path (`complete.rs:1027` patched), so the fence is released from cursor 0
  with `attempts` given back (`Open@E+2`), exactly like an `InvalidPart` caught behind the fence.

Changes (patched line numbers):

- `crates/core/src/multipart.rs:4382` — new `pub(crate) fn RetireObligation::value()`: the payload
  through `metadata::encode`. The check measures this, and the flip now puts this
  (`complete.rs:1848`), so the bytes measured are the bytes written by construction. The `{seg}`
  release row uses it too (`complete.rs:1311`); the bytes are unchanged (same
  `metadata::encode(payload)` expression).
- `crates/core/src/multipart/complete.rs:1150-1201` — new private `PartRetirement`: mints the two
  obligations from the named list and a part set through `RetireObligation::published_parts` /
  `unnamed_parts` (the code moved out of `FlipPlan::read`, base `complete.rs:1679-1695`), and
  `past_ceiling()` (`:1192`) returns `Refusal::SessionTooLarge { bytes: value.len(), limit:
  MAX_VALUE_BYTES }` for the first one whose stored value is `> MAX_VALUE_BYTES`. Exactly at the
  ceiling fits.
- `complete.rs:1116-1148` — `validate` takes the fence epoch, and after the named parts are found
  builds the `PartRetirement` and refuses on `past_ceiling()` (`:1138-1142`). `Validation::Valid`
  carries it (boxed, for clippy's `large_enum_variant`), and `HeldFence` keeps it (`:419`).
  `FlipPlan` no longer mints its own copies; `flip()` installs `fence.retired` (`:1837`). So the
  obligations the flip writes are the very values validated behind the fence.
- Docs: the `SessionTooLarge` doc (`multipart.rs:4941-4964`) now says Complete answers it and what
  `bytes` means there. The complete.rs module doc gains *The value ceiling* section (`:52`) and the
  table row (`:22`); the `fence_complete` and `validate` docs say where the check runs.
- `docs/design/architecture/05-building-block-view.md:252` and `:254` — the living architecture
  doc. AGENTS.md's docs-currency rule makes an API answer change a merge requirement there. The
  brief's out-of-scope list names only 0016 and ADRs, so this does not cross it. The text already
  covers both sets ("the named parts, and the session's other parts").
- Stale pointers fixed: base `complete.rs:669` `// deferred: #1011 — the value ceiling a flip's
  records are sized against.` is removed, because this patch is #1011. Base `complete.rs:266-267`,
  `multipart.rs:128-129` and `:3903` named #1011 as still to land.
- Base `multipart.rs:3339-3341`: the `PartNumberSet` doc said "the worst case — 10,000 alternating
  part numbers — still fits inside one value". That is the false claim #842 found. It now gives the
  real numbers: 58,909 bytes for the widest set inside 1..=10,000, and 128,901 past it. I counted
  this as part of the fix's scope (it is the defect's own premise, in code), not as editing 0016.

## The numbers (all confirmed exactly by the test)

The real `RetirePayload` serializes nothing but `parts` for these rows. `session: false`, empty
`chunks`, `generation: None` and `seg: None` are all skipped (`multipart.rs:3904-3918` base). So the
brief's pair did not need moving. Each `{"parts":…}` value is the bare set plus 10 bytes.

| Set | Bare set | `{parts}` value |
|---|---|---|
| odd 1..=9,999 (I1 published) | — | 58,901 |
| even 2..=10,000 (I1 unnamed) | — | 58,909 |
| odd 1..=19,999 (I2) | — | 128,901 |
| odd 1..=15,869 (I3 inside; I4 unnamed, first read) | 99,981 | 99,991 |
| odd 1..=15,871 (I3 past; I4 frozen; new leg's unnamed) | 99,995 | 100,005 |
| {2} (I4 and the new leg, published) | — | 17 |
| at-ceiling set (I3 at) | — | 100,000 |

Why 58,909 is the widest set inside S3's range: a canonical set's runs never touch, so 1..=10,000
holds at most 5,000 runs. A run `[a,b]` costs the same bytes as `[a,a]` when `a` and `b` have the
same digit count, so longer runs only lower the run count. With 5,000 singletons there is one spare
slot, so the set is "odds up to some point, then evens". Of those, all-evens has the most digits
(it trades the 1-digit `1` for the 5-digit `10000`). That gives 19,449 digits, i.e.
5,000 × 4 + 2 × 19,449 + 11 = 58,909.

**Leg not in the brief: (I3, at).** A value exactly at the ceiling must publish (`>` not `>=`).
Every all-singleton set has an odd value length (each `[n,n]` plus comma is even, the frame adds
11), so 100,000 needs one run whose ends differ in width: odd 1..=9,997, the run `[9999,10000]`,
even 10,002..=15,862, and 100,001, 100,003, 100,005, 100,007 (`:550`). Without it a `>` → `>=`
mutant survives, and the repo runs `cargo mutants --in-diff` (C5-mutants).

## What the base actually does (the red)

The brief says the *store* refuses the flip (FoundationDB 2103). On this base the
staged-publication committer refuses it first, before any I/O for the flip: `prepare` →
`check_flip_fence` → `check_contribution` finds the put past the ceiling
(`crates/core/src/staged_publication.rs:746-756`, `PublicationError::ValueOverCeiling`). Complete
then gives up with one release (`complete.rs:1510-1531` base → `give_up`). So the red is, for the
new leg:
`Err(Stopped { fence: Released { epoch: 1 }, cause: Publication(ValueOverCeiling { phase: Flip, key: "retire:bytes:s:…:1", bytes: 100005 }) })`
after two commits (fence, release). That is the brief's "child-2's J stopped error" all the same.
The wrapper's 2103 refusal stays useful: it guarantees nothing past the ceiling is ever written,
whichever layer would have caught it.

## Alternatives ruled out

- **Check only behind the fence.** Fails I2's, I3-past's and the new leg's "zero commits". Every
  oversize request would cost a fence + release (2 commits) and would cost any in-flight
  `UploadPart` its session bytes. Demonstrated: mutant (c) below.
- **Check only before the fence.** Fails I4: a part committed between the two reads crosses the
  ceiling and the flip is refused. Demonstrated: mutant (b).
- **Measure the bare `PartNumberSet`, or a size formula, instead of the stored value.** Off by the
  10-byte `{"parts":…}` frame, so 99,995 / 100,005 slips through. Demonstrated: mutant (a). A
  formula would also drift from the codec; measuring `RetireObligation::value()` cannot.
- **Measure in `validate` but keep `FlipPlan::read` minting its own obligations.** About the same
  diff (−15 lines in `FlipPlan::read` either way), but then the measured and installed values are
  two separate constructions that only agree by convention. Carrying them in `HeldFence` (+3 lines
  for the field, +1 boxed field in `Validation::Valid`) makes them one value.
- **Split an oversize set across several obligation values, or use the `all` wildcard for the
  unnamed set.** The first changes the persisted format, which the brief puts out of scope (and it
  is 0016's design). The second is reserved for teardown: `multipart.rs:3455-3460` and
  `RecordError::RetireAllPartsWithoutSession` refuse it anywhere else.
- **Refuse the out-of-range part number at `UploadPart`.** The brief's open question for sign-off.
  Out of scope here ("any change to `UploadPart` or the part-number range").

Not touched: the third flip obligation, `{generation}` for a superseded flat prior. Its chunk list
is the prior root's, capped at 158 refs × 315 bytes ≤ 50,000 (`multipart.rs:5367` base), so it
cannot cross 100,000. A segmented prior is named by its group, a few bytes.

## Test — `crates/core/tests/multipart_complete_value_ceiling.rs` (new, 9 tests)

Runs over `RedbMetadataStore::in_memory()` behind a `Ceiling` wrapper that refuses any batch
putting a value `> MAX_VALUE_BYTES` with a definite error, as `restore_completing_fence.rs:103-118`
does. It logs every batch sent, refused or not, and can apply one blind batch just before the next
commit. Sessions come from Create (`create_session`, #894). Parts are planted as canonical
`part:`/`psum:` pairs, each checked by decode→re-encode. Every stored obligation is compared byte
for byte with an independent spelling of the range encoding written in the test, and decoded
through `decode_retire_obligation`.

- `:466` `the_wrapper_refuses_a_value_past_the_ceiling_and_takes_one_at_it` — the fixture binds.
- `:482` **I1** — publishes; obligations stored at 58,901 and 58,909.
- `:503` **I2** — `SessionTooLarge { bytes: 128901 }`, zero commits, session bytes identical, then
  `abort_session` → `Fenced` through the wrapper.
- `:530` **I3 inside** (99,991 stored), `:562` **I3 at** (100,000 stored), `:580` **I3 past**
  (100,005, zero commits).
- `:611` **I4 control** (unnamed value 99,991 installed); `:630` **I4** — the wrapper plants part
  15,871 just before the fence commits: `SessionTooLarge { bytes: 100005 }`, exactly 2 batches
  (fence `Completing@1, attempts 1`; release `Open@2, attempts 0`), no obligation, no dirent.
- `:676` **I4 already past** (new) — see above.

The whole file runs in about 1 s.

## Red → green evidence

Command (what C4-verify itself runs, wrapped in `timeout`):
`timeout 1200 cargo test --quiet -p wyrd-core --test multipart_complete_value_ceiling`, in
`$PDCA_WORKTREE`. I did not run `run-verify.sh` itself: with `PDCA_LANE` unset it resets a shared
`../wyrd-verify` worktree through the host's primary checkout, outside my roots. Check runs it.

- **Green (fix applied):** `test result: ok. 9 passed; 0 failed`.
- **Red (production and docs reverted with `git checkout -- crates/core/src docs`, test kept):**
  `5 passed; 4 failed` — I2, I3-past, I4 and I4-already-past, each
  `expected Refused(SessionTooLarge { … }), got Err(Stopped(… Publication(ValueOverCeiling …)))`.
  I1, I3-inside, I3-at, I4-control and the wrapper test pass on the base, as they should: they
  publish there too.
- **Mutants of the fixed code**, each run against the final 9-test file:

| Mutant | Result | Fails |
|---|---|---|
| (a) measure the bare set | 5 passed; 4 failed | I2, I3-past, I4, I4-already-past |
| (b) check only before the fence | 8 passed; 1 failed | I4 |
| (c) check only behind the fence | 6 passed; 3 failed | I2, I3-past, I4-already-past |
| reviewer's: before the fence, published set only | 8 passed; 1 failed | **I4-already-past** (`zero commits`) |
| before the fence, unnamed set only | 7 passed; 2 failed | I2, I3-past |
| behind the fence, published set only | 6 passed; 3 failed | I4 (also I1, I4-control: this crude mutant drops the installed unnamed obligation) |
| `>` → `>=` | 8 passed; 1 failed | I3-at |

So the brief's self-test holds: a guard on one side alone fails I4 or I2, and a guard on one set
alone fails I4-already-past or I2.

## Refuting my own test

- **(a) Genuine red? Yes.** With production reverted on `b74afe3` (test kept), 4 of 9 fail,
  including the new leg with the stopped error from the refused flip. Every single-side,
  single-set and wrong-measure mutant also goes red (table above).
- **(b) Production path? Yes.** Every leg calls the real `complete_upload` / `abort_session` /
  `create_session` against a real `RedbMetadataStore`. The wrapper only refuses oversize values and
  injects one blind write; it never re-implements Complete. The obligations checked are the ones the
  production flip stored, read back from the inner store.
- **(c) Fixture includes the fault? Yes.** The wrapper really enforces the ceiling (its own test:
  100,001 bytes refused with nothing applied, 100,000 taken). The oversize sets are really planted
  (10,000 / 7,936 / 7,937 parts). The I4 plant really lands before the fence commit: the refusal
  names 100,005 bytes, which only the frozen set including 15,871 produces. In the new leg the
  oversize set is the unnamed one, and the published one is 17 bytes (asserted).

## The C4 diff-coverage failure in iteration 1

"patch.diff does not apply on origin/main" is structural, not a patch defect. This bundle is
stacked: `origin/main` (`d04e507`) is an ancestor of the base `b74afe3`, which adds 18 integration
commits (#1008, #1009, #1010, #1028-#1030 among them) that are not on `origin/main` yet. The patch
edits code those parents introduced (`HeldFence`, `FlipPlan`, the fence release), so it cannot apply
to `origin/main` without folding the parents in, which would break the brief's scope. C4-verify
already uses the folded base (`$PDCA_VERIFY_BASE`); the diff-coverage gate measuring against
`origin/main` looks like a harness gap for stacked bundles, so I left it as is. The patch applies
cleanly to `b74afe3` (`git apply --check`).

## Commit-readiness

- `cargo fmt --all -- --check`: clean.
- `cargo clippy -p wyrd-core --all-targets -- -D warnings`: clean.
- `typos` on all four files: clean. `git diff --check`: clean.
- The repo has no git hooks configured (`core.hooksPath` unset, no pre-commit config).
- Full gate `./engine/xtask.sh ci` (→ `cargo xtask ci` in `$PDCA_WORKTREE`, under `timeout 5400`):
  **`xtask ci: all checks passed`**, exit 0, on the final tree. It ran typos, `lint_docs.py`,
  `render_site.py --check`, the gitlink/unsafe/blackbox guards, fmt, workspace clippy, build and
  test (the new file's 9 tests among them: `9 passed`), cargo-machete, cargo-deny (advisories,
  licenses, bans, sources), conformance (5 valid + 6 invalid vectors), the statics and deploy
  guards, and `wyrd-dst` clippy and test under `--cfg madsim`.
  The **first** run failed one unrelated test,
  `wyrd-gateway-s3` `tests::a_bodyless_response_is_recorded_complete_not_aborted`
  (`crates/gateway-s3/src/lib.rs:4919`): its captured access log came back empty. This patch does
  not touch `gateway-s3` or anything it calls in that test (it checks how a 204 response is
  logged). Run alone it passed 3 of 3 (`97 passed` each time), and the full re-run passed, so I
  read it as a flake under load. Worth a tracking issue if it recurs; out of scope here.

No external dependency beyond the brief's `typos` (present) was needed, so there is no NEEDS-HUMAN
item from Do.
