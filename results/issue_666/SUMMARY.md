# Result — issue 666 / multipart-gateway-seam

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: A `MultipartGateway` companion trait in `crates/gateway-core`, mirroring
  `ContainerGateway` (`crates/gateway-core/src/lib.rs:389-431`). It carries the six multipart
  verbs in neutral container/object vocabulary. `wyrd-server`'s `Gateway` implements it at the
  composition root, beside its existing `ContainerGateway` impl
  (`crates/server/src/lib.rs:683-747`), and the S3 crate requires it of its gateway type, so the
  four later slices only fill in bodies. ADR-0046 decision 6 and ADR-0010 keep `ObjectGateway`
  object-only and bucket-free, so a new concern arrives as a narrow companion trait. Today there
  is no multipart seam at all; the S3 crate refuses every multipart form `501` through its
  subresource denylist (`crates/gateway-s3/src/lib.rs:410-413`).
- Success criterion: A NEW test shows legs (1)–(3), and the full gate shows leg (4).
  **(1) The composition root answers every verb, typed.** Calling each of the six
  `MultipartGateway` methods on a real `wyrd_server::Gateway` (over `RedbMetadataStore::in_memory`,
  as the peer suites build it) returns the seam's typed "not implemented" answer. That is a
  value the caller matches on, not a panic, not `todo!()`, and not an untyped boxed error.
  **(2) The real `Gateway` satisfies the S3 crate's bound.** A generic helper bounded on
  `ObjectGateway + ContainerGateway + MultipartGateway` accepts the real `Gateway`, and
  `S3Gateway::new` over it compiles. This leg proves only that the real `Gateway` meets the
  triple bound. It does **not** prove the S3 crate *requires* `MultipartGateway`: today's
  `S3Gateway::new` (`crates/gateway-s3/src/lib.rs:227-232`) already accepts a gateway that
  happens to implement it. That property is assigned to **source review at Check**: the diff
  must show `MultipartGateway` added to `S3Gateway::new`'s `where` clause and to every `G:` bound
  listed under `Citations expected`. Rust has no stable negative trait test, and the workspace
  has no compile-fail harness (no `trybuild`, no `compile_fail` doctest), so this slice does not
  add one.
  **(3) The wire is unchanged, and the seam is not entered.** Over the in-process listener, with
  `S3Gateway` serving a **sentinel gateway** whose six `MultipartGateway` methods each record
  that they were called (a counter held in a struct field, not a `static`), every multipart form
  still answers `501 NotImplemented` naming its subresource: `POST /b/k?uploads`,
  `PUT /b/k?partNumber=1&uploadId=u`, `POST /b/k?uploadId=u`, `DELETE /b/k?uploadId=u`,
  `GET /b/k?uploadId=u`, `GET /b?uploads`. After all six, the sentinel's counter is **zero**.
  The counter is what catches a request that enters the seam and then maps the "not
  implemented" answer to the same `501`, which the status alone cannot. The sentinel's
  `ObjectGateway` methods may simply answer an error, as `FaultyGateway` does
  (`crates/server/tests/request_capacity_planes.rs:189-218`); none is reached.
  **(4) The full gate is green.** `cargo xtask ci` (the C4-ci row) passes. That is the tracker's
  acceptance (`notes.json`: "`cargo xtask ci` green") and the repo's Rust gate (`AGENTS.md:41-54`).
  It is the only command that builds the S3 crate's own `#[cfg(test)]` doubles
  (`crates/gateway-s3/src/lib.rs:3516-3517` onward, and the helper bound at `:5141`), which
  `cargo test -p wyrd-server` never compiles. Any step the gate skips for a missing local tool
  (it warn-skips `typos`, `cargo-machete`) is listed as skipped in `build-notes.md`, never
  reported as green.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: the multipart companion trait and its neutral vocabulary in `gateway-core`, the
  composition-root implementation in `wyrd-server` answering "not implemented" for every verb,
  the S3 crate's bound on it, and every existing implementer and test double updated to satisfy
  that bound / out of scope: any routing change (the denylist at
  `crates/gateway-s3/src/lib.rs:410-413` and both guards stay exactly as they are; that is #667);
  any S3 XML, status or error-code mapping for these answers (#668–#670); any call into
  `wyrd_core::multipart` (#656–#659 provide those functions; #668 wires them); a `wyrd-core`
  dependency for `gateway-core` or `gateway-s3`; the PutObject sizing of #671.

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — N/A — close disposition (no patch to verify)
- C3 Change: none — patch.diff
- C4 Verification (red→green): none — N/A — close disposition (no patch to verify)
- C5 Causal adequacy: none — reviewer + human sign-off

## 4. Conformance (Check — stack)
- T1 Structure: none — N/A — close disposition (no patch to verify)
- T2 Shape: none — N/A — close disposition (no patch to verify)
- T3 Runtime: none — N/A — close disposition (no patch to verify)
- T4 Contribution: none — N/A — close disposition (no patch to verify)
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

# Advisory review — SKIPPED (builder-declared external dependency confirmed, #341)

The builder declared an unmet external dependency in build-notes.md and the claim was CONFIRMED deterministically (the named [[doctor.checks]] row's detect cmd exited non-zero), so the Check beat was not spent adjudicating a patch already stated to be unverifiable. Gates are recorded N/A; no reviewer or adversary ran. The bundle is resumable.

- NEEDS-HUMAN — External dependency `wyrd-base-advisories` confirmed absent (detect cmd `bash -c '. engine/lib/ensure-cargo.sh && ensure_cargo && cd ../wyrd && cargo deny --all-features --config deny-all-features.toml check advisories'` exited 1). Install hint: a RustSec advisory has landed against the base Cargo.lock: on main, waive it in deny-all-features.toml (the #543 tikv-client exposure boundary) or bump the crate, then iterate-do. Provide it and answer iterate-do to resume the full Do+Check band, or discontinue at sign-off.


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] External dependency `wyrd-base-advisories` confirmed absent (detect cmd `bash -c '. engine/lib/ensure-cargo.sh && ensure_cargo && cd ../wyrd && cargo deny --all-features --config deny-all-features.toml check advisories'` exited 1). Install hint: a RustSec advisory has landed against the base Cargo.lock: on main, waive it in deny-all-features.toml (the #543 tikv-client exposure boundary) or bump the crate, then iterate-do. Provide it and answer iterate-do to resume the full Do+Check band, or discontinue at sign-off.
- [x] external dependency: wyrd-base-advisories — `cargo xtask ci` fails on base 7bf7eb9 at its all-features `cargo deny … check advisories` step (RUSTSEC-2026-0346 and RUSTSEC-2026-0349 against rustls 0.21.12, reached only through the optional tikv-client 0.4.0), so leg (4) "the full gate is green" cannot be produced by this patch without a security-waiver or dependency change on main; every other gate step passes on the patched tree
- [x] **The supposedly fixed contract cannot express all failures its inputs require.** `brief.md:114-124` accepts `ContentHash`, raw upload IDs and `u32` part numbers, assigning validation to the implementer; `brief.md:128-139` reserves `Err` for store failures and fixes the answer vocabulary to core outcomes plus NotImplemented. On the target, `PartNumber::new(0)` returns `RecordError::PartNumberOutOfRange` (`crates/core/src/multipart.rs:938-942`), while the mirrored `Refusal` has no input-range or payload-integrity variant (`crates/core/src/multipart.rs:4302-4345`). The existing streaming implementation returns `GatewayError::PayloadMismatch` for a failed ContentHash check (`crates/server/src/lib.rs:392-396`); this is not a store failure. Define neutral validation/integrity answers, or explicitly permit typed non-store errors, before promising later slices only replace bodies (`brief.md:19-20`). Actual validation and wire mapping can remain in those later slices.
- [x] **The proposed test does not prove two properties claimed for it.** The positive helper in `brief.md:29-31` still compiles if S3 never adds the MultipartGateway bound: the current constructor already accepts any ObjectGateway + ContainerGateway (`crates/gateway-s3/src/lib.rs:227-232`), including a real Gateway that also implements the new trait. Likewise, contrary to `brief.md:36-39`, a request could call the new NotImplemented stub and then emit exactly the existing subresource 501 (`crates/gateway-s3/src/lib.rs:1777-1783`), leaving leg (3) green despite entering the seam. Add a compile-fail case for a gateway lacking MultipartGateway and a no-call sentinel for routing, or explicitly assign both properties to source review instead of claiming these positive tests establish them.
- [x] **The tracker’s full-gate acceptance requirement is missing from the brief.** `notes.json:1` explicitly requires “`cargo xtask ci` green”; `brief.md:24-40` promises all existing suites stay green but names only `cargo test -p wyrd-server`. That command does not compile/run dependency crates’ unit-test modules, including the S3 doubles this change must update (`crates/gateway-s3/src/lib.rs:3516-3517`, `:5141-5153`). It also omits the broader gate required by target `AGENTS.md:41-54`. Restore `cargo xtask ci` as an explicit acceptance command alongside the focused seam test, with skipped external-tool checks recorded rather than counted as green.
- [x] **Leg (4) "full gate green" is not met, but this patch did not cause the failure.** C4-ci failed in both attempts at `cargo deny --all-features --config deny-all-features.toml check advisories` (`gate-logs/C4-ci.log:3472-3550`). The failures are RUSTSEC-2026-0346 and RUSTSEC-2026-0349 against `rustls 0.21.12` via `tikv-client 0.4.0` → `wyrd-metadata-tikv`. The patch does not touch `Cargo.lock` or any manifest. `run_ci` stops at the first failure (`xtask/src/main.rs:1683-1687`), so conformance, statics, deploy-guard and DST **never ran** on this patch. I ran the skipped steps myself on the patched tree:
- [x] external dependency: RustSec advisory database (RUSTSEC-2026-0346, RUSTSEC-2026-0349 against rustls 0.21.12 via the optional tikv-client 0.4.0) — `cargo xtask ci` fails at its all-features `cargo deny ... check advisories` step on base 7bf7eb9, so leg (4) "the full gate is green" cannot be produced by this patch; every other gate step passes on the patched tree
- [x] **Leg 4 ("`cargo xtask ci` green") is not met, and the patch is not the cause.** `gate-logs/C4-ci.log:3476-3554` fails `cargo deny --all-features --config deny-all-features.toml check advisories` on RUSTSEC-2026-0346 and RUSTSEC-2026-0349 in `rustls 0.21.12`, which comes in via `tikv-client 0.4.0` → `wyrd-metadata-tikv` (`Cargo.lock:302`). The diff does not touch `Cargo.lock`. `run_ci` stops at the first failure (`xtask/src/main.rs:1683-1687`), so the frozen run never reached conformance, statics, deploy-guard or the DST step. I ran the two that could see this diff, using a scratch target dir: `cargo xtask statics` ("no DST-reachable shared mutable global state (ADR-0035)") and `cargo xtask conformance` ("5 valid + 6 invalid vectors pass"). Both pass. `wyrd-dst` depends on none of the touched crates (`crates/dst/Cargo.toml`), and the diff adds no orchestrator import, so the skipped tail cannot be broken by it. The human needs to decide: waive leg 4 for this slice, or land the rustls/tikv advisory fix first as a separate issue.
- [x] **Check the T4 "pass" before trusting it.** `check-gates.json` row `T4-batch-review` reports "0 blocking" from a "3x codex" review that took 8.78 s (`gate-logs/T4-batch-review.log`). Last round the same gate had 4 blocking findings. Three model passes over a 1,485-line diff in under 9 seconds is not plausible. The log has no per-pass output showing the passes actually ran on this diff, as opposed to a cache hit or a pass that failed quietly. My own review found no blocking defect, so this is about whether the gating row can be trusted, not about the code. Confirm the passes ran before counting the row.
- [x] external dependency: RustSec advisory database (RUSTSEC-2026-0346, RUSTSEC-2026-0349 against rustls 0.21.12 via the optional tikv-client 0.4.0) — `cargo xtask ci` fails at its all-features `cargo deny ... check advisories` step on base 7bf7eb9 (reproduced on a clean export), so leg (4) "the full gate is green" cannot be produced by this patch without a security-waiver change that belongs in its own PR on main; every other gate step passes on the patched tree (see "Leg (4)")

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
- By / date: Eduard Ralph / 2026-10-14

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
- Follow-up (human, issue_666 sign-off): tikv-client 0.4.0 is abandoned upstream and keeps drawing rustls-0.21 advisories (RUSTSEC-2026-0346/0349, waived under #543) — we may have to write/own our own TiKV client; file an issue tied to #435.
- Waiver (human, issue_666 sign-off): waive RUSTSEC-2026-0346 and RUSTSEC-2026-0349 (rustls 0.21.12 via tikv-client 0.4.0) in deny-all-features.toml under the #543 boundary — blocks `cargo xtask ci` on main for every bundle; draft entries in the issue_666 sign-off scratchpad.
- Gate (human, issue_666 sign-off): `cargo xtask ci` stops at the first failing step (`run_ci`, xtask/src/main.rs:1670-1687), so an unrelated base failure (cargo deny) hid conformance/statics/orchestrator-guard/DST from this patch — every check must run regardless of earlier failures, then report all failures together.
