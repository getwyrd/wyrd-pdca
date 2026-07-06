# Build notes — issue 364 / s3-http-wire-surface (iteration 4)

_Withheld from the reviewer. Rationale, alternatives, and the pre-declared NEEDS-HUMAN
calls for the human at sign-off._

Target branch: `getwyrd/wyrd @ feat/m4-production-metadata-backend` (M4 integration base).
Built in `$PDCA_WORKTREE = /home/eddie/wyrd/wyrd.pdca-wt-l1`; all `path:line` citations are
against that worktree at tip `5d87cc4` (= `origin/feat/m4-production-metadata-backend`).

This iteration **starts from the iteration-3 patch as the working floor** (it applied cleanly
to `5d87cc4`) and then **corrects exactly the three reject items** the iteration-3 sign-off
recorded, keeping everything the earlier sign-offs said to keep (signed PUT→GET→DELETE
byte-identical, unsigned/bad-sig → 403, fail-closed auth, auth-before-body, AWS-published-vector
pinning, RustCrypto on the auth boundary, real streaming, concurrent-DELETE idempotency,
percent-decoded key identity, STREAMING-payload 501, XML error escaping). The whole gate
(`cargo xtask ci`: fmt + clippy `-D warnings` + build + test incl. DST + `cargo deny` +
conformance) is **green** on the final tree.

## Iteration-3 reject items — disposition (all three corrected)

### Reject 1 — DELETE crash-leak: the "GC backstop" claim was false. **Now made real.**

The iteration-3 `delete_object` reclaimed fragments eagerly via `reclaim_fragments` but wrote
**no** orphan-ledger grace record. The custodian GC (`crates/custodian/src/gc.rs`) only reclaims
an unreferenced fragment when it carries a deadline — an **orphan grace record**
(`orphan:` ledger) or an expired pending lease; a fragment that is unreferenced but carries
*neither* is **conservatively kept forever** (`gc.rs:reconcile`, the "No evidence the grace
window elapsed — conservatively keep it" branch, `gc.rs` ~line 157). A committed object's
fragments carry no pending lease, so a crash **between** the `unlink` metadata commit and the
eager `reclaim_fragments` stranded them permanently — GC never reclaimed them. The doc comment
that claimed GC "is the backstop" was false.

**Fix — write the orphan grace record in the *same atomic commit* that unbinds the object.**
`metadata::unlink` (`crates/core/src/metadata.rs:325`) now takes an `orphaned_at_millis` and adds
a `put(orphan_key(dserver, frag), orphaned_at)` for **every fragment the removed object placed**
to the same conditional `WriteBatch` that deletes the dirent + inode
(`crates/core/src/metadata.rs:342-378`). Because the record lands atomically with the unbind,
there is **no window**: the instant the object becomes unreferenced there is already a durable
orphan record GC honours (`crates/server/src/lib.rs:226`). On the happy path the gateway's
`reclaim_fragments` deletes the fragments **and clears the orphan records**
(`crates/server/src/lib.rs:258-289`); on a crash before that runs, GC reclaims the recorded
orphans after the reader-safe grace window — the real backstop.

**Why atomic-in-`unlink`, not "write records after `unlink` returns":** writing the records in a
*separate* commit after the unlink just moves the same crash window (crash after unlink, before
the record commit → permanent strand). The cost of the alternative is not "smaller diff" — it is
*wrong* (re-opens the exact leak). The atomic batch is the smallest change that actually restores
the invariant.

**Single source of truth for the ledger key format.** The `orphan:` key protocol
(`ORPHAN_PREFIX`, `orphan_key`, `parse_orphan_key`) moved from `custodian::gc` **into**
`core::metadata` (`crates/core/src/metadata.rs:42-79`), beside `pending_key`, because it is now a
metadata-store key protocol the delete path **writes** and GC **reads** — they must never
key-format-drift or the backstop is silently dead. `gc.rs` now `pub(crate) use`s the core
definitions (`crates/custodian/src/gc.rs:32-42`); `reconstruction.rs`/`rebalance.rs` keep calling
`crate::gc::orphan_key` unchanged (re-export). No behaviour change to GC — the DST custodian
suite (`gc_reclaims_only_true_orphans_q3`, etc.) is green.

### Reject 2 — Placement-aware delete: reclaim targeted the `index`, not the placed D-server.

`reclaim_fragments` deleted via `ChunkStore::delete_fragment(frag)` (index-routed), but the
read/write path is placement-aware (`get_fragment_at`/`put_fragment_at`). After a custodian
rebalance moves a fragment, its chunk-map `placement` points at the new D-server; an index-routed
delete reclaims the *wrong* location and leaks the bytes.

**Fix.** Added the placement-aware counterpart `PlacementChunkStore::delete_fragment_at(dserver,
id)` (`crates/traits/src/lib.rs:327-339`) — an **additive default method** mirroring the existing
`get_fragment_at`/`put_fragment_at` exactly (default delegates to `delete_fragment`, so M0–M2
single-authority stores are unchanged). `reclaim_fragments` now deletes each fragment from the
D-server the chunk map placed it on (`chunk.fragments()` → `delete_fragment_at`,
`crates/server/src/lib.rs:270-277`), and `unlink`'s orphan records are likewise keyed by the
**placed** D-server, so GC reclaims from the fleet slot the fragment actually lives on.

> **NEEDS-HUMAN (trait touch):** the brief's out-of-scope list names "any change to `traits`",
> but the iteration-3 sign-off explicitly directed "Delete from the placed D-server per ChunkRef
> (placement-aware delete counterpart)". `delete_fragment_at` is the additive, non-breaking
> counterpart that direction requires (no existing impl changes; the default preserves behaviour).
> Flagged for ratification — it is a directed correction, not an unrequested scope expansion.

### Reject 3 — C4-ci recorded red (`cargo test --workspace --exclude wyrd-dst`, exit 101).

Re-ran the **whole** gate through the project runner on the current tree:
`PDCA_WORKTREE=… ./engine/xtask.sh ci` → **"xtask ci: all checks passed"** (fmt, clippy
`-D warnings`, build, full test incl. DST custodian/network/concurrency suites, `cargo deny`,
conformance). The iteration-3 red was the stale/flake the sign-off suspected (it did not
reproduce), and the DELETE corrections above are themselves gate-green. Not leaning on the
per-fix run alone — the full gate is green.

## Test / red→green (both demonstrated, not asserted)

- **Named test file `crates/server/tests/s3_http_wire.rs`** — added
  `placement_aware_delete::delete_reclaims_from_the_placed_dserver_not_the_index`
  (`s3_http_wire.rs:566-687`). A **relocatable** mock fleet keyed by `(dserver, fragment)` whose
  bare `delete_fragment` is a deliberate no-op (a fleet cannot route a delete without the placed
  id) — so a reclaim through `delete_fragment` leaks everything; only `delete_fragment_at`
  reclaims. Drives the production `Gateway::delete_object`.
  - **Demonstrated RED:** reverting `delete_fragment_at(dserver, frag)` → `delete_fragment(frag)`
    → "DELETE must reclaim every fragment from its placed D server (no placement leak)" fails.
  - **GREEN** with the fix. The 8 prior s3-wire tests remain green.
- **`crates/custodian/tests/gc_delete_backstop.rs`** (new) —
  `delete_crash_before_reclaim_is_recovered_by_gc_from_the_placed_dserver`. Drives the real
  production `metadata::unlink` (the delete's metadata commit) and the real `reconcile_step`
  (fenced GC control point), with the eager reclaim **skipped** to model the crash; the chunk is
  placed on a **non-identity** D-server (index 0 → D-server 1), so it also proves the orphan
  record is keyed by the placed D-server.
  - **Demonstrated RED:** reverting the orphan-record write in `unlink` → GC keeps the stranded
    fragment → "no permanent leak" assertion fails (panic).
  - **GREEN** with the fix (GC reclaims via the delete's orphan grace record).

**Why the backstop test lives in the custodian crate, not the named server file:** the crash-leak
contract spans *both* the delete's metadata commit (core) and GC's reclaim (custodian). The
honest end-to-end proof must run the real GC loop, and `GcContext`/`reconcile_step`/`Custodian`
live in custodian; the server crate does not depend on custodian. Putting it there drives
production on both sides rather than a proxy assertion of "record exists". The named server file
still carries the placement-aware DELETE red→green (the fix that lands in `crates/server`).

## Carry-forward judgments to fold in (surfaced again, not silently dropped)

- **Real-SDK interop still asserted, not proven** (iteration-3 note): the wire round-trip still
  signs with the gateway's own `sigv4::sign`, and a default modern SDK PUT hits the
  `STREAMING-AWS4-HMAC-SHA256-PAYLOAD` → 501 path. This is unchanged this iteration and remains a
  pre-declared **NEEDS-HUMAN** (a genuine boto3/aws-sdk interop harness is its own slice; the
  header-signed canonicalization is unit-pinned against AWS published vectors as the independent
  oracle). Not a reject basis — surfaced per the iteration-3 direction.
- **Crate boundary** (server vs the named `gateway-s3` crate): still the decided-but-ratifiable
  call from iteration-3 (landed inside `crates/server`). Surfaced again.

## Accepted residuals (explicitly NOT re-worked, per prior sign-offs)

- **TLS:** plaintext loopback at Check is accepted; TLS wiring is deferred to #367 (human
  decision). Not touched.
- **Replay-within-15-min / UNSIGNED-PAYLOAD-on-plaintext:** pre-declared residuals tied to the
  accepted TLS deferral.
- **Orphan-record crash-window garbage:** if the happy-path cleanup commit is lost to a crash,
  GC still reclaims the *bytes* (the record is what drives it), and consumes the record when it
  does so for a fragment still on disk; a record whose fragment the eager reclaim already deleted
  can briefly linger as bounded, tiny ledger metadata. This is strictly better than the
  permanent **byte** leak it replaces, and is documented at `crates/server/src/lib.rs:266-269`.

## NEEDS-HUMAN for sign-off
1. **`delete_fragment_at` trait addition** — additive default method directed by the iteration-3
   sign-off; ratify the `traits` touch (see Reject 2).
2. **Crate-boundary ratification** — inside `crates/server` (decided) vs a `gateway-s3` split.
3. **Real-SDK interop harness** — still a NEEDS-HUMAN slice (see carry-forward).
4. **RustCrypto adoption** (`sha2`/`hmac`, ADR-0003 §2 audit recorded in iteration-3 notes;
   `cargo deny` green, no allowlist edit) — carried unchanged.
5. **TLS deferral / #367**, **sequencing (M4 vs own M4→M7)**, **error-code floor breadth** —
   carried unchanged.

## Verification summary
- `cargo test -p wyrd-server --test s3_http_wire` → 9 passed (incl. the new placement test).
- `cargo test -p wyrd-custodian --test gc_delete_backstop` → 1 passed.
- `cargo fmt --all --check` clean; `cargo clippy --workspace --all-targets` → 0 warnings.
- Full gate `./engine/xtask.sh ci` → **"xtask ci: all checks passed"**.
- `patch.diff` re-verified to `git apply --check` cleanly on a fresh checkout of base `5d87cc4`.
