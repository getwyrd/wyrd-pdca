# Build notes — issue 364 / s3-http-wire-surface (iteration 6)

Target branch: `getwyrd/wyrd @ feat/m4-production-metadata-backend` (worktree base
`5d87cc4`, the current M4 integration tip — advanced from the brief-time `225d3bd` as
slices #427/#428 merged). All edits made in `$PDCA_WORKTREE`
(`/home/eddie/wyrd/wyrd.pdca-wt-l1`); `path:line` citations are against that tree.

This iteration keeps the accepted v5 floor (RustCrypto sha2/hmac provenance; crate boundary
committed to `crates/server`; AWS-KAT-pinned SigV4; the auth-before-body, streaming, and
DELETE-crash-leak fixes) and corrects **only** the three iter-5 BLOCKING items plus the two
minor fail-closed erosions. I started from `iteration-v5/patch.diff` (applied cleanly onto
`5d87cc4`), then made the changes below.

## The three BLOCKING items

### (1) PUT overwrite leaked the prior object's fragments — and (2) GET-during-DELETE truncation
These were fixed with **one** mechanism, because they are the same defect on two verbs: a
request-path operation was reclaiming superseded fragment bytes in a way that either leaked
them (overwrite: didn't reclaim at all) or reclaimed them *too eagerly* (delete: deleted them
out from under a concurrent reader). The reader-safe design in proposal 0005 (`0005:288-295`)
already answers both: **orphan the superseded fragments in the same atomic commit, and let the
custodian GC reclaim them after the reader-safe grace window** — never on the request path.

- **Overwrite** (`crates/core/src/metadata.rs:459` new `commit_chunk_map_superseding`, called
  by `crates/core/src/write.rs:271` `commit_overwrite`, reached from
  `crates/server/src/lib.rs:187` `commit_written`, `:193` the overwrite call): the CAS that swaps the new chunk map onto
  the inode now writes an orphan grace record for every fragment of the **prior** chunk map in
  the *same batch*. `commit_chunk_map` (used by reconstruction/backfill, which *keep* the
  fragments) is deliberately left non-orphaning — only a content overwrite supersedes bytes.
- **Delete** (`crates/server/src/lib.rs:235` `delete_object`): the eager `reclaim_fragments`
  call and the whole `reclaim_fragments` method are **removed**. `metadata::unlink` already
  wrote the orphan grace records atomically (`crates/core/src/metadata.rs:369`, the batch loop
  at `:434`); GC now is the
  sole reclaimer, so a GET that resolved the chunk map before the DELETE reads its fragments
  intact within the grace window.

GC needed no change: its reference set protects any fragment a *committed* chunk map still
places (`crates/custodian/src/gc.rs:124,200`), so the current object's fragments are never
reclaimed even while the superseded ones are — proven by
`overwrite_orphans_prior_fragments_reclaimed_by_gc_but_keeps_the_current`.

**Why not keep eager reclaim + add a version-hold on GET (the alternative for item 2)?** A
version-hold means every streaming GET registers a hold and every DELETE/overwrite consults
the hold set — new shared state on the hot read path and a new release/expiry lifecycle
(≈ a hold registry + 2 touch-points per verb, and a reader that dies without releasing must
still time out via *some* window). The grace window is that window, already designed and
already implemented in GC; removing eager reclaim is a **net deletion** on the request path
(the `reclaim_fragments` method, ~20 lines, plus the `delete_fragment_at` trait method it was
the only production user of — `crates/traits/src/lib.rs`). Per the brief's "Invariant to
restore" framing, the target is the smallest change that restores the reader-safe invariant,
which is *removing* the request-path reclaim, not adding a second subsystem to guard it.

Trade-off (documented, not hidden): reclaim of deleted/overwritten bytes now depends on the
custodian GC loop running. That loop exists and is fenced-control-point-driven
(`reconcile_step`) but is not yet a deployed process (0005 Option A, a later slice) — the same
posture the brief pins for the coordination prerequisite (#367). Until GC is deployed, a
delete/overwrite defers byte reclaim rather than doing it inline. This is the accepted 0005
design and exactly what iter-5 directed ("Make DELETE honour the grace window"); it is *not* a
leak (the orphan record is durable and GC reclaims deterministically after grace).

### (3) Real-SDK interop was asserted, not demonstrated
Added the **real Rust `aws-sdk-s3`** as a dev-dependency of `crates/server` and a new test
module `real_sdk_interop` (`crates/server/tests/s3_http_wire.rs`) that drives
`put_object → get_object → delete_object` against the loopback listener with the SDK's *own*
signer, canonicalizer, and `aws-chunked` framer — nothing in that path touches the gateway's
`sigv4`/`streaming` code. A stock SDK object round-trips **byte-identical** (9000-byte object,
so it spans several chunks over whatever wire form the SDK emits), and a client signing with a
credential the gateway never issued is refused `InvalidAccessKeyId` (fail-closed). This is the
genuine independent oracle the reviewers asked for across iterations 2–5; the self-signed wire
tests are kept as fast coverage, no longer the sole oracle.

**Dependency-wall handling (this is a NEEDS-HUMAN — see below).** `aws-sdk-s3`'s default HTTPS
client pulls the *legacy* `hyper-rustls 0.24 → rustls 0.21 → rustls-webpki 0.101.7`, which
carries `RUSTSEC-2026-0104` (a CRL-parsing panic) — `cargo deny check advisories` fails on it.
Rather than ignore an advisory on the auth-adjacent TLS stack (which would itself be a
governance decision), I removed the vulnerable crate from the graph entirely: the test uses a
**plaintext** hyper client (`aws-smithy-http-client::Builder::new().build_http()`) over
loopback, so no TLS stack is pulled at all. Configured deps:
`aws-sdk-s3` (default-features off; `rt-tokio,http-1x`), `aws-smithy-http-client`
(`default-client`), `aws-smithy-runtime-api`, `aws-smithy-types`, `aws-credential-types`
(`crates/server/Cargo.toml`). After that, **full `cargo deny check` is green** (advisories +
bans + licenses + sources) — every new transitive license is already on the ADR-0003 allowlist
(all Apache-2.0/MIT/BSD/Unicode-3.0). Plaintext-loopback matches the brief's accepted TLS
posture (public TLS deferred to #367); a live boto3/aws-cli leg stays a pre-declared DEFERRED
backstop.

## The two minor fail-closed erosions (iter-5)
`crates/server/src/s3/sigv4.rs` `verify`:
- **Whitespace (`:403` in v5):** header values now go through a `trim_all` helper
  (`sigv4.rs`) that strips leading/trailing whitespace *and* collapses internal whitespace runs
  to a single space (the SigV4 "Trimall" rule; quoted sections preserved), so a client that
  signed doubled internal spaces is not spuriously 403'd. Unit-anchored by
  `trim_all_folds_internal_whitespace_but_keeps_quoted`.
- **SignedHeaders order (`:384-385,405` in v5):** the SignedHeaders *string* in the
  string-to-sign is now the client's own list verbatim (`declared.join(";")`), not a re-sorted
  copy; the canonical header *block* is still sorted by name (a canonical-request requirement).
  A client whose SignedHeaders is not lexically sorted verifies against what it actually signed.

Both are validated end-to-end by the real-SDK round-trip (a live SDK's exact header set /
canonicalization now verifies).

## Red → green (demonstrated, then reverted)
The net-new module's compile-error red is acceptable per the brief, but I demonstrated the two
behavioural fixes are load-bearing:
- **GET-during-DELETE:** temporarily reinstated v5's eager reclaim in `delete_object` →
  `get_streaming_resolved_before_delete_is_not_truncated` FAILS
  (`InsufficientFragments { chunk_id: 6, have: 0, need: 6 }` — the truncation) → reverted → green.
- **Overwrite leak:** temporarily gated off the orphan records in
  `commit_chunk_map_superseding` → `overwrite_orphans_prior_fragments_reclaimed_by_gc_but_keeps_the_current`
  FAILS (`GC … left: Satisfied, right: Changed` — the fragment leaks) → reverted → green.

## Gate
`./engine/xtask.sh ci` (== `cargo xtask ci`: fmt, clippy `-D warnings`, build, whole-suite test
incl. the DST tier and the aws-sdk-s3 interop, `cargo deny`, conformance vectors) → **all checks
passed** in `$PDCA_WORKTREE`. The historically-flaky `gateway_lease_expiry.rs` ran green within
the same pass.

## NEEDS-HUMAN (for §6 at sign-off)
Standing calls carried from the brief (not re-litigated here): SigV4 scope / minimal S3
error-code floor; M4 sequencing (own branch vs. M4 integration); public-TLS deferral to #367
(and the rustls-provider deny.toml/license decision when TLS is wired). New this iteration:
- **New dev-dependency tree (`aws-sdk-s3` + ~aws-smithy/aws-runtime crates).** `cargo deny` is
  green and no denied license/advisory is introduced (I avoided the `rustls-webpki` advisory by
  going plaintext-only), but adding a large vendor SDK tree — even dev-only — is the ADR-0003
  three-test "new dependency" governance call (INTEGRATION.md §4), so the maintainer should
  ratify it explicitly. If undesired, the interop leg can fall back to `aws-sigv4` alone (a much
  smaller independent signer) at the cost of not exercising the SDK's full wire framing.

## Key citations
- In-process seam wrapped: `crates/server/src/lib.rs:123` (`put_object`), `:211` (`get_object`),
  `:283` (`get_object_streaming`), `:235` (`delete_object`); overwrite path `:187` `commit_written`.
- Reclaim discipline: `crates/core/src/metadata.rs:369` (`unlink` orphans), `:459`
  (`commit_chunk_map_superseding`); `crates/core/src/write.rs:271` (`commit_overwrite`);
  `crates/custodian/src/gc.rs:96,136,200` (grace-window reclaim, safety gate).
- Spec: blueprint:59, 698-699 (S3 front door, byte-identical round-trip); 620-623 (two TLS
  identities); `07-deployment-view.md:72` (HTTP/S3, SigV4); `14-threat-model.md:86` (fail-closed
  external auth); 0005:288-295 (reader-safe grace window + GC); 0015:789 (OOM cliff / streaming).
</content>
</invoke>
