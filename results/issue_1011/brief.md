# core: Complete — refuse part sets past the value ceiling (636.5e)

> Fifth of the five slices of #658 (636.5). It builds on child-2 (636.5b). Do reads ONLY this
> file. Read `crates/core/src/metadata.rs:549` (`MAX_VALUE_BYTES`), the part-set range encoding
> (`metadata.rs:1976-1977`, `multipart.rs:3184-3193`) and the obligation payload
> (`multipart.rs:3619-3632`) before writing code. #842 found 0016's "stays inside one value"
> claim (`0016:382-384`) false for sparse part numbers. Citations were verified at `4bac538`;
> re-resolve them on your base.

- **Slug:** multipart-complete-value-ceiling
- **Track:** alpha
- **Kind:** enhancement (design proposal)
- **Defect:** Complete's flip writes each part set inside its own obligation value: the
  published parts' `retire:records:` and the unnamed parts' `retire:bytes:`. Part numbers can be
  spread past S3's range (core's format allows up to 999,999, `multipart.rs:939`). Then a set can
  encode past `MAX_VALUE_BYTES`, and the store refuses that flip for good (FoundationDB `2103
  value_too_large`). After child-2, such a Complete fences, is refused at the flip, releases and
  returns the stopped error (child-2's J), every time. The session can never publish, only be
  aborted. The `all` wildcard is reserved for teardown (`multipart.rs:3174-3177`).
- **Success criterion:** A NEW test file `crates/core/tests/multipart_complete_value_ceiling.rs`.
  It runs over a wrapper that refuses any value past `MAX_VALUE_BYTES` (as
  `crates/custodian/tests/restore_completing_fence.rs:473-486` does). Sessions are made by #894's
  Create, and parts are planted as canonical records.
  **(I1)** Parts 1 to 10,000, Complete naming the odd ones: it publishes, and both `{parts}`
  obligations fit (about 58.9 KB each).
  **(I2)** Parts 1, 3, …, 19,999 (10,000 parts), Complete naming all of them (about 129 KB
  encoded): Complete refuses `Refused(SessionTooLarge { bytes, limit })` with **zero commits**.
  The session stays `Open` with `attempts` unchanged, and an Abort then fences it.
  **(I3) The boundary is the stored value, not the bare set.**
  - Parts 1, 3, …, 15,869 (7,935 parts), Complete naming all: the set encodes to 99,981 bytes
    and the `{"parts":…}` value to 99,991, so it publishes. The installed obligation's stored
    length is at most `MAX_VALUE_BYTES`.
  - Parts 1, 3, …, 15,871 (7,936 parts): the set is 99,995 bytes but the value is 100,005, so
    Complete refuses `SessionTooLarge` with zero commits.
  
  If the real payload carries more than `parts`, move the pair so it still straddles the ceiling,
  and record the numbers in `build-notes.md`.
  **(I4) A part added after the first read.** A set fits when Complete first reads it. The
  wrapper plants one more unnamed part just before the fence commits, so the unnamed parts'
  `retire:bytes:` value crosses the ceiling. Complete fences, refuses `SessionTooLarge` against the
  frozen set, and releases (`Open@E+2`, `attempts` restored).
- **Falsifiability:** RED is reachable in-process, and C4-verify sees it. On the reverted base
  (child-2 folded in), Complete exists and the file compiles. I2, I3's second case and I4 then
  fail, because the flip is refused (child-2's stopped error) instead of answering
  `SessionTooLarge`.
- **Invariant to restore:**
  - every value the flip writes fits the store's value ceiling;
  - within S3's part-number range (1 to 10,000), no client error can leave an `Open` session
    unable to publish;
  - outside that range, Complete refuses before it writes anything.
  
  **Sources:** #842's finding against `0016:382-384`; `metadata.rs:549`; `0016:994-1000`.
  SELF-TEST: it binds the pre-fence and the frozen-set checks, so a guard on one alone fails I4
  or I2.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** 1008
- **Conflicts with:** 1010
- **Ordering note:** `Depends on child-2`: the obligations it measures are the ones child-2's flip
  installs, and the refusal releases through child-1's release. It runs in parallel with child-3
  (see child-3's note). `Conflicts with child-4`: both edit the code around the fence.
- **Scope:**
  - the encoded-value check, before fencing and again against the frozen set, through
    `RetireObligation`;
  - the `SessionTooLarge` doc (`multipart.rs:4515-4526`), extended to say Complete answers it;
  - the new test file.
  
  Out of scope:
  - any change to `UploadPart` or the part-number range;
  - any new `Refusal` variant;
  - other children's legs;
  - 0016 and ADRs, `Cargo.lock`, `deny*.toml`.
- **Repro instruction:** after child-2, run I2. Complete fences and returns the stopped error from
  a refused flip, again on every retry.
- **Difficulty:** medium
- **Do model:** opus
- **External dependencies:** `typos`
- **Test file:** `crates/core/tests/multipart_complete_value_ceiling.rs`, a **NEW** file.
- **Verification posture:** (c) red→green at C4-verify.
  **Demonstrated reds:**
  - the check removed → (I2) fails;
  - the check measuring the bare set → (I3) fails;
  - the check run only before the fence → (I4) fails.
- **Production reach:**
  - (a) in-process over a real store;
  - (b) unreachable from S3 once #669 bounds part numbers to 10,000;
  - (c) no production caller;
  - (d) no existing production path changes.
- **Citations expected:** Do cites `path:line` for every change: `MAX_VALUE_BYTES`
  (`metadata.rs:549`), the obligation payload (`multipart.rs:3619-3632`), `RetireObligation`
  (`:3913`), `SessionTooLarge` (`:4515-4526`).
- **Prior-art check (triage cycles):** #842 (the finding). The nearest landed peer is Create's
  refusal of a session that could not later be fenced (#894).
- **Disposition hint:** new-feature

## Open questions

- **For sign-off (carried from the parent, still undecided):** this rule (refuse at Complete,
  reusing `SessionTooLarge`) was chosen at the 2026-10-19 plan review, not by the human. The
  alternative is to refuse the out-of-range part number in core at `UploadPart` (#927's verb). If
  that alternative is chosen, only this child is re-planned.
- **For #669:** the S3 `UploadPart` verb should refuse a part number above 10,000
  (`InvalidArgument`).

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR MAY
happen during the cycle. The PR MUST NOT be marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 1): rebuilding for the implementation-level findings — **No test pins the zero-commit refusal for the unnamed set.** `crates/core/src/multipart/complete.rs:1140` checks both obligations before the fence, but every zero-commit leg (I2 at `crates/core/tests/multipart_complete_value_ceiling.rs:501`, I3-past at `:578`) overflows the *published* `retire:records:` set. The only leg that overflows the *unnamed* `retire:bytes:` set is I4 (`:628`), and that one overflows after the fence. I ran a mutant where the check before the fence measures only the published set (the check after the fence still measures both). All 8 tests still passed. Concrete failing case for that mutant: parts {2} ∪ {1, 3, …, 15,871}, a Complete naming only [2], no part planted. Correct code refuses `SessionTooLarge { bytes: 100_005 }` with zero commits. The mutant fences, refuses after the fence, and releases: 2 commits and the session ends at `Open@2`. That breaks the brief's invariant that a Complete outside the range "refuses before it writes anything". Fix: add that leg and assert `take_sent().is_empty()` and that the session bytes are unchanged, as I3-past does.. Deferred, not addressed here: 3 finding(s) that need human judgment, held in deferred-findings.json for the human at handover — not build work
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 2): rebuilding for the implementation-level findings — T4 Contribution — Confirm the affected-file prior-art audit across merged and closed/rejected work — the supplied repository has one synthetic base commit and no remotes, and the brief's issue references plus aggregate review log cannot establish that audit (reviewer-prior-art.log:5, brief.md:91, gate-logs/T4-batch-review.log:10).. Deferred, not addressed here: 2 finding(s) that need human judgment, held in deferred-findings.json for the human at handover — not build work
- Failing gate: C4 diff coverage: changed lines executed by the patch's tests (advisory) — diff coverage not measured — patch.diff does not apply on origin/main
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
