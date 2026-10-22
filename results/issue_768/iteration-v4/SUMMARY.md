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
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 94.7% — 302 of 319 instrumentable changed lines executed (floor 80%); 319 of 744 changed lines were instru
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): fail — 29 mutants tested in 2m: 10 missed, 16 caught, 3 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_768/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.68s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #768: preserve stored modification-time ordering across overwrites and cap S3 timestamps at response time — red→green reproduced; one test weakness and two human decisions remain.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The agreed contract has exact, falsifiable expectations for skew, saturation, unstamped overwrites and wire timestamps; product acceptance remains separate below (`brief.md:62`; `crates/server/tests/last_modified_monotonic.rs:459`). |
| C2 Reproduction (red pre-fix) | PASS | Stashing production changes while retaining the new test produces five assertion failures, including 24 core-case mismatches; compilation succeeds (`reviewer-red.log:267`; `crates/server/tests/last_modified_monotonic.rs:247`). |
| C3 Change | PASS | The changes stay within the agreed publication/wire-time scope, with compatibility consequences documented and no ADR or persisted-layout change (`crates/core/src/metadata.rs:2250`; `docs/design/architecture/05-building-block-view.md:205`). |
| C4 Verification (red→green) | PASS | Restoring the patch passes all nine regression tests; repository checks also pass individually after relocating only the advisory database to resolve sandbox lock failures (`reviewer-green.log:19`; `reviewer-ci.log:4041`; `reviewer-deny.log:1`; `reviewer-deny-all-features.log:1`). |
| C5 Causal adequacy | NEEDS-HUMAN [impl] | Pin the warning boundary independently — the test aliases the production threshold, so all six incorrect constants survive and the promised one-day warning is not protected (`crates/core/src/metadata.rs:5564`; `crates/core/src/metadata.rs:5840`; `reviewer-mutants.log:9`). |
| T1 Structure | PASS | Publication ordering is derived from the same prior bytes guarded by each commit's CAS; protocol code retains its existing trait boundary and one clock reading per response (`crates/core/src/metadata.rs:2265`; `crates/core/src/metadata.rs:2469`; `crates/gateway-s3/src/lib.rs:2435`). |
| T2 Shape | PASS | Legacy absence and repair preservation remain compatible; typed refusal precedes writes and leased conflicts, and living documentation covers the changed contract (`crates/core/src/metadata.rs:2378`; `crates/core/src/metadata.rs:2451`; `docs/design/architecture/05-building-block-view.md:202`). |
| T3 Runtime | PASS | No new external await or background lifecycle is introduced in production; real-wire tests bound exchanges and abort their server task on drop (`crates/server/tests/last_modified_monotonic.rs:579`; `crates/server/tests/last_modified_monotonic.rs:603`; `reviewer-green.log:14`). |
| T4 Contribution | N/A | Contribution artifacts are intentionally absent at Check; their substantive audit is owed at publish. Path-based merged-history and closed-work checks completed without a competing fix (`gate-logs/T4-contribution.log:10`; `reviewer-prior-art.log:1`). |
| T5 Judgment | NEEDS-HUMAN | Confirm fulfillment of #902 before fleet rollout — clock synchronization remains an open, unexercised release dependency, so the claimed millisecond inter-host skew is not established by the single-host tests (`deploy/dist/systemd/wyrd-s3.service:13`; `docs/design/architecture/05-building-block-view.md:226`; `reviewer-prior-art.log:42`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept or revise the future-dated-mirror compatibility choice — newer-only mirrors already holding a future timestamp still skip overwrites and require ETag-based or full resynchronization (`brief.md:564`; `docs/design/architecture/05-building-block-view.md:231`; `crates/server/tests/last_modified_monotonic.rs:1080`). |

The builder-fixable concern is a test-oracle weakness, not an observed error in the current timestamp calculation. `MODIFIED_LEAD_WARN_MILLIS` is correctly 86,400,000 ms, but the logging test defines `DAY` from that same constant. Replacing the last multiplication with division makes the threshold 86 ms and moves the test's expectations with it. The independent rerun of all six constant mutations reports six survivors (`reviewer-mutants.log:3`). Use independent boundary values of 86,399,999, 86,400,000 and 86,400,001 ms to require silence, silence and a warning respectively, then rerun the mutations. The current exact-value integration tests do protect stored ordering and refusal; the diff adds no capability probe or load-time fallback that triggers the symptom-guard rule.

Verification distinguishes executed checks from frozen evidence:

- **Independent red→green:** ran `cargo test --offline -p wyrd-server --test last_modified_monotonic` with tracked production changes stashed, then after `git stash pop`. The red run has four passes and five failures; the restored run has nine passes. The real S3 wire leg exercises future and past timestamps, conditional responses, overwrite ETags, and the future-dated-mirror exception (`reviewer-red.log:282`; `reviewer-green.log:14`). To repeat that exception specifically, run `cargo test -p wyrd-server --test last_modified_monotonic wire_shows_a_future_stored_time_as_the_response_time_and_a_past_one_as_stored` from the target.
- **Independent repository gate:** `cargo xtask ci` completed workspace and simulation tests, fmt, clippy, build, typos, docs lint/render, dependency-use, conformance and structural scans. Its only failures were two advisory-database lock attempts on the read-only host cache (`reviewer-ci.log:3412`; `reviewer-ci.log:4041`). Reran both with the real `cargo deny`, a freshly fetched database under this sandbox, and configurations verified identical except for `advisories.db-path`; both passed (`reviewer-deny.log:1`; `reviewer-deny-all-features.log:1`). This is a resolved host limitation, not a patch defect or an advisory waiver.
- **Frozen coverage and feature evidence:** the coverage log records 302/319 instrumentable changed lines executed (94.7%), with 425 unscored lines; that is not a claim of coverage over all changed lines (`gate-logs/C4-diff-cov.log:769`). The TiKV gate records real feature-enabled compilation for the backend and server (`gate-logs/host-tikv.log:2`; `gate-logs/host-tikv.log:209`). The batch-review log reports zero blocking findings but contains no individual review rationale (`gate-logs/T4-batch-review.log:10`). These gates were adjudicated from their supplied logs.
- **Mutation triage:** the full frozen run records 10 missed, 16 caught and three unviable mutants (`gate-logs/C5-mutants.log:23`). Six misses are the independently reproduced threshold weakness above. Three whole-handler replacements would violate explicit server integration assertions for listing, HEAD and ranged HEAD (`crates/server/tests/last_modified_monotonic.rs:922`; `crates/server/tests/last_modified_monotonic.rs:936`; `crates/server/tests/s3_range_conditional.rs:804`), so their survival in that runner does not establish those behaviors are untested. The remaining miss erases diagnostic wording, while typed refusal and unchanged storage are asserted; it is not a second timestamp-correctness finding.

Prior-art checking covered every affected path against merged `main` history and the file lists of all 19 closed, unmerged PRs among 396 closed PRs. The only rejected-work path overlap was [PR #647](https://github.com/getwyrd/wyrd/pull/647), concerning segmented maps; its overlapping patches contain no modification-time rule. Existing metadata and whole-second rendering work are accounted for (`reviewer-prior-art.log:3`; `reviewer-prior-art.log:32`). [Issue #902](https://github.com/getwyrd/wyrd/issues/902) is still open; the target's S3 service orders only after network readiness. This remains a release dependency, not a request to expand this patch into clock-sync implementation.

All repository-source citations were checked in `$PDCA_TARGET`; evidence-log and brief citations refer to this review bundle. The target was usable, its production changes were restored, and the mutation runner's final source checksum matched its initial checksum. No implementation change was made.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — #768 (last-modified-never-decreases)

**Evidence re-run.** In a scratch copy of `$PDCA_TARGET`, `cargo test -p wyrd-server --test last_modified_monotonic` passes 9/9 with the patch. With `crates/core/src/metadata.rs`, `crates/core/src/write.rs` and `crates/gateway-s3/src/lib.rs` reverted to base, 5 tests fail by assertion (not a build error): the HEAD of `ahead` shows a future time at `:900`, the stored value is the clock instead of `seed + 1` at `:247`, and the core rule cases fail at `:568`. That matches `gate-logs/C4-verify.log`. The red→green is real and goes through the production path. I found two weak tests and two read-side edge cases:

- NEEDS-HUMAN [impl] — **The test for the one-day warning threshold checks the constant against itself.** `crates/core/src/metadata.rs:5564` defines the test's `DAY` as `MODIFIED_LEAD_WARN_MILLIS`, so the "exactly a day is not logged / a day + 1 is logged" checks (`:5867` and the asserts just above it) move whenever the constant moves. Concrete case: I changed `crates/core/src/metadata.rs:2287` to `24 * 60 * 60 / 1_000` (86 ms). All 5 `overwrite_modified_rule` tests and all 9 `last_modified_monotonic` tests still pass. The six C5 survivors on `:2287` are this same gap. In production that value would log a WARN on every overwrite whose prior leads by more than 86 ms, which ordinary skew already does. Fix: pin the threshold in the test with an independent literal (`86_400_000`). Same gap, smaller: no test checks the `Display` text of `UnstampedOverwrite` (`crates/core/src/metadata.rs:2306`, also a C5 survivor), and that text is the only thing an operator sees if a future path hits the refusal.

- NEEDS-HUMAN [impl] — **The upper bound of the wire window is no longer an independent check.** The brief built leg (7)'s window on hyper's `Date`, which is a separate clock read. The patch now writes `Date` itself, from the same `now_secs` it uses for the cap (`crates/gateway-s3/src/lib.rs:958`, `:2596`, `:2724`). So `shown <= date` at `crates/server/tests/last_modified_monotonic.rs:901` is true by construction for any value the cap produces. Concrete case: I made `epoch_secs_now` (`crates/gateway-s3/src/lib.rs:3001-3012`) return `as_secs() + 600`. Both wire tests (`wire_shows_a_future_stored_time…` and `wire_back_to_back_overwrites…`) still pass, even though every `Last-Modified` and `Date` is now 10 minutes in the future. The `If-Unmodified-Since` probe only catches a lead of about 59 minutes or more. Fix: also assert `shown <= wall_secs()`, read by the test after the response arrives. Writing `Date` in the handler is itself fine: hyper 1.10.1 skips its own `Date` when the response already has one (`proto/h1/role.rs:840`), and the stated reason (hyper's per-poll date cache) is sound.

- NEEDS-HUMAN [human] — **The cap makes date preconditions depend on the serving gateway's clock, which `main` never did. A gateway with a slow clock answers `304` for content that changed.** On `main`, `evaluate_conditionals` compares the stored time (`crates/gateway-s3/src/lib.rs:2925`), so every gateway gives the same answer. With the patch, a gateway whose clock is `s` seconds behind shows `min(stored, its clock)`, which is earlier than a healthy writer's stamp for `s` seconds after each write. Concrete case (a scratch unit test calling `shown_modified` and `evaluate_conditionals`; it passes): the client was last shown `R`. A healthy gateway overwrites at `R + 3 s`. At real `R + 3.5 s`, a gateway 5 s slow receives `If-Modified-Since: R`, shows `R - 2`, which is ≤ `R`, and answers `304 Not Modified` for new bytes. `main` answers `200`. In the same window, a mirror that copies only newer files can skip a fresh same-size overwrite. That is the #768 failure again, caused by a reader instead of a writer, and it lasts only until a later poll. The brief's Impact case 2 calls this gateway dependence "as today", which is not true for reads. The docs say skew "can move the shown time" (`docs/design/architecture/05-building-block-view.md:225`, `crates/gateway-s3/src/lib.rs:3029`) but do not say it can produce a wrong `304`. #902's clock sync keeps `s` at milliseconds. A human should decide whether this is accepted, and whether the docs should name the `304` effect.

- NEEDS-HUMAN [impl] — **A clock that cannot be read now puts 1970 on every object.** `epoch_secs_now` returns `0` when the clock reads before the epoch (`crates/gateway-s3/src/lib.rs:3011`). Before this patch, that `0` only fed the RFC-850 two-digit-year pivot. Now `shown_modified(Some(x), 0)` gives `Some(0)`: every HEAD and GET shows `Last-Modified: Thu, 01 Jan 1970 00:00:00 GMT`, every listing row shows `1970-01-01T00:00:00.000Z`, and `date_header(0)` replaces hyper's `Date` with a 1970 date (a scratch unit test confirms all three). The rubric's serialization-identity class says absent timestamps are "never fabricated (epoch-1970)". The trigger is rare (a clock before 1970) and the fix is small: when the clock read fails, skip the cap and leave `Date` to hyper (RFC 9110 §6.6.1: a server without a clock sends no `Date`).

**What I tried and could not break:**

- **Production path.** Legs (1), (2) and (6) run `Gateway::put_object` / `put_object_streaming` → `commit_written` (`crates/server/src/lib.rs:309-315`) → `write::commit_overwrite` → the leased commit. They do not use a copy of the logic.
- **Concurrency.** The stamp is computed from the same `prior` whose exact bytes the CAS (compare-and-swap) requires (`crates/core/src/metadata.rs:2250` with `:2265`, and `:2451` with `:2469`). A racing writer loses the CAS and recomputes from the new prior.
- **A surface that skips the cap.** None found. GET (full and ranged), HEAD (full, ranged and `304`), the `304` renderer and both listing versions all receive the shown value, because `modified` is replaced before any branch. CopyObject is refused (`crates/gateway-s3/src/lib.rs:1828`). DeleteObjects `<LastModifiedTime>` is refused (`:2048`).
- **The refusal firing in production.** Both PUT paths stamp `Some(now_millis())` (`crates/server/src/lib.rs:277`, `:452`), and `commit_written` is the only production caller of the overwrite commits.
- **Order of refusals.** Segmented map first, then the stamp, then the leases (`crates/core/src/metadata.rs:2244-2250`, `:2445-2451`). This matches the brief and is tested.
- **Saturation and the far-ahead cases.** These are pinned exactly on both core commits and both PUT paths.
- **The other C5 survivors.** `list_objects`, `serve_head` and `head_partial_content_response` replaced by `Default` look like a limit of the mutants run (it does not run `wyrd-server`'s wire tests, which would catch the first two), not a weak test. Ranged HEAD and HEAD-`304` with a future stored time have no wire assertion (diff-cov misses `crates/gateway-s3/src/lib.rs:2527`, `:2536-2537`). The risk is low because the shown value is set before the branch.

**Claims in the brief I think are wrong:**

- Impact, "Relies on #902" case 2: "as today" (see the third finding).
- Leg (7): "the seed, a fixed date, or a time from another response all fail". That still holds for those three, but once the patch writes `Date` itself, a skewed serving clock passes (see the second finding).

<!-- pdca:leaf-complete -->

### Advisory — code-review

No introduced production correctness bug found. Two low-priority advisory findings:

- NEEDS-HUMAN [impl] — `crates/core/src/metadata.rs:5564`: The warning-boundary test derives `DAY` from the production `MODIFIED_LEAD_WARN_MILLIS`, so its inputs and expectations move with an incorrect threshold. The frozen `gate-logs/C5-mutants.log` confirms all six arithmetic mutations of that constant survived; changing the final multiplication to division makes the threshold 86 ms instead of 86,400,000 ms while these assertions still pass. Use independent 86,400,000 ms and 86,400,001 ms inputs to pin when the warning must remain absent or appear.

- NEEDS-HUMAN [impl] — `crates/gateway-s3/src/lib.rs:928`: The new timestamp-capping loop traverses and mutates every object in the bucket before prefix filtering, delimiter grouping, and pagination, including objects that will never be rendered and requests with `max-keys=0`. This adds an avoidable full-bucket pass to each listing. Pass the same `now_secs` into the listing renderer and apply `shown_modified` only to emitted content rows; this preserves the shared clock reading while limiting the new work to the returned page.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [ ] C5 Causal adequacy — Pin the warning boundary independently — the test aliases the production threshold, so all six incorrect constants survive and the promised one-day warning is not protected (`crates/core/src/metadata.rs:5564`; `crates/core/src/metadata.rs:5840`; `reviewer-mutants.log:9`).
- [x] T5 Judgment — Confirm fulfillment of #902 before fleet rollout — clock synchronization remains an open, unexercised release dependency, so the claimed millisecond inter-host skew is not established by the single-host tests (`deploy/dist/systemd/wyrd-s3.service:13`; `docs/design/architecture/05-building-block-view.md:226`; `reviewer-prior-art.log:42`).
- [x] Validation — fitness-to-purpose — Accept or revise the future-dated-mirror compatibility choice — newer-only mirrors already holding a future timestamp still skip overwrites and require ETag-based or full resynchronization (`brief.md:564`; `docs/design/architecture/05-building-block-view.md:231`; `crates/server/tests/last_modified_monotonic.rs:1080`).
- [ ] **The test for the one-day warning threshold checks the constant against itself.** `crates/core/src/metadata.rs:5564` defines the test's `DAY` as `MODIFIED_LEAD_WARN_MILLIS`, so the "exactly a day is not logged / a day + 1 is logged" checks (`:5867` and the asserts just above it) move whenever the constant moves. Concrete case: I changed `crates/core/src/metadata.rs:2287` to `24 * 60 * 60 / 1_000` (86 ms). All 5 `overwrite_modified_rule` tests and all 9 `last_modified_monotonic` tests still pass. The six C5 survivors on `:2287` are this same gap. In production that value would log a WARN on every overwrite whose prior leads by more than 86 ms, which ordinary skew already does. Fix: pin the threshold in the test with an independent literal (`86_400_000`). Same gap, smaller: no test checks the `Display` text of `UnstampedOverwrite` (`crates/core/src/metadata.rs:2306`, also a C5 survivor), and that text is the only thing an operator sees if a future path hits the refusal.
- [ ] **The upper bound of the wire window is no longer an independent check.** The brief built leg (7)'s window on hyper's `Date`, which is a separate clock read. The patch now writes `Date` itself, from the same `now_secs` it uses for the cap (`crates/gateway-s3/src/lib.rs:958`, `:2596`, `:2724`). So `shown <= date` at `crates/server/tests/last_modified_monotonic.rs:901` is true by construction for any value the cap produces. Concrete case: I made `epoch_secs_now` (`crates/gateway-s3/src/lib.rs:3001-3012`) return `as_secs() + 600`. Both wire tests (`wire_shows_a_future_stored_time…` and `wire_back_to_back_overwrites…`) still pass, even though every `Last-Modified` and `Date` is now 10 minutes in the future. The `If-Unmodified-Since` probe only catches a lead of about 59 minutes or more. Fix: also assert `shown <= wall_secs()`, read by the test after the response arrives. Writing `Date` in the handler is itself fine: hyper 1.10.1 skips its own `Date` when the response already has one (`proto/h1/role.rs:840`), and the stated reason (hyper's per-poll date cache) is sound.
- [x] **The cap makes date preconditions depend on the serving gateway's clock, which `main` never did. A gateway with a slow clock answers `304` for content that changed.** On `main`, `evaluate_conditionals` compares the stored time (`crates/gateway-s3/src/lib.rs:2925`), so every gateway gives the same answer. With the patch, a gateway whose clock is `s` seconds behind shows `min(stored, its clock)`, which is earlier than a healthy writer's stamp for `s` seconds after each write. Concrete case (a scratch unit test calling `shown_modified` and `evaluate_conditionals`; it passes): the client was last shown `R`. A healthy gateway overwrites at `R + 3 s`. At real `R + 3.5 s`, a gateway 5 s slow receives `If-Modified-Since: R`, shows `R - 2`, which is ≤ `R`, and answers `304 Not Modified` for new bytes. `main` answers `200`. In the same window, a mirror that copies only newer files can skip a fresh same-size overwrite. That is the #768 failure again, caused by a reader instead of a writer, and it lasts only until a later poll. The brief's Impact case 2 calls this gateway dependence "as today", which is not true for reads. The docs say skew "can move the shown time" (`docs/design/architecture/05-building-block-view.md:225`, `crates/gateway-s3/src/lib.rs:3029`) but do not say it can produce a wrong `304`. #902's clock sync keeps `s` at milliseconds. A human should decide whether this is accepted, and whether the docs should name the `304` effect.
- [ ] **A clock that cannot be read now puts 1970 on every object.** `epoch_secs_now` returns `0` when the clock reads before the epoch (`crates/gateway-s3/src/lib.rs:3011`). Before this patch, that `0` only fed the RFC-850 two-digit-year pivot. Now `shown_modified(Some(x), 0)` gives `Some(0)`: every HEAD and GET shows `Last-Modified: Thu, 01 Jan 1970 00:00:00 GMT`, every listing row shows `1970-01-01T00:00:00.000Z`, and `date_header(0)` replaces hyper's `Date` with a 1970 date (a scratch unit test confirms all three). The rubric's serialization-identity class says absent timestamps are "never fabricated (epoch-1970)". The trigger is rare (a clock before 1970) and the fix is small: when the clock read fails, skip the cap and leave `Date` to hyper (RFC 9110 §6.6.1: a server without a clock sends no `Date`).
- [ ] `crates/core/src/metadata.rs:5564`: The warning-boundary test derives `DAY` from the production `MODIFIED_LEAD_WARN_MILLIS`, so its inputs and expectations move with an incorrect threshold. The frozen `gate-logs/C5-mutants.log` confirms all six arithmetic mutations of that constant survived; changing the final multiplication to division makes the threshold 86 ms instead of 86,400,000 ms while these assertions still pass. Use independent 86,400,000 ms and 86,400,001 ms inputs to pin when the warning must remain absent or appear.
- [ ] `crates/gateway-s3/src/lib.rs:928`: The new timestamp-capping loop traverses and mutates every object in the bucket before prefix filtering, delimiter grouping, and pagination, including objects that will never be rendered and requests with `max-keys=0`. This adds an avoidable full-bucket pass to each listing. Pass the same `now_secs` into the listing renderer and apply `shown_modified` only to emitted content rows; this preserves the shared clock reading while limiting the new work to the returned page.
- [x] **The shown-time cap does not establish “no change is missed.”** `brief.md:137-140` and `:481-494` promise extra transfers rather than missed changes for a future stored stamp. But the target currently exposes that future stamp unchanged (`$PDCA_TARGET/crates/gateway-s3/src/lib.rs:1034`, `:2559`). A mirror can therefore already hold mtime `p = T + 2 days`. After clock correction, a same-size overwrite stores `p + 1 ms` but the proposed cap shows approximately `T`, older than the mirror. The tracker explicitly says the client “sees the remote as older than the local copy” and skips it (`notes.json`, “Concrete failure”). This reproduces the very failure used to reject healing at `brief.md:421-428`; synchronizing hosts does not correct a mirror's saved timestamp. Revise the unconditional guarantee and required documentation, add this previously-observed-future-time case to the acceptance criteria, and identify the remaining client divergence for human adjudication.
- [x] **The wire criteria do not prove the specified cap or cover all changed paths.** Although `brief.md:104` says a future stamp is shown as the response time, `:109-121` assert only upper bounds, non-decrease, and a successful `If-Unmodified-Since` using a future date. For the seeded future object, returning a fixed old date before and after the overwrite satisfies all those comparisons while violating `min(stored, clock)`. The new wire legs also omit GET/304 and `If-Modified-Since`, despite requiring changes there (`brief.md:381-388`); the target has a separate 304 renderer at `$PDCA_TARGET/crates/gateway-s3/src/lib.rs:2667` and IMS comparison at `:2861`. Require the exact future-time result against a shared response `Date` sample, preservation of a known past timestamp, and observable checks of those additional paths.
- [x] **The supplied base still fails the brief's declared prerequisite.** `brief.md:219-221` declares no build-on dependency, but `:234-241` requires the advisory waiver on main and a refolded wave base before Do. In the target repository, `origin/main` now resolves to `4970140` (merge of waiver PR #903), while supplied HEAD remains `0dd47cc`; their merge base is the pre-waiver `7bf7eb9`. The target's `deny-all-features.toml:105-153` still lacks both waiver entries. Thus the main-side prerequisite has landed, but the supplied integration base has not incorporated it. Replace the historical patch-path instruction with the resolved prerequisite and a refreshed target-base requirement that is checked against Do's actual checkout; the external doctor check against another checkout cannot establish it.
- [x] size backstop — this slice is behaving oversized: patch is 110 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.
- [x] **The fix does not reach the wire, and for some sync rules it makes the issue's scenario worse.** `crates/core/src/metadata.rs:2272` commits `max(t, p + 1 ms)`. Both renderers truncate to seconds: `crates/gateway-s3/src/lib.rs:637` (listing, always `.000Z`) and `:3264` (header). The base already documents this at `docs/design/architecture/05-building-block-view.md:195-198`: #767 has landed. So the brief's statement "a listing on `main` renders milliseconds … `.751Z` after `.750Z`" is out of date here, and the bump is visible on **no** S3 surface unless `p` ends in `.999`.
- [x] **One bad clock reading now sticks to a file permanently. Before the fix the next good write healed it.** The rule has no cap (`crates/core/src/metadata.rs:2272`). The patch's own leg (1) shows a prior an hour ahead is carried forward exactly (`last_modified_monotonic.rs:136-151`).
- [x] The unconditional ordering contract is contradicted by the planned handling of absent metadata. `brief.md:82-89` promises the invariant for every committed generation on both public overwrite APIs, and `brief.md:61-62` / `brief.md:211-212` require unqualified “never decreases across overwrites” documentation. But `brief.md:52-53` and `brief.md:165-168` require a publisher with `None` to clear the stamp and a prior with `None` to accept the next publisher's stamp unchanged. Counterexample under those rules: `Some(2000)` → an unstamped overwrite → `None` → an overwrite publishing `Some(1000)` → `Some(1000)`. The same file loses its ordering history and later exposes a lower timestamp. This is an admitted core API input, not malformed metadata: target `crates/core/src/write.rs:63-68` explicitly permits plans with no metadata, and `crates/core/src/metadata.rs:2218` / `:2297` store that optional value. Current gateway PUTs supply `Some` (`crates/server/src/lib.rs:254`, `:405`), so the chosen implementation can still fix the reported gateway defect. Revise the invariant and required docs to limit the guarantee to uninterrupted timestamp-bearing overwrites, explicitly documenting the reset across an unstamped publication; add the chained case to A5 to pin that boundary. If the unconditional core guarantee is intended instead, the `None` policy needs a design revision.

## 7. Proven / not proven
- Proven by which oracle: gates overall = pass (stub oracles).
- Unproven / needs manual run: anything flagged in §6.

## 8. Ready-to-ship attachments
- patch.diff
- tracker-comment.md     (ALWAYS, every tracker item)
- build-notes.md         (builder rationale — for the human, not the reviewer)

## 9. Check sign-off                     ← human completes Check here
- Disposition confirmed / overridden:
- Outcome: iterated-to-Do
- Iteration delta (if iterating): The fix is right; the tests are too weak. Tighten the tests and make two small code fixes. Keep the brief's behavior and scope otherwise unchanged. 1. Warning threshold (C5, 6 surviving mutants): in the `overwrite_modified_rule` test (crates/core/src/metadata.rs:5564), stop deriving `DAY` from `MODIFIED_LEAD_WARN_MILLIS`. Use literal values: 86_399_999 and 86_400_000 must not warn, 86_400_001 must warn. Also assert the `Display` text of `UnstampedOverwrite`. Rerun mutants: the survivors on the constant must now be caught. 2. Wire window: since the handler writes `Date` itself, `shown <= Date` holds by construction. In legs (7) and (8), also assert `shown <= the test's wall clock read after the response`, so a gateway clock running ahead (e.g. epoch_secs_now + 600) fails. 3. Clock earlier than 1970 (crates/gateway-s3/src/lib.rs epoch_secs_now, `.unwrap_or(0)`): when the clock read fails, skip the shown-time cap and do not write `Date` (leave it to hyper / RFC 9110 §6.6.1). Never show 1970 on objects. Add a unit test. 4. Listing cost (crates/gateway-s3/src/lib.rs:928): apply `shown_modified` only to the content rows actually emitted (after prefix/delimiter/pagination), passing the same single `now_secs` into the renderer, instead of capping every object in the bucket.
- By / date: Eduard Ralph / 2026-10-18

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
