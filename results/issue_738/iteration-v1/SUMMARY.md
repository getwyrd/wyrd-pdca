# Result — issue 738 / s3-role-chunk-size-flag

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: The `s3` role cannot be told what chunk size to use, so every deployed
  gateway chunks at the 1 MiB default regardless of workload, hardware, or the D-server
  fan-out it fronts. This is a wiring gap, not a design position — the seam exists and the
  sibling role already exposes it. Verified on `main` at `65ca4fd`:
  `Gateway::with_chunk_size` exists at `crates/server/src/lib.rs:145`, doc-commented
  "mainly so tests can force multi-chunk objects"; `wyrd put --chunk-size N` is parsed at
  `crates/server/src/cli.rs:555-560`; and `cmd_s3` (`cli.rs:2113`) parses `--s3-listen`,
  `--data-dir`, `--region`, `--access-key`, `--secret-key`, `--metadata-backend`,
  `--coordination-backend`, `--endpoints`, `--otlp-endpoint` and no chunk size, so none of
  the six `Gateway::new` arms in `serve_s3_dispatch` (`cli.rs:2314,2320,2327,2334,2340,2347`)
  ever calls `with_chunk_size` and `DEFAULT_CHUNK_SIZE` (`lib.rs:51`, `1 << 20`) always
  wins. Two independent definitions of that default exist and can drift
  (`lib.rs:51` and `cli.rs:64`, both `1 << 20`).
- Success criterion: BINDING (demonstrable by C4-verify at Check, no container, no
  cluster): a new integration test drives the **built `wyrd` binary** as an `s3` role over
  a loopback listener with `--chunk-size N`, PUTs an object of size `4N` through it, and
  asserts the local chunk store under `<data-dir>/chunks` holds exactly the number of chunk
  directories `N` implies (`FsChunkStore` keys each chunk as a `<32-hex>` directory of
  `<index>.frag` fragments — `crates/chunkstore-fs/src/lib.rs:64,78`). The same test asserts
  the DEFAULT case, stated as the exact observable it is: the identical object PUT with NO
  `--chunk-size` produces the chunk count 1 MiB implies — i.e. absence selects
  `DEFAULT_CHUNK_SIZE`, unchanged from today. ("Byte-identical persisted state" is NOT the
  claim and would be false: the gateway records `modified: Some(now_millis())` and mints
  chunk ids from a fresh random epoch — `crates/server/src/lib.rs:194-198,245-279` — so no
  two runs agree byte for byte. The chunk count is the property the flag actually changes,
  and it is what is asserted.)
  Plus the two refusal boundaries, as an exact accepted/rejected pair each:
  * `--chunk-size 0` exits non-zero, before binding a listener, naming the flag on stderr;
    `--chunk-size 1` is ACCEPTED (the role starts and prints its listen line);
  * `--chunk-size 1073741824` (1 GiB, the ceiling, inclusive) is ACCEPTED and
    `--chunk-size 1073741825` is REJECTED, non-zero, naming the flag. The ceiling value is
    SETTLED in this brief — see Design — and is not Do's to choose. The acceptance legs
    only start the role; they never PUT, so no `Vec::with_capacity(chunk_size)`
    (`crates/core/src/write.rs:561-568`) is ever allocated at the ceiling.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: (a) parse `--chunk-size` in `cmd_s3`, defaulting to the existing value so an
  invocation without it is byte-identical to today; (b) thread it through `serve_s3_role` →
  `serve_s3_dispatch` to `Gateway::with_chunk_size` at each of the six composition arms; (c)
  refuse an invalid value at parse time — non-numeric, zero, and above a stated ceiling —
  rather than relying on the silent `.max(1)` clamp; (d) update `with_chunk_size`'s doc
  comment: it is no longer a test-only affordance; (e) add the flag to the `wyrd s3` usage
  line (`cli.rs:492`); (f) add it to `deploy/dist/env/s3.env.example` and the three
  `small-multi-node-fdb` gateway commands, and to the flag list
  `xtask/tests/dist_templates.rs:174-187` asserts; (g) collapse the two `DEFAULT_CHUNK_SIZE`
  definitions (`lib.rs:51`, `cli.rs:64`) into one so the role and the CLI cannot drift —
  **deliberate extra scope beyond the tracker's "consider", and here is its full review
  surface**, so it is judged rather than smuggled: `crates/server/src/lib.rs:51` becomes
  `pub` (an ADDITIVE change to `wyrd-server`'s public API — a new `pub const`, no existing
  item's signature or value changes), `crates/server/src/cli.rs:64` is deleted, and the ONE
  existing consumer of the CLI-local copy, `cmd_put` (`cli.rs:555-560`), reads the shared
  constant instead. Both consts are private today and both are `1 << 20` (verified on
  `main` at `65ca4fd`), so the collapse is value-preserving; the two `assert_eq!(…, 1 << 20)`
  unit tests (`lib.rs:775`, `cli.rs:2856`) collapse to one. It is IN scope because this
  slice's own compatibility claim — "absence selects `DEFAULT_CHUNK_SIZE`" — is a claim
  about a constant that currently exists twice and can drift; that is the difference between
  a claim that is checkable and one that is merely true today. If sign-off would rather see
  it split out, it is cleanly separable (drop (g); the criterion still holds against
  `cli.rs:64`'s copy) — say so at §9 rather than after the fact.
  **/ out of scope:** per-object or per-bucket chunk size (this is one process-level
  default, matching how every other role knob works); any change to the default VALUE;
  `--chunk-size` on any other role; **any change to `wyrd put`'s validation** — `cmd_put`
  accepts `0` today (`cli.rs:555-560`) and continues to; (g) changes which constant it reads
  and nothing else about its behaviour; the chunk-map budget constants
  `MAX_ROOT_VALUE_BYTES` / `MAX_ROOT_SEGMENTS` (that is #739, and proposal 0017
  §Dependencies says it must not be built until an architecture question about per-gateway
  ceilings is settled); removing the `.max(1)` floor inside `with_chunk_size` (see Design).

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: new-feature
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (3 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage 0.0% — 0 of 16 instrumentable changed lines executed (below the 80% floor); 16 of 99 changed lines were in
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 13 mutants tested in 78s: 7 caught, 6 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_738/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 14.23s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review of #738: expose `wyrd s3 --chunk-size N` with a shared 1 MiB default, validation and deployment configuration; the wiring passes red→green, but accepted larger chunks fail on the cluster transport.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | NEEDS-HUMAN | Reconcile the binding 1 GiB ceiling with transport capacity — an accepted 32 MiB configuration fails actual cluster PUTs, so choosing a smaller range or expanding transport support requires Plan re-entry (`brief.md:255`; `crates/server/src/lib.rs:62`; `reviewer-runtime.log:3`). |
| C2 Reproduction (red pre-fix) | PASS | Independently stashing the tracked fix while retaining the added binary test produced three assertion failures: two chunks instead of four, and both invalid boundaries started serving (`reviewer-red.log:14`, `reviewer-red.log:25`; `crates/server/tests/s3_chunk_size_flag.rs:261`). |
| C3 Change | PASS | The authorized wiring and compatibility changes are present across all six composition arms; `put` retains its validation and default value, and deployment defaults remain 1048576 (`crates/server/src/cli.rs:554`, `crates/server/src/cli.rs:2408`; `deploy/dist/env/s3.env.example:23`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Decide how to discharge the advisory coverage evidence — independent red→green succeeds, but the frozen 0/16 report has mismatched source positions and fresh profiling omits proven serving paths; the 80% claim is unresolved (`reviewer-restored-green.log:10`; `gate-logs/C4-diff-cov.log:366`; `crates/server/tests/s3_chunk_size_flag.rs:53`). |
| C5 Causal adequacy | PASS | Reading the previously ignored flag and applying it at composition removes the wiring cause; no capability probe or symptom guard is introduced, and all seven viable diff mutants were caught (`crates/server/src/cli.rs:2202`, `crates/server/src/cli.rs:2412`; `reviewer-mutants.log:16`). |
| T1 Structure | PASS | Configuration remains in the existing composition root and builder seam, preserving backend dependency direction; all feature combinations compile with the real toolchain (`crates/server/src/cli.rs:2394`; `reviewer-features.log:373`). |
| T2 Shape | PASS | The public signature's other caller, usage, deployment examples and living architecture are updated together; formatting, spelling and documentation checks pass (`crates/server/tests/s3_gateway_cluster.rs:153`; `crates/server/src/cli.rs:491`; `docs/design/architecture/m4-first-deployment-blueprint.md:1117`; `reviewer-ci.log:2`). |
| T3 Runtime | FAIL | Valid `--chunk-size 33554432` turns a 32 MiB cluster PUT into HTTP 500: its 5,592,514-byte fragment RPC exceeds the unchanged 4,194,304-byte receive limit (`crates/server/src/lib.rs:62`; `crates/server/src/dserver.rs:1206`; `pdca-reviewer-738-runtime/cluster-32m.stderr.log:5`). |
| T4 Contribution | FAIL | One independently confirmed transport defect from the required deep review remains unresolved; its three frozen entries are duplicates, while the contribution-artifact audit is N/A until publish (`gate-logs/T4-batch-review.log:10`; `gate-logs/T4-contribution.log:10`). |
| T5 Judgment | PASS | The tests make genuine binary and filesystem observations for the stated local contract; review separately exercised the omitted transport case and checked merged history plus rejected work by every affected path (`crates/server/tests/s3_chunk_size_flag.rs:242`; `reviewer-runtime.log:3`; `reviewer-prior-art.log:1`, `reviewer-prior-art.log:113`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept the operator-facing range and resource tradeoffs only after deciding the cluster transport policy — local success and startup-only ceiling tests do not establish useful fleet operation across the advertised range (`crates/server/tests/s3_chunk_size_flag.rs:300`; `deploy/dist/env/s3.env.example:12`; `reviewer-runtime.log:3`). |

Source citations are relative to `$PDCA_TARGET` (`target/`); brief, gate and reviewer evidence paths are relative to this review directory. The target was readable and matched the patch. The stash was restored, the tracked diff compared byte-for-byte equal, and the added test retained blob `fe4c306790760b536968e4dc9eead1648cf70aa5`. No production changes were made.

**The confirmed defect is a range/transport mismatch, not a missing composition arm.** The CLI accepts 32 MiB under its new ceiling (`crates/server/src/cli.rs:2308`) and hands it to the gRPC composition (`crates/server/src/cli.rs:2358`). RS(6,3) divides a full chunk into six aligned data shards (`crates/core/src/erasure.rs:80`), and each stored fragment travels in one RPC (`crates/chunkstore-grpc/src/client.rs:285`). Neither the real D-server service nor client configures larger decoding limits (`crates/server/src/dserver.rs:1206`; `crates/chunkstore-grpc/src/client.rs:251`). The live error confirms the effective limit directly.

The same 33,554,432-byte signed PUT produced these results through actual built `wyrd` processes:

| Storage path | Chunk size | Observed result |
|---|---:|---|
| Loopback D-server over gRPC | 16 MiB | HTTP 200 |
| Local filesystem | 32 MiB | HTTP 200 |
| Same loopback D-server over gRPC | 32 MiB | HTTP 500; decoded message exceeds 4 MiB |

`reviewer-runtime.log:1` records the controls; `pdca-reviewer-738-runtime/cluster-32m.stderr.log:5` records the transport error. `reviewer-runtime.py` contains the executable reproduction and kills/reaps its children. The single real D-server suffices to reproduce this per-message limit; this is not a claim about fleet durability. The frozen batch review's three reports describe this same defect, not three independent defects. Because the brief explicitly settles the ceiling, silently lowering it would violate the plan. The human decision is whether to revise that contract or authorize compatible transport changes and corresponding boundary tests.

**The regression proof is real; coverage accounting is not yet reliable.** Independent `cargo test -p wyrd-server --test s3_chunk_size_flag` passed initially, failed all three assertions with the tracked fix stashed, then passed all three after restoration (`reviewer-red.log:36`; `reviewer-restored-green.log:10`). This agrees with the frozen per-fix evidence (`gate-logs/C4-verify.log:10`, `gate-logs/C4-verify.log:51`). Independent diff mutation testing reproduced 13 mutants: seven caught, six unviable, no viable survivors (`reviewer-mutants.log:16`; frozen `gate-logs/C5-mutants.log:13`). Unviable mutants are not claimed as tested behavior.

The frozen coverage log reports 0/16, but includes comment/doc-comment positions such as `crates/server/src/cli.rs:2199` and `crates/server/src/cli.rs:2332` among its executable misses (`gate-logs/C4-diff-cov.log:367`, `gate-logs/C4-diff-cov.log:375`). An independent `cargo llvm-cov test -p wyrd-server --test s3_chunk_size_flag --json` passed all three tests and produced `reviewer-coverage.json`: the parser at line 2308 has two recorded entries, while serving code at line 2412 has zero despite successful PUTs. This is consistent with the successful long-running children being forcibly killed (`crates/server/tests/s3_chunk_size_flag.rs:53`), losing normal-exit profile collection; rejected children exit normally. The exact cause of the frozen source-map mismatch is not established. Neither its zero nor the fresh incomplete profiles prove those serving lines never executed. The advisory coverage row remains unresolved rather than becoming a fabricated C4 patch defect.

**The remaining gate evidence supports the scoped implementation, with one local host limitation.** Independent `cargo xtask ci` passed spelling, docs lint/render, repository guards, formatting, workspace clippy/build/tests and cargo-machete. It then stopped at cargo-deny because `/home/eddie/.cargo/advisory-dbs/db.lock` is read-only (`reviewer-ci.log:3006`). This is a sandbox limitation, not a dependency or patch failure. The frozen CI log explicitly shows all three deny checks passing, conformance/statics/DST checks running, and final success (`gate-logs/C4-ci.log:3218`, `gate-logs/C4-ci.log:3839`). Conformance and statics also passed independent direct reruns. I do not claim a complete independent CI green.

Independent pinned-toolchain `cargo clippy -p wyrd-server --features fdb,tikv,etcd --tests` passed, including FoundationDB compilation (`reviewer-features.log:330`, `reviewer-features.log:373`); frozen `host-tikv` also records successful real compilation (`gate-logs/host-tikv.log:209`). Thus the changed feature arms are supported by compilation, not solely code inspection. No live FDB/etcd deployment was exercised; the brief's binding criterion explicitly requires only local redb/mem/FS (`brief.md:154`). The separate gRPC investigation used the real transport and revealed the failure above.

Prior art was checked through GitHub merged history for all eight affected paths and by intersecting all 19 closed-unmerged PRs with those paths (`reviewer-prior-art.log:1`). The only rejected-work overlap was [PR #647](https://github.com/getwyrd/wyrd/pull/647), whose inspected diff concerns segmented chunk maps and adds no S3 role flag or default/ceiling change. No prior implementation of this role knob was found. No `INTEGRATION.md` was supplied or present in the target; the supplied standing rubric and root `AGENTS.md` were applied.

T4-contribution: **N/A** — `pr-description.md` is intentionally drafted after Check; its substantive audit must rerun at publish (`gate-logs/T4-contribution.log:10`). This deferral is neither a missing-evidence finding nor a human clearance item. This report is advisory and does not change deterministic gate results or accept the patch.

### Advisory — adversary

# Adversarial review — issue 738 (`wyrd s3 --chunk-size`)

Verdict: the red→green evidence holds up. The ceiling the brief settled does not: it
accepts values the cluster setup cannot store. I reproduced that with the built binary.

## Findings

- NEEDS-HUMAN [human] — **The 1 GiB ceiling accepts chunk sizes that every cluster PUT above ~24 MiB then fails on.** `crates/server/src/lib.rs:62` (`MAX_CHUNK_SIZE = 1 << 30`) and `crates/server/src/cli.rs:2308` accept any value up to 1 GiB. With `--endpoints`, though, each RS(6,3) fragment (~`chunk_size/6`) travels as one unary gRPC message. The D server (`crates/server/src/dserver.rs:1206`, `ChunkStoreServer::new(service)`) and the gateway's client (`crates/chunkstore-grpc/src/client.rs:251`) both keep tonic's default 4 MiB receive limit. **Reproduced** on a scratch copy with the patched binary, one real `wyrd d-server`, and `wyrd s3 --endpoints http://127.0.0.1:<port> --chunk-size 33554432`: the role starts, logs `role started … chunk_size=33554432`, and a signed 32 MiB PUT returns **HTTP 500 InternalError**, `D server rpc error: … decoded message length too large: found 5592514 bytes, the limit is: 4194304 bytes`. The 1 MiB control PUT of the same object returns 200. Boundary sweep: chunk 16 MiB / object 16 MiB → 200; chunk 24 MiB / object 24 MiB → **500**; chunk 24 MiB / object 20 MiB → 200. So the failure stays hidden: the gateway boots, small objects work, and the first large object fails. This contradicts the Goal ("an invalid value is refused at parse time"). It also contradicts two operator-facing claims this diff adds, `deploy/dist/env/s3.env.example:16` and `docs/design/architecture/m4-first-deployment-blueprint.md:1119` ("1..=1073741824 is accepted"), in a template that always sets `--endpoints`. The brief's own rationale says the "largest sensible values are tens of MiB", which is exactly the range that breaks. The ceiling doc (`lib.rs:56-61`) also counts only PUT memory. The streaming GET buffers up to 4 chunks in its channel (`lib.rs:426`, `lib.rs:530`) plus the one being read, so at the ceiling that is ~5 GiB per in-flight GET, not ~1.5 GiB. The brief marked the number as SETTLED, so this is a human decision, not a builder fix. Options: lower the ceiling (to ≤ ~24 MiB, or only when `--endpoints` is set), raise both tonic limits (this widens scope into `dserver.rs` and `chunkstore-grpc`), or keep 1 GiB and document the transport limit. The T4-batch-review gate raised the same point three times; this run confirms it end to end.

- NEEDS-HUMAN [impl] — **The new template assertion cannot go red.** `xtask/tests/dist_templates.rs:180` adds `"--chunk-size"`, but `:187` checks `s.contains(flag)` across the whole file. The comment this diff adds at `deploy/dist/env/s3.env.example:12` (`#   --chunk-size   bytes per chunk: …`) already satisfies it. **Reproduced:** I deleted ` --chunk-size 1048576` from the live `WYRD_S3_ARGS=` line (`s3.env.example:23`), leaving only the comment, and `cargo test -p xtask --test dist_templates env_examples_name_every_load_bearing_flag` still passes. Fix: for the new entry, find the line that starts with `WYRD_S3_ARGS=` and assert that it contains `--chunk-size`. The older `--s3-listen` / `--region` entries have the same weakness, but that predates this diff and is not in scope here.

- Minor, optional: the unit-test doc at `crates/server/src/cli.rs:2957` says "a sign … refused", but only `-1` is tested. `parse_s3_chunk_size(Some("+65536"))` returns `Ok(65536)` because `usize::from_str` accepts a leading `+`. I checked this with rustc: `"+65536" -> Ok(65536)`, `"+0" -> Ok(0)`, and the zero check still refuses the second one. This does no harm: the value is the same, it matches how `cmd_put` parses, and the rubric's sign rule targets RFC grammars. Only the comment overstates what is tested.

- Note: the C4-diff-cov result of **0.0% is a measuring problem, not missing coverage.** The integration test runs `wyrd` as a child process and ends it with SIGKILL (`crates/server/tests/s3_chunk_size_flag.rs:53`), so the child never writes its coverage profile. The `cli.rs` unit test was also outside the measured target. Judge execution by the behaviour-based red→green below, not by this row.

## Refutation attempts that failed

- **Red→green, re-run myself** on a scratch copy. With the patch: 3/3 green. With `cli.rs` and `lib.rs` reverted and the test kept: 3/3 red, each for the right reason (2 chunks instead of 4; `--chunk-size 0` and `--chunk-size 1073741825` both reach the listen line). The test uses `env!("CARGO_BIN_EXE_wyrd")`, the real production binary, not a copy.
- **Default leg is a real check:** a 4N = 2 MiB object gives 2 chunks at 1 MiB (`s3_chunk_size_flag.rs:271`), so any change to the default shows up.
- **The "names the flag" check can't pass by accident:** `usage()` (which now mentions `--chunk-size`) only prints for an unknown or missing subcommand (`cli.rs:417-425`), never on a `cmd_s3` error.
- **Gateways with different chunk sizes are fine, as the brief says:** `self.chunk_size` is read only at the write sites `lib.rs:194` and `lib.rs:334`. The read path and custodian never use it.
- **Child cleanup:** `Role` is declared after its tempdir, so it drops first and kills the child (`s3_chunk_size_flag.rs:51-56`). Every wait has a time limit.
- **Refused before bind:** the flag is parsed (`cli.rs:2202`) before backend selection and before `TcpListener::bind`.
- The four feature-gated arms are only compile-checked (tikv/etcd by the host-tikv gate). The edits are mechanical and match the arms that do compile. Not raised.

### Advisory — code-review

- NEEDS-HUMAN [impl] — **P1: Accepted chunk sizes exceed the cluster transport limit.** `crates/server/src/cli.rs:2308` accepts `--chunk-size 33554432`, and the new wiring applies it to the gRPC composition. A full 32 MiB chunk under RS(6,3) produces fragments over 5 MiB (`crates/core/src/erasure.rs:80`), but `ChunkStoreServer::new(service)` retains tonic's default 4 MiB decoding limit (`crates/server/src/dserver.rs:1206`). Thus an accepted configuration makes sufficiently large PUTs fail. The client also retains its default decoding limit (`crates/chunkstore-grpc/src/client.rs:251`), so increasing only the server limit leaves GETs broken. Make the transport support the brief's advertised range, accounting for fragment/protobuf overhead, and cover a PUT/GET above this boundary through the gRPC composition. This confirms the transport finding in the frozen batch-review log.

- NEEDS-HUMAN [impl] — **P2: Pin the new integration test's local backends.** `crates/server/tests/s3_chunk_size_flag.rs:82` inherits the process environment without passing `--metadata-backend redb --coordination-backend mem`. The role honors `WYRD_METADATA_BACKEND` and `WYRD_COORDINATION_BACKEND` (`crates/server/src/cli.rs:286`, `crates/server/src/cli.rs:376`), so a developer environment selecting TiKV/FDB/etcd makes this supposedly self-contained test fail on the default build; feature-enabled builds can instead contact external services and write metadata outside the temporary directory. Pass the two backend flags explicitly so the chunk-size test always exercises its intended local composition.

No additional reuse, simplification, or efficiency findings. Review used the target source and frozen gate evidence; no builds were rerun and no target files were changed.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C1 Spec — Reconcile the binding 1 GiB ceiling with transport capacity — an accepted 32 MiB configuration fails actual cluster PUTs, so choosing a smaller range or expanding transport support requires Plan re-entry (`brief.md:255`; `crates/server/src/lib.rs:62`; `reviewer-runtime.log:3`).
- [ ] C4 Verification (red→green) — Decide how to discharge the advisory coverage evidence — independent red→green succeeds, but the frozen 0/16 report has mismatched source positions and fresh profiling omits proven serving paths; the 80% claim is unresolved (`reviewer-restored-green.log:10`; `gate-logs/C4-diff-cov.log:366`; `crates/server/tests/s3_chunk_size_flag.rs:53`).
- [ ] Validation — fitness-to-purpose — Accept the operator-facing range and resource tradeoffs only after deciding the cluster transport policy — local success and startup-only ceiling tests do not establish useful fleet operation across the advertised range (`crates/server/tests/s3_chunk_size_flag.rs:300`; `deploy/dist/env/s3.env.example:12`; `reviewer-runtime.log:3`).
- [ ] **The 1 GiB ceiling accepts chunk sizes that every cluster PUT above ~24 MiB then fails on.** `crates/server/src/lib.rs:62` (`MAX_CHUNK_SIZE = 1 << 30`) and `crates/server/src/cli.rs:2308` accept any value up to 1 GiB. With `--endpoints`, though, each RS(6,3) fragment (~`chunk_size/6`) travels as one unary gRPC message. The D server (`crates/server/src/dserver.rs:1206`, `ChunkStoreServer::new(service)`) and the gateway's client (`crates/chunkstore-grpc/src/client.rs:251`) both keep tonic's default 4 MiB receive limit. **Reproduced** on a scratch copy with the patched binary, one real `wyrd d-server`, and `wyrd s3 --endpoints http://127.0.0.1:<port> --chunk-size 33554432`: the role starts, logs `role started … chunk_size=33554432`, and a signed 32 MiB PUT returns **HTTP 500 InternalError**, `D server rpc error: … decoded message length too large: found 5592514 bytes, the limit is: 4194304 bytes`. The 1 MiB control PUT of the same object returns 200. Boundary sweep: chunk 16 MiB / object 16 MiB → 200; chunk 24 MiB / object 24 MiB → **500**; chunk 24 MiB / object 20 MiB → 200. So the failure stays hidden: the gateway boots, small objects work, and the first large object fails. This contradicts the Goal ("an invalid value is refused at parse time"). It also contradicts two operator-facing claims this diff adds, `deploy/dist/env/s3.env.example:16` and `docs/design/architecture/m4-first-deployment-blueprint.md:1119` ("1..=1073741824 is accepted"), in a template that always sets `--endpoints`. The brief's own rationale says the "largest sensible values are tens of MiB", which is exactly the range that breaks. The ceiling doc (`lib.rs:56-61`) also counts only PUT memory. The streaming GET buffers up to 4 chunks in its channel (`lib.rs:426`, `lib.rs:530`) plus the one being read, so at the ceiling that is ~5 GiB per in-flight GET, not ~1.5 GiB. The brief marked the number as SETTLED, so this is a human decision, not a builder fix. Options: lower the ceiling (to ≤ ~24 MiB, or only when `--endpoints` is set), raise both tonic limits (this widens scope into `dserver.rs` and `chunkstore-grpc`), or keep 1 GiB and document the transport limit. The T4-batch-review gate raised the same point three times; this run confirms it end to end.
- [ ] **The new template assertion cannot go red.** `xtask/tests/dist_templates.rs:180` adds `"--chunk-size"`, but `:187` checks `s.contains(flag)` across the whole file. The comment this diff adds at `deploy/dist/env/s3.env.example:12` (`#   --chunk-size   bytes per chunk: …`) already satisfies it. **Reproduced:** I deleted ` --chunk-size 1048576` from the live `WYRD_S3_ARGS=` line (`s3.env.example:23`), leaving only the comment, and `cargo test -p xtask --test dist_templates env_examples_name_every_load_bearing_flag` still passes. Fix: for the new entry, find the line that starts with `WYRD_S3_ARGS=` and assert that it contains `--chunk-size`. The older `--s3-listen` / `--region` entries have the same weakness, but that predates this diff and is not in scope here.
- [ ] **P1: Accepted chunk sizes exceed the cluster transport limit.** `crates/server/src/cli.rs:2308` accepts `--chunk-size 33554432`, and the new wiring applies it to the gRPC composition. A full 32 MiB chunk under RS(6,3) produces fragments over 5 MiB (`crates/core/src/erasure.rs:80`), but `ChunkStoreServer::new(service)` retains tonic's default 4 MiB decoding limit (`crates/server/src/dserver.rs:1206`). Thus an accepted configuration makes sufficiently large PUTs fail. The client also retains its default decoding limit (`crates/chunkstore-grpc/src/client.rs:251`), so increasing only the server limit leaves GETs broken. Make the transport support the brief's advertised range, accounting for fragment/protobuf overhead, and cover a PUT/GET above this boundary through the gRPC composition. This confirms the transport finding in the frozen batch-review log.
- [ ] **P2: Pin the new integration test's local backends.** `crates/server/tests/s3_chunk_size_flag.rs:82` inherits the process environment without passing `--metadata-backend redb --coordination-backend mem`. The role honors `WYRD_METADATA_BACKEND` and `WYRD_COORDINATION_BACKEND` (`crates/server/src/cli.rs:286`, `crates/server/src/cli.rs:376`), so a developer environment selecting TiKV/FDB/etcd makes this supposedly self-contained test fail on the default build; feature-enabled builds can instead contact external services and write metadata outside the temporary directory. Pass the two backend flags explicitly so the chunk-size test always exercises its intended local composition.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_738/review-b
- [ ] The default-case criterion is not the claimed “byte-identical” check. The proposed test observes only a directory count (`brief.md:25-35`), while two executions cannot literally produce byte-identical persisted state: the gateway records `modified: Some(now_millis())` and mints chunk ids from a fresh random epoch (`crates/server/src/lib.rs:194-198`, `crates/server/src/lib.rs:245-279`). Replace “byte-identical” with the exact observable compatibility contract (for example, absent flag selects 1 MiB and yields the same chunk count), or specify a deterministic comparison that can actually prove the stronger claim.
- [ ] The upper-bound acceptance policy is still undecided, so “above a stated ceiling” can be made green by choosing any ceiling. The brief proposes 1 GiB but explicitly leaves the exact value open for Do to change (`brief.md:210-217`, `brief.md:276-280`); this is load-bearing because the write path immediately allocates `Vec::with_capacity(chunk_size)` (`crates/core/src/write.rs:561-568`). Settle the value and boundary semantics in the brief, with an exact accepted/rejected pair, before calling ceiling refusal falsifiable.
- [ ] The brief promotes an optional tracker suggestion into a required public-API/default refactor. The thread says only “Consider also collapsing the two `DEFAULT_CHUNK_SIZE` definitions,” but scope mandates it (`brief.md:99-100`, `brief.md:219-227`), which requires exposing the currently private server constant and changes the sibling `wyrd put` path that currently consumes the CLI-local constant (`crates/server/src/lib.rs:50-51`, `crates/server/src/cli.rs:555-559`). Either remove this second change or name and justify its extra API/callsite review surface as intentional scope.
- [ ] The open question invites a second CLI behavior change outside issue #738: “Does `wyrd put` ... deserve the same zero/ceiling refusal? ... Do it only if it costs nothing” (`brief.md:285-287`). Today `cmd_put` merely parses the value and accepts zero (`crates/server/src/cli.rs:555-559`), while the tracker and title are specifically about the `s3` role. Delete this “while here” authorization or explicitly add `put` validation, its compatibility effect, and its own red/green assertions to scope.
- [ ] The asserted build base is false in the supplied target. The brief says `cmd_s3` will “ALREADY” contain #736's `version` event field and `S3Config` field (`brief.md:83-88`), but target `main`'s role-started event has no `version` (`crates/server/src/cli.rs:2199-2206`) and its `S3Config` setup sets only `region` before metrics wiring (`crates/server/src/cli.rs:2377-2385`). A mere `Conflicts with: 736` declaration does not establish that prerequisite; revise the base/ordering claim or declare a resolvable stack/dependency on #736 so Do is not instructed to preserve code absent from its target.

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
- Iteration delta (if iterating): Auto-iterate (round 1): rebuilding for the implementation-level findings — C4 Verification (red→green) — Decide how to discharge the advisory coverage evidence — independent red→green succeeds, but the frozen 0/16 report has mismatched source positions and fresh profiling omits proven serving paths; the 80% claim is unresolved (`reviewer-restored-green.log:10`; `gate-logs/C4-diff-cov.log:366`; `crates/server/tests/s3_chunk_size_flag.rs:53`).; **The new template assertion cannot go red.** `xtask/tests/dist_templates.rs:180` adds `"--chunk-size"`, but `:187` checks `s.contains(flag)` across the whole file. The comment this diff adds at `deploy/dist/env/s3.env.example:12` (`# --chunk-size bytes per chunk: …`) already satisfies it. **Reproduced:** I deleted ` --chunk-size 1048576` from the live `WYRD_S3_ARGS=` line (`s3.env.example:23`), leaving only the comment, and `cargo test -p xtask --test dist_templates env_examples_name_every_load_bearing_flag` still passes. Fix: for the new entry, find the line that starts with `WYRD_S3_ARGS=` and assert that it contains `--chunk-size`. The older `--s3-listen` / `--region` entries have the same weakness, but that predates this diff and is not in scope here.; **P1: Accepted chunk sizes exceed the cluster transport limit.** `crates/server/src/cli.rs:2308` accepts `--chunk-size 33554432`, and the new wiring applies it to the gRPC composition. A full 32 MiB chunk under RS(6,3) produces fragments over 5 MiB (`crates/core/src/erasure.rs:80`), but `ChunkStoreServer::new(service)` retains tonic's default 4 MiB decoding limit (`crates/server/src/dserver.rs:1206`). Thus an accepted configuration makes sufficiently large PUTs fail. The client also retains its default decoding limit (`crates/chunkstore-grpc/src/client.rs:251`), so increasing only the server limit leaves GETs broken. Make the transport support the brief's advertised range, accounting for fragment/protobuf overhead, and cover a PUT/GET above this boundary through the gRPC composition. This confirms the transport finding in the frozen batch-review log.; **P2: Pin the new integration test's local backends.** `crates/server/tests/s3_chunk_size_flag.rs:82` inherits the process environment without passing `--metadata-backend redb --coordination-backend mem`. The role honors `WYRD_METADATA_BACKEND` and `WYRD_COORDINATION_BACKEND` (`crates/server/src/cli.rs:286`, `crates/server/src/cli.rs:376`), so a developer environment selecting TiKV/FDB/etcd makes this supposedly self-contained test fail on the default build; feature-enabled builds can instead contact external services and write metadata outside the temporary directory. Pass the two backend flags explicitly so the chunk-size test always exercises its intended local composition.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 3 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_738/review-b. 7 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- By / date: auto-iterate / 2026-09-30

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 5 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
