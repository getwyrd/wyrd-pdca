# Build notes — issue 768 / last-modified-never-decreases (iteration 3)

Target: getwyrd/wyrd @ main, built in `$PDCA_WORKTREE` on the stack base
`pdca-integration/main-rc423c4501a6d` @ `0dd47cc` (the bundle's `stack-base-tip`, the same
base as iterations 1 and 2). Line numbers below are on that tree with the patch applied.

## What the carry-forward asked, and what changed

The iteration-2 sign-off sent back two findings as bugs to fix, plus one to keep pinned:

1. **The `+1 ms` step never reached the wire.** Both S3 renderers drop the milliseconds
   (`crates/gateway-s3/src/lib.rs:637` listing, `:3264` header; #767 has landed). So in the
   issue's own scenario the client saw the *same* second before and after a same-size
   overwrite, and sync tools skipped it.
   **Fixed by changing the step's unit to the wire's.** An overwrite now commits
   `max(publishing clock, the start of the next whole second after the prior)`
   (`crates/core/src/metadata.rs:2325-2328`). Every S3 surface then shows the new
   generation at least one second later than the one it replaced.
2. **A far-future prior was carried forward for good**, where before the fix the next good
   write healed it. **Fixed with a one-day cap** (`metadata.rs:2262`, checked at
   `:2312-2324`): a prior more than a day ahead of the publishing clock is treated as a
   wrong reading, the publishing clock wins (as before #768), and a `tracing::warn!` is
   logged (`:2314`, target `wyrd.metadata.modified`).
3. **The unstamped-publication reset** stays as the brief bounds it, and leg 5 still pins
   the chained case (`crates/server/tests/last_modified_monotonic.rs`, the "reset across an
   unstamped generation" chain in `core_overwrite_commits_apply_the_rule_exactly`, `:397`).

The waiver for the base advisory (RUSTSEC-2026-0346/0349) has **not** landed on this
base: `0dd47cc` is the worktree HEAD, and `deny-all-features.toml` has no entry for either
id. The external-dependency marker for it is under "Gate runs".

### Where this departs from the brief's literal text (on purpose)

The carry-forward overrides three of the brief's exact values. I changed them rather than
keep both, because the old values *are* the rejected behaviour:

- **A1/A2:** the brief says "exactly `seeded + 1`". Now exactly the start of the next whole
  second after `seeded`. The seed is placed mid-second (`.500`, `an_hour_fast`,
  `last_modified_monotonic.rs:98-101`), so `+1 ms`, `+1000 ms` and the next-second rule all
  give different answers, and an implementation jumping to `u64::MAX` still fails.
- **A5:** the brief says prior `Some(u64::MAX)` → `Some(u64::MAX)`. A prior of `u64::MAX` is
  the most far-future stamp there is, so under finding 2 it now heals: → `Some(T)`. The
  "saturates, no panic, no wrap" property is still pinned, by priors at and near
  `u64::MAX` with a publisher within the cap (`u64::MAX - 1` → `u64::MAX`).
- **Invariant:** the brief's invariant ("new stamp ≥ superseded one" whenever both are
  stamped) now has a second deliberate bound, the one-day cap. It is the one case where
  `modified` can still go backwards, and it is stated in the helper doc, both field docs
  and the architecture doc.

Everything else in the brief holds as written: one pure helper in core, called by both
public overwrite commits; no server change; no ADR touched; the multipart flip (#658) must
call the helper explicitly; repairs keep preserving `modified`.

## The rule, and why this shape

`overwrite_modified(prior, published)` (`crates/core/src/metadata.rs:2307`):

- `published == None` → `None` (unchanged from the brief: the chain resets).
- `prior == None` → `published`.
- `prior - published > 1 day` → `published` (heal, logged).
- otherwise → `max(published, (prior / 1000 + 1) * 1000)`, with `saturating_add` /
  `saturating_mul`, so a prior near `u64::MAX` commits `u64::MAX`.

### Alternatives I weighed for the wire step

- **(chosen) `max(t, next_second(p))`.** Every overwrite shows a strictly later second on
  the wire. That satisfies both kinds of sync check: "newer than" (`If-Modified-Since`,
  HTTP caches, source-newer rules) and "differs from" (rclone, `aws s3 sync
  --exact-timestamps`). It is the brief's own rule shape (`max(t, p + 1 unit)`) with the
  unit set to what clients see.
  Cost: a key overwritten more than once a second runs ahead of the clock by about
  (overwrites − elapsed seconds). Concretely, 10 overwrites in one second leave the stamp
  about 9 s ahead, and that drains in 9 s of quiet. The brief already accepted this class
  of lead at millisecond size ("pinned as intended"); here it is second-sized. The day cap
  bounds it: a key hammered faster than 1/s for about a day reaches the cap and takes one
  backward step (the heal).
- **(rejected) conditional: `if t > p { t } else { next_second(p) }`.** Steps only when the
  publishing clock is not already later. A healthy fleet would behave exactly as today,
  with no burst lead. But it does not close the skew window. With gateway A `s` seconds
  fast and an accurate B writing `Δ` later in real time, `s ≤ Δ < s + 1` still gives
  `t > p` in the same wire second, so the overwrite is invisible again. Example: A is 2 s
  fast and B writes 2.5 s after A; the chance of a same-second collision is then about
  1 in 2. So it leaves the reported failure (skew hides a same-size overwrite) partly in
  place, and the carry-forward says that must be fixed. The code would be the same size
  (one `if` in place of one `max`). Rejected on behaviour, not size.
- **(rejected, iteration 2's rule) `max(t, p + 1 ms)`.** It is the finding. The new test
  proves it fails (see "Evidence").
- **(rejected) clamp or change the renderers.** Rendering milliseconds again would undo
  #767 (listing and header must name the same instant), and IMF-fixdate cannot carry
  sub-seconds at all. Out of scope too.

### The cap value: one day

- It has to be above one hour, because the maintainer's own scenario in the brief (a
  gateway one hour fast, A1/A2) is the case the rule exists to carry. A cap at or below an
  hour would heal the brief's central case.
- I checked the existing clock bounds. `DELTA_CLOCK_MILLIS = 1 s`
  (`crates/custodian/src/gc.rs:231`) is far too small: the issue's own example is 2 s of
  skew, and a few same-ms overwrites build a second of lead. SigV4's 15-minute window
  (`crates/gateway-s3/src/sigv4.rs:33`) would heal the brief's one-hour case. Neither fits,
  so the cap is its own constant, stated and justified at `metadata.rs:2254-2262`.
- A day is past any time-zone mix-up (up to 14 h) and well past the one-hour case. It still
  turns "stuck for good" into "heals at the next overwrite" for the real far-future cases
  (a wrong date, a 2036 RTC, a corrupt `u64::MAX`).
- Known limit, documented in the helper: past the cap the rule cannot tell "prior too far
  ahead" from "publisher too far behind" (a gateway with its clock reset to 2000). Either
  way the publisher wins, which is the pre-#768 behaviour.
- **Not** a skew bound in the brief's out-of-scope sense (option 2): no write is refused
  and operators carry no new requirement. It only decides which of two disagreeing
  readings is recorded.

### The log on heal

Core already logs through `tracing` (`metadata.rs:2583-2596`), so this adds no dependency.
The helper is the one place every overwrite passes through, so #658 gets the log by calling
it. The helper has no object id, so the event names the two readings and the lead, not the
key. That is enough to find the bad clock. Adding an `id` parameter only for the log would
put a logging argument on #658's call; I left it out.

## What changed (whole patch)

- `crates/core/src/metadata.rs`
  - `:2249-2252` `MODIFIED_WIRE_UNIT_MILLIS` (1000) and `:2254-2262`
    `MODIFIED_MAX_LEAD_MILLIS` (one day), both private, with their reasons.
  - `:2264-2329` `pub fn overwrite_modified`: the rule, the reset, the cap, saturation, the
    cost, which clock owns the input (it reads none, ADR-0009), and that #658 must call
    it.
  - `:2222` (`commit_chunk_map_superseding`) and `:2384`
    (`commit_chunk_map_superseding_leased`): `modified: overwrite_modified(prior.modified,
    meta.modified)`. Both CAS on `encode(prior)`, so the prior is the superseded
    generation.
  - `:1587-1595` `ObjectMeta::modified` and `:1645-1652` `InodeRecord::modified` docs
    (leg B).
- `docs/design/architecture/05-building-block-view.md:205-225`: the rule, why the step is
  a whole second, the burst-lead cost, both bounds (day cap, unstamped reset), that only
  core can publish unstamped, that ETag is the content-identity token, and the ADR-0047
  sentence. The repairs-preserve text sits just above (`:201-204`). No ADR touched.
- `crates/server/tests/last_modified_monotonic.rs` (new, 8 tests):
  - (1) `streaming_overwrite_after_a_fast_clock_steps_to_the_next_second` `:195`
  - (2) `buffered_overwrite_after_a_fast_clock_steps_to_the_next_second` `:221`
  - (3) `overwrite_over_a_past_stamp_records_the_current_clock` `:241`, both PUT paths
  - (4) `create_records_the_current_clock` `:262`, both PUT paths
  - (5) `core_overwrite_commits_apply_the_rule_exactly` `:397`: 16 single cases on both
    public commits (equal ms, same-second earlier/later, ahead, previous second, behind,
    legacy, three unstamped, exactly one day, one day + 1 ms, `u64::MAX` heal, three
    saturation cases) and three chains (three in a row, the unstamped reset,
    heal-then-resume)
  - (6) `overwrite_over_a_stamp_more_than_a_day_ahead_heals_to_the_clock` `:283`, both PUT
    paths
  - (7) `wire_shows_a_later_second_after_a_fast_clock` `:638`: the real `S3Gateway` over
    loopback, read with a stock `aws-sdk-s3` client; HEAD and ListObjectsV2 show exactly
    `seeded_secs + 1` after the overwrite and `+ 2` after a second one
  - (8) `wire_back_to_back_overwrites_each_show_a_later_second` `:676`: one healthy
    gateway, four back-to-back same-size PUTs, each a later second on HEAD and in the
    listing, leading the clock by at most one second per overwrite
  Seeding mirrors `segmented_object_read.rs:55-80` (`write_flat`); the S3 legs reuse the
  bucket-marker and in-process-server pattern of `s3_last_modified_agreement.rs` and the
  SDK client of `s3_gateway_cluster.rs:100-115`. I opened those two files beyond the
  brief's cited peers because the carry-forward is about the wire: only a test that reads
  the rendered header and listing can prove the step is visible. Every SDK await is
  bounded (30 s), and the server task is aborted on drop.

## Evidence

Runner: `./engine/scripts/run-verify.sh` (the project's C4-verify script) with
`PDCA_BUNDLE=results/issue_768`, `PDCA_BRIEF_BASE=0dd47cc…`.

- **This patch:** exit 0, `run-verify.sh: PASS — red without the fix, green with it (8
  test(s) ran red)`. Green: `8 passed; 0 failed`. Production reverted, test kept:
  `3 passed; 5 failed`, all assertion failures:
  - (5) `:387` `Superseding: equal (same millisecond)`: `left: Some(1700000000250)
    right: Some(1700000001000)`
  - (1) `:209` / (2) `:230`: `left: Some(1792146096116) right: Some(1792149697000)`
    (the gateway's clock, an hour behind the seed)
  - (7) `:655`: `left: (1792146096, 1792146096) right: (1792149697, 1792149697)`: HEAD and
    listing an hour *earlier* on the wire
  - (8) `:688`: `overwrite 1: Last-Modified 1792146096 must be later than the 1792146096 it
    replaced`
  - (3), (4) and (6) pass on the base by design: they pin that the fix leaves the ordinary
    case and the healing case alone.
- **Iteration 2's `+1 ms` rule with this test** (a scratch bundle: the same patch with the
  helper body swapped back to `max(published, prior + 1)`): the green leg fails, `2
  passed; 6 failed`. It fails exactly on the two findings:
  - (7) `:655`: `left: (1792149735, 1792149735) right: (1792149736, 1792149736)`, the
    same second on HEAD and listing (finding 1, on the real wire)
  - (8) `:688`: `1792146135 must be later than the 1792146135 it replaced`
  - (6) `:295`: `1792318935553 must be the clock … not carried from a prior two days
    ahead (1792318935552)` (finding 2)
  - (1), (2), (5): `Some(…501)` vs `Some(…000)`
  So this round does not re-submit the rejected approach.

### Gate runs (this round, worktree with the patch applied)

- `./engine/xtask.sh ci` (the project's gate wrapper, on `$PDCA_WORKTREE`): typos;
  `lint_docs.py` OK; `render_site.py --check` (99 pages, link audit OK); gitlink, unsafe
  and blackbox guards; `cargo fmt --check`; clippy; build; **`cargo test`: 201 test
  binaries, 1,718 passed, 0 failed, 14 ignored**, including all 8 `last_modified_monotonic`
  tests; cargo-machete clean; `cargo deny check` (`advisories ok, bans ok, licenses ok,
  sources ok`). It then stops at the second deny step,
  `cargo deny --all-features --config deny-all-features.toml check advisories`:
  `advisories FAILED` on RUSTSEC-2026-0346/0349 (`rustls 0.21.12` via `tikv-client 0.4.0`).
  That is the base's failure, the same one iterations 1 and 2 hit. The patch touches no
  `Cargo.toml`, `Cargo.lock` or deny config.
  - Two doc-comment-only edits in `metadata.rs` landed after that run started (wording in
    the helper's first paragraph and the cap's doc). After them I re-ran
    `cargo fmt --all -- --check` (clean) and `cargo clippy -p wyrd-core -p wyrd-server
    --all-targets` (clean), regenerated `patch.diff`, and re-ran `run-verify.sh` on the
    final patch: `PASS — red without the fix, green with it (8 test(s) ran red)`.
- `./engine/xtask.sh dst` (run on its own, because `ci` reaches DST only after the
  advisory step): clippy under `--cfg madsim`, then `cargo test -p wyrd-dst`: **10 test
  binaries, 85 passed, 0 failed, 1 ignored, exit 0**, including the long
  `committed_regression_seeds_stay_green` campaign.
- Raw logs (harness scratch, reclaimed by the driver):
  `$PDCA_SCRATCH/pdca-builder-768-verify.log`, `pdca-builder-768-verify-final.log`,
  `pdca-builder-768-ci.log` (advisory failure at the end), `pdca-builder-768-dst.log`,
  and `pdca-builder-768-v2rule/` (the iteration-2-rule cross-check).

The advisory is still not cleared on this base. The iteration-2 sign-off said to land
`results/issue_620/base-advisory-waiver.diff` on main and update `../wyrd` before this
re-run. That has not happened: `../wyrd` is at `7bf7eb9` and the worktree base at
`0dd47cc`, and neither `deny-all-features.toml` names RUSTSEC-2026-0346 or -0349. Landing
a security waiver is a policy call outside this slice, so the patch does not touch it.

NEEDS-HUMAN external dependency: cargo deny all-features advisories — the base `0dd47cc` still fails `cargo deny --all-features --config deny-all-features.toml check advisories` (RUSTSEC-2026-0346/0349, rustls 0.21.12 via tikv-client 0.4.0) before this patch is applied; the waiver (results/issue_620/base-advisory-waiver.diff) the iteration-2 sign-off asked to land on main has not landed, so the gating C4-ci row stays red for a reason this patch cannot clear within scope

The doctor row already exists (`pdca.toml:1402-1407`); the brief's `External dependencies`
only needs to name its id so the guard holds the bundle before a Do is spent:

```toml
[[doctor.checks]]
group = "workspace"
id    = "cargo deny all-features advisories"   # already present at pdca.toml:1402 — name it in the brief
cmd   = 'cd "${WYRD_REPO:-../wyrd}" && cargo deny --frozen --all-features --config deny-all-features.toml check advisories >/dev/null 2>&1'
hint  = "land the deny-all-features.toml waiver for RUSTSEC-2026-0346/0349 (results/issue_620/base-advisory-waiver.diff) on main, update ../wyrd, re-run"
level = "WARN"
```

## Refute-my-own-test

- **(a) Genuine red?** Yes. `run-verify.sh` reverted the production change, kept the test,
  and 5 of 8 tests failed as assertions (values above). The file compiles on the base
  because it uses only pre-existing API (`ObjectGateway`, `Gateway::put_object`, the two
  public `metadata::` commits, `S3Gateway`, `aws-sdk-s3`), so the red is a failed check,
  not a build error. It is also red against iteration 2's rule (6 of 8), so it binds the
  carry-forward, not just the original issue.
- **(b) Production path?** Yes. Legs 1–4 and 6 drive the real `wyrd_server::Gateway`
  (`put_object_streaming` / `put_object` → `commit_written` → `write::commit_overwrite` →
  `commit_chunk_map_superseding_leased`). Leg 5 calls both real public core commits on a
  real in-memory redb store. Legs 7–8 go through the shipping S3 surface (`S3Gateway::serve`
  over a loopback TCP listener, SigV4, the real `http_date` / `iso8601` renderers), read by
  a stock SDK that parses `Last-Modified` the way clients do. Nothing is mocked or copied.
  The expected values in the test are written out (constants, or `(second + 1) * 1000`
  from the seed), not computed by a copy of the helper.
- **(c) Fixture includes the fault?** Yes, as far as in-process allows. The fault is a
  prior stamp ahead of the publishing gateway's clock. The fixture commits exactly that
  record, an hour ahead of the host clock the gateway reads, through the normal create
  path. The store keeps no record of which gateway wrote a stamp, so it is the same record
  a fast-clocked peer leaves. For finding 2 the fixture commits a prior two days ahead
  (leg 6) and `u64::MAX` (leg 5). Real two-host skew is not reproduced; the brief says the
  libfaketime leg is not needed, and the store-level record is identical. Overwrites use
  different bytes of the same size (17 bytes each, asserted at `:196-200`), the case sync
  tools miss.

## Rubric self-review

- **One clock per lifecycle:** the helper reads no clock and says which one owns its input
  (`metadata.rs:2279-2281`). The test's `wall_millis` (`last_modified_monotonic.rs:88`)
  reads `SystemTime::now()` under `#[allow(clippy::disallowed_methods)]` with a
  "wall-clock exempt" comment (`:85-87`): it brackets the gateway's own `now_millis`, so it
  must read the same source, and the SDK signs with it too.
- **Docs currency:** the meaning of a persisted field changed, so the architecture doc and
  both field docs change in the same patch.
- **Serialization identity:** no field shape or serde attribute changed; `None` stays
  `None` (pinned by the three unstamped cases).
- **Absent entries:** a legacy (`None`) prior and an unstamped publication have explicit,
  tested behaviour. The heal is logged, not silent.
- **Transactions:** no new early return; the helper is pure apart from the log and runs
  before the batch is built.
- **Await discipline (test):** every SDK and listener await is under a 30 s timeout, and
  the spawned server task is aborted on drop.
- **Protocol input / grammar:** no parser touched; the S3 legs read the SDK's own parse of
  the header and listing.
- **Test fidelity / DST:** no new destructive or concurrent path; this changes a value
  inside the existing commit, so no new DST scenario. The existing DST suite still runs
  (see gate runs).
- **Commit-ready:** `cargo fmt --all`, `git diff --check`, `typos` on all three files,
  `lint_docs.py` (OK) and `render_site.py --check` (99 pages, link audit OK) are clean.

## For the human at sign-off

- **The rule changed from the brief's `+1 ms` to a whole-second step and gained a one-day
  cap,** because the iteration-2 sign-off asked for both. Three of the brief's exact values
  changed with it (A1/A2 and A5's `u64::MAX` case; see "Where this departs"). Please
  confirm the cap value (one day). It has to stay above one hour to keep the brief's own
  scenario carried. Anything from about 2 h to a few days would be defensible.
- **The visible cost:** a key overwritten more than once a second shows a `Last-Modified`
  ahead of real time, by about (overwrites − elapsed seconds), until writes slow down. On
  such a key, a client sending `If-Unmodified-Since: <now>` right after a burst can get
  412 until the clock catches up. RFC 9110 §8.8.2.1 also says an origin should not send a
  `Last-Modified` later than its `Date`. Any forward-carrying rule has this under skew
  (iteration 2's did too). This one also shows it on a healthy gateway for keys written
  faster than 1/s. The alternative that avoids it (the conditional rule above) leaves the
  skew window open. If you prefer that trade, it is a one-line change in the helper plus
  three expected values in leg 5 and leg 8's expectation.
- The bounded guarantee from the brief (⚑) is unchanged: it resets across an unstamped
  core publication, and no production path publishes unstamped.
- The arch-doc paragraph sits in a section that #894 and #895 also edit; expect a text
  conflict there, not a semantic one.
