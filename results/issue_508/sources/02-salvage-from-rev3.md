# Rev 3 (archived) — what to salvage, what to drop

The prior brief is preserved at `results/issue_508/iteration-v3/brief.md` (47 KB, iteration 3,
grounded on `cd82a29`). It was archived, not deleted, because roughly half of it survived four
rounds of adversarial review unrefuted. Rev 4 should **reuse** that half and **replace** the
other half with citations into proposal 0016.

## Salvage — the wire surface and the verification posture

- The whole **routing / error-code matrix**: exact status + S3 code per malformed form
  (`PUT /b/k?partNumber=1` with no uploadId, non-numeric partNumber, `?uploadId=U` with no
  partNumber, `PUT /b/k?uploads`, `DELETE /b/k?uploadId=U`, and the percent-encoded
  `?part%4Eumber=1`) → **400 `InvalidArgument`**; unknown upload-id → **404 `NoSuchUpload`**;
  wrong part list → **400 `InvalidPart`** / `InvalidPartOrder`; non-final part <5 MiB →
  **400 `EntityTooSmall`**; part number outside `1..=10000` → **400 `InvalidArgument`**;
  `GET /bucket?uploads` on an absent bucket → **404 `NoSuchBucket`**; malformed or oversized
  Complete body → **400 `MalformedXML`**. *Never assert merely "an error"* — the six forms the
  base already refuses **501** would otherwise be inert.
- The **percent-encoding fence**: `unsupported_subresource` matches **raw** keys while SigV4
  canonicalisation decodes-then-re-encodes, so on the base `?part%4Eumber=1` reaches the plain
  PUT arm and answers **200, overwriting the object**. That is a real red and the new routing
  must not be evadable (interacts with **#491**, which stays open and out of scope).
- The **C4 gate hazard**, which rev 3 got right and rev 4 must keep: `run-verify.sh`'s failure
  branch has **no zero-test guard** (`TESTS_RAN == 0` is only on the success branch), so a
  **compile error** in an added test prints "PASS — red without the fix" over a build that ran
  nothing. Therefore no added test may reference **anything the patch adds** — not a Rust
  symbol, not a crate newly added to a `Cargo.toml` (rev 3's concrete case: the tests must not
  use `rand`, which the slice makes a direct dependency of `crates/server`; use fixed seeds).
  Do must record, from the RED leg, how many tests actually ran and failed.
- **New test files, not appended legs**: `run-verify.sh` discriminates on an *added*
  `crates/<c>/tests/<t>.rs` (`_is_test_file`, `:90-93`) and degrades to a green-only,
  proves-nothing branch for a co-located test (`:397`).
- The **test-shape discriminators**: parts submitted **out of order**; at least one non-final
  part that is **not a whole multiple of the chunk size** (e.g. 5 MiB + 7 B), so an assembler
  assuming chunk alignment fails; ETags asserted against independently computed known answers;
  `ListParts` pagination with `IsTruncated` genuinely computed.
- The **harness reuse notes**: `sdk_client` (`crates/server/tests/s3_gateway_cluster.rs:96-110`)
  and `seed_bucket` (`crates/server/tests/s3_list_objects.rs:78-88`) — a `bucket:` marker must
  be seeded as raw bytes *before* the store moves into `Gateway::new`, or every listing leg
  404s, since nothing on `main` creates bucket records (**#511**).
- The **prior-art result**: net-new. `crates/gateway-s3/src/lib.rs` was last touched by #509
  (PR #612), #510 (PR #611) and #507 (PR #609) — all merged, none implements a multipart verb,
  all three deliberately preserve the denylist (the comment at `:1652-1653` names 508).
- **Retained draft:** `results/issue_508/s3_multipart_upload.rs` (44 KB, iteration-2 draft) is
  kept at the bundle root on purpose — it has working `run_gc_pass` / `run_restore_pass`
  helpers written against base-visible symbols. Left in place by this archive; keep the
  pointer if rev 4 still wants it, or say explicitly that it is superseded.

## Drop — anything rev 3 decided that 0016 now decides

Rev 3 tried to specify the publication proof, the staged-part protection class, the terminal
states, and the reclamation evidence **inside the brief**. That is what got it blocked: the
codex review found the session-less-part leak still not mechanically decidable, and the
adversary found a terminal-state deadlock reachable with no crash (a concurrent PUT to the same
key makes Complete's publish CAS lose, stranding the session in `Completing` with no verb able
to leave it), undisposed published part records, and `pending:` residue deployed GC never
reclaims. **Do not re-derive these.** Cite 0016's decisions 1–5 and 7, and 0016's
protocol-facing half of decision 6, and let the brief say which behaviour the tests pin.

Rev 3's **scope split** also changes: it deferred "the abandoned-upload reaper" to a follow-up
bundle *to be named*. That follow-up is now **#625**, whose design is 0016 decision 6.

## Re-ground everything

Rev 3's citations are pinned to `cd82a29`; `origin/main` is now `97e2392`. The round-1 codex
review failed rev 3 partly on citation drift (`flow.py:637` → `:461`, a misread of
`gc.rs:157-187`, and a claim contradicting `gc.rs:183-186`). Re-verify each `path:line`.
