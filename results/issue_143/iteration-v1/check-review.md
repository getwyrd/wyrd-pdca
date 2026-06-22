# Check review — issue 143 / m3.5-scrub-custodian

Advisory, artifact-only, decorrelated from the builder (build-notes.md withheld).
Grounded against the target source on `main` at `$PDCA_TARGET`
(`/home/eddie/wyrd/wyrd`, pre-patch — `crates/core/src/repair.rs` and the scrub
loop do not yet exist there) and against `patch.diff`. Every basis re-derived;
build-notes were not consulted.

## Verdict table

| Item | Verdict | Basis |
| --- | --- | --- |
| C1 — C1 Spec | PASS | `brief.md` is a well-formed plan pointer: four binding success legs (`brief.md:33-51`), a category-level invariant (`brief.md:52-60`), and an explicit in/out-of-scope (`brief.md:69-78`). Acceptance is re-derivable without the host artifact. |
| C2 — C2 Reproduction (red pre-fix) | PASS | No own gate (`check-gates.json:14-22`), but the per-fix red→green is evidenced by `C4-verify` PASS — "red without the fix, green with it" (`check-gates.json:42-49`); the two load-bearing legs ship documented flippable negations (`patch.diff` scrub.rs:18-22 negate `fragment_intact`; read_repair.rs:10-13 drop the enqueue loop). Net-new caveat noted in `brief.md:84-91`. |
| C3 — C3 Change | PASS | Patch realizes all four legs and stays in scope: new shared queue `crates/core/src/repair.rs` (patch.diff:145-233), read-path enqueue in `read_object` (patch.diff:124-141), scrub loop `crates/custodian/src/scrub.rs` (patch.diff:603-732) dispatched through the fenced `reconcile_step` (extends `reconciliation.rs:57-71` with a 5th `scrub` param; all callers updated). Produces obligations only — never dequeues/deletes (scrub.rs comment + no `delete_fragment` call). |
| C4 — C4 Verification (red→green) | PASS | Both gates green: `C4-ci` (fmt/clippy/build/test/deny/conformance) PASS (`check-gates.json:33-40`) and `C4-verify` red→green PASS (`check-gates.json:42-49`). APIs the patch leans on all verified on `main`: `reconcile_step/4` (`reconciliation.rs:57-62`), `GcContext{meta,fleet}` mirror (`gc.rs:77-84`), `referenced_fragments` (`gc.rs:179-201`), `chunk-format` `decode/encode/CORE_HEADER_LEN/header.chunk_id` (`crates/chunk-format/src/codec.rs:18-71,148`, `header.rs:11,121,130`). |
| C5 — C5 Causal adequacy | NEEDS-HUMAN | Always-human (oracle: reviewer + human sign-off, `check-gates.json:50-58`). Re-derived concerns for the human: (a) `read_object_from` computes corruption findings then **drops** them (`patch.diff` read.rs:40-46) — production is unaffected because the server reads via `read_path → read_object` which enqueues (`crates/server/src/lib.rs:147`, `read.rs:180`), but the `pub` no-meta entry is a latent silent-absorption footgun for any direct caller; (b) the EC "any-k-first" read enqueues only if the corrupt fragment is polled before `k` good shards complete — deterministic in the in-memory test but order-dependent under real concurrency (scrub is the backstop). Does the invariant, stated over the category, accept these? |
| T1 — T1 Structure | PASS | Tests land where the brief directs (`brief.md:79-83`): scrub legs in `crates/custodian/tests/scrub.rs` (patch.diff:782-1114), the read-path leg in `crates/core/tests/read_repair.rs` (patch.diff:234-494) because the enqueue seam lives in `core`. Modelled on the existing `gc.rs` in-memory-store harness. |
| T2 — T2 Shape | PASS | Assertions pin the binding behaviour, not incidentals: scrub detects/excludes/enqueues + does **not** delete and skips unreferenced orphans (patch.diff:1011-1061), telemetry surfaces read back (patch.diff:1100-1113); read path excludes-and-enqueues on the **same** queue keyed by the shared `repair::repair_key` (patch.diff:434-443), and an unrecoverable single-fragment read still enqueues (patch.diff:484-493). |
| T3 — T3 Runtime | PASS | `C4-ci` PASS includes the test target, so both new test files compile and run green (`check-gates.json:33-40`); the gate exercised them through the real `reconcile_step` fenced control point, not a test-only entry. |
| T4 — T4 Contribution | PASS | Each load-bearing test is shown contributory by `C4-verify` red→green (`check-gates.json:42-49`) and the documented flippable negations (scrub `fragment_intact`; read-path enqueue loop). Net-new structural slice, so non-load-bearing legs rest partly on criterion-absence as `brief.md:84-91` anticipates. |
| T5 — T5 Judgment | NEEDS-HUMAN | Always-human (oracle: reviewer + human sign-off, `check-gates.json:96-103`). Whether the test set is the right judgment of the category invariant — e.g. no coverage of a corrupt fragment whose checksum passes but whose `header.chunk_id` is misplaced (`fragment_intact` checks both, patch.diff:203-205) — is a human call. |
| V — Validation — fitness-to-purpose | NEEDS-HUMAN | Always-human (oracle: human at sign-off, `check-gates.json:104-112`). Whether this slice, as built, is fit for the milestone's durability purpose is the validator's judgment. |

## §6 — items the human must clear

1. **C5 (causal adequacy).** Confirm the invariant ("a checksum-failing fragment is
   never absorbed silently") tolerates: (a) `read_object_from`'s `pub` no-meta entry
   silently dropping findings (production routes around it today, but it is an exported
   footgun), and (b) the EC read's enqueue being contingent on poll order under real
   concurrency, with scrub as the backstop.
2. **T5 (judgment).** Confirm the test set adequately judges the category invariant,
   including the misplaced-but-intact-checksum fragment path that `fragment_intact`
   guards but no test exercises.
3. **V (validation).** Fitness-to-purpose sign-off for the slice as a whole.
