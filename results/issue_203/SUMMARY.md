# Result — issue 203 / fschunkstore-unique-temp-per-write

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect / goal: `FsChunkStore::put_fragment` writes to a temp file whose name is keyed on
- Success criterion: N concurrent `put_fragment` calls for the **same** `FragmentId`
- Repo + branch target: getwyrd/wyrd @ main   (resolve here at Plan — do not leave to Do)
- Scope (one logical fix) / out of scope: make `put_fragment`'s temp scratch private per write so concurrent same-id

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it.
- C5 Causal adequacy: none — reviewer + human sign-off

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 Contribution: none — (no gate configured)
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

# Check review — issue 203 / `fschunkstore-unique-temp-per-write`

Advisory, artifact-only, decorrelated from the builder. Inputs: `patch.diff`,
`brief.md`, `check-gates.json` (build-notes.md withheld by design). Citations
re-derived against the read-only target at `$PDCA_TARGET =
/home/eddie/wyrd/wyrd` (bare `main`, pre-patch) and against `patch.diff`.

## Verdict table

| Item | Verdict | Basis |
|------|---------|-------|
| C1 — C1 Spec | PASS | `brief.md:25-34` carries a concrete, testable Success criterion (N concurrent same-id puts all `Ok`, final `.frag` verifies, `list_fragments` ignores temps); root cause grounded — shared `{index}.tmp` at target `crates/chunkstore-fs/src/lib.rs:45-49`, write-then-rename at `:81-83`. |
| C2 — C2 Reproduction (red pre-fix) | PASS | Net-new `tests/concurrent_put.rs` races `WRITERS=64 × ROUNDS=16` same-id puts (`patch.diff` test body); pre-fix mechanism real (one shared `{:05}.tmp` for the id, target `lib.rs:45-49`, racing `fs::rename` → `NotFound` at `:83`); `C4-verify` oracle reports "red without the fix" (`check-gates.json:46`). Red is interleaving-dependent (acknowledged `brief.md:80-88`) but amplified + backstopped by the structural unit test. |
| C3 — C3 Change | PASS | Patch adds per-store `scratch_seq: AtomicU64`, unique `scratch_file_name(index, seq)` → `{index:05}.{seq}.tmp`, own-scratch cleanup on write/rename failure, and `reap_stale_temps()` at `open` — matches Scope (`brief.md:63-71`); atomic rename remains the sole publish point. |
| C4 — C4 Verification (red→green) | PASS | `check-gates.json`: `overall: pass`; gating `C4-ci` PASS ("xtask ci: all checks passed", `:36`) — clears the iteration-1 §6.1 madsim-DST red; `C4-verify` PASS ("red without the fix, green with it", `:46`). Per-store counter (no `std::process::id()`, no process-global static) means the rejected approach was not re-run unchanged. |
| C5 — C5 Causal adequacy | NEEDS-HUMAN | Mechanism is adequate for the brief's *in-process* scope (`Arc<store>`/`from_arc`): `fetch_add` hands each concurrent writer a distinct seq → distinct scratch → atomic rename sole publish. BUT the justification for (a) dropping cross-process disambiguation and (b) reap-at-open safety rests on *"ADR-0034, Model A — one D server per disk"* cited 3× in `patch.diff` comments — **ADR-0034 does not exist** in the target (highest is `docs/design/adr/0033-…`; the actual FsChunkStore layout ADR is `0032`, which makes no one-owner-per-root claim, `adr/0032-…:44-92`). Ungrounded load-bearing assumption + always-human item → human must confirm the one-owner-per-root invariant (or scope multi-process-same-root out). |
| T1 — T1 Structure | PASS | Test placed at the brief-specified net-new path `crates/chunkstore-fs/tests/concurrent_put.rs` (`brief.md:77`); a structural unit test (`scratch_names_are_unique_per_seq_and_invisible_to_listing`) is also added in-crate (`patch.diff`). |
| T2 — T2 Shape | PASS | Asserts the three Success-criterion clauses: every concurrent put `Ok`, `get_fragment` returns the byte-complete verifying fragment, `list_fragments == vec![id]` (scratch invisible) — maps to `brief.md:25-29`. |
| T3 — T3 Runtime | PASS | Every API used exists on target: `FragmentHeader::new_v1(u128,u64)` (`chunk-format/src/header.rs:130`), `ec_fragment_index` (`header.rs:119`), `encode` (`codec.rs:32`); `pollster` + `tempfile` are dev-deps and `bytes`/`wyrd-chunk-format` deps (`chunkstore-fs/Cargo.toml`); compiles — corroborated by `C4-ci` green. |
| T4 — T4 Contribution | PASS | Non-tautological: 64×16 real concurrent writers exercise the write→rename race (reds pre-fix per `C4-verify`), and the structural unit test pins seq-uniqueness independent of interleaving — together cover both behavioural and structural halves. |
| T5 — T5 Judgment | NEEDS-HUMAN | Oracle is "reviewer + human sign-off" (`check-gates.json:98`). The behavioural red is timing-dependent (`brief.md:80-88`); whether 64×16 reliably reds on the CI host — vs. effectively resting the regression on the structural unit test — is the judgment the brief explicitly defers to sign-off. |
| V — Validation — fitness-to-purpose | NEEDS-HUMAN | Always-human; oracle "human at sign-off" (`check-gates.json:107`). Does private-scratch + atomic-rename, per-store counter, and reap-at-open actually fit the real concurrent duplicate/repair-write purpose across the deployed profiles (single-binary `from_arc`, networked D server, NAS)? Human call. |

## §6 — items the human must clear

**§6.1 (C5) — Ungrounded `ADR-0034` citation; one-owner-per-root is load-bearing.**
`patch.diff` justifies three design choices on *"ADR-0034, Model A — one D
server per disk"*: (i) the per-store `scratch_seq` (vs. the iteration-1
`std::process::id()` scheme) is "sufficient" because "one D server owns its
root"; (ii) `reap_stale_temps()` at `open` "cannot race a live put's scratch"
because no other opener exists. **ADR-0034 is absent from the target** (`ls
docs/design/adr` tops out at `0033-fragment-durability-via-redundancy.md`; the
FsChunkStore layout ADR is `0032`, which records the layout but asserts no
single-owner/exclusive-open invariant — `adr/0032-…:44-92`). Two consequences
the human should weigh:
- If two processes ever open the *same* root concurrently, both `scratch_seq`
  counters start at 0 and can mint the **same** scratch name (`00007.0.tmp`) —
  the exact cross-process collision the dropped pid component prevented. The
  brief scopes the defect to in-process concurrency (`brief.md:16-22`), so this
  is plausibly out of scope — but that scoping decision should be **explicit**,
  not implied by a non-existent ADR.
- `reap_stale_temps` deleting another live opener's in-flight scratch has the
  same dependency. Confirm the one-owner-per-root invariant is real and locate
  its actual authority, then fix the comments to cite it (CLAUDE.md: citations
  must be grounded on the target; "Do must cite path:line", `brief.md:89`).

**§6.2 (T5) — Regression rests partly on a timing red.** The brief flags the
pre-fix red as intermittent on a fast/quiet machine (`brief.md:80-88`) and
sanctions resting on the structural post-fix invariant if a reliable timing red
is impractical. The structural unit test is present, but a human should confirm
the bundle's regression posture is acceptable (does the 64×16 behavioural test
reliably red pre-fix on CI, or is the structural test the real guard?).

**§6.3 (V) — Validation fitness-to-purpose.** Always-human: confirm the
private-scratch + atomic-rename + per-store-counter + reap-at-open design fits
the real-world duplicate/repair concurrent-write purpose across the deployed
profiles before sign-off.

### Advisory (non-gating) note
`is_temp_scratch_name` matches **any** `*.tmp` by suffix (`patch.diff`), and
`reap_stale_temps` runs an `fs::read_dir` traversal at every `open`. Both are the
iteration-1 §6.2/§6.3 advisories. They appear benign given the new per-store
design (scratch is invisible to `list_fragments`, target `lib.rs:99-136`/`221-223`),
and CI is now green — but they share the same one-owner-per-root assumption as
§6.1; resolving §6.1 settles them.


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] C5 — C5 Causal adequacy — Mechanism is adequate for the brief's *in-process* scope (`Arc<store>`/`from_arc`): `fetch_add` hands each concurrent writer a distinct seq → distinct scratch → atomic rename sole publish. BUT the justification for (a) dropping cross-process disambiguation and (b) reap-at-open safety rests on *"ADR-0034, Model A — one D server per disk"* cited 3× in `patch.diff` comments — **ADR-0034 does not exist** in the target (highest is `docs/design/adr/0033-…`; the actual FsChunkStore layout ADR is `0032`, which makes no one-owner-per-root claim, `adr/0032-…:44-92`). Ungrounded load-bearing assumption + always-human item → human must confirm the one-owner-per-root invariant (or scope multi-process-same-root out).
- [x] T5 — T5 Judgment — Oracle is "reviewer + human sign-off" (`check-gates.json:98`). The behavioural red is timing-dependent (`brief.md:80-88`); whether 64×16 reliably reds on the CI host — vs. effectively resting the regression on the structural unit test — is the judgment the brief explicitly defers to sign-off.
- [x] V — Validation — fitness-to-purpose — Always-human; oracle "human at sign-off" (`check-gates.json:107`). Does private-scratch + atomic-rename, per-store counter, and reap-at-open actually fit the real concurrent duplicate/repair-write purpose across the deployed profiles (single-binary `from_arc`, networked D server, NAS)? Human call.

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: merged-wider
- Iteration delta (if iterating):
- By / date: Eduard Ralph / 2026-06-23

## 10. Act candidates (hints for the next Act review)
- (empty is the common case)
- issue_203: patch comments should cite the *exclusive-open-per-root* invariant (holds under ADR-0034 Model A and Model B) rather than "Model A" specifically — comment-precision nit, not correctness.
