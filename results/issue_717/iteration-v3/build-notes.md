# Build notes — #717 (iteration 3), multipart staging + retirement records

Base: `getwyrd/wyrd @ main` = **`c824243`** ("Merge pull request #725 …/716-multipart-session-lifecycle-records"),
i.e. the brief's execution precondition (#715 and #716 merged) is satisfied — `origin/main`
*is* `c824243`, and `crates/core/src/multipart.rs` on it already carries `AdmissionRecord`,
`SessionRecord`, `SlotRecord` and `PartRecord` (2,154 lines). All `path:line` citations below
and in the patch are against **base + this patch** (the tree the reviewer's worktree holds);
see "Citation convention" at the end — that convention was itself inconsistent in iteration 2
and is repaired here.

Everything is built in `$PDCA_WORKTREE` = `/home/eddie/wyrd/wyrd.pdca-wt`.

---

## 1. What this iteration changes, and why

This is a **delta on iteration 2** (preserved in `iteration-v2/`), not a rebuild: iteration 2's
patch was re-applied to the base and then amended. The carry-forward named exactly three items;
all three are done, and nothing else in the design was re-opened.

### (a) The blocking review finding — `Records { segments }` was not checked against its token's epoch

`review-batch.md` (1 blocking, from the T4 batched review of iteration 2):

> `crates/core/src/multipart.rs:2949` **BUG**: A `Records { segments: Some(...) }` payload is
> accepted without checking that the segment group's epoch matches the session token epoch, so a
> misfiled retirement obligation can delete another completion attempt's segment generation.

The finding is real and it is the *same class* as every other leg of this child (a stored
record's fields disagreeing with the key that names them), so it belongs inside
`checked_against_token` — where the human's sign-off rationale put it — not in a new slice.

**What the design actually pins** (I read the proposal rather than guessing):

* a `seg:<group-nonce>:<epoch>:<index>` record's `<epoch>` **is** the `Completing` fence epoch of
  the attempt that wrote it, "so the segment-group is **per-attempt** — a rolled-back attempt's
  stale segments live in a key range disjoint from any later attempt's (the F18 fix)"
  (`docs/design/proposals/draft/0016-multipart-commit-protocol.md:354`);
* the obligation that retires them is installed **by the fence that ends that attempt**, in that
  fence's own batch: `require(mpu == Completing@E)` → `put session→Open` / `→Aborting@E+1` **plus**
  `put retire:records:{seg:<g>:<E>}` (`0016:663`, `:664`, `:665`; the same shape at `:880` for the
  restore door, and `:2358` — "naming **exactly epoch `E`'s** segment keys");
* the whole point of that epoch-scoping is the X40 / F18 refutation: a stale obligation must not be
  able to delete a *later* attempt's segments, because a later attempt may have won its root flip
  and had its segments **adopted by the published inode** (`0016:2568`, `:2486`, `:2834`).

So the rule the record can enforce is: **`segments.epoch` is the token's epoch or one less** —
the two epochs the installing batch holds (the one it read, `E`, and the one it minted, `E+1`).
Implemented at `crates/core/src/multipart.rs:3020-3032` (`checked_against_token`, `:2972`), typed as
`RecordError::RetireSegmentEpochMismatch { key_epoch, segment_epoch }`
(`crates/core/src/multipart.rs:376-395`, `Display` at `:607-616`), documented on the payload
field (`:2859-2860`), on the check (`:2965-2971`) and on the entry point's list of key relations
(`:3081-3083`).

**Why not strict equality** (the obvious reading of the finding). 0016 does not spell *which* of
the two epochs the token carries — `put retire:records:{seg:<g>:E}` names the payload, never the
key, and the three installing rows all read `Completing@E` while writing `@E+1` (`0016:663-665`).
The token grammar's own gloss is "the epoch whose fence installed the obligation"
(`0016:360-366`), which reads either way, and the first writers are #656–#659. Equality would
therefore be a **guess**, and if the writers spell the minted epoch every rollback obligation in
production becomes undecodable — a drain that can never clear it, a session whose terminal-delete
gate (`0016:673`) never opens, a permanent leak. The `{E, E+1}` window is the widest rule
derivable from the design and still refuses **both** misfilings that matter:

| payload epoch vs token epoch | verdict | why |
|---|---|---|
| `E_s == E_t` | accept | the fence's read epoch |
| `E_s == E_t − 1` | accept | the epoch that fence minted |
| `E_s > E_t` (any) | **reject** | names a **later** attempt — the X40 data-loss direction: that generation may already be published and adopted |
| `E_s < E_t − 1` | **reject** | names a stale generation no fence at `E_t` ever ended; the misfiling the finding describes |

The window is stated in the code as a dependency on the writers' fence convention, so #657's
author meets it at the boundary rather than discovering it in production
(`crates/core/src/multipart.rs:381-383`). `checked_sub` is used rather than `E_s + 1 == E_t`
so the comparison is total at `u64::MAX` and the underflow *is* the dangerous direction
(`crates/core/src/multipart.rs:3016-3019` for the comment, `:3026` for the test itself).

Rejected alternatives, with their cost:

* **Cross-check the group *nonce* too.** Not possible at this boundary and not cheap-but-declined:
  the nonce is minted with the session and deliberately **not** derived from the upload id
  (`0016:354`, `:509`), so the `retire:` key — which names only `<upload-id>:<epoch>` — carries
  nothing to compare it against. Doing it would mean reading the `mpu:` session record inside a
  decode, i.e. a store round trip in a pure function: a new `async` decode surface and a store
  handle threaded through every caller (`decode_retire_obligation` today is 21 lines,
  `crates/core/src/multipart.rs:3088-3108`, and has no store parameter anywhere in the module).
  That is a different slice, and it is #656–#659's.
* **Enforce it in `checked_shape` instead.** `checked_shape` is the *value-only* rule set, public
  so a writer can check its own payload before storing it (`crates/core/src/multipart.rs:2914`);
  the epoch relation needs the key, so putting it there would either be impossible or force the
  key into the writer-side API. One extra parameter on a public method versus four lines in the
  key-taking check: the key-taking check wins.

### (b) `renew_pending`'s mechanism, described wrongly in three doc comments

`renew_pending` puts `encode(entry)` — the entry **its caller handed it** — under a precondition
on the raw bytes it read (`crates/core/src/metadata.rs:2093`). Iteration 2's `PendingEntry` doc
closed with "so what a renewal stores is what was read", which is false: the renewal deliberately
stores a *new* lease expiry. The load-bearing property is the narrower one — the encoder's output
for a both-absent entry is byte-identical to what every pre-multipart build wrote, so a renewal
rewrites **only** the lease. Corrected at `crates/core/src/metadata.rs:1573-1585`, and the two
echoes in the test at `crates/core/tests/multipart_staging_retire.rs:25-32` and `:288-290`.
The **assertions** did not change: leg 2 was always asserting the right thing
(`:263-283`), only the prose around it was wrong.

### (c) `decode_owned_entry` now returns the key identity it parsed

`Result<(PartNumber, ChunkId, OwnedEntry), RecordError>`
(`crates/core/src/multipart.rs:2663-2666`, `:2689`), mirroring `decode_retire_obligation`'s
`(token, payload)` (`:3088-3091`). The `<part-number>` is what attributes staged residue to the
part attempt (`0016:353`) and the chunk id is half of the `orphan:<dserver>:<chunk>:<index>` keys
the entry's `staged` placement completes, so a reaper walking `sidx:<upload-id>:` would otherwise
parse the key a second time — a second decision site that can disagree with the first. The upload
id is deliberately **not** returned: it is `entry.owner()`, which the decode has just proved equal
to the key's. The test asserts the returned identity re-mints its own key
(`crates/core/tests/multipart_staging_retire.rs:176-180`, `:230-239`), which is what makes the
new return value load-bearing rather than decorative.

### Not attempted, per the carry-forward's explicit instruction

* `PendingEntryWire`'s `deny_unknown_fields` blast radius (silent field-drop vs. whole-sweep
  failure on an unrecognised field) — deferred to the follow-up tracker issue named in
  iteration 2's SUMMARY §10; a proper fix needs warn-and-continue in live readers (`gc.rs`,
  `write.rs`) that the brief places out of scope.
* Scope/size overrun (T1 / C3) — explicitly set aside for this round by human direction. For the
  record the patch is **2,052 added lines raw / 38 deleted / 12 files**, of which ~1,000 are doc comments; it
  grew by ~90 lines this round (the new error variant + its check + the new test + doc fixes).

---

## 2. The demonstrated red — 14 isolating negations

C4-verify's RED leg is criterion-absence (born-at-tier): with production reverted the test does
not compile, so it lands **UNVERIFIABLE (77)** exactly as the brief pre-declares. What replaces
it is the brief's demonstration list. The brief binds **eleven**; this round has **fourteen** —
the eleven, plus the second `Generation` guard held apart (see below), plus the scope arm of
leg 1d, plus the new leg 1r.

Method (script preserved at `$PDCA_SCRATCH/pdca-builder-717-negations/negate.py`, logs beside it):
drop exactly one production check, run `cargo test -p wyrd-core --test multipart_staging_retire`,
record, restore the file byte-for-byte. **Every one killed exactly one test — 26 passed, 1
failed, every time** (three needed an extra `let _ = …` binding because the crate builds with
`-D warnings` and an unused variable is a hard error; the check itself is still the only thing
removed).

| # | leg | check removed (`crates/core/src/…`) | test that went red | failure |
|---|---|---|---|---|
| 1 | **1b** owner vs key | `multipart.rs:2681-2686` the `owner != key_owner` arm | `owned_entry_owner_must_agree_with_its_key` | `expected OwnedEntryOwnerMismatch, got Ok((PartNumber(4), 77, OwnedEntry { owner: UploadId("1a1a…"), … }))` |
| 2 | **1d** generation identity | `multipart.rs:2986-2993` the `(inode, version)` comparison | `retire_payload_scope_must_agree_with_its_token` | `expected RetireGenerationIdentityMismatch, got Ok((Generation { inode: 42, version: 6 }, Generation { inode: 42, version: 5, … }))` |
| 3 | **1d** scope (generation payload under `s:`) | `multipart.rs:2997-2999` | `retire_payload_scope_must_agree_with_its_token` | `expected RetireTokenScopeMismatch, got Ok((Session { … epoch: 3, part: None }, Generation { … }))` |
| 4 | **1h** session under part token | `multipart.rs:3009` | `whole_session_obligations_are_rejected_under_a_per_part_token` | `expected RetireTokenSuffixMismatch for session, got Ok((Session { … part: Some((PartNumber(4), AttemptId("3c3c…"))) }, Session))` |
| 5 | **1h** part obligation under session token | `multipart.rs:3013` | `per_part_obligation_is_rejected_under_a_session_wide_token` | `expected RetireTokenSuffixMismatch, got Ok((Session { … part: None }, Chunks { … }))` |
| 6 | **1h** records under part token | `multipart.rs:3011` | `records_obligation_is_rejected_under_a_per_part_token` | `expected RetireTokenSuffixMismatch, got Ok((Session { … part: Some(…) }, Records { parts: Some(PartNumberSet([(3, 3)])), segments: None }))` |
| 7 | **1i** staged `EcScheme` | `multipart.rs:2518-2523` `checked_staged_scheme`'s body | `staged_placement_scheme_must_be_supported` | `assertion failed: metadata::decode::<StagedPlacement>(unsupported.as_bytes()).is_err()` |
| 8 | **1e** torn `PendingEntry` | `multipart.rs:2445-2447` `checked_ownership_pairing`'s test | `torn_pending_entry_is_rejected_under_both_readings` | `the pending: reading must refuse the torn shape {"lease_expiry_millis":9000,"owner":"1a1a…"}` |
| 9 | **1n** generation naming **both** sources | `multipart.rs:2928-2932` | `generation_obligation_names_exactly_one_source` | `assertion failed: matches!(…, Err(RecordError::RetireGenerationSourcesConflict { chunks: 1 }))` |
| 10 | **1n** generation naming **neither** | `multipart.rs:2933-2935` | `generation_obligation_names_exactly_one_source` | `assertion failed: matches!(…, Err(RecordError::RetireObligationOwesNothing { payload: "generation" }))` |
| 11 | **1p** nested `ChunkRef` geometry | `multipart.rs:2159-2168` `checked_chunk_scheme`'s body | `retire_payload_nested_chunk_scheme_must_be_supported` | `assertion failed: matches!(…, Err(RecordError::ChunkSchemeUnsupported { k: 0, m: 1, .. }))` |
| 12 | **1r** records segment epoch *(new)* | `multipart.rs:3026` the `checked_sub` window | `a_records_obligation_names_its_own_attempts_segment_generation` | `assertion failed: matches!(under(EPOCH + 2), Err(RecordError::RetireSegmentEpochMismatch { … }) if key_epoch == EPOCH + 2 && segment_epoch == EPOCH)` |
| 13 | **leg 2** byte identity | `metadata.rs:1606` one `skip_serializing_if` | `legacy_pending_entry_re_encodes_byte_identically` | `left: "{\"lease_expiry_millis\":9000,\"owner\":null}"` / `right: "{\"lease_expiry_millis\":9000}"` |
| 14 | **leg 3 corollary**, negated the other way — *added* a placement-length rule | `multipart.rs:2496-2499` `StagedPlacement::new` | `owned_entry_with_length_mismatched_placement_decodes` | `a length-mismatched placement is not a decode error: StagedSchemeUnsupported { k: 2, m: 1 }` |

Notes the brief asks for explicitly:

* **1n's two halves are two guards, so they get two negations** (rows 9 and 10). The brief allows
  them to ride one negation "if one guard covers both"; here `RetireGenerationSourcesConflict` and
  `RetireObligationOwesNothing` are distinct arms of `checked_shape`
  (`crates/core/src/multipart.rs:2928-2935`), and dropping either leaves the other's case
  accepted, so both were demonstrated separately.
* Row 12 is the leg this iteration adds; it is negated the same way as the rest and its test also
  pins the two **accepted** spellings (`EPOCH`, `EPOCH + 1`), so the window cannot silently widen
  to "anything ≤".

---

## 3. Forced self-refutation (the three questions)

**(a) Genuine red?** **Yes**, fourteen times over — §2 is exactly "revert the fix, re-run, watch it
go red", one check at a time, with the failure text pasted. The whole-file red is stronger still:
on pristine `c824243` with only the test file added, the crate does not compile (the four types,
the two decoders and the two `PendingEntry` fields do not exist) — the pre-declared born-at-tier
UNVERIFIABLE. No leg stayed green under its own negation.

**(b) Production path?** **Yes.** Every witness goes through the production codec and the
production entry points: `metadata::encode` / `metadata::decode`
(`crates/core/src/metadata.rs:1643-1647`, `:1649-1652`), `wyrd_core::multipart::decode_owned_entry`
and `::decode_retire_obligation`, and the production key builders `sidx_key` / `retire_key`
(`crates/core/src/multipart.rs:1128`, `:1292`). There is no stand-in, no re-implementation and no
hand-built key anywhere in the test: leg 1b's "wrong key" is `sidx_key(&upload_id(OTHER_HEX), …)`,
minted by the same function a writer would call. The `PendingEntry` legs drive the **shared live
record** (`crates/core/src/metadata.rs:1599-1613`) — the type on the existing `pending:` write and
renewal path — not a copy of it.

**(c) Fixture includes the fault?** **Yes.** Each negation's witness is the *failing* value, not a
curated healthy one: a payload owned by session `1a1a…` under session `2b2b…`'s key; a generation
payload under a session token and vice versa; `rs(0,1)` geometry; a `PendingEntry` with `owner` and
no `staged` (and the mirror); a `Records{segments:{epoch:3}}` under `s:…:5` and under `s:…:2`. The
positive controls sit in the same tests (the same bytes under the honest key decode), so each leg
turns on the key/value relation rather than on the value alone.

---

## 4. Verification run in this iteration

Runner: the project's `cargo xtask` toolchain via `$PDCA_WORKTREE` (`./engine/xtask.sh` delegates
`cargo xtask ci` there; xtask exposes no single-test subcommand, so the focused leg is the exact
command the brief's Falsifiability names).

| what | command | result |
|---|---|---|
| focused test (GREEN leg) | `cargo test -p wyrd-core --test multipart_staging_retire` | **27 passed, 0 failed** |
| whole crate | `cargo test -p wyrd-core` | 19 suites, **0 failures** |
| ripple crates (sample) | `cargo test -p wyrd-custodian --test gc`, `-p wyrd-metadata-redb --test conformance` | 10 + 6 passed |
| whole workspace compiles | `cargo test --workspace --all-features --no-run` | builds every test target |
| formatter (commit hook) | `cargo fmt --all -- --check` | clean |
| lints | `cargo clippy --workspace --all-targets --all-features` | clean (`-D warnings` is on) |
| docs gate (leg 4) | `python3 docs/publishing/tools/lint_docs.py`; `render_site.py --out … --check` | `lint_docs: OK`; `98 page(s)`, `link audit OK` |
| spell gate | `typos` over the four substantive files | clean |

`cargo xtask ci` end-to-end was **not** run here — iteration 2 recorded six server/custodian tests
stalling past five minutes, and Check re-runs the gate anyway; every leg of it that this patch can
plausibly break (fmt, clippy, the crate tests, the docs render, typos) was run individually above.

---

## 5. Scope / files (unchanged from the brief's 12)

`crates/core/src/multipart.rs` (+970), `crates/core/src/metadata.rs` (+84/-6, the `PendingEntry`
hunk + the in-file test constructor at `:3501`), the new
`crates/core/tests/multipart_staging_retire.rs` (+987), the one docs paragraph
(`docs/design/architecture/05-building-block-view.md:204`, extended this round to name the
attempt-scope check), and the 8 mechanical `owner: None, staged: None` ripple files. No custodian
**source**, no ADR/proposal/spec, no `Cargo.toml`/`Cargo.lock`.

## Citation convention (repaired this round)

Iteration 2 mixed two conventions for `metadata.rs:NNNN` — some numbers were base-relative
(`:1794`, `:1919`, `:1562-1566`), others post-patch (`:2085`, `:2090`), so the same statement was
cited under two different numbers in one patch. Everything is now **post-patch** (the tree the
reviewer's worktree and every future reader hold), and each anchor was re-derived from the file
rather than shifted arithmetically:

* `require(key, encode(prior))`, the `inode:` shape → `:1875`, `:2000`
* `renew_pending`'s raw-bytes CAS + re-encoded put → `:2093` (its read-back at `:2088`)
* `live_lease_guards`' pins → `:2124`, `:2128`
* `metadata::encode` → `:1643-1647`; `decode_segment_record` → `:2617-2628`

Five inherited citations were simply **wrong in this tree** (off by ~27 lines — authored against
an older base) and are corrected: `SegmentNonce`'s doc `:714-733`→`:741-763`, its C-1 sentence
`:724`→`:751-752`, `seg_key`'s totality property `:1219-1233`→`:1251-1258`, `parse_seg_key`'s
canonical epoch `:1296-1300`→`:1324-1328`, and the private `parse_canonical_u64`
`:1310-1318`→`:1336-1346`. Cross-file citations were re-checked at the same time and all hold
(`crates/core/src/erasure.rs:120` = `pub fn supported`, `crates/custodian/src/gc.rs:482-496` = the
expired-lease `pending:` scan, `AGENTS.md:146-149` / `:170-172`, `crates/traits/src/lib.rs:272-286`).

## Scratch

Everything throwaway lived under `$PDCA_SCRATCH/pdca-builder-717-*` — `-negations` (the negation
script and its 14 logs, from which §2's failure column is quoted verbatim), `-v2cmp` (iteration-2's
sources reconstructed from `iteration-v2/patch.diff`, used to diff this round's delta),
`-applycheck` (a pristine base export the patch was `git apply --check`ed against) and
`-docsrender` (the rendered site). **All four are removed**; nothing of mine survives outside the
worktree and this bundle. To re-run a negation, the recipe is one exact-string edit per row of
§2's table plus `cargo test -p wyrd-core --test multipart_staging_retire`.
