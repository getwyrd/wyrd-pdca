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
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 98.6% — 141 of 143 instrumentable changed lines executed (floor 80%); 143 of 449 changed lines were instru
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass —  WARN No mutants were viable: perhaps there is a problem with building in a scratch directory. Look in mutants.out/log/*

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_666/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.33s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of #666: add the typed `MultipartGateway` companion seam while retaining multipart HTTP 501 responses — one wire-test defect needs iteration; baseline-red evidence and full-CI disposition remain human decisions.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The acceptance boundary is explicit: establish the six-verb contract without enabling multipart routing; later behavior belongs to #667–#670 (`brief.md:19`, `brief.md:77`). |
| C2 Reproduction (red pre-fix) | NEEDS-HUMAN | Accept or reject the declared compile-only baseline evidence — independent stash/test/restore reproduces E0432 for the absent API, but no pre-fix behavioral assertion executes (`brief.md:90`; `pdca-reviewer-666-evidence/red.log:267`). |
| C3 Change | PASS | The six-file patch stays within the planned seam, implementers, tests and architecture update; the denylist and both routing guards are unchanged (`crates/gateway-s3/src/lib.rs:410`, `crates/gateway-s3/src/lib.rs:1730`, `docs/design/architecture/05-building-block-view.md:137`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Resolve the baseline advisory-gate disposition and obtain a complete CI result — four seam tests pass independently, but frozen CI fails on unchanged TiKV/rustls dependencies and reviewer CI stops at a read-only advisory lock; neither establishes a patch-caused build defect (`gate-logs/C4-ci.log:6472`; `pdca-reviewer-666-evidence/ci.log:3266`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Make malformed HTTP framing fail the new wire oracle — otherwise truncated responses can satisfy the claimed unchanged-wire acceptance test; six concrete false passes were reproduced (`crates/server/tests/multipart_gateway_seam.rs:463`, `crates/server/tests/multipart_gateway_seam.rs:473`; `pdca-reviewer-666-evidence/wire-probe.log:1`). |
| T1 Structure | PASS | The new concern preserves the narrow dependency boundary and composition-root ownership; neither seam crate gains a core dependency (`crates/gateway-core/Cargo.toml:11`, `crates/gateway-s3/Cargo.toml:11`, `crates/server/src/lib.rs:756`). |
| T2 Shape | PASS | The neutral outcomes cover core's refusal vocabulary plus invalid part numbers and unsupported capability; streaming and default-method conventions match the existing seam (`crates/gateway-core/src/lib.rs:488`, `crates/gateway-core/src/lib.rs:694`; `crates/core/src/multipart.rs:4302`). |
| T3 Runtime | PASS | The implemented behavior is exercised: all six real/default methods return typed refusals, and six signed forms reach the listener without entering either sentinel seam (`crates/server/tests/multipart_gateway_seam.rs:116`, `crates/server/tests/multipart_gateway_seam.rs:355`; `pdca-reviewer-666-evidence/green.log:17`). |
| T4 Contribution | PASS | Architecture currency and path-based prior art support this narrow contribution; merged history and closed/unmerged PR files were checked for all six affected paths, and #508 records the rejected monolith and subsequent split; publish-artifact audit is separately N/A below (`docs/design/architecture/05-building-block-view.md:137`; `pdca-reviewer-666-evidence/prior-art.log:122`). |
| T5 Judgment | PASS | The patch respects the settled split and adds no capability probe or runtime guard masking an eager cause; production multipart execution remains assigned to later issues (`crates/server/src/lib.rs:751`, `crates/gateway-core/src/lib.rs:672`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Decide whether this foundation-only API is the desired deliverable for #666 — clients still cannot perform multipart uploads, and the complete user workflow belongs to later slices (`brief.md:96`; `docs/design/architecture/05-building-block-view.md:137`). |

The actionable finding is a test-fidelity defect, not an observed malformed production response. At `crates/server/tests/multipart_gateway_seam.rs:473`, an invalid chunk size becomes zero; at `:475`, an oversized/truncated chunk returns the accumulated prefix; at `:479`, chunk CRLF bytes are skipped without validation. The helper also accepts EOF without a terminal zero chunk/final CRLF. The non-chunked branch at `:463` never checks `Content-Length`. Because assertions at `:390` only require status 501 and two body substrings, incomplete framing and even incomplete XML can pass. This falls under the target's protocol-input rubric (`AGENTS.md:161`).

I compiled a standalone probe containing the target's response-decoding code verbatim and applied the same assertions. All six malformed inputs passed: missing chunk terminators, an invalid next size, a truncated next chunk, a zero chunk missing its final CRLF, invalid data CRLF, and a body shorter than declared Content-Length. Reproduce with `./pdca-reviewer-666-evidence/wire_probe`; source is `pdca-reviewer-666-evidence/wire_probe.rs`. Use a strict existing HTTP client/parser or validate the declared length and complete chunk framing, with negative cases for these failures. This consolidates the four overlapping batch-review entries into one finding (`gate-logs/T4-batch-review.log:10`).

The independent evidence supports the seam implementation, with explicit limits:

- **PASS, applied patch:** `cargo test --locked --offline -p wyrd-server --test multipart_gateway_seam` ran all four tests. Stashing the tracked patch while retaining the new test produced the expected unresolved-import compile failure; restoring it produced the passing run. This is not a behavioral red→green (`pdca-reviewer-666-evidence/red.log:267`, `pdca-reviewer-666-evidence/green.log:17`).
- **PASS, required S3 bound:** an independent compiler probe calling `S3Gateway::new` with an object/container gateway lacking `MultipartGateway` fails specifically with E0277 at `crates/gateway-s3/src/lib.rs:229`; adding the empty multipart implementation compiles. This discharges the constructor-bound question with compilation as well as source review (`pdca-reviewer-666-evidence/bound-negative.log:1`, `pdca-reviewer-666-evidence/bound-positive.log:1`). All six specified S3 bounds were inspected.
- **PASS, checks actually completed:** reviewer `cargo xtask ci` completed typos, docs lint, the 99-page render/link audit, repository guards, formatting, workspace clippy/build/tests excluding DST, and cargo-machete. The brief's `typos` and `docs-renderer` dependencies were exercised. Separate reruns passed all 5 valid/6 invalid conformance vectors and the statics scanner (`pdca-reviewer-666-evidence/ci.log:6`, `pdca-reviewer-666-evidence/scanners.log:1`).
- **NEEDS-HUMAN, complete CI:** reviewer CI stopped at the advisory database's read-only lock (`pdca-reviewer-666-evidence/ci.log:3266`), a host limitation. The frozen run instead passed default cargo-deny and failed the all-features advisory check on RUSTSEC-2026-0346 and RUSTSEC-2026-0349 through TiKV's rustls 0.21.12 (`gate-logs/C4-ci.log:6469`). `Cargo.lock:3294` and dependency manifests are unchanged by this patch. Dependency maintenance belongs outside this seam change; the existing TiKV exposure-policy context is [#543](https://github.com/getwyrd/wyrd/issues/543). Neither run completed full CI; DST and the remaining downstream CI steps are not certified here.
- **PASS from frozen evidence, diff coverage:** 141/143 instrumentable changed lines executed (98.6%); 306 changed lines were unscored. This is not coverage of all new declarations (`gate-logs/C4-diff-cov.log:301`).
- **N/A as behavioral mutation evidence:** the mutation command reports success, but all six mutants were unviable; only its unmutated baseline ran successfully. It proves no assertion-level mutation rejection (`gate-logs/C5-mutants.log:10`).
- **PASS from frozen evidence, TiKV compile:** both feature-enabled clippy commands completed successfully; this is compile evidence, not a live TiKV integration test (`gate-logs/host-tikv.log:2`, `gate-logs/host-tikv.log:211`).
- **N/A, T4-contribution:** the publish artifacts do not exist at Check by design; the substantive audit reruns at publish (`gate-logs/T4-contribution.log:10`).

Prior-art investigation is complete for the available repository history: GitHub history was queried by each affected file path at the brief's base, and all 381 closed PRs were enumerated, including file lists for all 19 unmerged PRs. The only unmerged path overlap was the unrelated segmented-map PR #647. [#508's rejection record](https://github.com/getwyrd/wyrd/issues/508#issuecomment-5080780029) confirms the monolithic attempt's reviewability rejection, and its [six-slice follow-up](https://github.com/getwyrd/wyrd/issues/508#issuecomment-5137419986) assigns the companion trait to #666. These are consistent with the merged `ContainerGateway` precedent; no conflicting prior multipart seam was found (`pdca-reviewer-666-evidence/prior-art.log:3`, `pdca-reviewer-666-evidence/prior-art.log:127`).

Source citations above are relative to `$PDCA_TARGET`; evidence citations are relative to this review directory. The disposable target is readable and current for this patch: all six post-image blob hashes match `patch.diff` after restoration. No production or test source was edited by this review. No `INTEGRATION.md` was present in the supplied target; the supplied and target `AGENTS.md` review rubric was applied.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — issue 666 (multipart gateway seam)

Bottom line: I could not break the fix itself. Both new tests go red when I break the thing each one guards (results below). Three findings remain: one gap in the seam's contract that a later slice will hit, one gate that is red for reasons outside this patch, and the gating T4 review findings, which hold up but have low impact.

## Findings

- NEEDS-HUMAN [impl] — **An empty Complete list has no typed answer.** `crates/gateway-core/src/lib.rs:714-719` takes `named_parts: Vec<(u32, String)>`. Core refuses an empty list with `RecordError::NoPartsNamed` (`crates/core/src/multipart.rs:4095-4097`). Every seam arm for a bad list carries a part number (`InvalidPart { part_number, .. }` at `crates/gateway-core/src/lib.rs:500`, `PartNumberOutOfRange { part_number }` at `:528`), and the trait doc at `:657-659` rules out `Err` for protocol outcomes. So when #670 handles `complete_multipart_upload("b", "k", <valid id>, vec![])`, it has three options, all bad: make up a part number, return `Err` (a 500 where S3 answers `400 MalformedXML`), or change the trait. The brief requires that every validation failure have a typed answer "so later slices only replace bodies" (brief.md:152-154). The cheapest fix that keeps the brief's fixed shape is a doc precondition on the method: `named_parts` is non-empty, because the wire layer refuses an empty list as malformed before calling. The same edit can also say which arm wins when `upload_part` gets both an unparsable upload id and an out-of-range part number. Today `:661-664` names both arms but gives no order.

- NEEDS-HUMAN [human] — **Leg (4) "full gate green" is not met, but this patch did not cause the failure.** C4-ci failed in both attempts at `cargo deny --all-features --config deny-all-features.toml check advisories` (`gate-logs/C4-ci.log:3472-3550`). The failures are RUSTSEC-2026-0346 and RUSTSEC-2026-0349 against `rustls 0.21.12` via `tikv-client 0.4.0` → `wyrd-metadata-tikv`. The patch does not touch `Cargo.lock` or any manifest. `run_ci` stops at the first failure (`xtask/src/main.rs:1683-1687`), so conformance, statics, deploy-guard and DST **never ran** on this patch. I ran the skipped steps myself on the patched tree:
  - `cargo xtask statics` passed ("no DST-reachable shared mutable global state").
  - `cargo xtask conformance` passed (5 valid + 6 invalid vectors).
  - Deploy-guard: the diff adds no orchestrator import.
  - DST: `wyrd-dst` depends on none of the changed crates (`crates/dst/Cargo.toml:21-63`).

  So the only red is the advisory database. A human has to decide whether to accept with leg 4 formally unmet, or wait for a separate fix (a tikv-client bump or a dated ignore entry in `deny-all-features.toml`). That fix is out of scope here.

- NEEDS-HUMAN [impl] — **The four blocking T4 findings are real, but they cannot change this test's verdict.** I put a panic inside `dechunk` (`crates/server/tests/multipart_gateway_seam.rs:469`) and it fired in legs 2 and 3. Every 501 here comes back with chunked encoding (HTTP's chunk-by-chunk body framing), so `dechunk` is live code. It accepts a body with no terminal `0\r\n\r\n` chunk, and it treats an unparsable size line as 0 (`:473`, `unwrap_or(0)`). The fixed-length branch (`:463`) never checks Content-Length. So a torn response whose first chunk carries the XML would still pass the status, code and subresource checks at `:390-398`. Leg 3's counter is read separately, so no torn response can flip the property this slice claims. Still, T4 is a gating row. The fix is cheap:
  - panic on a bad size line;
  - require the terminal chunk;
  - compare the body length with Content-Length on the fixed-length branch.

  The same loose helper is copied in peer suites (`s3_list_objects.rs:235`, `s3_http_wire.rs:171`, `s3_head_object.rs:172`, `s3_copy_object_guard.rs:152`, `s3_streaming_trailer.rs:195`). Those copies are outside this diff, so they should be declined with an issue reference, not fixed here.

## Attacks I tried that did not land

- **Leg 1 tests the real `Gateway`, not a copy.** I changed `Gateway::list_multipart_uploads` (`crates/server/src/lib.rs:834`) to answer `Listed(vec![])`. `the_composition_root_answers_every_multipart_verb_not_implemented` failed (`left: Listed([]) right: Refused(NotImplemented)`, test `:180`), and the other three tests stayed green. This replaces the evidence C5-mutants could not give: its "pass" is empty (6 of 6 mutants unviable). The cause is probably the answer enums having no `Default`, so a mutant like `Ok(Default::default())` cannot compile, rather than the scratch-build problem the gate's WARN guesses. Either way, that row proves nothing.
- **Leg 3's counter catches what the status cannot.** I made the object route call `state.gateway.create_multipart_upload` for `POST ?uploads` just before the denylist guard (`crates/gateway-s3/src/lib.rs:1777`). The response stayed 501 (leg 2 still passed), but leg 3 failed on the counter (`left: 1 right: 0`, test `:365`). The brief's claim holds.
- **The S3 crate requires the trait (checked by reading the source).** `MultipartGateway` is on every bound the brief lists: `crates/gateway-s3/src/lib.rs:229, :797, :1527, :1638, :2002`, plus the test helper at `:5148`. All seven listed test doubles get an empty impl. Every other `S3Gateway::new` caller (`crates/server/tests/*.rs`, `crates/validate/tests/s3_client_roundtrip.rs`, `crates/server/src/cli.rs:2664`) passes the real `Gateway`, which has the generic impl at `crates/server/src/lib.rs:756`. That impl uses the same bounds as the `ContainerGateway` impl at `:688`.
- **The types match core arm for arm.** `MultipartRefusal`, `MultipartBackpressure` and `MultipartInvalidPart` match core's `Refusal`, `Backpressure` and `InvalidPart` (`crates/core/src/multipart.rs:4263-4346`). The four answer enums match `CreateOutcome`, `UploadPartOutcome`, `CompleteOutcome` and `AbortOutcome` (`:4380-4450`). None is `#[non_exhaustive]`. The bare `u64` timestamps do not force anyone to invent a value, because core always records `created_at_millis`, `committed_at_millis` and `completed_at_millis`, stamped from the wall clock (0016 §"clock lifecycle", `docs/design/proposals/draft/0016-multipart-commit-protocol.md:1950-1957`).
- **Routing is untouched.** The `gateway-s3` diff is imports, bounds and empty test impls only. The object route's undecoded denylist check (`:1777`, vs the decoded check at `:1730`) predates this patch, and routing belongs to #667, so I did not file it.
- **The red→green claim checks out.** C4-verify's UNVERIFIABLE result was declared in advance, and its log confirms the test does not compile on the reverted base (E0432 on every new symbol). The same test is green with the patch applied, in both the gate log and my rerun (4/4 passed).

<!-- pdca:leaf-complete -->

### Advisory — code-review

No introduced production correctness or efficiency defects found. One test-correctness finding also offers a reuse opportunity:

- NEEDS-HUMAN [impl] — `crates/server/tests/multipart_gateway_seam.rs:463` and `crates/server/tests/multipart_gateway_seam.rs:473`: the new wire helper can report success for malformed or truncated responses. The non-chunked branch ignores the declared `Content-Length`; `dechunk` treats invalid sizes as termination, returns accumulated bytes on truncation, and never requires the chunk delimiters or terminal CRLF. For example, a complete first chunk containing both asserted XML fragments followed by EOF passes the 501 assertions without a terminating zero chunk. Likewise, a short body containing those fragments passes despite a larger declared length. This weakens the new wire regression under the protocol-input rubric. Extract/reuse the framing checks already present in `crates/server/tests/s3_chunk_size_flag.rs:294` (`parse_response`) and `crates/server/tests/s3_chunk_size_flag.rs:357` (`decode_chunked`), and verify that truncated fixed-length bodies and malformed/incomplete chunked bodies fail before the XML assertions.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C2 Reproduction (red pre-fix) — Accept or reject the declared compile-only baseline evidence — independent stash/test/restore reproduces E0432 for the absent API, but no pre-fix behavioral assertion executes (`brief.md:90`; `pdca-reviewer-666-evidence/red.log:267`).
- [ ] C4 Verification (red→green) — Resolve the baseline advisory-gate disposition and obtain a complete CI result — four seam tests pass independently, but frozen CI fails on unchanged TiKV/rustls dependencies and reviewer CI stops at a read-only advisory lock; neither establishes a patch-caused build defect (`gate-logs/C4-ci.log:6472`; `pdca-reviewer-666-evidence/ci.log:3266`).
- [ ] C5 Causal adequacy — Make malformed HTTP framing fail the new wire oracle — otherwise truncated responses can satisfy the claimed unchanged-wire acceptance test; six concrete false passes were reproduced (`crates/server/tests/multipart_gateway_seam.rs:463`, `crates/server/tests/multipart_gateway_seam.rs:473`; `pdca-reviewer-666-evidence/wire-probe.log:1`).
- [ ] Validation — fitness-to-purpose — Decide whether this foundation-only API is the desired deliverable for #666 — clients still cannot perform multipart uploads, and the complete user workflow belongs to later slices (`brief.md:96`; `docs/design/architecture/05-building-block-view.md:137`).
- [ ] **An empty Complete list has no typed answer.** `crates/gateway-core/src/lib.rs:714-719` takes `named_parts: Vec<(u32, String)>`. Core refuses an empty list with `RecordError::NoPartsNamed` (`crates/core/src/multipart.rs:4095-4097`). Every seam arm for a bad list carries a part number (`InvalidPart { part_number, .. }` at `crates/gateway-core/src/lib.rs:500`, `PartNumberOutOfRange { part_number }` at `:528`), and the trait doc at `:657-659` rules out `Err` for protocol outcomes. So when #670 handles `complete_multipart_upload("b", "k", <valid id>, vec![])`, it has three options, all bad: make up a part number, return `Err` (a 500 where S3 answers `400 MalformedXML`), or change the trait. The brief requires that every validation failure have a typed answer "so later slices only replace bodies" (brief.md:152-154). The cheapest fix that keeps the brief's fixed shape is a doc precondition on the method: `named_parts` is non-empty, because the wire layer refuses an empty list as malformed before calling. The same edit can also say which arm wins when `upload_part` gets both an unparsable upload id and an out-of-range part number. Today `:661-664` names both arms but gives no order.
- [ ] **Leg (4) "full gate green" is not met, but this patch did not cause the failure.** C4-ci failed in both attempts at `cargo deny --all-features --config deny-all-features.toml check advisories` (`gate-logs/C4-ci.log:3472-3550`). The failures are RUSTSEC-2026-0346 and RUSTSEC-2026-0349 against `rustls 0.21.12` via `tikv-client 0.4.0` → `wyrd-metadata-tikv`. The patch does not touch `Cargo.lock` or any manifest. `run_ci` stops at the first failure (`xtask/src/main.rs:1683-1687`), so conformance, statics, deploy-guard and DST **never ran** on this patch. I ran the skipped steps myself on the patched tree:
- [ ] **The four blocking T4 findings are real, but they cannot change this test's verdict.** I put a panic inside `dechunk` (`crates/server/tests/multipart_gateway_seam.rs:469`) and it fired in legs 2 and 3. Every 501 here comes back with chunked encoding (HTTP's chunk-by-chunk body framing), so `dechunk` is live code. It accepts a body with no terminal `0\r\n\r\n` chunk, and it treats an unparsable size line as 0 (`:473`, `unwrap_or(0)`). The fixed-length branch (`:463`) never checks Content-Length. So a torn response whose first chunk carries the XML would still pass the status, code and subresource checks at `:390-398`. Leg 3's counter is read separately, so no torn response can flip the property this slice claims. Still, T4 is a gating row. The fix is cheap:
- [ ] `crates/server/tests/multipart_gateway_seam.rs:463` and `crates/server/tests/multipart_gateway_seam.rs:473`: the new wire helper can report success for malformed or truncated responses. The non-chunked branch ignores the declared `Content-Length`; `dechunk` treats invalid sizes as termination, returns accumulated bytes on truncation, and never requires the chunk delimiters or terminal CRLF. For example, a complete first chunk containing both asserted XML fragments followed by EOF passes the 501 assertions without a terminating zero chunk. Likewise, a short body containing those fragments passes despite a larger declared length. This weakens the new wire regression under the protocol-input rubric. Extract/reuse the framing checks already present in `crates/server/tests/s3_chunk_size_flag.rs:294` (`parse_response`) and `crates/server/tests/s3_chunk_size_flag.rs:357` (`decode_chunked`), and verify that truncated fixed-length bodies and malformed/incomplete chunked bodies fail before the XML assertions.
- [ ] C4 per-fix red->green: this patch's test red pre-fix, green post-fix unverifiable —                why this slice has no isolable red (the cargo output is above).
- [ ] C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo deny --all-features --config deny-all-features.toml check advisories` failed with exit status: 1
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_666/review-b
- [ ] external dependency: RustSec advisory database (RUSTSEC-2026-0346, RUSTSEC-2026-0349 against rustls 0.21.12 via the optional tikv-client 0.4.0) — `cargo xtask ci` fails at its all-features `cargo deny ... check advisories` step on base 7bf7eb9, so leg (4) "the full gate is green" cannot be produced by this patch; every other gate step passes on the patched tree
- [ ] **The supposedly fixed contract cannot express all failures its inputs require.** `brief.md:114-124` accepts `ContentHash`, raw upload IDs and `u32` part numbers, assigning validation to the implementer; `brief.md:128-139` reserves `Err` for store failures and fixes the answer vocabulary to core outcomes plus NotImplemented. On the target, `PartNumber::new(0)` returns `RecordError::PartNumberOutOfRange` (`crates/core/src/multipart.rs:938-942`), while the mirrored `Refusal` has no input-range or payload-integrity variant (`crates/core/src/multipart.rs:4302-4345`). The existing streaming implementation returns `GatewayError::PayloadMismatch` for a failed ContentHash check (`crates/server/src/lib.rs:392-396`); this is not a store failure. Define neutral validation/integrity answers, or explicitly permit typed non-store errors, before promising later slices only replace bodies (`brief.md:19-20`). Actual validation and wire mapping can remain in those later slices.
- [ ] **The proposed test does not prove two properties claimed for it.** The positive helper in `brief.md:29-31` still compiles if S3 never adds the MultipartGateway bound: the current constructor already accepts any ObjectGateway + ContainerGateway (`crates/gateway-s3/src/lib.rs:227-232`), including a real Gateway that also implements the new trait. Likewise, contrary to `brief.md:36-39`, a request could call the new NotImplemented stub and then emit exactly the existing subresource 501 (`crates/gateway-s3/src/lib.rs:1777-1783`), leaving leg (3) green despite entering the seam. Add a compile-fail case for a gateway lacking MultipartGateway and a no-call sentinel for routing, or explicitly assign both properties to source review instead of claiming these positive tests establish them.
- [ ] **The tracker’s full-gate acceptance requirement is missing from the brief.** `notes.json:1` explicitly requires “`cargo xtask ci` green”; `brief.md:24-40` promises all existing suites stay green but names only `cargo test -p wyrd-server`. That command does not compile/run dependency crates’ unit-test modules, including the S3 doubles this change must update (`crates/gateway-s3/src/lib.rs:3516-3517`, `:5141-5153`). It also omits the broader gate required by target `AGENTS.md:41-54`. Restore `cargo xtask ci` as an explicit acceptance command alongside the focused seam test, with skipped external-tool checks recorded rather than counted as green.

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
- Iteration delta (if iterating): Auto-iterate (round 1): rebuilding for the implementation-level findings — C2 Reproduction (red pre-fix) — Accept or reject the declared compile-only baseline evidence — independent stash/test/restore reproduces E0432 for the absent API, but no pre-fix behavioral assertion executes (`brief.md:90`; `pdca-reviewer-666-evidence/red.log:267`).; C4 Verification (red→green) — Resolve the baseline advisory-gate disposition and obtain a complete CI result — four seam tests pass independently, but frozen CI fails on unchanged TiKV/rustls dependencies and reviewer CI stops at a read-only advisory lock; neither establishes a patch-caused build defect (`gate-logs/C4-ci.log:6472`; `pdca-reviewer-666-evidence/ci.log:3266`).; C5 Causal adequacy — Make malformed HTTP framing fail the new wire oracle — otherwise truncated responses can satisfy the claimed unchanged-wire acceptance test; six concrete false passes were reproduced (`crates/server/tests/multipart_gateway_seam.rs:463`, `crates/server/tests/multipart_gateway_seam.rs:473`; `pdca-reviewer-666-evidence/wire-probe.log:1`).; **An empty Complete list has no typed answer.** `crates/gateway-core/src/lib.rs:714-719` takes `named_parts: Vec<(u32, String)>`. Core refuses an empty list with `RecordError::NoPartsNamed` (`crates/core/src/multipart.rs:4095-4097`). Every seam arm for a bad list carries a part number (`InvalidPart { part_number, .. }` at `crates/gateway-core/src/lib.rs:500`, `PartNumberOutOfRange { part_number }` at `:528`), and the trait doc at `:657-659` rules out `Err` for protocol outcomes. So when #670 handles `complete_multipart_upload("b", "k", <valid id>, vec![])`, it has three options, all bad: make up a part number, return `Err` (a 500 where S3 answers `400 MalformedXML`), or change the trait. The brief requires that every validation failure have a typed answer "so later slices only replace bodies" (brief.md:152-154). The cheapest fix that keeps the brief's fixed shape is a doc precondition on the method: `named_parts` is non-empty, because the wire layer refuses an empty list as malformed before calling. The same edit can also say which arm wins when `upload_part` gets both an unparsable upload id and an out-of-range part number. Today `:661-664` names both arms but gives no order.; **The four blocking T4 findings are real, but they cannot change this test's verdict.** I put a panic inside `dechunk` (`crates/server/tests/multipart_gateway_seam.rs:469`) and it fired in legs 2 and 3. Every 501 here comes back with chunked encoding (HTTP's chunk-by-chunk body framing), so `dechunk` is live code. It accepts a body with no terminal `0\r\n\r\n` chunk, and it treats an unparsable size line as 0 (`:473`, `unwrap_or(0)`). The fixed-length branch (`:463`) never checks Content-Length. So a torn response whose first chunk carries the XML would still pass the status, code and subresource checks at `:390-398`. Leg 3's counter is read separately, so no torn response can flip the property this slice claims. Still, T4 is a gating row. The fix is cheap:; `crates/server/tests/multipart_gateway_seam.rs:463` and `crates/server/tests/multipart_gateway_seam.rs:473`: the new wire helper can report success for malformed or truncated responses. The non-chunked branch ignores the declared `Content-Length`; `dechunk` treats invalid sizes as termination, returns accumulated bytes on truncation, and never requires the chunk delimiters or terminal CRLF. For example, a complete first chunk containing both asserted XML fragments followed by EOF passes the 501 assertions without a terminating zero chunk. Likewise, a short body containing those fragments passes despite a larger declared length. This weakens the new wire regression under the protocol-input rubric. Extract/reuse the framing checks already present in `crates/server/tests/s3_chunk_size_flag.rs:294` (`parse_response`) and `crates/server/tests/s3_chunk_size_flag.rs:357` (`decode_chunked`), and verify that truncated fixed-length bodies and malformed/incomplete chunked bodies fail before the XML assertions.; C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo deny --all-features --config deny-all-features.toml check advisories` failed with exit status: 1; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 4 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_666/review-b. Deferred, not addressed here: 5 finding(s) that need human judgment, held in deferred-findings.json for the human at handover — not build work
- By / date: auto-iterate / 2026-10-14

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
