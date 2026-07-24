# build-notes.md — issue 507 / list-objects-v2 (iteration 4)

## Scope of this iteration

The Iteration-3 carry-forward accepts the mainline (red→green independently corroborated;
pagination/token/encoding/denylist probes unrefuted) and says: **keep the approach, fix only
the one landed refutation plus the two minor advisory observations**. I applied the preserved
`iteration-v3/patch.diff` to the worktree unchanged, then made exactly the three targeted
changes below. Nothing else in the design changed.

Base: `getwyrd/wyrd @ main`, worktree at `$PDCA_WORKTREE` = `07d0244` (wave base). Patch
regenerated with `git diff` over the full change set (iteration-v3 + the three fixes).

## Fix 1 (the landed refutation) — client-chosen resume inside a delimiter group

**Defect (adversary, iteration-3):** the group-skip predicate in `compute_page`
(`crates/gateway-s3/src/lib.rs:537` in iteration-3) was
`resume_after.is_some_and(|r| cp.as_str() <= r)`. It treats *any* resume value ≥ the common
prefix as "group already consumed". That is only correct for the two **server-issued** resume
values (the v2 token = the group's last raw key `a/2`; the v1 `NextMarker` = the common prefix
`a/`). A **client-chosen** resume — v2 `start-after` or v1 `marker` — is an arbitrary key that
can land *strictly inside* the group (`a/1`). Concrete failure: bucket `{a/1, a/2, b}`,
`?list-type=2&delimiter=/&start-after=a/1` returned `Contents=[b], CommonPrefixes=[]` — AWS
applies the resume to the raw keyspace *before* rollup, so `a/2 > a/1` survives and rolls up →
`CommonPrefixes=[a/], Contents=[b]`. `a/2` was neither listed nor represented by a rollup on
*any* page: silent data invisibility.

**Fix (local to `compute_page`, `crates/gateway-s3/src/lib.rs:535-582` on the target
worktree):** replace the single `cp <= resume` test with the exact rule the carry-forward
prescribes:

- Delimit the group's extent `[group_start, group_end)` and read its `last_raw` key
  (`lib.rs:540-546`).
- `group_consumed = resume == cp || resume >= last_raw` (`lib.rs:556-557`) — collapse the whole
  group *only* for the two server-issued shapes (or any resume past the group's last key).
- Otherwise filter the group's raw keys individually: `survives` iff at least one key is
  strictly `> resume` (`lib.rs:560-566`); if any survives, emit the `a/` rollup once.
- The v2 token payload (`next_key`) stays the group's `last_raw` even when a client resume
  landed inside the group (`lib.rs:582`-region), so the *next* page's server-issued token
  (`>= last_raw`) collapses the whole group via `group_consumed` — pagination is unchanged and
  a common prefix is still never double-emitted across pages.

This preserves every previously-unrefuted behaviour: v2 token chaining (token = `last_raw`,
resume `>= last_raw` → skip), v1 `NextMarker` = `a/` resume (`resume == cp` → skip), plain
content-key resume (unchanged branch), `max-keys=0`, and the delimiter+max-keys truncation
chain.

**New tests (the existing start-after test was flat/no-delimiter and could not catch this):**
- `list_v2_start_after_inside_delimiter_group_still_rolls_up_survivors` — the exact adversary
  repro over the wire (`?delimiter=/&start-after=a/1` on `{a/1,a/2,b}` → `CommonPrefixes=[a/]`,
  `Contents=[b]`).
- `list_v1_marker_inside_delimiter_group_still_rolls_up_survivors` — same via the v1
  `?delimiter=/&marker=a/1` path.

## Fix 2 (minor observation) — the "raw match is sufficient" comment on the object-path fence

The adversary noted the object path deliberately keeps **raw** (un-decoded) subresource
matching and the iteration-3 comment claimed a raw match "is sufficient", which is stronger
than the code warrants: a doubly/percent-encoded `PUT /b/k?part%4Eumber=1` dodges the raw
match. The adversary explicitly did **not** press it (only a fully-credentialed, deliberately
encoding client can reach it, and such a client can issue the plain verb anyway — it is not a
listing-disclosure like the bucket-route gap). So per the carry-forward I **corrected the
comment**, not the behaviour: `unsupported_subresource`'s doc now states the residual honestly
and explains why a raw match is adequate *here* (object path → plain verb) but not on the
bucket route (→ listing) which uses `unsupported_subresource_decoded`
(`crates/gateway-s3/src/lib.rs:375-384`).

I did **not** change the object-path fence to decode: that is an object-verb behaviour change
outside #507's scope (it would touch the PUT/GET/DELETE paths that 508/509/510 also edit) for a
non-exploitable residual. Cost of the rejected alternative: adding a decoded pass on the object
path is ~1 extra call + a behaviour change on three verbs shared with peer bundles; the
prescribed action was a comment correction (~7 lines, zero behaviour change).

## Fix 3 (minor observation) — the unused `BucketRecord` struct

`BucketRecord` (`crates/core/src/metadata.rs:346` in iteration-3) was added but referenced by
no production code: `list_container` checks marker **presence** only (a plain `get().is_none()`
— `crates/server/src/lib.rs`), and the test seeds raw JSON. The adversary flagged it as
speculative #511 API with drift risk. The carry-forward says "use or drop". I **dropped** it:
writing `bucket:{name}` markers (and therefore owning the record's shape) is explicitly out of
#507's scope (brief line 53 — that is #511's job), and #507 only ever reads the marker for
existence, which is a plain `get`. Dropping removes the unused code and the drift risk; the
`bucket_key(name)` helper (which *is* used by `list_container`) stays. The test's seeded JSON
is raw bytes and references no production symbol, so it is unaffected.

## Carry-forward items from iterations 1-2 (left intact, still satisfied)

`max-keys=0` → empty non-truncated page; percent-decoded denylist on the bucket route (incl.
the five bucket-level subresources `versioning`/`intelligent-tiering`/`ownershipControls`/
`policyStatus`/`metadataTable`); delimiter+max-keys chaining under truncation without prefix
double-emit; `start-after` honoured (flat case); `encoding-type` refused `501`; `list-type≠2`
→ `400`; v1 `NextMarker` = common prefix with whole-group resume-skip; the typos reword of
`mis-decoding`. Open question (brief): stock `aws-cli ls`/`sync` need neither `start-after` nor
`encoding-type`; `start-after` is implemented (now correct inside delimiter groups),
`encoding-type` is refused loudly.

## Refute-your-own-test (forced, recorded)

- **(a) Genuine red?** Yes, two independent legs:
  - *Whole-feature red (C4-verify):* production reverted, all added tests kept → **17/17 fail
    by assertion** on the wave base (bucket GET → `400 InvalidRequest`, `split_bucket_key`
    returns `None`) — no compile-error red (the test imports no new production symbol).
  - *Fix-specific red (this iteration's change):* I reverted **only** the `compute_page`
    group-skip predicate back to iteration-3's `cp <= resume` (keeping the rest of the feature)
    and ran the two new tests: both fail with exactly the landed symptom —
    `left: [], right: ["a/"]` ("must still roll up the surviving a/2 into a/"). Restored the
    fix (verified byte-identical) and they pass. So the two new tests bind *this* fix, not just
    the feature at large.
- **(b) Production path?** Yes. Every assertion drives the shipping HTTP surface through a
  stock `aws-sdk-s3` client over a real loopback listener
  (`list_objects_v2().delimiter().start_after()`, `list_objects().delimiter().marker()`)
  against the production `S3Gateway` + `Gateway` + `RedbMetadataStore` stack. No production
  symbol is re-implemented in the test; the ETag oracle is an independent SHA-256.
- **(c) Fixture includes the fault?** Yes. The two new tests seed a real bucket marker and PUT
  the real keys `{a/1, a/2, b}`, then issue the exact request whose resume point lands *inside*
  the `a/` group (`start-after=a/1` / `marker=a/1`) — the fixture contains the survivor `a/2`
  the old code hid, so a regression re-drops it and the test goes red.

## Verification run (project runner)

- `./engine/scripts/run-verify.sh` (the C4-verify gate, on the regenerated `patch.diff`) →
  **PASS — red without the fix, green with it** (17 tests: 17 fail on base, 17 pass with fix).
- `cargo fmt -- --check` → clean (fmt applied to the two new tests, patch regenerated).
- `cargo clippy -p wyrd-gateway-s3 -p wyrd-server -p wyrd-gateway-core -p wyrd-core --tests
  -- -D warnings` → clean.
- `typos` over the four touched files → clean.

These are the fmt/clippy/typos/test legs the previously-red-then-green `cargo xtask ci` (C4-ci)
gate mirrors, so the target's own commit hooks have nothing to reject. I did **not** re-run the
full `cargo xtask ci` here (Check's gate runs it deterministically); the legs this patch could
affect are individually green.

## No external dependency gap

Everything ran in-process on the base toolchain (redb-in-memory + fs-tempdir + loopback). No
NEEDS-HUMAN external dependency. The off-Check `aws s3 ls`/`sync` acceptance (which needs a
marker backfill or #511, and the `aws` CLI doctor row) is unchanged from prior iterations and
is the human's Check-time manual step, not a build blocker.
