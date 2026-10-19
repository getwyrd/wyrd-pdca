# Build notes — issue 768 / last-modified-never-decreases (iteration 2)

Target: getwyrd/wyrd @ main, built in `$PDCA_WORKTREE` on the stack base
`pdca-integration/main-rc423c4501a6d` @ `0dd47cc` (the bundle's `stack-base-tip`, unchanged
since iteration 1). Line numbers below are on that tree with the patch applied.

## What the carry-forward asked, and what I did about it

The round-1 rebuild was triggered by one gating row: C4-ci failed at
`cargo deny --all-features --config deny-all-features.toml check advisories`
(RUSTSEC-2026-0346 and RUSTSEC-2026-0349 on `rustls 0.21.12`). Every review leg passed the
patch itself (C1–C3, C5, T1–T5 PASS; C4-verify PASS; C5-mutants 3 caught, 0 survived).

**That failure is on the base, not in this patch.** I re-ran the same advisory check on the
clean worktree at `0dd47cc`, before applying anything:

```
cargo deny --frozen --all-features --config deny-all-features.toml check advisories
-> error[vulnerability] RUSTSEC-2026-0346  rustls 0.21.12 (Cargo.lock:302)
-> error[vulnerability] RUSTSEC-2026-0349  rustls 0.21.12 (Cargo.lock:302)
-> advisories FAILED, exit=1
   Solution: Upgrade to >=0.23.46
```

The patch touches no `Cargo.toml` or `Cargo.lock`. The listed fix (rustls ≥ 0.23.46) is not
reachable while `tikv-client 0.4.0` pins the 0.21 line, so the only way to turn the row green
is a waiver in `deny-all-features.toml` or a dependency change on `main`. Both are policy
decisions outside this slice's scope (the rubric's "out of scope → decline with an issue
reference, not an in-PR fix"). Putting a security-advisory waiver inside a `Last-Modified`
PR would also hide it from the people who should review it. So I did **not** change
dependencies or deny config here.

This instance already knows about it: `pdca.toml:1403-1407` has the doctor row
`cargo deny all-features advisories`, whose hint points at a ready waiver
(`results/issue_620/base-advisory-waiver.diff`) to land on `main` as its own PR. This brief's
`External dependencies` did not name that id, so the pre-dispatch guard did not hold the
bundle. Declaring it for §6:

NEEDS-HUMAN external dependency: cargo deny all-features advisories — the base `0dd47cc` fails `cargo deny --all-features --config deny-all-features.toml check advisories` (RUSTSEC-2026-0346/0349, rustls 0.21.12 via tikv-client 0.4.0) before this patch is applied, so the gating C4-ci row stays red for every bundle on this base until the waiver (results/issue_620/base-advisory-waiver.diff) or a dependency bump lands on main; this patch cannot clear it within scope

The row already exists in `pdca.toml`, so no new row is needed; it only needs naming in the
brief's `External dependencies` so the guard holds the bundle instead of spending a Do:

```toml
[[doctor.checks]]
group = "workspace"
id    = "cargo deny all-features advisories"   # already present at pdca.toml:1403 — name it in the brief
cmd   = 'cd "${WYRD_REPO:-../wyrd}" && cargo deny --frozen --all-features --config deny-all-features.toml check advisories >/dev/null 2>&1'
hint  = "land the deny-all-features.toml waiver for RUSTSEC-2026-0346/0349 (results/issue_620/base-advisory-waiver.diff) on main, update ../wyrd, re-run"
level = "WARN"
```

**What changed in the patch this round.** One sentence in the architecture doc. The
adversary showed (iteration-v1 `check-advisory-adversary.md`) that on this base #767 has
landed, so every S3 surface renders whole seconds
(`docs/design/architecture/05-building-block-view.md:196-198`). The `+1 ms` step therefore
usually shows on the wire as the *same* second, not a later one. The previous doc text was
true but let a reader assume "strictly later". I added
(`05-building-block-view.md:209-211`): "Because every S3 surface reports whole seconds
(above), the `+1 ms` step usually lands in the same second: on the wire a later
generation's `Last-Modified` is never earlier than the one it replaced, but it can be
equal." I did **not** change the rule itself (for example, bumping to the next whole
second, or capping how far ahead a prior may be carried). The maintainer chose the rule at
Plan, and the adversary's two design findings are held in `deferred-findings.json` for the
human. They are not build work.

The code and the test are byte-identical to iteration 1, which every review leg passed.

## What changed (whole patch)

- **One pure helper in core**, `metadata::overwrite_modified(prior, published)`
  (`crates/core/src/metadata.rs:2269`): `published?`, then `max(published, prior + 1)` with
  `saturating_add`; a `None` prior lets the publisher's stamp stand. It reads no clock. It
  only orders the publisher's stamp against the superseded record's, so it adds no clock
  dependency to core (ADR-0009). The stamp is still the gateway's `now_millis`, and the
  `+1` comes from a value that same kind of clock wrote. Its doc comment
  (`metadata.rs:2245-2268`) states the rule, the `None` reset, saturation, that the
  multipart Complete flip (#658) must call it explicitly, and that repair commits keep
  preserving `modified`.
- **Both public overwrite commits call it** when building `next`:
  `commit_chunk_map_superseding` (`metadata.rs:2218`) and
  `commit_chunk_map_superseding_leased` (`metadata.rs:2330`, the one
  `write::commit_overwrite` calls). Both CAS on `encode(prior)`, so the prior stamp is from
  the generation actually superseded. No server change: both PUT paths reach it through
  `commit_written` → `write::commit_overwrite`.
- **Docs (leg B):** `ObjectMeta::modified` (`metadata.rs:1587-1594`) and
  `InodeRecord::modified` (`metadata.rs:1643-1653`) doc comments; the architecture doc's
  object-metadata paragraph (`docs/design/architecture/05-building-block-view.md:205-218`).
  It states the rule, the wire-granularity limit (new this round), the unstamped reset,
  that only core can publish unstamped, that ETag is the content-identity token, and the
  ADR-0047 sentence. The existing "repairs preserve" text sits just above at `:201-204`.
  No ADR touched.
- **Test:** `crates/server/tests/last_modified_monotonic.rs` (new).
  - (1) `streaming_overwrite_after_a_fast_clock_steps_one_millisecond_past_it` (`:129`)
  - (2) `buffered_overwrite_after_a_fast_clock_steps_one_millisecond_past_it` (`:155`)
  - (3) `overwrite_over_a_past_stamp_records_the_current_clock` (`:175`), both PUT paths
  - (4) `create_records_the_current_clock` (`:200`), both PUT paths
  - (5) `core_overwrite_commits_apply_the_rule_exactly` (`:297`): every case the brief
    lists, on both APIs, plus the 3-in-a-row lead and the chained `None` reset. Two extra
    `publisher None` cases (over a legacy prior and over `u64::MAX`) cover "whatever the
    prior".
  Seeding mirrors `crates/server/tests/segmented_object_read.rs:55-80` (`write_flat`):
  set `plan.object_meta.modified` before `commit_create`, then move the store into
  `Gateway::new`. Leg 5 uses an in-memory redb store and `read::read_inode` to read back,
  with an empty pending set for the leased API (no leases needed).

## What I ruled out

- **Fixing the advisory row inside this patch** (a waiver line in `deny-all-features.toml`,
  about 6 lines, or a dependency bump). Out of scope, a security-policy call, and it would
  conflict with the separate waiver PR the instance already has ready. See above.
- **Changing the rule to whole-second steps** (`max(t, (p / 1000 + 1) * 1000)`), which
  would make the step visible on the wire. It is a one-line change to
  `overwrite_modified`, plus updated exact values in leg 5 and legs 1–2. But it replaces
  the maintainer's decided `+1 ms` rule, and the adversary itself notes its cost (a burst of
  same-second overwrites builds a lead of seconds). Deferred to the human, as the driver
  already recorded.
- **Stamping at the server's two PUT sites:** the brief rejects it (two copies of one rule,
  a third for #658), and it would leave `commit_chunk_map_superseding` as a way around it.
- **Putting the logic inline in each committer:** the same rule twice. A shared `pub fn`
  costs about 6 lines of body and gives #658 one call to make.
- **Carrying the prior stamp through a `None` publication:** the brief bounds the rule there
  on purpose; leg 5's chained case pins the reset.

## Evidence (this round)

Runner for red→green: `./engine/scripts/run-verify.sh` (the project's C4-verify script)
with `PDCA_BUNDLE=results/issue_768` and `PDCA_BRIEF_BASE=0dd47cc…`.

- Exit 0: `run-verify.sh: PASS — red without the fix, green with it (5 test(s) ran red).`
- Fix applied: `5 passed; 0 failed`.
- Production reverted, test kept: `2 passed; 3 failed`, all three as assertion failures:
  - (1) `last_modified_monotonic.rs:143`: `left: Some(1792139800708) right: Some(1792143400660)`
    (the gateway's clock, an hour behind the seed);
  - (2) `:164`: `left: Some(1792139800706) right: Some(1792143400661)`;
  - (5) `:312`: `left: Some(1700000000000) right: Some(1700000000001)` (the
    same-millisecond case, first in the table).
  - (3) and (4) pass on the base by design: they pin that the fix leaves the ordinary
    case alone.

### Gate runs (this round, worktree with the patch applied)

- `./engine/xtask.sh ci`: typos; `lint_docs.py` OK; `render_site.py --check` (99 pages,
  link audit OK); gitlink, unsafe and blackbox guards; `cargo fmt --check`; clippy; build;
  **`cargo test`: 201 test binaries, 1,715 passed, 14 ignored, 0 failed** (all five
  `last_modified_monotonic` tests ok); cargo-machete clean; `cargo deny check`
  (`advisories ok, bans ok, licenses ok, sources ok`). It then stopped at the second deny
  step, `cargo deny --all-features --config deny-all-features.toml check advisories`:
  `advisories FAILED` on RUSTSEC-2026-0346/0349. That is the same failure the clean base
  gives (above), so it is not caused by this patch.
- `./engine/xtask.sh dst` (run on its own, because `ci` runs DST only after the advisory
  step): clippy under `--cfg madsim`, then `cargo test -p wyrd-dst`: **10 test binaries,
  85 passed, 0 failed, exit 0**. This includes the long `committed_regression_seeds_stay_green`
  campaign that the round-1 reviewer's run was cut off in.
- Raw logs (harness scratch, reclaimed by the driver): `$PDCA_SCRATCH/pdca-builder-768-verify.log`,
  `pdca-builder-768-ci.log` (advisory failure at its last lines), `pdca-builder-768-dst.log`.

## Refute-my-own-test

- **(a) Genuine red?** Yes. `run-verify.sh` reverted the production change, kept the test,
  and 3 of 5 tests failed as assertions (numbers above). The file compiles on the base
  because it only uses pre-existing API, so the red is a real failed check, not a build
  error.
- **(b) Production path?** Yes. Legs 1–4 drive the real `wyrd_server::Gateway` through
  `ObjectGateway::put_object_streaming` / `Gateway::put_object` / `head_object`. Those go
  through `commit_written` → `write::commit_overwrite` →
  `metadata::commit_chunk_map_superseding_leased`. Leg 5 calls the two real public core
  commits on a real (in-memory redb) `MetadataStore`. Nothing is mocked or copied.
- **(c) Fixture includes the fault?** Yes, as far as in-process allows. The fault is a
  prior stamp ahead of the publishing gateway's clock. The fixture commits exactly that
  record (one hour ahead of the host clock the gateway reads) through the normal create
  path. The store keeps no record of which gateway wrote a stamp, so this is the same
  record a fast-clocked peer leaves. Real two-host skew is not reproduced (the brief says
  the libfaketime leg is not needed). Overwrites use different bytes of the same size
  (`OLD`/`NEW`, both 17 bytes, asserted at `:130-134`), the case sync tools miss.
- **What the test does not prove:** it reads raw milliseconds through `head_object`, not
  the rendered header. So it cannot show what a client sees on the wire. That limit is now
  stated in the doc (above) and is the adversary's deferred design finding.

## Rubric self-review

- One clock per lifecycle: the helper reads no clock. The test's `wall_millis`
  (`last_modified_monotonic.rs:48`) reads `SystemTime::now()` under
  `#[allow(clippy::disallowed_methods)]` with a "wall-clock exempt" comment
  (`:45-47`) saying why: it brackets the gateway's own `now_millis` reading, so it must use
  the same source.
- Docs currency: the meaning of a persisted field changed, so the architecture doc and both
  field docs are updated in the same patch.
- Serialization identity: no field shape or serde attribute changed; `None` stays `None`.
- Absent entries: a legacy (`None`) prior and an unstamped publication have explicit,
  tested behaviour.
- Transactions: no new early return; the helper is pure and runs before the batch is built.
- Test fidelity / DST: no new destructive or concurrent path; this changes a value inside
  the existing commit, so no new DST scenario. The existing DST suite still runs.
- Commit-ready: `cargo fmt --all -- --check`, `git diff --check` and `typos` on all three
  files are clean.

## For the human at sign-off

- **C4-ci will be red again** for the base advisory reason above, unless the waiver has
  landed on `main` first. The NEEDS-HUMAN line above is the item to clear.
- The brief flags (⚑) that the guarantee is bounded: it holds across consecutive stamped
  overwrites and resets across an unstamped core publication. No production path publishes
  unstamped. Leg 5's chained case pins this.
- The adversary's two design findings (the step is usually invisible at whole-second wire
  precision; a far-future prior is carried forward with no cap) are in
  `deferred-findings.json`. Only the first is reflected here, and only as a doc sentence
  that states current behaviour. Either one needs your call before a rule change.
- C4-diff-cov reported "executed 0 tests" in round 1 because the coverage checkout
  (`/home/eddie/wyrd/wyrd-cov`) was on an older base with a 4-argument
  `put_object_streaming`; this target has 5 arguments
  (`crates/gateway-core/src/lib.rs:329-337`). That is a harness checkout problem, not the
  test. It belongs upstream (eduralph/pdca-harness) if it recurs.
- The arch doc paragraph lives in a section other in-flight briefs (#894, #895) also edit;
  expect a text conflict there, not a semantic one.
