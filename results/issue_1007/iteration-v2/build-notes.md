# Build notes — #1007 (636.5a), iteration 2: the Complete fence, its release, and a budget client errors never spend

Base: `getwyrd/wyrd` `main` at `376b106` (the worktree's HEAD). `patch.diff` applies to it. Line numbers below are on the **patched** tree unless marked "base".

I started from `iteration-v1/patch.diff`, because the carry-forward cites its line numbers and tells me to rebuild it rather than start over. That is the one prior-cycle file I built on. I also read the v1 Check findings the carry-forward summarises, to get the exact survivors and mutants.

## What the sign-off asked for, and where it is

| # | Item | Where |
|---|---|---|
| 1 | A store fault behind the fence tries **one** release | `complete.rs:656-664` calls `release` with `ReleaseFor::Fault`; `ReleaseFor` is at `:810`, its one-commit bound at `:823`. Module doc rewritten at `:59-75`. |
| 1 | Release lands → an error saying the fence was taken down | New `FenceState::Released` (`:327`). The release record is the one minted with the fence, so `attempts` goes back to the pre-fence value. |
| 1 | Release fails → today's `Stands` | `:887`, and `:878` for a `Conflict` with the session unchanged. |
| 1 | Release unknown → `InDoubt (Release)` | `:882-885`, same code as a rejection's release. |
| 1 | Conflict → the existing `moved()` path | `:873-875`, `moved` at `:898`. |
| 2 | Nonzero budget: D2, H, held fence at 1 and `MAX_COMPLETE_ATTEMPTS - 1` | `SPENT` in the test; `a_rejected_request_restores_the_attempts_it_read` (`:1525`), `a_bucket_deleted_behind_the_fence_…` (`:1548`), `a_held_fence_charges_one_more_…` (`:1571`). |
| 3 | D1 and H's first leg: zero commits, byte-identical session | Inside `d_typos_…` (`:932`) and `h_a_bucket_deleted_…` (`:1150`). |
| 4 | Paging: 130 parts, a tail part changed before the fence; scripted `scan_page` | `:1602`, `:1633`, `:1683`. The wrapper gained `Arm::NextPages` and a log of every scan. |
| 5 | Wrapper can fail a `psum:` `scan_page` after the fence commits; every release result asserted; `Stands` never `NotPlaced` | `OnCommit::Arm` / `Arm::FailScans` (`:284`). Tests `:1811`–`:2001`. |
| 6 | A release `Conflict` then a failed re-read must not claim "still stands" | New `FenceState::Unknown` (`:341`), produced at `:868-871`; tests `:1498`, `:2001`. |
| 7 | Keep the ordering error from `complete_fingerprint` | `fingerprint` is kept as a `Result` (`:551`); `answer_without_fence` (`:672`) asks the state first, then maps the error in `order_refusal` (`:697`). `out_of_order` and its `windows(2)` scan are gone. |
| 8 | Remaining C5 survivors | **70 mutants: 46 caught, 0 missed, 24 unviable.** See below. |

Other changes from v1: `docs/design/architecture/05-building-block-view.md:248` gained the fault policy and the unknown case (docs currency). `multipart.rs:2626` (`complete_fence`), `:2655` (`fence_released`) and `abort.rs:48-60` are unchanged from v1. `multipart.rs` header lines `:10-13` and `:110-114` and `:159` name the new module, as in v1.

## Decisions the sign-off left to me

**Added `Released`, did not reuse `NotPlaced`.** The sign-off allowed either. `NotPlaced` says "no fence of this complete landed", which is false after a fence landed and came down, and child-4 would act on it. The brief's stopped arm already says child-2 needs a "released" case, so this adds it now, with a producer. Cost: one enum variant and one `Display` arm, 8 lines.

**Added `Unknown`, did not reword `Stands`.** The sign-off allowed either. The stopped arm exists to say whether the fence stands, and a reworded `Stands` would still have the type say "it stands" when nothing knows. Cost: one variant and one `Display` arm, 12 lines.

**A fault's stopped error keeps the validation fault as its cause, even when the release also failed.** The fault is what stopped the request; the release was cleanup. The release's own failure is dropped in that case. The alternative is a second cause field on `Stopped`, which would be read by nothing yet. R18 below proves the tests bind this.

**A `Conflict` with the session moved back to `Open`, on the fault path, is an error: `Gone` with the fault as cause.** The rejection path answers its refusal there, but a fault has no refusal to answer. Re-validating against the new `Open` session would need a fence of its own, which is not what the sign-off asked for. The error says this Complete did not take the fence down.

**A torn `psum:` record behind the fence takes the same path as a read fault.** It is the same `Err(cause)` arm the sign-off pointed at (old `:607-611`), and a torn record minted no segment either. Same cost and same outcome table. If the human wanted only store faults released, it is a one-line match on `Stop` there.

**`order_refusal` has one branch no test can reach.** `complete_fingerprint` can also return `NoPartsNamed`, but `NamedParts::new` refuses an empty list (`complete.rs:138`), so that branch and the `PartNumber::new(..)?` beside it cannot fire. I return the error rather than panic: it surfaces as `Stop::Record`, which is the ADR-0045 shape. I used a `let … else` so there is no `_ =>` arm. Rejected alternative: a helper in `multipart.rs` returning the offending `PartNumber`, used by both `canonical_named_parts` and this file. It removes the unreachable branch, but it rewrites a merged function (about 20 lines of `multipart.rs`) to save two lines here.

## Carried over unchanged from v1

The pre-fence check is an optimization only; the frozen set is the `psum:` range, not `part:`; the release is minted with the fence, so `Stop::EpochExhausted` fires before anything is written; `FENCE_ROUNDS = RELEASE_ROUNDS = ABORT_ROUNDS`; a request for another target answers `NoSuchUpload`; a foreign clock is `Stopped { NotPlaced, ForeignClock }` with zero commits (no new clock read, the stamp belongs to the session's residency clock, module doc `:77-84`). DST is marked `// deferred: #660` (`:480`).

## Demonstrated reds

Each is a temporary edit to `complete.rs` or `multipart.rs`, applied by script to the final file, run through `cargo test -p wyrd-core --test multipart_complete --no-fail-fast`, then restored and checked with `cmp` (both files came back identical). R1–R6 are the brief's required list; R7–R20 are the carry-forward's.

| # | Temporary edit | Leg that went red, and how |
|---|---|---|
| R1 | Rejected request counted against `attempts` (no pre-check, no restore) | **D1**: `d_typos_…` at `:975`; H at `:1163` "nothing is fenced"; 8 tests in all |
| R2 | Pre-check kept, `attempts` not restored after the fence | **D2**: `d_typos_…`, "the attempt is given back" (left 1, right 0); 8 tests |
| R3 | Session left `Completing` after a rejection | **D's Abort leg**: `d_after_a_rejected_request_an_abort_fences`; 11 tests |
| R4 | A request meeting another publisher's `Completing` releases it | **(E)**: `e_another_publishers_…` at `:869` "zero commits"; 3 tests |
| R5a | A release that fails definitely is answered with its refusal | **D4**: "expected the stopped error, got Ok(Answered(…))"; 3 tests |
| R5b | The same, reported as in doubt | **D4**: "expected the stopped error, got Err(InDoubt(…))"; 3 tests |
| R6 | An unknown initial fence treated as success | **G0-fence**: "expected the in-doubt error, never an answer: got Ok(Held(…))" |
| R7 | `fence_released(0)` | `a_rejected_request_…`, H away from zero, fault-lands: "the attempt is given back" (left 0, right 1) |
| R8 | Fence charges a constant 1 | Same three plus the held-fence test: "the fence spends one attempt" (left 1, right 2) |
| R9 | `attempts_before` always 0 | `a_held_fence_charges_one_more_…` at `:1580` |
| R10 | A typo fences | `d_typos_…` at `:981` "typo 0: zero commits"; H's first leg; the paging refusal |
| R11 | `read_part_set` stops after the first page | The three paging tests |
| R12 | Walk continues from the wrong cursor | Two paging tests: the walk now breaks the ascending check |
| R13 | No release on a fault (the old policy) | 9 tests, including every `a_fault_behind_the_fence_…` |
| R14 | A failed release reported as `NotPlaced` | `…says_the_fence_stands…` at `:1872` (left `NotPlaced`, right `Stands`); D4 |
| R15 | A landed fault release reported as `NotPlaced` | `…takes_the_fence_down…` at `:1824`; 4 tests |
| R16 | A fault's release retries like a rejection's | `…tries_one_release_only` at `:1907` (5 commits, not 2) |
| R17 | A failed re-read claims `Stands` | Both re-read tests (left `Stands`, right `Unknown`) |
| R18 | A fault's error reports the release's failure | `…says_the_fence_stands…` at `:651`, plus two more |
| R19 | List order not checked before the fence | `d_typos_…`: "expected a refusal, got …" |
| R20 | List order checked before the state | `the_state_answers_before_the_order_of_the_list` at `:2078` |

**Reverted base.** With the fix removed the test target does not compile (`unresolved import wyrd_core::multipart::complete`), so C4-verify will report **UNVERIFIABLE (exit 77)**, as the brief pre-declares. The reds above are the evidence that each leg binds.

## Mutation run

`PDCA_WORKTREE=… PDCA_BUNDLE=… scripts/mutants-in-diff`, the C5 gate's own script, over this `patch.diff`: **70 mutants tested in 7m: 46 caught, 24 unviable, 0 missed.** The four v1 survivors (`attempts_before`, `DoubtfulCommit::fmt`, `:717 >`, `:718 ||`) are all caught now. All 24 unviable mutants try to build a `Default` of a type that has none (`SessionRecord`, `FenceOutcome`, `Stop` and so on), so none of them could compile with any test suite. No survivors to justify.

## How I ran the checks

- Test: `timeout … cargo test -p wyrd-core --test multipart_complete`, the target the brief's Falsifiability names, under a timeout. **35 passed.** The gate runner has no single-test mode.
- `cargo fmt --all -- --check`, `cargo clippy -p wyrd-core --tests` (workspace lints, `warnings = deny`), `typos` on every changed file, `python3 docs/publishing/tools/lint_docs.py`: all clean.
- `cargo doc -p wyrd-core --no-deps` reports errors, none in `complete.rs` or on lines this patch adds. They are on the base (for example `multipart.rs:5107`, `max_part_bytes`).
- `./engine/xtask.sh ci` (the full project gate, DST included) on the final tree: exit 0, `xtask ci: all checks passed`.
- No git hooks are installed and there is no pre-commit config, so commit-time checks are the ones above.

## Refuting my own test

- **(a) Genuine red?** Yes. Fully reverted, the test cannot build. With each defect put back one at a time, the named leg fails at the assertion that names the property (R1–R20).
- **(b) Production path?** Yes. Every leg calls the production `fence_complete` over `RedbMetadataStore::in_memory()`, through a wrapper that only delegates, withholds or scripts a store's answers. Sessions come from the production `create_session`; the Abort leg uses the production `abort_session`; records are checked with the production decoders. Scripted `scan_page` answers stand in for a backend breaking its contract, the one thing redb will not do on request. They feed the production `read_part_set`, not a copy of it.
- **(c) Fixture includes the fault?** Yes. Faults are armed at the commit they target, against a fence that really landed in redb: a failing `psum:` page and bucket read behind the fence, a real re-planted tail part on page two, a real deleted bucket, a really moved or torn session record, refused or unknown release commits, and sessions planted with attempts already spent.

## For the human at sign-off

**Size.** `patch.diff` is 154 KB, up from 112 KB, so the size backstop will fire again. You told me not to split at 112 KB. The growth is almost all tests you asked for: the test file went from 1,297 to 2,160 lines, and `complete.rs` from 858 to 951.

**Open question from the brief (visibility).** Unchanged: `fence_complete`, `HeldFence` and the error types are `pub` so the test can drive them; `HeldFence` cannot be cloned or built outside this function. child-2 may narrow them.

## Not done here (by the brief's scope)

Assembly and publication (#1008, #1009); the release's `retire:records:{seg}` obligation and the charging rule for fences that wrote segments (#1009); re-reads that settle an unknown outcome (#1010); the value ceiling (#1011); the reaper (#625), drain (#659), S3 verb (#670), DST (#660, marked in code). No edit to 0016, an ADR, `Cargo.lock` or `deny*.toml`.
