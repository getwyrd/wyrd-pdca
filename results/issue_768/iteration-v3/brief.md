# Design proposal — issue 768 / last-modified-never-decreases

> The Plan artifact (design-proposal form). Do reads ONLY this file. The `- **Label:** value`
> lines are parsed by the driver — keep their shape.
>
> The tracker labels #768 a bug, and its text asks for the ordering of `Last-Modified` to be
> decided before it is implemented. **Decided at Plan (2026-10-14, by the maintainer):** the
> issue's option 1. **No new ADR.** ADR-0047's `modified` is the **file's** last-modified time,
> a file-level property. A rule that keeps it from going backwards across overwrites of the same
> file stays inside that intent. The per-gateway clocks and the `+1 ms` are implementation detail
> below the level ADR-0047 decides. The rule is recorded in the living architecture doc and in the
> code's doc comments. Citations verified against `getwyrd/wyrd` `origin/main` @ `7bf7eb9` on
> 2026-10-14.

- **Slug:** last-modified-never-decreases
- **Track:** alpha
- **Kind:** enhancement (design proposal)
- **Goal:** A file's `Last-Modified` never goes backwards across an overwrite, whichever gateway
  of an active/active fleet performs it. Today each gateway stamps `modified` from its own clock
  (`now_millis`, `crates/server/src/lib.rs:813-823`) at both PUT paths (`:251-255` buffered,
  `:402-406` streaming). `commit_written` reads the prior record (`:269-277`) and
  `write::commit_overwrite` (`crates/core/src/write.rs:314-335`) passes `plan.object_meta`
  through unchanged. No code compares the new stamp with the one it replaces, and
  `InodeRecord::checked_shape` validates size and span, not time. The fleet sits behind a
  round-robin balancer with no affinity (`docs/design/architecture/m4-first-deployment-blueprint.md`
  §B.1; three gateways in `deploy/small-multi-node-fdb/docker-compose.yml`; active/active per
  #477). There, a gateway whose clock runs 2 s fast followed by an accurate one makes
  `Last-Modified` go backwards while the content moves forwards. `aws s3 sync` then skips the
  newer file when the bytes have the same size: silent divergence.
- **Success criterion:** **(A) The rule holds.** A NEW test drives the real `wyrd_server::Gateway`
  through the pre-existing `ObjectGateway` API:
  (1) seed a file whose committed `modified` is **one hour ahead** of the test's wall clock (the
  record a fast-clocked gateway leaves behind), then overwrite it through `put_object_streaming`
  with **different bytes of the same size**: afterwards `head_object` reports **exactly**
  `modified == seeded + 1` (not merely `>=`; an implementation that jumps to `u64::MAX` or any
  other value must fail) and the new ETag;
  (2) the same through the buffered `Gateway::put_object`, also **exactly** `seeded + 1`;
  (3) an overwrite of a file whose prior stamp is in the **past** records the current clock
  (between readings taken just before and just after the call), not `prior + 1`;
  (4) a **create** records the current clock, as today;
  (5) **the rule's exact values, on both core overwrite APIs.** The server legs cannot choose the
  publisher's clock reading, so this leg calls core directly, with an explicit publisher stamp
  `t`, through **both** public overwrite commits: `metadata::commit_chunk_map_superseding`
  (`crates/core/src/metadata.rs:2199`) and `metadata::commit_chunk_map_superseding_leased`
  (`:2268`, the one `write::commit_overwrite` calls; an empty pending set needs no leases,
  `:2426-2444`). Peer: `crates/core/tests/mutation_regressions.rs:446-460` overwrites a seeded
  record this way. For each API, the committed `modified` is:
  - prior `Some(t)`, publisher `Some(t)` (**equal**: two writes in one clock millisecond) →
    `Some(t + 1)`;
  - prior `Some(t + 5_000)` → `Some(t + 5_001)`;
  - prior `Some(t - 5_000)` → `Some(t)`;
  - prior `None` (a legacy record) → `Some(t)`;
  - publisher `None` → `None`, whatever the prior;
  - prior `Some(u64::MAX)` → `Some(u64::MAX)`: saturates, no panic, no wrap to 0;
  - three overwrites in a row, all with publisher `Some(t)`, over prior `Some(t)` →
    `t + 1`, `t + 2`, `t + 3` (the accumulated lead described under Impact, pinned as intended);
  - **the chain resets across an unstamped publication, pinned as intended:** over prior
    `Some(t + 5_000)`, an overwrite with publisher `None` commits `None`; a further overwrite
    with publisher `Some(t)` over that commits `Some(t)`, lower than the `t + 5_000` two
    generations back. This is the documented boundary of the rule (see Invariant and Design),
    not a defect. An implementation that carried `t + 5_000` forward through the `None`
    generation, or stamped that generation itself, fails this case.
  **(B) The rule is stated where readers look.** It is in the living architecture doc's
  object-metadata paragraph (`docs/design/architecture/05-building-block-view.md:197-200`) and in
  the doc comments of the `modified` fields (`crates/core/src/metadata.rs:1598-1600` on
  `ObjectMeta`, `:1648-1653` on `InodeRecord`). It says:
  - across overwrites that each record a time, a file's `modified` never decreases: such an
    overwrite records `max(publishing clock, prior modified + 1 ms)`;
  - a generation published with no time records none, and the overwrite after it starts from
    its own clock. Only core's API can publish without a time; both gateway PUT paths always
    stamp one;
  - repairs still preserve it unchanged;
  - `ETag` is the content-identity token;
  - **in the architecture doc only:** this rule is within ADR-0047's file-level meaning of
    `modified` (the file's last-modified time, `docs/design/adr/0047-object-metadata-model.md:36`).
    The `+1 ms` adjustment for a skewed clock is an implementation detail below that decision,
    not a change to it. This sentence is there so a reader comparing ADR-0047's wording
    ("content-publication time") with the code finds the reasoning without a new ADR.

  No ADR is added or edited.
- **Falsifiability:** RED on `origin/main`, in-process, on the plain `cargo test -p wyrd-server`
  target. (A1) and (A2) record the test's clock, an hour **behind** the seeded stamp, so the
  exact-value assertion fails. (A5) fails on `main` for the equal, ahead, saturation and
  repeated cases, because both APIs store the publisher's stamp as given
  (`crates/core/src/metadata.rs:2218`, `:2297`). The test uses only pre-existing API (it seeds
  through `wyrd_core::write` before `Gateway::new`, then calls `ObjectGateway` methods; A5 calls
  the two public `metadata::` commits), so C4-verify compiles it on the reverted base. Real two-host skew is not reproducible in-process, because `now_millis` reads
  the host clock. The seeded future stamp is a faithful stand-in: the store keeps no record of
  which gateway wrote a stamp, so a prior stamped by a fast clock looks exactly like this one.
  The registered `libfaketime (Tier-1 skew leg)` is not needed.
- **Invariant to restore:** For every file (key), when an overwrite carries a `modified` stamp and
  the generation it supersedes has one, the new stamp is ≥ the superseded one. **Bounded on
  purpose:** a publication with no stamp breaks the chain. Core admits one (a `WritePlan` left at
  its default commits no metadata, `crates/core/src/write.rs:63-68`), and it leaves no time to
  order against. Carrying the prior's time forward through it, or inventing one, would put a time
  on content its publisher never stamped, which the gateway refuses to do for the same reason
  (`crates/gateway-s3/src/lib.rs:4564-4567`). No production path publishes without a stamp: both
  gateway PUT paths set `Some(now_millis())` (`crates/server/src/lib.rs:251-255`, `:402-406`), and
  they reach core's overwrite only through `commit_written` (`:269-277`). So every production
  overwrite chain is unbroken. This holds on every path that publishes **over** a prior
  generation. Today that is both of core's public overwrite commits
  (`metadata::commit_chunk_map_superseding`, `crates/core/src/metadata.rs:2199`, and
  `metadata::commit_chunk_map_superseding_leased`, `:2268`) and therefore both PUT paths. The
  multipart Complete flip (#658) is a future path that must **adopt** the rule explicitly: 0016
  forbids it from reusing the leased committer (`0016:756`), so it cannot inherit the rule from
  that function. So the rule cannot be met by guarding one PUT path. Repair commits keep preserving
  `modified` unchanged (ADR-0047, "Which commits set metadata",
  `docs/design/adr/0047-object-metadata-model.md:53-61`). **Sources:** ADR-0047's file-level
  meaning of `modified`, the file's content-publication time (`0047-object-metadata-model.md:36`);
  the issue's failure analysis (sync tools compare size and mtime); the gateway's own stated
  reason for never fabricating a stamp, "sync tools compare `LastModified` to decide whether to
  transfer" (`crates/gateway-s3/src/lib.rs:4565-4567`). ADR-0024 imposes no skew bound on this
  path (`docs/design/adr/0024-clock-and-time-source-trust.md`, status Proposed).
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:**
- **Conflicts with:** 894, 895, 671, 767
- **Ordering note:** No build-on dependency. (An earlier draft depended on #767 because a new ADR
  was going to describe `Last-Modified` as a whole; with no ADR, that edge is gone. This brief's
  wire statements hold on `main` with or without #767; see Design.)
  `Conflicts with 894, 895`: both edit the metadata section of
  `docs/design/architecture/05-building-block-view.md` (this one `:197-200`, #894 and #895 `:202`).
  **Re-pointed 2026-10-15:** #656 was split at its re-plan into #894 (Create) and #895 (Abort),
  and both carry its edit to `:202`, so the edge names both in place of `656`.
  `Conflicts with 767`: #767 now adds a sentence on rendering to the same object-metadata
  paragraph (near `:189-194`).
  `Conflicts with 671`: both may touch the gateway's PUT commit path
  (`crates/server/src/lib.rs:234-410`). So this runs in a different wave from #656 and #767
  (both wave 0) and from #671. **Intake-cap override (wyrd-pdca-P1):** the human allowed all seven ids of this
  batch (656, 666, 671, 767, 768, 620, 686) in over the alpha cap in the 2026-10-14 Plan session.
  Count at that moment: alpha 4/6 planned (508, 625, 633, 654), room 2, need 7.
- **Scope:** making every overwrite publication keep a file's `Last-Modified` from going
  backwards, and stating that rule in the architecture doc and the `modified` field docs / out of
  scope: any new or edited ADR (the rule is within ADR-0047's file-level intent); a clock-skew
  bound or NTP requirement (option 2); the pre-epoch clock that stamps `modified: Some(0)` on a
  **create** (`now_millis`'s `.unwrap_or(0)`, `crates/server/src/lib.rs:819-822`; see Open
  questions); the listing's rendering granularity (#767); the multipart Complete flip (#658 builds
  it and must call the shared rule this slice adds; it does not inherit it); lease stamps (the
  same `now_millis` feeds them, and they keep their meaning).
- **Difficulty:** medium
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/server/tests/last_modified_monotonic.rs` — a **NEW** file (C4-verify
  treats only an added `crates/*/tests/*.rs` as the per-fix discriminator).
- **Citations expected:** Do must cite `path:line` on the target branch for every change.
  **Peer harness:** seed before the store moves into `Gateway::new`, and write the prior
  generation through `wyrd_core::write` with `plan.object_meta.modified` set, as
  `crates/server/tests/segmented_object_read.rs:55-80` (`write_flat`) does. The file's dirent name
  is `"{bucket}/{key}"` under `ROOT` (`crates/server/src/lib.rs:708-712`), and a key with no `/`
  works for the `ObjectGateway`-level calls. If the test needs a store handle after
  `Gateway::new`, the cloneable in-test store at `crates/server/tests/gateway_multi_writer.rs:58`
  is the precedent. The metadata trio is committed atomically with the chunk map by
  `metadata::commit_chunk_map_superseding_leased` (called from `crates/core/src/write.rs:321-335`),
  which CAS-es on the exact prior bytes. A stamp derived from that same `prior` is therefore
  derived from the generation the commit actually supersedes.
- **Prior-art check (triage cycles):** searched by path `crates/server/src/lib.rs`
  (`now_millis`, `commit_written`), `crates/core/src/write.rs` (`commit_overwrite`) and
  `docs/design/adr/0047-object-metadata-model.md`. ADR-0047 (PR #594, merged 2026-07-18)
  introduced `modified` with no ordering rule. Proposal 0017 (`5682572`) documents this gap and
  cites #768. Open PRs on getwyrd/wyrd: none. Closed or rejected PRs on #768: none.
- **Disposition hint:** likely-fix

Plan-review response: all four findings checked against `7bf7eb9` and revised in place.
(1) Impact and Design now cover equal-millisecond stamps and the write-burst lead, and (A5)
tests the equal and repeated cases. (2) (A1)/(A2) require exactly `seeded + 1`; (A5) adds the
`u64::MAX` saturation case and the other exact values. (3) Both public overwrite commits
(`metadata.rs:2199`, `:2268`) share one core helper; #658 must call it explicitly, since
`0016:756` rules out inheriting it from the leased committer. (4) The wire text now separates the
header (whole seconds on `main`) from listings (milliseconds on `main`, whole seconds only if
#767 lands). Also added `Conflicts with: 767`, because #767's revision edits the same doc paragraph.

Plan-review response (second review, 2026-10-15): the unstamped-publication finding holds and is
revised in place by **bounding** the guarantee, not by changing the `None` policy. The invariant,
the doc wording (leg B, Design § Docs) and Design § The rule now say the order holds across
consecutive stamped overwrites and resets across a publication with no stamp; A5 pins the chained
case. Reason for bounding rather than redesigning: the only way to keep an unbroken order through
a `None` generation is to carry the old time forward or invent one, and both stamp content its
publisher did not, which the gateway already refuses (`gateway-s3/src/lib.rs:4564-4567`). Checked
at `7bf7eb9`: both production overwrite callers stamp `Some` (`server/src/lib.rs:251-255`,
`:402-406`), so the reported gateway defect is fully fixed. ⚑ Human: confirm the bounded
guarantee at sign-off; an unconditional core guarantee would need a design change to the `None`
policy.

## Motivation

`Last-Modified` is the field sync and mirror tools trust to decide whether to transfer, and
Wyrd's deployment shape (N gateways, no affinity) makes consecutive writes to one file land on
different clocks as a matter of routine. No internal clock-sync requirement is recorded:
ADR-0047 states no clock assumption, and ADR-0024 does not bound skew on this path. Today the
field can move backwards while content moves forwards, and the size half of a sync tool's check
cannot catch a same-size overwrite. The blackbox validation tool's `sync` workflow (proposal
0017, #735) is blocked on the same gap.

## Design

**The rule.** When an overwrite publishes over a prior generation whose `modified` is `Some(p)`
and the publisher's stamp is `Some(t)`, the committed stamp is `max(t, p + 1)`. If the prior has
no stamp (a legacy record, ADR-0047's `None`), the publisher's stamp stands. A publisher that
sets no metadata keeps setting none, so a later stamped overwrite has nothing to order against
and records its own clock: the rule holds across consecutive stamped overwrites, not across an
unstamped one (leg A5's chained case). No production path publishes without a stamp (see
Invariant). A create has no prior and is unchanged. The `+ 1` is
saturating: `p` can be any `u64` (ADR-0045 decodes any value), and a prior of `u64::MAX` stays
`u64::MAX`. It was already past the year-9999 bound both renderers apply
(`crates/gateway-s3/src/lib.rs:632-633`, `:3195-3196`), so the S3 surface omits it before and
after; the rule does not make it worse.

**When the stamp differs from the clock.** It is not only skew. `now_millis` reads whole
milliseconds (`crates/server/src/lib.rs:813-823`), so on a fleet with accurate clocks two
overwrites of one file inside the same millisecond have `t == p`, and the second records
`p + 1`. A burst of overwrites faster than one per millisecond therefore builds a lead of about
(overwrites − elapsed milliseconds), which drains once writes slow down. In general the lead at
any moment is bounded by the largest clock skew in the fleet **plus** that write-burst lead, not
by skew alone. Leg (A5) pins the equal and repeated cases.

**What the wire shows.** Stored precision stays in milliseconds. On `main`, the two S3 surfaces
render it differently:
- the `Last-Modified` **header** (GET/HEAD) renders whole seconds (`http_date`,
  `crates/gateway-s3/src/lib.rs:3186-3188`), so two overwrites in one second can show the same
  header value: never decreasing, not strictly increasing;
- a **listing**'s `<LastModified>` renders milliseconds (`iso8601`, `:624-636`), so the
  `+ 1 ms` adjustment is visible there (`.751Z` after `.750Z`).
If #767 lands, listings also render whole seconds and behave like the header. This slice
depends on neither, and its tests read `modified` through `head_object` and core, not through a
rendered string.

**Where.** One pure helper in `wyrd-core` computes the stamp from the prior record and the
publisher's `ObjectMeta`, and **both** public overwrite commits call it when they build the new
record: `metadata::commit_chunk_map_superseding` (`crates/core/src/metadata.rs:2199`, the stamp
at `:2218`; it has no production caller today, only tests, but it is public and must not be a
way around the rule) and `metadata::commit_chunk_map_superseding_leased` (`:2268`, the stamp at
`:2297`; the one `write::commit_overwrite` calls, `crates/core/src/write.rs:314-335`). Both
already hold `prior`. Do not put it in each PUT path of the server. Core already takes time as a
parameter (`crates/server/src/lib.rs:811-812`), so this adds no clock dependency and stays
DST-friendly (ADR-0009). The multipart Complete flip (#658) will publish through its own
committer, because 0016 lists reusing the leased one as a wrong implementation (`0016:756`); it
must call the same helper, and this slice's doc comment on the helper says so. The repair
commits (reconstruction, backfill, rebalance, `metadata::commit_chunk_map`) do not go through
this path and keep preserving the stamp.

**Docs** (`AGENTS.md:154-158`, docs currency: a change to what a persisted field means updates the
living architecture doc in the same PR). Extend the paragraph at
`docs/design/architecture/05-building-block-view.md:197-200`, which already says repairs never
move `Last-Modified`, with the overwrite rule. Update the two `modified` field docs in
`crates/core/src/metadata.rs` (`:1598-1600`, `:1648-1653`) to say "the file's last-modified time;
never decreases across overwrites that record a time; a generation published without one resets
the order". ADR-0047 stays as it is.

## Alternatives considered

- **A new ADR refining ADR-0047.** Rejected by the maintainer at Plan: ADR-0047 decides
  `modified` at the file level, and this rule keeps that meaning.
- **Option 2, a declared skew bound that fails closed (NTP).** This pushes correctness onto
  operators, and it turns a clock problem into an availability problem: a gateway past the bound
  refuses writes. ADR-0024's skew budget is about lease and expiry safety, not presentation
  ordering. Rejected.
- **Option 3, document that `Last-Modified` is per-gateway wall clock.** This is honest, but it
  leaves `aws s3 sync` silently skipping newer same-size content on the supported deployment
  shape. Rejected.
- **Stamp at the server's two PUT sites instead of the core commit.** Two writers of one rule
  would drift, and the multipart flip would need a third copy; with one core helper it needs
  only a call. Rejected.
- **Put the rule inside the leased committer only.** Leaves the public non-leased overwrite
  commit as a way around it, and the multipart flip cannot use the leased committer at all
  (`0016:756`). Rejected in favour of one helper both call.

## Impact & compatibility

The stored stamp differs from the publishing clock in two cases, and only these:
- the prior stamp is **ahead** of the publishing clock (a fast-clocked gateway wrote it): the new
  stamp is prior + 1 ms instead of going backwards;
- the prior stamp **equals** the publishing clock (two overwrites in one millisecond, on any
  fleet, skewed or not): the new stamp is prior + 1 ms. Repeated quickly, this builds a small
  lead that drains once writes slow down (see Design).

Otherwise the stamp is the publishing clock, as today. On the wire, the header shows whole
seconds, so a sub-second adjustment is usually invisible there; a listing on `main` shows it to
the millisecond. No record shape changes and no migration is needed. Lease stamps are untouched.

## Open questions

1. The pre-epoch clock on a **create** (`Some(0)`, listing as `1970-01-01T00:00:00.000Z`) shares
   the root but is a separate change: the same clock read also feeds lease expiry. No code change
   for it belongs in this slice.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 1): rebuilding for the implementation-level findings — C4 Verification (red→green) — Arrange baseline dependency remediation and a complete CI rerun — the frozen gate fails on rustls advisories, while this sandbox cannot lock the advisory database; independent regression and coverage checks pass, but the supplemental DST campaign is incomplete (`review-dst.log:504`; `gate-logs/C4-ci.log:6596`; `gate-logs/C4-ci.log:6626`; `review-ci.log:3321`; `review-green.log:16`).; C4-ci — Local rerun passed typos, docs lint/render/link audit, repository guards, fmt, clippy, build, 1,715 tests with 14 ignored, and machete, then stopped at the read-only advisory-cache lock (`review-checks.log:1`; `review-ci.log:3321`). Frozen CI instead reaches two rustls 0.21.12 advisory failures (`gate-logs/C4-ci.log:6596`; `gate-logs/C4-ci.log:6626`). Neither establishes a defect introduced by this patch; Cargo.lock is unchanged (`Cargo.lock:3294`).; C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) FAILED (gating) — xtask: `cargo deny --all-features --config deny-all-features.toml check advisories` failed with exit status: 1. Deferred, not addressed here: 4 finding(s) that need human judgment, held in deferred-findings.json for the human at handover — not build work
- Failing gate: C4 Wyrd gate: cargo xtask ci (fmt/clippy/build/test/deny/conformance) — xtask: `cargo deny --all-features --config deny-all-features.toml check advisories` failed with exit status: 1
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Rebuild on the basis of the open review findings (bugs), keeping the scope of the brief: - The RUSTSEC-2026-0346/0349 base advisory (rustls 0.21.12 via tikv-client) is cleared as not caused by this patch; land results/issue_620/base-advisory-waiver.diff on main and update ../wyrd before the re-run, or C4-ci stays red. - Still open and must be addressed: the +1 ms step is invisible on the wire because both S3 renderers truncate to whole seconds (gateway-s3/src/lib.rs:637, :3264), so same-size overwrites can still look unchanged to sync tools; and a far-future prior stamp is carried forward forever with no cap, where before the fix the next good write healed it. - The unstamped-publication reset (item 3) is still open; keep it pinned by the leg-5 chained case.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Rebuild on the basis of the open review findings (bugs), keeping the scope of the brief:
  - The RUSTSEC-2026-0346/0349 base advisory (rustls 0.21.12 via tikv-client) is cleared as not caused by this patch; land results/issue_620/base-advisory-waiver.diff on main and update ../wyrd before the re-run, or C4-ci stays red.
  - Still open and must be addressed: the +1 ms step is invisible on the wire because both S3 renderers truncate to whole seconds (gateway-s3/src/lib.rs:637, :3264), so same-size overwrites can still look unchanged to sync tools; and a far-future prior stamp is carried forward forever with no cap, where before the fix the next good write healed it.
  - The unstamped-publication reset (item 3) is still open; keep it pinned by the leg-5 chained case.
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: Revise the brief to match the iteration-3 design the builder had to adopt against the brief's literal text: - Overwrite rule unit is the wire's: commit max(publishing clock, start of the next whole second after prior), not prior + 1 ms. Update A1/A2 expected values (seed mid-second, expect next whole second) and A5 cases accordingly. - Add the one-day cap as a second documented bound of the invariant: a prior more than 1 day ahead of the publishing clock heals to the publishing clock (logged warn). A5: prior Some(u64::MAX) now heals; keep a saturation case within the cap (u64::MAX - 1 -> u64::MAX). - State the accepted burst lead (>1 overwrite/s runs ahead by ~overwrites - elapsed seconds, drains when quiet) in Impact. - Keep the unstamped-publication reset pinned (leg-5 chained case). - Precondition before Do: land results/issue_620/base-advisory-waiver.diff (RUSTSEC-2026-0346/0349) on main and update ../wyrd; as of 7bf7eb9 the all-features advisory check still fails, so C4-ci would block again.
- Sign-off session carry-forward (captured live, before §9 flattened it):
  Revise the brief to match the iteration-3 design the builder had to adopt against the brief's literal text:
  - Overwrite rule unit is the wire's: commit max(publishing clock, start of the next whole second after prior), not prior + 1 ms. Update A1/A2 expected values (seed mid-second, expect next whole second) and A5 cases accordingly.
  - Add the one-day cap as a second documented bound of the invariant: a prior more than 1 day ahead of the publishing clock heals to the publishing clock (logged warn). A5: prior Some(u64::MAX) now heals; keep a saturation case within the cap (u64::MAX - 1 -> u64::MAX).
  - State the accepted burst lead (>1 overwrite/s runs ahead by ~overwrites - elapsed seconds, drains when quiet) in Impact.
  - Keep the unstamped-publication reset pinned (leg-5 chained case).
  - Precondition before Do: land results/issue_620/base-advisory-waiver.diff (RUSTSEC-2026-0346/0349) on main and update ../wyrd; as of 7bf7eb9 the all-features advisory check still fails, so C4-ci would block again.
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
