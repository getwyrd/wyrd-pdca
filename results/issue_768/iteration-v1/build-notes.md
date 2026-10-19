# Build notes — issue 768 / last-modified-never-decreases

Target: getwyrd/wyrd @ main, built in `$PDCA_WORKTREE` on the stack base
`pdca-integration/main-rc423c4501a6d` @ `0dd47cc` (the bundle's `stack-base-tip`). Line
numbers below are on that tree with the patch applied.

## What changed

- **One pure helper in core**, `metadata::overwrite_modified(prior, published)`
  (`crates/core/src/metadata.rs:2269`): `published?` then `max(published, prior + 1)` with
  `saturating_add`; a `None` prior lets the publisher's stamp stand. It reads no clock — it
  only orders the publisher's stamp against the superseded record's — so it adds no clock
  dependency to core (ADR-0009; the one-clock rule is unaffected: the stamp is still the
  gateway's `now_millis`, and the `+1` is derived from a value that clock wrote).
  Its doc comment states the rule, the `None` reset, saturation, that the multipart Complete
  flip (#658) must call it explicitly (it cannot inherit it from the leased committer), and
  that repair commits keep preserving `modified`.
- **Both public overwrite commits call it** when building `next`:
  `commit_chunk_map_superseding` (`metadata.rs:2218`) and
  `commit_chunk_map_superseding_leased` (`metadata.rs:2330`, the one
  `write::commit_overwrite` calls). Both CAS on `encode(prior)`, so the prior stamp is from
  the generation actually superseded. No server change: both PUT paths reach it through
  `commit_written` → `write::commit_overwrite`.
- **Docs (leg B):** `ObjectMeta::modified` (`metadata.rs:1587`) and `InodeRecord::modified`
  (`metadata.rs:1643`) doc comments; the architecture doc's object-metadata paragraph
  (`docs/design/architecture/05-building-block-view.md:205-215`), which now states the
  rule, the unstamped reset, that only core can publish unstamped, that ETag is the
  content-identity token, and the ADR-0047 sentence. The existing "repairs preserve" text
  sits just above at `:201-204`. No ADR touched.
- **Test:** `crates/server/tests/last_modified_monotonic.rs` (new).
  - (1) `streaming_overwrite_after_a_fast_clock_steps_one_millisecond_past_it` (`:129`)
  - (2) `buffered_overwrite_after_a_fast_clock_steps_one_millisecond_past_it` (`:155`)
  - (3) `overwrite_over_a_past_stamp_records_the_current_clock` (`:175`), both PUT paths
  - (4) `create_records_the_current_clock` (`:200`), both PUT paths
  - (5) `core_overwrite_commits_apply_the_rule_exactly` (`:297`): every case the brief
    lists, on both APIs, plus the 3-in-a-row lead and the chained `None` reset. I added two
    extra `publisher None` cases (over a legacy prior and over `u64::MAX`) to cover
    "whatever the prior".
  Seeding mirrors `crates/server/tests/segmented_object_read.rs:55-80` (`write_flat`),
  setting `plan.object_meta.modified` before `commit_create`, then moving the store into
  `Gateway::new`. Leg 5 uses an in-memory redb store and `read::read_inode` for the
  read-back, with an empty pending set for the leased API (no leases needed).

## What I ruled out

- Stamping at the server's two PUT sites: the brief rejects it (two copies of one rule, a
  third for #658), and it would leave `commit_chunk_map_superseding` as a way around it.
- Putting the logic inline in each committer: same rule twice. A shared `pub fn` costs
  ~6 lines of body and gives #658 one call to make.
- Carrying the prior stamp through a `None` publication: the brief bounds the rule there on
  purpose; leg 5's chained case pins the reset.

## Evidence

Runner: `./engine/scripts/run-verify.sh` (the project's C4-verify script) with
`PDCA_BUNDLE` = this bundle and `PDCA_BRIEF_BASE` = `0dd47cc`.

- Fix applied: `5 passed; 0 failed`.
- Production reverted, test kept: `2 passed; 3 failed` — (1) reported
  `left: Some(1792135574885) right: Some(1792139174839)` (the clock, an hour behind the
  seed), (2) the same, and (5) failed on `Superseding: equal (same millisecond)`
  (`left: Some(1700000000000) right: Some(1700000000001)`). (3) and (4) pass on the base by
  design: they pin that the fix does not change the ordinary case.
- Extra mutant (scratch bundle): helper applied to `commit_chunk_map_superseding` only,
  leased committer left at `meta.modified` → test red on `SupersedingLeased: equal (same
  millisecond)` and on (1)/(2). So leg 5 binds each API on its own, and the server legs
  bind the leased committer specifically.
- Full gate `./engine/xtask.sh ci` (fmt, clippy `-D warnings`, tests incl. DST, deny,
  typos, docs lint + render). Three runs:
  1. clippy `type_complexity` in the test → fixed with a `Case` type alias.
  2. Everything up to `cargo test` green except one test,
     `health_probe::a_hanging_health_probe_fails_closed_within_a_bounded_wait`
     (`crates/server/tests/health_probe.rs:244`, "connect to the configured health
     endpoint within budget: transport error"). It is a 5 s TCP-connect budget to a gRPC
     health listener; the file has no reference to object metadata or overwrite. It passed
     on run 3 unchanged → load flake, unrelated.
  3. typos, docs lint, docs render + link audit, gitlink/unsafe/blackbox guards, fmt,
     clippy, build, **all of `cargo test --workspace --exclude wyrd-dst` (201 test binaries,
     0 failures)**, cargo-machete and `cargo deny check` green. It then stopped at
     `cargo deny --all-features --config deny-all-features.toml check advisories`: RustSec
     advisories against `rustls 0.21.12` (via `tonic 0.10.2`, dev-dep of
     `wyrd-validate`). The patch touches no `Cargo.toml`/`Cargo.lock`, so this is
     pre-existing on the base and not caused by this change; it will be red for every
     bundle on this base until the dependency is bumped.
  DST runs after that step in `ci`, so it was run separately via
  `./engine/xtask.sh dst` (clippy + `cargo test -p wyrd-dst` under `--cfg madsim`):
  exit 0, 10 test binaries ok, no failures. (A first attempt was stopped by the tool's
  time limit with no failures so far; the re-run completed.)

## Refute-my-own-test

- **(a) Genuine red?** Yes. With production reverted the verifier ran the test and 3 of 5
  failed as assertions (numbers above); the file compiles on the base because it only uses
  pre-existing API.
- **(b) Production path?** Yes. Legs 1–4 drive the real `wyrd_server::Gateway` through
  `ObjectGateway::put_object_streaming` / `Gateway::put_object` / `head_object`, which go
  through `commit_written` → `write::commit_overwrite` →
  `metadata::commit_chunk_map_superseding_leased`. Leg 5 calls the two real public core
  commits on a real (in-memory redb) `MetadataStore`. Nothing is mocked or copied.
- **(c) Fixture includes the fault?** Yes, as far as in-process allows. The fault is a prior
  stamp ahead of the publishing gateway's clock; the fixture commits exactly that record
  (one hour ahead of the host clock the gateway reads) through the normal create path. The
  store keeps no record of which gateway wrote a stamp, so this is the same record a
  fast-clocked peer leaves. Real two-host skew is not reproduced (the brief says the
  libfaketime leg is not needed for this). Overwrites use different bytes of the same size
  (`OLD`/`NEW`, both 17 bytes — asserted), the case sync tools miss.

## Rubric self-review

- One clock per lifecycle: the helper reads no clock. The test's `wall_millis`
  (`last_modified_monotonic.rs:48`) reads `SystemTime::now()` under
  `#[allow(clippy::disallowed_methods)]` with a "wall-clock exempt" comment naming why: it
  brackets the gateway's own `now_millis` reading, so it must use the same source.
- Docs currency: the meaning of a persisted field changed, so the architecture doc and both
  field docs are updated in the same patch.
- Serialization identity: no field shape or serde attribute changed; `None` stays `None`.
- Absent entries: a legacy (`None`) prior and an unstamped publication have explicit,
  tested behaviour.
- Test fidelity / DST: no new destructive or concurrent path; this changes a value inside
  the existing commit, so no new DST scenario. The existing DST suite runs in `cargo xtask
  ci`.

## For the human at sign-off

- The brief flags (⚑) that the guarantee is bounded: it holds across consecutive stamped
  overwrites and resets across an unstamped core publication. No production path publishes
  unstamped. Leg 5's chained case pins this.
- The arch doc paragraph lives in a section other in-flight briefs (#894, #895, #767) also
  edit; expect a textual merge conflict there, not a semantic one.
