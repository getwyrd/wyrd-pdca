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
- C4 diff coverage: changed lines executed by the patch's tests: pass — diff coverage 99.5% — 628 of 631 instrumentable changed lines executed (floor 80%); 631 of 1203 changed lines were instr
- C5 surviving mutants on the bundle diff (cargo mutants --in-diff): pass — 39 mutants tested in 2m: 35 caught, 4 unviable

## 4. Conformance (Check — stack)
- T1 Structure: none — (no gate configured)
- T2 Shape: none — (no gate configured)
- T3 Runtime: none — (no gate configured)
- T4 batched multi-pass rubric review (3x codex, union, triaged): pass — review-branch: 0 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_768/review-b
- T4 contribution artifacts complete (user-impact opener + tracker id in both): deferred — pr-description.md not drafted yet — the substantive T4 audit of the contribution artifacts runs at publish
- T4 tikv feature compiles (crate + server selection arms): pass —     Finished `dev` profile [unoptimized + debuginfo] target(s) in 13.31s
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

Review #768: keep stored modification times ordered across overwrites and prevent S3 from showing a modification time later than its response clock.

| Item | Verdict | Basis |
|------|---------|-------|
| C1 Spec | PASS | The approved contract distinguishes stored millisecond ordering, saturation, unstamped-write refusal and whole-second S3 presentation; the latest test requirements are explicit (`brief.md:55`, `brief.md:633`, `docs/design/architecture/05-building-block-view.md:205`). |
| C2 Reproduction (red pre-fix) | PASS | Independently stashing the production changes while retaining the new test produced five assertion failures, including backward stored times and a future wire time; four control tests passed (`review-red.log:23`, `review-red.log:85`, `crates/server/tests/last_modified_monotonic.rs:266`). |
| C3 Change | PASS | Both current publication APIs and the GET/HEAD/range/listing surfaces satisfy the requested scope, including refusal before metadata writes and preservation of unstamped legacy behavior (`crates/core/src/metadata.rs:2250`, `crates/core/src/metadata.rs:2451`, `crates/gateway-s3/src/lib.rs:2452`, `crates/gateway-s3/src/lib.rs:1059`). |
| C4 Verification (red→green) | PASS | Restoring the exact patch passed all nine regression tests; broader checks and TiKV compilation also passed apart from an advisory-cache permission fault, discharged by real same-policy advisory reruns with a writable cache (`review-green-restored.log:19`, `review-ci.log:4046`, `review-deny.toml.log:2`, `review-deny-all-features.toml.log:2`, `review-tikv.log:200`). |
| C5 Causal adequacy | PASS | Ordering uses the generation protected by the overwrite CAS, and the typed refusal prevents a missing stamp from resetting that order; this removes the publication defect rather than concealing it behind a capability probe (`crates/core/src/metadata.rs:2265`, `crates/core/src/metadata.rs:2381`, `crates/core/src/metadata.rs:2401`, `crates/core/src/metadata.rs:2469`). |
| T1 Structure | PASS | Core owns the publication rule while S3 owns response-clock presentation, preserving the trait boundary and one clock source per lifecycle (`crates/core/src/metadata.rs:2361`, `crates/gateway-s3/src/lib.rs:3031`, `crates/gateway-s3/src/lib.rs:3084`). |
| T2 Shape | PASS | The five-file change stays within the approved behavior, documentation and regression scope; exact-value cases, warning boundaries and typed-error assertions make the enlarged test surface purposeful (`crates/server/tests/last_modified_monotonic.rs:489`, `crates/core/src/metadata.rs:5646`, `crates/core/src/metadata.rs:5850`, `docs/design/architecture/05-building-block-view.md:205`). |
| T3 Runtime | PASS | Added timestamp work is constant per overwrite and per emitted listing row; the regression bounds store/socket work and aborts its server task on drop, and real loopback requests passed (`crates/gateway-s3/src/lib.rs:933`, `crates/gateway-s3/src/lib.rs:1156`, `crates/server/tests/last_modified_monotonic.rs:129`, `crates/server/tests/last_modified_monotonic.rs:628`). |
| T4 Contribution | N/A | Contribution text is absent by design at Check; its substantive audit is owed to the mandatory publish-time rerun, not to human clearance now (`gate-logs/T4-contribution.log:10`). |
| T5 Judgment | NEEDS-HUMAN | Confirm the deployment meets the #902 host-clock synchronization prerequisite — #902 is still open and real fleet synchronization was not exercised, so the claimed millisecond skew window remains an external dependency (`docs/design/architecture/05-building-block-view.md:226`, `review-clock-dependency.json:1`). |
| Validation — fitness-to-purpose | NEEDS-HUMAN | Accept the ETag/full-copy recovery requirement for mirrors already holding future timestamps — the wire regression confirms that later overwrites can appear earlier than such a mirror's copy, so timestamp-only synchronization cannot establish the intended outcome (`brief.md:564`, `crates/server/tests/last_modified_monotonic.rs:1171`, `docs/design/architecture/05-building-block-view.md:232`). |

No builder-fixable defect was found. The two human decisions concern deployment readiness and the documented mirror exception; neither asks this patch to implement the separately tracked clock-sync work.

Source citations above were checked in the disposable `$PDCA_TARGET` at `target/`; brief and log citations refer to artifacts in this review directory. The target was usable, with no observed base-staleness caveat. Stash/pop restored its tracked diff byte-for-byte, and `git apply --reverse --check patch.diff` and `git diff --check` succeeded afterward. No implementation changes were made.

Independent verification supports the implementation:

- `cargo test --offline --locked -p wyrd-server --test last_modified_monotonic` produced **4 passed / 5 failed** with production reverted, then **9 passed** with it restored. Failures were runtime assertions, not compilation failures. The wire test exercised signed requests through the real server and S3 gateway, including ranged GET, date conditionals and the mirror exception. The latest carry-forward cases are present at `crates/server/tests/last_modified_monotonic.rs:191`, `crates/server/tests/last_modified_monotonic.rs:1022` and `crates/gateway-s3/src/lib.rs:5414`.
- The independent `cargo xtask ci` run reported **1,897 passed and 15 ignored tests**, including the simulation suite. Formatting, clippy, build, typos, documentation lint/render, dependency-use checks, conformance and structural scanners passed. Its overall exit was 1 solely because two advisory checks could not lock the read-only host cache (`review-ci.log:3418`, `review-ci.log:3423`). Both were rerun successfully with the real `cargo deny` tool, copying each policy unchanged except for its advisory database location inside this review directory (`review-deny.toml.log:1`, `review-deny-all-features.toml.log:1`). This is a resolved host limitation, not a patch verification failure. The frozen original CI also records success (`gate-logs/C4-ci.log:4244`).
- Both optional TiKV clippy commands were independently rerun successfully (`review-tikv.log:102`, `review-tikv.log:200`). These establish compilation, not a live TiKV deployment.

The instance-scoped coverage, mutation and batch-review wrappers are absent from this target as expected; their frozen logs were read. Coverage reports **628/631 instrumentable lines (99.5%)**, with 572 unscored lines and a stated generous treatment of colocated test lines (`gate-logs/C4-diff-cov.log:10`, `gate-logs/C4-diff-cov.log:691`). Mutation evidence reports **35 caught / 4 unviable**, with no surviving mutant reported (`gate-logs/C5-mutants.log:13`). Batch review reports zero blocking findings (`gate-logs/T4-batch-review.log:10`). These are frozen results, not claimed independent reruns.

Prior art was independently checked by every affected file path against merged commit history and all **396 closed/merged PRs**, including expanded file lists. The relevant merged predecessors are #594 (object metadata) and #898 (whole-second listing agreement). The only closed-unmerged path match was #647, concerning segmented chunk maps; its description does not propose this timestamp-ordering rule. The new regression path has no prior history. Evidence is in `review-path-history.json` and `review-prior-art.log:1`; no conflicting rejected implementation was found.

For sign-off, the seeded future-stamp tests faithfully reproduce the publication defect, but do not prove clock synchronization across deployed hosts. The inspected #902 state remains open. Separately, the successful wire regression demonstrates the future-timestamp mirror exception rather than resolving it; the maintainer's decision requested in `brief.md:564` remains owed. No `INTEGRATION.md` was supplied or present in the target, so no additional project-specific human-only checklist could be derived.

<!-- pdca:leaf-complete -->

### Advisory — adversary

# Adversarial review — issue #768 (last-modified-never-decreases)

**Verdict: I could not refute the fix.** The red→green proof reproduces, the tests run the real production path, and I found no input that breaks the stored rule or the shown-time cap within the brief's scope. One design gap is left for the maintainer, plus two small notes.

## What I re-ran

- **Green:** `cargo test -p wyrd-server --test last_modified_monotonic` on the patched tree at `$PDCA_TARGET`: 9 passed.
- **Red:** the same test on a scratch copy with `crates/core/src/{metadata,write}.rs`, `crates/gateway-s3/src/lib.rs` and `docs/` reverted to base `0806786`, test file kept. It compiles, and 5 tests fail by assertion: legs (1), (2), (6), (5) and (7). The other 4 pass, which the brief expects: (3), (4), (8) and the SDK-helper vector. This matches `gate-logs/C4-verify.log`.
- **Production path, not a copy:** legs (1)–(6) go through `Gateway::put_object` / `put_object_streaming` → `commit_written` (`crates/server/src/lib.rs:309-334`) → `write::commit_overwrite` → `commit_chunk_map_superseding_leased`. Leg (5) calls both public core commits directly. Legs (7)–(8) send signed raw HTTP/1.1 to a real `S3Gateway::serve` on loopback and parse dates with `aws-smithy-types`, not the gateway's own formatter. I found no mirrored or mocked rule.

## Attacks that did not land

- **Other callers that could now be refused:** `commit_written` (`crates/server/src/lib.rs:315`) is the only production caller of the overwrite commits, and both PUT paths stamp `Some(now_millis())` (`:277`, `:452`). Every other `plan_write` / `commit_create` use in `src/` is a create (`crates/core/src/write.rs:373-414`). DST tests never stamp `modified`, so their overwrites are `None` over `None`, which is still allowed.
- **Uncapped render sites:** every `Last-Modified` / `<LastModified>` site is capped. That covers the 200 and 206 header via `apply_object_headers` (`crates/gateway-s3/src/lib.rs:2611`), the 304 (`:2743`), the v1 and v2 listings (`render_contents`, `:1059`), and the date preconditions in both GET arms and HEAD (`:2452`, `:2489`, `:2534`). CopyObject is refused (`:1844-1849`), `If-Range` is not honoured, and no conditional PUT or DELETE exists. So no write is ever authorised against a capped time.
- **Duplicate or late `Date`:** hyper writes its own `Date` only when the response has none (hyper 1.12.0 `proto/h1/role.rs:865-866, 985`). Nothing else in the tree sets `Date`. The listing uses `insert` (`:966`), and each object response builds its header once.
- **Concurrency:** both commits CAS on the exact bytes of `prior` and compute the stamp from that same `prior` (`metadata.rs:2250`, `:2451`). A stale writer gets `Conflict` and re-reads, so the stamp is always ordered against the generation it actually replaces.
- **Order of refusals and saturation:** pinned by the core unit tests (segmented refusal first, stamp refusal before the leases) and by leg (5)'s `u64::MAX` cases. C5 caught all 35 viable mutants, including the literal one-day threshold.

## Findings

- NEEDS-HUMAN [human] — **The upload direction is never analysed, and no-heal makes its failure permanent.** Code: `crates/core/src/metadata.rs:2401` (no heal) plus `crates/gateway-s3/src/lib.rs:3084` (shown = min(stored, clock)). Concrete case: key `k` holds a stored time `F` far in the future, from a gateway booted a year fast or a corrupt `u64::MAX`, which decodes fine under ADR-0045. A user edits the local copy of `k` at time `L`, same size, on a synced host. A local→S3 tool that uploads only when the local mtime is newer than the S3 `LastModified` (the size-and-mtime rule the brief's Motivation cites) now always sees S3 = the listing's own clock, which is ≥ `L`. So it never uploads the edit. Before this patch, the same skip happened, but any stamped overwrite from an accurate gateway (for example a plain `aws s3 cp`) reset the stored time and unblocked later syncs. With no heal, every overwrite stores `F + 1`, so `k` stays blocked for same-size local edits until real time passes `F`, which never happens for `u64::MAX`. The edit never reaches the server, which runs against the brief's rule 1 ("no data must be inadvertently lost despite time drift"). Leg (B)'s text (`05-building-block-view.md:231`, `lib.rs:3066`: "A mirror that has only been shown such times misses no change") and Open question 2 cover only the S3→mirror direction. The maintainer should confirm Open question 2 with this case in view, and decide whether the operator correction the brief puts out of scope needs a tracked issue before release.
- `crates/gateway-s3/src/lib.rs:935-937`: the comment says that capping each row "so the listing and HEAD keep agreeing (#767) while a file carries a future stored time". That holds only when both requests land in the same clock second. For a future stored time, HEAD shows HEAD's clock second and the listing shows the listing's, so any pair that crosses a second boundary (or hits gateways with skewed clocks) disagrees. This is a consequence of the design the brief accepted, not a code bug. Two follow-ups: make the comment say "to the second, when served in the same second". Also, draft proposal 0017's oracle (`docs/design/proposals/draft/0017-blackbox-validation-tool.md:469-472`) treats a same-generation HEAD/listing mismatch as a failure. It will raise false failures on future-stored keys once implemented, which is out of scope here and worth an issue reference. Advisory only.
- `check-gates.json` C4-verify `path_line` ("9 test(s) ran red") overstates the evidence. `gate-logs/C4-verify.log` shows 5 failed and 4 passed on the reverted base, and my rerun agrees. The red is real and enough: the 4 green-on-main tests are the brief's by-design controls. The harness reports the number of tests run, not the number that failed. That is a reporting fix for the harness, not for this patch.

<!-- pdca:leaf-complete -->

### Advisory — code-review

No findings on either advisory lens: no correctness bugs introduced by this diff, and no actionable reuse, simplification, or efficiency issues identified.

Reviewed both overwrite commits, refusal ordering and saturation, S3 headers/listings/date preconditions, and the regression tests against the target source. Validation was assessed from the frozen gate logs; tests were not re-run. The regression suite passes with the fix and fails by assertion without it; mutation testing reports 35 caught and 4 unviable mutants, with none surviving.

<!-- pdca:leaf-complete -->

## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] T5 Judgment — Confirm the deployment meets the #902 host-clock synchronization prerequisite — #902 is still open and real fleet synchronization was not exercised, so the claimed millisecond skew window remains an external dependency (`docs/design/architecture/05-building-block-view.md:226`, `review-clock-dependency.json:1`).
- [x] Validation — fitness-to-purpose — Accept the ETag/full-copy recovery requirement for mirrors already holding future timestamps — the wire regression confirms that later overwrites can appear earlier than such a mirror's copy, so timestamp-only synchronization cannot establish the intended outcome (`brief.md:564`, `crates/server/tests/last_modified_monotonic.rs:1171`, `docs/design/architecture/05-building-block-view.md:232`).
- [x] **The upload direction is never analysed, and no-heal makes its failure permanent.** Code: `crates/core/src/metadata.rs:2401` (no heal) plus `crates/gateway-s3/src/lib.rs:3084` (shown = min(stored, clock)). Concrete case: key `k` holds a stored time `F` far in the future, from a gateway booted a year fast or a corrupt `u64::MAX`, which decodes fine under ADR-0045. A user edits the local copy of `k` at time `L`, same size, on a synced host. A local→S3 tool that uploads only when the local mtime is newer than the S3 `LastModified` (the size-and-mtime rule the brief's Motivation cites) now always sees S3 = the listing's own clock, which is ≥ `L`. So it never uploads the edit. Before this patch, the same skip happened, but any stamped overwrite from an accurate gateway (for example a plain `aws s3 cp`) reset the stored time and unblocked later syncs. With no heal, every overwrite stores `F + 1`, so `k` stays blocked for same-size local edits until real time passes `F`, which never happens for `u64::MAX`. The edit never reaches the server, which runs against the brief's rule 1 ("no data must be inadvertently lost despite time drift"). Leg (B)'s text (`05-building-block-view.md:231`, `lib.rs:3066`: "A mirror that has only been shown such times misses no change") and Open question 2 cover only the S3→mirror direction. The maintainer should confirm Open question 2 with this case in view, and decide whether the operator correction the brief puts out of scope needs a tracked issue before release.
- [x] **The shown-time cap does not establish “no change is missed.”** `brief.md:137-140` and `:481-494` promise extra transfers rather than missed changes for a future stored stamp. But the target currently exposes that future stamp unchanged (`$PDCA_TARGET/crates/gateway-s3/src/lib.rs:1034`, `:2559`). A mirror can therefore already hold mtime `p = T + 2 days`. After clock correction, a same-size overwrite stores `p + 1 ms` but the proposed cap shows approximately `T`, older than the mirror. The tracker explicitly says the client “sees the remote as older than the local copy” and skips it (`notes.json`, “Concrete failure”). This reproduces the very failure used to reject healing at `brief.md:421-428`; synchronizing hosts does not correct a mirror's saved timestamp. Revise the unconditional guarantee and required documentation, add this previously-observed-future-time case to the acceptance criteria, and identify the remaining client divergence for human adjudication.
- [x] **The wire criteria do not prove the specified cap or cover all changed paths.** Although `brief.md:104` says a future stamp is shown as the response time, `:109-121` assert only upper bounds, non-decrease, and a successful `If-Unmodified-Since` using a future date. For the seeded future object, returning a fixed old date before and after the overwrite satisfies all those comparisons while violating `min(stored, clock)`. The new wire legs also omit GET/304 and `If-Modified-Since`, despite requiring changes there (`brief.md:381-388`); the target has a separate 304 renderer at `$PDCA_TARGET/crates/gateway-s3/src/lib.rs:2667` and IMS comparison at `:2861`. Require the exact future-time result against a shared response `Date` sample, preservation of a known past timestamp, and observable checks of those additional paths.
- [x] **The supplied base still fails the brief's declared prerequisite.** `brief.md:219-221` declares no build-on dependency, but `:234-241` requires the advisory waiver on main and a refolded wave base before Do. In the target repository, `origin/main` now resolves to `4970140` (merge of waiver PR #903), while supplied HEAD remains `0dd47cc`; their merge base is the pre-waiver `7bf7eb9`. The target's `deny-all-features.toml:105-153` still lacks both waiver entries. Thus the main-side prerequisite has landed, but the supplied integration base has not incorporated it. Replace the historical patch-path instruction with the resolved prerequisite and a refreshed target-base requirement that is checked against Do's actual checkout; the external doctor check against another checkout cannot establish it.
- [x] size backstop — this slice is behaving oversized: patch is 139 KB (threshold 100 KB). Recommend answering `iterate-plan` at sign-off and authoring the split in the re-plan (`pdca split`), rather than `iterate-do`: a slice that is too big yields implementation-shaped findings every round, and splitting authors briefs, which is Plan's beat.

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
- By / date: Eduard Ralph / 2026-10-18

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 3 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
- File an issue for an operator repair tool that corrects a far-future stored `modified` (the no-heal rule leaves such a key blocking same-size uploads by newer-only sync tools; #768 adversary finding).
