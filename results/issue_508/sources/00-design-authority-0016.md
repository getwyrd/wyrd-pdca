# Design authority for this slice — proposal 0016 (READ FIRST)

The previous brief (`results/issue_508/iteration-v3/brief.md`) was **blocked**: the
publication + lifecycle half of the slice had no settled design. That design now exists.

- **Document:** `docs/design/proposals/draft/0016-multipart-commit-protocol.md`
  in the target checkout (`../wyrd`), 2302 lines, `status: draft`, tracking-issue **#626**.
- **Landed as:** `97e2392` "docs(626): draft proposal 0016 — the multipart commit protocol"
  (2026-07-24) — now the **tip of `origin/main`**.
- Read it with `git -C ../wyrd show 97e2392 --stat` and Read/Grep on the checkout.

## The split it declares (quote it in the brief, don't re-litigate it)

0016 **settles** everything underneath `CompleteMultipartUpload`, across seven decisions:

| § | Decision |
|---|---|
| `:597` | 1 — what replaces lease liveness as the publication-time proof |
| `:669` | 2 — a protection class for durable-but-unpublished bytes, per consumer |
| `:759` | 3 — lifecycle states and failure semantics |
| `:876` | 4 — bounded work for unbounded objects |
| `:1075` | 5 — reclamation evidence for failed in-flight work |
| `:1309` | 6 — the abandoned-upload reaper (designed here, **implemented in #625**) |
| `:1597` | 7 — chunk-map segmentation (the >10 GiB launch requirement) |

0016 **does not settle** — and hands to **#508, this bundle**: the S3 **wire surface**
(routing, denylist removal, the percent-encoding fence, exact status and error codes).
That half was stable across every round of adversarial review and is not in question.
The ETag **basis** is closed by ADR-0047 (lowercase-hex SHA-256; MD5 rejected); the
multipart ETag **composition** is #508's, constrained to one property: the published ETag
must be a pure function of the part records' recorded digests and their order.

## Scope decision for this re-plan (human, 2026-07-24)

Plan the **full slice** against 0016 — the wire surface *plus* Complete's publication and
lifecycle semantics as 0016 decides them. The **reaper implementation stays #625**; this
bundle implements only 0016's protocol-facing half of decision 6.

## Knobs this bundle owns

0016's *Open questions* (`:2240`) are each explicitly **non-gating for #508** and carry an
owner. #508 owns the values (inside the ranges 0016 settles) for: `MAX_MAP_CHUNKS`,
`MAX_SEG_CHUNKS`, `MAX_PART_CHUNKS`, `MAX_ROOT_SEGMENTS`, `MAX_STAGED_CHUNKS`,
`MAX_INFLIGHT_PARTS`, `chunk_size`, `R_publish`, `MAX_COMPLETE_ATTEMPTS`. No value inside a
settled range can break an invariant — but a value **outside** one can, so cite the range.
`MAX_SESSIONS` is **derived** (`⌊W_ref / U_ref⌋`), never independently chosen.

## Two traps the earlier drafts fell into

1. **Re-grounding.** Every citation in rev 3 was pinned to `cd82a29`. `origin/main` has
   moved (`97e2392`, plus the #616 xtask series). Re-verify every `path:line` against the
   current tip; the codex round-1 review failed rev 3 partly on citation drift.
2. **Superseded mechanisms.** 0016's *Open questions* records that the per-session
   `sinf:` in-flight **counter** was **removed** — the cap is now the `slot:<id>:` key
   space (decision 5), a start claiming one index under `require_absent` and an end
   deleting its own key. Rev 3 and the #626 review reports discuss the counter at length;
   treat that shape as **rejected**, retained only as rationale.
