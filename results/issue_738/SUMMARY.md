# Result — issue 738 / s3-role-chunk-size-flag

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: The `s3` role cannot be told what chunk size to use, so every gateway chunks
  at the 1 MiB default whatever its workload or fleet. The seam exists and the sibling
  command already exposes it; only the wiring is missing. Verified on `main` at `36f006d`:
  `Gateway::with_chunk_size` exists (`crates/server/src/lib.rs:144-148`, doc-commented
  "mainly so tests can force multi-chunk objects"); `wyrd put --chunk-size N` is parsed at
  `crates/server/src/cli.rs:555-560`; `cmd_s3` (`cli.rs:2151-2263`) parses no chunk size,
  so none of the six `Gateway::new` arms in `serve_s3_dispatch`
  (`cli.rs:2352,2358,2365,2372,2378,2385`) calls `with_chunk_size`, and the gateway's own
  `DEFAULT_CHUNK_SIZE` (`lib.rs:51`, `1 << 20`, applied at `lib.rs:100`) always wins. Worse
  than missing: `ParsedArgs::parse` (`cli.rs:2539-2561`) stores any unknown `--flag value`
  without complaint, so `wyrd s3 --chunk-size 65536` today starts, ignores the flag, and
  chunks at 1 MiB.
- Success criterion: BINDING — one NEW integration test file,
  `crates/server/tests/s3_chunk_size_flag.rs`, that drives the **built `wyrd` binary**
  (`env!("CARGO_BIN_EXE_wyrd")`) as an `s3` role on `--s3-listen 127.0.0.1:0` and asserts
  all four legs below. Every role it starts is pinned with `--metadata-backend redb
  --coordination-backend mem`. Chunks are counted as `<32-hex>` directories holding
  `.frag` files (`FsChunkStore` layout, `crates/chunkstore-fs/src/lib.rs:91-93`).
  * **(A) The flag sets the chunk size; its absence keeps 1 MiB.** With
    `--chunk-size 524288`, a signed PUT of one 2,097,152-byte object returns 200 and leaves
    exactly **4** chunk directories under `<data-dir>/chunks`. The same object PUT through a
    role started with NO `--chunk-size` leaves exactly **2**.
  * **(B) Exact accept/refuse boundaries.** The accepted range is `1 ..= 16777216`
    (16 MiB, inclusive). `1` and `16777216` are ACCEPTED: the role prints its
    `wyrd s3: serving S3-compatible HTTP on <addr>` line. `0`, `16777217` and `1MiB` are
    REFUSED: the process exits non-zero, stderr names `--chunk-size`, and the listen line
    never appears.
  * **(C) The ceiling fits today's cluster transport.** One production D server
    (`wyrd_server::dserver::DServer`, in-process, with its gRPC limits exactly as on `main`)
    and a role started with `--endpoints <that D server> --chunk-size 16777216`: a signed PUT
    of one 16,777,216-byte object returns 200, the D server's store then holds exactly **1**
    chunk directory, and a signed GET returns 200 with a body byte-equal to what was PUT.
  * **(D) The usage text lists the flag.** Bare `wyrd` exits 2 and its `wyrd s3 …` usage
    line contains `[--chunk-size N]`.
  The ceiling value is SETTLED and is not Do's to change; see Design. It is the maintainer's
  decision, taken in this re-plan's Plan session on 2026-10-01 (asked "is 16 MiB OK?",
  answered "yes, ceiling of 16 MB is fine"). It answers the 2026-09-30 sign-off's
  instruction "Settle a max chunk size that fits the D-server gRPC limit"
  (`iteration-v3/SUMMARY.md` §9). That sign-off did not name a value.
  "Default unchanged" means the chunk count in (A), not byte-identical persisted state: the
  gateway stamps `modified: Some(now_millis())` (`lib.rs:197`) and draws a random chunk-id
  epoch per process (`lib.rs:103`, `lib.rs:274`), so no two runs agree byte for byte.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: (a) `cmd_s3` reads `--chunk-size`. When the flag is absent, the gateway is
  composed exactly as today. (b) The value reaches the gateway on all six
  `(metadata, coordination)` arms of `serve_s3_dispatch`, on both chunk planes. If that
  changes `serve_s3_role`'s public signature, update its one external caller
  (`crates/server/tests/s3_gateway_cluster.rs:153`) and say so in `build-notes.md`.
  (c) Refuse, at startup and before the listener binds, any value that does not parse the
  way `cmd_put` parses it (`str::parse::<usize>`, `cli.rs:555-558`) or falls outside
  `1 ..= 16777216`, naming the flag in the `s3:` style of the role's other refusals. Don't rely on the silent `.max(1)` clamp in `with_chunk_size`. (d) State the
  ceiling and its reason (the D-server transport limit, below) once in code, beside the
  default, where whoever next wants to raise it will read why. (e) `with_chunk_size`'s doc
  comment: it is the role's configuration seam now, not a test affordance. (f) The `wyrd s3`
  usage line (`cli.rs:492`) gains `[--chunk-size N]`. (g) `deploy/dist/env/s3.env.example`'s
  `WYRD_S3_ARGS=` line gains `--chunk-size 1048576`, with a comment in the file's existing
  style naming what it decides, the accepted range, and that the ceiling comes from the
  D-server transport. The M4 blueprint's `wyrd s3` invocation
  (`docs/design/architecture/m4-first-deployment-blueprint.md:1109-1114`), which that file
  says it mirrors, gains the same flag. The three `small-multi-node-fdb` gateway `command:`
  arrays (`deploy/small-multi-node-fdb/docker-compose.yml:387,404,421`) gain
  `"--chunk-size", "1048576"`, the default stated explicitly so the stack records its value
  without changing behaviour. (h) `xtask/tests/dist_templates.rs`'s
  `env_examples_name_every_load_bearing_flag` checks that the live `WYRD_S3_ARGS=` line
  carries `--chunk-size`. It must check that line, not the whole file: iteration 1's
  version passed with the flag deleted, because the new comment alone satisfied a
  whole-file `contains`.
  **/ out of scope:** any change to the gRPC transport: message-size limits, the D server's
  service construction, `crates/chunkstore-grpc`, the DST mounts. Raising the ceiling past
  today's transport is a separate follow-up issue on the Foundations milestone. The
  maintainer said in the 2026-10-01 Plan session that they will file it. It is not filed
  yet (no matching issue on getwyrd/wyrd as of 2026-10-01), so it has no number here, and
  nothing in this slice waits on it. Also out: collapsing the two `DEFAULT_CHUNK_SIZE` definitions
  (`lib.rs:51`, `cli.rs:64`) — both stay as they are; any change to `wyrd put` (it keeps
  accepting `0`); the default VALUE; per-object or per-bucket chunk size; `--chunk-size` on
  any other role; adding the chunk size to the `role started` event (#778 owns that event);
  the TiKV compose stack (`deploy/small-multi-node/`); removing `.max(1)` inside
  `with_chunk_size`; the chunk-map ceilings `MAX_ROOT_SEGMENTS` / `MAX_ROOT_VALUE_BYTES`
  (`crates/core/src/metadata.rs:544,574`; #739 tracks how a small chunk size shrinks the
  largest storable object); any peak-memory figure in docs or templates (two earlier rounds
  stated one, and both were wrong).

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (4 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage 42.6% — 23 of 54 instrumentable changed lines executed (below the 80% floor); 54 of 197 changed lines were
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 10 mutants tested in 64s: 2 caught, 8 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_738/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 17.26s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #738: honor `wyrd s3 --chunk-size N` on both chunk planes, preserve the 1 MiB default, refuse values outside `1..=16777216` bytes at startup, and record deployment defaults; no actionable implementation defect found.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The four binary-level acceptance legs make the settled inclusive ceiling, unchanged default, transport compatibility and startup refusal falsifiable (`brief.md:29`; `crates/server/src/lib.rs:53`). |
| C2 Reproduction (red pre-fix) | PASS | Independently stashing the tracked fix while retaining the new test produced four assertion failures after successful compilation: ignored size, accepted zero, sixteen remote chunks instead of one, and missing usage (`reviewer-red.log:15`; `reviewer-red.log:45`). |
| C3 Change | PASS | The patch stays within configuration, composition, tests and deployment documentation; all six arms receive the setting, and transport code remains unchanged (`crates/server/src/cli.rs:2211`; `crates/server/src/cli.rs:2405`; `docs/design/architecture/m4-first-deployment-blueprint.md:1117`). |
| C4 Verification (red→green) | PASS | Restoring the patch independently makes all four tests pass; all backend features compile. Full CI has frozen green evidence, with the local advisory-database lock limitation and advisory coverage failure recorded below (`reviewer-restored-green.log:11`; `reviewer-features.log:340`; `gate-logs/C4-ci.log:3826`). |
| C5 Causal adequacy | PASS | Chunk counts and byte-equal GETs expose ignored wiring and transport failure; the fix supplies the missing configuration path without a capability probe or symptom guard (`crates/server/tests/s3_chunk_size_flag.rs:463`; `crates/server/tests/s3_chunk_size_flag.rs:606`; `crates/server/src/cli.rs:2462`). |
| T1 Structure | PASS | Composition remains in the server crate behind existing trait seams; the shared constructor covers six combinations and the changed public callsite is updated (`crates/server/src/cli.rs:2405`; `crates/server/src/cli.rs:2451`; `crates/server/tests/s3_gateway_cluster.rs:153`). |
| T2 Shape | PASS | Startup parsing, range validation and optional composition have distinct responsibilities; documentation records the operator contract, and template assertions inspect the live assignment (`crates/server/src/cli.rs:2472`; `xtask/tests/dist_templates.rs:192`; `deploy/dist/env/s3.env.example:27`). |
| T3 Runtime | PASS | Real binary tests establish both chunk planes, unchanged default behavior and a 16 MiB round-trip through unchanged production gRPC limits; process cleanup and strict response framing protect the discriminator (`crates/server/tests/s3_chunk_size_flag.rs:166`; `crates/server/tests/s3_chunk_size_flag.rs:335`; `crates/server/tests/s3_chunk_size_flag.rs:595`). |
| T4 Contribution | N/A | Contribution artifacts are absent by design at Check; their substantive audit remains mandatory at publish, as the deferred gate explicitly states (`gate-logs/T4-contribution.log:10`). |
| T5 Judgment | PASS | Affected-path history and all closed PR file lists reveal no competing flag implementation; the only overlapping closed PR concerns segmented maps, and the rejected transport expansion is absent (`reviewer-prior-art.log:2`; `reviewer-prior-art.log:11`; `brief.md:199`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Confirm the configurable chunk unit and documented deployment default meet the intended workloads — functional and transport checks do not establish workload-specific throughput or memory suitability; the already-settled 16 MiB ceiling is not reopened (`brief.md:213`; `deploy/dist/env/s3.env.example:12`). |

The implementation meets the binding acceptance criteria. The remaining human decision is fitness to the intended workloads; the coverage shortfall and CI rerun limit below are evidence caveats, not reproduced patch defects. Source citations are relative to `$PDCA_TARGET`; input and evidence files are relative to this review directory. All eight supplied post-patch blobs match the target after restoration, with no stash remaining (`reviewer-target-integrity.log:1`).

- **Independent behavioral evidence:** `cargo test -p wyrd-server --test s3_chunk_size_flag --locked --offline` compiled and failed all four tests with the tracked fix stashed, then passed all four after `git stash pop`. Local chunk counts were 4 with 524288 and 2 with no flag; invalid values exited before the listen message; the 16 MiB remote object occupied one chunk and returned byte-equal data. The new parser/composition/public-entry unit tests passed 3/3, and deployment template tests passed 14/14 (`reviewer-red.log:45`; `reviewer-restored-green.log:11`; `reviewer-unit.log:279`; `reviewer-templates.log:31`).
- **Build and scanner evidence:** On the pinned Rust 1.96.0 toolchain, `cargo check -p wyrd-server --features tikv,fdb,etcd --tests --locked --offline` passed, including actual FoundationDB bindings; no compile claim rests solely on reading feature-gated code (`reviewer-features.log:315`; `reviewer-features.log:340`). The CI rerun passed spelling, documentation lint/render/link audit, repository guards, formatting, workspace clippy/build/tests and cargo-machete. It then stopped because cargo-deny's advisory database lock is read-only in this sandbox (`reviewer-ci.log:2845`). This is a host limitation. The frozen CI log records successful deny checks, conformance, guards and DST through its final success; those remaining stages were not independently rerun as a complete chain (`gate-logs/C4-ci.log:3205`; `gate-logs/C4-ci.log:3826`). The standalone statics scanner also passed (`reviewer-statics.log:5`).
- **Advisory gate limits:** C4-diff-cov remains **FAIL**, 23/54 instrumentable changed lines (42.6%), below its 80% threshold (`gate-logs/C4-diff-cov.log:359`). Its log measures the new integration test and xtask tests, not the new server unit tests; the binary-driving tests kill their child processes, limiting profile collection (`gate-logs/C4-diff-cov.log:14`; `gate-logs/C4-diff-cov.log:42`; `crates/server/tests/s3_chunk_size_flag.rs:168`). The behavioral rerun does not turn this coverage measurement green. C5-mutants reports **PASS** with 2 caught and 8 unviable mutants, not ten demonstrated detections (`gate-logs/C5-mutants.log:13`). The frozen batch review reports zero blocking findings (`gate-logs/T4-batch-review.log:10`). These instance-scoped wrappers were adjudicated from their captured logs, not run from another checkout. The host-tikv frozen log also records a successful feature clippy run (`gate-logs/host-tikv.log:209`).
- **Prior art and scope:** Queried merged history independently for all eight affected paths and checked complete file lists for all 19 closed, unmerged PRs. Only [PR #647](https://github.com/getwyrd/wyrd/pull/647) overlaps, on `crates/server/src/lib.rs`; it concerns segmented chunk maps and was closed for needing a smaller change. Nothing found reopens the flag design. The earlier 1 GiB ceiling and transport expansion are explicitly rejected in the brief and absent from this diff (`reviewer-prior-art.log:3`; `reviewer-prior-art.log:11`; `brief.md:199`). Deferred contribution validation is **N/A**, owed at publish, not a missing-human-evidence finding (`gate-logs/T4-contribution.log:10`).

### Advisory — adversary

# Adversarial review — issue 738 (`wyrd s3 --chunk-size`)

Bottom line: I could not break the red→green proof or the 16 MiB ceiling. I reproduced
both on my own scratch copy. Two judgment calls remain, both about inputs the role still
accepts when it should not, plus one claim in the brief that overstates what leg (C) guards.

## Findings

- NEEDS-HUMAN [human] — **`--chunk-size=N` and misspellings still start the role silently
  at 1 MiB.** `crates/server/src/cli.rs:2652-2662` (`ParsedArgs::parse`) stores
  `--chunk-size=524288` as an unknown flag named `chunk-size=524288`, and that flag takes
  the NEXT argument as its value. `cmd_s3` only looks up `chunk-size`
  (`cli.rs:2211`), so it sees nothing. Reproduced on the built patched binary:
  `wyrd s3 --access-key k --secret-key s --s3-listen 127.0.0.1:0 --data-dir D
  --metadata-backend redb --coordination-backend mem --chunk-size=524288 --region zone-a`
  printed `serving S3-compatible HTTP on …` and `SigV4 required (region us-east-1 …)`. So
  the role runs 1 MiB chunks AND the wrong region, because `--region` was eaten as the
  value. `--chunksize 524288` (typo) also starts at 1 MiB. The brief's Defect calls this
  silent acceptance "worse than missing", and its Invariant says an operator's setting is
  "honoured … or refused when the role starts". The patch closes only the exact
  `--chunk-size N` spelling. This is not a regression: every flag already behaves this way,
  e.g. `--region=zone-a`. A real fix means `cmd_s3` rejects unknown flags and stray
  positionals, which touches every s3 flag and falls outside scope items (a)–(h). Scope
  call: accept and file a follow-up, or widen this slice.

- NEEDS-HUMAN [human] — **The floor of `1` is accepted, but tiny chunk sizes are too slow
  to use.** `cli.rs:2487` accepts `1`. On a debug build with the local-FS plane, a role at
  `--chunk-size 1` returned 200 for PUTs of 4,096 and 81,000 bytes. A 200,000-byte PUT got
  no response within 120 s: the read timed out with `Resource temporarily unavailable`.
  Every byte becomes its own RS(6,3) chunk, which means 9 fragment files per byte. The
  brief kept the floor at 1 based only on the chunk-map ceilings it hands to #739. This is
  a different cost, and it hits the same invariant: the role starts, then a modest PUT does
  not finish. If #739 is meant to cover every small-chunk cost, treat this as settled.
  Otherwise a human should decide whether to raise the floor (for example to 4 KiB). Low
  priority. A release build will be faster, but the cost still grows with object size
  divided by chunk size.

- **The brief overstates leg (C).** The brief says "(C) is also the leg that goes red if
  the ceiling is ever set past what the transport carries." The test as written does not
  do that. Leg C hard-codes `--chunk-size 16777216` and a 16 MiB object
  (`crates/server/tests/s3_chunk_size_flag.rs:599,604`), so raising `MAX_CHUNK_SIZE`
  (`crates/server/src/lib.rs:70`) to, say, 32 MiB leaves C green. Only leg B's exact
  boundary (`s3_chunk_size_flag.rs:492`, where `16777217` must be refused) goes red.
  Someone who raises the ceiling and updates B's numbers gets no transport proof unless
  they also move C. The template test hard-codes a second copy of the ceiling
  (`xtask/tests/dist_templates.rs:212`). C IS a real transport probe: in my scratch copy,
  with the ceiling at 32 MiB and C moved to a 24 MiB chunk and object, the PUT returned
  `500 InternalError`. The brief forbids importing the constant into the test, so this is
  a wording point, not a code defect. A one-line note on `MAX_CHUNK_SIZE` saying "move leg C
  with this value" would close it.

- **The C4-diff-cov fail is a measurement artifact, not missing tests.** The coverage
  runner measured only `--test s3_chunk_size_flag` (see `gate-logs/C4-diff-cov.log`). That
  test runs the role in a child process, which never writes a coverage profile. The
  in-process unit tests that run `parse_s3_chunk_size`, `check_s3_chunk_size`,
  `compose_s3_gateway` and the `serve_s3_role` refusal (`cli.rs:3005-3084`) show up as
  UNSCORED. The MISS lines (`cli.rs:2480-2498` and others) are exactly what those tests
  run. I ran them: 3 passed.

## Attacks that failed

- **The red→green proof reproduces independently.** On a scratch copy of `$PDCA_TARGET`,
  the patched tree passes 4 of 4 tests. With production code reverted (`cli.rs`, `lib.rs`,
  `s3_gateway_cluster.rs`) and the new test kept, it fails 4 of 4, at the same lines and
  for the same reasons as `gate-logs/C4-verify.log`: A gets 2 chunks instead of 4, B's `0`
  starts serving, C gets 16 chunk directories instead of 1, and D has no `[--chunk-size N]`.
  The test drives the built binary and a production `DServer::bind`/`serve`
  (`crates/server/src/dserver.rs:729,933`, with `ChunkStoreServer::new` at `:1206`). No
  gRPC size override exists anywhere in the tree, so the test is not a parallel copy of
  production.
- **All six dispatch arms compile.** `cargo check -p wyrd-server --features fdb,etcd,tikv
  --tests` passes here (FDB headers present). CI never built the fdb arms; `host-tikv`
  covers only tikv and etcd. Every arm goes through `compose_s3_gateway` (`cli.rs:2451`),
  and `None` there is plain `Gateway::new`, the same as before.
- **A mixed fleet, or a restart with a different size, reads old objects correctly.** The
  gateway reads `self.chunk_size` only on its two write paths (`lib.rs:204`, `:344`). GET
  walks the object's stored chunk map.
- **The ceiling's arithmetic and transport both hold.** A 16 MiB chunk gives fragments of
  about 2.67 MiB, under tonic's 4 MiB default. A 16 MiB PUT and GET cross a real D server
  with byte-equal bodies (leg C, green here). A 24 MiB chunk fails with 500, as shown above.
- **Edge inputs are refused correctly.** A trailing `--chunk-size` with no value exits 2
  with "flag `--chunk-size` needs a value". `0x100`, `-5`, `""`, `1MiB` and 2^64 are
  refused with the flag named. `+524288` is accepted, which matches the brief's
  `str::parse::<usize>` contract, so I did not raise it. A repeated flag uses the last
  value, which is the documented convention (`deploy/dist/systemd/wyrd-s3.service:22`).
- **The refusal comes before bind.** `parse_s3_chunk_size` runs at `cli.rs:2211`, before
  the runtime starts and the listener binds (`cli.rs:2247`, `:2257`). `serve_s3_role`
  checks the range again (it is `pub`), and a unit test pins that check.
- **C5-mutants is not adjudicable.** The log reports "2 caught, 8 unviable" but does not
  name the mutants, so I cannot judge it either way.

### Advisory — code-review

No findings on either lens: no introduced correctness bugs or actionable reuse, simplification, or efficiency issues identified in this diff.

Reviewed startup validation and all six composition arms (`crates/server/src/cli.rs:2211`, `crates/server/src/cli.rs:2405`), the shared gateway helper (`crates/server/src/cli.rs:2451`), integration-test framing and transport assertions (`crates/server/tests/s3_chunk_size_flag.rs:292`, `crates/server/tests/s3_chunk_size_flag.rs:596`), and the live template argument check (`xtask/tests/dist_templates.rs:192`).

Validation evidence: frozen logs show CI and TiKV/etcd compilation passed, and all four new integration tests passed with the fix and failed without it. Diff coverage remains advisory at 42.6%; mutation testing reports two caught and eight unviable mutants. Tests were not rerun during this read-only review.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] Validation — fitness-to-purpose — Confirm the configurable chunk unit and documented deployment default meet the intended workloads — functional and transport checks do not establish workload-specific throughput or memory suitability; the already-settled 16 MiB ceiling is not reopened (`brief.md:213`; `deploy/dist/env/s3.env.example:12`).
- [x] **`--chunk-size=N` and misspellings still start the role silently at 1 MiB.** `crates/server/src/cli.rs:2652-2662` (`ParsedArgs::parse`) stores `--chunk-size=524288` as an unknown flag named `chunk-size=524288`, and that flag takes the NEXT argument as its value. `cmd_s3` only looks up `chunk-size` (`cli.rs:2211`), so it sees nothing. Reproduced on the built patched binary: `wyrd s3 --access-key k --secret-key s --s3-listen 127.0.0.1:0 --data-dir D --metadata-backend redb --coordination-backend mem --chunk-size=524288 --region zone-a` printed `serving S3-compatible HTTP on …` and `SigV4 required (region us-east-1 …)`. So the role runs 1 MiB chunks AND the wrong region, because `--region` was eaten as the value. `--chunksize 524288` (typo) also starts at 1 MiB. The brief's Defect calls this silent acceptance "worse than missing", and its Invariant says an operator's setting is "honoured … or refused when the role starts". The patch closes only the exact `--chunk-size N` spelling. This is not a regression: every flag already behaves this way, e.g. `--region=zone-a`. A real fix means `cmd_s3` rejects unknown flags and stray positionals, which touches every s3 flag and falls outside scope items (a)–(h). Scope call: accept and file a follow-up, or widen this slice.
- [x] **The floor of `1` is accepted, but tiny chunk sizes are too slow to use.** `cli.rs:2487` accepts `1`. On a debug build with the local-FS plane, a role at `--chunk-size 1` returned 200 for PUTs of 4,096 and 81,000 bytes. A 200,000-byte PUT got no response within 120 s: the read timed out with `Resource temporarily unavailable`. Every byte becomes its own RS(6,3) chunk, which means 9 fragment files per byte. The brief kept the floor at 1 based only on the chunk-map ceilings it hands to #739. This is a different cost, and it hits the same invariant: the role starts, then a modest PUT does not finish. If #739 is meant to cover every small-chunk cost, treat this as settled. Otherwise a human should decide whether to raise the floor (for example to 4 KiB). Low priority. A release build will be faster, but the cost still grows with object size divided by chunk size.
- [x] **The claimed binding sign-off is unsupported by the supplied record.** `brief.md:54` makes the 16 MiB ceiling “SETTLED (sign-off, 2026-10-01)” and “not Do’s to change”; `brief.md:299-312` attributes scope decisions to an earlier sign-off, and `brief.md:303-305` says the transport follow-up is already filed. Yet `notes.json:1` contains `"comments":[]`, its scope only requires “Reject a zero or absurd value at parse time rather than clamping silently,” and no `sources/` is supplied. This leaves a user-visible limit and its claimed approval unverifiable. Cite the actual sign-off record and follow-up issue ID in the brief, or identify these as proposed decisions for human adjudication instead of completed approvals.
- [x] C1 Spec — Reconcile the binding 1 GiB ceiling with transport capacity — an accepted 32 MiB configuration fails actual cluster PUTs, so choosing a smaller range or expanding transport support requires Plan re-entry (`brief.md:255`; `crates/server/src/lib.rs:62`; `reviewer-runtime.log:3`).
- [x] **The 1 GiB ceiling accepts chunk sizes that every cluster PUT above ~24 MiB then fails on.** `crates/server/src/lib.rs:62` (`MAX_CHUNK_SIZE = 1 << 30`) and `crates/server/src/cli.rs:2308` accept any value up to 1 GiB. With `--endpoints`, though, each RS(6,3) fragment (~`chunk_size/6`) travels as one unary gRPC message. The D server (`crates/server/src/dserver.rs:1206`, `ChunkStoreServer::new(service)`) and the gateway's client (`crates/chunkstore-grpc/src/client.rs:251`) both keep tonic's default 4 MiB receive limit. **Reproduced** on a scratch copy with the patched binary, one real `wyrd d-server`, and `wyrd s3 --endpoints http://127.0.0.1:<port> --chunk-size 33554432`: the role starts, logs `role started … chunk_size=33554432`, and a signed 32 MiB PUT returns **HTTP 500 InternalError**, `D server rpc error: … decoded message length too large: found 5592514 bytes, the limit is: 4194304 bytes`. The 1 MiB control PUT of the same object returns 200. Boundary sweep: chunk 16 MiB / object 16 MiB → 200; chunk 24 MiB / object 24 MiB → **500**; chunk 24 MiB / object 20 MiB → 200. So the failure stays hidden: the gateway boots, small objects work, and the first large object fails. This contradicts the Goal ("an invalid value is refused at parse time"). It also contradicts two operator-facing claims this diff adds, `deploy/dist/env/s3.env.example:16` and `docs/design/architecture/m4-first-deployment-blueprint.md:1119` ("1..=1073741824 is accepted"), in a template that always sets `--endpoints`. The brief's own rationale says the "largest sensible values are tens of MiB", which is exactly the range that breaks. The ceiling doc (`lib.rs:56-61`) also counts only PUT memory. The streaming GET buffers up to 4 chunks in its channel (`lib.rs:426`, `lib.rs:530`) plus the one being read, so at the ceiling that is ~5 GiB per in-flight GET, not ~1.5 GiB. The brief marked the number as SETTLED, so this is a human decision, not a builder fix. Options: lower the ceiling (to ≤ ~24 MiB, or only when `--endpoints` is set), raise both tonic limits (this widens scope into `dserver.rs` and `chunkstore-grpc`), or keep 1 GiB and document the transport limit. The T4-batch-review gate raised the same point three times; this run confirms it end to end.
- [x] The default-case criterion is not the claimed “byte-identical” check. The proposed test observes only a directory count (`brief.md:25-35`), while two executions cannot literally produce byte-identical persisted state: the gateway records `modified: Some(now_millis())` and mints chunk ids from a fresh random epoch (`crates/server/src/lib.rs:194-198`, `crates/server/src/lib.rs:245-279`). Replace “byte-identical” with the exact observable compatibility contract (for example, absent flag selects 1 MiB and yields the same chunk count), or specify a deterministic comparison that can actually prove the stronger claim.
- [x] The upper-bound acceptance policy is still undecided, so “above a stated ceiling” can be made green by choosing any ceiling. The brief proposes 1 GiB but explicitly leaves the exact value open for Do to change (`brief.md:210-217`, `brief.md:276-280`); this is load-bearing because the write path immediately allocates `Vec::with_capacity(chunk_size)` (`crates/core/src/write.rs:561-568`). Settle the value and boundary semantics in the brief, with an exact accepted/rejected pair, before calling ceiling refusal falsifiable.
- [x] The brief promotes an optional tracker suggestion into a required public-API/default refactor. The thread says only “Consider also collapsing the two `DEFAULT_CHUNK_SIZE` definitions,” but scope mandates it (`brief.md:99-100`, `brief.md:219-227`), which requires exposing the currently private server constant and changes the sibling `wyrd put` path that currently consumes the CLI-local constant (`crates/server/src/lib.rs:50-51`, `crates/server/src/cli.rs:555-559`). Either remove this second change or name and justify its extra API/callsite review surface as intentional scope.
- [x] The open question invites a second CLI behavior change outside issue #738: “Does `wyrd put` ... deserve the same zero/ceiling refusal? ... Do it only if it costs nothing” (`brief.md:285-287`). Today `cmd_put` merely parses the value and accepts zero (`crates/server/src/cli.rs:555-559`), while the tracker and title are specifically about the `s3` role. Delete this “while here” authorization or explicitly add `put` validation, its compatibility effect, and its own red/green assertions to scope.
- [x] The asserted build base is false in the supplied target. The brief says `cmd_s3` will “ALREADY” contain #736's `version` event field and `S3Config` field (`brief.md:83-88`), but target `main`'s role-started event has no `version` (`crates/server/src/cli.rs:2199-2206`) and its `S3Config` setup sets only `region` before metrics wiring (`crates/server/src/cli.rs:2377-2385`). A mere `Conflicts with: 736` declaration does not establish that prerequisite; revise the base/ordering claim or declare a resolvable stack/dependency on #736 so Do is not instructed to preserve code absent from its target.
- [x] **The 192 MiB message limit applies to every D server, even when the fleet runs 1 MiB chunks.** `crates/chunkstore-grpc/src/lib.rs:50` sets `MAX_MESSAGE_BYTES = 192 << 20`, and `crates/server/src/dserver.rs:1210-1214` applies it with no configuration. The D server admits `DEFAULT_MAX_CONCURRENT_REQUESTS = 64` requests at once (`dserver.rs:61`), and tonic holds each whole message in memory before decoding it. So the worst-case request memory per D server goes from 64 × 4 MiB = 256 MiB to 64 × 192 MiB = 12 GiB, on every deployment, whether or not any gateway sets `--chunk-size`. `docs/design/architecture/08-crosscutting-concepts.md:111` states the limit for one request but not this total. The brief's fixed 1 GiB ceiling forced this, and the brief never considered the transport. A human should pick one: accept the 12 GiB worst case, make the D-server limit a d-server setting that follows the fleet's chunk size, or lower the `s3` ceiling to fit a smaller transport limit.
- [x] **"Every accepted value is transportable to a D server" (`crates/server/src/lib.rs:62-64`) only checks message size, not time.** The D server cuts any request after `DEFAULT_REQUEST_TIMEOUT = 30 s` (`dserver.rs:72`, applied at `:1199`), and that includes receiving the request body. The custodian's per-request timeout is 10 s by default (`cli.rs:863`). At `--chunk-size 1073741824`, one PUT sends 9 × ~171 MiB ≈ 1.5 GiB. On a 1 Gb/s gateway link (~119 MiB/s), three PUTs at once take about 39 s to send, so every fragment is cut at 30 s. A custodian rebuild reads 6 × 171 MiB ≈ 1 GiB, about 8.6 s of transfer time against a 10 s limit. At the least, the doc claim should say "fits one gRPC message". Whether 1 GiB is still the right ceiling is a sign-off question.
- [x] **Do not count the C4-diff-cov FAIL (34.8%) for or against the fix.** Its MISS lines do not match the patched source at `$PDCA_TARGET`. `crates/server/src/cli.rs:2199-2200` and `:2333-2337` are comment lines, and `:2316-2317` are a `}` and a blank line. This is the same position mismatch flagged last round. Separately, the serving paths run in a child process that `Role::drop` kills with SIGKILL (`s3_chunk_size_flag.rs:67-72`), so llvm-cov never receives their profile. The partial-revert runs above are better evidence of reach than this row.

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
- By / date: Eduard Ralph / 2026-09-30

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 1 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
- Follow-up (general, not #738): `wyrd s3` (and other roles) silently accept unknown flags and `--flag=value` spellings; reject them at startup.
