# Design proposal — issue 638 / fragment-write-deadline

> Plan artifact (design-proposal form; enhancement). Do reads ONLY this file.
> The `- **Label:** value` lines are parsed by the driver — keep their shape.
>
> **The design is already settled and is normative here:** proposal **0016 — the multipart
> commit protocol**, `docs/design/proposals/draft/0016-multipart-commit-protocol.md` on
> `origin/main` @ `22d71b4`. The passage that IS this slice's specification is
> **`0016:1551-1576`** — the end-to-end-deadline argument inside decision 5 — together with the
> strict margin `G_orphan > W_write + δ_clock` (`0016:1478`) and 0016's own failure-mode row at
> `0016:1784`. **Do MUST read `0016:1551-1576` in full before writing code.**
>
> Citations verified against `origin/main` @ `22d71b4` on 2026-07-26.
> This is **seam (vi)** — added on 2026-07-26 after two independent cross-vendor plan reviews
> found this requirement normative in 0016 but **owned by no slice** of the five-slice re-plan
> (`results/plan-review-codex-636-r2.md` F6, `results/plan-review-codex-637-r2.md` F4).

- **Slug:** fragment-write-deadline
- **Kind:** enhancement (design proposal)
- **Goal:** make `W_write` a **real** bound by giving the fragment-write path an authorization
  deadline **the D server itself enforces**. Today the `ChunkStore` seam cannot express it —
  `put_fragment` takes only `(id, fragment)` (`crates/traits/src/lib.rs:575-577`) and
  `FragmentPutRequest` carries only ID and bytes
  (`crates/proto/proto/wyrd/v0/chunk.proto:25-29`) — so the only bound in existence is the
  client's channel timeout (`crates/chunkstore-grpc/src/client.rs:169-190`), which bounds how long
  the **writer waits**, not when an already-accepted write **takes effect**.
- **Success criterion:** one **NEW** test file, `crates/chunkstore-grpc/tests/write_deadline.rs`,
  driving a **real in-process gRPC D server** (the idiom `crates/chunkstore-grpc/tests/` already
  uses), plus a seeded DST case appended to an existing file (see `Test file`).
  **(A) The server refuses an expired write — at the SERVER, not the client.** Send a
  `put_fragment` whose deadline has already passed, with the **client-side timeout generous** so a
  client-side bound cannot be what produces the refusal. Assert: the RPC is refused with a typed,
  operator-classifiable error, **and the fragment is not stored** — read back through
  `get_fragment` and assert `None`. Asserting only the error would pass an implementation that
  refuses the caller *after* persisting the bytes, which is the exact outcome (a) leak this slice
  exists to prevent.
  **(B) A write parked past its deadline is refused when it is finally processed** — 0016's own
  failure-mode row (`0016:1784`): "authorize a fragment write, park it in the D server's accept
  queue past `W_write`, and assert it is refused". Drive it with a server-side delay/pause seam so
  the deadline elapses **between acceptance and application**; assert refusal and absence. This is
  the leg that distinguishes a server-enforced deadline from a caller timeout, and **a
  client-timeout implementation fails it**: the client is still waiting happily.
  **(C) A live write is unaffected.** A `put_fragment` well inside its deadline stores and reads
  back byte-identical. Without this the slice can pass A and B by refusing everything.
  **(D) Absent deadline ⇒ exactly today's behaviour (additive compatibility).** A request carrying
  **no** deadline field stores normally. Assert it over the wire, because this is what keeps every
  existing writer — the ordinary write path, backfill, reconstruction, rebalance — working
  unchanged. A proto field is additive on the wire; the *trait* change is not source-compatible,
  so state in `build-notes.md` how existing callsites were migrated.
  **(E) The seam means the same thing on every implementation.** `chunkstore-fs`
  (`crates/chunkstore-fs/src/lib.rs:180`) honours the identical contract, so a caller cannot get
  a weaker guarantee by holding a local store. Assert A and C against it too — cheaply, in the
  same file or beside it.
  **(F) The refusal is classifiable, not a bare string.** It must be distinguishable by a caller
  from a genuine backend fault, in the register the seam already uses for typed cross-backend
  errors (`ScanCapExceeded`/`IntegrityFault`/`BlockReadFault` all live in `crates/traits` so every
  backend raises the *same* type — `crates/traits/src/lib.rs:288-300`). A deadline refusal is an
  expected, non-fault outcome and a caller must be able to tell it from "the disk is broken".
  **(G) `cargo xtask ci` green.**
- **Falsifiability:** RED is producible **in-process on this bundle's base**, with no container
  and no cluster — `crates/chunkstore-grpc/tests/` already stands up real in-process tonic servers
  over madsim/tokio. Legs A, B, E and F **red by assertion, not by build error**, provided the
  test file is written against the base seam: on the base a deadline cannot even be expressed, so
  the test must send the deadline **as a raw protobuf field on the wire** (or drive the generated
  client directly) rather than through a `ChunkStore` method signature this slice changes. Written
  that way the base compiles it, the server ignores an unknown/absent field, the write **succeeds**,
  and the assertion "refused and not stored" **fails** — a real red. Written the other way (calling
  a new `put_fragment` arity) it is a build error, and `run-verify.sh` cannot tell that from a real
  red: on a non-zero exit the `TESTS_RAN == 0` guard at `engine/scripts/run-verify.sh:416-427` is
  inside the cargo-*succeeded* branch and execution falls through to the unconditional `PASS` at
  `:433`. **Do MUST record from the RED leg how many tests ran and failed, and confirm the
  failures were assertions.**
  **Leg D is the exception and it is compile-shaped**, since "absent field ⇒ unchanged" is only
  meaningful against the new code; keep it in the same file only if it does not break the base
  compile, otherwise assert it post-fix only and say so.
  **Base:** this is a **wave-1** bundle (see `Ordering note`), so the driver exports
  `$PDCA_VERIFY_BASE = origin/pdca-integration/main` (`src/pdca_harness/flow.py:459`,
  `src/pdca_harness/gates.py:352-360`), honoured by `run-verify.sh` ahead of the brief's base
  (`:180-192`). Without it the gate resets to `origin/main` and the patch may not apply — this
  slice edits `crates/traits/src/lib.rs`, which #634 also edits.
- **Invariant to restore:** **a durable write that has been authorized must either take effect
  within a bounded, enforced window of its authorization or never take effect at all** — no
  accepted write may be applied arbitrarily late. Stated over the category (every fragment write
  to every store), not over multipart. **Source:** `0016:1551-1576` — "the bound is only real if
  the D server enforces it too … without it `G_orphan > W_write + δ_clock` bounds nothing, because
  a write parked in a server queue can land arbitrarily late" — resting on the *await discipline*
  MUST (`../wyrd/AGENTS.md:181-183`) and on the custodian's rule that a fragment is never
  reclaimed without evidence (`crates/custodian/src/gc.rs:183-187`). SELF-TEST: this cannot be
  satisfied by guarding one module — the writer and the store are different processes, and the
  whole point is that the *acceptor* must enforce what the *caller* cannot.
- **Scope:** the `ChunkStore::put_fragment` seam (`crates/traits`), the additive
  `FragmentPutRequest` field (`crates/proto`), the gRPC client send and the **D-server service
  refusal** (`crates/chunkstore-grpc/src/{client.rs:208, server.rs:66}`), the same contract on
  `chunkstore-fs` (`:180`), the typed refusal class, migration of every existing `put_fragment`
  callsite and test double, and the seeded DST case. / **out of scope:** choosing `W_write`'s
  **value** and the `G_orphan > W_write + δ_clock` margin — **#625** owns the windows; this slice
  ships the *mechanism* and takes the deadline as a parameter. Also out: the multipart records
  (#636), the staged protection class (#637), the S3 verbs (#508), `scan_page` (#634), the
  segmented map (#635), and any file under `docs/design/adr/` or `docs/design/specs/`.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:**
- **Conflicts with:** 634
- **Ordering note:** **wave 1**, alongside #635. No dependency on anything: the deadline mechanism
  stands alone and its first *consumer* is #636's staged fragment writes, which is why **#636
  depends on this bundle**. The conflict with **#634** is a file conflict — both edit
  `crates/traits/src/lib.rs` (#634 adds a `MetadataStore` method, this adds a `ChunkStore`
  parameter) — so they must not be built blind on one base; the scheduler orients by name order,
  putting `issue_634` in wave 0 and this in wave 1, which is the order the stack wants anyway.
  No conflict with #635 (disjoint crates), so the two share wave 1.
- **Surfaces:** data
- **Difficulty:** medium
- **External dependencies:** `typos`, `docs-renderer`
  <br>*(Both on the field's own line — the driver reads only that line. Needed because this slice
  changes an RPC, so the docs-currency rule applies (`../wyrd/AGENTS.md:154-157`) and the prose
  gates warn-skip when absent, letting a locally-green docs change open the PR red, INTEGRATION §3.
  **No `protoc`:** `crates/proto` builds its protobufs with **protox**, not `protoc` — the doctor
  row's own hint says so — and madsim uses `skip_protoc_run`, so this slice adds no build tool.)*
- **Test file:** `crates/chunkstore-grpc/tests/write_deadline.rs`
  <br>*(ONE added test file — the driver parses only this label's own line,
  `src/pdca_harness/brief.py:23-31`.)* The DST case for 0016's failure-mode row is **appended to
  the existing** `crates/dst/tests/network.rs` (the M2.6 network DST over the real gRPC ChunkStore
  on madsim's simulated network) — **not** a new `crates/dst/tests/*.rs` file. That is not
  stylistic: `run-verify.sh` puts every **added** test target into one cargo invocation and, seeing
  a `#![cfg(madsim)]` crate root among them, applies `RUSTFLAGS=--cfg madsim` to the whole
  invocation (`engine/scripts/run-verify.sh:100-131`, `:344-385`), which rebuilds the other crates
  against the simulator's dependency graph and would take leg A's assertion red down with it.
  Appended, the DST case runs where it belongs: `cargo xtask ci` → `run_dst`
  (`xtask/src/main.rs:1575-1614`), the **gating** row.
- **Verification posture:** DEFAULT — red pre-fix, green post-fix at Check, for legs A, B, C, E
  and F. Leg D is compile-shaped and declared as such. Nothing is deferred off-Check: the gRPC
  tests are in-process and `chunkstore-fs` is local.
- **Production reach:** the live path traverses this seam at Check — the tests drive the **real**
  tonic service and the real client, not doubles. The one thing not exercised here is a *multipart*
  caller, because none exists until #636; the ordinary write path is a caller from day one, which
  is why leg D's additive-compatibility assertion is binding rather than cosmetic.
- **Citations expected:** Do must cite `path:line` on the target branch for every change. **Peer
  callsites Do SHOULD open and mirror:**
  * `crates/traits/src/lib.rs:570-580` — the `ChunkStore` contract and `put_fragment`'s current
    signature; `:288-300` — how a typed, cross-backend error class is declared in the seam crate
    so every backend raises the same type (the model for the refusal class of leg F).
  * `crates/chunkstore-grpc/src/server.rs:66` — the service's `put_fragment`, where the refusal is
    enforced; `crates/chunkstore-grpc/src/client.rs:169-190` (the channel timeout that is *not*
    sufficient) and `:208` (the client `put_fragment` that must send the field).
  * `crates/chunkstore-fs/src/lib.rs:180` — the local store's `put_fragment`.
  * `crates/proto/proto/wyrd/v0/chunk.proto:25-29` — `FragmentPutRequest`; add the field
    additively (a new tag, never a renumber).
  * `crates/chunkstore-grpc/src/fanout.rs:79`, `:147`, `:189` — the fan-out wrapper and its
    in-crate double, both of which the seam change touches.
  * `crates/dst/tests/network.rs` — the existing network DST the seeded case joins.
- **Prior-art check (triage cycles):** searched by affected file path across merged history and
  all PRs. `crates/proto/proto/wyrd/v0/chunk.proto` has never carried a deadline field; no merged
  change adds a server-side write deadline (`git log -S"deadline" -- crates/chunkstore-grpc`
  surfaces only the client channel timeout from `0835df5`, "bound every operation — tikv-client
  does not", which is a *metadata* store change). No open PRs. **The one adjacent rejection, and
  why it does not cover this:** `results/issue_508/review-rejected.md:10` rejected "the new
  D-server fragment fan-out has no timeout" on the grounds that the `ChunkStore` implementation
  already bounds the network via `connect_with_timeout`. **That rejection stands and is not
  reopened** — it concerned a duplicate *caller-side* bound inside `core`. 0016 `:1557-1564` says a
  caller-only timeout is **insufficient**; this slice is the other end of the same property, which
  no existing bound provides and which the seam cannot currently express.
- **Disposition hint:** likely-fix

## Motivation

Decision 5's late-fragment safety argument has one load-bearing step: a fragment authorized under
the last live lease must land **strictly before** its position's orphan grace elapses, so the
`orphan:` mark is still present and GC never reclaims the evidence before the late fragment is
covered by it. That is the strict margin `G_orphan > W_write + δ_clock` (`0016:1478`).

The margin is arithmetic over `W_write`. If `W_write` is only a caller-side timeout, it bounds
nothing that matters: the caller gives up, but the write it already handed to the D server can sit
in an accept queue and be applied arbitrarily later — after the mark is gone, after the reaper has
torn the session down, after GC has reclaimed the position. The result is a durable fragment that
is unreferenced **and** unevidenced: 0016 outcome (a), the leak the whole protocol is built to
exclude.

0016 therefore requires the D server to **refuse** such a write, and calls that refusal "the
implementing slices' obligation". Across the five-slice re-plan no slice claimed it — #636's
staged writes assume the bound, #637's grace arithmetic assumes the bound, and neither ships it.
This slice ships it.

## Design

Read `0016:1551-1576`. Three properties, and the rest is implementation:

1. **The write carries its authorization instant** (or the derived deadline — Do chooses which
   travels, and states why; carrying the *deadline* avoids the receiver needing the sender's
   `W_write`, carrying the *instant* keeps the policy at the receiver. Either is defensible; a
   silent choice is not).
2. **The acceptor enforces it**, refusing rather than queueing, at the point where the write would
   otherwise be applied — so the check cannot be skipped by a write that was accepted earlier and
   processed later. That is the whole point of leg B.
3. **The refusal is typed** and distinguishable from a backend fault.

**Clock ownership.** The deadline is compared at the D server against *its* clock, and the margin
`δ_clock` in `G_orphan > W_write + δ_clock` exists precisely to absorb the skew between the two
evaluation sites. Do not invent a clock: `clippy.toml` denies a bare `SystemTime::now()` in the
library crates (wyrd#619), so the deadline must arrive as data and the comparison must happen where
a clock is already legitimately owned. State in `build-notes.md` which lifecycle owns the read.

**Additive on the wire, breaking in the seam.** A new protobuf field is backward-compatible by
construction (leg D). The Rust `ChunkStore::put_fragment` signature is not — every implementor and
every test double in the workspace must be migrated. Keep that churn mechanical and uniform, the
same discipline #634 applies to its own seam widening, so the reviewable surface stays the
enforcement and its tests.

## Alternatives considered

* **A caller-side timeout only** — the status quo, and 0016 rejects it explicitly (`:1557-1564`):
  it bounds how long the writer waits, not when the write takes effect.
* **Deriving the deadline at the server from a lease lookup** — would make every fragment write a
  metadata read, on the hot data path, and couples the chunk store to the metadata plane it is
  deliberately independent of (ADR-0010). The deadline travelling with the request keeps the D
  server ignorant of sessions and leases.
* **Enforcing it in the fan-out wrapper** (`fanout.rs`) rather than the service — that is still
  caller-side; a second gateway, or a retry, reaches the service directly.
* **Folding this into #636** — considered and rejected as the reason this slice exists: it spans
  `proto` + `traits` + two store implementations + DST, which is a different review problem from a
  commit protocol, and #508's whole re-plan exists because those were mixed once already.

## Impact & compatibility

* **Wire: additive.** An old client talking to a new server sends no field and behaves as today;
  a new client talking to an old server has its field ignored, which degrades to today's
  (unenforced) behaviour rather than failing. Note that degradation honestly in the doc comment —
  a mixed-version fleet does not get the guarantee.
* **Source: breaking within the workspace**, by construction.
* **Behavioural:** a write whose deadline has passed now fails where it previously succeeded. That
  is the point, and it is why leg C exists — a live write must be untouched.
* **Docs currency** (`../wyrd/AGENTS.md:154-157`): this changes an **RPC**, so the living
  architecture documents change in the same PR.

## Open questions

1. **Instant vs deadline on the wire** — see `Design` 1. Do chooses and justifies; the maintainer
   may overrule at sign-off.
2. **What a mixed-version fleet is promised.** The additive degradation above means an old D
   server silently does not enforce. Is that acceptable for Alpha (my assumption), or should the
   client refuse to use a server that does not advertise support? The latter needs a capability
   exchange this slice does not have.
3. **Whether `chunkstore-fs` can meaningfully "queue"** — leg B's parked-write scenario is natural
   for the gRPC service and artificial for a local store. If it is not reachable there, assert A
   and C on `fs` and say so rather than fabricating a queue.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.

## Iteration 1 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 1): rebuilding for the implementation-level findings — C5 Causal adequacy — Rebuild must move or repeat the expiry check at the atomic publish point—the clock read precedes `spawn_blocking`, disk write, and rename, so blocking-pool or I/O delay can still publish after `W_write` (`crates/chunkstore-fs/src/lib.rs:160`, `crates/chunkstore-fs/src/lib.rs:203`, `crates/chunkstore-fs/src/lib.rs:263`).; T4 Contribution — Decide how the five reported batch-review blockers will be discharged—the `review-branch` runner is absent here, so that red is provisional; contribcheck is green and the 50-path merged/closed prior-art audit found only unrelated withdrawn PR #336.; T5 Judgment — Rebuild evidence with a post-check/pre-rename delay on `FsChunkStore` and a genuine backend-fault control—the current layer delays before store entry, the DST fake uses a stronger check order, and leg F only compares malformed input (`crates/chunkstore-grpc/tests/write_deadline.rs:297`, `crates/dst/tests/network.rs:131`, `crates/chunkstore-grpc/tests/write_deadline.rs:465`).; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_. 1 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 5 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- Full previous attempt preserved in `iteration-v1/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 2 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 2): rebuilding for the implementation-level findings — C5 Causal adequacy — Close the interval between the last clock read and actual publication — the check precedes the potentially blocking rename, so clock skew/resolution margin cannot bound I/O and `W_write` is not yet an end-to-end effect bound (`crates/chunkstore-fs/src/lib.rs:291`, `crates/chunkstore-fs/src/lib.rs:299`).; T4 Contribution — Determine whether the gate-reported seven blocking review findings are valid or settled — `scripts/review-branch` is unavailable here, so that red row is provisional; affected-path history found no deadline implementation and none of nine closed-unmerged PRs overlapped the primary files, while open PR #645 matches the declared conflict.; T5 Judgment — Add a regression that advances time across the publication syscall, not merely before it — the current two-read test calls the pre-rename instant “publication” and therefore cannot catch the runtime gap (`crates/chunkstore-fs/tests/conformance.rs:316`, `crates/chunkstore-fs/tests/conformance.rs:318`).; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 7 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_. 2 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 27 mutants tested in 30s: 2 missed, 1 caught, 24 unviable
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 7 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- Full previous attempt preserved in `iteration-v2/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 3 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 3): rebuilding for the implementation-level findings — C5 Causal adequacy — Rebuild must make the expiry verdict atomic and crash-safe with publication—the code publishes first and explicitly admits a reader-visible interval before retraction, so a crash can leave late bytes even though `WriteDeadlineExpired` promises “not applied” (`crates/chunkstore-fs/src/lib.rs:296`, `crates/chunkstore-fs/src/lib.rs:319`, `crates/traits/src/lib.rs:565`).; T4 Contribution — Determine whether the harness-reported 9 blocking review findings are valid or settled—the `scripts/review-branch` runner is absent, so that red cannot be rerun; independent merged-history and all 9 closed-unmerged-PR affected-path checks found no deadline prior art, and open PR #645 is the brief’s declared #634 conflict.; T5 Judgment — Rebuild needs seeded destructive/concurrency/crash coverage: the DST parks before entering `FsChunkStore` and therefore exercises admission refusal, while the post-publication tests prove only eventual cleanup and a sequential pre-existing duplicate (`crates/dst/tests/network.rs:586`, `crates/chunkstore-fs/tests/conformance.rs:423`, `crates/chunkstore-fs/tests/conformance.rs:485`).; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 9 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_. 2 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 9 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- Full previous attempt preserved in `iteration-v3/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 4 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 4): rebuilding for the implementation-level findings — T4 Contribution — Determine whether the gate-reported seven batch-review blockers are valid or settled—the `scripts/review-branch --bundle` runner is absent, so that red is provisional; affected-path merged and closed/rejected history found no prior server-write-deadline implementation.; T5 Judgment — Rebuild must surface cleanup failure as a backend fault and add the combined regression—an injected unlink denial left one scratch entry while the call returned `WriteDeadlineExpired` because the cleanup error is discarded (`crates/chunkstore-fs/src/lib.rs:311`).; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 7 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_. 4 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 7 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- Full previous attempt preserved in `iteration-v4/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 5 — carry-forward (from the previous attempt)
- Sign-off rationale: Auto-iterate (round 5): rebuilding for the implementation-level findings — T4 Contribution — Decide whether the eight gate-reported batch-review blockers are valid or settled — `scripts/review-branch` is absent and contribcheck lacks its `pdca.toml` context, so those results are provisional; the independent affected-path audit found no semantic duplicate in merged history or nine closed-unmerged PRs.; T5 Judgment — The evidence must distinguish three real surviving mutants — rollback state checks at `crates/chunkstore-fs/src/lib.rs:196` and `crates/chunkstore-fs/src/lib.rs:198`, plus ordinary gRPC reconstruction of `ABORTED` at `crates/chunkstore-grpc/src/client.rs:172` — or the claimed cleanup and wire-effect coverage remains incomplete.; T4 batched multi-pass rubric review (3x codex, union, triaged) FAILED (gating) — review-branch: 8 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_. 6 finding(s) needing human judgment were deferred to sign-off, not addressed here.
- Failing gate: C5 surviving mutants on the bundle diff (cargo mutants --in-diff) (advisory) — 46 mutants tested in 39s: 3 missed, 9 caught, 34 unviable
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 8 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- Full previous attempt preserved in `iteration-v5/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).

## Iteration 6 — carry-forward (from the previous attempt)
- Sign-off rationale: Rebuild for the 9 blocking T4 batch-review findings on this iteration's patch (review-batch.md), not yet addressed in review-rejected.md: - Primary: the `CREATE_RETRIES = 2` margin protecting a live write from a racing rollback's directory removal may still be insufficient under 3+ staggered concurrent expirers (crates/chunkstore-fs/src/lib.rs:311) — build-notes itself calls this "measured, not proven" and unforceable by mutation at this seam, so it needs either a stronger bound/mechanism or a convincing argument plus adversarial-scale evidence, not just a 2000-round measurement. - Coupled: the regression test meant to cover that race (crates/chunkstore-fs/tests/concurrent_put.rs:257) uses writers already expired at admission, so they're rejected by the fast admission check and never reach rollback/removal — the claimed coverage for the retry margin is illusory. Fix the test to actually drive expiry-after-admission before judging the margin resolved. - Secondary: WriteEffect::Unknown's Display/guidance text tells callers to "re-authorize" when the correct remedy is "re-read" (crates/traits/src/lib.rs:1002,1025) — wrong recovery guidance in the new typed error, should be a small fix. Do not re-treat the CREATE_RETRIES margin as settled by measurement alone this round — either close the race deterministically or land a test that genuinely forces it before claiming green.
- Failing gate: T4 batched multi-pass rubric review (3x codex, union, triaged) — review-branch: 9 blocking, 0 recorded-rejected, 0 noise-dropped -> /home/eddie/development/wyrd/wyrd-pdca/results/issue_
- Full previous attempt preserved in `iteration-v6/` (patch.diff, build-notes.md, SUMMARY.md, check-*).
- Address the above; do NOT re-attempt the rejected approach unchanged. Satisfy the brief's Success criterion (the end result).
