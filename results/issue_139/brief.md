# Design proposal — issue 139 / m3.1-placement-record

> Implementation slice of the **already-accepted** proposal 0005 (Milestone 3 —
> custodians), PR-sequence step 1. The normative design lives in
> `docs/design/proposals/accepted/0005-milestone-3-custodians.md` §"The placement
> record" — an Accepted proposal, immutable (INTEGRATION §2). This brief does **not**
> re-decide the design; it points at 0005 and scopes the one slice for Do. The `##`
> sections summarise 0005 and cite it; they do not supersede it.
>
> **Design refs (per the issue's Refs line, verified on `origin/main`):** proposal 0005
> §"The placement record"; architecture **§6.1** *Write path (the commit point)*
> (`docs/design/architecture/06-runtime-view.md`) and **§7.3** *Failure domains*
> (`docs/design/architecture/07-deployment-view.md`); **ADR-0015** *Consistency contract:
> home-zone authority, version-fence reserved* (`docs/design/adr/0015-consistency-contract.md`)
> — the commit-point + version semantics the placement record must honour.

- **Slug:** m3.1-placement-record
- **Kind:** enhancement (design proposal — implements accepted proposal 0005, step 1)
- **Goal:** The committed chunk map records, per fragment index, the **stable D-server
  id** that holds the fragment (a per-`ChunkRef` placement vector of length *n*); the
  write path records it at the commit point and the read path resolves fragments from
  that record, retiring stateless `index % n` routing — so a fragment that has been
  *moved* is still found.
- **Success criterion:** An `rs(6,3)` write commits a per-fragment placement vector
  (one stable D-server id per fragment index) into the chunk map at the commit point,
  and the read path reconstructs the chunk by resolving each fragment **from that
  record**, including after the metadata store is reopened (process-restart
  equivalent). BINDING demonstrable condition: a regression in which a fragment is
  placed at a store that `index % n` would **not** select is still read correctly —
  red against today's `index % n` read path, green once the read consumes the record.
  (Recording the location on `ChunkRef` and using a *stable D-server id* rather than an
  endpoint URL are BINDING — they are accepted design in 0005, not Do's call. "Length-n
  placement vector" vs. an equivalent encoding is ILLUSTRATIVE.)
- **Repo + branch target:** getwyrd/wyrd @ main   (INTEGRATION §2 — single line, no maintenance branches)
- **Conflicts with:** 141
- **Ordering note:** no deps — foundation slice (M2 complete at #114). Conflicts with #141 (both edit the write fan-out / selector seam in `chunkstore-grpc` + `core`); #141 already `Depends on` #139, so the ordering already serialises them.
- **Surfaces:** data   (metadata model + write/read paths; no GUI)
- **Scope:** add a per-fragment placement record to the chunk map (`ChunkRef`,
  `crates/core/src/metadata.rs`), record it at the write commit point, consume it on the
  read path in place of `index % n`, and introduce the **stable D-server id** that
  registration (`Coordination`) carries and discovery resolves to a current endpoint.
  / out of scope: the custodian crate and its loops (#141); `list_fragments` /
  `delete_fragment` (#140); the version-conditional location *update* used by repair /
  rebalance (later 0005 slices 6–7); failure-domain *enforcement* on placement (#141 owns
  the selector); any on-disk fragment-format / `format_version` change (additive metadata only).
- **Test file:** `crates/core/tests/placement_record.rs` (new) — write records the
  placement vector; read resolves fragments from it, including after reopening the
  metadata store. (Server-level read coverage in `crates/server/tests/read_fanout.rs`
  is supplementary, not the regression home.)
- **Verification posture:** flippable regression (default) — the moved-fragment read
  test is **red** against today's `index % n` resolution and **green** once the read
  consumes the record; the restart-resolution leg is exercised in-process by reopening
  the metadata store (no Docker, Check-observable). The full `rs(6,3)`-over-real-tonic
  and DST seed coverage is supplementary evidence confirmed by `cargo xtask ci` / the
  DST sweep, not required to be green at C4-verify.
- **Citations expected:** Do must cite path:line on `origin/main` for every change
  (`crates/core/src/metadata.rs:73-80` `ChunkRef`; `crates/chunkstore-grpc/src/fanout.rs:9-12,51-53`).
- **Prior-art check (triage cycles):** searched merged history and all PRs by file path
  (`crates/core/src/metadata.rs`, `crates/chunkstore-grpc/src/fanout.rs`) — last touch is
  M2 `#114` (parallel fan-out write); **no** placement-record work merged, open, or
  closed. Net-new.
- **Disposition hint:** new-feature

## Motivation
M2 routes a fragment **statelessly**: `FanoutChunkStore::route(index) = stores[index % n]`
(`crates/chunkstore-grpc/src/fanout.rs:51-53`), and the fan-out's own docstring records
the debt — "the read resolves a fragment back to where the write put it without a
placement record (**the recorded-placement question is settled at M3**)"
(`fanout.rs:9-12`). The committed chunk map carries no location: `ChunkRef { id, scheme,
len }` (`crates/core/src/metadata.rs:73-80`). The moment a custodian *moves* a fragment
(reconstruction, rebalance — the whole point of M3), `index % n` resolves to the wrong
store. M3 cannot be built on stateless routing; recording placement is its first
load-bearing change, and proposal 0005 resolves M2's deferred open question in the
affirmative: **placement is recorded at commit.**

## Design
Per 0005 §"The placement record" (authoritative):
- **Shape.** The chunk map records, per fragment index, the stable D-server id holding
  it — a per-`ChunkRef` placement vector of length *n*. Recorded at the write commit,
  consumed by the read path (replacing `index % n`).
- **Stable D-server identity.** A D server is referenced by a **stable id**, not its
  endpoint URL (URLs rebind / NAT, and a record keyed on a URL would rot). Registration
  through `Coordination` carries `{ id, endpoint, failure-domain label }`; discovery
  resolves `id → current endpoint`. This slice introduces the stable id + the
  endpoint-resolution it implies; the failure-domain *label*'s consumer is #141.
- This is a composition-local change to `core` (metadata model + write/read paths) and
  `chunkstore-grpc` (the fan-out stops being the location authority); the `MetadataStore`
  and `ChunkStore` **traits are untouched** by it (0005).

Recording happens at the **commit point** (architecture §6.1) and must stay consistent
with the **consistency contract** (ADR-0015: home-zone authority, version-fence) — the
placement record is read + CAS-rewritten under the inode version, so a reader never sees
a hybrid. The stable D-server id + failure-domain label come from the failure-domain model
(architecture §7.3). The version-conditional location *update* (repair's atomic re-point)
is described in 0005 but belongs to a later slice — this slice only records-at-write and
resolves-on-read.

## Alternatives considered
Settled in 0005 §"The placement record" / §"Alternatives considered" and not reopened
here: keeping stateless `index % n` (rejected — breaks on the first custodian move);
keying placement on endpoint URL (rejected — URLs rot under rebind/NAT, hence the stable
id). 0005 is Accepted; per INTEGRATION §2 a change to it would require a new superseding
proposal, not an edit.

## Impact & compatibility
On-disk fragment format **unchanged** (`format_version` untouched); the placement field
is **additive metadata** on a never-yet-deployed schema, so M0–M2 chunks read through the
same path. No new third-party dependency. The M0/M1/M2 suites must stay green
(`cargo xtask ci`). The change is observable only on the backend (data surface).

## Open questions
- Encoding of the placement vector on `ChunkRef` (inline `Vec` vs. a side table) — a Do
  call provided the BINDING success criterion holds; flag for sign-off if it forces a
  `MetadataStore` trait change (it should not, per 0005).
- Stable-D-server-id type/source: confirm it threads through the existing `Coordination`
  registration without a trait break — a NEEDS-HUMAN if the trait must change (ADR /
  trait-contract territory).

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.
