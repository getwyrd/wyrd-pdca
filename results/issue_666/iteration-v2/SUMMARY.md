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
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): fail — xtask: `cargo deny --all-features --config deny-all-features.toml check advisories` failed with exit status: 1
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: unverifiable —                why this slice has no isolable red (the cargo output is above).
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 98.7% — 152 of 154 instrumentable changed lines executed (floor 80%); 154 of 489 changed lines were instru
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 16 mutants tested in 72s: 10 missed, 6 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_666/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 15.59s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Issue #666 adds a typed multipart gateway seam while preserving the six multipart 501 responses; no implementation defect found, with CI disposition and human acceptance still outstanding.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The seam-only boundary and four acceptance legs are explicit and testable; routing and persistence remain later slices (`brief.md:19`, `brief.md:77`). |
| C2 Reproduction (red pre-fix) | NEEDS-HUMAN | Accept the declared compile-only baseline for this new API — stash/test/restore reproduces E0432 before assertions execute; a separate injected wrong refusal does fail the behavioral assertion (`brief.md:90`; `pdca-reviewer-666-evidence/red.log:7`; `pdca-reviewer-666-evidence/behavioral-red.log:280`). |
| C3 Change | PASS | The patch stays within the companion seam, composition root, required callers, regression tests and living architecture; all six S3 bounds require the trait and routing remains unchanged (`crates/gateway-s3/src/lib.rs:229`; `crates/server/src/lib.rs:756`; `docs/design/architecture/05-building-block-view.md:137`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Resolve the existing TiKV advisory-gate disposition and obtain the required full CI green — the seven seam tests pass, but independent scanning reproduces the frozen all-features failure in unchanged dependencies; this is not a patch-caused defect (`pdca-reviewer-666-evidence/restored-green.log:19`; `pdca-reviewer-666-evidence/deny-rerun.log:10`; `gate-logs/C4-ci.log:6557`). |
| C5 Causal adequacy | PASS | The assertions detect wrong production refusals, forbidden seam entry, empty Complete lists and torn framing; all ten frozen accessor survivors are caught when the server integration suite is selected, and no capability probe masks a load-time cause (`crates/server/tests/multipart_gateway_seam.rs:129`, `crates/server/tests/multipart_gateway_seam.rs:398`, `crates/server/tests/multipart_gateway_seam.rs:690`; `pdca-reviewer-666-evidence/mutants.log:5`). |
| T1 Structure | PASS | The new concern preserves object-only dependency direction and runtime-neutral streaming without introducing a concrete-core dependency or shared mutable global (`crates/gateway-core/src/lib.rs:679`, `crates/gateway-core/src/lib.rs:711`, `crates/gateway-core/src/lib.rs:743`; `crates/server/tests/multipart_gateway_seam.rs:259`). |
| T2 Shape | PASS | Exhaustive typed refusals cover the agreed contract; the non-empty Complete input closes the previous empty-list hole, and listings retain the data required by later wire slices (`crates/gateway-core/src/lib.rs:488`, `crates/gateway-core/src/lib.rs:552`, `crates/gateway-core/src/lib.rs:646`). |
| T3 Runtime | PASS | Both real-gateway and sentinel listener tests preserve all six 501 responses with zero seam calls; exchanges are bounded and the serving task aborts on drop (`crates/server/tests/multipart_gateway_seam.rs:216`, `crates/server/tests/multipart_gateway_seam.rs:243`, `crates/server/tests/multipart_gateway_seam.rs:388`, `crates/server/tests/multipart_gateway_seam.rs:486`; `pdca-reviewer-666-evidence/restored-green.log:15`). |
| T4 Contribution | N/A | Commit/PR descriptions are absent by design; the substantive contribution audit must rerun at publish, so this deferred row is not a missing-evidence finding (`gate-logs/T4-contribution.log:10`). |
| T5 Judgment | PASS | The narrow slice respects the recorded rejection of monolithic multipart work; merged history for every affected path and all 19 closed unmerged PRs were checked, with no competing multipart seam found (`pdca-reviewer-666-evidence/prior-art-summary.log:1`; `pdca-reviewer-666-evidence/prior-art-rejected-context.log:4`; `brief.md:77`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Confirm that landing only the typed seam is useful for the planned follow-on slices — requests intentionally remain 501, so these passing checks do not establish usable multipart uploads (`brief.md:96`; `docs/design/architecture/05-building-block-view.md:137`). |

Source citations resolve under the supplied `$PDCA_TARGET`; brief and evidence citations resolve in this review directory. The disposable target matches `patch.diff` (`git apply --reverse --check` succeeds). Stash/pop restored every reviewed file byte-for-byte (`pdca-reviewer-666-evidence/red-restore.log:19`). No implementation changes were retained.

The prior implementation concerns are discharged. `NamedParts` makes an empty list unrepresentable at the seam, and its test checks both order-preserving accessors. The wire oracle now rejects 18 malformed framing variants before checking XML, including short fixed bodies and missing chunk terminators. The restored seven-test run passes. Changing the real server's Create refusal to `NoSuchUpload` in a separate scratch copy fails the expected equality assertion; this proves behavioral sensitivity without mislabelling the pre-fix compilation failure as an executed regression.

The constructor bound was also compiled independently. A generic caller with only `ObjectGateway + ContainerGateway` fails E0277 at `S3Gateway::new`; adding `MultipartGateway` compiles (`pdca-reviewer-666-evidence/bound-probe.log:4`, `pdca-reviewer-666-evidence/bound-probe.log:31`). This discharges the brief's source-review-only limitation for that acceptance leg.

The CI caveat belongs to the existing dependency gate. The independent `cargo xtask ci` run passed typos, documentation lint/render and link audit, repository guards, formatting, clippy, build, workspace tests and cargo-machete, then stopped at the sandbox's read-only advisory lock (`pdca-reviewer-666-evidence/ci.log:5`, `pdca-reviewer-666-evidence/ci.log:3268`). Re-running cargo-deny with only its database path relocated into this sandbox passed the default policy and reproduced RUSTSEC-2026-0346 and RUSTSEC-2026-0349 through the unchanged TiKV/rustls tree (`pdca-reviewer-666-evidence/deny-rerun.log:5`, `pdca-reviewer-666-evidence/deny-rerun.log:16`). No policy exception was added. All 27 dependency manifests, lockfile and advisory-policy inputs are byte-identical to the pre-fix base (`pdca-reviewer-666-evidence/dependency-baseline.log:28`). The existing policy assigns this optional-backend decision to #543/#443 (`deny-all-features.toml:72`); it is outside this seam patch.

Frozen evidence was read for each instance-scoped wrapper. C4-verify records seven passing tests and an unexecuted baseline (exit 77; `gate-logs/C4-verify.log:10`). Diff coverage reports 152/154 instrumentable changed lines (98.7%), with 335 changed lines unscored; this is not whole-patch coverage (`gate-logs/C4-diff-cov.log:144`). The frozen mutation run reports ten misses and six unviable mutations; the independent run using `--test-package wyrd-server --cargo-test-arg=--test=multipart_gateway_seam` catches all ten and retains six unviable (`gate-logs/C5-mutants.log:13`; `pdca-reviewer-666-evidence/mutants.log:5`). The batch-review log reports zero blocking findings but supplies no individual review text (`gate-logs/T4-batch-review.log:10`). The TiKV feature log shows successful compilation of both requested feature selections (`gate-logs/host-tikv.log:211`). The contribution audit remains deferred until publish.

The named external dependencies were exercised: typos and the renderer both ran, with 99 pages rendered and the link audit passing (`pdca-reviewer-666-evidence/ci.log:5`, `pdca-reviewer-666-evidence/ci.log:12`). No missing-tool or fixture-only substitute underlies the seam's acceptance evidence. The prior-art search covered all six changed paths and inspected every closed unmerged PR's file list; #647 overlaps only server code for segmented storage, while #508's comments document the seven rejected multipart attempts and subsequent split. 

The supplementary post-advisory checks passed for conformance (5 valid and 6 invalid vectors), shared-state scanning, the actual orchestrator-dependency scanner, and all-features licenses/bans/sources (`pdca-reviewer-666-evidence/remaining-ci.log:5`, `pdca-reviewer-666-evidence/remaining-ci.log:12`, `pdca-reviewer-666-evidence/deploy-guard.log:3`, `pdca-reviewer-666-evidence/deny-rerun.log:90`). The supplementary full DST run was interrupted after approximately ten minutes while its committed-seed regression test was still running; it establishes no complete DST pass or failure (`pdca-reviewer-666-evidence/remaining-ci.log:556`). C4 therefore still requires a complete CI result.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — issue 666 (multipart gateway seam)

**Bottom line: I could not refute the fix.** Legs 1–3 hold up when I break the code on purpose. Leg 4 (CI green) is not met, but this patch did not cause that. Two items need a human call, both `[human]`. I found nothing for the builder to fix (`[impl]`).

## Findings

- NEEDS-HUMAN [human] — **Leg 4 ("`cargo xtask ci` green") is not met, and the patch is not the cause.** `gate-logs/C4-ci.log:3476-3554` fails `cargo deny --all-features --config deny-all-features.toml check advisories` on RUSTSEC-2026-0346 and RUSTSEC-2026-0349 in `rustls 0.21.12`, which comes in via `tikv-client 0.4.0` → `wyrd-metadata-tikv` (`Cargo.lock:302`). The diff does not touch `Cargo.lock`. `run_ci` stops at the first failure (`xtask/src/main.rs:1683-1687`), so the frozen run never reached conformance, statics, deploy-guard or the DST step. I ran the two that could see this diff, using a scratch target dir: `cargo xtask statics` ("no DST-reachable shared mutable global state (ADR-0035)") and `cargo xtask conformance` ("5 valid + 6 invalid vectors pass"). Both pass. `wyrd-dst` depends on none of the touched crates (`crates/dst/Cargo.toml`), and the diff adds no orchestrator import, so the skipped tail cannot be broken by it. The human needs to decide: waive leg 4 for this slice, or land the rustls/tikv advisory fix first as a separate issue.

- NEEDS-HUMAN [human] — **Check the T4 "pass" before trusting it.** `check-gates.json` row `T4-batch-review` reports "0 blocking" from a "3x codex" review that took 8.78 s (`gate-logs/T4-batch-review.log`). Last round the same gate had 4 blocking findings. Three model passes over a 1,485-line diff in under 9 seconds is not plausible. The log has no per-pass output showing the passes actually ran on this diff, as opposed to a cache hit or a pass that failed quietly. My own review found no blocking defect, so this is about whether the gating row can be trusted, not about the code. Confirm the passes ran before counting the row.

## Refutation attempts that failed (this is a positive signal)

- **Leg 1 red, re-run by me.** I changed `Gateway::list_multipart_uploads` (`crates/server/src/lib.rs:835`) to return `Listed(Vec::new())`. `the_composition_root_answers_every_multipart_verb_not_implemented` then failed at `crates/server/tests/multipart_gateway_seam.rs:200` (`left: Listed([])`, `right: Refused(NotImplemented)`). The test calls the production impl through fully qualified trait calls, not a copy, so the red→green evidence is real. C4-verify's UNVERIFIABLE (exit 77) is the compile-only red the brief declared in advance.
- **Leg 3 counter.** I made the S3 object-path guard (`crates/gateway-s3/src/lib.rs:1777`) call `state.gateway.abort_multipart_upload(..)` and still answer the same 501. `every_multipart_form_still_answers_501_without_entering_the_seam` then failed at `multipart_gateway_seam.rs:398` (counter 1, expected 0). So the counter catches "entered the seam but still answered 501", which the status code alone cannot.
- **Wire oracle (the round-1 T4 finding).** I read `parse_response` / `decode_chunked` (`multipart_gateway_seam.rs:506-624`) looking for a torn response that still passes. I tried: a missing last chunk, a body shorter or longer than `content-length`, signed or overflowing chunk sizes, chunk extensions, bytes after the final CRLF, and a missing header terminator. Every one fails closed. The 18 torn fixtures at `:690` all hit the framing error before the XML checks run.
- **S3 bound (leg 2 is checked by reading the source).** Every public `S3Gateway` method is in the one `impl` at `crates/gateway-s3/src/lib.rs:227-229`, which has the three-trait bound. The bounds at `:797`, `:1527`, `:1638`, `:2002` and `:5148` are extended. The two bounds left as `G: ObjectGateway` (`:2318`, `:2416`) are private GET/HEAD helpers. A grep over `crates/` finds all seven test doubles with `impl MultipartGateway`. The server impl's bounds (`crates/server/src/lib.rs:757-761`) are the same as its `ContainerGateway` impl (`:689-694`), so any backend combination that met the old bound (tikv, fdb) also meets the new one.
- **The types match core.** `MultipartRefusal` (`crates/gateway-core/src/lib.rs:488`), `MultipartInvalidPart` (`:449`) and `MultipartBackpressure` (`:461`) match core's `Refusal` / `InvalidPart` / `Backpressure` (`crates/core/src/multipart.rs:4263-4346`) one variant per variant, plus the seam's two extra variants. None is `#[non_exhaustive]`. Every per-state answer in core (`crates/core/src/multipart.rs:4485-4545`) has a matching seam variant.
- **Timestamps (suspected, then dropped).** `ListedPart.modified`, `ListedUpload.initiated` and `MultipartPublication.modified` are documented as "epoch millis", while core calls its stamps "logical milliseconds" (`crates/core/src/multipart.rs:2298`, `:4359`). Proposal 0016 settles this: those stamps are the deployment wall clock, and "No logical-clock producer may open a session" (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:1950-1956`, `:1981-1984`). Not a defect.
- **C5's "10 missed" are false survivors.** `wyrd-gateway-core` has no tests of its own (`cargo test -p wyrd-gateway-core` runs 0), so a mutants run that only runs the mutated package's tests cannot kill anything there. I applied the `NamedParts::into_vec -> vec![(1, "xyzzy".into())]` mutant and ran the server test: `an_empty_complete_list_cannot_reach_the_seam` killed it at `multipart_gateway_seam.rs:135`. The two diff-coverage misses (`crates/gateway-s3/src/lib.rs:797`, `:2002`) are `where` lines, which never execute.
- **Minor, not raised as an item.** The trait doc's "Arguments arrive unvalidated" paragraph (`crates/gateway-core/src/lib.rs:694`) gives a typed answer for a bad upload id and a bad part number. It gives none for a `NamedParts` ETag that does not parse as a digest (core's Complete takes a validated `Digest`). A typed answer already exists for it: `InvalidPart { part_number, reason: DigestMismatch }` (`:453`) carries that entry's part number. So #670 needs no trait change. It deserves one doc line when #670 lands, not a rebuild now.

<!-- pdca:leaf-complete -->

### Advisory — code-review

No patch-introduced correctness bugs or hot-path efficiency regressions found.

- NEEDS-HUMAN [impl] — Low-priority reuse: `crates/server/tests/multipart_gateway_seam.rs:506` and `crates/server/tests/multipart_gateway_seam.rs:574` add near-copies of the response parser and chunk decoder at `crates/server/tests/s3_chunk_size_flag.rs:294` and `crates/server/tests/s3_chunk_size_flag.rs:357`. The copies already differ in repeated-transfer-encoding rejection, checked chunk-size arithmetic, and trailer validation. Extract a shared test-support parser for these two suites, retaining the new malformed-response fixtures, so framing fixes and regression coverage apply to both callers. This is an advisory cleanup, not a correctness blocker.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C2 Reproduction (red pre-fix) — Accept the declared compile-only baseline for this new API — stash/test/restore reproduces E0432 before assertions execute; a separate injected wrong refusal does fail the behavioral assertion (`brief.md:90`; `pdca-reviewer-666-evidence/red.log:7`; `pdca-reviewer-666-evidence/behavioral-red.log:280`).
- [ ] C4 Verification (red→green) — Resolve the existing TiKV advisory-gate disposition and obtain the required full CI green — the seven seam tests pass, but independent scanning reproduces the frozen all-features failure in unchanged dependencies; this is not a patch-caused defect (`pdca-reviewer-666-evidence/restored-green.log:19`; `pdca-reviewer-666-evidence/deny-rerun.log:10`; `gate-logs/C4-ci.log:6557`).
- [ ] Validation — fitness-to-purpose — Confirm that landing only the typed seam is useful for the planned follow-on slices — requests intentionally remain 501, so these passing checks do not establish usable multipart uploads (`brief.md:96`; `docs/design/architecture/05-building-block-view.md:137`).
- [ ] **Leg 4 ("`cargo xtask ci` green") is not met, and the patch is not the cause.** `gate-logs/C4-ci.log:3476-3554` fails `cargo deny --all-features --config deny-all-features.toml check advisories` on RUSTSEC-2026-0346 and RUSTSEC-2026-0349 in `rustls 0.21.12`, which comes in via `tikv-client 0.4.0` → `wyrd-metadata-tikv` (`Cargo.lock:302`). The diff does not touch `Cargo.lock`. `run_ci` stops at the first failure (`xtask/src/main.rs:1683-1687`), so the frozen run never reached conformance, statics, deploy-guard or the DST step. I ran the two that could see this diff, using a scratch target dir: `cargo xtask statics` ("no DST-reachable shared mutable global state (ADR-0035)") and `cargo xtask conformance` ("5 valid + 6 invalid vectors pass"). Both pass. `wyrd-dst` depends on none of the touched crates (`crates/dst/Cargo.toml`), and the diff adds no orchestrator import, so the skipped tail cannot be broken by it. The human needs to decide: waive leg 4 for this slice, or land the rustls/tikv advisory fix first as a separate issue.
- [ ] **Check the T4 "pass" before trusting it.** `check-gates.json` row `T4-batch-review` reports "0 blocking" from a "3x codex" review that took 8.78 s (`gate-logs/T4-batch-review.log`). Last round the same gate had 4 blocking findings. Three model passes over a 1,485-line diff in under 9 seconds is not plausible. The log has no per-pass output showing the passes actually ran on this diff, as opposed to a cache hit or a pass that failed quietly. My own review found no blocking defect, so this is about whether the gating row can be trusted, not about the code. Confirm the passes ran before counting the row.
- [ ] Low-priority reuse: `crates/server/tests/multipart_gateway_seam.rs:506` and `crates/server/tests/multipart_gateway_seam.rs:574` add near-copies of the response parser and chunk decoder at `crates/server/tests/s3_chunk_size_flag.rs:294` and `crates/server/tests/s3_chunk_size_flag.rs:357`. The copies already differ in repeated-transfer-encoding rejection, checked chunk-size arithmetic, and trailer validation. Extract a shared test-support parser for these two suites, retaining the new malformed-response fixtures, so framing fixes and regression coverage apply to both callers. This is an advisory cleanup, not a correctness blocker.
- [ ] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [ ] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo deny --all-features --config deny-all-features.toml check advisories` failed with exit status: 1
- [ ] external dependency: RustSec advisory database (RUSTSEC-2026-0346, RUSTSEC-2026-0349 against rustls 0.21.12 via the optional tikv-client 0.4.0) — `cargo xtask ci` fails at its all-features `cargo deny ... check advisories` step on base 7bf7eb9 (reproduced on a clean export), so leg (4) "the full gate is green" cannot be produced by this patch without a security-waiver change that belongs in its own PR on main; every other gate step passes on the patched tree (see "Leg (4)")
- [ ] **The supposedly fixed contract cannot express all failures its inputs require.** `brief.md:114-124` accepts `ContentHash`, raw upload IDs and `u32` part numbers, assigning validation to the implementer; `brief.md:128-139` reserves `Err` for store failures and fixes the answer vocabulary to core outcomes plus NotImplemented. On the target, `PartNumber::new(0)` returns `RecordError::PartNumberOutOfRange` (`crates/core/src/multipart.rs:938-942`), while the mirrored `Refusal` has no input-range or payload-integrity variant (`crates/core/src/multipart.rs:4302-4345`). The existing streaming implementation returns `GatewayError::PayloadMismatch` for a failed ContentHash check (`crates/server/src/lib.rs:392-396`); this is not a store failure. Define neutral validation/integrity answers, or explicitly permit typed non-store errors, before promising later slices only replace bodies (`brief.md:19-20`). Actual validation and wire mapping can remain in those later slices.
- [ ] **The proposed test does not prove two properties claimed for it.** The positive helper in `brief.md:29-31` still compiles if S3 never adds the MultipartGateway bound: the current constructor already accepts any ObjectGateway + ContainerGateway (`crates/gateway-s3/src/lib.rs:227-232`), including a real Gateway that also implements the new trait. Likewise, contrary to `brief.md:36-39`, a request could call the new NotImplemented stub and then emit exactly the existing subresource 501 (`crates/gateway-s3/src/lib.rs:1777-1783`), leaving leg (3) green despite entering the seam. Add a compile-fail case for a gateway lacking MultipartGateway and a no-call sentinel for routing, or explicitly assign both properties to source review instead of claiming these positive tests establish them.
- [ ] **The tracker’s full-gate acceptance requirement is missing from the brief.** `notes.json:1` explicitly requires “`cargo xtask ci` green”; `brief.md:24-40` promises all existing suites stay green but names only `cargo test -p wyrd-server`. That command does not compile/run dependency crates’ unit-test modules, including the S3 doubles this change must update (`crates/gateway-s3/src/lib.rs:3516-3517`, `:5141-5153`). It also omits the broader gate required by target `AGENTS.md:41-54`. Restore `cargo xtask ci` as an explicit acceptance command alongside the focused seam test, with skipped external-tool checks recorded rather than counted as green.
- [ ] **Leg (4) "full gate green" is not met, but this patch did not cause the failure.** C4-ci failed in both attempts at `cargo deny --all-features --config deny-all-features.toml check advisories` (`gate-logs/C4-ci.log:3472-3550`). The failures are RUSTSEC-2026-0346 and RUSTSEC-2026-0349 against `rustls 0.21.12` via `tikv-client 0.4.0` → `wyrd-metadata-tikv`. The patch does not touch `Cargo.lock` or any manifest. `run_ci` stops at the first failure (`xtask/src/main.rs:1683-1687`), so conformance, statics, deploy-guard and DST **never ran** on this patch. I ran the skipped steps myself on the patched tree:
- [ ] external dependency: RustSec advisory database (RUSTSEC-2026-0346, RUSTSEC-2026-0349 against rustls 0.21.12 via the optional tikv-client 0.4.0) — `cargo xtask ci` fails at its all-features `cargo deny ... check advisories` step on base 7bf7eb9, so leg (4) "the full gate is green" cannot be produced by this patch; every other gate step passes on the patched tree

## 7. Proven / not proven
- Proven by which oracle: gates overall = fail (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): Auto-iterate (round 2): rebuilding for the implementation-level findings — C2 Reproduction (red pre-fix) — Accept the declared compile-only baseline for this new API — stash/test/restore reproduces E0432 before assertions execute; a separate injected wrong refusal does fail the behavioral assertion (`brief.md:90`; `pdca-reviewer-666-evidence/red.log:7`; `pdca-reviewer-666-evidence/behavioral-red.log:280`).; C4 Verification (red→green) — Resolve the existing TiKV advisory-gate disposition and obtain the required full CI green — the seven seam tests pass, but independent scanning reproduces the frozen all-features failure in unchanged dependencies; this is not a patch-caused defect (`pdca-reviewer-666-evidence/restored-green.log:19`; `pdca-reviewer-666-evidence/deny-rerun.log:10`; `gate-logs/C4-ci.log:6557`).; Low-priority reuse: `crates/server/tests/multipart_gateway_seam.rs:506` and `crates/server/tests/multipart_gateway_seam.rs:574` add near-copies of the response parser and chunk decoder at `crates/server/tests/s3_chunk_size_flag.rs:294` and `crates/server/tests/s3_chunk_size_flag.rs:357`. The copies already differ in repeated-transfer-encoding rejection, checked chunk-size arithmetic, and trailer validation. Extract a shared test-support parser for these two suites, retaining the new malformed-response fixtures, so framing fixes and regression coverage apply to both callers. This is an advisory cleanup, not a correctness blocker.; C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo deny --all-features --config deny-all-features.toml check advisories` failed with exit status: 1. Deferred, not addressed here: 6 finding(s) that need human judgment, held in deferred-findings.json for the human at handover — not build work
- By / date: auto-iterate / 2026-10-14

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
