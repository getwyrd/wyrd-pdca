# Design proposal — issue 768 / last-modified-never-decreases

> The Plan artifact (design-proposal form). Do reads ONLY this file. The `- **Label:** value`
> lines are parsed by the driver — keep their shape.
>
> The tracker labels #768 a bug, and its text asks for the ordering of `Last-Modified` to be
> decided before it is implemented. **Decided at Plan (2026-10-14, by the maintainer):** the
> issue's option 1, with **no new ADR**. ADR-0047's `modified` is the **file's** last-modified
> time, a file-level property; a rule that keeps it from going backwards across overwrites of
> the same file stays inside that intent. **Re-planned 2026-10-16, decided by the maintainer:**
> 1. **No data must be inadvertently lost despite time drift.** The rule never steps a file's
>    stored time backwards on its own: no cap, no automatic heal.
> 2. **The stored stamp steps in milliseconds**: `max(publishing clock, prior + 1 ms)`. S3 keeps
>    reporting **whole seconds** (#767), so an overwrite inside the same second can show the same
>    `Last-Modified` as the one it replaced. Accepted: "the user might not be able to see the
>    difference, but that is ok".
> 3. **S3 never shows a time later than the response's `Date`** (RFC 9110 §8.8.2.1): the header,
>    the listing, and the evaluation of `If-Modified-Since` / `If-Unmodified-Since` all use the
>    **shown time**, the earlier of the stored time and the serving gateway's clock.
> 4. **Host clock sync is a requirement at spin-up** (#902, moved to 0.1 Alpha). This rule
>    relies on it to keep far-ahead stamps rare and the skew window at milliseconds.
> 5. **An overwrite that carries no time over a version that has one is refused** with a typed
>    error, and nothing is written. That closes the last way the stored order could reset; it
>    replaces the earlier "bounded guarantee".
>
> This reverses the iteration-3 design (a whole-second step with a one-day cap) and the
> iteration-2 sign-off's two findings against the `+1 ms` rule. Citations verified against
> `getwyrd/wyrd` @ `7bf7eb9` on 2026-10-16 (`main` when this was planned). `main` has since moved
> to `e4bfc46` (#903's advisory waiver, #896/#897 = #666/#686, #904, #907), and the wave base the
> earlier iterations got folds #666, #686, #767, #671 onto `7bf7eb9`. Line numbers shift on both,
> so Do cites the base it gets, which must first pass the Ordering note's precondition.

- **Slug:** last-modified-never-decreases
- **Track:** alpha
- **Kind:** enhancement (design proposal)
- **Goal:** A file's stored `modified` strictly increases across overwrites, whichever gateway of
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
- **Success criterion:** **(A) The rule holds, with exact values.** A NEW test file drives the
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
- **Falsifiability:** RED on `origin/main`, in-process, on the plain `cargo test -p wyrd-server`
  target (the gate's own C4-verify runner compiles and runs it; no `cfg` or feature gate). On
  `main`, (1), (2) and (6) record the gateway's clock, an hour or two days **behind** the seed,
  so the exact-value assertions fail. (5) fails on the equal, both ahead, two-days-ahead and
  `u64::MAX`-prior cases, on two of the three saturation cases, on both refused cases (`main`
  commits `None` where the test expects the error), and on the repeated, far-ahead and no-reset
  chains, because both APIs store the publisher's stamp as given (`metadata.rs:2218`, `:2297`);
  its behind, legacy, unstamped-over-unstamped and both-at-max cases pass on `main` by design. (7) fails
  on `main` before the overwrite: HEAD, GET and the listing show the seed, an hour later than
  `Date`, so outside the window; the `If-Unmodified-Since` HEAD gets `412` and the
  `If-Modified-Since` GET gets `200`, because `main` compares against the stored time. (7)'s
  `behind` checks and its "earlier than the seed" check pass on `main` by design: they pin that
  the cap leaves a past time alone and that the documented exception is real.
  (3), (4) and (8) pass on `main` by design: they pin that the fix leaves the ordinary case alone
  and adds no visible lead. (1), (2), (6) and the far-ahead cases of (5) are also red against the
  iteration-3 patch (whole-second step, one-day heal). The test uses only pre-existing API (it
  seeds through `wyrd_core::write` and a raw bucket-marker put before `Gateway::new`, then calls
  `ObjectGateway`, the two public `metadata::` commits, `S3Gateway`, signed raw HTTP and
  `aws-sdk-s3`, already a dev-dependency of `wyrd-server`, `crates/server/Cargo.toml:131`), so
  C4-verify compiles it on the reverted base and the red is a failed assertion, not a build error.
  Real two-host skew is not reproducible in-process, because `now_millis` reads the host clock.
  The seeded future stamp is a faithful stand-in: the store keeps no record of which gateway wrote
  a stamp, so a prior stamped by a fast clock looks exactly like this one. The registered
  `libfaketime (Tier-1 skew leg)` is not needed.
- **Invariant to restore:** **Stored:** for every file (key), when an overwrite carries a
  `modified` stamp and the generation it supersedes has one, the new stored stamp is **greater**
  than the superseded one (at least `+1 ms`), **however far ahead** the superseded stamp is
  (saturating at `u64::MAX`; S3 shows such a value as the response time, like any future time).
  The rule never moves a file's stored time backwards
  on its own: no cap, no automatic heal. Correcting a stamp a badly wrong clock left behind is an
  explicit operator action, out of scope here. **Shown:** every S3 response's `Last-Modified`
  (header and listing) and every date-precondition check uses `min(stored, serving gateway's
  clock)`, so no response shows a time later than its `Date`. **No reset:** an overwrite that
  carries no time over a version that has one is **refused** (a typed error, nothing written).
  Core admits such a publication today (a `WritePlan` left at its default commits no metadata,
  `crates/core/src/write.rs:63-68`), and committing it would leave the next stamped overwrite
  nothing to order against. Carrying the prior's time forward into it, or inventing one, would
  put a time on content its publisher never stamped, which the gateway refuses to do for the
  same reason (`crates/gateway-s3/src/lib.rs:4564-4567`), so the only fix that invents nothing is
  to refuse. No production path publishes an overwrite without a stamp, so no production write
  changes: the one production caller of the overwrite commits is `commit_written`
  (`crates/server/src/lib.rs:275`), reached only from the two PUT paths, which set
  `Some(now_millis())` (`:254`, `:405`); `wyrd put` only creates (`cmd_put` →
  `write::write_new_object` / `cluster_put`, which refuses an existing key, `crates/server/src/cli.rs`).
  An overwrite with no time over a version with no time (a legacy record, ADR-0047) stays allowed.
  The stored invariant holds on every path that publishes **over** a prior generation. Today that
  is both of core's public overwrite commits (`metadata::commit_chunk_map_superseding`,
  `crates/core/src/metadata.rs:2199`, and `metadata::commit_chunk_map_superseding_leased`,
  `:2268`) and therefore both PUT paths. The multipart Complete flip (#658) is a future path that
  must **adopt** the rule explicitly and must stamp its publication: proposal 0016 forbids it from
  reusing the leased committer (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:756`),
  so it cannot inherit the rule from that function. So the rule cannot be met by guarding one PUT
  path, and the shown-time invariant cannot be met by guarding one render site. Repair commits
  keep preserving `modified` unchanged (ADR-0047, "Which commits set metadata",
  `docs/design/adr/0047-object-metadata-model.md:53-61`). **Sources:** the maintainer's rule
  (2026-10-16): no data must be inadvertently lost despite time drift; RFC 9110 §8.8.2.1 (an
  origin server with a clock MUST NOT send a `Last-Modified` later than its `Date`, and MUST
  replace a future value with the `Date`); ADR-0047's file-level meaning of `modified`
  (`0047-object-metadata-model.md:36`); the issue's failure analysis (sync tools compare size and
  mtime); the gateway's own stated reason for never fabricating a stamp, "sync tools compare
  `LastModified` to decide whether to transfer" (`crates/gateway-s3/src/lib.rs:4565-4566`).
  ADR-0024 imposes no skew bound on this path (`docs/design/adr/0024-clock-and-time-source-trust.md`,
  status Proposed).
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:**
- **Conflicts with:** 894, 895, 671, 767
- **Ordering note:** No build-on dependency. Host clock sync (#902) is a **release** dependency,
  not a build one: the maintainer ruled on 2026-10-16 that #902's sync is a requirement at
  spin-up, and this rule relies on it (Impact, "Relies on #902"). This slice builds and tests
  without it, so `Depends on` stays empty. #902 was moved to 0.1 Alpha on 2026-10-16 so it ships
  in the same release as #768. `Conflicts with 767`: #767 (draft PR #898) changes the listing's
  rendering (`iso8601`, the `<LastModified>` site this slice also edits) and adds a sentence to
  the same object-metadata paragraph; it is already built and folded into this bundle's wave
  base, so Do edits on top of it. `Conflicts with 894, 895`: both edit the metadata section of
  `docs/design/architecture/05-building-block-view.md` (this one `:197-200`, #894 and #895
  `:202`); #656 was split into #894 (Create) and #895 (Abort), and both carry its edit to
  `:202`. `Conflicts with 671`: both may touch the gateway's PUT commit path
  (`crates/server/src/lib.rs:234-410`); #671 is also folded into the wave base
  (`stack-base` `pdca-integration/main-rc423c4501a6d` @ `0dd47cc`).
  **Precondition before Do (re-checked 2026-10-16 at plan review):** the all-features advisory
  waiver is on `main`: PR #903, merge `4970140`, waiver commit `0674f4d` (RUSTSEC-2026-0346 and
  -0349 recorded in `deny-all-features.toml`). The wave base the earlier iterations got,
  `pdca-integration/main-rc423c4501a6d` @ `0dd47cc` (`stack-base-tip`), folds #666, #686, #767,
  #671 onto the pre-waiver `7bf7eb9` (its merge base with `main`) and has neither entry, so on
  that base the gating C4-ci stays red for a reason outside this patch. **Do's own checkout must
  contain the waiver:** `git merge-base --is-ancestor 0674f4d HEAD` succeeds, or both ids appear
  in its `deny-all-features.toml`. If not, Do stops before building and says so in
  `build-notes.md`: the base needs refreshing onto current `main` (for example `origin/main`
  merged, with sign-off, into the integration branch, which keeps #767 and #671), and that is the
  human's step, not a change for `patch.diff`. The `cargo deny all-features advisories` doctor
  check under External dependencies cannot establish this: it probes `../wyrd` as checked out
  (on 2026-10-16, branch `fix/rustsec-2026-0346-0349-tikv-waiver`, which has the waiver), not
  Do's base.
  **Intake cap (wyrd-pdca-P1):** this is a repair of an ITERATE_PLAN brief the flow is driving,
  not new intake (alpha at 7/6 on 2026-10-16, `scripts/plan-cap`); the original intake was the
  human's override for the seven-id batch (656, 666, 671, 767, 768, 620, 686) in the 2026-10-14
  Plan session.
- **Scope:** (1) making every overwrite publication store a strictly later `modified` (`+1 ms`
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
- **Difficulty:** high
- **Do model:** opus
- **External dependencies:** `cargo deny all-features advisories` `typos` `docs-renderer`
- **Test file:** `crates/server/tests/last_modified_monotonic.rs` — a **NEW** file (C4-verify
  treats an added `crates/*/tests/*.rs` as the per-fix discriminator; dry-run with
  `run-verify.sh --classify` on the iteration-3 patch returns `ADDED_TEST` for this path).
- **Citations expected:** Do must cite `path:line` on the target branch for every change.
  **Peer callsite for the shown time:** the gateway already reads its clock at the wire edge for
  the date preconditions: `epoch_secs_now()` (`crates/gateway-s3/src/lib.rs:2865-2875`, the
  documented wall-clock exemption), passed into `evaluate_conditionals` at all three call sites
  (`:2345-2349`, `:2381-2385`, `:2426`). Read the clock the same way, once per response, and use
  the one value for the `200`/`206` header (`:2489`), the `304` renderer (`not_modified_response`,
  `:2590-2603`, header at `:2597`), the precondition check (`:2774-2793`, which today compares
  the stored `m / 1_000`: `If-Unmodified-Since` at `:2777`, `If-Modified-Since` at `:2792`) and
  the listing row (`render_contents`, `:1024`). `parse_http_date` (`:2853-2861`) accepts a
  future date; keep that, since leg (7)'s `If-Modified-Since` check relies on it. The `Date`
  header comes from hyper (1.10.1, `auto_date_header` on by default), written when it encodes the
  response head, after the handler has read its clock; that is why leg (7)'s window holds. Keep
  the existing degradation for a file with **no** stored time: it still omits the field. A
  stored time **past year 9999** is a future time, so under the cap it is now shown as the
  response time instead of being omitted. That is an intended change: the handler-level test
  `an_unrenderable_stored_modified_omits_last_modified_instead_of_malforming_it`
  (`crates/gateway-s3/src/lib.rs:5035-5075`, including its year-9999 boundary half) changes its
  expectation to match. Keep `http_date` and `iso8601` returning `None` past year 9999
  (`:3183-3186`, `:620-624`) for any direct caller, and keep their unit tests (for example
  `a_contents_row_omits_unpresentable_recorded_metadata`, `:4605-4625`, as long as
  `render_contents` is handed the value it renders).
  **Peer harnesses (all on `main`):** seed before the store moves into `Gateway::new`, writing
  the prior generation through `wyrd_core::write` with `plan.object_meta.modified` set, as
  `crates/server/tests/segmented_object_read.rs:55-80` (`write_flat`) does. The file's dirent
  name is `"{bucket}/{key}"` under `ROOT` (`crates/server/src/lib.rs:708-712`). For legs (7) and
  (8), start the S3 surface as `crates/server/tests/s3_list_objects.rs:82-117` does
  (`seed_bucket`, `start_gateway` on a loopback `TcpListener`); send signed raw requests as its
  `signed_headers_q` / `get_raw` do (`:164-233`), extended to return the response headers, since
  the test must read `Date` and `Last-Modified` from the same response; `sdk_client` (`:122-137`)
  is available for PUTs. The metadata trio is committed atomically with the chunk map by
  `metadata::commit_chunk_map_superseding_leased` (called from `crates/core/src/write.rs:321-335`),
  which CAS-es on the exact prior bytes, so a stamp derived from that same `prior` is derived from
  the generation the commit actually supersedes. Core already logs through `tracing`
  (`crates/core/src/metadata.rs:2496-2500`; `tracing` is a `wyrd-core` dependency,
  `crates/core/Cargo.toml:15`), so the far-ahead warning adds no dependency.
  **Reference implementations (in this bundle; neither was reviewed, both rounds were held at
  the advisory check):** `iteration-v2/patch.diff` has this brief's stored rule,
  `overwrite_modified` = `published.max(prior.saturating_add(1))` with no cap, called from both
  commits; it lacks the refusal of an unstamped overwrite (it commits `None` instead), the
  far-ahead warning, the shown-time cap, the S3 legs and the doc text leg (B) asks for. `iteration-v3/patch.diff` has an S3-wire test harness and the warning, but its
  rule (whole-second step, one-day heal) is the **rejected** one, and its expected values are
  wrong for this brief. Neither has the shown-time cap. Do may start from either, but must
  re-check every value against this brief and re-run the gates on the base it gets.
- **Prior-art check (triage cycles):** searched by path `crates/server/src/lib.rs`
  (`now_millis`, `commit_written`), `crates/core/src/write.rs` (`commit_overwrite`),
  `crates/core/src/metadata.rs` (both superseding commits), `crates/gateway-s3/src/lib.rs`
  (`http_date`, `iso8601`, `evaluate_conditionals`) and
  `docs/design/adr/0047-object-metadata-model.md`; re-run 2026-10-16. ADR-0047 (PR #594, merged
  2026-07-18) introduced `modified` with no ordering rule. Proposal 0017 (PR #765) documents this
  gap and cites #768. Open PRs touching `Last-Modified`: #898 (#767, rendering only). Closed or
  rejected PRs on this rule or on capping `Last-Modified` at `Date`: none.
- **Disposition hint:** likely-fix

## Motivation

`Last-Modified` is the field sync and mirror tools trust to decide whether to transfer, and
Wyrd's deployment shape (N gateways, no affinity) makes consecutive writes to one file land on
different clocks as a matter of routine. No internal clock-sync requirement is recorded:
ADR-0047 states no clock assumption, and ADR-0024 does not bound skew on this path. Today the
field can move backwards while content moves forwards, and the size half of a sync tool's check
cannot catch a same-size overwrite. The blackbox validation tool's `sync` workflow (proposal
0017, #735) is blocked on the same gap.

The design went through three shapes. Iterations 1 and 2 built the `+1 ms` rule. Their review
found that a `+1 ms` step is usually invisible on S3, which reports whole seconds, and that a
far-ahead stamp is carried for good. Iteration 3 answered with a whole-second step and a one-day
heal. On 2026-10-16 the maintainer settled it: a heal is a backwards step, which can lose data on
a mirror, so it goes; and a whole-second step makes a file overwritten faster than once a second
run ahead of real time without limit, so it goes too. The `+1 ms` rule comes back, and the
far-ahead stamp is handled where it is **shown**: S3 caps the shown time at the response's
`Date`, as RFC 9110 requires, so a stamp from a badly wrong clock no longer freezes what mirrors
see.

## Design

**The stored rule.** One pure function in `wyrd-core` computes the stamp an overwrite commits
from the prior record's `modified` (`p`) and the publisher's (`t`):
- `t` is `None` and `p` is `Some` → **refuse**: return a new typed error naming the file, before
  anything is written. Not a panic, and not `CommitOutcome::Conflict` (callers retry a conflict,
  which would loop). Name the error for what it is (an overwrite without a time over a stamped
  version) and document it on both commits.
- `t` is `None` and `p` is `None` → `None` (a legacy record overwritten without a time; nothing
  to order).
- `p` is `None` (a legacy record, ADR-0047) and `t` is `Some` → `t`.
- otherwise → `max(t, p + 1)`, with saturating arithmetic, so a prior at or near `u64::MAX`
  (decodable, ADR-0045) commits `u64::MAX`, never a wrapped value. This applies however far
  ahead `p` is.
- When `p` is **more than one day** ahead of `t`, also emit a `tracing::warn!` that names both
  readings and the lead, so an operator can find the clock that was wrong. The warning changes
  nothing: the committed value is the one above. Name the one-day threshold as a constant whose
  doc says it only triggers the log.

The function reads no clock: `t` is the publishing gateway's `now_millis` reading, the clock
that owns the publication-time lifecycle, and `p` was written by the same kind of clock
(ADR-0009; core already takes time as a parameter, `crates/server/src/lib.rs:811-812`). A create
has no prior and is unchanged.

**Where the stored rule runs.** **Both** public overwrite commits call it when they build the
new record: `metadata::commit_chunk_map_superseding` (`crates/core/src/metadata.rs:2199`, the
stamp at `:2218`; no production caller today, only tests, but it is public and must not be a way
around the rule) and `metadata::commit_chunk_map_superseding_leased` (`:2268`, the stamp at
`:2297`; the one `write::commit_overwrite` calls). Both already hold `prior`. Do not put it in
each PUT path of the server. The multipart Complete flip (#658) will publish through its own
committer (0016:756), so it must stamp its publication and call the same function; the
function's doc comment says so. The repair commits (reconstruction, backfill, rebalance,
`metadata::commit_chunk_map`) do not go through this path and keep preserving the stamp. No
server code changes.

**Order of refusals.** Both commits already refuse a segmented prior with a typed error before
writing (`ChunkMapError::SegmentedMapUnsupported`: `crates/core/src/metadata.rs:2224-2229` in
`commit_chunk_map_superseding`, `:2280-2284` in the leased one). Check the new refusal **after**
that one. Two in-crate tests overwrite a segmented prior that carries a time (its root takes
`modified: Some(..)` from `flat_root`, `:4525`, via `..flat_root(..)`, `:4554`) with
`ObjectMeta::default()`, and expect the segmented-map error (`:4032-4044`, `:4100-4115`). With
the new check first they would get the new error instead. In `commit_chunk_map_superseding` the
new record is built before the segmented check today, so compute the stamp (and its refusal)
after that check, not while building the record.

**The shown time (S3 gateway).** Per response, the gateway reads its clock once at the wire
edge, as it already does for the date preconditions (`epoch_secs_now`,
`crates/gateway-s3/src/lib.rs:2865-2875`), and shows `min(stored, that clock)`:
- in the `Last-Modified` header of GET and HEAD, including the `304` path (`:2489`, `:2597`);
- in each listing row's `<LastModified>` (`render_contents`, `:1024`), so HEAD and the listing
  keep agreeing (#767) while a file carries a future stored time;
- in `If-Modified-Since` / `If-Unmodified-Since` (`evaluate_conditionals`, `:2774-2793`), so a
  client that sends back a time it was shown is judged against that same kind of value.

A file with no stored time still omits the field, as today. A stored time past year 9999 is a
future time, so it is now shown as the response time rather than omitted (see Citations for
the existing test that changes). The stored time is never rewritten by a read. Keep the clock read outside the pure helpers, as
`epoch_secs_now` does, so the comparison stays testable.

**What the wire shows.** Stored precision stays in milliseconds; the S3 surface shows whole
seconds. So on S3:
- an overwrite shows a later `Last-Modified` whenever its stored stamp lands in a later second
  than the prior's, for example always when the publishing clock is at least a second past the
  prior;
- it shows the **same** one when the stored stamp stays in the prior's second (two overwrites in
  the same second);
- the shown time never exceeds the serving gateway's clock, so which gateway answers can move it
  by the skew between gateways. With clocks synced at spin-up (#902) that is milliseconds, which
  only matters at a second boundary.

**Docs** (`AGENTS.md:154-157`, docs currency: a change to what a persisted field means updates
the living architecture doc in the same PR). Extend the paragraph at
`docs/design/architecture/05-building-block-view.md:197-200`, which already says repairs never
move `Last-Modified`, with the stored rule, the shown time, the no-correction choice and its
reason, and the refusal of an unstamped overwrite (leg B). Update the two `modified` field docs in
`crates/core/src/metadata.rs` (`:1598-1600`, `:1648-1653`) and document the gateway's shown-time
helper. ADR-0047 stays as it is.

## Alternatives considered

- **A whole-second step, `max(t, next second after p)`** (iteration 3). Every overwrite shows a
  later time on S3, but a file overwritten faster than once a second runs ahead of real time by
  about (overwrites − elapsed seconds): a steady two a second for an hour leaves it an hour
  ahead, with no limit once the cap is gone. Rejected by the maintainer on 2026-10-16: the stored
  time should stay close to real time, and an overwrite S3 shows in the same second is acceptable.
- **A one-day cap with automatic heal** (iteration 3): a prior more than a day ahead is replaced
  by the publishing clock. Rejected by the maintainer on 2026-10-16: the heal is a backwards
  step, which is the failure this issue exists to prevent. Example: a gateway boots a year fast
  and stamps `f` with a date a year ahead; a mirror copies `f` and its local copy takes that
  date. After the clock is fixed, a same-size overwrite would heal to today's time, older than
  the mirror's copy, so a mirror that copies only newer files skips it, and every later
  overwrite too, until real time passes the bad date. With no cap, the stored time is ordered
  after the bad date, and the shown-time cap keeps what clients see at the current time.
  **Correction (plan review, 2026-10-16):** with the shown-time cap in place, this example no
  longer separates the two. A mirror that already holds the bad date sees today's time after the
  overwrite either way, and skips it either way (Open question 2). What still separates them: the
  heal moves the stored time backwards, which rule 1 forbids for every reader of the stored time
  (`ObjectGateway::head_object` hands it to every gateway, not only S3), while no heal keeps the
  order and pays with a shown time that moves on every request (Impact). The decision stays the
  maintainer's 2026-10-16 one; Open question 2 asks them to confirm it on this corrected basis.
- **Heal only priors past year 9999** (where no renderer shows a date, so a client never saw
  one). Offered on 2026-10-16 to cover a corrupt `u64::MAX` stamp; not chosen.
- **Report milliseconds in the listing** (drop #767). Listing tools would see every `+1 ms`
  step, but the header cannot carry milliseconds, so the listing and HEAD would disagree for
  almost every object again, which is #767's defect. Rejected: S3 stays whole seconds.
- **Cap only the header, not the listing or the preconditions.** RFC 9110 covers only the
  header. Rejected by the maintainer on 2026-10-16: HEAD and the listing would disagree while a
  file carries a future time (#767's defect again), and a client sending back the shown time in
  `If-Unmodified-Since` would get `412` until real time passes the stored time.
- **Leave the renderers alone and record the RFC 9110 deviation.** The previous revision of this
  brief. Rejected by the maintainer on 2026-10-16 ("replacing to Date is fine").
- **Accept the reset across an unstamped overwrite and document it** (the previous revision's
  "bounded guarantee"). Rejected by the maintainer on 2026-10-16: no production path does it
  today, but a future path that forgets to stamp (for example #658) would reset a file's order
  silently. Refusing makes the same requirement an error instead of a sentence in a doc comment.
- **Carry the prior's time into an unstamped overwrite, or have core read a clock for it.** The
  first puts a time on content its publisher never stamped (the gateway refuses to invent times,
  `crates/gateway-s3/src/lib.rs:4564-4567`); the second breaks core's rule of taking time as a
  parameter (ADR-0009). Rejected.
- **Keep a hidden "highest time seen" field** that survives an unstamped version. A new
  persisted field, so ADR-0047 territory. Rejected.
- **A time server on every host instead** (#902). Makes skew small, but does not fix this issue
  alone: synced clocks still differ by milliseconds, which shows as a backwards step when it
  falls across a second boundary, and sync can fail after boot or follow a wrong server.
  Not an alternative but a partner: the maintainer made #902's sync a requirement at spin-up,
  and this rule relies on it (Impact, "Relies on #902").
- **A new ADR refining ADR-0047.** Rejected by the maintainer at Plan: ADR-0047 decides
  `modified` at the file level, and this rule keeps that meaning.
- **Option 2, a declared skew bound that fails closed (NTP).** Pushes correctness onto operators
  and turns a clock problem into an availability problem. Rejected.
- **Option 3, document that `Last-Modified` is per-gateway wall clock.** Leaves `Last-Modified`
  going backwards on the supported deployment shape, so a mirror skips the newer content for as
  long as the skew lasts. Rejected.
- **Stamp at the server's two PUT sites, or inside the leased committer only.** Two copies of one
  rule drift, the public non-leased commit stays a way around it, and the multipart flip cannot
  use the leased committer (0016:756). Rejected in favour of one function both commits call.

## Impact & compatibility

**Stored time.** It differs from the publishing clock only when the prior is at or ahead of it:
- **The prior is ahead** by any amount (a fast or badly wrong clock wrote it): the new stamp is
  the prior + 1 ms, instead of going backwards. A prior far ahead keeps the stored time in the
  future, 1 ms later per overwrite, until real time passes it or an operator corrects it; a prior
  more than a day ahead also logs a warning. **Accepted cost (maintainer, 2026-10-16):** a wrong
  stored time, but no lost data.
- **The prior equals the clock** (two overwrites in one millisecond): prior + 1 ms. A lead only
  builds above a thousand overwrites a second on one file, and drains in milliseconds.

**Shown time.** S3 now complies with RFC 9110 §8.8.2.1: no response shows a `Last-Modified`
later than its `Date`. An overwrite can show the same second as the one it replaced.
**Accepted (maintainer, 2026-10-16):** "the user might not be able to see the difference, but
that is ok". `ETag` changes on every content change and is the token for clients that must see
each one. For a file whose stored time is in the future, the shown time is the current time,
which moves forward on each request: until real time passes the stored time, mirrors re-copy it
on every pass (extra transfers; no missed change for a mirror that was never shown a future
time, with the exception under "Relies on #902", case 1), and once a second has passed since a time
was shown, `If-Modified-Since` with that time stops answering `304` and `If-Unmodified-Since`
with it answers `412`. A stored time past year 9999 (only a corrupt value gets there) used to
omit the field; it is now shown as the response time too. Clients
doing optimistic concurrency should use `If-Match` with the ETag.

**Relies on #902.** Two drift cases remain, both made rare by host clock sync at spin-up, which
the maintainer made a requirement (#902):
1. **A far-ahead stored stamp** (a clock that went badly wrong after spin-up, a wrong time
   source, or a stamp written before #902): a mirror that was only ever shown capped times
   misses no change, but the file costs extra transfers and its date preconditions behave as
   above until real time passes the stamp. **Exception:** a mirror that already holds a time
   later than real time (copied from a gateway without this rule, or from the wrong clock itself
   while it served reads) sees each later overwrite at an earlier time than its copy and, if it
   copies only newer files, skips them until an overwrite lands after real time passes the time
   it holds. No shown time can fix that within RFC 9110 §8.8.2.1; such a mirror is re-synced by
   `ETag` or with a full copy (Open question 2). The far-ahead warning log points at the clock
   that caused it.
2. **The skew window.** The shown time depends on which gateway answers. With a gateway `s`
   seconds fast, an overwrite within about `s` seconds after its write can show the same or an
   earlier second, depending on the answering gateway, as today. With synced clocks, `s` is
   milliseconds.
**Accepted residual risk (maintainer, 2026-10-16)**, except the held-future-time mirror in case 1,
found at plan review after that decision and open for the maintainer (Open question 2).

**Unstamped overwrites.** A core caller that overwrites a stamped file with no time now gets an
error instead of a commit. No production path does this (see Invariant); code that does, today
only tests, must stamp. Overwriting a legacy record (no time) without a time still works.

No record shape changes and no migration is needed. Lease stamps are untouched.

## Open questions

1. The pre-epoch clock on a **create** (`Some(0)`, listing as `1970-01-01T00:00:00.000Z` on
   `main`) shares the root but is a separate change: the same clock read also feeds lease expiry,
   and ADR-0024 decision 1 ("no fabricated time") covers it. No code change for it belongs in
   this slice.
2. **A mirror that already holds a future time (plan review, 2026-10-16; the maintainer's
   call).** A mirror that copied a file while a gateway showed a future stored time holds that
   time. After this change every later overwrite is shown at the serving clock, earlier than the
   mirror's copy, so a mirror that copies only newer files skips it until an overwrite lands
   after real time passes the time it holds. No server-side change fixes that without showing a
   time later than `Date`. This is the failure the Alternatives cite against the one-day heal,
   and with the cap in place the heal behaves the same for such a mirror (see the correction
   there). The brief proceeds on the 2026-10-16 decisions (no heal, the cap), with leg (B)
   documenting the exception and the re-sync by `ETag`. The maintainer confirms that, or changes
   the design, before the result is accepted.

## Re-plan record

- **2026-10-14 Plan, two plan-review rounds:** exact values in (1)/(2) and (5); both public
  overwrite commits share one core function; the guarantee is **bounded** across an unstamped
  publication rather than redesigning the `None` policy (only core can publish unstamped; both
  gateway PUT paths stamp). Flagged for the human; replaced on 2026-10-16 (below) by refusing
  an unstamped overwrite of a stamped version.
- **Iteration 1 (opus/medium):** the `+1 ms` rule; reviewed. Gated C4-ci red on the base's
  advisory check, not on the patch.
- **Iteration 2 (opus/xhigh):** the `+1 ms` rule again; held at the advisory check, not reviewed.
  Sign-off sent two findings back as bugs: the `+1 ms` step is invisible on S3, and a far-future
  prior is carried forward for good.
- **Iteration 3 (opus/xhigh):** whole-second step and a one-day heal, against the brief's literal
  text; held at the advisory check, not reviewed. Sign-off sent it back to Plan.
- **2026-10-16 re-plan (this revision), decided by the maintainer in the Plan session:** the
  governing rule "no data must be inadvertently lost despite time drift"; the one-day cap and
  heal dropped (a heal is a backwards step); the whole-second step dropped (unbounded lead under
  frequent overwrites); back to the stored `+1 ms` step with S3 in whole seconds, accepting that
  S3 may show the same second for an overwrite. Both iteration-2 findings are now accepted costs,
  not bugs. Verifying that choice showed two drift cases worse than today for "differs" sync
  tools; the maintainer answered by making host clock sync a requirement at spin-up (#902, filed
  this session and moved to 0.1 Alpha). Last, the maintainer chose to cap the shown time at the
  response's `Date` (RFC 9110), in the header, the listing and the date preconditions, which also
  turns a far-ahead stamp's frozen shown time into extra transfers instead of missed changes
  (for mirrors that never held the future time; see the plan-review entry below).
  Finally, the maintainer chose to refuse an overwrite with no time over a stamped version,
  instead of accepting the order reset the 2026-10-14 brief allowed.
- **2026-10-16 plan review (revision pass):** (1) "no change is missed" was too strong: a mirror
  that already holds a future time skips overwrites under this design too. Leg (B), Impact and
  the heal alternative now say so, and Open question 2 puts it to the maintainer. (2) Leg (7) now
  pins the exact shown time (the window between the test's clock before the request and that
  response's `Date`), keeps a past time exact, and covers GET, the `304` renderer and
  `If-Modified-Since`; (8) gained a lower bound. (3) The advisory waiver is on `main` (#903), but
  the stale wave base `0dd47cc` lacks it; the precondition is now checked in Do's own checkout.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

Plan-review response: finding 1 (shown-time cap does not establish "no change is missed") is accepted and fixed in leg (B), Impact and the heal alternative. The exception is pinned in leg (7) and goes to the maintainer as Open question 2, because no server-side change can fix it within RFC 9110.
Plan-review response: finding 2 (wire criteria too loose) is fixed. Leg (7) pins the exact shown time against the same response's `Date`, keeps a past time exact, and covers GET, the `304` renderer and `If-Modified-Since`; leg (8) gained a lower bound.
Plan-review response: finding 3 (stale base) is fixed in the Ordering note. The waiver is on `main` (#903, `0674f4d`), Do checks for it in its own checkout and stops if it is missing, and refreshing the base `0dd47cc` onto `main` is the human's step.

## Iteration 4 — carry-forward (from the previous attempt)
- Sign-off rationale: The fix is right; the tests are too weak. Tighten the tests and make two small code fixes. Keep the brief's behavior and scope otherwise unchanged. 1. Warning threshold (C5, 6 surviving mutants): in the `overwrite_modified_rule` test (crates/core/src/metadata.rs:5564), stop deriving `DAY` from `MODIFIED_LEAD_WARN_MILLIS`. Use literal values: 86_399_999 and 86_400_000 must not warn, 86_400_001 must warn. Also assert the `Display` text of `UnstampedOverwrite`. Rerun mutants: the survivors on the constant must now be caught. 2. Wire window: since the handler writes `Date` itself, `shown <= Date` holds by construction. In legs (7) and (8), also assert `shown <= the test's wall clock read after the response`, so a gateway clock running ahead (e.g. epoch_secs_now + 600) fails. 3. Clock earlier than 1970 (crates/gateway-s3/src/lib.rs epoch_secs_now, `.unwrap_or(0)`): when the clock read fails, skip the shown-time cap and do not write `Date` (leave it to hyper / RFC 9110 §6.6.1). Never show 1970 on objects. Add a unit test. 4. Listing cost (crates/gateway-s3/src/lib.rs:928): apply `shown_modified` only to the content rows actually emitted (after prefix/delimiter/pagination), passing the same single `now_secs` into the renderer, instead of capping every object in the bucket.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  The fix is right; the tests are too weak. Tighten the tests and make two small code fixes. Keep the brief's behavior and scope otherwise unchanged.
  1. Warning threshold (C5, 6 surviving mutants): in the `overwrite_modified_rule` test (crates/core/src/metadata.rs:5564), stop deriving `DAY` from `MODIFIED_LEAD_WARN_MILLIS`. Use literal values: 86_399_999 and 86_400_000 must not warn, 86_400_001 must warn. Also assert the `Display` text of `UnstampedOverwrite`. Rerun mutants: the survivors on the constant must now be caught.
  2. Wire window: since the handler writes `Date` itself, `shown <= Date` holds by construction. In legs (7) and (8), also assert `shown <= the test's wall clock read after the response`, so a gateway clock running ahead (e.g. epoch_secs_now + 600) fails.
  3. Clock earlier than 1970 (crates/gateway-s3/src/lib.rs epoch_secs_now, `.unwrap_or(0)`): when the clock read fails, skip the shown-time cap and do not write `Date` (leave it to hyper / RFC 9110 §6.6.1). Never show 1970 on objects. Add a unit test.
  4. Listing cost (crates/gateway-s3/src/lib.rs:928): apply `shown_modified` only to the content rows actually emitted (after prefix/delimiter/pagination), passing the same single `now_secs` into the renderer, instead of capping every object in the bucket.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 29 mutants tested in 2m: 10 missed, 16 caught, 3 unviable
- Full previous attempt preserved in `iteration-v4/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 5 — carry-forward (from the previous attempt)
- Sign-off rationale: Production fix is correct; the rejection is for test gaps only. Keep the brief's design and rule unchanged; fix the tests: - Bound every await in the stored-value legs (1)-(6) with the existing `bounded()` helper (seed_file, put/stored, seed_record/record_bytes, core commits), so the DEADLINE doc's "every await" claim is true. This clears the gating T4 batch-review finding at last_modified_monotonic.rs:183. - Pin the ranged-GET cap (crates/gateway-s3/src/lib.rs:2452): add a ranged GET (plain, plus If-Unmodified-Since / If-Modified-Since probes) on the future-stored `ahead` file to leg (7) or to the router-level future-time test, asserting Last-Modified <= Date and 200/304 rather than 412/200. - Pin the handler-written Date on 304 (lib.rs:2745-2747) and run the HEAD precondition branch (lib.rs:2539-2545, currently a coverage MISS): router-level GET and HEAD with If-Modified-Since on a future-stored file, asserting last-modified == date, as assert_shown_as_the_clock does for 200/206. Keep the additions tight: the patch is already over the size backstop.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Production fix is correct; the rejection is for test gaps only. Keep the brief's design and rule unchanged; fix the tests:
  - Bound every await in the stored-value legs (1)-(6) with the existing `bounded()` helper (seed_file, put/stored, seed_record/record_bytes, core commits), so the DEADLINE doc's "every await" claim is true. This clears the gating T4 batch-review finding at last_modified_monotonic.rs:183.
  - Pin the ranged-GET cap (crates/gateway-s3/src/lib.rs:2452): add a ranged GET (plain, plus If-Unmodified-Since / If-Modified-Since probes) on the future-stored `ahead` file to leg (7) or to the router-level future-time test, asserting Last-Modified <= Date and 200/304 rather than 412/200.
  - Pin the handler-written Date on 304 (lib.rs:2745-2747) and run the HEAD precondition branch (lib.rs:2539-2545, currently a coverage MISS): router-level GET and HEAD with If-Modified-Since on a future-stored file, asserting last-modified == date, as assert_shown_as_the_clock does for 200/206.
  Keep the additions tight: the patch is already over the size backstop.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 1 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/wyrd/wyrd-pdca/results/issue_768/review-b
- Full previous attempt preserved in `iteration-v5/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
