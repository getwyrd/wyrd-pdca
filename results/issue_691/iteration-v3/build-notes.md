# Build notes — issue #691 (multipart-key-grammar), iteration 3

> Withheld from the reviewer by the driver; written for the human at sign-off.
> Target branch: `getwyrd/wyrd @ main`, base `339da46`. All `path:line` citations below are
> against the worktree `$PDCA_WORKTREE = /home/eddie/wyrd/wyrd.pdca-wt-l1` at that base,
> i.e. against the patch as it will land.

## 0. What this iteration changed, and why it is not the rejected attempt re-submitted

The carry-forward carries two rounds:

* **Iteration 1** failed C5 (`5 missed mutants`) with a named, routable defect: *"Rebuild
  must add a digit-bearing `Digest::from_hex` round trip and cover `AttemptId::as_str`."*
* **Iteration 2** passed **every gating gate** (`C4-ci` pass, `T4-batch-review` 0 blocking,
  `T4-contribution` pass, `C5` 0 survivors). Its two open items were both **NEEDS-HUMAN by
  construction**, not implementation defects: C4-verify is `UNVERIFIABLE` because this slice is
  **born-at-tier** (the brief pre-declares exactly that, `brief.md:49-62`), and T4 Contribution
  was "the reviewer could not re-run `scripts/review-branch --bundle` / `scripts/pdca
  contribcheck` from its artifacts". Neither is addressable by editing the patch.

So this iteration does not re-run the same code through the same gates. It **keeps the design**
(the brief pins the module's whole public surface at `brief.md:84-95`, and 0016 pins the key
grammar) and rebuilds against the two things the record actually shows are worth hardening —
evidence strength and the parser's fail-closed precision:

| # | Change | Why |
|---|---|---|
| 1 | `parse_retire_mode` distinguishes **"unknown mode"** from **"not a `retire:` key"** (`multipart.rs:700-726`) | v2 reported `parse_retire_mode(b"mpu:<id>")` as `UnknownRetireMode { mode: "mpu" }`, and `b"retire:bytes"` (truncated, no token) as `UnknownRetireMode { mode: "bytes" }` — i.e. *"mode `bytes` is neither `bytes` nor `records`"*. Both are now `MalformedKey { namespace: "retire:" }`. 0016 puts a *malformed key prefix* and a *third mode* on the same fail-closed side but they are different faults for the drain (`0016:434-440`): a third mode means a record whose disposal rule this build does not know (orphan-mark vs delete-only is the choice that must never be guessed); a foreign key means a neighbour's record. Naming a mode nothing ever wrote sends an operator hunting an obligation that does not exist. |
| 2 | `hex_lower` renders from a single `HEX_DIGITS` table (`multipart.rs:416-418`, `:458-465`) | v2 used `char::from_digit(..).unwrap_or('0')` — a silent fallback character on a path that defines a **digest identity**. The table has no fallback branch, so a rendering bug cannot hide behind one. |
| 3 | `is_token` via `matches!(b, b'0'..=b'9' \| b'a'..=b'f')` (`multipart.rs:173-175`) | Same predicate, one line, no `Range::contains` dance. |
| 4 | Doc: why `canonical_decimal` is **not** shared with `metadata::parse_canonical_u64` (`multipart.rs:511-522`); why the `retire:` token's part number is canonical-decimal and not fixed-width (`multipart.rs:743-750`) | The rubric says *"prefer extending a shared parser over writing a new one"* (`AGENTS.md:165-169`). The peer is **private** and `u64`-only (`metadata.rs:1310-1318`) and this child's scope pins `metadata.rs` untouched (`brief.md:107`), so the reason is now stated at the definition instead of leaving a reviewer to re-derive it. |
| 5 | Test: **three new binding legs** — the all-256-byte digest inverse, the session-range selectivity matrix, the `retire:` fault-class split (below) | Directly extends the class iteration 1's C5 finding came from: evidence that binds the *value*, not the *shape*. |
| 6 | Two intra-doc links demoted to plain code spans (`multipart.rs:375`, `:454`) | `cargo doc -p wyrd-core --no-deps` is **not** part of `cargo xtask ci` and the base already carries 15 errors of this class (`InodeRecord`→`InodeRecordWire`, `create`→`checked_for_publication`, …), but two of them were **mine**: a public `hex_lower` doc linking the private `HEX_DIGITS`, and a link to `Digest::of`, which does not exist until the next child. Neither is gated; neither should be added to the pile. After the fix `cargo doc` reports 15 — all pre-existing, none in `multipart.rs`. |

Everything else is the salvaged v2 material the brief instructs Do to take
(`brief.md:135-140`), unchanged.

## 1. The five legs, and where each lives

| Leg (brief.md:22-48) | Where |
|---|---|
| 1 round-trip + canonical rejection per keyed class | `multipart_keys.rs:296-559` — `NUMERIC_ADVERSITIES` (`:75-78`, 13 spellings incl. the bare `7` **and** the padded-but-short `007`) × `{slot:, part:, psum:, sidx:}`, plus per-class structural tables (empty id, id carrying `:`, truncated, trailing component, non-UTF-8) and the typed-error table `:360-415` |
| 2 byte order = numeric order across every width boundary | `multipart_keys.rs:571-592`, series `1, 9, 10, 99, …, 999_999` (`:566-570`) for slot **and** part |
| 3 no prefix is a prefix of another | `multipart_keys.rs:593-630` over 15 prefixes + the `mpuctl` singleton as a whole key; near-misses `:631-643`; the new per-session **range** matrix `:656-733` |
| 4 the `retire:` token grammar | `multipart_keys.rs:734-841` — both forms × both modes, the rejection table, and the fault-class split |
| 5 the pinned format constants | `multipart_keys.rs:842-876` |

Widths are the Plan-pinned ones, not re-litigated: `PART_NUMBER_WIDTH = 6`
(`multipart.rs:269`, doc-comment carries the protocol-neutrality reasoning `:255-268`),
`SLOT_INDEX_WIDTH = 6` (`multipart.rs:324`, doc `:315-323` cites `0016:1471`'s ≈524,288).

## 2. The forced self-refutation (a) / (b) / (c)

**(a) Genuine red? — YES.** Five negations, each applied to the **production** module, run
through the project's cargo, then reverted. Output below is from the *final* artifact (I re-ran
all five after the test file was rewritten for the budget, so this evidence matches what ships).
The two named in `brief.md:56-61` are (a)/(a2) and (b); (c)/(d)/(e) prove the three legs *this*
iteration adds are load-bearing too.

```
=== NEGATION (a) fixed-width parser accepts every short spelling
    [multipart.rs:505]  text.len() != width  ->  text.len() > width
part: non-canonical body "7": expected rejection for key "part:0123…cdef:7", got Ok((UploadId("0123…cdef"), PartNumber(7)))
slot: non-canonical body "7": expected rejection for key "slot:0123…cdef:7", got Ok((UploadId("0123…cdef"), SlotIndex(7)))
psum: non-canonical body "7": …  sidx: non-canonical part number "7": …
test result: FAILED. 17 passed; 4 failed

=== NEGATION (a2) fixed-width parser accepts ONLY the padded-but-short `007`
    [multipart.rs:505]  text.len() != width  ->  (text.len() != width && text.len() != 3)
part: non-canonical body "007": expected rejection for key "part:0123…cdef:007", got Ok((…, PartNumber(7)))
psum: non-canonical body "007": …   slot: non-canonical body "007": …   sidx: non-canonical part number "007": …
test result: FAILED. 17 passed; 4 failed

=== NEGATION (b) a prefix spelled without its trailing separator
    [multipart.rs:481]  MPU_PREFIX = b"mpu:"  ->  b"mpu"
no_key_prefix_is_a_prefix_of_another  panicked: scan("mpu") would return the mpuctl singleton
scan_mpu_does_not_reach_the_mpuctl_singleton  panicked: assertion failed: !MPUCTL_KEY.starts_with(MPU_PREFIX)
test result: FAILED. 19 passed; 2 failed

=== NEGATION (c) a foreign key reported as an unknown retire mode  [= the v2 behaviour]
a_retire_key_reports_an_unknown_mode_apart_from_a_foreign_key  panicked: assertion `left == right` failed
  left: UnknownRetireMode { mode: "mpu" }
 right: MalformedKey { namespace: "retire:", key: "mpu:00000000000000000000000000000000" }
test result: FAILED. 20 passed; 1 failed

=== NEGATION (d) two characters swapped in the lowercase-hex nibble table
    [multipart.rs:418]  b"0123456789abcdef"  ->  b"012345678a9bcdef"
digest_hex_is_the_exact_inverse_over_every_byte_value  panicked: hex_lower diverges from the std rendering at block 0
  left: "0001020304050607080a090b0c0d0e0f…"   right: "000102030405060708090a0b0c0d0e0f…"
serde_decode_routes_through_the_validating_constructors  panicked (the stored digest form moved)
test result: FAILED. 19 passed; 2 failed

=== NEGATION (e) the per-session retirement range widened to the whole mode prefix
    [multipart.rs:796-800]  retire_session_range -> mode.prefix().to_vec()
a_session_range_selects_exactly_its_own_class_and_session  panicked: the retire:bytes: range reached another session's retire:bytes: key
retire_key_round_trips_both_token_forms_under_both_modes   panicked: !key.starts_with(&retire_session_range(mode, &other))
the_canonical_spelling_of_every_key_is_pinned_to_the_byte  panicked: left "retire:records:"  right "retire:records:s:0123…cdef:"
test result: FAILED. 18 passed; 3 failed

=== RESTORED: test result: ok. 21 passed; 0 failed
```

Negation (a2) is the one worth reading twice: it admits **only** `007` — bare `7`, `07`,
`0007`, `00007` all still reject — so the failure message names `"007"` itself. That is exactly
the carried-forward MUST-FIX the archived #654 v2 review found missing, proved binding rather
than merely present in a list.

**(b) Production path? — YES.** `crates/core/tests/multipart_keys.rs` is an integration test:
it links `wyrd_core` and calls the shipped `wyrd_core::multipart::*` (`:25-34`). There is no
copy, mock, stand-in or re-implementation anywhere in the file — the five negations above are
the proof: each edits *production* `crates/core/src/multipart.rs` and the test notices.

**(c) Fixture includes the fault? — YES,** in the form this slice has one. There is no fleet
and no node to kill; the "fault" is a **non-canonical spelling**, and the fixture is built to
*contain* every one rather than avoid it: `NUMERIC_ADVERSITIES` is applied to all four
fixed-width parsers including the class the archived attempt curated out (`007`); the digest
alphabet leg walks all 256 byte values at two nibble positions and asserts
`accepted ⟺ lowercase hex` (`multipart_keys.rs:218-250`) rather than sampling four; the
disjointness matrix carries the pre-existing namespaces from *their* definitions
(`metadata::ORPHAN_PREFIX`, `SEG_PREFIX`, `SEGGRP_PREFIX`, `multipart_keys.rs:606-608`), not
copies, so a later prefix change in `metadata.rs` breaks this test rather than silently
diverging; and the range matrix builds a **second session's** keys and asserts each range
excludes them (`:707-712`).

## 3. Verification actually run

| What | Command (project's own runner) | Result |
|---|---|---|
| Focused suite | `cargo test -p wyrd-core --test multipart_keys` | **21 passed, 0 failed** |
| Whole gate | `./engine/xtask.sh ci` (= `cargo xtask ci` in `$PDCA_WORKTREE`) | **`xtask ci: all checks passed`, exit 0** — typos, `lint_docs`, `render_site --check` (98 pages, link audit OK), gitlink/unsafe guards, `cargo fmt --all -- --check`, `clippy --workspace --all-targets` (`-D warnings`), build, full test suite, `cargo deny check` (advisories/bans/licenses/sources ok) + both `--all-features` passes, conformance (5 valid + 6 invalid vectors), statics, deploy-guard, DST |
| Mutation (advisory C5) | `PDCA_BUNDLE=… scripts/mutants-in-diff` | **133 mutants: 50 caught, 83 unviable, 0 missed** |
| Applies to base | `git apply --check` in a throwaway worktree at `339da46` | clean; yields exactly the 3 files |
| Rustdoc (not gated) | `cargo doc -p wyrd-core --no-deps` | 15 errors, **all pre-existing on the base**, none in `multipart.rs` (see change 6) |

Every row above was re-run on the **final** tree, after the last edit (the doc-link tidy-up),
so none of this evidence is stale: `xtask ci` exit 0, mutants 0 missed, 21/21 focused.

The prose gates did **not** warn-skip on this host (`typos`, the doc renderer and `cargo-deny`
all ran), so this is closer to CI parity than INTEGRATION §3's caveat allows for — and unlike
iteration 2, `cargo deny` needed **no** `CARGO_HOME` relocation.

`cargo fmt --all` was run over every touched file before the diff was cut, so the target's own
commit hooks have nothing to reformat.

## 4. Budget — measured with the reviewer's own metric

The T2 reviewer counted iteration 2 at **1,121** "nonblank, noncomment added lines" against the
brief's 1,150 ceiling (`brief.md:109-112`). I reproduced that metric exactly (script:
non-blank, non-`//`-prefixed added lines per file) — it returns 1,121 for the archived v2 patch,
which calibrates it.

| File | semantic |
|---|---|
| `crates/core/src/lib.rs` | 1 |
| `crates/core/src/multipart.rs` | 454 |
| `crates/core/tests/multipart_keys.rs` | 675 |
| **total** | **1,130 / 1,150 (20 to spare)** |

Getting there was work, and it is the one place I traded something away: the first draft of this
iteration's test came to 861 semantic lines (total 1,316, **166 over**). I did **not** drop a
single assertion class to fit. What I cut was *restatement*:

* per-case `(&str, String)` label tuples → bare key arrays with the intent in a `//` comment
  (comments are free under the metric **and** under `rustfmt`, which was inflating each
  labelled case to 3–4 lines) — ≈ 55 lines across 6 tables;
* the four per-class "exact key bytes" assertions I had added inline → one 12-row table,
  `the_canonical_spelling_of_every_key_is_pinned_to_the_byte` (`multipart_keys.rs:296-328`),
  which also **gains** coverage (it now pins `mpuctl`, both retire spellings and every range)
  — ≈ 15 lines;
* `matches!(variant, ..) + Display` pairs → equality against the **whole** `RecordError`
  variant, which is strictly stronger (it pins the payload: namespace, token, number) — ≈ 20;
* four hand-rolled serde ser/de blocks → one `round_trips<T>` helper (`:279-295`) — ≈ 17;
* two `collect::<Vec<_>>` order series → one `windows(2)` loop over the shared series — ≈ 10;
* `letter_led_bytes()` builder → a `const [u8; 32]` literal, which is *better* evidence: the
  hex string and the bytes are now two independent literals that pin each other (`:168-184`).

If a reviewer would rather have the verbose form back, the cost is stated: it is ~120 lines of
restatement and it breaches the brief's ceiling by ~40.

## 5. Alternatives considered and rejected

* **Share `canonical_decimal` with `metadata::parse_canonical_u64`.** Rejected on scope, with
  the cost shown: the peer is `fn parse_canonical_u64` — **private**, `u64`-only
  (`metadata.rs:1310-1318`). Sharing means (i) making it `pub(crate)` **and** generic over
  `T: FromStr`, (ii) editing `crates/core/src/metadata.rs` — a **4th file**, which
  `brief.md:110-112` says means "the shape is wrong — STOP and hand back", and which
  `brief.md:107` explicitly pins as untouched. Two lines of edit, one line of `use` — but it is
  the one thing the brief forbids outright. The next child, which first *writes* one of these
  records, is where the two collapse into one helper; that is now recorded at the definition
  (`multipart.rs:515-522`) so it is not lost.
* **Add `is_mpuctl_key()` / a `parse_mpuctl_key`.** Rejected: `brief.md:84-95` enumerates the
  module's public surface and the singleton deliberately has "no id, no parser". The property
  the brief actually asks for — `scan("mpu:")` never returns it — is a statement about
  *constants* and is tested as one (`multipart_keys.rs:631-636`, and again inside the
  disjointness matrix at `:620-628`).
* **Zero-pad the part number inside the `retire:` token** for symmetry with `part:`/`psum:`.
  Rejected: it would make the token grammar diverge from `0016:358-380`, which spells it
  `s:<upload-id>:<epoch>[:<part-number>:<attempt-id>]` with no padding, and padding buys
  nothing — the only `retire:` range anything reads is the session emptiness gate
  `retire:<mode>:s:<id>:` (`0016:374-380`), which is order-free. Canonicality (the property
  that matters) is preserved by the leading-zero rule instead. Now documented at
  `multipart.rs:743-750` so the asymmetry reads as a decision rather than an oversight.
* **Co-located `#[cfg(test)]` unit tests in `multipart.rs`.** Allowed by `brief.md:117-120`
  but not taken: C4-verify's discriminator is an added `*/tests/*.rs`, the five legs must live
  in the named file, and duplicating them in-module would spend budget on restatement — the
  exact thing §4 had to cut to fit.

## 6. Items the human should look at (they are NOT defects I can close)

1. **C4-verify will report UNVERIFIABLE (exit 77), as pre-declared.** This slice is
   *born-at-tier*: on `339da46` there is no `multipart` module and no `multipart_keys` test
   target, so the "red" leg is **criterion absence** — reverting the fix makes the test fail to
   *compile*, not to *assert*. `brief.md:49-62` declares exactly this and calls it "EXPECTED and
   PRE-DECLARED, a §6 sign-off item, not a defect". The substitute the brief demands — two named
   negations with pasted failing output — is §2 above, and I ran five rather than two. Two prior
   reviewers reached the same place (`iteration-v1/check-review.md:9` C2 PASS,
   `iteration-v2/check-review.md:6` C2 PASS). **Nothing in the patch can change this**; only a
   human can accept criterion-absence as the oracle.
2. **T4 Contribution was NEEDS-HUMAN at iteration 2 because the reviewer could not re-run
   `scripts/review-branch --bundle` / `scripts/pdca contribcheck`.** That is a reviewer-sandbox
   limitation, not a property of this patch. Both gates ran green in the driver at iteration 2
   (`iteration-v2/check-gates.md`), and `review-rejected.md` (updated here with the two
   line numbers the rebuild shifted, `multipart.rs:475`/`:481`) still carries the only blocking
   finding either round produced.
3. **`review-rejected.md` is a disposition *proposal*, not a decision.** It declines the
   docs-currency finding on the ground that this slice persists nothing (no writer, no store
   call, no production consumer). Deleting a row re-blocks the gate — that is the intended
   override if you disagree.
4. **No external dependency was missing.** Everything ran: `typos`, the doc renderer,
   `cargo-deny`, `cargo-mutants`. No NEEDS-HUMAN external-dependency declaration is owed.

## 7. Scratch

Everything throwaway lived under `$PDCA_SCRATCH`
(`/var/tmp/pdca/wyrd-pdca-9c587031/issue_691/pdca-builder-691-*`): the module backup used to
apply/revert the five negations, the negation transcript, the `xtask ci` log, and a detached
`pdca-builder-691-applycheck` worktree for the apply-to-base check. The worktree was removed
(`git worktree remove --force`) and the files deleted; nothing was written to a hard-coded
`/tmp` path. `mutants.out/` and `target/` in `$PDCA_WORKTREE` are the driver's, pre-existing
and git-ignored — `git status --short` in the worktree shows exactly the three patched files.
