# Adversarial review — #842 restore fence for `Completing` sessions

Verdict: I could not break the production fence. Two test gaps let real regressions through
(bullets 1–2), and two gate rows claim more than they proved (bullets 3–4). All work below was
done in a scratch copy of `$PDCA_TARGET`, with the patch applied as found there.

- NEEDS-HUMAN [impl] — `crates/custodian/src/restore.rs:1491` (`emit_segments_unaccounted`) is
  pinned by no test. Running cargo-mutants on the diff with `--cap-lints=true`, the mutant
  `replace emit_segments_unaccounted with ()` **survives** the whole wyrd-custodian suite. A
  second mutant survives too: `restore.rs:266` (`Display for SegmentFault` → empty string). Why
  it matters: the CLI names only 20 sessions, then says "and N more (the audit log names every
  one)" (`crates/server/src/cli.rs:1402-1416`, `named_records` at `:1463-1474`). The blueprint
  sends operators to `action=session-segments-unaccounted` too. Failing case under the mutant:
  21 fenced `Completing` sessions, each holding one undecodable `seg:` record. The 21st is then
  named nowhere: not on stdout, and not in the audit log. Under the second mutant, the
  "and why: …" text prints blank. Fix: the test file already captures the audit seam (`AUDIT`
  and `audited()`, `crates/custodian/tests/restore_completing_fence.rs:336-371`), but only for
  `dangling` and `summary`. In leg H or K, assert that each named case emits a
  `session-segments-unaccounted` event carrying its `session`, its `record` and a non-empty
  `fault`. (The subscriber is installed once per binary, by the Order test, so the new leg must
  install it too, idempotently.)

- NEEDS-HUMAN [impl] — every fixture sets the session's `segment_nonce` equal to its upload id
  (`restore_completing_fence.rs:182-196`; `group()` at `:219`). So no test can tell the two
  apart. Concrete survivor, which I ran: change `restore.rs:911` from
  `SegmentGroup::from_nonce(record.segment_nonce().clone(), attempt)` to
  `SegmentGroup::new(upload.as_str(), attempt).expect(..)`. All 7 tests in
  `restore_completing_fence.rs` and all 9 in `restore_open_fence.rs` still pass. Under that
  mutant, a fenced session whose nonce differs from its id (`77…` vs `a7…`) is named
  `NotOfAttempt` on **every** re-run, because its own correct `{seg:(77…,3)}` obligation no
  longer matches. Its real `seg:77…:3:` range is never read, so a junk `seg:77…:3:000001` goes
  unnamed. That is a false alarm on every clean fence, and it hides the real fault (leg K). The
  production code is correct today: my probe with a distinct nonce passes on the patch as built.
  Fix: seed K's clean control and at least one damaged H/K case with a nonce that is not the
  upload id.

- NEEDS-HUMAN [human] — the C5 row ("36 mutants tested: 8 caught, 28 unviable", 0 missed) is
  not evidence of adequacy. The workspace sets `warnings = "deny"` (`Cargo.toml:230`), and
  `.cargo/mutants.toml` sets no `cap_lints`. So any mutant that stubs a body and leaves a
  parameter unused fails to compile, and is counted "unviable". Re-running the same 36 mutants
  with `--cap-lints=true` gives **25 caught, 2 missed, 9 unviable**; the 2 missed are the ones in
  bullet 1. Whether the gate (or `.cargo/mutants.toml`) should cap lints is a harness/repo
  decision outside this diff. The two survivors themselves are bullet 1's [impl] work.

- NEEDS-HUMAN [human] — leg (L), "`cargo xtask ci` green", is unverified, yet `check-gates.json`
  reports `overall: "pass"`. The gating C4-ci row is `unverifiable`: it hit the 7200 s timeout
  inside `crates/server/tests/custodian_day_one.rs`, after fmt, clippy, build and most tests
  had passed. It never ran machete, deny, conformance, statics, the orchestrator guard or DST.
  The hang is not this patch: here, with the patch applied, `custodian_day_one` passes 15/15 in
  0.19 s. What I re-ran myself, all green: `cargo xtask statics`;
  `cargo clippy -p wyrd-dst --all-targets` under `--cfg madsim`; and the DST `restore*` tests.
  Still not run: machete, deny, conformance, the orchestrator guard, the full DST seed sweep,
  and the server tests after `custodian_day_one`. Provisional (toolchain/time), not a
  refutation.

- Attacks I tried that did not land:
  - **Red→green.** It is real and runs the production `reconcile_after_restore`. With the
    production files reverted and the test kept, 7/7 fail by assertion. With the fix, 7/7 pass.
  - **One commit.** All three writes go in one batch (`restore.rs:807-840`). A split batch
    would fail G-atomic.
  - **`require_absent`.** It is on both keys. G-collision covers the records key and the bytes
    key separately. Its "key taken" cause names whichever key is taken.
  - **Value ceiling.** The written payloads are minimal, `{"session":true,"parts":"all"}` and
    `{"seg":{"nonce":…,"epoch":3}}`, with no default fields emitted. G-sparse would refuse
    `{session, parts}`.
  - **Last epoch.** `Completing@u64::MAX` takes the guard with no wrap.
  - **Paging.** Both the `seg:` and `part:` reads page correctly. The "paged" case and G-sparse
    each kill a stop-after-page-one mutant.
  - **The re-check.** It handles a foreign, undecodable, parts-only or absent obligation.
  - **Designed flows.** I traced rollback (lands `Open@E+1`), the Open-fence `Aborting`
    sessions and the reaper/operator rows: none of them gives a false `NoDeleter` or
    `NotOfAttempt`.
  - **Unknown commit outcome.** `CommitUnknownResult` is never read as a `Conflict`
    (`:859-863`).
