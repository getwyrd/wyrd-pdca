# Result — issue 768 / last-modified-never-decreases

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: A file's stored `modified` strictly increases across overwrites, whichever gateway of
  an active/active fleet performs them and however wrong that gateway's clock was; and no S3
  response shows a `Last-Modified` later than its own `Date`. Today each gateway stamps `modified`
  from its own clock (`now_millis`, `crates/server/src/lib.rs:813-823`) at both PUT paths (`:254`
  buffered, `:405` streaming). `commit_written` (`:269`) hands the plan to
  `write::commit_overwrite` (`crates/core/src/write.rs:314-335`), which passes
  `plan.object_meta` through, and both core overwrite commits store the publisher's stamp as
  given (`crates/core/src/metadata.rs:2218`, `:2297`). Nothing compares the new stamp with the
  one it replaces. The fleet sits behind a round-robin balancer with no affinity
  (`docs/design/architecture/m4-first-deployment-blueprint.md` §B.1; three gateways in
  `deploy/small-multi-node-fdb/docker-compose.yml`; active/active per #477). There, a gateway
  whose clock runs 2 s fast followed by an accurate one makes `Last-Modified` go backwards while
  the content moves forwards, and `aws s3 sync` skips the newer file when the bytes have the
  same size: silent divergence. The S3 gateway also renders the stored time as given
  (`crates/gateway-s3/src/lib.rs:1024` listing, `:2489` and `:2597` header) and evaluates date
  preconditions against it (`:2774-2793`), so a stamp from a fast clock is served as a future
  `Last-Modified`, which RFC 9110 §8.8.2.1 forbids.
- Success criterion: **(A) The rule holds, with exact values.** A NEW test file drives the
  real `wyrd_server::Gateway` through the pre-existing `ObjectGateway` API, both public core
  overwrite commits, and the real S3 surface. Legs (1)–(6) read the **stored** `modified`
  (`ObjectGateway::head_object` and core), not a rendered one.
  (1) **Streaming PUT after a fast clock.** Seed a file whose committed `modified` is **one hour
  ahead** of the test's wall clock and sits **mid-second** (`.500`). Overwrite it through
  `put_object_streaming` with **different bytes of the same size**. `head_object` then reports
  **exactly** `seed + 1` and the new ETag. The mid-second seed makes `seed + 1`, `seed + 1000`
  and "the next whole second" three different answers, so a whole-second step, a `+1 s` step or
  a jump to `u64::MAX` fails.
  (2) The same through the buffered `Gateway::put_object`: also exactly `seed + 1`.
  (3) An overwrite of a file whose prior stamp is **an hour in the past** records the current
  clock (between wall-clock readings taken just before and just after the call), on both PUT
  paths.
  (4) A **create** records the current clock, as today, on both PUT paths.
  (5) **The rule's exact values, on both core overwrite APIs.** The server legs cannot choose
  the publisher's clock reading, so this leg calls core directly, with an explicit publisher
  stamp, through **both** public overwrite commits: `metadata::commit_chunk_map_superseding`
  (`crates/core/src/metadata.rs:2199`) and `metadata::commit_chunk_map_superseding_leased`
  (`:2268`, the one `write::commit_overwrite` calls; an empty pending set needs no leases,
  `:2424-2425`). Peer: `crates/core/tests/mutation_regressions.rs:446-460` overwrites a seeded
  record this way. With `T = 1_700_000_000_250` and `DAY = 86_400_000`, each API commits exactly:
  - prior `Some(T)`, publisher `Some(T)` (equal: two writes in one millisecond) → `Some(T + 1)`;
  - prior `Some(T + 500)` (ahead, in the same second) → `Some(T + 501)`;
  - prior `Some(T + 5_000)` (ahead) → `Some(T + 5_001)`;
  - prior `Some(T - 1)` (one millisecond behind) → `Some(T)`;
  - prior `Some(T - 5_000)` (behind) → `Some(T)`;
  - prior `None` (a legacy record) → `Some(T)`;
  - publisher `None` over prior `None` (neither has a time) → commits, `None`;
  - **refused:** publisher `None` over prior `Some(T + 5_000)`, and over prior `Some(u64::MAX)`,
    returns the new typed error (not a panic, and not `CommitOutcome::Conflict`, which callers
    retry), and the stored record is byte-for-byte unchanged (same `modified`, same `version`);
  - **no cap, no heal:** prior `Some(T + 2 * DAY)` (two days ahead: a badly wrong clock) →
    `Some(T + 2 * DAY + 1)`, ordered after it, not replaced by `T`;
  - **no heal at the top of the range:** prior `Some(u64::MAX)`, publisher `Some(T)` →
    `Some(u64::MAX)`;
  - **saturation:** prior `Some(u64::MAX)` with publisher `Some(u64::MAX - 1)`, prior
    `Some(u64::MAX - 1)` with publisher `Some(u64::MAX - 1)`, and both `Some(u64::MAX)` →
    `Some(u64::MAX)` each: no panic, no wrap to 0;
  - **chain, repeated overwrites:** three overwrites in a row, each with publisher `Some(T)`,
    over prior `Some(T)` → `T + 1`, `T + 2`, `T + 3`;
  - **chain, a far-ahead prior is carried, pinned as intended:** over prior `Some(T + 2 * DAY)`,
    two overwrites with publisher `Some(T)` → `T + 2 * DAY + 1`, then `T + 2 * DAY + 2`;
  - **chain, no reset through an unstamped overwrite:** over prior `Some(T + 5_000)`, an
    overwrite with publisher `None` is refused and leaves the record unchanged; a further
    overwrite with publisher `Some(T)` then commits `Some(T + 5_001)`, ordered after the
    unchanged prior. An implementation that commits the unstamped overwrite (resetting the
    order), carries `T + 5_000` into it, or stamps it with an invented time, fails.
  (6) **No heal through the gateway.** Seed a prior **two days ahead** of the wall clock
  (mid-second); an overwrite records **exactly** `seed + 1`, on both PUT paths. An
  implementation that replaces a far-ahead prior with the clock (the iteration-3 one-day cap)
  fails here.
  (7) **Over the real S3 wire, a future stored time is shown as the response time, and a past one
  as stored.** Start `wyrd_gateway_s3::S3Gateway` over an in-process `Gateway` on a loopback
  listener, with the bucket marker and two files seeded before `Gateway::new`: `ahead`, stored
  **one hour ahead** of the wall clock (mid-second), and `behind`, stored **one hour behind** it
  (mid-second; call its whole second `P`). Send signed raw HTTP/1.1 requests whose response
  headers the test reads. For each response, `before` is the test's wall clock read just before
  sending (whole seconds, rounded down), and `Date` is that **same** response's header (assert it
  is present: RFC 9110 §6.6.1 requires it of an origin server with a clock, and hyper sends it;
  if it is missing, sending it is part of this slice). A shown time is **in the window** when
  `before ≤ shown ≤ Date`, in whole seconds. For a future stored time that is the exact value
  `min(stored, clock)` takes, to within the request's own duration, so the seed, a fixed date,
  or a time from another response all fail. Check, both **before** and **after** a same-size
  overwrite PUT of `ahead`:
  - HEAD `ahead`: `Last-Modified` is in the window;
  - GET `ahead` (`200`): `Last-Modified` is in the window;
  - ListObjectsV2: the `ahead` row's `<LastModified>` is in that listing's window, and the `behind`
    row's is exactly `P` (in the listing's format);
  - HEAD `ahead` with `If-Unmodified-Since` set to **59 minutes after the test's start** (still
    before the seed) → `200`, not `412`;
  - GET `ahead` with `If-Modified-Since` set to the same instant → `304`, not `200`, and that
    `304`'s `Last-Modified` is in its own window (the separate `304` renderer);
  - HEAD `behind`: `Last-Modified` is exactly `P`, the stored time, not the clock. GET `behind`
    with `If-Modified-Since: P` → `304` whose `Last-Modified` is `P`; with `P` minus one second →
    `200`.
  After the overwrite, HEAD `ahead` also shows the new ETag, and a `Last-Modified` that is not
  earlier than the one shown before it and is **earlier than the seed**. That last check pins the
  exception leg (B) documents: a mirror that copied `ahead` while a gateway showed the seed holds
  a later time than this overwrite shows.
  (8) **Over the real S3 wire, one healthy gateway: no visible lead.** Create a file, then three
  back-to-back same-size overwrite PUTs. After each, HEAD's `Last-Modified` and the listing's
  `<LastModified>` each lie between the test's wall clock read just before that PUT (whole
  seconds, rounded down) and their own response's `Date`, and each is **not earlier** than what
  the same surface showed before.
  Every network await is bounded by a timeout, and the server task is aborted when the test ends.
  **(B) The rule is stated where readers look.** In the living architecture doc's
  object-metadata paragraph (`docs/design/architecture/05-building-block-view.md:197-200`), in
  the doc comments of the `modified` fields (`crates/core/src/metadata.rs:1598-1600` on
  `ObjectMeta`, `:1648-1653` on `InodeRecord`), in the doc comment of the core rule itself, and
  beside the S3 gateway's shown-time helper. They say:
  - across overwrites that each record a time, a file's stored `modified` strictly increases:
    such an overwrite records `max(publishing clock, prior modified + 1 ms)`;
  - **what S3 shows:** the shown time is the earlier of the stored time and the serving
    gateway's clock, in whole seconds, used for the `Last-Modified` header, the listing's
    `<LastModified>` and the `If-Modified-Since` / `If-Unmodified-Since` checks. It is never later
    than the response's `Date` (RFC 9110 §8.8.2.1). An overwrite inside the same second as the
    time it replaced can show the **same** one. Which gateway answers can move the shown time by
    the clock skew between gateways, which #902's clock sync keeps at milliseconds. A client that
    must see every change compares `ETag`, the content-identity token;
  - **a stored time in the future** (left by a badly wrong clock) is shown as the serving
    gateway's current time, which moves forward with each request. Until real time passes the
    stored time, mirrors re-copy the file on each pass, and `If-Unmodified-Since` with an earlier
    shown time fails. A mirror that has only been shown such times misses no change. **The
    exception:** a mirror that already holds a time later than real time, copied while a gateway
    showed the future stored time (a gateway without this rule, or one whose own clock was that
    far ahead), sees each later overwrite at an earlier time than its copy. If it copies only
    newer files, it skips them until an overwrite lands after real time has passed the time it
    holds. No shown time can fix that without being later than `Date`, so re-sync such a mirror
    by `ETag` or with a full copy;
  - **no cap and no automatic correction** of the stored time: a prior stamped by a badly wrong
    clock is still ordered after, and the overwrite logs a warning when the prior is more than a
    day ahead. The reason: no data must be lost inadvertently to time drift. A backwards step can
    make a mirror that copies only newer files skip the new content;
  - **an overwrite with no time is refused** when the version it replaces has one (a typed
    error; nothing is written), so the stored order cannot reset. Over a version with no time (a
    legacy record) it is allowed and records none. Both gateway PUT paths always stamp, and any
    new path that publishes over a prior generation (the multipart Complete flip, #658) must
    stamp and must call the rule, or its overwrites of stamped files fail;
  - repairs still preserve `modified` unchanged;
  - **in the architecture doc only:** this rule is within ADR-0047's file-level meaning of
    `modified` (`docs/design/adr/0047-object-metadata-model.md:36`). The `+1 ms` step and the
    shown-time cap are implementation detail below that decision, not a change to it. This
    sentence is there so a reader comparing ADR-0047's wording ("content-publication time") with
    the code finds the reasoning without a new ADR.

  No ADR is added or edited.
- Repo + branch target: getwyrd/wyrd @ main
- Scope: (1) making every overwrite publication store a strictly later `modified` (`+1 ms`
  past the prior when the clock is not already later), with no cap and no automatic correction;
  refusing an overwrite that carries no time over a version that has one; a warning log when the
  superseded stamp is more than a day ahead; (2) on the S3 surface, showing `min(stored, serving gateway's clock)` in
  the `Last-Modified` header, the listing's `<LastModified>`, and the `If-Modified-Since` /
  `If-Unmodified-Since` evaluation; (3) stating both in the architecture doc, the `modified`
  field docs, the core rule's doc and the gateway's shown-time helper / out of scope: any new or
  edited ADR (the rule is within ADR-0047's file-level intent); a sub-second listing (#767 keeps
  whole seconds); changing the **stored** time when it is shown capped; correcting a far-ahead
  stamp (an explicit operator repair, a separate change); host clock sync (#902); a clock-skew
  bound or NTP requirement that refuses writes (option 2); the pre-epoch clock that stamps
  `modified: Some(0)` on a **create** (`now_millis`'s `.unwrap_or(0)`,
  `crates/server/src/lib.rs:822`); the multipart Complete flip (#658 builds it and must stamp and
  call the shared rule this slice adds; it does not inherit it); lease stamps (the same
  `now_millis` feeds them, and they keep their meaning); the advisory waiver itself (a separate
  change to `deny-all-features.toml`).

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — (no gate configured)
- C3 Change: none — patch.diff
- C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance): pass — xtask ci: all checks passed
- C4 per-fix red->green: this patch's test red pre-fix, green post-fix: pass — run-verify.sh: PASS — red without the fix, green with it (9 test(s) ran red).
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 99.3% — 599 of 603 instrumentable changed lines executed (floor 80%); 603 of 1169 changed lines were instr
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 39 mutants tested in 2m: 35 caught, 4 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): fail — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_768/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.40s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

The #768 fix orders stored modification times across overwrites and caps S3-visible times at each response’s `Date`; independent red→green checks pass, with one test-timeout correction and two human decisions outstanding.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The maintainer specifies exact millisecond ordering, saturation, unstamped refusal and wire-time bounds; the carry-forward changes remain within that scope (`brief.md:53`, `brief.md:277`, `brief.md:624`). |
| C2 Reproduction (red pre-fix) | PASS | With production changes stashed and the new test retained, the base compiled and failed five tests on timestamp assertions, including 24 core cases; this is a behavioral red (`pdca-reviewer-768-evidence/red.log:87`, `target/crates/server/tests/last_modified_monotonic.rs:251`). |
| C3 Change | PASS | The five affected files address publication ordering, S3 presentation, regression evidence and living documentation without changing persisted fields, dependencies or an ADR (`patch.diff:1`, `target/docs/design/architecture/05-building-block-view.md:205`). |
| C4 Verification (red→green) | PASS | Restoring the exact patch makes all nine discriminator tests pass; frozen CI evidence is green, and the independent advisory scans pass after relocating their cache (`pdca-reviewer-768-evidence/green-restored.log:21`, `gate-logs/C4-ci.log:4243`, `pdca-reviewer-768-evidence/deny-all-features-rerun.log:4`). |
| C5 Causal adequacy | PASS | Ordering is enforced against the generation protected by the publication CAS, and response times share one clock reading; this removes the demonstrated causes rather than hiding them behind a capability probe (`target/crates/core/src/metadata.rs:2401`, `target/crates/gateway-s3/src/lib.rs:2451`). |
| T1 Structure | PASS | Both public overwrite committers use the shared rule, while the S3 layer retains its gateway seam and owns presentation-time conversion (`target/crates/core/src/metadata.rs:2250`, `target/crates/core/src/metadata.rs:2451`, `target/crates/gateway-s3/src/lib.rs:3084`). |
| T2 Shape | PASS | Optional-field encoding and repair semantics remain intact, and typed refusal occurs before publication; literal warning boundaries and independent wire-clock bounds address the carry-forward test weaknesses (`target/crates/core/src/metadata.rs:1679`, `target/crates/core/src/metadata.rs:5853`, `target/crates/server/tests/last_modified_monotonic.rs:925`). |
| T3 Runtime | NEEDS-HUMAN | Confirm the #902 clock-sync requirement before fleet rollout — it remains open and no synchronized multi-host topology was exercised, so the evidence establishes seeded-prior and single-host behavior, not bounded fleet skew (`brief.md:246`, `target/deploy/dist/systemd/wyrd-s3.service:13`, `pdca-reviewer-768-evidence/prior-art-conclusions.log:10`). |
| T4 Contribution | FAIL | The required review disposition is incomplete because the timeout finding below remains grounded and unresolved; the affected-path prior-art check is complete, and contribution-artifact auditing is deferred by design (`gate-logs/T4-batch-review.log:10`, `target/AGENTS.md:206`, `gate-logs/T4-contribution.log:10`). |
| T5 Judgment | NEEDS-HUMAN [impl] | Bound the stored-value tests’ external awaits — filesystem and gateway calls in legs (1)–(6) bypass the existing deadline helper and can leave the test suite waiting indefinitely (`target/crates/server/tests/last_modified_monotonic.rs:183`, `target/crates/server/tests/last_modified_monotonic.rs:213`, `target/AGENTS.md:181`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept the ETag/full-copy recovery requirement for mirrors that already hold future mtimes — a newer version can appear older than such a mirror, as the wire regression demonstrates (`brief.md:564`, `target/docs/design/architecture/05-building-block-view.md:232`, `target/crates/server/tests/last_modified_monotonic.rs:1112`). |

**One implementation finding remains: bound the stored-value test operations.** The new `seed_file` directly awaits fragment writes at `target/crates/server/tests/last_modified_monotonic.rs:183`; buffered and streaming PUTs, HEAD and GET likewise have no enclosing timeout at lines 213, 224, 232 and 260. The stored-value test entry points invoke those helpers directly. The underlying fragment path joins filesystem operations without a timeout (`target/crates/core/src/write.rs:253`, `target/crates/chunkstore-fs/src/lib.rs:243`). The `bounded` helper exists at `target/crates/server/tests/last_modified_monotonic.rs:583`, but only the S3 setup/exchanges use it. Apply that bound to the stored-value operations or their enclosing test futures. This independently confirms the frozen review’s single finding; no tracked deferral was supplied. It concerns test robustness, not the timestamp algorithm.

**The timestamp behavior is independently reproduced.** In the disposable `target`, `cargo test --offline -p wyrd-server --test last_modified_monotonic` passed 9/9, then failed 5/9 after stashing the tracked production changes while retaining the new test, then passed 9/9 after `git stash pop`. Failures covered both PUT paths, the two-day-ahead prior, both core APIs and the real S3 wire. The restored source hashes match the original patch, and `git apply --reverse --check patch.diff` succeeds (`pdca-reviewer-768-evidence/stash-restore.log:18`, `pdca-reviewer-768-evidence/target-manifest.log:6`). The target is readable and contains the advisory waivers required by the brief; no stale-target caveat applies (`target/deny-all-features.toml:160`, `target/deny-all-features.toml:167`).

**Independent CI checks support the frozen evidence, with a resolved host caveat.** `cargo xtask ci` ran formatting, clippy, build, workspace tests, docs/spelling, dependency-use checks, conformance, structural scanners and DST successfully. Its overall exit was 1 solely because the two advisory scans could not lock the read-only cache under the reviewer’s home (`pdca-reviewer-768-evidence/ci-rerun.log:3418`). Both scans then passed using the real `cargo deny`, a writable advisory-cache copy inside this sandbox and configurations verified to differ only in `db-path` (`pdca-reviewer-768-evidence/deny-rerun.log:4`, `pdca-reviewer-768-evidence/deny-all-features-rerun.log:4`). No waiver or policy was added. The DST sweep’s 33 main tests passed (`pdca-reviewer-768-evidence/ci-rerun.log:3975`). This cache restriction is a host caveat, not a C4 patch defect or an unmet build-tool dependency.

The frozen gate rows were adjudicated separately; the instance-scoped coverage and mutation wrappers were not independently rerun:

| Gate | Verdict | Evidence |
|------|---------|----------|
| C4-ci | PASS | Captured CI ends with all checks passed, including actual advisory scans, docs rendering and DST (`gate-logs/C4-ci.log:16`, `gate-logs/C4-ci.log:3618`, `gate-logs/C4-ci.log:4243`). |
| C4-verify | PASS | Captured red is five failed assertions and green is nine passed tests; independent runs agree (`gate-logs/C4-verify.log:14`, `gate-logs/C4-verify.log:92`). |
| C4-diff-cov | PASS | Captured measurement is 599/603 instrumentable changed lines, 99.3%; 566 lines are unscored and the log explicitly includes existing suites/co-located tests in its coverage boundary (`gate-logs/C4-diff-cov.log:10`, `gate-logs/C4-diff-cov.log:690`). |
| C5-mutants | PASS | Captured baseline passes; 39 mutants yield 35 caught and four unviable, with no survivor reported (`gate-logs/C5-mutants.log:10`). |
| T4-batch-review | FAIL | Its single bounded-await finding is confirmed in target source above (`gate-logs/T4-batch-review.log:10`). |
| T4-contribution | N/A | PR/commit contribution artifacts are intentionally absent at Check; the substantive audit must rerun at publish (`gate-logs/T4-contribution.log:10`). |
| host-tikv | PASS | Captured real feature-enabled clippy checks finish for the TiKV crate and server; this is compile evidence, not a live TiKV service test (`gate-logs/host-tikv.log:2`, `gate-logs/host-tikv.log:209`). |

**Prior art was checked by every affected path.** The main-branch histories contain 24 commits for core metadata, 14 for core write, 37 for the S3 gateway, none for the new test, and 36 for the architecture page. All 19 closed-unmerged PRs were checked against their complete file lists; only [#647](https://github.com/getwyrd/wyrd/pull/647) touches these paths, and it concerns segmented maps. The merged precedents [#594](https://github.com/getwyrd/wyrd/pull/594) and [#898](https://github.com/getwyrd/wyrd/pull/898) establish metadata and listing precision, not this ordering/capping rule. The 15 closed PRs updated since the brief’s prior-art check were also inspected. No duplicate or rejected implementation of this rule was found (`pdca-reviewer-768-evidence/prior-art-conclusions.log:1`; raw API results are retained beside that log).

**The two human decisions concern deployment and mirror recovery.** [#902 remains open](https://github.com/getwyrd/wyrd/issues/902), and the target’s systemd units order startup only after networking. The in-process future-stamp fixture faithfully exposes the original defect—the independent red run proves that—but cannot establish synchronization between deployed hosts. This is the brief’s outstanding release dependency, not an in-PR request to implement #902. Separately, the signed wire test actually demonstrates the future-mtime mirror exception; the maintainer must accept the documented recovery policy or return that design decision to Plan. The already accepted millisecond step, saturation and lack of automatic healing are not re-raised as defects.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review: issue 768 (last-modified-never-decreases)

The red→green proof holds. I re-ran it in a scratch copy of the target. With the patch, `cargo test -p wyrd-server --test last_modified_monotonic` passes 9 of 9. With the three production files reverted (`crates/core/src/metadata.rs`, `crates/core/src/write.rs`, `crates/gateway-s3/src/lib.rs`), 5 tests fail on assertions and 4 pass. The tests that fail and pass match the brief's Falsifiability section exactly. The tests call the real production path: `Gateway::put_object` / `put_object_streaming` → `commit_written` (`crates/server/src/lib.rs:315`) → `write::commit_overwrite` → `commit_chunk_map_superseding_leased` → `overwrite_modified` (`crates/core/src/metadata.rs:2451`, `:2373`). The S3 legs run over a real loopback socket.

I could not break the stored rule. I could break the test net on the S3 side: two of the new S3 code paths can be removed and every test still passes.

- NEEDS-HUMAN [impl] — The cap on the **ranged GET** arm is not pinned by any test (`crates/gateway-s3/src/lib.rs:2452`). I changed that line to `meta.modified = shown_modified(meta.modified, None);`, which turns the cap off for ranged GETs only. Every test still passed: `wyrd-gateway-s3 --lib` (85), `last_modified_monotonic` (9), and `s3_range_conditional`, `s3_head_object`, `s3_last_modified_agreement`, `s3_object_metadata`, `s3_http_wire`, `s3_list_objects`. With that change, `GET /stamps/ahead` with `Range: bytes=0-3` returns a `206` whose `Last-Modified` is an hour later than its `Date`, and `If-Unmodified-Since: <start + 59 min>` on the same ranged GET returns `412`. Those are the exact defects the brief's leg (7) exists to catch. The production code is correct today; nothing would stop it regressing. Fix: add a ranged GET (plain, and with the `If-Unmodified-Since` / `If-Modified-Since` probe) to leg (7)'s `check_wire` (`crates/server/tests/last_modified_monotonic.rs:984`), or a ranged GET to the router-level `head_shows_a_future_stored_time_as_its_own_date` (`crates/gateway-s3/src/lib.rs:5369`).
- NEEDS-HUMAN [impl] — The `Date` the handler writes on a **`304`** is not pinned (`crates/gateway-s3/src/lib.rs:2745-2747`). I changed it to `date_header(now_secs.filter(|_| false))`, which leaves the `Date` to hyper. Every test above still passed. The patch's own reason for writing `Date` in the handler (`crates/gateway-s3/src/lib.rs:3092-3097`) is that hyper's cached `Date` can be one second earlier than the handler's clock reading. In that case a `304` for a file stored in the future carries a `Last-Modified` one second later than its `Date`, which breaks RFC 9110 §8.8.2.1. Leg (7)'s `304` check (`last_modified_monotonic.rs:1027-1035`) only catches this when a request happens to cross a second boundary. The HEAD precondition branch (`lib.rs:2539-2545`) is never run by the patch's tests at all: C4-diff-cov reports `MISS crates/gateway-s3/src/lib.rs:2543`. Fix: a router-level `GET` and `HEAD` with `If-Modified-Since` on a file stored in the future, asserting `last-modified == date`, the same way `assert_shown_as_the_clock` (`lib.rs:5350`) does for `200`/`206`.
- NEEDS-HUMAN [impl] — This supports the gating T4 finding at `last_modified_monotonic.rs:183`, with one more detail. The file's own doc says `DEADLINE` is the "upper bound on every await on the store or the socket" (`:122-123`). Legs (1)–(6) await the store and the gateway with no bound: `seed_file` (`:182-192`), `put` / `stored` (`:211-235`), and `seed_record` / `record_bytes` / the core commits (`:357-411`). Only the S3 legs use `bounded()`. Whether in-memory redb counts as "external work" under the rubric's await rule is debatable, but the doc's claim is false either way. Wrap those awaits in `bounded()`, or narrow the doc to the socket legs.
- Not a patch defect: the C4-verify row in `check-gates.json` says "9 test(s) ran red". Its own log (`gate-logs/C4-verify.log`) and my re-run both show **5** red and 4 passing by design (legs 3, 4 and 8, plus the SDK-helper test). The proof is still sound, but the gate's summary overstates it. C5's "35 caught" does not cover the two survivors above either: cargo-mutants never generates a mutation that drops one call site's cap or its `Date`.

Things I tried that did not break the fix:

- **Rule values and edges** (`metadata.rs:2373-2402`): `max(t, p+1)` with saturation, the legacy `None` prior, the refusal of an unstamped overwrite as a typed error rather than `Conflict`, and the warning threshold at exactly 86_400_000 / 86_400_001 ms. All correct, and pinned by the core unit tests and leg (5).
- **Order of refusals**: segmented shape first, then the stamp, then leases (`metadata.rs:2244-2250`, `:2445-2452`). Nothing is written on any refusal. The leases a refused write leaves behind are cleaned up by the existing pending sweep, the same as after a segmented refusal.
- **CAS staleness**: the stamp comes from the exact `prior` the batch requires, so a racing writer's prior can never be ordered against: the commit loses with `Conflict` and `commit_written` (`server/src/lib.rs:332`) returns an error.
- **Other ways to publish an overwrite**: there are none in production. `commit_written` is the only caller of `commit_overwrite`. The CLI only creates. Repairs go through `commit_chunk_map`, which preserves the stamp (`metadata.rs:2194-2203`). The DST tests that overwrite unstamped over unstamped still pass (C4-ci is green).
- **Other S3 surfaces that render a time**: none are missed. Both listing versions, HEAD 200/206/304, GET 200/206/304 and both date preconditions all take the capped value. There is no CopyObject result or `If-Range` that renders or compares a time.
- **hyper overriding the handler's `Date`**: it doesn't. hyper 1.10.1 skips its own `Date` when the response already has one (`proto/h1/role.rs:840-841`, `:961`).
- **A pre-epoch clock**: when the clock can't be read, nothing is capped, no `Date` is written and 1970 is never shown, as the carry-forward asked. On the PUT side, a `Some(0)` stamp from a pre-epoch clock is now ordered after the prior instead of going backwards.
- **Design-level holes** (same-second overwrites, multi-gateway skew inside the window, a mirror that already holds a future time): the brief documents all of these and the maintainer accepted them (Impact, "Relies on #902", Open question 2). I am not re-raising them.

<!-- pdca:leaf-complete -->

### Advisory — code-review

- NEEDS-HUMAN [impl] — `crates/server/tests/last_modified_monotonic.rs:183`: The new stored-value tests await real filesystem work without a deadline: `seed_file` directly awaits `write_fragments`, and their PUT/read calls at `:213`, `:224`, and `:260` are also outside a timeout. Unlike the wire-test setup, these callers never enclose the operations in `bounded`, so a stalled operation can leave the tests waiting indefinitely. Reuse the existing `bounded` helper (`:583`) around these call chains. This confirms the finding in the frozen `T4-batch-review` log.

No additional production-correctness, reuse/simplification, or efficiency findings in this diff. Reviewed the target source and frozen gate evidence; did not rerun builds or tests.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] T3 Runtime — Confirm the #902 clock-sync requirement before fleet rollout — it remains open and no synchronized multi-host topology was exercised, so the evidence establishes seeded-prior and single-host behavior, not bounded fleet skew (`brief.md:246`, `target/deploy/dist/systemd/wyrd-s3.service:13`, `pdca-reviewer-768-evidence/prior-art-conclusions.log:10`).
- [ ] T5 Judgment — Bound the stored-value tests’ external awaits — filesystem and gateway calls in legs (1)–(6) bypass the existing deadline helper and can leave the test suite waiting indefinitely (`target/crates/server/tests/last_modified_monotonic.rs:183`, `target/crates/server/tests/last_modified_monotonic.rs:213`, `target/AGENTS.md:181`).
- [ ] Validation — fitness-to-purpose — Accept the ETag/full-copy recovery requirement for mirrors that already hold future mtimes — a newer version can appear older than such a mirror, as the wire regression demonstrates (`brief.md:564`, `target/docs/design/architecture/05-building-block-view.md:232`, `target/crates/server/tests/last_modified_monotonic.rs:1112`).
- [ ] The cap on the **ranged GET** arm is not pinned by any test (`crates/gateway-s3/src/lib.rs:2452`). I changed that line to `meta.modified = shown_modified(meta.modified, None);`, which turns the cap off for ranged GETs only. Every test still passed: `wyrd-gateway-s3 --lib` (85), `last_modified_monotonic` (9), and `s3_range_conditional`, `s3_head_object`, `s3_last_modified_agreement`, `s3_object_metadata`, `s3_http_wire`, `s3_list_objects`. With that change, `GET /stamps/ahead` with `Range: bytes=0-3` returns a `206` whose `Last-Modified` is an hour later than its `Date`, and `If-Unmodified-Since: <start + 59 min>` on the same ranged GET returns `412`. Those are the exact defects the brief's leg (7) exists to catch. The production code is correct today; nothing would stop it regressing. Fix: add a ranged GET (plain, and with the `If-Unmodified-Since` / `If-Modified-Since` probe) to leg (7)'s `check_wire` (`crates/server/tests/last_modified_monotonic.rs:984`), or a ranged GET to the router-level `head_shows_a_future_stored_time_as_its_own_date` (`crates/gateway-s3/src/lib.rs:5369`).
- [ ] The `Date` the handler writes on a **`304`** is not pinned (`crates/gateway-s3/src/lib.rs:2745-2747`). I changed it to `date_header(now_secs.filter(|_| false))`, which leaves the `Date` to hyper. Every test above still passed. The patch's own reason for writing `Date` in the handler (`crates/gateway-s3/src/lib.rs:3092-3097`) is that hyper's cached `Date` can be one second earlier than the handler's clock reading. In that case a `304` for a file stored in the future carries a `Last-Modified` one second later than its `Date`, which breaks RFC 9110 §8.8.2.1. Leg (7)'s `304` check (`last_modified_monotonic.rs:1027-1035`) only catches this when a request happens to cross a second boundary. The HEAD precondition branch (`lib.rs:2539-2545`) is never run by the patch's tests at all: C4-diff-cov reports `MISS crates/gateway-s3/src/lib.rs:2543`. Fix: a router-level `GET` and `HEAD` with `If-Modified-Since` on a file stored in the future, asserting `last-modified == date`, the same way `assert_shown_as_the_clock` (`lib.rs:5350`) does for `200`/`206`.
- [ ] This supports the gating T4 finding at `last_modified_monotonic.rs:183`, with one more detail. The file's own doc says `DEADLINE` is the "upper bound on every await on the store or the socket" (`:122-123`). Legs (1)–(6) await the store and the gateway with no bound: `seed_file` (`:182-192`), `put` / `stored` (`:211-235`), and `seed_record` / `record_bytes` / the core commits (`:357-411`). Only the S3 legs use `bounded()`. Whether in-memory redb counts as "external work" under the rubric's await rule is debatable, but the doc's claim is false either way. Wrap those awaits in `bounded()`, or narrow the doc to the socket legs.
- [ ] `crates/server/tests/last_modified_monotonic.rs:183`: The new stored-value tests await real filesystem work without a deadline: `seed_file` directly awaits `write_fragments`, and their PUT/read calls at `:213`, `:224`, and `:260` are also outside a timeout. Unlike the wire-test setup, these callers never enclose the operations in `bounded`, so a stalled operation can leave the tests waiting indefinitely. Reuse the existing `bounded` helper (`:583`) around these call chains. This confirms the finding in the frozen `T4-batch-review` log.
- [ ] T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_768/review-b
- [ ] **The shown-time cap does not establish “no change is missed.”** `brief.md:137-140` and `:481-494` promise extra transfers rather than missed changes for a future stored stamp. But the target currently exposes that future stamp unchanged (`$PDCA_TARGET/crates/gateway-s3/src/lib.rs:1034`, `:2559`). A mirror can therefore already hold mtime `p = T + 2 days`. After clock correction, a same-size overwrite stores `p + 1 ms` but the proposed cap shows approximately `T`, older than the mirror. The tracker explicitly says the client “sees the remote as older than the local copy” and skips it (`notes.json`, “Concrete failure”). This reproduces the very failure used to reject healing at `brief.md:421-428`; synchronizing hosts does not correct a mirror's saved timestamp. Revise the unconditional guarantee and required documentation, add this previously-observed-future-time case to the acceptance criteria, and identify the remaining client divergence for human adjudication.
- [ ] **The wire criteria do not prove the specified cap or cover all changed paths.** Although `brief.md:104` says a future stamp is shown as the response time, `:109-121` assert only upper bounds, non-decrease, and a successful `If-Unmodified-Since` using a future date. For the seeded future object, returning a fixed old date before and after the overwrite satisfies all those comparisons while violating `min(stored, clock)`. The new wire legs also omit GET/304 and `If-Modified-Since`, despite requiring changes there (`brief.md:381-388`); the target has a separate 304 renderer at `$PDCA_TARGET/crates/gateway-s3/src/lib.rs:2667` and IMS comparison at `:2861`. Require the exact future-time result against a shared response `Date` sample, preservation of a known past timestamp, and observable checks of those additional paths.
- [ ] **The supplied base still fails the brief's declared prerequisite.** `brief.md:219-221` declares no build-on dependency, but `:234-241` requires the advisory waiver on main and a refolded wave base before Do. In the target repository, `origin/main` now resolves to `4970140` (merge of waiver PR #903), while supplied HEAD remains `0dd47cc`; their merge base is the pre-waiver `7bf7eb9`. The target's `deny-all-features.toml:105-153` still lacks both waiver entries. Thus the main-side prerequisite has landed, but the supplied integration base has not incorporated it. Replace the historical patch-path instruction with the resolved prerequisite and a refreshed target-base requirement that is checked against Do's actual checkout; the external doctor check against another checkout cannot establish it.
- [ ] size backstop — this slice is behaving oversized: patch is 136 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- Iteration delta (if iterating): Production fix is correct; the rejection is for test gaps only. Keep the brief's design and rule unchanged; fix the tests: - Bound every await in the stored-value legs (1)-(6) with the existing `bounded()` helper (seed_file, put/stored, seed_record/record_bytes, core commits), so the DEADLINE doc's "every await" claim is true. This clears the gating T4 batch-review finding at last_modified_monotonic.rs:183. - Pin the ranged-GET cap (crates/gateway-s3/src/lib.rs:2452): add a ranged GET (plain, plus If-Unmodified-Since / If-Modified-Since probes) on the future-stored `ahead` file to leg (7) or to the router-level future-time test, asserting Last-Modified <= Date and 200/304 rather than 412/200. - Pin the handler-written Date on 304 (lib.rs:2745-2747) and run the HEAD precondition branch (lib.rs:2539-2545, currently a coverage MISS): router-level GET and HEAD with If-Modified-Since on a future-stored file, asserting last-modified == date, as assert_shown_as_the_clock does for 200/206. Keep the additions tight: the patch is already over the size backstop.
- By / date: Eduard Ralph / 2026-10-18

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
