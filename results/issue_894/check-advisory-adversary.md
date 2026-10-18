# Adversarial review — issue 894, iteration 5 (multipart-admission-create)

**Verdict: I could not refute the fix.** This round's job was two new tests and no production change (the iteration-4 carry-forward). Both tests go red under the exact edit each was written for, and every extra edit I made to the recovery logic was caught by some test. I have no NEEDS-HUMAN items to file.

## What I re-ran (scratch copy of `$PDCA_TARGET`, removed afterwards)

- **Baseline:** `cargo test --offline -p wyrd-core --test multipart_create` passed, 43 of 43, the same as `gate-logs/C4-diff-cov.log`. I did not re-run the madsim suite. `gate-logs/C4-ci.log:4117-4128` shows all 7 `multipart_admission` tests green, including both `should_panic` reds and the witness leg. `xtask ci: all checks passed` is at `:4174`.
- **Iteration-4 test (1), the nonce check in `same_session`:** I replaced `&& *segment_nonce == other.segment_nonce` (`crates/core/src/multipart.rs:2456`) with `true`. Only `c6_iv_this_creates_request_under_another_nonce_is_still_a_collision` fails, with an `InDoubt` where `Created` was expected (`crates/core/tests/multipart_create.rs:334` panic via `:2170`). The fixture really does isolate the nonce. The rival is built through the same `request()` helper (same `CREATED_AT`, `CLOCK`, no content type), and `:2193-2203` checks that every other field `same_session` compares is equal. So the test cannot pass because some other field happens to differ.
- **Iteration-4 test (2), bucket checked before ledger:** I moved the bucket block (`crates/core/src/multipart/create.rs:407-415`) after the ledger block (`:417-444`). Only `b_a_missing_bucket_is_judged_before_a_full_ledger` fails (`multipart_create.rs:1255`, `SessionCap` where `NoSuchBucket` was expected).

## Extra edits I made to try to break it (cargo-mutants does not generate these)

All were caught. Each line gives the edit and the test that failed.

- `create.rs:381`: removed the line that clears the unsettled flag after a loop read finds some other record. Caught by `f9_a_1021_settled_absent_leaves_nothing_in_doubt`.
- `create.rs:633`: the settling read no longer clears the unsettled flag when it finds some other record. Caught by `f9_a_1021_settled_absent_leaves_nothing_in_doubt`.
- `create.rs:389`: while held, the branch for an identity taken by another record now reports our own moved-on session as `IdentityTaken`. Caught by `f10_its_own_session_moved_on_while_held_is_not_an_identity_taken`.
- `create.rs:389-390`: same idea, with `Moved`, `Other` and `Err` all collapsed into `IdentityTaken`. Caught by the same test.
- `create.rs:376`: the in-loop check for our own moved-on session now runs only when held, not when merely unsettled. Caught by `f10_its_own_session_fenced_before_the_re_read_ends_in_doubt_never_in_a_re_mint`.
- `create.rs:630-632`: when the settling read finds our session moved on, the error now carries the ending as its cause instead of `SessionMoved`. Caught by `f10_its_own_session_moved_on_is_kept_by_the_settling_read`.
- `create.rs:627`: the settling read now runs only when held (an unsettled 1021 is skipped). Caught by `f9_a_1021_settled_absent_leaves_nothing_in_doubt` and `f10_its_own_session_moved_on_is_kept_by_the_settling_read`.
- `create.rs:403`: the "lost" flag is no longer reset when a new identity is minted. Caught by `c2_…`, `c3_…`, and the new `c6_iv_…` (its `waits.is_empty()` assertion at `:2175`).

## Refutation attempts that failed (no finding)

- **Zero contention budget or a bad bootstrap ledger** (`create.rs:352`, `:426`). Ruled out. G7 (`multipart.rs:1775`) forces `w_ref ≥ U_ref`, so `max_sessions() ≥ 1`. That means `admission_cas_attempts()` (`:1711`) is at least 1, and `bootstrap`'s `count: 1` is never above its own limit.
- **A new identity minted while a batch may still land** (`create.rs:383-405`). The only re-mint path is guarded by `if let Some(held)` at `:384`. Any unsettled 1021 has already been cleared at `:381`, by a read of `mpu:<id>` that found neither R nor our own moved-on session. So an out-of-flight batch from an earlier identity cannot land unreported.
- **The termination claim** (`create.rs:320-323`, "at most `max_sessions + MAX_UPLOAD_ID_ATTEMPTS` commits"). Holds. Every commit that does not admit is followed by a return, a counted loss (`:448-450`, capped at the budget), or a counted collision (`:395-396`, capped at 2).
- **A wrong error class when in doubt** (`create.rs:207`). `classify` (`crates/traits/src/lib.rs:786-804`) stops at the first seam type in the chain, and `InDoubt`'s source is the `CommitUnknownResult` itself. A store fault, `TransientFault` or `IntegrityFault` that stopped the create therefore sits under `cause` and never takes over the class. That matches F9's "whatever stopped the create". It does mean an `IntegrityFault` hit while in doubt reads as `Indeterminate`, which is how the brief specified it.
- **Size headroom measured by a copy of production** (`multipart.rs:2388`). It isn't. The test writes its own JSON for each widest state (`multipart_create.rs:1474-1519`), decodes each through the production decoder, and pushes each through a `Ceiling` store that enforces `MAX_VALUE_BYTES` (`:1455-1465`). So the measurement is checked independently, not compared against a mirror of itself.
- **Scope:** no production caller was added. `create_session` is referenced only in `crates/core/src/multipart{.rs,/create.rs}`, `crates/core/tests/multipart_create.rs` and `crates/dst/tests/multipart_admission.rs`, as Production reach (c) requires.
- **Not raised, per the rubric:** a bucket marker deleted and re-created with identical bytes (an ABA case) is the deletion-fence gap marked `// deferred: #511` at `create.rs:701`, so it is settled. The cancel-safety caveat (`create.rs:328-335`) is the paragraph the iteration-2 sign-off asked for.

## Limits of this pass (not refutations)

- I can't confirm from my inputs that this round changed **no production code** compared with iteration 4. `iteration-v4/patch.diff` is not among my inputs. The current production code is what I attacked, and it holds either way.
- C4-verify is `unverifiable` (exit 77, `gate-logs/C4-verify.log`) because the new test calls an API that doesn't exist on the base. The brief (Verification posture (a)) declared this in advance, and it is not a defect. The red side of the evidence is the brief's table of demonstrated reds plus the mutation runs above.

<!-- pdca:leaf-complete -->
