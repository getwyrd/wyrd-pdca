# Result — issue 768 / last-modified-never-decreases

## 1. Spec (from brief.md)              ← Check verifies against THIS
- Defect: A file's `Last-Modified` never goes backwards across an overwrite, whichever gateway
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
- Success criterion: **(A) The rule holds.** A NEW test drives the real `wyrd_server::Gateway`
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
- Repo + branch target: getwyrd/wyrd @ main
- Scope: making every overwrite publication keep a file's `Last-Modified` from going
  backwards, and stating that rule in the architecture doc and the `modified` field docs / out of
  scope: any new or edited ADR (the rule is within ADR-0047's file-level intent); a clock-skew
  bound or NTP requirement (option 2); the pre-epoch clock that stamps `modified: Some(0)` on a
  **create** (`now_millis`'s `.unwrap_or(0)`, `crates/server/src/lib.rs:819-822`; see Open
  questions); the listing's rendering granularity (#767); the multipart Complete flip (#658 builds
  it and must call the shared rule this slice adds; it does not inherit it); lease stamps (the
  same `now_millis` feeds them, and they keep their meaning).

## 2. Disposition claimed               ← sign-off confirms or overrides
- Outcome: likely-fix
- Confidence: medium
- Recommendation: (set by Do)

## 3. Correctness (Check — chain)
- C1 Spec: none — brief.md
- C2 Reproduction (red pre-fix): none — N/A — close disposition (no patch to verify)
- C3 Change: none — patch.diff
- C4 Verification (red→green): none — N/A — close disposition (no patch to verify)
- C5 Causal adequacy: none — reviewer + human sign-off

## 4. Conformance (Check — stack)
- T1 Structure: none — N/A — close disposition (no patch to verify)
- T2 Shape: none — N/A — close disposition (no patch to verify)
- T3 Runtime: none — N/A — close disposition (no patch to verify)
- T4 Contribution: none — N/A — close disposition (no patch to verify)
- T5 Judgment: none — reviewer + human sign-off
- T5 judgment: → see §5.

## 5. Advisory review (artifact-only, decorrelated)
Reviewer ran without build-notes.md. Summary:

# Advisory review — SKIPPED (builder-declared external dependency confirmed, #341)

The builder declared an unmet external dependency in build-notes.md and the claim was CONFIRMED deterministically (the named [[doctor.checks]] row's detect cmd exited non-zero), so the Check beat was not spent adjudicating a patch already stated to be unverifiable. Gates are recorded N/A; no reviewer or adversary ran. The bundle is resumable.

- NEEDS-HUMAN — External dependency `cargo deny all-features advisories` confirmed absent (detect cmd `cd "${WYRD_REPO:-../wyrd}" && cargo deny --frozen --all-features --config deny-all-features.toml check advisories >/dev/null 2>&1` exited 1). Install hint: the target's all-features advisory wall (the second cargo-deny step of `cargo xtask ci`) fails on the base, so every bundle's gating C4-ci is red until a separate deny-all-features.toml PR records or clears the advisory (#543; precedent #790/#773; a ready waiver for RUSTSEC-2026-0346/0349 is results/issue_620/base-advisory-waiver.diff). Land it on main, update ../wyrd, then re-run. Checks ../wyrd as checked out, or $WYRD_REPO. Provide it and answer iterate-do to resume the full Do+Check band, or discontinue at sign-off.


## 6. NEEDS-HUMAN — items the human must clear before sign-off
- [x] External dependency `cargo deny all-features advisories` confirmed absent (detect cmd `cd "${WYRD_REPO:-../wyrd}" && cargo deny --frozen --all-features --config deny-all-features.toml check advisories >/dev/null 2>&1` exited 1). Install hint: the target's all-features advisory wall (the second cargo-deny step of `cargo xtask ci`) fails on the base, so every bundle's gating C4-ci is red until a separate deny-all-features.toml PR records or clears the advisory (#543; precedent #790/#773; a ready waiver for RUSTSEC-2026-0346/0349 is results/issue_620/base-advisory-waiver.diff). Land it on main, update ../wyrd, then re-run. Checks ../wyrd as checked out, or $WYRD_REPO. Provide it and answer iterate-do to resume the full Do+Check band, or discontinue at sign-off.
- [x] external dependency: cargo deny all-features advisories — the base `0dd47cc` fails `cargo deny --all-features --config deny-all-features.toml check advisories` (RUSTSEC-2026-0346/0349, rustls 0.21.12 via tikv-client 0.4.0) before this patch is applied, so the gating C4-ci row stays red for every bundle on this base until the waiver (results/issue_620/base-advisory-waiver.diff) or a dependency bump lands on main; this patch cannot clear it within scope
- [ ] The unconditional ordering contract is contradicted by the planned handling of absent metadata. `brief.md:82-89` promises the invariant for every committed generation on both public overwrite APIs, and `brief.md:61-62` / `brief.md:211-212` require unqualified “never decreases across overwrites” documentation. But `brief.md:52-53` and `brief.md:165-168` require a publisher with `None` to clear the stamp and a prior with `None` to accept the next publisher's stamp unchanged. Counterexample under those rules: `Some(2000)` → an unstamped overwrite → `None` → an overwrite publishing `Some(1000)` → `Some(1000)`. The same file loses its ordering history and later exposes a lower timestamp. This is an admitted core API input, not malformed metadata: target `crates/core/src/write.rs:63-68` explicitly permits plans with no metadata, and `crates/core/src/metadata.rs:2218` / `:2297` store that optional value. Current gateway PUTs supply `Some` (`crates/server/src/lib.rs:254`, `:405`), so the chosen implementation can still fix the reported gateway defect. Revise the invariant and required docs to limit the guarantee to uninterrupted timestamp-bearing overwrites, explicitly documenting the reset across an unstamped publication; add the chained case to A5 to pin that boundary. If the unconditional core guarantee is intended instead, the `None` policy needs a design revision.
- [ ] **The fix does not reach the wire, and for some sync rules it makes the issue's scenario worse.** `crates/core/src/metadata.rs:2272` commits `max(t, p + 1 ms)`. Both renderers truncate to seconds: `crates/gateway-s3/src/lib.rs:637` (listing, always `.000Z`) and `:3264` (header). The base already documents this at `docs/design/architecture/05-building-block-view.md:195-198`: #767 has landed. So the brief's statement "a listing on `main` renders milliseconds … `.751Z` after `.750Z`" is out of date here, and the bump is visible on **no** S3 surface unless `p` ends in `.999`.
- [ ] **One bad clock reading now sticks to a file permanently. Before the fix the next good write healed it.** The rule has no cap (`crates/core/src/metadata.rs:2272`). The patch's own leg (1) shows a prior an hour ahead is carried forward exactly (`last_modified_monotonic.rs:136-151`).
- [x] **The red C4-ci gate is not caused by this patch.** It fails only at `cargo deny … check advisories`: RUSTSEC-2026-0346 and RUSTSEC-2026-0349 on `rustls 0.21.12` (`Cargo.lock:302`, pulled in through `tikv-client 0.4.0`; see `gate-logs/C4-ci.log:3530-3608`). The patch does not touch `Cargo.lock`. fmt, clippy, build and the full test run passed, including all five new tests (`C4-ci.log:2574-2581`, `:5633-5640`). This needs a waiver, or a dependency fix on `main`. It is not a builder iteration.

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
- Iteration delta (if iterating): Rebuild on the basis of the open review findings (bugs), keeping the scope of the brief: - The RUSTSEC-2026-0346/0349 base advisory (rustls 0.21.12 via tikv-client) is cleared as not caused by this patch; land results/issue_620/base-advisory-waiver.diff on main and update ../wyrd before the re-run, or C4-ci stays red. - Still open and must be addressed: the +1 ms step is invisible on the wire because both S3 renderers truncate to whole seconds (gateway-s3/src/lib.rs:637, :3264), so same-size overwrites can still look unchanged to sync tools; and a far-future prior stamp is carried forward forever with no cap, where before the fix the next good write healed it. - The unstamped-publication reset (item 3) is still open; keep it pinned by the leg-5 chained case.
- By / date: Eduard Ralph / 2026-10-14

## 10. Act candidates (hints for the next Act review)
- Plan advisory: 1 finding(s); brief revised: yes (plan-advisory-*.md)
- (empty is the common case)
