# Build notes — issue #691, iteration 4 (multipart key grammar + validated identity types)

> Withheld from the reviewer by the driver. For the human at sign-off.

## 0. What this iteration is

Iteration 3's sign-off returned exactly **one** implementation-level finding, raised
independently by both T4 review passes and quoted verbatim in the carry-forward:

> Fix `parse_retire_mode` (crates/core/src/multipart.rs:712/729): it accepts truncated or
> non-UTF-8 keys such as `retire:bytes:` and `retire:bytes:\xff` on prefix match alone,
> contradicting its own doc-comment's fail-closed promise. **Validate the token following the
> prefix rather than dispatching on the prefix**; add the truncated and non-UTF-8 cases to the
> retire-grammar legs of `crates/core/tests/multipart_keys.rs`.

So this is **iteration 3's patch plus that fix** — not a re-derivation. Everything else in the
patch is unchanged from `iteration-v3/patch.diff`, which passed `C4-ci` (`cargo xtask ci`,
exit 0), `C5-mutants` (133 mutants, 50 caught, 83 unviable, **0 survivors**) and
`T4-contribution`, and whose C1/C2/C3/T1/T2/T3/T5 review cells were all PASS. The rebuild is
deliberately surgical: re-deriving the module would throw away that evidence and re-open
settled cells.

The diff against iteration 3 is, in production code, **exactly 7 non-comment lines**:

```
-pub fn parse_retire_mode(key: &[u8]) -> Result<RetireMode, RecordError> {   # renamed…
+fn retire_mode_prefix(key: &[u8]) -> Result<RetireMode, RecordError> {      # …to this, body verbatim
+pub fn parse_retire_mode(key: &[u8]) -> Result<RetireMode, RecordError> {   # new, 1-line body
+    parse_retire_key(key).map(|(mode, _)| mode)
+}
+
-    let mode = parse_retire_mode(key)?;                                     # in parse_retire_key
+    let mode = retire_mode_prefix(key)?;
```

(38 diff lines in `multipart.rs` and 44 in the test once doc-comments are counted.) Nothing
before `multipart.rs:690` moved — which also keeps every `review-rejected.md` anchor valid
(§7).

## 1. The defect, precisely

`parse_retire_mode` was the module's one **fail-open** function. Its old body (v3
`multipart.rs:711-728`) answered `Ok(RetireMode::Bytes)` for **any** byte string starting
`retire:bytes:` — including `retire:bytes:` itself (no token) and `retire:bytes:\xff` (token
bytes that are not UTF-8). Both name **no obligation**: `parse_retire_key` rejects them
downstream, so nothing is broken end-to-end *today*, and that is exactly why it survived three
rounds — the hole is only visible to a caller that dispatches on the mode alone.

That caller is the point of the function. 0016 puts the mode in the key precisely so that
disposal is decided at decode: `retire:bytes:` orphan-marks fragments, `retire:records:` must
never orphan anything, and "a boolean field misread once is silent data loss" — "The drain
dispatches on the prefix and **treats a `retire:` key it cannot parse as an error, never as a
default**" (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:434-440`, the
sentence at `:438-440`). A drain in #656–#659 that asked `parse_retire_mode` and got
`Ok(Bytes)` for a key it cannot read would orphan-mark on a guess — a permanent, data-losing
outcome, which is the **C-1** invariant this child exists to restore over the key space
(`docs/principles.md` §5 C-1). It is also the module's own stated rule turned inside out: one
spelling, one decision, enforced at decode (ADR-0045) — a function that answers a *decision*
from a prefix match is deciding without decoding.

## 2. The fix

`crates/core/src/multipart.rs` (post-patch line numbers):

| Line | Change |
|---|---|
| `:713` | **new private** `retire_mode_prefix(key)` — v3's `parse_retire_mode` body, moved verbatim. It still separates `UnknownRetireMode` (a third mode inside the namespace) from `MalformedKey` (not a `retire:` key at all), because those stay different faults for the drain. Private: "nothing outside this module may act on a prefix match" (`:700-703`). |
| `:746` | `parse_retire_mode` is now `parse_retire_key(key).map(\|(mode, _)\| mode)` — it answers a mode **only for a key that decodes whole, token included**. |
| `:828` | `parse_retire_key` calls `retire_mode_prefix` instead of `parse_retire_mode` (the dependency is inverted; there is no cycle). |

Behaviour delta, exhaustively: for every key `parse_retire_key` already accepted, the mode is
unchanged (proved by the round-trip leg, `multipart_keys.rs:733-772`, which asserts
`parse_retire_mode(key) == mode` for both modes × four token forms). For every key
`parse_retire_key` rejects, `parse_retire_mode` now rejects too, with the *same* error. No
third behaviour exists, by construction — which is the point of routing both answers through
one decode rather than keeping two code paths in step by convention.

`retire_mode_prefix` keeps the classification logic rather than folding it into
`parse_retire_key` because the `UnknownRetireMode` / `MalformedKey` split must happen *before*
the token is looked at: `retire:sideways:s:<id>:1` has a perfectly well-formed token and is
still not dispatchable, and reporting it as "malformed" would hide that this build met a
disposal rule it does not know.

## 3. The test changes

`crates/core/tests/multipart_keys.rs`:

- `:813` **new** `assert_retire_rejected(key)` — asserts **both** entry points refuse a key.
  Used on every row of the malformed table (`:802`, `:805`), so the property under test is not
  "these two bytes strings are rejected" but *"`parse_retire_mode` and `parse_retire_key`
  accept exactly the same keys"* — a rule over the whole grammar, which is what stops the two
  from drifting apart again as later children extend the token forms.
- `:799-800` two new table rows: `retire:bytes:` and `retire:records:` — a known mode with no
  token at all (the finding's first named case).
- `:804` the non-UTF-8 loop gains `non_utf8("retire:bytes:")` — `retire:bytes:\xff`, the
  finding's second named case (v3 only had `retire:bytes:s:\xff`, which the *key* parser
  already caught, so it could not see the hole).
- `:831` `a_retire_key_reports_an_unknown_mode_apart_from_a_foreign_key` renamed
  `…_apart_from_a_key_that_does_not_decode` and given four rows (`:847-850`):
  `retire:bytes:`, `retire:records:`, `retire:records:\xff`, `retire:bytes:s:`. This test
  asserts the **exact typed error** (`RecordError::MalformedKey { namespace: "retire:", key }`,
  payload included) from `parse_retire_mode`, so the fix is pinned to a specific error, not
  merely to "some error".

Both modes are covered on both new classes, deliberately: a fix that validated only the
`bytes` arm would be a half-fix, and `Records` is the arm where a wrong `Bytes` answer does the
damage.

## 4. Forced self-refutation (the three questions, answered with evidence)

**(a) Genuine red?** — Yes, three times, each run against the **final shipped test file** with
the production change reverted, then reverted back.

*Negation 0 — the iteration-3 defect restored* (`parse_retire_mode` → `retire_mode_prefix(key)`,
i.e. prefix dispatch):

```
---- a_retire_key_reports_an_unknown_mode_apart_from_a_key_that_does_not_decode stdout ----
thread '...' panicked at crates/core/tests/multipart_keys.rs:853:37:
not a retire: key: Bytes

---- retire_key_rejects_a_token_of_neither_form_and_every_malformed_spelling stdout ----
thread '...' panicked at crates/core/tests/multipart_keys.rs:81:5:
retire: mode: expected rejection for key "retire:bytes:x:0123456789abcdef0123456789abcdef:7", got Ok(Bytes)

test result: FAILED. 19 passed; 2 failed
```

`not a retire: key: Bytes` is literally the reviewers' finding: the parser handed back `Bytes`
for a key that names no obligation.

*Negation (a) — the brief's binding demo #1*: one fixed-width parser made to accept the
padded-but-short `007` spelling (`parse_slot_key`: `.or_else(|| fixed_width_u32(fields[1], 3))`).
Leg 1 goes red:

```
---- slot_key_round_trips_and_rejects_noncanonical stdout ----
thread '...' panicked at crates/core/tests/multipart_keys.rs:81:5:
slot: non-canonical body "007": expected rejection for key "slot:0123456789abcdef0123456789abcdef:007",
  got Ok((UploadId("0123456789abcdef0123456789abcdef"), SlotIndex(7)))

test result: FAILED. 20 passed; 1 failed
```

*Negation (b) — the brief's binding demo #2*: `MPU_PREFIX` spelled without its separator
(`b"mpu"`). Leg 3's named near-miss goes red:

```
---- scan_mpu_does_not_reach_the_mpuctl_singleton stdout ----
thread '...' panicked at crates/core/tests/multipart_keys.rs:633:5:
assertion failed: !MPUCTL_KEY.starts_with(MPU_PREFIX)

---- no_key_prefix_is_a_prefix_of_another stdout ----
thread '...' panicked at crates/core/tests/multipart_keys.rs:623:9:
the mpuctl singleton and "mpu" are not disjoint

test result: FAILED. 19 passed; 2 failed
```

All three negations were reverted; `diff` against the pre-negation copy confirms byte
identity, and `cargo test -p wyrd-core --test multipart_keys` is back to **21 passed; 0
failed**.

**(b) Production path?** — Yes. The test is an integration test in `crates/core/tests/`; it
`use`s `wyrd_core::multipart::{…}` (`multipart_keys.rs:25-34`) and calls the shipped public
functions. There is no stand-in, no mock, no re-implementation: negation 0 could not have gone
red if the test were driving a copy. The only helpers in the test build *inputs* (`hex_token`,
`non_utf8`) — no helper reimplements a key format or a parse; every expected key is either a
pinned literal (`multipart_keys.rs:296-326`) or the output of the production constructor.

**(c) Fixture includes the fault?** — Yes, and this is where v3 failed. The v3 rejection table
carried `non_utf8("retire:bytes:s:")` — a key whose *token form* is present, so the key parser
caught it and the mode hole stayed invisible. The new rows are the failing element itself:
`retire:bytes:` and `retire:bytes:\xff` are exactly the two keys the finding names, asserted
against exactly the function that mis-answered them. Nothing curates the fault out: the mode
assertion runs on **every** row of the malformed table, including the 14 pre-existing ones (one
of which — `retire:bytes:x:<id>:7` — is what negation 0 tripped on first).

## 5. Alternatives considered, with costs

1. **Validate a token *shape* inside `parse_retire_mode` instead of delegating** — e.g. require
   the remainder to be UTF-8 and to start `s:` or `g:`. Cost: **+8 lines** of a second,
   weaker grammar living beside the real one, and it still accepts
   `retire:bytes:s:<id>:007:4:<att>` (a non-canonical epoch) as `Ok(Bytes)` — the same class of
   hole, one layer in. Rejected: it re-creates two decode paths that must be kept in step by
   hand, which is what produced this finding.
2. **Delete `parse_retire_mode` and let callers use `parse_retire_key(..).0`** — smallest
   possible diff (−17 lines). Rejected: the brief pins the module's API and names
   `parse_retire_mode` in `Scope`; and 0016 wants a named "which disposal rule is this?" entry
   point because that question is asked in places (a drain's dispatch table) where discarding
   the token is the honest intent. The invariant to restore is *fail-closed decode*, not
   *fewest symbols* (`docs/principles.md` §1.2, §2: smallest change that restores the
   invariant, not smallest diff).
3. **Leave it and fix it in the consumer slices (#656–#659)** — 0 lines here. Rejected on the
   sign-off's own reasoning: this module is a durable format foundation whose whole job is to
   make an unvalidated value unrepresentable; pushing the check to five future call sites is
   the convention-at-call-sites pattern ADR-0045 exists to refuse, and it is five chances to
   forget.
4. **Also harden `parse_retire_key`'s `&text[mode.prefix().len()..]` slice** (`:830`) into
   `text.get(..)`. Cost: 0 net lines. **Not done**: the slice boundary is established five
   lines above by the prefix match on the same bytes, so `.get()` would add an unreachable
   `None` arm that no honest test can drive — and an untestable defensive branch is its own
   review finding. Recorded here so the human can overrule cheaply if they disagree.

## 6. Verification actually run (all in `$PDCA_WORKTREE`, `wyrd.pdca-wt-l0` @ `339da46`)

| What | Command | Result |
|---|---|---|
| Focused test (the brief's GREEN leg) | `cargo test -p wyrd-core --test multipart_keys` | **21 passed; 0 failed** |
| Whole project gate (the configured runner) | `./engine/xtask.sh ci` (→ `cargo xtask ci`) | **`xtask ci: all checks passed`**, exit 0 |
| Formatter (commit-hook parity) | `cargo fmt -p wyrd-core --check` | clean |
| Patch applies to base | `git apply --check` on a pristine `git archive origin/main` tree | applies cleanly; applied tree byte-identical to the worktree for all 3 files |
| Mutation (C5, advisory) | `scripts/mutants-in-diff` (`cargo mutants --in-diff patch.diff`) | see §8 |

Note on CI parity: this host **has** `typos`, the docs linter and the site renderer, so the
prose gates ran for real rather than warn-skipping (`ci.log:2-9`) — the brief's
`External dependencies` caveat did not bite. `cargo-deny`, `cargo-machete` and the conformance
vectors also ran. No external dependency was missing; **no NEEDS-HUMAN external-dependency
marker is warranted**.

C4-verify remains **UNVERIFIABLE (exit 77)**, exactly as the brief pre-declares: the base has
no `multipart_keys` target at all, so the RED leg is a compile failure, not a test failure.
That is the born-at-tier posture the brief declares (posture (a)) and it is a §6 sign-off item,
not a defect. The three negations in §4 are the substitute red the brief makes binding.

## 7. Things the human should know at sign-off

- **`review-rejected.md` anchors still resolve.** Every change is at `multipart.rs:690+`, so
  the recorded docs-currency rejections keyed to `multipart.rs:55`, `:468`, `:475`, `:481`
  still point at the same lines (`# Nothing here is written yet…`, `// 3. Keys and their
  canonical parsers`, `MPUCTL_KEY`, `MPU_PREFIX` — verified). If T4 re-raises that finding it
  will still be matched and dropped, not re-blocked.
- **Budget.** 1,144 added non-blank/non-comment lines across exactly **3 files** (module 457,
  test 686, `lib.rs` 1) against the brief's ≤1,150 / 3-file cap. Iteration 3 measured 1,130, so
  the fix cost 14 lines. The `assert_retire_rejected` helper exists partly for this: asserting
  both parsers inline on every row cost 10 extra lines after `rustfmt` exploded the calls.
- **Citation correction carried in this iteration.** v3 cited `docs/principles.md:109`/`:137`
  by line in one new doc-comment; the target repo has no `docs/principles.md` (it is a
  wyrd-pdca document) and the house convention in `metadata.rs:724`, `custodian/src/gc.rs:30`
  cites it by **section**. The new doc-comment uses `docs/principles.md` §5 C-1. Likewise the
  0016 sentence is now cited at its true lines `:438-440` (the paragraph is `:434-440`).
- **This is the second `iterate-do` at the §6 size-backstop threshold.** The previous sign-off
  left a standing instruction: *if this rebuild again returns fresh implementation-level parser
  defects elsewhere in the grammar, that is the signal the slicing — not the implementation —
  is the problem, and the next disposition should be `iterate-plan`.* For what it is worth as
  evidence toward that decision: before writing the fix I re-audited every parser in the module
  for the same fail-open shape (`split_key`'s `splitn` field counts, `fixed_width_u32`'s three
  refusals, `canonical_decimal`'s leading-zero rule, `is_token`'s byte-length check,
  `Digest::from_hex`'s 64-byte/lowercase check, and each of the seven `parse_*` entry points).
  `parse_retire_mode` was the only function in the module that returned a **decision** without
  completing a decode; every other public function is a total parse whose failure modes are
  already in the rejection tables. I cannot promise a fourth round finds nothing, but the
  fail-open class the last two rounds found is now empty by construction.

## 8. Mutation result (C5, advisory)

Run here via the gate's own script (`PDCA_BUNDLE=results/issue_691 scripts/mutants-in-diff`):

```
Found 134 mutants to test
ok       Unmutated baseline in 10s build + 1s test
134 mutants tested in 2m: 50 caught, 84 unviable
```

**0 survivors** (one more mutant than iteration 3's 133 — the extra is the new
`retire_mode_prefix`/`parse_retire_mode` split; it is unviable-or-caught, not missed). The
functions this iteration touched are covered by construction —

- `parse_retire_mode` → `Ok(Bytes)` / `Ok(Records)` mutants die on the round-trip leg
  (`multipart_keys.rs:757` asserts the mode for **both** modes) and on the malformed table
  (`:802`, which now asserts the mode function rejects).
- `retire_mode_prefix` → `Ok(Bytes)` / `Ok(Records)` and each `starts_with` flipped to
  `true`/`false` die on the same round-trip leg: a `records` key answered `Bytes` mis-slices
  its own token and fails to decode.

Iteration 3's run over the same file set reported 133 mutants, 50 caught, 83 unviable, **0
survivors** — unchanged by this iteration.

## 9. Scratch discipline

All throwaway work under `$PDCA_SCRATCH/pdca-builder-691-*` (negation backups, the CI log, the
`git archive` apply-check tree). The apply-check tree was removed immediately; the remaining
directory holds only text logs and is removed at hand-off. Nothing was written to `/tmp`.
