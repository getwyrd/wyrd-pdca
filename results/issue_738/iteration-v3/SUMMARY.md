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
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (8 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: fail — diff coverage 40.0% — 10 of 25 instrumentable changed lines executed (below the 80% floor); 25 of 182 changed lines were
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 21 mutants tested in 2m: 10 caught, 11 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_738/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 20.67s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #738: expose `wyrd s3 --chunk-size N`, preserve the 1 MiB default, reject invalid values, and support the accepted range over gRPC; functional evidence passes, with coverage disposition and deployment fitness still owed.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The observable chunk counts and inclusive 1–1073741824 bounds are explicit; transport repairs are authorized by the carry-forward findings (`brief.md:26`, `brief.md:379`, `brief.md:386`). |
| C2 Reproduction (red pre-fix) | PASS | Independent stash-and-test run compiled and failed on the actual symptom: two chunks instead of four, plus accepted invalid inputs; six behavioral failures, two parser passes (`reviewer-red.log:41`, `reviewer-red.log:62`). |
| C3 Change | PASS | Operator configuration reaches all six compositions; templates retain the existing default, and the transport extension addresses the earlier accepted-range defect without changing `put` validation (`target/crates/server/src/cli.rs:2411`, `target/deploy/dist/env/s3.env.example:27`, `target/crates/chunkstore-grpc/src/server.rs:82`). |
| C4 Verification (red→green) | NEEDS-HUMAN | Decide whether to accept the reproduced behavioral red→green while correcting or waiving the advisory coverage result — frozen coverage is 40%, but some misses name comments and fresh profiles omit serving subprocesses killed before profile flush (`gate-logs/C4-diff-cov.log:1056`, `reviewer-coverage-analysis.log:1`, `reviewer-restored-green.log:16`). |
| C5 Causal adequacy | PASS | The missing configuration connection and both transport decode limits are addressed directly; no capability probe or symptom guard is introduced, and the real binary/D-server test demonstrates large-fragment PUT and GET (`target/crates/server/src/cli.rs:2202`, `target/crates/chunkstore-grpc/src/client.rs:256`, `target/crates/server/tests/s3_chunk_size_flag.rs:574`). |
| T1 Structure | PASS | Composition remains in the server crate, while the transport owns its shared server constructor; production, test, benchmark, and DST mounts use that constructor (`target/crates/chunkstore-grpc/src/server.rs:72`, `target/crates/server/src/dserver.rs:1209`, `target/crates/dst/tests/network.rs:234`). |
| T2 Shape | PASS | One shared default and explicit decimal/range validation preserve existing defaults; live template arguments and living architecture documentation cover the operator surface (`target/crates/server/src/lib.rs:54`, `target/crates/server/src/cli.rs:2301`, `target/xtask/tests/dist_templates.rs:191`, `target/docs/design/architecture/m4-first-deployment-blueprint.md:1117`). |
| T3 Runtime | PASS | Real loopback executions prove local chunk counts, refusal/startup boundaries, and a byte-identical 32 MiB PUT/GET through production D-server transport; the 1 GiB boundary is startup-only as specified (`reviewer-restored-green.log:10`, `target/crates/server/tests/s3_chunk_size_flag.rs:532`, `target/crates/server/tests/s3_chunk_size_flag.rs:602`). |
| T4 Contribution | N/A | Commit/PR artifacts are absent by design; their substantive audit must run at publish, so this deferred row needs no human clearance (`gate-logs/T4-contribution.log:10`); path-based prior-art checks are recorded in `reviewer-prior-art-summary.log:1`. |
| T5 Judgment | PASS | No new implementation defect was found under the standing rubric: earlier response-framing, backend isolation, server-construction, rollout, and memory-guidance concerns are addressed (`target/crates/server/tests/s3_chunk_size_flag.rs:95`, `target/crates/server/tests/s3_chunk_size_flag.rs:343`, `target/crates/server/tests/s3_chunk_size_flag.rs:403`, `target/docs/design/architecture/08-crosscutting-concepts.md:111`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Confirm that the deployment's chosen chunk size, memory/bandwidth budget, and fleet upgrade order are acceptable — evidence exercises 32 MiB on loopback, not near-ceiling production load, and larger fragments require every reader/writer to carry the new bound (`target/crates/server/src/lib.rs:55`, `target/docs/design/architecture/08-crosscutting-concepts.md:111`). |

No new implementation defect warrants a rebuild. The two decisions above concern evidence disposition and operational fitness, not a claimed functional failure.

- **Behavior is independently reproduced.** In the disposable `target/`, `git stash push` preserved the added integration test while reverting tracked changes. `cargo test --offline --locked -p wyrd-server --test s3_chunk_size_flag` produced the six expected pre-fix assertion failures, then all eight tests passed after `git stash pop` (`reviewer-red.log:62`; `reviewer-restored-green.log:16`). The 2 MiB object yields four chunks at 512 KiB and two without the flag. The gRPC leg writes and reads 32 MiB through `DServer::serve`. Reversal checking against `patch.diff` succeeds after restoration; no implementation edits remain beyond the supplied patch.
- **Build and rubric evidence support the change.** The independent CI run passed prose checks, formatting, workspace clippy/build/tests, and dependency-use scanning before the sandbox refused an exclusive lock on the shared advisory cache (`reviewer-ci.log:3002`). All three dependency scans then passed with an unchanged policy and an offline copy of the advisory database under this review root (`reviewer-deny.log:13`, `reviewer-deny.log:22`). Conformance, statics, DST clippy/tests, and the deployment guard passed separately (`reviewer-ci-remainder.log:1`; `reviewer-deploy-guard.log:13`). `cargo check --offline --locked -p wyrd-server --features fdb,tikv,etcd --tests` also passed, so all six selection arms were type-checked (`reviewer-features.log:373`). This is not a live FDB/TiKV/etcd integration claim; those services are outside the brief's binding test. The frozen full-CI log reports success (`gate-logs/C4-ci.log:3835`). Mutation and batch-review wrappers were not rerun: their frozen logs report 10 caught/11 unviable mutants, with no survivors, and zero blocking findings respectively (`gate-logs/C5-mutants.log:13`; `gate-logs/T4-batch-review.log:10`).
- **Coverage cannot substantiate the reported reach gaps.** The frozen 40% result remains an advisory failure, not an 80% pass. Its misses include current comments at `target/crates/server/src/cli.rs:2199` and `:2333`, and a blank line at `:2317`. A fresh `cargo llvm-cov test` passed all eight tests but recorded zero executions of the serving dispatch functions and only four parser calls, from the normally exiting refusal cases (`reviewer-coverage-analysis.log:1`). Successful server children are killed by `Role::drop` (`target/crates/server/tests/s3_chunk_size_flag.rs:71`), preventing normal exit-profile writes. These measurements cannot establish that the observed serving paths never ran; neither do they establish the 80% threshold. C4 asks for disposition of that measurement limitation, not an implementation rebuild.

The prior-art check used GitHub metadata by all 22 affected paths, inspecting up to five latest default-branch commits per path and all 19 closed/unmerged PRs with their changed-file lists. The only closed-work path overlap was #647, concerning segmented metadata maps rather than this role flag or transport limit (`reviewer-prior-art-summary.log:1`; raw results in `reviewer-path-history.json` and `reviewer-closed-pr-paths.json`). Source citations above refer only to the supplied `target/`; no other checkout was used for grounding. No additional `INTEGRATION.md` was present in that target.

### Advisory — adversary

# Adversarial review — issue 738 (`wyrd s3 --chunk-size`)

I tried to refute the fix and could not break its core claims. Two findings remain: one doc
threshold that is off by one step, and one design call on D-server memory that a human should
make. Everything else below is evidence I checked and found sound.

All runs were in a scratch copy of `$PDCA_TARGET`. I confirmed it was byte-identical to the
target's patched files before running anything.

## Findings

- NEEDS-HUMAN [impl] — **The "above ~24 MiB" upgrade threshold is wrong at exactly 24 MiB.**
  `deploy/dist/env/s3.env.example:17`, `docs/design/architecture/m4-first-deployment-blueprint.md:1120`
  and `docs/design/architecture/08-crosscutting-concepts.md:111` all tell operators that the
  old-binary 4 MiB gRPC limit only bites "above ~24 MiB". But `--chunk-size 25165824`
  (exactly 24 MiB, a natural round value) already fails. Each RS(6,3) data shard is then exactly
  4 MiB (`crates/core/src/erasure.rs:80-83`), and the 48-byte fragment header/trailer plus the
  protobuf envelope push the message past 4,194,304. **Reproduced:** in the scratch copy I
  removed both `max_*_message_size` setters (old-binary behaviour) and set the test's `BIG_CHUNK`
  (`crates/server/tests/s3_chunk_size_flag.rs:59`) to `24 << 20`. The PUT returned `500` at
  `s3_chunk_size_flag.rs:588`. With `23 << 20` the same test passed. An operator on a mixed fleet
  who reads "above ~24 MiB" and picks 24 MiB gets the 500s the note is meant to prevent. Fix:
  say "24 MiB or more" (or "above 23 MiB") in all three places.

- NEEDS-HUMAN [human] — **Every D server now accepts 192 MiB messages, whatever chunk size the
  fleet uses, and this goes beyond the brief's scope.** `MAX_MESSAGE_BYTES = 192 << 20`
  (`crates/chunkstore-grpc/src/lib.rs:54`) is applied to every D server through `into_server`
  (`crates/server/src/dserver.rs:1209`). With the global 64-request admission limit
  (`dserver.rs:61`, `:1180`), the worst-case request buffering per D server rises from about
  256 MiB to about 12 GiB, even in a deployment that keeps the 1 MiB default.
  `08-crosscutting-concepts.md:111` says that worst case is "reached only when clients actually
  send near-ceiling fragments". That is only true for resident memory. tonic 0.14.6 calls
  `self.buf.reserve(len)` as soon as it reads the 5-byte gRPC header
  (`~/.cargo/registry/src/*/tonic-0.14.6/src/codec/decode.rs:199`), so the declared length alone
  causes the reservation. On default Linux overcommit that is only virtual memory. On a host with
  strict overcommit (`vm.overcommit_memory=2`), 64 headers that each claim ~192 MiB use up about
  12 GiB of commit charge, and Rust aborts the process when an allocation fails.

  None of the transport work is in the brief's Scope (a)–(g). It adds public API to
  `wyrd-chunkstore-grpc` (`MAX_MESSAGE_BYTES`, `ChunkStoreService::into_server`,
  `crates/chunkstore-grpc/src/server.rs:82`), and the brief's "Impact & compatibility" names only
  two changes outside `cmd_s3`. It came from an earlier review round's P1, and it is a real fix
  for a real failure. Sign-off should still choose deliberately among three options:
  1. Accept a fixed 192 MiB bound fleet-wide.
  2. Make the bound configurable, or derive it from the configured chunk size.
  3. Revisit the "settled" 1 GiB `MAX_CHUNK_SIZE` (`crates/server/src/lib.rs`). The brief chose
     that ceiling without considering the transport.

  This is not a correctness bug in the patch.

## Attacks on the evidence (did not refute)

- **The gate's red leg does not prove the transport half, but the tests do.** In the frozen
  `gate-logs/C4-verify.log`, `a_chunk_past_the_4_mib_grpc_default_round_trips_through_a_d_server`
  went red at the chunk-count assertion (`left: 32, right: 1`, `s3_chunk_size_flag.rs:595`). That
  red comes from the flag being ignored, not from the 4 MiB limit. `round_trip.rs:119` is in a
  modified file and calls `into_server`, so it cannot compile on the reverted tree and was never
  red-checked by the gate. I filled that gap by hand with the full patch applied:
  - Removing both size setters makes the cluster PUT return `500`.
  - Removing only the client setters makes the cluster GET fail the strict parser ("declared
    content-length 33554432, but 0 body bytes arrived").
  - With the client setters removed, `round_trip.rs`'s test also fails with `OUT_OF_RANGE`
    ("found 6291509 bytes, the limit is: 4194304").

  Both transport tests do discriminate.
- **`check-gates.json`'s C4-verify `path_line` says "8 test(s) ran red". The log shows 6 failed
  and 2 passed.** The 2 that passed are the response-parser self-tests, which should pass on
  either tree. This is a counting error in the harness and does not change the verdict.
- **C4-diff-cov's 40% "fail" is not evidence against the fix.** Its MISS positions do not match
  this tree. `crates/server/src/cli.rs:2199-2200` and `:2331-2338` are comment and doc-comment
  lines. `:2312-2316` is `parse_s3_chunk_size`'s out-of-range return, which the binary-driven
  refusal tests visibly execute (they go red pre-fix and green post-fix). The likely cause is that
  the gate does not see coverage from the `wyrd` child process, or that its line positions are
  shifted. This is the same harness issue earlier rounds hit.
- **Green leg reproduced.** `cargo test -p wyrd-server --test s3_chunk_size_flag` passed 8/8.
- **The template test now goes red.** I deleted ` --chunk-size 1048576` from the live
  `WYRD_S3_ARGS=` line and `env_examples_name_every_load_bearing_flag` failed at
  `xtask/tests/dist_templates.rs:196`. The iteration-1 finding is fixed.

## Attacks on the fix (could not break)

- **Parsing** (`cli.rs:2295-2315`): `+1`, `-1`, empty, `1MiB`, `18446744073709551616`, leading
  space and non-ASCII digits are all refused. A trailing `--chunk-size` with no value already
  errors in `ParsedArgs::parse` (`cli.rs:2618-2620`). `--chunk-size --endpoints x` takes
  `--endpoints` as the value and refuses it.
- **Refusal happens before bind:** the parse is at `cli.rs:2202` and the listener binds at `:2248`.
- **All six arms apply `with_chunk_size`** (`cli.rs:2411-2455`). The tikv/etcd arms compile under
  the `host-tikv` gate. No gate compiled the fdb arms, so I ran
  `cargo check -p wyrd-server --features fdb,etcd` myself and it compiled cleanly.
- **No bare `ChunkStoreServer::new` / `ChunkStoreClient::new` is left** anywhere in the tree.
  The client is built only at `crates/chunkstore-grpc/src/client.rs:256` (bounded), and the
  custodian (`cli.rs:1603`) and the gateway fan-out both go through it.
- **The compile-time fragment-fit assertion** (`crates/server/src/lib.rs:75-83`) checks only
  `DEFAULT_DURABILITY`. That is enough: the `s3` role never calls `with_durability`, so every
  accepted `--chunk-size` is covered.
- **DST does not model the size bound at all:** madsim-tonic's generated setters are no-ops. The
  code says so at `crates/chunkstore-grpc/src/server.rs:79`. DST never modelled the old 4 MiB
  default either, so this is not a new fidelity gap from this diff.
- **Not raised, because already settled:** the chunk-map blow-up at tiny chunk sizes (deferred to
  #739 by the brief) and the per-PUT `Vec::with_capacity(chunk_size)` at the ceiling (the brief
  accepted this with the 1 GiB ceiling).
- **Informational, outside the brief's scope (f):** `deploy/small-multi-node/docker-compose.yml:393,408,423`
  (the TiKV stack) still omits `--chunk-size`, while the FDB stack now names it.

### Advisory — code-review

No findings in either lens: no patch-introduced correctness bugs or actionable reuse, simplification, or efficiency issues found.

Reviewed the diff against `$PDCA_TARGET` and the frozen gate evidence. CI, red→green regression verification, and mutation checks pass. The separate advisory diff-coverage result remains below its threshold (40%, 10/25); this review does not resolve that validation result. Gates were not rerun.

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C4 Verification (red→green) — Decide whether to accept the reproduced behavioral red→green while correcting or waiving the advisory coverage result — frozen coverage is 40%, but some misses name comments and fresh profiles omit serving subprocesses killed before profile flush (`gate-logs/C4-diff-cov.log:1056`, `reviewer-coverage-analysis.log:1`, `reviewer-restored-green.log:16`).
- [ ] Validation — fitness-to-purpose — Confirm that the deployment's chosen chunk size, memory/bandwidth budget, and fleet upgrade order are acceptable — evidence exercises 32 MiB on loopback, not near-ceiling production load, and larger fragments require every reader/writer to carry the new bound (`target/crates/server/src/lib.rs:55`, `target/docs/design/architecture/08-crosscutting-concepts.md:111`).
- [ ] **The "above ~24 MiB" upgrade threshold is wrong at exactly 24 MiB.** `deploy/dist/env/s3.env.example:17`, `docs/design/architecture/m4-first-deployment-blueprint.md:1120` and `docs/design/architecture/08-crosscutting-concepts.md:111` all tell operators that the old-binary 4 MiB gRPC limit only bites "above ~24 MiB". But `--chunk-size 25165824` (exactly 24 MiB, a natural round value) already fails. Each RS(6,3) data shard is then exactly 4 MiB (`crates/core/src/erasure.rs:80-83`), and the 48-byte fragment header/trailer plus the protobuf envelope push the message past 4,194,304. **Reproduced:** in the scratch copy I removed both `max_*_message_size` setters (old-binary behaviour) and set the test's `BIG_CHUNK` (`crates/server/tests/s3_chunk_size_flag.rs:59`) to `24 << 20`. The PUT returned `500` at `s3_chunk_size_flag.rs:588`. With `23 << 20` the same test passed. An operator on a mixed fleet who reads "above ~24 MiB" and picks 24 MiB gets the 500s the note is meant to prevent. Fix: say "24 MiB or more" (or "above 23 MiB") in all three places.
- [ ] **Every D server now accepts 192 MiB messages, whatever chunk size the fleet uses, and this goes beyond the brief's scope.** `MAX_MESSAGE_BYTES = 192 << 20` (`crates/chunkstore-grpc/src/lib.rs:54`) is applied to every D server through `into_server` (`crates/server/src/dserver.rs:1209`). With the global 64-request admission limit (`dserver.rs:61`, `:1180`), the worst-case request buffering per D server rises from about 256 MiB to about 12 GiB, even in a deployment that keeps the 1 MiB default. `08-crosscutting-concepts.md:111` says that worst case is "reached only when clients actually send near-ceiling fragments". That is only true for resident memory. tonic 0.14.6 calls `self.buf.reserve(len)` as soon as it reads the 5-byte gRPC header (`~/.cargo/registry/src/*/tonic-0.14.6/src/codec/decode.rs:199`), so the declared length alone causes the reservation. On default Linux overcommit that is only virtual memory. On a host with strict overcommit (`vm.overcommit_memory=2`), 64 headers that each claim ~192 MiB use up about 12 GiB of commit charge, and Rust aborts the process when an allocation fails.
- [ ] The default-case criterion is not the claimed “byte-identical” check. The proposed test observes only a directory count (`brief.md:25-35`), while two executions cannot literally produce byte-identical persisted state: the gateway records `modified: Some(now_millis())` and mints chunk ids from a fresh random epoch (`crates/server/src/lib.rs:194-198`, `crates/server/src/lib.rs:245-279`). Replace “byte-identical” with the exact observable compatibility contract (for example, absent flag selects 1 MiB and yields the same chunk count), or specify a deterministic comparison that can actually prove the stronger claim.
- [ ] The upper-bound acceptance policy is still undecided, so “above a stated ceiling” can be made green by choosing any ceiling. The brief proposes 1 GiB but explicitly leaves the exact value open for Do to change (`brief.md:210-217`, `brief.md:276-280`); this is load-bearing because the write path immediately allocates `Vec::with_capacity(chunk_size)` (`crates/core/src/write.rs:561-568`). Settle the value and boundary semantics in the brief, with an exact accepted/rejected pair, before calling ceiling refusal falsifiable.
- [ ] The brief promotes an optional tracker suggestion into a required public-API/default refactor. The thread says only “Consider also collapsing the two `DEFAULT_CHUNK_SIZE` definitions,” but scope mandates it (`brief.md:99-100`, `brief.md:219-227`), which requires exposing the currently private server constant and changes the sibling `wyrd put` path that currently consumes the CLI-local constant (`crates/server/src/lib.rs:50-51`, `crates/server/src/cli.rs:555-559`). Either remove this second change or name and justify its extra API/callsite review surface as intentional scope.
- [ ] The open question invites a second CLI behavior change outside issue #738: “Does `wyrd put` ... deserve the same zero/ceiling refusal? ... Do it only if it costs nothing” (`brief.md:285-287`). Today `cmd_put` merely parses the value and accepts zero (`crates/server/src/cli.rs:555-559`), while the tracker and title are specifically about the `s3` role. Delete this “while here” authorization or explicitly add `put` validation, its compatibility effect, and its own red/green assertions to scope.
- [ ] The asserted build base is false in the supplied target. The brief says `cmd_s3` will “ALREADY” contain #736's `version` event field and `S3Config` field (`brief.md:83-88`), but target `main`'s role-started event has no `version` (`crates/server/src/cli.rs:2199-2206`) and its `S3Config` setup sets only `region` before metrics wiring (`crates/server/src/cli.rs:2377-2385`). A mere `Conflicts with: 736` declaration does not establish that prerequisite; revise the base/ordering claim or declare a resolvable stack/dependency on #736 so Do is not instructed to preserve code absent from its target.
- [ ] size backstop — this slice is behaving oversized: patch touches 22 files (threshold 20). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.
- [ ] C1 Spec — Reconcile the binding 1 GiB ceiling with transport capacity — an accepted 32 MiB configuration fails actual cluster PUTs, so choosing a smaller range or expanding transport support requires Plan re-entry (`brief.md:255`; `crates/server/src/lib.rs:62`; `reviewer-runtime.log:3`).
- [ ] **The 1 GiB ceiling accepts chunk sizes that every cluster PUT above ~24 MiB then fails on.** `crates/server/src/lib.rs:62` (`MAX_CHUNK_SIZE = 1 << 30`) and `crates/server/src/cli.rs:2308` accept any value up to 1 GiB. With `--endpoints`, though, each RS(6,3) fragment (~`chunk_size/6`) travels as one unary gRPC message. The D server (`crates/server/src/dserver.rs:1206`, `ChunkStoreServer::new(service)`) and the gateway's client (`crates/chunkstore-grpc/src/client.rs:251`) both keep tonic's default 4 MiB receive limit. **Reproduced** on a scratch copy with the patched binary, one real `wyrd d-server`, and `wyrd s3 --endpoints http://127.0.0.1:<port> --chunk-size 33554432`: the role starts, logs `role started … chunk_size=33554432`, and a signed 32 MiB PUT returns **HTTP 500 InternalError**, `D server rpc error: … decoded message length too large: found 5592514 bytes, the limit is: 4194304 bytes`. The 1 MiB control PUT of the same object returns 200. Boundary sweep: chunk 16 MiB / object 16 MiB → 200; chunk 24 MiB / object 24 MiB → **500**; chunk 24 MiB / object 20 MiB → 200. So the failure stays hidden: the gateway boots, small objects work, and the first large object fails. This contradicts the Goal ("an invalid value is refused at parse time"). It also contradicts two operator-facing claims this diff adds, `deploy/dist/env/s3.env.example:16` and `docs/design/architecture/m4-first-deployment-blueprint.md:1119` ("1..=1073741824 is accepted"), in a template that always sets `--endpoints`. The brief's own rationale says the "largest sensible values are tens of MiB", which is exactly the range that breaks. The ceiling doc (`lib.rs:56-61`) also counts only PUT memory. The streaming GET buffers up to 4 chunks in its channel (`lib.rs:426`, `lib.rs:530`) plus the one being read, so at the ceiling that is ~5 GiB per in-flight GET, not ~1.5 GiB. The brief marked the number as SETTLED, so this is a human decision, not a builder fix. Options: lower the ceiling (to ≤ ~24 MiB, or only when `--endpoints` is set), raise both tonic limits (this widens scope into `dserver.rs` and `chunkstore-grpc`), or keep 1 GiB and document the transport limit. The T4-batch-review gate raised the same point three times; this run confirms it end to end.
- [ ] **The 192 MiB message limit applies to every D server, even when the fleet runs 1 MiB chunks.** `crates/chunkstore-grpc/src/lib.rs:50` sets `MAX_MESSAGE_BYTES = 192 << 20`, and `crates/server/src/dserver.rs:1210-1214` applies it with no configuration. The D server admits `DEFAULT_MAX_CONCURRENT_REQUESTS = 64` requests at once (`dserver.rs:61`), and tonic holds each whole message in memory before decoding it. So the worst-case request memory per D server goes from 64 × 4 MiB = 256 MiB to 64 × 192 MiB = 12 GiB, on every deployment, whether or not any gateway sets `--chunk-size`. `docs/design/architecture/08-crosscutting-concepts.md:111` states the limit for one request but not this total. The brief's fixed 1 GiB ceiling forced this, and the brief never considered the transport. A human should pick one: accept the 12 GiB worst case, make the D-server limit a d-server setting that follows the fleet's chunk size, or lower the `s3` ceiling to fit a smaller transport limit.
- [ ] **"Every accepted value is transportable to a D server" (`crates/server/src/lib.rs:62-64`) only checks message size, not time.** The D server cuts any request after `DEFAULT_REQUEST_TIMEOUT = 30 s` (`dserver.rs:72`, applied at `:1199`), and that includes receiving the request body. The custodian's per-request timeout is 10 s by default (`cli.rs:863`). At `--chunk-size 1073741824`, one PUT sends 9 × ~171 MiB ≈ 1.5 GiB. On a 1 Gb/s gateway link (~119 MiB/s), three PUTs at once take about 39 s to send, so every fragment is cut at 30 s. A custodian rebuild reads 6 × 171 MiB ≈ 1 GiB, about 8.6 s of transfer time against a 10 s limit. At the least, the doc claim should say "fits one gRPC message". Whether 1 GiB is still the right ceiling is a sign-off question.
- [ ] **Do not count the C4-diff-cov FAIL (34.8%) for or against the fix.** Its MISS lines do not match the patched source at `$PDCA_TARGET`. `crates/server/src/cli.rs:2199-2200` and `:2333-2337` are comment lines, and `:2316-2317` are a `}` and a blank line. This is the same position mismatch flagged last round. Separately, the serving paths run in a child process that `Role::drop` kills with SIGKILL (`s3_chunk_size_flag.rs:67-72`), so llvm-cov never receives their profile. The partial-revert runs above are better evidence of reach than this row.

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Plan
- Iteration delta (if iterating): Ceiling (1 GiB) was set without considering the transport. Settle a max chunk size that fits the D-server gRPC limit (or make the limit configurable/derived) before building. Do not ship a fixed 192 MiB limit fleet-wide; it raises worst-case memory to ~12 GiB per D server. Split: (1) `--chunk-size` flag with a ceiling inside today's limit; (2) transport limit change as its own issue. Fix brief defects: drop "byte-identical", drop `wyrd put` validation and the forced DEFAULT_CHUNK_SIZE merge unless intended, resolve the #736 base claim. Doc threshold is ">= 24 MiB", not "above ~24 MiB".
- By / date: Eduard Ralph / 2026-09-30

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 5 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
