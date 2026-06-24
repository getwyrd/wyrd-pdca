# Brief — issue 198 / read-path-chunk-id-recheck

> The Plan artifact (docs 02 §PLAN). Human-authored. Do reads ONLY this file.
> The field labels below are parsed by the driver — keep the `- **Label:** value`
> shape. The success criterion is the load-bearing field: it is the sentence
> Check tests "did this work" against.

- **Slug:** read-path-chunk-id-recheck
- **Defect:** The read path accepts a stored fragment after verifying only its
  self-describing checksum, omitting the `header.chunk_id == chunk` recheck that the
  scrub and reconstruction paths enforce. `crates/core/src/read.rs` calls bare
  `decode(&fragment)` and accepts on `Ok` at the single-fragment / `EcScheme::None`
  site (`read.rs:138`) and at the ReedSolomon site (`read.rs:176`). The shared verify in
  `crates/core/src/repair.rs` — `fragment_intact` (`:53-54`) and `intact_shard`
  (`:64-68`) — admits a fragment only if it both decodes cleanly **and** its decoded
  header names the requested chunk; the doc comment at `repair.rs:50-51` explicitly
  claims the read path "decodes for the same effect inline." It does not. A
  misplaced-but-intact fragment (valid checksum, wrong `chunk_id` — a misrouted /
  placement-confused fragment) is therefore returned directly as the chunk payload
  (`None` scheme) or pushed as a shard at `index` into the ReedSolomon decoder → silent
  corrupt reconstruction. Scrub/repair would catch and re-enqueue this fragment; the read
  path silently trusts it.
- **Success criterion:** A read whose backing store returns a misplaced-but-intact
  fragment (correct self-describing checksum, but `header.chunk_id` ≠ the requested
  chunk) does **not** admit that fragment's bytes into the result — under `EcScheme::None`
  the read does not return the foreign payload as the chunk, and under ReedSolomon the
  foreign fragment is treated as **absent** and read around (exactly as a missing or
  checksum-failing fragment is), never fed to the decoder. Demonstrable at C4-verify by a
  flippable read-path test: a stored fragment carrying a valid header for a *different*
  chunk id is rejected. BINDING is "a wrong-`chunk_id` fragment is never admitted on the
  read path"; routing the read through the shared `core::repair` verify
  (`intact_shard` / `fragment_intact`) vs. an inline `header.chunk_id` check is
  ILLUSTRATIVE — Do's call, provided the check is the same one scrub/reconstruction use.
- **Invariant to restore:** Across **every** consumer that turns stored fragment bytes
  into a chunk's payload or an erasure-decoder shard — read, scrub, and reconstruction
  alike — a fragment is admitted only if it both decodes cleanly **and** its header's
  `chunk_id` matches the chunk it is being read under. The integrity gate is uniform; no
  consumer may admit a fragment on checksum alone. Source: proposal 0005 §174-176 and
  §262-267 (the load-bearing verify invariant), stated in-code at `repair.rs:48-51` as
  the verify "both producers share." Internal project invariant (Tier C), authoritative
  project docs. (Self-test: a one-site patch — e.g. fixing only the `None` scheme but not
  the ReedSolomon path — visibly fails this, since both read sites must enforce the
  match. Verified the read path is the *sole* non-compliant consumer: scrub uses
  `fragment_intact`, reconstruction uses `intact_shard`; so this is cause-removal at the
  one violating module, not a symptom-guard.)
- **Repo + branch target:** getwyrd/wyrd @ main   (resolve here at Plan — do not leave to Do)
- **Conflicts with:** none   (edits only `crates/core/src/read.rs`; the adjacent scrub brief shares the verify/`chunk_id` invariant but excludes the read path and edits no file this brief touches — code-disjoint, independently schedulable, no ordering constraint)
- **Surfaces:** data
- **Scope:** the read path's fragment-acceptance must not omit the `chunk_id` match that
  the shared verify enforces, at both the `EcScheme::None` and ReedSolomon sites. / out
  of scope: changing the on-disk fragment format or the shared `core::repair` verify
  itself (it is correct); the store-layer `get_fragment` corruption-error contract and
  scrub's handling of it (#207); whether the read path *enqueues* a repair obligation for
  the misplaced fragment (the existing corrupt-fragment repair-trigger plumbing is
  retained as-is — this fix is about not *admitting* the bad fragment).
- **Repro instruction:** On `main` @ `c2223a5`, in a read-path test: place a fragment file
  whose encoded header carries a valid checksum but a `chunk_id` other than the chunk
  being read (a misrouted/placement-confused fragment), at index 0 for the `None` scheme
  and at some surviving index for a ReedSolomon chunk. Issue a read for the target chunk.
  Observe that the `None`-scheme read returns the foreign payload, and the ReedSolomon
  read feeds the foreign shard into the decoder (silent corrupt reconstruction) — neither
  rejects on `chunk_id`.
- **Test file:** crates/core/tests/read_repair.rs   (misplaced-but-intact fragment is
  rejected/read-around on both schemes — red pre-fix, green post-fix)
- **Citations expected:** Do must cite path:line on the target branch for every change.
- **Prior-art check (triage cycles):** searched `crates/core/src/read.rs` across merged
  history (`6a33a33` scrub, `093732d` placement-record, `e32740f` parallel any-k read —
  none added the `chunk_id` recheck on read), open PRs (`gh pr list --state open` — none
  touch this file), and closed PRs — no prior or in-flight fix for this asymmetry.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a
draft PR MAY happen during the cycle (useful for CI feedback). The PR MUST NOT be
marked ready before sign-off accepts.
