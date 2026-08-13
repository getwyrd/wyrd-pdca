- **Slug:** multipart-budget-admission
- **Defect:** the key grammar exists (#691, merged at `9dbcd72`) but no record
  **values** do, and there is no envelope to encode/decode any. This child lands `Budget`
  (the profile tuple of `0016:1463-1480` and its pure derivations `U_ref` and
  **`MAX_SESSIONS = min( ⌊W_ref/U_ref⌋ , SCAN_CAP/2 )`** — `0016:1470` is explicit that the
  `SCAN_CAP/2` term is **a clamp the implementation applies**, not an operator range check,
  and it is load-bearing: `W_ref` is sized from host RAM and `U_ref` from the caps, so a
  legal pairing (large `W_ref`, small parts) makes `⌊W_ref/U_ref⌋` exceed `SCAN_CAP` and
  break the reaper's `scan("mpu:")`. `SCAN_CAP` is a base constant —
  `crates/traits/src/lib.rs:286`, `1 << 20`. **Both terms are binding; a derivation that
  omits the clamp is wrong** (plan-advisory finding, 2026-08-09)), `AdmissionRecord` (`mpuctl`,
  `0016:333-527`), and the shared `encode_record`/`decode_record` envelope that later
  record children extend with their own arms. Salvage from
  `results/issue_692/iteration-v2/patch.diff` (record types and decoders, added-file
  lines ~846–1818, the `Budget`/`AdmissionRecord` portion), fixing the recorded defects
  rather than re-shipping the reviewed shape.
- **Success criterion:** `Budget` and `AdmissionRecord` round-trip `encode`/`decode`, and
  each of the following hand-authored torn values is rejected with a typed error
  (ADR-0045) — one named negation per leg demonstrated in `build-notes.md` (drop the
  single check, paste the failing output, revert):
  **(1a)** an `AdmissionRecord` whose stored `max_sessions` disagrees with what its own
  stored `profile` tuple derives (the derivation functions are implemented HERE);
  **(1f)** `Budget::new` enforces BOTH ends of every knob range in `0016:1463-1480` that a
  **FORMAT** constant can decide. **Decode validates against stable format maxima, NEVER
  against live deployment knobs** (`0016:390-402` — the normative statement of this
  boundary; a decoder that enforced the current knob would make a durable record
  unreadable the day an operator lowers it). Four bounds, each **independently** enforced
  and independently falsified (see Falsifiability — one negation per bound, because a
  single out-of-range value can violate several at once and stay red on a surviving guard):
  **(1f-i)** `max_part_chunks` satisfies the value-ceiling rule
  `max_chunkref_bytes × max_part_chunks ≤ MAX_VALUE_BYTES / 2` (computable on the base:
  `metadata.rs:327`);
  **(1f-ii)** `max_staged_chunks ≥ max_part_chunks` (the lower end — at least one maximal
  part must remain stageable, `0016:1472`);
  **(1f-iii)** `max_staged_chunks ≤ MAX_ROOT_SEGMENTS × MAX_SEG_CHUNKS_FORMAT_MAX`, the
  publishable ceiling. `MAX_ROOT_SEGMENTS` is on the base (`metadata.rs:322`);
  **`MAX_SEG_CHUNKS` is NOT** — it has no code definition, only a prose reference. **This
  slice therefore DEFINES `MAX_SEG_CHUNKS_FORMAT_MAX`** as a compile-time constant of the
  encoding, derived exactly as the value-ceiling rule above
  (`max_chunkref_bytes × N ≤ MAX_VALUE_BYTES / 2`), with a `const` assertion tying the two.
  That is the format maximum `0016:390-402` demands; the *deployment* value stays #508's;
  **(1f-iv)** `max_inflight_parts ≤ max_parts_per_session` (`0016:1476`, iteration-13
  finding 2 — a session cannot have more parts in flight than it may ever hold);
  **(1g)** the `sidx:` scan bound counts **committed staging entries as well as** in-flight
  chunks, so an accepted profile can never exceed `SCAN_CAP/2` (batch-review
  `multipart.rs:1074`; `SCAN_CAP` at `crates/traits/src/lib.rs:286`);
  **(leg 3, binding the other way)** a decoded `AdmissionRecord` whose `count` exceeds
  its `max_sessions` still **decodes** — occupancy above a lowered cap is a legitimate
  live state, not a decode error (`0016:390-402`; the same
  liberal-on-read boundary at `metadata.rs:312-321`); assert it decodes. Identity relations
  are binding; occupancy relations are not.
  **NOT enforced here — `B_ops`, and this is deliberate.** The batch review's
  `multipart.rs:1041` blocker names the value-size **and** operation-envelope bounds
  together, but `B_ops` is a **backend-calibrated deployment knob owned by #625**
  (`0016:1475`, `:1487`) with no value on this base — it is not a format constant, so by
  `0016:390-402` it does **not** belong at decode. It is enforced where new work is
  *admitted* (the slot reserve / part commit / Complete fence), which is #508's and #625's.
  Do MUST NOT invent a `B_ops` value to satisfy 1f. (Plan-advisory finding, 2026-08-09:
  the previous wording made an unresolvable calibration a decode-time acceptance threshold.)
- **Falsifiability:** RED is criterion-ABSENCE — born-at-tier, as #691. C4-verify
  classifies `ADDED_TEST crates/core/tests/multipart_budget_admission.rs` + `CRATE
  crates/core`; the GREEN leg is `cargo test -p wyrd-core --test
  multipart_budget_admission`; the RED leg reverts production and the test fails to
  **compile** → **UNVERIFIABLE (exit 77), EXPECTED and PRE-DECLARED** as a §6 item.
  **Demonstrated red Do MUST capture instead (binding):** **six** named negations, one per
  independently-enforced bound — **1a, 1f-i, 1f-ii, 1f-iii, 1f-iv, 1g** — drop that single
  check, run the test, paste the failing output into `build-notes.md`, revert. **Each
  negation must ISOLATE its bound:** the torn value it uses must violate *only* that bound,
  so the test's red proves that guard is load-bearing. A value that trips two ceilings at
  once stays red on the surviving guard and falsifies nothing — that collapse (two
  negations for four bounds) was the plan-advisory finding of 2026-08-09, and is why the
  count is now one-per-bound rather than one-per-leg. A leg green under its own isolating
  negation must be rewritten. Leg 3 is negated the other way: make the occupancy case
  reject and show the assert-it-decodes leg fail.
- **Invariant to restore:** sourced from the TARGET repo, which is the only tree Do can
  read: **ADR-0045 §"Parse-don't-validate at decode, for structural invariants"**
  (`docs/design/adr/0045-metadata-validation-boundaries.md:42-49`) plus the format-maxima
  boundary at `0016:390-402`. (The harness-side catalogue rule is **C-1**,
  `docs/principles.md:109` / §6 row `:137` in the *wyrd-pdca* repo — recorded for the
  human's audit trail only. **Do cannot open it**: the builder is grounded on
  `$PDCA_WORKTREE`, a wyrd checkout, where that path does not exist. Cite the ADR, not the
  catalogue. Plan-advisory finding, 2026-08-09.) Over this child's category: **a stored
  record's fields may not
  disagree with each other, and the disagreement must surface as an error, never as a
  value** (ADR-0045). An admission record whose `max_sessions` does not match its own
  `profile` admits sessions past the memory bound the reconcile pass is sized for — a
  fleet-wide failure admitted by one unvalidated field. A profile accepted above its
  settled range produces a maximal part whose commit and whose compensation both time out
  permanently, stranding the slot forever (`0016:1466`, the `B_ops` clamp).
- **Repo + branch target:** getwyrd/wyrd @ main
- **Surfaces:** data
- **Reproduction:** n/a — new functionality on a base (`9dbcd72`) where only the key
  grammar exists (`crates/core/src/multipart.rs` is 854 lines of keys and identity types).
- **Scope:** extend `crates/core/src/multipart.rs` with
  `Budget`, `AdmissionRecord`, their validating decoders, and the
  `encode_record`/`decode_record` envelope (arms for this child's records only). One new
  test file. Budget ≈ 550 added lines / 2 files. Cite `path:line` on `9dbcd72` for every
  change; sources Do MUST open: `0016:333-527`, `0016:1463-1480`, ADR-0045;
  `metadata.rs:312-321` and `:327-352` (the format maxima the ranges are computed
  against). / out of scope: every other record type (child-2, child-3); `metadata.rs`
  and any file outside `multipart.rs` + the new test; the knob **values** (#655); the
  outcome enums, answer table, `Verb`, `MultipartEtag`, digests, `sha2` (#693 — no
  `Cargo.toml`/`Cargo.lock` change); store round trips (#656–#659); all `docs/` files.
- **Budget:** ≤ 550 added semantic lines (module extension ≈ 300, test ≈ 250) across
  exactly **2** files: `crates/core/src/multipart.rs` and the new test. Well under the
  100 KB size backstop v2 breached.
- **External dependencies:** `typos`, `cargo-deny`, `cargo-machete`, `cargo-mutants` — all registered doctor ids; prose/dependency-wall legs warn-skip locally while CI enforces them (INTEGRATION §3). Nothing else beyond the base Rust toolchain: pure functions, no runtime, no Docker, no new crate.
- **Test file:** `crates/core/tests/multipart_budget_admission.rs` — a **NEW** file, not
  optional (C4-verify's added-`*/tests/*.rs` discriminator). Co-located unit tests may
  ship in addition.
- **Verification posture:** declared — **born-at-tier (posture (a))**, as #691: the
  UNVERIFIABLE RED is PRE-DECLARED here so C2/C4 land as a known sign-off item rather than
  a surprise NEEDS-HUMAN (it surfaced as one in BOTH archived attempts). Everything built
  is exercised at Check under the named test + gating C4-ci, and the four negation
  demonstrations in `build-notes.md` replace the flippable red.
- **Production reach:** N/A by design — `Budget`/`AdmissionRecord` have no live writer
  until #656–#659 wire the store round trips; nothing on an existing path changes.
- **Citations expected:** cite `path:line` on `9dbcd72` for every change. **Read the two
  citation namespaces apart — this is a trap:** a `crates/core/src/multipart.rs:NNNN`
  reference tagged *(batch-review …)* or *(v2 review …)* is relative to the **v2 PATCHED
  file** (~2,027 lines) preserved in `results/issue_692/iteration-v2/patch.diff`, NOT to
  the base — `multipart.rs` is **854 lines** on `9dbcd72`, so those line numbers do not
  exist there. Locate them in the archived patch, by symbol; every other `path:line` in
  this brief is base-relative and resolvable directly. Peer callsites
  Do MAY open: `crates/core/src/metadata.rs:312-321` and `:327-352` (`MAX_VALUE_BYTES` /
  `MAX_ROOT_VALUE_BYTES` / `MAX_ROOT_SEGMENTS` — the **format** maxima leg 1f's ranges are
  computed against, and the occupancy-not-at-decode boundary of leg 3). **Salvage:**
  ``$PDCA_HARNESS_ROOT/results/issue_692/iteration-v2/patch.diff` — the path is relative to the HARNESS repo (wyrd-pdca), NOT to `$PDCA_WORKTREE`; a claude builder's cwd is the harness root so it resolves as written, but a codex builder/escalation runs with cwd = the worktree and must resolve it absolutely` — take the `Budget`/`AdmissionRecord` types,
  wire structs and decoders, then FIX the recorded defects (legs 1f/1g were found missing
  or one-ended) rather than re-shipping the reviewed shape;
  `results/issue_692/review-batch.md` holds those blockers verbatim.
- **Prior-art check (triage cycles):** verified at Plan against `9dbcd72`: no `Budget`,
  `AdmissionRecord` or record codec exists on `origin/main` (`multipart.rs` is 854 lines
  of keys and identity types, ending at the retirement-key parsers); no open PR touches
  these paths. Closed/rejected: #654's two archived attempts and #692's own two
  (`iteration-v1/`, `iteration-v2/`) — the batch review's three budget blockers
  (`multipart.rs:1027/1041/1074`) are this child's binding legs 1f/1g, not suggestions.
- **Difficulty:** medium
- **Depends on:**
- **Conflicts with:**
- **Ordering note:** **Wave 0 — the chain's root; both ordering fields are DELIBERATELY
  EMPTY, not unset.** #691 (the key grammar this child's types are built from) is COMPLETE
  **and merged** — `d986069`, PR #703, in `origin/main @ 9dbcd72` — so it is deliberately
  NOT carried as a `Depends on`: the base already contains it, and under `auto_merge =
  false` `_runnable` gates every declared `Depends on` on `merged.is_merged`, so naming an
  already-merged prerequisite adds a liveness check that can only ever block this bundle,
  never help it. `Conflicts with` is empty because this child touches only
  `crates/core/src/multipart.rs` and its own new test file — nothing #710 or #711 reads —
  so it MAY share a wave with either (and does: the computed schedule puts #710 and #715
  together in wave 0). **Downstream:** #716 depends on this child, #717 on #716; #693 and
  #655 follow #717. Anything that later edits `multipart.rs` must declare against this
  chain rather than assume it.
- **Disposition hint:** new-feature

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 1): rebuilding for the implementation-level findings — C2 Reproduction (red pre-fix) — Accept the declared born-at-tier exception — the clean base exits 101 because the named test target is absent, so it supplies no behavioral red against which to judge the implementation.; C4 Verification (red→green) — Accept green without a behavioral base red — the 10-test green and all six one-check negations reproduce, but the pre-fix leg proves only test-target absence (`crates/core/tests/multipart_budget_admission.rs:7`).; C5 Causal adequacy — Rebuild with an encoding-derived ceiling — actual JSON for a legal-width 32 MiB RS(6,3) `ChunkRef` is 303 bytes, so 165 refs total 50,161 bytes before part fields despite the claimed 302-byte upper bound (`crates/core/src/multipart.rs:919`).; T4 Contribution — Confirm closed/rejected prior art by both affected paths — merged history and open-PR files were mechanically checked, but the archived #654/#692 attempt file lists are unavailable in the permitted artifacts, so duplication cannot be ruled out.; T5 Judgment — Rebuild tests with independent numeric oracles for `U_ref` and `MAX_SESSIONS` — 25/46 mutants survive, including every `U_ref` operator and both quotient alternatives, so wrong admission arithmetic remains green (`crates/core/src/multipart.rs:977`, `crates/core/src/multipart.rs:1181`).; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 9 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_715/review-b. 7 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 46 mutants tested in 2m: 25 missed, 8 caught, 13 unviable
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 9 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_715/review-b
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 2): rebuilding for the implementation-level findings — C2 Reproduction (red pre-fix) — Accept the declared born-at-tier exception—the clean `9dbcd72` checkout exits 101 because the named test target is absent, so it proves criterion absence rather than a behavioral red.; C5 Causal adequacy — Rebuild the causal bound around the worst-case encoded value—the minimum-width derivation answers whether some spelling fits, not whether every admitted maximal record fits, so it does not restore the value-ceiling invariant (`crates/core/src/multipart.rs:930`, `crates/core/src/multipart.rs:1135`).; T4 Contribution — Decide whether the unavailable internal #654/#692 archived attempts or the reported seven batch-review blockers duplicate unresolved work—the permitted artifacts omit both `scripts/review-branch` output and those archives; target merged history and all GitHub closed-unmerged PRs were checked by both affected paths and found no duplicate.; T5 Judgment — Rebuild the test with an independent widest-record oracle—the current assertions hard-code 1,063 from the production minimum and merely require the 303-byte realistic case to imply a smaller quotient, so the unsafe admission arithmetic stays green (`crates/core/tests/multipart_budget_admission.rs:180`, `crates/core/tests/multipart_budget_admission.rs:208`).; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 7 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_715/review-b. 8 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 7 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_715/review-b
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: The brief itself carries unresolved defects, not just implementation gaps: (1) two incompatible formulas stated for MAX_SESSIONS (one omits the SCAN_CAP/2 clamp the target spec requires); (2) an unresolved policy prerequisite (B_ops) declared out of scope with no value source, blocking full enforcement of criterion 1f; (3) a referenced constant (MAX_SEG_CHUNKS) that has no code definition, only prose, forcing an ad hoc format-maximum invention; (4) a phantom citation (docs/principles.md:109/:137) that doesn't exist at the target — replace with ADR-0045's actual boundary text; (5) unverifiable tracker/prior-attempt claims (notes.json, sources/, salvage artifacts absent); (6) an incomplete falsifiability mapping — one of four required independently-enforced bounds in leg 1f is not independently tested. On top of this, T2 Shape is a flat FAIL: the required extensible codec envelope (per-record-type decode arms) was never built — the decoder is generic/private and the encoder accepts any Serialize. T4 batched review also fails gating (3 blocking findings). Size backstop is at threshold (2 rounds spent, patch already 11 semantic lines over the ≤550 cap) reinforcing that this slice needs re-authoring rather than another build attempt. Re-plan should resolve the formula contradiction, source or descope B_ops/MAX_SEG_CHUNKS, fix the citation, and re-derive a complete falsifiability plan for leg 1f before re-attempting Do; consider whether this splits along the B_ops/MAX_SEG_CHUNKS dependency boundary (#625/#508).
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_715/review-b
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
