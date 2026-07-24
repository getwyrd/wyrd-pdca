# Build notes — issue 626 / multipart-commit-protocol

*Withheld from the reviewer; written for the human at sign-off.*

## What I shipped

Exactly the two docs changes the brief's leg A specifies, and nothing else
(`git diff --stat`: 2 files, 925 insertions):

1. `docs/design/proposals/draft/0016-multipart-commit-protocol.md` — the draft proposal
   (924 lines), template section set intact (Motivation / Design / Alternatives considered /
   Graduation criteria / Backward compatibility / Open questions), frontmatter
   `type: proposal`, `status: draft`, `author: Eduard Ralph`, `tracking-issue: "#626"`,
   tags including `proposal`, `s3`, `multipart`, `metadata`.
2. `docs/design/proposals/README.md:32` — the index row, mirroring 0014's draft-row shape at
   `README.md:30` (`draft (authoring: #368)` → `draft (settlement: #626)`).

No file under `crates/` or `xtask/` is touched; no accepted/stable document is touched.

## The design, and why this one

The whole proposal turns on one substitution: **replace a clock with a reference**. Today
publication proves "these bytes are still safe to publish over" by lease *liveness*
(`live_lease_guards`, `crates/core/src/metadata.rs:763-796`; 30 s TTL,
`crates/server/src/lib.rs:53`). An assembled write cannot use a timer, so the proof becomes
"the record that protects these bytes is still exactly as I read it, and its session has not
been torn down". Everything else follows:

- **The fence is what makes the publication batch O(1).** Because every part commit
  preconditions on the session's exact `Open@E` bytes, and every teardown begins by CAS-ing
  the session, one session precondition proves the whole part set is unchanged. That was the
  key move: it avoids both per-part preconditions (envelope death, below) and a per-session
  counter (which would serialize concurrent `UploadPart`s through one hot key).
- **The retirement ledger** answers F4 generally: install an O(1) obligation atomically with
  the transition, drain it in `MARK_BATCH`-sized batches (the in-tree precedent and its
  rationale, `crates/custodian/src/restore.rs:92-100`). The ordering rule — *evidence before
  the lifting of protection* — is what makes invariant (2) hold at every instant, including
  mid-crash.
- **Reference-based reclamation** answers F3 without touching the `Defer` posture (#557): a
  session-owned `pending:` entry whose session is gone or fenced names a chunk nothing can
  publish. That judgment reads **no clock at all**, which is why it is sound where an
  expiry-based judgment is not (`gc.rs:77-104`, `gc.rs:91-94` for the symmetric argument the
  orphan ledger already relies on).
- **Admission control, not the reaper, is F7's bound.** The brief is explicit that "an
  asynchronous collector alone establishes no cardinality bound". So the cap is enforced at
  `CreateMultipartUpload` (a bounded `scan("mpu:")` plus a refusal, no hot counter), and the
  reaper supplies progress. Stated as: *admission bounds the namespace when the reaper is
  down; the reaper makes the system make progress.*

## Findings I added beyond the brief's F1–F10 (please look at these at sign-off)

Each of these came out of reading the target at `cd82a29`; none was handed to me, and each is
disposed of in the document:

1. **The value ceiling, not just the transaction ceiling.** `InodeRecord.chunk_map` is one
   JSON value (`metadata.rs:262-275`, encoded at `:352-356`). I measured a `ChunkRef`
   (`metadata.rs:124-136`) at **131 bytes** with small D-server ids and **302 bytes** with
   worst-case `u64` ids, so FoundationDB's 100 KB value ceiling
   (`crates/traits/src/lib.rs:744-758`) holds only **340–780 chunks** ≈ 340–780 MiB of object
   at the 1 MiB default chunk size. This is a *pre-existing* hazard (a 5 GiB single PUT
   already crosses it) that multipart makes routine, and it is outcome-(d) shaped. Disposed
   of as an admission bound enforced by **refusal** plus follow-up FU-1 (segmentation).
2. **S3 Complete publishes only the parts the client names.** My first draft assumed all
   staged parts publish; the unnamed ones' bytes would then have been deleted-as-records and
   leaked. Fixed with two retirement modes carried in the key (`retire:records:` vs
   `retire:bytes:`) plus a range-encoded part-number set in the payload.
3. **A rejected Complete must release the fence.** An invalid named-part list, a deleted
   bucket, or an over-cap map would otherwise wedge the session in `Completing` until
   `W_completing` — F1 with a client-error trigger.
4. **The bucket-existence precondition at publication** (ADR-0046 §4) — a Complete is an
   object PUT and inherits that rule; without it a session outliving its bucket strands an
   object in a deleted bucket.
5. **A rollout-order requirement.** An *old* restore pass marks every unreferenced,
   non-pending fragment `orphan:` (`restore.rs:257-269`), so a pre-upgrade custodian against
   a store with staged parts would mark live staged bytes. GC itself is safe (its
   conservative arm, `gc.rs:183-187`). Written into Backward compatibility as a MUST.
6. **Staged bytes are scrub/repair-eligible.** The cheaper design (protect but do not verify
   or repair) would have left a *durability* decay window across a staging window measured in
   hours — and durability is not in the allowed accepted-cost classes (availability /
   latency / capacity / operational). So decision 2 says yes for scrub and reconstruction,
   with the session fence serializing any re-place against Complete.

## Alternatives ruled out, with the cost shown

- **Per-part exact-value preconditions at Complete** (the obvious first implementation).
  10,000 parts × a part-record value. Even at a modest 4 KB per part record that is ~40 MB in
  one batch against a 10 MB / 5 s ceiling (`traits/src/lib.rs:744-758`); at the value ceiling
  it is ~1 GB. The largest legal upload would be the one that can never complete.
- **A per-session mutation counter CAS'd by every part commit.** Correct, and one
  precondition — but it turns N concurrent `UploadPart`s into N CAS contenders on one key
  (SDK defaults are 4–16 concurrent parts). The fence gets the same proof with CASes only on
  state transitions, of which there are at most four per session.
- **Chunk-map segmentation now** (the general fix for finding 1). Concretely: `.chunk_map` is
  read in **19 places across 8 non-test files** (`crates/core/src/{metadata,read}.rs`,
  `crates/server/src/lib.rs`, `crates/custodian/src/{gc,restore,reconstruction,rebalance,backfill}.rs`
  — `grep -rn "\.chunk_map" crates/ --include=*.rs | grep -v /tests/`), plus every backend's
  conformance expectations. The admission bound is two checks in the gateway. That is the
  cost ratio behind deferring it to FU-1 — and the deferral is safe only because the bound
  *refuses* rather than commits.
- **Staging inside `pending:` with a longer TTL.** Rejected on ADR-0046 grounds (synthesized
  encoding into an existing namespace) and because it inherits the very reclamation hazard
  decision 5 exists to escape.
- **Letting a `retire:` payload flag decide orphan-vs-delete.** A misread boolean is silent
  data loss; the mode lives in the key so a mis-parse is an error at decode (ADR-0045's
  boundary).

## Verification — the three refutation questions

**(a) Genuine red?** Yes, twice, on the real gate (`./engine/xtask.sh ci` → `cargo xtask ci`
in `$PDCA_WORKTREE`), and both prose legs executed rather than warn-skipped:

- `typos` went **red** on the new file as written — `0016-multipart-commit-protocol.md:161`
  (`mis-parse`) and `:163` (`unparseable`) — and green after rewording. That red was not
  manufactured; it was the gate catching my prose.
- The dangling-link audit went **red** on demand: I pointed the index row at
  `draft/0016-multipart-commit-protocolXX.md` and `render_site.py --check` reported
  `ERROR dangling link in output: proposals/index.html -> draft/0016-multipart-commit-protocolXX.md`
  and failed the gate; restoring the real filename made it green. So leg A1's discriminator
  is live on this host, not a warn-skip.
- Full `cargo xtask ci` then passed end to end on the patched worktree
  (`xtask ci: all checks passed`).

For legs A2 and B there is no mechanical red by declared design (brief posture (a),
NET-NEW/born-at-tier): "red" for leg B is criterion-absence under the Refutation standard,
exercised by the Check reviewer and adversary and decided by the human. I did not manufacture
a test to simulate that judgment.

**(b) Production path?** Yes. The gate ran against the actual worktree files that constitute
the deliverable — there is no copy, stand-in, or fixture in between. The artifact *is* the
unit under test for a design document, and `render_site.py --check` renders the whole site
including the real index row.

**(c) Fixture includes the fault?** Yes. The red was produced on the real new file and the
real index row (the injected fault was on the index row that ships), not on a scratch copy,
and the failing element — the link the audit resolves — is exactly the one the patch adds.
Nothing was curated out: the green run renders all 98 pages, the new one included.

## Environment / dependencies

Both external dependencies the brief registered were present and **executed** on this host:
`typos` (it failed the run on my two misspellings — proof it ran) and `docs-renderer`
(`markdown_it` + `yaml` imported; the render-and-link audit ran and failed on the injected
dangling link — proof it did not warn-skip). No undeclared dependency was needed: no Docker,
no cluster, no backend. **No NEEDS-HUMAN external dependency to declare.**

## The scope consequence you should weigh

This design is *not* cheap to implement, and I want that visible rather than buried:

- **Decision 2 reaches into five custodian passes** (gc, restore, scrub, reconstruction,
  desired_state) plus the placement call in `write.rs:108-114`. I considered the cheaper
  "protect but do not verify/repair" variant and rejected it in writing (Alternatives, last
  bullet) because its cost class is durability, which the brief does not permit as an accepted
  cost.
- **Decision 4 changes an existing committed contract**: `commit_chunk_map_superseding{,_leased}`
  stops expanding orphans inline, which touches the ordinary single-PUT overwrite path, not
  just multipart. That is deliberate — a 5 MiB PUT over a multipart-sized object has exactly
  the same fan-out — but it means #508 is not purely additive, and it is why decision 4 is one
  of the two ADR-graduation recommendations.

If the maintainer wants a smaller #508, the lever is *not* to weaken an invariant but to shrink
the advertised object ceiling (a smaller `MAX_MAP_CHUNKS`) — the design already refuses rather
than commits past it.

## What remains for the human at sign-off

- The proposal is a **draft** by design; ratification (draft → accepted, architecture-board /
  founding-maintainer authority under ADR-0037) is explicitly *not* this cycle's act, and the
  document says so in its opening note.
- Leg B is a judgment call the brief assigns to the reviewer, the adversary, and you. The
  place to attack it is the **execution register** (X1–X32) plus the six per-decision
  failure-mode tables: a concrete execution absent from those tables is the refutation.
- Two decisions are recommended for ADR graduation (the retirement ledger; the staging
  protection class). Filing FU-1..FU-4 is stated in the proposal as part of accepting it —
  I deliberately did not invent tracker numbers for issues that do not exist yet; each
  follow-up carries an exact title, owner and trigger instead.
- Scratch: the only files I created outside the bundle and the worktree are the gate logs
  under `$PDCA_SCRATCH` (`pdca-builder-626-ci*.log`), removed at the end of the run.
